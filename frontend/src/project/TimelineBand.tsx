import { useTranslation } from 'react-i18next'
import { addDays, differenceInCalendarDays, format } from 'date-fns'
import { CalendarRange } from 'lucide-react'
import { TIMELINE_H, CRITICAL_CLR, TASK_COLOR } from './GanttRenderer'
import type { ProjectTask } from '../api'

// The overview timeline band above the Gantt: milestones + summaries positioned
// across the project span. Extracted from ProjectEditorPage.
export default function TimelineBand({ tasks, projectStart, totalDays, locale, onSelect, selectedId }: {
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
      <div className="relative h-6 rounded bg-surface-0 border border-[#e8eaed]">
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
              className={`absolute top-1/2 -translate-y-1/2 h-3 rounded-sm text-[10px] text-white truncate px-1 ${selectedId === tk.id ? 'ring-1 ring-primary' : ''}`}
              style={{ left, width: w, background: tk.is_critical ? CRITICAL_CLR : TASK_COLOR }}>{tk.name}</button>
          )
        })}
      </div>
    </div>
  )
}
