// Autocompletion engine for the Maths code editor (LatexEditor). Pure logic — the popup
// UI lives in LatexEditor. Three completion contexts, detected from the text + caret:
//   • `\prefix`            → LaTeX commands (large KaTeX catalogue, with snippets)
//   • `\begin{p` / `\end{` → environment names (`\end` ranks the currently open ones first)
//   • graph sections       → plotter function / constant names (mathExpr)
// Snippets use the ‸ marker for the post-insertion caret position (stripped on insert).
import { CARET } from './mathSymbols'

export interface Completion {
  label: string            // what is matched & shown (command name without backslash)
  insert: string           // full replacement text (may contain ‸)
  tex?: string             // KaTeX source for the visual preview chip
  detail?: string          // short French description
  kind: 'cmd' | 'env' | 'fn' | 'const'
}

export interface AcContext {
  kind: 'latex' | 'env-begin' | 'env-end' | 'graph'
  start: number            // index in the text where the replacement begins
  prefix: string           // what the user already typed (without \ or \begin{)
}

// ── Catalogue ─────────────────────────────────────────────────────────────────────
const cmd = (label: string, insert: string, tex: string | undefined, detail: string | undefined): Completion =>
  ({ label, insert, tex, detail, kind: 'cmd' })
const sym = (name: string, detail?: string): Completion => cmd(name, `\\${name} `, `\\${name}`, detail)

const GREEK = ('alpha beta gamma delta epsilon varepsilon zeta eta theta vartheta iota kappa lambda mu nu xi pi varpi '
  + 'rho varrho sigma varsigma tau upsilon phi varphi chi psi omega Gamma Delta Theta Lambda Xi Pi Sigma Upsilon Phi Psi Omega').split(' ')
const FUNCS = 'sin cos tan sec csc cot arcsin arccos arctan sinh cosh tanh coth ln log lg exp det gcd min max sup inf arg deg dim hom ker Pr'.split(' ')
const OPS = ('pm mp times div cdot ast star circ bullet oplus ominus otimes oslash odot dagger ddagger amalg cap cup uplus '
  + 'sqcap sqcup vee wedge setminus wr diamond triangleleft triangleright bigtriangleup bigtriangledown').split(' ')
const RELS = ('leq geq equiv models prec succ sim perp preceq succeq simeq mid ll gg parallel subset supset approx bowtie '
  + 'subseteq supseteq cong neq smile sqsubseteq sqsupseteq doteq frown in ni notin propto vdash dashv asymp').split(' ')
const ARROWS = ('leftarrow rightarrow to gets uparrow downarrow leftrightarrow updownarrow Leftarrow Rightarrow Uparrow Downarrow '
  + 'Leftrightarrow Updownarrow mapsto longmapsto hookleftarrow hookrightarrow nearrow searrow swarrow nwarrow implies iff '
  + 'longleftarrow longrightarrow longleftrightarrow Longleftarrow Longrightarrow rightharpoonup rightharpoondown leftharpoonup leftharpoondown rightleftharpoons').split(' ')
const MISC = ('infty partial nabla forall exists nexists emptyset varnothing aleph hbar ell wp Re Im angle measuredangle triangle '
  + 'square blacksquare prime dots cdots vdots ddots ldots therefore because degree neg lnot top bot flat natural sharp '
  + 'clubsuit diamondsuit heartsuit spadesuit surd checkmark langle rangle lceil rceil lfloor rfloor vert Vert quad qquad').split(' ')
const ACCENTS = 'hat check breve acute grave tilde bar vec dot ddot mathring widehat widetilde overline underline overrightarrow overleftarrow overbrace underbrace boxed cancel phantom'.split(' ')
const FONTS = 'mathbb mathcal mathfrak mathscr mathrm mathbf mathit mathsf mathtt boldsymbol text textbf textit texttt operatorname'.split(' ')
const TWOARG: [string, string][] = [
  ['frac', 'Fraction'], ['dfrac', 'Fraction (display)'], ['tfrac', 'Fraction (inline)'],
  ['binom', 'Coefficient binomial'], ['dbinom', 'Binôme (display)'], ['tbinom', 'Binôme (inline)'],
  ['overset', 'Au-dessus de'], ['underset', 'En dessous de'], ['stackrel', 'Empiler'],
]
const BIGOPS_LIM = 'sum prod coprod int oint iint iiint bigcap bigcup bigsqcup bigvee bigwedge bigodot bigotimes bigoplus biguplus'.split(' ')

export const LATEX_ENVS = 'matrix pmatrix bmatrix Bmatrix vmatrix Vmatrix smallmatrix cases rcases aligned alignedat gathered array split'.split(' ')

const FONT_SAMPLE: Record<string, string> = { mathbb: 'R', mathcal: 'L', mathfrak: 'g', mathscr: 'F', boldsymbol: 'x' }

function buildLatexCatalogue(): Completion[] {
  const out: Completion[] = []
  GREEK.forEach(n => out.push(sym(n, 'Lettre grecque')))
  FUNCS.forEach(n => out.push(cmd(n, `\\${n}(${CARET})`, `\\${n}`, 'Fonction')))
  OPS.forEach(n => out.push(sym(n, 'Opérateur')))
  RELS.forEach(n => out.push(sym(n, 'Relation')))
  ARROWS.forEach(n => out.push(sym(n, 'Flèche')))
  MISC.forEach(n => out.push(sym(n)))
  ACCENTS.forEach(n => out.push(cmd(n, `\\${n}{${CARET}}`, `\\${n}{x}`, 'Accent / décoration')))
  FONTS.forEach(n => out.push(cmd(n, `\\${n}{${CARET}}`, `\\${n}{${FONT_SAMPLE[n] ?? 'A'}}`, 'Police')))
  TWOARG.forEach(([n, d]) => out.push(cmd(n, `\\${n}{${CARET}}{}`, `\\${n}{a}{b}`, d)))
  BIGOPS_LIM.forEach(n => out.push(cmd(n, `\\${n}_{${CARET}}^{}`, `\\${n}`, 'Grand opérateur')))
  out.push(
    cmd('sqrt', `\\sqrt{${CARET}}`, '\\sqrt{x}', 'Racine carrée'),
    cmd('sqrtn', `\\sqrt[${CARET}]{}`, '\\sqrt[n]{x}', 'Racine n-ième'),
    cmd('lim', `\\lim_{${CARET} \\to }`, '\\lim', 'Limite'),
    cmd('limsup', `\\limsup_{${CARET}}`, '\\limsup', 'Limite supérieure'),
    cmd('liminf', `\\liminf_{${CARET}}`, '\\liminf', 'Limite inférieure'),
    cmd('textcolor', `\\textcolor{${CARET}}{}`, '\\textcolor{red}{x}', 'Couleur'),
    cmd('hspace', `\\hspace{${CARET}}`, undefined, 'Espace horizontal'),
    cmd('left', `\\left(${CARET}\\right)`, '\\left(x\\right)', 'Délimiteurs adaptatifs'),
    cmd('begin', `\\begin{${CARET}}`, undefined, 'Environnement'),
    cmd('end', `\\end{${CARET}}`, undefined, 'Fin d\'environnement'),
    cmd('not', `\\not${CARET}`, '\\not=', 'Négation'),
    cmd('substack', `\\substack{${CARET} \\\\ }`, undefined, 'Indices empilés'),
    cmd('pmod', `\\pmod{${CARET}}`, '\\pmod{n}', 'Modulo'),
    cmd('bmod', `\\bmod ${CARET}`, '\\bmod', 'Modulo (binaire)'),
  )
  return out
}
export const LATEX_CATALOGUE: Completion[] = buildLatexCatalogue()

// Curated shortlist shown right after typing a bare `\`.
const CURATED = ['frac', 'sqrt', 'sum', 'int', 'lim', 'pi', 'alpha', 'times', 'cdot', 'infty', 'vec', 'begin']
const CURATED_ITEMS: Completion[] = CURATED
  .map(n => LATEX_CATALOGUE.find(c => c.label === n))
  .filter((c): c is Completion => !!c)

// Plotter names (mathExpr) for graph sections.
const GRAPH_FNS = 'sin cos tan asin acos atan sec csc cot sinh cosh tanh asinh acosh atanh exp ln log log10 log2 sqrt cbrt abs floor ceil round sign trunc pow atan2 min max mod fact'.split(' ')
const GRAPH_CONSTS = 'x pi e tau phi'.split(' ')
const GRAPH_ITEMS: Completion[] = [
  ...GRAPH_FNS.map((n): Completion => ({ label: n, insert: `${n}(${CARET})`, detail: 'Fonction', kind: 'fn' })),
  ...GRAPH_CONSTS.map((n): Completion => ({ label: n, insert: n, detail: n === 'x' ? 'Variable' : 'Constante', kind: 'const' })),
]

// ── Context detection ─────────────────────────────────────────────────────────────
// Which %%[…] section is the caret in? (No delimiter above ⇒ treat as formula/LaTeX.)
function sectionAt(text: string, caret: number): 'formule' | 'texte' | 'graphe' {
  const lines = text.slice(0, caret).split('\n')
  for (let i = lines.length - 1; i >= 0; i--) {
    const m = /^%%\[(formule|texte|graphe)\b/i.exec(lines[i].trim())
    if (m) return m[1].toLowerCase() as 'formule' | 'texte' | 'graphe'
  }
  return 'formule'
}

export function getContext(text: string, caret: number): AcContext | null {
  const before = text.slice(0, caret)
  const sec = sectionAt(text, caret)
  if (sec === 'texte') return null
  const mEnv = /\\(begin|end)\{([a-zA-Z]*)$/.exec(before)
  if (mEnv) return { kind: mEnv[1] === 'begin' ? 'env-begin' : 'env-end', start: caret - mEnv[2].length, prefix: mEnv[2] }
  if (sec === 'graphe') {
    const m = /([a-zA-Z][a-zA-Z0-9]*)$/.exec(before)
    return m ? { kind: 'graph', start: caret - m[1].length, prefix: m[1] } : null
  }
  const m = /\\([a-zA-Z]*)$/.exec(before)
  if (m) return { kind: 'latex', start: caret - m[1].length - 1, prefix: m[1] }
  return null
}

// Environments opened before the caret and not yet closed (innermost first).
function openEnvs(text: string, caret: number): string[] {
  const stack: string[] = []
  const re = /\\(begin|end)\{([a-zA-Z]+)\}/g
  const before = text.slice(0, caret)
  let m: RegExpExecArray | null
  while ((m = re.exec(before)) !== null) {
    if (m[1] === 'begin') stack.push(m[2])
    else { const i = stack.lastIndexOf(m[2]); if (i >= 0) stack.splice(i, 1) }
  }
  return stack.reverse()
}

// ── Filtering / ranking ───────────────────────────────────────────────────────────
function rank(items: Completion[], prefix: string, limit = 12): Completion[] {
  if (!prefix) return items.slice(0, limit)
  const p = prefix.toLowerCase()
  const scored: [number, Completion][] = []
  for (const it of items) {
    const l = it.label.toLowerCase()
    let s = -1
    if (it.label.startsWith(prefix)) s = 3
    else if (l.startsWith(p)) s = 2
    else if (l.includes(p)) s = 1
    if (s >= 0) scored.push([s, it])
  }
  scored.sort((a, b) => b[0] - a[0] || a[1].label.length - b[1].label.length || a[1].label.localeCompare(b[1].label))
  return scored.slice(0, limit).map(x => x[1])
}

export function getCompletions(text: string, caret: number, ctx: AcContext): Completion[] {
  if (ctx.kind === 'latex') {
    return ctx.prefix ? rank(LATEX_CATALOGUE, ctx.prefix) : CURATED_ITEMS
  }
  if (ctx.kind === 'graph') {
    const items = rank(GRAPH_ITEMS, ctx.prefix, 10)
    // Avoid a pointless popup when the only match is the constant already fully typed.
    if (items.length === 1 && items[0].kind === 'const' && items[0].label === ctx.prefix) return []
    return items
  }
  // Environments: `\end{` proposes the still-open ones first.
  const envItem = (n: string, detail?: string): Completion => ({
    label: n,
    insert: ctx.kind === 'env-begin' ? `${n}}${CARET}\\end{${n}}` : `${n}}${CARET}`,
    detail,
    kind: 'env',
  })
  if (ctx.kind === 'env-end') {
    const open = openEnvs(text, ctx.start).filter(n => n.startsWith(ctx.prefix))
    const rest = rank(LATEX_ENVS.filter(n => !open.includes(n)).map(n => envItem(n)), ctx.prefix, 12 - open.length)
    return [...open.map(n => envItem(n, 'Environnement ouvert')), ...rest]
  }
  return rank(LATEX_ENVS.map(n => envItem(n)), ctx.prefix)
}

export { CARET as AC_CARET }
