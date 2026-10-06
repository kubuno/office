import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { DLG_BTN } from './lib'
import { FloatingWindow, Button, Checkbox, Dropdown } from '@ui'
import { Printer } from 'lucide-react'
import { PAGE_FORMATS } from './mathPages'
import { buildSheetPrintDoc, printSheet, type PrintGrid, type PrintOptions, type PrintScaleMode } from './sheetPrint'

interface Props {
  /** Build the print grid for a given area choice — recomputed when the area changes. */
  buildGrid: (area: 'selection' | 'used') => PrintGrid | null
  hasSelection: boolean
  initial: PrintOptions
  onClose: () => void
}

export default function PrintDialog({ buildGrid, hasSelection, initial, onClose }: Props) {
  const { t } = useTranslation('office')
  const [opt, setOpt] = useState<PrintOptions>(initial)
  const [area, setArea] = useState<'selection' | 'used'>(hasSelection ? 'selection' : 'used')

  const grid = useMemo(() => buildGrid(area), [area, buildGrid])
  const previewDoc = useMemo(() => grid ? buildSheetPrintDoc(grid, opt, true) : '', [grid, opt])

  const set = <K extends keyof PrintOptions>(k: K, v: PrintOptions[K]) => setOpt(o => ({ ...o, [k]: v }))
  const sel = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary w-full'
  const lbl = 'text-xs font-medium text-text-secondary mb-1'

  const doPrint = () => { if (grid) printSheet(grid, opt) }

  const scaleModes: { id: PrintScaleMode; label: string }[] = [
    { id: 'normal',   label: t('pr_scale_normal', { defaultValue: 'Taille réelle' }) },
    { id: 'fitWidth', label: t('pr_scale_fitw', { defaultValue: 'Ajuster à la largeur' }) },
    { id: 'fitPage',  label: t('pr_scale_fitp', { defaultValue: 'Ajuster à la page' }) },
    { id: 'custom',   label: t('pr_scale_custom', { defaultValue: 'Personnalisée' }) },
  ]

  return (
    <FloatingWindow
      title={t('pr_title', { defaultValue: 'Imprimer' })}
      icon={<Printer size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={860} defaultHeight={620} minWidth={720} minHeight={460}
    >
      <div className="flex h-full text-sm" data-module="office">
        {/* Panneau d'options */}
        <div className="w-64 shrink-0 overflow-auto pr-3 space-y-3 border-r border-border">
          <div>
            <div className={lbl}>{t('pr_area', { defaultValue: 'Zone à imprimer' })}</div>
            <Dropdown className="w-full" value={area} onChange={v => setArea(v as 'selection' | 'used')}
              options={[...(hasSelection ? [{ value: 'selection', label: t('pr_area_sel', { defaultValue: 'Sélection' }) }] : []),
                        { value: 'used', label: t('pr_area_used', { defaultValue: 'Feuille entière' }) }]} />
          </div>
          <div>
            <div className={lbl}>{t('pr_orientation', { defaultValue: 'Orientation' })}</div>
            <div className="flex gap-1">
              {(['portrait', 'landscape'] as const).map(o => (
                <button key={o} onClick={() => set('orientation', o)}
                  className={`flex-1 h-8 rounded border text-xs ${opt.orientation === o ? 'border-primary bg-primary-light text-primary' : 'border-border'}`}>
                  {o === 'portrait' ? t('pr_portrait', { defaultValue: 'Portrait' }) : t('pr_landscape', { defaultValue: 'Paysage' })}
                </button>
              ))}
            </div>
          </div>
          <div>
            <div className={lbl}>{t('pr_paper', { defaultValue: 'Format papier' })}</div>
            <Dropdown className="w-full" value={opt.paper} onChange={v => set('paper', v)}
              options={PAGE_FORMATS.map(f => ({ value: f.id, label: f.name }))} />
          </div>
          <div>
            <div className={lbl}>{t('pr_scale', { defaultValue: 'Mise à l’échelle' })}</div>
            <Dropdown className="w-full" value={opt.scaleMode} onChange={v => set('scaleMode', v as PrintScaleMode)}
              options={scaleModes.map(m => ({ value: m.id, label: m.label }))} />
            {opt.scaleMode === 'custom' && (
              <div className="flex items-center gap-2 mt-1">
                <input type="number" min={10} max={400} value={opt.scalePct}
                  onChange={e => set('scalePct', Math.max(10, Math.min(400, +e.target.value || 100)))}
                  onKeyDown={e => e.stopPropagation()}
                  className="h-8 px-2 border border-border rounded w-20 outline-none focus:border-primary" />
                <span className="text-text-secondary">%</span>
              </div>
            )}
          </div>
          <div>
            <div className={lbl}>{t('pr_margin', { defaultValue: 'Marges (mm)' })}</div>
            <input type="number" min={0} max={40} value={opt.marginMm}
              onChange={e => set('marginMm', Math.max(0, Math.min(40, +e.target.value || 0)))}
              onKeyDown={e => e.stopPropagation()}
              className="h-8 px-2 border border-border rounded w-24 outline-none focus:border-primary" />
          </div>
          <div>
            <div className={lbl}>{t('pr_repeat', { defaultValue: 'Lignes de titre répétées' })}</div>
            <input type="number" min={0} max={10} value={opt.repeatRows}
              onChange={e => set('repeatRows', Math.max(0, Math.min(10, +e.target.value || 0)))}
              onKeyDown={e => e.stopPropagation()}
              className="h-8 px-2 border border-border rounded w-24 outline-none focus:border-primary" />
          </div>
          <div className="space-y-1.5 pt-1">
            <label className="flex items-center gap-2 text-xs cursor-pointer">
              <Checkbox checked={opt.gridlines} onChange={v => set('gridlines', v)} />
              {t('pr_gridlines', { defaultValue: 'Quadrillage' })}
            </label>
            <label className="flex items-center gap-2 text-xs cursor-pointer">
              <Checkbox checked={opt.headings} onChange={v => set('headings', v)} />
              {t('pr_headings', { defaultValue: 'En-têtes de lignes et colonnes' })}
            </label>
            <label className="flex items-center gap-2 text-xs cursor-pointer">
              <Checkbox checked={opt.centerH} onChange={v => set('centerH', v)} />
              {t('pr_center', { defaultValue: 'Centrer horizontalement' })}
            </label>
          </div>
        </div>

        {/* Aperçu */}
        <div className="flex-1 min-w-0 flex flex-col">
          <div className="flex-1 min-h-0 bg-surface-3 rounded-r overflow-hidden ml-3">
            {grid && grid.rows.length
              ? <iframe title="print-preview" className="w-full h-full border-0 bg-[#e8eaed]" srcDoc={previewDoc} />
              : <div className="w-full h-full flex items-center justify-center text-text-tertiary text-xs">{t('pr_empty', { defaultValue: 'Rien à imprimer dans cette zone.' })}</div>}
          </div>
          <div className="flex justify-end gap-2 pt-3 pl-3">
            <Button className={DLG_BTN} variant="primary" disabled={!grid || !grid.rows.length} onClick={doPrint}>
              <Printer size={15} className="mr-1" /> {t('pr_print', { defaultValue: 'Imprimer' })}
            </Button>
            <Button className={DLG_BTN} variant="ghost" onClick={onClose}>{t('pr_cancel', { defaultValue: 'Annuler' })}</Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}
