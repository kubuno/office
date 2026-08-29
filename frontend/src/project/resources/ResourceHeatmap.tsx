import { useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  addDays, addMonths, differenceInCalendarDays, format,
  getISOWeek, startOfMonth, startOfWeek,
} from 'date-fns'
import { AlertTriangle, ChevronLeft, ChevronRight, Users } from 'lucide-react'
import { Button, EmptyState, Tooltip } from '@ui'
import type { ProjectResource, ProjectTask, TaskAssignment } from '../../api'
import type { ResourceViewProps } from './types'
import { behaviorOf } from './types'

// Resource utilisation heatmap — the flagship resource-management view (à la
// Resource Guru / Float / Runn). Rows are resources, columns are time buckets,
// each cell is a utilisation percentage (allocation ÷ capacity) painted on a
// semantic gradient: neutral → green → amber → red. It answers, at a glance,
// "who is free, who is booked solid, and who is overbooked?".

// ── Scales ──────────────────────────────────────────────────────────────────
type Scale = 'day' | 'week' | 'month'
const COL_WIDTH: Record<Scale, number> = { day: 44, week: 52, month: 64 }
const LEFT_WIDTH = 224
// A working day is assumed to be eight hours; used only for the "h/day" hint,
// never for the utilisation maths (that is a pure allocation ÷ capacity ratio).
const HOURS_PER_DAY = 8

// ── Semantic gradient ─────────────────────────────────────────────────────────
// These greens/ambers/reds are fixed status colours, not theme tokens: a
// utilisation scale means the same thing on every skin, light or dark.
const PALE_GREEN = '#e6f4ea'
const GREEN = '#34a853'
const AMBER = '#e37400'
const RED = '#d93025'
const RED_DARK = '#a50e0e'

type Rgb = [number, number, number]
function hexToRgb(hex: string): Rgb {
  const n = parseInt(hex.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}
function mix(a: string, b: string, t: number): Rgb {
  const ca = hexToRgb(a)
  const cb = hexToRgb(b)
  const k = Math.max(0, Math.min(1, t))
  return [
    Math.round(ca[0] + (cb[0] - ca[0]) * k),
    Math.round(ca[1] + (cb[1] - ca[1]) * k),
    Math.round(ca[2] + (cb[2] - ca[2]) * k),
  ]
}
const rgbCss = (c: Rgb) => `rgb(${c[0]}, ${c[1]}, ${c[2]})`
// Perceived luminance, to keep the centred percentage readable on any fill.
const luminance = (c: Rgb) => (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]) / 255

interface CellPaint { zero: boolean; bg: string; fg: string; overload: boolean }
function paintFor(ratio: number): CellPaint {
  if (ratio <= 0.0001) return { zero: true, bg: '', fg: '', overload: false }
  let rgb: Rgb
  if (ratio < 0.8) rgb = mix(PALE_GREEN, GREEN, ratio / 0.8)          // free → busy
  else if (ratio <= 1.0) rgb = mix(GREEN, AMBER, (ratio - 0.8) / 0.2)  // busy → full
  else rgb = mix(RED, RED_DARK, Math.min(1, ratio - 1))               // overbooked
  const fg = luminance(rgb) < 0.6 ? '#ffffff' : '#202124'
  return { zero: false, bg: rgbCss(rgb), fg, overload: ratio > 1.0001 }
}

// ── Column model ───────────────────────────────────────────────────────────
interface Column {
  key: string
  /** Day offsets from projectStart (may fall outside the span at the edges). */
  start: number
  end: number
  /** In-span working-day offsets aggregated for this column. */
  days: number[]
  groupKey: string
  groupLabel: string
  subLabel: string
  isToday: boolean
}

const isWeekday = (d: Date) => {
  const g = d.getDay()
  return g !== 0 && g !== 6
}

/** Days in [start,end) that fall inside the project span. Working days only,
 *  unless that leaves nothing (a bucket that is all weekend), then all days. */
function bucketDays(start: number, end: number, totalDays: number, projectStart: Date): number[] {
  const from = Math.max(0, start)
  const to = Math.min(totalDays, end)
  const work: number[] = []
  const all: number[] = []
  for (let d = from; d < to; d++) {
    all.push(d)
    if (isWeekday(addDays(projectStart, d))) work.push(d)
  }
  return work.length ? work : all
}

function buildColumns(scale: Scale, projectStart: Date, totalDays: number, todayOffset: number, locale: import('date-fns').Locale): Column[] {
  const cols: Column[] = []
  const inToday = (start: number, end: number) => todayOffset >= start && todayOffset < end

  if (scale === 'day') {
    for (let d = 0; d < totalDays; d++) {
      const date = addDays(projectStart, d)
      cols.push({
        key: `d${d}`,
        start: d, end: d + 1, days: [d],
        groupKey: `${date.getFullYear()}-${date.getMonth()}`,
        groupLabel: format(date, 'MMMM yyyy', { locale }),
        subLabel: `${format(date, 'EEEEEE', { locale })} ${date.getDate()}`,
        isToday: inToday(d, d + 1),
      })
    }
    return cols
  }

  if (scale === 'week') {
    let d = 0
    while (d < totalDays) {
      const date = addDays(projectStart, d)
      const wStart = startOfWeek(date, { weekStartsOn: 1 })
      const start = differenceInCalendarDays(wStart, projectStart)
      const end = start + 7
      cols.push({
        key: `w${start}`,
        start, end, days: bucketDays(start, end, totalDays, projectStart),
        groupKey: `${wStart.getFullYear()}-${wStart.getMonth()}`,
        groupLabel: format(wStart, 'MMMM yyyy', { locale }),
        subLabel: `S${getISOWeek(date)}`,
        isToday: inToday(start, end),
      })
      d = end // start ≤ d < start+7, so this always advances
    }
    return cols
  }

  // month
  let d = 0
  while (d < totalDays) {
    const date = addDays(projectStart, d)
    const mStart = startOfMonth(date)
    const nStart = startOfMonth(addMonths(date, 1))
    const start = differenceInCalendarDays(mStart, projectStart)
    const end = differenceInCalendarDays(nStart, projectStart)
    cols.push({
      key: `m${start}`,
      start, end, days: bucketDays(start, end, totalDays, projectStart),
      groupKey: `${mStart.getFullYear()}`,
      groupLabel: format(mStart, 'yyyy', { locale }),
      subLabel: format(mStart, 'MMM', { locale }),
      isToday: inToday(start, end),
    })
    d = end
  }
  return cols
}

/** Consecutive columns sharing a groupKey — the top (month/year) header band. */
function groupSpans(cols: Column[]): Array<{ key: string; label: string; span: number }> {
  const out: Array<{ key: string; label: string; span: number }> = []
  for (const c of cols) {
    const last = out[out.length - 1]
    if (last && last.key === c.groupKey) last.span += 1
    else out.push({ key: c.groupKey, label: c.groupLabel, span: 1 })
  }
  return out
}

// ── Utilisation maths ─────────────────────────────────────────────────────────
/** Per-resource daily ratio (allocation ÷ capacity), indexed by day offset. */
function dailyRatios(
  resources: ProjectResource[], tasks: ProjectTask[], assignments: TaskAssignment[], totalDays: number,
): Map<string, Float64Array> {
  const span = Math.max(1, totalDays)
  const taskById = new Map(tasks.map(t => [t.id, t]))
  // Absolute allocation (in capacity units) per resource per day.
  const alloc = new Map<string, Float64Array>()
  for (const a of assignments) {
    const t = taskById.get(a.task_id)
    if (!t || t.task_type === 'summary') continue
    const es = t.early_start ?? 0
    const end = Math.min(span, es + Math.max(1, t.duration_days))
    let arr = alloc.get(a.resource_id)
    if (!arr) { arr = new Float64Array(span); alloc.set(a.resource_id, arr) }
    for (let d = Math.max(0, es); d < end; d++) arr[d] += a.units ?? 1
  }
  const ratios = new Map<string, Float64Array>()
  for (const r of resources) {
    const cap = r.capacity > 0 ? r.capacity : 1
    const a = alloc.get(r.id)
    const out = new Float64Array(span)
    if (a) for (let d = 0; d < span; d++) out[d] = a[d] / cap
    ratios.set(r.id, out)
  }
  return ratios
}

/** Tasks (non-summary) a resource is booked on, with their units — for the
 *  expanded per-task allocation bars. */
function tasksForResource(resourceId: string, tasks: ProjectTask[], assignments: TaskAssignment[]) {
  const byId = new Map(tasks.map(t => [t.id, t]))
  const out: Array<{ task: ProjectTask; units: number }> = []
  for (const a of assignments) {
    if (a.resource_id !== resourceId) continue
    const task = byId.get(a.task_id)
    if (!task || task.task_type === 'summary') continue
    out.push({ task, units: a.units ?? 1 })
  }
  return out.sort((x, y) => (x.task.early_start ?? 0) - (y.task.early_start ?? 0))
}

// ── Sub-components ─────────────────────────────────────────────────────────
function ResourceAvatar({ resource }: { resource: ProjectResource }) {
  return resource.avatar_url
    ? <img src={resource.avatar_url} alt="" className="w-8 h-8 rounded-full object-cover shrink-0" />
    : (
      <span
        className="w-8 h-8 rounded-full flex items-center justify-center text-white text-xs font-semibold shrink-0"
        style={{ background: resource.color }}
      >
        {resource.name[0]?.toUpperCase() ?? '?'}
      </span>
    )
}

// ── The view ───────────────────────────────────────────────────────────────
export default function ResourceHeatmap(props: ResourceViewProps) {
  const { resources, assignments, tasks, projectStart, totalDays, locale } = props
  const { t } = useTranslation('office')
  const [scale, setScale] = useState<Scale>('week')
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const scrollRef = useRef<HTMLDivElement>(null)
  const todayRef = useRef<HTMLDivElement>(null)

  // Cost resources carry no time load; material is skipped too, keeping the
  // heatmap to what actually consumes hours: people and equipment.
  const shown = useMemo(
    () => resources.filter(r => behaviorOf(r.kind) === 'work'),
    [resources],
  )

  const todayOffset = useMemo(
    () => differenceInCalendarDays(new Date(), projectStart),
    [projectStart],
  )
  const columns = useMemo(
    () => buildColumns(scale, projectStart, totalDays, todayOffset, locale),
    [scale, projectStart, totalDays, todayOffset, locale],
  )
  const groups = useMemo(() => groupSpans(columns), [columns])
  const ratios = useMemo(
    () => dailyRatios(shown, tasks, assignments, totalDays),
    [shown, tasks, assignments, totalDays],
  )

  const colW = COL_WIDTH[scale]
  const timeWidth = columns.length * colW

  const scrollToToday = () => {
    const el = todayRef.current
    if (el && typeof el.scrollIntoView === 'function') el.scrollIntoView({ inline: 'center', block: 'nearest' })
  }
  const nudge = (dir: -1 | 1) => {
    const el = scrollRef.current
    if (el) el.scrollBy({ left: dir * Math.round(el.clientWidth * 0.6), behavior: 'smooth' })
  }
  const toggleRow = (id: string) => setExpanded(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  const scaleOptions: Array<{ value: Scale; label: string }> = [
    { value: 'day', label: t('proj_heat_scale_day', { defaultValue: 'Jour' }) },
    { value: 'week', label: t('proj_heat_scale_week', { defaultValue: 'Semaine' }) },
    { value: 'month', label: t('proj_heat_scale_month', { defaultValue: 'Mois' }) },
  ]

  // Mean utilisation over a column's aggregated days, and the matching h/day.
  const cellValue = (rid: string, col: Column, capacity: number) => {
    const arr = ratios.get(rid)
    if (!arr || col.days.length === 0) return { ratio: 0, hoursPerDay: 0 }
    let sum = 0
    for (const d of col.days) sum += arr[d] ?? 0
    const ratio = sum / col.days.length
    return { ratio, hoursPerDay: ratio * capacity * HOURS_PER_DAY }
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        {/* Header: title, scale switch, today navigation */}
        <div className="flex items-center gap-3 flex-wrap">
          <Users size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_heat_title', { defaultValue: 'Charge des ressources' })}
          </h1>
          <div className="flex-1" />

          {/* Scale segmented control */}
          <div className="inline-flex items-center rounded-lg border border-border bg-surface-0 p-0.5">
            {scaleOptions.map(o => (
              <button
                key={o.value}
                type="button"
                onClick={() => setScale(o.value)}
                className={`px-3 h-7 text-sm rounded-md transition-colors ${
                  scale === o.value
                    ? 'bg-primary text-white'
                    : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                {o.label}
              </button>
            ))}
          </div>

          {/* Today navigation */}
          <div className="inline-flex items-center gap-1">
            <Button size="sm" variant="ghost" icon={<ChevronLeft size={16} />} aria-label={t('proj_heat_prev', { defaultValue: 'Précédent' })} onClick={() => nudge(-1)} />
            <Button size="sm" variant="secondary" onClick={scrollToToday}>
              {t('proj_heat_today', { defaultValue: "Aujourd'hui" })}
            </Button>
            <Button size="sm" variant="ghost" icon={<ChevronRight size={16} />} aria-label={t('proj_heat_next', { defaultValue: 'Suivant' })} onClick={() => nudge(1)} />
          </div>
        </div>

        {shown.length === 0 ? (
          <EmptyState
            variant="first-use"
            icon={<Users size={26} />}
            title={t('proj_heat_empty_title', { defaultValue: 'Aucune ressource planifiable' })}
            description={t('proj_heat_empty', { defaultValue: 'Ajoutez des personnes ou des équipements et affectez-les à des tâches pour voir leur charge.' })}
            t={t}
          />
        ) : (
          <div className="rounded-xl border border-border bg-surface-0 overflow-hidden">
            <div ref={scrollRef} className="overflow-x-auto">
              <div style={{ width: LEFT_WIDTH + timeWidth, minWidth: '100%' }}>

                {/* ── Sticky header (month band + sub-columns) ── */}
                <div className="sticky top-0 z-20 flex bg-surface-0 border-b border-border">
                  {/* Corner */}
                  <div
                    className="sticky left-0 z-10 bg-surface-0 border-r border-border flex items-end px-3 pb-1"
                    style={{ width: LEFT_WIDTH }}
                  >
                    <span className="text-xs font-medium text-text-secondary">
                      {t('proj_heat_resource', { defaultValue: 'Ressource' })}
                    </span>
                  </div>
                  <div style={{ width: timeWidth }}>
                    {/* Month / year band */}
                    <div className="flex h-6 border-b border-border">
                      {groups.map(g => (
                        <div
                          key={g.key}
                          className="flex items-center justify-center text-[11px] font-medium text-text-secondary border-r border-border truncate px-1"
                          style={{ width: g.span * colW }}
                        >
                          {g.label}
                        </div>
                      ))}
                    </div>
                    {/* Sub-columns */}
                    <div className="flex h-7">
                      {columns.map(c => (
                        <div
                          key={c.key}
                          ref={c.isToday ? todayRef : undefined}
                          className={`flex items-center justify-center text-[11px] border-r border-border truncate ${
                            c.isToday ? 'bg-primary/15 text-primary font-semibold' : 'text-text-tertiary'
                          }`}
                          style={{ width: colW }}
                        >
                          {c.subLabel}
                        </div>
                      ))}
                    </div>
                  </div>
                </div>

                {/* ── Resource rows ── */}
                {shown.map(r => {
                  const cap = r.capacity > 0 ? r.capacity : 1
                  const isOpen = expanded.has(r.id)
                  const bars = isOpen ? tasksForResource(r.id, tasks, assignments) : []
                  return (
                    <div key={r.id} className="border-b border-border last:border-b-0">
                      {/* Summary row */}
                      <div className="flex">
                        {/* Sticky resource label */}
                        <button
                          type="button"
                          onClick={() => toggleRow(r.id)}
                          className="sticky left-0 z-10 bg-surface-0 border-r border-border flex items-center gap-2.5 px-3 text-left hover:bg-surface-1 transition-colors"
                          style={{ width: LEFT_WIDTH, height: 52 }}
                        >
                          <ResourceAvatar resource={r} />
                          <span className="min-w-0">
                            <span className="block text-sm text-text-primary truncate">{r.name}</span>
                            {r.role && <span className="block text-[11px] text-text-tertiary truncate">{r.role}</span>}
                          </span>
                        </button>

                        {/* Utilisation cells */}
                        <div className="flex" style={{ height: 52 }}>
                          {columns.map(c => {
                            const { ratio, hoursPerDay } = cellValue(r.id, c, cap)
                            const paint = paintFor(ratio)
                            const pct = Math.round(ratio * 100)
                            const label = `${pct} % · ${hoursPerDay.toFixed(1)} h/j`
                            return (
                              <Tooltip key={c.key} label={label}>
                                <div
                                  className={`relative flex items-center justify-center border-r border-b border-border/60 text-xs tabular-nums ${
                                    c.isToday ? 'ring-1 ring-inset ring-primary/40' : ''
                                  }`}
                                  style={{
                                    width: colW,
                                    background: paint.zero ? 'var(--color-surface-1)' : paint.bg,
                                    color: paint.zero ? 'var(--color-text-tertiary)' : paint.fg,
                                  }}
                                >
                                  {paint.overload && (
                                    <AlertTriangle size={11} className="absolute top-0.5 right-0.5" strokeWidth={2.5} />
                                  )}
                                  {pct > 0 ? `${pct}%` : ''}
                                </div>
                              </Tooltip>
                            )
                          })}
                        </div>
                      </div>

                      {/* Expanded per-task allocation bars */}
                      {isOpen && (
                        <div className="flex bg-surface-1/60">
                          <div
                            className="sticky left-0 z-10 bg-surface-1 border-r border-border px-3 py-2"
                            style={{ width: LEFT_WIDTH }}
                          >
                            <span className="text-[11px] text-text-tertiary">
                              {t('proj_heat_tasks_count', { defaultValue: '{{n}} tâche(s)', n: bars.length })}
                            </span>
                          </div>
                          <div className="relative py-2" style={{ width: timeWidth }}>
                            {bars.length === 0 ? (
                              <div className="px-3 text-[11px] text-text-tertiary">
                                {t('proj_heat_no_tasks', { defaultValue: 'Aucune affectation.' })}
                              </div>
                            ) : (
                              <div className="space-y-1">
                                {bars.map(({ task, units }) => {
                                  const es = task.early_start ?? 0
                                  const dur = Math.max(1, task.duration_days)
                                  const left = (Math.max(0, es) / Math.max(1, totalDays)) * timeWidth
                                  const width = Math.max(6, (Math.min(dur, totalDays - Math.max(0, es)) / Math.max(1, totalDays)) * timeWidth)
                                  return (
                                    <div key={task.id} className="relative h-5">
                                      <Tooltip label={`${task.name} · ${Math.round(units * 100)} %`}>
                                        <div
                                          className="absolute top-0 h-5 rounded flex items-center px-1.5 text-[10px] text-white truncate"
                                          style={{ left, width, background: r.color }}
                                        >
                                          {task.name}
                                        </div>
                                      </Tooltip>
                                    </div>
                                  )
                                })}
                              </div>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            </div>
          </div>
        )}

        {/* Legend */}
        <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5 text-xs text-text-secondary">
          {[
            { c: 'var(--color-surface-1)', l: t('proj_heat_legend_free', { defaultValue: 'Libre (0 %)' }), border: true },
            { c: GREEN, l: t('proj_heat_legend_busy', { defaultValue: 'Chargé (< 80 %)' }) },
            { c: AMBER, l: t('proj_heat_legend_full', { defaultValue: 'Plein (80–100 %)' }) },
            { c: RED, l: t('proj_heat_legend_over', { defaultValue: 'Surcharge (> 100 %)' }) },
          ].map((s, i) => (
            <span key={i} className="inline-flex items-center gap-1.5">
              <span
                className={`w-3.5 h-3.5 rounded-sm ${s.border ? 'border border-border' : ''}`}
                style={{ background: s.c }}
              />
              {s.l}
            </span>
          ))}
          <span className="text-text-tertiary">
            {t('proj_heat_legend_note', { defaultValue: 'Utilisation = allocation ÷ capacité, moyenne des jours ouvrés de la période.' })}
          </span>
        </div>

      </div>
    </div>
  )
}
