// SHAPES of the whiteboard — painting, drawing gesture and yellow adjustment knobs.
//
// Everything here consumes the shared `shapes/` package (the one the spreadsheet,
// the documents, the slides and the diagrams draw with) so a star drawn on a board
// is the SAME star as in a slide, adjustment handles included. Split out of
// `whiteboard-engine.ts` / `WhiteboardApp.tsx`, which had grown a bespoke copy of
// each of these three concerns.

import { paintShapeView } from './shapes/canvas'
import { drawBoxFrom, finalizeDrawBox, type DrawBox } from './shapes/draw'
import { adjustHandles, adjustFromDrag, hitAdjust } from './shapes/adjust'
import { shapeSvgView } from './shapes/svg-view'
import type { ShapeElement } from './whiteboard-types'

// ── Board look ────────────────────────────────────────────────────────────────

/**
 * Corner radius of the board's plain `rect`, in content px.
 *
 * DELIBERATE divergence from the shared geometry: the whiteboard's rectangle is a
 * sticky-note-like card (Miro/FigJam draw theirs rounded too), and every board
 * ever created here stored `rect` expecting these soft corners. Users who want a
 * parametric radius pick `roundRect` from the gallery — that one is drawn by the
 * shared engine and carries a yellow knob. Kept in ONE constant so the canvas
 * renderer and the SVG export cannot drift apart.
 */
export const BOARD_RECT_RADIUS = 6

// ── Open geometries (lines and connectors) ───────────────────────────────────

/** An open path plus, at each end, a neighbouring point giving its direction. */
interface OpenPath {
  trace: (ctx: CanvasRenderingContext2D) => void
  start: { x: number; y: number; ax: number; ay: number }
  end:   { x: number; y: number; ax: number; ay: number }
}

/** Kinds whose stroke ends with an arrow head. */
const HEAD_AT_END = new Set(['lineArrow', 'lineDouble', 'elbowArrow', 'elbowDoubleArrow', 'curveArrow', 'curveDoubleArrow'])
/** Kinds whose stroke ALSO starts with one. */
const HEAD_AT_START = new Set(['lineDouble', 'elbowDoubleArrow', 'curveDoubleArrow'])

/**
 * The open path of a line-like kind inside its box, mirroring `shapes/svg.ts`:
 * every connector runs from the box's BOTTOM-LEFT corner to its TOP-RIGHT one
 * (`curve` is the exception, a quadratic resting on the bottom edge). Returns null
 * for anything that is not a line — the caller then has nothing of its own to draw.
 */
function openPathFor(kind: string, x: number, y: number, w: number, h: number): OpenPath | null {
  const l = x, t = y, r = x + w, b = y + h, cx = x + w / 2
  switch (kind) {
    case 'line': case 'lineArrow': case 'lineDouble':
      return {
        trace: c => { c.moveTo(l, b); c.lineTo(r, t) },
        start: { x: l, y: b, ax: r, ay: t },
        end:   { x: r, y: t, ax: l, ay: b },
      }
    case 'elbowConnector': case 'elbowArrow': case 'elbowDoubleArrow':
      return {
        trace: c => { c.moveTo(l, b); c.lineTo(cx, b); c.lineTo(cx, t); c.lineTo(r, t) },
        start: { x: l, y: b, ax: cx, ay: b },
        end:   { x: r, y: t, ax: cx, ay: t },
      }
    case 'curveConnector': case 'curveArrow': case 'curveDoubleArrow':
      return {
        trace: c => { c.moveTo(l, b); c.bezierCurveTo(x + w * 0.1, y + h * 0.15, x + w * 0.9, y + h * 0.85, r, t) },
        start: { x: l, y: b, ax: x + w * 0.1, ay: y + h * 0.15 },
        end:   { x: r, y: t, ax: x + w * 0.9, ay: y + h * 0.85 },
      }
    case 'curve':
      return {
        trace: c => { c.moveTo(l, b); c.quadraticCurveTo(cx, t, r, b) },
        start: { x: l, y: b, ax: cx, ay: t },
        end:   { x: r, y: b, ax: cx, ay: t },
      }
  }
  return null
}

/** Filled triangular head at `(tx, ty)`, pointing away from `(ax, ay)`. */
function arrowHead(ctx: CanvasRenderingContext2D, tx: number, ty: number, ax: number, ay: number, size: number) {
  const a = Math.atan2(ty - ay, tx - ax)
  const sp = Math.PI / 7
  ctx.beginPath()
  ctx.moveTo(tx, ty)
  ctx.lineTo(tx - size * Math.cos(a - sp), ty - size * Math.sin(a - sp))
  ctx.lineTo(tx - size * Math.cos(a + sp), ty - size * Math.sin(a + sp))
  ctx.closePath()
  ctx.fill()
}

/**
 * Draw a kind the shared engine has no AREA geometry for: the ten « Traits » of
 * the catalogue (lines, connectors, curves). Before this, such a shape silently
 * painted NOTHING on a board. Unknown kinds fall back to their bounding box, so
 * an object is never invisible.
 *
 * The caller has already set `strokeStyle`/`fillStyle`; only `lineWidth` is
 * adjusted, connectors being thin by convention (as in `shapes/svg.ts`).
 */
export function paintOpenShape(
  ctx: CanvasRenderingContext2D, kind: string,
  x: number, y: number, w: number, h: number, strokeWidth: number,
): void {
  const path = openPathFor(kind, x, y, w, h)
  const lw = Math.max(1.5, Math.min(Math.min(w, h) * 0.045, 3.5), strokeWidth)
  ctx.save()
  ctx.lineWidth = lw
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  if (!path) {
    // No geometry at all for this kind: show the box rather than nothing.
    ctx.setLineDash([6, 4])
    ctx.strokeRect(x, y, w, h)
    ctx.restore()
    return
  }
  ctx.beginPath()
  path.trace(ctx)
  ctx.stroke()
  const head = Math.max(6, lw * 3.2)
  ctx.fillStyle = ctx.strokeStyle
  if (HEAD_AT_END.has(kind)) arrowHead(ctx, path.end.x, path.end.y, path.end.ax, path.end.ay, head)
  if (HEAD_AT_START.has(kind)) arrowHead(ctx, path.start.x, path.start.y, path.start.ax, path.start.ay, head)
  ctx.restore()
}

// ── Painting a board shape ───────────────────────────────────────────────────

/**
 * Paint one shape element, in CONTENT space (the caller applied the viewport and
 * the element's rotation).
 *
 * Routing, in order: the board's own rounded `rect`, then `paintShapeView` — THE
 * shared entry point, which picks the renderer whose adjustment convention matches
 * the yellow knob — then the open geometries above.
 */
export function paintBoardShape(ctx: CanvasRenderingContext2D, shape: ShapeElement): void {
  const { x, y, width: w, height: h, kind, fill, stroke, strokeWidth = 2, adj } = shape
  ctx.save()
  ctx.fillStyle = fill ?? '#BBDEFB'
  ctx.strokeStyle = stroke ?? '#1a73e8'
  ctx.lineWidth = strokeWidth

  if (kind === 'rect') {
    ctx.beginPath()
    ctx.roundRect(x, y, w, h, BOARD_RECT_RADIUS)
    ctx.fill()
    if (strokeWidth > 0) ctx.stroke()
    ctx.restore()
    return
  }

  const painted = paintShapeView(ctx, kind, x, y, w, h, {
    adj,
    stroke: strokeWidth > 0,
    strokeWidth,
    solidFill: fill ?? undefined,
  })
  if (!painted) paintOpenShape(ctx, kind, x, y, w, h, strokeWidth)
  ctx.restore()
}

// ── Vector export ────────────────────────────────────────────────────────────

const escAttr = (s: string) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;')

/**
 * The shape's outline as SVG markup, in its OWN [0,0]→[w,h] space (the caller
 * translates and rotates it). The routing lives in `shapes/svg-view` — the SVG
 * counterpart of `paintShapeView` — so the exported file cannot drift from the
 * canvas: a shape whose yellow knob was dragged is a FRACTION-adjusted native
 * geometry, and feeding those fractions to the preset engine (which reads
 * 1/100000ths) would silently flatten it.
 */
export function boardShapeSvg(shape: ShapeElement): string {
  const { kind, width: w, height: h, adj } = shape
  const fill = shape.fill ?? '#BBDEFB'
  const sw = shape.strokeWidth ?? 2
  const stroke = sw > 0 ? (shape.stroke ?? '#1a73e8') : 'none'

  if (kind === 'rect') {
    // The board's own softly rounded rectangle (see BOARD_RECT_RADIUS).
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">`
      + `<rect x="0" y="0" width="${w}" height="${h}" rx="${BOARD_RECT_RADIUS}" fill="${escAttr(fill)}" stroke="${escAttr(stroke)}" stroke-width="${sw}"/></svg>`
  }
  return shapeSvgView(kind, w, h, { fill, stroke, strokeWidth: sw, adj })
}

// ── Yellow adjustment knobs ──────────────────────────────────────────────────

/** Knob radius in SCREEN px — divided by the zoom, like every other board chrome. */
const KNOB_R = 4.5
/** Grab tolerance in screen px. */
const KNOB_SLOP = 7

export interface ShapeRect { x: number; y: number; w: number; h: number }

export const shapeRect = (s: ShapeElement): ShapeRect => ({ x: s.x, y: s.y, w: s.width, h: s.height })

/**
 * A point expressed in the shape's OWN frame: a rotated shape must have the
 * pointer un-rotated first, otherwise its knobs stay where the shape used to be.
 */
export function toShapeLocal(r: ShapeRect, rotation: number, px: number, py: number): { x: number; y: number } {
  if (!rotation) return { x: px, y: py }
  const cx = r.x + r.w / 2, cy = r.y + r.h / 2
  const a = (-rotation * Math.PI) / 180
  const dx = px - cx, dy = py - cy
  return { x: cx + dx * Math.cos(a) - dy * Math.sin(a), y: cy + dx * Math.sin(a) + dy * Math.cos(a) }
}

/**
 * The yellow knobs of a selected shape, in Office's colours: they reshape the
 * GEOMETRY rather than the box, so they are deliberately a different colour and
 * shape from the blue resize squares. Shapes with no adjustment draw none.
 */
export function renderAdjustHandles(ctx: CanvasRenderingContext2D, shape: ShapeElement, zoom: number): void {
  const r = shapeRect(shape)
  const knobs = adjustHandles(shape.kind, r, shape.adj)
  if (knobs.length === 0) return
  ctx.save()
  // Turn with the shape: the knobs are geometry, not screen furniture.
  if (shape.rotation) {
    const cx = r.x + r.w / 2, cy = r.y + r.h / 2
    ctx.translate(cx, cy)
    ctx.rotate((shape.rotation * Math.PI) / 180)
    ctx.translate(-cx, -cy)
  }
  for (const k of knobs) {
    ctx.beginPath()
    ctx.arc(k.x, k.y, KNOB_R / zoom, 0, Math.PI * 2)
    ctx.fillStyle = '#ffd400'
    ctx.fill()
    ctx.strokeStyle = '#8a6d00'
    ctx.lineWidth = 1 / zoom
    ctx.stroke()
  }
  ctx.restore()
}

/** Index of the knob under a content-space point, or null. */
export function hitAdjustHandle(x: number, y: number, shape: ShapeElement, zoom: number): number | null {
  const r = shapeRect(shape)
  const p = toShapeLocal(r, shape.rotation ?? 0, x, y)
  const i = hitAdjust(shape.kind, r, p.x, p.y, shape.adj, KNOB_SLOP / zoom)
  return i >= 0 ? i : null
}

/** New `adj` array after dragging knob `index` to a content-space point. */
export function adjustShapeFromDrag(shape: ShapeElement, index: number, px: number, py: number): number[] {
  const r = shapeRect(shape)
  const p = toShapeLocal(r, shape.rotation ?? 0, px, py)
  return adjustFromDrag(shape.kind, r, index, p.x, p.y, shape.adj)
}

// ── Draw-to-create gesture ───────────────────────────────────────────────────

/** Live state of a « draw a shape » gesture, in content coordinates. */
export interface DrawGesture {
  startX: number
  startY: number
  curX:   number
  curY:   number
  /** Shift: constrain to a square. */
  square?: boolean
  /** Alt: grow from the centre. */
  fromCentre?: boolean
}

/** Below this (content px) on either side, the gesture counts as a plain click. */
const DRAW_MIN_SIZE = 10

const snapBox = (b: DrawBox, snap: (v: number) => number): DrawBox => ({
  x: snap(b.x), y: snap(b.y), w: snap(b.w), h: snap(b.h),
})

/**
 * The box the gesture currently spans — what the LIVE PREVIEW must paint. The
 * grid snap is applied here as well as on commit, so the shape does not jump when
 * the button is released.
 */
export function gestureBox(g: DrawGesture, snap: (v: number) => number): DrawBox {
  return snapBox(drawBoxFrom(g.startX, g.startY, g.curX, g.curY, { square: g.square, fromCentre: g.fromCentre }), snap)
}

/**
 * The box the gesture BECOMES on release: the drawn one, or a default-sized shape
 * centred on the click when the user merely clicked (clicking with a shape armed
 * used to do nothing at all).
 */
export function gestureFinalBox(kind: string, g: DrawGesture, snap: (v: number) => number): DrawBox {
  const box = drawBoxFrom(g.startX, g.startY, g.curX, g.curY, { square: g.square, fromCentre: g.fromCentre })
  return snapBox(finalizeDrawBox(kind, box, g.startX, g.startY, { minSize: DRAW_MIN_SIZE }), snap)
}
