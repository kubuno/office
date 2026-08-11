// DRAW-TO-CREATE, shared by every office sub-editor that lets a user draw a shape.
//
// The gesture the whole suite must offer (the presentations one, generalised): the
// user drags, and SEES THE REAL SHAPE grow under the cursor — not a placeholder
// rectangle, and certainly not a fixed-size shape that just appears on click.
//
// Two pieces, both unit-agnostic so each editor keeps its own coordinate space
// (slide fractions, world px, document px…):
//
//   • `drawBoxFrom()`  — the rubber-band box, with the usual modifiers
//     (Shift = square, Alt = grow from the centre);
//   • `finalizeDrawBox()` — what a gesture becomes on release: the drawn box, or
//     the catalogue's default size when the user merely CLICKED;
//   • `paintShapeGhost()` — the live preview for canvas editors: the real geometry,
//     translucent, falling back to a dashed band for kinds with no area (lines).
//
// Editors that own a real element while drawing (presentations resizes the element
// itself) only need the first two.

import { shapeDefaultSize, type ShapeKind } from './catalog'
import { paintShapeView } from './canvas'
import { getShapeProvider } from './registry'

export interface DrawBox { x: number; y: number; w: number; h: number }

export interface DrawBoxOpts {
  /** Shift: constrain to a square, using the longer side the user dragged. */
  square?: boolean
  /** Alt: treat the start point as the CENTRE instead of a corner. */
  fromCentre?: boolean
}

/**
 * Rubber-band box between the gesture's start point and the cursor. Always
 * normalised (positive w/h), so dragging up-left works like dragging down-right.
 */
export function drawBoxFrom(
  startX: number, startY: number, curX: number, curY: number,
  opts: DrawBoxOpts = {},
): DrawBox {
  let dx = curX - startX
  let dy = curY - startY
  if (opts.square) {
    // The longer side wins, so the shape follows the cursor's dominant direction.
    const m = Math.max(Math.abs(dx), Math.abs(dy))
    dx = m * (dx < 0 ? -1 : 1)
    dy = m * (dy < 0 ? -1 : 1)
  }
  if (opts.fromCentre) {
    return { x: startX - Math.abs(dx), y: startY - Math.abs(dy), w: Math.abs(dx) * 2, h: Math.abs(dy) * 2 }
  }
  return {
    x: Math.min(startX, startX + dx),
    y: Math.min(startY, startY + dy),
    w: Math.abs(dx),
    h: Math.abs(dy),
  }
}

export interface FinalizeOpts {
  /**
   * Below this on EITHER side the gesture counts as a click, not a drag. In the
   * editor's own unit (px for most, a slide fraction for presentations).
   */
  minSize: number
  /**
   * Size a click produces, in the editor's unit. Defaults to the catalogue's
   * default size, which is in px — fraction-based editors must pass their own.
   */
  defaultSize?: { w: number; h: number }
}

/**
 * What the gesture becomes on release: the drawn box when the user really dragged,
 * else a default-sized shape CENTRED on where they clicked. Returning a shape for a
 * plain click matters — clicking with a shape armed must never do nothing.
 */
export function finalizeDrawBox(
  kind: string, box: DrawBox, startX: number, startY: number, opts: FinalizeOpts,
): DrawBox {
  if (box.w >= opts.minSize && box.h >= opts.minSize) return box
  // `kind` is a plain string: editors carry legacy names of their own, and an
  // unknown one simply gets the catalogue's generic default size. A module that
  // REGISTERED the geometry may state its own (shapes/registry).
  const def = opts.defaultSize
    ?? getShapeProvider(kind)?.defaultSize
    ?? shapeDefaultSize(kind as ShapeKind)
  return { x: startX - def.w / 2, y: startY - def.h / 2, w: def.w, h: def.h }
}

export interface GhostOpts {
  /** Adjustment values, so the preview matches what the shape will look like. */
  adj?: number[]
  /** Translucent body (default: the project blue at 12 %). */
  fill?: string
  /** Outline colour (default: the project blue). */
  stroke?: string
  /** Outline width, ALREADY divided by the zoom by the caller when needed. */
  lineWidth?: number
}

/**
 * Live preview of the shape being drawn, on a canvas: the real geometry, so the
 * user sees exactly what they are about to get. Kinds with no area (lines,
 * connectors) have no geometry to preview, so they get a dashed band instead —
 * the caller does not have to special-case them.
 */
export function paintShapeGhost(
  ctx: CanvasRenderingContext2D, kind: string, box: DrawBox, opts: GhostOpts = {},
): void {
  if (box.w <= 0 || box.h <= 0) return
  const lw = opts.lineWidth ?? 1.5
  ctx.save()
  ctx.fillStyle = opts.fill ?? 'rgba(26, 115, 232, 0.12)'
  ctx.strokeStyle = opts.stroke ?? '#1a73e8'
  ctx.lineWidth = lw
  const drawn = paintShapeView(ctx, kind, box.x, box.y, box.w, box.h, {
    adj: opts.adj,
    stroke: true,
    strokeWidth: lw,
  })
  if (!drawn) {
    // No area geometry (a line or a connector): show the band it spans instead.
    ctx.setLineDash([6 * lw, 3 * lw])
    ctx.strokeRect(box.x, box.y, box.w, box.h)
  }
  ctx.restore()
}
