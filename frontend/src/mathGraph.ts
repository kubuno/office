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
