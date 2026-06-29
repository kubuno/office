import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input } from '@ui'
import { Table2 } from 'lucide-react'
import type { PivotAgg } from './pivot-engine'
import { AGG_LABEL } from './pivot-engine'

export interface PivotColumn { idx: number; letter: string; header: string }
export interface PivotResult { rowFields: number[]; colField: number | null; valueField: number; agg: PivotAgg; target: string }

interface Props {
  columns:       PivotColumn[]
  rangeLabel:    string
  defaultTarget: string
  onBuild:       (r: PivotResult) => void
  onClose:       () => void
}

export default function PivotDialog({ columns, rangeLabel, defaultTarget, onBuild, onClose }: Props) {
  const { t } = useTranslation('office')
  const [rowFields, setRowFields] = useState<number[]>(columns[0] ? [columns[0].idx] : [])
  const [colField, setColField]   = useState<number | null>(null)
  const [valueField, setValueField] = useState<number>(columns[columns.length - 1]?.idx ?? 0)
  const [agg, setAgg]   = useState<PivotAgg>('sum')
  const [target, setTarget] = useState(defaultTarget)

  const label = (idx: number) => { const c = columns.find(x => x.idx === idx); return c ? (c.header.trim() || t('pv_col', { defaultValue: 'Colonne {{l}}', l: c.letter })) : '' }
  const toggleRow = (idx: number) => setRowFields(f => f.includes(idx) ? f.filter(x => x !== idx) : [...f, idx])

  const sel = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary'
  const aggs: PivotAgg[] = ['sum', 'count', 'countNum', 'avg', 'min', 'max']

  return (
    <FloatingWindow
      title={t('pv_title', { defaultValue: 'Tableau croisé dynamique' })}
      icon={<Table2 size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={560} defaultHeight={460} minWidth={480} minHeight={340}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        <div className="text-text-secondary mb-3">{t('pv_source', { defaultValue: 'Source' })} : <span className="font-mono">{rangeLabel}</span></div>

        <div className="grid grid-cols-2 gap-4 flex-1 overflow-auto">
          {/* Lignes (multi, ordonné) */}
          <div>
            <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_rows', { defaultValue: 'Lignes' })}</div>
            <div className="border border-border rounded max-h-40 overflow-auto">
              {columns.map(c => (
                <label key={c.idx} className="flex items-center gap-2 px-2 py-1 hover:bg-surface-1 text-xs cursor-pointer">
                  <input type="checkbox" checked={rowFields.includes(c.idx)} onChange={() => toggleRow(c.idx)} />
                  {label(c.idx)}{rowFields.includes(c.idx) && <span className="ml-auto text-text-tertiary">{rowFields.indexOf(c.idx) + 1}</span>}
                </label>
              ))}
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
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_values', { defaultValue: 'Valeurs' })}</div>
              <select className={`${sel} w-full`} value={valueField} onChange={e => setValueField(+e.target.value)}>
                {columns.map(c => <option key={c.idx} value={c.idx}>{label(c.idx)}</option>)}
              </select>
            </div>
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_agg', { defaultValue: 'Synthétiser par' })}</div>
              <select className={`${sel} w-full`} value={agg} onChange={e => setAgg(e.target.value as PivotAgg)}>
                {aggs.map(a => <option key={a} value={a}>{AGG_LABEL[a]}</option>)}
              </select>
            </div>
            <div>
              <div className="text-xs font-medium text-text-secondary mb-1">{t('pv_target', { defaultValue: 'Destination (cellule)' })}</div>
              <Input value={target} onChange={e => setTarget(e.target.value)} className="font-mono" />
            </div>
          </div>
        </div>

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('pv_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" disabled={rowFields.length === 0}
            onClick={() => { onBuild({ rowFields, colField, valueField, agg, target: target.trim().toUpperCase() || 'A1' }); onClose() }}>
            {t('pv_build', { defaultValue: 'Créer' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
