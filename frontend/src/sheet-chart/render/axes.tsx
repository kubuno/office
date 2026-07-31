// Axis measurement & rendering: value ticks, category labels with the chart2
// anti-overlap cascade (flat → stagger → 45° → 90° → label rhythm), major
// gridlines and an emphasized zero line when the scale crosses zero.

import type { ReactNode } from 'react'
import { CHART_NEUTRAL, FONT_AXIS_LABEL } from '../model'
import type { ChartSpec } from '../model'
import type { ChartData } from '../data'
import { ticksOf } from '../scale'
import type { AxisScale } from '../scale'
import { approxMeasure } from '../layout'
import type { Rect, TextMeasurer } from '../layout'
import { formatAxisTick, truncateToWidth } from './format'
import { pointCount, categoryLabels } from './stacks'

export type XLabelMode = 'flat' | 'stagger' | 'rot45' | 'rot90'

export interface AxesResult {
  /** Data area: `plot` minus the label bands (marks are drawn here). */
  inner: Rect
  /** Height consumed by the bottom label band. */
  xBand: number
  /** Width consumed by the left label band. */
  yBand: number
  /** Bottom-label layout mode (categories; 'flat' for value axes). */
  xMode: XLabelMode
  /** Draw only every n-th bottom / left label. */
  xRhythm: number
  yRhythm: number
  /** Prepared (truncated/formatted) labels; empty when a band is hidden. */
  xLabels: string[]
  yLabels: string[]
}

const FS = FONT_AXIS_LABEL
const TICK_GAP = 6      // gap between labels and the axis line
const TOP_PAD = 5       // headroom so the topmost tick label is not clipped
const END_PAD = 4
const MAX_CAT_PX = 72   // rotated category labels are truncated to this width

const INK = 'var(--color-text-secondary, #5f6368)'

/**
 * Measure the label bands and derive the inner data area.
 * `yScale` is the value scale; `xScale` is only set for scatter (X value axis).
 * When `spec.kind === 'bar'` the axes are swapped: categories on the left,
 * values along the bottom.
 */
export function measureAxes(
  plot: Rect, spec: ChartSpec, data: ChartData, yScale: AxisScale,
  xScale: AxisScale | null, measure: TextMeasurer = approxMeasure,
): AxesResult {
  const swapped = spec.kind === 'bar'
  const percent = spec.stacking === 'percent'
  const n = pointCount(data)
  const cats = categoryLabels(data, n)
  const lh = measure('Ag', FS).h
  const valLabels = ticksOf(yScale).map(v =>
    formatAxisTick(v, yScale.step, { percent, pattern: spec.axisY.format }))
  const maxW = (arr: string[]) => arr.reduce((w, t) => Math.max(w, measure(t, FS).w), 0)

  if (swapped) {
    // Left: categories. Bottom: value ticks.
    let yBand = Math.min(maxW(cats), plot.w * 0.35) + TICK_GAP
    if (plot.w - yBand < 40) yBand = 2
    let xBand = lh + 4
    if (plot.h - xBand < 24) xBand = 2
    const inner: Rect = {
      x: plot.x + yBand, y: plot.y + TOP_PAD,
      w: Math.max(0, plot.w - yBand - END_PAD), h: Math.max(0, plot.h - TOP_PAD - xBand),
    }
    const perTick = inner.w / Math.max(valLabels.length - 1, 1)
    const bandH = inner.h / n
    return {
      inner, xBand, yBand, xMode: 'flat',
      xRhythm: Math.max(1, Math.ceil((maxW(valLabels) + 8) / Math.max(perTick, 1))),
      yRhythm: Math.max(1, Math.ceil((lh + 2) / Math.max(bandH, 1))),
      xLabels: xBand > 2 ? valLabels : [],
      yLabels: yBand > 2 ? cats.map(c => truncateToWidth(c, yBand - TICK_GAP, FS, measure)) : [],
    }
  }

  // Left: value ticks.
  let yBand = Math.min(maxW(valLabels), plot.w * 0.4) + TICK_GAP
  if (plot.w - yBand < 48) yBand = 2
  const availW = Math.max(0, plot.w - yBand - END_PAD)

  let xMode: XLabelMode = 'flat'
  let xRhythm = 1
  let xLabels: string[] = []
  let xBand = lh + 4

  if (xScale) {
    // Scatter X value axis: horizontal labels, rhythm only.
    xLabels = ticksOf(xScale).map(v => formatAxisTick(v, xScale.step, { pattern: spec.axisX.format }))
    const perTick = availW / Math.max(xLabels.length - 1, 1)
    xRhythm = Math.max(1, Math.ceil((maxW(xLabels) + 8) / Math.max(perTick, 1)))
  } else {
    // Category axis: chart2 cascade against the band width.
    const band = availW / n
    const rawW = maxW(cats)
    if (rawW <= band - 4) {
      xLabels = cats.map(c => truncateToWidth(c, Math.max(band - 4, 12), FS, measure))
    } else if (n > 1 && rawW <= 2 * band - 8) {
      xMode = 'stagger'
      xBand = 2 * lh + 4
      xLabels = cats.map(c => truncateToWidth(c, 2 * band - 8, FS, measure))
    } else {
      // Rotated labels: cap their length so the band never eats the plot.
      const maxCatPx = Math.min(MAX_CAT_PX, Math.max(plot.h * 0.4, 24))
      if (band >= lh * 1.5) {
        xMode = 'rot45'
        xBand = (Math.min(rawW, maxCatPx) + lh) * 0.7071 + 4
      } else {
        xMode = 'rot90'
        xBand = Math.min(rawW, maxCatPx) + 6
        xRhythm = Math.max(1, Math.ceil((lh + 2) / Math.max(band, 1)))
      }
      xLabels = cats.map(c => truncateToWidth(c, maxCatPx, FS, measure))
    }
  }
  if (plot.h - xBand < 28) { xBand = 2; xLabels = [] }

  const inner: Rect = {
    x: plot.x + yBand, y: plot.y + TOP_PAD,
    w: availW, h: Math.max(0, plot.h - TOP_PAD - xBand),
  }
  return { inner, xBand, yBand, xMode, xRhythm, yRhythm: 1, xLabels, yLabels: yBand > 2 ? valLabels : [] }
}

export interface AxesProps {
  plot: Rect
  inner: Rect
  spec: ChartSpec
  data: ChartData
  yScale: AxisScale
  xScale: AxisScale | null
  /** kind === 'bar': categories on the left, values along the bottom. */
  swapped: boolean
  /** Pass the precomputed measurement to avoid recomputation. */
  axes?: AxesResult
  measure?: TextMeasurer
}

/** Gridlines, axis frame, zero line, tick marks and tick/category labels. */
export function Axes(props: AxesProps): ReactNode {
  const { plot, inner, spec, data, yScale, xScale, swapped } = props
  const a = props.axes ?? measureAxes(plot, spec, data, yScale, xScale, props.measure ?? approxMeasure)
  const els: ReactNode[] = []
  const ySpan = (yScale.max - yScale.min) || 1
  const vPos = (v: number) => swapped
    ? inner.x + ((v - yScale.min) / ySpan) * inner.w
    : inner.y + (1 - (v - yScale.min) / ySpan) * inner.h
  const xSpan = xScale ? (xScale.max - xScale.min) || 1 : 1
  const xPos = (v: number) => inner.x + ((v - (xScale?.min ?? 0)) / xSpan) * inner.w
  const yTicks = ticksOf(yScale)
  const n = pointCount(data)
  const band = (swapped ? inner.h : inner.w) / n
  const bottom = inner.y + inner.h
  const right = inner.x + inner.w
  const lh = FS * 1.3

  // Major gridlines of the VALUE axis (spec.grid.y), perpendicular to it.
  if (spec.grid.y) {
    yTicks.forEach((t, i) => {
      const p = vPos(t)
      els.push(swapped
        ? <line key={`gv${i}`} x1={p} x2={p} y1={inner.y} y2={bottom} stroke={CHART_NEUTRAL} strokeOpacity={0.45} />
        : <line key={`gv${i}`} x1={inner.x} x2={right} y1={p} y2={p} stroke={CHART_NEUTRAL} strokeOpacity={0.45} />)
    })
  }
  // Major gridlines of the CATEGORY / X axis (spec.grid.x).
  if (spec.grid.x) {
    if (xScale) {
      ticksOf(xScale).forEach((t, i) => {
        const p = xPos(t)
        els.push(<line key={`gx${i}`} x1={p} x2={p} y1={inner.y} y2={bottom} stroke={CHART_NEUTRAL} strokeOpacity={0.45} />)
      })
    } else {
      for (let i = 0; i <= n; i++) {
        const p = (swapped ? inner.y : inner.x) + i * band
        els.push(swapped
          ? <line key={`gx${i}`} x1={inner.x} x2={right} y1={p} y2={p} stroke={CHART_NEUTRAL} strokeOpacity={0.45} />
          : <line key={`gx${i}`} x1={p} x2={p} y1={inner.y} y2={bottom} stroke={CHART_NEUTRAL} strokeOpacity={0.45} />)
      }
    }
  }

  // Axis frame (left + bottom, chart2 gray30).
  els.push(<line key="axl" x1={inner.x} x2={inner.x} y1={inner.y} y2={bottom} stroke={CHART_NEUTRAL} />)
  els.push(<line key="axb" x1={inner.x} x2={right} y1={bottom} y2={bottom} stroke={CHART_NEUTRAL} />)

  // Emphasized zero line when the value scale crosses zero.
  if (yScale.min < 0 && yScale.max > 0) {
    const p = vPos(0)
    els.push(swapped
      ? <line key="zero" x1={p} x2={p} y1={inner.y} y2={bottom} stroke="var(--color-text-tertiary, #80868b)" />
      : <line key="zero" x1={inner.x} x2={right} y1={p} y2={p} stroke="var(--color-text-tertiary, #80868b)" />)
  }

  // Value-axis tick marks (3px, outside).
  yTicks.forEach((t, i) => {
    const p = vPos(t)
    els.push(swapped
      ? <line key={`tv${i}`} x1={p} x2={p} y1={bottom} y2={bottom + 3} stroke={CHART_NEUTRAL} />
      : <line key={`tv${i}`} x1={inner.x - 3} x2={inner.x} y1={p} y2={p} stroke={CHART_NEUTRAL} />)
  })

  const text = (key: string, x: number, y: number, s: string, anchor: 'start' | 'middle' | 'end', transform?: string) => (
    <text key={key} x={x} y={y} fontSize={FS} fill={INK} textAnchor={anchor} transform={transform}>{s}</text>
  )

  if (swapped) {
    // Left: category labels, vertically centered per band, rhythm-thinned.
    // First category at the BOTTOM (LibreOffice/Excel horizontal-bar order).
    a.yLabels.forEach((label, i) => {
      if (i % a.yRhythm !== 0 || !label) return
      const cy = inner.y + (n - 1 - i + 0.5) * band + FS * 0.35
      els.push(text(`ly${i}`, inner.x - TICK_GAP + 2, cy, label, 'end'))
    })
    // Bottom: value tick labels.
    a.xLabels.forEach((label, i) => {
      if (i % a.xRhythm !== 0) return
      els.push(text(`lx${i}`, vPos(yTicks[i] ?? yScale.min), bottom + lh * 0.75, label, 'middle'))
    })
    return <g>{els}</g>
  }

  // Left: value tick labels.
  a.yLabels.forEach((label, i) => {
    if (i % a.yRhythm !== 0) return
    els.push(text(`ly${i}`, inner.x - TICK_GAP + 2, vPos(yTicks[i] ?? yScale.min) + FS * 0.35, label, 'end'))
  })
  // Bottom: category labels (cascade mode) or scatter X ticks.
  const xTickVals = xScale ? ticksOf(xScale) : null
  a.xLabels.forEach((label, i) => {
    if (i % a.xRhythm !== 0 || !label) return
    const cx = xTickVals ? xPos(xTickVals[i] ?? 0) : inner.x + (i + 0.5) * band
    switch (a.xMode) {
      case 'stagger':
        els.push(text(`lx${i}`, cx, bottom + lh * (i % 2 === 0 ? 0.75 : 1.75), label, 'middle'))
        break
      case 'rot45':
        els.push(text(`lx${i}`, cx, bottom + lh * 0.7, label, 'end', `rotate(-45 ${cx} ${bottom + lh * 0.7})`))
        break
      case 'rot90':
        els.push(text(`lx${i}`, cx + FS * 0.35, bottom + 4, label, 'end', `rotate(-90 ${cx + FS * 0.35} ${bottom + 4})`))
        break
      default:
        els.push(text(`lx${i}`, cx, bottom + lh * 0.75, label, 'middle'))
    }
  })
  return <g>{els}</g>
}
