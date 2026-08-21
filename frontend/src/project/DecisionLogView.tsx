import { useEffect, useMemo, useRef, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import type { TFunction } from 'i18next'
import { getDateLocale, useConfirm } from '@kubuno/sdk'
import {
  ArrowUpRight, CheckCircle2, CornerDownRight, GitBranch, HelpCircle, Lightbulb,
  ListTree, Plus, Scale, ShieldAlert, Trash2, User, Waypoints,
} from 'lucide-react'
import {
  Badge, Button, Callout, ConfirmDialog, Dropdown, EmptyState, FloatingWindow,
  Input, Textarea, Tooltip, useIsMobile, type DropdownOption,
} from '@ui'
import { projectsApi, type Decision, type DecisionEdit, type DecisionStatus } from '../api'

// Decision log — the choices a project made, and above all WHY.
//
// A project makes choices that outlive their reason. Six months on nobody
// remembers what was ruled out, nor what the trade-off was; the conclusion
// survives, the reasoning does not. So this screen is built around the
// REASONING, not the verdict: rationale, alternatives and consequences get as
// much room as the decision itself — the alternatives especially, since they
// are the half everyone drops and the half worth the most later.
//
// Two things therefore lead the screen:
//   • the UNEXPLAINED list — decisions marked `decided` with an empty rationale.
//     That is this log's own defect: such an entry cannot be re-examined
//     honestly, so for that decision the log is simply not doing its job.
//   • the SUPERSESSION chain — a replaced decision is history, not a mistake.
//     It stays legible, merely attenuated, and says who replaced it.
//
// The data only carries the chain one way (`supersedes_id` points at the OLD
// decision), so the reverse edge — "who replaced me" — is built here.

const STATUSES: DecisionStatus[] = ['proposed', 'decided', 'superseded', 'rejected']

const STATUS_VARIANT: Record<DecisionStatus, 'warning' | 'success' | 'neutral' | 'danger'> = {
  proposed:   'warning',
  decided:    'success',
  superseded: 'neutral',
  rejected:   'danger',
}

/** Everything a card needs from the screen. Passed as one object so the card
 *  components can live at module level: defined inside the parent they would be
 *  a new component type on every render, remounting the list — and dropping the
 *  edit in progress — each time a query settles. */
interface CardCtx {
  t: TFunction
  canEdit: boolean
  isMobile: boolean
  patch: (id: string, body: DecisionEdit) => void
  askDelete: (d: Decision) => void
  fmtDate: (iso: string) => string
  statusLabel: (s: DecisionStatus) => string
  statusOptions: DropdownOption[]
  stakeholderOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  riskOptions: DropdownOption[]
  /** Every other decision, so one can be named as the one being replaced.
   *  The server refuses `supersedes_id == id`, so self is left out here. */
  supersedesOptions: (id: string) => DropdownOption[]
  /** The reverse edge of `supersedes_id`: which decision replaced this one. */
  replacedBy: Map<string, Decision>
  focusDecision: (id: string) => void
  registerCard: (id: string, el: HTMLDivElement | null) => void
  /** Briefly ringed after a jump, so the eye lands on the right card. */
  flashId: string | null
  onOpenTask?: (taskId: string) => void
  onOpenRisks?: () => void
}

/** A decision is unexplained when it was actually taken and no reason was
 *  written down — the same rule the server applies for `summary.unexplained`. */
const isUnexplained = (d: Decision) => d.status === 'decided' && !d.rationale.trim()

/** One block of the record: a heading, then the prose — read as a paragraph,
 *  edited as a textarea committed on blur (one PATCH per edit, not per key). */
function RecordField({ label, icon, hint, value, placeholder, rows = 3, disabled, missing, missingLabel, onCommit }: {
  label: string
  icon?: React.ReactNode
  hint?: string
  value: string
  placeholder?: string
  rows?: number
  disabled?: boolean
  /** The absence is itself the news — say it in words rather than leave a dash. */
  missing?: boolean
  missingLabel?: string
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="flex items-center gap-1.5 text-sm font-medium text-text-secondary mb-1">
        {icon}
        <span>{label}</span>
      </label>
      {disabled ? (
        value.trim() ? (
          <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">{value}</p>
        ) : (
          <p className="text-sm italic" style={{ color: missing ? 'var(--color-danger)' : 'var(--color-text-tertiary)' }}>
            {missing ? missingLabel : '—'}
          </p>
        )
      ) : (
        <Textarea
          rows={rows}
          className="h-auto min-h-0 resize-y"
          value={draft}
          placeholder={placeholder}
          onChange={e => setDraft(e.target.value)}
          onBlur={() => { if (draft !== value) onCommit(draft) }}
        />
      )}
      {missing && missingLabel && !disabled && (
        <p className="text-xs mt-1" style={{ color: 'var(--color-danger)' }}>{missingLabel}</p>
      )}
      {hint && !missing && <p className="text-xs text-text-tertiary mt-1">{hint}</p>}
    </div>
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
      : tone === 'muted' ? 'var(--color-text-secondary)'
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

/** The two ends of the chain, said in words. The forward edge comes from the
 *  record itself; the backward one is rebuilt from the whole log. */
function ChainBanner({ decision, ctx }: { decision: Decision; ctx: CardCtx }) {
  const { t } = ctx
  const successor = ctx.replacedBy.get(decision.id)
  const replaces = decision.supersedes_id && decision.supersedes_title
    ? { id: decision.supersedes_id, title: decision.supersedes_title }
    : null
  if (!successor && !replaces) return null
  return (
    <div className="px-4 py-2 border-b border-border bg-surface-1 flex flex-col gap-1">
      {successor && (
        <p className="text-sm text-text-secondary flex items-start gap-1.5 flex-wrap">
          <CornerDownRight size={14} className="mt-[3px] shrink-0 text-text-tertiary" />
          <span>
            {t('proj_dec_replaced_by', {
              defaultValue: 'Remplacée par une décision plus récente.',
            })}{' '}
          </span>
          <button
            type="button"
            onClick={() => ctx.focusDecision(successor.id)}
            className="inline-flex items-center gap-1 text-primary hover:underline text-left"
          >
            {successor.code ? `${successor.code} — ${successor.title}` : successor.title}
            <ArrowUpRight size={13} className="shrink-0" />
          </button>
        </p>
      )}
      {replaces && (
        <p className="text-sm text-text-secondary flex items-start gap-1.5 flex-wrap">
          <GitBranch size={14} className="mt-[3px] shrink-0 text-text-tertiary" />
          <span>{t('proj_dec_supersedes', { defaultValue: 'Remplace' })}</span>
          <button
            type="button"
            onClick={() => ctx.focusDecision(replaces.id)}
            className="inline-flex items-center gap-1 text-primary hover:underline text-left"
          >
            {replaces.title}
            <ArrowUpRight size={13} className="shrink-0" />
          </button>
        </p>
      )}
    </div>
  )
}

/** What the decision hangs on: who took it, when, and what it touches. */
function CardMeta({ decision, ctx }: { decision: Decision; ctx: CardCtx }) {
  const { t } = ctx
  const chips: React.ReactNode[] = []
  if (decision.stakeholder_name) {
    chips.push(
      <span key="who" className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <User size={12} className="text-text-tertiary" />
        {t('proj_dec_decided_by', { defaultValue: 'tranchée par {{name}}', name: decision.stakeholder_name })}
      </span>,
    )
  }
  if (decision.task_id && decision.task_name) {
    const taskId = decision.task_id
    chips.push(
      <button
        key="task"
        type="button"
        disabled={!ctx.onOpenTask}
        onClick={() => ctx.onOpenTask?.(taskId)}
        className="inline-flex items-center gap-1 text-xs text-primary hover:underline disabled:no-underline disabled:text-text-tertiary disabled:cursor-default"
      >
        <ListTree size={12} />
        {decision.task_name}
      </button>,
    )
  }
  if (decision.risk_code) {
    chips.push(
      <button
        key="risk"
        type="button"
        disabled={!ctx.onOpenRisks}
        onClick={() => ctx.onOpenRisks?.()}
        className="inline-flex items-center gap-1 text-xs text-primary hover:underline disabled:no-underline disabled:text-text-tertiary disabled:cursor-default"
      >
        <ShieldAlert size={12} />
        {t('proj_dec_from_risk', { defaultValue: 'risque {{code}}', code: decision.risk_code })}
      </button>,
    )
  }
  if (chips.length === 0) return null
  return <div className="flex items-center gap-3 flex-wrap">{chips}</div>
}

/** The attachment row, in edit mode: code, date, and the three links a decision
 *  can carry — including the one it replaces, which the server turns into a
 *  status change on the older record. */
function CardLinks({ decision, ctx }: { decision: Decision; ctx: CardCtx }) {
  const { t } = ctx
  return (
    <div className={ctx.isMobile ? 'space-y-3' : 'grid grid-cols-3 gap-3'}>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t('proj_dec_stakeholder', { defaultValue: 'Tranchée par' })}
        </label>
        <Dropdown
          className="w-full" width="100%" height={36} fontSize={14} focusable
          value={decision.stakeholder_id ?? ''} options={ctx.stakeholderOptions}
          onChange={v => ctx.patch(decision.id, { stakeholder_id: v || null })}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t('proj_dec_task', { defaultValue: 'Lot de travail concerné' })}
        </label>
        <Dropdown
          className="w-full" width="100%" height={36} fontSize={14} focusable
          value={decision.task_id ?? ''} options={ctx.taskOptions}
          onChange={v => ctx.patch(decision.id, { task_id: v || null })}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t('proj_dec_risk', { defaultValue: 'Risque lié' })}
        </label>
        <Dropdown
          className="w-full" width="100%" height={36} fontSize={14} focusable
          value={decision.risk_id ?? ''} options={ctx.riskOptions}
          onChange={v => ctx.patch(decision.id, { risk_id: v || null })}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t('proj_dec_code', { defaultValue: 'Code' })}
        </label>
        <Input
          defaultValue={decision.code}
          key={`code-${decision.code}`}
          placeholder="D-01"
          onBlur={e => { if (e.target.value !== decision.code) ctx.patch(decision.id, { code: e.target.value }) }}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t('proj_dec_decided_on', { defaultValue: 'Date de décision' })}
        </label>
        <Input
          type="date"
          value={decision.decided_on ?? ''}
          onChange={e => ctx.patch(decision.id, { decided_on: e.target.value || null })}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t('proj_dec_supersedes_label', { defaultValue: 'Remplace la décision' })}
        </label>
        <Dropdown
          className="w-full" width="100%" height={36} fontSize={14} focusable
          value={decision.supersedes_id ?? ''} options={ctx.supersedesOptions(decision.id)}
          onChange={v => ctx.patch(decision.id, { supersedes_id: v || null })}
        />
        <p className="text-xs text-text-tertiary mt-1">
          {t('proj_dec_supersedes_hint', { defaultValue: 'L’ancienne décision passera automatiquement au statut « Remplacée ».' })}
        </p>
      </div>
    </div>
  )
}

/** One decision, read as a record rather than a table row: the question, the
 *  answer, the reason, what was ruled out, and what it costs. */
function DecisionCard({ decision, ctx }: { decision: Decision; ctx: CardCtx }) {
  const { t } = ctx
  const superseded = decision.status === 'superseded'
  const unexplained = isUnexplained(decision)
  const flashing = ctx.flashId === decision.id
  return (
    <div
      ref={el => { ctx.registerCard(decision.id, el) }}
      className={`bg-surface-0 border rounded-xl overflow-hidden scroll-mt-4 transition-opacity ${
        superseded ? 'opacity-70 hover:opacity-100 focus-within:opacity-100' : ''}`}
      style={{
        // Replaced decisions are HISTORY, not errors: attenuated, never struck
        // through, and full strength again the moment they are looked at.
        borderColor: flashing ? 'var(--color-primary)' : 'var(--color-border)',
        boxShadow: flashing ? '0 0 0 3px color-mix(in srgb, var(--color-primary) 28%, transparent)' : undefined,
      }}
    >
      <div className="px-4 py-3 border-b border-border bg-surface-1 flex items-start gap-3 flex-wrap">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap mb-0.5">
            <span className="font-mono text-xs text-text-tertiary">{decision.code || '—'}</span>
            <span className="inline-flex">
              <Badge variant={STATUS_VARIANT[decision.status]} dot>{ctx.statusLabel(decision.status)}</Badge>
            </span>
            {decision.decided_on && (
              <span className="text-xs text-text-tertiary">{ctx.fmtDate(decision.decided_on)}</span>
            )}
            {unexplained && (
              <Tooltip label={t('proj_dec_unexplained_tip', {
                defaultValue: 'Décision prise sans raisonnement consigné : elle ne pourra pas être réexaminée honnêtement.',
              })}>
                <span className="inline-flex">
                  <Badge variant="danger">{t('proj_dec_unexplained_chip', { defaultValue: 'Sans justification' })}</Badge>
                </span>
              </Tooltip>
            )}
          </div>
          {ctx.canEdit ? (
            <Input
              key={`title-${decision.title}`}
              defaultValue={decision.title}
              className="font-medium"
              onBlur={e => {
                const next = e.target.value.trim()
                if (!next) { e.target.value = decision.title; return }
                if (next !== decision.title) ctx.patch(decision.id, { title: next })
              }}
            />
          ) : (
            <h2 className="text-base font-medium text-text-primary">{decision.title}</h2>
          )}
          <div className="mt-1"><CardMeta decision={decision} ctx={ctx} /></div>
        </div>
        {ctx.canEdit && (
          <div className="flex items-center gap-2 shrink-0">
            <Dropdown
              height={36} fontSize={14} focusable width={168}
              value={decision.status} options={ctx.statusOptions}
              onChange={v => ctx.patch(decision.id, { status: v as DecisionStatus })}
            />
            <button
              type="button"
              onClick={() => ctx.askDelete(decision)}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={15} />
            </button>
          </div>
        )}
      </div>

      <ChainBanner decision={decision} ctx={ctx} />

      {/* Two columns rather than a capped page: a record is prose, and a
          1500-pixel line is unreadable — but leaving half the window empty is
          not the answer either. Left, the chain of reasoning; right, the half
          everyone forgets. */}
      <div className={ctx.isMobile ? 'p-4 space-y-4' : 'p-4 grid grid-cols-2 gap-6 items-start'}>
        <div className="space-y-4 min-w-0">
          <RecordField
            label={t('proj_dec_context', { defaultValue: 'Contexte' })}
            icon={<HelpCircle size={14} className="text-text-tertiary" />}
            hint={t('proj_dec_context_hint', { defaultValue: 'La question qu’il fallait trancher.' })}
            value={decision.context}
            placeholder={t('proj_dec_context_ph', { defaultValue: 'Ce qui a rendu cet arbitrage nécessaire…' })}
            disabled={!ctx.canEdit}
            onCommit={v => ctx.patch(decision.id, { context: v })}
          />
          <RecordField
            label={t('proj_dec_decision', { defaultValue: 'Décision' })}
            icon={<Scale size={14} className="text-text-tertiary" />}
            hint={t('proj_dec_decision_hint', { defaultValue: 'Ce qui a été retenu.' })}
            value={decision.decision}
            placeholder={t('proj_dec_decision_ph', { defaultValue: 'La solution retenue, en une phrase…' })}
            disabled={!ctx.canEdit}
            onCommit={v => ctx.patch(decision.id, { decision: v })}
          />
          <RecordField
            label={t('proj_dec_rationale', { defaultValue: 'Justification' })}
            icon={<Lightbulb size={14} className="text-text-tertiary" />}
            hint={t('proj_dec_rationale_hint', { defaultValue: 'Pourquoi celle-là. Sans cela, la décision ne pourra pas être réexaminée.' })}
            value={decision.rationale}
            placeholder={t('proj_dec_rationale_ph', { defaultValue: 'Les critères qui ont fait pencher la balance…' })}
            rows={4}
            disabled={!ctx.canEdit}
            missing={unexplained}
            missingLabel={t('proj_dec_rationale_missing', {
              defaultValue: 'Aucun raisonnement consigné — cette décision figure dans les manques du journal.',
            })}
            onCommit={v => ctx.patch(decision.id, { rationale: v })}
          />
        </div>

        <div className="space-y-4 min-w-0">
          {/* Given a frame of its own: the alternatives are what makes the log
              worth reading six months later, and a plain extra field would let
              them be skipped. */}
          <div className="rounded-lg border border-border bg-surface-1 p-3">
            <RecordField
              label={t('proj_dec_alternatives', { defaultValue: 'Alternatives écartées' })}
              icon={<Waypoints size={14} className="text-text-tertiary" />}
              hint={t('proj_dec_alternatives_hint', {
                defaultValue: 'Ce qui a été envisagé puis écarté, et pour quelle raison. C’est la moitié qu’on oublie — et celle qui vaut le plus dans six mois.',
              })}
              value={decision.alternatives}
              placeholder={t('proj_dec_alternatives_ph', { defaultValue: 'Option A — écartée parce que…\nOption B — écartée parce que…' })}
              rows={5}
              disabled={!ctx.canEdit}
              onCommit={v => ctx.patch(decision.id, { alternatives: v })}
            />
          </div>
          <RecordField
            label={t('proj_dec_consequences', { defaultValue: 'Conséquences' })}
            icon={<CornerDownRight size={14} className="text-text-tertiary" />}
            hint={t('proj_dec_consequences_hint', { defaultValue: 'Ce que la décision entraîne — y compris ce qu’elle coûte.' })}
            value={decision.consequences}
            placeholder={t('proj_dec_consequences_ph', { defaultValue: 'Ce qu’il faudra assumer, refaire ou surveiller…' })}
            rows={4}
            disabled={!ctx.canEdit}
            onCommit={v => ctx.patch(decision.id, { consequences: v })}
          />
        </div>
      </div>

      {ctx.canEdit && (
        <div className="px-4 pb-4">
          <CardLinks decision={decision} ctx={ctx} />
        </div>
      )}
    </div>
  )
}

export default function DecisionLogView({ projectId, canEdit = true, onOpenTask, onOpenRisks }: {
  projectId: string
  /** False in the mobile reading mode, where the log is read, not kept. */
  canEdit?: boolean
  /** Opens the work package a decision is attached to. */
  onOpenTask?: (taskId: string) => void
  /** Jumps to the risk register — decisions are often answers to a risk. */
  onOpenRisks?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const [statusFilter, setStatusFilter] = useState<string>('')
  const [flashId, setFlashId] = useState<string | null>(null)
  const [draft, setDraft] = useState<{ title: string; context: string; status: DecisionStatus; rationale: string }>(
    { title: '', context: '', status: 'proposed', rationale: '' })

  const cardRefs = useRef(new Map<string, HTMLDivElement>())
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => { if (flashTimer.current) clearTimeout(flashTimer.current) }, [])

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['decisions', projectId],
    queryFn: () => projectsApi.getDecisions(projectId),
  })
  // Who can be named as having settled the question. Shared cache key with the
  // stakeholder register, so both screens read one entry.
  const { data: holders } = useQuery({
    queryKey: ['stakeholders', projectId],
    queryFn: () => projectsApi.getStakeholders(projectId),
  })
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })
  const { data: risks } = useQuery({
    queryKey: ['risks', projectId],
    queryFn: () => projectsApi.getRisks(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['decisions', projectId] })
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const done = () => { setErrorMsg(null); inval() }

  const createMut = useMutation({
    mutationFn: (payload: DecisionEdit & { title: string }) => projectsApi.createDecision(projectId, payload),
    onSuccess: decision => {
      done()
      setCreateOpen(false)
      setDraft({ title: '', context: '', status: 'proposed', rationale: '' })
      focusDecision(decision.id)
    },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, body }: { id: string; body: DecisionEdit }) => projectsApi.updateDecision(projectId, id, body),
    onSuccess: done,
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteDecision(projectId, id),
    onSuccess: done,
    onError: fail,
  })

  const patch = (id: string, body: DecisionEdit) => updateMut.mutate({ id, body })

  const decisions = useMemo(() => data?.decisions ?? [], [data])
  const summary = data?.summary
  const unexplained = data?.unexplained ?? []

  /** The reverse edge of the chain. The record only knows which decision it
   *  replaces; the older one has to be told who replaced it, or it would read
   *  as merely "superseded" by nobody in particular. */
  const replacedBy = useMemo(() => {
    const map = new Map<string, Decision>()
    for (const d of decisions) {
      if (d.supersedes_id) map.set(d.supersedes_id, d)
    }
    return map
  }, [decisions])

  const statusLabel = (s: DecisionStatus) => ({
    proposed:   t('proj_dec_status_proposed', { defaultValue: 'Proposée' }),
    decided:    t('proj_dec_status_decided', { defaultValue: 'Décidée' }),
    superseded: t('proj_dec_status_superseded', { defaultValue: 'Remplacée' }),
    rejected:   t('proj_dec_status_rejected', { defaultValue: 'Rejetée' }),
  })[s]

  const statusOptions: DropdownOption[] = useMemo(() => STATUSES.map(s => ({
    value: s,
    label: {
      proposed:   t('proj_dec_status_proposed', { defaultValue: 'Proposée' }),
      decided:    t('proj_dec_status_decided', { defaultValue: 'Décidée' }),
      superseded: t('proj_dec_status_superseded', { defaultValue: 'Remplacée' }),
      rejected:   t('proj_dec_status_rejected', { defaultValue: 'Rejetée' }),
    }[s],
  })), [t])

  const filterOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_dec_filter_all', { defaultValue: 'Tous les statuts' }) },
    ...statusOptions,
  ], [statusOptions, t])

  const stakeholderOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_dec_no_stakeholder', { defaultValue: '— Non précisé —' }) },
    ...(holders?.stakeholders ?? []).map(s => ({
      value: s.id,
      label: s.role_title ? `${s.name} — ${s.role_title}` : s.name,
    })),
  ], [holders, t])

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_dec_no_task', { defaultValue: '— Aucun lot —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])

  const riskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_dec_no_risk', { defaultValue: '— Aucun risque —' }) },
    ...(risks?.risks ?? []).map(r => ({ value: r.id, label: `${r.code} ${r.title}`.trim() })),
  ], [risks, t])

  /** Every other decision. Self is excluded because the server refuses it —
   *  « Une décision ne peut pas se remplacer elle-même. » */
  const supersedesOptions = useMemo(() => {
    const base = decisions.map(d => ({ value: d.id, label: d.code ? `${d.code} — ${d.title}` : d.title }))
    const none = { value: '', label: t('proj_dec_no_supersedes', { defaultValue: '— Ne remplace rien —' }) }
    return (id: string): DropdownOption[] => [none, ...base.filter(o => o.value !== id)]
  }, [decisions, t])

  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })

  const registerCard = (id: string, el: HTMLDivElement | null) => {
    if (el) cardRefs.current.set(id, el); else cardRefs.current.delete(id)
  }

  /** Jumps to a record and rings it briefly — a silent scroll leaves the reader
   *  hunting for what just moved. */
  const focusDecision = (id: string) => {
    // Clearing the filter first: the target may well be a replaced decision the
    // current filter hides, and jumping to nothing is worse than not jumping.
    setStatusFilter('')
    setFlashId(id)
    if (flashTimer.current) clearTimeout(flashTimer.current)
    flashTimer.current = setTimeout(() => setFlashId(null), 2200)
    requestAnimationFrame(() => {
      cardRefs.current.get(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })
  }

  const askDelete = (d: Decision) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_dec_delete_title', { defaultValue: 'Supprimer la décision ?' }),
        message: t('proj_dec_delete_msg', {
          defaultValue: '« {{title}} » disparaîtra du journal, avec son raisonnement et ses alternatives. Une décision dépassée se remplace ; elle ne s’efface pas.',
          title: d.title,
        }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      })
      if (ok) deleteMut.mutate(d.id)
    })()
  }

  const canCreate = draft.title.trim().length > 0

  const submitCreate = () => {
    const title = draft.title.trim()
    if (!title) return
    void (async () => {
      // The server accepts a decided entry without a reason — and it lands
      // straight in the gap list. Saying so now, while it costs one sentence to
      // fix, beats discovering it in six months.
      if (draft.status === 'decided' && !draft.rationale.trim()) {
        const ok = await confirm({
          title: t('proj_dec_warn_title', { defaultValue: 'Enregistrer sans justification ?' }),
          message: t('proj_dec_warn_msg', {
            defaultValue: 'Cette décision sera consignée comme prise, mais sans le raisonnement qui l’a motivée : elle apparaîtra dans les décisions à justifier et ne pourra pas être réexaminée honnêtement plus tard.',
          }),
          confirmLabel: t('proj_dec_warn_confirm', { defaultValue: 'Enregistrer quand même' }),
          cancelLabel: t('proj_dec_warn_cancel', { defaultValue: 'Compléter d’abord' }),
          variant: 'warning',
        })
        if (!ok) return
      }
      createMut.mutate({
        title,
        context: draft.context.trim(),
        status: draft.status,
        rationale: draft.rationale.trim(),
      })
    })()
  }

  const ctx: CardCtx = {
    t, canEdit, isMobile, patch, askDelete, fmtDate, statusLabel,
    statusOptions, stakeholderOptions, taskOptions, riskOptions, supersedesOptions,
    replacedBy, focusDecision, registerCard, flashId, onOpenTask, onOpenRisks,
  }

  const visible = statusFilter ? decisions.filter(d => d.status === statusFilter) : decisions

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <GitBranch size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_dec_title', { defaultValue: 'Journal des décisions' })}
          </h1>
          <div className="flex-1" />
          {decisions.length > 0 && (
            <Dropdown
              height={36} fontSize={14} focusable width={180}
              value={statusFilter} options={filterOptions}
              onChange={setStatusFilter}
            />
          )}
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_dec_new', { defaultValue: 'Nouvelle décision' })}
            </Button>
          )}
        </div>

        {summary && summary.total > 0 && (
          <div className={isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
            <Stat label={t('proj_dec_stat_total', { defaultValue: 'Décisions' })} value={summary.total} />
            <Stat label={t('proj_dec_stat_pending', { defaultValue: 'En attente' })} value={summary.pending}
              tone={summary.pending > 0 ? undefined : 'muted'} />
            <Stat label={t('proj_dec_stat_decided', { defaultValue: 'Tranchées' })} value={summary.decided} />
            <Stat label={t('proj_dec_stat_superseded', { defaultValue: 'Remplacées' })} value={summary.superseded} tone="muted"
              icon={<GitBranch size={14} />} />
          </div>
        )}

        {/* The log's own defect, first — before the log itself. A decision
            recorded without its reasoning is an entry the journal cannot serve. */}
        {summary && summary.total > 0 && (
          unexplained.length > 0 ? (
            <Callout
              variant="warning"
              title={t('proj_dec_gaps_title', {
                defaultValue: '{{n}} décision(s) prise(s) sans justification',
                n: unexplained.length,
              })}
            >
              <div className="space-y-2">
                <p className="text-sm">
                  {t('proj_dec_gaps_msg', {
                    defaultValue: 'Ces décisions ont été actées, mais le raisonnement qui les a motivées n’a pas été consigné. Dans six mois, personne ne pourra dire si elles tiennent encore : il ne restera que la conclusion. Complétez-les tant que la raison est encore fraîche.',
                  })}
                </p>
                <div className="flex flex-wrap gap-2">
                  {unexplained.map(u => (
                    <button
                      key={u.id}
                      type="button"
                      onClick={() => focusDecision(u.id)}
                      className="inline-flex items-center gap-1.5 text-sm px-2 py-1 rounded border border-border bg-surface-0 hover:bg-surface-2 text-text-primary"
                    >
                      {u.code && <span className="font-mono text-xs text-text-tertiary">{u.code}</span>}
                      <span className="truncate max-w-[22rem]">{u.title}</span>
                      <ArrowUpRight size={13} className="text-text-tertiary shrink-0" />
                    </button>
                  ))}
                </div>
              </div>
            </Callout>
          ) : (
            <Callout
              variant="success"
              icon={<CheckCircle2 size={16} />}
              title={t('proj_dec_gaps_none_title', { defaultValue: 'Chaque décision prise porte sa justification' })}
            >
              <span className="text-sm">
                {t('proj_dec_gaps_none_msg', {
                  defaultValue: 'Aucune décision actée sans raisonnement : le journal reste relisible, et chaque choix pourra être réexaminé sur ses raisons plutôt que défendu par habitude.',
                })}
              </span>
            </Callout>
          )
        )}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {isError ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="error"
              icon={<GitBranch size={26} />}
              title={t('proj_dec_load_error', { defaultValue: 'Le journal n’a pas pu être chargé.' })}
              action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
              t={t}
            />
          </div>
        ) : isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-8 text-center text-sm text-text-secondary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : decisions.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="first-use"
              icon={<Scale size={26} />}
              title={t('proj_dec_empty_title', { defaultValue: 'Aucune décision consignée' })}
              description={t('proj_dec_empty', {
                defaultValue: 'Un projet fait des choix qui survivent à leur raison : six mois plus tard, plus personne ne se souvient de ce qui avait été écarté, ni pourquoi. Consignez-en une dès qu’un arbitrage est tranché — avec sa justification et les options rejetées, qui sont ce que l’on relira vraiment.',
              })}
              action={canEdit ? {
                label: t('proj_dec_new', { defaultValue: 'Nouvelle décision' }),
                icon: <Plus size={14} />,
                onClick: () => setCreateOpen(true),
              } : undefined}
              t={t}
            />
          </div>
        ) : visible.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="no-results"
              icon={<Scale size={26} />}
              title={t('proj_dec_filter_empty', { defaultValue: 'Aucune décision à ce statut' })}
              action={{
                label: t('proj_dec_filter_clear', { defaultValue: 'Afficher tous les statuts' }),
                onClick: () => setStatusFilter(''),
              }}
              t={t}
            />
          </div>
        ) : (
          // The server's order is kept: re-sorting here would bury exactly what
          // it chose to put on top.
          <div className="space-y-4">
            {visible.map(d => <DecisionCard key={d.id} decision={d} ctx={ctx} />)}
          </div>
        )}
      </div>

      {createOpen && (
        <FloatingWindow
          title={t('proj_dec_new', { defaultValue: 'Nouvelle décision' })}
          icon={<Scale size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={560} defaultHeight={draft.status === 'decided' ? 520 : 400} padding={16} t={t}
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
                {t('proj_dec_col_title', { defaultValue: 'Intitulé' })}
              </label>
              <Input
                autoFocus
                value={draft.title}
                placeholder={t('proj_dec_title_ph', { defaultValue: 'Ce qui est tranché, en une phrase…' })}
                onChange={e => setDraft(d => ({ ...d, title: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); submitCreate() } }}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_dec_context', { defaultValue: 'Contexte' })}
              </label>
              <Textarea
                rows={3}
                className="h-auto min-h-0 resize-y"
                value={draft.context}
                placeholder={t('proj_dec_context_ph', { defaultValue: 'Ce qui a rendu cet arbitrage nécessaire…' })}
                onChange={e => setDraft(d => ({ ...d, context: e.target.value }))}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_dec_status', { defaultValue: 'Statut' })}
              </label>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={draft.status} options={statusOptions}
                onChange={v => setDraft(d => ({ ...d, status: v as DecisionStatus }))}
              />
            </div>
            {draft.status === 'decided' && (
              // Asked for here rather than left to the log: the reason is never
              // as clear as at the moment the choice is made.
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_dec_rationale', { defaultValue: 'Justification' })}
                </label>
                <Textarea
                  rows={3}
                  className="h-auto min-h-0 resize-y"
                  value={draft.rationale}
                  placeholder={t('proj_dec_rationale_ph', { defaultValue: 'Les critères qui ont fait pencher la balance…' })}
                  onChange={e => setDraft(d => ({ ...d, rationale: e.target.value }))}
                />
                <p className="text-xs mt-1" style={{
                  color: draft.rationale.trim() ? 'var(--color-text-tertiary)' : 'var(--color-warning)',
                }}>
                  {draft.rationale.trim()
                    ? t('proj_dec_rationale_hint_short', { defaultValue: 'Les alternatives écartées et les conséquences se renseignent ensuite, dans le journal.' })
                    : t('proj_dec_rationale_warn_inline', { defaultValue: 'Une décision actée sans justification rejoindra les manques du journal.' })}
                </p>
              </div>
            )}
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
