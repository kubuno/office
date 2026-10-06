// Stacking arithmetic shared by the bar / line / area layers.

import type { ChartData, ResolvedSeries } from '../data'
import type { ChartStacking } from '../model'

export interface StackedPoint {
  /** Raw value (null = missing point). */
  v: number | null
  /** Stack base in value-axis units. */
  base: number
  /** base + this point's (possibly normalized) contribution. */
  top: number
  /** Signed share of the category's Σ|v| (for percent labels). */
  share: number
}

/** Number of points across all series and categories (≥ 1 for layout). */
export function pointCount(data: ChartData): number {
  let n = data.categories.length
  for (const s of data.series) {
    n = Math.max(n, s.values.length)
    if (s.x) n = Math.max(n, s.x.length)
  }
  return Math.max(n, 1)
}

/** Category labels padded to `n` with 1-based indices. */
export function categoryLabels(data: ChartData, n: number): string[] {
  const out: string[] = []
  for (let i = 0; i < n; i++) out.push(data.categories[i] ?? String(i + 1))
  return out
}

/**
 * Per-series stack positions, `[seriesIndex][pointIndex]`.
 * 'none'   → base 0, top = v.
 * 'stacked'→ bipolar cumulative: positives stack up, negatives stack down.
 * 'percent'→ same, on values normalized by the category's Σ|v| (0..±1).
 * Null values contribute nothing; base = top = the running total, so stacked
 * area bottoms stay continuous while the point itself renders as a gap.
 */
export function computeStacks(
  series: ResolvedSeries[], n: number, stacking: ChartStacking,
): StackedPoint[][] {
  const sumAbs = new Array<number>(n).fill(0)
  for (const s of series) for (let i = 0; i < n; i++) {
    const v = s.values[i]
    if (v != null) sumAbs[i] += Math.abs(v)
  }
  const posCum = new Array<number>(n).fill(0)
  const negCum = new Array<number>(n).fill(0)

  return series.map(s => {
    const row: StackedPoint[] = []
    for (let i = 0; i < n; i++) {
      const v = s.values[i] ?? null
      if (v == null) {
        row.push({ v: null, base: posCum[i], top: posCum[i], share: 0 })
        continue
      }
      const share = sumAbs[i] > 0 ? v / sumAbs[i] : 0
      const val = stacking === 'percent' ? share : v
      if (stacking === 'none') {
        row.push({ v, base: 0, top: val, share })
      } else if (val >= 0) {
        const base = posCum[i]
        posCum[i] = base + val
        row.push({ v, base, top: posCum[i], share })
      } else {
        const base = negCum[i]
        negCum[i] = base + val
        row.push({ v, base, top: negCum[i], share })
      }
    }
    return row
  })
}
