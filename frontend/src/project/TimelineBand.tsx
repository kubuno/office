import { useTranslation } from 'react-i18next'
import { useRef, useEffect, useState } from 'react'
import { addDays, differenceInCalendarDays, format } from 'date-fns'
import { CalendarRange } from 'lucide-react'
import { CRITICAL_CLR, TASK_COLOR } from './GanttRenderer'
import type { ProjectTask } from '../api'

// Overview timeline band above the Gantt: summaries + milestones laid out across
// the whole project span. Rendered on a <canvas> (crisp on any DPI, theme-aware),
// with a band twice as tall as the previous DOM version so the items breathe.
const TRACK_H = 48
const FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif'

type Hit = { id: string; x: number; w: number }

export default function TimelineBand({ tasks, projectStart, totalDays, locale, onSelect, selectedId }: {
  tasks: ProjectTask[]; projectStart: Date; totalDays: number; locale: import('date-fns').Locale
  onSelect: (id: string) => void; selectedId: string | null
}) {
  const { t } = useTranslation('office')
  const wrapRef   = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const hitsRef   = useRef<Hit[]>([])
  const [width, setWidth] = useState(0)

  const end  = addDays(projectStart, totalDays)
  const span = Math.max(1, totalDays)
  // The overview shows only the big picture: summaries/parents + milestones.
  const items = tasks.filter(tk => tk.task_type !== 'task' || tk.parent_id == null)

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
    canvas.height = TRACK_H * dpr
    canvas.style.width  = `${width}px`
    canvas.style.height = `${TRACK_H}px`
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

    ctx.clearRect(0, 0, width, TRACK_H)
    // track
    ctx.fillStyle = bg
    ctx.beginPath(); ctx.roundRect(0.5, 0.5, width - 1, TRACK_H - 1, 6); ctx.fill()
    ctx.strokeStyle = border; ctx.lineWidth = 1; ctx.stroke()

    // start / end date labels
    ctx.fillStyle = dim; ctx.font = `10px ${FONT}`; ctx.textBaseline = 'middle'
    ctx.textAlign = 'left';  ctx.fillText(format(projectStart, 'd MMM yy', { locale }), 6, TRACK_H / 2)
    ctx.textAlign = 'right'; ctx.fillText(format(end, 'd MMM yy', { locale }), width - 6, TRACK_H / 2)
    ctx.textAlign = 'left'

    const barH = 22, barY = (TRACK_H - barH) / 2
    const hits: Hit[] = []
    for (const tk of items) {
      const off = tk.early_start ?? Math.max(0, differenceInCalendarDays(tk.start_date ? new Date(tk.start_date) : projectStart, projectStart))
      const x = (off / span) * width
      if (tk.task_type === 'milestone') {
        const cx = x, cy = TRACK_H / 2, s = 8
        ctx.fillStyle = mile
        ctx.beginPath(); ctx.moveTo(cx, cy - s); ctx.lineTo(cx + s, cy); ctx.lineTo(cx, cy + s); ctx.lineTo(cx - s, cy); ctx.closePath(); ctx.fill()
        if (selectedId === tk.id) { ctx.strokeStyle = prim; ctx.lineWidth = 2; ctx.stroke() }
        hits.push({ id: tk.id, x: cx - s - 2, w: 2 * s + 4 })
      } else {
        const w = Math.max(4, (tk.duration_days / span) * width)
        // Summaries/parents are the phase overview → charcoal, matching the Gantt
        // bracket. A genuine leaf task placed here still shows its critical colour.
        ctx.fillStyle = tk.task_type === 'summary' || tk.parent_id == null ? summ : (tk.is_critical ? crit : prim)
        ctx.beginPath(); ctx.roundRect(x, barY, w, barH, 5); ctx.fill()
        if (selectedId === tk.id) { ctx.strokeStyle = prim; ctx.lineWidth = 2; ctx.beginPath(); ctx.roundRect(x, barY, w, barH, 5); ctx.stroke() }
        ctx.save(); ctx.beginPath(); ctx.rect(x, barY, w, barH); ctx.clip()
        ctx.fillStyle = '#ffffff'; ctx.font = `11px ${FONT}`; ctx.textBaseline = 'middle'
        ctx.fillText(tk.name, x + 6, TRACK_H / 2 + 0.5)
        ctx.restore()
        hits.push({ id: tk.id, x, w })
      }
    }
    hitsRef.current = hits
  }, [width, items, projectStart, totalDays, span, selectedId, locale])

  const onClick = (e: React.MouseEvent) => {
    const canvas = canvasRef.current
    if (!canvas) return
    const x = e.clientX - canvas.getBoundingClientRect().left
    // last drawn (topmost) hit wins
    for (let i = hitsRef.current.length - 1; i >= 0; i--) {
      const h = hitsRef.current[i]
      if (x >= h.x && x <= h.x + h.w) { onSelect(h.id); return }
    }
  }

  return (
    <div className="shrink-0 border-b border-border bg-surface-1 px-4 py-2">
      <div className="flex items-center gap-2 mb-1 text-[10px] font-semibold text-text-tertiary uppercase tracking-wide">
        <CalendarRange size={12} /> {t('proj_timeline', { defaultValue: 'Chronologie' })}
      </div>
      <div ref={wrapRef} style={{ height: TRACK_H }}>
        <canvas ref={canvasRef} onClick={onClick} className="cursor-pointer block" />
      </div>
    </div>
  )
}
