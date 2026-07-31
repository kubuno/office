// Chart wizard step 1 — the type / subtype matrix, mirroring the LibreOffice
// Calc chart-type dialog (tp_ChartType.cxx order): Column, Bar, Pie, Area,
// Line, XY (scatter), Net (radar), Column & Line (combo).
//
// A subtype is a plain patch spread onto the draft `SheetChart`; `undefined`
// values clear stale flags (they are dropped by JSON at persist time). The
// wizard always spreads (`{ ...draft, ...patch }`) so unknown/legacy fields
// survive untouched.

import type { SheetChart } from '../../api'

export type CatalogTypeId = 'column' | 'bar' | 'pie' | 'area' | 'line' | 'scatter' | 'radar' | 'combo'

export interface ChartSubtypeDef {
  id: string
  labelKey: string
  labelDefault: string
  apply: Partial<SheetChart>
  match: (c: SheetChart) => boolean
}

export interface ChartTypeDef {
  id: CatalogTypeId
  labelKey: string
  labelDefault: string
  /** Show the "smooth lines" option (line / scatter). */
  smoothOption?: boolean
  subtypes: ChartSubtypeDef[]
}

// Cross-type flags reset when the user picks a type/subtype; each subtype
// re-adds what it needs. `smooth` is intentionally preserved (line option).
const CLEAR: Partial<SheetChart> = {
  grouping: undefined, lines: undefined, symbols: undefined,
  holeSize: undefined, explode: undefined, filled: undefined, numLines: undefined,
}

const groupingOf = (c: SheetChart) => c.grouping === 'stacked' ? 'stacked' : c.grouping === 'percentStacked' ? 'percent' : 'normal'

/** Normal / Stacked / Percent triplet for a cartesian persisted type. */
function stackTriplet(type: SheetChart['type']): ChartSubtypeDef[] {
  return [
    { id: 'normal', labelKey: 'sheet_chart_st_normal', labelDefault: 'Normal',
      apply: { type }, match: c => groupingOf(c) === 'normal' },
    { id: 'stacked', labelKey: 'sheet_chart_st_stacked', labelDefault: 'Empilé',
      apply: { type, grouping: 'stacked' }, match: c => groupingOf(c) === 'stacked' },
    { id: 'percent', labelKey: 'sheet_chart_st_percent', labelDefault: 'Pourcentage',
      apply: { type, grouping: 'percentStacked' }, match: c => groupingOf(c) === 'percent' },
  ]
}

/** Points / Points & lines / Lines triplet (Calc order) for line/scatter. */
function symbolTriplet(type: SheetChart['type'], def: { lines: boolean; symbols: boolean }): ChartSubtypeDef[] {
  const lines = (c: SheetChart) => c.lines ?? def.lines
  const symbols = (c: SheetChart) => c.symbols ?? def.symbols
  return [
    { id: 'points', labelKey: 'sheet_chart_st_points', labelDefault: 'Points seuls',
      apply: { type, lines: false, symbols: true }, match: c => symbols(c) && !lines(c) },
    { id: 'points_lines', labelKey: 'sheet_chart_st_points_lines', labelDefault: 'Points et lignes',
      apply: { type, lines: true, symbols: true }, match: c => symbols(c) && lines(c) },
    { id: 'lines', labelKey: 'sheet_chart_st_lines', labelDefault: 'Lignes seules',
      apply: { type, lines: true, symbols: false }, match: c => !symbols(c) },
  ]
}

export const CHART_CATALOG: ChartTypeDef[] = [
  { id: 'column', labelKey: 'sheet_chart_ty_column', labelDefault: 'Colonnes', subtypes: stackTriplet('bar') },
  { id: 'bar', labelKey: 'sheet_chart_ty_bar', labelDefault: 'Barres', subtypes: stackTriplet('hbar') },
  { id: 'pie', labelKey: 'sheet_chart_ty_pie', labelDefault: 'Secteurs', subtypes: [
    { id: 'normal', labelKey: 'sheet_chart_st_normal', labelDefault: 'Normal',
      apply: { type: 'pie' }, match: c => c.type === 'pie' && !((c.explode ?? 0) > 0) },
    { id: 'exploded', labelKey: 'sheet_chart_st_exploded', labelDefault: 'Éclaté',
      apply: { type: 'pie', explode: 0.5 }, match: c => c.type === 'pie' && (c.explode ?? 0) > 0 },
    { id: 'donut', labelKey: 'sheet_chart_st_donut', labelDefault: 'Tore',
      apply: { type: 'donut' }, match: c => (c.type === 'donut' || (c.holeSize ?? 0) > 0) && !((c.explode ?? 0) > 0) },
    { id: 'donut_exploded', labelKey: 'sheet_chart_st_donut_exploded', labelDefault: 'Tore éclaté',
      apply: { type: 'donut', explode: 0.35 }, match: c => (c.type === 'donut' || (c.holeSize ?? 0) > 0) && (c.explode ?? 0) > 0 },
  ] },
  { id: 'area', labelKey: 'sheet_chart_ty_area', labelDefault: 'Zones', subtypes: stackTriplet('area') },
  { id: 'line', labelKey: 'sheet_chart_ty_line', labelDefault: 'Lignes', smoothOption: true,
    subtypes: symbolTriplet('line', { lines: true, symbols: false }) },
  { id: 'scatter', labelKey: 'sheet_chart_ty_scatter', labelDefault: 'XY (dispersion)', smoothOption: true,
    subtypes: symbolTriplet('scatter', { lines: false, symbols: true }) },
  { id: 'radar', labelKey: 'sheet_chart_ty_radar', labelDefault: 'Toile', subtypes: [
    { id: 'lines', labelKey: 'sheet_chart_st_lines', labelDefault: 'Lignes seules',
      apply: { type: 'radar' }, match: c => !c.filled },
    { id: 'filled', labelKey: 'sheet_chart_st_filled', labelDefault: 'Rempli',
      apply: { type: 'radar', filled: true }, match: c => !!c.filled },
  ] },
  { id: 'combo', labelKey: 'sheet_chart_ty_combo', labelDefault: 'Colonnes et lignes', subtypes: [
    { id: 'col_line', labelKey: 'sheet_chart_st_col_line', labelDefault: 'Colonnes et lignes',
      apply: { type: 'combo' }, match: c => groupingOf(c) === 'normal' },
    { id: 'col_stacked_line', labelKey: 'sheet_chart_st_col_stacked_line', labelDefault: 'Colonnes empilées et lignes',
      apply: { type: 'combo', grouping: 'stacked' }, match: c => groupingOf(c) !== 'normal' },
  ] },
]

/** Catalog type of a persisted chart (any vintage). */
export function catalogTypeOf(c: SheetChart): ChartTypeDef {
  const id: CatalogTypeId =
    c.type === 'hbar' ? 'bar'
    : c.type === 'pie' || c.type === 'donut' ? 'pie'
    : c.type === 'area' ? 'area'
    : c.type === 'line' ? 'line'
    : c.type === 'scatter' ? 'scatter'
    : c.type === 'radar' ? 'radar'
    : c.type === 'combo' ? 'combo'
    : 'column'
  return CHART_CATALOG.find(d => d.id === id) ?? CHART_CATALOG[0]
}

/** Currently-active subtype of `c` within `def` (first match wins). */
export function subtypeOf(c: SheetChart, def: ChartTypeDef): ChartSubtypeDef {
  return def.subtypes.find(s => s.match(c)) ?? def.subtypes[0]
}

/** Apply a type/subtype pick onto a draft, spread-preserving unknown fields. */
export function applySubtype(c: SheetChart, sub: ChartSubtypeDef): SheetChart {
  return { ...c, ...CLEAR, ...sub.apply }
}
