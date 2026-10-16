import * as Y from 'yjs'
import {
  Awareness,
  encodeAwarenessUpdate,
  applyAwarenessUpdate,
  removeAwarenessStates,
} from 'y-protocols/awareness'
import { IndexeddbPersistence } from 'y-indexeddb'
import { useEffect, useRef } from 'react'
import { useAuthStore, signedSocketUrl } from '@kubuno/sdk'

// Base64 of an awareness binary (it travels on the core's text channel, which
// relays text frames as they are between the clients of a room).
function b64encode(u8: Uint8Array): string {
  let s = ''
  for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i])
  return btoa(s)
}
function b64decode(str: string): Uint8Array {
  const s = atob(str)
  const u8 = new Uint8Array(s.length)
  for (let i = 0; i < s.length; i++) u8[i] = s.charCodeAt(i)
  return u8
}

// SHARED real-time collaboration glue: connects a `Y.Doc` to the core's generic
// Yjs service (`/collab/:room/sync`). Reusable by EVERY kubuno file editor
// (Office, PaintSharp, Flow, Notes…). The core relays/persists opaque binary
// updates; here remote updates are applied to the doc and local ones are sent.
// Automatic reconnection with backoff.
//
// Typical use in an editor:
//   const doc = useMemo(() => new Y.Doc(), [entityId])
//   const collab = useCollab(collabRoom('document', entityId, epoch), doc, !!entityId)
//   // ... bind `doc` to the content (TipTap Collaboration, Y.Map, etc.)

export type CollabStatus = 'connecting' | 'connected' | 'disconnected'

export interface CollabEvents {
  /** The core closed the room: the content was replaced behind it (REST save from
   *  outside the room). The session stops for good - no reconnection, and the
   *  local IndexedDB copy of the abandoned room is erased - and the editor must
   *  reload the stored content. */
  onReplaced?: () => void
  /** The socket dropped (reconnection is attempted). The room may have been
   *  abandoned meanwhile: a stale room is no longer admitted by the core. */
  onConnectionLost?: () => void
}

export interface CollabHandle {
  /** Broadcasts an awareness message (cursor/presence) - free text, not persisted. */
  sendAwareness: (json: unknown) => void
  /** Closes the session. */
  destroy: () => void
  /** Current state of the connection (read only, updated internally). */
  statusRef: { current: CollabStatus }
}

function wsUrl(room: string, token: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}/collab/${encodeURIComponent(room)}/sync?token=${encodeURIComponent(token)}`
}

/**
 * Connects a `Y.Doc` to the core's collab service. Returns a handle for awareness
 * and teardown. `getToken` lets the token be refreshed on every (re)connection.
 */
export function connectCollab(
  room: string,
  doc: Y.Doc,
  getToken: () => string | null,
  onStatus?: (s: CollabStatus) => void,
  onAwareness?: (json: unknown) => void,
  /** Called on the first sync: `empty` = the room had no state (→ this client may seed it). Once. */
  onSync?: (empty: boolean) => void,
  /** Yjs awareness instance (cursors/presence). When given, relayed over the text channel. */
  awareness?: Awareness,
  /** Builds the WS URL. Default: `/collab/:room/sync`. Lets a module (e.g. the
   *  whiteboard) target its own route while reusing this glue. */
  urlBuilder?: (room: string, token: string) => string,
  /** Local persistence (IndexedDB) of the `Y.Doc` → offline editing + hydration at
   *  start without a server. On by default (can be turned off for ephemeral docs). */
  persistLocal = true,
  /** Room lifecycle signals (see `collabEpoch.ts`). */
  events?: CollabEvents,
): CollabHandle {
  const buildUrl = urlBuilder ?? wsUrl
  let syncedOnce = false
  let ws: WebSocket | null = null
  let closed = false
  let retry = 0
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  const statusRef = { current: 'connecting' as CollabStatus }

  // ── Local persistence (offline-first) ───────────────────────────────────────
  // IndexedDB hydrates the doc on load (editing works without network) and keeps
  // local updates across reloads. On (re)connection, `onopen` pushes the doc's
  // full state → the server merges it (CRDT). Updates replayed from IndexedDB
  // (origin = the idb provider) are not sent one by one: they leave with the full
  // state on `onopen`. The database is keyed by the room name, which carries the
  // collaboration epoch: a replaced room never feeds its local copy into the next.
  let idb: IndexeddbPersistence | null = null
  if (persistLocal && typeof indexedDB !== 'undefined') {
    try { idb = new IndexeddbPersistence(room, doc) } catch { idb = null }
  }

  const setStatus = (s: CollabStatus) => { statusRef.current = s; onStatus?.(s) }

  // Local updates → server (except those coming from the server, origin 'remote',
  // or replayed from IndexedDB on hydration, origin = the idb provider).
  const onUpdate = (update: Uint8Array, origin: unknown) => {
    if (origin === 'remote' || (idb && origin === idb)) return
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(update as Uint8Array<ArrayBuffer>)
    }
  }
  doc.on('update', onUpdate)

  // ── Awareness (cursors/presence) ────────────────────────────────────────────
  const sendTxt = (obj: unknown) => {
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj))
  }
  const sendAwarenessFor = (clients: number[]) => {
    if (!awareness || clients.length === 0) return
    sendTxt({ kbAwareness: b64encode(encodeAwarenessUpdate(awareness, clients)) })
  }
  // Broadcasts local changes; answers new peers with our own state.
  const onAwarenessUpdate = (
    { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ) => {
    if (origin !== 'remote') {
      sendAwarenessFor([...added, ...updated, ...removed])
    } else if (added.length > 0 && awareness) {
      // A new peer appeared → (re)publish our own state so that it sees us.
      sendAwarenessFor([awareness.clientID])
    }
  }
  if (awareness) awareness.on('update', onAwarenessUpdate)

  const open = () => {
    const token = getToken()
    if (!token || closed) return
    setStatus('connecting')
    // The handshake cannot carry the Authorization header. Whatever URL the
    // builder produced (custom builders still append `?token=`), the access
    // token is stripped and a one-minute socket ticket bound to the room's
    // path is presented instead - a fresh one on every (re)connect.
    signedSocketUrl(buildUrl(room, token)).then(url => {
      if (closed) return
      attach(new WebSocket(url))
    }).catch(() => {
      if (closed) return
      setStatus('disconnected')
      scheduleReconnect()
    })
  }

  const attach = (sock: WebSocket) => {
    sock.binaryType = 'arraybuffer'
    ws = sock

    sock.onopen = () => {
      retry = 0
      setStatus('connected')
      // Push the current local state: useful when a client seeded the doc (new
      // document) - the state thus becomes shared/persisted on the server.
      const state = Y.encodeStateAsUpdate(doc)
      if (state.length > 2) sock.send(state as Uint8Array<ArrayBuffer>)
      // Publish our current presence so that peers already connected see us.
      if (awareness && awareness.getLocalState()) sendAwarenessFor([awareness.clientID])
    }
    sock.onmessage = (e) => {
      if (closed) return
      if (e.data instanceof ArrayBuffer) {
        Y.applyUpdate(doc, new Uint8Array(e.data), 'remote')
      } else if (typeof e.data === 'string') {
        try {
          const m = JSON.parse(e.data)
          if (m && m.type === 'replaced') {
            handleReplaced()
          } else if (m && m.type === 'sync') {
            if (!syncedOnce) { syncedOnce = true; onSync?.(!!m.empty) }
          } else if (m && typeof m.kbAwareness === 'string') {
            if (awareness) applyAwarenessUpdate(awareness, b64decode(m.kbAwareness), 'remote')
          } else {
            onAwareness?.(m)
          }
        } catch { /* ignore */ }
      }
    }
    sock.onclose = () => {
      if (ws === sock) ws = null
      setStatus('disconnected')
      if (!closed) {
        events?.onConnectionLost?.()
        scheduleReconnect()
      }
    }
    sock.onerror = () => { try { sock.close() } catch { /* ignore */ } }
  }

  // The core closed the room because the content was replaced behind it. Its
  // state is the previous content: stop for good, never push it anywhere again.
  const handleReplaced = () => {
    if (closed) return
    closed = true
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null }
    doc.off('update', onUpdate)
    // The IndexedDB copy belongs to the abandoned room (it is keyed by the room
    // name, which carries the epoch): erase it rather than leave it behind.
    if (idb) {
      const local = idb
      idb = null
      local.clearData().catch(() => { /* best effort */ })
    }
    try { ws?.close() } catch { /* ignore */ }
    ws = null
    setStatus('disconnected')
    events?.onReplaced?.()
  }

  const scheduleReconnect = () => {
    if (closed || reconnectTimer) return
    const delay = Math.min(1000 * 2 ** retry, 15000)
    retry += 1
    reconnectTimer = setTimeout(() => { reconnectTimer = null; open() }, delay)
  }

  open()

  return {
    sendAwareness: (json) => {
      if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(json))
    },
    statusRef,
    destroy: () => {
      closed = true
      if (reconnectTimer) clearTimeout(reconnectTimer)
      doc.off('update', onUpdate)
      // Detach local persistence without erasing it (the offline cache is kept for
      // the next opening of the same room).
      if (idb) { try { idb.destroy() } catch { /* ignore */ } idb = null }
      if (awareness) {
        awareness.off('update', onAwarenessUpdate)
        // Tell peers we are leaving (cursor removed) before closing.
        try { removeAwarenessStates(awareness, [awareness.clientID], 'local') } catch { /* ignore */ }
      }
      try { ws?.close() } catch { /* ignore */ }
      ws = null
    },
  }
}

/**
 * React hook: opens a collab session for `room` bound to `doc` while `enabled`.
 * Handles the token (authStore) and the cleanup on unmount.
 */
export function useCollab(
  room: string,
  doc: Y.Doc,
  enabled: boolean,
  opts?: {
    onStatus?: (s: CollabStatus) => void
    onAwareness?: (json: unknown) => void
    onSync?: (empty: boolean) => void
    /** Yjs awareness (cursors/presence) - relayed automatically when given. */
    awareness?: Awareness
    /** Builds the WS URL (default: the core's generic collab route). */
    urlBuilder?: (room: string, token: string) => string
    /** Local IndexedDB persistence (offline editing). Default: true. */
    persistLocal?: boolean
  } & CollabEvents,
): { sendAwareness: (json: unknown) => void } {
  const handleRef = useRef<CollabHandle | null>(null)
  // Callbacks kept in a ref → no reconnection on every render.
  const cbRef = useRef(opts)
  cbRef.current = opts
  const awareness = opts?.awareness

  useEffect(() => {
    if (!enabled) return
    const handle = connectCollab(
      room,
      doc,
      () => useAuthStore.getState().accessToken,
      (s) => cbRef.current?.onStatus?.(s),
      (j) => cbRef.current?.onAwareness?.(j),
      (empty) => cbRef.current?.onSync?.(empty),
      awareness,
      cbRef.current?.urlBuilder,
      cbRef.current?.persistLocal ?? true,
      {
        onReplaced:       () => cbRef.current?.onReplaced?.(),
        onConnectionLost: () => cbRef.current?.onConnectionLost?.(),
      },
    )
    handleRef.current = handle
    return () => { handle.destroy(); handleRef.current = null }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [room, enabled, awareness])

  return { sendAwareness: (json) => handleRef.current?.sendAwareness(json) }
}
