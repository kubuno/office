import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import {
  Plus, Trash2,
  Users, BarChart2, Link2, Link2Off, Milestone, Flag,
  Loader2, AlertTriangle, Star, FolderKanban,
  Indent, Outdent, ZoomIn, ZoomOut, Info, Share2, GanttChartSquare,
  ChevronRight, ChevronDown, ListChecks, CalendarRange,
  Copy, ArrowUp, ArrowDown, ChevronsDownUp, ChevronsUpDown,
  CheckCircle2, Circle, Filter, KanbanSquare, CalendarDays, Download, BarChart3, Network,
  FilePlus, CopyPlus, SlidersHorizontal, FileOutput, RotateCcw, ScrollText, ListTree, Package, ClipboardList, Waypoints, ShieldAlert, TriangleAlert, TrendingUp, Receipt, UsersRound, Grid3x3, BadgeCheck, Megaphone, Gavel, GitPullRequestArrow, FlagTriangleRight, Handshake, BookOpen,
} from 'lucide-react'
import { Dropdown, Button, Input, Textarea, Checkbox, MenuDropdown, useMenuDropdown, RangeSlider, useIsMobile, Tabs, type TabDef, type MenuItem } from '@ui'
import { DockArea, prompt, type DockPanel, type DockController } from '@kubuno/sdk'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { projectsApi, officeApi, type ProjectTask, type TaskDependency, type ProjectResource, type Project, type ProjectArtifactKey } from './api'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_PROJECTS } from './ribbon/officeThemes'
import { genericClipboardGroup } from './ribbon/clipboardGroup'
import { SaveButton } from './ribbon/SaveButton'
import { UndoRedoButtons } from './ribbon/UndoRedoButtons'
import { useEditHistory, type HistoryCtx, type HistoryEntry } from './history/useEditHistory'
import { useFileTab, backstageLabels, BackstageInfo } from './ribbon/ModuleBackstage'
import { ProjectsStartContent } from './ProjectsStartContent'
import type { RibbonTab, RibbonItem, RibbonGroup } from './ribbon/types'
import ProjectSettingsPanel from './project/ProjectSettingsPanel'
import DocumentProductionPanel from './project/DocumentProductionPanel'
import { format, addDays, differenceInCalendarDays, startOfMonth, addMonths, startOfWeek, isSameMonth, isSameDay } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { userColor, PresenceAvatars, RemoteCursors, usePublishCursor } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import CollaboratorsDialog from './CollaboratorsDialog'
import TaskDetailPanel from './project/TaskDetailPanel'
import TaskRow from './project/TaskRow'
import { effectiveProgress, parentIds } from './project/rollup'
import { calendarFromApi } from './project/workingCalendar'
import { useOfficeInstance } from './useOfficeInstance'
import TimelineBand from './project/TimelineBand'
import ResourcesView from './project/resources/ResourcesView'
import ProjectPropertiesPanel from './project/ProjectPropertiesPanel'
import RoadmapView from './project/RoadmapView'
import CharterView from './project/CharterView'
import WbsView from './project/WbsView'
import DeliverablesView from './project/DeliverablesView'
import RequirementsView from './project/RequirementsView'
import TraceabilityView from './project/TraceabilityView'
import RiskRegisterView from './project/RiskRegisterView'
import IssueLogView from './project/IssueLogView'
import EarnedValueView from './project/EarnedValueView'
import CostEntriesView from './project/CostEntriesView'
import RaciMatrixView from './project/RaciMatrixView'
import StakeholdersView from './project/StakeholdersView'
import QualityView from './project/QualityView'
import CommunicationsView from './project/CommunicationsView'
import DecisionLogView from './project/DecisionLogView'
import ChangeControlView from './project/ChangeControlView'
import ClosureView from './project/ClosureView'
import ProcurementView from './project/ProcurementView'
import ManagementPlansView from './project/ManagementPlansView'
import { GanttRenderer, ROW_H, HEADER_H, MIN_DAYS, VISIBLE_LEAD, DRAG_MARGIN, TIMELINE_H, TASK_COLOR, CRITICAL_CLR, MILESTONE_CLR, SUMMARY_CLR, GRID_CLR, PROGRESS_CLR, ZOOM_DAYW } from './project/GanttRenderer'
import type { ZoomLevel } from './project/GanttRenderer'
import { schedStart, schedEnd } from './project/schedule'
import { useGanttColumns, colStyle, isColResizable, isColHideable, GANTT_COL_IDS, type GanttColId } from './project/ganttTableConstants'
import { MobilePanelSheet } from './shell/MobilePanelSheet'
import { MobileTaskList, MobileTaskSummary } from './project/MobileTaskList'
import { PenLine, Eye } from 'lucide-react'

// ── Constants ─────────────────────────────────────────────────────────────────


// Colonnes de la table (largeurs fixes, façon MS Project).

// ── GanttRenderer ─────────────────────────────────────────────────────────────


// ── Dates planifiées (cohérentes avec la barre = offset CPM) ───────────────────


// ── Undo/redo helpers (server-backed history) ─────────────────────────────────

/** A partial PATCH body — both sides ("before"/"after") of an undoable edit. */
type EditPatch = Record<string, unknown>

/** Fields edited as a CONTINUOUS stream (typing, slider, number spinner): the whole
 *  gesture collapses into a single history entry. */
const COALESCED_FIELDS = new Set(['name', 'description', 'progress', 'duration_days'])

/** A stable avatar colour for a directory member added as a resource. */
function memberColor(userId: string): string {
  const palette = ['#1a73e8', '#188038', '#e37400', '#9334e6', '#d93025', '#00897b', '#c2185b', '#5f6368']
  let h = 0
  for (const ch of userId) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return palette[h % palette.length]
}
/** Idle time after which a continuous gesture is considered finished. */
const COALESCE_MS = 900

/** Everything of a task the API is able to restore (create + update payloads). */
type TaskSnapshot = {
  id:            string
  parent_id:     string | null
  position:      number
  wbs:           string
  name:          string
  description:   string
  status:        string
  priority:      string
  task_type:     string
  start_date:    string | null
  end_date:      string | null
  duration_days: number
  progress:      number
}

const snapshotTask = (tk: ProjectTask): TaskSnapshot => ({
  id: tk.id, parent_id: tk.parent_id ?? null, position: tk.position, wbs: tk.wbs,
  name: tk.name, description: tk.description, status: tk.status, priority: tk.priority,
  task_type: tk.task_type, start_date: tk.start_date, end_date: tk.end_date,
  duration_days: tk.duration_days, progress: tk.progress,
})

/**
 * Re-creates a task the server deleted. `createTask` only accepts a handful of
 * fields, so the rest is applied with a follow-up PATCH. Returns the NEW server id:
 * the caller MUST remap it (`HistoryCtx.remap`) so later entries keep resolving.
 */
async function recreateTask(projectId: string, snap: TaskSnapshot, parentId: string | null): Promise<string> {
  const created = await projectsApi.createTask(projectId, {
    name:          snap.name,
    parent_id:     parentId ?? undefined,
    position:      snap.position,
    task_type:     snap.task_type,
    start_date:    snap.start_date ?? undefined,
    duration_days: snap.duration_days,
  })
  await projectsApi.updateTask(projectId, created.id, {
    description: snap.description, status: snap.status, priority: snap.priority,
    end_date: snap.end_date ?? undefined, progress: snap.progress,
    position: snap.position, wbs: snap.wbs,
  } as never)
  return created.id
}

// ── Task Row ──────────────────────────────────────────────────────────────────


// ── Task Detail Panel ─────────────────────────────────────────────────────────

// ── Main Editor ───────────────────────────────────────────────────────────────

/** Hourly rate of a resource — committed on blur: one PATCH per edit, not one
 *  per keystroke. Tri-state on purpose: an EMPTY field sends `null`, which falls
 *  back to the project's default rate. That is NOT a rate of zero — a resource
 *  nobody priced and a resource that costs nothing say different things, and the
 *  cost engine reads them differently. */
function ResourceRateField({ rate, onCommit }: { rate: number | null; onCommit: (next: number | null) => void }) {
  const { t } = useTranslation('office')
  const [draft, setDraft] = useState(rate == null ? '' : String(rate))
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(rate == null ? '' : String(rate)) }, [rate])
  return (
    <Input
      type="number" min="0" step="0.01"
      className="w-20 text-right tabular-nums"
      value={draft}
      placeholder={t('proj_resource_rate_ph', { defaultValue: '—' })}
      title={t('proj_resource_rate_hint', { defaultValue: 'Coût horaire. Vide = taux par défaut du projet.' })}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        const n = parseFloat(draft)
        const next = Number.isFinite(n) ? n : null
        if (next === rate) return
        onCommit(next)
      }}
    />
  )
}

type Translate = (key: string, opts?: Record<string, unknown>) => string

/** Header cell of each column (the body cells live in `TaskRow`). */
function ganttHeaderCells(t: Translate): Record<GanttColId, { label: React.ReactNode; cls: string; title?: string }> {
  return {
    idx:      { label: '#', cls: 'justify-center' },
    mode:     { label: <GanttChartSquare size={13} />, cls: 'justify-center', title: t('proj_col_mode', { defaultValue: 'Mode' }) },
    name:     { label: t('proj_col_task'), cls: 'px-1.5' },
    dur:      { label: t('proj_col_duration'), cls: 'justify-end px-1.5' },
    progress: { label: '%', cls: 'justify-end px-1.5', title: t('proj_col_progress', { defaultValue: 'Avancement' }) },
    priority: { label: t('proj_col_priority', { defaultValue: 'Priorité' }), cls: 'px-1.5' },
    start:    { label: t('proj_col_start', { defaultValue: 'Début' }), cls: 'px-1.5' },
    end:      { label: t('proj_col_end', { defaultValue: 'Fin' }), cls: 'px-1.5' },
    variance: { label: t('proj_col_variance', { defaultValue: 'Écart' }), cls: 'px-1.5', title: t('proj_col_variance_hint', { defaultValue: 'Écart de début vs le plan de référence' }) },
    pred:     { label: t('proj_col_predecessors', { defaultValue: 'Préd.' }), cls: 'px-1.5' },
    res:      { label: t('proj_resources'), cls: 'px-1.5' },
  }
}

/** Plain-text name of a column (header context menu). */
function ganttColLabel(t: Translate, id: GanttColId): string {
  switch (id) {
    case 'idx':      return t('proj_col_number', { defaultValue: 'N°' })
    case 'mode':     return t('proj_col_mode', { defaultValue: 'Mode' })
    case 'name':     return t('proj_col_task')
    case 'dur':      return t('proj_col_duration')
    case 'progress': return t('proj_col_progress', { defaultValue: 'Avancement' })
    case 'priority': return t('proj_col_priority', { defaultValue: 'Priorité' })
    case 'start':    return t('proj_col_start', { defaultValue: 'Début' })
    case 'end':      return t('proj_col_end', { defaultValue: 'Fin' })
    case 'variance': return t('proj_col_variance', { defaultValue: 'Écart' })
    case 'pred':     return t('proj_col_predecessors', { defaultValue: 'Préd.' })
    case 'res':      return t('proj_resources')
  }
}

export default function ProjectEditorPage() {
  const { t, i18n } = useTranslation('office')
  const { id }     = useParams<{ id: string }>()
  const navigate   = useNavigate()
  const qc         = useQueryClient()
  const canvasRef  = useRef<HTMLCanvasElement>(null)
  const renderer   = useRef<GanttRenderer | null>(null)
  const ganttRef   = useRef<HTMLDivElement>(null)
  const ganttInnerRef = useRef<HTMLDivElement>(null)
  const workRef    = useRef<HTMLDivElement>(null)
  const dockRef    = useRef<DockController | null>(null)
  // Ouverture d'un panneau : docking sur desktop, FEUILLE DU BAS sur mobile.
  const openPanel = (which: 'inspector' | 'resources') => {
    if (isMobileViewRef.current) setMobilePanel(which)
    else dockRef.current?.open(which)
  }
  // (ref : `openPanel` est utilisé par le ruban, construit avant `isMobileView`)
  const isMobileViewRef = useRef(false)
  const [scrollLeft, setScrollLeft]   = useState(0)
  // Preview in CALENDAR-day offsets: {start, end} mirror the CPM's ES/EF span
  // (a working calendar can stretch a bar beyond start+duration).
  const [barPreview, setBarPreview]   = useState<{ taskId: string; start: number; end: number } | null>(null)
  const barDragRef = useRef<{
    taskId: string; mode: 'move' | 'resize' | 'resize-l' | 'move-sum'
    grabDayFloat: number
    origStart: number; origEnd: number; origDur: number
    origCType: string; origCDate: string | null
    /** How far the gesture may travel LEFT, in days (negative or 0). For a phase
     *  this is bounded by the MOVABLE leaves, not by the bracket: a locked child
     *  pinned at day 0 must not stop the free ones from sliding back. */
    minDelta: number
    // Pixel origin + activation: a drag only starts once the pointer travelled a
    // few pixels, so a sloppy click can never move a task (critical when zoomed out,
    // where a day is only a handful of pixels wide).
    startCX: number; startCY: number; active: boolean
  } | null>(null)
  // Edge auto-scroll state: direction (-1/0/1), the last pointer position (so the
  // preview can be recomputed while the plan scrolls under a still cursor) and the
  // ticker handle.
  const autoScrollRef = useRef<{ dir: number; clientX: number; clientY: number; timer: number | null }>({ dir: 0, clientX: 0, clientY: 0, timer: null })
  // Dragging one of the two project boundary markers (planned start / target end).
  const [markerPreview, setMarkerPreview] = useState<{ which: 'start' | 'end'; day: number } | null>(null)
  const [hoverMarker, setHoverMarker] = useState<'start' | 'end' | null>(null)
  const markerDragRef = useRef<{ which: 'start' | 'end'; grabDayFloat: number; origDay: number; startCX: number; active: boolean } | null>(null)
  const [linkPreview, setLinkPreview] = useState<{ x1: number; y1: number; x2: number; y2: number } | null>(null)
  const linkDragRef = useRef<{ fromId: string; x1: number; y1: number } | null>(null)
  const [selectedId, setSelectedId]   = useState<string | null>(null)
  // Drag & drop reordering of the task rows. `dragId` = the row being carried;
  // `dropHint` = where it would land. We suppress the browser's translucent drag
  // image (see `dragGhostRef`) and draw our own crisp insertion indicator instead.
  const [dragId, setDragId]           = useState<string | null>(null)
  const [dropHint, setDropHint]       = useState<{ id: string; place: 'before' | 'after' | 'inside' } | null>(null)
  const dragGhostRef = useRef<HTMLImageElement | null>(null)
  useEffect(() => {
    // A 1×1 transparent GIF used as the drag image → no ghost follows the cursor.
    const img = new Image()
    img.src = 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7'
    dragGhostRef.current = img
  }, [])
  const [newResName, setNewResName]   = useState('')
  const [activeTab, setActiveTab]     = useState<'gantt' | 'resources' | 'board' | 'calendar' | 'load' | 'pert' | 'roadmap' | 'charter' | 'wbs' | 'deliverables' | 'requirements' | 'traceability' | 'risks' | 'issues' | 'costs' | 'expenses' | 'stakeholders' | 'raci' | 'quality' | 'communications' | 'decisions' | 'changes' | 'closure' | 'procurement' | 'plans'>('gantt')
  const [filterText, setFilterText]     = useState('')
  const [filterStatus, setFilterStatus] = useState('')
  const [filterPriority, setFilterPriority] = useState('')
  const [sortBy, setSortBy]             = useState('')
  const [groupBy, setGroupBy]           = useState('')
  const [showFilters, setShowFilters]   = useState(false)
  const [zoom, setZoom]               = useState<ZoomLevel>('day')
  const [collapsed, setCollapsed]     = useState<Set<string>>(new Set())
  const [showTimeline, setShowTimeline] = useState(true)
  const [showProps, setShowProps]     = useState(false)
  const [shareOpen, setShareOpen]     = useState(false)
  const [ctxMenu, setCtxMenu]         = useState<{ x: number; y: number; taskId: string } | null>(null)
  const isMobileView = useIsMobile()
  isMobileViewRef.current = isMobileView
  // Mobile : le projet s'ouvre en LECTURE (liste des tâches en plein écran) ;
  // « Modifier » bascule en édition — même modèle que Documents/Présentations/Diagrammes.
  const [mode, setMode] = useState<'read' | 'edit'>(() =>
    typeof window !== 'undefined' && window.matchMedia('(max-width: 1023px)').matches ? 'read' : 'edit')
  // Panneau ouvert en feuille du bas (mobile) : la zone de docking ne tient pas.
  const [mobilePanel, setMobilePanel] = useState<'inspector' | 'resources' | null>(null)
  // Lecture mobile : fiche RÉSUMÉ de la tâche touchée (pas le formulaire d'édition).
  const [summaryId, setSummaryId] = useState<string | null>(null)
  // Task table columns: widths + visibility, remembered per project.
  const cols = useGanttColumns(id)
  const colMenu = useMenuDropdown()
  const memberMenu = useMenuDropdown()
  const dayW = ZOOM_DAYW[zoom]

  // ── Historique d'édition (Annuler / Rétablir) ──
  // Server-backed editor: no local document to snapshot, the stack holds INVERSE
  // API calls (see `history/useEditHistory`). Ids re-minted by the server on an undo
  // that re-creates a task are followed through `ctx.resolve` / `ctx.remap`.
  const history = useEditHistory()
  const historyRef = useRef(history)
  historyRef.current = history
  // A continuous gesture (typing a name, dragging the progress slider, spinning a
  // number field) must produce ONE undo step: while the same `key` keeps firing
  // inside the window, only the "after" side of the live entry moves.
  const coalesceRef = useRef<{ key: string; before: EditPatch; after: EditPatch; at: number } | null>(null)

  const { data, isLoading, isError } = useQuery({
    queryKey: ['project', id],
    queryFn:  () => projectsApi.get(id!),
    enabled:  !!id,
  })

  const project     = data?.project
  const allTasks    = useMemo(() => data?.tasks ?? [], [data])
  const deps        = useMemo(() => data?.dependencies ?? [], [data])
  const resources   = data?.resources ?? []
  const assignments = data?.assignments ?? []

  // ── Versions (roadmap) : chargées pour le sélecteur de version de l'inspecteur ──
  const { data: versions } = useQuery({ queryKey: ['versions', id], queryFn: () => projectsApi.listVersions(id!), enabled: !!id })

  // ── Tailoring : le projet ne montre que ce qu'il utilise ─────────────────────
  // Tant que les réglages ne sont pas chargés, tout est considéré actif : mieux
  // vaut un ruban complet une fraction de seconde qu'un ruban qui se remplit.
  const { data: settings } = useQuery({ queryKey: ['project-settings', id], queryFn: () => projectsApi.getSettings(id!), enabled: !!id })
  const uses = useCallback(
    (key: ProjectArtifactKey) => settings?.artifacts?.[key]?.enabled ?? true,
    [settings],
  )

  // Views the project switched off must not stay on screen. Falls back to the
  // first one still available rather than showing an empty body.
  useEffect(() => {
    const byView: Array<[typeof activeTab, ProjectArtifactKey]> = [
      ['gantt', 'schedule'], ['board', 'board'], ['calendar', 'calendar'],
      ['load', 'workload'], ['pert', 'network'], ['roadmap', 'roadmap'],
      // Last: when a view is switched off the fallback should land on a schedule
      // view, not on the charter (which is a document, not a way to see tasks).
      ['charter', 'charter'], ['wbs', 'wbs'], ['deliverables', 'deliverables'],
      ['requirements', 'requirements'], ['traceability', 'requirements'],
      ['risks', 'risks'], ['issues', 'issues'], ['costs', 'costs'], ['expenses', 'costs'],
      ['stakeholders', 'stakeholders'], ['raci', 'stakeholders'], ['quality', 'quality'],
      ['communications', 'communications'], ['decisions', 'decisions'], ['changes', 'changes'], ['closure', 'closure'], ['procurement', 'procurement'], ['plans', 'plans'],
    ]
    const current = byView.find(([view]) => view === activeTab)
    if (!current || uses(current[1])) return
    const fallback = byView.find(([, key]) => uses(key))
    if (fallback) setActiveTab(fallback[0])
  }, [activeTab, uses])

  // ── Plan de référence (baseline) : photo du planning prévu, comparée au réel ──
  const [activeBaselineId, setActiveBaselineId] = useState<string | null>(null)
  const { data: baselines } = useQuery({
    queryKey: ['baselines', id],
    queryFn:  () => projectsApi.listBaselines(id!),
    enabled:  !!id,
  })
  const activeBaseline = baselines?.find(b => b.id === activeBaselineId) ?? null
  // task_id → { offset de début prévu, durée prévue } — surimposé au Gantt et aux écarts.
  const baselineMap = useMemo(() => {
    if (!activeBaseline) return null
    const m = new Map<string, { es: number; dur: number }>()
    // `dur` is the CALENDAR span (EF−ES) so the ghost aligns with the bars, which
    // are also drawn on the calendar span — not the working-day duration.
    for (const s of activeBaseline.tasks) {
      const es = s.early_start ?? 0
      m.set(s.task_id, { es, dur: Math.max((s.early_finish ?? (es + s.duration_days)) - es, 0) })
    }
    return m
  }, [activeBaseline])
  // Who is on each task → drawn as avatars on the Gantt bars, so the responsible
  // person reads at a glance (initials on a coloured dot).
  const assigneeMap = useMemo(() => {
    const m = new Map<string, { label: string; color: string }[]>()
    for (const a of assignments) {
      const r = resources.find(x => x.id === a.resource_id)
      if (!r) continue
      const arr = m.get(a.task_id) ?? []
      arr.push({ label: (r.name[0] ?? '?').toUpperCase(), color: r.color })
      m.set(a.task_id, arr)
    }
    return m
  }, [assignments, resources])
  const captureBaselineMut = useMutation({
    mutationFn: (name?: string) => projectsApi.captureBaseline(id!, name),
    onSuccess: (b) => { qc.invalidateQueries({ queryKey: ['baselines', id] }); setActiveBaselineId(b.id) },
  })
  const deleteBaselineMut = useMutation({
    mutationFn: (bid: string) => projectsApi.deleteBaseline(id!, bid),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['baselines', id] }); setActiveBaselineId(null) },
  })
  const renameBaselineMut = useMutation({
    mutationFn: ({ bid, name }: { bid: string; name: string }) => projectsApi.updateBaseline(id!, bid, { name }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['baselines', id] }) },
  })
  // Primary baseline = the one the project is compared against (a single one per project).
  const setPrimaryBaselineMut = useMutation({
    mutationFn: (bid: string) => projectsApi.updateBaseline(id!, bid, { is_primary: true }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['baselines', id] }) },
  })

  // Filtre actif : ids des tâches correspondantes + leurs ancêtres (pour garder la
  // hiérarchie). null = aucun filtre.
  const filterActive = !!(filterText.trim() || filterStatus || filterPriority)
  const filteredIds = useMemo(() => {
    if (!filterActive) return null
    const q = filterText.trim().toLowerCase()
    const byId = new Map(allTasks.map(t => [t.id, t]))
    const keep = new Set<string>()
    for (const tk of allTasks) {
      const ok = (!q || tk.name.toLowerCase().includes(q))
        && (!filterStatus || tk.status === filterStatus)
        && (!filterPriority || tk.priority === filterPriority)
      if (ok) { let cur: ProjectTask | undefined = tk; while (cur) { keep.add(cur.id); cur = cur.parent_id ? byId.get(cur.parent_id) : undefined } }
    }
    return keep
  }, [filterActive, filterText, filterStatus, filterPriority, allTasks])

  // Tâches visibles (replie les sous-arbres des récapitulatifs repliés + filtre).
  const visibleTasks = useMemo(() => {
    // Tri / regroupement → liste à plat (perd la hiérarchie WBS le temps du tri).
    if (sortBy || groupBy) {
      const PRANK: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 }
      const SRANK: Record<string, number> = { in_progress: 0, not_started: 1, on_hold: 2, completed: 3, cancelled: 4 }
      const resName = (tk: ProjectTask) => { const a = assignments.find(x => x.task_id === tk.id); return a ? (resources.find(r => r.id === a.resource_id)?.name ?? '') : '~' }
      const key = (tk: ProjectTask, k: string): number | string => {
        switch (k) {
          case 'name': return tk.name.toLowerCase()
          case 'start': return tk.early_start ?? 0
          case 'end': return tk.early_finish ?? ((tk.early_start ?? 0) + tk.duration_days)
          case 'duration': return tk.duration_days
          case 'progress': return tk.progress
          case 'priority': return PRANK[tk.priority] ?? 9
          case 'status': return SRANK[tk.status] ?? 9
          case 'resource': return resName(tk)
          default: return 0
        }
      }
      const base = (filteredIds ? allTasks.filter(t => filteredIds.has(t.id)) : [...allTasks])
      base.sort((a, b) => {
        for (const k of [groupBy, sortBy].filter(Boolean)) {
          const ka = key(a, k), kb = key(b, k)
          if (ka < kb) return -1; if (ka > kb) return 1
        }
        return 0
      })
      return base.map(task => ({ task, depth: 0, hasChildren: false }))
    }
    const collapsedIds = collapsed
    const out: { task: ProjectTask; depth: number; hasChildren: boolean }[] = []
    const childrenOf = (pid: string | null) => allTasks.filter(t => (t.parent_id ?? null) === pid && (!filteredIds || filteredIds.has(t.id)))
    const walk = (pid: string | null, depth: number) => {
      for (const tk of childrenOf(pid)) {
        const kids = childrenOf(tk.id)
        out.push({ task: tk, depth, hasChildren: kids.length > 0 })
        if (kids.length > 0 && !collapsedIds.has(tk.id)) walk(tk.id, depth + 1)
      }
    }
    walk(null, 0)
    // Repli sur l'ordre d'origine si aucune hiérarchie détectée
    if (out.length === 0 && allTasks.length > 0 && !filteredIds) return allTasks.map(task => ({ task, depth: 0, hasChildren: false }))
    return out
  }, [allTasks, collapsed, filteredIds, sortBy, groupBy, assignments, resources])

  // Tâches affichées (board/calendrier) — appliquent le filtre à plat.
  const displayTasks = useMemo(() => filteredIds ? allTasks.filter(t => filteredIds.has(t.id)) : allTasks, [allTasks, filteredIds])

  const tasks = useMemo(() => visibleTasks.map(v => v.task), [visibleTasks])
  // Completion rolled up from children, and the set of tasks that ARE parents:
  // a parent's manually entered % is kept in the database but replaced on screen
  // by the roll-up, and its manual completion controls are disabled.
  const effProgress = useMemo(() => effectiveProgress(allTasks), [allTasks])
  const parentSet   = useMemo(() => parentIds(allTasks), [allTasks])
  const isParent    = (tid: string | null | undefined) => !!tid && parentSet.has(tid)
  // Numéro de ligne global (1-based) par id, pour les prédécesseurs.
  const taskNumber = useMemo(() => {
    const m = new Map<string, number>()
    allTasks.forEach((tk, i) => m.set(tk.id, i + 1))
    return m
  }, [allTasks])

  const projectStart = useMemo(() => project?.start_date ? new Date(project.start_date) : new Date(), [project?.start_date])
  // The project's working calendar, mirrored client-side: the Gantt uses it to
  // predict where the CPM will settle a dropped bar (snap to a worked day, convert
  // a dragged calendar span back to working days) so nothing hops after release.
  const { data: calData } = useQuery({ queryKey: ['project-calendars', id], queryFn: () => projectsApi.listCalendars(id!), enabled: !!id })
  const officeInstance = useOfficeInstance()
  const workCal = useMemo(
    () => calendarFromApi(calData, projectStart, officeInstance.projectIncludeWeekends),
    [calData, projectStart, officeInstance.projectIncludeWeekends],
  )
  // Target end date as a day offset from the project start (day 0). Drives both the
  // "target finish" marker on the Gantt and how far the timeline extends.
  const targetEndDay = useMemo(() => {
    if (!project?.end_date) return null
    const d = differenceInCalendarDays(new Date(project.end_date), projectStart)
    return d >= 0 ? d : null
  }, [project?.end_date, projectStart])
  // The computed finish (latest task end) — compared against the target below.
  const projectFinishDay = useMemo(() => allTasks.reduce((max, t) => {
    const ef = (t.early_finish ?? 0) || ((t.early_start ?? 0) + t.duration_days)
    return Math.max(max, ef)
  }, 0), [allTasks])
  const totalDays = useMemo(
    () => Math.max(MIN_DAYS, projectFinishDay + 5, targetEndDay != null ? targetEndDay + 5 : 0),
    [projectFinishDay, targetEndDay],
  )

  // ── Collaboration (présence + sync) ───────────────────────────────────────
  const ydoc = useMemo(() => new Y.Doc(), [id])
  const awareness = useMemo(() => new Awareness(ydoc), [ydoc])
  useEffect(() => () => awareness.destroy(), [awareness])
  const authUser = useAuthStore(s => s.user)
  useEffect(() => {
    if (!authUser) return
    awareness.setLocalStateField('user', {
      id: authUser.id, name: authUser.display_name || authUser.username || authUser.email,
      color: userColor(authUser.id), avatar: authUser.avatar_url,
    })
  }, [awareness, authUser])
  useCollab(`office-project:${id}`, ydoc, !!id, { awareness })
  // Sync données : un compteur partagé bumpé à chaque édition locale ; les pairs
  // qui reçoivent le changement (origin distant) réinvalident la requête.
  const bumpRev = useCallback(() => {
    try { ydoc.getMap('meta').set('rev', `${Date.now()}-${Math.random()}`) } catch { /* ignore */ }
  }, [ydoc])
  useEffect(() => {
    const m = ydoc.getMap('meta')
    const obs = (_e: unknown, tr: Y.Transaction) => { if (!tr.local) qc.invalidateQueries({ queryKey: ['project', id] }) }
    m.observe(obs)
    return () => m.unobserve(obs)
  }, [ydoc, qc, id])
  const publishCursor = usePublishCursor(awareness)

  // Invalidation + bump combinés après mutation.
  const refresh = useCallback(() => { const p = qc.invalidateQueries({ queryKey: ['project', id] }); bumpRev(); return p }, [qc, id, bumpRev])

  const saveProjectProps = async (patch: Partial<Project>) => {
    const projectId = id!
    // The project start is a FLOOR, not a magnet: pushing it EARLIER must only make
    // room before the plan, never pull tasks back to it. But an "as soon as possible"
    // task with no predecessor sits, by definition, on the project start — so it would
    // follow. To keep the schedule where the user placed it, we pin every free (ASAP)
    // leaf at its current absolute date before moving the start earlier. Tasks that
    // already carry a date constraint keep their date on their own.
    let pins: { id: string; date: string }[] = []
    if (patch.start_date !== undefined && project) {
      const oldStart = project.start_date ? new Date(project.start_date) : new Date()
      const earlier = patch.start_date != null && differenceInCalendarDays(new Date(patch.start_date), oldStart) < 0
      if (earlier) {
        for (const tk of allTasks) {
          if (tk.task_type === 'summary') continue
          if (allTasks.some(x => x.parent_id === tk.id)) continue // a parent rolls up
          const ct = tk.constraint_type ?? 'ASAP'
          if (ct === 'ASAP' || ct === 'ALAP') {
            pins.push({ id: tk.id, date: format(addDays(oldStart, tk.early_start ?? 0), 'yyyy-MM-dd') })
          }
        }
      }
    }
    await projectsApi.update(projectId, patch as never)
    for (const p of pins) {
      try { await projectsApi.updateTask(projectId, p.id, { constraint_type: 'SNET', constraint_date: p.date, start_date: p.date } as never) } catch { /* ignore */ }
    }
    // Re-run the CPM when dates move (it also relocates constrained tasks to keep
    // their absolute dates as the anchor shifts).
    if (patch.start_date !== undefined || patch.end_date !== undefined) {
      try { await projectsApi.computeCpm(projectId) } catch { /* keep dates even if CPM hiccups */ }
    }
    refresh()
  }
  const updateProjectMut = useMutation({
    mutationFn: (d: Partial<Project>) => projectsApi.update(id!, d as never),
    onSuccess: refresh,
  })

  const [titleDraft, setTitleDraft] = useState('')
  useEffect(() => { if (project?.title != null) setTitleDraft(project.title) }, [project?.title])
  const trashProjMut     = useMutation({ mutationFn: () => projectsApi.trash(id!), onSuccess: () => navigate('/office/projects') })
  const createProjMut    = useMutation({ mutationFn: () => projectsApi.create({ title: t('common_untitled') }), onSuccess: (p) => navigate(`/office/projects/${p.id}`) })
  const duplicateProjMut = useMutation({ mutationFn: () => projectsApi.duplicate(id!), onSuccess: (nid) => navigate(`/office/projects/${nid}`) })

  const createTaskMut = useMutation({
    mutationFn: (d?: { parent_id?: string; task_type?: string; position?: number }) => projectsApi.createTask(id!, d),
    onSuccess: refresh,
  })
  const updateTaskMut = useMutation({
    mutationFn: ({ taskId, data }: { taskId: string; data: Partial<ProjectTask> }) => projectsApi.updateTask(id!, taskId, data as never),
    onSuccess: refresh,
  })
  const deleteTaskMut = useMutation({
    mutationFn: (taskId: string) => projectsApi.deleteTask(id!, taskId),
    onSuccess: () => { setSelectedId(null); refresh() },
  })
  const computeCpmMut = useMutation({ mutationFn: () => projectsApi.computeCpm(id!), onSuccess: refresh })
  const createResMut  = useMutation({ mutationFn: (d: { name: string; user_id?: string; color?: string }) => projectsApi.createResource(id!, d), onSuccess: () => { setNewResName(''); qc.invalidateQueries({ queryKey: ['org-members', id] }); refresh() } })
  const membersQuery  = useQuery({ queryKey: ['org-members', id], queryFn: () => projectsApi.orgMembers(id!), enabled: !!id })
  const deleteResMut  = useMutation({ mutationFn: (rid: string) => projectsApi.deleteResource(id!, rid), onSuccess: refresh })
  const assignMut     = useMutation({ mutationFn: ({ taskId, rid }: { taskId: string; rid: string }) => projectsApi.assignResource(id!, taskId, { resource_id: rid }), onSuccess: refresh })
  const unassignMut   = useMutation({ mutationFn: ({ taskId, rid }: { taskId: string; rid: string }) => projectsApi.unassignResource(id!, taskId, rid), onSuccess: refresh })
  const addDepMut     = useMutation({ mutationFn: (d: { from_task_id: string; to_task_id: string }) => projectsApi.createDependency(id!, d), onSuccess: refresh })

  // ── Commandes annulables ────────────────────────────────────────────────────
  // Each user-visible edit goes through a "command": it performs the change AND
  // pushes its inverse onto the history. Nothing pushes while an entry is being
  // applied (`history.push` ignores those), and every entry resolves task/dependency
  // ids through `ctx.resolve` because an undo that re-creates an entity gets a new id.

  /** Discrete action: closes any running coalescing window, then stacks the entry. */
  const pushHistory = (entry: HistoryEntry) => { coalesceRef.current = null; history.push(entry) }

  /**
   * Stacks an edit that a continuous gesture may keep feeding. While `key` stays the
   * same and edits keep coming inside the window, the live entry's "after" side is
   * updated in place — the "before" side remains the state at the START of the gesture.
   * `key = null` → discrete edit (one entry, no coalescing).
   */
  const pushEdit = (
    key: string | null,
    before: EditPatch,
    after: EditPatch,
    apply: (ctx: HistoryCtx, patch: EditPatch) => Promise<void>,
    label: string,
  ) => {
    const live = coalesceRef.current
    if (key && live && live.key === key && Date.now() - live.at < COALESCE_MS) {
      Object.assign(live.after, after)
      live.at = Date.now()
      return
    }
    const state = { key: key ?? '', before, after: { ...after }, at: Date.now() }
    coalesceRef.current = null
    history.push({ label, undo: ctx => apply(ctx, state.before), redo: ctx => apply(ctx, state.after) })
    if (key) coalesceRef.current = state
  }

  /** Edits a task and stacks the restoration of the fields it touched. */
  const patchTaskCmd = (taskId: string, data: Partial<ProjectTask>, label = 'task-edit') => {
    const cur = allTasks.find(x => x.id === taskId)
    const projectId = id
    if (!cur || !projectId) return
    const src = cur as unknown as EditPatch
    const before: EditPatch = {}
    const after:  EditPatch = {}
    const keys: string[] = []
    for (const [k, v] of Object.entries(data)) { keys.push(k); before[k] = src[k]; after[k] = v }
    if (keys.length === 0) return
    const apply = async (ctx: HistoryCtx, patch: EditPatch) => {
      await projectsApi.updateTask(projectId, ctx.resolve(taskId), patch as never)
      refresh()
    }
    // Only field sets made ONLY of continuous fields coalesce (a status change must
    // never be swallowed by a running typing gesture).
    const key = keys.every(k => COALESCED_FIELDS.has(k)) ? `task:${taskId}:${keys.join(',')}` : null
    pushEdit(key, before, after, apply, label)
    updateTaskMut.mutate({ taskId, data })
  }

  /** Creates a task; the inverse deletes it (following its id through remaps). */
  const createTaskCmd = async (d?: { parent_id?: string; task_type?: string; position?: number }) => {
    const projectId = id
    if (!projectId) return
    // A failed action leaves nothing to undo.
    const created = await createTaskMut.mutateAsync(d).catch(() => null)
    if (!created) return
    pushHistory({
      label: 'task-create',
      undo: async ctx => { await projectsApi.deleteTask(projectId, ctx.resolve(created.id)); setSelectedId(null); refresh() },
      redo: async ctx => {
        const again = await projectsApi.createTask(projectId, {
          ...d, parent_id: d?.parent_id ? ctx.resolve(d.parent_id) : undefined,
        })
        ctx.remap(created.id, again.id)
        refresh()
      },
    })
  }

  /**
   * Deletes a task. The DB CASCADEs, so the whole subtree, the dependencies touching
   * it and its resource assignments go away too — the snapshot covers them all and
   * the undo re-creates everything (parents first, then children, then links).
   */
  const deleteTaskCmd = async (taskId: string) => {
    const projectId = id
    if (!projectId || !allTasks.some(x => x.id === taskId)) return
    const subtree: ProjectTask[] = []
    const collect = (tid: string) => {
      const tk = allTasks.find(x => x.id === tid)
      if (!tk) return
      subtree.push(tk)
      allTasks.filter(x => x.parent_id === tid).forEach(c => collect(c.id))
    }
    collect(taskId)
    const goneIds  = new Set(subtree.map(x => x.id))
    const snaps    = subtree.map(snapshotTask)   // parents before children
    const goneDeps = deps.filter(d => goneIds.has(d.from_task_id) || goneIds.has(d.to_task_id))
      .map(d => ({ from: d.from_task_id, to: d.to_task_id, type: d.dep_type, lag: d.lag_days }))
    const goneAssigns = assignments.filter(a => goneIds.has(a.task_id))
      .map(a => ({ task: a.task_id, res: a.resource_id, units: a.units }))

    try { await deleteTaskMut.mutateAsync(taskId) }
    catch { return }   // the action failed → there is nothing to undo
    pushHistory({
      label: 'task-delete',
      undo: async ctx => {
        for (const snap of snaps) {
          const newId = await recreateTask(projectId, snap, snap.parent_id ? ctx.resolve(snap.parent_id) : null)
          ctx.remap(snap.id, newId)
        }
        for (const d of goneDeps) {
          try {
            await projectsApi.createDependency(projectId, {
              from_task_id: ctx.resolve(d.from), to_task_id: ctx.resolve(d.to), dep_type: d.type, lag_days: d.lag,
            })
          } catch { /* the other end may have been deleted since */ }
        }
        for (const a of goneAssigns) {
          try { await projectsApi.assignResource(projectId, ctx.resolve(a.task), { resource_id: a.res, units: a.units }) }
          catch { /* the resource may have been deleted since */ }
        }
        try { await projectsApi.computeCpm(projectId) } catch { /* schedule stays as-is */ }
        refresh()
      },
      redo: async ctx => { await projectsApi.deleteTask(projectId, ctx.resolve(taskId)); setSelectedId(null); refresh() },
    })
  }

  /** Adds a dependency; the inverse removes it (and re-adds it on redo, new id). */
  const addDepCmd = async (fromId: string, toId: string, cpm = false) => {
    const projectId = id
    if (!projectId) return
    // A failed action leaves nothing to undo.
    const dep = await addDepMut.mutateAsync({ from_task_id: fromId, to_task_id: toId }).catch(() => null)
    if (!dep) return
    if (cpm) { try { await projectsApi.computeCpm(projectId) } catch { /* ignore */ } ; refresh() }
    pushHistory({
      label: 'dep-add',
      undo: async ctx => {
        await projectsApi.deleteDependency(projectId, ctx.resolve(dep.id))
        if (cpm) { try { await projectsApi.computeCpm(projectId) } catch { /* ignore */ } }
        refresh()
      },
      redo: async ctx => {
        const again = await projectsApi.createDependency(projectId, { from_task_id: ctx.resolve(fromId), to_task_id: ctx.resolve(toId) })
        ctx.remap(dep.id, again.id)
        if (cpm) { try { await projectsApi.computeCpm(projectId) } catch { /* ignore */ } }
        refresh()
      },
    })
  }

  /** Removes dependencies in one go (unlink) — a single undo step restores them all. */
  const removeDepsCmd = async (list: TaskDependency[]) => {
    const projectId = id
    if (!projectId || list.length === 0) return
    const recs = list.map(d => ({ id: d.id, from: d.from_task_id, to: d.to_task_id, type: d.dep_type, lag: d.lag_days }))
    try { for (const d of list) await projectsApi.deleteDependency(projectId, d.id) }
    catch { refresh(); return }
    refresh()
    pushHistory({
      label: 'dep-remove',
      undo: async ctx => {
        for (const r of recs) {
          const again = await projectsApi.createDependency(projectId, {
            from_task_id: ctx.resolve(r.from), to_task_id: ctx.resolve(r.to), dep_type: r.type, lag_days: r.lag,
          })
          ctx.remap(r.id, again.id)
        }
        refresh()
      },
      redo: async ctx => {
        for (const r of recs) {
          try { await projectsApi.deleteDependency(projectId, ctx.resolve(r.id)) } catch { /* already gone */ }
        }
        refresh()
      },
    })
  }

  /** Assigns / unassigns a resource — both directions are exact inverses. */
  const assignResCmd = (taskId: string, rid: string) => {
    const projectId = id
    if (!projectId) return
    assignMut.mutate({ taskId, rid })
    pushHistory({
      label: 'res-assign',
      undo: async ctx => { await projectsApi.unassignResource(projectId, ctx.resolve(taskId), rid); refresh() },
      redo: async ctx => { await projectsApi.assignResource(projectId, ctx.resolve(taskId), { resource_id: rid }); refresh() },
    })
  }
  const unassignResCmd = (taskId: string, rid: string) => {
    const projectId = id
    if (!projectId) return
    const units = assignments.find(a => a.task_id === taskId && a.resource_id === rid)?.units ?? 1
    unassignMut.mutate({ taskId, rid })
    pushHistory({
      label: 'res-unassign',
      undo: async ctx => { await projectsApi.assignResource(projectId, ctx.resolve(taskId), { resource_id: rid, units }); refresh() },
      redo: async ctx => { await projectsApi.unassignResource(projectId, ctx.resolve(taskId), rid); refresh() },
    })
  }

  // Undo/redo triggers — they also close any running coalescing window, otherwise a
  // later edit would keep feeding an entry that just moved to the other stack.
  const undoCmd = () => { coalesceRef.current = null; history.undo() }
  const redoCmd = () => { coalesceRef.current = null; history.redo() }

  // Ctrl+Z / Ctrl+Y (Ctrl+Maj+Z) — inert inside text fields so they keep their
  // native undo (task name, description, predecessors…).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return
      const k = e.key.toLowerCase()
      if (k !== 'z' && k !== 'y') return
      const el = e.target as HTMLElement | null
      if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return
      e.preventDefault()
      coalesceRef.current = null
      if (k === 'y' || e.shiftKey) historyRef.current.redo(); else historyRef.current.undo()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  // Opening another project starts from a blank history (ids of the old one are dead).
  useEffect(() => { historyRef.current.clear(); coalesceRef.current = null }, [id])

  // Upsert (type/lag) d'une dépendance puis recalcul CPM.
  const setDep = async (fromId: string, toId: string, dep_type: string, lag_days: number) => {
    if (!id) return
    const projectId = id
    const prev = deps.find(d => d.from_task_id === fromId && d.to_task_id === toId)
    const apply = async (ctx: HistoryCtx, patch: EditPatch) => {
      await projectsApi.createDependency(projectId, {
        from_task_id: ctx.resolve(fromId), to_task_id: ctx.resolve(toId),
        dep_type: patch.dep_type as string, lag_days: patch.lag_days as number,
      })
      try { await projectsApi.computeCpm(projectId) } catch { /* ignore */ }
      refresh()
    }
    if (prev) {
      // The lag field fires on every keystroke → coalesce per dependency.
      pushEdit(`dep:${fromId}:${toId}`, { dep_type: prev.dep_type, lag_days: prev.lag_days },
        { dep_type, lag_days }, apply, 'dep-edit')
    }
    await projectsApi.createDependency(projectId, { from_task_id: fromId, to_task_id: toId, dep_type, lag_days })
    await projectsApi.computeCpm(projectId)
    refresh()
  }

  // Prédécesseurs : texte « 1;3 » ↔ dépendances FS.
  const predecessorText = useCallback((taskId: string) => {
    return deps.filter(d => d.to_task_id === taskId)
      .map(d => taskNumber.get(d.from_task_id))
      .filter((n): n is number => !!n).sort((a, b) => a - b).join(';')
  }, [deps, taskNumber])
  // Commits the "1;3" field: one history entry for the whole set (adds + removals).
  const setPredecessors = async (taskId: string, text: string) => {
    const projectId = id
    if (!projectId) return
    const wanted = new Set(text.split(/[;,\s]+/).map(s => parseInt(s.trim())).filter(n => n >= 1)
      .map(n => allTasks[n - 1]?.id).filter((x): x is string => !!x && x !== taskId))
    const current = deps.filter(d => d.to_task_id === taskId)
    const removed = current.filter(d => !wanted.has(d.from_task_id))
    const have = new Set(current.map(d => d.from_task_id))
    const added = [...wanted].filter(fid => !have.has(fid))
    if (removed.length === 0 && added.length === 0) return

    const gone = removed.map(d => ({ id: d.id, from: d.from_task_id, to: d.to_task_id, type: d.dep_type, lag: d.lag_days }))
    const born: { from: string; id: string }[] = []
    try {
      for (const d of removed) await projectsApi.deleteDependency(projectId, d.id)
      for (const fid of added) {
        const dep = await projectsApi.createDependency(projectId, { from_task_id: fid, to_task_id: taskId })
        born.push({ from: fid, id: dep.id })
      }
    } catch { refresh(); return }
    refresh()
    pushHistory({
      label: 'predecessors',
      undo: async ctx => {
        for (const b of born) {
          try { await projectsApi.deleteDependency(projectId, ctx.resolve(b.id)) } catch { /* already gone */ }
        }
        for (const g of gone) {
          const again = await projectsApi.createDependency(projectId, {
            from_task_id: ctx.resolve(g.from), to_task_id: ctx.resolve(g.to), dep_type: g.type, lag_days: g.lag,
          })
          ctx.remap(g.id, again.id)
        }
        refresh()
      },
      redo: async ctx => {
        for (const g of gone) {
          try { await projectsApi.deleteDependency(projectId, ctx.resolve(g.id)) } catch { /* already gone */ }
        }
        for (const b of born) {
          const again = await projectsApi.createDependency(projectId, { from_task_id: ctx.resolve(b.from), to_task_id: ctx.resolve(taskId) })
          ctx.remap(b.id, again.id)
        }
        refresh()
      },
    })
  }

  const ganttH = HEADER_H + tasks.length * ROW_H
  // At REST the chart is tight: VISIBLE_LEAD days before the start, VISIBLE_LEAD after
  // the last content (task finish or end marker). The wide DRAG_MARGIN only appears
  // WHILE a boundary marker is dragged — that extra room is what the edge auto-scroll
  // reveals, then it collapses again. `markerPreview` is non-null exactly during such
  // a drag, so it drives the switch. The left offset changes with it, so the drag
  // start/end compensate `scrollLeft` by the same amount (no visual jump).
  const markerDragging = markerPreview !== null
  const leadDays  = markerDragging ? DRAG_MARGIN : VISIBLE_LEAD
  const trailDays = markerDragging ? DRAG_MARGIN : VISIBLE_LEAD
  // Right edge of the real content (last task finish or the end marker), with a small
  // floor so an almost-empty project still shows a usable span — NOT MIN_DAYS, which
  // would re-create the big trailing margin the tight layout removes.
  const contentRight = Math.max(projectFinishDay, targetEndDay ?? 0, 14)
  const drawRight = contentRight + trailDays
  const ganttW = (leadDays + drawRight) * dayW
  // The imperative offset used inside pointer handlers (state may lag a frame): the
  // wide margin is active exactly while a marker drag is live.
  const activeLeadDays = () => (markerDragRef.current?.active ? DRAG_MARGIN : VISIBLE_LEAD)
  // Re-anchor the view to the resting position (day 0 at VISIBLE_LEAD from the left).
  // With leadDays back to VISIBLE_LEAD that is simply scrollLeft = 0.
  const anchorViewToStart = () => { const el = ganttRef.current; if (el) el.scrollLeft = 0 }

  const doRender = useCallback(() => {
    const canvas = canvasRef.current, host = ganttRef.current
    if (!canvas || !host) return
    const viewW = host.clientWidth
    if (viewW <= 0) return
    // (Re)crée le renderer si absent OU si le canvas a été remonté (sinon on
    // dessinerait sur un canvas détaché → diagramme blanc).
    if (!renderer.current || renderer.current.el !== canvas) renderer.current = new GanttRenderer(canvas)
    renderer.current.resize(viewW, ganttH)
    renderer.current.render(tasks, deps, projectStart, totalDays, scrollLeft, viewW, getDateLocale(i18n.language), dayW, barPreview, linkPreview, baselineMap, assigneeMap, targetEndDay, markerPreview, hoverMarker, hoverMarker === 'start' ? t('proj_marker_start_tip', { defaultValue: 'Début du projet — glissez pour changer la date' }) : t('proj_marker_end_tip', { defaultValue: 'Fin prévue du projet — glissez pour changer la date' }), leadDays, drawRight)
  }, [tasks, deps, projectStart, totalDays, scrollLeft, ganttH, i18n.language, dayW, activeTab, showTimeline, barPreview, linkPreview, baselineMap, assigneeMap, targetEndDay, markerPreview, hoverMarker, t, leadDays, drawRight])
  useEffect(() => { doRender() }, [doRender])
  useEffect(() => {
    if (!ganttRef.current) return
    const ro = new ResizeObserver(doRender)
    ro.observe(ganttRef.current)
    return () => ro.disconnect()
  }, [doRender])

  // Curseur de présence sur la zone de travail.
  const onWorkMouseMove = useCallback((e: React.MouseEvent) => {
    const el = workRef.current; if (!el) return
    const r = el.getBoundingClientRect()
    publishCursor({ x: e.clientX - r.left, y: e.clientY - r.top })
  }, [publishCursor])

  // ── Glisser-déposer des barres du Gantt (déplacer / redimensionner) ──────────
  const taskRowAt = (clientY: number): ProjectTask | null => {
    const c = canvasRef.current; if (!c) return null
    const row = Math.floor((clientY - c.getBoundingClientRect().top - HEADER_H) / ROW_H)
    return tasks[row] ?? null
  }
  // The scroll position read LIVE from the container: the React state lags a frame
  // behind (it is set from the scroll event), which matters during edge auto-scroll.
  const liveScrollLeft = () => ganttRef.current?.scrollLeft ?? scrollLeft
  const ganttBarHit = (clientX: number, clientY: number) => {
    const c = canvasRef.current; if (!c) return null
    const rect = c.getBoundingClientRect()
    const xWorld = (clientX - rect.left) + liveScrollLeft()
    const yRel = clientY - rect.top
    // The chart is shifted right by `activeLeadDays()` (wider mid-drag), so a world x
    // maps to a day through these two helpers — nothing else may convert.
    const ld = activeLeadDays()
    const dayAt = (xw: number) => xw / dayW - ld
    const xOfDay = (d: number) => (d + ld) * dayW
    // ── the two project boundary markers, grabbable from the header band ──
    // Restricted to the header so the dashed rule crossing the bars can never steal
    // a click meant for a task.
    if (yRel < HEADER_H) {
      const cand: { which: 'start' | 'end'; day: number; side: 'left' | 'right' }[] = [
        { which: 'start', day: 0, side: 'right' },
        ...(targetEndDay != null ? [{ which: 'end' as const, day: targetEndDay, side: 'left' as const }] : []),
      ]
      for (const m of cand) {
        const gx = xOfDay(m.day)
        const label = format(addDays(projectStart, m.day), 'd MMM', { locale: getDateLocale(i18n.language) })
        const fr = renderer.current?.flagRect(gx, m.side, label)
        // Grabbable from the whole header column of the marker — the flag OR a
        // comfortable band around its rule — so it is easy to catch (not a 20px strip).
        const onFlag = fr ? (xWorld >= fr.x - 3 && xWorld <= fr.x + fr.w + 3) : false
        if (onFlag || Math.abs(xWorld - gx) <= 9) {
          return { task: null, mode: 'marker' as const, marker: m.which, dayFloat: dayAt(xWorld), bx: gx, bw: 0, row: -1 }
        }
      }
      return null
    }
    const row = Math.floor((yRel - HEADER_H) / ROW_H)
    const task = tasks[row]
    if (!task) return null
    const es = task.early_start ?? 0
    // The bar's real extent is the CPM's calendar span ES→EF: a working calendar
    // stretches it past start+duration (weekends are crossed, not worked), so
    // duration_days alone would put the grips and link dot in the wrong place.
    const calEnd = task.early_finish ?? (es + task.duration_days)
    // Any task with sub-tasks is a summary bracket: grab it to move the WHOLE phase
    // (its children slide with it). Its extent is the rolled-up span.
    const hasKids = allTasks.some(x => x.parent_id === task.id)
    if (task.task_type === 'summary' || hasKids) {
      const ef = task.early_finish ?? (es + 1)
      const sbx = xOfDay(es), sbw = Math.max((ef - es) * dayW, 6), send = sbx + sbw
      if (xWorld < sbx - 2 || xWorld > send + 3) return null
      // A phase whose every leaf has started has nothing that may move: refuse the
      // drag up front (not-allowed cursor) instead of gliding a bracket that will
      // snap back on release.
      const hasMovableLeaf = (pid: string): boolean => allTasks.some(x => x.parent_id === pid &&
        (allTasks.some(y => y.parent_id === x.id) ? hasMovableLeaf(x.id) : (x.progress ?? 0) === 0))
      const mode = hasMovableLeaf(task.id) ? 'move-sum' as const : 'locked' as const
      return { task, mode, dayFloat: dayAt(xWorld), bx: sbx, bw: sbw, row }
    }
    // A milestone is a diamond on its day — draggable as a whole (no edges to resize).
    if (task.task_type === 'milestone') {
      const cx = xOfDay(es)
      if (Math.abs(xWorld - cx) > 9) return null
      const mode = (task.progress ?? 0) > 0 ? 'locked' as const : 'move' as const
      return { task, mode, dayFloat: dayAt(xWorld), bx: cx, bw: 0, row }
    }
    if (task.task_type !== 'task') return null
    const bx = xOfDay(es)
    const bw = Math.max((calEnd - es) * dayW, 4)
    const end = bx + bw
    // Zone pastille (juste après la barre) → création de lien.
    if (xWorld >= end + 1 && xWorld <= end + 14) return { task, mode: 'link' as const, dayFloat: dayAt(xWorld), bx, bw, row }
    if (xWorld < bx - 2 || xWorld > end + 3) return null
    // Edge grips resize (left → start date, right → end date). The grip width is
    // capped at a third of the bar so the two never overlap on a short bar; the
    // rest of the bar moves the whole task.
    const grip = Math.min(8, bw / 3)
    const raw = xWorld <= bx + grip ? 'resize-l'
              : xWorld >= end - grip ? 'resize'
              : 'move'
    // Once a task has started (progress > 0) its start date is fixed — like an
    // "actual start" in a real planner: moving it or dragging its left edge is
    // 'locked'. Once it is DONE (100%) its finish is fixed too, so the right edge
    // locks as well — a completed task is fully pinned. 'locked' still selects the
    // task; it just refuses the date-changing drag.
    const started = task.progress > 0
    const done    = task.progress >= 100
    const mode = (started && (raw === 'resize-l' || raw === 'move')) || (done && raw === 'resize')
      ? 'locked' : raw
    return { task, mode: mode as 'move' | 'resize' | 'resize-l' | 'locked', dayFloat: dayAt(xWorld), bx, bw, row }
  }
  const onGanttDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return
    const hit = ganttBarHit(e.clientX, e.clientY)
    if (!hit) return
    e.preventDefault()
    // A boundary marker carries no task: it moves the PROJECT's dates.
    if (hit.mode === 'marker') {
      try { (e.currentTarget as HTMLCanvasElement).setPointerCapture(e.pointerId) } catch { /* best-effort */ }
      markerDragRef.current = {
        which: hit.marker, grabDayFloat: hit.dayFloat,
        origDay: hit.marker === 'start' ? 0 : (targetEndDay ?? 0),
        startCX: e.clientX, active: false,
      }
      return
    }
    setSelectedId(hit.task.id)
    // Start locked (task already under way): select it, but do not start a drag that
    // would move its start date.
    if (hit.mode === 'locked') return
    // Capture the pointer: the drag keeps working outside the canvas and can only
    // end on a real release — leaving the area no longer commits anything.
    try { (e.currentTarget as HTMLCanvasElement).setPointerCapture(e.pointerId) } catch { /* capture is best-effort */ }
    if (hit.mode === 'link') {
      const x1 = hit.bx + hit.bw - liveScrollLeft() + 6
      const y1 = HEADER_H + hit.row * ROW_H + ROW_H / 2
      linkDragRef.current = { fromId: hit.task.id, x1, y1 }
      setLinkPreview({ x1, y1, x2: x1, y2: y1 })
      return
    }
    const es = hit.task.early_start ?? 0
    const origEnd = hit.task.task_type === 'summary' || allTasks.some(x => x.parent_id === hit.task.id)
      ? (hit.task.early_finish ?? (es + 1))
      : (hit.task.early_finish ?? (es + hit.task.duration_days))
    // Left travel limit. Moving a phase shifts only its MOVABLE (not-yet-started)
    // leaves, so what may hit day 0 is the earliest of THOSE — never the bracket,
    // whose left edge may be pinned by a locked child and would otherwise freeze
    // the whole gesture after a day or two.
    let minDelta = -es
    if (hit.mode === 'move-sum') {
      const kids = (pid: string) => allTasks.filter(x => (x.parent_id ?? null) === pid)
      const leaves: ProjectTask[] = []
      const collect = (pid: string) => { for (const ch of kids(pid)) { if (kids(ch.id).length === 0) leaves.push(ch); else collect(ch.id) } }
      collect(hit.task.id)
      const movable = leaves.filter(l => (l.progress ?? 0) === 0)
      minDelta = movable.length ? -Math.min(...movable.map(l => l.early_start ?? 0)) : 0
    }
    barDragRef.current = {
      taskId: hit.task.id, mode: hit.mode, grabDayFloat: hit.dayFloat,
      origStart: es, origEnd, origDur: hit.task.duration_days,
      origCType: hit.task.constraint_type ?? 'ASAP', origCDate: hit.task.constraint_date ?? null,
      minDelta,
      startCX: e.clientX, startCY: e.clientY, active: false,
    }
    // No preview yet: it appears once the pointer has actually travelled (threshold
    // in onGanttMove), so a plain click never nudges a bar.
  }
  // Where the dragged element sits for a given pointer X, in fractional CALENDAR
  // days. Shared by pointer moves and the auto-scroll ticker, so a plan scrolling
  // under a stationary cursor keeps the preview truthful.
  const recomputePreview = (clientX: number) => {
    const c = canvasRef.current; const drag = barDragRef.current
    if (!c || !drag) return
    const rect = c.getBoundingClientRect()
    const curFloat = ((clientX - rect.left) + liveScrollLeft()) / dayW - activeLeadDays()
    // Fractional delta → the bar GLIDES with the cursor instead of snapping day by
    // day. It is settled onto whole (working) days only on release (in onGanttUp).
    const delta = curFloat - drag.grabDayFloat
    const span = drag.origEnd - drag.origStart
    if (drag.mode === 'move' || drag.mode === 'move-sum') {
      // Clamp the DELTA, not the resulting start: for a phase the bracket may sit
      // left of what actually moves, and clamping its start would stall the drag.
      const d = Math.max(delta, drag.minDelta)
      const start = drag.origStart + d
      setBarPreview({ taskId: drag.taskId, start, end: start + span })
    } else if (drag.mode === 'resize-l') {
      // Left edge moves; the finish stays put.
      const start = Math.min(Math.max(0, drag.origStart + delta), drag.origEnd - 0.4)
      setBarPreview({ taskId: drag.taskId, start, end: drag.origEnd })
    } else {
      const end = Math.max(drag.origEnd + delta, drag.origStart + 0.4)
      setBarPreview({ taskId: drag.taskId, start: drag.origStart, end })
    }
  }
  // Where a dragged BOUNDARY marker sits for a given pointer X. Shared by pointer
  // moves and the auto-scroll ticker (the plan scrolls under a still cursor).
  const recomputeMarker = (clientX: number) => {
    const c = canvasRef.current; const md = markerDragRef.current
    if (!c || !md) return
    const rect = c.getBoundingClientRect()
    const curFloat = ((clientX - rect.left) + liveScrollLeft()) / dayW - activeLeadDays()
    const day = md.origDay + (curFloat - md.grabDayFloat)
    const clamped = md.which === 'start'
      ? Math.min(day, (targetEndDay ?? Number.POSITIVE_INFINITY) - 1)
      : Math.max(day, 1)
    setMarkerPreview({ which: md.which, day: clamped })
  }
  // Arm/adjust the edge auto-scroll for the current pointer. Driven by a timer, not
  // by move events, so parking the cursor against a border keeps the plan scrolling
  // — for a bar drag OR a boundary marker drag alike.
  const armAutoScroll = (clientX: number, clientY: number, rect: DOMRect) => {
    const EDGE = 44
    const dir = clientX > rect.right - EDGE ? 1 : clientX < rect.left + EDGE ? -1 : 0
    autoScrollRef.current.dir = dir
    autoScrollRef.current.clientX = clientX
    autoScrollRef.current.clientY = clientY
    if (dir !== 0 && autoScrollRef.current.timer === null) {
      autoScrollRef.current.timer = window.setInterval(() => {
        const st = autoScrollRef.current
        const g2 = ganttRef.current
        if (!g2 || st.dir === 0) return
        const activeBar = barDragRef.current, activeMarker = markerDragRef.current
        if (!activeBar && !activeMarker) return
        const before = g2.scrollLeft
        g2.scrollLeft = Math.max(0, before + st.dir * 16)
        if (g2.scrollLeft !== before) {
          if (activeBar) recomputePreview(st.clientX)
          else recomputeMarker(st.clientX)
        }
      }, 16)
    } else if (dir === 0 && autoScrollRef.current.timer !== null) {
      window.clearInterval(autoScrollRef.current.timer)
      autoScrollRef.current.timer = null
    }
  }
  const onGanttMove = (e: React.PointerEvent) => {
    const c = canvasRef.current; if (!c) return
    const rect = c.getBoundingClientRect()
    const link = linkDragRef.current
    if (link) { setLinkPreview({ x1: link.x1, y1: link.y1, x2: e.clientX - rect.left, y2: e.clientY - rect.top }); c.style.cursor = 'crosshair'; return }
    const md = markerDragRef.current
    if (md) {
      if (!md.active) {
        if (Math.abs(e.clientX - md.startCX) < 4) return
        md.active = true
        // Open the drag margins: the content shifts right by (DRAG_MARGIN −
        // VISIBLE_LEAD) days, so add the same to scrollLeft to keep the view (and every
        // day value) exactly where it is — this reveals the room the edge auto-scroll
        // will use, collapsed again on release. The inner width must GROW first (it is
        // React state, a frame behind), else the browser clamps the new scrollLeft to
        // the old, narrow content and the view jumps.
        const g = ganttRef.current
        if (g) {
          if (ganttInnerRef.current) ganttInnerRef.current.style.width = `${(DRAG_MARGIN * 2 + contentRight) * dayW}px`
          g.scrollLeft += (DRAG_MARGIN - VISIBLE_LEAD) * dayW
          setScrollLeft(g.scrollLeft)
        }
      }
      armAutoScroll(e.clientX, e.clientY, rect)
      recomputeMarker(e.clientX)
      c.style.cursor = 'ew-resize'
      return
    }
    const drag = barDragRef.current
    if (drag) {
      // A drag only becomes real after ~4px of travel: below that it is a click.
      // Matters most zoomed out, where 4px can already be several days.
      if (!drag.active) {
        if (Math.hypot(e.clientX - drag.startCX, e.clientY - drag.startCY) < 4) return
        drag.active = true
      }
      armAutoScroll(e.clientX, e.clientY, rect)
      recomputePreview(e.clientX)
      return
    }
    const hit = ganttBarHit(e.clientX, e.clientY)
    c.style.cursor = hit
      ? (hit.mode === 'marker' ? 'ew-resize'
        : hit.mode === 'locked' ? 'not-allowed'
        : hit.mode === 'resize' || hit.mode === 'resize-l' ? 'ew-resize'
        : hit.mode === 'link' ? 'crosshair' : 'grab')
      : 'default'
    // On-canvas naming tooltip: track which marker is hovered (drawn by the renderer
    // at the marker, unlike a native title which floats at the cursor).
    const hov = hit?.mode === 'marker' ? hit.marker : null
    setHoverMarker(prev => prev === hov ? prev : hov)
  }
  const stopAutoScroll = () => {
    if (autoScrollRef.current.timer !== null) window.clearInterval(autoScrollRef.current.timer)
    autoScrollRef.current.timer = null
    autoScrollRef.current.dir = 0
  }
  // Escape aborts the gesture in flight: the bar springs back, nothing is sent.
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== 'Escape') return
      if (barDragRef.current) { barDragRef.current = null; stopAutoScroll(); setBarPreview(null) }
      if (markerDragRef.current) {
        const wasActive = markerDragRef.current.active
        markerDragRef.current = null; stopAutoScroll()
        // Abort restores the pre-drag view: reverse the activate compensation.
        if (wasActive) { const g = ganttRef.current; if (g) { g.scrollLeft = Math.max(0, g.scrollLeft - (DRAG_MARGIN - VISIBLE_LEAD) * dayW); setScrollLeft(g.scrollLeft) } }
        setMarkerPreview(null)
      }
      if (linkDragRef.current) { linkDragRef.current = null; setLinkPreview(null) }
    }
    window.addEventListener('keydown', onKey)
    // The ticker must never outlive the page.
    return () => { window.removeEventListener('keydown', onKey); stopAutoScroll() }
  }, [dayW])
  // Re-applies a Gantt patch and re-runs the CPM (the bars come from it).
  const applyGanttPatch = async (ctx: HistoryCtx, taskId: string, patch: EditPatch) => {
    if (!id) return
    await projectsApi.updateTask(id, ctx.resolve(taskId), patch as never)
    try { await projectsApi.computeCpm(id) } catch { /* schedule stays as-is */ }
    refresh()
  }
  const onGanttUp = async (e?: React.PointerEvent) => {
    // Fin de création de lien.
    const link = linkDragRef.current
    if (link) {
      linkDragRef.current = null; setLinkPreview(null)
      const target = e ? taskRowAt(e.clientY) : null
      if (target && target.id !== link.fromId && id && !deps.some(d => d.from_task_id === link.fromId && d.to_task_id === target.id)) {
        try { await addDepCmd(link.fromId, target.id, true) } catch { /* ignore */ }
      }
      return
    }
    // A boundary marker was dragged: move the project's planned start / target end.
    const md = markerDragRef.current
    if (md) {
      markerDragRef.current = null
      stopAutoScroll()
      // Collapse the drag margins back to the resting width, compensating scrollLeft
      // by the same (DRAG_MARGIN − VISIBLE_LEAD) days so the view does not jump. A
      // start commit shifts the epoch (day 0 = the new start), so it re-anchors to the
      // resting position (scrollLeft 0) instead. Done together with clearing the
      // preview so leadDays and scrollLeft never disagree for a frame.
      const collapse = () => {
        const g = ganttRef.current
        if (!g) return
        g.scrollLeft = md.which === 'start' ? 0 : Math.max(0, g.scrollLeft - (DRAG_MARGIN - VISIBLE_LEAD) * dayW)
        setScrollLeft(g.scrollLeft)
      }
      const mp = markerPreview
      if (!mp || !md.active) { collapse(); setMarkerPreview(null); return }
      const day = Math.round(mp.day)
      if (day === md.origDay) { collapse(); setMarkerPreview(null); return }
      // Settle the marker onto the whole day and HOLD it there until the saved
      // project date has been refetched — clearing now would snap the marker back to
      // its old date for a frame before the new one arrives (the zig-zag).
      setMarkerPreview({ which: md.which, day })
      const date = format(addDays(projectStart, day), 'yyyy-MM-dd')
      const before = md.which === 'start'
        ? { start_date: project?.start_date ?? null }
        : { end_date: project?.end_date ?? null }
      const after = md.which === 'start' ? { start_date: date } : { end_date: date }
      try { await saveProjectProps(after as Partial<Project>) } catch { /* ignore */ }
      try { await refresh() } catch { /* ignore */ }
      collapse()
      setMarkerPreview(null)   // project date now reflects it → single clean transition
      pushHistory({
        label: md.which === 'start' ? 'project-start' : 'project-end',
        undo: async () => { await saveProjectProps(before as Partial<Project>) },
        redo: async () => { await saveProjectProps(after as Partial<Project>) },
      })
      return
    }
    const drag = barDragRef.current, prev = barPreview
    barDragRef.current = null
    stopAutoScroll()
    // Below the drag threshold nothing happened: it was a click (selection only).
    if (!drag || !drag.active || !prev || !id) { setBarPreview(null); return }
    const projectId = id
    const taskId = drag.taskId
    const dayDate = (off: number) => format(addDays(projectStart, off), 'yyyy-MM-dd')

    // Moving a summary bracket slides the WHOLE phase: every movable descendant leaf
    // shifts by the same day delta (anchored with SNET at its shifted, snapped-to-
    // working date). A parent owns no date of its own, so this is the only
    // meaningful way to "set a parent's start".
    if (drag.mode === 'move-sum') {
      const delta = Math.round(prev.start) - drag.origStart
      if (delta === 0) { setBarPreview(null); return }
      // Keep the bracket preview on screen until the children have actually moved
      // (updates + CPM + refetch); clearing it now would snap the bracket back to its
      // old span for a frame before the descendants reposition — the "zig-zag".
      setBarPreview({ taskId, start: drag.origStart + delta, end: drag.origEnd + delta })
      const kids = (pid: string) => allTasks.filter(x => (x.parent_id ?? null) === pid)
      const leaves: ProjectTask[] = []
      const collect = (pid: string) => { for (const ch of kids(pid)) { if (kids(ch.id).length === 0) leaves.push(ch); else collect(ch.id) } }
      collect(taskId)
      // A started leaf's start is locked, so a phase move leaves it in place; only
      // the not-yet-started tasks slide — each snapped to its calendar's next
      // worked day, exactly as the CPM will place it.
      const changes = leaves.filter(l => (l.progress ?? 0) === 0).map(l => {
        const ns = workCal.nextWorking(Math.max(0, (l.early_start ?? 0) + delta))
        return {
          id: l.id,
          after:  { constraint_type: 'SNET', constraint_date: dayDate(ns), start_date: dayDate(ns) } as EditPatch,
          before: { constraint_type: l.constraint_type ?? 'ASAP', constraint_date: l.constraint_date ?? null, start_date: l.start_date ?? dayDate(l.early_start ?? 0) } as EditPatch,
        }
      })
      if (!changes.length) { setBarPreview(null); return }
      const applyAll = async (ctx: HistoryCtx, list: { id: string; patch: EditPatch }[]) => {
        await Promise.all(list.map(c => projectsApi.updateTask(projectId, ctx.resolve(c.id), c.patch as never)))
        try { await projectsApi.computeCpm(projectId) } catch { /* ignore */ }
        await refresh()
      }
      try {
        await Promise.all(changes.map(c => projectsApi.updateTask(projectId, c.id, c.after as never)))
        await projectsApi.computeCpm(projectId)
      } catch { /* ignore */ }
      await refresh()
      setBarPreview(null)   // children have moved → clearing reveals the final position, no snap-back
      pushHistory({
        label: 'gantt-move-phase',
        undo: ctx => applyAll(ctx, changes.map(c => ({ id: c.id, patch: c.before }))),
        redo: ctx => applyAll(ctx, changes.map(c => ({ id: c.id, patch: c.after }))),
      })
      return
    }

    // The drag glides in fractional CALENDAR days; the commit converts back to the
    // scheduler's terms — a start snapped to the next worked day, a duration in
    // WORKING days — using the same calendar as the CPM, so the settled preview
    // already sits where the recomputed schedule will land (no hop on arrival).
    let rStart = drag.origStart, newDur = drag.origDur
    if (drag.mode === 'move') {
      rStart = workCal.nextWorking(Math.max(0, Math.round(prev.start)))
    } else if (drag.mode === 'resize-l') {
      rStart = workCal.nextWorking(Math.max(0, Math.round(prev.start)))
      if (rStart >= drag.origEnd) rStart = workCal.nextWorking(Math.max(0, drag.origEnd - 1))
      newDur = Math.max(1, workCal.workingDaysBetween(rStart, drag.origEnd))
    } else {   // resize (right edge)
      newDur = Math.max(1, workCal.workingDaysBetween(drag.origStart, Math.round(prev.end)))
    }
    const changed = rStart !== drag.origStart || newDur !== drag.origDur
    if (!changed) { setBarPreview(null); return }
    // Settle the bar onto its PREDICTED final geometry at release, then hold it
    // there until the recomputed schedule lands — one clean transition, no zig-zag.
    const predictedEnd = newDur > 0 ? workCal.advance(rStart, newDur) : rStart
    setBarPreview({ taskId, start: rStart, end: predictedEnd })

    // ONE undo step per gesture: the drag only commits on mouse-up, and the "before"
    // side comes from the offsets captured when the drag started. A start_date that
    // was NULL cannot be restored as such (the API reads NULL as « keep »), so the
    // inverse restores the equivalent DATE for the original day offset.
    //
    // The scheduler places bars from constraints, not from `start_date`, so moving a
    // task (or its left edge) anchors the start with a "Start no earlier than"
    // constraint at the dropped date — the drag then actually reschedules. SNET is
    // used rather than "Must start on" on purpose: MSO pins the late dates too, which
    // zeroes the task's slack and paints it critical (red) even when it isn't. SNET
    // keeps the real float, so the red/blue colouring stays truthful. The right edge
    // only changes the duration. Undo restores the task's original constraint (often
    // none) and, for a resize, its duration; `constraint_date: null` clears it.
    const before: EditPatch = drag.mode === 'resize'
      ? { duration_days: drag.origDur }
      : {
          constraint_type: drag.origCType,
          constraint_date: drag.origCDate,
          start_date: dayDate(drag.origStart),
          ...(drag.mode === 'resize-l' ? { duration_days: drag.origDur } : {}),
        }
    const after: EditPatch = drag.mode === 'resize'
      ? { duration_days: newDur }
      : {
          constraint_type: 'SNET',
          constraint_date: dayDate(rStart),
          start_date: dayDate(rStart),
          ...(drag.mode === 'resize-l' ? { duration_days: newDur } : {}),
        }
    try {
      await projectsApi.updateTask(projectId, taskId, after as never)
      await projectsApi.computeCpm(projectId)
    } catch { /* ignore */ }
    // Keep the preview on screen until the refetched schedule has landed, THEN drop
    // it — otherwise the bar snaps back to its old spot for a frame before the new
    // position arrives. Clearing after the refresh makes it a single transition.
    try { await refresh() } catch { /* ignore */ }
    setBarPreview(null)
    pushHistory({
      label: drag.mode === 'move' ? 'gantt-move' : 'gantt-resize',
      undo: ctx => applyGanttPatch(ctx, taskId, before),
      redo: ctx => applyGanttPatch(ctx, taskId, after),
    })
  }

  const selectedTask = tasks.find(t => t.id === selectedId) ?? null

  // Actions du ruban
  const selIndex = selectedTask ? allTasks.findIndex(t => t.id === selectedTask.id) : -1
  const insertTask = (type?: string) => { void createTaskCmd({ task_type: type, position: selIndex >= 0 ? selIndex + 1 : undefined }) }
  const linkSelectedToPrev = () => {
    if (selIndex <= 0) return
    void addDepCmd(allTasks[selIndex - 1].id, allTasks[selIndex].id)
  }
  const unlinkTask = (taskId: string) => { void removeDepsCmd(deps.filter(d => d.to_task_id === taskId)) }
  const unlinkSelected = () => { if (selectedTask) unlinkTask(selectedTask.id) }
  const setProgress = (p: number) => selectedTask && patchTaskCmd(selectedTask.id, { progress: p })
  const setStatus   = (taskId: string, status: string) => {
    // « Terminé » implies 100 % — a single PATCH keeps it a single undo step.
    const patch = { status } as Partial<ProjectTask>
    // A parent's completion is rolled up from its children, so status never sets
    // its (now inactive) manual %.
    if (status === 'completed' && !isParent(taskId)) patch.progress = 100
    patchTaskCmd(taskId, patch)
  }
  const setPriority = (taskId: string, priority: string) => patchTaskCmd(taskId, { priority } as Partial<ProjectTask>)

  // Indent: make a task the child of its nearest preceding sibling.
  const indentTask = (taskId: string) => {
    const idx = allTasks.findIndex(x => x.id === taskId)
    if (idx < 0) return
    const me = allTasks[idx]
    for (let i = idx - 1; i >= 0; i--) {
      if ((allTasks[i].parent_id ?? null) === (me.parent_id ?? null)) {
        patchTaskCmd(taskId, { parent_id: allTasks[i].id } as Partial<ProjectTask>, 'task-indent'); return
      }
    }
  }
  // Outdent: reparent to the grandparent.
  const outdentTask = (taskId: string) => {
    const me = allTasks.find(x => x.id === taskId)
    if (!me?.parent_id) return
    const parent = allTasks.find(x => x.id === me.parent_id)
    patchTaskCmd(taskId, { parent_id: (parent?.parent_id ?? null) } as Partial<ProjectTask>, 'task-outdent')
  }
  // Move a task up/down by swapping its position with the neighbour.
  const moveTask = (taskId: string, dir: 'up' | 'down') => {
    const idx = allTasks.findIndex(x => x.id === taskId)
    const j = dir === 'up' ? idx - 1 : idx + 1
    const projectId = id
    if (idx < 0 || j < 0 || j >= allTasks.length || !projectId) return
    const me = allTasks[idx], other = allTasks[j]
    // Both PATCHes belong to the same user action → one entry swaps them back.
    const swap = async (ctx: HistoryCtx, mePos: number, otherPos: number) => {
      await projectsApi.updateTask(projectId, ctx.resolve(me.id), { position: mePos } as never)
      await projectsApi.updateTask(projectId, ctx.resolve(other.id), { position: otherPos } as never)
      refresh()
    }
    pushHistory({
      label: 'task-move',
      undo: ctx => swap(ctx, me.position, other.position),
      redo: ctx => swap(ctx, other.position, me.position),
    })
    updateTaskMut.mutate({ taskId: me.id, data: { position: other.position } as Partial<ProjectTask> })
    updateTaskMut.mutate({ taskId: other.id, data: { position: me.position } as Partial<ProjectTask> })
  }
  // Reorder by drag & drop: move a task (with its whole subtree) before/after a
  // target, or nest it inside one. The backend orders tasks by a global `position`
  // and rebuilds the tree from `parent_id`, so we renumber the pre-order sequence
  // and, when nesting, set the dragged task's parent. Cross-subtree drops onto a
  // descendant are refused (the server would reject the cycle anyway).
  const reorderTaskCmd = (draggedId: string, targetId: string, place: 'before' | 'after' | 'inside') => {
    const projectId = id
    if (!projectId || draggedId === targetId) return
    const byId = new Map(allTasks.map(t => [t.id, t]))
    const dragged = byId.get(draggedId), target = byId.get(targetId)
    if (!dragged || !target) return

    // Children of a node, already in position order (allTasks is sorted by position).
    const kidsOf = (pid: string | null) => allTasks.filter(t => (t.parent_id ?? null) === pid)
    const flat: ProjectTask[] = []
    const walk = (pid: string | null) => { for (const t of kidsOf(pid)) { flat.push(t); walk(t.id) } }
    walk(null)

    // The dragged task's whole subtree travels together.
    const inBlock = new Set<string>([draggedId])
    const collect = (pid: string) => { for (const c of kidsOf(pid)) { inBlock.add(c.id); collect(c.id) } }
    collect(draggedId)
    if (inBlock.has(targetId)) return   // cannot drop a task into itself / its own subtree

    const block = flat.filter(t => inBlock.has(t.id))
    const rest  = flat.filter(t => !inBlock.has(t.id))
    const ti    = rest.findIndex(t => t.id === targetId)
    if (ti < 0) return
    const insertAt = place === 'before' ? ti : ti + 1   // 'inside' drops as target's first child
    const newFlat  = [...rest.slice(0, insertAt), ...block, ...rest.slice(insertAt)]
    const newParent: string | null = place === 'inside' ? targetId : (target.parent_id ?? null)

    // Compute the minimal set of field changes (position for anyone who moved,
    // parent for the dragged task if it changed).
    type Chg = { id: string; patch: Partial<ProjectTask>; before: Partial<ProjectTask> }
    const changes: Chg[] = []
    newFlat.forEach((t, i) => {
      const patch: Partial<ProjectTask> = {}
      const before: Partial<ProjectTask> = {}
      if (t.position !== i) { patch.position = i; before.position = t.position }
      if (t.id === draggedId && (t.parent_id ?? null) !== newParent) {
        ;(patch as { parent_id: string | null }).parent_id = newParent
        ;(before as { parent_id: string | null }).parent_id = t.parent_id ?? null
      }
      if (Object.keys(patch).length) changes.push({ id: t.id, patch, before })
    })
    if (!changes.length) return

    const applyAll = async (ctx: HistoryCtx, list: { id: string; patch: Partial<ProjectTask> }[]) => {
      for (const c of list) await projectsApi.updateTask(projectId, ctx.resolve(c.id), c.patch as never)
      refresh()
    }
    pushHistory({
      label: 'task-reorder',
      undo: ctx => applyAll(ctx, changes.map(c => ({ id: c.id, patch: c.before }))),
      redo: ctx => applyAll(ctx, changes.map(c => ({ id: c.id, patch: c.patch }))),
    })
    // Fire immediately (ids are real here).
    void (async () => {
      for (const c of changes) await projectsApi.updateTask(projectId, c.id, c.patch as never)
      refresh()
    })()
  }
  const onRowDragStart = (taskId: string, e: React.DragEvent) => {
    setDragId(taskId)
    if (dragGhostRef.current) e.dataTransfer.setDragImage(dragGhostRef.current, 0, 0)
    e.dataTransfer.effectAllowed = 'move'
    try { e.dataTransfer.setData('text/plain', taskId) } catch { /* some browsers forbid it */ }
  }
  const onRowDragOver = (taskId: string, place: 'before' | 'after' | 'inside') => {
    if (!dragId || taskId === dragId) return
    setDropHint(prev => (prev && prev.id === taskId && prev.place === place) ? prev : { id: taskId, place })
  }
  const onRowDrop = (taskId: string) => {
    if (dragId && dropHint && dropHint.id === taskId) reorderTaskCmd(dragId, dropHint.id, dropHint.place)
    setDragId(null); setDropHint(null)
  }
  const onRowDragEnd = () => { setDragId(null); setDropHint(null) }

  // Duplicate a task (copies the main fields onto a freshly created one).
  const duplicateTask = async (taskId: string) => {
    const src = allTasks.find(x => x.id === taskId); if (!src || !id) return
    const projectId = id
    const idx = allTasks.findIndex(x => x.id === taskId)
    // Same payload as before: the copy keeps the fields, not the WBS code nor the dates.
    const copy: TaskSnapshot = {
      ...snapshotTask(src), name: `${src.name} (${t('common_copy', { defaultValue: 'copie' })})`,
      position: idx + 1, wbs: '', start_date: null, end_date: null,
    }
    const newId = await recreateTask(projectId, copy, src.parent_id)
    refresh()
    pushHistory({
      label: 'task-duplicate',
      undo: async ctx => { await projectsApi.deleteTask(projectId, ctx.resolve(newId)); setSelectedId(null); refresh() },
      redo: async ctx => {
        const again = await recreateTask(projectId, copy, src.parent_id ? ctx.resolve(src.parent_id) : null)
        ctx.remap(newId, again)
        refresh()
      },
    })
  }
  const summaryIds = useCallback(() => new Set(allTasks.filter(tk => allTasks.some(c => c.parent_id === tk.id)).map(tk => tk.id)), [allTasks])
  const expandAll   = () => setCollapsed(new Set())
  const collapseAll = () => setCollapsed(summaryIds())

  // ── Export ───────────────────────────────────────────────────────────────────
  const download = (blob: Blob, name: string) => {
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a'); a.href = url; a.download = name; a.click()
    URL.revokeObjectURL(url)
  }
  const exportCsv = () => {
    const esc = (s: unknown) => `"${String(s ?? '').replace(/"/g, '""')}"`
    const header = ['WBS', t('proj_col_task', { defaultValue: 'Nom' }), 'Type', t('proj_col_status', { defaultValue: 'Statut' }), t('proj_col_priority', { defaultValue: 'Priorité' }), t('proj_col_start', { defaultValue: 'Début' }), t('proj_col_end', { defaultValue: 'Fin' }), t('proj_col_duration', { defaultValue: 'Durée' }), '%', t('proj_col_predecessors', { defaultValue: 'Préd.' }), t('proj_resources')]
    const rows = allTasks.map(tk => {
      const res = assignments.filter(a => a.task_id === tk.id).map(a => resources.find(r => r.id === a.resource_id)?.name).filter(Boolean).join(', ')
      return [tk.wbs, tk.name, tk.task_type, tk.status, tk.priority, format(schedStart(tk, projectStart), 'yyyy-MM-dd'), format(schedEnd(tk, projectStart), 'yyyy-MM-dd'), tk.duration_days, effProgress.get(tk.id) ?? tk.progress, predecessorText(tk.id), res].map(esc).join(',')
    })
    const csv = '﻿' + [header.map(esc).join(','), ...rows].join('\r\n')
    download(new Blob([csv], { type: 'text/csv;charset=utf-8' }), `${project?.title || 'projet'}.csv`)
  }
  const exportGanttPng = () => {
    const off = document.createElement('canvas')
    const r = new GanttRenderer(off)
    const fullW = Math.max(1, totalDays * dayW)
    r.resize(fullW, ganttH)
    r.render(tasks, deps, projectStart, totalDays, 0, fullW, getDateLocale(i18n.language), dayW)
    off.toBlob((b) => { if (b) download(b, `${project?.title || 'projet'}-gantt.png`) }, 'image/png')
  }

  // MenuDropdown gère le clic extérieur ; on ferme en plus au défilement / Échap.
  useEffect(() => {
    if (!ctxMenu) return
    const close = () => setCtxMenu(null)
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setCtxMenu(null) }
    window.addEventListener('scroll', close, true)
    window.addEventListener('keydown', onKey)
    return () => { window.removeEventListener('scroll', close, true); window.removeEventListener('keydown', onKey) }
  }, [ctxMenu])

  // Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du ruban.
  // Appelé AVANT tout return anticipé (règle des hooks). `defaultTab` = 'home', le
  // premier onglet non-Fichier de `projRibbon`.
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_PROJECTS,
    labels: backstageLabels(t),
    startContent: <ProjectsStartContent />,
    defaultTab: 'home',
    openKey: id,
    doc: {
      info: (
        <BackstageInfo
          title={titleDraft}
          onTitleChange={setTitleDraft}
          onTitleCommit={() => { if (titleDraft && titleDraft !== project?.title) updateProjectMut.mutate({ title: titleDraft }) }}
          extension=".kbprj"
          subtitle={t('proj_page_projects', { defaultValue: 'Projet' })}
          general={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('proj_page_projects', { defaultValue: 'Projet' })],
            ...(project?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(project.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
          stats={[
            [t('proj_grp_tasks', { defaultValue: 'Tâches' }), allTasks.length],
            [t('proj_resources', { defaultValue: 'Ressources' }), resources.length],
            [t('proj_estimated_total', { defaultValue: 'Charge estimée (h)' }), Math.round(allTasks.reduce((s, tk) => s + (tk.estimated_hours ?? 0), 0))],
            [t('proj_spent_total', { defaultValue: 'Temps passé (h)' }), Math.round(allTasks.reduce((s, tk) => s + (tk.spent_hours ?? 0), 0))],
          ]}
        />
      ),
      // Project settings live in the File tab, where settings belong — not in a
      // side rail too narrow for them.
      extra: [{
        id: 'properties',
        label: t('proj_props_tab', { defaultValue: 'Propriétés' }),
        icon: <Info size={17} />,
        content: project ? <ProjectPropertiesPanel project={project} onSave={saveProjectProps} /> : null,
      }, {
        id: 'produce',
        label: t('proj_produce_docs', { defaultValue: 'Produire un document' }),
        icon: <FileOutput size={17} />,
        content: <DocumentProductionPanel projectId={id!}
          onOpenDocument={docId => navigate(`/office/documents/${docId}`)} />,
      }, {
        id: 'settings',
        label: t('proj_modules_panel', { defaultValue: 'Modules du projet' }),
        icon: <SlidersHorizontal size={17} />,
        content: <ProjectSettingsPanel projectId={id!} />,
      }],
      onPrint: () => window.print(),
      onClose: () => navigate('/office/projects'),
    },
  })
  const updateResMut  = useMutation({
    mutationFn: ({ rid, data }: { rid: string; data: { hourly_rate: number | null } }) => projectsApi.updateResource(id!, rid, data),
    onSuccess: refresh,
  })

  if (isLoading) return <div className="flex items-center justify-center h-full"><Loader2 size={24} className="animate-spin text-text-tertiary" /></div>
  if (isError || !project) return <div className="flex items-center justify-center h-full gap-2 text-danger"><AlertTriangle size={18} /><span className="text-sm">{t('proj_not_found')}</span></div>

  const ctxTask = ctxMenu ? allTasks.find(tk => tk.id === ctxMenu.taskId) : null

  // ── Macros API : surface exposée à l'objet global `Kubuno` des scripts.
  //    Read-only for this first version. Returns the `Kubuno` global object.
  const makeApi = () => {
    const Project = {
      /** Number of tasks in the project. */
      getTaskCount: () => allTasks.length,
      /** All tasks with id, name and scheduled start/end dates (ISO, CPM-consistent). */
      getTasks: () => allTasks.map(tk => ({
        id: tk.id,
        name: tk.name,
        start: format(schedStart(tk, projectStart), 'yyyy-MM-dd'),
        end: format(schedEnd(tk, projectStart), 'yyyy-MM-dd'),
      })),
    }
    const App = {
      getType: () => 'project',
      getId: () => id,
      toast: (msg: unknown) => console.log(String(msg)),
      log: (msg: unknown) => console.log(String(msg)),
    }
    return { Project, App }
  }

  const selId = selectedTask?.id
  const projRibbon: RibbonTab[] = [
    // ── Accueil ──
    { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
      genericClipboardGroup(t),
      // Opérations sur le fichier (jadis groupe « Fichier ») déplacées dans un groupe
      // « Projet » : les actions de fichier vivent désormais dans le backstage (onglet
      // Fichier), mais Nouveau/Dupliquer/Export restent accessibles sur le ruban.
      { id: 'project', label: t('proj_page_projects', { defaultValue: 'Projet' }), items: [
        { id: 'new', kind: 'button', icon: <FilePlus size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: () => createProjMut.mutate() },
        { id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: () => duplicateProjMut.mutate() },
        { id: 'exp-csv', kind: 'button', icon: <Download size={15} />, label: t('proj_export_csv', { defaultValue: 'Export CSV' }), onClick: exportCsv },
        { id: 'exp-png', kind: 'button', icon: <Download size={15} />, label: t('proj_export_png', { defaultValue: 'Export PNG' }), onClick: exportGanttPng },
      ] },
      { id: 'tasks', label: t('proj_grp_tasks', { defaultValue: 'Tâches' }), items: [
        { id: 'it', kind: 'button', size: 'large', icon: <Plus size={18} />, label: t('proj_insert_task', { defaultValue: 'Tâche' }), onClick: () => insertTask('task') },
        { id: 'ms', kind: 'button', icon: <Milestone size={15} />, label: t('proj_type_milestone'), onClick: () => insertTask('milestone') },
        { id: 'sum', kind: 'button', icon: <FolderKanban size={15} />, label: t('proj_type_summary'), onClick: () => insertTask('summary') },
        { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete'), disabled: !selectedTask, onClick: () => { if (selId) void deleteTaskCmd(selId) } },
      ] },
      { id: 'progress', label: t('proj_grp_progress', { defaultValue: 'Avancement' }), items: [
        ...[0, 25, 50, 75, 100].map(p => ({ id: 'p' + p, kind: 'button' as const, icon: <span className="text-[10px] font-bold">{p}</span>, tooltip: p + '%', disabled: !selectedTask || isParent(selId), onClick: () => setProgress(p) })),
        { id: 'cpm', kind: 'button', icon: computeCpmMut.isPending ? <Loader2 size={15} className="animate-spin" /> : <ListChecks size={15} />, label: t('proj_respect_links', { defaultValue: 'Replanifier' }), disabled: computeCpmMut.isPending, onClick: () => computeCpmMut.mutate() },
      ] },
      // Hidden entirely when the project does not use baselines.
      ...(uses('baselines') ? ([{ id: 'baseline', label: t('proj_grp_baseline', { defaultValue: 'Plan de référence' }), items: [
        { id: 'bl-capture', kind: 'button', icon: captureBaselineMut.isPending ? <Loader2 size={15} className="animate-spin" /> : <Flag size={15} />, label: t('proj_baseline_capture', { defaultValue: 'Définir' }), disabled: captureBaselineMut.isPending, onClick: () => captureBaselineMut.mutate(undefined) },
        ...((baselines?.length ?? 0) > 0 ? [
          { id: 'bl-select', kind: 'dropdown' as const, width: 160, value: activeBaselineId ?? '',
            options: [{ value: '', label: t('proj_baseline_none', { defaultValue: 'Comparer : aucun' }) }, ...(baselines ?? []).map(b => ({ value: b.id, label: b.is_primary ? `★ ${b.name}` : b.name }))],
            onChange: (v: string) => setActiveBaselineId(v || null) },
          { id: 'bl-del', kind: 'button' as const, icon: <Trash2 size={15} />, tooltip: t('proj_baseline_delete', { defaultValue: 'Supprimer ce plan de référence' }), disabled: !activeBaselineId, onClick: () => { if (activeBaselineId) deleteBaselineMut.mutate(activeBaselineId) } },
          { id: 'bl-rename', kind: 'button' as const, icon: <PenLine size={15} />, label: t('proj_baseline_rename', { defaultValue: 'Renommer' }), disabled: !activeBaselineId, onClick: () => {
            if (!activeBaselineId) return
            void (async () => {
              const name = await prompt({
                title: t('proj_baseline_rename', { defaultValue: 'Renommer' }),
                message: t('proj_baseline_rename_msg', { defaultValue: 'Nom du plan de référence :' }),
                defaultValue: activeBaseline?.name ?? '',
                confirmLabel: t('common_save', { defaultValue: 'Enregistrer' }),
              })
              if (!name) return                    // cancelled or empty: keep the current name
              renameBaselineMut.mutate({ bid: activeBaselineId, name })
            })()
          } },
          { id: 'bl-primary', kind: 'button' as const, icon: <Star size={15} />, label: t('proj_baseline_set_primary', { defaultValue: 'Définir comme référence' }), disabled: !activeBaselineId || !!activeBaseline?.is_primary, onClick: () => { if (activeBaselineId) setPrimaryBaselineMut.mutate(activeBaselineId) } },
        ] : []),
      ] }] as RibbonGroup[]) : []),
    ] },
    // ── Tâche ──
    { id: 'task', label: t('proj_tab_task', { defaultValue: 'Tâche' }), groups: [
      { id: 'insert', label: t('proj_grp_insert', { defaultValue: 'Insérer' }), items: [
        { id: 't', kind: 'button', icon: <Plus size={15} />, label: t('proj_insert_task', { defaultValue: 'Tâche' }), onClick: () => insertTask('task') },
        { id: 'm', kind: 'button', icon: <Milestone size={15} />, label: t('proj_type_milestone'), onClick: () => insertTask('milestone') },
        { id: 's', kind: 'button', icon: <FolderKanban size={15} />, label: t('proj_type_summary'), onClick: () => insertTask('summary') },
      ] },
      { id: 'hier', label: t('proj_grp_hier', { defaultValue: 'Hiérarchie' }), items: [
        { id: 'indent', kind: 'button', icon: <Indent size={15} />, label: t('proj_indent', { defaultValue: 'Abaisser' }), disabled: !selectedTask, onClick: () => selId && indentTask(selId) },
        { id: 'outdent', kind: 'button', icon: <Outdent size={15} />, label: t('proj_outdent', { defaultValue: 'Élever' }), disabled: !selectedTask?.parent_id, onClick: () => selId && outdentTask(selId) },
      ] },
      { id: 'move', label: t('proj_grp_move', { defaultValue: 'Déplacer' }), items: [
        { id: 'up', kind: 'button', icon: <ArrowUp size={15} />, label: t('proj_move_up', { defaultValue: 'Monter' }), disabled: !selectedTask, onClick: () => selId && moveTask(selId, 'up') },
        { id: 'down', kind: 'button', icon: <ArrowDown size={15} />, label: t('proj_move_down', { defaultValue: 'Descendre' }), disabled: !selectedTask, onClick: () => selId && moveTask(selId, 'down') },
      ] },
      { id: 'tedit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'dup', kind: 'button', icon: <Copy size={15} />, label: t('proj_duplicate_task', { defaultValue: 'Dupliquer' }), disabled: !selectedTask, onClick: () => selId && duplicateTask(selId) },
        { id: 'del2', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete'), disabled: !selectedTask, onClick: () => { if (selId) void deleteTaskCmd(selId) } },
      ] },
    ] },
    // ── Liaisons ──
    { id: 'links', label: t('proj_grp_links', { defaultValue: 'Liaisons' }), groups: [
      { id: 'links', label: t('proj_grp_links', { defaultValue: 'Liaisons' }), items: [
        { id: 'link', kind: 'button', size: 'large', icon: <Link2 size={18} />, label: t('proj_link', { defaultValue: 'Lier' }), disabled: selIndex <= 0, onClick: linkSelectedToPrev },
        { id: 'unlink', kind: 'button', icon: <Link2Off size={15} />, label: t('proj_unlink', { defaultValue: 'Délier' }), disabled: !selectedTask, onClick: unlinkSelected },
      ] },
    ] },
    // ── Format ──
    // ── Périmètre ── ce que le projet s'engage à faire, avant tout planning.
    { id: 'scope', label: t('proj_tab_scope', { defaultValue: 'Périmètre' }), groups: [
      { id: 'framing', label: t('proj_grp_framing', { defaultValue: 'Cadrage' }), items: [
        uses('plans') && { id: 'plans', kind: 'toggle' as const, paletteTile: true, icon: <BookOpen size={15} />, label: t('proj_plans', { defaultValue: 'Plans' }), active: activeTab === 'plans', onClick: () => setActiveTab('plans') },
        uses('charter') && { id: 'charter', kind: 'toggle' as const, paletteTile: true, icon: <ScrollText size={15} />, label: t('proj_charter', { defaultValue: 'Charte' }), active: activeTab === 'charter', onClick: () => setActiveTab('charter') },
        uses('wbs') && { id: 'wbs', kind: 'toggle' as const, paletteTile: true, icon: <ListTree size={15} />, label: t('proj_wbs', { defaultValue: 'Découpage' }), active: activeTab === 'wbs', onClick: () => setActiveTab('wbs') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'commitments', label: t('proj_grp_commitments', { defaultValue: 'Engagements' }), items: [
        uses('deliverables') && { id: 'deliverables', kind: 'toggle' as const, paletteTile: true, icon: <Package size={15} />, label: t('proj_deliverables', { defaultValue: 'Livrables' }), active: activeTab === 'deliverables', onClick: () => setActiveTab('deliverables') },
        uses('requirements') && { id: 'requirements', kind: 'toggle' as const, paletteTile: true, icon: <ClipboardList size={15} />, label: t('proj_requirements', { defaultValue: 'Exigences' }), active: activeTab === 'requirements', onClick: () => setActiveTab('requirements') },
        uses('requirements') && { id: 'traceability', kind: 'toggle' as const, paletteTile: true, icon: <Waypoints size={15} />, label: t('proj_traceability', { defaultValue: 'Traçabilité' }), active: activeTab === 'traceability', onClick: () => setActiveTab('traceability') },
      ].filter(Boolean) as RibbonItem[] },
    ] },
    // ── Pilotage ── ce qui se surveille pendant que le projet tourne.
    { id: 'control', label: t('proj_tab_control', { defaultValue: 'Pilotage' }), groups: [
      { id: 'uncertainty', label: t('proj_grp_uncertainty', { defaultValue: 'Aléas' }), items: [
        uses('risks') && { id: 'risks', kind: 'toggle' as const, paletteTile: true, icon: <ShieldAlert size={15} />, label: t('proj_risks', { defaultValue: 'Risques' }), active: activeTab === 'risks', onClick: () => setActiveTab('risks') },
        uses('issues') && { id: 'issues', kind: 'toggle' as const, paletteTile: true, icon: <TriangleAlert size={15} />, label: t('proj_issues', { defaultValue: 'Incidents' }), active: activeTab === 'issues', onClick: () => setActiveTab('issues') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'people', label: t('proj_grp_people', { defaultValue: 'Parties prenantes' }), items: [
        uses('stakeholders') && { id: 'stakeholders', kind: 'toggle' as const, paletteTile: true, icon: <UsersRound size={15} />, label: t('proj_stakeholders', { defaultValue: 'Registre' }), active: activeTab === 'stakeholders', onClick: () => setActiveTab('stakeholders') },
        uses('stakeholders') && { id: 'raci', kind: 'toggle' as const, paletteTile: true, icon: <Grid3x3 size={15} />, label: t('proj_raci', { defaultValue: 'RACI' }), active: activeTab === 'raci', onClick: () => setActiveTab('raci') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'conformance', label: t('proj_grp_conformance', { defaultValue: 'Qualité' }), items: [
        uses('quality') && { id: 'quality', kind: 'toggle' as const, paletteTile: true, icon: <BadgeCheck size={15} />, label: t('proj_quality', { defaultValue: 'Indicateurs' }), active: activeTab === 'quality', onClick: () => setActiveTab('quality') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'closing', label: t('proj_grp_closing', { defaultValue: 'Clôture' }), items: [
        uses('closure') && { id: 'closure', kind: 'toggle' as const, paletteTile: true, icon: <FlagTriangleRight size={15} />, label: t('proj_closure', { defaultValue: 'Bilan' }), active: activeTab === 'closure', onClick: () => setActiveTab('closure') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'change', label: t('proj_grp_change', { defaultValue: 'Changements' }), items: [
        uses('changes') && { id: 'changes', kind: 'toggle' as const, paletteTile: true, icon: <GitPullRequestArrow size={15} />, label: t('proj_changes', { defaultValue: 'Demandes' }), active: activeTab === 'changes', onClick: () => setActiveTab('changes') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'comms', label: t('proj_grp_comms', { defaultValue: 'Communication' }), items: [
        uses('communications') && { id: 'communications', kind: 'toggle' as const, paletteTile: true, icon: <Megaphone size={15} />, label: t('proj_comms', { defaultValue: 'Plan' }), active: activeTab === 'communications', onClick: () => setActiveTab('communications') },
        uses('decisions') && { id: 'decisions', kind: 'toggle' as const, paletteTile: true, icon: <Gavel size={15} />, label: t('proj_decisions', { defaultValue: 'Décisions' }), active: activeTab === 'decisions', onClick: () => setActiveTab('decisions') },
      ].filter(Boolean) as RibbonItem[] },
      { id: 'money', label: t('proj_grp_money', { defaultValue: 'Coûts' }), items: [
        uses('procurement') && { id: 'procurement', kind: 'toggle' as const, paletteTile: true, icon: <Handshake size={15} />, label: t('proj_procurement', { defaultValue: 'Contrats' }), active: activeTab === 'procurement', onClick: () => setActiveTab('procurement') },
        uses('costs') && { id: 'costs', kind: 'toggle' as const, paletteTile: true, icon: <TrendingUp size={15} />, label: t('proj_costs', { defaultValue: 'Valeur acquise' }), active: activeTab === 'costs', onClick: () => setActiveTab('costs') },
        uses('costs') && { id: 'expenses', kind: 'toggle' as const, paletteTile: true, icon: <Receipt size={15} />, label: t('proj_expenses', { defaultValue: 'Dépenses' }), active: activeTab === 'expenses', onClick: () => setActiveTab('expenses') },
      ].filter(Boolean) as RibbonItem[] },
    ] },
    // ── Format ──
    { id: 'format', label: t('proj_tab_format', { defaultValue: 'Format' }), groups: [
      { id: 'status', label: t('proj_col_status', { defaultValue: 'Statut' }), items: [
        { id: 'st-todo', kind: 'button', icon: <Circle size={15} />, label: t('proj_status_not_started', { defaultValue: 'À faire' }), disabled: !selectedTask, onClick: () => selId && setStatus(selId, 'not_started') },
        { id: 'st-prog', kind: 'button', icon: <Loader2 size={15} />, label: t('proj_status_in_progress', { defaultValue: 'En cours' }), disabled: !selectedTask, onClick: () => selId && setStatus(selId, 'in_progress') },
        { id: 'st-done', kind: 'button', icon: <CheckCircle2 size={15} />, label: t('proj_status_completed', { defaultValue: 'Terminé' }), disabled: !selectedTask, onClick: () => { if (selId) setStatus(selId, 'completed') } },
      ] },
      { id: 'prio', label: t('proj_col_priority', { defaultValue: 'Priorité' }), items: [
        ...([['low', '#34a853'], ['medium', '#fbbc04'], ['high', '#ea4335'], ['critical', '#b80672']] as Array<[string, string]>).map(([p, c]) => ({ id: 'pr-' + p, kind: 'button' as const, icon: <Flag size={15} style={{ color: c }} />, label: t('proj_priority_' + p, { defaultValue: p }), disabled: !selectedTask, onClick: () => selId && setPriority(selId, p) })),
      ] },
    ] },
    // ── Affichage ──
    { id: 'view', label: t('proj_grp_view', { defaultValue: 'Affichage' }), groups: [
      // The plan views (Gantt, Table, Calendar…) moved to a centred tab strip at the
      // top of the plan area (see `planViews`); the ribbon keeps only the toggles.
      { id: 'show', label: t('proj_grp_show', { defaultValue: 'Afficher' }), items: [
        { id: 'tl', kind: 'toggle', icon: <CalendarRange size={15} />, label: t('proj_timeline', { defaultValue: 'Chronologie' }), active: showTimeline, onClick: () => setShowTimeline(s => !s) },
        { id: 'filter', kind: 'toggle', icon: <Filter size={15} />, label: t('proj_filters', { defaultValue: 'Filtres' }), active: showFilters || filterActive, onClick: () => setShowFilters(s => !s) },
        { id: 'info', kind: 'button', icon: <Info size={15} />, label: t('proj_info', { defaultValue: 'Informations' }), onClick: () => openPanel('inspector') },
        // The dock "Ressources" panel is superseded by the full Ressources view (a
        // plan-view tab); its ribbon button is removed to avoid two ways in.
      ] },
      { id: 'outline', label: t('proj_grp_outline', { defaultValue: 'Plan' }), items: [
        { id: 'exp', kind: 'button', icon: <ChevronsUpDown size={15} />, label: t('proj_expand_all', { defaultValue: 'Tout déplier' }), onClick: expandAll },
        { id: 'col', kind: 'button', icon: <ChevronsDownUp size={15} />, label: t('proj_collapse_all', { defaultValue: 'Tout replier' }), onClick: collapseAll },
      ] },
      { id: 'zoom', label: t('proj_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'zout', kind: 'button', icon: <ZoomOut size={15} />, label: t('proj_zoom_out', { defaultValue: 'Arrière' }), disabled: zoom === 'month', onClick: () => setZoom(z => z === 'day' ? 'week' : 'month') },
        { id: 'zin', kind: 'button', icon: <ZoomIn size={15} />, label: t('proj_zoom_in', { defaultValue: 'Avant' }), disabled: zoom === 'day', onClick: () => setZoom(z => z === 'month' ? 'week' : 'day') },
      ] },
    ] },
  ]

  // ── Docking panels (Inspecteur + Ressources) ──
  const inspectorPanel = (
    <div className="h-full w-full bg-surface-0 overflow-y-auto">
      {selectedTask ? (<>
        <TaskDetailPanel task={selectedTask} resources={resources} assignments={assignments} versions={versions} projectId={id!} showTimeLog={uses('timelog')} showVersions={uses('roadmap')}
          onUpdate={d => patchTaskCmd(selectedTask.id, d)}
          onAssign={rid => assignResCmd(selectedTask.id, rid)}
          onUnassign={rid => unassignResCmd(selectedTask.id, rid)}
          onClose={() => { if (isMobileView) setMobilePanel(null); else dockRef.current?.close('inspector') }}
          hideHeader={isMobileView}
          isParent={isParent(selectedTask.id)} rollupProgress={effProgress.get(selectedTask.id) ?? selectedTask.progress} />
        <div className="p-3 border-t border-border">
          <p className="text-xs font-medium text-text-secondary mb-2">{t('proj_predecessors', { defaultValue: 'Prédécesseurs' })}</p>
          {deps.filter(d => d.to_task_id === selectedTask.id).map(dep => {
            const from = allTasks.find(tk => tk.id === dep.from_task_id)
            return (
              <div key={dep.id} className="flex items-center gap-1.5 mb-1.5">
                <span className="flex-1 min-w-0 truncate text-xs text-text-primary" title={from?.name}>{taskNumber.get(dep.from_task_id)} · {from?.name}</span>
                <Dropdown width={72} height={28} fontSize={12} value={dep.dep_type} onChange={v => setDep(dep.from_task_id, selectedTask.id, v, dep.lag_days)}
                  options={['FS', 'SS', 'FF', 'SF'].map(v => ({ value: v, label: v }))} />
                <input type="number" value={dep.lag_days} title={t('proj_lag', { defaultValue: 'Décalage (jours)' })}
                  onChange={e => setDep(dep.from_task_id, selectedTask.id, dep.dep_type, parseInt(e.target.value) || 0)}
                  className="w-12 h-7 text-xs text-right border border-border rounded outline-none focus:border-primary px-1.5" />
                <button onClick={() => { void removeDepsCmd([dep]) }} className="text-text-tertiary hover:text-danger p-0.5 flex-shrink-0"><Trash2 size={13} /></button>
              </div>
            )
          })}
          {deps.filter(d => d.to_task_id === selectedTask.id).length === 0 && (
            <p className="text-xs text-text-tertiary italic">{t('proj_no_predecessors', { defaultValue: 'Aucun prédécesseur' })}</p>
          )}
          {/* Ajout d'un prédécesseur */}
          <div className="mt-2">
            <Dropdown className="w-full" height={36} fontSize={14} value="" onChange={v => { if (v) void addDepCmd(v, selectedTask.id) }}
              options={[{ value: '', label: t('proj_add_predecessor', { defaultValue: '+ Ajouter un prédécesseur…' }) },
                        ...allTasks.filter(tk => tk.id !== selectedTask.id && !deps.some(d => d.to_task_id === selectedTask.id && d.from_task_id === tk.id))
                          .map(tk => ({ value: tk.id, label: `${taskNumber.get(tk.id)} · ${tk.name}` }))]} />
          </div>
        </div>
      </>) : (
        <div className="p-4 text-xs text-text-tertiary text-center">{t('proj_select_task_hint', { defaultValue: 'Sélectionnez une tâche pour voir ses détails.' })}</div>
      )}
    </div>
  )
  const memberItems: MenuItem[] = (membersQuery.data && membersQuery.data.length > 0)
    ? membersQuery.data.map(m => ({
        type: 'action' as const,
        label: m.display_name,
        shortcut: m.email,
        icon: m.avatar_url
          ? <img src={m.avatar_url} alt="" className="w-4 h-4 rounded-full object-cover" />
          : <UsersRound size={14} />,
        onClick: () => createResMut.mutate({ name: m.display_name, user_id: m.id, color: memberColor(m.id) }),
      }))
    : [{ type: 'action' as const, label: t('proj_no_org_members', { defaultValue: 'Aucun autre membre dans votre unité' }), disabled: true, onClick: () => {} }]
  const resourcesPanel = (
    <div className="h-full w-full bg-surface-0 overflow-y-auto p-3">
      <p className="text-xs text-text-secondary mb-3">
        {t('proj_resources_intro', { defaultValue: 'Les personnes et ressources que vous pouvez affecter aux tâches du projet.' })}
      </p>

      {/* Primary action: add a real person from the organizational unit. */}
      <button onClick={e => memberMenu.openAt(e.currentTarget.getBoundingClientRect().left, e.currentTarget.getBoundingClientRect().bottom + 4)}
        className="w-full flex items-center justify-center gap-2 mb-2 h-9 rounded-lg bg-primary text-white text-sm font-medium hover:opacity-90">
        <UsersRound size={15} /> {t('proj_add_member', { defaultValue: 'Ajouter un membre de mon unité' })}
      </button>
      {memberMenu.pos && <MenuDropdown items={memberItems} pos={memberMenu.pos} onClose={memberMenu.close} minWidth={240} />}

      {/* Secondary: a free-form external resource (subcontractor, role placeholder). */}
      <div className="flex gap-2 mb-4">
        <Input type="text" className="flex-1" placeholder={t('proj_resource_ext_ph', { defaultValue: 'Ressource externe (sous-traitant…)' })}
          value={newResName} onChange={e => setNewResName(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && newResName.trim()) createResMut.mutate({ name: newResName.trim() }) }} />
        <Button onClick={() => newResName.trim() && createResMut.mutate({ name: newResName.trim() })} disabled={!newResName.trim()} loading={createResMut.isPending}>{t('proj_add')}</Button>
      </div>

      {resources.length === 0 ? (
        <div className="text-center py-10 px-4">
          <UsersRound size={28} className="mx-auto text-text-tertiary opacity-40 mb-2" />
          <p className="text-sm text-text-secondary">{t('proj_no_resources_title', { defaultValue: 'Personne sur le projet pour l’instant' })}</p>
          <p className="text-xs text-text-tertiary mt-1">{t('proj_no_resources_hint2', { defaultValue: 'Ajoutez des membres, puis affectez-les aux tâches.' })}</p>
        </div>
      ) : (
        <div className="space-y-2">
          {resources.map(r => {
            const taskCount = assignments.filter(a => a.resource_id === r.id).length
            return (
              <div key={r.id} className="flex items-start gap-2.5 p-2.5 border border-border rounded-xl bg-surface-1">
                {r.avatar_url
                  ? <img src={r.avatar_url} alt="" className="w-9 h-9 rounded-full object-cover flex-shrink-0" />
                  : <div className="w-9 h-9 rounded-full flex items-center justify-center text-white text-sm font-bold flex-shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</div>}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-1.5">
                    <p className="text-sm font-medium text-text-primary truncate">{r.name}</p>
                    <span className={`shrink-0 px-1.5 py-px rounded text-[10px] font-medium ${r.user_id ? 'bg-primary/10 text-primary' : 'bg-surface-2 text-text-secondary'}`}>
                      {r.user_id ? t('proj_res_member', { defaultValue: 'Membre' }) : t('proj_res_external', { defaultValue: 'Externe' })}
                    </span>
                  </div>
                  <p className="text-xs text-text-tertiary truncate mt-0.5">
                    {t('proj_res_task_count', { count: taskCount, defaultValue: `${taskCount} tâche(s) affectée(s)` })} · {t('proj_res_capacity', { defaultValue: 'Capacité' })} {Math.round(r.capacity * 100)}%
                  </p>
                  <div className="flex items-center gap-1.5 mt-1.5">
                    <span className="text-[11px] text-text-tertiary shrink-0">{t('proj_res_rate', { defaultValue: 'Taux/h' })}</span>
                    <ResourceRateField rate={r.hourly_rate}
                      onCommit={rate => updateResMut.mutate({ rid: r.id, data: { hourly_rate: rate } })} />
                  </div>
                </div>
                <button onClick={() => deleteResMut.mutate(r.id)} className="text-text-tertiary hover:text-danger p-1 flex-shrink-0 -mt-0.5" title={t('common_delete', { defaultValue: 'Supprimer' })}><Trash2 size={14} /></button>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
  // Immersion LECTURE mobile.
  const readMobile = isMobileView && mode === 'read'
  const summaryTask = summaryId ? allTasks.find(tk => tk.id === summaryId) ?? null : null
  const projPanels: Record<string, DockPanel> = {
    inspector: { label: t('proj_info', { defaultValue: 'Informations' }), render: () => inspectorPanel },
    resources: { label: t('proj_resources'), render: () => resourcesPanel },
  }

  return (
    <OfficeShell
      // Lecture mobile : ruban vide → plein écran (pas de barre du bas).
      ribbon={readMobile ? [] : [fileTab, ...projRibbon]}
      hideHeaderActions={readMobile}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_PROJECTS}
      chromeless
      topbarHeight={64}
      onBack={() => navigate('/office/projects')}
      titleIcon={<div className="w-3 h-3 rounded-full flex-shrink-0 ring-1 ring-white/40" style={{ background: project.color }} />}
      title={titleDraft}
      onTitleChange={setTitleDraft}
      onTitleCommit={() => { if (titleDraft && titleDraft !== project.title) updateProjectMut.mutate({ title: titleDraft }) }}
      titlePlaceholder={t('common_untitled')}
      titleActions={<>
        {/* Mobile : bascule lecture ↔ édition (pastille « Modifier » en lecture). */}
        {readMobile ? (
          <button onClick={() => setMode('edit')}
            className="flex items-center gap-1.5 h-8 px-3 rounded-md bg-surface-0/15 text-white text-xs font-medium border border-white/25 hover:bg-surface-0/25 transition-colors flex-shrink-0"
            title={t('common_edit', { defaultValue: 'Modifier' })}>
            <PenLine size={15} /> {t('common_edit', { defaultValue: 'Modifier' })}
          </button>
        ) : isMobileView && (
          <button onClick={() => setMode('read')}
            className="p-1.5 rounded hover:bg-surface-0/10 transition-colors flex-shrink-0 text-white/90"
            title={t('doc_mode_read', { defaultValue: 'Lecture' })}>
            <Eye size={16} />
          </button>
        )}
        {!readMobile && <>
        {/* Immediate save: persist the current title (no reliable dirty signal → omit `dirty`). */}
        <SaveButton
          onSave={() => updateProjectMut.mutate({ title: titleDraft || project.title })}
          saving={updateProjectMut.isPending}
          label={t('doc_save', { defaultValue: 'Enregistrer' })}
        />
        <UndoRedoButtons
          onUndo={undoCmd} onRedo={redoCmd}
          canUndo={history.canUndo} canRedo={history.canRedo}
          undoLabel={t('common_undo', { defaultValue: 'Annuler' })}
          redoLabel={t('common_redo', { defaultValue: 'Rétablir' })}
        />
        <button onClick={() => updateProjectMut.mutate({ is_starred: !project.is_starred })}
          className={`p-1.5 rounded hover:bg-surface-0/10 transition-colors flex-shrink-0 ${project.is_starred ? 'text-warning' : 'text-white/90'}`}
          title={project.is_starred ? t('proj_unstar', { defaultValue: 'Retirer des favoris' }) : t('proj_star', { defaultValue: 'Ajouter aux favoris' })}>
          <Star size={15} className={project.is_starred ? 'fill-warning text-warning' : ''} />
        </button>
        </>}
      </>}
      topbarActions={
        readMobile ? (
          // Immersion lecture : partage en icône seule.
          <button onClick={() => setShareOpen(true)} title={t('proj_share', { defaultValue: 'Partager' })}
            className="p-1.5 rounded hover:bg-surface-0/10 transition-colors flex-shrink-0 text-white/90">
            <Share2 size={16} />
          </button>
        ) : (
        <div className="flex items-center gap-2">
          <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
          <button onClick={() => setShareOpen(true)}
            className="flex items-center gap-1.5 h-8 px-3 rounded-md bg-surface-0/15 text-white text-sm font-medium border border-white/25 hover:bg-surface-0/25 transition-colors">
            <Share2 size={15} /> {t('proj_share', { defaultValue: 'Partager' })}</button>
        </div>
        )
      }
      onDelete={() => trashProjMut.mutate()}
      deleteTitle={t('proj_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('proj_delete_confirm_title', { defaultValue: 'Supprimer ce projet ?' }),
        message: t('proj_delete_confirm_msg', { defaultValue: 'Le projet sera déplacé dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }), variant: 'danger',
      }}
    >
      <div ref={workRef} className="relative flex flex-col flex-1 min-w-0 overflow-hidden" onMouseMove={onWorkMouseMove} onMouseLeave={() => publishCursor(null)}>

      {(() => {
      // The plan's views (Gantt, Table, Calendar, Workload, Network, Roadmap) live in
      // a centred tab strip at the top of the plan area rather than in the ribbon —
      // using the core's @ui Tabs primitive.
      const planViews = ([
        uses('schedule') && { id: 'gantt',    icon: BarChart2,    label: t('proj_tab_gantt') },
        uses('board')    && { id: 'board',    icon: KanbanSquare, label: t('proj_view_board', { defaultValue: 'Tableau' }) },
        uses('calendar') && { id: 'calendar', icon: CalendarDays, label: t('proj_view_calendar', { defaultValue: 'Calendrier' }) },
        uses('workload') && { id: 'resources', icon: Users,        label: t('proj_view_resources', { defaultValue: 'Ressources' }) },
        uses('network')  && { id: 'pert',     icon: Network,      label: t('proj_view_pert', { defaultValue: 'Réseau' }) },
        uses('roadmap')  && { id: 'roadmap',  icon: Milestone,    label: t('proj_roadmap', { defaultValue: 'Roadmap' }) },
      ].filter(Boolean) as TabDef<string>[])
      const planViewIds = new Set(planViews.map(v => v.id))
      const body = (
        <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
      {!isMobileView && planViews.length > 1 && (
        <div className="shrink-0 flex justify-center border-b border-border bg-surface-1">
          <Tabs tabs={planViews} value={planViewIds.has(activeTab) ? activeTab : ''} onChange={v => setActiveTab(v as typeof activeTab)} t={t} />
        </div>
      )}
      {activeBaseline && (
        <div className="flex items-center gap-3 px-3 py-1.5 border-b border-border bg-surface-1 shrink-0 text-xs flex-wrap">
          <Flag size={13} className="text-text-secondary" />
          <span className="text-text-secondary">
            {t('proj_baseline_compared', { defaultValue: 'Comparé au plan de référence' })}{' '}
            <span className="font-medium text-text-primary">« {activeBaseline.name} »</span>{' '}
            {t('proj_baseline_of', { defaultValue: 'du' })} {format(new Date(activeBaseline.captured_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })}
          </span>
          <span className="flex items-center gap-1 ml-auto"><span className="inline-block w-4 h-1 rounded-sm" style={{ background: CRITICAL_CLR }} /> {t('proj_baseline_late', { defaultValue: 'en retard' })}</span>
          <span className="flex items-center gap-1"><span className="inline-block w-4 h-1 rounded-sm" style={{ background: '#1e8e3e' }} /> {t('proj_baseline_early', { defaultValue: 'en avance' })}</span>
          <button onClick={() => setActiveBaselineId(null)} className="text-[var(--color-primary)] hover:underline">{t('proj_baseline_hide', { defaultValue: 'Masquer' })}</button>
        </div>
      )}
      {(showFilters || filterActive || sortBy || groupBy) && (
        <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-surface-1 shrink-0 flex-wrap">
          <Filter size={13} className="text-text-tertiary" />
          <input value={filterText} onChange={e => setFilterText(e.target.value)} placeholder={t('proj_filter_search', { defaultValue: 'Rechercher une tâche…' })}
            className="px-2 h-6 text-xs leading-none border border-border rounded outline-none focus:border-primary w-40" />
          <Dropdown height={24} fontSize={12} value={filterStatus} onChange={setFilterStatus} options={[
            { value: '', label: t('proj_all_statuses', { defaultValue: 'Tous statuts' }) },
            ...['not_started', 'in_progress', 'on_hold', 'completed', 'cancelled'].map(s => ({ value: s, label: t('proj_status_' + s, { defaultValue: s }) })),
          ]} />
          <Dropdown height={24} fontSize={12} value={filterPriority} onChange={setFilterPriority} options={[
            { value: '', label: t('proj_all_priorities', { defaultValue: 'Toutes priorités' }) },
            ...['low', 'medium', 'high', 'critical'].map(p => ({ value: p, label: t('proj_priority_' + p, { defaultValue: p }) })),
          ]} />
          <span className="w-px h-4 bg-border" />
          <Dropdown height={24} fontSize={12} value={sortBy} onChange={setSortBy} options={[
            { value: '', label: t('proj_sort_none', { defaultValue: 'Tri : WBS' }) },
            ...['name', 'start', 'end', 'duration', 'priority', 'progress', 'status'].map(k => ({ value: k, label: t('proj_sort_' + k, { defaultValue: k }) })),
          ]} />
          <Dropdown height={24} fontSize={12} value={groupBy} onChange={setGroupBy} options={[
            { value: '', label: t('proj_group_none', { defaultValue: 'Grouper : aucun' }) },
            ...['status', 'priority', 'resource'].map(k => ({ value: k, label: t('proj_group_' + k, { defaultValue: k }) })),
          ]} />
          {(filterActive || sortBy || groupBy) && (
            <button onClick={() => { setFilterText(''); setFilterStatus(''); setFilterPriority(''); setSortBy(''); setGroupBy('') }} className="text-xs text-primary hover:underline">{t('proj_clear_filters', { defaultValue: 'Effacer' })}</button>
          )}
          <span className="text-[11px] text-text-tertiary ml-auto">{t('proj_filter_count', { count: displayTasks.length, defaultValue: `${displayTasks.length} tâche(s)` })}</span>
        </div>
      )}
      {showTimeline && activeTab === 'gantt' && !isMobileView && (
        <TimelineBand tasks={allTasks} projectStart={projectStart} totalDays={totalDays} locale={getDateLocale(i18n.language)} onSelect={setSelectedId} selectedId={selectedId} />
      )}
      {/* MOBILE, vue Gantt : le tableau fait 830 px de colonnes → on affiche une
          LISTE de tâches (hiérarchie, dates, avancement, mini-planning) ; les
          autres vues (tableau/calendrier/charge/réseau) restent telles quelles. */}
      {isMobileView && activeTab === 'gantt' ? (
        <MobileTaskList
          rows={visibleTasks}
          progressMap={effProgress}
          projectStart={projectStart}
          totalDays={totalDays}
          selectedId={selectedId}
          collapsed={collapsed}
          dateFmt={d => format(d, 'd MMM', { locale: getDateLocale(i18n.language) })}
          dates={tk => ({ start: schedStart(tk, projectStart), end: schedEnd(tk, projectStart) })}
          canEdit={!readMobile}
          onSelect={id => { setSelectedId(id); setMobilePanel(readMobile ? null : 'inspector'); if (readMobile) setSummaryId(id) }}
          onToggle={tid => setCollapsed(s2 => { const n = new Set(s2); n.has(tid) ? n.delete(tid) : n.add(tid); return n })}
          onLongPress={(tid, x, y) => { setSelectedId(tid); setCtxMenu({ x, y, taskId: tid }) }}
          onAdd={() => { void createTaskCmd(undefined) }}
        />
      ) : activeTab === 'charter' ? (
        <CharterView projectId={id!} isOwner={!!project && !!authUser && project.owner_id === authUser.id} canEdit={!readMobile} />
      ) : activeTab === 'wbs' ? (
        <WbsView projectId={id!} onOpenTask={setSelectedId} canEdit={!readMobile} />
      ) : activeTab === 'deliverables' ? (
        <DeliverablesView projectId={id!} isOwner={!!project && !!authUser && project.owner_id === authUser.id} canEdit={!readMobile} />
      ) : activeTab === 'requirements' ? (
        <RequirementsView projectId={id!} canEdit={!readMobile} />
      ) : activeTab === 'traceability' ? (
        <TraceabilityView projectId={id!} onOpenRequirement={() => setActiveTab('requirements')} />
      ) : activeTab === 'risks' ? (
        <RiskRegisterView projectId={id!} canEdit={!readMobile} onOpenIssues={() => setActiveTab('issues')} />
      ) : activeTab === 'issues' ? (
        <IssueLogView projectId={id!} canEdit={!readMobile} onOpenRisks={() => setActiveTab('risks')} />
      ) : activeTab === 'costs' ? (
        <EarnedValueView projectId={id!} canEdit={!readMobile} onOpenEntries={() => setActiveTab('expenses')} />
      ) : activeTab === 'expenses' ? (
        <CostEntriesView projectId={id!} canEdit={!readMobile} />
      ) : activeTab === 'plans' ? (
        <ManagementPlansView projectId={id!} canEdit={!readMobile}
          onOpenArtifact={tab => setActiveTab(tab as typeof activeTab)} />
      ) : activeTab === 'procurement' ? (
        <ProcurementView projectId={id!} canEdit={!readMobile} onOpenTask={setSelectedId}
          onOpenRisks={() => setActiveTab('risks')} onOpenStakeholders={() => setActiveTab('stakeholders')} />
      ) : activeTab === 'closure' ? (
        <ClosureView projectId={id!} canEdit={!readMobile}
          isOwner={!!project && !!authUser && project.owner_id === authUser.id}
          onOpenArtifact={tab => setActiveTab(tab as typeof activeTab)} />
      ) : activeTab === 'changes' ? (
        <ChangeControlView projectId={id!} canEdit={!readMobile}
          isOwner={!!project && !!authUser && project.owner_id === authUser.id}
          onOpenTask={setSelectedId} onOpenBaselines={() => setActiveTab('gantt')} />
      ) : activeTab === 'communications' ? (
        <CommunicationsView projectId={id!} canEdit={!readMobile} onOpenStakeholders={() => setActiveTab('stakeholders')} />
      ) : activeTab === 'decisions' ? (
        <DecisionLogView projectId={id!} canEdit={!readMobile}
          onOpenTask={setSelectedId} onOpenRisks={() => setActiveTab('risks')} />
      ) : activeTab === 'quality' ? (
        <QualityView projectId={id!} canEdit={!readMobile}
          onOpenExpenses={() => setActiveTab('expenses')} onOpenIssues={() => setActiveTab('issues')} />
      ) : activeTab === 'stakeholders' ? (
        <StakeholdersView projectId={id!} canEdit={!readMobile} onOpenRaci={() => setActiveTab('raci')} />
      ) : activeTab === 'raci' ? (
        <RaciMatrixView projectId={id!} canEdit={!readMobile}
          onOpenStakeholders={() => setActiveTab('stakeholders')} onOpenTask={setSelectedId} />
      ) : activeTab === 'board' ? (
        <BoardView
          tasks={displayTasks} resources={resources} assignments={assignments} progressMap={effProgress}
          selectedId={selectedId} onSelect={setSelectedId}
          onSetStatus={(taskId, st) => setStatus(taskId, st)}
          onContextMenu={(e, taskId) => { e.preventDefault(); setSelectedId(taskId); setCtxMenu({ x: e.clientX, y: e.clientY, taskId }) }}
        />
      ) : activeTab === 'calendar' ? (
        <CalendarView
          tasks={displayTasks} projectStart={projectStart} locale={getDateLocale(i18n.language)}
          selectedId={selectedId} onSelect={setSelectedId}
          onContextMenu={(e, taskId) => { e.preventDefault(); setSelectedId(taskId); setCtxMenu({ x: e.clientX, y: e.clientY, taskId }) }}
        />
      ) : activeTab === 'resources' ? (
        <ResourcesView projectId={id!} resources={resources} assignments={assignments} tasks={allTasks} projectStart={projectStart} totalDays={totalDays} locale={getDateLocale(i18n.language)} canEdit={!readMobile} onRefresh={refresh} />
      ) : activeTab === 'load' ? (
        <ResourceLoadView tasks={allTasks} resources={resources} assignments={assignments} projectStart={projectStart} totalDays={totalDays} dayW={dayW} locale={getDateLocale(i18n.language)} />
      ) : activeTab === 'pert' ? (
        <PertView tasks={displayTasks} deps={deps} projectStart={projectStart} locale={getDateLocale(i18n.language)} selectedId={selectedId} progressMap={effProgress}
          onSelect={setSelectedId} onContextMenu={(e, taskId) => { e.preventDefault(); setSelectedId(taskId); setCtxMenu({ x: e.clientX, y: e.clientY, taskId }) }} />
      ) : activeTab === 'roadmap' ? (
        <RoadmapView projectId={id!} tasks={allTasks} onOpenTask={setSelectedId} />
      ) : (

        <div className="flex flex-1 overflow-hidden">
          {/* ── Table des tâches ── */}
          <div ref={cols.tableRef} className="shrink-0 flex flex-col overflow-hidden border-r border-border" style={cols.containerStyle}>
            {/* Header: right-click opens the column menu, each border is a resize grip. */}
            <div
              className="flex items-stretch border-b border-border bg-surface-1 shrink-0 text-[11px] font-medium text-text-secondary"
              style={{ height: HEADER_H }}
              onContextMenu={e => { e.preventDefault(); colMenu.openAt(e.clientX, e.clientY) }}
            >
              {(() => {
                const shown = GANTT_COL_IDS.filter(cid => cols.visible[cid])
                const heads = ganttHeaderCells(t)
                return shown.map((cid, ci) => {
                  const h = heads[cid]
                  return (
                    <div key={cid}
                      className={`relative flex items-center overflow-hidden whitespace-nowrap ${h.cls} ${ci < shown.length - 1 ? 'border-r border-border' : ''}`}
                      style={colStyle(cid)} title={h.title}>
                      <span className="truncate">{h.label}</span>
                      {/* Grip: drag = resize, double-click (`detail >= 2` on the second
                          press, so it works even mid-drag) = default width. */}
                      {isColResizable(cid) && (
                        <span
                          onPointerDown={e => { if (e.detail >= 2) cols.resetColumn(cid); else cols.startResize(cid, e) }}
                          onDoubleClick={() => cols.resetColumn(cid)}
                          title={t('proj_col_resize_hint', { defaultValue: 'Glisser pour redimensionner · double-clic pour la largeur par défaut' })}
                          className="absolute top-0 right-0 z-10 h-full w-[7px] cursor-col-resize touch-none hover:bg-primary/40"
                        />
                      )}
                    </div>
                  )
                })
              })()}
            </div>
            <div className="flex-1 overflow-y-auto" id="task-table-scroll">
              {visibleTasks.map(({ task, depth, hasChildren }) => (
                <TaskRow
                  key={task.id} task={task} index={taskNumber.get(task.id) ?? 0} depth={depth}
                  isSelected={selectedId === task.id} hasChildren={hasChildren} collapsed={collapsed.has(task.id)}
                  onToggle={() => setCollapsed(s => { const n = new Set(s); n.has(task.id) ? n.delete(task.id) : n.add(task.id); return n })}
                  onSelect={() => setSelectedId(task.id === selectedId ? null : task.id)}
                  onUpdate={d => patchTaskCmd(task.id, d)}
                  onContextMenu={e => { e.preventDefault(); setSelectedId(task.id); setCtxMenu({ x: e.clientX, y: e.clientY, taskId: task.id }) }}
                  resources={resources} assignments={assignments} projectStart={projectStart}
                  predecessorText={predecessorText(task.id)} onSetPredecessors={txt => { void setPredecessors(task.id, txt) }}
                  locale={getDateLocale(i18n.language)}
                  baseline={baselineMap?.get(task.id) ?? null}
                  visible={cols.visible}
                  rollupProgress={hasChildren ? (effProgress.get(task.id) ?? task.progress) : undefined}
                  dnd={{
                    isDragging: dragId === task.id,
                    hint: dropHint?.id === task.id ? dropHint.place : null,
                    canNest: !sortBy && !groupBy,
                    disabled: !!sortBy || !!groupBy,
                    onDragStart: e => onRowDragStart(task.id, e),
                    onDragOver: place => onRowDragOver(task.id, place),
                    onDrop: () => onRowDrop(task.id),
                    onDragEnd: onRowDragEnd,
                  }}
                />
              ))}
              <button onClick={() => { void createTaskCmd(undefined) }}
                className="flex items-center gap-1.5 w-full px-4 py-2 text-xs text-text-tertiary hover:bg-surface-1 hover:text-primary border-b border-[#f1f3f4]">
                <Plus size={12} /> {t('proj_add_task')}
              </button>
            </div>
          </div>

          {/* ── Gantt ── */}
          <div ref={ganttRef} className="flex-1 overflow-x-auto overflow-y-hidden" onScroll={e => setScrollLeft((e.target as HTMLDivElement).scrollLeft)}>
            <div ref={ganttInnerRef} style={{ width: ganttW, height: ganttH, position: 'relative' }}>
              {/* Épinglé au viewport : reste visible quand on scrolle horizontalement. */}
              {/* Pointer events + capture: the drag survives leaving the canvas and
                  only ever ends on a real release (leaving no longer commits). */}
              <canvas ref={canvasRef} style={{ position: 'sticky', top: 0, left: 0, touchAction: 'none' }}
                onPointerDown={onGanttDown} onPointerMove={onGanttMove} onPointerUp={onGanttUp}
                onPointerLeave={() => { const c = canvasRef.current; if (c && !barDragRef.current && !linkDragRef.current) c.style.cursor = 'default'; if (!markerDragRef.current) setHoverMarker(null) }} />
            </div>
          </div>
          </div>
      )}
        </div>
      )
      // Zone de DOCKING sur desktop ; sur mobile les panneaux passent en feuilles
      // du bas (`openPanel`) et la liste occupe tout l'écran.
      if (isMobileView) return body
      return (
        <DockArea
          panels={projPanels}
          storageKey="kubuno:office:projectDock"
          defaultArrangement={{ right: [['inspector', 'resources']] }}
          controllerRef={dockRef}
          viewportBg="#ffffff"
          className="flex flex-1 min-w-0 overflow-hidden"
        >
          {body}
        </DockArea>
      )
      })()}

      {/* ── Status bar ── (masquée sur mobile : le bord bas appartient au ruban) */}
      {!isMobileView && <div className="flex items-center gap-4 px-4 py-1.5 border-t border-border bg-surface-1 text-xs text-text-tertiary shrink-0">
        <span>{t('proj_task_count', { count: allTasks.length })}</span><span>·</span>
        <span>{t('proj_on_critical_path', { count: allTasks.filter(tk => tk.is_critical).length })}</span><span>·</span>
        <span>{t('proj_completed_count', { done: allTasks.filter(tk => tk.status === 'completed').length, total: allTasks.length })}</span>
        {targetEndDay != null && (() => {
          const loc = getDateLocale(i18n.language)
          const finishD = format(addDays(projectStart, Math.max(0, projectFinishDay - 1)), 'd MMM yy', { locale: loc })
          const targetD = format(addDays(projectStart, targetEndDay), 'd MMM yy', { locale: loc })
          const over = projectFinishDay - targetEndDay
          return (
            <><span>·</span>
            <span className={over > 0 ? 'text-danger font-medium' : ''}>
              {t('proj_finish_vs_target', { defaultValue: 'Fin prévue {{f}} · cible {{c}}', f: finishD, c: targetD })}
              {over > 0 && ' ' + t('proj_overrun', { defaultValue: '(+{{n}} j)', n: over })}
            </span></>
          )
        })()}
        <div className="flex-1" />
        <span className="capitalize">{t(`proj_zoom_${zoom}`, { defaultValue: zoom })}</span>
      </div>}

      {/* Curseurs distants (présence) */}
      <RemoteCursors awareness={awareness} selfClientId={awareness.clientID} toScreen={c => ({ left: c.x, top: c.y })} />
      </div>

      {/* ── Menu contextuel (composant MenuDropdown de @ui) ── */}
      {ctxMenu && ctxTask && (() => {
        const taskId = ctxMenu.taskId
        const i = allTasks.findIndex(x => x.id === taskId)
        const items: MenuItem[] = [
          { type: 'action', label: t('proj_ctx_insert_above', { defaultValue: 'Insérer au-dessus' }), icon: <Plus size={14} />, onClick: () => { void createTaskCmd({ position: Math.max(0, i) }) } },
          { type: 'action', label: t('proj_ctx_insert_below', { defaultValue: 'Insérer en dessous' }), icon: <Plus size={14} />, onClick: () => { void createTaskCmd({ position: i + 1 }) } },
          { type: 'action', label: t('proj_add_subtask'), icon: <Indent size={14} />, onClick: () => { void createTaskCmd({ parent_id: taskId }) } },
          { type: 'action', label: t('proj_duplicate_task', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => { void duplicateTask(taskId) } },
          { type: 'separator' },
          { type: 'submenu', label: t('proj_grp_hier', { defaultValue: 'Hiérarchie' }), items: [
            { type: 'action', label: t('proj_indent', { defaultValue: 'Abaisser' }), icon: <Indent size={14} />, onClick: () => indentTask(taskId) },
            { type: 'action', label: t('proj_outdent', { defaultValue: 'Élever' }), icon: <Outdent size={14} />, disabled: !ctxTask.parent_id, onClick: () => outdentTask(taskId) },
            { type: 'separator' },
            { type: 'action', label: t('proj_move_up', { defaultValue: 'Monter' }), icon: <ArrowUp size={14} />, onClick: () => moveTask(taskId, 'up') },
            { type: 'action', label: t('proj_move_down', { defaultValue: 'Descendre' }), icon: <ArrowDown size={14} />, onClick: () => moveTask(taskId, 'down') },
          ] },
          { type: 'submenu', label: t('proj_col_type', { defaultValue: 'Type' }), items: [
            { type: 'action', label: t('proj_type_task', { defaultValue: 'Tâche' }), checked: ctxTask.task_type === 'task', onClick: () => patchTaskCmd(taskId, { task_type: 'task' }) },
            { type: 'action', label: t('proj_type_milestone'), checked: ctxTask.task_type === 'milestone', icon: <Milestone size={14} />, onClick: () => patchTaskCmd(taskId, { task_type: 'milestone' }) },
            { type: 'action', label: t('proj_type_summary'), checked: ctxTask.task_type === 'summary', icon: <FolderKanban size={14} />, onClick: () => patchTaskCmd(taskId, { task_type: 'summary' }) },
          ] },
          { type: 'submenu', label: t('proj_col_status', { defaultValue: 'Statut' }), items: [
            ['not_started', 'À faire'], ['in_progress', 'En cours'], ['on_hold', 'En attente'], ['completed', 'Terminé'], ['cancelled', 'Annulé'],
          ].map(([s, l]) => ({ type: 'action' as const, label: t('proj_status_' + s, { defaultValue: l }), checked: ctxTask.status === s, onClick: () => setStatus(taskId, s) })) },
          { type: 'submenu', label: t('proj_col_priority', { defaultValue: 'Priorité' }), items: [
            ['low', 'Basse'], ['medium', 'Moyenne'], ['high', 'Haute'], ['critical', 'Critique'],
          ].map(([p, l]) => ({ type: 'action' as const, label: t('proj_priority_' + p, { defaultValue: l }), checked: ctxTask.priority === p, onClick: () => setPriority(taskId, p) })) },
          // A parent's % is rolled up from its children — no manual setter.
          ...(isParent(taskId) ? [] : [{ type: 'submenu' as const, label: t('proj_grp_progress', { defaultValue: 'Avancement' }), items: [
            [0, '0%'], [25, '25%'], [50, '50%'], [75, '75%'], [100, '100%'],
          ].map(([p, l]) => ({ type: 'action' as const, label: l as string, checked: ctxTask.progress === p, onClick: () => patchTaskCmd(taskId, { progress: p as number }) })) }]),
          { type: 'separator' },
          { type: 'action', label: t('proj_link_to_prev', { defaultValue: 'Lier au précédent' }), icon: <Link2 size={14} />, disabled: i <= 0, onClick: () => { void addDepCmd(allTasks[i - 1].id, taskId) } },
          { type: 'action', label: t('proj_unlink', { defaultValue: 'Délier' }), icon: <Link2Off size={14} />, onClick: () => unlinkTask(taskId) },
          { type: 'action', label: t('proj_info', { defaultValue: 'Informations' }), icon: <Info size={14} />, onClick: () => { setSelectedId(taskId); openPanel('inspector') } },
          { type: 'separator' },
          { type: 'action', label: t('common_delete'), icon: <Trash2 size={14} />, onClick: () => { void deleteTaskCmd(taskId) } },
        ]
        return <MenuDropdown items={items} pos={{ top: ctxMenu.y, left: ctxMenu.x }} onClose={() => setCtxMenu(null)} />
      })()}

      {/* ── Menu contextuel de l'EN-TÊTE du tableau : colonnes affichées ── */}
      {colMenu.pos && (() => {
        const items: MenuItem[] = [
          { type: 'label', text: t('proj_cols_menu_title', { defaultValue: 'Colonnes affichées' }) },
          ...GANTT_COL_IDS.map((cid): MenuItem => ({
            type: 'action',
            label: isColHideable(cid)
              ? ganttColLabel(t, cid)
              : t('proj_col_always_shown', { defaultValue: '{{col}} (toujours affichée)', col: ganttColLabel(t, cid) }),
            checked: cols.visible[cid],
            disabled: !isColHideable(cid),
            onClick: () => cols.toggleColumn(cid),
          })),
          { type: 'separator' },
          { type: 'action', label: t('proj_cols_reset', { defaultValue: 'Réinitialiser les colonnes' }), icon: <RotateCcw size={14} />, disabled: !cols.customised, onClick: () => cols.resetAll() },
        ]
        return <MenuDropdown items={items} pos={colMenu.pos} onClose={colMenu.close} minWidth={210} />
      })()}

      {/* Panneaux en FEUILLE DU BAS (mobile) : même contenu que le docking. */}
      {isMobileView && mobilePanel && (
        <MobilePanelSheet
          title={projPanels[mobilePanel].label}
          height={mobilePanel === 'inspector' ? '70vh' : '60vh'}
          onClose={() => setMobilePanel(null)}
        >
          {projPanels[mobilePanel].render()}
        </MobilePanelSheet>
      )}

      {/* Lecture mobile : fiche résumé de la tâche (consultation seule). */}
      {readMobile && summaryTask && (
        <MobileTaskSummary
          task={summaryTask}
          rollupProgress={isParent(summaryTask.id) ? (effProgress.get(summaryTask.id) ?? summaryTask.progress) : undefined}
          resources={resources}
          assignments={assignments}
          dateFmt={d => format(d, 'd MMM yyyy', { locale: getDateLocale(i18n.language) })}
          dates={tk => ({ start: schedStart(tk, projectStart), end: schedEnd(tk, projectStart) })}
          onClose={() => setSummaryId(null)}
        />
      )}

      {shareOpen && id && (
        <CollaboratorsDialog entityId={id} cacheKey="proj-collab" title={t('proj_share_title', { defaultValue: 'Partager le projet' })}
          onClose={() => setShareOpen(false)}
          api={{
            listCollaborators: projectsApi.listCollaborators, addCollaborator: projectsApi.addCollaborator,
            updateCollaborator: projectsApi.updateCollaborator, removeCollaborator: projectsApi.removeCollaborator,
            searchRecipients: officeApi.searchRecipients,
          }} />
      )}
    </OfficeShell>
  )
}

// ── Bande chronologie ────────────────────────────────────────────────────────


// ── Kanban board (by status) ──────────────────────────────────────────────────

const PRIO_CLR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }
const BOARD_COLS: Array<[string, string, string]> = [
  ['not_started', 'À faire', '#9aa0a6'],
  ['in_progress', 'En cours', '#1a73e8'],
  ['on_hold', 'En attente', '#fbbc04'],
  ['completed', 'Terminé', '#34a853'],
  ['cancelled', 'Annulé', '#d93025'],
]

function BoardView({ tasks, resources, assignments, selectedId, onSelect, onSetStatus, onContextMenu, progressMap }: {
  tasks: ProjectTask[]; resources: ProjectResource[]; assignments: { task_id: string; resource_id: string }[]
  selectedId: string | null; onSelect: (id: string) => void
  onSetStatus: (taskId: string, status: string) => void
  onContextMenu: (e: React.MouseEvent, taskId: string) => void
  progressMap: Map<string, number>
}) {
  const { t } = useTranslation('office')
  const [dragId, setDragId] = useState<string | null>(null)
  const [overCol, setOverCol] = useState<string | null>(null)
  const cards = tasks.filter(tk => tk.task_type !== 'summary')
  const resOf = (taskId: string) => assignments.filter(a => a.task_id === taskId).map(a => resources.find(r => r.id === a.resource_id)).filter(Boolean) as ProjectResource[]
  return (
    <div className="flex-1 overflow-x-auto overflow-y-hidden p-3 flex gap-3 items-start bg-surface-1">
      {BOARD_COLS.map(([st, label, clr]) => {
        const col = cards.filter(tk => tk.status === st)
        return (
          <div key={st}
            onDragOver={e => { e.preventDefault(); setOverCol(st) }}
            onDragLeave={() => setOverCol(c => c === st ? null : c)}
            onDrop={() => { if (dragId) onSetStatus(dragId, st); setDragId(null); setOverCol(null) }}
            className={`flex-shrink-0 w-64 bg-surface-0 rounded-lg border flex flex-col max-h-full ${overCol === st ? 'border-primary ring-1 ring-primary/30' : 'border-border'}`}>
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border" style={{ borderTop: `3px solid ${clr}` }}>
              <span className="text-xs font-semibold text-text-primary">{t('proj_status_' + st, { defaultValue: label })}</span>
              <span className="text-[11px] text-text-tertiary">{col.length}</span>
            </div>
            <div className="flex-1 overflow-y-auto p-2 space-y-2 min-h-[60px]">
              {col.map(tk => (
                <div key={tk.id} draggable
                  onDragStart={() => setDragId(tk.id)} onDragEnd={() => { setDragId(null); setOverCol(null) }}
                  onClick={() => onSelect(tk.id)} onContextMenu={e => onContextMenu(e, tk.id)}
                  className={`rounded-md border bg-surface-0 p-2 cursor-pointer transition-shadow hover:shadow-sm ${selectedId === tk.id ? 'border-primary ring-1 ring-primary/30' : 'border-border'}`}>
                  <div className="flex items-start gap-1.5">
                    {tk.task_type === 'milestone' && <Milestone size={12} className="text-orange-500 mt-0.5 flex-shrink-0" />}
                    <Flag size={11} className="mt-0.5 flex-shrink-0" style={{ color: PRIO_CLR[tk.priority] ?? '#9aa0a6' }} />
                    <span className="text-xs text-text-primary leading-snug flex-1">{tk.name}</span>
                  </div>
                  {(() => { const pct = progressMap.get(tk.id) ?? tk.progress; return pct > 0 && (
                    <div className="h-1 bg-surface-3 rounded-full overflow-hidden mt-1.5"><div className="h-full rounded-full" style={{ width: `${pct}%`, background: PROGRESS_CLR }} /></div>
                  ) })()}
                  {resOf(tk.id).length > 0 && (
                    <div className="flex items-center gap-0.5 mt-1.5">
                      {resOf(tk.id).slice(0, 4).map(r => (
                        <span key={r.id} title={r.name} className="w-5 h-5 rounded-full flex items-center justify-center text-white text-[10px] font-bold ring-1 ring-white" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</span>
                      ))}
                    </div>
                  )}
                </div>
              ))}
              {col.length === 0 && <p className="text-[11px] text-text-tertiary text-center py-3 italic">—</p>}
            </div>
          </div>
        )
      })}
    </div>
  )
}

// ── Calendar (month grid) ─────────────────────────────────────────────────────

function CalendarView({ tasks, projectStart, locale, selectedId, onSelect, onContextMenu }: {
  tasks: ProjectTask[]; projectStart: Date; locale: import('date-fns').Locale
  selectedId: string | null; onSelect: (id: string) => void
  onContextMenu: (e: React.MouseEvent, taskId: string) => void
}) {
  const { t } = useTranslation('office')
  const [monthOffset, setMonthOffset] = useState(0)
  const month = addMonths(startOfMonth(projectStart), monthOffset)
  const gridStart = startOfWeek(month, { weekStartsOn: 1 })
  const days = Array.from({ length: 42 }, (_, i) => addDays(gridStart, i))
  const items = tasks.filter(tk => tk.task_type !== 'summary').map(tk => ({ tk, s: schedStart(tk, projectStart), e: schedEnd(tk, projectStart) }))
  const dow = Array.from({ length: 7 }, (_, i) => format(addDays(gridStart, i), 'EEE', { locale }))
  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-surface-0">
      <div className="flex items-center gap-3 px-4 py-2 border-b border-border">
        <button onClick={() => setMonthOffset(o => o - 1)} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><ChevronRight size={16} className="rotate-180" /></button>
        <span className="text-sm font-semibold text-text-primary capitalize min-w-[140px] text-center">{format(month, 'MMMM yyyy', { locale })}</span>
        <button onClick={() => setMonthOffset(o => o + 1)} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><ChevronRight size={16} /></button>
        <button onClick={() => setMonthOffset(0)} className="text-xs text-primary hover:underline ml-2">{t('proj_today', { defaultValue: "Aujourd'hui" })}</button>
      </div>
      <div className="grid grid-cols-7 border-b border-border text-[11px] font-medium text-text-tertiary">
        {dow.map((d, i) => <div key={i} className="px-2 py-1 text-center capitalize border-r border-border last:border-0">{d}</div>)}
      </div>
      <div className="flex-1 grid grid-cols-7 grid-rows-6 overflow-y-auto">
        {days.map((day, i) => {
          const dayItems = items.filter(it => day >= it.s && day <= it.e)
          const inMonth = isSameMonth(day, month)
          const today = isSameDay(day, new Date())
          return (
            <div key={i} className={`border-r border-b border-border p-1 overflow-hidden min-h-[70px] ${inMonth ? '' : 'bg-surface-1'}`}>
              <div className={`text-[10px] mb-0.5 ${today ? 'bg-primary text-white rounded-full w-4 h-4 flex items-center justify-center' : inMonth ? 'text-text-secondary' : 'text-text-tertiary'}`}>{format(day, 'd')}</div>
              <div className="space-y-0.5">
                {dayItems.slice(0, 3).map(({ tk, s }) => (
                  <button key={tk.id} onClick={() => onSelect(tk.id)} onContextMenu={e => onContextMenu(e, tk.id)}
                    className={`block w-full text-left text-[10px] px-1 py-0.5 rounded truncate text-white ${selectedId === tk.id ? 'ring-1 ring-black/30' : ''}`}
                    style={{ background: tk.is_critical ? CRITICAL_CLR : (tk.task_type === 'milestone' ? MILESTONE_CLR : TASK_COLOR), opacity: isSameDay(day, s) ? 1 : 0.6 }}>
                    {tk.task_type === 'milestone' ? '◆ ' : ''}{tk.name}
                  </button>
                ))}
                {dayItems.length > 3 && <span className="text-[10px] text-text-tertiary">+{dayItems.length - 3}</span>}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ── Charge des ressources (histogramme jour par jour) ──────────────────────────

const LOAD_ROW = 56
function ResourceLoadView({ tasks, resources, assignments, projectStart, totalDays, dayW, locale }: {
  tasks: ProjectTask[]; resources: ProjectResource[]
  assignments: { task_id: string; resource_id: string; units: number }[]
  projectStart: Date; totalDays: number; dayW: number; locale: import('date-fns').Locale
}) {
  const { t } = useTranslation('office')
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const TOP = 24

  const alloc = useMemo(() => {
    const byId = new Map(tasks.map(tk => [tk.id, tk]))
    const m = new Map<string, number[]>(resources.map(r => [r.id, new Array(totalDays).fill(0)]))
    for (const a of assignments) {
      const tk = byId.get(a.task_id); const arr = m.get(a.resource_id)
      if (!tk || !arr || tk.task_type === 'summary') continue
      // Calendar span (EF−ES), aligned with the Gantt bars.
      const s = tk.early_start ?? 0, e = tk.early_finish ?? (s + tk.duration_days)
      for (let d = Math.max(0, s); d < Math.min(totalDays, e); d++) arr[d] += a.units
    }
    return m
  }, [tasks, resources, assignments, totalDays])

  useEffect(() => {
    const c = canvasRef.current; if (!c) return
    const dpr = window.devicePixelRatio || 1
    const W = totalDays * dayW, H = TOP + resources.length * LOAD_ROW
    c.width = W * dpr; c.height = H * dpr; c.style.width = `${W}px`; c.style.height = `${H}px`
    const ctx = c.getContext('2d')!; ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, W, H)
    // weekend shading + grid
    for (let d = 0; d <= totalDays; d++) {
      const x = d * dayW
      const dow = addDays(projectStart, d).getDay()
      if (dow === 0 || dow === 6) { ctx.fillStyle = '#f8f9fa'; ctx.fillRect(x, TOP, dayW, H - TOP) }
      if (dow === 1) { ctx.strokeStyle = GRID_CLR; ctx.lineWidth = 0.5; ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, H); ctx.stroke() }
    }
    // month labels
    let cur = -1
    for (let d = 0; d <= totalDays; d++) {
      const date = addDays(projectStart, d)
      if (date.getMonth() !== cur) { cur = date.getMonth(); ctx.fillStyle = '#5f6368'; ctx.font = 'bold 10px Outfit, sans-serif'; ctx.textAlign = 'left'; ctx.fillText(format(date, 'MMM yy', { locale }), d * dayW + 3, 14) }
    }
    // per-resource histogram
    resources.forEach((r, i) => {
      const arr = alloc.get(r.id) ?? []
      const cap = r.capacity || 1
      const peak = Math.max(cap, ...arr)
      const base = TOP + i * LOAD_ROW + LOAD_ROW - 8
      const maxH = LOAD_ROW - 18
      // separator
      ctx.strokeStyle = '#e8eaed'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(0, TOP + i * LOAD_ROW); ctx.lineTo(W, TOP + i * LOAD_ROW); ctx.stroke()
      // capacity line
      const capY = base - (cap / peak) * maxH
      ctx.strokeStyle = '#9aa0a6'; ctx.setLineDash([4, 3]); ctx.beginPath(); ctx.moveTo(0, capY); ctx.lineTo(W, capY); ctx.stroke(); ctx.setLineDash([])
      // bars
      for (let d = 0; d < totalDays; d++) {
        const load = arr[d]; if (load <= 0) continue
        const h = (load / peak) * maxH
        ctx.fillStyle = load > cap ? '#d93025cc' : '#1a73e8aa'
        ctx.fillRect(d * dayW + 0.5, base - h, Math.max(1, dayW - 1), h)
      }
    })
  }, [alloc, resources, projectStart, totalDays, dayW, locale])

  if (resources.length === 0) return <div className="flex-1 flex items-center justify-center text-sm text-text-tertiary italic">{t('proj_no_resources_hint')}</div>

  return (
    <div className="flex-1 flex overflow-hidden">
      <div className="shrink-0 w-44 border-r border-border bg-surface-1">
        <div style={{ height: 24 }} className="border-b border-border" />
        {resources.map(r => (
          <div key={r.id} className="flex items-center gap-2 px-2 border-b border-[#f1f3f4]" style={{ height: LOAD_ROW }}>
            <div className="w-6 h-6 rounded-full flex items-center justify-center text-white text-[10px] font-bold flex-shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</div>
            <div className="min-w-0"><p className="text-xs font-medium text-text-primary truncate">{r.name}</p><p className="text-[10px] text-text-tertiary">{Math.round(r.capacity * 100)}%</p></div>
          </div>
        ))}
      </div>
      <div className="flex-1 overflow-auto"><canvas ref={canvasRef} className="block" /></div>
    </div>
  )
}

// ── Vue réseau (PERT) ──────────────────────────────────────────────────────────

const PERT_NW = 170, PERT_NH = 64, PERT_COLW = 214, PERT_ROWH = 92, PERT_PAD = 24
function PertView({ tasks, deps, projectStart, locale, selectedId, onSelect, onContextMenu, progressMap }: {
  tasks: ProjectTask[]; deps: TaskDependency[]; projectStart: Date; locale: import('date-fns').Locale
  selectedId: string | null; onSelect: (id: string) => void; onContextMenu: (e: React.MouseEvent, id: string) => void
  progressMap: Map<string, number>
}) {
  const { t } = useTranslation('office')
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const layout = useMemo(() => {
    const nodes = tasks.filter(tk => tk.task_type !== 'summary')
    const ids = new Set(nodes.map(n => n.id))
    const edges = deps.filter(d => ids.has(d.from_task_id) && ids.has(d.to_task_id))
    const level = new Map(nodes.map(n => [n.id, 0]))
    let changed = true, iter = 0
    while (changed && iter++ < nodes.length + 2) { changed = false; for (const e of edges) { const nl = (level.get(e.from_task_id) ?? 0) + 1; if (nl > (level.get(e.to_task_id) ?? 0)) { level.set(e.to_task_id, nl); changed = true } } }
    const byLevel = new Map<number, ProjectTask[]>()
    for (const n of nodes) { const l = level.get(n.id) ?? 0; if (!byLevel.has(l)) byLevel.set(l, []); byLevel.get(l)!.push(n) }
    const pos = new Map<string, { x: number; y: number }>()
    let maxRows = 0, maxLevel = 0
    for (const [l, arr] of byLevel) { maxLevel = Math.max(maxLevel, l); arr.forEach((n, r) => pos.set(n.id, { x: PERT_PAD + l * PERT_COLW, y: PERT_PAD + r * PERT_ROWH })); maxRows = Math.max(maxRows, arr.length) }
    return { nodes, edges, pos, W: PERT_PAD * 2 + (maxLevel + 1) * PERT_COLW, H: Math.max(200, PERT_PAD * 2 + maxRows * PERT_ROWH) }
  }, [tasks, deps])

  useEffect(() => {
    const c = canvasRef.current; if (!c) return
    const dpr = window.devicePixelRatio || 1
    const { W, H, nodes, edges, pos } = layout
    c.width = W * dpr; c.height = H * dpr; c.style.width = `${W}px`; c.style.height = `${H}px`
    const ctx = c.getContext('2d')!; ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#fafafa'; ctx.fillRect(0, 0, W, H)
    const byId = new Map(nodes.map(n => [n.id, n]))
    // edges
    for (const e of edges) {
      const a = pos.get(e.from_task_id), b = pos.get(e.to_task_id); if (!a || !b) continue
      const x1 = a.x + PERT_NW, y1 = a.y + PERT_NH / 2, x2 = b.x, y2 = b.y + PERT_NH / 2
      const crit = byId.get(e.from_task_id)?.is_critical && byId.get(e.to_task_id)?.is_critical
      ctx.strokeStyle = crit ? CRITICAL_CLR : '#9aa0a6'; ctx.lineWidth = crit ? 2 : 1.25
      const mx = (x1 + x2) / 2
      ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(mx, y1); ctx.lineTo(mx, y2); ctx.lineTo(x2, y2); ctx.stroke()
      ctx.fillStyle = crit ? CRITICAL_CLR : '#9aa0a6'
      ctx.beginPath(); ctx.moveTo(x2, y2); ctx.lineTo(x2 - 6, y2 - 3.5); ctx.lineTo(x2 - 6, y2 + 3.5); ctx.closePath(); ctx.fill()
    }
    // nodes
    for (const n of nodes) {
      const p = pos.get(n.id)!; const sel = n.id === selectedId
      const clr = n.task_type === 'milestone' ? MILESTONE_CLR : n.is_critical ? CRITICAL_CLR : TASK_COLOR
      ctx.fillStyle = '#fff'; ctx.strokeStyle = sel ? '#1a73e8' : clr; ctx.lineWidth = sel ? 2.5 : 1.5
      ctx.beginPath(); ctx.roundRect(p.x, p.y, PERT_NW, PERT_NH, 6); ctx.fill(); ctx.stroke()
      ctx.fillStyle = clr; ctx.fillRect(p.x + 3, p.y + 1.5, PERT_NW - 6, 4)
      ctx.fillStyle = '#202124'; ctx.font = 'bold 11px Outfit, sans-serif'; ctx.textAlign = 'left'
      const name = n.name.length > 24 ? n.name.slice(0, 23) + '…' : n.name
      ctx.fillText(name, p.x + 8, p.y + 20)
      ctx.fillStyle = '#5f6368'; ctx.font = '10px Outfit, sans-serif'
      ctx.fillText(`${format(schedStart(n, projectStart), 'd MMM', { locale })} → ${format(schedEnd(n, projectStart), 'd MMM', { locale })}`, p.x + 8, p.y + 38)
      ctx.fillText(`${n.duration_days}j · ${progressMap.get(n.id) ?? n.progress}%`, p.x + 8, p.y + 53)
    }
  }, [layout, selectedId, projectStart, locale, progressMap])

  const nodeAt = (e: React.MouseEvent): string | null => {
    const c = canvasRef.current; if (!c) return null
    const r = c.getBoundingClientRect()
    const x = e.clientX - r.left, y = e.clientY - r.top
    for (const n of layout.nodes) { const p = layout.pos.get(n.id)!; if (x >= p.x && x <= p.x + PERT_NW && y >= p.y && y <= p.y + PERT_NH) return n.id }
    return null
  }

  if (layout.nodes.length === 0) return <div className="flex-1 flex items-center justify-center text-sm text-text-tertiary italic">{t('proj_no_tasks_hint', { defaultValue: 'Aucune tâche à afficher' })}</div>

  return (
    <div className="flex-1 overflow-auto bg-surface-1">
      <canvas ref={canvasRef} className="block cursor-pointer"
        onClick={e => { const id = nodeAt(e); if (id) onSelect(id) }}
        onContextMenu={e => { const id = nodeAt(e); if (id) onContextMenu(e, id) }} />
    </div>
  )
}
