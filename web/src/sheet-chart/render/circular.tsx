// Pie / donut renderer (chart2 PieChart flavour): slices start at 12 o'clock
import { hoverTip } from './marks'
// and run clockwise, like Calc. Negative and missing values are excluded
// (clamped to 0, the Calc/Excel pie behaviour). A pie plots the FIRST series
// only; a donut (holeSize > 0) plots every series as concentric rings, first
// series innermost (chart2 ring order). Legend semantics for pies (categories
// with slice colors, not series) are provided to ChartView via `pieLegendData`.

import type { ReactNode } from 'react'
import { CHART2_PALETTE, CHART_NEUTRAL, FONT_DATA_LABEL } from '../model'
import type { ChartSpec } from '../model'
import type { ChartData } from '../data'
import { approxMeasure } from '../layout'
import type { ChartLayout, TextMeasurer } from '../layout'
import { pointCount, categoryLabels } from './stacks'
import { formatDataValue, formatShare, inkFor } from './format'

const TWO_PI = Math.PI * 2
const f = (v: number) => Math.round(v * 100) / 100
/** Point at radius `r`, angle `a` measured CLOCKWISE from 12 o'clock. */
const ptX = (cx: number, r: number, a: number) => cx + r * Math.sin(a)
const ptY = (cy: number, r: number, a: number) => cy - r * Math.cos(a)

/** Below this share a data label always moves outside the slice. */
const OUTSIDE_MIN_FRAC = 0.05

/** Color of slice `i`: per-point palette index, explicit pointColors override. */
export function sliceColor(pointColors: string[] | undefined, i: number): string {
  return pointColors?.[i] ?? CHART2_PALETTE[i % CHART2_PALETTE.length]
}

/**
 * Pie/donut legends list CATEGORIES with slice colors (chart2 per-point legend
 * expansion), not series. ChartView feeds this pseudo-data to the shared
 * layout/legend pipeline instead of touching layout.ts.
 */
export function pieLegendData(data: ChartData): ChartData {
  const n = pointCount(data)
  const cats = categoryLabels(data, n)
  return {
    series: cats.map((name, i) => ({
      name,
      color: sliceColor(data.pointColors, i),
      kind: 'pie' as const,
      axis: 'primary' as const,
      values: [data.series[0]?.values[i] ?? null],
    })),
    categories: [],
  }
}

interface Slice { i: number; v: number; frac: number; a0: number; a1: number }

/** Angular slices of one ring; null/negative/zero values yield no slice. */
function computeSlices(values: (number | null)[], n: number): Slice[] {
  const clamped: number[] = []
  let total = 0
  for (let i = 0; i < n; i++) {
    const v = values[i]
    const c = v != null && Number.isFinite(v) ? Math.max(v, 0) : 0
    clamped.push(c)
    total += c
  }
  if (total <= 0) return []
  const out: Slice[] = []
  let a = 0
  for (let i = 0; i < n; i++) {
    if (clamped[i] <= 0) continue
    const frac = clamped[i] / total
    out.push({ i, v: clamped[i], frac, a0: a, a1: a + frac * TWO_PI })
    a += frac * TWO_PI
  }
  return out
}

/**
 * Annular sector path (r0 = 0 → plain sector). Full turns render as rings
 * built from two half arcs; combine with fill-rule "evenodd" for the hole.
 */
function sectorPath(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  if (a1 - a0 >= TWO_PI - 1e-4) {
    const ring = (r: number, sweep: 0 | 1) =>
      `M ${f(cx)} ${f(cy - r)} A ${f(r)} ${f(r)} 0 1 ${sweep} ${f(cx)} ${f(cy + r)} ` +
      `A ${f(r)} ${f(r)} 0 1 ${sweep} ${f(cx)} ${f(cy - r)} Z`
    return r0 > 0.5 ? `${ring(r1, 1)} ${ring(r0, 0)}` : ring(r1, 1)
  }
  const large = a1 - a0 > Math.PI ? 1 : 0
  const o0 = `${f(ptX(cx, r1, a0))} ${f(ptY(cy, r1, a0))}`
  const o1 = `${f(ptX(cx, r1, a1))} ${f(ptY(cy, r1, a1))}`
  if (r0 <= 0.5) return `M ${f(cx)} ${f(cy)} L ${o0} A ${f(r1)} ${f(r1)} 0 ${large} 1 ${o1} Z`
  const i0 = `${f(ptX(cx, r0, a0))} ${f(ptY(cy, r0, a0))}`
  const i1 = `${f(ptX(cx, r0, a1))} ${f(ptY(cy, r0, a1))}`
  return `M ${o0} A ${f(r1)} ${f(r1)} 0 ${large} 1 ${o1} L ${i1} A ${f(r0)} ${f(r0)} 0 ${large} 0 ${i0} Z`
}

export interface CircularPlotProps {
  spec: ChartSpec
  data: ChartData
  layout: ChartLayout
  measure?: TextMeasurer
}

export function CircularPlot({ spec, data, layout, measure }: CircularPlotProps): ReactNode {
  const m = measure ?? approxMeasure
  const plot = layout.plot
  if (plot.w < 32 || plot.h < 32) return null
  const n = pointCount(data)
  const cats = categoryLabels(data, n)
  const rings = spec.holeSize > 0 ? data.series : data.series.slice(0, 1)
  const K = Math.max(rings.length, 1)
  const slicesPerRing = rings.map(s => computeSlices(s.values, n))
  if (!slicesPerRing.some(sl => sl.length > 0)) return null

  const fs = FONT_DATA_LABEL
  const cx = plot.x + plot.w / 2
  const cy = plot.y + plot.h / 2
  let maxR = Math.max(Math.min(plot.w, plot.h) / 2 - 2, 4)

  const labelText = (sl: Slice) =>
    spec.dataLabels === 'percent' ? formatShare(sl.frac) : formatDataValue(sl.v)

  // Label placement (single ring only): inside when the text fits the slice
  // near the label radius, outside with a leader line otherwise. Outside
  // labels reserve a horizontal band, shrinking the pie to keep them inside.
  const single = K === 1
  const wantLabels = !!spec.dataLabels && single
  const outSet = new Set<number>()
  if (wantLabels) {
    const hole = spec.holeSize
    const rL = maxR * (hole + (1 - hole) * 0.66)
    let maxOutW = 0
    for (const sl of slicesPerRing[0]) {
      const room = 2 * rL * Math.sin(Math.min((sl.a1 - sl.a0) / 2, Math.PI / 2))
      if (sl.frac < OUTSIDE_MIN_FRAC || m(labelText(sl), fs).w > room) {
        outSet.add(sl.i)
        maxOutW = Math.max(maxOutW, m(labelText(sl), fs).w)
      }
    }
    if (outSet.size > 0) {
      const reserve = Math.min(maxOutW + 16, plot.w * 0.3)
      maxR = Math.max(Math.min(plot.w / 2 - reserve, plot.h / 2 - fs * 1.3 - 4) - 2, 4)
    }
  }

  // Exploded slices shrink the radius so offset slices stay inside the plot.
  const explode = single ? spec.explode : 0
  const R = explode > 0 ? maxR / (1 + explode) : maxR
  const offR = explode * R
  const hole = spec.holeSize * R

  const els: ReactNode[] = []
  const labelEls: ReactNode[] = []

  slicesPerRing.forEach((slices, k) => {
    const s = rings[k]
    const r0 = hole + (R - hole) * (k / K)
    const r1 = hole + (R - hole) * ((k + 1) / K)
    for (const sl of slices) {
      const mid = (sl.a0 + sl.a1) / 2
      const dx = explode > 0 ? offR * Math.sin(mid) : 0
      const dy = explode > 0 ? -offR * Math.cos(mid) : 0
      const fill = sliceColor(data.pointColors, sl.i)
      const tipTxt = (K > 1 ? `${s.name}\n` : '') +
        `${cats[sl.i]}: ${formatDataValue(sl.v)} (${formatShare(sl.frac)})`
      els.push(
        <path
          key={`s${k}-${sl.i}`} d={sectorPath(cx + dx, cy + dy, r0, r1, sl.a0, sl.a1)}
          fill={fill} fillRule="evenodd"
          stroke="var(--color-surface-0, #ffffff)" strokeWidth={1} strokeLinejoin="round"
          {...hoverTip(tipTxt)}
        />,
      )
      // Multi-ring donut: centered in-segment labels when the ring is thick
      // enough; thin segments stay unlabelled (tooltips still carry the value).
      if (spec.dataLabels && !single && r1 - r0 >= fs + 4 && sl.frac >= 0.04) {
        const rL = (r0 + r1) / 2
        labelEls.push(
          <text
            key={`d${k}-${sl.i}`} x={f(ptX(cx, rL, mid))} y={f(ptY(cy, rL, mid) + fs * 0.35)}
            fontSize={fs} textAnchor="middle" fill={inkFor(fill)}
          >
            {labelText(sl)}
          </text>,
        )
      }
    }
  })

  // Single-ring labels: inside at 2/3 radius, or outside with a leader line.
  // Angle order makes outside labels monotonic in y per side, so a simple
  // "previous label on this side" distance check prevents overlaps.
  if (wantLabels) {
    let lastYRight = -Infinity
    let lastYLeft = Infinity
    for (const sl of slicesPerRing[0]) {
      const mid = (sl.a0 + sl.a1) / 2
      const dx = explode > 0 ? offR * Math.sin(mid) : 0
      const dy = explode > 0 ? -offR * Math.cos(mid) : 0
      const txt = labelText(sl)
      if (!outSet.has(sl.i)) {
        const rL = hole + (R - hole) * 0.66
        labelEls.push(
          <text
            key={`di${sl.i}`} x={f(ptX(cx + dx, rL, mid))} y={f(ptY(cy + dy, rL, mid) + fs * 0.35)}
            fontSize={fs} textAnchor="middle" fill={inkFor(sliceColor(data.pointColors, sl.i))}
          >
            {txt}
          </text>,
        )
      } else {
        const right = Math.sin(mid) >= 0
        const ay = ptY(cy + dy, R, mid)
        if (right) { if (ay - lastYRight < fs * 1.15) continue; lastYRight = ay }
        else { if (lastYLeft - ay < fs * 1.15) continue; lastYLeft = ay }
        const ax = ptX(cx + dx, R, mid)
        const ex = ptX(cx + dx, R + 7, mid)
        const ey = ptY(cy + dy, R + 7, mid)
        const tx = ex + (right ? 6 : -6)
        els.push(
          <polyline
            key={`ld${sl.i}`} points={`${f(ax)},${f(ay)} ${f(ex)},${f(ey)} ${f(tx)},${f(ey)}`}
            fill="none" stroke={CHART_NEUTRAL}
          />,
        )
        labelEls.push(
          <text
            key={`do${sl.i}`} x={f(tx + (right ? 3 : -3))} y={f(ey + fs * 0.35)}
            fontSize={fs} textAnchor={right ? 'start' : 'end'}
            fill="var(--color-text-primary, #202124)"
          >
            {txt}
          </text>,
        )
      }
    }
  }

  return <g fontFamily="inherit">{els}{labelEls}</g>
}
