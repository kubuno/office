import { useMemo, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  Network, AlertTriangle, Package, ListTree, ShieldCheck, RefreshCw, CheckCircle2, Search,
} from 'lucide-react'
import { Button, Badge, Callout, Checkbox, EmptyState, ProgressBar, Tooltip, useIsMobile } from '@ui'
import { projectsApi, type Requirement, type RequirementPriority } from '../api'
import { PriorityBadge, REQ_PRIORITIES, STATUS_BADGE, useScopeLabels } from './RequirementsView'

// Traceability matrix — the one screen that answers "is anything left hanging?".
//
// It is built around its ORPHANS, not around its rows: a requirement nothing
// realises is a commitment nobody keeps, and a deliverable no requirement
// justifies is work nobody asked for. Both are shown as findings to act on,
// never as a footnote under the table.

/** One number of the summary strip. */
function Stat({ label, value, tone, icon }: {
  label: string
  value: number
  tone?: 'danger' | 'success' | 'muted'
  icon?: React.ReactNode
}) {
  const color = tone === 'danger' ? 'var(--color-danger)'
    : tone === 'success' ? 'var(--color-success)'
      : 'var(--color-text-primary)'
  return (
    <div className="flex-1 min-w-[8rem] rounded-lg bg-surface-1 border border-border px-3 py-2">
      <div className="flex items-center gap-1.5 text-sm text-text-secondary mb-0.5">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <p className="text-xl font-semibold" style={{ color }}>{value}</p>
    </div>
  )
}

/** MoSCoW spread as a single stacked bar — the shape of the commitment. */
function MoscowBar({ byPriority, total, labels }: {
  byPriority: Record<RequirementPriority, number>
  total: number
  labels: Record<RequirementPriority, string>
}) {
  const fill: Record<RequirementPriority, string> = {
    must: 'var(--color-danger)',
    should: 'var(--color-warning)',
    could: 'var(--color-primary)',
    wont: 'var(--color-text-tertiary)',
  }
  return (
    <div>
      <div className="flex h-2.5 rounded-full overflow-hidden bg-surface-2">
        {REQ_PRIORITIES.map(p => {
          const n = byPriority[p] ?? 0
          if (!n || !total) return null
          return (
            <Tooltip key={p} label={`${labels[p]} — ${n}`}>
              <div style={{ width: `${(n / total) * 100}%`, background: fill[p] }} />
            </Tooltip>
          )
        })}
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1 mt-2">
        {REQ_PRIORITIES.map(p => (
          <span key={p} className="flex items-center gap-1.5 text-sm text-text-secondary">
            <span className="h-2.5 w-2.5 rounded-full shrink-0" style={{ background: fill[p] }} />
            <span className={p === 'wont' ? 'line-through' : undefined}>{labels[p]}</span>
            <span className="text-text-primary font-medium">{byPriority[p] ?? 0}</span>
          </span>
        ))}
      </div>
    </div>
  )
}

/** The targets a requirement reaches, split by kind. */
function TargetCell({ items, icon, emptyLabel }: {
  items: string[]
  icon: React.ReactNode
  emptyLabel: string
}) {
  if (items.length === 0) {
    return <span className="text-sm text-text-tertiary italic">{emptyLabel}</span>
  }
  return (
    <ul className="flex flex-wrap gap-1.5">
      {items.map((name, i) => (
        <li key={`${name}-${i}`} className="flex items-center gap-1.5 rounded-full bg-surface-2 px-2.5 py-1 text-sm text-text-primary">
          <span className="text-text-secondary shrink-0">{icon}</span>
          <span className="truncate max-w-[14rem]">{name}</span>
        </li>
      ))}
    </ul>
  )
}

export default function TraceabilityView({ projectId, onOpenRequirement }: {
  projectId: string
  /** Jumps to the requirement in the register — the matrix diagnoses, it does not edit. */
  onOpenRequirement?: (id: string) => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const labels = useScopeLabels()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()
  const [untracedOnly, setUntracedOnly] = useState(false)

  const { data, isLoading, isFetching } = useQuery({
    queryKey: ['traceability', projectId],
    queryFn: () => projectsApi.getTraceability(projectId),
  })

  const rows = useMemo(() => {
    const list = data?.requirements ?? []
    return untracedOnly ? list.filter(r => r.links.length === 0) : list
  }, [data, untracedOnly])

  if (isLoading || !data) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        </div>
      </div>
    )
  }

  const { summary, orphans } = data
  const coverage = summary.total > 0 ? Math.round((summary.traced / summary.total) * 100) : 0
  // High coverage is GOOD here, so the bar cannot use the quota thresholds
  // (`auto` turns red near the top): the colour is pinned from the rate itself.
  const coverageVariant = summary.total === 0 ? 'primary'
    : coverage === 100 ? 'success'
      : coverage >= 75 ? 'primary'
        : coverage >= 40 ? 'warning'
          : 'danger'
  const clean = orphans.requirements.length === 0 && orphans.deliverables.length === 0

  const refresh = () => qc.invalidateQueries({ queryKey: ['traceability', projectId] })

  const th = 'text-left text-sm font-medium text-text-secondary px-3 py-2 whitespace-nowrap'
  const td = 'px-3 py-2 align-top'

  const titleOf = (r: Pick<Requirement, 'code' | 'title'>) =>
    r.code ? `${r.code} · ${r.title}` : r.title

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Network size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('trace_title', { defaultValue: 'Matrice de traçabilité' })}
          </h1>
          <div className="flex-1" />
          <Button size="sm" variant="secondary" icon={<RefreshCw size={14} />} loading={isFetching} onClick={refresh}>
            {t('common_refresh', { defaultValue: 'Actualiser' })}
          </Button>
        </div>

        {summary.total === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              icon={<Network size={26} />}
              variant="first-use"
              title={t('trace_empty_title', { defaultValue: 'Aucune exigence saisie' })}
              description={t('trace_empty_desc', { defaultValue: 'La matrice de traçabilité relie chaque exigence à ce qui la réalise — livrables et lots de travail — et met en évidence les deux angles morts : les exigences que rien ne réalise, et les livrables qu’aucune exigence ne justifie. Saisissez des exigences dans le registre pour la remplir.' })}
              t={t}
            />
          </div>
        ) : (
          <>
            {/* ── Synthèse ── */}
            <div className="bg-surface-0 border border-border rounded-xl p-5 space-y-4">
              <div>
                <div className="flex items-baseline gap-2 mb-1.5 flex-wrap">
                  <span className="text-sm text-text-secondary">
                    {t('trace_coverage', { defaultValue: 'Couverture des exigences' })}
                  </span>
                  <span className="text-2xl font-semibold text-text-primary">{coverage}%</span>
                  <span className="text-sm text-text-tertiary">
                    {t('trace_coverage_detail', {
                      defaultValue: '{{traced}} exigence(s) tracée(s) sur {{total}}',
                      traced: summary.traced, total: summary.total,
                    })}
                  </span>
                </div>
                <ProgressBar value={summary.traced} max={summary.total} variant={coverageVariant} t={t} />
              </div>

              <div className={isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
                <Stat label={t('trace_stat_total', { defaultValue: 'Exigences' })} value={summary.total} />
                <Stat label={t('trace_stat_traced', { defaultValue: 'Tracées' })} value={summary.traced} tone="success" />
                <Stat label={t('trace_stat_untraced', { defaultValue: 'Non tracées' })} value={summary.untraced}
                  tone={summary.untraced > 0 ? 'danger' : 'success'} />
                <Stat label={t('trace_stat_verified', { defaultValue: 'Vérifiées' })} value={summary.verified}
                  icon={<ShieldCheck size={14} />} />
              </div>

              <div>
                <p className="text-sm text-text-secondary mb-1.5">
                  {t('trace_moscow', { defaultValue: 'Répartition MoSCoW' })}
                </p>
                <MoscowBar byPriority={summary.by_priority} total={summary.total} labels={labels.priority} />
              </div>
            </div>

            {/* ── Les angles morts ── */}
            {clean ? (
              <Callout variant="success" icon={<CheckCircle2 size={16} />}
                title={t('trace_clean_title', { defaultValue: 'Couverture complète' })}>
                {t('trace_clean', { defaultValue: 'Chaque exigence est réalisée par au moins un livrable ou un lot de travail, et chaque livrable répond à au moins une exigence. Rien n’est laissé de côté, rien n’est produit sans raison.' })}
              </Callout>
            ) : (
              <div className={isMobile ? 'space-y-4' : 'grid grid-cols-2 gap-4'}>
                {/* Requirements nothing realises. */}
                <div className="bg-surface-0 border border-border rounded-xl p-5">
                  <div className="flex items-center gap-2 mb-1">
                    <AlertTriangle size={17} className="shrink-0" style={{ color: 'var(--color-danger)' }} />
                    <h2 className="text-sm font-semibold text-text-primary">
                      {t('trace_orphan_reqs_title', { defaultValue: 'Exigences que rien ne réalise' })}
                    </h2>
                    <Badge variant={orphans.requirements.length > 0 ? 'danger' : 'success'}>
                      {orphans.requirements.length}
                    </Badge>
                  </div>
                  <p className="text-sm text-text-secondary mb-3">
                    {t('trace_orphan_reqs_desc', { defaultValue: 'Un engagement pris que personne ne tient : aucun livrable ni lot de travail ne leur est rattaché.' })}
                  </p>
                  {orphans.requirements.length === 0 ? (
                    <p className="text-sm text-text-tertiary italic">
                      {t('trace_orphan_reqs_none', { defaultValue: 'Aucune — toutes les exigences sont réalisées.' })}
                    </p>
                  ) : (
                    <ul className="space-y-1.5">
                      {orphans.requirements.map(r => (
                        <li key={r.id}>
                          <button
                            onClick={() => onOpenRequirement?.(r.id)}
                            className="w-full text-left flex items-center gap-2 rounded-lg px-2 py-1.5 hover:bg-surface-1"
                          >
                            <PriorityBadge priority={r.priority} label={labels.priority[r.priority]} />
                            <span className="text-sm text-text-primary truncate flex-1 min-w-0">{titleOf(r)}</span>
                            <Badge variant={STATUS_BADGE[r.status]} size="sm">{labels.status[r.status]}</Badge>
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>

                {/* Deliverables no requirement justifies. */}
                <div className="bg-surface-0 border border-border rounded-xl p-5">
                  <div className="flex items-center gap-2 mb-1">
                    <Package size={17} className="shrink-0" style={{ color: 'var(--color-warning)' }} />
                    <h2 className="text-sm font-semibold text-text-primary">
                      {t('trace_orphan_dels_title', { defaultValue: 'Livrables qu’aucune exigence ne justifie' })}
                    </h2>
                    <Badge variant={orphans.deliverables.length > 0 ? 'warning' : 'success'}>
                      {orphans.deliverables.length}
                    </Badge>
                  </div>
                  <p className="text-sm text-text-secondary mb-3">
                    {t('trace_orphan_dels_desc', { defaultValue: 'Du travail que rien ne demande : soit une exigence manque au registre, soit le livrable est de trop.' })}
                  </p>
                  {orphans.deliverables.length === 0 ? (
                    <p className="text-sm text-text-tertiary italic">
                      {t('trace_orphan_dels_none', { defaultValue: 'Aucun — chaque livrable répond à une exigence.' })}
                    </p>
                  ) : (
                    <ul className="space-y-1.5">
                      {orphans.deliverables.map(d => (
                        <li key={d.id} className="flex items-center gap-2 rounded-lg px-2 py-1.5">
                          <Package size={14} className="text-text-secondary shrink-0" />
                          <span className="text-sm text-text-primary truncate flex-1 min-w-0">
                            {d.code ? `${d.code} · ${d.name}` : d.name}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>
            )}

            {/* ── La matrice ── */}
            <div className="bg-surface-0 border border-border rounded-xl">
              <div className="flex items-center gap-3 px-5 pt-4 pb-3 flex-wrap">
                <h2 className="text-sm font-semibold text-text-primary">
                  {t('trace_matrix', { defaultValue: 'Matrice — ce que chaque exigence atteint' })}
                </h2>
                <div className="flex-1" />
                <Checkbox
                  checked={untracedOnly}
                  onChange={setUntracedOnly}
                  label={t('trace_filter_untraced', { defaultValue: 'Seulement les non tracées' })}
                />
              </div>

              {rows.length === 0 ? (
                <EmptyState
                  icon={<Search size={26} />}
                  variant="no-results"
                  compact
                  title={t('trace_filter_empty', { defaultValue: 'Aucune exigence non tracée' })}
                  description={t('trace_filter_empty_desc', { defaultValue: 'Toutes les exigences sont rattachées à au moins un livrable ou un lot de travail.' })}
                  action={{
                    label: t('trace_show_all', { defaultValue: 'Afficher toute la matrice' }),
                    onClick: () => setUntracedOnly(false),
                  }}
                  t={t}
                />
              ) : (
                // A wide matrix scrolls INSIDE its own box, never past the page edge.
                <div className="overflow-x-auto">
                  <table className="w-full border-collapse" style={{ minWidth: isMobile ? 560 : 900 }}>
                    <thead>
                      <tr>
                        <th className={th} style={{ width: 260 }}>{t('trace_col_req', { defaultValue: 'Exigence' })}</th>
                        <th className={th} style={{ width: 150 }}>{t('trace_col_priority', { defaultValue: 'Priorité' })}</th>
                        <th className={th}>{t('trace_col_deliverables', { defaultValue: 'Livrables' })}</th>
                        <th className={th}>{t('trace_col_tasks', { defaultValue: 'Lots de travail' })}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map(r => {
                        const dels = r.links.filter(l => l.deliverable_id)
                          .map(l => l.deliverable_name ?? t('req_link_deliverable', { defaultValue: 'Livrable' }))
                        const tks = r.links.filter(l => l.task_id)
                          .map(l => l.task_name ?? t('req_link_task', { defaultValue: 'Lot de travail' }))
                        const untraced = r.links.length === 0
                        return (
                          <tr key={r.id} className="border-t border-border hover:bg-surface-1">
                            <td className={td}>
                              <button
                                onClick={() => onOpenRequirement?.(r.id)}
                                className="text-left text-sm text-text-primary hover:underline"
                              >
                                {titleOf(r)}
                              </button>
                              {untraced && (
                                <span className="flex items-center gap-1 mt-1"
                                  style={{ color: 'var(--color-danger)' }}>
                                  <AlertTriangle size={13} className="shrink-0" />
                                  <span className="text-sm">
                                    {t('trace_row_untraced', { defaultValue: 'Rien ne la réalise' })}
                                  </span>
                                </span>
                              )}
                            </td>
                            <td className={td}>
                              <PriorityBadge priority={r.priority} label={labels.priority[r.priority]} />
                            </td>
                            <td className={td}>
                              <TargetCell items={dels} icon={<Package size={13} />}
                                emptyLabel={t('trace_no_deliverable', { defaultValue: 'Aucun livrable' })} />
                            </td>
                            <td className={td}>
                              <TargetCell items={tks} icon={<ListTree size={13} />}
                                emptyLabel={t('trace_no_task', { defaultValue: 'Aucun lot de travail' })} />
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}
