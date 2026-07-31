// Cartesian plot renderer: columns/bars (grouped, stacked, percent — bipolar
// stacking for negatives), lines (straight or Catmull-Rom smoothed, optional
// markers), areas (plain/stacked), XY scatter and the column+line combo.
// Pure component: props → SVG, no state beyond React's useId for the clip.

import { useId } from 'react'
import type { ReactNode } from 'react'
import { BAR_GAP_RATIO, FONT_DATA_LABEL } from '../model'
import type { ChartSpec } from '../model'
import type { ChartData } from '../data'
import { autoScaleLinear, computeValueScale, computeXExtent } from '../scale'
import type { AxisScale } from '../scale'
import { approxMeasure } from '../layout'
import type { ChartLayout, TextMeasurer } from '../layout'
import { Axes, measureAxes } from './axes'
import { computeStacks, pointCount, categoryLabels } from './stacks'
import { linePath, areaPath, Marker, symbolFor, hoverTip } from './marks'
import type { Pt } from './marks'
import { formatDataValue, formatShare, inkFor } from './format'

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi)

/** Scatter X value scale (chart2 margins; manual axisX bounds honoured). */
function scatterXScale(spec: ChartSpec, data: ChartData, widthPx: number): AxisScale {
  const nTicks = clamp(Math.floor(widthPx / 56), 2, 10)
  const ext = computeXExtent(data)
  const auto = autoScaleLinear(ext.min, ext.max, nTicks, { expandCloseToBorder: true })
  const min = spec.axisX.min ?? auto.min
  const max = spec.axisX.max ?? auto.max
  if (min === auto.min && max === auto.max) return auto
  const span = autoScaleLinear(min, max, nTicks)
  return { min, max, step: span.step, minorCount: auto.minorCount }
}

export interface CartesianPlotProps {
  spec: ChartSpec
  data: ChartData
  layout: ChartLayout
  measure?: TextMeasurer
}

export function CartesianPlot({ spec, data, layout, measure }: CartesianPlotProps): ReactNode {
  const clipId = useId()
  const m = measure ?? approxMeasure
  const plot = layout.plot
  const swapped = spec.kind === 'bar'
  const isScatter = spec.kind === 'scatter'
  if (plot.w < 24 || plot.h < 20) return null

  const n = pointCount(data)
  const cats = categoryLabels(data, n)
  const percent = spec.stacking === 'percent'
  const valueLen = swapped ? plot.w : plot.h
  const yScale = computeValueScale(spec, data, clamp(Math.floor(valueLen / 26), 2, 10))
  const xScale = isScatter ? scatterXScale(spec, data, plot.w) : null
  const axes = measureAxes(plot, spec, data, yScale, xScale, m)
  const inner = axes.inner
  if (inner.w < 4 || inner.h < 4) return null

  // ── Coordinate mapping ─────────────────────────────────────────────────────
  const ySpan = (yScale.max - yScale.min) || 1
  const vPos = (v: number) => swapped
    ? inner.x + ((v - yScale.min) / ySpan) * inner.w
    : inner.y + (1 - (v - yScale.min) / ySpan) * inner.h
  const xSpan = xScale ? (xScale.max - xScale.min) || 1 : 1
  const xPos = (v: number) => inner.x + ((v - (xScale?.min ?? 0)) / xSpan) * inner.w
  const band = (swapped ? inner.h : inner.w) / n
  // Horizontal bars: first category at the BOTTOM (LibreOffice/Excel order).
  const catSlot = (i: number) => swapped ? n - 1 - i : i
  const catCenter = (i: number) => (swapped ? inner.y : inner.x) + (catSlot(i) + 0.5) * band

  // ── Bar geometry (chart2: GapWidth 100% → gap = one bar width) ─────────────
  const stacks = computeStacks(data.series, n, spec.stacking)
  const nBars = data.series.filter(s => s.kind === 'column' || s.kind === 'bar').length
  const grouped = spec.stacking === 'none'
  const barW = Math.max(grouped ? band / (Math.max(nBars, 1) + BAR_GAP_RATIO) : band / (1 + BAR_GAP_RATIO), 0.5)
  const barStart = (barIdx: number, i: number) => {
    const s0 = (swapped ? inner.y : inner.x) + catSlot(i) * band
    return grouped ? s0 + (band - nBars * barW) / 2 + barIdx * barW : s0 + (band - barW) / 2
  }

  const tip = (name: string, cat: string, p: { v: number | null; share: number }) => {
    const val = p.v == null ? '' : formatDataValue(p.v)
    const pct = percent ? ` (${formatShare(p.share)})` : ''
    return `${name}\n${cat}: ${val}${pct}`
  }
  const pointColor = (s: { color: string }, i: number) =>
    (data.series.length === 1 ? data.pointColors?.[i] : undefined) ?? s.color

  // ── Mark layers (areas behind bars behind lines behind points) ─────────────
  const areaEls: ReactNode[] = []
  const barEls: ReactNode[] = []
  const lineEls: ReactNode[] = []
  const ptEls: ReactNode[] = []
  const labelEls: ReactNode[] = []
  let barIdx = 0

  data.series.forEach((s, si) => {
    const st = stacks[si]
    if (s.kind === 'column' || s.kind === 'bar') {
      const bi = barIdx++
      for (let i = 0; i < n; i++) {
        const p = st[i]
        if (p.v == null) continue
        const a = vPos(p.base), b = vPos(p.top)
        const off = barStart(bi, i)
        const r = swapped
          ? { x: Math.min(a, b), y: off, width: Math.abs(b - a), height: barW }
          : { x: off, y: Math.min(a, b), width: barW, height: Math.abs(b - a) }
        barEls.push(
          <rect
            key={`b${si}-${i}`} {...r} fill={pointColor(s, i)}
            stroke={grouped ? 'none' : 'var(--color-surface-0, #ffffff)'}
            strokeWidth={grouped ? 0 : 1}
            {...hoverTip(tip(s.name, cats[i], p))}
          />,
        )
      }
    } else if (s.kind === 'area') {
      const top: (Pt | null)[] = []
      const bot: (Pt | null)[] = []
      for (let i = 0; i < n; i++) {
        const p = st[i]
        if (p.v == null) { top.push(null); bot.push(null); continue }
        top.push({ x: catCenter(i), y: vPos(p.top) })
        bot.push({ x: catCenter(i), y: vPos(p.base) })
      }
      areaEls.push(
        <path
          key={`a${si}`} d={areaPath(top, bot, spec.smooth)} fill={s.color}
          fillOpacity={spec.stacking === 'none' && data.series.length > 1 ? 0.7 : 1}
          {...hoverTip(s.name)}
        />,
      )
    } else if (s.kind === 'scatter') {
      const pts: (Pt | null)[] = s.values.map((v, i) => {
        const xv = s.x ? s.x[i] : i + 1
        return v == null || xv == null ? null : { x: xPos(xv), y: vPos(v) }
      })
      if (spec.lines) {
        lineEls.push(<path key={`l${si}`} d={linePath(pts, spec.smooth)} fill="none"
          stroke={s.color} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />)
      }
      pts.forEach((pt, i) => {
        if (!pt) return
        ptEls.push(<Marker key={`p${si}-${i}`} shape={symbolFor(si)} cx={pt.x} cy={pt.y} r={3.8}
          fill={pointColor(s, i)} title={tip(s.name, s.x ? `(${formatDataValue(s.x[i] ?? 0)})` : cats[i], st[i])} />)
      })
    } else { // 'line' (including combo line series)
      const pts: (Pt | null)[] = st.map((p, i) => p.v == null ? null : { x: catCenter(i), y: vPos(p.top) })
      if (spec.lines || spec.kind === 'combo') {
        lineEls.push(<path key={`l${si}`} d={linePath(pts, spec.smooth)} fill="none"
          stroke={s.color} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />)
      }
      pts.forEach((pt, i) => {
        if (!pt) return
        if (spec.symbols || !spec.lines) {
          ptEls.push(<Marker key={`p${si}-${i}`} shape={symbolFor(si)} cx={pt.x} cy={pt.y} r={3.8}
            fill={pointColor(s, i)} title={tip(s.name, cats[i], st[i])} />)
        } else {
          // Invisible hover target so tooltips work on bare lines.
          ptEls.push(
            <circle key={`h${si}-${i}`} cx={pt.x} cy={pt.y} r={8} fill="transparent"
              {...hoverTip(tip(s.name, cats[i], st[i]))} />,
          )
        }
      })
    }

    // ── Data labels ──────────────────────────────────────────────────────────
    if (!spec.dataLabels) return
    const isBarSeries = s.kind === 'column' || s.kind === 'bar'
    if (isBarSeries && barW < 14) return
    const rhythm = isBarSeries ? 1 : Math.max(1, Math.ceil(30 / band))
    const fs = FONT_DATA_LABEL
    for (let i = 0; i < n; i++) {
      const p = st[i]
      if (p.v == null || i % rhythm !== 0) continue
      const label = spec.dataLabels === 'percent' ? formatShare(p.share) : formatDataValue(p.v)
      if (isBarSeries) {
        const a = vPos(p.base), b = vPos(p.top)
        const off = barStart(barSlotIndex(data, si), i) + barW / 2
        if (!grouped) {
          // Stacked: centered inside the segment, skipped when too small.
          if (Math.abs(b - a) < 11) continue
          const c = (a + b) / 2
          labelEls.push(swapped
            ? <text key={`d${si}-${i}`} x={c} y={off + fs * 0.35} fontSize={fs} textAnchor="middle" fill={inkFor(pointColor(s, i))}>{label}</text>
            : <text key={`d${si}-${i}`} x={off} y={c + fs * 0.35} fontSize={fs} textAnchor="middle" fill={inkFor(pointColor(s, i))}>{label}</text>)
        } else {
          // Grouped: just outside the bar end (chart2 "outside" default).
          const end = vPos(p.top)
          labelEls.push(swapped
            ? <text key={`d${si}-${i}`} x={end + (p.v >= 0 ? 3 : -3)} y={off + fs * 0.35} fontSize={fs}
                textAnchor={p.v >= 0 ? 'start' : 'end'} fill="var(--color-text-primary, #202124)">{label}</text>
            : <text key={`d${si}-${i}`} x={off} y={end + (p.v >= 0 ? -3 : fs + 1)} fontSize={fs}
                textAnchor="middle" fill="var(--color-text-primary, #202124)">{label}</text>)
        }
      } else {
        // Lines / areas / scatter: above the point.
        const xv = s.kind === 'scatter' && s.x ? s.x[i] : null
        const cx = s.kind === 'scatter' ? (xv == null && !s.x ? xPos(i + 1) : xv == null ? null : xPos(xv)) : catCenter(i)
        if (cx == null) continue
        labelEls.push(<text key={`d${si}-${i}`} x={cx} y={vPos(p.top) - 6} fontSize={fs}
          textAnchor="middle" fill="var(--color-text-primary, #202124)">{label}</text>)
      }
    }
  })

  const pad = 4 // let markers/strokes bleed slightly over the inner edge
  return (
    <g fontFamily="inherit">
      <Axes plot={plot} inner={inner} spec={spec} data={data} yScale={yScale}
        xScale={xScale} swapped={swapped} axes={axes} measure={m} />
      <defs>
        <clipPath id={clipId}>
          <rect x={inner.x - pad} y={inner.y - pad} width={inner.w + 2 * pad} height={inner.h + 2 * pad} />
        </clipPath>
      </defs>
      <g clipPath={`url(#${clipId})`}>
        {areaEls}
        {barEls}
        {lineEls}
        {ptEls}
      </g>
      {labelEls}
    </g>
  )
}

/** Index of `si` among the bar-drawn series (bar slot for label placement). */
function barSlotIndex(data: ChartData, si: number): number {
  let idx = 0
  for (let i = 0; i < si; i++) {
    const k = data.series[i].kind
    if (k === 'column' || k === 'bar') idx++
  }
  return idx
}
