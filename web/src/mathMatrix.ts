// mathMatrix — numeric linear-algebra helpers for the Maths module. It reads a LaTeX matrix
// (\begin{…matrix}…\end{…matrix}, each cell a constant expression), computes determinant /
// transpose / inverse / rank / trace and renders the result back to LaTeX. Cells are evaluated
// with the module's numeric evaluator (mathEval), so a non-numeric (symbolic) matrix yields null.
import { evalLatex } from './mathEval'

export interface Matrix { m: number[][]; env: string }
export interface MatResult { latex: string; error: string | null }

function gcd(a: number, b: number): number { a = Math.abs(a); b = Math.abs(b); while (b) { [a, b] = [b, a % b] } return a || 1 }
// A value as a clean integer, a small reduced fraction, or a trimmed decimal.
function fracLatex(v: number): string {
  if (!Number.isFinite(v)) return '\\text{—}'
  const r = Math.round(v)
  if (Math.abs(v - r) < 1e-9) return String(r)
  for (let d = 2; d <= 1000; d++) { const p = v * d; if (Math.abs(p - Math.round(p)) < 1e-9) { const g = gcd(Math.round(p), d), n = Math.round(p) / g, den = d / g; return n < 0 ? `-\\frac{${-n}}{${den}}` : `\\frac{${n}}{${den}}` } }
  return Number(v.toPrecision(6)).toString()
}

// Parse the first \begin{…matrix}…\end{…matrix} found in `latex`. Returns null if not a numeric,
// rectangular matrix.
export function parseMatrix(latex: string): Matrix | null {
  const mm = /\\begin\{([a-zA-Z]*matrix)\}([\s\S]*?)\\end\{\1\}/.exec(latex ?? '')
  if (!mm) return null
  const env = mm[1]
  const rows = mm[2].split(/\\\\/).map(s => s.trim()).filter(s => s.length)
  if (!rows.length) return null
  const grid = rows.map(r => r.split('&').map(c => evalLatex(c.trim())))
  const cols = grid[0].length
  if (grid.some(r => r.length !== cols || r.some(v => v == null))) return null
  return { m: grid as number[][], env }
}

export function matToLatex(m: number[][], env = 'pmatrix'): string {
  return `\\begin{${env}}${m.map(r => r.map(fracLatex).join(' & ')).join(' \\\\ ')}\\end{${env}}`
}

const isSquare = (m: number[][]) => m.length > 0 && m.every(r => r.length === m.length)

export function determinant(m: number[][]): number | null {
  if (!isSquare(m)) return null
  const n = m.length, a = m.map(r => r.slice())
  let d = 1
  for (let i = 0; i < n; i++) {
    let p = i
    for (let r = i + 1; r < n; r++) if (Math.abs(a[r][i]) > Math.abs(a[p][i])) p = r
    if (Math.abs(a[p][i]) < 1e-12) return 0
    if (p !== i) { [a[i], a[p]] = [a[p], a[i]]; d = -d }
    d *= a[i][i]
    for (let r = i + 1; r < n; r++) { const f = a[r][i] / a[i][i]; for (let c = i; c < n; c++) a[r][c] -= f * a[i][c] }
  }
  return d
}

export function transpose(m: number[][]): number[][] {
  return m[0].map((_, c) => m.map(r => r[c]))
}

export function inverse(m: number[][]): number[][] | null {
  if (!isSquare(m)) return null
  const n = m.length
  const a = m.map((r, i) => [...r, ...Array.from({ length: n }, (_, j) => (i === j ? 1 : 0))])
  for (let i = 0; i < n; i++) {
    let p = i
    for (let r = i + 1; r < n; r++) if (Math.abs(a[r][i]) > Math.abs(a[p][i])) p = r
    if (Math.abs(a[p][i]) < 1e-12) return null
    if (p !== i) { [a[i], a[p]] = [a[p], a[i]] }
    const piv = a[i][i]
    for (let c = 0; c < 2 * n; c++) a[i][c] /= piv
    for (let r = 0; r < n; r++) if (r !== i) { const f = a[r][i]; for (let c = 0; c < 2 * n; c++) a[r][c] -= f * a[i][c] }
  }
  return a.map(r => r.slice(n))
}

export function rank(m: number[][]): number {
  const a = m.map(r => r.slice()), rows = a.length, cols = a[0].length
  let lead = 0, rk = 0
  for (let r = 0; r < rows && lead < cols; r++) {
    let i = r
    while (Math.abs(a[i][lead]) < 1e-12) { i++; if (i === rows) { i = r; lead++; if (lead === cols) return rk } }
    [a[i], a[r]] = [a[r], a[i]]
    const piv = a[r][lead]
    for (let c = 0; c < cols; c++) a[r][c] /= piv
    for (let j = 0; j < rows; j++) if (j !== r) { const f = a[j][lead]; for (let c = 0; c < cols; c++) a[j][c] -= f * a[r][c] }
    rk++; lead++
  }
  return rk
}

export function trace(m: number[][]): number | null {
  if (!isSquare(m)) return null
  return m.reduce((s, r, i) => s + r[i], 0)
}

const NOT_MAT = 'Sélectionnez un bloc contenant une matrice numérique.'
const NOT_SQUARE = 'Opération réservée aux matrices carrées.'

export function determinantLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  const d = determinant(mat.m); if (d == null) return { latex: '', error: NOT_SQUARE }
  return { latex: `${matToLatex(mat.m, 'vmatrix')} = ${fracLatex(d)}`, error: null }
}
export function transposeLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  return { latex: `${matToLatex(mat.m)}^{T} = ${matToLatex(transpose(mat.m))}`, error: null }
}
export function inverseLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  if (!isSquare(mat.m)) return { latex: '', error: NOT_SQUARE }
  const inv = inverse(mat.m); if (!inv) return { latex: '', error: 'Matrice non inversible (déterminant nul).' }
  return { latex: `${matToLatex(mat.m)}^{-1} = ${matToLatex(inv)}`, error: null }
}
export function rankLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  return { latex: `\\operatorname{rg}\\,${matToLatex(mat.m)} = ${rank(mat.m)}`, error: null }
}
export function traceLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  const tr = trace(mat.m); if (tr == null) return { latex: '', error: NOT_SQUARE }
  return { latex: `\\operatorname{tr}\\,${matToLatex(mat.m)} = ${fracLatex(tr)}`, error: null }
}

// ── Advanced: power, characteristic polynomial, eigenvalues, RREF, linear system ──────
const identity = (n: number): number[][] => Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (i === j ? 1 : 0)))
function matMul(a: number[][], b: number[][]): number[][] {
  return a.map(row => b[0].map((_, j) => row.reduce((s, v, k) => s + v * b[k][j], 0)))
}
export function matPow(m: number[][], p: number): number[][] | null {
  if (!isSquare(m)) return null
  const n = m.length
  let base = m
  if (p < 0) { const inv = inverse(m); if (!inv) return null; base = inv; p = -p }
  let r = identity(n)
  for (let i = 0; i < p; i++) r = matMul(r, base)
  return r
}
export function powerLatex(latex: string, p: number): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  if (!isSquare(mat.m)) return { latex: '', error: NOT_SQUARE }
  const r = matPow(mat.m, p); if (!r) return { latex: '', error: 'Matrice non inversible (puissance négative).' }
  return { latex: `${matToLatex(mat.m)}^{${p}} = ${matToLatex(r)}`, error: null }
}

// Reduced row-echelon form + rank.
function rrefFull(m: number[][]): { R: number[][]; rank: number } {
  const a = m.map(r => r.slice()), rows = a.length, cols = a[0].length
  let lead = 0, rk = 0
  for (let r = 0; r < rows && lead < cols; r++) {
    let i = r
    while (Math.abs(a[i][lead]) < 1e-12) { i++; if (i === rows) { i = r; lead++; if (lead === cols) return { R: a, rank: rk } } }
    if (i !== r) { [a[i], a[r]] = [a[r], a[i]] }
    const piv = a[r][lead]
    for (let c = 0; c < cols; c++) a[r][c] /= piv
    for (let j = 0; j < rows; j++) if (j !== r) { const f = a[j][lead]; for (let c = 0; c < cols; c++) a[j][c] -= f * a[r][c] }
    rk++; lead++
  }
  return { R: a, rank: rk }
}
export function rrefLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  return { latex: `${matToLatex(mat.m)} \\;\\sim\\; ${matToLatex(rrefFull(mat.m).R)}`, error: null }
}

// Characteristic polynomial coefficients [1, c₁, …, cₙ] of det(λI − A) via Faddeev–LeVerrier.
function charPolyCoeffs(A: number[][]): number[] {
  const n = A.length
  const c = [1]
  let M = identity(n) // M₁ = I
  c.push(-trace(matMul(A, M))!) // c₁ = −tr(A)
  for (let k = 2; k <= n; k++) {
    M = matMul(A, M).map((row, i) => row.map((v, j) => v + (i === j ? c[k - 1] : 0))) // Mₖ = A·M_{k−1} + c_{k−1}I
    c.push(-trace(matMul(A, M))! / k)
  }
  return c
}
const polyVarLatex = (c: number[], v: string) => {
  const deg = c.length - 1
  const parts: string[] = []
  for (let i = 0; i < c.length; i++) {
    const co = c[i], p = deg - i
    if (Math.abs(co) < 1e-9 && i > 0) continue
    const mag = Math.abs(co)
    const coef = p > 0 && Math.abs(mag - 1) < 1e-9 ? '' : fracLatex(mag)
    const xp = p === 0 ? '' : p === 1 ? v : `${v}^{${p}}`
    parts.push((co < 0 ? '-' : parts.length ? '+' : '') + (coef + xp || '0'))
  }
  return parts.join(' ')
}
export function charPolyLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  if (!isSquare(mat.m)) return { latex: '', error: NOT_SQUARE }
  return { latex: `\\chi(\\lambda) = ${polyVarLatex(charPolyCoeffs(mat.m), '\\lambda')}`, error: null }
}
export function eigenvaluesLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  if (!isSquare(mat.m)) return { latex: '', error: NOT_SQUARE }
  const c = charPolyCoeffs(mat.m), n = c.length - 1
  const P = (x: number) => c.reduce((s, k) => s * x + k, 0) // Horner
  const R = 1 + Math.max(...c.slice(1).map(Math.abs))
  const roots: number[] = []
  const push = (r: number) => { if (!roots.some(x => Math.abs(x - r) < 1e-6)) roots.push(r) }
  const steps = 4000, dx = (2 * R) / steps
  let px = -R, pv = P(-R)
  for (let i = 1; i <= steps; i++) {
    const x = -R + i * dx, v = P(x)
    if (pv * v < 0) { let lo = px, hi = x, fl = pv; for (let k = 0; k < 60; k++) { const mid = (lo + hi) / 2, fm = P(mid); if (fl * fm < 0) hi = mid; else { lo = mid; fl = fm } } push((lo + hi) / 2) } else if (Math.abs(v) < 1e-9) push(x)
    px = x; pv = v
  }
  roots.sort((a, b) => a - b)
  const list = roots.map((r, i) => `\\lambda_{${i + 1}} \\approx ${fracLatex(Math.abs(r - Math.round(r)) < 1e-6 ? Math.round(r) : Number(r.toPrecision(6)))}`).join(',\\quad ')
  const suffix = roots.length < n ? '\\quad (\\text{+ valeurs propres complexes})' : ''
  return { latex: roots.length ? list + suffix : '\\text{Aucune valeur propre réelle}', error: null }
}

// Solve a linear system given as an augmented matrix [A | b] (n rows, m+1 columns).
export function solveSystemLatex(latex: string): MatResult {
  const mat = parseMatrix(latex); if (!mat) return { latex: '', error: NOT_MAT }
  const cols = mat.m[0].length, unk = cols - 1
  if (unk < 1) return { latex: '', error: 'Fournissez une matrice augmentée [A | b] (colonnes = inconnues + 1).' }
  const { R } = rrefFull(mat.m)
  const rankA = rrefFull(mat.m.map(r => r.slice(0, unk))).rank
  const rankAug = R.filter(r => r.some(v => Math.abs(v) > 1e-9)).length
  if (rankAug > rankA) return { latex: '\\text{Système incompatible : aucune solution.}', error: null }
  if (rankA < unk) return { latex: '\\text{Système à une infinité de solutions.}', error: null }
  const sol: string[] = []
  for (let j = 0; j < unk; j++) { const row = R.find(r => Math.abs(r[j] - 1) < 1e-9 && r.every((v, c) => c === j || c >= unk || Math.abs(v) < 1e-9)); sol.push(`x_{${j + 1}} = ${fracLatex(row ? row[cols - 1] : 0)}`) }
  return { latex: `\\begin{cases} ${sol.join(' \\\\ ')} \\end{cases}`, error: null }
}
