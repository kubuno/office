// Adjustment handles (the yellow knobs) for a shape selected in a document.
//
// Same engine as the spreadsheet and the slides (`shapes/adjust`): the knob
// reshapes the geometry — an arrow's head, a rounded rectangle's radius, a star's
// waist, a callout's tail — without resizing the object. Rendered inside the
// existing rotated selection box, so a rotated shape keeps its knobs on the
// geometry and not beside it; the pointer is brought back into the box's frame
// (de-rotation) before being turned into values, exactly like the slides overlay.
//
// WHY THE DRAG ONLY PREVIEWS, AND COMMITS ON RELEASE
// The document is painted from BITMAPS: every distinct `alt` makes the engine
// regenerate an SVG at device resolution, hand it to `getImage()` — a fresh
// `Image` with an ASYNCHRONOUS decode — and cache it by URL. Writing the node on
// every mouse move would therefore (a) paint the grey « not loaded yet »
// placeholder on most frames, i.e. a flicker instead of a preview, and (b) leave
// one image plus one wrap-contour sample per frame in caches that are never
// evicted. So the drag paints the new geometry as a WIREFRAME over the untouched
// shape — which is also what Word and LibreOffice show — and the node (and its
// bitmap) is written exactly once, on release.

import { useRef, useState } from 'react'
import { adjustHandles, adjustFromDrag } from '../../shapes/adjust'
import { paintShapeGhost } from '../../shapes/draw'

export interface ShapeAdjustHandlesProps {
  kind: string
  /** Committed values (from the node), or undefined for the geometry's defaults. */
  adj?: number[]
  /** Box size in SCREEN px — the selection box this overlay lives in. */
  w: number
  h: number
  /** Rotation in degrees, to bring the pointer back into the box's frame. */
  rotation: number
  /** Client-space centre of the box, re-read at every move (the page may scroll). */
  centre: () => { x: number; y: number } | null
  /** Release: the new values to store on the node. */
  onCommit: (adj: number[]) => void
  /** Coarse pointer (finger): bigger knobs, per the platform touch guidance. */
  coarse?: boolean
  label?: string
}

export function ShapeAdjustHandles({
  kind, adj, w, h, rotation, centre, onCommit, coarse, label,
}: ShapeAdjustHandlesProps) {
  // Values being dragged. `null` = nothing in progress, the node's own values show.
  const [preview, setPreview] = useState<number[] | null>(null)
  const previewRef = useRef<number[] | null>(null)
  const cvRef = useRef<HTMLCanvasElement | null>(null)

  const shown = preview ?? adj
  const knobs = w > 0 && h > 0 ? adjustHandles(kind, { x: 0, y: 0, w, h }, shown) : []
  if (!knobs.length) return null

  // The wireframe canvas is inflated by half a box on each side: a callout's tail
  // and a few other geometries legitimately reach OUTSIDE their box.
  const padX = w / 2, padY = h / 2

  const paintPreview = (values: number[] | null) => {
    const cv = cvRef.current
    const ctx = cv?.getContext('2d')
    if (!cv || !ctx) return
    const dpr = typeof window !== 'undefined' ? (window.devicePixelRatio || 1) : 1
    const cw = Math.max(1, Math.round((w + 2 * padX) * dpr))
    const ch = Math.max(1, Math.round((h + 2 * padY) * dpr))
    if (cv.width !== cw || cv.height !== ch) { cv.width = cw; cv.height = ch }
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, cv.width, cv.height)
    if (!values) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    paintShapeGhost(ctx, kind, { x: padX, y: padY, w, h }, { adj: values, lineWidth: 1.5 })
  }

  const startAdjust = (index: number) => (e: React.PointerEvent) => {
    e.preventDefault(); e.stopPropagation()
    const rad = (rotation || 0) * Math.PI / 180
    const onMove = (ev: PointerEvent) => {
      const c = centre()
      if (!c || w <= 0 || h <= 0) return
      let dx = ev.clientX - c.x, dy = ev.clientY - c.y
      if (rad) {
        const cs = Math.cos(-rad), sn = Math.sin(-rad)
        const nx = dx * cs - dy * sn, ny = dx * sn + dy * cs
        dx = nx; dy = ny
      }
      const next = adjustFromDrag(kind, { x: 0, y: 0, w, h }, index, w / 2 + dx, h / 2 + dy, shown)
      previewRef.current = next
      setPreview(next)
      paintPreview(next)
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      document.body.style.userSelect = ''
      const next = previewRef.current
      previewRef.current = null
      setPreview(null)
      paintPreview(null)
      if (next) onCommit(next)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    document.body.style.userSelect = 'none'
  }

  const r = coarse ? 7 : 5
  // TOUCH TARGET. The knob itself must stay small — it sits ON the geometry and a
  // big disc would hide what it is reshaping — so the target is enlarged
  // INVISIBLY instead, by an absolutely positioned child that overflows the knob
  // (the slides overlay does exactly this). 14 px of ink inside a 34 px target,
  // which is the platform minimum; on a mouse the knob is its own target.
  const grab = coarse ? 10 : 0
  return (
    <>
      {/* Wireframe of the geometry being reshaped (empty until a knob is dragged). */}
      <canvas
        ref={cvRef}
        style={{
          position: 'absolute', left: -padX, top: -padY,
          width: w + 2 * padX, height: h + 2 * padY,
          pointerEvents: 'none',
        }}
      />
      {knobs.map(k => (
        <div
          key={`adj-${k.index}`}
          onPointerDown={startAdjust(k.index)}
          title={label}
          style={{
            position: 'absolute', left: k.x - r, top: k.y - r,
            width: r * 2, height: r * 2, borderRadius: '50%',
            background: '#ffd400', border: '1px solid #8a6d00',
            boxShadow: '0 1px 2px rgba(0,0,0,0.18)',
            pointerEvents: 'auto', touchAction: 'none', cursor: 'pointer',
          }}
        >
          {grab > 0 && <span style={{ position: 'absolute', inset: -grab, borderRadius: '50%' }} />}
        </div>
      ))}
    </>
  )
}
