import { Fragment, useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useConfirm } from '@kubuno/sdk'
import {
  ShieldAlert, Plus, Trash2, ChevronRight, ChevronDown, Search, Siren,
  GitBranch, TrendingUp, TrendingDown, Coins, Grid3x3, X,
} from 'lucide-react'
import {
  Button, Input, Textarea, Dropdown, Badge, Callout, Tooltip, EmptyState,
  FloatingWindow, ConfirmDialog, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type Risk, type RiskEdit, type RiskKind, type RiskCategory,
  type RiskStatus, type RiskStrategy,
} from '../api'

// Risk register — everything that might still happen, ranked by exposure.
//
// Three things drive the layout:
//  • The probability/impact MATRIX is the screen, not a decoration. It is the
//    only view that shows where the exposure is concentrated, and it doubles as
//    the register's filter: click a cell, read the risks that sit in it.
//  • A risk is a THREAT or an OPPORTUNITY, and the two are not mirror images:
//    they take different response strategies and their expected monetary value
//    carries the opposite sign. The screen never blurs the two.
//  • EMV is a provision, not a score. A bare signed number is unreadable, so it
//    is always spelled out: an exposure to carry, or a gain hoped for.
//
// Editing happens in place (the modal is for creation only); long fields live in
// a per-row detail panel and are committed on blur.

// ── Vocabularies (the server answers 422 to anything outside these lists) ─────

export const RISK_KINDS: RiskKind[] = ['threat', 'opportunity']
export const RISK_CATEGORIES: RiskCategory[] = [
  'technical', 'external', 'organizational', 'management', 'commercial',
]
export const RISK_STATUSES: RiskStatus[] = [
  'identified', 'analysing', 'responding', 'occurred', 'closed',
]
/**
 * The statuses the matrix, the "open" count and the EMV are computed over: what
 * may STILL happen. A risk that came true is no longer a probability — it is an
 * issue, and it is followed in the issue log from then on. Kept in step with the
 * server, which applies the very same rule.
 */
export const STILL_AHEAD = new Set<RiskStatus>(['identified', 'analysing', 'responding'])
/** A threat is avoided, reduced, transferred, accepted or escalated… */
export const THREAT_STRATEGIES: RiskStrategy[] = [
  'avoid', 'mitigate', 'transfer', 'accept', 'escalate',
]
/** …an opportunity is exploited, enhanced, shared, accepted or escalated. */
export const OPPORTUNITY_STRATEGIES: RiskStrategy[] = [
  'exploit', 'enhance', 'share', 'accept', 'escalate',
]

/** The strategies the server will accept for that nature — nothing else. */
export function strategiesFor(kind: RiskKind): RiskStrategy[] {
  return kind === 'threat' ? THREAT_STRATEGIES : OPPORTUNITY_STRATEGIES
}

/**
 * Changing the nature invalidates the strategy: `avoid` means nothing for an
 * opportunity and the server rejects the pair. `accept` and `escalate` belong to
 * both lists and survive; anything else falls back to the natural default.
 */
function strategyAfterKindChange(kind: RiskKind, current: RiskStrategy): RiskStrategy {
  const allowed = strategiesFor(kind)
  if (allowed.includes(current)) return current
  return kind === 'threat' ? 'mitigate' : 'exploit'
}

const LEVELS = [1, 2, 3, 4, 5]

// ── Severity: the 4 bands of the matrix ──────────────────────────────────────

type SeverityBand = 'low' | 'moderate' | 'high' | 'critical'

/** probability × impact, cut into the four usual bands. */
export function severityBand(score: number): SeverityBand {
  if (score <= 4) return 'low'
  if (score <= 9) return 'moderate'
  if (score <= 14) return 'high'
  return 'critical'
}

/**
 * Band colours, taken from the theme tokens so a re-themed instance (and dark
 * mode, where the host swaps the very same variables) recolours the matrix.
 * There is no orange token: the "high" band is MIXED from its two neighbours,
 * which keeps it in step with whatever warning and danger become.
 */
const SEVERITY_COLOR: Record<SeverityBand, string> = {
  low: 'var(--color-success)',
  moderate: 'var(--color-warning)',
  high: 'color-mix(in srgb, var(--color-warning) 45%, var(--color-danger))',
  critical: 'var(--color-danger)',
}

/**
 * The band colour laid over the surface at a chosen opacity. Shade is modulated
 * by the BACKGROUND alpha only — the figure keeps `--color-text-primary`, which
 * flips with the theme, so it stays legible on a light surface and on a dark one.
 */
function severityTint(band: SeverityBand, pct: number) {
  return `color-mix(in srgb, ${SEVERITY_COLOR[band]} ${pct}%, transparent)`
}

/** The score as a chip carrying its band's colour — same palette as the matrix. */
function ScoreChip({ score, title }: { score: number; title?: string }) {
  const band = severityBand(score)
  return (
    <span
      title={title}
      className="inline-flex items-center justify-center rounded-md px-2 py-0.5 text-sm font-semibold text-text-primary"
      style={{
        background: severityTint(band, 26),
        boxShadow: `inset 0 0 0 1px ${severityTint(band, 55)}`,
      }}
    >
      {score}
    </span>
  )
}

// ── Human labels ─────────────────────────────────────────────────────────────

type BadgeVariant = 'default' | 'primary' | 'success' | 'warning' | 'danger' | 'neutral'

const STATUS_BADGE: Record<RiskStatus, BadgeVariant> = {
  identified: 'default', analysing: 'primary', responding: 'warning',
  occurred: 'danger', closed: 'neutral',
}

export function useRiskLabels() {
  const { t } = useTranslation('office')
  return useMemo(() => ({
    kind: {
      threat: t('risk_kind_threat', { defaultValue: 'Menace' }),
      opportunity: t('risk_kind_opportunity', { defaultValue: 'Opportunité' }),
    } as Record<RiskKind, string>,
    category: {
      technical: t('risk_cat_technical', { defaultValue: 'Technique' }),
      external: t('risk_cat_external', { defaultValue: 'Externe' }),
      organizational: t('risk_cat_organizational', { defaultValue: 'Organisationnelle' }),
      management: t('risk_cat_management', { defaultValue: 'Management' }),
      commercial: t('risk_cat_commercial', { defaultValue: 'Commerciale' }),
    } as Record<RiskCategory, string>,
    status: {
      identified: t('risk_status_identified', { defaultValue: 'Identifié' }),
      analysing: t('risk_status_analysing', { defaultValue: 'En analyse' }),
      responding: t('risk_status_responding', { defaultValue: 'Réponse en cours' }),
      occurred: t('risk_status_occurred', { defaultValue: 'Survenu' }),
      closed: t('risk_status_closed', { defaultValue: 'Clos' }),
    } as Record<RiskStatus, string>,
    strategy: {
      avoid: t('risk_strat_avoid', { defaultValue: 'Éviter' }),
      mitigate: t('risk_strat_mitigate', { defaultValue: 'Réduire' }),
      transfer: t('risk_strat_transfer', { defaultValue: 'Transférer' }),
      exploit: t('risk_strat_exploit', { defaultValue: 'Exploiter' }),
      enhance: t('risk_strat_enhance', { defaultValue: 'Améliorer' }),
      share: t('risk_strat_share', { defaultValue: 'Partager' }),
      accept: t('risk_strat_accept', { defaultValue: 'Accepter' }),
      escalate: t('risk_strat_escalate', { defaultValue: 'Escalader' }),
    } as Record<RiskStrategy, string>,
    severity: {
      low: t('risk_sev_low', { defaultValue: 'Faible' }),
      moderate: t('risk_sev_moderate', { defaultValue: 'Modérée' }),
      high: t('risk_sev_high', { defaultValue: 'Élevée' }),
      critical: t('risk_sev_critical', { defaultValue: 'Critique' }),
    } as Record<SeverityBand, string>,
    // Words for the two axes: a bare 1-to-5 scale means nothing on its own.
    probability: {
      1: t('risk_prob_1', { defaultValue: 'Très improbable' }),
      2: t('risk_prob_2', { defaultValue: 'Peu probable' }),
      3: t('risk_prob_3', { defaultValue: 'Possible' }),
      4: t('risk_prob_4', { defaultValue: 'Probable' }),
      5: t('risk_prob_5', { defaultValue: 'Quasi certain' }),
    } as Record<number, string>,
    impact: {
      1: t('risk_impact_1', { defaultValue: 'Négligeable' }),
      2: t('risk_impact_2', { defaultValue: 'Mineur' }),
      3: t('risk_impact_3', { defaultValue: 'Modéré' }),
      4: t('risk_impact_4', { defaultValue: 'Majeur' }),
      5: t('risk_impact_5', { defaultValue: 'Critique' }),
    } as Record<number, string>,
  }), [t])
}

/** Threats read red, opportunities green — never two shades of the same colour. */
const KIND_FG: Record<RiskKind, string> = {
  threat: 'var(--color-danger)',
  opportunity: 'var(--color-success)',
}

function KindBadge({ kind, label }: { kind: RiskKind; label: string }) {
  return (
    <Badge variant={kind === 'threat' ? 'danger' : 'success'} dot>{label}</Badge>
  )
}

// ── Expected monetary value ──────────────────────────────────────────────────

/**
 * EMV = probability (%) × monetary impact, signed by the nature of the risk:
 * a threat costs, an opportunity brings in. Null as soon as one of the two
 * quantitative fields is missing — an unpriced risk is not a risk worth zero.
 */
function computeEmv(risk: Pick<Risk, 'kind' | 'probability_pct' | 'monetary_impact'>): number | null {
  const { probability_pct: p, monetary_impact: m } = risk
  if (p === null || m === null) return null
  const value = (p / 100) * m
  return risk.kind === 'threat' ? -Math.abs(value) : Math.abs(value)
}

// ── The probability/impact matrix ────────────────────────────────────────────

type Cell = { p: number; i: number }

/**
 * 5×5 heat map. Impact runs left to right, probability runs BOTTOM to TOP — the
 * universal convention, so the worst corner is the top right and stays where
 * every reader expects it.
 */
function RiskMatrix({ matrix, selected, onSelect, isMobile }: {
  matrix: number[][]
  selected: Cell | null
  onSelect: (cell: Cell | null) => void
  isMobile: boolean
}) {
  const { t } = useTranslation('office')
  const labels = useRiskLabels()

  const count = (p: number, i: number) => matrix?.[p - 1]?.[i - 1] ?? 0
  const cellSize = isMobile ? 46 : 58
  const axisWidth = isMobile ? 26 : 128

  return (
    <div>
      <div className="flex gap-2">
        {/* Vertical axis title — rotated so it reads bottom-up, like the scale. */}
        <div
          className="flex items-center justify-center text-sm text-text-secondary shrink-0"
          style={{ writingMode: 'vertical-rl', transform: 'rotate(180deg)' }}
        >
          {t('risk_axis_probability', { defaultValue: 'Probabilité' })}
        </div>

        <div className="flex-1 min-w-0 overflow-x-auto">
          <div
            style={{
              minWidth: axisWidth + cellSize * 5 + 24,
              display: 'grid',
              gridTemplateColumns: `${axisWidth}px repeat(5, minmax(${cellSize}px, 1fr))`,
              gap: 4,
            }}
          >
            {/* Probability 5 → 1, top to bottom. */}
            {[5, 4, 3, 2, 1].map(p => (
              <Fragment key={p}>
                <div className="flex items-center justify-end gap-1.5 pr-1 min-w-0">
                  {!isMobile && (
                    <span className="text-sm text-text-tertiary truncate">{labels.probability[p]}</span>
                  )}
                  <span className="text-sm font-semibold text-text-secondary shrink-0">{p}</span>
                </div>

                {LEVELS.map(i => {
                  const n = count(p, i)
                  const score = p * i
                  const band = severityBand(score)
                  const isSelected = selected?.p === p && selected?.i === i
                  const label = t('risk_cell_tip', {
                    defaultValue: '{{n}} risque(s) · probabilité {{p}} × impact {{i}} = {{score}} ({{band}})',
                    n, p, i, score, band: labels.severity[band],
                  })
                  return (
                    <Tooltip key={i} label={label}>
                      <button
                        type="button"
                        // Not `disabled`: a disabled button swallows the pointer
                        // events the tooltip needs, and the empty cells are
                        // exactly the ones whose meaning has to be explained.
                        aria-disabled={n === 0}
                        onClick={() => { if (n > 0) onSelect(isSelected ? null : { p, i }) }}
                        aria-pressed={isSelected}
                        className={`rounded-md flex items-center justify-center text-text-primary transition-[box-shadow] ${n > 0 ? 'cursor-pointer' : 'cursor-default'}`}
                        style={{
                          height: cellSize,
                          // Shade comes from the alpha of the band colour only.
                          background: severityTint(band, n > 0 ? 26 : 9),
                          boxShadow: isSelected
                            ? `inset 0 0 0 2px ${SEVERITY_COLOR[band]}`
                            : `inset 0 0 0 1px ${severityTint(band, 40)}`,
                        }}
                      >
                        <span
                          className="text-base font-semibold"
                          // An empty cell still carries its figure, kept quiet.
                          style={{ opacity: n > 0 ? 1 : 0.28 }}
                        >
                          {n}
                        </span>
                      </button>
                    </Tooltip>
                  )
                })}
              </Fragment>
            ))}

            {/* Impact scale, under the grid. */}
            <div />
            {LEVELS.map(i => (
              <div key={i} className="text-center min-w-0 pt-0.5">
                <div className="text-sm font-semibold text-text-secondary">{i}</div>
                {!isMobile && (
                  <div className="text-sm text-text-tertiary truncate">{labels.impact[i]}</div>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>

      <p className="text-center text-sm text-text-secondary mt-1">
        {t('risk_axis_impact', { defaultValue: 'Impact' })}
      </p>

      <div className="flex items-center gap-3 flex-wrap mt-3">
        {(['low', 'moderate', 'high', 'critical'] as SeverityBand[]).map(band => (
          <span key={band} className="inline-flex items-center gap-1.5 text-sm text-text-secondary">
            <span
              className="w-3.5 h-3.5 rounded"
              style={{
                background: severityTint(band, 32),
                boxShadow: `inset 0 0 0 1px ${severityTint(band, 60)}`,
              }}
            />
            {labels.severity[band]}
          </span>
        ))}
        <span className="text-sm text-text-tertiary">
          {t('risk_matrix_closed_note', { defaultValue: 'Les risques survenus et clos n’y figurent pas.' })}
        </span>
      </div>
    </div>
  )
}

// ── Small in-place editors ───────────────────────────────────────────────────

/** Single-line field committed to the server on blur (never on each keystroke). */
function InlineText({ value, placeholder, disabled, className, mono, onCommit }: {
  value: string
  placeholder?: string
  disabled?: boolean
  className?: string
  mono?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  if (disabled) {
    return (
      <span className={`text-sm text-text-primary ${mono ? 'font-mono' : ''} ${className ?? ''}`}>
        {value || '—'}
      </span>
    )
  }
  return (
    <Input
      className={className}
      value={draft}
      placeholder={placeholder}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => { if (draft !== value) onCommit(draft) }}
    />
  )
}

/** Long field of the detail panel, committed on blur. */
function LongField({ label, hint, value, rows = 3, disabled, onCommit }: {
  label: string
  hint?: string
  value: string
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
        disabled={disabled}
        hint={hint}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => { if (draft !== value) onCommit(draft) }}
      />
    </div>
  )
}

/** Nullable number committed on blur. An empty field means "not priced", not 0. */
function NumberField({ label, hint, value, min, max, step, suffix, disabled, onCommit }: {
  label: string
  hint?: string
  value: number | null
  min?: number
  max?: number
  step?: number
  suffix?: string
  disabled?: boolean
  onCommit: (next: number | null) => void
}) {
  const [draft, setDraft] = useState(value === null ? '' : String(value))
  useEffect(() => { setDraft(value === null ? '' : String(value)) }, [value])

  const commit = () => {
    const raw = draft.trim()
    if (raw === '') { if (value !== null) onCommit(null); return }
    const parsed = Number(raw.replace(',', '.'))
    if (!Number.isFinite(parsed)) { setDraft(value === null ? '' : String(value)); return }
    const clamped = Math.min(max ?? Infinity, Math.max(min ?? -Infinity, parsed))
    if (clamped !== value) onCommit(clamped)
    else setDraft(String(clamped))
  }

  return (
    <div>
      <label className="text-sm text-text-secondary mb-1 block">{label}</label>
      <Input
        type="number"
        inputMode="decimal"
        min={min}
        max={max}
        step={step}
        value={draft}
        disabled={disabled}
        hint={hint}
        rightIcon={suffix ? <span className="text-sm text-text-tertiary">{suffix}</span> : undefined}
        onChange={e => setDraft(e.target.value)}
        onBlur={commit}
      />
    </div>
  )
}

// ── One row of the register ──────────────────────────────────────────────────

function RiskRow({ risk, canEdit, isMobile, expanded, onToggle, ownerOptions, colCount, nf, onUpdate, onDelete, onMaterialize }: {
  risk: Risk
  canEdit: boolean
  isMobile: boolean
  expanded: boolean
  onToggle: () => void
  ownerOptions: DropdownOption[]
  colCount: number
  nf: Intl.NumberFormat
  onUpdate: (patch: RiskEdit) => void
  onDelete: () => void
  onMaterialize: () => void
}) {
  const { t } = useTranslation('office')
  const labels = useRiskLabels()

  const opts = (values: string[], map: Record<string, string>): DropdownOption[] =>
    values.map(v => ({ value: v, label: map[v] }))

  const levelOpts: DropdownOption[] = LEVELS.map(n => ({ value: String(n), label: String(n) }))
  const emv = computeEmv(risk)
  const cell = 'px-2 py-1.5 align-middle'

  // Strategy options follow the nature: the server refuses a mismatched pair.
  const strategyOpts = opts(strategiesFor(risk.kind), labels.strategy)

  const ownerValue = risk.owner_id ?? ''
  // An owner who is no longer a collaborator would otherwise vanish from the
  // trigger: keep the name the server gave us in the list.
  const ownerList: DropdownOption[] = ownerValue && !ownerOptions.some(o => o.value === ownerValue)
    ? [...ownerOptions, { value: ownerValue, label: risk.owner_name ?? t('risk_owner_unknown', { defaultValue: 'Utilisateur inconnu' }) }]
    : ownerOptions

  const changeKind = (next: RiskKind) => {
    // One patch, not two: the pair must stay consistent at every instant.
    onUpdate({ kind: next, response_strategy: strategyAfterKindChange(next, risk.response_strategy) })
  }

  return (
    <>
      <tr className="border-t border-border hover:bg-surface-1">
        {/* Code — also the disclosure control for the detail panel. */}
        <td className={cell}>
          <div className="flex items-center gap-1">
            <button
              onClick={onToggle}
              title={t('risk_toggle_detail', { defaultValue: 'Détail du risque' })}
              className="p-1 rounded text-text-tertiary hover:bg-surface-2 shrink-0"
            >
              {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
            </button>
            <InlineText
              value={risk.code}
              mono
              className="w-[6rem] font-mono"
              disabled={!canEdit}
              placeholder={t('risk_code_ph', { defaultValue: 'Sans code' })}
              onCommit={v => onUpdate({ code: v })}
            />
            {risk.parent_risk_id && (
              <Tooltip
                label={t('risk_secondary_tip', {
                  defaultValue: 'Risque secondaire — il découle de la réponse apportée à {{code}} · {{title}}',
                  code: risk.parent_code ?? '?', title: risk.parent_title ?? '',
                })}
              >
                {/* Wrapped: `Tooltip` clones its child to inject handlers, and a
                    plain icon element forwards them fine. */}
                <span className="inline-flex text-text-tertiary shrink-0">
                  <GitBranch size={14} />
                </span>
              </Tooltip>
            )}
          </div>
        </td>

        {/* Title */}
        <td className={cell}>
          <InlineText
            value={risk.title}
            className="w-full min-w-[14rem]"
            disabled={!canEdit}
            onCommit={v => { if (v.trim()) onUpdate({ title: v.trim() }) }}
          />
        </td>

        {!isMobile && (
          <td className={cell}>
            {canEdit ? (
              <Dropdown
                height={36} fontSize={14} focusable width="100%"
                value={risk.kind}
                options={opts(RISK_KINDS, labels.kind)}
                buttonStyle={{ color: KIND_FG[risk.kind], borderColor: KIND_FG[risk.kind], fontWeight: 600 }}
                onChange={v => changeKind(v as RiskKind)}
              />
            ) : <KindBadge kind={risk.kind} label={labels.kind[risk.kind]} />}
          </td>
        )}

        {!isMobile && (
          <td className={cell}>
            {canEdit ? (
              <Dropdown height={36} fontSize={14} focusable width="100%" value={risk.category}
                options={opts(RISK_CATEGORIES, labels.category)}
                onChange={v => onUpdate({ category: v as RiskCategory })} />
            ) : <span className="text-sm text-text-primary">{labels.category[risk.category]}</span>}
          </td>
        )}

        {/* Probability × impact, and the score they produce. */}
        <td className={cell}>
          <div className="flex items-center gap-1.5">
            {canEdit ? (
              <>
                <Dropdown height={36} fontSize={14} focusable width={58} value={String(risk.probability)}
                  options={levelOpts}
                  onChange={v => onUpdate({ probability: Number(v) })} />
                <span className="text-sm text-text-tertiary">×</span>
                <Dropdown height={36} fontSize={14} focusable width={58} value={String(risk.impact)}
                  options={levelOpts}
                  onChange={v => onUpdate({ impact: Number(v) })} />
              </>
            ) : (
              <span className="text-sm text-text-primary">{risk.probability} × {risk.impact}</span>
            )}
            <ScoreChip
              score={risk.score}
              title={t('risk_score_tip', {
                defaultValue: 'Gravité {{band}} — {{p}} × {{i}} = {{score}}',
                band: labels.severity[severityBand(risk.score)],
                p: risk.probability, i: risk.impact, score: risk.score,
              })}
            />
          </div>
        </td>

        {!isMobile && (
          <td className={cell}>
            {canEdit ? (
              <Dropdown height={36} fontSize={14} focusable width="100%" value={risk.response_strategy}
                options={strategyOpts}
                onChange={v => onUpdate({ response_strategy: v as RiskStrategy })} />
            ) : <span className="text-sm text-text-primary">{labels.strategy[risk.response_strategy]}</span>}
          </td>
        )}

        {!isMobile && (
          <td className={cell}>
            {canEdit ? (
              <Dropdown
                height={36} fontSize={14} focusable width="100%"
                value={ownerValue}
                placeholder={t('risk_owner_none', { defaultValue: 'Non attribué' })}
                options={ownerList}
                onChange={v => onUpdate({ owner_id: v || null })}
              />
            ) : (
              <span className="text-sm text-text-primary">
                {risk.owner_name ?? t('risk_owner_none', { defaultValue: 'Non attribué' })}
              </span>
            )}
          </td>
        )}

        <td className={cell}>
          {canEdit ? (
            <Dropdown height={36} fontSize={14} focusable width="100%" value={risk.status}
              options={opts(RISK_STATUSES, labels.status)}
              onChange={v => onUpdate({ status: v as RiskStatus })} />
          ) : <Badge variant={STATUS_BADGE[risk.status]}>{labels.status[risk.status]}</Badge>}
        </td>

        <td className={`${cell} text-right whitespace-nowrap`}>
          {canEdit && risk.status !== 'closed' && (
            <button
              onClick={onMaterialize}
              title={t('risk_materialize', { defaultValue: 'Ce risque s’est produit' })}
              className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Siren size={15} />
            </button>
          )}
          {canEdit && (
            <button
              onClick={onDelete}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={15} />
            </button>
          )}
        </td>
      </tr>

      {expanded && (
        <tr className="border-t border-border bg-surface-1">
          <td colSpan={colCount} className="p-4">
            <div className="space-y-3">
              {risk.parent_risk_id && (
                <Callout variant="info" icon={<GitBranch size={16} />}
                  title={t('risk_secondary_title', { defaultValue: 'Risque secondaire' })}>
                  {t('risk_secondary_body', {
                    defaultValue: 'Il découle de la réponse apportée au risque {{code}} — {{title}}.',
                    code: risk.parent_code ?? '?', title: risk.parent_title ?? '',
                  })}
                </Callout>
              )}

              {/* On a narrow screen these attributes leave the table; they live here. */}
              {isMobile && (
                <div className="grid grid-cols-1 gap-3">
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_kind', { defaultValue: 'Nature' })}</label>
                    <Dropdown height={36} fontSize={14} focusable width="100%" value={risk.kind} disabled={!canEdit}
                      options={opts(RISK_KINDS, labels.kind)}
                      buttonStyle={{ color: KIND_FG[risk.kind], borderColor: KIND_FG[risk.kind], fontWeight: 600 }}
                      onChange={v => changeKind(v as RiskKind)} />
                  </div>
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_category', { defaultValue: 'Catégorie' })}</label>
                    <Dropdown height={36} fontSize={14} focusable width="100%" value={risk.category} disabled={!canEdit}
                      options={opts(RISK_CATEGORIES, labels.category)}
                      onChange={v => onUpdate({ category: v as RiskCategory })} />
                  </div>
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_strategy', { defaultValue: 'Stratégie de réponse' })}</label>
                    <Dropdown height={36} fontSize={14} focusable width="100%" value={risk.response_strategy} disabled={!canEdit}
                      options={strategyOpts}
                      onChange={v => onUpdate({ response_strategy: v as RiskStrategy })} />
                  </div>
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_owner', { defaultValue: 'Responsable' })}</label>
                    <Dropdown height={36} fontSize={14} focusable width="100%" value={ownerValue} disabled={!canEdit}
                      placeholder={t('risk_owner_none', { defaultValue: 'Non attribué' })}
                      options={ownerList}
                      onChange={v => onUpdate({ owner_id: v || null })} />
                  </div>
                </div>
              )}

              <LongField
                label={t('risk_description', { defaultValue: 'Description' })}
                hint={t('risk_description_hint', { defaultValue: 'La cause, l’événement incertain et son effet sur le projet.' })}
                value={risk.description} rows={3} disabled={!canEdit}
                onCommit={v => onUpdate({ description: v })}
              />
              <LongField
                label={t('risk_trigger_signs', { defaultValue: 'Signes avant-coureurs' })}
                hint={t('risk_trigger_hint', { defaultValue: 'Ce qui annonce l’événement — le signal qui doit déclencher la réponse.' })}
                value={risk.trigger_signs} rows={2} disabled={!canEdit}
                onCommit={v => onUpdate({ trigger_signs: v })}
              />
              <LongField
                label={t('risk_response_plan', { defaultValue: 'Plan de réponse' })}
                hint={t('risk_response_hint', {
                  defaultValue: 'Ce qui est fait pour appliquer la stratégie « {{strategy}} ».',
                  strategy: labels.strategy[risk.response_strategy],
                })}
                value={risk.response_plan} rows={3} disabled={!canEdit}
                onCommit={v => onUpdate({ response_plan: v })}
              />
              <LongField
                label={t('risk_residual', { defaultValue: 'Risque résiduel' })}
                hint={t('risk_residual_hint', { defaultValue: 'Ce qui subsiste une fois la réponse exécutée.' })}
                value={risk.residual_notes} rows={2} disabled={!canEdit}
                onCommit={v => onUpdate({ residual_notes: v })}
              />

              {/* Quantitative analysis — the pair that produces the EMV. */}
              <div className="rounded-lg border border-border bg-surface-0 p-3">
                <p className="text-sm font-medium text-text-primary mb-2">
                  {t('risk_quant_title', { defaultValue: 'Analyse quantitative' })}
                </p>
                <div className="grid grid-cols-2 gap-3">
                  <NumberField
                    label={t('risk_probability_pct', { defaultValue: 'Probabilité chiffrée' })}
                    value={risk.probability_pct} min={0} max={100} step={1} suffix="%"
                    disabled={!canEdit}
                    onCommit={v => onUpdate({ probability_pct: v })}
                  />
                  <NumberField
                    label={t('risk_monetary_impact', { defaultValue: 'Impact monétaire' })}
                    value={risk.monetary_impact} step={100}
                    disabled={!canEdit}
                    onCommit={v => onUpdate({ monetary_impact: v })}
                  />
                </div>
                <div className="mt-2 flex items-center gap-2 flex-wrap">
                  <span className="text-sm text-text-secondary">
                    {t('risk_emv_label', { defaultValue: 'Valeur monétaire attendue (EMV)' })}
                  </span>
                  {emv === null ? (
                    <span className="text-sm text-text-tertiary">
                      {t('risk_emv_missing', { defaultValue: 'Non chiffrée — renseignez la probabilité et l’impact monétaire.' })}
                    </span>
                  ) : (
                    <span className="text-base font-semibold" style={{ color: KIND_FG[risk.kind] }}>
                      {emv < 0 ? '−' : '+'}{nf.format(Math.abs(emv))}
                    </span>
                  )}
                </div>
                {emv !== null && (
                  <p className="text-sm text-text-tertiary mt-0.5">
                    {risk.kind === 'threat'
                      ? t('risk_emv_threat_hint', { defaultValue: 'Une menace coûte : cette somme est la provision à porter pour ce risque.' })
                      : t('risk_emv_opportunity_hint', { defaultValue: 'Une opportunité rapporte : cette somme est le gain espéré pour ce risque.' })}
                  </p>
                )}
              </div>
            </div>
          </td>
        </tr>
      )}
    </>
  )
}

// ── Creation dialog (a modal is for creation only — edits happen in place) ────

function CreateDialog({ onClose, onCreate, pending }: {
  onClose: () => void
  onCreate: (data: RiskEdit & { title: string }) => void
  pending: boolean
}) {
  const { t } = useTranslation('office')
  const labels = useRiskLabels()
  const [title, setTitle] = useState('')
  const [code, setCode] = useState('')
  const [kind, setKind] = useState<RiskKind>('threat')
  const [category, setCategory] = useState<RiskCategory>('technical')
  const [probability, setProbability] = useState(3)
  const [impact, setImpact] = useState(3)

  const score = probability * impact

  const submit = () => {
    const trimmed = title.trim()
    if (!trimmed) return
    onCreate({
      title: trimmed,
      // An empty code is left out so the server can number it itself.
      ...(code.trim() ? { code: code.trim() } : {}),
      kind,
      category,
      probability,
      impact,
      status: 'identified',
      // Default strategy always belongs to the chosen nature's list.
      response_strategy: kind === 'threat' ? 'mitigate' : 'exploit',
    })
  }

  const levelOpts = (map: Record<number, string>): DropdownOption[] =>
    LEVELS.map(n => ({ value: String(n), label: `${n} — ${map[n]}` }))

  return (
    <FloatingWindow
      title={t('risk_new_title', { defaultValue: 'Nouveau risque' })}
      icon={<ShieldAlert size={16} />}
      onClose={onClose}
      defaultWidth={540} defaultHeight={440} padding={16} t={t}
      actions={{
        confirm: {
          label: t('common_create', { defaultValue: 'Créer' }),
          onClick: submit,
          disabled: !title.trim(),
          loading: pending,
          autoFocus: false,
        },
        cancel: { label: t('common_cancel', { defaultValue: 'Annuler' }) },
      }}
    >
      <div className="space-y-3">
        <div>
          <label className="text-sm text-text-secondary mb-1 block">
            {t('risk_col_title', { defaultValue: 'Intitulé' })}
          </label>
          <Input
            autoFocus
            value={title}
            placeholder={t('risk_title_ph', { defaultValue: 'L’événement incertain, formulé en une ligne…' })}
            onChange={e => setTitle(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submit() } }}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_code', { defaultValue: 'Code' })}</label>
            <Input value={code} placeholder={t('risk_code_auto', { defaultValue: 'Automatique' })} onChange={e => setCode(e.target.value)} />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_kind', { defaultValue: 'Nature' })}</label>
            <Dropdown height={36} fontSize={14} focusable width="100%" value={kind}
              buttonStyle={{ color: KIND_FG[kind], borderColor: KIND_FG[kind], fontWeight: 600 }}
              options={RISK_KINDS.map(v => ({ value: v, label: labels.kind[v] }))}
              onChange={v => setKind(v as RiskKind)} />
          </div>
          <div className="col-span-2">
            <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_category', { defaultValue: 'Catégorie' })}</label>
            <Dropdown height={36} fontSize={14} focusable width="100%" value={category}
              options={RISK_CATEGORIES.map(v => ({ value: v, label: labels.category[v] }))}
              onChange={v => setCategory(v as RiskCategory)} />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_probability', { defaultValue: 'Probabilité' })}</label>
            <Dropdown height={36} fontSize={14} focusable width="100%" value={String(probability)}
              options={levelOpts(labels.probability)}
              onChange={v => setProbability(Number(v))} />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('risk_col_impact', { defaultValue: 'Impact' })}</label>
            <Dropdown height={36} fontSize={14} focusable width="100%" value={String(impact)}
              options={levelOpts(labels.impact)}
              onChange={v => setImpact(Number(v))} />
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-sm text-text-secondary">{t('risk_col_score', { defaultValue: 'Gravité' })}</span>
          <ScoreChip score={score} />
          <span className="text-sm text-text-tertiary">{labels.severity[severityBand(score)]}</span>
        </div>
        <p className="text-sm text-text-tertiary">
          {t('risk_new_hint', { defaultValue: 'La stratégie de réponse, le plan et le chiffrage se saisissent ensuite dans le registre.' })}
        </p>
      </div>
    </FloatingWindow>
  )
}

// ── Summary strip ────────────────────────────────────────────────────────────

function Stat({ label, value, tone, icon, onClick, active, title }: {
  label: string
  value: string | number
  tone?: 'danger' | 'success' | 'muted'
  icon?: React.ReactNode
  /** Makes the tile a filter control rather than a plain figure. */
  onClick?: () => void
  active?: boolean
  title?: string
}) {
  const color = tone === 'danger' ? 'var(--color-danger)'
    : tone === 'success' ? 'var(--color-success)'
      : 'var(--color-text-primary)'
  const body = (
    <>
      <div className="flex items-center gap-1.5 text-sm text-text-secondary mb-0.5">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <p className="text-xl font-semibold" style={{ color }}>{value}</p>
    </>
  )
  const shell = 'flex-1 min-w-[8rem] rounded-lg bg-surface-1 border px-3 py-2'
  if (!onClick) return <div className={`${shell} border-border`}>{body}</div>
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-pressed={active}
      className={`${shell} text-left hover:bg-surface-2 ${active ? '' : 'border-border'}`}
      style={active ? { borderColor: color, boxShadow: `inset 0 0 0 1px ${color}` } : undefined}
    >
      {body}
    </button>
  )
}

// ── The register ─────────────────────────────────────────────────────────────

export default function RiskRegisterView({ projectId, canEdit = true, onOpenIssues }: {
  projectId: string
  /** False in the mobile reading mode, where the project is shown, not edited. */
  canEdit?: boolean
  /** Jumps to the issue log — offered once a risk has been turned into an issue. */
  onOpenIssues?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const labels = useRiskLabels()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()

  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [creating, setCreating] = useState(false)
  const [search, setSearch] = useState('')
  const [kindFilter, setKindFilter] = useState<'all' | RiskKind>('all')
  const [statusFilter, setStatusFilter] = useState<'all' | RiskStatus>('all')
  const [cell, setCell] = useState<Cell | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [notice, setNotice] = useState<{ text: string; offerIssues: boolean } | null>(null)

  const { data, isLoading } = useQuery({
    queryKey: ['risks', projectId],
    queryFn: () => projectsApi.getRisks(projectId),
  })
  // Owners are picked among the people who actually have access to the project.
  const { data: access } = useQuery({
    queryKey: ['proj-collab', projectId],
    queryFn: () => projectsApi.listCollaborators(projectId),
  })

  const nf = useMemo(
    () => new Intl.NumberFormat(i18n.language || 'fr-FR', { maximumFractionDigits: 0 }),
    [i18n.language],
  )

  const inval = () => { qc.invalidateQueries({ queryKey: ['risks', projectId] }) }
  // The api client flattens server errors to the root — `response.data` is undefined.
  const fail = (err: unknown) =>
    setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const createMut = useMutation({
    mutationFn: (payload: RiskEdit & { title: string }) => projectsApi.createRisk(projectId, payload),
    onSuccess: () => { setErrorMsg(null); setCreating(false); inval() },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ rid, patch }: { rid: string; patch: RiskEdit }) =>
      projectsApi.updateRisk(projectId, rid, patch),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (rid: string) => projectsApi.deleteRisk(projectId, rid),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const materializeMut = useMutation({
    mutationFn: (rid: string) => projectsApi.materializeRisk(projectId, rid),
    onSuccess: (res) => {
      setErrorMsg(null)
      // An issue already opened is not a failure — say it plainly.
      setNotice({
        text: res.existing
          ? t('risk_materialize_existing', {
            defaultValue: 'Un incident était déjà ouvert pour ce risque : {{code}} — {{title}}.',
            code: res.issue.code, title: res.issue.title,
          })
          : t('risk_materialize_done', {
            defaultValue: 'Incident {{code}} ouvert dans le journal, rattaché à ce risque.',
            code: res.issue.code,
          }),
        offerIssues: !!onOpenIssues,
      })
      inval()
      qc.invalidateQueries({ queryKey: ['issues', projectId] })
    },
    onError: fail,
  })

  const all = useMemo(() => data?.risks ?? [], [data])
  const matrix = data?.matrix ?? []
  const summary = data?.summary

  const ownerOptions: DropdownOption[] = useMemo(() => {
    const seen = new Set<string>()
    const list: DropdownOption[] = [
      { value: '', label: t('risk_owner_none', { defaultValue: 'Non attribué' }) },
    ]
    const push = (id: string, name: string | null, email: string) => {
      if (!id || seen.has(id)) return
      seen.add(id)
      list.push({ value: id, label: name || email })
    }
    if (access?.owner) push(access.owner.id, access.owner.display_name, access.owner.email)
    for (const c of access?.collaborators ?? []) push(c.user_id, c.display_name, c.email)
    return list
  }, [access, t])

  // Highest exposure first — the register is read from the top.
  const sorted = useMemo(
    () => all.slice().sort((a, b) => b.score - a.score || a.code.localeCompare(b.code, undefined, { numeric: true })),
    [all],
  )

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return sorted.filter(r => {
      if (kindFilter !== 'all' && r.kind !== kindFilter) return false
      if (statusFilter !== 'all' && r.status !== statusFilter) return false
      // Same rule as the matrix, which plots only what may still happen:
      // otherwise the row count would not match the figure just clicked.
      if (cell && (!STILL_AHEAD.has(r.status) || r.probability !== cell.p || r.impact !== cell.i)) return false
      if (!q) return true
      return r.title.toLowerCase().includes(q)
        || r.code.toLowerCase().includes(q)
        || r.description.toLowerCase().includes(q)
        || (r.owner_name ?? '').toLowerCase().includes(q)
    })
  }, [sorted, search, kindFilter, statusFilter, cell])

  const toggle = (id: string) => setExpanded(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  const askDelete = async (risk: Risk) => {
    const ok = await confirm({
      title: t('risk_delete_title', { defaultValue: 'Supprimer ce risque ?' }),
      message: t('risk_delete_msg', {
        defaultValue: '« {{title}} » sera retiré du registre et de la matrice.',
        title: risk.title,
      }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) deleteMut.mutate(risk.id)
  }

  const askMaterialize = async (risk: Risk) => {
    const ok = await confirm({
      title: t('risk_materialize_title', { defaultValue: 'Ce risque s’est produit ?' }),
      message: t('risk_materialize_msg', {
        defaultValue: 'Un incident sera ouvert dans le journal, rattaché à « {{title}} », et le risque passera au statut « Survenu ». Il quittera alors la matrice, qui ne montre que ce qui peut encore arriver.',
        title: risk.title,
      }),
      confirmLabel: t('risk_materialize_confirm', { defaultValue: 'Ouvrir l’incident' }),
      variant: 'warning',
    })
    if (ok) materializeMut.mutate(risk.id)
  }

  // Columns actually rendered — the detail panel takes over the rest on mobile.
  const colCount = isMobile ? 5 : 9

  const filtersActive = search.trim() !== '' || kindFilter !== 'all' || statusFilter !== 'all' || cell !== null
  const clearFilters = () => { setSearch(''); setKindFilter('all'); setStatusFilter('all'); setCell(null) }

  const th = 'text-left text-sm font-medium text-text-secondary px-2 py-2 whitespace-nowrap'

  const totalEmv = summary?.total_emv ?? 0
  const emvTone = totalEmv < 0 ? 'danger' : totalEmv > 0 ? 'success' : 'muted'

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <ShieldAlert size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('risk_title', { defaultValue: 'Registre des risques' })}
          </h1>
          {summary && summary.total > 0 && (
            <span className="text-sm text-text-tertiary">
              {t('risk_count_summary', {
                defaultValue: '{{total}} risque(s) · {{open}} ouvert(s)',
                total: summary.total, open: summary.open,
              })}
            </span>
          )}
          <div className="flex-1" />
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreating(true)}>
              {t('risk_new', { defaultValue: 'Nouveau risque' })}
            </Button>
          )}
        </div>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)}>{errorMsg}</Callout>
        )}

        {notice && (
          <Callout
            variant="info"
            icon={<Siren size={16} />}
            dismissible
            onDismiss={() => setNotice(null)}
            action={notice.offerIssues && onOpenIssues
              ? { label: t('risk_open_issues', { defaultValue: 'Voir le journal des incidents' }), onClick: () => { setNotice(null); onOpenIssues() } }
              : undefined}
          >
            {notice.text}
          </Callout>
        )}

        {isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : all.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              icon={<ShieldAlert size={26} />}
              variant="first-use"
              title={t('risk_empty_title', { defaultValue: 'Aucun risque recensé' })}
              description={t('risk_empty_desc', { defaultValue: 'Le registre recense ce qui peut encore arriver au projet — menaces comme opportunités —, le positionne dans la matrice probabilité/impact et attribue à chacun une stratégie de réponse et un responsable.' })}
              action={canEdit ? {
                label: t('risk_new', { defaultValue: 'Nouveau risque' }),
                onClick: () => setCreating(true),
                icon: <Plus size={15} />,
              } : undefined}
              t={t}
            />
          </div>
        ) : (
          <>
            {/* Summary — the numbers a steering committee asks for. */}
            {summary && (
              <div className="space-y-3">
                <div className="flex gap-3 flex-wrap">
                  <Stat
                    label={t('risk_stat_total', { defaultValue: 'Risques' })}
                    value={summary.total}
                    icon={<Grid3x3 size={14} />}
                  />
                  <Stat
                    label={t('risk_stat_open', { defaultValue: 'Encore ouverts' })}
                    value={summary.open}
                  />
                  {/* Without this tile the gap between `total` and `open` is
                      unexplainable: a risk that came true is no longer open. */}
                  <Stat
                    label={t('risk_stat_occurred', { defaultValue: 'Survenus' })}
                    value={summary.occurred}
                    tone={summary.occurred > 0 ? 'danger' : 'muted'}
                    icon={<Siren size={14} />}
                    active={statusFilter === 'occurred'}
                    title={statusFilter === 'occurred'
                      ? t('risk_stat_occurred_off', { defaultValue: 'Afficher de nouveau tous les statuts' })
                      : t('risk_stat_occurred_on', { defaultValue: 'N’afficher que les risques survenus' })}
                    onClick={() => {
                      // Toggling the tile drives the existing status filter, and
                      // drops the cell filter, which excludes occurred risks.
                      setStatusFilter(prev => (prev === 'occurred' ? 'all' : 'occurred'))
                      setCell(null)
                    }}
                  />
                  <Stat
                    label={t('risk_stat_threats', { defaultValue: 'Menaces' })}
                    value={summary.threats}
                    tone="danger"
                    icon={<TrendingDown size={14} />}
                  />
                  <Stat
                    label={t('risk_stat_opportunities', { defaultValue: 'Opportunités' })}
                    value={summary.opportunities}
                    tone="success"
                    icon={<TrendingUp size={14} />}
                  />
                </div>

                {/* EMV is a provision, so it is stated in words, never as a bare number. */}
                <div className="rounded-lg border border-border bg-surface-0 px-4 py-3">
                  <div className="flex items-center gap-2 flex-wrap">
                    <Coins size={16} className="text-text-secondary shrink-0" />
                    <span className="text-sm text-text-secondary">
                      {t('risk_emv_total_label', { defaultValue: 'Valeur monétaire attendue du portefeuille' })}
                    </span>
                    <span
                      className="text-xl font-semibold"
                      style={{
                        color: emvTone === 'danger' ? 'var(--color-danger)'
                          : emvTone === 'success' ? 'var(--color-success)'
                            : 'var(--color-text-primary)',
                      }}
                    >
                      {totalEmv < 0 ? '−' : totalEmv > 0 ? '+' : ''}{nf.format(Math.abs(totalEmv))}
                    </span>
                  </div>
                  <p className="text-sm text-text-secondary mt-1">
                    {summary.priced === 0
                      ? t('risk_emv_none_priced', { defaultValue: 'Aucun risque encore ouvert n’est chiffré : renseignez la probabilité en % et l’impact monétaire d’un risque pour que le projet sache ce qu’il devrait provisionner.' })
                      : totalEmv < 0
                        ? t('risk_emv_exposure', {
                          defaultValue: 'C’est une exposition : le projet devrait provisionner cette somme pour couvrir ses menaces. Calculée sur {{priced}} des {{open}} risque(s) encore ouvert(s) — les risques survenus et clos en sont exclus.',
                          priced: summary.priced, open: summary.open,
                        })
                        : totalEmv > 0
                          ? t('risk_emv_gain', {
                            defaultValue: 'C’est un gain espéré : les opportunités chiffrées l’emportent sur les menaces. Calculé sur {{priced}} des {{open}} risque(s) encore ouvert(s) — les risques survenus et clos en sont exclus.',
                            priced: summary.priced, open: summary.open,
                          })
                          : t('risk_emv_neutral', {
                            defaultValue: 'Menaces et opportunités chiffrées s’équilibrent exactement. Calculé sur {{priced}} des {{open}} risque(s) encore ouvert(s) — les risques survenus et clos en sont exclus.',
                            priced: summary.priced, open: summary.open,
                          })}
                  </p>
                </div>
              </div>
            )}

            {/* The matrix — and the register's main filter. */}
            <div className="bg-surface-0 border border-border rounded-xl p-4">
              <div className="flex items-center gap-2 flex-wrap mb-3">
                <h2 className="text-base font-medium text-text-primary">
                  {t('risk_matrix_title', { defaultValue: 'Matrice probabilité / impact' })}
                </h2>
                <span className="text-sm text-text-tertiary">
                  {t('risk_matrix_hint', { defaultValue: 'Cliquez une case pour ne lire que ses risques.' })}
                </span>
              </div>

              <RiskMatrix matrix={matrix} selected={cell} onSelect={setCell} isMobile={isMobile} />

              {cell && (
                <div className="mt-3 flex items-center gap-2 flex-wrap">
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-2 pl-3 pr-1 py-1 text-sm text-text-primary">
                    {t('risk_cell_filter', {
                      defaultValue: 'Probabilité {{p}} × impact {{i}} — gravité {{score}}',
                      p: cell.p, i: cell.i, score: cell.p * cell.i,
                    })}
                    <button
                      onClick={() => setCell(null)}
                      title={t('risk_cell_filter_clear', { defaultValue: 'Retirer ce filtre' })}
                      className="p-1 rounded-full text-text-tertiary hover:text-danger hover:bg-surface-3"
                    >
                      <X size={13} />
                    </button>
                  </span>
                </div>
              )}
            </div>

            {/* Filters */}
            <div className="flex items-center gap-2 flex-wrap">
              <Input
                className="w-64"
                value={search}
                leftIcon={<Search size={15} />}
                placeholder={t('risk_search_ph', { defaultValue: 'Rechercher un risque…' })}
                onChange={e => setSearch(e.target.value)}
              />
              <Dropdown
                height={36} fontSize={14} focusable width={170}
                value={kindFilter}
                options={[
                  { value: 'all', label: t('risk_kind_all', { defaultValue: 'Toutes natures' }) },
                  ...RISK_KINDS.map(v => ({ value: v, label: labels.kind[v] })),
                ]}
                onChange={v => setKindFilter(v as 'all' | RiskKind)}
              />
              <Dropdown
                height={36} fontSize={14} focusable width={185}
                value={statusFilter}
                options={[
                  { value: 'all', label: t('risk_status_all', { defaultValue: 'Tous statuts' }) },
                  ...RISK_STATUSES.map(v => ({ value: v, label: labels.status[v] })),
                ]}
                onChange={v => setStatusFilter(v as 'all' | RiskStatus)}
              />
              {filtersActive && (
                <Button size="sm" variant="text" onClick={clearFilters}>
                  {t('common_clear_filters', { defaultValue: 'Effacer les filtres' })}
                </Button>
              )}
            </div>

            {filtered.length === 0 ? (
              <div className="bg-surface-0 border border-border rounded-xl">
                <EmptyState
                  icon={<Search size={26} />}
                  variant="no-results"
                  title={t('risk_no_results', { defaultValue: 'Aucun risque ne correspond' })}
                  description={t('risk_no_results_desc', { defaultValue: 'Les filtres actifs excluent tous les risques du registre.' })}
                  action={{ label: t('common_clear_filters', { defaultValue: 'Effacer les filtres' }), onClick: clearFilters }}
                  t={t}
                />
              </div>
            ) : (
              // A wide register scrolls INSIDE its own box, never past the page edge.
              <div className="bg-surface-0 border border-border rounded-xl overflow-x-auto">
                <table className="w-full border-collapse" style={{ minWidth: isMobile ? 520 : 1280 }}>
                  <thead>
                    <tr>
                      <th className={th} style={{ width: 180 }}>{t('risk_col_code', { defaultValue: 'Code' })}</th>
                      <th className={th}>{t('risk_col_title', { defaultValue: 'Intitulé' })}</th>
                      {!isMobile && <th className={th} style={{ width: 150 }}>{t('risk_col_kind', { defaultValue: 'Nature' })}</th>}
                      {!isMobile && <th className={th} style={{ width: 175 }}>{t('risk_col_category', { defaultValue: 'Catégorie' })}</th>}
                      <th className={th} style={{ width: 195 }}>{t('risk_col_pi', { defaultValue: 'P × I' })}</th>
                      {!isMobile && <th className={th} style={{ width: 160 }}>{t('risk_col_strategy', { defaultValue: 'Stratégie' })}</th>}
                      {!isMobile && <th className={th} style={{ width: 175 }}>{t('risk_col_owner', { defaultValue: 'Responsable' })}</th>}
                      <th className={th} style={{ width: 170 }}>{t('risk_col_status', { defaultValue: 'Statut' })}</th>
                      <th className={th} style={{ width: 80 }} />
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map(risk => (
                      <RiskRow
                        key={risk.id}
                        risk={risk}
                        canEdit={canEdit}
                        isMobile={isMobile}
                        expanded={expanded.has(risk.id)}
                        onToggle={() => toggle(risk.id)}
                        ownerOptions={ownerOptions}
                        colCount={colCount}
                        nf={nf}
                        onUpdate={patch => updateMut.mutate({ rid: risk.id, patch })}
                        onDelete={() => { void askDelete(risk) }}
                        onMaterialize={() => { void askMaterialize(risk) }}
                      />
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}
      </div>

      {creating && (
        <CreateDialog
          onClose={() => setCreating(false)}
          onCreate={payload => createMut.mutate(payload)}
          pending={createMut.isPending}
        />
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
