import { addDays, format } from 'date-fns'
import type { ProjectTask, TaskDependency } from '../api'
import { effectiveProgress } from './rollup'

export const ROW_H    = 28
export const HEADER_H = 56
export const MIN_DAYS = 60
/** Days of margin shown before the start / after the end AT REST. The chart is kept
 *  tight to this at rest; the wider drag margin only appears while a boundary marker
 *  is being dragged (so it can be scrolled to the edges). */
export const VISIBLE_LEAD = 7
/** Days of scrollable margin added on the dragged side WHILE a boundary marker is
 *  dragged — the room the edge auto-scroll reveals. Hidden at rest. */
export const DRAG_MARGIN = 45
export const TIMELINE_H = 60

// Fallbacks, used only when the theme tokens cannot be read (an offscreen canvas
// with no computed style). The live colours come from the CSS variables in theme(),
// so the diagram follows the light/dark theme like the rest of the module.
export const TASK_COLOR   = '#1a73e8'
export const CRITICAL_CLR = '#d93025'
export const MILESTONE_CLR = '#ea4335'
export const SUMMARY_CLR  = '#5f6368'
export const GRID_CLR     = '#e8eaed'
export const PROGRESS_CLR = '#34a853'

// Zoom : pixels par jour selon l'échelle de temps.
export type ZoomLevel = 'day' | 'week' | 'month'
export const ZOOM_DAYW: Record<ZoomLevel, number> = { day: 26, week: 9, month: 3.2 }

/** The palette a render pass draws with, resolved from the theme tokens. */
interface GanttTheme {
  bg:       string
  altRow:   string
  weekend:  string
  headerBg: string
  grid:     string
  task:     string
  critical: string
  summary:  string
  progress: string
  early:    string    // baseline ghost when ahead of plan
  slate:    string    // dependency arrows, on-time ghost
  textStrong: string
  textDim:    string
  onBar:      string  // label sitting on a coloured bar
}

const FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif'

/** Snap a coordinate so a 1px stroke lands on a device pixel rather than
 *  straddling two — the difference between a crisp grid line and a grey smudge. */
function crisp(v: number): number { return Math.round(v) + 0.5 }

export class GanttRenderer {
  readonly el:    HTMLCanvasElement
  private canvas: HTMLCanvasElement
  private ctx:    CanvasRenderingContext2D
  private dpr:    number
  private themeCache: { key: string; theme: GanttTheme } | null = null

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

  /** Read the theme tokens off the canvas's own computed style, so the diagram
   *  recolours with the app. Cached per theme signature: reading a dozen custom
   *  properties every frame during a drag would be wasteful, and the values only
   *  move when the user flips the theme. */
  private theme(): GanttTheme {
    const root = document.documentElement
    const key = `${root.getAttribute('data-theme') ?? ''}|${root.style.colorScheme}|${getComputedStyle(root).colorScheme}`
    if (this.themeCache && this.themeCache.key === key) return this.themeCache.theme

    const cs = getComputedStyle(this.canvas)
    const pick = (name: string, fallback: string): string => {
      const v = cs.getPropertyValue(name).trim()
      return v || fallback
    }
    const theme: GanttTheme = {
      bg:       pick('--color-surface-0', '#ffffff'),
      altRow:   pick('--color-surface-1', '#fafafa'),
      weekend:  pick('--color-surface-2', '#f1f3f4'),
      headerBg: pick('--color-surface-1', '#f1f3f4'),
      grid:     pick('--color-border', GRID_CLR),
      task:     pick('--color-primary', TASK_COLOR),
      critical: pick('--color-danger', CRITICAL_CLR),
      summary:  '#666666',
      progress: pick('--color-success', PROGRESS_CLR),
      early:    pick('--color-success', '#1e8e3e'),
      slate:    pick('--color-text-tertiary', '#9aa0a6'),
      textStrong: pick('--color-text-primary', '#202124'),
      textDim:    pick('--color-text-secondary', '#5f6368'),
      onBar:      '#ffffff',
    }
    this.themeCache = { key, theme }
    return theme
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
    preview?:     { taskId: string; start: number; end: number } | null,
    linkPreview?: { x1: number; y1: number; x2: number; y2: number } | null,
    baseline?:    Map<string, { es: number; dur: number }> | null,
    assignees?:   Map<string, { label: string; color: string }[]> | null,
    targetEndDay?: number | null,
    /** While a boundary marker is being dragged, where it currently sits (day
     *  offset). The committed dates are untouched until the drag is released. */
    markerPreview?: { which: 'start' | 'end'; day: number } | null,
    /** The boundary marker under the cursor, if any — draws a naming tooltip so the
     *  user knows the rule is the project start / end and can be dragged. */
    hoverMarker?: 'start' | 'end' | null,
    /** The (already-translated) tooltip text for the hovered marker. */
    hoverText?: string,
    /** Days of margin before day 0 (7 at rest, wider while a marker is dragged). */
    leadDays: number = VISIBLE_LEAD,
    /** Rightmost day to draw (content right edge + its margin). */
    drawRight: number = 0,
  ) {
    const ctx = this.ctx
    const w   = viewportW
    const h   = HEADER_H + tasks.length * ROW_H
    const th  = this.theme()

    ctx.clearRect(0, 0, w, h)
    ctx.fillStyle = th.bg
    ctx.fillRect(0, 0, w, h)

    // Day → x. `leadDays` shifts the chart right so day 0 (the project start) is not
    // against the left edge; the whole grid (weekends, day numbers, veils, markers)
    // is drawn from `startDay` (which may be negative, into the lead-in) up to
    // `drawRight` (the right content edge + its margin), so both margins are drawn
    // and simply scrolled to only when a marker is dragged.
    const px = (day: number) => (day + leadDays) * dayW - scrollLeft
    const startDay = Math.floor(scrollLeft / dayW) - leadDays
    const endDay   = Math.min(drawRight, startDay + Math.ceil(viewportW / dayW) + 1)
    const showDays = dayW >= 12
    const stepW    = dayW

    // ── weekends (subtle grey bands, as in Instagantt) ──
    for (let d = startDay; d <= endDay; d++) {
      const date = addDays(projectStart, d)
      const dow  = date.getDay()
      if (dow === 0 || dow === 6) {
        const x = px(d)
        ctx.fillStyle = th.weekend
        ctx.globalAlpha = 0.5
        ctx.fillRect(x, HEADER_H, stepW, tasks.length * ROW_H)
        ctx.globalAlpha = 1
      }
    }

    // ── "today" column: a tinted band over the whole day, the way Instagantt marks
    // the current date (clearer and calmer than a dashed rule). ──
    const todayOffset = Math.floor((Date.now() - projectStart.getTime()) / 86400000)
    if (todayOffset >= startDay && todayOffset <= endDay) {
      const tx = px(todayOffset)
      ctx.fillStyle = th.task
      ctx.globalAlpha = 0.12
      ctx.fillRect(tx, HEADER_H, stepW, tasks.length * ROW_H)
      ctx.globalAlpha = 1
      // and its header cell, so the column reads as "today" up top too
      ctx.fillStyle = th.task
      ctx.globalAlpha = 0.18
      ctx.fillRect(tx, HEADER_H / 2, stepW, HEADER_H / 2)
      ctx.globalAlpha = 1
    }

    // ── vertical grid (weeks always, days only when zoomed in) ──
    for (let d = startDay; d <= endDay; d++) {
      const date = addDays(projectStart, d)
      const isWeekStart = date.getDay() === 1
      if (!showDays && !isWeekStart) continue
      const x = px(d)
      ctx.strokeStyle = th.grid
      ctx.globalAlpha = isWeekStart ? 1 : 0.5
      ctx.lineWidth = 1
      ctx.beginPath(); ctx.moveTo(crisp(x), HEADER_H); ctx.lineTo(crisp(x), h); ctx.stroke()
      ctx.globalAlpha = 1
    }

    // ── header (mois + jours/semaines) ──
    ctx.fillStyle = th.headerBg
    ctx.fillRect(0, 0, w, HEADER_H)
    ctx.strokeStyle = th.grid; ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(0, crisp(HEADER_H)); ctx.lineTo(w, crisp(HEADER_H)); ctx.stroke()
    ctx.beginPath(); ctx.moveTo(0, crisp(HEADER_H / 2)); ctx.lineTo(w, crisp(HEADER_H / 2)); ctx.stroke()

    let curMonth = -1
    for (let d = startDay; d <= endDay; d++) {
      const date = addDays(projectStart, d)
      if (date.getMonth() !== curMonth) {
        curMonth = date.getMonth()
        const x = px(d)
        ctx.fillStyle = th.textStrong
        ctx.font = `600 11px ${FONT}`
        ctx.textAlign = 'left'
        ctx.fillText(format(date, 'MMMM yyyy', { locale }), x + 5, 18)
      }
    }

    ctx.textAlign = 'center'
    if (showDays) {
      for (let d = startDay; d <= endDay; d++) {
        const date = addDays(projectStart, d)
        const x    = px(d) + dayW / 2
        const dow  = date.getDay()
        ctx.fillStyle = dow === 0 || dow === 6 ? th.slate : th.textDim
        ctx.font = `9px ${FONT}`
        ctx.fillText(String(date.getDate()), x, 42)
      }
    } else {
      for (let d = startDay; d <= endDay; d++) {
        const date = addDays(projectStart, d)
        if (date.getDay() !== 1) continue
        const x = px(d) + (dayW * 3.5)
        ctx.fillStyle = th.textDim
        ctx.font = `9px ${FONT}`
        ctx.fillText(`S${format(date, 'w', { locale })}`, x, 42)
      }
    }
    ctx.textAlign = 'left'

    // ── dependency connectors (drawn UNDER the bars, elbow / orthogonal) ──
    // Finish-to-start elbows in the Instagantt manner: a stub out of the
    // predecessor's end, a vertical run, then into the successor's start.
    deps.forEach(dep => {
      const fromIdx = tasks.findIndex(t => t.id === dep.from_task_id)
      const toIdx   = tasks.findIndex(t => t.id === dep.to_task_id)
      if (fromIdx < 0 || toIdx < 0) return
      const fromTask = tasks[fromIdx], toTask = tasks[toIdx]
      const fromEnd = px((fromTask.early_finish ?? ((fromTask.early_start ?? 0) + fromTask.duration_days)))
      const fromY   = HEADER_H + fromIdx * ROW_H + ROW_H / 2
      const toStart = px(toTask.early_start ?? 0)
      const toY     = HEADER_H + toIdx * ROW_H + ROW_H / 2
      this.elbow(fromEnd, fromY, toStart, toY, th.slate)
    })

    // ── task bars ──
    // Geometry (x, bw) and row height are unchanged from the previous renderer:
    // the drag hit-test in ProjectEditorPage computes the same figures, so touching
    // them here would break resizing and linking.
    const BAR_H  = 18

    // Summary bars span from their earliest descendant's start to their latest
    // descendant's finish (Instagantt/MS-Project convention) — a summary's own
    // stored duration is meaningless as a bar. Computed once per render, memoised.
    const childrenOf = new Map<string, ProjectTask[]>()
    for (const t of tasks) {
      if (t.parent_id) {
        const arr = childrenOf.get(t.parent_id)
        if (arr) arr.push(t); else childrenOf.set(t.parent_id, [t])
      }
    }
    const byId = new Map(tasks.map(t => [t.id, t]))
    const isParentId = (id: string) => (childrenOf.get(id)?.length ?? 0) > 0
    // A parent's completion, rolled up from its children (its own stored % is
    // meaningless once it has sub-tasks) — shown on the summary bracket.
    const effProg = effectiveProgress(tasks)
    // While one element is dragged, everything that must FOLLOW it moves with the
    // preview so parents re-span and children slide in step — all gliding, not
    // jumping. A dragged summary (a parent) shifts its descendants; a dragged leaf
    // just moves itself (its parent bracket re-spans through `effStart`).
    let dragLeaf: string | null = null, dragAncestor: string | null = null, dragShift = 0
    if (preview) {
      if (isParentId(preview.taskId)) {
        dragAncestor = preview.taskId
        dragShift = preview.start - ((byId.get(preview.taskId)?.early_start) ?? 0)
      } else {
        dragLeaf = preview.taskId
      }
    }
    const descends = (t: ProjectTask, anc: string): boolean => {
      let p = t.parent_id; let g = 0
      while (p && g++ < 64) { if (p === anc) return true; p = byId.get(p)?.parent_id ?? null }
      return false
    }
    const effStart = (t: ProjectTask): number => {
      if (dragLeaf && t.id === dragLeaf && preview) return preview.start
      // A started/done descendant is locked in place, so it does not follow a phase
      // move — only the free (not-yet-started) ones slide, and the bracket spans the
      // result (only its movable side moves).
      if (dragAncestor && descends(t, dragAncestor) && (t.progress ?? 0) === 0) return (t.early_start ?? 0) + dragShift
      return t.early_start ?? 0
    }
    // Exclusive END of the bar in CALENDAR days — the CPM's early_finish, which a
    // working calendar can stretch beyond start+duration (weekends are crossed, not
    // worked). duration_days alone would draw the bar too short.
    const calEnd = (t: ProjectTask): number => t.early_finish ?? ((t.early_start ?? 0) + t.duration_days)
    const effEnd = (t: ProjectTask): number => {
      if (dragLeaf && t.id === dragLeaf && preview) return preview.end
      if (dragAncestor && descends(t, dragAncestor) && (t.progress ?? 0) === 0) return calEnd(t) + dragShift
      return calEnd(t)
    }

    const spanCache = new Map<string, { es: number; ef: number } | null>()
    const spanOf = (id: string): { es: number; ef: number } | null => {
      const cached = spanCache.get(id)
      if (cached !== undefined) return cached
      spanCache.set(id, null)   // guard against cycles
      let es = Infinity, ef = -Infinity
      for (const c of childrenOf.get(id) ?? []) {
        if (c.task_type === 'summary' || isParentId(c.id)) {
          const sub = spanOf(c.id)
          if (sub) { es = Math.min(es, sub.es); ef = Math.max(ef, sub.ef) }
        } else {
          const cs = effStart(c)
          es = Math.min(es, cs); ef = Math.max(ef, Math.max(effEnd(c), cs))
        }
      }
      const res = es <= ef ? { es, ef } : null
      spanCache.set(id, res)
      return res
    }

    tasks.forEach((task, i) => {
      const y = HEADER_H + i * ROW_H
      const startOffset = effStart(task)
      const endOffset   = effEnd(task)
      const dur = Math.max(endOffset - startOffset, 0)
      const x   = px(startOffset)
      const bw  = Math.max(dur * dayW, 4)
      const barY = y + (ROW_H - BAR_H) / 2
      // Cull off-screen bars — but NOT summaries: their drawn extent is the span of
      // their children (computed below), not this tiny stored bar, so culling on the
      // stored geometry would wrongly drop a summary whose bracket still crosses the
      // viewport once scrolled past its own start.
      if (task.task_type !== 'summary' && (x + bw < -200 || x > viewportW)) return

      // ── baseline ghost (planned), a thin strip below the bar ──
      const bl = baseline?.get(task.id)
      if (bl) {
        const bx  = px(bl.es)
        const bbw = Math.max(bl.dur * dayW, 4)
        const slip = startOffset - bl.es
        ctx.fillStyle = slip > 0 ? th.critical : slip < 0 ? th.early : th.slate
        ctx.globalAlpha = 0.85
        ctx.beginPath(); ctx.roundRect(bx, barY + BAR_H + 1, bbw, 3, 1.5); ctx.fill()
        ctx.globalAlpha = 1
      }

      if (task.task_type === 'milestone') {
        const cx = x, cy = y + ROW_H / 2, s = 7
        ctx.fillStyle = task.is_critical ? th.critical : th.summary
        ctx.beginPath()
        ctx.moveTo(cx, cy - s); ctx.lineTo(cx + s, cy); ctx.lineTo(cx, cy + s); ctx.lineTo(cx - s, cy)
        ctx.closePath(); ctx.fill()
        this.rightLabel(task.name, cx + s + 8, cy, th.textStrong, viewportW)
        return
      }

      // Any task that has sub-tasks is a summary (drawn as a bracket that spans them),
      // whatever its stored type — matching MS Project.
      if (task.task_type === 'summary' || (childrenOf.get(task.id)?.length ?? 0) > 0) {
        // Span the children; fall back to the task's own extent if it has none.
        const span = spanOf(task.id)
        let sStart = span ? span.es : startOffset
        let sEnd   = span ? span.ef : startOffset + Math.max(dur, 1)
        const sx  = px(sStart)
        const sbw = Math.max((sEnd - sStart) * dayW, 6)
        if (sx + sbw < -200 || sx > viewportW) return
        // Thick charcoal bracket with drooping end tabs (MS-Project / Instagantt).
        const topY = y + 5, sh = 9, tab = 6, tabW = 9
        ctx.fillStyle = th.summary
        ctx.beginPath(); ctx.roundRect(sx, topY, sbw, sh, 1.5); ctx.fill()
        ctx.beginPath()
        ctx.moveTo(sx, topY); ctx.lineTo(sx, topY + sh + tab); ctx.lineTo(sx + tabW, topY + sh); ctx.lineTo(sx + tabW, topY); ctx.closePath()
        ctx.moveTo(sx + sbw, topY); ctx.lineTo(sx + sbw, topY + sh + tab); ctx.lineTo(sx + sbw - tabW, topY + sh); ctx.lineTo(sx + sbw - tabW, topY); ctx.closePath()
        ctx.fill()
        this.rightLabel(`${task.name}  ·  ${effProg.get(task.id) ?? task.progress}%`, sx + sbw + 8, y + ROW_H / 2, th.textStrong, viewportW)
        return
      }

      // ── the task bar: a solid coloured pill, Instagantt style ──
      const color = task.is_critical ? th.critical : th.task
      ctx.fillStyle = color
      ctx.beginPath(); ctx.roundRect(x, barY, bw, BAR_H, 4); ctx.fill()

      // progress: darken the completed portion (works on any bar colour / theme)
      if (task.progress > 0 && task.progress < 100) {
        const pw = bw * (task.progress / 100)
        ctx.save()
        ctx.beginPath(); ctx.roundRect(x, barY, bw, BAR_H, 4); ctx.clip()
        ctx.fillStyle = '#000000'; ctx.globalAlpha = 0.22
        ctx.fillRect(x, barY, pw, BAR_H)
        ctx.restore()
        ctx.globalAlpha = 1
      }

      // Lock/done markers on the bar:
      //  • started (progress > 0)  → padlock at the head (start date fixed)
      //  • done   (progress = 100) → padlock at the tail too (finish fixed) and a
      //                              check mark meaning "completed"
      const cy = barY + BAR_H / 2
      const isDone = task.progress >= 100
      if (task.progress > 0 && bw >= 14 && x + 14 > 0) this.padlock(x + 9, cy)
      if (isDone && x + bw > 0) {
        if (bw >= 30) this.padlock(x + bw - 9, cy)
        if (bw >= 52) this.check(x + bw / 2, cy)
        else if (bw >= 22 && bw < 30) this.check(x + bw - 9, cy)   // narrow: show "done" if no room for the tail lock
      }

      // Assignee avatars to the right of the bar (initials on a coloured dot), then
      // the task name — the person responsible reads at a glance (Instagantt-style).
      let labelX = x + bw + 16
      const aList = assignees?.get(task.id)
      if (aList && aList.length) {
        let ax = x + bw + 20
        for (const a of aList.slice(0, 3)) { this.avatarDot(ax, y + ROW_H / 2, a.label, a.color); ax += 12 }
        labelX = ax + 6
      }
      // Instagantt puts the task name to the RIGHT of the bar, in body ink —
      // always legible regardless of the bar's colour, and never anonymous.
      this.rightLabel(task.name, labelX, y + ROW_H / 2, th.textStrong, viewportW)

      // link handle (drag onto another bar to create a dependency)
      ctx.fillStyle = th.bg; ctx.strokeStyle = color; ctx.lineWidth = 1.25
      ctx.beginPath(); ctx.arc(x + bw + 6, barY + BAR_H / 2, 3, 0, Math.PI * 2); ctx.fill(); ctx.stroke()
    })

    // ── elastic link-creation line ──
    if (linkPreview) {
      ctx.strokeStyle = th.task; ctx.lineWidth = 2; ctx.setLineDash([5, 3])
      ctx.beginPath(); ctx.moveTo(linkPreview.x1, linkPreview.y1); ctx.lineTo(linkPreview.x2, linkPreview.y2); ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = th.task; ctx.beginPath(); ctx.arc(linkPreview.x2, linkPreview.y2, 3, 0, Math.PI * 2); ctx.fill()
    }

    // ── "target finish" marker: the project's target end date (a goal line). Red when
    // the computed schedule overruns it, violet otherwise. Drawn over the bars. ──
    // ── the project's two boundary markers: planned START and target END ──
    // Both are drawn the same way (dashed rule + dated flag in the header band) and
    // both can be dragged to move the project's dates; the flag points INWARD, so
    // the pair reads as the window the plan must fit into.
    const startDayMarker = markerPreview?.which === 'start' ? markerPreview.day : 0
    const endDayMarker   = markerPreview?.which === 'end'   ? markerPreview.day : targetEndDay
    const overrun = endDayMarker != null && tasks.some(t =>
      t.task_type !== 'summary' && (t.early_finish ?? ((t.early_start ?? 0) + t.duration_days)) > endDayMarker)

    // Everything OUTSIDE the project window is veiled, so the window the plan must
    // fit into reads at a glance: before the start, and after the target end.
    const sx = px(startDayMarker)
    ctx.fillStyle = '#7c3aed'
    ctx.globalAlpha = 0.07
    if (sx > 0) ctx.fillRect(0, HEADER_H, Math.min(sx, viewportW), h - HEADER_H)
    if (endDayMarker != null) {
      const ex = px(endDayMarker)
      if (ex < viewportW) ctx.fillRect(Math.max(ex, 0), HEADER_H, viewportW - Math.max(ex, 0), h - HEADER_H)
    }
    ctx.globalAlpha = 1

    this.boundary(sx, h, viewportW, '#7c3aed', 'right',
      format(addDays(projectStart, startDayMarker), 'd MMM', { locale }))
    if (endDayMarker != null) {
      this.boundary(px(endDayMarker), h, viewportW, overrun ? th.critical : '#7c3aed', 'left',
        format(addDays(projectStart, endDayMarker), 'd MMM', { locale }))
    }

    // Naming tooltip for the hovered boundary — drawn ON the canvas, at the marker,
    // so it is reliable and correctly positioned (a native `title` floats at the
    // cursor and only after a delay).
    if (hoverMarker && hoverText) {
      const hx = px(hoverMarker === 'start' ? startDayMarker : endDayMarker!)
      this.markerTooltip(hx, viewportW, hoverText)
    }
  }

  /** A small dark tooltip bubble just under the header, pointing at the marker at
   *  `gx`, kept within the viewport horizontally. */
  private markerTooltip(gx: number, viewportW: number, text: string) {
    const ctx = this.ctx
    ctx.font = `500 11px ${FONT}`
    const padX = 8, tw = ctx.measureText(text).width + padX * 2, th2 = 22
    let x = gx - tw / 2
    x = Math.max(4, Math.min(x, viewportW - tw - 4))
    const y = HEADER_H + 4
    ctx.fillStyle = 'rgba(32,33,36,0.94)'
    ctx.beginPath(); ctx.roundRect(x, y, tw, th2, 5); ctx.fill()
    // little pointer up towards the marker
    ctx.beginPath(); ctx.moveTo(Math.max(x + 6, Math.min(gx, x + tw - 6)), y - 5)
    ctx.lineTo(Math.max(x + 6, Math.min(gx, x + tw - 6)) - 5, y + 1)
    ctx.lineTo(Math.max(x + 6, Math.min(gx, x + tw - 6)) + 5, y + 1)
    ctx.closePath(); ctx.fill()
    ctx.fillStyle = '#ffffff'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
    ctx.fillText(text, x + tw / 2, y + th2 / 2 + 0.5)
    ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic'
  }

  /** One project boundary: a dashed vertical rule plus a dated flag in the header,
   *  laid out to the `side` given (the flag sits inside the project window). */
  private boundary(gx: number, h: number, viewportW: number, col: string, side: 'left' | 'right', label: string) {
    const ctx = this.ctx
    if (gx < -80 || gx > viewportW + 80) return
    ctx.strokeStyle = col; ctx.lineWidth = 1.5; ctx.setLineDash([2, 3])
    ctx.beginPath(); ctx.moveTo(crisp(gx), HEADER_H); ctx.lineTo(crisp(gx), h); ctx.stroke()
    ctx.setLineDash([])
    ctx.font = `600 9px ${FONT}`
    const tw = ctx.measureText(label).width + 12
    const bx = side === 'left' ? gx - tw : gx
    ctx.fillStyle = col
    ctx.beginPath(); ctx.roundRect(bx, HEADER_H - 15, tw, 14, 3); ctx.fill()
    ctx.fillStyle = '#ffffff'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
    ctx.fillText(label, bx + tw / 2, HEADER_H - 8)
    ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic'
  }

  /** Where a boundary flag sits horizontally, so the page can hit-test a drag on it.
   *  Mirrors `boundary()` exactly — same font, same padding. */
  flagRect(gx: number, side: 'left' | 'right', label: string): { x: number; w: number } {
    const ctx = this.ctx
    ctx.font = `600 9px ${FONT}`
    const w = ctx.measureText(label).width + 12
    return { x: side === 'left' ? gx - w : gx, w }
  }

  /** Orthogonal finish-to-start connector with an arrowhead into the successor. */
  private elbow(fromX: number, fromY: number, toX: number, toY: number, color: string) {
    const ctx = this.ctx
    const stub = 10, approach = 8
    ctx.strokeStyle = color; ctx.lineWidth = 1.5
    ctx.beginPath()
    ctx.moveTo(fromX, fromY)
    ctx.lineTo(fromX + stub, fromY)
    if (toX - approach >= fromX + stub) {
      // Room to drop straight down into the successor's start.
      ctx.lineTo(fromX + stub, toY)
      ctx.lineTo(toX, toY)
    } else {
      // Successor starts to the left: route around via the mid gutter.
      const midY = fromY + (toY > fromY ? ROW_H / 2 : -ROW_H / 2)
      ctx.lineTo(fromX + stub, midY)
      ctx.lineTo(toX - approach, midY)
      ctx.lineTo(toX - approach, toY)
      ctx.lineTo(toX, toY)
    }
    ctx.stroke()
    // arrowhead pointing right, into the bar's start
    ctx.fillStyle = color
    ctx.beginPath()
    ctx.moveTo(toX, toY); ctx.lineTo(toX - 6, toY - 3.5); ctx.lineTo(toX - 6, toY + 3.5)
    ctx.closePath(); ctx.fill()
  }

  /** A small white padlock at the head of a bar: the task has started, so its start
   *  date is locked. Drawn in white — it sits on the coloured fill. */
  private padlock(cx: number, cy: number) {
    const ctx = this.ctx
    const bodyW = 8, bodyH = 6
    const bx = cx - bodyW / 2, by = cy - bodyH / 2 + 1
    ctx.strokeStyle = '#ffffff'; ctx.lineWidth = 1.4
    ctx.beginPath(); ctx.arc(cx, by, 2.4, Math.PI, 0); ctx.stroke()   // shackle
    ctx.fillStyle = '#ffffff'
    ctx.beginPath(); ctx.roundRect(bx, by, bodyW, bodyH, 1.5); ctx.fill()   // body
    ctx.fillStyle = 'rgba(0,0,0,0.4)'
    ctx.beginPath(); ctx.arc(cx, by + bodyH / 2, 0.95, 0, Math.PI * 2); ctx.fill()   // keyhole
  }

  /** An assignee avatar: initials on a coloured dot with a white ring so overlapping
   *  avatars stay separable. */
  private avatarDot(cx: number, cy: number, label: string, color: string) {
    const ctx = this.ctx
    ctx.fillStyle = '#ffffff'; ctx.beginPath(); ctx.arc(cx, cy, 8, 0, Math.PI * 2); ctx.fill()
    ctx.fillStyle = color;     ctx.beginPath(); ctx.arc(cx, cy, 6.75, 0, Math.PI * 2); ctx.fill()
    ctx.fillStyle = '#ffffff'; ctx.font = `600 8px ${FONT}`
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
    ctx.fillText(label, cx, cy + 0.5)
    ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic'
  }

  /** A white check mark drawn on a bar — the task is completed (100%). */
  private check(cx: number, cy: number) {
    const ctx = this.ctx
    ctx.strokeStyle = '#ffffff'; ctx.lineWidth = 1.8
    ctx.lineCap = 'round'; ctx.lineJoin = 'round'
    ctx.beginPath()
    ctx.moveTo(cx - 3.5, cy + 0.3); ctx.lineTo(cx - 1, cy + 2.8); ctx.lineTo(cx + 3.6, cy - 2.8)
    ctx.stroke()
    ctx.lineCap = 'butt'
  }

  /** A task name anchored to the right of its bar, in body ink, skipped once it
   *  starts past the viewport. */
  private rightLabel(name: string, x: number, midY: number, color: string, viewportW: number) {
    if (!name || x > viewportW) return
    const ctx = this.ctx
    ctx.font = `12px ${FONT}`
    ctx.fillStyle = color
    ctx.textBaseline = 'middle'
    ctx.fillText(name, x, midY + 0.5)
    ctx.textBaseline = 'alphabetic'
  }
}
