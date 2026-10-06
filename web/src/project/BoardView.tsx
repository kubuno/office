import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Milestone, Flag } from 'lucide-react'
import { PROGRESS_CLR } from './GanttRenderer'
import type { ProjectTask, ProjectResource } from '../api'

const PRIO_CLR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }
const BOARD_COLS: Array<[string, string, string]> = [
  ['not_started', 'À faire', '#9aa0a6'],
  ['in_progress', 'En cours', '#1a73e8'],
  ['on_hold', 'En attente', '#fbbc04'],
  ['completed', 'Terminé', '#34a853'],
  ['cancelled', 'Annulé', '#d93025'],
]

// Kanban board: leaf tasks grouped by status, drag a card between columns to set it.
export default function BoardView({ tasks, resources, assignments, selectedId, onSelect, onSetStatus, onContextMenu, progressMap }: {
  tasks: ProjectTask[]; resources: ProjectResource[]; assignments: { task_id: string; resource_id: string }[]
  selectedId: string | null; onSelect: (id: string) => void
  onSetStatus: (taskId: string, status: string) => void
  onContextMenu: (e: React.MouseEvent, taskId: string) => void
  progressMap: Map<string, number>
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
            className={`flex-shrink-0 w-64 bg-surface-0 rounded-lg border flex flex-col max-h-full ${overCol === st ? 'border-primary ring-1 ring-primary/30' : 'border-border'}`}>
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border" style={{ borderTop: `3px solid ${clr}` }}>
              <span className="text-xs font-semibold text-text-primary">{t('proj_status_' + st, { defaultValue: label })}</span>
              <span className="text-[11px] text-text-tertiary">{col.length}</span>
            </div>
            <div className="flex-1 overflow-y-auto p-2 space-y-2 min-h-[60px]">
              {col.map(tk => (
                <div key={tk.id} draggable
                  onDragStart={() => setDragId(tk.id)} onDragEnd={() => { setDragId(null); setOverCol(null) }}
                  onClick={() => onSelect(tk.id)} onContextMenu={e => onContextMenu(e, tk.id)}
                  className={`rounded-md border bg-surface-0 p-2 cursor-pointer transition-shadow hover:shadow-sm ${selectedId === tk.id ? 'border-primary ring-1 ring-primary/30' : 'border-border'}`}>
                  <div className="flex items-start gap-1.5">
                    {tk.task_type === 'milestone' && <Milestone size={12} className="text-orange-500 mt-0.5 flex-shrink-0" />}
                    <Flag size={11} className="mt-0.5 flex-shrink-0" style={{ color: PRIO_CLR[tk.priority] ?? '#9aa0a6' }} />
                    <span className="text-xs text-text-primary leading-snug flex-1">{tk.name}</span>
                  </div>
                  {(() => { const pct = progressMap.get(tk.id) ?? tk.progress; return pct > 0 && (
                    <div className="h-1 bg-surface-3 rounded-full overflow-hidden mt-1.5"><div className="h-full rounded-full" style={{ width: `${pct}%`, background: PROGRESS_CLR }} /></div>
                  ) })()}
                  {resOf(tk.id).length > 0 && (
                    <div className="flex items-center gap-0.5 mt-1.5">
                      {resOf(tk.id).slice(0, 4).map(r => (
                        <span key={r.id} title={r.name} className="w-5 h-5 rounded-full flex items-center justify-center text-white text-[10px] font-bold ring-1 ring-white" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</span>
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
