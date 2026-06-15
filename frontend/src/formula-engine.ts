// ── Moteur de formules du tableur ────────────────────────────────────────────
// Tokeniseur + parseur à descente récursive + évaluateur, avec registre de
// fonctions, références/plages de cellules (évaluation récursive + détection de
// cycles) et valeurs d'erreur façon Excel/Sheets.
//
// Remplace l'ancien `evaluateFormula` qui ne gérait qu'un seul `=SUM(A1:B2)`.

import type { SheetData, CellData } from './api'

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
  CIRC: { err: '#REF!' } as FErr, // circulaire → #REF!
}
export const isErr = (v: unknown): v is FErr =>
  typeof v === 'object' && v !== null && 'err' in (v as Record<string, unknown>)
const isMatrix = (v: Value): v is Matrix => Array.isArray(v)

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
  | { t: 'name'; v: string }         // nom de fonction
  | { t: 'op'; v: string }
  | { t: 'lp' } | { t: 'rp' } | { t: 'comma' } | { t: 'colon' }

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
    // $A$1 / A1 / nom de fonction / TRUE / FALSE
    if (/[A-Za-z$_]/.test(c)) {
      let j = i
      while (j < n && /[A-Za-z0-9$_.]/.test(src[j])) j++
      const word = src.slice(i, j); i = j
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
    if (c === ',' || c === ';') { toks.push({ t: 'comma' }); i++; continue }
    if (c === ':') { toks.push({ t: 'colon' }); i++; continue }
    return ERR.VALUE
  }
  return toks
}

// ── AST ─────────────────────────────────────────────────────────────────────────
type Node =
  | { k: 'num'; v: number }
  | { k: 'str'; v: string }
  | { k: 'bool'; v: boolean }
  | { k: 'ref'; v: string }
  | { k: 'range'; a: string; b: string }
  | { k: 'unary'; op: string; e: Node }
  | { k: 'postfix'; op: string; e: Node }
  | { k: 'bin'; op: string; l: Node; r: Node }
  | { k: 'call'; name: string; args: Node[] }

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
    while (this.peek()?.t === 'op' && (this.peek() as { v: string }).v === '%') { this.next(); e = { k: 'postfix', op: '%', e } }
    return e
  }
  parsePrimary(): Node {
    const tk = this.next()
    if (!tk) throw ERR.VALUE
    if (tk.t === 'num') return { k: 'num', v: tk.v }
    if (tk.t === 'str') return { k: 'str', v: tk.v }
    if (tk.t === 'bool') return { k: 'bool', v: tk.v }
    if (tk.t === 'ref') {
      if (this.peek()?.t === 'colon') { this.next(); const b = this.next(); if (b?.t !== 'ref') throw ERR.REF; return { k: 'range', a: tk.v, b: b.v } }
      return { k: 'ref', v: tk.v }
    }
    if (tk.t === 'name') {
      if (this.peek()?.t === 'lp') {
        this.next()
        const args: Node[] = []
        if (this.peek()?.t !== 'rp') {
          args.push(this.parseCompare())
          while (this.peek()?.t === 'comma') { this.next(); args.push(this.parseCompare()) }
        }
        if (this.next()?.t !== 'rp') throw ERR.VALUE
        return { k: 'call', name: tk.v, args }
      }
      throw ERR.NAME
    }
    if (tk.t === 'lp') { const e = this.parseCompare(); if (this.next()?.t !== 'rp') throw ERR.VALUE; return e }
    throw ERR.VALUE
  }
}

// ── Coercions ───────────────────────────────────────────────────────────────────
function toNum(v: Scalar): number | FErr {
  if (typeof v === 'number') return v
  if (typeof v === 'boolean') return v ? 1 : 0
  if (isErr(v)) return v
  if (v === '' || v == null) return 0
  const n = Number(v)
  return isNaN(n) ? ERR.VALUE : n
}
function toStr(v: Scalar): string {
  if (isErr(v)) return v.err
  if (typeof v === 'boolean') return v ? 'TRUE' : 'FALSE'
  if (v == null) return ''
  return String(v)
}
function toBool(v: Scalar): boolean {
  if (typeof v === 'boolean') return v
  if (typeof v === 'number') return v !== 0
  if (typeof v === 'string') return /^true$/i.test(v) ? true : v !== ''
  return false
}
function flatten(v: Value): Scalar[] {
  if (isMatrix(v)) return v.flat()
  return [v]
}

// ── Évaluateur ──────────────────────────────────────────────────────────────────
class Evaluator {
  constructor(private data: SheetData, private visiting: Set<string>) {}

  cellRaw(ref: string): CellData | undefined { return this.data.cells[ref] }

  cellValue(ref: string): Scalar {
    if (this.visiting.has(ref)) return ERR.CIRC
    const cell = this.cellRaw(ref)
    if (!cell) return ''
    if (cell.f && cell.f.startsWith('=')) {
      this.visiting.add(ref)
      const r = evaluate(cell.f, this.data, this.visiting)
      this.visiting.delete(ref)
      return isMatrix(r) ? (r[0]?.[0] ?? '') : r
    }
    const v = cell.v
    if (v == null) return ''
    if (typeof v === 'string' && v !== '' && !isNaN(Number(v))) return Number(v)
    return v as Scalar
  }

  range(a: string, b: string): Matrix {
    const ma = a.match(/^([A-Z]+)(\d+)$/), mb = b.match(/^([A-Z]+)(\d+)$/)
    if (!ma || !mb) return [[ERR.REF]]
    const c1 = colToIndex(ma[1]), c2 = colToIndex(mb[1])
    const r1 = parseInt(ma[2]), r2 = parseInt(mb[2])
    const lo = Math.min(c1, c2), hi = Math.max(c1, c2)
    const tr = Math.min(r1, r2), br = Math.max(r1, r2)
    const out: Matrix = []
    for (let r = tr; r <= br; r++) {
      const row: Scalar[] = []
      for (let c = lo; c <= hi; c++) row.push(this.cellValue(`${indexToCol(c)}${r}`))
      out.push(row)
    }
    return out
  }

  eval(node: Node): Value {
    switch (node.k) {
      case 'num': return node.v
      case 'str': return node.v
      case 'bool': return node.v
      case 'ref': return this.cellValue(node.v)
      case 'range': return this.range(node.a, node.b)
      case 'unary': {
        const e = this.scalar(node.e); if (isErr(e)) return e
        const num = toNum(e); if (isErr(num)) return num
        return node.op === '-' ? -num : num
      }
      case 'postfix': {
        const e = this.scalar(node.e); if (isErr(e)) return e
        const num = toNum(e); if (isErr(num)) return num
        return num / 100
      }
      case 'bin': return this.binary(node)
      case 'call': return this.call(node.name, node.args)
    }
  }

  scalar(node: Node): Scalar {
    const v = this.eval(node)
    if (isMatrix(v)) return v[0]?.[0] ?? ''
    return v
  }

  binary(node: { op: string; l: Node; r: Node }): Value {
    const op = node.op
    const L = this.scalar(node.l), R = this.scalar(node.r)
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
    if (!fn) return ERR.NAME
    try { return fn(this, args) } catch (e) { return isErr(e) ? e : ERR.VALUE }
  }
}

// ── Registre de fonctions ───────────────────────────────────────────────────────
type Fn = (ev: Evaluator, args: Node[]) => Value
const num = (ev: Evaluator, n: Node): number | FErr => { const s = ev.scalar(n); if (isErr(s)) return s; return toNum(s) }
const str = (ev: Evaluator, n: Node): string => toStr(ev.scalar(n))
const matchCriteria = (cell: Scalar, crit: Scalar): boolean => {
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

function compareScalar(a: Scalar, b: Scalar): number {
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
function excelSerial(d: Date): number { return (d.getTime() - EXCEL_EPOCH) / 86400000 }
function serialToDate(n: number | FErr): Date | null { if (isErr(n)) return null; const ms = EXCEL_EPOCH + n * 86400000; const d = new Date(ms); return isNaN(d.getTime()) ? null : d }
function formatText(v: Scalar, fmt: string): string {
  if (typeof v === 'number') {
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
/** Évalue une formule "=..." et renvoie la valeur (ou une FErr). */
export function evaluate(formula: string, data: SheetData, visiting: Set<string> = new Set()): Value {
  const body = formula.startsWith('=') ? formula.slice(1) : formula
  const toks = tokenize(body)
  if (isErr(toks)) return toks
  if (toks.length === 0) return ''
  let ast: Node
  try { const p = new Parser(toks); ast = p.parse(); if (p.peek()) return ERR.VALUE } catch (e) { return isErr(e) ? e : ERR.VALUE }
  const ev = new Evaluator(data, visiting)
  try { return ev.eval(ast) } catch (e) { return isErr(e) ? e : ERR.VALUE }
}

/** Formate une valeur évaluée en chaîne pour l'affichage de la cellule. */
export function formatValue(v: Value): string {
  if (isMatrix(v)) v = v[0]?.[0] ?? ''
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
