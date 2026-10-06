// Chart creation/edit wizard, modelled on the LibreOffice Calc 4-step chart
// wizard (dlg_CreationWizard.cxx): Chart type → Data range → Data series →
// Chart elements. Free roadmap navigation, Back/Next, and Finish available
// from step 1 (every setting has a sensible default).
//
// The wizard edits a spread-copy of the persisted `SheetChart`; unknown /
// legacy fields survive untouched and `onConfirm` hands the full draft back
// to the host (which persists it via persistCharts).

import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { BarChart3 } from 'lucide-react'
import { Button, FloatingWindow } from '@ui'
import type { SheetChart } from './../api'
import { DLG_BTN } from './../lib'
import { CellValueGetter } from './data'
import { StepType } from './wizard/StepType'
import { StepRange } from './wizard/StepRange'
import { StepSeries } from './wizard/StepSeries'
import { StepElements } from './wizard/StepElements'
import { WizardPreview } from './wizard/Preview'

export interface ChartWizardProps {
  /** Creation: e.g. `{ type: 'bar', range: <selection> }`; edit: the existing chart. */
  initial: SheetChart
  getCell: CellValueGetter
  /** Optional host hook to pick a range on the grid (step 2 target button). */
  onPickRange?: (cb: (range: string) => void) => void
  onConfirm: (chart: SheetChart) => void
  onCancel: () => void
  /**
   * Step to land on. Defaults to the first one; the chart context menu uses it to
   * open « Modifier le type de graphique… » on 'type' and « Sélectionner des
   * données… » on 'range'. Navigation stays free afterwards.
   */
  initialStep?: ChartWizardStep
}

const STEPS = [
  { id: 'type', labelKey: 'sheet_chart_step_type', labelDefault: 'Type de diagramme' },
  { id: 'range', labelKey: 'sheet_chart_step_range', labelDefault: 'Plage de données' },
  { id: 'series', labelKey: 'sheet_chart_step_series', labelDefault: 'Séries de données' },
  { id: 'elements', labelKey: 'sheet_chart_step_elements', labelDefault: 'Éléments du diagramme' },
] as const

/** Identifier of one wizard step, for `initialStep`. */
export type ChartWizardStep = typeof STEPS[number]['id']

export function ChartWizard({ initial, getCell, onPickRange, onConfirm, onCancel, initialStep }: ChartWizardProps) {
  const { t } = useTranslation('office')
  const [draft, setDraft] = useState<SheetChart>(() => ({ ...initial }))
  const [step, setStep] = useState(() => {
    const i = STEPS.findIndex(s => s.id === initialStep)
    return i >= 0 ? i : 0
  })
  // Preview identity: strip the id so downstream code treats it as ephemeral.
  const previewChart = useMemo(() => ({ ...draft, id: undefined }), [draft])

  const stepBody = () => {
    switch (STEPS[step].id) {
      case 'type': return <StepType draft={draft} onChange={setDraft} t={t} />
      case 'range': return <StepRange draft={draft} onChange={setDraft} getCell={getCell} onPickRange={onPickRange} t={t} />
      case 'series': return <StepSeries draft={draft} onChange={setDraft} getCell={getCell} t={t} />
      case 'elements': return <StepElements draft={draft} onChange={setDraft} t={t} />
    }
  }

  return (
    <FloatingWindow
      title={<span className="flex items-center gap-2"><BarChart3 size={16} /> {t('sheet_chart_wizard_title', { defaultValue: 'Assistant de diagramme' })}</span>}
      onClose={onCancel} defaultWidth={880} backdrop>
      <div className="flex" style={{ height: 420 }}>
        {/* Roadmap (free navigation, like the Calc wizard) */}
        <div className="w-44 shrink-0 border-r border-border py-2">
          <div className="px-3 pb-1.5 text-[10px] uppercase tracking-wide text-text-tertiary">{t('sheet_chart_steps', { defaultValue: 'Étapes' })}</div>
          {STEPS.map((s, i) => (
            <button key={s.id} type="button" onClick={() => setStep(i)}
              className={`w-full text-left px-3 py-1.5 flex items-center gap-2 text-xs ${i === step ? 'bg-[#e8f0fe] text-primary' : 'hover:bg-[#f1f3f4]'}`}>
              <span className={`w-4 h-4 rounded-full border flex items-center justify-center text-[9px] shrink-0 ${i === step ? 'border-primary text-primary' : 'border-[#dadce0] text-text-tertiary'}`}>{i + 1}</span>
              {t(s.labelKey, { defaultValue: s.labelDefault })}
            </button>
          ))}
        </div>
        {/* Current step */}
        <div className="flex-1 min-w-0 overflow-auto">{stepBody()}</div>
        {/* Live preview */}
        <div className="w-[252px] shrink-0 border-l border-border p-2 flex flex-col gap-1.5">
          <div className="text-[10px] uppercase tracking-wide text-text-tertiary px-1">{t('sheet_chart_preview', { defaultValue: 'Aperçu' })}</div>
          <div className="border border-border rounded bg-surface-0 flex-none overflow-hidden">
            <WizardPreview chart={previewChart} getCell={getCell} width={234} height={180} t={t} />
          </div>
        </div>
      </div>
      <div className="flex items-center justify-between border-t border-border px-3 py-2.5">
        <div className="flex gap-2">
          <Button className={DLG_BTN} variant="ghost" disabled={step === 0} onClick={() => setStep(s => Math.max(s - 1, 0))}>
            {t('sheet_chart_back', { defaultValue: 'Précédent' })}
          </Button>
          <Button className={DLG_BTN} variant="ghost" disabled={step === STEPS.length - 1} onClick={() => setStep(s => Math.min(s + 1, STEPS.length - 1))}>
            {t('sheet_chart_next', { defaultValue: 'Suivant' })}
          </Button>
        </div>
        <div className="flex gap-2">
          <Button className={DLG_BTN} onClick={() => onConfirm(draft)}>{t('sheet_chart_finish', { defaultValue: 'Terminer' })}</Button>
          <Button className={DLG_BTN} variant="ghost" onClick={onCancel}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
