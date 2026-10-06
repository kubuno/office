// mathCas — a tiny symbolic engine for the Maths module. It differentiates and simplifies
// expressions in the single variable `x`, pretty-prints them back to LaTeX, finds real roots
// numerically and builds a table of values. It reuses the grapher's AST (mathExpr) and the
// LaTeX→text conversion (mathEval), so it covers exactly the closed-form expressions the module
// already understands. Everything is best-effort: unsupported input returns an `error` string.
import { parseExpr, compile, type Node } from './mathExpr'
import { latexToExpr, fmtResult } from './mathEval'

// ── AST constructors ──────────────────────────────────────────────────────────────
const num = (v: number): Node => ({ k: 'num', v })
const neg = (a: Node): Node => ({ k: 'neg', a })
const bin = (op: string, a: Node, b: Node): Node => ({ k: 'bin', op, a, b })
const call = (name: string, args: Node[]): Node => ({ k: 'call', name, args })
const X: Node = { k: 'var' }

// Structural equality (used by simplification rules like a−a=0, a/a=1).
function nodeEq(a: Node, b: Node): boolean {
  if (a.k !== b.k) return false
  switch (a.k) {
    case 'num': return a.v === (b as { v: number }).v
    case 'var': return true
    case 'neg': return nodeEq(a.a, (b as { a: Node }).a)
    case 'bin': { const bb = b as { op: string; a: Node; b: Node }; return a.op === bb.op && nodeEq(a.a, bb.a) && nodeEq(a.b, bb.b) }
    case 'call': { const bb = b as { name: string; args: Node[] }; return a.name === bb.name && a.args.length === bb.args.length && a.args.every((x, i) => nodeEq(x, bb.args[i])) }
  }
}

// ── Symbolic differentiation (d/dx) ─────────────────────────────────────────────────
export function differentiate(n: Node): Node {
  switch (n.k) {
    case 'num': return num(0)
    case 'var': return num(1)
    case 'neg': return neg(differentiate(n.a))
    case 'bin': {
      const { op, a, b } = n
      if (op === '+' || op === '-') return bin(op, differentiate(a), differentiate(b))
      if (op === '*') return bin('+', bin('*', differentiate(a), b), bin('*', a, differentiate(b))) // uv' + u'v
      if (op === '/') return bin('/', bin('-', bin('*', differentiate(a), b), bin('*', a, differentiate(b))), bin('^', b, num(2))) // (u'v−uv')/v²
      if (op === '^') {
        if (b.k === 'num') return bin('*', bin('*', num(b.v), bin('^', a, num(b.v - 1))), differentiate(a)) // n·u^(n−1)·u'
        if (a.k === 'num') return bin('*', bin('*', n, call('ln', [a])), differentiate(b))                  // aᵘ·ln(a)·u'
        return bin('*', n, bin('+', bin('*', differentiate(b), call('ln', [a])), bin('*', b, bin('/', differentiate(a), a)))) // uᵛ·(v'·ln u + v·u'/u)
      }
      throw new Error('Expression non dérivable')
    }
    case 'call': {
      if (n.args.length !== 1) throw new Error(`Dérivée de « ${n.name} » non prise en charge`)
      const u = n.args[0]
      const du = differentiate(u)
      const chain = (d: Node): Node => bin('*', d, du) // f(u)' = f'(u)·u'
      switch (n.name) {
        case 'sin':  return chain(call('cos', [u]))
        case 'cos':  return chain(neg(call('sin', [u])))
        case 'tan':  return chain(bin('/', num(1), bin('^', call('cos', [u]), num(2))))
        case 'exp':  return chain(call('exp', [u]))
        case 'ln':   return chain(bin('/', num(1), u))
        case 'log': case 'log10': return chain(bin('/', num(1), bin('*', u, call('ln', [num(10)]))))
        case 'log2': return chain(bin('/', num(1), bin('*', u, call('ln', [num(2)]))))
        case 'sqrt': return chain(bin('/', num(1), bin('*', num(2), call('sqrt', [u]))))
        case 'asin': return chain(bin('/', num(1), call('sqrt', [bin('-', num(1), bin('^', u, num(2)))])))
        case 'acos': return chain(neg(bin('/', num(1), call('sqrt', [bin('-', num(1), bin('^', u, num(2)))]))))
        case 'atan': return chain(bin('/', num(1), bin('+', num(1), bin('^', u, num(2)))))
        case 'sinh': return chain(call('cosh', [u]))
        case 'cosh': return chain(call('sinh', [u]))
        case 'tanh': return chain(bin('-', num(1), bin('^', call('tanh', [u]), num(2))))
        case 'abs':  return chain(call('sign', [u]))
        default: throw new Error(`Dérivée de « ${n.name} » non prise en charge`)
      }
    }
  }
}

// ── Simplification (constant folding + algebraic identities, run to a fixpoint) ──────
function simpOnce(n: Node): Node {
  switch (n.k) {
    case 'num': case 'var': return n
    case 'neg': { const a = simpOnce(n.a); if (a.k === 'num') return num(-a.v); if (a.k === 'neg') return a.a; return neg(a) }
    case 'call': return call(n.name, n.args.map(simpOnce))
    case 'bin': {
      const a = simpOnce(n.a), b = simpOnce(n.b), op = n.op
      const an = a.k === 'num' ? a.v : null
      const bn = b.k === 'num' ? b.v : null
      if (an != null && bn != null) {
        if (op === '+') return num(an + bn)
        if (op === '-') return num(an - bn)
        if (op === '*') return num(an * bn)
        if (op === '/' && bn !== 0) return num(an / bn)
        if (op === '^') { const p = Math.pow(an, bn); if (Number.isFinite(p)) return num(p) }
      }
      if (op === '+') { if (an === 0) return b; if (bn === 0) return a }
      if (op === '-') { if (bn === 0) return a; if (an === 0) return neg(b); if (nodeEq(a, b)) return num(0) }
      if (op === '*') {
        if (an === 0 || bn === 0) return num(0); if (an === 1) return b; if (bn === 1) return a; if (an === -1) return neg(b); if (bn === -1) return neg(a)
        // Fold a numeric factor into a nested product: c·(d·y) → (c·d)·y  (so 3·(2x) → 6x).
        if (an != null && b.k === 'bin' && b.op === '*') { if (b.a.k === 'num') return bin('*', num(an * b.a.v), b.b); if (b.b.k === 'num') return bin('*', num(an * b.b.v), b.a) }
        if (bn != null && a.k === 'bin' && a.op === '*') { if (a.a.k === 'num') return bin('*', num(bn * a.a.v), a.b); if (a.b.k === 'num') return bin('*', num(bn * a.b.v), a.a) }
      }
      if (op === '/') { if (an === 0) return num(0); if (bn === 1) return a; if (nodeEq(a, b)) return num(1) }
      if (op === '^') { if (bn === 0) return num(1); if (bn === 1) return a; if (an === 1) return num(1); if (an === 0) return num(0) }
      return bin(op, a, b)
    }
  }
}
export function simplify(n: Node): Node {
  let cur = n
  for (let i = 0; i < 25; i++) { const next = simpOnce(cur); if (nodeEq(next, cur)) return next; cur = next }
  return cur
}

// ── Pretty-print AST → LaTeX ─────────────────────────────────────────────────────────
const FN_LATEX: Record<string, string> = {
  sin: '\\sin', cos: '\\cos', tan: '\\tan', asin: '\\arcsin', acos: '\\arccos', atan: '\\arctan',
  sinh: '\\sinh', cosh: '\\cosh', tanh: '\\tanh', ln: '\\ln', log: '\\log', log10: '\\log', log2: '\\log_2',
  sign: '\\operatorname{sign}',
}
function numLatex(v: number): string {
  if (!Number.isFinite(v)) return '\\text{indéfini}'
  // mathEval turns e/pi/tau into their numeric value; render the well-known ones back symbolically.
  if (Math.abs(v - Math.E) < 1e-9) return 'e'
  if (Math.abs(v - Math.PI) < 1e-9) return '\\pi'
  if (Math.abs(v - Math.PI * 2) < 1e-9) return '\\tau'
  if (Number.isInteger(v) && Math.abs(v) < 1e15) return String(v)
  return Number(v.toPrecision(10)).toString()
}
// Fold `e^u` (e came in as a raw number from mathEval) back into the exp() function so the
// derivative stays e^u instead of the ugly 2.718…^u · ln(2.718…).
function normConst(n: Node): Node {
  switch (n.k) {
    case 'neg': return neg(normConst(n.a))
    case 'call': return call(n.name, n.args.map(normConst))
    case 'bin': {
      const a = normConst(n.a), b = normConst(n.b)
      if (n.op === '^' && a.k === 'num' && Math.abs(a.v - Math.E) < 1e-9) return call('exp', [b])
      return bin(n.op, a, b)
    }
    default: return n
  }
}
// Precedence for parenthesisation: +/− = 1, unary/* = 2, ^ = 4, atoms & \frac = 5.
function prec(n: Node): number {
  switch (n.k) {
    case 'num': return n.v < 0 ? 2 : 5
    case 'var': case 'call': return 5
    case 'neg': return 2
    case 'bin':
      if (n.op === '+' || n.op === '-') return 1
      if (n.op === '*') return 2
      if (n.op === '/') return 5 // renders as \frac — self-contained
      return 4 // ^
  }
}
const atomicBase = (n: Node) => (n.k === 'num' && n.v >= 0) || n.k === 'var' || n.k === 'call'
export function nodeToLatex(n: Node): string {
  const wrap = (c: Node, min: number) => { const s = nodeToLatex(c); return prec(c) < min ? `\\left(${s}\\right)` : s }
  switch (n.k) {
    case 'num': return numLatex(n.v)
    case 'var': return 'x'
    case 'neg': return `-${wrap(n.a, 2)}`
    case 'call': {
      const u = n.args[0]
      if (n.name === 'sqrt') return `\\sqrt{${nodeToLatex(u)}}`
      if (n.name === 'cbrt') return `\\sqrt[3]{${nodeToLatex(u)}}`
      if (n.name === 'abs') return `\\left|${nodeToLatex(u)}\\right|`
      if (n.name === 'exp') return `e^{${nodeToLatex(u)}}`
      const args = n.args.map(nodeToLatex).join(',\\ ')
      return `${FN_LATEX[n.name] ?? `\\operatorname{${n.name}}`}\\left(${args}\\right)`
    }
    case 'bin': {
      if (n.op === '/') return `\\frac{${nodeToLatex(n.a)}}{${nodeToLatex(n.b)}}`
      if (n.op === '^') { const base = atomicBase(n.a) ? nodeToLatex(n.a) : `\\left(${nodeToLatex(n.a)}\\right)`; return `${base}^{${nodeToLatex(n.b)}}` }
      if (n.op === '*') {
        const sa = wrap(n.a, 2)
        // Coefficient juxtaposition (2x, 3\sin(x), 2(x−1)); keep a space after a control word so \pi x ≠ \pix.
        if (n.a.k === 'num' && n.a.v >= 0 && n.b.k !== 'num') {
          const rightAtomic = n.b.k === 'var' || n.b.k === 'call' || (n.b.k === 'bin' && n.b.op === '^')
          const sumParen = n.b.k === 'bin' && (n.b.op === '+' || n.b.op === '-')
          if (rightAtomic || sumParen) { const sb = sumParen ? `\\left(${nodeToLatex(n.b)}\\right)` : wrap(n.b, 2); return /[a-zA-Z]$/.test(sa) ? `${sa} ${sb}` : `${sa}${sb}` }
        }
        return `${sa} \\cdot ${wrap(n.b, 2)}`
      }
      return `${wrap(n.a, 1)} ${n.op} ${wrap(n.b, n.op === '-' ? 2 : 1)}`
    }
  }
}

export interface CasResult { latex: string; error: string | null }
const NOT_EXPR = "Ce bloc n'est pas une expression de x exploitable."

// d/dx of a LaTeX formula → LaTeX.
export function derivativeLatex(latex: string): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  try { return { latex: nodeToLatex(simplify(differentiate(normConst(parseExpr(expr))))), error: null } }
  catch (e) { return { latex: '', error: e instanceof Error ? e.message : 'Dérivée impossible' } }
}
// Algebraic simplification of a LaTeX formula → LaTeX.
export function simplifyLatex(latex: string): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  try { return { latex: nodeToLatex(simplify(normConst(parseExpr(expr)))), error: null } }
  catch (e) { return { latex: '', error: e instanceof Error ? e.message : 'Simplification impossible' } }
}

// Real roots of f(x)=0 on [xmin,xmax] by sampling + bisection on sign changes.
export function solveRoots(latex: string, xmin = -10, xmax = 10, steps = 2000): { roots: number[]; error: string | null } {
  const expr = latexToExpr(latex)
  if (!expr) return { roots: [], error: NOT_EXPR }
  const c = compile(expr)
  if (c.error) return { roots: [], error: c.error }
  const f = c.fn
  const roots: number[] = []
  const push = (r: number) => { if (Number.isFinite(r) && !roots.some(x => Math.abs(x - r) < 1e-6)) roots.push(r) }
  const dx = (xmax - xmin) / steps
  let px = xmin, pv = f(xmin)
  if (Math.abs(pv) < 1e-9) push(px)
  for (let i = 1; i <= steps; i++) {
    const x = xmin + i * dx, v = f(x)
    if (Number.isFinite(pv) && Number.isFinite(v) && pv * v < 0) {
      // bisection between px and x
      let lo = px, hi = x, flo = pv
      for (let k = 0; k < 60; k++) { const mid = (lo + hi) / 2, fm = f(mid); if (Math.abs(fm) < 1e-12) { lo = hi = mid; break } if (flo * fm < 0) hi = mid; else { lo = mid; flo = fm } }
      push((lo + hi) / 2)
    } else if (Math.abs(v) < 1e-9) push(x)
    px = x; pv = v
  }
  return { roots: roots.sort((a, b) => a - b).slice(0, 12), error: null }
}

// Table of values → a LaTeX array with an x row and an f(x) row.
export function valueTableLatex(latex: string, xs: number[]): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  const c = compile(expr)
  if (c.error) return { latex: '', error: c.error }
  const cell = (v: number) => (Number.isFinite(v) ? fmtResult(Number(v.toPrecision(6))) : '\\text{—}')
  const cols = 'c|' + 'c'.repeat(xs.length)
  const xrow = xs.map(x => numLatex(x)).join(' & ')
  const yrow = xs.map(x => cell(c.fn(x))).join(' & ')
  return { latex: `\\begin{array}{${cols}} x & ${xrow} \\\\ \\hline f(x) & ${yrow} \\end{array}`, error: null }
}

// ── Rational / polynomial helpers (clean output for expand / integral / Taylor) ──────
function gcd(a: number, b: number): number { a = Math.abs(a); b = Math.abs(b); while (b) { [a, b] = [b, a % b] } return a || 1 }
function factorial(n: number): number { let r = 1; for (let i = 2; i <= n; i++) r *= i; return r }
// Best rational p/q ≈ x with a bounded denominator (our coefficients are integer/​small-factorial).
function toFrac(x: number, maxDen = 5040): [number, number] {
  if (Number.isInteger(x)) return [x, 1]
  for (let d = 2; d <= maxDen; d++) { const p = x * d; if (Math.abs(p - Math.round(p)) < 1e-9) { const g = gcd(Math.round(p), d); return [Math.round(p) / g, d / g] } }
  return [x, 1]
}
const xPow = (k: number) => (k === 0 ? '' : k === 1 ? 'x' : `x^{${k}}`)
// Render coeff·x^pow as a signed term ({neg, body}); coefficient shown as a reduced fraction.
function monomial(coeff: number, pow: number): { neg: boolean; body: string } | null {
  if (Math.abs(coeff) < 1e-12) return null
  const [p, q] = toFrac(Math.abs(coeff))
  const ps = Number.isInteger(p) ? String(p) : String(Number(p.toPrecision(6))) // round irrational coeffs
  const xp = xPow(pow)
  const hideOne = pow > 0 && p === 1
  const numTex = pow === 0 ? ps : `${hideOne ? '' : ps}${xp}`
  return { neg: coeff < 0, body: q === 1 ? numTex : `\\frac{${numTex}}{${q}}` }
}
function joinTerms(parts: ({ neg: boolean; body: string } | null)[]): string {
  const t = parts.filter(Boolean) as { neg: boolean; body: string }[]
  if (!t.length) return '0'
  let s = (t[0].neg ? '-' : '') + t[0].body
  for (let i = 1; i < t.length; i++) s += (t[i].neg ? ' - ' : ' + ') + t[i].body
  return s
}
// Coefficient array of a single-variable polynomial (index = power of x); null if not polynomial.
function polyOf(n: Node): number[] | null {
  switch (n.k) {
    case 'num': return [n.v]
    case 'var': return [0, 1]
    case 'neg': { const p = polyOf(n.a); return p ? p.map(c => -c) : null }
    case 'call': return null
    case 'bin': {
      const a = polyOf(n.a), b = polyOf(n.b)
      if (n.op === '+' || n.op === '-') { if (!a || !b) return null; const len = Math.max(a.length, b.length), r: number[] = []; for (let i = 0; i < len; i++) r[i] = (a[i] || 0) + (n.op === '-' ? -1 : 1) * (b[i] || 0); return r }
      if (n.op === '*') { if (!a || !b) return null; const r = new Array(a.length + b.length - 1).fill(0); for (let i = 0; i < a.length; i++) for (let j = 0; j < b.length; j++) r[i + j] += a[i] * b[j]; return r }
      if (n.op === '/') { if (!a || !b) return null; if (b.length === 1 && Math.abs(b[0]) > 1e-12) return a.map(c => c / b[0]); return null }
      if (n.op === '^') { if (!a || n.b.k !== 'num' || !Number.isInteger(n.b.v) || n.b.v < 0 || n.b.v > 10) return null; let r = [1]; for (let k = 0; k < n.b.v; k++) { const nr = new Array(r.length + a.length - 1).fill(0); for (let i = 0; i < r.length; i++) for (let j = 0; j < a.length; j++) nr[i + j] += r[i] * a[j]; r = nr } return r }
      return null
    }
  }
}
function trimPoly(p: number[]): number[] { let e = p.length - 1; while (e > 0 && Math.abs(p[e]) < 1e-12) e--; return p.slice(0, e + 1) }
const polyToLatex = (p: number[]) => joinTerms(trimPoly(p).map((c, i) => monomial(c, i)).reverse())

// ── Symbolic integration (∫ … dx) ───────────────────────────────────────────────────
class NotSupported extends Error {}
// Linear argument k·x → k (for ∫f(kx)dx = F(kx)/k); null otherwise.
function linCoef(u: Node): number | null {
  if (u.k === 'var') return 1
  if (u.k === 'bin' && u.op === '*') { if (u.a.k === 'num' && u.b.k === 'var') return u.a.v; if (u.b.k === 'num' && u.a.k === 'var') return u.b.v }
  return null
}
export function integrate(n: Node): Node {
  switch (n.k) {
    case 'num': return bin('*', num(n.v), X)                                   // ∫c dx = c·x
    case 'var': return bin('/', bin('^', X, num(2)), num(2))                   // ∫x dx = x²/2
    case 'neg': return neg(integrate(n.a))
    case 'bin': {
      const { op, a, b } = n
      if (op === '+' || op === '-') return bin(op, integrate(a), integrate(b)) // linearity
      if (op === '*') { if (a.k === 'num') return bin('*', a, integrate(b)); if (b.k === 'num') return bin('*', b, integrate(a)); throw new NotSupported('Intégrale de ce produit non prise en charge') }
      if (op === '/') { if (b.k === 'num') return bin('/', integrate(a), b); if (a.k === 'num' && b.k === 'var') return bin('*', a, call('ln', [call('abs', [X])])); throw new NotSupported('Intégrale de ce quotient non prise en charge') }
      if (op === '^') { if (a.k === 'var' && b.k === 'num') { if (b.v === -1) return call('ln', [call('abs', [X])]); return bin('/', bin('^', X, num(b.v + 1)), num(b.v + 1)) } throw new NotSupported('Intégrale de cette puissance non prise en charge') }
      throw new NotSupported('Intégrale non prise en charge')
    }
    case 'call': {
      const u = n.args[0]; const k = u ? linCoef(u) : null
      const over = (node: Node) => (k === 1 ? node : bin('/', node, num(k as number)))
      if (k != null) {
        if (n.name === 'exp') return over(call('exp', [u]))
        if (n.name === 'sin') return over(neg(call('cos', [u])))
        if (n.name === 'cos') return over(call('sin', [u]))
      }
      if (n.name === 'sqrt' && u.k === 'var') return bin('*', num(2 / 3), bin('^', X, num(1.5))) // ∫√x = (2/3)x^{3/2}
      throw new NotSupported(`Intégrale de « ${n.name} » non prise en charge`)
    }
  }
}
// ∫ f dx (+ C). Polynomials are integrated term-by-term for clean fractional output.
export function integralLatex(latex: string): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  try {
    const ast = normConst(parseExpr(expr))
    const p = polyOf(ast)
    if (p) { const terms = trimPoly(p).map((c, i) => monomial(c / (i + 1), i + 1)); return { latex: `${joinTerms(terms.reverse())} + C`, error: null } }
    return { latex: `${nodeToLatex(simplify(integrate(ast)))} + C`, error: null }
  } catch (e) { return { latex: '', error: e instanceof Error ? e.message : 'Intégrale impossible' } }
}
// Numeric definite integral ∫ₐᵇ f dx (composite Simpson) — the "area under the curve".
export function definiteIntegral(latex: string, a: number, b: number): { value: number | null; error: string | null } {
  const expr = latexToExpr(latex)
  if (!expr) return { value: null, error: NOT_EXPR }
  const c = compile(expr)
  if (c.error) return { value: null, error: c.error }
  const n = 1000, h = (b - a) / n
  let s = c.fn(a) + c.fn(b)
  for (let i = 1; i < n; i++) s += (i % 2 ? 4 : 2) * c.fn(a + i * h)
  const v = (s * h) / 3
  return Number.isFinite(v) ? { value: v, error: null } : { value: null, error: 'Intégrale divergente sur cet intervalle.' }
}

// ── Expand / factor (polynomials) ────────────────────────────────────────────────────
export function expandLatex(latex: string): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  try { const p = polyOf(normConst(parseExpr(expr))); if (!p) return { latex: '', error: 'Développement limité aux expressions polynomiales.' }; return { latex: polyToLatex(p), error: null } }
  catch (e) { return { latex: '', error: e instanceof Error ? e.message : 'Développement impossible' } }
}
// Factor a polynomial of degree ≤ 2 over the reals: a(x−r₁)(x−r₂), a(x−r)², or a(x−r).
export function factorLatex(latex: string): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  let p: number[] | null
  try { p = polyOf(normConst(parseExpr(expr))) } catch { p = null }
  if (!p) return { latex: '', error: 'Factorisation limitée aux polynômes.' }
  const c = trimPoly(p)
  const lead = (a: number) => (Math.abs(a - 1) < 1e-9 ? '' : numLatex(a))
  const linFactor = (r: number) => { const rr = toFrac(r); const rl = rr[1] === 1 ? String(rr[0]) : `\\frac{${rr[0]}}{${rr[1]}}`; return Math.abs(r) < 1e-12 ? 'x' : `\\left(x ${r < 0 ? '+' : '-'} ${rl.replace('-', '')}\\right)` }
  if (c.length <= 1) return { latex: numLatex(c[0] ?? 0), error: null }
  if (c.length === 2) { const [b, a] = c; return { latex: `${lead(a)}${linFactor(-b / a)}`, error: null } }
  if (c.length === 3) {
    const [cc, b, a] = c, disc = b * b - 4 * a * cc
    if (disc < -1e-9) return { latex: '', error: 'Pas de factorisation réelle (discriminant < 0).' }
    const s = Math.sqrt(Math.max(0, disc)), r1 = (-b - s) / (2 * a), r2 = (-b + s) / (2 * a)
    const body = Math.abs(r1 - r2) < 1e-9 ? `${linFactor(r1)}^{2}` : `${linFactor(r1)}${linFactor(r2)}`
    return { latex: `${lead(a)}${body}`, error: null }
  }
  return { latex: '', error: 'Factorisation limitée au degré 2.' }
}

// ── Taylor / Maclaurin series around 0 ───────────────────────────────────────────────
function evalAt(n: Node, x: number): number {
  switch (n.k) {
    case 'num': return n.v
    case 'var': return x
    case 'neg': return -evalAt(n.a, x)
    case 'bin': { const a = evalAt(n.a, x), b = evalAt(n.b, x); switch (n.op) { case '+': return a + b; case '-': return a - b; case '*': return a * b; case '/': return a / b; case '^': return Math.pow(a, b) } return NaN }
    case 'call': {
      const a = evalAt(n.args[0], x)
      switch (n.name) {
        case 'sin': return Math.sin(a); case 'cos': return Math.cos(a); case 'tan': return Math.tan(a)
        case 'exp': return Math.exp(a); case 'ln': return Math.log(a); case 'log': case 'log10': return Math.log10(a); case 'log2': return Math.log2(a)
        case 'sqrt': return Math.sqrt(a); case 'abs': return Math.abs(a); case 'sign': return Math.sign(a)
        case 'sinh': return Math.sinh(a); case 'cosh': return Math.cosh(a); case 'tanh': return Math.tanh(a)
        case 'asin': return Math.asin(a); case 'acos': return Math.acos(a); case 'atan': return Math.atan(a)
      }
      return NaN
    }
  }
}
// coeff·(base)^pow term, base being a LaTeX string (e.g. "\left(x - 2\right)").
function powTermBase(coeff: number, pow: number, base: string): { neg: boolean; body: string } | null {
  if (Math.abs(coeff) < 1e-12) return null
  const [p, q] = toFrac(Math.abs(coeff))
  const ps = Number.isInteger(p) ? String(p) : String(Number(p.toPrecision(6)))
  const xp = pow === 0 ? '' : pow === 1 ? base : `${base}^{${pow}}`
  const numTex = pow === 0 ? ps : `${pow > 0 && p === 1 ? '' : ps}${xp}`
  return { neg: coeff < 0, body: q === 1 ? numTex : `\\frac{${numTex}}{${q}}` }
}
// Taylor polynomial up to `order` around x = `center` (Maclaurin when center = 0).
export function taylorLatex(latex: string, order = 5, center = 0): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  const base = center === 0 ? '' : center > 0 ? `\\left(x - ${ratLatex(center)}\\right)` : `\\left(x + ${ratLatex(-center)}\\right)`
  try {
    let cur = normConst(parseExpr(expr))
    const terms: ({ neg: boolean; body: string } | null)[] = []
    for (let k = 0; k <= order; k++) {
      const va = evalAt(cur, center)
      if (!Number.isFinite(va)) return { latex: '', error: `Développement impossible (singularité en x = ${center}).` }
      const rounded = Math.abs(va - Math.round(va)) < 1e-9 ? Math.round(va) : va
      terms.push(center === 0 ? monomial(rounded / factorial(k), k) : powTermBase(rounded / factorial(k), k, base))
      cur = simplify(differentiate(cur))
    }
    const rem = center === 0 ? `x^{${order + 1}}` : `${base}^{${order + 1}}`
    return { latex: `${joinTerms(terms)} + O\\left(${rem}\\right)`, error: null }
  } catch (e) { return { latex: '', error: e instanceof Error ? e.message : 'Développement de Taylor impossible' } }
}

// ── Function study (analyse de fonction) ─────────────────────────────────────────────
// Round a value to a clean number for display (collapses float noise, keeps e/π via numLatex).
const clean = (v: number) => (Math.abs(v - Math.round(v)) < 1e-9 ? Math.round(v) : Number(v.toPrecision(6)))

// n-th derivative → LaTeX (n = 2 for the second derivative button).
export function nthDerivativeLatex(latex: string, n: number): CasResult {
  const expr = latexToExpr(latex)
  if (!expr) return { latex: '', error: NOT_EXPR }
  try {
    let cur = normConst(parseExpr(expr))
    for (let i = 0; i < n; i++) cur = simplify(differentiate(cur))
    return { latex: nodeToLatex(cur), error: null }
  } catch (e) { return { latex: '', error: e instanceof Error ? e.message : 'Dérivation impossible' } }
}

// Tangent line at x = a: y = f(a) + f'(a)(x − a). Returns the equation LaTeX plus a plain-text
// expression (for plotting the tangent alongside the curve).
export function tangentAt(latex: string, a: number): { latex: string; expr: string; error: string | null } {
  const e = latexToExpr(latex)
  if (!e) return { latex: '', expr: '', error: NOT_EXPR }
  try {
    const ast = normConst(parseExpr(e))
    const fa = evalAt(ast, a), fpa = evalAt(simplify(differentiate(ast)), a)
    if (!Number.isFinite(fa) || !Number.isFinite(fpa)) return { latex: '', expr: '', error: 'Tangente indéfinie en ce point.' }
    const node = simplify(bin('+', bin('*', num(fpa), bin('-', X, num(a))), num(fa)))
    return { latex: `y = ${nodeToLatex(node)}`, expr: `(${fpa})*(x-(${a}))+(${fa})`, error: null }
  } catch (err) { return { latex: '', expr: '', error: err instanceof Error ? err.message : 'Tangente impossible' } }
}

// Critical points on [-10,10]: roots of f', classified via the sign of f'' (min / max / inflection).
export function extremaLatex(latex: string): CasResult {
  const e = latexToExpr(latex)
  if (!e) return { latex: '', error: NOT_EXPR }
  try {
    const ast = normConst(parseExpr(e))
    const fp = simplify(differentiate(ast))
    if (fp.k === 'num') return { latex: '', error: 'Pas d\'extremum (dérivée constante).' }
    const { roots, error } = solveRoots(nodeToLatex(fp))
    if (error) return { latex: '', error }
    if (!roots.length) return { latex: '', error: 'Aucun point critique sur [-10, 10].' }
    const fpp = simplify(differentiate(fp))
    const xr = roots.map(r => numLatex(clean(r))).join(' & ')
    const yr = roots.map(r => { const v = evalAt(ast, r); return Number.isFinite(v) ? numLatex(clean(v)) : '\\text{—}' }).join(' & ')
    const nr = roots.map(r => { const s = evalAt(fpp, r); return s > 1e-7 ? '\\text{min}' : s < -1e-7 ? '\\text{max}' : '\\text{infl.}' }).join(' & ')
    const cols = 'c|' + 'c'.repeat(roots.length)
    return { latex: `\\begin{array}{${cols}} x & ${xr} \\\\ \\hline f(x) & ${yr} \\\\ \\hline \\text{nature} & ${nr} \\end{array}`, error: null }
  } catch (err) { return { latex: '', error: err instanceof Error ? err.message : 'Étude impossible' } }
}

// Numeric limit of f at a finite point or ±∞. `at` may be ±Infinity.
export function limitLatex(latex: string, at: number): CasResult {
  const e = latexToExpr(latex)
  if (!e) return { latex: '', error: NOT_EXPR }
  const c = compile(e)
  if (c.error) return { latex: '', error: c.error }
  const f = c.fn
  const atL = !Number.isFinite(at) ? (at > 0 ? '+\\infty' : '-\\infty') : numLatex(clean(at))
  const wrap = (L: string) => ({ latex: `\\lim_{x \\to ${atL}} f(x) = ${L}`, error: null })
  if (!Number.isFinite(at)) {
    const s = at > 0 ? 1 : -1
    // Large, well-separated samples so slow (1/x-rate) convergence is detected, not misread as divergence.
    const v = [1e5, 1e8, 1e11].map(x => f(s * x))
    if (v.every(Number.isFinite) && Math.abs(v[2] - v[1]) < 1e-5 * (1 + Math.abs(v[2]))) return wrap(numLatex(clean(v[2])))
    if (Math.abs(v[2]) > 1e10) return wrap(v[2] > 0 ? '+\\infty' : '-\\infty')
    return { latex: '', error: "La limite en l'infini n'existe pas (divergence)." }
  }
  const l = [1e-4, 1e-6, 1e-8].map(h => f(at - h))
  const r = [1e-4, 1e-6, 1e-8].map(h => f(at + h))
  const lv = l[2], rv = r[2]
  if (Number.isFinite(lv) && Number.isFinite(rv) && Math.abs(lv - rv) < 1e-4 * (1 + Math.abs(lv))) return wrap(numLatex(clean((lv + rv) / 2)))
  if ((!Number.isFinite(lv) || Math.abs(lv) > 1e8) && (!Number.isFinite(rv) || Math.abs(rv) > 1e8)) return { latex: '', error: 'Limite infinie (asymptote verticale en ce point).' }
  return { latex: '', error: "La limite n'existe pas (valeurs différentes à gauche et à droite)." }
}

// ── Pro extensions: exact solving, inflection, asymptotes, series, sum/product, eval ──
// Signed value as an integer or a reduced fraction (small denominator).
function ratLatex(v: number): string {
  if (Math.abs(v - Math.round(v)) < 1e-9) return String(Math.round(v))
  const [p, q] = toFrac(v)
  return q === 1 ? String(p) : p < 0 ? `-\\frac{${-p}}{${q}}` : `\\frac{${p}}{${q}}`
}
// √n = coef·√rad with the largest square factor pulled out.
function simplifyRadical(n: number): { coef: number; rad: number } {
  n = Math.round(n); let coef = 1, rad = n
  for (let f = 2; f * f <= rad; f++) while (rad % (f * f) === 0) { rad /= f * f; coef *= f }
  return { coef, rad }
}
const radTerm = (coef: number, rad: number) => (rad === 1 ? String(coef) : coef === 1 ? `\\sqrt{${rad}}` : `${coef}\\sqrt{${rad}}`)

// Exact roots of a polynomial of degree ≤ 2 (rational, radical or complex form).
export function solveExactLatex(latex: string): CasResult {
  const e = latexToExpr(latex); if (!e) return { latex: '', error: NOT_EXPR }
  let p: number[] | null
  try { p = polyOf(normConst(parseExpr(e))) } catch { p = null }
  if (!p) return { latex: '', error: 'Résolution exacte limitée aux polynômes (degré ≤ 2).' }
  const c = trimPoly(p)
  if (c.length <= 1) return { latex: '', error: Math.abs(c[0] ?? 0) > 1e-9 ? 'Aucune solution.' : 'Tout réel est solution.' }
  if (c.length === 2) { const [b, a] = c; return { latex: `x = ${ratLatex(-b / a)}`, error: null } }
  if (c.length === 3) {
    const [cc, b, a] = c, D = b * b - 4 * a * cc
    if (Math.abs(D) < 1e-9) return { latex: `x = ${ratLatex(-b / (2 * a))} \\quad (\\text{racine double})`, error: null }
    // Format (nb ± coef√rad)/den, reducing the fraction when all terms are integers.
    const pm = (nb: number, coef: number, rad: number, den: number, imaginary: boolean) => {
      const allInt = [nb, coef, den].every(Number.isInteger)
      const g = allInt ? gcd(gcd(Math.abs(nb), coef), Math.abs(den)) || 1 : 1
      const NB = nb / g, CO = coef / g, DEN = den / g
      let rt = radTerm(CO, rad)
      if (imaginary) rt = rt === '1' ? 'i' : `${rt}\\,i`
      const numer = Math.abs(NB) < 1e-9 ? `\\pm ${rt}` : `${ratLatex(NB)} \\pm ${rt}`
      return DEN === 1 ? `x = ${numer}` : `x = \\frac{${numer}}{${DEN}}`
    }
    if (D > 0) {
      const { coef, rad } = simplifyRadical(D)
      if (rad === 1) return { latex: `x_1 = ${ratLatex((-b - coef) / (2 * a))}, \\quad x_2 = ${ratLatex((-b + coef) / (2 * a))}`, error: null }
      return { latex: pm(-b, coef, rad, 2 * a, false), error: null }
    }
    const { coef, rad } = simplifyRadical(-D)
    return { latex: pm(-b, coef, rad, 2 * a, true), error: null }
  }
  return { latex: '', error: 'Degré > 2 : utilisez « Résoudre » (numérique).' }
}

// Inflection points: roots of f'' on [-10,10], with the value of f there.
export function inflectionLatex(latex: string): CasResult {
  const e = latexToExpr(latex); if (!e) return { latex: '', error: NOT_EXPR }
  try {
    const ast = normConst(parseExpr(e))
    const fpp = simplify(differentiate(simplify(differentiate(ast))))
    if (fpp.k === 'num') return { latex: '', error: "Pas de point d'inflexion (f'' constante)." }
    const { roots, error } = solveRoots(nodeToLatex(fpp))
    if (error) return { latex: '', error }
    if (!roots.length) return { latex: '', error: "Aucun point d'inflexion sur [-10, 10]." }
    const xr = roots.map(r => numLatex(clean(r))).join(' & ')
    const yr = roots.map(r => { const v = evalAt(ast, r); return Number.isFinite(v) ? numLatex(clean(v)) : '\\text{—}' }).join(' & ')
    return { latex: `\\begin{array}{c|${'c'.repeat(roots.length)}} x & ${xr} \\\\ \\hline f(x) & ${yr} \\end{array}`, error: null }
  } catch (err) { return { latex: '', error: err instanceof Error ? err.message : 'Étude impossible' } }
}

// Horizontal (limits at ±∞) and vertical (blow-ups on [-20,20]) asymptotes.
export function asymptotesLatex(latex: string): CasResult {
  const e = latexToExpr(latex); if (!e) return { latex: '', error: NOT_EXPR }
  const c = compile(e); if (c.error) return { latex: '', error: c.error }
  const f = c.fn, lines: string[] = []
  const snap0 = (v: number) => (Math.abs(v) < 1e-6 ? 0 : clean(v))
  const lp = f(1e9), lp2 = f(1e6)
  if (Number.isFinite(lp) && Math.abs(lp - lp2) < 1e-4 * (1 + Math.abs(lp))) lines.push(`y = ${numLatex(snap0(lp))} \\quad (x \\to +\\infty)`)
  const lm = f(-1e9), lm2 = f(-1e6)
  if (Number.isFinite(lm) && Math.abs(lm - lm2) < 1e-4 * (1 + Math.abs(lm)) && !(Number.isFinite(lp) && Math.abs(lp - lm) < 1e-6)) lines.push(`y = ${numLatex(snap0(lm))} \\quad (x \\to -\\infty)`)
  const va: number[] = []
  for (let x = -20; x <= 20; x += 0.02) { const v = f(x); if (Number.isFinite(v) && Math.abs(v) > 1e5) { const xr = Math.round(x * 50) / 50; if (!va.some(a => Math.abs(a - xr) < 0.5)) va.push(xr) } }
  va.forEach(a => lines.push(`x = ${numLatex(clean(a))} \\quad (\\text{asymptote verticale})`))
  if (!lines.length) return { latex: '', error: 'Aucune asymptote détectée.' }
  return { latex: lines.length > 1 ? `\\begin{gathered} ${lines.join(' \\\\ ')} \\end{gathered}` : lines[0], error: null }
}

// Finite sum Σ and product Π over integer x from a to b of the current formula.
function accumulate(latex: string, a: number, b: number, kind: 'sum' | 'prod'): CasResult {
  const e = latexToExpr(latex); if (!e) return { latex: '', error: NOT_EXPR }
  const c = compile(e); if (c.error) return { latex: '', error: c.error }
  a = Math.round(a); b = Math.round(b)
  if (b < a || b - a > 1e6) return { latex: '', error: 'Bornes invalides (a ≤ b, ≤ 10⁶ termes).' }
  let acc = kind === 'sum' ? 0 : 1
  for (let i = a; i <= b; i++) { const v = c.fn(i); if (!Number.isFinite(v)) return { latex: '', error: 'Terme non défini dans l’intervalle.' }; acc = kind === 'sum' ? acc + v : acc * v }
  const op = kind === 'sum' ? '\\sum' : '\\prod'
  return { latex: `${op}_{x=${a}}^{${b}} ${latex.trim()} = ${numLatex(clean(acc))}`, error: null }
}
export const sumLatex = (latex: string, a: number, b: number) => accumulate(latex, a, b, 'sum')
export const productLatex = (latex: string, a: number, b: number) => accumulate(latex, a, b, 'prod')

// Evaluate the current function at a given x = a.
export function evalPointLatex(latex: string, a: number): CasResult {
  const e = latexToExpr(latex); if (!e) return { latex: '', error: NOT_EXPR }
  const c = compile(e); if (c.error) return { latex: '', error: c.error }
  const v = c.fn(a)
  if (!Number.isFinite(v)) return { latex: '', error: 'Fonction non définie en ce point.' }
  return { latex: `f\\left(${ratLatex(a)}\\right) = ${numLatex(clean(v))}`, error: null }
}
