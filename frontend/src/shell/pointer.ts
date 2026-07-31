// Détection du POINTEUR GROSSIER (doigt), partagée par les éditeurs à canevas
// (présentations, diagrammes…) : une poignée de 10 px est intouchable au doigt,
// les guides d'accessibilité visant 44 px. `(pointer: coarse)` est réévalué en
// direct (émulation navigateur, souris branchée sur une tablette…).
import { useEffect, useState } from 'react'

export function useCoarsePointer(): boolean {
  const [coarse, setCoarse] = useState(
    () => typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches)
  useEffect(() => {
    if (typeof window === 'undefined') return
    const mql = window.matchMedia('(pointer: coarse)')
    const on = (e: MediaQueryListEvent) => setCoarse(e.matches)
    setCoarse(mql.matches)
    mql.addEventListener('change', on)
    return () => mql.removeEventListener('change', on)
  }, [])
  return coarse
}
