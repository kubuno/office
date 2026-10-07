package com.kubuno.docs.ui.render

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.LinkInteractionListener
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.BaselineShift
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.sp
import com.kubuno.docs.pm.PmAtomRole
import com.kubuno.docs.pm.PmChangeKind
import com.kubuno.docs.pm.PmInline
import com.kubuno.docs.pm.PmMark
import com.kubuno.docs.ui.DocsColors
import com.kubuno.docs.ui.DocsPage
import com.kubuno.docs.ui.DocsType
import com.kubuno.docs.ui.docsPageInk

/**
 * Inline rendering: a run of [PmInline] becomes one [AnnotatedString].
 *
 * THE single inline engine of this app: the block, list and table renderers all
 * go through [PmText]/[buildAnnotated]. Keeping a second private implementation
 * next to the block renderer is what let the two drift apart on script scale,
 * revision ink and link colour, with only one of them ever reaching the screen.
 *
 * The whole point of this file is that marks COMPOSE. A single character can be
 * bold, coloured, superscript, linked, commented and part of a tracked insertion
 * at the same time, so the marks of a run are folded into ONE [SpanStyle] before
 * anything is emitted, instead of being applied one at a time: font size is the
 * obvious case — an explicit `fontSize` mark and a `superscript` mark have to be
 * multiplied together, and applying them independently would either drop one or
 * shrink the run twice.
 *
 * The visual reference is the web canvas renderer
 * (`office/frontend/src/canvas-engine.ts`): script scale, script offsets,
 * revision ink and the "deletion wins over insertion" rule are transcribed from
 * it, not invented. Divergences are documented where they occur.
 *
 * Invariant: an unrecognised mark NEVER costs the text it carries. Anything this
 * renderer does not understand degrades to the base style and the characters are
 * still appended.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Styling
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The few colours the inline renderer has to supply itself, i.e. the ones that
 * are NOT in the document: link ink, the tint of a commented run, and whether
 * tracked revisions are shown as markup at all.
 *
 * Passed in rather than read from the theme inside [buildAnnotated] so the
 * builder stays a pure function (unit-testable on the JVM, callable from a
 * background thread when a screen pre-measures a long document).
 */
@Immutable
data class PmInlineStyling(
    /** Ink of a `link` mark, used only when the run carries no explicit colour. */
    val linkColor: Color,
    /** Background of a `comment` mark, used only when the run carries no highlight. */
    val commentBackground: Color,
    /** Ink of a note CALL in the flow — canvas-engine paints it `#1a73e8`. */
    val noteCallColor: Color,
    /** Ink of the marker standing for an atom this version cannot draw. */
    val atomInk: Color,
    val linkUnderline: Boolean = true,
    /**
     * Paint tracked insertions/deletions as markup (author ink + underline /
     * strikethrough). False renders the text plainly — the equivalent of the
     * web's "final document" display for insertions, except that deleted text
     * still shows, since dropping runs is a layout decision, not an inline one.
     */
    val revisionInk: Boolean = true,
) {
    companion object {
        val Light = PmInlineStyling(
            linkColor = DocsColors.Primary,
            // No web counterpart: the canvas paints comment ranges with the colour
            // the comment pane assigns to each thread, which this client does not
            // have at inline-render time. The module's own warning hue at low alpha
            // is used instead, so a commented run reads as "annotated" without
            // fighting a real `highlight` mark.
            commentBackground = Color(0x33F9AB00),
            noteCallColor = DocsColors.Primary,
            atomInk = DocsColors.TextSecondary,
        )
        val Dark = PmInlineStyling(
            linkColor = DocsColors.DarkPrimary,
            commentBackground = Color(0x3DFDD663),
            noteCallColor = DocsColors.DarkPrimary,
            atomInk = DocsColors.DarkTextSecondary,
        )
    }
}

@Composable
@ReadOnlyComposable
fun pmInlineStyling(): PmInlineStyling =
    if (isSystemInDarkTheme()) PmInlineStyling.Dark else PmInlineStyling.Light

// ─────────────────────────────────────────────────────────────────────────────
// Annotations and constants
// ─────────────────────────────────────────────────────────────────────────────

object PmInlineRenderer {

    /**
     * Tag of the string annotation carrying a `comment` mark's thread id, so a
     * screen can map a tap (or a long-press selection) back to a thread without
     * re-walking the ProseMirror JSON.
     */
    const val COMMENT_TAG: String = "pm-comment"

    /** Tag carrying the ProseMirror type name of a mark this renderer ignores. */
    const val UNKNOWN_MARK_TAG: String = "pm-unknown-mark"

    /** Tag carrying `insertion` / `deletion`, for a revision-aware screen. */
    const val TRACK_CHANGE_TAG: String = "pm-track-change"

    /**
     * Tag carrying the ProseMirror type of an inline ATOM, so a screen can map a
     * tap on a note call back to the atom without re-walking the JSON.
     */
    const val ATOM_TAG: String = "pm-atom"

    /** Id of the inline slot a picture atom is drawn in — see [pmInlineContent]. */
    fun imageSlotId(index: Int): String = "pm-inline-image-$index"

    /** canvas-engine.ts SCRIPT_SCALE — relative size of a sub/superscript run. */
    const val SCRIPT_SCALE: Float = 0.66f

    /**
     * Baseline shifts for sub/superscript.
     *
     * The canvas offsets the drawing baseline by `-0.36 × base` (super) and
     * `+0.18 × base` (sub), where `base` is the size BEFORE [SCRIPT_SCALE].
     * Compose instead multiplies the ascent of the run's OWN (already shrunk)
     * font, so the web ratios are converted once here:
     * `0.36 / (SCRIPT_SCALE × ascent≈0.9) ≈ 0.6` and `0.18 / … ≈ 0.3`.
     * Using Compose's stock 0.5 / −0.5 would sit the exponent visibly lower
     * than the same document on the desktop.
     */
    val SuperscriptShift: BaselineShift = BaselineShift(0.6f)
    val SubscriptShift: BaselineShift = BaselineShift(-0.3f)

    /** Thread id of the comment covering [offset], or null. */
    fun commentIdAt(text: AnnotatedString, offset: Int): String? =
        text.getStringAnnotations(COMMENT_TAG, offset, offset).firstOrNull()?.item

    /** Href of the link covering [offset], or null. */
    fun linkAt(text: AnnotatedString, offset: Int): String? =
        text.getLinkAnnotations(offset, offset)
            .firstNotNullOfOrNull { (it.item as? LinkAnnotation.Url)?.url }

    /**
     * Stable ink of a revision author, transcribed from
     * `office/frontend/src/documents/track-changes.ts` (`authorColor`): the same
     * 31-multiplier hash over the same palette, so a change shows the same colour
     * here and in the web review pane.
     *
     * Known divergence: the web keys on `authorId || author`, but the parser only
     * keeps the author NAME ([PmMark.TrackChange] has no author id), so a document
     * whose revisions are attributed by id resolves to a different hue. An empty
     * key yields the first hue on both sides.
     */
    fun authorInk(key: String): Color {
        var hash = 0
        for (c in key) hash = hash * 31 + c.code
        // The web keeps the hash unsigned (`>>> 0`) before the modulo; the bits are
        // identical, only the interpretation differs — hence UInt and not Int here.
        val index = (hash.toUInt() % AUTHOR_PALETTE.size.toUInt()).toInt()
        return AUTHOR_PALETTE[index]
    }

    private val AUTHOR_PALETTE: List<Color> = listOf(
        Color(0xFF1A73E8), Color(0xFFD93025), Color(0xFF1E8E3E), Color(0xFFF9AB00),
        Color(0xFF9334E6), Color(0xFFE8710A), Color(0xFF12B5CB), Color(0xFFD01884),
        Color(0xFF7CB342), Color(0xFF3949AB),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Builder
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Folds [inlines] into an [AnnotatedString].
 *
 * [base] is NOT baked into the spans: only the DELTAS a mark introduces are
 * emitted, so the caller keeps passing the same style to the text composable and
 * a user font-scale change still applies. [base] is read for one thing only —
 * the size a sub/superscript run shrinks from.
 *
 * [onLinkClick] receives the href of a tapped link. Null leaves the link to
 * Compose, which opens it with the platform URI handler.
 */
fun buildAnnotated(
    inlines: List<PmInline>,
    base: TextStyle,
    styling: PmInlineStyling = PmInlineStyling.Light,
    onLinkClick: ((String) -> Unit)? = null,
): AnnotatedString {
    val baseSize: TextUnit = base.fontSize.takeIf { it.isSpecified } ?: DocsType.Body.fontSize
    val listener: LinkInteractionListener? = onLinkClick?.let { handler ->
        LinkInteractionListener { link ->
            (link as? LinkAnnotation.Url)?.url?.let(handler)
        }
    }

    return buildAnnotatedString {
        inlines.forEachIndexed { index, inline ->
            when (inline) {
                is PmInline.Text -> if (inline.text.isNotEmpty()) {
                    val resolved = resolve(inline.marks)
                    val start = length
                    withStyle(resolved.spanStyle(baseSize, styling)) { append(inline.text) }
                    annotate(resolved, start, length, listener)
                }

                is PmInline.Atom -> appendAtom(inline, index, baseSize, styling, listener)
            }
        }
    }
}

/**
 * An inline atom, in the flow.
 *
 * Nothing here may end up appending nothing: an atom that leaves no trace is an
 * atom the reader cannot know about, and this client would then be editing (and
 * saving) a document it does not show.
 */
private fun AnnotatedString.Builder.appendAtom(
    atom: PmInline.Atom,
    index: Int,
    baseSize: TextUnit,
    styling: PmInlineStyling,
    listener: LinkInteractionListener?,
) {
    val start = length
    when (atom.role) {
        // The call of a note: a superscript number, already numbered in document
        // order by the parser. The note TEXT is not in the flow on the web either
        // — the block renderer shows it under the block that calls it.
        PmAtomRole.FOOTNOTE, PmAtomRole.ENDNOTE -> {
            val resolved = resolve(atom.marks)
            resolved.script = Script.SUPER
            if (resolved.color == null) resolved.color = styling.noteCallColor
            withStyle(resolved.spanStyle(baseSize, styling)) {
                append(atom.label.ifEmpty { NOTE_FALLBACK_MARKER })
            }
            annotate(resolved, start, length, listener)
        }

        // A field is measured and painted exactly like text on the web; the parser
        // already resolved what it displays.
        PmAtomRole.FIELD -> {
            val resolved = resolve(atom.marks)
            withStyle(resolved.spanStyle(baseSize, styling)) {
                append(atom.label.ifEmpty { atomMarker(atom.type) })
            }
            annotate(resolved, start, length, listener)
        }

        // A picture in the flow: a reserved slot that [pmInlineContent] fills with
        // the real image, keyed on the same index.
        PmAtomRole.IMAGE -> appendInlineContent(
            PmInlineRenderer.imageSlotId(index),
            atom.text.ifEmpty { IMAGE_PLACEHOLDER_ALT },
        )

        // Truly unknown: a discreet but VISIBLE marker, plus whatever text the
        // atom carries. Silence would hide content the user wrote.
        PmAtomRole.OTHER -> {
            withStyle(SpanStyle(color = styling.atomInk, fontStyle = FontStyle.Italic)) {
                append(atomMarker(atom.type))
            }
            if (atom.text.isNotEmpty()) {
                withStyle(resolve(atom.marks).spanStyle(baseSize, styling)) { append(atom.text) }
            }
        }
    }
    if (length > start) {
        addStringAnnotation(PmInlineRenderer.ATOM_TAG, atom.type, start, length)
    }
}

/** The link, comment, revision and unknown-mark annotations of one emitted range. */
private fun AnnotatedString.Builder.annotate(
    resolved: ResolvedRun,
    start: Int,
    end: Int,
    listener: LinkInteractionListener?,
) {
    if (end <= start) return
    resolved.link?.let { href ->
        // `styles = null`: the visuals already live in the run's own SpanStyle,
        // where a document colour mark can override the link ink. Link styles
        // would be layered on top and win.
        addLink(LinkAnnotation.Url(href, null, listener), start, end)
    }
    resolved.commentId?.let { addStringAnnotation(PmInlineRenderer.COMMENT_TAG, it, start, end) }
    resolved.revision()?.let {
        addStringAnnotation(PmInlineRenderer.TRACK_CHANGE_TAG, it.kind.id, start, end)
    }
    resolved.unknown?.forEach {
        addStringAnnotation(PmInlineRenderer.UNKNOWN_MARK_TAG, it, start, end)
    }
}

/** Shown when a note call somehow lost its number — never nothing. */
private const val NOTE_FALLBACK_MARKER = "*"

private const val IMAGE_PLACEHOLDER_ALT = "Image"

/** Square brackets, not a symbol: they render on every device font. */
private fun atomMarker(type: String): String = "[${type.ifBlank { "?" }}]"

/**
 * The inline slots [buildAnnotated] reserved for picture atoms, keyed the same
 * way (the index in [inlines], so the two walks cannot drift apart).
 *
 * Separate from the builder because a slot needs a density — a [Placeholder] is
 * sized in sp — and a composable to draw, neither of which belongs in a pure
 * function.
 */
@Composable
fun pmInlineContent(
    inlines: List<PmInline>,
    base: TextStyle,
): Map<String, InlineTextContent> {
    val density = LocalDensity.current
    val lineHeight = base.fontSize.takeIf { it.isSpecified } ?: DocsType.Body.fontSize
    val slots = mutableMapOf<String, InlineTextContent>()
    inlines.forEachIndexed { index, inline ->
        val atom = inline as? PmInline.Atom ?: return@forEachIndexed
        val image = atom.image ?: return@forEachIndexed
        val declaredW = image.width.takeIf { it.isFinite() && it > 0f }
        val declaredH = image.height.takeIf { it.isFinite() && it > 0f }
        val size: Pair<TextUnit, TextUnit> = if (declaredW == null) {
            // No declared box: a square the height of the line, so the picture
            // takes the room a character would instead of a made-up default.
            lineHeight to lineHeight
        } else {
            val w = DocsPage.px(declaredW).coerceIn(MinInlineImage, MaxInlineImage)
            val h = (w * (declaredH ?: declaredW) / declaredW).coerceIn(MinInlineImage, MaxInlineImage)
            with(density) { w.toSp() to h.toSp() }
        }
        slots[PmInlineRenderer.imageSlotId(index)] = InlineTextContent(
            Placeholder(
                width = size.first,
                height = size.second,
                placeholderVerticalAlign = PlaceholderVerticalAlign.TextCenter,
            ),
        ) {
            PmImage(image)
        }
    }
    return slots
}

/** A picture in the flow stays inside the reading column; below 8 dp it is invisible. */
private val MinInlineImage = 8.dp
private val MaxInlineImage = 240.dp

/**
 * The note atoms of a run, in call order.
 *
 * Their TEXT is not part of the flow — the web reserves a block at the bottom of
 * the page (footnotes) or at the end of the document (endnotes) — so a block
 * renderer takes them from here and shows them under the block that calls them.
 */
fun inlineNotes(inlines: List<PmInline>): List<PmInline.Atom> =
    inlines.filterIsInstance<PmInline.Atom>()
        .filter { it.role == PmAtomRole.FOOTNOTE || it.role == PmAtomRole.ENDNOTE }

// ─────────────────────────────────────────────────────────────────────────────
// Mark folding
// ─────────────────────────────────────────────────────────────────────────────

private enum class Script { NONE, SUB, SUPER }

/** The marks of one run, flattened — see the file header for why this exists. */
private class ResolvedRun {
    var bold = false
    var italic = false
    var underline = false
    var strike = false
    var script = Script.NONE
    var color: Color? = null
    var background: Color? = null
    var family: FontFamily? = null
    var fontSizePt: Float? = null
    var link: String? = null
    var commentId: String? = null
    var insertion: PmMark.TrackChange? = null
    var deletion: PmMark.TrackChange? = null
    var unknown: MutableList<String>? = null

    /** Deletion wins over insertion, exactly like `revisionInk` on the web. */
    fun revision(): PmMark.TrackChange? = deletion ?: insertion

    fun spanStyle(baseSize: TextUnit, styling: PmInlineStyling): SpanStyle {
        val explicitSize: TextUnit? =
            fontSizePt?.let { (it * DocsPage.PtToPx * DocsPage.PhoneTypeScale).sp }
        val fontSize: TextUnit = when {
            script != Script.NONE ->
                (explicitSize ?: baseSize) * PmInlineRenderer.SCRIPT_SCALE
            explicitSize != null -> explicitSize
            else -> TextUnit.Unspecified
        }

        val ink = if (styling.revisionInk) revision()?.let {
            PmInlineRenderer.authorInk(it.author)
        } else null

        val decorations = buildList {
            if (underline) add(TextDecoration.Underline)
            if (strike) add(TextDecoration.LineThrough)
            if (link != null && styling.linkUnderline) add(TextDecoration.Underline)
            if (styling.revisionInk) {
                // A run inserted then deleted carries BOTH strokes, as on the web.
                if (insertion != null) add(TextDecoration.Underline)
                if (deletion != null) add(TextDecoration.LineThrough)
            }
        }

        return SpanStyle(
            color = ink ?: color ?: link?.let { styling.linkColor } ?: Color.Unspecified,
            fontSize = fontSize,
            fontWeight = if (bold) FontWeight.Bold else null,
            fontStyle = if (italic) FontStyle.Italic else null,
            fontFamily = family,
            background = background
                ?: commentId?.let { styling.commentBackground }
                ?: Color.Unspecified,
            baselineShift = when (script) {
                Script.SUPER -> PmInlineRenderer.SuperscriptShift
                Script.SUB -> PmInlineRenderer.SubscriptShift
                Script.NONE -> null
            },
            textDecoration = when (decorations.size) {
                0 -> null
                1 -> decorations[0]
                else -> TextDecoration.combine(decorations)
            },
        )
    }
}

private fun resolve(marks: List<PmMark>): ResolvedRun {
    val out = ResolvedRun()
    for (mark in marks) {
        when (mark) {
            PmMark.Bold -> out.bold = true
            PmMark.Italic -> out.italic = true
            PmMark.Underline -> out.underline = true
            PmMark.Strike -> out.strike = true
            // Sub and super are mutually exclusive: the last one in the list wins,
            // which is the order the editor itself resolves them in.
            PmMark.Subscript -> out.script = Script.SUB
            PmMark.Superscript -> out.script = Script.SUPER
            is PmMark.TextColor -> out.color = Color(mark.argb)
            is PmMark.Highlight -> out.background = Color(mark.argb)
            is PmMark.FontFamily -> familyOf(mark.family)?.let { out.family = it }
            is PmMark.FontSize -> out.fontSizePt = mark.points
            is PmMark.Link -> out.link = mark.href
            is PmMark.Comment -> out.commentId = mark.id
            is PmMark.TrackChange -> when (mark.kind) {
                PmChangeKind.INSERTION -> out.insertion = mark
                PmChangeKind.DELETION -> out.deletion = mark
            }
            // The safety net: recorded as an annotation, never a reason to skip
            // the text the mark covers.
            is PmMark.UnknownMark ->
                (out.unknown ?: mutableListOf<String>().also { out.unknown = it }).add(mark.type)
        }
    }
    return out
}

/**
 * Maps a document font name onto one of the platform's generic families.
 *
 * Mobile keeps the system face (prior product decision), so a named desktop font
 * resolves to null and the run simply inherits the base family. Only the three
 * generic shapes are honoured, because they change the MEANING of a run — a
 * monospaced code span or a serif quotation would otherwise read as plain body
 * text.
 */
private fun familyOf(name: String): FontFamily? {
    val n = name.lowercase().trim()
    return when {
        n.contains("mono") || n.contains("courier") || n.contains("consol") -> FontFamily.Monospace
        // "sans-serif" contains "serif"; it must not fall into the serif branch.
        n.contains("sans") -> null
        n.contains("serif") || n.contains("times") || n.contains("georgia") ||
            n.contains("garamond") || n.contains("cambria") || n.contains("palatino") ||
            n.contains("book antiqua") -> FontFamily.Serif
        n.contains("cursive") || n.contains("script") || n.contains("comic") -> FontFamily.Cursive
        else -> null
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Composable
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Paints a run of inlines.
 *
 * Links are real [LinkAnnotation]s, so Compose handles hit-testing itself: no
 * pointer-position arithmetic, and a link stays clickable across a line wrap.
 * When [onLinkClick] is null the href goes to the platform URI handler, guarded
 * because an href copied from a document can be anything at all and a browserless
 * device (or a bad scheme) throws.
 */
@Composable
fun PmText(
    inlines: List<PmInline>,
    style: TextStyle,
    modifier: Modifier = Modifier,
    color: Color = Color.Unspecified,
    styling: PmInlineStyling = pmInlineStyling(),
    textAlign: TextAlign? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
    softWrap: Boolean = true,
    onLinkClick: ((String) -> Unit)? = null,
) {
    val uriHandler = LocalUriHandler.current
    val currentClick by rememberUpdatedState(onLinkClick)
    // Remembered so the callback identity stays stable: it is a cache key of the
    // AnnotatedString below, and a fresh lambda per recomposition would rebuild
    // the whole run on every frame.
    val handler: (String) -> Unit = remember(uriHandler) {
        { url ->
            val external = currentClick
            if (external != null) external(url) else runCatching { uriHandler.openUri(url) }
        }
    }

    val annotated = remember(inlines, style, styling, handler) {
        buildAnnotated(inlines, style, styling, handler)
    }
    val slots = pmInlineContent(inlines, style)

    val ink = when {
        color.isSpecified -> color
        style.color.isSpecified -> style.color
        else -> docsPageInk()
    }

    Text(
        text = annotated,
        modifier = modifier,
        color = ink,
        textAlign = textAlign ?: TextAlign.Unspecified,
        overflow = overflow,
        softWrap = softWrap,
        maxLines = maxLines,
        inlineContent = slots,
        style = style,
    )
}
