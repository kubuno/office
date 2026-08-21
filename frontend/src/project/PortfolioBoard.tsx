import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { format, parseISO } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { AlertTriangle, FolderKanban, Lock } from 'lucide-react'
import {
  Badge, Button, Callout, DataTable, EmptyState, ProgressBar, Tooltip, useIsMobile,
  type DataTableColumn,
} from '@ui'
import { projectsApi, type PortfolioFlag, type PortfolioProject } from '../api'

/**
 * Portfolio board — every project judged by the SAME measures, side by side.
 *
 * This is not the portfolio tree (`ProjectTree`, which shows the parent/child
 * structure): nothing here is about hierarchy. The point of the screen is the
 * comparison itself. Each figure below is already visible inside its own
 * project; what no single project can show is that it carries six open critical
 * risks while every other one carries none. A project quietly in trouble looks
 * healthy until it is put next to its peers.
 *
 * Deliberately NO overall health score. The server does not compute one either:
 * a single number would say "this project is at 62 %" without ever saying which
 * of the six things is wrong — and the answer ("the change requests have been
 * waiting for a decision for three weeks") is the only actionable part. So the
 * flags are shown named, with their count, exactly as the server raised them.
 */

// ── Flags ────────────────────────────────────────────────────────────────────

interface FlagMeta {
  /** i18n key and the French wording it defaults to. */
  key:   string
  label: string
  /** Why it was raised — shown on hover, since the badge only has room to name it. */
  hintKey: string
  hint:    string
  variant: 'danger' | 'warning'
  /** The figure behind the flag, when the payload carries one. */
  count?: (p: PortfolioProject) => number
}

/** Fixed reading order, so the same flag sits in the same place on every row. */
const FLAG_ORDER: PortfolioFlag[] = [
  'late_work', 'overdue_issues', 'high_risks', 'changes_waiting', 'over_budget', 'past_end_date',
]

const FLAGS: Record<PortfolioFlag, FlagMeta> = {
  late_work: {
    key: 'proj_pf_flag_late_work', label: 'Travail en retard',
    hintKey: 'proj_pf_flag_late_work_hint',
    hint: 'Des tâches ont dépassé leur date de fin sans être terminées.',
    variant: 'danger', count: p => p.tasks.late,
  },
  overdue_issues: {
    key: 'proj_pf_flag_overdue_issues', label: 'Incidents en retard',
    hintKey: 'proj_pf_flag_overdue_issues_hint',
    hint: 'Des incidents ouverts ont dépassé leur échéance de résolution.',
    variant: 'danger', count: p => p.issues.overdue,
  },
  high_risks: {
    key: 'proj_pf_flag_high_risks', label: 'Risques critiques ouverts',
    hintKey: 'proj_pf_flag_high_risks_hint',
    hint: 'Des risques à score élevé sont encore ouverts, donc encore à traiter.',
    variant: 'danger', count: p => p.risks.high,
  },
  changes_waiting: {
    key: 'proj_pf_flag_changes_waiting', label: 'Changements en attente de décision',
    hintKey: 'proj_pf_flag_changes_waiting_hint',
    hint: 'Des demandes de changement attendent qu’on les accepte ou les refuse.',
    variant: 'warning', count: p => p.changes.awaiting,
  },
  over_budget: {
    key: 'proj_pf_flag_over_budget', label: 'Dépenses au-delà du budget',
    hintKey: 'proj_pf_flag_over_budget_hint',
    hint: 'Les dépenses directes déjà engagées dépassent le budget des tâches.',
    variant: 'danger',
  },
  past_end_date: {
    key: 'proj_pf_flag_past_end_date', label: 'Date de fin dépassée',
    hintKey: 'proj_pf_flag_past_end_date_hint',
    hint: 'La date de fin planifiée est passée alors que le projet n’est pas terminé.',
    variant: 'warning',
  },
}

/** Project status, worded as elsewhere in the module. */
const STATUS: Record<string, { key: string; def: string }> = {
  active:    { key: 'proj_pstatus_active',    def: 'Actif' },
  on_hold:   { key: 'proj_pstatus_on_hold',   def: 'En pause' },
  completed: { key: 'proj_pstatus_completed', def: 'Terminé' },
  cancelled: { key: 'proj_pstatus_cancelled', def: 'Annulé' },
}

/**
 * Reading order of the board. Three groups, and nothing finer:
 *   0 — open projects carrying at least one flag (what the screen is opened for),
 *   1 — open projects carrying none,
 *   2 — closed projects, which are an outcome and not a problem: a project that
 *       ended over budget stays over budget forever, and letting it sit at the
 *       top would push a live project out of sight.
 * Inside the first group, more flags first — a plain count of what is displayed,
 * not a hidden score — then alphabetical, so the order is reproducible.
 */
function attentionRank(p: PortfolioProject): number {
  if (p.closure_status === 'closed') return 2
  return p.flags.length > 0 ? 0 : 1
}

/** Single sortable value carrying that order (ascending = attention first). */
function attentionSort(p: PortfolioProject): number {
  return attentionRank(p) * 100 - p.flags.length
}

// ── Small pieces ─────────────────────────────────────────────────────────────

function FlagBadges({ project, t, wrap }: {
  project: PortfolioProject
  t: TFunction<'office'>
  /** Cards let the badges wrap freely; the table column keeps them compact. */
  wrap?: boolean
}) {
  const flags = FLAG_ORDER.filter(f => project.flags.includes(f))
  if (flags.length === 0) {
    return (
      <span className="text-xs text-text-tertiary">
        {project.closure_status === 'closed'
          ? t('proj_pf_flag_none_closed', { defaultValue: 'Clos, rien en suspens' })
          : t('proj_pf_flag_none', { defaultValue: 'Rien à signaler' })}
      </span>
    )
  }
  return (
    <div className={`flex gap-1 ${wrap === false ? '' : 'flex-wrap'}`}>
      {flags.map(f => {
        const meta = FLAGS[f]
        const n = meta.count?.(project) ?? 0
        return (
          // Tooltip clones its child to attach the pointer handlers, so the Badge
          // is wrapped in a span rather than handed over directly.
          <Tooltip key={f} label={t(meta.hintKey, { defaultValue: meta.hint })}>
            <span className="inline-flex">
              <Badge variant={meta.variant} size="sm">
                {t(meta.key, { defaultValue: meta.label })}
                {n > 0 ? ` · ${n}` : ''}
              </Badge>
            </span>
          </Tooltip>
        )
      })}
    </div>
  )
}

function ClosureBadge({ status, t }: { status: PortfolioProject['closure_status']; t: TFunction<'office'> }) {
  if (status === 'open') return null
  return (
    <Badge variant="neutral" size="sm">
      <span className="inline-flex items-center gap-1">
        {status === 'closed' && <Lock size={10} />}
        {status === 'closed'
          ? t('proj_pf_closed', { defaultValue: 'Clos' })
          : t('proj_pf_closing', { defaultValue: 'En clôture' })}
      </span>
    </Badge>
  )
}

/** One measure of a card: a label and its value. */
function CardFigure({ label, value, muted }: { label: string; value: string; muted?: boolean }) {
  return (
    <div className="flex flex-col min-w-0">
      <span className="text-[11px] text-text-tertiary truncate">{label}</span>
      <span className={`text-sm tabular-nums truncate ${muted ? 'text-text-tertiary' : 'text-text-primary'}`}>{value}</span>
    </div>
  )
}

/** Mobile layout: one card per project — a wide table on a phone hides the flags. */
function PortfolioCard({ project, t, money, endLabel, onOpen }: {
  project: PortfolioProject
  t: TFunction<'office'>
  money: (v: number) => string
  endLabel: string
  onOpen?: () => void
}) {
  const p = project
  return (
    <button
      type="button"
      onClick={onOpen}
      disabled={!onOpen}
      className="w-full text-left bg-surface-0 border border-border rounded-xl p-4 flex flex-col gap-3 hover:bg-surface-1 disabled:hover:bg-surface-0"
    >
      <div className="flex items-start gap-2">
        <span className="flex-1 min-w-0 text-sm font-medium text-text-primary truncate">{p.title}</span>
        <ClosureBadge status={p.closure_status} t={t} />
      </div>

      <FlagBadges project={p} t={t} />

      <ProgressBar
        value={p.progress} variant="primary" size="sm" showValue t={t}
        label={t('proj_pf_progress', { defaultValue: 'Avancement' })}
        formatValue={v => `${Math.round(v)} %`}
      />

      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        <CardFigure
          label={t('proj_pf_tasks', { defaultValue: 'Tâches' })}
          value={t('proj_pf_tasks_value', {
            defaultValue: '{{done}} / {{total}} terminées',
            done: p.tasks.done, total: p.tasks.total,
          })}
        />
        <CardFigure
          label={t('proj_pf_deliverables', { defaultValue: 'Livrables acceptés' })}
          value={`${p.deliverables.accepted} / ${p.deliverables.total}`}
        />
        <CardFigure
          label={t('proj_pf_late_critical', { defaultValue: 'En retard / critiques' })}
          value={`${p.tasks.late} / ${p.tasks.critical}`}
          muted={p.tasks.late === 0}
        />
        <CardFigure label={t('proj_pf_end', { defaultValue: 'Fin' })} value={endLabel} />
        <CardFigure label={t('proj_pf_budget', { defaultValue: 'Budget' })} value={money(p.money.budget)} />
        <CardFigure label={t('proj_pf_spent', { defaultValue: 'Dépenses directes' })} value={money(p.money.spent_direct)} />
        <CardFigure label={t('proj_pf_exposure', { defaultValue: 'Exposition' })} value={money(p.money.exposure)} />
      </div>
    </button>
  )
}

// ── Board ────────────────────────────────────────────────────────────────────

export interface PortfolioBoardProps {
  /** Opening a row is the caller's business — this screen only compares. */
  onOpenProject?: (projectId: string) => void
}

export default function PortfolioBoard({ onOpenProject }: PortfolioBoardProps) {
  const { t, i18n } = useTranslation('office')
  const isMobile = useIsMobile()

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: ['portfolio'],
    queryFn:  () => projectsApi.getPortfolio(),
    staleTime: 30_000,
  })

  // No currency is available here: it is configured per project, and the board
  // adds projects that may not share one. Amounts are therefore plain numbers.
  const money = useMemo(() => {
    const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 })
    return (v: number) => nf.format(v)
  }, [i18n.language])

  const dateLocale = useMemo(() => getDateLocale(i18n.language), [i18n.language])
  // `parseISO`, never `new Date('2027-01-30')`: the latter is read as midnight
  // UTC and shows the previous day west of Greenwich.
  const endLabel = useMemo(() => (p: PortfolioProject) =>
    p.end_date ? format(parseISO(p.end_date), 'd MMM yyyy', { locale: dateLocale }) : '—',
  [dateLocale])

  const rows = useMemo(() => {
    const list = [...(data?.projects ?? [])]
    list.sort((a, b) =>
      attentionRank(a) - attentionRank(b) ||
      b.flags.length - a.flags.length ||
      a.title.localeCompare(b.title, i18n.language))
    return list
  }, [data, i18n.language])

  const summary = data?.summary
  const errMessage = isError
    ? ((error as { message?: string } | null)?.message
       ?? t('proj_pf_load_error', { defaultValue: 'Le portefeuille n’a pas pu être chargé.' }))
    : undefined

  const exposureHint = t('proj_pf_exposure_hint', {
    defaultValue: 'Montant engagé sur des contrats où le projet, et non le fournisseur, absorbe un dépassement : la part qui grossira si les estimations se révèlent fausses.',
  })

  // Column widths must add up to at most `minTableWidth`, otherwise the table
  // squeezes them and truncates the flags without saying so.
  const columns: DataTableColumn<PortfolioProject>[] = useMemo(() => [
    {
      id: 'project', required: true, primary: true, width: 260, minWidth: 200,
      header: t('proj_col_name', { defaultValue: 'Nom' }),
      sortValue: p => p.title,
      cell: p => {
        const st = STATUS[p.status]
        return (
          <div className="flex flex-col gap-0.5 min-w-0">
            <span className="truncate text-text-primary">{p.title}</span>
            <span className="flex items-center gap-1.5 text-[11px] text-text-tertiary">
              <span className="truncate">{st ? t(st.key, { defaultValue: st.def }) : p.status}</span>
              <ClosureBadge status={p.closure_status} t={t} />
            </span>
          </div>
        )
      },
    },
    {
      id: 'attention', required: true, width: 360, minWidth: 240,
      header: t('proj_pf_col_attention', { defaultValue: 'Demande de l’attention' }),
      sortValue: attentionSort,
      cell: p => <FlagBadges project={p} t={t} />,
    },
    {
      id: 'progress', width: 150, minWidth: 120,
      header: t('proj_pf_progress', { defaultValue: 'Avancement' }),
      sortValue: p => p.progress,
      cell: p => (
        <ProgressBar
          value={p.progress} variant="primary" size="sm" showValue t={t}
          formatValue={v => `${Math.round(v)} %`}
        />
      ),
    },
    {
      id: 'tasks', width: 150, minWidth: 130,
      header: t('proj_pf_tasks', { defaultValue: 'Tâches' }),
      sortValue: p => p.tasks.total,
      cell: p => (
        <div className="flex flex-col gap-0.5 text-xs tabular-nums">
          <span className="text-text-primary">{p.tasks.done} / {p.tasks.total}</span>
          <span className="text-text-tertiary">
            {t('proj_pf_tasks_detail', {
              defaultValue: '{{late}} en retard · {{critical}} critiques',
              late: p.tasks.late, critical: p.tasks.critical,
            })}
          </span>
        </div>
      ),
    },
    {
      id: 'deliverables', width: 110, minWidth: 90, align: 'right',
      header: t('proj_pf_deliverables', { defaultValue: 'Livrables acceptés' }),
      sortValue: p => p.deliverables.accepted,
      cell: p => (
        <span className="text-xs tabular-nums text-text-secondary">
          {p.deliverables.accepted} / {p.deliverables.total}
        </span>
      ),
    },
    {
      id: 'end', width: 110, minWidth: 90,
      header: t('proj_pf_end', { defaultValue: 'Fin' }),
      sortValue: p => (p.end_date ? parseISO(p.end_date) : null),
      cell: p => <span className="text-xs text-text-secondary">{endLabel(p)}</span>,
    },
    {
      id: 'budget', width: 110, minWidth: 90, align: 'right',
      header: t('proj_pf_budget', { defaultValue: 'Budget' }),
      sortValue: p => p.money.budget,
      cell: p => <span className="text-xs tabular-nums text-text-secondary">{money(p.money.budget)}</span>,
    },
    {
      id: 'spent', width: 110, minWidth: 90, align: 'right',
      header: t('proj_pf_spent', { defaultValue: 'Dépenses directes' }),
      sortValue: p => p.money.spent_direct,
      cell: p => (
        <span className={`text-xs tabular-nums ${p.flags.includes('over_budget') ? 'text-danger' : 'text-text-secondary'}`}>
          {money(p.money.spent_direct)}
        </span>
      ),
    },
    {
      id: 'exposure', width: 120, minWidth: 100, align: 'right',
      headerText: t('proj_pf_exposure', { defaultValue: 'Exposition' }),
      header: (
        <Tooltip label={exposureHint}>
          <span className="underline decoration-dotted underline-offset-2">
            {t('proj_pf_exposure', { defaultValue: 'Exposition' })}
          </span>
        </Tooltip>
      ),
      sortValue: p => p.money.exposure,
      cell: p => <span className="text-xs tabular-nums text-text-secondary">{money(p.money.exposure)}</span>,
    },
  ], [t, money, endLabel, exposureHint])

  const emptyState = (
    <EmptyState
      variant="first-use"
      icon={<FolderKanban size={26} />}
      title={t('proj_pf_empty_title', { defaultValue: 'Aucun projet à comparer' })}
      description={t('proj_pf_empty_desc', {
        defaultValue: 'Ce tableau met les projets côte à côte sur les mêmes mesures. Il se remplira dès qu’un projet vous sera accessible.',
      })}
      t={t}
    />
  )

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Header: the same sentence a person would say out loud reading the board. */}
      <div className="shrink-0 px-4 py-3 border-b border-border">
        <h2 className="text-sm font-semibold text-text-primary">
          {t('proj_pf_title', { defaultValue: 'Tableau de bord du portefeuille' })}
        </h2>
        {summary && (
          <>
            <p className="mt-1 text-xs text-text-secondary">
              {t('proj_pf_summary', {
                defaultValue: '{{total}} projet(s) mesurés de la même façon : {{attention}} demandent de l’attention, {{closed}} sont clos.',
                total: summary.total, attention: summary.needs_attention, closed: summary.closed,
              })}
            </p>
            <p className="mt-0.5 text-xs text-text-tertiary tabular-nums">
              {t('proj_pf_summary_money', {
                defaultValue: 'Budget cumulé {{budget}} · dépenses directes {{spent}}',
                budget: money(summary.budget), spent: money(summary.spent_direct),
              })}
              {' · '}
              <Tooltip label={exposureHint}>
                <span className="underline decoration-dotted underline-offset-2">
                  {t('proj_pf_summary_exposure', {
                    defaultValue: 'exposition {{exposure}}',
                    exposure: money(summary.exposure),
                  })}
                </span>
              </Tooltip>
            </p>
          </>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-4 flex flex-col gap-4">
        {errMessage && (
          <Callout
            variant="danger" t={t}
            action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
          >
            {errMessage}
          </Callout>
        )}

        {/* Why anything was reordered — said once, rather than a legend per row. */}
        {summary && summary.needs_attention > 0 && (
          <Callout variant="warning" t={t} icon={<AlertTriangle size={16} />}>
            {t('proj_pf_attention_note', {
              defaultValue: '{{n}} projet(s) portent au moins un drapeau et sont remontés en tête. Aucun indice de santé n’est calculé : chaque drapeau dit lui-même ce qui ne va pas.',
              n: summary.needs_attention,
            })}
          </Callout>
        )}

        {isMobile ? (
          <div className="flex flex-col gap-3">
            {isLoading && (
              <p className="text-sm text-text-tertiary text-center py-10">
                {t('common_loading', { defaultValue: 'Chargement…' })}
              </p>
            )}
            {!isLoading && !errMessage && rows.length === 0 && emptyState}
            {rows.map(p => (
              <PortfolioCard
                key={p.id} project={p} t={t} money={money} endLabel={endLabel(p)}
                onOpen={onOpenProject ? () => onOpenProject(p.id) : undefined}
              />
            ))}
            {!isLoading && errMessage && rows.length === 0 && (
              <div className="flex justify-center">
                <Button variant="secondary" size="sm" onClick={() => { void refetch() }}>
                  {t('common_retry', { defaultValue: 'Réessayer' })}
                </Button>
              </div>
            )}
          </div>
        ) : (
          <DataTable<PortfolioProject>
            rows={rows}
            columns={columns}
            rowKey={p => p.id}
            loading={isLoading}
            error={errMessage}
            onRetry={() => { void refetch() }}
            // The board is already ordered by what asks for attention; a default
            // sort would silently undo that on first paint.
            defaultSort={null}
            pageSize={0}
            resizableColumns={false}
            layout="table"
            // 260+360+150+150+110+110+110+110+120 = 1480, the sum of the declared
            // widths: below it the table scrolls inside its own box instead of
            // squeezing the columns and truncating the flags without saying so.
            minTableWidth={1480}
            onRowClick={onOpenProject ? p => onOpenProject(p.id) : undefined}
            emptyState={emptyState}
            t={t}
          />
        )}
      </div>
    </div>
  )
}
