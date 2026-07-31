// GEOMETRY API of the shared shapes — the one every office sub-editor draws with.
//
// `svg.ts` renders the same geometries as SVG markup (documents, spreadsheet
// overlays, galleries). The editors that paint on a CANVAS — presentations,
// whiteboard, diagrams — could not use it: they needed paths, not markup. This
// module exposes the layer underneath, so all five share ONE geometry:
//
//   • `shapePaths()`  → the SVG path data of each sub-path (renderer-agnostic);
//   • `shapeCanvasPaths()` → the same as `Path2D`, ready for ctx.fill/stroke;
//   • `shapeTextBox()` → where text goes inside the shape (OOXML text frame);
//   • `hasShapeGeometry()` / `resolveShapeKind()` → what a legacy editor kind maps to.
//
// Everything comes from LibreOffice's preset data (see preset-engine.ts): a star
// drawn in a slide is now the SAME star as in a sheet or a document, adjustment
// handles included.

import type { ShapeKind } from './catalog'
import { presetOf, presetPath, presetAdjValues, presetTextFrame } from './preset-engine'

/** One sub-path of a geometry: its outline plus how it must be painted. */
export interface ShapeSubPath {
  /** SVG path data — usable as-is in `<path d>` or `new Path2D(d)`. */
  d: string
  fill: boolean
  stroke: boolean
  /**
   * Luminance factor of this sub-path's fill (LibreOffice's DARKEN/LIGHTEN
   * commands: the shaded faces of a cube, the top of a cylinder). 1 = untouched.
   */
  shade: number
}

/**
 * Legacy kind names used by editors written before the shared catalogue, mapped
 * onto their canonical geometry. Old slides and boards keep their stored kind;
 * only the RENDERING is unified.
 */
const KIND_ALIASES: Record<string, ShapeKind> = {
  // Presentations
  rightArrow: 'arrow',
  leftArrow: 'arrowLeft',
  upArrow: 'arrowUp',
  downArrow: 'arrowDown',
  // NB: 'plus' n'est PAS un alias — le catalogue le connaît déjà (croix).
  speech: 'calloutRoundRect',
  // Whiteboard / diagrams
  square: 'rect',
  circle: 'ellipse',
  rectangle: 'rect',
  rounded: 'roundRect',
  oval: 'ellipse',
  process: 'flowProcess',
  decision: 'flowDecision',
  terminator: 'flowTerminator',
  document: 'flowDocument',
  data: 'flowData',
}

/** Canonical kind for an editor's stored value, or null when unknown. */
export function resolveShapeKind(kind: string): ShapeKind | null {
  if (presetOf(kind as ShapeKind)) return kind as ShapeKind
  const alias = KIND_ALIASES[kind]
  return alias && presetOf(alias) ? alias : null
}

/** True when the shared engine can draw this kind (alias included). */
export function hasShapeGeometry(kind: string): boolean {
  return resolveShapeKind(kind) !== null
}

/**
 * Sub-paths of a geometry inside a `w`×`h` box, origin at its top-left corner.
 * Returns null for a kind the engine does not know (lines and connectors, which
 * `svg.ts` draws by hand) — the caller then keeps its own path.
 */
export function shapePaths(kind: string, w: number, h: number, adj?: number[]): ShapeSubPath[] | null {
  const resolved = resolveShapeKind(kind)
  if (!resolved) return null
  const preset = presetOf(resolved)
  if (!preset) return null
  return presetPath(preset, { w, h, adj: presetAdjValues(preset, adj) })
    .map(sp => ({ d: sp.d.trim(), fill: sp.fill, stroke: sp.stroke, shade: sp.shade }))
}

/** The same, as `Path2D` objects — what a canvas renderer wants. */
export function shapeCanvasPaths(
  kind: string, w: number, h: number, adj?: number[],
): { path: Path2D; fill: boolean; stroke: boolean; shade: number }[] | null {
  const subs = shapePaths(kind, w, h, adj)
  if (!subs) return null
  return subs.map(sp => ({ path: new Path2D(sp.d), fill: sp.fill, stroke: sp.stroke, shade: sp.shade }))
}

/**
 * Text frame of the geometry (OOXML `<a:rect>`): where a caption belongs inside
 * the shape — the inner rectangle of a callout, the shaft of an arrow. Falls back
 * to the whole box when the preset declares none.
 */
export function shapeTextBox(
  kind: string, w: number, h: number, adj?: number[],
): { x: number; y: number; w: number; h: number } {
  const resolved = resolveShapeKind(kind)
  const preset = resolved ? presetOf(resolved) : null
  if (!preset) return { x: 0, y: 0, w, h }
  return presetTextFrame(preset, { w, h, adj: presetAdjValues(preset, adj) }) ?? { x: 0, y: 0, w, h }
}

/**
 * Apply a sub-path's shade to a solid colour. Gradients and patterns cannot be
 * shaded, so a canvas renderer only calls this when its fill is a plain colour.
 */
export function shadeColour(hex: string, k: number): string {
  if (k === 1 || !/^#[0-9a-fA-F]{6}$/.test(hex)) return hex
  const c = (o: number) => Math.max(0, Math.min(255, Math.round(parseInt(hex.slice(o, o + 2), 16) * k)))
  return `#${[c(1), c(3), c(5)].map(v => v.toString(16).padStart(2, '0')).join('')}`
}

/**
 * Paint a shape on a canvas: fill and stroke every sub-path, shading the ones
 * LibreOffice marks as darker/lighter. The caller sets `fillStyle`/`strokeStyle`
 * and the transform; `solidFill` (when the fill is a plain colour) enables shading.
 *
 * Returns false when the kind has no shared geometry, so the caller can fall back
 * to its own drawing without a second lookup.
 */
export function paintShape(
  ctx: CanvasRenderingContext2D,
  kind: string, x: number, y: number, w: number, h: number,
  opts: { adj?: number[]; fill?: boolean; stroke?: boolean; solidFill?: string } = {},
): boolean {
  const subs = shapeCanvasPaths(kind, w, h, opts.adj)
  if (!subs) return false
  const doFill = opts.fill !== false
  const doStroke = opts.stroke !== false
  ctx.save()
  ctx.translate(x, y)
  for (const sp of subs) {
    if (doFill && sp.fill) {
      if (opts.solidFill && sp.shade !== 1) {
        const prev = ctx.fillStyle
        ctx.fillStyle = shadeColour(opts.solidFill, sp.shade)
        ctx.fill(sp.path)
        ctx.fillStyle = prev
      } else {
        ctx.fill(sp.path)
      }
    }
    if (doStroke && sp.stroke) ctx.stroke(sp.path)
  }
  ctx.restore()
  return true
}
