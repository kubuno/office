// Word's « Table des matières » window (Références → Table des matières
// personnalisée…): ONE window with four tabs — Index, Table des matières,
// Table des illustrations, Table des références — two preview panes, and the
// « Options… » / « Modifier… » sub-windows.
//
// The tab the user validates decides WHICH table is generated: OK on the
// « Table des illustrations » tab inserts a table of figures, not a TOC.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Checkbox, Dropdown, FloatingWindow, NumberInput, Radio, Tabs } from '@ui'

import { DLG_BTN } from '../../lib'
import { FORMAT_LABELS, LEADER_LABELS } from './formats'
import { PrintPreview, WebPreview, type PreviewRow } from './PreviewPanes'
import { TableStyleDialog } from './TableStyleDialog'
import { TocOptionsDialog } from './TocOptionsDialog'
import {
  AUTHORITY_CATEGORIES, TABLE_FORMATS,
  type AuthoritiesSettings, type AuthorityCategory, type FiguresSettings, type IndexSettings,
  type LeaderKind, type ReferencesSettings, type TableFormat, type TableKind, type TocSettings,
} from './types'

export interface ReferencesDialogProps {
  /** Tab shown first — the ribbon opens the one matching the button clicked. */
  initialTab?: TableKind
  settings: ReferencesSettings
  /** Style names offered by the « Options… » sub-dialog. */
  styleNames: string[]
  /** Caption labels found in the document (« Figure », « Tableau »…). */
  captionLabels: string[]
  onApply: (kind: TableKind, settings: ReferencesSettings) => void
  onClose: () => void
}

const TOC_ROWS: PreviewRow[] = [
  { text: 'Titre 1', level: 1, page: '1' },
  { text: 'Titre 2', level: 2, page: '3' },
  { text: 'Titre 3', level: 3, page: '5' },
]

export function ReferencesDialog({
  initialTab = 'toc', settings, styleNames, captionLabels, onApply, onClose,
}: ReferencesDialogProps) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<TableKind>(initialTab)
  const [toc, setToc] = useState<TocSettings>(settings.toc)
  const [figures, setFigures] = useState<FiguresSettings>(settings.figures)
  const [index, setIndex] = useState<IndexSettings>(settings.index)
  const [authorities, setAuthorities] = useState<AuthoritiesSettings>(settings.authorities)
  const [optionsOpen, setOptionsOpen] = useState(false)
  const [styleOpen, setStyleOpen] = useState(false)

  const common = tab === 'toc' ? toc : tab === 'figures' ? figures : tab === 'index' ? index : authorities
  const patchCommon = (p: Partial<typeof common>) => {
    if (tab === 'toc') setToc(s => ({ ...s, ...p }))
    else if (tab === 'figures') setFigures(s => ({ ...s, ...p }))
    else if (tab === 'index') setIndex(s => ({ ...s, ...p }))
    else setAuthorities(s => ({ ...s, ...p }))
  }

  const figureRows: PreviewRow[] = [1, 2, 3].map(n => ({
    level: 1, page: String(n * 2 - 1),
    text: figures.includeLabel ? `${figures.label} ${n} : ${t('doc_tof_sample', { defaultValue: 'Légende' })}` : t('doc_tof_sample', { defaultValue: 'Légende' }),
  }))
  const indexRows: PreviewRow[] = [
    { text: 'Aristote', level: 1, page: '2' },
    { text: index.type === 'indented' ? 'Astronomie' : 'Astronomie, 4', level: index.type === 'indented' ? 2 : 1, page: '4' },
    { text: 'Terre', level: 1, page: '5, 7' },
  ]
  const authorityRows: PreviewRow[] = [
    { text: 'Baldwin v. Alabama, 472 U.S. 372 (1985)', level: 1, page: authorities.usePassim ? 'passim' : '5, 6, 9' },
    { text: 'Forsyth v. Hammond, 166 U.S. 506 (1897)', level: 1, page: '7' },
  ]
  const rows = tab === 'toc' ? TOC_ROWS : tab === 'figures' ? figureRows : tab === 'index' ? indexRows : authorityRows

  const label = (k: string, d: string) => t(k, { defaultValue: d })

  return (
    <FloatingWindow title={label('doc_toc', 'Table des matières')} onClose={onClose} defaultWidth={720} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <Tabs size="sm" value={tab} onChange={v => setTab(v as TableKind)}
          tabs={[
            { id: 'index', label: label('doc_tab_index', 'Index') },
            { id: 'toc', label: label('doc_toc', 'Table des matières') },
            { id: 'figures', label: label('doc_tab_figures', 'Table des illustrations') },
            { id: 'authorities', label: label('doc_tab_authorities', 'Table des références') },
          ]} />

        <div className="flex gap-4">
          <PrintPreview label={label('doc_preview_print', 'Aperçu avant impression')} rows={rows} settings={common} />
          {tab === 'toc'
            ? <WebPreview label={label('doc_preview_web', 'Aperçu web')} rows={rows} hyperlinks={toc.useHyperlinks} format={toc.format} />
            : <WebPreview label={label('doc_preview_web', 'Aperçu web')} rows={rows} hyperlinks format={common.format} />}
        </div>

        <div className="flex gap-6">
          {/* Left column: what is printed. */}
          <div className="flex-1 min-w-0 flex flex-col gap-2">
            <label className="flex items-center gap-2">
              <Checkbox checked={common.showPageNumbers} onChange={v => patchCommon({ showPageNumbers: v })} />
              <span>{label('doc_toc_pagenums', 'Afficher les numéros de page')}</span>
            </label>
            <label className="flex items-center gap-2">
              <Checkbox checked={common.rightAlign} disabled={!common.showPageNumbers}
                onChange={v => patchCommon({ rightAlign: v })} />
              <span>{label('doc_toc_right_align', 'Aligner les numéros de page à droite')}</span>
            </label>
            <label className="flex items-center gap-2">
              <span className="w-[130px] text-text-secondary">{label('doc_toc_leader_chars', 'Caractères de suite :')}</span>
              <Dropdown width={150} height={32} value={common.leader} disabled={!common.rightAlign || !common.showPageNumbers}
                options={(['none', 'dots', 'dashes', 'underline'] as LeaderKind[]).map(k => ({ value: k, label: LEADER_LABELS[k] }))}
                onChange={v => patchCommon({ leader: v as LeaderKind })} />
            </label>

            {tab === 'figures' && (
              <>
                <label className="flex items-center gap-2">
                  <span className="w-[130px] text-text-secondary">{label('doc_tof_label', 'Légende :')}</span>
                  <Dropdown width={150} height={32} value={figures.label}
                    options={(captionLabels.length ? captionLabels : ['Figure', 'Tableau', 'Équation']).map(l => ({ value: l, label: l }))}
                    onChange={v => setFigures(s => ({ ...s, label: v }))} />
                </label>
                <label className="flex items-center gap-2">
                  <Checkbox checked={figures.includeLabel} onChange={v => setFigures(s => ({ ...s, includeLabel: v }))} />
                  <span>{label('doc_tof_include_label', "Inclure l'étiquette et le numéro")}</span>
                </label>
              </>
            )}

            {tab === 'index' && (
              <>
                <div className="flex items-center gap-4">
                  <span className="w-[130px] text-text-secondary">{label('doc_index_type', 'Type :')}</span>
                  <Radio checked={index.type === 'indented'} onChange={() => setIndex(s => ({ ...s, type: 'indented' }))}
                    label={label('doc_index_indented', 'En retrait')} />
                  <Radio checked={index.type === 'runin'} onChange={() => setIndex(s => ({ ...s, type: 'runin' }))}
                    label={label('doc_index_runin', 'Sur le même niveau')} />
                </div>
                <label className="flex items-center gap-2">
                  <span className="w-[130px] text-text-secondary">{label('doc_index_columns', 'Colonnes :')}</span>
                  <NumberInput className="w-[80px] h-8" min={1} max={4} step={1} value={index.columns}
                    onChange={n => setIndex(s => ({ ...s, columns: Math.max(1, Math.min(4, Math.round(n))) }))} />
                </label>
              </>
            )}

            {tab === 'authorities' && (
              <>
                <label className="flex items-center gap-2">
                  <span className="w-[130px] text-text-secondary">{label('doc_toa_category', 'Catégorie :')}</span>
                  <Dropdown width={190} height={32} value={authorities.category}
                    options={AUTHORITY_CATEGORIES.map(c => ({ value: c, label: label('doc_toa_cat_' + c, CATEGORY_FR[c]) }))}
                    onChange={v => setAuthorities(s => ({ ...s, category: v as AuthorityCategory }))} />
                </label>
                <label className="flex items-center gap-2">
                  <Checkbox checked={authorities.usePassim} onChange={v => setAuthorities(s => ({ ...s, usePassim: v }))} />
                  <span>{label('doc_toa_passim', 'Utiliser passim')}</span>
                </label>
                <label className="flex items-center gap-2">
                  <Checkbox checked={authorities.keepFormatting} onChange={v => setAuthorities(s => ({ ...s, keepFormatting: v }))} />
                  <span>{label('doc_toa_keep_format', "Conserver la mise en forme d'origine")}</span>
                </label>
              </>
            )}
          </div>

          {/* Right column: the web behaviour, TOC only (as in Word). */}
          <div className="flex-1 min-w-0">
            {tab === 'toc' && (
              <label className="flex items-start gap-2">
                <Checkbox checked={toc.useHyperlinks} onChange={v => setToc(s => ({ ...s, useHyperlinks: v }))} />
                <span>{label('doc_toc_hyperlinks', 'Utiliser des liens hypertexte à la place des numéros de page')}</span>
              </label>
            )}
          </div>
        </div>

        {/* Général */}
        <div className="border-t border-border pt-3">
          <div className="text-text-secondary mb-2">{label('doc_toc_general', 'Général')}</div>
          <div className="flex items-center gap-3 flex-wrap">
            <span className="w-[130px] text-text-secondary">{label('doc_toc_formats', 'Formats :')}</span>
            <Dropdown width={190} height={32} value={common.format}
              options={TABLE_FORMATS.map(f => ({ value: f, label: label('doc_fmt_' + f, FORMAT_LABELS[f]) }))}
              onChange={v => patchCommon({ format: v as TableFormat })} />
          </div>
          {tab === 'toc' && (
            <div className="flex items-center gap-3 mt-2">
              <span className="w-[130px] text-text-secondary">{label('doc_toc_levels', 'Afficher les niveaux :')}</span>
              <NumberInput className="w-[80px] h-8" min={1} max={9} step={1} value={toc.levels}
                onChange={n => setToc(s => ({ ...s, levels: Math.max(1, Math.min(9, Math.round(n))) }))} />
            </div>
          )}
        </div>

        <div className="flex items-center justify-between pt-1">
          <div className="flex gap-2">
            {tab === 'toc' && (
              <Button className={DLG_BTN} variant="secondary" size="sm" onClick={() => setOptionsOpen(true)}>
                {label('doc_toc_options', 'Options…')}
              </Button>
            )}
            <Button className={DLG_BTN} variant="secondary" size="sm" onClick={() => setStyleOpen(true)}>
              {label('doc_toc_modify', 'Modifier…')}
            </Button>
          </div>
          <div className="flex gap-2">
            <Button className={DLG_BTN} variant="primary" size="sm"
              onClick={() => { onApply(tab, { toc, figures, index, authorities }); onClose() }}>
              {t('common_ok', { defaultValue: 'OK' })}
            </Button>
            <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
              {t('common_cancel', { defaultValue: 'Annuler' })}
            </Button>
          </div>
        </div>
      </div>

      {optionsOpen && (
        <TocOptionsDialog styleNames={styleNames} settings={toc}
          onApply={patch => setToc(s => ({ ...s, ...patch }))}
          onClose={() => setOptionsOpen(false)} />
      )}
      {styleOpen && (
        <TableStyleDialog
          stylePrefix={tab === 'toc' ? 'TM' : tab === 'figures' ? 'TI' : tab === 'index' ? 'Index' : 'TR'}
          levelCount={tab === 'toc' ? 9 : 3}
          format={common.format}
          levelStyles={common.levelStyles}
          onApply={ls => patchCommon({ levelStyles: ls })}
          onClose={() => setStyleOpen(false)} />
      )}
    </FloatingWindow>
  )
}

const CATEGORY_FR: Record<AuthorityCategory, string> = {
  all: 'Toutes', cases: 'Cas', statutes: 'Statuts', other: 'Autres autorités',
  rules: 'Règles', treatises: 'Traités', regulations: 'Règlements',
  constitutional: 'Dispositions constitutionnelles',
}

export default ReferencesDialog
