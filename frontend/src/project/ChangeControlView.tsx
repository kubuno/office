import { useEffect, useMemo, useRef, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import type { TFunction } from 'i18next'
import { getDateLocale, useConfirm } from '@kubuno/sdk'
import {
  AlertTriangle, ArrowUpRight, CalendarClock, CheckCircle2, ClipboardList, Coins,
  FileSearch, GitCompare, ListTree, Lock, Plus, Scale, ShieldAlert, ShieldQuestion,
  Trash2, User,
} from 'lucide-react'
import {
  Badge, Button, Callout, ConfirmDialog, Dropdown, EmptyState, FloatingWindow,
  Input, Textarea, Tooltip, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type ChangeAssessment, type ChangeCategory, type ChangeKind, type ChangeRequest,
  type ChangeRequestEdit, type ChangeStatus, type ChangeUrgency,
} from '../api'

// Change control — the register of what was asked for, what it would cost, and
// what was decided.
//
// A change request lives through three distinct MOMENTS, and the whole screen is
// built to keep them distinct, because collapsing them is exactly how projects
// lose control of their own scope:
//
//   1. ASKED     — somebody wants something, and says why.
//   2. ASSESSED  — somebody priced it: days, money, and what it does to scope,
//                  risk and quality. Until that has happened the request is a
//                  wish, not a decision anybody can take.
//   3. DECIDED   — the owner rules: approved, partially approved, rejected or
//                  deferred. The server REFUSES to approve an unassessed
//                  request, so this screen never offers the button — it explains
//                  what is missing instead.
//
// Each request is therefore drawn as three numbered bands rather than a row of
// fields: the eye can see at a glance which moment a request is stuck in, and
// the gestures of each moment belong to different people.
//
// The figure that leads the screen is `summary.approved_impact`: the cumulated
// effect of everything that was said yes to. Nobody keeps that number, and it is
// the reason a project stops resembling the plan it is still judged against.

const STATUSES: ChangeStatus[] = [
  'submitted', 'assessing', 'approved', 'partially_approved',
  'rejected', 'deferred', 'implemented', 'withdrawn',
]

const CATEGORIES: ChangeCategory[] = ['scope', 'schedule', 'cost', 'quality', 'resource', 'requirement', 'other']
const KINDS: ChangeKind[] = ['change', 'corrective', 'preventive', 'defect_repair']
const URGENCIES: ChangeUrgency[] = ['low', 'normal', 'high', 'critical']

const STATUS_VARIANT: Record<ChangeStatus, 'default' | 'primary' | 'success' | 'warning' | 'danger' | 'neutral'> = {
  submitted:          'neutral',
  assessing:          'warning',
  approved:           'success',
  partially_approved: 'primary',
  rejected:           'danger',
  deferred:           'warning',
  implemented:        'success',
  withdrawn:          'neutral',
}

const URGENCY_VARIANT: Record<ChangeUrgency, 'neutral' | 'default' | 'warning' | 'danger'> = {
  low: 'neutral', normal: 'default', high: 'warning', critical: 'danger',
}

/** Once the board has ruled, the record is frozen: the server refuses any edit
 *  or delete, because the register has to keep what was actually read. */
const DECIDED: ChangeStatus[] = ['approved', 'partially_approved', 'rejected', 'deferred', 'implemented', 'withdrawn']
const isDecided = (s: ChangeStatus) => DECIDED.includes(s)

/** An approval — the two statuses the server refuses on an unassessed request. */
const isApproval = (s: ChangeStatus) => s === 'approved' || s === 'partially_approved'

/** The server refuses an empty assessment: at least one of the five dimensions
 *  has to say something. Mirrored here so the refusal never has to be suffered. */
const assessmentSaysSomething = (a: {
  days: string; cost: string; scope: string; risk: string; quality: string
}) => a.days.trim() !== '' || a.cost.trim() !== ''
  || a.scope.trim() !== '' || a.risk.trim() !== '' || a.quality.trim() !== ''

/** A decision note is mandatory for a refusal — and for a partial approval,
 *  which must say what was kept and what was not. */
const noteRequired = (s: ChangeStatus) => s === 'rejected' || s === 'partially_approved'

/** Everything a card needs from the screen, passed as one object so the card
 *  components can live at module level: defined inside the parent they would be
 *  a new component type on every render, remounting the list — and dropping the
 *  edit in progress — every time a query settles. */
interface CardCtx {
  t: TFunction
  canEdit: boolean
  isOwner: boolean
  isMobile: boolean
  patch: (id: string, body: ChangeRequestEdit) => void
  askDelete: (c: ChangeRequest) => void
  openAssess: (c: ChangeRequest) => void
  openDecide: (c: ChangeRequest, status: ChangeStatus) => void
  fmtDate: (iso: string) => string
  money: (v: number) => string
  days: (v: number) => string
  statusLabel: (s: ChangeStatus) => string
  categoryLabel: (c: ChangeCategory) => string
  kindLabel: (k: ChangeKind) => string
  urgencyLabel: (u: ChangeUrgency) => string
  categoryOptions: DropdownOption[]
  kindOptions: DropdownOption[]
  urgencyOptions: DropdownOption[]
  stakeholderOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  riskOptions: DropdownOption[]
  registerCard: (id: string, el: HTMLDivElement | null) => void
  /** Briefly ringed after a jump, so the eye lands on the right card. */
  flashId: string | null
  onOpenTask?: (taskId: string) => void
  onOpenBaselines?: () => void
}

// ── Small building blocks ────────────────────────────────────────────────────

/** One block of the record: a heading, then the prose — read as a paragraph,
 *  edited as a textarea committed on blur (one PATCH per edit, not per key). */
function RecordField({ label, icon, hint, value, placeholder, rows = 3, disabled, emptyLabel, onCommit }: {
  label: string
  icon?: React.ReactNode
  hint?: string
  value: string
  placeholder?: string
  rows?: number
  disabled?: boolean
  /** Said in words rather than left as a dash when the absence is the news. */
  emptyLabel?: string
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div className="min-w-0">
      <label className="flex items-center gap-1.5 text-sm font-medium text-text-secondary mb-1">
        {icon}
        <span>{label}</span>
      </label>
      {disabled ? (
        value.trim() ? (
          <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">{value}</p>
        ) : (
          <p className="text-sm italic text-text-tertiary">{emptyLabel ?? '—'}</p>
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
      {hint && <p className="text-xs text-text-tertiary mt-1">{hint}</p>}
    </div>
  )
}

/** One number of the summary strip. */
function Stat({ label, value, tone, icon }: {
  label: string
  value: string | number
  tone?: 'danger' | 'success' | 'warning' | 'muted'
  icon?: React.ReactNode
}) {
  const color = tone === 'danger' ? 'var(--color-danger)'
    : tone === 'success' ? 'var(--color-success)'
      : tone === 'warning' ? 'var(--color-warning)'
        : tone === 'muted' ? 'var(--color-text-secondary)'
          : 'var(--color-text-primary)'
  return (
    <div className="flex-1 min-w-[8rem] rounded-lg bg-surface-1 border border-border px-3 py-2">
      <div className="flex items-center gap-1.5 text-sm text-text-secondary mb-0.5">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <p className="text-xl font-semibold tabular-nums" style={{ color }}>{value}</p>
    </div>
  )
}

/** The header of one of the three moments. The numbered marker is what makes
 *  them read as a sequence rather than as three unrelated panels — and its
 *  colour says whether that moment has happened yet. */
function StepHeader({ index, title, state, subtitle, right }: {
  index: number
  title: string
  /** `done` — it happened. `pending` — it is this request's next move.
   *  `blocked` — it cannot happen yet. `off` — nothing is expected here. */
  state: 'done' | 'pending' | 'blocked' | 'off'
  subtitle?: React.ReactNode
  right?: React.ReactNode
}) {
  const color = state === 'done' ? 'var(--color-success)'
    : state === 'pending' ? 'var(--color-warning)'
      : state === 'blocked' ? 'var(--color-danger)'
        : 'var(--color-text-tertiary)'
  return (
    <div className="flex items-start gap-2.5 mb-2">
      <span
        className="shrink-0 mt-[1px] w-5 h-5 rounded-full inline-flex items-center justify-center text-xs font-medium"
        style={{
          color,
          border: `1px solid ${color}`,
          background: state === 'done'
            ? 'color-mix(in srgb, var(--color-success) 12%, transparent)'
            : 'transparent',
        }}
      >
        {index}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium text-text-primary">{title}</div>
        {subtitle && <div className="text-xs text-text-secondary mt-0.5">{subtitle}</div>}
      </div>
      {right && <div className="shrink-0">{right}</div>}
    </div>
  )
}

/** A read-only figure of the assessment. `null` is NOT zero: nobody priced it. */
function ImpactFigure({ label, value, icon, unpriced }: {
  label: string
  value: string | null
  icon: React.ReactNode
  unpriced: string
}) {
  return (
    <div className="rounded-lg border border-border bg-surface-1 px-3 py-2 min-w-0 flex-1">
      <div className="flex items-center gap-1.5 text-xs text-text-secondary mb-0.5">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      {value === null ? (
        <p className="text-sm italic text-text-tertiary">{unpriced}</p>
      ) : (
        <p className="text-lg font-semibold tabular-nums text-text-primary">{value}</p>
      )}
    </div>
  )
}

// ── The three bands of one request ───────────────────────────────────────────

/** Moment 1: what is being asked for, and why. */
function RequestBand({ change, ctx }: { change: ChangeRequest; ctx: CardCtx }) {
  const { t } = ctx
  const frozen = isDecided(change.status)
  const editable = ctx.canEdit && !frozen
  const chips: React.ReactNode[] = []
  if (change.stakeholder_name) {
    chips.push(
      <span key="who" className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <User size={12} className="text-text-tertiary" />
        {t('proj_chg_requested_by', { defaultValue: 'demandée par {{name}}', name: change.stakeholder_name })}
      </span>,
    )
  }
  if (change.task_id && change.task_name) {
    const taskId = change.task_id
    chips.push(
      <button
        key="task"
        type="button"
        disabled={!ctx.onOpenTask}
        onClick={() => ctx.onOpenTask?.(taskId)}
        className="inline-flex items-center gap-1 text-xs text-primary hover:underline disabled:no-underline disabled:text-text-tertiary disabled:cursor-default"
      >
        <ListTree size={12} />
        {change.task_name}
      </button>,
    )
  }
  if (change.risk_code) {
    chips.push(
      <span key="risk" className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <ShieldAlert size={12} className="text-text-tertiary" />
        {t('proj_chg_from_risk', { defaultValue: 'risque {{code}}', code: change.risk_code })}
      </span>,
    )
  }
  if (change.decision_title) {
    chips.push(
      <span key="dec" className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <Scale size={12} className="text-text-tertiary" />
        {change.decision_title}
      </span>,
    )
  }

  return (
    <div className="px-4 py-3">
      <StepHeader
        index={1}
        state="done"
        title={t('proj_chg_step_requested', { defaultValue: 'Demandée' })}
        subtitle={t('proj_chg_requested_on', {
          defaultValue: 'le {{date}}',
          date: ctx.fmtDate(change.requested_on),
        })}
      />

      {chips.length > 0 && <div className="flex items-center gap-3 flex-wrap mb-3">{chips}</div>}

      <div className={ctx.isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-6 items-start'}>
        <RecordField
          label={t('proj_chg_description', { defaultValue: 'Ce qui est demandé' })}
          icon={<ClipboardList size={14} className="text-text-tertiary" />}
          value={change.description}
          placeholder={t('proj_chg_description_ph', { defaultValue: 'Ce qu’il faudrait changer, concrètement…' })}
          disabled={!editable}
          onCommit={v => ctx.patch(change.id, { description: v })}
        />
        <RecordField
          label={t('proj_chg_justification', { defaultValue: 'Justification' })}
          icon={<ShieldQuestion size={14} className="text-text-tertiary" />}
          hint={t('proj_chg_justification_hint', {
            defaultValue: 'Pourquoi c’est nécessaire. Un changement que personne ne justifie est une préférence.',
          })}
          value={change.justification}
          placeholder={t('proj_chg_justification_ph', { defaultValue: 'Ce qui rend ce changement nécessaire…' })}
          disabled={!editable}
          emptyLabel={t('proj_chg_justification_empty', { defaultValue: 'Aucune justification consignée.' })}
          onCommit={v => ctx.patch(change.id, { justification: v })}
        />
      </div>

      {editable && (
        <div className={ctx.isMobile ? 'space-y-3 mt-3' : 'grid grid-cols-3 gap-3 mt-3'}>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_category', { defaultValue: 'Nature' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={change.category} options={ctx.categoryOptions}
              onChange={v => ctx.patch(change.id, { category: v as ChangeCategory })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_kind', { defaultValue: 'Type d’action' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={change.kind} options={ctx.kindOptions}
              onChange={v => ctx.patch(change.id, { kind: v as ChangeKind })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_urgency', { defaultValue: 'Urgence' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={change.urgency} options={ctx.urgencyOptions}
              onChange={v => ctx.patch(change.id, { urgency: v as ChangeUrgency })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_code', { defaultValue: 'Code' })}
            </label>
            <Input
              key={`code-${change.code}`}
              defaultValue={change.code}
              placeholder="CR-01"
              onBlur={e => { if (e.target.value !== change.code) ctx.patch(change.id, { code: e.target.value }) }}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_requested_on_label', { defaultValue: 'Date de la demande' })}
            </label>
            <Input
              type="date"
              value={change.requested_on.slice(0, 10)}
              onChange={e => { if (e.target.value) ctx.patch(change.id, { requested_on: e.target.value }) }}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_stakeholder', { defaultValue: 'Demandeur' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={change.stakeholder_id ?? ''} options={ctx.stakeholderOptions}
              onChange={v => ctx.patch(change.id, { stakeholder_id: v || null })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_task', { defaultValue: 'Lot de travail concerné' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={change.task_id ?? ''} options={ctx.taskOptions}
              onChange={v => ctx.patch(change.id, { task_id: v || null })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_chg_risk', { defaultValue: 'Risque lié' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={change.risk_id ?? ''} options={ctx.riskOptions}
              onChange={v => ctx.patch(change.id, { risk_id: v || null })}
            />
          </div>
        </div>
      )}
    </div>
  )
}

/** Moment 2: what it would cost. Five dimensions, and `null` said as
 *  "non évalué" — never as a zero the reader would take for a measurement. */
function AssessmentBand({ change, ctx }: { change: ChangeRequest; ctx: CardCtx }) {
  const { t } = ctx
  const assessed = change.assessed_on !== null
  const frozen = isDecided(change.status)
  const unpriced = t('proj_chg_not_assessed_value', { defaultValue: 'non évalué' })

  return (
    <div className="px-4 py-3 border-t border-border" style={{
      background: assessed ? undefined : 'color-mix(in srgb, var(--color-warning) 6%, transparent)',
    }}>
      <StepHeader
        index={2}
        state={assessed ? 'done' : frozen ? 'off' : 'pending'}
        title={t('proj_chg_step_assessed', { defaultValue: 'Évaluée' })}
        subtitle={assessed
          ? t('proj_chg_assessed_on', {
            defaultValue: 'le {{date}}',
            date: change.assessed_on ? ctx.fmtDate(change.assessed_on) : '',
          })
          : t('proj_chg_not_assessed_sub', {
            defaultValue: 'Personne n’a encore chiffré ce que ce changement coûterait.',
          })}
        right={ctx.canEdit && !frozen ? (
          <Button size="sm" variant={assessed ? 'secondary' : 'primary'} icon={<FileSearch size={14} />}
            onClick={() => ctx.openAssess(change)}>
            {assessed
              ? t('proj_chg_reassess', { defaultValue: 'Réévaluer' })
              : t('proj_chg_assess', { defaultValue: 'Évaluer' })}
          </Button>
        ) : undefined}
      />

      {!assessed ? (
        <p className="text-sm text-text-secondary">
          {t('proj_chg_not_assessed_msg', {
            defaultValue: 'Tant que l’effet n’est pas chiffré, cette demande ne peut pas être approuvée : il n’y a rien à approuver, seulement une intention. Chiffrez les jours et le coût, et dites ce que le changement fait au périmètre, au risque et à la qualité.',
          })}
        </p>
      ) : (
        <div className="space-y-3">
          <div className={ctx.isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
            <ImpactFigure
              label={t('proj_chg_impact_days', { defaultValue: 'Effet sur le calendrier' })}
              icon={<CalendarClock size={13} />}
              value={change.impact_days === null ? null : ctx.days(change.impact_days)}
              unpriced={unpriced}
            />
            <ImpactFigure
              label={t('proj_chg_impact_cost', { defaultValue: 'Effet sur le budget' })}
              icon={<Coins size={13} />}
              value={change.impact_cost === null ? null : ctx.money(change.impact_cost)}
              unpriced={unpriced}
            />
          </div>
          <div className={ctx.isMobile ? 'space-y-3' : 'grid grid-cols-3 gap-6 items-start'}>
            <RecordField
              label={t('proj_chg_impact_scope', { defaultValue: 'Périmètre' })}
              value={change.impact_scope}
              rows={3}
              disabled
              emptyLabel={t('proj_chg_dimension_empty', { defaultValue: 'Rien de consigné.' })}
              onCommit={() => {}}
            />
            <RecordField
              label={t('proj_chg_impact_risk', { defaultValue: 'Risque' })}
              value={change.impact_risk}
              rows={3}
              disabled
              emptyLabel={t('proj_chg_dimension_empty', { defaultValue: 'Rien de consigné.' })}
              onCommit={() => {}}
            />
            <RecordField
              label={t('proj_chg_impact_quality', { defaultValue: 'Qualité' })}
              value={change.impact_quality}
              rows={3}
              disabled
              emptyLabel={t('proj_chg_dimension_empty', { defaultValue: 'Rien de consigné.' })}
              onCommit={() => {}}
            />
          </div>
        </div>
      )}
    </div>
  )
}

/** Moment 3: the ruling. Reserved to the owner, and — for an approval — barred
 *  until the assessment exists. The bar is explained, not merely enforced. */
function DecisionBand({ change, ctx }: { change: ChangeRequest; ctx: CardCtx }) {
  const { t } = ctx
  const decided = isDecided(change.status)
  const assessed = change.assessed_on !== null

  return (
    <div className="px-4 py-3 border-t border-border bg-surface-1">
      <StepHeader
        index={3}
        state={decided ? 'done' : assessed ? 'pending' : 'blocked'}
        title={t('proj_chg_step_decided', { defaultValue: 'Tranchée' })}
        subtitle={decided
          ? t('proj_chg_decided_on', {
            defaultValue: 'le {{date}}',
            date: change.decided_on ? ctx.fmtDate(change.decided_on) : '',
          })
          : t('proj_chg_awaiting_decision_sub', { defaultValue: 'En attente de décision.' })}
        right={(
          <span className="inline-flex">
            <Badge variant={STATUS_VARIANT[change.status]} dot>{ctx.statusLabel(change.status)}</Badge>
          </span>
        )}
      />

      {decided ? (
        <div className="space-y-2">
          {change.decision_note.trim() ? (
            <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">{change.decision_note}</p>
          ) : (
            <p className="text-sm italic text-text-tertiary">
              {t('proj_chg_no_note', { defaultValue: 'Aucun motif consigné.' })}
            </p>
          )}
          {isApproval(change.status) && (
            change.baseline_name ? (
              <p className="text-xs text-text-secondary inline-flex items-center gap-1.5">
                <GitCompare size={12} className="text-text-tertiary" />
                {t('proj_chg_baseline_named', {
                  defaultValue: 'Plan de référence après approbation : {{name}}',
                  name: change.baseline_name,
                })}
              </p>
            ) : (
              <button
                type="button"
                disabled={!ctx.onOpenBaselines}
                onClick={() => ctx.onOpenBaselines?.()}
                className="text-xs inline-flex items-center gap-1.5 text-warning hover:underline disabled:no-underline disabled:cursor-default"
                style={{ color: 'var(--color-warning)' }}
              >
                <AlertTriangle size={12} />
                {t('proj_chg_baseline_missing_card', {
                  defaultValue: 'Approuvée sans qu’aucun plan de référence n’enregistre ce que le plan est devenu.',
                })}
                <ArrowUpRight size={12} />
              </button>
            )
          )}
          <p className="text-xs text-text-tertiary inline-flex items-center gap-1.5">
            <Lock size={12} />
            {t('proj_chg_frozen', {
              defaultValue: 'Tranchée : le registre garde la trace de ce que le comité a lu. Cette demande n’est plus modifiable ni supprimable.',
            })}
          </p>
        </div>
      ) : !ctx.isOwner ? (
        <p className="text-sm text-text-secondary">
          {t('proj_chg_owner_only', {
            defaultValue: 'Seul le responsable du projet peut trancher une demande de changement.',
          })}
        </p>
      ) : (
        <div className="space-y-2">
          {!assessed && (
            <Callout variant="warning" icon={<AlertTriangle size={16} />}>
              <span className="text-sm">
                {t('proj_chg_cannot_approve', {
                  defaultValue: 'Approbation impossible : rien ne dit encore ce que ce changement coûte. Approuver à l’aveugle est précisément ce que la maîtrise des changements existe pour empêcher — évaluez la demande d’abord.',
                })}
              </span>
            </Callout>
          )}
          <div className="flex items-center gap-2 flex-wrap">
            {assessed ? (
              <>
                <Button size="sm" icon={<CheckCircle2 size={14} />}
                  onClick={() => ctx.openDecide(change, 'approved')}>
                  {t('proj_chg_approve', { defaultValue: 'Approuver' })}
                </Button>
                <Button size="sm" variant="secondary"
                  onClick={() => ctx.openDecide(change, 'partially_approved')}>
                  {t('proj_chg_approve_partial', { defaultValue: 'Approuver partiellement' })}
                </Button>
              </>
            ) : (
              <Button size="sm" icon={<FileSearch size={14} />} disabled={!ctx.canEdit}
                onClick={() => ctx.openAssess(change)}>
                {t('proj_chg_assess_now', { defaultValue: 'Évaluer maintenant' })}
              </Button>
            )}
            <Button size="sm" variant="secondary"
              onClick={() => ctx.openDecide(change, 'deferred')}>
              {t('proj_chg_defer', { defaultValue: 'Reporter' })}
            </Button>
            <Button size="sm" variant="secondary"
              onClick={() => ctx.openDecide(change, 'rejected')}>
              {t('proj_chg_reject', { defaultValue: 'Refuser' })}
            </Button>
            <Button size="sm" variant="ghost"
              onClick={() => ctx.openDecide(change, 'withdrawn')}>
              {t('proj_chg_withdraw', { defaultValue: 'Retirer' })}
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

/** One request, read as a record: asked, priced, ruled. */
function ChangeCard({ change, ctx }: { change: ChangeRequest; ctx: CardCtx }) {
  const { t } = ctx
  const frozen = isDecided(change.status)
  const flashing = ctx.flashId === change.id
  return (
    <div
      ref={el => { ctx.registerCard(change.id, el) }}
      className="bg-surface-0 border rounded-xl overflow-hidden scroll-mt-4"
      style={{
        borderColor: flashing ? 'var(--color-primary)' : 'var(--color-border)',
        boxShadow: flashing ? '0 0 0 3px color-mix(in srgb, var(--color-primary) 28%, transparent)' : undefined,
      }}
    >
      <div className="px-4 py-3 border-b border-border bg-surface-1 flex items-start gap-3 flex-wrap">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap mb-0.5">
            <span className="font-mono text-xs text-text-tertiary">{change.code || '—'}</span>
            <span className="inline-flex">
              <Badge variant={STATUS_VARIANT[change.status]} dot>{ctx.statusLabel(change.status)}</Badge>
            </span>
            <span className="inline-flex">
              <Badge variant="neutral">{ctx.categoryLabel(change.category)}</Badge>
            </span>
            {change.kind !== 'change' && (
              <span className="inline-flex">
                <Badge variant="neutral">{ctx.kindLabel(change.kind)}</Badge>
              </span>
            )}
            {change.urgency !== 'normal' && (
              <span className="inline-flex">
                <Badge variant={URGENCY_VARIANT[change.urgency]}>{ctx.urgencyLabel(change.urgency)}</Badge>
              </span>
            )}
          </div>
          {ctx.canEdit && !frozen ? (
            <Input
              key={`title-${change.title}`}
              defaultValue={change.title}
              className="font-medium"
              onBlur={e => {
                const next = e.target.value.trim()
                if (!next) { e.target.value = change.title; return }
                if (next !== change.title) ctx.patch(change.id, { title: next })
              }}
            />
          ) : (
            <h2 className="text-base font-medium text-text-primary">{change.title}</h2>
          )}
        </div>
        {ctx.canEdit && (
          frozen ? (
            <Tooltip label={t('proj_chg_delete_frozen_tip', {
              defaultValue: 'Une demande tranchée ne se supprime pas : le registre doit garder ce que le comité a lu.',
            })}>
              <span className="inline-flex p-1.5 text-text-tertiary opacity-50">
                <Lock size={15} />
              </span>
            </Tooltip>
          ) : (
            <button
              type="button"
              onClick={() => ctx.askDelete(change)}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={15} />
            </button>
          )
        )}
      </div>

      <RequestBand change={change} ctx={ctx} />
      <AssessmentBand change={change} ctx={ctx} />
      <DecisionBand change={change} ctx={ctx} />
    </div>
  )
}

// ── The screen ───────────────────────────────────────────────────────────────

interface AssessDraft { days: string; cost: string; scope: string; risk: string; quality: string }
const EMPTY_ASSESS: AssessDraft = { days: '', cost: '', scope: '', risk: '', quality: '' }

export default function ChangeControlView({
  projectId, canEdit = true, isOwner = false, onOpenTask, onOpenBaselines,
}: {
  projectId: string
  /** False in the mobile reading mode, where the register is read, not kept. */
  canEdit?: boolean
  /** Only the project owner may rule on a request. */
  isOwner?: boolean
  /** Opens the work package a request is attached to. */
  onOpenTask?: (taskId: string) => void
  /** Jumps to the baselines — an approval with no baseline leaves the plan unrecorded. */
  onOpenBaselines?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [statusFilter, setStatusFilter] = useState<string>('')
  const [flashId, setFlashId] = useState<string | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const [createDraft, setCreateDraft] = useState<{
    title: string; description: string; justification: string
    category: ChangeCategory; kind: ChangeKind; urgency: ChangeUrgency
  }>({ title: '', description: '', justification: '', category: 'scope', kind: 'change', urgency: 'normal' })

  const [assessTarget, setAssessTarget] = useState<ChangeRequest | null>(null)
  const [assessDraft, setAssessDraft] = useState<AssessDraft>(EMPTY_ASSESS)

  const [decideTarget, setDecideTarget] = useState<ChangeRequest | null>(null)
  const [decideStatus, setDecideStatus] = useState<ChangeStatus>('approved')
  const [decideNote, setDecideNote] = useState('')
  const [decideBaseline, setDecideBaseline] = useState('')

  const cardRefs = useRef(new Map<string, HTMLDivElement>())
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => { if (flashTimer.current) clearTimeout(flashTimer.current) }, [])

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['changes', projectId],
    queryFn: () => projectsApi.getChanges(projectId),
  })
  // Shared cache keys with the other registers, so both screens read one entry.
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
  const { data: baselines } = useQuery({
    queryKey: ['baselines', projectId],
    queryFn: () => projectsApi.listBaselines(projectId),
  })
  // Read only for its currency: an impact in money is meaningless unsigned.
  const { data: costs } = useQuery({
    queryKey: ['costs', projectId],
    queryFn: () => projectsApi.getCosts(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['changes', projectId] })
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const done = () => { setErrorMsg(null); inval() }

  const createMut = useMutation({
    mutationFn: (payload: ChangeRequestEdit & { title: string }) => projectsApi.createChange(projectId, payload),
    onSuccess: change => {
      done()
      setCreateOpen(false)
      setCreateDraft({ title: '', description: '', justification: '', category: 'scope', kind: 'change', urgency: 'normal' })
      focusChange(change.id)
    },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, body }: { id: string; body: ChangeRequestEdit }) => projectsApi.updateChange(projectId, id, body),
    onSuccess: done,
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteChange(projectId, id),
    onSuccess: done,
    onError: fail,
  })
  const assessMut = useMutation({
    mutationFn: ({ id, body }: { id: string; body: ChangeAssessment }) => projectsApi.assessChange(projectId, id, body),
    onSuccess: change => { done(); setAssessTarget(null); focusChange(change.id) },
    onError: fail,
  })
  const decideMut = useMutation({
    mutationFn: ({ id, body }: {
      id: string
      body: { status: ChangeStatus; decision_note?: string; baseline_id?: string | null }
    }) => projectsApi.decideChange(projectId, id, body),
    onSuccess: change => { done(); setDecideTarget(null); focusChange(change.id) },
    onError: fail,
  })

  const patch = (id: string, body: ChangeRequestEdit) => updateMut.mutate({ id, body })

  const changes = useMemo(() => data?.changes ?? [], [data])
  const summary = data?.summary
  const awaiting = useMemo(() => data?.awaiting ?? [], [data])

  // ── Labels ────────────────────────────────────────────────────────────────

  const statusLabel = (s: ChangeStatus) => ({
    submitted:          t('proj_chg_status_submitted', { defaultValue: 'Demandée' }),
    assessing:          t('proj_chg_status_assessing', { defaultValue: 'Évaluée' }),
    approved:           t('proj_chg_status_approved', { defaultValue: 'Approuvée' }),
    partially_approved: t('proj_chg_status_partial', { defaultValue: 'Partiellement approuvée' }),
    rejected:           t('proj_chg_status_rejected', { defaultValue: 'Refusée' }),
    deferred:           t('proj_chg_status_deferred', { defaultValue: 'Reportée' }),
    implemented:        t('proj_chg_status_implemented', { defaultValue: 'Mise en œuvre' }),
    withdrawn:          t('proj_chg_status_withdrawn', { defaultValue: 'Retirée' }),
  })[s]

  const categoryLabel = (c: ChangeCategory) => ({
    scope:       t('proj_chg_cat_scope', { defaultValue: 'Périmètre' }),
    schedule:    t('proj_chg_cat_schedule', { defaultValue: 'Calendrier' }),
    cost:        t('proj_chg_cat_cost', { defaultValue: 'Coût' }),
    quality:     t('proj_chg_cat_quality', { defaultValue: 'Qualité' }),
    resource:    t('proj_chg_cat_resource', { defaultValue: 'Ressources' }),
    requirement: t('proj_chg_cat_requirement', { defaultValue: 'Exigence' }),
    other:       t('proj_chg_cat_other', { defaultValue: 'Autre' }),
  })[c]

  const kindLabel = (k: ChangeKind) => ({
    change:        t('proj_chg_kind_change', { defaultValue: 'Changement' }),
    corrective:    t('proj_chg_kind_corrective', { defaultValue: 'Action corrective' }),
    preventive:    t('proj_chg_kind_preventive', { defaultValue: 'Action préventive' }),
    defect_repair: t('proj_chg_kind_defect', { defaultValue: 'Réparation de défaut' }),
  })[k]

  const urgencyLabel = (u: ChangeUrgency) => ({
    low:      t('proj_chg_urg_low', { defaultValue: 'Faible' }),
    normal:   t('proj_chg_urg_normal', { defaultValue: 'Normale' }),
    high:     t('proj_chg_urg_high', { defaultValue: 'Élevée' }),
    critical: t('proj_chg_urg_critical', { defaultValue: 'Critique' }),
  })[u]

  // ── Options ───────────────────────────────────────────────────────────────

  const statusOptions: DropdownOption[] = useMemo(
    () => STATUSES.map(s => ({ value: s, label: statusLabel(s) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )
  const filterOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_chg_filter_all', { defaultValue: 'Tous les statuts' }) },
    ...statusOptions,
  ], [statusOptions, t])

  const categoryOptions: DropdownOption[] = useMemo(
    () => CATEGORIES.map(c => ({ value: c, label: categoryLabel(c) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )
  const kindOptions: DropdownOption[] = useMemo(
    () => KINDS.map(k => ({ value: k, label: kindLabel(k) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )
  const urgencyOptions: DropdownOption[] = useMemo(
    () => URGENCIES.map(u => ({ value: u, label: urgencyLabel(u) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )

  const stakeholderOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_chg_no_stakeholder', { defaultValue: '— Non précisé —' }) },
    ...(holders?.stakeholders ?? []).map(s => ({
      value: s.id,
      label: s.role_title ? `${s.name} — ${s.role_title}` : s.name,
    })),
  ], [holders, t])

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_chg_no_task', { defaultValue: '— Aucun lot —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])

  const riskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_chg_no_risk', { defaultValue: '— Aucun risque —' }) },
    ...(risks?.risks ?? []).map(r => ({ value: r.id, label: `${r.code} ${r.title}`.trim() })),
  ], [risks, t])

  const baselineOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_chg_no_baseline', { defaultValue: '— Aucun plan de référence —' }) },
    ...(baselines ?? []).map(b => ({
      value: b.id,
      label: b.is_primary
        ? t('proj_chg_baseline_primary', { defaultValue: '{{name}} (principal)', name: b.name })
        : b.name,
    })),
  ], [baselines, t])

  /** The statuses a ruling can land on. `withdrawn` is here because it is what
   *  the server suggests instead of deleting a request the board has read; the
   *  list is built at the top level of the component — building it inside the
   *  decision window would make it a conditional hook. */
  const decisionOptions: DropdownOption[] = useMemo(() => ([
    { value: 'approved',           label: t('proj_chg_status_approved', { defaultValue: 'Approuvée' }) },
    { value: 'partially_approved', label: t('proj_chg_status_partial', { defaultValue: 'Partiellement approuvée' }) },
    { value: 'rejected',           label: t('proj_chg_status_rejected', { defaultValue: 'Refusée' }) },
    { value: 'deferred',           label: t('proj_chg_status_deferred', { defaultValue: 'Reportée' }) },
    { value: 'withdrawn',          label: t('proj_chg_status_withdrawn', { defaultValue: 'Retirée' }) },
    { value: 'implemented',        label: t('proj_chg_status_implemented', { defaultValue: 'Mise en œuvre' }) },
  ]), [t])

  // ── Formatting ────────────────────────────────────────────────────────────

  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })

  // `Intl` throws on anything that is not an ISO 4217 code, so an exotic currency
  // falls back to a plain number followed by the code as written.
  const money = useMemo(() => {
    const currency = costs?.config.currency ?? ''
    if (currency) {
      try {
        const nf = new Intl.NumberFormat(i18n.language, { style: 'currency', currency, maximumFractionDigits: 0 })
        return (n: number) => nf.format(n)
      } catch { /* not an ISO code — handled below */ }
      const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 })
      return (n: number) => `${nf.format(n)} ${currency}`
    }
    const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 })
    return (n: number) => nf.format(n)
  }, [costs, i18n.language])

  /** Compact, for the figures inside a card. */
  const days = useMemo(() => {
    const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 })
    return (n: number) => t('proj_chg_days', { defaultValue: '{{n}} j', n: nf.format(n) })
  }, [i18n.language, t])

  /** Spelled out, for the headline sentence — which is read as a sentence. */
  const daysLong = useMemo(() => {
    const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 })
    return (n: number) => t('proj_chg_days_long', {
      defaultValue: '{{n}} jours', n: nf.format(n), count: Math.round(Math.abs(n)),
    })
  }, [i18n.language, t])

  // ── Navigation between cards ──────────────────────────────────────────────

  const registerCard = (id: string, el: HTMLDivElement | null) => {
    if (el) cardRefs.current.set(id, el); else cardRefs.current.delete(id)
  }

  /** Jumps to a record and rings it briefly — a silent scroll leaves the reader
   *  hunting for what just moved. */
  const focusChange = (id: string) => {
    // Clearing the filter first: the target may well be hidden by it, and
    // jumping to nothing is worse than not jumping.
    setStatusFilter('')
    setFlashId(id)
    if (flashTimer.current) clearTimeout(flashTimer.current)
    flashTimer.current = setTimeout(() => setFlashId(null), 2200)
    requestAnimationFrame(() => {
      cardRefs.current.get(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })
  }

  // ── Actions ───────────────────────────────────────────────────────────────

  const askDelete = (c: ChangeRequest) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_chg_delete_title', { defaultValue: 'Supprimer la demande ?' }),
        message: t('proj_chg_delete_msg', {
          defaultValue: '« {{title}} » disparaîtra du registre, avec son évaluation. Une demande qui n’a plus lieu d’être se retire ; elle ne s’efface pas.',
          title: c.title,
        }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      })
      if (ok) deleteMut.mutate(c.id)
    })()
  }

  const openAssess = (c: ChangeRequest) => {
    setAssessTarget(c)
    setAssessDraft({
      days:    c.impact_days === null ? '' : String(c.impact_days),
      cost:    c.impact_cost === null ? '' : String(c.impact_cost),
      scope:   c.impact_scope,
      risk:    c.impact_risk,
      quality: c.impact_quality,
    })
  }

  const submitAssess = () => {
    if (!assessTarget || !assessmentSaysSomething(assessDraft)) return
    const num = (s: string) => {
      const trimmed = s.trim()
      if (trimmed === '') return null
      const v = Number(trimmed.replace(',', '.'))
      return Number.isFinite(v) ? v : null
    }
    assessMut.mutate({
      id: assessTarget.id,
      body: {
        impact_days:    num(assessDraft.days),
        impact_cost:    num(assessDraft.cost),
        impact_scope:   assessDraft.scope.trim(),
        impact_risk:    assessDraft.risk.trim(),
        impact_quality: assessDraft.quality.trim(),
      },
    })
  }

  const openDecide = (c: ChangeRequest, status: ChangeStatus) => {
    setDecideTarget(c)
    setDecideStatus(status)
    setDecideNote(c.decision_note)
    setDecideBaseline(c.baseline_id ?? (baselines ?? []).find(b => b.is_primary)?.id ?? '')
  }

  const decideBlocked = isApproval(decideStatus) && decideTarget !== null && decideTarget.assessed_on === null
  const decideNoteMissing = noteRequired(decideStatus) && !decideNote.trim()
  const canDecide = decideTarget !== null && !decideBlocked && !decideNoteMissing

  const submitDecide = () => {
    if (!decideTarget || !canDecide) return
    decideMut.mutate({
      id: decideTarget.id,
      body: {
        status: decideStatus,
        decision_note: decideNote.trim(),
        // Only an approval moves the plan, so only an approval names the
        // baseline the plan moved to.
        baseline_id: isApproval(decideStatus) ? (decideBaseline || null) : null,
      },
    })
  }

  const canCreate = createDraft.title.trim().length > 0
  const submitCreate = () => {
    const title = createDraft.title.trim()
    if (!title) return
    createMut.mutate({
      title,
      description:   createDraft.description.trim(),
      justification: createDraft.justification.trim(),
      category:      createDraft.category,
      kind:          createDraft.kind,
      urgency:       createDraft.urgency,
    })
  }

  // ── The headline figure ───────────────────────────────────────────────────

  /** The sentence nobody can produce otherwise: what saying yes has already
   *  done to the plan the project is still judged against. */
  const impactSentence = useMemo(() => {
    if (!summary) return null
    const { days: d, cost: c } = summary.approved_impact
    if (summary.approved === 0) {
      return t('proj_chg_impact_none', {
        defaultValue: 'Aucun changement approuvé : le projet est encore exactement celui du plan de référence.',
      })
    }
    if (d === 0 && c === 0) {
      return t('proj_chg_impact_zero', {
        defaultValue: 'Depuis le plan de référence, les changements approuvés n’ont ajouté ni jours ni budget — pour autant que leur effet ait été chiffré.',
      })
    }
    if (d >= 0 && c >= 0) {
      return t('proj_chg_impact_added', {
        defaultValue: 'Depuis le plan de référence, les changements approuvés ont ajouté {{days}} et {{cost}}.',
        days: daysLong(d), cost: money(c),
      })
    }
    return t('proj_chg_impact_shift', {
      defaultValue: 'Depuis le plan de référence, les changements approuvés déplacent le plan de {{days}} et {{cost}}.',
      days: daysLong(d), cost: money(c),
    })
  }, [summary, daysLong, money, t])

  const costedSentence = useMemo(() => {
    if (!summary || summary.approved === 0) return null
    const { costed } = summary.approved_impact
    if (costed >= summary.approved) {
      return t('proj_chg_costed_all', {
        defaultValue: 'Le montant porte sur les {{n}} demande(s) approuvée(s).',
        n: summary.approved,
      })
    }
    return t('proj_chg_costed_partial', {
      defaultValue: 'Attention : le montant ne porte que sur {{costed}} demande(s) approuvée(s) sur {{total}}. Les autres n’ont jamais été chiffrées — le total réel est donc plus élevé que celui affiché.',
      costed, total: summary.approved,
    })
  }, [summary, t])

  const costedIncomplete = !!summary && summary.approved > 0
    && summary.approved_impact.costed < summary.approved

  // ── Waiting lists ─────────────────────────────────────────────────────────

  const toAssess = useMemo(() => awaiting.filter(a => !a.assessed), [awaiting])
  const toDecide = useMemo(() => awaiting.filter(a => a.assessed), [awaiting])

  const ctx: CardCtx = {
    t, canEdit, isOwner, isMobile, patch, askDelete, openAssess, openDecide,
    fmtDate, money, days, statusLabel, categoryLabel, kindLabel, urgencyLabel,
    categoryOptions, kindOptions, urgencyOptions,
    stakeholderOptions, taskOptions, riskOptions,
    registerCard, flashId, onOpenTask, onOpenBaselines,
  }

  const visible = statusFilter ? changes.filter(c => c.status === statusFilter) : changes

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <GitCompare size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_chg_title', { defaultValue: 'Demandes de changement' })}
          </h1>
          <div className="flex-1" />
          {changes.length > 0 && (
            <Dropdown
              height={36} fontSize={14} focusable width={200}
              value={statusFilter} options={filterOptions}
              onChange={setStatusFilter}
            />
          )}
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_chg_new', { defaultValue: 'Nouvelle demande' })}
            </Button>
          )}
        </div>

        {/* The figure that leads everything: what saying yes has already cost.
            Given a panel of its own rather than a tile in a row — it is the one
            number that explains why the project no longer looks like its plan. */}
        {summary && summary.total > 0 && impactSentence && (
          <div className="rounded-xl border bg-surface-0 px-4 py-4" style={{
            borderColor: costedIncomplete ? 'var(--color-warning)' : 'var(--color-border)',
          }}>
            <div className="flex items-start gap-3">
              <GitCompare size={18} className="shrink-0 mt-[3px] text-text-tertiary" />
              <div className="min-w-0">
                <p className="text-base font-medium text-text-primary leading-relaxed">{impactSentence}</p>
                {costedSentence && (
                  <p className="text-sm mt-1" style={{
                    color: costedIncomplete ? 'var(--color-warning)' : 'var(--color-text-secondary)',
                  }}>
                    {costedSentence}
                  </p>
                )}
              </div>
            </div>
          </div>
        )}

        {summary && summary.total > 0 && (
          <div className={isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
            <Stat label={t('proj_chg_stat_total', { defaultValue: 'Demandes' })} value={summary.total} />
            <Stat label={t('proj_chg_stat_to_assess', { defaultValue: 'À évaluer' })}
              value={summary.awaiting_assessment}
              tone={summary.awaiting_assessment > 0 ? 'warning' : 'muted'}
              icon={<FileSearch size={14} />} />
            <Stat label={t('proj_chg_stat_to_decide', { defaultValue: 'À trancher' })}
              value={summary.awaiting_decision}
              tone={summary.awaiting_decision > 0 ? 'warning' : 'muted'}
              icon={<Scale size={14} />} />
            <Stat label={t('proj_chg_stat_approved', { defaultValue: 'Approuvées' })}
              value={summary.approved} tone="success" />
            <Stat label={t('proj_chg_stat_rejected', { defaultValue: 'Refusées' })}
              value={summary.rejected} tone="muted" />
            <Stat label={t('proj_chg_stat_deferred', { defaultValue: 'Reportées' })}
              value={summary.deferred} tone="muted" />
          </div>
        )}

        {/* The change is in the plan, but nothing records what the plan became:
            the project is then measured against a baseline that no longer
            describes what it was allowed to do. */}
        {summary && summary.approved_without_baseline > 0 && (
          <Callout
            variant="warning"
            title={t('proj_chg_no_baseline_title', {
              defaultValue: '{{n}} approbation(s) sans plan de référence',
              n: summary.approved_without_baseline,
            })}
            action={onOpenBaselines ? {
              label: t('proj_chg_open_baselines', { defaultValue: 'Voir les plans de référence' }),
              onClick: onOpenBaselines,
              icon: <ArrowUpRight size={14} />,
            } : undefined}
          >
            <span className="text-sm">
              {t('proj_chg_no_baseline_msg', {
                defaultValue: 'Ces changements ont été approuvés et sont donc dans le plan — mais aucun plan de référence n’enregistre ce que le plan est devenu. Le projet continuera d’être jugé sur un plan de référence qui ne décrit plus ce qu’il a le droit de faire. Capturez un plan de référence, puis nommez-le sur la décision.',
              })}
            </span>
          </Callout>
        )}

        {/* Two waiting lists, deliberately separated: évaluer et trancher ne
            sont ni le même geste, ni les mêmes personnes. */}
        {(toAssess.length > 0 || toDecide.length > 0) && (
          <div className={isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-3 items-start'}>
            {toAssess.length > 0 && (
              <div className="rounded-xl border border-border bg-surface-0 p-3">
                <div className="flex items-center gap-2 mb-2">
                  <FileSearch size={15} className="text-text-tertiary" />
                  <h2 className="text-sm font-medium text-text-primary">
                    {t('proj_chg_queue_assess', { defaultValue: 'À évaluer ({{n}})', n: toAssess.length })}
                  </h2>
                </div>
                <p className="text-xs text-text-secondary mb-2">
                  {t('proj_chg_queue_assess_msg', {
                    defaultValue: 'Personne n’a encore chiffré ce que ces demandes coûteraient. Aucune ne peut être approuvée en l’état.',
                  })}
                </p>
                <div className="flex flex-wrap gap-2">
                  {toAssess.map(a => (
                    <button
                      key={a.id}
                      type="button"
                      onClick={() => focusChange(a.id)}
                      className="inline-flex items-center gap-1.5 text-sm px-2 py-1 rounded border border-border bg-surface-1 hover:bg-surface-2 text-text-primary"
                    >
                      {a.code && <span className="font-mono text-xs text-text-tertiary">{a.code}</span>}
                      <span className="truncate max-w-[18rem]">{a.title}</span>
                      {a.urgency !== 'normal' && (
                        <span className="inline-flex">
                          <Badge variant={URGENCY_VARIANT[a.urgency]} size="sm">{urgencyLabel(a.urgency)}</Badge>
                        </span>
                      )}
                      <ArrowUpRight size={13} className="text-text-tertiary shrink-0" />
                    </button>
                  ))}
                </div>
              </div>
            )}
            {toDecide.length > 0 && (
              <div className="rounded-xl border border-border bg-surface-0 p-3">
                <div className="flex items-center gap-2 mb-2">
                  <Scale size={15} className="text-text-tertiary" />
                  <h2 className="text-sm font-medium text-text-primary">
                    {t('proj_chg_queue_decide', { defaultValue: 'À trancher ({{n}})', n: toDecide.length })}
                  </h2>
                </div>
                <p className="text-xs text-text-secondary mb-2">
                  {isOwner
                    ? t('proj_chg_queue_decide_msg_owner', {
                      defaultValue: 'Évaluées : l’effet est chiffré et la décision vous revient.',
                    })
                    : t('proj_chg_queue_decide_msg', {
                      defaultValue: 'Évaluées : l’effet est chiffré, elles attendent la décision du responsable du projet.',
                    })}
                </p>
                <div className="flex flex-wrap gap-2">
                  {toDecide.map(a => (
                    <button
                      key={a.id}
                      type="button"
                      onClick={() => focusChange(a.id)}
                      className="inline-flex items-center gap-1.5 text-sm px-2 py-1 rounded border border-border bg-surface-1 hover:bg-surface-2 text-text-primary"
                    >
                      {a.code && <span className="font-mono text-xs text-text-tertiary">{a.code}</span>}
                      <span className="truncate max-w-[18rem]">{a.title}</span>
                      {a.urgency !== 'normal' && (
                        <span className="inline-flex">
                          <Badge variant={URGENCY_VARIANT[a.urgency]} size="sm">{urgencyLabel(a.urgency)}</Badge>
                        </span>
                      )}
                      <ArrowUpRight size={13} className="text-text-tertiary shrink-0" />
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        {summary && summary.total > 0 && awaiting.length === 0 && (
          <Callout
            variant="success"
            icon={<CheckCircle2 size={16} />}
            title={t('proj_chg_queue_empty_title', { defaultValue: 'Aucune demande en attente' })}
          >
            <span className="text-sm">
              {t('proj_chg_queue_empty_msg', {
                defaultValue: 'Toutes les demandes ont été évaluées puis tranchées : le registre est à jour, et rien n’avance en dehors du processus.',
              })}
            </span>
          </Callout>
        )}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {isError ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="error"
              icon={<GitCompare size={26} />}
              title={t('proj_chg_load_error', { defaultValue: 'Le registre n’a pas pu être chargé.' })}
              action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
              t={t}
            />
          </div>
        ) : isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-8 text-center text-sm text-text-secondary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : changes.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="first-use"
              icon={<GitCompare size={26} />}
              title={t('proj_chg_empty_title', { defaultValue: 'Aucune demande de changement' })}
              description={t('proj_chg_empty', {
                defaultValue: 'Un projet ne dérive presque jamais d’un seul grand écart : il dérive de petits « oui » que personne n’a chiffrés. Consignez ici chaque demande, chiffrez son effet, puis tranchez-la — c’est ce qui permet de dire, plus tard, de combien le plan a bougé et pourquoi.',
              })}
              action={canEdit ? {
                label: t('proj_chg_new', { defaultValue: 'Nouvelle demande' }),
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
              icon={<GitCompare size={26} />}
              title={t('proj_chg_filter_empty', { defaultValue: 'Aucune demande à ce statut' })}
              action={{
                label: t('proj_chg_filter_clear', { defaultValue: 'Afficher tous les statuts' }),
                onClick: () => setStatusFilter(''),
              }}
              t={t}
            />
          </div>
        ) : (
          // The server's order is kept: re-sorting here would bury exactly what
          // it chose to put on top.
          <div className="space-y-4">
            {visible.map(c => <ChangeCard key={c.id} change={c} ctx={ctx} />)}
          </div>
        )}
      </div>

      {/* ── Nouvelle demande ── */}
      {createOpen && (
        <FloatingWindow
          title={t('proj_chg_new', { defaultValue: 'Nouvelle demande de changement' })}
          icon={<GitCompare size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={580} defaultHeight={560} padding={16} t={t}
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
                {t('proj_chg_col_title', { defaultValue: 'Intitulé' })}
              </label>
              <Input
                autoFocus
                value={createDraft.title}
                placeholder={t('proj_chg_title_ph', { defaultValue: 'Ce qui est demandé, en une phrase…' })}
                onChange={e => setCreateDraft(d => ({ ...d, title: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); submitCreate() } }}
              />
            </div>
            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_chg_category', { defaultValue: 'Nature' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={createDraft.category} options={categoryOptions}
                  onChange={v => setCreateDraft(d => ({ ...d, category: v as ChangeCategory }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_chg_kind', { defaultValue: 'Type d’action' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={createDraft.kind} options={kindOptions}
                  onChange={v => setCreateDraft(d => ({ ...d, kind: v as ChangeKind }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_chg_urgency', { defaultValue: 'Urgence' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={createDraft.urgency} options={urgencyOptions}
                  onChange={v => setCreateDraft(d => ({ ...d, urgency: v as ChangeUrgency }))}
                />
              </div>
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_description', { defaultValue: 'Ce qui est demandé' })}
              </label>
              <Textarea
                rows={3}
                className="h-auto min-h-0 resize-y"
                value={createDraft.description}
                placeholder={t('proj_chg_description_ph', { defaultValue: 'Ce qu’il faudrait changer, concrètement…' })}
                onChange={e => setCreateDraft(d => ({ ...d, description: e.target.value }))}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_justification', { defaultValue: 'Justification' })}
              </label>
              <Textarea
                rows={3}
                className="h-auto min-h-0 resize-y"
                value={createDraft.justification}
                placeholder={t('proj_chg_justification_ph', { defaultValue: 'Ce qui rend ce changement nécessaire…' })}
                onChange={e => setCreateDraft(d => ({ ...d, justification: e.target.value }))}
              />
              <p className="text-xs text-text-tertiary mt-1">
                {t('proj_chg_create_hint', {
                  defaultValue: 'L’évaluation — jours, coût, périmètre, risque, qualité — se renseigne ensuite : la demande démarre au statut « Demandée ».',
                })}
              </p>
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* ── Évaluation ── */}
      {assessTarget && (
        <FloatingWindow
          title={t('proj_chg_assess_title', { defaultValue: 'Évaluer l’effet du changement' })}
          icon={<FileSearch size={16} />}
          onClose={() => setAssessTarget(null)}
          defaultWidth={620} defaultHeight={600} padding={16} t={t}
          actions={{
            confirm: {
              label: t('proj_chg_assess_save', { defaultValue: 'Enregistrer l’évaluation' }),
              disabled: !assessmentSaysSomething(assessDraft),
              loading: assessMut.isPending,
              autoFocus: true,
              onClick: submitAssess,
            },
          }}
        >
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              {t('proj_chg_assess_intro', {
                defaultValue: '« {{title}} » — chiffrez ce que ce changement fait au projet. Une fois évaluée, la demande passe au statut « Évaluée » et devient approuvable.',
                title: assessTarget.title,
              })}
            </p>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_chg_impact_days', { defaultValue: 'Effet sur le calendrier' })}
                </label>
                <Input
                  type="number"
                  step="0.5"
                  value={assessDraft.days}
                  placeholder={t('proj_chg_impact_days_ph', { defaultValue: 'en jours, ex. 15' })}
                  onChange={e => setAssessDraft(d => ({ ...d, days: e.target.value }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_chg_impact_cost', { defaultValue: 'Effet sur le budget' })}
                </label>
                <Input
                  type="number"
                  step="100"
                  value={assessDraft.cost}
                  placeholder={t('proj_chg_impact_cost_ph', { defaultValue: 'montant, ex. 20700' })}
                  onChange={e => setAssessDraft(d => ({ ...d, cost: e.target.value }))}
                />
              </div>
            </div>
            <p className="text-xs text-text-tertiary">
              {t('proj_chg_impact_null_hint', {
                defaultValue: 'Laissez vide ce qui n’a pas été chiffré : le registre l’affichera « non évalué » plutôt que zéro — ce n’est pas la même chose.',
              })}
            </p>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_impact_scope', { defaultValue: 'Périmètre' })}
              </label>
              <Textarea
                rows={2}
                className="h-auto min-h-0 resize-y"
                value={assessDraft.scope}
                placeholder={t('proj_chg_impact_scope_ph', { defaultValue: 'Ce que le changement ajoute, retire ou déplace dans le périmètre…' })}
                onChange={e => setAssessDraft(d => ({ ...d, scope: e.target.value }))}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_impact_risk', { defaultValue: 'Risque' })}
              </label>
              <Textarea
                rows={2}
                className="h-auto min-h-0 resize-y"
                value={assessDraft.risk}
                placeholder={t('proj_chg_impact_risk_ph', { defaultValue: 'Les risques que le changement crée, aggrave ou lève…' })}
                onChange={e => setAssessDraft(d => ({ ...d, risk: e.target.value }))}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_impact_quality', { defaultValue: 'Qualité' })}
              </label>
              <Textarea
                rows={2}
                className="h-auto min-h-0 resize-y"
                value={assessDraft.quality}
                placeholder={t('proj_chg_impact_quality_ph', { defaultValue: 'Ce que le changement fait aux critères d’acceptation…' })}
                onChange={e => setAssessDraft(d => ({ ...d, quality: e.target.value }))}
              />
            </div>
            {!assessmentSaysSomething(assessDraft) && (
              <p className="text-xs" style={{ color: 'var(--color-warning)' }}>
                {t('proj_chg_assess_empty', {
                  defaultValue: 'Une évaluation vide est refusée : au moins une des cinq dimensions doit dire quelque chose.',
                })}
              </p>
            )}
          </div>
        </FloatingWindow>
      )}

      {/* ── Décision ── */}
      {decideTarget && (
        <FloatingWindow
          title={t('proj_chg_decide_title', { defaultValue: 'Trancher la demande' })}
          icon={<Scale size={16} />}
          onClose={() => setDecideTarget(null)}
          defaultWidth={580} defaultHeight={520} padding={16} t={t}
          actions={{
            confirm: {
              label: t('proj_chg_decide_save', { defaultValue: 'Enregistrer la décision' }),
              disabled: !canDecide,
              loading: decideMut.isPending,
              danger: decideStatus === 'rejected',
              autoFocus: true,
              onClick: submitDecide,
            },
          }}
        >
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              {decideTarget.code ? `${decideTarget.code} — ${decideTarget.title}` : decideTarget.title}
            </p>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_decision', { defaultValue: 'Décision' })}
              </label>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={decideStatus}
                options={decisionOptions}
                onChange={v => setDecideStatus(v as ChangeStatus)}
              />
            </div>

            {decideBlocked && (
              <Callout variant="danger" icon={<AlertTriangle size={16} />}>
                <span className="text-sm">
                  {t('proj_chg_decide_blocked', {
                    defaultValue: 'Cette demande n’a pas été évaluée : le serveur refusera de l’approuver. Chiffrez d’abord son effet — approuver sans savoir ce que cela coûte est exactement ce que la maîtrise des changements interdit.',
                  })}
                </span>
              </Callout>
            )}

            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_chg_decision_note', { defaultValue: 'Motif de la décision' })}
              </label>
              <Textarea
                rows={4}
                className="h-auto min-h-0 resize-y"
                value={decideNote}
                placeholder={decideStatus === 'partially_approved'
                  ? t('proj_chg_note_ph_partial', { defaultValue: 'Ce qui a été retenu, et ce qui ne l’a pas été…' })
                  : decideStatus === 'rejected'
                    ? t('proj_chg_note_ph_reject', { defaultValue: 'Pourquoi la demande est refusée…' })
                    : t('proj_chg_note_ph', { defaultValue: 'Ce qui a motivé la décision…' })}
                onChange={e => setDecideNote(e.target.value)}
              />
              <p className="text-xs mt-1" style={{
                color: decideNoteMissing ? 'var(--color-warning)' : 'var(--color-text-tertiary)',
              }}>
                {decideStatus === 'rejected'
                  ? t('proj_chg_note_required_reject', {
                    defaultValue: 'Obligatoire : un refus sans motif est refusé par le serveur — et laisse le demandeur sans réponse.',
                  })
                  : decideStatus === 'partially_approved'
                    ? t('proj_chg_note_required_partial', {
                      defaultValue: 'Obligatoire : une approbation partielle doit dire ce qui a été retenu et ce qui ne l’a pas été.',
                    })
                    : t('proj_chg_note_optional', {
                      defaultValue: 'Facultatif, mais c’est la seule trace de ce qui a fait pencher la balance.',
                    })}
              </p>
            </div>

            {isApproval(decideStatus) && (
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_chg_baseline', { defaultValue: 'Plan de référence après approbation' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={decideBaseline} options={baselineOptions}
                  onChange={setDecideBaseline}
                />
                <p className="text-xs mt-1" style={{
                  color: decideBaseline ? 'var(--color-text-tertiary)' : 'var(--color-warning)',
                }}>
                  {decideBaseline
                    ? t('proj_chg_baseline_hint', {
                      defaultValue: 'Le plan que ce changement fait entrer en vigueur : c’est lui qui dira, plus tard, ce que le projet avait le droit de faire.',
                    })
                    : t('proj_chg_baseline_warn', {
                      defaultValue: 'Sans plan de référence nommé, le changement entrera dans le plan sans que rien n’enregistre ce que le plan est devenu.',
                    })}
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
