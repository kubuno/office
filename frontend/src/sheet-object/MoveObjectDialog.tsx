// « Déplacer le graphique… » — Excel's "Move Chart" dialog, reduced to the one
// choice that means something here: WHICH SHEET the object lives on.
//
// Excel offers two placements: a brand-new chart SHEET (a tab that holds nothing but
// the chart) or an object embedded in an existing worksheet. The sheet model has no
// notion of a chart-only sheet — every sheet is a grid — so only the second is
// offered, as a plain sheet picker. Adding the first would mean a sheet kind the rest
// of the editor cannot render, i.e. a dead option.
//
// Fully generic and controlled: it knows nothing about charts, the sheet data or the
// API. The host receives a target sheet id and performs the move (read the target's
// object array, append, write both sheets).

import { useState } from 'react'
import type { TFunction } from 'i18next'
import { FloatingWindow, Radio } from '@ui'
import { Move } from 'lucide-react'
import { DialogFooter, Section } from './dialogFields'

/** One candidate destination. */
export interface MoveObjectSheet {
  id: string
  name: string
}

export interface MoveObjectDialogProps {
  /** Every sheet of the workbook, in tab order. */
  sheets: readonly MoveObjectSheet[]
  /** Sheet the object sits on today — preselected, and never a valid target. */
  currentSheetId: string
  /** Commit — the dialog does NOT close itself, the host unmounts it. */
  onSubmit: (targetSheetId: string) => void
  onCancel: () => void
  /** Object name appended to the window title ("Graphique 2"…). */
  title?: string
  /** True while the host is writing; disables OK so the move cannot be fired twice. */
  busy?: boolean
  t: TFunction
}

export function MoveObjectDialog({
  sheets, currentSheetId, onSubmit, onCancel, title, busy, t,
}: MoveObjectDialogProps) {
  const [target, setTarget] = useState(currentSheetId)

  const head = t('sheet_obj_move_title', { defaultValue: 'Déplacer le graphique' })
  // Moving onto its own sheet is a no-op: OK stays disabled rather than silently
  // rewriting the sheet.
  const canSubmit = !busy && !!target && target !== currentSheetId

  return (
    <FloatingWindow
      title={title ? `${head} — ${title}` : head}
      icon={<Move size={16} />}
      onClose={onCancel}
      defaultWidth={420}
      backdrop
    >
      <div className="flex flex-col p-4 gap-3" data-module="office">
        <Section title={t('sheet_obj_move_target', { defaultValue: 'Feuille de destination' })}>
          <div className="max-h-64 overflow-y-auto flex flex-col gap-1.5">
            {sheets.map(s => (
              <Radio
                key={s.id}
                checked={target === s.id}
                onChange={() => setTarget(s.id)}
                label={s.id === currentSheetId
                  ? t('sheet_obj_move_current', { defaultValue: '{{name}} (feuille actuelle)', name: s.name })
                  : s.name}
              />
            ))}
          </div>
          <div className="text-text-secondary">
            {t('sheet_obj_move_help', { defaultValue: 'L’objet garde sa taille et sa mise en forme ; il reprend sa position sur la feuille de destination.' })}
          </div>
        </Section>

        <DialogFooter onOk={() => onSubmit(target)} onCancel={onCancel} okDisabled={!canSubmit} t={t} />
      </div>
    </FloatingWindow>
  )
}
