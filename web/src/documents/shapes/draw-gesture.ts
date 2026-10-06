// The DRAG ITSELF of draw-to-create: mouse gesture and pointer (finger/pen) gesture.
//
// Split out of `draw-tool.ts`, which keeps what a finished gesture PRODUCES (the
// node attributes and the insertion). Both gestures share the same three
// callbacks and the same geometry; only the input plumbing differs, and it
// differs enough to be worth writing twice rather than behind a flag:
//
//   · the mouse can hold modifiers (Shift = square, Alt = from the centre) and a
//     secondary button, which abandons the gesture;
//   · a finger has neither, but it has FRIENDS: a second finger landing on the
//     page means the user wants to pinch-zoom, and the shape being traced must be
//     abandoned rather than dropped at whatever size the first finger had reached.
//
// Coordinates: everything here is in PAGE-LOCAL document px, i.e. (0, 0) is the
// top-left corner of the SHEET (margins included), 96 dpi, zoom removed.

import { drawBoxFrom, finalizeDrawBox, type DrawBox } from '../../shapes/draw'

/** Below this (document px, both sides) the gesture counts as a click, not a drag. */
export const SHAPE_DRAW_MIN = 8

/** Page-local document px of a client point on a page canvas. */
export function pagePointFromClient(
  canvas: HTMLCanvasElement, zoom: number, clientX: number, clientY: number,
): { x: number; y: number } {
  const r = canvas.getBoundingClientRect()
  const z = zoom || 1
  return { x: (clientX - r.left) / z, y: (clientY - r.top) / z }
}

export interface ShapeDrawHandlers {
  /** Every move: the box to preview (page-local document px). */
  onPreview: (box: DrawBox) => void
  /** Release: the box to turn into a shape. */
  onCommit: (box: DrawBox) => void
  /** Escape / right button: the user gave up drawing, nothing is created. */
  onCancel: () => void
  /**
   * The gesture was INTERRUPTED by another one (a second finger starting a
   * pinch, a system pointer cancel) — the user did not give up drawing, they
   * started doing something else. Same outcome (no shape), but the caller can
   * legitimately keep the tool armed. Defaults to `onCancel`.
   */
  onAbandon?: () => void
}

/**
 * Run one MOUSE draw gesture. Listeners live on `window` so the drag survives
 * leaving the page canvas (and the ghost keeps following the cursor), exactly
 * like the spreadsheet's own draw loop.
 */
export function beginShapeDraw(
  kind: string,
  canvas: HTMLCanvasElement,
  zoom: number,
  e: { clientX: number; clientY: number },
  h: ShapeDrawHandlers,
): void {
  const start = pagePointFromClient(canvas, zoom, e.clientX, e.clientY)
  let box: DrawBox = { x: start.x, y: start.y, w: 0, h: 0 }

  const detach = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
    window.removeEventListener('keydown', onKey, true)
  }
  const onMove = (ev: MouseEvent) => {
    const p = pagePointFromClient(canvas, zoom, ev.clientX, ev.clientY)
    // Shift = square, Alt = grow from the centre — the shared modifiers.
    box = drawBoxFrom(start.x, start.y, p.x, p.y, { square: ev.shiftKey, fromCentre: ev.altKey })
    h.onPreview(box)
  }
  const onUp = (ev: MouseEvent) => {
    detach()
    if (ev.button !== 0) { h.onCancel(); return }
    h.onCommit(finalizeDrawBox(kind, box, start.x, start.y, { minSize: SHAPE_DRAW_MIN }))
  }
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key !== 'Escape') return
    ev.preventDefault(); ev.stopPropagation()
    detach(); h.onCancel()
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  window.addEventListener('keydown', onKey, true)
}

/**
 * Run one TOUCH / PEN draw gesture.
 *
 * Touch pointers are implicitly captured by the element they land on, so the
 * events keep coming (and keep bubbling up to `window`) even when the finger
 * leaves the page — no `setPointerCapture` needed, and none wanted: capturing
 * would not change what we receive, but it would hide the SECOND finger's
 * `pointerdown` from us, and that one is the whole point of `onDown` below.
 *
 * A second pointer CANCELS: the user is starting a pinch-zoom on the page, and
 * committing the half-traced box at that moment would drop an object nobody
 * asked for, right where they were trying to look. Word behaves the same way —
 * a two-finger gesture is navigation, never drawing.
 */
export function beginShapePointerDraw(
  kind: string,
  canvas: HTMLCanvasElement,
  zoom: number,
  e: { clientX: number; clientY: number; pointerId: number },
  h: ShapeDrawHandlers,
): void {
  const id = e.pointerId
  const start = pagePointFromClient(canvas, zoom, e.clientX, e.clientY)
  let box: DrawBox = { x: start.x, y: start.y, w: 0, h: 0 }

  const detach = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', onAbort)
    window.removeEventListener('pointerdown', onDown, true)
    window.removeEventListener('keydown', onKey, true)
  }
  const onMove = (ev: PointerEvent) => {
    if (ev.pointerId !== id) return
    const p = pagePointFromClient(canvas, zoom, ev.clientX, ev.clientY)
    box = drawBoxFrom(start.x, start.y, p.x, p.y)
    h.onPreview(box)
  }
  const onUp = (ev: PointerEvent) => {
    if (ev.pointerId !== id) return
    detach()
    h.onCommit(finalizeDrawBox(kind, box, start.x, start.y, { minSize: SHAPE_DRAW_MIN }))
  }
  const abandon = () => (h.onAbandon ?? h.onCancel)()
  const onAbort = (ev: PointerEvent) => {
    if (ev.pointerId !== id) return
    detach(); abandon()
  }
  // Capture phase: the second finger must abort the trace before anything else
  // (the pinch handler of the scroll container) gets to act on it.
  const onDown = (ev: PointerEvent) => {
    if (ev.pointerId === id) return
    detach(); abandon()
  }
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key !== 'Escape') return
    ev.preventDefault(); ev.stopPropagation()
    detach(); h.onCancel()
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', onAbort)
  window.addEventListener('pointerdown', onDown, true)
  window.addEventListener('keydown', onKey, true)
}
