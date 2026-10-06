// Radar (net) renderer, chart2 NetChart flavour: a polygonal value grid with
// one spoke per category (first category at 12 o'clock, clockwise, matching
// the pie orientation), series drawn as closed polygons — contour lines,
// optional markers and an optional translucent fill (FilledNet). Stacking and
// the value scale reuse the shared stacks.ts / scale.ts machinery.

import type { ReactNode } from 'react'
import { CHART_NEUTRAL, FONT_AXIS_LABEL, FONT_DATA_LABEL } from '../model'
import type { ChartSpec } from '../model'
import type { ChartData } from '../data'
import { computeValueScale, ticksOf } from '../scale'
import { approxMeasure } from '../layout'
import type { ChartLayout, TextMeasurer } from '../layout'
import { computeStacks, pointCount, categoryLabels } from './stacks'
import { linePath, Marker, symbolFor, hoverTip } from './marks'
import type { Pt } from './marks'
import { formatAxisTick, formatDataValue, formatShare, truncateToWidth } from './format'

const TWO_PI = Math.PI * 2
const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi)
const f = (v: number) => Math.round(v * 100) / 100

const INK = 'var(--color-text-secondary, #5f6368)'

export interface RadarPlotProps {
  spec: ChartSpec
  data: ChartData
  layout: ChartLayout
  measure?: TextMeasurer
}

export function RadarPlot({ spec, data, layout, measure }: RadarPlotProps): ReactNode {
  const m = measure ?? approxMeasure
  const plot = layout.plot
  if (plot.w < 48 || plot.h < 48) return null
  const n = pointCount(data)
  const percent = spec.stacking === 'percent'
  const catsRaw = categoryLabels(data, n)
  const lfs = FONT_AXIS_LABEL
  const maxCatW = Math.min(plot.w * 0.22, 80)
  const cats = catsRaw.map(c => truncateToWidth(c, maxCatW, lfs, m))
  const catW = cats.reduce((w, c) => Math.max(w, m(c, lfs).w), 0)

  const cx = plot.x + plot.w / 2
  const cy = plot.y + plot.h / 2
  const R = Math.max(Math.min(plot.w / 2 - catW - 10, plot.h / 2 - lfs * 1.3 - 6), 12)
  const scale = computeValueScale(spec, data, clamp(Math.floor(R / 28), 2, 6))
  const span = (scale.max - scale.min) || 1
  const rOf = (v: number) => clamp((v - scale.min) / span, 0, 1) * R
  const ang = (i: number) => (i * TWO_PI) / Math.max(n, 1)
  const pX = (r: number, a: number) => cx + r * Math.sin(a)
  const pY = (r: number, a: number) => cy - r * Math.cos(a)

  const gridEls: ReactNode[] = []
  const textEls: ReactNode[] = []

  // ── Net grid: a polygonal ring per major tick + one spoke per category ─────
  const ticks = ticksOf(scale)
  ticks.forEach((t, ti) => {
    const r = rOf(t)
    if (r < 1) return
    if (n >= 3) {
      const pts = Array.from({ length: n }, (_, i) => `${f(pX(r, ang(i)))},${f(pY(r, ang(i)))}`).join(' ')
      gridEls.push(<polygon key={`g${ti}`} points={pts} fill="none" stroke={CHART_NEUTRAL} strokeOpacity={0.55} />)
    } else {
      // Degenerate nets (1–2 categories) fall back to circular rings.
      gridEls.push(<circle key={`g${ti}`} cx={f(cx)} cy={f(cy)} r={f(r)} fill="none" stroke={CHART_NEUTRAL} strokeOpacity={0.55} />)
    }
  })
  for (let i = 0; i < n; i++) {
    gridEls.push(
      <line key={`sp${i}`} x1={f(cx)} y1={f(cy)} x2={f(pX(R, ang(i)))} y2={f(pY(R, ang(i)))}
        stroke={CHART_NEUTRAL} strokeOpacity={0.55} />,
    )
  }

  // Value tick labels along the 12-o'clock spoke.
  ticks.forEach((t, ti) => {
    const r = rOf(t)
    if (ti > 0 && r < lfs) return // skip labels crowding the center
    textEls.push(
      <text key={`tv${ti}`} x={f(cx + 4)} y={f(cy - r + lfs * 0.35)} fontSize={lfs} fill={INK}>
        {formatAxisTick(t, scale.step, { percent, pattern: spec.axisY.format })}
      </text>,
    )
  })

  // Category labels at the spoke ends, anchored by their side of the net.
  cats.forEach((c, i) => {
    if (!c) return
    const a = ang(i)
    const s = Math.sin(a), co = Math.cos(a)
    const anchor = Math.abs(s) < 0.35 ? 'middle' : s > 0 ? 'start' : 'end'
    // Baseline shift: above the net at the top, below at the bottom.
    const dy = lfs * (0.35 - 0.5 * co)
    textEls.push(
      <text key={`c${i}`} x={f(pX(R + 6, a))} y={f(pY(R + 6, a) + dy)} fontSize={lfs}
        textAnchor={anchor} fill={INK}>
        {c}
      </text>,
    )
  })

  // ── Series polygons ────────────────────────────────────────────────────────
  const stacks = computeStacks(data.series, n, spec.stacking)
  // A net with neither lines nor fill would be invisible: force markers then.
  const showSymbols = spec.symbols || (!spec.lines && !spec.filled)
  const fillEls: ReactNode[] = []
  const lineEls: ReactNode[] = []
  const ptEls: ReactNode[] = []
  const labelEls: ReactNode[] = []

  data.series.forEach((s, si) => {
    const st = stacks[si]
    const pts: (Pt | null)[] = []
    for (let i = 0; i < n; i++) {
      const p = st[i]
      pts.push(p.v == null ? null : { x: pX(rOf(p.top), ang(i)), y: pY(rOf(p.top), ang(i)) })
    }
    const solid = pts.filter((p): p is Pt => p != null)
    if (spec.filled && solid.length >= 3) {
      fillEls.push(
        <polygon key={`f${si}`} points={solid.map(p => `${f(p.x)},${f(p.y)}`).join(' ')}
          fill={s.color} fillOpacity={0.4} stroke="none" />,
      )
    }
    if (spec.lines || spec.filled) {
      // Close the polygon by repeating the first point; nulls break the runs.
      const closed = n >= 2 ? [...pts, pts[0]] : pts
      lineEls.push(
        <path key={`l${si}`} d={linePath(closed, false)} fill="none" stroke={s.color}
          strokeWidth={spec.filled ? 1.5 : 2} strokeLinejoin="round" strokeLinecap="round" />,
      )
    }
    pts.forEach((pt, i) => {
      if (!pt) return
      const p = st[i]
      const val = p.v == null ? '' : formatDataValue(p.v)
      const tip = `${s.name}\n${catsRaw[i]}: ${val}${percent ? ` (${formatShare(p.share)})` : ''}`
      if (showSymbols) {
        ptEls.push(<Marker key={`p${si}-${i}`} shape={symbolFor(si)} cx={pt.x} cy={pt.y} r={3.5} fill={s.color} title={tip} />)
      } else {
        // Invisible hover target so tooltips work on bare polygons.
        ptEls.push(
          <circle key={`h${si}-${i}`} cx={f(pt.x)} cy={f(pt.y)} r={7} fill="transparent"
            {...hoverTip(tip)} />,
        )
      }
      if (spec.dataLabels && p.v != null) {
        // Radially outward from the point, clear of the polygon edge.
        const a = ang(i)
        labelEls.push(
          <text key={`d${si}-${i}`} x={f(pt.x + 8 * Math.sin(a))} y={f(pt.y - 8 * Math.cos(a) + FONT_DATA_LABEL * 0.35)}
            fontSize={FONT_DATA_LABEL} textAnchor="middle" fill="var(--color-text-primary, #202124)">
            {spec.dataLabels === 'percent' ? formatShare(p.share) : formatDataValue(p.v)}
          </text>,
        )
      }
    })
  })

  return <g fontFamily="inherit">{gridEls}{fillEls}{lineEls}{ptEls}{textEls}{labelEls}</g>
}
