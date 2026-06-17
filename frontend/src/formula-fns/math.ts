// ── Math & Trigonometry functions for the spreadsheet formula engine ──────────
// Excel/Sheets-compatible Math & Trig functions. Each entry follows the
// `Fn = (ev, args) => Value` contract from the formula engine: FErr values are
// propagated, invalid domains return #NUM!, type errors #VALUE!, division by
// zero #DIV/0!. Matrix-returning functions return a `Matrix` (Scalar[][]).

import {
  type Fn,
  type Evaluator,
  type Node,
  type Value,
  type Scalar,
  type Matrix,
  ERR,
  isErr,
  isMatrix,
  num,
  str,
  flatten,
  toNum,
  toStr,
  toBool,
  colToIndex,
  indexToCol,
} from '../formula-engine'

// ── Local helpers ─────────────────────────────────────────────────────────────

// Evaluate an optional numeric argument; `undefined` node falls back to `def`.
const numOpt = (ev: Evaluator, n: Node | undefined, def: number): number | typeof ERR.NUM => {
  if (n === undefined) return def
  const v = num(ev, n)
  return isErr(v) ? (v as typeof ERR.NUM) : v
}

// Factorial of a non-negative integer (no overflow guard beyond Infinity).
function factorial(n: number): number {
  let r = 1
  for (let i = 2; i <= n; i++) r *= i
  return r
}

// Greatest common divisor of two non-negative integers.
function gcd2(a: number, b: number): number {
  a = Math.abs(Math.trunc(a))
  b = Math.abs(Math.trunc(b))
  while (b) { [a, b] = [b, a % b] }
  return a
}

// Collect all numeric values from a list of args (ranges/scalars), ignoring
// blanks/text — mirrors `ev.nums` but kept local for clarity where reused.
function collectNums(ev: Evaluator, args: Node[]): number[] | typeof ERR.VALUE {
  const r = ev.nums(args)
  return isErr(r) ? (r as typeof ERR.VALUE) : r
}

// Convert an evaluated argument into a rectangular numeric matrix. Scalars
// become 1×1. Returns an FErr if any cell can't be coerced to a number.
function toNumMatrix(v: Value): number[][] | typeof ERR.VALUE {
  const m: Matrix = isMatrix(v) ? v : [[v as Scalar]]
  const out: number[][] = []
  for (const row of m) {
    const r: number[] = []
    for (const cell of row) {
      const n = toNum(cell)
      if (isErr(n)) return ERR.VALUE
      r.push(n)
    }
    out.push(r)
  }
  return out
}

// ── Roman / Arabic ─────────────────────────────────────────────────────────────
const ROMAN_TABLE: [number, string][] = [
  [1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'],
  [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'],
  [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I'],
]
const ROMAN_VALUES: Record<string, number> = { I: 1, V: 5, X: 10, L: 50, C: 100, D: 500, M: 1000 }

// ── The registry ───────────────────────────────────────────────────────────────
export const MATH_FNS: Record<string, Fn> = {
  // ── Trigonometry ─────────────────────────────────────────────────────────────
  SIN: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.sin(x) },
  COS: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.cos(x) },
  TAN: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.tan(x) },
  ASIN: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x < -1 || x > 1 ? ERR.NUM : Math.asin(x) },
  ACOS: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x < -1 || x > 1 ? ERR.NUM : Math.acos(x) },
  ATAN: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.atan(x) },
  ATAN2: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const y = num(ev, a[1]); if (isErr(y)) return y
    if (x === 0 && y === 0) return ERR.DIV0
    // Excel's ATAN2(x_num, y_num) maps to Math.atan2(y, x).
    return Math.atan2(y, x)
  },
  COT: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const t = Math.tan(x); return t === 0 ? ERR.DIV0 : 1 / t },
  SEC: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const c = Math.cos(x); return c === 0 ? ERR.DIV0 : 1 / c },
  CSC: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const s = Math.sin(x); return s === 0 ? ERR.DIV0 : 1 / s },
  ACOT: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    // ACOT returns a value in (0, PI): atan(1/x) for x>=0, +PI for x<0.
    if (x === 0) return Math.PI / 2
    return x > 0 ? Math.atan(1 / x) : Math.atan(1 / x) + Math.PI
  },

  // ── Hyperbolic ───────────────────────────────────────────────────────────────
  SINH: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.sinh(x) },
  COSH: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.cosh(x) },
  TANH: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.tanh(x) },
  ASINH: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.asinh(x) },
  ACOSH: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x < 1 ? ERR.NUM : Math.acosh(x) },
  ATANH: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x <= -1 || x >= 1 ? ERR.NUM : Math.atanh(x) },
  COTH: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const t = Math.tanh(x); return t === 0 ? ERR.DIV0 : 1 / t },
  SECH: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return 1 / Math.cosh(x) },
  CSCH: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const s = Math.sinh(x); return s === 0 ? ERR.DIV0 : 1 / s },
  ACOTH: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return Math.abs(x) <= 1 ? ERR.NUM : Math.atanh(1 / x) },

  // ── Angles ───────────────────────────────────────────────────────────────────
  DEGREES: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : x * 180 / Math.PI },
  RADIANS: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : x * Math.PI / 180 },

  // ── Logs ─────────────────────────────────────────────────────────────────────
  LOG10: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x <= 0 ? ERR.NUM : Math.log10(x) },

  // ── Rounding family ──────────────────────────────────────────────────────────
  EVEN: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const c = Math.ceil(Math.abs(x) / 2) * 2; return x < 0 ? -c : c },
  ODD: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const s = x < 0 ? -1 : 1; const ax = Math.abs(x)
    let r = Math.ceil(ax)
    if (r % 2 === 0) r += 1
    if (ax === 0) r = 1
    return s * r
  },
  MROUND: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const m = num(ev, a[1]); if (isErr(m)) return m
    if (m === 0) return 0
    if ((x > 0 && m < 0) || (x < 0 && m > 0)) return ERR.NUM
    return Math.round(x / m) * m
  },
  TRUNC: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const d = numOpt(ev, a[1], 0); if (isErr(d)) return d
    const f = Math.pow(10, d)
    return Math.trunc(x * f) / f
  },
  'CEILING.MATH': (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const sig = a[1] !== undefined ? num(ev, a[1]) : 1; if (isErr(sig)) return sig
    const mode = numOpt(ev, a[2], 0); if (isErr(mode)) return mode
    if (sig === 0) return 0
    const s = Math.abs(sig)
    // mode != 0 rounds away from zero for negatives, else toward zero.
    if (x >= 0) return Math.ceil(x / s) * s
    return mode !== 0 ? -Math.ceil(Math.abs(x) / s) * s : -Math.floor(Math.abs(x) / s) * s
  },
  'CEILING.PRECISE': (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const sig = a[1] !== undefined ? num(ev, a[1]) : 1; if (isErr(sig)) return sig
    const s = Math.abs(sig)
    if (s === 0) return 0
    return Math.ceil(x / s) * s
  },
  'ISO.CEILING': (ev, a) => {
    // ISO.CEILING == CEILING.PRECISE (always toward +inf).
    const x = num(ev, a[0]); if (isErr(x)) return x
    const sig = a[1] !== undefined ? num(ev, a[1]) : 1; if (isErr(sig)) return sig
    const s = Math.abs(sig)
    if (s === 0) return 0
    return Math.ceil(x / s) * s
  },
  'FLOOR.MATH': (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const sig = a[1] !== undefined ? num(ev, a[1]) : 1; if (isErr(sig)) return sig
    const mode = numOpt(ev, a[2], 0); if (isErr(mode)) return mode
    if (sig === 0) return 0
    const s = Math.abs(sig)
    if (x >= 0) return Math.floor(x / s) * s
    return mode !== 0 ? -Math.floor(Math.abs(x) / s) * s : -Math.ceil(Math.abs(x) / s) * s
  },
  'FLOOR.PRECISE': (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const sig = a[1] !== undefined ? num(ev, a[1]) : 1; if (isErr(sig)) return sig
    const s = Math.abs(sig)
    if (s === 0) return 0
    return Math.floor(x / s) * s
  },

  // ── Sign / quotient ──────────────────────────────────────────────────────────
  SIGN: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.sign(x) },
  QUOTIENT: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const d = num(ev, a[1]); if (isErr(d)) return d
    return d === 0 ? ERR.DIV0 : Math.trunc(x / d)
  },

  // ── Combinatorics / factorials ───────────────────────────────────────────────
  FACT: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const n = Math.trunc(x); return n < 0 ? ERR.NUM : factorial(n) },
  FACTDOUBLE: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const n = Math.trunc(x)
    if (n < -1) return ERR.NUM
    let r = 1
    for (let i = n; i > 1; i -= 2) r *= i
    return r
  },
  COMBIN: (ev, a) => {
    const n = num(ev, a[0]); if (isErr(n)) return n
    const k = num(ev, a[1]); if (isErr(k)) return k
    const N = Math.trunc(n), K = Math.trunc(k)
    if (N < 0 || K < 0 || K > N) return ERR.NUM
    // Multiplicative form avoids large intermediate factorials.
    let r = 1
    for (let i = 0; i < K; i++) r = r * (N - i) / (i + 1)
    return Math.round(r)
  },
  COMBINA: (ev, a) => {
    const n = num(ev, a[0]); if (isErr(n)) return n
    const k = num(ev, a[1]); if (isErr(k)) return k
    const N = Math.trunc(n), K = Math.trunc(k)
    if (N < 0 || K < 0) return ERR.NUM
    if (N === 0 && K === 0) return 1
    // COMBINA(n,k) = COMBIN(n+k-1, k)
    const m = N + K - 1
    let r = 1
    for (let i = 0; i < K; i++) r = r * (m - i) / (i + 1)
    return Math.round(r)
  },
  MULTINOMIAL: (ev, a) => {
    const nums = collectNums(ev, a); if (isErr(nums)) return nums
    let sum = 0, denom = 1
    for (const v of nums) {
      const n = Math.trunc(v)
      if (n < 0) return ERR.NUM
      sum += n
      denom *= factorial(n)
    }
    return factorial(sum) / denom
  },

  // ── GCD / LCM ────────────────────────────────────────────────────────────────
  GCD: (ev, a) => {
    const nums = collectNums(ev, a); if (isErr(nums)) return nums
    let g = 0
    for (const v of nums) { if (v < 0) return ERR.NUM; g = gcd2(g, v) }
    return g
  },
  LCM: (ev, a) => {
    const nums = collectNums(ev, a); if (isErr(nums)) return nums
    let l = 1
    for (const v of nums) {
      const n = Math.trunc(Math.abs(v))
      if (v < 0) return ERR.NUM
      if (n === 0) return 0
      l = l / gcd2(l, n) * n
    }
    return l
  },

  // ── Roman / Arabic / base conversion ─────────────────────────────────────────
  ROMAN: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    let n = Math.trunc(x)
    if (n < 0 || n > 3999) return ERR.VALUE
    // Only classic form (mode 0) is supported.
    let out = ''
    for (const [val, sym] of ROMAN_TABLE) { while (n >= val) { out += sym; n -= val } }
    return out
  },
  ARABIC: (ev, a) => {
    const s = str(ev, a[0]).toUpperCase().trim()
    if (s === '') return 0
    let sign = 1, str2 = s
    if (str2.startsWith('-')) { sign = -1; str2 = str2.slice(1) }
    let total = 0, prev = 0
    for (let i = str2.length - 1; i >= 0; i--) {
      const v = ROMAN_VALUES[str2[i]]
      if (v === undefined) return ERR.VALUE
      if (v < prev) total -= v
      else { total += v; prev = v }
    }
    return sign * total
  },
  BASE: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const radix = num(ev, a[1]); if (isErr(radix)) return radix
    const minLen = numOpt(ev, a[2], 0); if (isErr(minLen)) return minLen
    const n = Math.trunc(x), r = Math.trunc(radix)
    if (n < 0 || r < 2 || r > 36) return ERR.NUM
    let s = n.toString(r).toUpperCase()
    while (s.length < minLen) s = '0' + s
    return s
  },
  DECIMAL: (ev, a) => {
    const s = str(ev, a[0]).trim().toUpperCase()
    const radix = num(ev, a[1]); if (isErr(radix)) return radix
    const r = Math.trunc(radix)
    if (r < 2 || r > 36) return ERR.NUM
    if (s === '') return 0
    let acc = 0
    for (const ch of s) {
      const d = parseInt(ch, 36)
      if (isNaN(d) || d >= r) return ERR.NUM
      acc = acc * r + d
    }
    return acc
  },

  // ── Misc scalar math ─────────────────────────────────────────────────────────
  SQRTPI: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x < 0 ? ERR.NUM : Math.sqrt(x * Math.PI) },

  // ── Aggregates over multiple args ────────────────────────────────────────────
  SUMSQ: (ev, a) => {
    const nums = collectNums(ev, a); if (isErr(nums)) return nums
    return nums.reduce((acc, v) => acc + v * v, 0)
  },
  SUMPRODUCT: (ev, a) => {
    if (a.length === 0) return 0
    const arrays: number[][] = []
    let len = -1
    for (const arg of a) {
      const v = ev.eval(arg)
      const flat = flatten(v)
      const nums: number[] = []
      for (const s of flat) {
        if (isErr(s)) return s
        const n = toNum(s)
        // Non-numeric cells count as 0 in SUMPRODUCT.
        nums.push(isErr(n) ? 0 : n)
      }
      if (len === -1) len = nums.length
      else if (nums.length !== len) return ERR.VALUE
      arrays.push(nums)
    }
    let sum = 0
    for (let i = 0; i < len; i++) {
      let prod = 1
      for (const arr of arrays) prod *= arr[i]
      sum += prod
    }
    return sum
  },
  SUMX2MY2: (ev, a) => {
    const xs = flatten(ev.eval(a[0])), ys = flatten(ev.eval(a[1]))
    if (xs.length !== ys.length) return ERR.NA
    let sum = 0
    for (let i = 0; i < xs.length; i++) {
      const x = toNum(xs[i]), y = toNum(ys[i])
      if (isErr(x)) return x; if (isErr(y)) return y
      sum += x * x - y * y
    }
    return sum
  },
  SUMX2PY2: (ev, a) => {
    const xs = flatten(ev.eval(a[0])), ys = flatten(ev.eval(a[1]))
    if (xs.length !== ys.length) return ERR.NA
    let sum = 0
    for (let i = 0; i < xs.length; i++) {
      const x = toNum(xs[i]), y = toNum(ys[i])
      if (isErr(x)) return x; if (isErr(y)) return y
      sum += x * x + y * y
    }
    return sum
  },
  SUMXMY2: (ev, a) => {
    const xs = flatten(ev.eval(a[0])), ys = flatten(ev.eval(a[1]))
    if (xs.length !== ys.length) return ERR.NA
    let sum = 0
    for (let i = 0; i < xs.length; i++) {
      const x = toNum(xs[i]), y = toNum(ys[i])
      if (isErr(x)) return x; if (isErr(y)) return y
      const d = x - y
      sum += d * d
    }
    return sum
  },
  SERIESSUM: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const n = num(ev, a[1]); if (isErr(n)) return n
    const m = num(ev, a[2]); if (isErr(m)) return m
    const coeffs = flatten(ev.eval(a[3]))
    let sum = 0
    for (let i = 0; i < coeffs.length; i++) {
      const c = toNum(coeffs[i])
      if (isErr(c)) return c
      sum += c * Math.pow(x, n + i * m)
    }
    return sum
  },

  // ── SUBTOTAL ─────────────────────────────────────────────────────────────────
  SUBTOTAL: (ev, a) => {
    const code = num(ev, a[0]); if (isErr(code)) return code
    // Codes >= 101 ignore hidden rows; we have no row-visibility info, so they
    // behave the same as 1-11 here.
    const fn = code >= 101 ? code - 100 : code
    const nums = collectNums(ev, a.slice(1)); if (isErr(nums)) return nums
    const n = nums.length
    const sum = nums.reduce((x, y) => x + y, 0)
    switch (fn) {
      case 1: return n ? sum / n : ERR.DIV0                       // AVERAGE
      case 2: return nums.length                                  // COUNT
      case 3: { // COUNTA
        let c = 0
        for (const arg of a.slice(1)) for (const s of flatten(ev.eval(arg))) if (!(s === '' || s == null)) c++
        return c
      }
      case 4: return n ? Math.max(...nums) : 0                    // MAX
      case 5: return n ? Math.min(...nums) : 0                    // MIN
      case 6: return nums.reduce((x, y) => x * y, 1)              // PRODUCT
      case 7: { // STDEV (sample)
        if (n < 2) return ERR.DIV0
        const mean = sum / n
        const v = nums.reduce((acc, x) => acc + (x - mean) ** 2, 0) / (n - 1)
        return Math.sqrt(v)
      }
      case 8: { // STDEVP (population)
        if (n < 1) return ERR.DIV0
        const mean = sum / n
        const v = nums.reduce((acc, x) => acc + (x - mean) ** 2, 0) / n
        return Math.sqrt(v)
      }
      case 9: return sum                                          // SUM
      case 10: { // VAR (sample)
        if (n < 2) return ERR.DIV0
        const mean = sum / n
        return nums.reduce((acc, x) => acc + (x - mean) ** 2, 0) / (n - 1)
      }
      case 11: { // VARP (population)
        if (n < 1) return ERR.DIV0
        const mean = sum / n
        return nums.reduce((acc, x) => acc + (x - mean) ** 2, 0) / n
      }
      default: return ERR.VALUE
    }
  },

  // ── Random ───────────────────────────────────────────────────────────────────
  RAND: () => Math.random(),
  RANDBETWEEN: (ev, a) => {
    const lo = num(ev, a[0]); if (isErr(lo)) return lo
    const hi = num(ev, a[1]); if (isErr(hi)) return hi
    const l = Math.ceil(lo), h = Math.floor(hi)
    if (l > h) return ERR.NUM
    return l + Math.floor(Math.random() * (h - l + 1))
  },
  RANDARRAY: (ev, a) => {
    const rows = Math.trunc(numOpt(ev, a[0], 1) as number)
    const cols = Math.trunc(numOpt(ev, a[1], 1) as number)
    const min = numOpt(ev, a[2], 0); if (isErr(min)) return min
    const max = numOpt(ev, a[3], 1); if (isErr(max)) return max
    const integer = a[4] !== undefined ? toBool(ev.scalar(a[4])) : false
    if (rows < 1 || cols < 1) return ERR.VALUE
    if (min > max) return ERR.VALUE
    const out: Matrix = []
    for (let r = 0; r < rows; r++) {
      const row: Scalar[] = []
      for (let c = 0; c < cols; c++) {
        const v = min + Math.random() * (max - min)
        row.push(integer ? Math.floor(min + Math.random() * (max - min + 1)) : v)
      }
      out.push(row)
    }
    return out
  },

  // ── SEQUENCE ─────────────────────────────────────────────────────────────────
  SEQUENCE: (ev, a) => {
    const rows = Math.trunc(numOpt(ev, a[0], 1) as number)
    const cols = Math.trunc(numOpt(ev, a[1], 1) as number)
    const start = numOpt(ev, a[2], 1); if (isErr(start)) return start
    const step = numOpt(ev, a[3], 1); if (isErr(step)) return step
    if (rows < 1 || cols < 1) return ERR.VALUE
    const out: Matrix = []
    let v = start
    for (let r = 0; r < rows; r++) {
      const row: Scalar[] = []
      for (let c = 0; c < cols; c++) { row.push(v); v += step }
      out.push(row)
    }
    return out
  },

  // ── Matrix algebra ───────────────────────────────────────────────────────────
  MUNIT: (ev, a) => {
    const n = num(ev, a[0]); if (isErr(n)) return n
    const dim = Math.trunc(n)
    if (dim < 1) return ERR.VALUE
    const out: Matrix = []
    for (let r = 0; r < dim; r++) {
      const row: Scalar[] = []
      for (let c = 0; c < dim; c++) row.push(r === c ? 1 : 0)
      out.push(row)
    }
    return out
  },
  MMULT: (ev, a) => {
    const A = toNumMatrix(ev.eval(a[0])); if (isErr(A)) return A
    const B = toNumMatrix(ev.eval(a[1])); if (isErr(B)) return B
    const ar = A.length, ac = A[0]?.length ?? 0
    const br = B.length, bc = B[0]?.length ?? 0
    if (ac !== br || ar === 0 || bc === 0) return ERR.VALUE
    const out: Matrix = []
    for (let i = 0; i < ar; i++) {
      const row: Scalar[] = []
      for (let j = 0; j < bc; j++) {
        let s = 0
        for (let k = 0; k < ac; k++) s += A[i][k] * B[k][j]
        row.push(s)
      }
      out.push(row)
    }
    return out
  },
  MDETERM: (ev, a) => {
    const M = toNumMatrix(ev.eval(a[0])); if (isErr(M)) return M
    const n = M.length
    if (n === 0 || M.some(r => r.length !== n)) return ERR.VALUE
    return determinant(M.map(r => [...r]))
  },
  MINVERSE: (ev, a) => {
    const M = toNumMatrix(ev.eval(a[0])); if (isErr(M)) return M
    const n = M.length
    if (n === 0 || M.some(r => r.length !== n)) return ERR.VALUE
    const inv = invert(M.map(r => [...r]))
    if (inv === null) return ERR.NUM
    return inv as Matrix
  },
}

// ── Linear-algebra helpers (Gaussian elimination) ─────────────────────────────

// Determinant via LU-style elimination with partial pivoting.
function determinant(m: number[][]): number {
  const n = m.length
  let det = 1
  for (let i = 0; i < n; i++) {
    let pivot = i
    for (let r = i + 1; r < n; r++) if (Math.abs(m[r][i]) > Math.abs(m[pivot][i])) pivot = r
    if (m[pivot][i] === 0) return 0
    if (pivot !== i) { [m[i], m[pivot]] = [m[pivot], m[i]]; det = -det }
    det *= m[i][i]
    for (let r = i + 1; r < n; r++) {
      const f = m[r][i] / m[i][i]
      for (let c = i; c < n; c++) m[r][c] -= f * m[i][c]
    }
  }
  return det
}

// Matrix inverse via Gauss-Jordan elimination; returns null if singular.
function invert(m: number[][]): number[][] | null {
  const n = m.length
  // Build [m | I].
  const aug = m.map((row, i) => [...row, ...Array.from({ length: n }, (_, j) => (i === j ? 1 : 0))])
  for (let i = 0; i < n; i++) {
    let pivot = i
    for (let r = i + 1; r < n; r++) if (Math.abs(aug[r][i]) > Math.abs(aug[pivot][i])) pivot = r
    if (Math.abs(aug[pivot][i]) < 1e-12) return null
    if (pivot !== i) [aug[i], aug[pivot]] = [aug[pivot], aug[i]]
    const pv = aug[i][i]
    for (let c = 0; c < 2 * n; c++) aug[i][c] /= pv
    for (let r = 0; r < n; r++) {
      if (r === i) continue
      const f = aug[r][i]
      for (let c = 0; c < 2 * n; c++) aug[r][c] -= f * aug[i][c]
    }
  }
  return aug.map(row => row.slice(n))
}

// Silence unused-import lints for symbols kept for API parity / future use.
void colToIndex; void indexToCol; void toStr
