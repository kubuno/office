// ── Lookup & Reference formula functions ──────────────────────────────────────
// Implements Excel's "Lookup & Reference" family on top of the spreadsheet
// formula engine: CHOOSE, COLUMNS/ROWS, TRANSPOSE, LOOKUP, XLOOKUP/XMATCH,
// dynamic-array helpers (FILTER, SORT, SORTBY, UNIQUE, HSTACK, VSTACK, TAKE,
// DROP, CHOOSECOLS, CHOOSEROWS, TOCOL, TOROW, EXPAND) and reference functions
// (ROW, COLUMN, ADDRESS, INDIRECT, OFFSET, AREAS, FORMULATEXT).
//
// Matrix-returning functions return a `Matrix` (Scalar[][]). The grid does not
// spill these, but the engine handles them correctly (scalar context takes the
// top-left cell).

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
  compareScalar,
  colToIndex,
  indexToCol,
} from '../formula-engine'

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Coerce a Value into a 2-D matrix (a scalar becomes a 1x1 matrix). */
function asMatrix(v: Value): Matrix {
  return isMatrix(v) ? v : [[v]]
}

/** First error found while scanning a value (or null). */
function firstErr(v: Value): Scalar | null {
  for (const s of flatten(v)) if (isErr(s)) return s
  return null
}

/** Parse "A1" into a 0-based { row, col }, or null when malformed. */
function parseRef(ref: string): { row: number; col: number } | null {
  const m = ref.match(/^([A-Z]+)(\d+)$/i)
  if (!m) return null
  return { col: colToIndex(m[1].toUpperCase()), row: parseInt(m[2], 10) - 1 }
}

/**
 * Inspect an argument node to recover the referenced range's 1-based bounds.
 * Only `range` and `ref` nodes carry positional information; anything else
 * returns null (the caller falls back to a documented default).
 */
function rangeBounds(node: Node | undefined): { r1: number; c1: number; r2: number; c2: number } | null {
  if (!node) return null
  if (node.k === 'ref') {
    const p = parseRef(node.v)
    if (!p) return null
    return { r1: p.row + 1, c1: p.col + 1, r2: p.row + 1, c2: p.col + 1 }
  }
  if (node.k === 'range') {
    const a = parseRef(node.a), b = parseRef(node.b)
    if (!a || !b) return null
    return {
      r1: Math.min(a.row, b.row) + 1,
      c1: Math.min(a.col, b.col) + 1,
      r2: Math.max(a.row, b.row) + 1,
      c2: Math.max(a.col, b.col) + 1,
    }
  }
  return null
}

/** True when two scalars are equal (case-insensitive for text). */
function scalarEq(a: Scalar, b: Scalar): boolean {
  return compareScalar(a, b) === 0
}

// ── CHOOSE / COLUMNS / ROWS / TRANSPOSE ────────────────────────────────────────

const CHOOSE: Fn = (ev, a) => {
  const idx = num(ev, a[0])
  if (isErr(idx)) return idx
  const i = Math.trunc(idx)
  if (i < 1 || i >= a.length) return ERR.VALUE
  return ev.eval(a[i])
}

const COLUMNS: Fn = (ev, a) => {
  const v = ev.eval(a[0])
  return asMatrix(v)[0]?.length ?? 0
}

const ROWS: Fn = (ev, a) => {
  const v = ev.eval(a[0])
  return asMatrix(v).length
}

const TRANSPOSE: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const rows = m.length, cols = m[0]?.length ?? 0
  const out: Matrix = []
  for (let c = 0; c < cols; c++) {
    const row: Scalar[] = []
    for (let r = 0; r < rows; r++) row.push(m[r][c])
    out.push(row)
  }
  return out
}

// ── LOOKUP (vector form) ───────────────────────────────────────────────────────

/**
 * LOOKUP(value, lookup_vector, [result_vector]). Approximate match against a
 * vector assumed ascending: returns the result paired with the largest value
 * <= target. The array form (single 2-arg matrix) is not handled — callers
 * almost always use the vector form, and VLOOKUP/HLOOKUP cover the rest.
 */
const LOOKUP: Fn = (ev, a) => {
  const target = ev.scalar(a[0])
  if (isErr(target)) return target
  const lookupVec = flatten(ev.eval(a[1]))
  const resultVec = a[2] ? flatten(ev.eval(a[2])) : lookupVec
  let best = -1
  for (let i = 0; i < lookupVec.length; i++) {
    if (compareScalar(lookupVec[i], target) <= 0) best = i
  }
  if (best < 0) return ERR.NA
  return resultVec[best] ?? ERR.NA
}

// ── XLOOKUP / XMATCH ───────────────────────────────────────────────────────────

/**
 * Find the index (0-based) of `target` in `vec` per Excel's match/search modes.
 * matchMode: 0 exact (default), -1 exact-or-next-smaller, 1 exact-or-next-larger,
 *            2 wildcard.
 * searchMode: 1 first→last (default), -1 last→first. Binary modes (2/-2) fall
 *            back to a linear scan (results are identical for sorted input).
 */
function xfind(vec: Scalar[], target: Scalar, matchMode: number, searchMode: number): number {
  const order = searchMode === -1 ? [...vec.keys()].reverse() : [...vec.keys()]
  // Exact / wildcard.
  if (matchMode === 0 || matchMode === 2) {
    const wildcard = matchMode === 2
    const re = wildcard
      ? new RegExp('^' + toStr(target).replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.') + '$', 'i')
      : null
    for (const i of order) {
      if (re ? re.test(toStr(vec[i])) : scalarEq(vec[i], target)) return i
    }
    return -1
  }
  // Approximate: pick the closest value on the requested side.
  let best = -1
  let bestDiff = Infinity
  for (let i = 0; i < vec.length; i++) {
    const cmp = compareScalar(vec[i], target)
    if (cmp === 0) return i
    if (matchMode === -1 && cmp < 0) {
      const diff = -cmp
      if (diff < bestDiff) { bestDiff = diff; best = i }
    }
    if (matchMode === 1 && cmp > 0) {
      const diff = cmp
      if (diff < bestDiff) { bestDiff = diff; best = i }
    }
  }
  return best
}

const XLOOKUP: Fn = (ev, a) => {
  if (a.length < 3) return ERR.VALUE
  const target = ev.scalar(a[0])
  if (isErr(target)) return target
  const lookupArr = flatten(ev.eval(a[1]))
  const returnVal = ev.eval(a[2])
  const returnMat = asMatrix(returnVal)
  // Determine orientation of return array relative to the lookup vector.
  const vertical = returnMat.length === lookupArr.length
  const matchMode = a[4] ? num(ev, a[4]) : 0
  if (isErr(matchMode)) return matchMode
  const searchMode = a[5] ? num(ev, a[5]) : 1
  if (isErr(searchMode)) return searchMode
  const idx = xfind(lookupArr, target, matchMode, searchMode)
  if (idx < 0) {
    if (a[3]) return ev.eval(a[3])
    return ERR.NA
  }
  if (vertical) { const row = returnMat[idx]; if (!row) return ERR.NA; return row.length === 1 ? row[0] : [row] }
  // Horizontal return array → return the matching column.
  const out: Matrix = []
  for (const row of returnMat) out.push([row[idx]])
  return out.length === 1 ? out[0][0] : out
}

const XMATCH: Fn = (ev, a) => {
  const target = ev.scalar(a[0])
  if (isErr(target)) return target
  const vec = flatten(ev.eval(a[1]))
  const matchMode = a[2] ? num(ev, a[2]) : 0
  if (isErr(matchMode)) return matchMode
  const searchMode = a[3] ? num(ev, a[3]) : 1
  if (isErr(searchMode)) return searchMode
  const idx = xfind(vec, target, matchMode, searchMode)
  return idx < 0 ? ERR.NA : idx + 1
}

// ── FILTER / SORT / SORTBY / UNIQUE ────────────────────────────────────────────

const FILTER: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const e = firstErr(m); if (e) return e
  const include = asMatrix(ev.eval(a[1]))
  // `include` is a column or row vector; flatten and match by row count.
  const flags = include.length === m.length ? include.map(r => r[0]) : (include[0] ?? [])
  const out: Matrix = []
  for (let i = 0; i < m.length; i++) {
    const f = flags[i]
    if (f != null && !isErr(f) && toBool(f)) out.push(m[i])
  }
  if (out.length === 0) return a[2] ? ev.eval(a[2]) : ERR.NA
  return out
}

const SORT: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0])).map(r => [...r])
  const sortIndex = a[1] ? num(ev, a[1]) : 1
  if (isErr(sortIndex)) return sortIndex
  const order = a[2] ? num(ev, a[2]) : 1
  if (isErr(order)) return order
  const col = Math.trunc(sortIndex) - 1
  const dir = order < 0 ? -1 : 1
  m.sort((x, y) => compareScalar(x[col] ?? '', y[col] ?? '') * dir)
  return m
}

const SORTBY: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0])).map((r, i) => ({ row: [...r], i }))
  // Build the list of (by-vector, order) key pairs.
  const keys: { vec: Scalar[]; dir: number }[] = []
  for (let i = 1; i + 1 <= a.length; i += 2) {
    const vec = flatten(ev.eval(a[i]))
    const dir = a[i + 1] ? num(ev, a[i + 1]) : 1
    if (isErr(dir)) return dir
    keys.push({ vec, dir: (dir as number) < 0 ? -1 : 1 })
  }
  m.sort((x, y) => {
    for (const k of keys) {
      const c = compareScalar(k.vec[x.i] ?? '', k.vec[y.i] ?? '') * k.dir
      if (c !== 0) return c
    }
    return 0
  })
  return m.map(e => e.row)
}

const UNIQUE: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const byCol = a[1] ? toBool(ev.scalar(a[1])) : false
  const exactlyOnce = a[2] ? toBool(ev.scalar(a[2])) : false
  const keyOf = (arr: Scalar[]) => arr.map(s => toStr(s).toLowerCase()).join(' ')
  if (byCol) {
    // Transpose, unique rows, transpose back.
    const cols = m[0]?.length ?? 0
    const colArr: Scalar[][] = []
    for (let c = 0; c < cols; c++) colArr.push(m.map(r => r[c]))
    const kept = dedupe(colArr, keyOf, exactlyOnce)
    const rows = m.length
    const out: Matrix = []
    for (let r = 0; r < rows; r++) out.push(kept.map(col => col[r]))
    return out.length ? out : ERR.NA
  }
  const out = dedupe(m, keyOf, exactlyOnce)
  return out.length ? out : ERR.NA
}

function dedupe(rows: Scalar[][], keyOf: (r: Scalar[]) => string, exactlyOnce: boolean): Scalar[][] {
  const counts = new Map<string, number>()
  for (const r of rows) counts.set(keyOf(r), (counts.get(keyOf(r)) ?? 0) + 1)
  const seen = new Set<string>()
  const out: Scalar[][] = []
  for (const r of rows) {
    const k = keyOf(r)
    if (seen.has(k)) continue
    seen.add(k)
    if (exactlyOnce && counts.get(k) !== 1) continue
    out.push(r)
  }
  return out
}

// ── HSTACK / VSTACK ────────────────────────────────────────────────────────────

const VSTACK: Fn = (ev, a) => {
  const mats = a.map(n => asMatrix(ev.eval(n)))
  const width = Math.max(0, ...mats.map(m => m[0]?.length ?? 0))
  const out: Matrix = []
  for (const m of mats) for (const row of m) {
    const r = [...row]
    while (r.length < width) r.push(ERR.NA)
    out.push(r)
  }
  return out
}

const HSTACK: Fn = (ev, a) => {
  const mats = a.map(n => asMatrix(ev.eval(n)))
  const height = Math.max(0, ...mats.map(m => m.length))
  const out: Matrix = []
  for (let r = 0; r < height; r++) {
    const row: Scalar[] = []
    for (const m of mats) {
      const w = m[0]?.length ?? 0
      const src = m[r]
      for (let c = 0; c < w; c++) row.push(src ? src[c] : ERR.NA)
    }
    out.push(row)
  }
  return out
}

// ── TAKE / DROP / CHOOSECOLS / CHOOSEROWS ──────────────────────────────────────

const TAKE: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const rows = a[1] != null && a[1] !== undefined ? num(ev, a[1]) : null
  const cols = a[2] != null && a[2] !== undefined ? num(ev, a[2]) : null
  if (isErr(rows)) return rows
  if (isErr(cols)) return cols
  let out = sliceRows(m, rows as number | null, true)
  out = sliceCols(out, cols as number | null, true)
  return out.length ? out : ERR.VALUE
}

const DROP: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const rows = a[1] != null ? num(ev, a[1]) : null
  const cols = a[2] != null ? num(ev, a[2]) : null
  if (isErr(rows)) return rows
  if (isErr(cols)) return cols
  let out = sliceRows(m, rows as number | null, false)
  out = sliceCols(out, cols as number | null, false)
  return out.length ? out : ERR.VALUE
}

/** Keep (take) or remove (drop) `n` rows from the start (n>0) or end (n<0). */
function sliceRows(m: Matrix, n: number | null, take: boolean): Matrix {
  if (n == null) return m
  const k = Math.trunc(n)
  if (take) return k >= 0 ? m.slice(0, k) : m.slice(m.length + k)
  return k >= 0 ? m.slice(k) : m.slice(0, m.length + k)
}

function sliceCols(m: Matrix, n: number | null, take: boolean): Matrix {
  if (n == null) return m
  const k = Math.trunc(n)
  return m.map(row => {
    if (take) return k >= 0 ? row.slice(0, k) : row.slice(row.length + k)
    return k >= 0 ? row.slice(k) : row.slice(0, row.length + k)
  })
}

const CHOOSECOLS: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const width = m[0]?.length ?? 0
  const picks: number[] = []
  for (let i = 1; i < a.length; i++) {
    for (const s of flatten(ev.eval(a[i]))) {
      const n = toNum(s); if (isErr(n)) return n
      const idx = n < 0 ? width + Math.trunc(n) : Math.trunc(n) - 1
      if (idx < 0 || idx >= width) return ERR.VALUE
      picks.push(idx)
    }
  }
  return m.map(row => picks.map(c => row[c]))
}

const CHOOSEROWS: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const height = m.length
  const out: Matrix = []
  for (let i = 1; i < a.length; i++) {
    for (const s of flatten(ev.eval(a[i]))) {
      const n = toNum(s); if (isErr(n)) return n
      const idx = n < 0 ? height + Math.trunc(n) : Math.trunc(n) - 1
      if (idx < 0 || idx >= height) return ERR.VALUE
      out.push([...m[idx]])
    }
  }
  return out
}

// ── TOCOL / TOROW ──────────────────────────────────────────────────────────────

/**
 * Flatten a matrix to a vector. scanByColumn (3rd arg, default false=by row)
 * controls traversal order; ignoreEmpty (2nd arg: 0 keep, 1 ignore blanks,
 * 2 ignore errors, 3 ignore both) trims unwanted scalars.
 */
function flattenOrdered(ev: Evaluator, a: Node[]): Scalar[] {
  const m = asMatrix(ev.eval(a[0]))
  const ignore = a[1] ? Math.trunc(Number(toNum(ev.scalar(a[1])) as number)) : 0
  const byCol = a[2] ? toBool(ev.scalar(a[2])) : false
  const seq: Scalar[] = []
  const rows = m.length, cols = m[0]?.length ?? 0
  if (byCol) {
    for (let c = 0; c < cols; c++) for (let r = 0; r < rows; r++) seq.push(m[r][c])
  } else {
    for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) seq.push(m[r][c])
  }
  const skipBlank = ignore === 1 || ignore === 3
  const skipErr = ignore === 2 || ignore === 3
  return seq.filter(s => {
    if (skipBlank && (s === '' || s == null)) return false
    if (skipErr && isErr(s)) return false
    return true
  })
}

const TOCOL: Fn = (ev, a) => flattenOrdered(ev, a).map(s => [s])

const TOROW: Fn = (ev, a) => [flattenOrdered(ev, a)]

// ── EXPAND ─────────────────────────────────────────────────────────────────────

const EXPAND: Fn = (ev, a) => {
  const m = asMatrix(ev.eval(a[0]))
  const curRows = m.length, curCols = m[0]?.length ?? 0
  const rows = a[1] != null ? num(ev, a[1]) : null
  const cols = a[2] != null ? num(ev, a[2]) : null
  if (isErr(rows)) return rows
  if (isErr(cols)) return cols
  const pad: Scalar = a[3] ? ev.scalar(a[3]) : ERR.NA
  const targetRows = rows == null ? curRows : Math.trunc(rows as number)
  const targetCols = cols == null ? curCols : Math.trunc(cols as number)
  if (targetRows < curRows || targetCols < curCols) return ERR.VALUE
  const out: Matrix = []
  for (let r = 0; r < targetRows; r++) {
    const row: Scalar[] = []
    for (let c = 0; c < targetCols; c++) {
      row.push(r < curRows && c < curCols ? m[r][c] : pad)
    }
    out.push(row)
  }
  return out
}

// ── ROW / COLUMN ───────────────────────────────────────────────────────────────

/**
 * ROW([ref]) — without positional context for the calling cell, ROW() with no
 * argument cannot know its own row, so it returns 1 (documented limitation).
 * With a range/ref argument we read the node directly and return the top row.
 */
const ROW: Fn = (_ev, a) => {
  if (!a[0]) return 1 // no caller position available → default to 1
  const b = rangeBounds(a[0])
  return b ? b.r1 : 1
}

const COLUMN: Fn = (_ev, a) => {
  if (!a[0]) return 1 // no caller position available → default to 1
  const b = rangeBounds(a[0])
  return b ? b.c1 : 1
}

// ── ADDRESS ────────────────────────────────────────────────────────────────────

/**
 * ADDRESS(row, col, [abs_num], [a1], [sheet]) → e.g. "$A$1".
 * abs_num: 1 $A$1 (default), 2 A$1, 3 $A1, 4 A1. R1C1 style (a1=FALSE) is
 * approximated as "R{row}C{col}".
 */
const ADDRESS: Fn = (ev, a) => {
  const row = num(ev, a[0]); if (isErr(row)) return row
  const col = num(ev, a[1]); if (isErr(col)) return col
  const absNum = a[2] ? num(ev, a[2]) : 1; if (isErr(absNum)) return absNum
  const a1 = a[3] ? toBool(ev.scalar(a[3])) : true
  const sheet = a[4] ? str(ev, a[4]) : ''
  const r = Math.trunc(row as number), c = Math.trunc(col as number)
  if (r < 1 || c < 1) return ERR.VALUE
  let ref: string
  if (a1) {
    const colAbs = absNum === 1 || absNum === 3 ? '$' : ''
    const rowAbs = absNum === 1 || absNum === 2 ? '$' : ''
    ref = `${colAbs}${indexToCol(c - 1)}${rowAbs}${r}`
  } else {
    ref = `R${r}C${c}` // R1C1-style approximation
  }
  return sheet ? `${sheet}!${ref}` : ref
}

// ── INDIRECT ───────────────────────────────────────────────────────────────────

/**
 * INDIRECT(ref_text, [a1]) — evaluate the range named by a string. We build a
 * `range`/`ref` Node and feed it back through the evaluator. Only A1-style
 * single cells and "A1:B2" ranges are supported (R1C1 and named ranges are
 * out of scope for this engine).
 */
const INDIRECT: Fn = (ev, a) => {
  const text = str(ev, a[0]).trim().toUpperCase()
  if (!text) return ERR.REF
  const m = text.match(/^([A-Z]+\d+)(?::([A-Z]+\d+))?$/)
  if (!m) return ERR.REF
  const node: Node = m[2]
    ? { k: 'range', a: m[1], b: m[2] }
    : { k: 'ref', v: m[1] }
  return ev.eval(node)
}

// ── OFFSET ─────────────────────────────────────────────────────────────────────

/**
 * OFFSET(reference, rows, cols, [height], [width]) — shift a reference and
 * resize it. We recover the base reference's top-left from the argument node
 * (range/ref); when the node carries no positional info OFFSET cannot proceed
 * and returns #REF!. The resulting range is rebuilt and evaluated.
 */
const OFFSET: Fn = (ev, a) => {
  const base = rangeBounds(a[0])
  if (!base) return ERR.REF // no positional reference available
  const dRow = num(ev, a[1]); if (isErr(dRow)) return dRow
  const dCol = num(ev, a[2]); if (isErr(dCol)) return dCol
  const baseH = base.r2 - base.r1 + 1, baseW = base.c2 - base.c1 + 1
  const h = a[3] ? num(ev, a[3]) : baseH; if (isErr(h)) return h
  const w = a[4] ? num(ev, a[4]) : baseW; if (isErr(w)) return w
  const r1 = base.r1 + Math.trunc(dRow as number)
  const c1 = base.c1 + Math.trunc(dCol as number)
  const r2 = r1 + Math.trunc(h as number) - 1
  const c2 = c1 + Math.trunc(w as number) - 1
  if (r1 < 1 || c1 < 1 || r2 < r1 || c2 < c1) return ERR.REF
  const node: Node = {
    k: 'range',
    a: `${indexToCol(c1 - 1)}${r1}`,
    b: `${indexToCol(c2 - 1)}${r2}`,
  }
  return ev.eval(node)
}

// ── AREAS / FORMULATEXT ────────────────────────────────────────────────────────

/**
 * AREAS(reference) → number of areas. The engine has no union/multi-area
 * reference syntax, so any single reference is always 1 area.
 */
const AREAS: Fn = () => 1

/**
 * FORMULATEXT(reference) → the formula text of the referenced cell. We read the
 * cell's raw formula via the evaluator (best-effort): if the cell has a stored
 * "=..." formula we return it, otherwise #N/A as Excel does for non-formula or
 * unavailable cells.
 */
const FORMULATEXT: Fn = (ev, a) => {
  const b = rangeBounds(a[0])
  if (!b) return ERR.NA
  // Read the raw cell at the top-left of the reference.
  const ref = `${indexToCol(b.c1 - 1)}${b.r1}`
  const raw = (ev as unknown as { cellRaw(ref: string): { f?: string } | undefined }).cellRaw?.(ref)
  const f = raw?.f
  return f && f.startsWith('=') ? f : ERR.NA
}

// ── Registry ───────────────────────────────────────────────────────────────────

export const LOOKUP_FNS: Record<string, Fn> = {
  CHOOSE,
  COLUMNS,
  ROWS,
  TRANSPOSE,
  LOOKUP,
  XLOOKUP,
  XMATCH,
  FILTER,
  SORT,
  SORTBY,
  UNIQUE,
  HSTACK,
  VSTACK,
  TAKE,
  DROP,
  CHOOSECOLS,
  CHOOSEROWS,
  TOCOL,
  TOROW,
  EXPAND,
  ROW,
  COLUMN,
  ADDRESS,
  INDIRECT,
  OFFSET,
  AREAS,
  FORMULATEXT,
}
