// Value-axis auto-scaling, ported from LibreOffice chart2
// (chart2/source/view/axes/ScaleAutomatism.cxx, linear branch).

import { ChartKind, ChartSpec, ChartStacking } from './model'
import { ChartData } from './data'

export interface AxisScale {
  min: number
  max: number
  step: number        // major tick interval ("nice" 1/2/5 × 10^n)
  minorCount: number  // minor intervals per major interval
}

export interface AutoScaleOptions {
  /** Pull the minimum down to 0 (bar/area value axis). */
  expandToZero: boolean
  /** Narrow value bands (min/max > 5/6): widen towards zero. */
  expandTowardZero: boolean
  /** Add one step of margin when data sits within 1/21 of a border (not bars). */
  expandCloseToBorder: boolean
}

/** chart2 per-plotter flags for the value axis (VSeriesPlotter + overrides). */
export function scaleOptionsFor(kind: ChartKind): AutoScaleOptions {
  const bar = kind === 'column' || kind === 'bar' || kind === 'combo'
  return {
    expandToZero: bar || kind === 'area',
    expandTowardZero: true,
    // BarChart/NetChart override this to false (bars start at 0, no margin).
    expandCloseToBorder: !bar && kind !== 'radar',
  }
}

/**
 * Compute a "nice" linear scale covering [dataMin, dataMax] with at most
 * `nMaxTicks` major intervals. Faithful port of ScaleAutomatism's
 * calculateExplicitIncrementAndScaleForLinear.
 */
export function autoScaleLinear(
  dataMin: number, dataMax: number, nMaxTicks = 10, options?: Partial<AutoScaleOptions>,
): AxisScale {
  const opts: AutoScaleOptions = {
    expandToZero: false, expandTowardZero: false, expandCloseToBorder: false, ...options,
  }
  if (!Number.isFinite(dataMin) || !Number.isFinite(dataMax)) { dataMin = 0; dataMax = 1 }
  let min = dataMin, max = dataMax
  if (min > max) max = min
  // Pure-negative range: negate + swap, work in positives, swap back at the end.
  const swap = min < 0 && max <= 0
  if (swap) { const t = min; min = -max; max = -t }
  // chart2 keeps the raw data bounds (fSourceMinimum/Maximum) for the
  // close-to-border checks of STEP 4 — captured before the STEP 2 expansion.
  const srcMin = min, srcMax = max

  // STEP 2: provisional minimum.
  if (min > 0) {
    if (min === max || min / max < 5 / 6) { if (opts.expandToZero) min = 0 }
    else if (opts.expandTowardZero) min -= (max - min) / 2
  }
  if (min === max) { if (max === 0) max = 1; else if (max > 0) max *= 2; else min *= 2 }

  // STEP 3: nice 1/2/5 × 10^n step, iterating up while too many intervals.
  let axMin = 0, axMax = 0, distance = 0
  let norm = 0, mag = 0
  let first = true
  for (let guard = 0; guard < 80; guard++) {
    if (first) {
      const raw = (max - min) / Math.max(nMaxTicks, 1)
      const exp = Math.floor(Math.log10(Math.abs(raw) > 0 ? Math.abs(raw) : 1))
      mag = Math.pow(10, exp)
      norm = raw / mag
      if (norm <= 1) norm = 1
      else if (norm <= 2) norm = 2
      else if (norm <= 5) norm = 5
      else { mag *= 10; norm = 1 }
      first = false
    } else {
      // Too many intervals → next step in the 1, 2, 5, 10, 20, … sequence.
      if (norm === 1) norm = 2
      else if (norm === 2) norm = 5
      else { mag *= 10; norm = 1 }
    }
    distance = norm * mag

    // STEP 4: snap bounds onto the tick grid (base 0).
    axMin = Math.floor(min / distance + 1e-9) * distance
    axMax = Math.ceil(max / distance - 1e-9) * distance
    // One step of margin when a datum sits within 1/21 of the opposite border.
    if (opts.expandCloseToBorder) {
      if (axMin !== 0 && (axMax - srcMin) / (axMax - axMin) > 20 / 21) axMin -= distance
      if (axMax !== 0 && (srcMax - axMin) / (axMax - axMin) > 20 / 21) axMax += distance
    }
    if (Math.floor((axMax - axMin) / distance + 1e-7) <= nMaxTicks) break
  }

  if (swap) { const t = axMin; axMin = -axMax; axMax = -t }
  return { min: axMin, max: axMax, step: distance, minorCount: 2 }
}

// ── Data extent ──────────────────────────────────────────────────────────────

/**
 * Y extent of the resolved data. Stacked: per-category sums of positive and
 * of negative values. Percent: fixed [0, 1] (AxisType::PERCENT).
 */
export function computeExtent(data: ChartData, stacking: ChartStacking): { min: number; max: number } {
  if (stacking === 'percent') return { min: 0, max: 1 }
  let min = Infinity, max = -Infinity
  if (stacking === 'stacked') {
    const len = Math.max(0, ...data.series.map(s => s.values.length))
    for (let i = 0; i < len; i++) {
      let pos = 0, neg = 0
      for (const s of data.series) {
        const v = s.values[i]
        if (v == null) continue
        if (v >= 0) pos += v; else neg += v
      }
      if (pos > max) max = pos
      if (neg < min) min = neg
      if (pos < min) min = pos   // all-negative categories: top of the stack is 0
      if (neg > -Infinity && neg < 0 && 0 > max) max = 0
    }
  } else {
    for (const s of data.series) for (const v of s.values) {
      if (v == null) continue
      if (v < min) min = v
      if (v > max) max = v
    }
  }
  if (min === Infinity) { min = 0; max = 1 }
  return { min, max }
}

/** X extent for scatter charts (falls back to 1..n point indices). */
export function computeXExtent(data: ChartData): { min: number; max: number } {
  let min = Infinity, max = -Infinity
  for (const s of data.series) {
    const xs = s.x
    if (xs) {
      for (const v of xs) { if (v == null) continue; if (v < min) min = v; if (v > max) max = v }
    } else if (s.values.length > 0) {
      min = Math.min(min, 1); max = Math.max(max, s.values.length)
    }
  }
  if (min === Infinity) { min = 0; max = 1 }
  return { min, max }
}

/**
 * Full value-axis scale for a spec: percent → fixed 0..100%, manual bounds
 * honoured, otherwise chart2 auto-scale with the kind's expansion flags.
 */
export function computeValueScale(spec: ChartSpec, data: ChartData, nMaxTicks = 10): AxisScale {
  if (spec.stacking === 'percent') {
    const s = autoScaleLinear(0, 1, nMaxTicks, { expandToZero: true })
    return { ...s, min: 0, max: 1 }
  }
  const ext = computeExtent(data, spec.stacking)
  const auto = autoScaleLinear(ext.min, ext.max, nMaxTicks, scaleOptionsFor(spec.kind))
  const min = spec.axisY.min ?? auto.min
  const max = spec.axisY.max ?? auto.max
  if (min === auto.min && max === auto.max) return auto
  // Manual bounds: keep a nice step for the requested span.
  const span = autoScaleLinear(min, max, nMaxTicks)
  return { min, max, step: span.step, minorCount: auto.minorCount }
}

// ── Ticks & formatting ───────────────────────────────────────────────────────

/** Major tick positions, min..max inclusive (guards float drift). */
export function ticksOf(scale: AxisScale): number[] {
  const out: number[] = []
  if (scale.step <= 0 || !Number.isFinite(scale.step)) return out
  const n = Math.round((scale.max - scale.min) / scale.step)
  for (let i = 0; i <= n && i <= 1000; i++) out.push(scale.min + i * scale.step)
  return out
}

/** Default tick label: float-noise-free, locale decimal separator. */
export function formatTick(v: number, step: number): string {
  const clean = Number(v.toPrecision(12))
  const dec = step < 1 ? Math.max(0, Math.min(6, -Math.floor(Math.log10(step)))) : 0
  return clean.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: Math.max(dec, 0) })
}
