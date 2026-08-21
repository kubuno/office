// Task table columns (MS-Project style): identity, default widths, and the
// per-project resize/visibility state. Shared by the table header
// (ProjectEditorPage) and the task row (TaskRow) so both always agree.
//
// Widths are published as CSS custom properties (`--gcol-<id>`) on the table
// container: header cells and every row read `width: var(--gcol-<id>)`. Dragging
// a handle rewrites those properties on the container node ONLY — no React
// render, so a table with hundreds of rows stays fluid while resizing. The final
// width is committed to React state (and persisted) once, on pointer up.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { CSSProperties, PointerEvent as ReactPointerEvent, RefObject } from 'react'
import { useAuthStore } from '@kubuno/sdk'
import { useModulePrefs } from '../userPrefs'

export type GanttColId =
  | 'idx' | 'mode' | 'name' | 'dur' | 'progress' | 'priority'
  | 'start' | 'end' | 'variance' | 'pred' | 'res'

/** Columns, left to right. */
export const GANTT_COL_IDS: GanttColId[] = [
  'idx', 'mode', 'name', 'dur', 'progress', 'priority', 'start', 'end', 'variance', 'pred', 'res',
]

/** Default widths (px) — also what a double-click on a resize handle restores. */
export const COL_W: Record<GanttColId, number> = {
  idx: 34, mode: 34, name: 168, dur: 64, progress: 52, priority: 108,
  start: 92, end: 92, variance: 68, pred: 96, res: 120,
}

/** Lower bound per column: a column can be narrowed, never squashed to nothing. */
export const COL_MIN: Record<GanttColId, number> = {
  idx: 28, mode: 28, name: 96, dur: 44, progress: 40, priority: 64,
  start: 60, end: 60, variance: 48, pred: 48, res: 60,
}

/** Upper bound: past this the Gantt loses all its room. */
export const COL_MAX = 480

/** Pictogram columns (a row number, a scheduling-mode icon): nothing to widen. */
const FIXED_COLS: GanttColId[] = ['idx', 'mode']
export const isColResizable = (id: GanttColId): boolean => !FIXED_COLS.includes(id)

/** The task name IS the row's identity — it can never be hidden. */
export const COL_ALWAYS_VISIBLE: GanttColId = 'name'
export const isColHideable = (id: GanttColId): boolean => id !== COL_ALWAYS_VISIBLE

export const PRIO_COLOR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }

// ── CSS custom properties ─────────────────────────────────────────────────────

/** Custom property holding column `id`'s width. */
export const colVar = (id: GanttColId): string => `--gcol-${id}`
/** Custom property holding the sum of the VISIBLE columns (the table's width). */
export const TABLE_VAR = '--gcol-table'

// Built once: every cell of every row reuses the same frozen style object, so a
// re-render never allocates one style object per cell.
const CELL_STYLE = GANTT_COL_IDS.reduce((acc, id) => {
  acc[id] = { width: `var(${colVar(id)}, ${COL_W[id]}px)` }
  return acc
}, {} as Record<GanttColId, CSSProperties>)

/** Width style of a cell of column `id` (reads the container's custom property). */
export const colStyle = (id: GanttColId): CSSProperties => CELL_STYLE[id]

// ── Per-project layout, persisted in the user's module preferences ────────────

export interface GanttColLayout {
  /** Overridden widths; a missing column keeps `COL_W`. */
  widths: Partial<Record<GanttColId, number>>
  /** Columns the user hid. */
  hidden: GanttColId[]
}

/** Key under `preferences.office` (see `userPrefs`). */
const PREF_KEY = 'ganttColumns'
/** Hard cap on remembered projects, so the user row can never bloat. */
const MAX_PROJECTS = 40

interface GanttColPrefs {
  [PREF_KEY]: Record<string, GanttColLayout>
  /** Required by useModulePrefs' generic: office prefs hold other keys too. */
  [key: string]: unknown
}

const DEFAULT_PREFS: GanttColPrefs = { [PREF_KEY]: {} }
const EMPTY_LAYOUT: GanttColLayout = { widths: {}, hidden: [] }

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v))

/** Read + sanitise one project's layout (stored JSON is user data: trust nothing). */
function readLayout(bag: Record<string, GanttColLayout> | undefined, projectId: string | undefined): GanttColLayout {
  const raw = projectId ? bag?.[projectId] : undefined
  if (!raw || typeof raw !== 'object') return EMPTY_LAYOUT
  const widths: Partial<Record<GanttColId, number>> = {}
  const storedW = (raw.widths ?? {}) as Partial<Record<GanttColId, number>>
  for (const id of GANTT_COL_IDS) {
    const w = storedW[id]
    if (typeof w === 'number' && Number.isFinite(w)) widths[id] = clamp(Math.round(w), COL_MIN[id], COL_MAX)
  }
  const storedH = Array.isArray(raw.hidden) ? raw.hidden : []
  const hidden = GANTT_COL_IDS.filter(id => isColHideable(id) && storedH.includes(id))
  return { widths, hidden }
}

const widthOf = (l: GanttColLayout, id: GanttColId) => l.widths[id] ?? COL_W[id]

/** Sum of the visible columns = the table panel's width. */
function tableWidth(l: GanttColLayout): number {
  let total = 0
  for (const id of GANTT_COL_IDS) if (!l.hidden.includes(id)) total += widthOf(l, id)
  return total
}

// Writing `preferences.office` is a read-modify-write of the whole bag: two
// overlapping PATCH /me clobber each other (known race). Every write is therefore
// queued on a single chain, each one re-reading the store after the previous one.
let writeChain: Promise<unknown> = Promise.resolve()

/** Latest stored map, read from the store (never from a render closure: a queued
 *  write must see what the previous one saved). */
function storedBag(): Record<string, GanttColLayout> {
  const office = useAuthStore.getState().user?.preferences?.office as Record<string, unknown> | undefined
  const raw = office?.[PREF_KEY]
  return raw && typeof raw === 'object' ? { ...(raw as Record<string, GanttColLayout>) } : {}
}

/** Debounce before persisting: a drag writes once, at rest, not once per pixel. */
const PERSIST_MS = 600

export interface GanttColumnsApi {
  /** Visibility per column (the name column is always true). */
  visible: Record<GanttColId, boolean>
  /** Effective widths (defaults + user overrides). */
  widths: Record<GanttColId, number>
  /** Custom properties + width, to spread on the table container's `style`. */
  containerStyle: CSSProperties
  /** Attach to the table container (the node carrying the custom properties). */
  tableRef: RefObject<HTMLDivElement | null>
  /** Pointer-down on a resize handle. */
  startResize: (id: GanttColId, e: ReactPointerEvent) => void
  /** Double-click on a handle: back to this column's default width. */
  resetColumn: (id: GanttColId) => void
  toggleColumn: (id: GanttColId) => void
  /** Default widths and every column visible again. */
  resetAll: () => void
  /** True when the layout differs from the defaults. */
  customised: boolean
}

/**
 * Column widths + visibility of the task table, remembered PER PROJECT.
 *
 * Persisted in the user's `office` preferences (server side, so the layout
 * follows the user across browsers) rather than in localStorage.
 */
export function useGanttColumns(projectId: string | undefined): GanttColumnsApi {
  const { prefs, update } = useModulePrefs<GanttColPrefs>('office', DEFAULT_PREFS)
  const bag = prefs[PREF_KEY] as Record<string, GanttColLayout> | undefined

  const [layout, setLayout] = useState<GanttColLayout>(() => readLayout(bag, projectId))
  const layoutRef = useRef(layout)
  layoutRef.current = layout
  const tableRef = useRef<HTMLDivElement | null>(null)

  // Serialised form of what the server holds for THIS project: a stable dependency
  // (the prefs object gets a new identity on every render of the auth store).
  const storedJson = JSON.stringify(readLayout(bag, projectId))
  // Local edits win over the stored copy until they have been flushed, otherwise
  // a PATCH round-trip would snap the column the user just dragged back.
  const dirtyRef = useRef(false)

  useEffect(() => {
    // A new project starts from ITS stored layout, never from the previous one.
    dirtyRef.current = false
    const next = JSON.parse(storedJson) as GanttColLayout
    if (JSON.stringify(layoutRef.current) !== storedJson) setLayout(next)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  useEffect(() => {
    if (dirtyRef.current) return
    if (JSON.stringify(layoutRef.current) === storedJson) return
    setLayout(JSON.parse(storedJson) as GanttColLayout)
  }, [storedJson])

  // ── Persistence (debounced, one write per gesture) ──
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingRef = useRef<GanttColLayout | null>(null)
  const updateRef = useRef(update)
  updateRef.current = update
  const projectRef = useRef(projectId)
  projectRef.current = projectId

  const flush = useCallback(() => {
    const pid = projectRef.current
    const next = pendingRef.current
    pendingRef.current = null
    if (timerRef.current) { clearTimeout(timerRef.current); timerRef.current = null }
    if (!pid || !next) return
    writeChain = writeChain.then(async () => {
      const isDefault = !Object.keys(next.widths).length && !next.hidden.length
      const merged = storedBag()
      if (isDefault) delete merged[pid]
      else merged[pid] = next
      // Keep the most recently written projects only (insertion order).
      const keys = Object.keys(merged)
      if (keys.length > MAX_PROJECTS) for (const k of keys.slice(0, keys.length - MAX_PROJECTS)) delete merged[k]
      try { await updateRef.current({ [PREF_KEY]: merged }) } catch { /* offline: the layout stays local */ }
      // Only the LAST write hands authority back to the stored copy: a newer edit
      // made while this PATCH was in flight must not be overwritten by its echo.
      if (!pendingRef.current) dirtyRef.current = false
    }).catch(() => { /* never break the chain */ })
  }, [])

  const commit = useCallback((next: GanttColLayout) => {
    dirtyRef.current = true
    setLayout(next)
    pendingRef.current = next
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(flush, PERSIST_MS)
  }, [flush])

  // Unmounting (leaving the project) must not lose the last gesture.
  useEffect(() => () => { flush() }, [flush])

  // ── Live resize: the container's custom properties, no React render ──
  const paint = useCallback((l: GanttColLayout) => {
    const el = tableRef.current
    if (!el) return
    for (const id of GANTT_COL_IDS) el.style.setProperty(colVar(id), `${widthOf(l, id)}px`)
    el.style.setProperty(TABLE_VAR, `${tableWidth(l)}px`)
  }, [])

  const startResize = useCallback((id: GanttColId, e: ReactPointerEvent) => {
    if (!isColResizable(id)) return
    e.preventDefault()
    e.stopPropagation()
    const startX = e.clientX
    const base = layoutRef.current
    const startW = widthOf(base, id)
    let width = startW
    const prevCursor = document.body.style.cursor
    const prevSelect = document.body.style.userSelect
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const move = (ev: PointerEvent) => {
      const w = clamp(Math.round(startW + ev.clientX - startX), COL_MIN[id], COL_MAX)
      if (w === width) return
      width = w
      paint({ ...base, widths: { ...base.widths, [id]: w } })
    }
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      document.body.style.cursor = prevCursor
      document.body.style.userSelect = prevSelect
      if (width !== startW) commit({ ...layoutRef.current, widths: { ...layoutRef.current.widths, [id]: width } })
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }, [commit, paint])

  const resetColumn = useCallback((id: GanttColId) => {
    const widths = { ...layoutRef.current.widths }
    if (widths[id] === undefined) return
    delete widths[id]
    commit({ ...layoutRef.current, widths })
  }, [commit])

  const toggleColumn = useCallback((id: GanttColId) => {
    if (!isColHideable(id)) return
    const cur = layoutRef.current
    const hidden = cur.hidden.includes(id) ? cur.hidden.filter(x => x !== id) : [...cur.hidden, id]
    commit({ ...cur, hidden })
  }, [commit])

  const resetAll = useCallback(() => { commit({ widths: {}, hidden: [] }) }, [commit])

  const visible = useMemo(() => GANTT_COL_IDS.reduce((acc, id) => {
    acc[id] = !layout.hidden.includes(id)
    return acc
  }, {} as Record<GanttColId, boolean>), [layout])

  const widths = useMemo(() => GANTT_COL_IDS.reduce((acc, id) => {
    acc[id] = widthOf(layout, id)
    return acc
  }, {} as Record<GanttColId, number>), [layout])

  const containerStyle = useMemo(() => {
    const style: Record<string, string> = { width: `var(${TABLE_VAR})` }
    for (const id of GANTT_COL_IDS) style[colVar(id)] = `${widthOf(layout, id)}px`
    style[TABLE_VAR] = `${tableWidth(layout)}px`
    return style as CSSProperties
  }, [layout])

  const customised = !!Object.keys(layout.widths).length || !!layout.hidden.length

  return { visible, widths, containerStyle, tableRef, startResize, resetColumn, toggleColumn, resetAll, customised }
}
