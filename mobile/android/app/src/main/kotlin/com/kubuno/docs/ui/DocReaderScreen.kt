package com.kubuno.docs.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.FileDownload
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kubuno.android.account.SharedAccount
import com.kubuno.android.ui.components.KubunoBadge
import com.kubuno.android.ui.components.KubunoBadgeVariant
import com.kubuno.android.ui.components.KubunoButton
import com.kubuno.android.ui.components.KubunoButtonSize
import com.kubuno.android.ui.components.KubunoButtonVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoConfirmDialog
import com.kubuno.android.ui.components.KubunoEmptyState
import com.kubuno.android.ui.components.KubunoEmptyTone
import com.kubuno.android.ui.components.KubunoProgressBar
import com.kubuno.android.ui.components.KubunoPromptDialog
import com.kubuno.android.ui.components.KubunoSeparator
import com.kubuno.android.ui.components.KubunoSpinner
import com.kubuno.android.ui.components.KubunoSpinnerSize
import com.kubuno.docs.pm.PmDoc
import com.kubuno.docs.pm.PmNode
import com.kubuno.docs.pm.PmPageNumberFormat
import com.kubuno.docs.pm.PmPageNumbers
import com.kubuno.docs.pm.plainText
import com.kubuno.docs.ui.render.LocalDocsAccount
import com.kubuno.docs.ui.render.PmBlock

/**
 * The reading surface of an open document.
 *
 * It is a full-screen overlay, not a navigation destination: the app keeps a
 * single state object and swaps this in whenever `openDoc` is non-null, exactly
 * like the photo viewer of the photos app.
 *
 * What it renders is the reflowed body — the phone deliberately drops the
 * paginated sheet (see [DocsPage]), so the page-anchored artefacts that only
 * exist because of pagination (running header, running footer, automatic page
 * numbers) are surfaced once, labelled, instead of being painted on every page.
 */
@Composable
fun DocReaderScreen(
    state: DocsUiState,
    onClose: () -> Unit,
    onEdit: () -> Unit,
    onRetry: () -> Unit,
    onRename: (String, String) -> Unit,
    onToggleStar: (String) -> Unit,
    onDuplicate: (String) -> Unit,
    onTrash: (String) -> Unit,
    onExport: (DocsExportFormat) -> Unit,
    modifier: Modifier = Modifier,
) {
    val open = state.openDoc
    val account = state.account

    var renaming by remember { mutableStateOf(false) }
    var confirmingTrash by remember { mutableStateOf(false) }

    Column(
        modifier
            .fillMaxSize()
            .background(docsPageBackdrop())
            .navigationBarsPadding(),
    ) {
        DocReaderBar(
            title = open?.document?.title.orEmpty(),
            emoji = open?.document?.icon,
            starred = open?.document?.isStarred == true,
            hasDocument = open != null,
            onClose = onClose,
            onRenameRequest = { renaming = true },
            onToggleStar = { open?.let { onToggleStar(it.document.id) } },
            onEdit = onEdit,
            onDuplicate = { open?.let { onDuplicate(it.document.id) } },
            onExport = onExport,
            onTrashRequest = { confirmingTrash = true },
        )

        // An export is a plain download of several megabytes: an indeterminate
        // bar under the bar is enough feedback and never moves the reading area.
        if (state.exporting) {
            KubunoProgressBar(
                progress = null,
                modifier = Modifier.fillMaxWidth(),
                showValue = false,
            )
        }

        if (open != null) {
            DocReaderMeta(
                wordCount = open.document.wordCount,
                sourceFormat = open.document.sourceFormat,
                trashed = open.document.isTrashed,
                editing = state.editing,
            )
        }

        Box(Modifier.fillMaxWidth().weight(1f)) {
            when {
                open == null && state.opening -> Box(
                    Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) { KubunoSpinner(size = KubunoSpinnerSize.LG) }

                open == null -> DocReaderFailure(error = state.error, onRetry = onRetry)

                account == null -> KubunoCallout(
                    text = "Aucun compte Kubuno sur cet appareil.",
                    modifier = Modifier.padding(16.dp),
                    variant = KubunoCalloutVariant.DANGER,
                )

                open.doc.isEmpty -> KubunoEmptyState(
                    icon = Icons.Outlined.Description,
                    title = "Document vide",
                    modifier = Modifier.align(Alignment.Center),
                    description = "Ce document ne contient encore aucun texte.",
                    tone = KubunoEmptyTone.FIRST_USE,
                    actions = {
                        KubunoButton(
                            text = "Modifier",
                            onClick = onEdit,
                            size = KubunoButtonSize.SM,
                        )
                    },
                )

                else -> DocReaderBody(account = account, doc = open.doc)
            }
        }
    }

    if (renaming && open != null) {
        KubunoPromptDialog(
            title = "Renommer le document",
            onConfirm = { value ->
                renaming = false
                onRename(open.document.id, value)
            },
            onDismiss = { renaming = false },
            label = "Titre",
            placeholder = "Document sans titre",
            initialValue = open.document.title,
        )
    }

    if (confirmingTrash && open != null) {
        KubunoConfirmDialog(
            title = "Mettre à la corbeille ?",
            message = "Le document sera retiré de vos listes. Vous pourrez le restaurer depuis la corbeille.",
            onConfirm = {
                confirmingTrash = false
                onTrash(open.document.id)
            },
            onDismiss = { confirmingTrash = false },
            confirmText = "Mettre à la corbeille",
            danger = true,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Top bar
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The tone-coloured bar of the reader. The office editor dresses its own top bar
 * with the per-editor tone, and keeping it here is what tells the user, at a
 * glance, that this screen belongs to the documents editor and not to the list.
 */
@Composable
private fun DocReaderBar(
    title: String,
    emoji: String?,
    starred: Boolean,
    hasDocument: Boolean,
    onClose: () -> Unit,
    onRenameRequest: () -> Unit,
    onToggleStar: () -> Unit,
    onEdit: () -> Unit,
    onDuplicate: () -> Unit,
    onExport: (DocsExportFormat) -> Unit,
    onTrashRequest: () -> Unit,
) {
    val tone = docsTone()
    var menuOpen by remember { mutableStateOf(false) }

    Surface(color = tone, contentColor = DocsColors.OnTone) {
        Row(
            Modifier
                .statusBarsPadding()
                .fillMaxWidth()
                .padding(horizontal = 4.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onClose) {
                Icon(
                    Icons.Filled.Close,
                    contentDescription = "Fermer le document",
                    tint = DocsColors.OnTone,
                )
            }

            Row(
                Modifier
                    .weight(1f)
                    .clickable(enabled = hasDocument, onClick = onRenameRequest)
                    .padding(horizontal = 4.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                // `icon` is the emoji the web puts in front of the title; it is
                // text, never a vector, so it is rendered as such.
                if (!emoji.isNullOrBlank()) {
                    Text(emoji, style = MaterialTheme.typography.bodyLarge, color = DocsColors.OnTone)
                }
                Text(
                    text = title.ifBlank { "Document sans titre" },
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Medium,
                    color = DocsColors.OnTone,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            if (hasDocument) {
                IconButton(onClick = onToggleStar) {
                    Icon(
                        if (starred) Icons.Filled.Star else Icons.Filled.StarBorder,
                        contentDescription = if (starred) "Retirer des favoris" else "Ajouter aux favoris",
                        tint = DocsColors.OnTone,
                    )
                }
                IconButton(onClick = onEdit) {
                    Icon(
                        Icons.Filled.Edit,
                        contentDescription = "Modifier le document",
                        tint = DocsColors.OnTone,
                    )
                }
                Box {
                    IconButton(onClick = { menuOpen = true }) {
                        Icon(
                            Icons.Filled.MoreVert,
                            contentDescription = "Actions du document",
                            tint = DocsColors.OnTone,
                        )
                    }
                    DocumentMenu(
                        expanded = menuOpen,
                        starred = starred,
                        onDismiss = { menuOpen = false },
                        onRenameRequest = onRenameRequest,
                        onToggleStar = onToggleStar,
                        onDuplicate = onDuplicate,
                        onExport = onExport,
                        onTrashRequest = onTrashRequest,
                    )
                }
            }
        }
    }
}

@Composable
private fun DocumentMenu(
    expanded: Boolean,
    starred: Boolean,
    onDismiss: () -> Unit,
    onRenameRequest: () -> Unit,
    onToggleStar: () -> Unit,
    onDuplicate: () -> Unit,
    onExport: (DocsExportFormat) -> Unit,
    onTrashRequest: () -> Unit,
) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        DropdownMenuItem(
            text = { Text("Renommer") },
            onClick = { onDismiss(); onRenameRequest() },
            leadingIcon = { Icon(Icons.Outlined.Edit, contentDescription = null) },
        )
        DropdownMenuItem(
            text = { Text(if (starred) "Retirer des favoris" else "Ajouter aux favoris") },
            onClick = { onDismiss(); onToggleStar() },
            leadingIcon = {
                Icon(
                    if (starred) Icons.Filled.StarBorder else Icons.Filled.Star,
                    contentDescription = null,
                )
            },
        )
        DropdownMenuItem(
            text = { Text("Dupliquer") },
            onClick = { onDismiss(); onDuplicate() },
            leadingIcon = { Icon(Icons.Filled.ContentCopy, contentDescription = null) },
        )
        KubunoSeparator(Modifier.padding(vertical = 4.dp))
        DropdownMenuItem(
            text = { Text("Exporter en .docx") },
            onClick = { onDismiss(); onExport(DocsExportFormat.DOCX) },
            leadingIcon = { Icon(Icons.Filled.FileDownload, contentDescription = null) },
        )
        DropdownMenuItem(
            text = { Text("Exporter en .odt") },
            onClick = { onDismiss(); onExport(DocsExportFormat.ODT) },
            leadingIcon = { Icon(Icons.Filled.FileDownload, contentDescription = null) },
        )
        KubunoSeparator(Modifier.padding(vertical = 4.dp))
        DropdownMenuItem(
            text = { Text("Mettre à la corbeille", color = MaterialTheme.colorScheme.error) },
            onClick = { onDismiss(); onTrashRequest() },
            leadingIcon = {
                Icon(
                    Icons.Filled.Delete,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.error,
                )
            },
        )
    }
}

/** The quiet strip of facts under the bar: size of the document and its state. */
@Composable
private fun DocReaderMeta(
    wordCount: Int,
    sourceFormat: String?,
    trashed: Boolean,
    editing: Boolean,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(docsHover())
            .padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = if (wordCount <= 1) "$wordCount mot" else "$wordCount mots",
            style = DocsType.Note,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (!sourceFormat.isNullOrBlank()) {
            KubunoBadge(text = ".${sourceFormat.lowercase()}", variant = KubunoBadgeVariant.NEUTRAL)
        }
        if (trashed) {
            KubunoBadge(text = "Corbeille", variant = KubunoBadgeVariant.DANGER)
        }
        if (editing) {
            KubunoBadge(text = "Session d'édition", variant = KubunoBadgeVariant.PRIMARY, dot = true)
        }
    }
}

/** Nothing could be opened: say why, and offer the single useful action. */
@Composable
private fun DocReaderFailure(error: DocsError?, onRetry: () -> Unit) {
    Column(
        Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        KubunoCallout(
            text = error?.message ?: "Ce document n'a pas pu être ouvert.",
            variant = KubunoCalloutVariant.DANGER,
            title = "Ouverture impossible",
        )
        KubunoButton(
            text = "Réessayer",
            onClick = onRetry,
            variant = KubunoButtonVariant.SECONDARY,
            size = KubunoButtonSize.SM,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Body
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The reflowed body.
 *
 * The block list is driven by a LazyColumn rather than a scrolling Column on
 * purpose: an imported document routinely carries a few thousand top-level
 * nodes, and a Column composes and measures every one of them before the first
 * frame — several seconds of blank screen, and an allocation per node that never
 * comes back. The LazyColumn keeps only the visible window alive, so opening a
 * long document costs the same as opening a short one. Nesting stays inside one
 * block (a table, a list), which is bounded and therefore safe to lay out
 * eagerly.
 */
@Composable
private fun DocReaderBody(account: SharedAccount, doc: PmDoc) {
    val listState = rememberLazyListState()

    // The block renderer takes no account parameter: image nodes pick the account
    // up from this composition local, so it has to be provided once around the
    // whole body rather than threaded through every block.
    CompositionLocalProvider(LocalDocsAccount provides account) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= DocsPage.WideWindowBreakpoint
        val gutter = if (wide) DocsPage.ColumnPaddingWide else DocsPage.ColumnPaddingCompact

        Surface(
            modifier = Modifier
                // widthIn comes FIRST: it narrows the constraints that fillMaxSize
                // then fills, which is what centres the sheet on a tablet instead
                // of stretching it to an unreadable line length.
                .widthIn(max = DocsPage.MaxColumnWidth + gutter * 2)
                .fillMaxSize()
                .align(Alignment.TopCenter),
            color = docsPageSurface(),
            shape = if (wide) DocsShape.Card else DocsShape.Page,
            shadowElevation = if (wide) DocsPage.PageElevation else 0.dp,
        ) {
            LazyColumn(
                state = listState,
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(
                    start = gutter,
                    end = gutter,
                    top = 20.dp,
                    bottom = 64.dp,
                ),
            ) {
                val header = doc.header.takeIf { it.hasText() }
                val headerEven = doc.headerEven?.takeIf { doc.evenOdd && it.hasText() }
                val footer = doc.footer.takeIf { it.hasText() }
                val footerEven = doc.footerEven?.takeIf { doc.evenOdd && it.hasText() }

                if (header != null) {
                    item(key = "header") {
                        RunningBand(
                            label = if (headerEven != null) "En-tête (pages impaires)" else "En-tête",
                            nodes = header,
                            below = true,
                        )
                    }
                }
                if (headerEven != null) {
                    item(key = "header-even") {
                        RunningBand(
                            label = "En-tête (pages paires)",
                            nodes = headerEven,
                            below = true,
                        )
                    }
                }

                itemsIndexed(
                    items = doc.blocks,
                    key = { index, _ -> "block-$index" },
                ) { _, node ->
                    PmBlock(
                        node = node,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }

                if (footer != null) {
                    item(key = "footer") {
                        RunningBand(
                            label = if (footerEven != null) "Pied de page (pages impaires)" else "Pied de page",
                            nodes = footer,
                            below = false,
                        )
                    }
                }
                if (footerEven != null) {
                    item(key = "footer-even") {
                        RunningBand(
                            label = "Pied de page (pages paires)",
                            nodes = footerEven,
                            below = false,
                        )
                    }
                }

                if (doc.pageNumbers != PmPageNumbers.NONE) {
                    item(key = "page-numbers") {
                        PageNumberNote(
                            position = doc.pageNumbers,
                            format = doc.pageNumberFormat,
                            start = doc.pageNumberStart,
                        )
                    }
                }
            }
        }
    }
    }
}

/** True when a running header/footer carries anything worth showing. */
private fun List<PmNode>.hasText(): Boolean = any { it.plainText().isNotBlank() }

/**
 * A running header or footer, shown once at the matching end of the reflowed
 * column. It is labelled and dimmed so it reads as document furniture rather
 * than as the first (or last) paragraph of the body, which is the only real risk
 * of surfacing it outside a paginated sheet.
 */
@Composable
private fun RunningBand(
    label: String,
    nodes: List<PmNode>,
    below: Boolean,
) {
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        if (!below) KubunoSeparator(Modifier.padding(bottom = 8.dp))
        Text(
            text = label,
            style = DocsType.Note,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(bottom = 4.dp),
        )
        Column(Modifier.fillMaxWidth().alpha(0.7f)) {
            nodes.forEach { node ->
                PmBlock(node = node, modifier = Modifier.fillMaxWidth())
            }
        }
        if (below) KubunoSeparator(Modifier.padding(top = 8.dp))
    }
}

/**
 * The automatic page numbering, stated instead of painted: the phone reflows the
 * document into one fluid column, so there is no page to put a number on, and
 * silently dropping the setting would hide a real property of the document.
 */
@Composable
private fun PageNumberNote(
    position: PmPageNumbers,
    format: PmPageNumberFormat,
    start: Int,
) {
    val where = when (position) {
        PmPageNumbers.FOOTER_RIGHT -> "en pied de page, à droite"
        PmPageNumbers.FOOTER_CENTER -> "en pied de page, centrée"
        PmPageNumbers.HEADER_RIGHT -> "en en-tête, à droite"
        PmPageNumbers.HEADER_CENTER -> "en en-tête, centrée"
        PmPageNumbers.NONE -> return
    }
    val sample = when (format) {
        PmPageNumberFormat.ARABIC -> "1, 2, 3"
        PmPageNumberFormat.ROMAN_LOWER -> "i, ii, iii"
        PmPageNumberFormat.ROMAN_UPPER -> "I, II, III"
        PmPageNumberFormat.ALPHA_LOWER -> "a, b, c"
        PmPageNumberFormat.ALPHA_UPPER -> "A, B, C"
    }
    Column(Modifier.fillMaxWidth().padding(top = 12.dp)) {
        KubunoSeparator()
        Text(
            text = "Numérotation $where ($sample), à partir de $start.",
            style = DocsType.Note,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 8.dp),
        )
        Text(
            text = "Les pages ne sont pas reproduites sur mobile : le texte est remis en colonne continue.",
            style = DocsType.Note,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier
                .padding(top = 2.dp)
                .alpha(0.8f),
        )
    }
}
