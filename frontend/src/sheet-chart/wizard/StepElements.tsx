// Wizard step 4 — chart elements (Calc tp_Wizard_TitlesAndObjects): titles,
// legend visibility + position, major gridlines, data labels.

import { Checkbox } from '@ui'
import type { SheetChart } from '../../api'
import { normalizeKind, isCartesian } from '../normalize'
import { Field, Seg, WIZ_INPUT } from './controls'
import type { WizardTranslate } from './state'

export function StepElements({ draft, onChange, t }: {
  draft: SheetChart
  onChange: (c: SheetChart) => void
  t: WizardTranslate
}) {
  const kind = normalizeKind(draft.type)
  const cartesian = isCartesian(kind)
  const legendOn = draft.legend !== false
  const setAxis = (axis: 'axisX' | 'axisY', title: string) =>
    onChange({ ...draft, [axis]: { ...draft[axis], title: title || undefined } })

  return (
    <div className="p-3 space-y-3">
      <Field label={t('sheet_chart_title', { defaultValue: 'Titre' })}>
        <input value={draft.title ?? ''} onChange={e => onChange({ ...draft, title: e.target.value || undefined })}
          onKeyDown={e => e.stopPropagation()} className={WIZ_INPUT} />
      </Field>
      {cartesian && (<>
        <Field label={t('sheet_chart_x_axis_title', { defaultValue: 'Titre de l’axe X' })}>
          <input value={draft.axisX?.title ?? ''} onChange={e => setAxis('axisX', e.target.value)}
            onKeyDown={e => e.stopPropagation()} className={WIZ_INPUT} />
        </Field>
        <Field label={t('sheet_chart_y_axis_title', { defaultValue: 'Titre de l’axe Y' })}>
          <input value={draft.axisY?.title ?? ''} onChange={e => setAxis('axisY', e.target.value)}
            onKeyDown={e => e.stopPropagation()} className={WIZ_INPUT} />
        </Field>
      </>)}
      <div className="border-t border-border pt-3 space-y-2">
        <Checkbox checked={legendOn} onChange={v => onChange({ ...draft, legend: v ? true : false })}
          label={t('sheet_chart_show_legend', { defaultValue: 'Afficher la légende' })} />
        <Field label={t('sheet_chart_legend_pos', { defaultValue: 'Position' })}>
          <Seg
            options={[
              { value: 'left', label: t('sheet_chart_pos_left', { defaultValue: 'Gauche' }) },
              { value: 'right', label: t('sheet_chart_pos_right', { defaultValue: 'Droite' }) },
              { value: 'top', label: t('sheet_chart_pos_top', { defaultValue: 'Haut' }) },
              { value: 'bottom', label: t('sheet_chart_pos_bottom', { defaultValue: 'Bas' }) },
            ]}
            value={draft.legendPos ?? 'right'}
            onChange={v => onChange({ ...draft, legendPos: v })}
            disabled={!legendOn}
          />
        </Field>
      </div>
      {cartesian && (
        <div className="border-t border-border pt-3">
          <Field label={t('sheet_chart_grids', { defaultValue: 'Grilles' })}>
            <div className="flex items-center gap-4">
              <Checkbox checked={draft.grid?.x ?? false} onChange={v => onChange({ ...draft, grid: { ...draft.grid, x: v } })}
                label={t('sheet_chart_x_axis', { defaultValue: 'Axe X' })} />
              <Checkbox checked={draft.grid?.y ?? true} onChange={v => onChange({ ...draft, grid: { ...draft.grid, y: v } })}
                label={t('sheet_chart_y_axis', { defaultValue: 'Axe Y' })} />
            </div>
          </Field>
        </div>
      )}
      <div className="border-t border-border pt-3">
        <Field label={t('sheet_chart_data_labels', { defaultValue: 'Étiquettes de données' })}>
          <Seg
            options={[
              { value: 'none', label: t('sheet_chart_dl_none', { defaultValue: 'Aucune' }) },
              { value: 'value', label: t('sheet_chart_dl_value', { defaultValue: 'Valeur' }) },
              { value: 'percent', label: t('sheet_chart_dl_percent', { defaultValue: 'Pourcentage' }) },
            ]}
            value={draft.dataLabels ?? 'none'}
            onChange={v => onChange({ ...draft, dataLabels: v === 'none' ? undefined : v })}
          />
        </Field>
      </div>
    </div>
  )
}
