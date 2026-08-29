import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import { Input, Button } from '@ui'
import { CalendarOff, Plus, Trash2, UserRound } from 'lucide-react'
import { projectsApi, type ResourceTimeOff, type ProjectResource } from '../../api'
import type { ResourceViewProps } from './types'
import { behaviorOf } from './types'

// Availability sub-view: manage each resource's time off / leave. The workload
// heatmap treats these days as unavailable, so a booking over them reads as overload.
export default function ResourceAvailability({ projectId, resources, canEdit }: ResourceViewProps) {
  const { t } = useTranslation('office')
  // Only people and equipment have a working availability; material/cost do not.
  const list = resources.filter(r => behaviorOf(r.kind) === 'work')
  const [selId, setSelId] = useState<string | null>(list[0]?.id ?? null)
  const selected = list.find(r => r.id === selId) ?? null

  return (
    <div className="h-full w-full flex bg-surface-0">
      {/* Resource list */}
      <div className="w-64 shrink-0 border-r border-border overflow-y-auto">
        {list.length === 0 && (
          <p className="text-sm text-text-tertiary text-center py-10 px-4">
            {t('res_avail_none', { defaultValue: 'Ajoutez des personnes ou équipements pour gérer leur disponibilité.' })}
          </p>
        )}
        {list.map(r => (
          <button key={r.id} onClick={() => setSelId(r.id)}
            className={`w-full flex items-center gap-2.5 px-3 py-2 text-left border-b border-[#f1f3f4] ${selId === r.id ? 'bg-primary/5' : 'hover:bg-surface-1'}`}>
            {r.avatar_url
              ? <img src={r.avatar_url} alt="" className="w-7 h-7 rounded-full object-cover shrink-0" />
              : <span className="w-7 h-7 rounded-full flex items-center justify-center text-white text-xs font-bold shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</span>}
            <span className="flex-1 min-w-0">
              <span className="block text-sm text-text-primary truncate">{r.name}</span>
              {r.role && <span className="block text-xs text-text-tertiary truncate">{r.role}</span>}
            </span>
          </button>
        ))}
      </div>
      {/* Time-off panel for the selected resource */}
      <div className="flex-1 min-w-0 overflow-y-auto p-4">
        {selected
          ? <TimeOffEditor key={selected.id} projectId={projectId} resource={selected} canEdit={canEdit} />
          : <div className="h-full flex items-center justify-center text-text-tertiary text-sm">
              <UserRound size={18} className="mr-2 opacity-50" /> {t('res_avail_pick', { defaultValue: 'Choisissez une ressource.' })}
            </div>}
      </div>
    </div>
  )
}

function TimeOffEditor({ projectId, resource, canEdit }: { projectId: string; resource: ProjectResource; canEdit: boolean }) {
  const { t } = useTranslation('office')
  const [entries, setEntries] = useState<ResourceTimeOff[]>([])
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [reason, setReason] = useState('')
  const [busy, setBusy] = useState(false)

  const load = () => { projectsApi.listTimeOff(projectId, resource.id).then(setEntries).catch(() => setEntries([])) }
  useEffect(load, [projectId, resource.id])

  const add = async () => {
    if (!from || !to) return
    setBusy(true)
    try { await projectsApi.createTimeOff(projectId, resource.id, { from_date: from, to_date: to, reason: reason.trim() || undefined }); setFrom(''); setTo(''); setReason(''); load() }
    finally { setBusy(false) }
  }
  const remove = async (id: string) => { await projectsApi.deleteTimeOff(projectId, resource.id, id); load() }

  return (
    <div className="max-w-xl">
      <div className="flex items-center gap-2 mb-3">
        <CalendarOff size={16} className="text-text-secondary" />
        <h3 className="text-sm font-semibold text-text-primary">{t('res_avail_title', { defaultValue: 'Absences / congés' })} — {resource.name}</h3>
      </div>

      {canEdit && (
        <div className="flex flex-wrap items-end gap-2 mb-4 p-3 rounded-xl border border-border bg-surface-1">
          <label className="text-xs text-text-tertiary">
            <span className="block mb-1">{t('res_avail_from', { defaultValue: 'Du' })}</span>
            <Input type="date" value={from} onChange={e => setFrom(e.target.value)} />
          </label>
          <label className="text-xs text-text-tertiary">
            <span className="block mb-1">{t('res_avail_to', { defaultValue: 'Au' })}</span>
            <Input type="date" value={to} onChange={e => setTo(e.target.value)} />
          </label>
          <label className="text-xs text-text-tertiary flex-1 min-w-[140px]">
            <span className="block mb-1">{t('res_avail_reason', { defaultValue: 'Motif' })}</span>
            <Input type="text" value={reason} onChange={e => setReason(e.target.value)} placeholder={t('res_avail_reason_ph', { defaultValue: 'Congé, formation…' })} />
          </label>
          <Button onClick={add} disabled={!from || !to} loading={busy}><Plus size={14} /> {t('common_add', { defaultValue: 'Ajouter' })}</Button>
        </div>
      )}

      {entries.length === 0
        ? <p className="text-sm text-text-tertiary italic py-6 text-center">{t('res_avail_empty', { defaultValue: 'Aucune absence enregistrée.' })}</p>
        : <div className="space-y-2">
            {entries.map(e => (
              <div key={e.id} className="flex items-center gap-3 p-2.5 rounded-lg border border-border bg-surface-1">
                <CalendarOff size={14} className="text-text-tertiary shrink-0" />
                <span className="flex-1 min-w-0">
                  <span className="text-sm text-text-primary">{format(parseISO(e.from_date), 'd MMM yyyy')} → {format(parseISO(e.to_date), 'd MMM yyyy')}</span>
                  {e.reason && <span className="block text-xs text-text-tertiary truncate">{e.reason}</span>}
                </span>
                {canEdit && <button onClick={() => remove(e.id)} className="text-text-tertiary hover:text-danger p-1 shrink-0"><Trash2 size={14} /></button>}
              </div>
            ))}
          </div>}
    </div>
  )
}
