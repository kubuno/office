// Notes du présentateur sur MOBILE : le volet fixe de 100px du desktop mangerait
// le tiers d'un écran de téléphone → feuille du bas, ouverte à la demande depuis
// le ruban (onglet Accueil, groupe Affichage).
import { useTranslation } from 'react-i18next'
import { X } from 'lucide-react'

export function MobileNotesSheet({ notes, onChange, onClose }: {
  notes: string
  onChange: (v: string) => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  return (
    <>
      <div className="fixed inset-0 z-[60] bg-black/40" style={{ animation: 'kb-sheet-fade .15s ease-out' }} onClick={onClose} />
      <div className="fixed left-0 right-0 bottom-0 z-[61] flex flex-col rounded-t-2xl bg-surface-1 shadow-2xl"
        style={{ maxHeight: '60vh', paddingBottom: 'env(safe-area-inset-bottom)', animation: 'kb-sheet-up .18s ease-out' }}>
        <div className="flex items-center justify-between px-4 pt-3 pb-2 flex-shrink-0">
          <span className="text-xs font-medium text-text-secondary uppercase tracking-wide">
            {t('pres_presenter_notes', { defaultValue: 'Notes du présentateur' })}
          </span>
          <button onClick={onClose} className="w-9 h-9 -mr-2 flex items-center justify-center rounded-full text-text-secondary active:bg-surface-2">
            <X size={18} />
          </button>
        </div>
        <textarea
          value={notes}
          onChange={e => onChange(e.target.value)}
          autoFocus
          className="mx-4 mb-4 flex-1 min-h-[8rem] rounded-lg border border-border bg-surface-0 p-3 text-xs text-text-primary resize-none focus:outline-none focus:ring-2 focus:ring-primary"
          placeholder={t('pres_notes_placeholder', { defaultValue: 'Ajoutez vos notes ici…' })}
        />
      </div>
    </>
  )
}
