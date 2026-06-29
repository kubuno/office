import { useEffect, useReducer, useRef } from 'react'

// Historique Annuler/Rétablir générique par SNAPSHOTS, pour les éditeurs dont l'état
// est un objet sérialisable unique mis à jour via un seul setter (Données/Projet/Maths
// — pas de Yjs/ProseMirror pour les données). Coalesce les éditions rapides (debounce)
// et se RÉ-INITIALISE sur `resetKey` (changement de document / fin de chargement) pour
// ne JAMAIS enregistrer le passage état-vide → état-chargé (sinon Annuler viderait tout).
export function useSnapshotHistory<T>(value: T, apply: (v: T) => void, resetKey: unknown, delay = 400) {
  const undoStack = useRef<T[]>([])
  const redoStack = useRef<T[]>([])
  const last = useRef<T>(value)
  const skip = useRef(false)              // ignore la prochaine variation (restauration ou rebaseline)
  const [, tick] = useReducer(x => x + 1, 0)

  // Rebaseline : nouveau document / chargement terminé → on adopte la valeur courante
  // comme référence sans rien enregistrer, et on vide les piles.
  useEffect(() => {
    undoStack.current = []
    redoStack.current = []
    last.current = value
    skip.current = true
    tick()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resetKey])

  useEffect(() => {
    if (skip.current) { skip.current = false; last.current = value; return }
    if (value === last.current) return
    const id = setTimeout(() => {
      if (value === last.current) return
      undoStack.current.push(last.current)
      if (undoStack.current.length > 100) undoStack.current.shift()
      redoStack.current = []
      last.current = value
      tick()
    }, delay)
    return () => clearTimeout(id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value])

  const undo = () => {
    if (!undoStack.current.length) return
    redoStack.current.push(last.current)
    const v = undoStack.current.pop() as T
    skip.current = true
    last.current = v
    apply(v)
    tick()
  }
  const redo = () => {
    if (!redoStack.current.length) return
    undoStack.current.push(last.current)
    const v = redoStack.current.pop() as T
    skip.current = true
    last.current = v
    apply(v)
    tick()
  }
  return { undo, redo, canUndo: undoStack.current.length > 0, canRedo: redoStack.current.length > 0 }
}
