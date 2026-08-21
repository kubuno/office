import { useEffect, useMemo, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { addDays, format, parseISO } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { AlertTriangle, CalendarCheck, Coins, Gauge, LineChart, Receipt, Settings2, Target } from 'lucide-react'
import {
  Badge, Button, Callout, Card, DataTable, Dropdown, EmptyState, Input, Separator, Spinner, Tooltip,
  useIsMobile, type DataTableColumn, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type CostConfig, type CostCurvePoint, type CostOverview, type CostTaskLine, type EacMethod,
} from '../api'

// Earned value — the one screen that answers "are we ahead or behind, over or
// under budget, and where does this land?" with three numbers that are actually
// comparable, because all three are money:
//
//   PV  what should have been done by the status date, valued at its budget
//   EV  what HAS been done, valued at ITS BUDGET — never at what it cost
//   AC  what it actually cost
//
// Everything else is a subtraction or a ratio of those three. The screen is
// therefore built around two obligations the raw numbers do not carry:
//
//  • an index whose divisor is zero is NOT 1.0 and NOT 0 — it is unmeasurable,
//    and it says so, because "1.0" reads as "on plan";
//  • an index computed over a plan that is half un-budgeted, or over a project
//    where nobody logs hours, is flattering and wrong. That warning sits at the
//    TOP, not in a footnote: a project with no logged hours shows a magnificent
//    CPI, and someone will believe it.

// ── Series colours ────────────────────────────────────────────────────────────
// The theme ships a categorical chart ramp (`--kb-chart-*`, light and dark sets)
// precisely so charts stop inventing hex codes. Two rules decide what goes where:
//
//  • PV is the PLAN — a reference line, not a competitor. It is drawn neutral,
//    thinner and dashed, so the eye reads it as the ruler the other two are
//    measured against. It deliberately sits outside the categorical ramp.
//  • EV and AC take the ramp's first two slots (blue, orange). Status colours
//    (--color-danger / --color-success) are NOT used: they mean "good"/"bad", and
//    an actual-cost line painted red would pass judgement before the reader has
//    compared anything.
//
// The pair was checked for colour-vision separation on both surfaces (worst
// adjacent ΔE 24.7 light / 26.8 dark, well above the 8 floor). Fallbacks are
// spelled out because a host older than the ramp would otherwise paint nothing.
const SERIES = {
  light: { ev: 'var(--kb-chart-1, #2a78d6)', ac: 'var(--kb-chart-2, #eb6834)' },
  dark:  { ev: 'var(--kb-chart-1-dark, #3987e5)', ac: 'var(--kb-chart-2-dark, #d95926)' },
}
/** The plan line and the chart furniture: text tokens, which every theme remaps. */
const NEUTRAL = 'var(--color-text-tertiary, #80868b)'
const GRID    = 'var(--color-border, #e0e0e0)'

/**
 * Light or dark, read from the root element's colour scheme.
 *
 * Kubuno themes do NOT follow `prefers-color-scheme`: the theme engine writes
 * the variables and `color-scheme` as inline style from JS. A CSS-only media
 * query would therefore keep the light series on a hand-picked dark theme.
 */
function useDarkScheme(): boolean {
  const read = () => typeof document !== 'undefined'
    && getComputedStyle(document.documentElement).colorScheme.includes('dark')
  const [dark, setDark] = useState(read)
  useEffect(() => {
    const sync = () => setDark(read())
    const observer = new MutationObserver(sync)
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['style', 'class', 'data-theme'] })
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    mq.addEventListener('change', sync)
    return () => { observer.disconnect(); mq.removeEventListener('change', sync) }
  }, [])
  return dark
}

// ── Formatting ────────────────────────────────────────────────────────────────

function useMoney(currency: string, language: string) {
  return useMemo(() => {
    // The currency is free-form server-side; an unknown code makes Intl throw,
    // and a dashboard must not blank out because someone typed "€".
    let fmt: Intl.NumberFormat
    try {
      fmt = new Intl.NumberFormat(language, { style: 'currency', currency, maximumFractionDigits: 0 })
    } catch {
      fmt = new Intl.NumberFormat(language, { maximumFractionDigits: 0 })
    }
    const compact = new Intl.NumberFormat(language, { notation: 'compact', maximumFractionDigits: 1 })
    return {
      full:    (v: number) => fmt.format(v),
      signed:  (v: number) => `${v > 0 ? '+' : ''}${fmt.format(v)}`,
      compact: (v: number) => compact.format(v),
    }
  }, [currency, language])
}

const fmtIndex = (v: number, language: string) =>
  new Intl.NumberFormat(language, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v)

/** A ratio is "on plan" inside a 5 % band — 0.99 is noise, not a warning. */
const BAND = 0.05

// ── Small building blocks ─────────────────────────────────────────────────────

type Tone = 'good' | 'bad' | 'flat'
const TONE_CLASS: Record<Tone, string> = {
  good: 'text-success',
  bad:  'text-danger',
  flat: 'text-text-primary',
}
const TONE_VARIANT: Record<Tone, 'success' | 'danger' | 'neutral'> = {
  good: 'success', bad: 'danger', flat: 'neutral',
}

/** A money variance: negative is the bad direction (overspent, behind). */
const varianceTone = (v: number, scale: number): Tone => {
  const eps = Math.max(scale * 0.005, 0.5)
  if (v < -eps) return 'bad'
  if (v > eps) return 'good'
  return 'flat'
}
/** CPI and SPI: above 1 is good. */
const indexTone = (v: number): Tone => (v > 1 + BAND ? 'good' : v < 1 - BAND ? 'bad' : 'flat')
/** TCPI is the exception: it is the effort STILL REQUIRED, so above 1 is bad. */
const tcpiTone = (v: number): Tone => (v > 1 + BAND ? 'bad' : v < 1 - BAND ? 'good' : 'flat')

function Figure({ label, value, hint, tone = 'flat', mono = true }: {
  label: string
  value: string
  hint?: string
  tone?: Tone
  mono?: boolean
}) {
  return (
    <div className="min-w-0">
      <div className="text-xs text-text-secondary truncate">{label}</div>
      <div className={`text-lg font-semibold ${mono ? 'tabular-nums' : ''} ${TONE_CLASS[tone]}`}>{value}</div>
      {hint && <div className="text-[11px] text-text-tertiary leading-snug mt-0.5">{hint}</div>}
    </div>
  )
}

/**
 * An index, or an honest refusal to state one.
 *
 * CPI is EV/AC and SPI is EV/PV: when the divisor is zero the ratio does not
 * exist. Printing 1.00 would read "exactly on plan" and printing 0 would read
 * "catastrophe" — both are inventions. The cell says it cannot be measured and
 * the tooltip says which divisor is missing.
 */
function IndexFigure({ label, value, unmeasurableReason, tone, language }: {
  label: string
  value: number | null
  unmeasurableReason: string
  tone: (v: number) => Tone
  language: string
}) {
  const { t } = useTranslation('office')
  if (value === null) {
    return (
      <div className="min-w-0">
        <div className="text-xs text-text-secondary truncate">{label}</div>
        <Tooltip label={unmeasurableReason}>
          <span className="inline-flex mt-0.5">
            <Badge variant="neutral">{t('proj_ev_unmeasurable', { defaultValue: 'Non mesurable' })}</Badge>
          </span>
        </Tooltip>
      </div>
    )
  }
  return <Figure label={label} value={fmtIndex(value, language)} tone={tone(value)} />
}

/** A number field that commits on blur — one PATCH per edit, not per keystroke. */
function NumField({ value, onCommit, disabled, placeholder, step }: {
  value: number | null
  onCommit: (next: number | null) => void
  disabled?: boolean
  placeholder?: string
  step?: string
}) {
  const [draft, setDraft] = useState(value === null ? '' : String(value))
  useEffect(() => { setDraft(value === null ? '' : String(value)) }, [value])
  return (
    <Input
      type="number" step={step ?? '0.01'} min="0"
      value={draft}
      disabled={disabled}
      placeholder={placeholder}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        const next = draft.trim() === '' ? null : Number(draft)
        if (next !== null && !Number.isFinite(next)) { setDraft(value === null ? '' : String(value)); return }
        if (next === value) return
        onCommit(next)
      }}
    />
  )
}

// ── The S-curve ───────────────────────────────────────────────────────────────

/** Rounds an axis maximum up to a readable step (1, 2, 2.5, 5 × 10ⁿ). */
function niceCeil(v: number): number {
  if (!(v > 0)) return 1
  const exp = Math.floor(Math.log10(v))
  const base = 10 ** exp
  const n = v / base
  const step = n <= 1 ? 1 : n <= 2 ? 2 : n <= 2.5 ? 2.5 : n <= 5 ? 5 : 10
  return step * base
}

interface CurveProps {
  curve:        CostCurvePoint[]
  /** The status date as an offset. The server samples the curve exactly there,
   *  so AC lands ON the marker rather than up to a week short of it. */
  statusOffset: number
  /** The day the offsets count from — served with the measurement. */
  origin:       Date
  language:     string
  money:        ReturnType<typeof useMoney>
  currency:     string
  compact:      boolean
}

function SCurve({ curve, statusOffset, origin, language, money, currency, compact }: CurveProps) {
  const { t } = useTranslation('office')
  const dark = useDarkScheme()
  const colors = dark ? SERIES.dark : SERIES.light
  const wrapRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(720)
  const [hover, setHover] = useState<number | null>(null)

  useEffect(() => {
    const el = wrapRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(entries => {
      const w = entries[0]?.contentRect.width
      if (w && w > 0) setWidth(w)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  const height = compact ? 220 : 300
  // Room on the right for the end-of-line labels, and only when there is room:
  // squeezing them onto a narrow screen would overlap the last data point.
  const roomForLabels = width >= 560
  const padL = compact ? 46 : 66
  const padR = roomForLabels ? 74 : 14
  const padT = 12
  const padB = 30
  const iw = Math.max(10, width - padL - padR)
  const ih = Math.max(10, height - padT - padB)

  const maxOffset = curve.length ? curve[curve.length - 1].offset : 0
  const rawMax = curve.reduce((m, p) => Math.max(m, p.pv, p.ev ?? 0, p.ac ?? 0), 0)
  const yMax = niceCeil(rawMax)

  const X = (o: number) => padL + (maxOffset > 0 ? (o / maxOffset) * iw : iw / 2)
  const Y = (v: number) => padT + ih - (v / yMax) * ih

  const dateOf = (offset: number) =>
    format(addDays(origin, offset), 'd MMM yyyy', { locale: getDateLocale(language) })

  /**
   * One path per series. A `null` BREAKS the path instead of bridging it: AC is
   * unknown past the status date, so the line simply ends there. It is never
   * carried forward, interpolated, or floored to zero — all three would draw
   * spending that nobody recorded.
   *
   * A lone known value therefore yields a bare `M`, which SVG draws as nothing.
   * That is exactly the EV case and it is handled by an explicit marker below,
   * not by relaxing this rule.
   */
  const pathOf = (get: (p: CostCurvePoint) => number | null) => {
    let out = ''
    let open = false
    for (const p of curve) {
      const v = get(p)
      if (v === null) { open = false; continue }
      out += `${open ? 'L' : 'M'}${X(p.offset).toFixed(1)} ${Y(v).toFixed(1)} `
      open = true
    }
    return out.trim()
  }

  const lastKnown = (get: (p: CostCurvePoint) => number | null) => {
    for (let i = curve.length - 1; i >= 0; i--) {
      const v = get(curve[i])
      if (v !== null) return { offset: curve[i].offset, value: v }
    }
    return null
  }

  const pvEnd = lastKnown(p => p.pv)
  const evEnd = lastKnown(p => p.ev)
  const acEnd = lastKnown(p => p.ac)

  // Earned value is a MEASUREMENT, not a history: progress is one number per
  // task, with no record of what it was last month. The server therefore states
  // it at the status date and nowhere else, and the chart shows it as the single
  // point it is — a line would be a story nobody recorded. Counted rather than
  // assumed, so a server that one day keeps a real EV history draws a line
  // without this file changing.
  const evPoints = curve.reduce((n, p) => n + (p.ev === null ? 0 : 1), 0)
  const evIsPoint = evPoints === 1

  // EV and AC end at the same date and are often close in value: their labels
  // would then sit on top of each other. Pull them apart just enough to read.
  const labelNudge = evEnd && acEnd && Math.abs(Y(evEnd.value) - Y(acEnd.value)) < 13 ? 6 : 0

  const yTicks = [0, 0.25, 0.5, 0.75, 1].map(f => f * yMax)
  const xTickCount = compact ? 3 : 5
  const xTicks = useMemo(() => {
    if (curve.length <= xTickCount) return curve.map(p => p.offset)
    const step = (curve.length - 1) / (xTickCount - 1)
    return Array.from({ length: xTickCount }, (_, i) => curve[Math.round(i * step)].offset)
  }, [curve, xTickCount])

  const onMove = (e: React.PointerEvent<SVGRectElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    const x = e.clientX - rect.left + padL
    let best = 0
    let bestD = Infinity
    curve.forEach((p, i) => {
      const d = Math.abs(X(p.offset) - x)
      if (d < bestD) { bestD = d; best = i }
    })
    setHover(best)
  }

  const hp = hover !== null ? curve[hover] : null
  // The bubble follows the crosshair and flips before it leaves the box.
  const hx = hp ? X(hp.offset) : 0
  const flip = hx > width - 190

  const legend: Array<{ key: string; label: string; color: string; dashed?: boolean; dot?: boolean }> = [
    { key: 'pv', label: t('proj_ev_pv_short', { defaultValue: 'PV — valeur planifiée' }), color: NEUTRAL, dashed: true },
    {
      key: 'ev',
      label: evIsPoint
        ? t('proj_ev_ev_point', { defaultValue: 'EV — valeur acquise (à la date d’état)' })
        : t('proj_ev_ev_short', { defaultValue: 'EV — valeur acquise' }),
      color: colors.ev,
      dot: evIsPoint,
    },
    { key: 'ac', label: t('proj_ev_ac_short', { defaultValue: 'AC — coût réel' }), color: colors.ac },
  ]

  return (
    <div>
      {/* The legend is never optional: three lines cannot be told apart by
          colour alone, and the end-of-line labels disappear on narrow screens. */}
      <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5 mb-2">
        {legend.map(s => (
          <span key={s.key} className="inline-flex items-center gap-2 text-xs text-text-secondary">
            <svg width="18" height="10" aria-hidden="true">
              {s.dot
                ? <circle cx="9" cy="5" r="4.5" fill={s.color} />
                : <line x1="0" y1="5" x2="18" y2="5" stroke={s.color} strokeWidth={2}
                    strokeDasharray={s.dashed ? '4 3' : undefined} strokeLinecap="round" />}
            </svg>
            {s.label}
          </span>
        ))}
      </div>

      <div ref={wrapRef} className="relative w-full" style={{ height }}>
        <svg width={width} height={height} role="img"
          aria-label={t('proj_ev_curve_alt', { defaultValue: 'Courbe en S : valeur planifiée, valeur acquise et coût réel cumulés.' })}>
          {/* Grid and axes stay recessive — the data is the drawing. */}
          {yTicks.map(v => (
            <g key={v}>
              <line x1={padL} y1={Y(v)} x2={padL + iw} y2={Y(v)} stroke={GRID} strokeWidth={1} />
              <text x={padL - 8} y={Y(v) + 4} textAnchor="end"
                fontSize={11} fill="var(--color-text-tertiary, #80868b)" className="tabular-nums">
                {money.compact(v)}
              </text>
            </g>
          ))}
          {xTicks.map(o => (
            <text key={o} x={X(o)} y={height - 10} textAnchor="middle"
              fontSize={11} fill="var(--color-text-tertiary, #80868b)">
              {format(addDays(origin, o), 'd MMM', { locale: getDateLocale(language) })}
            </text>
          ))}

          {/* The status date: the whole screen is stated AT this instant, so the
              curve has to show where it falls. */}
          {statusOffset >= 0 && statusOffset <= maxOffset && (
            <g>
              <line x1={X(statusOffset)} y1={padT} x2={X(statusOffset)} y2={padT + ih}
                stroke="var(--color-text-secondary, #5f6368)" strokeWidth={1.5} strokeDasharray="2 4" />
              <text x={X(statusOffset) + 4} y={padT + 10} fontSize={11}
                fill="var(--color-text-secondary, #5f6368)">
                {t('proj_ev_status_marker', { defaultValue: 'Date d’état' })}
              </text>
            </g>
          )}

          {/* PV first, underneath: it is the ruler, not a contender. */}
          <path d={pathOf(p => p.pv)} fill="none" stroke={NEUTRAL} strokeWidth={2}
            strokeDasharray="5 4" strokeLinecap="round" strokeLinejoin="round" />
          <path d={pathOf(p => p.ac)} fill="none" stroke={colors.ac} strokeWidth={2}
            strokeLinecap="round" strokeLinejoin="round" />
          <path d={pathOf(p => p.ev)} fill="none" stroke={colors.ev} strokeWidth={2}
            strokeLinecap="round" strokeLinejoin="round" />

          {/* The EV marker is not decoration: when earned value is known at one
              date only, this disc IS the series — the path above drew nothing.
              A surface-coloured ring keeps it readable where it lands on the AC
              line, which is exactly where the two are worth comparing. */}
          {evEnd && (
            <circle cx={X(evEnd.offset)} cy={Y(evEnd.value)} r={evIsPoint ? 5.5 : 4.5}
              fill={colors.ev} stroke="var(--color-surface-0, #ffffff)" strokeWidth={evIsPoint ? 2 : 0} />
          )}
          {/* On AC the disc means the opposite: the line stops here because
              nothing is recorded past the status date. */}
          {acEnd && <circle cx={X(acEnd.offset)} cy={Y(acEnd.value)} r={4.5} fill={colors.ac} />}

          {roomForLabels && (
            <g fontSize={11} fill="var(--color-text-secondary, #5f6368)">
              {pvEnd && <text x={X(pvEnd.offset) + 8} y={Y(pvEnd.value) + 4}>PV</text>}
              {evEnd && <text x={X(evEnd.offset) + 8} y={Y(evEnd.value) + 4 - labelNudge}>EV</text>}
              {acEnd && <text x={X(acEnd.offset) + 8} y={Y(acEnd.value) + 4 + labelNudge}>AC</text>}
            </g>
          )}

          {hp && (
            <g>
              <line x1={X(hp.offset)} y1={padT} x2={X(hp.offset)} y2={padT + ih}
                stroke="var(--color-border-strong, #bdc1c6)" strokeWidth={1} />
              <circle cx={X(hp.offset)} cy={Y(hp.pv)} r={4} fill={NEUTRAL} />
              {hp.ev !== null && <circle cx={X(hp.offset)} cy={Y(hp.ev)} r={4} fill={colors.ev} />}
              {hp.ac !== null && <circle cx={X(hp.offset)} cy={Y(hp.ac)} r={4} fill={colors.ac} />}
            </g>
          )}

          {/* One transparent target over the whole plot: the hit area is the
              column, not the 4 px dot. */}
          <rect x={padL} y={padT} width={iw} height={ih} fill="transparent"
            onPointerMove={onMove} onPointerLeave={() => setHover(null)} />
        </svg>

        {hp && (
          <div
            className="absolute pointer-events-none rounded-md border border-border bg-surface-0 shadow-lg px-3 py-2 text-xs"
            style={{ left: flip ? hx - 178 : hx + 12, top: padT + 4, width: 166 }}
          >
            <div className="text-text-primary font-medium mb-1">{dateOf(hp.offset)}</div>
            {[
              { label: 'PV', value: hp.pv as number | null, color: NEUTRAL },
              { label: 'EV', value: hp.ev, color: colors.ev },
              { label: 'AC', value: hp.ac, color: colors.ac },
            ].map(row => (
              <div key={row.label} className="flex items-center gap-2 py-0.5">
                <span className="w-2.5 h-2.5 rounded-full shrink-0" style={{ background: row.color }} />
                <span className="text-text-secondary w-6">{row.label}</span>
                <span className="ml-auto tabular-nums text-text-primary">
                  {row.value === null
                    ? t('proj_ev_not_yet_known', { defaultValue: 'inconnu' })
                    : money.full(row.value)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      <p className="text-[11px] text-text-tertiary mt-1">
        {t('proj_ev_curve_note', {
          defaultValue: 'Montants cumulés en {{currency}}. Le coût réel suit les dates réelles de dépense et s’arrête à la date d’état : au-delà, rien n’est enregistré.',
          currency,
        })}
        {evIsPoint && ' '}
        {evIsPoint && t('proj_ev_curve_note_ev', {
          defaultValue: 'La valeur acquise, elle, ne se lit qu’en un point : l’avancement est un état, pas un historique — le serveur sait ce qui est acquis aujourd’hui, pas ce qui l’était le mois dernier.',
        })}
      </p>
    </div>
  )
}

// ── The screen ────────────────────────────────────────────────────────────────

export default function EarnedValueView({ projectId, canEdit = true, onOpenEntries }: {
  projectId: string
  /** False in the mobile reading mode: the measurement is shown, not retuned. */
  canEdit?: boolean
  /** Opens the cost-entry screen — where AC actually comes from. */
  onOpenEntries?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const { data, isLoading, isError, refetch } = useQuery<CostOverview>({
    queryKey: ['costs', projectId],
    queryFn: () => projectsApi.getCosts(projectId),
  })
  const configMut = useMutation({
    mutationFn: (patch: Partial<Pick<CostConfig, 'currency' | 'eac_method' | 'default_hourly_rate' | 'status_date' | 'manual_etc'>>) =>
      projectsApi.updateCostConfig(projectId, patch),
    onSuccess: () => { setErrorMsg(null); qc.invalidateQueries({ queryKey: ['costs', projectId] }) },
    // The api client flattens server errors to the root: there is no
    // `response.data` to read here.
    onError: (err: unknown) => setErrorMsg((err as { message?: string }).message
      ?? t('common_error', { defaultValue: 'Une erreur est survenue.' })),
  })

  const currency = data?.config.currency ?? 'EUR'
  const money = useMoney(currency, i18n.language)

  const fmtDate = (d: Date) => format(d, 'd MMMM yyyy', { locale: getDateLocale(i18n.language) })

  // ── Coverage: what the indices are actually computed over ───────────────────
  const coverage = data?.coverage
  const gaps = useMemo(() => {
    if (!coverage) return [] as string[]
    const out: string[] = []
    if (coverage.leaf_tasks > 0 && coverage.costed_tasks < coverage.leaf_tasks) {
      out.push(t('proj_ev_gap_budget', {
        defaultValue: '{{missing}} lot(s) sur {{total}} n’ont pas de budget. Ils comptent pour zéro dans le BAC, la valeur planifiée et la valeur acquise : les indices ne portent que sur la part budgétée du projet.',
        missing: coverage.leaf_tasks - coverage.costed_tasks,
        total: coverage.leaf_tasks,
      }))
    }
    if (coverage.logged_hours === 0) {
      out.push(t('proj_ev_gap_hours', {
        defaultValue: 'Aucune heure n’est saisie. Le coût réel se limite aux dépenses directes ({{direct}}) : la main-d’œuvre, qui est presque toujours le premier poste, est absente. Le CPI est donc flatteur — il compare du travail fait à un coût qui n’a pas été enregistré.',
        direct: money.full(coverage.direct_cost),
      }))
    } else if (!coverage.has_rate && coverage.labour_cost === 0) {
      out.push(t('proj_ev_gap_rate', {
        defaultValue: '{{hours}} h sont saisies mais aucun taux horaire par défaut n’est défini : elles ne sont valorisées nulle part et le coût réel les ignore.',
        hours: new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(coverage.logged_hours),
      }))
    }
    return out
  }, [coverage, money, t, i18n.language])

  // ── Rendering guards ────────────────────────────────────────────────────────
  if (isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center bg-surface-1">
        <Spinner />
      </div>
    )
  }
  if (isError || !data) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <EmptyState
            variant="error"
            icon={<AlertTriangle size={26} />}
            title={t('proj_ev_load_error_title', { defaultValue: 'Mesure indisponible' })}
            description={t('proj_ev_load_error', { defaultValue: 'La valeur acquise n’a pas pu être calculée.' })}
            action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
            t={t}
          />
        </div>
      </div>
    )
  }

  const { totals, config, tasks, curve } = data
  const scale = Math.max(totals.bac, totals.ac, totals.ev, 1)
  // Served with the measurement: the day the offsets count from, and where the
  // status date falls among them. Nothing is derived, nothing is guessed.
  const statusDate = parseISO(data.status_date)
  const origin = parseISO(data.origin)
  const statusOffset = data.status_offset

  // ── The verdict ─────────────────────────────────────────────────────────────
  const scheduleWord = totals.spi === null
    ? null
    : totals.spi > 1 + BAND ? t('proj_ev_verdict_ahead', { defaultValue: 'en avance sur le plan' })
    : totals.spi < 1 - BAND ? t('proj_ev_verdict_late', { defaultValue: 'en retard sur le plan' })
    : t('proj_ev_verdict_on_time', { defaultValue: 'conforme au plan' })

  const costWord = totals.cpi === null
    ? null
    : totals.cpi > 1 + BAND ? t('proj_ev_verdict_under', { defaultValue: 'sous le budget' })
    : totals.cpi < 1 - BAND ? t('proj_ev_verdict_over', { defaultValue: 'au-dessus du budget' })
    : t('proj_ev_verdict_on_budget', { defaultValue: 'dans le budget' })

  const scheduleTone: Tone = totals.spi === null ? 'flat' : indexTone(totals.spi)
  const costTone: Tone = totals.cpi === null ? 'flat' : indexTone(totals.cpi)

  const nothingToMeasure = totals.bac === 0
  const curveUseless = nothingToMeasure || curve.length < 2

  // ── The forecast method ─────────────────────────────────────────────────────
  const methodOptions: DropdownOption[] = [
    { value: 'cpi',     label: t('proj_ev_method_cpi', { defaultValue: 'Performance de coût actuelle' }) },
    { value: 'budget',  label: t('proj_ev_method_budget', { defaultValue: 'Reste conforme au budget' }) },
    { value: 'cpi_spi', label: t('proj_ev_method_cpi_spi', { defaultValue: 'Coût et délai actuels' }) },
    { value: 'manual',  label: t('proj_ev_method_manual', { defaultValue: 'Ré-estimation manuelle' }) },
  ]
  const methodExplainer: Record<EacMethod, string> = {
    cpi:     t('proj_ev_method_cpi_help', { defaultValue: 'Suppose que le projet continuera à dépenser aussi (mal ou bien) qu’il l’a fait jusqu’ici : EAC = BAC / CPI.' }),
    budget:  t('proj_ev_method_budget_help', { defaultValue: 'Suppose que le dérapage est derrière nous et que tout le reste se déroulera comme prévu : EAC = AC + BAC − EV.' }),
    cpi_spi: t('proj_ev_method_cpi_spi_help', { defaultValue: 'Suppose que le coût ET le retard se poursuivent tels quels — l’hypothèse la plus pessimiste, et souvent la plus réaliste sur un projet contraint par une date.' }),
    manual:  t('proj_ev_method_manual_help', { defaultValue: 'Ignore les ratios : quelqu’un a ré-estimé le reste à faire à la main. EAC = AC + reste à faire saisi.' }),
  }

  // ── The work-package table ──────────────────────────────────────────────────
  // Sorted worst first: the point of the breakdown is to name WHICH package is
  // slipping, and a table sorted by WBS buries it. Cost variance decides, with
  // schedule variance breaking ties.
  const rows = [...tasks].sort((a, b) => (a.cv + a.sv) - (b.cv + b.sv))
  // Costs charged to no work package are real spend: the server carries them in
  // the project AC but they belong to no line here. The column would otherwise
  // silently fail to add up to the total.
  const unattributedAc = totals.ac - rows.reduce((sum, r) => sum + r.ac, 0)

  const columns: DataTableColumn<CostTaskLine>[] = [
    {
      id: 'wbs', header: t('proj_ev_col_wbs', { defaultValue: 'WBS' }), headerText: 'WBS',
      width: 90, sortValue: r => r.wbs,
      cell: r => <span className="font-mono text-xs text-text-tertiary">{r.wbs}</span>,
    },
    {
      id: 'name', header: t('proj_ev_col_task', { defaultValue: 'Lot de travail' }), headerText: t('proj_ev_col_task', { defaultValue: 'Lot de travail' }),
      minWidth: 200, primary: true, required: true, sortValue: r => r.name,
      cell: r => (
        <div className="min-w-0">
          <div className="text-sm text-text-primary truncate">{r.name}</div>
          <div className="text-[11px] text-text-tertiary">
            {t('proj_ev_progress', { defaultValue: 'Avancement {{p}} %', p: r.progress })}
          </div>
        </div>
      ),
    },
    {
      id: 'bac', header: 'BAC', align: 'right', width: 110, sortValue: r => r.bac ?? -1,
      cell: r => r.bac === null
        ? <Tooltip label={t('proj_ev_no_budget_row', { defaultValue: 'Ce lot n’a pas de budget : il ne pèse ni dans la valeur planifiée, ni dans la valeur acquise.' })}>
            <span className="inline-flex"><Badge variant="neutral">{t('proj_ev_no_budget', { defaultValue: 'Non budgété' })}</Badge></span>
          </Tooltip>
        : <span className="tabular-nums text-sm text-text-primary">{money.full(r.bac)}</span>,
    },
    { id: 'pv', header: 'PV', align: 'right', width: 110, sortValue: r => r.pv,
      cell: r => <span className="tabular-nums text-sm text-text-secondary">{money.full(r.pv)}</span> },
    { id: 'ev', header: 'EV', align: 'right', width: 110, sortValue: r => r.ev,
      cell: r => <span className="tabular-nums text-sm text-text-primary">{money.full(r.ev)}</span> },
    { id: 'ac', header: 'AC', align: 'right', width: 110, sortValue: r => r.ac,
      cell: r => <span className="tabular-nums text-sm text-text-primary">{money.full(r.ac)}</span> },
    {
      id: 'cv', header: 'CV', align: 'right', width: 110, sortValue: r => r.cv,
      cell: r => <span className={`tabular-nums text-sm ${TONE_CLASS[varianceTone(r.cv, scale)]}`}>{money.signed(r.cv)}</span>,
    },
    {
      id: 'sv', header: 'SV', align: 'right', width: 110, sortValue: r => r.sv,
      cell: r => <span className={`tabular-nums text-sm ${TONE_CLASS[varianceTone(r.sv, scale)]}`}>{money.signed(r.sv)}</span>,
    },
    {
      id: 'cpi', header: 'CPI', align: 'right', width: 90, sortValue: r => r.cpi,
      cell: r => r.cpi === null
        ? <span className="text-xs text-text-tertiary">{t('proj_ev_dash', { defaultValue: '—' })}</span>
        : <span className={`tabular-nums text-sm ${TONE_CLASS[indexTone(r.cpi)]}`}>{fmtIndex(r.cpi, i18n.language)}</span>,
    },
    {
      id: 'spi', header: 'SPI', align: 'right', width: 90, sortValue: r => r.spi,
      cell: r => r.spi === null
        ? <span className="text-xs text-text-tertiary">{t('proj_ev_dash', { defaultValue: '—' })}</span>
        : <span className={`tabular-nums text-sm ${TONE_CLASS[indexTone(r.spi)]}`}>{fmtIndex(r.spi, i18n.language)}</span>,
    },
  ]

  const currencyOptions: DropdownOption[] = (() => {
    const common = ['EUR', 'USD', 'GBP', 'CHF', 'CAD', 'XAF', 'XOF', 'MAD', 'TND', 'JPY', 'CNY']
    const all = common.includes(currency) ? common : [currency, ...common]
    return all.map(c => ({ value: c, label: c }))
  })()

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Gauge size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_ev_title', { defaultValue: 'Valeur acquise' })}
          </h1>
          <div className="flex-1" />
          {onOpenEntries && (
            <Button size="sm" variant="secondary" icon={<Receipt size={14} />} onClick={onOpenEntries}>
              {t('proj_ev_open_entries', { defaultValue: 'Dépenses saisies' })}
            </Button>
          )}
        </div>

        {/* The status date is not a setting, it is the premise: "sommes-nous en
            retard ?" means nothing without saying at which date we stand. */}
        <div className="flex items-center gap-3 flex-wrap bg-surface-0 border border-border rounded-xl px-4 py-3">
          <CalendarCheck size={16} className="text-text-secondary shrink-0" />
          <span className="text-sm text-text-secondary">
            {t('proj_ev_status_date_label', { defaultValue: 'Mesure arrêtée au' })}
          </span>
          {canEdit ? (
            <>
              <Input
                type="date"
                className="w-44"
                value={config.status_date ?? ''}
                onChange={e => configMut.mutate({ status_date: e.target.value || null })}
              />
              {config.status_date === null && (
                <span className="text-xs text-text-tertiary">
                  {t('proj_ev_status_today', { defaultValue: 'aujourd’hui ({{date}}) — laissez vide pour suivre le jour courant', date: fmtDate(statusDate) })}
                </span>
              )}
            </>
          ) : (
            <span className="text-sm font-medium text-text-primary">{fmtDate(statusDate)}</span>
          )}
        </div>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {/* A status date before the project starts is not an error, but every
            figure below it is a zero — worth saying once rather than letting the
            reader wonder why nothing is measured. */}
        {statusOffset < 0 && (
          <Callout variant="info" t={t}>
            {t('proj_ev_status_before_start', {
              defaultValue: 'La date d’état précède le début du projet ({{start}}) : rien n’était encore planifié ni produit, tous les montants sont donc nuls.',
              start: fmtDate(origin),
            })}
          </Callout>
        )}

        {/* ── Coverage warning ── The most important block on this screen: an
            index computed over a third of the plan is not a project-level
            statement, and a project where nobody logs hours shows a superb CPI.
            It sits above the numbers, not under them. */}
        {gaps.length > 0 && (
          <Callout
            variant={coverage && coverage.costed_tasks === 0 ? 'danger' : 'warning'}
            title={t('proj_ev_coverage_title', { defaultValue: 'Ces indices ne portent pas sur tout le projet' })}
            t={t}
          >
            <ul className="list-disc pl-4 space-y-1">
              {gaps.map((g, i) => <li key={i}>{g}</li>)}
            </ul>
          </Callout>
        )}

        {/* ── Verdict ── */}
        <Card title={t('proj_ev_verdict_title', { defaultValue: 'Où en est le projet ?' })} icon={<Target size={16} />}>
          {nothingToMeasure ? (
            <p className="text-sm text-text-secondary">
              {t('proj_ev_verdict_no_bac', { defaultValue: 'Aucun lot de travail n’a de budget : le budget à l’achèvement est nul, donc il n’y a rien à mesurer. Attribuez un coût budgété aux lots pour que la valeur acquise ait un sens.' })}
            </p>
          ) : (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                {scheduleWord
                  ? <Badge variant={TONE_VARIANT[scheduleTone]} dot>{scheduleWord}</Badge>
                  : <Badge variant="neutral">{t('proj_ev_schedule_unmeasurable', { defaultValue: 'Avancement non mesurable' })}</Badge>}
                {costWord
                  ? <Badge variant={TONE_VARIANT[costTone]} dot>{costWord}</Badge>
                  : <Badge variant="neutral">{t('proj_ev_cost_unmeasurable', { defaultValue: 'Coût non mesurable' })}</Badge>}
              </div>
              <p className="text-sm text-text-primary leading-relaxed">
                {t('proj_ev_verdict_body', {
                  defaultValue: 'Au {{date}}, le plan prévoyait {{pv}} de travail ; {{ev}} ont réellement été produits, pour {{ac}} dépensés.',
                  date: fmtDate(statusDate),
                  pv: money.full(totals.pv), ev: money.full(totals.ev), ac: money.full(totals.ac),
                })}
                {' '}
                {totals.sv !== 0 && t('proj_ev_verdict_sv', {
                  defaultValue: 'L’écart de délai est de {{sv}} de travail{{sense}}.',
                  sv: money.signed(totals.sv),
                  sense: totals.sv < 0
                    ? t('proj_ev_verdict_sv_late', { defaultValue: ' que le projet aurait dû avoir produit' })
                    : t('proj_ev_verdict_sv_ahead', { defaultValue: ' produits en avance' }),
                })}
                {' '}
                {totals.cv !== 0 && t('proj_ev_verdict_cv', {
                  defaultValue: 'L’écart de coût est de {{cv}}.',
                  cv: money.signed(totals.cv),
                })}
              </p>
              <p className="text-sm text-text-primary leading-relaxed">
                {totals.eac === null || totals.vac === null
                  ? t('proj_ev_verdict_no_eac', { defaultValue: 'L’atterrissage ne peut pas être estimé avec la méthode choisie : elle a besoin d’un indice qui n’existe pas encore (rien n’a été dépensé, ou rien n’a été acquis).' })
                  : totals.vac < 0
                    ? t('proj_ev_verdict_land_over', {
                        defaultValue: 'À ce rythme, le projet atterrit à {{eac}} pour un budget de {{bac}}, soit {{vac}} de dépassement.',
                        eac: money.full(totals.eac), bac: money.full(totals.bac), vac: money.full(Math.abs(totals.vac)),
                      })
                    : t('proj_ev_verdict_land_under', {
                        defaultValue: 'À ce rythme, le projet atterrit à {{eac}} pour un budget de {{bac}}, soit {{vac}} de marge.',
                        eac: money.full(totals.eac), bac: money.full(totals.bac), vac: money.full(totals.vac),
                      })}
              </p>
            </div>
          )}
        </Card>

        {/* ── The four measures, then what they imply ── */}
        <Card title={t('proj_ev_measures_title', { defaultValue: 'Les mesures' })} icon={<Coins size={16} />}>
          <div className={`grid gap-4 ${isMobile ? 'grid-cols-2' : 'grid-cols-4'}`}>
            <Figure label={t('proj_ev_bac', { defaultValue: 'BAC — budget à l’achèvement' })} value={money.full(totals.bac)}
              hint={t('proj_ev_bac_hint', { defaultValue: 'Le total budgété des lots.' })} />
            <Figure label={t('proj_ev_pv', { defaultValue: 'PV — valeur planifiée' })} value={money.full(totals.pv)}
              hint={t('proj_ev_pv_hint', { defaultValue: 'Ce qui devait être fait à la date d’état.' })} />
            <Figure label={t('proj_ev_ev', { defaultValue: 'EV — valeur acquise' })} value={money.full(totals.ev)}
              hint={t('proj_ev_ev_hint', { defaultValue: 'Ce qui l’a été, valorisé au budget — pas au coût.' })} />
            <Figure label={t('proj_ev_ac', { defaultValue: 'AC — coût réel' })} value={money.full(totals.ac)}
              hint={t('proj_ev_ac_hint', { defaultValue: 'Ce que ça a coûté : dépenses saisies et heures valorisées.' })} />
          </div>

          <Separator className="my-4" />

          <div className={`grid gap-4 ${isMobile ? 'grid-cols-2' : 'grid-cols-4'}`}>
            <Figure label={t('proj_ev_cv', { defaultValue: 'CV — écart de coût' })} value={money.signed(totals.cv)}
              tone={varianceTone(totals.cv, scale)}
              hint={t('proj_ev_cv_hint', { defaultValue: 'EV − AC. Négatif = dépassement.' })} />
            <Figure label={t('proj_ev_sv', { defaultValue: 'SV — écart de délai' })} value={money.signed(totals.sv)}
              tone={varianceTone(totals.sv, scale)}
              hint={t('proj_ev_sv_hint', { defaultValue: 'EV − PV, exprimé en travail. Négatif = retard.' })} />
            <IndexFigure label={t('proj_ev_cpi', { defaultValue: 'CPI — indice de coût' })}
              value={totals.cpi} tone={indexTone} language={i18n.language}
              unmeasurableReason={t('proj_ev_cpi_null', { defaultValue: 'CPI = EV / AC, et rien n’a encore été dépensé. Le ratio n’existe pas — afficher 1,00 laisserait croire que tout va bien.' })} />
            <IndexFigure label={t('proj_ev_spi', { defaultValue: 'SPI — indice de délai' })}
              value={totals.spi} tone={indexTone} language={i18n.language}
              unmeasurableReason={t('proj_ev_spi_null', { defaultValue: 'SPI = EV / PV, et rien n’était encore planifié à cette date. Le ratio n’existe pas.' })} />
          </div>
        </Card>

        {/* ── The S-curve ── */}
        <Card title={t('proj_ev_curve_title', { defaultValue: 'Courbe en S' })} icon={<LineChart size={16} />}>
          {curveUseless ? (
            <EmptyState
              variant="unavailable"
              compact
              icon={<LineChart size={26} />}
              title={t('proj_ev_curve_empty_title', { defaultValue: 'Rien à tracer' })}
              description={nothingToMeasure
                ? t('proj_ev_curve_empty_bac', { defaultValue: 'Aucun lot n’a de budget : les trois courbes seraient plates à zéro, ce qui ne dirait rien. Budgétez les lots pour voir la valeur planifiée se dessiner.' })
                : t('proj_ev_curve_empty_span', { defaultValue: 'Le plan ne couvre pas assez de temps pour tracer une courbe : donnez des dates aux lots de travail.' })}
              t={t}
            />
          ) : (
            <SCurve
              curve={curve}
              statusOffset={statusOffset}
              origin={origin}
              language={i18n.language}
              money={money}
              currency={currency}
              compact={isMobile}
            />
          )}
        </Card>

        {/* ── Forecast ── */}
        <Card title={t('proj_ev_forecast_title', { defaultValue: 'Atterrissage' })} icon={<Gauge size={16} />}>
          <div className="flex flex-wrap items-center gap-3 mb-3">
            <span className="text-sm text-text-secondary">
              {t('proj_ev_method_label', { defaultValue: 'Méthode d’estimation' })}
            </span>
            <Dropdown
              value={config.eac_method}
              options={methodOptions}
              onChange={v => configMut.mutate({ eac_method: v as EacMethod })}
              height={36} fontSize={14} width={260} focusable
              disabled={!canEdit}
            />
            {config.eac_method === 'manual' && (
              <div className="flex items-center gap-2">
                <span className="text-sm text-text-secondary">
                  {t('proj_ev_manual_etc', { defaultValue: 'Reste à faire estimé' })}
                </span>
                <div className="w-40">
                  <NumField
                    value={config.manual_etc}
                    disabled={!canEdit}
                    placeholder={currency}
                    onCommit={v => configMut.mutate({ manual_etc: v })}
                  />
                </div>
              </div>
            )}
          </div>
          <p className="text-xs text-text-secondary leading-relaxed mb-4">
            {methodExplainer[config.eac_method]}
          </p>

          <div className={`grid gap-4 ${isMobile ? 'grid-cols-2' : 'grid-cols-4'}`}>
            <Figure label={t('proj_ev_eac', { defaultValue: 'EAC — coût final estimé' })}
              value={totals.eac === null ? t('proj_ev_unmeasurable', { defaultValue: 'Non mesurable' }) : money.full(totals.eac)}
              mono={totals.eac !== null}
              hint={t('proj_ev_eac_hint', { defaultValue: 'Ce que le projet aura coûté en tout, selon la méthode choisie.' })} />
            <Figure label={t('proj_ev_etc', { defaultValue: 'ETC — reste à dépenser' })}
              value={totals.etc === null ? t('proj_ev_unmeasurable', { defaultValue: 'Non mesurable' }) : money.full(totals.etc)}
              mono={totals.etc !== null}
              hint={t('proj_ev_etc_hint', { defaultValue: 'EAC − AC : ce qu’il reste à sortir.' })} />
            <Figure label={t('proj_ev_vac', { defaultValue: 'VAC — écart à l’arrivée' })}
              value={totals.vac === null ? t('proj_ev_unmeasurable', { defaultValue: 'Non mesurable' }) : money.signed(totals.vac)}
              mono={totals.vac !== null}
              tone={totals.vac === null ? 'flat' : varianceTone(totals.vac, scale)}
              hint={t('proj_ev_vac_hint', { defaultValue: 'BAC − EAC. Négatif = le projet finit hors budget.' })} />
            {/* TCPI is read the OTHER way round: it is the performance still
                required, so a high value is an alarm, not a success. */}
            <IndexFigure label={t('proj_ev_tcpi', { defaultValue: 'TCPI — performance à tenir' })}
              value={totals.tcpi} tone={tcpiTone} language={i18n.language}
              unmeasurableReason={t('proj_ev_tcpi_null', { defaultValue: 'TCPI = (BAC − EV) / (BAC − AC), et le budget restant est nul : il n’y a plus de marge à répartir sur le travail restant.' })} />
          </div>
          {totals.tcpi !== null && totals.tcpi > 1 + BAND && (
            <p className="text-xs text-danger mt-3">
              {t('proj_ev_tcpi_warning', {
                defaultValue: 'Pour finir dans le budget, tout le travail restant devrait être réalisé à {{tcpi}} — c’est-à-dire mieux que le projet ne l’a jamais fait ({{cpi}} jusqu’ici).',
                tcpi: fmtIndex(totals.tcpi, i18n.language),
                cpi: totals.cpi === null ? '—' : fmtIndex(totals.cpi, i18n.language),
              })}
            </p>
          )}
        </Card>

        {/* ── Per work package ── */}
        <Card
          title={t('proj_ev_tasks_title', { defaultValue: 'Par lot de travail' })}
          subtitle={t('proj_ev_tasks_sub', { defaultValue: 'Les lots les plus en difficulté d’abord — c’est là qu’on voit lequel dérape.' })}
          flush
          footer={Math.abs(unattributedAc) > 0.5 ? (
            <p className="text-xs text-text-secondary">
              {t('proj_ev_unattributed', {
                defaultValue: '{{amount}} de dépenses ne sont rattachées à aucun lot : elles comptent dans le coût réel du projet, mais n’apparaissent sur aucune ligne ci-dessus.',
                amount: money.full(unattributedAc),
              })}
            </p>
          ) : undefined}
        >
          <DataTable<CostTaskLine>
            rows={rows}
            columns={columns}
            rowKey={r => r.task_id}
            minTableWidth={1100}
            pageSize={0}
            resizableColumns={false}
            emptyState={
              <EmptyState
                variant="first-use"
                icon={<Coins size={26} />}
                title={t('proj_ev_tasks_empty_title', { defaultValue: 'Aucun lot mesurable' })}
                description={t('proj_ev_tasks_empty', { defaultValue: 'La valeur acquise se calcule lot par lot : créez des lots de travail et donnez-leur un budget.' })}
                t={t}
              />
            }
            t={t}
          />
        </Card>

        {/* ── Measurement settings ── */}
        <Card title={t('proj_ev_settings_title', { defaultValue: 'Réglages de la mesure' })} icon={<Settings2 size={16} />} dense>
          <div className={`grid gap-4 ${isMobile ? 'grid-cols-1' : 'grid-cols-3'}`}>
            <div>
              <label className="block text-xs text-text-secondary mb-1">
                {t('proj_ev_currency', { defaultValue: 'Devise' })}
              </label>
              <Dropdown
                value={currency}
                options={currencyOptions}
                onChange={v => configMut.mutate({ currency: v })}
                height={36} fontSize={14} width="100%" focusable
                disabled={!canEdit}
              />
            </div>
            <div>
              <label className="block text-xs text-text-secondary mb-1">
                {t('proj_ev_rate', { defaultValue: 'Taux horaire par défaut' })}
              </label>
              <NumField
                value={config.default_hourly_rate}
                disabled={!canEdit}
                placeholder={t('proj_ev_rate_ph', { defaultValue: 'Aucun' })}
                onCommit={v => configMut.mutate({ default_hourly_rate: v })}
              />
              <p className="text-[11px] text-text-tertiary mt-1">
                {t('proj_ev_rate_hint', { defaultValue: 'Appliqué aux heures saisies quand la ressource n’a pas son propre taux. Sans lui, la main-d’œuvre ne pèse rien dans le coût réel.' })}
              </p>
            </div>
            <div>
              <label className="block text-xs text-text-secondary mb-1">
                {t('proj_ev_settings_status_date', { defaultValue: 'Date d’état' })}
              </label>
              <Input
                type="date"
                value={config.status_date ?? ''}
                disabled={!canEdit}
                onChange={e => configMut.mutate({ status_date: e.target.value || null })}
              />
              <p className="text-[11px] text-text-tertiary mt-1">
                {t('proj_ev_settings_status_hint', { defaultValue: 'Vide = aujourd’hui. Toute la page — écarts, indices, courbe — est arrêtée à cette date.' })}
              </p>
            </div>
          </div>
          {coverage && (
            <p className="text-[11px] text-text-tertiary mt-3">
              {t('proj_ev_coverage_footer', {
                defaultValue: 'Couverture : {{costed}}/{{leaf}} lots budgétés · {{hours}} h saisies · main-d’œuvre {{labour}} · dépenses directes {{direct}}.',
                costed: coverage.costed_tasks, leaf: coverage.leaf_tasks,
                hours: new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(coverage.logged_hours),
                labour: money.full(coverage.labour_cost),
                direct: money.full(coverage.direct_cost),
              })}
            </p>
          )}
        </Card>

      </div>
    </div>
  )
}
