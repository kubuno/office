import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Package, Plus, Trash2, Milestone, Flag } from 'lucide-react'
import { Button, Input, Dropdown, Checkbox } from '@ui'
import { projectsApi, type ProjectTask, type ProjectVersion } from '../api'

const CLOSED_STATUS = new Set(['completed', 'cancelled'])

// Roadmap: release versions grouping tasks, each with a progress bar and its task
// list (done tasks struck through) — modelled on OpenProject's Roadmap page.
export default function RoadmapView({ projectId, tasks, onOpenTask }: {
  projectId: string
  tasks: ProjectTask[]
  onOpenTask?: (id: string) => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const [showClosed, setShowClosed] = useState(false)

  const { data: versions } = useQuery({ queryKey: ['versions', projectId], queryFn: () => projectsApi.listVersions(projectId) })
  const inval = () => qc.invalidateQueries({ queryKey: ['versions', projectId] })
  const createMut = useMutation({ mutationFn: () => projectsApi.createVersion(projectId), onSuccess: inval })
  const updateMut = useMutation({ mutationFn: ({ vid, data }: { vid: string; data: Partial<Pick<ProjectVersion, 'name' | 'due_date' | 'status'>> }) => projectsApi.updateVersion(projectId, vid, data), onSuccess: inval })
  const deleteMut = useMutation({ mutationFn: (vid: string) => projectsApi.deleteVersion(projectId, vid), onSuccess: inval })

  const byVersion = useMemo(() => {
    const m = new Map<string, ProjectTask[]>()
    for (const tk of tasks) { if (tk.version_id) { const a = m.get(tk.version_id) ?? []; a.push(tk); m.set(tk.version_id, a) } }
    return m
  }, [tasks])

  const list = (versions ?? []).filter(v => showClosed || v.status !== 'closed')

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="max-w-3xl mx-auto p-6">
        <div className="flex items-center gap-3 mb-5">
          <h1 className="text-xl font-semibold text-text-primary">{t('proj_roadmap', { defaultValue: 'Roadmap' })}</h1>
          <div className="flex-1" />
          <Checkbox checked={showClosed} onChange={setShowClosed} label={t('proj_roadmap_show_closed', { defaultValue: 'Versions terminées' })} />
          <Button size="sm" icon={<Plus size={14} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('proj_version_new', { defaultValue: 'Nouvelle version' })}
          </Button>
        </div>

        {(versions ?? []).length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl p-10 flex flex-col items-center text-center gap-3">
            <div className="w-12 h-12 rounded-xl bg-surface-2 flex items-center justify-center"><Package size={22} className="text-text-tertiary" /></div>
            <p className="text-sm text-text-secondary max-w-md">{t('proj_roadmap_empty', { defaultValue: 'Aucune version. Créez une version pour regrouper des tâches en jalons de livraison et suivre leur avancement.' })}</p>
          </div>
        ) : list.map(v => {
          const vtasks = byVersion.get(v.id) ?? []
          const done = vtasks.filter(tk => CLOSED_STATUS.has(tk.status)).length
          const total = vtasks.length
          const pct = total ? Math.round((done / total) * 100) : 0
          return (
            <div key={v.id} className="bg-surface-0 border border-border rounded-xl p-5 mb-4">
              <div className="flex items-center gap-2 mb-3">
                <Package size={18} className="text-text-secondary shrink-0" />
                <Input value={v.name} onChange={e => updateMut.mutate({ vid: v.id, data: { name: e.target.value } })} className="max-w-xs" />
                <Dropdown height={36} fontSize={14} value={v.status} onChange={s => updateMut.mutate({ vid: v.id, data: { status: s as ProjectVersion['status'] } })}
                  options={[{ value: 'open', label: t('proj_version_open', { defaultValue: 'Ouverte' }) }, { value: 'locked', label: t('proj_version_locked', { defaultValue: 'Verrouillée' }) }, { value: 'closed', label: t('proj_version_closed', { defaultValue: 'Terminée' }) }]} />
                <div className="flex-1" />
                <Input type="date" value={v.due_date ?? ''} onChange={e => updateMut.mutate({ vid: v.id, data: { due_date: e.target.value || null } })}
                  className="max-w-[9.5rem]" title={t('proj_version_due', { defaultValue: 'Échéance' })} />
                <button onClick={() => deleteMut.mutate(v.id)} className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"><Trash2 size={15} /></button>
              </div>

              {/* Progress bar */}
              <div className="flex items-center gap-3 mb-2">
                <div className="flex-1 h-2.5 rounded-full bg-surface-2 overflow-hidden">
                  <div className="h-full rounded-full transition-all" style={{ width: `${pct}%`, background: pct === 100 ? '#1e8e3e' : 'var(--color-primary)' }} />
                </div>
                <span className="text-xs text-text-secondary shrink-0">{t('proj_roadmap_progress', { defaultValue: '{{pct}}% d’avancement', pct })}</span>
              </div>
              <p className="text-xs text-text-tertiary mb-3">
                {t('proj_roadmap_counts', { defaultValue: '{{done}} terminée(s) · {{open}} en cours', done, open: total - done })}
                {v.due_date && ` · ${t('proj_version_due', { defaultValue: 'Échéance' })} ${format(new Date(v.due_date), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })}`}
              </p>

              {/* Related tasks */}
              {vtasks.length === 0 ? (
                <p className="text-xs text-text-tertiary italic">{t('proj_roadmap_no_tasks', { defaultValue: 'Aucune tâche rattachée à cette version.' })}</p>
              ) : (
                <ul className="space-y-1">
                  {vtasks.map(tk => {
                    const isDone = CLOSED_STATUS.has(tk.status)
                    const Icon = tk.task_type === 'milestone' ? Milestone : Flag
                    return (
                      <li key={tk.id}>
                        <button onClick={() => onOpenTask?.(tk.id)} className="flex items-center gap-1.5 text-left text-sm hover:underline">
                          <Icon size={12} className={isDone ? 'text-text-tertiary' : 'text-[var(--color-primary)]'} />
                          <span className={isDone ? 'line-through text-text-tertiary' : 'text-text-primary'}>{tk.name}</span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}
