// Field-well definitions per visual + aggregation helpers. The "wells" describe
// where each dragged/clicked field lands in a visual's config (axis, legend,
// values, size…) — mirroring Power BI's field buckets.

export type WellId = 'axis' | 'legend' | 'values' | 'size' | 'tooltips' | 'category'

export interface Well { id: WellId; label: string; multi: boolean; kind: 'dimension' | 'metric' }

const AXIS: Well    = { id: 'axis',     label: 'Axe',          multi: false, kind: 'dimension' }
const CATEGORY: Well= { id: 'category', label: 'Catégorie',    multi: false, kind: 'dimension' }
const LEGEND: Well  = { id: 'legend',   label: 'Légende',      multi: false, kind: 'dimension' }
const VALUES: Well  = { id: 'values',   label: 'Valeurs',      multi: true,  kind: 'metric' }
const VALUE1: Well  = { id: 'values',   label: 'Valeur',       multi: false, kind: 'metric' }
const SIZE: Well    = { id: 'size',     label: 'Taille',       multi: false, kind: 'metric' }

export function wellsFor(type: string): Well[] {
  switch (type) {
    case 'card':
    case 'gauge':
    case 'bullet':
    case 'progress_ring':
      return [VALUE1]
    case 'kpi_card':
    case 'scorecard':
      return [VALUE1, { id: 'tooltips', label: 'Objectif / comparaison', multi: false, kind: 'metric' }]
    case 'multi_row_card':
    case 'kpi_grid':
    case 'funnel':
    case 'pie_chart':
    case 'donut_chart':
    case 'treemap':
      return [CATEGORY, VALUE1]
    case 'bar_chart':
    case 'column_h':
    case 'line_chart':
    case 'smooth_line':
    case 'step_line':
    case 'area_chart':
    case 'waterfall':
    case 'sparkline':
      return [AXIS, LEGEND, VALUES]
    case 'stacked_bar':
    case 'stacked_bar_100':
    case 'stacked_area':
    case 'ribbon_chart':
    case 'heatmap':
    case 'matrix':
      return [AXIS, LEGEND, VALUE1]
    case 'combo_chart':
      return [AXIS, { id: 'values', label: 'Colonnes puis courbe', multi: true, kind: 'metric' }]
    case 'scatter_chart':
      return [{ id: 'axis', label: 'X', multi: false, kind: 'metric' }, { id: 'values', label: 'Y', multi: false, kind: 'metric' }]
    case 'bubble_chart':
      return [{ id: 'axis', label: 'X', multi: false, kind: 'metric' }, { id: 'values', label: 'Y', multi: false, kind: 'metric' }, SIZE]
    case 'histogram':
    case 'box_plot':
      return [CATEGORY, VALUE1]
    case 'radar_chart':
      return [CATEGORY, VALUES]
    case 'data_table':
      return [{ id: 'axis', label: 'Colonnes', multi: true, kind: 'dimension' }, VALUES]
    case 'slicer':
    case 'slicer_dropdown':
    case 'slicer_range':
      return [{ id: 'axis', label: 'Champ', multi: false, kind: 'dimension' }]
    default:
      return [AXIS, VALUES]
  }
}

export const AGG_FUNCTIONS = [
  { value: 'SUM',            label: 'Somme' },
  { value: 'AVG',            label: 'Moyenne' },
  { value: 'COUNT',          label: 'Nombre' },
  { value: 'COUNT_DISTINCT', label: 'Nombre distinct' },
  { value: 'MIN',            label: 'Minimum' },
  { value: 'MAX',            label: 'Maximum' },
]

export const FILTER_OPERATORS = [
  { value: 'eq',          label: 'est égal à' },
  { value: 'neq',         label: 'est différent de' },
  { value: 'gt',          label: 'supérieur à' },
  { value: 'gte',         label: 'supérieur ou égal' },
  { value: 'lt',          label: 'inférieur à' },
  { value: 'lte',         label: 'inférieur ou égal' },
  { value: 'like',        label: 'contient' },
  { value: 'in',          label: 'fait partie de' },
  { value: 'is_null',     label: 'est vide' },
  { value: 'is_not_null', label: "n'est pas vide" },
]
