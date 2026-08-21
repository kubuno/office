import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { format, parseISO } from 'date-fns'
import { getDateLocale, prompt, useConfirm } from '@kubuno/sdk'
import {
  Gauge, Plus, Trash2, ChevronDown, ChevronRight, FlaskConical, TrendingUp, TrendingDown,
  Minus, ClipboardCheck, Coins, ShieldCheck, TriangleAlert, ArrowRight, Ruler, CircleAlert,
} from 'lucide-react'
import {
  Badge, Button, Callout, Card, ConfirmDialog, Dropdown, EmptyState, FloatingWindow,
  Input, ProgressBar, Textarea, Toggle, Tooltip, useIsMobile,
  type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type CoqCategory, type QualityCheck, type QualityCheckEdit, type QualityDirection,
  type QualityFrequency, type QualityMetric, type QualityMetricEdit, type QualityMetricLine,
  type QualityReading, type QualityResult,
} from '../api'

// Quality — what "good enough" means in numbers, and the evidence it was met.
//
// Three things live here, in that order of importance:
//
//  1. THE METRICS. Their point is not the latest number but its position against
//     the bound it must respect — which is why every metric carries a sparkline
//     with its tolerance band drawn on it, and why entering a reading is a field
//     on the row rather than a dialog: it is the gesture of this screen.
//
//     The server answers THREE states, not two. `conforms: null` means the metric
//     states nothing to compare against — no target, no tolerance. Calling that
//     "conforming" would invent a standard nobody set, so it is shown as NOT
//     ASSESSABLE, with a way to give it a bound. `at_risk` is the third: still
//     inside the band, but close to an edge and heading for it. Painting it green
//     with the rest would hide the only metric worth looking at today.
//
//  2. THE CHECKS. A check always bears on a deliverable or a work package, and a
//     failure always carries what was found — both are server rules, asked for
//     up front here rather than discovered through a 422.
//
//  3. THE COST OF QUALITY. Not a pie chart: what the project spends to AVOID
//     failure, set against what it pays BECAUSE failure happened.

// ── Vocabulary ───────────────────────────────────────────────────────────────

const DIRECTIONS: QualityDirection[] = ['higher', 'lower', 'target']
const FREQUENCIES: QualityFrequency[] = ['continuous', 'daily', 'weekly', 'sprint', 'monthly', 'milestone', 'once']
const RESULTS: QualityResult[] = ['pending', 'pass', 'fail', 'waived']
/** Conformance first, then failure: the two halves the whole section contrasts. */
const COQ_ORDER: CoqCategory[] = ['prevention', 'appraisal', 'internal_failure', 'external_failure']

function directionLabel(d: QualityDirection, t: TFunction): string {
  return {
    higher: t('qual_dir_higher', { defaultValue: 'Plus c’est haut, mieux c’est' }),
    lower:  t('qual_dir_lower', { defaultValue: 'Plus c’est bas, mieux c’est' }),
    target: t('qual_dir_target', { defaultValue: 'Au plus près de la cible' }),
  }[d]
}

function frequencyLabel(f: QualityFrequency, t: TFunction): string {
  return {
    continuous: t('qual_freq_continuous', { defaultValue: 'En continu' }),
    daily:      t('qual_freq_daily', { defaultValue: 'Quotidienne' }),
    weekly:     t('qual_freq_weekly', { defaultValue: 'Hebdomadaire' }),
    sprint:     t('qual_freq_sprint', { defaultValue: 'Par itération' }),
    monthly:    t('qual_freq_monthly', { defaultValue: 'Mensuelle' }),
    milestone:  t('qual_freq_milestone', { defaultValue: 'À chaque jalon' }),
    once:       t('qual_freq_once', { defaultValue: 'Une seule fois' }),
  }[f]
}

function resultLabel(r: QualityResult, t: TFunction): string {
  return {
    pending: t('qual_result_pending', { defaultValue: 'En attente' }),
    pass:    t('qual_result_pass', { defaultValue: 'Conforme' }),
    fail:    t('qual_result_fail', { defaultValue: 'En échec' }),
    waived:  t('qual_result_waived', { defaultValue: 'Dérogation' }),
  }[r]
}

function coqLabel(c: CoqCategory, t: TFunction): string {
  return {
    prevention:       t('qual_coq_prevention', { defaultValue: 'Prévention' }),
    appraisal:        t('qual_coq_appraisal', { defaultValue: 'Évaluation' }),
    internal_failure: t('qual_coq_internal', { defaultValue: 'Défaillance interne' }),
    external_failure: t('qual_coq_external', { defaultValue: 'Défaillance externe' }),
  }[c]
}

const RESULT_BADGE: Record<QualityResult, 'neutral' | 'success' | 'danger' | 'warning'> = {
  pending: 'neutral',
  pass:    'success',
  fail:    'danger',
  waived:  'warning',
}

// ── Conformity: the three states, plus the two the UI has to tell apart ──────

/**
 * `unmeasured` — no reading at all: nothing has been observed yet.
 * `unrated`    — read, but the metric sets no target and no tolerance, so there
 *                is nothing to compare it to. NOT "conforming".
 * `breach`     — outside the tolerance.
 * `drifting`   — inside it, but near an edge and moving towards it.
 * `conform`    — inside it, with room to spare.
 *
 * The server's `summary.unmeasured` folds the first two together; this screen
 * counts them apart, since the fix differs: measure it, or define a bound.
 */
type MetricState = 'unmeasured' | 'unrated' | 'breach' | 'drifting' | 'conform'

function metricState(line: QualityMetricLine): MetricState {
  if (!line.latest) return 'unmeasured'
  if (line.conforms === null) return 'unrated'
  if (!line.conforms) return 'breach'
  return line.at_risk ? 'drifting' : 'conform'
}

const STATE_BADGE: Record<MetricState, 'neutral' | 'success' | 'warning' | 'danger'> = {
  unmeasured: 'neutral',
  unrated:    'neutral',
  breach:     'danger',
  drifting:   'warning',
  conform:    'success',
}

/** Stroke of the trend line; a CSS variable so a theme recolours every chart. */
const STATE_COLOR: Record<MetricState, string> = {
  unmeasured: 'var(--color-text-tertiary)',
  unrated:    'var(--color-text-tertiary)',
  breach:     'var(--color-danger)',
  drifting:   'var(--color-warning)',
  conform:    'var(--color-success)',
}

function stateLabel(s: MetricState, t: TFunction): string {
  return {
    unmeasured: t('qual_state_unmeasured', { defaultValue: 'Jamais mesuré' }),
    unrated:    t('qual_state_unrated', { defaultValue: 'Non évaluable' }),
    breach:     t('qual_state_breach', { defaultValue: 'Hors tolérance' }),
    drifting:   t('qual_state_drifting', { defaultValue: 'Conforme, mais dérive' }),
    conform:    t('qual_state_conform', { defaultValue: 'Conforme et stable' }),
  }[s]
}

const nf = (lang: string, v: number, digits = 2) =>
  new Intl.NumberFormat(lang, { maximumFractionDigits: digits }).format(v)

const withUnit = (lang: string, v: number, unit: string) =>
  unit ? `${nf(lang, v)} ${unit}` : nf(lang, v)

/**
 * The requirement in words. The bounds win over `target`/`direction` because
 * that is exactly what the server compares against: a metric carrying both a
 * ceiling and a target is judged on the ceiling.
 */
function requirementText(m: QualityMetric, lang: string, t: TFunction): string | null {
  const { tolerance_min: lo, tolerance_max: hi } = m
  if (lo !== null && hi !== null) {
    return t('qual_req_between', {
      defaultValue: 'entre {{lo}} et {{hi}}',
      lo: nf(lang, lo), hi: withUnit(lang, hi, m.unit),
    })
  }
  if (lo !== null) return `≥ ${withUnit(lang, lo, m.unit)}`
  if (hi !== null) return `≤ ${withUnit(lang, hi, m.unit)}`
  if (m.target !== null) {
    if (m.direction === 'higher') return `≥ ${withUnit(lang, m.target, m.unit)}`
    if (m.direction === 'lower') return `≤ ${withUnit(lang, m.target, m.unit)}`
    // An exact target with no tolerance can only ever be missed — which is why
    // the server refuses to judge it. Say the target, not a rule.
    return t('qual_req_target_only', {
      defaultValue: 'cible {{v}}, sans tolérance',
      v: withUnit(lang, m.target, m.unit),
    })
  }
  return null
}

/**
 * Is the last move an improvement? Same precedence as the server: a two-sided
 * band is judged from its middle, a single bound from the safe side, and only a
 * metric with neither falls back to `direction`.
 */
function trendQuality(m: QualityMetric, value: number, previous: number): 'better' | 'worse' | 'flat' {
  if (value === previous) return 'flat'
  const { tolerance_min: lo, tolerance_max: hi } = m
  if (lo !== null && hi !== null) {
    const mid = (lo + hi) / 2
    return Math.abs(value - mid) < Math.abs(previous - mid) ? 'better' : 'worse'
  }
  if (lo !== null) return value > previous ? 'better' : 'worse'
  if (hi !== null) return value < previous ? 'better' : 'worse'
  if (m.target !== null && m.direction === 'target') {
    return Math.abs(value - m.target) < Math.abs(previous - m.target) ? 'better' : 'worse'
  }
  if (m.direction === 'higher') return value > previous ? 'better' : 'worse'
  if (m.direction === 'lower') return value < previous ? 'better' : 'worse'
  return 'flat'
}

/** Parses a typed number, tolerating the decimal comma. Null when unusable. */
function parseNumber(raw: string): number | null {
  const s = raw.trim().replace(',', '.')
  if (!s) return null
  const n = Number(s)
  return Number.isFinite(n) ? n : null
}

/** Flattened by the api client: server errors carry `message` at the root. */
const errText = (err: unknown, fallback: string) =>
  (err as { message?: string }).message ?? fallback

// ── Sparkline ────────────────────────────────────────────────────────────────

/**
 * The trend of a metric, with its tolerance drawn on it.
 *
 * A curve of raw values says nothing: 240 ms is excellent under a 300 ms ceiling
 * and a disaster under 200. So the scale is stretched to CONTAIN the bounds, the
 * conforming band is painted, and each bound gets its dashed line. Reading the
 * chart then answers the only question that matters — how much room is left.
 *
 * Inline SVG, no charting library: a hundred of these must cost nothing.
 */
function Sparkline({ series, min, max, color, label, width = 148, height = 42 }: {
  series: QualityReading[]
  /** Lower bound of the conforming band; null leaves that side open. */
  min: number | null
  max: number | null
  /** Stroke of the trend line — the metric's state colour. */
  color: string
  /** Text description, for the tooltip and for assistive technology. */
  label: string
  width?: number
  height?: number
}) {
  const pad = 5
  const values = series.map(r => r.value)
  if (values.length === 0) return null

  // The scale spans readings AND bounds, so a threshold is never off-screen.
  const scope = [...values]
  if (min !== null) scope.push(min)
  if (max !== null) scope.push(max)
  let lo = Math.min(...scope)
  let hi = Math.max(...scope)
  if (hi === lo) { hi = lo + 1; lo -= 1 }
  const breathe = (hi - lo) * 0.12
  lo -= breathe; hi += breathe

  const y = (v: number) => pad + ((hi - v) / (hi - lo)) * (height - 2 * pad)
  const x = (i: number) =>
    values.length === 1 ? width / 2 : pad + (i / (values.length - 1)) * (width - 2 * pad)

  const points = values.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(' ')
  const lastX = x(values.length - 1)
  const lastY = y(values[values.length - 1])
  // Open sides bleed to the edge: nothing bounds them.
  const bandTop = max !== null ? y(max) : 0
  const bandBottom = min !== null ? y(min) : height

  return (
    <Tooltip label={label}>
      <span className="inline-flex shrink-0">
        <svg
          width={width} height={height} viewBox={`0 0 ${width} ${height}`}
          className="block overflow-visible" role="img" aria-label={label}
        >
          {(min !== null || max !== null) && (
            <rect
              x={0} y={bandTop} width={width} height={Math.max(0, bandBottom - bandTop)}
              fill="var(--color-success-light)"
            />
          )}
          {max !== null && (
            <line x1={0} x2={width} y1={y(max)} y2={y(max)}
              stroke="var(--color-danger)" strokeWidth={1} strokeDasharray="3 3" opacity={0.75} />
          )}
          {min !== null && (
            <line x1={0} x2={width} y1={y(min)} y2={y(min)}
              stroke="var(--color-danger)" strokeWidth={1} strokeDasharray="3 3" opacity={0.75} />
          )}
          {values.length > 1 && (
            <polyline points={points} fill="none" stroke={color} strokeWidth={1.75}
              strokeLinejoin="round" strokeLinecap="round" />
          )}
          <circle cx={lastX} cy={lastY} r={3} fill={color} />
        </svg>
      </span>
    </Tooltip>
  )
}

// ── Small editing primitives, defined once at module level ───────────────────
// Declaring them inside the render would make a NEW component type on every
// pass: React would unmount the field and the value being typed would vanish.

/** Text field committed on blur — one PATCH per edit, not one per keystroke. */
function CommitInput({ value, placeholder, disabled, required, className, mono, onCommit }: {
  value: string
  placeholder?: string
  disabled?: boolean
  /** Refuse an empty value and snap back; the server answers 422 anyway. */
  required?: boolean
  className?: string
  mono?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <Input
      className={`${className ?? ''}${mono ? ' font-mono' : ''}`}
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

/** Numeric field that can legitimately be emptied — a bound one chooses not to set. */
function CommitNumber({ value, placeholder, disabled, className, onCommit }: {
  value: number | null
  placeholder?: string
  disabled?: boolean
  className?: string
  onCommit: (next: number | null) => void
}) {
  const asText = value === null ? '' : String(value)
  const [draft, setDraft] = useState(asText)
  useEffect(() => { setDraft(asText) }, [asText])
  return (
    <Input
      type="text" inputMode="decimal"
      className={className}
      value={draft}
      placeholder={placeholder}
      disabled={disabled}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        const next = parseNumber(draft)
        if (draft.trim() !== '' && next === null) { setDraft(asText); return }
        if (next === value) { setDraft(asText); return }
        onCommit(next)
      }}
    />
  )
}

function CommitTextarea({ value, placeholder, disabled, rows, onCommit }: {
  value: string
  placeholder?: string
  disabled?: boolean
  rows?: number
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])
  return (
    <Textarea
      value={draft}
      rows={rows ?? 3}
      placeholder={placeholder}
      disabled={disabled}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => { if (draft !== value) onCommit(draft) }}
    />
  )
}

function Field({ label, children, className }: {
  label: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div className={className}>
      <label className="text-sm text-text-secondary mb-1 block">{label}</label>
      {children}
    </div>
  )
}

// ── Entering a reading ───────────────────────────────────────────────────────

/**
 * The most frequent gesture of the screen, so it sits ON the metric's row: a
 * value, its date, and nothing else in the way. A dialog for this would turn one
 * keystroke into four clicks.
 */
function MeasureForm({ unit, disabled, pending, compact, onSubmit }: {
  unit: string
  disabled?: boolean
  pending?: boolean
  /** Narrow screens drop the optional note to keep the row on one line. */
  compact?: boolean
  onSubmit: (value: number, measuredOn: string, notes: string) => void
}) {
  const { t } = useTranslation('office')
  const today = () => new Date().toISOString().slice(0, 10)
  const [value, setValue] = useState('')
  const [day, setDay] = useState(today)
  const [notes, setNotes] = useState('')

  const parsed = parseNumber(value)
  const submit = () => {
    if (parsed === null || disabled) return
    onSubmit(parsed, day || today(), notes.trim())
    setValue(''); setNotes(''); setDay(today())
  }

  return (
    <div className="flex items-center gap-2 flex-wrap">
      <Input
        type="text" inputMode="decimal"
        className="w-24"
        value={value}
        disabled={disabled}
        placeholder={unit || t('qual_measure_ph', { defaultValue: 'Valeur' })}
        aria-label={t('qual_measure_value', { defaultValue: 'Valeur mesurée' })}
        onChange={e => setValue(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submit() } }}
      />
      <Input
        type="date"
        className="w-[9.5rem]"
        value={day}
        disabled={disabled}
        aria-label={t('qual_measure_date', { defaultValue: 'Date de la mesure' })}
        onChange={e => setDay(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submit() } }}
      />
      {!compact && (
        <Input
          className="w-40"
          value={notes}
          disabled={disabled}
          placeholder={t('qual_measure_note_ph', { defaultValue: 'Note (facultatif)' })}
          aria-label={t('qual_measure_note', { defaultValue: 'Note sur la mesure' })}
          onChange={e => setNotes(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submit() } }}
        />
      )}
      {/* `md` on purpose: h-9, the height of the two fields it sits beside. */}
      <Button
        size="md" variant="secondary" icon={<Plus size={14} />}
        disabled={disabled || parsed === null} loading={pending}
        onClick={submit}
      >
        {t('qual_measure_add', { defaultValue: 'Relever' })}
      </Button>
    </div>
  )
}

// ── One metric ───────────────────────────────────────────────────────────────

function MetricCard({
  line, canEdit, isMobile, expanded, onToggle, deliverableOptions, taskOptions,
  measuring, onPatch, onDelete, onMeasure, onDeleteReading,
}: {
  line: QualityMetricLine
  canEdit: boolean
  isMobile: boolean
  expanded: boolean
  onToggle: () => void
  deliverableOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  measuring: boolean
  onPatch: (patch: QualityMetricEdit) => void
  onDelete: () => void
  onMeasure: (value: number, measuredOn: string, notes: string) => void
  onDeleteReading: (readingId: string) => void
}) {
  const { t, i18n } = useTranslation('office')
  const lang = i18n.language
  const m = line.metric
  const state = metricState(line)
  const requirement = requirementText(m, lang, t)
  // With both bounds the margin is a share of the band; with one, the distance
  // left before it. Same number, different sentence.
  const hasBand = m.tolerance_min !== null && m.tolerance_max !== null

  const fmtDay = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(lang) })

  // The move since the previous reading — the half of the story a single value
  // cannot tell. Both halves are needed, so they are derived together.
  const move = line.latest !== null && line.previous !== null
    ? { delta: line.latest.value - line.previous,
        previous: line.previous,
        trend: trendQuality(m, line.latest.value, line.previous) }
    : null

  const sparkLabel = t('qual_spark_label', {
    defaultValue: '{{n}} mesure(s){{req}}',
    n: line.series.length,
    req: requirement ? t('qual_spark_req', { defaultValue: ' — exigence : {{req}}', req: requirement }) : '',
  })

  return (
    <div className="border border-border rounded-xl bg-surface-0 overflow-hidden">

      {/* Head — identity, last value, requirement, trend, and the reading field. */}
      <div className="p-3 flex items-start gap-3 flex-wrap">
        <button
          onClick={onToggle}
          title={t('qual_toggle_detail', { defaultValue: 'Détail de l’indicateur' })}
          className="p-1 mt-1 rounded text-text-tertiary hover:bg-surface-2 shrink-0"
        >
          {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
        </button>

        <div className="min-w-0 flex-1 basis-64">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-xs text-text-tertiary shrink-0">{m.code || '—'}</span>
            <span className="text-sm font-medium text-text-primary break-words">{m.name}</span>
            <Badge variant={STATE_BADGE[state]} dot>{stateLabel(state, t)}</Badge>
            {!m.is_active && (
              <Badge variant="neutral">{t('qual_inactive', { defaultValue: 'Suspendu' })}</Badge>
            )}
          </div>

          <div className="mt-1.5 flex items-baseline gap-2 flex-wrap">
            {line.latest ? (
              <>
                <span className="text-xl font-medium text-text-primary tabular-nums">
                  {nf(lang, line.latest.value)}
                </span>
                {m.unit && <span className="text-sm text-text-secondary">{m.unit}</span>}
                {move !== null && (
                  <span
                    className={`inline-flex items-center gap-0.5 text-xs tabular-nums ${
                      move.trend === 'better' ? 'text-success' : move.trend === 'worse' ? 'text-danger' : 'text-text-tertiary'
                    }`}
                    title={t('qual_delta_title', {
                      defaultValue: 'Mesure précédente : {{v}}',
                      v: withUnit(lang, move.previous, m.unit),
                    })}
                  >
                    {move.delta > 0 ? <TrendingUp size={13} /> : move.delta < 0 ? <TrendingDown size={13} /> : <Minus size={13} />}
                    {move.delta > 0 ? '+' : ''}{nf(lang, move.delta)}
                  </span>
                )}
                <span className="text-xs text-text-tertiary">
                  {t('qual_measured_on', { defaultValue: 'le {{d}}', d: fmtDay(line.latest.measured_on) })}
                </span>
              </>
            ) : (
              <span className="text-sm text-text-tertiary">
                {t('qual_no_reading', { defaultValue: 'Aucune mesure relevée.' })}
              </span>
            )}
          </div>

          <div className="mt-1 text-xs text-text-secondary flex items-center gap-1.5 flex-wrap">
            <Ruler size={12} className="shrink-0 text-text-tertiary" />
            {requirement ? (
              <span>{t('qual_requirement', { defaultValue: 'Exigence : {{req}}', req: requirement })}</span>
            ) : (
              <span className="text-text-tertiary">
                {t('qual_no_requirement', { defaultValue: 'Aucune exigence : ni cible ni tolérance.' })}
              </span>
            )}
            {line.margin !== null && (
              <span className="text-text-tertiary">
                · {hasBand
                  ? t('qual_margin_band', {
                      defaultValue: 'marge {{p}} % de la plage',
                      p: nf(lang, line.margin * 100, 0),
                    })
                  : t('qual_margin_bound', {
                      defaultValue: 'marge {{p}} % avant la borne',
                      p: nf(lang, line.margin * 100, 0),
                    })}
              </span>
            )}
          </div>
        </div>

        {line.series.length > 0 && (
          <Sparkline
            series={line.series}
            min={m.tolerance_min}
            max={m.tolerance_max}
            color={STATE_COLOR[state]}
            label={sparkLabel}
            width={isMobile ? 120 : 148}
          />
        )}

        {canEdit && (
          <div className="flex items-start gap-1">
            <MeasureForm
              unit={m.unit} pending={measuring} compact={isMobile}
              onSubmit={onMeasure}
            />
            <button
              onClick={onDelete}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="p-1.5 mt-1 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger shrink-0"
            >
              <Trash2 size={15} />
            </button>
          </div>
        )}
      </div>

      {/* What each state actually asks of the reader. */}
      {state === 'unrated' && (
        <div className="px-3 pb-3">
          <Callout
            variant="info" icon={<CircleAlert size={16} />}
            title={t('qual_unrated_title', { defaultValue: 'Rien à quoi comparer ce chiffre' })}
            action={canEdit && !expanded
              ? { label: t('qual_unrated_action', { defaultValue: 'Définir une tolérance' }), onClick: onToggle }
              : undefined}
            t={t}
          >
            {t('qual_unrated_body', {
              defaultValue: 'Cet indicateur n’a ni cible ni tolérance : la valeur est collectée, mais aucune exigence n’est tenue. Donnez-lui une borne pour qu’il puisse être jugé.',
            })}
          </Callout>
        </div>
      )}
      {state === 'drifting' && (
        <div className="px-3 pb-3">
          <Callout variant="warning" title={t('qual_drift_title', { defaultValue: 'Encore conforme, mais il s’en éloigne' })} t={t}>
            {t('qual_drift_body', {
              defaultValue: 'La dernière mesure est dans la tolérance, proche d’une borne, et s’en rapproche depuis la précédente. C’est maintenant qu’il faut agir, pas au dépassement.',
            })}
          </Callout>
        </div>
      )}

      {/* Detail — what makes the number reproducible, and where it comes from. */}
      {expanded && (
        <div className="border-t border-border bg-surface-1 p-4 space-y-4">

          {/* The method comes FIRST: a metric nobody can reproduce is a slogan. */}
          <div className="rounded-lg border border-border bg-surface-0 p-3">
            <div className="flex items-center gap-2 mb-2">
              <FlaskConical size={15} className="text-text-secondary shrink-0" />
              <span className="text-sm font-medium text-text-primary">
                {t('qual_method', { defaultValue: 'Méthode de mesure' })}
              </span>
            </div>
            <p className="text-xs text-text-tertiary mb-2">
              {t('qual_method_hint', {
                defaultValue: 'Comment ce chiffre est obtenu : l’outil, la requête, l’échantillon. Quiconque suit ces lignes doit retrouver la même valeur.',
              })}
            </p>
            <CommitTextarea
              value={m.method} disabled={!canEdit} rows={3}
              placeholder={t('qual_method_ph', { defaultValue: 'Ex. : temps de réponse P95 relevé sur les 7 derniers jours dans le tableau de bord de la passerelle.' })}
              onCommit={v => onPatch({ method: v })}
            />
            {!m.method.trim() && (
              <p className="text-xs text-warning mt-2 flex items-start gap-1">
                <TriangleAlert size={13} className="shrink-0 mt-0.5" />
                {t('qual_method_missing', {
                  defaultValue: 'Sans méthode, personne ne peut reproduire ce chiffre — ni le contester.',
                })}
              </p>
            )}
          </div>

          <div className={`grid gap-3 ${isMobile ? 'grid-cols-1' : 'grid-cols-2'}`}>
            <Field label={t('qual_code', { defaultValue: 'Code' })}>
              <CommitInput value={m.code} mono disabled={!canEdit} placeholder="Q-01"
                onCommit={v => onPatch({ code: v.trim() })} />
            </Field>
            <Field label={t('qual_unit', { defaultValue: 'Unité' })}>
              <CommitInput value={m.unit} disabled={!canEdit}
                placeholder={t('qual_unit_ph', { defaultValue: 'ms, %, défauts…' })}
                onCommit={v => onPatch({ unit: v.trim() })} />
            </Field>
          </div>

          <Field label={t('qual_description', { defaultValue: 'Description' })}>
            <CommitTextarea
              value={m.description} disabled={!canEdit} rows={2}
              placeholder={t('qual_description_ph', { defaultValue: 'Ce que l’indicateur cherche à garantir…' })}
              onCommit={v => onPatch({ description: v })}
            />
          </Field>

          <div className={`grid gap-3 ${isMobile ? 'grid-cols-1' : 'grid-cols-3'}`}>
            <Field label={t('qual_target', { defaultValue: 'Cible' })}>
              <CommitNumber value={m.target} disabled={!canEdit} placeholder="—"
                onCommit={v => onPatch({ target: v })} />
            </Field>
            <Field label={t('qual_tol_min', { defaultValue: 'Tolérance — borne basse' })}>
              <CommitNumber value={m.tolerance_min} disabled={!canEdit} placeholder="—"
                onCommit={v => onPatch({ tolerance_min: v })} />
            </Field>
            <Field label={t('qual_tol_max', { defaultValue: 'Tolérance — borne haute' })}>
              <CommitNumber value={m.tolerance_max} disabled={!canEdit} placeholder="—"
                onCommit={v => onPatch({ tolerance_max: v })} />
            </Field>
          </div>
          <p className="text-xs text-text-tertiary -mt-2">
            {t('qual_bounds_hint', {
              defaultValue: 'Une borne suffit : seule la basse donne un plancher, seule la haute un plafond, les deux forment une plage. Sans aucune des trois valeurs, l’indicateur reste non évaluable — ses mesures seront collectées, jamais jugées.',
            })}
          </p>

          <div className={`grid gap-3 ${isMobile ? 'grid-cols-1' : 'grid-cols-2'}`}>
            <Field label={t('qual_direction', { defaultValue: 'Sens' })}>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                disabled={!canEdit}
                value={m.direction}
                options={DIRECTIONS.map(d => ({ value: d, label: directionLabel(d, t) }))}
                onChange={v => onPatch({ direction: v as QualityDirection })}
              />
            </Field>
            <Field label={t('qual_frequency', { defaultValue: 'Fréquence' })}>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                disabled={!canEdit}
                value={m.frequency}
                options={FREQUENCIES.map(f => ({ value: f, label: frequencyLabel(f, t) }))}
                onChange={v => onPatch({ frequency: v as QualityFrequency })}
              />
            </Field>
            <Field label={t('qual_deliverable', { defaultValue: 'Livrable concerné' })}>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                disabled={!canEdit}
                value={m.deliverable_id ?? ''}
                options={deliverableOptions}
                onChange={v => onPatch({ deliverable_id: v || null })}
              />
            </Field>
            <Field label={t('qual_task', { defaultValue: 'Lot de travail' })}>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                disabled={!canEdit}
                value={m.task_id ?? ''}
                options={taskOptions}
                onChange={v => onPatch({ task_id: v || null })}
              />
            </Field>
          </div>

          <Toggle
            checked={m.is_active}
            disabled={!canEdit}
            label={t('qual_active', { defaultValue: 'Suivi actif' })}
            description={t('qual_active_hint', {
              defaultValue: 'Un indicateur suspendu reste consultable mais sort des compteurs de conformité.',
            })}
            onChange={e => onPatch({ is_active: e.target.checked })}
          />

          {/* History — the series the sparkline draws, readable and correctable. */}
          <div>
            <div className="text-sm font-medium text-text-primary mb-2">
              {t('qual_history', { defaultValue: 'Historique des mesures' })}
            </div>
            {line.series.length === 0 ? (
              <p className="text-sm text-text-tertiary">
                {t('qual_history_empty', { defaultValue: 'Aucune mesure relevée pour l’instant.' })}
              </p>
            ) : (
              <ul className="divide-y divide-border border border-border rounded-lg bg-surface-0">
                {[...line.series].reverse().map(r => (
                  <li key={r.id} className="flex items-center gap-3 px-3 py-2">
                    <span className="text-xs text-text-tertiary w-28 shrink-0">{fmtDay(r.measured_on)}</span>
                    <span className="text-sm text-text-primary tabular-nums shrink-0">
                      {withUnit(lang, r.value, m.unit)}
                    </span>
                    <span className="text-xs text-text-secondary min-w-0 flex-1 break-words">{r.notes}</span>
                    {canEdit && (
                      <button
                        onClick={() => onDeleteReading(r.id)}
                        title={t('qual_delete_reading', { defaultValue: 'Supprimer cette mesure' })}
                        className="p-1 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger shrink-0"
                      >
                        <Trash2 size={14} />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

// ── One check ────────────────────────────────────────────────────────────────

function CheckRow({ check, canEdit, isMobile, deliverableOptions, taskOptions, onPatch, onDelete, onSetResult }: {
  check: QualityCheck
  canEdit: boolean
  isMobile: boolean
  deliverableOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  onPatch: (patch: QualityCheckEdit) => void
  onDelete: () => void
  /** Goes through the parent: declaring a failure asks for the finding first. */
  onSetResult: (result: QualityResult) => void
}) {
  const { t, i18n } = useTranslation('office')
  const fmtDay = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })
  const target = check.deliverable_name ?? check.task_name

  return (
    <div className="border border-border rounded-lg bg-surface-0 p-3 space-y-2">
      <div className="flex items-start gap-2 flex-wrap">
        <div className="min-w-0 flex-1 basis-64">
          {canEdit ? (
            <CommitInput value={check.label} required className="w-full"
              onCommit={v => onPatch({ label: v.trim() })} />
          ) : (
            <span className="text-sm text-text-primary">{check.label}</span>
          )}
        </div>
        {canEdit ? (
          <Dropdown
            height={36} fontSize={14} focusable width={isMobile ? '100%' : 168}
            value={check.result}
            options={RESULTS.map(r => ({ value: r, label: resultLabel(r, t) }))}
            onChange={v => onSetResult(v as QualityResult)}
          />
        ) : (
          <Badge variant={RESULT_BADGE[check.result]} dot>{resultLabel(check.result, t)}</Badge>
        )}
        {canEdit && (
          <button
            onClick={onDelete}
            title={t('common_delete', { defaultValue: 'Supprimer' })}
            className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger shrink-0"
          >
            <Trash2 size={15} />
          </button>
        )}
      </div>

      <div className="flex items-center gap-2 flex-wrap text-xs text-text-tertiary">
        {/* A check always bears on something — the server refuses it otherwise. */}
        {canEdit ? (
          <>
            <Dropdown
              height={36} fontSize={14} focusable width={isMobile ? '100%' : 220}
              value={check.deliverable_id ?? ''}
              options={deliverableOptions}
              onChange={v => onPatch({ deliverable_id: v || null })}
            />
            <Dropdown
              height={36} fontSize={14} focusable width={isMobile ? '100%' : 220}
              value={check.task_id ?? ''}
              options={taskOptions}
              onChange={v => onPatch({ task_id: v || null })}
            />
          </>
        ) : (
          <span>{target ?? t('qual_check_unattached', { defaultValue: 'Rattaché à rien' })}</span>
        )}
        {check.checked_on && (
          <span>{t('qual_checked_on', { defaultValue: 'Contrôlé le {{d}}', d: fmtDay(check.checked_on) })}</span>
        )}
      </div>

      {!check.deliverable_id && !check.task_id && (
        <p className="text-xs text-danger flex items-start gap-1">
          <TriangleAlert size={13} className="shrink-0 mt-0.5" />
          {t('qual_check_orphan', {
            defaultValue: 'Ce contrôle ne porte sur rien : rattachez-le à un livrable ou à un lot de travail, sinon il ne prouve rien.',
          })}
        </p>
      )}

      {(check.result === 'fail' || check.result === 'waived' || check.evidence) && (
        <Field label={check.result === 'waived'
          ? t('qual_waiver_reason', { defaultValue: 'Motif de la dérogation' })
          : t('qual_evidence', { defaultValue: 'Constat' })}>
          <CommitTextarea
            value={check.evidence} disabled={!canEdit} rows={2}
            placeholder={t('qual_evidence_ph', { defaultValue: 'Ce que le contrôle a relevé…' })}
            onCommit={v => onPatch({ evidence: v })}
          />
        </Field>
      )}
    </div>
  )
}

// ── Cost of quality ──────────────────────────────────────────────────────────

function CoqLine({ category, amount, total, lang, t }: {
  category: CoqCategory
  amount: number
  /** Denominator of the share — conformance + failure, never including the unclassified. */
  total: number
  lang: string
  t: TFunction
}) {
  const share = total > 0 ? (amount / total) * 100 : 0
  return (
    <div className="flex items-baseline justify-between gap-3 py-1">
      <span className="text-sm text-text-secondary min-w-0 break-words">{coqLabel(category, t)}</span>
      <span className="text-sm text-text-primary tabular-nums shrink-0">
        {nf(lang, amount)}
        <span className="text-xs text-text-tertiary ml-1.5">{nf(lang, share, 0)} %</span>
      </span>
    </div>
  )
}

// ── The screen ───────────────────────────────────────────────────────────────

export default function QualityView({ projectId, canEdit = true, onOpenExpenses, onOpenIssues }: {
  projectId: string
  /** False in the mobile reading mode, where quality is read, not kept. */
  canEdit?: boolean
  /** Jumps to the expenses: the cost of quality is read off them, not typed twice. */
  onOpenExpenses?: () => void
  /** Jumps to the issue log — where a failed check belongs next. */
  onOpenIssues?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const lang = i18n.language
  const qc = useQueryClient()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set())
  const [metricOpen, setMetricOpen] = useState(false)
  const [checkOpen, setCheckOpen] = useState(false)
  const [metricDraft, setMetricDraft] = useState({ name: '', code: '', unit: '', tolerance_min: '', tolerance_max: '' })
  const [checkDraft, setCheckDraft] = useState({ label: '', deliverable_id: '', task_id: '' })

  const { data: overview, isLoading, isError, refetch } = useQuery({
    queryKey: ['quality', projectId],
    queryFn: () => projectsApi.getQuality(projectId),
  })
  const { data: checks } = useQuery({
    queryKey: ['quality-checks', projectId],
    queryFn: () => projectsApi.listQualityChecks(projectId),
  })
  // Both query keys are shared with the scope screens: one cache entry each.
  const { data: deliverables } = useQuery({
    queryKey: ['deliverables', projectId],
    queryFn: () => projectsApi.listDeliverables(projectId),
  })
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })

  const fail = (err: unknown) =>
    setErrorMsg(errText(err, t('common_error', { defaultValue: 'Une erreur est survenue.' })))
  const invalMetrics = () => qc.invalidateQueries({ queryKey: ['quality', projectId] })
  const invalChecks = () => {
    qc.invalidateQueries({ queryKey: ['quality-checks', projectId] })
    invalMetrics()
  }

  const createMetric = useMutation({
    mutationFn: (data: QualityMetricEdit & { name: string }) => projectsApi.createQualityMetric(projectId, data),
    onSuccess: () => {
      setErrorMsg(null); invalMetrics(); setMetricOpen(false)
      setMetricDraft({ name: '', code: '', unit: '', tolerance_min: '', tolerance_max: '' })
    },
    onError: fail,
  })
  const patchMetric = useMutation({
    mutationFn: ({ id, data }: { id: string; data: QualityMetricEdit }) =>
      projectsApi.updateQualityMetric(projectId, id, data),
    onSuccess: () => { setErrorMsg(null); invalMetrics() },
    onError: fail,
  })
  const removeMetric = useMutation({
    mutationFn: (id: string) => projectsApi.deleteQualityMetric(projectId, id),
    onSuccess: () => { setErrorMsg(null); invalMetrics() },
    onError: fail,
  })
  const addMeasurement = useMutation({
    mutationFn: ({ id, value, measured_on, notes }: { id: string; value: number; measured_on: string; notes: string }) =>
      projectsApi.addQualityMeasurement(projectId, id, { value, measured_on, notes: notes || undefined }),
    onSuccess: () => { setErrorMsg(null); invalMetrics() },
    onError: fail,
  })
  const removeMeasurement = useMutation({
    mutationFn: ({ id, readingId }: { id: string; readingId: string }) =>
      projectsApi.deleteQualityMeasurement(projectId, id, readingId),
    onSuccess: () => { setErrorMsg(null); invalMetrics() },
    onError: fail,
  })
  const createCheck = useMutation({
    mutationFn: (data: QualityCheckEdit & { label: string }) => projectsApi.createQualityCheck(projectId, data),
    onSuccess: () => {
      setErrorMsg(null); invalChecks(); setCheckOpen(false)
      setCheckDraft({ label: '', deliverable_id: '', task_id: '' })
    },
    onError: fail,
  })
  const patchCheck = useMutation({
    mutationFn: ({ id, data }: { id: string; data: QualityCheckEdit }) =>
      projectsApi.updateQualityCheck(projectId, id, data),
    onSuccess: () => { setErrorMsg(null); invalChecks() },
    onError: fail,
  })
  const removeCheck = useMutation({
    mutationFn: (id: string) => projectsApi.deleteQualityCheck(projectId, id),
    onSuccess: () => { setErrorMsg(null); invalChecks() },
    onError: fail,
  })

  const metrics = useMemo(() => overview?.metrics ?? [], [overview])

  /**
   * The server's own tally, over the ACTIVE metrics. Two things about it:
   *
   *  • `conforming` INCLUDES the drifting ones, so the stable count is the
   *    difference. Both tiles are shown: "conforming" alone would bury the only
   *    metrics worth looking at, and drifting alone would read as a breach.
   *  • `unmeasured` (never read) and `unrated` (read, but nothing to compare it
   *    to) are separate figures, because the fix differs: take a measurement, or
   *    set a bound.
   *
   * The five tiles therefore add up to `summary.active`.
   */
  const summary = overview?.summary
  const counts = useMemo(() => ({
    conform:    Math.max(0, (summary?.conforming ?? 0) - (summary?.drifting ?? 0)),
    drifting:   summary?.drifting ?? 0,
    breach:     summary?.breaching ?? 0,
    unrated:    summary?.unrated ?? 0,
    unmeasured: summary?.unmeasured ?? 0,
  }), [summary])

  const noneOption = (label: string): DropdownOption => ({ value: '', label })
  const deliverableOptions: DropdownOption[] = useMemo(() => [
    noneOption(t('qual_no_deliverable', { defaultValue: '— Aucun livrable —' })),
    ...(deliverables ?? []).map(d => ({ value: d.id, label: `${d.code ? d.code + ' ' : ''}${d.name}`.trim() })),
  ], [deliverables, t])
  const taskOptions: DropdownOption[] = useMemo(() => [
    noneOption(t('qual_no_task', { defaultValue: '— Aucun lot —' })),
    ...(wbs ?? []).map(e => ({ value: e.id, label: `${e.wbs} ${e.name}`.trim() })),
  ], [wbs, t])
  const canAttach = (deliverables?.length ?? 0) > 0 || (wbs?.length ?? 0) > 0

  const toggle = (id: string) => setExpanded(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  const askDeleteMetric = async (m: QualityMetric) => {
    const ok = await confirm({
      title: t('qual_delete_metric_title', { defaultValue: 'Supprimer l’indicateur ?' }),
      message: t('qual_delete_metric_msg', {
        defaultValue: '« {{name}} » et toutes ses mesures seront supprimés définitivement.',
        name: m.name,
      }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) removeMetric.mutate(m.id)
  }

  const askDeleteCheck = async (c: QualityCheck) => {
    const ok = await confirm({
      title: t('qual_delete_check_title', { defaultValue: 'Supprimer le contrôle ?' }),
      message: t('qual_delete_check_msg', { defaultValue: '« {{label}} » sera supprimé définitivement.', label: c.label }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) removeCheck.mutate(c.id)
  }

  /**
   * Declaring a failure asks for the finding FIRST, then sends one request.
   * The server rejects a `fail` with no evidence ("Indiquez ce que le contrôle a
   * relevé…"), so firing the PATCH blind would only produce a 422 to translate
   * back into the very question asked here.
   */
  const setResult = async (c: QualityCheck, result: QualityResult) => {
    if (result === c.result) return
    if (result === 'fail' && !c.evidence.trim()) {
      const evidence = await prompt({
        title: t('qual_fail_title', { defaultValue: 'Déclarer le contrôle en échec' }),
        message: t('qual_fail_msg', {
          defaultValue: 'Qu’a relevé le contrôle sur « {{label}} » ? Un échec sans constat est une affirmation, pas une preuve.',
          label: c.label,
        }),
        placeholder: t('qual_evidence_ph', { defaultValue: 'Ce que le contrôle a relevé…' }),
        confirmLabel: t('qual_fail_confirm', { defaultValue: 'Déclarer l’échec' }),
        multiline: true,
      })
      if (evidence === null || !evidence.trim()) return
      patchCheck.mutate({ id: c.id, data: { result, evidence: evidence.trim() } })
      return
    }
    if (result === 'waived' && !c.evidence.trim()) {
      const reason = await prompt({
        title: t('qual_waive_title', { defaultValue: 'Accorder une dérogation' }),
        message: t('qual_waive_msg', {
          defaultValue: 'Pourquoi « {{label}} » est-il levé malgré tout ? Une dérogation non motivée devient une règle non écrite.',
          label: c.label,
        }),
        placeholder: t('qual_waive_ph', { defaultValue: 'Motif de la dérogation…' }),
        confirmLabel: t('qual_waive_confirm', { defaultValue: 'Accorder' }),
        multiline: true,
      })
      if (reason === null || !reason.trim()) return
      patchCheck.mutate({ id: c.id, data: { result, evidence: reason.trim() } })
      return
    }
    patchCheck.mutate({ id: c.id, data: { result } })
  }

  const coq = overview?.cost_of_quality
  const conformance = coq?.conformance ?? 0
  const failure = coq?.failure ?? 0
  const unclassified = coq?.unclassified ?? 0
  const coqTotal = conformance + failure
  const checkCounts = overview?.checks

  const canCreateMetric = metricDraft.name.trim().length > 0
  // The server refuses a check attached to nothing, so the button waits for one.
  const canCreateCheck = checkDraft.label.trim().length > 0
    && (checkDraft.deliverable_id !== '' || checkDraft.task_id !== '')

  const submitMetric = () => {
    if (!canCreateMetric) return
    createMetric.mutate({
      name: metricDraft.name.trim(),
      code: metricDraft.code.trim(),
      unit: metricDraft.unit.trim(),
      tolerance_min: parseNumber(metricDraft.tolerance_min),
      tolerance_max: parseNumber(metricDraft.tolerance_max),
    })
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Gauge size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('qual_title', { defaultValue: 'Qualité' })}
          </h1>
          <div className="flex-1" />
          {canEdit && (
            <>
              <Button size="sm" variant="secondary" icon={<ClipboardCheck size={14} />}
                disabled={!canAttach}
                title={canAttach ? undefined : t('qual_check_needs_target', {
                  defaultValue: 'Créez d’abord un livrable ou un lot de travail : un contrôle porte toujours sur quelque chose.',
                })}
                onClick={() => setCheckOpen(true)}>
                {t('qual_new_check', { defaultValue: 'Nouveau contrôle' })}
              </Button>
              <Button size="sm" icon={<Plus size={14} />} onClick={() => setMetricOpen(true)}>
                {t('qual_new_metric', { defaultValue: 'Nouvel indicateur' })}
              </Button>
            </>
          )}
        </div>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {/* ── The state of the metrics, in one line ─────────────────────────── */}
        {metrics.length > 0 && (
          <div className={`grid gap-3 ${isMobile ? 'grid-cols-2' : 'grid-cols-5'}`}>
            {([
              ['conform', counts.conform, 'text-success'],
              ['drifting', counts.drifting, 'text-warning'],
              ['breach', counts.breach, 'text-danger'],
              ['unrated', counts.unrated, 'text-text-tertiary'],
              ['unmeasured', counts.unmeasured, 'text-text-tertiary'],
            ] as const).map(([key, value, tone]) => (
              <div key={key} className="bg-surface-0 border border-border rounded-xl p-3">
                <div className={`text-2xl font-medium tabular-nums ${tone}`}>{value}</div>
                <div className="text-xs text-text-secondary mt-0.5">{stateLabel(key, t)}</div>
              </div>
            ))}
          </div>
        )}

        {counts.drifting > 0 && (
          <Callout
            variant="warning"
            title={t('qual_drift_banner_title', { defaultValue: 'Tout va bien, et tout va mal finir' })}
            t={t}
          >
            {t('qual_drift_banner', {
              defaultValue: '{{n}} indicateur(s) restent dans leur tolérance mais s’approchent d’une borne. Ce sont ceux à regarder aujourd’hui : quand ils basculeront, il sera trop tard pour agir en amont.',
              n: counts.drifting,
            })}
          </Callout>
        )}
        {counts.unrated > 0 && (
          <Callout
            variant="info"
            title={t('qual_unrated_banner_title', { defaultValue: 'Des chiffres collectés, pas des exigences tenues' })}
            t={t}
          >
            {t('qual_unrated_banner', {
              defaultValue: '{{n}} indicateur(s) n’ont ni cible ni tolérance : rien ne permet de dire s’ils sont conformes. Ils sont comptés à part plutôt que présentés comme au vert.',
              n: counts.unrated,
            })}
          </Callout>
        )}

        {/* ── Metrics ──────────────────────────────────────────────────────── */}
        <Card
          title={t('qual_metrics', { defaultValue: 'Indicateurs' })}
          icon={<Gauge size={16} />}
          subtitle={t('qual_metrics_sub', {
            defaultValue: 'Ce que « assez bon » veut dire en chiffres, et la marge qu’il reste avant de ne plus l’être.',
          })}
        >
          {isLoading ? (
            <p className="text-sm text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</p>
          ) : isError ? (
            <EmptyState
              variant="error"
              icon={<Gauge size={26} />}
              title={t('qual_load_error', { defaultValue: 'La qualité n’a pas pu être chargée.' })}
              action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
              compact
              t={t}
            />
          ) : metrics.length === 0 ? (
            <EmptyState
              variant="first-use"
              icon={<Gauge size={26} />}
              title={t('qual_metrics_empty_title', { defaultValue: 'Aucun indicateur' })}
              description={t('qual_metrics_empty', {
                defaultValue: 'Un indicateur transforme une intention — « le site doit être rapide » — en un nombre, une borne et une méthode pour l’obtenir. Sans borne, il n’y a rien à tenir.',
              })}
              action={canEdit ? {
                label: t('qual_new_metric', { defaultValue: 'Nouvel indicateur' }),
                icon: <Plus size={14} />,
                onClick: () => setMetricOpen(true),
              } : undefined}
              compact
              t={t}
            />
          ) : (
            <div className="space-y-3">
              {metrics.map(line => (
                <MetricCard
                  key={line.metric.id}
                  line={line}
                  canEdit={canEdit}
                  isMobile={isMobile}
                  expanded={expanded.has(line.metric.id)}
                  onToggle={() => toggle(line.metric.id)}
                  deliverableOptions={deliverableOptions}
                  taskOptions={taskOptions}
                  measuring={addMeasurement.isPending}
                  onPatch={data => patchMetric.mutate({ id: line.metric.id, data })}
                  onDelete={() => { void askDeleteMetric(line.metric) }}
                  onMeasure={(value, measured_on, notes) =>
                    addMeasurement.mutate({ id: line.metric.id, value, measured_on, notes })}
                  onDeleteReading={readingId =>
                    removeMeasurement.mutate({ id: line.metric.id, readingId })}
                />
              ))}
            </div>
          )}
        </Card>

        {/* ── Checks ───────────────────────────────────────────────────────── */}
        <Card
          title={t('qual_checks', { defaultValue: 'Contrôles' })}
          icon={<ClipboardCheck size={16} />}
          subtitle={t('qual_checks_sub', {
            defaultValue: 'La preuve qu’un livrable a été vérifié — et, quand il ne passe pas, ce qui a été relevé.',
          })}
          actions={checkCounts ? (
            <div className="flex items-center gap-1.5 flex-wrap">
              {RESULTS.map(r => checkCounts[r] > 0 && (
                <Badge key={r} variant={RESULT_BADGE[r]}>{checkCounts[r]} {resultLabel(r, t)}</Badge>
              ))}
            </div>
          ) : undefined}
        >
          {(checks?.length ?? 0) === 0 ? (
            <EmptyState
              variant="first-use"
              icon={<ClipboardCheck size={26} />}
              title={t('qual_checks_empty_title', { defaultValue: 'Aucun contrôle' })}
              description={t('qual_checks_empty', {
                defaultValue: 'Un contrôle porte toujours sur un livrable ou un lot de travail : c’est ce qui le rend vérifiable. Les échecs remontent en tête de liste.',
              })}
              action={canEdit && canAttach ? {
                label: t('qual_new_check', { defaultValue: 'Nouveau contrôle' }),
                icon: <Plus size={14} />,
                onClick: () => setCheckOpen(true),
              } : undefined}
              compact
              t={t}
            />
          ) : (
            <div className="space-y-2">
              {(checkCounts?.fail ?? 0) > 0 && onOpenIssues && (
                <Callout
                  variant="danger"
                  title={t('qual_fail_banner_title', { defaultValue: 'Des contrôles en échec' })}
                  action={{
                    label: t('qual_open_issues', { defaultValue: 'Ouvrir le journal des incidents' }),
                    onClick: onOpenIssues,
                  }}
                  t={t}
                >
                  {t('qual_fail_banner', {
                    defaultValue: 'Un échec constaté ne se referme pas tout seul : il devient un incident que quelqu’un porte, ou il sera oublié.',
                  })}
                </Callout>
              )}
              {(checks ?? []).map(c => (
                <CheckRow
                  key={c.id}
                  check={c}
                  canEdit={canEdit}
                  isMobile={isMobile}
                  deliverableOptions={deliverableOptions}
                  taskOptions={taskOptions}
                  onPatch={data => patchCheck.mutate({ id: c.id, data })}
                  onDelete={() => { void askDeleteCheck(c) }}
                  onSetResult={r => { void setResult(c, r) }}
                />
              ))}
            </div>
          )}
        </Card>

        {/* ── Cost of quality ──────────────────────────────────────────────── */}
        <Card
          title={t('qual_coq', { defaultValue: 'Coût de la qualité' })}
          icon={<Coins size={16} />}
          subtitle={t('qual_coq_sub', {
            defaultValue: 'Ce que le projet dépense pour éviter la défaillance, face à ce qu’il paie parce qu’elle est survenue. Lu sur les dépenses déjà saisies, jamais ressaisi.',
          })}
          actions={onOpenExpenses ? (
            <Button size="sm" variant="text" icon={<ArrowRight size={14} />} onClick={onOpenExpenses}>
              {t('qual_open_expenses', { defaultValue: 'Voir les dépenses' })}
            </Button>
          ) : undefined}
        >
          {coqTotal === 0 && unclassified === 0 ? (
            <EmptyState
              variant="first-use"
              icon={<Coins size={26} />}
              title={t('qual_coq_empty_title', { defaultValue: 'Aucune dépense classée' })}
              description={t('qual_coq_empty', {
                defaultValue: 'Le coût de la qualité se lit sur les dépenses du projet : chacune peut être marquée « prévention », « évaluation », « défaillance interne » ou « défaillance externe ». Tant qu’aucune ne l’est, il n’y a rien à opposer.',
              })}
              action={onOpenExpenses ? {
                label: t('qual_open_expenses', { defaultValue: 'Voir les dépenses' }),
                onClick: onOpenExpenses,
              } : undefined}
              compact
              t={t}
            />
          ) : (
            <div className="space-y-4">
              {/* The two totals, face to face. This is the whole argument. */}
              <div className={`grid gap-3 ${isMobile ? 'grid-cols-1' : 'grid-cols-2'}`}>
                <div className="rounded-lg border border-border bg-surface-0 p-4">
                  <div className="flex items-center gap-2 text-sm text-text-secondary">
                    <ShieldCheck size={15} className="text-success shrink-0" />
                    {t('qual_coq_conformance', { defaultValue: 'Dépensé pour éviter la défaillance' })}
                  </div>
                  <div className="text-2xl font-medium text-text-primary tabular-nums mt-1">
                    {nf(lang, conformance)}
                  </div>
                  <div className="mt-2">
                    <CoqLine category="prevention" amount={coq?.by_category.prevention ?? 0} total={coqTotal} lang={lang} t={t} />
                    <CoqLine category="appraisal" amount={coq?.by_category.appraisal ?? 0} total={coqTotal} lang={lang} t={t} />
                  </div>
                </div>
                <div className="rounded-lg border border-border bg-surface-0 p-4">
                  <div className="flex items-center gap-2 text-sm text-text-secondary">
                    <TriangleAlert size={15} className="text-danger shrink-0" />
                    {t('qual_coq_failure', { defaultValue: 'Payé parce qu’elle est survenue' })}
                  </div>
                  <div className="text-2xl font-medium text-text-primary tabular-nums mt-1">
                    {nf(lang, failure)}
                  </div>
                  <div className="mt-2">
                    <CoqLine category="internal_failure" amount={coq?.by_category.internal_failure ?? 0} total={coqTotal} lang={lang} t={t} />
                    <CoqLine category="external_failure" amount={coq?.by_category.external_failure ?? 0} total={coqTotal} lang={lang} t={t} />
                  </div>
                </div>
              </div>

              {coqTotal > 0 && (
                <div>
                  <ProgressBar
                    value={conformance} max={coqTotal} variant="success" showValue t={t}
                    label={t('qual_coq_ratio', { defaultValue: 'Part consacrée à éviter la défaillance' })}
                    formatValue={(v, max) => `${nf(lang, (v / max) * 100, 0)} %`}
                  />
                  <p className="text-xs text-text-tertiary mt-2">
                    {failure > conformance
                      ? t('qual_coq_ratio_bad', {
                          defaultValue: 'Le projet paie plus la défaillance qu’il ne dépense à l’éviter. Chaque euro de prévention est censé en économiser plusieurs de reprise : ici, l’arbitrage penche du mauvais côté.',
                        })
                      : t('qual_coq_ratio_good', {
                          defaultValue: 'Le projet dépense davantage à éviter la défaillance qu’il n’en paie les conséquences — c’est le sens attendu, à condition que la part de défaillance ne remonte pas.',
                        })}
                  </p>
                </div>
              )}

              {/* Kept in plain sight: folding it in would make the split look precise. */}
              {unclassified > 0 && (
                <Callout
                  variant="info"
                  title={t('qual_coq_unclassified_title', { defaultValue: 'Dépenses non classées' })}
                  action={onOpenExpenses
                    ? { label: t('qual_open_expenses', { defaultValue: 'Voir les dépenses' }), onClick: onOpenExpenses }
                    : undefined}
                  t={t}
                >
                  {t('qual_coq_unclassified', {
                    defaultValue: '{{amount}} n’ont été rattachés à aucune des quatre catégories. Ils ne sont pas répartis d’office : la comparaison ci-dessus porte donc sur {{total}}, pas sur la totalité des dépenses.',
                    amount: nf(lang, unclassified),
                    total: nf(lang, coqTotal),
                  })}
                </Callout>
              )}
            </div>
          )}
        </Card>
      </div>

      {/* ── Creating a metric ──────────────────────────────────────────────── */}
      {metricOpen && (
        <FloatingWindow
          title={t('qual_new_metric', { defaultValue: 'Nouvel indicateur' })}
          icon={<Gauge size={16} />}
          onClose={() => setMetricOpen(false)}
          defaultWidth={520} defaultHeight={460} padding={16} t={t}
          actions={{
            confirm: {
              label: t('common_create', { defaultValue: 'Créer' }),
              disabled: !canCreateMetric,
              loading: createMetric.isPending,
              autoFocus: true,
              onClick: submitMetric,
            },
          }}
        >
          <div className="space-y-3">
            <Field label={t('qual_name', { defaultValue: 'Indicateur' })}>
              <Input
                autoFocus
                value={metricDraft.name}
                placeholder={t('qual_name_ph', { defaultValue: 'Ex. : temps de réponse de la page d’accueil' })}
                onChange={e => setMetricDraft(d => ({ ...d, name: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submitMetric() } }}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('qual_code', { defaultValue: 'Code' })}>
                <Input
                  className="font-mono"
                  value={metricDraft.code}
                  placeholder={t('qual_code_ph', { defaultValue: 'Q-01 (auto si vide)' })}
                  onChange={e => setMetricDraft(d => ({ ...d, code: e.target.value }))}
                />
              </Field>
              <Field label={t('qual_unit', { defaultValue: 'Unité' })}>
                <Input
                  value={metricDraft.unit}
                  placeholder={t('qual_unit_ph', { defaultValue: 'ms, %, défauts…' })}
                  onChange={e => setMetricDraft(d => ({ ...d, unit: e.target.value }))}
                />
              </Field>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('qual_tol_min', { defaultValue: 'Tolérance — borne basse' })}>
                <Input
                  type="text" inputMode="decimal"
                  value={metricDraft.tolerance_min}
                  placeholder="—"
                  onChange={e => setMetricDraft(d => ({ ...d, tolerance_min: e.target.value }))}
                />
              </Field>
              <Field label={t('qual_tol_max', { defaultValue: 'Tolérance — borne haute' })}>
                <Input
                  type="text" inputMode="decimal"
                  value={metricDraft.tolerance_max}
                  placeholder="—"
                  onChange={e => setMetricDraft(d => ({ ...d, tolerance_max: e.target.value }))}
                />
              </Field>
            </div>
            <Callout variant="info" t={t}>
              {t('qual_new_metric_hint', {
                defaultValue: 'Sans au moins une borne, l’indicateur restera « non évaluable » : ses mesures seront collectées, mais jamais jugées. La méthode de mesure se renseigne ensuite, dans le détail de l’indicateur.',
              })}
            </Callout>
          </div>
        </FloatingWindow>
      )}

      {/* ── Creating a check ───────────────────────────────────────────────── */}
      {checkOpen && (
        <FloatingWindow
          title={t('qual_new_check', { defaultValue: 'Nouveau contrôle' })}
          icon={<ClipboardCheck size={16} />}
          onClose={() => setCheckOpen(false)}
          defaultWidth={520} defaultHeight={420} padding={16} t={t}
          actions={{
            confirm: {
              label: t('common_create', { defaultValue: 'Créer' }),
              disabled: !canCreateCheck,
              loading: createCheck.isPending,
              autoFocus: true,
              onClick: () => createCheck.mutate({
                label: checkDraft.label.trim(),
                deliverable_id: checkDraft.deliverable_id || null,
                task_id: checkDraft.task_id || null,
              }),
            },
          }}
        >
          <div className="space-y-3">
            <Field label={t('qual_check_label', { defaultValue: 'Contrôle' })}>
              <Input
                autoFocus
                value={checkDraft.label}
                placeholder={t('qual_check_label_ph', { defaultValue: 'Ce qui sera vérifié…' })}
                onChange={e => setCheckDraft(d => ({ ...d, label: e.target.value }))}
              />
            </Field>
            <Field label={t('qual_deliverable', { defaultValue: 'Livrable concerné' })}>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={checkDraft.deliverable_id}
                options={deliverableOptions}
                onChange={v => setCheckDraft(d => ({ ...d, deliverable_id: v }))}
              />
            </Field>
            <Field label={t('qual_task', { defaultValue: 'Lot de travail' })}>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={checkDraft.task_id}
                options={taskOptions}
                onChange={v => setCheckDraft(d => ({ ...d, task_id: v }))}
              />
            </Field>
            <Callout variant={canCreateCheck ? 'info' : 'warning'} t={t}>
              {t('qual_check_attach_hint', {
                defaultValue: 'Rattachez le contrôle à un livrable ou à un lot de travail — l’un des deux suffit. Un contrôle qui ne porte sur rien ne prouve rien, et le serveur le refuse.',
              })}
            </Callout>
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
