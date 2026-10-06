// Column + line combo with a secondary Y axis on the right (chart2 "bar and
// line" with series attached to a second axis). ChartView routes here only
// when a series declares `axis: 'secondary'`; plain combos stay on the shared
// cartesian renderer. Each axis group gets its own ScaleAutomatism scale and,
// under stacking, its own stack accumulation (chart2: one coordinate system
// per attached axis). Visuals mirror cartesian.tsx (palette, gaps, labels,
// tooltips) for strict consistency.

import { useId } from 'react'
import type { ReactNode } from 'react'
import { BAR_GAP_RATIO, CHART_NEUTRAL, FONT_AXIS_LABEL, FONT_DATA_LABEL } from '../model'
import type { ChartSpec } from '../model'
import type { ChartData } from '../data'
import { computeValueScale, ticksOf } from '../scale'
import { approxMeasure } from '../layout'
import type { ChartLayout, Rect, TextMeasurer } from '../layout'
import { Axes, measureAxes } from './axes'
import { computeStacks, pointCount, categoryLabels } from './stacks'
import type { StackedPoint } from './stacks'
import { linePath, Marker, symbolFor, hoverTip } from './marks'
import type { Pt } from './marks'
import { formatAxisTick, formatDataValue, formatShare, inkFor } from './format'

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi)

/** True when the resolved data uses a secondary value axis. */
export function hasSecondaryAxis(data: ChartData): boolean {
  return data.series.some(s => s.axis === 'secondary')
}

export interface ComboPlotProps {
  spec: ChartSpec
  data: ChartData
  layout: ChartLayout
  measure?: TextMeasurer
}

export function ComboPlot({ spec, data, layout, measure }: ComboPlotProps): ReactNode {
  const clipId = useId()
  const m = measure ?? approxMeasure
  const plot = layout.plot
  if (plot.w < 40 || plot.h < 20) return null
  const n = pointCount(data)
  const cats = categoryLabels(data, n)
  const percent = spec.stacking === 'percent'
  const grouped = spec.stacking === 'none'

  const prim = data.series.filter(s => s.axis !== 'secondary')
  const sec = data.series.filter(s => s.axis === 'secondary')
  const nTicks = clamp(Math.floor(plot.h / 26), 2, 10)
  const yScale = computeValueScale(spec, { ...data, series: prim.length > 0 ? prim : data.series }, nTicks)
  // Secondary axis: independent auto-scale — the model's manual bounds
  // (axisY.min/max) belong to the primary axis only.
  const spec2: ChartSpec = { ...spec, axisY: { format: spec.axisY.format } }
  const y2Scale = sec.length > 0 ? computeValueScale(spec2, { ...data, series: sec }, nTicks) : yScale

  // Right label band for the secondary axis (mirrors measureAxes' left band).
  const y2Labels = ticksOf(y2Scale).map(v => formatAxisTick(v, y2Scale.step, { percent, pattern: spec.axisY.format }))
  const band2 = Math.min(y2Labels.reduce((w, t) => Math.max(w, m(t, FONT_AXIS_LABEL).w), 0), plot.w * 0.25) + 8
  const plotMain: Rect = { x: plot.x, y: plot.y, w: Math.max(0, plot.w - band2), h: plot.h }
  const axes = measureAxes(plotMain, spec, data, yScale, null, m)
  const inner = axes.inner
  if (inner.w < 4 || inner.h < 4) return null

  // ── Coordinate mapping, one value scale per axis group ─────────────────────
  const span1 = (yScale.max - yScale.min) || 1
  const span2 = (y2Scale.max - y2Scale.min) || 1
  const vPos1 = (v: number) => inner.y + (1 - (v - yScale.min) / span1) * inner.h
  const vPos2 = (v: number) => inner.y + (1 - (v - y2Scale.min) / span2) * inner.h
  const band = inner.w / n
  const catCenter = (i: number) => inner.x + (i + 0.5) * band

  // Per-axis stacking, mapped back to the original series order.
  const stacksP = computeStacks(prim, n, spec.stacking)
  const stacksS = computeStacks(sec, n, spec.stacking)
  const stacks: StackedPoint[][] = []
  { let pi = 0, sj = 0
    for (const s of data.series) stacks.push(s.axis === 'secondary' ? stacksS[sj++] : stacksP[pi++]) }

  // Bar slots: grouped → one slot per column series; stacked → one stack per
  // axis group that has columns (primary and secondary stacks side by side).
  const colSeries = data.series.filter(s => s.kind === 'column' || s.kind === 'bar')
  const primHasCols = colSeries.some(s => s.axis !== 'secondary')
  const secHasCols = colSeries.some(s => s.axis === 'secondary')
  const nSlots = Math.max(grouped ? colSeries.length : (primHasCols ? 1 : 0) + (secHasCols ? 1 : 0), 1)
  const barW = Math.max(band / (nSlots + BAR_GAP_RATIO), 0.5)
  const barX = (slot: number, i: number) => inner.x + i * band + (band - nSlots * barW) / 2 + slot * barW

  const tip = (name: string, cat: string, p: StackedPoint) => {
    const val = p.v == null ? '' : formatDataValue(p.v)
    return `${name}\n${cat}: ${val}${percent ? ` (${formatShare(p.share)})` : ''}`
  }

  const barEls: ReactNode[] = []
  const lineEls: ReactNode[] = []
  const ptEls: ReactNode[] = []
  const labelEls: ReactNode[] = []
  const fs = FONT_DATA_LABEL
  let colIdx = 0

  data.series.forEach((s, si) => {
    const st = stacks[si]
    const secondary = s.axis === 'secondary'
    const vPos = secondary ? vPos2 : vPos1
    if (s.kind === 'column' || s.kind === 'bar') {
      const slot = grouped ? colIdx++ : (secondary ? (primHasCols ? 1 : 0) : 0)
      for (let i = 0; i < n; i++) {
        const p = st[i]
        if (p.v == null) continue
        const a = vPos(p.base), b = vPos(p.top)
        barEls.push(
          <rect
            key={`b${si}-${i}`} x={barX(slot, i)} y={Math.min(a, b)} width={barW} height={Math.abs(b - a)}
            fill={s.color} stroke={grouped ? 'none' : 'var(--color-surface-0, #ffffff)'} strokeWidth={grouped ? 0 : 1}
            {...hoverTip(tip(s.name, cats[i], p))}
          />,
        )
        if (spec.dataLabels && barW >= 14) {
          const label = spec.dataLabels === 'percent' ? formatShare(p.share) : formatDataValue(p.v)
          const cxm = barX(slot, i) + barW / 2
          if (grouped) {
            // Grouped: just outside the bar end (chart2 "outside" default).
            labelEls.push(<text key={`d${si}-${i}`} x={cxm} y={b + (p.v >= 0 ? -3 : fs + 1)} fontSize={fs}
              textAnchor="middle" fill="var(--color-text-primary, #202124)">{label}</text>)
          } else if (Math.abs(b - a) >= 11) {
            // Stacked: centered inside the segment, skipped when too small.
            labelEls.push(<text key={`d${si}-${i}`} x={cxm} y={(a + b) / 2 + fs * 0.35} fontSize={fs}
              textAnchor="middle" fill={inkFor(s.color)}>{label}</text>)
          }
        }
      }
    } else { // 'line'
      const pts: (Pt | null)[] = st.map((p, i) => p.v == null ? null : { x: catCenter(i), y: vPos(p.top) })
      lineEls.push(<path key={`l${si}`} d={linePath(pts, spec.smooth)} fill="none"
        stroke={s.color} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />)
      const rhythm = Math.max(1, Math.ceil(30 / band))
      pts.forEach((pt, i) => {
        if (!pt) return
        const p = st[i]
        if (spec.symbols) {
          ptEls.push(<Marker key={`p${si}-${i}`} shape={symbolFor(si)} cx={pt.x} cy={pt.y} r={3.8}
            fill={s.color} title={tip(s.name, cats[i], p)} />)
        } else {
          // Invisible hover target so tooltips work on bare lines.
          ptEls.push(
            <circle key={`h${si}-${i}`} cx={pt.x} cy={pt.y} r={8} fill="transparent"
              {...hoverTip(tip(s.name, cats[i], p))} />,
          )
        }
        if (spec.dataLabels && p.v != null && i % rhythm === 0) {
          labelEls.push(<text key={`d${si}-${i}`} x={pt.x} y={pt.y - 6} fontSize={fs}
            textAnchor="middle" fill="var(--color-text-primary, #202124)">
            {spec.dataLabels === 'percent' ? formatShare(p.share) : formatDataValue(p.v)}
          </text>)
        }
      })
    }
  })

  // ── Secondary axis: frame line, tick marks and labels on the right edge ────
  const axis2Els: ReactNode[] = []
  if (sec.length > 0) {
    const rx = inner.x + inner.w
    const bottom = inner.y + inner.h
    axis2Els.push(<line key="ax2" x1={rx} x2={rx} y1={inner.y} y2={bottom} stroke={CHART_NEUTRAL} />)
    ticksOf(y2Scale).forEach((t, i) => {
      const y = vPos2(t)
      axis2Els.push(<line key={`t2${i}`} x1={rx} x2={rx + 3} y1={y} y2={y} stroke={CHART_NEUTRAL} />)
      axis2Els.push(
        <text key={`l2${i}`} x={rx + 5} y={y + FONT_AXIS_LABEL * 0.35} fontSize={FONT_AXIS_LABEL}
          fill="var(--color-text-secondary, #5f6368)">
          {y2Labels[i]}
        </text>,
      )
    })
  }

  const pad = 4 // let markers/strokes bleed slightly over the inner edge
  return (
    <g fontFamily="inherit">
      <Axes plot={plotMain} inner={inner} spec={spec} data={data} yScale={yScale}
        xScale={null} swapped={false} axes={axes} measure={m} />
      {axis2Els}
      <defs>
        <clipPath id={clipId}>
          <rect x={inner.x - pad} y={inner.y - pad} width={inner.w + 2 * pad} height={inner.h + 2 * pad} />
        </clipPath>
      </defs>
      <g clipPath={`url(#${clipId})`}>
        {barEls}
        {lineEls}
        {ptEls}
      </g>
      {labelEls}
    </g>
  )
}
