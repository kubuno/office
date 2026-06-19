// Tiny zero-dependency math expression compiler used by the function-graph block. It turns a
// textual expression like `sin(x) + x^2` into a `(x) => number` closure. A tolerant recursive
// descent parser builds an AST once; evaluation then runs per sampled x. Supports + - * / ^,
// unary minus, parentheses, implicit multiplication (2x, 2(x+1), x(x+1)), the variable `x`,
// the constants pi/e/tau/phi and a set of common functions. Invalid input yields an error string
// and a closure returning NaN (so the plotter simply draws nothing for that function).

export interface Compiled { fn: (x: number) => number; error: string | null }

const UNARY: Record<string, (a: number) => number> = {
  sin: Math.sin, cos: Math.cos, tan: Math.tan,
  asin: Math.asin, acos: Math.acos, atan: Math.atan,
  sinh: Math.sinh, cosh: Math.cosh, tanh: Math.tanh,
  exp: Math.exp, ln: Math.log, log: Math.log10, log10: Math.log10, log2: Math.log2,
  sqrt: Math.sqrt, cbrt: Math.cbrt, abs: Math.abs,
  floor: Math.floor, ceil: Math.ceil, round: Math.round, sign: Math.sign, trunc: Math.trunc,
}
const BINARY: Record<string, (a: number, b: number) => number> = {
  pow: Math.pow, atan2: Math.atan2, min: Math.min, max: Math.max,
  mod: (a, b) => a - b * Math.floor(a / b),
  log: (b, x) => Math.log(x) / Math.log(b),   // log(base, value) when called with two args
}
const CONSTS: Record<string, number> = { pi: Math.PI, e: Math.E, tau: Math.PI * 2, phi: (1 + Math.sqrt(5)) / 2 }

type Node =
  | { k: 'num'; v: number }
  | { k: 'var' }
  | { k: 'neg'; a: Node }
  | { k: 'bin'; op: string; a: Node; b: Node }
  | { k: 'call'; name: string; args: Node[] }

type Tok = { t: 'num'; v: number } | { t: 'id'; v: string } | { t: 'op'; v: string } | { t: 'p'; v: '(' | ')' | ',' }

function tokenize(src: string): Tok[] {
  // Normalise common unicode operators so pasted formulas still parse.
  const s = src.replace(/[×·∗]/g, '*').replace(/[−–—]/g, '-').replace(/÷/g, '/').replace(/π/g, 'pi')
  const toks: Tok[] = []
  let i = 0
  while (i < s.length) {
    const c = s[i]
    if (c === ' ' || c === '\t' || c === '\n') { i++; continue }
    if (/[0-9.]/.test(c)) {
      let j = i + 1
      while (j < s.length && /[0-9.]/.test(s[j])) j++
      // exponent form 1e-3
      if (s[j] === 'e' && /[0-9+\-]/.test(s[j + 1] ?? '')) { j++; if (s[j] === '+' || s[j] === '-') j++; while (j < s.length && /[0-9]/.test(s[j])) j++ }
      toks.push({ t: 'num', v: parseFloat(s.slice(i, j)) }); i = j; continue
    }
    if (/[a-zA-Z_]/.test(c)) {
      let j = i + 1
      while (j < s.length && /[a-zA-Z0-9_]/.test(s[j])) j++
      toks.push({ t: 'id', v: s.slice(i, j) }); i = j; continue
    }
    if ('+-*/^'.includes(c)) { toks.push({ t: 'op', v: c }); i++; continue }
    if (c === '(' || c === ')' || c === ',') { toks.push({ t: 'p', v: c }); i++; continue }
    throw new Error(`Caractère inattendu « ${c} »`)
  }
  return toks
}

function buildAst(toks: Tok[]): Node {
  let p = 0
  const peek = () => toks[p]
  const isFactorStart = (tk: Tok | undefined) => !!tk && (tk.t === 'num' || tk.t === 'id' || (tk.t === 'p' && tk.v === '('))

  // expr := term (('+'|'-') term)*
  function parseExpr(): Node {
    let left = parseTerm()
    while (peek()?.t === 'op' && (peek() as { v: string }).v === '+' || peek()?.t === 'op' && (peek() as { v: string }).v === '-') {
      const op = (toks[p++] as { v: string }).v
      left = { k: 'bin', op, a: left, b: parseTerm() }
    }
    return left
  }
  // term := factor (('*'|'/'| implicit) factor)*
  function parseTerm(): Node {
    let left = parseFactor()
    for (;;) {
      const tk = peek()
      if (tk?.t === 'op' && (tk.v === '*' || tk.v === '/')) { p++; left = { k: 'bin', op: tk.v, a: left, b: parseFactor() } }
      else if (isFactorStart(tk)) { left = { k: 'bin', op: '*', a: left, b: parseFactor() } }  // implicit multiplication
      else break
    }
    return left
  }
  // factor := base ('^' factor)?   (right associative)
  function parseFactor(): Node {
    const base = parseBase()
    if (peek()?.t === 'op' && (peek() as { v: string }).v === '^') { p++; return { k: 'bin', op: '^', a: base, b: parseFactor() } }
    return base
  }
  function parseBase(): Node {
    const tk = peek()
    if (!tk) throw new Error('Expression incomplète')
    if (tk.t === 'op' && tk.v === '-') { p++; return { k: 'neg', a: parseBase() } }
    if (tk.t === 'op' && tk.v === '+') { p++; return parseBase() }
    if (tk.t === 'num') { p++; return { k: 'num', v: tk.v } }
    if (tk.t === 'p' && tk.v === '(') { p++; const e = parseExpr(); expectClose(); return e }
    if (tk.t === 'id') {
      p++
      const name = tk.v.toLowerCase()
      if (peek()?.t === 'p' && (peek() as { v: string }).v === '(') {            // function call
        p++
        const args: Node[] = []
        if (!(peek()?.t === 'p' && (peek() as { v: string }).v === ')')) {
          args.push(parseExpr())
          while (peek()?.t === 'p' && (peek() as { v: string }).v === ',') { p++; args.push(parseExpr()) }
        }
        expectClose()
        if (!(name in UNARY) && !(name in BINARY)) throw new Error(`Fonction inconnue « ${tk.v} »`)
        return { k: 'call', name, args }
      }
      if (name === 'x') return { k: 'var' }
      if (name in CONSTS) return { k: 'num', v: CONSTS[name] }
      throw new Error(`Symbole inconnu « ${tk.v} »`)
    }
    throw new Error('Expression invalide')
  }
  function expectClose() {
    const tk = toks[p]
    if (!(tk?.t === 'p' && tk.v === ')')) throw new Error('Parenthèse fermante manquante')
    p++
  }

  const ast = parseExpr()
  if (p < toks.length) throw new Error('Texte en trop dans l\'expression')
  return ast
}

function evalNode(n: Node, x: number): number {
  switch (n.k) {
    case 'num': return n.v
    case 'var': return x
    case 'neg': return -evalNode(n.a, x)
    case 'bin': {
      const a = evalNode(n.a, x), b = evalNode(n.b, x)
      switch (n.op) { case '+': return a + b; case '-': return a - b; case '*': return a * b; case '/': return a / b; case '^': return Math.pow(a, b) }
      return NaN
    }
    case 'call': {
      const a = n.args.map(arg => evalNode(arg, x))
      if (n.args.length === 2 && n.name in BINARY) return BINARY[n.name](a[0], a[1])
      if (n.name in UNARY) return UNARY[n.name](a[0])
      if (n.name in BINARY) return BINARY[n.name](a[0], a[1])
      return NaN
    }
  }
}

export function compile(expr: string): Compiled {
  const src = (expr ?? '').trim()
  if (!src) return { fn: () => NaN, error: null }
  try {
    const ast = buildAst(tokenize(src))
    return { fn: (x: number) => { try { return evalNode(ast, x) } catch { return NaN } }, error: null }
  } catch (e) {
    return { fn: () => NaN, error: e instanceof Error ? e.message : 'Expression invalide' }
  }
}
