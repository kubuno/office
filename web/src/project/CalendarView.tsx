import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { addMonths, startOfMonth, startOfWeek, addDays, isSameMonth, isSameDay, formatDate } from '@kubuno/sdk'
import { ChevronRight } from 'lucide-react'
import { schedStart, schedEnd } from './schedule'
import { CRITICAL_CLR, MILESTONE_CLR, TASK_COLOR } from './GanttRenderer'
import type { ProjectTask } from '../api'

// Month grid: each leaf task is drawn on the days it spans (schedule-aware).
export default function CalendarView({ tasks, projectStart, selectedId, onSelect, onContextMenu }: {
  tasks: ProjectTask[]; projectStart: Date
  selectedId: string | null; onSelect: (id: string) => void
  onContextMenu: (e: React.MouseEvent, taskId: string) => void
}) {
  const { t } = useTranslation('office')
  const [monthOffset, setMonthOffset] = useState(0)
  const month = addMonths(startOfMonth(projectStart), monthOffset)
  const gridStart = startOfWeek(month, 1)
  const days = Array.from({ length: 42 }, (_, i) => addDays(gridStart, i))
  const items = tasks.filter(tk => tk.task_type !== 'summary').map(tk => ({ tk, s: schedStart(tk, projectStart), e: schedEnd(tk, projectStart) }))
  const dow = Array.from({ length: 7 }, (_, i) => formatDate(addDays(gridStart, i), { weekday: 'short' }))
  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-surface-0">
      <div className="flex items-center gap-3 px-4 py-2 border-b border-border">
        <button onClick={() => setMonthOffset(o => o - 1)} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><ChevronRight size={16} className="rotate-180" /></button>
        <span className="text-sm font-semibold text-text-primary capitalize min-w-[140px] text-center">{formatDate(month, { month: 'long', year: 'numeric' })}</span>
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
              <div className={`text-[10px] mb-0.5 ${today ? 'bg-primary text-white rounded-full w-4 h-4 flex items-center justify-center' : inMonth ? 'text-text-secondary' : 'text-text-tertiary'}`}>{formatDate(day, { day: 'numeric' })}</div>
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
