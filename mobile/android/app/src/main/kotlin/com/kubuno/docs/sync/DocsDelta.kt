package com.kubuno.docs.sync

import android.content.Context
import android.util.Log
import com.kubuno.android.account.SharedAccount
import com.kubuno.docs.net.DocChangeKind
import com.kubuno.docs.net.DocsClients
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * The etag of every document the account owns, kept up to date from
 * `GET /documents/delta`.
 *
 * WHY THIS EXISTS: `PATCH /documents/:id` is only guarded against a concurrent
 * write when it sends the current etag as `If-Match`, and `GET /documents/:id`
 * does NOT return one — the module injects etags into the create/update
 * responses and into the delta stream, nowhere else. Without this cache the
 * first save of every freshly opened document would go out unguarded and could
 * silently overwrite what another device wrote in the meantime.
 *
 * WHY DELTA AND NOT THE LISTING: `GET /documents` carries no etag at all, its
 * `total` is the row count of the page being returned rather than a grand total,
 * and it only ever returns rows that still exist — a cache fed from it could
 * never learn that a document was deleted. The delta stream is ordered by a
 * monotonic `change_seq`, carries explicit `kind=deleted` tombstones, and
 * returns a resume cursor, so every pull after the first transfers only what
 * actually moved.
 *
 * KNOWN LIMIT: the module filters the stream on `owner_id`, so a document merely
 * SHARED with the user never appears here; saving one keeps falling back to a
 * write without `If-Match`.
 */
@Singleton
class DocsDelta @Inject constructor(
    @ApplicationContext private val appContext: Context,
    private val clients: DocsClients,
) {

    /**
     * Same leniency as the network layer: a server that gains a field must not
     * make an already-written cache file unreadable.
     */
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        prettyPrint = false
    }

    /** The cursor is not a secret and is a single number: plain preferences fit. */
    private val prefs = appContext.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    /** Memory tier, keyed by account: document id → etag. */
    private val memory = ConcurrentHashMap<String, Map<String, String>>()

    /** One pull at a time: two concurrent drains would interleave their writes. */
    private val mutex = Mutex()

    /**
     * The etag last published for [id], or null when the cache has never seen
     * that document (never synced, not owned by this account, or created since
     * the last pull).
     *
     * Reads the cache file on the first call for an account, so call it off the
     * main thread.
     */
    fun etagOf(account: SharedAccount, id: String): String? = snapshot(account)[id]

    /**
     * Drains the delta stream for [account] into the cache.
     *
     * Returns null when the stream was drained (or stopped at the page cap), and
     * the failure otherwise: the pages applied before it are already persisted
     * with their cursor, so a later call resumes instead of starting over.
     */
    suspend fun pull(account: SharedAccount): Throwable? = mutex.withLock {
        withContext(Dispatchers.IO) {
            try {
                drain(account)
                null
            } catch (cancelled: CancellationException) {
                // Cancellation is the caller going away, not a sync failure:
                // swallowing it here would break structured concurrency.
                throw cancelled
            } catch (t: Throwable) {
                t
            }
        }
    }

    private suspend fun drain(account: SharedAccount) {
        val api = clients.api(account)
        val etags = LinkedHashMap(snapshot(account))
        var cursor = cursor(account)
        var pages = 0

        while (true) {
            val page = api.delta(cursor, PAGE_LIMIT)
            pages++

            for (change in page.changes) {
                when (change.kind) {
                    // A trashed document keeps its etag: it can be restored and
                    // written again without passing through this stream twice.
                    DocChangeKind.MODIFIED, DocChangeKind.TRASHED ->
                        change.etag?.takeIf { it.isNotEmpty() }?.let { etags[change.uuid] = it }
                    DocChangeKind.DELETED -> etags.remove(change.uuid)
                    // Unknown kinds are skipped rather than treated as a removal:
                    // a newer module must not be able to empty an older client's cache.
                    else -> Unit
                }
            }

            val advanced = page.cursor > cursor
            val next = if (advanced) page.cursor else cursor

            // Persist page by page, etags BEFORE the cursor. A crash between the
            // two replays a page that was already applied — harmless, every entry
            // is an idempotent put/remove — whereas the reverse order would skip
            // it for good and leave a permanent hole in the cache.
            persist(account, etags)
            writeCursor(account, next)
            cursor = next

            if (!page.hasMore) break
            // Defensive: `has_more` with a cursor that did not move would spin
            // forever. Stop and let the next pull retry instead.
            if (!advanced) break
            if (pages >= MAX_PAGES) break
        }
    }

    // ── Storage ──────────────────────────────────────────────────────────────

    private fun snapshot(account: SharedAccount): Map<String, String> =
        memory.getOrPut(key(account)) { readEtags(account) }

    private fun readEtags(account: SharedAccount): Map<String, String> {
        val file = etagFile(account)
        if (!file.exists()) return emptyMap()
        return runCatching {
            json.decodeFromString(DocsEtagFile.serializer(), file.readText()).etags
        }.onFailure {
            // An unreadable cache costs an unguarded first save, not a crash; the
            // next pull rewrites the file.
            Log.w(TAG, "etag cache unreadable, starting from an empty one", it)
        }.getOrDefault(emptyMap())
    }

    private fun persist(account: SharedAccount, etags: Map<String, String>) {
        val copy: Map<String, String> = LinkedHashMap(etags)
        memory[key(account)] = copy
        val file = etagFile(account)
        runCatching {
            file.parentFile?.mkdirs()
            // Write-then-rename: a half-written file truncated by a kill would be
            // unparseable and would silently drop every cached etag.
            val tmp = File(file.parentFile, file.name + ".tmp")
            tmp.writeText(json.encodeToString(DocsEtagFile.serializer(), DocsEtagFile(copy)))
            file.delete()
            tmp.renameTo(file)
        }.onFailure {
            // The memory tier still serves this process; only the next launch
            // pays for it, by re-pulling from the stored cursor.
            Log.w(TAG, "etag cache write failed", it)
        }
    }

    /** The resume point stored for [account]; 0 means "never synced". */
    private fun cursor(account: SharedAccount): Long = prefs.getLong(cursorKey(account), 0L)

    private fun writeCursor(account: SharedAccount, cursor: Long) {
        prefs.edit().putLong(cursorKey(account), cursor).apply()
    }

    private fun etagFile(account: SharedAccount): File =
        File(File(File(appContext.filesDir, DIR), key(account)), ETAGS_FILE)

    private fun cursorKey(account: SharedAccount): String = "cursor:" + key(account)

    /**
     * A per-account, filesystem-safe folder name. The system account name is an
     * address-like string, so it is sanitised, and its hash is appended because
     * sanitising alone could map two accounts onto the same folder.
     */
    private fun key(account: SharedAccount): String {
        val name = account.systemName
        return safe(name) + "-" + name.hashCode().toUInt().toString(16)
    }

    private fun safe(raw: String): String = raw.map { c ->
        if (c.isLetterOrDigit() || c == '.' || c == '_' || c == '-') c else '_'
    }.joinToString("")

    private companion object {
        /** The module clamps `limit` to 1..500 and defaults to 200. */
        const val PAGE_LIMIT = 200

        /** Ceiling on one drain, so a first sync of a huge account cannot run unbounded. */
        const val MAX_PAGES = 200

        const val PREFS_FILE = "kubuno-docs-sync"
        const val DIR = "docs-delta"
        const val ETAGS_FILE = "etags.json"
        const val TAG = "DocsSync"
    }
}

/** On-disk shape of the cache — a wrapper so the file can gain fields later. */
@Serializable
private data class DocsEtagFile(
    val etags: Map<String, String> = emptyMap(),
)
