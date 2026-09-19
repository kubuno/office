package com.kubuno.docs.pm

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull

/**
 * Turns the server's `content_json` into a [PmDoc].
 *
 * Everything here is a pure function: no Android, no Compose, no I/O — the parser
 * is the piece most likely to meet a document shape nobody anticipated, so it has
 * to be testable on the JVM and it has to be total. No input throws: a missing
 * field, a wrong type or an unheard-of node all degrade, never fail.
 *
 * Node and mark names, and the attribute spellings they carry, are the web
 * editor's (`office/frontend/src/DocumentEditorPage.tsx` for the schema,
 * `canvas-engine.ts` for how each one is actually read). They are not guessed.
 */
object PmParser {

    /** Guards against a pathological nesting depth turning into a stack overflow. */
    private const val MAX_DEPTH = 64

    /** Default highlight when the mark carries no color, as in the web renderer. */
    private val DEFAULT_HIGHLIGHT = 0xFFFFF176.toInt()

    /** Inline code run: the web paints it monospaced on a light grey chip (#f1f3f4). */
    private const val CODE_FONT = "monospace"
    private val CODE_BACKGROUND = 0xFFF1F3F4.toInt()

    /**
     * Ceiling of a table's column count. Word itself stops at 63 columns, so a
     * wider grid means a corrupt or mis-imported body — and the renderer allocates
     * per-column arrays from this number, which a bogus `colspan` would otherwise
     * blow up into an OutOfMemoryError.
     */
    const val MAX_TABLE_COLUMNS: Int = 64

    /**
     * Last-resort display text of a `field`, transcribed from `fieldLabel()` of
     * office/frontend/src/documents/fields.ts. The canvas engine uses it for the
     * very same reason: a field is never rendered as nothing.
     */
    private val FIELD_LABELS = mapOf(
        "page" to "N° de page",
        "pages" to "Nb pages",
        "date" to "Date",
        "title" to "Titre",
        "ref" to "Renvoi",
    )

    /** `fieldLabel('other')`, and the label of any kind this client does not know. */
    private const val FIELD_LABEL_OTHER = "Champ"

    /**
     * The counters that only make sense over a WHOLE document: notes are numbered
     * in document order, exactly like the canvas engine numbers them while it
     * walks the body, so a call shows the same number on both surfaces.
     *
     * Known divergence: the web does not consume a number for a call that is a
     * tracked DELETION while it displays the final document. This client always
     * shows deletions, so it always consumes one.
     */
    private class Notes {
        var footnotes: Int = 0
        var endnotes: Int = 0
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    /**
     * Accepts the three shapes a document body can arrive in:
     *  - the editor's layout envelope `{_type:"multi-page", sections, pages, …}`,
     *  - a bare ProseMirror document `{type:"doc", content:[…]}`,
     *  - the on-disk file envelope `{version:1, content:<either of the above>}`,
     *    which the module normally unwraps server-side but which also reaches a
     *    client that read a raw content file.
     */
    fun parseDocument(content: JsonElement): PmDoc = parseDocument(content, 0)

    /** Convenience for the many places that hold a nullable body. */
    fun parseDocumentOrEmpty(content: JsonElement?): PmDoc =
        content?.let { parseDocument(it, 0) } ?: PmDoc.Empty

    private fun parseDocument(content: JsonElement, unwraps: Int): PmDoc {
        // Not even an object: there is a body, this version just cannot read it.
        val root = content as? JsonObject ?: return unreadableDoc()

        // File envelope: unwrap, then re-enter with the real body. Bounded so a
        // self-nesting payload cannot walk the stack down.
        val type = root["_type"].str()
        if (type == null && root["type"].str() == null && unwraps < 4) {
            val inner = root["content"]
            if (inner is JsonObject) return parseDocument(inner, unwraps + 1)
        }

        val notes = Notes()
        return if (type == "multi-page") parseEnvelope(root, notes) else parseBareDoc(root, notes)
    }

    /**
     * A body whose root shape this version does not know. It carries the marker
     * block rather than no block at all, so the screen can say "unreadable" where
     * it would otherwise say "empty document" over content that does exist.
     */
    private fun unreadableDoc(): PmDoc =
        PmDoc(blocks = listOf(PmNode.Unknown(PmNode.Unknown.UNREADABLE_BODY)))

    // ── Envelope ──────────────────────────────────────────────────────────────

    private fun parseEnvelope(root: JsonObject, notes: Notes): PmDoc {
        val sections = root["sections"].arr()
            ?.mapNotNull { (it as? JsonObject)?.let(::parseSection) }
            ?.takeIf { it.isNotEmpty() }
            ?: listOf(PmSection())

        // Pages are laid end to end with an explicit break between them: that is
        // exactly what the editor shows, and it keeps the renderer from having to
        // know anything about the envelope.
        //
        // `DocEditorScreen.editBlock` walks the very same pages to rewrite a block
        // by index: any change to the walk below (including how a malformed page
        // entry is counted) has to be mirrored there, or an edit lands in the
        // wrong block.
        val blocks = mutableListOf<PmNode>()
        val pages = root["pages"].arr()
        if (pages == null) {
            // The geometry is readable, the BODY is not: keep the page setup and
            // say so, instead of showing an empty sheet.
            blocks += PmNode.Unknown(PmNode.Unknown.UNREADABLE_BODY)
        } else {
            pages.forEach { page ->
                val body = (page as? JsonObject)?.get("content")
                val pageBlocks = blocksOf(body, notes, 0)
                if (blocks.isNotEmpty()) blocks += PmNode.PageBreak
                blocks += pageBlocks
            }
            if (blocks.isEmpty()) blocks += PmNode.Paragraph(emptyList())
        }

        return PmDoc(
            blocks = blocks,
            sections = sections,
            paperSize = PmPaperSize.fromId(root["paperSize"].str()),
            header = headerFooter(root["header"], notes),
            footer = headerFooter(root["footer"], notes),
            headerEven = root["headerEven"].takeIfPresent()?.let { headerFooter(it, notes) },
            footerEven = root["footerEven"].takeIfPresent()?.let { headerFooter(it, notes) },
            firstPageDifferent = root["hfFirstPage"].bool(),
            evenOdd = root["evenOdd"].bool(),
            pageNumbers = PmPageNumbers.fromId(root["pageNumbers"].str()),
            pageNumberFormat = PmPageNumberFormat.fromId(root["pageNumFormat"].str()),
            pageNumberStart = root["pageNumStart"].int() ?: 1,
            trackChanges = root["trackChanges"].bool(),
            pageColorArgb = parseColor(root["pageColor"].str()),
        )
    }

    private fun parseBareDoc(root: JsonObject, notes: Notes): PmDoc {
        // An absent (or non-array) `content` is a root shape this version does not
        // know — an envelope of another kind, a half-written body. An EMPTY array,
        // on the other hand, really is an empty document.
        val children = root["content"].arr() ?: return unreadableDoc()
        val blocks = children.mapNotNull { parseBlock(it, notes, 1) }
            .ifEmpty { listOf(PmNode.Paragraph(emptyList())) }
        return PmDoc(blocks = blocks)
    }

    private fun parseSection(o: JsonObject): PmSection = PmSection(
        id = o["id"].str().orEmpty(),
        orientation = PmOrientation.fromId(o["orientation"].str()),
        margins = marginsOf(o["margins"] as? JsonObject),
        columns = (o["columns"].int() ?: 1).coerceIn(1, 3),
    )

    private fun marginsOf(o: JsonObject?): PmMargins {
        if (o == null) return PmMargins.Default
        return PmMargins(
            top = o["top"].int() ?: PmMargins.DEFAULT,
            right = o["right"].int() ?: PmMargins.DEFAULT,
            bottom = o["bottom"].int() ?: PmMargins.DEFAULT,
            left = o["left"].int() ?: PmMargins.DEFAULT,
        )
    }

    /**
     * Headers and footers went through two legacy shapes before becoming real
     * documents: a plain string, then three aligned zones `{l,c,r}`. Both still
     * live in stored files, so both are migrated here rather than dropped.
     */
    private fun headerFooter(value: JsonElement?, notes: Notes): List<PmNode> = when {
        value == null -> emptyList()
        value is JsonObject && value["type"].str() == "doc" -> blocksOf(value, notes, 0)
        value is JsonObject && (value.containsKey("l") || value.containsKey("c") || value.containsKey("r")) -> {
            listOfNotNull(
                zoneParagraph(value["l"].str(), PmAlign.LEFT),
                zoneParagraph(value["c"].str(), PmAlign.CENTER),
                zoneParagraph(value["r"].str(), PmAlign.RIGHT),
            )
        }
        value is JsonPrimitive -> value.contentOrNull
            ?.takeIf { it.isNotEmpty() }
            ?.let { listOf(PmNode.Paragraph(listOf(PmInline.Text(it)))) }
            ?: emptyList()
        else -> emptyList()
    }

    private fun zoneParagraph(text: String?, align: PmAlign): PmNode? {
        if (text.isNullOrEmpty()) return null
        return PmNode.Paragraph(listOf(PmInline.Text(text)), PmBlockStyle(align = align))
    }

    // ── Blocks ────────────────────────────────────────────────────────────────

    private fun blocksOf(node: JsonElement?, notes: Notes, depth: Int): List<PmNode> {
        val children = (node as? JsonObject)?.get("content").arr() ?: return emptyList()
        return children.mapNotNull { parseBlock(it, notes, depth + 1) }
    }

    private fun parseBlock(element: JsonElement, notes: Notes, depth: Int): PmNode? {
        val o = element as? JsonObject ?: return null
        if (depth > MAX_DEPTH) return PmNode.Unknown("depth-limit", flattenInlines(o, notes, depth))
        val type = o["type"].str() ?: return PmNode.Unknown("", flattenInlines(o, notes, depth))
        val attrs = o["attrs"] as? JsonObject

        return when (type) {
            "paragraph" -> PmNode.Paragraph(inlinesOf(o, notes), blockStyle(attrs))
            "heading" -> PmNode.Heading(
                level = (attrs?.get("level").int() ?: 1).coerceIn(1, 6),
                inlines = inlinesOf(o, notes),
                style = blockStyle(attrs),
            )

            "bulletList" -> PmNode.BulletList(listItemsOf(o, notes, depth))
            "orderedList" -> PmNode.OrderedList(
                start = attrs?.get("start").int() ?: 1,
                items = listItemsOf(o, notes, depth),
            )
            "taskList" -> PmNode.TaskList(listItemsOf(o, notes, depth))
            "listItem", "taskItem" -> parseListItem(o, notes, depth)

            "blockquote" -> PmNode.Blockquote(blocksOf(o, notes, depth))
            "codeBlock" -> PmNode.CodeBlock(
                code = rawText(o),
                language = attrs?.get("language").str()?.takeIf { it.isNotEmpty() },
            )
            "horizontalRule" -> PmNode.HorizontalRule
            "pageBreak" -> PmNode.PageBreak

            // A section break carries the geometry of the section that FOLLOWS it,
            // and its margins live flat on the node (not under a `margins` object).
            "sectionBreak" -> PmNode.SectionBreak(
                orientation = PmOrientation.fromId(attrs?.get("orientation").str()),
                margins = PmMargins(
                    top = attrs?.get("top").int() ?: PmMargins.DEFAULT,
                    right = attrs?.get("right").int() ?: PmMargins.DEFAULT,
                    bottom = attrs?.get("bottom").int() ?: PmMargins.DEFAULT,
                    left = attrs?.get("left").int() ?: PmMargins.DEFAULT,
                ),
                columns = (attrs?.get("columns").int() ?: 1).coerceIn(1, 3),
            )

            "table" -> parseTable(o, attrs, notes, depth)
            "tableRow" -> parseTableRow(o, notes, depth)
            "tableCell", "tableHeader" -> parseTableCell(o, notes, depth)

            "image" -> imageOf(attrs)

            else -> PmNode.Unknown(type, flattenInlines(o, notes, depth))
        }
    }

    private fun imageOf(attrs: JsonObject?): PmNode.Image = PmNode.Image(
        src = attrs?.get("src").str().orEmpty(),
        alt = attrs?.get("alt").str(),
        width = attrs?.get("width").float() ?: 0f,
        height = attrs?.get("height").float() ?: 0f,
        align = PmAlign.fromId(attrs?.get("align").str()),
        wrap = PmImageWrap.fromId(attrs?.get("wrap").str()),
    )

    private fun blockStyle(attrs: JsonObject?): PmBlockStyle {
        if (attrs == null) return PmBlockStyle.Default
        return PmBlockStyle(
            align = PmAlign.fromId(attrs["textAlign"].str()),
            indentLevel = attrs["indent"].int() ?: 0,
            indentLeftPx = attrs["indentLeft"].float(),
            indentFirstLinePx = attrs["indentFirstLine"].float(),
            indentRightPx = attrs["indentRight"].float(),
            lineHeight = attrs["lineHeight"].float(),
            spaceBeforePx = attrs["spaceBefore"].float(),
            spaceAfterPx = attrs["spaceAfter"].float(),
            styleName = attrs["styleName"].str()?.takeIf { it.isNotEmpty() },
            pageBreakBefore = attrs["pageBreakBefore"].bool(),
        )
    }

    private fun listItemsOf(o: JsonObject, notes: Notes, depth: Int): List<PmNode.ListItem> =
        o["content"].arr()
            ?.mapNotNull { child -> (child as? JsonObject)?.let { parseListItem(it, notes, depth + 1) } }
            ?: emptyList()

    private fun parseListItem(o: JsonObject, notes: Notes, depth: Int): PmNode.ListItem {
        // `checked` exists only on a task item; a plain bullet/number keeps it null
        // so the renderer can tell "no checkbox" from "unticked checkbox".
        val checked = (o["attrs"] as? JsonObject)?.get("checked").boolOrNull()
        return PmNode.ListItem(checked, blocksOf(o, notes, depth))
    }

    private fun parseTable(o: JsonObject, attrs: JsonObject?, notes: Notes, depth: Int): PmNode.Table {
        val rows = o["content"].arr()
            ?.mapNotNull { child -> (child as? JsonObject)?.let { parseTableRow(it, notes, depth + 1) } }
            ?: emptyList()
        // The column count is the widest row once merges are counted, which is how
        // the web derives it too — a short row must not shrink the grid. Bounded:
        // the renderer sizes per-column arrays from it, and a corrupt body can
        // carry a row whose spans add up to millions of columns.
        val colCount = rows.maxOfOrNull { row -> row.cells.sumOf { it.colspan } } ?: 1
        val headerRepeat = attrs?.get("headerRepeat").bool()
        return PmNode.Table(
            rows = rows,
            colCount = colCount.coerceIn(1, MAX_TABLE_COLUMNS),
            colWidths = floatList(attrs?.get("colWidths")),
            rowHeights = floatList(attrs?.get("rowHeights")),
            style = attrs?.get("tableStyle").str()?.takeIf { it.isNotEmpty() } ?: "grid",
            accentArgb = parseColor(attrs?.get("accent").str()),
            align = PmAlign.fromId(attrs?.get("tableAlign").str()),
            headerRows = attrs?.get("headerRows").int() ?: if (headerRepeat) 1 else 0,
            borderArgb = parseColor(attrs?.get("tableBorderColor").str()),
            borderWidthPx = attrs?.get("tableBorderWidth").float(),
        )
    }

    private fun parseTableRow(o: JsonObject, notes: Notes, depth: Int): PmNode.TableRow {
        val cells = o["content"].arr()
            ?.mapNotNull { child -> (child as? JsonObject)?.let { parseTableCell(it, notes, depth + 1) } }
            ?: emptyList()
        return PmNode.TableRow(cells)
    }

    private fun parseTableCell(o: JsonObject, notes: Notes, depth: Int): PmNode.TableCell {
        val attrs = o["attrs"] as? JsonObject
        return PmNode.TableCell(
            children = blocksOf(o, notes, depth),
            // Spans are clamped at the source: a single aberrant `colspan` is
            // enough to make the sum of a row overflow into a grid nothing can
            // allocate.
            colspan = (attrs?.get("colspan").int() ?: 1).coerceIn(1, MAX_TABLE_COLUMNS),
            rowspan = (attrs?.get("rowspan").int() ?: 1).coerceIn(1, MAX_TABLE_COLUMNS),
            merged = attrs?.get("merged").bool(),
            backgroundArgb = parseColor(attrs?.get("cellBg").str()),
            vAlign = PmCellVAlign.fromId(attrs?.get("cellVAlign").str()),
        )
    }

    // ── Inlines ───────────────────────────────────────────────────────────────

    private fun inlinesOf(block: JsonObject, notes: Notes): List<PmInline> {
        val children = block["content"].arr() ?: return emptyList()
        val out = mutableListOf<PmInline>()
        for (child in children) {
            val o = child as? JsonObject ?: continue
            inlineOf(o, notes)?.let { out += it }
        }
        return out
    }

    /**
     * One inline child.
     *
     * Returns null ONLY for a node that carries nothing at all (an empty text
     * run). Everything else — including an atom this version does not model —
     * comes back as a value, because every inline atom of the web schema carries
     * content the user wrote: `footnote`/`endnote` hold the TEXT of the note
     * (`attrs.text`, cf. office/frontend/src/documents/endnotes.ts), `inlineImage`
     * a picture, `field` its result. Dropping them, as this parser used to, made a
     * paragraph holding a picture render as a blank area and let an edit erase a
     * footnote the reader had never seen.
     */
    private fun inlineOf(o: JsonObject, notes: Notes): PmInline? {
        val type = o["type"].str().orEmpty()
        val attrs = o["attrs"] as? JsonObject
        return when (type) {
            "text" -> o["text"].str()
                ?.takeIf { it.isNotEmpty() }
                ?.let { PmInline.Text(it, marksOf(o)) }

            // A hard break is a line break and nothing else: turning it into a
            // newline keeps the text readable without a dedicated node type.
            "hardBreak" -> PmInline.Text("\n", marksOf(o))

            // Note calls: the canvas engine numbers them in document order,
            // arabic for a footnote and lowercase roman for an endnote.
            "footnote" -> noteAtom(o, attrs, type, PmAtomRole.FOOTNOTE, (++notes.footnotes).toString())
            "endnote" -> noteAtom(o, attrs, type, PmAtomRole.ENDNOTE, romanLower(++notes.endnotes))

            // `image` is a BLOCK node in the web schema, but a body that puts one
            // inside a paragraph must still show the picture rather than a hole.
            "inlineImage", "image" -> {
                val image = imageOf(attrs)
                PmInline.Atom(
                    type = type,
                    role = PmAtomRole.IMAGE,
                    text = image.altText.orEmpty(),
                    image = image,
                    marks = marksOf(o),
                    source = o,
                )
            }

            // A Word field paints its CACHED result with the formatting of the
            // run, exactly like text. An empty cache falls back to the label of
            // the kind, because a field is never invisible on the web.
            "field" -> PmInline.Atom(
                type = type,
                role = PmAtomRole.FIELD,
                label = fieldDisplay(attrs),
                marks = marksOf(o),
                source = o,
            )

            else -> PmInline.Atom(
                type = type,
                role = PmAtomRole.OTHER,
                // `text` is the attribute both note atoms use for their content,
                // and the one a custom atom is most likely to carry too.
                text = attrs?.get("text").str().orEmpty(),
                marks = marksOf(o),
                source = o,
            )
        }
    }

    private fun noteAtom(
        o: JsonObject,
        attrs: JsonObject?,
        type: String,
        role: PmAtomRole,
        marker: String,
    ): PmInline.Atom = PmInline.Atom(
        type = type,
        role = role,
        label = marker,
        text = attrs?.get("text").str().orEmpty(),
        marks = marksOf(o),
        source = o,
    )

    /**
     * Displayed text of a `field`: its cached result, else the label of its kind.
     *
     * Known divergence: the web RECOMPUTES `page`, `pages` and `date` before
     * falling back to the cache (`fieldText` in documents/fields.ts). This client
     * paginates nothing, so it can only be honest about the cached value.
     */
    private fun fieldDisplay(attrs: JsonObject?): String {
        val cached = attrs?.get("cached").str()
        if (!cached.isNullOrEmpty()) return cached
        return FIELD_LABELS[attrs?.get("kind").str()] ?: FIELD_LABEL_OTHER
    }

    /** `endnoteMarker()` of documents/endnotes.ts — Word's default endnote numbering. */
    private fun romanLower(n: Int): String {
        if (n < 1) return "i"
        val map = listOf(
            1000 to "m", 900 to "cm", 500 to "d", 400 to "cd", 100 to "c", 90 to "xc",
            50 to "l", 40 to "xl", 10 to "x", 9 to "ix", 5 to "v", 4 to "iv", 1 to "i",
        )
        var v = n
        val out = StringBuilder()
        for ((weight, symbol) in map) {
            while (v >= weight) {
                out.append(symbol)
                v -= weight
            }
        }
        return out.toString()
    }

    /**
     * Every inline of a subtree, flattened — the readable fallback of
     * [PmNode.Unknown]. Leaves go through [inlineOf] like a paragraph's own
     * children, so an unmodelled construct cannot swallow the note or the picture
     * it happens to contain.
     */
    private fun flattenInlines(node: JsonObject, notes: Notes, depth: Int): List<PmInline> {
        if (depth > MAX_DEPTH) return emptyList()
        val out = mutableListOf<PmInline>()
        fun walk(o: JsonObject, d: Int, root: Boolean) {
            if (d > MAX_DEPTH) return
            val children = o["content"].arr()
            if (children != null && o["type"].str() != "text") {
                children.forEach { child -> (child as? JsonObject)?.let { walk(it, d + 1, false) } }
                return
            }
            // The node the fallback is built FOR is named by the block renderer
            // itself; turning it into an inline marker would say it twice.
            if (!root) inlineOf(o, notes)?.let { out += it }
        }
        walk(node, depth, root = true)
        return out
    }

    /** Raw text of a block, marks discarded — used for code blocks. */
    private fun rawText(o: JsonObject): String =
        o["content"].arr()
            ?.mapNotNull { (it as? JsonObject)?.takeIf { n -> n["type"].str() == "text" }?.get("text").str() }
            ?.joinToString("")
            ?: ""

    private fun marksOf(node: JsonObject): List<PmMark> {
        val marks = node["marks"].arr() ?: return emptyList()
        val out = mutableListOf<PmMark>()
        for (m in marks) {
            val o = m as? JsonObject ?: continue
            val type = o["type"].str() ?: continue
            val attrs = o["attrs"] as? JsonObject
            when (type) {
                "bold" -> out += PmMark.Bold
                "italic" -> out += PmMark.Italic
                "underline" -> out += PmMark.Underline
                "strike" -> out += PmMark.Strike
                "subscript" -> out += PmMark.Subscript
                "superscript" -> out += PmMark.Superscript

                // Inline code has no mark of its own here: it resolves to the very
                // pair the web renderer applies, so both surfaces look the same.
                "code" -> {
                    out += PmMark.FontFamily(CODE_FONT)
                    out += PmMark.Highlight(CODE_BACKGROUND)
                }

                "highlight" -> out += PmMark.Highlight(
                    parseColor(attrs?.get("color").str()) ?: DEFAULT_HIGHLIGHT
                )

                // textStyle is the catch-all mark of the web schema: it carries the
                // colour, the family and the size at once.
                "textStyle" -> {
                    parseColor(attrs?.get("color").str())?.let { out += PmMark.TextColor(it) }
                    attrs?.get("fontFamily").str()?.takeIf { it.isNotBlank() }
                        ?.let { out += PmMark.FontFamily(it.trim().trim('"', '\'')) }
                    parseFontSize(attrs?.get("fontSize"))?.let { out += PmMark.FontSize(it) }
                }

                "link" -> attrs?.get("href").str()?.takeIf { it.isNotEmpty() }
                    ?.let { out += PmMark.Link(it) }

                "comment" -> attrs?.get("commentId").str()?.takeIf { it.isNotEmpty() }
                    ?.let { out += PmMark.Comment(it) }

                "insertion", "deletion" -> out += PmMark.TrackChange(
                    kind = PmChangeKind.fromId(type),
                    author = attrs?.get("author").str().orEmpty(),
                    date = attrs?.get("date").str().orEmpty(),
                    id = attrs?.get("id").str().orEmpty(),
                )

                else -> out += PmMark.UnknownMark(type)
            }
        }
        return out
    }

    /**
     * Font sizes travel as a CSS-ish string ("11pt"). The web measures whatever
     * number it finds as POINTS regardless of the unit, so this does the same:
     * diverging here would render the same document at two different sizes.
     */
    private fun parseFontSize(value: JsonElement?): Float? {
        val raw = value.str()?.trim() ?: return null
        val digits = raw.takeWhile { it.isDigit() || it == '.' || it == '-' || it == '+' }
        return digits.toFloatOrNull()?.takeIf { it > 0f }
    }

    // ── Colors ────────────────────────────────────────────────────────────────

    /** The CSS colour names the editor's palettes and defaults actually emit. */
    private val NAMED_COLORS = mapOf(
        "transparent" to 0x00000000,
        "black" to 0xFF000000.toInt(),
        "silver" to 0xFFC0C0C0.toInt(),
        "gray" to 0xFF808080.toInt(),
        "grey" to 0xFF808080.toInt(),
        "white" to 0xFFFFFFFF.toInt(),
        "maroon" to 0xFF800000.toInt(),
        "red" to 0xFFFF0000.toInt(),
        "purple" to 0xFF800080.toInt(),
        "fuchsia" to 0xFFFF00FF.toInt(),
        "green" to 0xFF008000.toInt(),
        "lime" to 0xFF00FF00.toInt(),
        "olive" to 0xFF808000.toInt(),
        "yellow" to 0xFFFFFF00.toInt(),
        "navy" to 0xFF000080.toInt(),
        "blue" to 0xFF0000FF.toInt(),
        "teal" to 0xFF008080.toInt(),
        "aqua" to 0xFF00FFFF.toInt(),
        "cyan" to 0xFF00FFFF.toInt(),
        "magenta" to 0xFFFF00FF.toInt(),
        "orange" to 0xFFFFA500.toInt(),
    )

    /**
     * Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()/rgba()` and the basic
     * CSS names into packed ARGB. An unparseable value yields null and the mark is
     * simply not applied — a wrong colour is worse than the default one.
     */
    fun parseColor(value: String?): Int? {
        val raw = value?.trim()?.lowercase() ?: return null
        if (raw.isEmpty()) return null
        NAMED_COLORS[raw]?.let { return it }

        if (raw.startsWith("#")) {
            val hex = raw.substring(1)
            return when (hex.length) {
                3, 4 -> hexToArgb(hex.map { "$it$it" }.joinToString(""))
                6, 8 -> hexToArgb(hex)
                else -> null
            }
        }

        if (raw.startsWith("rgb")) {
            val inside = raw.substringAfter('(', "").substringBefore(')', "")
            if (inside.isEmpty()) return null
            val parts = inside.split(',', '/', ' ').map { it.trim() }.filter { it.isNotEmpty() }
            if (parts.size < 3) return null
            val r = channel(parts[0]) ?: return null
            val g = channel(parts[1]) ?: return null
            val b = channel(parts[2]) ?: return null
            val a = if (parts.size >= 4) alpha(parts[3]) ?: 255 else 255
            return (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        return null
    }

    /** `hex` is 6 (rrggbb) or 8 (rrggbbaa) characters; CSS puts alpha LAST. */
    private fun hexToArgb(hex: String): Int? {
        val r = hex.substring(0, 2).toIntOrNull(16) ?: return null
        val g = hex.substring(2, 4).toIntOrNull(16) ?: return null
        val b = hex.substring(4, 6).toIntOrNull(16) ?: return null
        val a = if (hex.length >= 8) (hex.substring(6, 8).toIntOrNull(16) ?: return null) else 255
        return (a shl 24) or (r shl 16) or (g shl 8) or b
    }

    /** One `rgb()` colour channel: `0..255`, or a percentage of it. */
    private fun channel(token: String): Int? {
        val percent = token.endsWith("%")
        val n = (if (percent) token.dropLast(1) else token).toFloatOrNull() ?: return null
        val v = if (percent) n * 255f / 100f else n
        return v.toInt().coerceIn(0, 255)
    }

    /** The alpha of `rgba()`: `0..1`, or a percentage. */
    private fun alpha(token: String): Int? {
        val percent = token.endsWith("%")
        val n = (if (percent) token.dropLast(1) else token).toFloatOrNull() ?: return null
        val v = if (percent) n / 100f else n
        return (v * 255f).toInt().coerceIn(0, 255)
    }

    // ── Lenient JSON accessors ────────────────────────────────────────────────
    // Kotlinx's own `jsonObject`/`jsonPrimitive` THROW on a type mismatch, which is
    // exactly what this parser must never do. Everything below returns null instead.

    private fun JsonElement?.arr(): JsonArray? = this as? JsonArray

    /** Null for both an absent key and an explicit JSON `null`. */
    private fun JsonElement?.takeIfPresent(): JsonElement? =
        this?.takeUnless { it is JsonNull }

    private fun JsonElement?.str(): String? = (this as? JsonPrimitive)?.contentOrNull

    private fun JsonElement?.float(): Float? = (this as? JsonPrimitive)?.contentOrNull?.toFloatOrNull()

    private fun JsonElement?.int(): Int? = this.float()?.toInt()

    private fun JsonElement?.boolOrNull(): Boolean? {
        val p = this as? JsonPrimitive ?: return null
        p.booleanOrNull?.let { return it }
        // Some stored attributes use "1"/"0" rather than a JSON boolean.
        return when (p.contentOrNull) {
            "1", "true" -> true
            "0", "false" -> false
            else -> null
        }
    }

    private fun JsonElement?.bool(): Boolean = this.boolOrNull() ?: false

    private fun floatList(value: JsonElement?): List<Float>? =
        value.arr()?.mapNotNull { it.float() }?.takeIf { it.isNotEmpty() }
}
