import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { Dropdown } from '@ui'
import { GanttChartSquare, ChevronRight, ChevronDown, Milestone, FolderKanban, Flag } from 'lucide-react'
import { schedStart, schedEnd } from './schedule'
import { COL_W, PRIO_COLOR } from './ganttTableConstants'
import type { ProjectTask, ProjectResource } from '../api'

// One row of the task table (name/duration/progress/priority/dates/variance/predecessors/
// resources), with inline editing. Extracted from ProjectEditorPage.
export default function TaskRow({
  task, index, depth, isSelected, hasChildren, collapsed,
  onToggle, onSelect, onUpdate, onContextMenu,
  resources, assignments, projectStart, predecessorText, onSetPredecessors, locale, baseline,
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
  baseline?:   { es: number; dur: number } | null
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
            className="w-full bg-transparent text-right outline-none focus:bg-surface-0 focus:ring-1 focus:ring-primary rounded"
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
            className="w-full bg-transparent text-right outline-none focus:bg-surface-0 focus:ring-1 focus:ring-primary rounded"
            value={task.progress}
            onClick={e => e.stopPropagation()}
            onChange={e => onUpdate({ progress: Math.max(0, Math.min(100, parseInt(e.target.value) || 0)) })} />
        )}
      </div>

      {/* Priorité */}
      <div className={`${cell}`} style={{ width: COL_W.priority }}>
        <span className="w-2 h-2 rounded-full shrink-0 mr-1" style={{ background: PRIO_COLOR[task.priority] ?? '#9aa0a6' }} />
        <div className="flex-1 min-w-0" onClick={e => e.stopPropagation()}>
          <Dropdown variant="ghost" height={26} fontSize={12} value={task.priority} onChange={v => onUpdate({ priority: v } as Partial<ProjectTask>)}
            options={[{ value: 'low', label: t('proj_priority_low', { defaultValue: 'Basse' }) },
                      { value: 'medium', label: t('proj_priority_medium', { defaultValue: 'Moyenne' }) },
                      { value: 'high', label: t('proj_priority_high', { defaultValue: 'Haute' }) },
                      { value: 'critical', label: t('proj_priority_critical', { defaultValue: 'Critique' }) }]} />
        </div>
      </div>

      {/* Début / Fin (dates planifiées) */}
      <div className={`${cell} text-text-secondary`} style={{ width: COL_W.start }}>{format(schedStart(task, projectStart), 'd MMM yy', { locale })}</div>
      <div className={`${cell} text-text-secondary`} style={{ width: COL_W.end }}>{format(schedEnd(task, projectStart), 'd MMM yy', { locale })}</div>
      <div className={cell} style={{ width: COL_W.variance }}>
        {baseline ? (() => {
          const slip = (task.early_start ?? 0) - baseline.es
          if (slip === 0) return <span className="text-text-tertiary">0 j</span>
          return <span className={slip > 0 ? 'text-danger font-medium' : 'text-success font-medium'}>{slip > 0 ? '+' : '−'}{Math.abs(slip)} j</span>
        })() : <span className="text-text-tertiary">—</span>}
      </div>

      {/* Prédécesseurs (éditable : numéros de ligne, ex "1;2") */}
      <div className={`${cell}`} style={{ width: COL_W.pred }}>
        <input className="w-full bg-transparent outline-none focus:bg-surface-0 focus:ring-1 focus:ring-primary rounded text-text-secondary"
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
