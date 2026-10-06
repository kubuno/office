// mathStats — descriptive statistics and linear regression for the Maths module. It reads a
// list of numbers (comma/semicolon separated, or a matrix row) or a list of (x, y) points and
// summarises them. Cells are evaluated with the module's numeric evaluator (mathEval).
import { evalLatex, fmtResult } from './mathEval'

export interface StatResult { latex: string; error: string | null }
const fmt = (v: number) => (Number.isFinite(v) ? fmtResult(Number(v.toPrecision(8))) : '\\text{—}')

// Parse a flat list of numbers from LaTeX (strips matrix/braces wrappers, splits on , ; & \\).
export function parseList(latex: string): number[] | null {
  let s = (latex ?? '').trim()
  if (!s) return null
  s = s.replace(/\\begin\{[a-zA-Z*]+\}|\\end\{[a-zA-Z*]+\}|\\left|\\right|\\{|\\}|[{}]/g, ' ')
  const parts = s.split(/[,;&]|\\\\/).map(p => p.trim()).filter(p => p.length)
  if (!parts.length) return null
  const nums = parts.map(p => evalLatex(p))
  if (nums.some(v => v == null)) return null
  return nums as number[]
}

// Parse a list of (x, y) points:  (1,2),(2,3.5),(3,5)   or  \left(1;2\right)…
export function parsePoints(latex: string): [number, number][] | null {
  const s = (latex ?? '').replace(/\\left|\\right/g, '')
  const re = /\(([^()]+)\)/g
  const pts: [number, number][] = []
  let m: RegExpExecArray | null
  while ((m = re.exec(s))) {
    const c = m[1].split(/[,;]/).map(x => evalLatex(x.trim()))
    if (c.length !== 2 || c.some(v => v == null)) return null
    pts.push([c[0] as number, c[1] as number])
  }
  return pts.length ? pts : null
}

function quantile(sorted: number[], q: number): number {
  const pos = (sorted.length - 1) * q
  const base = Math.floor(pos), rest = pos - base
  return sorted[base + 1] !== undefined ? sorted[base] + rest * (sorted[base + 1] - sorted[base]) : sorted[base]
}

// Full descriptive summary: n, sum, mean, variance, standard deviation, min, max.
export function statsLatex(latex: string): StatResult {
  const xs = parseList(latex)
  if (!xs) return { latex: '', error: 'Entrez une liste de nombres (ex. 1, 2, 3, 4).' }
  const n = xs.length
  const sum = xs.reduce((a, b) => a + b, 0)
  const mean = sum / n
  const varc = xs.reduce((a, b) => a + (b - mean) ** 2, 0) / n
  return {
    latex: `\\begin{aligned} n &= ${n} & \\bar{x} &= ${fmt(mean)} \\\\ \\sum x &= ${fmt(sum)} & \\sigma &= ${fmt(Math.sqrt(varc))} \\\\ \\min &= ${fmt(Math.min(...xs))} & \\sigma^2 &= ${fmt(varc)} \\\\ \\max &= ${fmt(Math.max(...xs))} & & \\end{aligned}`,
    error: null,
  }
}

// Five-number summary (min, Q1, median, Q3, max).
export function fiveNumberLatex(latex: string): StatResult {
  const xs = parseList(latex)
  if (!xs) return { latex: '', error: 'Entrez une liste de nombres.' }
  const s = [...xs].sort((a, b) => a - b)
  return { latex: `\\min = ${fmt(s[0])},\\ Q_1 = ${fmt(quantile(s, 0.25))},\\ \\tilde{x} = ${fmt(quantile(s, 0.5))},\\ Q_3 = ${fmt(quantile(s, 0.75))},\\ \\max = ${fmt(s[s.length - 1])}`, error: null }
}

// Least-squares linear regression y = a·x + b, with the coefficient of determination R².
export function regressionLatex(latex: string): StatResult {
  const pts = parsePoints(latex)
  if (!pts || pts.length < 2) return { latex: '', error: 'Entrez au moins deux points (ex. (1,2),(2,3),(3,5)).' }
  const n = pts.length
  const sx = pts.reduce((a, p) => a + p[0], 0), sy = pts.reduce((a, p) => a + p[1], 0)
  const sxx = pts.reduce((a, p) => a + p[0] * p[0], 0), sxy = pts.reduce((a, p) => a + p[0] * p[1], 0)
  const den = n * sxx - sx * sx
  if (Math.abs(den) < 1e-12) return { latex: '', error: 'Régression impossible (abscisses toutes égales).' }
  const a = (n * sxy - sx * sy) / den
  const b = (sy - a * sx) / n
  const my = sy / n
  const ssTot = pts.reduce((s, p) => s + (p[1] - my) ** 2, 0)
  const ssRes = pts.reduce((s, p) => s + (p[1] - (a * p[0] + b)) ** 2, 0)
  const r2 = ssTot < 1e-12 ? 1 : 1 - ssRes / ssTot
  const bTerm = Math.abs(b) < 1e-9 ? '' : ` ${b < 0 ? '-' : '+'} ${fmt(Math.abs(b))}`
  return { latex: `y = ${fmt(a)}\\,x${bTerm} \\qquad R^2 = ${fmt(r2)}`, error: null }
}
