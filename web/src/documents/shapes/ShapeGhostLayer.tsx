// Live preview of the shape being drawn, over the pages of the document.
//
// The document itself is painted on a <canvas>, so the preview is a second canvas
// laid exactly over a page — nothing is added to the render pipeline, nothing is
// re-laid-out, and the ghost disappears with the component. The geometry is the
// REAL one (shared `paintShapeGhost`): the user sees the star, the arrow or the
// callout grow under the cursor, not a placeholder rectangle.
//
// ONE LAYER PER PAGE, EACH PAINTING THE PART THAT FALLS ON IT
// A drag that leaves the sheet it started on used to be clipped to that sheet,
// because a single layer covered a single page. The parent now mounts one layer
// per page and hands them all the SAME box, expressed in the coordinates of the
// overlay container (document px, zoom removed): each layer paints the portion
// that lands on its own page and ignores the rest. That works for any page
// arrangement — single column or the wrapped grid of the « several pages » view —
// since a layer only ever needs to know its own rectangle.
//
// Each layer bleeds by HALF the gap that separates it from its neighbours, so the
// ghost stays continuous across the join instead of being cut at the paper edge.
// Half, exactly: the bleeds tile and never overlap, and the translucent fill is
// therefore never painted twice over the same pixels (which would show up as a
// darker band between the sheets).
//
// The box is painted IMPERATIVELY through the ref: a drag emits a box per pointer
// move, and pushing that through React state would re-render the whole editor
// sixty times a second for a preview nobody else needs to know about. The backing
// store of a layer is allocated the first time that layer actually has something
// to paint, so a drag that stays on one page costs exactly one extra canvas.

import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'
import { paintShapeGhost, type DrawBox } from '../../shapes/draw'

export interface ShapeGhostHandle {
  /**
   * Paint the box, or clear the layer with `null`.
   *
   * The box is in the OVERLAY CONTAINER's document px: the same frame as
   * `left`/`top` below, divided by the zoom. Page-local coordinates would not do
   * — the very point of the layer is to be able to draw a box that started on
   * another page.
   */
  paint: (box: DrawBox | null) => void
}

export interface ShapeGhostLayerProps {
  /** Position/size of the page canvas, in the overlay's coordinate space (CSS px). */
  left: number
  top: number
  width: number
  height: number
  /** Bleed painted around the page — half the gutter between two pages (CSS px). */
  padX?: number
  padY?: number
  /** Current zoom: the box is expressed in document px, the layer in CSS px. */
  zoom: number
  /** Device pixel ratio — the backing store is sized for it, like the page canvas. */
  dpr: number
  /** Geometry being drawn. */
  kind: string
}

export const ShapeGhostLayer = forwardRef<ShapeGhostHandle, ShapeGhostLayerProps>(
  function ShapeGhostLayer({ left, top, width, height, padX = 0, padY = 0, zoom, dpr, kind }, ref) {
    const cvRef = useRef<HTMLCanvasElement | null>(null)
    const boxRef = useRef<DrawBox | null>(null)
    // The backing store has been allocated (and may therefore hold ink to clear).
    const sizedRef = useRef(false)

    const cssW = width + 2 * padX
    const cssH = height + 2 * padY

    const paint = (box: DrawBox | null) => {
      boxRef.current = box
      const cv = cvRef.current
      if (!cv) return
      const z = zoom || 1
      // This layer's rectangle, in the container's document px.
      const x0 = (left - padX) / z, y0 = (top - padY) / z
      const w0 = cssW / z, h0 = cssH / z
      // Generous margin: a callout's tail and a few other geometries paint
      // slightly outside their box, and the outline has a width of its own.
      const hit = !!box && box.w > 0 && box.h > 0
        && box.x - 8 < x0 + w0 && box.x + box.w + 8 > x0
        && box.y - 8 < y0 + h0 && box.y + box.h + 8 > y0
      if (!hit || !box) {
        // Nothing of ours on this page: erase what was there, but never allocate
        // a backing store for a page the drag has not reached.
        if (!sizedRef.current) return
        const ctx0 = cv.getContext('2d')
        if (!ctx0) return
        ctx0.setTransform(1, 0, 0, 1, 0, 0)
        ctx0.clearRect(0, 0, cv.width, cv.height)
        return
      }
      const bw = Math.max(1, Math.round(cssW * dpr))
      const bh = Math.max(1, Math.round(cssH * dpr))
      if (!sizedRef.current || cv.width !== bw || cv.height !== bh) {
        cv.width = bw; cv.height = bh          // resizing also clears the canvas
        sizedRef.current = true
      }
      const ctx = cv.getContext('2d')
      if (!ctx) return
      ctx.setTransform(1, 0, 0, 1, 0, 0)
      ctx.clearRect(0, 0, cv.width, cv.height)
      ctx.setTransform(dpr * z, 0, 0, dpr * z, 0, 0)
      // Container document px → this layer's own document px.
      const local: DrawBox = { x: box.x - x0, y: box.y - y0, w: box.w, h: box.h }
      // Outline kept at a constant SCREEN thickness, whatever the zoom.
      paintShapeGhost(ctx, kind, local, { lineWidth: 1.5 / z })
    }

    useImperativeHandle(ref, () => ({ paint }))

    // Geometry changed under us (zoom commit, page resize): repaint what we hold.
    useEffect(() => {
      if (sizedRef.current) paint(boxRef.current)
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [left, top, width, height, padX, padY, dpr, zoom, kind])

    return (
      <canvas
        ref={cvRef}
        // The width/height ATTRIBUTES are left alone until the layer actually has
        // something to paint (see `paint`): an untouched page costs no pixels.
        style={{
          position: 'absolute', left: left - padX, top: top - padY, width: cssW, height: cssH,
          zIndex: 28, pointerEvents: 'none',
        }}
      />
    )
  },
)
