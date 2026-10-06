// "Afficher le texte de remplacement…" — the accessible description of a floating
// object (chart, picture, shape), Excel's Alt Text pane.
//
// Fully generic and controlled: it takes the current value and hands a new one back.
// It knows nothing about the sheet, the editor or the object's kind — the caller
// decides where the text is persisted (`SheetChart.altText`, `SheetImage.altText`,
// `SheetShape.altText`).
//
// About "Marquer comme décoratif": Excel stores a real boolean; the sheet model has a
// single `altText` string. The dialog therefore reports the flag separately, and the
// recommended mapping for a host with nowhere to keep it is
// `decorative → altText: ''` (an empty description is exactly what a screen reader
// must skip). Pass `decorative` back in on reopen if the host does keep it.

import { useRef } from 'react'
import type { TFunction } from 'i18next'
import { Checkbox, FloatingWindow } from '@ui'
import { Accessibility } from 'lucide-react'
import { DialogFooter, Section, useDraft } from './dialogFields'

export interface AltTextValue {
  /** Accessible description. Empty when the object is marked decorative. */
  text: string
  /** True when the object carries no meaning and assistive tech should skip it. */
  decorative: boolean
}

export interface AltTextDialogProps {
  /** Current value; in uncontrolled mode (no `onChange`) it seeds the draft on mount. */
  value: AltTextValue
  /** Provide to drive the dialog from the host (live). Omit for plain OK / Annuler. */
  onChange?: (next: AltTextValue) => void
  /** Commit — the dialog does NOT close itself, the host unmounts it. */
  onSubmit: (next: AltTextValue) => void
  onCancel: () => void
  /** Object name appended to the window title ("Image", "Graphique 2", "Forme 1"…). */
  title?: string
  t: TFunction
}

export function AltTextDialog({ value, onChange, onSubmit, onCancel, title, t }: AltTextDialogProps) {
  const [draft, patch] = useDraft(value, onChange)
  // Description typed before ticking "decorative", restored if the user unticks it.
  const kept = useRef(value.text)

  const head = t('sheet_obj_alt_title', { defaultValue: 'Texte de remplacement' })

  const setDecorative = (on: boolean) => {
    if (on) { kept.current = draft.text; patch({ decorative: true, text: '' }) }
    else patch({ decorative: false, text: kept.current })
  }

  const submit = () => onSubmit(draft.decorative
    ? { text: '', decorative: true }
    : { text: draft.text.trim(), decorative: false })

  return (
    <FloatingWindow
      title={title ? `${head} — ${title}` : head}
      icon={<Accessibility size={16} />}
      onClose={onCancel}
      defaultWidth={460}
      backdrop
    >
      <div className="p-4 space-y-3" data-module="office">
        <Section>
          <p className="text-text-secondary">
            {t('sheet_obj_alt_help', { defaultValue: 'Décrivez l’objet pour les personnes qui ne peuvent pas le voir (lecteurs d’écran, image non chargée).' })}
          </p>
          <textarea
            autoFocus
            value={draft.text}
            disabled={draft.decorative}
            onChange={e => patch({ text: e.target.value })}
            // The sheet listens for keys on the window; a field must never feed it.
            onKeyDown={e => e.stopPropagation()}
            rows={4}
            className="w-full px-2 py-1.5 border border-[#dadce0] rounded outline-none focus:border-primary resize-y disabled:bg-surface-2 disabled:text-text-tertiary"
            placeholder={t('sheet_obj_alt_placeholder', { defaultValue: 'Description de l’objet…' })}
          />
        </Section>

        <Checkbox
          checked={draft.decorative}
          onChange={setDecorative}
          label={t('sheet_obj_alt_decorative', { defaultValue: 'Marquer comme décoratif' })}
          description={t('sheet_obj_alt_decorative_help', { defaultValue: 'Les lecteurs d’écran ignoreront cet objet ; aucune description n’est nécessaire.' })}
        />

        <DialogFooter onOk={submit} onCancel={onCancel} t={t} />
      </div>
    </FloatingWindow>
  )
}
