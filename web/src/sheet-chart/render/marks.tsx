// Point markers (chart2 standard symbol cycle) and line/area path builders.

import type { ReactNode } from 'react'
import { SYMBOL_CYCLE } from '../model'

export interface Pt { x: number; y: number }

const f = (n: number) => Math.round(n * 100) / 100

/** chart2 symbol shape for series `i` (nStandardSymbol % 8). */
export function symbolFor(seriesIndex: number): string {
  return SYMBOL_CYCLE[seriesIndex % SYMBOL_CYCLE.length]
}

/** Closed SVG path for a chart2 standard symbol centered on (cx, cy). */
export function symbolPath(shape: string, cx: number, cy: number, r: number): string {
  const x = f(cx), y = f(cy), R = f(r)
  switch (shape) {
    case 'diamond':
      return `M ${x} ${f(y - R)} L ${f(x + R)} ${y} L ${x} ${f(y + R)} L ${f(x - R)} ${y} Z`
    case 'arrow-down':
      return `M ${f(x - R)} ${f(y - R)} L ${f(x + R)} ${f(y - R)} L ${x} ${f(y + R)} Z`
    case 'arrow-up':
      return `M ${f(x - R)} ${f(y + R)} L ${f(x + R)} ${f(y + R)} L ${x} ${f(y - R)} Z`
    case 'arrow-right':
      return `M ${f(x - R)} ${f(y - R)} L ${f(x - R)} ${f(y + R)} L ${f(x + R)} ${y} Z`
    case 'arrow-left':
      return `M ${f(x + R)} ${f(y - R)} L ${f(x + R)} ${f(y + R)} L ${f(x - R)} ${y} Z`
    case 'bowtie':
      return `M ${f(x - R)} ${f(y - R)} L ${f(x - R)} ${f(y + R)} L ${x} ${y} Z ` +
             `M ${f(x + R)} ${f(y - R)} L ${f(x + R)} ${f(y + R)} L ${x} ${y} Z`
    case 'hourglass':
      return `M ${f(x - R)} ${f(y - R)} L ${f(x + R)} ${f(y - R)} L ${x} ${y} Z ` +
             `M ${f(x - R)} ${f(y + R)} L ${f(x + R)} ${f(y + R)} L ${x} ${y} Z`
    case 'square':
    default:
      return `M ${f(x - R)} ${f(y - R)} L ${f(x + R)} ${f(y - R)} L ${f(x + R)} ${f(y + R)} L ${f(x - R)} ${f(y + R)} Z`
  }
}

/**
 * Hover text for a chart mark, as the `title` ATTRIBUTE plus an accessible name.
 *
 * The shell mounts a global interceptor that turns `title` attributes into the
 * project's tooltip (dark bubble, pointer-anchored). An SVG `<title>` CHILD would
 * bypass it and show the browser's own grey bubble instead, so every mark spreads
 * this helper. `title` is a valid attribute on SVG elements in the DOM but React's
 * SVGProps does not declare it; spreading this narrow type is what gets it past
 * the checker (returning SVGProps would drag an incompatible `ref` along).
 */
export interface MarkHover { title?: string; 'aria-label'?: string }
export function hoverTip(text?: string): MarkHover {
  return text ? { title: text, 'aria-label': text } : {}
}

/** One data-point marker with a 1px surface ring for overlap separation. */
export function Marker(props: {
  shape: string; cx: number; cy: number; r: number; fill: string; title?: string
}): ReactNode {
  return (
    <path
      d={symbolPath(props.shape, props.cx, props.cy, props.r)}
      fill={props.fill}
      stroke="var(--color-surface-0, #ffffff)"
      strokeWidth={1}
      {...hoverTip(props.title)}
    />
  )
}

/** Contiguous non-null runs of a point sequence. */
function splitRuns(pts: (Pt | null)[]): Pt[][] {
  const runs: Pt[][] = []
  let cur: Pt[] = []
  for (const p of pts) {
    if (p) cur.push(p)
    else if (cur.length > 0) { runs.push(cur); cur = [] }
  }
  if (cur.length > 0) runs.push(cur)
  return runs
}

/**
 * Path commands from run[0] to the end (without the leading "M"): straight
 * segments, or Catmull-Rom smoothing converted to cubic Béziers.
 */
function runCmds(run: Pt[], smooth: boolean): string {
  if (run.length < 2) return ''
  if (!smooth) return run.slice(1).map(p => `L ${f(p.x)} ${f(p.y)}`).join(' ')
  let d = ''
  for (let i = 0; i < run.length - 1; i++) {
    const p0 = run[Math.max(i - 1, 0)]
    const p1 = run[i]
    const p2 = run[i + 1]
    const p3 = run[Math.min(i + 2, run.length - 1)]
    const c1x = p1.x + (p2.x - p0.x) / 6, c1y = p1.y + (p2.y - p0.y) / 6
    const c2x = p2.x - (p3.x - p1.x) / 6, c2y = p2.y - (p3.y - p1.y) / 6
    d += ` C ${f(c1x)} ${f(c1y)} ${f(c2x)} ${f(c2y)} ${f(p2.x)} ${f(p2.y)}`
  }
  return d.trim()
}

/** Open path for a series polyline; null points break it into separate runs. */
export function linePath(pts: (Pt | null)[], smooth: boolean): string {
  return splitRuns(pts)
    .filter(run => run.length >= 2)
    .map(run => `M ${f(run[0].x)} ${f(run[0].y)} ${runCmds(run, smooth)}`)
    .join(' ')
}

/**
 * Closed path between a top and a bottom polyline (index-aligned). Runs are
 * split where either side is null; each run is closed top → reversed bottom.
 */
export function areaPath(top: (Pt | null)[], bottom: (Pt | null)[], smooth: boolean): string {
  const n = Math.min(top.length, bottom.length)
  const parts: string[] = []
  let start = -1
  const flush = (end: number) => {
    if (start < 0 || end - start < 2) { start = -1; return }
    const t: Pt[] = [], b: Pt[] = []
    for (let i = start; i < end; i++) { t.push(top[i] as Pt); b.push(bottom[i] as Pt) }
    b.reverse()
    parts.push(
      `M ${f(t[0].x)} ${f(t[0].y)} ${runCmds(t, smooth)} ` +
      `L ${f(b[0].x)} ${f(b[0].y)} ${runCmds(b, smooth)} Z`,
    )
    start = -1
  }
  for (let i = 0; i < n; i++) {
    if (top[i] && bottom[i]) { if (start < 0) start = i }
    else flush(i)
  }
  flush(n)
  return parts.join(' ')
}
