// One-line "give it a name" dialog — « Enregistrer comme modèle… » asks for the
// template's name with it, and any later "name this preset" command can reuse it.
//
// Why not `appPrompt` (the module's own prompt, from macros/FormRuntime): its host
// component `DialogHost` is mounted by `MacrosMenu`, i.e. only while the ribbon's
// Affichage tab is rendered. Called from anywhere else, `appPrompt` resolves `null`
// straight away and the command silently does nothing. A real dialog cannot fail
// that way.
//
// Fully generic and controlled: name in, name out. Never a browser dialog.

import { useState } from 'react'
import type { TFunction } from 'i18next'
import { FloatingWindow } from '@ui'
import { Save } from 'lucide-react'
import { DialogFooter, Section } from './dialogFields'

export interface NameDialogProps {
  /** Pre-filled suggestion, selected on open so typing replaces it. */
  value: string
  /** Commit — the dialog does NOT close itself, the host unmounts it. */
  onSubmit: (name: string) => void
  onCancel: () => void
  /** Window title, e.g. « Enregistrer comme modèle ». */
  title: string
  /** Label of the single field (default « Nom »). */
  label?: string
  /** Optional hint under the field. */
  help?: string
  t: TFunction
}

export function NameDialog({ value, onSubmit, onCancel, title, label, help, t }: NameDialogProps) {
  const [name, setName] = useState(value)
  const trimmed = name.trim()

  return (
    <FloatingWindow
      // Callers reuse the MENU label, whose trailing ellipsis means "opens a dialog";
      // once open it is noise, so it is dropped here rather than duplicating labels in
      // the 13 language blocks. Both the single character and the three dots.
      title={title.replace(/(…|\.\.\.)\s*$/, '')}
      icon={<Save size={16} />}
      onClose={onCancel}
      defaultWidth={420}
      backdrop
    >
      <div className="flex flex-col p-4 gap-3" data-module="office">
        <Section>
          <label className="flex items-center gap-2">
            <span className="text-text-secondary shrink-0" style={{ width: 80 }}>
              {label ?? t('sheet_obj_name_label', { defaultValue: 'Nom' })}
            </span>
            <input
              autoFocus
              value={name}
              onChange={e => setName(e.target.value)}
              // The sheet listens for keys on the window; a field must never feed it.
              onKeyDown={e => {
                e.stopPropagation()
                if (e.key === 'Enter' && trimmed) onSubmit(trimmed)
              }}
              onFocus={e => e.currentTarget.select()}
              className="flex-1 min-w-0 h-8 px-2 border border-[#dadce0] rounded outline-none focus:border-primary"
            />
          </label>
          {help && <div className="text-text-secondary">{help}</div>}
        </Section>

        <DialogFooter onOk={() => onSubmit(trimmed)} onCancel={onCancel} okDisabled={!trimmed} t={t} />
      </div>
    </FloatingWindow>
  )
}
