// ── Excel Engineering functions, part 2 ──────────────────────────────────────
// Adds the Bessel family (BESSELJ/Y/I/K) and the transcendental complex-number
// functions (IMEXP, IMLN, IMLOG10/2, IMPOWER, IMSQRT, the trig and hyperbolic
// IM* functions). All functions are registered into ENGINEERING2_FNS.
//
// The Bessel implementations are ports of the classic Numerical Recipes routines
// (bessj0/bessj1/bessj, bessy0/bessy1/bessy, bessi0/bessi1/bessi, bessk0/bessk1/
// bessk). The complex-string parse/format helpers replicate the conventions of
// engineering.ts so values produced here compose with the existing IM* family.

import {
  type Fn,
  type Evaluator,
  type Node,
  ERR,
  isErr,
  num,
  str,
} from '../formula-engine'

// ── Complex helpers (replicated from engineering.ts to stay self-contained) ────

type Complex = { re: number; im: number; suffix: 'i' | 'j' }

/** Parse an Excel complex string like "3+4i", "-2.5j", "5", "i", "-i". */
function parseComplex(s: string): Complex | typeof ERR.NUM {
  const raw = s.trim()
  if (raw === '') return { re: 0, im: 0, suffix: 'i' }
  const suffix: 'i' | 'j' = raw.includes('j') ? 'j' : 'i'
  // Pure real number.
  if (!raw.includes('i') && !raw.includes('j')) {
    const n = Number(raw)
    return isNaN(n) ? ERR.NUM : { re: n, im: 0, suffix }
  }
  // Strip the suffix then split into real + imaginary parts.
  const body = raw.replace(/[ij]/g, '')
  // Pure imaginary shortcuts: "i", "-i", "+i".
  if (body === '' || body === '+') return { re: 0, im: 1, suffix }
  if (body === '-') return { re: 0, im: -1, suffix }
  // Find the split point: a +/- that is not at position 0 and not an exponent sign.
  let splitAt = -1
  for (let k = body.length - 1; k > 0; k--) {
    const c = body[k]
    if ((c === '+' || c === '-') && body[k - 1] !== 'e' && body[k - 1] !== 'E') {
      splitAt = k
      break
    }
  }
  if (splitAt === -1) {
    // Only an imaginary part, e.g. "4i" → body "4", or "-3i" handled here too.
    let imStr = body
    if (imStr === '+' || imStr === '') imStr = '1'
    else if (imStr === '-') imStr = '-1'
    const im = Number(imStr)
    return isNaN(im) ? ERR.NUM : { re: 0, im, suffix }
  }
  const reStr = body.slice(0, splitAt)
  let imStr = body.slice(splitAt)
  if (imStr === '+' || imStr === '') imStr = '1'
  else if (imStr === '-') imStr = '-1'
  const re = Number(reStr)
  const im = Number(imStr)
  return isNaN(re) || isNaN(im) ? ERR.NUM : { re, im, suffix }
}

/** Format a complex number back into Excel's "a+bi" string form. */
function fmtComplex(re: number, im: number, suffix: 'i' | 'j'): string {
  const r = Math.round(re * 1e12) / 1e12
  const i = Math.round(im * 1e12) / 1e12
  if (i === 0) return String(r)
  const imPart = i === 1 ? suffix : i === -1 ? `-${suffix}` : `${i}${suffix}`
  if (r === 0) return imPart
  const sign = i < 0 ? '' : '+'
  return `${r}${sign}${imPart}`
}

/** Parse a single complex argument from a formula node. */
function getComplex(ev: Evaluator, n: Node): Complex | typeof ERR.NUM {
  return parseComplex(str(ev, n))
}

// ── Bessel functions (Numerical Recipes ports, real argument) ──────────────────

/** Bessel function of the first kind of order 0, J0(x). */
function bessj0(x: number): number {
  const ax = Math.abs(x)
  if (ax < 8.0) {
    const y = x * x
    const p1 =
      -2957821389.0 +
      y * (7062834065.0 + y * (-512359803.6 + y * (10879881.29 + y * (-86327.92757 + y * 228.4622733))))
    const p2 =
      40076544269.0 +
      y * (745249964.8 + y * (7189466.438 + y * (47447.2647 + y * (226.1030244 + y))))
    return p1 / p2
  }
  const z = 8.0 / ax
  const y = z * z
  const xx = ax - 0.785398164
  const p1 =
    1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)))
  const p2 =
    -0.1562499995e-1 +
    y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 + y * -0.934935152e-7)))
  return Math.sqrt(0.636619772 / ax) * (Math.cos(xx) * p1 - z * Math.sin(xx) * p2)
}

/** Bessel function of the first kind of order 1, J1(x). */
function bessj1(x: number): number {
  const ax = Math.abs(x)
  if (ax < 8.0) {
    const y = x * x
    const p1 =
      x *
      (72362614232.0 +
        y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * -30.16036606)))))
    const p2 =
      144725228442.0 +
      y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y))))
    return p1 / p2
  }
  const z = 8.0 / ax
  const y = z * z
  const xx = ax - 2.356194491
  const p1 =
    1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * -0.240337019e-6)))
  const p2 =
    0.04687499995 +
    y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)))
  const ans = Math.sqrt(0.636619772 / ax) * (Math.cos(xx) * p1 - z * Math.sin(xx) * p2)
  return x < 0.0 ? -ans : ans
}

/** Bessel function of the first kind of integer order n, Jn(x). */
function bessj(n: number, x: number): number {
  if (n === 0) return bessj0(x)
  if (n === 1) return bessj1(x)
  const ACC = 40.0
  const BIGNO = 1.0e10
  const BIGNI = 1.0e-10
  const ax = Math.abs(x)
  if (ax === 0.0) return 0.0
  let ans: number
  if (ax > n) {
    // Upward recurrence is stable when x > n.
    const tox = 2.0 / ax
    let bjm = bessj0(ax)
    let bj = bessj1(ax)
    for (let j = 1; j < n; j++) {
      const bjp = j * tox * bj - bjm
      bjm = bj
      bj = bjp
    }
    ans = bj
  } else {
    // Downward recurrence (Miller's algorithm) for x <= n.
    const tox = 2.0 / ax
    const m = 2 * Math.floor((n + Math.floor(Math.sqrt(ACC * n))) / 2)
    let jsum = false
    let bjp = 0.0
    let bj = 1.0
    let bjm = 0.0
    let sum = 0.0
    ans = 0.0
    for (let j = m; j > 0; j--) {
      bjm = j * tox * bj - bjp
      bjp = bj
      bj = bjm
      if (Math.abs(bj) > BIGNO) {
        bj *= BIGNI
        bjp *= BIGNI
        ans *= BIGNI
        sum *= BIGNI
      }
      if (jsum) sum += bj
      jsum = !jsum
      if (j === n) ans = bjp
    }
    sum = 2.0 * sum - bj
    ans /= sum
  }
  return x < 0.0 && n % 2 === 1 ? -ans : ans
}

/** Bessel function of the second kind of order 0, Y0(x). */
function bessy0(x: number): number {
  if (x < 8.0) {
    const y = x * x
    const p1 =
      -2957821389.0 +
      y * (7062834065.0 + y * (-512359803.6 + y * (10879881.29 + y * (-86327.92757 + y * 228.4622733))))
    const p2 =
      40076544269.0 +
      y * (745249964.8 + y * (7189466.438 + y * (47447.26470 + y * (226.1030244 + y))))
    return p1 / p2 + 0.636619772 * bessj0(x) * Math.log(x)
  }
  const z = 8.0 / x
  const y = z * z
  const xx = x - 0.785398164
  const p1 =
    1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)))
  const p2 =
    -0.1562499995e-1 +
    y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 + y * -0.934945152e-7)))
  return Math.sqrt(0.636619772 / x) * (Math.sin(xx) * p1 + z * Math.cos(xx) * p2)
}

/** Bessel function of the second kind of order 1, Y1(x). */
function bessy1(x: number): number {
  if (x < 8.0) {
    const y = x * x
    const p1 =
      x *
      (-4.900604943e13 +
        y *
          (1.275274390e13 +
            y *
              (-5.153438139e11 +
                y * (7.349264551e9 + y * (-4.237922726e7 + y * 8.511937935e4)))))
    const p2 =
      2.499580570e14 +
      y *
        (4.244419664e12 +
          y *
            (3.733650367e10 +
              y * (2.245904002e8 + y * (1.020426050e6 + y * (3.549632885e3 + y)))))
    return p1 / p2 + 0.636619772 * (bessj1(x) * Math.log(x) - 1.0 / x)
  }
  const z = 8.0 / x
  const y = z * z
  const xx = x - 2.356194491
  const p1 =
    1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * -0.240337019e-6)))
  const p2 =
    0.04687499995 +
    y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)))
  return Math.sqrt(0.636619772 / x) * (Math.sin(xx) * p1 + z * Math.cos(xx) * p2)
}

/** Bessel function of the second kind of integer order n, Yn(x). */
function bessy(n: number, x: number): number {
  if (n === 0) return bessy0(x)
  if (n === 1) return bessy1(x)
  // Upward recurrence is always stable for Y.
  const tox = 2.0 / x
  let by = bessy1(x)
  let bym = bessy0(x)
  for (let j = 1; j < n; j++) {
    const byp = j * tox * by - bym
    bym = by
    by = byp
  }
  return by
}

/** Modified Bessel function of the first kind of order 0, I0(x). */
function bessi0(x: number): number {
  const ax = Math.abs(x)
  if (ax < 3.75) {
    let y = x / 3.75
    y *= y
    return (
      1.0 +
      y *
        (3.5156229 +
          y * (3.0899424 + y * (1.2067492 + y * (0.2659732 + y * (0.360768e-1 + y * 0.45813e-2)))))
    )
  }
  const y = 3.75 / ax
  return (
    (Math.exp(ax) / Math.sqrt(ax)) *
    (0.39894228 +
      y *
        (0.1328592e-1 +
          y *
            (0.225319e-2 +
              y *
                (-0.157565e-2 +
                  y *
                    (0.916281e-2 +
                      y *
                        (-0.2057706e-1 +
                          y * (0.2635537e-1 + y * (-0.1647633e-1 + y * 0.392377e-2))))))))
  )
}

/** Modified Bessel function of the first kind of order 1, I1(x). */
function bessi1(x: number): number {
  const ax = Math.abs(x)
  let ans: number
  if (ax < 3.75) {
    let y = x / 3.75
    y *= y
    ans =
      ax *
      (0.5 +
        y *
          (0.87890594 +
            y * (0.51498869 + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3))))))
  } else {
    const y = 3.75 / ax
    ans =
      0.2282967e-1 + y * (-0.2895312e-1 + y * (0.1787654e-1 - y * 0.420059e-2))
    ans =
      0.39894228 +
      y * (-0.3988024e-1 + y * (-0.362018e-2 + y * (0.163801e-2 + y * (-0.1031555e-1 + y * ans))))
    ans *= Math.exp(ax) / Math.sqrt(ax)
  }
  return x < 0.0 ? -ans : ans
}

/** Modified Bessel function of the first kind of integer order n, In(x). */
function bessi(n: number, x: number): number {
  if (n === 0) return bessi0(x)
  if (n === 1) return bessi1(x)
  if (x === 0.0) return 0.0
  const ACC = 40.0
  const BIGNO = 1.0e10
  const BIGNI = 1.0e-10
  const tox = 2.0 / Math.abs(x)
  let bip = 0.0
  let bi = 1.0
  let ans = 0.0
  const m = 2 * (n + Math.floor(Math.sqrt(ACC * n)))
  for (let j = m; j > 0; j--) {
    const bim = bip + j * tox * bi
    bip = bi
    bi = bim
    if (Math.abs(bi) > BIGNO) {
      ans *= BIGNI
      bi *= BIGNI
      bip *= BIGNI
    }
    if (j === n) ans = bip
  }
  ans *= bessi0(x) / bi
  return x < 0.0 && n % 2 === 1 ? -ans : ans
}

/** Modified Bessel function of the second kind of order 0, K0(x). */
function bessk0(x: number): number {
  if (x <= 2.0) {
    const y = (x * x) / 4.0
    return (
      -Math.log(x / 2.0) * bessi0(x) +
      (-0.57721566 +
        y *
          (0.42278420 +
            y *
              (0.23069756 +
                y * (0.3488590e-1 + y * (0.262698e-2 + y * (0.10750e-3 + y * 0.74e-5))))))
    )
  }
  const y = 2.0 / x
  return (
    (Math.exp(-x) / Math.sqrt(x)) *
    (1.25331414 +
      y *
        (-0.7832358e-1 +
          y *
            (0.2189568e-1 +
              y * (-0.1062446e-1 + y * (0.587872e-2 + y * (-0.251540e-2 + y * 0.53208e-3))))))
  )
}

/** Modified Bessel function of the second kind of order 1, K1(x). */
function bessk1(x: number): number {
  if (x <= 2.0) {
    const y = (x * x) / 4.0
    return (
      Math.log(x / 2.0) * bessi1(x) +
      (1.0 / x) *
        (1.0 +
          y *
            (0.15443144 +
              y *
                (-0.67278579 +
                  y * (-0.18156897 + y * (-0.1919402e-1 + y * (-0.110404e-2 + y * -0.4686e-4))))))
    )
  }
  const y = 2.0 / x
  return (
    (Math.exp(-x) / Math.sqrt(x)) *
    (1.25331414 +
      y *
        (0.23498619 +
          y *
            (-0.3655620e-1 +
              y * (0.1504268e-1 + y * (-0.780353e-2 + y * (0.325614e-2 + y * -0.68245e-3))))))
  )
}

/** Modified Bessel function of the second kind of integer order n, Kn(x). */
function bessk(n: number, x: number): number {
  if (n === 0) return bessk0(x)
  if (n === 1) return bessk1(x)
  // Upward recurrence is always stable for K.
  const tox = 2.0 / x
  let bkm = bessk0(x)
  let bk = bessk1(x)
  for (let j = 1; j < n; j++) {
    const bkp = bkm + j * tox * bk
    bkm = bk
    bk = bkp
  }
  return bk
}

/** Read (x, n) Bessel arguments; n is truncated to a non-negative integer. */
function besselArgs(
  ev: Evaluator,
  args: Node[],
): { x: number; n: number } | typeof ERR.NUM | typeof ERR.VALUE {
  const x = num(ev, args[0])
  if (isErr(x)) return ERR.VALUE
  const nv = num(ev, args[1])
  if (isErr(nv)) return ERR.VALUE
  if (!isFinite(x) || !isFinite(nv)) return ERR.NUM
  const n = Math.trunc(nv)
  if (n < 0) return ERR.NUM
  return { x, n }
}

// ── Complex transcendental functions ──────────────────────────────────────────

/** Divide complex `a` by complex `b`. Returns ERR.NUM on division by zero. */
function cdiv(a: Complex, b: Complex): Complex | typeof ERR.NUM {
  const den = b.re * b.re + b.im * b.im
  if (den === 0) return ERR.NUM
  return {
    re: (a.re * b.re + a.im * b.im) / den,
    im: (a.im * b.re - a.re * b.im) / den,
    suffix: a.suffix,
  }
}

/** Reciprocal of a complex number, used by CSC/SEC/COT/CSCH/SECH. */
function recip(c: Complex): Complex | typeof ERR.NUM {
  return cdiv({ re: 1, im: 0, suffix: c.suffix }, c)
}

// exp(a+bi) = e^a (cos b + i sin b)
function cexp(c: Complex): Complex {
  const e = Math.exp(c.re)
  return { re: e * Math.cos(c.im), im: e * Math.sin(c.im), suffix: c.suffix }
}

// sin(a+bi) = sin a cosh b + i cos a sinh b
function csin(c: Complex): Complex {
  return {
    re: Math.sin(c.re) * Math.cosh(c.im),
    im: Math.cos(c.re) * Math.sinh(c.im),
    suffix: c.suffix,
  }
}

// cos(a+bi) = cos a cosh b - i sin a sinh b
function ccos(c: Complex): Complex {
  return {
    re: Math.cos(c.re) * Math.cosh(c.im),
    im: -Math.sin(c.re) * Math.sinh(c.im),
    suffix: c.suffix,
  }
}

// sinh(a+bi) = sinh a cos b + i cosh a sin b
function csinh(c: Complex): Complex {
  return {
    re: Math.sinh(c.re) * Math.cos(c.im),
    im: Math.cosh(c.re) * Math.sin(c.im),
    suffix: c.suffix,
  }
}

// cosh(a+bi) = cosh a cos b + i sinh a sin b
function ccosh(c: Complex): Complex {
  return {
    re: Math.cosh(c.re) * Math.cos(c.im),
    im: Math.sinh(c.re) * Math.sin(c.im),
    suffix: c.suffix,
  }
}

/** Build a unary complex function: string in → string out, FErr-propagating. */
function imUnary(fn: (c: Complex) => Complex | typeof ERR.NUM): Fn {
  return (ev, a) => {
    const c = getComplex(ev, a[0])
    if (isErr(c)) return c
    const r = fn(c)
    if (isErr(r)) return r
    return fmtComplex(r.re, r.im, c.suffix)
  }
}

// ── Registry ──────────────────────────────────────────────────────────────────

export const ENGINEERING2_FNS: Record<string, Fn> = {
  // ── Bessel ──
  BESSELJ: (ev, a) => {
    const r = besselArgs(ev, a)
    if (isErr(r)) return r
    return bessj(r.n, r.x)
  },
  BESSELY: (ev, a) => {
    const r = besselArgs(ev, a)
    if (isErr(r)) return r
    if (r.x <= 0) return ERR.NUM // Y_n diverges at/below 0
    return bessy(r.n, r.x)
  },
  BESSELI: (ev, a) => {
    const r = besselArgs(ev, a)
    if (isErr(r)) return r
    return bessi(r.n, r.x)
  },
  BESSELK: (ev, a) => {
    const r = besselArgs(ev, a)
    if (isErr(r)) return r
    if (r.x <= 0) return ERR.NUM // K_n diverges at/below 0
    return bessk(r.n, r.x)
  },

  // ── Complex exponential / logarithms ──
  IMEXP: imUnary(cexp),
  IMLN: imUnary((c) => {
    const mod = Math.sqrt(c.re * c.re + c.im * c.im)
    if (mod === 0) return ERR.NUM
    return { re: Math.log(mod), im: Math.atan2(c.im, c.re), suffix: c.suffix }
  }),
  IMLOG10: imUnary((c) => {
    const mod = Math.sqrt(c.re * c.re + c.im * c.im)
    if (mod === 0) return ERR.NUM
    const ln10 = Math.LN10
    return { re: Math.log(mod) / ln10, im: Math.atan2(c.im, c.re) / ln10, suffix: c.suffix }
  }),
  IMLOG2: imUnary((c) => {
    const mod = Math.sqrt(c.re * c.re + c.im * c.im)
    if (mod === 0) return ERR.NUM
    const ln2 = Math.LN2
    return { re: Math.log(mod) / ln2, im: Math.atan2(c.im, c.re) / ln2, suffix: c.suffix }
  }),
  // IMPOWER(inumber, number): z^p = exp(p · ln z) via polar form.
  IMPOWER: (ev, a) => {
    const c = getComplex(ev, a[0])
    if (isErr(c)) return c
    const p = num(ev, a[1])
    if (isErr(p)) return p
    const mod = Math.sqrt(c.re * c.re + c.im * c.im)
    if (mod === 0) {
      // 0^p: defined as 0 for p > 0, 1 for p == 0, error otherwise.
      if (p > 0) return fmtComplex(0, 0, c.suffix)
      if (p === 0) return fmtComplex(1, 0, c.suffix)
      return ERR.NUM
    }
    const arg = Math.atan2(c.im, c.re)
    const rp = Math.pow(mod, p)
    return fmtComplex(rp * Math.cos(p * arg), rp * Math.sin(p * arg), c.suffix)
  },
  IMSQRT: imUnary((c) => {
    const mod = Math.sqrt(c.re * c.re + c.im * c.im)
    const r = Math.sqrt(mod)
    const halfArg = Math.atan2(c.im, c.re) / 2
    return { re: r * Math.cos(halfArg), im: r * Math.sin(halfArg), suffix: c.suffix }
  }),

  // ── Complex trigonometric ──
  IMSIN: imUnary(csin),
  IMCOS: imUnary(ccos),
  IMTAN: imUnary((c) => cdiv(csin(c), ccos(c))), // sin/cos
  IMCSC: imUnary((c) => recip(csin(c))), // 1/sin
  IMSEC: imUnary((c) => recip(ccos(c))), // 1/cos
  IMCOT: imUnary((c) => cdiv(ccos(c), csin(c))), // cos/sin

  // ── Complex hyperbolic ──
  IMSINH: imUnary(csinh),
  IMCOSH: imUnary(ccosh),
  IMCSCH: imUnary((c) => recip(csinh(c))), // 1/sinh
  IMSECH: imUnary((c) => recip(ccosh(c))), // 1/cosh
}
