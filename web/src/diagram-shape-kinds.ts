// Which SHARED geometry a diagram stencil id is painted with — and, above all,
// which ones are painted by the diagrams themselves.
//
// The diagrams registry predates the office-wide shapes catalogue: it draws some
// 150 ids of its own (see `stencils.ts`). Two families live side by side:
//
//  • the diagrams' OWN stencils — the dozen basic geometries ('rect', 'star',
//    'cloud', 'callout'…) plus every business template (flowchart, UML, network,
//    hardware icons, swimlanes, UI mock-ups). They keep their bespoke drawing, so
//    a diagram authored before the catalogue existed is pixel-for-pixel the same
//    today. They have NO adjustment values (no `avLst` behind them), therefore no
//    yellow knob — `sharedKindOf` returns null and the editor draws none.
//
//  • the CATALOGUE shapes, added to the palette by `stencils.ts` (the categories
//    named « Formes · … »). Those are painted by the shared engine, exactly as in
//    a slide, a sheet or a document, and they DO get their yellow knobs.
//
// Division of labour, by design: the house 'star' is the plain five-pointed star
// diagrams have always drawn, with a fixed 0.40 waist. A user who wants an
// ADJUSTABLE star picks one from the shared gallery instead ('star4', 'star6',
// 'star8'… under « Formes · Étoiles et bannières »). The catalogue kinds whose
// name collides with a house stencil id are not offered twice: the house stencil
// wins, which is precisely what keeps existing diagrams unchanged.
//
// One function answers the question for the whole sub-module, so the renderer and
// the editor's knobs can never disagree: what is painted by the shared engine is
// exactly what gets adjustment handles.

import { hasShapeGeometry } from './shapes/paths'
import { hasNativeGeometry } from './shapes/native-geometry'
import { adjustHandles } from './shapes/adjust'

/**
 * Diagram stencil ids that are spelt like a catalogue kind but are drawn by the
 * diagrams' OWN code. Without this list they would be mistaken for catalogue
 * shapes by the fallback below (a house 'star' would sprout a phantom knob and
 * the ghost of a draw gesture would preview the shared geometry).
 *
 * These are exactly the basic geometries of `STENCILS` whose `case` sits in
 * `renderShape`'s switch — keep the two in sync.
 */
export const HOUSE_STENCIL_IDS = new Set([
  'rect',
  'ellipse',
  'triangle',
  'diamond',
  'hexagon',
  'cloud',
  'cross',
  'star',
  'cylinder',
  'trapezoid',
  'parallelogram',
  'callout',
])

/**
 * The shared kind painting this stencil id, or null when the id is drawn by the
 * diagrams' own code (basic house geometries, business templates and rasterised
 * hardware icons).
 */
export function sharedKindOf(type: string): string | null {
  // House geometries first: they take precedence over any same-named catalogue kind.
  if (HOUSE_STENCIL_IDS.has(type)) return null
  // Rasterised SVG icons: an image, never a geometry.
  if (type.startsWith('hw_')) return null
  // The bespoke ids are prefixed or snake_case ('flow_process', 'shp_cube',
  // 'ui_button', 'rounded_rect') and never collide with a catalogue kind, which
  // is camel case ('flowProcess', 'roundRect'). So an id the shared engine knows
  // IS a catalogue kind offered in the palette — the engine already paints it.
  return hasShapeGeometry(type) || hasNativeGeometry(type) ? type : null
}

/** True when this stencil id is painted by the shared engine. */
export function isSharedStencil(type: string): boolean {
  return sharedKindOf(type) !== null
}

/**
 * Adjustment knobs of a diagram shape, in world coordinates — empty for a
 * geometry that has none (a rectangle), for every bespoke template and for the
 * house basic geometries, which carry no adjustment values at all.
 */
export function stencilAdjustHandles(
  shape: { type: string; x: number; y: number; w: number; h: number; adj?: number[] },
) {
  const kind = sharedKindOf(shape.type)
  if (!kind) return []
  return adjustHandles(kind, { x: shape.x, y: shape.y, w: shape.w, h: shape.h }, shape.adj)
}
