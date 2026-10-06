import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  Coins, Info, PieChart, User, Users, Wallet,
} from 'lucide-react'
import {
  Badge, Callout, Card, DataTable, EmptyState,
  type DataTableColumn,
} from '@ui'
import {
  projectsApi,
  type CostOverview, type ProjectResource, type ProjectTask, type TaskAssignment,
} from '../../api'
import type { ResourceViewProps } from './types'
import { KIND_META, behaviorOf, taskCountByResource } from './types'
import { KIND_ICON } from './kindIcons'

// ── Costing assumptions ─────────────────────────────────────────────────────────
// This screen is an ESTIMATE, not an earned-value engine (that lives in
// EarnedValueView). It answers a blunter question — "what does each resource cost
// us, roughly?" — by valuing planned effort at the resource's standard rate.
//
// The one modelling choice worth stating up front: a task's calendar duration is
// turned into worked hours with a flat 8 h working day. We have no per-task
// working calendar here, so this is a deliberate approximation, surfaced in the
// help text rather than hidden.
const HOURS_PER_DAY = 8

/** lucide glyph for each resource kind (KIND_META only carries the icon *name*). */
/** Kubuno's currency field is free-form; a couple of common symbols are mapped to
 *  ISO codes so `Intl` does not throw, and anything else is passed through. */
function normaliseCurrency(raw: string | undefined): string {
  if (!raw || raw === '€') return 'EUR'
  if (raw === '$') return 'USD'
  if (raw === '£') return 'GBP'
  return raw
}

/** A currency formatter that survives an unknown code (Intl throws otherwise). */
function makeMoney(currency: string, language: string) {
  let fmt: Intl.NumberFormat
  try {
    fmt = new Intl.NumberFormat(language, { style: 'currency', currency, maximumFractionDigits: 0 })
  } catch {
    fmt = new Intl.NumberFormat(language, { maximumFractionDigits: 0 })
  }
  return (v: number) => fmt.format(v)
}

// ── Per-resource cost model ─────────────────────────────────────────────────────

interface CostRow {
  resource:     ProjectResource
  /** Planned worked hours (person/equipment) — null for material/cost kinds. */
  plannedHours: number | null
  /** Planned material quantity — null unless the kind is material. */
  plannedQty:   number | null
  /** The rate shown in the table (per hour, per unit, or a flat fee). */
  rate:         number
  /** '/h', '/{unit}' or '' (flat fee), for the rate cell. */
  rateSuffix:   string
  plannedCost:  number
  /** Actual cost, distributed from logged hours — null when nothing is logged. */
  actualCost:   number | null
}

/**
 * Builds one costed row per resource from the assignment graph.
 *
 * Planned cost is `effort × standard rate`, plus a per-use fee for every task the
 * resource is booked on. Actual cost only exists for labour-like kinds and only
 * when someone has actually logged hours; otherwise it is `null` and the cell
 * shows an em dash rather than a flattering zero.
 */
function buildRows(
  resources: ProjectResource[],
  assignments: TaskAssignment[],
  tasks: ProjectTask[],
  defaultRate: number,
): { rows: CostRow[]; hasActuals: boolean } {
  const taskById = new Map(tasks.map(t => [t.id, t]))
  const counts = taskCountByResource(assignments)

  // Total assignment units per task — used to split a task's logged hours across
  // the resources booked on it, weighted by their allocation.
  const unitTotalByTask = new Map<string, number>()
  for (const a of assignments) {
    const t = taskById.get(a.task_id)
    if (!t || t.task_type === 'summary') continue
    unitTotalByTask.set(a.task_id, (unitTotalByTask.get(a.task_id) ?? 0) + (a.units ?? 1))
  }

  const hasActuals = tasks.some(t => (t.spent_hours ?? 0) > 0)

  // Accumulate planned hours and distributed actual hours per resource.
  const plannedHoursById = new Map<string, number>()
  const actualHoursById = new Map<string, number>()
  for (const a of assignments) {
    const t = taskById.get(a.task_id)
    if (!t || t.task_type === 'summary') continue
    const units = a.units ?? 1
    const plannedH = units * Math.max(0, t.duration_days) * HOURS_PER_DAY
    plannedHoursById.set(a.resource_id, (plannedHoursById.get(a.resource_id) ?? 0) + plannedH)

    const totalUnits = unitTotalByTask.get(a.task_id) ?? units
    const spent = t.spent_hours ?? 0
    if (spent > 0 && totalUnits > 0) {
      const share = spent * (units / totalUnits)
      actualHoursById.set(a.resource_id, (actualHoursById.get(a.resource_id) ?? 0) + share)
    }
  }

  const rows = resources.map<CostRow>(r => {
    const assignmentCount = counts.get(r.id) ?? 0
    const perUse = (r.cost_per_use ?? 0) * assignmentCount

    if (behaviorOf(r.kind) === 'cost') {
      // A flat-fee resource: the whole cost is the per-use amount, booked once per
      // task it appears on. There is no rate and no effort.
      return {
        resource: r,
        plannedHours: null,
        plannedQty: null,
        rate: r.cost_per_use ?? 0,
        rateSuffix: '',
        plannedCost: perUse,
        actualCost: null,
      }
    }

    if (behaviorOf(r.kind) === 'material') {
      // Material has a unit price and a quantity we cannot infer from the schedule
      // — so quantity is left at 0 (to be estimated) and only the per-use fee, if
      // any, contributes to the planned cost.
      const rate = r.hourly_rate ?? 0
      return {
        resource: r,
        plannedHours: null,
        plannedQty: 0,
        rate,
        rateSuffix: r.unit_label ? `/${r.unit_label}` : '/u',
        plannedCost: 0 * rate + perUse,
        actualCost: null,
      }
    }

    // person / equipment: valued at the standard hourly rate (or the project
    // default when the resource has none of its own).
    const rate = r.hourly_rate ?? defaultRate
    const plannedHours = plannedHoursById.get(r.id) ?? 0
    const actualHours = actualHoursById.get(r.id)
    return {
      resource: r,
      plannedHours,
      plannedQty: null,
      rate,
      rateSuffix: '/h',
      plannedCost: plannedHours * rate + perUse,
      actualCost: hasActuals && actualHours !== undefined ? actualHours * rate : null,
    }
  })

  return { rows, hasActuals }
}

// ── Small display helpers ───────────────────────────────────────────────────────

/** A resource identity cell: avatar (image or coloured initial) + name + role. */
function ResourceCell({ resource }: { resource: ProjectResource }) {
  const initial = resource.name.trim().charAt(0).toUpperCase() || '?'
  return (
    <div className="flex items-center gap-2.5 min-w-0">
      {resource.avatar_url ? (
        <img
          src={resource.avatar_url}
          alt=""
          className="w-7 h-7 rounded-full object-cover shrink-0"
        />
      ) : (
        <span
          className="w-7 h-7 rounded-full shrink-0 flex items-center justify-center text-xs font-medium text-white"
          style={{ background: resource.color || 'var(--color-primary)' }}
          aria-hidden="true"
        >
          {initial}
        </span>
      )}
      <div className="min-w-0">
        <div className="text-sm text-text-primary truncate">{resource.name}</div>
        {resource.role && (
          <div className="text-[11px] text-text-tertiary truncate">{resource.role}</div>
        )}
      </div>
    </div>
  )
}

/** A KPI figure, mirroring EarnedValueView's compact stat block. */
function Figure({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="min-w-0">
      <div className="text-xs text-text-secondary truncate">{label}</div>
      <div className="text-lg font-semibold tabular-nums text-text-primary">{value}</div>
      {hint && <div className="text-[11px] text-text-tertiary leading-snug mt-0.5">{hint}</div>}
    </div>
  )
}

const numberFmt = (v: number, language: string, digits = 0) =>
  new Intl.NumberFormat(language, { maximumFractionDigits: digits }).format(v)

// ── The screen ──────────────────────────────────────────────────────────────────

export default function ResourceCostView(props: ResourceViewProps) {
  const { projectId, resources, assignments, tasks } = props
  const { t, i18n } = useTranslation('office')
  const language = i18n.language

  // The cost config gives us the display currency and the fallback hourly rate.
  // It is a nicety, not a dependency: if the call fails we fall back to sane
  // defaults ('€' → EUR, no default rate) and the table still renders.
  const { data: costs } = useQuery<CostOverview>({
    queryKey: ['costs', projectId],
    queryFn: () => projectsApi.getCosts(projectId),
    retry: false,
    staleTime: 60_000,
  })

  const currency = normaliseCurrency(costs?.config.currency)
  const defaultRate = costs?.config.default_hourly_rate ?? 0
  const money = useMemo(() => makeMoney(currency, language), [currency, language])

  const { rows, hasActuals } = useMemo(
    () => buildRows(resources, assignments, tasks, defaultRate),
    [resources, assignments, tasks, defaultRate],
  )

  // ── Aggregates ────────────────────────────────────────────────────────────────
  const totalPlanned = rows.reduce((s, r) => s + r.plannedCost, 0)
  const totalActual = rows.reduce((s, r) => s + (r.actualCost ?? 0), 0)
  const avgPerResource = rows.length ? totalPlanned / rows.length : 0

  // Planned cost split by resource kind, for the mix recap.
  const byKind = useMemo(() => {
    const acc: Record<string, number> = { person: 0, equipment: 0, material: 0, cost: 0 }
    for (const r of rows) acc[r.resource.kind] = (acc[r.resource.kind] ?? 0) + r.plannedCost
    return acc
  }, [rows])

  const columns: DataTableColumn<CostRow>[] = [
    {
      id: 'resource',
      header: t('proj_rescost_col_resource', { defaultValue: 'Ressource' }),
      headerText: t('proj_rescost_col_resource', { defaultValue: 'Ressource' }),
      minWidth: 200, primary: true, required: true,
      sortValue: r => r.resource.name,
      cell: r => <ResourceCell resource={r.resource} />,
    },
    {
      id: 'kind',
      header: t('proj_rescost_col_kind', { defaultValue: 'Type' }),
      headerText: t('proj_rescost_col_kind', { defaultValue: 'Type' }),
      width: 130,
      sortValue: r => r.resource.kind,
      cell: r => {
        const meta = KIND_META[r.resource.kind]
        const Icon = KIND_ICON[r.resource.kind] ?? User
        return (
          <span
            className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-xs font-medium"
            style={{ color: meta?.tint, background: `color-mix(in srgb, ${meta?.tint} 12%, transparent)` }}
          >
            <Icon size={13} />
            {meta?.label ?? r.resource.kind}
          </span>
        )
      },
    },
    {
      id: 'rate',
      header: t('proj_rescost_col_rate', { defaultValue: 'Taux' }),
      headerText: t('proj_rescost_col_rate', { defaultValue: 'Taux' }),
      align: 'right', width: 130,
      sortValue: r => r.rate,
      cell: r => (
        <span className="tabular-nums text-sm text-text-primary">
          {money(r.rate)}
          <span className="text-text-tertiary">{r.rateSuffix}</span>
        </span>
      ),
    },
    {
      id: 'planned_units',
      header: t('proj_rescost_col_units', { defaultValue: 'Quantité planifiée' }),
      headerText: t('proj_rescost_col_units', { defaultValue: 'Quantité planifiée' }),
      align: 'right', width: 160,
      sortValue: r => r.plannedHours ?? r.plannedQty ?? -1,
      cell: r => {
        if (r.plannedHours !== null) {
          return (
            <span className="tabular-nums text-sm text-text-secondary">
              {numberFmt(r.plannedHours, language, 1)}{' '}
              {t('proj_rescost_hours_suffix', { defaultValue: 'h' })}
            </span>
          )
        }
        if (r.plannedQty !== null) {
          return (
            <span className="tabular-nums text-sm text-text-secondary">
              {numberFmt(r.plannedQty, language, 0)}{' '}
              {r.resource.unit_label ?? t('proj_rescost_unit_generic', { defaultValue: 'u.' })}
            </span>
          )
        }
        return <span className="text-xs text-text-tertiary">{t('proj_rescost_dash', { defaultValue: '—' })}</span>
      },
    },
    {
      id: 'planned_cost',
      header: t('proj_rescost_col_planned', { defaultValue: 'Coût planifié' }),
      headerText: t('proj_rescost_col_planned', { defaultValue: 'Coût planifié' }),
      align: 'right', width: 150,
      sortValue: r => r.plannedCost,
      cell: r => <span className="tabular-nums text-sm font-medium text-text-primary">{money(r.plannedCost)}</span>,
    },
    {
      id: 'actual_cost',
      header: t('proj_rescost_col_actual', { defaultValue: 'Coût réel' }),
      headerText: t('proj_rescost_col_actual', { defaultValue: 'Coût réel' }),
      align: 'right', width: 150,
      sortValue: r => r.actualCost ?? -1,
      cell: r => r.actualCost === null
        ? <span className="text-xs text-text-tertiary">{t('proj_rescost_dash', { defaultValue: '—' })}</span>
        : <span className="tabular-nums text-sm text-text-primary">{money(r.actualCost)}</span>,
    },
  ]

  // Sorted most expensive first — the point of the table is to see where the money
  // goes, and a table sorted by name buries it.
  const sortedRows = [...rows].sort((a, b) => b.plannedCost - a.plannedCost)

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Coins size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_rescost_title', { defaultValue: 'Coût par ressource' })}
          </h1>
        </div>

        {/* ── KPI cards ── */}
        <div className="grid gap-4 grid-cols-2 lg:grid-cols-4">
          <Card dense>
            <div className="flex items-start gap-3">
              <Wallet size={18} className="text-primary shrink-0 mt-0.5" />
              <Figure
                label={t('proj_rescost_kpi_total', { defaultValue: 'Coût total planifié' })}
                value={money(totalPlanned)}
                hint={hasActuals
                  ? t('proj_rescost_kpi_total_actual', { defaultValue: 'Réel à ce jour {{v}}', v: money(totalActual) })
                  : undefined}
              />
            </div>
          </Card>
          <Card dense>
            <div className="flex items-start gap-3">
              <Users size={18} className="text-text-secondary shrink-0 mt-0.5" />
              <Figure
                label={t('proj_rescost_kpi_count', { defaultValue: 'Ressources' })}
                value={numberFmt(rows.length, language)}
              />
            </div>
          </Card>
          <Card dense>
            <div className="flex items-start gap-3">
              <Coins size={18} className="text-text-secondary shrink-0 mt-0.5" />
              <Figure
                label={t('proj_rescost_kpi_avg', { defaultValue: 'Coût moyen / ressource' })}
                value={money(avgPerResource)}
              />
            </div>
          </Card>
          <Card dense>
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 text-xs text-text-secondary mb-1.5">
                <PieChart size={13} className="shrink-0" />
                {t('proj_rescost_kpi_mix', { defaultValue: 'Répartition par type' })}
              </div>
              <div className="space-y-1">
                {Object.keys(byKind).map(kind => {
                  const amount = byKind[kind] ?? 0
                  if (amount <= 0) return null
                  const meta = KIND_META[kind]
                  if (!meta) return null
                  const pct = totalPlanned > 0 ? (amount / totalPlanned) * 100 : 0
                  return (
                    <div key={kind} className="flex items-center gap-2 text-[11px]">
                      <span className="w-2 h-2 rounded-full shrink-0" style={{ background: meta.tint }} />
                      <span className="text-text-secondary truncate">{meta.label}</span>
                      <span className="ml-auto tabular-nums text-text-primary">{Math.round(pct)}%</span>
                    </div>
                  )
                })}
                {totalPlanned <= 0 && (
                  <div className="text-[11px] text-text-tertiary">
                    {t('proj_rescost_mix_empty', { defaultValue: 'Aucun coût planifié' })}
                  </div>
                )}
              </div>
            </div>
          </Card>
        </div>

        {/* ── Assumption note ── */}
        <Callout variant="info" icon={<Info size={16} />} t={t}>
          {t('proj_rescost_assumption', {
            defaultValue: 'Coût planifié estimé = heures planifiées × taux standard (jours ouvrés × {{h}} h par jour, hors calendrier détaillé). Les matériels sont valorisés à la quantité saisie ; les ressources « coût » à leur forfait. C’est une estimation lisible, pas une mesure de valeur acquise.',
            h: HOURS_PER_DAY,
          })}
        </Callout>

        {/* ── The table ── */}
        <Card
          flush
          footer={rows.length > 0 ? (
            <div className="flex items-center justify-between gap-4 px-1">
              <span className="text-sm font-medium text-text-primary">
                {t('proj_rescost_total_row', { defaultValue: 'Total' })}
              </span>
              <div className="flex items-center gap-8">
                {hasActuals && (
                  <span className="text-sm text-text-secondary tabular-nums">
                    {t('proj_rescost_total_actual', { defaultValue: 'Réel {{v}}', v: money(totalActual) })}
                  </span>
                )}
                <span className="text-base font-semibold text-text-primary tabular-nums">
                  {money(totalPlanned)}
                </span>
              </div>
            </div>
          ) : undefined}
        >
          <DataTable<CostRow>
            rows={sortedRows}
            columns={columns}
            rowKey={r => r.resource.id}
            minTableWidth={880}
            pageSize={0}
            resizableColumns={false}
            emptyState={
              <EmptyState
                variant="first-use"
                icon={<Coins size={26} />}
                title={t('proj_rescost_empty_title', { defaultValue: 'Aucune ressource à chiffrer' })}
                description={t('proj_rescost_empty', { defaultValue: 'Ajoutez des ressources et affectez-les aux tâches pour estimer leur coût.' })}
                t={t}
              />
            }
            t={t}
          />
        </Card>

      </div>
    </div>
  )
}
