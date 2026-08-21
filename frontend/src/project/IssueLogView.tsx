import { useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import type { TFunction } from 'i18next'
import { getDateLocale, prompt, useConfirm } from '@kubuno/sdk'
import {
  AlertOctagon, AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, Flame,
  ListTree, Plus, ShieldAlert, Trash2,
} from 'lucide-react'
import {
  Button, Input, Textarea, Dropdown, Badge, Callout, EmptyState, FloatingWindow,
  ConfirmDialog, Tooltip, useIsMobile, type DropdownOption,
} from '@ui'
import { projectsApi, type Issue, type IssueEdit, type IssueStatus } from '../api'

// Issue log — what has already gone wrong, and what is being done about it.
//
// Two things drive this screen. The OVERDUE count, because it is the only figure
// that demands an act today: an unresolved issue whose date has passed is a
// promise already broken. And the link back to the RISK register, because an
// issue born of a foreseen risk is the proof the register was worth keeping —
// losing that thread turns two registers into two unrelated lists.
//
// Closing an issue is never a plain status change: the server refuses `closed`
// without a resolution (422), so the resolution is asked for HERE, before the
// request leaves, and both travel in a single PATCH.

const STATUSES: IssueStatus[] = ['open', 'in_progress', 'resolved', 'closed']

const STATUS_VARIANT: Record<IssueStatus, 'warning' | 'primary' | 'success' | 'neutral'> = {
  open:        'warning',
  in_progress: 'primary',
  resolved:    'success',
  closed:      'neutral',
}

/** Severities 1→5. Colour AND filled-segment count both change, so a 5 can never
 *  be mistaken for a 1 at a glance — a bare number could. */
const SEVERITY_COLOR = [
  'var(--color-text-tertiary)',
  'var(--color-primary)',
  'var(--color-warning)',
  'var(--color-danger)',
  'var(--color-danger)',
]
const SEVERITY_HEIGHT = [8, 11, 14, 17, 20]

/** Column template of the desktop list: chevron, code, issue, severity, owner,
 *  due date, status, actions. */
const GRID = '28px 84px minmax(220px,1fr) 128px 176px 152px 168px 96px'

const clampSeverity = (n: number) => Math.min(5, Math.max(1, Math.round(n) || 1))

/** An issue is late when its date has passed and nobody has resolved it — the
 *  same rule the server applies for `summary.overdue`. */
function isOverdue(issue: Issue, today: string): boolean {
  if (!issue.due_date) return false
  if (issue.status === 'resolved' || issue.status === 'closed') return false
  return issue.due_date < today
}

/** Everything a row needs from the screen. Passed as one object so the row
 *  components can live at module level: defined inside the parent they would be
 *  a new component type on every render, remounting the list — and dropping the
 *  edit in progress — each time a query settles. */
interface RowCtx {
  t: TFunction
  canEdit: boolean
  isMobile: boolean
  /** Today as `YYYY-MM-DD`, comparable to a due date. */
  today: string
  expanded: Set<string>
  toggle: (id: string) => void
  patch: (id: string, body: IssueEdit) => void
  askClose: (issue: Issue) => void
  askDelete: (issue: Issue) => void
  setStatus: (issue: Issue, next: IssueStatus) => void
  statusLabel: (s: IssueStatus) => string
  severityLabel: (n: number) => string
  fmtDate: (iso: string) => string
  daysLate: (iso: string) => number
  statusOptions: DropdownOption[]
  severityOptions: DropdownOption[]
  ownerOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  onOpenRisks?: () => void
}

/** The 1→5 severity as a five-bar gauge: readable without being read. Editable
 *  in place — clicking a bar sets that level. */
function SeverityScale({ value, label, disabled, onChange }: {
  value: number
  /** Full wording of the current level, for the tooltip. */
  label: string
  disabled?: boolean
  onChange?: (next: number) => void
}) {
  const level = clampSeverity(value)
  const color = SEVERITY_COLOR[level - 1]
  const editable = !disabled && !!onChange
  return (
    <Tooltip label={label}>
      <div className="flex items-center gap-2 h-9">
        <div className="flex items-end gap-[3px] h-[20px]">
          {SEVERITY_HEIGHT.map((h, i) => {
            const style = { height: h, background: i < level ? color : 'var(--color-surface-2)' }
            return editable
              ? (
                <button
                  key={i}
                  type="button"
                  title={String(i + 1)}
                  onClick={() => onChange(i + 1)}
                  className="w-[7px] rounded-sm block cursor-pointer hover:opacity-70"
                  style={style}
                />
              )
              : <span key={i} className="w-[7px] rounded-sm block" style={style} />
          })}
        </div>
        <span className="text-sm font-semibold tabular-nums" style={{ color }}>{level}</span>
        {level === 5 && <Flame size={13} style={{ color }} />}
      </div>
    </Tooltip>
  )
}

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

/** Field committed on blur — one PATCH per edit, not one per keystroke. */
function CellInput({ value, placeholder, type, disabled, required, className, onCommit }: {
  value: string
  placeholder?: string
  type?: string
  disabled?: boolean
  /** Refuse an empty value and snap back rather than let the server answer 422. */
  required?: boolean
  className?: string
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <Input
      type={type}
      className={className}
      value={draft}
      placeholder={placeholder}
      disabled={disabled}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        if (draft === value) return
        if (required && !draft.trim()) { setDraft(value); return }
        onCommit(draft)
      }}
    />
  )
}

/** Long field of the detail panel, committed on blur. */
function LongField({ label, value, placeholder, rows = 3, disabled, onCommit }: {
  label: string
  value: string
  placeholder?: string
  rows?: number
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="text-sm text-text-secondary mb-1 block">{label}</label>
      <Textarea
        rows={rows}
        className="h-auto min-h-0 resize-y"
        value={draft}
        placeholder={placeholder}
        disabled={disabled}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => { if (draft !== value) onCommit(draft) }}
      />
    </div>
  )
}

/** The mention that keeps the two registers tied together. */
function RiskLink({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  if (!issue.risk_code) return null
  const text = ctx.t('proj_issue_from_risk', { defaultValue: 'issu du risque {{code}}', code: issue.risk_code })
  return (
    <Tooltip label={issue.risk_title || text}>
      <button
        type="button"
        disabled={!ctx.onOpenRisks}
        onClick={() => ctx.onOpenRisks?.()}
        className="inline-flex items-center gap-1 text-[11px] text-primary hover:underline disabled:no-underline disabled:text-text-tertiary disabled:cursor-default"
      >
        <ShieldAlert size={12} />
        {text}
      </button>
    </Tooltip>
  )
}

/** The date cell: being late is the whole point, so it is said, not merely tinted. */
function DueCell({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  const late = isOverdue(issue, ctx.today)
  if (ctx.canEdit) {
    return (
      <div className="flex flex-col gap-0.5 min-w-0">
        <Input
          type="date"
          value={issue.due_date ?? ''}
          className={late ? 'text-danger font-medium' : undefined}
          onChange={e => ctx.patch(issue.id, { due_date: e.target.value || null })}
        />
        {late && issue.due_date && (
          <span className="inline-flex items-center gap-1 text-[11px] text-danger">
            <AlertTriangle size={11} />
            {ctx.t('proj_issue_late_by', { defaultValue: 'en retard de {{n}} j', n: ctx.daysLate(issue.due_date) })}
          </span>
        )}
      </div>
    )
  }
  if (!issue.due_date) return <span className="text-sm text-text-tertiary">—</span>
  return (
    <span className={late ? 'text-sm font-semibold text-danger' : 'text-sm text-text-secondary'}>
      {ctx.fmtDate(issue.due_date)}
    </span>
  )
}

/** The panel under a row: everything the line has no room for. */
function DetailPanel({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  const { t } = ctx
  return (
    <div className="border-t border-border bg-surface-1 px-4 py-4 space-y-3">
      <LongField
        label={t('proj_issue_description', { defaultValue: 'Description' })}
        value={issue.description}
        placeholder={t('proj_issue_description_ph', { defaultValue: 'Ce qui s’est passé, quand, et ce que cela empêche…' })}
        disabled={!ctx.canEdit}
        onCommit={v => ctx.patch(issue.id, { description: v })}
      />
      <LongField
        label={t('proj_issue_resolution', { defaultValue: 'Résolution' })}
        value={issue.resolution}
        placeholder={t('proj_issue_resolution_ph', { defaultValue: 'Ce qui a été fait pour le régler…' })}
        disabled={!ctx.canEdit}
        onCommit={v => ctx.patch(issue.id, { resolution: v })}
      />
      <div className={ctx.isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-3'}>
        <div>
          <label className="text-sm text-text-secondary mb-1 block">
            {t('proj_issue_task', { defaultValue: 'Lot de travail lié' })}
          </label>
          {ctx.canEdit ? (
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={issue.task_id ?? ''} options={ctx.taskOptions}
              onChange={v => ctx.patch(issue.id, { task_id: v || null })}
            />
          ) : (
            <p className="text-sm text-text-primary flex items-center gap-1.5 h-9">
              <ListTree size={14} className="text-text-tertiary" />
              {issue.task_name ?? t('proj_issue_no_task', { defaultValue: '— Aucun lot —' })}
            </p>
          )}
        </div>
        <div>
          <label className="text-sm text-text-secondary mb-1 block">
            {t('proj_issue_code', { defaultValue: 'Code' })}
          </label>
          {ctx.canEdit
            ? <CellInput value={issue.code} placeholder="I-01" onCommit={v => ctx.patch(issue.id, { code: v })} />
            : <p className="text-sm font-mono text-text-tertiary h-9 flex items-center">{issue.code || '—'}</p>}
        </div>
      </div>
      {issue.risk_code && (
        <p className="text-sm text-text-secondary">
          {t('proj_issue_from_risk_full', {
            defaultValue: 'Cet incident est la réalisation du risque {{code}} — {{title}}.',
            code: issue.risk_code, title: issue.risk_title ?? '',
          })}{' '}
          {ctx.onOpenRisks && (
            <button type="button" onClick={() => ctx.onOpenRisks?.()} className="text-primary hover:underline">
              {t('proj_issue_open_risks', { defaultValue: 'Ouvrir le registre des risques' })}
            </button>
          )}
        </p>
      )}
      {issue.resolved_at && (
        <p className="text-xs text-text-tertiary">
          {t('proj_issue_resolved_on', { defaultValue: 'Résolu le {{date}}', date: ctx.fmtDate(issue.resolved_at) })}
        </p>
      )}
    </div>
  )
}

/** Closing and deleting — the two acts a row offers directly. */
function RowActions({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  const { t } = ctx
  if (!ctx.canEdit) return <span />
  return (
    <div className="flex items-center gap-1 justify-end">
      {issue.status !== 'closed' && (
        <Button
          size="sm" variant="text" icon={<CheckCircle2 size={14} />}
          title={t('proj_issue_close_hint', { defaultValue: 'Clore l’incident — la résolution sera demandée si elle manque.' })}
          onClick={() => ctx.askClose(issue)}
        >
          {t('proj_issue_close', { defaultValue: 'Clore' })}
        </Button>
      )}
      <button
        type="button"
        onClick={() => ctx.askDelete(issue)}
        title={t('common_delete', { defaultValue: 'Supprimer' })}
        className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
      >
        <Trash2 size={15} />
      </button>
    </div>
  )
}

function ExpandToggle({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  const open = ctx.expanded.has(issue.id)
  return (
    <button
      type="button"
      onClick={() => ctx.toggle(issue.id)}
      title={open
        ? ctx.t('proj_issue_collapse', { defaultValue: 'Replier le détail' })
        : ctx.t('proj_issue_expand', { defaultValue: 'Voir le détail' })}
      className="p-1 rounded hover:bg-surface-2 text-text-tertiary"
    >
      {open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
    </button>
  )
}

/** Desktop: one grid line per issue, its detail folding out underneath. */
function DesktopRow({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  const { t } = ctx
  return (
    <div className="border-b border-border last:border-b-0">
      <div className="grid items-center gap-2 px-3 py-1.5" style={{ gridTemplateColumns: GRID }}>
        <ExpandToggle issue={issue} ctx={ctx} />
        <span className="font-mono text-xs text-text-tertiary truncate">{issue.code || '—'}</span>
        <div className="min-w-0">
          {ctx.canEdit
            ? <CellInput value={issue.title} required onCommit={v => ctx.patch(issue.id, { title: v.trim() })} />
            : <p className="text-sm text-text-primary truncate">{issue.title}</p>}
          <RiskLink issue={issue} ctx={ctx} />
        </div>
        <SeverityScale
          value={issue.severity}
          label={ctx.severityLabel(issue.severity)}
          disabled={!ctx.canEdit}
          onChange={ctx.canEdit ? n => ctx.patch(issue.id, { severity: n }) : undefined}
        />
        {ctx.canEdit ? (
          <Dropdown
            className="w-full" height={36} fontSize={14} focusable
            value={issue.owner_id ?? ''} options={ctx.ownerOptions}
            onChange={v => ctx.patch(issue.id, { owner_id: v || null })}
          />
        ) : (
          <span className="text-sm text-text-secondary truncate">
            {issue.owner_name ?? t('proj_issue_no_owner_short', { defaultValue: 'Non attribué' })}
          </span>
        )}
        <DueCell issue={issue} ctx={ctx} />
        {ctx.canEdit ? (
          <Dropdown
            className="w-full" height={36} fontSize={14} focusable
            value={issue.status} options={ctx.statusOptions}
            onChange={v => ctx.setStatus(issue, v as IssueStatus)}
          />
        ) : (
          // Tooltip and Badge aside: a Badge does not forward unknown props, so
          // anything wrapping it needs a real element of its own.
          <span className="inline-flex">
            <Badge variant={STATUS_VARIANT[issue.status]} dot>{ctx.statusLabel(issue.status)}</Badge>
          </span>
        )}
        <RowActions issue={issue} ctx={ctx} />
      </div>
      {ctx.expanded.has(issue.id) && <DetailPanel issue={issue} ctx={ctx} />}
    </div>
  )
}

/** Mobile: the line becomes a card — severity and lateness first, the rest
 *  stacked under it. */
function MobileRow({ issue, ctx }: { issue: Issue; ctx: RowCtx }) {
  const { t } = ctx
  const late = isOverdue(issue, ctx.today)
  return (
    <div className="border-b border-border last:border-b-0">
      <div className="px-3 py-2.5 space-y-2">
        <div className="flex items-start gap-2">
          <ExpandToggle issue={issue} ctx={ctx} />
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-0.5">
              <span className="font-mono text-xs text-text-tertiary">{issue.code || '—'}</span>
              <SeverityScale value={issue.severity} label={ctx.severityLabel(issue.severity)} disabled />
            </div>
            {ctx.canEdit
              ? <CellInput value={issue.title} required onCommit={v => ctx.patch(issue.id, { title: v.trim() })} />
              : <p className="text-sm text-text-primary">{issue.title}</p>}
            <RiskLink issue={issue} ctx={ctx} />
          </div>
        </div>
        <div className="flex items-center gap-2 flex-wrap pl-8">
          <span className="inline-flex">
            <Badge variant={STATUS_VARIANT[issue.status]} dot>{ctx.statusLabel(issue.status)}</Badge>
          </span>
          {issue.due_date && (late ? (
            <span className="inline-flex items-center gap-1 text-xs font-semibold text-danger">
              <AlertTriangle size={12} />
              {t('proj_issue_late_since', { defaultValue: 'en retard depuis le {{date}}', date: ctx.fmtDate(issue.due_date) })}
            </span>
          ) : (
            <span className="text-xs text-text-tertiary">
              {t('proj_issue_due_on', { defaultValue: 'échéance {{date}}', date: ctx.fmtDate(issue.due_date) })}
            </span>
          ))}
          <span className="text-xs text-text-tertiary truncate">
            {issue.owner_name ?? t('proj_issue_no_owner_short', { defaultValue: 'Non attribué' })}
          </span>
        </div>
        {ctx.canEdit && (
          <div className="pl-8 grid grid-cols-2 gap-2">
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={issue.status} options={ctx.statusOptions}
              onChange={v => ctx.setStatus(issue, v as IssueStatus)}
            />
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={String(clampSeverity(issue.severity))} options={ctx.severityOptions}
              onChange={v => ctx.patch(issue.id, { severity: Number(v) })}
            />
          </div>
        )}
        <div className="pl-8"><RowActions issue={issue} ctx={ctx} /></div>
      </div>
      {ctx.expanded.has(issue.id) && <DetailPanel issue={issue} ctx={ctx} />}
    </div>
  )
}

export default function IssueLogView({ projectId, canEdit = true, onOpenRisks }: {
  projectId: string
  /** False in the mobile reading mode, where the log is read, not kept. */
  canEdit?: boolean
  /** Jumps to the risk register — the other half of the same story. */
  onOpenRisks?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set())
  const [createOpen, setCreateOpen] = useState(false)
  const [draft, setDraft] = useState<{ title: string; severity: number; due_date: string }>(
    { title: '', severity: 3, due_date: '' })

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['issues', projectId],
    queryFn: () => projectsApi.getIssues(projectId),
  })
  // The breakdown supplies the work packages an issue can be attached to; the
  // key is shared with the WBS screen, so both read one cache entry.
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })
  // Who the issue can be handed to: the project's owner and its collaborators.
  const { data: people } = useQuery({
    queryKey: ['proj-collab', projectId],
    queryFn: () => projectsApi.listCollaborators(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['issues', projectId] })
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const done = () => { setErrorMsg(null); inval() }

  const createMut = useMutation({
    mutationFn: (payload: IssueEdit & { title: string }) => projectsApi.createIssue(projectId, payload),
    onSuccess: () => { done(); setCreateOpen(false); setDraft({ title: '', severity: 3, due_date: '' }) },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, body }: { id: string; body: IssueEdit }) => projectsApi.updateIssue(projectId, id, body),
    onSuccess: done,
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteIssue(projectId, id),
    onSuccess: done,
    onError: fail,
  })

  const patch = (id: string, body: IssueEdit) => updateMut.mutate({ id, body })

  const issues = data?.issues ?? []
  const summary = data?.summary
  // Today, in the reader's timezone, as a string comparable to a due date.
  const today = useMemo(() => format(new Date(), 'yyyy-MM-dd'), [])

  const statusLabel = (s: IssueStatus) => ({
    open:        t('proj_issue_status_open', { defaultValue: 'Ouvert' }),
    in_progress: t('proj_issue_status_in_progress', { defaultValue: 'En cours' }),
    resolved:    t('proj_issue_status_resolved', { defaultValue: 'Résolu' }),
    closed:      t('proj_issue_status_closed', { defaultValue: 'Clos' }),
  })[s]

  const severityLabel = (n: number) => ({
    1: t('proj_issue_sev_1', { defaultValue: 'Gravité 1 — Mineure' }),
    2: t('proj_issue_sev_2', { defaultValue: 'Gravité 2 — Faible' }),
    3: t('proj_issue_sev_3', { defaultValue: 'Gravité 3 — Modérée' }),
    4: t('proj_issue_sev_4', { defaultValue: 'Gravité 4 — Majeure' }),
    5: t('proj_issue_sev_5', { defaultValue: 'Gravité 5 — Critique' }),
  }[clampSeverity(n)] ?? String(n))

  const statusOptions: DropdownOption[] = useMemo(
    () => STATUSES.map(s => ({
      value: s,
      label: {
        open:        t('proj_issue_status_open', { defaultValue: 'Ouvert' }),
        in_progress: t('proj_issue_status_in_progress', { defaultValue: 'En cours' }),
        resolved:    t('proj_issue_status_resolved', { defaultValue: 'Résolu' }),
        closed:      t('proj_issue_status_closed', { defaultValue: 'Clos' }),
      }[s],
    })), [t])

  const severityOptions: DropdownOption[] = useMemo(
    () => [1, 2, 3, 4, 5].map(n => ({
      value: String(n),
      label: {
        1: t('proj_issue_sev_1', { defaultValue: 'Gravité 1 — Mineure' }),
        2: t('proj_issue_sev_2', { defaultValue: 'Gravité 2 — Faible' }),
        3: t('proj_issue_sev_3', { defaultValue: 'Gravité 3 — Modérée' }),
        4: t('proj_issue_sev_4', { defaultValue: 'Gravité 4 — Majeure' }),
        5: t('proj_issue_sev_5', { defaultValue: 'Gravité 5 — Critique' }),
      }[n] ?? String(n),
    })), [t])

  const ownerOptions: DropdownOption[] = useMemo(() => {
    const opts: DropdownOption[] = [{ value: '', label: t('proj_issue_no_owner', { defaultValue: '— Non attribué —' }) }]
    const seen = new Set<string>()
    if (people?.owner) {
      opts.push({ value: people.owner.id, label: people.owner.display_name || people.owner.email })
      seen.add(people.owner.id)
    }
    for (const c of people?.collaborators ?? []) {
      if (seen.has(c.user_id)) continue
      seen.add(c.user_id)
      opts.push({ value: c.user_id, label: c.display_name || c.email })
    }
    return opts
  }, [people, t])

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_issue_no_task', { defaultValue: '— Aucun lot —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])

  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })

  /** Whole days elapsed since a date that has passed. */
  const daysLate = (iso: string) =>
    Math.max(1, Math.round((Date.parse(`${today}T00:00:00`) - Date.parse(`${iso}T00:00:00`)) / 86_400_000))

  const toggle = (id: string) => setExpanded(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  /** Closing an issue: the server refuses `closed` without a resolution, so it
   *  is asked for here and both fields travel in a single request. */
  const askClose = (issue: Issue) => {
    void (async () => {
      let resolution = issue.resolution.trim()
      if (!resolution) {
        const answer = await prompt({
          title: t('proj_issue_close_title', { defaultValue: 'Clore l’incident' }),
          message: t('proj_issue_close_msg', {
            defaultValue: 'Comment « {{title}} » a-t-il été réglé ? Un incident clos sans résolution n’apprend rien à ceux qui le reliront.',
            title: issue.title,
          }),
          placeholder: t('proj_issue_close_ph', { defaultValue: 'Ce qui a été fait, et par qui…' }),
          confirmLabel: t('proj_issue_close', { defaultValue: 'Clore' }),
          multiline: true,
        })
        if (answer === null) return // cancelled
        resolution = answer.trim()
        if (!resolution) return
      }
      patch(issue.id, { status: 'closed', resolution })
    })()
  }

  /** A status picked from the dropdown goes through the same guard: `closed`
   *  without a resolution would come straight back as a 422. */
  const setStatus = (issue: Issue, next: IssueStatus) => {
    if (next === issue.status) return
    if (next === 'closed' && !issue.resolution.trim()) { askClose(issue); return }
    patch(issue.id, { status: next })
  }

  const askDelete = (issue: Issue) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_issue_delete_title', { defaultValue: 'Supprimer l’incident ?' }),
        message: t('proj_issue_delete_msg', {
          defaultValue: '« {{title}} » sera supprimé définitivement du journal. Un incident réglé se clôt ; il ne s’efface pas.',
          title: issue.title,
        }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      })
      if (ok) deleteMut.mutate(issue.id)
    })()
  }

  const submitCreate = () => {
    if (!draft.title.trim()) return
    createMut.mutate({
      title: draft.title.trim(),
      severity: draft.severity,
      due_date: draft.due_date || null,
    })
  }

  const ctx: RowCtx = {
    t, canEdit, isMobile, today, expanded, toggle, patch, askClose, askDelete, setStatus,
    statusLabel, severityLabel, fmtDate, daysLate,
    statusOptions, severityOptions, ownerOptions, taskOptions, onOpenRisks,
  }

  const canCreate = draft.title.trim().length > 0

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <AlertOctagon size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_issue_title', { defaultValue: 'Journal des incidents' })}
          </h1>
          <div className="flex-1" />
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_issue_new', { defaultValue: 'Nouvel incident' })}
            </Button>
          )}
        </div>

        {/* Three figures, of which only one demands anything today. */}
        {summary && summary.total > 0 && (
          <div className="bg-surface-0 border border-border rounded-xl p-5 space-y-3">
            <div className={isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
              <Stat label={t('proj_issue_stat_total', { defaultValue: 'Incidents' })} value={summary.total} />
              <Stat label={t('proj_issue_stat_open', { defaultValue: 'Ouverts' })} value={summary.open}
                tone={summary.open > 0 ? undefined : 'success'} />
              {summary.overdue > 0 ? (
                // The one number that is an instruction, not a statistic.
                <div className="flex-1 min-w-[8rem] rounded-lg border px-3 py-2" style={{ borderColor: 'var(--color-danger)' }}>
                  <div className="flex items-center gap-1.5 text-sm mb-0.5" style={{ color: 'var(--color-danger)' }}>
                    <AlertTriangle size={14} />
                    <span className="truncate">{t('proj_issue_stat_overdue', { defaultValue: 'En retard' })}</span>
                  </div>
                  <p className="text-xl font-semibold" style={{ color: 'var(--color-danger)' }}>{summary.overdue}</p>
                </div>
              ) : (
                // Nothing late is news worth saying, not a zero worth printing.
                <div className="flex-1 min-w-[8rem] rounded-lg bg-surface-1 border border-border px-3 py-2">
                  <div className="flex items-center gap-1.5 text-sm mb-0.5" style={{ color: 'var(--color-success)' }}>
                    <CheckCircle2 size={14} />
                    <span className="truncate">{t('proj_issue_stat_overdue', { defaultValue: 'En retard' })}</span>
                  </div>
                  <p className="text-sm font-medium" style={{ color: 'var(--color-success)' }}>
                    {t('proj_issue_no_overdue', { defaultValue: 'Aucun — tout est dans les temps' })}
                  </p>
                </div>
              )}
            </div>

            {/* The spread of severities: five open trifles and one critical make
                the same total, and never the same situation. */}
            <div className="flex items-center gap-3 flex-wrap">
              <span className="text-sm text-text-secondary">
                {t('proj_issue_by_severity', { defaultValue: 'Par gravité' })}
              </span>
              <div className="flex items-center gap-3 flex-wrap">
                {[5, 4, 3, 2, 1].map(n => {
                  const count = summary.by_severity[String(n)] ?? 0
                  if (!count) return null
                  return (
                    <Tooltip key={n} label={severityLabel(n)}>
                      <span className="inline-flex items-center gap-1.5 text-sm">
                        <span className="w-2.5 h-2.5 rounded-sm" style={{ background: SEVERITY_COLOR[n - 1] }} />
                        <span className="text-text-primary font-medium tabular-nums">{count}</span>
                        <span className="text-text-tertiary">{t('proj_issue_sev_short', { defaultValue: 'G{{n}}', n })}</span>
                      </span>
                    </Tooltip>
                  )
                })}
              </div>
            </div>
          </div>
        )}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {isError ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="error"
              icon={<AlertOctagon size={26} />}
              title={t('proj_issue_load_error', { defaultValue: 'Le journal n’a pas pu être chargé.' })}
              action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
              t={t}
            />
          </div>
        ) : isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-8 text-center text-sm text-text-secondary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : issues.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="first-use"
              icon={<CheckCircle2 size={26} />}
              title={t('proj_issue_empty_title', { defaultValue: 'Aucun incident — bonne nouvelle' })}
              description={t('proj_issue_empty', { defaultValue: 'Le journal recense ce qui a déjà mal tourné : un problème constaté, qui a un responsable, une échéance et une résolution. Tant qu’il reste vide, rien ne bloque le projet — ouvrez-en un dès qu’un obstacle apparaît, plutôt que de le laisser vivre dans une conversation.' })}
              action={canEdit ? {
                label: t('proj_issue_new', { defaultValue: 'Nouvel incident' }),
                icon: <Plus size={14} />,
                onClick: () => setCreateOpen(true),
              } : undefined}
              t={t}
            />
          </div>
        ) : (
          // The list keeps the server's order: still open first, then by falling
          // severity. Re-sorting here would bury exactly what it puts on top.
          <div className="bg-surface-0 border border-border rounded-xl overflow-hidden">
            <div className="overflow-x-auto">
              <div className={isMobile ? '' : 'min-w-[1040px]'}>
                {!isMobile && (
                  <div className="grid items-center gap-2 px-3 py-2 border-b border-border bg-surface-1 text-sm text-text-secondary"
                    style={{ gridTemplateColumns: GRID }}>
                    <span />
                    <span>{t('proj_issue_code', { defaultValue: 'Code' })}</span>
                    <span>{t('proj_issue_col_title', { defaultValue: 'Incident' })}</span>
                    <span>{t('proj_issue_severity', { defaultValue: 'Gravité' })}</span>
                    <span>{t('proj_issue_owner', { defaultValue: 'Responsable' })}</span>
                    <span>{t('proj_issue_due', { defaultValue: 'Échéance' })}</span>
                    <span>{t('proj_issue_status', { defaultValue: 'Statut' })}</span>
                    <span />
                  </div>
                )}
                {issues.map(issue => isMobile
                  ? <MobileRow key={issue.id} issue={issue} ctx={ctx} />
                  : <DesktopRow key={issue.id} issue={issue} ctx={ctx} />)}
              </div>
            </div>
          </div>
        )}
      </div>

      {createOpen && (
        <FloatingWindow
          title={t('proj_issue_new', { defaultValue: 'Nouvel incident' })}
          icon={<AlertOctagon size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={480} defaultHeight={360} padding={16} t={t}
          actions={{
            confirm: {
              label: t('common_create', { defaultValue: 'Créer' }),
              disabled: !canCreate,
              loading: createMut.isPending,
              autoFocus: true,
              onClick: submitCreate,
            },
          }}
        >
          <div className="space-y-3">
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_issue_col_title', { defaultValue: 'Incident' })}
              </label>
              <Input
                autoFocus
                value={draft.title}
                placeholder={t('proj_issue_title_ph', { defaultValue: 'Ce qui bloque, en une phrase…' })}
                onChange={e => setDraft(d => ({ ...d, title: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); submitCreate() } }}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_issue_severity', { defaultValue: 'Gravité' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={String(draft.severity)} options={severityOptions}
                  onChange={v => setDraft(d => ({ ...d, severity: Number(v) }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_issue_due', { defaultValue: 'Échéance' })}
                </label>
                <Input type="date" value={draft.due_date}
                  onChange={e => setDraft(d => ({ ...d, due_date: e.target.value }))} />
              </div>
            </div>
            {/* A new issue is never closed, so no resolution is asked for here —
                it is what closing it will require, later. */}
            <p className="text-xs text-text-tertiary">
              {t('proj_issue_create_hint', { defaultValue: 'La description, le responsable et la résolution se renseignent ensuite, directement dans le journal.' })}
            </p>
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
