package com.kubuno.docs.ui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.sp
import com.kubuno.android.ui.components.KubunoBadge
import com.kubuno.android.ui.components.KubunoBadgeVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoSeparator
import com.kubuno.docs.pm.PmAlign
import com.kubuno.docs.pm.PmBlockStyle
import com.kubuno.docs.pm.PmInline
import com.kubuno.docs.pm.PmNode
import com.kubuno.docs.pm.PmOrientation
import com.kubuno.docs.ui.DocsColors
import com.kubuno.docs.ui.DocsPage
import com.kubuno.docs.ui.DocsShape
import com.kubuno.docs.ui.DocsType
import com.kubuno.docs.ui.docsPageInk
import com.kubuno.docs.ui.docsToneLight

/**
 * Compose rendering of the BLOCK nodes of a parsed document body.
 *
 * The web editor lays its blocks out on a paginated canvas
 * (office/frontend/src/canvas-engine.ts); the phone reflows them into a single
 * fluid column instead — the rationale is in `DocsPage`'s own documentation.
 * The consequence for this file: a page or section break has no geometric
 * effect here, so it is rendered as an explicit but sober landmark rather than
 * being dropped, which would silently change how the document reads.
 *
 * Lists, tables and images are rendered elsewhere ([PmList], [PmTable],
 * [PmImage]); this file only dispatches to them. Inline runs are NOT rendered
 * here either: they go through [PmText], the app's single inline engine.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Render style
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Everything the block renderers need that is NOT in the document: the ink, the
 * hairlines and the accents of the host surface.
 *
 * It travels through a composition local so the renderers that take only a node
 * ([PmList], [PmTable], [PmImage], and the recursive calls inside table cells or
 * list items) stay on the same palette without threading a parameter through
 * every signature.
 */
@Immutable
data class PmRenderStyle(
    /** Body text style; headings derive from `DocsType`, not from this. */
    val base: TextStyle,
    /** Document ink. Inline `color` marks override it per span. */
    val ink: Color,
    /** Secondary ink for landmarks and fallbacks — never for document text. */
    val muted: Color,
    /** Hairline of quotes and fallback frames. */
    val rule: Color,
    /**
     * Ground of a code BLOCK. canvas-engine paints it on `#f8f9fa` (surface-1),
     * which is NOT the `#f1f3f4` (surface-2) of an inline `code` mark — the two
     * greys are one step apart and the block used to be given the inline one.
     */
    val codeBackground: Color,
    /** Link colour, and the colour of a tracked insertion. */
    val accent: Color,
    /** Colour of a tracked deletion. */
    val deletion: Color,
    /** Ground painted under a commented run. */
    val commentGround: Color,
    /**
     * When false, tracked runs are painted as plain text. The renderer never
     * DROPS a deletion: hiding text the server still stores would make the
     * screen disagree with the document.
     */
    val showTrackChanges: Boolean = true,
)

/** Null until a [PmBlocks] provides one, so nesting inherits instead of resetting. */
val LocalPmRenderStyle = compositionLocalOf<PmRenderStyle?> { null }

/** The office defaults, theme-aware. */
@Composable
@ReadOnlyComposable
fun defaultPmRenderStyle(base: TextStyle = DocsType.Body): PmRenderStyle = PmRenderStyle(
    base = base,
    ink = docsPageInk(),
    muted = MaterialTheme.colorScheme.onSurfaceVariant,
    rule = MaterialTheme.colorScheme.outline,
    // surfaceContainerLow, not surfaceVariant: the former is theme.css surface-1
    // (#f8f9fa), the ground canvas-engine gives a code block.
    codeBackground = MaterialTheme.colorScheme.surfaceContainerLow,
    accent = docsToneLight(),
    deletion = MaterialTheme.colorScheme.error,
    commentGround = DocsColors.WarningLight.copy(alpha = 0.55f),
)

/** The style in force, or the office defaults outside any [PmBlocks]. */
@Composable
@ReadOnlyComposable
fun currentPmRenderStyle(): PmRenderStyle = LocalPmRenderStyle.current ?: defaultPmRenderStyle()

/**
 * The inline engine's own contract, derived from the block style so a block and
 * the runs inside it cannot disagree on a colour. The theme-aware defaults supply
 * what a block style does not own (note calls, atom markers).
 */
@Composable
@ReadOnlyComposable
fun PmRenderStyle.inlineStyling(): PmInlineStyling = pmInlineStyling().copy(
    linkColor = accent,
    commentBackground = commentGround,
    revisionInk = showTrackChanges,
)

// ─────────────────────────────────────────────────────────────────────────────
// Entry points
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Renders a run of block nodes as a single fluid column.
 *
 * Vertical rhythm comes from each block's own space-before/space-after (the
 * document's, or canvas-engine's defaults), not from an arrangement spacing:
 * stacking both would double every gap.
 */
@Composable
fun PmBlocks(
    nodes: List<PmNode>,
    style: PmRenderStyle = currentPmRenderStyle(),
    modifier: Modifier = Modifier,
) {
    CompositionLocalProvider(LocalPmRenderStyle provides style) {
        Column(modifier = modifier.fillMaxWidth()) {
            nodes.forEach { node ->
                // `pageBreakBefore` is an attribute of the paragraph, not a node;
                // canvas-engine treats it exactly like a pageBreak node, so the
                // landmark has to be drawn here, before the block itself.
                if (node.blockStyleOrNull()?.pageBreakBefore == true) {
                    PmBreakLandmark(label = LABEL_PAGE_BREAK, style = style)
                }
                PmBlock(node)
            }
        }
    }
}

/**
 * Renders one block node, dispatching on its type. Unmodelled nodes land on
 * [PmNode.Unknown] and are still shown as readable text.
 */
@Composable
fun PmBlock(node: PmNode, modifier: Modifier = Modifier) {
    val style = currentPmRenderStyle()
    when (node) {
        is PmNode.Paragraph -> PmTextBlock(
            inlines = node.inlines,
            blockStyle = node.style,
            textStyle = style.base,
            level = 0,
            style = style,
            modifier = modifier,
        )

        is PmNode.Heading -> PmTextBlock(
            inlines = node.inlines,
            blockStyle = node.style,
            textStyle = DocsType.heading(node.level),
            level = node.level,
            style = style,
            modifier = modifier,
        )

        is PmNode.Blockquote -> PmBlockquote(node, style, modifier)
        is PmNode.CodeBlock -> PmCodeBlock(node, style, modifier)
        PmNode.HorizontalRule -> PmRule(modifier)
        PmNode.PageBreak -> PmBreakLandmark(LABEL_PAGE_BREAK, style, modifier)
        is PmNode.SectionBreak -> PmSectionLandmark(node, style, modifier)

        is PmNode.BulletList -> PmList(node)
        is PmNode.OrderedList -> PmList(node)
        is PmNode.TaskList -> PmList(node)
        is PmNode.Table -> PmTable(node)
        is PmNode.Image -> PmImage(node)

        // Structural nodes only ever reached when a document nests them oddly
        // (a bare list item or table row at block level). Rendering their
        // children keeps the text visible instead of dropping a whole subtree.
        is PmNode.ListItem -> PmBlocks(node.children, style, modifier)
        is PmNode.TableCell -> PmBlocks(node.children, style, modifier)
        is PmNode.TableRow -> PmBlocks(node.cells.flatMap { it.children }, style, modifier)

        is PmNode.Unknown -> PmUnknownBlock(node, style, modifier)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Text blocks
// ─────────────────────────────────────────────────────────────────────────────

/**
 * A paragraph or a heading. [level] is 0 for a paragraph and 1..6 for a heading,
 * which is what picks the default spacing from canvas-engine's H_BEFORE/H_AFTER.
 */
@Composable
private fun PmTextBlock(
    inlines: List<PmInline>,
    blockStyle: PmBlockStyle,
    textStyle: TextStyle,
    level: Int,
    style: PmRenderStyle,
    modifier: Modifier = Modifier,
) {
    val before = blockStyle.spaceBeforePx?.let { DocsPage.px(it) } ?: DocsType.spaceBefore(level)
    val after = blockStyle.spaceAfterPx?.let { DocsPage.px(it) } ?: DocsType.spaceAfter(level)
    val indentStart = blockStyle.indentLeftPx
        ?: (blockStyle.indentLevel * DocsPage.ListIndent.value * DocsPage.PhoneTypeScale)
    val indentEnd = blockStyle.indentRightPx ?: 0f

    val resolved = textStyle.copy(
        textAlign = blockStyle.align.toTextAlign(),
        lineHeight = blockStyle.lineHeight
            ?.let { ratio -> textStyle.fontSize.scaledBy(ratio) }
            ?: textStyle.lineHeight,
        // A first-line indent cannot be expressed with padding: it belongs to the
        // paragraph, which is why it goes through TextIndent.
        textIndent = blockStyle.indentFirstLinePx
            ?.takeIf { it != 0f }
            ?.let { TextIndent(firstLine = (it * DocsPage.PhoneTypeScale).sp) }
            ?: textStyle.textIndent,
    )

    val layout = modifier
        .fillMaxWidth()
        .padding(
            top = before,
            bottom = after,
            start = DocsPage.px(indentStart.coerceAtLeast(0f)),
            end = DocsPage.px(indentEnd.coerceAtLeast(0f)),
        )

    if (inlines.isEmpty()) {
        // An empty paragraph is deliberate spacing in a word processor, so it has
        // to keep its line box instead of collapsing to nothing.
        Spacer(layout.height(resolved.lineBoxHeight()))
        return
    }

    val notes = inlineNotes(inlines)
    if (notes.isEmpty()) {
        PmText(
            inlines = inlines,
            style = resolved,
            modifier = layout,
            color = style.ink,
            styling = style.inlineStyling(),
        )
        return
    }

    Column(layout) {
        PmText(
            inlines = inlines,
            style = resolved,
            modifier = Modifier.fillMaxWidth(),
            color = style.ink,
            styling = style.inlineStyling(),
        )
        notes.forEach { PmNoteLine(it, style) }
    }
}

/**
 * The text of a note, under the block that calls it.
 *
 * The web reserves a block at the bottom of the PAGE (footnotes) or at the end of
 * the DOCUMENT (endnotes); the phone column has neither, so the note is shown
 * right under the paragraph that references it — the alternative, hiding it
 * behind a tap, leaves what the user wrote invisible on the screen.
 * Format transcribed from `buildEndnoteLines`: "<marker>. <text>", 9 pt, and the
 * ellipsis of an empty note.
 */
@Composable
private fun PmNoteLine(atom: PmInline.Atom, style: PmRenderStyle) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = DocsPage.px(2), start = DocsPage.px(12)),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(text = "${atom.label}.", style = DocsType.Note, color = style.ink)
        // weight, so a long note wraps under itself instead of pushing the marker
        // out of the column.
        Text(
            text = atom.text.ifBlank { EMPTY_NOTE },
            style = DocsType.Note,
            color = style.ink,
            modifier = Modifier.weight(1f),
        )
    }
}

/** canvas-engine's own stand-in for a note with no text yet (`text || '…'`). */
private const val EMPTY_NOTE = "…"

@Composable
private fun PmBlockquote(node: PmNode.Blockquote, style: PmRenderStyle, modifier: Modifier = Modifier) {
    // The canvas has no blockquote treatment of its own (TipTap styles it in CSS
    // there), so the phone uses the design system's quote shape: an accent rule
    // plus an inset. Assumed divergence, kept sober on purpose.
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = DocsType.BlockSpacing)
            .height(IntrinsicSize.Min),
    ) {
        Box(
            Modifier
                .width(3.dp)
                .fillMaxHeight()
                .clip(DocsShape.Chip)
                .background(style.accent),
        )
        PmBlocks(node.children, style, Modifier.padding(start = 12.dp))
    }
}

@Composable
private fun PmCodeBlock(node: PmNode.CodeBlock, style: PmRenderStyle, modifier: Modifier = Modifier) {
    // canvas-engine renders code as Courier New 10pt on #f8f9fa. Code lines are
    // scrolled rather than wrapped: wrapping is what makes indented code
    // unreadable on a narrow screen.
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = DocsPage.px(4)),
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .clip(DocsShape.Chip)
                .background(style.codeBackground)
                .padding(horizontal = 12.dp, vertical = 8.dp)
                .horizontalScroll(rememberScrollState()),
        ) {
            Text(
                text = node.code,
                style = DocsType.Code,
                color = style.ink,
                softWrap = false,
            )
        }
        node.language?.let { language ->
            Text(
                text = language,
                style = DocsType.Note,
                color = style.muted,
                modifier = Modifier.padding(top = 2.dp, start = 4.dp),
            )
        }
    }
}

@Composable
private fun PmRule(modifier: Modifier = Modifier) {
    // canvas-engine draws the rule as a hairline row with 8px of air above and
    // below; the shared separator carries the same hairline.
    KubunoSeparator(
        modifier
            .fillMaxWidth()
            .padding(vertical = DocsType.BlockSpacing),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Breaks
// ─────────────────────────────────────────────────────────────────────────────

private const val LABEL_PAGE_BREAK = "Saut de page"

/** A hairline row with a centred caption — the fluid column's stand-in for a page edge. */
@Composable
private fun PmBreakLandmark(label: String, style: PmRenderStyle, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = DocsPage.px(12)),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        KubunoSeparator(Modifier.weight(1f))
        Text(text = label, style = DocsType.Note, color = style.muted)
        KubunoSeparator(Modifier.weight(1f))
    }
}

/**
 * A section break also changes the page geometry of everything that follows.
 * The phone cannot show that change (it has no pages), so the geometry is
 * spelled out in the caption instead of being lost.
 */
@Composable
private fun PmSectionLandmark(node: PmNode.SectionBreak, style: PmRenderStyle, modifier: Modifier = Modifier) {
    val details = buildList {
        add("Nouvelle section")
        if (node.orientation == PmOrientation.LANDSCAPE) add("paysage")
        if (node.columns > 1) add("${node.columns} colonnes")
    }.joinToString(" · ")

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = DocsPage.px(12)),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        KubunoSeparator(Modifier.weight(1f))
        KubunoBadge(text = details, variant = KubunoBadgeVariant.NEUTRAL)
        KubunoSeparator(Modifier.weight(1f))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fallback
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The safety net for a node type this version does not model (embeds, custom
 * blocks…). The parser already flattened its content, so the reader still gets
 * it; the rule on the left says "this is not rendered faithfully" without
 * shouting, and an empty node names its type instead of leaving a blank.
 *
 * The unreadable-body marker is the exception: it is not an unsupported block but
 * a body this version failed to READ, and the reader has to be able to tell that
 * from an empty document — so it gets a sentence, not a type name.
 */
@Composable
private fun PmUnknownBlock(node: PmNode.Unknown, style: PmRenderStyle, modifier: Modifier = Modifier) {
    if (node.type == PmNode.Unknown.UNREADABLE_BODY) {
        KubunoCallout(
            text = "Cette version de l'application n'a pas su lire le contenu de ce " +
                "document. Il n'est pas vide : ouvrez-le dans l'éditeur web pour le consulter.",
            modifier = modifier.padding(vertical = DocsType.BlockSpacing),
            variant = KubunoCalloutVariant.WARNING,
            title = "Contenu illisible",
        )
        return
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = DocsPage.px(4))
            .height(IntrinsicSize.Min),
    ) {
        Box(
            Modifier
                .width(2.dp)
                .fillMaxHeight()
                .clip(DocsShape.Chip)
                .background(style.rule),
        )
        if (node.inlines.isEmpty()) {
            Text(
                text = "Contenu non pris en charge" + node.type.takeIf { it.isNotBlank() }?.let { " ($it)" }.orEmpty(),
                style = DocsType.Note.copy(fontStyle = FontStyle.Italic),
                color = style.muted,
                modifier = Modifier.padding(start = 10.dp),
            )
        } else {
            Column(Modifier.padding(start = 10.dp)) {
                PmText(
                    inlines = node.inlines,
                    style = style.base,
                    color = style.ink,
                    styling = style.inlineStyling(),
                )
                inlineNotes(node.inlines).forEach { PmNoteLine(it, style) }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Small helpers
// ─────────────────────────────────────────────────────────────────────────────

private fun PmAlign.toTextAlign(): TextAlign = when (this) {
    PmAlign.LEFT -> TextAlign.Start
    PmAlign.CENTER -> TextAlign.Center
    PmAlign.RIGHT -> TextAlign.End
    PmAlign.JUSTIFY -> TextAlign.Justify
}

/** The paragraph attributes of a block, for the blocks that carry any. */
private fun PmNode.blockStyleOrNull(): PmBlockStyle? = when (this) {
    is PmNode.Paragraph -> style
    is PmNode.Heading -> style
    else -> null
}

private fun TextUnit.scaledBy(ratio: Float): TextUnit =
    if (isSpecified) (value * ratio).sp else this

/**
 * Height of one empty line, used to keep an empty paragraph's line box.
 * Approximated in dp from the sp metric: the exact box is only known after text
 * layout, and an empty paragraph has no text to lay out.
 */
private fun TextStyle.lineBoxHeight(): Dp = when {
    lineHeight.isSpecified -> lineHeight.value.dp
    fontSize.isSpecified -> (fontSize.value * DocsType.LineHeightRatio).dp
    else -> DocsType.Body.fontSize.value.dp
}
