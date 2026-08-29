import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { Dropdown } from '@ui'
import { GanttChartSquare, ChevronRight, ChevronDown, Milestone, FolderKanban, CheckCircle2, Circle, Check, GripVertical, UserRound } from 'lucide-react'
import { schedStart, schedEnd } from './schedule'
import { colStyle, PRIO_COLOR, type GanttColId } from './ganttTableConstants'
import type { ProjectTask, ProjectResource } from '../api'

/** Pill colour for a completion value: green when done, primary while in progress,
 *  muted when not started — echoes the coloured % pill in Instagantt. */
function pctPill(pct: number): string {
  if (pct >= 100) return 'bg-success/15 text-success'
  if (pct > 0)    return 'bg-primary/10 text-primary'
  return 'bg-surface-2 text-text-tertiary'
}

// One row of the task table (name/duration/progress/priority/dates/variance/predecessors/
// resources), with inline editing. Extracted from ProjectEditorPage.
export default function TaskRow({
  task, index, depth, isSelected, hasChildren, collapsed,
  onToggle, onSelect, onUpdate, onContextMenu,
  resources, assignments, projectStart, predecessorText, onSetPredecessors, locale, baseline, visible, dnd, rollupProgress,
}: {
  task:        ProjectTask
  index:       number
  depth:       number
  isSelected:  boolean
  hasChildren: boolean
  collapsed:   boolean
  /** For a parent row: completion rolled up from its direct children. The task's
   *  own stored % is kept in the database but no longer shown — it is meaningless
   *  once the task has sub-tasks. Undefined for leaves. */
  rollupProgress?: number
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
  baseline?:   { es: number; dur: number } | null
  /** Columns the user kept visible (header and rows share the same model). */
  visible:     Record<GanttColId, boolean>
  /** Drag & drop reordering wiring (owned by the page). */
  dnd?: {
    isDragging: boolean
    hint:       'before' | 'after' | 'inside' | null
    canNest:    boolean
    disabled:   boolean
    onDragStart: (e: React.DragEvent) => void
    onDragOver:  (place: 'before' | 'after' | 'inside') => void
    onDrop:      () => void
    onDragEnd:   () => void
  }
}) {
  const { t } = useTranslation('office')
  const [editingName, setEditingName] = useState(false)
  const [nameVal, setNameVal] = useState(task.name)
  const [predVal, setPredVal] = useState(predecessorText)
  useEffect(() => { setPredVal(predecessorText) }, [predecessorText])
  useEffect(() => { setNameVal(task.name) }, [task.name])

  // Who is on this task — shown as avatars so it reads as "assigned to a person".
  const assignedResources = assignments
    .filter(a => a.task_id === task.id)
    .map(a => resources.find(r => r.id === a.resource_id))
    .filter((r): r is ProjectResource => !!r)

  const cell = 'shrink-0 px-1.5 border-r border-[#f1f3f4] h-7 flex items-center overflow-hidden'

  // Where would a dropped row land relative to this one? Top/bottom thirds mean
  // before/after; the middle band nests inside (only for rows that can hold
  // children, and only when the list is in its natural tree order).
  const canNest = !!dnd?.canNest && task.task_type !== 'milestone'
  const onRowDragOver = (e: React.DragEvent) => {
    if (!dnd || dnd.disabled || dnd.isDragging) return
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    const r = e.currentTarget.getBoundingClientRect()
    const rel = (e.clientY - r.top) / r.height
    const place = canNest && rel > 0.33 && rel < 0.67 ? 'inside' : rel < 0.5 ? 'before' : 'after'
    dnd.onDragOver(place)
  }

  return (
    <div
      className={`relative group flex items-stretch border-b border-[#f1f3f4] text-xs select-none
                  ${dnd?.isDragging ? 'opacity-95 ring-1 ring-primary ring-inset bg-primary/5'
                    : isSelected ? 'bg-primary/5'
                    : task.task_type === 'summary' ? 'bg-surface-1 hover:bg-surface-2'
                    : 'hover:bg-surface-1'}
                  ${dnd?.hint === 'inside' ? 'ring-2 ring-primary ring-inset bg-primary/5' : ''}`}
      onClick={onSelect}
      onContextMenu={onContextMenu}
      onDragOver={onRowDragOver}
      onDrop={dnd && !dnd.disabled ? e => { e.preventDefault(); dnd.onDrop() } : undefined}
      onDragEnd={dnd?.onDragEnd}
    >
      {/* Crisp insertion indicator (no translucent drag ghost). */}
      {dnd?.hint === 'before' && <div className="absolute left-0 right-0 -top-px h-0.5 bg-primary z-20 pointer-events-none" />}
      {dnd?.hint === 'after'  && <div className="absolute left-0 right-0 -bottom-px h-0.5 bg-primary z-20 pointer-events-none" />}
      {/* Drag handle: appears on hover, grips the whole row (and its subtree). */}
      {dnd && !dnd.disabled && (
        <div
          draggable
          onDragStart={dnd.onDragStart}
          onClick={e => e.stopPropagation()}
          title={t('proj_reorder_drag', { defaultValue: 'Glisser pour réordonner' })}
          className="absolute left-0 top-0 bottom-0 w-4 flex items-center justify-center cursor-grab active:cursor-grabbing
                     text-text-tertiary opacity-0 group-hover:opacity-100 hover:text-text-primary z-10"
        >
          <GripVertical size={13} />
        </div>
      )}
      {visible.idx && <div className={`${cell} justify-center text-text-tertiary`} style={colStyle('idx')}>{index}</div>}
      {visible.mode && <div className={`${cell} justify-center text-text-tertiary`} style={colStyle('mode')} title={t('proj_mode_auto', { defaultValue: 'Planification automatique' })}>
        <GanttChartSquare size={12} />
      </div>}

      {/* Name (indentation + expand) */}
      <div className={`${cell}`} style={{ ...colStyle('name'), paddingLeft: depth * 12 + 4 }}>
        {hasChildren ? (
          <button onClick={e => { e.stopPropagation(); onToggle() }} className="mr-0.5 text-text-tertiary hover:text-text-primary">
            {collapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
          </button>
        ) : <span className="w-3 shrink-0" />}
        {task.task_type === 'summary' || hasChildren ? <FolderKanban size={11} className="shrink-0 text-text-tertiary mr-1" />
          : task.task_type === 'milestone' ? <Milestone size={11} className="shrink-0 text-orange-500 mr-1" />
          : (() => {
              // Completion toggle (Instagantt-style): click to mark the task done —
              // the ring fills with a green check — click again to reopen it. An empty
              // ring is muted; a started task (0 < %) tints it with the accent; hovering
              // an open task previews the check so the affordance is discoverable.
              const done = task.progress >= 100
              return (
                <button
                  onClick={e => { e.stopPropagation(); onUpdate(done
                    ? { progress: 0, status: 'not_started' } as Partial<ProjectTask>
                    : { progress: 100, status: 'completed' } as Partial<ProjectTask>) }}
                  title={done
                    ? t('proj_mark_undone', { defaultValue: 'Marquer comme non terminé' })
                    : t('proj_mark_done', { defaultValue: 'Marquer comme terminé' })}
                  className="group/chk shrink-0 mr-1 relative w-[15px] h-[15px] flex items-center justify-center rounded-full hover:bg-success/10"
                >
                  {done
                    ? <CheckCircle2 size={14} className="text-success" />
                    : <>
                        <Circle size={14} className={task.progress > 0 ? 'text-primary' : 'text-text-tertiary'} />
                        <Check size={9} className="absolute text-success opacity-0 group-hover/chk:opacity-70" strokeWidth={3} />
                      </>}
                </button>
              )
            })()}
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

      {/* Duration (editable) */}
      {visible.dur && <div className={`${cell} justify-end`} style={colStyle('dur')}>
        {task.task_type === 'summary' || hasChildren ? (
          <span className="text-text-tertiary">{t('proj_days_short', { count: task.duration_days })}</span>
        ) : (
          <input type="number" min={task.task_type === 'milestone' ? 0 : 1}
            className="w-full bg-transparent text-right outline-none focus:bg-surface-0 focus:ring-1 focus:ring-primary rounded"
            value={task.duration_days}
            onClick={e => e.stopPropagation()}
            onChange={e => onUpdate({ duration_days: Math.max(task.task_type === 'milestone' ? 0 : 1, parseInt(e.target.value) || 0) })} />
        )}
      </div>}

      {/* Progress (%), editable — shown as a coloured pill (Instagantt style) */}
      {visible.progress && <div className={`${cell} justify-center`} style={colStyle('progress')}>
        {task.task_type === 'summary' || hasChildren ? (
          // Rolled up from the children — the parent's own stored % is not shown.
          <span title={t('proj_progress_rollup', { defaultValue: 'Avancement cumulé des sous-tâches' })}
            className={`px-1.5 py-0.5 rounded text-[11px] font-medium ${pctPill(rollupProgress ?? task.progress)}`}>{rollupProgress ?? task.progress}%</span>
        ) : (
          <span className={`inline-flex items-center justify-end rounded px-1.5 py-0.5 text-[11px] font-medium min-w-[38px] focus-within:ring-1 focus-within:ring-primary ${pctPill(task.progress)}`}>
            <input type="number" min={0} max={100}
              className="w-[26px] bg-transparent text-right outline-none tabular-nums [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
              value={task.progress}
              onClick={e => e.stopPropagation()}
              onChange={e => onUpdate({ progress: Math.max(0, Math.min(100, parseInt(e.target.value) || 0)) })} />
            <span className="opacity-60 ml-0.5">%</span>
          </span>
        )}
      </div>}

      {/* Priority */}
      {visible.priority && <div className={`${cell}`} style={colStyle('priority')}>
        <span className="w-2 h-2 rounded-full shrink-0 mr-1" style={{ background: PRIO_COLOR[task.priority] ?? '#9aa0a6' }} />
        <div className="flex-1 min-w-0" onClick={e => e.stopPropagation()}>
          <Dropdown variant="ghost" height={26} fontSize={12} value={task.priority} onChange={v => onUpdate({ priority: v } as Partial<ProjectTask>)}
            options={[{ value: 'low', label: t('proj_priority_low', { defaultValue: 'Basse' }) },
                      { value: 'medium', label: t('proj_priority_medium', { defaultValue: 'Moyenne' }) },
                      { value: 'high', label: t('proj_priority_high', { defaultValue: 'Haute' }) },
                      { value: 'critical', label: t('proj_priority_critical', { defaultValue: 'Critique' }) }]} />
        </div>
      </div>}

      {/* Start / End (scheduled dates) */}
      {visible.start && <div className={`${cell} text-text-secondary`} style={colStyle('start')}>{format(schedStart(task, projectStart), 'd MMM yy', { locale })}</div>}
      {visible.end && <div className={`${cell} text-text-secondary`} style={colStyle('end')}>{format(schedEnd(task, projectStart), 'd MMM yy', { locale })}</div>}
      {visible.variance && <div className={cell} style={colStyle('variance')}>
        {baseline ? (() => {
          const slip = (task.early_start ?? 0) - baseline.es
          if (slip === 0) return <span className="text-text-tertiary">0 j</span>
          return <span className={slip > 0 ? 'text-danger font-medium' : 'text-success font-medium'}>{slip > 0 ? '+' : '−'}{Math.abs(slip)} j</span>
        })() : <span className="text-text-tertiary">—</span>}
      </div>}

      {/* Predecessors (editable: row numbers, e.g. "1;2") */}
      {visible.pred && <div className={`${cell}`} style={colStyle('pred')}>
        <input className="w-full bg-transparent outline-none focus:bg-surface-0 focus:ring-1 focus:ring-primary rounded text-text-secondary"
          value={predVal}
          onClick={e => e.stopPropagation()}
          onChange={e => setPredVal(e.target.value)}
          onBlur={() => { if (predVal !== predecessorText) onSetPredecessors(predVal) }}
          onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur() }}
          placeholder="—" />
      </div>}

      {/* Resources */}
      {visible.res && <div className={`${cell}`} style={colStyle('res')}>
        {assignedResources.length > 0 ? (
          <div className="flex items-center -space-x-1.5" title={assignedResources.map(r => r.name).join(', ')}>
            {assignedResources.slice(0, 4).map(r => (
              r.avatar_url
                ? <img key={r.id} src={r.avatar_url} alt="" className="w-5 h-5 rounded-full ring-1 ring-surface-0 object-cover shrink-0" />
                : <span key={r.id} className="w-5 h-5 rounded-full ring-1 ring-surface-0 flex items-center justify-center text-white text-[9px] font-semibold shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</span>
            ))}
            {assignedResources.length > 4 && (
              <span className="w-5 h-5 rounded-full ring-1 ring-surface-0 bg-surface-2 text-text-secondary flex items-center justify-center text-[9px] font-semibold shrink-0">+{assignedResources.length - 4}</span>
            )}
          </div>
        ) : (
          <span title={t('proj_unassigned', { defaultValue: 'Non affecté' })}
            className="w-5 h-5 rounded-full border border-dashed border-border flex items-center justify-center text-text-tertiary shrink-0">
            <UserRound size={11} className="opacity-50" />
          </span>
        )}
      </div>}
    </div>
  )
}
