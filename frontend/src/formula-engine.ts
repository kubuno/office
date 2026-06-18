// ── Moteur de formules du tableur ────────────────────────────────────────────
// Tokeniseur + parseur à descente récursive + évaluateur, avec registre de
// fonctions, références/plages de cellules (évaluation récursive + détection de
// cycles) et valeurs d'erreur façon Excel/Sheets.
//
// Remplace l'ancien `evaluateFormula` qui ne gérait qu'un seul `=SUM(A1:B2)`.

import type { SheetData, CellData } from './api'
// Catégories de fonctions Excel (façon alphabétique Microsoft) — voir formula-fns/.
import { MATH_FNS } from './formula-fns/math'
import { STAT_FNS } from './formula-fns/stat'
import { TEXT_FNS } from './formula-fns/text'
import { DATE_FNS } from './formula-fns/date'
import { LOGICAL_FNS } from './formula-fns/logical'
import { LOOKUP_FNS } from './formula-fns/lookup'
import { FINANCIAL_FNS } from './formula-fns/financial'
import { ENGINEERING_FNS } from './formula-fns/engineering'
import { STAT2_FNS } from './formula-fns/stat2'
import { FINANCIAL2_FNS } from './formula-fns/financial2'
import { ENGINEERING2_FNS } from './formula-fns/engineering2'
import { MISC_FNS } from './formula-fns/misc'

// ── Valeurs ───────────────────────────────────────────────────────────────────
export type FErr = { err: string }
export type Scalar = number | string | boolean | FErr
export type Matrix = Scalar[][]
export type Value = Scalar | Matrix

export const ERR = {
  DIV0: { err: '#DIV/0!' } as FErr,
  VALUE: { err: '#VALUE!' } as FErr,
  REF: { err: '#REF!' } as FErr,
  NAME: { err: '#NAME?' } as FErr,
  NA: { err: '#N/A' } as FErr,
  NUM: { err: '#NUM!' } as FErr,
  CIRC: { err: '#REF!' } as FErr, // circular → #REF!
  SPILL: { err: '#SPILL!' } as FErr, // dynamic array cannot spill (range blocked)
  CALC: { err: '#CALC!' } as FErr, // calculation error (e.g. a bare lambda value)
}
export const isErr = (v: unknown): v is FErr =>
  typeof v === 'object' && v !== null && 'err' in (v as Record<string, unknown>)
export const isMatrix = (v: Value): v is Matrix => Array.isArray(v)

// First-class function value (LAMBDA) with its captured closure scope. It is not
// part of the public Value union — it flows through evaluation and is consumed by
// LET/MAP/REDUCE/… or by direct application; a bare lambda surfaces as #CALC!.
export interface Lambda { kind: 'lambda'; params: string[]; body: Node; scope: Scope }
export type Scope = Record<string, Value | Lambda>
export const isLambda = (v: unknown): v is Lambda =>
  typeof v === 'object' && v !== null && (v as { kind?: string }).kind === 'lambda'

// ── Colonnes ↔ index ───────────────────────────────────────────────────────────
export function colToIndex(col: string): number {
  let n = 0
  for (const ch of col) n = n * 26 + (ch.charCodeAt(0) - 64)
  return n - 1
}
export function indexToCol(i: number): string {
  let s = ''; i += 1
  while (i > 0) { const r = (i - 1) % 26; s = String.fromCharCode(65 + r) + s; i = Math.floor((i - 1) / 26) }
  return s
}

// ── Tokeniseur ──────────────────────────────────────────────────────────────────
type Tok =
  | { t: 'num'; v: number }
  | { t: 'str'; v: string }
  | { t: 'bool'; v: boolean }
  | { t: 'ref'; v: string }          // A1 (sans $)
  | { t: 'sheet'; v: string }        // a sheet qualifier "Sheet!" / "'Sheet Name'!"
  | { t: 'name'; v: string }         // nom de fonction
  | { t: 'op'; v: string }
  | { t: 'lp' } | { t: 'rp' } | { t: 'comma' } | { t: 'semi' } | { t: 'colon' }
  | { t: 'lbrace' } | { t: 'rbrace' } | { t: 'hash' }

function tokenize(src: string): Tok[] | FErr {
  const toks: Tok[] = []
  let i = 0
  const n = src.length
  while (i < n) {
    const c = src[i]
    if (c === ' ' || c === '\t' || c === '\n') { i++; continue }
    if (c === '"') {
      let s = ''; i++
      while (i < n && src[i] !== '"') { if (src[i] === '"' && src[i + 1] === '"') { s += '"'; i += 2 } else { s += src[i++] } }
      if (i >= n) return ERR.VALUE
      i++ // ferme le "
      toks.push({ t: 'str', v: s }); continue
    }
    if (/[0-9]/.test(c) || (c === '.' && /[0-9]/.test(src[i + 1] ?? ''))) {
      let j = i + 1
      while (j < n && /[0-9.]/.test(src[j])) j++
      if (src[j] === 'e' || src[j] === 'E') { j++; if (src[j] === '+' || src[j] === '-') j++; while (j < n && /[0-9]/.test(src[j])) j++ }
      toks.push({ t: 'num', v: parseFloat(src.slice(i, j)) }); i = j; continue
    }
    // Quoted sheet name: 'Sheet Name'! (single quotes, '' escapes a quote).
    if (c === "'") {
      let s = ''; i++
      while (i < n && src[i] !== "'") { if (src[i] === "'" && src[i + 1] === "'") { s += "'"; i += 2 } else { s += src[i++] } }
      if (i >= n) return ERR.VALUE
      i++ // closing '
      if (src[i] === '!') { i++; toks.push({ t: 'sheet', v: s }); continue }
      return ERR.VALUE // a quoted name not followed by '!' is not valid here
    }
    // $A$1 / A1 / nom de fonction / TRUE / FALSE / Sheet! qualifier
    if (/[A-Za-z$_]/.test(c)) {
      let j = i
      while (j < n && /[A-Za-z0-9$_.]/.test(src[j])) j++
      const word = src.slice(i, j); i = j
      // Unquoted sheet qualifier "Sheet1!" — keep original case for the name.
      if (src[i] === '!') { i++; toks.push({ t: 'sheet', v: word.replace(/\$/g, '') }); continue }
      const bare = word.replace(/\$/g, '')
      if (/^[A-Za-z]+[0-9]+$/.test(bare)) { toks.push({ t: 'ref', v: bare.toUpperCase() }) }
      else if (/^true$/i.test(bare)) toks.push({ t: 'bool', v: true })
      else if (/^false$/i.test(bare)) toks.push({ t: 'bool', v: false })
      else toks.push({ t: 'name', v: bare.toUpperCase() })
      continue
    }
    // opérateurs
    if (c === '<' && src[i + 1] === '=') { toks.push({ t: 'op', v: '<=' }); i += 2; continue }
    if (c === '>' && src[i + 1] === '=') { toks.push({ t: 'op', v: '>=' }); i += 2; continue }
    if (c === '<' && src[i + 1] === '>') { toks.push({ t: 'op', v: '<>' }); i += 2; continue }
    if ('+-*/^&=<>%'.includes(c)) { toks.push({ t: 'op', v: c }); i++; continue }
    if (c === '(') { toks.push({ t: 'lp' }); i++; continue }
    if (c === ')') { toks.push({ t: 'rp' }); i++; continue }
    if (c === ',') { toks.push({ t: 'comma' }); i++; continue }
    if (c === ';') { toks.push({ t: 'semi' }); i++; continue }
    if (c === '{') { toks.push({ t: 'lbrace' }); i++; continue }
    if (c === '}') { toks.push({ t: 'rbrace' }); i++; continue }
    if (c === '#') { toks.push({ t: 'hash' }); i++; continue }
    if (c === ':') { toks.push({ t: 'colon' }); i++; continue }
    return ERR.VALUE
  }
  return toks
}

// ── AST ─────────────────────────────────────────────────────────────────────────
export type Node =
  | { k: 'num'; v: number }
  | { k: 'str'; v: string }
  | { k: 'bool'; v: boolean }
  | { k: 'ref'; v: string; sheet?: string }
  | { k: 'range'; a: string; b: string; sheet?: string }
  | { k: 'unary'; op: string; e: Node }
  | { k: 'postfix'; op: string; e: Node }
  | { k: 'bin'; op: string; l: Node; r: Node }
  | { k: 'call'; name: string; args: Node[] }
  | { k: 'array'; rows: Node[][] }       // {1,2;3,4} array constant
  | { k: 'spillref'; ref: string }        // A1# spilled-range operator
  | { k: 'var'; name: string }            // a LET/LAMBDA-bound name
  | { k: 'apply'; fn: Node; args: Node[] } // applying a value: LAMBDA(x,x)(5)

// ── Parseur (descente récursive, précédence) ────────────────────────────────────
class Parser {
  i = 0
  constructor(private toks: Tok[]) {}
  peek() { return this.toks[this.i] }
  next() { return this.toks[this.i++] }
  parse(): Node { return this.parseCompare() }

  parseCompare(): Node {
    let l = this.parseConcat()
    while (this.peek()?.t === 'op' && ['=', '<>', '<', '>', '<=', '>='].includes((this.peek() as { v: string }).v)) {
      const op = (this.next() as { v: string }).v; l = { k: 'bin', op, l, r: this.parseConcat() }
    }
    return l
  }
  parseConcat(): Node {
    let l = this.parseAdd()
    while (this.peek()?.t === 'op' && (this.peek() as { v: string }).v === '&') { this.next(); l = { k: 'bin', op: '&', l, r: this.parseAdd() } }
    return l
  }
  parseAdd(): Node {
    let l = this.parseMul()
    while (this.peek()?.t === 'op' && ['+', '-'].includes((this.peek() as { v: string }).v)) { const op = (this.next() as { v: string }).v; l = { k: 'bin', op, l, r: this.parseMul() } }
    return l
  }
  parseMul(): Node {
    let l = this.parsePow()
    while (this.peek()?.t === 'op' && ['*', '/'].includes((this.peek() as { v: string }).v)) { const op = (this.next() as { v: string }).v; l = { k: 'bin', op, l, r: this.parsePow() } }
    return l
  }
  parsePow(): Node {
    let l = this.parseUnary()
    while (this.peek()?.t === 'op' && (this.peek() as { v: string }).v === '^') { this.next(); l = { k: 'bin', op: '^', l, r: this.parseUnary() } }
    return l
  }
  parseUnary(): Node {
    if (this.peek()?.t === 'op' && ['-', '+'].includes((this.peek() as { v: string }).v)) { const op = (this.next() as { v: string }).v; return { k: 'unary', op, e: this.parseUnary() } }
    return this.parsePostfix()
  }
  parsePostfix(): Node {
    let e = this.parsePrimary()
    for (;;) {
      const p = this.peek()
      if (p?.t === 'op' && p.v === '%') { this.next(); e = { k: 'postfix', op: '%', e } }
      // Applying the result of an expression to arguments — LAMBDA(x,x+1)(5).
      else if (p?.t === 'lp') { this.next(); e = { k: 'apply', fn: e, args: this.parseArgList() } }
      else break
    }
    return e
  }
  // Parse a "(" already consumed argument list up to and including ")".
  parseArgList(): Node[] {
    const args: Node[] = []
    if (this.peek()?.t !== 'rp') {
      args.push(this.parseCompare())
      while (this.peek()?.t === 'comma' || this.peek()?.t === 'semi') { this.next(); args.push(this.parseCompare()) }
    }
    if (this.next()?.t !== 'rp') throw ERR.VALUE
    return args
  }
  parsePrimary(): Node {
    const tk = this.next()
    if (!tk) throw ERR.VALUE
    if (tk.t === 'num') return { k: 'num', v: tk.v }
    if (tk.t === 'str') return { k: 'str', v: tk.v }
    if (tk.t === 'bool') return { k: 'bool', v: tk.v }
    if (tk.t === 'ref') {
      if (this.peek()?.t === 'colon') { this.next(); const b = this.next(); if (b?.t !== 'ref') throw ERR.REF; return { k: 'range', a: tk.v, b: b.v } }
      // A1# — spilled-range operator (the whole array spilled from anchor A1).
      if (this.peek()?.t === 'hash') { this.next(); return { k: 'spillref', ref: tk.v } }
      return { k: 'ref', v: tk.v }
    }
    // Sheet-qualified reference: 'Sheet'!A1 or 'Sheet'!A1:C10.
    if (tk.t === 'sheet') {
      const r = this.next()
      if (r?.t !== 'ref') throw ERR.REF
      if (this.peek()?.t === 'colon') { this.next(); const b = this.next(); if (b?.t !== 'ref') throw ERR.REF; return { k: 'range', a: r.v, b: b.v, sheet: tk.v } }
      return { k: 'ref', v: r.v, sheet: tk.v }
    }
    if (tk.t === 'name') {
      if (this.peek()?.t === 'lp') {
        this.next()
        return { k: 'call', name: tk.v, args: this.parseArgList() }
      }
      // A bare name is a LET/LAMBDA-bound variable (resolved against the scope).
      return { k: 'var', name: tk.v }
    }
    if (tk.t === 'lp') { const e = this.parseCompare(); if (this.next()?.t !== 'rp') throw ERR.VALUE; return e }
    // Array constant {1,2;3,4}: ',' separates columns, ';' separates rows.
    if (tk.t === 'lbrace') {
      const rows: Node[][] = []
      let row: Node[] = []
      if (this.peek()?.t !== 'rbrace') {
        row.push(this.parseCompare())
        for (;;) {
          const p = this.peek()
          if (p?.t === 'comma') { this.next(); row.push(this.parseCompare()) }
          else if (p?.t === 'semi') { this.next(); rows.push(row); row = []; row.push(this.parseCompare()) }
          else break
        }
      }
      rows.push(row)
      if (this.next()?.t !== 'rbrace') throw ERR.VALUE
      return { k: 'array', rows }
    }
    throw ERR.VALUE
  }
}

// ── Coercions ───────────────────────────────────────────────────────────────────
export function toNum(v: Scalar): number | FErr {
  if (typeof v === 'number') return v
  if (typeof v === 'boolean') return v ? 1 : 0
  if (isErr(v)) return v
  if (v === '' || v == null) return 0
  const n = Number(v)
  return isNaN(n) ? ERR.VALUE : n
}
export function toStr(v: Scalar): string {
  if (isErr(v)) return v.err
  if (typeof v === 'boolean') return v ? 'TRUE' : 'FALSE'
  if (v == null) return ''
  return String(v)
}
export function toBool(v: Scalar): boolean {
  if (typeof v === 'boolean') return v
  if (typeof v === 'number') return v !== 0
  if (typeof v === 'string') return /^true$/i.test(v) ? true : v !== ''
  return false
}
export function flatten(v: Value): Scalar[] {
  if (isMatrix(v)) return v.flat()
  return [v]
}

// Apply a scalar function over a value, preserving matrix shape (for unary/% ops).
function mapValue(v: Value, f: (s: Scalar) => Scalar): Value {
  if (!isMatrix(v)) return f(v)
  return v.map(row => row.map(f))
}

// Element-wise combine of two values with Excel array-broadcasting: a 1×N or N×1
// operand is reused across the missing dimension; out-of-range cells become #N/A.
function broadcast(L: Value, R: Value, f: (a: Scalar, b: Scalar) => Scalar): Value {
  const lm = isMatrix(L) ? L : [[L]]
  const rm = isMatrix(R) ? R : [[R]]
  const rows = Math.max(lm.length, rm.length)
  const cols = Math.max(lm[0]?.length ?? 0, rm[0]?.length ?? 0)
  const pick = (m: Matrix, i: number, j: number): Scalar | undefined => {
    const row = m[m.length === 1 ? 0 : i]
    if (!row) return undefined
    const c = row.length === 1 ? 0 : j
    return c < row.length ? row[c] : undefined
  }
  const out: Matrix = []
  for (let i = 0; i < rows; i++) {
    const row: Scalar[] = []
    for (let j = 0; j < cols; j++) {
      const a = pick(lm, i, j), b = pick(rm, i, j)
      row.push(a === undefined || b === undefined ? ERR.NA : f(a, b))
    }
    out.push(row)
  }
  return out
}

// Coerce an eval result into a Matrix for the array-iterating lambda helpers.
function asMat(v: Value): Matrix { return isMatrix(v) ? v : [[v]] }
// Reduce a lambda result to a single Scalar (top-left if it returned an array).
function lamScalar(v: Value | Lambda): Scalar {
  if (isLambda(v)) return ERR.CALC
  return isMatrix(v) ? (v[0]?.[0] ?? ERR.NA) : v
}

// ── Évaluateur ──────────────────────────────────────────────────────────────────
export class Evaluator {
  constructor(
    private data: SheetData,
    private visiting: Set<string>,
    private spill?: SpillIndex,
    private scope: Scope = {},
    // Workbook-level defined names (name → formula text). Shared across children.
    private names: Record<string, string> = data.names ?? {},
    private nameCache: Map<string, Value | Lambda> = new Map(),
    private resolving: Set<string> = new Set(),
  ) {}

  // A child evaluator sharing data/visiting/spill/names but with extra names in scope.
  withScope(extra: Scope): Evaluator {
    return new Evaluator(this.data, this.visiting, this.spill, { ...this.scope, ...extra }, this.names, this.nameCache, this.resolving)
  }

  // Build a LAMBDA value capturing the current scope as its closure.
  makeLambda(params: string[], body: Node): Lambda { return { kind: 'lambda', params, body, scope: this.scope } }

  // Resolve a workbook-defined name (named range / value / lambda), memoised. A
  // named definition is evaluated in a CLEAN local scope (workbook names are
  // global; LET locals must not leak) but still sees the other names + itself,
  // so a named LAMBDA can recurse (Fact = LAMBDA(n, IF(n<2,1,n*Fact(n-1)))).
  resolveName(name: string): Value | Lambda | undefined {
    if (!(name in this.names)) return undefined
    const cached = this.nameCache.get(name)
    if (cached !== undefined) return cached
    if (this.resolving.has(name)) return ERR.CIRC
    this.resolving.add(name)
    const ast = parseFormula(this.names[name])
    let result: Value | Lambda
    if (isErr(ast)) result = ast
    else {
      const e = new Evaluator(this.data, this.visiting, this.spill, {}, this.names, this.nameCache, this.resolving)
      try { result = e.eval(ast) as Value | Lambda } catch (err) { result = isErr(err) ? err : ERR.VALUE }
    }
    this.resolving.delete(name)
    this.nameCache.set(name, result)
    return result
  }

  // Apply a lambda to already-evaluated argument values.
  applyLambda(fn: Lambda, args: (Value | Lambda)[]): Value | Lambda {
    if (args.length !== fn.params.length) return ERR.VALUE
    const extra: Scope = {}
    fn.params.forEach((p, i) => { extra[p] = args[i] })
    const child = new Evaluator(this.data, this.visiting, this.spill, { ...fn.scope, ...extra }, this.names, this.nameCache, this.resolving)
    return child.eval(fn.body) as Value | Lambda
  }

  cellRaw(ref: string): CellData | undefined { return this.data.cells[ref] }

  // Cells of a sheet by name (undefined = the active/own sheet). null = unknown sheet.
  private cellsOf(sheet?: string): Record<string, CellData> | null {
    if (sheet === undefined) return this.data.cells
    return this.data.sheets?.[sheet] ?? null
  }

  cellValue(ref: string, sheet?: string): Scalar {
    const cells = this.cellsOf(sheet)
    if (cells === null) return ERR.REF
    const vkey = sheet === undefined ? ref : `${sheet}!${ref}`
    if (this.visiting.has(vkey)) return ERR.CIRC
    const cell = cells[ref]
    if (!cell) {
      // Empty cell may carry a value spilled from a dynamic-array anchor (own sheet only).
      const sv = sheet === undefined ? this.spill?.values[ref] : undefined
      return sv === undefined ? '' : sv
    }
    if (cell.f && cell.f.startsWith('=')) {
      this.visiting.add(vkey)
      // Evaluate against the referenced sheet's cells (cross-sheet formulas).
      const data = sheet === undefined ? this.data : { cells, sheets: this.data.sheets, names: this.data.names }
      const r = evaluate(cell.f, data, this.visiting)
      this.visiting.delete(vkey)
      return isMatrix(r) ? (r[0]?.[0] ?? '') : r
    }
    const v = cell.v
    if (v == null) return ''
    if (typeof v === 'string' && v !== '' && !isNaN(Number(v))) return Number(v)
    return v as Scalar
  }

  range(a: string, b: string, sheet?: string): Matrix {
    if (this.cellsOf(sheet) === null) return [[ERR.REF]]
    const ma = a.match(/^([A-Z]+)(\d+)$/), mb = b.match(/^([A-Z]+)(\d+)$/)
    if (!ma || !mb) return [[ERR.REF]]
    const c1 = colToIndex(ma[1]), c2 = colToIndex(mb[1])
    const r1 = parseInt(ma[2]), r2 = parseInt(mb[2])
    const lo = Math.min(c1, c2), hi = Math.max(c1, c2)
    const tr = Math.min(r1, r2), br = Math.max(r1, r2)
    const out: Matrix = []
    for (let r = tr; r <= br; r++) {
      const row: Scalar[] = []
      for (let c = lo; c <= hi; c++) row.push(this.cellValue(`${indexToCol(c)}${r}`, sheet))
      out.push(row)
    }
    return out
  }

  eval(node: Node): Value {
    switch (node.k) {
      case 'num': return node.v
      case 'str': return node.v
      case 'bool': return node.v
      case 'ref': return this.cellValue(node.v, node.sheet)
      case 'range': return this.range(node.a, node.b, node.sheet)
      case 'unary': return mapValue(this.eval(node.e), s => {
        const num = toNum(s); if (isErr(num)) return num
        return node.op === '-' ? -num : num
      })
      case 'postfix': return mapValue(this.eval(node.e), s => {
        const num = toNum(s); if (isErr(num)) return num
        return num / 100
      })
      case 'bin': return this.binary(node)
      case 'call': return this.call(node.name, node.args)
      case 'array': return this.array(node.rows)
      case 'spillref': {
        const a = this.spill?.anchors[node.ref]
        if (!a || a.blocked) return ERR.REF
        return a.matrix
      }
      case 'var': {
        const v = this.scope[node.name]
        if (v !== undefined) return v as Value
        const nm = this.resolveName(node.name)   // fall back to a workbook defined name
        return (nm === undefined ? ERR.NAME : nm) as Value
      }
      case 'apply': {
        const fn = this.eval(node.fn) as Value | Lambda
        if (!isLambda(fn)) return ERR.CALC
        return this.applyLambda(fn, node.args.map(a => this.eval(a) as Value | Lambda)) as Value
      }
    }
  }

  // Array constant {1,2;3,4} → a rectangular Matrix (rows padded with #N/A).
  array(rows: Node[][]): Value {
    const out: Matrix = []
    let width = 0
    for (const row of rows) { if (row.length > width) width = row.length }
    if (width === 0) return ERR.VALUE
    for (const row of rows) {
      const r: Scalar[] = []
      for (const cell of row) r.push(this.scalar(cell))
      while (r.length < width) r.push(ERR.NA)
      out.push(r)
    }
    return out
  }

  scalar(node: Node): Scalar {
    const v = this.eval(node)
    if (isMatrix(v)) return v[0]?.[0] ?? ''
    return v
  }

  binary(node: { op: string; l: Node; r: Node }): Value {
    const L = this.eval(node.l), R = this.eval(node.r)
    // Element-wise broadcasting when either operand is an array (Excel dynamic arrays).
    if (isMatrix(L) || isMatrix(R)) return broadcast(L, R, (a, b) => this.binScalar(node.op, a, b))
    return this.binScalar(node.op, L, R)
  }

  binScalar(op: string, L: Scalar, R: Scalar): Scalar {
    if (isErr(L)) return L; if (isErr(R)) return R
    if (op === '&') return toStr(L) + toStr(R)
    if (['=', '<>', '<', '>', '<=', '>='].includes(op)) {
      let cmp: number
      if (typeof L === 'number' && typeof R === 'number') cmp = L - R
      else if (typeof L === 'boolean' || typeof R === 'boolean') cmp = (toBool(L) ? 1 : 0) - (toBool(R) ? 1 : 0)
      else cmp = toStr(L).toLowerCase() < toStr(R).toLowerCase() ? -1 : toStr(L).toLowerCase() > toStr(R).toLowerCase() ? 1 : 0
      switch (op) {
        case '=': return cmp === 0
        case '<>': return cmp !== 0
        case '<': return cmp < 0
        case '>': return cmp > 0
        case '<=': return cmp <= 0
        case '>=': return cmp >= 0
      }
    }
    const a = toNum(L), b = toNum(R)
    if (isErr(a)) return a; if (isErr(b)) return b
    switch (op) {
      case '+': return a + b
      case '-': return a - b
      case '*': return a * b
      case '/': return b === 0 ? ERR.DIV0 : a / b
      case '^': return Math.pow(a, b)
    }
    return ERR.VALUE
  }

  // valeurs numériques d'un argument (plage ou scalaire), en ignorant le texte/vides
  nums(args: Node[]): number[] | FErr {
    const out: number[] = []
    for (const arg of args) {
      const v = this.eval(arg)
      for (const s of flatten(v)) {
        if (isErr(s)) return s
        if (typeof s === 'number') out.push(s)
        else if (typeof s === 'boolean') out.push(s ? 1 : 0)
        else if (typeof s === 'string' && s !== '' && !isNaN(Number(s))) out.push(Number(s))
      }
    }
    return out
  }

  call(name: string, args: Node[]): Value {
    const fn = FUNCTIONS[name]
    if (fn) { try { return fn(this, args) } catch (e) { return isErr(e) ? e : ERR.VALUE } }
    // Not a built-in: a LET/LAMBDA-bound name OR a workbook defined name used as a
    // function (e.g. a named LAMBDA) — resolve and apply it.
    const bound = this.scope[name] !== undefined ? this.scope[name] : this.resolveName(name)
    if (isLambda(bound)) return this.applyLambda(bound, args.map(a => this.eval(a) as Value | Lambda)) as Value
    return ERR.NAME
  }
}

// ── Registre de fonctions ───────────────────────────────────────────────────────
export type Fn = (ev: Evaluator, args: Node[]) => Value
export const num = (ev: Evaluator, n: Node): number | FErr => { const s = ev.scalar(n); if (isErr(s)) return s; return toNum(s) }
export const str = (ev: Evaluator, n: Node): string => toStr(ev.scalar(n))
export const matchCriteria = (cell: Scalar, crit: Scalar): boolean => {
  const cs = toStr(crit).trim()
  const m = cs.match(/^(<=|>=|<>|=|<|>)(.*)$/)
  if (m) {
    const [, op, rhsRaw] = m
    const rhs = rhsRaw.trim()
    const cn = typeof cell === 'number' ? cell : Number(cell)
    const rn = Number(rhs)
    if (!isNaN(cn) && !isNaN(rn)) {
      switch (op) { case '<=': return cn <= rn; case '>=': return cn >= rn; case '<>': return cn !== rn; case '=': return cn === rn; case '<': return cn < rn; case '>': return cn > rn }
    }
    const a = toStr(cell).toLowerCase(), b = rhs.toLowerCase()
    if (op === '=') return a === b; if (op === '<>') return a !== b
    return false
  }
  // joker * / ?
  if (cs.includes('*') || cs.includes('?')) {
    const re = new RegExp('^' + cs.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.') + '$', 'i')
    return re.test(toStr(cell))
  }
  if (typeof cell === 'number' && !isNaN(Number(cs))) return cell === Number(cs)
  return toStr(cell).toLowerCase() === cs.toLowerCase()
}

const FUNCTIONS: Record<string, Fn> = {
  // Excel categories first; the base implementations below TAKE PRECEDENCE (same names).
  ...MATH_FNS, ...STAT_FNS, ...TEXT_FNS, ...DATE_FNS, ...LOGICAL_FNS, ...LOOKUP_FNS, ...FINANCIAL_FNS, ...ENGINEERING_FNS,
  ...STAT2_FNS, ...FINANCIAL2_FNS, ...ENGINEERING2_FNS, ...MISC_FNS,

  // ── First-class functions: LAMBDA / LET and the lambda-helper family ──────────
  // LAMBDA(p1, …, pN, calculation) → a callable value capturing the current scope.
  LAMBDA: (ev, a) => {
    if (a.length < 1) return ERR.VALUE
    const params: string[] = []
    for (let i = 0; i < a.length - 1; i++) { const p = a[i]; if (p.k !== 'var') return ERR.VALUE; params.push(p.name) }
    return ev.makeLambda(params, a[a.length - 1]) as unknown as Value
  },
  // LET(name1, value1, [name2, value2, …], calculation) — bind names then compute.
  LET: (ev, a) => {
    if (a.length < 3 || a.length % 2 === 0) return ERR.VALUE
    let e = ev
    for (let i = 0; i < a.length - 1; i += 2) {
      const nameNode = a[i]; if (nameNode.k !== 'var') return ERR.VALUE
      const val = e.eval(a[i + 1]) as Value | Lambda // later names may reference earlier ones
      e = e.withScope({ [nameNode.name]: val })
    }
    return e.eval(a[a.length - 1])
  },
  // MAP(array1, [array2, …], lambda) → element-wise lambda over same-shape arrays.
  MAP: (ev, a) => {
    if (a.length < 2) return ERR.VALUE
    const lam = ev.eval(a[a.length - 1]) as Value | Lambda
    if (!isLambda(lam)) return ERR.VALUE
    const arrays = a.slice(0, -1).map(n => asMat(ev.eval(n)))
    const rows = arrays[0].length, cols = arrays[0].reduce((w, r) => Math.max(w, r.length), 0)
    const out: Matrix = []
    for (let i = 0; i < rows; i++) {
      const row: Scalar[] = []
      for (let j = 0; j < cols; j++) row.push(lamScalar(ev.applyLambda(lam, arrays.map(m => (m[i]?.[j] ?? ERR.NA) as Value | Lambda))))
      out.push(row)
    }
    return out
  },
  // REDUCE(initial, array, lambda(acc, value)) → fold in row-major order.
  REDUCE: (ev, a) => {
    if (a.length !== 3) return ERR.VALUE
    const lam = ev.eval(a[2]) as Value | Lambda; if (!isLambda(lam)) return ERR.VALUE
    let acc = ev.eval(a[0]) as Value | Lambda
    for (const s of flatten(asMat(ev.eval(a[1])))) acc = ev.applyLambda(lam, [acc, s])
    return lamScalar(acc)
  },
  // SCAN(initial, array, lambda(acc, value)) → REDUCE keeping every intermediate.
  SCAN: (ev, a) => {
    if (a.length !== 3) return ERR.VALUE
    const lam = ev.eval(a[2]) as Value | Lambda; if (!isLambda(lam)) return ERR.VALUE
    let acc = ev.eval(a[0]) as Value | Lambda
    return asMat(ev.eval(a[1])).map(row => row.map(s => { acc = ev.applyLambda(lam, [acc, s]); return lamScalar(acc) }))
  },
  // BYROW(array, lambda(row)) → apply lambda to each row, returns a column vector.
  BYROW: (ev, a) => {
    if (a.length !== 2) return ERR.VALUE
    const m = asMat(ev.eval(a[0]))
    const lam = ev.eval(a[1]) as Value | Lambda; if (!isLambda(lam)) return ERR.VALUE
    return m.map(row => [lamScalar(ev.applyLambda(lam, [[row]]))])
  },
  // BYCOL(array, lambda(col)) → apply lambda to each column, returns a row vector.
  BYCOL: (ev, a) => {
    if (a.length !== 2) return ERR.VALUE
    const m = asMat(ev.eval(a[0]))
    const lam = ev.eval(a[1]) as Value | Lambda; if (!isLambda(lam)) return ERR.VALUE
    const cols = m.reduce((w, r) => Math.max(w, r.length), 0)
    const row: Scalar[] = []
    for (let j = 0; j < cols; j++) row.push(lamScalar(ev.applyLambda(lam, [m.map(r => [r[j] ?? ERR.NA])])))
    return [row]
  },
  // MAKEARRAY(rows, cols, lambda(row, col)) → build a matrix from 1-based indices.
  MAKEARRAY: (ev, a) => {
    if (a.length !== 3) return ERR.VALUE
    const rn = num(ev, a[0]); if (isErr(rn)) return rn
    const cn = num(ev, a[1]); if (isErr(cn)) return cn
    const R = Math.trunc(rn), C = Math.trunc(cn)
    if (R < 1 || C < 1 || R * C > 200000) return ERR.VALUE
    const lam = ev.eval(a[2]) as Value | Lambda; if (!isLambda(lam)) return ERR.VALUE
    const out: Matrix = []
    for (let i = 1; i <= R; i++) { const row: Scalar[] = []; for (let j = 1; j <= C; j++) row.push(lamScalar(ev.applyLambda(lam, [i, j]))); out.push(row) }
    return out
  },

  SUM: (ev, a) => { const n = ev.nums(a); return isErr(n) ? n : n.reduce((x, y) => x + y, 0) },
  AVERAGE: (ev, a) => { const n = ev.nums(a); if (isErr(n)) return n; return n.length ? n.reduce((x, y) => x + y, 0) / n.length : ERR.DIV0 },
  MIN: (ev, a) => { const n = ev.nums(a); if (isErr(n)) return n; return n.length ? Math.min(...n) : 0 },
  MAX: (ev, a) => { const n = ev.nums(a); if (isErr(n)) return n; return n.length ? Math.max(...n) : 0 },
  COUNT: (ev, a) => { const n = ev.nums(a); return isErr(n) ? n : n.length },
  COUNTA: (ev, a) => { let c = 0; for (const arg of a) for (const s of flatten(ev.eval(arg))) if (!(s === '' || s == null)) c++; return c },
  COUNTBLANK: (ev, a) => { let c = 0; for (const arg of a) for (const s of flatten(ev.eval(arg))) if (s === '' || s == null) c++; return c },
  PRODUCT: (ev, a) => { const n = ev.nums(a); if (isErr(n)) return n; return n.reduce((x, y) => x * y, 1) },
  MEDIAN: (ev, a) => { const n = ev.nums(a); if (isErr(n)) return n; if (!n.length) return ERR.NUM; const s = [...n].sort((x, y) => x - y); const m = Math.floor(s.length / 2); return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2 },
  IF: (ev, a) => { if (a.length < 2) return ERR.VALUE; const c = ev.scalar(a[0]); if (isErr(c)) return c; return toBool(c) ? ev.eval(a[1]) : (a[2] ? ev.eval(a[2]) : false) },
  IFERROR: (ev, a) => { const v = ev.eval(a[0]); return isErr(v) || (isMatrix(v) && isErr(v[0]?.[0])) ? ev.eval(a[1]) : v },
  IFS: (ev, a) => { for (let i = 0; i + 1 < a.length; i += 2) { const c = ev.scalar(a[i]); if (isErr(c)) return c; if (toBool(c)) return ev.eval(a[i + 1]) } return ERR.NA },
  AND: (ev, a) => { for (const arg of a) for (const s of flatten(ev.eval(arg))) { if (isErr(s)) return s; if (!toBool(s)) return false } return true },
  OR: (ev, a) => { for (const arg of a) for (const s of flatten(ev.eval(arg))) { if (isErr(s)) return s; if (toBool(s)) return true } return false },
  NOT: (ev, a) => { const s = ev.scalar(a[0]); return isErr(s) ? s : !toBool(s) },
  ROUND: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const d = a[1] ? num(ev, a[1]) : 0; if (isErr(d)) return d; const f = Math.pow(10, d); return Math.round(x * f) / f },
  ROUNDUP: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const d = a[1] ? num(ev, a[1]) : 0; if (isErr(d)) return d; const f = Math.pow(10, d); return (x < 0 ? -1 : 1) * Math.ceil(Math.abs(x) * f) / f },
  ROUNDDOWN: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const d = a[1] ? num(ev, a[1]) : 0; if (isErr(d)) return d; const f = Math.pow(10, d); return (x < 0 ? -1 : 1) * Math.floor(Math.abs(x) * f) / f },
  FLOOR: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const m = a[1] ? num(ev, a[1]) : 1; if (isErr(m)) return m; return m === 0 ? 0 : Math.floor(x / m) * m },
  CEILING: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const m = a[1] ? num(ev, a[1]) : 1; if (isErr(m)) return m; return m === 0 ? 0 : Math.ceil(x / m) * m },
  INT: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.floor(x) },
  ABS: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.abs(x) },
  MOD: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const d = num(ev, a[1]); if (isErr(d)) return d; return d === 0 ? ERR.DIV0 : ((x % d) + d) % d },
  POWER: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const p = num(ev, a[1]); if (isErr(p)) return p; return Math.pow(x, p) },
  SQRT: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x < 0 ? ERR.NUM : Math.sqrt(x) },
  EXP: (ev, a) => { const x = num(ev, a[0]); return isErr(x) ? x : Math.exp(x) },
  LN: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; return x <= 0 ? ERR.NUM : Math.log(x) },
  LOG: (ev, a) => { const x = num(ev, a[0]); if (isErr(x)) return x; const b = a[1] ? num(ev, a[1]) : 10; if (isErr(b)) return b; return x <= 0 ? ERR.NUM : Math.log(x) / Math.log(b) },
  PI: () => Math.PI,
  SUMIF: (ev, a) => sumIf(ev, a, false),
  AVERAGEIF: (ev, a) => sumIf(ev, a, true),
  COUNTIF: (ev, a) => { const range = ev.eval(a[0]); const crit = ev.scalar(a[1]); let c = 0; for (const s of flatten(range)) if (matchCriteria(s, crit)) c++; return c },
  SUMIFS: (ev, a) => sumIfs(ev, a),
  COUNTIFS: (ev, a) => countIfs(ev, a),
  VLOOKUP: (ev, a) => lookup(ev, a, 'v'),
  HLOOKUP: (ev, a) => lookup(ev, a, 'h'),
  INDEX: (ev, a) => {
    const m = ev.eval(a[0]); if (!isMatrix(m)) return ERR.REF
    const r = a[1] ? num(ev, a[1]) : 1; if (isErr(r)) return r
    const c = a[2] ? num(ev, a[2]) : (m[0]?.length === 1 ? 1 : (r as number)) // gestion vecteur
    if (a.length < 3 && m.length === 1) { const cc = r as number; return m[0]?.[cc - 1] ?? ERR.REF }
    const cn = isErr(c) ? 1 : (c as number)
    return m[(r as number) - 1]?.[cn - 1] ?? ERR.REF
  },
  MATCH: (ev, a) => {
    const target = ev.scalar(a[0]); const vec = flatten(ev.eval(a[1])); const type = a[2] ? num(ev, a[2]) : 1
    if (isErr(type)) return type
    if (type === 0) { for (let i = 0; i < vec.length; i++) if (toStr(vec[i]).toLowerCase() === toStr(target).toLowerCase()) return i + 1; return ERR.NA }
    let best = -1
    for (let i = 0; i < vec.length; i++) {
      const cmp = compareScalar(vec[i], target)
      if (type === 1 && cmp <= 0) best = i
      if (type === -1 && cmp >= 0) best = i
    }
    return best < 0 ? ERR.NA : best + 1
  },
  LEN: (ev, a) => str(ev, a[0]).length,
  LEFT: (ev, a) => { const s = str(ev, a[0]); const n = a[1] ? num(ev, a[1]) : 1; if (isErr(n)) return n; return s.slice(0, n) },
  RIGHT: (ev, a) => { const s = str(ev, a[0]); const n = a[1] ? num(ev, a[1]) : 1; if (isErr(n)) return n; return n <= 0 ? '' : s.slice(-n) },
  MID: (ev, a) => { const s = str(ev, a[0]); const st = num(ev, a[1]); if (isErr(st)) return st; const ln = num(ev, a[2]); if (isErr(ln)) return ln; return s.slice(st - 1, st - 1 + ln) },
  TRIM: (ev, a) => str(ev, a[0]).replace(/\s+/g, ' ').trim(),
  UPPER: (ev, a) => str(ev, a[0]).toUpperCase(),
  LOWER: (ev, a) => str(ev, a[0]).toLowerCase(),
  PROPER: (ev, a) => str(ev, a[0]).replace(/\b\w/g, c => c.toUpperCase()).replace(/\B\w/g, c => c.toLowerCase()),
  CONCAT: (ev, a) => { let s = ''; for (const arg of a) for (const x of flatten(ev.eval(arg))) s += toStr(x); return s },
  CONCATENATE: (ev, a) => { let s = ''; for (const arg of a) s += str(ev, arg); return s },
  TEXTJOIN: (ev, a) => { const sep = str(ev, a[0]); const skip = toBool(ev.scalar(a[1])); const parts: string[] = []; for (let i = 2; i < a.length; i++) for (const x of flatten(ev.eval(a[i]))) { const t = toStr(x); if (!(skip && t === '')) parts.push(t) } return parts.join(sep) },
  REPLACE: (ev, a) => { const s = str(ev, a[0]); const st = num(ev, a[1]); if (isErr(st)) return st; const ln = num(ev, a[2]); if (isErr(ln)) return ln; const ns = str(ev, a[3]); return s.slice(0, st - 1) + ns + s.slice(st - 1 + ln) },
  SUBSTITUTE: (ev, a) => { const s = str(ev, a[0]); const oldT = str(ev, a[1]); const newT = str(ev, a[2]); return s.split(oldT).join(newT) },
  TEXT: (ev, a) => { const v = ev.scalar(a[0]); const fmt = str(ev, a[1]); return formatText(v, fmt) },
  VALUE: (ev, a) => { const s = str(ev, a[0]); const n = Number(s.replace(/[^0-9.\-eE]/g, '')); return isNaN(n) ? ERR.VALUE : n },
  TODAY: () => { const d = new Date(); return excelSerial(new Date(d.getFullYear(), d.getMonth(), d.getDate())) },
  NOW: () => excelSerial(new Date()),
  YEAR: (ev, a) => { const d = serialToDate(num(ev, a[0])); return d ? d.getFullYear() : ERR.VALUE },
  MONTH: (ev, a) => { const d = serialToDate(num(ev, a[0])); return d ? d.getMonth() + 1 : ERR.VALUE },
  DAY: (ev, a) => { const d = serialToDate(num(ev, a[0])); return d ? d.getDate() : ERR.VALUE },
  ISBLANK: (ev, a) => { const c = ev.eval(a[0]); const s = isMatrix(c) ? c[0]?.[0] : c; return s === '' || s == null },
  ISNUMBER: (ev, a) => typeof ev.scalar(a[0]) === 'number',
  ISTEXT: (ev, a) => typeof ev.scalar(a[0]) === 'string',
  ISERROR: (ev, a) => isErr(ev.scalar(a[0])),
  ISLOGICAL: (ev, a) => typeof ev.scalar(a[0]) === 'boolean',
  TRUE: () => true,
  FALSE: () => false,
}

export function compareScalar(a: Scalar, b: Scalar): number {
  if (typeof a === 'number' && typeof b === 'number') return a - b
  const as = toStr(a).toLowerCase(), bs = toStr(b).toLowerCase()
  return as < bs ? -1 : as > bs ? 1 : 0
}
function sumIf(ev: Evaluator, a: Node[], avg: boolean): Value {
  const range = ev.eval(a[0]); const crit = ev.scalar(a[1])
  const sumRange = a[2] ? ev.eval(a[2]) : range
  const rf = flatten(range), sf = flatten(sumRange)
  let sum = 0, cnt = 0
  for (let i = 0; i < rf.length; i++) if (matchCriteria(rf[i], crit)) { const v = toNum(sf[i] ?? 0); if (!isErr(v)) { sum += v; cnt++ } }
  return avg ? (cnt ? sum / cnt : ERR.DIV0) : sum
}
function sumIfs(ev: Evaluator, a: Node[]): Value {
  const sumRange = flatten(ev.eval(a[0]))
  const pairs: { range: Scalar[]; crit: Scalar }[] = []
  for (let i = 1; i + 1 < a.length; i += 2) pairs.push({ range: flatten(ev.eval(a[i])), crit: ev.scalar(a[i + 1]) })
  let sum = 0
  for (let i = 0; i < sumRange.length; i++) { if (pairs.every(p => matchCriteria(p.range[i], p.crit))) { const v = toNum(sumRange[i] ?? 0); if (!isErr(v)) sum += v } }
  return sum
}
function countIfs(ev: Evaluator, a: Node[]): Value {
  const pairs: { range: Scalar[]; crit: Scalar }[] = []
  for (let i = 0; i + 1 < a.length; i += 2) pairs.push({ range: flatten(ev.eval(a[i])), crit: ev.scalar(a[i + 1]) })
  if (!pairs.length) return 0
  let c = 0
  for (let i = 0; i < pairs[0].range.length; i++) if (pairs.every(p => matchCriteria(p.range[i], p.crit))) c++
  return c
}
function lookup(ev: Evaluator, a: Node[], kind: 'v' | 'h'): Value {
  const target = ev.scalar(a[0]); const m = ev.eval(a[1])
  if (!isMatrix(m)) return ERR.REF
  const idx = num(ev, a[2]); if (isErr(idx)) return idx
  const exact = a[3] ? !toBool(ev.scalar(a[3])) : false
  if (kind === 'v') {
    for (const row of m) {
      if (exact ? toStr(row[0]).toLowerCase() === toStr(target).toLowerCase() : matchApprox(row[0], target)) return row[(idx as number) - 1] ?? ERR.REF
    }
  } else {
    const head = m[0]
    for (let c = 0; c < head.length; c++) if (exact ? toStr(head[c]).toLowerCase() === toStr(target).toLowerCase() : matchApprox(head[c], target)) return m[(idx as number) - 1]?.[c] ?? ERR.REF
  }
  return ERR.NA
}
function matchApprox(cell: Scalar, target: Scalar): boolean { return compareScalar(cell, target) === 0 }

// ── Dates (sérial Excel : jours depuis 1899-12-30) ───────────────────────────────
const EXCEL_EPOCH = Date.UTC(1899, 11, 30)
// Sérial Excel STABLE au fuseau : on calcule depuis les composantes LOCALES via
// Date.UTC (sinon le décalage UTC introduit une partie fractionnaire → DATE() non
// entière, comparaisons de dates faussées). serialToDate reconstruit une date locale
// dont les composantes y/m/d correspondent (pour YEAR/MONTH/DAY locaux).
export function excelSerial(d: Date): number {
  const utc = Date.UTC(d.getFullYear(), d.getMonth(), d.getDate(), d.getHours(), d.getMinutes(), d.getSeconds())
  const raw = (utc - EXCEL_EPOCH) / 86400000
  // Excel's 1900 date system pretends 1900 was a leap year (the phantom
  // 1900-02-29 = serial 60). Real dates up to 1900-02-28 are therefore one
  // ahead of the naive offset; shift them back so serial 1 = 1900-01-01.
  return raw < 61 ? raw - 1 : raw
}
export function serialToDate(n: number | FErr): Date | null {
  if (isErr(n)) return null
  const days = n < 60 ? n + 1 : n // undo the phantom-leap-day offset below serial 60
  const u = new Date(EXCEL_EPOCH + days * 86400000)
  if (isNaN(u.getTime())) return null
  return new Date(u.getUTCFullYear(), u.getUTCMonth(), u.getUTCDate(), u.getUTCHours(), u.getUTCMinutes(), u.getUTCSeconds())
}
// French/English day & month names for date formatting (TEXT / number formats).
const DAY_ABBR = ['dim', 'lun', 'mar', 'mer', 'jeu', 'ven', 'sam']
const DAY_FULL = ['dimanche', 'lundi', 'mardi', 'mercredi', 'jeudi', 'vendredi', 'samedi']
const MON_ABBR = ['janv', 'févr', 'mars', 'avr', 'mai', 'juin', 'juil', 'août', 'sept', 'oct', 'nov', 'déc']
const MON_FULL = ['janvier', 'février', 'mars', 'avril', 'mai', 'juin', 'juillet', 'août', 'septembre', 'octobre', 'novembre', 'décembre']

// A format is a date/time format if (ignoring quoted literals) it uses date/time
// letters and isn't a plain numeric format. Supports FR (J/M/A) and EN (d/m/y) codes.
function isDateFormat(fmt: string): boolean {
  const s = fmt.replace(/"[^"]*"/g, '').replace(/\[[^\]]*\]/g, '')
  if (/[#0]/.test(s)) return false
  return /[jdayhs]/i.test(s) && /[jdma]/i.test(s)
}

// Format an Excel serial as a date/time string per a format like "JJJ", "dd/mm/yyyy".
export function formatDateSerial(serial: number, fmt: string): string {
  const d = serialToDate(serial)
  if (!d) return String(serial)
  const day = d.getDate(), mon = d.getMonth(), yr = d.getFullYear(), dow = d.getDay()
  const hr = d.getHours(), se = d.getSeconds(), mi = d.getMinutes()
  const pad = (n: number) => String(n).padStart(2, '0')
  let out = '', i = 0
  while (i < fmt.length) {
    const c = fmt[i]
    if (c === '"') { let j = i + 1; while (j < fmt.length && fmt[j] !== '"') j++; out += fmt.slice(i + 1, j); i = j + 1; continue }
    const lc = c.toLowerCase()
    if (lc === 'j' || lc === 'd') { let n = 0; while (i < fmt.length && fmt[i].toLowerCase() === lc) { n++; i++ }; out += n === 1 ? String(day) : n === 2 ? pad(day) : n === 3 ? DAY_ABBR[dow] : DAY_FULL[dow]; continue }
    if (lc === 'm') {
      let n = 0; while (i < fmt.length && fmt[i].toLowerCase() === 'm') { n++; i++ }
      const prev = out.replace(/\s$/, '').slice(-1) // minutes when right after hours/':'
      if (prev === ':' || /[hH]/.test(prev)) { out += n >= 2 ? pad(mi) : String(mi) }
      else { out += n === 1 ? String(mon + 1) : n === 2 ? pad(mon + 1) : n === 3 ? MON_ABBR[mon] : MON_FULL[mon] }
      continue
    }
    if (lc === 'a' || lc === 'y') { let n = 0; while (i < fmt.length && /[ayAY]/.test(fmt[i])) { n++; i++ }; out += n <= 2 ? String(yr).slice(-2) : String(yr); continue }
    if (lc === 'h') { let n = 0; while (i < fmt.length && fmt[i].toLowerCase() === 'h') { n++; i++ }; out += n >= 2 ? pad(hr) : String(hr); continue }
    if (lc === 's') { let n = 0; while (i < fmt.length && fmt[i].toLowerCase() === 's') { n++; i++ }; out += n >= 2 ? pad(se) : String(se); continue }
    out += c; i++
  }
  return out
}

/** Format a numeric value per a number-format code's first section (handles leading
 *  zeros like "00", decimals, thousands grouping and a trailing "%"). */
function formatNumberCode(value: number, sec: string): string {
  if (sec === '@' || sec === '') return String(value)
  const isPercent = sec.includes('%')
  let v = isPercent ? value * 100 : value
  const neg = v < 0; v = Math.abs(v)
  const dot = sec.indexOf('.')
  const intTok = (dot >= 0 ? sec.slice(0, dot) : sec).replace(/[^0#,]/g, '')
  const fracTok = dot >= 0 ? sec.slice(dot + 1).replace(/[^0#]/g, '') : ''
  const dec = fracTok.length
  const minInt = (intTok.replace(/,/g, '').match(/0/g) ?? []).length
  const grouping = intTok.includes(',')
  let [ip, fp] = v.toFixed(dec).split('.')
  if (ip.length < minInt) ip = ip.padStart(minInt, '0')
  if (grouping) ip = ip.replace(/\B(?=(\d{3})+(?!\d))/g, ' ') // narrow no-break space
  let out = fp ? `${ip}.${fp}` : ip
  if (isPercent) out += ' %'
  return neg ? `-${out}` : out
}

/** Display a value per a raw number-format code (date or numeric). */
export function formatCode(value: number, code: string): string {
  if (isDateFormat(code)) return formatDateSerial(value, code)
  return formatNumberCode(value, code.split(';')[0])
}

function formatText(v: Scalar, fmt: string): string {
  if (typeof v === 'number') {
    if (isDateFormat(fmt)) return formatDateSerial(v, fmt)
    if (/0|#/.test(fmt)) {
      const dec = (fmt.split('.')[1] || '').replace(/[^0#]/g, '').length
      let s = v.toFixed(dec)
      if (/,/.test(fmt)) { const [int, frac] = s.split('.'); s = int.replace(/\B(?=(\d{3})+(?!\d))/g, ',') + (frac ? '.' + frac : '') }
      return s
    }
  }
  return toStr(v)
}

// ── API publique ─────────────────────────────────────────────────────────────────
/** Tokenise + parse a formula body into an AST (or an FErr). Empty → empty string. */
function parseFormula(formula: string): Node | FErr {
  const body = formula.startsWith('=') ? formula.slice(1) : formula
  const toks = tokenize(body)
  if (isErr(toks)) return toks
  if (toks.length === 0) return { k: 'str', v: '' }
  try { const p = new Parser(toks); const ast = p.parse(); if (p.peek()) return ERR.VALUE; return ast } catch (e) { return isErr(e) ? e : ERR.VALUE }
}

/** Évalue une formule "=..." et renvoie la valeur (ou une FErr). */
export function evaluate(formula: string, data: SheetData, visiting: Set<string> = new Set(), spill?: SpillIndex): Value {
  const ast = parseFormula(formula)
  if (isErr(ast)) return ast
  const ev = new Evaluator(data, visiting, spill)
  try { const r = ev.eval(ast); return isLambda(r) ? ERR.CALC : r } catch (e) { return isErr(e) ? e : ERR.VALUE }
}

/** Validate a defined-name definition string; returns an error message or null. */
export function validateNameFormula(formula: string): string | null {
  const ast = parseFormula(formula)
  return isErr(ast) ? ast.err : null
}

/** A defined name is a valid Excel-style identifier (letter/underscore start, not a cell ref). */
export function isValidDefinedName(name: string): boolean {
  if (!/^[A-Za-z_][A-Za-z0-9_.]*$/.test(name)) return false
  if (/^[A-Za-z]{1,3}[0-9]{1,7}$/.test(name)) return false // looks like a cell reference
  if (/^(TRUE|FALSE)$/i.test(name)) return false
  return true
}

// ── Conditional formatting ───────────────────────────────────────────────────
/** Shift the RELATIVE references (those without `$`) of a formula by (dCol, dRow).
 *  Skips strings, sheet-qualified refs and function names. Mirrors the importer's
 *  shared-formula translation; used to anchor a CF rule formula to each cell. */
export function translateRefs(formula: string, dCol: number, dRow: number): string {
  if (dCol === 0 && dRow === 0) return formula
  let out = '', i = 0, inStr = false
  while (i < formula.length) {
    const c = formula[i]
    if (inStr) { out += c; if (c === '"') inStr = false; i++; continue }
    if (c === '"') { inStr = true; out += c; i++; continue }
    const prev = out[out.length - 1] ?? ' '
    const boundary = !/[A-Za-z0-9_!'$.]/.test(prev)
    if (boundary && (c === '$' || /[A-Za-z]/.test(c))) {
      let j = i; const colAbs = formula[j] === '$'; if (colAbs) j++
      const ls = j; while (j < formula.length && /[A-Za-z]/.test(formula[j])) j++
      const letters = formula.slice(ls, j)
      const rowAbs = formula[j] === '$'; const ds = rowAbs ? j + 1 : j
      let k = ds; while (k < formula.length && /[0-9]/.test(formula[k])) k++
      const digits = formula.slice(ds, k)
      const next = formula[k] ?? ' '
      if (letters && letters.length <= 3 && digits && digits.length <= 7 && next !== '(') {
        let col = colToIndex(letters.toUpperCase()), row = parseInt(digits, 10)
        if (!colAbs) col += dCol
        if (!rowAbs) row += dRow
        if (col < 0) col = 0
        if (row < 1) row = 1
        out += (colAbs ? '$' : '') + indexToCol(col) + (rowAbs ? '$' : '') + row
        i = k; continue
      }
    }
    out += c; i++
  }
  return out
}

export interface CondStyle { bg?: string; color?: string; bold?: boolean; italic?: boolean }
export interface CondRule { type: string; op: string; formulas: string[]; dxf: CondStyle; stop: boolean }
export interface CondBlock { ranges: string[]; rules: CondRule[] }

const truthyCF = (v: Value): boolean => {
  const s = isMatrix(v) ? (v[0]?.[0] ?? false) : v
  if (isErr(s)) return false
  if (typeof s === 'boolean') return s
  if (typeof s === 'number') return s !== 0
  return false
}
function parseA1Rect(ref: string): { c1: number; r1: number; c2: number; r2: number } | null {
  const m = /^\$?([A-Za-z]{1,3})\$?([0-9]{1,7})(?::\$?([A-Za-z]{1,3})\$?([0-9]{1,7}))?$/.exec(ref.trim())
  if (!m) return null
  const c1 = colToIndex(m[1].toUpperCase()), r1 = +m[2]
  const c2 = m[3] ? colToIndex(m[3].toUpperCase()) : c1, r2 = m[4] ? +m[4] : r1
  return { c1: Math.min(c1, c2), r1: Math.min(r1, r2), c2: Math.max(c1, c2), r2: Math.max(r1, r2) }
}

/** Evaluate the workbook's conditional-formatting rules → per-cell style overrides
 *  (bg/colour/bold). Each `expression` rule's formula is anchored at the top-left of
 *  its range and translated to every cell; the first matching rule wins. */
export function computeCondFormats(data: SheetData): Record<string, CondStyle> {
  const out: Record<string, CondStyle> = {}
  const cf = (data as SheetData & { cf?: CondBlock[] }).cf
  if (!cf || !cf.length) return out
  const MAX_CELLS = 60000
  for (const block of cf) {
    const rects: { c1: number; r1: number; c2: number; r2: number }[] = []
    let ar = Infinity, ac = Infinity
    for (const ref of block.ranges) { const b = parseA1Rect(ref); if (b) { rects.push(b); ar = Math.min(ar, b.r1); ac = Math.min(ac, b.c1) } }
    if (!rects.length) continue
    for (const b of rects) {
      if ((b.c2 - b.c1 + 1) * (b.r2 - b.r1 + 1) > MAX_CELLS) continue
      for (let r = b.r1; r <= b.r2; r++) for (let c = b.c1; c <= b.c2; c++) {
        const key = `${indexToCol(c)}${r}`
        if (out[key]) continue // first (highest-priority) block to match wins
        for (const rule of block.rules) {
          if (rule.type !== 'expression' || !rule.formulas[0]) continue
          if (truthyCF(evaluate(translateRefs(rule.formulas[0], c - ac, r - ar), data))) { out[key] = rule.dxf; break }
        }
      }
    }
  }
  return out
}

// ── Dynamic-array spilling ───────────────────────────────────────────────────────
/** Per-anchor spill geometry and the values spilled into neighbour cells. */
export interface SpillIndex {
  /** Non-anchor cell key ("B2") → the value spilled into it. */
  values: Record<string, Scalar>
  /** Anchor cell key → its full result matrix + geometry + blocked flag. */
  anchors: Record<string, { matrix: Matrix; rows: number; cols: number; blocked: boolean }>
  /** Every covered cell key (anchor included) → its anchor key. */
  origin: Record<string, string>
}

const MAX_SPILL_CELLS = 200000 // guard against pathological huge spills

/**
 * Scan every formula cell, evaluate it, and lay out the dynamic-array spills.
 * A formula whose result is a matrix larger than 1×1 spills from its cell
 * (the anchor, top-left) into the cells below/right. A spill is blocked
 * (#SPILL!) when a target cell already holds its own content or is already
 * claimed by an earlier spill. Iterates to a small fixpoint so a spill that
 * references another spilled cell still resolves.
 */
export function computeSpills(data: SheetData): SpillIndex {
  let prev: SpillIndex = { values: {}, anchors: {}, origin: {} }
  // Formula cells in row-major order → deterministic blocking/overlap resolution.
  const formulaKeys: { key: string; c: number; r: number }[] = []
  for (const key of Object.keys(data.cells)) {
    const cell = data.cells[key]
    if (!cell?.f || !cell.f.startsWith('=')) continue
    const m = /^([A-Z]+)([0-9]+)$/.exec(key)
    if (!m) continue
    formulaKeys.push({ key, c: colToIndex(m[1]), r: parseInt(m[2], 10) })
  }
  formulaKeys.sort((a, b) => a.r - b.r || a.c - b.c)

  for (let pass = 0; pass < 4; pass++) {
    const values: Record<string, Scalar> = {}
    const anchors: SpillIndex['anchors'] = {}
    const origin: Record<string, string> = {}
    const claimed: Record<string, string> = {}
    for (const { key, c, r } of formulaKeys) {
      const cell = data.cells[key]
      const v = evaluate(cell.f!, data, new Set(), prev)
      if (!isMatrix(v)) continue
      const rows = v.length
      const cols = v.reduce((w, row) => Math.max(w, row.length), 0)
      if (rows <= 1 && cols <= 1) continue // 1×1 → ordinary scalar, no spill
      let blocked = rows * cols > MAX_SPILL_CELLS
      const targets: { tkey: string; dr: number; dc: number }[] = []
      if (!blocked) {
        outer: for (let dr = 0; dr < rows; dr++) {
          for (let dc = 0; dc < cols; dc++) {
            if (dr === 0 && dc === 0) continue // anchor itself
            const tc = c + dc, tr = r + dr
            if (tc >= 16384 || tr > 1048576) { blocked = true; break outer }
            const tkey = `${indexToCol(tc)}${tr}`
            const tcell = data.cells[tkey]
            const hasOwn = tcell && (!!tcell.f || (tcell.v != null && tcell.v !== ''))
            if (hasOwn || claimed[tkey]) { blocked = true; break outer }
            targets.push({ tkey, dr, dc })
          }
        }
      }
      anchors[key] = { matrix: v, rows, cols, blocked }
      origin[key] = key
      if (blocked) continue
      for (const { tkey, dr, dc } of targets) {
        values[tkey] = v[dr]?.[dc] ?? ERR.NA
        origin[tkey] = key
        claimed[tkey] = key
      }
    }
    const next: SpillIndex = { values, anchors, origin }
    // Stop early once the spilled-value map stabilises (or nothing spilled).
    const a = Object.keys(prev.values), b = Object.keys(values)
    const stable = a.length === b.length && b.every(k => prev.values[k] === values[k])
    prev = next
    if (stable) break
  }
  return prev
}

/** Formate une valeur évaluée en chaîne pour l'affichage de la cellule. */
export function formatValue(v: Value): string {
  if (isLambda(v)) return ERR.CALC.err
  if (isMatrix(v)) v = v[0]?.[0] ?? ''
  if (isLambda(v)) return ERR.CALC.err
  if (isErr(v)) return v.err
  if (typeof v === 'boolean') return v ? 'TRUE' : 'FALSE'
  if (typeof v === 'number') {
    if (!isFinite(v)) return ERR.NUM.err
    return String(Math.round(v * 1e10) / 1e10)
  }
  return v == null ? '' : String(v)
}

/** Liste des noms de fonctions reconnus (pour l'autocomplétion). */
export const KNOWN_FUNCTIONS = Object.keys(FUNCTIONS)
