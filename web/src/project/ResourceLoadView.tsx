import { useMemo, useRef, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { addDays, formatDate } from '@kubuno/sdk'
import { GRID_CLR } from './GanttRenderer'
import type { ProjectTask, ProjectResource } from '../api'

const LOAD_ROW = 56

// Day-by-day workload histogram per resource (capacity line + over-allocation in red).
export default function ResourceLoadView({ tasks, resources, assignments, projectStart, totalDays, dayW }: {
  tasks: ProjectTask[]; resources: ProjectResource[]
  assignments: { task_id: string; resource_id: string; units: number }[]
  projectStart: Date; totalDays: number; dayW: number
  /** No longer used (formatDate resolves the locale itself); kept until callers stop passing it. */
  locale?: unknown
}) {
  const { t } = useTranslation('office')
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const TOP = 24

  const alloc = useMemo(() => {
    const byId = new Map(tasks.map(tk => [tk.id, tk]))
    const m = new Map<string, number[]>(resources.map(r => [r.id, new Array(totalDays).fill(0)]))
    for (const a of assignments) {
      const tk = byId.get(a.task_id); const arr = m.get(a.resource_id)
      if (!tk || !arr || tk.task_type === 'summary') continue
      // Calendar span (EF−ES), aligned with the Gantt bars.
      const s = tk.early_start ?? 0, e = tk.early_finish ?? (s + tk.duration_days)
      for (let d = Math.max(0, s); d < Math.min(totalDays, e); d++) arr[d] += a.units
    }
    return m
  }, [tasks, resources, assignments, totalDays])

  useEffect(() => {
    const c = canvasRef.current; if (!c) return
    const dpr = window.devicePixelRatio || 1
    const W = totalDays * dayW, H = TOP + resources.length * LOAD_ROW
    c.width = W * dpr; c.height = H * dpr; c.style.width = `${W}px`; c.style.height = `${H}px`
    const ctx = c.getContext('2d')!; ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, W, H)
    // weekend shading + grid
    for (let d = 0; d <= totalDays; d++) {
      const x = d * dayW
      const dow = addDays(projectStart, d).getDay()
      if (dow === 0 || dow === 6) { ctx.fillStyle = '#f8f9fa'; ctx.fillRect(x, TOP, dayW, H - TOP) }
      if (dow === 1) { ctx.strokeStyle = GRID_CLR; ctx.lineWidth = 0.5; ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, H); ctx.stroke() }
    }
    // month labels
    let cur = -1
    for (let d = 0; d <= totalDays; d++) {
      const date = addDays(projectStart, d)
      if (date.getMonth() !== cur) { cur = date.getMonth(); ctx.fillStyle = '#5f6368'; ctx.font = 'bold 10px Outfit, sans-serif'; ctx.textAlign = 'left'; ctx.fillText(formatDate(date, { month: 'short', year: '2-digit' }), d * dayW + 3, 14) }
    }
    // per-resource histogram
    resources.forEach((r, i) => {
      const arr = alloc.get(r.id) ?? []
      const cap = r.capacity || 1
      const peak = Math.max(cap, ...arr)
      const base = TOP + i * LOAD_ROW + LOAD_ROW - 8
      const maxH = LOAD_ROW - 18
      // separator
      ctx.strokeStyle = '#e8eaed'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(0, TOP + i * LOAD_ROW); ctx.lineTo(W, TOP + i * LOAD_ROW); ctx.stroke()
      // capacity line
      const capY = base - (cap / peak) * maxH
      ctx.strokeStyle = '#9aa0a6'; ctx.setLineDash([4, 3]); ctx.beginPath(); ctx.moveTo(0, capY); ctx.lineTo(W, capY); ctx.stroke(); ctx.setLineDash([])
      // bars
      for (let d = 0; d < totalDays; d++) {
        const load = arr[d]; if (load <= 0) continue
        const h = (load / peak) * maxH
        ctx.fillStyle = load > cap ? '#d93025cc' : '#1a73e8aa'
        ctx.fillRect(d * dayW + 0.5, base - h, Math.max(1, dayW - 1), h)
      }
    })
  }, [alloc, resources, projectStart, totalDays, dayW])

  if (resources.length === 0) return <div className="flex-1 flex items-center justify-center text-sm text-text-tertiary italic">{t('proj_no_resources_hint')}</div>

  return (
    <div className="flex-1 flex overflow-hidden">
      <div className="shrink-0 w-44 border-r border-border bg-surface-1">
        <div style={{ height: 24 }} className="border-b border-border" />
        {resources.map(r => (
          <div key={r.id} className="flex items-center gap-2 px-2 border-b border-[#f1f3f4]" style={{ height: LOAD_ROW }}>
            <div className="w-6 h-6 rounded-full flex items-center justify-center text-white text-[10px] font-bold flex-shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase()}</div>
            <div className="min-w-0"><p className="text-xs font-medium text-text-primary truncate">{r.name}</p><p className="text-[10px] text-text-tertiary">{Math.round(r.capacity * 100)}%</p></div>
          </div>
        ))}
      </div>
      <div className="flex-1 overflow-auto"><canvas ref={canvasRef} className="block" /></div>
    </div>
  )
}
