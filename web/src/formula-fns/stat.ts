// ── Statistical functions for the spreadsheet formula engine ──────────────────
// Excel/Sheets-compatible statistics: descriptive stats, regression,
// distributions (normal/Poisson/binomial/exponential) and helpers.
//
// Conventions (see ../formula-engine):
//   Fn = (ev: Evaluator, args: Node[]) => Value
//   - propagate FErr unchanged
//   - domain errors → ERR.NUM ; type errors → ERR.VALUE ; not found → ERR.NA
//
// Note: distributions that lack a closed form (inverse CDFs, gamma) are
// implemented via numerical approximations and commented accordingly.

import {
  type Fn, type Evaluator, type Node, type Value, type Scalar,
  ERR, isErr, isMatrix, num, str, flatten, toNum, toStr, toBool,
  compareScalar, matchCriteria,
} from '../formula-engine'

// ── Internal helpers ──────────────────────────────────────────────────────────

type FErr = { err: string }

// Collect numbers from a single argument, ignoring text/blanks (Excel-style).
function argNums(ev: Evaluator, node: Node): number[] | FErr {
  const out: number[] = []
  for (const s of flatten(ev.eval(node))) {
    if (isErr(s)) return s
    if (typeof s === 'number') out.push(s)
    else if (typeof s === 'boolean') out.push(s ? 1 : 0)
    else if (typeof s === 'string' && s !== '' && !isNaN(Number(s))) out.push(Number(s))
  }
  return out
}

// Collect numbers from an argument, coercing like the *A functions:
// text → 0, TRUE → 1, FALSE → 0, blanks ignored. Errors propagate.
function argNumsA(ev: Evaluator, node: Node): number[] | FErr {
  const out: number[] = []
  for (const s of flatten(ev.eval(node))) {
    if (isErr(s)) return s
    if (typeof s === 'number') out.push(s)
    else if (typeof s === 'boolean') out.push(s ? 1 : 0)
    else if (typeof s === 'string') { if (s !== '') out.push(isNaN(Number(s)) ? 0 : Number(s)) }
    // null/blank → ignored
  }
  return out
}

const sum = (a: number[]): number => a.reduce((x, y) => x + y, 0)
const mean = (a: number[]): number => sum(a) / a.length

// Sum of squared deviations from the mean.
function devSq(a: number[]): number {
  const m = mean(a)
  return a.reduce((s, x) => s + (x - m) * (x - m), 0)
}

// Pearson covariance helper over two equal-length arrays.
function covar(x: number[], y: number[], sample: boolean): number {
  const n = x.length
  const mx = mean(x), my = mean(y)
  let s = 0
  for (let i = 0; i < n; i++) s += (x[i] - mx) * (y[i] - my)
  return s / (sample ? n - 1 : n)
}

// Pearson correlation coefficient.
function correl(x: number[], y: number[]): number | FErr {
  const n = x.length
  const mx = mean(x), my = mean(y)
  let sxy = 0, sxx = 0, syy = 0
  for (let i = 0; i < n; i++) {
    const dx = x[i] - mx, dy = y[i] - my
    sxy += dx * dy; sxx += dx * dx; syy += dy * dy
  }
  const den = Math.sqrt(sxx * syy)
  if (den === 0) return ERR.DIV0
  return sxy / den
}

// Linear regression slope/intercept of y on x.
function linreg(y: number[], x: number[]): { slope: number; intercept: number } | FErr {
  const n = x.length
  const mx = mean(x), my = mean(y)
  let sxy = 0, sxx = 0
  for (let i = 0; i < n; i++) { const dx = x[i] - mx; sxy += dx * (y[i] - my); sxx += dx * dx }
  if (sxx === 0) return ERR.DIV0
  const slope = sxy / sxx
  return { slope, intercept: my - slope * mx }
}

// Evaluate two array args into parallel numeric vectors of equal length.
// Returns FErr on error, or null on length mismatch (caller decides error).
function twoVecs(ev: Evaluator, n1: Node, n2: Node): { x: number[]; y: number[] } | FErr | null {
  const va = flatten(ev.eval(n1)), vb = flatten(ev.eval(n2))
  const x: number[] = [], y: number[] = []
  if (va.length !== vb.length) return null
  for (let i = 0; i < va.length; i++) {
    const a = va[i], b = vb[i]
    if (isErr(a)) return a
    if (isErr(b)) return b
    // Skip pairs where either side is non-numeric (Excel pairwise behaviour).
    const an = typeof a === 'number' ? a : (typeof a === 'boolean' ? (a ? 1 : 0) : (typeof a === 'string' && a !== '' && !isNaN(Number(a)) ? Number(a) : null))
    const bn = typeof b === 'number' ? b : (typeof b === 'boolean' ? (b ? 1 : 0) : (typeof b === 'string' && b !== '' && !isNaN(Number(b)) ? Number(b) : null))
    if (an == null || bn == null) continue
    x.push(an); y.push(bn)
  }
  return { x, y }
}

// k-th smallest (1-based), array assumed already collected.
function nthSorted(a: number[], k: number, largest: boolean): number | FErr {
  if (k < 1 || k > a.length) return ERR.NUM
  const s = [...a].sort((p, q) => p - q)
  return largest ? s[s.length - k] : s[k - 1]
}

// Percentile (inclusive, PERCENTILE.INC / linear interpolation), 0<=p<=1.
function percentileInc(a: number[], p: number): number | FErr {
  if (p < 0 || p > 1) return ERR.NUM
  const s = [...a].sort((x, y) => x - y)
  const n = s.length
  if (n === 1) return s[0]
  const rank = p * (n - 1)
  const lo = Math.floor(rank)
  const frac = rank - lo
  if (lo + 1 >= n) return s[n - 1]
  return s[lo] + frac * (s[lo + 1] - s[lo])
}

// Percentile (exclusive, PERCENTILE.EXC), 1/(n+1) <= p <= n/(n+1).
function percentileExc(a: number[], p: number): number | FErr {
  const s = [...a].sort((x, y) => x - y)
  const n = s.length
  if (p < 1 / (n + 1) || p > n / (n + 1)) return ERR.NUM
  const rank = p * (n + 1) - 1
  const lo = Math.floor(rank)
  const frac = rank - lo
  if (lo + 1 >= n) return s[n - 1]
  if (lo < 0) return s[0]
  return s[lo] + frac * (s[lo + 1] - s[lo])
}

// Percent rank (INC). value need not be in the array.
function percentRankInc(a: number[], x: number, sig: number): number | FErr {
  const s = [...a].sort((p, q) => p - q)
  const n = s.length
  if (x < s[0] || x > s[n - 1]) return ERR.NA
  // exact match
  for (let i = 0; i < n; i++) if (s[i] === x) {
    // for repeated values, Excel uses the first occurrence position
    return truncTo(i / (n - 1), sig)
  }
  // interpolate between bounding values
  let i = 0
  while (i < n - 1 && s[i + 1] < x) i++
  const lo = s[i], hi = s[i + 1]
  const rank = (i + (x - lo) / (hi - lo)) / (n - 1)
  return truncTo(rank, sig)
}

// Percent rank (EXC).
function percentRankExc(a: number[], x: number, sig: number): number | FErr {
  const s = [...a].sort((p, q) => p - q)
  const n = s.length
  if (x < s[0] || x > s[n - 1]) return ERR.NA
  for (let i = 0; i < n; i++) if (s[i] === x) return truncTo((i + 1) / (n + 1), sig)
  let i = 0
  while (i < n - 1 && s[i + 1] < x) i++
  const lo = s[i], hi = s[i + 1]
  const rank = ((i + 1) + (x - lo) / (hi - lo)) / (n + 1)
  return truncTo(rank, sig)
}

function truncTo(v: number, sig: number): number {
  const f = Math.pow(10, sig)
  return Math.trunc(v * f) / f
}

// Factorial / permutations.
function factorial(n: number): number {
  let r = 1
  for (let i = 2; i <= n; i++) r *= i
  return r
}
function permut(n: number, k: number): number {
  let r = 1
  for (let i = 0; i < k; i++) r *= (n - i)
  return r
}

// ── erf / normal distribution primitives ──────────────────────────────────────

// Abramowitz & Stegun 7.1.26 approximation of the error function (|err| < 1.5e-7).
function erf(x: number): number {
  const sign = x < 0 ? -1 : 1
  x = Math.abs(x)
  const t = 1 / (1 + 0.3275911 * x)
  const y = 1 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x)
  return sign * y
}

// Standard normal PDF.
function normPdf(x: number): number {
  return Math.exp(-0.5 * x * x) / Math.sqrt(2 * Math.PI)
}

// Standard normal CDF via erf.
function normCdf(x: number): number {
  return 0.5 * (1 + erf(x / Math.SQRT2))
}

// Inverse standard normal CDF — Beasley-Springer / Moro rational approximation.
function normInvStd(p: number): number {
  if (p <= 0 || p >= 1) return NaN
  const a = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00]
  const b = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01]
  const c = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00, -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00]
  const d = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00]
  const plow = 0.02425, phigh = 1 - plow
  let x: number
  if (p < plow) {
    const q = Math.sqrt(-2 * Math.log(p))
    x = (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1)
  } else if (p <= phigh) {
    const q = p - 0.5, r = q * q
    x = (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1)
  } else {
    const q = Math.sqrt(-2 * Math.log(1 - p))
    x = -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1)
  }
  // One Halley refinement step for extra accuracy.
  const e = normCdf(x) - p
  const u = e * Math.sqrt(2 * Math.PI) * Math.exp(x * x / 2)
  x = x - u / (1 + x * u / 2)
  return x
}

// Lanczos approximation of ln(Gamma).
function gammaln(x: number): number {
  const g = [676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7]
  if (x < 0.5) {
    // reflection formula
    return Math.log(Math.PI / Math.sin(Math.PI * x)) - gammaln(1 - x)
  }
  x -= 1
  let a = 0.99999999999980993
  const t = x + 7.5
  for (let i = 0; i < g.length; i++) a += g[i] / (x + i + 1)
  return 0.5 * Math.log(2 * Math.PI) + (x + 0.5) * Math.log(t) - t + Math.log(a)
}

const lnFactorial = (n: number): number => gammaln(n + 1)

// Binomial coefficient via log-gamma (avoids overflow).
function binomCoef(n: number, k: number): number {
  return Math.exp(lnFactorial(n) - lnFactorial(k) - lnFactorial(n - k))
}

// ── Small evaluation helpers (mirroring engine style) ─────────────────────────

// Resolve a 1-arg numeric value (scalar), returning FErr on error.
function n1(ev: Evaluator, node: Node): number | FErr { return num(ev, node) }

// ── Registry ──────────────────────────────────────────────────────────────────

export const STAT_FNS: Record<string, Fn> = {
  // ── Descriptive ─────────────────────────────────────────────────────────────
  AVEDEV: (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    if (!n.length) return ERR.NUM
    const m = mean(n)
    return n.reduce((s, x) => s + Math.abs(x - m), 0) / n.length
  },
  AVERAGEA: (ev, a) => {
    const out: number[] = []
    for (const arg of a) { const r = argNumsA(ev, arg); if (isErr(r)) return r; out.push(...r) }
    return out.length ? mean(out) : ERR.DIV0
  },
  DEVSQ: (ev, a) => { const n = ev.nums(a); if (isErr(n)) return n; if (!n.length) return ERR.NUM; return devSq(n) },
  GEOMEAN: (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    if (!n.length) return ERR.NUM
    let logSum = 0
    for (const x of n) { if (x <= 0) return ERR.NUM; logSum += Math.log(x) }
    return Math.exp(logSum / n.length)
  },
  HARMEAN: (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    if (!n.length) return ERR.NUM
    let s = 0
    for (const x of n) { if (x <= 0) return ERR.NUM; s += 1 / x }
    return n.length / s
  },
  KURT: (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    const N = n.length
    if (N < 4) return ERR.NUM // needs at least 4 points (DIV/0 otherwise)
    const m = mean(n)
    const sd = Math.sqrt(devSq(n) / (N - 1))
    if (sd === 0) return ERR.DIV0
    let s = 0
    for (const x of n) s += Math.pow((x - m) / sd, 4)
    return (N * (N + 1)) / ((N - 1) * (N - 2) * (N - 3)) * s - (3 * (N - 1) * (N - 1)) / ((N - 2) * (N - 3))
  },
  SKEW: (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    const N = n.length
    if (N < 3) return ERR.NUM
    const m = mean(n)
    const sd = Math.sqrt(devSq(n) / (N - 1))
    if (sd === 0) return ERR.DIV0
    let s = 0
    for (const x of n) s += Math.pow((x - m) / sd, 3)
    return (N / ((N - 1) * (N - 2))) * s
  },
  'SKEW.P': (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    const N = n.length
    if (N < 1) return ERR.NUM
    const m = mean(n)
    const sd = Math.sqrt(devSq(n) / N) // population sd
    if (sd === 0) return ERR.DIV0
    let s = 0
    for (const x of n) s += Math.pow((x - m) / sd, 3)
    return s / N
  },

  // ── Largest / smallest ────────────────────────────────────────────────────────
  LARGE: (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; const k = n1(ev, a[1]); if (isErr(k)) return k; return nthSorted(n, Math.trunc(k), true) },
  SMALL: (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; const k = n1(ev, a[1]); if (isErr(k)) return k; return nthSorted(n, Math.trunc(k), false) },

  // ── *A min/max (text→0, bool counts) ──────────────────────────────────────────
  MAXA: (ev, a) => { const out: number[] = []; for (const arg of a) { const r = argNumsA(ev, arg); if (isErr(r)) return r; out.push(...r) } return out.length ? Math.max(...out) : 0 },
  MINA: (ev, a) => { const out: number[] = []; for (const arg of a) { const r = argNumsA(ev, arg); if (isErr(r)) return r; out.push(...r) } return out.length ? Math.min(...out) : 0 },

  // ── MAXIFS / MINIFS ────────────────────────────────────────────────────────────
  MAXIFS: (ev, a) => ifsExtreme(ev, a, true),
  MINIFS: (ev, a) => ifsExtreme(ev, a, false),

  // ── AVERAGEIFS ────────────────────────────────────────────────────────────────
  AVERAGEIFS: (ev, a) => {
    const avgRange = flatten(ev.eval(a[0]))
    const pairs: { range: Scalar[]; crit: Scalar }[] = []
    for (let i = 1; i + 1 < a.length; i += 2) pairs.push({ range: flatten(ev.eval(a[i])), crit: ev.scalar(a[i + 1]) })
    let s = 0, c = 0
    for (let i = 0; i < avgRange.length; i++) {
      if (pairs.every(p => matchCriteria(p.range[i], p.crit))) {
        const v = toNum(avgRange[i] ?? 0)
        if (!isErr(v)) { s += v; c++ }
      }
    }
    return c ? s / c : ERR.DIV0
  },

  // ── Mode ──────────────────────────────────────────────────────────────────────
  MODE: (ev, a) => modeSngl(ev, a),
  'MODE.SNGL': (ev, a) => modeSngl(ev, a),
  'MODE.MULT': (ev, a) => {
    const n = ev.nums(a); if (isErr(n)) return n
    const counts = new Map<number, number>()
    for (const x of n) counts.set(x, (counts.get(x) ?? 0) + 1)
    let maxC = 0
    for (const c of counts.values()) if (c > maxC) maxC = c
    if (maxC < 2) return ERR.NA
    // Return vertical array (column) preserving first-seen order.
    const out: Scalar[][] = []
    const seen = new Set<number>()
    for (const x of n) if (counts.get(x) === maxC && !seen.has(x)) { seen.add(x); out.push([x]) }
    return out
  },

  // ── Variance / standard deviation ─────────────────────────────────────────────
  'VAR.S': (ev, a) => varSample(ev, a),
  VAR: (ev, a) => varSample(ev, a),
  'VAR.P': (ev, a) => varPop(ev, a),
  VARP: (ev, a) => varPop(ev, a),
  VARA: (ev, a) => varA(ev, a, true),
  VARPA: (ev, a) => varA(ev, a, false),
  'STDEV.S': (ev, a) => { const v = varSample(ev, a); return isErr(v) ? v : Math.sqrt(v as number) },
  STDEV: (ev, a) => { const v = varSample(ev, a); return isErr(v) ? v : Math.sqrt(v as number) },
  'STDEV.P': (ev, a) => { const v = varPop(ev, a); return isErr(v) ? v : Math.sqrt(v as number) },
  STDEVP: (ev, a) => { const v = varPop(ev, a); return isErr(v) ? v : Math.sqrt(v as number) },
  STDEVA: (ev, a) => { const v = varA(ev, a, true); return isErr(v) ? v : Math.sqrt(v as number) },
  STDEVPA: (ev, a) => { const v = varA(ev, a, false); return isErr(v) ? v : Math.sqrt(v as number) },

  // ── Regression / correlation ──────────────────────────────────────────────────
  CORREL: (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (r.x.length < 1) return ERR.DIV0; return correl(r.x, r.y) },
  PEARSON: (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (r.x.length < 1) return ERR.DIV0; return correl(r.x, r.y) },
  RSQ: (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (r.x.length < 1) return ERR.DIV0; const c = correl(r.x, r.y); return isErr(c) ? c : (c as number) * (c as number) },
  'COVARIANCE.P': (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (!r.x.length) return ERR.DIV0; return covar(r.x, r.y, false) },
  'COVARIANCE.S': (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (r.x.length < 2) return ERR.DIV0; return covar(r.x, r.y, true) },
  // SLOPE/INTERCEPT take (known_y, known_x).
  SLOPE: (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (r.x.length < 2) return ERR.DIV0; const lr = linreg(r.x, r.y); return isErr(lr) ? lr : lr.slope },
  INTERCEPT: (ev, a) => { const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r; if (r.x.length < 2) return ERR.DIV0; const lr = linreg(r.x, r.y); return isErr(lr) ? lr : lr.intercept },
  // FORECAST(x, known_y, known_x).
  FORECAST: (ev, a) => forecast(ev, a),
  'FORECAST.LINEAR': (ev, a) => forecast(ev, a),
  // STEYX(known_y, known_x): standard error of the regression.
  STEYX: (ev, a) => {
    const r = twoVecs(ev, a[0], a[1]); if (r === null) return ERR.NA; if (isErr(r)) return r
    const N = r.x.length
    if (N < 3) return ERR.DIV0
    // y is r.x (first arg = known_y), x is r.y (second arg = known_x)
    const y = r.x, x = r.y
    const mx = mean(x), my = mean(y)
    let sxx = 0, syy = 0, sxy = 0
    for (let i = 0; i < N; i++) { const dx = x[i] - mx, dy = y[i] - my; sxx += dx * dx; syy += dy * dy; sxy += dx * dy }
    if (sxx === 0) return ERR.DIV0
    const v = (syy - (sxy * sxy) / sxx) / (N - 2)
    return Math.sqrt(Math.max(0, v))
  },

  // ── Percentiles / quartiles ───────────────────────────────────────────────────
  'PERCENTILE.INC': (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const p = n1(ev, a[1]); if (isErr(p)) return p; return percentileInc(n, p) },
  PERCENTILE: (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const p = n1(ev, a[1]); if (isErr(p)) return p; return percentileInc(n, p) },
  'PERCENTILE.EXC': (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const p = n1(ev, a[1]); if (isErr(p)) return p; return percentileExc(n, p) },
  'QUARTILE.INC': (ev, a) => quartile(ev, a, false),
  QUARTILE: (ev, a) => quartile(ev, a, false),
  'QUARTILE.EXC': (ev, a) => quartile(ev, a, true),
  'PERCENTRANK.INC': (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const x = n1(ev, a[1]); if (isErr(x)) return x; const sig = a[2] ? n1(ev, a[2]) : 3; if (isErr(sig)) return sig; return percentRankInc(n, x, Math.trunc(sig)) },
  PERCENTRANK: (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const x = n1(ev, a[1]); if (isErr(x)) return x; const sig = a[2] ? n1(ev, a[2]) : 3; if (isErr(sig)) return sig; return percentRankInc(n, x, Math.trunc(sig)) },
  'PERCENTRANK.EXC': (ev, a) => { const n = argNums(ev, a[0]); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const x = n1(ev, a[1]); if (isErr(x)) return x; const sig = a[2] ? n1(ev, a[2]) : 3; if (isErr(sig)) return sig; return percentRankExc(n, x, Math.trunc(sig)) },

  // ── Rank ──────────────────────────────────────────────────────────────────────
  'RANK.EQ': (ev, a) => rank(ev, a, 'eq'),
  RANK: (ev, a) => rank(ev, a, 'eq'),
  'RANK.AVG': (ev, a) => rank(ev, a, 'avg'),

  // ── Standardize / trimmed mean ────────────────────────────────────────────────
  STANDARDIZE: (ev, a) => {
    const x = n1(ev, a[0]); if (isErr(x)) return x
    const m = n1(ev, a[1]); if (isErr(m)) return m
    const sd = n1(ev, a[2]); if (isErr(sd)) return sd
    if (sd <= 0) return ERR.NUM
    return (x - m) / sd
  },
  TRIMMEAN: (ev, a) => {
    const n = argNums(ev, a[0]); if (isErr(n)) return n
    if (!n.length) return ERR.NUM
    const pct = n1(ev, a[1]); if (isErr(pct)) return pct
    if (pct < 0 || pct >= 1) return ERR.NUM
    const s = [...n].sort((x, y) => x - y)
    // number to trim from each end (rounded down to nearest even, halved)
    let trim = Math.floor(s.length * pct)
    trim = Math.floor(trim / 2)
    const kept = s.slice(trim, s.length - trim)
    return kept.length ? mean(kept) : ERR.NUM
  },

  // ── Combinatorics ─────────────────────────────────────────────────────────────
  PERMUT: (ev, a) => {
    const n = n1(ev, a[0]); if (isErr(n)) return n
    const k = n1(ev, a[1]); if (isErr(k)) return k
    const ni = Math.trunc(n), ki = Math.trunc(k)
    if (ni < 0 || ki < 0 || ki > ni) return ERR.NUM
    return permut(ni, ki)
  },
  PERMUTATIONA: (ev, a) => {
    const n = n1(ev, a[0]); if (isErr(n)) return n
    const k = n1(ev, a[1]); if (isErr(k)) return k
    const ni = Math.trunc(n), ki = Math.trunc(k)
    if (ni < 0 || ki < 0) return ERR.NUM
    return Math.pow(ni, ki) // permutations with repetition
  },

  // ── Fisher transform ──────────────────────────────────────────────────────────
  FISHER: (ev, a) => { const x = n1(ev, a[0]); if (isErr(x)) return x; if (x <= -1 || x >= 1) return ERR.NUM; return 0.5 * Math.log((1 + x) / (1 - x)) },
  FISHERINV: (ev, a) => { const y = n1(ev, a[0]); if (isErr(y)) return y; const e = Math.exp(2 * y); return (e - 1) / (e + 1) },

  // ── Gamma ─────────────────────────────────────────────────────────────────────
  GAMMALN: (ev, a) => { const x = n1(ev, a[0]); if (isErr(x)) return x; if (x <= 0) return ERR.NUM; return gammaln(x) },
  'GAMMALN.PRECISE': (ev, a) => { const x = n1(ev, a[0]); if (isErr(x)) return x; if (x <= 0) return ERR.NUM; return gammaln(x) },

  // ── Normal distribution ───────────────────────────────────────────────────────
  'NORM.DIST': (ev, a) => normDist(ev, a),
  NORMDIST: (ev, a) => normDist(ev, a),
  'NORM.S.DIST': (ev, a) => {
    const z = n1(ev, a[0]); if (isErr(z)) return z
    // NORM.S.DIST requires a cumulative flag; NORMSDIST (no suffix) is cumulative-only.
    const cum = a.length > 1 ? toBool(ev.scalar(a[1])) : true
    return cum ? normCdf(z) : normPdf(z)
  },
  NORMSDIST: (ev, a) => { const z = n1(ev, a[0]); if (isErr(z)) return z; return normCdf(z) },
  'NORM.INV': (ev, a) => normInv(ev, a),
  NORMINV: (ev, a) => normInv(ev, a),
  'NORM.S.INV': (ev, a) => { const p = n1(ev, a[0]); if (isErr(p)) return p; if (p <= 0 || p >= 1) return ERR.NUM; return normInvStd(p) },
  NORMSINV: (ev, a) => { const p = n1(ev, a[0]); if (isErr(p)) return p; if (p <= 0 || p >= 1) return ERR.NUM; return normInvStd(p) },
  // GAUSS(z) = NORM.S.DIST(z,TRUE) - 0.5 ; PHI(z) = standard normal PDF.
  GAUSS: (ev, a) => { const z = n1(ev, a[0]); if (isErr(z)) return z; return normCdf(z) - 0.5 },
  PHI: (ev, a) => { const z = n1(ev, a[0]); if (isErr(z)) return z; return normPdf(z) },

  // ── Poisson ───────────────────────────────────────────────────────────────────
  'POISSON.DIST': (ev, a) => poisson(ev, a),
  POISSON: (ev, a) => poisson(ev, a),

  // ── Binomial ──────────────────────────────────────────────────────────────────
  'BINOM.DIST': (ev, a) => binom(ev, a),
  BINOMDIST: (ev, a) => binom(ev, a),

  // ── Exponential ───────────────────────────────────────────────────────────────
  'EXPON.DIST': (ev, a) => expon(ev, a),
  EXPONDIST: (ev, a) => expon(ev, a),

  // ── Confidence interval (normal) ──────────────────────────────────────────────
  'CONFIDENCE.NORM': (ev, a) => confidence(ev, a),
  CONFIDENCE: (ev, a) => confidence(ev, a),
}

// ── Shared implementations ─────────────────────────────────────────────────────

function modeSngl(ev: Evaluator, a: Node[]): Value {
  const n = ev.nums(a); if (isErr(n)) return n
  const counts = new Map<number, number>()
  for (const x of n) counts.set(x, (counts.get(x) ?? 0) + 1)
  let best: number | null = null, bestC = 1
  for (const x of n) { const c = counts.get(x)!; if (c > bestC) { bestC = c; best = x } }
  // No value repeats → #N/A
  if (best === null) return ERR.NA
  // Return the first value (in original order) achieving the max count.
  for (const x of n) if (counts.get(x) === bestC) return x
  return ERR.NA
}

function varSample(ev: Evaluator, a: Node[]): Value {
  const n = ev.nums(a); if (isErr(n)) return n
  if (n.length < 2) return ERR.DIV0
  return devSq(n) / (n.length - 1)
}
function varPop(ev: Evaluator, a: Node[]): Value {
  const n = ev.nums(a); if (isErr(n)) return n
  if (n.length < 1) return ERR.DIV0
  return devSq(n) / n.length
}
function varA(ev: Evaluator, a: Node[], sample: boolean): Value {
  const out: number[] = []
  for (const arg of a) { const r = argNumsA(ev, arg); if (isErr(r)) return r; out.push(...r) }
  if (sample) { if (out.length < 2) return ERR.DIV0; return devSq(out) / (out.length - 1) }
  if (out.length < 1) return ERR.DIV0
  return devSq(out) / out.length
}

function ifsExtreme(ev: Evaluator, a: Node[], max: boolean): Value {
  const range = flatten(ev.eval(a[0]))
  const pairs: { range: Scalar[]; crit: Scalar }[] = []
  for (let i = 1; i + 1 < a.length; i += 2) pairs.push({ range: flatten(ev.eval(a[i])), crit: ev.scalar(a[i + 1]) })
  let best: number | null = null
  for (let i = 0; i < range.length; i++) {
    if (pairs.every(p => matchCriteria(p.range[i], p.crit))) {
      const v = toNum(range[i] ?? 0)
      if (isErr(v)) continue
      if (typeof range[i] !== 'number') continue
      if (best === null || (max ? v > best : v < best)) best = v
    }
  }
  return best === null ? 0 : best // Excel returns 0 when no cell matches
}

function quartile(ev: Evaluator, a: Node[], exc: boolean): Value {
  const n = argNums(ev, a[0]); if (isErr(n)) return n
  if (!n.length) return ERR.NUM
  const q = n1(ev, a[1]); if (isErr(q)) return q
  const qi = Math.trunc(q)
  if (qi < 0 || qi > 4) return ERR.NUM
  const p = qi / 4
  return exc ? percentileExc(n, p) : percentileInc(n, p)
}

function rank(ev: Evaluator, a: Node[], kind: 'eq' | 'avg'): Value {
  const x = n1(ev, a[0]); if (isErr(x)) return x
  const ref = argNums(ev, a[1]); if (isErr(ref)) return ref
  const order = a[2] ? n1(ev, a[2]) : 0; if (isErr(order)) return order
  const asc = order !== 0
  const sorted = [...ref].sort((p, q) => asc ? p - q : q - p)
  const first = sorted.indexOf(x)
  if (first < 0) return ERR.NA
  if (kind === 'eq') return first + 1
  // RANK.AVG: average of the positions occupied by equal values.
  let last = first
  while (last + 1 < sorted.length && sorted[last + 1] === x) last++
  return ((first + 1) + (last + 1)) / 2
}

function forecast(ev: Evaluator, a: Node[]): Value {
  const x = n1(ev, a[0]); if (isErr(x)) return x
  const r = twoVecs(ev, a[1], a[2]); if (r === null) return ERR.NA; if (isErr(r)) return r
  if (r.x.length < 1) return ERR.DIV0
  // a[1] = known_y, a[2] = known_x → r.x is y, r.y is x
  const lr = linreg(r.x, r.y); if (isErr(lr)) return lr
  return lr.intercept + lr.slope * x
}

function normDist(ev: Evaluator, a: Node[]): Value {
  const x = n1(ev, a[0]); if (isErr(x)) return x
  const m = n1(ev, a[1]); if (isErr(m)) return m
  const sd = n1(ev, a[2]); if (isErr(sd)) return sd
  if (sd <= 0) return ERR.NUM
  const cum = toBool(ev.scalar(a[3]))
  const z = (x - m) / sd
  return cum ? normCdf(z) : normPdf(z) / sd
}

function normInv(ev: Evaluator, a: Node[]): Value {
  const p = n1(ev, a[0]); if (isErr(p)) return p
  const m = n1(ev, a[1]); if (isErr(m)) return m
  const sd = n1(ev, a[2]); if (isErr(sd)) return sd
  if (p <= 0 || p >= 1) return ERR.NUM
  if (sd <= 0) return ERR.NUM
  return m + sd * normInvStd(p)
}

function poisson(ev: Evaluator, a: Node[]): Value {
  const k = n1(ev, a[0]); if (isErr(k)) return k
  const lambda = n1(ev, a[1]); if (isErr(lambda)) return lambda
  const cum = toBool(ev.scalar(a[2]))
  const kk = Math.trunc(k)
  if (kk < 0 || lambda < 0) return ERR.NUM
  if (cum) {
    let s = 0
    for (let i = 0; i <= kk; i++) s += Math.exp(-lambda + i * Math.log(lambda) - lnFactorial(i))
    return s
  }
  return Math.exp(-lambda + kk * Math.log(lambda) - lnFactorial(kk))
}

function binom(ev: Evaluator, a: Node[]): Value {
  const k = n1(ev, a[0]); if (isErr(k)) return k
  const n = n1(ev, a[1]); if (isErr(n)) return n
  const p = n1(ev, a[2]); if (isErr(p)) return p
  const cum = toBool(ev.scalar(a[3]))
  const kk = Math.trunc(k), nn = Math.trunc(n)
  if (kk < 0 || kk > nn || p < 0 || p > 1) return ERR.NUM
  const pmf = (i: number): number => binomCoef(nn, i) * Math.pow(p, i) * Math.pow(1 - p, nn - i)
  if (cum) { let s = 0; for (let i = 0; i <= kk; i++) s += pmf(i); return s }
  return pmf(kk)
}

function expon(ev: Evaluator, a: Node[]): Value {
  const x = n1(ev, a[0]); if (isErr(x)) return x
  const lambda = n1(ev, a[1]); if (isErr(lambda)) return lambda
  const cum = toBool(ev.scalar(a[2]))
  if (x < 0 || lambda <= 0) return ERR.NUM
  return cum ? 1 - Math.exp(-lambda * x) : lambda * Math.exp(-lambda * x)
}

function confidence(ev: Evaluator, a: Node[]): Value {
  const alpha = n1(ev, a[0]); if (isErr(alpha)) return alpha
  const sd = n1(ev, a[1]); if (isErr(sd)) return sd
  const size = n1(ev, a[2]); if (isErr(size)) return size
  if (alpha <= 0 || alpha >= 1) return ERR.NUM
  if (sd <= 0) return ERR.NUM
  const nn = Math.trunc(size)
  if (nn < 1) return ERR.NUM
  // z for two-sided interval: NORM.S.INV(1 - alpha/2)
  const z = normInvStd(1 - alpha / 2)
  return z * sd / Math.sqrt(nn)
}

// Note: `str`, `toStr`, `compareScalar`, `isMatrix` are imported for parity with
// the engine's helper surface; only a subset is used directly above.
void str; void toStr; void compareScalar; void isMatrix
