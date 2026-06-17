// ── Statistical functions (part 2) for the spreadsheet formula engine ─────────
// Excel/Sheets-compatible statistical distributions, tests, regression and
// confidence helpers that complement stat.ts.
//
// Conventions (see ../formula-engine):
//   Fn = (ev: Evaluator, args: Node[]) => Value
//   - propagate FErr unchanged (numeric inputs scanned first)
//   - domain / numerical failure → ERR.NUM ; wrong type → ERR.VALUE
//
// All numerical helpers (gammaln, regularized incomplete gamma/beta, erf,
// normal pdf/cdf and their inverses) are self-contained in this file via
// standard algorithms (Lanczos, series + Lentz continued fractions, bisection
// on the regularized CDFs). They are duplicated rather than shared on purpose.
//
// LIMITATION: LINEST/LOGEST/TREND/GROWTH support single-variable linear
// regression robustly (slope/intercept + full stats matrix). Multiple
// regression (several x-columns) is handled via a general least-squares solver
// for the coefficients; the extended statistics block is exact for the
// single-variable case and approximate for the multi-variable case.

import {
  type Fn, type Evaluator, type Node, type Value, type Scalar, type Matrix, type FErr,
  ERR, isErr, isMatrix, num, flatten, toBool,
} from '../formula-engine'

// ── Numerical helpers ─────────────────────────────────────────────────────────

// Natural log of the gamma function (Lanczos approximation, g = 7).
const LANCZOS_G = 7
const LANCZOS_C = [
  0.99999999999980993, 676.5203681218851, -1259.1392167224028,
  771.32342877765313, -176.61502916214059, 12.507343278686905,
  -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
]
function gammaln(x: number): number {
  if (x < 0.5) {
    // Reflection: Γ(x)Γ(1-x) = π / sin(πx)
    return Math.log(Math.PI / Math.sin(Math.PI * x)) - gammaln(1 - x)
  }
  x -= 1
  let a = LANCZOS_C[0]
  const t = x + LANCZOS_G + 0.5
  for (let i = 1; i < LANCZOS_G + 2; i++) a += LANCZOS_C[i] / (x + i)
  return 0.5 * Math.log(2 * Math.PI) + (x + 0.5) * Math.log(t) - t + Math.log(a)
}

// True gamma function with correct sign for negative non-integers.
function gammaFn(x: number): number {
  if (x > 0) return Math.exp(gammaln(x))
  // For x < 0.5 use reflection directly to keep the sign.
  const sinpx = Math.sin(Math.PI * x)
  if (sinpx === 0) return NaN // pole at non-positive integers
  return Math.PI / (sinpx * Math.exp(gammaln(1 - x)))
}

// Regularized lower incomplete gamma P(a,x) = γ(a,x)/Γ(a).
function gammaP(a: number, x: number): number {
  if (x < 0 || a <= 0) return NaN
  if (x === 0) return 0
  if (x < a + 1) {
    // Series representation.
    let ap = a
    let sum = 1 / a
    let del = sum
    for (let n = 0; n < 1000; n++) {
      ap += 1
      del *= x / ap
      sum += del
      if (Math.abs(del) < Math.abs(sum) * 1e-15) break
    }
    return sum * Math.exp(-x + a * Math.log(x) - gammaln(a))
  }
  // Continued fraction for Q(a,x), then P = 1 - Q.
  const tiny = 1e-300
  let b = x + 1 - a
  let c = 1 / tiny
  let d = 1 / b
  let h = d
  for (let i = 1; i < 1000; i++) {
    const an = -i * (i - a)
    b += 2
    d = an * d + b
    if (Math.abs(d) < tiny) d = tiny
    c = b + an / c
    if (Math.abs(c) < tiny) c = tiny
    d = 1 / d
    const del = d * c
    h *= del
    if (Math.abs(del - 1) < 1e-15) break
  }
  const q = Math.exp(-x + a * Math.log(x) - gammaln(a)) * h
  return 1 - q
}

// Regularized incomplete beta function I_x(a,b) via Lentz continued fraction.
function betacf(a: number, b: number, x: number): number {
  const tiny = 1e-300
  const qab = a + b
  const qap = a + 1
  const qam = a - 1
  let c = 1
  let d = 1 - (qab * x) / qap
  if (Math.abs(d) < tiny) d = tiny
  d = 1 / d
  let h = d
  for (let m = 1; m < 300; m++) {
    const m2 = 2 * m
    let aa = (m * (b - m) * x) / ((qam + m2) * (a + m2))
    d = 1 + aa * d
    if (Math.abs(d) < tiny) d = tiny
    c = 1 + aa / c
    if (Math.abs(c) < tiny) c = tiny
    d = 1 / d
    h *= d * c
    aa = (-(a + m) * (qab + m) * x) / ((a + m2) * (qap + m2))
    d = 1 + aa * d
    if (Math.abs(d) < tiny) d = tiny
    c = 1 + aa / c
    if (Math.abs(c) < tiny) c = tiny
    d = 1 / d
    const del = d * c
    h *= del
    if (Math.abs(del - 1) < 1e-15) break
  }
  return h
}
function betaI(x: number, a: number, b: number): number {
  if (x <= 0) return 0
  if (x >= 1) return 1
  const bt = Math.exp(
    gammaln(a + b) - gammaln(a) - gammaln(b) + a * Math.log(x) + b * Math.log(1 - x),
  )
  if (x < (a + 1) / (a + b + 2)) return (bt * betacf(a, b, x)) / a
  return 1 - (bt * betacf(b, a, 1 - x)) / b
}

// Error function via the regularized incomplete gamma.
function erf(x: number): number {
  if (x === 0) return 0
  const p = gammaP(0.5, x * x)
  return x >= 0 ? p : -p
}

// Standard normal pdf / cdf.
function normPdf(z: number): number {
  return Math.exp(-0.5 * z * z) / Math.sqrt(2 * Math.PI)
}
function normCdf(z: number): number {
  return 0.5 * (1 + erf(z / Math.SQRT2))
}

// Inverse standard normal CDF (Acklam's rational approximation).
function normInv(p: number): number {
  if (p <= 0) return -Infinity
  if (p >= 1) return Infinity
  const a = [-3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2,
    1.38357751867269e2, -3.066479806614716e1, 2.506628277459239]
  const b = [-5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2,
    6.680131188771972e1, -1.328068155288572e1]
  const c = [-7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838,
    -2.549732539343734, 4.374664141464968, 2.938163982698783]
  const d = [7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996,
    3.754408661907416]
  const pl = 0.02425
  let x: number
  if (p < pl) {
    const q = Math.sqrt(-2 * Math.log(p))
    x = (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) /
      ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1)
  } else if (p <= 1 - pl) {
    const q = p - 0.5
    const r = q * q
    x = (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q /
      (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1)
  } else {
    const q = Math.sqrt(-2 * Math.log(1 - p))
    x = -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) /
      ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1)
  }
  // One Newton step to refine.
  const e = normCdf(x) - p
  const u = e / normPdf(x)
  x = x - u / (1 + (x * u) / 2)
  return x
}

// Generic monotone-increasing inverse via bisection on [lo, hi].
function bisectInv(cdf: (t: number) => number, target: number, lo: number, hi: number): number {
  let a = lo
  let b = hi
  // Expand the upper bound if needed.
  let guard = 0
  while (cdf(b) < target && guard < 200) { b = b * 2 + 1; guard++ }
  for (let i = 0; i < 200; i++) {
    const m = (a + b) / 2
    const v = cdf(m)
    if (v < target) a = m
    else b = m
    if (b - a < 1e-12 * Math.max(1, Math.abs(m))) break
  }
  return (a + b) / 2
}

// ── Distribution primitives ───────────────────────────────────────────────────

function gammaCdf(x: number, alpha: number, beta: number): number {
  if (x <= 0) return 0
  return gammaP(alpha, x / beta)
}
function gammaPdf(x: number, alpha: number, beta: number): number {
  if (x < 0) return 0
  if (x === 0) return alpha === 1 ? 1 / beta : 0
  return Math.exp((alpha - 1) * Math.log(x) - x / beta - alpha * Math.log(beta) - gammaln(alpha))
}
function chisqCdf(x: number, df: number): number {
  return gammaCdf(x, df / 2, 2)
}
function fCdf(x: number, d1: number, d2: number): number {
  if (x <= 0) return 0
  const y = (d1 * x) / (d1 * x + d2)
  return betaI(y, d1 / 2, d2 / 2)
}
function tCdf(t: number, df: number): number {
  const x = df / (df + t * t)
  const ib = 0.5 * betaI(x, df / 2, 0.5)
  return t > 0 ? 1 - ib : ib
}

// ── Argument helpers ──────────────────────────────────────────────────────────

// Read a numeric argument, propagating FErr.
function n(ev: Evaluator, node: Node): number | FErr { return num(ev, node) }

// Collect numbers from a range/array argument (only numeric scalars kept).
function nums(ev: Evaluator, node: Node): number[] | FErr {
  const out: number[] = []
  for (const s of flatten(ev.eval(node))) {
    if (isErr(s)) return s
    if (typeof s === 'number') out.push(s)
  }
  return out
}

// Optional boolean argument; returns def when the node is absent.
function optBool(ev: Evaluator, node: Node | undefined, def: boolean): boolean | FErr {
  if (!node) return def
  const s = ev.scalar(node)
  if (isErr(s)) return s
  return toBool(s)
}

// Read a 2D matrix from a range argument (single scalar → 1×1).
function matrix(ev: Evaluator, node: Node): Matrix {
  const v = ev.eval(node)
  return isMatrix(v) ? v : [[v as Scalar]]
}

// Extract numeric columns from a matrix as number[][] (column-major), or FErr.
function numColumns(m: Matrix): number[][] | FErr {
  const rows = m.length
  const cols = m[0]?.length ?? 0
  const out: number[][] = []
  for (let c = 0; c < cols; c++) {
    const col: number[] = []
    for (let r = 0; r < rows; r++) {
      const s = m[r][c]
      if (isErr(s)) return s
      if (typeof s !== 'number') return ERR.VALUE
      col.push(s)
    }
    out.push(col)
  }
  return out
}

const sumArr = (a: number[]): number => a.reduce((x, y) => x + y, 0)
const meanArr = (a: number[]): number => sumArr(a) / a.length

// Combinations C(n, k).
function comb(nn: number, k: number): number {
  if (k < 0 || k > nn) return 0
  return Math.exp(gammaln(nn + 1) - gammaln(k + 1) - gammaln(nn - k + 1))
}

// ── Gamma / Beta family ───────────────────────────────────────────────────────

const GAMMA: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  // Excel: poles at 0 and negative integers.
  if (x === 0 || (x < 0 && Number.isInteger(x))) return ERR.NUM
  const g = gammaFn(x)
  return Number.isFinite(g) ? g : ERR.NUM
}

const GAMMA_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const alpha = n(ev, a[1]); if (isErr(alpha)) return alpha
  const beta = n(ev, a[2]); if (isErr(beta)) return beta
  const cum = optBool(ev, a[3], true); if (isErr(cum)) return cum
  if (x < 0 || alpha <= 0 || beta <= 0) return ERR.NUM
  return cum ? gammaCdf(x, alpha, beta) : gammaPdf(x, alpha, beta)
}

const GAMMA_INV: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const alpha = n(ev, a[1]); if (isErr(alpha)) return alpha
  const beta = n(ev, a[2]); if (isErr(beta)) return beta
  if (p < 0 || p > 1 || alpha <= 0 || beta <= 0) return ERR.NUM
  if (p === 0) return 0
  const x = bisectInv((t) => gammaCdf(t, alpha, beta), p, 0, alpha * beta + 10 * beta)
  return x
}

const BETA_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const alpha = n(ev, a[1]); if (isErr(alpha)) return alpha
  const beta = n(ev, a[2]); if (isErr(beta)) return beta
  const cum = optBool(ev, a[3], true); if (isErr(cum)) return cum
  const lo = a[4] ? n(ev, a[4]) : 0; if (isErr(lo)) return lo
  const hi = a[5] ? n(ev, a[5]) : 1; if (isErr(hi)) return hi
  if (alpha <= 0 || beta <= 0 || hi <= lo) return ERR.NUM
  const z = (x - lo) / (hi - lo)
  if (z < 0 || z > 1) return ERR.NUM
  if (cum) return betaI(z, alpha, beta)
  // pdf in standardized units, scaled by 1/(hi-lo).
  if (z <= 0 || z >= 1) return 0
  const pdf = Math.exp(
    gammaln(alpha + beta) - gammaln(alpha) - gammaln(beta) +
    (alpha - 1) * Math.log(z) + (beta - 1) * Math.log(1 - z),
  )
  return pdf / (hi - lo)
}

// Legacy BETADIST(x, alpha, beta, [A], [B]) — cumulative only.
const BETADIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const alpha = n(ev, a[1]); if (isErr(alpha)) return alpha
  const beta = n(ev, a[2]); if (isErr(beta)) return beta
  const lo = a[3] ? n(ev, a[3]) : 0; if (isErr(lo)) return lo
  const hi = a[4] ? n(ev, a[4]) : 1; if (isErr(hi)) return hi
  if (alpha <= 0 || beta <= 0 || hi <= lo) return ERR.NUM
  const z = (x - lo) / (hi - lo)
  if (z < 0 || z > 1) return ERR.NUM
  return betaI(z, alpha, beta)
}

const BETA_INV: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const alpha = n(ev, a[1]); if (isErr(alpha)) return alpha
  const beta = n(ev, a[2]); if (isErr(beta)) return beta
  const lo = a[3] ? n(ev, a[3]) : 0; if (isErr(lo)) return lo
  const hi = a[4] ? n(ev, a[4]) : 1; if (isErr(hi)) return hi
  if (p < 0 || p > 1 || alpha <= 0 || beta <= 0 || hi <= lo) return ERR.NUM
  const z = bisectInv((t) => betaI(t, alpha, beta), p, 0, 1)
  return lo + z * (hi - lo)
}

// ── Chi-square ────────────────────────────────────────────────────────────────

const CHISQ_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const df = n(ev, a[1]); if (isErr(df)) return df
  const cum = optBool(ev, a[2], true); if (isErr(cum)) return cum
  if (x < 0 || df < 1) return ERR.NUM
  if (cum) return chisqCdf(x, df)
  return gammaPdf(x, df / 2, 2)
}

const CHISQ_DIST_RT: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (x < 0 || df < 1) return ERR.NUM
  return 1 - chisqCdf(x, df)
}

const CHISQ_INV: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (p < 0 || p > 1 || df < 1) return ERR.NUM
  if (p === 0) return 0
  return bisectInv((t) => chisqCdf(t, df), p, 0, df + 10 * Math.sqrt(2 * df) + 20)
}

const CHISQ_INV_RT: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (p <= 0 || p > 1 || df < 1) return ERR.NUM
  return bisectInv((t) => chisqCdf(t, df), 1 - p, 0, df + 10 * Math.sqrt(2 * df) + 20)
}

// CHISQ.TEST(actual_range, expected_range) → p-value.
const CHISQ_TEST: Fn = (ev, a) => {
  const obs = matrix(ev, a[0])
  const exp = matrix(ev, a[1])
  const rows = obs.length
  const cols = obs[0]?.length ?? 0
  if (exp.length !== rows || (exp[0]?.length ?? 0) !== cols) return ERR.NA
  let chi2 = 0
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const o = obs[r][c]
      const e = exp[r][c]
      if (isErr(o)) return o
      if (isErr(e)) return e
      if (typeof o !== 'number' || typeof e !== 'number') return ERR.VALUE
      if (e === 0) return ERR.DIV0
      chi2 += ((o - e) * (o - e)) / e
    }
  }
  // Degrees of freedom: (r-1)(c-1) for a table, else n-1 for a vector.
  const df = rows === 1 || cols === 1 ? rows * cols - 1 : (rows - 1) * (cols - 1)
  if (df < 1) return ERR.NUM
  return 1 - chisqCdf(chi2, df)
}

// ── F-distribution ────────────────────────────────────────────────────────────

const F_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const d1 = n(ev, a[1]); if (isErr(d1)) return d1
  const d2 = n(ev, a[2]); if (isErr(d2)) return d2
  const cum = optBool(ev, a[3], true); if (isErr(cum)) return cum
  if (x < 0 || d1 < 1 || d2 < 1) return ERR.NUM
  if (cum) return fCdf(x, d1, d2)
  // pdf
  if (x === 0) return d1 > 2 ? 0 : d1 === 2 ? 1 : Infinity
  const logpdf =
    0.5 * (d1 * Math.log(d1) + d2 * Math.log(d2)) +
    (d1 / 2 - 1) * Math.log(x) -
    0.5 * (d1 + d2) * Math.log(d2 + d1 * x) -
    (gammaln(d1 / 2) + gammaln(d2 / 2) - gammaln((d1 + d2) / 2))
  return Math.exp(logpdf)
}

const F_DIST_RT: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const d1 = n(ev, a[1]); if (isErr(d1)) return d1
  const d2 = n(ev, a[2]); if (isErr(d2)) return d2
  if (x < 0 || d1 < 1 || d2 < 1) return ERR.NUM
  return 1 - fCdf(x, d1, d2)
}

const F_INV: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const d1 = n(ev, a[1]); if (isErr(d1)) return d1
  const d2 = n(ev, a[2]); if (isErr(d2)) return d2
  if (p < 0 || p > 1 || d1 < 1 || d2 < 1) return ERR.NUM
  if (p === 0) return 0
  return bisectInv((t) => fCdf(t, d1, d2), p, 0, 1e6)
}

const F_INV_RT: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const d1 = n(ev, a[1]); if (isErr(d1)) return d1
  const d2 = n(ev, a[2]); if (isErr(d2)) return d2
  if (p <= 0 || p > 1 || d1 < 1 || d2 < 1) return ERR.NUM
  return bisectInv((t) => fCdf(t, d1, d2), 1 - p, 0, 1e6)
}

// F.TEST(array1, array2) → two-tailed p-value for equal variances.
const F_TEST: Fn = (ev, a) => {
  const x = nums(ev, a[0]); if (isErr(x)) return x
  const y = nums(ev, a[1]); if (isErr(y)) return y
  if (x.length < 2 || y.length < 2) return ERR.DIV0
  const varOf = (arr: number[]): number => {
    const m = meanArr(arr)
    return arr.reduce((s, v) => s + (v - m) * (v - m), 0) / (arr.length - 1)
  }
  const v1 = varOf(x)
  const v2 = varOf(y)
  if (v1 === 0 || v2 === 0) return ERR.DIV0
  const f = v1 / v2
  const d1 = x.length - 1
  const d2 = y.length - 1
  // Two-tailed: 2 × min(P(F<=f), P(F>=f)).
  const cdf = fCdf(f, d1, d2)
  const tail = Math.min(cdf, 1 - cdf)
  return 2 * tail
}

// ── Student-t ─────────────────────────────────────────────────────────────────

const T_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const df = n(ev, a[1]); if (isErr(df)) return df
  const cum = optBool(ev, a[2], true); if (isErr(cum)) return cum
  if (df < 1) return ERR.NUM
  if (cum) return tCdf(x, df)
  // pdf
  const logpdf =
    gammaln((df + 1) / 2) - gammaln(df / 2) -
    0.5 * Math.log(df * Math.PI) -
    ((df + 1) / 2) * Math.log(1 + (x * x) / df)
  return Math.exp(logpdf)
}

const T_DIST_2T: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (x < 0 || df < 1) return ERR.NUM
  return 2 * (1 - tCdf(x, df))
}

const T_DIST_RT: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (df < 1) return ERR.NUM
  return 1 - tCdf(x, df)
}

// Legacy TDIST(x, df, tails) — x >= 0, tails 1 or 2.
const TDIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const df = n(ev, a[1]); if (isErr(df)) return df
  const tails = n(ev, a[2]); if (isErr(tails)) return tails
  if (x < 0 || df < 1 || (tails !== 1 && tails !== 2)) return ERR.NUM
  const rt = 1 - tCdf(x, df)
  return tails === 2 ? 2 * rt : rt
}

const T_INV: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (p <= 0 || p >= 1 || df < 1) return ERR.NUM
  return bisectInv((t) => tCdf(t, df), p, -1e6, 1e6)
}

const T_INV_2T: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const df = n(ev, a[1]); if (isErr(df)) return df
  if (p <= 0 || p > 1 || df < 1) return ERR.NUM
  // Two-tailed: find t with P(|T|>t) = p, i.e. tCdf(t) = 1 - p/2.
  return bisectInv((t) => tCdf(t, df), 1 - p / 2, 0, 1e6)
}

// T.TEST(array1, array2, tails, type)
const T_TEST: Fn = (ev, a) => {
  const x = nums(ev, a[0]); if (isErr(x)) return x
  const y = nums(ev, a[1]); if (isErr(y)) return y
  const tails = n(ev, a[2]); if (isErr(tails)) return tails
  const type = n(ev, a[3]); if (isErr(type)) return type
  if ((tails !== 1 && tails !== 2)) return ERR.NUM
  let t: number
  let df: number
  const mx = meanArr(x)
  const my = meanArr(y)
  if (type === 1) {
    // Paired.
    if (x.length !== y.length) return ERR.NA
    const nP = x.length
    const diffs = x.map((v, i) => v - y[i])
    const md = meanArr(diffs)
    const sd = diffs.reduce((s, v) => s + (v - md) * (v - md), 0) / (nP - 1)
    if (sd === 0) return ERR.DIV0
    t = md / Math.sqrt(sd / nP)
    df = nP - 1
  } else {
    const nx = x.length
    const ny = y.length
    const vx = x.reduce((s, v) => s + (v - mx) * (v - mx), 0) / (nx - 1)
    const vy = y.reduce((s, v) => s + (v - my) * (v - my), 0) / (ny - 1)
    if (type === 2) {
      // Equal variance (homoscedastic).
      const sp = ((nx - 1) * vx + (ny - 1) * vy) / (nx + ny - 2)
      t = (mx - my) / Math.sqrt(sp * (1 / nx + 1 / ny))
      df = nx + ny - 2
    } else {
      // Unequal variance (Welch).
      const sxn = vx / nx
      const syn = vy / ny
      t = (mx - my) / Math.sqrt(sxn + syn)
      df = (sxn + syn) * (sxn + syn) /
        ((sxn * sxn) / (nx - 1) + (syn * syn) / (ny - 1))
    }
  }
  const rt = 1 - tCdf(Math.abs(t), df)
  return tails === 2 ? 2 * rt : rt
}

// ── Other distributions ───────────────────────────────────────────────────────

const HYPGEOM_DIST: Fn = (ev, a) => {
  const k = n(ev, a[0]); if (isErr(k)) return k
  const ns = n(ev, a[1]); if (isErr(ns)) return ns
  const K = n(ev, a[2]); if (isErr(K)) return K
  const N = n(ev, a[3]); if (isErr(N)) return N
  const cum = optBool(ev, a[4], false); if (isErr(cum)) return cum
  if (ns < 0 || ns > N || K < 0 || K > N || k < 0 || k > ns) return ERR.NUM
  const pmf = (j: number): number => (comb(K, j) * comb(N - K, ns - j)) / comb(N, ns)
  if (!cum) return pmf(k)
  let s = 0
  const lo = Math.max(0, ns - (N - K))
  for (let j = lo; j <= k; j++) s += pmf(j)
  return s
}

// Legacy HYPGEOMDIST(sample_s, number_sample, population_s, number_pop) — pmf.
const HYPGEOMDIST: Fn = (ev, a) => {
  const k = n(ev, a[0]); if (isErr(k)) return k
  const ns = n(ev, a[1]); if (isErr(ns)) return ns
  const K = n(ev, a[2]); if (isErr(K)) return K
  const N = n(ev, a[3]); if (isErr(N)) return N
  if (ns < 0 || ns > N || K < 0 || K > N || k < 0 || k > ns) return ERR.NUM
  return (comb(K, k) * comb(N - K, ns - k)) / comb(N, ns)
}

const LOGNORM_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const mu = n(ev, a[1]); if (isErr(mu)) return mu
  const sd = n(ev, a[2]); if (isErr(sd)) return sd
  const cum = optBool(ev, a[3], true); if (isErr(cum)) return cum
  if (sd <= 0 || x <= 0) return ERR.NUM
  const z = (Math.log(x) - mu) / sd
  if (cum) return normCdf(z)
  return normPdf(z) / (x * sd)
}

// Legacy LOGNORMDIST(x, mean, sd) — cdf.
const LOGNORMDIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const mu = n(ev, a[1]); if (isErr(mu)) return mu
  const sd = n(ev, a[2]); if (isErr(sd)) return sd
  if (sd <= 0 || x <= 0) return ERR.NUM
  return normCdf((Math.log(x) - mu) / sd)
}

const LOGNORM_INV: Fn = (ev, a) => {
  const p = n(ev, a[0]); if (isErr(p)) return p
  const mu = n(ev, a[1]); if (isErr(mu)) return mu
  const sd = n(ev, a[2]); if (isErr(sd)) return sd
  if (p <= 0 || p >= 1 || sd <= 0) return ERR.NUM
  return Math.exp(mu + sd * normInv(p))
}

const NEGBINOM_DIST: Fn = (ev, a) => {
  const f = n(ev, a[0]); if (isErr(f)) return f
  const s = n(ev, a[1]); if (isErr(s)) return s
  const p = n(ev, a[2]); if (isErr(p)) return p
  const cum = optBool(ev, a[3], false); if (isErr(cum)) return cum
  if (f < 0 || s < 1 || p < 0 || p > 1) return ERR.NUM
  const pmf = (j: number): number =>
    comb(j + s - 1, s - 1) * Math.pow(p, s) * Math.pow(1 - p, j)
  if (!cum) return pmf(f)
  let acc = 0
  for (let j = 0; j <= f; j++) acc += pmf(j)
  return acc
}

// Legacy NEGBINOMDIST(number_f, number_s, prob_s) — pmf.
const NEGBINOMDIST: Fn = (ev, a) => {
  const f = n(ev, a[0]); if (isErr(f)) return f
  const s = n(ev, a[1]); if (isErr(s)) return s
  const p = n(ev, a[2]); if (isErr(p)) return p
  if (f < 0 || s < 1 || p < 0 || p > 1) return ERR.NUM
  return comb(f + s - 1, s - 1) * Math.pow(p, s) * Math.pow(1 - p, f)
}

const WEIBULL_DIST: Fn = (ev, a) => {
  const x = n(ev, a[0]); if (isErr(x)) return x
  const alpha = n(ev, a[1]); if (isErr(alpha)) return alpha
  const beta = n(ev, a[2]); if (isErr(beta)) return beta
  const cum = optBool(ev, a[3], true); if (isErr(cum)) return cum
  if (x < 0 || alpha <= 0 || beta <= 0) return ERR.NUM
  const r = Math.pow(x / beta, alpha)
  if (cum) return 1 - Math.exp(-r)
  return (alpha / beta) * Math.pow(x / beta, alpha - 1) * Math.exp(-r)
}

// ── Binomial helpers ──────────────────────────────────────────────────────────

function binomPmf(k: number, nn: number, p: number): number {
  return comb(nn, k) * Math.pow(p, k) * Math.pow(1 - p, nn - k)
}

const BINOM_INV: Fn = (ev, a) => {
  const trials = n(ev, a[0]); if (isErr(trials)) return trials
  const p = n(ev, a[1]); if (isErr(p)) return p
  const alpha = n(ev, a[2]); if (isErr(alpha)) return alpha
  if (trials < 0 || p < 0 || p > 1 || alpha < 0 || alpha > 1) return ERR.NUM
  let acc = 0
  for (let k = 0; k <= trials; k++) {
    acc += binomPmf(k, trials, p)
    if (acc >= alpha) return k
  }
  return trials
}

const BINOM_DIST_RANGE: Fn = (ev, a) => {
  const trials = n(ev, a[0]); if (isErr(trials)) return trials
  const p = n(ev, a[1]); if (isErr(p)) return p
  const s1 = n(ev, a[2]); if (isErr(s1)) return s1
  const s2raw = a[3] ? n(ev, a[3]) : s1; if (isErr(s2raw)) return s2raw
  const s2 = s2raw
  if (trials < 0 || p < 0 || p > 1 || s1 < 0 || s2 < s1 || s2 > trials) return ERR.NUM
  let acc = 0
  for (let k = Math.ceil(s1); k <= Math.floor(s2); k++) acc += binomPmf(k, trials, p)
  return acc
}

// ── Confidence / covariance / misc ────────────────────────────────────────────

const CONFIDENCE_T: Fn = (ev, a) => {
  const alpha = n(ev, a[0]); if (isErr(alpha)) return alpha
  const sd = n(ev, a[1]); if (isErr(sd)) return sd
  const size = n(ev, a[2]); if (isErr(size)) return size
  if (alpha <= 0 || alpha >= 1 || sd <= 0 || size < 1) return ERR.NUM
  if (size === 1) return ERR.DIV0
  const df = size - 1
  // t critical value at 1 - alpha/2.
  const tcrit = bisectInv((t) => tCdf(t, df), 1 - alpha / 2, 0, 1e6)
  return (tcrit * sd) / Math.sqrt(size)
}

// COVAR(array1, array2) — population covariance.
const COVAR: Fn = (ev, a) => {
  const x = nums(ev, a[0]); if (isErr(x)) return x
  const y = nums(ev, a[1]); if (isErr(y)) return y
  if (x.length !== y.length || x.length === 0) return ERR.NA
  const mx = meanArr(x)
  const my = meanArr(y)
  let s = 0
  for (let i = 0; i < x.length; i++) s += (x[i] - mx) * (y[i] - my)
  return s / x.length
}

// PROB(x_range, prob_range, lower_limit, [upper_limit])
const PROB: Fn = (ev, a) => {
  const xs = nums(ev, a[0]); if (isErr(xs)) return xs
  const ps = nums(ev, a[1]); if (isErr(ps)) return ps
  const lower = n(ev, a[2]); if (isErr(lower)) return lower
  const upperRaw = a[3] ? n(ev, a[3]) : lower; if (isErr(upperRaw)) return upperRaw
  const upper = upperRaw
  if (xs.length !== ps.length || xs.length === 0) return ERR.NA
  const total = sumArr(ps)
  if (Math.abs(total - 1) > 1e-6) return ERR.NUM
  for (const pp of ps) if (pp < 0 || pp > 1) return ERR.NUM
  let acc = 0
  for (let i = 0; i < xs.length; i++) {
    if (xs[i] >= lower && xs[i] <= upper) acc += ps[i]
  }
  return acc
}

// Z.TEST(array, x, [sigma]) — one-tailed.
const Z_TEST: Fn = (ev, a) => {
  const arr = nums(ev, a[0]); if (isErr(arr)) return arr
  const x = n(ev, a[1]); if (isErr(x)) return x
  if (arr.length === 0) return ERR.NA
  const m = meanArr(arr)
  let sigma: number
  if (a[2]) {
    const s = n(ev, a[2]); if (isErr(s)) return s
    sigma = s
  } else {
    if (arr.length < 2) return ERR.DIV0
    const v = arr.reduce((acc, vv) => acc + (vv - m) * (vv - m), 0) / (arr.length - 1)
    sigma = Math.sqrt(v)
  }
  if (sigma <= 0) return ERR.NUM
  const z = (m - x) / (sigma / Math.sqrt(arr.length))
  return 1 - normCdf(z)
}

// FREQUENCY(data_array, bins_array) → single-column Matrix of length bins+1.
const FREQUENCY: Fn = (ev, a) => {
  const data = nums(ev, a[0]); if (isErr(data)) return data
  const bins = nums(ev, a[1]); if (isErr(bins)) return bins
  // Sort bins ascending while tracking original order for the output.
  const sorted = [...bins].sort((p, q) => p - q)
  const counts = new Array(sorted.length + 1).fill(0) as number[]
  for (const v of data) {
    let placed = false
    for (let i = 0; i < sorted.length; i++) {
      if (v <= sorted[i]) { counts[i]++; placed = true; break }
    }
    if (!placed) counts[counts.length - 1]++
  }
  // Map back to original bin order, last cell = overflow.
  const out: Matrix = []
  for (let i = 0; i < bins.length; i++) {
    const rank = sorted.indexOf(bins[i])
    out.push([counts[rank]])
  }
  out.push([counts[counts.length - 1]])
  return out
}

// ── Regression ────────────────────────────────────────────────────────────────

// Solve a small linear system A·x = b by Gaussian elimination (n×n).
function solveLinear(A: number[][], b: number[]): number[] | null {
  const nn = A.length
  const m = A.map((row, i) => [...row, b[i]])
  for (let col = 0; col < nn; col++) {
    let piv = col
    for (let r = col + 1; r < nn; r++) if (Math.abs(m[r][col]) > Math.abs(m[piv][col])) piv = r
    if (Math.abs(m[piv][col]) < 1e-12) return null
    ;[m[col], m[piv]] = [m[piv], m[col]]
    for (let r = 0; r < nn; r++) {
      if (r === col) continue
      const f = m[r][col] / m[col][col]
      for (let c = col; c <= nn; c++) m[r][c] -= f * m[col][c]
    }
  }
  return m.map((row, i) => row[nn] / row[i])
}

// Read y and x design from arguments. Returns { y, X } where X is row-major
// with each row already including the intercept column (if useConst).
interface Regression { y: number[]; X: number[][]; nVars: number }
function buildDesign(
  ev: Evaluator, ysNode: Node, xsNode: Node | undefined, useConst: boolean,
): Regression | FErr {
  const ymat = matrix(ev, ysNode)
  const ycols = numColumns(ymat)
  if (isErr(ycols)) return ycols
  const y = ycols.flat()
  let xCols: number[][]
  if (xsNode) {
    const xmat = matrix(ev, xsNode)
    const xc = numColumns(xmat)
    if (isErr(xc)) return xc
    // Orient: if rows==y.length use the columns as variables, else assume the
    // matrix is laid out the other way and flatten each variable accordingly.
    if (xmat.length === y.length) xCols = xc
    else {
      // x supplied as rows of variables — transpose.
      const rowsAsVars: number[][] = []
      for (let r = 0; r < xmat.length; r++) {
        const row: number[] = []
        for (let c = 0; c < (xmat[0]?.length ?? 0); c++) {
          const s = xmat[r][c]
          if (isErr(s)) return s
          if (typeof s !== 'number') return ERR.VALUE
          row.push(s)
        }
        rowsAsVars.push(row)
      }
      xCols = rowsAsVars
    }
  } else {
    // Default x = 1, 2, 3, …
    xCols = [y.map((_, i) => i + 1)]
  }
  const nObs = y.length
  for (const col of xCols) if (col.length !== nObs) return ERR.REF
  const nVars = xCols.length
  const X: number[][] = []
  for (let i = 0; i < nObs; i++) {
    const row: number[] = []
    for (let v = 0; v < nVars; v++) row.push(xCols[v][i])
    if (useConst) row.push(1)
    X.push(row)
  }
  return { y, X, nVars }
}

// Ordinary least squares: returns coefficient vector (length = cols of X).
function ols(y: number[], X: number[][]): number[] | null {
  const cols = X[0].length
  const ata: number[][] = Array.from({ length: cols }, () => new Array(cols).fill(0))
  const atb: number[] = new Array(cols).fill(0)
  for (let i = 0; i < X.length; i++) {
    for (let r = 0; r < cols; r++) {
      atb[r] += X[i][r] * y[i]
      for (let c = 0; c < cols; c++) ata[r][c] += X[i][r] * X[i][c]
    }
  }
  return solveLinear(ata, atb)
}

// Predict using coefficients (X row already includes intercept if used).
function predict(coef: number[], X: number[][]): number[] {
  return X.map((row) => row.reduce((s, v, i) => s + v * coef[i], 0))
}

// Build the new-x design rows for TREND/GROWTH prediction.
function buildNewX(
  ev: Evaluator, newNode: Node | undefined, nVars: number, nObs: number,
  trainXFirstCol: number[] | undefined, useConst: boolean,
): number[][] | FErr {
  if (!newNode) {
    // Predict on the training x by default: caller supplies trainX rows.
    return [] // signal: use training design
  }
  const m = matrix(ev, newNode)
  const cols = numColumns(m)
  if (isErr(cols)) return cols
  let xCols: number[][]
  if (cols.length === nVars && (m[0]?.length ?? 0) === nVars) xCols = cols
  else if (nVars === 1) {
    // Single variable: any shape; flatten all values into one column.
    xCols = [cols.flat()]
  } else if (m.length === nVars) {
    // Variables laid out as rows.
    const t: number[][] = []
    for (let r = 0; r < m.length; r++) {
      const row: number[] = []
      for (let c = 0; c < (m[0]?.length ?? 0); c++) {
        const s = m[r][c]
        if (isErr(s)) return s
        if (typeof s !== 'number') return ERR.VALUE
        row.push(s)
      }
      t.push(row)
    }
    xCols = t
  } else {
    xCols = cols
  }
  const len = xCols[0]?.length ?? 0
  const rows: number[][] = []
  for (let i = 0; i < len; i++) {
    const row: number[] = []
    for (let v = 0; v < nVars; v++) row.push(xCols[v][i])
    if (useConst) row.push(1)
    rows.push(row)
  }
  void trainXFirstCol; void nObs
  return rows
}

// Compute the full LINEST statistics matrix for the single/multi-variable fit.
function linestStats(
  y: number[], X: number[][], coef: number[], useConst: boolean,
): Matrix {
  const nObs = y.length
  const p = X[0].length // number of coefficients (incl. intercept)
  const yhat = predict(coef, X)
  const ymean = meanArr(y)
  let ssr = 0 // regression sum of squares
  let sse = 0 // residual sum of squares
  let sst = 0
  for (let i = 0; i < nObs; i++) {
    sse += (y[i] - yhat[i]) * (y[i] - yhat[i])
    ssr += (yhat[i] - ymean) * (yhat[i] - ymean)
    sst += (y[i] - ymean) * (y[i] - ymean)
  }
  const dfRes = nObs - p
  const r2 = sst === 0 ? 1 : (useConst ? ssr / sst : (sst - sse) / sst)
  const seY = dfRes > 0 ? Math.sqrt(sse / dfRes) : 0
  const fStat = dfRes > 0 && sse > 0 ? (ssr / (p - (useConst ? 1 : 0))) / (sse / dfRes) : 0
  // Standard errors of coefficients via (X'X)^-1 diagonal.
  const cols = p
  const ata: number[][] = Array.from({ length: cols }, () => new Array(cols).fill(0))
  for (let i = 0; i < nObs; i++) {
    for (let r = 0; r < cols; r++) for (let c = 0; c < cols; c++) ata[r][c] += X[i][r] * X[i][c]
  }
  const inv = invertMatrix(ata)
  const se: number[] = new Array(cols).fill(0)
  if (inv) for (let i = 0; i < cols; i++) se[i] = Math.sqrt(Math.max(0, inv[i][i]) * (sse / Math.max(1, dfRes)))
  // Excel orders coefficients right-to-left: [bn, …, b1, intercept].
  const coefRev = [...coef].reverse()
  const seRev = [...se].reverse()
  const out: Matrix = []
  out.push(coefRev)
  out.push(seRev)
  out.push([r2, seY])
  out.push([fStat, dfRes])
  out.push([ssr, sse])
  // Pad rows 2-4 to the width of the coefficient row with #N/A like Excel.
  const width = coefRev.length
  for (let r = 2; r < out.length; r++) {
    while (out[r].length < width) out[r].push(ERR.NA)
  }
  return out
}

// Invert a small square matrix (Gauss-Jordan); null if singular.
function invertMatrix(A: number[][]): number[][] | null {
  const nn = A.length
  const m = A.map((row, i) => [...row, ...row.map((_, j) => (i === j ? 1 : 0))])
  for (let col = 0; col < nn; col++) {
    let piv = col
    for (let r = col + 1; r < nn; r++) if (Math.abs(m[r][col]) > Math.abs(m[piv][col])) piv = r
    if (Math.abs(m[piv][col]) < 1e-12) return null
    ;[m[col], m[piv]] = [m[piv], m[col]]
    const d = m[col][col]
    for (let c = 0; c < 2 * nn; c++) m[col][c] /= d
    for (let r = 0; r < nn; r++) {
      if (r === col) continue
      const f = m[r][col]
      for (let c = 0; c < 2 * nn; c++) m[r][c] -= f * m[col][c]
    }
  }
  return m.map((row) => row.slice(nn))
}

const TREND: Fn = (ev, a) => {
  const useConst = a.length >= 4 ? optBool(ev, a[3], true) : true
  if (isErr(useConst)) return useConst
  const reg = buildDesign(ev, a[0], a[1], useConst)
  if (isErr(reg)) return reg
  const coef = ols(reg.y, reg.X)
  if (!coef) return ERR.NUM
  let newRows = buildNewX(ev, a[2], reg.nVars, reg.y.length, undefined, useConst)
  if (isErr(newRows)) return newRows
  if (newRows.length === 0) newRows = reg.X
  const yhat = predict(coef, newRows)
  return yhat.map((v) => [v])
}

const GROWTH: Fn = (ev, a) => {
  const useConst = a.length >= 4 ? optBool(ev, a[3], true) : true
  if (isErr(useConst)) return useConst
  // Fit log(y) linearly.
  const ymat = matrix(ev, a[0])
  const yc = numColumns(ymat)
  if (isErr(yc)) return yc
  const yvals = yc.flat()
  for (const v of yvals) if (v <= 0) return ERR.NUM
  const logy = yvals.map((v) => Math.log(v))
  // Reuse the design builder for x, but with logy as response.
  const reg = buildDesign(ev, a[0], a[1], useConst)
  if (isErr(reg)) return reg
  const coef = ols(logy, reg.X)
  if (!coef) return ERR.NUM
  let newRows = buildNewX(ev, a[2], reg.nVars, reg.y.length, undefined, useConst)
  if (isErr(newRows)) return newRows
  if (newRows.length === 0) newRows = reg.X
  const yhat = predict(coef, newRows)
  return yhat.map((v) => [Math.exp(v)])
}

const LINEST: Fn = (ev, a) => {
  const useConst = a.length >= 3 ? optBool(ev, a[2], true) : true
  if (isErr(useConst)) return useConst
  const stats = a.length >= 4 ? optBool(ev, a[3], false) : false
  if (isErr(stats)) return stats
  const reg = buildDesign(ev, a[0], a[1], useConst)
  if (isErr(reg)) return reg
  const coef = ols(reg.y, reg.X)
  if (!coef) return ERR.NUM
  if (!stats) {
    // Single row: coefficients right-to-left (Excel order).
    return [[...coef].reverse()]
  }
  return linestStats(reg.y, reg.X, coef, useConst)
}

const LOGEST: Fn = (ev, a) => {
  const useConst = a.length >= 3 ? optBool(ev, a[2], true) : true
  if (isErr(useConst)) return useConst
  const stats = a.length >= 4 ? optBool(ev, a[3], false) : false
  if (isErr(stats)) return stats
  const ymat = matrix(ev, a[0])
  const yc = numColumns(ymat)
  if (isErr(yc)) return yc
  const yvals = yc.flat()
  for (const v of yvals) if (v <= 0) return ERR.NUM
  const logy = yvals.map((v) => Math.log(v))
  const reg = buildDesign(ev, a[0], a[1], useConst)
  if (isErr(reg)) return reg
  const coef = ols(logy, reg.X)
  if (!coef) return ERR.NUM
  // LOGEST returns the exponentiated coefficients (m_i = e^{b_i}).
  if (!stats) return [[...coef].map((c) => Math.exp(c)).reverse()]
  const st = linestStats(logy, reg.X, coef, useConst)
  // First row exponentiated for the slope/intercept multipliers.
  st[0] = st[0].map((v) => (typeof v === 'number' ? Math.exp(v) : v))
  return st
}

// ── Registry ──────────────────────────────────────────────────────────────────

export const STAT2_FNS: Record<string, Fn> = {
  // Gamma / Beta
  GAMMA,
  'GAMMA.DIST': GAMMA_DIST, GAMMADIST: GAMMA_DIST,
  'GAMMA.INV': GAMMA_INV, GAMMAINV: GAMMA_INV,
  'BETA.DIST': BETA_DIST, BETADIST,
  'BETA.INV': BETA_INV, BETAINV: BETA_INV,
  // Chi-square
  'CHISQ.DIST': CHISQ_DIST, 'CHISQ.DIST.RT': CHISQ_DIST_RT, CHIDIST: CHISQ_DIST_RT,
  'CHISQ.INV': CHISQ_INV, 'CHISQ.INV.RT': CHISQ_INV_RT, CHIINV: CHISQ_INV_RT,
  'CHISQ.TEST': CHISQ_TEST, CHITEST: CHISQ_TEST,
  // F-distribution
  'F.DIST': F_DIST, 'F.DIST.RT': F_DIST_RT, FDIST: F_DIST_RT,
  'F.INV': F_INV, 'F.INV.RT': F_INV_RT, FINV: F_INV_RT,
  'F.TEST': F_TEST, FTEST: F_TEST,
  // Student-t
  'T.DIST': T_DIST, 'T.DIST.2T': T_DIST_2T, 'T.DIST.RT': T_DIST_RT, TDIST,
  'T.INV': T_INV, 'T.INV.2T': T_INV_2T, TINV: T_INV_2T,
  'T.TEST': T_TEST, TTEST: T_TEST,
  // Other distributions
  'HYPGEOM.DIST': HYPGEOM_DIST, HYPGEOMDIST,
  'LOGNORM.DIST': LOGNORM_DIST, LOGNORMDIST, 'LOGNORM.INV': LOGNORM_INV, LOGINV: LOGNORM_INV,
  'NEGBINOM.DIST': NEGBINOM_DIST, NEGBINOMDIST,
  'WEIBULL.DIST': WEIBULL_DIST, WEIBULL: WEIBULL_DIST,
  // Binomial helpers
  'BINOM.INV': BINOM_INV, CRITBINOM: BINOM_INV,
  'BINOM.DIST.RANGE': BINOM_DIST_RANGE,
  // Confidence / covariance / misc
  'CONFIDENCE.T': CONFIDENCE_T,
  COVAR,
  PROB,
  'Z.TEST': Z_TEST, ZTEST: Z_TEST,
  FREQUENCY,
  // Regression
  TREND, GROWTH, LINEST, LOGEST,
}
