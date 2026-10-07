package com.kubuno.docs.ui.render

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.kubuno.docs.pm.PmAlign
import com.kubuno.docs.pm.PmCellVAlign
import com.kubuno.docs.pm.PmNode
import com.kubuno.docs.pm.PmParser
import com.kubuno.docs.ui.DocsColors
import com.kubuno.docs.ui.DocsPage
import kotlin.math.max
import kotlin.math.min

/**
 * Table rendering for the document body.
 *
 * The grid geometry is transcribed from the web canvas engine
 * (office/frontend/src/canvas-engine.ts, `layoutTable` / `paintTableBorders`):
 * same cell padding, same minimum row height, same banding rules, same default
 * hairline. Two things are deliberately NOT reproduced, because they belong to a
 * paginated sheet the phone does not draw:
 *   - the autofit column algorithm, which sizes columns from the measured text
 *     of every cell (it needs a text measurer over the whole table before the
 *     first layout pass; on a phone the result would be sub-readable columns
 *     anyway);
 *   - `headerRows`, which only exists to repeat a header across page breaks.
 *
 * Instead, a table keeps its document-px width and scrolls HORIZONTALLY inside
 * its own viewport, so a wide table never drags the reading column sideways.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Metrics (canvas-engine.ts)
// ─────────────────────────────────────────────────────────────────────────────

/** CELL_PAD_X / CELL_PAD_Y: generous horizontally, near-zero vertically. */
private val CellPadX: Dp = DocsPage.px(6)
private val CellPadY: Dp = DocsPage.px(2)

/** MIN_ROW_H. */
private val MinRowHeight: Dp = DocsPage.px(22)

/** MIN_COL_W: the floor of a column whose width the document set itself. */
private val MinExplicitColumnWidth: Dp = DocsPage.px(24)

/**
 * Floor of an evenly distributed column. The web's 24 px floor assumes a 794 px
 * sheet; on a ~360 dp phone it would let a six-column table squeeze every column
 * to a two-character ribbon. A readable minimum is used instead and the overflow
 * is absorbed by the horizontal scroller — an assumed divergence from the web.
 */
private val MinAutoColumnWidth: Dp = 72.dp

/** Upper bound of an auto-distributed table: the web's own text measure. */
private val MaxAutoTableWidth: Dp = DocsPage.MaxColumnWidth

/** Default table hairline of `paintTableBorders` when the document sets none. */
private const val DefaultBorderWidthPx: Float = 1f

/** `tint(accent, .16)` / `tint(accent, .06)` of layoutTable. */
private const val HeaderTintAlpha: Float = 0.16f
private const val StripeTintAlpha: Float = 0.06f

/**
 * Hard ceiling on the number of columns laid out.
 *
 * The grid allocates several arrays of this size and measures one cell per slot,
 * so an aberrant `colspan` in a corrupt or mis-imported body (nothing stops a
 * `colspan` of a billion in the JSON) would turn one table into an
 * OutOfMemoryError as soon as the document is shown. Word itself stops at 63
 * columns, so no real document is truncated by this. [PmParser] clamps the model
 * to the same value; the renderer re-clamps because a [PmNode.Table] can also be
 * built by hand.
 */
private const val MaxColumns: Int = PmParser.MAX_TABLE_COLUMNS

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Draws a [PmNode.Table]. Any other node is ignored rather than rejected: the
 * block renderer dispatches on node type, and a table that lost its identity in
 * a malformed document must not take the screen down with it.
 */
@Composable
fun PmTable(node: PmNode) {
    val table = node as? PmNode.Table ?: return
    if (table.rows.isEmpty()) return

    val colCount = table.colCount.coerceIn(1, MaxColumns)
    val placed = remember(table, colCount) { placeCells(table, colCount) }
    if (placed.isEmpty()) return

    // The office palette is light-only on the desktop, so night mode promotes the
    // core-ui dark references rather than inventing a hue (same rule as DocsDesign).
    val dark = isSystemInDarkTheme()
    val accent = table.accentArgb?.let { Color(it) }
        ?: if (dark) DocsColors.DarkPrimary else DocsColors.Primary
    val borderColor = table.borderArgb?.let { Color(it) }
        ?: if (dark) DocsColors.DarkBorderStrong else DocsColors.BorderStrong

    val style = table.style.lowercase()
    val borderWidthPx = table.borderWidthPx?.takeIf { it.isFinite() && it > 0f } ?: DefaultBorderWidthPx
    // Style "plain" carries no table-wide default border; since this model has no
    // per-cell borders, a plain table is simply borderless.
    val drawBorders = style != "plain"
    val borderWidth = DocsPage.px(borderWidthPx)

    val backgrounds = remember(placed, style, accent) {
        placed.map { cellBackground(it.cell, it.row, style, accent) }
    }

    BoxWithConstraints(Modifier.fillMaxWidth()) {
        // A table nested in another scroller is measured with unbounded width; the
        // cap keeps "distribute evenly" from producing a kilometre-wide grid.
        val available = maxWidth.coerceAtMost(MaxAutoTableWidth)
        val widths = columnWidths(table, colCount, available)
        val total = widths.fold(0.dp) { acc, w -> acc + w }
        val slack = (available - total).coerceAtLeast(0.dp)
        val leading = when (table.align) {
            PmAlign.CENTER -> slack / 2
            PmAlign.RIGHT -> slack
            else -> 0.dp
        }

        Box(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState()),
        ) {
            TableGrid(
                table = table,
                placed = placed,
                widths = widths,
                backgrounds = backgrounds,
                borderColor = borderColor,
                borderWidth = borderWidth,
                drawBorders = drawBorders,
                modifier = Modifier.padding(start = leading),
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grid placement
// ─────────────────────────────────────────────────────────────────────────────

private class PlacedCell(
    val cell: PmNode.TableCell,
    val row: Int,
    val col: Int,
    val colSpan: Int,
    val rowSpan: Int,
)

/**
 * Resolves every cell to a (row, column, spans) slot, mirroring `layoutTable`'s
 * own walk: a cell already flagged `merged` was absorbed by a neighbour and is
 * skipped, a column still covered by a rowspan above is stepped over, and spans
 * are clamped to the grid. Rows of unequal length are therefore fine — a short
 * row leaves its trailing slots empty, a long one drops what does not fit,
 * neither throws.
 */
private fun placeCells(table: PmNode.Table, colCount: Int): List<PlacedCell> {
    val rowCount = table.rows.size
    // Rows still covered by a rowspan, per column; decremented at each row.
    val occupied = IntArray(colCount)
    val out = ArrayList<PlacedCell>()
    table.rows.forEachIndexed { r, row ->
        var c = 0
        for (cell in row.cells) {
            if (cell.merged) continue
            while (c < colCount && occupied[c] > 0) c++
            if (c >= colCount) break
            val colSpan = cell.colspan.coerceIn(1, colCount - c)
            val rowSpan = cell.rowspan.coerceIn(1, max(1, rowCount - r))
            out += PlacedCell(cell, r, c, colSpan, rowSpan)
            for (k in c until c + colSpan) occupied[k] = rowSpan
            c += colSpan
        }
        for (k in 0 until colCount) if (occupied[k] > 0) occupied[k]--
    }
    return out
}

/**
 * Column widths in dp.
 *
 * Explicit widths are honoured as authored (the web respects a hand-dragged
 * column instead of stretching it), which is what makes a wide table scroll.
 * A width list that does not match the grid, or carries junk, is discarded as a
 * whole — a half-applied list is worse than an even grid.
 */
private fun columnWidths(table: PmNode.Table, colCount: Int, available: Dp): List<Dp> {
    val explicit = table.colWidths
        ?.takeIf { list -> list.size == colCount && list.all { it.isFinite() && it > 4f } }
    if (explicit != null) {
        return explicit.map { DocsPage.px(it).coerceAtLeast(MinExplicitColumnWidth) }
    }
    val even = (available / colCount).coerceAtLeast(MinAutoColumnWidth)
    return List(colCount) { even }
}

/**
 * Cell ground: the document's own `cellBg` wins, otherwise the table style bands
 * the first row and, for "striped", every odd row. The tints stay translucent so
 * the same value works over a light and a dark sheet.
 */
private fun cellBackground(
    cell: PmNode.TableCell,
    rowIndex: Int,
    style: String,
    accent: Color,
): Color? {
    cell.backgroundArgb?.let { return Color(it) }
    return when {
        rowIndex == 0 && (style == "header" || style == "striped") -> accent.copy(alpha = HeaderTintAlpha)
        style == "striped" && rowIndex % 2 == 1 -> accent.copy(alpha = StripeTintAlpha)
        else -> null
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout + painting
// ─────────────────────────────────────────────────────────────────────────────

private class CellRect(val x: Float, val y: Float, val w: Float, val h: Float)

private class TableGeometry(
    /** Parallel to the placed-cell list, so a cell and its ground share an index. */
    val cells: List<CellRect>,
    /** Deduplicated border segments, each [x0, y0, x1, y1]. */
    val edges: List<FloatArray>,
)

/**
 * Holds the geometry produced by the measure pass for the draw pass.
 *
 * It is deliberately NOT snapshot state: the value is written while measuring and
 * read while drawing the very same node, and the draw phase always runs after the
 * layout phase of the frame that produced it. Making it a `MutableState` would
 * schedule a recomposition on every measure for no benefit.
 */
private class GeometryHolder {
    var value: TableGeometry? = null
}

@Composable
private fun TableGrid(
    table: PmNode.Table,
    placed: List<PlacedCell>,
    widths: List<Dp>,
    backgrounds: List<Color?>,
    borderColor: Color,
    borderWidth: Dp,
    drawBorders: Boolean,
    modifier: Modifier = Modifier,
) {
    val geometry = remember { GeometryHolder() }
    val rowCount = table.rows.size

    Layout(
        content = {
            for (p in placed) {
                // Clipping is what keeps an over-wide word or image from bleeding into
                // the neighbouring cell: the column width is fixed by the grid, the
                // content is not.
                Box(
                    Modifier
                        .clipToBounds()
                        .padding(horizontal = CellPadX, vertical = CellPadY),
                ) {
                    PmBlocks(p.cell.children)
                }
            }
        },
        modifier = modifier.drawWithContent {
            val g = geometry.value
            if (g != null) {
                for (i in g.cells.indices) {
                    val bg = backgrounds.getOrNull(i) ?: continue
                    val r = g.cells[i]
                    drawRect(color = bg, topLeft = Offset(r.x, r.y), size = Size(r.w, r.h))
                }
            }
            drawContent()
            if (g != null && drawBorders) {
                // A hairline thinner than a device pixel would disappear entirely at
                // small sizes; the web keeps it visible at every zoom for the same reason.
                val stroke = max(borderWidth.toPx(), 1f)
                for (e in g.edges) {
                    drawLine(borderColor, Offset(e[0], e[1]), Offset(e[2], e[3]), strokeWidth = stroke)
                }
            }
        },
    ) { measurables, _ ->
        val colCount = widths.size
        val colX = IntArray(colCount + 1)
        for (i in 0 until colCount) colX[i + 1] = colX[i] + widths[i].roundToPx().coerceAtLeast(0)

        val placeables = measurables.mapIndexed { i, measurable ->
            val p = placed[i]
            val spanWidth = (colX[min(p.col + p.colSpan, colCount)] - colX[p.col]).coerceAtLeast(0)
            measurable.measure(Constraints.fixedWidth(spanWidth))
        }

        // Row heights: the authored height is a MINIMUM, never a cap (the web's
        // "atleast" mode, its default), so content can always grow a row.
        val minRow = MinRowHeight.roundToPx()
        val rowH = IntArray(rowCount) { r ->
            val spec = table.rowHeights?.getOrNull(r)?.takeIf { it.isFinite() && it > 0f }
            max(minRow, spec?.let { DocsPage.px(it).roundToPx() } ?: 0)
        }
        placeables.forEachIndexed { i, placeable ->
            val p = placed[i]
            if (p.rowSpan == 1 && p.row < rowCount) rowH[p.row] = max(rowH[p.row], placeable.height)
        }
        // A spanning cell only grows the LAST row it covers, like layoutTable: the
        // rows above keep the height their own content asked for.
        placeables.forEachIndexed { i, placeable ->
            val p = placed[i]
            if (p.rowSpan > 1) {
                var have = 0
                for (k in p.row until min(p.row + p.rowSpan, rowCount)) have += rowH[k]
                val last = min(p.row + p.rowSpan - 1, rowCount - 1)
                if (last >= 0 && placeable.height > have) rowH[last] += placeable.height - have
            }
        }
        val rowY = IntArray(rowCount + 1)
        for (r in 0 until rowCount) rowY[r + 1] = rowY[r] + rowH[r]

        val rects = ArrayList<CellRect>(placed.size)
        val edges = ArrayList<FloatArray>(if (drawBorders) placed.size * 4 else 0)
        // Adjacent cells share an edge; drawing both would double its thickness and
        // its opacity, so each segment is emitted once, keyed on its coordinates.
        val seen = HashSet<String>()
        for (p in placed) {
            val x0 = colX[p.col]
            val x1 = colX[min(p.col + p.colSpan, colCount)]
            val y0 = rowY[min(p.row, rowCount)]
            val y1 = rowY[min(p.row + p.rowSpan, rowCount)]
            rects += CellRect(
                x = x0.toFloat(),
                y = y0.toFloat(),
                w = (x1 - x0).toFloat(),
                h = (y1 - y0).toFloat(),
            )
            if (!drawBorders) continue
            addEdge(edges, seen, x0, y0, x1, y0)
            addEdge(edges, seen, x0, y1, x1, y1)
            addEdge(edges, seen, x0, y0, x0, y1)
            addEdge(edges, seen, x1, y0, x1, y1)
        }
        geometry.value = TableGeometry(rects, edges)

        val tableWidth = colX[colCount]
        val tableHeight = rowY[rowCount]
        layout(tableWidth, tableHeight) {
            placeables.forEachIndexed { i, placeable ->
                val p = placed[i]
                val top = rowY[min(p.row, rowCount)]
                val spanHeight = rowY[min(p.row + p.rowSpan, rowCount)] - top
                val slack = (spanHeight - placeable.height).coerceAtLeast(0)
                val dy = when (p.cell.vAlign) {
                    PmCellVAlign.CENTER -> slack / 2
                    PmCellVAlign.BOTTOM -> slack
                    PmCellVAlign.TOP -> 0
                }
                // place(), not placeRelative(): the grid coordinates are the ones the
                // backgrounds and borders are painted with, and those are not mirrored,
                // so mirroring the cells under a RTL locale would tear the table apart.
                placeable.place(colX[p.col], top + dy)
            }
        }
    }
}

private fun addEdge(
    out: MutableList<FloatArray>,
    seen: MutableSet<String>,
    x0: Int,
    y0: Int,
    x1: Int,
    y1: Int,
) {
    if (x0 == x1 && y0 == y1) return
    if (!seen.add("$x0:$y0:$x1:$y1")) return
    out += floatArrayOf(x0.toFloat(), y0.toFloat(), x1.toFloat(), y1.toFloat())
}
