// DRAW-TO-CREATE for the documents editor — the presentations gesture, generalised.
//
// Picking a geometry in the ribbon no longer INSERTS anything: it ARMS a tool, and
// the next drag on a page draws the shape, with the real geometry growing under the
// cursor (the ghost is painted by `ShapeGhostLayer`). A plain click still produces a
// shape, at the catalogue's default size, centred on the click — an armed tool must
// never do nothing.
//
// Coordinates: everything here is in PAGE-LOCAL document px, i.e. (0, 0) is the
// top-left corner of the SHEET (margins included), 96 dpi, zoom removed. That is
// exactly the frame `posHRel`/`posVRel` = 'page' resolve offsets in, so the box the
// user drew becomes the object's position with no conversion at all.

import type { Editor } from '@tiptap/core'
import { NodeSelection } from '@tiptap/pm/state'
import type { DrawBox } from '../../shapes/draw'
import type { ShapeKind } from '../../shapes/catalog'
import { defaultShapeParams, shapeAlt, shapeSrcOf, type DocShapeParams } from './params'

// The DRAG itself lives in `draw-gesture.ts` (mouse and finger/pen); it is
// re-exported here so callers keep one import for the whole tool.
export {
  SHAPE_DRAW_MIN, pagePointFromClient, beginShapeDraw, beginShapePointerDraw,
  type ShapeDrawHandlers,
} from './draw-gesture'

/** Margins of the page drawn on, in document px (the content box's origin). */
export interface PageMargins { left: number; top: number }

/**
 * Attributes of a shape that was DRAWN (as opposed to inserted in the flow).
 *
 * A freehand tracing only makes sense as a FLOATING object sitting where the user
 * drew it, which is also what Word produces: wrap « in front of text », and a
 * position pinned to the PRINTABLE AREA (`posHRel`/`posVRel` = margin) rather than
 * to the anchor paragraph.
 *
 * Why the printable area and not the paragraph: a paragraph-relative offset has to
 * be measured against an anchor line whose ordinate only exists after the NEXT
 * layout pass, so the shape could only be placed approximately. Against the
 * margins the drawn box IS the offset — the shape lands exactly under the cursor,
 * and the horizontal offset even keeps its meaning in the renderer's own
 * coordinate space (x = 0 is the left of the content area).
 *
 * The anchor node is still inserted next to the text under the pointer, so the
 * object belongs to the right page and travels with a copy/paste of that region.
 * The user can switch to « move with text » from the layout options at any time.
 */
export function drawnShapeAttrs(
  p: DocShapeParams, box: DrawBox, margins: PageMargins,
): Record<string, unknown> {
  const w = Math.max(1, Math.round(box.w))
  const h = Math.max(1, Math.round(box.h))
  return {
    src: shapeSrcOf(p, w, h),
    width: w,
    height: h,
    align: 'left',
    alt: shapeAlt(p),
    wrap: 'front',
    wrapX: Math.round(box.x - margins.left),
    wrapY: Math.round(box.y - margins.top),
    posHRel: 'margin',
    posVRel: 'margin',
    // Pinned to the sheet: the offsets above are page coordinates, so the object
    // must not be re-flowed with the anchor paragraph (Word « Fix position on page »).
    moveWithText: false,
  }
}

/**
 * Insert a drawn shape and select it. `at` is a text position on the page it was
 * drawn on; the block-level anchor is the top-level node containing it, so the
 * object is laid out on that very page.
 */
export function insertDrawnShape(
  ed: Editor, at: number | null, kind: ShapeKind, box: DrawBox, margins: PageMargins,
): void {
  const type = ed.state.schema.nodes.image
  if (!type) return
  const doc = ed.state.doc
  const safe = at == null ? doc.content.size : Math.max(0, Math.min(at, doc.content.size))
  const $p = doc.resolve(safe)
  const insertAt = $p.depth >= 1 ? $p.before(1) : safe
  const node = type.create(drawnShapeAttrs(defaultShapeParams(kind), box, margins))
  const tr = ed.state.tr.insert(insertAt, node)
  try { tr.setSelection(NodeSelection.create(tr.doc, insertAt)) } catch { /* structure moved: keep the caret */ }
  ed.view.dispatch(tr)
  ed.view.focus()
}
