package com.kubuno.docs.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.FormatBold
import androidx.compose.material.icons.filled.FormatItalic
import androidx.compose.material.icons.filled.FormatListNumbered
import androidx.compose.material.icons.filled.FormatUnderlined
import androidx.compose.material.icons.filled.Save
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.kubuno.android.ui.components.KubunoButton
import com.kubuno.android.ui.components.KubunoButtonSize
import com.kubuno.android.ui.components.KubunoButtonVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoChip
import com.kubuno.android.ui.components.KubunoConfirmDialog
import com.kubuno.android.ui.components.KubunoSeparator
import com.kubuno.android.ui.components.KubunoSpinner
import com.kubuno.android.ui.components.KubunoSpinnerSize
import com.kubuno.android.ui.theme.KubunoTheme
import com.kubuno.docs.pm.PmAtomRole
import com.kubuno.docs.pm.PmDoc
import com.kubuno.docs.pm.PmInline
import com.kubuno.docs.pm.PmMark
import com.kubuno.docs.pm.PmNode
import com.kubuno.docs.pm.plainText
import com.kubuno.docs.ui.render.PmBlock
import com.kubuno.docs.ui.render.PmBlocks
import com.kubuno.docs.ui.render.PmText
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull

/**
 * The document editing surface — the assumed v1, not the web ribbon.
 *
 * WHAT IT DOES: edit the text of paragraphs, headings and list items; toggle
 * bold / italic / underline; turn a block into a bulleted or numbered list and
 * back; change the heading level of a block.
 *
 * WHAT IT DOES NOT DO, so the next developer stops looking for it:
 *  - no tables, images, shapes, footnotes/endnotes, comments, track changes,
 *    sections, headers/footers, page numbers or page geometry: those blocks are
 *    rendered READ-ONLY and are never rewritten. A PARAGRAPH is read-only too as
 *    soon as it holds an inline atom (a note call and its text, an inline
 *    picture, a Word field): the text field carries one flat string, and writing
 *    that string back would replace the atom with characters — see
 *    `isRewritable`;
 *  - no text-run selection: there is no caret model here, so a mark toggle
 *    applies to the WHOLE focused block, and retyping a mixed-format block
 *    collapses it to the formatting of its first run (the same trade-off
 *    `DocsViewModel.applyEdit(blockIndex, text)` documents);
 *  - no block splitting or merging: Enter inserts a line break (a `hardBreak`
 *    node) inside the current block instead of creating a new paragraph;
 *  - no nested lists: a list item holding more than one block, or a non-text
 *    block, is shown read-only rather than flattened into an editable field,
 *    because writing it back would delete what the field cannot show;
 *  - no colour, font, size, alignment, indent or line-spacing controls; the
 *    values already stored in the document are honoured on render.
 *
 * Every edit goes out as a whole ProseMirror body through
 * [DocsViewModel.applyEdit] (the `JsonElement` overload): the raw JSON is
 * patched in place — never re-serialized from [PmDoc], which is a rendering
 * model and would silently drop everything it does not paint.
 */
@Composable
fun DocEditorScreen(
    state: DocsUiState,
    viewModel: DocsViewModel,
    modifier: Modifier = Modifier,
    onClose: () -> Unit = viewModel::closeDocument,
) {
    val open = state.openDoc
    val scheme = MaterialTheme.colorScheme

    // The focused editable run. Reset whenever another document is opened: the
    // index it holds only means something inside one body.
    var focus by remember(open?.document?.id) { mutableStateOf<DocBlockPath?>(null) }
    var discardPrompt by remember(open?.document?.id) { mutableStateOf(false) }

    /** Rewrites the node [path] addresses and hands the new body to the ViewModel. */
    fun editAt(path: DocBlockPath, transform: (JsonObject) -> JsonObject?) {
        val root = open?.contentJson ?: return
        editTarget(root, path, transform)?.let { viewModel.applyEdit(it) }
    }

    /** Same, for the focused node. */
    fun editFocused(transform: (JsonObject) -> JsonObject?) {
        editAt(focus ?: return, transform)
    }

    /** Rewrites a TOP-LEVEL block, which the transform may turn into several. */
    fun editTopLevel(blockIndex: Int, transform: (JsonObject) -> List<JsonObject>?) {
        val root = open?.contentJson ?: return
        editBlock(root, blockIndex, transform)?.let { viewModel.applyEdit(it) }
    }

    Column(
        modifier
            .fillMaxSize()
            .background(scheme.surface)
            // No statusBarsPadding here: the top bar carries it itself, so the
            // tone reaches behind the status bar exactly like the reader's.
            .imePadding()
            .navigationBarsPadding(),
    ) {
        EditorTopBar(
            title = open?.document?.title.orEmpty(),
            state = state,
            onClose = onClose,
            onSave = viewModel::save,
            onStartEditing = viewModel::startEditing,
        )

        val saveError = state.saveError
        if (saveError != null && state.conflict == null) {
            Column(
                Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                KubunoCallout(
                    text = saveError.message,
                    variant = KubunoCalloutVariant.DANGER,
                    title = "Enregistrement impossible",
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    KubunoButton(
                        text = "Réessayer",
                        onClick = viewModel::save,
                        variant = KubunoButtonVariant.SECONDARY,
                        size = KubunoButtonSize.SM,
                    )
                    KubunoButton(
                        text = "Masquer",
                        onClick = viewModel::saveErrorConsumed,
                        variant = KubunoButtonVariant.GHOST,
                        size = KubunoButtonSize.SM,
                    )
                }
            }
        }

        if (open == null) {
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.Center,
            ) {
                KubunoSpinner(size = KubunoSpinnerSize.MD)
            }
            return@Column
        }

        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .background(docsPageSurface())
                .verticalScroll(rememberScrollState())
                .padding(
                    horizontal = DocsPage.ColumnPaddingCompact,
                    vertical = 16.dp,
                ),
        ) {
            DocumentBlocks(
                doc = open.doc,
                editing = state.editing,
                onFocus = { focus = it },
                // Addressed by the field's own path, not by `focus`: the two are
                // the same in practice, but a keystroke must never be written to
                // whatever block the toolbar happens to point at.
                onText = { path, text -> editAt(path) { node -> withText(node, text) } },
            )
            // Bottom breathing room so the last block is not glued to the toolbar.
            Spacer(Modifier.height(48.dp))
        }

        val active = focus
        if (state.editing && active != null) {
            val focusedBlock = open.doc.textBlockAt(active)
            EditorToolbar(
                marks = focusedBlock?.activeMarks() ?: emptySet(),
                listKind = open.doc.listKindAt(active),
                headingLevel = (focusedBlock as? PmNode.Heading)?.level,
                // Heading levels are meaningless inside a list item in this v1:
                // the chips are hidden rather than shown as dead controls.
                showHeadings = active.itemIndex == null,
                onToggleMark = { mark -> editFocused { node -> withMarkToggled(node, mark) } },
                onToggleList = { kind ->
                    val wasKind = open.doc.listKindAt(active)
                    editTopLevel(active.blockIndex) { node -> toggledList(node, kind) }
                    // The field the user was in is destroyed by the structural
                    // change; point the toolbar at what replaced it — the first
                    // item of the new list, or the block the list unwrapped to.
                    focus = if (wasKind == kind) {
                        DocBlockPath(active.blockIndex)
                    } else {
                        DocBlockPath(active.blockIndex, itemIndex = 0)
                    }
                },
                onHeading = { level -> editFocused { node -> withHeadingLevel(node, level) } },
            )
        }
    }

    // ── 412 arbitration ──────────────────────────────────────────────────────
    // A rejected save is never resolved silently: one of the two bodies has to be
    // dropped, and only the user can say which.
    val conflict = state.conflict
    if (conflict != null) {
        if (discardPrompt) {
            KubunoConfirmDialog(
                title = "Abandonner vos modifications ?",
                message = "Le texte enregistré sur le serveur remplacera le vôtre. " +
                    "Vos modifications non enregistrées seront perdues.",
                onConfirm = {
                    discardPrompt = false
                    viewModel.resolveConflict(keepLocal = false)
                },
                onDismiss = { discardPrompt = false },
                confirmText = "Reprendre celle du serveur",
                cancelText = "Retour",
                danger = true,
            )
        } else {
            KubunoConfirmDialog(
                title = "Document modifié ailleurs",
                message = "Ce document a été enregistré ailleurs pendant que vous le " +
                    "modifiiez. Gardez votre version pour écraser celle du serveur, ou " +
                    "reprenez la version du serveur.",
                onConfirm = { viewModel.resolveConflict(keepLocal = true) },
                // The cancel button and a tap outside both land here on purpose:
                // the destructive branch then asks for its own confirmation.
                onDismiss = { discardPrompt = true },
                confirmText = "Garder ma version",
                cancelText = "Version du serveur",
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Chrome
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The tone-coloured bar of the editor.
 *
 * The tone IS the chrome of the office editor on the web: `WORKSPACE_OFFICE`
 * paints the top bar `--kbn-office-tabstrip` (#1557b0) with white text, and the
 * documents editor mounts `OfficeShell` with exactly that default. Painting this
 * bar on the surface colour made the same document change identity between
 * reading (blue bar) and editing (white bar); the reader's own bar is the
 * reference here.
 */
@Composable
private fun EditorTopBar(
    title: String,
    state: DocsUiState,
    onClose: () -> Unit,
    onSave: () -> Unit,
    onStartEditing: () -> Unit,
) {
    Surface(color = docsTone(), contentColor = DocsColors.OnTone) {
        Row(
            Modifier
                .statusBarsPadding()
                .fillMaxWidth()
                .padding(start = 4.dp, end = 12.dp, top = 4.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            IconButton(onClick = onClose) {
                Icon(
                    Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = "Fermer le document",
                    tint = DocsColors.OnTone,
                )
            }
            Text(
                title.ifBlank { "Document" },
                style = MaterialTheme.typography.titleMedium,
                color = DocsColors.OnTone,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            SaveStatus(state = state, onSave = onSave, onStartEditing = onStartEditing)
        }
    }
}

/**
 * The save state, as the three cases the user can act on.
 *
 * Everything here sits ON the tone, so the ink is [DocsColors.OnTone] and the
 * buttons are SECONDARY (a light chip on the blue bar) — a PRIMARY button would
 * be blue on blue.
 */
@Composable
private fun SaveStatus(state: DocsUiState, onSave: () -> Unit, onStartEditing: () -> Unit) {
    when {
        state.saving -> Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            KubunoSpinner(size = KubunoSpinnerSize.SM)
            Text(
                "Enregistrement…",
                style = MaterialTheme.typography.bodySmall,
                color = DocsColors.OnTone,
            )
        }

        state.dirty -> KubunoButton(
            text = "Enregistrer",
            onClick = onSave,
            variant = KubunoButtonVariant.SECONDARY,
            size = KubunoButtonSize.SM,
            icon = { Icon(Icons.Filled.Save, contentDescription = null, modifier = Modifier.size(16.dp)) },
        )

        state.editing -> Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(
                Icons.Filled.Check,
                contentDescription = null,
                tint = DocsColors.OnTone,
                modifier = Modifier.size(16.dp),
            )
            Text(
                "À jour",
                style = MaterialTheme.typography.bodySmall,
                color = DocsColors.OnTone,
            )
        }

        else -> KubunoButton(
            text = "Modifier",
            onClick = onStartEditing,
            variant = KubunoButtonVariant.SECONDARY,
            size = KubunoButtonSize.SM,
            icon = { Icon(Icons.Filled.Edit, contentDescription = null, modifier = Modifier.size(16.dp)) },
        )
    }
}

/**
 * The formatting bar, anchored above the keyboard (the screen carries
 * `imePadding`). It offers exactly the actions this v1 implements — no dead
 * control stands in for a feature that is not there.
 */
@Composable
private fun EditorToolbar(
    marks: Set<String>,
    listKind: String?,
    headingLevel: Int?,
    showHeadings: Boolean,
    onToggleMark: (String) -> Unit,
    onToggleList: (String) -> Unit,
    onHeading: (Int?) -> Unit,
) {
    Column(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface)) {
        KubunoSeparator()
        Row(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            KubunoChip(
                label = "Gras",
                selected = MARK_BOLD in marks,
                onClick = { onToggleMark(MARK_BOLD) },
                leadingIcon = { ToolbarIcon(Icons.Filled.FormatBold) },
            )
            KubunoChip(
                label = "Italique",
                selected = MARK_ITALIC in marks,
                onClick = { onToggleMark(MARK_ITALIC) },
                leadingIcon = { ToolbarIcon(Icons.Filled.FormatItalic) },
            )
            KubunoChip(
                label = "Souligné",
                selected = MARK_UNDERLINE in marks,
                onClick = { onToggleMark(MARK_UNDERLINE) },
                leadingIcon = { ToolbarIcon(Icons.Filled.FormatUnderlined) },
            )
            Spacer(Modifier.width(4.dp))
            KubunoChip(
                label = "Puces",
                selected = listKind == BULLET_LIST,
                onClick = { onToggleList(BULLET_LIST) },
                leadingIcon = { ToolbarIcon(Icons.AutoMirrored.Filled.FormatListBulleted) },
            )
            KubunoChip(
                label = "Numéros",
                selected = listKind == ORDERED_LIST,
                onClick = { onToggleList(ORDERED_LIST) },
                leadingIcon = { ToolbarIcon(Icons.Filled.FormatListNumbered) },
            )
        }
        if (showHeadings) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(start = 12.dp, end = 12.dp, bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                KubunoChip(
                    label = "Normal",
                    selected = headingLevel == null,
                    onClick = { onHeading(null) },
                )
                for (level in 1..3) {
                    KubunoChip(
                        label = "Titre $level",
                        selected = headingLevel == level,
                        onClick = { onHeading(level) },
                    )
                }
            }
        }
    }
}

@Composable
private fun ToolbarIcon(icon: ImageVector) {
    Icon(icon, contentDescription = null, modifier = Modifier.size(16.dp))
}

// ─────────────────────────────────────────────────────────────────────────────
// Body
// ─────────────────────────────────────────────────────────────────────────────

/** Addresses one editable run: a top-level block, or one item of a list block. */
private data class DocBlockPath(val blockIndex: Int, val itemIndex: Int? = null)

@Composable
private fun DocumentBlocks(
    doc: PmDoc,
    editing: Boolean,
    onFocus: (DocBlockPath) -> Unit,
    onText: (DocBlockPath, String) -> Unit,
) {
    doc.blocks.forEachIndexed { index, node ->
        key(index) {
            when (node) {
                is PmNode.Heading -> if (node.isRewritable()) {
                    BlockTextField(
                        text = node.plainText(),
                        style = DocsType.heading(node.level).withMarks(node.activeMarks()),
                        editing = editing,
                        path = DocBlockPath(index),
                        onFocus = onFocus,
                        onText = onText,
                        inlines = node.inlines,
                        topSpace = DocsType.spaceBefore(node.level),
                        bottomSpace = DocsType.spaceAfter(node.level),
                    )
                } else {
                    // The reason is only shown while EDITING: outside edit mode the
                    // block is simply being read, and "not editable" is noise.
                    RenderedBlock(label = if (editing) atomReason(node.inlines) else null, node = node)
                }

                is PmNode.Paragraph -> if (node.isRewritable()) {
                    BlockTextField(
                        text = node.plainText(),
                        style = DocsType.Body.withMarks(node.activeMarks()),
                        editing = editing,
                        path = DocBlockPath(index),
                        onFocus = onFocus,
                        onText = onText,
                        inlines = node.inlines,
                        topSpace = DocsType.spaceBefore(0),
                        bottomSpace = DocsType.spaceAfter(0),
                    )
                } else {
                    // The reason is only shown while EDITING: outside edit mode the
                    // block is simply being read, and "not editable" is noise.
                    RenderedBlock(label = if (editing) atomReason(node.inlines) else null, node = node)
                }

                // An unknown block only ever showed the FLATTENED text of a whole
                // subtree; writing that text back would replace the subtree with
                // it. It says what it is on its own, hence no extra label.
                is PmNode.Unknown -> RenderedBlock(label = null, node = node)

                is PmNode.BulletList -> ListBlock(
                    items = node.items,
                    blockIndex = index,
                    marker = { "•" },
                    editing = editing,
                    onFocus = onFocus,
                    onText = onText,
                )

                is PmNode.OrderedList -> ListBlock(
                    items = node.items,
                    blockIndex = index,
                    marker = { i -> "${node.start + i}." },
                    editing = editing,
                    onFocus = onFocus,
                    onText = onText,
                )

                // Task state is not editable in this v1, so the box is a glyph and
                // not a control; the label itself stays editable like any item.
                is PmNode.TaskList -> ListBlock(
                    items = node.items,
                    blockIndex = index,
                    marker = { i -> if (node.items[i].checked == true) "☑" else "☐" },
                    editing = editing,
                    onFocus = onFocus,
                    onText = onText,
                )

                is PmNode.CodeBlock -> ReadOnlyBlock(
                    label = "Bloc de code",
                    body = node.code,
                    style = DocsType.Code,
                )

                is PmNode.Blockquote -> ReadOnlyBlock(
                    label = "Citation",
                    body = node.plainText(),
                    style = DocsType.Body,
                )

                is PmNode.Table -> ReadOnlyBlock(
                    label = "Tableau — non modifiable dans cette version",
                    body = node.plainText(),
                    style = DocsType.Body,
                )

                is PmNode.Image -> ReadOnlyBlock(
                    label = "Image — non modifiable dans cette version",
                    body = node.altText.orEmpty(),
                    style = DocsType.Body,
                )

                PmNode.HorizontalRule -> Box(Modifier.padding(vertical = DocsType.BlockSpacing)) {
                    KubunoSeparator()
                }

                PmNode.PageBreak -> BreakMarker("Saut de page")
                is PmNode.SectionBreak -> BreakMarker("Saut de section")

                // Rows and cells never appear at top level; the parser only emits
                // them inside a table, which is rendered above.
                is PmNode.TableRow, is PmNode.TableCell, is PmNode.ListItem -> ReadOnlyBlock(
                    label = "Contenu non modifiable dans cette version",
                    body = node.plainText(),
                    style = DocsType.Body,
                )
            }
        }
    }
}

@Composable
private fun ListBlock(
    items: List<PmNode.ListItem>,
    blockIndex: Int,
    marker: (Int) -> String,
    editing: Boolean,
    onFocus: (DocBlockPath) -> Unit,
    onText: (DocBlockPath, String) -> Unit,
) {
    Column(Modifier.padding(vertical = DocsType.spaceAfter(0))) {
        items.forEachIndexed { itemIndex, item ->
            key(itemIndex) {
                val only = item.children.singleOrNull()
                Row(
                    Modifier.fillMaxWidth().padding(start = DocsPage.ListIndent),
                    verticalAlignment = Alignment.Top,
                ) {
                    Text(
                        marker(itemIndex),
                        style = DocsType.Body.copy(color = docsPageInk()),
                        modifier = Modifier.widthIn(min = 24.dp),
                    )
                    if (only is PmNode.TextBlock && only.isRewritable()) {
                        BlockTextField(
                            text = only.plainText(),
                            style = DocsType.Body.withMarks(only.activeMarks()),
                            editing = editing,
                            path = DocBlockPath(blockIndex, itemIndex),
                            onFocus = onFocus,
                            onText = onText,
                            inlines = only.inlines,
                            modifier = Modifier.weight(1f),
                        )
                    } else {
                        // More than one block, a nested list, or an inline atom the
                        // field cannot carry: writing it back through a single text
                        // field would delete what the field does not show, so the
                        // item is read-only — rendered by the shared renderer, so
                        // its notes and pictures stay visible.
                        PmBlocks(item.children, modifier = Modifier.weight(1f))
                    }
                }
            }
        }
    }
}

/**
 * One editable run.
 *
 * Outside editing mode it renders the document's own inline formatting; inside
 * it, the field carries the block's common formatting only — see the file header
 * for why there is no per-run editing here.
 */
@Composable
private fun BlockTextField(
    text: String,
    style: TextStyle,
    editing: Boolean,
    path: DocBlockPath,
    onFocus: (DocBlockPath) -> Unit,
    onText: (DocBlockPath, String) -> Unit,
    inlines: List<PmInline>,
    modifier: Modifier = Modifier,
    topSpace: Dp = 0.dp,
    bottomSpace: Dp = 2.dp,
) {
    val ink = docsPageInk()
    if (!editing) {
        // The shared inline engine, not a local one: a run rendered here and the
        // same run rendered by the reader must not come out differently.
        PmText(
            inlines = inlines,
            style = style.copy(color = ink),
            modifier = modifier
                .fillMaxWidth()
                .padding(top = topSpace, bottom = bottomSpace),
            color = ink,
        )
        return
    }

    var focused by remember { mutableStateOf(false) }
    var value by remember(path.blockIndex, path.itemIndex) { mutableStateOf(TextFieldValue(text)) }
    // The document is the source of truth, but re-pushing it into a field that
    // has the caret would fight the user on every keystroke, so the field only
    // catches up while it is idle.
    LaunchedEffect(text, focused) {
        if (!focused && text != value.text) value = TextFieldValue(text)
    }

    BasicTextField(
        value = value,
        onValueChange = {
            value = it
            onText(path, it.text)
        },
        textStyle = style.copy(color = ink),
        cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
        modifier = modifier
            .fillMaxWidth()
            .padding(top = topSpace, bottom = bottomSpace)
            .onFocusChanged { focusState ->
                focused = focusState.isFocused
                if (focusState.isFocused) onFocus(path)
            },
    )
}

/**
 * A block the editor will not rewrite, drawn by the shared renderer so that
 * everything it holds — a note and its text, a picture, a field — stays visible.
 * [label] says WHY it cannot be edited; it is null for a block that already
 * explains itself.
 */
@Composable
private fun RenderedBlock(label: String?, node: PmNode) {
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (label != null) {
            Text(
                label,
                style = MaterialTheme.typography.bodySmall,
                color = KubunoTheme.colors.textTertiary,
            )
        }
        PmBlock(node)
    }
}

@Composable
private fun ReadOnlyBlock(label: String, body: String, style: TextStyle) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = DocsType.BlockSpacing),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodySmall,
            color = KubunoTheme.colors.textTertiary,
        )
        if (body.isNotBlank()) {
            Text(body, style = style.copy(color = docsPageInk()))
        }
    }
}

@Composable
private fun BreakMarker(label: String) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = DocsType.BlockSpacing),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.weight(1f)) { KubunoSeparator() }
        Text(
            label,
            style = MaterialTheme.typography.bodySmall,
            color = KubunoTheme.colors.textTertiary,
        )
        Box(Modifier.weight(1f)) { KubunoSeparator() }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering helpers
// ─────────────────────────────────────────────────────────────────────────────

private const val MARK_BOLD = "bold"
private const val MARK_ITALIC = "italic"
private const val MARK_UNDERLINE = "underline"

private const val BULLET_LIST = "bulletList"
private const val ORDERED_LIST = "orderedList"
private const val TASK_LIST = "taskList"
private const val LIST_ITEM = "listItem"

/**
 * True when the WHOLE content of the block can be put back from the text field.
 *
 * The field holds one flat string and [withText] rebuilds the node's content from
 * it, so a block is only rewritable when its content IS that string:
 *  - a block holding an inline ATOM is not. A footnote or endnote call carries
 *    the text of the note in an attribute, an `inlineImage` carries a picture, a
 *    `field` carries a Word field: none of that survives a round trip through a
 *    string, and the user would lose — on the server — something he never saw on
 *    screen. Such a block is shown READ-ONLY with its reason, which is the only
 *    honest option until this editor has a run-level caret model;
 *  - an UNKNOWN block is not either: the model only carries the FLATTENED text of
 *    its subtree, so rewriting it would replace the subtree with that text.
 *
 * [withText] keeps a second, defensive guard for the same atoms; this one is what
 * stops the editor from OFFERING an edit that cannot be honoured.
 */
private fun PmNode.TextBlock.isRewritable(): Boolean =
    this !is PmNode.Unknown && inlines.none { it is PmInline.Atom }

/** Why a block holding atoms is not editable, named after what it actually holds. */
private fun atomReason(inlines: List<PmInline>): String {
    val roles = inlines.filterIsInstance<PmInline.Atom>().map { it.role }.toSet()
    val what = buildList {
        if (PmAtomRole.FOOTNOTE in roles || PmAtomRole.ENDNOTE in roles) add("une note")
        if (PmAtomRole.IMAGE in roles) add("une image")
        if (PmAtomRole.FIELD in roles) add("un champ")
        if (PmAtomRole.OTHER in roles) add("un élément non pris en charge")
    }
    if (what.isEmpty()) return "Non modifiable dans cette version"
    return "Contient ${what.joinToString(" et ")} — non modifiable dans cette version"
}

/** The marks carried by EVERY text run of the block — what a block toggle shows. */
private fun PmNode.TextBlock.activeMarks(): Set<String> {
    val runs = inlines.filterIsInstance<PmInline.Text>().filter { it.text.isNotBlank() }
    if (runs.isEmpty()) return emptySet()
    val out = mutableSetOf<String>()
    if (runs.all { PmMark.Bold in it.marks }) out += MARK_BOLD
    if (runs.all { PmMark.Italic in it.marks }) out += MARK_ITALIC
    if (runs.all { PmMark.Underline in it.marks }) out += MARK_UNDERLINE
    return out
}

private fun TextStyle.withMarks(marks: Set<String>): TextStyle = copy(
    fontWeight = if (MARK_BOLD in marks) FontWeight.Bold else fontWeight,
    fontStyle = if (MARK_ITALIC in marks) FontStyle.Italic else fontStyle,
    textDecoration = if (MARK_UNDERLINE in marks) TextDecoration.Underline else textDecoration,
)

/** The text block a path points at, list items included. */
private fun PmDoc.textBlockAt(path: DocBlockPath): PmNode.TextBlock? {
    val block = blocks.getOrNull(path.blockIndex) ?: return null
    if (path.itemIndex == null) return block as? PmNode.TextBlock
    val items = itemsOf(block) ?: return null
    return items.getOrNull(path.itemIndex)?.children?.singleOrNull() as? PmNode.TextBlock
}

/** "bulletList"/"orderedList" when the path sits in (or on) a list, else null. */
private fun PmDoc.listKindAt(path: DocBlockPath?): String? =
    when (blocks.getOrNull(path?.blockIndex ?: -1)) {
        is PmNode.BulletList -> BULLET_LIST
        is PmNode.OrderedList -> ORDERED_LIST
        else -> null
    }

private fun itemsOf(block: PmNode): List<PmNode.ListItem>? = when (block) {
    is PmNode.BulletList -> block.items
    is PmNode.OrderedList -> block.items
    is PmNode.TaskList -> block.items
    else -> null
}

// ─────────────────────────────────────────────────────────────────────────────
// ProseMirror body rewrites
// ─────────────────────────────────────────────────────────────────────────────
// The raw JSON is patched in place rather than rebuilt from PmDoc: the parsed
// model is a rendering model and drops what it does not paint (page geometry,
// tables, images, tracked changes), so a round-trip through it would silently
// delete document features. The index walk below mirrors PmParser exactly —
// synthetic page breaks of the multi-page envelope included — so the block the
// renderer shows at position N is the node that gets rewritten.

/** `_type` of the editor's layout envelope around the ProseMirror body. */
private const val MULTI_PAGE = "multi-page"

/**
 * Replaces the top-level block at [index]. [transform] may return several nodes
 * (wrapping a paragraph into a list, unwrapping a list into paragraphs) or null
 * to leave the body untouched. Returns null when nothing changed.
 */
private fun editBlock(
    root: JsonElement,
    index: Int,
    transform: (JsonObject) -> List<JsonObject>?,
): JsonElement? {
    val o = root as? JsonObject ?: return null

    // On-disk envelope `{version, content}`: unwrap, rewrite, put back.
    if (o["_type"].asText() == null && o["type"].asText() == null) {
        val inner = o["content"] as? JsonObject ?: return null
        val rewritten = editBlock(inner, index, transform) ?: return null
        return JsonObject(o.toMutableMap().apply { put("content", rewritten) })
    }

    if (o["_type"].asText() == MULTI_PAGE) {
        val pages = o["pages"] as? JsonArray ?: return null
        // PmParser.parseEnvelope walks EVERY entry of `pages`: it lays the pages
        // end to end with one page break between them as soon as a block has been
        // emitted, and falls back to a single empty paragraph only when the WHOLE
        // envelope yields nothing. Both rules are mirrored here — a page whose
        // body is unreadable contributes no block but still carries the page break
        // the parser emitted, and the fallback paragraph belongs to the first
        // usable page. Diverge on either and an edit lands in the wrong block.
        val fallbackPage = if (pages.none { pageBlockCount(it) > 0 }) firstWritablePage(pages) else -1
        var logical = 0
        var emitted = false
        var hit = false
        val newPages = buildJsonArray {
            pages.forEachIndexed { pageIndex, page ->
                // The page break the parser inserted between two pages.
                if (emitted) logical++
                val pageObject = page as? JsonObject
                val body = pageObject?.get("content") as? JsonObject
                if (pageObject == null || body == null) {
                    add(page)
                    return@forEachIndexed
                }
                val (newBody, next, changed) = editInBody(
                    body = body,
                    start = logical,
                    index = index,
                    synthesizeEmpty = pageIndex == fallbackPage,
                    transform = transform,
                )
                if (next > logical) emitted = true
                logical = next
                if (changed) hit = true
                add(JsonObject(pageObject.toMutableMap().apply { put("content", newBody) }))
            }
        }
        if (!hit) return null
        return JsonObject(o.toMutableMap().apply { put("pages", newPages) })
    }

    // A bare body that yields no block renders as one empty paragraph
    // (PmParser.parseBareDoc), so the synthetic block belongs to it.
    val (newBody, _, changed) = editInBody(o, 0, index, synthesizeEmpty = true, transform = transform)
    return if (changed) newBody else null
}

/** How many blocks PmParser emits for one `pages[]` entry. */
private fun pageBlockCount(page: JsonElement): Int {
    val body = (page as? JsonObject)?.get("content") as? JsonObject ?: return 0
    val children = body["content"] as? JsonArray ?: return 0
    return children.count { it is JsonObject }
}

/** Index of the first page whose body can receive a block, or -1. */
private fun firstWritablePage(pages: JsonArray): Int =
    pages.indexOfFirst { (it as? JsonObject)?.get("content") is JsonObject }

/**
 * Rewrites one block inside a `{type:"doc", content:[…]}` body.
 * Returns the body, the next logical index, and whether anything changed.
 *
 * [synthesizeEmpty] says whether the single empty paragraph PmParser falls back
 * to belongs to THIS body. A merely empty page of a multi-page envelope holds no
 * block at all, and therefore no index.
 */
private fun editInBody(
    body: JsonObject,
    start: Int,
    index: Int,
    synthesizeEmpty: Boolean,
    transform: (JsonObject) -> List<JsonObject>?,
): Triple<JsonObject, Int, Boolean> {
    val children = body["content"] as? JsonArray
    if (children == null || children.isEmpty()) {
        if (!synthesizeEmpty) return Triple(body, start, false)
        // The body renders as one empty paragraph, which has no node to rewrite:
        // create the one the user just acted on. It occupies one index either way.
        if (index != start) return Triple(body, start + 1, false)
        val replacement = transform(emptyParagraph()) ?: return Triple(body, start + 1, false)
        val content = buildJsonArray { replacement.forEach { add(it) } }
        return Triple(
            JsonObject(body.toMutableMap().apply { put("content", content) }),
            start + replacement.size,
            true,
        )
    }

    var logical = start
    var changed = false
    val out = buildJsonArray {
        for (child in children) {
            val node = child as? JsonObject
            if (node == null) {
                // Not a node: the parser skips it, so it holds no index.
                add(child)
                continue
            }
            val replacement = if (logical == index) transform(node) else null
            if (replacement == null) {
                add(node)
            } else {
                replacement.forEach { add(it) }
                changed = true
            }
            logical++
        }
    }
    val newBody = if (changed) JsonObject(body.toMutableMap().apply { put("content", out) }) else body
    return Triple(newBody, logical, changed)
}

/** Applies [transform] to the node a [DocBlockPath] addresses, list items included. */
private fun editTarget(
    root: JsonElement,
    path: DocBlockPath,
    transform: (JsonObject) -> JsonObject?,
): JsonElement? = editBlock(root, path.blockIndex) { block ->
    val rewritten = if (path.itemIndex == null) {
        transform(block)
    } else {
        editListItem(block, path.itemIndex, transform)
    }
    rewritten?.let { listOf(it) }
}

/** Rewrites the first block of list item [itemIndex], keeping every sibling. */
private fun editListItem(
    list: JsonObject,
    itemIndex: Int,
    transform: (JsonObject) -> JsonObject?,
): JsonObject? {
    val items = list["content"] as? JsonArray ?: return null
    val item = items.getOrNull(itemIndex) as? JsonObject ?: return null
    val children = item["content"] as? JsonArray
    val firstIndex = children?.indexOfFirst { it is JsonObject } ?: -1

    val newItem = if (children == null || firstIndex < 0) {
        val replacement = transform(emptyParagraph()) ?: return null
        JsonObject(item.toMutableMap().apply { put("content", buildJsonArray { add(replacement) }) })
    } else {
        val replacement = transform(children[firstIndex] as JsonObject) ?: return null
        val newChildren = buildJsonArray {
            children.forEachIndexed { i, child -> add(if (i == firstIndex) replacement else child) }
        }
        JsonObject(item.toMutableMap().apply { put("content", newChildren) })
    }

    val newItems = buildJsonArray {
        items.forEachIndexed { i, existing -> add(if (i == itemIndex) newItem else existing) }
    }
    return JsonObject(list.toMutableMap().apply { put("content", newItems) })
}

/**
 * The same node carrying [text], split on newlines into text runs separated by
 * `hardBreak` nodes — the schema's own line break, and what the parser turns
 * back into "\n". The marks of the first run are carried over so retyping a bold
 * line stays bold.
 *
 * Every OTHER inline child — a footnote or endnote call and the note it carries,
 * an inline picture, a Word field — is put back untouched AFTER the rewritten
 * runs. It cannot be put back in place: the field is a flat string and holds no
 * trace of where an atom sat in it. Appending is therefore a deliberate
 * trade-off — the atom may move to the end of its paragraph — against the only
 * alternative, deleting it from the server behind the user's back.
 *
 * In practice this loop has nothing to move: `isRewritable` keeps a block
 * carrying an atom out of the text field entirely. It is the last line of
 * defence, because the cost of missing one path is a note lost for good.
 */
private fun withText(node: JsonObject, text: String): JsonObject {
    val children = node["content"] as? JsonArray
    val marks = children
        ?.filterIsInstance<JsonObject>()
        ?.firstOrNull { it["type"].asText() == "text" }
        ?.get("marks")
    val preserved = children.orEmpty().filterNot { child ->
        val type = (child as? JsonObject)?.get("type").asText()
        type == "text" || type == "hardBreak"
    }
    val content = buildJsonArray {
        text.split('\n').forEachIndexed { i, line ->
            if (i > 0) add(buildJsonObject { put("type", JsonPrimitive("hardBreak")) })
            if (line.isNotEmpty()) {
                add(
                    buildJsonObject {
                        put("type", JsonPrimitive("text"))
                        put("text", JsonPrimitive(line))
                        if (marks != null) put("marks", marks)
                    },
                )
            }
        }
        preserved.forEach { add(it) }
    }
    return JsonObject(node.toMutableMap().apply { put("content", content) })
}

/**
 * Toggles [markType] on every text run of the block: the mark goes ON unless the
 * whole block already carries it, which is the state the toolbar chip shows.
 * Returns null for a block with no text — there is nothing to mark yet.
 */
private fun withMarkToggled(node: JsonObject, markType: String): JsonObject? {
    val children = node["content"] as? JsonArray ?: return null
    val runs = children.filterIsInstance<JsonObject>().filter { it["type"].asText() == "text" }
    if (runs.isEmpty()) return null
    val enable = !runs.all { hasMark(it, markType) }
    val out = buildJsonArray {
        for (child in children) {
            val run = child as? JsonObject
            if (run == null || run["type"].asText() != "text") add(child) else add(withMark(run, markType, enable))
        }
    }
    return JsonObject(node.toMutableMap().apply { put("content", out) })
}

private fun hasMark(run: JsonObject, markType: String): Boolean =
    (run["marks"] as? JsonArray)?.any { (it as? JsonObject)?.get("type").asText() == markType } == true

private fun withMark(run: JsonObject, markType: String, enable: Boolean): JsonObject {
    val marks = run["marks"] as? JsonArray
    val kept = marks?.filter { (it as? JsonObject)?.get("type").asText() != markType } ?: emptyList()
    val newMarks = buildJsonArray {
        kept.forEach { add(it) }
        if (enable) add(buildJsonObject { put("type", JsonPrimitive(markType)) })
    }
    return JsonObject(
        run.toMutableMap().apply {
            if (newMarks.isEmpty()) remove("marks") else put("marks", newMarks)
        },
    )
}

/**
 * Turns the block into a heading of [level], or back into a paragraph when it is
 * null. A named style ("Heading 1") is dropped on the way: it would keep
 * describing the previous role and fight the new node type on the web side.
 */
private fun withHeadingLevel(node: JsonObject, level: Int?): JsonObject {
    val attrs = (node["attrs"] as? JsonObject)?.toMutableMap() ?: mutableMapOf()
    attrs.remove("styleName")
    if (level == null) attrs.remove("level") else attrs["level"] = JsonPrimitive(level)
    return JsonObject(
        node.toMutableMap().apply {
            put("type", JsonPrimitive(if (level == null) "paragraph" else "heading"))
            if (attrs.isEmpty()) remove("attrs") else put("attrs", JsonObject(attrs))
        },
    )
}

/**
 * Toggles the block between a plain block and a one-item list of [kind]:
 * the same kind unwraps, another kind converts, anything else wraps.
 */
private fun toggledList(node: JsonObject, kind: String): List<JsonObject> {
    when (node["type"].asText()) {
        kind -> return unwrapList(node)
        BULLET_LIST, ORDERED_LIST, TASK_LIST -> return listOf(retypedList(node, kind))
        else -> Unit
    }
    // A heading becomes a paragraph on the way in: a list item's content is a
    // paragraph in the editor's schema, and a heading inside one would not load
    // back on the web.
    val content = if (node["type"].asText() == "heading") withHeadingLevel(node, null) else node
    val item = buildJsonObject {
        put("type", JsonPrimitive(LIST_ITEM))
        put("content", buildJsonArray { add(content) })
    }
    return listOf(
        buildJsonObject {
            put("type", JsonPrimitive(kind))
            put("content", buildJsonArray { add(item) })
        },
    )
}

/** Same items, different list type — task items become plain items. */
private fun retypedList(list: JsonObject, kind: String): JsonObject {
    val items = list["content"] as? JsonArray
    val newItems = buildJsonArray {
        items?.forEach { item ->
            val o = item as? JsonObject
            if (o == null) {
                add(item)
            } else {
                // `checked` only means something on a task item; carrying it over
                // would leave a stray attribute the web schema rejects.
                val attrs = (o["attrs"] as? JsonObject)?.toMutableMap()?.apply { remove("checked") }
                add(
                    JsonObject(
                        o.toMutableMap().apply {
                            put("type", JsonPrimitive(LIST_ITEM))
                            if (attrs == null || attrs.isEmpty()) remove("attrs") else put("attrs", JsonObject(attrs))
                        },
                    ),
                )
            }
        }
    }
    return JsonObject(
        list.toMutableMap().apply {
            put("type", JsonPrimitive(kind))
            put("content", newItems)
        },
    )
}

/** Every item's blocks, lifted back to the top level. */
private fun unwrapList(list: JsonObject): List<JsonObject> {
    val items = list["content"] as? JsonArray ?: return listOf(emptyParagraph())
    val out = mutableListOf<JsonObject>()
    for (item in items) {
        val children = (item as? JsonObject)?.get("content") as? JsonArray ?: continue
        out += children.filterIsInstance<JsonObject>()
    }
    return out.ifEmpty { listOf(emptyParagraph()) }
}

private fun emptyParagraph(): JsonObject = buildJsonObject {
    put("type", JsonPrimitive("paragraph"))
    put("content", buildJsonArray { })
}

/** Lenient read: kotlinx's own accessors throw on a type mismatch. */
private fun JsonElement?.asText(): String? = (this as? JsonPrimitive)?.contentOrNull
