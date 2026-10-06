// Interpreter for LibreOffice's OOXML preset geometries (see preset-data.ts).
//
// Everything a preset needs is data: adjustment defaults, guide EQUATIONS
// ("logwidth*$0 /100000", "if(?2 ,?2 ,?3 )"…), a path as typed coordinates plus
// segment commands, the text frame, and the handles with their ranges. This module
// evaluates that data for a given box and adjustment values — so every shape is
// rendered EXACTLY as Office and LibreOffice render it, and its yellow handles sit
// exactly where they do there, because they come from the same numbers.
//
// Equation language (svx EnhancedCustomShapeFunctionParser): + - * /, unary minus,
// abs sqrt sin cos tan atan2(y,x) min max, if(a,b,c) = a>0?b:c, constant pi,
// logwidth/logheight (the box), $N (adjustment N, raw), ?N (equation N).
// Angles fed to sin/cos are already converted by the data (pi*x/10800000), and
// ARCANGLETO takes 1/60000ths of a degree.

import { PRESETS, KIND_TO_PRESET, type Param, type Preset, type PresetHandle } from './preset-data'

export interface PresetEnv {
  w: number
  h: number
  adj: number[]
}

// ── Equation evaluation ──────────────────────────────────────────────────────

interface EvalCtx extends PresetEnv {
  preset: Preset
  memo: (number | undefined)[]
  depth: number
}

/** Tiny recursive-descent parser, evaluated on the fly (expressions are short). */
function evalExpr(src: string, ctx: EvalCtx): number {
  let i = 0
  const n = src.length
  const skip = () => { while (i < n && src[i] === ' ') i++ }

  const primary = (): number => {
    skip()
    const c = src[i]
    if (c === '(') { i++; const v = sum(); skip(); if (src[i] === ')') i++; return v }
    if (c === '-') { i++; return -primary() }
    if (c === '$') {
      i++; const s = i; while (i < n && /\d/.test(src[i])) i++
      return ctx.adj[Number(src.slice(s, i))] ?? 0
    }
    if (c === '?') {
      i++; const s = i; while (i < n && /\d/.test(src[i])) i++
      return evalEquation(Number(src.slice(s, i)), ctx)
    }
    if (c >= '0' && c <= '9') {
      const s = i; while (i < n && /[\d.]/.test(src[i])) i++
      return Number(src.slice(s, i))
    }
    const s = i; while (i < n && /[a-z2]/.test(src[i])) i++
    const word = src.slice(s, i)
    switch (word) {
      case 'logwidth': return ctx.w
      case 'logheight': return ctx.h
      case 'pi': return Math.PI
    }
    skip()
    if (src[i] === '(') {
      i++
      const args: number[] = [sum()]
      skip()
      while (src[i] === ',') { i++; args.push(sum()); skip() }
      if (src[i] === ')') i++
      switch (word) {
        case 'abs': return Math.abs(args[0])
        case 'sqrt': return Math.sqrt(Math.max(0, args[0]))
        case 'sin': return Math.sin(args[0])
        case 'cos': return Math.cos(args[0])
        case 'tan': return Math.tan(args[0])
        case 'atan2': return Math.atan2(args[0], args[1])
        case 'min': return Math.min(...args)
        case 'max': return Math.max(...args)
        case 'if': return args[0] > 0 ? args[1] : args[2]
      }
    }
    return 0
  }
  const product = (): number => {
    let v = primary()
    for (;;) {
      skip()
      if (src[i] === '*') { i++; v *= primary() }
      else if (src[i] === '/') { i++; const d = primary(); v = d === 0 ? 0 : v / d }
      else return v
    }
  }
  const sum = (): number => {
    let v = product()
    for (;;) {
      skip()
      if (src[i] === '+') { i++; v += product() }
      else if (src[i] === '-') { i++; v -= product() }
      else return v
    }
  }
  return sum()
}

function evalEquation(idx: number, ctx: EvalCtx): number {
  const hit = ctx.memo[idx]
  if (hit !== undefined) return hit
  if (ctx.depth > 128) return 0 // defensive: the data has no cycles
  ctx.depth++
  const v = evalExpr(ctx.preset.e[idx] ?? '0', ctx)
  ctx.depth--
  ctx.memo[idx] = v
  return v
}

function evalParam(p: Param, ctx: EvalCtx): number {
  switch (p[0]) {
    case 0: return p[1]
    case 1: return evalEquation(p[1], ctx)
    case 2: return ctx.adj[p[1]] ?? 0
    default: return p[1]
  }
}

function makeCtx(preset: Preset, env: PresetEnv): EvalCtx {
  return { ...env, preset, memo: new Array(preset.e.length), depth: 0 }
}

/** Adjustment values in use: the shape's own (raw OOXML units) over the defaults. */
export function presetAdjDefaults(preset: Preset): number[] {
  return preset.a.map(([, v]) => v)
}
export function presetAdjValues(preset: Preset, adj?: number[]): number[] {
  const def = presetAdjDefaults(preset)
  return def.map((d, i) => {
    const v = adj?.[i]
    return typeof v === 'number' && Number.isFinite(v) ? v : d
  })
}

// ── Path building ────────────────────────────────────────────────────────────

export interface SubPath {
  d: string
  fill: boolean
  stroke: boolean
  /** Fill luminance tweak from DARKEN/…/LIGHTENLESS segments (1 = untouched). */
  shade: number
}

const fmt = (v: number) => (Math.round(v * 100) / 100).toString()

/**
 * Build the sub-paths. Segment commands are LibreOffice's
 * EnhancedCustomShapeSegmentCommand; the ones present in the preset data are
 * MOVETO(1) LINETO(2) CURVETO(3) CLOSE(4) END(5) NOFILL(6) NOSTROKE(7)
 * QUADRATICCURVETO(16) ARCANGLETO(17) and the four shading markers (18–21).
 */
export function presetPath(preset: Preset, env: PresetEnv): SubPath[] {
  const ctx = makeCtx(preset, env)
  const out: SubPath[] = []
  let ci = 0                    // coordinate cursor
  let sub = 0                   // sub-path index (for SubViewSize scaling)
  let d = ''
  let fill = true, stroke = true, shade = 1
  let cx = 0, cy = 0            // current point (needed by ARCANGLETO)

  // Sub-path coordinate spaces: constants are scaled from the sub-view space to
  // the box; equation/adjustment params are already in log space.
  const scale = (): { sx: number; sy: number } => {
    const v = preset.v?.[Math.min(sub, (preset.v?.length ?? 1) - 1)]
    if (!v) return { sx: 1, sy: 1 }
    return { sx: v[0] > 0 ? env.w / v[0] : 1, sy: v[1] > 0 ? env.h / v[1] : 1 }
  }
  const point = (): { x: number; y: number } => {
    const pair = preset.c[ci++]
    if (!pair) return { x: cx, y: cy }
    const { sx, sy } = scale()
    const x = pair[0][0] === 0 ? pair[0][1] * sx : evalParam(pair[0], ctx)
    const y = pair[1][0] === 0 ? pair[1][1] * sy : evalParam(pair[1], ctx)
    return { x, y }
  }
  const flush = () => {
    if (d) out.push({ d, fill, stroke, shade })
    d = ''; fill = true; stroke = true; shade = 1
    sub++
  }

  for (const [cmd, count] of preset.s) {
    switch (cmd) {
      case 1: for (let k = 0; k < count; k++) { const p = point(); cx = p.x; cy = p.y; d += `M ${fmt(p.x)},${fmt(p.y)} ` } break
      case 2: for (let k = 0; k < count; k++) { const p = point(); cx = p.x; cy = p.y; d += `L ${fmt(p.x)},${fmt(p.y)} ` } break
      case 3: for (let k = 0; k < count; k++) {
        const a = point(), b = point(), e = point(); cx = e.x; cy = e.y
        d += `C ${fmt(a.x)},${fmt(a.y)} ${fmt(b.x)},${fmt(b.y)} ${fmt(e.x)},${fmt(e.y)} `
      } break
      case 16: for (let k = 0; k < count; k++) {
        const a = point(), e = point(); cx = e.x; cy = e.y
        d += `Q ${fmt(a.x)},${fmt(a.y)} ${fmt(e.x)},${fmt(e.y)} `
      } break
      case 17: for (let k = 0; k < count; k++) {
        // (wR, hR) then (stAng, swAng) — OOXML arcTo. The angles are in DEGREES:
        // LibreOffice's generator already divided the 1/60000° units ("…/60000.0"
        // in the equations). The current point lies at stAng on the ellipse; sweep
        // by swAng (parametric angles, exactly as EnhancedCustomShape2d draws them).
        const r = point()
        const pair = preset.c[ci++]
        const st = (pair ? evalParam(pair[0], ctx) : 0) * Math.PI / 180
        const sw = (pair ? evalParam(pair[1], ctx) : 0) * Math.PI / 180
        const ecx = cx - r.x * Math.cos(st), ecy = cy - r.y * Math.sin(st)
        const sweep = sw >= 0 ? 1 : 0
        // A full-turn sweep degenerates in SVG (end == start draws nothing): emit
        // it as two half arcs. Kept for any sweep > 180° for numeric robustness.
        const steps = Math.abs(sw) > Math.PI ? 2 : 1
        for (let part = 1; part <= steps; part++) {
          const a2 = st + (sw * part) / steps
          const ex = ecx + r.x * Math.cos(a2), ey = ecy + r.y * Math.sin(a2)
          d += `A ${fmt(r.x)},${fmt(r.y)} 0 0 ${sweep} ${fmt(ex)},${fmt(ey)} `
          cx = ex; cy = ey
        }
      } break
      case 4: d += 'Z '; break
      case 5: flush(); break
      case 6: fill = false; break
      case 7: stroke = false; break
      case 18: shade = 0.7; break   // DARKEN
      case 19: shade = 0.85; break  // DARKENLESS
      case 20: shade = 1.3; break   // LIGHTEN
      case 21: shade = 1.15; break  // LIGHTENLESS
      default: break
    }
  }
  flush()
  return out
}

/** Text frame of the preset, evaluated for the box (defaults to the whole box). */
export function presetTextFrame(preset: Preset, env: PresetEnv): { x: number; y: number; w: number; h: number } {
  if (!preset.t) return { x: 0, y: 0, w: env.w, h: env.h }
  const ctx = makeCtx(preset, env)
  const x0 = evalParam(preset.t[0][0], ctx), y0 = evalParam(preset.t[0][1], ctx)
  const x1 = evalParam(preset.t[1][0], ctx), y1 = evalParam(preset.t[1][1], ctx)
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) }
}

// ── Handles ──────────────────────────────────────────────────────────────────

export interface PresetHandlePos {
  index: number
  x: number
  y: number
}

export function presetHandlePositions(preset: Preset, env: PresetEnv): PresetHandlePos[] {
  const ctx = makeCtx(preset, env)
  return (preset.h ?? []).map((h, index) => ({
    index,
    x: evalParam(h.p[0], ctx),
    y: evalParam(h.p[1], ctx),
  }))
}

/** Which adjustment indices a handle drives. */
function handleRefs(h: PresetHandle): number[] {
  return [h.rx, h.ry, h.ra, h.rr].filter((v): v is number => typeof v === 'number')
}

/** Search bounds for one driven adjustment (its declared range when finite). */
function refBounds(h: PresetHandle, ref: number, env: PresetEnv, preset: Preset): [number, number] {
  const ctx = makeCtx(preset, env)
  const rd = (p?: Param) => (p ? evalParam(p, ctx) : undefined)
  const FIN = (v: number | undefined) => v !== undefined && Math.abs(v) < 2_000_000_000
  if (h.ra === ref) return [0, 21_600_000]                    // angle, 1/60000°
  let lo: number | undefined, hi: number | undefined
  if (h.rx === ref) { lo = rd(h.xm); hi = rd(h.xM) }
  if (h.ry === ref) { lo = rd(h.ym); hi = rd(h.yM) }
  if (h.rr === ref) { lo = rd(h.rm); hi = rd(h.rM) }
  return [FIN(lo) ? lo! : -100_000, FIN(hi) ? hi! : 200_000]
}

/**
 * Drag a handle to (px, py): solve for the driven adjustment value(s) whose handle
 * position lands closest to the pointer. Numeric (coarse grid + refinement): the
 * data's position formulas are arbitrary, and this stays correct for all of them —
 * including polar handles — without a per-formula inverse.
 */
export function presetAdjustFromDrag(
  preset: Preset, env: PresetEnv, index: number, px: number, py: number,
): number[] {
  const h = preset.h?.[index]
  const adj = presetAdjValues(preset, env.adj)
  if (!h) return adj
  const refs = handleRefs(h)
  for (const ref of refs) {
    if (ref < 0 || ref >= adj.length) continue
    const [lo, hi] = refBounds(h, ref, { ...env, adj }, preset)
    const dist = (v: number): number => {
      const trial = adj.slice(); trial[ref] = v
      const ctx = makeCtx(preset, { ...env, adj: trial })
      const hx = evalParam(h.p[0], ctx), hy = evalParam(h.p[1], ctx)
      return (hx - px) * (hx - px) + (hy - py) * (hy - py)
    }
    // Coarse grid, then two zoom-ins around the best sample.
    let best = adj[ref], bestD = dist(best)
    let a = lo, b = hi
    for (let round = 0; round < 3; round++) {
      const steps = 48
      for (let k = 0; k <= steps; k++) {
        const v = a + ((b - a) * k) / steps
        const dd = dist(v)
        if (dd < bestD) { bestD = dd; best = v }
      }
      const span = (b - a) / steps * 2
      a = Math.max(lo, best - span); b = Math.min(hi, best + span)
    }
    adj[ref] = Math.round(best)
  }
  return adj
}

// ── Kind lookup ──────────────────────────────────────────────────────────────

/** The preset backing a catalogue kind, or null (lines/connectors stay bespoke). */
export function presetOf(kind: string): Preset | null {
  const name = KIND_TO_PRESET[kind]
  return name ? PRESETS[name] ?? null : null
}
