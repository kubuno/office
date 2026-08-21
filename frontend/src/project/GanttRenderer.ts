import { addDays, format } from 'date-fns'
import type { ProjectTask, TaskDependency } from '../api'

export const ROW_H    = 28
export const HEADER_H = 56
export const MIN_DAYS = 60
export const TIMELINE_H = 60

export const TASK_COLOR   = '#1a73e8'
export const CRITICAL_CLR = '#d93025'
export const MILESTONE_CLR = '#ea4335'
export const SUMMARY_CLR  = '#5f6368'
export const GRID_CLR     = '#e8eaed'
export const PROGRESS_CLR = '#34a853'

// Zoom : pixels par jour selon l'échelle de temps.
export type ZoomLevel = 'day' | 'week' | 'month'
export const ZOOM_DAYW: Record<ZoomLevel, number> = { day: 26, week: 9, month: 3.2 }

export class GanttRenderer {
  readonly el:    HTMLCanvasElement
  private canvas: HTMLCanvasElement
  private ctx:    CanvasRenderingContext2D
  private dpr:    number

  constructor(canvas: HTMLCanvasElement) {
    this.el = canvas
    this.canvas = canvas
    this.ctx = canvas.getContext('2d')!
    this.dpr = window.devicePixelRatio || 1
  }

  resize(w: number, h: number) {
    this.canvas.width  = w * this.dpr
    this.canvas.height = h * this.dpr
    this.canvas.style.width  = `${w}px`
    this.canvas.style.height = `${h}px`
    this.ctx.setTransform(1, 0, 0, 1, 0, 0)
    this.ctx.scale(this.dpr, this.dpr)
  }

  render(
    tasks:        ProjectTask[],
    deps:         TaskDependency[],
    projectStart: Date,
    totalDays:    number,
    scrollLeft:   number,
    viewportW:    number,
    locale:       import('date-fns').Locale,
    dayW:         number,
    preview?:     { taskId: string; start: number; dur: number } | null,
    linkPreview?: { x1: number; y1: number; x2: number; y2: number } | null,
    baseline?:    Map<string, { es: number; dur: number }> | null,
  ) {
    const ctx = this.ctx
    const w   = viewportW
    const h   = HEADER_H + tasks.length * ROW_H

    ctx.clearRect(0, 0, w, h)
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(0, 0, w, h)

    const startDay = Math.floor(scrollLeft / dayW)
    const endDay   = Math.min(totalDays, startDay + Math.ceil(viewportW / dayW) + 1)
    const showDays = dayW >= 12   // n'afficher les numéros de jour qu'en échelle « jour »
    const stepW    = dayW         // largeur d'une colonne jour

    // ── weekends + grille ──
    for (let d = startDay; d <= endDay; d++) {
      const x = d * dayW - scrollLeft
      const date = addDays(projectStart, d)
      const dow  = date.getDay()
      if (showDays && (dow === 0 || dow === 6)) {
        ctx.fillStyle = '#f8f9fa'
        ctx.fillRect(x, HEADER_H, stepW, tasks.length * ROW_H)
      }
      // lignes verticales : chaque jour en échelle jour, sinon chaque lundi
      if (showDays || dow === 1) {
        ctx.strokeStyle = GRID_CLR
        ctx.lineWidth   = 0.5
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke()
      }
    }

    // ── fonds de lignes alternés ──
    tasks.forEach((_, i) => {
      if (i % 2 === 1) {
        ctx.fillStyle = '#fafafa'
        ctx.fillRect(0, HEADER_H + i * ROW_H, w, ROW_H)
      }
    })

    // ── header (mois + jours/semaines) ──
    ctx.fillStyle = '#f1f3f4'
    ctx.fillRect(0, 0, w, HEADER_H)
    ctx.strokeStyle = GRID_CLR; ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(0, HEADER_H); ctx.lineTo(w, HEADER_H); ctx.stroke()

    let curMonth = -1
    for (let d = startDay; d <= endDay; d++) {
      const date = addDays(projectStart, d)
      if (date.getMonth() !== curMonth) {
        curMonth = date.getMonth()
        const x = d * dayW - scrollLeft
        ctx.fillStyle = '#202124'
        ctx.font = 'bold 11px Google Sans, sans-serif'
        ctx.textAlign = 'left'
        ctx.fillText(format(date, 'MMM yyyy', { locale }), x + 4, 18)
      }
    }

    if (showDays) {
      for (let d = startDay; d <= endDay; d++) {
        const date = addDays(projectStart, d)
        const x    = d * dayW - scrollLeft + dayW / 2
        const dow  = date.getDay()
        ctx.fillStyle = dow === 0 || dow === 6 ? '#80868b' : '#5f6368'
        ctx.font = '9px Google Sans, sans-serif'
        ctx.textAlign = 'center'
        ctx.fillText(String(date.getDate()), x, 36)
      }
    } else {
      // marqueurs hebdomadaires (lundis)
      for (let d = startDay; d <= endDay; d++) {
        const date = addDays(projectStart, d)
        if (date.getDay() !== 1) continue
        const x = d * dayW - scrollLeft
        ctx.fillStyle = '#5f6368'
        ctx.font = '9px Google Sans, sans-serif'
        ctx.textAlign = 'left'
        ctx.fillText(format(date, 'd', { locale }), x + 2, 36)
      }
    }
    ctx.textAlign = 'left'

    // ── barres de tâches ──
    tasks.forEach((task, i) => {
      const y = HEADER_H + i * ROW_H
      const ov = preview && preview.taskId === task.id ? preview : null
      const startOffset = ov ? ov.start : (task.early_start ?? 0)
      const dur = ov ? ov.dur : task.duration_days
      const x   = startOffset * dayW - scrollLeft
      const bw  = Math.max(dur * dayW, 4)
      if (x + bw < 0 || x > viewportW) return

      if (task.task_type === 'milestone') {
        const cx = x, cy = y + ROW_H / 2, s = 7
        ctx.fillStyle = task.is_critical ? CRITICAL_CLR : MILESTONE_CLR
        ctx.beginPath()
        ctx.moveTo(cx, cy - s); ctx.lineTo(cx + s, cy); ctx.lineTo(cx, cy + s); ctx.lineTo(cx - s, cy)
        ctx.closePath(); ctx.fill()
        return
      }

      const color = task.task_type === 'summary' ? SUMMARY_CLR : task.is_critical ? CRITICAL_CLR : TASK_COLOR
      const barH  = ROW_H - 10
      const barY  = y + (ROW_H - barH) / 2

      // ── barre fantôme du plan de référence (prévu), sous la barre courante ──
      // Le décalage prévu → réel se lit d'un coup d'œil : rouge = glissé plus tard,
      // vert = en avance, gris = à l'heure. (Ce que Google/OpenProject ne montrent pas.)
      const bl = baseline?.get(task.id)
      if (bl) {
        const bx  = bl.es * dayW - scrollLeft
        const bbw = Math.max(bl.dur * dayW, 4)
        const slip = startOffset - bl.es
        const gclr = slip > 0 ? CRITICAL_CLR : slip < 0 ? '#1e8e3e' : '#9aa0a6'
        ctx.fillStyle = gclr + 'cc'
        ctx.beginPath(); ctx.roundRect(bx, barY + barH - 1, bbw, 4, 2); ctx.fill()
      }

      ctx.fillStyle = color + '33'
      ctx.beginPath(); ctx.roundRect(x, barY, bw, barH, 3); ctx.fill()

      if (task.progress > 0) {
        const pw = bw * (task.progress / 100)
        ctx.fillStyle = task.is_critical ? CRITICAL_CLR + '88' : PROGRESS_CLR + '88'
        ctx.beginPath(); ctx.roundRect(x, barY, pw, barH, 3); ctx.fill()
      }

      ctx.strokeStyle = color; ctx.lineWidth = 1.5
      ctx.beginPath(); ctx.roundRect(x, barY, bw, barH, 3); ctx.stroke()

      // libellé : nom + ressources à droite de la barre
      ctx.fillStyle = '#5f6368'
      ctx.font = '10px Google Sans, sans-serif'
      if (bw > 40) {
        ctx.fillStyle = '#202124'
        ctx.fillText(task.name, x + 6, barY + barH / 2 + 4)
      }

      // pastille de liaison (glisser vers une autre barre = créer une dépendance)
      if (task.task_type !== 'summary') {
        ctx.fillStyle = '#ffffff'; ctx.strokeStyle = color; ctx.lineWidth = 1.5
        ctx.beginPath(); ctx.arc(x + bw + 6, barY + barH / 2, 3.5, 0, Math.PI * 2); ctx.fill(); ctx.stroke()
      }
    })

    // ── flèches de dépendance ──
    deps.forEach(dep => {
      const fromIdx = tasks.findIndex(t => t.id === dep.from_task_id)
      const toIdx   = tasks.findIndex(t => t.id === dep.to_task_id)
      if (fromIdx < 0 || toIdx < 0) return
      const fromTask = tasks[fromIdx], toTask = tasks[toIdx]
      const fromX = (fromTask.early_start ?? 0) * dayW + fromTask.duration_days * dayW - scrollLeft
      const fromY = HEADER_H + fromIdx * ROW_H + ROW_H / 2
      const toX   = (toTask.early_start ?? 0) * dayW - scrollLeft
      const toY   = HEADER_H + toIdx * ROW_H + ROW_H / 2

      ctx.strokeStyle = '#9aa0a6'; ctx.lineWidth = 1; ctx.setLineDash([3, 2])
      ctx.beginPath(); ctx.moveTo(fromX, fromY)
      ctx.bezierCurveTo(fromX + 20, fromY, toX - 20, toY, toX, toY); ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = '#9aa0a6'
      ctx.beginPath(); ctx.moveTo(toX, toY); ctx.lineTo(toX - 6, toY - 3); ctx.lineTo(toX - 6, toY + 3)
      ctx.closePath(); ctx.fill()
    })

    // ── ligne « aujourd'hui » ──
    const today = new Date()
    const todayOffset = Math.round((today.getTime() - projectStart.getTime()) / 86400000)
    const todayX = todayOffset * dayW - scrollLeft
    if (todayX >= 0 && todayX <= viewportW) {
      ctx.strokeStyle = '#ea4335'; ctx.lineWidth = 1.5; ctx.setLineDash([4, 3])
      ctx.beginPath(); ctx.moveTo(todayX, HEADER_H); ctx.lineTo(todayX, h); ctx.stroke()
      ctx.setLineDash([])
    }

    // ── ligne élastique de création de lien ──
    if (linkPreview) {
      ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 2; ctx.setLineDash([5, 3])
      ctx.beginPath(); ctx.moveTo(linkPreview.x1, linkPreview.y1); ctx.lineTo(linkPreview.x2, linkPreview.y2); ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = '#1a73e8'; ctx.beginPath(); ctx.arc(linkPreview.x2, linkPreview.y2, 3, 0, Math.PI * 2); ctx.fill()
    }

    // hit-test des barres : renvoyé via une propriété pour le drag (calculé dehors)
  }
}
