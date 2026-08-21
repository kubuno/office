import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Plus, Trash2, Clock } from 'lucide-react'
import { Input, Dropdown, Button } from '@ui'
import { projectsApi, type TimeActivity } from '../api'

const ACTIVITIES: TimeActivity[] = ['development', 'design', 'coordination', 'testing', 'documentation', 'other']

// Per-task time log: dated hours entries whose sum rolls up into the task's spent
// hours. Modelled on OpenProject's time tracking. Mounted in the task inspector.
export default function TimeLogSection({ projectId, taskId }: { projectId: string; taskId: string }) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const [hours, setHours] = useState('')
  const [spentOn, setSpentOn] = useState(() => new Date().toISOString().slice(0, 10))
  const [activity, setActivity] = useState<TimeActivity>('development')
  const [comment, setComment] = useState('')

  const activityLabel = (a: TimeActivity) => t('proj_activity_' + a, { defaultValue: a })

  const { data: entries } = useQuery({
    queryKey: ['time-entries', projectId, taskId],
    queryFn: () => projectsApi.listTimeEntries(projectId, taskId),
  })

  // A logged entry changes the task's rolled-up spent_hours, so refresh the project too.
  const inval = () => {
    qc.invalidateQueries({ queryKey: ['time-entries', projectId, taskId] })
    qc.invalidateQueries({ queryKey: ['project', projectId] })
  }
  const createMut = useMutation({
    mutationFn: () => projectsApi.createTimeEntry(projectId, { task_id: taskId, spent_on: spentOn, hours: parseFloat(hours) || 0, activity, comment: comment.trim() }),
    onSuccess: () => { setHours(''); setComment(''); inval() },
  })
  const deleteMut = useMutation({
    mutationFn: (eid: string) => projectsApi.deleteTimeEntry(projectId, eid),
    onSuccess: inval,
  })

  const total = (entries ?? []).reduce((s, e) => s + e.hours, 0)

  return (
    <div>
      <div className="flex items-center gap-1.5 mb-2">
        <Clock size={13} className="text-text-tertiary" />
        <label className="text-xs text-text-tertiary">{t('proj_time_log', { defaultValue: 'Journal de temps' })}</label>
        {total > 0 && <span className="text-xs text-text-secondary ml-auto">{t('proj_time_total', { defaultValue: '{{h}} h au total', h: total })}</span>}
      </div>

      {(entries ?? []).length > 0 && (
        <ul className="space-y-1 mb-2">
          {entries!.map(e => (
            <li key={e.id} className="flex items-center gap-2 text-xs bg-surface-1 rounded px-2 py-1">
              <span className="text-text-tertiary shrink-0 tabular-nums">{format(new Date(e.spent_on), 'd MMM', { locale: getDateLocale(i18n.language) })}</span>
              <span className="font-medium text-text-primary tabular-nums shrink-0">{e.hours} h</span>
              <span className="text-text-secondary shrink-0">{activityLabel(e.activity)}</span>
              {e.comment && <span className="text-text-tertiary truncate">· {e.comment}</span>}
              <button onClick={() => deleteMut.mutate(e.id)} className="ml-auto p-0.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger shrink-0" aria-label={t('common_delete', { defaultValue: 'Supprimer' })}><Trash2 size={13} /></button>
            </li>
          ))}
        </ul>
      )}

      {/* Add form — all fields share the @ui field height (h-9) and type scale. */}
      <div className="grid grid-cols-2 gap-1.5">
        <Input type="date" value={spentOn} onChange={e => setSpentOn(e.target.value)} />
        <Input type="number" min="0" step="0.25" placeholder={t('proj_hours_placeholder', { defaultValue: 'Heures' })} value={hours} onChange={e => setHours(e.target.value)} />
        <Dropdown className="w-full" height={36} fontSize={14} value={activity} onChange={v => setActivity(v as TimeActivity)}
          options={ACTIVITIES.map(a => ({ value: a, label: activityLabel(a) }))} />
        <Input placeholder={t('proj_comment_placeholder', { defaultValue: 'Commentaire' })} value={comment} onChange={e => setComment(e.target.value)} />
      </div>
      <Button size="sm" variant="secondary" icon={<Plus size={14} />} className="mt-1.5 w-full justify-center"
        disabled={!(parseFloat(hours) > 0)} loading={createMut.isPending} onClick={() => createMut.mutate()}>
        {t('proj_time_add', { defaultValue: 'Enregistrer le temps' })}
      </Button>
    </div>
  )
}
