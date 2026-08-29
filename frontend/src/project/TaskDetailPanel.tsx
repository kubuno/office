import { useEffect, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { AlertTriangle } from 'lucide-react'
import { Input, Dropdown, RangeSlider, Textarea, Checkbox } from '@ui'
import type { ProjectTask, ProjectResource, ProjectVersion, CostOverview } from '../api'
import TimeLogSection from './TimeLogSection'

type ConstraintType = ProjectTask['constraint_type']

/** Constraints other than ASAP/ALAP are anchored to a date the server requires. */
const DATED_CONSTRAINTS: ConstraintType[] = ['SNET', 'SNLT', 'FNET', 'FNLT', 'MSO', 'MFO']
const needsDate = (type: ConstraintType) => DATED_CONSTRAINTS.includes(type)

// Task inspector: edit a task's attributes (name, type, priority, status, duration,
// progress, work hours, description), see its CPM analysis, and assign resources.
// Extracted from ProjectEditorPage to keep that file from growing without bound.
export default function TaskDetailPanel({ task, resources, assignments, versions, projectId, showTimeLog = true, showVersions = true, onUpdate, onAssign, onUnassign, onClose, hideHeader, isParent = false, rollupProgress }: {
  task: ProjectTask
  resources: ProjectResource[]
  assignments: { task_id: string; resource_id: string; units: number }[]
  versions?: ProjectVersion[]
  /** When set, the spent-hours field becomes a rolled-up total and a time log is shown. */
  projectId?: string
  /** Artifacts the project switched off must not surface here either. */
  showTimeLog?: boolean
  showVersions?: boolean
  onUpdate: (data: Partial<ProjectTask>) => void
  onAssign: (resourceId: string) => void
  onUnassign: (resourceId: string) => void
  onClose: () => void
  /** En feuille du bas (mobile), l'en-tête de la feuille porte déjà le titre. */
  hideHeader?: boolean
  /** True when the task has children: its duration and completion are rolled up
   *  from them, so the manual editors are replaced by read-only figures (the
   *  stored values stay in the database, just hidden). */
  isParent?: boolean
  /** Completion rolled up from the direct children — shown instead of the manual
   *  % when `isParent`. */
  rollupProgress?: number
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const taskAssignments = assignments.filter(a => a.task_id === task.id)

  // Currency of the project's cost configuration, READ FROM THE CACHE the cost
  // screens fill — never fetched: the inspector must not fire a request just to
  // letter a label. Nothing cached (costs never opened) → the label stays
  // unit-less rather than guessing a symbol.
  const currency = projectId
    ? qc.getQueryData<CostOverview>(['costs', projectId])?.config.currency
    : undefined

  // A dated constraint is rejected by the server until it carries a date, so the
  // type is held locally until the user supplies one; drafts are dropped as soon
  // as the task switches or the server echoes a new constraint back.
  const [draftType, setDraftType] = useState<ConstraintType | null>(null)
  const [draftDate, setDraftDate] = useState<string | null>(null)
  useEffect(() => { setDraftType(null); setDraftDate(null) }, [task.id, task.constraint_type, task.constraint_date])

  const constraintType = draftType ?? task.constraint_type
  const constraintDate = draftDate ?? task.constraint_date ?? ''
  const constraintPending = needsDate(constraintType) && !constraintDate

  const changeConstraintType = (next: ConstraintType) => {
    if (!needsDate(next)) {
      setDraftType(null); setDraftDate(null)
      onUpdate({ constraint_type: next, constraint_date: null })
      return
    }
    if (constraintDate) {
      setDraftType(null)
      onUpdate({ constraint_type: next, constraint_date: constraintDate })
      return
    }
    setDraftType(next) // wait for the date before touching the server
  }

  const changeConstraintDate = (value: string) => {
    setDraftDate(value)
    if (!value) { setDraftType(constraintType); return } // cleared: nothing valid to send yet
    setDraftType(null)
    onUpdate({ constraint_type: constraintType, constraint_date: value })
  }

  const constraintOptions: { value: ConstraintType; label: string }[] = [
    { value: 'ASAP', label: t('proj_constraint_asap', { defaultValue: 'Dès que possible' }) },
    { value: 'ALAP', label: t('proj_constraint_alap', { defaultValue: 'Le plus tard possible' }) },
    { value: 'SNET', label: t('proj_constraint_snet', { defaultValue: 'Ne pas commencer avant le' }) },
    { value: 'SNLT', label: t('proj_constraint_snlt', { defaultValue: 'Commencer au plus tard le' }) },
    { value: 'FNET', label: t('proj_constraint_fnet', { defaultValue: 'Ne pas finir avant le' }) },
    { value: 'FNLT', label: t('proj_constraint_fnlt', { defaultValue: 'Finir au plus tard le' }) },
    { value: 'MSO',  label: t('proj_constraint_mso',  { defaultValue: 'Doit commencer le' }) },
    { value: 'MFO',  label: t('proj_constraint_mfo',  { defaultValue: 'Doit finir le' }) },
  ]

  // Negative total float means a constraint cannot be honoured by the schedule.
  const negativeFloat = task.total_float != null && task.total_float < 0

  return (
    <div className="h-full w-full bg-surface-0 overflow-y-auto">
      {!hideHeader && (
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="text-sm font-medium text-text-primary">{t('proj_details')}</span>
        <button onClick={onClose} className="text-text-tertiary hover:text-text-primary">✕</button>
      </div>
      )}
      <div className="p-4 space-y-4">
        <div>
          <label className="text-xs text-text-tertiary mb-1 block">{t('proj_name')}</label>
          <Input value={task.name} onChange={e => onUpdate({ name: e.target.value })} />
        </div>
        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_type')}</label>
            <Dropdown className="w-full" height={36} fontSize={14} value={task.task_type} onChange={v => onUpdate({ task_type: v as ProjectTask['task_type'] })}
              options={[{ value: 'task', label: t('proj_type_task') }, { value: 'milestone', label: t('proj_type_milestone') }, { value: 'summary', label: t('proj_type_summary') }]} />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_priority')}</label>
            <Dropdown className="w-full" height={36} fontSize={14} value={task.priority} onChange={v => onUpdate({ priority: v as ProjectTask['priority'] })}
              options={[{ value: 'low', label: t('proj_priority_low') }, { value: 'medium', label: t('proj_priority_medium') }, { value: 'high', label: t('proj_priority_high') }, { value: 'critical', label: t('proj_priority_critical') }]} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_status')}</label>
            <Dropdown className="w-full" height={36} fontSize={14} value={task.status} onChange={v => onUpdate({ status: v as ProjectTask['status'] })}
              options={[{ value: 'not_started', label: t('proj_status_not_started') }, { value: 'in_progress', label: t('proj_status_in_progress') }, { value: 'completed', label: t('proj_status_completed') }, { value: 'on_hold', label: t('proj_status_on_hold') }, { value: 'cancelled', label: t('proj_status_cancelled') }]} />
          </div>
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_duration_label')}</label>
            {isParent ? (
              <div className="h-9 flex items-center px-3 rounded-md bg-surface-1 text-sm text-text-secondary" title={t('proj_duration_rollup', { defaultValue: 'Durée déduite des sous-tâches' })}>
                {t('proj_days_short', { count: task.duration_days })}
              </div>
            ) : (
              <Input type="number" min="1"
                value={task.duration_days} onChange={e => onUpdate({ duration_days: parseInt(e.target.value) || 1 })} />
            )}
          </div>
        </div>
        {isParent ? (
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_progress_rollup_label', { defaultValue: 'Avancement cumulé' })}</label>
            <div className="flex items-center gap-2" title={t('proj_progress_rollup', { defaultValue: 'Avancement cumulé des sous-tâches' })}>
              <div className="flex-1 h-2 bg-surface-2 rounded-full overflow-hidden">
                <div className="h-full rounded-full bg-primary" style={{ width: `${rollupProgress ?? task.progress}%` }} />
              </div>
              <span className="text-sm tabular-nums text-text-secondary w-10 text-right">{rollupProgress ?? task.progress}%</span>
            </div>
          </div>
        ) : (
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_progress_label', { value: task.progress })}</label>
            <RangeSlider min={0} max={100} step={5} className="w-full" value={task.progress} onChange={v => onUpdate({ progress: v })} aria-label={t('proj_progress_label', { value: task.progress })} />
          </div>
        )}
        <div className="grid grid-cols-2 gap-2">
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_estimated_hours', { defaultValue: 'Charge estimée (h)' })}</label>
            <Input type="number" min="0" step="0.5" value={task.estimated_hours ?? ''} onChange={e => onUpdate({ estimated_hours: parseFloat(e.target.value) || 0 })} />
          </div>
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_spent_hours', { defaultValue: 'Temps passé (h)' })}</label>
            {projectId ? (
              <div className="h-9 flex items-center px-3 rounded-lg bg-surface-1 border border-border text-sm text-text-secondary tabular-nums">
                {task.spent_hours != null ? t('proj_hours_value', { defaultValue: '{{h}} h', h: task.spent_hours }) : '—'}
              </div>
            ) : (
              <Input type="number" min="0" step="0.5" value={task.spent_hours ?? ''} onChange={e => onUpdate({ spent_hours: parseFloat(e.target.value) || 0 })} />
            )}
          </div>
        </div>
        {/* Budget of the work package — its BAC, the yardstick earned value is
            measured against. Tri-state on purpose: an EMPTY field sends `null`
            (nobody costed this package), never 0 (costed, and costed at zero).
            The two say very different things about the plan, so `|| 0` — used
            for the hour fields above, where zero is a fine default — would be a
            bug here. */}
        <div>
          <label className="text-xs text-text-tertiary mb-1 block">
            {currency
              ? t('proj_budget_cost_cur', { defaultValue: 'Budget ({{cur}})', cur: currency })
              : t('proj_budget_cost', { defaultValue: 'Budget' })}
          </label>
          <Input type="number" min="0" step="0.01" value={task.budget_cost ?? ''}
            placeholder={t('proj_budget_cost_ph', { defaultValue: 'Non chiffré' })}
            onChange={e => {
              const n = parseFloat(e.target.value)
              onUpdate({ budget_cost: Number.isFinite(n) ? n : null })
            }} />
          <p className="text-xs text-text-tertiary mt-1">
            {t('proj_budget_cost_hint', { defaultValue: 'Laissé vide, le lot n’est pas chiffré — ce n’est pas la même chose qu’un budget de zéro.' })}
          </p>
        </div>
        {/* Date constraints steer the scheduler; the deadline only reports lateness. */}
        <div className="space-y-2">
          <p className="text-xs font-medium text-text-primary">{t('proj_constraint_section', { defaultValue: 'Contrainte de date' })}</p>
          <div className="grid grid-cols-2 gap-2">
            <div className={`flex flex-col gap-1${needsDate(constraintType) ? '' : ' col-span-2'}`}>
              <label className="text-xs text-text-tertiary">{t('proj_constraint', { defaultValue: 'Contrainte' })}</label>
              <Dropdown className="w-full" height={36} fontSize={14} value={constraintType}
                onChange={v => changeConstraintType(v as ConstraintType)} options={constraintOptions} />
            </div>
            {needsDate(constraintType) && (
              <div className="flex flex-col gap-1">
                <label className="text-xs text-text-tertiary">{t('proj_constraint_date', { defaultValue: 'Date' })}</label>
                <Input type="date" value={constraintDate} onChange={e => changeConstraintDate(e.target.value)} />
              </div>
            )}
          </div>
          {constraintPending && (
            <p className="text-xs text-text-tertiary">{t('proj_constraint_needs_date', { defaultValue: 'Choisissez une date pour appliquer cette contrainte.' })}</p>
          )}
          <div className="flex flex-col gap-1">
            <label className="text-xs text-text-tertiary">{t('proj_deadline', { defaultValue: 'Échéance' })}</label>
            <Input type="date" value={task.deadline_date ?? ''} onChange={e => onUpdate({ deadline_date: e.target.value || null })} />
            <p className="text-xs text-text-tertiary">{t('proj_deadline_hint', { defaultValue: 'Une échéance ne déplace pas le planning : elle signale seulement un retard.' })}</p>
            {task.deadline_missed && (
              <p className="text-xs text-danger flex items-center gap-1">
                <AlertTriangle size={13} />
                {t('proj_deadline_missed', { defaultValue: 'Échéance dépassée' })}
              </p>
            )}
          </div>
        </div>
        {projectId && showTimeLog && <TimeLogSection projectId={projectId} taskId={task.id} />}
        {showVersions && versions && versions.length > 0 && (
          <div>
            <label className="text-xs text-text-tertiary mb-1 block">{t('proj_version', { defaultValue: 'Version' })}</label>
            <Dropdown className="w-full" height={36} fontSize={14} value={task.version_id ?? ''} onChange={v => onUpdate({ version_id: (v || null) as ProjectTask['version_id'] })}
              options={[{ value: '', label: t('proj_version_none', { defaultValue: 'Aucune' }) }, ...versions.map(v => ({ value: v.id, label: v.name }))]} />
          </div>
        )}
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
              <span className={negativeFloat || task.is_critical ? 'text-danger font-medium' : 'text-success'}>
                {t('proj_days_short', { count: task.total_float ?? 0 })}{' '}
                {negativeFloat
                  ? t('proj_negative_float', { defaultValue: 'plan intenable' })
                  : task.is_critical ? t('proj_critical_warning') : ''}
              </span>
              {task.free_float != null && (<>
                {/* Free float answers a different question from total float: how long
                    this task can slip before it pushes a successor, rather than
                    before it pushes the project end. */}
                <span title={t('proj_free_float_hint', { defaultValue: 'Retard possible sans décaler la tâche suivante' })}>
                  {t('proj_free_float', { defaultValue: 'Marge libre :' })}
                </span>
                <span>{t('proj_days_short', { count: task.free_float })}</span>
              </>)}
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
