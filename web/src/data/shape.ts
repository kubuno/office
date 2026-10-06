// Data-shaping helpers: turn flat query rows into the series structures the
// visuals consume (categories + multiple series, pivoted by a legend column).
import { toNum } from './format'

export interface Series { name: string; values: number[]; color?: string }
export interface ShapedData { categories: string[]; series: Series[] }

/**
 * Shape rows for category charts.
 * - axis      : category column (X)
 * - series    : optional legend column → one series per distinct value
 * - metrics   : value columns; if a legend is set only metrics[0] is used
 */
export function shapeSeries(
  rows: Record<string, unknown>[],
  axis: string,
  metrics: string[],
  legend?: string,
): ShapedData {
  if (!rows.length) return { categories: [], series: [] }

  if (legend) {
    const valueCol = metrics[0]
    const cats: string[] = []
    const seenCat = new Set<string>()
    const seriesMap = new Map<string, Map<string, number>>()
    for (const r of rows) {
      const c = String(r[axis] ?? '')
      const s = String(r[legend] ?? '')
      if (!seenCat.has(c)) { seenCat.add(c); cats.push(c) }
      if (!seriesMap.has(s)) seriesMap.set(s, new Map())
      seriesMap.get(s)!.set(c, toNum(r[valueCol]))
    }
    const series: Series[] = [...seriesMap.entries()].map(([name, m]) => ({
      name, values: cats.map(c => m.get(c) ?? 0),
    }))
    return { categories: cats, series }
  }

  const categories = rows.map(r => String(r[axis] ?? ''))
  const series: Series[] = metrics.map(m => ({ name: m, values: rows.map(r => toNum(r[m])) }))
  return { categories, series }
}

/** Sum of a single metric across rows (for cards / totals). */
export function sumMetric(rows: Record<string, unknown>[], metric: string): number {
  return rows.reduce((a, r) => a + toNum(r[metric]), 0)
}

/** Bucket numeric values into a histogram (equal-width bins). */
export function histogram(values: number[], binCount = 12): { label: string; count: number; x0: number; x1: number }[] {
  if (!values.length) return []
  const min = Math.min(...values), max = Math.max(...values)
  if (min === max) return [{ label: String(min), count: values.length, x0: min, x1: max }]
  const width = (max - min) / binCount
  const bins = Array.from({ length: binCount }, (_, i) => ({ x0: min + i * width, x1: min + (i + 1) * width, count: 0, label: '' }))
  for (const v of values) {
    let idx = Math.floor((v - min) / width)
    if (idx >= binCount) idx = binCount - 1
    if (idx < 0) idx = 0
    bins[idx].count++
  }
  return bins.map(b => ({ ...b, label: `${Math.round(b.x0)}` }))
}

/** Quartiles for a box plot. */
export function quartiles(values: number[]): { min: number; q1: number; med: number; q3: number; max: number } {
  const s = [...values].sort((a, b) => a - b)
  const q = (p: number) => {
    if (!s.length) return 0
    const idx = (s.length - 1) * p
    const lo = Math.floor(idx), hi = Math.ceil(idx)
    return s[lo] + (s[hi] - s[lo]) * (idx - lo)
  }
  return { min: s[0] ?? 0, q1: q(0.25), med: q(0.5), q3: q(0.75), max: s[s.length - 1] ?? 0 }
}

/** Simple linear regression → slope/intercept for a trend line. */
export function linregress(xs: number[], ys: number[]): { slope: number; intercept: number } {
  const n = xs.length
  if (!n) return { slope: 0, intercept: 0 }
  const sx = xs.reduce((a, b) => a + b, 0), sy = ys.reduce((a, b) => a + b, 0)
  const sxx = xs.reduce((a, b) => a + b * b, 0), sxy = xs.reduce((a, b, i) => a + b * ys[i], 0)
  const denom = n * sxx - sx * sx
  if (denom === 0) return { slope: 0, intercept: sy / n }
  const slope = (n * sxy - sx * sy) / denom
  return { slope, intercept: (sy - slope * sx) / n }
}

/** Moving average for line smoothing / forecast hints. */
export function movingAverage(values: number[], window = 3): number[] {
  if (window <= 1) return values
  return values.map((_, i) => {
    const start = Math.max(0, i - window + 1)
    const slice = values.slice(start, i + 1)
    return slice.reduce((a, b) => a + b, 0) / slice.length
  })
}

/**
 * Squarified treemap layout in a W×H rectangle. Returns one rect per item,
 * gap-free, with aspect ratios kept close to 1 (recursive row packing).
 */
export function treemapLayout(items: { value: number }[], W: number, H: number): { x: number; y: number; w: number; h: number }[] {
  const out: { x: number; y: number; w: number; h: number }[] = items.map(() => ({ x: 0, y: 0, w: 0, h: 0 }))
  const total = items.reduce((a, b) => a + Math.max(0, b.value), 0) || 1
  // Indices sorted by descending value, laid out by area.
  const idx = items.map((_, i) => i).sort((a, b) => items[b].value - items[a].value)
  const areas = idx.map(i => (Math.max(0, items[i].value) / total) * W * H)

  const worst = (row: number[], len: number): number => {
    const s = row.reduce((a, b) => a + b, 0)
    const max = Math.max(...row), min = Math.min(...row)
    const len2 = len * len, s2 = s * s
    return Math.max((len2 * max) / s2, s2 / (len2 * min))
  }

  let x = 0, y = 0, availW = W, availH = H
  let i = 0
  while (i < areas.length) {
    const horizontal = availW >= availH
    const len = horizontal ? availH : availW
    const row: number[] = [areas[i]]
    let j = i + 1
    while (j < areas.length) {
      const next = [...row, areas[j]]
      if (worst(next, len) > worst(row, len)) break
      row.push(areas[j]); j++
    }
    const rowSum = row.reduce((a, b) => a + b, 0)
    const thickness = rowSum / len
    let along = 0
    for (let k = 0; k < row.length; k++) {
      const cell = row[k] / thickness
      if (horizontal) out[idx[i + k]] = { x, y: y + along, w: thickness, h: cell }
      else out[idx[i + k]] = { x: x + along, y, w: cell, h: thickness }
      along += cell
    }
    if (horizontal) { x += thickness; availW -= thickness } else { y += thickness; availH -= thickness }
    i = j
  }
  return out
}
