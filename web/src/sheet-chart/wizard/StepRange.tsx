// Wizard step 2 — data range (Calc tp_RangeChooser): range field, series in
// rows/columns, first row/column as labels. Defaults are auto-detected with
// the Calc heuristics (detectRangeLayout); explicit user picks are persisted
// on the draft and always win.

import { Checkbox } from '@ui'
import type { SheetChart } from '../../api'
import { detectRangeLayout, CellValueGetter } from '../data'
import { Field, Seg, WIZ_INPUT } from './controls'
import type { WizardTranslate } from './state'

export function StepRange({ draft, onChange, getCell, onPickRange, t }: {
  draft: SheetChart
  onChange: (c: SheetChart) => void
  getCell: CellValueGetter
  /** Optional host hook: let the user pick the range on the grid. */
  onPickRange?: (cb: (range: string) => void) => void
  t: WizardTranslate
}) {
  const hasExplicitSeries = !draft.range && ((draft.series?.length ?? 0) > 0 || (draft.vals?.length ?? 0) > 0)
  // Pure auto-detection (no draft overrides) provides the displayed defaults.
  const detected = draft.range ? detectRangeLayout({ range: draft.range }, getCell) : null

  // Editing the range (re)binds the chart to a simple-range source: explicit
  // series and flat import refs are dropped, like re-choosing data in Calc.
  const setRange = (range: string) => onChange({
    ...draft, range: range || undefined,
    series: undefined, vals: undefined, cats: undefined, catsRange: undefined,
  })

  const seriesIn = draft.seriesIn ?? detected?.seriesIn ?? 'cols'
  const firstRow = draft.firstRowHeader ?? detected?.firstRowHeader ?? false
  const firstCol = draft.firstColHeader ?? detected?.firstColHeader ?? false

  return (
    <div className="p-3 space-y-3">
      <Field label={t('sheet_chart_step_range', { defaultValue: 'Plage de données' })}>
        <div className="flex gap-1.5">
          <input value={draft.range ?? ''} onChange={e => setRange(e.target.value.trim())}
            onKeyDown={e => e.stopPropagation()} placeholder="A1:D8" spellCheck={false}
            className={`${WIZ_INPUT} font-mono`} />
          {onPickRange && (
            <button type="button" onClick={() => onPickRange(setRange)}
              title={t('sheet_chart_pick_range', { defaultValue: 'Sélectionner la plage sur la grille' })}
              className="h-8 px-2 border border-[#dadce0] rounded hover:bg-[#f1f3f4] text-sm shrink-0">⛶</button>
          )}
        </div>
      </Field>
      {hasExplicitSeries && (
        <div className="text-xs text-text-tertiary">
          {t('sheet_chart_explicit_hint', { defaultValue: 'Ce diagramme utilise des séries explicites (voir l’étape Séries de données). Saisir une plage ici les remplacera.' })}
        </div>
      )}
      <Field label={t('sheet_chart_series_in', { defaultValue: 'Séries' })}>
        <Seg
          options={[
            { value: 'cols', label: t('sheet_chart_series_cols', { defaultValue: 'En colonnes' }) },
            { value: 'rows', label: t('sheet_chart_series_rows', { defaultValue: 'En lignes' }) },
          ]}
          value={seriesIn}
          onChange={v => onChange({ ...draft, seriesIn: v })}
          disabled={!draft.range}
        />
      </Field>
      {/* Aligned under the field column (label width 9rem + gap 0.5rem). */}
      <div className="space-y-2" style={{ paddingLeft: 'calc(9rem + 0.5rem)' }}>
        <Checkbox checked={firstRow} disabled={!draft.range}
          onChange={v => onChange({ ...draft, firstRowHeader: v })}
          label={t('sheet_chart_first_row', { defaultValue: 'Première ligne comme étiquette' })} />
        <Checkbox checked={firstCol} disabled={!draft.range}
          onChange={v => onChange({ ...draft, firstColHeader: v })}
          label={t('sheet_chart_first_col', { defaultValue: 'Première colonne comme étiquette' })} />
      </div>
    </div>
  )
}
