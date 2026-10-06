// Yellow ADJUSTMENT knobs of a diagram shape — the handles Word and LibreOffice
// put on a geometry to reshape it without resizing it: the head of an arrow, the
// corner radius of a rounded rectangle, the waist of a star, the tail of a bubble.
//
// Same knobs, same maths and same look as the spreadsheet's (see `shapes/adjust`):
// only the coordinate space differs — diagrams live in WORLD pixels and paint at a
// zoom, so sizes are divided by the zoom to stay constant on screen.
//
// A shape may be rotated: the knobs belong to its own frame, so the pointer is
// un-rotated before hit-testing and the canvas is rotated before painting —
// otherwise the knobs stay where the shape USED to be.

import { adjustFromDrag, hitAdjust } from './shapes/adjust'
import { sharedKindOf, stencilAdjustHandles } from './diagram-shape-kinds'

/** What these helpers need of a diagram shape (structurally a `DiagramShape`). */
export interface AdjustableShape {
  type:      string
  x:         number
  y:         number
  w:         number
  h:         number
  rotation?: number
  adj?:      number[]
}

const rectOf = (s: AdjustableShape) => ({ x: s.x, y: s.y, w: s.w, h: s.h })

/** Un-rotate a world point into the shape's own (unrotated) frame. */
function toLocal(s: AdjustableShape, px: number, py: number) {
  if (!s.rotation) return { x: px, y: py }
  const cx = s.x + s.w / 2, cy = s.y + s.h / 2
  const a = (-s.rotation * Math.PI) / 180
  const dx = px - cx, dy = py - cy
  return { x: cx + dx * Math.cos(a) - dy * Math.sin(a), y: cy + dx * Math.sin(a) + dy * Math.cos(a) }
}

/** Index of the knob under a world point, or -1. */
export function hitShapeAdjust(s: AdjustableShape, px: number, py: number, slop: number): number {
  const kind = sharedKindOf(s.type)
  if (!kind) return -1
  const p = toLocal(s, px, py)
  return hitAdjust(kind, rectOf(s), p.x, p.y, s.adj, slop)
}

/** New adjustment array after dragging knob `index` to a world point. */
export function shapeAdjustFromDrag(
  s: AdjustableShape, index: number, px: number, py: number,
): number[] | null {
  const kind = sharedKindOf(s.type)
  if (!kind) return null
  const p = toLocal(s, px, py)
  return adjustFromDrag(kind, rectOf(s), index, p.x, p.y, s.adj)
}

/**
 * Paint the knobs of a shape (canvas already in world space). Office's yellow:
 * they reshape the geometry rather than the box, so they are deliberately a
 * different colour and shape from the blue resize handles.
 */
export function paintAdjustHandles(
  ctx: CanvasRenderingContext2D, s: AdjustableShape, zoom: number,
): void {
  const knobs = stencilAdjustHandles(s)
  if (!knobs.length) return
  ctx.save()
  // Turn with the shape: the knobs are geometry, not screen furniture.
  if (s.rotation) {
    const cx = s.x + s.w / 2, cy = s.y + s.h / 2
    ctx.translate(cx, cy); ctx.rotate((s.rotation * Math.PI) / 180); ctx.translate(-cx, -cy)
  }
  ctx.setLineDash([])
  for (const k of knobs) {
    ctx.beginPath()
    ctx.arc(k.x, k.y, 4.5 / zoom, 0, Math.PI * 2)
    ctx.fillStyle = '#ffd400'; ctx.fill()
    ctx.strokeStyle = '#8a6d00'; ctx.lineWidth = 1 / zoom; ctx.stroke()
  }
  ctx.restore()
}
