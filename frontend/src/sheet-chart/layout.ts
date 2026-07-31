// Page layout: split the chart box into title / legend / axis titles / plot,
// following chart2's ChartView + VLegend flow (subtract elements in order,
// then a 2% page margin around the plot area).

import {
  ChartSpec, LegendPosition, PAGE_MARGIN_RATIO, LEGEND_MAX_WIDTH_RATIO,
  FONT_TITLE, FONT_LEGEND, FONT_AXIS_TITLE,
} from './model'
import { ChartData } from './data'
import { isCartesian } from './normalize'

export interface Rect { x: number; y: number; w: number; h: number }
export interface Size { w: number; h: number }

/** Text metrics provider; hosts may plug canvas-based measurement. */
export type TextMeasurer = (text: string, fontSize: number) => Size

/** Cheap width heuristic (≈ average latin glyph advance) + line height. */
export const approxMeasure: TextMeasurer = (text, fontSize) => ({
  w: text.length * fontSize * 0.58,
  h: fontSize * 1.3,
})

export interface PlacedText { text: string; x: number; y: number; fontSize: number }

export interface LegendEntry {
  label: string
  color: string
  /** Position of the entry's symbol box, relative to the legend rect. */
  x: number
  y: number
}

export interface LegendLayout {
  rect: Rect
  entries: LegendEntry[]
  symbolSize: number
  fontSize: number
}

export interface ChartLayout {
  outer: Rect
  title?: PlacedText          // centered; render with text-anchor middle
  legend?: LegendLayout
  axisTitleX?: PlacedText     // centered under the plot
  axisTitleY?: PlacedText     // centered left of the plot, rotated -90°
  /** Remaining area for the plot, axis labels included (axes.tsx subtracts the label bands). */
  plot: Rect
}

/** Resolve the legend 'auto' mode: pies and multi-series charts show it. */
export function legendVisible(spec: ChartSpec, data: ChartData): boolean {
  if (spec.legend.show === 'auto') return spec.kind === 'pie' || data.series.length > 1
  return spec.legend.show
}

const TITLE_GAP = 6

function layoutLegend(
  rest: Rect, position: LegendPosition, data: ChartData, measure: TextMeasurer,
): LegendLayout {
  const fs = FONT_LEGEND
  const symbol = fs * 0.85
  const padX = Math.max(6, fs * 0.5)
  const padY = Math.max(4, fs * 0.35)
  const gapSymText = Math.max(4, fs * 0.3)
  const labels = data.series.map(s => s.name)
  const colors = data.series.map(s => s.color)
  const lineH = Math.max(symbol, measure('Ag', fs).h) + 4

  const vertical = position === 'left' || position === 'right'
  const maxTextW = Math.max(20, rest.w * LEGEND_MAX_WIDTH_RATIO - symbol - gapSymText - 2 * padX)
  const entryW = (label: string) => Math.min(measure(label, fs).w, maxTextW) + symbol + gapSymText

  const entries: LegendEntry[] = []
  let rect: Rect
  if (vertical) {
    const w = Math.max(0, ...labels.map(entryW)) + 2 * padX
    const h = Math.min(rest.h, labels.length * lineH + 2 * padY)
    const x = position === 'right' ? rest.x + rest.w - w : rest.x
    const y = rest.y + Math.max(0, (rest.h - h) / 2)   // vertically centered (anchor 0.5)
    rect = { x, y, w, h }
    labels.forEach((label, i) => entries.push({ label, color: colors[i], x: padX, y: padY + i * lineH }))
  } else {
    // Horizontal: flow entries into rows within the available width.
    const availW = rest.w
    let cx = 0, cy = padY, rowMax = 0
    const pos: { x: number; y: number }[] = []
    labels.forEach(label => {
      const w = entryW(label) + padX
      if (cx > 0 && cx + w > availW) { cx = 0; cy += lineH }
      pos.push({ x: cx + padX, y: cy })
      cx += w
      rowMax = Math.max(rowMax, cx)
    })
    const h = cy + lineH + padY
    const w = Math.min(availW, rowMax + padX)
    const x = rest.x + Math.max(0, (rest.w - w) / 2)
    const y = position === 'top' ? rest.y : rest.y + rest.h - h
    rect = { x, y, w, h }
    labels.forEach((label, i) => entries.push({ label, color: colors[i], x: pos[i].x, y: pos[i].y }))
  }
  return { rect, entries, symbolSize: symbol, fontSize: fs }
}

/**
 * Compute the full layout for a chart box of `width` × `height` px.
 * Subtraction order (chart2 ChartView): title → legend → axis titles →
 * 2% page margin → plot.
 */
export function computeLayout(
  width: number, height: number, spec: ChartSpec, data: ChartData,
  measure: TextMeasurer = approxMeasure,
): ChartLayout {
  const outer: Rect = { x: 0, y: 0, w: width, h: height }
  let rest: Rect = { ...outer }
  const layout: ChartLayout = { outer, plot: rest }

  if (spec.title) {
    const m = measure(spec.title, FONT_TITLE)
    layout.title = { text: spec.title, x: rest.x + rest.w / 2, y: rest.y + TITLE_GAP + m.h * 0.75, fontSize: FONT_TITLE }
    const used = m.h + 2 * TITLE_GAP
    rest = { ...rest, y: rest.y + used, h: Math.max(0, rest.h - used) }
  }

  if (legendVisible(spec, data) && data.series.length > 0) {
    const legend = layoutLegend(rest, spec.legend.position, data, measure)
    const vertical = spec.legend.position === 'left' || spec.legend.position === 'right'
    // On small boxes an auto legend would crush the plot: drop it when the
    // remaining plot space would fall under a usable minimum.
    const tooBig = vertical ? rest.w - legend.rect.w < 90 : rest.h - legend.rect.h < 60
    if (!(spec.legend.show === 'auto' && tooBig)) {
      layout.legend = legend
      const gap = 4
      switch (spec.legend.position) {
        case 'right':  rest = { ...rest, w: Math.max(0, legend.rect.x - gap - rest.x) }; break
        case 'left':   rest = { ...rest, x: legend.rect.x + legend.rect.w + gap, w: Math.max(0, rest.w - legend.rect.w - gap) }; break
        case 'top':    rest = { ...rest, y: legend.rect.y + legend.rect.h + gap, h: Math.max(0, rest.h - legend.rect.h - gap) }; break
        case 'bottom': rest = { ...rest, h: Math.max(0, legend.rect.y - gap - rest.y) }; break
      }
    }
  }

  if (isCartesian(spec.kind)) {
    if (spec.axisX.title) {
      const m = measure(spec.axisX.title, FONT_AXIS_TITLE)
      layout.axisTitleX = { text: spec.axisX.title, x: rest.x + rest.w / 2, y: rest.y + rest.h - m.h * 0.25, fontSize: FONT_AXIS_TITLE }
      rest = { ...rest, h: Math.max(0, rest.h - m.h - 2) }
    }
    if (spec.axisY.title) {
      const m = measure(spec.axisY.title, FONT_AXIS_TITLE)
      layout.axisTitleY = { text: spec.axisY.title, x: rest.x + m.h * 0.8, y: rest.y + rest.h / 2, fontSize: FONT_AXIS_TITLE }
      rest = { ...rest, x: rest.x + m.h + 2, w: Math.max(0, rest.w - m.h - 2) }
    }
  }

  // chart2 page margin: 2% of the box on each side of the remaining area.
  const mx = width * PAGE_MARGIN_RATIO, my = height * PAGE_MARGIN_RATIO
  layout.plot = {
    x: rest.x + mx, y: rest.y + my,
    w: Math.max(0, rest.w - 2 * mx), h: Math.max(0, rest.h - 2 * my),
  }
  return layout
}
