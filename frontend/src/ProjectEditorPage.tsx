import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import {
  Plus, Trash2,
  Users, BarChart2, Link2, Link2Off, Milestone, Flag,
  Loader2, AlertTriangle, Star, FolderKanban,
  Indent, ZoomIn, ZoomOut, Info, Share2, GanttChartSquare,
  ChevronRight, ChevronDown, ListChecks, CalendarRange,
} from 'lucide-react'
import { Dropdown, Button, Input, Textarea, Checkbox, MenuDropdown, RangeSlider, type MenuItem } from '@ui'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { projectsApi, officeApi, type ProjectTask, type TaskDependency, type ProjectResource, type Project } from './api'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_PROJECTS } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'
import type { RibbonTab } from './ribbon/types'
import { format, addDays, differenceInCalendarDays } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { userColor, PresenceAvatars, RemoteCursors, usePublishCursor } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import CollaboratorsDialog from './CollaboratorsDialog'
import { MacrosMenu } from './macros/MacrosMenu'

// ── Constants ─────────────────────────────────────────────────────────────────

const ROW_H    = 28
const HEADER_H = 56
const MIN_DAYS = 60
const TIMELINE_H = 60

const TASK_COLOR   = '#1a73e8'
const CRITICAL_CLR = '#d93025'
const MILESTONE_CLR = '#ea4335'
const SUMMARY_CLR  = '#5f6368'
const GRID_CLR     = '#e8eaed'
const PROGRESS_CLR = '#34a853'

// Zoom : pixels par jour selon l'échelle de temps.
type ZoomLevel = 'day' | 'week' | 'month'
const ZOOM_DAYW: Record<ZoomLevel, number> = { day: 26, week: 9, month: 3.2 }

// Colonnes de la table (largeurs fixes, façon MS Project).
const COL_W = { idx: 34, mode: 34, name: 168, dur: 64, start: 92, end: 92, pred: 96, res: 120 }
const TABLE_W = Object.values(COL_W).reduce((a, b) => a + b, 0)

// ── GanttRenderer ─────────────────────────────────────────────────────────────

class GanttRenderer {
  readonly el:    HTMLCanvasElement
  private canvas: HTMLCanvasElement
  private ctx:    CanvasRenderingContext2D
  private dpr:    number

  constructor(canvas: HTMLCanvasElement) {
    this.el = canvas
    this.canvas = canvas
    this.ctx = canvas.getContext('2d')!
    this.dpr = window.devicePixelRatio || 1
  }

  resize(w: number, h: number) {
    this.canvas.width  = w * this.dpr
    this.canvas.height = h * this.dpr
    this.canvas.style.width  = `${w}px`
    this.canvas.style.height = `${h}px`
    this.ctx.setTransform(1, 0, 0, 1, 0, 0)
    this.ctx.scale(this.dpr, this.dpr)
  }

  render(
    tasks:        ProjectTask[],
    deps:         TaskDependency[],
    projectStart: Date,
    totalDays:    number,
    scrollLeft:   number,
    viewportW:    number,
    locale:       import('date-fns').Locale,
    dayW:         number,
  ) {
    const ctx = this.ctx
    const w   = viewportW
    const h   = HEADER_H + tasks.length * ROW_H

    ctx.clearRect(0, 0, w, h)
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(0, 0, w, h)

    const startDay = Math.floor(scrollLeft / dayW)
    const endDay   = Math.min(totalDays, startDay + Math.ceil(viewportW / dayW) + 1)
    const showDays = dayW >= 12   // n'afficher les numéros de jour qu'en échelle « jour »
    const stepW    = dayW         // largeur d'une colonne jour

    // ── weekends + grille ──
    for (let d = startDay; d <= endDay; d++) {
      const x = d * dayW - scrollLeft
      const date = addDays(projectStart, d)
      const dow  = date.getDay()
      if (showDays && (dow === 0 || dow === 6)) {
        ctx.fillStyle = '#f8f9fa'
        ctx.fillRect(x, HEADER_H, stepW, tasks.length * ROW_H)
      }
      // lignes verticales : chaque jour en échelle jour, sinon chaque lundi
      if (showDays || dow === 1) {
        ctx.strokeStyle = GRID_CLR
        ctx.lineWidth   = 0.5
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke()
      }
    }

    // ── fonds de lignes alternés ──
    tasks.forEach((_, i) => {
      if (i % 2 === 1) {
        ctx.fillStyle = '#fafafa'
        ctx.fillRect(0, HEADER_H + i * ROW_H, w, ROW_H)
      }
    })

    // ── header (mois + jours/semaines) ──
    ctx.fillStyle = '#f1f3f4'
    ctx.fillRect(0, 0, w, HEADER_H)
    ctx.strokeStyle = GRID_CLR; ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(0, HEADER_H); ctx.lineTo(w, HEADER_H); ctx.stroke()

    let curMonth = -1
    for (let d = startDay; d <= endDay; d++) {
      const date = addDays(projectStart, d)
      if (date.getMonth() !== curMonth) {
        curMonth = date.getMonth()
        const x = d * dayW - scrollLeft
        ctx.fillStyle = '#202124'
        ctx.font = 'bold 11px Google Sans, sans-serif'
        ctx.textAlign = 'left'
        ctx.fillText(format(date, 'MMM yyyy', { locale }), x + 4, 18)
      }
    }

    if (showDays) {
      for (let d = startDay; d <= endDay; d++) {
        const date = addDays(projectStart, d)
        const x    = d * dayW - scrollLeft + dayW / 2
        const dow  = date.getDay()
        ctx.fillStyle = dow === 0 || dow === 6 ? '#80868b' : '#5f6368'
        ctx.font = '9px Google Sans, sans-serif'
        ctx.textAlign = 'center'
        ctx.fillText(String(date.getDate()), x, 36)
      }
    } else {
      // marqueurs hebdomadaires (lundis)
      for (let d = startDay; d <= endDay; d++) {
        const date = addDays(projectStart, d)
        if (date.getDay() !== 1) continue
        const x = d * dayW - scrollLeft
        ctx.fillStyle = '#5f6368'
        ctx.font = '9px Google Sans, sans-serif'
        ctx.textAlign = 'left'
        ctx.fillText(format(date, 'd', { locale }), x + 2, 36)
      }
    }
    ctx.textAlign = 'left'

    // ── barres de tâches ──
    tasks.forEach((task, i) => {
      const y = HEADER_H + i * ROW_H
      const startOffset = task.early_start ?? 0
      const dur = task.duration_days
      const x   = startOffset * dayW - scrollLeft
      const bw  = Math.max(dur * dayW, 4)
      if (x + bw < 0 || x > viewportW) return

      if (task.task_type === 'milestone') {
        const cx = x, cy = y + ROW_H / 2, s = 7
        ctx.fillStyle = task.is_critical ? CRITICAL_CLR : MILESTONE_CLR
        ctx.beginPath()
        ctx.moveTo(cx, cy - s); ctx.lineTo(cx + s, cy); ctx.lineTo(cx, cy + s); ctx.lineTo(cx - s, cy)
        ctx.closePath(); ctx.fill()
        return
      }

      const color = task.task_type === 'summary' ? SUMMARY_CLR : task.is_critical ? CRITICAL_CLR : TASK_COLOR
      const barH  = ROW_H - 10
      const barY  = y + (ROW_H - barH) / 2

      ctx.fillStyle = color + '33'
      ctx.beginPath(); ctx.roundRect(x, barY, bw, barH, 3); ctx.fill()

      if (task.progress > 0) {
        const pw = bw * (task.progress / 100)
        ctx.fillStyle = task.is_critical ? CRITICAL_CLR + '88' : PROGRESS_CLR + '88'
        ctx.beginPath(); ctx.roundRect(x, barY, pw, barH, 3); ctx.fill()
      }

      ctx.strokeStyle = color; ctx.lineWidth = 1.5
      ctx.beginPath(); ctx.roundRect(x, barY, bw, barH, 3); ctx.stroke()

      // libellé : nom + ressources à droite de la barre
      ctx.fillStyle = '#5f6368'
      ctx.font = '10px Google Sans, sans-serif'
      if (bw > 40) {
        ctx.fillStyle = '#202124'
        ctx.fillText(task.name, x + 6, barY + barH / 2 + 4)
      }
    })

    // ── flèches de dépendance ──
    deps.forEach(dep => {
      const fromIdx = tasks.findIndex(t => t.id === dep.from_task_id)
      const toIdx   = tasks.findIndex(t => t.id === dep.to_task_id)
      if (fromIdx < 0 || toIdx < 0) return
      const fromTask = tasks[fromIdx], toTask = tasks[toIdx]
      const fromX = (fromTask.early_start ?? 0) * dayW + fromTask.duration_days * dayW - scrollLeft
      const fromY = HEADER_H + fromIdx * ROW_H + ROW_H / 2
      const toX   = (toTask.early_start ?? 0) * dayW - scrollLeft
      const toY   = HEADER_H + toIdx * ROW_H + ROW_H / 2

      ctx.strokeStyle = '#9aa0a6'; ctx.lineWidth = 1; ctx.setLineDash([3, 2])
      ctx.beginPath(); ctx.moveTo(fromX, fromY)
      ctx.bezierCurveTo(fromX + 20, fromY, toX - 20, toY, toX, toY); ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = '#9aa0a6'
      ctx.beginPath(); ctx.moveTo(toX, toY); ctx.lineTo(toX - 6, toY - 3); ctx.lineTo(toX - 6, toY + 3)
      ctx.closePath(); ctx.fill()
    })

    // ── ligne « aujourd'hui » ──
    const today = new Date()
    const todayOffset = Math.round((today.getTime() - projectStart.getTime()) / 86400000)
    const todayX = todayOffset * dayW - scrollLeft
    if (todayX >= 0 && todayX <= viewportW) {
      ctx.strokeStyle = '#ea4335'; ctx.lineWidth = 1.5; ctx.setLineDash([4, 3])
      ctx.beginPath(); ctx.moveTo(todayX, HEADER_H); ctx.lineTo(todayX, h); ctx.stroke()
      ctx.setLineDash([])
    }

    // hit-test des barres : renvoyé via une propriété pour le drag (calculé dehors)
  }
}

// ── Dates planifiées (cohérentes avec la barre = offset CPM) ───────────────────

function schedStart(task: ProjectTask, projectStart: Date): Date {
  return addDays(projectStart, task.early_start ?? 0)
}
function schedEnd(task: ProjectTask, projectStart: Date): Date {
  const ef = (task.early_finish ?? ((task.early_start ?? 0) + task.duration_days))
  return addDays(projectStart, Math.max(0, ef - 1))
}

// ── Task Row ──────────────────────────────────────────────────────────────────

function TaskRow({
  task, index, depth, isSelected, hasChildren, collapsed,
  onToggle, onSelect, onUpdate, onContextMenu,
  resources, assignments, projectStart, predecessorText, onSetPredecessors, locale,
}: {
  task:        ProjectTask
  index:       number
  depth:       number
  isSelected:  boolean
  hasChildren: boolean
  collapsed:   boolean
  onToggle:    () => void
  onSelect:    () => void
  onUpdate:    (data: Partial<ProjectTask>) => void
  onContextMenu: (e: React.MouseEvent) => void
  resources:   ProjectResource[]
  assignments: { task_id: string; resource_id: string }[]
  projectStart: Date
  predecessorText: string
  onSetPredecessors: (text: string) => void
  locale:      import('date-fns').Locale
}) {
  const { t } = useTranslation('office')
  const [editingName, setEditingName] = useState(false)
  const [nameVal, setNameVal] = useState(task.name)
  const [predVal, setPredVal] = useState(predecessorText)
  useEffect(() => { setPredVal(predecessorText) }, [predecessorText])
  useEffect(() => { setNameVal(task.name) }, [task.name])

  const assignedNames = assignments
    .filter(a => a.task_id === task.id)
    .map(a => resources.find(r => r.id === a.resource_id)?.name)
    .filter(Boolean).join(', ')

  const cell = 'shrink-0 px-1.5 border-r border-[#f1f3f4] h-7 flex items-center overflow-hidden'

  return (
    <div
      className={`flex items-stretch border-b border-[#f1f3f4] text-xs select-none
                  ${isSelected ? 'bg-primary/5' : 'hover:bg-surface-1'}`}
      onClick={onSelect}
      onContextMenu={onContextMenu}
    >
      <div className={`${cell} justify-center text-text-tertiary`} style={{ width: COL_W.idx }}>{index}</div>
      <div className={`${cell} justify-center text-text-tertiary`} style={{ width: COL_W.mode }} title={t('proj_mode_auto', { defaultValue: 'Planification automatique' })}>
        <GanttChartSquare size={12} />
      </div>

      {/* Nom (avec indentation + expand) */}
      <div className={`${cell}`} style={{ width: COL_W.name, paddingLeft: depth * 12 + 4 }}>
        {hasChildren ? (
          <button onClick={e => { e.stopPropagation(); onToggle() }} className="mr-0.5 text-text-tertiary hover:text-text-primary">
            {collapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
          </button>
        ) : <span className="w-3 shrink-0" />}
        {task.task_type === 'milestone' ? <Milestone size={10} className="shrink-0 text-orange-500 mr-1" />
          : task.task_type === 'summary' ? <FolderKanban size={10} className="shrink-0 text-text-tertiary mr-1" />
          : <Flag size={10} className="shrink-0 text-primary mr-1" />}
        {editingName ? (
          <input autoFocus className="flex-1 min-w-0 bg-transparent border-b border-primary outline-none text-xs"
            value={nameVal} onChange={e => setNameVal(e.target.value)}
            onBlur={() => { setEditingName(false); if (nameVal.trim()) onUpdate({ name: nameVal.trim() }) }}
            onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur(); if (e.key === 'Escape') { setEditingName(false); setNameVal(task.name) } }}
            onClick={e => e.stopPropagation()} />
        ) : (
          <span className={`flex-1 min-w-0 truncate cursor-text ${task.task_type === 'summary' ? 'font-medium' : ''}`}
            onDoubleClick={e => { e.stopPropagation(); setEditingName(true) }}>{task.name}</span>
        )}
      </div>

      {/* Durée (éditable) */}
      <div className={`${cell} justify-end`} style={{ width: COL_W.dur }}>
        {task.task_type === 'summary' ? (
          <span className="text-text-tertiary">{t('proj_days_short', { count: task.duration_days })}</span>
        ) : (
          <input type="number" min={task.task_type === 'milestone' ? 0 : 1}
            className="w-full bg-transparent text-right outline-none focus:bg-white focus:ring-1 focus:ring-primary rounded"
            value={task.duration_days}
            onClick={e => e.stopPropagation()}
            onChange={e => onUpdate({ duration_days: Math.max(task.task_type === 'milestone' ? 0 : 1, parseInt(e.target.value) || 0) })} />
        )}
      </div>

      {/* Début / Fin (dates planifiées) */}
      <div className={`${cell} text-text-secondary`} style={{ width: COL_W.start }}>{format(schedStart(task, projectStart), 'd MMM yy', { locale })}</div>
      <div className={`${cell} text-text-secondary`} style={{ width: COL_W.end }}>{format(schedEnd(task, projectStart), 'd MMM yy', { locale })}</div>

      {/* Prédécesseurs (éditable : numéros de ligne, ex "1;2") */}
      <div className={`${cell}`} style={{ width: COL_W.pred }}>
        <input className="w-full bg-transparent outline-none focus:bg-white focus:ring-1 focus:ring-primary rounded text-text-secondary"
          value={predVal}
          onClick={e => e.stopPropagation()}
          onChange={e => setPredVal(e.target.value)}
          onBlur={() => { if (predVal !== predecessorText) onSetPredecessors(predVal) }}
          onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur() }}
          placeholder="—" />
      </div>

      {/* Ressources */}
      <div className={`${cell} text-text-tertiary`} style={{ width: COL_W.res }}><span className="truncate">{assignedNames}</span></div>
    </div>
  )
}

// ── Task Detail Panel ─────────────────────────────────────────────────────────

function TaskDetailPanel({ task, resources, assignments, onUpdate, onAssign, onUnassign, onClose }: {
  task: ProjectTask
  resources: ProjectResource[]
  assignments: { task_id: string; resource_id: string; units: number }[]
  onUpdate: (data: Partial<ProjectTask>) => void
  onAssign: (resourceId: string) => void
  onUnassign: (resourceId: string) => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const taskAssignments = assignments.filter(a => a.task_id === task.id)
  return (
    <div className="w-72 shrink-0 border-l border-border bg-white overflow-y-auto">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="text-sm font-medium text-text-primary">{t('proj_details')}</span>
        <button onClick={onClose} className="text-text-tertiary hover:text-text-primary">✕</button>
      </div>
      <div className="p-4 space-y-4">
        <div>
          <label className="text-xs text-text-tertiary mb-1 block">{t('proj_name')}</label>
          <Input value={task.name} onChange={e => onUpdate({ name: e.target.value })} />
        </div>
        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_type')}</label>
            <Dropdown className="w-full" value={task.task_type} onChange={v => onUpdate({ task_type: v as ProjectTask['task_type'] })}
              options={[{ value: 'task', label: t('proj_type_task') }, { value: 'milestone', label: t('proj_type_milestone') }, { value: 'summary', label: t('proj_type_summary') }]} />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_priority')}</label>
            <Dropdown className="w-full" value={task.priority} onChange={v => onUpdate({ priority: v as ProjectTask['priority'] })}
              options={[{ value: 'low', label: t('proj_priority_low') }, { value: 'medium', label: t('proj_priority_medium') }, { value: 'high', label: t('proj_priority_high') }, { value: 'critical', label: t('proj_priority_critical') }]} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_status')}</label>
            <Dropdown className="w-full" value={task.status} onChange={v => onUpdate({ status: v as ProjectTask['status'] })}
              options={[{ value: 'not_started', label: t('proj_status_not_started') }, { value: 'in_progress', label: t('proj_status_in_progress') }, { value: 'completed', label: t('proj_status_completed') }, { value: 'on_hold', label: t('proj_status_on_hold') }, { value: 'cancelled', label: t('proj_status_cancelled') }]} />
          </div>
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_duration_label')}</label>
            <Input type="number" min="1"
              value={task.duration_days} onChange={e => onUpdate({ duration_days: parseInt(e.target.value) || 1 })} />
          </div>
        </div>
        <div>
          <label className="text-xs text-text-tertiary mb-1 block">{t('proj_progress_label', { value: task.progress })}</label>
          <RangeSlider min={0} max={100} step={5} className="w-full" value={task.progress} onChange={v => onUpdate({ progress: v })} aria-label={t('proj_progress_label', { value: task.progress })} />
        </div>
        <div>
          <label className="text-xs text-text-tertiary mb-1 block">{t('proj_description')}</label>
          <Textarea rows={3} className="h-auto min-h-0 resize-none text-xs"
            value={task.description} onChange={e => onUpdate({ description: e.target.value })} />
        </div>
        {task.early_start !== null && (
          <div className="bg-surface-1 rounded-lg p-3 space-y-1">
            <p className="text-xs font-medium text-text-primary mb-2">{t('proj_cpm_analysis')}</p>
            <div className="grid grid-cols-2 gap-1 text-xs text-text-secondary">
              <span>ES:</span><span>{t('proj_days_short', { count: task.early_start ?? 0 })}</span>
              <span>EF:</span><span>{t('proj_days_short', { count: task.early_finish ?? 0 })}</span>
              <span>LS:</span><span>{t('proj_days_short', { count: task.late_start ?? 0 })}</span>
              <span>LF:</span><span>{t('proj_days_short', { count: task.late_finish ?? 0 })}</span>
              <span>{t('proj_float')}</span>
              <span className={task.is_critical ? 'text-danger font-medium' : 'text-success'}>
                {t('proj_days_short', { count: task.total_float ?? 0 })} {task.is_critical ? t('proj_critical_warning') : ''}
              </span>
            </div>
          </div>
        )}
        <div>
          <label className="text-xs text-text-tertiary mb-2 block">{t('proj_resources')}</label>
          <div className="space-y-1">
            {resources.map(r => {
              const assigned = taskAssignments.some(a => a.resource_id === r.id)
              return (
                <div key={r.id} className="flex items-center gap-2">
                  <Checkbox checked={assigned} onChange={() => assigned ? onUnassign(r.id) : onAssign(r.id)} />
                  <span className="w-3 h-3 rounded-full shrink-0" style={{ background: r.color }} />
                  <span className="text-xs text-text-primary">{r.name}</span>
                  {r.role && <span className="text-xs text-text-tertiary">· {r.role}</span>}
                </div>
              )
            })}
            {resources.length === 0 && <p className="text-xs text-text-tertiary italic">{t('proj_no_resources_defined')}</p>}
          </div>
        </div>
      </div>
    </div>
  )
}

// ── Main Editor ───────────────────────────────────────────────────────────────

export default function ProjectEditorPage() {
  const { t, i18n } = useTranslation('office')
  const { id }     = useParams<{ id: string }>()
  const navigate   = useNavigate()
  const qc         = useQueryClient()
  const canvasRef  = useRef<HTMLCanvasElement>(null)
  const renderer   = useRef<GanttRenderer | null>(null)
  const ganttRef   = useRef<HTMLDivElement>(null)
  const workRef    = useRef<HTMLDivElement>(null)
  const [scrollLeft, setScrollLeft]   = useState(0)
  const [selectedId, setSelectedId]   = useState<string | null>(null)
  const [newResName, setNewResName]   = useState('')
  const [activeTab, setActiveTab]     = useState<'gantt' | 'resources'>('gantt')
  const [zoom, setZoom]               = useState<ZoomLevel>('day')
  const [collapsed, setCollapsed]     = useState<Set<string>>(new Set())
  const [showTimeline, setShowTimeline] = useState(true)
  const [showProps, setShowProps]     = useState(false)
  const [shareOpen, setShareOpen]     = useState(false)
  const [ctxMenu, setCtxMenu]         = useState<{ x: number; y: number; taskId: string } | null>(null)
  const dayW = ZOOM_DAYW[zoom]

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

  // Tâches visibles (replie les sous-arbres des récapitulatifs repliés).
  const visibleTasks = useMemo(() => {
    const collapsedIds = collapsed
    const out: { task: ProjectTask; depth: number; hasChildren: boolean }[] = []
    const childrenOf = (pid: string | null) => allTasks.filter(t => (t.parent_id ?? null) === pid)
    const walk = (pid: string | null, depth: number) => {
      for (const tk of childrenOf(pid)) {
        const kids = childrenOf(tk.id)
        out.push({ task: tk, depth, hasChildren: kids.length > 0 })
        if (kids.length > 0 && !collapsedIds.has(tk.id)) walk(tk.id, depth + 1)
      }
    }
    walk(null, 0)
    // Repli sur l'ordre d'origine si aucune hiérarchie détectée
    if (out.length === 0 && allTasks.length > 0) return allTasks.map(task => ({ task, depth: 0, hasChildren: false }))
    return out
  }, [allTasks, collapsed])

  const tasks = useMemo(() => visibleTasks.map(v => v.task), [visibleTasks])
  // Numéro de ligne global (1-based) par id, pour les prédécesseurs.
  const taskNumber = useMemo(() => {
    const m = new Map<string, number>()
    allTasks.forEach((tk, i) => m.set(tk.id, i + 1))
    return m
  }, [allTasks])

  const projectStart = useMemo(() => project?.start_date ? new Date(project.start_date) : new Date(), [project?.start_date])
  const totalDays    = Math.max(MIN_DAYS, allTasks.reduce((max, t) => {
    const ef = (t.early_finish ?? 0) || ((t.early_start ?? 0) + t.duration_days)
    return Math.max(max, ef + 5)
  }, MIN_DAYS))

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
  const refresh = useCallback(() => { qc.invalidateQueries({ queryKey: ['project', id] }); bumpRev() }, [qc, id, bumpRev])

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
  const createResMut  = useMutation({ mutationFn: (d: { name: string }) => projectsApi.createResource(id!, d), onSuccess: () => { setNewResName(''); refresh() } })
  const deleteResMut  = useMutation({ mutationFn: (rid: string) => projectsApi.deleteResource(id!, rid), onSuccess: refresh })
  const assignMut     = useMutation({ mutationFn: ({ taskId, rid }: { taskId: string; rid: string }) => projectsApi.assignResource(id!, taskId, { resource_id: rid }), onSuccess: refresh })
  const unassignMut   = useMutation({ mutationFn: ({ taskId, rid }: { taskId: string; rid: string }) => projectsApi.unassignResource(id!, taskId, rid), onSuccess: refresh })
  const addDepMut     = useMutation({ mutationFn: (d: { from_task_id: string; to_task_id: string }) => projectsApi.createDependency(id!, d), onSuccess: refresh })
  const delDepMut     = useMutation({ mutationFn: (depId: string) => projectsApi.deleteDependency(id!, depId), onSuccess: refresh })

  // Prédécesseurs : texte « 1;3 » ↔ dépendances FS.
  const predecessorText = useCallback((taskId: string) => {
    return deps.filter(d => d.to_task_id === taskId)
      .map(d => taskNumber.get(d.from_task_id))
      .filter((n): n is number => !!n).sort((a, b) => a - b).join(';')
  }, [deps, taskNumber])
  const setPredecessors = useCallback((taskId: string, text: string) => {
    const wanted = new Set(text.split(/[;,\s]+/).map(s => parseInt(s.trim())).filter(n => n >= 1)
      .map(n => allTasks[n - 1]?.id).filter((x): x is string => !!x && x !== taskId))
    const current = deps.filter(d => d.to_task_id === taskId)
    for (const dep of current) if (!wanted.has(dep.from_task_id)) delDepMut.mutate(dep.id)
    const have = new Set(current.map(d => d.from_task_id))
    for (const fid of wanted) if (!have.has(fid)) addDepMut.mutate({ from_task_id: fid, to_task_id: taskId })
  }, [deps, allTasks, delDepMut, addDepMut])

  const ganttH = HEADER_H + tasks.length * ROW_H
  const ganttW = totalDays * dayW

  const doRender = useCallback(() => {
    const canvas = canvasRef.current, host = ganttRef.current
    if (!canvas || !host) return
    const viewW = host.clientWidth
    if (viewW <= 0) return
    // (Re)crée le renderer si absent OU si le canvas a été remonté (sinon on
    // dessinerait sur un canvas détaché → diagramme blanc).
    if (!renderer.current || renderer.current.el !== canvas) renderer.current = new GanttRenderer(canvas)
    renderer.current.resize(viewW, ganttH)
    renderer.current.render(tasks, deps, projectStart, totalDays, scrollLeft, viewW, getDateLocale(i18n.language), dayW)
  }, [tasks, deps, projectStart, totalDays, scrollLeft, ganttH, i18n.language, dayW, activeTab, showProps, showTimeline])
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

  const selectedTask = tasks.find(t => t.id === selectedId) ?? null

  // Actions du ruban
  const selIndex = selectedTask ? allTasks.findIndex(t => t.id === selectedTask.id) : -1
  const insertTask = (type?: string) => createTaskMut.mutate({ task_type: type, position: selIndex >= 0 ? selIndex + 1 : undefined })
  const linkSelectedToPrev = () => {
    if (selIndex <= 0) return
    addDepMut.mutate({ from_task_id: allTasks[selIndex - 1].id, to_task_id: allTasks[selIndex].id })
  }
  const unlinkSelected = () => {
    if (!selectedTask) return
    deps.filter(d => d.to_task_id === selectedTask.id).forEach(d => delDepMut.mutate(d.id))
  }
  const setProgress = (p: number) => selectedTask && updateTaskMut.mutate({ taskId: selectedTask.id, data: { progress: p } })

  // MenuDropdown gère le clic extérieur ; on ferme en plus au défilement / Échap.
  useEffect(() => {
    if (!ctxMenu) return
    const close = () => setCtxMenu(null)
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setCtxMenu(null) }
    window.addEventListener('scroll', close, true)
    window.addEventListener('keydown', onKey)
    return () => { window.removeEventListener('scroll', close, true); window.removeEventListener('keydown', onKey) }
  }, [ctxMenu])

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

  const projRibbon: RibbonTab[] = [{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
    fileGroup(t, { onNew: () => createProjMut.mutate(), onDuplicate: () => duplicateProjMut.mutate() }),
    { id: 'tasks', label: t('proj_grp_tasks', { defaultValue: 'Tâches' }), items: [
      { id: 'it', kind: 'button', icon: <Plus size={15} />, label: t('proj_insert_task', { defaultValue: 'Tâche' }), onClick: () => insertTask('task') },
      { id: 'ms', kind: 'button', icon: <Milestone size={15} />, label: t('proj_type_milestone'), onClick: () => insertTask('milestone') },
      { id: 'sum', kind: 'button', icon: <FolderKanban size={15} />, label: t('proj_type_summary'), onClick: () => insertTask('summary') },
      { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete'), disabled: !selectedTask, onClick: () => selectedTask && deleteTaskMut.mutate(selectedTask.id) },
    ] },
    { id: 'links', label: t('proj_grp_links', { defaultValue: 'Liaisons' }), items: [
      { id: 'link', kind: 'button', icon: <Link2 size={15} />, label: t('proj_link', { defaultValue: 'Lier' }), disabled: selIndex <= 0, onClick: linkSelectedToPrev },
      { id: 'unlink', kind: 'button', icon: <Link2Off size={15} />, label: t('proj_unlink', { defaultValue: 'Délier' }), disabled: !selectedTask, onClick: unlinkSelected },
      { id: 'sub', kind: 'button', icon: <Indent size={15} />, label: t('proj_subtask', { defaultValue: 'Abaisser' }), disabled: !selectedTask, onClick: () => selectedTask && createTaskMut.mutate({ parent_id: selectedTask.id }) },
    ] },
    { id: 'progress', label: t('proj_grp_progress', { defaultValue: 'Avancement' }), items: [
      { id: 'p0', kind: 'button', icon: <span className="text-[11px] font-bold">0%</span>, tooltip: '0%', disabled: !selectedTask, onClick: () => setProgress(0) },
      { id: 'p50', kind: 'button', icon: <span className="text-[11px] font-bold">50%</span>, tooltip: '50%', disabled: !selectedTask, onClick: () => setProgress(50) },
      { id: 'p100', kind: 'button', icon: <span className="text-[10px] font-bold">100</span>, tooltip: '100%', disabled: !selectedTask, onClick: () => setProgress(100) },
      { id: 'cpm', kind: 'button', icon: computeCpmMut.isPending ? <Loader2 size={15} className="animate-spin" /> : <ListChecks size={15} />, label: t('proj_respect_links', { defaultValue: 'Replanifier' }), disabled: computeCpmMut.isPending, onClick: () => computeCpmMut.mutate() },
    ] },
    { id: 'view', label: t('proj_grp_view', { defaultValue: 'Affichage' }), items: [
      { id: 'gantt', kind: 'toggle', icon: <BarChart2 size={15} />, label: t('proj_tab_gantt'), active: activeTab === 'gantt', onClick: () => setActiveTab('gantt') },
      { id: 'res', kind: 'toggle', icon: <Users size={15} />, label: t('proj_resources'), active: activeTab === 'resources', onClick: () => setActiveTab('resources') },
      { id: 'tl', kind: 'toggle', icon: <CalendarRange size={15} />, label: t('proj_timeline', { defaultValue: 'Chronologie' }), active: showTimeline, onClick: () => setShowTimeline(s => !s) },
    ] },
    { id: 'zoom', label: t('proj_grp_zoom', { defaultValue: 'Zoom' }), items: [
      { id: 'zout', kind: 'button', icon: <ZoomOut size={15} />, label: t('proj_zoom_out', { defaultValue: 'Arrière' }), disabled: zoom === 'month', onClick: () => setZoom(z => z === 'day' ? 'week' : 'month') },
      { id: 'zin', kind: 'button', icon: <ZoomIn size={15} />, label: t('proj_zoom_in', { defaultValue: 'Avant' }), disabled: zoom === 'day', onClick: () => setZoom(z => z === 'month' ? 'week' : 'day') },
    ] },
    { id: 'props', label: t('proj_grp_props', { defaultValue: 'Propriétés' }), items: [
      { id: 'info', kind: 'toggle', icon: <Info size={15} />, label: t('proj_info', { defaultValue: 'Informations' }), active: showProps, disabled: !selectedTask, onClick: () => { if (selectedTask) setShowProps(s => !s) } },
    ] },
  ] }]

  return (
    <OfficeShell
      ribbon={projRibbon}
      theme={THEME_PROJECTS}
      chromeless
      topbarHeight={64}
      onBack={() => navigate('/office/projects')}
      titleIcon={<div className="w-3 h-3 rounded-full flex-shrink-0 ring-1 ring-white/40" style={{ background: project.color }} />}
      title={titleDraft}
      onTitleChange={setTitleDraft}
      onTitleCommit={() => { if (titleDraft && titleDraft !== project.title) updateProjectMut.mutate({ title: titleDraft }) }}
      titlePlaceholder={t('common_untitled')}
      saveStatus={updateProjectMut.isPending ? t('proj_saving', { defaultValue: 'Enregistrement…' }) : t('doc_saved')}
      titleActions={
        <button onClick={() => updateProjectMut.mutate({ is_starred: !project.is_starred })}
          className={`p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 ${project.is_starred ? 'text-warning' : 'text-white/90'}`}
          title={project.is_starred ? t('proj_unstar', { defaultValue: 'Retirer des favoris' }) : t('proj_star', { defaultValue: 'Ajouter aux favoris' })}>
          <Star size={15} className={project.is_starred ? 'fill-warning text-warning' : ''} />
        </button>
      }
      topbarActions={
        <div className="flex items-center gap-2">
          {id && <MacrosMenu docType="project" docId={id} buildApi={makeApi} defaultLabel={project.title} />}
          <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
          <Button variant="secondary" size="sm" icon={<Share2 size={15} />} onClick={() => setShareOpen(true)}>{t('proj_share', { defaultValue: 'Partager' })}</Button>
        </div>
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

      {/* ── Bande chronologie ── */}
      {showTimeline && activeTab === 'gantt' && (
        <TimelineBand tasks={allTasks} projectStart={projectStart} totalDays={totalDays} locale={getDateLocale(i18n.language)} onSelect={setSelectedId} selectedId={selectedId} />
      )}

      {activeTab === 'resources' ? (
        <div className="flex-1 overflow-y-auto p-6">
          <div className="max-w-lg">
            <h2 className="text-base font-semibold text-text-primary mb-4 flex items-center gap-2"><Users size={16} className="text-primary" /> {t('proj_project_resources')}</h2>
            <div className="flex gap-2 mb-4">
              <div className="flex-1">
                <Input type="text" placeholder={t('proj_resource_name_placeholder')} value={newResName} onChange={e => setNewResName(e.target.value)}
                  onKeyDown={e => { if (e.key === 'Enter' && newResName.trim()) createResMut.mutate({ name: newResName.trim() }) }} />
              </div>
              <Button onClick={() => newResName.trim() && createResMut.mutate({ name: newResName.trim() })} disabled={!newResName.trim()} loading={createResMut.isPending}>{t('proj_add')}</Button>
            </div>
            <div className="space-y-2">
              {resources.map(r => (
                <div key={r.id} className="flex items-center gap-3 p-3 border border-border rounded-xl bg-surface-1">
                  <div className="w-8 h-8 rounded-full flex items-center justify-center text-white text-xs font-bold" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</div>
                  <div className="flex-1"><p className="text-sm font-medium text-text-primary">{r.name}</p>{r.role && <p className="text-xs text-text-tertiary">{r.role}</p>}</div>
                  <span className="text-xs text-text-tertiary">{r.capacity * 100}%</span>
                  <button onClick={() => deleteResMut.mutate(r.id)} className="text-text-tertiary hover:text-danger p-1"><Trash2 size={14} /></button>
                </div>
              ))}
              {resources.length === 0 && <p className="text-sm text-text-tertiary text-center py-8 italic">{t('proj_no_resources_hint')}</p>}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex flex-1 overflow-hidden">
          {/* ── Table des tâches ── */}
          <div className="shrink-0 flex flex-col overflow-hidden border-r border-border" style={{ width: TABLE_W }}>
            <div className="flex items-stretch border-b border-border bg-surface-1 shrink-0 text-[11px] font-medium text-text-secondary" style={{ height: HEADER_H }}>
              <div className="flex items-center justify-center border-r border-[#e8eaed]" style={{ width: COL_W.idx }}>#</div>
              <div className="flex items-center justify-center border-r border-[#e8eaed]" style={{ width: COL_W.mode }} title={t('proj_col_mode', { defaultValue: 'Mode' })}><GanttChartSquare size={13} /></div>
              <div className="flex items-center px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.name }}>{t('proj_col_task')}</div>
              <div className="flex items-center justify-end px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.dur }}>{t('proj_col_duration')}</div>
              <div className="flex items-center px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.start }}>{t('proj_col_start', { defaultValue: 'Début' })}</div>
              <div className="flex items-center px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.end }}>{t('proj_col_end', { defaultValue: 'Fin' })}</div>
              <div className="flex items-center px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.pred }}>{t('proj_col_predecessors', { defaultValue: 'Préd.' })}</div>
              <div className="flex items-center px-1.5" style={{ width: COL_W.res }}>{t('proj_resources')}</div>
            </div>
            <div className="flex-1 overflow-y-auto" id="task-table-scroll">
              {visibleTasks.map(({ task, depth, hasChildren }) => (
                <TaskRow
                  key={task.id} task={task} index={taskNumber.get(task.id) ?? 0} depth={depth}
                  isSelected={selectedId === task.id} hasChildren={hasChildren} collapsed={collapsed.has(task.id)}
                  onToggle={() => setCollapsed(s => { const n = new Set(s); n.has(task.id) ? n.delete(task.id) : n.add(task.id); return n })}
                  onSelect={() => setSelectedId(task.id === selectedId ? null : task.id)}
                  onUpdate={d => updateTaskMut.mutate({ taskId: task.id, data: d })}
                  onContextMenu={e => { e.preventDefault(); setSelectedId(task.id); setCtxMenu({ x: e.clientX, y: e.clientY, taskId: task.id }) }}
                  resources={resources} assignments={assignments} projectStart={projectStart}
                  predecessorText={predecessorText(task.id)} onSetPredecessors={txt => setPredecessors(task.id, txt)}
                  locale={getDateLocale(i18n.language)}
                />
              ))}
              <button onClick={() => createTaskMut.mutate(undefined)}
                className="flex items-center gap-1.5 w-full px-4 py-2 text-xs text-text-tertiary hover:bg-surface-1 hover:text-primary border-b border-[#f1f3f4]">
                <Plus size={12} /> {t('proj_add_task')}
              </button>
            </div>
          </div>

          {/* ── Gantt ── */}
          <div ref={ganttRef} className="flex-1 overflow-x-auto overflow-y-hidden" onScroll={e => setScrollLeft((e.target as HTMLDivElement).scrollLeft)}>
            <div style={{ width: ganttW, height: ganttH, position: 'relative' }}>
              {/* Épinglé au viewport : reste visible quand on scrolle horizontalement. */}
              <canvas ref={canvasRef} style={{ position: 'sticky', top: 0, left: 0 }} />
            </div>
          </div>

          {/* ── Détails ── */}
          {showProps && selectedTask && (
            <TaskDetailPanel task={selectedTask} resources={resources} assignments={assignments}
              onUpdate={d => updateTaskMut.mutate({ taskId: selectedTask.id, data: d })}
              onAssign={rid => assignMut.mutate({ taskId: selectedTask.id, rid })}
              onUnassign={rid => unassignMut.mutate({ taskId: selectedTask.id, rid })}
              onClose={() => setShowProps(false)} />
          )}
        </div>
      )}

      {/* ── Status bar ── */}
      <div className="flex items-center gap-4 px-4 py-1.5 border-t border-border bg-surface-1 text-xs text-text-tertiary shrink-0">
        <span>{t('proj_task_count', { count: allTasks.length })}</span><span>·</span>
        <span>{t('proj_on_critical_path', { count: allTasks.filter(tk => tk.is_critical).length })}</span><span>·</span>
        <span>{t('proj_completed_count', { done: allTasks.filter(tk => tk.status === 'completed').length, total: allTasks.length })}</span>
        <div className="flex-1" />
        <span className="capitalize">{t(`proj_zoom_${zoom}`, { defaultValue: zoom })}</span>
      </div>

      {/* Curseurs distants (présence) */}
      <RemoteCursors awareness={awareness} selfClientId={awareness.clientID} toScreen={c => ({ left: c.x, top: c.y })} />
      </div>

      {/* ── Menu contextuel (composant MenuDropdown de @ui) ── */}
      {ctxMenu && ctxTask && (() => {
        const taskId = ctxMenu.taskId
        const items: MenuItem[] = [
          { type: 'action', label: t('proj_ctx_insert_above', { defaultValue: 'Insérer une tâche au-dessus' }), icon: <Plus size={14} />, onClick: () => { const i = allTasks.findIndex(x => x.id === taskId); createTaskMut.mutate({ position: Math.max(0, i) }) } },
          { type: 'action', label: t('proj_ctx_insert_below', { defaultValue: 'Insérer une tâche en dessous' }), icon: <Plus size={14} />, onClick: () => { const i = allTasks.findIndex(x => x.id === taskId); createTaskMut.mutate({ position: i + 1 }) } },
          { type: 'action', label: t('proj_add_subtask'), icon: <Indent size={14} />, onClick: () => createTaskMut.mutate({ parent_id: taskId }) },
          { type: 'action', label: t('proj_make_milestone', { defaultValue: 'Convertir en jalon' }), icon: <Milestone size={14} />, onClick: () => updateTaskMut.mutate({ taskId, data: { task_type: 'milestone' } }) },
          { type: 'action', label: t('proj_make_summary', { defaultValue: 'Convertir en récapitulatif' }), icon: <FolderKanban size={14} />, onClick: () => updateTaskMut.mutate({ taskId, data: { task_type: 'summary' } }) },
          { type: 'separator' },
          { type: 'action', label: t('proj_unlink', { defaultValue: 'Délier' }), icon: <Link2Off size={14} />, onClick: () => deps.filter(d => d.to_task_id === taskId).forEach(d => delDepMut.mutate(d.id)) },
          { type: 'action', label: t('proj_info', { defaultValue: 'Informations' }), icon: <Info size={14} />, onClick: () => { setSelectedId(taskId); setShowProps(true) } },
          { type: 'separator' },
          { type: 'action', label: t('common_delete'), icon: <Trash2 size={14} />, onClick: () => deleteTaskMut.mutate(taskId) },
        ]
        return <MenuDropdown items={items} pos={{ top: ctxMenu.y, left: ctxMenu.x }} onClose={() => setCtxMenu(null)} />
      })()}

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

function TimelineBand({ tasks, projectStart, totalDays, locale, onSelect, selectedId }: {
  tasks: ProjectTask[]; projectStart: Date; totalDays: number; locale: import('date-fns').Locale
  onSelect: (id: string) => void; selectedId: string | null
}) {
  const { t } = useTranslation('office')
  const end = addDays(projectStart, totalDays)
  // On ne place sur la chronologie que les récapitulatifs + jalons (vue d'ensemble).
  const items = tasks.filter(tk => tk.task_type !== 'task' || (tk.parent_id == null))
  const span = Math.max(1, totalDays)
  return (
    <div className="shrink-0 border-b border-border bg-surface-1 px-4 py-2" style={{ height: TIMELINE_H }}>
      <div className="flex items-center gap-2 mb-1 text-[10px] font-semibold text-text-tertiary uppercase tracking-wide">
        <CalendarRange size={12} /> {t('proj_timeline', { defaultValue: 'Chronologie' })}
      </div>
      <div className="relative h-6 rounded bg-white border border-[#e8eaed]">
        {/* étiquettes début / fin */}
        <span className="absolute left-1 top-1/2 -translate-y-1/2 text-[10px] text-text-tertiary">{format(projectStart, 'd MMM yy', { locale })}</span>
        <span className="absolute right-1 top-1/2 -translate-y-1/2 text-[10px] text-text-tertiary">{format(end, 'd MMM yy', { locale })}</span>
        {items.map(tk => {
          const off = tk.early_start ?? Math.max(0, differenceInCalendarDays(tk.start_date ? new Date(tk.start_date) : projectStart, projectStart))
          const left = `${(off / span) * 100}%`
          const w = `${Math.max(1.5, (tk.duration_days / span) * 100)}%`
          const isMile = tk.task_type === 'milestone'
          return isMile ? (
            <button key={tk.id} onClick={() => onSelect(tk.id)} title={tk.name}
              className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2" style={{ left }}>
              <span className="block w-2.5 h-2.5 rotate-45 bg-orange-500 border border-white" />
            </button>
          ) : (
            <button key={tk.id} onClick={() => onSelect(tk.id)} title={tk.name}
              className={`absolute top-1/2 -translate-y-1/2 h-3 rounded-sm text-[9px] text-white truncate px-1 ${selectedId === tk.id ? 'ring-1 ring-primary' : ''}`}
              style={{ left, width: w, background: tk.is_critical ? CRITICAL_CLR : TASK_COLOR }}>{tk.name}</button>
          )
        })}
      </div>
    </div>
  )
}
