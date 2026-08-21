import { useEffect, useMemo, useRef, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO, startOfDay } from 'date-fns'
import type { TFunction } from 'i18next'
import { getDateLocale, useConfirm } from '@kubuno/sdk'
import {
  ArrowUpRight, Banknote, CalendarClock, CircleCheckBig, ClipboardList,
  Coins, Handshake, Hourglass, Infinity as InfinityIcon, ListTree, Lock, Package,
  Pencil, Plus, Receipt, Scale, ShieldAlert, ShoppingCart, Split, Trash2, Truck, User,
} from 'lucide-react'
import {
  Badge, Button, Callout, ConfirmDialog, Dropdown, EmptyState, FloatingWindow,
  Input, ProgressBar, Textarea, Tooltip, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type ContractType, type PaymentEdit, type PaymentStatus, type Procurement, type ProcurementEdit,
  type ProcurementLine, type ProcurementPayment, type ProcurementStatus, type RiskSide,
} from '../api'

// Procurement — the register of what the project buys from other people, and of
// what it has promised to pay them.
//
// The field that organises the whole screen is the CONTRACT TYPE, because it is
// the only one that answers the question a procurement register is for: when the
// estimate turns out to be wrong, WHO PAYS?
//
//   fixed price / fixed incentive   → the SUPPLIER absorbs the overrun.
//   cost plus fee / cost incentive  → the PROJECT absorbs it: it reimburses costs.
//   time and material               → SHARED — and unbounded unless a ceiling caps it.
//
// A single "committed" figure hides exactly that. 99 000 € committed of which
// 48 000 € sit on the supplier's back is not the same exposure as 99 000 € on
// time and material, and no register that prints one number can tell the
// difference. So the screen never shows the total without the split, and leads
// with the split said in a sentence.
//
// Two absences get their own place, because an absence is invisible in a table:
//   • UNCAPPED — time and material with no not-to-exceed. Not an amount: the
//     lack of one. Nothing bounds what such a contract can cost.
//   • UNPRICED — live contracts with no value recorded. They commit something
//     nobody can total.

// ── Vocabulary ───────────────────────────────────────────────────────────────

const CONTRACT_TYPES: ContractType[] = [
  'fixed_price', 'fixed_incentive', 'cost_plus_fee', 'cost_plus_incentive', 'time_material', 'other',
]
const STATUSES: ProcurementStatus[] = ['planned', 'tendering', 'awarded', 'active', 'closed', 'cancelled']
const PAYMENT_STATUSES: PaymentStatus[] = ['planned', 'invoiced', 'paid', 'disputed', 'cancelled']

type Tone = 'default' | 'primary' | 'success' | 'warning' | 'danger' | 'neutral'

/** Contract STATUS is a state of the relationship, so it takes a status colour.
 *  The contract TYPE never does: a fixed price is not "good" and a cost-plus is
 *  not "bad" — they are two ways of placing a risk, and colouring them would
 *  pass a judgement the register has no business passing. */
const STATUS_VARIANT: Record<ProcurementStatus, Tone> = {
  planned:   'neutral',
  tendering: 'default',
  awarded:   'primary',
  active:    'primary',
  closed:    'success',
  cancelled: 'neutral',
}

const PAYMENT_VARIANT: Record<PaymentStatus, Tone> = {
  planned:   'neutral',
  invoiced:  'default',
  paid:      'success',
  disputed:  'danger',
  cancelled: 'neutral',
}

/** Committed: the register keeps the trace of it, so the server refuses a delete. */
const ENGAGED: ProcurementStatus[] = ['awarded', 'active', 'closed']
const isEngaged = (s: ProcurementStatus) => ENGAGED.includes(s)

/** The two statuses the server refuses to set on a contract with no value:
 *  awarding something whose amount nobody wrote down commits an unknown. */
const needsValue = (s: ProcurementStatus) => s === 'awarded' || s === 'active'

/** Reimbursed at cost — the project, not the supplier, carries the estimate. */
const isReimbursable = (c: ContractType) =>
  c === 'cost_plus_fee' || c === 'cost_plus_incentive' || c === 'time_material'

/**
 * Exposure ramp, NOT a status ramp. One hue, three intensities, reading "how
 * much of this money the project itself is on the hook for": faint when the
 * supplier carries it, full when the project does. Nothing here says good or bad.
 */
function riskFill(side: RiskSide): string {
  switch (side) {
    case 'supplier': return 'color-mix(in srgb, var(--color-primary) 22%, var(--color-surface-2))'
    case 'shared':   return 'color-mix(in srgb, var(--color-primary) 55%, var(--color-surface-2))'
    case 'buyer':    return 'var(--color-primary)'
    default:         return 'color-mix(in srgb, var(--color-text-tertiary) 40%, var(--color-surface-2))'
  }
}

/** The same ramp, diluted enough to carry text on top of it. The saturated
 *  `riskFill` is only ever used for surfaces nothing is written on — the bar
 *  segments and the small swatches beside a label. */
function riskTint(side: RiskSide): string {
  return side === 'unknown'
    ? 'color-mix(in srgb, var(--color-text-tertiary) 14%, transparent)'
    : 'color-mix(in srgb, var(--color-primary) 12%, transparent)'
}

function riskIcon(side: RiskSide, size = 13) {
  switch (side) {
    case 'supplier': return <Truck size={size} />
    case 'buyer':    return <Banknote size={size} />
    case 'shared':   return <Split size={size} />
    default:         return <Scale size={size} />
  }
}

/** Said in a full sentence rather than as a label: "régie avec honoraires" means
 *  nothing to the person who will have to pay the overrun. */
function riskSentence(t: TFunction, side: RiskSide, capped: boolean): string {
  switch (side) {
    case 'supplier':
      return t('proj_prc_risk_supplier_msg', {
        defaultValue: 'Un dépassement est absorbé par le fournisseur : le prix est ferme, le projet paie ce qui a été convenu.',
      })
    case 'buyer':
      return t('proj_prc_risk_buyer_msg', {
        defaultValue: 'Un dépassement est à la charge du projet : ce contrat rembourse les coûts constatés, quels qu’ils soient.',
      })
    case 'shared':
      return capped
        ? t('proj_prc_risk_shared_capped_msg', {
          defaultValue: 'Le projet paie le temps réellement passé : le dépassement est partagé, et le plafond l’arrête.',
        })
        : t('proj_prc_risk_shared_uncapped_msg', {
          defaultValue: 'Le projet paie le temps réellement passé, et aucun plafond n’est fixé : rien ne borne ce que ce contrat peut coûter.',
        })
    default:
      return t('proj_prc_risk_unknown_msg', {
        defaultValue: 'Le type de contrat ne dit pas qui absorbe un dépassement : tant qu’il n’est pas précisé, l’exposition du projet est inconnue.',
      })
  }
}

/** Time and material with no ceiling — the absence the register has to name. */
const isUncapped = (c: Procurement) => c.contract_type === 'time_material' && c.not_to_exceed === null

/** Late, for the row's own styling. The COUNTS always come from the server
 *  (`overdue_payments`); this only decides which line gets the mark. */
function isLate(p: ProcurementPayment, today: Date): boolean {
  if (!p.due_on) return false
  if (p.status !== 'planned' && p.status !== 'invoiced') return false
  return parseISO(p.due_on) < today
}

/** "a, b et c" — a plain join, so no `Intl.ListFormat` availability to worry about. */
function joinParts(parts: string[], and: string): string {
  if (parts.length <= 1) return parts.join('')
  return `${parts.slice(0, -1).join(', ')} ${and} ${parts[parts.length - 1]}`
}

const numOrNull = (s: string): number | null => {
  const trimmed = s.trim()
  if (trimmed === '') return null
  const v = Number(trimmed.replace(',', '.'))
  return Number.isFinite(v) ? v : null
}

/** Everything a card needs, passed as one object so the card components can live
 *  at module level: defined inside the parent they would be a new component type
 *  on every render, remounting the list — and dropping the edit in progress —
 *  each time a query settles. */
interface CardCtx {
  t: TFunction
  canEdit: boolean
  isMobile: boolean
  today: Date
  patch: (id: string, body: ProcurementEdit) => void
  changeStatus: (c: Procurement, next: ProcurementStatus) => void
  askDelete: (c: Procurement) => void
  openPayment: (contractId: string, payment: ProcurementPayment | null) => void
  askDeletePayment: (contractId: string, p: ProcurementPayment) => void
  setPaymentStatus: (contractId: string, p: ProcurementPayment, next: PaymentStatus) => void
  fmtDate: (iso: string) => string
  money: (v: number) => string
  statusLabel: (s: ProcurementStatus) => string
  typeLabel: (c: ContractType) => string
  paymentStatusLabel: (s: PaymentStatus) => string
  riskLabel: (s: RiskSide) => string
  typeOptions: DropdownOption[]
  statusOptions: DropdownOption[]
  paymentStatusOptions: DropdownOption[]
  stakeholderOptions: DropdownOption[]
  deliverableOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  riskOptions: DropdownOption[]
  registerCard: (id: string, el: HTMLDivElement | null) => void
  /** Briefly ringed after a jump, so the eye lands on the right card. */
  flashId: string | null
  onOpenTask?: (taskId: string) => void
  onOpenRisks?: () => void
  onOpenStakeholders?: () => void
}

// ── Small building blocks ────────────────────────────────────────────────────

/** Long prose read as prose. Several paragraphs flow in two columns — no width
 *  cap anywhere on this screen, and a 200-character line is unreadable. A single
 *  paragraph stays in one flow: `break-inside-avoid` would leave a column empty. */
function Prose({ text, empty, columns }: { text: string; empty: string; columns: boolean }) {
  const paragraphs = useMemo(
    () => text.split(/\n{2,}/).map(p => p.trim()).filter(Boolean),
    [text],
  )
  if (paragraphs.length === 0) {
    return <p className="text-sm italic text-text-tertiary">{empty}</p>
  }
  const twoCols = columns && paragraphs.length > 1
  return (
    <div className={twoCols ? 'columns-2 gap-8' : undefined}>
      {paragraphs.map((p, i) => (
        <p
          key={i}
          className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed break-inside-avoid mb-2 last:mb-0"
        >
          {p}
        </p>
      ))}
    </div>
  )
}

/** One block of the record: a heading, then the prose — edited as a textarea
 *  committed on blur (one PATCH per edit, not per keystroke). */
function RecordField({ label, icon, hint, value, placeholder, rows = 3, disabled, emptyLabel, columns, onCommit }: {
  label: string
  icon?: React.ReactNode
  hint?: string
  value: string
  placeholder?: string
  rows?: number
  disabled?: boolean
  /** Said in words rather than left as a dash when the absence is the news. */
  emptyLabel?: string
  columns?: boolean
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
        <Prose text={value} empty={emptyLabel ?? '—'} columns={!!columns} />
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

/** A read-only figure. `null` is NOT zero — it is "nobody wrote it down". */
function Figure({ label, value, icon, missing, warn }: {
  label: string
  value: string | null
  icon: React.ReactNode
  missing: string
  warn?: boolean
}) {
  return (
    <div
      className="rounded-lg border bg-surface-1 px-3 py-2 min-w-0 flex-1"
      style={{ borderColor: warn ? 'var(--color-warning)' : 'var(--color-border)' }}
    >
      <div className="flex items-center gap-1.5 text-xs text-text-secondary mb-0.5">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      {value === null ? (
        <p className="text-sm italic" style={{ color: warn ? 'var(--color-warning)' : 'var(--color-text-tertiary)' }}>
          {missing}
        </p>
      ) : (
        <p className="text-lg font-semibold tabular-nums text-text-primary">{value}</p>
      )}
    </div>
  )
}

interface Segment { side: RiskSide; amount: number; label: string }

/** The committed total, drawn as what it actually is: three or four different
 *  exposures that happen to be denominated in the same currency. */
function CommittedSplit({ segments, total, money, share }: {
  segments: Segment[]
  total: number
  money: (v: number) => string
  share: (v: number) => string
}) {
  const shown = segments.filter(s => s.amount > 0)
  if (total <= 0 || shown.length === 0) return null
  return (
    <div className="mt-3">
      <div className="flex h-3 rounded-full overflow-hidden border border-border bg-surface-2">
        {shown.map(s => (
          <Tooltip key={s.side} label={`${s.label} — ${money(s.amount)} (${share(s.amount / total)})`}>
            <div style={{ width: `${(s.amount / total) * 100}%`, background: riskFill(s.side) }} />
          </Tooltip>
        ))}
      </div>
      <div className="flex flex-wrap gap-x-5 gap-y-1 mt-2">
        {shown.map(s => (
          <span key={s.side} className="inline-flex items-center gap-1.5 text-xs text-text-secondary">
            <span
              className="w-2.5 h-2.5 rounded-sm shrink-0 border border-border"
              style={{ background: riskFill(s.side) }}
            />
            <span>{s.label}</span>
            <span className="tabular-nums font-medium text-text-primary">{money(s.amount)}</span>
            <span className="text-text-tertiary tabular-nums">{share(s.amount / total)}</span>
          </span>
        ))}
      </div>
    </div>
  )
}

/** A link to another register, drawn as a chip. Disabled rather than hidden when
 *  the screen was not given a way to get there. */
function LinkChip({ icon, label, onClick }: {
  icon: React.ReactNode
  label: string
  onClick?: () => void
}) {
  if (!onClick) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <span className="text-text-tertiary">{icon}</span>
        {label}
      </span>
    )
  }
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
    >
      {icon}
      {label}
      <ArrowUpRight size={12} />
    </button>
  )
}

// ── One payment milestone ────────────────────────────────────────────────────

function PaymentRow({ payment, contractId, ctx }: {
  payment: ProcurementPayment
  contractId: string
  ctx: CardCtx
}) {
  const { t } = ctx
  const late = isLate(payment, ctx.today)
  return (
    <div
      className={ctx.isMobile
        ? 'px-3 py-2 border-t border-border'
        : 'px-3 py-2 border-t border-border flex items-center gap-3'}
      style={late ? { background: 'color-mix(in srgb, var(--color-danger) 7%, transparent)' } : undefined}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-sm text-text-primary truncate">
            {payment.label || t('proj_prc_pay_untitled', { defaultValue: 'Échéance sans libellé' })}
          </span>
          {payment.invoice_ref && (
            <span className="inline-flex items-center gap-1 font-mono text-xs text-text-tertiary">
              <Receipt size={11} />
              {payment.invoice_ref}
            </span>
          )}
        </div>
        <div className="flex items-center gap-3 flex-wrap mt-0.5">
          <span
            className="inline-flex items-center gap-1 text-xs"
            style={{ color: late ? 'var(--color-danger)' : 'var(--color-text-secondary)' }}
          >
            <CalendarClock size={11} />
            {payment.due_on
              ? (late
                ? t('proj_prc_pay_overdue_on', { defaultValue: 'échue le {{date}}', date: ctx.fmtDate(payment.due_on) })
                : t('proj_prc_pay_due_on', { defaultValue: 'échéance le {{date}}', date: ctx.fmtDate(payment.due_on) }))
              : t('proj_prc_pay_no_due', { defaultValue: 'aucune échéance fixée' })}
          </span>
          {payment.paid_on && (
            <span className="inline-flex items-center gap-1 text-xs text-text-secondary">
              <CircleCheckBig size={11} style={{ color: 'var(--color-success)' }} />
              {t('proj_prc_pay_paid_on', { defaultValue: 'payée le {{date}}', date: ctx.fmtDate(payment.paid_on) })}
            </span>
          )}
        </div>
      </div>

      <div className={ctx.isMobile
        ? 'flex items-center gap-2 flex-wrap mt-2'
        : 'flex items-center gap-2 shrink-0'}>
        <span className="text-sm font-medium tabular-nums text-text-primary w-28 text-right">
          {ctx.money(payment.amount)}
        </span>
        {ctx.canEdit ? (
          <Dropdown
            width={150} height={36} fontSize={14} focusable
            value={payment.status}
            options={ctx.paymentStatusOptions}
            onChange={v => ctx.setPaymentStatus(contractId, payment, v as PaymentStatus)}
          />
        ) : (
          <span className="inline-flex">
            <Badge variant={PAYMENT_VARIANT[payment.status]} dot>
              {ctx.paymentStatusLabel(payment.status)}
            </Badge>
          </span>
        )}
        {ctx.canEdit && (
          <>
            <button
              type="button"
              onClick={() => ctx.openPayment(contractId, payment)}
              title={t('common_edit', { defaultValue: 'Modifier' })}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-text-primary"
            >
              <Pencil size={14} />
            </button>
            <button
              type="button"
              onClick={() => ctx.askDeletePayment(contractId, payment)}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={14} />
            </button>
          </>
        )}
      </div>
    </div>
  )
}

// ── The bands of one contract ────────────────────────────────────────────────

/** Who carries the overrun — said before anything else about the contract. */
function RiskBand({ line, ctx }: { line: ProcurementLine; ctx: CardCtx }) {
  const { t } = ctx
  const c = line.contract
  const uncapped = isUncapped(c)
  const capped = c.not_to_exceed !== null
  return (
    <div
      className="px-4 py-3 flex items-start gap-2.5"
      style={{
        background: uncapped
          ? 'color-mix(in srgb, var(--color-warning) 8%, transparent)'
          : 'color-mix(in srgb, var(--color-primary) 4%, transparent)',
      }}
    >
      <span
        className="shrink-0 mt-[2px] w-5 h-5 rounded-full inline-flex items-center justify-center border border-border text-text-secondary"
        style={{ background: riskTint(line.risk_side) }}
      >
        {riskIcon(line.risk_side, 12)}
      </span>
      <div className="min-w-0">
        <p className="text-sm text-text-primary leading-relaxed">
          {riskSentence(t, line.risk_side, capped)}
        </p>
        {uncapped && (
          <p className="text-xs mt-1 inline-flex items-center gap-1.5" style={{ color: 'var(--color-warning)' }}>
            <InfinityIcon size={13} />
            {t('proj_prc_uncapped_inline', {
              defaultValue: 'Sans plafond, l’exposition du projet n’a pas de borne supérieure : ce n’est pas un montant élevé, c’est un montant absent.',
            })}
          </p>
        )}
      </div>
    </div>
  )
}

/** What was bought, from whom, and why it was bought rather than built. */
function ContractBand({ line, ctx }: { line: ProcurementLine; ctx: CardCtx }) {
  const { t } = ctx
  const c = line.contract
  const editable = ctx.canEdit
  const chips: React.ReactNode[] = []

  if (c.supplier_name) {
    chips.push(
      <span key="sup" className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <Truck size={12} className="text-text-tertiary" />
        {c.supplier_name}
        {c.supplier_contact && <span className="text-text-tertiary">· {c.supplier_contact}</span>}
      </span>,
    )
  }
  if (c.stakeholder_name) {
    chips.push(
      <LinkChip
        key="holder"
        icon={<User size={12} />}
        label={t('proj_prc_owner_chip', { defaultValue: 'suivi par {{name}}', name: c.stakeholder_name })}
        onClick={ctx.onOpenStakeholders}
      />,
    )
  }
  if (c.deliverable_name) {
    chips.push(
      <span key="deliv" className="inline-flex items-center gap-1 text-xs text-text-secondary">
        <Package size={12} className="text-text-tertiary" />
        {c.deliverable_name}
      </span>,
    )
  }
  if (c.task_id && c.task_name) {
    const taskId = c.task_id
    chips.push(
      <LinkChip
        key="task"
        icon={<ListTree size={12} />}
        label={c.task_name}
        onClick={ctx.onOpenTask ? () => ctx.onOpenTask?.(taskId) : undefined}
      />,
    )
  }
  if (c.risk_code) {
    chips.push(
      <LinkChip
        key="risk"
        icon={<ShieldAlert size={12} />}
        label={t('proj_prc_risk_chip', { defaultValue: 'risque {{code}}', code: c.risk_code })}
        onClick={ctx.onOpenRisks}
      />,
    )
  }

  const dates: string[] = []
  if (c.awarded_on) dates.push(t('proj_prc_awarded_on', { defaultValue: 'attribué le {{date}}', date: ctx.fmtDate(c.awarded_on) }))
  if (c.starts_on) dates.push(t('proj_prc_starts_on', { defaultValue: 'début {{date}}', date: ctx.fmtDate(c.starts_on) }))
  if (c.ends_on) dates.push(t('proj_prc_ends_on', { defaultValue: 'fin {{date}}', date: ctx.fmtDate(c.ends_on) }))

  return (
    <div className="px-4 py-3 border-t border-border">
      {chips.length > 0 && <div className="flex items-center gap-3 flex-wrap mb-3">{chips}</div>}

      <div className={ctx.isMobile ? 'grid grid-cols-2 gap-2 mb-3' : 'flex gap-2 mb-3'}>
        <Figure
          label={t('proj_prc_value', { defaultValue: 'Montant engagé' })}
          icon={<Coins size={13} />}
          value={c.value === null ? null : ctx.money(c.value)}
          missing={t('proj_prc_value_missing', { defaultValue: 'non renseigné' })}
          warn={c.value === null && needsValue(c.status)}
        />
        <Figure
          label={t('proj_prc_nte', { defaultValue: 'Plafond' })}
          icon={<Hourglass size={13} />}
          value={c.not_to_exceed === null ? null : ctx.money(c.not_to_exceed)}
          missing={isReimbursable(c.contract_type)
            ? t('proj_prc_nte_missing_reimb', { defaultValue: 'aucun plafond' })
            : t('proj_prc_nte_none', { defaultValue: 'sans objet' })}
          warn={isUncapped(c)}
        />
      </div>

      {dates.length > 0 && (
        <p className="text-xs text-text-secondary mb-3 inline-flex items-center gap-1.5">
          <CalendarClock size={12} className="text-text-tertiary" />
          {dates.join(' · ')}
        </p>
      )}

      <div className={ctx.isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-6 items-start'}>
        <RecordField
          label={t('proj_prc_sow', { defaultValue: 'Énoncé des travaux' })}
          icon={<ClipboardList size={14} className="text-text-tertiary" />}
          hint={t('proj_prc_sow_hint', {
            defaultValue: 'Ce que le fournisseur doit livrer. C’est ce texte, et lui seul, qui dira plus tard si la prestation est conforme.',
          })}
          value={c.statement_of_work}
          placeholder={t('proj_prc_sow_ph', { defaultValue: 'Ce qui est acheté, précisément…' })}
          disabled={!editable}
          columns={!ctx.isMobile}
          emptyLabel={t('proj_prc_sow_empty', { defaultValue: 'Aucun énoncé des travaux : rien ne définit ce qui est dû.' })}
          onCommit={v => ctx.patch(c.id, { statement_of_work: v })}
        />
        <RecordField
          label={t('proj_prc_make_or_buy', { defaultValue: 'Faire ou faire faire' })}
          icon={<Handshake size={14} className="text-text-tertiary" />}
          hint={t('proj_prc_make_or_buy_hint', {
            defaultValue: 'Pourquoi on achète plutôt que de construire. C’est la décision qui sera re-débattue dans un an, quand plus personne ne se souviendra de ce qui l’avait motivée.',
          })}
          value={c.make_or_buy_note}
          placeholder={t('proj_prc_make_or_buy_ph', { defaultValue: 'Ce qui a fait pencher pour l’achat : compétence, délai, capacité, coût…' })}
          disabled={!editable}
          columns={!ctx.isMobile}
          emptyLabel={t('proj_prc_make_or_buy_empty', {
            defaultValue: 'Aucune note : rien ne dit pourquoi ce travail est acheté plutôt que fait en interne.',
          })}
          onCommit={v => ctx.patch(c.id, { make_or_buy_note: v })}
        />
      </div>

      <div className="mt-3">
        <RecordField
          label={t('proj_prc_terms', { defaultValue: 'Conditions' })}
          icon={<Scale size={14} className="text-text-tertiary" />}
          value={c.terms}
          rows={2}
          placeholder={t('proj_prc_terms_ph', { defaultValue: 'Garanties, pénalités, propriété, réversibilité, résiliation…' })}
          disabled={!editable}
          columns={!ctx.isMobile}
          emptyLabel={t('proj_prc_terms_empty', { defaultValue: 'Aucune condition consignée.' })}
          onCommit={v => ctx.patch(c.id, { terms: v })}
        />
      </div>

      {editable && (
        <div className={ctx.isMobile ? 'space-y-3 mt-4' : 'grid grid-cols-3 gap-3 mt-4'}>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_code', { defaultValue: 'Code' })}
            </label>
            <Input
              key={`code-${c.code}`}
              defaultValue={c.code}
              placeholder="PO-01"
              onBlur={e => { if (e.target.value !== c.code) ctx.patch(c.id, { code: e.target.value }) }}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_type', { defaultValue: 'Type de contrat' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={c.contract_type} options={ctx.typeOptions}
              onChange={v => ctx.patch(c.id, { contract_type: v as ContractType })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_status', { defaultValue: 'Statut' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={c.status} options={ctx.statusOptions}
              onChange={v => ctx.changeStatus(c, v as ProcurementStatus)}
            />
          </div>

          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_supplier', { defaultValue: 'Fournisseur' })}
            </label>
            <Input
              key={`sup-${c.supplier_name}`}
              defaultValue={c.supplier_name}
              placeholder={t('proj_prc_supplier_ph', { defaultValue: 'Raison sociale…' })}
              onBlur={e => { if (e.target.value !== c.supplier_name) ctx.patch(c.id, { supplier_name: e.target.value }) }}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_supplier_contact', { defaultValue: 'Contact chez le fournisseur' })}
            </label>
            <Input
              key={`cnt-${c.supplier_contact}`}
              defaultValue={c.supplier_contact}
              placeholder={t('proj_prc_supplier_contact_ph', { defaultValue: 'Nom, courriel, téléphone…' })}
              onBlur={e => { if (e.target.value !== c.supplier_contact) ctx.patch(c.id, { supplier_contact: e.target.value }) }}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_holder', { defaultValue: 'Partie prenante responsable' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={c.stakeholder_id ?? ''} options={ctx.stakeholderOptions}
              onChange={v => ctx.patch(c.id, { stakeholder_id: v || null })}
            />
          </div>

          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_value', { defaultValue: 'Montant engagé' })}
            </label>
            <Input
              key={`val-${c.value ?? ''}`}
              type="number"
              step="100"
              defaultValue={c.value === null ? '' : String(c.value)}
              placeholder={t('proj_prc_value_ph', { defaultValue: 'ex. 48000' })}
              onBlur={e => {
                const next = numOrNull(e.target.value)
                if (next !== c.value) ctx.patch(c.id, { value: next })
              }}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_nte_label', { defaultValue: 'Plafond (à ne pas dépasser)' })}
            </label>
            <Input
              key={`nte-${c.not_to_exceed ?? ''}`}
              type="number"
              step="100"
              defaultValue={c.not_to_exceed === null ? '' : String(c.not_to_exceed)}
              placeholder={t('proj_prc_nte_ph', { defaultValue: 'ex. 60000' })}
              onBlur={e => {
                const next = numOrNull(e.target.value)
                if (next !== c.not_to_exceed) ctx.patch(c.id, { not_to_exceed: next })
              }}
            />
            {isReimbursable(c.contract_type) && (
              <p className="text-xs mt-1" style={{
                color: c.not_to_exceed === null ? 'var(--color-warning)' : 'var(--color-text-tertiary)',
              }}>
                {c.not_to_exceed === null
                  ? t('proj_prc_nte_warn', {
                    defaultValue: 'En régie, le plafond est la seule borne : sans lui, ce contrat peut coûter n’importe quoi.',
                  })
                  : t('proj_prc_nte_ok', { defaultValue: 'Au-delà, le fournisseur ne peut plus facturer.' })}
              </p>
            )}
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_awarded_on_label', { defaultValue: 'Date d’attribution' })}
            </label>
            <Input
              type="date"
              value={c.awarded_on ? c.awarded_on.slice(0, 10) : ''}
              onChange={e => ctx.patch(c.id, { awarded_on: e.target.value || null })}
            />
          </div>

          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_starts_on_label', { defaultValue: 'Début d’exécution' })}
            </label>
            <Input
              type="date"
              value={c.starts_on ? c.starts_on.slice(0, 10) : ''}
              onChange={e => ctx.patch(c.id, { starts_on: e.target.value || null })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_ends_on_label', { defaultValue: 'Fin d’exécution' })}
            </label>
            <Input
              type="date"
              value={c.ends_on ? c.ends_on.slice(0, 10) : ''}
              onChange={e => ctx.patch(c.id, { ends_on: e.target.value || null })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_deliverable', { defaultValue: 'Livrable' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={c.deliverable_id ?? ''} options={ctx.deliverableOptions}
              onChange={v => ctx.patch(c.id, { deliverable_id: v || null })}
            />
          </div>

          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_task', { defaultValue: 'Lot de travail' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={c.task_id ?? ''} options={ctx.taskOptions}
              onChange={v => ctx.patch(c.id, { task_id: v || null })}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_risk', { defaultValue: 'Risque couvert' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={c.risk_id ?? ''} options={ctx.riskOptions}
              onChange={v => ctx.patch(c.id, { risk_id: v || null })}
            />
          </div>
        </div>
      )}
    </div>
  )
}

/** What has been promised to be paid, and what has been. */
function PaymentsBand({ line, ctx }: { line: ProcurementLine; ctx: CardCtx }) {
  const { t } = ctx
  const c = line.contract
  const value = c.value
  const scheduled = line.payments.reduce((sum, p) => p.status === 'cancelled' ? sum : sum + p.amount, 0)

  return (
    <div className="border-t border-border bg-surface-1">
      <div className="px-4 py-3">
        <div className="flex items-start gap-3 flex-wrap mb-2">
          <div className="min-w-0 flex-1">
            <h3 className="text-sm font-medium text-text-primary inline-flex items-center gap-1.5">
              <Banknote size={14} className="text-text-tertiary" />
              {t('proj_prc_payments', { defaultValue: 'Échéances de paiement' })}
            </h3>
            {line.overdue_payments > 0 && (
              <p className="text-xs mt-0.5" style={{ color: 'var(--color-danger)' }}>
                {t('proj_prc_overdue_line', {
                  defaultValue: '{{n}} échéance(s) dépassée(s) et toujours non payée(s).',
                  n: line.overdue_payments,
                })}
              </p>
            )}
          </div>
          {ctx.canEdit && (
            <Button size="sm" variant="secondary" icon={<Plus size={14} />}
              onClick={() => ctx.openPayment(c.id, null)}>
              {t('proj_prc_pay_add', { defaultValue: 'Ajouter une échéance' })}
            </Button>
          )}
        </div>

        {/* Paid, invoiced and remaining come from the server: recomputing them
            here would eventually disagree with the register itself. */}
        <div className="flex items-center gap-x-5 gap-y-1 flex-wrap text-sm mb-2">
          <span className="text-text-secondary">
            {t('proj_prc_paid_of', { defaultValue: 'Payé' })}{' '}
            <span className="font-medium tabular-nums text-text-primary">{ctx.money(line.paid)}</span>
          </span>
          <span className="text-text-secondary">
            {t('proj_prc_invoiced', { defaultValue: 'Facturé, non payé' })}{' '}
            <span className="font-medium tabular-nums text-text-primary">{ctx.money(line.invoiced)}</span>
          </span>
          <span className="text-text-secondary">
            {t('proj_prc_remaining', { defaultValue: 'Reste à payer' })}{' '}
            {line.remaining === null ? (
              <span className="italic text-text-tertiary">
                {t('proj_prc_remaining_unknown', { defaultValue: 'inconnu — le contrat n’a pas de montant' })}
              </span>
            ) : (
              <span className="font-medium tabular-nums text-text-primary">{ctx.money(line.remaining)}</span>
            )}
          </span>
        </div>

        {value !== null && value > 0 && (
          // Explicit `primary`: `auto` would turn a contract paid at 95 % red,
          // and paying a supplier what was agreed is not a danger.
          <ProgressBar
            value={Math.min(line.paid, value)}
            max={value}
            variant="primary"
            size="sm"
            showValue
            formatValue={paid => t('proj_prc_paid_ratio', {
              defaultValue: '{{paid}} sur {{total}}',
              paid: ctx.money(paid),
              total: ctx.money(value),
            })}
            label={t('proj_prc_paid_label', { defaultValue: 'Payé' })}
          />
        )}

        {value !== null && scheduled > value && (
          <p className="text-xs mt-2" style={{ color: 'var(--color-warning)' }}>
            {t('proj_prc_schedule_over', {
              defaultValue: 'Les échéances totalisent {{scheduled}}, soit plus que le montant engagé ({{value}}).',
              scheduled: ctx.money(scheduled), value: ctx.money(value),
            })}
          </p>
        )}
      </div>

      {line.payments.length === 0 ? (
        <p className="px-4 pb-3 text-sm text-text-secondary">
          {t('proj_prc_pay_empty', {
            defaultValue: 'Aucune échéance : rien ne dit quand ce contrat sera payé, ni sur quoi la facture sera adossée.',
          })}
        </p>
      ) : (
        <div>
          {line.payments.map(p => (
            <PaymentRow key={p.id} payment={p} contractId={c.id} ctx={ctx} />
          ))}
        </div>
      )}

      {ctx.canEdit && line.payments.length > 0 && (
        <p className="px-4 py-2 text-xs text-text-tertiary border-t border-border">
          {t('proj_prc_pay_paid_hint', {
            defaultValue: 'Passer une échéance à « payée » l’horodate ; en repartir efface la date de paiement.',
          })}
        </p>
      )}
    </div>
  )
}

/** Closed out: what the supplier was actually worth, said while it is still fresh. */
function ClosureBand({ line, ctx }: { line: ProcurementLine; ctx: CardCtx }) {
  const { t } = ctx
  const c = line.contract
  const editable = ctx.canEdit
  return (
    <div className="px-4 py-3 border-t border-border">
      <div className={ctx.isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-6 items-start'}>
        <RecordField
          label={t('proj_prc_performance', { defaultValue: 'Appréciation de la prestation' })}
          icon={<CircleCheckBig size={14} className="text-text-tertiary" />}
          hint={t('proj_prc_performance_hint', {
            defaultValue: 'Ce que ce fournisseur vaut réellement — la seule chose qui servira au prochain projet qui envisagera de l’appeler.',
          })}
          value={c.performance_note}
          rows={3}
          placeholder={t('proj_prc_performance_ph', { defaultValue: 'Délais tenus, qualité, réactivité, ce qu’il a fallu reprendre…' })}
          disabled={!editable}
          columns={!ctx.isMobile}
          emptyLabel={t('proj_prc_performance_empty', { defaultValue: 'Aucune appréciation consignée.' })}
          onCommit={v => ctx.patch(c.id, { performance_note: v })}
        />
        <RecordField
          label={t('proj_prc_closure', { defaultValue: 'Note de clôture' })}
          icon={<Lock size={14} className="text-text-tertiary" />}
          value={c.closure_note}
          rows={3}
          placeholder={t('proj_prc_closure_ph', { defaultValue: 'Réserves levées, soldes, garanties encore courantes…' })}
          disabled={!editable}
          columns={!ctx.isMobile}
          emptyLabel={t('proj_prc_closure_empty', { defaultValue: 'Aucune note de clôture.' })}
          onCommit={v => ctx.patch(c.id, { closure_note: v })}
        />
      </div>
      {editable && (
        <div className={ctx.isMobile ? 'mt-3' : 'mt-3 grid grid-cols-3 gap-3'}>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('proj_prc_closed_on', { defaultValue: 'Date de clôture' })}
            </label>
            <Input
              type="date"
              value={c.closed_on ? c.closed_on.slice(0, 10) : ''}
              onChange={e => ctx.patch(c.id, { closed_on: e.target.value || null })}
            />
          </div>
        </div>
      )}
      {!editable && c.closed_on && (
        <p className="text-xs text-text-secondary mt-2 inline-flex items-center gap-1.5">
          <Lock size={12} className="text-text-tertiary" />
          {t('proj_prc_closed_on_read', { defaultValue: 'Clôturé le {{date}}', date: ctx.fmtDate(c.closed_on) })}
        </p>
      )}
    </div>
  )
}

/** One contract, read as a record: who carries the risk, what was bought, what
 *  remains to be paid, what the supplier was worth. */
function ContractCard({ line, ctx }: { line: ProcurementLine; ctx: CardCtx }) {
  const { t } = ctx
  const c = line.contract
  const engaged = isEngaged(c.status)
  const flashing = ctx.flashId === c.id
  const showClosure = c.status === 'closed' || c.status === 'active' || c.status === 'cancelled'
    || c.performance_note.trim() !== '' || c.closure_note.trim() !== ''

  return (
    <div
      ref={el => { ctx.registerCard(c.id, el) }}
      className="bg-surface-0 border rounded-xl overflow-hidden scroll-mt-4"
      style={{
        borderColor: flashing ? 'var(--color-primary)'
          : isUncapped(c) ? 'var(--color-warning)' : 'var(--color-border)',
        boxShadow: flashing ? '0 0 0 3px color-mix(in srgb, var(--color-primary) 28%, transparent)' : undefined,
      }}
    >
      <div className="px-4 py-3 border-b border-border bg-surface-1 flex items-start gap-3 flex-wrap">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap mb-0.5">
            <span className="font-mono text-xs text-text-tertiary">{c.code || '—'}</span>
            <span className="inline-flex">
              <Badge variant={STATUS_VARIANT[c.status]} dot>{ctx.statusLabel(c.status)}</Badge>
            </span>
            {/* Neutral on purpose: the type places a risk, it does not grade one. */}
            <Tooltip label={riskSentence(t, line.risk_side, c.not_to_exceed !== null)}>
              <span className="inline-flex">
                <Badge variant="neutral">{ctx.typeLabel(c.contract_type)}</Badge>
              </span>
            </Tooltip>
            <span
              className="inline-flex items-center gap-1.5 text-xs px-2 py-0.5 rounded-full border border-border text-text-primary"
              style={{ background: riskTint(line.risk_side) }}
            >
              {/* The intensity lives in the swatch, never under the label. */}
              <span
                className="w-2 h-2 rounded-full shrink-0 border border-border"
                style={{ background: riskFill(line.risk_side) }}
              />
              {ctx.riskLabel(line.risk_side)}
            </span>
            {line.overdue_payments > 0 && (
              <span className="inline-flex">
                <Badge variant="danger" dot>
                  {t('proj_prc_overdue_badge', { defaultValue: '{{n}} en retard', n: line.overdue_payments })}
                </Badge>
              </span>
            )}
          </div>
          {ctx.canEdit ? (
            <Input
              key={`title-${c.title}`}
              defaultValue={c.title}
              className="font-medium"
              onBlur={e => {
                const next = e.target.value.trim()
                if (!next) { e.target.value = c.title; return }
                if (next !== c.title) ctx.patch(c.id, { title: next })
              }}
            />
          ) : (
            <h2 className="text-base font-medium text-text-primary">{c.title}</h2>
          )}
        </div>
        {ctx.canEdit && (
          engaged ? (
            <Tooltip label={t('proj_prc_delete_locked_tip', {
              defaultValue: 'Un contrat attribué, actif ou clos ne se supprime pas : le registre doit garder la trace de ce qui a été engagé. Passez-le au statut « annulé ».',
            })}>
              <span className="inline-flex p-1.5 text-text-tertiary opacity-50">
                <Lock size={15} />
              </span>
            </Tooltip>
          ) : (
            <button
              type="button"
              onClick={() => ctx.askDelete(c)}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={15} />
            </button>
          )
        )}
      </div>

      <RiskBand line={line} ctx={ctx} />
      <ContractBand line={line} ctx={ctx} />
      <PaymentsBand line={line} ctx={ctx} />
      {showClosure && <ClosureBand line={line} ctx={ctx} />}
    </div>
  )
}

// ── The screen ───────────────────────────────────────────────────────────────

interface PayDraft {
  label: string
  due_on: string
  amount: string
  status: PaymentStatus
  invoice_ref: string
}
const EMPTY_PAY: PayDraft = { label: '', due_on: '', amount: '', status: 'planned', invoice_ref: '' }

interface CreateDraft {
  title: string
  supplier_name: string
  contract_type: ContractType
  value: string
  statement_of_work: string
  make_or_buy_note: string
}
const EMPTY_CREATE: CreateDraft = {
  title: '', supplier_name: '', contract_type: 'fixed_price', value: '',
  statement_of_work: '', make_or_buy_note: '',
}

export default function ProcurementView({
  projectId, canEdit = true, onOpenTask, onOpenRisks, onOpenStakeholders,
}: {
  projectId: string
  /** False in the mobile reading mode, where the register is read, not kept. */
  canEdit?: boolean
  /** Opens the work package a contract is attached to. */
  onOpenTask?: (taskId: string) => void
  /** Jumps to the risk register — a contract is often the answer to a risk. */
  onOpenRisks?: () => void
  /** Jumps to the stakeholder register — the person who follows the supplier. */
  onOpenStakeholders?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [statusFilter, setStatusFilter] = useState<string>('')
  const [flashId, setFlashId] = useState<string | null>(null)

  const [createOpen, setCreateOpen] = useState(false)
  const [createDraft, setCreateDraft] = useState<CreateDraft>(EMPTY_CREATE)

  /** Awarding needs a value: rather than suffer the refusal, the amount is asked
   *  for and sent WITH the status, in one PATCH. */
  const [awardTarget, setAwardTarget] = useState<{ contract: Procurement; status: ProcurementStatus } | null>(null)
  const [awardValue, setAwardValue] = useState('')

  const [payTarget, setPayTarget] = useState<{ contractId: string; payment: ProcurementPayment | null } | null>(null)
  const [payDraft, setPayDraft] = useState<PayDraft>(EMPTY_PAY)

  const cardRefs = useRef(new Map<string, HTMLDivElement>())
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => { if (flashTimer.current) clearTimeout(flashTimer.current) }, [])

  const today = useMemo(() => startOfDay(new Date()), [])

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['procurement', projectId],
    queryFn: () => projectsApi.getProcurement(projectId),
  })
  // Shared cache keys with the other registers, so both screens read one entry.
  const { data: holders } = useQuery({
    queryKey: ['stakeholders', projectId],
    queryFn: () => projectsApi.getStakeholders(projectId),
  })
  const { data: deliverables } = useQuery({
    queryKey: ['deliverables', projectId],
    queryFn: () => projectsApi.listDeliverables(projectId),
  })
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })
  const { data: risks } = useQuery({
    queryKey: ['risks', projectId],
    queryFn: () => projectsApi.getRisks(projectId),
  })
  // Read only for its currency: an amount is meaningless unsigned.
  const { data: costs } = useQuery({
    queryKey: ['costs', projectId],
    queryFn: () => projectsApi.getCosts(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['procurement', projectId] })
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const done = () => { setErrorMsg(null); inval() }

  const createMut = useMutation({
    mutationFn: (payload: ProcurementEdit & { title: string }) => projectsApi.createProcurement(projectId, payload),
    onSuccess: contract => {
      done()
      setCreateOpen(false)
      setCreateDraft(EMPTY_CREATE)
      focusContract(contract.id)
    },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, body }: { id: string; body: ProcurementEdit }) =>
      projectsApi.updateProcurement(projectId, id, body),
    onSuccess: done,
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteProcurement(projectId, id),
    onSuccess: done,
    onError: fail,
  })
  const addPayMut = useMutation({
    mutationFn: ({ contractId, body }: { contractId: string; body: PaymentEdit & { amount: number } }) =>
      projectsApi.addPayment(projectId, contractId, body),
    onSuccess: () => { done(); setPayTarget(null) },
    onError: fail,
  })
  const updatePayMut = useMutation({
    mutationFn: ({ contractId, paymentId, body }: { contractId: string; paymentId: string; body: PaymentEdit }) =>
      projectsApi.updatePayment(projectId, contractId, paymentId, body),
    onSuccess: () => { done(); setPayTarget(null) },
    onError: fail,
  })
  const deletePayMut = useMutation({
    mutationFn: ({ contractId, paymentId }: { contractId: string; paymentId: string }) =>
      projectsApi.deletePayment(projectId, contractId, paymentId),
    onSuccess: done,
    onError: fail,
  })

  const patch = (id: string, body: ProcurementEdit) => updateMut.mutate({ id, body })

  const lines = useMemo(() => data?.contracts ?? [], [data])
  const summary = data?.summary
  const uncapped = useMemo(() => data?.uncapped ?? [], [data])

  // ── Labels ────────────────────────────────────────────────────────────────

  const statusLabel = (s: ProcurementStatus) => ({
    planned:   t('proj_prc_status_planned', { defaultValue: 'Prévu' }),
    tendering: t('proj_prc_status_tendering', { defaultValue: 'Consultation' }),
    awarded:   t('proj_prc_status_awarded', { defaultValue: 'Attribué' }),
    active:    t('proj_prc_status_active', { defaultValue: 'En cours' }),
    closed:    t('proj_prc_status_closed', { defaultValue: 'Clos' }),
    cancelled: t('proj_prc_status_cancelled', { defaultValue: 'Annulé' }),
  })[s]

  const typeLabel = (c: ContractType) => ({
    fixed_price:         t('proj_prc_type_fixed', { defaultValue: 'Forfait' }),
    fixed_incentive:     t('proj_prc_type_fixed_inc', { defaultValue: 'Forfait avec intéressement' }),
    cost_plus_fee:       t('proj_prc_type_cpf', { defaultValue: 'Régie avec honoraires' }),
    cost_plus_incentive: t('proj_prc_type_cpi', { defaultValue: 'Régie avec intéressement' }),
    time_material:       t('proj_prc_type_tm', { defaultValue: 'Régie (temps passé)' }),
    other:               t('proj_prc_type_other', { defaultValue: 'Autre' }),
  })[c]

  const paymentStatusLabel = (s: PaymentStatus) => ({
    planned:   t('proj_prc_pay_planned', { defaultValue: 'Prévue' }),
    invoiced:  t('proj_prc_pay_invoiced', { defaultValue: 'Facturée' }),
    paid:      t('proj_prc_pay_paid', { defaultValue: 'Payée' }),
    disputed:  t('proj_prc_pay_disputed', { defaultValue: 'Contestée' }),
    cancelled: t('proj_prc_pay_cancelled', { defaultValue: 'Annulée' }),
  })[s]

  /** Who absorbs the overrun, in three words — the sentence says the rest. */
  const riskLabel = (s: RiskSide) => ({
    supplier: t('proj_prc_side_supplier', { defaultValue: 'Risque fournisseur' }),
    buyer:    t('proj_prc_side_buyer', { defaultValue: 'Risque projet' }),
    shared:   t('proj_prc_side_shared', { defaultValue: 'Risque partagé' }),
    unknown:  t('proj_prc_side_unknown', { defaultValue: 'Risque indéterminé' }),
  })[s]

  // ── Options ───────────────────────────────────────────────────────────────

  const statusOptions: DropdownOption[] = useMemo(
    () => STATUSES.map(s => ({ value: s, label: statusLabel(s) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )
  const filterOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_prc_filter_all', { defaultValue: 'Tous les statuts' }) },
    ...statusOptions,
  ], [statusOptions, t])

  const typeOptions: DropdownOption[] = useMemo(
    () => CONTRACT_TYPES.map(c => ({ value: c, label: typeLabel(c) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )
  const paymentStatusOptions: DropdownOption[] = useMemo(
    () => PAYMENT_STATUSES.map(s => ({ value: s, label: paymentStatusLabel(s) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  )

  const stakeholderOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_prc_no_holder', { defaultValue: '— Non précisé —' }) },
    ...(holders?.stakeholders ?? []).map(s => ({
      value: s.id,
      label: s.role_title ? `${s.name} — ${s.role_title}` : s.name,
    })),
  ], [holders, t])

  const deliverableOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_prc_no_deliverable', { defaultValue: '— Aucun livrable —' }) },
    ...(deliverables ?? []).map(d => ({ value: d.id, label: `${d.code} ${d.name}`.trim() })),
  ], [deliverables, t])

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_prc_no_task', { defaultValue: '— Aucun lot —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])

  const riskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_prc_no_risk', { defaultValue: '— Aucun risque —' }) },
    ...(risks?.risks ?? []).map(r => ({ value: r.id, label: `${r.code} ${r.title}`.trim() })),
  ], [risks, t])

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

  const share = useMemo(() => {
    const nf = new Intl.NumberFormat(i18n.language, { style: 'percent', maximumFractionDigits: 0 })
    return (ratio: number) => nf.format(ratio)
  }, [i18n.language])

  // ── Navigation between cards ──────────────────────────────────────────────

  const registerCard = (id: string, el: HTMLDivElement | null) => {
    if (el) cardRefs.current.set(id, el); else cardRefs.current.delete(id)
  }

  /** Jumps to a record and rings it briefly — a silent scroll leaves the reader
   *  hunting for what just moved. */
  const focusContract = (id: string) => {
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

  /** The server refuses to award a contract with no value. Rather than let the
   *  refusal happen, the amount is asked for and sent with the status. */
  const changeStatus = (c: Procurement, next: ProcurementStatus) => {
    if (next === c.status) return
    if (needsValue(next) && c.value === null) {
      setAwardTarget({ contract: c, status: next })
      setAwardValue('')
      return
    }
    patch(c.id, { status: next })
  }

  const awardAmount = numOrNull(awardValue)
  const canAward = awardTarget !== null && awardAmount !== null && awardAmount >= 0

  const submitAward = () => {
    if (!awardTarget || !canAward || awardAmount === null) return
    patch(awardTarget.contract.id, { status: awardTarget.status, value: awardAmount })
    setAwardTarget(null)
  }

  const askDelete = (c: Procurement) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_prc_delete_title', { defaultValue: 'Supprimer le contrat ?' }),
        message: t('proj_prc_delete_msg', {
          defaultValue: '« {{title}} » disparaîtra du registre, avec ses échéances de paiement. Rien n’ayant encore été engagé, il n’y a rien à conserver.',
          title: c.title,
        }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      })
      if (ok) deleteMut.mutate(c.id)
    })()
  }

  const openPayment = (contractId: string, payment: ProcurementPayment | null) => {
    setPayTarget({ contractId, payment })
    setPayDraft(payment ? {
      label:       payment.label,
      due_on:      payment.due_on ? payment.due_on.slice(0, 10) : '',
      amount:      String(payment.amount),
      status:      payment.status,
      invoice_ref: payment.invoice_ref,
    } : EMPTY_PAY)
  }

  const payAmount = numOrNull(payDraft.amount)
  const canSavePayment = payTarget !== null && payAmount !== null

  const submitPayment = () => {
    if (!payTarget || !canSavePayment || payAmount === null) return
    const body = {
      label:       payDraft.label.trim(),
      due_on:      payDraft.due_on || null,
      amount:      payAmount,
      status:      payDraft.status,
      invoice_ref: payDraft.invoice_ref.trim(),
    }
    if (payTarget.payment) {
      updatePayMut.mutate({ contractId: payTarget.contractId, paymentId: payTarget.payment.id, body })
    } else {
      addPayMut.mutate({ contractId: payTarget.contractId, body })
    }
  }

  const askDeletePayment = (contractId: string, p: ProcurementPayment) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_prc_pay_delete_title', { defaultValue: 'Supprimer l’échéance ?' }),
        message: t('proj_prc_pay_delete_msg', {
          defaultValue: '« {{label}} » ({{amount}}) sera retirée de l’échéancier du contrat.',
          label: p.label || t('proj_prc_pay_untitled', { defaultValue: 'Échéance sans libellé' }),
          amount: money(p.amount),
        }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      })
      if (ok) deletePayMut.mutate({ contractId, paymentId: p.id })
    })()
  }

  const setPaymentStatus = (contractId: string, p: ProcurementPayment, next: PaymentStatus) => {
    if (next === p.status) return
    // `paid_on` is the server's business: it stamps it on `paid` and clears it
    // on the way out. Sending one from here would fight it.
    updatePayMut.mutate({ contractId, paymentId: p.id, body: { status: next } })
  }

  const canCreate = createDraft.title.trim().length > 0
  const submitCreate = () => {
    const title = createDraft.title.trim()
    if (!title) return
    createMut.mutate({
      title,
      supplier_name:     createDraft.supplier_name.trim(),
      contract_type:     createDraft.contract_type,
      value:             numOrNull(createDraft.value),
      statement_of_work: createDraft.statement_of_work.trim(),
      make_or_buy_note:  createDraft.make_or_buy_note.trim(),
    })
  }

  // ── The headline: who carries the money ───────────────────────────────────

  /** Contracts of an undeclared type: their money sits in the total but in none
   *  of the three buckets, and pretending otherwise would misstate the exposure. */
  const unknownCommitted = useMemo(() => {
    if (!summary) return 0
    const c = summary.committed
    return Math.max(0, c.total - c.supplier_risk - c.buyer_risk - c.shared_risk)
  }, [summary])

  const segments: Segment[] = useMemo(() => {
    if (!summary) return []
    return [
      { side: 'supplier', amount: summary.committed.supplier_risk, label: riskLabel('supplier') },
      { side: 'shared',   amount: summary.committed.shared_risk,   label: riskLabel('shared') },
      { side: 'buyer',    amount: summary.committed.buyer_risk,    label: riskLabel('buyer') },
      { side: 'unknown',  amount: unknownCommitted,                label: riskLabel('unknown') },
    ]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [summary, unknownCommitted, t])

  /** The sentence that has to be read before any figure: a single "committed"
   *  total describes an exposure the project does not actually carry. */
  const committedSentence = useMemo(() => {
    if (!summary) return null
    const c = summary.committed
    if (c.total === 0) {
      return t('proj_prc_committed_none', {
        defaultValue: 'Aucun montant n’est encore engagé : rien n’a été attribué, ou rien n’a été chiffré.',
      })
    }
    const parts: string[] = []
    if (c.supplier_risk > 0) {
      parts.push(t('proj_prc_part_supplier', {
        defaultValue: '{{amount}} sont portés par le fournisseur (prix ferme)',
        amount: money(c.supplier_risk),
      }))
    }
    if (c.shared_risk > 0) {
      parts.push(t('proj_prc_part_shared', {
        defaultValue: '{{amount}} sont partagés (temps passé)',
        amount: money(c.shared_risk),
      }))
    }
    if (c.buyer_risk > 0) {
      parts.push(t('proj_prc_part_buyer', {
        defaultValue: '{{amount}} restent à la charge du projet (coûts remboursés)',
        amount: money(c.buyer_risk),
      }))
    }
    if (unknownCommitted > 0) {
      parts.push(t('proj_prc_part_unknown', {
        defaultValue: '{{amount}} relèvent de contrats dont le type ne dit pas qui absorbe un dépassement',
        amount: money(unknownCommitted),
      }))
    }
    return t('proj_prc_committed_sentence', {
      defaultValue: 'Sur {{total}} engagés, {{parts}}.',
      total: money(c.total),
      parts: joinParts(parts, t('common_and', { defaultValue: 'et' })),
    })
  }, [summary, unknownCommitted, money, t])

  /** The follow-up: what proportion of the money the project itself is on the
   *  hook for. It is the number nobody keeps, and the one that decides whether
   *  a bad estimate becomes the project's problem. */
  const exposureSentence = useMemo(() => {
    if (!summary || summary.committed.total === 0) return null
    const c = summary.committed
    const exposed = c.buyer_risk + c.shared_risk + unknownCommitted
    if (exposed === 0) {
      return t('proj_prc_exposure_none', {
        defaultValue: 'Tout est au forfait : une mauvaise estimation coûtera au fournisseur, pas au projet.',
      })
    }
    if (c.supplier_risk === 0) {
      return t('proj_prc_exposure_all', {
        defaultValue: 'Aucun de ces contrats ne transfère le risque : une mauvaise estimation sera intégralement payée par le projet.',
      })
    }
    return t('proj_prc_exposure_mixed', {
      defaultValue: '{{ratio}} de l’engagement reste porté par le projet : c’est cette part-là, et non le total, qui grossira si les estimations se révèlent fausses.',
      ratio: share(exposed / c.total),
    })
  }, [summary, unknownCommitted, share, t])

  const ctx: CardCtx = {
    t, canEdit, isMobile, today, patch, changeStatus, askDelete,
    openPayment, askDeletePayment, setPaymentStatus,
    fmtDate, money, statusLabel, typeLabel, paymentStatusLabel, riskLabel,
    typeOptions, statusOptions, paymentStatusOptions,
    stakeholderOptions, deliverableOptions, taskOptions, riskOptions,
    registerCard, flashId, onOpenTask, onOpenRisks, onOpenStakeholders,
  }

  const visible = statusFilter ? lines.filter(l => l.contract.status === statusFilter) : lines
  const awardContract = awardTarget?.contract ?? null

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <ShoppingCart size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_prc_title', { defaultValue: 'Contrats et achats' })}
          </h1>
          <div className="flex-1" />
          {lines.length > 0 && (
            <Dropdown
              height={36} fontSize={14} focusable width={200}
              value={statusFilter} options={filterOptions}
              onChange={setStatusFilter}
            />
          )}
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_prc_new', { defaultValue: 'Nouveau contrat' })}
            </Button>
          )}
        </div>

        {/* The committed total NEVER appears without its split: 99 000 € dont
            48 000 € sur le dos du fournisseur n'est pas la même exposition que
            99 000 € en régie, et un chiffre unique efface exactement ça. */}
        {summary && summary.total > 0 && committedSentence && (
          <div className="rounded-xl border border-border bg-surface-0 px-4 py-4">
            <div className="flex items-start gap-3">
              <Scale size={18} className="shrink-0 mt-[3px] text-text-tertiary" />
              <div className="min-w-0 flex-1">
                <p className="text-base font-medium text-text-primary leading-relaxed">{committedSentence}</p>
                {exposureSentence && (
                  <p className="text-sm text-text-secondary mt-1 leading-relaxed">{exposureSentence}</p>
                )}
                <CommittedSplit
                  segments={segments}
                  total={summary.committed.total}
                  money={money}
                  share={share}
                />
              </div>
            </div>
          </div>
        )}

        {summary && summary.total > 0 && (
          <div className={isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
            <Stat label={t('proj_prc_stat_total', { defaultValue: 'Contrats' })}
              value={summary.total} icon={<Handshake size={14} />} />
            <Stat label={t('proj_prc_stat_open', { defaultValue: 'En cours' })}
              value={summary.open} icon={<Truck size={14} />} />
            <Stat label={t('proj_prc_stat_paid', { defaultValue: 'Déjà payé' })}
              value={money(summary.paid)} tone="success" icon={<Banknote size={14} />} />
            <Stat label={t('proj_prc_stat_overdue', { defaultValue: 'Échéances en retard' })}
              value={summary.overdue_payments}
              tone={summary.overdue_payments > 0 ? 'danger' : 'muted'}
              icon={<CalendarClock size={14} />} />
            <Stat label={t('proj_prc_stat_unpriced', { defaultValue: 'Sans montant' })}
              value={summary.unpriced}
              tone={summary.unpriced > 0 ? 'warning' : 'muted'}
              icon={<Coins size={14} />} />
          </div>
        )}

        {/* An absence, not an amount: it cannot be read off any total. */}
        {uncapped.length > 0 && (
          <Callout
            variant="warning"
            icon={<InfinityIcon size={16} />}
            title={t('proj_prc_uncapped_title', {
              defaultValue: '{{n}} contrat(s) en régie sans plafond',
              n: uncapped.length,
            })}
          >
            <div className="space-y-2">
              <p className="text-sm">
                {t('proj_prc_uncapped_msg', {
                  defaultValue: 'Ces contrats paient le temps passé et aucun montant maximal n’a été fixé : rien ne borne ce qu’ils peuvent coûter. Ce n’est pas une ligne élevée dans le registre, c’est une ligne qui n’a pas de fin — elle ne se verra dans aucun total tant que la facture n’est pas arrivée.',
                })}
              </p>
              <div className="flex flex-wrap gap-2">
                {uncapped.map(u => (
                  <button
                    key={u.id}
                    type="button"
                    onClick={() => focusContract(u.id)}
                    className="inline-flex items-center gap-1.5 text-sm px-2 py-1 rounded border border-border bg-surface-0 hover:bg-surface-2 text-text-primary"
                  >
                    {u.code && <span className="font-mono text-xs text-text-tertiary">{u.code}</span>}
                    <span className="truncate max-w-[18rem]">{u.title}</span>
                    {u.supplier && <span className="text-xs text-text-tertiary">· {u.supplier}</span>}
                    <ArrowUpRight size={13} className="text-text-tertiary shrink-0" />
                  </button>
                ))}
              </div>
            </div>
          </Callout>
        )}

        {summary && summary.unpriced > 0 && (
          <Callout
            variant="warning"
            icon={<Coins size={16} />}
            title={t('proj_prc_unpriced_title', {
              defaultValue: '{{n}} contrat(s) en cours sans montant',
              n: summary.unpriced,
            })}
          >
            <span className="text-sm">
              {t('proj_prc_unpriced_msg', {
                defaultValue: 'Ces contrats engagent le projet, mais personne n’a écrit combien : ils ne figurent dans aucun total, et le montant réellement engagé est donc supérieur à celui affiché ci-dessus. Renseignez leur montant — le serveur l’exigera de toute façon pour les attribuer.',
              })}
            </span>
          </Callout>
        )}

        {summary && summary.overdue_payments > 0 && (
          <Callout
            variant="danger"
            icon={<CalendarClock size={16} />}
            title={t('proj_prc_overdue_title', {
              defaultValue: '{{n}} échéance(s) de paiement en retard',
              n: summary.overdue_payments,
            })}
          >
            <span className="text-sm">
              {t('proj_prc_overdue_msg', {
                defaultValue: 'La date est passée et l’échéance n’est toujours pas payée. Un fournisseur impayé arrête de livrer bien avant d’envoyer une relance.',
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
              icon={<ShoppingCart size={26} />}
              title={t('proj_prc_load_error', { defaultValue: 'Le registre des achats n’a pas pu être chargé.' })}
              action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
              t={t}
            />
          </div>
        ) : isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-8 text-center text-sm text-text-secondary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : lines.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="first-use"
              icon={<ShoppingCart size={26} />}
              title={t('proj_prc_empty_title', { defaultValue: 'Aucun contrat' })}
              description={t('proj_prc_empty', {
                defaultValue: 'Un projet n’achète presque jamais « pour un montant » : il achète sous un type de contrat, et c’est ce type qui décide qui paiera quand l’estimation se révélera fausse. Consignez ici chaque contrat, son type, son échéancier — et la raison pour laquelle ce travail est acheté plutôt que fait en interne.',
              })}
              action={canEdit ? {
                label: t('proj_prc_new', { defaultValue: 'Nouveau contrat' }),
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
              icon={<ShoppingCart size={26} />}
              title={t('proj_prc_filter_empty', { defaultValue: 'Aucun contrat à ce statut' })}
              action={{
                label: t('proj_prc_filter_clear', { defaultValue: 'Afficher tous les statuts' }),
                onClick: () => setStatusFilter(''),
              }}
              t={t}
            />
          </div>
        ) : (
          // The server's order is kept: re-sorting here would bury exactly what
          // it chose to put on top.
          <div className="space-y-4">
            {visible.map(l => <ContractCard key={l.contract.id} line={l} ctx={ctx} />)}
          </div>
        )}
      </div>

      {/* ── Nouveau contrat ── */}
      {createOpen && (
        <FloatingWindow
          title={t('proj_prc_new', { defaultValue: 'Nouveau contrat' })}
          icon={<ShoppingCart size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={600} defaultHeight={600} padding={16} t={t}
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
                {t('proj_prc_col_title', { defaultValue: 'Objet du contrat' })}
              </label>
              <Input
                autoFocus
                value={createDraft.title}
                placeholder={t('proj_prc_title_ph', { defaultValue: 'Ce qui est acheté, en une phrase…' })}
                onChange={e => setCreateDraft(d => ({ ...d, title: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); submitCreate() } }}
              />
            </div>
            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_supplier', { defaultValue: 'Fournisseur' })}
                </label>
                <Input
                  value={createDraft.supplier_name}
                  placeholder={t('proj_prc_supplier_ph', { defaultValue: 'Raison sociale…' })}
                  onChange={e => setCreateDraft(d => ({ ...d, supplier_name: e.target.value }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_type', { defaultValue: 'Type de contrat' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={createDraft.contract_type} options={typeOptions}
                  onChange={v => setCreateDraft(d => ({ ...d, contract_type: v as ContractType }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_value', { defaultValue: 'Montant engagé' })}
                </label>
                <Input
                  type="number"
                  step="100"
                  value={createDraft.value}
                  placeholder={t('proj_prc_value_ph', { defaultValue: 'ex. 48000' })}
                  onChange={e => setCreateDraft(d => ({ ...d, value: e.target.value }))}
                />
              </div>
            </div>
            <p className="text-xs text-text-tertiary">
              {riskSentence(t, createDraft.contract_type === 'fixed_price' || createDraft.contract_type === 'fixed_incentive'
                ? 'supplier'
                : createDraft.contract_type === 'time_material'
                  ? 'shared'
                  : createDraft.contract_type === 'other'
                    ? 'unknown'
                    : 'buyer',
              false)}
            </p>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_prc_sow', { defaultValue: 'Énoncé des travaux' })}
              </label>
              <Textarea
                rows={3}
                className="h-auto min-h-0 resize-y"
                value={createDraft.statement_of_work}
                placeholder={t('proj_prc_sow_ph', { defaultValue: 'Ce qui est acheté, précisément…' })}
                onChange={e => setCreateDraft(d => ({ ...d, statement_of_work: e.target.value }))}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_prc_make_or_buy', { defaultValue: 'Faire ou faire faire' })}
              </label>
              <Textarea
                rows={3}
                className="h-auto min-h-0 resize-y"
                value={createDraft.make_or_buy_note}
                placeholder={t('proj_prc_make_or_buy_ph', { defaultValue: 'Ce qui a fait pencher pour l’achat : compétence, délai, capacité, coût…' })}
                onChange={e => setCreateDraft(d => ({ ...d, make_or_buy_note: e.target.value }))}
              />
              <p className="text-xs text-text-tertiary mt-1">
                {t('proj_prc_create_hint', {
                  defaultValue: 'Le contrat démarre au statut « Prévu ». Le montant peut attendre — mais il devient obligatoire pour l’attribuer.',
                })}
              </p>
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* ── Attribution : le montant, exigé avant la refus du serveur ── */}
      {awardTarget && awardContract && (
        <FloatingWindow
          title={t('proj_prc_award_title', { defaultValue: 'Attribuer le contrat' })}
          icon={<Handshake size={16} />}
          onClose={() => setAwardTarget(null)}
          defaultWidth={520} defaultHeight={340} padding={16} t={t}
          actions={{
            confirm: {
              label: t('proj_prc_award_confirm', { defaultValue: 'Enregistrer et attribuer' }),
              disabled: !canAward,
              loading: updateMut.isPending,
              autoFocus: true,
              onClick: submitAward,
            },
          }}
        >
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              {awardContract.code ? `${awardContract.code} — ${awardContract.title}` : awardContract.title}
            </p>
            <Callout variant="info" icon={<Coins size={16} />}>
              <span className="text-sm">
                {t('proj_prc_award_msg', {
                  defaultValue: 'Indiquez le montant engagé avant d’attribuer ce contrat : sans lui, l’engagement n’apparaît dans aucun total et personne ne peut dire ce que le projet a promis de payer.',
                })}
              </span>
            </Callout>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_prc_value', { defaultValue: 'Montant engagé' })}
              </label>
              <Input
                autoFocus
                type="number"
                step="100"
                value={awardValue}
                placeholder={t('proj_prc_value_ph', { defaultValue: 'ex. 48000' })}
                onChange={e => setAwardValue(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter' && canAward) { e.preventDefault(); submitAward() } }}
              />
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* ── Échéance de paiement ── */}
      {payTarget && (
        <FloatingWindow
          title={payTarget.payment
            ? t('proj_prc_pay_edit_title', { defaultValue: 'Modifier l’échéance' })
            : t('proj_prc_pay_add_title', { defaultValue: 'Nouvelle échéance de paiement' })}
          icon={<Banknote size={16} />}
          onClose={() => setPayTarget(null)}
          defaultWidth={560} defaultHeight={480} padding={16} t={t}
          actions={{
            confirm: {
              label: payTarget.payment
                ? t('common_save', { defaultValue: 'Enregistrer' })
                : t('common_add', { defaultValue: 'Ajouter' }),
              disabled: !canSavePayment,
              loading: addPayMut.isPending || updatePayMut.isPending,
              autoFocus: true,
              onClick: submitPayment,
            },
          }}
        >
          <div className="space-y-3">
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_prc_pay_label', { defaultValue: 'Libellé' })}
              </label>
              <Input
                autoFocus
                value={payDraft.label}
                placeholder={t('proj_prc_pay_label_ph', { defaultValue: 'Acompte, jalon de recette, solde…' })}
                onChange={e => setPayDraft(d => ({ ...d, label: e.target.value }))}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_pay_due', { defaultValue: 'Échéance' })}
                </label>
                <Input
                  type="date"
                  value={payDraft.due_on}
                  onChange={e => setPayDraft(d => ({ ...d, due_on: e.target.value }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_pay_amount', { defaultValue: 'Montant' })}
                </label>
                <Input
                  type="number"
                  step="100"
                  value={payDraft.amount}
                  placeholder={t('proj_prc_pay_amount_ph', { defaultValue: 'ex. 12000' })}
                  onChange={e => setPayDraft(d => ({ ...d, amount: e.target.value }))}
                />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_pay_status', { defaultValue: 'Statut' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={payDraft.status} options={paymentStatusOptions}
                  onChange={v => setPayDraft(d => ({ ...d, status: v as PaymentStatus }))}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_prc_pay_invoice', { defaultValue: 'Référence de facture' })}
                </label>
                <Input
                  value={payDraft.invoice_ref}
                  placeholder={t('proj_prc_pay_invoice_ph', { defaultValue: 'ex. FA-2027-114' })}
                  onChange={e => setPayDraft(d => ({ ...d, invoice_ref: e.target.value }))}
                />
              </div>
            </div>
            <p className="text-xs text-text-tertiary">
              {t('proj_prc_pay_paid_hint', {
                defaultValue: 'Passer une échéance à « payée » l’horodate ; en repartir efface la date de paiement.',
              })}
            </p>
            {!canSavePayment && (
              <p className="text-xs" style={{ color: 'var(--color-warning)' }}>
                {t('proj_prc_pay_amount_required', {
                  defaultValue: 'Une échéance sans montant ne dit rien : indiquez ce qui sera payé.',
                })}
              </p>
            )}
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
