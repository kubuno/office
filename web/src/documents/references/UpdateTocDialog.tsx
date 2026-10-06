// Word's « Mettre à jour la table des matières » prompt: page numbers only, or
// the whole table. The distinction is real — updating page numbers keeps entry
// texts the user may have edited by hand, which a full rebuild would discard.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, FloatingWindow, Radio } from '@ui'

import { DLG_BTN } from '../../lib'

export type TocUpdateMode = 'pages' | 'all'

export interface UpdateTocDialogProps {
  onApply: (mode: TocUpdateMode) => void
  onClose: () => void
}

export function UpdateTocDialog({ onApply, onClose }: UpdateTocDialogProps) {
  const { t } = useTranslation('office')
  const label = (k: string, d: string) => t(k, { defaultValue: d })
  const [mode, setMode] = useState<TocUpdateMode>('pages')

  return (
    <FloatingWindow title={label('doc_toc_update_title', 'Mettre à jour la table des matières')}
      onClose={onClose} defaultWidth={420} backdrop>
      <div className="flex flex-col gap-3" data-module="office">
        <div className="text-text-secondary">
          {label('doc_toc_update_hint', 'La table des matières va être mise à jour. Sélectionnez une des options suivantes :')}
        </div>
        <Radio checked={mode === 'pages'} onChange={() => setMode('pages')}
          label={label('doc_toc_update_pages', 'Mettre à jour les numéros de page uniquement')} />
        <Radio checked={mode === 'all'} onChange={() => setMode('all')}
          label={label('doc_toc_update_all', 'Mettre à jour toute la table')} />
        <div className="flex justify-end gap-2 pt-1">
          <Button className={DLG_BTN} variant="primary" size="sm" onClick={() => { onApply(mode); onClose() }}>
            {t('common_ok', { defaultValue: 'OK' })}
          </Button>
          <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>
            {t('common_cancel', { defaultValue: 'Annuler' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

export default UpdateTocDialog
