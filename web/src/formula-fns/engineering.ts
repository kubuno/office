// ── Excel Engineering & Database functions ───────────────────────────────────
// Adds Excel "Engineering" (base conversions, bitwise, complex numbers, ERF,
// CONVERT, DELTA/GESTEP) and "Database" (D-functions) families to the formula
// engine. All functions are registered into ENGINEERING_FNS.

import {
  type Fn,
  type Evaluator,
  type Node,
  type Value,
  type Scalar,
  type FErr,
  type Matrix,
  ERR,
  isErr,
  isMatrix,
  num,
  str,
  flatten,
  toNum,
  toStr,
  matchCriteria,
} from '../formula-engine'

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Read a single optional numeric argument, defaulting to `def` when absent. */
function optNum(ev: Evaluator, args: Node[], i: number, def: number): number | typeof ERR.VALUE {
  if (args.length <= i || args[i] == null) return def
  const n = num(ev, args[i])
  return isErr(n) ? ERR.VALUE : n
}

/** Coerce a value to an integer, rejecting non-integers like Excel does. */
function toInt(v: number): number | typeof ERR.NUM {
  if (!isFinite(v)) return ERR.NUM
  // Excel truncates toward zero for these conversions.
  return Math.trunc(v)
}

// ── Base conversions ──────────────────────────────────────────────────────────
// Excel uses a 10-digit two's-complement representation with a sign bit for
// BIN/OCT/HEX. Numbers range within those 10 "digits" of the source base.

/** Parse a string in `base` into a signed decimal, honoring Excel's 10-digit
 *  two's-complement sign bit. Returns ERR.NUM on invalid input/overflow. */
function parseBase(raw: string, base: 2 | 8 | 16): number | typeof ERR.NUM {
  const s = raw.trim().toUpperCase()
  if (s === '') return 0
  if (s.length > 10) return ERR.NUM
  const valid = base === 2 ? /^[01]+$/ : base === 8 ? /^[0-7]+$/ : /^[0-9A-F]+$/
  if (!valid.test(s)) return ERR.NUM
  // bits per digit: 1 (bin), 3 (oct), 4 (hex) → 10-digit width.
  const bits = base === 2 ? 10 : base === 8 ? 30 : 40
  let n = parseInt(s, base)
  if (!isFinite(n)) return ERR.NUM
  // Sign bit set (top bit of the 10-digit field) → negative value.
  const signBit = Math.pow(2, bits - 1)
  if (n >= signBit) n -= Math.pow(2, bits)
  return n
}

/** Format a signed decimal into `base` using Excel's 10-digit two's-complement
 *  width. `places` optionally zero-pads positive results. */
function formatBase(
  dec: number,
  base: 2 | 8 | 16,
  places: number | undefined,
): Scalar {
  const t = toInt(dec)
  if (isErr(t)) return t
  let n = t
  const bits = base === 2 ? 10 : base === 8 ? 30 : 40
  const max = Math.pow(2, bits - 1) - 1
  const min = -Math.pow(2, bits - 1)
  if (n > max || n < min) return ERR.NUM
  let out: string
  if (n < 0) {
    // Two's complement representation across the full digit width.
    out = (n + Math.pow(2, bits)).toString(base).toUpperCase()
    return out // negative results are always full-width, places is ignored
  }
  out = n.toString(base).toUpperCase()
  if (places != null) {
    const p = Math.trunc(places)
    if (p < 0 || p < out.length) return ERR.NUM
    out = out.padStart(p, '0')
  }
  return out
}

function convFn(
  fromBase: 2 | 8 | 16,
  toBase: 2 | 8 | 16 | 10,
): Fn {
  return (ev, a) => {
    const s = str(ev, a[0])
    const dec = parseBase(s, fromBase)
    if (isErr(dec)) return dec
    if (toBase === 10) return dec
    const places = a.length > 1 && a[1] != null ? optNum(ev, a, 1, NaN) : undefined
    if (places !== undefined && isErr(places)) return ERR.VALUE
    return formatBase(dec, toBase, places === undefined || isNaN(places) ? undefined : places)
  }
}

// ── Bitwise (non-negative integers < 2^48 per Excel) ──────────────────────────

function bitArg(ev: Evaluator, n: Node): number | typeof ERR.NUM {
  const v = num(ev, n)
  if (isErr(v)) return ERR.NUM
  const t = toInt(v)
  if (isErr(t)) return t
  if (t < 0 || t >= 281474976710656) return ERR.NUM // 2^48
  return t
}

/** Bitwise op over numbers that may exceed 32 bits (JS bit ops are 32-bit). */
function bitwise(x: number, y: number, op: (a: number, b: number) => number): number {
  // Split into high/low 24-bit halves to stay within safe integer math.
  const xl = x % 0x1000000, xh = Math.floor(x / 0x1000000)
  const yl = y % 0x1000000, yh = Math.floor(y / 0x1000000)
  return op(xh, yh) * 0x1000000 + op(xl, yl)
}

// ── ERF / ERFC (Abramowitz-Stegun approximation 7.1.26) ───────────────────────

function erf(x: number): number {
  const sign = x < 0 ? -1 : 1
  const ax = Math.abs(x)
  const t = 1 / (1 + 0.3275911 * ax)
  const y =
    1 -
    (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) *
      t *
      Math.exp(-ax * ax)
  return sign * y
}
function erfc(x: number): number {
  return 1 - erf(x)
}

// ── Complex numbers ───────────────────────────────────────────────────────────

type Complex = { re: number; im: number; suffix: 'i' | 'j' }

/** Parse an Excel complex string like "3+4i", "-2.5j", "5", "i", "-i". */
function parseComplex(s: string): Complex | typeof ERR.NUM {
  const raw = s.trim()
  if (raw === '') return { re: 0, im: 0, suffix: 'i' }
  const suffix: 'i' | 'j' = raw.includes('j') ? 'j' : 'i'
  // Pure real number.
  if (!raw.includes('i') && !raw.includes('j')) {
    const n = Number(raw)
    return isNaN(n) ? ERR.NUM : { re: n, im: 0, suffix }
  }
  // Strip the suffix then split into real + imaginary parts.
  const body = raw.replace(/[ij]/g, '')
  // Pure imaginary shortcuts: "i", "-i", "+i".
  if (body === '' || body === '+') return { re: 0, im: 1, suffix }
  if (body === '-') return { re: 0, im: -1, suffix }
  // Find the split point: a +/- that is not at position 0 and not an exponent sign.
  let splitAt = -1
  for (let k = body.length - 1; k > 0; k--) {
    const c = body[k]
    if ((c === '+' || c === '-') && body[k - 1] !== 'e' && body[k - 1] !== 'E') {
      splitAt = k
      break
    }
  }
  if (splitAt === -1) {
    // Only an imaginary part, e.g. "4i" → body "4", or "-3i" handled here too.
    let imStr = body
    if (imStr === '+' || imStr === '') imStr = '1'
    else if (imStr === '-') imStr = '-1'
    const im = Number(imStr)
    return isNaN(im) ? ERR.NUM : { re: 0, im, suffix }
  }
  const reStr = body.slice(0, splitAt)
  let imStr = body.slice(splitAt)
  if (imStr === '+' || imStr === '') imStr = '1'
  else if (imStr === '-') imStr = '-1'
  const re = Number(reStr)
  const im = Number(imStr)
  return isNaN(re) || isNaN(im) ? ERR.NUM : { re, im, suffix }
}

/** Format a complex number back into Excel's "a+bi" string form. */
function fmtComplex(re: number, im: number, suffix: 'i' | 'j'): string {
  const r = Math.round(re * 1e12) / 1e12
  const i = Math.round(im * 1e12) / 1e12
  if (i === 0) return String(r)
  const imPart =
    i === 1 ? suffix : i === -1 ? `-${suffix}` : `${i}${suffix}`
  if (r === 0) return imPart
  const sign = i < 0 ? '' : '+'
  return `${r}${sign}${imPart}`
}

function getComplex(ev: Evaluator, n: Node): Complex | typeof ERR.NUM {
  return parseComplex(str(ev, n))
}

// ── CONVERT unit subset ───────────────────────────────────────────────────────
// Each non-temperature unit maps to a base factor (value × factor = base unit).
// Temperature is handled specially (affine, not multiplicative).

const CONVERT_UNITS: Record<string, { group: string; factor: number }> = {
  // Mass (base: gram)
  g: { group: 'mass', factor: 1 },
  kg: { group: 'mass', factor: 1000 },
  lbm: { group: 'mass', factor: 453.59237 },
  // Distance (base: meter)
  m: { group: 'dist', factor: 1 },
  km: { group: 'dist', factor: 1000 },
  mi: { group: 'dist', factor: 1609.344 },
  ft: { group: 'dist', factor: 0.3048 },
  in: { group: 'dist', factor: 0.0254 },
  // Time (base: second)
  sec: { group: 'time', factor: 1 },
  s: { group: 'time', factor: 1 },
  min: { group: 'time', factor: 60 },
  hr: { group: 'time', factor: 3600 },
  day: { group: 'time', factor: 86400 },
}

const TEMP_UNITS = new Set(['C', 'F', 'K', 'cel', 'fah', 'kel'])

/** Convert a temperature value to Kelvin from its source unit. */
function tempToK(v: number, unit: string): number | null {
  switch (unit) {
    case 'C':
    case 'cel':
      return v + 273.15
    case 'F':
    case 'fah':
      return ((v - 32) * 5) / 9 + 273.15
    case 'K':
    case 'kel':
      return v
    default:
      return null
  }
}
/** Convert a temperature value from Kelvin to the target unit. */
function tempFromK(k: number, unit: string): number | null {
  switch (unit) {
    case 'C':
    case 'cel':
      return k - 273.15
    case 'F':
    case 'fah':
      return ((k - 273.15) * 9) / 5 + 32
    case 'K':
    case 'kel':
      return k
    default:
      return null
  }
}

// ── Database (D-functions) helpers ────────────────────────────────────────────

/** Resolve the database `field` argument to a 0-based column index.
 *  `field` may be a column header (string) or a 1-based numeric index. */
function resolveField(field: Scalar, headers: Scalar[]): number {
  if (typeof field === 'number') {
    const idx = Math.trunc(field) - 1
    return idx >= 0 && idx < headers.length ? idx : -1
  }
  const want = toStr(field).toLowerCase()
  for (let i = 0; i < headers.length; i++) {
    if (toStr(headers[i]).toLowerCase() === want) return i
  }
  return -1
}

/** Iterate database rows that satisfy the criteria range, yielding the value of
 *  the target field for each match. Criteria: OR across rows, AND within a row. */
function dbMatches(database: Matrix, fieldIdx: number, criteria: Matrix): Scalar[] {
  const dbHeaders = database[0] ?? []
  const critHeaders = criteria[0] ?? []
  const out: Scalar[] = []
  for (let r = 1; r < database.length; r++) {
    const row = database[r]
    let rowOk = false
    // Each criteria row is an OR alternative.
    for (let cr = 1; cr < criteria.length; cr++) {
      const critRow = criteria[cr]
      let allOk = true
      for (let cc = 0; cc < critHeaders.length; cc++) {
        const crit = critRow[cc]
        if (crit === '' || crit == null) continue // empty criterion = no constraint
        // Map criteria column header to a database column.
        const hname = toStr(critHeaders[cc]).toLowerCase()
        const dbCol = dbHeaders.findIndex((h) => toStr(h).toLowerCase() === hname)
        if (dbCol < 0) {
          allOk = false
          break
        }
        if (!matchCriteria(row[dbCol], crit)) {
          allOk = false
          break
        }
      }
      if (allOk) {
        rowOk = true
        break
      }
    }
    if (rowOk) out.push(row[fieldIdx])
  }
  return out
}

/** Build a D-function from a numeric aggregator over the matching field values. */
function dFn(agg: (vals: number[]) => Scalar): Fn {
  return (ev, a) => {
    const database = ev.eval(a[0])
    const criteria = ev.eval(a[2])
    if (!isMatrix(database) || !isMatrix(criteria)) return ERR.VALUE
    const headers = database[0] ?? []
    const fieldVal = ev.scalar(a[1])
    if (isErr(fieldVal)) return fieldVal
    const fieldIdx = resolveField(fieldVal, headers)
    if (fieldIdx < 0) return ERR.VALUE
    const cells = dbMatches(database, fieldIdx, criteria)
    // Numeric aggregators ignore non-numeric cells.
    const nums: number[] = []
    for (const c of cells) {
      if (isErr(c)) return c
      if (typeof c === 'number') nums.push(c)
      else if (typeof c === 'boolean') nums.push(c ? 1 : 0)
      else if (typeof c === 'string' && c !== '' && !isNaN(Number(c))) nums.push(Number(c))
    }
    return agg(nums)
  }
}

const variance = (vals: number[], pop: boolean): number | FErr => {
  const n = vals.length
  if (n < (pop ? 1 : 2)) return ERR.NUM
  const mean = vals.reduce((x, y) => x + y, 0) / n
  const ss = vals.reduce((s, v) => s + (v - mean) * (v - mean), 0)
  return ss / (pop ? n : n - 1)
}

// ── Registry ──────────────────────────────────────────────────────────────────

export const ENGINEERING_FNS: Record<string, Fn> = {
  // ── Base conversions ──
  BIN2DEC: convFn(2, 10),
  BIN2HEX: convFn(2, 16),
  BIN2OCT: convFn(2, 8),
  OCT2DEC: convFn(8, 10),
  OCT2BIN: convFn(8, 2),
  OCT2HEX: convFn(8, 16),
  HEX2DEC: convFn(16, 10),
  HEX2BIN: convFn(16, 2),
  HEX2OCT: convFn(16, 8),
  DEC2BIN: (ev, a) => {
    const v = num(ev, a[0])
    if (isErr(v)) return v
    const places = a.length > 1 && a[1] != null ? optNum(ev, a, 1, NaN) : undefined
    if (places !== undefined && isErr(places)) return ERR.VALUE
    return formatBase(v, 2, places === undefined || isNaN(places) ? undefined : places)
  },
  DEC2OCT: (ev, a) => {
    const v = num(ev, a[0])
    if (isErr(v)) return v
    const places = a.length > 1 && a[1] != null ? optNum(ev, a, 1, NaN) : undefined
    if (places !== undefined && isErr(places)) return ERR.VALUE
    return formatBase(v, 8, places === undefined || isNaN(places) ? undefined : places)
  },
  DEC2HEX: (ev, a) => {
    const v = num(ev, a[0])
    if (isErr(v)) return v
    const places = a.length > 1 && a[1] != null ? optNum(ev, a, 1, NaN) : undefined
    if (places !== undefined && isErr(places)) return ERR.VALUE
    return formatBase(v, 16, places === undefined || isNaN(places) ? undefined : places)
  },

  // ── Bitwise ──
  BITAND: (ev, a) => {
    const x = bitArg(ev, a[0]); if (isErr(x)) return x
    const y = bitArg(ev, a[1]); if (isErr(y)) return y
    return bitwise(x, y, (p, q) => p & q)
  },
  BITOR: (ev, a) => {
    const x = bitArg(ev, a[0]); if (isErr(x)) return x
    const y = bitArg(ev, a[1]); if (isErr(y)) return y
    return bitwise(x, y, (p, q) => p | q)
  },
  BITXOR: (ev, a) => {
    const x = bitArg(ev, a[0]); if (isErr(x)) return x
    const y = bitArg(ev, a[1]); if (isErr(y)) return y
    return bitwise(x, y, (p, q) => p ^ q)
  },
  BITLSHIFT: (ev, a) => {
    const x = bitArg(ev, a[0]); if (isErr(x)) return x
    const sh = num(ev, a[1]); if (isErr(sh)) return sh
    const s = Math.trunc(sh)
    if (Math.abs(s) > 53) return ERR.NUM
    const r = s >= 0 ? x * Math.pow(2, s) : Math.floor(x / Math.pow(2, -s))
    return r >= 281474976710656 ? ERR.NUM : r
  },
  BITRSHIFT: (ev, a) => {
    const x = bitArg(ev, a[0]); if (isErr(x)) return x
    const sh = num(ev, a[1]); if (isErr(sh)) return sh
    const s = Math.trunc(sh)
    if (Math.abs(s) > 53) return ERR.NUM
    const r = s >= 0 ? Math.floor(x / Math.pow(2, s)) : x * Math.pow(2, -s)
    return r >= 281474976710656 ? ERR.NUM : r
  },

  // ── DELTA / GESTEP ──
  DELTA: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const y = optNum(ev, a, 1, 0); if (isErr(y)) return y
    return x === y ? 1 : 0
  },
  GESTEP: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    const step = optNum(ev, a, 1, 0); if (isErr(step)) return step
    return x >= step ? 1 : 0
  },

  // ── CONVERT (subset of common units; unknown units → #N/A) ──
  CONVERT: (ev, a) => {
    const v = num(ev, a[0]); if (isErr(v)) return v
    const from = str(ev, a[1])
    const to = str(ev, a[2])
    // Temperature uses affine conversions, handled separately.
    if (TEMP_UNITS.has(from) || TEMP_UNITS.has(to)) {
      if (!TEMP_UNITS.has(from) || !TEMP_UNITS.has(to)) return ERR.NA
      const k = tempToK(v, from)
      if (k == null) return ERR.NA
      const out = tempFromK(k, to)
      return out == null ? ERR.NA : out
    }
    const uf = CONVERT_UNITS[from]
    const ut = CONVERT_UNITS[to]
    if (!uf || !ut) return ERR.NA
    if (uf.group !== ut.group) return ERR.NA // incompatible units
    return (v * uf.factor) / ut.factor
  },

  // ── ERF / ERFC ──
  ERF: (ev, a) => {
    const lo = num(ev, a[0]); if (isErr(lo)) return lo
    if (a.length > 1 && a[1] != null) {
      const hi = num(ev, a[1]); if (isErr(hi)) return hi
      return erf(hi) - erf(lo) // two-argument form: ERF(lower, upper)
    }
    return erf(lo)
  },
  ERFC: (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    return erfc(x)
  },
  'ERF.PRECISE': (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    return erf(x)
  },
  'ERFC.PRECISE': (ev, a) => {
    const x = num(ev, a[0]); if (isErr(x)) return x
    return erfc(x)
  },

  // ── Complex numbers ──
  COMPLEX: (ev, a) => {
    const re = num(ev, a[0]); if (isErr(re)) return re
    const im = num(ev, a[1]); if (isErr(im)) return im
    const suffix = a.length > 2 && a[2] != null ? str(ev, a[2]) : 'i'
    if (suffix !== 'i' && suffix !== 'j') return ERR.VALUE
    return fmtComplex(re, im, suffix)
  },
  IMABS: (ev, a) => {
    const c = getComplex(ev, a[0]); if (isErr(c)) return c
    return Math.sqrt(c.re * c.re + c.im * c.im)
  },
  IMREAL: (ev, a) => {
    const c = getComplex(ev, a[0]); if (isErr(c)) return c
    return c.re
  },
  IMAGINARY: (ev, a) => {
    const c = getComplex(ev, a[0]); if (isErr(c)) return c
    return c.im
  },
  IMARGUMENT: (ev, a) => {
    const c = getComplex(ev, a[0]); if (isErr(c)) return c
    if (c.re === 0 && c.im === 0) return ERR.DIV0
    return Math.atan2(c.im, c.re)
  },
  IMCONJUGATE: (ev, a) => {
    const c = getComplex(ev, a[0]); if (isErr(c)) return c
    return fmtComplex(c.re, -c.im, c.suffix)
  },
  IMSUM: (ev, a) => {
    let re = 0, im = 0
    let suffix: 'i' | 'j' = 'i'
    for (const arg of a) {
      for (const s of flatten(ev.eval(arg))) {
        if (isErr(s)) return s
        const c = parseComplex(toStr(s)); if (isErr(c)) return c
        re += c.re; im += c.im; suffix = c.suffix
      }
    }
    return fmtComplex(re, im, suffix)
  },
  IMSUB: (ev, a) => {
    const x = getComplex(ev, a[0]); if (isErr(x)) return x
    const y = getComplex(ev, a[1]); if (isErr(y)) return y
    return fmtComplex(x.re - y.re, x.im - y.im, x.suffix)
  },
  IMPRODUCT: (ev, a) => {
    let re = 1, im = 0
    let suffix: 'i' | 'j' = 'i'
    for (const arg of a) {
      for (const s of flatten(ev.eval(arg))) {
        if (isErr(s)) return s
        const c = parseComplex(toStr(s)); if (isErr(c)) return c
        // (re + im i)(c.re + c.im i)
        const nr = re * c.re - im * c.im
        const ni = re * c.im + im * c.re
        re = nr; im = ni; suffix = c.suffix
      }
    }
    return fmtComplex(re, im, suffix)
  },
  IMDIV: (ev, a) => {
    const x = getComplex(ev, a[0]); if (isErr(x)) return x
    const y = getComplex(ev, a[1]); if (isErr(y)) return y
    const den = y.re * y.re + y.im * y.im
    if (den === 0) return ERR.NUM
    const re = (x.re * y.re + x.im * y.im) / den
    const im = (x.im * y.re - x.re * y.im) / den
    return fmtComplex(re, im, x.suffix)
  },

  // ── Database (D-functions) ──
  DSUM: dFn((v) => v.reduce((x, y) => x + y, 0)),
  DAVERAGE: dFn((v) => (v.length ? v.reduce((x, y) => x + y, 0) / v.length : ERR.DIV0)),
  DCOUNT: dFn((v) => v.length),
  DMAX: dFn((v) => (v.length ? Math.max(...v) : 0)),
  DMIN: dFn((v) => (v.length ? Math.min(...v) : 0)),
  DPRODUCT: dFn((v) => (v.length ? v.reduce((x, y) => x * y, 1) : 0)),
  DSTDEV: dFn((v) => { const r = variance(v, false); return isErr(r) ? r : Math.sqrt(r) }),
  DSTDEVP: dFn((v) => { const r = variance(v, true); return isErr(r) ? r : Math.sqrt(r) }),
  DVAR: dFn((v) => variance(v, false)),
  DVARP: dFn((v) => variance(v, true)),
  // DCOUNTA counts non-blank cells (any type), so it cannot reuse dFn's numeric filter.
  DCOUNTA: (ev, a) => {
    const database = ev.eval(a[0])
    const criteria = ev.eval(a[2])
    if (!isMatrix(database) || !isMatrix(criteria)) return ERR.VALUE
    const headers = database[0] ?? []
    const fieldVal = ev.scalar(a[1])
    if (isErr(fieldVal)) return fieldVal
    const fieldIdx = resolveField(fieldVal, headers)
    if (fieldIdx < 0) return ERR.VALUE
    const cells = dbMatches(database, fieldIdx, criteria)
    let count = 0
    for (const c of cells) if (!(c === '' || c == null)) count++
    return count
  },
  // DGET returns the single matching value, erroring on 0 or >1 matches.
  DGET: (ev, a) => {
    const database = ev.eval(a[0])
    const criteria = ev.eval(a[2])
    if (!isMatrix(database) || !isMatrix(criteria)) return ERR.VALUE
    const headers = database[0] ?? []
    const fieldVal = ev.scalar(a[1])
    if (isErr(fieldVal)) return fieldVal
    const fieldIdx = resolveField(fieldVal, headers)
    if (fieldIdx < 0) return ERR.VALUE
    const cells = dbMatches(database, fieldIdx, criteria)
    if (cells.length === 0) return ERR.VALUE
    if (cells.length > 1) return ERR.NUM
    return cells[0] ?? ERR.VALUE
  },
}
