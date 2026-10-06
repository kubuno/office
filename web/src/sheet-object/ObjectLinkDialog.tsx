// « Lien… » — the hyperlink carried by a floating object (picture, shape). Excel puts
// the same "Insert hyperlink" dialog behind it; ours is the one field that survives in
// the sheet model (`SheetImage.link` / `SheetShape.link`), plus the removal command.
//
// Charts are NOT linkable (Excel refuses too), so this dialog is never opened for one —
// the object's `ObjBinding.linkable` decides.
//
// Fully generic and controlled: it reads a URL and hands a new one back. An empty
// submission and « Supprimer le lien » both mean "no link", i.e. `''`.

import type { TFunction } from 'i18next'
import { Button, FloatingWindow } from '@ui'
import { Link2 } from 'lucide-react'
import { DialogFooter, Section, useDraft } from './dialogFields'

export interface ObjectLinkValue {
  /** Hyperlink target; '' when the object carries none. */
  link: string
}

export interface ObjectLinkDialogProps {
  /** Current value; in uncontrolled mode (no `onChange`) it seeds the draft on mount. */
  value: ObjectLinkValue
  /** Provide to drive the dialog from the host (live). Omit for plain OK / Annuler. */
  onChange?: (next: ObjectLinkValue) => void
  /** Commit — the dialog does NOT close itself, the host unmounts it. */
  onSubmit: (next: ObjectLinkValue) => void
  onCancel: () => void
  /** Object name appended to the window title ("Image 2", "Forme 1"…). */
  title?: string
  t: TFunction
}

export function ObjectLinkDialog({ value, onChange, onSubmit, onCancel, title, t }: ObjectLinkDialogProps) {
  const [draft, patch] = useDraft(value, onChange)
  const head = t('sheet_obj_link_title', { defaultValue: 'Lien' })

  return (
    <FloatingWindow
      title={title ? `${head} — ${title}` : head}
      icon={<Link2 size={16} />}
      onClose={onCancel}
      defaultWidth={460}
      backdrop
    >
      <div className="flex flex-col p-4 gap-3" data-module="office">
        <Section>
          <label className="flex items-center gap-2">
            <span className="text-text-secondary shrink-0" style={{ width: 80 }}>
              {t('sheet_obj_link_url', { defaultValue: 'Adresse' })}
            </span>
            <input
              autoFocus
              type="url"
              value={draft.link}
              onChange={e => patch({ link: e.target.value })}
              // The sheet listens for keys on the window; a field must never feed it.
              onKeyDown={e => {
                e.stopPropagation()
                if (e.key === 'Enter') onSubmit({ link: draft.link.trim() })
              }}
              placeholder="https://"
              className="flex-1 min-w-0 h-8 px-2 border border-[#dadce0] rounded outline-none focus:border-primary"
            />
          </label>
          <div className="text-text-secondary">
            {t('sheet_obj_link_help', { defaultValue: 'Un clic sur l’objet ouvrira cette adresse dans un nouvel onglet.' })}
          </div>
        </Section>

        <DialogFooter
          onOk={() => onSubmit({ link: draft.link.trim() })}
          onCancel={onCancel}
          t={t}
          left={draft.link ? (
            <Button variant="ghost" onClick={() => onSubmit({ link: '' })}>
              {t('sheet_obj_link_remove', { defaultValue: 'Supprimer le lien' })}
            </Button>
          ) : undefined}
        />
      </div>
    </FloatingWindow>
  )
}
