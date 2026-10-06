// ── Miscellaneous Excel-compatible functions ─────────────────────────────────
// Grab-bag of functions that do not fit the other category files:
//   - Text/regex (Excel 2024 family): REGEXTEST, REGEXEXTRACT, REGEXREPLACE
//   - Array shaping: WRAPROWS, WRAPCOLS
//   - Lookup-ish: HYPERLINK
//   - Information: CELL, INFO
//   - Web (browser-safe): ENCODEURL, FILTERXML
//   - AGGREGATE
//
// Conventions (see ../formula-engine):
//   Fn = (ev: Evaluator, args: Node[]) => Value
//   - propagate FErr where required (see AGGREGATE rules);
//   - bad argument type → ERR.VALUE;
//   - out-of-domain → ERR.NUM.
//
// NOTE: WEBSERVICE is intentionally OMITTED. It performs an arbitrary outbound
// HTTP GET, which is incompatible with the sandboxed, self-hosted security model
// of this project (no uncontrolled network egress from the formula engine).
//
// Several Information functions are necessarily approximated because the formula
// engine receives only an AST (Node[]) and a flat SheetData map — it has no
// layout, column-width, formatting or "hidden row" metadata. Approximations are
// documented inline at each call site.

import {
  type Fn,
  type Evaluator,
  type Node,
  type Value,
  type Scalar,
  type Matrix,
  type FErr,
  ERR,
  isErr,
  isMatrix,
  num,
  str,
  flatten,
  toStr,
} from '../formula-engine'

// ── Internal helpers ──────────────────────────────────────────────────────────

// Coerce any Value to a Matrix (single scalar → 1×1).
const asMatrix = (v: Value): Matrix => (isMatrix(v) ? v : [[v]])

// Build RegExp flags from an Excel `case_sensitivity` argument.
// 0 (default) = case-sensitive, 1 = case-insensitive.
function buildFlags(caseInsensitive: boolean, global: boolean): string {
  let f = ''
  if (global) f += 'g'
  if (caseInsensitive) f += 'i'
  return f
}

// Compile a RegExp, returning ERR.VALUE on an invalid pattern.
function compileRegex(pattern: string, flags: string): RegExp | FErr {
  try {
    return new RegExp(pattern, flags)
  } catch {
    return ERR.VALUE
  }
}

// Read an optional numeric argument with a default; propagate FErr.
function optNum(ev: Evaluator, args: Node[], i: number, def: number): number | FErr {
  if (i >= args.length) return def
  const n = num(ev, args[i])
  return n
}

// ── Text / regex (Excel 2024 family — JS RegExp semantics) ────────────────────

// REGEXTEST(text, pattern, [case_sensitivity]) → boolean.
const REGEXTEST: Fn = (ev, a) => {
  if (a.length < 2) return ERR.VALUE
  const text = str(ev, a[0])
  const pattern = str(ev, a[1])
  const cs = optNum(ev, a, 2, 0)
  if (isErr(cs)) return cs
  const re = compileRegex(pattern, buildFlags(cs === 1, false))
  if (isErr(re)) return re
  return re.test(text)
}

// REGEXEXTRACT(text, pattern, [return_mode], [case_sensitivity])
//   return_mode 0 = first match (default)
//   return_mode 1 = all matches (Matrix column)
//   return_mode 2 = capture groups of first match (Matrix column)
//   no match → ERR.NA
const REGEXEXTRACT: Fn = (ev, a) => {
  if (a.length < 2) return ERR.VALUE
  const text = str(ev, a[0])
  const pattern = str(ev, a[1])
  const mode = optNum(ev, a, 2, 0)
  if (isErr(mode)) return mode
  const cs = optNum(ev, a, 3, 0)
  if (isErr(cs)) return cs
  const insensitive = cs === 1

  if (mode === 1) {
    // All matches as a column vector.
    const re = compileRegex(pattern, buildFlags(insensitive, true))
    if (isErr(re)) return re
    const out: Scalar[][] = []
    let m: RegExpExecArray | null
    while ((m = re.exec(text)) !== null) {
      out.push([m[0]])
      // Guard against zero-width matches causing an infinite loop.
      if (m.index === re.lastIndex) re.lastIndex++
    }
    return out.length ? out : ERR.NA
  }

  if (mode === 2) {
    // Capture groups of the first match as a column vector.
    const re = compileRegex(pattern, buildFlags(insensitive, false))
    if (isErr(re)) return re
    const m = re.exec(text)
    if (!m) return ERR.NA
    const groups = m.slice(1)
    if (!groups.length) return m[0] // no capture groups → whole match
    return groups.map((g) => [g ?? ''])
  }

  // mode 0 (or anything else) = first whole match.
  const re = compileRegex(pattern, buildFlags(insensitive, false))
  if (isErr(re)) return re
  const m = re.exec(text)
  return m ? m[0] : ERR.NA
}

// REGEXREPLACE(text, pattern, replacement, [occurrence], [case_sensitivity])
//   occurrence 0 = all matches (default), N = replace only the Nth match.
const REGEXREPLACE: Fn = (ev, a) => {
  if (a.length < 3) return ERR.VALUE
  const text = str(ev, a[0])
  const pattern = str(ev, a[1])
  const replacement = str(ev, a[2])
  const occurrence = optNum(ev, a, 3, 0)
  if (isErr(occurrence)) return occurrence
  const cs = optNum(ev, a, 4, 0)
  if (isErr(cs)) return cs
  const insensitive = cs === 1

  if (occurrence === 0) {
    const re = compileRegex(pattern, buildFlags(insensitive, true))
    if (isErr(re)) return re
    return text.replace(re, replacement)
  }

  // Replace only the Nth (1-based) match.
  const re = compileRegex(pattern, buildFlags(insensitive, true))
  if (isErr(re)) return re
  let count = 0
  const out = text.replace(re, (match) => {
    count++
    return count === occurrence ? replacement : match
  })
  return out
}

// ── Array shaping ─────────────────────────────────────────────────────────────

// WRAPROWS(vector, wrap_count, [pad_with])
// Reshape a 1-D vector into rows of `wrap_count`, padding the last row.
const WRAPROWS: Fn = (ev, a) => {
  if (a.length < 2) return ERR.VALUE
  const data = flatten(ev.eval(a[0]))
  const wrap = num(ev, a[1])
  if (isErr(wrap)) return wrap
  const w = Math.trunc(wrap)
  if (w < 1) return ERR.NUM
  const pad: Scalar = a.length > 2 ? ev.scalar(a[2]) : ERR.NA
  const rows: Matrix = []
  for (let i = 0; i < data.length; i += w) {
    const row: Scalar[] = []
    for (let j = 0; j < w; j++) {
      const idx = i + j
      row.push(idx < data.length ? data[idx] : pad)
    }
    rows.push(row)
  }
  return rows.length ? rows : ERR.VALUE
}

// WRAPCOLS(vector, wrap_count, [pad_with])
// Reshape a 1-D vector column-major into columns of `wrap_count`.
const WRAPCOLS: Fn = (ev, a) => {
  if (a.length < 2) return ERR.VALUE
  const data = flatten(ev.eval(a[0]))
  const wrap = num(ev, a[1])
  if (isErr(wrap)) return wrap
  const h = Math.trunc(wrap)
  if (h < 1) return ERR.NUM
  const pad: Scalar = a.length > 2 ? ev.scalar(a[2]) : ERR.NA
  const cols = Math.ceil(data.length / h)
  if (cols < 1) return ERR.VALUE
  // Build a matrix of `h` rows × `cols` columns, filling column by column.
  const out: Matrix = []
  for (let r = 0; r < h; r++) out.push(new Array<Scalar>(cols).fill(pad))
  for (let k = 0; k < data.length; k++) {
    const col = Math.floor(k / h)
    const row = k % h
    out[row][col] = data[k]
  }
  return out
}

// ── Lookup-ish ────────────────────────────────────────────────────────────────

// HYPERLINK(link_location, [friendly_name])
// In a non-navigating cell context we simply return the display text:
// the friendly name if provided, otherwise the link itself.
const HYPERLINK: Fn = (ev, a) => {
  if (a.length < 1) return ERR.VALUE
  const link = str(ev, a[0])
  if (a.length > 1) {
    const friendly = ev.scalar(a[1])
    if (isErr(friendly)) return friendly
    return toStr(friendly)
  }
  return link
}

// ── Information ───────────────────────────────────────────────────────────────

// Defensively extract a top-left A1 address from a reference Node, if it is a
// {k:'ref'} or {k:'range'}. Returns '' when no address can be determined
// (e.g. a literal or computed argument) — callers then fall back gracefully.
function refAddress(node: Node | undefined): string {
  if (!node) return ''
  const n = node as unknown as Record<string, unknown>
  if (n.k === 'ref' && typeof n.v === 'string') return n.v
  if (n.k === 'range' && typeof n.a === 'string') return n.a
  return ''
}

// Split an A1 address into column letters and 1-based row number.
function splitA1(addr: string): { col: string; row: number } | null {
  const m = /^([A-Za-z]+)(\d+)$/.exec(addr)
  if (!m) return null
  return { col: m[1].toUpperCase(), row: parseInt(m[2], 10) }
}

// Convert column letters to a 1-based column index.
function colNumber(col: string): number {
  let n = 0
  for (const ch of col) n = n * 26 + (ch.charCodeAt(0) - 64)
  return n
}

// CELL(info_type, [reference])
// Supported info_types (computed without layout metadata):
//   "address"      → absolute A1 address of the reference, "" if unknown (approx)
//   "col"          → 1-based column index, 0 if unknown (approx)
//   "row"          → 1-based row number, 0 if unknown (approx)
//   "contents"     → the (top-left) value of the reference
//   "type"         → "b" blank, "l" label (text), "v" value (everything else)
//   "prefix"       → text alignment prefix; we cannot know alignment → "" (approx)
//   "width"        → best-effort fixed default column width 8 (approx; no layout)
//   "format"       → "G" (General) — number-format metadata not tracked (approx)
//   "color"        → 0 (no negative-in-color formatting tracked) (approx)
//   "parentheses"  → 0 (formatting not tracked) (approx)
//   "protect"      → 1 (cells are protected by default in Excel) (approx)
// Any other info_type → ERR.VALUE.
const CELL: Fn = (ev, a) => {
  if (a.length < 1) return ERR.VALUE
  const infoType = str(ev, a[0]).toLowerCase()
  const refNode = a.length > 1 ? a[1] : undefined
  const addr = refAddress(refNode)
  const parts = addr ? splitA1(addr) : null

  switch (infoType) {
    case 'address':
      // Absolute form $COL$ROW. Approximated: returns "" if the argument is not
      // a plain ref/range (we cannot synthesise an address for a literal).
      return parts ? `$${parts.col}$${parts.row}` : ''
    case 'col':
      // Approximated: 0 when no address is available.
      return parts ? colNumber(parts.col) : 0
    case 'row':
      // Approximated: 0 when no address is available.
      return parts ? parts.row : 0
    case 'contents': {
      // Top-left value of the reference (or of the literal argument).
      if (refNode === undefined) return ''
      const v = ev.eval(refNode)
      const s = isMatrix(v) ? v[0]?.[0] ?? '' : v
      return s
    }
    case 'type': {
      if (refNode === undefined) return 'b'
      const v = ev.eval(refNode)
      const s = isMatrix(v) ? v[0]?.[0] ?? '' : v
      if (s === '' || s == null) return 'b'
      if (typeof s === 'string') return 'l'
      return 'v'
    }
    case 'prefix':
      // Alignment prefix is unknown without layout → "" (approx).
      return ''
    case 'width':
      // No column-width metadata → Excel default 8 (approx).
      return 8
    case 'format':
      // Number format not tracked → General (approx).
      return 'G'
    case 'color':
      // Negative-in-color formatting not tracked (approx).
      return 0
    case 'parentheses':
      // Formatting not tracked (approx).
      return 0
    case 'protect':
      // Cells are protected by default in Excel (approx).
      return 1
    default:
      return ERR.VALUE
  }
}

// INFO(type) → environment information (fixed, browser-safe values).
const INFO: Fn = (ev, a) => {
  if (a.length < 1) return ERR.VALUE
  const type = str(ev, a[0]).toLowerCase()
  switch (type) {
    case 'numfile':
      return 1
    case 'recalc':
      return 'Automatic'
    case 'system':
      return 'pcdos'
    case 'release':
      return '16.0'
    case 'origin':
      return '$A:$A1'
    case 'osversion':
      return 'Windows (32-bit) NT 10.00'
    case 'directory':
      return ''
    default:
      return ERR.VALUE
  }
}

// ── Web (browser-safe only) ───────────────────────────────────────────────────

// ENCODEURL(text) → URL-encode using encodeURIComponent (DOM-safe).
const ENCODEURL: Fn = (ev, a) => {
  if (a.length < 1) return ERR.VALUE
  const text = str(ev, a[0])
  return encodeURIComponent(text)
}

// FILTERXML(xml, xpath)
// Parse the XML with DOMParser and evaluate the XPath with document.evaluate.
// A single matched node → its text content (string). A node set → a Matrix
// column. Parse/XPath errors → ERR.VALUE. In a non-DOM environment (SSR,
// Node test harness) where DOMParser/document are unavailable → ERR.VALUE.
const FILTERXML: Fn = (ev, a) => {
  if (a.length < 2) return ERR.VALUE
  const xml = str(ev, a[0])
  const xpath = str(ev, a[1])

  // Guard: DOM APIs must exist (browser only).
  const g = globalThis as unknown as {
    DOMParser?: typeof DOMParser
    document?: Document
    XPathResult?: typeof XPathResult
  }
  if (typeof g.DOMParser === 'undefined' || typeof g.document === 'undefined') {
    return ERR.VALUE
  }

  try {
    const parser = new g.DOMParser()
    const doc = parser.parseFromString(xml, 'application/xml')
    // DOMParser reports malformed XML via a <parsererror> element.
    if (doc.getElementsByTagName('parsererror').length > 0) return ERR.VALUE

    const result = g.document.evaluate(
      xpath,
      doc,
      null,
      // ORDERED_NODE_SNAPSHOT_TYPE = 7
      (g.XPathResult ? g.XPathResult.ORDERED_NODE_SNAPSHOT_TYPE : 7),
      null,
    )
    const len = result.snapshotLength
    if (len === 0) return ERR.VALUE
    if (len === 1) {
      const node = result.snapshotItem(0)
      return node ? (node.textContent ?? '') : ''
    }
    const out: Matrix = []
    for (let i = 0; i < len; i++) {
      const node = result.snapshotItem(i)
      out.push([node ? (node.textContent ?? '') : ''])
    }
    return out
  } catch {
    return ERR.VALUE
  }
}

// ── AGGREGATE ─────────────────────────────────────────────────────────────────

// Sort a numeric array ascending (numeric, not lexicographic).
const sortAsc = (xs: number[]): number[] => [...xs].sort((x, y) => x - y)

// Linear-interpolation percentile (PERCENTILE.INC semantics), p in [0,1].
function percentileInc(sorted: number[], p: number): number | FErr {
  if (!sorted.length || p < 0 || p > 1) return ERR.NUM
  if (sorted.length === 1) return sorted[0]
  const rank = p * (sorted.length - 1)
  const lo = Math.floor(rank)
  const frac = rank - lo
  if (lo + 1 >= sorted.length) return sorted[lo]
  return sorted[lo] + frac * (sorted[lo + 1] - sorted[lo])
}

// Exclusive percentile (PERCENTILE.EXC semantics), p in (0,1).
function percentileExc(sorted: number[], p: number): number | FErr {
  const n = sorted.length
  if (!n) return ERR.NUM
  // Valid range for EXC is 1/(n+1) .. n/(n+1).
  if (p <= 0 || p >= 1) return ERR.NUM
  const rank = p * (n + 1) - 1
  if (rank < 0 || rank > n - 1) return ERR.NUM
  const lo = Math.floor(rank)
  const frac = rank - lo
  if (lo + 1 >= n) return sorted[lo]
  return sorted[lo] + frac * (sorted[lo + 1] - sorted[lo])
}

// Compute one of the 19 AGGREGATE function numbers over a numeric array.
// `k` is only consulted by function_num 14..19 (LARGE/SMALL/percentile/quartile).
function aggregateCompute(fnum: number, xs: number[], k: number): Scalar {
  const n = xs.length
  const sum = xs.reduce((s, x) => s + x, 0)
  const mean = n ? sum / n : 0
  // Variance helpers.
  const ssd = xs.reduce((s, x) => s + (x - mean) * (x - mean), 0)
  switch (fnum) {
    case 1: // AVERAGE
      return n ? sum / n : ERR.DIV0
    case 2: // COUNT (numeric values already filtered to numbers)
      return n
    case 3: // COUNTA — count of non-empty values; here data is numeric only
      return n
    case 4: // MAX
      return n ? Math.max(...xs) : 0
    case 5: // MIN
      return n ? Math.min(...xs) : 0
    case 6: // PRODUCT
      return xs.reduce((s, x) => s * x, 1)
    case 7: // STDEV.S (sample)
      return n > 1 ? Math.sqrt(ssd / (n - 1)) : ERR.DIV0
    case 8: // STDEV.P (population)
      return n ? Math.sqrt(ssd / n) : ERR.DIV0
    case 9: // SUM
      return sum
    case 10: // VAR.S (sample)
      return n > 1 ? ssd / (n - 1) : ERR.DIV0
    case 11: // VAR.P (population)
      return n ? ssd / n : ERR.DIV0
    case 12: { // MEDIAN
      if (!n) return ERR.NUM
      const s = sortAsc(xs)
      const m = Math.floor(s.length / 2)
      return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
    }
    case 13: { // MODE.SNGL
      if (!n) return ERR.NA
      const counts = new Map<number, number>()
      let best = xs[0]
      let bestCount = 0
      for (const x of xs) {
        const c = (counts.get(x) ?? 0) + 1
        counts.set(x, c)
        if (c > bestCount) {
          bestCount = c
          best = x
        }
      }
      return bestCount <= 1 ? ERR.NA : best
    }
    case 14: { // LARGE
      const kk = Math.trunc(k)
      if (kk < 1 || kk > n) return ERR.NUM
      const s = sortAsc(xs)
      return s[n - kk]
    }
    case 15: { // SMALL
      const kk = Math.trunc(k)
      if (kk < 1 || kk > n) return ERR.NUM
      const s = sortAsc(xs)
      return s[kk - 1]
    }
    case 16: // PERCENTILE.INC
      return percentileInc(sortAsc(xs), k)
    case 17: { // QUARTILE.INC (k = quartile 0..4 → percentile k/4)
      const q = Math.trunc(k)
      if (q < 0 || q > 4) return ERR.NUM
      return percentileInc(sortAsc(xs), q / 4)
    }
    case 18: // PERCENTILE.EXC
      return percentileExc(sortAsc(xs), k)
    case 19: { // QUARTILE.EXC (k = quartile 0..4 → percentile k/4)
      const q = Math.trunc(k)
      if (q < 1 || q > 3) return ERR.NUM // EXC quartiles are 1..3
      return percentileExc(sortAsc(xs), q / 4)
    }
    default:
      return ERR.VALUE
  }
}

// AGGREGATE(function_num, options, ref1, [ref2], ...) OR
// AGGREGATE(function_num, options, array, [k])
//
// options control what to ignore:
//   0/4: nothing                1/5: hidden rows
//   2/6: errors                 3/7: hidden rows + errors
// The engine has NO hidden-row information, so hidden-row handling is a no-op
// (treated as "ignore nothing extra"). We DO honour the "ignore errors" options
// (2, 3, 6, 7) by filtering FErr from the data. For options 0, 1, 4, 5 we
// propagate the first FErr encountered. (Options 5/6/7 also ignore nested
// SUBTOTAL/AGGREGATE — not applicable here.)
const AGGREGATE: Fn = (ev, a) => {
  if (a.length < 3) return ERR.VALUE
  const fnum = num(ev, a[0])
  if (isErr(fnum)) return fnum
  const opt = num(ev, a[1])
  if (isErr(opt)) return opt
  const f = Math.trunc(fnum)
  const o = Math.trunc(opt)
  if (f < 1 || f > 19) return ERR.VALUE
  if (o < 0 || o > 7) return ERR.VALUE

  const ignoreErrors = o === 2 || o === 3 || o === 6 || o === 7
  const needsK = f >= 14 // 14..19 take a trailing k argument

  // Determine which args are data and which (if any) is the trailing k.
  let dataArgs: Node[]
  let k = 0
  if (needsK) {
    if (a.length < 4) return ERR.VALUE
    const kVal = num(ev, a[a.length - 1])
    if (isErr(kVal)) return kVal
    k = kVal
    dataArgs = a.slice(2, a.length - 1)
  } else {
    dataArgs = a.slice(2)
  }

  // Collect numeric data, optionally filtering errors.
  const xs: number[] = []
  for (const arg of dataArgs) {
    const v = ev.eval(arg)
    for (const s of flatten(v)) {
      if (isErr(s)) {
        if (ignoreErrors) continue
        return s // propagate first error for non-error-ignoring options
      }
      if (typeof s === 'number') xs.push(s)
      else if (typeof s === 'boolean') xs.push(s ? 1 : 0)
      else if (typeof s === 'string' && s !== '' && !isNaN(Number(s))) xs.push(Number(s))
      // text / blank → ignored (Excel AGGREGATE ignores non-numeric for these fns)
    }
  }

  return aggregateCompute(f, xs, k)
}

// ── Export registry ───────────────────────────────────────────────────────────

export const MISC_FNS: Record<string, Fn> = {
  REGEXTEST,
  REGEXEXTRACT,
  REGEXREPLACE,
  WRAPROWS,
  WRAPCOLS,
  HYPERLINK,
  CELL,
  INFO,
  ENCODEURL,
  FILTERXML,
  AGGREGATE,
}
