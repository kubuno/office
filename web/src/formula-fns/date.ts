// ── Date & Time functions for the spreadsheet formula engine ─────────────────
// Excel/Sheets-compatible Date & Time functions. The sheet stores dates as
// Excel serial numbers (days since 1899-12-30). Time-of-day is the fractional
// part of the serial. FErr values propagate; invalid dates yield #VALUE!/#NUM!.

import {
  type Fn,
  type Evaluator,
  type Node,
  type Value,
  ERR,
  isErr,
  num,
  str,
  flatten,
  toNum,
  excelSerial,
  serialToDate,
} from '../formula-engine'

// ── Helpers ──────────────────────────────────────────────────────────────────

// Evaluate an argument as an Excel serial number (with FErr propagation).
function serialArg(ev: Evaluator, n: Node): number | typeof ERR.VALUE {
  const v = num(ev, n)
  if (isErr(v)) return v as typeof ERR.VALUE
  return v
}

// Build a serial from a calendar date; Date(y, m-1, d) handles month/day overflow.
function ymdToSerial(y: number, m: number, d: number): number {
  return excelSerial(new Date(y, m - 1, d))
}

// Parse a textual date into a serial; returns null on failure.
function parseDate(text: string): number | null {
  const t = text.trim()
  if (t === '') return null
  // ISO YYYY-MM-DD (optionally with time)
  let m = t.match(/^(\d{4})-(\d{1,2})-(\d{1,2})/)
  if (m) return ymdToSerial(+m[1], +m[2], +m[3])
  // D/M/Y or M/D/Y — assume M/D/Y (US, Excel default) for ambiguous inputs
  m = t.match(/^(\d{1,2})[/.](\d{1,2})[/.](\d{2,4})$/)
  if (m) {
    let y = +m[3]
    if (y < 100) y += y < 30 ? 2000 : 1900
    return ymdToSerial(y, +m[1], +m[2])
  }
  // Fall back to the JS date parser (handles e.g. "March 5, 2020")
  const parsed = Date.parse(t)
  if (!isNaN(parsed)) {
    const d = new Date(parsed)
    return ymdToSerial(d.getFullYear(), d.getMonth() + 1, d.getDate())
  }
  return null
}

// Fractional part of a serial as whole seconds in the day (rounded).
function timePartSeconds(serial: number): number {
  let frac = serial - Math.floor(serial)
  if (frac < 0) frac += 1
  return Math.round(frac * 86400)
}

// WEEKDAY return values for the various `type` codes.
function weekdayValue(serial: number, type: number): number | typeof ERR.NUM {
  const d = serialToDate(serial)
  if (!d) return ERR.VALUE as unknown as number
  const js = d.getDay() // 0=Sun … 6=Sat
  switch (type) {
    case 1: return js + 1 // 1=Sun … 7=Sat
    case 2: return js === 0 ? 7 : js // 1=Mon … 7=Sun
    case 3: return js === 0 ? 6 : js - 1 // 0=Mon … 6=Sun
    case 11: return js === 0 ? 7 : js // 1=Mon … 7=Sun
    case 12: return ((js + 5) % 7) + 1 // Tue=1 … Mon=7
    case 13: return ((js + 4) % 7) + 1 // Wed=1 … Tue=7
    case 14: return ((js + 3) % 7) + 1 // Thu=1 … Wed=7
    case 15: return ((js + 2) % 7) + 1 // Fri=1 … Thu=7
    case 16: return ((js + 1) % 7) + 1 // Sat=1 … Fri=7
    case 17: return js + 1 // Sun=1 … Sat=7
    default: return ERR.NUM
  }
}

// 30/360 day count between two serials (US or European convention).
function days360(startSerial: number, endSerial: number, european: boolean): number | null {
  const ds = serialToDate(startSerial)
  const de = serialToDate(endSerial)
  if (!ds || !de) return null
  let d1 = ds.getDate()
  let d2 = de.getDate()
  const m1 = ds.getMonth() + 1
  const m2 = de.getMonth() + 1
  const y1 = ds.getFullYear()
  const y2 = de.getFullYear()
  if (european) {
    if (d1 === 31) d1 = 30
    if (d2 === 31) d2 = 30
  } else {
    // US (NASD) convention
    if (d1 === 31) d1 = 30
    if (d2 === 31 && d1 === 30) d2 = 30
  }
  return (y2 - y1) * 360 + (m2 - m1) * 30 + (d2 - d1)
}

// Add `months` to a date and return its serial (EDATE behaviour).
function addMonths(serial: number, months: number): number | null {
  const d = serialToDate(serial)
  if (!d) return null
  const y = d.getFullYear()
  const m = d.getMonth() + months
  const day = d.getDate()
  // Clamp day to the last day of the target month (Excel behaviour).
  const targetY = y + Math.floor(m / 12)
  const targetM = ((m % 12) + 12) % 12 // 0-based
  const lastDay = new Date(targetY, targetM + 1, 0).getDate()
  return ymdToSerial(targetY, targetM + 1, Math.min(day, lastDay))
}

// Decode a NETWORKDAYS.INTL / WORKDAY.INTL `weekend` parameter into a
// 7-element boolean mask indexed by JS getDay() (0=Sun … 6=Sat).
function weekendMask(weekend: number | string): boolean[] | null {
  // String form: 7 chars of '0'/'1', Monday … Sunday, '1' = weekend.
  if (typeof weekend === 'string') {
    if (!/^[01]{7}$/.test(weekend)) return null
    // Monday-indexed → JS getDay-indexed (0=Sun … 6=Sat)
    const mon = weekend.split('').map((c) => c === '1')
    // mon[0]=Mon … mon[6]=Sun ; map to [Sun, Mon, Tue, Wed, Thu, Fri, Sat]
    return [mon[6], mon[0], mon[1], mon[2], mon[3], mon[4], mon[5]]
  }
  // Numeric codes → weekend days, indexed in JS getDay() order (0=Sun … 6=Sat).
  const codeMap: Record<number, number[]> = {
    1: [0, 6], 2: [0, 1], 3: [1, 2], 4: [2, 3], 5: [3, 4], 6: [4, 5], 7: [5, 6],
    11: [0], 12: [1], 13: [2], 14: [3], 15: [4], 16: [5], 17: [6],
  }
  const days = codeMap[weekend]
  if (!days) return null
  const mask = [false, false, false, false, false, false, false]
  for (const d of days) mask[d] = true
  return mask
}

// Collect holiday serials (as a Set of integer day numbers) from an argument.
function collectHolidays(ev: Evaluator, node: Node | undefined): Set<number> {
  const set = new Set<number>()
  if (!node) return set
  for (const s of flatten(ev.eval(node))) {
    if (isErr(s)) continue
    const n = toNum(s)
    if (!isErr(n)) set.add(Math.floor(n as number))
  }
  return set
}

// ── Function registry ─────────────────────────────────────────────────────────

export const DATE_FNS: Record<string, Fn> = {
  DATE: (ev, a) => {
    const y = num(ev, a[0]); if (isErr(y)) return y
    const m = num(ev, a[1]); if (isErr(m)) return m
    const d = num(ev, a[2]); if (isErr(d)) return d
    return ymdToSerial(y as number, m as number, d as number)
  },

  DATEVALUE: (ev, a) => {
    const text = str(ev, a[0])
    const s = parseDate(text)
    return s == null ? ERR.VALUE : s
  },

  TIMEVALUE: (ev, a) => {
    const text = str(ev, a[0]).trim()
    const m = text.match(/(\d{1,2}):(\d{1,2})(?::(\d{1,2}))?\s*(am|pm)?/i)
    if (!m) return ERR.VALUE
    let h = +m[1]
    const mm = +m[2]
    const ss = m[3] ? +m[3] : 0
    const ap = m[4]?.toLowerCase()
    if (ap === 'pm' && h < 12) h += 12
    if (ap === 'am' && h === 12) h = 0
    if (h > 23 || mm > 59 || ss > 59) return ERR.VALUE
    return (h * 3600 + mm * 60 + ss) / 86400
  },

  TIME: (ev, a) => {
    const h = num(ev, a[0]); if (isErr(h)) return h
    const m = num(ev, a[1]); if (isErr(m)) return m
    const s = num(ev, a[2]); if (isErr(s)) return s
    const total = (h as number) * 3600 + (m as number) * 60 + (s as number)
    // Excel keeps only the fractional day part.
    let frac = (total / 86400) % 1
    if (frac < 0) frac += 1
    return frac
  },

  HOUR: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    return Math.floor(timePartSeconds(serial as number) / 3600)
  },

  MINUTE: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    return Math.floor((timePartSeconds(serial as number) % 3600) / 60)
  },

  SECOND: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    return timePartSeconds(serial as number) % 60
  },

  WEEKDAY: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    const type = a[1] ? num(ev, a[1]) : 1
    if (isErr(type)) return type
    return weekdayValue(serial as number, type as number)
  },

  WEEKNUM: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    const type = a[1] ? num(ev, a[1]) : 1
    if (isErr(type)) return type
    const d = serialToDate(serial as number)
    if (!d) return ERR.VALUE
    // ISO week-numbering (system 21).
    if ((type as number) === 21) return DATE_FNS.ISOWEEKNUM(ev, a) as Value
    // First day of the week: type 1 → Sunday, type 2 → Monday.
    const weekStartsMonday = (type as number) === 2
    const jan1 = new Date(d.getFullYear(), 0, 1)
    const jan1Dow = jan1.getDay() // 0=Sun … 6=Sat
    const offset = weekStartsMonday ? (jan1Dow === 0 ? 6 : jan1Dow - 1) : jan1Dow
    const dayOfYear = Math.floor((d.getTime() - jan1.getTime()) / 86400000) + 1
    return Math.floor((dayOfYear + offset - 1) / 7) + 1
  },

  ISOWEEKNUM: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    const d = serialToDate(serial as number)
    if (!d) return ERR.VALUE
    // ISO 8601: week 1 contains the first Thursday of the year.
    const target = new Date(d.getFullYear(), d.getMonth(), d.getDate())
    const dayNr = (target.getDay() + 6) % 7 // Mon=0 … Sun=6
    target.setDate(target.getDate() - dayNr + 3) // nearest Thursday
    const firstThursday = new Date(target.getFullYear(), 0, 4)
    const firstDayNr = (firstThursday.getDay() + 6) % 7
    firstThursday.setDate(firstThursday.getDate() - firstDayNr + 3)
    return 1 + Math.round((target.getTime() - firstThursday.getTime()) / (7 * 86400000))
  },

  EDATE: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    const months = num(ev, a[1]); if (isErr(months)) return months
    const r = addMonths(serial as number, Math.trunc(months as number))
    return r == null ? ERR.VALUE : r
  },

  EOMONTH: (ev, a) => {
    const serial = serialArg(ev, a[0]); if (isErr(serial)) return serial
    const months = num(ev, a[1]); if (isErr(months)) return months
    const d = serialToDate(serial as number)
    if (!d) return ERR.VALUE
    const m = d.getMonth() + Math.trunc(months as number)
    const targetY = d.getFullYear() + Math.floor(m / 12)
    const targetM = ((m % 12) + 12) % 12 // 0-based
    // Day 0 of the following month is the last day of the target month.
    return ymdToSerial(targetY, targetM + 2, 0)
  },

  DAYS: (ev, a) => {
    const end = serialArg(ev, a[0]); if (isErr(end)) return end
    const start = serialArg(ev, a[1]); if (isErr(start)) return start
    return Math.floor(end as number) - Math.floor(start as number)
  },

  DAYS360: (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const end = serialArg(ev, a[1]); if (isErr(end)) return end
    // Optional method flag: TRUE → European convention.
    let european = false
    if (a[2]) {
      const meth = ev.scalar(a[2])
      if (isErr(meth)) return meth
      const mn = toNum(meth)
      if (isErr(mn)) return mn
      european = (mn as number) !== 0
    }
    const r = days360(start as number, end as number, european)
    return r == null ? ERR.VALUE : r
  },

  DATEDIF: (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const end = serialArg(ev, a[1]); if (isErr(end)) return end
    const unit = str(ev, a[2]).toUpperCase()
    const ds = serialToDate(start as number)
    const de = serialToDate(end as number)
    if (!ds || !de) return ERR.VALUE
    if ((end as number) < (start as number)) return ERR.NUM
    const y1 = ds.getFullYear(), m1 = ds.getMonth(), d1 = ds.getDate()
    const y2 = de.getFullYear(), m2 = de.getMonth(), d2 = de.getDate()
    switch (unit) {
      case 'D':
        return Math.floor(end as number) - Math.floor(start as number)
      case 'Y': {
        let years = y2 - y1
        if (m2 < m1 || (m2 === m1 && d2 < d1)) years--
        return years
      }
      case 'M': {
        let months = (y2 - y1) * 12 + (m2 - m1)
        if (d2 < d1) months--
        return months
      }
      case 'MD': {
        // Days, ignoring months and years.
        let days = d2 - d1
        if (days < 0) {
          const prevMonthLast = new Date(y2, m2, 0).getDate()
          days += prevMonthLast
        }
        return days
      }
      case 'YM': {
        let months = (m2 - m1)
        if (d2 < d1) months--
        if (months < 0) months += 12
        return months
      }
      case 'YD': {
        // Days, ignoring years.
        let anchor = new Date(y1, m2, d2)
        if (anchor.getTime() < ds.getTime()) anchor = new Date(y1 + 1, m2, d2)
        return Math.round((anchor.getTime() - ds.getTime()) / 86400000)
      }
      default:
        return ERR.NUM
    }
  },

  NETWORKDAYS: (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const end = serialArg(ev, a[1]); if (isErr(end)) return end
    const holidays = collectHolidays(ev, a[2])
    let s = Math.floor(start as number)
    let e = Math.floor(end as number)
    const sign = s <= e ? 1 : -1
    if (sign < 0) { const t = s; s = e; e = t }
    let count = 0
    for (let cur = s; cur <= e; cur++) {
      const dow = serialToDate(cur)?.getDay() ?? 0
      if (dow === 0 || dow === 6) continue // weekend (Sat/Sun)
      if (holidays.has(cur)) continue
      count++
    }
    return count * sign
  },

  'NETWORKDAYS.INTL': (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const end = serialArg(ev, a[1]); if (isErr(end)) return end
    let weekend: number | string = 1
    if (a[2]) {
      const w = ev.scalar(a[2])
      if (isErr(w)) return w
      weekend = typeof w === 'string' ? w : (toNum(w) as number)
    }
    const mask = weekendMask(weekend)
    if (!mask) return ERR.NUM
    const holidays = collectHolidays(ev, a[3])
    let s = Math.floor(start as number)
    let e = Math.floor(end as number)
    const sign = s <= e ? 1 : -1
    if (sign < 0) { const t = s; s = e; e = t }
    let count = 0
    for (let cur = s; cur <= e; cur++) {
      const dow = serialToDate(cur)?.getDay() ?? 0
      if (mask[dow]) continue
      if (holidays.has(cur)) continue
      count++
    }
    return count * sign
  },

  WORKDAY: (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const days = num(ev, a[1]); if (isErr(days)) return days
    const holidays = collectHolidays(ev, a[2])
    let n = Math.trunc(days as number)
    let cur = Math.floor(start as number)
    const step = n >= 0 ? 1 : -1
    n = Math.abs(n)
    while (n > 0) {
      cur += step
      const dow = serialToDate(cur)?.getDay() ?? 0
      if (dow === 0 || dow === 6) continue
      if (holidays.has(cur)) continue
      n--
    }
    return cur
  },

  'WORKDAY.INTL': (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const days = num(ev, a[1]); if (isErr(days)) return days
    let weekend: number | string = 1
    if (a[2]) {
      const w = ev.scalar(a[2])
      if (isErr(w)) return w
      weekend = typeof w === 'string' ? w : (toNum(w) as number)
    }
    const mask = weekendMask(weekend)
    if (!mask) return ERR.NUM
    const holidays = collectHolidays(ev, a[3])
    let n = Math.trunc(days as number)
    let cur = Math.floor(start as number)
    const step = n >= 0 ? 1 : -1
    n = Math.abs(n)
    while (n > 0) {
      cur += step
      const dow = serialToDate(cur)?.getDay() ?? 0
      if (mask[dow]) continue
      if (holidays.has(cur)) continue
      n--
    }
    return cur
  },

  YEARFRAC: (ev, a) => {
    const start = serialArg(ev, a[0]); if (isErr(start)) return start
    const end = serialArg(ev, a[1]); if (isErr(end)) return end
    const basis = a[2] ? num(ev, a[2]) : 0
    if (isErr(basis)) return basis
    let s = start as number
    let e = end as number
    if (s > e) { const t = s; s = e; e = t }
    const ds = serialToDate(s)
    const de = serialToDate(e)
    if (!ds || !de) return ERR.VALUE
    switch (basis as number) {
      case 0: { // 30/360 US
        const d = days360(s, e, false)
        return d == null ? ERR.VALUE : d / 360
      }
      case 1: { // actual/actual
        const y1 = ds.getFullYear(), y2 = de.getFullYear()
        if (y1 === y2) {
          const yearDays = ((y1 % 4 === 0 && y1 % 100 !== 0) || y1 % 400 === 0) ? 366 : 365
          return (e - s) / yearDays
        }
        // Average year length across the span (Excel approximation).
        let totalDaysInYears = 0
        for (let y = y1; y <= y2; y++) {
          totalDaysInYears += ((y % 4 === 0 && y % 100 !== 0) || y % 400 === 0) ? 366 : 365
        }
        const avgYearLen = totalDaysInYears / (y2 - y1 + 1)
        return (e - s) / avgYearLen
      }
      case 2: // actual/360
        return (e - s) / 360
      case 3: // actual/365
        return (e - s) / 365
      case 4: { // 30/360 European
        const d = days360(s, e, true)
        return d == null ? ERR.VALUE : d / 360
      }
      default:
        return ERR.NUM
    }
  },
}
