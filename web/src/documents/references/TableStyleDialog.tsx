// Word's « Modifier… » sub-dialog: pick the style of one level of the generated
// table (TM 1..TM 9 for a table of contents) and adjust it.
//
// Word chains a second « Modifier le style » window behind a Modify button; we
// put the same four properties directly in this window instead, because the
// chain would add a click without adding a capability.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Checkbox, ColorField, Dropdown, FloatingWindow, NumberInput } from '@ui'

import { DLG_BTN } from '../../lib'
import { entryStyle } from './formats'
import type { LevelStyle, TableFormat } from './types'

const FONTS = ['Calibri', 'Arial', 'Times New Roman', 'Georgia', 'Verdana', 'Courier New', 'Garamond']

export interface TableStyleDialogProps {
  /** Prefix of the level names: « TM » for a TOC, « TI » for a table of figures. */
  stylePrefix: string
  levelCount: number
  format: TableFormat
  levelStyles: Record<number, LevelStyle>
  onApply: (levelStyles: Record<number, LevelStyle>) => void
  onClose: () => void
}

export function TableStyleDialog({
  stylePrefix, levelCount, format, levelStyles, onApply, onClose,
}: TableStyleDialogProps) {
  const { t } = useTranslation('office')
  const [sel, setSel] = useState(1)
  const [draft, setDraft] = useState<Record<number, LevelStyle>>(levelStyles)

  const base = entryStyle(format, sel)
  const cur = draft[sel] ?? {}
  // What the level looks like today = the format, overridden by the level style.
  const eff = {
    fontFamily: cur.fontFamily ?? base.fontFamily ?? 'Calibri',
    fontSizePt: cur.fontSizePt ?? base.fontSizePt ?? 11,
    bold: cur.bold ?? !!base.bold,
    italic: cur.italic ?? !!base.italic,
    color: cur.color ?? base.color ?? '#202124',
  }
  const patch = (p: Partial<LevelStyle>) => setDraft(d => ({ ...d, [sel]: { ...(d[sel] ?? {}), ...p } }))

  return (
    <FloatingWindow title={t('doc_table_style_title', { defaultValue: 'Style' })}
      onClose={onClose} defaultWidth={520} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <div className="text-text-secondary">
          {t('doc_table_style_hint', { defaultValue: "Sélectionnez le style approprié pour votre entrée d'index ou de table" })}
        </div>

        <div className="flex gap-4">
          <div className="w-[170px] shrink-0">
            <div className="text-text-secondary mb-1">{t('doc_table_style_styles', { defaultValue: 'Styles :' })}</div>
            <div className="border border-border rounded-[--kb-radius-sm] h-[190px] overflow-auto">
              {Array.from({ length: levelCount }, (_, i) => i + 1).map(n => (
                <button key={n} type="button"
                  onMouseDown={e => e.preventDefault()} onClick={() => setSel(n)}
                  className={`w-full text-left px-2 py-1 ${n === sel ? 'bg-primary/15 text-text-primary' : 'hover:bg-surface-hover'}`}>
                  ¶ {stylePrefix} {n}
                </button>
              ))}
            </div>
          </div>

          <div className="flex-1 min-w-0 flex flex-col gap-2">
            <div className="text-text-secondary">{t('doc_table_style_preview', { defaultValue: 'Aperçu' })}</div>
            <div className="border border-border rounded-[--kb-radius-sm] bg-surface px-3 py-2 truncate"
              style={{
                fontFamily: eff.fontFamily, fontSize: `${eff.fontSizePt}pt`, color: eff.color,
                fontWeight: eff.bold ? 600 : 400, fontStyle: eff.italic ? 'italic' : 'normal',
              }}>
              {t('doc_toc_preview1', { defaultValue: 'Titre 1' })}
            </div>

            <label className="flex items-center gap-2">
              <span className="w-[64px] text-text-secondary">{t('doc_font', { defaultValue: 'Police' })}</span>
              <Dropdown width={190} height={32} value={eff.fontFamily}
                options={FONTS.map(f => ({ value: f, label: f }))}
                onChange={v => patch({ fontFamily: v })} />
            </label>
            <label className="flex items-center gap-2">
              <span className="w-[64px] text-text-secondary">{t('doc_size', { defaultValue: 'Taille' })}</span>
              <NumberInput className="w-[80px] h-8" min={6} max={72} step={0.5} value={eff.fontSizePt}
                onChange={n => patch({ fontSizePt: Math.max(6, Math.min(72, n)) })} />
            </label>
            <div className="flex items-center gap-4">
              <label className="flex items-center gap-2">
                <Checkbox checked={eff.bold} onChange={v => patch({ bold: v })} />
                <span>{t('doc_bold', { defaultValue: 'Gras' })}</span>
              </label>
              <label className="flex items-center gap-2">
                <Checkbox checked={eff.italic} onChange={v => patch({ italic: v })} />
                <span>{t('doc_italic', { defaultValue: 'Italique' })}</span>
              </label>
            </div>
            <label className="flex items-center gap-2">
              <span className="w-[64px] text-text-secondary">{t('doc_color', { defaultValue: 'Couleur' })}</span>
              <ColorField color={eff.color} onChange={c => patch({ color: c })} />
            </label>
          </div>
        </div>

        <div className="flex items-center justify-between pt-1">
          <Button className={DLG_BTN} variant="secondary" size="sm"
            onClick={() => setDraft(d => { const n = { ...d }; delete n[sel]; return n })}>
            {t('doc_table_style_reset', { defaultValue: 'Rétablir' })}
          </Button>
          <div className="flex gap-2">
            <Button className={DLG_BTN} variant="primary" size="sm" onClick={() => { onApply(draft); onClose() }}>
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

export default TableStyleDialog
