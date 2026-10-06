// Bouton « Partager » de la barre de titre des éditeurs à ruban.
//
// ICÔNE SEULE, dans le style des boutons CIRCULAIRES qui le suivent (notifications,
// paramètres, aide, applications — le cluster `HeaderActions` en mode compact) :
// 36 × 36, entièrement arrondi, fond transparent, survol translucide, icône 18.
// Une pastille à libellé attirait l'œil plus que l'action ne le mérite et cassait
// l'alignement de la rangée. Le libellé reste dans `title`/`aria-label`, donc
// l'infobulle du shell l'affiche.
import { UserPlus } from 'lucide-react'

export function ShareButton({ onShare, label }: {
  onShare: () => void
  label: string
}) {
  return (
    <button
      onClick={onShare}
      title={label}
      aria-label={label}
      className="w-9 h-9 rounded-full flex items-center justify-center transition-colors
                 flex-shrink-0 text-white/75 hover:bg-white/15 focus:outline-none"
    >
      <UserPlus size={18} />
    </button>
  )
}
