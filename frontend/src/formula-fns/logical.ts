// ── Spreadsheet formula functions: Logical & Information ──────────────────────
// Excel/Sheets-compatible Logical and Information functions for the formula
// engine. Functions already defined in the core engine (IF, IFERROR, IFS, AND,
// OR, NOT, TRUE, FALSE / ISBLANK, ISNUMBER, ISTEXT, ISERROR, ISLOGICAL) are NOT
// redefined here.
//
// Error-propagation rule: any FErr encountered in an argument is propagated.

import {
  type Fn,
  type Node,
  type Value,
  type Scalar,
  ERR,
  isErr,
  isMatrix,
  flatten,
  toBool,
} from '../formula-engine'

// ── Small helpers ─────────────────────────────────────────────────────────────

// Evaluate a node to a scalar (collapsing a matrix to its top-left cell).
function scalarOf(ev: { eval(n: Node): Value }, n: Node): Scalar {
  const v = ev.eval(n)
  return isMatrix(v) ? (v[0]?.[0] ?? '') : v
}

// True when a scalar is the specific #N/A error.
const isNA = (s: Scalar): boolean => isErr(s) && s.err === ERR.NA.err

// ── Logical functions ─────────────────────────────────────────────────────────

// Map of FErr.err string → ERROR.TYPE code (Excel).
const ERROR_TYPE_CODES: Record<string, number> = {
  '#NULL!': 1,
  '#DIV/0!': 2,
  '#VALUE!': 3,
  '#REF!': 4,
  '#NAME?': 5,
  '#NUM!': 6,
  '#N/A': 7,
}

// Stub for first-class-function helpers the current parser cannot represent
// (no lambdas / function references in the grammar).
const unsupported: Fn = () => ERR.NA // not supported by the current parser

export const LOGICAL_FNS: Record<string, Fn> = {
  // ── Logical ────────────────────────────────────────────────────────────────

  // XOR: TRUE when an odd number of arguments are TRUE. Propagates errors.
  XOR: (ev, args) => {
    let trueCount = 0
    let seen = false
    for (const arg of args) {
      for (const s of flatten(ev.eval(arg))) {
        if (isErr(s)) return s
        seen = true
        if (toBool(s)) trueCount++
      }
    }
    if (!seen) return ERR.VALUE
    return trueCount % 2 === 1
  },

  // IFNA(value, value_if_na): like IFERROR but only for #N/A.
  IFNA: (ev, args) => {
    if (args.length < 2) return ERR.VALUE
    const v = ev.eval(args[0])
    const s = isMatrix(v) ? (v[0]?.[0] ?? '') : v
    return isNA(s) ? ev.eval(args[1]) : v
  },

  // SWITCH(expr, val1, res1, [val2, res2, ...], [default]).
  SWITCH: (ev, args) => {
    if (args.length < 2) return ERR.VALUE
    const expr = scalarOf(ev, args[0])
    if (isErr(expr)) return expr
    // Iterate value/result pairs starting at index 1.
    let i = 1
    for (; i + 1 < args.length; i += 2) {
      const candidate = scalarOf(ev, args[i])
      if (isErr(candidate)) return candidate
      if (scalarsEqual(expr, candidate)) return ev.eval(args[i + 1])
    }
    // A trailing odd argument is the default value.
    if (i < args.length) return ev.eval(args[i])
    return ERR.NA
  },

  // LAMBDA/LET/MAP/REDUCE/SCAN/BYROW/BYCOL/MAKEARRAY are implemented natively in
  // formula-engine.ts (they need first-class lambdas + scope in the evaluator).
  FILTER: unsupported, // overridden by the real FILTER in lookup.ts

  // ── Information ──────────────────────────────────────────────────────────────

  // ISERR: TRUE for any error except #N/A.
  ISERR: (ev, args) => {
    const s = scalarOf(ev, args[0])
    return isErr(s) && !isNA(s)
  },

  // ISNA: TRUE only for #N/A.
  ISNA: (ev, args) => isNA(scalarOf(ev, args[0])),

  // ISNONTEXT: TRUE when the value is not text (errors/blanks count as non-text).
  ISNONTEXT: (ev, args) => typeof scalarOf(ev, args[0]) !== 'string',

  // ISEVEN(number): TRUE when the truncated integer part is even.
  ISEVEN: (ev, args) => {
    const n = numberOf(ev, args[0])
    if (isErr(n)) return n
    return Math.trunc(n) % 2 === 0
  },

  // ISODD(number): TRUE when the truncated integer part is odd.
  ISODD: (ev, args) => {
    const n = numberOf(ev, args[0])
    if (isErr(n)) return n
    return Math.abs(Math.trunc(n) % 2) === 1
  },

  // ISREF: best-effort. The engine has no first-class reference value type, so
  // we report TRUE only when the argument is a literal cell/range node.
  ISREF: (_ev, args) => {
    const node = args[0]
    return !!node && (node.k === 'ref' || node.k === 'range')
  },

  // ISFORMULA: best-effort. The referenced cell's formula is not exposed here, so
  // return FALSE when it cannot be determined.
  ISFORMULA: () => false, // best-effort: cannot inspect the referenced cell's formula here

  // N(v): number → itself, TRUE → 1, FALSE → 0, date serial is already a number,
  // text → 0, error → the error.
  N: (ev, args) => {
    const s = scalarOf(ev, args[0])
    if (isErr(s)) return s
    if (typeof s === 'number') return s
    if (typeof s === 'boolean') return s ? 1 : 0
    return 0 // text (and blanks) → 0
  },

  // NA(): always the #N/A error.
  NA: () => ERR.NA,

  // TYPE(v): 1 number, 2 text, 4 logical, 16 error, 64 array.
  TYPE: (ev, args) => {
    const v = ev.eval(args[0])
    if (isMatrix(v)) return 64
    if (isErr(v)) return 16
    if (typeof v === 'number') return 1
    if (typeof v === 'boolean') return 4
    return 2 // text (including blank string)
  },

  // ERROR.TYPE(err): numeric code 1..7 for the error, else #N/A.
  'ERROR.TYPE': (ev, args) => {
    const s = scalarOf(ev, args[0])
    if (!isErr(s)) return ERR.NA
    return ERROR_TYPE_CODES[s.err] ?? ERR.NA
  },

  // SHEET / SHEETS: the engine is single-sheet, so always 1.
  SHEET: () => 1, // single-sheet engine
  SHEETS: () => 1, // single-sheet engine

  // ISOMITTED: only meaningful for LAMBDA optional args, unsupported here.
  ISOMITTED: () => false,
}

// ── Local numeric coercion (avoids importing the engine's private helpers) ────

function numberOf(ev: { eval(n: Node): Value }, node: Node): number | typeof ERR.VALUE {
  const s = scalarOf(ev, node)
  if (isErr(s)) return s as unknown as typeof ERR.VALUE
  if (typeof s === 'number') return s
  if (typeof s === 'boolean') return s ? 1 : 0
  if (s === '' || s == null) return 0
  const n = Number(s)
  return isNaN(n) ? ERR.VALUE : n
}

// Excel-style equality used by SWITCH: numbers compared numerically, text
// case-insensitively, booleans by truthiness.
function scalarsEqual(a: Scalar, b: Scalar): boolean {
  if (typeof a === 'number' && typeof b === 'number') return a === b
  if (typeof a === 'boolean' || typeof b === 'boolean') return toBool(a) === toBool(b)
  return String(a).toLowerCase() === String(b).toLowerCase()
}
