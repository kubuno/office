// Vue LISTE mobile des tâches d'un projet — remplace le tableau du Gantt (830 px
// de colonnes, impossible sur un téléphone) par des lignes à deux niveaux, façon
// application de gestion de projet mobile : nom + hiérarchie, dates, avancement,
// et une MINI-BARRE situant la tâche dans la durée du projet (le Gantt en un coup
// d'œil, sans défilement horizontal).
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown, ChevronRight, Diamond, Flag, Plus, X } from 'lucide-react'
import type { ProjectTask, ProjectResource } from '../api'

const PRIO_CLR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }
const STATUS_BG: Record<string, string> = {
  not_started: 'bg-surface-2 text-text-secondary',
  in_progress: 'bg-primary/10 text-primary',
  on_hold:     'bg-warning/15 text-warning',
  completed:   'bg-success/10 text-success',
  cancelled:   'bg-surface-2 text-text-tertiary line-through',
}

export interface MobileTaskRow {
  task: ProjectTask
  depth: number
  hasChildren: boolean
}

const DAY = 86400000

/** Position et largeur (en %) de la tâche dans la durée totale du projet. */
function span(start: Date, end: Date, projectStart: Date, totalDays: number) {
  const from = Math.max(0, Math.round((start.getTime() - projectStart.getTime()) / DAY))
  const days = Math.max(1, Math.round((end.getTime() - start.getTime()) / DAY) + 1)
  const total = Math.max(1, totalDays)
  return { left: Math.min(100, (from / total) * 100), width: Math.max(1.5, Math.min(100, (days / total) * 100)) }
}

export function MobileTaskList({
  rows, projectStart, totalDays, selectedId, collapsed, dateFmt, dates,
  onSelect, onToggle, onLongPress, onAdd, canEdit,
}: {
  rows: MobileTaskRow[]
  projectStart: Date
  totalDays: number
  selectedId: string | null
  collapsed: Set<string>
  dateFmt: (d: Date) => string
  /** Dates PLANIFIÉES (offsets CPM), identiques à celles des barres du Gantt. */
  dates: (task: ProjectTask) => { start: Date; end: Date }
  onSelect: (id: string) => void
  onToggle: (id: string) => void
  /** Appui long = menu contextuel de la tâche (il n'y a pas de clic droit au doigt). */
  onLongPress: (id: string, x: number, y: number) => void
  onAdd: () => void
  canEdit: boolean
}) {
  const { t } = useTranslation('office')

  // Appui long : armé au pointerdown, annulé au moindre déplacement (le doigt qui
  // fait défiler la liste ne doit pas ouvrir de menu).
  const lpRef = useRef<{ timer: ReturnType<typeof setTimeout>; x: number; y: number } | null>(null)
  // ⚠️ Au relâchement, le navigateur émet un clic souris SYNTHÉTIQUE : il ouvrait
  // la fiche de la tâche ET refermait le menu contextuel à peine affiché. Le
  // drapeau est consommé par `preventDefault` sur le `touchend` (seul moyen fiable
  // de supprimer toute la séquence souris synthétique).
  const lpFired = useRef(false)
  const cancel = () => { if (lpRef.current) { clearTimeout(lpRef.current.timer); lpRef.current = null } }
  const pressProps = (id: string) => !canEdit ? {} : {
    onPointerDown: (e: React.PointerEvent) => {
      if (e.pointerType === 'mouse') return
      const x = e.clientX, y = e.clientY
      cancel()
      lpRef.current = { x, y, timer: setTimeout(() => {
        lpRef.current = null
        lpFired.current = true
        navigator.vibrate?.(10)
        onLongPress(id, x, y)
      }, 500) }
    },
    onPointerMove: (e: React.PointerEvent) => {
      const lp = lpRef.current
      if (lp && Math.hypot(e.clientX - lp.x, e.clientY - lp.y) > 10) cancel()
    },
    onPointerUp: cancel,
    onPointerCancel: cancel,
    onTouchEnd: (e: React.TouchEvent) => { if (lpFired.current) { e.preventDefault(); lpFired.current = false } },
    onClick: (e: React.MouseEvent) => { if (lpFired.current) { e.preventDefault(); return } onSelect(id) },
  }

  return (
    <div className="flex-1 min-h-0 overflow-y-auto bg-white">
      {rows.map(({ task, depth, hasChildren }) => {
        const { start, end } = dates(task)
        const sp = span(start, end, projectStart, totalDays)
        const active = selectedId === task.id
        const done = task.status === 'completed'
        return (
          <div
            key={task.id}
            onClick={canEdit ? undefined : () => onSelect(task.id)}
            {...pressProps(task.id)}
            className={`flex items-start gap-2 px-3 py-2.5 border-b border-[#f1f3f4] touch-manipulation
                        ${active ? 'bg-primary/5' : 'active:bg-surface-1'}`}
            style={{ paddingLeft: 12 + depth * 14 }}
          >
            {hasChildren ? (
              <button onClick={e => { e.stopPropagation(); onToggle(task.id) }}
                className="w-7 h-7 -ml-1 flex items-center justify-center rounded-full text-text-tertiary active:bg-surface-2 flex-shrink-0">
                {collapsed.has(task.id) ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
              </button>
            ) : (
              <span className="w-1.5 h-1.5 mt-2 rounded-full flex-shrink-0" style={{ background: PRIO_CLR[task.priority] ?? '#9aa0a6' }} />
            )}

            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-1.5">
                {task.task_type === 'milestone' && <Diamond size={12} className="text-danger flex-shrink-0" />}
                {task.is_critical && <Flag size={12} className="text-danger flex-shrink-0" />}
                <span className={`text-xs truncate ${done ? 'text-text-tertiary line-through' : 'text-text-primary'} ${task.task_type === 'summary' ? 'font-medium' : ''}`}>
                  {task.name || t('proj_untitled_task', { defaultValue: 'Tâche sans nom' })}
                </span>
              </div>
              <div className="flex items-center gap-2 mt-0.5 text-[11px] text-text-tertiary">
                <span className="truncate">{dateFmt(start)} → {dateFmt(end)}</span>
                <span>·</span>
                <span className="whitespace-nowrap">{t('proj_days', { count: task.duration_days, defaultValue: `${task.duration_days} j` })}</span>
              </div>
              {/* Mini-planning : la tâche située dans la durée du projet. */}
              <div className="relative h-1.5 mt-1.5 rounded-full bg-surface-2 overflow-hidden">
                <div className="absolute top-0 bottom-0 rounded-full"
                  style={{ left: `${sp.left}%`, width: `${sp.width}%`, background: task.is_critical ? '#d93025' : task.task_type === 'summary' ? '#5f6368' : '#1a73e8' }} />
              </div>
            </div>

            <div className="flex flex-col items-end gap-1 flex-shrink-0">
              <span className={`px-1.5 py-0.5 rounded text-[10px] ${STATUS_BG[task.status] ?? ''}`}>
                {t('proj_status_' + task.status, { defaultValue: task.status })}
              </span>
              <span className="text-[11px] tabular-nums text-text-secondary">{task.progress}%</span>
            </div>
          </div>
        )
      })}

      {canEdit && (
        <button onClick={onAdd}
          className="flex items-center gap-2 w-full px-4 h-12 text-xs text-primary active:bg-surface-1 touch-manipulation">
          <Plus size={16} /> {t('proj_add_task', { defaultValue: 'Ajouter une tâche' })}
        </button>
      )}
      {!rows.length && !canEdit && (
        <p className="p-6 text-center text-xs text-text-tertiary">{t('proj_no_tasks', { defaultValue: 'Aucune tâche' })}</p>
      )}
      <div className="h-6" />
    </div>
  )
}

// ── Fiche de tâche (mode LECTURE) ────────────────────────────────────────────
// En lecture, on ne montre pas le formulaire d'édition : un résumé compact, plus
// lisible sur un téléphone et cohérent avec l'immersion « consultation ».
export function MobileTaskSummary({ task, resources, assignments, dateFmt, dates, onClose }: {
  task: ProjectTask
  resources: ProjectResource[]
  assignments: { task_id: string; resource_id: string; units: number }[]
  dateFmt: (d: Date) => string
  dates: (task: ProjectTask) => { start: Date; end: Date }
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const { start, end } = dates(task)
  const names = assignments.filter(a => a.task_id === task.id)
    .map(a => resources.find(r => r.id === a.resource_id)?.name).filter(Boolean)
  const Row = ({ label, value }: { label: string; value: React.ReactNode }) => (
    <div className="flex items-start justify-between gap-3 py-2 border-b border-[#f1f3f4] last:border-0">
      <span className="text-xs text-text-tertiary">{label}</span>
      <span className="text-xs text-text-primary text-right">{value}</span>
    </div>
  )
  return (
    <>
      <div className="fixed inset-0 z-[60] bg-black/40" style={{ animation: 'kb-sheet-fade .15s ease-out' }} onClick={onClose} />
      <div className="fixed left-0 right-0 bottom-0 z-[61] flex flex-col rounded-t-2xl bg-surface-1 shadow-2xl max-h-[70vh]"
        style={{ paddingBottom: 'env(safe-area-inset-bottom)', animation: 'kb-sheet-up .18s ease-out' }}>
        <div className="flex items-start justify-between gap-2 px-4 pt-3 pb-2 flex-shrink-0 border-b border-border">
          <span className="text-sm font-medium text-text-primary">{task.name}</span>
          <button onClick={onClose} className="w-9 h-9 -mr-2 -mt-1 flex items-center justify-center rounded-full text-text-secondary active:bg-surface-2 flex-shrink-0">
            <X size={18} />
          </button>
        </div>
        <div className="px-4 py-2 overflow-y-auto">
          <Row label={t('proj_col_status', { defaultValue: 'Statut' })} value={t('proj_status_' + task.status, { defaultValue: task.status })} />
          <Row label={t('proj_col_priority', { defaultValue: 'Priorité' })} value={t('proj_priority_' + task.priority, { defaultValue: task.priority })} />
          <Row label={t('proj_col_start', { defaultValue: 'Début' })} value={dateFmt(start)} />
          <Row label={t('proj_col_end', { defaultValue: 'Fin' })} value={dateFmt(end)} />
          <Row label={t('proj_col_duration', { defaultValue: 'Durée' })} value={t('proj_days', { count: task.duration_days, defaultValue: `${task.duration_days} j` })} />
          <Row label={t('proj_col_progress', { defaultValue: 'Avancement' })} value={`${task.progress} %`} />
          {task.is_critical && <Row label={t('proj_critical_path', { defaultValue: 'Chemin critique' })} value={t('common_yes', { defaultValue: 'Oui' })} />}
          {!!names.length && <Row label={t('proj_resources', { defaultValue: 'Ressources' })} value={names.join(', ')} />}
          {!!task.description && (
            <div className="py-2">
              <p className="text-xs text-text-tertiary mb-1">{t('proj_description', { defaultValue: 'Description' })}</p>
              <p className="text-xs text-text-primary whitespace-pre-wrap">{task.description}</p>
            </div>
          )}
        </div>
      </div>
    </>
  )
}
