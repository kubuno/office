// Wizard step 3 — data series (Calc tp_DataSource): ordered series list with
// add/remove/up/down, per-series name / value range / color, and the global
// categories range. A range-based draft is shown as its derived series; the
// first edit converts the draft to explicit series (withExplicitSeries).

import { useState } from 'react'
import { ChevronDown, ChevronUp, Plus, Trash2 } from 'lucide-react'
import { ColorField } from '@ui'
import type { SheetChart, SheetChartSeries } from '../../api'
import { CellValueGetter } from '../data'
import { seriesColor } from '../model'
import { explicitData, moveItem, seriesDisplayName, withExplicitSeries, WizardTranslate } from './state'
import { Field, WIZ_INPUT } from './controls'

export function StepSeries({ draft, onChange, getCell, t }: {
  draft: SheetChart
  onChange: (c: SheetChart) => void
  getCell: CellValueGetter
  t: WizardTranslate
}) {
  const ed = explicitData(draft, getCell)
  const [sel, setSel] = useState(0)
  const series = ed.series
  const selIdx = Math.min(sel, Math.max(series.length - 1, 0))
  const s: SheetChartSeries | undefined = series[selIdx]

  const commit = (next: SheetChartSeries[], catsRange = ed.catsRange) =>
    onChange(withExplicitSeries(draft, next, catsRange))
  const patchSel = (patch: Partial<SheetChartSeries>) => {
    if (!s) return
    commit(series.map((x, i) => i === selIdx ? { ...x, ...patch } : x))
  }

  const add = () => { commit([...series, { valsRange: '' }]); setSel(series.length) }
  const remove = () => {
    if (!s) return
    commit(series.filter((_, i) => i !== selIdx))
    setSel(Math.max(selIdx - 1, 0))
  }
  const move = (delta: number) => {
    const next = moveItem(series, selIdx, delta)
    if (next !== series) { commit(next); setSel(selIdx + delta) }
  }

  const isScatter = draft.type === 'scatter'
  const flatValsHint = s?.vals && !s.valsRange
    ? t('sheet_chart_flat_refs', { defaultValue: '{{n}} références importées', n: s.vals.length })
    : undefined
  const flatCatsHint = !ed.catsRange && (series[0]?.cats?.length ?? 0) > 0
    ? t('sheet_chart_flat_refs', { defaultValue: '{{n}} références importées', n: series[0].cats?.length ?? 0 })
    : undefined

  return (
    <div className="flex h-full min-h-0">
      <div className="w-44 shrink-0 border-r border-border flex flex-col min-h-0">
        <div className="px-3 py-1.5 text-[10px] uppercase tracking-wide text-text-tertiary">{t('sheet_chart_step_series', { defaultValue: 'Séries de données' })}</div>
        <div className="flex-1 overflow-auto">
          {series.map((x, i) => (
            <button key={i} type="button" onClick={() => setSel(i)}
              className={`w-full text-left px-3 py-1.5 flex items-center gap-2 text-xs ${i === selIdx ? 'bg-[#e8f0fe] text-primary' : 'hover:bg-[#f1f3f4]'}`}>
              <span className="w-2.5 h-2.5 rounded-sm shrink-0" style={{ background: seriesColor(i, x.color) }} />
              <span className="truncate">{seriesDisplayName(x, i, getCell, t)}</span>
            </button>
          ))}
          {series.length === 0 && (
            <div className="px-3 py-2 text-xs text-text-tertiary">{t('sheet_chart_no_series', { defaultValue: 'Aucune série' })}</div>
          )}
        </div>
        <div className="flex items-center gap-0.5 border-t border-border p-1">
          <button type="button" onClick={add} title={t('sheet_chart_add_series', { defaultValue: 'Ajouter une série' })} className="w-7 h-7 flex items-center justify-center rounded hover:bg-[#f1f3f4]"><Plus size={14} /></button>
          <button type="button" onClick={remove} disabled={!s} title={t('common_delete', { defaultValue: 'Supprimer' })} className="w-7 h-7 flex items-center justify-center rounded hover:bg-[#f1f3f4] disabled:opacity-40"><Trash2 size={14} /></button>
          <button type="button" onClick={() => move(-1)} disabled={selIdx <= 0} title={t('sheet_chart_move_up', { defaultValue: 'Monter' })} className="w-7 h-7 flex items-center justify-center rounded hover:bg-[#f1f3f4] disabled:opacity-40"><ChevronUp size={14} /></button>
          <button type="button" onClick={() => move(1)} disabled={selIdx >= series.length - 1} title={t('sheet_chart_move_down', { defaultValue: 'Descendre' })} className="w-7 h-7 flex items-center justify-center rounded hover:bg-[#f1f3f4] disabled:opacity-40"><ChevronDown size={14} /></button>
        </div>
      </div>
      <div className="flex-1 min-w-0 p-3 space-y-3 overflow-auto">
        {s ? (<>
          <Field label={t('sheet_chart_series_name', { defaultValue: 'Nom' })}>
            <input value={s.name ?? ''} onChange={e => patchSel({ name: e.target.value || undefined, nameRef: e.target.value ? undefined : s.nameRef })}
              onKeyDown={e => e.stopPropagation()} spellCheck={false}
              placeholder={s.nameRef ? String(getCell(s.nameRef) ?? '') : t('sheet_chart_series_n', { defaultValue: 'Série {{n}}', n: selIdx + 1 })}
              className={WIZ_INPUT} />
          </Field>
          <Field label={t('sheet_chart_y_values', { defaultValue: 'Valeurs Y' })}>
            <input value={s.valsRange ?? ''} onChange={e => patchSel({ valsRange: e.target.value.trim() || undefined, vals: undefined })}
              onKeyDown={e => e.stopPropagation()} spellCheck={false}
              placeholder={flatValsHint ?? 'B2:B10'} className={`${WIZ_INPUT} font-mono`} />
          </Field>
          {isScatter && (
            <Field label={t('sheet_chart_x_values', { defaultValue: 'Valeurs X' })}>
              <input value={s.xRange ?? ''} onChange={e => patchSel({ xRange: e.target.value.trim() || undefined })}
                onKeyDown={e => e.stopPropagation()} spellCheck={false}
                placeholder="A2:A10" className={`${WIZ_INPUT} font-mono`} />
            </Field>
          )}
          <Field label={t('sheet_chart_series_color', { defaultValue: 'Couleur' })}>
            <ColorField color={s.color ?? seriesColor(selIdx)} onChange={hex => patchSel({ color: hex })} width={120} height={28} />
          </Field>
        </>) : (
          <div className="text-sm text-text-tertiary">{t('sheet_chart_no_series', { defaultValue: 'Aucune série' })}</div>
        )}
        <div className="border-t border-border pt-3">
          <Field label={t('sheet_chart_categories', { defaultValue: 'Catégories' })}>
            <input value={ed.catsRange ?? ''} onChange={e => commit(series, e.target.value.trim() || undefined)}
              onKeyDown={e => e.stopPropagation()} spellCheck={false}
              placeholder={flatCatsHint ?? 'A2:A10'} className={`${WIZ_INPUT} font-mono`} />
          </Field>
        </div>
      </div>
    </div>
  )
}
