// ── Financial functions for the spreadsheet formula engine ───────────────────
// Excel-compatible financial functions (annuities, NPV/IRR families, depreciation,
// rate conversions, T-bills, simple-interest securities).
//
// Conventions:
//   - Propagate FErr from arguments unchanged.
//   - Out-of-domain inputs (e.g. nper<=0, negative rate where forbidden) → ERR.NUM.
//   - Annuity sign convention follows Excel: cash you receive is positive, cash
//     you pay out is negative. PMT is typically negative for a positive PV loan.
//   - Day-count for simple-interest securities (ACCRINTM/DISC/INTRATE/RECEIVED and
//     T-bills) uses actual/360 — Excel's basis=2 — to stay library-free. The real
//     Excel default basis (0 = US 30/360) and the full basis selector are NOT
//     modelled; the optional `basis` argument is accepted but ignored.
//   - XIRR/XNPV use actual/365 (year fraction = days / 365) per Excel.
//
// Omitted (too complex without a finance/day-count library — see note at bottom):
//   PRICE, PRICEDISC, PRICEMAT, YIELD, YIELDDISC, YIELDMAT, DURATION, MDURATION,
//   COUPDAYBS, COUPDAYS, COUPDAYSNC, COUPNCD, COUPNUM, COUPPCD, ACCRINT (periodic),
//   ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD, AMORDEGRC, AMORLINC.

import {
  type Fn,
  type Evaluator,
  type Node,
  type Value,
  ERR,
  isErr,
  num,
  flatten,
  toNum,
  serialToDate,
} from '../formula-engine'

// ── Local helpers ────────────────────────────────────────────────────────────

type FErr = typeof ERR.NUM

const isFErr = (v: unknown): v is FErr => isErr(v)

// Read an optional numeric argument, defaulting when absent.
function optNum(ev: Evaluator, args: Node[], i: number, def: number): number | FErr {
  if (i >= args.length) return def
  return num(ev, args[i])
}

// Collect a flat list of numbers from one or more arguments (ranges or scalars),
// ignoring blanks/text — used by NPV/IRR/MIRR value lists. Propagates FErr.
function numList(ev: Evaluator, args: Node[]): number[] | FErr {
  const out: number[] = []
  for (const arg of args) {
    for (const s of flatten(ev.eval(arg))) {
      if (isErr(s)) return s
      // Keep numbers/booleans; coerce numeric strings via toNum; skip blanks/text.
      if (typeof s === 'number') out.push(s)
      else if (typeof s === 'boolean') out.push(s ? 1 : 0)
      else if (typeof s === 'string' && s !== '' && !isNaN(Number(s))) {
        const n = toNum(s)
        if (isErr(n)) return n
        out.push(n)
      }
    }
  }
  return out
}

// ── Core annuity primitives (pure math, no FErr) ─────────────────────────────

// Future value of an annuity. type: 0 = end of period, 1 = beginning.
function fvCore(rate: number, nper: number, pmt: number, pv: number, type: number): number {
  if (rate === 0) return -(pv + pmt * nper)
  const f = Math.pow(1 + rate, nper)
  return -(pv * f + pmt * (1 + rate * type) * (f - 1) / rate)
}

// Present value of an annuity.
function pvCore(rate: number, nper: number, pmt: number, fv: number, type: number): number {
  if (rate === 0) return -(fv + pmt * nper)
  const f = Math.pow(1 + rate, nper)
  return -(fv + pmt * (1 + rate * type) * (f - 1) / rate) / f
}

// Payment per period of an annuity.
function pmtCore(rate: number, nper: number, pv: number, fv: number, type: number): number {
  if (rate === 0) return -(pv + fv) / nper
  const f = Math.pow(1 + rate, nper)
  return -(pv * f + fv) * rate / ((1 + rate * type) * (f - 1))
}

// Interest part of a payment for a given period (1-based per).
function ipmtCore(rate: number, per: number, nper: number, pv: number, fv: number, type: number): number {
  const pmt = pmtCore(rate, nper, pv, fv, type)
  // Outstanding balance just before period `per`.
  let interest: number
  if (per === 1) {
    interest = type === 1 ? 0 : -pv * rate
  } else {
    // Balance after (per-1) payments, valued at start of period `per`.
    const fvPrev = fvCore(rate, per - 1, pmt, pv, type)
    interest = type === 1 ? -fvPrev * rate / (1 + rate) : -fvPrev * rate
  }
  return interest
}

// ── Annuity functions ────────────────────────────────────────────────────────

const PMT: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const nper = num(ev, a[1]); if (isFErr(nper)) return nper
  const pv = num(ev, a[2]); if (isFErr(pv)) return pv
  const fv = optNum(ev, a, 3, 0); if (isFErr(fv)) return fv
  const type = optNum(ev, a, 4, 0); if (isFErr(type)) return type
  if (nper === 0) return ERR.NUM
  return pmtCore(rate, nper, pv, fv, type)
}

const FV: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const nper = num(ev, a[1]); if (isFErr(nper)) return nper
  const pmt = num(ev, a[2]); if (isFErr(pmt)) return pmt
  const pv = optNum(ev, a, 3, 0); if (isFErr(pv)) return pv
  const type = optNum(ev, a, 4, 0); if (isFErr(type)) return type
  return fvCore(rate, nper, pmt, pv, type)
}

const PV: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const nper = num(ev, a[1]); if (isFErr(nper)) return nper
  const pmt = num(ev, a[2]); if (isFErr(pmt)) return pmt
  const fv = optNum(ev, a, 3, 0); if (isFErr(fv)) return fv
  const type = optNum(ev, a, 4, 0); if (isFErr(type)) return type
  return pvCore(rate, nper, pmt, fv, type)
}

const NPER: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const pmt = num(ev, a[1]); if (isFErr(pmt)) return pmt
  const pv = num(ev, a[2]); if (isFErr(pv)) return pv
  const fv = optNum(ev, a, 3, 0); if (isFErr(fv)) return fv
  const type = optNum(ev, a, 4, 0); if (isFErr(type)) return type
  if (rate === 0) {
    if (pmt === 0) return ERR.NUM
    return -(pv + fv) / pmt
  }
  const adj = pmt * (1 + rate * type) / rate
  const arg = (adj - fv) / (pv + adj)
  if (!(arg > 0)) return ERR.NUM
  return Math.log(arg) / Math.log(1 + rate)
}

const RATE: Fn = (ev, a) => {
  const nper = num(ev, a[0]); if (isFErr(nper)) return nper
  const pmt = num(ev, a[1]); if (isFErr(pmt)) return pmt
  const pv = num(ev, a[2]); if (isFErr(pv)) return pv
  const fv = optNum(ev, a, 3, 0); if (isFErr(fv)) return fv
  const type = optNum(ev, a, 4, 0); if (isFErr(type)) return type
  let guess = optNum(ev, a, 5, 0.1); if (isFErr(guess)) return guess
  // Newton's method on f(rate) = FV(rate,...) + fv ; objective: balance closes.
  const f = (r: number) => fvCore(r, nper, pmt, pv, type) - fv
  let r = guess
  for (let i = 0; i < 100; i++) {
    const fr = f(r)
    if (Math.abs(fr) < 1e-8) return r
    // Numerical derivative.
    const dr = 1e-6
    const dfr = (f(r + dr) - f(r - dr)) / (2 * dr)
    if (dfr === 0) break
    const next = r - fr / dfr
    if (!isFinite(next)) break
    if (Math.abs(next - r) < 1e-10) { r = next; if (Math.abs(f(r)) < 1e-6) return r; break }
    r = next
  }
  return Math.abs(f(r)) < 1e-6 ? r : ERR.NUM
}

const IPMT: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const per = num(ev, a[1]); if (isFErr(per)) return per
  const nper = num(ev, a[2]); if (isFErr(nper)) return nper
  const pv = num(ev, a[3]); if (isFErr(pv)) return pv
  const fv = optNum(ev, a, 4, 0); if (isFErr(fv)) return fv
  const type = optNum(ev, a, 5, 0); if (isFErr(type)) return type
  if (per < 1 || per > nper) return ERR.NUM
  return ipmtCore(rate, per, nper, pv, fv, type)
}

const PPMT: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const per = num(ev, a[1]); if (isFErr(per)) return per
  const nper = num(ev, a[2]); if (isFErr(nper)) return nper
  const pv = num(ev, a[3]); if (isFErr(pv)) return pv
  const fv = optNum(ev, a, 4, 0); if (isFErr(fv)) return fv
  const type = optNum(ev, a, 5, 0); if (isFErr(type)) return type
  if (per < 1 || per > nper) return ERR.NUM
  const pmt = pmtCore(rate, nper, pv, fv, type)
  const ipmt = ipmtCore(rate, per, nper, pv, fv, type)
  return pmt - ipmt
}

const CUMIPMT: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const nper = num(ev, a[1]); if (isFErr(nper)) return nper
  const pv = num(ev, a[2]); if (isFErr(pv)) return pv
  const start = num(ev, a[3]); if (isFErr(start)) return start
  const end = num(ev, a[4]); if (isFErr(end)) return end
  const type = optNum(ev, a, 5, 0); if (isFErr(type)) return type
  if (rate <= 0 || nper <= 0 || pv <= 0 || start < 1 || end < start || end > nper) return ERR.NUM
  let total = 0
  for (let p = Math.floor(start); p <= Math.floor(end); p++) total += ipmtCore(rate, p, nper, pv, 0, type)
  return total
}

const CUMPRINC: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const nper = num(ev, a[1]); if (isFErr(nper)) return nper
  const pv = num(ev, a[2]); if (isFErr(pv)) return pv
  const start = num(ev, a[3]); if (isFErr(start)) return start
  const end = num(ev, a[4]); if (isFErr(end)) return end
  const type = optNum(ev, a, 5, 0); if (isFErr(type)) return type
  if (rate <= 0 || nper <= 0 || pv <= 0 || start < 1 || end < start || end > nper) return ERR.NUM
  const pmt = pmtCore(rate, nper, pv, 0, type)
  let total = 0
  for (let p = Math.floor(start); p <= Math.floor(end); p++) total += pmt - ipmtCore(rate, p, nper, pv, 0, type)
  return total
}

// ISPMT: interest on a straight-line principal-reduction loan for period `per`.
const ISPMT: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const per = num(ev, a[1]); if (isFErr(per)) return per
  const nper = num(ev, a[2]); if (isFErr(nper)) return nper
  const pv = num(ev, a[3]); if (isFErr(pv)) return pv
  if (nper === 0) return ERR.NUM
  return -pv * rate * (1 - per / nper)
}

// ── NPV / IRR family ─────────────────────────────────────────────────────────

const NPV: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const values = numList(ev, a.slice(1)); if (isFErr(values)) return values
  if (rate === -1) return ERR.DIV0
  let npv = 0
  for (let i = 0; i < values.length; i++) npv += values[i] / Math.pow(1 + rate, i + 1)
  return npv
}

// NPV at a given rate for IRR root-finding (period 0 = first cash flow).
function irrNpv(rate: number, cf: number[]): number {
  let s = 0
  for (let i = 0; i < cf.length; i++) s += cf[i] / Math.pow(1 + rate, i)
  return s
}

const IRR: Fn = (ev, a) => {
  const cf = numList(ev, [a[0]]); if (isFErr(cf)) return cf
  if (cf.length < 2) return ERR.NUM
  let guess = optNum(ev, a, 1, 0.1); if (isFErr(guess)) return guess
  // Newton's method first; fall back to bisection over a wide bracket.
  let r = guess
  for (let i = 0; i < 100; i++) {
    const f = irrNpv(r, cf)
    if (Math.abs(f) < 1e-7) return r
    const dr = 1e-6
    const d = (irrNpv(r + dr, cf) - irrNpv(r - dr, cf)) / (2 * dr)
    if (d === 0) break
    const next = r - f / d
    if (!isFinite(next) || next <= -1) break
    if (Math.abs(next - r) < 1e-10) { r = next; break }
    r = next
  }
  if (Math.abs(irrNpv(r, cf)) < 1e-6 && r > -1) return r
  // Bisection fallback.
  let lo = -0.9999, hi = 10
  let flo = irrNpv(lo, cf), fhi = irrNpv(hi, cf)
  if (flo * fhi > 0) return ERR.NUM
  for (let i = 0; i < 200; i++) {
    const mid = (lo + hi) / 2
    const fm = irrNpv(mid, cf)
    if (Math.abs(fm) < 1e-9) return mid
    if (flo * fm < 0) { hi = mid; fhi = fm } else { lo = mid; flo = fm }
  }
  return (lo + hi) / 2
}

const MIRR: Fn = (ev, a) => {
  const cf = numList(ev, [a[0]]); if (isFErr(cf)) return cf
  const financeRate = num(ev, a[1]); if (isFErr(financeRate)) return financeRate
  const reinvestRate = num(ev, a[2]); if (isFErr(reinvestRate)) return reinvestRate
  const n = cf.length
  if (n < 2) return ERR.NUM
  let pvNeg = 0, fvPos = 0
  for (let i = 0; i < n; i++) {
    if (cf[i] < 0) pvNeg += cf[i] / Math.pow(1 + financeRate, i)
    else fvPos += cf[i] * Math.pow(1 + reinvestRate, n - 1 - i)
  }
  if (pvNeg === 0 || fvPos === 0) return ERR.DIV0
  return Math.pow(-fvPos / pvNeg, 1 / (n - 1)) - 1
}

// XNPV / XIRR — irregular cash flows with explicit dates (Excel serials).
// Year fraction = (date - date0) / 365 (actual/365).
function readDatedFlows(ev: Evaluator, valuesNode: Node, datesNode: Node): { v: number[]; d: number[] } | FErr {
  const v = numList(ev, [valuesNode]); if (isFErr(v)) return v
  const draw = numList(ev, [datesNode]); if (isFErr(draw)) return draw
  if (v.length !== draw.length || v.length < 2) return ERR.NUM
  // Truncate fractional serials to whole days.
  const d = draw.map(x => Math.floor(x))
  return { v, d }
}

function xnpvCore(rate: number, v: number[], d: number[]): number {
  const d0 = d[0]
  let s = 0
  for (let i = 0; i < v.length; i++) s += v[i] / Math.pow(1 + rate, (d[i] - d0) / 365)
  return s
}

const XNPV: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const flows = readDatedFlows(ev, a[1], a[2]); if (isFErr(flows)) return flows
  if (rate <= -1) return ERR.NUM
  return xnpvCore(rate, flows.v, flows.d)
}

const XIRR: Fn = (ev, a) => {
  const flows = readDatedFlows(ev, a[0], a[1]); if (isFErr(flows)) return flows
  let guess = optNum(ev, a, 2, 0.1); if (isFErr(guess)) return guess
  const { v, d } = flows
  let r = guess
  for (let i = 0; i < 100; i++) {
    const f = xnpvCore(r, v, d)
    if (Math.abs(f) < 1e-7) return r
    const dr = 1e-6
    const der = (xnpvCore(r + dr, v, d) - xnpvCore(r - dr, v, d)) / (2 * dr)
    if (der === 0) break
    const next = r - f / der
    if (!isFinite(next) || next <= -1) break
    if (Math.abs(next - r) < 1e-10) { r = next; break }
    r = next
  }
  if (Math.abs(xnpvCore(r, v, d)) < 1e-6 && r > -1) return r
  // Bisection fallback.
  let lo = -0.9999, hi = 10
  let flo = xnpvCore(lo, v, d), fhi = xnpvCore(hi, v, d)
  if (flo * fhi > 0) return ERR.NUM
  for (let i = 0; i < 200; i++) {
    const mid = (lo + hi) / 2
    const fm = xnpvCore(mid, v, d)
    if (Math.abs(fm) < 1e-9) return mid
    if (flo * fm < 0) { hi = mid; fhi = fm } else { lo = mid; flo = fm }
  }
  return (lo + hi) / 2
}

// FVSCHEDULE: compound a principal over a series of period rates.
const FVSCHEDULE: Fn = (ev, a) => {
  const principal = num(ev, a[0]); if (isFErr(principal)) return principal
  const rates = numList(ev, a.slice(1)); if (isFErr(rates)) return rates
  let v = principal
  for (const r of rates) v *= 1 + r
  return v
}

// ── Depreciation ─────────────────────────────────────────────────────────────

const SLN: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isFErr(cost)) return cost
  const salvage = num(ev, a[1]); if (isFErr(salvage)) return salvage
  const life = num(ev, a[2]); if (isFErr(life)) return life
  if (life === 0) return ERR.DIV0
  return (cost - salvage) / life
}

const SYD: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isFErr(cost)) return cost
  const salvage = num(ev, a[1]); if (isFErr(salvage)) return salvage
  const life = num(ev, a[2]); if (isFErr(life)) return life
  const per = num(ev, a[3]); if (isFErr(per)) return per
  if (life <= 0 || per < 1 || per > life) return ERR.NUM
  return (cost - salvage) * (life - per + 1) * 2 / (life * (life + 1))
}

// DB: fixed-declining-balance depreciation. month defaults to 12.
const DB: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isFErr(cost)) return cost
  const salvage = num(ev, a[1]); if (isFErr(salvage)) return salvage
  const life = num(ev, a[2]); if (isFErr(life)) return life
  const period = num(ev, a[3]); if (isFErr(period)) return period
  const month = optNum(ev, a, 4, 12); if (isFErr(month)) return month
  if (life <= 0 || cost < 0 || period < 1 || month < 1 || month > 12) return ERR.NUM
  if (cost === 0) return 0
  // Excel rounds the rate to 3 decimals.
  const rate = Math.round((1 - Math.pow(salvage / cost, 1 / life)) * 1000) / 1000
  const firstYear = cost * rate * month / 12
  if (period === 1) return firstYear
  // Accumulate to the start of `period`.
  let total = firstYear
  let dep = firstYear
  for (let p = 2; p <= Math.floor(period); p++) {
    dep = (cost - total) * rate
    if (p === Math.floor(period)) {
      // Last (possibly partial) year handling: Excel applies (12-month)/12 in year life+1.
      if (p === life + 1) return (cost - total) * rate * (12 - month) / 12
      return dep
    }
    total += dep
  }
  return dep
}

// DDB: double-declining-balance. factor defaults to 2.
const DDB: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isFErr(cost)) return cost
  const salvage = num(ev, a[1]); if (isFErr(salvage)) return salvage
  const life = num(ev, a[2]); if (isFErr(life)) return life
  const period = num(ev, a[3]); if (isFErr(period)) return period
  const factor = optNum(ev, a, 4, 2); if (isFErr(factor)) return factor
  if (life <= 0 || period < 1 || period > life || factor <= 0) return ERR.NUM
  let total = 0
  let dep = 0
  for (let p = 1; p <= Math.floor(period); p++) {
    dep = Math.min((cost - total) * factor / life, Math.max(cost - salvage - total, 0))
    if (dep < 0) dep = 0
    total += dep
  }
  return dep
}

// VDB: variable-declining-balance from start_period to end_period.
const VDB: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isFErr(cost)) return cost
  const salvage = num(ev, a[1]); if (isFErr(salvage)) return salvage
  const life = num(ev, a[2]); if (isFErr(life)) return life
  const start = num(ev, a[3]); if (isFErr(start)) return start
  const end = num(ev, a[4]); if (isFErr(end)) return end
  const factor = optNum(ev, a, 5, 2); if (isFErr(factor)) return factor
  const noSwitch = optNum(ev, a, 6, 0); if (isFErr(noSwitch)) return noSwitch
  if (life <= 0 || start < 0 || end < start || end > life || factor <= 0) return ERR.NUM

  // Accumulate DDB period by period; optionally switch to straight-line when SL
  // on remaining book is larger (Excel's default unless no_switch is TRUE).
  const wholeDdb = (from: number, to: number): number => {
    let total = 0
    let bookLocal = cost
    let slDep = 0
    let switched = false
    const endI = Math.ceil(to)
    let sum = 0
    for (let p = 1; p <= endI; p++) {
      let dep: number
      const remaining = bookLocal - salvage
      const ddb = Math.min(bookLocal * factor / life, Math.max(remaining, 0))
      if (!toBoolish(noSwitch)) {
        const slPeriodsLeft = life - (p - 1)
        slDep = slPeriodsLeft > 0 ? remaining / slPeriodsLeft : 0
        if (switched || slDep > ddb) { switched = true; dep = Math.max(Math.min(slDep, remaining), 0) }
        else dep = ddb
      } else {
        dep = ddb
      }
      bookLocal -= dep
      total += dep
      // Fraction of this period inside [from, to].
      const periodStart = p - 1
      const periodEnd = p
      const lo = Math.max(periodStart, from)
      const hi = Math.min(periodEnd, to)
      if (hi > lo) sum += dep * (hi - lo)
    }
    return sum
  }
  return wholeDdb(start, end)
}

function toBoolish(n: number): boolean { return n !== 0 }

// ── Rate conversions & growth ────────────────────────────────────────────────

const EFFECT: Fn = (ev, a) => {
  const nominal = num(ev, a[0]); if (isFErr(nominal)) return nominal
  const npery = num(ev, a[1]); if (isFErr(npery)) return npery
  const n = Math.floor(npery)
  if (nominal <= 0 || n < 1) return ERR.NUM
  return Math.pow(1 + nominal / n, n) - 1
}

const NOMINAL: Fn = (ev, a) => {
  const effect = num(ev, a[0]); if (isFErr(effect)) return effect
  const npery = num(ev, a[1]); if (isFErr(npery)) return npery
  const n = Math.floor(npery)
  if (effect <= 0 || n < 1) return ERR.NUM
  return (Math.pow(1 + effect, 1 / n) - 1) * n
}

// RRI: equivalent interest rate for the growth of an investment.
const RRI: Fn = (ev, a) => {
  const nper = num(ev, a[0]); if (isFErr(nper)) return nper
  const pv = num(ev, a[1]); if (isFErr(pv)) return pv
  const fv = num(ev, a[2]); if (isFErr(fv)) return fv
  if (nper <= 0 || pv === 0) return ERR.NUM
  return Math.pow(fv / pv, 1 / nper) - 1
}

// PDURATION: periods required to reach a future value at a fixed rate.
const PDURATION: Fn = (ev, a) => {
  const rate = num(ev, a[0]); if (isFErr(rate)) return rate
  const pv = num(ev, a[1]); if (isFErr(pv)) return pv
  const fv = num(ev, a[2]); if (isFErr(fv)) return fv
  if (rate <= 0 || pv <= 0 || fv <= 0) return ERR.NUM
  return (Math.log(fv) - Math.log(pv)) / Math.log(1 + rate)
}

// ── Dollar fraction conversions ──────────────────────────────────────────────

const DOLLARDE: Fn = (ev, a) => {
  const fractional = num(ev, a[0]); if (isFErr(fractional)) return fractional
  const fraction = num(ev, a[1]); if (isFErr(fraction)) return fraction
  const f = Math.floor(fraction)
  if (f < 0) return ERR.NUM
  if (f === 0) return ERR.DIV0
  const intPart = Math.trunc(fractional)
  const frac = fractional - intPart
  // Number of digits in the fraction base, used to shift the fractional part.
  const digits = Math.ceil(Math.log10(f) || 1)
  return intPart + (frac * Math.pow(10, digits)) / f
}

const DOLLARFR: Fn = (ev, a) => {
  const decimal = num(ev, a[0]); if (isFErr(decimal)) return decimal
  const fraction = num(ev, a[1]); if (isFErr(fraction)) return fraction
  const f = Math.floor(fraction)
  if (f < 0) return ERR.NUM
  if (f === 0) return ERR.DIV0
  const intPart = Math.trunc(decimal)
  const frac = decimal - intPart
  const digits = Math.ceil(Math.log10(f) || 1)
  return intPart + (frac * f) / Math.pow(10, digits)
}

// ── Simple-interest securities (actual/360; basis argument ignored) ──────────
// daysActual/360 day count between two Excel serials.
function daysBetween(d1: number, d2: number): number | null {
  const a = serialToDate(Math.floor(d1)); const b = serialToDate(Math.floor(d2))
  if (!a || !b) return null
  return (b.getTime() - a.getTime()) / 86400000
}

// ACCRINTM: accrued interest for a security paying interest at maturity.
// accrued = par * rate * (days / 360)  [actual/360]
const ACCRINTM: Fn = (ev, a) => {
  const issue = num(ev, a[0]); if (isFErr(issue)) return issue
  const settlement = num(ev, a[1]); if (isFErr(settlement)) return settlement
  const rate = num(ev, a[2]); if (isFErr(rate)) return rate
  const par = num(ev, a[3]); if (isFErr(par)) return par
  // a[4] = basis (ignored; actual/360 assumed)
  if (rate <= 0 || par <= 0) return ERR.NUM
  const days = daysBetween(issue, settlement)
  if (days == null || days < 0) return ERR.NUM
  return par * rate * (days / 360)
}

// DISC: discount rate of a security. actual/360.
const DISC: Fn = (ev, a) => {
  const settlement = num(ev, a[0]); if (isFErr(settlement)) return settlement
  const maturity = num(ev, a[1]); if (isFErr(maturity)) return maturity
  const pr = num(ev, a[2]); if (isFErr(pr)) return pr
  const redemption = num(ev, a[3]); if (isFErr(redemption)) return redemption
  if (pr <= 0 || redemption <= 0) return ERR.NUM
  const days = daysBetween(settlement, maturity)
  if (days == null || days <= 0) return ERR.NUM
  return (redemption - pr) / redemption * (360 / days)
}

// INTRATE: interest rate of a fully invested security. actual/360.
const INTRATE: Fn = (ev, a) => {
  const settlement = num(ev, a[0]); if (isFErr(settlement)) return settlement
  const maturity = num(ev, a[1]); if (isFErr(maturity)) return maturity
  const investment = num(ev, a[2]); if (isFErr(investment)) return investment
  const redemption = num(ev, a[3]); if (isFErr(redemption)) return redemption
  if (investment <= 0 || redemption <= 0) return ERR.NUM
  const days = daysBetween(settlement, maturity)
  if (days == null || days <= 0) return ERR.NUM
  return (redemption - investment) / investment * (360 / days)
}

// RECEIVED: amount received at maturity for a fully invested security. actual/360.
const RECEIVED: Fn = (ev, a) => {
  const settlement = num(ev, a[0]); if (isFErr(settlement)) return settlement
  const maturity = num(ev, a[1]); if (isFErr(maturity)) return maturity
  const investment = num(ev, a[2]); if (isFErr(investment)) return investment
  const discount = num(ev, a[3]); if (isFErr(discount)) return discount
  if (investment <= 0 || discount <= 0) return ERR.NUM
  const days = daysBetween(settlement, maturity)
  if (days == null || days <= 0) return ERR.NUM
  const denom = 1 - discount * days / 360
  if (denom === 0) return ERR.DIV0
  return investment / denom
}

// ── Treasury bills (actual/360 basis as Excel defines them) ──────────────────

const TBILLEQ: Fn = (ev, a) => {
  const settlement = num(ev, a[0]); if (isFErr(settlement)) return settlement
  const maturity = num(ev, a[1]); if (isFErr(maturity)) return maturity
  const discount = num(ev, a[2]); if (isFErr(discount)) return discount
  if (discount <= 0) return ERR.NUM
  const days = daysBetween(settlement, maturity)
  if (days == null || days <= 0 || days > 365) return ERR.NUM
  // Bond-equivalent yield: (365 * discount) / (360 - discount * days)
  const denom = 360 - discount * days
  if (denom <= 0) return ERR.NUM
  return (365 * discount) / denom
}

const TBILLPRICE: Fn = (ev, a) => {
  const settlement = num(ev, a[0]); if (isFErr(settlement)) return settlement
  const maturity = num(ev, a[1]); if (isFErr(maturity)) return maturity
  const discount = num(ev, a[2]); if (isFErr(discount)) return discount
  if (discount <= 0) return ERR.NUM
  const days = daysBetween(settlement, maturity)
  if (days == null || days <= 0 || days > 365) return ERR.NUM
  // Price per $100 face value.
  return 100 * (1 - discount * days / 360)
}

const TBILLYIELD: Fn = (ev, a) => {
  const settlement = num(ev, a[0]); if (isFErr(settlement)) return settlement
  const maturity = num(ev, a[1]); if (isFErr(maturity)) return maturity
  const pr = num(ev, a[2]); if (isFErr(pr)) return pr
  if (pr <= 0) return ERR.NUM
  const days = daysBetween(settlement, maturity)
  if (days == null || days <= 0 || days > 365) return ERR.NUM
  return ((100 - pr) / pr) * (360 / days)
}

// ── Registry ─────────────────────────────────────────────────────────────────

export const FINANCIAL_FNS: Record<string, Fn> = {
  PMT,
  IPMT,
  PPMT,
  FV,
  PV,
  NPER,
  RATE,
  NPV,
  IRR,
  MIRR,
  XIRR,
  XNPV,
  FVSCHEDULE,
  SLN,
  SYD,
  DB,
  DDB,
  VDB,
  CUMIPMT,
  CUMPRINC,
  EFFECT,
  NOMINAL,
  RRI,
  PDURATION,
  ISPMT,
  DOLLARDE,
  DOLLARFR,
  ACCRINTM,
  DISC,
  INTRATE,
  RECEIVED,
  TBILLEQ,
  TBILLPRICE,
  TBILLYIELD,
}
