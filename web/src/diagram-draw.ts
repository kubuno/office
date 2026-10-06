// DRAW-TO-CREATE in diagrams: the gesture the whole office suite offers, applied
// to the diagram canvas.
//
// Diagrams used to POSE a shape — one click and a fixed-size stencil appeared at
// the centre of the viewport. The presentations gesture is the one users asked to
// see everywhere: arm a shape, then drag on the canvas and WATCH THE REAL SHAPE
// grow under the cursor (Shift = square, Alt = from the centre), a plain click
// still giving a default-sized shape where they clicked.
//
// The gesture itself lives in `shapes/draw` and is shared with the other editors;
// this module only binds it to the diagram's WORLD coordinates and to its grid
// snapping, so the preview and the committed shape land on the very same pixels.

import { drawBoxFrom, finalizeDrawBox, paintShapeGhost, type DrawBox } from './shapes/draw'
import { sharedKindOf } from './diagram-shape-kinds'
import { renderShape, mergeStyle } from './stencils'

/** A draw gesture in progress: the armed stencil and the rubber-band box. */
export interface DrawingShape {
  /** Stencil id being drawn (a diagram id, not necessarily a catalogue kind). */
  kind:   string
  /** Gesture origin, in world coordinates (already snapped). */
  startX: number
  startY: number
  /** Current box, in world coordinates (already snapped). */
  box:    DrawBox
}

export interface DrawModifiers {
  /** Shift: constrain to a square. */
  square?:     boolean
  /** Alt: grow from the centre instead of from the corner. */
  fromCentre?: boolean
}

/** Start a gesture at a world point. */
export function beginDraw(kind: string, wx: number, wy: number, snap: (v: number) => number): DrawingShape {
  const x = snap(wx), y = snap(wy)
  return { kind, startX: x, startY: y, box: { x, y, w: 0, h: 0 } }
}

/**
 * Move the gesture to a world point. Both ends are snapped BEFORE the box is
 * built, so the preview shows exactly the box that will be committed — no jump
 * when the button is released.
 */
export function updateDraw(
  st: DrawingShape, wx: number, wy: number, mods: DrawModifiers, snap: (v: number) => number,
): DrawingShape {
  const box = drawBoxFrom(st.startX, st.startY, snap(wx), snap(wy), mods)
  return { ...st, box }
}

/**
 * What the gesture becomes on release: the drawn box, or a default-sized shape
 * CENTRED on the click when the user merely clicked (the drag never really
 * started). `minSize` is in world pixels — the caller divides by the zoom so the
 * threshold is a constant number of SCREEN pixels.
 */
export function finishDraw(
  st: DrawingShape, opts: { minSize: number; defaultSize: { w: number; h: number }; snap: (v: number) => number },
): DrawBox {
  const box = finalizeDrawBox(st.kind, st.box, st.startX, st.startY, {
    minSize: opts.minSize, defaultSize: opts.defaultSize,
  })
  return { x: opts.snap(box.x), y: opts.snap(box.y), w: Math.round(box.w), h: Math.round(box.h) }
}

/** Neutral caption style: the preview draws no label, but `renderShape` asks for one. */
const GHOST_LABEL_STYLE = {
  fontFamily: 'Inter', fontSize: 12, bold: false, italic: false,
  color: '#1a73e8', align: 'center', verticalAlign: 'middle',
}

/**
 * Live preview on the canvas (already in world space): the REAL shape being
 * drawn, translucent, so the user sees exactly what they are about to get.
 *
 * Shared geometries go through the suite-wide ghost; the diagrams' own business
 * templates (flowchart, UML, network, hardware icons) are previewed with the very
 * renderer that will draw them, painted see-through. A dashed band frames the box
 * in both cases — it is the only feedback a template with no outline (the plain
 * text stencil) can give.
 */
export function paintDrawGhost(ctx: CanvasRenderingContext2D, st: DrawingShape, zoom: number): void {
  const { box } = st
  if (box.w <= 0 || box.h <= 0) return
  const kind = sharedKindOf(st.kind)
  if (kind) {
    paintShapeGhost(ctx, kind, box, { lineWidth: 1.5 / zoom })
  } else {
    ctx.save()
    ctx.globalAlpha = 0.6
    renderShape(
      ctx, st.kind, box.x, box.y, box.w, box.h,
      mergeStyle({ fillColor: 'rgba(26,115,232,0.12)', strokeColor: '#1a73e8', strokeWidth: 1.5 / zoom }),
      '', GHOST_LABEL_STYLE,
    )
    ctx.restore()
  }
  ctx.save()
  ctx.strokeStyle = 'rgba(26, 115, 232, 0.55)'
  ctx.lineWidth = 1 / zoom
  ctx.setLineDash([4 / zoom, 3 / zoom])
  ctx.strokeRect(box.x, box.y, box.w, box.h)
  ctx.restore()
}
