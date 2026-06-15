import {
  useState, useEffect, useRef, useCallback, useMemo,
} from 'react'
import { useTranslation } from 'react-i18next'
import { useParams, useNavigate } from 'react-router-dom'
import { getDateLocale } from '@kubuno/sdk'
import { format } from 'date-fns'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  ArrowLeft, Star, Plus, Trash2, MoreVertical, Copy,
  Bold, Italic, Underline, AlignLeft, AlignCenter, AlignRight,
  PaintBucket, Type, Hash, ExternalLink, Percent, Euro,
  Grid2x2, ChevronDown, X, UserPlus,
} from 'lucide-react'
import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { spreadsheetsApi, officeApi, SheetData, CellData, SpreadsheetSheet, SheetMeta } from './api'
import { useSystemFonts } from './systemAssets'
import CollaboratorsDialog from './CollaboratorsDialog'
import { FormulaInput } from './FormulaInput'
import { parseRefs, refBounds, nameTokenAt, argContextAt, parseSyntaxArgs } from './formula-refs'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { usePresenceUsers, PresenceAvatarList, userColor, usePublishCursor, RemoteCursors, type PresenceUser } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import { evaluate, formatValue, colToIndex, indexToCol } from './formula-engine'

const rangeAsc  = (a: number, b: number): number[] => { const o: number[] = []; for (let i = a; i <= b; i++) o.push(i); return o }
const rangeDesc = (a: number, b: number): number[] => { const o: number[] = []; for (let i = a; i >= b; i--) o.push(i); return o }

// ── Recopie incrémentée : génère une série en prolongeant le motif des cellules
//    source (nombres, jours de la semaine, mois, dates, préfixe+nombre). ────────
const _nameCache: Record<string, string[]> = {}
function localeNames(lang: string, type: 'weekday' | 'month', variant: 'long' | 'short'): string[] {
  const key = `${lang}|${type}|${variant}`
  if (_nameCache[key]) return _nameCache[key]
  const fmt = new Intl.DateTimeFormat(lang, type === 'weekday' ? { weekday: variant } : { month: variant })
  const out: string[] = []
  if (type === 'weekday') for (let i = 0; i < 7; i++) out.push(fmt.format(new Date(Date.UTC(2024, 0, 1 + i))).replace(/\.$/, '').toLowerCase()) // 2024-01-01 = lundi
  else for (let m = 0; m < 12; m++) out.push(fmt.format(new Date(Date.UTC(2024, m, 1))).replace(/\.$/, '').toLowerCase())
  _nameCache[key] = out
  return out
}
const capLike = (sample: string, s: string) => /^[\p{Lu}]/u.test(sample.trim()) ? s.charAt(0).toUpperCase() + s.slice(1) : s
const lastStep = (a: number[]) => a.length >= 2 ? a[a.length - 1] - a[a.length - 2] : (a.length === 1 ? 0 : 1)

const pad2 = (n: number) => String(Math.abs(n)).padStart(2, '0')
const daysInMonth = (y: number, m: number) => new Date(Date.UTC(y, m, 0)).getUTCDate() // m=1..12
const allEqual = <T,>(a: T[]) => a.every(x => x === a[0])
const consecutiveDiffs = (a: number[]) => a.slice(1).map((x, i) => x - a[i])
// Suite arithmétique « parfaite » (pas constant) ?
const constStep = (a: number[]) => a.length >= 2 && allEqual(consecutiveDiffs(a))

// Génère `count` nombres en prolongeant la tendance : régression linéaire pour
// ≥3 points, série géométrique si rapport constant, sinon pas du dernier écart.
function numberSeries(N: number[], count: number, forward: boolean): number[] {
  const n = N.length
  // Géométrique : ≥3 points, rapport constant ≠ 1, pas non constant.
  if (n >= 3 && N.every(v => v !== 0) && !constStep(N)) {
    const ratios = N.slice(1).map((v, i) => v / N[i])
    if (ratios.every(r => Math.abs(r - ratios[0]) < 1e-9) && Math.abs(ratios[0] - 1) > 1e-9) {
      const r = ratios[0], anchor = forward ? N[n - 1] : N[0]
      return Array.from({ length: count }, (_, k) => forward ? anchor * Math.pow(r, k + 1) : anchor * Math.pow(r, -(k + 1)))
    }
  }
  // Régression linéaire y = a + b·x (x = 0..n-1).
  if (n >= 3) {
    let sx = 0, sy = 0, sxx = 0, sxy = 0
    for (let i = 0; i < n; i++) { sx += i; sy += N[i]; sxx += i * i; sxy += i * N[i] }
    const b = (n * sxy - sx * sy) / (n * sxx - sx * sx), a = (sy - b * sx) / n
    return Array.from({ length: count }, (_, k) => { const x = forward ? (n - 1) + (k + 1) : -(k + 1); return round10(a + b * x) })
  }
  const step = n >= 2 ? N[n - 1] - N[n - 2] : 0, anchor = forward ? N[n - 1] : N[0]
  return Array.from({ length: count }, (_, k) => round10(forward ? anchor + step * (k + 1) : anchor - step * (k + 1)))
}
const round10 = (n: number) => Math.round(n * 1e10) / 1e10

// Date souple : ISO (AAAA-MM-JJ) ou JJ/MM/AAAA. Renvoie {y,m,d,fmt}.
function parseFlexDate(s: string): { y: number; m: number; d: number; fmt: 'iso' | 'dmy' } | null {
  let mt = s.match(/^(\d{4})-(\d{1,2})-(\d{1,2})$/)
  if (mt) return { y: +mt[1], m: +mt[2], d: +mt[3], fmt: 'iso' }
  mt = s.match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/)
  if (mt) return { y: +mt[3], m: +mt[2], d: +mt[1], fmt: 'dmy' }
  return null
}
function fmtFlexDate(y: number, m: number, d: number, fmt: 'iso' | 'dmy'): string {
  const dd = Math.min(d, daysInMonth(y, m)) // évite le 31 février
  return fmt === 'iso' ? `${y}-${pad2(m)}-${pad2(dd)}` : `${pad2(dd)}/${pad2(m)}/${y}`
}

// Découpe une cellule en jetons : nombres, mots (lettres), séparateurs (le reste).
type FillTok = { type: 'num' | 'word' | 'sep'; val: string }
function tokenizeCell(s: string): FillTok[] {
  const m = s.match(/\d+|\p{L}+|[^\d\p{L}]+/gu) || []
  return m.map(v => ({ type: /^\d+$/.test(v) ? 'num' : /^\p{L}+$/u.test(v) ? 'word' : 'sep', val: v }))
}
// Série de noms (jours / mois, variantes longue+courte, locale courante) ou null.
function nameSeries(col: string[], count: number, forward: boolean, lang: string): string[] | null {
  for (const type of ['weekday', 'month'] as const) {
    for (const variant of ['long', 'short'] as const) {
      const names = localeNames(lang, type, variant)
      const idxs = col.map(s => names.indexOf(s.replace(/\.$/, '').toLowerCase()))
      if (idxs.every(i => i >= 0)) {
        const step = lastStep(idxs) || 1, anchor = forward ? idxs[idxs.length - 1] : idxs[0], L = names.length
        return Array.from({ length: count }, (_, k) => { const i = ((((forward ? anchor + step * (k + 1) : anchor - step * (k + 1)) % L) + L) % L); return capLike(col[0], names[i]) })
      }
    }
  }
  return null
}

export function fillSeries(srcVals: (string | number)[], count: number, forward: boolean, lang: string): (string | number)[] | null {
  const strs = srcVals.map(v => String(v).trim())
  const gen = <T,>(f: (k: number) => T): T[] => Array.from({ length: count }, (_, k) => f(k + 1))

  // 1. Nombres purs → tendance (régression / géométrique / arithmétique).
  //    Conserve le remplissage par zéros si présent (ex. 001, 002 → 003).
  const nums = srcVals.map(v => typeof v === 'number' ? v : (strs[srcVals.indexOf(v)] !== '' && !isNaN(Number(v)) ? Number(v) : null))
  if (nums.every(n => n != null) && strs.every(s => /^-?\d+(?:\.\d+)?$/.test(s))) {
    const N = nums as number[]
    const padW = strs.every(s => /^\d+$/.test(s)) && strs.some(s => s.length > 1 && s[0] === '0') ? Math.max(...strs.map(s => s.length)) : 0
    const seq = numberSeries(N, count, forward)
    return padW ? seq.map(n => String(Math.round(n)).padStart(padW, '0')) : seq
  }

  // 2. Heures HH:MM ou HH:MM:SS.
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

  // 3. Dates (ISO ou JJ/MM/AAAA) → pas en mois / années / jours selon le motif.
  const dts = strs.map(parseFlexDate)
  if (dts.every(d => d) && allEqual(dts.map(d => d!.fmt))) {
    const D = dts as { y: number; m: number; d: number; fmt: 'iso' | 'dmy' }[]
    const fmt = D[0].fmt
    const monthIdx = D.map(d => d.y * 12 + (d.m - 1))
    const sameDay = allEqual(D.map(d => d.d))
    const sameMonthDay = allEqual(D.map(d => d.m)) && sameDay
    if (sameDay && constStep(monthIdx) && consecutiveDiffs(monthIdx)[0] % 12 !== 0) {       // série mensuelle
      const step = monthIdx[monthIdx.length - 1] - monthIdx[monthIdx.length - 2], anchor = forward ? monthIdx[monthIdx.length - 1] : monthIdx[0]
      return gen(k => { const mi = forward ? anchor + step * k : anchor - step * k; return fmtFlexDate(Math.floor(mi / 12), (mi % 12) + 1, D[0].d, fmt) })
    }
    if (sameMonthDay && constStep(D.map(d => d.y))) {                                        // série annuelle
      const step = D[D.length - 1].y - D[D.length - 2].y, anchor = forward ? D[D.length - 1].y : D[0].y
      return gen(k => fmtFlexDate(forward ? anchor + step * k : anchor - step * k, D[0].m, D[0].d, fmt))
    }
    const ms = D.map(d => Date.UTC(d.y, d.m - 1, d.d))                                       // série journalière
    const dayStep = ms.length >= 2 ? Math.round((ms[ms.length - 1] - ms[ms.length - 2]) / 86400000) : 1
    const anchor = forward ? ms[ms.length - 1] : ms[0]
    return gen(k => { const dd = new Date(anchor + (forward ? dayStep * k : -dayStep * k) * 86400000); return fmtFlexDate(dd.getUTCFullYear(), dd.getUTCMonth() + 1, dd.getUTCDate(), fmt) })
  }

  // 4. Approche par jetons : on aligne les cellules (mot / nombre / séparateur)
  //    et on prolonge chaque jeton variable indépendamment. Couvre les jours/mois
  //    seuls, le « texte + nombre » (Item1, $5, v1.2.3) ET les SÉRIES COMBINÉES
  //    (ex. « Lundi, 1 » → « Mardi, 2 » → « Mercredi, 3 »).
  const toks = strs.map(tokenizeCell)
  const len = toks[0].length
  if (len > 0 && toks.every(t => t.length === len && t.every((tk, i) => tk.type === toks[0][i].type))) {
    const colSeries: string[][] = []
    const isConst = (col: string[]) => col.length >= 2 && allEqual(col)   // motif fixe seulement si ≥2 valeurs identiques
    let ok = true, anyVaries = false
    for (let i = 0; i < len && ok; i++) {
      const col = toks.map(t => t[i].val)
      const type = toks[0][i].type
      if (isConst(col)) { colSeries.push(Array(count).fill(col[0])); continue }
      if (type === 'sep') {
        if (allEqual(col)) { colSeries.push(Array(count).fill(col[0])); continue }  // séparateur d'une seule cellule
        ok = false; break                                                            // séparateur variable → abandon
      }
      if (type === 'num') {
        const padW = col.some(x => x.length > 1 && x[0] === '0') ? Math.max(...col.map(x => x.length)) : 0
        const Ni = col.map(x => parseInt(x, 10))
        // Source unique : on incrémente de 1 (« Q1 » → Q2, Q3). Sinon : tendance.
        const seq = Ni.length >= 2 ? numberSeries(Ni, count, forward)
          : Array.from({ length: count }, (_, k) => forward ? Ni[0] + (k + 1) : Ni[0] - (k + 1))
        colSeries.push(seq.map(v => { const r = Math.round(v); return padW ? String(Math.max(0, r)).padStart(padW, '0') : String(r) }))
        anyVaries = true
        continue
      }
      // word
      const ws = nameSeries(col, count, forward, lang)
      if (ws) { colSeries.push(ws); anyVaries = true; continue }
      if (allEqual(col)) { colSeries.push(Array(count).fill(col[0])); continue }       // mot constant (non reconnu)
      ok = false; break                                                                // mots variables non reconnus → abandon
    }
    if (ok && anyVaries) return gen(k => colSeries.map(c => c[k - 1]).join(''))
  }
  return null
}

// Décale les références relatives d'une formule de (dCol, dRow) — pour le
// copier/coller et la recopie (les refs `$` restent figées).
function translateFormula(f: string, dCol: number, dRow: number): string {
  return f.replace(/(\$?)([A-Za-z]{1,3})(\$?)(\d+)/g, (_m, ad: string, col: string, ar: string, row: string) => {
    let c = colToIndex(col.toUpperCase()) + (ad ? 0 : dCol)
    let r = parseInt(row) + (ar ? 0 : dRow)
    if (c < 0 || r < 1) return '#REF!'
    return `${ad}${indexToCol(c)}${ar}${r}`
  })
}
import { Dropdown, Button, StartPage, ColorField, GradientField, gradientToCss, DEFAULT_GRADIENT, ColorSwatchPicker, AnchoredPopover } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_SPREADSHEET } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { ModuleFileBrowser } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
// ── Formula autocomplete ──────────────────────────────────────────────────────

const FORMULA_FUNCTIONS = [
  { name: 'SUM',        descKey: 'sheet_fn_sum',       syntax: 'SUM(nombre1, ...)' },
  { name: 'AVERAGE',    descKey: 'sheet_fn_average',   syntax: 'AVERAGE(nombre1, ...)' },
  { name: 'COUNT',      descKey: 'sheet_fn_count',     syntax: 'COUNT(val1, ...)' },
  { name: 'COUNTA',     descKey: 'sheet_fn_counta',    syntax: 'COUNTA(val1, ...)' },
  { name: 'MIN',        descKey: 'sheet_fn_min',       syntax: 'MIN(nombre1, ...)' },
  { name: 'MAX',        descKey: 'sheet_fn_max',       syntax: 'MAX(nombre1, ...)' },
  { name: 'IF',         descKey: 'sheet_fn_if',        syntax: 'IF(condition, si_vrai, si_faux)' },
  { name: 'IFERROR',    descKey: 'sheet_fn_iferror',   syntax: 'IFERROR(valeur, si_erreur)' },
  { name: 'AND',        descKey: 'sheet_fn_and',       syntax: 'AND(val1, val2, ...)' },
  { name: 'OR',         descKey: 'sheet_fn_or',        syntax: 'OR(val1, val2, ...)' },
  { name: 'NOT',        descKey: 'sheet_fn_not',       syntax: 'NOT(valeur)' },
  { name: 'ROUND',      descKey: 'sheet_fn_round',     syntax: 'ROUND(nombre, décimales)' },
  { name: 'FLOOR',      descKey: 'sheet_fn_floor',     syntax: 'FLOOR(nombre, [multiple])' },
  { name: 'CEILING',    descKey: 'sheet_fn_ceiling',   syntax: 'CEILING(nombre, [multiple])' },
  { name: 'ABS',        descKey: 'sheet_fn_abs',       syntax: 'ABS(nombre)' },
  { name: 'MOD',        descKey: 'sheet_fn_mod',       syntax: 'MOD(nombre, diviseur)' },
  { name: 'POWER',      descKey: 'sheet_fn_power',     syntax: 'POWER(nombre, exposant)' },
  { name: 'SQRT',       descKey: 'sheet_fn_sqrt',      syntax: 'SQRT(nombre)' },
  { name: 'SUMIF',      descKey: 'sheet_fn_sumif',     syntax: 'SUMIF(plage, critère, [plage_somme])' },
  { name: 'COUNTIF',    descKey: 'sheet_fn_countif',   syntax: 'COUNTIF(plage, critère)' },
  { name: 'AVERAGEIF',  descKey: 'sheet_fn_averageif', syntax: 'AVERAGEIF(plage, critère, [plage_moy])' },
  { name: 'VLOOKUP',    descKey: 'sheet_fn_vlookup',   syntax: 'VLOOKUP(valeur, plage, colonne, [exact])' },
  { name: 'HLOOKUP',    descKey: 'sheet_fn_hlookup',   syntax: 'HLOOKUP(valeur, plage, ligne, [exact])' },
  { name: 'INDEX',      descKey: 'sheet_fn_index',     syntax: 'INDEX(plage, ligne, [colonne])' },
  { name: 'MATCH',      descKey: 'sheet_fn_match',     syntax: 'MATCH(valeur, plage, [type])' },
  { name: 'LEN',        descKey: 'sheet_fn_len',       syntax: 'LEN(texte)' },
  { name: 'LEFT',       descKey: 'sheet_fn_left',      syntax: 'LEFT(texte, [nb_car])' },
  { name: 'RIGHT',      descKey: 'sheet_fn_right',     syntax: 'RIGHT(texte, [nb_car])' },
  { name: 'MID',        descKey: 'sheet_fn_mid',       syntax: 'MID(texte, début, nb_car)' },
  { name: 'TRIM',       descKey: 'sheet_fn_trim',      syntax: 'TRIM(texte)' },
  { name: 'UPPER',      descKey: 'sheet_fn_upper',     syntax: 'UPPER(texte)' },
  { name: 'LOWER',      descKey: 'sheet_fn_lower',     syntax: 'LOWER(texte)' },
  { name: 'CONCAT',     descKey: 'sheet_fn_concat',    syntax: 'CONCAT(texte1, texte2, ...)' },
  { name: 'TEXT',       descKey: 'sheet_fn_text',      syntax: 'TEXT(valeur, format)' },
  { name: 'TODAY',      descKey: 'sheet_fn_today',     syntax: 'TODAY()' },
  { name: 'NOW',        descKey: 'sheet_fn_now',       syntax: 'NOW()' },
  { name: 'YEAR',       descKey: 'sheet_fn_year',      syntax: 'YEAR(date)' },
  { name: 'MONTH',      descKey: 'sheet_fn_month',     syntax: 'MONTH(date)' },
  { name: 'DAY',        descKey: 'sheet_fn_day',       syntax: 'DAY(date)' },
  { name: 'ISBLANK',    descKey: 'sheet_fn_isblank',   syntax: 'ISBLANK(valeur)' },
  { name: 'ISNUMBER',   descKey: 'sheet_fn_isnumber',  syntax: 'ISNUMBER(valeur)' },
  { name: 'ISTEXT',     descKey: 'sheet_fn_istext',    syntax: 'ISTEXT(valeur)' },
]
type FormulaFn = typeof FORMULA_FUNCTIONS[number]

// Catégorie de chaque fonction → couleur « pertinente » dans l'autocomplétion
// (math = bleu, stat = cyan, logique = violet, recherche = vert, texte = orange,
//  date = rouge, info = brun). Donne un repère visuel comme dans Google Sheets.
type FnCat = 'math' | 'stat' | 'logical' | 'lookup' | 'text' | 'date' | 'info'
const FN_CATEGORY: Record<string, FnCat> = {
  SUM: 'math', ROUND: 'math', FLOOR: 'math', CEILING: 'math', ABS: 'math', MOD: 'math', POWER: 'math', SQRT: 'math',
  AVERAGE: 'stat', COUNT: 'stat', COUNTA: 'stat', MIN: 'stat', MAX: 'stat', SUMIF: 'stat', COUNTIF: 'stat', AVERAGEIF: 'stat',
  IF: 'logical', IFERROR: 'logical', AND: 'logical', OR: 'logical', NOT: 'logical',
  VLOOKUP: 'lookup', HLOOKUP: 'lookup', INDEX: 'lookup', MATCH: 'lookup',
  LEN: 'text', LEFT: 'text', RIGHT: 'text', MID: 'text', TRIM: 'text', UPPER: 'text', LOWER: 'text', CONCAT: 'text', TEXT: 'text',
  TODAY: 'date', NOW: 'date', YEAR: 'date', MONTH: 'date', DAY: 'date',
  ISBLANK: 'info', ISNUMBER: 'info', ISTEXT: 'info',
}
const CAT_COLOR: Record<FnCat, string> = {
  math:    '#1a73e8',
  stat:    '#12b5cb',
  logical: '#9334e6',
  lookup:  '#188038',
  text:    '#e8710a',
  date:    '#d93025',
  info:    '#795548',
}
const fnColor = (name: string): string => CAT_COLOR[FN_CATEGORY[name] ?? 'math']

// ── Constants ─────────────────────────────────────────────────────────────────

const MAX_COLS = 26
const MAX_ROWS = 200
const DEFAULT_COL_WIDTH = 100
const DEFAULT_ROW_HEIGHT = 24
const ROW_HEADER_WIDTH = 46
const COL_HEADER_HEIGHT = 24

const COLS = Array.from({ length: MAX_COLS }, (_, i) => String.fromCharCode(65 + i))
const ROWS = Array.from({ length: MAX_ROWS }, (_, i) => i + 1)

// ── Helpers ───────────────────────────────────────────────────────────────────

function cellKey(col: string, row: number) { return `${col}${row}` }

function cellDisplay(cell: CellData | undefined): string {
  if (!cell) return ''
  if (cell.v === null || cell.v === undefined) return ''
  return String(cell.v)
}

// Valeur numérique d'une cellule (formule évaluée ou nombre brut), sinon null.
function numericValue(cell: CellData | undefined, data: SheetData): number | null {
  if (!cell) return null
  if (cell.f && cell.f.startsWith('=')) {
    const v = evaluate(cell.f, data)
    const s = Array.isArray(v) ? v[0]?.[0] : v
    return typeof s === 'number' ? s : null
  }
  if (typeof cell.v === 'number') return cell.v
  if (typeof cell.v === 'string' && cell.v !== '' && !isNaN(Number(cell.v))) return Number(cell.v)
  return null
}

// Applique le format numérique d'une cellule (devise / pourcentage / décimales /
// séparateur de milliers) via Intl, en respectant la langue de l'app.
function formatNumber(n: number, s: NonNullable<CellData['s']>, lang: string): string {
  const dec = s.decimals
  const fmt = s.numFmt
  if (fmt === 'scientific') return n.toExponential(dec ?? 2)
  const opts: Intl.NumberFormatOptions = { useGrouping: s.thousands ?? (fmt === 'currency' || fmt === 'number') }
  if (dec != null) { opts.minimumFractionDigits = dec; opts.maximumFractionDigits = dec }
  else { opts.maximumFractionDigits = (fmt === 'currency' || fmt === 'percent') ? 2 : 10; if (fmt === 'currency') opts.minimumFractionDigits = 2 }
  if (fmt === 'currency') { opts.style = 'currency'; opts.currency = 'EUR' }
  else if (fmt === 'percent') { opts.style = 'percent' }
  try { return new Intl.NumberFormat(lang, opts).format(n) } catch { return String(n) }
}

function resolveValue(cell: CellData | undefined, data: SheetData): string {
  if (!cell) return ''
  if (cell.f && cell.f.startsWith('=')) {
    return formatValue(evaluate(cell.f, data))
  }
  return cellDisplay(cell)
}

// ── Cell component ────────────────────────────────────────────────────────────

interface CellProps {
  col: string
  row: number
  data: SheetData
  selected: boolean
  inRange: boolean
  editing: boolean
  colWidth: number
  rowHeight: number
  onClick: (col: string, row: number, e: React.MouseEvent) => void
  onDoubleClick: (col: string, row: number) => void
  onMouseDown: (col: string, row: number, e: React.MouseEvent) => void
  onMouseEnter: (col: string, row: number) => void
  onEditCommit: (col: string, row: number, val: string) => void
  onEditAbort: () => void
  editValue: string
  onEditChange: (val: string, el: HTMLInputElement) => void
  onEditSelect: (el: HTMLInputElement) => void
  assistKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => boolean
  onArrow: (dir: 'up' | 'down' | 'left' | 'right') => void
  onTab: (shift: boolean) => void
  isFillCorner?: boolean
  onFillStart?: () => void
}

// Glyphes du sélecteur de bordures : grille grise faible + arêtes actives en bleu.
function borderGlyph(edges: { t?: boolean; r?: boolean; b?: boolean; l?: boolean; inner?: boolean }) {
  const A = '#1a73e8', G = '#bdc1c6'
  return (
    <svg width="18" height="18" viewBox="0 0 18 18">
      {/* contour faible */}
      <rect x="2" y="2" width="14" height="14" fill="none" stroke={G} strokeWidth="1" />
      {/* arêtes actives */}
      {edges.t && <line x1="2" y1="2"  x2="16" y2="2"  stroke={A} strokeWidth="2" />}
      {edges.b && <line x1="2" y1="16" x2="16" y2="16" stroke={A} strokeWidth="2" />}
      {edges.l && <line x1="2" y1="2"  x2="2"  y2="16" stroke={A} strokeWidth="2" />}
      {edges.r && <line x1="16" y1="2" x2="16" y2="16" stroke={A} strokeWidth="2" />}
      {edges.inner && <>
        <line x1="9" y1="2" x2="9" y2="16" stroke={A} strokeWidth="2" />
        <line x1="2" y1="9" x2="16" y2="9" stroke={A} strokeWidth="2" />
      </>}
    </svg>
  )
}
const BorderIcon = {
  all:    borderGlyph({ t: true, r: true, b: true, l: true, inner: true }),
  inner:  borderGlyph({ inner: true }),
  outer:  borderGlyph({ t: true, r: true, b: true, l: true }),
  none:   borderGlyph({}),
  top:    borderGlyph({ t: true }),
  bottom: borderGlyph({ b: true }),
  left:   borderGlyph({ l: true }),
  right:  borderGlyph({ r: true }),
}

function Cell({
  col, row, data, selected, inRange, editing, colWidth, rowHeight,
  onClick, onDoubleClick, onMouseDown, onMouseEnter,
  onEditCommit, onEditAbort, editValue, onEditChange, onEditSelect, assistKeyDown, onArrow, onTab,
  isFillCorner, onFillStart,
}: CellProps) {
  const { i18n } = useTranslation('office')
  const key = cellKey(col, row)
  const cell = data.cells[key]
  const style = cell?.s ?? {}
  const num = numericValue(cell, data)
  const display = (num != null && (style.numFmt || style.decimals != null || style.thousands))
    ? formatNumber(num, style, i18n.language)
    : resolveValue(cell, data)
  // Convention tableur : les nombres s'alignent à droite par défaut, le texte à gauche.
  const isNumeric = num != null
  const effAlign = style.align ?? (isNumeric ? 'right' : 'left')

  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (editing && inputRef.current) {
      const el = inputRef.current
      el.focus()
      const n = el.value.length
      el.setSelectionRange(n, n)   // caret en fin (compatible avec la saisie directe d'un caractère)
    }
  }, [editing])

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (assistKeyDown(e)) return   // autocomplétion ouverte : ↑↓/Tab/Entrée/Échap lui reviennent
    if (e.key === 'Enter') { onEditCommit(col, row, e.currentTarget.value); e.preventDefault() }
    if (e.key === 'Escape') { onEditAbort(); e.preventDefault() }
    if (e.key === 'Tab') { onEditCommit(col, row, e.currentTarget.value); onTab(e.shiftKey); e.preventDefault() }
    if (e.key === 'ArrowUp')    { onEditCommit(col, row, e.currentTarget.value); onArrow('up');    e.preventDefault() }
    if (e.key === 'ArrowDown')  { onEditCommit(col, row, e.currentTarget.value); onArrow('down');  e.preventDefault() }
    if (e.key === 'ArrowLeft' && e.currentTarget.selectionStart === 0)
      { onEditCommit(col, row, e.currentTarget.value); onArrow('left');  e.preventDefault() }
    if (e.key === 'ArrowRight' && e.currentTarget.selectionStart === e.currentTarget.value.length)
      { onEditCommit(col, row, e.currentTarget.value); onArrow('right'); e.preventDefault() }
  }

  const bgColor = editing
    ? 'white'
    : inRange
      ? (selected ? '#c2d7fd' : '#e8f0fe')
      : (style.bg ?? 'white')

  const td: React.CSSProperties = {
    width: colWidth,
    minWidth: colWidth,
    maxWidth: colWidth,
    height: rowHeight,
    // Lignes de grille par défaut, surchargées par les bordures explicites de la cellule.
    borderTop:    style.bt ? `1px solid ${style.bt}` : '1px solid transparent',
    borderLeft:   style.bl ? `1px solid ${style.bl}` : '1px solid transparent',
    borderRight:  style.br ? `1px solid ${style.br}` : '1px solid #e2e4e6',
    borderBottom: style.bb ? `1px solid ${style.bb}` : '1px solid #e2e4e6',
    position: 'relative',
    overflow: 'hidden',
    boxSizing: 'border-box',
    backgroundColor: bgColor,
    // Dégradé de remplissage (sauf pendant l'édition / la sélection en surbrillance).
    backgroundImage: (!editing && !inRange && style.bgGradient) ? gradientToCss(style.bgGradient) : undefined,
    outline: selected && !editing ? '2px solid #1a73e8' : 'none',
    outlineOffset: '-2px',
    userSelect: 'none',
  }

  const textStyle: React.CSSProperties = {
    fontWeight: style.bold ? 'bold' : undefined,
    fontStyle: style.italic ? 'italic' : undefined,
    textDecoration: [style.underline && 'underline', style.strike && 'line-through']
      .filter(Boolean).join(' ') || undefined,
    fontSize: style.fontSize ? `${style.fontSize}px` : '13px',
    fontFamily: style.fontFamily || undefined,
    color: style.color ?? '#202124',
    textAlign: effAlign,
    whiteSpace: style.wrap ? 'normal' : 'nowrap',
  }

  return (
    <td
      style={td}
      onClick={e => onClick(col, row, e)}
      onDoubleClick={() => onDoubleClick(col, row)}
      onMouseDown={e => onMouseDown(col, row, e)}
      onMouseEnter={() => onMouseEnter(col, row)}
    >
      {editing ? (
        <FormulaInput
          inputRef={inputRef}
          value={editValue}
          onChange={e => onEditChange(e.target.value, e.target)}
          onSelect={e => onEditSelect(e.currentTarget)}
          onKeyDown={handleKeyDown}
          onBlur={e => onEditCommit(col, row, e.target.value)}
          containerStyle={{
            position: 'absolute', inset: 0, zIndex: 10,
            border: '2px solid #1a73e8', background: 'white', boxSizing: 'border-box',
          }}
          inputStyle={{
            width: '100%', height: '100%', padding: '0 4px',
            fontSize: 13, fontFamily: 'inherit', border: 'none', outline: 'none', boxSizing: 'border-box',
          }}
        />
      ) : (
        <div style={{
          ...textStyle,
          padding: '0 4px',
          overflow: 'hidden',
          textOverflow: style.wrap ? 'clip' : 'ellipsis',
          height: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: effAlign === 'right' ? 'flex-end' : effAlign === 'center' ? 'center' : 'flex-start',
        }}>
          {display}
        </div>
      )}
      {isFillCorner && !editing && (
        <div
          onMouseDown={(e) => { e.stopPropagation(); e.preventDefault(); onFillStart?.() }}
          style={{
            position: 'absolute', right: -3, bottom: -3, width: 7, height: 7,
            background: '#1a73e8', border: '1px solid white', borderRadius: 1,
            cursor: 'crosshair', zIndex: 15,
          }}
        />
      )}
    </td>
  )
}

// ── Main SpreadsheetEditor ────────────────────────────────────────────────────

interface SpreadsheetEditorProps {
  ssId: string
  sheetMetas: SheetMeta[]
  onSheetMetasChange: (metas: SheetMeta[]) => void
  onSavingChange?: (saving: boolean) => void   // remonte l'état d'enregistrement à la topbar
  onPresenceChange?: (users: PresenceUser[]) => void  // remonte les avatars de présence à la topbar
}

const SHEET_FONTS = ['Arial', 'Helvetica', 'Times New Roman', 'Courier New', 'Georgia', 'Verdana']

function SpreadsheetEditor({ ssId, sheetMetas, onSheetMetasChange, onSavingChange, onPresenceChange }: SpreadsheetEditorProps) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const fontFamilies = useSystemFonts(SHEET_FONTS)
  const [activeSheetId, setActiveSheetId] = useState<string>(sheetMetas[0]?.id ?? '')

  // ── Collaboration temps réel (Yjs) ──────────────────────────────────────────
  // Un Y.Doc par tableur ; les cellules de chaque feuille vivent dans un Y.Map
  // `cells:<sheetId>`. Présence (sélection) via awareness. Room ACL = owner.
  const ydoc = useMemo(() => new Y.Doc(), [ssId])
  const awareness = useMemo(() => new Awareness(ydoc), [ydoc])
  const authUser = useAuthStore(s => s.user)
  const [collabEmpty, setCollabEmpty] = useState<boolean | null>(null)
  useEffect(() => { setCollabEmpty(null) }, [ssId])
  useCollab(`office-spreadsheet:${ssId}`, ydoc, !!ssId, { awareness, onSync: setCollabEmpty })
  useEffect(() => () => awareness.destroy(), [awareness])
  useEffect(() => {
    if (!authUser) return
    awareness.setLocalStateField('user', {
      id: authUser.id, name: authUser.display_name || authUser.username || authUser.email,
      color: userColor(authUser.id), avatar: authUser.avatar_url,
    })
  }, [awareness, authUser])
  // Présence : curseur souris (coords contenu de la grille, scroll inclus).
  const publishCursor = usePublishCursor(awareness)
  const gridRef = useRef<HTMLDivElement>(null)
  const onGridCursor = useCallback((e: React.MouseEvent) => {
    const el = gridRef.current
    if (!el) return
    const r = el.getBoundingClientRect()
    publishCursor({ x: e.clientX - r.left + el.scrollLeft, y: e.clientY - r.top + el.scrollTop })
  }, [publishCursor])
  const cellsMap = useMemo(() => ydoc.getMap<CellData>(`cells:${activeSheetId}`), [ydoc, activeSheetId])
  // Remonte les avatars de présence à la topbar (WorkspaceShell). `presenceUsers`
  // est un nouveau tableau à chaque rendu → on ne remonte qu'au changement de contenu
  // (sinon boucle de rendu parent↔enfant).
  const presenceUsers = usePresenceUsers(awareness, awareness.clientID)
  const lastPresenceRef = useRef('')
  useEffect(() => {
    const json = JSON.stringify(presenceUsers)
    if (json !== lastPresenceRef.current) { lastPresenceRef.current = json; onPresenceChange?.(presenceUsers) }
  })

  // Selection: anchor + range end (for multi-cell selection)
  const [selectedCell, setSelectedCell] = useState<{ col: string; row: number } | null>(null)
  const [rangeEnd,     setRangeEnd]     = useState<{ col: string; row: number } | null>(null)
  const isDragSelecting = useRef(false)
  // Presse-papier interne (copier/couper une plage) + poignée de recopie.
  const clipboard = useRef<{ cells: (CellData | undefined)[][]; rows: number; cols: number; cut: boolean; originCol: number; originRow: number } | null>(null)
  const fillStart = useRef<{ c1: number; c2: number; r1: number; r2: number } | null>(null)
  const [fillTo, setFillTo] = useState<{ col: string; row: number } | null>(null)
  const isFilling = useRef(false)
  const performFillRef = useRef<() => void>(() => {})

  const [editingCell,      setEditingCell]      = useState<{ col: string; row: number } | null>(null)
  const [cellDraft,        setCellDraft]        = useState('')  // valeur live de la cellule en édition (contrôlée)
  const [editingSheetName, setEditingSheetName] = useState<string | null>(null)
  const [sheetNameDraft,   setSheetNameDraft]   = useState('')

  // Formula bar state (independent from cell editing)
  const [fbDraft, setFbDraft] = useState('')
  const formulaBarRef = useRef<HTMLInputElement>(null)
  const fbActiveRef = useRef(false)

  const [barFocused, setBarFocused] = useState(false)

  // Autocomplete / assistance de formule (barre OU cellule)
  const [acOpen,        setAcOpen]        = useState(false)
  const [acSuggestions, setAcSuggestions] = useState<FormulaFn[]>([])
  const [acIdx,         setAcIdx]         = useState(0)
  const [argInfo,       setArgInfo]       = useState<{ name: string; argIndex: number } | null>(null)
  const [assistPos,     setAssistPos]     = useState<{ left: number; top: number } | null>(null)
  const activeInputRef  = useRef<HTMLInputElement | null>(null)
  const activeSetterRef = useRef<(v: string) => void>(() => {})

  // Column / row resize
  const [localColWidths,  setLocalColWidths]  = useState<Record<string, number>>({})
  const [localRowHeights, setLocalRowHeights] = useState<Record<string, number>>({})
  const [bordersOpen, setBordersOpen] = useState(false)
  const [fillOpen, setFillOpen] = useState(false)
  const [textColorOpen, setTextColorOpen] = useState(false)
  const textColorBtnRef = useRef<HTMLButtonElement>(null)
  const resizingCol = useRef<{ col: string; startX: number; startW: number } | null>(null)
  const resizingRow = useRef<{ row: number; startY: number; startH: number } | null>(null)
  const resizeCursor = resizingCol.current ? 'col-resize' : resizingRow.current ? 'row-resize' : undefined

  const sheetQuery = useQuery({
    queryKey: ['spreadsheet-sheet', ssId, activeSheetId],
    queryFn:  () => spreadsheetsApi.getSheet(ssId, activeSheetId),
    enabled:  !!activeSheetId,
    staleTime: Infinity,
    // On gère l'état localement (optimiste) + autosauvegarde ; un refetch au
    // retour sur l'onglet écraserait les saisies non encore sauvegardées.
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  })

  const sheet = sheetQuery.data
  const sheetData: SheetData = sheet?.data ?? { cells: {} }
  const sheetDataRef = useRef(sheetData); sheetDataRef.current = sheetData

  const getColWidth = (col: string) => localColWidths[col] ?? DEFAULT_COL_WIDTH
  const getRowHeight = (row: number) => localRowHeights[String(row)] ?? DEFAULT_ROW_HEIGHT

  // Position en pixels (espace de la grille scrollable) d'une colonne/ligne.
  const colX = (c0: number) => { let x = ROW_HEADER_WIDTH; for (let c = 0; c < c0; c++) x += getColWidth(COLS[c]); return x }
  const rowY = (r1: number) => { let y = COL_HEADER_HEIGHT; for (let r = 1; r < r1; r++) y += getRowHeight(r); return y }
  const colsW = (c1: number, c2: number) => { let w = 0; for (let c = c1; c <= c2; c++) w += getColWidth(COLS[c]); return w }
  const rowsH = (r1: number, r2: number) => { let h = 0; for (let r = r1; r <= r2; r++) h += getRowHeight(r); return h }

  // Formule en cours d'édition (barre OU cellule) → encadrés colorés des plages.
  const editingFormula =
    editingCell ? cellDraft :
    (barFocused && fbDraft.startsWith('=') ? fbDraft : '')
  const refHighlights = (editingFormula.startsWith('=') ? parseRefs(editingFormula) : [])
    .map(r => {
      const b = refBounds(r.text)
      if (!b || b.c1 >= MAX_COLS || b.r1 > MAX_ROWS) return null
      const c2 = Math.min(b.c2, MAX_COLS - 1), r2 = Math.min(b.r2, MAX_ROWS)
      return { color: r.color, left: colX(b.c1), top: rowY(b.r1), width: colsW(b.c1, c2), height: rowsH(b.r1, r2) }
    })
    .filter((x): x is { color: string; left: number; top: number; width: number; height: number } => x !== null)

  // Sélections des autres participants (présence) → cadres colorés + étiquette nom.
  // `presenceUsers` (usePresenceUsers) force déjà un re-render à chaque changement d'awareness.
  const remoteSelections: { color: string; name: string; left: number; top: number; width: number; height: number }[] = []
  awareness.getStates().forEach((st, cid) => {
    if (cid === awareness.clientID) return
    const s = st as { user?: { name: string; color: string }; sel?: { col: string; row: number; sheet?: string } }
    if (!s.user || !s.sel || (s.sel.sheet && s.sel.sheet !== activeSheetId)) return
    const ci = COLS.indexOf(s.sel.col)
    if (ci < 0 || s.sel.row < 1 || s.sel.row > MAX_ROWS) return
    remoteSelections.push({
      color: s.user.color, name: s.user.name,
      left: colX(ci), top: rowY(s.sel.row), width: getColWidth(s.sel.col), height: getRowHeight(s.sel.row),
    })
  })

  // Init local sizes from server
  useEffect(() => {
    if (sheet) {
      setLocalColWidths(sheet.col_widths ?? {})
      setLocalRowHeights(sheet.row_heights ?? {})
    }
  }, [sheet?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const saveMut = useMutation({
    mutationFn: (data: SheetData) => spreadsheetsApi.updateSheet(ssId, activeSheetId, { data }),
    onSuccess: (updated) => {
      qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], updated)
    },
  })

  // Remonte l'état d'enregistrement au parent (affiché dans la topbar WorkspaceShell).
  useEffect(() => { onSavingChange?.(saveMut.isPending) }, [saveMut.isPending, onSavingChange])

  const saveDimensionsMut = useMutation({
    mutationFn: (dims: { col_widths?: Record<string, number>; row_heights?: Record<string, number> }) =>
      spreadsheetsApi.updateSheet(ssId, activeSheetId, dims),
    onSuccess: (updated) => {
      qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], updated)
    },
  })

  const pendingDataRef = useRef<SheetData | null>(null)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const schedulesSave = useCallback((data: SheetData) => {
    pendingDataRef.current = data
    if (saveTimer.current) clearTimeout(saveTimer.current)
    saveTimer.current = setTimeout(() => {
      if (pendingDataRef.current) { saveMut.mutate(pendingDataRef.current); pendingDataRef.current = null }
    }, 1200)
  }, [saveMut])

  // ── Écriture centralisée : cache RQ (rendu) + Y.Map (collab) + sauvegarde ──────
  // `applyingRemote` empêche de re-diffuser un changement reçu d'un pair.
  const applyingRemote = useRef(false)
  const commitData = useCallback((newData: SheetData) => {
    qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], (old: SpreadsheetSheet | undefined) =>
      old ? { ...old, data: newData } : old)
    schedulesSave(newData)
    if (!applyingRemote.current) {
      // Diff contre la vue PRÉCÉDENTE (pas la map entière) : on ne touche QUE les
      // cellules que ce client a réellement changées/supprimées → ne piétine jamais
      // les cellules ajoutées par un autre participant.
      const prev = sheetDataRef.current.cells
      ydoc.transact(() => {
        for (const [k, c] of Object.entries(newData.cells)) {
          const cj = JSON.stringify(c)
          if (JSON.stringify(prev[k]) !== cj) cellsMap.set(k, JSON.parse(cj))  // JSON pur (pas d'`undefined`) → encodable par Yjs
        }
        for (const k of Object.keys(prev)) if (!(k in newData.cells)) cellsMap.delete(k)
      }, 'local')
    }
  }, [qc, ssId, activeSheetId, schedulesSave, ydoc, cellsMap])

  // Changements distants (autre participant) → reconstruit les cellules dans le cache RQ.
  useEffect(() => {
    const mirror = () => {
      const cells: Record<string, CellData> = {}
      cellsMap.forEach((v, k) => { cells[k] = v })
      applyingRemote.current = true
      qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], (old: SpreadsheetSheet | undefined) =>
        old ? { ...old, data: { ...old.data, cells } } : old)
      applyingRemote.current = false
    }
    const handler = (_e: Y.YMapEvent<CellData>, txn: Y.Transaction) => { if (txn.origin !== 'local') mirror() }
    cellsMap.observe(handler)
    return () => cellsMap.unobserve(handler)
  }, [cellsMap, qc, ssId, activeSheetId])

  // Seed : à la 1ʳᵉ synchro, si la salle Yjs n'a pas (encore) cette feuille mais que
  // le backend a des cellules → on alimente le Y.Map ; sinon on adopte le Y.Map.
  const seededSheets = useRef<Set<string>>(new Set())
  useEffect(() => {
    if (collabEmpty === null || !sheet) return
    if (seededSheets.current.has(activeSheetId)) return
    seededSheets.current.add(activeSheetId)
    const backendCells = (sheet.data?.cells) ?? {}
    if (cellsMap.size === 0 && Object.keys(backendCells).length > 0) {
      ydoc.transact(() => { for (const [k, c] of Object.entries(backendCells)) cellsMap.set(k, JSON.parse(JSON.stringify(c))) }, 'local')
    } else if (cellsMap.size > 0) {
      const cells: Record<string, CellData> = {}
      cellsMap.forEach((v, k) => { cells[k] = v })
      applyingRemote.current = true
      qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], (old: SpreadsheetSheet | undefined) =>
        old ? { ...old, data: { ...old.data, cells } } : old)
      applyingRemote.current = false
    }
  }, [collabEmpty, sheet, activeSheetId, cellsMap, ydoc, qc, ssId])

  // Présence : publie la cellule sélectionnée (+ feuille) pour les autres participants.
  useEffect(() => {
    awareness.setLocalStateField('sel', selectedCell ? { col: selectedCell.col, row: selectedCell.row, sheet: activeSheetId } : null)
  }, [selectedCell, activeSheetId, awareness])

  // Vide la sauvegarde différée immédiatement (avant de quitter / masquer l'onglet)
  // pour ne pas perdre les saisies non encore envoyées.
  const flushSave = useCallback(() => {
    if (saveTimer.current) { clearTimeout(saveTimer.current); saveTimer.current = null }
    if (pendingDataRef.current) { saveMut.mutate(pendingDataRef.current); pendingDataRef.current = null }
  }, [saveMut])

  useEffect(() => {
    const onHide = () => { if (document.visibilityState === 'hidden') flushSave() }
    document.addEventListener('visibilitychange', onHide)
    window.addEventListener('pagehide', flushSave)
    window.addEventListener('beforeunload', flushSave)
    return () => {
      document.removeEventListener('visibilitychange', onHide)
      window.removeEventListener('pagehide', flushSave)
      window.removeEventListener('beforeunload', flushSave)
      flushSave() // flush au démontage (changement de feuille / sortie de l'éditeur)
    }
  }, [flushSave])

  const updateCell = useCallback((col: string, row: number, raw: string) => {
    const key = cellKey(col, row)
    const isFormula = raw.startsWith('=')
    let v: string | number | null = raw
    if (!isFormula && raw !== '') {
      const n = Number(raw)
      if (!isNaN(n) && raw.trim() !== '') v = n
    }
    if (raw === '') v = null

    const newData: SheetData = {
      ...sheetData,
      cells: {
        ...sheetData.cells,
        [key]: { ...sheetData.cells[key], v: isFormula ? undefined : v, f: isFormula ? raw : undefined },
      },
    }
    commitData(newData)
  }, [sheetData, commitData])

  // ── Selection helpers ───────────────────────────────────────────────────────

  // Coin bas-droit de la sélection (emplacement de la poignée de recopie).
  const selEndCorner = rangeEnd ?? selectedCell
  const fillCornerCol = selectedCell ? COLS[Math.max(COLS.indexOf(selectedCell.col), COLS.indexOf(selEndCorner!.col))] : null
  const fillCornerRow = selectedCell ? Math.max(selectedCell.row, selEndCorner!.row) : null

  function isInSelection(col: string, row: number): boolean {
    if (!selectedCell) return false
    const end = rangeEnd ?? selectedCell
    const c1 = Math.min(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col))
    const c2 = Math.max(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col))
    const r1 = Math.min(selectedCell.row, end.row)
    const r2 = Math.max(selectedCell.row, end.row)
    const ci = COLS.indexOf(col)
    if (ci >= c1 && ci <= c2 && row >= r1 && row <= r2) return true
    // Aperçu de la zone de recopie pendant le glissé de la poignée.
    if (fillTo && fillStart.current) {
      const fs = fillStart.current, fc = COLS.indexOf(fillTo.col)
      const fc1 = Math.min(fs.c1, fc), fc2 = Math.max(fs.c2, fc)
      const fr1 = Math.min(fs.r1, fillTo.row), fr2 = Math.max(fs.r2, fillTo.row)
      if (ci >= fc1 && ci <= fc2 && row >= fr1 && row <= fr2) return true
    }
    return false
  }

  function isColHighlighted(col: string): boolean {
    if (!selectedCell) return false
    const end = rangeEnd ?? selectedCell
    const c1 = Math.min(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col))
    const c2 = Math.max(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col))
    return COLS.indexOf(col) >= c1 && COLS.indexOf(col) <= c2
  }

  function isRowHighlighted(row: number): boolean {
    if (!selectedCell) return false
    const end = rangeEnd ?? selectedCell
    const r1 = Math.min(selectedCell.row, end.row)
    const r2 = Math.max(selectedCell.row, end.row)
    return row >= r1 && row <= r2
  }

  // ── Formula bar sync ────────────────────────────────────────────────────────

  useEffect(() => {
    if (fbActiveRef.current) return // don't override while user is typing in formula bar
    const val = selectedCell
      ? (sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]?.f
        ?? cellDisplay(sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]))
      : ''
    setFbDraft(val)
  }, [selectedCell, sheetData])

  // ── Cell interactions ───────────────────────────────────────────────────────

  // Valide la saisie de la cellule en cours d'édition (les `<td>` ne sont pas
  // focusables → cliquer ailleurs ne déclenche pas le `blur` de l'input ; on
  // committe donc explicitement le brouillon avant de quitter la cellule).
  const commitEditingDraft = () => {
    if (!editingCell) return
    updateCell(editingCell.col, editingCell.row, cellDraft)
    setEditingCell(null)
  }

  const handleCellClick = (col: string, row: number, e: React.MouseEvent) => {
    commitEditingDraft()
    if (e.shiftKey && selectedCell) {
      setRangeEnd({ col, row })
    } else {
      setSelectedCell({ col, row })
      setRangeEnd(null)
    }
  }

  const handleCellMouseDown = (col: string, row: number, e: React.MouseEvent) => {
    if (e.shiftKey) return // handled by onClick
    if (e.button !== 0) return
    commitEditingDraft()
    isDragSelecting.current = true
    setSelectedCell({ col, row })
    setRangeEnd(null)
  }

  const handleCellMouseEnter = (col: string, row: number) => {
    if (isFilling.current) { setFillTo({ col, row }); return }
    if (isDragSelecting.current) {
      setRangeEnd({ col, row })
    }
  }

  // Démarre l'édition d'une cellule en initialisant le brouillon contrôlé.
  // `initial` fourni = saisie directe d'un caractère (remplace le contenu).
  const startEditCell = (col: string, row: number, initial?: string) => {
    const c = sheetData.cells[cellKey(col, row)]
    setCellDraft(initial !== undefined ? initial : (c?.f ?? cellDisplay(c)))
    setEditingCell({ col, row })
  }

  const handleCellDoubleClick = (col: string, row: number) => {
    setSelectedCell({ col, row })
    setRangeEnd(null)
    startEditCell(col, row)
  }

  const handleEditCommit = (col: string, row: number, val: string) => {
    updateCell(col, row, val)
    setEditingCell(null)
    // Sync formula bar
    if (!fbActiveRef.current) {
      const isFormula = val.startsWith('=')
      setFbDraft(isFormula ? val : val)
    }
  }

  const handleEditAbort = () => {
    setEditingCell(null)
  }

  // Recalcule l'assistance (autocomplétion de nom OU helper d'argument) selon le
  // texte + position du caret de l'input actif (barre ou cellule).
  const refreshAssist = (text: string, caret: number, el: HTMLInputElement | null, setter: (v: string) => void) => {
    activeInputRef.current = el
    activeSetterRef.current = setter
    if (el) { const r = el.getBoundingClientRect(); setAssistPos({ left: r.left, top: r.bottom }) }
    const token = nameTokenAt(text, caret)
    if (token) {
      const matches = FORMULA_FUNCTIONS.filter(f => f.name.startsWith(token))
      setAcSuggestions(matches); setAcIdx(0); setAcOpen(matches.length > 0); setArgInfo(null)
    } else {
      setAcOpen(false)
      const ctx = argContextAt(text, caret)
      setArgInfo(ctx && FORMULA_FUNCTIONS.some(f => f.name === ctx.name) ? ctx : null)
    }
  }

  const closeAssist = () => { setAcOpen(false); setArgInfo(null) }

  const applyAcSuggestion = (fn: FormulaFn) => {
    const el = activeInputRef.current
    const text = el ? el.value : fbDraft
    const caret = el?.selectionStart ?? text.length
    const token = nameTokenAt(text, caret)
    const insertAt = caret - token.length
    const newVal = text.slice(0, insertAt) + fn.name + '(' + text.slice(caret)
    const newCaret = insertAt + fn.name.length + 1
    activeSetterRef.current(newVal)
    setAcOpen(false)
    setTimeout(() => {
      el?.focus()
      el?.setSelectionRange(newCaret, newCaret)
      refreshAssist(newVal, newCaret, el, activeSetterRef.current)
    }, 0)
  }

  // Navigation/validation clavier de l'autocomplétion ; renvoie true si consommé.
  const assistKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): boolean => {
    if (!acOpen || acSuggestions.length === 0) return false
    if (e.key === 'ArrowDown') { setAcIdx(i => Math.min(i + 1, acSuggestions.length - 1)); e.preventDefault(); return true }
    if (e.key === 'ArrowUp')   { setAcIdx(i => Math.max(i - 1, 0)); e.preventDefault(); return true }
    if (e.key === 'Tab' || e.key === 'Enter') { applyAcSuggestion(acSuggestions[acIdx]); e.preventDefault(); return true }
    if (e.key === 'Escape')    { setAcOpen(false); e.preventDefault(); return true }
    return false
  }

  const moveSelection = useCallback((dir: 'up' | 'down' | 'left' | 'right') => {
    setSelectedCell(prev => {
      if (!prev) return prev
      const ci = COLS.indexOf(prev.col)
      let nc = ci, nr = prev.row
      if (dir === 'right') nc = Math.min(ci + 1, MAX_COLS - 1)
      if (dir === 'left')  nc = Math.max(ci - 1, 0)
      if (dir === 'down')  nr = Math.min(nr + 1, MAX_ROWS)
      if (dir === 'up')    nr = Math.max(nr - 1, 1)
      return { col: COLS[nc], row: nr }
    })
    setRangeEnd(null)
  }, [])

  const handleTab = useCallback((shift: boolean) => {
    if (shift) moveSelection('left')
    else moveSelection('right')
  }, [moveSelection])

  // ── Copier / coller / recopie (plages) ──────────────────────────────────────
  const bounds = () => {
    if (!selectedCell) return null
    const end = rangeEnd ?? selectedCell
    return {
      c1: Math.min(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col)),
      c2: Math.max(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col)),
      r1: Math.min(selectedCell.row, end.row),
      r2: Math.max(selectedCell.row, end.row),
    }
  }
  // Écriture groupée (une seule reconstruction + une seule sauvegarde).
  const writeCells = (entries: { col: string; row: number; cell: CellData | null }[]) => {
    const cells = { ...sheetData.cells }
    for (const { col, row, cell } of entries) {
      const k = cellKey(col, row)
      if (!cell || (cell.v == null && !cell.f && !cell.s)) delete cells[k]
      else cells[k] = cell
    }
    const newData: SheetData = { ...sheetData, cells }
    commitData(newData)
  }

  const copySelection = (cut: boolean) => {
    const b = bounds(); if (!b) return
    const cells: (CellData | undefined)[][] = []
    for (let r = b.r1; r <= b.r2; r++) {
      const row: (CellData | undefined)[] = []
      for (let c = b.c1; c <= b.c2; c++) { const cd = sheetData.cells[cellKey(COLS[c], r)]; row.push(cd ? structuredClone(cd) : undefined) }
      cells.push(row)
    }
    clipboard.current = { cells, rows: b.r2 - b.r1 + 1, cols: b.c2 - b.c1 + 1, cut, originCol: b.c1, originRow: b.r1 }
  }

  const pasteSelection = () => {
    const cb = clipboard.current; if (!cb || !selectedCell) return
    const destC = COLS.indexOf(selectedCell.col), destR = selectedCell.row
    const dCol = destC - cb.originCol, dRow = destR - cb.originRow
    const entries: { col: string; row: number; cell: CellData | null }[] = []
    if (cb.cut) for (let r = 0; r < cb.rows; r++) for (let c = 0; c < cb.cols; c++) entries.push({ col: COLS[cb.originCol + c], row: cb.originRow + r, cell: null })
    for (let r = 0; r < cb.rows; r++) for (let c = 0; c < cb.cols; c++) {
      const tc = destC + c, tr = destR + r
      if (tc >= MAX_COLS || tr > MAX_ROWS) continue
      const src = cb.cells[r][c]
      if (!src) { entries.push({ col: COLS[tc], row: tr, cell: null }); continue }
      const cell = structuredClone(src)
      if (cell.f && cell.f.startsWith('=')) cell.f = translateFormula(cell.f, dCol, dRow)
      entries.push({ col: COLS[tc], row: tr, cell })
    }
    writeCells(entries)
    if (cb.cut) clipboard.current = null
    setSelectedCell({ col: COLS[destC], row: destR })
    setRangeEnd({ col: COLS[Math.min(MAX_COLS - 1, destC + cb.cols - 1)], row: Math.min(MAX_ROWS, destR + cb.rows - 1) })
  }

  const clearSelection = () => {
    const b = bounds(); if (!b) return
    const entries: { col: string; row: number; cell: CellData | null }[] = []
    for (let r = b.r1; r <= b.r2; r++) for (let c = b.c1; c <= b.c2; c++) entries.push({ col: COLS[c], row: r, cell: null })
    writeCells(entries)
  }

  // Recopie : remplit la zone étendue depuis le bloc source (séries numériques
  // si ≥2 nombres sur l'axe, sinon recopie tuilée avec décalage des formules).
  const performFill = () => {
    const src = fillStart.current; const to = fillTo
    if (!src || !to) return
    const toC = COLS.indexOf(to.col), toR = to.row
    const srcH = src.r2 - src.r1 + 1, srcW = src.c2 - src.c1 + 1
    const entries: { col: string; row: number; cell: CellData | null }[] = []
    const at = (c: number, r: number) => sheetData.cells[cellKey(COLS[c], r)]
    const tileCell = (srcCell: CellData | undefined, dCol: number, dRow: number): CellData | null => {
      if (!srcCell) return null
      const cell = structuredClone(srcCell)
      if (cell.f && cell.f.startsWith('=')) cell.f = translateFormula(cell.f, dCol, dRow)
      return cell
    }

    const lang = i18n.language
    if (toR > src.r2 || toR < src.r1) {                       // ── vertical ──
      const down = toR > src.r2
      const targetRows = down ? rangeAsc(src.r2 + 1, toR) : rangeDesc(src.r1 - 1, toR)
      for (let c = src.c1; c <= src.c2; c++) {
        const srcCells = Array.from({ length: srcH }, (_, i) => at(c, src.r1 + i))
        const hasFormula = srcCells.some(cd => cd?.f?.startsWith('='))
        const series = hasFormula ? null : fillSeries(srcCells.map(cd => (cd?.v ?? '') as string | number), targetRows.length, down, lang)
        const sty = (down ? at(c, src.r2) : at(c, src.r1))?.s
        targetRows.forEach((r, i) => {
          if (series) entries.push({ col: COLS[c], row: r, cell: { v: series[i] as string | number, ...(sty ? { s: sty } : {}) } })
          else { const tileIdx = (((r - src.r1) % srcH) + srcH) % srcH; entries.push({ col: COLS[c], row: r, cell: tileCell(at(c, src.r1 + tileIdx), 0, r - (src.r1 + tileIdx)) }) }
        })
      }
    } else if (toC > src.c2 || toC < src.c1) {                // ── horizontal ──
      const right = toC > src.c2
      const targetCols = right ? rangeAsc(src.c2 + 1, toC) : rangeDesc(src.c1 - 1, toC)
      for (let r = src.r1; r <= src.r2; r++) {
        const srcCells = Array.from({ length: srcW }, (_, i) => at(src.c1 + i, r))
        const hasFormula = srcCells.some(cd => cd?.f?.startsWith('='))
        const series = hasFormula ? null : fillSeries(srcCells.map(cd => (cd?.v ?? '') as string | number), targetCols.length, right, lang)
        const sty = (right ? at(src.c2, r) : at(src.c1, r))?.s
        targetCols.forEach((c, i) => {
          if (series) entries.push({ col: COLS[c], row: r, cell: { v: series[i] as string | number, ...(sty ? { s: sty } : {}) } })
          else { const tileIdx = (((c - src.c1) % srcW) + srcW) % srcW; entries.push({ col: COLS[c], row: r, cell: tileCell(at(src.c1 + tileIdx, r), c - (src.c1 + tileIdx), 0) }) }
        })
      }
    }
    writeCells(entries)
    setSelectedCell({ col: COLS[Math.min(src.c1, toC)], row: Math.min(src.r1, toR) })
    setRangeEnd({ col: COLS[Math.max(src.c2, toC)], row: Math.max(src.r2, toR) })
  }
  performFillRef.current = performFill

  const startFill = () => {
    const b = bounds(); if (!b) return
    fillStart.current = b
    isFilling.current = true
    setFillTo({ col: COLS[b.c2], row: b.r2 })
  }

  // Global mouse-up to end drag selection
  useEffect(() => {
    const up = () => {
      isDragSelecting.current = false
      if (isFilling.current) { performFillRef.current(); isFilling.current = false; fillStart.current = null; setFillTo(null) }
    }
    window.addEventListener('mouseup', up)
    return () => window.removeEventListener('mouseup', up)
  }, [])

  // Global key handler (only when grid has logical focus, not form elements)
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't intercept when any input/textarea has focus
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return
      if (!selectedCell || editingCell) return

      if (e.ctrlKey || e.metaKey) {
        const k = e.key.toLowerCase()
        if (k === 'c') { copySelection(false); e.preventDefault(); return }
        if (k === 'x') { copySelection(true);  e.preventDefault(); return }
        if (k === 'v') { pasteSelection();      e.preventDefault(); return }
        return
      }
      if (e.key === 'ArrowUp')    { moveSelection('up');    e.preventDefault() }
      if (e.key === 'ArrowDown')  { moveSelection('down');  e.preventDefault() }
      if (e.key === 'ArrowLeft')  { moveSelection('left');  e.preventDefault() }
      if (e.key === 'ArrowRight') { moveSelection('right'); e.preventDefault() }
      if (e.key === 'Enter')      { startEditCell(selectedCell.col, selectedCell.row); e.preventDefault() }
      if (e.key === 'Tab')        { moveSelection(e.shiftKey ? 'left' : 'right'); e.preventDefault() }
      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (rangeEnd && (rangeEnd.col !== selectedCell.col || rangeEnd.row !== selectedCell.row)) clearSelection()
        else updateCell(selectedCell.col, selectedCell.row, '')
        e.preventDefault()
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
        startEditCell(selectedCell.col, selectedCell.row, e.key)
        e.preventDefault()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [selectedCell, rangeEnd, sheetData, editingCell, moveSelection, updateCell]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Resize handlers ─────────────────────────────────────────────────────────

  function startColResize(col: string, e: React.MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    resizingCol.current = { col, startX: e.clientX, startW: getColWidth(col) }
  }

  function startRowResize(row: number, e: React.MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    resizingRow.current = { row, startY: e.clientY, startH: getRowHeight(row) }
  }

  function onGridMouseMove(e: React.MouseEvent) {
    if (resizingCol.current) {
      const { col, startX, startW } = resizingCol.current
      const newW = Math.max(20, startW + (e.clientX - startX))
      setLocalColWidths(prev => ({ ...prev, [col]: newW }))
    }
    if (resizingRow.current) {
      const { row, startY, startH } = resizingRow.current
      const newH = Math.max(14, startH + (e.clientY - startY))
      setLocalRowHeights(prev => ({ ...prev, [String(row)]: newH }))
    }
  }

  function onGridMouseUp() {
    if (resizingCol.current) {
      saveDimensionsMut.mutate({
        col_widths: { ...(sheet?.col_widths ?? {}), ...localColWidths },
      })
      resizingCol.current = null
    }
    if (resizingRow.current) {
      saveDimensionsMut.mutate({
        row_heights: { ...(sheet?.row_heights ?? {}), ...localRowHeights },
      })
      resizingRow.current = null
    }
  }

  // ── Style actions ───────────────────────────────────────────────────────────

  const toggleStyle = (prop: keyof NonNullable<CellData['s']>) => {
    if (!selectedCell) return
    const key = cellKey(selectedCell.col, selectedCell.row)
    const cell = sheetData.cells[key] ?? {}
    const currentStyle = cell.s ?? {}
    const newStyle = { ...currentStyle, [prop]: !currentStyle[prop as keyof typeof currentStyle] }
    const newData: SheetData = {
      ...sheetData,
      cells: { ...sheetData.cells, [key]: { ...cell, s: newStyle } },
    }
    commitData(newData)
  }

  const setAlign = (align: 'left' | 'center' | 'right') => {
    if (!selectedCell) return
    const key = cellKey(selectedCell.col, selectedCell.row)
    const cell = sheetData.cells[key] ?? {}
    const newData: SheetData = {
      ...sheetData,
      cells: { ...sheetData.cells, [key]: { ...cell, s: { ...(cell.s ?? {}), align } } },
    }
    commitData(newData)
  }

  // Applique un patch de style à toute la sélection (plage incluse).
  const applyToSelection = (patch: Partial<NonNullable<CellData['s']>>) => {
    if (!selectedCell) return
    const end = rangeEnd ?? selectedCell
    const c1 = Math.min(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col))
    const c2 = Math.max(COLS.indexOf(selectedCell.col), COLS.indexOf(end.col))
    const r1 = Math.min(selectedCell.row, end.row)
    const r2 = Math.max(selectedCell.row, end.row)
    const cells = { ...sheetData.cells }
    for (let ci = c1; ci <= c2; ci++) {
      for (let ri = r1; ri <= r2; ri++) {
        const k = cellKey(COLS[ci], ri)
        const cell = cells[k] ?? {}
        cells[k] = { ...cell, s: { ...(cell.s ?? {}), ...patch } }
      }
    }
    const newData: SheetData = { ...sheetData, cells }
    commitData(newData)
  }

  // Format numérique : bascule devise/pourcentage, ajuste les décimales.
  const setNumFmt = (fmt: NonNullable<CellData['s']>['numFmt']) =>
    applyToSelection({ numFmt: selectedCellStyle.numFmt === fmt ? undefined : fmt })
  const adjustDecimals = (delta: number) => {
    const fmt = selectedCellStyle.numFmt
    const base = selectedCellStyle.decimals ?? (fmt === 'currency' || fmt === 'percent' ? 2 : 0)
    applyToSelection({ decimals: Math.max(0, Math.min(10, base + delta)) })
  }

  // ── Bordures (selon la position dans la plage : contour vs intérieur) ─────────
  type BorderKind = 'all' | 'outer' | 'inner' | 'top' | 'bottom' | 'left' | 'right' | 'none'
  const applyBorders = (kind: BorderKind, color = '#5f6368') => {
    const b = bounds(); if (!b) return
    const cells = { ...sheetData.cells }
    for (let ci = b.c1; ci <= b.c2; ci++) {
      for (let ri = b.r1; ri <= b.r2; ri++) {
        const k = cellKey(COLS[ci], ri)
        const cell = cells[k] ?? {}
        const s = { ...(cell.s ?? {}) }
        const isTop = ri === b.r1, isBottom = ri === b.r2
        const isLeft = ci === b.c1, isRight = ci === b.c2
        const set = (edge: 'bt' | 'br' | 'bb' | 'bl', on: boolean) => {
          if (on) s[edge] = color; else delete s[edge]
        }
        switch (kind) {
          case 'none':
            delete s.bt; delete s.br; delete s.bb; delete s.bl; break
          case 'all':
            s.bt = color; s.br = color; s.bb = color; s.bl = color; break
          case 'outer':
            set('bt', isTop); set('bb', isBottom); set('bl', isLeft); set('br', isRight); break
          case 'inner':
            set('bt', !isTop); set('bb', !isBottom); set('bl', !isLeft); set('br', !isRight); break
          case 'top':    if (isTop)    s.bt = color; break
          case 'bottom': if (isBottom) s.bb = color; break
          case 'left':   if (isLeft)   s.bl = color; break
          case 'right':  if (isRight)  s.br = color; break
        }
        cells[k] = { ...cell, s }
      }
    }
    const newData: SheetData = { ...sheetData, cells }
    commitData(newData)
    setBordersOpen(false)
  }

  // ── Sheet tab management ────────────────────────────────────────────────────

  const createSheetMut = useMutation({
    mutationFn: () => spreadsheetsApi.createSheet(ssId),
    onSuccess: (sheet) => {
      const newMetas = [...sheetMetas, sheet]
      onSheetMetasChange(newMetas)
      setActiveSheetId(sheet.id)
    },
  })

  const deleteSheetMut = useMutation({
    mutationFn: (sheetId: string) => spreadsheetsApi.deleteSheet(ssId, sheetId),
    onSuccess: (_, sheetId) => {
      const newMetas = sheetMetas.filter(s => s.id !== sheetId)
      onSheetMetasChange(newMetas)
      if (activeSheetId === sheetId) setActiveSheetId(newMetas[0]?.id ?? '')
    },
  })

  const renameSheetMut = useMutation({
    mutationFn: ({ sheetId, name }: { sheetId: string; name: string }) =>
      spreadsheetsApi.updateSheet(ssId, sheetId, { name }),
    onSuccess: (updated) => {
      onSheetMetasChange(sheetMetas.map(s => s.id === updated.id ? { ...s, name: updated.name } : s))
      setEditingSheetName(null)
    },
  })

  const selectedCellStyle = selectedCell
    ? sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]?.s ?? {}
    : {}

  const cellAddressLabel = selectedCell
    ? (rangeEnd && (rangeEnd.col !== selectedCell.col || rangeEnd.row !== selectedCell.row)
        ? `${selectedCell.col}${selectedCell.row}:${rangeEnd.col}${rangeEnd.row}`
        : `${selectedCell.col}${selectedCell.row}`)
    : ''

  return (
    <div
      className="flex flex-col h-full bg-white select-none"
      style={{ cursor: resizeCursor }}
      onMouseMove={onGridMouseMove}
      onMouseUp={onGridMouseUp}
    >
      {/* Toolbar */}
      <div className="flex items-center gap-0.5 px-2 py-1 border-b border-[#e2e4e6] bg-[#f8f9fa] flex-shrink-0 flex-wrap">
        <Dropdown
          height={28}
          fontSize={12}
          className="mr-1"
          width={120}
          value={selectedCellStyle.fontFamily ?? 'Arial'}
          onChange={v => applyToSelection({ fontFamily: v })}
          options={fontFamilies.map(f => ({ value: f, label: f }))}
        />

        <input
          type="number"
          value={selectedCellStyle.fontSize ?? 11}
          onChange={e => applyToSelection({ fontSize: Number(e.target.value) })}
          className="w-12 h-7 px-1 text-xs border border-[#dadce0] rounded bg-white text-center mr-1"
          min={6} max={96}
        />

        <div className="w-px h-5 bg-[#dadce0] mx-0.5" />

        <button
          onClick={() => toggleStyle('bold')}
          title={t('sheet_bold')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] transition-colors ${selectedCellStyle.bold ? 'bg-[#e8f0fe] text-primary' : ''}`}
        >
          <Bold size={14} />
        </button>
        <button
          onClick={() => toggleStyle('italic')}
          title={t('sheet_italic')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] transition-colors ${selectedCellStyle.italic ? 'bg-[#e8f0fe] text-primary' : ''}`}
        >
          <Italic size={14} />
        </button>
        <button
          onClick={() => toggleStyle('underline')}
          title={t('sheet_underline')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] transition-colors ${selectedCellStyle.underline ? 'bg-[#e8f0fe] text-primary' : ''}`}
        >
          <Underline size={14} />
        </button>
        <button
          onClick={() => toggleStyle('strike')}
          title={t('sheet_strikethrough')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] transition-colors ${selectedCellStyle.strike ? 'bg-[#e8f0fe] text-primary' : ''}`}
        >
          <span className="text-xs font-bold line-through">S</span>
        </button>

        <div className="w-px h-5 bg-[#dadce0] mx-0.5" />

        <div>
          <button ref={textColorBtnRef} onClick={() => setTextColorOpen(o => !o)} className="w-7 h-7 flex flex-col items-center justify-center rounded hover:bg-[#e8eaed] gap-0" title={t('sheet_text_color')}>
            <Type size={11} />
            <div className="w-4 h-1 rounded-sm mt-0.5" style={{ background: selectedCellStyle.color ?? '#000' }} />
          </button>
          <AnchoredPopover anchorRef={textColorBtnRef} open={textColorOpen} onClose={() => setTextColorOpen(false)}>
            <ColorSwatchPicker
              color={selectedCellStyle.color ?? '#000000'}
              t={t}
              onChange={hex => applyToSelection({ color: hex })}
              onClose={() => setTextColorOpen(false)}
              customLabel={t('sheet_text_color')}
            />
          </AnchoredPopover>
        </div>
        <div className="relative">
          <button onClick={() => setFillOpen(o => !o)} className="w-7 h-7 flex flex-col items-center justify-center rounded hover:bg-[#e8eaed] gap-0" title={t('sheet_fill_color')}>
            <PaintBucket size={11} />
            <div className="w-4 h-1 rounded-sm mt-0.5" style={{
              background: selectedCellStyle.bgGradient ? gradientToCss(selectedCellStyle.bgGradient) : (selectedCellStyle.bg ?? 'transparent'),
              border: '1px solid #dadce0' }} />
          </button>
          {fillOpen && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setFillOpen(false)} />
              <div className="absolute left-0 top-8 z-50 bg-white border border-[#dadce0] rounded-lg shadow-lg p-2 flex flex-col gap-2" style={{ width: 200 }}>
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs text-text-secondary">{t('sheet_fill_solid')}</span>
                  <div className="flex items-center gap-1">
                    <ColorField width={26} height={20}
                      color={selectedCellStyle.bg ?? '#ffffff'}
                      onChange={hex => applyToSelection({ bg: hex, bgGradient: undefined })} />
                    <button onClick={() => applyToSelection({ bg: undefined, bgGradient: undefined })}
                      className="px-1.5 h-5 text-[10px] rounded border border-[#dadce0] hover:bg-[#f1f3f4]">{t('sheet_fill_none')}</button>
                  </div>
                </div>
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs text-text-secondary">{t('sheet_fill_gradient')}</span>
                  <GradientField width={40} height={20}
                    value={selectedCellStyle.bgGradient ?? DEFAULT_GRADIENT}
                    onChange={g => applyToSelection({ bgGradient: g, bg: undefined })} />
                </div>
              </div>
            </>
          )}
        </div>

        <div className="w-px h-5 bg-[#dadce0] mx-0.5" />

        <button
          onClick={() => setAlign('left')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] ${selectedCellStyle.align === 'left' || !selectedCellStyle.align ? 'bg-[#e8f0fe] text-primary' : ''}`}
          title={t('sheet_align_left')}
        >
          <AlignLeft size={14} />
        </button>
        <button
          onClick={() => setAlign('center')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] ${selectedCellStyle.align === 'center' ? 'bg-[#e8f0fe] text-primary' : ''}`}
          title={t('sheet_align_center')}
        >
          <AlignCenter size={14} />
        </button>
        <button
          onClick={() => setAlign('right')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] ${selectedCellStyle.align === 'right' ? 'bg-[#e8f0fe] text-primary' : ''}`}
          title={t('sheet_align_right')}
        >
          <AlignRight size={14} />
        </button>

        <div className="w-px h-5 bg-[#dadce0] mx-0.5" />

        <button
          onClick={() => toggleStyle('wrap')}
          className={`h-7 px-2 text-xs flex items-center rounded hover:bg-[#e8eaed] ${selectedCellStyle.wrap ? 'bg-[#e8f0fe] text-primary' : ''}`}
          title={t('sheet_wrap_text')}
        >
          {t('sheet_wrap')}
        </button>

        <div className="w-px h-5 bg-[#dadce0] mx-0.5" />

        <button onClick={() => setNumFmt('currency')} title={t('sheet_format_currency')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] ${selectedCellStyle.numFmt === 'currency' ? 'bg-[#e8f0fe] text-primary' : ''}`}>
          <Euro size={14} />
        </button>
        <button onClick={() => setNumFmt('percent')} title={t('sheet_format_percent')}
          className={`w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] ${selectedCellStyle.numFmt === 'percent' ? 'bg-[#e8f0fe] text-primary' : ''}`}>
          <Percent size={14} />
        </button>
        <button onClick={() => adjustDecimals(-1)} title={t('sheet_format_dec_less')}
          className="h-7 px-1.5 text-[11px] font-mono flex items-center rounded hover:bg-[#e8eaed]">.0&lt;</button>
        <button onClick={() => adjustDecimals(1)} title={t('sheet_format_dec_more')}
          className="h-7 px-1.5 text-[11px] font-mono flex items-center rounded hover:bg-[#e8eaed]">.00&gt;</button>
        <button onClick={() => setNumFmt('number')} title={t('sheet_number_format')}
          className={`h-7 px-2 text-xs flex items-center rounded hover:bg-[#e8eaed] gap-1 ${selectedCellStyle.numFmt === 'number' ? 'bg-[#e8f0fe] text-primary' : ''}`}>
          <Hash size={12} /> 123
        </button>

        <div className="w-px h-5 bg-[#dadce0] mx-0.5" />

        {/* Bordures */}
        <div className="relative">
          <button onClick={() => setBordersOpen(o => !o)} title={t('sheet_borders')}
            className={`h-7 px-1.5 flex items-center gap-0.5 rounded hover:bg-[#e8eaed] ${bordersOpen ? 'bg-[#e8f0fe] text-primary' : ''}`}>
            <Grid2x2 size={14} /> <ChevronDown size={10} />
          </button>
          {bordersOpen && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setBordersOpen(false)} />
              <div className="absolute left-0 top-8 z-50 bg-white border border-[#dadce0] rounded-lg shadow-lg p-1.5"
                   style={{ width: 168 }}>
                <div className="grid grid-cols-4 gap-0.5">
                  {([
                    ['all',    t('sheet_border_all'),    BorderIcon.all],
                    ['inner',  t('sheet_border_inner'),  BorderIcon.inner],
                    ['outer',  t('sheet_border_outer'),  BorderIcon.outer],
                    ['none',   t('sheet_border_none'),   BorderIcon.none],
                    ['top',    t('sheet_border_top'),    BorderIcon.top],
                    ['bottom', t('sheet_border_bottom'), BorderIcon.bottom],
                    ['left',   t('sheet_border_left'),   BorderIcon.left],
                    ['right',  t('sheet_border_right'),  BorderIcon.right],
                  ] as const).map(([kind, label, glyph]) => (
                    <button key={kind} title={label}
                      onClick={() => applyBorders(kind)}
                      className="w-9 h-9 flex items-center justify-center rounded hover:bg-[#e8eaed] text-text-secondary">
                      {glyph}
                    </button>
                  ))}
                </div>
              </div>
            </>
          )}
        </div>

        {saveMut.isPending && (
          <span className="ml-2 text-xs text-text-tertiary">{t('sheet_saving')}</span>
        )}
      </div>

      {/* Formula bar */}
      <div className="relative flex-shrink-0">
        <div className="flex items-center border-b border-[#e2e4e6] bg-white" style={{ height: 26 }}>
          <div
            className="flex items-center justify-center border-r border-[#e2e4e6] text-xs text-text-secondary font-mono bg-white flex-shrink-0"
            style={{ width: 80, height: '100%' }}
          >
            {cellAddressLabel}
          </div>
          <div className="flex items-center justify-center border-r border-[#e2e4e6] flex-shrink-0 px-2 text-xs italic text-text-tertiary" style={{ height: '100%' }}>
            fx
          </div>
          <FormulaInput
            inputRef={formulaBarRef}
            containerStyle={{ flex: 1, height: '100%' }}
            inputStyle={{ width: '100%', height: '100%', padding: '0 8px', fontSize: 14, border: 'none', outline: 'none', boxSizing: 'border-box' }}
            value={editingCell ? cellDraft : fbDraft}
            onChange={e => {
              const v = e.target.value
              const setter = editingCell ? setCellDraft : setFbDraft
              setter(v)
              refreshAssist(v, e.target.selectionStart ?? v.length, e.target, setter)
            }}
            onSelect={e => {
              const el = e.currentTarget
              refreshAssist(el.value, el.selectionStart ?? el.value.length, el, editingCell ? setCellDraft : setFbDraft)
            }}
            onFocus={() => { fbActiveRef.current = true; setBarFocused(true) }}
            onBlur={() => {
              fbActiveRef.current = false
              setBarFocused(false)
              closeAssist()
              // Commit on blur only if value changed
              if (selectedCell) {
                const orig = sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]?.f
                  ?? cellDisplay(sheetData.cells[cellKey(selectedCell.col, selectedCell.row)])
                if (fbDraft !== orig) {
                  updateCell(selectedCell.col, selectedCell.row, fbDraft)
                }
              }
            }}
            onKeyDown={e => {
              if (assistKeyDown(e)) return
              if (e.key === 'Enter' && selectedCell) {
                updateCell(selectedCell.col, selectedCell.row, fbDraft)
                moveSelection('down')
                formulaBarRef.current?.blur()
                e.preventDefault()
              }
              if (e.key === 'Escape') {
                const orig = selectedCell
                  ? (sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]?.f
                    ?? cellDisplay(sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]))
                  : ''
                setFbDraft(orig)
                formulaBarRef.current?.blur()
                e.preventDefault()
              }
            }}
            placeholder={t('sheet_formula_placeholder')}
          />
        </div>

        {/* Autocomplete dropdown — style Google Sheets : nom coloré par catégorie,
            l'item actif révèle sa description + syntaxe, pied d'aide clavier. */}
        {acOpen && acSuggestions.length > 0 && assistPos && (
          <div
            style={{
              position: 'fixed',
              top: assistPos.top,
              left: assistPos.left,
              width: 420,
              maxWidth: 'calc(100vw - 24px)',
              zIndex: 1000,
              background: 'white',
              border: '1px solid #dadce0',
              borderRadius: 6,
              boxShadow: '0 6px 22px rgba(0,0,0,.20)',
              overflow: 'hidden',
            }}
          >
            {/* Bouton fermer */}
            <button
              onMouseDown={e => { e.preventDefault(); setAcOpen(false) }}
              title={t('common_close')}
              style={{
                position: 'absolute', top: 6, right: 6, zIndex: 1,
                width: 22, height: 22, borderRadius: '50%', border: 'none',
                background: '#f1f3f4', color: '#5f6368', cursor: 'pointer',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
              }}
            >
              <X size={13} />
            </button>

            <div style={{ maxHeight: 300, overflowY: 'auto', padding: '4px 0' }}>
              {acSuggestions.map((fn, i) => {
                const active = i === acIdx
                const color = fnColor(fn.name)
                return (
                  <div
                    key={fn.name}
                    onMouseDown={e => { e.preventDefault(); applyAcSuggestion(fn) }}
                    onMouseEnter={() => setAcIdx(i)}
                    style={{
                      padding: active ? '8px 14px 10px' : '5px 14px',
                      background: active ? '#f1f3f4' : 'white',
                      cursor: 'pointer',
                      borderLeft: active ? `3px solid ${color}` : '3px solid transparent',
                    }}
                  >
                    <span style={{ fontWeight: 600, fontSize: 13, color, fontFamily: 'monospace' }}>
                      {fn.name}
                    </span>
                    {active && (
                      <>
                        <div style={{ fontSize: 12, color: '#3c4043', marginTop: 3, lineHeight: 1.35 }}>
                          {t(fn.descKey)}
                        </div>
                        <div style={{ fontSize: 11, color: '#80868b', marginTop: 3, fontFamily: 'monospace' }}>
                          {fn.syntax}
                        </div>
                      </>
                    )}
                  </div>
                )
              })}
            </div>

            {/* Pied : aide clavier */}
            <div style={{
              display: 'flex', alignItems: 'center', gap: 6,
              padding: '6px 12px', borderTop: '1px solid #e8eaed',
              fontSize: 11, color: '#5f6368', background: '#fafafa',
            }}>
              <kbd style={{
                fontFamily: 'inherit', fontSize: 10, padding: '1px 6px',
                border: '1px solid #dadce0', borderRadius: 4, background: 'white', color: '#3c4043',
              }}>{t('sheet_ac_tab', 'Tabulation')}</kbd>
              <span>{t('sheet_ac_accept', 'pour accepter.')}</span>
              <span style={{ marginLeft: 4, color: '#80868b' }}>↑↓</span>
              <span>{t('sheet_ac_navigate', 'pour naviguer')}</span>
            </div>
          </div>
        )}

        {/* Helper d'argument — signature avec l'argument courant surligné + description */}
        {!acOpen && argInfo && assistPos && (() => {
          const meta = FORMULA_FUNCTIONS.find(f => f.name === argInfo.name)
          if (!meta) return null
          const args = parseSyntaxArgs(meta.syntax)
          const color = fnColor(meta.name)
          let cur = argInfo.argIndex
          const lastRepeat = args.length > 0 && args[args.length - 1].repeat
          if (cur >= args.length) cur = lastRepeat ? args.length - 1 : -1
          return (
            <div
              style={{
                position: 'fixed', top: assistPos.top, left: assistPos.left,
                width: 380, maxWidth: 'calc(100vw - 24px)', zIndex: 1000,
                background: 'white', border: '1px solid #dadce0', borderRadius: 6,
                boxShadow: '0 6px 22px rgba(0,0,0,.20)', overflow: 'hidden',
              }}
            >
              <div style={{ padding: '10px 14px', borderBottom: '1px solid #f1f3f4', position: 'relative' }}>
                <button
                  onMouseDown={e => { e.preventDefault(); setArgInfo(null) }}
                  title={t('common_close')}
                  style={{
                    position: 'absolute', top: 6, right: 6, width: 22, height: 22, borderRadius: '50%',
                    border: 'none', background: '#f1f3f4', color: '#5f6368', cursor: 'pointer',
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                  }}
                ><X size={13} /></button>
                {/* Signature */}
                <div style={{ fontFamily: 'monospace', fontSize: 13, paddingRight: 24 }}>
                  <span style={{ fontWeight: 700, color }}>{meta.name}</span>
                  <span style={{ color: '#3c4043' }}>(</span>
                  {args.map((a, k) => {
                    const isCur = k === cur
                    const label = (a.optional ? '[' : '') + a.name + (a.optional ? ']' : '')
                    return (
                      <span key={k}>
                        {k > 0 && <span style={{ color: '#3c4043' }}>; </span>}
                        <span style={{
                          fontWeight: isCur ? 700 : 400,
                          color: isCur ? '#188038' : '#5f6368',
                          background: isCur ? '#e6f4ea' : 'transparent',
                          borderRadius: 3, padding: isCur ? '0 3px' : 0,
                        }}>{label}</span>
                        {a.repeat && k === args.length - 1 && <span style={{ color: '#80868b' }}>; …</span>}
                      </span>
                    )
                  })}
                  <span style={{ color: '#3c4043' }}>)</span>
                </div>
              </div>
              {/* À propos */}
              <div style={{ padding: '10px 14px' }}>
                <div style={{ fontSize: 10, fontWeight: 700, color: '#80868b', letterSpacing: '.5px', marginBottom: 3 }}>
                  {t('sheet_arg_about', 'À PROPOS')}
                </div>
                <div style={{ fontSize: 12, color: '#3c4043', lineHeight: 1.4 }}>{t(meta.descKey)}</div>
                {cur >= 0 && (
                  <div style={{ marginTop: 8, fontSize: 12 }}>
                    <span style={{ fontWeight: 700, color: '#188038', fontFamily: 'monospace' }}>{args[cur].name}</span>
                    {args[cur].optional && <span style={{ color: '#80868b' }}> — {t('sheet_arg_optional', 'facultatif')}</span>}
                    {args[cur].repeat && <span style={{ color: '#80868b' }}> — {t('sheet_arg_repeatable', 'répétable')}</span>}
                  </div>
                )}
              </div>
            </div>
          )
        })()}
      </div>

      {/* Grid */}
      <div
        ref={gridRef}
        className="flex-1 overflow-auto relative"
        style={{ fontFamily: 'Arial, sans-serif', fontSize: 13 }}
        onMouseMove={onGridCursor}
        onMouseLeave={() => publishCursor(null)}
      >
        {sheetQuery.isLoading ? (
          <div className="flex items-center justify-center h-full text-text-tertiary text-sm">{t('common_loading')}</div>
        ) : (
          <table
            style={{ borderCollapse: 'collapse', tableLayout: 'fixed', userSelect: 'none' }}
          >
            {/* Column headers */}
            <thead>
              <tr>
                {/* Corner */}
                <th
                  style={{
                    width: ROW_HEADER_WIDTH,
                    minWidth: ROW_HEADER_WIDTH,
                    height: COL_HEADER_HEIGHT,
                    position: 'sticky',
                    top: 0,
                    left: 0,
                    zIndex: 3,
                    backgroundColor: '#f8f9fa',
                    borderRight: '1px solid #c1c7cd',
                    borderBottom: '1px solid #c1c7cd',
                    boxSizing: 'border-box',
                  }}
                />
                {COLS.map(col => {
                  const w = getColWidth(col)
                  return (
                    <th
                      key={col}
                      style={{
                        width: w,
                        minWidth: w,
                        height: COL_HEADER_HEIGHT,
                        position: 'sticky',
                        top: 0,
                        zIndex: 2,
                        backgroundColor: isColHighlighted(col) ? '#d3e3fd' : '#f8f9fa',
                        borderRight: '1px solid #c1c7cd',
                        borderBottom: '1px solid #c1c7cd',
                        fontSize: 12,
                        color: '#444',
                        fontWeight: 500,
                        textAlign: 'center',
                        boxSizing: 'border-box',
                        userSelect: 'none',
                      }}
                    >
                      {col}
                      {/* Column resize handle */}
                      <div
                        onMouseDown={e => startColResize(col, e)}
                        style={{
                          position: 'absolute',
                          right: 0,
                          top: 0,
                          width: 5,
                          height: '100%',
                          cursor: 'col-resize',
                          zIndex: 10,
                        }}
                      />
                    </th>
                  )
                })}
              </tr>
            </thead>

            {/* Rows */}
            <tbody>
              {ROWS.map(row => {
                const rh = getRowHeight(row)
                return (
                  <tr key={row}>
                    {/* Row header */}
                    <td
                      style={{
                        width: ROW_HEADER_WIDTH,
                        minWidth: ROW_HEADER_WIDTH,
                        height: rh,
                        position: 'sticky',
                        left: 0,
                        zIndex: 1,
                        backgroundColor: isRowHighlighted(row) ? '#d3e3fd' : '#f8f9fa',
                        borderRight: '1px solid #c1c7cd',
                        borderBottom: '1px solid #e2e4e6',
                        fontSize: 12,
                        color: '#444',
                        textAlign: 'center',
                        boxSizing: 'border-box',
                        userSelect: 'none',
                      }}
                    >
                      {row}
                      {/* Row resize handle */}
                      <div
                        onMouseDown={e => startRowResize(row, e)}
                        style={{
                          position: 'absolute',
                          bottom: 0,
                          left: 0,
                          width: '100%',
                          height: 4,
                          cursor: 'row-resize',
                          zIndex: 10,
                        }}
                      />
                    </td>
                    {COLS.map(col => (
                      <Cell
                        key={col}
                        col={col}
                        row={row}
                        data={sheetData}
                        selected={selectedCell?.col === col && selectedCell?.row === row}
                        inRange={isInSelection(col, row)}
                        editing={editingCell?.col === col && editingCell?.row === row}
                        colWidth={getColWidth(col)}
                        rowHeight={rh}
                        onClick={handleCellClick}
                        onDoubleClick={handleCellDoubleClick}
                        onMouseDown={handleCellMouseDown}
                        onMouseEnter={handleCellMouseEnter}
                        onEditCommit={handleEditCommit}
                        onEditAbort={handleEditAbort}
                        editValue={editingCell?.col === col && editingCell?.row === row ? cellDraft : ''}
                        onEditChange={(v, el) => { setCellDraft(v); refreshAssist(v, el.selectionStart ?? v.length, el, setCellDraft) }}
                        onEditSelect={el => refreshAssist(el.value, el.selectionStart ?? el.value.length, el, setCellDraft)}
                        assistKeyDown={assistKeyDown}
                        onArrow={moveSelection}
                        onTab={handleTab}
                        isFillCorner={!!selectedCell && !editingCell && col === fillCornerCol && row === fillCornerRow}
                        onFillStart={startFill}
                      />
                    ))}
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}

        {/* Encadrés colorés des plages référencées par la formule en édition */}
        {refHighlights.map((h, i) => (
          <div
            key={i}
            style={{
              position: 'absolute', left: h.left, top: h.top, width: h.width, height: h.height,
              border: `2px solid ${h.color}`, background: `${h.color}14`,
              pointerEvents: 'none', zIndex: 1, boxSizing: 'border-box',
            }}
          />
        ))}

        {/* Sélections des autres participants (présence collaborative) */}
        {remoteSelections.map((s, i) => (
          <div
            key={`sel${i}`}
            style={{
              position: 'absolute', left: s.left, top: s.top, width: s.width, height: s.height,
              border: `2px solid ${s.color}`, pointerEvents: 'none', zIndex: 2, boxSizing: 'border-box',
            }}
          >
            <div style={{
              position: 'absolute', top: -15, left: -2, background: s.color, color: '#fff',
              fontSize: 10, lineHeight: '13px', padding: '0 4px', borderRadius: 3,
              whiteSpace: 'nowrap', fontWeight: 600,
            }}>{s.name}</div>
          </div>
        ))}

        {/* Curseurs souris distants (présence) — repère contenu (scrolle avec la grille) */}
        <RemoteCursors awareness={awareness} selfClientId={awareness.clientID} toScreen={c => ({ left: c.x, top: c.y })} />
      </div>

      {/* Sheet tabs */}
      <div
        className="flex items-center border-t border-[#e2e4e6] bg-[#f8f9fa] flex-shrink-0 overflow-x-auto"
        style={{ height: 36 }}
      >
        <button
          onClick={() => createSheetMut.mutate()}
          disabled={createSheetMut.isPending}
          className="w-8 h-8 flex items-center justify-center text-text-secondary hover:bg-[#e8eaed] rounded m-1 flex-shrink-0"
          title={t('sheet_add_sheet')}
        >
          <Plus size={16} />
        </button>

        <div className="flex items-end h-full overflow-x-auto">
          {sheetMetas.map(meta => (
            <div
              key={meta.id}
              className={`flex items-center h-full px-3 gap-1.5 cursor-pointer border-t-2 flex-shrink-0 group ${
                meta.id === activeSheetId
                  ? 'border-primary bg-white text-primary font-medium'
                  : 'border-transparent text-text-secondary hover:bg-[#e8eaed] hover:text-text-primary'
              }`}
              onClick={() => { setActiveSheetId(meta.id); setSelectedCell(null); setRangeEnd(null); setEditingCell(null) }}
              onDoubleClick={() => { setEditingSheetName(meta.id); setSheetNameDraft(meta.name) }}
            >
              {editingSheetName === meta.id ? (
                <input
                  autoFocus
                  value={sheetNameDraft}
                  onChange={e => setSheetNameDraft(e.target.value)}
                  onBlur={() => renameSheetMut.mutate({ sheetId: meta.id, name: sheetNameDraft })}
                  onKeyDown={e => {
                    if (e.key === 'Enter') renameSheetMut.mutate({ sheetId: meta.id, name: sheetNameDraft })
                    if (e.key === 'Escape') setEditingSheetName(null)
                    e.stopPropagation()
                  }}
                  className="text-xs outline-none border-b border-primary bg-transparent w-24"
                  onClick={e => e.stopPropagation()}
                />
              ) : (
                <span className="text-xs">{meta.name}</span>
              )}
              {sheetMetas.length > 1 && meta.id === activeSheetId && (
                <button
                  onClick={e => { e.stopPropagation(); deleteSheetMut.mutate(meta.id) }}
                  className="opacity-0 group-hover:opacity-100 w-4 h-4 flex items-center justify-center rounded hover:bg-danger/10 hover:text-danger"
                  title={t('sheet_delete_sheet')}
                >
                  <Trash2 size={10} />
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

// ── Tabs ──────────────────────────────────────────────────────────────────────


const SPREADSHEET_MIME = 'application/vnd.oasis.opendocument.spreadsheet'


// ── SpreadsheetApp (list + editor) ────────────────────────────────────────────

export default function SpreadsheetApp({ recent, starred, trashed }: {
  recent?: boolean; starred?: boolean; trashed?: boolean
} = {}) {
  const { t, i18n } = useTranslation('office')
  const { id } = useParams<{ id?: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [isOpeningFile, setIsOpeningFile] = useState(false)

  const { data: recentData } = useQuery({
    queryKey: ['spreadsheets', { recent: true }],
    queryFn:  () => spreadsheetsApi.list({ recent: true, limit: 20 }),
    staleTime: 30_000,
    enabled:  !recent && !starred && !trashed,
  })

  const listParams = { recent, starred, trashed }

  const listQuery = useQuery({
    queryKey: ['spreadsheets', listParams],
    queryFn:  () => spreadsheetsApi.list(listParams),
    staleTime: 30_000,
  })

  const ssQuery = useQuery({
    queryKey: ['spreadsheet', id],
    queryFn:  () => spreadsheetsApi.get(id!),
    enabled:  !!id,
    staleTime: 30_000,
  })

  const [title, setTitle] = useState('')
  const [isSaving, setIsSaving] = useState(false)   // statut d'enregistrement pour la topbar
  const [presence, setPresence] = useState<PresenceUser[]>([])  // avatars de présence (topbar)
  const [shareOpen, setShareOpen] = useState(false)
  const [sheetMetas, setSheetMetas] = useState<SheetMeta[]>([])

  useEffect(() => {
    if (ssQuery.data) {
      setTitle(ssQuery.data.spreadsheet.title)
      setSheetMetas(ssQuery.data.sheets)
    }
  }, [ssQuery.data])

  const createMut = useMutation({
    mutationFn: () => spreadsheetsApi.create({ title: t('common_untitled') }),
    onSuccess: (ss) => {
      qc.invalidateQueries({ queryKey: ['spreadsheets'] })
      navigate(`/office/spreadsheets/${ss.id}`)
    },
  })

  const listTrashMut = useMutation({
    mutationFn: (id: string) => spreadsheetsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spreadsheets'] }),
  })

  const listRestoreMut = useMutation({
    mutationFn: (id: string) => spreadsheetsApi.restore(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spreadsheets'] }),
  })

  const listDeleteMut = useMutation({
    mutationFn: (id: string) => spreadsheetsApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spreadsheets'] }),
  })

  const listStarMut = useMutation({
    mutationFn: ({ id, val }: { id: string; val: boolean }) => spreadsheetsApi.update(id, { is_starred: val }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spreadsheets'] }),
  })

  const duplicateMut = useMutation({
    mutationFn: (id: string) => spreadsheetsApi.duplicate(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spreadsheets'] }),
  })

  const updateTitleMut = useMutation({
    mutationFn: (t: string) => spreadsheetsApi.update(id!, { title: t }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spreadsheets'] }),
  })

  const starMut = useMutation({
    mutationFn: (starred: boolean) => spreadsheetsApi.update(id!, { is_starred: starred }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['spreadsheet', id] })
      qc.invalidateQueries({ queryKey: ['spreadsheets'] })
    },
  })

  const trashMut = useMutation({
    mutationFn: () => spreadsheetsApi.trash(id!),
    onSuccess: () => { navigate('/office/spreadsheets'); qc.invalidateQueries({ queryKey: ['spreadsheets'] }) },
  })

  // ── Editor view ─────────────────────────────────────────────────────────────
  if (id) {
    const ss = ssQuery.data?.spreadsheet

    // Chrome standard (WorkspaceShell) : masque l'AppHeader global (chromeless →
    // gain vertical) et héberge titre + actions + HeaderActions dans sa topbar.
    return (
      <OfficeShell
        ribbon={[{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
          groups: [fileGroup(t, { onNew: () => createMut.mutate(), onDuplicate: () => duplicateMut.mutate(id) })] }]}
        theme={THEME_SPREADSHEET}
        chromeless
        topbarHeight={64}
        onBack={() => navigate('/office/spreadsheets')}
        titleIcon={<Grid2x2 size={16} className="text-white/90 flex-shrink-0" />}
        saveStatus={isSaving ? t('sheet_saving') : t('doc_saved')}
        topbarActions={
          <div className="flex items-center gap-2">
            <PresenceAvatarList users={presence} />
            <button onClick={() => setShareOpen(true)}
              className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-primary text-white text-sm font-medium hover:bg-primary-hover transition-colors">
              <UserPlus size={15} /> {t('share_button', 'Partager')}
            </button>
          </div>
        }
        title={title}
        onTitleChange={setTitle}
        onTitleCommit={() => updateTitleMut.mutate(title)}
        titlePlaceholder={t('common_untitled')}
        titleActions={(
          <button
            onClick={() => starMut.mutate(!(ss?.is_starred))}
            className={`p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 ${ss?.is_starred ? 'text-warning' : 'text-white/90'}`}
            title={ss?.is_starred ? t('sheet_unstar') : t('sheet_star')}
          >
            <Star size={15} fill={ss?.is_starred ? 'currentColor' : 'none'} />
          </button>
        )}
        onDelete={() => trashMut.mutate()}
        deleteTitle={t('sheet_move_to_trash')}
        deleteConfirm={{
          title: t('sheet_delete_confirm_title', { defaultValue: 'Supprimer ce tableur ?' }),
          message: t('sheet_delete_confirm_msg', { defaultValue: 'Le tableur sera déplacé dans la corbeille.' }),
          confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
          variant: 'danger',
        }}
      >
        {sheetMetas.length > 0 ? (
          <div className="flex-1 min-w-0 flex flex-col">
            <SpreadsheetEditor
              ssId={id}
              sheetMetas={sheetMetas}
              onSheetMetasChange={setSheetMetas}
              onSavingChange={setIsSaving}
              onPresenceChange={setPresence}
            />
          </div>
        ) : ssQuery.isLoading ? (
          <div className="flex-1 flex items-center justify-center text-text-tertiary text-sm">{t('common_loading')}</div>
        ) : null}
        {shareOpen && (
          <CollaboratorsDialog
            entityId={id}
            cacheKey="sheet-collab"
            title={t('share_title_sheet', 'Partager le tableur')}
            onClose={() => setShareOpen(false)}
            api={{
              listCollaborators:  spreadsheetsApi.listCollaborators,
              addCollaborator:    spreadsheetsApi.addCollaborator,
              updateCollaborator: spreadsheetsApi.updateCollaborator,
              removeCollaborator: spreadsheetsApi.removeCollaborator,
              searchRecipients:   officeApi.searchRecipients,
            }}
          />
        )}
      </OfficeShell>
    )
  }

  // ── handleOpenFile (for ModuleFileBrowser) ────────────────────────────────────
  const handleOpenFile = (file: FileItem): boolean => {
    const meta = file.metadata as Record<string, unknown> | undefined
    const ssId = meta?.office_spreadsheet_id as string | undefined
    if (ssId) {
      navigate(`/office/spreadsheets/${ssId}`)
      return true
    }
    if (file.mime_type !== SPREADSHEET_MIME) return false
    if (isOpeningFile) return true
    setIsOpeningFile(true)
    spreadsheetsApi.openByFile(file.id)
      .then(ss => navigate(`/office/spreadsheets/${ss.id}`))
      .catch(() => { /* silently ignore */ })
      .finally(() => setIsOpeningFile(false))
    return true
  }

  // ── List view ────────────────────────────────────────────────────────────────
  const spreadsheets = listQuery.data?.spreadsheets ?? []
  const listTitle = trashed ? t('sheet_list_trash') : starred ? t('sheet_list_starred') : recent ? t('sheet_list_recent') : t('sheet_list_all')

  // Default view: show tabs (Récents | Parcourir)
  if (!recent && !starred && !trashed) {
    const sheetIcon = (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#1e8e3e" strokeWidth="1.5">
        <rect x="3" y="3" width="18" height="18" rx="2" />
        <line x1="3" y1="8" x2="21" y2="8" /><line x1="3" y1="13" x2="21" y2="13" />
        <line x1="3" y1="18" x2="21" y2="18" /><line x1="8" y1="3" x2="8" y2="21" /><line x1="14" y1="3" x2="14" y2="21" />
      </svg>
    )
    const recentItems: StartPageRecentItem[] = (recentData?.spreadsheets ?? []).map(ss => ({
      id:       ss.id,
      name:     ss.title || t('common_untitled'),
      subtitle: format(new Date(ss.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
      icon:     sheetIcon,
      onClick:  () => navigate(`/office/spreadsheets/${ss.id}`),
      actions: [
        { id: 'open',  label: t('sheet_open_in_editor'), icon: <ExternalLink size={15} />, onClick: () => navigate(`/office/spreadsheets/${ss.id}`) },
        { id: 'dup',   label: t('common_duplicate', { defaultValue: 'Dupliquer' }),         icon: <Copy size={15} />,         onClick: () => duplicateMut.mutate(ss.id) },
        { id: 'trash', label: t('common_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => { spreadsheetsApi.trash(ss.id).then(() => qc.invalidateQueries({ queryKey: ['spreadsheets'] })) } },
      ],
    }))
    const tabs: StartPageTab[] = [{
      id: 'browse', label: t('sheet_tab_browse'),
      content: (
        <ModuleFileBrowser
          folderPathPrefix="Office/Spreadsheets"
          title={t('sheet_list_all')}
          onOpenFile={handleOpenFile}
          fileTypeModuleId="office-spreadsheets"
          toolbarContent={
            <Button size="sm" icon={<Plus size={15} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
              {t('sheet_new')}
            </Button>
          }
          fileContextActions={[
            {
              id:      'open-editor',
              label:   t('sheet_open_in_editor'),
              icon:    ExternalLink,
              visible: (f) =>
                !!(f.metadata as Record<string, unknown>)?.office_spreadsheet_id ||
                f.mime_type === SPREADSHEET_MIME,
              onClick: handleOpenFile,
            },
          ]}
          emptyState={
            <div className="flex flex-col items-center justify-center py-24 text-center">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-text-tertiary mb-4 opacity-30">
                <rect x="3" y="3" width="18" height="18" rx="2" />
                <line x1="3" y1="8" x2="21" y2="8" /><line x1="3" y1="13" x2="21" y2="13" />
                <line x1="3" y1="18" x2="21" y2="18" /><line x1="8" y1="3" x2="8" y2="21" /><line x1="14" y1="3" x2="14" y2="21" />
              </svg>
              <p className="text-text-secondary font-medium mb-1">{t('sheet_empty')}</p>
              <button onClick={() => createMut.mutate()} className="text-sm text-primary hover:underline mt-2">
                {t('sheet_create_first')}
              </button>
            </div>
          }
        />
      ),
    }]
    return (
      <StartPage
        recentTitle={t('sheet_tab_recent')}
        recentItems={recentItems}
        recentEmpty={
          <div className="flex flex-col items-center gap-2 px-2 text-center">
            <p className="text-text-tertiary text-xs">{t('sheet_no_recent')}</p>
          </div>
        }
        tabs={tabs}
        defaultTab="browse"
      />
    )
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-5xl mx-auto px-6 py-6">
        <div className="flex items-center justify-between mb-6">
          <h1 className="text-xl font-semibold text-text-primary">{listTitle}</h1>
          {!trashed && (
            <Button icon={<Plus size={16} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
              {t('sheet_new')}
            </Button>
          )}
        </div>

        {listQuery.isLoading ? (
          <div className="text-center py-16 text-text-tertiary text-sm">{t('common_loading')}</div>
        ) : spreadsheets.length === 0 ? (
          <div className="text-center py-20">
            <div className="w-16 h-16 bg-surface-2 rounded-2xl flex items-center justify-center mx-auto mb-4">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-text-tertiary">
                <rect x="3" y="3" width="18" height="18" rx="2" />
                <line x1="3" y1="9" x2="21" y2="9" />
                <line x1="3" y1="15" x2="21" y2="15" />
                <line x1="9" y1="3" x2="9" y2="21" />
                <line x1="15" y1="3" x2="15" y2="21" />
              </svg>
            </div>
            <p className="text-text-secondary text-sm font-medium mb-1">{t('sheet_empty')}</p>
            <p className="text-text-tertiary text-xs mb-4">{t('sheet_empty_hint')}</p>
            <Button onClick={() => createMut.mutate()}>
              {t('sheet_new')}
            </Button>
          </div>
        ) : (
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4">
            {spreadsheets.map(ss => (
              <div
                key={ss.id}
                className="group relative border border-border rounded-xl overflow-hidden hover:shadow-md transition-shadow cursor-pointer bg-white"
                onClick={() => navigate(`/office/spreadsheets/${ss.id}`)}
              >
                <div className="h-32 bg-[#e6f4ea] flex items-center justify-center">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#1e8e3e" strokeWidth="1.5">
                    <rect x="3" y="3" width="18" height="18" rx="2" />
                    <line x1="3" y1="8" x2="21" y2="8" />
                    <line x1="3" y1="13" x2="21" y2="13" />
                    <line x1="3" y1="18" x2="21" y2="18" />
                    <line x1="8" y1="3" x2="8" y2="21" />
                    <line x1="14" y1="3" x2="14" y2="21" />
                  </svg>
                </div>
                <div className="px-3 py-2.5">
                  <p className="text-sm font-medium text-text-primary truncate">{ss.title || t('common_untitled')}</p>
                  <p className="text-xs text-text-tertiary mt-0.5">
                    {format(new Date(ss.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) })}
                  </p>
                </div>
                {ss.is_starred && (
                  <div className="absolute top-2 right-8">
                    <Star size={14} className="text-yellow-500" fill="currentColor" />
                  </div>
                )}
                {/* Dropdown */}
                <div className="absolute top-2 right-2" onClick={e => e.stopPropagation()}>
                  <DropdownMenu.Root>
                    <DropdownMenu.Trigger asChild>
                      <button className="opacity-0 group-hover:opacity-100 p-1 rounded bg-white/80 hover:bg-white shadow-sm text-text-tertiary">
                        <MoreVertical size={14} />
                      </button>
                    </DropdownMenu.Trigger>
                    <DropdownMenu.Portal>
                      <DropdownMenu.Content
                        className="z-50 bg-white border border-[#e8eaed] rounded-xl shadow-lg py-1 min-w-[160px]"
                        align="end"
                      >
                        {ss.is_trashed ? (
                          <>
                            <DropdownMenu.Item
                              className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                              onSelect={() => listRestoreMut.mutate(ss.id)}
                            >
                              <ArrowLeft size={14} /> {t('sheet_restore')}
                            </DropdownMenu.Item>
                            <DropdownMenu.Item
                              className="flex items-center gap-2 px-3 py-2 text-sm text-danger hover:bg-danger/5 cursor-pointer outline-none"
                              onSelect={() => listDeleteMut.mutate(ss.id)}
                            >
                              <Trash2 size={14} /> {t('sheet_delete_permanently')}
                            </DropdownMenu.Item>
                          </>
                        ) : (
                          <>
                            <DropdownMenu.Item
                              className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                              onSelect={() => listStarMut.mutate({ id: ss.id, val: !ss.is_starred })}
                            >
                              <Star size={14} className={ss.is_starred ? 'text-warning fill-warning' : ''} />
                              {ss.is_starred ? t('sheet_unstar') : t('sheet_star')}
                            </DropdownMenu.Item>
                            <DropdownMenu.Item
                              className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                              onSelect={() => duplicateMut.mutate(ss.id)}
                            >
                              <Copy size={14} /> {t('common_duplicate')}
                            </DropdownMenu.Item>
                            <DropdownMenu.Separator className="my-1 h-px bg-border" />
                            <DropdownMenu.Item
                              className="flex items-center gap-2 px-3 py-2 text-sm text-danger hover:bg-danger/5 cursor-pointer outline-none"
                              onSelect={() => listTrashMut.mutate(ss.id)}
                            >
                              <Trash2 size={14} /> {t('sheet_move_to_trash')}
                            </DropdownMenu.Item>
                          </>
                        )}
                      </DropdownMenu.Content>
                    </DropdownMenu.Portal>
                  </DropdownMenu.Root>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
