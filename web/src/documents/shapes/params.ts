// « kbshape: » — the shape description a document carries on its image node.
//
// In the documents editor a shape is NOT a first-class object: it is an `image`
// node whose `src` is an SVG data-URL produced by the shared renderer, and whose
// `alt` holds the parameters that produced it. Shapes therefore inherit the whole
// image machinery for free (selection, resize, rotation, wrapping, anchoring,
// z-order, DOCX export) instead of duplicating it.
//
// WHERE `adj` LIVES — and why here rather than in a ProseMirror attribute:
//   • ONE field to reconcile in Yjs instead of two that can drift apart. `src` and
//     `adj` must always agree (the bitmap is generated FROM the adjustments); two
//     independent attributes let a concurrent edit pair a new `adj` with a stale
//     `src`, painting a geometry nobody asked for.
//   • The DOCX pipeline already round-trips `alt` verbatim as an opaque string, so
//     `avLst` values survive a save/reload with no reader or writer change.
//   • The shared renderer already accepts `adj` as its 7th argument, so the value
//     travels from the payload to the geometry with nothing in between to teach.

import { shapeDefaultSize, type ShapeKind } from '../../shapes/catalog'
import type { ShapeParams } from '../../shapes/svg'
import { shapeSvgView } from '../../shapes/svg-view'

/**
 * The `kbshape:` payload. Extends the shared `ShapeParams` (kind/fill/stroke/sw)
 * with the adjustment values — an addition, so every payload written before this
 * existed still parses, and one written now still reads fine anywhere that only
 * knows the shared fields.
 */
export interface DocShapeParams extends ShapeParams {
  /**
   * Adjustment values (OOXML `avLst`): fractions for the few native geometries,
   * raw preset units otherwise — exactly what `shapes/adjust` produces.
   */
  adj?: number[]
}

export const svgToDataUrl = (svg: string): string =>
  'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg)

/** Serialise the parameters into the `alt` attribute. */
export const shapeAlt = (p: DocShapeParams): string =>
  `kbshape:${encodeURIComponent(JSON.stringify(p))}`

/** Read the parameters back, or null when the alt is not a shape. */
export function parseShapeAlt(alt: string | null | undefined): DocShapeParams | null {
  if (!alt?.startsWith('kbshape:')) return null
  try { return JSON.parse(decodeURIComponent(alt.slice(8))) as DocShapeParams } catch { return null }
}

/**
 * The SVG data-URL of a shape at a given pixel size (adjustments included).
 *
 * Goes through `shapeSvgView`, NEVER `shapeSvg` directly: adjustment values come in
 * two conventions (fractions for the handful of native geometries, OOXML preset
 * units for the other 138) and only the *View router tells them apart. Feeding a
 * fraction straight to the preset engine reads it as 1/100000ths and silently
 * flattens the shape — a rounded rectangle whose knob was dragged came out with
 * SQUARE corners.
 */
export function shapeSrcOf(p: DocShapeParams, w: number, h: number): string {
  const width = Math.max(1, w), height = Math.max(1, h)
  // `sw` is a FRACTION of the smaller side here; `shapeSvgView` takes px.
  const swPx = p.sw != null ? Math.max(0.5, Math.min(width, height) * p.sw) : Math.max(1, Math.min(width, height) * 0.0075)
  return svgToDataUrl(shapeSvgView(p.kind, width, height, {
    fill: p.fill, stroke: p.stroke, strokeWidth: swPx, adj: p.adj,
  }))
}

/** Geometries with no area: only the outline carries the colour. */
const STROKE_ONLY = new Set<string>([
  'line', 'lineArrow', 'lineDouble',
  'elbowConnector', 'elbowArrow', 'elbowDoubleArrow',
  'curveConnector', 'curveArrow', 'curveDoubleArrow', 'curve',
])
export const isStrokeOnlyShape = (kind: string): boolean => STROKE_ONLY.has(kind)

/** Office blue, the fill/outline a freshly drawn shape gets. */
export const SHAPE_FILL = '#dbe7ff'
export const SHAPE_STROKE = '#1a73e8'

/** Parameters of a brand-new shape of that geometry. */
export function defaultShapeParams(kind: ShapeKind): DocShapeParams {
  return { kind, fill: isStrokeOnlyShape(kind) ? 'none' : SHAPE_FILL, stroke: SHAPE_STROKE }
}

/** Catalogue default size — what a plain CLICK (no drag) produces. */
export const shapeClickSize = shapeDefaultSize
