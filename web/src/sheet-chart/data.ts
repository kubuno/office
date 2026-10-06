// ChartSpec + cell accessor → resolved, render-ready data.
//
// This module never touches the sheet model directly: the host provides a
// `CellValueGetter` (in SpreadsheetApp it wraps `numericValue`/`resolveValue`,
// so formulas are evaluated live and charts recompute with the grid).
// Refs are bare A1 refs on the active sheet (sheet-qualified prefixes were
// already stripped by the xlsx importer).

import { refBounds } from '../formula-refs'
import {
  ChartSpec, SeriesSpec, RangeSource, seriesColor, MAX_CHART_POINTS,
} from './model'

/** Raw cell value as surfaced by the host (formula results included). */
export type CellValue = number | string | boolean | null | undefined
export type CellValueGetter = (ref: string) => CellValue

export interface ResolvedSeries {
  name: string
  color: string
  /** Concrete drawing kind for this series (combo splits column/line). */
  kind: 'column' | 'bar' | 'line' | 'area' | 'pie' | 'radar' | 'scatter'
  axis: 'primary' | 'secondary'
  /** Y values, index-aligned with `categories` (null = missing/non-numeric). */
  values: (number | null)[]
  /** X values (scatter only); undefined → use the point index. */
  x?: (number | null)[]
}

export interface ChartData {
  series: ResolvedSeries[]
  /** Category labels; may be empty (renderers then use 1..n). */
  categories: string[]
  /** Explicit per-point colors passed through from the spec (pie slices). */
  pointColors?: string[]
}

export interface ResolveOptions {
  /** i18n fallback name for unnamed series (default: "Series N"). */
  seriesName?: (index: number) => string
}

// ── A1 helpers ───────────────────────────────────────────────────────────────

/** 0-indexed column → letters ("A", "AA"…). */
export function colLetter(c: number): string {
  let s = ''
  for (let n = c + 1; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(64 + ((n - 1) % 26) + 1) + s
  return s
}

/** 0-indexed col + 1-indexed row → "B3". */
export function cellRef(c: number, r: number): string { return colLetter(c) + r }

/**
 * Expand a rectangular range into a grid of refs, `grid[row][col]`,
 * capped at MAX_CHART_POINTS cells. Returns null when unparseable.
 */
export function expandRange(range: string): string[][] | null {
  const b = refBounds(range)
  if (!b) return null
  const rows = Math.min(b.r2 - b.r1 + 1, MAX_CHART_POINTS)
  const cols = Math.min(b.c2 - b.c1 + 1, 256)
  if (rows * cols > MAX_CHART_POINTS) return null
  const grid: string[][] = []
  for (let r = 0; r < rows; r++) {
    const row: string[] = []
    for (let c = 0; c < cols; c++) row.push(cellRef(b.c1 + c, b.r1 + r))
    grid.push(row)
  }
  return grid
}

// ── Value coercion ───────────────────────────────────────────────────────────

function toNumber(v: CellValue): number | null {
  if (typeof v === 'number') return Number.isFinite(v) ? v : null
  if (typeof v === 'boolean') return v ? 1 : 0
  if (typeof v === 'string' && v.trim() !== '') {
    const n = Number(v.replace(',', '.'))
    return Number.isFinite(n) ? n : null
  }
  return null
}

function toLabel(v: CellValue): string {
  if (v === null || v === undefined) return ''
  return String(v)
}

// ── Range layout auto-detection (Calc heuristics) ────────────────────────────

export interface RangeLayout { seriesIn: 'cols' | 'rows'; firstRowHeader: boolean; firstColHeader: boolean }

/**
 * Auto-detect orientation and header flags for a simple-range source, per
 * sc/source/ui/drawfunc/fuins2.cxx + ScChartPositioner::CheckColRowHeaders
 * (chartpos.cxx — the routine chart insertion actually uses, NOT the stricter
 * ScTable::HasColHeader):
 *  - orientation: columns by default; a single-row range → rows;
 *  - header row/column: contains NO numeric (value) cell — empty and text
 *    cells both qualify, so the classic empty top-left corner is fine.
 * Explicit flags on the source always win.
 */
export function detectRangeLayout(source: RangeSource, getCell: CellValueGetter): RangeLayout | null {
  const grid = expandRange(source.range)
  if (!grid || grid.length === 0 || grid[0].length === 0) return null
  const rows = grid.length, cols = grid[0].length

  const hasColHeader = (): boolean => {
    if (rows < 2) return false
    for (let c = 0; c < cols; c++) if (toNumber(getCell(grid[0][c])) !== null) return false
    return true
  }
  const hasRowHeader = (): boolean => {
    if (cols < 2) return false
    for (let r = 0; r < rows; r++) if (toNumber(getCell(grid[r][0])) !== null) return false
    return true
  }

  const seriesIn = source.seriesIn ?? (rows === 1 ? 'rows' : 'cols')
  return {
    seriesIn,
    firstRowHeader: source.firstRowHeader ?? hasColHeader(),
    firstColHeader: source.firstColHeader ?? hasRowHeader(),
  }
}

// ── Resolution ───────────────────────────────────────────────────────────────

function defaultName(i: number, opts?: ResolveOptions): string {
  return opts?.seriesName ? opts.seriesName(i) : `Series ${i + 1}`
}

/** Drawing kind for series `i` of `n` given the chart kind. */
function seriesKind(spec: ChartSpec, s: SeriesSpec | undefined, i: number, n: number): ResolvedSeries['kind'] {
  if (spec.kind === 'combo') {
    if (s?.kind) return s.kind
    // chart2 NumberOfLines: last N series drawn as lines, at least one column kept.
    const lines = Math.min(spec.numLines, Math.max(n - 1, 0))
    return i >= n - lines ? 'line' : 'column'
  }
  return spec.kind
}

function readRefs(refs: string[], getCell: CellValueGetter): (number | null)[] {
  return refs.slice(0, MAX_CHART_POINTS).map(r => toNumber(getCell(r)))
}

function readRangeColumn(range: string, getCell: CellValueGetter): (number | null)[] {
  const grid = expandRange(range)
  if (!grid) return []
  // Accept a single column, a single row, or flatten row-major otherwise.
  const flat: string[] = grid.length === 1 ? grid[0] : grid[0].length === 1 ? grid.map(r => r[0]) : grid.flat()
  return readRefs(flat, getCell)
}

function readLabelRange(range: string, getCell: CellValueGetter): string[] {
  const grid = expandRange(range)
  if (!grid) return []
  const flat: string[] = grid.length === 1 ? grid[0] : grid.map(r => r[0])
  return flat.slice(0, MAX_CHART_POINTS).map(r => toLabel(getCell(r)))
}

/** Resolve a simple-range source into series + categories. */
function resolveRange(spec: ChartSpec, source: RangeSource, getCell: CellValueGetter, opts?: ResolveOptions): ChartData {
  const layout = detectRangeLayout(source, getCell)
  const grid0 = expandRange(source.range)
  if (!layout || !grid0) return { series: [], categories: [] }
  // Work in "series in columns" space: transpose when series are in rows.
  const grid = layout.seriesIn === 'rows' ? grid0[0].map((_, c) => grid0.map(row => row[c])) : grid0
  const nameHeader = layout.seriesIn === 'cols' ? layout.firstRowHeader : layout.firstColHeader
  const catHeader = layout.seriesIn === 'cols' ? layout.firstColHeader : layout.firstRowHeader

  const dataRows = nameHeader ? grid.slice(1) : grid
  if (dataRows.length === 0) return { series: [], categories: [] }
  const nCols = grid[0].length

  const categories: string[] = catHeader
    ? dataRows.map(row => toLabel(getCell(row[0])))
    : []
  const firstValCol = catHeader ? 1 : 0

  // Scatter: the first value column provides shared X values.
  const isScatter = spec.kind === 'scatter'
  const xCol = isScatter && nCols - firstValCol >= 2 ? firstValCol : -1
  const x = xCol >= 0 ? dataRows.map(row => toNumber(getCell(row[xCol]))) : undefined

  const series: ResolvedSeries[] = []
  for (let c = firstValCol; c < nCols; c++) {
    if (c === xCol) continue
    const i = series.length
    const name = nameHeader ? toLabel(getCell(grid[0][c])) : ''
    series.push({
      name: name || defaultName(i, opts),
      color: seriesColor(i),
      kind: seriesKind(spec, undefined, i, Math.max(nCols - firstValCol - (xCol >= 0 ? 1 : 0), 1)),
      axis: 'primary',
      values: dataRows.map(row => toNumber(getCell(row[c]))),
      ...(x ? { x } : {}),
    })
  }
  return { series, categories, pointColors: spec.pointColors }
}

/**
 * Resolve the chart data for a spec. Pure with respect to the spec; all sheet
 * access goes through `getCell`. Cheap enough to run per render, but hosts
 * should memoize on (spec, sheet revision).
 */
export function resolveChartData(spec: ChartSpec, getCell: CellValueGetter, opts?: ResolveOptions): ChartData {
  if (spec.source) return resolveRange(spec, spec.source, getCell, opts)

  const n = spec.series.length
  const series: ResolvedSeries[] = spec.series.map((s, i) => {
    const values = s.valsRange ? readRangeColumn(s.valsRange, getCell) : s.vals ? readRefs(s.vals, getCell) : []
    let x: (number | null)[] | undefined
    if (spec.kind === 'scatter') {
      if (s.xRange) x = readRangeColumn(s.xRange, getCell)
      else if (s.catRefs) x = readRefs(s.catRefs, getCell)
    }
    const liveName = s.nameRef ? toLabel(getCell(s.nameRef)) : ''
    return {
      name: liveName || s.name || defaultName(i, opts),
      color: seriesColor(i, s.color),
      kind: seriesKind(spec, s, i, n),
      axis: s.axis ?? 'primary',
      values,
      ...(x ? { x } : {}),
    }
  })

  let categories: string[] = []
  if (spec.catsRange) categories = readLabelRange(spec.catsRange, getCell)
  else {
    const catRefs = spec.catRefs ?? spec.series.find(s => s.catRefs)?.catRefs
    if (catRefs && spec.kind !== 'scatter') categories = catRefs.slice(0, MAX_CHART_POINTS).map(r => toLabel(getCell(r)))
  }

  return { series, categories, pointColors: spec.pointColors }
}
