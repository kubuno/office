// Chart TEMPLATES — the storage behind « Enregistrer comme modèle… » /
// « Appliquer un modèle » of the chart context menu.
//
// A template is the LOOK of a chart, never its data: type + subtype flags,
// palette, legend, data labels, gridlines, chart-area fill/outline and font.
// Applying one therefore leaves `range` / `series` / `vals` / `cats` /
// `catsRange` / `title` / geometry strictly untouched — the same chart, dressed
// differently.
//
// Persistence goes through `useModulePrefs('office', …)`, i.e. the user's
// backend preferences (`core.users.preferences.office`), so templates follow the
// user across browsers and devices. NEVER localStorage. PostgreSQL merges at the
// root of `preferences`, and `useModulePrefs` re-reads the whole `office` object
// before writing, so storing our key never clobbers another office setting.

import type { SheetChart } from '../api'
import { useModulePrefs } from '../userPrefs'

/**
 * The fields a template carries. Everything absent from this list is DATA or
 * identity and is preserved when a template is applied.
 */
export const CHART_TEMPLATE_STYLE_KEYS = [
  // Type + subtype flags (the wizard's step-1 matrix).
  'type', 'grouping', 'lines', 'symbols', 'smooth',
  'holeSize', 'explode', 'filled', 'numLines',
  // Palette + elements.
  'colors', 'legend', 'legendPos', 'dataLabels', 'grid',
  // Chart-area formatting.
  'fill', 'border', 'borderWidth', 'font',
] as const

export type ChartTemplateStyleKey = typeof CHART_TEMPLATE_STYLE_KEYS[number]
export type ChartTemplateStyle = Pick<SheetChart, ChartTemplateStyleKey>

export interface ChartTemplate {
  id: string
  /** User-visible name, as typed in the "save as template" prompt. */
  name: string
  /** Epoch ms, for stable ordering (most recent last). */
  savedAt: number
  style: ChartTemplateStyle
}

/** Preference key under `preferences.office`. */
export const CHART_TEMPLATES_PREF = 'sheetChartTemplates'
/** Hard cap, so a runaway save loop can never bloat the user row. */
export const CHART_TEMPLATES_MAX = 24

/** Shape of the slice of office prefs this module owns. */
export interface ChartTemplatePrefs {
  sheetChartTemplates: ChartTemplate[]
  /** Required by useModulePrefs' generic: office prefs hold other modules' keys. */
  [key: string]: unknown
}

const DEFAULT_PREFS: ChartTemplatePrefs = { sheetChartTemplates: [] }

// crypto.randomUUID() throws outside a secure context (plain HTTP on a LAN host),
// which is a supported deployment here — hence the hand-rolled id.
function templateId(): string {
  return `tpl_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`
}

/** Extract the style-only projection of a chart. */
export function chartTemplateStyle(chart: SheetChart): ChartTemplateStyle {
  const src = chart as unknown as Record<string, unknown>
  const style: Record<string, unknown> = {}
  for (const k of CHART_TEMPLATE_STYLE_KEYS) {
    const v = src[k]
    if (v !== undefined) style[k] = v
  }
  return style as ChartTemplateStyle
}

/** Snapshot the current chart's look under `name`. */
export function templateFromChart(chart: SheetChart, name: string): ChartTemplate {
  return { id: templateId(), name, savedAt: Date.now(), style: chartTemplateStyle(chart) }
}

// Cross-type flags are cleared before a template's own values land, so a stale
// `explode` / `holeSize` from a pie never survives into a bar chart (same
// contract as the wizard catalog's CLEAR).
const CLEAR_FLAGS: Partial<SheetChart> = {
  grouping: undefined, lines: undefined, symbols: undefined, smooth: undefined,
  holeSize: undefined, explode: undefined, filled: undefined, numLines: undefined,
}

/**
 * The patch that applies `tpl` to a chart. Contains style keys only — spread it
 * onto the chart (or hand it to a persisting patcher) and the data survives.
 */
export function chartTemplatePatch(tpl: ChartTemplate): Partial<SheetChart> {
  return { ...CLEAR_FLAGS, ...tpl.style }
}

/** Pure "chart dressed with this template" — data and geometry preserved. */
export function applyChartTemplate(chart: SheetChart, tpl: ChartTemplate): SheetChart {
  return { ...chart, ...chartTemplatePatch(tpl) }
}

/**
 * Read/write the user's chart templates.
 *
 * Must be called from a component (it is a hook): the spreadsheet editor holds
 * it once and passes `templates` + `save` down to `buildChartMenu`, which stays
 * a pure function.
 */
export function useChartTemplates(): {
  templates: ChartTemplate[]
  save: (name: string, chart: SheetChart) => Promise<ChartTemplate>
  remove: (id: string) => Promise<void>
} {
  const { prefs, update } = useModulePrefs<ChartTemplatePrefs>('office', DEFAULT_PREFS)
  const stored = prefs.sheetChartTemplates
  const templates = Array.isArray(stored) ? stored.filter(t => t && typeof t.id === 'string') : []

  const save = async (name: string, chart: SheetChart): Promise<ChartTemplate> => {
    const tpl = templateFromChart(chart, name)
    // Re-saving under an existing name replaces it (Excel overwrites the .crtx).
    const next = [...templates.filter(x => x.name !== tpl.name), tpl].slice(-CHART_TEMPLATES_MAX)
    await update({ sheetChartTemplates: next })
    return tpl
  }

  const remove = async (id: string): Promise<void> => {
    await update({ sheetChartTemplates: templates.filter(x => x.id !== id) })
  }

  return { templates, save, remove }
}
