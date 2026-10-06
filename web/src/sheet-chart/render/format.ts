// Number/text formatting helpers for axis ticks, data labels and legends.

import { formatTick } from '../scale'
import type { TextMeasurer } from '../layout'

export interface TickFormatOptions {
  /** Percent-stacked axis: values are fractions 0..1 rendered as 0%..100%. */
  percent?: boolean
  /** Model number-format pattern (tiny subset: '%' flag + decimal count). */
  pattern?: string
}

/** Decimal places requested by a "0.00"-style pattern. */
function patternDecimals(pattern: string): number {
  const dot = pattern.indexOf('.')
  if (dot < 0) return 0
  let n = 0
  for (let i = dot + 1; i < pattern.length && (pattern[i] === '0' || pattern[i] === '#'); i++) n++
  return n
}

/** Locale-aware compact notation ("12 k", "1.2 M") for large magnitudes. */
const compactFmt = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })

/**
 * Axis tick label: percent axis → "40%", model pattern subset honoured,
 * steps ≥ 1000 abbreviated (locale compact notation), else plain locale number.
 */
export function formatAxisTick(v: number, step: number, opts?: TickFormatOptions): string {
  if (opts?.percent) return `${Math.round(v * 100)}%`
  const p = opts?.pattern
  if (p) {
    const dec = patternDecimals(p)
    if (p.includes('%')) return (v * 100).toLocaleString(undefined, { maximumFractionDigits: dec }) + '%'
    return v.toLocaleString(undefined, { minimumFractionDigits: dec, maximumFractionDigits: dec })
  }
  if (Math.abs(step) >= 1000) return compactFmt.format(v)
  return formatTick(v, step)
}

/** Data-label / tooltip value: float-noise-free locale string, ≤ 2 decimals. */
export function formatDataValue(v: number): string {
  const clean = Number(v.toPrecision(10))
  return clean.toLocaleString(undefined, { maximumFractionDigits: 2 })
}

/** Percentage share label: 1 decimal below 10%, integer above ("7.5%", "23%"). */
export function formatShare(frac: number): string {
  const pct = frac * 100
  const digits = Math.abs(pct) < 9.95 ? 1 : 0
  return `${pct.toLocaleString(undefined, { maximumFractionDigits: digits })}%`
}

/** Truncate `text` with an ellipsis so it measures ≤ `maxW` at `fontSize`. */
export function truncateToWidth(text: string, maxW: number, fontSize: number, measure: TextMeasurer): string {
  if (measure(text, fontSize).w <= maxW) return text
  let lo = 0
  let hi = text.length
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2)
    if (measure(text.slice(0, mid) + '…', fontSize).w <= maxW) lo = mid
    else hi = mid - 1
  }
  return lo <= 0 ? '…' : text.slice(0, lo) + '…'
}

/** Readable ink color over a series-colored fill (center data labels). */
export function inkFor(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex)
  if (!m) return 'var(--color-text-primary, #202124)'
  const n = parseInt(m[1], 16)
  const lum = 0.2126 * ((n >> 16) & 255) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255)
  return lum < 140 ? '#ffffff' : '#202124'
}
