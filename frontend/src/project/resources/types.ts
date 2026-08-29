import type { ProjectResource, ProjectTask, TaskAssignment } from '../../api'

/** Everything a resource sub-view needs. Sub-views mutate through `projectsApi`
 *  (they have `projectId`) and call `onRefresh` to reload the project bundle. */
export interface ResourceViewProps {
  projectId:    string
  resources:    ProjectResource[]
  assignments:  TaskAssignment[]
  tasks:        ProjectTask[]
  projectStart: Date
  totalDays:    number
  locale:       import('date-fns').Locale
  canEdit:      boolean
  onRefresh:    () => void
}

/** Peak daily allocation (in capacity units) per resource id, over the project span.
 *  Shared by the registry and the heatmap so utilisation reads the same everywhere. */
export function peakLoadByResource(props: Pick<ResourceViewProps, 'resources' | 'assignments' | 'tasks' | 'totalDays'>): Map<string, number> {
  const { assignments, tasks, totalDays } = props
  const byId = new Map(tasks.map(t => [t.id, t]))
  const peak = new Map<string, number>()
  const daily = new Map<string, Float64Array>()
  for (const a of assignments) {
    const t = byId.get(a.task_id)
    if (!t || t.task_type === 'summary') continue
    const es = t.early_start ?? 0
    const end = Math.min(totalDays, es + Math.max(1, t.duration_days))
    let arr = daily.get(a.resource_id)
    if (!arr) { arr = new Float64Array(Math.max(1, totalDays)); daily.set(a.resource_id, arr) }
    for (let d = Math.max(0, es); d < end; d++) arr[d] += a.units ?? 1
  }
  for (const [rid, arr] of daily) {
    let m = 0
    for (let i = 0; i < arr.length; i++) if (arr[i] > m) m = arr[i]
    peak.set(rid, m)
  }
  return peak
}

/** Number of tasks a resource is assigned to. */
export function taskCountByResource(assignments: TaskAssignment[]): Map<string, number> {
  const m = new Map<string, number>()
  for (const a of assignments) m.set(a.resource_id, (m.get(a.resource_id) ?? 0) + 1)
  return m
}

/** Cost/scheduling behaviour of a resource kind:
 *  - 'work'     : consumes time, has a capacity → appears in the workload heatmap;
 *  - 'material' : consumed by the unit (unit price), no capacity;
 *  - 'cost'     : a fixed amount, no time and no unit. */
export type ResourceBehavior = 'work' | 'material' | 'cost'
export type ResourceCategory = 'human' | 'physical' | 'tech' | 'financial'

export interface KindMeta { label: string; tint: string; behavior: ResourceBehavior; category: ResourceCategory; external?: boolean }

// The PMI-flavoured resource taxonomy: human, physical, technological/informational
// and financial resources, each mapped to a cost behaviour.
export const KIND_META: Record<string, KindMeta> = {
  person:         { label: "Membre d'équipe",           tint: 'var(--color-primary)', behavior: 'work',     category: 'human' },
  contractor:     { label: 'Consultant / sous-traitant', tint: '#5f6368',             behavior: 'work',     category: 'human', external: true },
  equipment:      { label: 'Équipement',                 tint: '#00897b',             behavior: 'work',     category: 'physical' },
  facility:       { label: 'Installation / local',       tint: '#3949ab',             behavior: 'work',     category: 'physical' },
  material:       { label: 'Matériel / consommable',     tint: '#e37400',             behavior: 'material', category: 'physical' },
  software:       { label: 'Licence logicielle',         tint: '#8e24aa',             behavior: 'material', category: 'tech' },
  infrastructure: { label: 'Infrastructure cloud/réseau', tint: '#0277bd',            behavior: 'cost',     category: 'tech' },
  information:    { label: 'Données / doc / PI',          tint: '#00838f',             behavior: 'cost',     category: 'tech' },
  financial:      { label: 'Budget / réserve',           tint: '#2e7d32',             behavior: 'cost',     category: 'financial' },
  cost:           { label: 'Coût forfaitaire',           tint: '#9334e6',             behavior: 'cost',     category: 'financial' },
}

export function behaviorOf(kind: string): ResourceBehavior {
  return KIND_META[kind]?.behavior ?? 'work'
}

/** The add menu, grouped by PMI category. */
export const CATEGORY_GROUPS: { category: ResourceCategory; label: string; kinds: string[] }[] = [
  { category: 'human',     label: 'Humaines',       kinds: ['person', 'contractor'] },
  { category: 'physical',  label: 'Physiques',      kinds: ['equipment', 'facility', 'material'] },
  { category: 'tech',      label: 'Technologiques', kinds: ['software', 'infrastructure', 'information'] },
  { category: 'financial', label: 'Financières',    kinds: ['financial', 'cost'] },
]
