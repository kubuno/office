package com.kubuno.docs.pm

import kotlinx.serialization.json.JsonObject

/**
 * The Android-side model of an office "document" body.
 *
 * The server stores document content as ProseMirror JSON, either bare
 * (`{type:"doc",content:[...]}`) or inside the editor's layout envelope
 * (`{_type:"multi-page", sections, pages, header, footer, ...}`). Both shapes are
 * flattened into a single [PmDoc] by `PmParser.parseDocument`.
 *
 * Nothing here touches Android or Compose on purpose: the model and its parser
 * stay unit-testable on the JVM, and the renderer is free to map it to whatever
 * the screen needs.
 *
 * Lengths are in the document's own unit — CSS pixels at 96 dpi, exactly like the
 * web editor's canvas — EXCEPT font sizes, which the web stores and reads as
 * points. Colors are packed ARGB ints, directly usable by `Color(argb)`.
 */

/** Paper formats the office module offers. Dimensions are portrait, in cm. */
enum class PmPaperSize(val id: String, val widthCm: Double, val heightCm: Double) {
    A4("a4", 21.0, 29.7),
    A5("a5", 14.8, 21.0),
    A3("a3", 29.7, 42.0),
    LETTER("letter", 21.59, 27.94),
    LEGAL("legal", 21.59, 35.56);

    companion object {
        /** Unknown or missing ids fall back to A4, the editor's own default. */
        fun fromId(id: String?): PmPaperSize = PmPaperSize.entries.firstOrNull { it.id == id } ?: A4
    }
}

enum class PmOrientation(val id: String) {
    PORTRAIT("portrait"),
    LANDSCAPE("landscape");

    companion object {
        fun fromId(id: String?): PmOrientation = if (id == "landscape") LANDSCAPE else PORTRAIT
    }
}

/** Where the automatic page number is painted, or [NONE]. */
enum class PmPageNumbers(val id: String) {
    NONE("none"),
    FOOTER_RIGHT("footer-right"),
    FOOTER_CENTER("footer-center"),
    HEADER_RIGHT("header-right"),
    HEADER_CENTER("header-center");

    companion object {
        fun fromId(id: String?): PmPageNumbers = PmPageNumbers.entries.firstOrNull { it.id == id } ?: NONE
    }
}

enum class PmPageNumberFormat(val id: String) {
    ARABIC("arabic"),
    ROMAN_LOWER("roman-lower"),
    ROMAN_UPPER("roman-upper"),
    ALPHA_LOWER("alpha-lower"),
    ALPHA_UPPER("alpha-upper");

    companion object {
        fun fromId(id: String?): PmPageNumberFormat =
            PmPageNumberFormat.entries.firstOrNull { it.id == id } ?: ARABIC
    }
}

enum class PmAlign(val id: String) {
    LEFT("left"),
    CENTER("center"),
    RIGHT("right"),
    JUSTIFY("justify");

    companion object {
        fun fromId(id: String?): PmAlign = PmAlign.entries.firstOrNull { it.id == id } ?: LEFT
    }
}

/** Vertical placement of a table cell's content. */
enum class PmCellVAlign(val id: String) {
    TOP("top"),
    CENTER("center"),
    BOTTOM("bottom");

    companion object {
        fun fromId(id: String?): PmCellVAlign = PmCellVAlign.entries.firstOrNull { it.id == id } ?: TOP
    }
}

/**
 * How a block image sits relative to the text. Only [INLINE] reserves height in
 * the flow; every other mode floats, which is why the renderer must special-case
 * it instead of stacking the image like a paragraph.
 */
enum class PmImageWrap(val id: String) {
    INLINE("inline"),
    SQUARE("square"),
    TOP_BOTTOM("topBottom"),
    BEHIND("behind"),
    FRONT("front");

    companion object {
        fun fromId(id: String?): PmImageWrap = PmImageWrap.entries.firstOrNull { it.id == id } ?: INLINE
    }

    val isFloating: Boolean get() = this == SQUARE || this == BEHIND || this == FRONT
}

/** Which side of a tracked revision a run belongs to. */
enum class PmChangeKind(val id: String) {
    INSERTION("insertion"),
    DELETION("deletion");

    companion object {
        fun fromId(id: String?): PmChangeKind = if (id == "deletion") DELETION else INSERTION
    }
}

/** Page margins, in document pixels. The editor's factory value is 96 (one inch). */
data class PmMargins(
    val top: Int = DEFAULT,
    val right: Int = DEFAULT,
    val bottom: Int = DEFAULT,
    val left: Int = DEFAULT,
) {
    companion object {
        const val DEFAULT = 96
        val Default = PmMargins()
    }
}

/**
 * A run of the document sharing one page geometry. Section 0 is the document's
 * base section; every [PmNode.SectionBreak] opens the next one.
 */
data class PmSection(
    val id: String = "",
    val orientation: PmOrientation = PmOrientation.PORTRAIT,
    val margins: PmMargins = PmMargins.Default,
    val columns: Int = 1,
)

/**
 * Paragraph-level formatting shared by [PmNode.Paragraph] and [PmNode.Heading].
 * All of it is optional: an absent value means "the renderer's default", never 0.
 */
data class PmBlockStyle(
    val align: PmAlign = PmAlign.LEFT,
    /** Indentation LEVEL (not pixels): the web multiplies it by its list step. */
    val indentLevel: Int = 0,
    val indentLeftPx: Float? = null,
    val indentFirstLinePx: Float? = null,
    val indentRightPx: Float? = null,
    /** Line-height multiplier; the web's own default is 1.15 when absent. */
    val lineHeight: Float? = null,
    val spaceBeforePx: Float? = null,
    val spaceAfterPx: Float? = null,
    /** Named style ("Title", "Heading 1"…). Carried for round-tripping only. */
    val styleName: String? = null,
    val pageBreakBefore: Boolean = false,
) {
    companion object {
        val Default = PmBlockStyle()
    }
}

/**
 * A character-level mark.
 *
 * [UnknownMark] is the safety net, and it is not optional: the server's editor
 * carries far more marks than this client renders (bookmarks, spell language,
 * text effects, reference entries…). An unrecognised mark must leave the text
 * readable, never break the screen.
 */
sealed interface PmMark {
    data object Bold : PmMark
    data object Italic : PmMark
    data object Underline : PmMark
    data object Strike : PmMark
    data object Subscript : PmMark
    data object Superscript : PmMark

    /** Foreground color, packed ARGB. */
    data class TextColor(val argb: Int) : PmMark

    /** Background/highlight color, packed ARGB. */
    data class Highlight(val argb: Int) : PmMark

    data class FontFamily(val family: String) : PmMark

    /** Font size in POINTS — the unit the web stores ("11pt") and measures in. */
    data class FontSize(val points: Float) : PmMark

    data class Link(val href: String) : PmMark

    /** Anchor of a comment thread; the thread itself comes from the comments API. */
    data class Comment(val id: String) : PmMark

    data class TrackChange(
        val kind: PmChangeKind,
        val author: String = "",
        val date: String = "",
        val id: String = "",
    ) : PmMark

    data class UnknownMark(val type: String) : PmMark
}

/**
 * What an inline [PmInline.Atom] is, once recognised — which is what the renderer
 * paints. The ProseMirror node name is kept alongside it whatever the role.
 */
enum class PmAtomRole {
    /** Footnote call: arabic number; the note text sits at the bottom of the page on the web. */
    FOOTNOTE,

    /** Endnote call: lowercase roman number; the notes are collected at the end of the document. */
    ENDNOTE,

    /** A picture sitting IN the text flow (`inlineImage`). */
    IMAGE,

    /** A Word field (PAGE, DATE, REF…): the web paints its cached result as ordinary text. */
    FIELD,

    /** Anything this version does not model: shown as a marker, never dropped. */
    OTHER,
}

/**
 * Inline content: a run of text, or an ATOM.
 *
 * An atom is a leaf node that carries content OF ITS OWN — the text of a footnote
 * or of an endnote (`attrs.text`), a picture, the result of a field. Dropping one
 * deletes something the user wrote, so every inline child of a block becomes
 * either a [Text] or an [Atom]; nothing is skipped.
 */
sealed interface PmInline {
    data class Text(val text: String, val marks: List<PmMark> = emptyList()) : PmInline

    data class Atom(
        /** ProseMirror node name, verbatim ("footnote", "inlineImage", "field"…). */
        val type: String,
        val role: PmAtomRole,
        /**
         * What stands for the atom IN the text flow: a note's call number, a
         * field's result. Empty when the atom shows nothing inline (a picture).
         */
        val label: String = "",
        /** The text the atom CARRIES (a note's text, a picture's alt). May be empty. */
        val text: String = "",
        /** Set when [role] is [PmAtomRole.IMAGE]: the picture to draw. */
        val image: PmNode.Image? = null,
        val marks: List<PmMark> = emptyList(),
        /**
         * The untouched JSON node. A rewrite of the block can then put the atom
         * back exactly as the server sent it instead of re-serializing this model,
         * which knows nothing of the atom's other attributes.
         */
        val source: JsonObject? = null,
    ) : PmInline
}

/**
 * A block-level node.
 *
 * [Unknown] is the safety net for any node type this version does not model. It
 * still carries the flattened text of its subtree, so an unsupported construct
 * shows up as plain readable text instead of vanishing or crashing the screen.
 */
sealed interface PmNode {

    /** Any block whose content is a run of inlines — paragraphs, headings, fallbacks. */
    sealed interface TextBlock : PmNode {
        val inlines: List<PmInline>
    }

    data class Paragraph(
        override val inlines: List<PmInline>,
        val style: PmBlockStyle = PmBlockStyle.Default,
    ) : TextBlock

    data class Heading(
        val level: Int,
        override val inlines: List<PmInline>,
        val style: PmBlockStyle = PmBlockStyle.Default,
    ) : TextBlock

    data class BulletList(val items: List<ListItem>) : PmNode

    data class OrderedList(val start: Int, val items: List<ListItem>) : PmNode

    data class TaskList(val items: List<ListItem>) : PmNode

    /** `checked` is null for a plain list item and non-null for a task item. */
    data class ListItem(val checked: Boolean?, val children: List<PmNode>) : PmNode

    data class Blockquote(val children: List<PmNode>) : PmNode

    data class CodeBlock(val code: String, val language: String? = null) : PmNode

    data object HorizontalRule : PmNode

    /** Forces the following content onto a new page, keeping the section geometry. */
    data object PageBreak : PmNode

    /** Opens a new section: the geometry it carries applies to what FOLLOWS it. */
    data class SectionBreak(
        val orientation: PmOrientation = PmOrientation.PORTRAIT,
        val margins: PmMargins = PmMargins.Default,
        val columns: Int = 1,
    ) : PmNode

    data class Table(
        val rows: List<TableRow>,
        val colCount: Int,
        /** Explicit column widths in px, or null for "distribute evenly". */
        val colWidths: List<Float>? = null,
        val rowHeights: List<Float>? = null,
        /** plain | grid | striped | header — drives borders and banding. */
        val style: String = "grid",
        val accentArgb: Int? = null,
        val align: PmAlign = PmAlign.LEFT,
        /** Rows repeated at the top of each page; 0 for none. */
        val headerRows: Int = 0,
        val borderArgb: Int? = null,
        val borderWidthPx: Float? = null,
    ) : PmNode

    data class TableRow(val cells: List<TableCell>) : PmNode

    data class TableCell(
        val children: List<PmNode>,
        val colspan: Int = 1,
        val rowspan: Int = 1,
        /** True when a neighbouring merge absorbed this cell: keep it, draw nothing. */
        val merged: Boolean = false,
        val backgroundArgb: Int? = null,
        val vAlign: PmCellVAlign = PmCellVAlign.TOP,
    ) : PmNode

    data class Image(
        val src: String,
        val alt: String? = null,
        val width: Float = 0f,
        val height: Float = 0f,
        val align: PmAlign = PmAlign.LEFT,
        val wrap: PmImageWrap = PmImageWrap.INLINE,
    ) : PmNode {
        /**
         * The `alt` attribute doubles as a payload slot for the editor's re-editable
         * objects (`kbshape:`, `kbtext:`, `kbtextrich:`, `kbenvelope:`), so it is
         * only human-readable text when it carries none of those prefixes.
         */
        val altText: String?
            get() = alt?.takeUnless { a -> META_ALT_PREFIXES.any { a.startsWith(it) } }

        private companion object {
            val META_ALT_PREFIXES = listOf("kbshape:", "kbtext:", "kbtextrich:", "kbenvelope:")
        }
    }

    data class Unknown(
        val type: String,
        override val inlines: List<PmInline> = emptyList(),
    ) : TextBlock {
        companion object {
            /**
             * [type] of the block the parser emits when it could not read the body
             * at all (a root that is not an object, an envelope with no `pages`, a
             * document with no `content`). Without it an unreadable body and an
             * empty document look exactly the same on screen, and the reader
             * announces "empty document" over content it simply failed to parse.
             */
            const val UNREADABLE_BODY: String = "kb-unreadable-body"
        }
    }
}

/**
 * A whole document, pages already flattened into one block list with a
 * [PmNode.PageBreak] between them.
 */
data class PmDoc(
    val blocks: List<PmNode>,
    val sections: List<PmSection> = listOf(PmSection()),
    val paperSize: PmPaperSize = PmPaperSize.A4,
    val header: List<PmNode> = emptyList(),
    val footer: List<PmNode> = emptyList(),
    /** Used on even pages when [evenOdd] is set; null means "same as [header]". */
    val headerEven: List<PmNode>? = null,
    val footerEven: List<PmNode>? = null,
    /** True = the first page carries no header and no footer. */
    val firstPageDifferent: Boolean = false,
    val evenOdd: Boolean = false,
    val pageNumbers: PmPageNumbers = PmPageNumbers.NONE,
    val pageNumberFormat: PmPageNumberFormat = PmPageNumberFormat.ARABIC,
    val pageNumberStart: Int = 1,
    val trackChanges: Boolean = false,
    val pageColorArgb: Int? = null,
) {
    /** The base section, i.e. the geometry in force before any section break. */
    val baseSection: PmSection get() = sections.firstOrNull() ?: PmSection()

    val isEmpty: Boolean get() = blocks.all { it.isBlank() }

    companion object {
        /** A single empty paragraph — what a brand-new document looks like. */
        val Empty = PmDoc(blocks = listOf(PmNode.Paragraph(emptyList())))
    }
}

/**
 * What an inline contributes to a plain-text rendering: the characters that stand
 * IN the text flow. For an atom that is its label (a note's call number, a field's
 * result); an atom with no label falls back to the text it carries, so a picture
 * still contributes its alternative text instead of nothing.
 *
 * A note's own TEXT is deliberately not concatenated here: on screen it belongs
 * under the block, not in the middle of the sentence that calls it.
 */
fun PmInline.plainText(): String = when (this) {
    is PmInline.Text -> text
    is PmInline.Atom -> label.ifEmpty { text }
}

/**
 * True when a node paints nothing a reader could see.
 *
 * Whitespace-only text counts as blank, but an atom, a picture, a rule or a table
 * never does: a document whose only content is a picture or a footnote must not be
 * announced as empty. The unreadable-body marker is never blank either — that is
 * the whole point of emitting it.
 */
fun PmNode.isBlank(): Boolean = when (this) {
    is PmNode.Unknown -> type != PmNode.Unknown.UNREADABLE_BODY && inlines.all { it.isBlank() }
    is PmNode.TextBlock -> inlines.all { it.isBlank() }
    is PmNode.BulletList -> items.all { it.isBlank() }
    is PmNode.OrderedList -> items.all { it.isBlank() }
    is PmNode.TaskList -> items.all { it.isBlank() }
    is PmNode.ListItem -> children.all { it.isBlank() }
    is PmNode.Blockquote -> children.all { it.isBlank() }
    is PmNode.CodeBlock -> code.isBlank()
    is PmNode.Table -> false
    is PmNode.TableRow -> cells.all { it.isBlank() }
    is PmNode.TableCell -> children.all { it.isBlank() }
    is PmNode.Image -> false
    PmNode.HorizontalRule -> false
    PmNode.PageBreak, is PmNode.SectionBreak -> true
}

fun PmInline.isBlank(): Boolean = when (this) {
    is PmInline.Text -> text.isBlank()
    is PmInline.Atom -> false
}

/** Concatenated text of a node and its subtree, for previews, search and word counts. */
fun PmNode.plainText(): String = when (this) {
    is PmNode.TextBlock -> inlines.joinToString("") { it.plainText() }
    is PmNode.BulletList -> items.joinToString("\n") { it.plainText() }
    is PmNode.OrderedList -> items.joinToString("\n") { it.plainText() }
    is PmNode.TaskList -> items.joinToString("\n") { it.plainText() }
    is PmNode.ListItem -> children.joinToString("\n") { it.plainText() }
    is PmNode.Blockquote -> children.joinToString("\n") { it.plainText() }
    is PmNode.CodeBlock -> code
    is PmNode.Table -> rows.joinToString("\n") { it.plainText() }
    is PmNode.TableRow -> cells.joinToString("\t") { it.plainText() }
    is PmNode.TableCell -> children.joinToString("\n") { it.plainText() }
    is PmNode.Image -> altText.orEmpty()
    PmNode.HorizontalRule, PmNode.PageBreak -> ""
    is PmNode.SectionBreak -> ""
}

/** Plain text of the whole body, paragraphs separated by newlines. */
fun PmDoc.plainText(): String = blocks.joinToString("\n") { it.plainText() }
