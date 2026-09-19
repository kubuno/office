package com.kubuno.docs.ui.render

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.kubuno.android.ui.components.KubunoCheckbox
import com.kubuno.docs.pm.PmNode
import com.kubuno.docs.ui.DocsPage
import com.kubuno.docs.ui.DocsType
import com.kubuno.docs.ui.docsPageInk

/**
 * Renders the three list families of a document body — bullet, ordered and task
 * lists — including lists nested inside a list item.
 *
 * Geometry follows the web painter (office/frontend/src/canvas-engine.ts): one
 * nesting level is `LIST_INDENT` = 32 px (`DocsPage.ListIndent`), and the marker
 * is a HANGING one, painted in the gutter to the left of the text rather than
 * inside the text column, so wrapped lines stay aligned under the first one.
 *
 * Marker glyph/format cycles with depth. The paginated canvas painter collapses
 * every level to `•` / `1.` because it re-enters `block()` with a list context
 * that carries no depth, but the editable document itself is a real nested
 * `ul`/`ol` tree and reads with the classic per-level cycle; reproducing the
 * cycle is what makes a three-level outline legible on a phone, where the 32 px
 * step alone is easy to lose track of.
 */

/** canvas-engine.ts gives a list item spaceBefore/spaceAfter = 2 px. */
private val ItemSpacing = DocsPage.px(2)

/** Gap between the hanging marker and the text column, inside the 32 dp gutter. */
private val MarkerGap = 8.dp

/**
 * The checkbox is an 18 dp square while a body line box is ~17 sp; one dp of top
 * padding drops it onto the optical centre of that first line.
 */
private val CheckboxTopPadding = 1.dp

/** Nested `ul` glyphs, in the order a browser cycles disc → circle → square. */
private val BulletGlyphs = listOf("•", "◦", "▪")

/**
 * Draws [node] when it is a list, at nesting level [depth] (0 = outermost).
 *
 * Anything that is not a list is handed back to [PmBlocks] rather than dropped:
 * a caller that mis-routes a node must not lose its content. This cannot recurse,
 * since [PmBlocks] only sends list nodes here.
 */
@Composable
fun PmList(node: PmNode, depth: Int = 0) {
    when (node) {
        is PmNode.BulletList -> ListBody(node.items, depth) { TextMarker(bulletGlyph(depth)) }

        is PmNode.OrderedList -> ListBody(node.items, depth) { index ->
            // `start` is the ProseMirror attribute: a list may legitimately begin
            // at 5, or at 0, so the number is start-relative, never index + 1.
            TextMarker(orderedMarker(depth, node.start + index))
        }

        is PmNode.TaskList -> ListBody(node.items, depth) { index ->
            TaskMarker(node.items[index].checked == true)
        }

        // A bare item reaches here when a malformed body puts one outside a list.
        is PmNode.ListItem -> {
            val checked = node.checked
            ListBody(listOf(node), depth) {
                if (checked != null) TaskMarker(checked) else TextMarker(bulletGlyph(depth))
            }
        }

        else -> PmBlocks(listOf(node))
    }
}

/**
 * The shared item column. [marker] receives the item index so an ordered list can
 * compute its own number while bullet and task lists ignore it.
 */
@Composable
private fun ListBody(
    items: List<PmNode.ListItem>,
    depth: Int,
    marker: @Composable (index: Int) -> Unit,
) {
    if (items.isEmpty()) return
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(ItemSpacing),
    ) {
        items.forEachIndexed { index, item ->
            Row(modifier = Modifier.fillMaxWidth()) {
                // `widthIn(min=)` rather than a fixed width: a deep roman marker
                // ("viii.") is wider than the gutter and must push the text
                // column instead of being clipped.
                Box(
                    modifier = Modifier
                        .widthIn(min = DocsPage.ListIndent)
                        .padding(end = MarkerGap),
                    contentAlignment = Alignment.TopEnd,
                ) {
                    marker(index)
                }
                Column(modifier = Modifier.weight(1f)) {
                    ItemContent(item, depth)
                }
            }
        }
    }
}

/**
 * Content of one item: its blocks go to [PmBlocks], but a nested list is drawn
 * here with `depth + 1`. Routing it through [PmBlocks] instead would restart the
 * marker cycle at level 0, and every level of the outline would look the same.
 */
@Composable
private fun ItemContent(item: PmNode.ListItem, depth: Int) {
    for (chunk in chunkItem(item.children)) {
        when (chunk) {
            is ItemChunk.Blocks -> PmBlocks(chunk.nodes)
            is ItemChunk.Nested -> PmList(chunk.node, depth + 1)
        }
    }
}

private sealed interface ItemChunk {
    data class Blocks(val nodes: List<PmNode>) : ItemChunk
    data class Nested(val node: PmNode) : ItemChunk
}

private fun isListNode(node: PmNode): Boolean =
    node is PmNode.BulletList || node is PmNode.OrderedList || node is PmNode.TaskList

/**
 * Splits an item's children into runs of ordinary blocks and single nested lists.
 * Grouping the ordinary blocks keeps them in ONE [PmBlocks] call, so that
 * renderer can still apply its own inter-block spacing between them.
 */
private fun chunkItem(children: List<PmNode>): List<ItemChunk> {
    val out = mutableListOf<ItemChunk>()
    val run = mutableListOf<PmNode>()
    for (child in children) {
        if (isListNode(child)) {
            if (run.isNotEmpty()) {
                out += ItemChunk.Blocks(run.toList())
                run.clear()
            }
            out += ItemChunk.Nested(child)
        } else {
            run += child
        }
    }
    if (run.isNotEmpty()) out += ItemChunk.Blocks(run.toList())
    return out
}

@Composable
private fun TextMarker(text: String) {
    Text(
        text = text,
        style = DocsType.Body,
        color = docsPageInk(),
        softWrap = false,
    )
}

@Composable
private fun TaskMarker(checked: Boolean) {
    KubunoCheckbox(
        checked = checked,
        // Read-only for now: editing a task item in place is not implemented, so
        // the callback is a deliberate no-op and the control is left disabled to
        // avoid advertising a tap target that would do nothing.
        onCheckedChange = {},
        modifier = Modifier.padding(top = CheckboxTopPadding),
        enabled = false,
    )
}

private fun bulletGlyph(depth: Int): String = BulletGlyphs[depth.mod(BulletGlyphs.size)]

/** decimal → lower alpha → lower roman, then back to decimal, one step per level. */
private fun orderedMarker(depth: Int, number: Int): String = when (depth.mod(3)) {
    1 -> "${alphaNumeral(number)}."
    2 -> "${romanNumeral(number)}."
    else -> "$number."
}

/** 1 → a, 26 → z, 27 → aa. Out-of-range values keep their digits, never blank. */
private fun alphaNumeral(n: Int): String {
    if (n < 1) return n.toString()
    var rest = n
    val out = StringBuilder()
    while (rest > 0) {
        val rem = (rest - 1) % 26
        out.append('a' + rem)
        rest = (rest - 1) / 26
    }
    return out.reverse().toString()
}

// Same table as DocumentEditorPage.tsx toRoman(), lowercased like its 'roman-lower'.
private val RomanUnits = listOf(
    1000 to "m", 900 to "cm", 500 to "d", 400 to "cd", 100 to "c", 90 to "xc",
    50 to "l", 40 to "xl", 10 to "x", 9 to "ix", 5 to "v", 4 to "iv", 1 to "i",
)

/** Falls back to digits outside 1..3999, exactly like the web's own guard. */
private fun romanNumeral(n: Int): String {
    if (n < 1 || n > 3999) return n.toString()
    var rest = n
    val out = StringBuilder()
    for ((value, glyph) in RomanUnits) {
        while (rest >= value) {
            out.append(glyph)
            rest -= value
        }
    }
    return out.toString()
}
