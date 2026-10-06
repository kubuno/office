// Word's « Options de la table des matières » sub-dialog: which SOURCES feed the
// table — paragraph styles (with a level per style), outline levels, and TC
// fields — plus a « Rétablir » that puts the defaults back.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Checkbox, FloatingWindow } from '@ui'

import { DLG_BTN } from '../../lib'
import type { TocSettings } from './types'

export interface TocOptionsDialogProps {
  /** Style names offered in the grid, in the document's own order. */
  styleNames: string[]
  settings: TocSettings
  onApply: (patch: Pick<TocSettings, 'fromStyles' | 'fromOutline' | 'fromFields' | 'styleLevels'>) => void
  onClose: () => void
}

/** Word's built-in mapping: Titre 1..9 → level 1..9, everything else empty. */
function defaultLevels(styleNames: string[]): Record<string, number> {
  const out: Record<string, number> = {}
  for (const name of styleNames) {
    const m = /^(?:titre|heading)\s*([1-9])$/i.exec(name.trim())
    if (m) out[name] = Number(m[1])
  }
  return out
}

export function TocOptionsDialog({ styleNames, settings, onApply, onClose }: TocOptionsDialogProps) {
  const { t } = useTranslation('office')
  const [fromStyles, setFromStyles] = useState(settings.fromStyles)
  const [fromOutline, setFromOutline] = useState(settings.fromOutline)
  const [fromFields, setFromFields] = useState(settings.fromFields)
  const [levels, setLevels] = useState<Record<string, number>>(
    Object.keys(settings.styleLevels).length ? settings.styleLevels : defaultLevels(styleNames),
  )

  const setLevel = (name: string, raw: string) => {
    const v = raw.trim()
    setLevels(prev => {
      const next = { ...prev }
      const n = Number(v)
      if (!v || !Number.isFinite(n) || n < 1 || n > 9) delete next[name]
      else next[name] = Math.trunc(n)
      return next
    })
  }

  return (
    <FloatingWindow
      title={t('doc_toc_options_title', { defaultValue: 'Options de la table des matières' })}
      onClose={onClose} defaultWidth={460} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <div className="text-text-secondary">
          {t('doc_toc_options_build', { defaultValue: 'Construire la table des matières à partir de :' })}
        </div>

        <label className="flex items-center gap-2">
          <Checkbox checked={fromStyles} onChange={setFromStyles} />
          <span>{t('doc_toc_options_styles', { defaultValue: 'Styles' })}</span>
        </label>

        <div className={fromStyles ? '' : 'opacity-40 pointer-events-none'}>
          <div className="flex items-baseline justify-between px-1 pb-1">
            <span className="text-text-secondary">{t('doc_toc_options_available', { defaultValue: 'Styles disponibles :' })}</span>
            <span className="text-text-secondary">{t('doc_toc_options_level', { defaultValue: 'Niveau :' })}</span>
          </div>
          <div className="border border-border rounded-[--kb-radius-sm] max-h-[190px] overflow-auto divide-y divide-border">
            {styleNames.map(name => (
              <div key={name} className="flex items-center justify-between gap-3 px-2 py-1">
                <span className="truncate">{name}</span>
                <input
                  type="text" inputMode="numeric"
                  className="w-[56px] h-7 px-2 rounded-[--kb-radius-sm] border border-border bg-surface text-center outline-none focus:ring-2 focus:ring-primary"
                  value={levels[name] != null ? String(levels[name]) : ''}
                  onChange={e => setLevel(name, e.target.value)}
                  aria-label={`${name} — ${t('doc_toc_options_level', { defaultValue: 'Niveau :' })}`} />
              </div>
            ))}
          </div>
        </div>

        <label className="flex items-center gap-2">
          <Checkbox checked={fromOutline} onChange={setFromOutline} />
          <span>{t('doc_toc_options_outline', { defaultValue: 'Niveaux hiérarchiques' })}</span>
        </label>
        <label className="flex items-center gap-2">
          <Checkbox checked={fromFields} onChange={setFromFields} />
          <span>{t('doc_toc_options_fields', { defaultValue: "Champs d'entrée de table" })}</span>
        </label>

        <div className="flex items-center justify-between pt-1">
          <Button className={DLG_BTN} variant="secondary" size="sm"
            onClick={() => { setFromStyles(true); setFromOutline(true); setFromFields(false); setLevels(defaultLevels(styleNames)) }}>
            {t('doc_toc_options_reset', { defaultValue: 'Rétablir' })}
          </Button>
          <div className="flex gap-2">
            <Button className={DLG_BTN} variant="primary" size="sm"
              onClick={() => { onApply({ fromStyles, fromOutline, fromFields, styleLevels: levels }); onClose() }}>
              {t('common_ok', { defaultValue: 'OK' })}
            </Button>
            <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
              {t('common_cancel', { defaultValue: 'Annuler' })}
            </Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}

export default TocOptionsDialog
