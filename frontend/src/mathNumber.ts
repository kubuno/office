// mathNumber — integer/number-theory helpers for the Maths module: prime factorisation,
// gcd/lcm, combinatorics (factorial, C(n,k), A(n,k)) and base conversion. Pure numeric, no
// dependency on the CAS. Each public *Latex function returns { latex, error }.
export interface NumResult { latex: string; error: string | null }

function gcd(a: number, b: number): number { a = Math.abs(a); b = Math.abs(b); while (b) { [a, b] = [b, a % b] } return a }

// Prime factorisation of a positive integer → LaTeX like  360 = 2^{3} \times 3^{2} \times 5.
export function primeFactorsLatex(n: number): NumResult {
  if (!Number.isInteger(n) || Math.abs(n) < 2) return { latex: '', error: 'Entrez un entier ≥ 2.' }
  let m = Math.abs(n)
  const factors: [number, number][] = []
  for (let p = 2; p * p <= m; p++) { if (m % p === 0) { let e = 0; while (m % p === 0) { m /= p; e++ } factors.push([p, e]) } }
  if (m > 1) factors.push([m, 1])
  const body = factors.map(([p, e]) => (e === 1 ? String(p) : `${p}^{${e}}`)).join(' \\times ')
  return { latex: `${n} = ${n < 0 ? '-' : ''}${body}`, error: null }
}

export function gcdLcmLatex(a: number, b: number): NumResult {
  if (!Number.isInteger(a) || !Number.isInteger(b)) return { latex: '', error: 'Entrez deux entiers.' }
  const g = gcd(a, b)
  const l = g === 0 ? 0 : Math.abs(a / g * b)
  return { latex: `\\gcd(${a},\\, ${b}) = ${g} \\qquad \\operatorname{ppcm}(${a},\\, ${b}) = ${l}`, error: null }
}

function factorial(n: number): number { let r = 1; for (let i = 2; i <= n; i++) r *= i; return r }
export function factorialLatex(n: number): NumResult {
  if (!Number.isInteger(n) || n < 0) return { latex: '', error: 'Entrez un entier ≥ 0.' }
  if (n > 170) return { latex: '', error: 'Trop grand (n ≤ 170).' }
  return { latex: `${n}! = ${factorial(n)}`, error: null }
}
// Binomial coefficient + arrangements: C(n,k) and A(n,k).
export function binomialLatex(n: number, k: number): NumResult {
  if (![n, k].every(Number.isInteger) || n < 0 || k < 0 || k > n) return { latex: '', error: 'Entrez des entiers 0 ≤ k ≤ n.' }
  let c = 1
  for (let i = 0; i < k; i++) c = (c * (n - i)) / (i + 1)
  const cVal = Math.round(c)
  const aVal = Math.round(c * factorial(k))
  return { latex: `\\binom{${n}}{${k}} = ${cVal} \\qquad A_{${n}}^{${k}} = ${aVal}`, error: null }
}

// Base conversion of a (possibly negative) integer to binary, octal and hexadecimal.
export function baseConvertLatex(n: number): NumResult {
  if (!Number.isInteger(n)) return { latex: '', error: 'Entrez un entier.' }
  const s = n < 0 ? '-' : ''
  const a = Math.abs(n)
  return { latex: `\\begin{aligned} ${n} &= (${s}${a.toString(2)})_2 \\\\ &= (${s}${a.toString(8)})_8 \\\\ &= (${s}\\text{${a.toString(16).toUpperCase()}})_{16} \\end{aligned}`, error: null }
}
