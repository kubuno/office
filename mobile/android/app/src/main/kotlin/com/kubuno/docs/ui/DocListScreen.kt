package com.kubuno.docs.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.automirrored.filled.ViewList
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Article
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.DeleteForever
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.GridView
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kubuno.android.ui.components.KubunoButton
import com.kubuno.android.ui.components.KubunoButtonSize
import com.kubuno.android.ui.components.KubunoButtonVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoCard
import com.kubuno.android.ui.components.KubunoChip
import com.kubuno.android.ui.components.KubunoConfirmDialog
import com.kubuno.android.ui.components.KubunoEmptyState
import com.kubuno.android.ui.components.KubunoEmptyTone
import com.kubuno.android.ui.components.KubunoListRow
import com.kubuno.android.ui.components.KubunoPromptDialog
import com.kubuno.android.ui.components.KubunoSpinner
import com.kubuno.android.ui.components.KubunoSpinnerSize
import com.kubuno.android.ui.components.KubunoTab
import com.kubuno.android.ui.components.KubunoTabs
import com.kubuno.android.ui.components.KubunoTabsVariant
import com.kubuno.android.ui.components.KubunoTextField
import com.kubuno.docs.net.DocumentSummaryDto
import com.kubuno.docs.net.TemplateDto
import java.time.Instant
import java.time.OffsetDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * The home screen of the documents app: the three destinations of the web start
 * page (office/frontend/src/DocumentsApp.tsx — recents column, "Parcourir" tab,
 * "Modèles" tab), collapsed into a single tab strip because a phone has no room
 * for the desktop's two-pane start page.
 *
 * Everything it renders comes from [DocsUiState]; every action leaves through an
 * explicit lambda, so the screen holds no view model and stays previewable. The
 * only local state is presentational (list/grid, which menu is open, which
 * dialog is up) — never anything the server or the view model owns.
 */
@Composable
fun DocListScreen(
    state: DocsUiState,
    modifier: Modifier = Modifier,
    onSelectTab: (DocsTab) -> Unit,
    onRefresh: () -> Unit,
    onLoadMore: () -> Unit,
    onSetFilter: (DocsFilter) -> Unit,
    onSetSort: (DocsSort) -> Unit,
    onOpenSearch: () -> Unit,
    onSearch: (String) -> Unit,
    onClearSearch: () -> Unit,
    onLoadTemplates: () -> Unit,
    onOpenDocument: (String) -> Unit,
    /** (title, templateId) — both null creates a blank document, like the web's "Nouveau". */
    onCreateDocument: (String?, String?) -> Unit,
    onRename: (String, String) -> Unit,
    onToggleStar: (String) -> Unit,
    onDuplicate: (String) -> Unit,
    onShare: (String) -> Unit,
    /**
     * Export is a property of the OPEN document in the view model
     * (`exportAs(format)`), so from a listing row the host has to open the
     * document first; this only names the row the user picked.
     */
    onExport: (String) -> Unit,
    onTrash: (String) -> Unit,
    onRestore: (String) -> Unit,
    onDeleteForever: (String) -> Unit,
    onToggleSelect: (String) -> Unit,
    onClearSelection: () -> Unit,
    onTrashSelection: () -> Unit,
    onDismissError: () -> Unit,
    /**
     * Trailing slot of the title bar. The shell puts the account switcher there:
     * this screen draws the only top bar of the app, so there is nowhere else for
     * it to live.
     */
    accountSlot: @Composable () -> Unit = {},
) {
    // The templates tab is lazy on the server side too; ask for it the first time
    // it is shown rather than on every app start.
    LaunchedEffect(state.tab) {
        if (state.tab == DocsTab.TEMPLATES && state.templates.isEmpty()) onLoadTemplates()
    }

    // Presentational-only state. `rememberSaveable` so a rotation does not send
    // the user back to the grid after they chose the list.
    var gridMode by rememberSaveable { mutableStateOf(true) }
    var renameTarget by remember { mutableStateOf<DocumentSummaryDto?>(null) }
    var deleteTarget by remember { mutableStateOf<DocumentSummaryDto?>(null) }

    Column(
        modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding(),
    ) {
        if (state.selectionMode) {
            SelectionBar(
                count = state.selection.size,
                onClose = onClearSelection,
                onTrash = onTrashSelection,
            )
        } else {
            TopBar(
                searchActive = state.searchActive,
                query = state.query,
                searching = state.searching,
                onOpenSearch = onOpenSearch,
                onSearch = onSearch,
                onClearSearch = onClearSearch,
                onRefresh = onRefresh,
                accountSlot = accountSlot,
            )
        }

        KubunoTabs(
            tabs = TAB_LABELS,
            selectedId = state.tab.name,
            onSelect = { id -> onSelectTab(DocsTab.valueOf(id)) },
            modifier = Modifier.padding(horizontal = 12.dp),
            variant = KubunoTabsVariant.UNDERLINED,
        )

        // A failure that still leaves rows on screen is a banner, not an empty
        // state: throwing away a usable listing because a refresh failed is worse
        // than showing it slightly stale. Templates are fetched on their own, so
        // the banner has to read the failure of the tab actually on screen.
        val tabError = if (state.tab == DocsTab.TEMPLATES) state.templatesError else state.error
        if (tabError != null && hasRowsFor(state)) {
            KubunoCallout(
                text = tabError.message,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                variant = KubunoCalloutVariant.DANGER,
            )
        }

        Box(Modifier.fillMaxSize()) {
            when (state.tab) {
                DocsTab.RECENTS -> RecentsTab(
                    state = state,
                    onOpenDocument = onOpenDocument,
                    onCreateDocument = { onCreateDocument(null, null) },
                    onDismissError = onDismissError,
                    onRefresh = onRefresh,
                )

                DocsTab.BROWSE -> BrowseTab(
                    state = state,
                    gridMode = gridMode,
                    onToggleViewMode = { gridMode = !gridMode },
                    onSetFilter = onSetFilter,
                    onSetSort = onSetSort,
                    onLoadMore = onLoadMore,
                    onOpenDocument = onOpenDocument,
                    onCreateDocument = { onCreateDocument(null, null) },
                    onRenameRequest = { renameTarget = it },
                    onToggleStar = onToggleStar,
                    onDuplicate = onDuplicate,
                    onShare = onShare,
                    onExport = onExport,
                    onTrash = onTrash,
                    onRestore = onRestore,
                    onDeleteRequest = { deleteTarget = it },
                    onToggleSelect = onToggleSelect,
                    onRefresh = onRefresh,
                )

                DocsTab.TEMPLATES -> TemplatesTab(
                    state = state,
                    onUseTemplate = { template -> onCreateDocument(null, template.id) },
                    onRefresh = onLoadTemplates,
                )
            }
        }
    }

    // Renaming and permanent deletion are confirmed with the design system's own
    // dialogs — the project forbids platform alert/confirm dialogs.
    renameTarget?.let { target ->
        KubunoPromptDialog(
            title = "Renommer le document",
            onConfirm = { title ->
                renameTarget = null
                if (title.isNotBlank()) onRename(target.id, title)
            },
            onDismiss = { renameTarget = null },
            label = "Titre",
            placeholder = "Sans titre",
            initialValue = target.title,
            confirmText = "Renommer",
        )
    }

    deleteTarget?.let { target ->
        // The module refuses permanent deletion on a live document (`is_trashed`
        // must already be true), so a row outside the trash is trashed first by
        // the view model — the user has to be told before confirming.
        val name = target.title.ifBlank { "Sans titre" }
        KubunoConfirmDialog(
            title = "Supprimer définitivement ?",
            message = if (target.isTrashed) {
                "« $name » et ses versions seront supprimés définitivement. Cette action est irréversible."
            } else {
                "Le serveur n'accepte la suppression définitive que depuis la corbeille : " +
                    "« $name » y sera d'abord placé, puis supprimé avec ses versions. " +
                    "Cette action est irréversible."
            },
            onConfirm = {
                deleteTarget = null
                onDeleteForever(target.id)
            },
            onDismiss = { deleteTarget = null },
            confirmText = "Supprimer",
            danger = true,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Chrome
// ─────────────────────────────────────────────────────────────────────────────

@Composable
private fun TopBar(
    searchActive: Boolean,
    query: String,
    searching: Boolean,
    onOpenSearch: () -> Unit,
    onSearch: (String) -> Unit,
    onClearSearch: () -> Unit,
    onRefresh: () -> Unit,
    accountSlot: @Composable () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (searchActive) {
            KubunoTextField(
                value = query,
                onValueChange = onSearch,
                modifier = Modifier.weight(1f),
                placeholder = "Rechercher un document",
                leadingIcon = {
                    Icon(
                        Icons.Filled.Search,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                },
                trailingIcon = {
                    if (searching) {
                        KubunoSpinner(size = KubunoSpinnerSize.SM)
                    } else {
                        IconButton(onClick = onClearSearch) {
                            Icon(Icons.Filled.Close, contentDescription = "Fermer la recherche")
                        }
                    }
                },
            )
        } else {
            Text(
                text = "Documents",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = onOpenSearch) {
                Icon(Icons.Filled.Search, contentDescription = "Rechercher")
            }
            IconButton(onClick = onRefresh) {
                Icon(Icons.Filled.Refresh, contentDescription = "Actualiser")
            }
            accountSlot()
        }
    }
}

@Composable
private fun SelectionBar(count: Int, onClose: () -> Unit, onTrash: () -> Unit) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 3.dp,
        shadowElevation = 3.dp,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 4.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onClose) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Annuler la sélection")
            }
            Text(
                text = "$count sélectionné${if (count > 1) "s" else ""}",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = onTrash) {
                Icon(Icons.Filled.Delete, contentDescription = "Mettre à la corbeille")
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Recents
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The recents rail. The server caps this branch at 20 rows and never pages it,
 * so the whole set fits in one horizontal strip — the phone equivalent of the
 * start page's recents column.
 */
@Composable
private fun RecentsTab(
    state: DocsUiState,
    onOpenDocument: (String) -> Unit,
    onCreateDocument: () -> Unit,
    onDismissError: () -> Unit,
    onRefresh: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .navigationBarsPadding(),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Récents",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            KubunoButton(
                text = "Nouveau document",
                onClick = onCreateDocument,
                size = KubunoButtonSize.SM,
                enabled = !state.busy,
                loading = state.busy,
                icon = { Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(16.dp)) },
            )
        }

        val error = state.error
        when {
            state.loading && state.recents.isEmpty() -> LoadingBlock()

            error != null && state.recents.isEmpty() -> KubunoEmptyState(
                icon = Icons.Filled.Description,
                title = "Chargement impossible",
                modifier = Modifier.fillMaxWidth().padding(24.dp),
                description = error.message,
                tone = KubunoEmptyTone.ERROR,
                actions = {
                    KubunoButton(
                        text = "Réessayer",
                        onClick = { onDismissError(); onRefresh() },
                        variant = KubunoButtonVariant.SECONDARY,
                        size = KubunoButtonSize.SM,
                    )
                },
            )

            state.recents.isEmpty() -> KubunoEmptyState(
                icon = Icons.Filled.Description,
                title = "Aucun document récent",
                modifier = Modifier.fillMaxWidth().padding(24.dp),
                description = "Les documents que vous ouvrez apparaîtront ici.",
                tone = KubunoEmptyTone.FIRST_USE,
                actions = {
                    KubunoButton(
                        text = "Créer un document",
                        onClick = onCreateDocument,
                        size = KubunoButtonSize.SM,
                    )
                },
            )

            else -> LazyRow(
                contentPadding = PaddingValues(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(state.recents, key = { it.id }) { doc ->
                    RecentCard(doc = doc, onOpen = { onOpenDocument(doc.id) })
                }
            }
        }

        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun RecentCard(doc: DocumentSummaryDto, onOpen: () -> Unit) {
    KubunoCard(modifier = Modifier.width(176.dp), flush = true) {
        Column {
            DocumentPreview(
                starred = doc.isStarred,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(104.dp),
            )
            Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
                Text(
                    doc.title.ifBlank { "Sans titre" },
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    shortDate(doc.updatedAt),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(6.dp))
                KubunoButton(
                    text = "Ouvrir",
                    onClick = onOpen,
                    modifier = Modifier.fillMaxWidth(),
                    variant = KubunoButtonVariant.SECONDARY,
                    size = KubunoButtonSize.SM,
                )
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Browse
// ─────────────────────────────────────────────────────────────────────────────

@Composable
private fun BrowseTab(
    state: DocsUiState,
    gridMode: Boolean,
    onToggleViewMode: () -> Unit,
    onSetFilter: (DocsFilter) -> Unit,
    onSetSort: (DocsSort) -> Unit,
    onLoadMore: () -> Unit,
    onOpenDocument: (String) -> Unit,
    onCreateDocument: () -> Unit,
    onRenameRequest: (DocumentSummaryDto) -> Unit,
    onToggleStar: (String) -> Unit,
    onDuplicate: (String) -> Unit,
    onShare: (String) -> Unit,
    onExport: (String) -> Unit,
    onTrash: (String) -> Unit,
    onRestore: (String) -> Unit,
    onDeleteRequest: (DocumentSummaryDto) -> Unit,
    onToggleSelect: (String) -> Unit,
    onRefresh: () -> Unit,
) {
    // Only one row menu can be open at a time; holding the id rather than a
    // boolean per row keeps that invariant without a map.
    var menuFor by remember { mutableStateOf<String?>(null) }

    val rowActions: @Composable (DocumentSummaryDto) -> Unit = { doc ->
        DocumentActionsMenu(
            doc = doc,
            expanded = menuFor == doc.id,
            onDismiss = { menuFor = null },
            onOpenDocument = onOpenDocument,
            onRenameRequest = onRenameRequest,
            onToggleStar = onToggleStar,
            onDuplicate = onDuplicate,
            onShare = onShare,
            onExport = onExport,
            onTrash = onTrash,
            onRestore = onRestore,
            onDeleteRequest = onDeleteRequest,
        )
    }

    Column(Modifier.fillMaxSize()) {
        BrowseToolbar(
            state = state,
            gridMode = gridMode,
            onToggleViewMode = onToggleViewMode,
            onSetSort = onSetSort,
            onCreateDocument = onCreateDocument,
        )
        FilterChips(filter = state.filter, onSetFilter = onSetFilter)

        val error = state.error
        when {
            state.loading && state.documents.isEmpty() -> LoadingBlock()

            error != null && state.documents.isEmpty() -> KubunoEmptyState(
                icon = Icons.Filled.Description,
                title = "Chargement impossible",
                modifier = Modifier.fillMaxWidth().padding(24.dp),
                description = error.message,
                tone = KubunoEmptyTone.ERROR,
                actions = {
                    KubunoButton(
                        text = "Réessayer",
                        onClick = onRefresh,
                        variant = KubunoButtonVariant.SECONDARY,
                        size = KubunoButtonSize.SM,
                    )
                },
            )

            state.documents.isEmpty() -> BrowseEmptyState(
                state = state,
                onCreateDocument = onCreateDocument,
            )

            gridMode -> DocumentGrid(
                state = state,
                onLoadMore = onLoadMore,
                onOpenDocument = onOpenDocument,
                onToggleSelect = onToggleSelect,
                onOpenMenu = { menuFor = it },
                rowActions = rowActions,
            )

            else -> DocumentList(
                state = state,
                onLoadMore = onLoadMore,
                onOpenDocument = onOpenDocument,
                onToggleSelect = onToggleSelect,
                onOpenMenu = { menuFor = it },
                rowActions = rowActions,
            )
        }
    }
}

@Composable
private fun BrowseToolbar(
    state: DocsUiState,
    gridMode: Boolean,
    onToggleViewMode: () -> Unit,
    onSetSort: (DocsSort) -> Unit,
    onCreateDocument: () -> Unit,
) {
    var sortMenu by remember { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 4.dp, top = 10.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The web puts "Nouveau" in the browser's toolbar and has no floating
        // action button; the phone keeps that placement.
        KubunoButton(
            text = "Nouveau document",
            onClick = onCreateDocument,
            size = KubunoButtonSize.SM,
            enabled = !state.busy,
            loading = state.busy,
            icon = { Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(16.dp)) },
        )
        Spacer(Modifier.weight(1f))
        Box {
            IconButton(onClick = { sortMenu = true }) {
                Icon(Icons.AutoMirrored.Filled.Sort, contentDescription = "Trier")
            }
            DropdownMenu(expanded = sortMenu, onDismissRequest = { sortMenu = false }) {
                SORT_LABELS.forEach { (sort, label) ->
                    DropdownMenuItem(
                        text = { Text(label) },
                        onClick = {
                            sortMenu = false
                            onSetSort(sort)
                        },
                        // A tick, like the web's view selector: a filled star means
                        // "favourite" everywhere else on this screen.
                        trailingIcon = {
                            if (state.sortOrder == sort) {
                                Icon(
                                    Icons.Filled.Check,
                                    contentDescription = "Tri actif",
                                    tint = docsTone(),
                                    modifier = Modifier.size(16.dp),
                                )
                            }
                        },
                    )
                }
            }
        }
        IconButton(onClick = onToggleViewMode) {
            Icon(
                if (gridMode) Icons.AutoMirrored.Filled.ViewList else Icons.Filled.GridView,
                contentDescription = if (gridMode) "Afficher en liste" else "Afficher en grille",
            )
        }
    }
}

@Composable
private fun FilterChips(filter: DocsFilter, onSetFilter: (DocsFilter) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FILTER_LABELS.forEach { (value, label) ->
            KubunoChip(
                label = label,
                selected = filter == value,
                onClick = { onSetFilter(value) },
            )
        }
    }
}

@Composable
private fun BrowseEmptyState(state: DocsUiState, onCreateDocument: () -> Unit) {
    val searching = state.query.isNotBlank()
    when {
        searching -> KubunoEmptyState(
            icon = Icons.Filled.Search,
            title = "Aucun résultat",
            modifier = Modifier.fillMaxWidth().padding(24.dp),
            description = "Aucun document ne correspond à « ${state.query} ».",
            tone = KubunoEmptyTone.NO_RESULTS,
        )

        state.filter == DocsFilter.TRASH -> KubunoEmptyState(
            icon = Icons.Filled.DeleteForever,
            title = "Corbeille vide",
            modifier = Modifier.fillMaxWidth().padding(24.dp),
            description = "Les documents supprimés apparaîtront ici.",
            tone = KubunoEmptyTone.NO_RESULTS,
        )

        state.filter == DocsFilter.STARRED -> KubunoEmptyState(
            icon = Icons.Filled.Star,
            title = "Aucun favori",
            modifier = Modifier.fillMaxWidth().padding(24.dp),
            description = "Marquez un document comme favori pour le retrouver ici.",
            tone = KubunoEmptyTone.NO_RESULTS,
        )

        state.filter == DocsFilter.SHARED -> KubunoEmptyState(
            icon = Icons.Filled.Description,
            title = "Aucun document partagé",
            modifier = Modifier.fillMaxWidth().padding(24.dp),
            description = "Les documents que l'on partage avec vous apparaîtront ici.",
            tone = KubunoEmptyTone.NO_RESULTS,
        )

        // An empty library is a first use, not a failed search: it gets the
        // welcoming tone and the create action.
        else -> KubunoEmptyState(
            icon = Icons.Filled.Description,
            title = "Aucun document",
            modifier = Modifier.fillMaxWidth().padding(24.dp),
            description = "Créez votre premier document pour commencer.",
            tone = KubunoEmptyTone.FIRST_USE,
            actions = {
                KubunoButton(text = "Nouveau document", onClick = onCreateDocument, size = KubunoButtonSize.SM)
            },
        )
    }
}

@Composable
private fun DocumentGrid(
    state: DocsUiState,
    onLoadMore: () -> Unit,
    onOpenDocument: (String) -> Unit,
    onToggleSelect: (String) -> Unit,
    onOpenMenu: (String) -> Unit,
    rowActions: @Composable (DocumentSummaryDto) -> Unit,
) {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = 160.dp),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 24.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        items(state.documents, key = { it.id }) { doc ->
            DocumentTile(
                doc = doc,
                selected = doc.id in state.selection,
                selectionMode = state.selectionMode,
                onOpen = { onOpenDocument(doc.id) },
                onToggleSelect = { onToggleSelect(doc.id) },
                onOpenMenu = { onOpenMenu(doc.id) },
                actions = { rowActions(doc) },
            )
        }
        if (state.canLoadMore || state.loadingMore) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                LoadMoreFooter(loading = state.loadingMore, onLoadMore = onLoadMore)
            }
        }
    }
}

@Composable
private fun DocumentList(
    state: DocsUiState,
    onLoadMore: () -> Unit,
    onOpenDocument: (String) -> Unit,
    onToggleSelect: (String) -> Unit,
    onOpenMenu: (String) -> Unit,
    rowActions: @Composable (DocumentSummaryDto) -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 8.dp, end = 8.dp, top = 8.dp, bottom = 24.dp),
    ) {
        items(state.documents, key = { it.id }) { doc ->
            DocumentRow(
                doc = doc,
                selected = doc.id in state.selection,
                selectionMode = state.selectionMode,
                onOpen = { onOpenDocument(doc.id) },
                onToggleSelect = { onToggleSelect(doc.id) },
                onOpenMenu = { onOpenMenu(doc.id) },
                actions = { rowActions(doc) },
            )
        }
        if (state.canLoadMore || state.loadingMore) {
            item { LoadMoreFooter(loading = state.loadingMore, onLoadMore = onLoadMore) }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DocumentRow(
    doc: DocumentSummaryDto,
    selected: Boolean,
    selectionMode: Boolean,
    onOpen: () -> Unit,
    onToggleSelect: () -> Unit,
    onOpenMenu: () -> Unit,
    actions: @Composable () -> Unit,
) {
    // The long press lives on a wrapper: KubunoListRow only exposes a plain
    // onClick, and re-implementing the row here would fork the design system.
    Box(
        Modifier
            .fillMaxWidth()
            .clip(DocsShape.Row)
            .combinedClickable(
                onClick = { if (selectionMode) onToggleSelect() else onOpen() },
                onLongClick = onToggleSelect,
            ),
    ) {
        KubunoListRow(
            title = doc.title.ifBlank { "Sans titre" },
            subtitle = rowSubtitle(doc),
            selected = selected,
            leading = {
                DocumentGlyph(starred = doc.isStarred)
            },
            trailing = {
                Box {
                    IconButton(onClick = onOpenMenu) {
                        Icon(Icons.Filled.MoreVert, contentDescription = "Actions du document")
                    }
                    actions()
                }
            },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DocumentTile(
    doc: DocumentSummaryDto,
    selected: Boolean,
    selectionMode: Boolean,
    onOpen: () -> Unit,
    onToggleSelect: () -> Unit,
    onOpenMenu: () -> Unit,
    actions: @Composable () -> Unit,
) {
    Box(
        Modifier
            .clip(DocsShape.Card)
            .combinedClickable(
                onClick = { if (selectionMode) onToggleSelect() else onOpen() },
                onLongClick = onToggleSelect,
            ),
    ) {
        KubunoCard(modifier = Modifier.fillMaxWidth(), flush = true) {
            Column {
                DocumentPreview(
                    starred = doc.isStarred,
                    selected = selected,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(112.dp),
                )
                Row(
                    Modifier.padding(start = 12.dp, end = 0.dp, top = 8.dp, bottom = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            doc.title.ifBlank { "Sans titre" },
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            rowSubtitle(doc),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Box {
                        IconButton(onClick = onOpenMenu) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "Actions du document")
                        }
                        actions()
                    }
                }
            }
        }
    }
}

/**
 * The menu of a single row.
 *
 * `DocActions` is written by another agent; wrapping the call here means a
 * change to its signature is a one-line fix instead of one per call site.
 */
@Composable
private fun DocumentActionsMenu(
    doc: DocumentSummaryDto,
    expanded: Boolean,
    onDismiss: () -> Unit,
    onOpenDocument: (String) -> Unit,
    onRenameRequest: (DocumentSummaryDto) -> Unit,
    onToggleStar: (String) -> Unit,
    onDuplicate: (String) -> Unit,
    onShare: (String) -> Unit,
    onExport: (String) -> Unit,
    onTrash: (String) -> Unit,
    onRestore: (String) -> Unit,
    onDeleteRequest: (DocumentSummaryDto) -> Unit,
) {
    // Every branch closes the menu first: the action can open a dialog, and a
    // menu left expanded behind it would steal the dialog's dismiss taps.
    DocActions(
        document = doc,
        expanded = expanded,
        onDismiss = onDismiss,
        onOpen = { onDismiss(); onOpenDocument(doc.id) },
        onRename = { onDismiss(); onRenameRequest(doc) },
        onToggleStar = { onDismiss(); onToggleStar(doc.id) },
        onDuplicate = { onDismiss(); onDuplicate(doc.id) },
        onShare = { onDismiss(); onShare(doc.id) },
        onExport = { onDismiss(); onExport(doc.id) },
        onTrash = { onDismiss(); onTrash(doc.id) },
        onRestore = { onDismiss(); onRestore(doc.id) },
        onDeleteForever = { onDismiss(); onDeleteRequest(doc) },
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Templates
// ─────────────────────────────────────────────────────────────────────────────

@Composable
private fun TemplatesTab(
    state: DocsUiState,
    onUseTemplate: (TemplateDto) -> Unit,
    onRefresh: () -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Modèles",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            if (state.templatesLoading) KubunoSpinner(size = KubunoSpinnerSize.SM)
        }

        val error = state.templatesError
        when {
            state.templatesLoading && state.templates.isEmpty() -> LoadingBlock()

            // A failed first load must not read as "this server has no
            // templates": a 401 or a 500 says nothing about the catalogue.
            error != null && state.templates.isEmpty() -> KubunoEmptyState(
                icon = Icons.Filled.Article,
                title = "Chargement impossible",
                modifier = Modifier.fillMaxWidth().padding(24.dp),
                description = error.message,
                tone = KubunoEmptyTone.ERROR,
                actions = {
                    KubunoButton(
                        text = "Réessayer",
                        onClick = onRefresh,
                        variant = KubunoButtonVariant.SECONDARY,
                        size = KubunoButtonSize.SM,
                    )
                },
            )

            state.templates.isEmpty() -> KubunoEmptyState(
                icon = Icons.Filled.Article,
                title = "Aucun modèle",
                modifier = Modifier.fillMaxWidth().padding(24.dp),
                description = "Les modèles enregistrés sur ce serveur apparaîtront ici.",
                tone = KubunoEmptyTone.FIRST_USE,
                actions = {
                    KubunoButton(
                        text = "Actualiser",
                        onClick = onRefresh,
                        variant = KubunoButtonVariant.SECONDARY,
                        size = KubunoButtonSize.SM,
                    )
                },
            )

            else -> LazyVerticalGrid(
                columns = GridCells.Adaptive(minSize = 160.dp),
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = 12.dp, end = 12.dp, bottom = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(state.templates, key = { it.id }) { template ->
                    TemplateCard(
                        template = template,
                        busy = state.busy,
                        onUse = { onUseTemplate(template) },
                    )
                }
            }
        }
    }
}

@Composable
private fun TemplateCard(template: TemplateDto, busy: Boolean, onUse: () -> Unit) {
    KubunoCard(modifier = Modifier.fillMaxWidth(), flush = true) {
        Column {
            TemplatePreview(modifier = Modifier.fillMaxWidth().height(112.dp))
            Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
                Text(
                    template.name.ifBlank { "Modèle" },
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    template.description?.takeIf { it.isNotBlank() } ?: shortDate(template.createdAt),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(8.dp))
                KubunoButton(
                    text = "Utiliser",
                    onClick = onUse,
                    modifier = Modifier.fillMaxWidth(),
                    size = KubunoButtonSize.SM,
                    enabled = !busy,
                )
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Small pieces
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The tile thumbnail: a blank sheet with a few ruled lines, the same suggestion
 * of a page the web uses for its template previews. It is deliberately drawn
 * rather than fetched — the module serves no document thumbnails.
 */
@Composable
private fun DocumentPreview(
    starred: Boolean,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
) {
    Box(
        modifier.background(if (selected) docsSelected() else MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            Icons.Filled.Description,
            contentDescription = null,
            tint = docsTone(),
            modifier = Modifier.size(36.dp),
        )
        if (starred) {
            Icon(
                Icons.Filled.Star,
                contentDescription = "Favori",
                tint = DocsColors.Warning,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(8.dp)
                    .size(16.dp),
            )
        }
    }
}

/**
 * The template thumbnail, transcribed from the web card (DocumentsApp.tsx): a
 * sheet three quarters as wide as the frame, on `surface-1`, with ruled lines
 * painted in `surface-2` — the faint tint of the web, not a border colour — and
 * the module's own 8 px radius.
 */
@Composable
private fun TemplatePreview(modifier: Modifier = Modifier) {
    Box(
        modifier.background(MaterialTheme.colorScheme.surfaceContainerLow),
        contentAlignment = Alignment.Center,
    ) {
        val rule = MaterialTheme.colorScheme.surfaceVariant
        // The web dims every line but the first to half opacity.
        val faint = rule.copy(alpha = 0.5f)
        Surface(
            shape = DocsShape.Card,
            color = docsPageSurface(),
            modifier = Modifier
                .fillMaxWidth(0.75f)
                .fillMaxHeight(0.8f),
        ) {
            Column(
                Modifier.padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Box(Modifier.fillMaxWidth(0.6f).height(5.dp).background(rule))
                Box(Modifier.fillMaxWidth().height(3.dp).background(faint))
                Box(Modifier.fillMaxWidth(0.833f).height(3.dp).background(faint))
                Box(Modifier.fillMaxWidth().height(3.dp).background(faint))
                Box(Modifier.fillMaxWidth(0.666f).height(3.dp).background(faint))
            }
        }
    }
}

@Composable
private fun DocumentGlyph(starred: Boolean) {
    Box(contentAlignment = Alignment.Center) {
        Icon(
            Icons.Filled.Description,
            contentDescription = null,
            tint = docsTone(),
            modifier = Modifier.size(24.dp),
        )
        if (starred) {
            Icon(
                Icons.Filled.Star,
                contentDescription = "Favori",
                tint = DocsColors.Warning,
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .size(10.dp),
            )
        }
    }
}

@Composable
private fun LoadingBlock() {
    Box(
        Modifier
            .fillMaxWidth()
            .height(160.dp),
        contentAlignment = Alignment.Center,
    ) {
        KubunoSpinner()
    }
}

/**
 * The paging footer. Composing it is what asks for the next page — the listing
 * only ever reaches it by scrolling to the end — and the view model ignores the
 * call when nothing is left, so no extra guard is needed here.
 */
@Composable
private fun LoadMoreFooter(loading: Boolean, onLoadMore: () -> Unit) {
    LaunchedEffect(Unit) { onLoadMore() }
    Box(
        Modifier
            .fillMaxWidth()
            .height(56.dp),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            KubunoSpinner(size = KubunoSpinnerSize.SM)
        } else {
            KubunoButton(
                text = "Charger plus",
                onClick = onLoadMore,
                variant = KubunoButtonVariant.TEXT,
                size = KubunoButtonSize.SM,
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Labels & formatting
// ─────────────────────────────────────────────────────────────────────────────

private val TAB_LABELS: List<KubunoTab> = listOf(
    KubunoTab(DocsTab.RECENTS.name, "Récents"),
    KubunoTab(DocsTab.BROWSE.name, "Parcourir"),
    KubunoTab(DocsTab.TEMPLATES.name, "Modèles"),
)

private val FILTER_LABELS: List<Pair<DocsFilter, String>> = listOf(
    DocsFilter.ALL to "Tous",
    DocsFilter.STARRED to "Favoris",
    DocsFilter.SHARED to "Partagés",
    DocsFilter.TRASH to "Corbeille",
)

private val SORT_LABELS: List<Pair<DocsSort, String>> = listOf(
    DocsSort.RECENT to "Modifiés récemment",
    DocsSort.TITLE to "Titre (A → Z)",
    DocsSort.CREATED to "Date de création",
)

/** "12 mars" — the compact form the web's recents column uses. */
private val SHORT_DATE: DateTimeFormatter = DateTimeFormatter.ofPattern("d MMM", Locale.FRENCH)

private fun shortDate(raw: String?): String {
    val value = raw?.takeIf { it.isNotBlank() } ?: return ""
    // The module serves RFC 3339 with an offset, but a bare `Z` instant shows up
    // on rows written by older versions — both are accepted rather than blanking
    // the date.
    val instant = runCatching { OffsetDateTime.parse(value).toInstant() }
        .recoverCatching { Instant.parse(value) }
        .getOrNull() ?: return ""
    return SHORT_DATE.format(instant.atZone(ZoneId.systemDefault()))
}

/** Modification date, plus the word count when the server knows one. */
private fun rowSubtitle(doc: DocumentSummaryDto): String {
    val date = shortDate(doc.updatedAt)
    val words = if (doc.wordCount > 0) "${doc.wordCount} mot${if (doc.wordCount > 1) "s" else ""}" else ""
    return listOf(date, words).filter { it.isNotEmpty() }.joinToString(" · ")
}

/** True when the active tab already shows rows, i.e. an error is only a banner. */
private fun hasRowsFor(state: DocsUiState): Boolean = when (state.tab) {
    DocsTab.RECENTS -> state.recents.isNotEmpty()
    DocsTab.BROWSE -> state.documents.isNotEmpty()
    DocsTab.TEMPLATES -> state.templates.isNotEmpty()
}
