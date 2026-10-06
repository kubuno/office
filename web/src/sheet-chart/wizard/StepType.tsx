// Wizard step 1 — chart type & subtype gallery (Calc tp_ChartType).

import { Checkbox } from '@ui'
import type { SheetChart } from '../../api'
import { CHART_CATALOG, applySubtype, catalogTypeOf, subtypeOf } from './catalog'
import { ChartThumb } from './thumbs'
import type { WizardTranslate } from './state'

export function StepType({ draft, onChange, t }: {
  draft: SheetChart
  onChange: (c: SheetChart) => void
  t: WizardTranslate
}) {
  const def = catalogTypeOf(draft)
  const sub = subtypeOf(draft, def)
  return (
    <div className="flex h-full min-h-0">
      <div className="w-40 shrink-0 border-r border-border overflow-auto py-1">
        {CHART_CATALOG.map(d => (
          <button key={d.id} type="button"
            onClick={() => { if (d.id !== def.id) onChange(applySubtype(draft, d.subtypes[0])) }}
            className={`w-full text-left px-3 py-1.5 flex items-center gap-2 text-xs ${d.id === def.id ? 'bg-[#e8f0fe] text-primary' : 'hover:bg-[#f1f3f4]'}`}>
            <ChartThumb typeId={d.id} subtypeId={d.subtypes[0].id} width={28} height={20} />
            {t(d.labelKey, { defaultValue: d.labelDefault })}
          </button>
        ))}
      </div>
      <div className="flex-1 min-w-0 p-3 overflow-auto">
        <div className="text-xs text-text-secondary mb-2">{t(def.labelKey, { defaultValue: def.labelDefault })}</div>
        <div className="flex flex-wrap gap-2">
          {def.subtypes.map(s => (
            <button key={s.id} type="button" onClick={() => onChange(applySubtype(draft, s))}
              title={t(s.labelKey, { defaultValue: s.labelDefault })}
              className={`flex flex-col items-center gap-1 p-2 rounded border ${s.id === sub.id ? 'border-primary bg-[#e8f0fe]' : 'border-[#dadce0] hover:bg-[#f1f3f4]'}`}>
              <ChartThumb typeId={def.id} subtypeId={s.id} width={72} height={50} />
              <span className="text-[10px] max-w-[80px] truncate">{t(s.labelKey, { defaultValue: s.labelDefault })}</span>
            </button>
          ))}
        </div>
        {def.smoothOption && (
          <div className="mt-4">
            <Checkbox checked={!!draft.smooth} onChange={v => onChange({ ...draft, smooth: v || undefined })}
              label={t('sheet_chart_smooth', { defaultValue: 'Lignes lisses' })} />
          </div>
        )}
      </div>
    </div>
  )
}
