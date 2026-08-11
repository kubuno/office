// Adjustment handles — the yellow knobs Word and LibreOffice put on a shape to
// reshape it without resizing it: the head of an arrow, the corner radius of a
// rounded rectangle, the waist of a star, the tail of a callout.
//
// They mirror OOXML's `<a:avLst><a:gd name="adj1" fmla="val 25000"/>`: each value
// is a FRACTION of the shape's box (stored 0..1 here, written as 1/100000ths at
// export), so a shape keeps its look when resized. A shape without adjustments
// simply declares none — exactly like `prstGeom` presets that carry no `avLst`.

import type { ShapeKind } from './catalog'
import {
  presetOf, presetAdjValues, presetHandlePositions, presetAdjustFromDrag,
} from './preset-engine'
import { getShapeProvider } from './registry'

export interface AdjustSpec {
  /** OOXML name, e.g. "adj1" — what the value is written as on export. */
  name: string
  /** Default fraction when the shape carries no explicit value. */
  def: number
  min: number
  max: number
  /** Which box axis the knob slides along. */
  axis: 'x' | 'y'
  /**
   * Knob position inside the box, as fractions of width/height. It receives the
   * BOX because several geometries size a feature on the smaller side (an arrow
   * head is a fraction of min(w,h), not of the width): computing the knob without
   * the box put it beside the corner it is supposed to grab.
   */
  at: (v: number, box: { w: number; h: number }) => { fx: number; fy: number }
  /** Fraction implied by a knob dropped at (fx, fy). */
  from: (fx: number, fy: number, box: { w: number; h: number }) => number
}

const clamp01 = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/**
 * Adjustments per geometry. Only the shapes that HAVE one appear: the others
 * (rectangle, ellipse, triangle, diamond, line) are fully described by their box,
 * in our renderer as in Office.
 */
/**
 * Keyed by kind name rather than by `ShapeKind`: an editor may carry legacy names
 * of its own (the spreadsheet's 'callout' and 'plus' predate the catalogue), and
 * an unknown key simply falls through to the preset engine below.
 */
export const SHAPE_ADJUSTMENTS: Record<string, AdjustSpec[]> = {
  // Corner radius, as a fraction of the smaller side.
  roundRect: [{
    // Corner radius = min(w,h) * v in the geometry: the knob follows the arc's
    // start on the top edge, not an arbitrary fraction of the width.
    name: 'adj', def: 0.16, min: 0, max: 0.5, axis: 'x',
    at: (v, b) => ({ fx: b.w > 0 ? (Math.min(b.w, b.h) * v) / b.w : 0, fy: 0 }),
    from: (fx, _fy, b) => {
      const m = Math.min(b.w, b.h)
      return m > 0 ? clamp01((fx * b.w) / m, 0, 0.5) : 0.16
    },
  }],
  // Two knobs, like Word's block arrow: shaft thickness, then head length.
  arrow: [
    {
      name: 'adj1', def: 0.5, min: 0.05, max: 0.95, axis: 'y',
      at: v => ({ fx: 0, fy: 0.5 - v / 2 }),
      from: (_fx, fy) => clamp01((0.5 - fy) * 2, 0.05, 0.95),
    },
    {
      // Head length. The geometry computes it as min(w, min(w,h) * v), so the knob
      // is placed with the SAME formula — it then sits exactly on the corner where
      // the head meets the shaft, and follows it while dragging.
      name: 'adj2', def: 0.5, min: 0.05, max: 1, axis: 'x',
      at: (v, b) => ({ fx: b.w > 0 ? (b.w - Math.min(b.w, Math.min(b.w, b.h) * v)) / b.w : 0, fy: 0 }),
      from: (fx, _fy, b) => {
        const m = Math.min(b.w, b.h)
        return m > 0 ? clamp01(((1 - fx) * b.w) / m, 0.05, 1) : 0.5
      },
    },
  ],
  // Waist of the star: the inner radius as a fraction of the outer one. The knob
  // rides the first INNER vertex (top-right of the first notch), like Office.
  star: [{
    name: 'adj', def: 0.38, min: 0.05, max: 0.95, axis: 'x',
    at: v => {
      const a = Math.PI / 5 - Math.PI / 2   // first inner vertex of a 5-point star
      return { fx: 0.5 + (v / 2) * Math.cos(a), fy: 0.5 + (v / 2) * Math.sin(a) }
    },
    from: (fx, fy) => {
      const dx = (fx - 0.5) * 2, dy = (fy - 0.5) * 2
      return clamp01(Math.hypot(dx, dy), 0.05, 0.95)
    },
  }],
  // Callout: where the body stops, and where the tail points.
  callout: [
    {
      name: 'adj1', def: 0.72, min: 0.3, max: 0.95, axis: 'y',
      at: v => ({ fx: 0, fy: v }),
      from: (_fx, fy) => clamp01(fy, 0.3, 0.95),
    },
    {
      name: 'adj2', def: 0.28, min: 0, max: 1, axis: 'x',
      at: v => ({ fx: v, fy: 1 }),
      from: fx => clamp01(fx, 0, 1),
    },
  ],
  // Thickness of the cross's arms.
  plus: [{
    // Arm thickness = min(w,h) * v: the knob sits on the top-left inner corner.
    name: 'adj', def: 0.35, min: 0.05, max: 0.95, axis: 'x',
    at: (v, b) => ({ fx: b.w > 0 ? 0.5 - (Math.min(b.w, b.h) * v) / (2 * b.w) : 0.5, fy: 0 }),
    from: (fx, _fy, b) => {
      const m = Math.min(b.w, b.h)
      return m > 0 ? clamp01(((0.5 - fx) * 2 * b.w) / m, 0.05, 0.95) : 0.35
    },
  }],
}

/**
 * Adjustment specs of a kind, wherever they come from: a module that REGISTERED the
 * geometry (shapes/registry) first, then the suite's own native table. Null means
 * the kind is preset-backed and its handles come from LibreOffice's data instead.
 *
 * A registered kind wins, deliberately: a module that contributes a geometry also
 * owns how it is adjusted.
 */
function adjSpecsOf(kind: string): AdjustSpec[] | undefined {
  return getShapeProvider(kind)?.adjustments ?? SHAPE_ADJUSTMENTS[kind]
}

/**
 * Adjustment values in use for a shape: its own, or the defaults.
 * Native and REGISTERED kinds use FRACTIONS (0..1); preset-backed kinds use the raw
 * OOXML units of LibreOffice's data (val/100000-style), like the files themselves.
 */
export function adjValues(kind: string, adj?: number[]): number[] {
  const specs = adjSpecsOf(kind)
  if (specs) {
    return specs.map((s, i) => {
      const v = adj?.[i]
      return typeof v === 'number' && Number.isFinite(v) ? clamp01(v, s.min, s.max) : s.def
    })
  }
  const preset = presetOf(kind as ShapeKind)
  return preset ? presetAdjValues(preset, adj) : []
}

export interface AdjustHandle {
  index: number
  /** Content-space position of the knob. */
  x: number
  y: number
}

/** Knob positions for a shape occupying `rect` (content space). */
export function adjustHandles(
  kind: string, rect: { x: number; y: number; w: number; h: number }, adj?: number[],
): AdjustHandle[] {
  const specs = adjSpecsOf(kind)
  if (specs) {
    const values = adjValues(kind, adj)
    return specs.map((s, i) => {
      const { fx, fy } = s.at(values[i], { w: rect.w, h: rect.h })
      return { index: i, x: rect.x + fx * rect.w, y: rect.y + fy * rect.h }
    })
  }
  // Preset-backed kind: the handles come from LibreOffice's data, evaluated for
  // this box — they sit exactly where Office puts them.
  const preset = presetOf(kind as ShapeKind)
  if (!preset) return []
  return presetHandlePositions(preset, { w: rect.w, h: rect.h, adj: presetAdjValues(preset, adj) })
    .map(h => ({ index: h.index, x: rect.x + h.x, y: rect.y + h.y }))
}

/** New adjustment array after dragging knob `index` to a content-space point. */
export function adjustFromDrag(
  kind: string, rect: { x: number; y: number; w: number; h: number },
  index: number, px: number, py: number, adj?: number[],
): number[] {
  const specs = adjSpecsOf(kind)
  const values = adjValues(kind, adj)
  if (rect.w <= 0 || rect.h <= 0) return values
  if (specs) {
    const spec = specs[index]
    if (!spec) return values
    const fx = (px - rect.x) / rect.w
    const fy = (py - rect.y) / rect.h
    values[index] = spec.from(fx, fy, { w: rect.w, h: rect.h })
    return values
  }
  const preset = presetOf(kind as ShapeKind)
  if (!preset) return values
  return presetAdjustFromDrag(preset, { w: rect.w, h: rect.h, adj: values }, index, px - rect.x, py - rect.y)
}

/** Hit test: index of the knob under a point, or -1. */
export function hitAdjust(
  kind: string, rect: { x: number; y: number; w: number; h: number },
  px: number, py: number, adj?: number[], slop = 7,
): number {
  for (const h of adjustHandles(kind, rect, adj)) {
    if (Math.abs(px - h.x) <= slop && Math.abs(py - h.y) <= slop) return h.index
  }
  return -1
}

