// Word's « Macros » dialog (Affichage → Macros → Afficher les macros).
//
// Same layout as Word: the macro name on top, the list below, the action column
// on the right, then the scope selector and the description.
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Dropdown, FloatingWindow } from '@ui'

import { DLG_BTN } from '../lib'

export interface MacroRow {
  key: string
  label: string
  /** First comment line of the source, shown as Word shows the description. */
  description?: string
}

export interface MacrosDialogProps {
  macros: MacroRow[]
  docTitle: string
  busy?: boolean
  onRun: (key: string) => void
  onEdit: (key: string) => void
  onCreate: (name: string) => void
  onDelete: (key: string) => void
  onClose: () => void
}

export function MacrosDialog({
  macros, docTitle, busy, onRun, onEdit, onCreate, onDelete, onClose,
}: MacrosDialogProps) {
  const { t } = useTranslation('office')
  const label = (k: string, d: string) => t(k, { defaultValue: d })
  const [sel, setSel] = useState<string>(macros[0]?.key ?? '')
  const [name, setName] = useState<string>(macros[0]?.label ?? '')

  useEffect(() => {
    if (!macros.some(m => m.key === sel)) {
      setSel(macros[0]?.key ?? '')
      setName(macros[0]?.label ?? '')
    }
  }, [macros, sel])

  const current = useMemo(() => macros.find(m => m.key === sel) ?? null, [macros, sel])
  // Word enables « Créer » when the typed name matches no existing macro, and
  // the run/edit/delete column when a macro IS selected.
  const typed = name.trim()
  const known = macros.some(m => m.label.toLowerCase() === typed.toLowerCase())
  const canCreate = !!typed && !known

  const act = (fn: (key: string) => void) => () => { if (current) fn(current.key) }

  return (
    <FloatingWindow title={label('macros_title', 'Macros')} onClose={onClose} defaultWidth={640} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <label className="flex flex-col gap-1">
          <span className="text-text-secondary">{label('macro_name', 'Nom de la macro :')}</span>
          <input type="text" value={name} autoFocus
            onChange={e => {
              setName(e.target.value)
              const hit = macros.find(m => m.label.toLowerCase() === e.target.value.trim().toLowerCase())
              if (hit) setSel(hit.key)
            }}
            className="h-8 px-2 rounded-[--kb-radius-sm] border border-border bg-surface outline-none focus:ring-2 focus:ring-primary" />
        </label>

        <div className="flex gap-3">
          <div className="flex-1 min-w-0 border border-border rounded-[--kb-radius-sm] h-[200px] overflow-auto bg-surface">
            {macros.map(m => (
              <button key={m.key} type="button" onMouseDown={e => e.preventDefault()}
                onDoubleClick={() => onRun(m.key)}
                onClick={() => { setSel(m.key); setName(m.label) }}
                className={`w-full text-left px-2 py-1 truncate ${m.key === sel ? 'bg-primary/15' : 'hover:bg-surface-hover'}`}>
                {m.label}
              </button>
            ))}
            {!macros.length && (
              <div className="px-2 py-2 text-text-tertiary">{label('macros_empty', 'Aucune macro dans ce document.')}</div>
            )}
          </div>

          <div className="w-[170px] shrink-0 flex flex-col gap-2">
            <Button size="sm" variant="primary" className="justify-center" disabled={!current || busy}
              onClick={act(onRun)}>{label('macro_run', 'Exécuter')}</Button>
            <Button size="sm" variant="secondary" className="justify-center" disabled={!current}
              onClick={act(onEdit)}>{label('macro_modify', 'Modifier')}</Button>
            <Button size="sm" variant="secondary" className="justify-center" disabled={!canCreate}
              onClick={() => onCreate(typed)}>{label('macro_create', 'Créer')}</Button>
            <Button size="sm" variant="secondary" className="justify-center" disabled={!current}
              onClick={act(onDelete)}>{label('macro_delete_btn', 'Supprimer')}</Button>
          </div>
        </div>

        <label className="flex items-center gap-2">
          <span className="text-text-secondary shrink-0">{label('macros_scope', 'Macros disponibles dans :')}</span>
          <Dropdown width={280} height={32} value="doc"
            options={[{ value: 'doc', label: label('macros_scope_doc', 'Ce document ({{n}})').replace('{{n}}', docTitle) }]}
            onChange={() => {}} />
        </label>

        <div className="flex flex-col gap-1">
          <span className="text-text-secondary">{label('macro_description', 'Description :')}</span>
          <div className="min-h-[64px] px-2 py-1 rounded-[--kb-radius-sm] border border-border bg-surface text-text-secondary whitespace-pre-wrap">
            {current?.description || ''}
          </div>
        </div>

        <div className="flex justify-end">
          <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
            {t('common_cancel', { defaultValue: 'Annuler' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

export default MacrosDialog
