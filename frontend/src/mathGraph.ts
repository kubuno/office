// Data model + helpers for a function-graph block (rendered by GraphBlock on a <canvas>).
// A graph holds one or more functions of `x`, a domain, optional fixed y-range and display flags.

export interface GraphFn { expr: string; color: string }

export interface GraphSpec {
  fns: GraphFn[]
  xmin: number
  xmax: number
  ymin: number | null   // null → auto-scale Y from the sampled values
  ymax: number | null
  grid: boolean
  axisNumbers: boolean
  height: number        // canvas height in CSS px
}

// A pleasant, distinguishable default palette for successive curves.
export const GRAPH_COLORS = ['#1a73e8', '#d93025', '#188038', '#e8710a', '#9334e6', '#00897b', '#c2185b', '#5d4037']

export function defaultGraphSpec(n = 0): GraphSpec {
  return {
    fns: [{ expr: 'sin(x)', color: GRAPH_COLORS[n % GRAPH_COLORS.length] }],
    xmin: -10, xmax: 10, ymin: null, ymax: null, grid: true, axisNumbers: true, height: 280,
  }
}

// Validate + fill a possibly-partial spec coming from storage.
export function normalizeGraphSpec(s: unknown): GraphSpec {
  const d = defaultGraphSpec()
  const o = (s && typeof s === 'object') ? s as Record<string, unknown> : {}
  const fns = Array.isArray(o.fns) && o.fns.length
    ? (o.fns as unknown[]).map((f, i) => {
        const fo = (f && typeof f === 'object') ? f as Record<string, unknown> : {}
        return { expr: typeof fo.expr === 'string' ? fo.expr : '', color: typeof fo.color === 'string' ? fo.color : GRAPH_COLORS[i % GRAPH_COLORS.length] }
      })
    : d.fns
  const num = (v: unknown, fb: number) => (typeof v === 'number' && isFinite(v)) ? v : fb
  const numOrNull = (v: unknown) => (typeof v === 'number' && isFinite(v)) ? v : null
  return {
    fns,
    xmin: num(o.xmin, d.xmin), xmax: num(o.xmax, d.xmax),
    ymin: numOrNull(o.ymin), ymax: numOrNull(o.ymax),
    grid: o.grid !== false, axisNumbers: o.axisNumbers !== false,
    height: num(o.height, d.height),
  }
}

// "Nice" tick step (1, 2, 5 × 10^k) covering [lo, hi] with about `target` divisions.
export function niceStep(lo: number, hi: number, target: number): number {
  const raw = (hi - lo) / Math.max(1, target)
  if (!isFinite(raw) || raw <= 0) return 1
  const mag = Math.pow(10, Math.floor(Math.log10(raw)))
  const norm = raw / mag
  const step = norm >= 5 ? 5 : norm >= 2 ? 2 : 1
  return step * mag
}

// Format a tick label without trailing float noise.
export function fmtTick(v: number, step: number): string {
  if (v === 0) return '0'
  const decimals = Math.max(0, -Math.floor(Math.log10(step)))
  return Number(v.toFixed(Math.min(6, decimals))).toString()
}

export interface CompiledCurve { fn: (x: number) => number; error: string | null; color: string }

// Draw a graph into a prepared 2D context (transform + clear already done by the caller).
// Shared by the interactive <canvas> block and the print/export rasteriser.
export function drawGraph(ctx: CanvasRenderingContext2D, spec: GraphSpec, cssW: number, cssH: number, compiled: CompiledCurve[]): void {
  const { xmin, xmax } = spec
  if (!(xmax > xmin)) return
  // Resolve Y range (auto-scale from samples when not fixed).
  let ymin = spec.ymin, ymax = spec.ymax
  if (ymin == null || ymax == null) {
    let lo = Infinity, hi = -Infinity
    const N = Math.max(64, Math.floor(cssW))
    for (const c of compiled) {
      if (c.error) continue
      for (let i = 0; i <= N; i++) { const y = c.fn(xmin + (xmax - xmin) * i / N); if (isFinite(y)) { if (y < lo) lo = y; if (y > hi) hi = y } }
    }
    if (!isFinite(lo) || !isFinite(hi) || lo === hi) { lo = -1; hi = 1 }
    const pad = (hi - lo) * 0.1 || 1
    ymin = ymin ?? (lo - pad); ymax = ymax ?? (hi + pad)
  }
  if (!(ymax > ymin)) { ymin -= 1; ymax += 1 }

  const px = (x: number) => (x - xmin) / (xmax - xmin) * cssW
  const py = (y: number) => cssH - (y - ymin) / (ymax - ymin) * cssH
  const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v))

  // Grid + numeric labels.
  const xStep = niceStep(xmin, xmax, cssW / 72)
  const yStep = niceStep(ymin, ymax, cssH / 56)
  if (spec.grid) {
    ctx.strokeStyle = '#eceff1'; ctx.lineWidth = 1
    ctx.beginPath()
    for (let x = Math.ceil(xmin / xStep) * xStep; x <= xmax; x += xStep) { const X = Math.round(px(x)) + 0.5; ctx.moveTo(X, 0); ctx.lineTo(X, cssH) }
    for (let y = Math.ceil(ymin / yStep) * yStep; y <= ymax; y += yStep) { const Y = Math.round(py(y)) + 0.5; ctx.moveTo(0, Y); ctx.lineTo(cssW, Y) }
    ctx.stroke()
  }
  // Axes.
  ctx.strokeStyle = '#9aa0a6'; ctx.lineWidth = 1.2
  ctx.beginPath()
  const ax0 = clamp(py(0), 0, cssH), ay0 = clamp(px(0), 0, cssW)
  ctx.moveTo(0, Math.round(ax0) + 0.5); ctx.lineTo(cssW, Math.round(ax0) + 0.5)
  ctx.moveTo(Math.round(ay0) + 0.5, 0); ctx.lineTo(Math.round(ay0) + 0.5, cssH)
  ctx.stroke()
  if (spec.axisNumbers) {
    ctx.fillStyle = '#5f6368'; ctx.font = '10px system-ui, sans-serif'
    ctx.textAlign = 'center'; ctx.textBaseline = 'top'
    const labelY = clamp(ax0 + 3, 2, cssH - 12)
    for (let x = Math.ceil(xmin / xStep) * xStep; x <= xmax; x += xStep) { if (Math.abs(x) < xStep / 2) continue; ctx.fillText(fmtTick(x, xStep), clamp(px(x), 12, cssW - 12), labelY) }
    ctx.textAlign = 'left'; ctx.textBaseline = 'middle'
    const labelX = clamp(ay0 + 4, 2, cssW - 28)
    for (let y = Math.ceil(ymin / yStep) * yStep; y <= ymax; y += yStep) { if (Math.abs(y) < yStep / 2) continue; ctx.fillText(fmtTick(y, yStep), labelX, clamp(py(y), 8, cssH - 8)) }
  }
  // Curves (break the path on non-finite values or large jumps near vertical asymptotes).
  const jump = (ymax - ymin) * 4
  ctx.lineWidth = 2; ctx.lineJoin = 'round'; ctx.lineCap = 'round'
  for (const c of compiled) {
    if (c.error) continue
    ctx.strokeStyle = c.color; ctx.beginPath()
    let pen = false, lastY = NaN
    const steps = Math.floor(cssW)
    for (let i = 0; i <= steps; i++) {
      const x = xmin + (xmax - xmin) * i / steps
      const y = c.fn(x)
      if (!isFinite(y)) { pen = false; lastY = NaN; continue }
      if (pen && Math.abs(y - lastY) > jump) pen = false      // probable discontinuity
      const X = px(x), Y = py(clamp(y, ymin - jump, ymax + jump))
      if (pen) ctx.lineTo(X, Y); else { ctx.moveTo(X, Y); pen = true }
      lastY = y
    }
    ctx.stroke()
  }
}
