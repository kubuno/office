// Numeric evaluation of a (closed-form) LaTeX formula — the "live calculator" of the Maths
// module. The LaTeX is parsed with the WYSIWYG tree parser (mathTree), converted into a plain
// textual expression and evaluated by the zero-dependency compiler (mathExpr). Anything that
// is not a constant numeric expression (free variables, matrices, big operators, …) simply
// yields `null`: the UI then shows nothing.
import { parse, type MRow, type MNode } from './mathTree'
import { compile } from './mathExpr'

const FN: Record<string, string> = {
  '\\sin': 'sin', '\\cos': 'cos', '\\tan': 'tan',
  '\\sec': 'sec', '\\csc': 'csc', '\\cot': 'cot',
  '\\arcsin': 'asin', '\\arccos': 'acos', '\\arctan': 'atan',
  '\\sinh': 'sinh', '\\cosh': 'cosh', '\\tanh': 'tanh',
  '\\ln': 'ln', '\\log': 'log', '\\lg': 'log', '\\exp': 'exp',
}
const CONST: Record<string, string> = {
  '\\pi': 'pi', '\\tau': 'tau', '\\phi': 'phi', '\\varphi': 'phi',
}
const OPS: Record<string, string> = {
  '+': '+', '-': '-', '\\cdot': '*', '\\times': '*', '\\ast': '*', '\\div': '/', '*': '*', '/': '/',
}
// Top-level relation atoms used to split "a = b" style formulas into evaluable segments.
const RELATIONS = new Set(['=', '<', '>', '\\neq', '\\leq', '\\geq', '\\approx', '\\equiv', '\\sim', '\\simeq'])

class NotEvaluable extends Error {}

function convRow(row: MRow): string {
  if (!row.length) throw new NotEvaluable()
  let out = ''
  for (let i = 0; i < row.length; i++) {
    const n = row[i]
    // Function application: \sin followed by a delimited group, a plain "(", or one operand.
    if (n.k === 'atom' && FN[n.t]) {
      const next = row[i + 1]
      if (!next) throw new NotEvaluable()
      if (next.k === 'delim') { out += `${FN[n.t]}(${convRow(next.b)})`; i++; continue }
      if (next.k === 'atom' && next.t === '(') { out += FN[n.t]; continue }   // "(…)" atoms follow
      out += `${FN[n.t]}(${convNode(next)})`; i++; continue
    }
    out += convNode(n)
  }
  return out
}

function convNode(n: MNode): string {
  switch (n.k) {
    case 'atom': return convAtom(n.t)
    case 'frac': return `((${convRow(n.n)})/(${convRow(n.d)}))`
    case 'sqrt': return `sqrt(${convRow(n.b)})`
    case 'root': return `((${convRow(n.b)})^(1/(${convRow(n.i)})))`
    case 'script': {
      if (n.sub || !n.sup) throw new NotEvaluable()
      return `((${convNode(n.base)})^(${convRow(n.sup)}))`
    }
    case 'delim': {
      const inner = convRow(n.b)
      if (n.o === '|' && n.c === '|') return `abs(${inner})`
      return `(${inner})`
    }
    default: throw new NotEvaluable()   // big / acc / mat / raw
  }
}

function convAtom(t: string): string {
  if (OPS[t]) return OPS[t]
  if (CONST[t]) return ` ${CONST[t]} `
  if (/^[0-9.]$/.test(t)) return t
  if (t === ',') return '.'             // decimal comma
  // Lowercase letters only: e → Euler constant, x poisons via NaN, the rest fail to compile.
  // Uppercase letters are always symbolic (E, R, …) — mathExpr lowercases identifiers, so
  // letting them through would silently turn E into Euler's number.
  if (/^[a-z]$/.test(t)) return t
  if (t === '(' || t === ')' || t === '!') return t
  throw new NotEvaluable()
}

// Split the top-level row on relation atoms ("x = 1+2" → ["x", "1+2"]).
function splitOnRelations(row: MRow): MRow[] {
  const out: MRow[] = []
  let cur: MRow = []
  for (const n of row) {
    if (n.k === 'atom' && RELATIONS.has(n.t)) { out.push(cur); cur = [] } else cur.push(n)
  }
  out.push(cur)
  return out
}

// Evaluate a LaTeX source; `null` when it is not a constant numeric expression.
// With relations, segments are tried right to left ("1+2 =" and "x = 1+2" both work).
export function evalLatex(latex: string): number | null {
  const src = (latex ?? '').trim()
  if (!src || src.includes('\\square')) return null
  let row: MRow
  try { row = parse(src) } catch { return null }
  const segments = splitOnRelations(row).reverse()
  for (const seg of segments) {
    if (!seg.length) continue
    try {
      const expr = convRow(seg)
      const c = compile(expr)
      if (c.error) continue
      const v = c.fn(NaN)                // NaN poisons any use of the variable x
      if (Number.isFinite(v)) return v
    } catch { /* try the next segment */ }
  }
  return null
}

// Convert a LaTeX formula to a plain textual expression in `x` (for the symbolic engine
// and the grapher). With a relation ("y = x^2"), the side that actually mentions x is
// kept (else the right-hand side). Returns null if it is not a supported expression.
export function latexToExpr(latex: string): string | null {
  const src = (latex ?? '').trim()
  if (!src || src.includes('\\square')) return null
  let row: MRow
  try { row = parse(src) } catch { return null }
  const segments = splitOnRelations(row)
  // Prefer a segment that mentions the variable x; otherwise the last non-empty one.
  const hasX = (seg: MRow): boolean => seg.some(n => (n.k === 'atom' && n.t === 'x'))
  const ordered = [...segments].reverse()
  const pick = ordered.find(s => s.length && hasX(s)) ?? ordered.find(s => s.length)
  if (!pick) return null
  try {
    const expr = convRow(pick)
    return expr.trim() || null
  } catch { return null }
}

// Compact display of a result (12 significant digits, no float noise).
export function fmtResult(v: number): string {
  if (!Number.isFinite(v)) return ''
  if (Number.isInteger(v) && Math.abs(v) < 1e15) return String(v)
  return Number(v.toPrecision(12)).toString()
}

// LaTeX form of a result (scientific notation becomes m × 10^k).
export function resultLatex(v: number): string {
  const s = fmtResult(v)
  const m = /^(-?[\d.]+)e([+-]?\d+)$/.exec(s)
  if (m) return `${m[1]} \\times 10^{${Number(m[2])}}`
  return s
}
