import { useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { useConfirm } from '@kubuno/sdk'
import {
  Users, Plus, Trash2, ChevronRight, ChevronDown, Search, Mail, Network,
  Megaphone, CircleCheck, TrendingDown, X,
} from 'lucide-react'
import {
  Button, Input, Textarea, Dropdown, Callout, Tooltip, EmptyState,
  FloatingWindow, ConfirmDialog, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type Stakeholder, type StakeholderEdit, type StakeholderCategory,
  type EngagementLevel, type EngagementGap, type StakeholderQuadrant,
} from '../api'

// Stakeholder register — who can help the project, who can stop it, and what is
// expected of each of them.
//
// The screen exists for two outputs, and the layout says so:
//
//  • The POWER/INTEREST GRID is not a headcount. Every cell belongs to one of
//    four quadrants, and a quadrant is an INSTRUCTION — manage closely, keep
//    satisfied, keep informed, monitor. Those words are written on the grid
//    itself, never relegated to a legend: a reader must not need a decoder to
//    learn what to do with the person they just found. The grid doubles as the
//    register's filter.
//  • The ENGAGEMENT GAPS are a to-do list. A register that only records the
//    present state describes a situation instead of asking for anything. Each
//    gap is therefore spelled out as a sentence — "X opposes the project, they
//    must be brought round to supporting it" — and the ones the project wants
//    LESS involved (a negative distance, which is rare) are shown, not hidden.
//
// Editing happens in place; the floating window is for creation only.

// ── Vocabularies (the server answers 422 to anything outside these lists) ─────

export const STAKEHOLDER_CATEGORIES: StakeholderCategory[] = [
  'internal', 'external', 'sponsor', 'customer', 'supplier', 'regulator', 'team',
]

/** PMBOK's five levels, IN ORDER. The order is what makes a gap a direction
 *  rather than a mere difference — never re-sort this list. */
export const ENGAGEMENT_LEVELS: EngagementLevel[] = [
  'unaware', 'resistant', 'neutral', 'supportive', 'leading',
]

/**
 * The threshold that cuts the grid in four. The server applies exactly `>= 4`
 * when it fills `summary.by_quadrant`; diverging here would make the drawing
 * disagree with the figures printed on it.
 */
const HIGH = 4

export function quadrantOf(power: number, interest: number): StakeholderQuadrant {
  if (power >= HIGH) return interest >= HIGH ? 'manage_closely' : 'keep_satisfied'
  return interest >= HIGH ? 'keep_informed' : 'monitor'
}

const engagementIndex = (level: EngagementLevel) => {
  const i = ENGAGEMENT_LEVELS.indexOf(level)
  return i < 0 ? 2 : i
}

// ── Human labels ─────────────────────────────────────────────────────────────

function useStakeholderLabels() {
  const { t } = useTranslation('office')
  return useMemo(() => ({
    category: {
      internal:  t('sh_cat_internal',  { defaultValue: 'Interne' }),
      external:  t('sh_cat_external',  { defaultValue: 'Externe' }),
      sponsor:   t('sh_cat_sponsor',   { defaultValue: 'Sponsor' }),
      customer:  t('sh_cat_customer',  { defaultValue: 'Client' }),
      supplier:  t('sh_cat_supplier',  { defaultValue: 'Fournisseur' }),
      regulator: t('sh_cat_regulator', { defaultValue: 'Régulateur' }),
      team:      t('sh_cat_team',      { defaultValue: 'Équipe' }),
    } as Record<StakeholderCategory, string>,
    // The five levels, translated. Kept short: they are read inside a dropdown.
    engagement: {
      unaware:    t('sh_eng_unaware',    { defaultValue: 'Ignore le projet' }),
      resistant:  t('sh_eng_resistant',  { defaultValue: 'S’y oppose' }),
      neutral:    t('sh_eng_neutral',    { defaultValue: 'Neutre' }),
      supportive: t('sh_eng_supportive', { defaultValue: 'Le soutient' }),
      leading:    t('sh_eng_leading',    { defaultValue: 'Le porte' }),
    } as Record<EngagementLevel, string>,
    // What a quadrant PRESCRIBES — the reason the grid is drawn at all.
    quadrant: {
      manage_closely: t('sh_quad_manage', { defaultValue: 'Gérer de près' }),
      keep_satisfied: t('sh_quad_satisfy', { defaultValue: 'Garder satisfait' }),
      keep_informed:  t('sh_quad_inform', { defaultValue: 'Tenir informé' }),
      monitor:        t('sh_quad_monitor', { defaultValue: 'Surveiller' }),
    } as Record<StakeholderQuadrant, string>,
    quadrantAdvice: {
      manage_closely: t('sh_quad_manage_adv', { defaultValue: 'Beaucoup de pouvoir, très concernés : à impliquer dans chaque décision.' }),
      keep_satisfied: t('sh_quad_satisfy_adv', { defaultValue: 'Beaucoup de pouvoir, peu concernés : peuvent tout arrêter sans prévenir.' }),
      keep_informed:  t('sh_quad_inform_adv', { defaultValue: 'Très concernés, peu de pouvoir : de bons relais, à nourrir en information.' }),
      monitor:        t('sh_quad_monitor_adv', { defaultValue: 'Ni pouvoir ni intérêt marqués : un effort minimal suffit, tant que rien ne bouge.' }),
    } as Record<StakeholderQuadrant, string>,
    // A bare 1-to-5 scale means nothing on its own: each notch gets a word.
    power: {
      1: t('sh_power_1', { defaultValue: 'Aucun poids' }),
      2: t('sh_power_2', { defaultValue: 'Peu de poids' }),
      3: t('sh_power_3', { defaultValue: 'Écouté' }),
      4: t('sh_power_4', { defaultValue: 'Influent' }),
      5: t('sh_power_5', { defaultValue: 'Décide' }),
    } as Record<number, string>,
    interest: {
      1: t('sh_interest_1', { defaultValue: 'Indifférent' }),
      2: t('sh_interest_2', { defaultValue: 'Peu concerné' }),
      3: t('sh_interest_3', { defaultValue: 'Concerné' }),
      4: t('sh_interest_4', { defaultValue: 'Très concerné' }),
      5: t('sh_interest_5', { defaultValue: 'Directement impacté' }),
    } as Record<number, string>,
  }), [t])
}

// ── Quadrant colours ─────────────────────────────────────────────────────────
//
// Taken from the theme's CATEGORICAL chart ramp (`--kb-chart-*`), never from the
// status tokens: a quadrant is neither good nor bad, and `--color-danger` on
// "manage closely" would pass a judgement the grid does not make. `monitor` gets
// no hue at all — the quadrant that asks for the least attention is the quietest
// one on the page, and a fourth hue there would only add noise.
//
// The ramp ships two hand-tuned sets rather than one auto-derived pair: Kubuno
// themes do NOT follow `prefers-color-scheme` (the engine writes the variables
// from JS), so the set is picked in JS too. Fallback hexes are spelled out for a
// host older than the ramp, which would otherwise paint nothing.
const QUADRANT_HUE: Record<'light' | 'dark', Record<StakeholderQuadrant, string>> = {
  light: {
    manage_closely: 'var(--kb-chart-1, #2a78d6)',
    keep_satisfied: 'var(--kb-chart-7, #4a3aa7)',
    keep_informed:  'var(--kb-chart-5, #e87ba4)',
    monitor:        'var(--color-text-tertiary, #80868b)',
  },
  dark: {
    manage_closely: 'var(--kb-chart-1-dark, #3987e5)',
    keep_satisfied: 'var(--kb-chart-7-dark, #9085e9)',
    keep_informed:  'var(--kb-chart-5-dark, #d55181)',
    monitor:        'var(--color-text-tertiary, #9aa0a6)',
  },
}

/** The quadrant hue laid over the surface at a chosen opacity. Only the alpha of
 *  the BACKGROUND changes — the figures keep `--color-text-primary`, which flips
 *  with the theme, so they stay legible on a light surface and on a dark one. */
const tint = (color: string, pct: number) =>
  `color-mix(in srgb, ${color} ${pct}%, transparent)`

/**
 * Light or dark, read from the root element's colour scheme.
 *
 * A CSS media query would not do: a hand-picked dark theme does not set
 * `prefers-color-scheme`, it sets `color-scheme` inline from the theme engine.
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

function useQuadrantColors(): Record<StakeholderQuadrant, string> {
  const dark = useDarkScheme()
  return QUADRANT_HUE[dark ? 'dark' : 'light']
}

/** The quadrant as a chip carrying its own colour — same palette as the grid,
 *  which is what lets a row be traced back to the square it came from. */
function QuadrantChip({ quadrant, color, label, title }: {
  quadrant: StakeholderQuadrant
  color: string
  label: string
  title?: string
}) {
  return (
    <span
      title={title}
      data-quadrant={quadrant}
      className="inline-flex items-center gap-1.5 rounded-md px-2 py-0.5 text-sm text-text-primary whitespace-nowrap"
      style={{
        background: tint(color, 16),
        boxShadow: `inset 0 0 0 1px ${tint(color, 45)}`,
      }}
    >
      <span className="w-2 h-2 rounded-full shrink-0" style={{ background: color }} />
      {label}
    </span>
  )
}

// ── The power/interest grid ──────────────────────────────────────────────────

type GridSelection =
  | { type: 'cell'; power: number; interest: number }
  | { type: 'quadrant'; quadrant: StakeholderQuadrant }

/** Same selection twice? Then clicking it again clears it. */
function sameSelection(a: GridSelection | null, b: GridSelection): boolean {
  if (!a || a.type !== b.type) return false
  if (a.type === 'cell' && b.type === 'cell') return a.power === b.power && a.interest === b.interest
  if (a.type === 'quadrant' && b.type === 'quadrant') return a.quadrant === b.quadrant
  return false
}

/** Geometry shared by the four panels and by the two axis rulers, so that the
 *  scale numbers line up with the cells they name. */
interface GridMetrics {
  cell: number
  /** Gap between two cells of the same quadrant. */
  gap: number
  /** Gutter between quadrants — the visible seam of the `>= 4` threshold. */
  gutter: number
  padding: number
  header: number
}

const panelWidth = (m: GridMetrics, cols: number) => m.padding * 2 + cols * m.cell + (cols - 1) * m.gap

/**
 * One quadrant: its instruction, its headcount, and the cells it contains.
 *
 * The four panels are NOT equal quarters — the threshold sits at 4 on a scale of
 * 5, so the "high" side is two notches wide and the "low" side three. Drawing
 * them equal would be prettier and false.
 */
function QuadrantPanel({ quadrant, powers, interests, grid, color, metrics, selection, onSelect, isMobile }: {
  quadrant: StakeholderQuadrant
  /** Power values covered, top to bottom. */
  powers: number[]
  /** Interest values covered, left to right. */
  interests: number[]
  grid: number[][]
  color: string
  metrics: GridMetrics
  selection: GridSelection | null
  onSelect: (next: GridSelection) => void
  isMobile: boolean
}) {
  const { t } = useTranslation('office')
  const labels = useStakeholderLabels()

  const count = (p: number, i: number) => grid?.[p - 1]?.[i - 1] ?? 0
  const total = powers.reduce((sum, p) => sum + interests.reduce((s, i) => s + count(p, i), 0), 0)

  const quadrantSelected = selection?.type === 'quadrant' && selection.quadrant === quadrant
  const headerSelection: GridSelection = { type: 'quadrant', quadrant }

  return (
    <div
      className="rounded-lg"
      style={{
        width: panelWidth(metrics, interests.length),
        padding: metrics.padding,
        background: tint(color, 7),
        boxShadow: quadrantSelected
          ? `inset 0 0 0 2px ${color}`
          : `inset 0 0 0 1px ${tint(color, 32)}`,
      }}
    >
      {/* The instruction, written on the grid itself. Clicking it reads the
          whole quadrant in the register below. */}
      <button
        type="button"
        onClick={() => onSelect(headerSelection)}
        aria-pressed={quadrantSelected}
        title={labels.quadrantAdvice[quadrant]}
        className="w-full text-left block rounded hover:opacity-80 overflow-hidden"
        style={{ height: metrics.header }}
      >
        <span className="flex items-baseline gap-1.5">
          <span className="text-sm font-semibold leading-snug" style={{ color }}>
            {labels.quadrant[quadrant]}
          </span>
          <span className="text-sm text-text-tertiary tabular-nums shrink-0">{total}</span>
        </span>

      </button>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: `repeat(${interests.length}, ${metrics.cell}px)`,
          gap: metrics.gap,
        }}
      >
        {powers.map(p => interests.map(i => {
          const n = count(p, i)
          const cellSelection: GridSelection = { type: 'cell', power: p, interest: i }
          const selected = sameSelection(selection, cellSelection)
          return (
            <Tooltip
              key={`${p}-${i}`}
              label={t('sh_cell_tip', {
                defaultValue: '{{n}} partie(s) prenante(s) · pouvoir {{p}} ({{pw}}) × intérêt {{i}} ({{iw}}) — {{advice}}',
                n, p, i,
                pw: labels.power[p], iw: labels.interest[i],
                advice: labels.quadrantAdvice[quadrant],
              })}
            >
              <button
                type="button"
                // Not `disabled`: a disabled button swallows the pointer events
                // the tooltip needs, and an empty cell is exactly the one whose
                // meaning has to be explained.
                aria-disabled={n === 0}
                aria-pressed={selected}
                onClick={() => { if (n > 0) onSelect(cellSelection) }}
                className={`rounded-md flex items-center justify-center text-text-primary transition-[box-shadow] ${n > 0 ? 'cursor-pointer' : 'cursor-default'}`}
                style={{
                  height: metrics.cell,
                  background: tint(color, n > 0 ? 26 : 8),
                  boxShadow: selected
                    ? `inset 0 0 0 2px ${color}`
                    : `inset 0 0 0 1px ${tint(color, 34)}`,
                }}
              >
                <span
                  className="text-base font-semibold tabular-nums"
                  // An empty cell keeps its figure, kept quiet.
                  style={{ opacity: n > 0 ? 1 : 0.3 }}
                >
                  {n}
                </span>
              </button>
            </Tooltip>
          )
        }))}
      </div>
    </div>
  )
}

/** The power ruler of one band, aligned on that band's cells. */
function PowerRuler({ powers, metrics, isMobile, words }: {
  powers: number[]
  metrics: GridMetrics
  isMobile: boolean
  words: Record<number, string>
}) {
  return (
    <div
      style={{
        paddingTop: metrics.padding + metrics.header,
        display: 'grid',
        gridTemplateRows: `repeat(${powers.length}, ${metrics.cell}px)`,
        rowGap: metrics.gap,
      }}
    >
      {powers.map(p => (
        <div key={p} className="flex items-center justify-end gap-1.5 pr-1 min-w-0">
          {!isMobile && <span className="text-sm text-text-tertiary leading-snug">{words[p]}</span>}
          <span className="text-sm font-semibold text-text-secondary shrink-0">{p}</span>
        </div>
      ))}
    </div>
  )
}

/** The interest ruler of one band, aligned on that band's columns. */
function InterestRuler({ interests, metrics, isMobile, words }: {
  interests: number[]
  metrics: GridMetrics
  isMobile: boolean
  words: Record<number, string>
}) {
  return (
    <div
      style={{
        paddingLeft: metrics.padding,
        paddingRight: metrics.padding,
        display: 'grid',
        gridTemplateColumns: `repeat(${interests.length}, ${metrics.cell}px)`,
        columnGap: metrics.gap,
      }}
    >
      {interests.map(i => (
        <div key={i} className="text-center min-w-0 pt-0.5">
          <div className="text-sm font-semibold text-text-secondary">{i}</div>
          {!isMobile && <div className="text-sm text-text-tertiary leading-snug">{words[i]}</div>}
        </div>
      ))}
    </div>
  )
}

/**
 * The 5×5 grid. Interest runs left to right, power runs BOTTOM to TOP — the
 * usual convention, so the people who must be managed closely sit in the top
 * right corner, where every reader already looks for them.
 */
function PowerInterestGrid({ grid, selection, onSelect, isMobile }: {
  grid: number[][]
  selection: GridSelection | null
  onSelect: (next: GridSelection) => void
  isMobile: boolean
}) {
  const { t } = useTranslation('office')
  const labels = useStakeholderLabels()
  const colors = useQuadrantColors()

  const metrics: GridMetrics = {
    cell: isMobile ? 40 : 72,
    gap: 4,
    gutter: 10,
    padding: 8,
    // One line: the quadrant's name and its count. The instruction that goes with
    // it is too long for a panel this narrow and reads in the legend below, where
    // it fits whole rather than as an ellipsis. Fixed height, because it is what
    // keeps the power ruler aligned with the cells beside it.
    header: isMobile ? 20 : 22,
  }
  const axisWidth = isMobile ? 22 : 122
  // Low side = 1..3, high side = 4..5: the threshold, drawn to scale.
  const lowInterest = [1, 2, 3]
  const highInterest = [4, 5]
  const highPower = [5, 4]
  const lowPower = [3, 2, 1]

  const leftWidth = panelWidth(metrics, lowInterest.length)
  const rightWidth = panelWidth(metrics, highInterest.length)

  const panel = (quadrant: StakeholderQuadrant, powers: number[], interests: number[]) => (
    <QuadrantPanel
      quadrant={quadrant}
      powers={powers}
      interests={interests}
      grid={grid}
      color={colors[quadrant]}
      metrics={metrics}
      selection={selection}
      onSelect={onSelect}
      isMobile={isMobile}
    />
  )

  return (
    <div>
      <div className="flex gap-2">
        {/* Vertical axis title — rotated so it reads bottom-up, like the scale. */}
        <div
          className="flex items-center justify-center text-sm text-text-secondary shrink-0"
          style={{ writingMode: 'vertical-rl', transform: 'rotate(180deg)' }}
        >
          {t('sh_axis_power', { defaultValue: 'Pouvoir' })}
        </div>

        <div className="flex-1 min-w-0 overflow-x-auto">
          <div
            style={{
              width: axisWidth + metrics.gutter * 2 + leftWidth + rightWidth,
              display: 'grid',
              gridTemplateColumns: `${axisWidth}px ${leftWidth}px ${rightWidth}px`,
              gap: metrics.gutter,
            }}
          >
            {/* Top band: power 5 and 4 — the people who can decide. */}
            <PowerRuler powers={highPower} metrics={metrics} isMobile={isMobile} words={labels.power} />
            {panel('keep_satisfied', highPower, lowInterest)}
            {panel('manage_closely', highPower, highInterest)}

            {/* Bottom band: power 3 down to 1. */}
            <PowerRuler powers={lowPower} metrics={metrics} isMobile={isMobile} words={labels.power} />
            {panel('monitor', lowPower, lowInterest)}
            {panel('keep_informed', lowPower, highInterest)}

            {/* Interest scale, under the grid. */}
            <div />
            <InterestRuler interests={lowInterest} metrics={metrics} isMobile={isMobile} words={labels.interest} />
            <InterestRuler interests={highInterest} metrics={metrics} isMobile={isMobile} words={labels.interest} />
          </div>
        </div>
      </div>

      <p className="text-center text-sm text-text-secondary mt-1">
        {t('sh_axis_interest', { defaultValue: 'Intérêt' })}
      </p>

      {/* The instruction each quadrant carries. It lives here rather than in the
          panel header because a header two cells wide can only show it as an
          ellipsis — and the instruction is the whole point of the grid. */}
      {!isMobile && (
        <ul className="mt-3 space-y-1">
          {(['manage_closely', 'keep_satisfied', 'keep_informed', 'monitor'] as StakeholderQuadrant[]).map(q => (
            <li key={q} className="flex items-baseline gap-2 text-sm">
              <span className="w-2 h-2 rounded-full shrink-0 translate-y-px"
                style={{ background: colors[q] }} />
              <span className="font-medium shrink-0" style={{ color: colors[q] }}>
                {labels.quadrant[q]}
              </span>
              <span className="text-text-tertiary leading-snug">{labels.quadrantAdvice[q]}</span>
            </li>
          ))}
        </ul>
      )}

      <p className="text-sm text-text-tertiary mt-2">
        {t('sh_grid_hint', {
          defaultValue: 'Le seuil est à 4 sur 5 : les quatre zones ne sont donc pas des quarts égaux. Cliquez une zone — ou une case — pour ne lire qu’elle dans le registre.',
        })}
      </p>
    </div>
  )
}

// ── Engagement: the ladder, and the gaps as a list of things to do ───────────

/** The five levels as a track: where the person stands, and where the project
 *  needs them. The ground to cover is drawn, not just named. */
function EngagementLadder({ current, desired, color, currentLabel, desiredLabel }: {
  current: EngagementLevel
  desired: EngagementLevel
  color: string
  currentLabel: string
  desiredLabel: string
}) {
  const { t } = useTranslation('office')
  const now = engagementIndex(current)
  const want = engagementIndex(desired)
  const lo = Math.min(now, want)
  const hi = Math.max(now, want)

  return (
    <Tooltip
      label={t('sh_ladder_tip', {
        defaultValue: 'Aujourd’hui : {{current}} · attendu : {{desired}}',
        current: currentLabel, desired: desiredLabel,
      })}
    >
      <div className="flex items-center gap-[3px]" aria-hidden>
        {ENGAGEMENT_LEVELS.map((_, i) => {
          const isNow = i === now
          const isWant = i === want
          const between = i > lo && i < hi
          return (
            <span
              key={i}
              className="rounded-sm block"
              style={{
                width: isNow || isWant ? 16 : 12,
                height: isNow ? 12 : 8,
                background: isNow ? color
                  : isWant ? tint(color, 22)
                    : between ? tint(color, 14)
                      : 'var(--color-surface-2)',
                boxShadow: isWant ? `inset 0 0 0 1.5px ${color}` : undefined,
              }}
            />
          )
        })}
      </div>
    </Tooltip>
  )
}

/** The current level, told as a state rather than as a keyword. */
function currentClause(t: TFunction, level: EngagementLevel): string {
  switch (level) {
    case 'unaware':    return t('sh_state_unaware', { defaultValue: 'ignore encore l’existence du projet' })
    case 'resistant':  return t('sh_state_resistant', { defaultValue: 's’oppose au projet' })
    case 'neutral':    return t('sh_state_neutral', { defaultValue: 'reste neutre vis-à-vis du projet' })
    case 'supportive': return t('sh_state_supportive', { defaultValue: 'soutient le projet' })
    case 'leading':    return t('sh_state_leading', { defaultValue: 'porte le projet' })
  }
}

/** The desired level, told as the thing to obtain — an infinitive, so it slots
 *  into "il faut l’amener à …". */
function goalClause(t: TFunction, level: EngagementLevel): string {
  switch (level) {
    case 'unaware':    return t('sh_goal_unaware', { defaultValue: 'se désintéresser du projet' })
    case 'resistant':  return t('sh_goal_resistant', { defaultValue: 'exprimer ouvertement ses réserves' })
    case 'neutral':    return t('sh_goal_neutral', { defaultValue: 'adopter une position neutre' })
    case 'supportive': return t('sh_goal_supportive', { defaultValue: 'soutenir le projet' })
    case 'leading':    return t('sh_goal_leading', { defaultValue: 'porter le projet' })
  }
}

/** One gap, written as an instruction. */
function gapSentence(t: TFunction, gap: EngagementGap, desiredLabel: string): string {
  const state = currentClause(t, gap.current)
  if (gap.distance > 0) {
    return t('sh_gap_up', {
      defaultValue: '{{name}} {{state}} : il faut l’amener à {{goal}}.',
      name: gap.name, state, goal: goalClause(t, gap.desired),
    })
  }
  // Negative: the project wants LESS involvement than it has today. Rare enough
  // that it deserves its own wording instead of being folded into the same list.
  return t('sh_gap_down', {
    defaultValue: '{{name}} {{state}} — plus que ce que le projet attend de lui : le niveau visé est « {{desired}} ».',
    name: gap.name, state, desired: desiredLabel,
  })
}

function GapItem({ gap, onOpen }: { gap: EngagementGap; onOpen: () => void }) {
  const { t } = useTranslation('office')
  const labels = useStakeholderLabels()
  const colors = useQuadrantColors()
  const color = colors[gap.quadrant]
  const steps = Math.abs(gap.distance)
  const down = gap.distance < 0

  return (
    <button
      type="button"
      onClick={onOpen}
      title={t('sh_gap_open', { defaultValue: 'Voir cette partie prenante dans le registre' })}
      className="w-full text-left rounded-lg border border-border bg-surface-0 px-3 py-2 hover:bg-surface-1"
    >
      <div className="flex items-start gap-3">
        <div className="flex-1 min-w-0">
          <p className="text-sm text-text-primary">
            {gapSentence(t, gap, labels.engagement[gap.desired])}
          </p>
          <div className="flex items-center gap-2 flex-wrap mt-1.5">
            <QuadrantChip
              quadrant={gap.quadrant}
              color={color}
              label={labels.quadrant[gap.quadrant]}
              title={labels.quadrantAdvice[gap.quadrant]}
            />
            <span className="inline-flex items-center gap-1 text-sm text-text-secondary">
              {down && <TrendingDown size={13} />}
              {down
                ? t('sh_gap_steps_down', { defaultValue: '{{n}} niveau(x) de trop', n: steps })
                : t('sh_gap_steps_up', { defaultValue: '{{n}} niveau(x) à gagner', n: steps })}
            </span>
            <span className="text-sm text-text-tertiary">
              {t('sh_gap_scale', {
                defaultValue: 'pouvoir {{p}} · intérêt {{i}}',
                p: gap.power, i: gap.interest,
              })}
            </span>
          </div>
        </div>
        <div className="shrink-0 pt-0.5">
          <EngagementLadder
            current={gap.current}
            desired={gap.desired}
            color={color}
            currentLabel={labels.engagement[gap.current]}
            desiredLabel={labels.engagement[gap.desired]}
          />
        </div>
      </div>
    </button>
  )
}

// ── In-place editors ─────────────────────────────────────────────────────────

/** Single-line field committed to the server on blur, never on each keystroke. */
function InlineText({ value, placeholder, disabled, className, type, required, onCommit }: {
  value: string
  placeholder?: string
  disabled?: boolean
  className?: string
  type?: string
  /** Refuse an empty value and snap back rather than let the server answer 422. */
  required?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  if (disabled) {
    return <span className={`text-sm text-text-primary ${className ?? ''}`}>{value || '—'}</span>
  }
  return (
    <Input
      type={type}
      className={className}
      value={draft}
      placeholder={placeholder}
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

/** The 1→5 scale as five bars: readable without being read, and settable in one
 *  click — a 5-item menu for a 5-notch scale is a menu too many. */
const SCALE_BAR_HEIGHT = [8, 11, 14, 17, 20]

function ScaleGauge({ value, word, disabled, onChange }: {
  value: number
  /** Full wording of the current notch, for the tooltip. */
  word: string
  disabled?: boolean
  onChange?: (next: number) => void
}) {
  const level = Math.min(5, Math.max(1, Math.round(value) || 1))
  const editable = !disabled && !!onChange
  const color = 'var(--color-primary)'
  return (
    <Tooltip label={word}>
      <div className="flex items-center gap-2 h-9">
        <div className="flex items-end gap-[3px] h-[20px]">
          {SCALE_BAR_HEIGHT.map((h, i) => {
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
      </div>
    </Tooltip>
  )
}

// ── One row of the register ──────────────────────────────────────────────────

/**
 * Everything a row needs from the screen, passed as one object so the row
 * components can live at module level: defined inside the parent they would be
 * a new component TYPE on every render, remounting the list — and dropping the
 * edit in progress — each time a query settles.
 */
interface RowCtx {
  t: TFunction
  canEdit: boolean
  isMobile: boolean
  colCount: number
  labels: ReturnType<typeof useStakeholderLabels>
  colors: Record<StakeholderQuadrant, string>
  categoryOptions: DropdownOption[]
  engagementOptions: DropdownOption[]
  expanded: Set<string>
  toggle: (id: string) => void
  patch: (id: string, body: StakeholderEdit) => void
  askDelete: (holder: Stakeholder) => void
}

function EngagementPair({ holder, ctx, stacked }: {
  holder: Stakeholder
  ctx: RowCtx
  stacked?: boolean
}) {
  const { t, canEdit, engagementOptions } = ctx
  const width = stacked ? '100%' : 138
  return (
    <div className={stacked ? 'grid grid-cols-2 gap-2' : 'flex items-center gap-1.5'}>
      <Dropdown
        height={36} fontSize={14} focusable width={width}
        disabled={!canEdit}
        value={holder.engagement_current}
        options={engagementOptions}
        onChange={v => ctx.patch(holder.id, { engagement_current: v as EngagementLevel })}
      />
      {!stacked && (
        <span className="text-text-tertiary shrink-0" title={t('sh_col_engagement', { defaultValue: 'Engagement : actuel → souhaité' })}>→</span>
      )}
      <Dropdown
        height={36} fontSize={14} focusable width={width}
        disabled={!canEdit}
        value={holder.engagement_desired}
        options={engagementOptions}
        onChange={v => ctx.patch(holder.id, { engagement_desired: v as EngagementLevel })}
      />
    </div>
  )
}

function StakeholderDetail({ holder, ctx }: { holder: Stakeholder; ctx: RowCtx }) {
  const { t, canEdit, isMobile, colCount, labels, categoryOptions } = ctx
  return (
    <tr className="border-t border-border bg-surface-1">
      <td colSpan={colCount} className="px-3 py-3">
        <div className={isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-3'}>
          {/* On mobile the row itself cannot carry these columns: they live here. */}
          {isMobile && (
            <>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('sh_col_category', { defaultValue: 'Catégorie' })}
                </label>
                <Dropdown
                  height={36} fontSize={14} focusable width="100%"
                  disabled={!canEdit}
                  value={holder.category}
                  options={categoryOptions}
                  onChange={v => ctx.patch(holder.id, { category: v as StakeholderCategory })}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('sh_col_engagement', { defaultValue: 'Engagement : actuel → souhaité' })}
                </label>
                <EngagementPair holder={holder} ctx={ctx} stacked />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('sh_col_organisation', { defaultValue: 'Organisation' })}
                </label>
                <InlineText
                  value={holder.organisation}
                  disabled={!canEdit}
                  placeholder={t('sh_organisation_ph', { defaultValue: 'Direction, entreprise, administration…' })}
                  onCommit={v => ctx.patch(holder.id, { organisation: v })}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('sh_col_role', { defaultValue: 'Rôle' })}
                </label>
                <InlineText
                  value={holder.role_title}
                  disabled={!canEdit}
                  placeholder={t('sh_role_ph', { defaultValue: 'Fonction dans l’organisation' })}
                  onCommit={v => ctx.patch(holder.id, { role_title: v })}
                />
              </div>
            </>
          )}

          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_email', { defaultValue: 'Courriel de contact' })}
            </label>
            <InlineText
              type="email"
              value={holder.contact_email}
              disabled={!canEdit}
              placeholder={t('sh_email_ph', { defaultValue: 'prenom.nom@exemple.fr' })}
              onCommit={v => ctx.patch(holder.id, { contact_email: v })}
            />
            <p className="text-sm text-text-tertiary mt-1">
              {t('sh_email_hint', { defaultValue: 'Par où passe la communication convenue plus bas.' })}
            </p>
          </div>

          <LongField
            label={t('sh_col_expectations', { defaultValue: 'Attentes' })}
            hint={t('sh_expectations_hint', { defaultValue: 'Ce que cette personne attend du projet — et à quoi elle jugera qu’il a réussi.' })}
            value={holder.expectations}
            disabled={!canEdit}
            onCommit={v => ctx.patch(holder.id, { expectations: v })}
          />
          <LongField
            label={t('sh_col_influence', { defaultValue: 'Influence' })}
            hint={t('sh_influence_hint', { defaultValue: 'Sur quoi elle pèse, par qui elle est écoutée, ce qu’elle peut débloquer ou arrêter.' })}
            value={holder.influence_notes}
            disabled={!canEdit}
            onCommit={v => ctx.patch(holder.id, { influence_notes: v })}
          />
          <LongField
            label={t('sh_col_communication', { defaultValue: 'Communication' })}
            hint={t('sh_communication_hint', { defaultValue: 'Quoi lui dire, à quelle fréquence, par quel canal, et qui s’en charge.' })}
            value={holder.communication_notes}
            disabled={!canEdit}
            onCommit={v => ctx.patch(holder.id, { communication_notes: v })}
          />
        </div>
      </td>
    </tr>
  )
}

function StakeholderRow({ holder, ctx }: { holder: Stakeholder; ctx: RowCtx }) {
  const { t, canEdit, isMobile, labels, colors, categoryOptions } = ctx
  const expanded = ctx.expanded.has(holder.id)
  const quadrant = quadrantOf(holder.power, holder.interest)
  const cell = 'px-2 py-1.5 align-middle'

  return (
    <>
      <tr id={`sh-row-${holder.id}`} className="border-t border-border hover:bg-surface-1">
        {/* Name — also the disclosure control for the detail panel. */}
        <td className={cell}>
          <div className="flex items-center gap-1">
            <button
              onClick={() => ctx.toggle(holder.id)}
              title={t('sh_toggle_detail', { defaultValue: 'Attentes, influence, communication' })}
              className="p-1 rounded text-text-tertiary hover:bg-surface-2 shrink-0"
            >
              {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
            </button>
            <InlineText
              value={holder.name}
              required
              disabled={!canEdit}
              className="flex-1 min-w-0"
              placeholder={t('sh_name_ph', { defaultValue: 'Nom' })}
              onCommit={v => ctx.patch(holder.id, { name: v.trim() })}
            />
            {holder.contact_email && (
              <Tooltip label={holder.contact_email}>
                <span className="text-text-tertiary shrink-0 inline-flex"><Mail size={14} /></span>
              </Tooltip>
            )}
          </div>
        </td>

        {!isMobile && (
          <td className={cell}>
            <InlineText
              value={holder.organisation}
              disabled={!canEdit}
              placeholder={t('sh_organisation_ph', { defaultValue: 'Direction, entreprise, administration…' })}
              onCommit={v => ctx.patch(holder.id, { organisation: v })}
            />
          </td>
        )}
        {!isMobile && (
          <td className={cell}>
            <InlineText
              value={holder.role_title}
              disabled={!canEdit}
              placeholder={t('sh_role_ph', { defaultValue: 'Fonction dans l’organisation' })}
              onCommit={v => ctx.patch(holder.id, { role_title: v })}
            />
          </td>
        )}
        {!isMobile && (
          <td className={cell}>
            <Dropdown
              height={36} fontSize={14} focusable width="100%"
              disabled={!canEdit}
              value={holder.category}
              options={categoryOptions}
              onChange={v => ctx.patch(holder.id, { category: v as StakeholderCategory })}
            />
          </td>
        )}

        <td className={cell}>
          <ScaleGauge
            value={holder.power}
            word={t('sh_power_tip', {
              defaultValue: 'Pouvoir {{n}} sur 5 — {{word}}',
              n: holder.power, word: labels.power[holder.power] ?? '',
            })}
            disabled={!canEdit}
            onChange={n => ctx.patch(holder.id, { power: n })}
          />
        </td>
        <td className={cell}>
          <ScaleGauge
            value={holder.interest}
            word={t('sh_interest_tip', {
              defaultValue: 'Intérêt {{n}} sur 5 — {{word}}',
              n: holder.interest, word: labels.interest[holder.interest] ?? '',
            })}
            disabled={!canEdit}
            onChange={n => ctx.patch(holder.id, { interest: n })}
          />
        </td>

        {!isMobile && (
          <td className={cell}>
            <QuadrantChip
              quadrant={quadrant}
              color={colors[quadrant]}
              label={labels.quadrant[quadrant]}
              title={labels.quadrantAdvice[quadrant]}
            />
          </td>
        )}
        {!isMobile && (
          <td className={cell}>
            <EngagementPair holder={holder} ctx={ctx} />
          </td>
        )}

        <td className={`${cell} text-right`}>
          {canEdit && (
            <button
              onClick={() => ctx.askDelete(holder)}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="p-1.5 rounded text-text-tertiary hover:text-danger hover:bg-surface-2"
            >
              <Trash2 size={15} />
            </button>
          )}
        </td>
      </tr>

      {expanded && <StakeholderDetail holder={holder} ctx={ctx} />}
    </>
  )
}

// ── Creation dialog (a window is for creation only — edits happen in place) ───

function CreateDialog({ onClose, onCreate, pending }: {
  onClose: () => void
  onCreate: (data: StakeholderEdit & { name: string }) => void
  pending: boolean
}) {
  const { t } = useTranslation('office')
  const labels = useStakeholderLabels()
  const colors = useQuadrantColors()
  const [name, setName] = useState('')
  const [organisation, setOrganisation] = useState('')
  const [roleTitle, setRoleTitle] = useState('')
  const [category, setCategory] = useState<StakeholderCategory>('internal')
  const [power, setPower] = useState(3)
  const [interest, setInterest] = useState(3)
  const [current, setCurrent] = useState<EngagementLevel>('neutral')
  const [desired, setDesired] = useState<EngagementLevel>('supportive')

  const quadrant = quadrantOf(power, interest)
  const engagementOptions: DropdownOption[] =
    ENGAGEMENT_LEVELS.map(v => ({ value: v, label: labels.engagement[v] }))

  const submit = () => {
    const trimmed = name.trim()
    if (!trimmed) return
    onCreate({
      name: trimmed,
      organisation: organisation.trim(),
      role_title: roleTitle.trim(),
      category,
      power,
      interest,
      engagement_current: current,
      engagement_desired: desired,
    })
  }

  return (
    <FloatingWindow
      title={t('sh_new_title', { defaultValue: 'Nouvelle partie prenante' })}
      icon={<Users size={16} />}
      onClose={onClose}
      defaultWidth={560} defaultHeight={520} padding={16} t={t}
      actions={{
        confirm: {
          label: t('common_create', { defaultValue: 'Créer' }),
          onClick: submit,
          disabled: !name.trim(),
          loading: pending,
          autoFocus: false,
        },
        cancel: { label: t('common_cancel', { defaultValue: 'Annuler' }) },
      }}
    >
      <div className="space-y-3">
        <div>
          <label className="text-sm text-text-secondary mb-1 block">
            {t('sh_col_name', { defaultValue: 'Nom' })}
          </label>
          <Input
            autoFocus
            value={name}
            placeholder={t('sh_name_dialog_ph', { defaultValue: 'La personne ou l’instance à qui le projet a affaire…' })}
            onChange={e => setName(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submit() } }}
          />
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_organisation', { defaultValue: 'Organisation' })}
            </label>
            <Input
              value={organisation}
              placeholder={t('sh_organisation_ph', { defaultValue: 'Direction, entreprise, administration…' })}
              onChange={e => setOrganisation(e.target.value)}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_role', { defaultValue: 'Rôle' })}
            </label>
            <Input
              value={roleTitle}
              placeholder={t('sh_role_ph', { defaultValue: 'Fonction dans l’organisation' })}
              onChange={e => setRoleTitle(e.target.value)}
            />
          </div>
          <div className="col-span-2">
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_category', { defaultValue: 'Catégorie' })}
            </label>
            <Dropdown
              height={36} fontSize={14} focusable width="100%"
              value={category}
              options={STAKEHOLDER_CATEGORIES.map(v => ({ value: v, label: labels.category[v] }))}
              onChange={v => setCategory(v as StakeholderCategory)}
            />
          </div>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_power', { defaultValue: 'Pouvoir' })}
            </label>
            <div className="flex items-center gap-2">
              <ScaleGauge
                value={power}
                word={t('sh_power_tip', { defaultValue: 'Pouvoir {{n}} sur 5 — {{word}}', n: power, word: labels.power[power] ?? '' })}
                onChange={setPower}
              />
              <span className="text-sm text-text-tertiary leading-snug">{labels.power[power]}</span>
            </div>
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_interest', { defaultValue: 'Intérêt' })}
            </label>
            <div className="flex items-center gap-2">
              <ScaleGauge
                value={interest}
                word={t('sh_interest_tip', { defaultValue: 'Intérêt {{n}} sur 5 — {{word}}', n: interest, word: labels.interest[interest] ?? '' })}
                onChange={setInterest}
              />
              <span className="text-sm text-text-tertiary leading-snug">{labels.interest[interest]}</span>
            </div>
          </div>
        </div>

        {/* What that pair means, said straight away — otherwise the two scales
            are just two numbers the creator has no reason to care about. */}
        <div className="flex items-start gap-2 rounded-lg bg-surface-1 border border-border px-3 py-2">
          <QuadrantChip
            quadrant={quadrant}
            color={colors[quadrant]}
            label={labels.quadrant[quadrant]}
          />
          <span className="text-sm text-text-secondary flex-1 min-w-0">
            {labels.quadrantAdvice[quadrant]}
          </span>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_engagement_current', { defaultValue: 'Engagement actuel' })}
            </label>
            <Dropdown
              height={36} fontSize={14} focusable width="100%"
              value={current}
              options={engagementOptions}
              onChange={v => setCurrent(v as EngagementLevel)}
            />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">
              {t('sh_col_engagement_desired', { defaultValue: 'Engagement souhaité' })}
            </label>
            <Dropdown
              height={36} fontSize={14} focusable width="100%"
              value={desired}
              options={engagementOptions}
              onChange={v => setDesired(v as EngagementLevel)}
            />
          </div>
        </div>

        <p className="text-sm text-text-tertiary">
          {t('sh_new_hint', { defaultValue: 'Les attentes, l’influence et le plan de communication se saisissent ensuite dans le registre.' })}
        </p>
      </div>
    </FloatingWindow>
  )
}

// ── The register ─────────────────────────────────────────────────────────────

export default function StakeholdersView({ projectId, canEdit = true, onOpenRaci }: {
  projectId: string
  /** False in the mobile reading mode, where the project is shown, not edited. */
  canEdit?: boolean
  /** Jumps to the RACI matrix — the two artefacts are read together. */
  onOpenRaci?: () => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const labels = useStakeholderLabels()
  const colors = useQuadrantColors()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()

  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [creating, setCreating] = useState(false)
  const [search, setSearch] = useState('')
  const [categoryFilter, setCategoryFilter] = useState<'all' | StakeholderCategory>('all')
  const [selection, setSelection] = useState<GridSelection | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const { data, isLoading } = useQuery({
    queryKey: ['stakeholders', projectId],
    queryFn: () => projectsApi.getStakeholders(projectId),
  })

  const inval = () => { qc.invalidateQueries({ queryKey: ['stakeholders', projectId] }) }
  // The api client flattens server errors to the root — `response.data` is undefined.
  const fail = (err: unknown) =>
    setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const createMut = useMutation({
    mutationFn: (payload: StakeholderEdit & { name: string }) => projectsApi.createStakeholder(projectId, payload),
    onSuccess: () => { setErrorMsg(null); setCreating(false); inval() },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ hid, patch }: { hid: string; patch: StakeholderEdit }) =>
      projectsApi.updateStakeholder(projectId, hid, patch),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (hid: string) => projectsApi.deleteStakeholder(projectId, hid),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })

  const all = useMemo(() => data?.stakeholders ?? [], [data])
  const grid = data?.grid ?? []
  const summary = data?.summary
  // Already sorted by the server, most urgent first (distance, then power × interest).
  const gaps: EngagementGap[] = useMemo(() => data?.gaps ?? [], [data])

  const categoryOptions: DropdownOption[] = useMemo(
    () => STAKEHOLDER_CATEGORIES.map(v => ({ value: v, label: labels.category[v] })),
    [labels],
  )
  const engagementOptions: DropdownOption[] = useMemo(
    () => ENGAGEMENT_LEVELS.map(v => ({ value: v, label: labels.engagement[v] })),
    [labels],
  )

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return all.filter(h => {
      if (categoryFilter !== 'all' && h.category !== categoryFilter) return false
      if (selection?.type === 'cell'
        && (h.power !== selection.power || h.interest !== selection.interest)) return false
      if (selection?.type === 'quadrant'
        && quadrantOf(h.power, h.interest) !== selection.quadrant) return false
      if (!q) return true
      return h.name.toLowerCase().includes(q)
        || h.organisation.toLowerCase().includes(q)
        || h.role_title.toLowerCase().includes(q)
        || h.contact_email.toLowerCase().includes(q)
        || h.expectations.toLowerCase().includes(q)
    })
  }, [all, search, categoryFilter, selection])

  const toggle = (id: string) => setExpanded(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  /** Reading a gap means going to the person it names: open their detail panel,
   *  drop whatever filter would hide them, and scroll them into view. */
  const openHolder = (id: string) => {
    setSelection(null)
    setCategoryFilter('all')
    setSearch('')
    setExpanded(prev => new Set(prev).add(id))
    // After the filters have been re-rendered, not before.
    requestAnimationFrame(() => {
      document.getElementById(`sh-row-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    })
  }

  const askDelete = async (holder: Stakeholder) => {
    const ok = await confirm({
      title: t('sh_delete_title', { defaultValue: 'Retirer cette partie prenante ?' }),
      message: t('sh_delete_msg', {
        defaultValue: '« {{name}} » quittera le registre, la grille pouvoir / intérêt et la matrice RACI, où ses rôles seront effacés.',
        name: holder.name,
      }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) deleteMut.mutate(holder.id)
  }

  const select = (next: GridSelection) =>
    setSelection(prev => (sameSelection(prev, next) ? null : next))

  const filtersActive = search.trim() !== '' || categoryFilter !== 'all' || selection !== null
  const clearFilters = () => { setSearch(''); setCategoryFilter('all'); setSelection(null) }

  // Columns actually rendered — the detail panel takes over the rest on mobile.
  const colCount = isMobile ? 4 : 9
  // Kept in step with the widths declared below: a table narrower than the sum
  // of its columns is silently squeezed, and the content truncated.
  const minTableWidth = isMobile ? 460 : 1482

  const ctx: RowCtx = {
    t, canEdit, isMobile, colCount, labels, colors,
    categoryOptions, engagementOptions, expanded, toggle,
    patch: (id, body) => updateMut.mutate({ hid: id, patch: body }),
    askDelete: h => { void askDelete(h) },
  }

  const th = 'text-left text-sm font-medium text-text-secondary px-2 py-2 whitespace-nowrap'

  const selectionLabel = selection === null ? ''
    : selection.type === 'quadrant'
      ? t('sh_filter_quadrant', {
        defaultValue: 'Zone « {{name}} »',
        name: labels.quadrant[selection.quadrant],
      })
      : t('sh_filter_cell', {
        defaultValue: 'Pouvoir {{p}} · intérêt {{i}}',
        p: selection.power, i: selection.interest,
      })

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Users size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('sh_title', { defaultValue: 'Registre des parties prenantes' })}
          </h1>
          {summary && summary.total > 0 && (
            <span className="text-sm text-text-tertiary">
              {t('sh_count_summary', {
                defaultValue: '{{total}} partie(s) prenante(s) · {{aligned}} au niveau d’engagement attendu',
                total: summary.total, aligned: summary.aligned,
              })}
            </span>
          )}
          <div className="flex-1" />
          {onOpenRaci && (
            <Button size="sm" variant="secondary" icon={<Network size={14} />} onClick={onOpenRaci}>
              {t('sh_open_raci', { defaultValue: 'Matrice RACI' })}
            </Button>
          )}
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreating(true)}>
              {t('sh_new', { defaultValue: 'Nouvelle partie prenante' })}
            </Button>
          )}
        </div>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)}>{errorMsg}</Callout>
        )}

        {isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : all.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              icon={<Users size={26} />}
              variant="first-use"
              title={t('sh_empty_title', { defaultValue: 'Aucune partie prenante recensée' })}
              description={t('sh_empty_desc', {
                defaultValue: 'Le registre recense tous ceux dont le projet dépend : qui peut l’aider, qui peut le bloquer, et ce que chacun en attend. Chaque personne y est placée selon son pouvoir et son intérêt — ce qui dit l’attention qu’elle mérite — et selon l’écart entre son engagement d’aujourd’hui et celui dont le projet a besoin.',
              })}
              action={canEdit ? {
                label: t('sh_new', { defaultValue: 'Nouvelle partie prenante' }),
                onClick: () => setCreating(true),
                icon: <Plus size={15} />,
              } : undefined}
              t={t}
            />
          </div>
        ) : (
          <>
            {/* The grid and the gaps, side by side: the first says how much
                attention each person is owed, the second what to do about it. */}
            <div className={isMobile ? 'space-y-4' : 'flex gap-4 items-start'}>
              <div className={`bg-surface-0 border border-border rounded-xl p-4 ${isMobile ? '' : 'shrink-0'}`}>
                <div className="flex items-center gap-2 flex-wrap mb-3">
                  <h2 className="text-base font-medium text-text-primary">
                    {t('sh_grid_title', { defaultValue: 'Grille pouvoir / intérêt' })}
                  </h2>
                </div>
                <PowerInterestGrid
                  grid={grid}
                  selection={selection}
                  onSelect={select}
                  isMobile={isMobile}
                />
              </div>

              <div className={`bg-surface-0 border border-border rounded-xl p-4 ${isMobile ? '' : 'flex-1 min-w-0'}`}>
                <div className="flex items-center gap-2 flex-wrap mb-1">
                  <Megaphone size={16} className="text-text-secondary shrink-0" />
                  <h2 className="text-base font-medium text-text-primary">
                    {t('sh_gaps_title', { defaultValue: 'Écarts d’engagement' })}
                  </h2>
                </div>

                {gaps.length === 0 ? (
                  <Callout variant="success" icon={<CircleCheck size={16} />} className="mt-2">
                    {t('sh_gaps_none', {
                      defaultValue: 'Les {{total}} parties prenantes sont déjà au niveau d’engagement attendu. Il n’y a rien à rattraper — seulement à le maintenir.',
                      total: summary?.total ?? all.length,
                    })}
                  </Callout>
                ) : (
                  <>
                    <p className="text-sm text-text-tertiary mb-3">
                      {t('sh_gaps_hint', {
                        defaultValue: 'À traiter de haut en bas : le plus grand écart d’abord, et à écart égal celui qui pèse le plus sur le projet.',
                      })}
                    </p>
                    <div className="space-y-2">
                      {gaps.map(gap => (
                        <GapItem key={gap.id} gap={gap} onOpen={() => openHolder(gap.id)} />
                      ))}
                    </div>
                    {(summary?.aligned ?? 0) > 0 && (
                      <p className="text-sm text-text-tertiary mt-3">
                        {t('sh_gaps_aligned', {
                          defaultValue: '{{aligned}} autre(s) partie(s) prenante(s) sont déjà au niveau attendu.',
                          aligned: summary?.aligned ?? 0,
                        })}
                      </p>
                    )}
                  </>
                )}
              </div>
            </div>

            {/* Filters */}
            <div className="flex items-center gap-2 flex-wrap">
              <Input
                className="w-64"
                value={search}
                leftIcon={<Search size={15} />}
                placeholder={t('sh_search_ph', { defaultValue: 'Rechercher une partie prenante…' })}
                onChange={e => setSearch(e.target.value)}
              />
              <Dropdown
                height={36} fontSize={14} focusable width={185}
                value={categoryFilter}
                options={[
                  { value: 'all', label: t('sh_cat_all', { defaultValue: 'Toutes catégories' }) },
                  ...categoryOptions,
                ]}
                onChange={v => setCategoryFilter(v as 'all' | StakeholderCategory)}
              />
              {selection && (
                <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-2 pl-3 pr-1 py-1 text-sm text-text-primary">
                  {selectionLabel}
                  <button
                    onClick={() => setSelection(null)}
                    title={t('sh_filter_clear', { defaultValue: 'Retirer ce filtre' })}
                    className="p-1 rounded-full text-text-tertiary hover:text-danger hover:bg-surface-3"
                  >
                    <X size={13} />
                  </button>
                </span>
              )}
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
                  title={t('sh_no_results', { defaultValue: 'Aucune partie prenante ne correspond' })}
                  description={t('sh_no_results_desc', { defaultValue: 'Les filtres actifs excluent tout le registre.' })}
                  action={{ label: t('common_clear_filters', { defaultValue: 'Effacer les filtres' }), onClick: clearFilters }}
                  t={t}
                />
              </div>
            ) : (
              // A wide register scrolls INSIDE its own box, never past the page edge.
              <div className="bg-surface-0 border border-border rounded-xl overflow-x-auto">
                <table className="w-full border-collapse" style={{ minWidth: minTableWidth }}>
                  <thead>
                    <tr>
                      <th className={th} style={{ width: isMobile ? 200 : 240 }}>
                        {t('sh_col_name', { defaultValue: 'Nom' })}
                      </th>
                      {!isMobile && <th className={th} style={{ width: 170 }}>{t('sh_col_organisation', { defaultValue: 'Organisation' })}</th>}
                      {!isMobile && <th className={th} style={{ width: 170 }}>{t('sh_col_role', { defaultValue: 'Rôle' })}</th>}
                      {!isMobile && <th className={th} style={{ width: 150 }}>{t('sh_col_category', { defaultValue: 'Catégorie' })}</th>}
                      <th className={th} style={{ width: isMobile ? 104 : 118 }}>{t('sh_col_power', { defaultValue: 'Pouvoir' })}</th>
                      <th className={th} style={{ width: isMobile ? 104 : 118 }}>{t('sh_col_interest', { defaultValue: 'Intérêt' })}</th>
                      {!isMobile && <th className={th} style={{ width: 150 }}>{t('sh_col_quadrant', { defaultValue: 'À faire' })}</th>}
                      {!isMobile && <th className={th} style={{ width: 310 }}>{t('sh_col_engagement', { defaultValue: 'Engagement : actuel → souhaité' })}</th>}
                      <th className={th} style={{ width: isMobile ? 52 : 56 }} />
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map(holder => (
                      <StakeholderRow key={holder.id} holder={holder} ctx={ctx} />
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
