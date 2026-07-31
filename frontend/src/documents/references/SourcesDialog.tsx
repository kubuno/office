// Word's « Gérer les sources » / « Ajouter une nouvelle source… »: the list of
// the document's sources on the left, the edited source on the right.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Dropdown, FloatingWindow } from '@ui'

import { DLG_BTN } from '../../lib'
import { EMPTY_SOURCE, SOURCE_TYPES, sortSources, sourceLabel, type Source, type SourceType } from './sources'

export interface SourcesDialogProps {
  sources: Source[]
  /** `true` when opened by « Insérer une citation » — the OK button inserts. */
  pickMode?: boolean
  onSave: (sources: Source[]) => void
  onInsert?: (source: Source) => void
  onClose: () => void
}

const TYPE_LABELS: Record<SourceType, string> = {
  book: 'Livre', article: 'Article de revue', website: 'Site web', report: 'Rapport',
}

const uid = (): string => `s${Date.now().toString(36)}${Math.floor(Math.random() * 1e6).toString(36)}`

export function SourcesDialog({ sources, pickMode, onSave, onInsert, onClose }: SourcesDialogProps) {
  const { t } = useTranslation('office')
  const label = (k: string, d: string) => t(k, { defaultValue: d })
  const [list, setList] = useState<Source[]>(sources)
  const [selId, setSelId] = useState<string>(sources[0]?.id ?? '')
  const sel = list.find(s => s.id === selId) ?? null

  const patch = (p: Partial<Source>) =>
    setList(l => l.map(s => (s.id === selId ? { ...s, ...p } : s)))

  const addNew = () => {
    const s: Source = { id: uid(), ...EMPTY_SOURCE }
    setList(l => [...l, s])
    setSelId(s.id)
  }

  const field = (key: keyof Source, lbl: string) => (
    <label className="flex items-center gap-2">
      <span className="w-[110px] text-text-secondary shrink-0">{lbl}</span>
      <input type="text" value={String(sel?.[key] ?? '')} disabled={!sel}
        onChange={e => patch({ [key]: e.target.value } as Partial<Source>)}
        className="flex-1 h-8 px-2 rounded-[--kb-radius-sm] border border-border bg-surface outline-none focus:ring-2 focus:ring-primary disabled:opacity-40" />
    </label>
  )

  return (
    <FloatingWindow
      title={pickMode ? label('doc_cite_pick_title', 'Insérer une citation') : label('doc_cite_manage', 'Gérer les sources')}
      onClose={onClose} defaultWidth={680} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <div className="flex gap-4">
          <div className="w-[240px] shrink-0 flex flex-col gap-2">
            <div className="text-text-secondary">{label('doc_cite_list', 'Sources du document :')}</div>
            <div className="border border-border rounded-[--kb-radius-sm] h-[230px] overflow-auto">
              {sortSources(list).map(s => (
                <button key={s.id} type="button" onMouseDown={e => e.preventDefault()} onClick={() => setSelId(s.id)}
                  className={`w-full text-left px-2 py-1 truncate ${s.id === selId ? 'bg-primary/15' : 'hover:bg-surface-hover'}`}>
                  {sourceLabel(s)}
                </button>
              ))}
              {!list.length && (
                <div className="px-2 py-2 text-text-tertiary">{label('doc_cite_empty', 'Aucune source.')}</div>
              )}
            </div>
            <div className="flex gap-2">
              <Button size="sm" variant="secondary" onClick={addNew}>{label('doc_cite_add', 'Nouvelle')}</Button>
              <Button size="sm" variant="secondary" disabled={!sel}
                onClick={() => { setList(l => l.filter(s => s.id !== selId)); setSelId('') }}>
                {label('doc_cite_delete', 'Supprimer')}
              </Button>
            </div>
          </div>

          <div className="flex-1 min-w-0 flex flex-col gap-2">
            <label className="flex items-center gap-2">
              <span className="w-[110px] text-text-secondary shrink-0">{label('doc_cite_type', 'Type de source')}</span>
              <Dropdown width={200} height={32} value={sel?.type ?? 'book'} disabled={!sel}
                options={SOURCE_TYPES.map(v => ({ value: v, label: label('doc_cite_type_' + v, TYPE_LABELS[v]) }))}
                onChange={v => patch({ type: v as SourceType })} />
            </label>
            {field('author', label('doc_cite_author', 'Auteur'))}
            {field('title', label('doc_cite_title', 'Titre'))}
            {field('year', label('doc_cite_year', 'Année'))}
            {field('container', label('doc_cite_container', 'Revue / site'))}
            {field('publisher', label('doc_cite_publisher', 'Éditeur'))}
            {field('url', label('doc_cite_url', 'URL'))}
          </div>
        </div>

        <div className="flex justify-end gap-2 pt-1">
          <Button className={DLG_BTN} variant="primary" size="sm" disabled={pickMode && !sel}
            onClick={() => {
              onSave(list)
              if (pickMode && sel && onInsert) onInsert(sel)
              onClose()
            }}>
            {pickMode ? t('doc_insert', { defaultValue: 'Insérer' }) : t('common_ok', { defaultValue: 'OK' })}
          </Button>
          <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
            {t('common_cancel', { defaultValue: 'Annuler' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

export default SourcesDialog
