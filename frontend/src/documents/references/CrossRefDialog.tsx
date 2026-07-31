// Word's « Renvoi »: insert a reference to a heading, a caption or a bookmark,
// as its text, its page number, or both.
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Checkbox, Dropdown, FloatingWindow } from '@ui'

import { DLG_BTN } from '../../lib'

export type CrossRefKind = 'heading' | 'figure' | 'bookmark'
export type CrossRefWhat = 'text' | 'page' | 'textPage'

export interface CrossRefTarget {
  id: string
  label: string
  page: number
  kind: CrossRefKind
}

export interface CrossRefDialogProps {
  targets: CrossRefTarget[]
  onInsert: (target: CrossRefTarget, what: CrossRefWhat, asLink: boolean) => void
  onClose: () => void
}

export function CrossRefDialog({ targets, onInsert, onClose }: CrossRefDialogProps) {
  const { t } = useTranslation('office')
  const label = (k: string, d: string) => t(k, { defaultValue: d })
  const [kind, setKind] = useState<CrossRefKind>('heading')
  const [what, setWhat] = useState<CrossRefWhat>('textPage')
  const [asLink, setAsLink] = useState(true)
  const list = useMemo(() => targets.filter(x => x.kind === kind), [targets, kind])
  const [sel, setSel] = useState<string>('')
  const current = list.find(x => x.id === sel) ?? list[0]

  return (
    <FloatingWindow title={label('doc_crossref', 'Renvoi')} onClose={onClose} defaultWidth={520} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <div className="flex gap-4">
          <label className="flex items-center gap-2">
            <span className="text-text-secondary">{label('doc_xref_type', 'Catégorie :')}</span>
            <Dropdown width={160} height={32} value={kind}
              options={[
                { value: 'heading', label: label('doc_xref_heading', 'Titre') },
                { value: 'figure', label: label('doc_xref_figure', 'Légende') },
                { value: 'bookmark', label: label('doc_xref_bookmark', 'Signet') },
              ]}
              onChange={v => { setKind(v as CrossRefKind); setSel('') }} />
          </label>
          <label className="flex items-center gap-2">
            <span className="text-text-secondary">{label('doc_xref_insert_what', 'Insérer :')}</span>
            <Dropdown width={170} height={32} value={what}
              options={[
                { value: 'textPage', label: label('doc_xref_text_page', 'Texte et numéro de page') },
                { value: 'text', label: label('doc_xref_text', 'Texte seul') },
                { value: 'page', label: label('doc_xref_page', 'Numéro de page') },
              ]}
              onChange={v => setWhat(v as CrossRefWhat)} />
          </label>
        </div>

        <div>
          <div className="text-text-secondary mb-1">{label('doc_xref_for', 'Pour quel élément :')}</div>
          <div className="border border-border rounded-[--kb-radius-sm] h-[190px] overflow-auto">
            {list.length === 0 && (
              <div className="px-2 py-2 text-text-tertiary">{label('doc_xref_empty', 'Aucun élément de cette catégorie.')}</div>
            )}
            {list.map(x => (
              <button key={x.id} type="button" onMouseDown={e => e.preventDefault()} onClick={() => setSel(x.id)}
                className={`w-full text-left px-2 py-1 truncate ${current?.id === x.id ? 'bg-primary/15' : 'hover:bg-surface-hover'}`}>
                {x.label}
              </button>
            ))}
          </div>
        </div>

        <label className="flex items-center gap-2">
          <Checkbox checked={asLink} onChange={setAsLink} />
          <span>{label('doc_xref_as_link', 'Insérer comme lien hypertexte')}</span>
        </label>

        <div className="flex justify-end gap-2 pt-1">
          <Button className={DLG_BTN} variant="primary" size="sm" disabled={!current}
            onClick={() => { if (current) { onInsert(current, what, asLink); onClose() } }}>
            {t('doc_insert', { defaultValue: 'Insérer' })}
          </Button>
          <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
            {t('common_cancel', { defaultValue: 'Annuler' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

export default CrossRefDialog
