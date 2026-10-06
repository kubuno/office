// Undo/Redo for SERVER-BACKED editors (reports, projects…) — the ones whose every
// edit is a React Query mutation instead of a local document model.
//
// Those editors have no in-memory document to snapshot, so the history is a stack
// of INVERSE OPERATIONS: each entry knows how to undo itself and how to redo
// itself. The catch is that ids are minted by the server, so undoing a deletion
// re-creates the entity under a NEW id and every later entry still referring to
// the old id would break. The stack therefore carries an ID MAP: entries never
// capture a raw id, they resolve it through `ctx.resolve(id)` at execution time,
// and whoever re-creates an entity calls `ctx.remap(oldId, newId)`.
//
// Entries are applied strictly one at a time, and `applyingRef` is raised while an
// entry runs so the editor's own mutation wrappers know not to push a new entry
// for the change they are about to observe.
import { useCallback, useRef, useState } from 'react'

export interface HistoryCtx {
  /** Current id of an entity that an earlier undo/redo may have re-created. */
  resolve: (id: string) => string
  /** Record that `oldId` now lives under `newId` (after a re-creation). */
  remap: (oldId: string, newId: string) => void
}

export interface HistoryEntry {
  /** Short label — surfaced in tooltips and useful when debugging the stack. */
  label?: string
  undo: (ctx: HistoryCtx) => void | Promise<void>
  redo: (ctx: HistoryCtx) => void | Promise<void>
}

export interface EditHistory {
  push: (entry: HistoryEntry) => void
  undo: () => void
  redo: () => void
  canUndo: boolean
  canRedo: boolean
  clear: () => void
  /** True while an entry is being applied — do NOT push from inside a mutation then. */
  applyingRef: React.MutableRefObject<boolean>
}

export function useEditHistory(limit = 100): EditHistory {
  const undoStack = useRef<HistoryEntry[]>([])
  const redoStack = useRef<HistoryEntry[]>([])
  const idMap = useRef(new Map<string, string>())
  const applyingRef = useRef(false)
  // Mirrors the stack depths so the buttons re-render when they change.
  const [depths, setDepths] = useState<[number, number]>([0, 0])
  const sync = useCallback(() => setDepths([undoStack.current.length, redoStack.current.length]), [])

  const ctx: HistoryCtx = {
    // Follow the chain: an entity re-created twice maps A→B→C.
    resolve: (id: string) => {
      let cur = id
      for (let i = 0; i < 16 && idMap.current.has(cur); i++) cur = idMap.current.get(cur)!
      return cur
    },
    remap: (oldId: string, newId: string) => { if (oldId !== newId) idMap.current.set(oldId, newId) },
  }

  const push = useCallback((entry: HistoryEntry) => {
    if (applyingRef.current) return   // a change caused BY an undo is not a new action
    undoStack.current.push(entry)
    if (undoStack.current.length > limit) undoStack.current.shift()
    redoStack.current = []            // a fresh action invalidates the redo branch
    sync()
  }, [limit, sync])

  const run = useCallback((from: React.MutableRefObject<HistoryEntry[]>, to: React.MutableRefObject<HistoryEntry[]>, dir: 'undo' | 'redo') => {
    if (applyingRef.current) return
    const entry = from.current.pop()
    if (!entry) return
    applyingRef.current = true
    sync()
    void Promise.resolve(entry[dir](ctx))
      .then(() => { to.current.push(entry) })
      .catch(err => {
        // A failed inverse would desync the stack — drop the whole branch rather
        // than let a later undo apply on top of a state that never rolled back.
        console.error('[history] failed to ' + dir, err)
        from.current = []
        to.current = []
      })
      .finally(() => { applyingRef.current = false; sync() })
  // `ctx` is rebuilt each render but only closes over refs — stable in practice.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sync])

  const undo = useCallback(() => run(undoStack, redoStack, 'undo'), [run])
  const redo = useCallback(() => run(redoStack, undoStack, 'redo'), [run])
  const clear = useCallback(() => {
    undoStack.current = []; redoStack.current = []; idMap.current.clear(); sync()
  }, [sync])

  return { push, undo, redo, canUndo: depths[0] > 0, canRedo: depths[1] > 0, clear, applyingRef }
}
