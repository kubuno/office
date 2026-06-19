// ── Math expression tree (a real WYSIWYG model, MathQuill/Mathcha-style) ─────────
// A document is a row (list) of nodes; structures hold sub-rows. The tree is the editable
// model; it serialises to LaTeX (storage / preview / code mode) and parses back from the
// subset we emit + common input (unknown commands degrade to literal `atom`s, never crash).
// The editor mutates the tree in place (held in a ref) and rerenders via a tick counter.

export type MRow = MNode[]
export type MNode =
  | { k: 'atom'; t: string }                                   // literal token: x 1 + \alpha \pi …
  | { k: 'frac'; n: MRow; d: MRow }
  | { k: 'sqrt'; b: MRow }
  | { k: 'root'; i: MRow; b: MRow }
  | { k: 'script'; base: MNode; sup: MRow | null; sub: MRow | null }
  | { k: 'delim'; o: string; c: string; b: MRow }
  | { k: 'big'; op: string; lo: MRow | null; hi: MRow | null }
  | { k: 'acc'; cmd: string; b: MRow }
  | { k: 'mat'; env: string; rows: MRow[][] }
  | { k: 'raw'; t: string }

export interface Cursor { row: MRow; idx: number; anchor: number | null } // anchor = selection start in `row`

// Two-keystroke auto-conversions (the 2nd char replaces the preceding single-char atom) — makes
// typing maths feel like typing text: `->`→→, `<=`→≤, etc.
export const COMBO: Record<string, string> = {
  '->': '\\to', '=>': '\\Rightarrow', '<-': '\\leftarrow', '<=': '\\leq', '>=': '\\geq', '!=': '\\neq',
  '~=': '\\approx', '+-': '\\pm', '-+': '\\mp', '<<': '\\ll', '>>': '\\gg', '==': '\\equiv', '|-': '\\vdash',
  '.+': '\\cdot', '**': '\\times', '*.': '\\ast', ':=': '\\coloneqq', '~~': '\\approx', '<>': '\\neq',
}

const ACCENTS = new Set(['\\hat', '\\vec', '\\bar', '\\tilde', '\\overline', '\\underline', '\\widehat', '\\widetilde', '\\dot', '\\ddot', '\\dddot', '\\check', '\\breve', '\\acute', '\\grave', '\\mathring', '\\overrightarrow', '\\overleftarrow'])
const BIGOPS = new Set(['\\sum', '\\prod', '\\int', '\\oint', '\\iint', '\\iiint', '\\bigcup', '\\bigcap', '\\bigsqcup', '\\bigvee', '\\bigwedge', '\\bigodot', '\\bigotimes', '\\bigoplus', '\\lim', '\\limsup', '\\liminf', '\\coprod'])
const MAT_ENVS = new Set(['matrix', 'pmatrix', 'bmatrix', 'Bmatrix', 'vmatrix', 'Vmatrix', 'smallmatrix', 'cases', 'aligned', 'gathered', 'array'])

// ── Serialise tree → LaTeX ────────────────────────────────────────────────────────
export function ser(row: MRow): string { return row.map(serNode).join('') }
function serNode(n: MNode): string {
  switch (n.k) {
    case 'atom': return n.t + (n.t.length > 1 && /^\\[a-zA-Z]+$/.test(n.t) ? ' ' : '')
    case 'raw': return n.t
    case 'frac': return `\\frac{${ser(n.n)}}{${ser(n.d)}}`
    case 'sqrt': return `\\sqrt{${ser(n.b)}}`
    case 'root': return `\\sqrt[${ser(n.i)}]{${ser(n.b)}}`
    case 'script': { let s = serNode(n.base); if (n.sub) s += `_{${ser(n.sub)}}`; if (n.sup) s += `^{${ser(n.sup)}}`; return s }
    case 'delim': return `\\left${n.o} ${ser(n.b)} \\right${n.c}`
    case 'big': { let s = n.op + ' '; if (n.lo) s += `_{${ser(n.lo)}}`; if (n.hi) s += `^{${ser(n.hi)}}`; return s }
    case 'acc': return `${n.cmd}{${ser(n.b)}}`
    case 'mat': return `\\begin{${n.env}}${n.rows.map(r => r.map(ser).join(' & ')).join(' \\\\ ')}\\end{${n.env}}`
  }
}

// ── Tokenise + parse LaTeX → tree (tolerant) ─────────────────────────────────────
function tokenize(src: string): string[] {
  const toks: string[] = []
  let i = 0
  while (i < src.length) {
    const c = src[i]
    if (c === '\\') {
      const m = /^\\([a-zA-Z]+|.)/.exec(src.slice(i))
      if (m) { toks.push('\\' + m[1]); i += m[0].length; continue }
    }
    if (c === ' ' || c === '\n' || c === '\t') { i++; continue }
    toks.push(c); i++
  }
  return toks
}

export function parse(src: string): MRow {
  const toks = tokenize((src ?? '').trim())
  let p = 0
  const peek = () => toks[p]
  const eat = () => toks[p++]

  function parseGroup(): MRow {
    if (peek() === '{') { eat(); const r = parseRow(['}']); if (peek() === '}') eat(); return r }
    // single token group
    const tk = peek()
    if (tk == null) return []
    return [parseAtomOrStruct()]
  }
  function parseOptional(): MRow | null {
    if (peek() !== '[') return null
    eat(); const r = parseRow([']']); if (peek() === ']') eat(); return r
  }
  function attachScripts(base: MNode): MNode {
    let sup: MRow | null = null, sub: MRow | null = null
    while (peek() === '^' || peek() === '_') {
      const op = eat()
      const g = parseGroup()
      if (op === '^') sup = g; else sub = g
    }
    return (sup || sub) ? { k: 'script', base, sup, sub } : base
  }
  function parseAtomOrStruct(): MNode {
    const tk = eat()!
    if (tk === '\\frac' || tk === '\\dfrac' || tk === '\\tfrac') { const n = parseGroup(); const d = parseGroup(); return { k: 'frac', n, d } }
    if (tk === '\\sqrt') { const idx = parseOptional(); const b = parseGroup(); return idx ? { k: 'root', i: idx, b } : { k: 'sqrt', b } }
    if (tk === '\\left') { const o = eat() ?? '('; const b = parseRow(['\\right']); if (peek() === '\\right') eat(); const c = eat() ?? ')'; return { k: 'delim', o: o === '.' ? '' : o, c: c === '.' ? '' : c, b } }
    if (tk === '\\begin') { let env = ''; if (peek() === '{') { eat(); while (peek() && peek() !== '}') env += eat(); if (peek() === '}') eat() } return parseMatrix(env) }
    if (ACCENTS.has(tk)) { const b = parseGroup(); return { k: 'acc', cmd: tk, b } }
    if (BIGOPS.has(tk)) { let lo: MRow | null = null, hi: MRow | null = null; while (peek() === '^' || peek() === '_') { const op = eat(); const g = parseGroup(); if (op === '^') hi = g; else lo = g } return { k: 'big', op: tk, lo, hi } }
    // a brace group used standalone → flatten its contents (keep as raw-ish row inside an atom group)
    if (tk === '{') { const r = parseRow(['}']); if (peek() === '}') eat(); return r.length === 1 ? r[0] : { k: 'delim', o: '', c: '', b: r } }
    return { k: 'atom', t: tk }
  }
  function parseMatrix(env: string): MNode {
    const rows: MRow[][] = [[]]
    let cur: MRow = []
    const push = () => { rows[rows.length - 1].push(cur); cur = [] }
    while (peek() != null && peek() !== '\\end') {
      if (peek() === '&') { eat(); push(); continue }
      if (peek() === '\\\\') { eat(); push(); rows.push([]); continue }
      cur.push(parseScripted())
    }
    push()
    if (peek() === '\\end') { eat(); if (peek() === '{') { eat(); while (peek() && peek() !== '}') eat(); if (peek() === '}') eat() } }
    return { k: 'mat', env: MAT_ENVS.has(env) ? env : 'matrix', rows }
  }
  function parseScripted(): MNode { return attachScripts(parseAtomOrStruct()) }
  function parseRow(stop: string[]): MRow {
    const row: MRow = []
    while (peek() != null && !stop.includes(peek()!)) {
      if (peek() === '&' || peek() === '\\\\') break
      row.push(parseScripted())
    }
    return row
  }
  return parseRow([])
}

// ── Caret slots (in-order linear positions) for Left/Right + click ───────────────
export interface Slot { row: MRow; idx: number }
export function slots(root: MRow): Slot[] { const out: Slot[] = []; rowSlots(root, out); return out }
function rowSlots(row: MRow, out: Slot[]) {
  for (let i = 0; i < row.length; i++) { out.push({ row, idx: i }); nodeSlots(row[i], out) }
  out.push({ row, idx: row.length })
}
function nodeSlots(n: MNode, out: Slot[]) {
  switch (n.k) {
    case 'frac': rowSlots(n.n, out); rowSlots(n.d, out); break
    case 'sqrt': rowSlots(n.b, out); break
    case 'root': rowSlots(n.i, out); rowSlots(n.b, out); break
    case 'script': nodeSlots(n.base, out); if (n.sub) rowSlots(n.sub, out); if (n.sup) rowSlots(n.sup, out); break
    case 'delim': rowSlots(n.b, out); break
    case 'big': if (n.lo) rowSlots(n.lo, out); if (n.hi) rowSlots(n.hi, out); break
    case 'acc': rowSlots(n.b, out); break
    case 'mat': n.rows.forEach(r => r.forEach(c => rowSlots(c, out))); break
  }
}

// ── Render tree → KaTeX (with caret + clickable atoms + selection highlight) ─────
const CARET = '\\htmlClass{mc-caret}{\\rule{0pt}{1.05em}}'
export function renderTree(root: MRow, cur: Cursor): { latex: string; map: Slot[] } {
  const map: Slot[] = []
  const reg = (s: Slot) => { map.push(s); return map.length - 1 }
  const caretAt = (row: MRow, idx: number) => row === cur.row && idx === cur.idx && cur.anchor == null
  const sel = (row: MRow): [number, number] | null => (row === cur.row && cur.anchor != null && cur.anchor !== cur.idx) ? [Math.min(cur.anchor, cur.idx), Math.max(cur.anchor, cur.idx)] : null
  function rRow(row: MRow): string {
    if (row.length === 0) {
      const n = reg({ row, idx: 0 })
      return `\\htmlClass{mc-h mc-h-${n} mc-empty}{${caretAt(row, 0) ? CARET : ''}\\square}`
    }
    const sr = sel(row)
    let out = ''
    for (let i = 0; i < row.length; i++) {
      if (caretAt(row, i)) out += CARET
      const n = reg({ row, idx: i + 1 })
      const inSel = sr && i >= sr[0] && i < sr[1]
      const body = `\\htmlClass{mc-h mc-h-${n}}{${rNode(row[i])}}`
      out += inSel ? `\\htmlClass{mc-sel}{${body}}` : body
    }
    if (caretAt(row, row.length)) out += CARET
    return out
  }
  function rNode(n: MNode): string {
    switch (n.k) {
      case 'atom': return n.t === '' ? '\\square' : n.t + (n.t.length > 1 && /^\\[a-zA-Z]+$/.test(n.t) ? ' ' : '')
      case 'raw': return n.t
      case 'frac': return `\\frac{${rRow(n.n)}}{${rRow(n.d)}}`
      case 'sqrt': return `\\sqrt{${rRow(n.b)}}`
      case 'root': return `\\sqrt[${rRow(n.i)}]{${rRow(n.b)}}`
      case 'script': { let s = rNode(n.base); if (n.sub) s += `_{${rRow(n.sub)}}`; if (n.sup) s += `^{${rRow(n.sup)}}`; return s }
      case 'delim': return `\\left${n.o || '.'} ${rRow(n.b)} \\right${n.c || '.'}`
      case 'big': { let s = n.op + ' '; if (n.lo) s += `_{${rRow(n.lo)}}`; if (n.hi) s += `^{${rRow(n.hi)}}`; return s }
      case 'acc': return `${n.cmd}{${rRow(n.b)}}`
      case 'mat': return `\\begin{${n.env}}${n.rows.map(r => r.map(rRow).join(' & ')).join(' \\\\ ')}\\end{${n.env}}`
    }
  }
  return { latex: rRow(root), map }
}

// ── Edit operations (mutate root + cursor in place) ──────────────────────────────
export function deleteSelection(cur: Cursor): boolean {
  if (cur.anchor == null || cur.anchor === cur.idx) { cur.anchor = null; return false }
  const a = Math.min(cur.anchor, cur.idx), b = Math.max(cur.anchor, cur.idx)
  const removed = cur.row.splice(a, b - a); cur.idx = a; cur.anchor = null
  return removed.length > 0
}
export function insertNode(cur: Cursor, n: MNode): void { deleteSelection(cur); cur.row.splice(cur.idx, 0, n); cur.idx++ }
export function insertAtom(cur: Cursor, t: string): void { insertNode(cur, { k: 'atom', t }) }
export function insertRow(cur: Cursor, row: MRow): void { deleteSelection(cur); cur.row.splice(cur.idx, 0, ...row); cur.idx += row.length }

export function moveH(root: MRow, cur: Cursor, dir: 1 | -1): void {
  if (cur.anchor != null) { cur.idx = dir < 0 ? Math.min(cur.anchor, cur.idx) : Math.max(cur.anchor, cur.idx); cur.anchor = null; return }
  const sl = slots(root)
  const k = sl.findIndex(s => s.row === cur.row && s.idx === cur.idx)
  const nk = Math.max(0, Math.min(sl.length - 1, (k < 0 ? 0 : k) + dir))
  cur.row = sl[nk].row; cur.idx = sl[nk].idx
}
export function selectH(cur: Cursor, dir: 1 | -1): void {
  if (cur.anchor == null) cur.anchor = cur.idx
  cur.idx = Math.max(0, Math.min(cur.row.length, cur.idx + dir))
}
export function moveHome(cur: Cursor, end: boolean): void { cur.anchor = null; cur.idx = end ? cur.row.length : 0 }
export function moveV(root: MRow, cur: Cursor, dir: 1 | -1): void {
  // Up/Down between paired fields (frac num↔den, script sup↔sub, root index↔body).
  const par = findParent(root, cur.row); if (!par) return
  const n = par.node
  const go = (r: MRow | null) => { if (r) { cur.row = r; cur.idx = Math.min(cur.idx, r.length); cur.anchor = null } }
  if (n.k === 'frac') go(dir < 0 ? (par.field === 'd' ? n.n : null) : (par.field === 'n' ? n.d : null))
  else if (n.k === 'script') go(dir < 0 ? (par.field === 'sub' ? n.sup : null) : (par.field === 'sup' ? n.sub : null))
  else if (n.k === 'root') go(dir < 0 ? (par.field === 'b' ? n.i : null) : (par.field === 'i' ? n.b : null))
  else if (n.k === 'big') go(dir < 0 ? (par.field === 'lo' ? n.hi : null) : (par.field === 'hi' ? n.lo : null))
}
export function backspace(root: MRow, cur: Cursor): void {
  if (deleteSelection(cur)) return
  if (cur.idx > 0) {
    const prev = cur.row[cur.idx - 1]
    // Stepping a script back removes only the script wrapper (keep the base atom).
    if (prev.k === 'script') { cur.row.splice(cur.idx - 1, 1, prev.base); return }
    cur.row.splice(cur.idx - 1, 1); cur.idx--
    return
  }
  // At the start of a sub-row: hop out before the structure (a second backspace deletes it).
  const par = findParent(root, cur.row)
  if (par) {
    const empty = isEmptyStruct(par.node)
    cur.row = par.row; cur.idx = par.idx
    if (empty) cur.row.splice(cur.idx, 1)
  }
}
function isEmptyStruct(n: MNode): boolean {
  switch (n.k) {
    case 'frac': return !n.n.length && !n.d.length
    case 'sqrt': return !n.b.length
    case 'root': return !n.i.length && !n.b.length
    case 'delim': return !n.b.length
    case 'acc': return !n.b.length
    case 'big': return !(n.lo?.length) && !(n.hi?.length)
    default: return false
  }
}
export function deleteFwd(root: MRow, cur: Cursor): void {
  if (deleteSelection(cur)) return
  if (cur.idx < cur.row.length) cur.row.splice(cur.idx, 1)
}

// Take the operand to wrap: the current selection, else the single node before the cursor.
function takeOperand(cur: Cursor): MRow {
  if (cur.anchor != null && cur.anchor !== cur.idx) {
    const a = Math.min(cur.anchor, cur.idx), b = Math.max(cur.anchor, cur.idx)
    const out = cur.row.splice(a, b - a); cur.idx = a; cur.anchor = null; return out
  }
  if (cur.idx > 0) { const x = cur.row.splice(cur.idx - 1, 1); cur.idx--; return x }
  return []
}
export function makeFrac(cur: Cursor): void {
  const num = takeOperand(cur); const den: MRow = []
  cur.row.splice(cur.idx, 0, { k: 'frac', n: num, d: den }); cur.idx++
  cur.row = den; cur.idx = 0
}
export function makeScript(cur: Cursor, kind: 'sup' | 'sub'): void {
  const prev = cur.idx > 0 ? cur.row[cur.idx - 1] : null
  if (prev && prev.k === 'script') {
    if (kind === 'sup') { prev.sup = prev.sup ?? []; cur.row = prev.sup } else { prev.sub = prev.sub ?? []; cur.row = prev.sub }
    cur.idx = cur.row.length; return
  }
  const base: MNode = prev ? (cur.row.splice(cur.idx - 1, 1), cur.idx--, prev) : { k: 'atom', t: '\\square' }
  const field: MRow = []
  cur.row.splice(cur.idx, 0, { k: 'script', base, sup: kind === 'sup' ? field : null, sub: kind === 'sub' ? field : null }); cur.idx++
  cur.row = field; cur.idx = 0
}
// Insert a structure with empty slots; the selection (if any) seeds the primary slot.
export function insertStruct(cur: Cursor, kind: string, opt?: { o?: string; c?: string; op?: string; cmd?: string; rows?: number; cols?: number; env?: string }): void {
  const seed = (cur.anchor != null && cur.anchor !== cur.idx) ? takeOperand(cur) : []
  let node: MNode, into: MRow
  switch (kind) {
    case 'sqrt': { const b = seed; node = { k: 'sqrt', b }; into = b; break }
    case 'root': { const i: MRow = [], b = seed; node = { k: 'root', i, b }; into = b.length ? i : b; break }
    case 'delim': { const b = seed; node = { k: 'delim', o: opt?.o ?? '(', c: opt?.c ?? ')', b }; into = b; break }
    case 'big': { const lo: MRow = [], hi: MRow = []; node = { k: 'big', op: opt?.op ?? '\\sum', lo, hi }; into = lo; break }
    case 'acc': { const b = seed; node = { k: 'acc', cmd: opt?.cmd ?? '\\hat', b }; into = b; break }
    case 'mat': { const R = opt?.rows ?? 2, C = opt?.cols ?? 2; const rows: MRow[][] = Array.from({ length: R }, () => Array.from({ length: C }, () => [] as MRow)); node = { k: 'mat', env: opt?.env ?? 'pmatrix', rows }; into = rows[0][0]; break }
    default: { const b = seed; node = { k: 'sqrt', b }; into = b }
  }
  deleteSelection(cur)
  cur.row.splice(cur.idx, 0, node); cur.idx++
  cur.row = into; cur.idx = into.length
}
// Insert a parsed LaTeX snippet (from the palette) at the cursor, landing on its first □.
export function insertLatex(root: MRow, cur: Cursor, tex: string): void {
  const sub = parse(tex)
  insertRow(cur, sub)
  // move into the first empty placeholder atom (\square), if any, for guided filling
  const sl = slots(root)
  const k = sl.findIndex(s => s.row === cur.row && s.idx === cur.idx)
  for (let j = Math.max(0, k - sub.length * 4); j < sl.length; j++) {
    const s = sl[j]; if (s.idx < s.row.length && s.row[s.idx]?.k === 'atom' && (s.row[s.idx] as { t: string }).t === '\\square') { cur.row = s.row; cur.idx = s.idx; cur.anchor = s.idx + 1; return }
    if (s.row.length === 0) { cur.row = s.row; cur.idx = 0; return }
  }
}

// Find the parent structure containing a given row (for Up/Down navigation + delete).
export function findParent(root: MRow, target: MRow): { node: MNode; field: string; row: MRow; idx: number } | null {
  function inRow(row: MRow): ReturnType<typeof findParent> {
    for (let i = 0; i < row.length; i++) {
      const n = row[i]
      const fields: [string, MRow | null][] =
        n.k === 'frac' ? [['n', n.n], ['d', n.d]]
        : n.k === 'sqrt' ? [['b', n.b]]
        : n.k === 'root' ? [['i', n.i], ['b', n.b]]
        : n.k === 'script' ? [['sub', n.sub], ['sup', n.sup]]
        : n.k === 'delim' ? [['b', n.b]]
        : n.k === 'big' ? [['lo', n.lo], ['hi', n.hi]]
        : n.k === 'acc' ? [['b', n.b]]
        : n.k === 'mat' ? n.rows.flatMap((r, ri) => r.map((c, ci) => [`mat:${ri}:${ci}`, c] as [string, MRow])) : []
      for (const [f, sub] of fields) { if (sub === target) return { node: n, field: f, row, idx: i }; if (sub) { const deep = inRow(sub); if (deep) return deep } }
    }
    return null
  }
  return inRow(root)
}
