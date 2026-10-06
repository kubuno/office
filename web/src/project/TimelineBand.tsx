import { useTranslation } from 'react-i18next'
import { useRef, useEffect, useState, useMemo } from 'react'
import { addDays, differenceInDays, formatDate } from '@kubuno/sdk'
import { CalendarRange } from 'lucide-react'
import { CRITICAL_CLR, TASK_COLOR } from './GanttRenderer'
import type { ProjectTask } from '../api'

// Overview timeline band above the Gantt: the top-level phases + every milestone
// laid out across the whole project span. Rendered on a <canvas> (crisp on any
// DPI, theme-aware). Items that overlap in time are stacked on separate lanes
// (greedy interval packing) so nothing is ever drawn over something else; the
// band grows with the number of lanes it needs.
const LANE_H = 20
const LANE_GAP = 3
const PAD_Y = 6
const MAX_LANES = 6
const MIN_TRACK_H = 48
const FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif'
/** Room kept on each side for the start/end date labels (never under a bar). */
const DATE_GUTTER = 64

type Hit = { id: string; x: number; w: number; lane: number }
type Item = { tk: ProjectTask; off: number; x: number; w: number; lane: number; milestone: boolean }

/** Pixel layout of the overview: x/w per item and the lane each one lands on. */
function layoutItems(tasks: ProjectTask[], projectStart: Date, span: number, innerW: number): { items: Item[]; lanes: number } {
  // Only the big picture: top-level items (the phases) and milestones at any depth.
  // Nested summaries subdivide their phase — they would always overlap it.
  const picked = tasks.filter(tk => tk.parent_id == null || tk.task_type === 'milestone')
  const placed: Item[] = []
  for (const tk of picked) {
    const off = tk.early_start ?? Math.max(0, differenceInDays(tk.start_date ? new Date(tk.start_date) : projectStart, projectStart))
    const milestone = tk.task_type === 'milestone'
    // Bars follow the CALENDAR span (EF−ES), like the Gantt — working-day
    // durations would fall short of the bars they are meant to summarise.
    const calDays = milestone ? 0 : Math.max(0, (tk.early_finish ?? off + tk.duration_days) - off)
    const x = (off / span) * innerW
    const w = milestone ? 20 : Math.max(4, (calDays / span) * innerW)
    placed.push({ tk, off, x: milestone ? x - 10 : x, w, lane: 0, milestone })
  }
  // Greedy packing: earliest day first, the widest of a same-day tie first (so
  // the phases take the upper lanes and the milestones tuck in below them), each
  // item on the first lane that is free at its x.
  placed.sort((a, b) => a.off - b.off || b.w - a.w)
  const laneEnd: number[] = []
  for (const it of placed) {
    let lane = laneEnd.findIndex(end => end <= it.x - 2)
    if (lane === -1) {
      if (laneEnd.length < MAX_LANES) { lane = laneEnd.length; laneEnd.push(0) }
      // Out of lanes: share the one that frees up first (rare; still readable).
      else lane = laneEnd.indexOf(Math.min(...laneEnd))
    }
    it.lane = lane
    laneEnd[lane] = it.x + it.w
  }
  return { items: placed, lanes: Math.max(1, laneEnd.length) }
}

/** Fit `text` in `maxW` px, ellipsised; empty when even "…" would not fit. */
function fitText(ctx: CanvasRenderingContext2D, text: string, maxW: number): string {
  if (maxW < 8) return ''
  if (ctx.measureText(text).width <= maxW) return text
  let lo = 0, hi = text.length
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (ctx.measureText(text.slice(0, mid) + '…').width <= maxW) lo = mid
    else hi = mid - 1
  }
  return lo === 0 ? '' : text.slice(0, lo) + '…'
}

export default function TimelineBand({ tasks, projectStart, totalDays, onSelect, selectedId }: {
  tasks: ProjectTask[]; projectStart: Date; totalDays: number
  /** Kept for call-site compatibility; date formatting now uses the SDK's locale-aware helpers. */
  locale?: unknown
  onSelect: (id: string) => void; selectedId: string | null
}) {
  const { t } = useTranslation('office')
  const wrapRef   = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const hitsRef   = useRef<Hit[]>([])
  const [width, setWidth] = useState(0)

  const end  = addDays(projectStart, totalDays)
  const span = Math.max(1, totalDays)
  const innerW = Math.max(0, width - 2 * DATE_GUTTER)

  const layout = useMemo(() => layoutItems(tasks, projectStart, span, innerW), [tasks, projectStart, span, innerW])
  const trackH = Math.max(MIN_TRACK_H, 2 * PAD_Y + layout.lanes * LANE_H + (layout.lanes - 1) * LANE_GAP)
  // Lanes are centred in the band (a single lane sits in the middle, as before).
  const lanesTop = (trackH - (layout.lanes * LANE_H + (layout.lanes - 1) * LANE_GAP)) / 2
  const laneY = (lane: number) => lanesTop + lane * (LANE_H + LANE_GAP)

  // Track the band's rendered width so the canvas is pixel-exact and reflows.
  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(entries => setWidth(Math.floor(entries[0].contentRect.width)))
    ro.observe(el)
    setWidth(el.clientWidth)
    return () => ro.disconnect()
  }, [])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || width <= 0) return
    const dpr = window.devicePixelRatio || 1
    canvas.width  = width * dpr
    canvas.height = trackH * dpr
    canvas.style.width  = `${width}px`
    canvas.style.height = `${trackH}px`
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    const cs = getComputedStyle(canvas)
    const pick = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback
    const bg     = pick('--color-surface-0', '#ffffff')
    const border = pick('--color-border', '#e8eaed')
    const dim    = pick('--color-text-secondary', '#5f6368')
    const prim   = pick('--color-primary', TASK_COLOR)
    const crit   = pick('--color-danger', CRITICAL_CLR)
    const mile   = pick('--color-warning', '#f97316')
    const summ   = '#666666'

    ctx.clearRect(0, 0, width, trackH)
    // track
    ctx.fillStyle = bg
    ctx.beginPath(); ctx.roundRect(0.5, 0.5, width - 1, trackH - 1, 6); ctx.fill()
    ctx.strokeStyle = border; ctx.lineWidth = 1; ctx.stroke()

    // start / end date labels, in their own gutters
    ctx.fillStyle = dim; ctx.font = `10px ${FONT}`; ctx.textBaseline = 'middle'
    ctx.textAlign = 'left';  ctx.fillText(formatDate(projectStart, { day: 'numeric', month: 'short', year: '2-digit' }), 6, trackH / 2)
    ctx.textAlign = 'right'; ctx.fillText(formatDate(end, { day: 'numeric', month: 'short', year: '2-digit' }), width - 6, trackH / 2)
    ctx.textAlign = 'left'

    const hits: Hit[] = []
    for (const it of layout.items) {
      const { tk } = it
      const x = DATE_GUTTER + it.x
      const y = laneY(it.lane)
      if (it.milestone) {
        const cx = x + 10, cy = y + LANE_H / 2, s = 8
        ctx.fillStyle = mile
        ctx.beginPath(); ctx.moveTo(cx, cy - s); ctx.lineTo(cx + s, cy); ctx.lineTo(cx, cy + s); ctx.lineTo(cx - s, cy); ctx.closePath(); ctx.fill()
        if (selectedId === tk.id) { ctx.strokeStyle = prim; ctx.lineWidth = 2; ctx.stroke() }
        hits.push({ id: tk.id, x: cx - s - 2, w: 2 * s + 4, lane: it.lane })
      } else {
        // Top-level items are the phase overview → charcoal, matching the Gantt
        // bracket. A genuine leaf task placed here still shows its critical colour.
        ctx.fillStyle = tk.task_type === 'summary' || tk.parent_id == null ? summ : (tk.is_critical ? crit : prim)
        ctx.beginPath(); ctx.roundRect(x, y, it.w, LANE_H, 5); ctx.fill()
        if (selectedId === tk.id) { ctx.strokeStyle = prim; ctx.lineWidth = 2; ctx.beginPath(); ctx.roundRect(x, y, it.w, LANE_H, 5); ctx.stroke() }
        ctx.fillStyle = '#ffffff'; ctx.font = `11px ${FONT}`; ctx.textBaseline = 'middle'
        const label = fitText(ctx, tk.name, it.w - 12)
        if (label) ctx.fillText(label, x + 6, y + LANE_H / 2 + 0.5)
        hits.push({ id: tk.id, x, w: it.w, lane: it.lane })
      }
    }
    hitsRef.current = hits
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [width, layout, trackH, lanesTop, projectStart, totalDays, selectedId])

  const onClick = (e: React.MouseEvent) => {
    const canvas = canvasRef.current
    if (!canvas) return
    const r = canvas.getBoundingClientRect()
    const x = e.clientX - r.left, y = e.clientY - r.top
    // last drawn (topmost) hit wins
    for (let i = hitsRef.current.length - 1; i >= 0; i--) {
      const h = hitsRef.current[i]
      const top = laneY(h.lane)
      if (x >= h.x && x <= h.x + h.w && y >= top && y <= top + LANE_H) { onSelect(h.id); return }
    }
  }

  return (
    <div className="shrink-0 border-b border-border bg-surface-1 px-4 py-2">
      <div className="flex items-center gap-2 mb-1 text-[10px] font-semibold text-text-tertiary uppercase tracking-wide">
        <CalendarRange size={12} /> {t('proj_timeline', { defaultValue: 'Chronologie' })}
      </div>
      <div ref={wrapRef} style={{ height: trackH }}>
        <canvas ref={canvasRef} onClick={onClick} className="cursor-pointer block" />
      </div>
    </div>
  )
}
