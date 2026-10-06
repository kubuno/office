// The SVG counterpart of `canvas.ts` — same routing, other output surface.
//
// `paintShapeView` exists because a shape's adjustment values come in TWO
// conventions (fractions for the handful of native geometries, OOXML preset units
// for the other 138) and mixing them silently flattens the shape. Every surface
// that draws a shape therefore needs that same routing, not just canvases: the
// board's SVG export, the sheet's SVG overlay, any thumbnail.
//
// Without this module each of them re-implements the `FRACTION_KINDS` test, and
// they drift apart — which is exactly how an adjusted rounded rectangle ends up
// correct on screen and flat in the exported file.

import { shapeSvg } from './svg'
import { shadeColour } from './paths'
import type { ShapeKind } from './catalog'
import { SHAPE_ADJUSTMENTS, adjValues } from './adjust'
import { shapeGeometry, asNative, hasNativeGeometry } from './native-geometry'
import { getShapeProvider } from './registry'

/** Kinds whose adjustment values are FRACTIONS (hand-written, not preset units). */
const FRACTION_KINDS = new Set(Object.keys(SHAPE_ADJUSTMENTS))

/** True when `shapeOutline` owns this kind (fraction adjustments, own geometry). */
export function isFractionKind(kind: string): boolean {
  return FRACTION_KINDS.has(kind) && hasNativeGeometry(kind)
}

export interface ShapeOutline {
  /** SVG path data in the shape's own [0,0]→[w,h] box. */
  d: string
  /** False for open geometries (a line): stroke it, never fill it. */
  closed: boolean
}

/**
 * Outline of a FRACTION-adjusted native geometry, or null when the kind belongs to
 * the preset engine. Callers that build their own `<path>` (React components with
 * their own attributes) use this; callers that just want markup use `shapeSvgView`.
 *
 * @param inset half the outline width, so the stroke stays inside the box.
 */
export function shapeOutline(
  kind: string, w: number, h: number, inset = 0, adj?: number[],
): ShapeOutline | null {
  if (!isFractionKind(kind)) return null
  const { d, closed } = shapeGeometry(asNative(kind), w, h, inset, adj)
  return { d, closed }
}

export interface ShapeSvgViewOpts {
  fill?: string
  /** Outline colour, or 'none'. */
  stroke?: string
  /** Outline width in px (NOT the fraction `shapeSvg` takes — converted here). */
  strokeWidth?: number
  adj?: number[]
}

/**
 * A shape as a complete, self-contained `<svg>` string — routed the same way
 * `paintShapeView` routes a canvas, so an adjusted shape looks identical whether
 * it is painted, exported or thumbnailed.
 */
export function shapeSvgView(
  kind: string, w: number, h: number, opts: ShapeSvgViewOpts = {},
): string {
  const fill = opts.fill ?? '#dbe7ff'
  const sw = opts.strokeWidth ?? 0
  const stroke = sw > 0 ? (opts.stroke ?? '#1a73e8') : 'none'

  // A geometry contributed by a module (shapes/registry) comes FIRST — same order
  // as `paintShapeView`, so the two surfaces never disagree.
  const provided = getShapeProvider(kind)
  if (provided) {
    const body = provided.geometry(w, h, opts.adj).paths.map(sp => {
      const f = sp.fill === false ? 'none'
        : (sp.shade != null && sp.shade !== 1 ? shadeColour(fill, sp.shade) : fill)
      const st = sp.stroke === false ? 'none' : stroke
      return `<path d="${sp.d}" fill="${f}" stroke="${st}" stroke-width="${sw}"`
        + ` stroke-linejoin="round" stroke-linecap="round"/>`
    }).join('')
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" style="overflow:visible">${body}</svg>`
  }

  const outline = shapeOutline(kind, w, h, stroke === 'none' ? 0 : sw / 2, opts.adj)
  if (outline) {
    // overflow visible: a callout tail legitimately reaches OUTSIDE the box.
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" style="overflow:visible">`
      + `<path d="${outline.d}" fill="${outline.closed ? fill : 'none'}" stroke="${stroke}"`
      + ` stroke-width="${sw}" stroke-linejoin="round" stroke-linecap="round"/></svg>`
  }
  // `shapeSvg` wants the outline width as a FRACTION of the smaller side.
  const swFrac = stroke === 'none' ? 0 : sw / Math.max(1, Math.min(w, h))
  return shapeSvg(kind as ShapeKind, w, h, fill, stroke, swFrac, adjValues(kind, opts.adj))
}
