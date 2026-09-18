import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn, MenuDropdown, type MenuItem } from '@ui'
import { prompt, formatDate, addDays, differenceInDays } from '@kubuno/sdk'
import { LayoutGrid, Maximize, RotateCcw, Scissors, Timer, Link2, Magnet, CalendarClock } from 'lucide-react'
import { schedStart, schedEnd } from './schedule'
import { CRITICAL_CLR, MILESTONE_CLR, TASK_COLOR } from './GanttRenderer'
import { useModulePrefs } from '../userPrefs'
import type { ProjectTask, TaskDependency } from '../api'

// Network (precedence) diagram of the schedule: one card per task, one arrow per
// dependency — the activity-on-node network of the PMBOK, on a free workspace in
// the manner of the Flow module: pan & zoom, cards you can drag, links you draw
// from a card's output port and edit or cut in place. The layered auto-layout
// (a column per precedence rank) stays the default; a card the user moved keeps
// its place (remembered per project, per user) until "Rearrange" is asked for.

export const NODE_W = 190
export const NODE_H = 78
const COL_W = 250
const ROW_H = 108
const PAD = 40
const PORT_R = 6
/** Pixels of travel before a press on a card becomes a drag (a sloppy click selects). */
const DRAG_THRESHOLD = 4
/** Grid step for snap-to-grid — matches the background dot spacing. */
const GRID = 24
const snapTo = (v: number) => Math.round(v / GRID) * GRID
/** Time-scaled mode: pixels per calendar day, minimum card width, ruler height. */
const DAY_W = 16
const MIN_BAR_W = 96
const RULER_H = 30

interface Viewport { tx: number; ty: number; scale: number }
type Pt = { x: number; y: number }
type Pos = Record<string, Pt>

const DEP_TYPES: TaskDependency['dep_type'][] = ['FS', 'SS', 'FF', 'SF']

// ── Persistence of the hand-made layout (user preferences, per project) ──────
const PREF_KEY = 'networkLayout'
const MAX_PROJECTS = 40
interface NetPrefs { [PREF_KEY]: Record<string, Pos>; [key: string]: unknown }
const DEFAULT_PREFS: NetPrefs = { [PREF_KEY]: {} }

function readPos(bag: Record<string, Pos> | undefined, projectId: string): Pos {
  const raw = bag?.[projectId]
  if (!raw || typeof raw !== 'object') return {}
  const out: Pos = {}
  for (const [id, p] of Object.entries(raw)) {
    if (p && Number.isFinite(p.x) && Number.isFinite(p.y)) out[id] = { x: Math.round(p.x), y: Math.round(p.y) }
  }
  return out
}

// ── Auto-layout: a column per precedence rank, rows in schedule order ────────
// Rows are handed out in BANDS following the WBS: a phase (summary) gets a band
// of rows for all its descendants, nested phases get sub-bands inside it, and
// consecutive leaves of one parent share rows across the columns. Every phase
// therefore occupies one rectangle of the grid — its hull never interleaves
// with a sibling's.
/** Rows left free after a phase's band, so stacked hulls never touch. */
const GROUP_GAP = 0.5

/** Any task that has children is a phase, whatever its `task_type` says — the
 *  Gantt treats it the same way (bracket, rolled-up progress). */
function parentIds(all: ProjectTask[]): Set<string> {
  const ids = new Set<string>()
  for (const tk of all) if (tk.parent_id) ids.add(tk.parent_id)
  for (const tk of all) if (tk.task_type === 'summary') ids.add(tk.id)
  return ids
}

function autoLayout(all: ProjectTask[], nodes: ProjectTask[], edges: TaskDependency[]): Pos {
  const nodeIds = new Set(nodes.map(n => n.id))
  const parents = parentIds(all)
  const level = new Map(nodes.map(n => [n.id, 0]))
  let changed = true, iter = 0
  while (changed && iter++ < nodes.length + 2) {
    changed = false
    for (const e of edges) {
      const nl = (level.get(e.from_task_id) ?? 0) + 1
      if (nl > (level.get(e.to_task_id) ?? 0)) { level.set(e.to_task_id, nl); changed = true }
    }
  }
  const childrenOf = new Map<string | null, ProjectTask[]>()
  for (const tk of all) { const k = tk.parent_id ?? null; if (!childrenOf.has(k)) childrenOf.set(k, []); childrenOf.get(k)!.push(tk) }
  const pos: Pos = {}
  const place = (items: ProjectTask[], row0: number): number => {
    let row = row0, i = 0
    while (i < items.length) {
      if (parents.has(items[i].id)) {
        const used = place(childrenOf.get(items[i].id) ?? [], row)
        if (used > 0) row += used + GROUP_GAP
        i++
        continue
      }
      // A run of leaves: one counter per column, ordered by schedule.
      const run: ProjectTask[] = []
      while (i < items.length && !parents.has(items[i].id)) run.push(items[i++])
      run.sort((a, b) => (a.early_start ?? 0) - (b.early_start ?? 0) || a.name.localeCompare(b.name))
      const counters = new Map<number, number>()
      for (const leaf of run) {
        if (!nodeIds.has(leaf.id)) continue
        const l = level.get(leaf.id) ?? 0, r = counters.get(l) ?? 0
        pos[leaf.id] = { x: PAD + l * COL_W, y: PAD + (row + r) * ROW_H }
        counters.set(l, r + 1)
      }
      row += Math.max(0, ...counters.values())
    }
    return Math.max(0, row - row0)
  }
  place(childrenOf.get(null) ?? [], 0)
  return pos
}

// ── Time-scaled layout (Primavera-style TSLD): X = date, width = duration ─────
// Same WBS bands as the free layout (a phase gets a block of rows, sub-phases
// nested), but a leaf's X is its early-start date and its width its calendar
// span; within a band, leaves are packed into lanes by their time interval so
// bars never overlap. The result reads left-to-right as the schedule.
function timeLayout(all: ProjectTask[], nodes: ProjectTask[]): { pos: Pos; width: Record<string, number>; minDay: number; maxDay: number } {
  const nodeIds = new Set(nodes.map(n => n.id))
  const parents = parentIds(all)
  const es = (t: ProjectTask) => t.early_start ?? 0
  const calDays = (t: ProjectTask) => t.task_type === 'milestone' ? 0 : Math.max(0, (t.early_finish ?? es(t) + t.duration_days) - es(t))
  const days = nodes.map(es)
  const minDay = days.length ? Math.min(...days) : 0
  let maxDay = minDay
  const childrenOf = new Map<string | null, ProjectTask[]>()
  for (const tk of all) { const k = tk.parent_id ?? null; if (!childrenOf.has(k)) childrenOf.set(k, []); childrenOf.get(k)!.push(tk) }
  const pos: Pos = {}, width: Record<string, number> = {}
  const xOf = (t: ProjectTask) => PAD + (es(t) - minDay) * DAY_W
  const wOf = (t: ProjectTask) => Math.max(MIN_BAR_W, calDays(t) * DAY_W)
  const place = (items: ProjectTask[], row0: number): number => {
    let row = row0, i = 0
    while (i < items.length) {
      if (parents.has(items[i].id)) {
        const used = place(childrenOf.get(items[i].id) ?? [], row)
        if (used > 0) row += used + GROUP_GAP
        i++
        continue
      }
      const run: ProjectTask[] = []
      while (i < items.length && !parents.has(items[i].id)) run.push(items[i++])
      run.sort((a, b) => es(a) - es(b))
      const laneEnd: number[] = []
      for (const leaf of run) {
        if (!nodeIds.has(leaf.id)) continue
        const x = xOf(leaf), w = wOf(leaf)
        let lane = laneEnd.findIndex(end => end <= x - 8)
        if (lane === -1) { lane = laneEnd.length; laneEnd.push(0) }
        laneEnd[lane] = x + w
        pos[leaf.id] = { x, y: PAD + RULER_H + (row + lane) * ROW_H }
        width[leaf.id] = w
        maxDay = Math.max(maxDay, es(leaf) + calDays(leaf))
      }
      row += Math.max(0, laneEnd.length)
    }
    return Math.max(0, row - row0)
  }
  place(childrenOf.get(null) ?? [], 0)
  return { pos, width, minDay, maxDay }
}

// ── Phase hulls: a translucent frame around a summary's descendants ──────────
const HULL_PAD = 10
/** Room kept above the content for the phase name. */
const HULL_HEAD = 26
export interface Hull { id: string; name: string; depth: number; x: number; y: number; w: number; h: number; ids: string[] }
function computeHulls(all: ProjectTask[], nodeIds: Set<string>, posOf: (id: string) => Pt, widthOf: (id: string) => number): Hull[] {
  const childrenOf = new Map<string | null, ProjectTask[]>()
  for (const tk of all) { const k = tk.parent_id ?? null; if (!childrenOf.has(k)) childrenOf.set(k, []); childrenOf.get(k)!.push(tk) }
  const parents = parentIds(all)
  const out: Hull[] = []
  type Box = { x1: number; y1: number; x2: number; y2: number; ids: string[] }
  // A phase's box wraps its own cards and its nested phases' frames (each with
  // its header room), so a nested frame always leaves its parent's name visible.
  const visit = (summary: ProjectTask, depth: number): Box | null => {
    let box: Box | null = null
    const grow = (b: Box) => { box = box ? { x1: Math.min(box.x1, b.x1), y1: Math.min(box.y1, b.y1), x2: Math.max(box.x2, b.x2), y2: Math.max(box.y2, b.y2), ids: [...box.ids, ...b.ids] } : b }
    for (const ch of childrenOf.get(summary.id) ?? []) {
      if (parents.has(ch.id)) {
        const inner = visit(ch, depth + 1)
        if (inner) grow({ x1: inner.x1 - HULL_PAD, y1: inner.y1 - HULL_HEAD, x2: inner.x2 + HULL_PAD, y2: inner.y2 + HULL_PAD, ids: inner.ids })
      } else if (nodeIds.has(ch.id)) {
        const p = posOf(ch.id)
        grow({ x1: p.x, y1: p.y, x2: p.x + widthOf(ch.id), y2: p.y + NODE_H, ids: [ch.id] })
      }
    }
    if (!box) return null
    const b: Box = box
    out.push({ id: summary.id, name: summary.name, depth, x: b.x1 - HULL_PAD, y: b.y1 - HULL_HEAD, w: b.x2 - b.x1 + 2 * HULL_PAD, h: b.y2 - b.y1 + HULL_HEAD + HULL_PAD, ids: b.ids })
    return b
  }
  const walk = (items: ProjectTask[], depth: number) => { for (const it of items) if (parents.has(it.id)) visit(it, depth) }
  walk(childrenOf.get(null) ?? [], 0)
  // Outer frames first, so a nested frame paints on top of its parent.
  return out.sort((a, b) => a.depth - b.depth)
}

// ── Orthogonal connector (right port → left port), rounded bends ─────────────
function orthRoute(src: Pt, dst: Pt): Pt[] {
  const P = 24
  if (dst.x >= src.x + P * 2) {
    if (Math.abs(src.y - dst.y) < 1) return [src, dst]
    const mx = Math.round((src.x + dst.x) / 2)
    return [src, { x: mx, y: src.y }, { x: mx, y: dst.y }, dst]
  }
  // Target behind the source: go out, around, and in from the left.
  const my = Math.round((src.y + dst.y) / 2)
  return [src, { x: src.x + P, y: src.y }, { x: src.x + P, y: my }, { x: dst.x - P, y: my }, { x: dst.x - P, y: dst.y }, dst]
}
const dist = (p: Pt, q: Pt) => Math.hypot(q.x - p.x, q.y - p.y)
function roundedPath(pts: Pt[]): string {
  if (pts.length < 2) return ''
  const R = 10
  const rad = pts.map(() => 0)
  for (let i = 1; i < pts.length - 1; i++) rad[i] = Math.min(R, dist(pts[i - 1], pts[i]) / 2, dist(pts[i], pts[i + 1]) / 2)
  let d = `M ${pts[0].x} ${pts[0].y}`
  for (let s = 0; s < pts.length - 1; s++) {
    const p0 = pts[s], p1 = pts[s + 1]
    const len = dist(p0, p1) || 1
    const dx = (p1.x - p0.x) / len, dy = (p1.y - p0.y) / len
    const endCut = s + 1 < pts.length - 1 ? rad[s + 1] : 0
    d += ` L ${p1.x - dx * endCut} ${p1.y - dy * endCut}`
    if (s + 1 < pts.length - 1) {
      const p2 = pts[s + 2], l2 = dist(p1, p2) || 1
      d += ` Q ${p1.x} ${p1.y} ${p1.x + ((p2.x - p1.x) / l2) * rad[s + 1]} ${p1.y + ((p2.y - p1.y) / l2) * rad[s + 1]}`
    }
  }
  return d
}
/** Point at the middle of the polyline (for the label and the cut button). */
function midPoint(pts: Pt[]): Pt {
  const total = pts.slice(1).reduce((s, p, i) => s + dist(pts[i], p), 0)
  let acc = 0
  for (let i = 0; i < pts.length - 1; i++) {
    const l = dist(pts[i], pts[i + 1])
    if (acc + l >= total / 2) { const k = l ? (total / 2 - acc) / l : 0; return { x: pts[i].x + (pts[i + 1].x - pts[i].x) * k, y: pts[i].y + (pts[i + 1].y - pts[i].y) * k } }
    acc += l
  }
  return pts[pts.length - 1]
}

export default function NetworkView({
  projectId, tasks, deps, projectStart, selectedId, progressMap, canEdit,
  onSelect, onContextMenu, onConnect, onDeleteDep, onSetDep,
}: {
  projectId: string
  tasks: ProjectTask[]; deps: TaskDependency[]; projectStart: Date
  /** No longer used (formatDate resolves the locale itself); kept until callers stop passing it. */
  locale?: unknown
  selectedId: string | null; progressMap: Map<string, number>; canEdit: boolean
  onSelect: (id: string | null) => void
  onContextMenu: (e: React.MouseEvent, id: string) => void
  /** Draws a link; resolves false when the server refused it (it would close a loop). */
  onConnect: (fromId: string, toId: string) => Promise<boolean>
  onDeleteDep: (dep: TaskDependency) => void
  onSetDep: (dep: TaskDependency, patch: { dep_type?: TaskDependency['dep_type']; lag_days?: number }) => void
}) {
  const { t } = useTranslation('office')
  const containerRef = useRef<HTMLDivElement>(null)

  // Phases (summaries, and any task that has sub-tasks) are not activities of
  // the network: their span is their children's. They become frames instead.
  const nodes = useMemo(() => { const parents = parentIds(tasks); return tasks.filter(tk => !parents.has(tk.id)) }, [tasks])
  const edges = useMemo(() => { const ids = new Set(nodes.map(n => n.id)); return deps.filter(d => ids.has(d.from_task_id) && ids.has(d.to_task_id)) }, [nodes, deps])
  const byId = useMemo(() => new Map(nodes.map(n => [n.id, n])), [nodes])

  // ── Positions: auto-layout overridden by what the user moved ──
  const { prefs, update } = useModulePrefs<NetPrefs>('office', DEFAULT_PREFS)
  const storedJson = JSON.stringify(readPos(prefs[PREF_KEY], projectId))
  const [manual, setManual] = useState<Pos>(() => JSON.parse(storedJson) as Pos)
  const manualRef = useRef(manual); manualRef.current = manual
  const dirtyRef = useRef(false)
  useEffect(() => {
    // The stored copy is authoritative only while nothing local is in flight.
    if (dirtyRef.current) return
    if (JSON.stringify(manualRef.current) !== storedJson) setManual(JSON.parse(storedJson) as Pos)
  }, [storedJson])
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const persist = useCallback((next: Pos) => {
    dirtyRef.current = true
    setManual(next)
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      const merged = { ...(prefs[PREF_KEY] ?? {}) }
      if (Object.keys(next).length) merged[projectId] = next; else delete merged[projectId]
      const keys = Object.keys(merged)
      if (keys.length > MAX_PROJECTS) for (const k of keys.slice(0, keys.length - MAX_PROJECTS)) delete merged[k]
      update({ [PREF_KEY]: merged }).catch(() => { /* offline: the layout stays local */ }).finally(() => { dirtyRef.current = false })
    }, 600)
  }, [prefs, projectId, update])

  // Time-scaled mode (X = schedule) — remembered locally per project. In this
  // mode positions and widths come from the schedule, not from hand-placement.
  const timescaleKey = `office:net:timescale:${projectId}`
  const [timescale, setTimescale] = useState<boolean>(() => { try { return localStorage.getItem(timescaleKey) === '1' } catch { return false } })
  const toggleTimescale = () => setTimescale(s => { const n = !s; try { localStorage.setItem(timescaleKey, n ? '1' : '0') } catch { /* quota */ } return n })

  // Container size — the visible world range (for an "infinite" time ruler).
  const [size, setSize] = useState({ w: 0, h: 0 })
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const ro = new ResizeObserver(es => { const r = es[0].contentRect; setSize({ w: r.width, h: r.height }) })
    ro.observe(el)
    setSize({ w: el.clientWidth, h: el.clientHeight })
    return () => ro.disconnect()
  }, [])

  const auto = useMemo(() => autoLayout(tasks, nodes, edges), [tasks, nodes, edges])
  const time = useMemo(() => timeLayout(tasks, nodes), [tasks, nodes])
  const posOf = useCallback((id: string): Pt => timescale ? (time.pos[id] ?? { x: PAD, y: PAD }) : (manual[id] ?? auto[id] ?? { x: PAD, y: PAD }), [timescale, time, manual, auto])
  const widthOf = useCallback((id: string): number => timescale ? (time.width[id] ?? MIN_BAR_W) : NODE_W, [timescale, time])
  const nodeIds = useMemo(() => new Set(nodes.map(n => n.id)), [nodes])
  const hulls = useMemo(() => computeHulls(tasks, nodeIds, posOf, widthOf), [tasks, nodeIds, posOf, widthOf])

  // ── Viewport (pan + zoom), remembered locally per project ──
  const vpKey = `office:net:vp:${projectId}`
  const [vp, setVp] = useState<Viewport>(() => {
    try {
      const v = JSON.parse(localStorage.getItem(vpKey) || 'null') as Viewport | null
      if (v && Number.isFinite(v.tx) && Number.isFinite(v.ty) && v.scale >= 0.25 && v.scale <= 2.5) return v
    } catch { /* invalid JSON → default */ }
    return { tx: 0, ty: 0, scale: 1 }
  })
  useEffect(() => {
    const h = setTimeout(() => { try { localStorage.setItem(vpKey, JSON.stringify(vp)) } catch { /* quota */ } }, 300)
    return () => clearTimeout(h)
  }, [vp, vpKey])
  const toWorld = useCallback((clientX: number, clientY: number): Pt => {
    const r = containerRef.current!.getBoundingClientRect()
    return { x: (clientX - r.left - vp.tx) / vp.scale, y: (clientY - r.top - vp.ty) / vp.scale }
  }, [vp])
  const zoomBy = (f: number) => setVp(v => ({ ...v, scale: Math.min(2.5, Math.max(0.25, v.scale * f)) }))

  // Month ticks for the time ruler, generated across the VISIBLE viewport (so the
  // ruler seems to extend infinitely on both sides as you pan). Each tick carries
  // its world X (for gridlines behind the cards) and its screen X (for the header
  // band, which is pinned to the top of the container).
  const monthTicks = useMemo(() => {
    if (!timescale || size.w <= 0) return [] as { worldX: number; screenX: number; label: string }[]
    const dayAtScreenX = (sx: number) => time.minDay + ((sx - vp.tx) / vp.scale - PAD) / DAY_W
    const dLeft = Math.floor(dayAtScreenX(0)) - 2, dRight = Math.ceil(dayAtScreenX(size.w)) + 2
    const first = addDays(projectStart, dLeft), last = addDays(projectStart, dRight)
    const ticks: { worldX: number; screenX: number; label: string }[] = []
    let cur = new Date(first.getFullYear(), first.getMonth(), 1)
    for (let guard = 0; cur <= last && guard < 600; guard++) {
      const off = differenceInDays(cur, projectStart)
      const worldX = PAD + (off - time.minDay) * DAY_W
      ticks.push({ worldX, screenX: vp.tx + worldX * vp.scale, label: formatDate(cur, { month: 'short', year: '2-digit' }) })
      cur = new Date(cur.getFullYear(), cur.getMonth() + 1, 1)
    }
    return ticks
  }, [timescale, size.w, vp, time.minDay, projectStart])
  const gridBottom = useMemo(() => timescale
    ? Math.max(PAD + RULER_H + ROW_H, ...nodes.map(n => posOf(n.id).y + NODE_H), ...hulls.map(h => h.y + h.h)) + PAD
    : 0, [timescale, nodes, hulls, posOf])

  // Snap-to-grid while dragging cards — remembered locally per project.
  const snapKey = `office:net:snap:${projectId}`
  const [snap, setSnap] = useState<boolean>(() => { try { return localStorage.getItem(snapKey) === '1' } catch { return false } })
  const toggleSnap = () => setSnap(s => { const n = !s; try { localStorage.setItem(snapKey, n ? '1' : '0') } catch { /* quota */ } return n })
  const fitToContent = useCallback(() => {
    const el = containerRef.current
    if (!el || !nodes.length) { setVp({ tx: 0, ty: 0, scale: 1 }); return }
    const boxes = [...nodes.map(n => { const p = posOf(n.id); return { x: p.x, y: p.y, w: widthOf(n.id), h: NODE_H } }), ...hulls]
    const minX = Math.min(...boxes.map(b => b.x)) - PAD, minY = Math.min(...boxes.map(b => b.y)) - PAD
    const maxX = Math.max(...boxes.map(b => b.x + b.w)) + PAD, maxY = Math.max(...boxes.map(b => b.y + b.h)) + PAD
    const r = el.getBoundingClientRect()
    const bw = maxX - minX || 1, bh = maxY - minY || 1
    const scale = Math.min(1.5, Math.max(0.25, Math.min(r.width / bw, r.height / bh)))
    setVp({ tx: (r.width - bw * scale) / 2 - minX * scale, ty: (r.height - bh * scale) / 2 - minY * scale, scale })
  }, [nodes, hulls, posOf, widthOf])
  // Re-frame when the layout mode flips (time-scaled spreads far wider).
  const prevTsRef = useRef(timescale)
  useEffect(() => { if (prevTsRef.current !== timescale) { prevTsRef.current = timescale; fitToContent() } }, [timescale, fitToContent])
  // First opening of a project: frame the whole network.
  const fittedRef = useRef(false)
  useEffect(() => {
    if (fittedRef.current || !nodes.length) return
    fittedRef.current = true
    if (!localStorage.getItem(vpKey)) fitToContent()
  }, [nodes.length, vpKey, fitToContent])
  // React registers `wheel` as passive: a native listener is needed to keep
  // Ctrl+wheel from zooming the whole page.
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const h = (e: WheelEvent) => e.preventDefault()
    el.addEventListener('wheel', h, { passive: false })
    return () => el.removeEventListener('wheel', h)
  }, [])
  const onWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      const rect = containerRef.current!.getBoundingClientRect()
      const cx = e.clientX - rect.left, cy = e.clientY - rect.top
      const factor = e.deltaY > 0 ? 0.9 : 1.1
      setVp(v => { const ns = Math.min(2.5, Math.max(0.25, v.scale * factor)); const k = ns / v.scale; return { scale: ns, tx: cx - (cx - v.tx) * k, ty: cy - (cy - v.ty) * k } })
      return
    }
    setVp(v => ({ ...v, tx: v.tx - e.deltaX, ty: v.ty - e.deltaY }))
  }

  // ── Gestures: pan the background, drag a card, draw a link ──
  const pan = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null)
  const drag = useRef<{ selectId: string; items: { id: string; orig: Pt }[]; startX: number; startY: number; moved: boolean } | null>(null)
  const [connect, setConnect] = useState<{ source: string; sx: number; sy: number; end: Pt; over: string | null } | null>(null)
  const [grabbing, setGrabbing] = useState(false)
  const [hoverEdge, setHoverEdge] = useState<string | null>(null)
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const flash = (text: string) => { setNotice(text); if (noticeTimer.current) clearTimeout(noticeTimer.current); noticeTimer.current = setTimeout(() => setNotice(null), 2800) }

  const onBgPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0 && e.button !== 1) return
    if (e.button === 0 && !(e.target as HTMLElement).dataset.bg) return
    e.preventDefault()
    pan.current = { x: e.clientX, y: e.clientY, tx: vp.tx, ty: vp.ty }
    setGrabbing(true)
    containerRef.current?.setPointerCapture?.(e.pointerId)
  }
  const onNodePointerDown = (e: React.PointerEvent, id: string) => {
    if (e.button !== 0) return
    e.stopPropagation()
    drag.current = { selectId: id, items: [{ id, orig: posOf(id) }], startX: e.clientX, startY: e.clientY, moved: false }
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
  }
  /** Grabbing a phase's header moves every card inside it. */
  const onHullPointerDown = (e: React.PointerEvent, hull: Hull) => {
    if (e.button !== 0) return
    e.stopPropagation()
    drag.current = { selectId: hull.id, items: hull.ids.map(id => ({ id, orig: posOf(id) })), startX: e.clientX, startY: e.clientY, moved: false }
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
  }
  const onPortPointerDown = (e: React.PointerEvent, id: string) => {
    if (e.button !== 0 || !canEdit) return
    e.stopPropagation()
    const p = posOf(id)
    setConnect({ source: id, sx: p.x + widthOf(id), sy: p.y + NODE_H / 2, end: toWorld(e.clientX, e.clientY), over: null })
    containerRef.current?.setPointerCapture?.(e.pointerId)
  }
  const onPointerMove = (e: React.PointerEvent) => {
    if (connect) {
      const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null
      const over = (el?.closest?.('[data-node]') as HTMLElement | null)?.dataset.node ?? null
      setConnect(c => c && { ...c, end: toWorld(e.clientX, e.clientY), over: over && over !== c.source ? over : null })
    } else if (drag.current) {
      // In time-scaled mode positions are schedule-bound: a card cannot be moved
      // (the press still selects on release, since `moved` stays false).
      if (timescale) return
      const d = drag.current
      let dx = (e.clientX - d.startX) / vp.scale, dy = (e.clientY - d.startY) / vp.scale
      if (!d.moved && Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return
      d.moved = true
      // Snap the whole group by its leader so the cards keep their relative
      // offsets and the leader lands on a grid line.
      if (snap && d.items.length) {
        const lead = d.items[0]
        dx = snapTo(lead.orig.x + dx) - lead.orig.x
        dy = snapTo(lead.orig.y + dy) - lead.orig.y
      }
      setManual(m => { const next = { ...m }; for (const it of d.items) next[it.id] = { x: Math.round(it.orig.x + dx), y: Math.round(it.orig.y + dy) }; return next })
    } else if (pan.current) {
      const p = pan.current
      setVp(v => ({ ...v, tx: p.tx + (e.clientX - p.x), ty: p.ty + (e.clientY - p.y) }))
    }
  }
  const onPointerUp = () => {
    if (connect) {
      const c = connect
      setConnect(null)
      if (c.over) {
        void onConnect(c.source, c.over).then(ok => {
          if (!ok) flash(t('proj_net_link_refused', { defaultValue: 'Liaison impossible : elle fermerait une boucle.' }))
        })
      }
    }
    if (drag.current) {
      const d = drag.current
      drag.current = null
      if (d.moved) persist(manualRef.current)
      else onSelect(d.selectId === selectedId ? null : d.selectId)
    }
    pan.current = null
    setGrabbing(false)
  }
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { setConnect(null); drag.current = null } }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // ── Menus ──
  const openMenu = (e: React.MouseEvent, items: MenuItem[]) => { e.preventDefault(); e.stopPropagation(); setMenu({ x: e.clientX, y: e.clientY, items }) }
  const depTypeLabel = (ty: TaskDependency['dep_type']) => ({
    FS: t('proj_dep_fs', { defaultValue: 'Fin → Début (FS)' }),
    SS: t('proj_dep_ss', { defaultValue: 'Début → Début (SS)' }),
    FF: t('proj_dep_ff', { defaultValue: 'Fin → Fin (FF)' }),
    SF: t('proj_dep_sf', { defaultValue: 'Début → Fin (SF)' }),
  })[ty]
  const edgeMenu = (d: TaskDependency): MenuItem[] => [
    { type: 'label', text: `${byId.get(d.from_task_id)?.name ?? '?'} → ${byId.get(d.to_task_id)?.name ?? '?'}` },
    { type: 'submenu', label: t('proj_dep_type', { defaultValue: 'Type de liaison' }), icon: <Link2 size={14} />, items: DEP_TYPES.map((ty): MenuItem => ({
      type: 'action', label: depTypeLabel(ty), checked: d.dep_type === ty, disabled: !canEdit, onClick: () => { if (ty !== d.dep_type) onSetDep(d, { dep_type: ty }) },
    })) },
    { type: 'action', label: t('proj_dep_lag_menu', { defaultValue: 'Décalage… ({{n}} j)', n: d.lag_days }), icon: <Timer size={14} />, disabled: !canEdit, onClick: () => {
      void (async () => {
        const v = await prompt({
          title: t('proj_dep_lag', { defaultValue: 'Décalage' }),
          message: t('proj_dep_lag_msg', { defaultValue: 'Jours de décalage (négatif = avance) :' }),
          defaultValue: String(d.lag_days),
          confirmLabel: t('common_save', { defaultValue: 'Enregistrer' }),
        })
        if (v == null) return
        const n = parseInt(v, 10)
        if (Number.isFinite(n) && n !== d.lag_days) onSetDep(d, { lag_days: n })
      })()
    } },
    { type: 'separator' },
    { type: 'action', label: t('proj_dep_delete', { defaultValue: 'Supprimer la liaison' }), icon: <Scissors size={14} />, disabled: !canEdit, onClick: () => onDeleteDep(d) },
  ]
  const canvasMenu = (): MenuItem[] => [
    { type: 'action', label: t('proj_net_timescale', { defaultValue: 'Échelle de temps' }), icon: <CalendarClock size={14} />, checked: timescale, onClick: toggleTimescale },
    { type: 'action', label: t('proj_net_snap', { defaultValue: 'Magnétisme sur la grille' }), icon: <Magnet size={14} />, checked: snap, disabled: timescale, onClick: toggleSnap },
    { type: 'separator' },
    { type: 'action', label: t('proj_net_fit', { defaultValue: 'Ajuster à l’écran' }), icon: <Maximize size={14} />, onClick: fitToContent },
    { type: 'action', label: t('proj_net_reset_view', { defaultValue: 'Réinitialiser la vue' }), icon: <RotateCcw size={14} />, onClick: () => setVp({ tx: 0, ty: 0, scale: 1 }) },
    { type: 'separator' },
    { type: 'action', label: t('proj_net_rearrange', { defaultValue: 'Réorganiser automatiquement' }), icon: <LayoutGrid size={14} />, disabled: timescale || !Object.keys(manual).length, onClick: () => persist({}) },
  ]

  // ── Edge geometry ──
  const routes = useMemo(() => {
    const m = new Map<string, Pt[]>()
    for (const d of edges) {
      const a = posOf(d.from_task_id), b = posOf(d.to_task_id)
      m.set(d.id, orthRoute({ x: a.x + widthOf(d.from_task_id), y: a.y + NODE_H / 2 }, { x: b.x, y: b.y + NODE_H / 2 }))
    }
    return m
  }, [edges, posOf, widthOf])
  const edgeLabel = (d: TaskDependency) => `${d.dep_type !== 'FS' ? d.dep_type : ''}${d.lag_days ? `${d.dep_type !== 'FS' ? ' ' : ''}${d.lag_days > 0 ? '+' : '−'}${Math.abs(d.lag_days)} j` : ''}`

  if (nodes.length === 0) {
    return <div className="flex-1 flex items-center justify-center text-sm text-text-tertiary italic">{t('proj_no_tasks_hint', { defaultValue: 'Aucune tâche à afficher' })}</div>
  }

  return (
    <div
      ref={containerRef}
      data-bg="1"
      className={cn('relative flex-1 min-h-0 overflow-hidden bg-surface-1 select-none', grabbing ? 'cursor-grabbing' : 'cursor-default')}
      style={{ touchAction: 'none', backgroundImage: 'radial-gradient(var(--color-border, #c4c7cc) 1px, transparent 1px)', backgroundSize: `${24 * vp.scale}px ${24 * vp.scale}px`, backgroundPosition: `${vp.tx}px ${vp.ty}px` }}
      onWheel={onWheel}
      onPointerDown={onBgPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onAuxClick={e => { if (e.button === 1) e.preventDefault() }}
      onContextMenu={e => { if ((e.target as HTMLElement).dataset.bg) openMenu(e, canvasMenu()) }}
    >
      <div style={{ position: 'absolute', transform: `translate(${vp.tx}px, ${vp.ty}px) scale(${vp.scale})`, transformOrigin: '0 0' }}>
        {/* Month gridlines (time-scaled mode) — behind the cards, in world space
            so they pan and zoom with the diagram. The month labels themselves are
            drawn in the pinned top band below (foreground). */}
        {timescale && monthTicks.map((m, i) => (
          <div key={`grid-${i}`} className="absolute border-l border-border/50 pointer-events-none"
            style={{ left: m.worldX, top: 0, height: gridBottom }} />
        ))}
        {/* Phase frames (behind everything): the body lets links, cards and the
            background through; only the header takes the pointer. */}
        {hulls.map(h => {
          const sel = h.id === selectedId
          const pct = progressMap.get(h.id)
          return (
            <div key={h.id} data-hull={h.id}
              className={cn('absolute rounded-xl border pointer-events-none', sel ? 'border-primary ring-2 ring-primary/40' : 'border-primary/25')}
              style={{ left: h.x, top: h.y, width: h.w, height: h.h, background: 'color-mix(in srgb, var(--color-primary, #1a73e8) 5%, transparent)' }}>
              <div
                className="absolute left-0 top-0 right-0 flex items-center gap-1.5 px-2.5 text-[11px] font-semibold text-primary truncate pointer-events-auto cursor-grab active:cursor-grabbing"
                style={{ height: HULL_HEAD }}
                title={t('proj_net_phase_hint', { defaultValue: 'Phase — glisser pour déplacer toutes ses tâches' })}
                onPointerDown={e => onHullPointerDown(e, h)}
                onContextMenu={e => { onSelect(h.id); onContextMenu(e, h.id) }}
              >
                <span className="truncate">{h.name}</span>
                {pct != null && <span className="font-normal opacity-70">· {pct}%</span>}
              </div>
            </div>
          )
        })}

        {/* Links */}
        <svg style={{ position: 'absolute', overflow: 'visible', pointerEvents: 'none', left: 0, top: 0 }}>
          <defs>
            <marker id="net-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="context-stroke" /></marker>
          </defs>
          {edges.map(d => {
            const pts = routes.get(d.id)!
            const crit = !!byId.get(d.from_task_id)?.is_critical && !!byId.get(d.to_task_id)?.is_critical
            const hovered = hoverEdge === d.id
            const color = hovered ? 'var(--color-primary, #1a73e8)' : crit ? CRITICAL_CLR : '#9aa0a6'
            const mid = midPoint(pts)
            const label = edgeLabel(d)
            // The context menu sits on the group: a right-click on the cut
            // button (which covers the link's midpoint) must open it too.
            return (
              <g key={d.id} onPointerEnter={() => setHoverEdge(d.id)} onPointerLeave={() => setHoverEdge(h => h === d.id ? null : h)}
                onContextMenu={ev => openMenu(ev, edgeMenu(d))}>
                <path d={roundedPath(pts)} stroke={color} strokeWidth={crit || hovered ? 2 : 1.5} fill="none" markerEnd="url(#net-arrow)" />
                <path d={roundedPath(pts)} stroke="transparent" strokeWidth={16} fill="none" style={{ pointerEvents: 'stroke', cursor: 'pointer' }} />
                {label && !hovered && (
                  <g style={{ pointerEvents: 'none' }}>
                    <rect x={mid.x - 18} y={mid.y - 8} width={36} height={16} rx={8} fill="#ffffff" stroke={crit ? CRITICAL_CLR : '#dadce0'} />
                    <text x={mid.x} y={mid.y + 3.5} textAnchor="middle" fontSize={9} fill="#5f6368" fontFamily="inherit">{label}</text>
                  </g>
                )}
                {/* Cut button on hover (Flow's mid-link affordance). */}
                {hovered && canEdit && (
                  <g style={{ pointerEvents: 'all', cursor: 'pointer' }} onClick={ev => { ev.stopPropagation(); onDeleteDep(d) }}>
                    <title>{t('proj_dep_delete', { defaultValue: 'Supprimer la liaison' })}</title>
                    <circle cx={mid.x} cy={mid.y} r={9} fill="#d93025" />
                    <path d={`M ${mid.x - 3.5} ${mid.y - 3.5} L ${mid.x + 3.5} ${mid.y + 3.5} M ${mid.x + 3.5} ${mid.y - 3.5} L ${mid.x - 3.5} ${mid.y + 3.5}`} stroke="#fff" strokeWidth={1.8} />
                  </g>
                )}
              </g>
            )
          })}
          {connect && (
            <path d={roundedPath(orthRoute({ x: connect.sx, y: connect.sy }, connect.over ? { x: posOf(connect.over).x, y: posOf(connect.over).y + NODE_H / 2 } : connect.end))}
              stroke="var(--color-primary, #1a73e8)" strokeWidth={2} strokeDasharray="5 4" fill="none" markerEnd="url(#net-arrow)" />
          )}
        </svg>

        {/* Cards */}
        {nodes.map(n => {
          const p = posOf(n.id)
          const sel = n.id === selectedId
          const clr = n.task_type === 'milestone' ? MILESTONE_CLR : n.is_critical ? CRITICAL_CLR : TASK_COLOR
          const pct = progressMap.get(n.id) ?? n.progress
          const target = connect?.over === n.id
          const hasIn = edges.some(d => d.to_task_id === n.id), hasOut = edges.some(d => d.from_task_id === n.id)
          const fl = n.total_float
          return (
            <div key={n.id} data-node={n.id}
              className={cn('absolute rounded-lg bg-surface-0 border shadow-sm transition-shadow', timescale ? 'cursor-pointer' : 'cursor-grab active:cursor-grabbing',
                sel ? 'ring-2 ring-primary border-primary' : 'border-border', target && 'ring-2 ring-primary shadow-lg')}
              style={{ left: p.x, top: p.y, width: widthOf(n.id), height: NODE_H }}
              onPointerDown={e => onNodePointerDown(e, n.id)}
              onContextMenu={e => { onSelect(n.id); onContextMenu(e, n.id) }}
            >
              <div className="h-1.5 rounded-t-lg" style={{ background: clr }} />
              <div className="px-2.5 pt-1.5 text-[11px] font-semibold text-text-primary truncate" title={n.name}>{n.name}</div>
              <div className="px-2.5 text-[10px] text-text-secondary truncate">{formatDate(schedStart(n, projectStart), { day: 'numeric', month: 'short' })} → {formatDate(schedEnd(n, projectStart), { day: 'numeric', month: 'short' })}</div>
              <div className="px-2.5 flex items-center justify-between text-[10px] text-text-secondary">
                <span>{t('proj_days_short', { count: n.duration_days })} · {pct}%</span>
                {fl != null && n.task_type !== 'milestone' && (
                  <span className={cn('rounded px-1', fl <= 0 ? 'bg-danger/10 text-danger' : 'bg-surface-2 text-text-tertiary')}
                    title={t('proj_net_float_hint', { defaultValue: 'Marge totale' })}>{t('proj_net_float', { defaultValue: 'marge {{n}} j', n: fl })}</span>
                )}
              </div>
              {/* Ports: the left one receives links, the right one starts them. */}
              <span className="absolute rounded-full border-2 bg-surface-0" style={{ left: -PORT_R, top: NODE_H / 2 - PORT_R, width: PORT_R * 2, height: PORT_R * 2, borderColor: hasIn ? clr : '#9aa0a6', background: hasIn ? clr : undefined }} />
              <span
                className={cn('absolute rounded-full border-2 bg-surface-0', canEdit && 'cursor-crosshair hover:scale-125')}
                style={{ right: -PORT_R, top: NODE_H / 2 - PORT_R, width: PORT_R * 2, height: PORT_R * 2, borderColor: hasOut ? clr : '#9aa0a6', background: hasOut ? clr : undefined, transition: 'transform .1s' }}
                title={canEdit ? t('proj_net_port_hint', { defaultValue: 'Glisser vers une autre tâche pour la lier' }) : undefined}
                onPointerDown={e => onPortPointerDown(e, n.id)}
              />
            </div>
          )
        })}
      </div>

      {/* Time ruler — pinned to the TOP of the container (foreground, above every
          card), spanning the full width. Month labels follow the horizontal
          pan/zoom and are generated across the visible range, so the ruler reads
          as if it extended infinitely on both sides. */}
      {timescale && (
        <div className="absolute left-0 top-0 right-0 z-30 overflow-hidden border-b border-border bg-surface-0/90 backdrop-blur-sm pointer-events-none" style={{ height: RULER_H }}>
          {monthTicks.map((m, i) => (
            <div key={i} className="absolute top-0 h-full flex items-center" style={{ left: m.screenX }}>
              <span className="absolute left-0 top-0 h-full w-px bg-border/70" />
              <span className="pl-1.5 text-[11px] font-medium text-text-secondary capitalize whitespace-nowrap">{m.label}</span>
            </div>
          ))}
        </div>
      )}

      {connect && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-40 bg-primary text-white text-xs px-3 py-1 rounded-full shadow pointer-events-none">
          {t('proj_net_connect_hint', { defaultValue: 'Relâchez sur la tâche qui doit suivre' })}
        </div>
      )}
      {notice && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-40 bg-danger text-white text-xs px-3 py-1 rounded-full shadow pointer-events-none">{notice}</div>
      )}

      {/* Zoom / layout controls */}
      <div className="absolute bottom-3 right-3 flex items-center gap-1 bg-surface-0 border border-border rounded-lg px-2 py-1 text-text-secondary text-xs" onPointerDown={e => e.stopPropagation()}>
        <button className="px-1.5 hover:text-text-primary" onClick={() => zoomBy(0.9)}>−</button>
        <span className="w-10 text-center">{Math.round(vp.scale * 100)}%</span>
        <button className="px-1.5 hover:text-text-primary" onClick={() => zoomBy(1.1)}>+</button>
        <button className={cn('px-1.5 hover:text-text-primary', timescale && 'text-primary')} onClick={toggleTimescale} aria-pressed={timescale}
          title={timescale ? t('proj_net_timescale_on', { defaultValue: 'Échelle de temps : activée' }) : t('proj_net_timescale_off', { defaultValue: 'Échelle de temps : positionner les tâches par date' })}><CalendarClock size={13} /></button>
        <button className={cn('px-1.5 hover:text-text-primary disabled:opacity-40', snap && !timescale && 'text-primary')} onClick={toggleSnap} disabled={timescale} aria-pressed={snap}
          title={timescale ? t('proj_net_snap_na', { defaultValue: 'Magnétisme indisponible en échelle de temps' }) : snap ? t('proj_net_snap_on', { defaultValue: 'Magnétisme sur la grille : activé' }) : t('proj_net_snap_off', { defaultValue: 'Magnétisme sur la grille : désactivé' })}><Magnet size={13} /></button>
        <button className="px-1.5 hover:text-text-primary" onClick={fitToContent} title={t('proj_net_fit', { defaultValue: 'Ajuster à l’écran' })}><Maximize size={13} /></button>
        <button className="px-1.5 hover:text-text-primary disabled:opacity-40" disabled={timescale || !Object.keys(manual).length} onClick={() => persist({})} title={t('proj_net_rearrange', { defaultValue: 'Réorganiser automatiquement' })}><LayoutGrid size={13} /></button>
      </div>

      {menu && <MenuDropdown items={menu.items} pos={{ top: menu.y, left: menu.x }} onClose={() => setMenu(null)} />}
    </div>
  )
}
