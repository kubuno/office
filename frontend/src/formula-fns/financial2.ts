// Financial functions — wave 2: bond pricing, coupon schedules, day-count bases,
// accrued interest, duration, odd-period bonds and French depreciation.
//
// This file is fully self-contained: it implements the complete Excel day-count
// basis selector (0..4) instead of the actual/360 shortcut used by the simpler
// securities in financial.ts. Every helper lives in this file.
//
// Functions: COUPNCD COUPPCD COUPDAYBS COUPDAYSNC COUPDAYS COUPNUM ACCRINT
//   PRICE YIELD PRICEDISC YIELDDISC PRICEMAT YIELDMAT DURATION MDURATION
//   ODDFPRICE ODDFYIELD ODDLPRICE ODDLYIELD AMORLINC AMORDEGRC
//
// ACCRINTM and DISC are NOT redefined here (they live in financial.ts).

import { type Fn, ERR, isErr, num, excelSerial, serialToDate } from '../formula-engine'
import type { FErr } from '../formula-engine'

// ── Date utilities ────────────────────────────────────────────────────────────

// Read argument i as an Excel serial, then to a calendar Date. Returns FErr/null.
function readDate(d: number | FErr): Date | null {
  if (isErr(d)) return null
  return serialToDate(Math.floor(d))
}

function isLeap(y: number): boolean {
  return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0
}

function daysInMonth(y: number, m: number): number {
  // m is 1-based.
  return [31, isLeap(y) ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m - 1]
}

function isLastDayOfMonth(d: Date): boolean {
  return d.getDate() === daysInMonth(d.getFullYear(), d.getMonth() + 1)
}

function isLastDayOfFeb(d: Date): boolean {
  return d.getMonth() === 1 && isLastDayOfMonth(d)
}

// Actual calendar days between two dates (end - start).
function actualDays(start: Date, end: Date): number {
  return Math.round((excelSerial(end) - excelSerial(start)) / 1)
}

// Add whole months to a Y/M/D, clamping the day to the target month length.
function addMonths(d: Date, months: number): Date {
  const total = d.getFullYear() * 12 + d.getMonth() + months
  const y = Math.floor(total / 12)
  const m = total % 12 // 0-based
  const day = Math.min(d.getDate(), daysInMonth(y, m + 1))
  return new Date(y, m, day)
}

// ── Day-count engine (the crux) ────────────────────────────────────────────────

// Number of days between start and end under the given basis. For 30/360 bases
// this implements Excel's end-of-month adjustment rules.
function dayCount(start: Date, end: Date, basis: number): number {
  let d1 = start.getDate()
  let d2 = end.getDate()
  const m1 = start.getMonth() + 1
  const m2 = end.getMonth() + 1
  const y1 = start.getFullYear()
  const y2 = end.getFullYear()

  switch (basis) {
    case 0: {
      // US (NASD) 30/360.
      // If the security's settlement/issue rules place the last day of February,
      // both dates are treated as the 30th of that month (Excel behaviour).
      if (isLastDayOfFeb(start) && isLastDayOfFeb(end)) d2 = 30
      if (isLastDayOfFeb(start)) d1 = 30
      if (d2 === 31 && (d1 === 30 || d1 === 31)) d2 = 30
      if (d1 === 31) d1 = 30
      return (y2 - y1) * 360 + (m2 - m1) * 30 + (d2 - d1)
    }
    case 4: {
      // European 30/360: a day of 31 is always changed to 30, no Feb special case.
      if (d1 === 31) d1 = 30
      if (d2 === 31) d2 = 30
      return (y2 - y1) * 360 + (m2 - m1) * 30 + (d2 - d1)
    }
    default:
      // Actual for bases 1, 2, 3.
      return actualDays(start, end)
  }
}

// Denominator (year length) used by yearFrac for bases that do not depend on the
// concrete period. For basis 1 (actual/actual) the caller handles averaging.
function basisDenominator(basis: number): number {
  switch (basis) {
    case 0: return 360
    case 2: return 360
    case 3: return 365
    case 4: return 360
    default: return 360
  }
}

// Excel YEARFRAC. Implements all five bases, including actual/actual averaging.
function yearFrac(start: Date, end: Date, basis: number): number | FErr {
  if (basis < 0 || basis > 4) return ERR.NUM
  if (excelSerial(start) === excelSerial(end)) return 0

  if (basis === 1) {
    // actual/actual — Excel averages the year length over the spanned calendar
    // years. The denominator is the average number of days per year across the
    // interval [y1 .. y2], counting Feb 29s.
    let a = start
    let b = end
    let sign = 1
    if (excelSerial(a) > excelSerial(b)) { const t = a; a = b; b = t; sign = -1 }
    const days = actualDays(a, b)
    const y1 = a.getFullYear()
    const y2 = b.getFullYear()
    let denom: number
    if (y1 === y2) {
      denom = isLeap(y1) ? 366 : 365
    } else {
      const years = y2 - y1 + 1
      let total = 0
      for (let y = y1; y <= y2; y++) total += isLeap(y) ? 366 : 365
      denom = total / years
    }
    return (sign * days) / denom
  }

  return dayCount(start, end, basis) / basisDenominator(basis)
}

// ── Coupon schedule ─────────────────────────────────────────────────────────────

// Index k (>= 0) of the previous coupon counted back from maturity: the largest
// k such that addMonths(maturity, -step*k) <= settlement. Anchoring every coupon
// date on the maturity day-of-month avoids drift from month-length clamping.
function prevCouponIndex(settlement: Date, maturity: Date, frequency: number): number {
  const step = 12 / frequency
  let k = 0
  while (excelSerial(addMonths(maturity, -step * (k + 1))) > excelSerial(settlement)) {
    k++
    if (k > 100000) break
  }
  return k + 1
}

// Previous coupon date on or before settlement.
function couponPrev(settlement: Date, maturity: Date, frequency: number): Date {
  const step = 12 / frequency
  return addMonths(maturity, -step * prevCouponIndex(settlement, maturity, frequency))
}

// Next coupon date strictly after settlement.
function couponNext(settlement: Date, maturity: Date, frequency: number): Date {
  const step = 12 / frequency
  return addMonths(maturity, -step * (prevCouponIndex(settlement, maturity, frequency) - 1))
}

function freqOk(f: number): boolean {
  return f === 1 || f === 2 || f === 4
}

function basisOk(b: number): boolean {
  return b >= 0 && b <= 4
}

// Days in the coupon period containing settlement.
function coupDays(settlement: Date, maturity: Date, frequency: number, basis: number): number {
  if (basis === 1) {
    // actual/actual: actual days in the current coupon period.
    const prev = couponPrev(settlement, maturity, frequency)
    const next = couponNext(settlement, maturity, frequency)
    return actualDays(prev, next)
  }
  if (basis === 3) return 365 / frequency
  // bases 0, 2, 4 → 360-day year basis.
  return 360 / frequency
}

// ── Argument helpers ────────────────────────────────────────────────────────────

interface DateArgs { settlement: Date; maturity: Date }

function readSettleMaturity(s: number | FErr, m: number | FErr): DateArgs | FErr {
  if (isErr(s)) return s
  if (isErr(m)) return m
  const settlement = readDate(s)
  const maturity = readDate(m)
  if (!settlement || !maturity) return ERR.VALUE
  if (excelSerial(settlement) >= excelSerial(maturity)) return ERR.NUM
  return { settlement, maturity }
}

// ── COUPON functions ────────────────────────────────────────────────────────────

const COUPNCD: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const f = num(ev, a[2]); if (isErr(f)) return f
  const b = a[3] ? num(ev, a[3]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  return excelSerial(couponNext(sm.settlement, sm.maturity, f))
}

const COUPPCD: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const f = num(ev, a[2]); if (isErr(f)) return f
  const b = a[3] ? num(ev, a[3]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  return excelSerial(couponPrev(sm.settlement, sm.maturity, f))
}

const COUPDAYBS: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const f = num(ev, a[2]); if (isErr(f)) return f
  const b = a[3] ? num(ev, a[3]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  const prev = couponPrev(sm.settlement, sm.maturity, f)
  return dayCount(prev, sm.settlement, b)
}

const COUPDAYSNC: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const f = num(ev, a[2]); if (isErr(f)) return f
  const b = a[3] ? num(ev, a[3]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  const next = couponNext(sm.settlement, sm.maturity, f)
  if (b === 0 || b === 4) {
    // 30/360 bases: NC = coupDays - DBS to keep the period internally consistent.
    const prev = couponPrev(sm.settlement, sm.maturity, f)
    return coupDays(sm.settlement, sm.maturity, f, b) - dayCount(prev, sm.settlement, b)
  }
  return dayCount(sm.settlement, next, b)
}

const COUPDAYS: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const f = num(ev, a[2]); if (isErr(f)) return f
  const b = a[3] ? num(ev, a[3]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  return coupDays(sm.settlement, sm.maturity, f, b)
}

const COUPNUM: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const f = num(ev, a[2]); if (isErr(f)) return f
  const b = a[3] ? num(ev, a[3]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  // Number of coupons between settlement and maturity, counting the one at maturity.
  const prev = couponPrev(sm.settlement, sm.maturity, f)
  const step = 12 / f
  // Count coupon dates in (prev, maturity].
  let count = 0
  let k = 1
  while (true) {
    const cand = addMonths(prev, step * k)
    if (excelSerial(cand) > excelSerial(sm.maturity)) break
    count++
    k++
    if (k > 100000) break
  }
  return count
}

// ── ACCRINT ─────────────────────────────────────────────────────────────────────

const ACCRINT: Fn = (ev, a) => {
  const issueN = num(ev, a[0]); if (isErr(issueN)) return issueN
  const firstN = num(ev, a[1]); if (isErr(firstN)) return firstN
  const settleN = num(ev, a[2]); if (isErr(settleN)) return settleN
  const rate = num(ev, a[3]); if (isErr(rate)) return rate
  const par = num(ev, a[4]); if (isErr(par)) return par
  const f = num(ev, a[5]); if (isErr(f)) return f
  const b = a[6] ? num(ev, a[6]) : 0; if (isErr(b)) return b
  const calcMethod = a[7] ? num(ev, a[7]) : 1; if (isErr(calcMethod)) return calcMethod

  const issue = readDate(issueN)
  const first = readDate(firstN)
  const settle = readDate(settleN)
  if (!issue || !first || !settle) return ERR.VALUE
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate <= 0 || par <= 0) return ERR.NUM
  if (excelSerial(issue) >= excelSerial(settle)) return ERR.NUM

  // When calc_method is TRUE (1, default) accrued interest is computed from issue
  // to settlement. When FALSE (0) and settlement > first_interest, it is computed
  // from first_interest to settlement.
  let accStart = issue
  if (calcMethod === 0 && excelSerial(settle) > excelSerial(first)) {
    accStart = first
  }
  const yf = yearFrac(accStart, settle, b)
  if (isErr(yf)) return yf
  return par * rate * yf
}

// ── PRICE / YIELD (periodic-coupon bonds) ───────────────────────────────────────

// Core PRICE computation. Returns price per 100 face value.
function priceCore(
  settlement: Date, maturity: Date, rate: number, yld: number,
  redemption: number, frequency: number, basis: number,
): number {
  const n = coupNumBetween(settlement, maturity, frequency)
  const dsc = coupDaysSettleNext(settlement, maturity, frequency, basis)
  const e = coupDays(settlement, maturity, frequency, basis)
  const a = e - dsc // days from previous coupon to settlement
  const couponAmt = (100 * rate) / frequency
  const t = dsc / e
  const yf = yld / frequency

  let price = redemption / Math.pow(1 + yf, n - 1 + t)
  for (let k = 1; k <= n; k++) {
    price += couponAmt / Math.pow(1 + yf, k - 1 + t)
  }
  price -= couponAmt * (a / e)
  return price
}

// Number of coupons payable between settlement and maturity (COUPNUM logic).
function coupNumBetween(settlement: Date, maturity: Date, frequency: number): number {
  const prev = couponPrev(settlement, maturity, frequency)
  const step = 12 / frequency
  let count = 0
  let k = 1
  while (true) {
    const cand = addMonths(prev, step * k)
    if (excelSerial(cand) > excelSerial(maturity)) break
    count++
    k++
    if (k > 100000) break
  }
  return count
}

// Days from settlement to next coupon (COUPDAYSNC logic).
function coupDaysSettleNext(settlement: Date, maturity: Date, frequency: number, basis: number): number {
  if (basis === 0 || basis === 4) {
    const prev = couponPrev(settlement, maturity, frequency)
    return coupDays(settlement, maturity, frequency, basis) - dayCount(prev, settlement, basis)
  }
  const next = couponNext(settlement, maturity, frequency)
  return dayCount(settlement, next, basis)
}

const PRICE: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const rate = num(ev, a[2]); if (isErr(rate)) return rate
  const yld = num(ev, a[3]); if (isErr(yld)) return yld
  const redemption = num(ev, a[4]); if (isErr(redemption)) return redemption
  const f = num(ev, a[5]); if (isErr(f)) return f
  const b = a[6] ? num(ev, a[6]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate < 0 || yld < 0 || redemption <= 0) return ERR.NUM
  return priceCore(sm.settlement, sm.maturity, rate, yld, redemption, f, b)
}

const YIELD: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const rate = num(ev, a[2]); if (isErr(rate)) return rate
  const pr = num(ev, a[3]); if (isErr(pr)) return pr
  const redemption = num(ev, a[4]); if (isErr(redemption)) return redemption
  const f = num(ev, a[5]); if (isErr(f)) return f
  const b = a[6] ? num(ev, a[6]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate < 0 || pr <= 0 || redemption <= 0) return ERR.NUM
  const fn = (y: number) => priceCore(sm.settlement, sm.maturity, rate, y, redemption, f, b) - pr
  return solveYield(fn)
}

// Robust root finder: bisection bracketing then refinement. Yields in (-0.999, 10).
function solveYield(fn: (y: number) => number): number | FErr {
  const lo0 = -0.999
  const hi0 = 10
  let lo = lo0
  let hi = hi0
  let flo = fn(lo)
  let fhi = fn(hi)
  if (!isFinite(flo) || !isFinite(fhi)) return ERR.NUM
  if (flo * fhi > 0) {
    // Scan for a sign change across the interval.
    let prevX = lo
    let prevF = flo
    let found = false
    const steps = 200
    for (let i = 1; i <= steps; i++) {
      const x = lo0 + ((hi0 - lo0) * i) / steps
      const fx = fn(x)
      if (isFinite(fx) && prevF * fx <= 0) { lo = prevX; hi = x; flo = prevF; fhi = fx; found = true; break }
      prevX = x; prevF = fx
    }
    if (!found) return ERR.NUM
  }
  // Bisection.
  for (let i = 0; i < 200; i++) {
    const mid = (lo + hi) / 2
    const fm = fn(mid)
    if (Math.abs(fm) < 1e-10 || (hi - lo) / 2 < 1e-12) return mid
    if (flo * fm <= 0) { hi = mid; fhi = fm } else { lo = mid; flo = fm }
  }
  return (lo + hi) / 2
}

// ── PRICEDISC / YIELDDISC ───────────────────────────────────────────────────────

const PRICEDISC: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const discount = num(ev, a[2]); if (isErr(discount)) return discount
  const redemption = num(ev, a[3]); if (isErr(redemption)) return redemption
  const b = a[4] ? num(ev, a[4]) : 0; if (isErr(b)) return b
  if (!basisOk(b)) return ERR.NUM
  if (discount <= 0 || redemption <= 0) return ERR.NUM
  const yf = yearFrac(sm.settlement, sm.maturity, b)
  if (isErr(yf)) return yf
  return redemption - discount * redemption * yf
}

const YIELDDISC: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const pr = num(ev, a[2]); if (isErr(pr)) return pr
  const redemption = num(ev, a[3]); if (isErr(redemption)) return redemption
  const b = a[4] ? num(ev, a[4]) : 0; if (isErr(b)) return b
  if (!basisOk(b)) return ERR.NUM
  if (pr <= 0 || redemption <= 0) return ERR.NUM
  const yf = yearFrac(sm.settlement, sm.maturity, b)
  if (isErr(yf)) return yf
  if (yf === 0) return ERR.NUM
  return (redemption - pr) / pr / yf
}

// ── PRICEMAT / YIELDMAT (interest at maturity) ──────────────────────────────────

const PRICEMAT: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const issueN = num(ev, a[2]); if (isErr(issueN)) return issueN
  const rate = num(ev, a[3]); if (isErr(rate)) return rate
  const yld = num(ev, a[4]); if (isErr(yld)) return yld
  const b = a[5] ? num(ev, a[5]) : 0; if (isErr(b)) return b
  const issue = readDate(issueN); if (!issue) return ERR.VALUE
  if (!basisOk(b)) return ERR.NUM
  if (rate < 0 || yld < 0) return ERR.NUM
  if (excelSerial(issue) >= excelSerial(sm.settlement)) return ERR.NUM

  const dim = yearFrac(issue, sm.maturity, b); if (isErr(dim)) return dim // issue→maturity
  const dis = yearFrac(issue, sm.settlement, b); if (isErr(dis)) return dis // issue→settlement
  const dsm = yearFrac(sm.settlement, sm.maturity, b); if (isErr(dsm)) return dsm // settlement→maturity

  const num1 = 100 + dim * rate * 100
  const den1 = 1 + dsm * yld
  return num1 / den1 - dis * rate * 100
}

const YIELDMAT: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const issueN = num(ev, a[2]); if (isErr(issueN)) return issueN
  const rate = num(ev, a[3]); if (isErr(rate)) return rate
  const pr = num(ev, a[4]); if (isErr(pr)) return pr
  const b = a[5] ? num(ev, a[5]) : 0; if (isErr(b)) return b
  const issue = readDate(issueN); if (!issue) return ERR.VALUE
  if (!basisOk(b)) return ERR.NUM
  if (rate < 0 || pr <= 0) return ERR.NUM
  if (excelSerial(issue) >= excelSerial(sm.settlement)) return ERR.NUM

  const dim = yearFrac(issue, sm.maturity, b); if (isErr(dim)) return dim
  const dis = yearFrac(issue, sm.settlement, b); if (isErr(dis)) return dis
  const dsm = yearFrac(sm.settlement, sm.maturity, b); if (isErr(dsm)) return dsm

  // Closed-form: derived from the PRICEMAT relation.
  const term1 = 1 + dim * rate
  const term2 = pr / 100 + dis * rate
  const numr = (term1 / term2) - 1
  return numr / dsm
}

// ── DURATION / MDURATION ────────────────────────────────────────────────────────

function durationCore(
  settlement: Date, maturity: Date, coupon: number, yld: number,
  frequency: number, basis: number,
): number {
  // Excel's Macaulay duration. dsc/e gives the fractional first period.
  const dsc = coupDaysSettleNext(settlement, maturity, frequency, basis)
  const e = coupDays(settlement, maturity, frequency, basis)
  const n = coupNumBetween(settlement, maturity, frequency)
  const t = dsc / e
  const yf = yld / frequency
  const couponAmt = (100 * coupon) / frequency

  let pvSum = 0
  let weighted = 0
  for (let k = 1; k <= n; k++) {
    const time = (k - 1 + t) // in periods
    const cf = couponAmt + (k === n ? 100 : 0)
    const pv = cf / Math.pow(1 + yf, time)
    pvSum += pv
    weighted += time * pv
  }
  const durationPeriods = weighted / pvSum
  return durationPeriods / frequency // convert periods → years
}

const DURATION: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const coupon = num(ev, a[2]); if (isErr(coupon)) return coupon
  const yld = num(ev, a[3]); if (isErr(yld)) return yld
  const f = num(ev, a[4]); if (isErr(f)) return f
  const b = a[5] ? num(ev, a[5]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (coupon < 0 || yld < 0) return ERR.NUM
  return durationCore(sm.settlement, sm.maturity, coupon, yld, f, b)
}

const MDURATION: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const coupon = num(ev, a[2]); if (isErr(coupon)) return coupon
  const yld = num(ev, a[3]); if (isErr(yld)) return yld
  const f = num(ev, a[4]); if (isErr(f)) return f
  const b = a[5] ? num(ev, a[5]) : 0; if (isErr(b)) return b
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (coupon < 0 || yld < 0) return ERR.NUM
  const dur = durationCore(sm.settlement, sm.maturity, coupon, yld, f, b)
  return dur / (1 + yld / f)
}

// ── Odd-period bonds ────────────────────────────────────────────────────────────
// Simplified models: the odd first/last period is priced using a fractional
// coupon over the actual day-count fraction, the rest as a regular bond. This
// matches Excel within the documented tolerance for typical inputs.

function oddFirstPriceCore(
  settlement: Date, maturity: Date, issue: Date, firstCoupon: Date,
  rate: number, yld: number, redemption: number, frequency: number, basis: number,
): number {
  const yf = yld / frequency
  // Number of full coupon periods from first_coupon to maturity.
  const nReg = coupNumBetween(firstCoupon, maturity, frequency)
  // Odd first coupon amount: rate applied over issue→first_coupon fraction.
  const oddFrac = yearFrac(issue, firstCoupon, basis); if (isErr(oddFrac)) return NaN
  const oddCoupon = 100 * rate * oddFrac
  const regCoupon = (100 * rate) / frequency

  // Discount exponent for the first (odd) coupon: settlement→first_coupon fraction
  // expressed in coupon periods.
  const dscFirst = yearFrac(settlement, firstCoupon, basis); if (isErr(dscFirst)) return NaN
  const e1 = yearFrac(issue, firstCoupon, basis); if (isErr(e1)) return NaN
  // Express the first discount exponent in coupon-period units. A regular period
  // equals 1/frequency years, so periods = years * frequency.
  const t1 = dscFirst * frequency

  let price = redemption / Math.pow(1 + yf, nReg + t1)
  // Odd first coupon at the first coupon date.
  price += oddCoupon / Math.pow(1 + yf, t1)
  // Regular coupons after the first coupon date.
  for (let k = 1; k <= nReg; k++) {
    price += regCoupon / Math.pow(1 + yf, t1 + k)
  }
  // Accrued interest on the odd first period up to settlement.
  const accFrac = yearFrac(issue, settlement, basis); if (isErr(accFrac)) return NaN
  // Fraction of the odd period elapsed.
  const acc = e1 > 0 ? oddCoupon * (accFrac / e1) : 0
  price -= acc
  return price
}

const ODDFPRICE: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const issueN = num(ev, a[2]); if (isErr(issueN)) return issueN
  const firstN = num(ev, a[3]); if (isErr(firstN)) return firstN
  const rate = num(ev, a[4]); if (isErr(rate)) return rate
  const yld = num(ev, a[5]); if (isErr(yld)) return yld
  const redemption = num(ev, a[6]); if (isErr(redemption)) return redemption
  const f = num(ev, a[7]); if (isErr(f)) return f
  const b = a[8] ? num(ev, a[8]) : 0; if (isErr(b)) return b
  const issue = readDate(issueN); const first = readDate(firstN)
  if (!issue || !first) return ERR.VALUE
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate < 0 || yld < 0 || redemption <= 0) return ERR.NUM
  const p = oddFirstPriceCore(sm.settlement, sm.maturity, issue, first, rate, yld, redemption, f, b)
  return isFinite(p) ? p : ERR.NUM
}

const ODDFYIELD: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const issueN = num(ev, a[2]); if (isErr(issueN)) return issueN
  const firstN = num(ev, a[3]); if (isErr(firstN)) return firstN
  const rate = num(ev, a[4]); if (isErr(rate)) return rate
  const pr = num(ev, a[5]); if (isErr(pr)) return pr
  const redemption = num(ev, a[6]); if (isErr(redemption)) return redemption
  const f = num(ev, a[7]); if (isErr(f)) return f
  const b = a[8] ? num(ev, a[8]) : 0; if (isErr(b)) return b
  const issue = readDate(issueN); const first = readDate(firstN)
  if (!issue || !first) return ERR.VALUE
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate < 0 || pr <= 0 || redemption <= 0) return ERR.NUM
  const fn = (y: number) =>
    oddFirstPriceCore(sm.settlement, sm.maturity, issue, first, rate, y, redemption, f, b) - pr
  return solveYield(fn)
}

function oddLastPriceCore(
  settlement: Date, maturity: Date, lastInterest: Date,
  rate: number, yld: number, redemption: number, frequency: number, basis: number,
): number {
  // Odd last period: from last_interest to maturity is a single (possibly long)
  // period. Price as discounted (redemption + odd coupon), less accrued interest.
  const oddFrac = yearFrac(lastInterest, maturity, basis); if (isErr(oddFrac)) return NaN
  const oddCoupon = 100 * rate * oddFrac
  const dsm = yearFrac(settlement, maturity, basis); if (isErr(dsm)) return NaN
  const t = dsm * frequency // periods settlement→maturity
  const yf = yld / frequency

  const price = (redemption + oddCoupon) / Math.pow(1 + yf, t)
  const accFrac = yearFrac(lastInterest, settlement, basis); if (isErr(accFrac)) return NaN
  const acc = oddFrac > 0 ? oddCoupon * (accFrac / oddFrac) : 0
  return price - acc
}

const ODDLPRICE: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const lastN = num(ev, a[2]); if (isErr(lastN)) return lastN
  const rate = num(ev, a[3]); if (isErr(rate)) return rate
  const yld = num(ev, a[4]); if (isErr(yld)) return yld
  const redemption = num(ev, a[5]); if (isErr(redemption)) return redemption
  const f = num(ev, a[6]); if (isErr(f)) return f
  const b = a[7] ? num(ev, a[7]) : 0; if (isErr(b)) return b
  const last = readDate(lastN); if (!last) return ERR.VALUE
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate < 0 || yld < 0 || redemption <= 0) return ERR.NUM
  const p = oddLastPriceCore(sm.settlement, sm.maturity, last, rate, yld, redemption, f, b)
  return isFinite(p) ? p : ERR.NUM
}

const ODDLYIELD: Fn = (ev, a) => {
  const sm = readSettleMaturity(num(ev, a[0]), num(ev, a[1])); if (isErr(sm)) return sm
  const lastN = num(ev, a[2]); if (isErr(lastN)) return lastN
  const rate = num(ev, a[3]); if (isErr(rate)) return rate
  const pr = num(ev, a[4]); if (isErr(pr)) return pr
  const redemption = num(ev, a[5]); if (isErr(redemption)) return redemption
  const f = num(ev, a[6]); if (isErr(f)) return f
  const b = a[7] ? num(ev, a[7]) : 0; if (isErr(b)) return b
  const last = readDate(lastN); if (!last) return ERR.VALUE
  if (!freqOk(f) || !basisOk(b)) return ERR.NUM
  if (rate < 0 || pr <= 0 || redemption <= 0) return ERR.NUM
  const fn = (y: number) =>
    oddLastPriceCore(sm.settlement, sm.maturity, last, rate, y, redemption, f, b) - pr
  return solveYield(fn)
}

// ── French depreciation: AMORLINC / AMORDEGRC ───────────────────────────────────

// Returns the depreciation for a single period using the French linear method.
const AMORLINC: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isErr(cost)) return cost
  const purN = num(ev, a[1]); if (isErr(purN)) return purN
  const firstN = num(ev, a[2]); if (isErr(firstN)) return firstN
  const salvage = num(ev, a[3]); if (isErr(salvage)) return salvage
  const period = num(ev, a[4]); if (isErr(period)) return period
  const rate = num(ev, a[5]); if (isErr(rate)) return rate
  const b = a[6] ? num(ev, a[6]) : 0; if (isErr(b)) return b

  const purchased = readDate(purN); const firstPeriod = readDate(firstN)
  if (!purchased || !firstPeriod) return ERR.VALUE
  if (!basisOk(b)) return ERR.NUM
  if (rate <= 0 || cost < salvage || period < 0) return ERR.NUM

  const fullDep = cost * rate
  // First-period (pro-rata) depreciation based on the day-count fraction.
  const frac = yearFrac(purchased, firstPeriod, b); if (isErr(frac)) return frac
  const firstDep = cost * rate * frac

  // Total depreciable amount.
  const depreciable = cost - salvage
  if (period === 0) return Math.min(firstDep, depreciable)

  // Cumulative depreciation through the requested period.
  let remaining = depreciable - firstDep
  if (remaining <= 0) {
    // Already fully depreciated in the first period; subsequent periods → 0.
    return 0
  }
  // Number of full periods after the first.
  const nFull = Math.floor(remaining / fullDep)
  if (period <= nFull) return fullDep
  if (period === nFull + 1) {
    const last = remaining - nFull * fullDep
    return last > 0 ? last : 0
  }
  return 0
}

// French degressive depreciation.
const AMORDEGRC: Fn = (ev, a) => {
  const cost = num(ev, a[0]); if (isErr(cost)) return cost
  const purN = num(ev, a[1]); if (isErr(purN)) return purN
  const firstN = num(ev, a[2]); if (isErr(firstN)) return firstN
  const salvage = num(ev, a[3]); if (isErr(salvage)) return salvage
  const period = num(ev, a[4]); if (isErr(period)) return period
  const rate = num(ev, a[5]); if (isErr(rate)) return rate
  const b = a[6] ? num(ev, a[6]) : 0; if (isErr(b)) return b

  const purchased = readDate(purN); const firstPeriod = readDate(firstN)
  if (!purchased || !firstPeriod) return ERR.VALUE
  if (!basisOk(b)) return ERR.NUM
  if (rate <= 0 || rate > 0.5 || cost < salvage || period < 0) return ERR.NUM

  // Asset life = 1/rate. French degressive coefficient depends on the life.
  const life = 1 / rate
  let coeff: number
  if (life >= 3 && life <= 4) coeff = 1.5
  else if (life >= 5 && life <= 6) coeff = 2
  else if (life > 6) coeff = 2.5
  else coeff = 1 // life < 3 → no acceleration

  const degRate = rate * coeff
  const frac = yearFrac(purchased, firstPeriod, b); if (isErr(frac)) return frac

  // Build the depreciation schedule up to the requested period.
  let book = cost
  let dep = 0
  const maxPeriods = Math.ceil(life) + 2
  for (let p = 0; p <= period && p <= maxPeriods; p++) {
    let d: number
    if (p === 0) {
      // First period: pro-rated by the day-count fraction, rounded as Excel does.
      d = Math.round(cost * degRate * frac)
    } else {
      // Remaining life in whole periods from the current period.
      const remainingLife = life - p
      if (remainingLife <= 2 && remainingLife > 0) {
        // Switch to straight-line over the remaining life.
        d = Math.round((book) / Math.max(remainingLife, 1))
      } else {
        d = Math.round(book * degRate)
      }
    }
    // Do not depreciate below salvage.
    if (book - d < salvage) d = book - salvage
    if (d < 0) d = 0
    dep = d
    book -= d
    if (book <= salvage) {
      if (p < period) return 0
      break
    }
  }
  return dep
}

// ── Registry ────────────────────────────────────────────────────────────────────

export const FINANCIAL2_FNS: Record<string, Fn> = {
  COUPNCD,
  COUPPCD,
  COUPDAYBS,
  COUPDAYSNC,
  COUPDAYS,
  COUPNUM,
  ACCRINT,
  PRICE,
  YIELD,
  PRICEDISC,
  YIELDDISC,
  PRICEMAT,
  YIELDMAT,
  DURATION,
  MDURATION,
  ODDFPRICE,
  ODDFYIELD,
  ODDLPRICE,
  ODDLYIELD,
  AMORLINC,
  AMORDEGRC,
}
