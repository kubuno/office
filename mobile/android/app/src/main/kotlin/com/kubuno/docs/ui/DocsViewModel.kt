package com.kubuno.docs.ui

import android.content.Context
import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.kubuno.android.account.SharedAccount
import com.kubuno.android.account.SharedAccounts
import com.kubuno.docs.net.ApiErrorDto
import com.kubuno.docs.net.CreateDocumentBody
import com.kubuno.docs.net.DocErrorCode
import com.kubuno.docs.net.DocsApi
import com.kubuno.docs.net.DocsClients
import com.kubuno.docs.net.DocumentDto
import com.kubuno.docs.net.DocumentSummaryDto
import com.kubuno.docs.net.EditorPresenceDto
import com.kubuno.docs.net.TemplateDto
import com.kubuno.docs.net.UpdateDocumentBody
import com.kubuno.docs.pm.PmDoc
import com.kubuno.docs.pm.PmParser
import com.kubuno.docs.sync.DocsDelta
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import java.io.IOException
import java.util.UUID
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import retrofit2.HttpException

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

/** The three destinations of the bottom bar. */
enum class DocsTab { RECENTS, BROWSE, TEMPLATES }

/** What the BROWSE listing is showing. Each value maps to one server branch. */
enum class DocsFilter { ALL, STARRED, SHARED, TRASH }

/** Client-side ordering of the listing; the server always answers newest-first. */
enum class DocsSort { RECENT, TITLE, CREATED }

/** The two binary formats the module can export a document to. */
enum class DocsExportFormat(val id: String, val extension: String, val mime: String) {
    DOCX(
        "docx",
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    ODT("odt", "odt", "application/vnd.oasis.opendocument.text"),
}

/**
 * A failure, as the UI should treat it: [code] is the server's stable error code
 * (or one of the two local codes below) and is what screens branch on; [message]
 * is short French prose meant only for display.
 */
data class DocsError(val code: String, val message: String) {
    companion object {
        /** No response at all: airplane mode, dead Wi-Fi, unreachable instance. */
        const val NETWORK = "NETWORK"

        /** No Kubuno account on the device: nothing can be loaded until one exists. */
        const val NO_ACCOUNT = "NO_ACCOUNT"
    }
}

/**
 * The document currently open, with everything needed to render AND to write it
 * back: the row, the parsed body, the raw body (what a save actually sends) and
 * the etags.
 *
 * [etag] is what a save sends as `If-Match`. `GET /documents/:id` does not carry
 * one (only create/update/delta inject it), so on open it comes from the delta
 * etag cache; every write then replaces it with the etag that write returned. It
 * stays null only when the cache has never seen the document — a document shared
 * by someone else, or one created since the last pull — and the save then goes
 * out unguarded.
 */
data class OpenDocument(
    val document: DocumentDto,
    val doc: PmDoc,
    val contentJson: JsonElement?,
    val etag: String? = null,
    val contentEtag: String? = null,
    /** Other people in the editing session, as reported by `editing/join`. */
    val editors: List<EditorPresenceDto> = emptyList(),
)

/**
 * A rejected save (412): the local body and the server's current one, so the UI
 * can offer a choice instead of silently losing one of the two.
 */
data class DocsConflict(
    val documentId: String,
    val localDoc: PmDoc,
    val localContent: JsonElement?,
    val serverDocument: DocumentDto,
    val serverDoc: PmDoc,
    val serverContent: JsonElement?,
    /**
     * Etag of the server version, from the delta cache — the conflict re-read
     * itself carries none. Adopting the server body with this etag is what lets
     * the NEXT save be guarded again; null leaves it unguarded.
     */
    val serverEtag: String? = null,
)

/** A downloaded export ready to hand to the system share/open sheet (one-shot). */
data class ExportReady(val file: File, val mime: String, val fileName: String)

/**
 * Everything the docs screens render.
 *
 * One immutable object for the whole app, exactly as in the photos app: screens
 * read fields, never call back into a second source of truth. One-shot effects
 * ([message], [exportReady]) are nullable fields the UI consumes and then clears
 * through the matching `xConsumed()` method — there is no separate event channel.
 */
data class DocsUiState(
    /** False until the account probe finished; screens show a spinner meanwhile. */
    val ready: Boolean = false,
    val account: SharedAccount? = null,
    val accounts: List<SharedAccount> = emptyList(),

    val tab: DocsTab = DocsTab.RECENTS,
    val filter: DocsFilter = DocsFilter.ALL,
    val sortOrder: DocsSort = DocsSort.RECENT,

    val loading: Boolean = false,
    val loadingMore: Boolean = false,
    /** True while the last page came back full, i.e. another page may exist. */
    val canLoadMore: Boolean = false,
    val error: DocsError? = null,

    /** The BROWSE listing (root level, or the active filter/search). */
    val documents: List<DocumentSummaryDto> = emptyList(),
    /** The RECENTS listing; capped at 20 rows server-side, so it never pages. */
    val recents: List<DocumentSummaryDto> = emptyList(),
    val templates: List<TemplateDto> = emptyList(),
    val templatesLoading: Boolean = false,
    /**
     * Failure of the LAST template load, kept apart from [error] so the Templates
     * tab can tell "this instance has no template" from "the catalogue could not
     * be read". Cleared by the next [DocsViewModel.loadTemplates].
     */
    val templatesError: DocsError? = null,

    /** True once the search field is open; [query] may still be empty. */
    val searchActive: Boolean = false,
    val query: String = "",
    val searching: Boolean = false,

    val opening: Boolean = false,
    val openDoc: OpenDocument? = null,
    /** True when an editing session is joined and the body is writable. */
    val editing: Boolean = false,
    /** True when local edits have not been saved yet. */
    val dirty: Boolean = false,
    val saving: Boolean = false,
    val saveError: DocsError? = null,
    val conflict: DocsConflict? = null,

    /** Ids ticked in the listing; selection mode is on whenever this is non-empty. */
    val selection: Set<String> = emptySet(),
    /** True while a create/rename/trash/duplicate round-trip is in flight. */
    val busy: Boolean = false,

    val exporting: Boolean = false,
    val exportReady: ExportReady? = null,

    /** One-shot user message (snackbar), cleared through [DocsViewModel.messageConsumed]. */
    val message: String? = null,
) {
    val hasAccount: Boolean get() = account != null
    val selectionMode: Boolean get() = selection.isNotEmpty()
}

// ─────────────────────────────────────────────────────────────────────────────
// ViewModel
// ─────────────────────────────────────────────────────────────────────────────

@HiltViewModel
class DocsViewModel @Inject constructor(
    @ApplicationContext private val appContext: Context,
    sharedAccounts: SharedAccounts,
    private val clients: DocsClients,
    private val delta: DocsDelta,
) : ViewModel() {

    /** The Kubuno accounts on the device. Docs owns none: it is a consumer app. */
    private val accounts: List<SharedAccount> = sharedAccounts.list()

    private val _state = MutableStateFlow(DocsUiState(accounts = accounts))
    val state: StateFlow<DocsUiState> = _state.asStateFlow()

    private val api: DocsApi? get() = _state.value.account?.let(clients::api)

    /** Only used to read the module's error envelope out of a failed response. */
    private val json = Json { ignoreUnknownKeys = true }

    private var searchJob: Job? = null
    private var pingJob: Job? = null

    /**
     * `Idempotency-Key` of the save of the CURRENT body, minted when that save
     * starts and dropped as soon as the server acknowledges it or the body
     * changes again (see [applyEdit]).
     *
     * The module answers a known key with the response it stored and writes
     * NOTHING, so a key that two different edits can share silently discards the
     * second one — which is why this is neither a counter (reset on every open)
     * nor a clock, but an id generated per write intent.
     */
    private var saveIntentKey: String? = null

    /**
     * Same, for a creation: [CreateIntent.fingerprint] is what the user asked
     * for, so pressing "create" again after a lost reply replays that creation
     * instead of producing a twin, while a creation asked for after an
     * acknowledged one always gets a key of its own.
     */
    private var createIntent: CreateIntent? = null

    private data class CreateIntent(val fingerprint: String, val key: String)

    init {
        viewModelScope.launch {
            // Several apps can register a system account for the same identity,
            // but only the one still holding the refresh token can mint access
            // tokens — the others 401 on every call. So take the first account
            // that actually yields a token rather than the first listed.
            val account = pickUsableAccount()
            _state.value = _state.value.copy(
                account = account,
                ready = true,
                error = if (account == null) {
                    DocsError(DocsError.NO_ACCOUNT, "Aucun compte Kubuno sur cet appareil.")
                } else {
                    null
                },
            )
            if (account != null) refresh()
        }
    }

    /** The first shared account that can currently borrow an access token. */
    private suspend fun pickUsableAccount(): SharedAccount? {
        if (accounts.size <= 1) return accounts.firstOrNull()
        val usable = withContext(Dispatchers.IO) {
            accounts.firstOrNull { account ->
                runCatching { clients.raw(account).bearer.validAccessToken() != null }
                    .getOrDefault(false)
            }
        }
        return usable ?: accounts.firstOrNull()
    }

    /** Switches instance; every cached listing belongs to the previous one. */
    fun selectAccount(account: SharedAccount) {
        if (account.systemName == _state.value.account?.systemName) return
        saveIntentKey = null
        createIntent = null
        _state.value = _state.value.copy(
            account = account,
            documents = emptyList(),
            recents = emptyList(),
            templates = emptyList(),
            templatesError = null,
            selection = emptySet(),
            openDoc = null,
            editing = false,
            dirty = false,
            conflict = null,
            error = null,
        )
        refresh()
    }

    // ── Listings ─────────────────────────────────────────────────────────────

    /** Reloads the listing of the current tab, plus the recents strip. */
    fun refresh() {
        val api = api ?: return
        _state.value = _state.value.copy(loading = true, error = null)
        viewModelScope.launch {
            val listing = runCatching {
                withContext(Dispatchers.IO) { fetchPage(api, offset = 0) }
            }
            val recents = runCatching {
                withContext(Dispatchers.IO) {
                    api.listDocuments(recent = true, limit = RECENTS_LIMIT).documents
                }
            }.getOrDefault(_state.value.recents)

            listing.onSuccess { page ->
                _state.value = _state.value.copy(
                    loading = false,
                    documents = sorted(page, _state.value.sortOrder),
                    recents = recents,
                    canLoadMore = page.size >= PAGE_SIZE,
                    error = null,
                )
            }.onFailure { e ->
                _state.value = _state.value.copy(
                    loading = false,
                    recents = recents,
                    error = errorOf(e),
                )
            }
            if (_state.value.tab == DocsTab.TEMPLATES && _state.value.templates.isEmpty()) {
                loadTemplates()
            }
        }
    }

    /** Appends the next page of the BROWSE listing. No-op when nothing is left. */
    fun loadMore() {
        val api = api ?: return
        val current = _state.value
        if (current.loading || current.loadingMore || !current.canLoadMore) return
        _state.value = current.copy(loadingMore = true)
        viewModelScope.launch {
            val page = runCatching {
                withContext(Dispatchers.IO) { fetchPage(api, offset = current.documents.size) }
            }.getOrNull()
            val merged = if (page == null) {
                current.documents
            } else {
                // The server can repeat a row when something moved between the
                // two pages; de-duplicating on id keeps the list keys unique.
                val seen = current.documents.map { it.id }.toSet()
                current.documents + page.filterNot { it.id in seen }
            }
            _state.value = _state.value.copy(
                loadingMore = false,
                documents = sorted(merged, _state.value.sortOrder),
                canLoadMore = page != null && page.size >= PAGE_SIZE,
            )
        }
    }

    /**
     * One page of the listing for the active filter and search.
     *
     * `total` in the response is this page's row count, never a grand total, so
     * paging is decided by comparing the page size to [PAGE_SIZE] and nothing else.
     */
    private suspend fun fetchPage(api: DocsApi, offset: Int): List<DocumentSummaryDto> {
        val s = _state.value
        val q = s.query.trim().ifEmpty { null }
        val filter = s.filter
        return api.listDocuments(
            search = q,
            starred = if (q == null && filter == DocsFilter.STARRED) true else null,
            shared = if (q == null && filter == DocsFilter.SHARED) true else null,
            trashed = filter == DocsFilter.TRASH,
            limit = PAGE_SIZE,
            offset = offset,
        ).documents
    }

    fun selectTab(tab: DocsTab) {
        if (tab == _state.value.tab) return
        _state.value = _state.value.copy(tab = tab, selection = emptySet())
        when (tab) {
            DocsTab.TEMPLATES -> if (_state.value.templates.isEmpty()) loadTemplates()
            DocsTab.BROWSE -> if (_state.value.documents.isEmpty()) refresh()
            DocsTab.RECENTS -> if (_state.value.recents.isEmpty()) refresh()
        }
    }

    fun setFilter(filter: DocsFilter) {
        if (filter == _state.value.filter) return
        _state.value = _state.value.copy(filter = filter, selection = emptySet(), documents = emptyList())
        refresh()
    }

    fun setSortOrder(sort: DocsSort) {
        if (sort == _state.value.sortOrder) return
        _state.value = _state.value.copy(
            sortOrder = sort,
            documents = sorted(_state.value.documents, sort),
            recents = sorted(_state.value.recents, sort),
        )
    }

    private fun sorted(rows: List<DocumentSummaryDto>, sort: DocsSort): List<DocumentSummaryDto> =
        when (sort) {
            // Timestamps are RFC 3339 with a fixed layout, so lexicographic order
            // is chronological order and no date parsing is needed here.
            DocsSort.RECENT -> rows.sortedByDescending { it.updatedAt.orEmpty() }
            DocsSort.CREATED -> rows.sortedByDescending { it.createdAt.orEmpty() }
            DocsSort.TITLE -> rows.sortedBy { it.title.lowercase() }
        }

    /**
     * Loads the template catalogue. Its failure lands in
     * [DocsUiState.templatesError], not in the shared [DocsUiState.error]: a 401
     * or a 500 here used to leave the Templates tab showing an empty catalogue,
     * which reads as "this instance ships none".
     */
    fun loadTemplates() {
        val api = api ?: return
        _state.value = _state.value.copy(templatesLoading = true, templatesError = null)
        viewModelScope.launch {
            val templates = runCatching {
                withContext(Dispatchers.IO) { api.listTemplates().templates }
            }
            _state.value = _state.value.copy(
                templatesLoading = false,
                templates = templates.getOrDefault(_state.value.templates),
                templatesError = templates.exceptionOrNull()?.let(::errorOf),
            )
        }
    }

    // ── Search ───────────────────────────────────────────────────────────────

    fun openSearch() {
        _state.value = _state.value.copy(searchActive = true)
    }

    /** Debounced title search; an empty query falls back to the plain listing. */
    fun search(query: String) {
        _state.value = _state.value.copy(query = query, searchActive = true, searching = true)
        searchJob?.cancel()
        searchJob = viewModelScope.launch {
            delay(SEARCH_DEBOUNCE_MS)
            val api = api ?: return@launch
            val page = runCatching { withContext(Dispatchers.IO) { fetchPage(api, offset = 0) } }
            _state.value = _state.value.copy(
                searching = false,
                documents = sorted(page.getOrDefault(emptyList()), _state.value.sortOrder),
                canLoadMore = page.getOrNull()?.let { it.size >= PAGE_SIZE } ?: false,
            )
        }
    }

    fun clearSearch() {
        searchJob?.cancel()
        _state.value = _state.value.copy(searchActive = false, query = "", searching = false)
        refresh()
    }

    // ── Selection ────────────────────────────────────────────────────────────

    fun toggleSelect(id: String) {
        val cur = _state.value.selection
        _state.value = _state.value.copy(selection = if (id in cur) cur - id else cur + id)
    }

    fun clearSelection() {
        _state.value = _state.value.copy(selection = emptySet())
    }

    /** Trashes every selected document, dropping the rows as they go. */
    fun trashSelection() {
        val ids = _state.value.selection.toList()
        if (ids.isEmpty()) return
        _state.value = _state.value.copy(selection = emptySet())
        ids.forEach { trash(it) }
    }

    // ── Open / close ─────────────────────────────────────────────────────────

    /** Loads a document and its body. Read-only until [startEditing]. */
    fun openDocument(id: String) {
        val account = _state.value.account ?: return
        val api = clients.api(account)
        saveIntentKey = null
        _state.value = _state.value.copy(
            opening = true,
            error = null,
            saveError = null,
            conflict = null,
            dirty = false,
            editing = false,
        )
        viewModelScope.launch {
            // Started before the read so both round-trips overlap: the etag this
            // document will save against does not come from the read (see
            // [freshEtag]).
            val etagAhead = async { freshEtag(account, id) }
            val result = runCatching { withContext(Dispatchers.IO) { api.getDocument(id) } }
            val knownEtag = etagAhead.await()
            result.onSuccess { response ->
                _state.value = _state.value.copy(
                    opening = false,
                    openDoc = OpenDocument(
                        document = response.document,
                        doc = PmParser.parseDocumentOrEmpty(response.contentJson),
                        contentJson = response.contentJson,
                        etag = response.document.etag ?: knownEtag,
                        contentEtag = response.document.contentEtag,
                    ),
                )
            }.onFailure { e ->
                _state.value = _state.value.copy(opening = false, error = errorOf(e))
            }
        }
    }

    /**
     * The etag the server last published for [id], refreshing the delta cache
     * first.
     *
     * `GET /documents/:id` answers `{document, content_json}` straight from the
     * row and never injects an etag — the module does that only in the
     * create/update responses and in the delta stream — so this is the only way
     * to hold a real `If-Match` for a document that this session has not written
     * yet. Null when the pull failed or the document never appeared in the
     * stream (the module filters it on `owner_id`, so a document shared by
     * someone else never does); the save then goes out unguarded, i.e. last
     * writer wins.
     */
    private suspend fun freshEtag(account: SharedAccount, id: String): String? {
        delta.pull(account)?.let {
            Log.w(TAG, "delta pull failed; a save of $id may go out without If-Match", it)
        }
        return withContext(Dispatchers.IO) { delta.etagOf(account, id) }
    }

    /** Closes the open document, leaving the editing session if one was joined. */
    fun closeDocument() {
        val id = _state.value.openDoc?.document?.id
        val wasEditing = _state.value.editing
        stopPing()
        _state.value = _state.value.copy(
            openDoc = null,
            editing = false,
            dirty = false,
            saving = false,
            saveError = null,
            conflict = null,
        )
        if (wasEditing && id != null) {
            val api = api ?: return
            // `leave` saves the draft and drops the session; failing to reach it
            // only leaves a stale session that the server expires on its own.
            viewModelScope.launch {
                runCatching { withContext(Dispatchers.IO) { api.leaveEditing(id) } }
            }
        }
    }

    // ── Editing ──────────────────────────────────────────────────────────────

    /**
     * Joins the editing session. The body to edit is the DRAFT's, which `join`
     * creates when missing — not the one `GET /documents/:id` returned — so the
     * open document's content is replaced with what join hands back.
     */
    fun startEditing() {
        val api = api ?: return
        val open = _state.value.openDoc ?: return
        if (_state.value.editing) return
        viewModelScope.launch {
            val joined = runCatching {
                withContext(Dispatchers.IO) { api.joinEditing(open.document.id) }
            }
            joined.onSuccess { response ->
                val current = _state.value.openDoc ?: return@onSuccess
                val content = response.contentJson ?: current.contentJson
                _state.value = _state.value.copy(
                    editing = true,
                    dirty = false,
                    saveError = null,
                    openDoc = current.copy(
                        doc = PmParser.parseDocumentOrEmpty(content),
                        contentJson = content,
                        editors = response.editors,
                    ),
                )
                startPing(open.document.id)
            }.onFailure { e ->
                _state.value = _state.value.copy(error = errorOf(e))
            }
        }
    }

    /** Leaves the editing session and returns the document to read-only. */
    fun stopEditing() {
        val api = api ?: return
        val id = _state.value.openDoc?.document?.id ?: return
        stopPing()
        _state.value = _state.value.copy(editing = false)
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { api.leaveEditing(id) } }
        }
    }

    /** A session goes stale after two minutes without a ping. */
    private fun startPing(id: String) {
        stopPing()
        pingJob = viewModelScope.launch {
            while (isActive) {
                delay(PING_INTERVAL_MS)
                runCatching { withContext(Dispatchers.IO) { api?.pingEditing(id) } }
            }
        }
    }

    private fun stopPing() {
        pingJob?.cancel()
        pingJob = null
    }

    /**
     * Replaces the whole body with ProseMirror JSON the caller built. The body
     * is re-parsed so the renderer sees the change immediately; nothing is sent
     * until [save].
     */
    fun applyEdit(content: JsonElement) {
        val open = _state.value.openDoc ?: return
        // A new body is a new write intent: keeping the pending key would make
        // the module replay the previous save's response and write nothing.
        saveIntentKey = null
        _state.value = _state.value.copy(
            dirty = true,
            saveError = null,
            openDoc = open.copy(
                contentJson = content,
                doc = PmParser.parseDocumentOrEmpty(content),
            ),
        )
    }

    // ── Save ─────────────────────────────────────────────────────────────────

    /**
     * Writes the body back.
     *
     * `If-Match` carries the etag of the version this edit started from, so a
     * write that someone else raced becomes a 412 the user can arbitrate instead
     * of a silent overwrite.
     *
     * `Idempotency-Key` identifies the write INTENT (see [saveIntentKey]): it is
     * minted for the body being saved and kept across retries of that same save,
     * so a reply lost on the way back costs a replay and not a second write,
     * while the next, different save always carries a key of its own.
     */
    fun save() {
        val api = api ?: return
        val open = _state.value.openDoc ?: return
        val content = open.contentJson ?: return
        if (_state.value.saving) return
        val id = open.document.id
        val etag = open.etag
        val key = saveIntentKey ?: newIntentKey("save").also { saveIntentKey = it }
        val wasEditing = _state.value.editing
        _state.value = _state.value.copy(saving = true, saveError = null)
        viewModelScope.launch {
            val result = runCatching {
                withContext(Dispatchers.IO) {
                    api.updateDocument(
                        id = id,
                        body = UpdateDocumentBody(contentJson = content),
                        ifMatch = etag,
                        idempotencyKey = key,
                    )
                }
            }
            result.onSuccess { response ->
                // A joined session writes to the draft file; promoting it is what
                // makes the edit visible to everyone else, so its failure is a
                // failed save — not a detail to swallow.
                val promotion = if (wasEditing) {
                    runCatching { withContext(Dispatchers.IO) { api.saveEditing(id) } }
                        .exceptionOrNull()
                        ?.let { errorOf(it) }
                        ?.let { DocsError(it.code, "Brouillon non publié : " + it.message) }
                } else {
                    null
                }
                // The body itself landed, so its new etag is adopted either way:
                // dropping it would make the retry send a stale `If-Match` and
                // turn it into a conflict the user never caused.
                if (promotion == null) saveIntentKey = null
                val current = _state.value.openDoc
                _state.value = _state.value.copy(
                    saving = false,
                    dirty = promotion != null,
                    saveError = promotion,
                    conflict = null,
                    openDoc = current?.copy(
                        document = response.document,
                        // Keep the local body: the response echoes what was just
                        // sent, and re-parsing it would only rebuild the model.
                        etag = response.document.etag ?: current.etag,
                        contentEtag = response.document.contentEtag ?: current.contentEtag,
                    ),
                    message = if (promotion == null) "Document enregistré" else null,
                )
                patchRow(id) { it.copy(updatedAt = response.document.updatedAt) }
            }.onFailure { e ->
                val error = errorOf(e)
                if (error.code == DocErrorCode.PRECONDITION_FAILED) {
                    enterConflict(id, error)
                } else {
                    _state.value = _state.value.copy(saving = false, saveError = error)
                }
            }
        }
    }

    /**
     * A fresh `Idempotency-Key`.
     *
     * Random, because the module scopes stored responses to (user, key) alone:
     * two write intents that share a key make the second one return the first
     * one's response WITHOUT writing anything. A clock- or counter-derived key
     * collides exactly that way, and a key that changes under a retry writes
     * twice instead.
     */
    private fun newIntentKey(prefix: String): String = prefix + ":" + UUID.randomUUID()

    /** Fetches the server's current version so the UI can show both sides. */
    private suspend fun enterConflict(id: String, error: DocsError) {
        val account = _state.value.account ?: return
        val api = clients.api(account)
        val open = _state.value.openDoc
        val server = runCatching { withContext(Dispatchers.IO) { api.getDocument(id) } }.getOrNull()
        if (server == null || open == null) {
            _state.value = _state.value.copy(saving = false, saveError = error)
            return
        }
        _state.value = _state.value.copy(
            saving = false,
            saveError = error,
            conflict = DocsConflict(
                documentId = id,
                localDoc = open.doc,
                localContent = open.contentJson,
                serverDocument = server.document,
                serverDoc = PmParser.parseDocumentOrEmpty(server.contentJson),
                serverContent = server.contentJson,
                // The re-read carries no etag; the write that caused this 412 is
                // in the delta stream, so its etag is what the cache now holds.
                serverEtag = freshEtag(account, id),
            ),
        )
    }

    /**
     * Settles a 412.
     *
     * `keepLocal = true` re-sends the local body with no `If-Match` under a new
     * key, which is the only way to win the race deliberately: the etag we hold
     * is provably stale. `keepLocal = false` adopts the server's body, the local
     * edits are dropped, and the next save is guarded by the server etag the
     * conflict carries.
     */
    fun resolveConflict(keepLocal: Boolean) {
        val conflict = _state.value.conflict ?: return
        val open = _state.value.openDoc ?: return
        if (!keepLocal) {
            saveIntentKey = null
            _state.value = _state.value.copy(
                conflict = null,
                saveError = null,
                dirty = false,
                openDoc = open.copy(
                    document = conflict.serverDocument,
                    doc = conflict.serverDoc,
                    contentJson = conflict.serverContent,
                    etag = conflict.serverEtag,
                    contentEtag = conflict.serverDocument.contentEtag,
                ),
            )
            return
        }
        // Deliberately overwriting is a different write from the one the server
        // rejected, so it gets its own key.
        saveIntentKey = null
        _state.value = _state.value.copy(
            conflict = null,
            saveError = null,
            openDoc = open.copy(etag = null),
        )
        save()
    }

    // ── Document mutations ───────────────────────────────────────────────────

    /**
     * Creates a document, optionally from a template, and opens it.
     *
     * The creation keeps one `Idempotency-Key` until the server acknowledges it,
     * so asking again for the same document after a lost reply replays that
     * creation instead of making a twin, and asking for a second blank document
     * once the first one exists still creates a second one.
     */
    fun createDocument(title: String? = null, templateId: String? = null) {
        val api = api ?: return
        val clean = title?.trim()?.takeIf { it.isNotEmpty() }
        // NUL-separated because a title may contain anything: two different
        // (title, template) pairs must never share a fingerprint, or the second
        // creation would reuse the first one's key and replay its response.
        val fingerprint = clean.orEmpty() + "\u0000" + templateId.orEmpty()
        val intent = createIntent?.takeIf { it.fingerprint == fingerprint }
            ?: CreateIntent(fingerprint, newIntentKey("create")).also { createIntent = it }
        _state.value = _state.value.copy(busy = true)
        viewModelScope.launch {
            val result = runCatching {
                withContext(Dispatchers.IO) {
                    api.createDocument(
                        body = CreateDocumentBody(title = clean, templateId = templateId),
                        idempotencyKey = intent.key,
                    )
                }
            }
            result.onSuccess { response ->
                createIntent = null
                saveIntentKey = null
                _state.value = _state.value.copy(
                    busy = false,
                    openDoc = OpenDocument(
                        document = response.document,
                        doc = PmParser.parseDocumentOrEmpty(response.contentJson),
                        contentJson = response.contentJson,
                        etag = response.document.etag,
                        contentEtag = response.document.contentEtag,
                    ),
                    editing = false,
                    dirty = false,
                    message = "Document créé",
                )
                refresh()
            }.onFailure { e ->
                _state.value = _state.value.copy(busy = false, error = errorOf(e))
            }
        }
    }

    /** Renames a document; the module renames the underlying file to match. */
    fun rename(id: String, title: String) {
        val api = api ?: return
        val clean = title.trim()
        if (clean.isEmpty()) return
        val open = _state.value.openDoc?.takeIf { it.document.id == id }
        patchRow(id) { it.copy(title = clean) }
        _state.value = _state.value.copy(
            busy = true,
            openDoc = open?.copy(document = open.document.copy(title = clean))
                ?: _state.value.openDoc,
        )
        viewModelScope.launch {
            val result = runCatching {
                withContext(Dispatchers.IO) {
                    api.updateDocument(id, UpdateDocumentBody(title = clean), ifMatch = open?.etag)
                }
            }
            result.onSuccess { response ->
                val current = _state.value.openDoc
                _state.value = _state.value.copy(
                    busy = false,
                    openDoc = if (current?.document?.id == id) {
                        current.copy(
                            document = response.document,
                            etag = response.document.etag ?: current.etag,
                        )
                    } else {
                        current
                    },
                )
            }.onFailure { e ->
                _state.value = _state.value.copy(busy = false, error = errorOf(e))
                refresh()
            }
        }
    }

    /** Toggles the star, optimistically in the listing and on the open document. */
    fun toggleStar(id: String) {
        val api = api ?: return
        val open = _state.value.openDoc?.takeIf { it.document.id == id }
        val current = open?.document?.isStarred
            ?: rowOf(id)?.isStarred
            ?: return
        val next = !current
        patchRow(id) { it.copy(isStarred = next) }
        if (open != null) {
            _state.value = _state.value.copy(
                openDoc = open.copy(document = open.document.copy(isStarred = next)),
            )
        }
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    api.updateDocument(id, UpdateDocumentBody(isStarred = next))
                }
            }.onSuccess { response ->
                // Every PATCH mints a new etag, this one included. Dropping it
                // would leave the open document holding the one from before the
                // star toggle, and the next save would be rejected as a conflict
                // nobody caused — where "keep the server version" throws the
                // user's text away.
                val starred = _state.value.openDoc
                if (starred?.document?.id == id) {
                    _state.value = _state.value.copy(
                        openDoc = starred.copy(
                            document = response.document,
                            etag = response.document.etag ?: starred.etag,
                            contentEtag = response.document.contentEtag ?: starred.contentEtag,
                        ),
                    )
                }
            }.onFailure {
                patchRow(id) { it.copy(isStarred = current) }
                val reverted = _state.value.openDoc
                if (reverted?.document?.id == id) {
                    _state.value = _state.value.copy(
                        openDoc = reverted.copy(document = reverted.document.copy(isStarred = current)),
                    )
                }
            }
        }
    }

    fun duplicate(id: String) {
        val api = api ?: return
        _state.value = _state.value.copy(busy = true)
        viewModelScope.launch {
            val result = runCatching { withContext(Dispatchers.IO) { api.duplicate(id) } }
            _state.value = _state.value.copy(
                busy = false,
                message = if (result.isSuccess) "Copie créée" else null,
                error = result.exceptionOrNull()?.let(::errorOf),
            )
            if (result.isSuccess) refresh()
        }
    }

    fun trash(id: String) {
        val api = api ?: return
        dropRow(id)
        if (_state.value.openDoc?.document?.id == id) closeDocument()
        viewModelScope.launch {
            val result = runCatching { withContext(Dispatchers.IO) { api.trash(id) } }
            _state.value = _state.value.copy(
                message = if (result.isSuccess) "Déplacé vers la corbeille" else null,
                error = result.exceptionOrNull()?.let(::errorOf),
            )
            if (result.isFailure) refresh()
        }
    }

    fun restore(id: String) {
        val api = api ?: return
        dropRow(id)
        viewModelScope.launch {
            val result = runCatching { withContext(Dispatchers.IO) { api.restore(id) } }
            _state.value = _state.value.copy(
                message = if (result.isSuccess) "Document restauré" else null,
                error = result.exceptionOrNull()?.let(::errorOf),
            )
            refresh()
        }
    }

    /**
     * Permanent removal. The module refuses it on a live document, so a row that
     * is not trashed yet is trashed first — otherwise the call is a plain 404.
     */
    fun deleteForever(id: String) {
        val api = api ?: return
        val needsTrash = rowOf(id)?.isTrashed == false
        dropRow(id)
        viewModelScope.launch {
            val result = runCatching {
                withContext(Dispatchers.IO) {
                    if (needsTrash) api.trash(id)
                    api.deleteForever(id)
                }
            }
            _state.value = _state.value.copy(
                message = if (result.isSuccess) "Document supprimé définitivement" else null,
                error = result.exceptionOrNull()?.let(::errorOf),
            )
            if (result.isFailure) refresh()
        }
    }

    // ── Export ───────────────────────────────────────────────────────────────

    /**
     * Downloads the open document as .docx or .odt into the cache and signals the
     * UI to hand the file to the system. The stream is copied on the IO
     * dispatcher: an export can be several megabytes.
     */
    fun exportAs(format: DocsExportFormat) {
        val api = api ?: return
        val open = _state.value.openDoc ?: return
        _state.value = _state.value.copy(exporting = true)
        viewModelScope.launch {
            val fileName = safeFileName(open.document.title) + "." + format.extension
            val result = runCatching {
                withContext(Dispatchers.IO) {
                    val body = when (format) {
                        DocsExportFormat.DOCX -> api.exportDocx(open.document.id)
                        DocsExportFormat.ODT -> api.exportOdt(open.document.id)
                    }
                    val dir = File(appContext.cacheDir, "exports").apply { mkdirs() }
                    val file = File(dir, fileName)
                    body.byteStream().use { input -> file.outputStream().use { input.copyTo(it) } }
                    file
                }
            }
            result.onSuccess { file ->
                _state.value = _state.value.copy(
                    exporting = false,
                    exportReady = ExportReady(file, format.mime, fileName),
                )
            }.onFailure { e ->
                _state.value = _state.value.copy(exporting = false, error = errorOf(e))
            }
        }
    }

    /** Strips the characters Android and the share targets choke on. */
    private fun safeFileName(title: String): String {
        val cleaned = title.trim().replace(Regex("[\\\\/:*?\"<>|]"), "_")
        return cleaned.ifEmpty { "document" }.take(120)
    }

    // ── One-shot effects ─────────────────────────────────────────────────────

    fun messageConsumed() {
        _state.value = _state.value.copy(message = null)
    }

    fun errorConsumed() {
        _state.value = _state.value.copy(error = null)
    }

    fun saveErrorConsumed() {
        _state.value = _state.value.copy(saveError = null)
    }

    fun exportConsumed() {
        _state.value = _state.value.copy(exportReady = null)
    }

    // ── Local row helpers ────────────────────────────────────────────────────

    private fun rowOf(id: String): DocumentSummaryDto? =
        _state.value.documents.firstOrNull { it.id == id }
            ?: _state.value.recents.firstOrNull { it.id == id }

    private fun patchRow(id: String, edit: (DocumentSummaryDto) -> DocumentSummaryDto) {
        val map = { rows: List<DocumentSummaryDto> -> rows.map { if (it.id == id) edit(it) else it } }
        _state.value = _state.value.copy(
            documents = map(_state.value.documents),
            recents = map(_state.value.recents),
        )
    }

    private fun dropRow(id: String) {
        _state.value = _state.value.copy(
            documents = _state.value.documents.filterNot { it.id == id },
            recents = _state.value.recents.filterNot { it.id == id },
            selection = _state.value.selection - id,
        )
    }

    // ── Errors ───────────────────────────────────────────────────────────────

    /**
     * Turns a failed call into a code the UI branches on. The code comes from the
     * module's `{ "error": … }` envelope; the HTTP status is only the fallback for
     * a body that is missing or not ours (a proxy error page, say).
     */
    private fun errorOf(t: Throwable): DocsError = when (t) {
        is HttpException -> {
            // Retrofit buffers the error body in memory, so reading it here costs
            // no I/O and cannot block.
            val body = runCatching { t.response()?.errorBody()?.string() }.getOrNull()
            val code = body
                ?.let { runCatching { json.decodeFromString(ApiErrorDto.serializer(), it).error }.getOrNull() }
                ?: statusCode(t.code())
            DocsError(code, frenchMessage(code))
        }
        is IOException -> DocsError(DocsError.NETWORK, frenchMessage(DocsError.NETWORK))
        else -> DocsError(DocErrorCode.INTERNAL, frenchMessage(DocErrorCode.INTERNAL))
    }

    private fun statusCode(status: Int): String = when (status) {
        401 -> DocErrorCode.UNAUTHORIZED
        403 -> DocErrorCode.FORBIDDEN
        404 -> DocErrorCode.NOT_FOUND
        409 -> DocErrorCode.CONFLICT
        412 -> DocErrorCode.PRECONDITION_FAILED
        422 -> DocErrorCode.VALIDATION
        else -> DocErrorCode.INTERNAL
    }

    /** Short French prose per code. Never parsed — only displayed. */
    private fun frenchMessage(code: String): String = when (code) {
        DocErrorCode.UNAUTHORIZED ->
            "Session expirée. Reconnectez-vous depuis l'application Kubuno Drive."
        DocErrorCode.FORBIDDEN -> "Vous n'avez pas accès à ce document."
        DocErrorCode.POLICY_DISABLED -> "Cette fonction est désactivée sur ce serveur."
        DocErrorCode.NOT_FOUND -> "Document introuvable."
        DocErrorCode.VALIDATION -> "Demande invalide."
        DocErrorCode.CONFLICT -> "Opération impossible dans l'état actuel."
        DocErrorCode.PRECONDITION_FAILED -> "Le document a été modifié ailleurs."
        DocErrorCode.CONVERSION_ERROR -> "Ce format n'a pas pu être converti."
        DocErrorCode.DATABASE_ERROR, DocErrorCode.INTERNAL -> "Erreur du serveur. Réessayez."
        DocsError.NETWORK -> "Connexion impossible. Vérifiez votre réseau."
        DocsError.NO_ACCOUNT -> "Aucun compte Kubuno sur cet appareil."
        else -> "Une erreur est survenue."
    }

    override fun onCleared() {
        stopPing()
        searchJob?.cancel()
        super.onCleared()
    }

    private companion object {
        /** The server caps `limit` at 200; 100 keeps a page light on mobile. */
        const val PAGE_SIZE = 100

        /** The recents branch caps itself at 20 rows, so asking for more is moot. */
        const val RECENTS_LIMIT = 20

        const val SEARCH_DEBOUNCE_MS = 300L

        /** Two minutes without a ping expires the session; refresh well before. */
        const val PING_INTERVAL_MS = 45_000L

        const val TAG = "DocsSync"
    }
}
