import { useCallback, useEffect, useRef } from 'react'
import { api } from '@kubuno/sdk'

// Collaboration epochs (see the server's `services/collab_epoch.rs`).
//
// Every document, spreadsheet and presentation carries an epoch, and its Yjs room
// is named after it. A save made through the REST API by a client outside the
// room (desktop apps, Android, a version restore) opens a new epoch: the old room -
// which still holds the previous content - is closed by the core, and its
// clients must reload the stored content into a fresh room. A save from the web
// names the epoch of its room (`X-Collab-Epoch`) and the server refuses it with
// 409 `COLLAB_EPOCH_STALE` once that room was abandoned, so a session still
// holding the old state can never write it back.
//
// The epoch a save names is always the one of the editor instance that produced
// the content (passed explicitly by the editor, never looked up globally): a save
// still in flight from an editor being replaced must carry ITS epoch, not the one
// the remounted editor just joined.

export type CollabEntity = 'document' | 'spreadsheet' | 'presentation'

/** Why an editor learnt that its room was abandoned. */
export type ReplacedReason =
  /** The core closed the room (or the server reports another epoch). */
  | 'closed'
  /** A save was refused: edits it carried never reached the stored content. */
  | 'stale-save'

/** Header naming the epoch of the room a save comes from. */
export const COLLAB_EPOCH_HEADER = 'X-Collab-Epoch'

/** Room name of an entity at an epoch - same rule as the server's `room_name`. */
export function collabRoom(entity: CollabEntity, id: string, epoch: number): string {
  return epoch > 0 ? `office-${entity}:${id}~${epoch}` : `office-${entity}:${id}`
}

/** Headers of a content save produced by an editor bound to `epoch`. Without an
 *  epoch the save replaces the content, like any REST client's would. */
export function collabEpochHeaders(epoch: number | null | undefined): Record<string, string> {
  return typeof epoch === 'number' ? { [COLLAB_EPOCH_HEADER]: String(epoch) } : {}
}

/** Is this error the server refusing a save from an abandoned room? */
export function isCollabEpochStale(err: unknown): boolean {
  const r = (err as { response?: { status?: number; data?: { error?: string } } } | null)?.response
  return r?.status === 409 && r.data?.error === 'COLLAB_EPOCH_STALE'
}

const key = (entity: CollabEntity, id: string) => `${entity}:${id}`

// Editors listening for refused saves of their entity (saves start in many places;
// the API layer reports the refusal here, see `reportIfStale`).
const staleListeners = new Map<string, Set<(epoch: number | null | undefined) => void>>()

/** Called by the API layer on a failed content save sent with `epoch`: reports a
 *  refusal for a stale room to the editor bound to that epoch. */
export function reportIfStale(entity: CollabEntity, id: string, epoch: number | null | undefined, err: unknown): void {
  if (!isCollabEpochStale(err)) return
  staleListeners.get(key(entity, id))?.forEach(fn => fn(epoch))
}

async function fetchCurrentEpoch(entity: CollabEntity, id: string): Promise<number> {
  const r = await api.get<{ collab_epoch: number }>(`/office/collab/${entity}/${id}/epoch`)
  return r.data.collab_epoch
}

/**
 * Watches for the room of an editor bound to `epoch` being abandoned. `epoch` is
 * `null` until the editor has loaded the stored content (and with it the current
 * epoch); it must not change afterwards - a new epoch means a new room, a new
 * `Y.Doc` and a remounted editor.
 *
 * `onReplaced` is called when:
 * - the core closes the room (`replaced` frame, wired through `useCollab`'s
 *   `onReplaced: signalClosed`) - reason `closed`;
 * - after the socket was lost (the core no longer admits a stale room), or when
 *   the tab comes back to the foreground or online, the server reports another
 *   epoch - reason `closed`;
 * - a content save sent with this epoch is refused - reason `stale-save`,
 *   possibly after a `closed` (the host reloads once).
 */
export function useCollabEpoch(
  entity: CollabEntity,
  id: string,
  epoch: number | null,
  onReplaced: (reason: ReplacedReason) => void,
): { signalClosed: () => void; checkEpoch: () => void } {
  const closedRef = useRef(false)
  const lastCheckRef = useRef(0)
  const cbRef = useRef(onReplaced)
  cbRef.current = onReplaced

  useEffect(() => {
    if (epoch === null) return
    const k = key(entity, id)
    const onStale = (sent: number | null | undefined) => { if (sent === epoch) cbRef.current('stale-save') }
    let set = staleListeners.get(k)
    if (!set) { set = new Set(); staleListeners.set(k, set) }
    set.add(onStale)
    return () => {
      set.delete(onStale)
      if (set.size === 0) staleListeners.delete(k)
    }
  }, [entity, id, epoch])

  const signalClosed = useCallback(() => {
    if (closedRef.current) return
    closedRef.current = true
    cbRef.current('closed')
  }, [])

  const checkEpoch = useCallback(() => {
    if (epoch === null || closedRef.current) return
    const now = Date.now()
    if (now - lastCheckRef.current < 5000) return
    lastCheckRef.current = now
    fetchCurrentEpoch(entity, id)
      .then(current => { if (current !== epoch) signalClosed() })
      .catch(() => { /* offline or gone: the next signal will tell */ })
  }, [entity, id, epoch, signalClosed])

  useEffect(() => {
    const onVisible = () => { if (document.visibilityState === 'visible') checkEpoch() }
    document.addEventListener('visibilitychange', onVisible)
    window.addEventListener('online', checkEpoch)
    return () => {
      document.removeEventListener('visibilitychange', onVisible)
      window.removeEventListener('online', checkEpoch)
    }
  }, [checkEpoch])

  return { signalClosed, checkEpoch }
}
