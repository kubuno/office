// Word's « Marquer entrée » (index) and « Marquer citation » (table of
// authorities). Same shape, two payloads, so one component.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Dropdown, FloatingWindow } from '@ui'

import { DLG_BTN } from '../../lib'
import { AUTHORITY_CATEGORIES, type AuthorityCategory } from './types'

export type MarkMode = 'index' | 'citation'

export interface MarkEntryDialogProps {
  mode: MarkMode
  /** Text currently selected — what Word pre-fills the main field with. */
  initial: string
  onMarkIndex: (text: string, sub: string) => void
  onMarkCitation: (long: string, short: string, category: AuthorityCategory) => void
  onClose: () => void
}

const CATEGORY_FR: Record<AuthorityCategory, string> = {
  all: 'Toutes', cases: 'Cas', statutes: 'Statuts', other: 'Autres autorités',
  rules: 'Règles', treatises: 'Traités', regulations: 'Règlements',
  constitutional: 'Dispositions constitutionnelles',
}

export function MarkEntryDialog({ mode, initial, onMarkIndex, onMarkCitation, onClose }: MarkEntryDialogProps) {
  const { t } = useTranslation('office')
  const label = (k: string, d: string) => t(k, { defaultValue: d })
  const [main, setMain] = useState(initial)
  const [second, setSecond] = useState('')
  const [category, setCategory] = useState<AuthorityCategory>('cases')

  const Field = ({ lbl, value, onChange }: { lbl: string; value: string; onChange: (v: string) => void }) => (
    <label className="flex items-center gap-2">
      <span className="w-[150px] text-text-secondary shrink-0">{lbl}</span>
      <input type="text" value={value} onChange={e => onChange(e.target.value)} autoFocus={lbl.startsWith('Entrée') || lbl.startsWith('Citation')}
        className="flex-1 h-8 px-2 rounded-[--kb-radius-sm] border border-border bg-surface outline-none focus:ring-2 focus:ring-primary" />
    </label>
  )

  const apply = () => {
    if (!main.trim()) return
    if (mode === 'index') onMarkIndex(main, second)
    else onMarkCitation(main, second, category)
    onClose()
  }

  return (
    <FloatingWindow
      title={mode === 'index' ? label('doc_index_mark_title', "Marquer l'entrée d'index") : label('doc_toa_mark_title', 'Marquer la citation')}
      onClose={onClose} defaultWidth={520} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        {mode === 'index' ? (
          <>
            <Field lbl={label('doc_index_entry', 'Entrée :')} value={main} onChange={setMain} />
            <Field lbl={label('doc_index_subentry', 'Sous-entrée :')} value={second} onChange={setSecond} />
          </>
        ) : (
          <>
            <Field lbl={label('doc_toa_long', 'Citation longue :')} value={main} onChange={setMain} />
            <Field lbl={label('doc_toa_short', 'Citation courte :')} value={second} onChange={setSecond} />
            <label className="flex items-center gap-2">
              <span className="w-[150px] text-text-secondary shrink-0">{label('doc_toa_category', 'Catégorie :')}</span>
              <Dropdown width={220} height={32} value={category}
                options={AUTHORITY_CATEGORIES.filter(c => c !== 'all')
                  .map(c => ({ value: c, label: label('doc_toa_cat_' + c, CATEGORY_FR[c]) }))}
                onChange={v => setCategory(v as AuthorityCategory)} />
            </label>
          </>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Button className={DLG_BTN} variant="primary" size="sm" disabled={!main.trim()} onClick={apply}>
            {label('doc_mark_btn', 'Marquer')}
          </Button>
          <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
            {t('common_cancel', { defaultValue: 'Annuler' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

export default MarkEntryDialog
