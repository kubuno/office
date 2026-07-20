import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input } from '@ui'
import { Table2, Plus, X } from 'lucide-react'
import type { PivotAgg, PivotValueSpec, PivotFilterSpec } from './pivot-engine'
import { AGG_LABEL } from './pivot-engine'

export interface PivotColumn { idx: number; letter: string; header: string }
export interface PivotResult {
  rowFields: number[]
  colField:  number | null
  values:    PivotValueSpec[]
  filters:   PivotFilterSpec[]
  target:    string
}

interface Props {
  columns:       PivotColumn[]
  rangeLabel:    string
  defaultTarget: string
  /** Valeurs distinctes d'une colonne source (UI des filtres). */
  distinct:      (field: number) => string[]
  /** Configuration existante — le dialogue passe en mode « Modifier ». */
  initial?:      PivotResult
  onBuild:       (r: PivotResult) => void
  onClose:       () => void
}

export default function PivotDialog({ columns, rangeLabel, defaultTarget, distinct, initial, onBuild, onClose }: Props) {
  const { t } = useTranslation('office')
  const editMode = !!initial
  const [rowFields, setRowFields] = useState<number[]>(initial?.rowFields ?? (columns[0] ? [columns[0].idx] : []))
  const [colField, setColField]   = useState<number | null>(initial?.colField ?? null)
  const [values, setValues]       = useState<PivotValueSpec[]>(
    initial?.values?.length ? initial.values : [{ field: columns[columns.length - 1]?.idx ?? 0, agg: 'sum' }])
  const [filters, setFilters]     = useState<PivotFilterSpec[]>(initial?.filters ?? [])
  const [target, setTarget]       = useState(initial?.target ?? defaultTarget)

  const label = (idx: number) => { const c = columns.find(x => x.idx === idx); return c ? (c.header.trim() || t('pv_col', { defaultValue: 'Colonne {{l}}', l: c.letter })) : '' }
  const toggleRow = (idx: number) => setRowFields(f => f.includes(idx) ? f.filter(x => x !== idx) : [...f, idx])

  const setValue = (i: number, patch: Partial<PivotValueSpec>) =>
    setValues(vs => vs.map((v, j) => j === i ? { ...v, ...patch } : v))
  const setFilter = (i: number, patch: Partial<PivotFilterSpec>) =>
    setFilters(fs => fs.map((f, j) => j === i ? { ...f, ...patch } : f))
  const toggleFilterVal = (i: number, val: string) =>
    setFilters(fs => fs.map((f, j) => j === i
      ? { ...f, selected: f.selected.includes(val) ? f.selected.filter(x => x !== val) : [...f.selected, val] }
      : f))

  const sel = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary'
  const aggs: PivotAgg[] = ['sum', 'count', 'countNum', 'avg', 'min', 'max']

  return (
    <FloatingWindow
      title={editMode ? t('pv_title_edit', { defaultValue: 'Modifier le tableau croisé dynamique' }) : t('pv_title', { defaultValue: 'Tableau croisé dynamique' })}
      icon={<Table2 size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={620} defaultHeight={540} minWidth={520} minHeight={380}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        <div className="text-text-secondary mb-3">{t('pv_source', { defaultValue: 'Source' })} : <span className="font-mono">{rangeLabel}</span></div>

        <div className="grid grid-cols-2 gap-4 flex-1 overflow-auto">
          <div className="space-y-3">
            {/* Lignes (multi, ordonné) */}
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_rows', { defaultValue: 'Lignes' })}</div>
              <div className="border border-border rounded max-h-36 overflow-auto">
                {columns.map(c => (
                  <label key={c.idx} className="flex items-center gap-2 px-2 py-1 hover:bg-surface-1 text-xs cursor-pointer">
                    <input type="checkbox" checked={rowFields.includes(c.idx)} onChange={() => toggleRow(c.idx)} />
                    {label(c.idx)}{rowFields.includes(c.idx) && <span className="ml-auto text-text-tertiary">{rowFields.indexOf(c.idx) + 1}</span>}
                  </label>
                ))}
              </div>
            </div>

            {/* Filtres de rapport */}
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_filters', { defaultValue: 'Filtres' })}</div>
              {filters.map((f, i) => (
                <div key={i} className="border border-border rounded p-2 mb-2 space-y-1">
                  <div className="flex items-center gap-2">
                    <select className={`${sel} flex-1 h-7`} value={f.field}
                      onChange={e => setFilter(i, { field: +e.target.value, selected: [] })}>
                      {columns.map(c => <option key={c.idx} value={c.idx}>{label(c.idx)}</option>)}
                    </select>
                    <button onClick={() => setFilters(fs => fs.filter((_, j) => j !== i))}
                      className="text-text-tertiary hover:text-danger" title={t('pv_filter_remove', { defaultValue: 'Retirer le filtre' })}>
                      <X size={14} />
                    </button>
                  </div>
                  <div className="max-h-24 overflow-auto">
                    {distinct(f.field).map(v => (
                      <label key={v} className="flex items-center gap-2 px-1 py-0.5 text-xs cursor-pointer hover:bg-surface-1">
                        <input type="checkbox" checked={f.selected.includes(v)} onChange={() => toggleFilterVal(i, v)} />
                        <span className="truncate">{v === '' ? t('pv_blank', { defaultValue: '(vide)' }) : v}</span>
                      </label>
                    ))}
                  </div>
                  {f.selected.length === 0 && (
                    <div className="text-[11px] text-text-tertiary">{t('pv_filter_all', { defaultValue: 'Aucune coche = tout inclure' })}</div>
                  )}
                </div>
              ))}
              <button
                onClick={() => setFilters(fs => [...fs, { field: columns[0]?.idx ?? 0, selected: [] }])}
                className="flex items-center gap-1 text-xs text-primary hover:underline">
                <Plus size={13} /> {t('pv_filter_add', { defaultValue: 'Ajouter un filtre' })}
              </button>
            </div>
          </div>

          <div className="space-y-3">
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_cols', { defaultValue: 'Colonnes' })}</div>
              <select className={`${sel} w-full`} value={colField ?? ''} onChange={e => setColField(e.target.value === '' ? null : +e.target.value)}>
                <option value="">{t('pv_none', { defaultValue: '(aucune)' })}</option>
                {columns.map(c => <option key={c.idx} value={c.idx}>{label(c.idx)}</option>)}
              </select>
            </div>

            {/* Valeurs (multi, chacune avec sa fonction) */}
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_values', { defaultValue: 'Valeurs' })}</div>
              {values.map((v, i) => (
                <div key={i} className="flex items-center gap-2 mb-1.5">
                  <select className={`${sel} flex-1`} value={v.field} onChange={e => setValue(i, { field: +e.target.value })}>
                    {columns.map(c => <option key={c.idx} value={c.idx}>{label(c.idx)}</option>)}
                  </select>
                  <select className={`${sel} w-32`} value={v.agg} onChange={e => setValue(i, { agg: e.target.value as PivotAgg })}>
                    {aggs.map(a => <option key={a} value={a}>{AGG_LABEL[a]}</option>)}
                  </select>
                  {values.length > 1 && (
                    <button onClick={() => setValues(vs => vs.filter((_, j) => j !== i))}
                      className="text-text-tertiary hover:text-danger" title={t('pv_value_remove', { defaultValue: 'Retirer' })}>
                      <X size={14} />
                    </button>
                  )}
                </div>
              ))}
              <button
                onClick={() => setValues(vs => [...vs, { field: columns[columns.length - 1]?.idx ?? 0, agg: 'sum' }])}
                className="flex items-center gap-1 text-xs text-primary hover:underline">
                <Plus size={13} /> {t('pv_value_add', { defaultValue: 'Ajouter une valeur' })}
              </button>
            </div>

            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_target', { defaultValue: 'Destination (cellule)' })}</div>
              <Input value={target} onChange={e => setTarget(e.target.value)} className="font-mono" />
            </div>

            <div className="text-[11px] text-text-tertiary leading-snug">
              {t('pv_live_hint', { defaultValue: 'Le tableau croisé reste lié à sa source : il se recalcule automatiquement quand les données changent.' })}
            </div>
          </div>
        </div>

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('pv_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" disabled={rowFields.length === 0 || values.length === 0}
            onClick={() => { onBuild({ rowFields, colField, values, filters, target: target.trim().toUpperCase() || 'A1' }); onClose() }}>
            {editMode ? t('pv_apply', { defaultValue: 'Appliquer' }) : t('pv_build', { defaultValue: 'Créer' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
