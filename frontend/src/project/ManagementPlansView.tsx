import React, { useEffect, useMemo, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import type { TFunction } from 'i18next'
import { getDateLocale, useConfirm } from '@kubuno/sdk'
import {
  AlertTriangle, ArrowUpRight, Boxes, CalendarRange, CheckCircle2, CircleDashed, Coins,
  Contact, Gauge, Layers, ListChecks, Megaphone, PenLine, Plus, Replace, Scale,
  ShieldCheck, ShoppingCart, Target, Trash2, Users, Wrench,
} from 'lucide-react'
import {
  Badge, Button, Callout, ConfirmDialog, Dropdown, EmptyState, Input, Textarea, Toggle,
  Tooltip, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type ManagementPlan, type ManagementPlanEdit, type PlanArea, type PlanReviewFrequency,
  type PlanSlot,
} from '../api'

// The subsidiary management plans — how each area of the project is run.
//
// Every other screen of the module answers WHAT: which risks, which change
// requests, which costs. This one answers HOW. That would make it one more form
// among many, except for one thing: FOUR of the figures typed here are read back
// by the other artifacts, which until now judged against a rule hard-coded in
// the module — the same rule for a three-task plan and a construction programme.
//
//   • cost    · variance_threshold_pct   → the cost variance the earned value
//                                          screen calls a drift rather than noise
//   • schedule· variance_threshold_pct   → the same, for the schedule variance
//   • risk    · risk_appetite_score      → the score above which a risk is
//                                          ESCALATED, not merely owned
//   • change  · change_authority_amount  → what the project manager settles
//               change_authority_days      alone; beyond it, the board sits
//
// So the page says it out loud, twice: a summary at the top listing the four
// figures with the screen each one feeds, and — beside every input — the same
// sentence plus a link to that screen. A threshold left unset is not a blank to
// be filled: it means the artifact keeps its own judgement, and that is spelled
// out too, because it is a decision as much as setting one.
//
// TRAP made visible: the server gathers those figures from ACTIVE plans only. A
// threshold typed on a switched-off plan is stored and simply never read; the
// card and the summary both say so rather than letting it look enforced.
//
// The server refuses a threshold offered on a plan that does not read it. The
// interface never puts one there, so that refusal is unreachable from here.

/** The twelve areas, in the reading order of the integrated plan document — not
 *  alphabetical. Only a fallback: the server already returns them ordered. */
const AREA_ORDER: PlanArea[] = [
  'scope', 'requirements', 'schedule', 'cost', 'quality', 'resource',
  'communications', 'risk', 'procurement', 'stakeholder', 'change', 'configuration',
]

const FREQUENCIES: PlanReviewFrequency[] = [
  'weekly', 'biweekly', 'monthly', 'quarterly', 'milestone', 'on_demand',
]

/** The artifact each area is actually run on. Scope, resources and configuration
 *  have no single screen of their own, so they get no link rather than a wrong
 *  one: an arrow that lands nowhere useful is worse than no arrow. */
const AREA_TAB: Partial<Record<PlanArea, string>> = {
  requirements:   'requirements',
  schedule:       'gantt',
  cost:           'costs',
  quality:        'quality',
  communications: 'communications',
  risk:           'risks',
  procurement:    'procurement',
  stakeholder:    'stakeholders',
  change:         'changes',
}

const AREA_ICON: Record<PlanArea, React.ReactNode> = {
  scope:          <Target size={15} />,
  requirements:   <ListChecks size={15} />,
  schedule:       <CalendarRange size={15} />,
  cost:           <Coins size={15} />,
  quality:        <ShieldCheck size={15} />,
  resource:       <Users size={15} />,
  communications: <Megaphone size={15} />,
  risk:           <AlertTriangle size={15} />,
  procurement:    <ShoppingCart size={15} />,
  stakeholder:    <Contact size={15} />,
  change:         <Replace size={15} />,
  configuration:  <Boxes size={15} />,
}

function areaLabel(t: TFunction, area: PlanArea): string {
  switch (area) {
    case 'scope':          return t('proj_plan_area_scope', { defaultValue: 'Périmètre' })
    case 'requirements':   return t('proj_plan_area_requirements', { defaultValue: 'Exigences' })
    case 'schedule':       return t('proj_plan_area_schedule', { defaultValue: 'Échéancier' })
    case 'cost':           return t('proj_plan_area_cost', { defaultValue: 'Coûts' })
    case 'quality':        return t('proj_plan_area_quality', { defaultValue: 'Qualité' })
    case 'resource':       return t('proj_plan_area_resource', { defaultValue: 'Ressources' })
    case 'communications': return t('proj_plan_area_communications', { defaultValue: 'Communications' })
    case 'risk':           return t('proj_plan_area_risk', { defaultValue: 'Risques' })
    case 'procurement':    return t('proj_plan_area_procurement', { defaultValue: 'Approvisionnements' })
    case 'stakeholder':    return t('proj_plan_area_stakeholder', { defaultValue: 'Parties prenantes' })
    case 'change':         return t('proj_plan_area_change', { defaultValue: 'Changements' })
    case 'configuration':  return t('proj_plan_area_configuration', { defaultValue: 'Configuration' })
  }
}

/** One line saying what the area covers — so an area nobody planned still tells
 *  the reader what planning it would mean. */
function areaPurpose(t: TFunction, area: PlanArea): string {
  switch (area) {
    case 'scope':          return t('proj_plan_purpose_scope', { defaultValue: 'Comment le contenu du projet est défini, validé et tenu contre la dérive.' })
    case 'requirements':   return t('proj_plan_purpose_requirements', { defaultValue: 'Comment les exigences sont recueillies, arbitrées et tracées jusqu’aux livrables.' })
    case 'schedule':       return t('proj_plan_purpose_schedule', { defaultValue: 'Comment l’échéancier est bâti, mesuré et remis à jour.' })
    case 'cost':           return t('proj_plan_purpose_cost', { defaultValue: 'Comment le budget est estimé, engagé et suivi.' })
    case 'quality':        return t('proj_plan_purpose_quality', { defaultValue: 'Ce qui est contrôlé, par quel moyen, et ce qui vaut acceptation.' })
    case 'resource':       return t('proj_plan_purpose_resource', { defaultValue: 'Comment l’équipe est constituée, affectée et libérée.' })
    case 'communications': return t('proj_plan_purpose_communications', { defaultValue: 'Qui reçoit quoi, à quelle fréquence et par quel canal.' })
    case 'risk':           return t('proj_plan_purpose_risk', { defaultValue: 'Comment les risques sont identifiés, cotés, traités — et jusqu’où le projet les accepte.' })
    case 'procurement':    return t('proj_plan_purpose_procurement', { defaultValue: 'Ce qui est acheté à l’extérieur, comment et sous quel contrat.' })
    case 'stakeholder':    return t('proj_plan_purpose_stakeholder', { defaultValue: 'Comment les parties prenantes sont identifiées et associées.' })
    case 'change':         return t('proj_plan_purpose_change', { defaultValue: 'Comment une demande de changement est instruite, et qui tranche.' })
    case 'configuration':  return t('proj_plan_purpose_configuration', { defaultValue: 'Comment les versions des documents et des livrables sont identifiées et gelées.' })
  }
}

function freqLabel(t: TFunction, f: PlanReviewFrequency): string {
  switch (f) {
    case 'weekly':    return t('proj_plan_freq_weekly', { defaultValue: 'Chaque semaine' })
    case 'biweekly':  return t('proj_plan_freq_biweekly', { defaultValue: 'Toutes les deux semaines' })
    case 'monthly':   return t('proj_plan_freq_monthly', { defaultValue: 'Chaque mois' })
    case 'quarterly': return t('proj_plan_freq_quarterly', { defaultValue: 'Chaque trimestre' })
    case 'milestone': return t('proj_plan_freq_milestone', { defaultValue: 'À chaque jalon' })
    case 'on_demand': return t('proj_plan_freq_on_demand', { defaultValue: 'À la demande' })
  }
}

/** The three states an area can be in, plus the switched-off case. They look
 *  alike in a list and mean entirely different things — an unplanned area is a
 *  deliberate choice, an active area with no approach is a defect. */
type PlanState = 'unplanned' | 'written' | 'empty' | 'off'

function planState(slot: PlanSlot): PlanState {
  if (!slot.planned || !slot.plan) return 'unplanned'
  if (!slot.plan.is_active) return 'off'
  return slot.plan.approach.trim() ? 'written' : 'empty'
}

const STATE_VARIANT: Record<PlanState, 'neutral' | 'success' | 'warning'> = {
  unplanned: 'neutral',
  written:   'success',
  empty:     'warning',
  off:       'neutral',
}

function stateLabel(t: TFunction, s: PlanState): string {
  switch (s) {
    case 'unplanned': return t('proj_plan_state_unplanned', { defaultValue: 'Non planifié' })
    case 'written':   return t('proj_plan_state_written', { defaultValue: 'Rédigé' })
    case 'empty':     return t('proj_plan_state_empty', { defaultValue: 'Activé, non rédigé' })
    case 'off':       return t('proj_plan_state_off', { defaultValue: 'Désactivé' })
  }
}

// ── The four figures that leave this page ────────────────────────────────────

/** One threshold as the summary shows it: what it is, where it acts, what it is
 *  worth today — and, when it is not set, what the other screen does instead. */
interface AppliedThreshold {
  key:      string
  area:     PlanArea
  tab:      string
  label:    string
  /** Formatted value, or null when the project has not set one. */
  value:    string | null
  /** What the reading artifact does while it is unset. */
  fallback: string
  /** Set — but on a plan that is switched off, so the server never reads it. */
  ignored:  boolean
}

// ── Small pieces ─────────────────────────────────────────────────────────────

/** One prose block of a plan, committed on blur like the rest of the module. */
function ProseField({ label, icon, value, placeholder, rows = 3, readOnly, onCommit }: {
  label: string
  icon?: React.ReactNode
  value: string
  placeholder?: string
  rows?: number
  readOnly?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="flex items-center gap-1.5 text-sm font-medium text-text-secondary mb-1">
        {icon}
        <span>{label}</span>
      </label>
      {readOnly ? (
        value.trim()
          ? <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">{value}</p>
          : <p className="text-sm italic text-text-tertiary">—</p>
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
    </div>
  )
}

/** A number that is read back elsewhere. Committed on blur; cleared by emptying
 *  the field, which is a real answer (« le projet n’a pas fixé de seuil ») and
 *  not the absence of one. */
function ThresholdInput({ label, unit, value, step, placeholder, readOnly, validate, onCommit }: {
  label: string
  unit?: string
  value: number | null
  step?: string
  placeholder?: string
  readOnly?: boolean
  /** Refuses a value the server would refuse, so the refusal never travels. */
  validate?: (n: number) => string | null
  onCommit: (next: number | null) => void
}) {
  const [draft, setDraft] = useState(value === null ? '' : String(value))
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    setDraft(value === null ? '' : String(value))
    setError(null)
  }, [value])

  const commit = () => {
    const raw = draft.trim()
    if (raw === '') {
      setError(null)
      if (value !== null) onCommit(null)
      return
    }
    const next = Number(raw.replace(',', '.'))
    if (!Number.isFinite(next)) { setDraft(value === null ? '' : String(value)); setError(null); return }
    const problem = validate?.(next) ?? null
    setError(problem)
    if (problem) return
    if (next !== value) onCommit(next)
  }

  if (readOnly) {
    return (
      <div>
        <label className="text-xs text-text-tertiary mb-1 block">{label}</label>
        <p className="text-sm text-text-primary tabular-nums">
          {value === null ? '—' : `${value}${unit ? ` ${unit}` : ''}`}
        </p>
      </div>
    )
  }

  return (
    <div>
      <label className="text-xs text-text-tertiary mb-1 block">{label}</label>
      <div className="flex items-center gap-1.5">
        <Input
          type="number"
          step={step}
          className="w-32 tabular-nums"
          value={draft}
          placeholder={placeholder}
          error={error ?? undefined}
          onChange={e => setDraft(e.target.value)}
          onBlur={commit}
        />
        {unit && <span className="text-sm text-text-tertiary shrink-0">{unit}</span>}
      </div>
    </div>
  )
}

/** The frame every threshold sits in. It looks different from the prose beside
 *  it on purpose: what is typed here does not stay on this page. */
function ThresholdBox({ icon, title, effect, unset, tab, tabLabel, isSet, active, openLabel, notReadLabel, onOpenArtifact, children }: {
  icon: React.ReactNode
  title: string
  /** What the figure changes on the other screen, once set. */
  effect: string
  /** What that screen does while it is not set. */
  unset: string
  tab?: string
  tabLabel: string
  isSet: boolean
  /** The plan itself. A switched-off plan is not consulted by the server. */
  active: boolean
  openLabel: string
  notReadLabel: string
  onOpenArtifact?: (tab: string) => void
  children: React.ReactNode
}) {
  return (
    <div className="rounded-lg border border-border bg-surface-1 p-3 space-y-2">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-text-secondary shrink-0">{icon}</span>
        <span className="text-sm font-semibold text-text-primary">{title}</span>
        <div className="flex-1" />
        <span className="inline-flex items-center gap-1 text-xs text-text-tertiary shrink-0">
          <ArrowUpRight size={13} />
          {tabLabel}
        </span>
        {tab && onOpenArtifact && (
          <Button size="sm" variant="text" onClick={() => onOpenArtifact(tab)}>
            {openLabel}
          </Button>
        )}
      </div>

      {children}

      <p className="text-xs leading-relaxed" style={{
        color: isSet ? 'var(--color-text-secondary)' : 'var(--color-text-tertiary)',
      }}>
        {isSet ? effect : unset}
      </p>

      {isSet && !active && (
        <p className="flex items-start gap-1.5 text-xs leading-relaxed" style={{ color: 'var(--color-warning)' }}>
          <AlertTriangle size={13} className="shrink-0 mt-px" />
          <span>{notReadLabel}</span>
        </p>
      )}
    </div>
  )
}

/** Everything a card needs, gathered once so the card itself stays readable. */
interface PlanCtx {
  t: TFunction
  canEdit: boolean
  freqOptions: DropdownOption[]
  money: (n: number) => string
  fmtDate: (iso: string) => string
  onOpenArtifact?: (tab: string) => void
  patch: (area: PlanArea, body: ManagementPlanEdit) => void
  startPlanning: (area: PlanArea) => void
  askUnplan: (area: PlanArea) => void
  pendingArea: PlanArea | null
  registerCard: (area: PlanArea, el: HTMLDivElement | null) => void
  flashArea: PlanArea | null
}

/** The thresholds of one area, if it holds any. Rendered only on the plan that
 *  READS them: the server refuses the others, and an input that can only earn a
 *  refusal has no business being on screen. */
function PlanThresholds({ plan, ctx }: { plan: ManagementPlan; ctx: PlanCtx }) {
  const { t, canEdit, onOpenArtifact } = ctx
  const readOnly = !canEdit
  const openLabel = t('proj_plan_open_artifact', { defaultValue: 'Ouvrir' })
  const notRead = t('proj_plan_threshold_not_read', {
    defaultValue: 'Le plan est désactivé : ce seuil est conservé mais plus lu. Réactivez-le pour qu’il s’applique.',
  })

  if (plan.area === 'cost' || plan.area === 'schedule') {
    const isCost = plan.area === 'cost'
    return (
      <ThresholdBox
        icon={<Gauge size={15} />}
        title={isCost
          ? t('proj_plan_th_cost_title', { defaultValue: 'Écart de coût admis' })
          : t('proj_plan_th_sched_title', { defaultValue: 'Écart de délai admis' })}
        tabLabel={t('proj_plan_read_by_ev', { defaultValue: 'Lu par « Valeur acquise »' })}
        tab="costs"
        openLabel={openLabel}
        notReadLabel={notRead}
        isSet={plan.variance_threshold_pct !== null}
        active={plan.is_active}
        onOpenArtifact={onOpenArtifact}
        effect={isCost
          ? t('proj_plan_th_cost_effect', { defaultValue: 'Au-delà de cet écart, rapporté au budget à l’achèvement, la valeur acquise signale une dérive de coût.' })
          : t('proj_plan_th_sched_effect', { defaultValue: 'Au-delà de cet écart, rapporté au budget à l’achèvement, la valeur acquise signale une dérive de délai.' })}
        unset={t('proj_plan_th_variance_unset', { defaultValue: 'Aucun seuil : la valeur acquise n’annonce aucune dérive et s’en tient aux chiffres. Un seuil que le projet n’a pas fixé n’est pas un seuil à inventer.' })}
      >
        <ThresholdInput
          label={t('proj_plan_th_variance_label', { defaultValue: 'Écart admis' })}
          unit="%"
          step="1"
          placeholder={t('proj_plan_th_variance_ph', { defaultValue: 'ex. 10' })}
          value={plan.variance_threshold_pct}
          readOnly={readOnly}
          validate={n => n < 0
            ? t('proj_plan_th_variance_err', { defaultValue: 'Un écart admis se donne en pourcentage positif.' })
            : null}
          onCommit={next => ctx.patch(plan.area, { variance_threshold_pct: next })}
        />
      </ThresholdBox>
    )
  }

  if (plan.area === 'risk') {
    return (
      <ThresholdBox
        icon={<Scale size={15} />}
        title={t('proj_plan_th_risk_title', { defaultValue: 'Appétit au risque' })}
        tabLabel={t('proj_plan_read_by_risks', { defaultValue: 'Lu par « Risques »' })}
        tab="risks"
        openLabel={openLabel}
        notReadLabel={notRead}
        isSet={plan.risk_appetite_score !== null}
        active={plan.is_active}
        onOpenArtifact={onOpenArtifact}
        effect={t('proj_plan_th_risk_effect', { defaultValue: 'Au-dessus de ce score, un risque n’est plus seulement suivi par son responsable : il doit être escaladé, et le registre le liste comme tel.' })}
        unset={t('proj_plan_th_risk_unset', { defaultValue: 'Aucun appétit fixé : rien n’est escaladé. Le registre cote les risques sans jamais dire lequel dépasse ce que le projet accepte.' })}
      >
        <ThresholdInput
          label={t('proj_plan_th_risk_label', { defaultValue: 'Score d’escalade (1–25)' })}
          step="1"
          placeholder={t('proj_plan_th_risk_ph', { defaultValue: 'ex. 12' })}
          value={plan.risk_appetite_score}
          readOnly={readOnly}
          validate={n => (!Number.isInteger(n) || n < 1 || n > 25)
            ? t('proj_plan_th_risk_err', { defaultValue: 'L’appétit au risque se règle sur l’échelle des scores, de 1 à 25 (probabilité × impact).' })
            : null}
          onCommit={next => ctx.patch(plan.area, { risk_appetite_score: next })}
        />
      </ThresholdBox>
    )
  }

  if (plan.area === 'change') {
    const isSet = plan.change_authority_amount !== null || plan.change_authority_days !== null
    return (
      <ThresholdBox
        icon={<Scale size={15} />}
        title={t('proj_plan_th_change_title', { defaultValue: 'Ce que le chef de projet tranche seul' })}
        tabLabel={t('proj_plan_read_by_changes', { defaultValue: 'Lu par « Changements »' })}
        tab="changes"
        openLabel={openLabel}
        notReadLabel={notRead}
        isSet={isSet}
        active={plan.is_active}
        onOpenArtifact={onOpenArtifact}
        effect={t('proj_plan_th_change_effect', { defaultValue: 'En deçà des deux limites, la demande évaluée se décide sans réunir personne. Au-delà de l’une ou de l’autre, le comité doit siéger — et le registre l’annonce avant qu’on s’en aperçoive.' })}
        unset={t('proj_plan_th_change_unset', { defaultValue: 'Aucune délégation : toute demande revient au comité, comme avant. Rien n’est présumé sur ce que le chef de projet pourrait décider seul.' })}
      >
        <div className="flex flex-wrap gap-3">
          <ThresholdInput
            label={t('proj_plan_th_change_amount', { defaultValue: 'Jusqu’à (montant)' })}
            step="100"
            placeholder={t('proj_plan_th_change_amount_ph', { defaultValue: 'ex. 5000' })}
            value={plan.change_authority_amount}
            readOnly={readOnly}
            validate={n => n < 0
              ? t('proj_plan_th_change_amount_err', { defaultValue: 'Une délégation se donne en montant positif.' })
              : null}
            onCommit={next => ctx.patch(plan.area, { change_authority_amount: next })}
          />
          <ThresholdInput
            label={t('proj_plan_th_change_days', { defaultValue: 'Jusqu’à (jours)' })}
            unit={t('proj_plan_days_unit', { defaultValue: 'j' })}
            step="1"
            placeholder={t('proj_plan_th_change_days_ph', { defaultValue: 'ex. 5' })}
            value={plan.change_authority_days}
            readOnly={readOnly}
            validate={n => (!Number.isInteger(n) || n < 0)
              ? t('proj_plan_th_change_days_err', { defaultValue: 'Un délai délégué se donne en nombre entier de jours.' })
              : null}
            onCommit={next => ctx.patch(plan.area, { change_authority_days: next })}
          />
        </div>
        {isSet && (
          <p className="text-xs text-text-tertiary">
            {t('proj_plan_th_change_recap', {
              defaultValue: 'Aujourd’hui : au-delà de {{amount}} ou de {{days}}, le comité tranche.',
              amount: plan.change_authority_amount === null
                ? t('proj_plan_th_change_no_amount', { defaultValue: 'tout montant' })
                : ctx.money(plan.change_authority_amount),
              days: plan.change_authority_days === null
                ? t('proj_plan_th_change_no_days', { defaultValue: 'tout délai' })
                : t('proj_plan_days_n', { defaultValue: '{{n}} j', n: plan.change_authority_days }),
            })}
          </p>
        )}
      </ThresholdBox>
    )
  }

  return null
}

/** One area. Three states, told apart on sight: never planned, planned and
 *  written, planned and left blank. */
function PlanCard({ slot, ctx }: { slot: PlanSlot; ctx: PlanCtx }) {
  const { t, canEdit, onOpenArtifact } = ctx
  const area = slot.area
  const state = planState(slot)
  const plan = slot.plan
  const tab = AREA_TAB[area]
  const readOnly = !canEdit

  return (
    <div
      ref={el => ctx.registerCard(area, el)}
      className="bg-surface-0 border border-border rounded-xl p-5 space-y-3"
      style={{
        boxShadow: ctx.flashArea === area ? '0 0 0 2px var(--color-primary)' : undefined,
        borderColor: state === 'empty' ? 'var(--color-warning)' : undefined,
      }}
    >
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-text-secondary shrink-0">{AREA_ICON[area]}</span>
        <h3 className="text-sm font-semibold text-text-primary">{areaLabel(t, area)}</h3>
        <span className="inline-flex">
          <Badge variant={STATE_VARIANT[state]} size="sm">{stateLabel(t, state)}</Badge>
        </span>
        <div className="flex-1" />
        {tab && onOpenArtifact && (
          <Tooltip label={t('proj_plan_open_area', {
            defaultValue: 'Ouvrir « {{area}} » — ce que le projet y a consigné',
            area: areaLabel(t, area),
          })}>
            <button
              type="button"
              onClick={() => onOpenArtifact(tab)}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-text-primary"
            >
              <ArrowUpRight size={15} />
            </button>
          </Tooltip>
        )}
      </div>

      <p className="text-xs text-text-secondary leading-relaxed">{areaPurpose(t, area)}</p>

      {state === 'unplanned' || !plan ? (
        // Not a defect: no project needs the twelve. Saying so plainly avoids the
        // reflex of filling every box to make the page look finished.
        <div className="flex items-center gap-3 flex-wrap">
          <p className="flex items-start gap-1.5 text-xs text-text-tertiary leading-relaxed flex-1 min-w-[14rem]">
            <CircleDashed size={13} className="shrink-0 mt-px" />
            <span>{t('proj_plan_unplanned_hint', {
              defaultValue: 'Personne n’a écrit comment ce domaine est piloté. Tous les projets n’ont pas besoin des douze — ne le planifiez que s’il le mérite.',
            })}</span>
          </p>
          {canEdit && (
            <Button
              size="sm" variant="secondary" icon={<Plus size={14} />}
              loading={ctx.pendingArea === area}
              onClick={() => ctx.startPlanning(area)}
            >
              {t('proj_plan_start', { defaultValue: 'Planifier ce domaine' })}
            </Button>
          )}
        </div>
      ) : (
        <>
          {state === 'empty' && (
            <p className="flex items-start gap-1.5 text-xs leading-relaxed" style={{ color: 'var(--color-warning)' }}>
              <AlertTriangle size={13} className="shrink-0 mt-px" />
              <span>{t('proj_plan_empty_hint', {
                defaultValue: 'Le plan est activé mais l’approche est vide : il affirme que le domaine est gouverné sans dire comment.',
              })}</span>
            </p>
          )}

          <div className="flex items-end gap-3 flex-wrap">
            <div className="min-w-[12rem]">
              <label className="text-xs text-text-tertiary mb-1 block">
                {t('proj_plan_review', { defaultValue: 'Revue du plan' })}
              </label>
              {readOnly ? (
                <p className="text-sm text-text-primary">{freqLabel(t, plan.review_frequency)}</p>
              ) : (
                <Dropdown
                  height={36} fontSize={14} focusable width="100%"
                  value={plan.review_frequency}
                  options={ctx.freqOptions}
                  onChange={v => ctx.patch(area, { review_frequency: v as PlanReviewFrequency })}
                />
              )}
            </div>
            <div className="flex-1" />
            {!readOnly && (
              <Toggle
                size="sm"
                checked={plan.is_active}
                label={t('proj_plan_active', { defaultValue: 'Plan en vigueur' })}
                onChange={e => ctx.patch(area, { is_active: e.target.checked })}
              />
            )}
          </div>

          {!plan.is_active && (
            <p className="text-xs text-text-tertiary leading-relaxed">
              {t('proj_plan_off_hint', {
                defaultValue: 'Plan mis en sommeil : son texte est conservé, mais il ne fait plus foi — et ses seuils ne sont plus lus.',
              })}
            </p>
          )}

          <ProseField
            label={t('proj_plan_approach', { defaultValue: 'Approche' })}
            icon={<PenLine size={13} />}
            value={plan.approach}
            rows={4}
            readOnly={readOnly}
            placeholder={t('proj_plan_approach_ph', { defaultValue: 'Comment ce domaine est piloté : la méthode retenue et pourquoi elle convient à ce projet…' })}
            onCommit={next => ctx.patch(area, { approach: next })}
          />
          <ProseField
            label={t('proj_plan_roles', { defaultValue: 'Rôles' })}
            icon={<Users size={13} />}
            value={plan.roles}
            readOnly={readOnly}
            placeholder={t('proj_plan_roles_ph', { defaultValue: 'Qui fait quoi, et qui décide quand il faut trancher…' })}
            onCommit={next => ctx.patch(area, { roles: next })}
          />
          <ProseField
            label={t('proj_plan_procedures', { defaultValue: 'Procédures' })}
            icon={<ListChecks size={13} />}
            value={plan.procedures}
            readOnly={readOnly}
            placeholder={t('proj_plan_procedures_ph', { defaultValue: 'Les étapes à suivre, dans l’ordre, et ce qui les déclenche…' })}
            onCommit={next => ctx.patch(area, { procedures: next })}
          />
          <ProseField
            label={t('proj_plan_tools', { defaultValue: 'Outils' })}
            icon={<Wrench size={13} />}
            value={plan.tools}
            readOnly={readOnly}
            placeholder={t('proj_plan_tools_ph', { defaultValue: 'Modèles, registres, logiciels — ce sur quoi le domaine s’appuie…' })}
            onCommit={next => ctx.patch(area, { tools: next })}
          />

          <PlanThresholds plan={plan} ctx={ctx} />

          <div className="flex items-center gap-2 flex-wrap pt-1">
            <span className="text-xs text-text-tertiary">
              {t('proj_plan_updated', { defaultValue: 'Mis à jour le {{date}}', date: ctx.fmtDate(plan.updated_at) })}
            </span>
            <div className="flex-1" />
            {canEdit && (
              <Button
                size="sm" variant="textDanger" icon={<Trash2 size={14} />}
                onClick={() => ctx.askUnplan(area)}
              >
                {t('proj_plan_unplan', { defaultValue: 'Ne plus planifier' })}
              </Button>
            )}
          </div>
        </>
      )}
    </div>
  )
}

/** One line of the summary: a figure, where it acts, and what happens without it. */
function EffectRow({ row, ctx }: { row: AppliedThreshold; ctx: PlanCtx }) {
  const { t, onOpenArtifact } = ctx
  return (
    <li className="flex items-start gap-2 py-2 border-b border-border last:border-b-0">
      <span className="shrink-0 mt-0.5" style={{
        color: row.ignored ? 'var(--color-warning)'
          : row.value ? 'var(--color-success)' : 'var(--color-text-tertiary)',
      }}>
        {row.ignored ? <AlertTriangle size={15} /> : row.value ? <CheckCircle2 size={15} /> : <CircleDashed size={15} />}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-sm text-text-primary">{row.label}</span>
          {row.value ? (
            <span className="inline-flex">
              <Badge variant={row.ignored ? 'warning' : 'primary'} size="sm">{row.value}</Badge>
            </span>
          ) : (
            <span className="text-xs text-text-tertiary">
              {t('proj_plan_effect_unset', { defaultValue: 'non réglé' })}
            </span>
          )}
        </div>
        <p className="text-xs text-text-secondary leading-relaxed mt-0.5">
          {row.ignored
            ? t('proj_plan_effect_ignored', {
                defaultValue: 'Réglé, mais le plan « {{area}} » est désactivé : la valeur est conservée et n’est plus lue.',
                area: areaLabel(t, row.area),
              })
            : row.value
              ? t('proj_plan_effect_applied', {
                  defaultValue: 'En vigueur, depuis le plan « {{area}} ».',
                  area: areaLabel(t, row.area),
                })
              : row.fallback}
        </p>
      </div>
      {onOpenArtifact && (
        <Button size="sm" variant="text" icon={<ArrowUpRight size={14} />}
          onClick={() => onOpenArtifact(row.tab)}>
          {t('proj_plan_open_artifact', { defaultValue: 'Ouvrir' })}
        </Button>
      )}
    </li>
  )
}

// ── The screen ───────────────────────────────────────────────────────────────

export default function ManagementPlansView({ projectId, canEdit = true, onOpenArtifact }: {
  projectId: string
  /** False in the mobile reading mode, where the project is shown, not edited. */
  canEdit?: boolean
  /** Jumps to the artifact a plan governs — or that reads one of its thresholds. */
  onOpenArtifact?: (tab: string) => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()

  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [flashArea, setFlashArea] = useState<PlanArea | null>(null)

  const cardRefs = useRef(new Map<PlanArea, HTMLDivElement>())
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => { if (flashTimer.current) clearTimeout(flashTimer.current) }, [])

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['plans', projectId],
    queryFn: () => projectsApi.getPlans(projectId),
  })
  // Read only for its currency: a delegated authority in money is meaningless
  // unsigned. Same cache key as the cost screens, so it usually costs nothing.
  const { data: costs } = useQuery({
    queryKey: ['costs', projectId],
    queryFn: () => projectsApi.getCosts(projectId),
  })

  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const inval = () => {
    qc.invalidateQueries({ queryKey: ['plans', projectId] })
    // The thresholds typed here are computed server-side into the other views'
    // payloads. Refetching them is not housekeeping: it is the whole point of
    // the screen — the change must already be true when the reader gets there.
    qc.invalidateQueries({ queryKey: ['costs', projectId] })
    qc.invalidateQueries({ queryKey: ['risks', projectId] })
    qc.invalidateQueries({ queryKey: ['changes', projectId] })
  }

  const saveMut = useMutation({
    mutationFn: ({ area, body }: { area: PlanArea; body: ManagementPlanEdit }) =>
      projectsApi.savePlan(projectId, area, body),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (area: PlanArea) => projectsApi.deletePlan(projectId, area),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })

  const patch = (area: PlanArea, body: ManagementPlanEdit) => saveMut.mutate({ area, body })

  const focusArea = (area: PlanArea) => {
    const el = cardRefs.current.get(area)
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    setFlashArea(area)
    if (flashTimer.current) clearTimeout(flashTimer.current)
    flashTimer.current = setTimeout(() => setFlashArea(null), 1600)
  }

  const startPlanning = (area: PlanArea) => {
    // An area starts governed and blank; the card says so immediately, which is
    // the invitation to write the approach rather than leave the switch on.
    saveMut.mutate({ area, body: { is_active: true } }, { onSuccess: () => focusArea(area) })
  }

  const askUnplan = (area: PlanArea) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_plan_unplan_title', {
          defaultValue: 'Ne plus planifier « {{area}} » ?',
          area: areaLabel(t, area),
        }),
        message: t('proj_plan_unplan_msg', {
          defaultValue: 'Le domaine redevient non planifié : l’approche, les rôles, les procédures, les outils et les seuils sont effacés. Ce n’est pas la même chose que désactiver le plan, qui le met en sommeil en gardant son texte — et permet de le remettre en vigueur tel quel.',
        }),
        confirmLabel: t('proj_plan_unplan_confirm', { defaultValue: 'Ne plus planifier' }),
        cancelLabel: t('common_cancel', { defaultValue: 'Annuler' }),
        variant: 'danger',
      })
      if (ok) deleteMut.mutate(area)
    })()
  }

  const registerCard = (area: PlanArea, el: HTMLDivElement | null) => {
    if (el) cardRefs.current.set(area, el); else cardRefs.current.delete(area)
  }

  // ── Formatting ────────────────────────────────────────────────────────────

  const fmtDate = (iso: string) =>
    format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })

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

  const pct = useMemo(() => {
    const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 })
    return (n: number) => `${nf.format(n)} %`
  }, [i18n.language])

  const freqOptions = useMemo<DropdownOption[]>(
    () => FREQUENCIES.map(f => ({ value: f, label: freqLabel(t, f) })), [t])

  // ── Data ──────────────────────────────────────────────────────────────────

  const slots = useMemo<PlanSlot[]>(() => {
    const got = data?.plans ?? []
    if (got.length) return got
    return AREA_ORDER.map(area => ({ area, planned: false, plan: null }))
  }, [data])

  const planOf = useMemo(() => {
    const map = new Map<PlanArea, ManagementPlan | null>()
    for (const s of slots) map.set(s.area, s.plan)
    return map
  }, [slots])

  /** Mirrors, field for field, what the server gathers for the other artifacts —
   *  including the fact that it only looks at ACTIVE plans. */
  const applied = useMemo<AppliedThreshold[]>(() => {
    const cost = planOf.get('cost') ?? null
    const sched = planOf.get('schedule') ?? null
    const risk = planOf.get('risk') ?? null
    const change = planOf.get('change') ?? null

    const varianceRow = (
      key: string, area: PlanArea, plan: ManagementPlan | null, label: string, fallback: string,
    ): AppliedThreshold => ({
      key, area, tab: 'costs', label,
      value: plan?.variance_threshold_pct != null ? pct(plan.variance_threshold_pct) : null,
      fallback,
      ignored: plan?.variance_threshold_pct != null && !plan.is_active,
    })

    const changeParts: string[] = []
    if (change?.change_authority_amount != null) changeParts.push(money(change.change_authority_amount))
    if (change?.change_authority_days != null) {
      changeParts.push(t('proj_plan_days_n', { defaultValue: '{{n}} j', n: change.change_authority_days }))
    }

    return [
      varianceRow('cost_variance', 'cost', cost,
        t('proj_plan_effect_cost', { defaultValue: 'Écart de coût au-delà duquel une dérive est signalée' }),
        t('proj_plan_effect_cost_none', { defaultValue: 'Sans seuil, la valeur acquise ne signale aucune dérive de coût — elle affiche les chiffres et laisse juger.' })),
      varianceRow('schedule_variance', 'schedule', sched,
        t('proj_plan_effect_sched', { defaultValue: 'Écart de délai au-delà duquel une dérive est signalée' }),
        t('proj_plan_effect_sched_none', { defaultValue: 'Sans seuil, la valeur acquise ne signale aucune dérive de délai.' })),
      {
        key: 'risk_appetite', area: 'risk', tab: 'risks',
        label: t('proj_plan_effect_risk', { defaultValue: 'Score au-delà duquel un risque doit être escaladé' }),
        value: risk?.risk_appetite_score != null
          ? t('proj_plan_score_of', { defaultValue: '{{n}} / 25', n: risk.risk_appetite_score })
          : null,
        fallback: t('proj_plan_effect_risk_none', { defaultValue: 'Sans appétit fixé, rien n’est escaladé : le registre cote les risques sans dire lequel dépasse ce que le projet accepte.' }),
        ignored: risk?.risk_appetite_score != null && !risk.is_active,
      },
      {
        key: 'change_authority', area: 'change', tab: 'changes',
        label: t('proj_plan_effect_change', { defaultValue: 'Ce que le chef de projet tranche seul' }),
        value: changeParts.length ? changeParts.join(' · ') : null,
        fallback: t('proj_plan_effect_change_none', { defaultValue: 'Sans délégation, toute demande de changement revient au comité.' }),
        ignored: changeParts.length > 0 && !!change && !change.is_active,
      },
    ]
  }, [planOf, money, pct, t])

  const withoutApproach = data?.without_approach ?? []
  const summary = data?.summary
  const inForce = applied.filter(r => r.value !== null && !r.ignored).length

  const ctx: PlanCtx = {
    t, canEdit, freqOptions, money, fmtDate, onOpenArtifact,
    patch, startPlanning, askUnplan,
    pendingArea: saveMut.isPending ? saveMut.variables?.area ?? null : null,
    registerCard, flashArea,
  }

  // ── Render ────────────────────────────────────────────────────────────────

  if (isLoading) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <div className="bg-surface-0 border border-border rounded-xl p-8 text-center text-sm text-text-secondary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        </div>
      </div>
    )
  }

  if (isError || !data) {
    // Never an empty collection here — the twelve areas always come back. The
    // only empty state this screen can be in is a failed load.
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <EmptyState
            variant="error"
            icon={<Layers size={26} />}
            title={t('proj_plan_error_title', { defaultValue: 'Les plans de management n’ont pas pu être chargés' })}
            description={t('proj_plan_error_desc', { defaultValue: 'Rien n’est connu de ce que le projet a planifié : mieux vaut réessayer que de lire une page vide.' })}
            action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
            t={t}
          />
        </div>
      </div>
    )
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Layers size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_plan_title', { defaultValue: 'Plans de management' })}
          </h1>
          <span className="inline-flex">
            <Badge variant={withoutApproach.length ? 'warning' : 'primary'}>
              {t('proj_plan_count', {
                defaultValue: '{{active}} domaine(s) planifié(s) sur {{areas}}',
                active: summary?.active ?? 0,
                areas: summary?.areas ?? AREA_ORDER.length,
              })}
            </Badge>
          </span>
        </div>

        <p className="text-sm text-text-secondary leading-relaxed">
          {t('proj_plan_intro', {
            defaultValue: 'Les autres écrans disent ce que le projet contient : quels risques, quels changements, quels coûts. Celui-ci dit comment chaque domaine est piloté — et quatre des valeurs saisies ici sont relues par ces écrans, qui jugeaient jusque-là contre une règle identique pour un plan de trois tâches et un programme de construction.',
          })}
        </p>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {/* ── The defect proper to this screen, in the lead ── */}
        {withoutApproach.length > 0 && (
          <Callout
            variant="warning"
            icon={<AlertTriangle size={16} />}
            title={t('proj_plan_gap_title', {
              defaultValue: '{{n}} plan(s) activé(s) sans approche rédigée',
              n: withoutApproach.length,
            })}
          >
            <div className="space-y-2">
              <p className="text-sm">
                {t('proj_plan_gap_msg', {
                  defaultValue: 'Ces plans affirment que leur domaine est gouverné alors que rien ne dit comment. Les désactiver serait plus honnête que les laisser vides — les rédiger, plus utile.',
                })}
              </p>
              <div className="flex items-center gap-2 flex-wrap">
                {withoutApproach.map(area => (
                  <Button key={area} size="sm" variant="secondary" onClick={() => focusArea(area)}>
                    {areaLabel(t, area)}
                  </Button>
                ))}
              </div>
            </div>
          </Callout>
        )}

        {/* ── What this screen changes elsewhere ── */}
        <div className="bg-surface-0 border border-border rounded-xl p-5">
          <div className="flex items-center gap-2 flex-wrap mb-1">
            <Gauge size={18} className="text-text-secondary shrink-0" />
            <h2 className="text-sm font-semibold text-text-primary">
              {t('proj_plan_effects_title', { defaultValue: 'Ce que ces plans changent ailleurs' })}
            </h2>
            <div className="flex-1" />
            <span className="text-xs text-text-tertiary shrink-0">
              {t('proj_plan_effects_count', {
                defaultValue: '{{n}}/{{total}} en vigueur',
                n: inForce, total: applied.length,
              })}
            </span>
          </div>
          <p className="text-xs text-text-secondary mb-2 leading-relaxed">
            {t('proj_plan_effects_intro', {
              defaultValue: 'Ces quatre valeurs ne restent pas sur cette page : les artefacts les relisent pour décider ce qu’ils signalent. Un seuil laissé vide n’est pas un oubli — c’est le choix de laisser l’artefact garder son propre jugement.',
            })}
          </p>
          <ul>
            {applied.map(row => <EffectRow key={row.key} row={row} ctx={ctx} />)}
          </ul>
        </div>

        {/* ── The twelve areas, in reading order ── */}
        {/* Multi-column flow rather than a grid: a grid makes every row as tall as
            its tallest card, so an unplanned area would leave a hole beside a
            fully written one. No width cap — the page uses the window it is given. */}
        <div className={isMobile ? 'space-y-4' : 'columns-2 gap-4 [&>*]:break-inside-avoid [&>*]:mb-4'}>
          {slots.map(slot => <PlanCard key={slot.area} slot={slot} ctx={ctx} />)}
        </div>
      </div>

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
