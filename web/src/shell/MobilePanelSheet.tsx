// Feuille du bas générique pour les PANNEAUX d'un éditeur à canevas sur mobile
// (bibliothèque de formes, propriétés, calques…). Sur desktop ces panneaux vivent
// dans la zone de docking ; sur un téléphone, la même arborescence est rendue
// telle quelle dans une feuille, sans dupliquer le contenu.
import type { ReactNode } from 'react'
import { X } from 'lucide-react'

export function MobilePanelSheet({ title, onClose, children, height = '62vh' }: {
  title: ReactNode
  onClose: () => void
  children: ReactNode
  /** Hauteur de la feuille (une bibliothèque de formes en veut plus qu'un formulaire). */
  height?: string
}) {
  return (
    <>
      <div className="fixed inset-0 z-[60] bg-black/40" style={{ animation: 'kb-sheet-fade .15s ease-out' }} onClick={onClose} />
      <div className="fixed left-0 right-0 bottom-0 z-[61] flex flex-col rounded-t-2xl bg-surface-1 shadow-2xl overflow-hidden"
        style={{ height, paddingBottom: 'env(safe-area-inset-bottom)', animation: 'kb-sheet-up .18s ease-out' }}>
        <div className="flex items-center justify-between px-4 pt-3 pb-2 flex-shrink-0 border-b border-border">
          <span className="text-xs font-medium text-text-secondary uppercase tracking-wide">{title}</span>
          <button onClick={onClose} className="w-9 h-9 -mr-2 flex items-center justify-center rounded-full text-text-secondary active:bg-surface-2">
            <X size={18} />
          </button>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto">{children}</div>
      </div>
    </>
  )
}
