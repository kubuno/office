package com.kubuno.docs.ui

import android.content.Context
import android.content.Intent
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import com.kubuno.android.account.SharedAccount
import com.kubuno.android.account.SharedAccounts
import com.kubuno.android.ui.components.KubunoButton
import com.kubuno.android.ui.components.KubunoButtonSize
import com.kubuno.android.ui.components.KubunoButtonVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoConfirmDialog
import com.kubuno.android.ui.components.KubunoEmptyState
import com.kubuno.android.ui.components.KubunoEmptyTone
import com.kubuno.android.ui.components.KubunoSpinner
import com.kubuno.android.ui.components.KubunoSpinnerSize
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The assembled root of the documents app.
 *
 * Structure follows the repository convention (photos, mail, maps): no
 * navigation library, one immutable state object, and every full-screen surface
 * as an overlay driven by a nullable state field.
 *
 * The shell is deliberately thin. [DocListScreen] owns the whole listing
 * experience (tab strip, search bar, filters, selection bar, its own rename and
 * permanent-delete dialogs); [DocReaderScreen] and [DocEditorScreen] own the open
 * document, the editor also owning the save-conflict arbitration because it is
 * the only screen that writes. What is left here is what must exist exactly once
 * whatever is showing: the account gate and switcher, the unsaved-changes guard,
 * the transient message surface, and the two bottom sheets (share, export) that
 * a listing row can ask for but cannot host.
 */
@Composable
fun DocsApp(viewModel: DocsViewModel = hiltViewModel()) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current

    // Why the export failure is a field and not a toast: handing the file over can
    // fail after the download succeeded, and the user needs to keep reading why.
    var exportError by remember { mutableStateOf<String?>(null) }
    // Accounts found by the gate below after the view model took its snapshot.
    var rescanned by remember { mutableStateOf<List<SharedAccount>>(emptyList()) }

    // Transient success prose. Kept as a toast rather than a snackbar because the
    // editor overlay covers the whole window and has nowhere to host one.
    LaunchedEffect(state.message) {
        val message = state.message ?: return@LaunchedEffect
        Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
        viewModel.messageConsumed()
    }

    LaunchedEffect(state.exportReady) {
        val ready = state.exportReady ?: return@LaunchedEffect
        exportError = shareExport(context, ready)
        viewModel.exportConsumed()
    }

    if (!state.ready) {
        Box(
            Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background),
            contentAlignment = Alignment.Center,
        ) {
            KubunoSpinner(size = KubunoSpinnerSize.LG)
        }
        return
    }

    val account = state.account
    if (account == null) {
        // The view model reads the device accounts once, when it is built. A user
        // who follows the instruction — sign in from Drive, come back — would stay
        // locked out for the life of the process, so the gate re-reads the list
        // (on demand and when the app returns to the foreground) and hands the
        // view model whatever it finds.
        NoAccountGate(
            onFound = { found ->
                rescanned = found
                found.firstOrNull()?.let(viewModel::selectAccount)
            },
        )
        return
    }

    var confirmDiscard by remember { mutableStateOf(false) }
    var shareTarget by remember { mutableStateOf<ShareTarget?>(null) }
    // The reader has no id to retry with once an open has failed, and the export
    // sheet needs the document to be open before it can name its source format.
    var lastOpenedId by remember { mutableStateOf<String?>(null) }
    var pendingExportId by remember { mutableStateOf<String?>(null) }

    // Closing a document with unsaved edits must ask first: leaving the editing
    // session drops whatever was never PATCHed.
    val requestCloseDocument: () -> Unit = {
        pendingExportId = null
        if (state.dirty) confirmDiscard = true else viewModel.closeDocument()
    }

    // One handler for the whole back stack: several overlays can be open at once
    // (a document opened from a search result), and nested handlers would make
    // their priority depend on composition order.
    BackHandler(enabled = state.openDoc != null || state.searchActive || state.selectionMode) {
        when {
            state.openDoc != null -> requestCloseDocument()
            state.searchActive -> viewModel.clearSearch()
            else -> viewModel.clearSelection()
        }
    }

    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        DocListScreen(
            state = state,
            modifier = Modifier.fillMaxSize(),
            onSelectTab = viewModel::selectTab,
            onRefresh = viewModel::refresh,
            onLoadMore = viewModel::loadMore,
            onSetFilter = viewModel::setFilter,
            onSetSort = viewModel::setSortOrder,
            onOpenSearch = viewModel::openSearch,
            onSearch = viewModel::search,
            onClearSearch = viewModel::clearSearch,
            onLoadTemplates = viewModel::loadTemplates,
            onOpenDocument = { id ->
                lastOpenedId = id
                // A plain open must not resurrect an export sheet asked for earlier.
                pendingExportId = null
                viewModel.openDocument(id)
            },
            onCreateDocument = { title, templateId -> viewModel.createDocument(title, templateId) },
            onRename = viewModel::rename,
            onToggleStar = viewModel::toggleStar,
            onDuplicate = viewModel::duplicate,
            onShare = { id ->
                shareTarget = ShareTarget(id, documentTitle(state, id))
            },
            // Exporting needs the document itself (its source format decides what
            // "save to source" may do), so a row export opens it first and the
            // sheet below waits for that document to arrive.
            onExport = { id ->
                lastOpenedId = id
                pendingExportId = id
                viewModel.openDocument(id)
            },
            onTrash = viewModel::trash,
            onRestore = viewModel::restore,
            onDeleteForever = viewModel::deleteForever,
            onToggleSelect = viewModel::toggleSelect,
            onClearSelection = viewModel::clearSelection,
            onTrashSelection = viewModel::trashSelection,
            onDismissError = viewModel::errorConsumed,
            accountSlot = {
                AccountButton(
                    account = account,
                    // A rescan can know about accounts the snapshot predates.
                    accounts = (state.accounts + rescanned).distinctBy { it.systemName },
                    onSelectAccount = viewModel::selectAccount,
                )
            },
        )

        val open = state.openDoc
        if (open != null || state.opening) {
            if (state.editing && open != null) {
                DocEditorScreen(
                    state = state,
                    viewModel = viewModel,
                    modifier = Modifier.fillMaxSize(),
                    onClose = requestCloseDocument,
                )
            } else {
                DocReaderScreen(
                    state = state,
                    onClose = requestCloseDocument,
                    onEdit = viewModel::startEditing,
                    onRetry = { lastOpenedId?.let(viewModel::openDocument) },
                    onRename = viewModel::rename,
                    onToggleStar = viewModel::toggleStar,
                    onDuplicate = viewModel::duplicate,
                    onTrash = viewModel::trash,
                    onExport = viewModel::exportAs,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }

        shareTarget?.let { target ->
            DocShareSheet(
                documentId = target.id,
                documentTitle = target.title,
                account = account,
                onDismiss = { shareTarget = null },
            )
        }

        val exported = open?.takeIf { it.document.id == pendingExportId }
        if (exported != null) {
            DocExportSheet(
                document = exported.document,
                account = account,
                onDismiss = { pendingExportId = null },
                dirty = state.dirty,
                onMessage = { line -> Toast.makeText(context, line, Toast.LENGTH_SHORT).show() },
            )
        }

        // Drawn last so it stays readable over the reader and the editor, which
        // are the screens an export is started from.
        exportError?.let { line ->
            Column(
                Modifier
                    .align(Alignment.BottomCenter)
                    .padding(16.dp)
                    .navigationBarsPadding(),
            ) {
                KubunoCallout(
                    text = line,
                    variant = KubunoCalloutVariant.DANGER,
                    title = "Partage impossible",
                )
                KubunoButton(
                    text = "Fermer",
                    onClick = { exportError = null },
                    variant = KubunoButtonVariant.TEXT,
                    size = KubunoButtonSize.SM,
                )
            }
        }

        if (confirmDiscard) {
            KubunoConfirmDialog(
                title = "Modifications non enregistrées",
                message = "Fermer ce document maintenant abandonnera les modifications qui n'ont pas été enregistrées.",
                onConfirm = {
                    confirmDiscard = false
                    viewModel.closeDocument()
                },
                onDismiss = { confirmDiscard = false },
                confirmText = "Fermer sans enregistrer",
                cancelText = "Continuer l'édition",
                danger = true,
            )
        }
    }
}

/** The document a share sheet was opened for, with the title to show in it. */
private data class ShareTarget(val id: String, val title: String)

/** Title of a listed document, for the surfaces that only receive an id. */
private fun documentTitle(state: DocsUiState, id: String): String =
    (state.documents + state.recents).firstOrNull { it.id == id }?.title.orEmpty()

/**
 * The "no account" screen, plus the rescan that gets out of it.
 *
 * [onFound] is called with the accounts read from the device, whenever a rescan
 * finds at least one: on demand, and each time the app comes back to the
 * foreground — which is exactly when the user returns from the sibling app they
 * just signed in to.
 */
@Composable
private fun NoAccountGate(onFound: (List<SharedAccount>) -> Unit) {
    val context = LocalContext.current
    var searchedInVain by remember { mutableStateOf(false) }

    // `asked` distinguishes the user's own tap from the automatic rescan: only a
    // tap that found nothing deserves an answer on screen.
    val rescan: (asked: Boolean) -> Unit = { asked ->
        val found = runCatching { sharedAccountsOf(context).list() }.getOrDefault(emptyList())
        if (found.isEmpty()) {
            if (asked) searchedInVain = true
        } else {
            searchedInVain = false
            onFound(found)
        }
    }

    LifecycleResumeEffect(Unit) {
        rescan(false)
        onPauseOrDispose {}
    }

    NoAccount(onRetry = { rescan(true) }, searchedInVain = searchedInVain)
}

/**
 * Shown when the device carries no Kubuno account: Documents is a consumer app,
 * it cannot sign anyone in and can only wait for a sibling app to do it.
 */
@Composable
fun NoAccount(
    modifier: Modifier = Modifier,
    onRetry: (() -> Unit)? = null,
    searchedInVain: Boolean = false,
) {
    val retryAction: (@Composable () -> Unit)? = onRetry?.let { retry ->
        {
            KubunoButton(
                text = "Réessayer",
                onClick = retry,
                variant = KubunoButtonVariant.SECONDARY,
                size = KubunoButtonSize.SM,
            )
        }
    }
    Box(
        modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .padding(32.dp),
        contentAlignment = Alignment.Center,
    ) {
        KubunoEmptyState(
            icon = Icons.Filled.CloudOff,
            title = "Aucun compte Kubuno",
            description = "Connectez-vous depuis une application Kubuno, par exemple Drive : " +
                "Documents réutilise le compte déjà enregistré sur l'appareil." +
                // Silence after a tap would read as a broken button.
                (if (searchedInVain) "\n\nToujours aucun compte sur cet appareil." else ""),
            tone = KubunoEmptyTone.UNAVAILABLE,
            actions = retryAction,
        )
    }
}

@EntryPoint
@InstallIn(SingletonComponent::class)
internal interface DocsAccountsEntryPoint {
    fun sharedAccounts(): SharedAccounts
}

private fun sharedAccountsOf(context: Context): SharedAccounts =
    EntryPointAccessors
        .fromApplication(context.applicationContext, DocsAccountsEntryPoint::class.java)
        .sharedAccounts()

// ─────────────────────────────────────────────────────────────────────────────
// Account switcher
// ─────────────────────────────────────────────────────────────────────────────

@Composable
private fun AccountButton(
    account: SharedAccount,
    accounts: List<SharedAccount>,
    onSelectAccount: (SharedAccount) -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Box {
        AccountAvatar(
            account = account,
            // A single account has nothing to switch to, so the avatar stays inert.
            modifier = Modifier
                .padding(6.dp)
                .clickable(enabled = accounts.size > 1) { menuOpen = true },
        )
        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            accounts.forEach { candidate ->
                DropdownMenuItem(
                    text = {
                        Column {
                            Text(candidate.label, style = MaterialTheme.typography.bodyMedium)
                            Text(
                                candidate.host,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    },
                    onClick = {
                        menuOpen = false
                        onSelectAccount(candidate)
                    },
                )
            }
        }
    }
}

@Composable
private fun AccountAvatar(account: SharedAccount, modifier: Modifier = Modifier) {
    val initial = account.label.trim().firstOrNull()?.uppercaseChar()?.toString() ?: "?"
    Surface(
        modifier = modifier.size(34.dp),
        shape = CircleShape,
        color = docsTone(),
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                text = initial,
                color = Color.White,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
            )
            // Drawn over the initial: the avatar endpoint answers with nothing when
            // the user has no picture, and the letter stays visible underneath.
            AsyncImage(
                model = docsAvatarUrl(account),
                contentDescription = null,
                contentScale = androidx.compose.ui.layout.ContentScale.Crop,
                modifier = Modifier.matchParentSize().clip(CircleShape),
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Export hand-off
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Hands a downloaded export to the system share sheet.
 *
 * The file is shared where the view model wrote it (`cache/exports`, exposed
 * under that name in `res/xml/file_paths.xml`): staging a second copy only
 * duplicated several megabytes for nothing.
 *
 * Returns null on success, or the line to show the user — a hand-off that fails
 * silently leaves them staring at a document that never left the app.
 */
private suspend fun shareExport(context: Context, ready: ExportReady): String? {
    val exists = withContext(Dispatchers.IO) { runCatching { ready.file.isFile }.getOrDefault(false) }
    if (!exists) return "Le fichier exporté est introuvable. Relancez l'export."
    val uri = runCatching {
        FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", ready.file)
    }.getOrNull() ?: return "Le fichier exporté n'a pas pu être partagé (« ${ready.fileName} »)."
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = ready.mime
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    return runCatching { context.startActivity(Intent.createChooser(intent, "Partager le document")) }
        .fold(
            onSuccess = { null },
            onFailure = { "Aucune application ne peut recevoir ce document." },
        )
}
