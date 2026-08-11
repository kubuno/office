// ONE canvas entry point for every office sub-editor that paints shapes on a
// <canvas> (presentations, whiteboard, diagrams) — the counterpart of the sheet's
// SVG `ShapeGlyph`. It routes each kind to the renderer whose adjustment CONVENTION
// matches the yellow knob, so a knob drag and the painted geometry always agree:
//
//   • the few NATIVE geometries whose knobs use FRACTIONS (roundRect, arrow, star,
//     callout, plus — see shapes/adjust `SHAPE_ADJUSTMENTS`) are drawn from
//     `native-geometry`, which reads those very fractions;
//   • every other catalogue kind goes through `paintShape` (the OOXML preset engine,
//     whose knobs come from the preset — same units on both sides).
//
// Mixing the two — e.g. painting a fraction-adjusted roundRect with the preset
// engine — silently rounds to ~0 (the preset reads 1/100000ths, not 0..0.5), which
// is exactly the bug this indirection prevents.

import { paintShape, shadeColour } from './paths'
import { SHAPE_ADJUSTMENTS } from './adjust'
import { shapeGeometry, asNative, hasNativeGeometry } from './native-geometry'
import { getShapeProvider } from './registry'

/** Kinds whose adjustment values are FRACTIONS (hand-written, not preset units). */
const FRACTION_KINDS = new Set(Object.keys(SHAPE_ADJUSTMENTS))

export interface PaintShapeViewOpts {
  /** Adjustment values (fractions for native kinds, preset units otherwise). */
  adj?: number[]
  /** Fill the geometry (default true). The caller sets `ctx.fillStyle` beforehand. */
  fill?: boolean
  /** Stroke the outline. The caller sets `ctx.strokeStyle`/`lineWidth` beforehand. */
  stroke?: boolean
  /** Outline width in px — only used to inset native geometry so it stays in the box. */
  strokeWidth?: number
  /** Plain fill colour, enabling per-sub-path shading on preset kinds (cube, cylinder). */
  solidFill?: string
}

/**
 * Paint a shape on a canvas. Returns false when no shared geometry exists for the
 * kind (lines/connectors), so the caller can fall back to its own drawing.
 */
export function paintShapeView(
  ctx: CanvasRenderingContext2D,
  kind: string, x: number, y: number, w: number, h: number,
  opts: PaintShapeViewOpts = {},
): boolean {
  // A geometry contributed by a module (shapes/registry) comes FIRST: it is the
  // only one nothing else knows how to draw, and registering an existing id is the
  // documented way for a sub-editor to specialise a shape.
  const provided = getShapeProvider(kind)
  if (provided) {
    const { paths } = provided.geometry(w, h, opts.adj)
    ctx.save()
    ctx.translate(x, y)
    for (const sp of paths) {
      const path = new Path2D(sp.d)
      if (opts.fill !== false && sp.fill !== false) {
        if (opts.solidFill && sp.shade != null && sp.shade !== 1) {
          const prev = ctx.fillStyle
          ctx.fillStyle = shadeColour(opts.solidFill, sp.shade)
          ctx.fill(path)
          ctx.fillStyle = prev
        } else {
          ctx.fill(path)
        }
      }
      if (opts.stroke && sp.stroke !== false) ctx.stroke(path)
    }
    ctx.restore()
    return true
  }
  // Native fraction geometry: the knob (fractions) and this path read the same value.
  if (FRACTION_KINDS.has(kind) && hasNativeGeometry(kind)) {
    const inset = opts.stroke ? (opts.strokeWidth ?? 0) / 2 : 0
    const { d } = shapeGeometry(asNative(kind), w, h, inset, opts.adj)
    const path = new Path2D(d)
    ctx.save()
    ctx.translate(x, y)
    if (opts.fill !== false) ctx.fill(path)
    if (opts.stroke) ctx.stroke(path)
    ctx.restore()
    return true
  }
  // Everything else: the OOXML preset engine (knobs come from the preset too).
  return paintShape(ctx, kind, x, y, w, h, {
    adj: opts.adj,
    fill: opts.fill,
    stroke: opts.stroke,
    solidFill: opts.solidFill,
  })
}
