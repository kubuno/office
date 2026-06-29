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
  FilePlus, CopyPlus,
} from 'lucide-react'
import { Dropdown, Button, Input, Textarea, Checkbox, MenuDropdown, RangeSlider, type MenuItem } from '@ui'
import { DockArea, type DockPanel, type DockController } from '@kubuno/sdk'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { projectsApi, officeApi, type ProjectTask, type TaskDependency, type ProjectResource, type Project } from './api'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_PROJECTS } from './ribbon/officeThemes'
import { SaveButton } from './ribbon/SaveButton'
import { useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'
import { ProjectsStartContent } from './ProjectsStartContent'
import type { RibbonTab } from './ribbon/types'
import { format, addDays, differenceInCalendarDays, startOfMonth, addMonths, startOfWeek, isSameMonth, isSameDay } from 'date-fns'
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
const COL_W = { idx: 34, mode: 34, name: 168, dur: 64, progress: 52, priority: 80, start: 92, end: 92, pred: 96, res: 120 }
const PRIO_COLOR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }
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
    preview?:     { taskId: string; start: number; dur: number } | null,
    linkPreview?: { x1: number; y1: number; x2: number; y2: number } | null,
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
      const ov = preview && preview.taskId === task.id ? preview : null
      const startOffset = ov ? ov.start : (task.early_start ?? 0)
      const dur = ov ? ov.dur : task.duration_days
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

      // pastille de liaison (glisser vers une autre barre = créer une dépendance)
      if (task.task_type !== 'summary') {
        ctx.fillStyle = '#ffffff'; ctx.strokeStyle = color; ctx.lineWidth = 1.5
        ctx.beginPath(); ctx.arc(x + bw + 6, barY + barH / 2, 3.5, 0, Math.PI * 2); ctx.fill(); ctx.stroke()
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

    // ── ligne élastique de création de lien ──
    if (linkPreview) {
      ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 2; ctx.setLineDash([5, 3])
      ctx.beginPath(); ctx.moveTo(linkPreview.x1, linkPreview.y1); ctx.lineTo(linkPreview.x2, linkPreview.y2); ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = '#1a73e8'; ctx.beginPath(); ctx.arc(linkPreview.x2, linkPreview.y2, 3, 0, Math.PI * 2); ctx.fill()
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

      {/* Avancement (%) éditable */}
      <div className={`${cell} justify-end`} style={{ width: COL_W.progress }}>
        {task.task_type === 'summary' ? (
          <span className="text-text-tertiary">{task.progress}%</span>
        ) : (
          <input type="number" min={0} max={100}
            className="w-full bg-transparent text-right outline-none focus:bg-white focus:ring-1 focus:ring-primary rounded"
            value={task.progress}
            onClick={e => e.stopPropagation()}
            onChange={e => onUpdate({ progress: Math.max(0, Math.min(100, parseInt(e.target.value) || 0)) })} />
        )}
      </div>

      {/* Priorité */}
      <div className={`${cell}`} style={{ width: COL_W.priority }}>
        <span className="w-2 h-2 rounded-full shrink-0 mr-1" style={{ background: PRIO_COLOR[task.priority] ?? '#9aa0a6' }} />
        <select value={task.priority} onClick={e => e.stopPropagation()} onChange={e => onUpdate({ priority: e.target.value } as Partial<ProjectTask>)}
          className="flex-1 min-w-0 bg-transparent outline-none text-text-secondary cursor-pointer focus:bg-white rounded">
          <option value="low">{t('proj_priority_low', { defaultValue: 'Basse' })}</option>
          <option value="medium">{t('proj_priority_medium', { defaultValue: 'Moyenne' })}</option>
          <option value="high">{t('proj_priority_high', { defaultValue: 'Haute' })}</option>
          <option value="critical">{t('proj_priority_critical', { defaultValue: 'Critique' })}</option>
        </select>
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
    <div className="h-full w-full bg-white overflow-y-auto">
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
  const dockRef    = useRef<DockController | null>(null)
  const [scrollLeft, setScrollLeft]   = useState(0)
  const [barPreview, setBarPreview]   = useState<{ taskId: string; start: number; dur: number } | null>(null)
  const barDragRef = useRef<{ taskId: string; mode: 'move' | 'resize'; grabDayFloat: number; origStart: number; origDur: number } | null>(null)
  const [linkPreview, setLinkPreview] = useState<{ x1: number; y1: number; x2: number; y2: number } | null>(null)
  const linkDragRef = useRef<{ fromId: string; x1: number; y1: number } | null>(null)
  const [selectedId, setSelectedId]   = useState<string | null>(null)
  const [newResName, setNewResName]   = useState('')
  const [activeTab, setActiveTab]     = useState<'gantt' | 'resources' | 'board' | 'calendar' | 'load' | 'pert'>('gantt')
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
  // Upsert (type/lag) d'une dépendance puis recalcul CPM.
  const setDep = async (fromId: string, toId: string, dep_type: string, lag_days: number) => {
    if (!id) return
    await projectsApi.createDependency(id, { from_task_id: fromId, to_task_id: toId, dep_type, lag_days })
    await projectsApi.computeCpm(id)
    refresh()
  }

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
    renderer.current.render(tasks, deps, projectStart, totalDays, scrollLeft, viewW, getDateLocale(i18n.language), dayW, barPreview, linkPreview)
  }, [tasks, deps, projectStart, totalDays, scrollLeft, ganttH, i18n.language, dayW, activeTab, showTimeline, barPreview, linkPreview])
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
  const ganttBarHit = (clientX: number, clientY: number) => {
    const c = canvasRef.current; if (!c) return null
    const rect = c.getBoundingClientRect()
    const xWorld = (clientX - rect.left) + scrollLeft
    const yRel = clientY - rect.top
    if (yRel < HEADER_H) return null
    const row = Math.floor((yRel - HEADER_H) / ROW_H)
    const task = tasks[row]
    if (!task || task.task_type !== 'task') return null
    const bx = (task.early_start ?? 0) * dayW
    const bw = Math.max(task.duration_days * dayW, 4)
    const end = bx + bw
    // Zone pastille (juste après la barre) → création de lien.
    if (xWorld >= end + 1 && xWorld <= end + 14) return { task, mode: 'link' as const, dayFloat: xWorld / dayW, bx, bw, row }
    if (xWorld < bx - 2 || xWorld > end + 3) return null
    return { task, mode: (xWorld >= end - 6 ? 'resize' : 'move') as 'move' | 'resize', dayFloat: xWorld / dayW, bx, bw, row }
  }
  const onGanttDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return
    const hit = ganttBarHit(e.clientX, e.clientY)
    if (!hit) return
    e.preventDefault()
    setSelectedId(hit.task.id)
    if (hit.mode === 'link') {
      const x1 = hit.bx + hit.bw - scrollLeft + 6
      const y1 = HEADER_H + hit.row * ROW_H + ROW_H / 2
      linkDragRef.current = { fromId: hit.task.id, x1, y1 }
      setLinkPreview({ x1, y1, x2: x1, y2: y1 })
      return
    }
    barDragRef.current = { taskId: hit.task.id, mode: hit.mode, grabDayFloat: hit.dayFloat, origStart: hit.task.early_start ?? 0, origDur: hit.task.duration_days }
    setBarPreview({ taskId: hit.task.id, start: hit.task.early_start ?? 0, dur: hit.task.duration_days })
  }
  const onGanttMove = (e: React.MouseEvent) => {
    const c = canvasRef.current; if (!c) return
    const rect = c.getBoundingClientRect()
    const link = linkDragRef.current
    if (link) { setLinkPreview({ x1: link.x1, y1: link.y1, x2: e.clientX - rect.left, y2: e.clientY - rect.top }); c.style.cursor = 'crosshair'; return }
    const drag = barDragRef.current
    if (drag) {
      const curFloat = ((e.clientX - rect.left) + scrollLeft) / dayW
      const delta = Math.round(curFloat - drag.grabDayFloat)
      if (drag.mode === 'move') setBarPreview({ taskId: drag.taskId, start: Math.max(0, drag.origStart + delta), dur: drag.origDur })
      else setBarPreview({ taskId: drag.taskId, start: drag.origStart, dur: Math.max(1, drag.origDur + delta) })
      return
    }
    const hit = ganttBarHit(e.clientX, e.clientY)
    c.style.cursor = hit ? (hit.mode === 'resize' ? 'ew-resize' : hit.mode === 'link' ? 'crosshair' : 'grab') : 'default'
  }
  const onGanttUp = async (e?: React.MouseEvent) => {
    // Fin de création de lien.
    const link = linkDragRef.current
    if (link) {
      linkDragRef.current = null; setLinkPreview(null)
      const target = e ? taskRowAt(e.clientY) : null
      if (target && target.id !== link.fromId && id && !deps.some(d => d.from_task_id === link.fromId && d.to_task_id === target.id)) {
        try { await projectsApi.createDependency(id, { from_task_id: link.fromId, to_task_id: target.id }); await projectsApi.computeCpm(id) } catch { /* ignore */ }
        refresh()
      }
      return
    }
    const drag = barDragRef.current, prev = barPreview
    barDragRef.current = null
    if (!drag || !prev || !id) { setBarPreview(null); return }
    const changed = drag.mode === 'move' ? prev.start !== drag.origStart : prev.dur !== drag.origDur
    if (!changed) { setBarPreview(null); return }
    try {
      if (drag.mode === 'move') await projectsApi.updateTask(id, drag.taskId, { start_date: format(addDays(projectStart, prev.start), 'yyyy-MM-dd') } as never)
      else await projectsApi.updateTask(id, drag.taskId, { duration_days: prev.dur } as never)
      await projectsApi.computeCpm(id)
    } catch { /* ignore */ }
    setBarPreview(null)
    refresh()
  }

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
  const setStatus   = (taskId: string, status: string) => updateTaskMut.mutate({ taskId, data: { status } as Partial<ProjectTask> })
  const setPriority = (taskId: string, priority: string) => updateTaskMut.mutate({ taskId, data: { priority } as Partial<ProjectTask> })

  // Indent: make a task the child of its nearest preceding sibling.
  const indentTask = (taskId: string) => {
    const idx = allTasks.findIndex(x => x.id === taskId)
    if (idx < 0) return
    const me = allTasks[idx]
    for (let i = idx - 1; i >= 0; i--) {
      if ((allTasks[i].parent_id ?? null) === (me.parent_id ?? null)) {
        updateTaskMut.mutate({ taskId, data: { parent_id: allTasks[i].id } as Partial<ProjectTask> }); return
      }
    }
  }
  // Outdent: reparent to the grandparent.
  const outdentTask = (taskId: string) => {
    const me = allTasks.find(x => x.id === taskId)
    if (!me?.parent_id) return
    const parent = allTasks.find(x => x.id === me.parent_id)
    updateTaskMut.mutate({ taskId, data: { parent_id: (parent?.parent_id ?? null) } as Partial<ProjectTask> })
  }
  // Move a task up/down by swapping its position with the neighbour.
  const moveTask = (taskId: string, dir: 'up' | 'down') => {
    const idx = allTasks.findIndex(x => x.id === taskId)
    const j = dir === 'up' ? idx - 1 : idx + 1
    if (idx < 0 || j < 0 || j >= allTasks.length) return
    const me = allTasks[idx], other = allTasks[j]
    updateTaskMut.mutate({ taskId: me.id, data: { position: other.position } as Partial<ProjectTask> })
    updateTaskMut.mutate({ taskId: other.id, data: { position: me.position } as Partial<ProjectTask> })
  }
  // Duplicate a task (copies the main fields onto a freshly created one).
  const duplicateTask = async (taskId: string) => {
    const src = allTasks.find(x => x.id === taskId); if (!src || !id) return
    const idx = allTasks.findIndex(x => x.id === taskId)
    const nt = await projectsApi.createTask(id, { task_type: src.task_type, position: idx + 1 })
    await projectsApi.updateTask(id, nt.id, {
      name: `${src.name} (${t('common_copy', { defaultValue: 'copie' })})`,
      duration_days: src.duration_days, priority: src.priority, status: src.status,
      description: src.description, progress: src.progress,
    } as never)
    refresh()
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
      return [tk.wbs, tk.name, tk.task_type, tk.status, tk.priority, format(schedStart(tk, projectStart), 'yyyy-MM-dd'), format(schedEnd(tk, projectStart), 'yyyy-MM-dd'), tk.duration_days, tk.progress, predecessorText(tk.id), res].map(esc).join(',')
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
    doc: {
      info: (
        <InfoPanel
          title={project?.title || t('common_untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('proj_page_projects', { defaultValue: 'Projet' })}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('proj_page_projects', { defaultValue: 'Projet' })],
            [t('proj_grp_tasks', { defaultValue: 'Tâches' }), allTasks.length],
            [t('proj_resources', { defaultValue: 'Ressources' }), resources.length],
            ...(project?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(project.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: () => navigate('/office/projects'),
    },
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
        { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete'), disabled: !selectedTask, onClick: () => selId && deleteTaskMut.mutate(selId) },
      ] },
      { id: 'progress', label: t('proj_grp_progress', { defaultValue: 'Avancement' }), items: [
        ...[0, 25, 50, 75, 100].map(p => ({ id: 'p' + p, kind: 'button' as const, icon: <span className="text-[10px] font-bold">{p}</span>, tooltip: p + '%', disabled: !selectedTask, onClick: () => setProgress(p) })),
        { id: 'cpm', kind: 'button', icon: computeCpmMut.isPending ? <Loader2 size={15} className="animate-spin" /> : <ListChecks size={15} />, label: t('proj_respect_links', { defaultValue: 'Replanifier' }), disabled: computeCpmMut.isPending, onClick: () => computeCpmMut.mutate() },
      ] },
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
        { id: 'del2', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete'), disabled: !selectedTask, onClick: () => selId && deleteTaskMut.mutate(selId) },
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
    { id: 'format', label: t('proj_tab_format', { defaultValue: 'Format' }), groups: [
      { id: 'status', label: t('proj_col_status', { defaultValue: 'Statut' }), items: [
        { id: 'st-todo', kind: 'button', icon: <Circle size={15} />, label: t('proj_status_not_started', { defaultValue: 'À faire' }), disabled: !selectedTask, onClick: () => selId && setStatus(selId, 'not_started') },
        { id: 'st-prog', kind: 'button', icon: <Loader2 size={15} />, label: t('proj_status_in_progress', { defaultValue: 'En cours' }), disabled: !selectedTask, onClick: () => selId && setStatus(selId, 'in_progress') },
        { id: 'st-done', kind: 'button', icon: <CheckCircle2 size={15} />, label: t('proj_status_completed', { defaultValue: 'Terminé' }), disabled: !selectedTask, onClick: () => { if (selId) { setStatus(selId, 'completed'); setProgress(100) } } },
      ] },
      { id: 'prio', label: t('proj_col_priority', { defaultValue: 'Priorité' }), items: [
        ...([['low', '#34a853'], ['medium', '#fbbc04'], ['high', '#ea4335'], ['critical', '#b80672']] as Array<[string, string]>).map(([p, c]) => ({ id: 'pr-' + p, kind: 'button' as const, icon: <Flag size={15} style={{ color: c }} />, label: t('proj_priority_' + p, { defaultValue: p }), disabled: !selectedTask, onClick: () => selId && setPriority(selId, p) })),
      ] },
    ] },
    // ── Affichage ──
    { id: 'view', label: t('proj_grp_view', { defaultValue: 'Affichage' }), groups: [
      { id: 'views', label: t('proj_grp_views', { defaultValue: 'Vues' }), items: [
        { id: 'gantt', kind: 'toggle', icon: <BarChart2 size={15} />, label: t('proj_tab_gantt'), active: activeTab === 'gantt', onClick: () => setActiveTab('gantt') },
        { id: 'board', kind: 'toggle', icon: <KanbanSquare size={15} />, label: t('proj_view_board', { defaultValue: 'Tableau' }), active: activeTab === 'board', onClick: () => setActiveTab('board') },
        { id: 'cal', kind: 'toggle', icon: <CalendarDays size={15} />, label: t('proj_view_calendar', { defaultValue: 'Calendrier' }), active: activeTab === 'calendar', onClick: () => setActiveTab('calendar') },
        { id: 'load', kind: 'toggle', icon: <BarChart3 size={15} />, label: t('proj_view_load', { defaultValue: 'Charge' }), active: activeTab === 'load', onClick: () => setActiveTab('load') },
        { id: 'pert', kind: 'toggle', icon: <Network size={15} />, label: t('proj_view_pert', { defaultValue: 'Réseau' }), active: activeTab === 'pert', onClick: () => setActiveTab('pert') },
      ] },
      { id: 'show', label: t('proj_grp_show', { defaultValue: 'Afficher' }), items: [
        { id: 'tl', kind: 'toggle', icon: <CalendarRange size={15} />, label: t('proj_timeline', { defaultValue: 'Chronologie' }), active: showTimeline, onClick: () => setShowTimeline(s => !s) },
        { id: 'filter', kind: 'toggle', icon: <Filter size={15} />, label: t('proj_filters', { defaultValue: 'Filtres' }), active: showFilters || filterActive, onClick: () => setShowFilters(s => !s) },
        { id: 'info', kind: 'button', icon: <Info size={15} />, label: t('proj_info', { defaultValue: 'Informations' }), onClick: () => dockRef.current?.open('inspector') },
        { id: 'res', kind: 'button', icon: <Users size={15} />, label: t('proj_resources'), onClick: () => dockRef.current?.open('resources') },
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
    <div className="h-full w-full bg-white overflow-y-auto">
      {selectedTask ? (<>
        <TaskDetailPanel task={selectedTask} resources={resources} assignments={assignments}
          onUpdate={d => updateTaskMut.mutate({ taskId: selectedTask.id, data: d })}
          onAssign={rid => assignMut.mutate({ taskId: selectedTask.id, rid })}
          onUnassign={rid => unassignMut.mutate({ taskId: selectedTask.id, rid })}
          onClose={() => dockRef.current?.close('inspector')} />
        <div className="p-3 border-t border-border">
          <p className="text-xs font-medium text-text-secondary mb-2">{t('proj_predecessors', { defaultValue: 'Prédécesseurs' })}</p>
          {deps.filter(d => d.to_task_id === selectedTask.id).map(dep => {
            const from = allTasks.find(tk => tk.id === dep.from_task_id)
            return (
              <div key={dep.id} className="flex items-center gap-1.5 mb-1.5">
                <span className="flex-1 min-w-0 truncate text-xs text-text-primary" title={from?.name}>{taskNumber.get(dep.from_task_id)} · {from?.name}</span>
                <select value={dep.dep_type} onChange={e => setDep(dep.from_task_id, selectedTask.id, e.target.value, dep.lag_days)}
                  className="text-[11px] border border-border rounded bg-white outline-none focus:border-primary px-0.5 py-0.5">
                  <option value="FS">FS</option><option value="SS">SS</option><option value="FF">FF</option><option value="SF">SF</option>
                </select>
                <input type="number" value={dep.lag_days} title={t('proj_lag', { defaultValue: 'Décalage (jours)' })}
                  onChange={e => setDep(dep.from_task_id, selectedTask.id, dep.dep_type, parseInt(e.target.value) || 0)}
                  className="w-12 text-[11px] text-right border border-border rounded outline-none focus:border-primary px-1 py-0.5" />
                <button onClick={() => delDepMut.mutate(dep.id)} className="text-text-tertiary hover:text-danger p-0.5 flex-shrink-0"><Trash2 size={13} /></button>
              </div>
            )
          })}
          {deps.filter(d => d.to_task_id === selectedTask.id).length === 0 && (
            <p className="text-xs text-text-tertiary italic">{t('proj_no_predecessors', { defaultValue: 'Aucun prédécesseur' })}</p>
          )}
          {/* Ajout d'un prédécesseur */}
          <select value="" onChange={e => { if (e.target.value) addDepMut.mutate({ from_task_id: e.target.value, to_task_id: selectedTask.id }) }}
            className="mt-2 w-full text-xs border border-border rounded bg-white outline-none focus:border-primary px-1.5 py-1">
            <option value="">{t('proj_add_predecessor', { defaultValue: '+ Ajouter un prédécesseur…' })}</option>
            {allTasks.filter(tk => tk.id !== selectedTask.id && !deps.some(d => d.to_task_id === selectedTask.id && d.from_task_id === tk.id)).map(tk => (
              <option key={tk.id} value={tk.id}>{taskNumber.get(tk.id)} · {tk.name}</option>
            ))}
          </select>
        </div>
      </>) : (
        <div className="p-4 text-xs text-text-tertiary text-center">{t('proj_select_task_hint', { defaultValue: 'Sélectionnez une tâche pour voir ses détails.' })}</div>
      )}
    </div>
  )
  const resourcesPanel = (
    <div className="h-full w-full bg-white overflow-y-auto p-3">
      <div className="flex gap-2 mb-3">
        <div className="flex-1">
          <Input type="text" placeholder={t('proj_resource_name_placeholder')} value={newResName} onChange={e => setNewResName(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter' && newResName.trim()) createResMut.mutate({ name: newResName.trim() }) }} />
        </div>
        <Button onClick={() => newResName.trim() && createResMut.mutate({ name: newResName.trim() })} disabled={!newResName.trim()} loading={createResMut.isPending}>{t('proj_add')}</Button>
      </div>
      <div className="space-y-2">
        {resources.map(r => (
          <div key={r.id} className="flex items-center gap-3 p-2.5 border border-border rounded-xl bg-surface-1">
            <div className="w-8 h-8 rounded-full flex items-center justify-center text-white text-xs font-bold flex-shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</div>
            <div className="flex-1 min-w-0"><p className="text-sm font-medium text-text-primary truncate">{r.name}</p>{r.role && <p className="text-xs text-text-tertiary truncate">{r.role}</p>}</div>
            <span className="text-xs text-text-tertiary">{r.capacity * 100}%</span>
            <button onClick={() => deleteResMut.mutate(r.id)} className="text-text-tertiary hover:text-danger p-1 flex-shrink-0"><Trash2 size={14} /></button>
          </div>
        ))}
        {resources.length === 0 && <p className="text-sm text-text-tertiary text-center py-8 italic">{t('proj_no_resources_hint')}</p>}
      </div>
    </div>
  )
  const projPanels: Record<string, DockPanel> = {
    inspector: { label: t('proj_info', { defaultValue: 'Informations' }), render: () => inspectorPanel },
    resources: { label: t('proj_resources'), render: () => resourcesPanel },
  }

  return (
    <OfficeShell
      ribbon={[fileTab, ...projRibbon]}
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
      saveStatus={updateProjectMut.isPending ? t('proj_saving', { defaultValue: 'Enregistrement…' }) : t('doc_saved')}
      titleActions={<>
        {/* Immediate save: persist the current title (no reliable dirty signal → omit `dirty`). */}
        <SaveButton
          onSave={() => updateProjectMut.mutate({ title: titleDraft || project.title })}
          saving={updateProjectMut.isPending}
          label={t('doc_save', { defaultValue: 'Enregistrer' })}
        />
        <button onClick={() => updateProjectMut.mutate({ is_starred: !project.is_starred })}
          className={`p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 ${project.is_starred ? 'text-warning' : 'text-white/90'}`}
          title={project.is_starred ? t('proj_unstar', { defaultValue: 'Retirer des favoris' }) : t('proj_star', { defaultValue: 'Ajouter aux favoris' })}>
          <Star size={15} className={project.is_starred ? 'fill-warning text-warning' : ''} />
        </button>
      </>}
      topbarActions={
        <div className="flex items-center gap-2">
          {id && <MacrosMenu docType="project" docId={id} buildApi={makeApi} defaultLabel={project.title} />}
          <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
          <button onClick={() => setShareOpen(true)}
            className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-white/15 text-white text-sm font-medium border border-white/25 hover:bg-white/25 transition-colors">
            <Share2 size={15} /> {t('proj_share', { defaultValue: 'Partager' })}</button>
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

      <DockArea
        panels={projPanels}
        storageKey="kubuno:office:projectDock"
        defaultArrangement={{ right: [['inspector', 'resources']] }}
        controllerRef={dockRef}
        viewportBg="#ffffff"
        className="flex flex-1 min-w-0 overflow-hidden"
      >
        <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
      {(showFilters || filterActive || sortBy || groupBy) && (
        <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-surface-1 shrink-0 flex-wrap">
          <Filter size={13} className="text-text-tertiary" />
          <input value={filterText} onChange={e => setFilterText(e.target.value)} placeholder={t('proj_filter_search', { defaultValue: 'Rechercher une tâche…' })}
            className="px-2 py-0.5 text-xs border border-border rounded outline-none focus:border-primary w-40" />
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
      {showTimeline && activeTab === 'gantt' && (
        <TimelineBand tasks={allTasks} projectStart={projectStart} totalDays={totalDays} locale={getDateLocale(i18n.language)} onSelect={setSelectedId} selectedId={selectedId} />
      )}
      {activeTab === 'board' ? (
        <BoardView
          tasks={displayTasks} resources={resources} assignments={assignments}
          selectedId={selectedId} onSelect={setSelectedId}
          onSetStatus={(taskId, st) => { setStatus(taskId, st); if (st === 'completed') updateTaskMut.mutate({ taskId, data: { progress: 100 } }) }}
          onContextMenu={(e, taskId) => { e.preventDefault(); setSelectedId(taskId); setCtxMenu({ x: e.clientX, y: e.clientY, taskId }) }}
        />
      ) : activeTab === 'calendar' ? (
        <CalendarView
          tasks={displayTasks} projectStart={projectStart} locale={getDateLocale(i18n.language)}
          selectedId={selectedId} onSelect={setSelectedId}
          onContextMenu={(e, taskId) => { e.preventDefault(); setSelectedId(taskId); setCtxMenu({ x: e.clientX, y: e.clientY, taskId }) }}
        />
      ) : activeTab === 'load' ? (
        <ResourceLoadView tasks={allTasks} resources={resources} assignments={assignments} projectStart={projectStart} totalDays={totalDays} dayW={dayW} locale={getDateLocale(i18n.language)} />
      ) : activeTab === 'pert' ? (
        <PertView tasks={displayTasks} deps={deps} projectStart={projectStart} locale={getDateLocale(i18n.language)} selectedId={selectedId}
          onSelect={setSelectedId} onContextMenu={(e, taskId) => { e.preventDefault(); setSelectedId(taskId); setCtxMenu({ x: e.clientX, y: e.clientY, taskId }) }} />
      ) : (

        <div className="flex flex-1 overflow-hidden">
          {/* ── Table des tâches ── */}
          <div className="shrink-0 flex flex-col overflow-hidden border-r border-border" style={{ width: TABLE_W }}>
            <div className="flex items-stretch border-b border-border bg-surface-1 shrink-0 text-[11px] font-medium text-text-secondary" style={{ height: HEADER_H }}>
              <div className="flex items-center justify-center border-r border-[#e8eaed]" style={{ width: COL_W.idx }}>#</div>
              <div className="flex items-center justify-center border-r border-[#e8eaed]" style={{ width: COL_W.mode }} title={t('proj_col_mode', { defaultValue: 'Mode' })}><GanttChartSquare size={13} /></div>
              <div className="flex items-center px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.name }}>{t('proj_col_task')}</div>
              <div className="flex items-center justify-end px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.dur }}>{t('proj_col_duration')}</div>
              <div className="flex items-center justify-end px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.progress }} title={t('proj_col_progress', { defaultValue: 'Avancement' })}>%</div>
              <div className="flex items-center px-1.5 border-r border-[#e8eaed]" style={{ width: COL_W.priority }}>{t('proj_col_priority', { defaultValue: 'Priorité' })}</div>
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
              <canvas ref={canvasRef} style={{ position: 'sticky', top: 0, left: 0 }}
                onMouseDown={onGanttDown} onMouseMove={onGanttMove} onMouseUp={onGanttUp}
                onMouseLeave={() => { linkDragRef.current = null; setLinkPreview(null); onGanttUp() }} />
            </div>
          </div>
          </div>
      )}
        </div>
      </DockArea>

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
        const i = allTasks.findIndex(x => x.id === taskId)
        const items: MenuItem[] = [
          { type: 'action', label: t('proj_ctx_insert_above', { defaultValue: 'Insérer au-dessus' }), icon: <Plus size={14} />, onClick: () => createTaskMut.mutate({ position: Math.max(0, i) }) },
          { type: 'action', label: t('proj_ctx_insert_below', { defaultValue: 'Insérer en dessous' }), icon: <Plus size={14} />, onClick: () => createTaskMut.mutate({ position: i + 1 }) },
          { type: 'action', label: t('proj_add_subtask'), icon: <Indent size={14} />, onClick: () => createTaskMut.mutate({ parent_id: taskId }) },
          { type: 'action', label: t('proj_duplicate_task', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => duplicateTask(taskId) },
          { type: 'separator' },
          { type: 'submenu', label: t('proj_grp_hier', { defaultValue: 'Hiérarchie' }), items: [
            { type: 'action', label: t('proj_indent', { defaultValue: 'Abaisser' }), icon: <Indent size={14} />, onClick: () => indentTask(taskId) },
            { type: 'action', label: t('proj_outdent', { defaultValue: 'Élever' }), icon: <Outdent size={14} />, disabled: !ctxTask.parent_id, onClick: () => outdentTask(taskId) },
            { type: 'separator' },
            { type: 'action', label: t('proj_move_up', { defaultValue: 'Monter' }), icon: <ArrowUp size={14} />, onClick: () => moveTask(taskId, 'up') },
            { type: 'action', label: t('proj_move_down', { defaultValue: 'Descendre' }), icon: <ArrowDown size={14} />, onClick: () => moveTask(taskId, 'down') },
          ] },
          { type: 'submenu', label: t('proj_col_type', { defaultValue: 'Type' }), items: [
            { type: 'action', label: t('proj_type_task', { defaultValue: 'Tâche' }), checked: ctxTask.task_type === 'task', onClick: () => updateTaskMut.mutate({ taskId, data: { task_type: 'task' } }) },
            { type: 'action', label: t('proj_type_milestone'), checked: ctxTask.task_type === 'milestone', icon: <Milestone size={14} />, onClick: () => updateTaskMut.mutate({ taskId, data: { task_type: 'milestone' } }) },
            { type: 'action', label: t('proj_type_summary'), checked: ctxTask.task_type === 'summary', icon: <FolderKanban size={14} />, onClick: () => updateTaskMut.mutate({ taskId, data: { task_type: 'summary' } }) },
          ] },
          { type: 'submenu', label: t('proj_col_status', { defaultValue: 'Statut' }), items: [
            ['not_started', 'À faire'], ['in_progress', 'En cours'], ['on_hold', 'En attente'], ['completed', 'Terminé'], ['cancelled', 'Annulé'],
          ].map(([s, l]) => ({ type: 'action' as const, label: t('proj_status_' + s, { defaultValue: l }), checked: ctxTask.status === s, onClick: () => { setStatus(taskId, s); if (s === 'completed') updateTaskMut.mutate({ taskId, data: { progress: 100 } }) } })) },
          { type: 'submenu', label: t('proj_col_priority', { defaultValue: 'Priorité' }), items: [
            ['low', 'Basse'], ['medium', 'Moyenne'], ['high', 'Haute'], ['critical', 'Critique'],
          ].map(([p, l]) => ({ type: 'action' as const, label: t('proj_priority_' + p, { defaultValue: l }), checked: ctxTask.priority === p, onClick: () => setPriority(taskId, p) })) },
          { type: 'submenu', label: t('proj_grp_progress', { defaultValue: 'Avancement' }), items: [
            [0, '0%'], [25, '25%'], [50, '50%'], [75, '75%'], [100, '100%'],
          ].map(([p, l]) => ({ type: 'action' as const, label: l as string, checked: ctxTask.progress === p, onClick: () => updateTaskMut.mutate({ taskId, data: { progress: p as number } }) })) },
          { type: 'separator' },
          { type: 'action', label: t('proj_link_to_prev', { defaultValue: 'Lier au précédent' }), icon: <Link2 size={14} />, disabled: i <= 0, onClick: () => addDepMut.mutate({ from_task_id: allTasks[i - 1].id, to_task_id: taskId }) },
          { type: 'action', label: t('proj_unlink', { defaultValue: 'Délier' }), icon: <Link2Off size={14} />, onClick: () => deps.filter(d => d.to_task_id === taskId).forEach(d => delDepMut.mutate(d.id)) },
          { type: 'action', label: t('proj_info', { defaultValue: 'Informations' }), icon: <Info size={14} />, onClick: () => { setSelectedId(taskId); dockRef.current?.open('inspector') } },
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

// ── Kanban board (by status) ──────────────────────────────────────────────────

const PRIO_CLR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }
const BOARD_COLS: Array<[string, string, string]> = [
  ['not_started', 'À faire', '#9aa0a6'],
  ['in_progress', 'En cours', '#1a73e8'],
  ['on_hold', 'En attente', '#fbbc04'],
  ['completed', 'Terminé', '#34a853'],
  ['cancelled', 'Annulé', '#d93025'],
]

function BoardView({ tasks, resources, assignments, selectedId, onSelect, onSetStatus, onContextMenu }: {
  tasks: ProjectTask[]; resources: ProjectResource[]; assignments: { task_id: string; resource_id: string }[]
  selectedId: string | null; onSelect: (id: string) => void
  onSetStatus: (taskId: string, status: string) => void
  onContextMenu: (e: React.MouseEvent, taskId: string) => void
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
            className={`flex-shrink-0 w-64 bg-white rounded-lg border flex flex-col max-h-full ${overCol === st ? 'border-primary ring-1 ring-primary/30' : 'border-border'}`}>
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border" style={{ borderTop: `3px solid ${clr}` }}>
              <span className="text-xs font-semibold text-text-primary">{t('proj_status_' + st, { defaultValue: label })}</span>
              <span className="text-[11px] text-text-tertiary">{col.length}</span>
            </div>
            <div className="flex-1 overflow-y-auto p-2 space-y-2 min-h-[60px]">
              {col.map(tk => (
                <div key={tk.id} draggable
                  onDragStart={() => setDragId(tk.id)} onDragEnd={() => { setDragId(null); setOverCol(null) }}
                  onClick={() => onSelect(tk.id)} onContextMenu={e => onContextMenu(e, tk.id)}
                  className={`rounded-md border bg-white p-2 cursor-pointer transition-shadow hover:shadow-sm ${selectedId === tk.id ? 'border-primary ring-1 ring-primary/30' : 'border-border'}`}>
                  <div className="flex items-start gap-1.5">
                    {tk.task_type === 'milestone' && <Milestone size={12} className="text-orange-500 mt-0.5 flex-shrink-0" />}
                    <Flag size={11} className="mt-0.5 flex-shrink-0" style={{ color: PRIO_CLR[tk.priority] ?? '#9aa0a6' }} />
                    <span className="text-xs text-text-primary leading-snug flex-1">{tk.name}</span>
                  </div>
                  {tk.progress > 0 && (
                    <div className="h-1 bg-surface-3 rounded-full overflow-hidden mt-1.5"><div className="h-full rounded-full" style={{ width: `${tk.progress}%`, background: PROGRESS_CLR }} /></div>
                  )}
                  {resOf(tk.id).length > 0 && (
                    <div className="flex items-center gap-0.5 mt-1.5">
                      {resOf(tk.id).slice(0, 4).map(r => (
                        <span key={r.id} title={r.name} className="w-5 h-5 rounded-full flex items-center justify-center text-white text-[9px] font-bold ring-1 ring-white" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</span>
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
    <div className="flex-1 flex flex-col overflow-hidden bg-white">
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
                    className={`block w-full text-left text-[9px] px-1 py-0.5 rounded truncate text-white ${selectedId === tk.id ? 'ring-1 ring-black/30' : ''}`}
                    style={{ background: tk.is_critical ? CRITICAL_CLR : (tk.task_type === 'milestone' ? MILESTONE_CLR : TASK_COLOR), opacity: isSameDay(day, s) ? 1 : 0.6 }}>
                    {tk.task_type === 'milestone' ? '◆ ' : ''}{tk.name}
                  </button>
                ))}
                {dayItems.length > 3 && <span className="text-[9px] text-text-tertiary">+{dayItems.length - 3}</span>}
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
      const s = tk.early_start ?? 0, e = s + tk.duration_days
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
      if (date.getMonth() !== cur) { cur = date.getMonth(); ctx.fillStyle = '#5f6368'; ctx.font = 'bold 10px Google Sans, sans-serif'; ctx.textAlign = 'left'; ctx.fillText(format(date, 'MMM yy', { locale }), d * dayW + 3, 14) }
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
function PertView({ tasks, deps, projectStart, locale, selectedId, onSelect, onContextMenu }: {
  tasks: ProjectTask[]; deps: TaskDependency[]; projectStart: Date; locale: import('date-fns').Locale
  selectedId: string | null; onSelect: (id: string) => void; onContextMenu: (e: React.MouseEvent, id: string) => void
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
      ctx.fillStyle = '#202124'; ctx.font = 'bold 11px Google Sans, sans-serif'; ctx.textAlign = 'left'
      const name = n.name.length > 24 ? n.name.slice(0, 23) + '…' : n.name
      ctx.fillText(name, p.x + 8, p.y + 20)
      ctx.fillStyle = '#5f6368'; ctx.font = '10px Google Sans, sans-serif'
      ctx.fillText(`${format(schedStart(n, projectStart), 'd MMM', { locale })} → ${format(schedEnd(n, projectStart), 'd MMM', { locale })}`, p.x + 8, p.y + 38)
      ctx.fillText(`${n.duration_days}j · ${n.progress}%`, p.x + 8, p.y + 53)
    }
  }, [layout, selectedId, projectStart, locale])

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
