// fill-series.ts — Autofill series generator for the spreadsheet fill handle.
//
// Pure module (only `Intl` is used, available in Node) so the whole pattern engine
// is unit-testable via `node --experimental-strip-types`. Given the source cells of
// a fill-handle drag, `fillSeries` extends the detected pattern by `count` steps in
// either direction, or returns null (caller then tiles/repeats the source).
//
// Supported patterns (each a dedicated detector, tried most-specific first):
//   numbers (linear / geometric / regression, leading-zero pad), booleans,
//   thousands-grouped numbers, currency / percent / decimal with affixes,
//   roman numerals, alphabetic base-26 letters, 24h & 12h(AM/PM) times,
//   dates (ISO / DMY, sep -/./, daily / weekly / weekday-only / end-of-month /
//   monthly / yearly), month & weekday names (long/short, accents, ALL-CAPS,
//   trailing dot), "<month> <year>" with carry, quarters Qn / Tn with wrap+carry,
//   English & French ordinals, and the generic "text + number" token series.

// ── tiny numeric helpers ────────────────────────────────────────────────────────
export const pad2 = (n: number) => String(Math.abs(n)).padStart(2, '0')
const allEqual = <T,>(a: T[]) => a.every(x => x === a[0])
const consecutiveDiffs = (a: number[]) => a.slice(1).map((x, i) => x - a[i])
const constStep = (a: number[]) => a.length >= 2 && allEqual(consecutiveDiffs(a))
const lastStep = (a: number[]) => a.length >= 2 ? a[a.length - 1] - a[a.length - 2] : (a.length === 1 ? 0 : 1)
const round10 = (n: number) => Math.round(n * 1e10) / 1e10
const daysInMonth = (y: number, m: number) => new Date(Date.UTC(y, m, 0)).getUTCDate() // m=1..12

// ── locale month / weekday names ────────────────────────────────────────────────
const _nameCache: Record<string, string[]> = {}
function localeNames(lang: string, type: 'weekday' | 'month', variant: 'long' | 'short'): string[] {
  const key = `${lang}|${type}|${variant}`
  if (_nameCache[key]) return _nameCache[key]
  const fmt = new Intl.DateTimeFormat(lang, type === 'weekday' ? { weekday: variant } : { month: variant })
  const out: string[] = []
  if (type === 'weekday') for (let i = 0; i < 7; i++) out.push(fmt.format(new Date(Date.UTC(2024, 0, 1 + i))).replace(/\.$/, '').toLowerCase()) // 2024-01-01 = Monday
  else for (let m = 0; m < 12; m++) out.push(fmt.format(new Date(Date.UTC(2024, m, 1))).replace(/\.$/, '').toLowerCase())
  _nameCache[key] = out
  return out
}
const stripAccents = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '')
// Accent- and case-insensitive lookup of `s` in a list of (already lowercased) names.
const nameMatch = (s: string, names: string[]) => {
  const norm = stripAccents(s.replace(/\.$/, '').trim().toLowerCase())
  return names.findIndex(n => stripAccents(n) === norm)
}
// Re-apply the source cell's casing (Title / ALL-CAPS / lower) + trailing dot to a name.
function applyNameStyle(sample: string, word: string): string {
  const hasDot = /\.\s*$/.test(sample)
  const core = sample.replace(/\.\s*$/, '').trim()
  let out = word
  if (core.length > 1 && core === core.toUpperCase() && /\p{Lu}/u.test(core)) out = word.toUpperCase()
  else if (/^\p{Lu}/u.test(core)) out = word.charAt(0).toUpperCase() + word.slice(1)
  return hasDot ? out + '.' : out
}

// ── numeric trend (shared by several detectors) ─────────────────────────────────
// Extends a numeric sequence: geometric (≥3 pts, constant ratio ≠1), else linear
// regression (≥3 pts), else arithmetic step of the last gap (copy if a single cell).
function numberSeries(N: number[], count: number, forward: boolean): number[] {
  const n = N.length
  if (n >= 3 && N.every(v => v !== 0) && !constStep(N)) {
    const ratios = N.slice(1).map((v, i) => v / N[i])
    if (ratios.every(r => Math.abs(r - ratios[0]) < 1e-9) && Math.abs(ratios[0] - 1) > 1e-9) {
      const r = ratios[0], anchor = forward ? N[n - 1] : N[0]
      return Array.from({ length: count }, (_, k) => round10(forward ? anchor * Math.pow(r, k + 1) : anchor * Math.pow(r, -(k + 1))))
    }
  }
  if (n >= 3) {
    let sx = 0, sy = 0, sxx = 0, sxy = 0
    for (let i = 0; i < n; i++) { sx += i; sy += N[i]; sxx += i * i; sxy += i * N[i] }
    const b = (n * sxy - sx * sy) / (n * sxx - sx * sx), a = (sy - b * sx) / n
    return Array.from({ length: count }, (_, k) => { const x = forward ? (n - 1) + (k + 1) : -(k + 1); return round10(a + b * x) })
  }
  const step = n >= 2 ? N[n - 1] - N[n - 2] : 0, anchor = forward ? N[n - 1] : N[0]
  return Array.from({ length: count }, (_, k) => round10(forward ? anchor + step * (k + 1) : anchor - step * (k + 1)))
}

// ── dates ───────────────────────────────────────────────────────────────────────
type DateFmt = { order: 'ymd' | 'dmy'; sep: string }
const fmtKey = (f: DateFmt) => `${f.order}${f.sep}`
function parseFlexDate(s: string): { y: number; m: number; d: number; fmt: DateFmt } | null {
  let mt = s.match(/^(\d{4})([-/.])(\d{1,2})\2(\d{1,2})$/)         // YYYY‹sep›MM‹sep›DD
  if (mt) return { y: +mt[1], m: +mt[3], d: +mt[4], fmt: { order: 'ymd', sep: mt[2] } }
  mt = s.match(/^(\d{1,2})([-/.])(\d{1,2})\2(\d{4})$/)            // DD‹sep›MM‹sep›YYYY
  if (mt) return { y: +mt[4], m: +mt[3], d: +mt[1], fmt: { order: 'dmy', sep: mt[2] } }
  return null
}
function fmtFlexDate(y: number, m: number, d: number, fmt: DateFmt): string {
  const dd = Math.min(d, daysInMonth(y, m)) // never overflow (e.g. 31 Feb → 28/29)
  const s = fmt.sep
  return fmt.order === 'ymd' ? `${y}${s}${pad2(m)}${s}${pad2(dd)}` : `${pad2(dd)}${s}${pad2(m)}${s}${y}`
}
const isWeekend = (ms: number) => { const w = new Date(ms).getUTCDay(); return w === 0 || w === 6 }
const stepBusinessDays = (ms: number, n: number) => {       // advance n business days (n may be negative)
  const dir = n < 0 ? -1 : 1
  let left = Math.abs(n)
  while (left > 0) { ms += dir * 86400000; if (!isWeekend(ms)) left-- }
  return ms
}

// ── roman numerals ──────────────────────────────────────────────────────────────
const ROMAN: [string, number][] = [['M', 1000], ['CM', 900], ['D', 500], ['CD', 400], ['C', 100], ['XC', 90], ['L', 50], ['XL', 40], ['X', 10], ['IX', 9], ['V', 5], ['IV', 4], ['I', 1]]
function toRoman(n: number): string { if (n < 1 || n > 3999) return ''; let r = ''; for (const [s, v] of ROMAN) while (n >= v) { r += s; n -= v }; return r }
function fromRoman(s: string): number | null {
  const u = s.toUpperCase()
  if (!/^[MDCLXVI]+$/.test(u)) return null
  let i = 0, n = 0
  for (const [sym, v] of ROMAN) while (u.startsWith(sym, i)) { n += v; i += sym.length }
  return i === u.length && toRoman(n) === u ? n : null   // reject malformed (e.g. "IIII")
}

// ── alphabetic base-26 (A, B … Z, AA, AB …) ─────────────────────────────────────
const colVal = (s: string) => { let n = 0; for (const c of s.toUpperCase()) n = n * 26 + (c.charCodeAt(0) - 64); return n }
const colStr = (n: number) => { let s = ''; while (n > 0) { const r = (n - 1) % 26; s = String.fromCharCode(65 + r) + s; n = Math.floor((n - 1) / 26) } return s }

// ── ordinals ────────────────────────────────────────────────────────────────────
const ordEn = (n: number) => { const v = n % 100; if (v >= 11 && v <= 13) return 'th'; return ({ 1: 'st', 2: 'nd', 3: 'rd' } as Record<number, string>)[n % 10] || 'th' }

// ── number grouping (thousands) ─────────────────────────────────────────────────
const GROUP_RE = /^(-?\d{1,3}(?:([,  ])\d{3})+)$/  // 1,000 · 1 000 (regular & NBSP space)

// ── tokens (text + number) ──────────────────────────────────────────────────────
type FillTok = { type: 'num' | 'word' | 'sep'; val: string }
function tokenizeCell(s: string): FillTok[] {
  const m = s.match(/\d+|\p{L}+|[^\d\p{L}]+/gu) || []
  return m.map(v => ({ type: /^\d+$/.test(v) ? 'num' : /^\p{L}+$/u.test(v) ? 'word' : 'sep', val: v }))
}
// Continue a column of month/weekday names → returns the rendered names or null.
function nameColumn(col: string[], count: number, forward: boolean, lang: string): string[] | null {
  for (const kind of ['weekday', 'month'] as const) {
    for (const variant of ['long', 'short'] as const) {
      const names = localeNames(lang, kind, variant)
      const idxs = col.map(s => nameMatch(s, names))
      if (idxs.every(i => i >= 0)) {
        const L = names.length, step = lastStep(idxs) || 1, anchor = forward ? idxs[idxs.length - 1] : idxs[0]
        return Array.from({ length: count }, (_, k) => {
          const i = (((forward ? anchor + step * (k + 1) : anchor - step * (k + 1)) % L) + L) % L
          return applyNameStyle(col[0], names[i])
        })
      }
    }
  }
  return null
}

// ════════════════════════════════════════════════════════════════════════════════
export function fillSeries(srcVals: (string | number)[], count: number, forward: boolean, lang: string): (string | number)[] | null {
  if (count <= 0) return []
  const strs = srcVals.map(v => String(v).trim())
  const gen = <T,>(f: (k: number) => T): T[] => Array.from({ length: count }, (_, k) => f(k + 1))
  const nonEmpty = strs.every(s => s !== '')
  if (!nonEmpty) return null

  // 1 ── Pure numbers (leading-zero pad preserved: 001, 002 → 003). ───────────────
  if (strs.every(s => /^-?\d+(?:\.\d+)?$/.test(s))) {
    const N = strs.map(Number)
    const padW = strs.every(s => /^\d+$/.test(s)) && strs.some(s => s.length > 1 && s[0] === '0') ? Math.max(...strs.map(s => s.length)) : 0
    const seq = numberSeries(N, count, forward)
    return padW ? seq.map(n => String(Math.round(n)).padStart(padW, '0')) : seq
  }

  // 2 ── Booleans (TRUE/FALSE, VRAI/FAUX, Yes/No…) → cycle, preserving source case.
  const BOOL_SETS = [['true', 'false'], ['vrai', 'faux'], ['yes', 'no'], ['oui', 'non'], ['on', 'off']]
  for (const set of BOOL_SETS) {
    if (strs.length >= 1 && strs.every(s => set.includes(s.toLowerCase()))) {
      const n = strs.length
      return gen(k => { const idx = forward ? n - 1 + k : -k; return strs[(((idx % n) + n) % n)] })
    }
  }

  // 3 ── Thousands-grouped numbers (1,000 → 2,000 / 1 000 → 2 000). ────────────────
  const grp = strs.map(s => s.match(GROUP_RE))
  if (grp.every(m => m)) {
    const sepCh = grp[0]![2]
    const N = strs.map(s => Number(s.replace(/[,  ]/g, '')))
    const fmtGroup = (n: number) => {
      const neg = n < 0; const digits = String(Math.round(Math.abs(n)))
      let out = ''; for (let i = 0; i < digits.length; i++) { if (i > 0 && (digits.length - i) % 3 === 0) out += sepCh; out += digits[i] }
      return (neg ? '-' : '') + out
    }
    return numberSeries(N, count, forward).map(fmtGroup)
  }

  // 4 ── 24-hour times HH:MM[:SS]. ─────────────────────────────────────────────────
  const tm = strs.map(s => s.match(/^(\d{1,2}):(\d{2})(?::(\d{2}))?$/))
  if (tm.every(m => m)) {
    const secs = tm.map(m => (+m![1]) * 3600 + (+m![2]) * 60 + (+(m![3] || 0)))
    const hasSec = tm[0]![3] != null
    const step = secs.length >= 2 ? secs[secs.length - 1] - secs[secs.length - 2] : 3600
    const anchor = forward ? secs[secs.length - 1] : secs[0]
    return gen(k => {
      let t = forward ? anchor + step * k : anchor - step * k
      t = ((t % 86400) + 86400) % 86400
      const h = Math.floor(t / 3600), mi = Math.floor((t % 3600) / 60), se = t % 60
      return hasSec ? `${pad2(h)}:${pad2(mi)}:${pad2(se)}` : `${pad2(h)}:${pad2(mi)}`
    })
  }

  // 5 ── 12-hour times with meridiem (1:00 PM → 2:00 PM), case + dotting preserved.
  const ap = strs.map(s => s.match(/^(\d{1,2}):(\d{2})(?::(\d{2}))?\s*([AaPp])(\.?)[Mm](\.?)$/))
  if (ap.every(m => m)) {
    const secs = ap.map(m => { let h = (+m![1]) % 12; if (/p/i.test(m![4])) h += 12; return h * 3600 + (+m![2]) * 60 + (+(m![3] || 0)) })
    const hasSec = ap[0]![3] != null
    const upper = ap[0]![4] === ap[0]![4].toUpperCase()
    const dots = ap[0]![5] === '.' // "p.m." style
    const step = secs.length >= 2 ? secs[secs.length - 1] - secs[secs.length - 2] : 3600
    const anchor = forward ? secs[secs.length - 1] : secs[0]
    return gen(k => {
      let t = forward ? anchor + step * k : anchor - step * k
      t = ((t % 86400) + 86400) % 86400
      const h = Math.floor(t / 3600), mi = Math.floor((t % 3600) / 60), se = t % 60
      let h12 = h % 12; if (h12 === 0) h12 = 12
      let mer = (h < 12 ? 'a' : 'p') + (dots ? '.' : '') + 'm' + (dots ? '.' : '')
      if (upper) mer = mer.toUpperCase()
      return `${h12}:${pad2(mi)}${hasSec ? ':' + pad2(se) : ''} ${mer}`
    })
  }

  // 6 ── Dates: daily / weekly / weekday-only / end-of-month / monthly / yearly. ──
  const dts = strs.map(parseFlexDate)
  if (dts.every(d => d) && allEqual(dts.map(d => fmtKey(d!.fmt)))) {
    const D = dts as { y: number; m: number; d: number; fmt: DateFmt }[]
    const fmt = D[0].fmt
    const ms = D.map(d => Date.UTC(d.y, d.m - 1, d.d))
    const monthIdx = D.map(d => d.y * 12 + (d.m - 1))
    const out = (mss: number) => { const dd = new Date(mss); return fmtFlexDate(dd.getUTCFullYear(), dd.getUTCMonth() + 1, dd.getUTCDate(), fmt) }

    // End-of-month: every source is the last day of its month and months progress.
    if (D.every(p => p.d === daysInMonth(p.y, p.m)) && constStep(monthIdx) && consecutiveDiffs(monthIdx)[0] !== 0) {
      const step = lastStep(monthIdx), anchor = forward ? monthIdx[monthIdx.length - 1] : monthIdx[0]
      return gen(k => { const mi = forward ? anchor + step * k : anchor - step * k; const y = Math.floor(mi / 12), m = (mi % 12) + 1; return fmtFlexDate(y, m, daysInMonth(y, m), fmt) })
    }
    // Weekday-only: consecutive business days, with at least one Fri→Mon weekend gap.
    if (D.length >= 2 && ms.every(m => !isWeekend(m))) {
      const diffs = ms.slice(1).map((m, i) => Math.round((m - ms[i]) / 86400000))
      const businessConsec = ms.slice(1).every((m, i) => stepBusinessDays(ms[i], 1) === m)
      if (businessConsec && diffs.some(d => d > 1)) {
        const anchor = forward ? ms[ms.length - 1] : ms[0]
        return gen(k => out(stepBusinessDays(anchor, forward ? k : -k)))
      }
    }
    const sameDay = allEqual(D.map(d => d.d))
    const sameMonthDay = allEqual(D.map(d => d.m)) && sameDay
    if (sameDay && constStep(monthIdx) && consecutiveDiffs(monthIdx)[0] % 12 !== 0 && consecutiveDiffs(monthIdx)[0] !== 0) { // monthly
      const step = lastStep(monthIdx), anchor = forward ? monthIdx[monthIdx.length - 1] : monthIdx[0]
      return gen(k => { const mi = forward ? anchor + step * k : anchor - step * k; return fmtFlexDate(Math.floor(mi / 12), (mi % 12) + 1, D[0].d, fmt) })
    }
    if (sameMonthDay && constStep(D.map(d => d.y)) && consecutiveDiffs(D.map(d => d.y))[0] !== 0) { // yearly
      const step = lastStep(D.map(d => d.y)), anchor = forward ? D[D.length - 1].y : D[0].y
      return gen(k => fmtFlexDate(forward ? anchor + step * k : anchor - step * k, D[0].m, D[0].d, fmt))
    }
    const dayStep = ms.length >= 2 ? Math.round((ms[ms.length - 1] - ms[ms.length - 2]) / 86400000) : 1 // daily / weekly
    const anchor = forward ? ms[ms.length - 1] : ms[0]
    return gen(k => out(anchor + (forward ? dayStep * k : -dayStep * k) * 86400000))
  }

  // 7 ── Quarters Qn / Tn (+ optional year): cycle 1..4 with carry into the year. ──
  const qz = strs.map(s => s.match(/^(Q|T|Quarter\s*|Trimestre\s*)([1-4])(?:(\s+)(\d{4}))?$/i))
  if (qz.every(m => m) && allEqual(qz.map(m => m![1].toLowerCase())) && allEqual(qz.map(m => (m![4] ? 'y' : 'n')))) {
    const hasYear = qz[0]![4] != null
    const pre = qz[0]![1], sep = qz[0]![3] || ' '
    const abs = qz.map(m => hasYear ? (+m![4]) * 4 + (+m![2] - 1) : (+m![2] - 1))
    if (hasYear ? constStep(abs) : true) {
      const step = lastStep(abs) || 1, anchor = forward ? abs[abs.length - 1] : abs[0]
      return gen(k => {
        const a = forward ? anchor + step * k : anchor - step * k
        const q = ((a % 4) + 4) % 4 + 1
        return hasYear ? `${pre}${q}${sep}${Math.floor(a / 4)}` : `${pre}${q}`
      })
    }
  }

  // 8 ── "<month> <year>" (Jan 2024 → Feb 2024 … Dec 2024 → Jan 2025). ─────────────
  const my = strs.map(s => { const m = s.match(/^(\p{L}+)(\s+)(\d{4})$/u); return m ? { name: m[1], sep: m[2], year: +m[3] } : null })
  if (my.every(p => p)) {
    for (const kind of ['month'] as const) {
      for (const variant of ['long', 'short'] as const) {
        const names = localeNames(lang, kind, variant)
        const idxs = my.map(p => nameMatch(p!.name, names))
        if (idxs.every(i => i >= 0)) {
          const abs = my.map((p, i) => p!.year * 12 + idxs[i])
          if (constStep(abs) || abs.length === 1) {
            const step = lastStep(abs) || 1, anchor = forward ? abs[abs.length - 1] : abs[0], sep = my[0]!.sep
            return gen(k => { const a = forward ? anchor + step * k : anchor - step * k; return `${applyNameStyle(my[0]!.name, names[((a % 12) + 12) % 12])}${sep}${Math.floor(a / 12)}` })
          }
        }
      }
    }
  }

  // 9 ── Month / weekday names alone (long/short, accents, ALL-CAPS, dot). ──────────
  const names = nameColumn(strs, count, forward, lang)
  if (names) return names

  // 10 ── Roman numerals (≥2 cells, at least one multi-char; pure single letters
  //       fall through to the alphabetic detector below). ──────────────────────────
  if (strs.length >= 2 && strs.some(s => s.length > 1)) {
    const rv = strs.map(fromRoman)
    if (rv.every(v => v != null) && constStep(rv as number[]) && consecutiveDiffs(rv as number[])[0] !== 0) {
      const V = rv as number[], lower = strs.every(s => s === s.toLowerCase())
      const step = lastStep(V) || 1, anchor = forward ? V[V.length - 1] : V[0]
      return gen(k => { const n = forward ? anchor + step * k : anchor - step * k; const r = toRoman(n); return lower ? r.toLowerCase() : r })
    }
  }

  // 11 ── Alphabetic base-26 letters (A,B → C ; AA,AB → AC). Restricted to single
  //       letters (any case) or short UPPERCASE runs so it never hijacks words. ─────
  const letterLike = strs.every(s => /^[A-Za-z]$/.test(s)) || strs.every(s => /^[A-Z]{1,3}$/.test(s))
  if (strs.length >= 2 && letterLike && allEqual(strs.map(s => s === s.toUpperCase()))) {
    const V = strs.map(colVal)
    if (constStep(V) && consecutiveDiffs(V)[0] !== 0) {
      const lower = strs[0] === strs[0].toLowerCase()
      const step = lastStep(V) || 1, anchor = forward ? V[V.length - 1] : V[0]
      return gen(k => { const n = forward ? anchor + step * k : anchor - step * k; if (n < 1) return ''; const s = colStr(n); return lower ? s.toLowerCase() : s })
    }
  }

  // 12 ── Ordinals: English (1st,2nd → 3rd) and French (1er,2e → 3e). ──────────────
  const oen = strs.map(s => s.match(/^(\d+)(st|nd|rd|th)$/i))
  if (oen.every(m => m)) {
    const N = oen.map(m => +m![1]), up = oen[0]![2] === oen[0]![2].toUpperCase()
    const step = lastStep(N) || 1, anchor = forward ? N[N.length - 1] : N[0]
    return gen(k => { const n = forward ? anchor + step * k : anchor - step * k; if (n < 1) return ''; const suf = up ? ordEn(n).toUpperCase() : ordEn(n); return `${n}${suf}` })
  }
  const ofr = strs.map(s => s.match(/^(\d+)(ers?|res?|es?|d)$/i))
  if (ofr.every(m => m) && strs.some(s => /\D/.test(s))) {
    const N = ofr.map(m => +m![1])
    const step = lastStep(N) || 1, anchor = forward ? N[N.length - 1] : N[0]
    return gen(k => { const n = forward ? anchor + step * k : anchor - step * k; if (n < 1) return ''; return `${n}${n === 1 ? 'er' : 'e'}` })
  }

  // 13 ── Single number wrapped in fixed affixes: currency / percent / decimals
  //       ($1.50 → $2.50 · 10.5% → 11.5% · Item1 → Item2 · ref-12 → ref-13). ───────
  const af = strs.map(s => s.match(/^(\D*?)(-?\d+(?:\.\d+)?)(\D*)$/))
  if (af.every(m => m) && allEqual(af.map(m => m![1])) && allEqual(af.map(m => m![3]))) {
    const pre = af[0]![1], suf = af[0]![3]
    const decimals = Math.max(...af.map(m => { const dot = m![2].indexOf('.'); return dot < 0 ? 0 : m![2].length - dot - 1 }))
    const N = af.map(m => Number(m![2]))
    // A single affixed number increments by 1 (Item1 → Item2), like the token path;
    // two or more follow the detected trend.
    const seq = N.length >= 2 ? numberSeries(N, count, forward) : gen(k => forward ? N[0] + k : N[0] - k)
    return seq.map(n => `${pre}${decimals > 0 ? n.toFixed(decimals) : String(Math.round(n))}${suf}`)
  }

  // 14 ── Generic "text + number" tokens — aligns cells token-by-token and extends
  //        each varying token independently (Item1 → Item2 ; "Lundi, 1" → "Mardi, 2"
  //        ; v1.2.9 → v1.2.10). ─────────────────────────────────────────────────────
  const toks = strs.map(tokenizeCell)
  const len = toks[0].length
  if (len > 0 && toks.every(t => t.length === len && t.every((tk, i) => tk.type === toks[0][i].type))) {
    const colSeries: string[][] = []
    const isConst = (col: string[]) => col.length >= 2 && allEqual(col)
    // For a SINGLE source cell with several numbers (e.g. a version "v1.2.9"), only the
    // LAST number advances; earlier ones stay fixed. With ≥2 cells every column trends.
    const numIdx = toks[0].map((tk, i) => tk.type === 'num' ? i : -1).filter(i => i >= 0)
    const lastNumIdx = numIdx[numIdx.length - 1]
    let ok = true, anyVaries = false
    for (let i = 0; i < len && ok; i++) {
      const col = toks.map(t => t[i].val)
      const type = toks[0][i].type
      if (isConst(col)) { colSeries.push(Array(count).fill(col[0])); continue }
      if (type === 'sep') {
        if (allEqual(col)) { colSeries.push(Array(count).fill(col[0])); continue }
        ok = false; break
      }
      if (type === 'num') {
        const padW = col.some(x => x.length > 1 && x[0] === '0') ? Math.max(...col.map(x => x.length)) : 0
        const Ni = col.map(x => parseInt(x, 10))
        const advance = Ni.length >= 2 || i === lastNumIdx
        const seq = Ni.length >= 2 ? numberSeries(Ni, count, forward)
          : advance ? Array.from({ length: count }, (_, k) => forward ? Ni[0] + (k + 1) : Ni[0] - (k + 1))
            : Array(count).fill(Ni[0])
        colSeries.push(seq.map(v => { const r = Math.round(v); return padW ? String(Math.max(0, r)).padStart(padW, '0') : String(r) }))
        if (advance) anyVaries = true
        continue
      }
      const ws = nameColumn(col, count, forward, lang) // word: try month/weekday names
      if (ws) { colSeries.push(ws); anyVaries = true; continue }
      if (allEqual(col)) { colSeries.push(Array(count).fill(col[0])); continue }
      ok = false; break
    }
    if (ok && anyVaries) return gen(k => colSeries.map(c => c[k - 1]).join(''))
  }
  return null
}
