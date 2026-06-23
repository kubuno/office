// Number / date / value formatting helpers shared by every Data (BI) visual.
// Power BI-like format strings drive how measures render across cards, charts,
// tables and tooltips. Kept framework-agnostic (pure functions) so charts stay
// cheap to render.

export type NumberFormat =
  | 'auto' | 'number' | 'integer' | 'decimal1' | 'decimal2'
  | 'thousands' | 'compact' | 'percent' | 'percent1' | 'percent2'
  | 'currency_eur' | 'currency_usd' | 'currency_gbp'
  | 'scientific' | 'duration' | 'bytes'

export const NUMBER_FORMATS: { value: NumberFormat; label: string }[] = [
  { value: 'auto',         label: 'Automatique' },
  { value: 'number',       label: '1 234,56' },
  { value: 'integer',      label: '1 235' },
  { value: 'decimal1',     label: '1 234,5' },
  { value: 'decimal2',     label: '1 234,56' },
  { value: 'thousands',    label: '1 234 (milliers)' },
  { value: 'compact',      label: '1,2 k / 3,4 M' },
  { value: 'percent',      label: '12 %' },
  { value: 'percent1',     label: '12,3 %' },
  { value: 'percent2',     label: '12,34 %' },
  { value: 'currency_eur', label: '1 234 €' },
  { value: 'currency_usd', label: '$1,234' },
  { value: 'currency_gbp', label: '£1,234' },
  { value: 'scientific',   label: '1,23E3' },
  { value: 'duration',     label: '1 h 23 min' },
  { value: 'bytes',        label: '1,2 Ko / 3,4 Mo' },
]

const LOCALE = 'fr-FR'

export function toNum(v: unknown): number {
  if (typeof v === 'number') return v
  if (typeof v === 'string') { const n = parseFloat(v.replace(/\s/g, '').replace(',', '.')); return Number.isFinite(n) ? n : 0 }
  return 0
}

export function isNumericValue(v: unknown): boolean {
  if (typeof v === 'number') return true
  if (typeof v === 'string') return v.trim() !== '' && Number.isFinite(parseFloat(v.replace(/\s/g, '').replace(',', '.')))
  return false
}

function compact(n: number, digits = 1): string {
  const abs = Math.abs(n)
  const sign = n < 0 ? '-' : ''
  if (abs >= 1e12) return `${sign}${(abs / 1e12).toFixed(digits).replace('.', ',')} T`
  if (abs >= 1e9)  return `${sign}${(abs / 1e9).toFixed(digits).replace('.', ',')} Md`
  if (abs >= 1e6)  return `${sign}${(abs / 1e6).toFixed(digits).replace('.', ',')} M`
  if (abs >= 1e3)  return `${sign}${(abs / 1e3).toFixed(digits).replace('.', ',')} k`
  return n.toLocaleString(LOCALE, { maximumFractionDigits: digits })
}

function bytes(n: number): string {
  const units = ['o', 'Ko', 'Mo', 'Go', 'To', 'Po']
  let v = Math.abs(n), i = 0
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
  return `${n < 0 ? '-' : ''}${v.toFixed(i === 0 ? 0 : 1).replace('.', ',')} ${units[i]}`
}

function duration(seconds: number): string {
  const s = Math.round(Math.abs(seconds))
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60
  if (h) return `${h} h ${m} min`
  if (m) return `${m} min ${sec} s`
  return `${sec} s`
}

/** Format a numeric value according to a Power BI-like format token. */
export function formatValue(value: unknown, fmt?: NumberFormat | string, opts?: { decimals?: number; prefix?: string; suffix?: string }): string {
  const n = toNum(value)
  let out: string
  switch (fmt) {
    case 'integer':      out = Math.round(n).toLocaleString(LOCALE); break
    case 'decimal1':     out = n.toLocaleString(LOCALE, { minimumFractionDigits: 1, maximumFractionDigits: 1 }); break
    case 'decimal2':     out = n.toLocaleString(LOCALE, { minimumFractionDigits: 2, maximumFractionDigits: 2 }); break
    case 'thousands':    out = (n / 1000).toLocaleString(LOCALE, { maximumFractionDigits: 1 }); break
    case 'compact':      out = compact(n, opts?.decimals ?? 1); break
    case 'percent':      out = `${(n * 100).toLocaleString(LOCALE, { maximumFractionDigits: 0 })} %`; break
    case 'percent1':     out = `${(n * 100).toLocaleString(LOCALE, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} %`; break
    case 'percent2':     out = `${(n * 100).toLocaleString(LOCALE, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} %`; break
    case 'currency_eur': out = `${n.toLocaleString(LOCALE, { maximumFractionDigits: 0 })} €`; break
    case 'currency_usd': out = `$${n.toLocaleString('en-US', { maximumFractionDigits: 0 })}`; break
    case 'currency_gbp': out = `£${n.toLocaleString('en-GB', { maximumFractionDigits: 0 })}`; break
    case 'scientific':   out = n.toExponential(opts?.decimals ?? 2).replace('.', ',').replace('e', 'E').replace('E+', 'E'); break
    case 'duration':     out = duration(n); break
    case 'bytes':        out = bytes(n); break
    case 'number':       out = n.toLocaleString(LOCALE, { maximumFractionDigits: 2 }); break
    case 'auto':
    default:
      out = Number.isInteger(n) ? n.toLocaleString(LOCALE) : n.toLocaleString(LOCALE, { maximumFractionDigits: 2 })
  }
  return `${opts?.prefix ?? ''}${out}${opts?.suffix ?? ''}`
}

/** Short axis-tick formatting: always compact to keep axes readable. */
export function formatAxis(value: number): string {
  if (Math.abs(value) >= 1000) return compact(value, 1)
  return Number.isInteger(value) ? String(value) : value.toLocaleString(LOCALE, { maximumFractionDigits: 1 })
}

/** Format a category/dimension label, truncating long strings. */
export function formatLabel(v: unknown, max = 18): string {
  const s = v == null ? '' : String(v)
  return s.length > max ? s.slice(0, max - 1) + '…' : s
}
