import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button } from '@ui'
import { ArrowDownUp, Plus, Trash2, ArrowUp, ArrowDown } from 'lucide-react'

export interface SortColumn { idx: number; letter: string; header: string }
export interface SortLevel { col: number; asc: boolean }

interface Props {
  columns:        SortColumn[]   // colonnes de la plage (idx absolu + lettre + en-tête)
  rangeLabel:     string         // ex. "A1:D20" (affiché)
  initialHeaders: boolean
  onSort:         (headers: boolean, levels: SortLevel[]) => void
  onClose:        () => void
}

// Tri multi-colonnes façon Excel : « Trier par » + N niveaux « puis par ».
export default function SortDialog({ columns, rangeLabel, initialHeaders, onSort, onClose }: Props) {
  const { t } = useTranslation('office')
  const [headers, setHeaders] = useState(initialHeaders)
  const [levels, setLevels]   = useState<SortLevel[]>([{ col: columns[0]?.idx ?? 0, asc: true }])

  const colLabel = (idx: number): string => {
    const c = columns.find(x => x.idx === idx)
    if (!c) return ''
    return headers && c.header.trim() ? c.header : t('sort_column', { defaultValue: 'Colonne {{l}}', l: c.letter })
  }
  // Colonnes encore disponibles pour un nouveau niveau (chacune triée une fois).
  const used = new Set(levels.map(l => l.col))
  const firstFree = columns.find(c => !used.has(c.idx))?.idx ?? columns[0]?.idx ?? 0

  const setLevel = (i: number, patch: Partial<SortLevel>) => setLevels(ls => ls.map((l, j) => j === i ? { ...l, ...patch } : l))
  const addLevel = () => setLevels(ls => [...ls, { col: firstFree, asc: true }])
  const removeLevel = (i: number) => setLevels(ls => ls.length > 1 ? ls.filter((_, j) => j !== i) : ls)
  const move = (i: number, d: -1 | 1) => setLevels(ls => {
    const j = i + d; if (j < 0 || j >= ls.length) return ls
    const a = [...ls];[a[i], a[j]] = [a[j], a[i]]; return a
  })

  const sel = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary'

  return (
    <FloatingWindow
      title={t('sort_title', { defaultValue: 'Trier la plage' })}
      icon={<ArrowDownUp size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={540} defaultHeight={400} minWidth={460} minHeight={300}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        <div className="flex items-center justify-between mb-3">
          <span className="text-text-secondary">{t('sort_range', { defaultValue: 'Plage' })} : <span className="font-mono">{rangeLabel}</span></span>
          <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={headers} onChange={e => setHeaders(e.target.checked)} /> {t('sort_has_headers', { defaultValue: 'Ligne d’en-tête' })}</label>
        </div>

        <div className="flex-1 overflow-auto space-y-2">
          {levels.map((lv, i) => (
            <div key={i} className="flex items-center gap-2">
              <span className="text-xs text-text-tertiary w-16 flex-shrink-0">{i === 0 ? t('sort_by', { defaultValue: 'Trier par' }) : t('sort_then', { defaultValue: 'puis par' })}</span>
              <select className={`${sel} flex-1`} value={lv.col} onChange={e => setLevel(i, { col: +e.target.value })}>
                {columns.map(c => <option key={c.idx} value={c.idx}>{colLabel(c.idx)}</option>)}
              </select>
              <select className={`${sel} w-44`} value={lv.asc ? '1' : '0'} onChange={e => setLevel(i, { asc: e.target.value === '1' })}>
                <option value="1">{t('sort_asc', { defaultValue: 'Croissant (A→Z, 0→9)' })}</option>
                <option value="0">{t('sort_desc', { defaultValue: 'Décroissant (Z→A, 9→0)' })}</option>
              </select>
              <button className="p-1 rounded hover:bg-surface-2 text-text-secondary disabled:opacity-30" disabled={i === 0} onClick={() => move(i, -1)}><ArrowUp size={14} /></button>
              <button className="p-1 rounded hover:bg-surface-2 text-text-secondary disabled:opacity-30" disabled={i === levels.length - 1} onClick={() => move(i, 1)}><ArrowDown size={14} /></button>
              <button className="p-1 rounded hover:bg-danger-light text-danger disabled:opacity-30" disabled={levels.length <= 1} onClick={() => removeLevel(i)}><Trash2 size={14} /></button>
            </div>
          ))}
          <button onClick={addLevel} disabled={levels.length >= columns.length} className="flex items-center gap-1.5 text-primary text-xs hover:underline disabled:opacity-40 disabled:no-underline mt-1">
            <Plus size={14} /> {t('sort_add_level', { defaultValue: 'Ajouter un niveau' })}
          </button>
        </div>

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('sort_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" onClick={() => { onSort(headers, levels); onClose() }}>{t('sort_apply', { defaultValue: 'Trier' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
