// Chart wizard — pure draft helpers (no React).
//
// The wizard edits a working copy of the persisted `SheetChart`; these
// helpers convert between its data shapes: simple range (step 2) and
// explicit series (step 3, LibreOffice tp_DataSource style).

import type { SheetChart, SheetChartSeries } from '../../api'
import { refBounds } from '../../formula-refs'
import { cellRef, detectRangeLayout, CellValueGetter } from '../data'

export type WizardTranslate = (key: string, opts?: { defaultValue?: string; [k: string]: unknown }) => string

/** Explicit-series view of a draft (derived when the draft is range-based). */
export interface ExplicitData {
  series: SheetChartSeries[]
  catsRange?: string
  /** True when this was derived from `range` (an edit converts the draft). */
  derived: boolean
}

/**
 * Decompose a simple-range chart into explicit series (nameRef + valsRange per
 * series, plus a categories range), honouring the Calc layout heuristics.
 */
export function seriesFromRange(chart: SheetChart, getCell: CellValueGetter): ExplicitData {
  const empty: ExplicitData = { series: [], derived: true }
  if (!chart.range) return empty
  const layout = detectRangeLayout({
    range: chart.range, seriesIn: chart.seriesIn,
    firstRowHeader: chart.firstRowHeader, firstColHeader: chart.firstColHeader,
  }, getCell)
  const b = refBounds(chart.range)
  if (!layout || !b) return empty

  const series: SheetChartSeries[] = []
  let catsRange: string | undefined
  if (layout.seriesIn === 'cols') {
    const r1 = b.r1 + (layout.firstRowHeader ? 1 : 0)
    if (r1 > b.r2) return empty
    const c1 = b.c1 + (layout.firstColHeader ? 1 : 0)
    if (layout.firstColHeader) catsRange = `${cellRef(b.c1, r1)}:${cellRef(b.c1, b.r2)}`
    for (let c = c1; c <= b.c2; c++) {
      series.push({
        ...(layout.firstRowHeader ? { nameRef: cellRef(c, b.r1) } : {}),
        valsRange: `${cellRef(c, r1)}:${cellRef(c, b.r2)}`,
      })
    }
  } else {
    const c1 = b.c1 + (layout.firstColHeader ? 1 : 0)
    if (c1 > b.c2) return empty
    const r1 = b.r1 + (layout.firstRowHeader ? 1 : 0)
    if (layout.firstRowHeader) catsRange = `${cellRef(c1, b.r1)}:${cellRef(b.c2, b.r1)}`
    for (let r = r1; r <= b.r2; r++) {
      series.push({
        ...(layout.firstColHeader ? { nameRef: cellRef(b.c1, r) } : {}),
        valsRange: `${cellRef(c1, r)}:${cellRef(b.c2, r)}`,
      })
    }
  }
  return { series, catsRange, derived: true }
}

/** Explicit-series view of any draft (existing series, flat refs, or range). */
export function explicitData(chart: SheetChart, getCell: CellValueGetter): ExplicitData {
  if (chart.series && chart.series.length > 0) {
    return { series: chart.series, catsRange: chart.catsRange, derived: false }
  }
  if (chart.vals && chart.vals.length > 0) {
    return { series: [{ vals: chart.vals, ...(chart.cats ? { cats: chart.cats } : {}) }], catsRange: chart.catsRange, derived: false }
  }
  return seriesFromRange(chart, getCell)
}

/**
 * Write an explicit series list back onto the draft. The simple-range fields
 * are cleared (series now own the data), everything else is spread-preserved.
 */
export function withExplicitSeries(chart: SheetChart, series: SheetChartSeries[], catsRange: string | undefined): SheetChart {
  return {
    ...chart, series, catsRange,
    range: undefined, seriesIn: undefined, firstRowHeader: undefined, firstColHeader: undefined,
    vals: undefined, cats: undefined,
  }
}

/** Display name of a series row in the wizard list. */
export function seriesDisplayName(s: SheetChartSeries, i: number, getCell: CellValueGetter, t: WizardTranslate): string {
  if (s.name) return s.name
  if (s.nameRef) {
    const v = getCell(s.nameRef)
    if (v !== null && v !== undefined && String(v) !== '') return String(v)
  }
  return t('sheet_chart_series_n', { defaultValue: 'Série {{n}}', n: i + 1 })
}

/** Move item `i` of `arr` by `delta` (returns a new array; no-op at bounds). */
export function moveItem<T>(arr: T[], i: number, delta: number): T[] {
  const j = i + delta
  if (j < 0 || j >= arr.length) return arr
  const next = arr.slice()
  const [it] = next.splice(i, 1)
  next.splice(j, 0, it)
  return next
}
