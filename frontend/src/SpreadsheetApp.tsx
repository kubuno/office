import {
  useState, useEffect, useLayoutEffect, useRef, useCallback, useMemo,
} from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { useParams, useNavigate } from 'react-router-dom'
import { getDateLocale } from '@kubuno/sdk'
import { format } from 'date-fns'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  ArrowLeft, Star, Plus, Trash2, MoreVertical, Copy,
  Bold, Italic, Underline, AlignLeft, AlignCenter, AlignRight,
  PaintBucket, Type, Hash, ExternalLink, Percent, Euro,
  Grid2x2, ChevronDown, X, UserPlus, Snowflake, Filter, Check, Tag, TableCellsMerge, Grid3x3, WrapText,
  Crop, RotateCw, RotateCcw, BringToFront, SendToBack, ImageOff, RefreshCw, Image as ImageIcon,
  Scissors, ClipboardPaste, Sigma, Eraser, ImagePlus, ArrowUpAZ, ArrowDownAZ, Rows3, Columns3,
  Table2, Shapes, Smile, Workflow, BarChart3, Activity, Undo2, Redo2, Paintbrush, Search, CopyMinus,
  LineChart as LineChartIcon, PieChart as PieChartIcon, BarChartHorizontal, ScatterChart as ScatterChartIcon,
} from 'lucide-react'
import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { spreadsheetsApi, officeApi, SheetData, SheetImage, SheetEquation, SheetChart, ChartType, CellData, SpreadsheetSheet, SheetMeta } from './api'
import { BarChart, LineChart, PieChart, ScatterChart } from './DataCharts'
import { useSystemFonts } from './systemAssets'
import CollaboratorsDialog from './CollaboratorsDialog'
import { FormulaInput } from './FormulaInput'
import { parseRefs, refBounds, nameTokenAt, argContextAt, parseSyntaxArgs } from './formula-refs'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { usePresenceUsers, PresenceAvatarList, userColor, usePublishCursor, RemoteCursors, type PresenceUser } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import { evaluate, formatValue, formatCode, colToIndex, indexToCol, computeSpills, computeCondFormats, type SpillIndex, type CondStyle } from './formula-engine'

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
import katex from 'katex'
import 'katex/dist/katex.min.css'
import LatexEditor from './LatexEditor'
import { Dropdown, Button, StartPage, ColorField, GradientField, gradientToCss, rgbaFromHex, DEFAULT_GRADIENT, ColorSwatchPicker, AnchoredPopover, MenuDropdown, FloatingWindow, type MenuItem } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
import type { RibbonTab } from './ribbon/types'
import { StatusBar, StatusButton, StatusSep, StatusSpacer, StatusZoom } from './shell/StatusBar'
import { MacrosMenu } from './macros/MacrosMenu'
import NameManagerDialog from './NameManagerDialog'
import { appAlert, appConfirm, appPrompt } from './macros/FormRuntime'
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
// Noms de fonctions valides (pour souligner les fautes dans la barre de formule).
const FN_NAMES = new Set(FORMULA_FUNCTIONS.map(f => f.name))

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

// Dimensions à l'échelle d'Excel : 16 384 colonnes (A..XFD) × 1 048 576 lignes.
// On n'affiche/calcule JAMAIS tout d'un coup : la grille est virtualisée (canvas =
// fenêtre visible), la géométrie des lignes est analytique (pas de tableau de 1M),
// et les boucles « tout le tableur » sont bornées à la plage RÉELLEMENT utilisée.
const MAX_COLS = 16384
const MAX_ROWS = 1048576
const DEFAULT_COL_WIDTH = 100
const DEFAULT_ROW_HEIGHT = 24
const ROW_HEADER_WIDTH = 46
const COL_HEADER_HEIGHT = 24

// Accès colonne ⇄ nom sans allouer 16 384 chaînes : un Proxy qui calcule à la volée
// via indexToCol/colToIndex. `COLS[c]` (nom), `COLS.indexOf(nom)` (index), `COLS.length`
// restent identiques au code existant (zéro churn) mais en O(1)/O(log) sans tableau.
const COLS = new Proxy([] as unknown as string[], {
  get(_t, prop) {
    if (prop === 'indexOf') return (name: unknown) => colToIndex(String(name))
    if (prop === 'length')  return MAX_COLS
    if (typeof prop === 'string' && /^[0-9]+$/.test(prop)) return indexToCol(Number(prop))
    return (Array.prototype as unknown as Record<PropertyKey, unknown>)[prop as PropertyKey]
  },
})

// ── Helpers ───────────────────────────────────────────────────────────────────

function cellKey(col: string, row: number) { return `${col}${row}` }

function cellDisplay(cell: CellData | undefined): string {
  if (!cell) return ''
  if (cell.v === null || cell.v === undefined) return ''
  return String(cell.v)
}

// Valeur numérique d'une cellule (formule évaluée ou nombre brut), sinon null.
function asNumber(s: unknown): number | null {
  if (typeof s === 'number') return s
  if (typeof s === 'string' && s !== '' && !isNaN(Number(s))) return Number(s)
  return null
}

function numericValue(cell: CellData | undefined, data: SheetData, key?: string, spill?: SpillIndex): number | null {
  if (!cell) {
    // Empty cell may carry a spilled value from a dynamic-array anchor.
    if (key && spill) return asNumber(spill.values[key])
    return null
  }
  if (cell.f && cell.f.startsWith('=')) {
    if (key && spill?.anchors[key]?.blocked) return null
    const v = evaluate(cell.f, data, undefined, spill)
    const s = Array.isArray(v) ? v[0]?.[0] : v
    return typeof s === 'number' ? s : null
  }
  if (typeof cell.v === 'number') return cell.v
  if (typeof cell.v === 'string' && cell.v !== '' && !isNaN(Number(cell.v))) return Number(cell.v)
  return null
}

// Parse une adresse « B3 » → {c,r} (0-based col, 1-based row), bornée à la grille.
function parseRefAddr(ref: string): { c: number; r: number } | null {
  const m = /^([A-Za-z]{1,3})([0-9]{1,7})$/.exec(ref.trim())
  if (!m) return null
  const c = colToIndex(m[1].toUpperCase()), r = +m[2]
  if (c < 0 || c >= MAX_COLS || r < 1 || r > MAX_ROWS) return null
  return { c, r }
}
// Parse une plage « A1:C5 » (ou une cellule seule) → rectangle de bords inclusifs.
function parseRangeAddr(range: string): { c1: number; r1: number; c2: number; r2: number } | null {
  const parts = range.split(':')
  const a = parseRefAddr(parts[0]); if (!a) return null
  const b = parts[1] ? parseRefAddr(parts[1]) : a; if (!b) return null
  return { c1: Math.min(a.c, b.c), r1: Math.min(a.r, b.r), c2: Math.max(a.c, b.c), r2: Math.max(a.r, b.r) }
}

// Excel-style aggregate of a rectangular selection for the status bar:
// count of non-empty cells + numeric count/sum/avg/min/max (formulas resolved).
// Aggregate the selection. Iterates ONLY the existing cells (+ spilled values) that
// fall inside the rectangle — O(populated cells), not O(area). Selecting a whole
// column (1 048 576 rows) would otherwise scan a million empty cells on every render.
function selectionAggregate(data: SheetData, c1: number, c2: number, r1: number, r2: number, spill?: SpillIndex) {
  let count = 0, num = 0, sum = 0, min = Infinity, max = -Infinity
  const add = (key: string, cell: CellData | undefined) => {
    const m = /^([A-Z]+)([0-9]+)$/.exec(key); if (!m) return
    const c = colToIndex(m[1]), r = +m[2]
    if (c < c1 || c > c2 || r < r1 || r > r2) return
    const spilled = !cell && spill ? spill.values[key] !== undefined : false
    if (!spilled && (!cell || (cell.v == null && !cell.f))) return
    count++
    const n = numericValue(cell, data, key, spill)
    if (n != null && !isNaN(n)) { num++; sum += n; if (n < min) min = n; if (n > max) max = n }
  }
  for (const key in data.cells) add(key, data.cells[key])
  if (spill) for (const key in spill.values) if (!(key in data.cells)) add(key, undefined)
  return { count, num, sum, avg: num ? sum / num : 0, min: num ? min : 0, max: num ? max : 0 }
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

function resolveValue(cell: CellData | undefined, data: SheetData, key?: string, spill?: SpillIndex): string {
  if (!cell) {
    // Empty cell may show a value spilled from a dynamic-array anchor.
    if (key && spill) { const sv = spill.values[key]; if (sv !== undefined) return formatValue(sv) }
    return ''
  }
  if (cell.f && cell.f.startsWith('=')) {
    if (key && spill?.anchors[key]?.blocked) return '#SPILL!'
    return formatValue(evaluate(cell.f, data, undefined, spill))
  }
  return cellDisplay(cell)
}

// ── Canvas drawing helpers ────────────────────────────────────────────────────

// Builds a CanvasGradient matching the cell's stored Gradient (same convention as
// the CSS gradientToCss helper: angle 0° points up, increasing clockwise).
function makeCanvasGradient(
  ctx: CanvasRenderingContext2D, g: import('@ui').Gradient,
  x: number, y: number, w: number, h: number,
): CanvasGradient {
  let grad: CanvasGradient
  if (g.type === 'radial') {
    const cx = x + w / 2, cy = y + h / 2
    grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(w, h) / 2)
  } else {
    const cx = x + w / 2, cy = y + h / 2
    const dx = Math.sin(g.angle * Math.PI / 180), dy = -Math.cos(g.angle * Math.PI / 180)
    const half = (Math.abs(dx) * w + Math.abs(dy) * h) / 2
    grad = ctx.createLinearGradient(cx - dx * half, cy - dy * half, cx + dx * half, cy + dy * half)
  }
  for (const s of [...g.stops].sort((a, b) => a.position - b.position)) {
    grad.addColorStop(Math.min(1, Math.max(0, s.position / 100)), rgbaFromHex(s.color, s.opacity))
  }
  return grad
}

// Small funnel glyph drawn in a column header (filter affordance). Filled blue when
// the column has an active filter, hollow grey otherwise.
function drawFunnel(ctx: CanvasRenderingContext2D, cx: number, cy: number, active: boolean) {
  const w = 9, h = 8, x = cx - w / 2, y = cy - h / 2
  ctx.save()
  ctx.strokeStyle = active ? '#1a73e8' : '#9aa0a6'
  ctx.fillStyle = active ? '#1a73e8' : '#9aa0a6'
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.moveTo(x, y); ctx.lineTo(x + w, y)
  ctx.lineTo(x + w * 0.6, y + h * 0.5); ctx.lineTo(x + w * 0.6, y + h)
  ctx.lineTo(x + w * 0.4, y + h * 0.78); ctx.lineTo(x + w * 0.4, y + h * 0.5)
  ctx.closePath()
  if (active) ctx.fill(); else ctx.stroke()
  ctx.restore()
}

// Greedy word-wrap of `text` to fit `maxWidth` (px) using the ctx's current font.
function wrapText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): string[] {
  const words = text.split(/\s+/)
  const lines: string[] = []
  let line = ''
  for (const word of words) {
    const test = line ? `${line} ${word}` : word
    if (line && ctx.measureText(test).width > maxWidth) { lines.push(line); line = word }
    else line = test
  }
  if (line) lines.push(line)
  return lines
}

// ── Border selector glyphs ──────────────────────────────────────────────────

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

// ── Cell editor overlay ───────────────────────────────────────────────────────
// In the canvas grid, only the cell being edited needs a real DOM input. It is
// positioned absolutely over the canvas in content (scrollable) coordinates.

interface CellEditorProps {
  col: string
  row: number
  left: number
  top: number
  width: number
  height: number
  value: string
  onChange: (val: string, el: HTMLInputElement) => void
  onSelect: (el: HTMLInputElement) => void
  onCommit: (col: string, row: number, val: string) => void
  onAbort: () => void
  assistKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => boolean
  onArrow: (dir: 'up' | 'down' | 'left' | 'right') => void
  onTab: (shift: boolean) => void
  fontSize?: number
}

function CellEditor({
  col, row, left, top, width, height, value,
  onChange, onSelect, onCommit, onAbort, assistKeyDown, onArrow, onTab, fontSize = 13,
}: CellEditorProps) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.focus()
    const n = el.value.length
    el.setSelectionRange(n, n)   // caret en fin (compatible avec la saisie directe d'un caractère)
  }, [])

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (assistKeyDown(e)) return   // autocomplétion ouverte : ↑↓/Tab/Entrée/Échap lui reviennent
    // Enter commits then moves to the next row (Shift+Enter = previous), like Excel/Sheets.
    if (e.key === 'Enter') { onCommit(col, row, e.currentTarget.value); onArrow(e.shiftKey ? 'up' : 'down'); e.preventDefault() }
    if (e.key === 'Escape') { onAbort(); e.preventDefault() }
    if (e.key === 'Tab') { onCommit(col, row, e.currentTarget.value); onTab(e.shiftKey); e.preventDefault() }
    if (e.key === 'ArrowUp')    { onCommit(col, row, e.currentTarget.value); onArrow('up');    e.preventDefault() }
    if (e.key === 'ArrowDown')  { onCommit(col, row, e.currentTarget.value); onArrow('down');  e.preventDefault() }
    if (e.key === 'ArrowLeft' && e.currentTarget.selectionStart === 0)
      { onCommit(col, row, e.currentTarget.value); onArrow('left');  e.preventDefault() }
    if (e.key === 'ArrowRight' && e.currentTarget.selectionStart === e.currentTarget.value.length)
      { onCommit(col, row, e.currentTarget.value); onArrow('right'); e.preventDefault() }
  }

  return (
    <FormulaInput
      inputRef={inputRef}
      value={value}
      knownFunctions={FN_NAMES}
      onChange={e => onChange(e.target.value, e.target)}
      onSelect={e => onSelect(e.currentTarget)}
      onKeyDown={handleKeyDown}
      onBlur={e => onCommit(col, row, e.target.value)}
      containerStyle={{
        position: 'absolute', left, top, width, height, zIndex: 20,
        border: '2px solid #1a73e8', background: 'white', boxSizing: 'border-box',
      }}
      inputStyle={{
        width: '100%', height: '100%', padding: '0 4px',
        fontSize, fontFamily: 'inherit', border: 'none', outline: 'none', boxSizing: 'border-box',
      }}
    />
  )
}

// ── Column filter popup ───────────────────────────────────────────────────────
// Lists the distinct displayed values of a column with checkboxes; applying keeps
// only the checked values (null = no filter, i.e. all values allowed).

function ColumnFilterPopup({ x, y, values, initialAllowed, onApply, onClose, t }: {
  x: number; y: number; values: string[]; initialAllowed: Set<string> | null
  onApply: (allowed: Set<string> | null) => void; onClose: () => void
  t: TFunction<'office'>
}) {
  const [checked, setChecked] = useState<Set<string>>(() => initialAllowed ? new Set(initialAllowed) : new Set(values))
  const [q, setQ] = useState('')
  const shown = values.filter(v => v.toLowerCase().includes(q.toLowerCase()))
  const toggle = (v: string) => setChecked(prev => { const n = new Set(prev); if (n.has(v)) n.delete(v); else n.add(v); return n })
  const label = (v: string) => v === '' ? t('sheet_filter_blanks', '(Vides)') : v
  const apply = () => { onApply(checked.size === values.length ? null : new Set(checked)); onClose() }

  return (
    <>
      <div className="fixed inset-0 z-[60]" onMouseDown={onClose} />
      <div
        className="fixed z-[61] bg-white border border-[#dadce0] rounded-lg shadow-xl flex flex-col"
        style={{ left: Math.min(x, window.innerWidth - 248), top: y + 4, width: 230, maxHeight: 360 }}
        onMouseDown={e => e.stopPropagation()}
      >
        <div className="p-2 border-b border-[#eee]">
          <input
            autoFocus value={q} onChange={e => setQ(e.target.value)}
            placeholder={t('sheet_filter_search', 'Rechercher…')}
            className="w-full h-7 px-2 text-xs border border-[#dadce0] rounded outline-none"
          />
          <div className="flex gap-3 mt-1.5 text-[11px] text-primary">
            <button onClick={() => setChecked(new Set(values))} className="hover:underline">{t('sheet_filter_all', 'Tout sélectionner')}</button>
            <button onClick={() => setChecked(new Set())} className="hover:underline">{t('sheet_filter_clear', 'Effacer')}</button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto py-1">
          {shown.length === 0 && <div className="px-3 py-2 text-xs text-text-tertiary">—</div>}
          {shown.map(v => (
            <button key={v} onClick={() => toggle(v)} className="w-full flex items-center gap-2 px-3 py-1 hover:bg-[#f1f3f4] text-xs text-left">
              <span className={`w-4 h-4 flex-shrink-0 flex items-center justify-center rounded border ${checked.has(v) ? 'bg-primary border-primary text-white' : 'border-[#bbb]'}`}>
                {checked.has(v) && <Check size={11} />}
              </span>
              <span className="truncate">{label(v)}</span>
            </button>
          ))}
        </div>
        <div className="flex justify-end gap-2 p-2 border-t border-[#eee]">
          <button onClick={onClose} className="px-3 h-7 text-xs rounded hover:bg-[#f1f3f4]">{t('common_cancel', 'Annuler')}</button>
          <button onClick={apply} className="px-3 h-7 text-xs rounded bg-primary text-white hover:opacity-90">{t('common_ok', 'OK')}</button>
        </div>
      </div>
    </>
  )
}

// ── Main SpreadsheetEditor ────────────────────────────────────────────────────

interface SpreadsheetEditorProps {
  ssId: string
  sheetMetas: SheetMeta[]
  onSheetMetasChange: (metas: SheetMeta[]) => void
  onSavingChange?: (saving: boolean) => void   // remonte l'état d'enregistrement à la topbar
  onPresenceChange?: (users: PresenceUser[]) => void  // remonte les avatars de présence à la topbar
  // Le ruban est construit ICI (où vivent les actions + le style sélectionné) puis
  // remonté au parent qui le rend dans OfficeShell.
  onRibbonChange?: (ribbon: RibbonTab[]) => void
  onNew?: () => void
  onDuplicate?: () => void
}

const SHEET_FONTS = ['Arial', 'Helvetica', 'Times New Roman', 'Courier New', 'Georgia', 'Verdana']
const SHEET_FONT_SIZES = ['8', '9', '10', '11', '12', '14', '16', '18', '20', '24', '28', '36', '48', '72']

function SpreadsheetEditor({ ssId, sheetMetas, onSheetMetasChange, onSavingChange, onPresenceChange, onRibbonChange, onNew, onDuplicate }: SpreadsheetEditorProps) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const fontFamilies = useSystemFonts(SHEET_FONTS)
  const [activeSheetId, setActiveSheetId] = useState<string>(sheetMetas[0]?.id ?? '')
  // Keep a valid active sheet: on first load the sheet list may arrive after mount,
  // and navigating to another spreadsheet reuses this component with a stale id — in
  // both cases fall back to the first sheet (e.g. opening an .xlsx selects sheet 1).
  useEffect(() => {
    if (sheetMetas.length && !sheetMetas.some(m => m.id === activeSheetId)) {
      setActiveSheetId(sheetMetas[0].id)
    }
  }, [sheetMetas, activeSheetId])

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
  const rangeEndRef = useRef(rangeEnd); rangeEndRef.current = rangeEnd
  // Format painter: the captured style to stamp onto the NEXT selected range (null = off).
  const [painterStyle, setPainterStyle] = useState<CellData['s'] | null>(null)
  const painterRef = useRef(painterStyle); painterRef.current = painterStyle
  const painterApplyRef = useRef<() => void>(() => {})
  // Find & Replace.
  const [frOpen, setFrOpen] = useState<false | 'find' | 'replace'>(false)
  const [frFind, setFrFind] = useState('')
  const [frReplace, setFrReplace] = useState('')
  const [frCase, setFrCase] = useState(false)
  const isDragSelecting = useRef(false)
  // Full row/column header selection drag (kind + anchor index/row).
  const headerSelect = useRef<{ kind: 'col' | 'row'; anchor: number } | null>(null)
  // Drag-to-MOVE selected columns/rows by their header: lo..hi = moved block, plus the live
  // insertion boundary (index before which the block drops). `moved` flips once the pointer
  // actually leaves the block, so a plain click still re-selects a single column/row.
  const headerMove = useRef<{ axis: 'col' | 'row'; lo: number; hi: number; moved: boolean } | null>(null)
  const headerMoveTarget = useRef<number | null>(null)
  const performHeaderMoveRef = useRef<() => void>(() => {})
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

  // Freeze panes: number of rows (from the top) / columns (from the left) that stay
  // pinned while scrolling. Persisted on the sheet (frozen_rows / frozen_cols).
  const [frozenRows, setFrozenRows] = useState(0)
  const [frozenCols, setFrozenCols] = useState(0)
  // Column filters: colIndex → set of allowed display values (absent = no filter).
  // A row is hidden when any filtered column's cell value is not in its allowed set.
  const [colFilters, setColFilters] = useState<Record<number, Set<string>>>({})
  const [filterMode, setFilterMode] = useState(false)  // show the funnel buttons in headers
  const [filterPopup, setFilterPopup] = useState<{ col: number; x: number; y: number } | null>(null)

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
  const [localMerges,     setLocalMerges]     = useState<string[]>([])
  const [showGridlines,   setShowGridlines]   = useState(true)
  const [zoom,            setZoom]            = useState(1)   // 1 = 100% ; scale la grille canvas
  const [bordersOpen, setBordersOpen] = useState(false)
  const [freezeOpen, setFreezeOpen] = useState(false)
  const [fillOpen, setFillOpen] = useState(false)
  const [textColorOpen, setTextColorOpen] = useState(false)
  const [nameManagerOpen, setNameManagerOpen] = useState(false)
  const textColorBtnRef = useRef<HTMLButtonElement>(null)
  const fillBtnRef = useRef<HTMLButtonElement>(null)
  const bordersBtnRef = useRef<HTMLButtonElement>(null)
  const freezeBtnRef = useRef<HTMLButtonElement>(null)
  const insertBtnRef = useRef<HTMLButtonElement>(null)
  const deleteBtnRef = useRef<HTMLButtonElement>(null)
  const symbolsBtnRef = useRef<HTMLButtonElement>(null)
  const equationBtnRef = useRef<HTMLButtonElement>(null)
  const [insertCellsOpen, setInsertCellsOpen] = useState(false)
  const [deleteCellsOpen, setDeleteCellsOpen] = useState(false)
  const [symbolsOpen, setSymbolsOpen] = useState(false)
  const [equationOpen, setEquationOpen] = useState(false)
  const resizingCol = useRef<{ col: string; startX: number; startW: number } | null>(null)
  const resizingRow = useRef<{ row: number; startY: number; startH: number } | null>(null)
  // Canvas grid rendering: the body cells, headers, gridlines and selection are
  // painted on a single viewport-sized <canvas>; only the editing input + overlays
  // remain in the DOM. `hoverEdge` drives the resize cursor on header borders.
  const canvasRef = useRef<HTMLCanvasElement>(null)
  // Transparent overlay canvas, stacked ABOVE the floating objects (images on the grid canvas,
  // DOM equations/charts), that paints only the selection chrome — so the selection shows over
  // any object while letting it remain visible underneath. Clipped to the grid body (never over
  // the sticky headers).
  const selCanvasRef = useRef<HTMLCanvasElement>(null)
  const [hoverEdge, setHoverEdge] = useState<'col' | 'row' | null>(null)
  const resizeCursor = resizingCol.current || hoverEdge === 'col' ? 'col-resize'
    : resizingRow.current || hoverEdge === 'row' ? 'row-resize' : undefined

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

  // Workbook-level defined names (named ranges / values / LAMBDAs), shared across
  // every sheet. Stored in a collaborative Y.Map (persisted with the doc) and fed
  // to the formula engine via `sheetData.names` so formulas can reference them.
  const namesMap = useMemo(() => ydoc.getMap<string>('names'), [ydoc])
  const [definedNames, setDefinedNames] = useState<Record<string, string>>({})
  useEffect(() => {
    const sync = () => { const o: Record<string, string> = {}; namesMap.forEach((v, k) => { o[k] = v }); setDefinedNames(o) }
    sync(); namesMap.observe(sync)
    return () => namesMap.unobserve(sync)
  }, [namesMap])
  const setDefinedName = useCallback((name: string, def: string) => { namesMap.set(name.trim().toUpperCase(), def) }, [namesMap])
  const deleteDefinedName = useCallback((name: string) => { namesMap.delete(name.toUpperCase()) }, [namesMap])

  // Seed workbook names from the backend (e.g. after importing an .xlsx/.ods) into
  // the Y.Map once, when the collab room has none yet.
  const namesSeeded = useRef(false)
  useEffect(() => {
    if (namesSeeded.current || collabEmpty === null) return
    const backendNames = sheet?.names
    if (!backendNames || Object.keys(backendNames).length === 0) return
    namesSeeded.current = true
    if (namesMap.size === 0) ydoc.transact(() => { for (const [k, v] of Object.entries(backendNames)) namesMap.set(k.toUpperCase(), v) }, 'local')
  }, [collabEmpty, sheet, namesMap, ydoc])

  // All sheets' cells by name — required to resolve cross-sheet references
  // ('Feuille'!A1). The active sheet's live cells are overlaid below.
  const activeSheetName = sheetMetas.find(m => m.id === activeSheetId)?.name
  const allSheetsQuery = useQuery({
    queryKey: ['spreadsheet-allsheets', ssId, sheetMetas.map(m => m.id).join(',')],
    queryFn: async () => {
      const out: Record<string, Record<string, CellData>> = {}
      await Promise.all(sheetMetas.map(async m => {
        try { const s = await spreadsheetsApi.getSheet(ssId, m.id); out[m.name] = s.data.cells } catch { out[m.name] = {} }
      }))
      return out
    },
    enabled: sheetMetas.length > 1,
    staleTime: Infinity, refetchOnWindowFocus: false,
  })

  const sheetData: SheetData = useMemo(() => {
    const cells = sheet?.data?.cells ?? {}
    const sheets = allSheetsQuery.data
      ? { ...allSheetsQuery.data, ...(activeSheetName ? { [activeSheetName]: cells } : {}) }
      : undefined
    return { cells, names: definedNames, merges: localMerges, sheets, cf: sheet?.data?.cf ?? [], colStyles: sheet?.data?.colStyles ?? {}, rowStyles: sheet?.data?.rowStyles ?? {} }
  }, [sheet?.data, definedNames, localMerges, allSheetsQuery.data, activeSheetName])

  // Effective style of a cell = column default < row default < cell-specific.
  // Whole-column/row formatting is stored at the col/row level (O(1)) instead of
  // being materialised onto every cell, so it also applies to future-empty cells.
  const colStyles = sheetData.colStyles ?? {}
  const rowStyles = sheetData.rowStyles ?? {}
  const styleAt = useCallback((col: string, row: number, cellS?: CellData['s']): NonNullable<CellData['s']> => {
    const cs = colStyles[col], rs = rowStyles[String(row)]
    if (!cs && !rs) return cellS ?? {}
    return { ...cs, ...rs, ...cellS }
  }, [colStyles, rowStyles])
  const sheetDataRef = useRef(sheetData); sheetDataRef.current = sheetData

  // Dynamic-array spill layout: which empty cells receive values spilled from a
  // formula anchor, and which anchors are blocked (#SPILL!). Recomputed only when
  // the sheet data changes. Drives the renderer, the status-bar aggregate and the
  // spill-aware value resolvers (`A1#` references the whole spilled range).
  const spill = useMemo(() => computeSpills(sheetData), [sheetData])
  const spillRef = useRef(spill); spillRef.current = spill

  // Status-bar aggregate (count/sum/avg/min/max over the selection). Memoised on the
  // selection rectangle + data so it is NOT recomputed (O(cells)) on every unrelated
  // re-render (ribbon dropdowns, zoom, hover…) — only when selection or data changes.
  const selAgg = useMemo(() => {
    if (!selectedCell) return null
    const end = rangeEnd ?? selectedCell
    const a1 = colToIndex(selectedCell.col), a2 = colToIndex(end.col)
    return selectionAggregate(sheetData, Math.min(a1, a2), Math.max(a1, a2), Math.min(selectedCell.row, end.row), Math.max(selectedCell.row, end.row), spill)
  }, [selectedCell, rangeEnd, sheetData, spill])

  // Merged cell ranges: the top-left "anchor" spans the rectangle; the other
  // "covered" cells are hidden. Derived from sheetData.merges (imported or edited).
  const mergeInfo = useMemo(() => {
    const anchors = new Map<string, { c1: number; r1: number; c2: number; r2: number }>()
    const covered = new Map<string, string>()
    for (const ref of (sheetData.merges ?? [])) {
      const b = parseRangeAddr(ref); if (!b) continue
      const aKey = `${COLS[b.c1]}${b.r1}`
      anchors.set(aKey, b)
      // Defensive: a pathological merge (e.g. a whole-column B1:D1048576 sneaking in via
      // import) must not build a million-entry covered map. The anchor still renders as a
      // single spanned block; only far-off covered hit-testing is skipped. Real merges are
      // tiny, so this never trips in practice.
      if ((b.r2 - b.r1 + 1) * (b.c2 - b.c1 + 1) > 100_000) continue
      for (let r = b.r1; r <= b.r2; r++) for (let c = b.c1; c <= b.c2; c++) { const k = `${COLS[c]}${r}`; if (k !== aKey) covered.set(k, aKey) }
    }
    return { anchors, covered }
  }, [sheetData.merges])
  const mergeInfoRef = useRef(mergeInfo); mergeInfoRef.current = mergeInfo

  // Conditional formatting → per-cell style overrides (bg / colour / bold), computed
  // once per data change (evaluates each rule's formula anchored to the cell).
  const cfOverrides = useMemo(() => computeCondFormats(sheetData), [sheetData])

  // Per-sheet default row height (imported from xlsx sheetFormatPr; else the app default).
  const sheetRowH = sheet?.data?.defaultRowHeight ?? DEFAULT_ROW_HEIGHT
  const sheetColW = sheet?.data?.defaultColWidth ?? DEFAULT_COL_WIDTH
  const getColWidth = (col: string) => localColWidths[col] ?? sheetColW
  // Auto-fit: a row WITHOUT an explicit height grows to fit its tallest cell font
  // (e.g. a 48pt title sitting in an otherwise-default row). Rows with a stored
  // height (xlsx customHeight) keep it. ~1.2× the px font size matches Excel/Calc.
  const autoRowHeight = useMemo(() => {
    const m: Record<number, number> = {}
    for (const key in sheetData.cells) {
      const mm = /^([A-Z]+)(\d+)$/.exec(key); if (!mm) continue
      const row = +mm[2]
      if (localRowHeights[String(row)] != null) continue
      const fs = sheetData.cells[key]?.s?.fontSize
      if (!fs) continue
      const h = Math.round(fs * 1.2)
      if (h > sheetRowH && h > (m[row] ?? 0)) m[row] = h
    }
    return m
  }, [sheetData.cells, localRowHeights, sheetRowH])

  // Embedded pictures (imported from xlsx drawings). Held in local state so they can
  // be moved/resized/rotated, then persisted. Decoded into HTMLImageElement objects
  // once each; a bumped tick forces a redraw as each finishes loading.
  const [localImages, setLocalImages] = useState<SheetImage[]>([])
  const localImagesRef = useRef(localImages); localImagesRef.current = localImages
  const [selectedImage, setSelectedImage] = useState<number | null>(null)
  // Index of the picture currently in interactive crop mode (double-click to enter).
  const [cropMode, setCropMode] = useState<number | null>(null)
  const cropModeRef = useRef(cropMode); cropModeRef.current = cropMode
  const sheetImages = localImages
  const imageCache = useRef<Map<string, HTMLImageElement>>(new Map())
  const [imagesTick, setImagesTick] = useState(0)
  useEffect(() => {
    for (const im of sheetImages) {
      if (!imageCache.current.has(im.src)) {
        const el = new Image()
        el.onload = () => setImagesTick(n => n + 1)
        el.src = im.src
        imageCache.current.set(im.src, el)
      }
    }
  }, [sheetImages])

  // Persist the picture list (position/size/rotation) to the sheet content.
  const saveImagesMut = useMutation({
    mutationFn: (imgs: SheetImage[]) => spreadsheetsApi.updateSheet(ssId, activeSheetId, { images: imgs }),
  })

  // Equation objects (LaTeX → KaTeX), floating DOM overlays over the grid.
  const [localEquations, setLocalEquations] = useState<SheetEquation[]>([])
  const localEquationsRef = useRef(localEquations); localEquationsRef.current = localEquations
  const [selectedEq, setSelectedEq] = useState<string | null>(null)
  const selectedEqRef = useRef(selectedEq); selectedEqRef.current = selectedEq
  const [editingEq, setEditingEq] = useState<{ id: string; latex: string } | null>(null)
  const [eqMenu, setEqMenu] = useState<{ x: number; y: number; id: string } | null>(null)
  // Chart objects (SVG, computed from a cell range), floating over the grid.
  const [localCharts, setLocalCharts] = useState<SheetChart[]>([])
  const localChartsRef = useRef(localCharts); localChartsRef.current = localCharts
  const [selectedChart, setSelectedChart] = useState<string | null>(null)
  const selectedChartRef = useRef(selectedChart); selectedChartRef.current = selectedChart
  const [chartMenu, setChartMenu] = useState<{ x: number; y: number; id: string } | null>(null)
  // Chart-insertion dialog (Excel "Insérer un graphique"): pick a type, preview, insert.
  const [chartDialog, setChartDialog] = useState<{ type: ChartType } | null>(null)
  const saveEquationsMut = useMutation({
    mutationFn: (eqs: SheetEquation[]) => spreadsheetsApi.updateSheet(ssId, activeSheetId, { equations: eqs }),
  })
  const persistEquations = (eqs: SheetEquation[]) => { setLocalEquations(eqs); saveEquationsMut.mutate(eqs) }
  // Insert an equation at the active cell. `openEditor` opens the mini editor (for a
  // blank/new equation); predefined gallery equations are inserted directly.
  const insertEquation = (latex = '', openEditor = false) => {
    const col = selectedCell ? COLS.indexOf(selectedCell.col) : 0
    const row = selectedCell ? selectedCell.row : 1
    const bx = (geom.colLeft[col] - ROW_HEADER_WIDTH) / zoom, by = (geom.rowTop[row] - COL_HEADER_HEIGHT) / zoom
    const eq: SheetEquation = { id: crypto.randomUUID(), bx, by, latex }
    persistEquations([...localEquationsRef.current, eq])
    setSelectedEq(eq.id); setSelectedImage(null); setSelectedCell(null)
    if (openEditor) setEditingEq({ id: eq.id, latex: eq.latex })
  }
  const deleteEquation = (id: string) => { persistEquations(localEquationsRef.current.filter(e => e.id !== id)); if (selectedEqRef.current === id) setSelectedEq(null); setEqMenu(null) }
  const renderKatex = (latex: string) => {
    try { return katex.renderToString(latex || '\\,', { displayMode: true, throwOnError: false, strict: false, output: 'html' }) }
    catch { return `<span style="color:#d93025">${(latex || '').replace(/</g, '&lt;')}</span>` }
  }
  const startEqDrag = (eq: SheetEquation, e: React.MouseEvent) => {
    e.stopPropagation(); e.preventDefault()
    setSelectedEq(eq.id); setSelectedImage(null); setSelectedCell(null); setRangeEnd(null)
    const sx = e.clientX, sy = e.clientY, ox = eq.bx, oy = eq.by, z = zoom
    let moved = false
    const onMove = (ev: MouseEvent) => {
      moved = true
      const dx = (ev.clientX - sx) / z, dy = (ev.clientY - sy) / z
      setLocalEquations(prev => prev.map(x => x.id === eq.id ? { ...x, bx: ox + dx, by: oy + dy } : x))
    }
    const onUp = () => {
      window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp)
      if (moved) saveEquationsMut.mutate(localEquationsRef.current)
    }
    window.addEventListener('mousemove', onMove); window.addEventListener('mouseup', onUp)
  }

  // ── Chart objects ─────────────────────────────────────────────────────────────
  const saveChartsMut = useMutation({
    mutationFn: (charts: SheetChart[]) => spreadsheetsApi.updateSheet(ssId, activeSheetId, { charts }),
  })
  const persistCharts = (charts: SheetChart[]) => { setLocalCharts(charts); saveChartsMut.mutate(charts) }
  // A bare cell ref ("R18") → its numeric value (else its displayed text).
  const cellRefVal = (ref: string): number | string => {
    const m = /^([A-Z]+)(\d+)$/.exec(ref); if (!m) return ''
    const key = `${m[1]}${m[2]}`; const cell = sheetData.cells[key]
    const n = numericValue(cell, sheetData, key, spill)
    return n != null ? n : resolveValue(cell, sheetData, key, spill)
  }
  // Chart rows. Imported charts give explicit value/category cell refs; UI charts give
  // a rectangular range (1st column = labels, 2nd = values; header row auto-detected).
  const chartData = (chart: SheetChart): { data: Record<string, unknown>[]; dimension: string; metric: string } => {
    if (chart.vals && chart.vals.length) {
      const data = chart.vals.map((ref, i) => ({ Catégorie: chart.cats?.[i] ? String(cellRefVal(chart.cats[i])) : `${i + 1}`, Valeur: cellRefVal(ref) }))
      return { data, dimension: 'Catégorie', metric: 'Valeur' }
    }
    const b = parseRangeAddr(chart.range ?? '')
    if (!b) return { data: [], dimension: 'Catégorie', metric: 'Valeur' }
    const valOf = (c: number, r: number) => cellRefVal(`${COLS[c]}${r}`)
    const valCol = Math.min(b.c1 + 1, b.c2)
    const firstVal = valOf(valCol, b.r1)
    const hasHeader = typeof firstVal === 'string' && firstVal !== ''
    const dimension = hasHeader ? String(valOf(b.c1, b.r1) || 'Catégorie') : 'Catégorie'
    const metric = hasHeader ? String(firstVal || 'Valeur') : 'Valeur'
    const data: Record<string, unknown>[] = []
    // Defensive cap: a range spanning a whole column (D1:D1048576) must never iterate
    // a million rows on every render. 4096 data points is far beyond any real chart.
    const rEnd = Math.min(b.r2, b.r1 + 4096)
    for (let r = b.r1 + (hasHeader ? 1 : 0); r <= rEnd; r++) data.push({ [dimension]: String(valOf(b.c1, r) ?? ''), [metric]: valOf(valCol, r) })
    return { data, dimension, metric }
  }
  // Content-space rect of a chart: explicit box (UI / once manipulated) else its
  // imported cell anchor (EMU offsets, like images).
  const chartRect = (chart: SheetChart) => {
    if (chart.bx != null && chart.by != null && chart.bw != null && chart.bh != null) {
      return { x: ROW_HEADER_WIDTH + chart.bx * zoom, y: COL_HEADER_HEIGHT + chart.by * zoom, w: chart.bw * zoom, h: chart.bh * zoom }
    }
    if (chart.fromCol != null && chart.fromRow != null) {
      const EMU = 9525
      const fc = Math.min(Math.max(chart.fromCol, 0), MAX_COLS), fr = Math.min(Math.max(chart.fromRow + 1, 1), MAX_ROWS)
      const x = geom.colLeft[fc] + (chart.fromColOff ?? 0) / EMU * zoom, y = geom.rowTop[fr] + (chart.fromRowOff ?? 0) / EMU * zoom
      let w: number, h: number
      if (chart.toCol != null && chart.toRow != null) {
        const tc = Math.min(Math.max(chart.toCol, 0), MAX_COLS), tr = Math.min(Math.max(chart.toRow + 1, 1), MAX_ROWS)
        w = (geom.colLeft[tc] + (chart.toColOff ?? 0) / EMU * zoom) - x; h = (geom.rowTop[tr] + (chart.toRowOff ?? 0) / EMU * zoom) - y
      } else { w = (chart.extCx ?? 0) / EMU * zoom; h = (chart.extCy ?? 0) / EMU * zoom }
      return { x, y, w, h }
    }
    return { x: ROW_HEADER_WIDTH + (chart.bx ?? 0) * zoom, y: COL_HEADER_HEIGHT + (chart.by ?? 0) * zoom, w: (chart.bw ?? 380) * zoom, h: (chart.bh ?? 240) * zoom }
  }
  const chartRectRef = useRef(chartRect); chartRectRef.current = chartRect
  // A1-range string for the current selection, capped to the used extent so a
  // whole-column/row selection yields a finite chart source (not D1:D1048576).
  const chartSourceRange = () => {
    const b = bounds(); if (!b) return 'A1:B5'
    const r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1)), c2 = Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1))
    return `${COLS[b.c1]}${b.r1}:${COLS[c2]}${r2}`
  }
  const insertChart = (type: ChartType) => {
    const b = bounds()
    const range = chartSourceRange()
    const col = b ? b.c2 + 1 : 2, row = b ? b.r1 : 1
    const bx = (geom.colLeft[Math.min(col, MAX_COLS)] - ROW_HEADER_WIDTH) / zoom, by = (geom.rowTop[row] - COL_HEADER_HEIGHT) / zoom
    const chart: SheetChart = { id: crypto.randomUUID(), bx, by, bw: 380, bh: 240, type, range }
    persistCharts([...localChartsRef.current, chart])
    setSelectedChart(chart.id ?? null); setSelectedImage(null); setSelectedEq(null); setSelectedCell(null)
  }
  const deleteChart = (id: string) => { persistCharts(localChartsRef.current.filter(c => c.id !== id)); if (selectedChartRef.current === id) setSelectedChart(null); setChartMenu(null) }
  const setChartType = (id: string, type: ChartType) => persistCharts(localChartsRef.current.map(c => c.id === id ? { ...c, type } : c))
  // Drag (move) or resize a chart via its overlay; `handle` 'move' or 'se'.
  const startChartDrag = (chart: SheetChart, handle: 'move' | 'se', e: React.MouseEvent) => {
    e.stopPropagation(); e.preventDefault()
    setSelectedChart(chart.id ?? null); setSelectedImage(null); setSelectedEq(null); setSelectedCell(null); setRangeEnd(null)
    const r = chartRectRef.current(chart)
    const o = { bx: (r.x - ROW_HEADER_WIDTH) / zoom, by: (r.y - COL_HEADER_HEIGHT) / zoom, bw: r.w / zoom, bh: r.h / zoom }
    const sx = e.clientX, sy = e.clientY, z = zoom
    let moved = false
    const onMove = (ev: MouseEvent) => {
      moved = true
      const dx = (ev.clientX - sx) / z, dy = (ev.clientY - sy) / z
      setLocalCharts(prev => prev.map(c => c.id === chart.id
        ? (handle === 'move' ? { ...c, bx: o.bx + dx, by: o.by + dy, bw: o.bw, bh: o.bh } : { ...c, bx: o.bx, by: o.by, bw: Math.max(160, o.bw + dx), bh: Math.max(120, o.bh + dy) })
        : c))
    }
    const onUp = () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); if (moved) saveChartsMut.mutate(localChartsRef.current) }
    window.addEventListener('mousemove', onMove); window.addEventListener('mouseup', onUp)
  }
  // Render the SVG chart for an object from its current data.
  const renderChart = (chart: SheetChart) => {
    const { data, dimension, metric } = chartData(chart)
    const palette = chart.colors && chart.colors.length ? chart.colors : undefined
    const common = { data, dimension, metric, title: chart.title, palette }
    switch (chart.type) {
      case 'line': return <LineChart {...common} />
      case 'area': return <LineChart {...common} />
      case 'pie': return <PieChart {...common} legend={chart.legend !== false} dataLabels={chart.dataLabels} />
      case 'scatter': return <ScatterChart data={data} xColumn={dimension} yColumn={metric} title={chart.title} />
      case 'hbar': return <BarChart {...common} horizontal />
      default: return <BarChart {...common} />
    }
  }

  const getRowHeight = (row: number) => localRowHeights[String(row)] ?? autoRowHeight[row] ?? sheetRowH

  // Plage RÉELLEMENT utilisée (dernière ligne/colonne contenant une cellule). Sert à
  // borner les rares parcours « tout le tableur » (filtres, valeurs uniques) — sinon
  // ils itéreraient 1 048 576 lignes. Les cellules sont stockées de façon éparse.
  const usedBounds = useMemo(() => {
    let maxRow = 1, maxCol = 0
    for (const k of Object.keys(sheetData.cells)) {
      const m = /^([A-Za-z]+)([0-9]+)$/.exec(k)
      if (!m) continue
      const r = +m[2]; if (r > maxRow) maxRow = r
      const c = colToIndex(m[1]); if (c > maxCol) maxCol = c
    }
    return { maxRow: Math.min(maxRow, MAX_ROWS), maxCol: Math.min(maxCol, MAX_COLS - 1) }
  }, [sheetData])

  // Étendue de défilement DYNAMIQUE (façon Excel/Sheets) : la barre ne couvre PAS
  // d'emblée les 1 048 576 lignes (le thumb serait microscopique et inutilisable).
  // L'étendue est DÉRIVÉE (pas monotone) du max entre : plage utilisée, cellule active
  // (gère les sauts Name Box/clavier), et bas/droite actuellement visibles (`viewEnd`,
  // mis à jour au défilement). → thumb toujours utilisable, qui GRANDIT quand on
  // descend dans le vide et RÉTRÉCIT quand on remonte. Réinitialisé par feuille.
  const [viewEnd, setViewEnd] = useState({ row: 0, col: 0 })
  const selR = selectedCell ? selectedCell.row : 0
  const selC = selectedCell ? colToIndex(selectedCell.col) : 0
  const extentRows = Math.min(MAX_ROWS, Math.max(usedBounds.maxRow + 40, selR + 40, viewEnd.row + 40, 60))
  const extentCols = Math.min(MAX_COLS, Math.max(usedBounds.maxCol + 6, selC + 6, viewEnd.col + 6, 30))

  // Name Box (zone Nom) : champ d'adresse éditable pour SAUTER à une cellule/plage
  // (ex. « A1 », « XFD1048576 ») — seul moyen pratique d'atteindre les bords Excel.
  const [nameBox, setNameBox] = useState('')
  const nameBoxFocused = useRef(false)
  const nameBoxRef = useRef<HTMLInputElement>(null)

  // Displayed text of a cell (formula evaluated + number format applied) — shared by
  // the canvas renderer and the column-filter value list.
  const cellText = useCallback((col: string, row: number): string => {
    const key = `${col}${row}`
    const sp = spillRef.current
    const cell = sheetDataRef.current.cells[key]
    if (!cell) return resolveValue(undefined, sheetDataRef.current, key, sp)
    const style = cell.s ?? {}
    const num = numericValue(cell, sheetDataRef.current, key, sp)
    return (num != null && (style.numFmt || style.decimals != null || style.thousands))
      ? formatNumber(num, style, i18n.language)
      : resolveValue(cell, sheetDataRef.current, key, sp)
  }, [i18n.language])

  // Rows hidden by active column filters (a row is hidden if any filtered column's
  // value is not in that column's allowed set). Empty when no filter is active.
  const hiddenRows = useMemo(() => {
    const hidden = new Set<number>()
    const active = Object.entries(colFilters).filter(([, s]) => s && s.size >= 0) as [string, Set<string>][]
    if (active.length === 0) return hidden
    // Borné à la plage utilisée (les lignes vides au-delà ne sont jamais « filtrées »).
    for (let r = 1; r <= usedBounds.maxRow; r++) {
      for (const [ci, allowed] of active) {
        if (!allowed.has(cellText(COLS[+ci], r))) { hidden.add(r); break }
      }
    }
    return hidden
  }, [colFilters, sheetData, cellText, usedBounds]) // eslint-disable-line react-hooks/exhaustive-deps

  // Cumulative pixel geometry for the canvas renderer + hit-testing. `colLeft[c]` is
  // the left edge of column `c`; `rowTop[r]` the top of row `r` (1-based, indexed
  // 1..MAX_ROWS+1). Filter-hidden rows contribute 0 height (collapsed), so the rest
  // of the renderer/hit-test simply skips them.
  const geom = useMemo(() => {
    // Colonnes : tableau réel (16 384 nombres ≈ 130 Ko, construit en <1 ms — négligeable).
    const colLeft = new Array<number>(MAX_COLS + 1)
    colLeft[0] = ROW_HEADER_WIDTH
    for (let c = 0; c < MAX_COLS; c++) colLeft[c + 1] = colLeft[c] + getColWidth(indexToCol(c)) * zoom

    // Lignes : géométrie ANALYTIQUE (échelle Excel, jusqu'à 1 048 576 lignes). On ne
    // construit JAMAIS de tableau par ligne : seules les rares lignes redimensionnées
    // ou masquées sont stockées (indices triés + préfixes des écarts au défaut), donc
    // rowY()/rowAtY() sont en O(log overrides). 1-based, valide 1..MAX_ROWS+1.
    const sset = new Set<number>()
    for (const k of Object.keys(localRowHeights)) { const n = +k; if (n >= 1 && n <= MAX_ROWS) sset.add(n) }
    for (const k of Object.keys(autoRowHeight)) { const n = +k; if (n >= 1 && n <= MAX_ROWS) sset.add(n) }
    hiddenRows.forEach(r => sset.add(r))
    const sIdx = [...sset].sort((a, b) => a - b)
    const zRowH = sheetRowH * zoom // scaled default row height (cells scale; headers stay fixed)
    const sPrefix = new Array<number>(sIdx.length + 1); sPrefix[0] = 0
    for (let i = 0; i < sIdx.length; i++) {
      const r = sIdx[i]
      const size = hiddenRows.has(r) ? 0 : getRowHeight(r) * zoom
      sPrefix[i + 1] = sPrefix[i] + (size - zRowH)
    }
    const cntBelow = (r: number) => {                          // nb d'overrides d'indice < r
      let lo = 0, hi = sIdx.length
      while (lo < hi) { const m = (lo + hi) >> 1; if (sIdx[m] < r) lo = m + 1; else hi = m }
      return lo
    }
    const rowYof = (r: number) => COL_HEADER_HEIGHT + (r - 1) * zRowH + sPrefix[cntBelow(r)]
    // Proxy : le code existant continue d'écrire `rowTop[r]` sans modification.
    const rowTop = new Proxy({} as Record<number, number>, { get: (_t, prop) => rowYof(Number(prop)) })
    return { colLeft, rowTop, rowYof, totalW: colLeft[MAX_COLS], totalH: rowYof(MAX_ROWS + 1) }
  }, [localColWidths, localRowHeights, autoRowHeight, hiddenRows, sheetRowH, sheetColW, zoom]) // eslint-disable-line react-hooks/exhaustive-deps

  // Content-space rectangle (pre-scroll, current zoom) of a picture: an explicit box
  // override (base px from the data origin, set when the user manipulates it) if present,
  // else derived from its cell anchor. `rot` is in degrees.
  const EMU_PER_PX = 9525
  const imageRect = useCallback((im: SheetImage) => {
    const { colLeft, rowTop } = geom
    const rot = im.rot ?? 0
    if (im.bx != null && im.by != null && im.bw != null && im.bh != null) {
      return { x: ROW_HEADER_WIDTH + im.bx * zoom, y: COL_HEADER_HEIGHT + im.by * zoom, w: im.bw * zoom, h: im.bh * zoom, rot }
    }
    const fc = Math.min(Math.max(im.fromCol, 0), MAX_COLS), fr = Math.min(Math.max(im.fromRow + 1, 1), MAX_ROWS)
    const x = colLeft[fc] + (im.fromColOff ?? 0) / EMU_PER_PX * zoom
    const y = rowTop[fr] + (im.fromRowOff ?? 0) / EMU_PER_PX * zoom
    let w: number, h: number
    if (im.toCol != null && im.toRow != null) {
      const tc = Math.min(Math.max(im.toCol, 0), MAX_COLS), tr = Math.min(Math.max(im.toRow + 1, 1), MAX_ROWS)
      w = (colLeft[tc] + (im.toColOff ?? 0) / EMU_PER_PX * zoom) - x
      h = (rowTop[tr] + (im.toRowOff ?? 0) / EMU_PER_PX * zoom) - y
    } else { w = (im.extCx ?? 0) / EMU_PER_PX * zoom; h = (im.extCy ?? 0) / EMU_PER_PX * zoom }
    return { x, y, w, h, rot }
  }, [geom, zoom])
  const imageRectRef = useRef(imageRect); imageRectRef.current = imageRect

  // Convert a content-space rect back to a persisted base-px box (zoom=1, from origin).
  const rectToBox = useCallback((r: { x: number; y: number; w: number; h: number }) => ({
    bx: (r.x - ROW_HEADER_WIDTH) / zoom, by: (r.y - COL_HEADER_HEIGHT) / zoom, bw: r.w / zoom, bh: r.h / zoom,
  }), [zoom])

  // Column index whose span contains content-x (or -1 outside the grid body).
  const colAtX = (x: number): number => {
    const { colLeft } = geom
    if (x < colLeft[0] || x >= colLeft[MAX_COLS]) return -1
    let lo = 0, hi = MAX_COLS
    while (lo < hi) { const m = (lo + hi) >> 1; if (colLeft[m + 1] <= x) lo = m + 1; else hi = m }
    return lo
  }
  // Row number whose span contains content-y (or -1 outside the grid body).
  const rowAtY = (y: number): number => {
    const { rowTop } = geom
    if (y < rowTop[1] || y >= rowTop[MAX_ROWS + 1]) return -1
    let lo = 1, hi = MAX_ROWS + 1
    while (lo < hi) { const m = (lo + hi) >> 1; if (rowTop[m + 1] <= y) lo = m + 1; else hi = m }
    return lo
  }

  // Suit le bas/la droite VISIBLES pour dimensionner l'étendue (throttle rAF). Quand on
  // descend dans le vide, `viewEnd` avance → l'étendue grandit (on peut continuer) ;
  // quand on remonte, elle rétrécit → thumb toujours proportionné.
  const viewEndRaf = useRef(0)
  const trackViewEnd = (el: HTMLElement) => {
    if (viewEndRaf.current) return
    viewEndRaf.current = requestAnimationFrame(() => {
      viewEndRaf.current = 0
      const row = rowAtY(el.scrollTop + el.clientHeight)
      const col = colAtX(el.scrollLeft + el.clientWidth)
      setViewEnd(prev => {
        const nr = row < 0 ? MAX_ROWS : row, nc = col < 0 ? MAX_COLS - 1 : col
        return (prev.row === nr && prev.col === nc) ? prev : { row: nr, col: nc }
      })
    })
  }

  // Défile pour rendre la cellule (c index, r ligne) visible — tient compte des volets
  // gelés. Aucun effet si déjà visible. Le navigateur clampe scrollTop à la hauteur du
  // spacer : on ne l'appelle donc qu'APRÈS extension de l'étendue (cf. layout effect).
  const doScrollIntoView = (c: number, r: number) => {
    const el = gridRef.current; if (!el) return
    const { colLeft, rowYof } = geom
    const fX = colLeft[Math.min(frozenCols, MAX_COLS)], fY = rowYof(Math.min(frozenRows, MAX_ROWS) + 1)
    const cx = colLeft[c], cw = colLeft[c + 1] - colLeft[c], cy = rowYof(r), ch = rowYof(r + 1) - rowYof(r)
    if (cx < el.scrollLeft + fX)                el.scrollLeft = Math.max(0, cx - fX)
    else if (cx + cw > el.scrollLeft + el.clientWidth) el.scrollLeft = cx + cw - el.clientWidth
    if (cy < el.scrollTop + fY)                 el.scrollTop = Math.max(0, cy - fY)
    else if (cy + ch > el.scrollTop + el.clientHeight) el.scrollTop = cy + ch - el.clientHeight
  }

  // Mise en vue de la cellule active (clavier / Name Box). Si elle est au-delà de
  // l'étendue, on l'agrandit D'ABORD puis on défile au rendu suivant (sinon le spacer
  // trop court fait clamper scrollTop). `lastScrolled` évite de re-défiler quand
  // l'étendue grandit pour une autre raison (défilement de fond).
  // L'étendue inclut déjà `selR/selC` (dérivée) → dans CE rendu le spacer est assez
  // grand pour la cellule active ; on défile donc directement (pas de double passe).
  const lastScrolled = useRef('')
  useLayoutEffect(() => {
    if (!selectedCell) return
    const key = `${selectedCell.col}${selectedCell.row}`
    if (lastScrolled.current === key) return
    lastScrolled.current = key
    doScrollIntoView(colToIndex(selectedCell.col), selectedCell.row)
  }, [selectedCell]) // eslint-disable-line react-hooks/exhaustive-deps

  // Saute à la cellule/plage saisie dans la Name Box (l'effet ci-dessus fait défiler
  // + étend l'étendue). Accepte « A1 » ou « A1:C10 » (insensible à la casse).
  const jumpToRef = (raw: string): boolean => {
    let target = raw.trim()
    // A defined name → resolve to its range definition ("=A1:C3" / "=Sheet!A1:C3").
    const named = definedNames[target.toUpperCase()]
    if (named) target = named.replace(/^=/, '').replace(/^[^!]+!/, '').replace(/\$/g, '')
    const m = /^([A-Za-z]{1,3})([0-9]{1,7})(?::([A-Za-z]{1,3})([0-9]{1,7}))?$/.exec(target)
    if (!m) return false
    const c1 = colToIndex(m[1].toUpperCase()), r1 = +m[2]
    if (c1 < 0 || c1 >= MAX_COLS || r1 < 1 || r1 > MAX_ROWS) return false
    // Snap a single-cell target to its merge anchor (a merge behaves as one cell).
    const mi = mergeInfoRef.current, k0 = `${indexToCol(c1)}${r1}`
    const aKey = mi.anchors.has(k0) ? k0 : mi.covered.get(k0)
    const a = !m[3] && aKey ? mi.anchors.get(aKey) : null
    setSelectedCell(a ? { col: indexToCol(a.c1), row: a.r1 } : { col: indexToCol(c1), row: r1 })
    if (m[3]) {
      const c2 = Math.min(colToIndex(m[3].toUpperCase()), MAX_COLS - 1), r2 = Math.min(+m[4], MAX_ROWS)
      setRangeEnd({ col: indexToCol(c2), row: r2 })
    } else setRangeEnd(null)
    return true
  }

  // Formule en cours d'édition (barre OU cellule) → encadrés colorés des plages.
  const editingFormula =
    editingCell ? cellDraft :
    (barFocused && fbDraft.startsWith('=') ? fbDraft : '')
  const refHighlights = (editingFormula.startsWith('=') ? parseRefs(editingFormula) : [])
    .map(r => {
      const b = refBounds(r.text)
      if (!b || b.c1 >= MAX_COLS || b.r1 > MAX_ROWS) return null
      const c2 = Math.min(b.c2, MAX_COLS - 1), r2 = Math.min(b.r2, MAX_ROWS)
      return {
        color: r.color,
        left: geom.colLeft[b.c1], top: geom.rowTop[b.r1],
        width: geom.colLeft[c2 + 1] - geom.colLeft[b.c1], height: geom.rowTop[r2 + 1] - geom.rowTop[b.r1],
      }
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
      left: geom.colLeft[ci], top: geom.rowTop[s.sel.row],
      width: geom.colLeft[ci + 1] - geom.colLeft[ci], height: geom.rowTop[s.sel.row + 1] - geom.rowTop[s.sel.row],
    })
  })

  // Init local sizes + freeze from server (and reset filters) when the sheet changes.
  useEffect(() => {
    if (sheet) {
      setLocalColWidths(sheet.col_widths ?? {})
      setLocalRowHeights(sheet.row_heights ?? {})
      setFrozenRows(sheet.frozen_rows ?? 0)
      setFrozenCols(sheet.frozen_cols ?? 0)
      setLocalMerges(sheet.data?.merges ?? [])
      setShowGridlines(sheet.data?.gridlines !== false)
      setLocalImages(sheet.data?.images ?? [])
      setSelectedImage(null)
      setLocalEquations(sheet.data?.equations ?? [])
      setSelectedEq(null); setEditingEq(null)
      setLocalCharts((sheet.data?.charts ?? []).map(c => c.id ? c : { ...c, id: crypto.randomUUID() }))
      setSelectedChart(null)
    }
    setColFilters({})
    setFilterMode(false)
    setFilterPopup(null)
    setViewEnd({ row: 0, col: 0 })
  }, [sheet?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const saveMut = useMutation({
    mutationFn: (data: SheetData) => spreadsheetsApi.updateSheet(ssId, activeSheetId, { data }),
    onSuccess: (updated) => {
      // KEEP the cached (optimistic) data: the PATCH echoes only `{ cells }` and as a FRESH
      // object (same content, new ref) — replacing it would wipe col/row styles AND make the
      // undo/redo recorder see a phantom edit on every save. Our optimistic cells already equal
      // what the server stored (and supersede it if a collab peer edited meanwhile).
      qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], (old: SpreadsheetSheet | undefined) =>
        old ? { ...old, ...updated, data: { ...updated.data, ...old.data } } : updated)
    },
  })

  // Remonte l'état d'enregistrement au parent (affiché dans la topbar WorkspaceShell).
  useEffect(() => { onSavingChange?.(saveMut.isPending) }, [saveMut.isPending, onSavingChange])

  const saveDimensionsMut = useMutation({
    mutationFn: (dims: { col_widths?: Record<string, number>; row_heights?: Record<string, number>; frozen_rows?: number; frozen_cols?: number; merges?: string[]; gridlines?: boolean }) =>
      spreadsheetsApi.updateSheet(ssId, activeSheetId, dims),
    onSuccess: (updated) => {
      // A dimensions-only save must NOT touch the cached data blob: its response echoes the
      // server's CURRENT cells, which may be STALE relative to an in-flight (debounced) cell
      // save (e.g. a column move commits cells then saves merges/widths). Keep `old.data`.
      qc.setQueryData(['spreadsheet-sheet', ssId, activeSheetId], (old: SpreadsheetSheet | undefined) =>
        old ? { ...old, ...updated, data: { ...updated.data, ...old.data } } : updated)
    },
  })

  // Current selection rectangle (0-based col, 1-based row), or null.
  const selRect = useCallback((): { c1: number; r1: number; c2: number; r2: number } | null => {
    if (!selectedCell) return null
    const a = { c: colToIndex(selectedCell.col), r: selectedCell.row }
    const b = rangeEnd ? { c: colToIndex(rangeEnd.col), r: rangeEnd.row } : a
    return { c1: Math.min(a.c, b.c), r1: Math.min(a.r, b.r), c2: Math.max(a.c, b.c), r2: Math.max(a.r, b.r) }
  }, [selectedCell, rangeEnd])

  // Persist + apply the merge list (optimistic).
  const applyMerges = useCallback((next: string[]) => {
    setLocalMerges(next)
    saveDimensionsMut.mutate({ merges: next })
  }, [saveDimensionsMut])

  const rectsOverlap = (a: { c1: number; r1: number; c2: number; r2: number }, b: { c1: number; r1: number; c2: number; r2: number }) =>
    a.c1 <= b.c2 && a.c2 >= b.c1 && a.r1 <= b.r2 && a.r2 >= b.r1

  const mergeSelection = useCallback(() => {
    const b = selRect(); if (!b || (b.c1 === b.c2 && b.r1 === b.r2)) return
    // Cap a whole-column/row merge to the used extent — a B1:D1048576 merge would make
    // mergeInfo build a million-entry covered map and freeze the sheet.
    const r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1))
    const c2 = Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1))
    if (b.c1 === c2 && b.r1 === r2) return
    const ref = `${COLS[b.c1]}${b.r1}:${COLS[c2]}${r2}`
    const kept = localMerges.filter(m => { const mb = parseRangeAddr(m); return !mb || !rectsOverlap(mb, b) })
    applyMerges([...kept, ref])
  }, [selRect, localMerges, applyMerges, usedBounds])

  const unmergeSelection = useCallback(() => {
    const b = selRect(); if (!b) return
    const kept = localMerges.filter(m => { const mb = parseRangeAddr(m); return !mb || !rectsOverlap(mb, b) })
    if (kept.length !== localMerges.length) applyMerges(kept)
  }, [selRect, localMerges, applyMerges])

  // Is the current selection (partly) merged? Drives the toggle button state.
  const selectionHasMerge = useMemo(() => {
    const b = selRect(); if (!b) return false
    return localMerges.some(m => { const mb = parseRangeAddr(m); return mb && rectsOverlap(mb, b) })
  }, [selRect, localMerges])
  const toggleMerge = useCallback(() => { if (selectionHasMerge) unmergeSelection(); else mergeSelection() }, [selectionHasMerge, unmergeSelection, mergeSelection])

  // Hide / show rows & columns (width/height 0 = hidden). Persisted like a resize.
  const hideCols = useCallback(() => {
    const b = selRect(); if (!b) return
    const cw = { ...localColWidths }
    for (let c = b.c1; c <= b.c2; c++) cw[COLS[c]] = 0
    setLocalColWidths(cw); saveDimensionsMut.mutate({ col_widths: cw })
  }, [selRect, localColWidths, saveDimensionsMut])
  const unhideCols = useCallback(() => {
    const b = selRect(); if (!b) return
    const cw = { ...localColWidths }; let changed = false
    for (let c = b.c1; c <= b.c2; c++) if ((cw[COLS[c]] ?? DEFAULT_COL_WIDTH) === 0) { cw[COLS[c]] = DEFAULT_COL_WIDTH; changed = true }
    if (changed) { setLocalColWidths(cw); saveDimensionsMut.mutate({ col_widths: cw }) }
  }, [selRect, localColWidths, saveDimensionsMut])
  const hideRows = useCallback(() => {
    const b = selRect(); if (!b) return
    const rh = { ...localRowHeights }
    for (let r = b.r1; r <= b.r2; r++) rh[String(r)] = 0
    setLocalRowHeights(rh); saveDimensionsMut.mutate({ row_heights: rh })
  }, [selRect, localRowHeights, saveDimensionsMut])
  const unhideRows = useCallback(() => {
    const b = selRect(); if (!b) return
    const rh = { ...localRowHeights }; let changed = false
    for (let r = b.r1; r <= b.r2; r++) if ((rh[String(r)] ?? DEFAULT_ROW_HEIGHT) === 0) { rh[String(r)] = DEFAULT_ROW_HEIGHT; changed = true }
    if (changed) { setLocalRowHeights(rh); saveDimensionsMut.mutate({ row_heights: rh }) }
  }, [selRect, localRowHeights, saveDimensionsMut])
  // Set an exact height (px) on every selected row / width (px) on every selected column.
  const promptRowHeight = async () => {
    const b = selRect(); if (!b) return
    const v = await appPrompt(t('sheet_set_row_height', { defaultValue: 'Hauteur de ligne (pixels) :' }), String(Math.round(getRowHeight(b.r1))))
    if (v == null) return
    const px = Math.round(Number(v)); if (!Number.isFinite(px) || px < 0) return
    const rh = { ...localRowHeights }
    for (let r = b.r1; r <= b.r2; r++) rh[String(r)] = px
    setLocalRowHeights(rh); saveDimensionsMut.mutate({ row_heights: rh })
  }
  const promptColWidth = async () => {
    const b = selRect(); if (!b) return
    const v = await appPrompt(t('sheet_set_col_width', { defaultValue: 'Largeur de colonne (pixels) :' }), String(Math.round(getColWidth(COLS[b.c1]))))
    if (v == null) return
    const px = Math.round(Number(v)); if (!Number.isFinite(px) || px < 0) return
    const cw = { ...localColWidths }
    for (let c = b.c1; c <= b.c2; c++) cw[COLS[c]] = px
    setLocalColWidths(cw); saveDimensionsMut.mutate({ col_widths: cw })
  }

  // Autofit columns [c1..c2] to their widest cell content (double-click the header edge).
  const autofitColumns = (c1: number, c2: number) => {
    const ctx = canvasRef.current?.getContext('2d'); if (!ctx) return
    const max: Record<number, number> = {}
    for (const key in sheetData.cells) {
      const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) continue
      const c = colToIndex(m[1]); if (c < c1 || c > c2) continue
      const txt = cellText(m[1], +m[2]); if (!txt) continue
      const s = sheetData.cells[key].s ?? {}
      ctx.font = `${s.italic ? 'italic ' : ''}${s.bold ? 'bold ' : ''}${s.fontSize ?? 13}px ${s.fontFamily || 'Arial'}, sans-serif`
      const w = ctx.measureText(String(txt)).width + 12
      if (w > (max[c] ?? 0)) max[c] = w
    }
    const cw = { ...localColWidths }
    for (let c = c1; c <= c2; c++) cw[COLS[c]] = Math.max(40, Math.min(Math.round(max[c] ?? DEFAULT_COL_WIDTH), 600))
    setLocalColWidths(cw); saveDimensionsMut.mutate({ col_widths: cw })
  }

  // Toggle the sheet's default gridlines (persisted).
  const toggleGridlines = useCallback(() => {
    setShowGridlines(v => { const next = !v; saveDimensionsMut.mutate({ gridlines: next }); return next })
  }, [saveDimensionsMut])

  // Set the frozen panes (optimistic state + persisted on the sheet).
  const applyFreeze = useCallback((rows: number, cols: number) => {
    const r = Math.max(0, Math.min(MAX_ROWS, rows)), c = Math.max(0, Math.min(MAX_COLS, cols))
    setFrozenRows(r); setFrozenCols(c)
    saveDimensionsMut.mutate({ frozen_rows: r, frozen_cols: c })
  }, [saveDimensionsMut])

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

  // ── Undo / Redo ───────────────────────────────────────────────────────────────
  // A snapshot holds REFERENCES to the immutable editable state (cells, col/row styles,
  // merges, dimensions, freeze) — replaced wholesale on each edit, so reference inequality
  // means a real change and the recording effect stays O(1). Recording is centralised: any
  // change to the watched state (not from a remote sync or a restore) pushes the previous
  // snapshot onto the undo stack.
  type HistSnap = { cells: Record<string, CellData>; colStyles?: SheetData['colStyles']; rowStyles?: SheetData['rowStyles']; merges: string[]; colW: Record<string, number>; rowH: Record<string, number>; fr: number; fc: number }
  const histRef = useRef<{ undo: HistSnap[]; redo: HistSnap[]; last: HistSnap | null; restoreTo: HistSnap | null }>({ undo: [], redo: [], last: null, restoreTo: null })
  const [, setHistTick] = useState(0)
  const snapNow = useCallback((): HistSnap => ({
    cells: sheet?.data?.cells ?? {}, colStyles: sheet?.data?.colStyles, rowStyles: sheet?.data?.rowStyles,
    merges: localMerges, colW: localColWidths, rowH: localRowHeights, fr: frozenRows, fc: frozenCols,
  }), [sheet?.data, localMerges, localColWidths, localRowHeights, frozenRows, frozenCols])
  const sameSnap = (a: HistSnap, b: HistSnap) => a.cells === b.cells && a.colStyles === b.colStyles && a.rowStyles === b.rowStyles && a.merges === b.merges && a.colW === b.colW && a.rowH === b.rowH && a.fr === b.fr && a.fc === b.fc
  useEffect(() => {
    const h = histRef.current, cur = snapNow()
    if (h.last == null) { h.last = cur; return }
    if (sameSnap(cur, h.last)) return
    if (h.restoreTo) { h.last = cur; if (sameSnap(cur, h.restoreTo)) h.restoreTo = null; return }
    if (applyingRemote.current) { h.last = cur; return }
    h.undo.push(h.last); if (h.undo.length > 200) h.undo.shift()
    h.redo = []; h.last = cur
    setHistTick(t => t + 1)
  }, [snapNow]) // eslint-disable-line react-hooks/exhaustive-deps
  // Reset history when switching sheets (snapshots are sheet-specific).
  useEffect(() => { histRef.current = { undo: [], redo: [], last: null, restoreTo: null }; setHistTick(t => t + 1) }, [activeSheetId])
  const applySnapshot = useCallback((s: HistSnap) => {
    histRef.current.restoreTo = s
    setLocalMerges(s.merges); setLocalColWidths(s.colW); setLocalRowHeights(s.rowH)
    setFrozenRows(s.fr); setFrozenCols(s.fc)
    commitData({ ...sheetDataRef.current, cells: s.cells, colStyles: s.colStyles, rowStyles: s.rowStyles })
    saveDimensionsMut.mutate({ merges: s.merges, col_widths: s.colW, row_heights: s.rowH, frozen_rows: s.fr, frozen_cols: s.fc })
  }, [commitData, saveDimensionsMut])
  const undo = useCallback(() => {
    const h = histRef.current; if (!h.undo.length) return
    h.redo.push(snapNow()); const s = h.undo.pop() as HistSnap; h.last = s
    applySnapshot(s); setHistTick(t => t + 1)
  }, [snapNow, applySnapshot])
  const redo = useCallback(() => {
    const h = histRef.current; if (!h.redo.length) return
    h.undo.push(snapNow()); const s = h.redo.pop() as HistSnap; h.last = s
    applySnapshot(s); setHistTick(t => t + 1)
  }, [snapNow, applySnapshot])
  const canUndo = histRef.current.undo.length > 0
  const canRedo = histRef.current.redo.length > 0

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

  // Coin bas-droit de la sélection (emplacement de la poignée de recopie). Étendu au
  // coin bas-droit d'une fusion présente à ce coin (la fusion = une seule cellule).

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

  // The merge rectangle covering (col,row), or null. A merged region behaves as a
  // single cell: clicking it selects its anchor + spans it; arrows skip over it.
  const mergeRectAt = (col: string, row: number): { c1: number; r1: number; c2: number; r2: number } | null => {
    const mi = mergeInfoRef.current
    const k = `${col}${row}`
    if (mi.anchors.has(k)) return mi.anchors.get(k) ?? null
    const a = mi.covered.get(k)
    return a ? (mi.anchors.get(a) ?? null) : null
  }
  // Snap (col,row) to its merge anchor (top-left) when it is part of a merge.
  const anchorOf = (col: string, row: number): { col: string; row: number } => {
    const r = mergeRectAt(col, row)
    return r ? { col: COLS[r.c1], row: r.r1 } : { col, row }
  }

  const handleCellClick = (col: string, row: number, e: React.MouseEvent) => {
    commitEditingDraft()
    if (e.shiftKey && selectedCell) {
      setRangeEnd({ col, row })
    } else {
      setSelectedCell(anchorOf(col, row))
      setRangeEnd(null)
    }
  }

  const handleCellMouseDown = (col: string, row: number, e: React.MouseEvent) => {
    if (e.shiftKey) return // handled by onClick
    if (e.button !== 0) return
    commitEditingDraft()
    isDragSelecting.current = true
    setSelectedCell(anchorOf(col, row))
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
    const a = anchorOf(col, row)
    setSelectedCell(a)
    setRangeEnd(null)
    startEditCell(a.col, a.row)
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
      // Treat a merged cell as one unit: step out from its far edge in the direction.
      const ci = COLS.indexOf(prev.col)
      const cur = mergeRectAt(prev.col, prev.row) ?? { c1: ci, r1: prev.row, c2: ci, r2: prev.row }
      let nc = cur.c1, nr = cur.r1
      if (dir === 'right') nc = Math.min(cur.c2 + 1, MAX_COLS - 1)
      if (dir === 'left')  nc = Math.max(cur.c1 - 1, 0)
      if (dir === 'down') { let r = cur.r2; do { r++ } while (r <= MAX_ROWS && hiddenRows.has(r)); if (r <= MAX_ROWS) nr = r }
      if (dir === 'up')   { let r = cur.r1; do { r-- } while (r >= 1 && hiddenRows.has(r)); if (r >= 1) nr = r }
      return anchorOf(COLS[nc], nr) // snap onto the merge anchor if we land in one
    })
    setRangeEnd(null)
  }, [hiddenRows]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleTab = useCallback((shift: boolean) => {
    if (shift) moveSelection('left')
    else moveSelection('right')
  }, [moveSelection])

  // Ctrl+Arrow: jump to the edge of the contiguous data block (Excel behaviour). `extend`
  // moves the range end (Ctrl+Shift+Arrow) instead of the active cell.
  const jumpToEdge = useCallback((dir: 'up' | 'down' | 'left' | 'right', extend: boolean) => {
    const cells = sheetDataRef.current.cells
    const has = (c: number, r: number) => { const v = cells[`${COLS[c]}${r}`]; return !!v && ((v.v != null && v.v !== '') || !!v.f) }
    const dc = dir === 'left' ? -1 : dir === 'right' ? 1 : 0
    const dr = dir === 'up' ? -1 : dir === 'down' ? 1 : 0
    setSelectedCell(sel => {
      if (!sel) return sel
      const from = extend && rangeEndRef.current ? rangeEndRef.current : sel
      let c = COLS.indexOf(from.col), r = from.row
      const inR = (cc: number, rr: number) => cc >= 0 && cc < MAX_COLS && rr >= 1 && rr <= MAX_ROWS
      let nc = c + dc, nr = r + dr
      if (inR(nc, nr)) {
        if (has(nc, nr)) { while (inR(nc + dc, nr + dr) && has(nc + dc, nr + dr)) { nc += dc; nr += dr } }
        else { while (inR(nc + dc, nr + dr) && !has(nc + dc, nr + dr)) { nc += dc; nr += dr } if (inR(nc + dc, nr + dr)) { nc += dc; nr += dr } }
        c = nc; r = nr
      }
      if (extend) { setRangeEnd({ col: COLS[c], row: r }); return sel }
      setRangeEnd(null); return { col: COLS[c], row: r }
    })
  }, [rangeEnd]) // eslint-disable-line react-hooks/exhaustive-deps

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
    // Cap a whole-column/row copy to the used extent — cloning a million empty cells
    // (and holding them on the clipboard) would freeze the sheet.
    const r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1)), c2 = Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1))
    const cells: (CellData | undefined)[][] = []
    for (let r = b.r1; r <= r2; r++) {
      const row: (CellData | undefined)[] = []
      for (let c = b.c1; c <= c2; c++) { const cd = sheetData.cells[cellKey(COLS[c], r)]; row.push(cd ? structuredClone(cd) : undefined) }
      cells.push(row)
    }
    clipboard.current = { cells, rows: r2 - b.r1 + 1, cols: c2 - b.c1 + 1, cut, originCol: b.c1, originRow: b.r1 }
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
    // Cap to the used extent: clearing a whole empty column needn't touch a million cells.
    const r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1)), c2 = Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1))
    const entries: { col: string; row: number; cell: CellData | null }[] = []
    for (let r = b.r1; r <= r2; r++) for (let c = b.c1; c <= c2; c++) entries.push({ col: COLS[c], row: r, cell: null })
    writeCells(entries)
  }

  // Paste special: only part of the clipboard (values / formats / formulas / transposed /
  // everything-but-borders). Destination formatting is kept where the mode pastes content only.
  const pasteSpecial = (mode: 'values' | 'format' | 'formula' | 'transpose' | 'noborders') => {
    const cb = clipboard.current; if (!cb || !selectedCell) return
    const destC = COLS.indexOf(selectedCell.col), destR = selectedCell.row
    const stripBorders = (s?: CellData['s']) => { if (!s) return s; const n = { ...s } as Record<string, unknown>; for (const kk of ['bt', 'bb', 'bl', 'br', 'btw', 'bbw', 'blw', 'brw']) delete n[kk]; return n as CellData['s'] }
    const entries: { col: string; row: number; cell: CellData | null }[] = []
    for (let r = 0; r < cb.rows; r++) for (let c = 0; c < cb.cols; c++) {
      const src = cb.cells[r][c]
      const tc = mode === 'transpose' ? destC + r : destC + c
      const tr = mode === 'transpose' ? destR + c : destR + r
      if (tc < 0 || tc >= MAX_COLS || tr < 1 || tr > MAX_ROWS) continue
      const existing = sheetData.cells[cellKey(COLS[tc], tr)]
      if (!src) { if (mode !== 'format') entries.push({ col: COLS[tc], row: tr, cell: existing?.s ? { s: existing.s } : null }); continue }
      const dCol = tc - (cb.originCol + c), dRow = tr - (cb.originRow + r)
      const tf = (f?: string | null) => (f && f.startsWith('=')) ? translateFormula(f, dCol, dRow) : (f ?? undefined)
      let cell: CellData
      if (mode === 'format') cell = { ...(existing ?? {}), s: src.s ? structuredClone(src.s) : undefined }
      else if (mode === 'values') cell = { ...(existing ?? {}), v: src.v ?? null, f: undefined }
      else if (mode === 'formula') cell = { ...(existing ?? {}), v: src.f ? undefined : (src.v ?? null), f: tf(src.f) }
      else { cell = structuredClone(src); cell.f = tf(src.f); if (mode === 'noborders') cell.s = stripBorders(cell.s) }
      entries.push({ col: COLS[tc], row: tr, cell })
    }
    writeCells(entries)
  }

  // Insert blank cells, shifting the affected band of populated cells right / down.
  const insertCellsShift = (dir: 'right' | 'down') => {
    const b = bounds(); if (!b) return
    const cnt = dir === 'down' ? b.r2 - b.r1 + 1 : b.c2 - b.c1 + 1
    const out: Record<string, CellData> = {}
    for (const key in sheetData.cells) {
      const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) continue
      const c = colToIndex(m[1]), r = +m[2]
      const inBand = dir === 'down' ? (c >= b.c1 && c <= b.c2 && r >= b.r1) : (r >= b.r1 && r <= b.r2 && c >= b.c1)
      if (!inBand) { out[key] = sheetData.cells[key]; continue }
      const nc = dir === 'right' ? c + cnt : c, nr = dir === 'down' ? r + cnt : r
      if (nc < MAX_COLS && nr <= MAX_ROWS) out[`${indexToCol(nc)}${nr}`] = sheetData.cells[key]
    }
    commitData({ ...sheetData, cells: out })
  }

  // Delete the selected cells, shifting the band of cells below / to the right back up / left.
  const deleteCellsShift = (dir: 'left' | 'up') => {
    const b = bounds(); if (!b) return
    const cnt = dir === 'up' ? b.r2 - b.r1 + 1 : b.c2 - b.c1 + 1
    const out: Record<string, CellData> = {}
    for (const key in sheetData.cells) {
      const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) continue
      const c = colToIndex(m[1]), r = +m[2]
      if (dir === 'up') {
        if (c < b.c1 || c > b.c2) { out[key] = sheetData.cells[key]; continue }
        if (r >= b.r1 && r <= b.r2) continue
        if (r > b.r2) out[`${indexToCol(c)}${r - cnt}`] = sheetData.cells[key]; else out[key] = sheetData.cells[key]
      } else {
        if (r < b.r1 || r > b.r2) { out[key] = sheetData.cells[key]; continue }
        if (c >= b.c1 && c <= b.c2) continue
        if (c > b.c2) out[`${indexToCol(c - cnt)}${r}`] = sheetData.cells[key]; else out[key] = sheetData.cells[key]
      }
    }
    commitData({ ...sheetData, cells: out })
  }

  // Move a block of columns [lo..hi] so it lands BEFORE column `before` (drag-reorder by
  // header). Relocates cell content+formatting, column styles, widths and merges; the cells in
  // between shift to fill the gap. Formula refs are NOT re-pointed (known limitation).
  const moveColumns = (lo: number, hi: number, before: number) => {
    if (before >= lo && before <= hi + 1) return // dropped in place
    const w = hi - lo + 1
    const map = new Map<number, number>()
    if (before > hi + 1) { for (let c = lo; c <= hi; c++) map.set(c, before - w + (c - lo)); for (let c = hi + 1; c <= before - 1; c++) map.set(c, c - w) }
    else { for (let c = lo; c <= hi; c++) map.set(c, before + (c - lo)); for (let c = before; c <= lo - 1; c++) map.set(c, c + w) }
    const mc = (c: number) => map.get(c) ?? c
    const cells: Record<string, CellData> = {}
    for (const key in sheetData.cells) { const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) { cells[key] = sheetData.cells[key]; continue } cells[`${indexToCol(mc(colToIndex(m[1])))}${m[2]}`] = sheetData.cells[key] }
    let colStyles = sheetData.colStyles
    if (colStyles) { const cs: Record<string, NonNullable<CellData['s']>> = {}; for (const k in colStyles) cs[indexToCol(mc(colToIndex(k)))] = colStyles[k]; colStyles = cs }
    const merges = (localMerges ?? []).map(ref => { const b = parseRangeAddr(ref); if (!b) return ref; const a = mc(b.c1), z = mc(b.c2); return `${indexToCol(Math.min(a, z))}${b.r1}:${indexToCol(Math.max(a, z))}${b.r2}` })
    const cw: Record<string, number> = {}; for (const k in localColWidths) cw[indexToCol(mc(colToIndex(k)))] = localColWidths[k]
    setLocalColWidths(cw); setLocalMerges(merges)
    commitData({ ...sheetData, cells, colStyles })
    saveDimensionsMut.mutate({ merges, col_widths: cw })
    const nl = mc(lo); setSelectedCell({ col: COLS[nl], row: 1 }); setRangeEnd({ col: COLS[nl + w - 1], row: MAX_ROWS })
  }

  // Move a block of rows [lo..hi] so it lands BEFORE row `before` (drag-reorder by header).
  const moveRows = (lo: number, hi: number, before: number) => {
    if (before >= lo && before <= hi + 1) return
    const w = hi - lo + 1
    const map = new Map<number, number>()
    if (before > hi + 1) { for (let r = lo; r <= hi; r++) map.set(r, before - w + (r - lo)); for (let r = hi + 1; r <= before - 1; r++) map.set(r, r - w) }
    else { for (let r = lo; r <= hi; r++) map.set(r, before + (r - lo)); for (let r = before; r <= lo - 1; r++) map.set(r, r + w) }
    const mr = (r: number) => map.get(r) ?? r
    const cells: Record<string, CellData> = {}
    for (const key in sheetData.cells) { const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) { cells[key] = sheetData.cells[key]; continue } cells[`${m[1]}${mr(+m[2])}`] = sheetData.cells[key] }
    let rowStyles = sheetData.rowStyles
    if (rowStyles) { const rs: Record<string, NonNullable<CellData['s']>> = {}; for (const k in rowStyles) rs[String(mr(+k))] = rowStyles[k]; rowStyles = rs }
    const merges = (localMerges ?? []).map(ref => { const b = parseRangeAddr(ref); if (!b) return ref; const a = mr(b.r1), z = mr(b.r2); return `${indexToCol(b.c1)}${Math.min(a, z)}:${indexToCol(b.c2)}${Math.max(a, z)}` })
    const rh: Record<string, number> = {}; for (const k in localRowHeights) rh[String(mr(+k))] = localRowHeights[k]
    setLocalRowHeights(rh); setLocalMerges(merges)
    commitData({ ...sheetData, cells, rowStyles })
    saveDimensionsMut.mutate({ merges, row_heights: rh })
    const nl = mr(lo); setSelectedCell({ col: COLS[0], row: nl }); setRangeEnd({ col: COLS[MAX_COLS - 1], row: nl + w - 1 })
  }

  // Commit the in-progress header move on mouse-up.
  const performHeaderMove = () => {
    const hm = headerMove.current, before = headerMoveTarget.current
    if (hm && hm.moved && before != null) { if (hm.axis === 'col') moveColumns(hm.lo, hm.hi, before); else moveRows(hm.lo, hm.hi, before) }
  }
  performHeaderMoveRef.current = performHeaderMove

  // Copy a textual deep-link to the active cell to the clipboard.
  const copyCellLink = () => {
    if (!selectedCell) return
    const link = `${location.origin}${location.pathname}#${activeSheetName ? `${activeSheetName}!` : ''}${selectedCell.col}${selectedCell.row}`
    navigator.clipboard?.writeText(link).then(() => void appAlert(t('sheet_link_copied', { defaultValue: 'Lien de la cellule copié.' }))).catch(() => {})
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
      headerSelect.current = null
      if (isFilling.current) { performFillRef.current(); isFilling.current = false; fillStart.current = null; setFillTo(null) }
      if (headerMove.current) { performHeaderMoveRef.current(); headerMove.current = null; headerMoveTarget.current = null; drawSelRef.current() }
      if (autoScrollRaf.current) { cancelAnimationFrame(autoScrollRaf.current); autoScrollRaf.current = null }
      dragPointer.current = null
      // Format painter: stamp the captured style onto the just-finished selection, then clear.
      if (painterRef.current !== null) painterApplyRef.current()
    }
    window.addEventListener('mouseup', up)
    return () => window.removeEventListener('mouseup', up)
  }, [])

  // Global key handler (only when grid has logical focus, not form elements)
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't intercept when the keystroke targets ANOTHER editable surface : champ,
      // contenteditable, éditeur Monaco (input = DIV `native-edit-context`, PAS une
      // textarea → instanceof ne suffit pas), ou toute fenêtre flottante marquée
      // `data-kubuno-floating` (ex. IDE de macros). Sinon Espace/Entrée y ouvrent une
      // édition de cellule qui vole le focus.
      const tgt = e.target as HTMLElement | null
      if (tgt && (tgt instanceof HTMLInputElement || tgt instanceof HTMLTextAreaElement
        || tgt.isContentEditable || tgt.closest('.monaco-editor') || tgt.closest('[data-kubuno-floating]'))) return
      // A selected equation takes Delete/Backspace and Escape.
      if (selectedEqRef.current && !editingEq) {
        if (e.key === 'Delete' || e.key === 'Backspace') { deleteEquation(selectedEqRef.current); e.preventDefault(); return }
        if (e.key === 'Escape') { setSelectedEq(null); e.preventDefault(); return }
      }
      // A selected chart takes Delete/Backspace and Escape.
      if (selectedChartRef.current) {
        if (e.key === 'Delete' || e.key === 'Backspace') { deleteChart(selectedChartRef.current); e.preventDefault(); return }
        if (e.key === 'Escape') { setSelectedChart(null); e.preventDefault(); return }
      }
      // Escape leaves crop mode first (applies the current crop).
      if (cropModeRef.current !== null && e.key === 'Escape') { setCropMode(null); e.preventDefault(); return }
      // A selected picture takes Delete/Backspace (and Escape to deselect).
      if (selectedImageRef.current !== null && !editingCell && cropModeRef.current === null) {
        if (e.key === 'Delete' || e.key === 'Backspace') {
          const idx = selectedImageRef.current
          setLocalImages(prev => { const next = prev.filter((_, i) => i !== idx); saveImagesMut.mutate(next); return next })
          setSelectedImage(null); e.preventDefault(); return
        }
        if (e.key === 'Escape') { setSelectedImage(null); e.preventDefault(); return }
      }
      // Undo / redo work without a cell selection (but not while editing a cell — let the
      // browser handle text undo there).
      if ((e.ctrlKey || e.metaKey) && !editingCell) {
        const z = e.key.toLowerCase()
        if (z === 'z' && !e.shiftKey) { undo(); e.preventDefault(); return }
        if (z === 'y' || (z === 'z' && e.shiftKey)) { redo(); e.preventDefault(); return }
        if (z === 'f') { setFrOpen('find'); e.preventDefault(); return }
        if (z === 'h') { setFrOpen('replace'); e.preventDefault(); return }
      }
      // Escape cancels the format painter.
      if (e.key === 'Escape' && painterRef.current !== null) { setPainterStyle(null); e.preventDefault(); return }
      if (!selectedCell || editingCell) return

      if (e.ctrlKey || e.metaKey) {
        const k = e.key.toLowerCase()
        if (k === 'c') { copySelection(false); e.preventDefault(); return }
        if (k === 'x') { copySelection(true);  e.preventDefault(); return }
        if (k === 'v') { pasteSelection();      e.preventDefault(); return }
        if (k === 'a') { setSelectedCell({ col: COLS[0], row: 1 }); setRangeEnd({ col: COLS[Math.max(usedBounds.maxCol, 0)], row: Math.max(usedBounds.maxRow, 1) }); e.preventDefault(); return }
        if (e.key === 'Home') { setSelectedCell({ col: COLS[0], row: 1 }); setRangeEnd(null); e.preventDefault(); return }
        if (e.key === 'ArrowUp')    { jumpToEdge('up',    e.shiftKey); e.preventDefault(); return }
        if (e.key === 'ArrowDown')  { jumpToEdge('down',  e.shiftKey); e.preventDefault(); return }
        if (e.key === 'ArrowLeft')  { jumpToEdge('left',  e.shiftKey); e.preventDefault(); return }
        if (e.key === 'ArrowRight') { jumpToEdge('right', e.shiftKey); e.preventDefault(); return }
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
  }, [selectedCell, rangeEnd, sheetData, editingCell, moveSelection, updateCell, undo, redo, jumpToEdge, usedBounds]) // eslint-disable-line react-hooks/exhaustive-deps

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
      // The drag delta is in (zoomed) screen px; stored width is unscaled → ÷ zoom.
      const newW = Math.max(20, startW + (e.clientX - startX) / zoom)
      setLocalColWidths(prev => ({ ...prev, [col]: newW }))
    }
    if (resizingRow.current) {
      const { row, startY, startH } = resizingRow.current
      const newH = Math.max(14, startH + (e.clientY - startY) / zoom)
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

  // Toggle a boolean style over the WHOLE selection (a single active cell uses its own
  // value; a range/whole-column toggles off the current EFFECTIVE state). Routed through
  // applyToSelection so whole-column/row selections are stored at the col/row level.
  const toggleStyle = (prop: keyof NonNullable<CellData['s']>) => {
    if (!selectedCell) return
    applyToSelection({ [prop]: !selectedCellStyle[prop as keyof typeof selectedCellStyle] })
  }

  const setAlign = (align: 'left' | 'center' | 'right') => applyToSelection({ align })

  // Applique un patch de style à toute la sélection (plage incluse).
  // Merge a style patch into a base record; keys set to `undefined` are removed
  // (a toggle-off must not leave a dead key that defeats inheritance).
  const mergeStyle = (base: CellData['s'], patch: Partial<NonNullable<CellData['s']>>): NonNullable<CellData['s']> => {
    const s = { ...(base ?? {}) } as Record<string, unknown>
    for (const k in patch) { const v = (patch as Record<string, unknown>)[k]; if (v === undefined) delete s[k]; else s[k] = v }
    return s as NonNullable<CellData['s']>
  }

  const applyToSelection = (patch: Partial<NonNullable<CellData['s']>>) => {
    if (!selectedCell) return
    const end = rangeEnd ?? selectedCell
    const sc = COLS.indexOf(selectedCell.col), ec = COLS.indexOf(end.col)
    const c1 = Math.min(sc, ec), c2raw = Math.max(sc, ec)
    const r1 = Math.min(selectedCell.row, end.row), r2raw = Math.max(selectedCell.row, end.row)
    // A header click selects the FULL column (rows 1..MAX) or FULL row (cols 0..MAX).
    // Such "select everything" formatting is stored once at the column/row level
    // instead of being materialised onto every cell — O(1), and it also applies to
    // cells typed later. Excel-faithful precedence: cell > row > column (see styleAt).
    const wholeCols = r1 === 1 && r2raw >= MAX_ROWS
    const wholeRows = c1 === 0 && c2raw >= MAX_COLS - 1
    if (wholeCols || wholeRows) {
      const newData: SheetData = { ...sheetData }
      if (wholeCols) {
        const cs = { ...(sheetData.colStyles ?? {}) }
        for (let ci = c1; ci <= c2raw; ci++) cs[COLS[ci]] = mergeStyle(cs[COLS[ci]], patch)
        newData.colStyles = cs
      }
      if (wholeRows) {
        const rs = { ...(sheetData.rowStyles ?? {}) }
        for (let ri = r1; ri <= r2raw; ri++) rs[String(ri)] = mergeStyle(rs[String(ri)], patch)
        newData.rowStyles = rs
      }
      // Strip the patched keys from any per-cell override inside the band, so the new
      // column/row default actually shows (an existing cell style would otherwise win).
      // Iterates only POPULATED cells, so it stays cheap even on a whole-column apply.
      const cells = { ...sheetData.cells }
      let changed = false
      for (const key in cells) {
        const km = /^([A-Z]+)(\d+)$/.exec(key); if (!km) continue
        const ci = COLS.indexOf(km[1]), ri = +km[2]
        const inBand = (wholeCols && ci >= c1 && ci <= c2raw) || (wholeRows && ri >= r1 && ri <= r2raw)
        if (!inBand) continue
        const s = cells[key].s; if (!s) continue
        let touched = false; const ns = { ...s } as Record<string, unknown>
        for (const k in patch) { if (k in ns) { delete ns[k]; touched = true } }
        if (touched) { cells[key] = { ...cells[key], s: ns as NonNullable<CellData['s']> }; changed = true }
      }
      if (changed) newData.cells = cells
      commitData(newData)
      return
    }
    // Bounded rectangle: per-cell. No cap — a manual drag selection is whatever the
    // user dragged (at most the visible area); whole-column/row went through the
    // col/row branch above, so this can never explode to MAX_ROWS×MAX_COLS.
    const cells = { ...sheetData.cells }
    for (let ci = c1; ci <= c2raw; ci++) {
      for (let ri = r1; ri <= r2raw; ri++) {
        const k = cellKey(COLS[ci], ri)
        const cell = cells[k] ?? {}
        cells[k] = { ...cell, s: mergeStyle(cell.s, patch) }
      }
    }
    commitData({ ...sheetData, cells })
  }

  // Format painter: stamp a captured style over the current selection (replacing each cell's
  // formatting), capped to the used extent for whole-column/row selections.
  const applyPainter = (style: CellData['s']) => {
    const b = bounds(); if (!b) return
    const c2 = Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1)), r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1))
    const cells = { ...sheetData.cells }
    for (let ci = b.c1; ci <= c2; ci++) for (let ri = b.r1; ri <= r2; ri++) {
      const k = cellKey(COLS[ci], ri); const cell = cells[k]
      if (style && Object.keys(style).length) cells[k] = { ...(cell ?? {}), s: { ...style } }
      else if (cell?.s) cells[k] = { ...cell, s: undefined }
    }
    commitData({ ...sheetData, cells })
  }
  painterApplyRef.current = () => { const ps = painterRef.current; if (ps !== null) { applyPainter(ps); setPainterStyle(null) } }

  // Remove duplicate rows within the selection (keep the first occurrence; rows below shift up).
  const removeDuplicates = () => {
    const b = bounds(); if (!b) return
    const r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1))
    const rowKey = (r: number) => { const parts: string[] = []; for (let c = b.c1; c <= b.c2; c++) parts.push(cellText(COLS[c], r)); return parts.join('') }
    const seen = new Set<string>(); const keepRows: number[] = []
    for (let r = b.r1; r <= r2; r++) { const k = rowKey(r); if (seen.has(k)) continue; seen.add(k); keepRows.push(r) }
    const removed = (r2 - b.r1 + 1) - keepRows.length
    if (removed === 0) { void appAlert(t('sheet_dup_none', { defaultValue: 'Aucun doublon trouvé.' })); return }
    const cells = { ...sheetData.cells }
    // Clear the selection band then re-lay the kept rows compactly from the top.
    for (let r = b.r1; r <= r2; r++) for (let c = b.c1; c <= b.c2; c++) delete cells[cellKey(COLS[c], r)]
    keepRows.forEach((srcR, i) => { for (let c = b.c1; c <= b.c2; c++) { const src = sheetData.cells[cellKey(COLS[c], srcR)]; if (src) cells[cellKey(COLS[c], b.r1 + i)] = src } })
    commitData({ ...sheetData, cells })
    void appAlert(t('sheet_dup_removed', { defaultValue: '{{n}} ligne(s) en double supprimée(s).', n: removed }))
  }

  // ── Find & Replace ────────────────────────────────────────────────────────────
  const frMatches = () => {
    const q = frCase ? frFind : frFind.toLowerCase(); if (!q) return [] as { c: number; r: number }[]
    const out: { c: number; r: number }[] = []
    for (const key in sheetData.cells) {
      const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) continue
      const txt = cellText(m[1], +m[2])
      if ((frCase ? txt : txt.toLowerCase()).includes(q)) out.push({ c: colToIndex(m[1]), r: +m[2] })
    }
    return out.sort((a, b) => a.r - b.r || a.c - b.c)
  }
  const frFindNext = (back = false) => {
    const ms = frMatches(); if (!ms.length) { void appAlert(t('sheet_fr_none', { defaultValue: 'Aucun résultat.' })); return }
    const cur = selectedCell ? { c: colToIndex(selectedCell.col), r: selectedCell.row } : { c: -1, r: 1 }
    let idx: number
    if (back) { for (idx = ms.length - 1; idx >= 0; idx--) if (ms[idx].r < cur.r || (ms[idx].r === cur.r && ms[idx].c < cur.c)) break; if (idx < 0) idx = ms.length - 1 }
    else { idx = ms.findIndex(m => m.r > cur.r || (m.r === cur.r && m.c > cur.c)); if (idx === -1) idx = 0 }
    const tgt = ms[idx]; setSelectedCell({ col: COLS[tgt.c], row: tgt.r }); setRangeEnd(null)
  }
  const frReplaceAll = () => {
    const q = frCase ? frFind : frFind.toLowerCase(); if (!q) return
    const re = new RegExp(frFind.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), frCase ? 'g' : 'gi')
    const cells = { ...sheetData.cells }; let n = 0
    for (const key in cells) {
      const cell = cells[key]
      const isF = cell.f != null
      const val = isF ? cell.f! : (cell.v != null ? String(cell.v) : '')
      if (typeof val !== 'string' || !(frCase ? val : val.toLowerCase()).includes(q)) continue
      const nv = val.replace(re, frReplace)
      if (nv !== val) { n++; cells[key] = isF ? { ...cell, f: nv } : { ...cell, v: nv } }
    }
    if (n) commitData({ ...sheetData, cells })
    void appAlert(t('sheet_fr_replaced', { defaultValue: '{{n}} remplacement(s) effectué(s).', n }))
  }
  const frReplaceOne = () => {
    if (!selectedCell || !frFind) { frFindNext(); return }
    const key = cellKey(selectedCell.col, selectedCell.row); const cell = sheetData.cells[key]
    const isF = !!cell?.f; const val = isF ? cell!.f! : (cell?.v != null ? String(cell!.v) : '')
    const re = new RegExp(frFind.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), frCase ? '' : 'i')
    if ((frCase ? val : val.toLowerCase()).includes(frCase ? frFind : frFind.toLowerCase())) {
      const nv = val.replace(re, frReplace)
      commitData({ ...sheetData, cells: { ...sheetData.cells, [key]: isF ? { ...cell!, f: nv } : { ...cell, v: nv } } })
    }
    frFindNext()
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
    // Only a whole-column/row selection is capped to the used extent (else
    // MAX_ROWS×MAX_COLS cells). A bounded manual selection is applied in full.
    const wholeCols = b.r1 === 1 && b.r2 >= MAX_ROWS
    const wholeRows = b.c1 === 0 && b.c2 >= MAX_COLS - 1
    const c2 = wholeRows ? Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1)) : b.c2
    const r2 = wholeCols ? Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1)) : b.r2
    const cells = { ...sheetData.cells }
    for (let ci = b.c1; ci <= c2; ci++) {
      for (let ri = b.r1; ri <= r2; ri++) {
        const k = cellKey(COLS[ci], ri)
        const cell = cells[k] ?? {}
        const s = { ...(cell.s ?? {}) }
        const isTop = ri === b.r1, isBottom = ri === r2
        const isLeft = ci === b.c1, isRight = ci === c2
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
    ? styleAt(selectedCell.col, selectedCell.row, sheetData.cells[cellKey(selectedCell.col, selectedCell.row)]?.s)
    : {}

  // A selected picture owns the Name Box (Excel shows "Image N", no cell reference).
  const cellAddressLabel = selectedImage != null
    ? t('sheet_image_label', { defaultValue: 'Image {{n}}', n: selectedImage + 1 })
    : selectedCell
    ? (rangeEnd && (rangeEnd.col !== selectedCell.col || rangeEnd.row !== selectedCell.row)
        ? `${selectedCell.col}${selectedCell.row}:${rangeEnd.col}${rangeEnd.row}`
        : `${selectedCell.col}${selectedCell.row}`)
    : ''

  // Reflète l'adresse courante dans la Name Box tant qu'on n'y tape pas.
  useEffect(() => { if (!nameBoxFocused.current) setNameBox(cellAddressLabel) }, [cellAddressLabel])

  // Animated bounding rect of a MULTI-cell selection (content-space, scroll offset applied
  // at draw via fx/fy). The outer selection border is drawn at this rect so it grows/slides
  // to its new bounds instead of jumping. Single-cell selections show only the active-cell
  // box (drawn separately); `multi` gates whether the border is animated/shown.
  const animRangeRef = useRef<{ x: number; y: number; w: number; h: number; fx: boolean; fy: boolean; multi: boolean } | null>(null)
  const rangeAnimRaf = useRef<number | null>(null)
  const lastRangeKey = useRef<string | null>(null)
  const selRangeTarget = useCallback(() => {
    if (!selectedCell) return null
    const end = rangeEnd ?? selectedCell
    const sc = COLS.indexOf(selectedCell.col), ec = COLS.indexOf(end.col)
    const c1 = Math.min(sc, ec)
    let c2 = Math.max(sc, ec)
    const r1 = Math.min(selectedCell.row, end.row)
    let r2 = Math.max(selectedCell.row, end.row)
    if (c1 < 0 || r1 < 1 || r2 > MAX_ROWS) return null
    // Extend the bottom-right corner over a merged cell — same as the fill handle
    // (fillCorner) so the border, fill and handle all sit on the same corner.
    const mi = mergeInfoRef.current, k = `${COLS[c2]}${r2}`
    const aKey = mi.anchors.has(k) ? k : mi.covered.get(k)
    const mr = aKey ? mi.anchors.get(aKey) : undefined
    if (mr) { c2 = Math.max(c2, mr.c2); r2 = Math.max(r2, mr.r2) }
    const { colLeft, rowTop } = geom
    if (rowTop[r2 + 1] <= rowTop[r1]) return null
    const fc = Math.min(frozenCols, MAX_COLS), fr = Math.min(frozenRows, MAX_ROWS)
    return { x: colLeft[c1], y: rowTop[r1], w: colLeft[c2 + 1] - colLeft[c1], h: rowTop[r2 + 1] - rowTop[r1], fx: c1 < fc, fy: r1 <= fr, multi: c1 !== c2 || r1 !== r2 }
  }, [selectedCell, rangeEnd, geom, frozenCols, frozenRows])
  // Read the latest target via a ref so the tween effect can depend on selection only —
  // selecting cells grows the dynamic extent (→ new geom → new selRangeTarget); depending on
  // it directly would re-fire the effect and snap-cancel the slide mid-flight.
  const selRangeTargetRef = useRef(selRangeTarget); selRangeTargetRef.current = selRangeTarget

  // ── Canvas grid renderer ──────────────────────────────────────────────────────
  // Paints the visible viewport only: backgrounds, gridlines, custom borders, text,
  // selection highlight + outline, then the sticky row/column headers and corner.
  const drawGrid = useCallback(() => {
    const canvas = canvasRef.current, gridEl = gridRef.current
    if (!canvas || !gridEl) return
    const vw = gridEl.clientWidth, vh = gridEl.clientHeight
    if (vw === 0 || vh === 0) return
    const dpr = window.devicePixelRatio || 1
    if (canvas.width !== Math.round(vw * dpr) || canvas.height !== Math.round(vh * dpr)) {
      canvas.width = Math.round(vw * dpr); canvas.height = Math.round(vh * dpr)
      canvas.style.width = `${vw}px`; canvas.style.height = `${vh}px`
    }
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, vw, vh)

    const sl = gridEl.scrollLeft, st = gridEl.scrollTop
    const { colLeft, rowTop } = geom
    const lang = i18n.language
    const data = sheetData
    const cells = data.cells

    // Draw one cell's text (formula resolved + number format) within a rect. Shared
    // by the normal grid loop and the merged-cell pass (which passes a span rect).
    const drawCellContent = (key: string, cell: CellData | undefined, x: number, y: number, w: number, h: number, c?: number, r?: number) => {
      const km = c == null || r == null ? /^([A-Z]+)(\d+)$/.exec(key) : null
      const style = styleAt(km ? km[1] : COLS[c!], km ? +km[2] : r!, cell?.s)
      const num = numericValue(cell, data, key, spill)
      const display = (num != null && style.numFmtCode) ? formatCode(num, style.numFmtCode)
        : (num != null && (style.numFmt || style.decimals != null || style.thousands)) ? formatNumber(num, style, lang)
        : resolveValue(cell, data, key, spill)
      if (display === '') return
      const cfo = cfOverrides[key]
      const align = style.align ?? (num != null ? 'right' : 'left')
      const fs = (style.fontSize ?? 13) * zoom
      ctx.font = `${(cfo?.italic ?? style.italic) ? 'italic ' : ''}${(cfo?.bold ?? style.bold) ? 'bold ' : ''}${fs}px ${style.fontFamily || 'Arial'}, sans-serif`
      ctx.fillStyle = cfo?.color ?? style.color ?? '#202124'
      const padX = 4
      let tx: number
      if (align === 'right') { ctx.textAlign = 'right'; tx = x + w - padX }
      else if (align === 'center') { ctx.textAlign = 'center'; tx = x + w / 2 }
      else { ctx.textAlign = 'left'; tx = x + padX }
      // Vertical anchor (centre line of the text block) honouring valign.
      const valign = style.valign ?? 'center'
      const vmid = (blockH: number) => valign === 'top' ? y + blockH / 2 + 2 : valign === 'bottom' ? y + h - blockH / 2 - 2 : y + h / 2
      // Text overflow: an unwrapped text cell whose content is wider than its column
      // spills over the neighbouring EMPTY cells (Excel behaviour). Extend the clip.
      let clipX = x, clipW = w
      if (c != null && r != null && num == null && !style.wrap) {
        const textW = ctx.measureText(String(display)).width + padX * 2
        if (textW > w) {
          const free = (cc: number): boolean => {
            if (cc < 0 || cc >= MAX_COLS) return false
            const k = `${COLS[cc]}${r}`
            if (mergeInfo.covered.has(k) || mergeInfo.anchors.has(k)) return false
            if (spill.values[k] !== undefined) return false
            const nc = cells[k]
            return !(nc && ((nc.v != null && nc.v !== '') || nc.f))
          }
          if (align !== 'right') { let cc = c + 1; while (clipW < textW && free(cc)) { clipW += colLeft[cc + 1] - colLeft[cc]; cc++ } }
          if (align !== 'left')  { let cc = c - 1; while (clipW < textW && free(cc)) { const cw = colLeft[cc + 1] - colLeft[cc]; clipX -= cw; clipW += cw; cc-- } }
        }
      }
      // Vertical clip: unwrapped text taller than its row overflows up/down (Excel keeps
      // an oversized title fully visible across short rows instead of clipping it).
      const vover = style.wrap ? 0 : Math.max(0, (fs * 1.2 - h) / 2)
      ctx.save(); ctx.beginPath(); ctx.rect(clipX, y - vover, clipW, h + 2 * vover); ctx.clip()
      if (style.wrap) {
        const lh = fs * 1.25, lines = wrapText(ctx, String(display), w - padX * 2)
        let ty = vmid(lines.length * lh) - (lines.length - 1) * lh / 2 + 1
        for (const ln of lines) { ctx.fillText(ln, tx, ty); ty += lh }
      } else {
        const s = String(display), ty = vmid(fs) + 1
        ctx.fillText(s, tx, ty)
        if (style.underline || style.strike) {
          const tw = Math.min(ctx.measureText(s).width, w - padX * 2)
          const lx = align === 'right' ? tx - tw : align === 'center' ? tx - tw / 2 : tx
          ctx.strokeStyle = ctx.fillStyle as string; ctx.lineWidth = 1
          if (style.underline) { const uy = Math.round(ty + fs * 0.42) + 0.5; ctx.beginPath(); ctx.moveTo(lx, uy); ctx.lineTo(lx + tw, uy); ctx.stroke() }
          if (style.strike)    { const my = Math.round(ty) + 0.5;             ctx.beginPath(); ctx.moveTo(lx, my); ctx.lineTo(lx + tw, my); ctx.stroke() }
        }
      }
      ctx.restore()
    }

    // Crisp line helpers drawn in DEVICE pixels: the stored width (1 thin / 2 medium /
    // 3 thick) maps to that many PHYSICAL pixels, so the different edge weights Excel
    // uses stay distinct. Doing this in device space also avoids a naive 1px-logical
    // stroke becoming 2 physical px on a HiDPI/retina screen (dpr=2) and reading as too
    // thick. lineWidth is expressed in logical units (ww/dpr → ww device px); the centre
    // is snapped to a device half-pixel (odd ww) or pixel (even ww) so the stroke fills
    // whole physical pixels.
    const hlineAt = (color: string, ww: number, x1: number, x2: number, y0: number) => { ctx.strokeStyle = color; ctx.lineWidth = ww / dpr; const yy = (Math.round(y0 * dpr) + (ww % 2 ? 0.5 : 0)) / dpr; ctx.beginPath(); ctx.moveTo(x1, yy); ctx.lineTo(x2, yy); ctx.stroke() }
    const vlineAt = (color: string, ww: number, y1: number, y2: number, x0: number) => { ctx.strokeStyle = color; ctx.lineWidth = ww / dpr; const xx = (Math.round(x0 * dpr) + (ww % 2 ? 0.5 : 0)) / dpr; ctx.beginPath(); ctx.moveTo(xx, y1); ctx.lineTo(xx, y2); ctx.stroke() }
    // Of two borders meeting on a shared edge, the heavier (wider) one wins; a plain
    // colour defaults to 1px. Returns null when neither edge defines a border.
    const pickEdge = (c1?: string, w1?: number, c2?: string, w2?: number): { color: string; w: number } | null => {
      const a = c1 ? { color: c1, w: w1 ?? 1 } : null
      const b = c2 ? { color: c2, w: w2 ?? 1 } : null
      if (!a) return b
      if (!b) return a
      return a.w >= b.w ? a : b
    }
    // The merge anchor key covering a cell (the cell itself if it is an anchor),
    // or null when the cell is not merged. Used to skip only INTERIOR merge edges
    // (both sides in the same merge) while keeping boundary borders (e.g. a cell's
    // border against a neighbouring merged region).
    const originOf = (c: number, r: number): string | null => { const k = `${COLS[c]}${r}`; return mergeInfo.anchors.has(k) ? k : (mergeInfo.covered.get(k) ?? null) }
    // Draw a single cell's borders (used by the merged-cell pass, whose rectangle has no neighbours to share with).
    const drawBorders = (st: NonNullable<CellData['s']>, x: number, y: number, w: number, h: number) => {
      if (st.bt) hlineAt(st.bt, st.btw ?? 1, x, x + w, y)
      if (st.bb) hlineAt(st.bb, st.bbw ?? 1, x, x + w, y + h)
      if (st.bl) vlineAt(st.bl, st.blw ?? 1, y, y + h, x)
      if (st.br) vlineAt(st.br, st.brw ?? 1, y, y + h, x + w)
      ctx.lineWidth = 1
    }

    // Freeze boundaries in content coords (= screen coords for the pinned bands).
    const fCols = Math.min(frozenCols, MAX_COLS)
    const fRows = Math.min(frozenRows, MAX_ROWS)
    const freezeX = colLeft[fCols]
    const freezeY = rowTop[fRows + 1]

    // Paint a cell block [c0..cN]×[r0..rN], mapped to screen via (offX,offY) and
    // clipped to the rect [cx0,cy0,cx1,cy1]. Used once per freeze quadrant.
    const paint = (c0: number, cN: number, r0: number, rN: number, offX: number, offY: number, cx0: number, cy0: number, cx1: number, cy1: number) => {
      if (c0 > cN || r0 > rN || cx1 <= cx0 || cy1 <= cy0) return
      const sx = (cx: number) => cx - offX, sy = (cy: number) => cy - offY
      ctx.save()
      ctx.beginPath(); ctx.rect(cx0, cy0, cx1 - cx0, cy1 - cy0); ctx.clip()

      // Backgrounds (selection highlight wins, then gradient, then solid bg).
      for (let c = c0; c <= cN; c++) {
        const col = COLS[c], x = sx(colLeft[c]), w = colLeft[c + 1] - colLeft[c]
        for (let r = r0; r <= rN; r++) {
          const h = rowTop[r + 1] - rowTop[r]; if (h === 0) continue
          const y = sy(rowTop[r])
          // The selection tint is painted on the TOP overlay canvas (above images/objects),
          // not here — cells render their own background only.
          const bgKey = `${col}${r}`
          if (mergeInfo.covered.has(bgKey) || mergeInfo.anchors.has(bgKey)) continue
          const style = styleAt(col, r, cells[bgKey]?.s)
          if (style?.bgGradient) { ctx.fillStyle = makeCanvasGradient(ctx, style.bgGradient, x, y, w, h); ctx.fillRect(x, y, w, h) }
          else { const bg = cfOverrides[bgKey]?.bg ?? style?.bg; if (bg) { ctx.fillStyle = bg; ctx.fillRect(x, y, w, h) } } // CF fill overrides the base
        }
      }

      // Default gridlines (each cell's right + bottom edge) — hidden when the sheet
      // has gridlines turned off (only explicit borders show).
      if (showGridlines) {
        // Snap to a device half-pixel so a 1-device-px line stays crisp on any DPI
        // (a plain +0.5 logical offset renders 2 physical px on a retina screen).
        const dsnap = (v: number) => (Math.round(v * dpr) + 0.5) / dpr
        ctx.strokeStyle = '#e2e4e6'; ctx.lineWidth = 1 / dpr; ctx.beginPath()
        for (let c = c0; c <= cN; c++) { const xr = dsnap(sx(colLeft[c + 1])); ctx.moveTo(xr, sy(rowTop[r0])); ctx.lineTo(xr, sy(rowTop[rN + 1])) }
        for (let r = r0; r <= rN; r++) { if (rowTop[r + 1] === rowTop[r]) continue; const yb = dsnap(sy(rowTop[r + 1])); ctx.moveTo(sx(colLeft[c0]), yb); ctx.lineTo(sx(colLeft[cN + 1]), yb) }
        ctx.stroke()
      }

      // Explicit borders — drawn ONCE per shared edge (two adjacent cells share a
      // single line; the heavier border wins). Merged cells are handled separately.
      const sOf = (c: number, r: number) => (c >= 0 && c < MAX_COLS && r >= 1 && r <= MAX_ROWS) ? styleAt(COLS[c], r, cells[`${COLS[c]}${r}`]?.s) : undefined
      const colVisible = (c: number) => c >= 0 && c < MAX_COLS && colLeft[c + 1] > colLeft[c] // width > 0 (not hidden)
      // Vertical edges (between column c and c+1). Hidden (0-width) columns must not
      // paint their borders, otherwise they stack at the collapsed x as a stray line.
      for (let c = c0 - 1; c <= cN; c++) {
        for (let r = r0; r <= rN; r++) {
          if (rowTop[r + 1] === rowTop[r]) continue
          const oa = originOf(c, r), ob = originOf(c + 1, r); if (oa && oa === ob) continue // interior merge edge
          const lc = colVisible(c) ? sOf(c, r) : undefined, rc = colVisible(c + 1) ? sOf(c + 1, r) : undefined
          if (!lc?.br && !rc?.bl) continue
          const e = pickEdge(lc?.br, lc?.brw, rc?.bl, rc?.blw); if (!e) continue
          vlineAt(e.color, e.w, sy(rowTop[r]), sy(rowTop[r + 1]), sx(colLeft[c + 1]))
        }
      }
      // Horizontal edges (between row r and r+1).
      for (let r = r0 - 1; r <= rN; r++) {
        if (rowTop[r + 1] === rowTop[r]) continue
        for (let c = c0; c <= cN; c++) {
          const oa = originOf(c, r), ob = originOf(c, r + 1); if (oa && oa === ob) continue // interior merge edge
          const tc = sOf(c, r), bc = sOf(c, r + 1)
          if (!tc?.bb && !bc?.bt) continue
          const e = pickEdge(tc?.bb, tc?.bbw, bc?.bt, bc?.btw); if (!e) continue
          hlineAt(e.color, e.w, sx(colLeft[c]), sx(colLeft[c + 1]), sy(rowTop[r + 1]))
        }
      }
      ctx.lineWidth = 1

      // Text.
      ctx.textBaseline = 'middle'
      // Left-overflow pass: a cell whose ORIGIN column is left of the viewport but whose
      // unwrapped text overflows rightward INTO it must still be drawn (Excel keeps a long
      // title visible while you scroll past its origin column). For each row, the only
      // candidate is the first non-empty cell scanning left from c0-1 (any cells between it
      // and c0 are empty, else they would block the overflow). drawCellContent's own clip +
      // the quadrant clip reveal just the visible tail; a non-overflowing cell paints
      // off-screen and is clipped away. Scan is bounded so a far-left empty stretch is cheap.
      if (c0 > 0) {
        const SCAN_BACK = 256
        for (let r = r0; r <= rN; r++) {
          const h = rowTop[r + 1] - rowTop[r]; if (h === 0) continue
          const lo = Math.max(0, c0 - SCAN_BACK)
          for (let cc = c0 - 1; cc >= lo; cc--) {
            const k = `${COLS[cc]}${r}`
            if (mergeInfo.covered.has(k) || mergeInfo.anchors.has(k) || spill.values[k] !== undefined) break
            const cell = cells[k]
            if (cell && ((cell.v != null && cell.v !== '') || cell.f)) {
              drawCellContent(k, cell, sx(colLeft[cc]), sy(rowTop[r]), colLeft[cc + 1] - colLeft[cc], h, cc, r)
              break
            }
          }
        }
      }
      for (let c = c0; c <= cN; c++) {
        const x = sx(colLeft[c]), w = colLeft[c + 1] - colLeft[c]
        for (let r = r0; r <= rN; r++) {
          const h = rowTop[r + 1] - rowTop[r]; if (h === 0) continue
          const key = `${COLS[c]}${r}`
          const cell = cells[key]
          // Render real cells AND empty cells that receive a spilled value.
          if (!cell && spill.values[key] === undefined) continue
          if (mergeInfo.covered.has(key) || mergeInfo.anchors.has(key)) continue // merges drawn below
          drawCellContent(key, cell, x, sy(rowTop[r]), w, h, c, r)
        }
      }

      // Merged cells in TWO passes: fill every region first, THEN draw borders +
      // content. Otherwise an adjacent merge's fill would cover the previous merge's
      // shared border (e.g. the per-row separators of stacked M:N merges vanished).
      const visMerges: { b: { c1: number; r1: number; c2: number; r2: number }; aKey: string; x: number; y: number; w: number; hh: number }[] = []
      for (const [aKey, b] of mergeInfo.anchors) {
        if (b.c2 < c0 || b.c1 > cN || b.r2 < r0 || b.r1 > rN) continue
        const x = sx(colLeft[b.c1]), y = sy(rowTop[b.r1])
        const w = colLeft[b.c2 + 1] - colLeft[b.c1], hh = rowTop[b.r2 + 1] - rowTop[b.r1]
        if (w <= 0 || hh <= 0) continue
        visMerges.push({ b, aKey, x, y, w, hh })
      }
      for (const { aKey, b, x, y, w, hh } of visMerges) {
        const st = styleAt(COLS[b.c1], b.r1, cells[aKey]?.s)
        // Selection tint handled on the overlay canvas; merged cell shows its own fill.
        ctx.fillStyle = cfOverrides[aKey]?.bg ?? st?.bg ?? '#ffffff'
        ctx.fillRect(x, y, w, hh)
      }
      for (const { aKey, b, x, y, w, hh } of visMerges) {
        const st = cells[aKey]?.s
        // Outer borders from the PERIMETER cells (not just the anchor): left from the
        // left column, right from the right column, bottom from the bottom row — so a
        // table's right edge stored on the merge's last column still shows.
        const sL = cells[`${COLS[b.c1]}${b.r1}`]?.s, sR = cells[`${COLS[b.c2]}${b.r1}`]?.s, sB = cells[`${COLS[b.c1]}${b.r2}`]?.s
        const mb = { bt: st?.bt, btw: st?.btw, bb: sB?.bb, bbw: sB?.bbw, bl: sL?.bl, blw: sL?.blw, br: sR?.br, brw: sR?.brw }
        if (mb.bt || mb.br || mb.bb || mb.bl) drawBorders(mb, x, y, w, hh)
        drawCellContent(aKey, cells[aKey], x, y, w, hh)
      }
      ctx.restore()
    }

    // Visible scrollable ranges (the part beyond the frozen bands).
    let sc0 = colAtX(sl + freezeX); if (sc0 < 0) sc0 = fCols; sc0 = Math.max(sc0, fCols)
    let scN = colAtX(sl + vw - 1); if (scN < 0) scN = MAX_COLS - 1
    let sr0 = rowAtY(st + freezeY); if (sr0 < 0) sr0 = fRows + 1; sr0 = Math.max(sr0, fRows + 1)
    let srN = rowAtY(st + vh - 1); if (srN < 0) srN = MAX_ROWS

    // Four quadrants: body (scrolls both), frozen-top, frozen-left, corner.
    paint(sc0, scN, sr0, srN, sl, st, freezeX, freezeY, vw, vh)
    if (fRows > 0) paint(sc0, scN, 1, fRows, sl, 0, freezeX, COL_HEADER_HEIGHT, vw, freezeY)
    if (fCols > 0) paint(0, fCols - 1, sr0, srN, 0, st, ROW_HEADER_WIDTH, freezeY, freezeX, vh)
    if (fCols > 0 && fRows > 0) paint(0, fCols - 1, 1, fRows, 0, 0, ROW_HEADER_WIDTH, COL_HEADER_HEIGHT, freezeX, freezeY)

    // Embedded pictures (xlsx drawings). Anchored to cells in grid coordinates with
    // EMU offsets (1 px = 9525 EMU). Drawn in the scrollable body, clipped to the grid
    // (frozen panes are not special-cased — uncommon for imported picture sheets).
    if (sheetImages.length) {
      ctx.save()
      ctx.beginPath(); ctx.rect(ROW_HEADER_WIDTH, COL_HEADER_HEIGHT, vw - ROW_HEADER_WIDTH, vh - COL_HEADER_HEIGHT); ctx.clip()
      sheetImages.forEach((im, idx) => {
        const el = imageCache.current.get(im.src)
        if (!el || !el.complete || !el.naturalWidth) return
        const r = imageRect(im)
        const left = r.x - sl, top = r.y - st, w = r.w, h = r.h
        if (w <= 0 || h <= 0) return
        const cx = left + w / 2, cy = top + h / 2
        // Crop mode: show the whole image dimmed, the kept region bright, plus a crop frame.
        if (cropMode === idx) {
          const cl = im.cropL ?? 0, ct = im.cropT ?? 0, cr = im.cropR ?? 0, cb = im.cropB ?? 0
          const kw = Math.max(0.02, 1 - cl - cr), kh = Math.max(0.02, 1 - ct - cb)
          const fw = w / kw, fh = h / kh, fxl = left - cl * fw, fyt = top - ct * fh
          ctx.save()
          ctx.globalAlpha = 0.35; ctx.drawImage(el, fxl, fyt, fw, fh); ctx.globalAlpha = 1
          ctx.save(); ctx.beginPath(); ctx.rect(left, top, w, h); ctx.clip(); ctx.drawImage(el, fxl, fyt, fw, fh); ctx.restore()
          ctx.strokeStyle = '#000'; ctx.lineWidth = 1; ctx.strokeRect(fxl, fyt, fw, fh)
          ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 2; ctx.strokeRect(left, top, w, h)
          // Corner brackets as crop handles.
          ctx.fillStyle = '#1a73e8'
          const corners: [number, number][] = [[left, top], [cx, top], [left + w, top], [left, cy], [left + w, cy], [left, top + h], [cx, top + h], [left + w, top + h]]
          for (const [hx, hy] of corners) ctx.fillRect(hx - 4, hy - 4, 8, 8)
          ctx.restore()
          return
        }
        ctx.save()
        if (r.rot) { ctx.translate(cx, cy); ctx.rotate(r.rot * Math.PI / 180); ctx.translate(-cx, -cy) }
        // Honour crop insets (a:srcRect): draw only the kept source sub-rectangle.
        const cl = im.cropL ?? 0, ct = im.cropT ?? 0, cr = im.cropR ?? 0, cb = im.cropB ?? 0
        if (cl || ct || cr || cb) {
          const nw = el.naturalWidth, nh = el.naturalHeight
          ctx.drawImage(el, cl * nw, ct * nh, Math.max(1, (1 - cl - cr) * nw), Math.max(1, (1 - ct - cb) * nh), left, top, w, h)
        } else {
          ctx.drawImage(el, left, top, w, h)
        }
        // Selection chrome: outline + 8 resize handles + a rotation handle.
        if (selectedImage === idx) {
          ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 1.5; ctx.setLineDash([])
          ctx.strokeRect(left, top, w, h)
          const hs: [number, number][] = [
            [left, top], [cx, top], [left + w, top],
            [left, cy], [left + w, cy],
            [left, top + h], [cx, top + h], [left + w, top + h],
          ]
          // Rotation handle above the top-centre.
          ctx.beginPath(); ctx.moveTo(cx, top); ctx.lineTo(cx, top - 22); ctx.stroke()
          ctx.fillStyle = '#fff'
          ctx.beginPath(); ctx.arc(cx, top - 22, 5, 0, Math.PI * 2); ctx.fill(); ctx.stroke()
          for (const [hx, hy] of hs) { ctx.beginPath(); ctx.arc(hx, hy, 4.5, 0, Math.PI * 2); ctx.fill(); ctx.stroke() }
        }
        ctx.restore()
      })
      ctx.restore()
    }

    // Selection chrome (transparent fill + active-cell box + range border + spill outline)
    // is painted on a SEPARATE top overlay canvas (drawSelection) so it sits ABOVE images and
    // DOM objects while staying transparent — not here.

    // Freeze divider lines.
    if (fCols > 0 || fRows > 0) {
      ctx.strokeStyle = '#9aa0a6'; ctx.lineWidth = 1; ctx.beginPath()
      if (fCols > 0) { const x = Math.round(freezeX) - 0.5; ctx.moveTo(x, COL_HEADER_HEIGHT); ctx.lineTo(x, vh) }
      if (fRows > 0) { const y = Math.round(freezeY) - 0.5; ctx.moveTo(ROW_HEADER_WIDTH, y); ctx.lineTo(vw, y) }
      ctx.stroke()
    }

    // ── Column headers (frozen cols pinned, scrollable cols scroll) ──
    const drawColHeaders = (c0: number, cN: number, offX: number, hx0: number, hx1: number) => {
      if (c0 > cN || hx1 <= hx0) return
      const sx = (cx: number) => cx - offX
      ctx.save(); ctx.beginPath(); ctx.rect(hx0, 0, hx1 - hx0, COL_HEADER_HEIGHT); ctx.clip()
      ctx.textBaseline = 'middle'
      for (let c = c0; c <= cN; c++) {
        const col = COLS[c], x = sx(colLeft[c]), w = colLeft[c + 1] - colLeft[c]
        ctx.fillStyle = isColHighlighted(col) ? '#d3e3fd' : '#f8f9fa'
        ctx.fillRect(x, 0, w, COL_HEADER_HEIGHT)
        ctx.fillStyle = '#444'; ctx.font = '500 12px Arial, sans-serif'; ctx.textAlign = 'center'
        ctx.fillText(col, x + w / 2, COL_HEADER_HEIGHT / 2 + 1)
        if (filterMode || colFilters[c]) drawFunnel(ctx, x + w - 13, COL_HEADER_HEIGHT / 2, !!colFilters[c])
      }
      ctx.strokeStyle = '#c1c7cd'; ctx.lineWidth = 1; ctx.beginPath()
      for (let c = c0; c <= cN; c++) { const xr = Math.round(sx(colLeft[c + 1])) + 0.5; ctx.moveTo(xr, 0); ctx.lineTo(xr, COL_HEADER_HEIGHT) }
      ctx.stroke()
      // Hidden-column indicator: a thick dark tick where columns are collapsed (width 0).
      ctx.strokeStyle = '#5f6368'; ctx.lineWidth = 2; ctx.beginPath()
      for (let c = Math.max(c0, 1); c <= cN; c++) {
        if (colLeft[c + 1] > colLeft[c] && colLeft[c] === colLeft[c - 1]) { const xm = Math.round(sx(colLeft[c])); ctx.moveTo(xm, 3); ctx.lineTo(xm, COL_HEADER_HEIGHT - 3) }
      }
      ctx.stroke(); ctx.lineWidth = 1; ctx.restore()
    }
    drawColHeaders(sc0, scN, sl, freezeX, vw)
    if (fCols > 0) drawColHeaders(0, fCols - 1, 0, ROW_HEADER_WIDTH, freezeX)
    ctx.strokeStyle = '#c1c7cd'; ctx.lineWidth = 1; ctx.beginPath()
    const colHb = Math.round(COL_HEADER_HEIGHT) - 0.5; ctx.moveTo(ROW_HEADER_WIDTH, colHb); ctx.lineTo(vw, colHb); ctx.stroke()

    // ── Row headers ──
    const drawRowHeaders = (r0: number, rN: number, offY: number, hy0: number, hy1: number) => {
      if (r0 > rN || hy1 <= hy0) return
      const sy = (cy: number) => cy - offY
      ctx.save(); ctx.beginPath(); ctx.rect(0, hy0, ROW_HEADER_WIDTH, hy1 - hy0); ctx.clip()
      ctx.font = '12px Arial, sans-serif'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
      for (let r = r0; r <= rN; r++) {
        const h = rowTop[r + 1] - rowTop[r]; if (h === 0) continue
        const y = sy(rowTop[r])
        ctx.fillStyle = isRowHighlighted(r) ? '#d3e3fd' : '#f8f9fa'
        ctx.fillRect(0, y, ROW_HEADER_WIDTH, h)
        ctx.fillStyle = '#444'; ctx.fillText(String(r), ROW_HEADER_WIDTH / 2, y + h / 2 + 1)
      }
      ctx.strokeStyle = '#e2e4e6'; ctx.lineWidth = 1; ctx.beginPath()
      for (let r = r0; r <= rN; r++) { if (rowTop[r + 1] === rowTop[r]) continue; const yb = Math.round(sy(rowTop[r + 1])) + 0.5; ctx.moveTo(0, yb); ctx.lineTo(ROW_HEADER_WIDTH, yb) }
      ctx.stroke()
      // Hidden-row indicator: a thick dark tick where rows are collapsed (height 0).
      ctx.strokeStyle = '#5f6368'; ctx.lineWidth = 2; ctx.beginPath()
      for (let r = Math.max(r0, 2); r <= rN; r++) {
        if (rowTop[r + 1] > rowTop[r] && rowTop[r] === rowTop[r - 1]) { const ym = Math.round(sy(rowTop[r])); ctx.moveTo(3, ym); ctx.lineTo(ROW_HEADER_WIDTH - 3, ym) }
      }
      ctx.stroke(); ctx.lineWidth = 1; ctx.restore()
    }
    drawRowHeaders(sr0, srN, st, freezeY, vh)
    if (fRows > 0) drawRowHeaders(1, fRows, 0, COL_HEADER_HEIGHT, freezeY)
    ctx.strokeStyle = '#c1c7cd'; ctx.lineWidth = 1; ctx.beginPath()
    const rowVb = Math.round(ROW_HEADER_WIDTH) - 0.5; ctx.moveTo(rowVb, COL_HEADER_HEIGHT); ctx.lineTo(rowVb, vh); ctx.stroke()

    // ── Corner ──
    ctx.fillStyle = '#f8f9fa'; ctx.fillRect(0, 0, ROW_HEADER_WIDTH, COL_HEADER_HEIGHT)
    ctx.strokeStyle = '#c1c7cd'; ctx.lineWidth = 1; ctx.beginPath()
    ctx.moveTo(Math.round(ROW_HEADER_WIDTH) - 0.5, 0); ctx.lineTo(Math.round(ROW_HEADER_WIDTH) - 0.5, COL_HEADER_HEIGHT)
    ctx.moveTo(0, Math.round(COL_HEADER_HEIGHT) - 0.5); ctx.lineTo(ROW_HEADER_WIDTH, Math.round(COL_HEADER_HEIGHT) - 0.5)
    ctx.stroke()
  }, [geom, sheetData, spill, mergeInfo, cfOverrides, showGridlines, zoom, selectedCell, rangeEnd, fillTo, i18n.language, frozenRows, frozenCols, filterMode, colFilters, sheetImages, imagesTick, selectedImage, cropMode, imageRect]) // eslint-disable-line react-hooks/exhaustive-deps

  // Paint the selection chrome on the transparent TOP overlay canvas: a translucent range tint
  // (so any object underneath stays visible), a slightly stronger anchor-cell tint, the 2px
  // active-cell box, the animated multi-cell range border, and the spill outline. Clipped to the
  // grid body so it never covers the sticky headers.
  const drawSelection = useCallback(() => {
    const canvas = selCanvasRef.current, gridEl = gridRef.current
    if (!canvas || !gridEl) return
    const vw = gridEl.clientWidth, vh = gridEl.clientHeight
    if (vw === 0 || vh === 0) return
    const dpr = window.devicePixelRatio || 1
    if (canvas.width !== Math.round(vw * dpr) || canvas.height !== Math.round(vh * dpr)) {
      canvas.width = Math.round(vw * dpr); canvas.height = Math.round(vh * dpr)
      canvas.style.width = `${vw}px`; canvas.style.height = `${vh}px`
    }
    const ctx = canvas.getContext('2d'); if (!ctx) return
    const sl = gridEl.scrollLeft, st = gridEl.scrollTop
    // Absolute (not sticky): keep it pinned to the viewport top-left via a transform.
    canvas.style.transform = `translate(${sl}px, ${st}px)`
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, vw, vh)
    if (!selectedCell) return
    const ac = COLS.indexOf(selectedCell.col), ar = selectedCell.row
    if (ac < 0 || ar < 1 || ar > MAX_ROWS) return
    const { colLeft, rowTop } = geom
    const fCols = Math.min(frozenCols, MAX_COLS), fRows = Math.min(frozenRows, MAX_ROWS)
    ctx.save()
    ctx.beginPath(); ctx.rect(ROW_HEADER_WIDTH, COL_HEADER_HEIGHT, vw - ROW_HEADER_WIDTH, vh - COL_HEADER_HEIGHT); ctx.clip()
    // Snap a logical coordinate so a stroke of logical width `lw` lands on WHOLE physical
    // pixels → crisp selection outlines without the blurry antialiased look (dpr-aware).
    const snap = (v: number, lw: number) => (Math.round(v * dpr) + (Math.round(lw * dpr) % 2 ? 0.5 : 0)) / dpr
    const crispRect = (x0: number, y0: number, x1: number, y1: number, lw: number) => {
      const l = snap(x0, lw), tp = snap(y0, lw), r = snap(x1, lw), b = snap(y1, lw)
      ctx.lineWidth = lw; ctx.strokeRect(l, tp, r - l, b - tp)
    }
    // Bounds shared by the range fill AND border so they always move together: the ANIMATED
    // rect while a tween is in flight, else the live target (recomputed from the current geom,
    // so they stay glued together through a zoom too).
    const arect = (rangeAnimRaf.current != null ? animRangeRef.current : null) ?? selRangeTargetRef.current()
    if (arect) {
      const x = arect.x - (arect.fx ? 0 : sl), y = arect.y - (arect.fy ? 0 : st)
      ctx.fillStyle = 'rgba(26,115,232,0.12)'
      ctx.fillRect(x, y, arect.w, arect.h)
    }
    // Active cell: a touch more tint + the 2px box (merge-aware).
    const mr = mergeInfo.anchors.get(`${selectedCell.col}${ar}`)
    const ac2 = mr ? mr.c2 : ac, ar2 = mr ? mr.r2 : ar
    if (rowTop[ar2 + 1] > rowTop[ar]) {
      const x = colLeft[ac] - (ac < fCols ? 0 : sl), y = rowTop[ar] - (ar <= fRows ? 0 : st)
      const w = colLeft[ac2 + 1] - colLeft[ac], h = rowTop[ar2 + 1] - rowTop[ar]
      ctx.fillStyle = 'rgba(26,115,232,0.10)'; ctx.fillRect(x, y, w, h)
      ctx.strokeStyle = '#1a73e8'; crispRect(x + 1, y + 1, x + w - 1, y + h - 1, 2)
    }
    // Animated outer border of a multi-cell selection — THINNER (1px) than the active-cell
    // box (2px) so the active cell reads as the stronger of the two outlines.
    if (arect && rangeEnd && (rangeEnd.col !== selectedCell.col || rangeEnd.row !== selectedCell.row)) {
      const x = arect.x - (arect.fx ? 0 : sl), y = arect.y - (arect.fy ? 0 : st)
      ctx.strokeStyle = '#1a73e8'; crispRect(x, y, x + arect.w, y + arect.h, 1)
    }
    // Fill handle at the selection's bottom-right corner — drawn here (overlay) at the SAME
    // animated bounds as the fill/border so it stays glued to them during the slide.
    if (arect && !editingCell && !isFilling.current) {
      const hx = Math.round(arect.x + arect.w - (arect.fx ? 0 : sl)), hy = Math.round(arect.y + arect.h - (arect.fy ? 0 : st))
      ctx.fillStyle = '#fff'; ctx.fillRect(hx - 4, hy - 4, 8, 8)
      ctx.fillStyle = '#1a73e8'; ctx.fillRect(hx - 3, hy - 3, 6, 6)
    }
    // Spilled-range outline (dashed) when the active cell is inside a dynamic-array spill.
    const anchorKey = spill.origin[`${selectedCell.col}${ar}`]
    const sa = anchorKey ? spill.anchors[anchorKey] : undefined
    const sm = anchorKey ? /^([A-Z]+)([0-9]+)$/.exec(anchorKey) : null
    if (sa && !sa.blocked && sm && sa.rows * sa.cols > 1) {
      const sc = colToIndex(sm[1]), srr = +sm[2]
      const cc2 = Math.min(sc + sa.cols - 1, MAX_COLS - 1), rr2 = Math.min(srr + sa.rows - 1, MAX_ROWS)
      const x = colLeft[sc] - (sc < fCols ? 0 : sl), y = rowTop[srr] - (srr <= fRows ? 0 : st)
      ctx.strokeStyle = '#1a73e8'; ctx.setLineDash([3, 2])
      crispRect(x, y, colLeft[cc2 + 1] - (sc < fCols ? 0 : sl), rowTop[rr2 + 1] - (srr <= fRows ? 0 : st), 1)
      ctx.setLineDash([])
    }
    // Drop indicator for a column/row drag-move: a thick line at the insertion boundary.
    if (headerMove.current?.moved && headerMoveTarget.current != null) {
      const before = headerMoveTarget.current
      ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 3; ctx.beginPath()
      if (headerMove.current.axis === 'col') {
        const x = snap(colLeft[Math.min(before, MAX_COLS)] - (before < fCols ? 0 : sl), 3)
        ctx.moveTo(x, COL_HEADER_HEIGHT); ctx.lineTo(x, vh)
      } else {
        const y = snap(rowTop[Math.min(before, MAX_ROWS)] - (before <= fRows ? 0 : st), 3)
        ctx.moveTo(ROW_HEADER_WIDTH, y); ctx.lineTo(vw, y)
      }
      ctx.stroke()
    }
    ctx.restore()
  }, [geom, selectedCell, rangeEnd, mergeInfo, spill, frozenCols, frozenRows, editingCell])
  const drawSelRef = useRef(drawSelection); drawSelRef.current = drawSelection
  useEffect(() => { drawSelection() }, [drawSelection])

  // Grow / shrink the MULTI-cell selection border to its new bounds PROGRESSIVELY — every
  // bound change eases over ~110ms (easeOutCubic), including a live drag-select (each mousemove
  // restarts the tween from the current animated bounds, so the border elastically follows the
  // cursor) and reductions. Snap only on first show, a single-cell selection (no border), or no
  // real change.
  useEffect(() => {
    const t = selRangeTargetRef.current()
    const key = t ? `${Math.round(t.x)},${Math.round(t.y)},${Math.round(t.w)},${Math.round(t.h)}` : null
    // Re-run guard: a click fires mousedown AND click (same bounds) and a geom change does not
    // re-fire this effect; NOT cancelling on a no-op lets an in-flight slide finish.
    if (key === lastRangeKey.current) return
    lastRangeKey.current = key
    if (!t) { animRangeRef.current = null; if (rangeAnimRaf.current) { cancelAnimationFrame(rangeAnimRaf.current); rangeAnimRaf.current = null } return }
    const from = animRangeRef.current
    if (!from || !t.multi || (from.x === t.x && from.y === t.y && from.w === t.w && from.h === t.h)) {
      animRangeRef.current = t; return
    }
    if (rangeAnimRaf.current) cancelAnimationFrame(rangeAnimRaf.current)
    const f = from, start = performance.now(), dur = 110
    const ease = (x: number) => 1 - Math.pow(1 - x, 3)
    const step = (now: number) => {
      const p = Math.min(1, (now - start) / dur), e = ease(p)
      animRangeRef.current = { x: f.x + (t.x - f.x) * e, y: f.y + (t.y - f.y) * e, w: f.w + (t.w - f.w) * e, h: f.h + (t.h - f.h) * e, fx: t.fx, fy: t.fy, multi: t.multi }
      drawSelRef.current()
      if (p < 1) rangeAnimRaf.current = requestAnimationFrame(step)
      else { animRangeRef.current = t; rangeAnimRaf.current = null; drawSelRef.current() }
    }
    rangeAnimRaf.current = requestAnimationFrame(step)
  }, [selectedCell, rangeEnd])
  useEffect(() => () => { if (rangeAnimRaf.current) cancelAnimationFrame(rangeAnimRaf.current) }, [])

  // Redraw on every dependency change, after layout (fonts loading included).
  useEffect(() => { drawGrid() }, [drawGrid])
  useEffect(() => {
    if (!document.fonts) return
    document.fonts.ready.then(() => drawGrid()).catch(() => {})
  }, [drawGrid])
  // Redraw when the viewport itself resizes.
  useEffect(() => {
    const el = gridRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => { drawGrid(); drawSelection() })
    ro.observe(el)
    return () => ro.disconnect()
  }, [drawGrid, drawSelection])

  // ── Canvas pointer hit-testing ────────────────────────────────────────────────
  const RESIZE_GRAB = 4
  // The canvas grid receives mousedown AND mouseup on the same element, so the
  // browser fires a trailing `click` after a drag — which would collapse a range
  // selection (or a fill) back to a single cell. `didDrag` suppresses that click.
  const didDrag = useRef(false)
  // Pointer position in both screen (viewport) and content (scrolled) coordinates.
  // Frozen rows/cols are not scrolled, so a pointer inside a frozen band maps to
  // content coords without the scroll offset.
  // True when the pointer is over the container's native scrollbar gutter (right /
  // bottom). clientWidth/clientHeight EXCLUDE the scrollbar, so a position past them
  // sits on the scrollbar — a click there must NOT fall through to a cell underneath.
  const isOnScrollbar = (e: React.MouseEvent) => {
    const el = gridRef.current; if (!el) return false
    const rect = el.getBoundingClientRect()
    return (e.clientX - rect.left) >= el.clientWidth || (e.clientY - rect.top) >= el.clientHeight
  }
  const pointerPos = (e: React.MouseEvent) => {
    const el = gridRef.current
    if (!el) return null
    const rect = el.getBoundingClientRect()
    const screenX = e.clientX - rect.left, screenY = e.clientY - rect.top
    const freezeX = geom.colLeft[Math.min(frozenCols, MAX_COLS)]
    const freezeY = geom.rowTop[Math.min(frozenRows, MAX_ROWS) + 1]
    return {
      screenX, screenY,
      contentX: screenX < freezeX ? screenX : screenX + el.scrollLeft,
      contentY: screenY < freezeY ? screenY : screenY + el.scrollTop,
    }
  }
  // Column whose right edge is within grab distance of content-x (resize), else -1.
  // Localisé autour de la colonne sous le curseur (pas de balayage des 16 384 colonnes).
  const colEdgeAt = (x: number) => {
    const { colLeft } = geom
    const c0 = colAtX(x)
    if (c0 < 0) return -1
    for (const c of [c0 - 1, c0, c0 + 1]) if (c >= 0 && c < MAX_COLS && Math.abs(colLeft[c + 1] - x) <= RESIZE_GRAB) return c
    return -1
  }
  // Localisé autour de la ligne sous le curseur (pas de balayage des 1 048 576 lignes).
  const rowEdgeAt = (y: number) => {
    const r0 = rowAtY(y)
    if (r0 < 0) return -1
    for (const r of [r0 - 1, r0, r0 + 1]) if (r >= 1 && r <= MAX_ROWS && Math.abs(geom.rowTop[r + 1] - y) <= RESIZE_GRAB) return r
    return -1
  }
  // Column whose header funnel button is under content-x (when visible), else -1.
  const funnelAt = (x: number) => {
    const c = colAtX(x)
    if (c < 0 || !(filterMode || colFilters[c])) return -1
    return Math.abs(x - (geom.colLeft[c + 1] - 13)) <= 8 ? c : -1
  }
  const openFilterPopup = (col: number, e: React.MouseEvent) => {
    const rect = gridRef.current?.getBoundingClientRect()
    setFilterPopup({ col, x: e.clientX, y: (rect?.top ?? 0) + COL_HEADER_HEIGHT })
  }

  // ── Embedded picture manipulation (select / move / resize / rotate) ───────────
  const selectedImageRef = useRef<number | null>(null); selectedImageRef.current = selectedImage
  const imgDragRef = useRef<{ idx: number; handle: string; sx: number; sy: number; box: { bx: number; by: number; bw: number; bh: number }; rot: number; z: number } | null>(null)
  // Set when a mousedown selects/drags a picture, so the trailing `click` event does
  // not also select the cell underneath (pictures float above the grid).
  const imgClickGuard = useRef(false)
  // Map a native event's client coords to grid content coords (scroll-aware).
  const contentFromClient = (clientX: number, clientY: number) => {
    const el = gridRef.current; if (!el) return null
    const rect = el.getBoundingClientRect()
    const sx = clientX - rect.left, sy = clientY - rect.top
    const fX = geom.colLeft[Math.min(frozenCols, MAX_COLS)], fY = geom.rowTop[Math.min(frozenRows, MAX_ROWS) + 1]
    return { x: sx < fX ? sx : sx + el.scrollLeft, y: sy < fY ? sy : sy + el.scrollTop }
  }
  // Topmost picture (+ which handle) under a content-space point. Resize/rotate
  // handles are only live for the currently-selected picture; others hit only body.
  const imageHitTest = (px: number, py: number): { idx: number; handle: string } | null => {
    const imgs = localImagesRef.current
    for (let idx = imgs.length - 1; idx >= 0; idx--) {
      const r = imageRectRef.current(imgs[idx])
      if (r.w <= 0 || r.h <= 0) continue
      const cx = r.x + r.w / 2, cy = r.y + r.h / 2
      const a = -(r.rot || 0) * Math.PI / 180
      const ddx = px - cx, ddy = py - cy
      const lx = cx + ddx * Math.cos(a) - ddy * Math.sin(a)
      const ly = cy + ddx * Math.sin(a) + ddy * Math.cos(a)
      const T = 7
      if (idx === selectedImageRef.current) {
        if (Math.abs(lx - cx) <= T && Math.abs(ly - (r.y - 22)) <= T) return { idx, handle: 'rotate' }
        const handles: [string, number, number][] = [
          ['nw', r.x, r.y], ['n', cx, r.y], ['ne', r.x + r.w, r.y],
          ['w', r.x, cy], ['e', r.x + r.w, cy],
          ['sw', r.x, r.y + r.h], ['s', cx, r.y + r.h], ['se', r.x + r.w, r.y + r.h],
        ]
        for (const [hn, hx, hy] of handles) if (Math.abs(lx - hx) <= T && Math.abs(ly - hy) <= T) return { idx, handle: hn }
      }
      if (lx >= r.x && lx <= r.x + r.w && ly >= r.y && ly <= r.y + r.h) return { idx, handle: 'move' }
    }
    return null
  }
  const patchImage = (idx: number, patch: Partial<SheetImage>) =>
    setLocalImages(prev => prev.map((im, i) => i === idx ? { ...im, ...patch } : im))
  const startImageDrag = (hit: { idx: number; handle: string }, e: React.MouseEvent) => {
    e.preventDefault()
    imgClickGuard.current = true
    setSelectedImage(hit.idx)
    // Selecting a picture clears any cell selection/edit so no cell stays lit underneath.
    setSelectedCell(null); setRangeEnd(null); setEditingCell(null)
    const im = localImagesRef.current[hit.idx]
    const r = imageRectRef.current(im)
    const box = (im.bx != null && im.by != null && im.bw != null && im.bh != null)
      ? { bx: im.bx, by: im.by, bw: im.bw, bh: im.bh }
      : rectToBox(r)
    // Bake the full box (+ rotation) so anchor-based pictures become box-based for the
    // whole drag; subsequent partial patches then always keep every field populated.
    patchImage(hit.idx, { ...box, rot: r.rot })
    const p = contentFromClient(e.clientX, e.clientY); if (!p) return
    imgDragRef.current = { idx: hit.idx, handle: hit.handle, sx: p.x, sy: p.y, box, rot: r.rot, z: zoom }
    const onMove = (ev: MouseEvent) => {
      const d = imgDragRef.current; if (!d) return
      const cp = contentFromClient(ev.clientX, ev.clientY); if (!cp) return
      const z = d.z, MIN = 8
      const wdx = cp.x - d.sx, wdy = cp.y - d.sy
      if (d.handle === 'rotate') {
        const cx = ROW_HEADER_WIDTH + (d.box.bx + d.box.bw / 2) * z
        const cy = COL_HEADER_HEIGHT + (d.box.by + d.box.bh / 2) * z
        patchImage(d.idx, { rot: Math.round(Math.atan2(cp.y - cy, cp.x - cx) * 180 / Math.PI + 90) })
        return
      }
      if (d.handle === 'move') { patchImage(d.idx, { bx: d.box.bx + wdx / z, by: d.box.by + wdy / z, bw: d.box.bw, bh: d.box.bh }); return }
      // Resize: operate along the picture's local axes (exact for unrotated).
      const a = -(d.rot || 0) * Math.PI / 180
      const ldx = wdx * Math.cos(a) - wdy * Math.sin(a), ldy = wdx * Math.sin(a) + wdy * Math.cos(a)
      let { bx, by, bw, bh } = d.box
      if (d.handle.includes('e')) bw = Math.max(MIN, d.box.bw + ldx / z)
      if (d.handle.includes('s')) bh = Math.max(MIN, d.box.bh + ldy / z)
      if (d.handle.includes('w')) { const n = Math.max(MIN, d.box.bw - ldx / z); bx = d.box.bx + (d.box.bw - n); bw = n }
      if (d.handle.includes('n')) { const n = Math.max(MIN, d.box.bh - ldy / z); by = d.box.by + (d.box.bh - n); bh = n }
      patchImage(d.idx, { bx, by, bw, bh })
    }
    const onUp = () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      imgDragRef.current = null
      saveImagesMut.mutate(localImagesRef.current)
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  // ── Interactive cropping (a:srcRect) — full image shown, drag a crop frame ─────
  const clampN = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
  // The full (uncropped) image extent in content coords, given the kept-region rect.
  const cropFullExtent = (im: SheetImage, r: { x: number; y: number; w: number; h: number }) => {
    const cl = im.cropL ?? 0, ct = im.cropT ?? 0, cr = im.cropR ?? 0, cb = im.cropB ?? 0
    const kw = Math.max(0.02, 1 - cl - cr), kh = Math.max(0.02, 1 - ct - cb)
    const fw = r.w / kw, fh = r.h / kh
    return { x: r.x - cl * fw, y: r.y - ct * fh, w: fw, h: fh }
  }
  // Which crop-frame handle (or 'move') is under a content point, while cropping.
  const cropHitTest = (px: number, py: number): string | null => {
    const idx = cropModeRef.current; if (idx == null) return null
    const im = localImagesRef.current[idx]; if (!im) return null
    const r = imageRectRef.current(im)
    const cx = r.x + r.w / 2, cy = r.y + r.h / 2, T = 9
    const handles: [string, number, number][] = [
      ['nw', r.x, r.y], ['n', cx, r.y], ['ne', r.x + r.w, r.y], ['w', r.x, cy], ['e', r.x + r.w, cy],
      ['sw', r.x, r.y + r.h], ['s', cx, r.y + r.h], ['se', r.x + r.w, r.y + r.h],
    ]
    for (const [hn, hx, hy] of handles) if (Math.abs(px - hx) <= T && Math.abs(py - hy) <= T) return hn
    if (px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h) return 'move'
    return null
  }
  const startCropDrag = (handle: string, e: React.MouseEvent) => {
    e.preventDefault()
    imgClickGuard.current = true
    const idx = cropModeRef.current; if (idx == null) return
    const im = localImagesRef.current[idx]
    const r0 = imageRectRef.current(im)
    const full = cropFullExtent(im, r0)
    const frame0 = { x: r0.x, y: r0.y, w: r0.w, h: r0.h }
    const p = contentFromClient(e.clientX, e.clientY); if (!p) return
    const onMove = (ev: MouseEvent) => {
      const cp = contentFromClient(ev.clientX, ev.clientY); if (!cp) return
      const dx = cp.x - p.x, dy = cp.y - p.y, MIN = 10
      let { x, y, w, h } = frame0
      if (handle === 'move') {
        x = clampN(frame0.x + dx, full.x, full.x + full.w - w)
        y = clampN(frame0.y + dy, full.y, full.y + full.h - h)
      } else {
        if (handle.includes('e')) w = clampN(frame0.w + dx, MIN, full.x + full.w - x)
        if (handle.includes('s')) h = clampN(frame0.h + dy, MIN, full.y + full.h - y)
        if (handle.includes('w')) { const nx = clampN(frame0.x + dx, full.x, frame0.x + frame0.w - MIN); w = frame0.w + (frame0.x - nx); x = nx }
        if (handle.includes('n')) { const ny = clampN(frame0.y + dy, full.y, frame0.y + frame0.h - MIN); h = frame0.h + (frame0.y - ny); y = ny }
      }
      const cl = (x - full.x) / full.w, ct = (y - full.y) / full.h
      const cr = (full.x + full.w - (x + w)) / full.w, cb = (full.y + full.h - (y + h)) / full.h
      patchImage(idx, { ...rectToBox({ x, y, w, h }), cropL: cl, cropT: ct, cropR: cr, cropB: cb })
    }
    const onUp = () => {
      window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp)
      saveImagesMut.mutate(localImagesRef.current)
    }
    window.addEventListener('mousemove', onMove); window.addEventListener('mouseup', onUp)
  }

  // ── Picture actions (ribbon) ──────────────────────────────────────────────────
  // Apply a transform to the selected picture, baking the anchor into an explicit box
  // first (so rotation/order stay stable), then persist.
  const mutateSelectedImage = (fn: (im: SheetImage) => SheetImage) => {
    const idx = selectedImage; if (idx == null) return
    setLocalImages(prev => {
      const next = prev.map((im, i) => {
        if (i !== idx) return im
        const base = im.bx != null ? im : { ...im, ...rectToBox(imageRect(im)), rot: im.rot ?? 0 }
        return fn(base)
      })
      saveImagesMut.mutate(next)
      return next
    })
  }
  const rotateSelectedImage = (deg: number) => mutateSelectedImage(im => ({ ...im, rot: Math.round(((im.rot ?? 0) + deg) % 360) }))
  // Drop every manipulation override → back to the original imported cell anchor.
  const resetSelectedImage = () => {
    const idx = selectedImage; if (idx == null) return
    setLocalImages(prev => {
      const next = prev.map((im, i) => {
        if (i !== idx) return im
        const { bx, by, bw, bh, rot, cropL, cropT, cropR, cropB, ...rest } = im // eslint-disable-line @typescript-eslint/no-unused-vars
        return rest
      })
      saveImagesMut.mutate(next); return next
    })
    setCropMode(null)
  }
  // Z-order: pictures paint in array order, so moving an item to the end brings it to
  // front; to the start sends it to back. Keep `selectedImage` pointing at it.
  const reorderSelectedImage = (toFront: boolean) => {
    const idx = selectedImage; if (idx == null) return
    setLocalImages(prev => {
      if (idx < 0 || idx >= prev.length) return prev
      const next = prev.slice(); const [im] = next.splice(idx, 1)
      if (toFront) next.push(im); else next.unshift(im)
      saveImagesMut.mutate(next); return next
    })
    setSelectedImage(toFront ? localImagesRef.current.length - 1 : 0)
  }
  const deleteSelectedImage = () => {
    const idx = selectedImage; if (idx == null) return
    setLocalImages(prev => { const next = prev.filter((_, i) => i !== idx); saveImagesMut.mutate(next); return next })
    setSelectedImage(null); setCropMode(null)
  }

  const handleGridMouseDown = (e: React.MouseEvent) => {
    if (isOnScrollbar(e)) return // click on the native scrollbar, not a cell
    const p = pointerPos(e); if (!p) return
    const inColHdr = p.screenY < COL_HEADER_HEIGHT, inRowHdr = p.screenX < ROW_HEADER_WIDTH
    if (inColHdr && !inRowHdr) {
      const f = funnelAt(p.contentX); if (f >= 0) { e.preventDefault(); openFilterPopup(f, e); return }
      const c = colEdgeAt(p.contentX); if (c >= 0) { startColResize(COLS[c], e); return }
      // Click on a column header → select the entire column (drag extends across columns).
      const cc = colAtX(p.contentX)
      if (cc >= 0) {
        e.preventDefault()
        setEditingCell(null)
        // If it is already part of a full-column selection → start a drag-to-MOVE (a plain
        // click without movement still falls back to re-selecting this single column).
        const fullCol = selectedCell?.row === 1 && rangeEnd?.row === MAX_ROWS
        const lo = Math.min(colToIndex(selectedCell?.col ?? 'A'), colToIndex(rangeEnd?.col ?? 'A'))
        const hi = Math.max(colToIndex(selectedCell?.col ?? 'A'), colToIndex(rangeEnd?.col ?? 'A'))
        if (fullCol && cc >= lo && cc <= hi) {
          headerMove.current = { axis: 'col', lo, hi, moved: false }
          headerMoveTarget.current = null
        } else {
          setSelectedCell({ col: COLS[cc], row: 1 }); setRangeEnd({ col: COLS[cc], row: MAX_ROWS })
          headerSelect.current = { kind: 'col', anchor: cc }
          isDragSelecting.current = true
        }
      }
      return
    }
    if (inRowHdr && !inColHdr) {
      const r = rowEdgeAt(p.contentY); if (r >= 1) { startRowResize(r, e); return }
      // Click on a row header → select the entire row (drag extends across rows).
      const rr = rowAtY(p.contentY)
      if (rr >= 1) {
        e.preventDefault()
        setEditingCell(null)
        const fullRow = selectedCell?.col === COLS[0] && rangeEnd?.col === COLS[MAX_COLS - 1]
        const lo = Math.min(selectedCell?.row ?? 1, rangeEnd?.row ?? 1)
        const hi = Math.max(selectedCell?.row ?? 1, rangeEnd?.row ?? 1)
        if (fullRow && rr >= lo && rr <= hi) {
          headerMove.current = { axis: 'row', lo, hi, moved: false }
          headerMoveTarget.current = null
        } else {
          setSelectedCell({ col: COLS[0], row: rr }); setRangeEnd({ col: COLS[MAX_COLS - 1], row: rr })
          headerSelect.current = { kind: 'row', anchor: rr }
          isDragSelecting.current = true
        }
      }
      return
    }
    if (inColHdr || inRowHdr) { headerSelect.current = null; return } // corner
    // Crop mode: drag the crop frame/handles; a click outside applies & exits.
    if (cropModeRef.current !== null) {
      const ch = cropHitTest(p.contentX, p.contentY)
      if (ch) { startCropDrag(ch, e); return }
      setCropMode(null)
    }
    // Fill handle: a mousedown within a few px of the selection's bottom-right corner
    // starts a fill drag (the handle is canvas-drawn, so we hit-test it here).
    if (!editingCell && selectedCell && e.button === 0) {
      const tg = selRangeTarget()
      if (tg && Math.abs(p.contentX - (tg.x + tg.w)) <= 6 && Math.abs(p.contentY - (tg.y + tg.h)) <= 6) {
        e.preventDefault(); didDrag.current = false; startFill(); return
      }
    }
    // Embedded picture: clicking one (or a handle of the selected one) starts a
    // move/resize/rotate drag instead of a cell selection.
    const imgHit = imageHitTest(p.contentX, p.contentY)
    if (imgHit) { startImageDrag(imgHit, e); return }
    if (selectedImageRef.current !== null) setSelectedImage(null)
    setSelectedEq(null); setSelectedChart(null)
    const c = colAtX(p.contentX), r = rowAtY(p.contentY)
    if (c >= 0 && r >= 1) { didDrag.current = false; handleCellMouseDown(COLS[c], r, e) }
  }

  // Extend the in-progress drag (header / cell range / fill) to a content-space point.
  // Shared by the live mousemove and the edge auto-scroll loop.
  const extendDragTo = (contentX: number, contentY: number) => {
    // Drag-to-move a column/row block: track the insertion boundary + mark "moved" once the
    // pointer leaves the original block.
    if (headerMove.current) {
      const hm = headerMove.current
      if (hm.axis === 'col') {
        const c = colAtX(Math.max(contentX, ROW_HEADER_WIDTH + 1)); if (c < 0) return
        const before = contentX > (geom.colLeft[c] + geom.colLeft[c + 1]) / 2 ? c + 1 : c
        if (before < hm.lo || before > hm.hi + 1) hm.moved = true
        headerMoveTarget.current = before
      } else {
        const r = rowAtY(Math.max(contentY, COL_HEADER_HEIGHT + 1)); if (r < 1) return
        const before = contentY > (geom.rowTop[r] + geom.rowTop[r + 1]) / 2 ? r + 1 : r
        if (before < hm.lo || before > hm.hi + 1) hm.moved = true
        headerMoveTarget.current = before
      }
      didDrag.current = true
      drawSelRef.current()
      return
    }
    if (headerSelect.current) {
      const hs = headerSelect.current
      if (hs.kind === 'col') { const c = colAtX(Math.max(contentX, ROW_HEADER_WIDTH + 1)); if (c >= 0) { didDrag.current = true; setRangeEnd({ col: COLS[c], row: MAX_ROWS }) } }
      else { const r = rowAtY(Math.max(contentY, COL_HEADER_HEIGHT + 1)); if (r >= 1) { didDrag.current = true; setRangeEnd({ col: COLS[MAX_COLS - 1], row: r }) } }
      return
    }
    if (isDragSelecting.current || isFilling.current) {
      const c = colAtX(contentX), r = rowAtY(contentY)
      if (c >= 0 && r >= 1) {
        if (selectedCell && (COLS[c] !== selectedCell.col || r !== selectedCell.row)) didDrag.current = true
        if (isFilling.current) didDrag.current = true
        handleCellMouseEnter(COLS[c], r)
      }
    }
  }

  // Edge auto-scroll while drag-selecting: when the cursor passes a body edge, scroll the
  // grid and keep extending the selection to the clamped edge cell, so the selection can grow
  // (or shrink) past the visible area. Driven by rAF + a window-level pointer tracker (the
  // grid's own mousemove stops firing once the cursor leaves it).
  const dragPointer = useRef<{ x: number; y: number } | null>(null)
  const autoScrollRaf = useRef<number | null>(null)
  const autoScrollFrame = () => {
    const el = gridRef.current, p = dragPointer.current
    const dragging = isDragSelecting.current || isFilling.current || !!headerSelect.current
    if (!el || !p || !dragging) { autoScrollRaf.current = null; return }
    const rect = el.getBoundingClientRect()
    const bodyL = rect.left + ROW_HEADER_WIDTH, bodyT = rect.top + COL_HEADER_HEIGHT
    const bodyR = rect.left + el.clientWidth, bodyB = rect.top + el.clientHeight
    const v = (over: number) => Math.sign(over) * Math.min(40, 6 + Math.abs(over) * 0.35)
    let vx = 0, vy = 0
    if (p.x > bodyR) vx = v(p.x - bodyR); else if (p.x < bodyL) vx = v(p.x - bodyL)
    if (p.y > bodyB) vy = v(p.y - bodyB); else if (p.y < bodyT) vy = v(p.y - bodyT)
    if (vx || vy) {
      el.scrollLeft += vx; el.scrollTop += vy
      const cx = Math.max(bodyL + 1, Math.min(p.x, bodyR - 1)), cy = Math.max(bodyT + 1, Math.min(p.y, bodyB - 1))
      const cf = contentFromClient(cx, cy)
      if (cf) extendDragTo(cf.x, cf.y)
      trackViewEnd(el); drawGrid(); drawSelection()
    }
    autoScrollRaf.current = requestAnimationFrame(autoScrollFrameRef.current)
  }
  const autoScrollFrameRef = useRef(autoScrollFrame); autoScrollFrameRef.current = autoScrollFrame
  useEffect(() => {
    const onMove = (ev: MouseEvent) => {
      if (!(isDragSelecting.current || isFilling.current || headerSelect.current)) return
      dragPointer.current = { x: ev.clientX, y: ev.clientY }
      if (autoScrollRaf.current == null) autoScrollRaf.current = requestAnimationFrame(autoScrollFrameRef.current)
    }
    window.addEventListener('mousemove', onMove)
    return () => window.removeEventListener('mousemove', onMove)
  }, [])

  const handleGridMouseMove = (e: React.MouseEvent) => {
    onGridCursor(e) // presence cursor
    const p = pointerPos(e); if (!p) return
    // Resize cursor affordance over header edges.
    let edge: 'col' | 'row' | null = null
    if (p.screenY < COL_HEADER_HEIGHT && p.screenX >= ROW_HEADER_WIDTH && colEdgeAt(p.contentX) >= 0) edge = 'col'
    else if (p.screenX < ROW_HEADER_WIDTH && p.screenY >= COL_HEADER_HEIGHT && rowEdgeAt(p.contentY) >= 1) edge = 'row'
    if (edge !== hoverEdge) setHoverEdge(edge)
    if (headerMove.current || headerSelect.current) { extendDragTo(p.contentX, p.contentY); return }
    if (isDragSelecting.current || isFilling.current) extendDragTo(p.contentX, p.contentY)
  }

  const handleGridClick = (e: React.MouseEvent) => {
    if (imgClickGuard.current) { imgClickGuard.current = false; return } // click consumed by a picture
    if (didDrag.current) { didDrag.current = false; return } // trailing click after a drag
    if (isOnScrollbar(e)) return
    const p = pointerPos(e); if (!p) return
    if (p.screenX < ROW_HEADER_WIDTH || p.screenY < COL_HEADER_HEIGHT) return
    const c = colAtX(p.contentX), r = rowAtY(p.contentY)
    if (c >= 0 && r >= 1) handleCellClick(COLS[c], r, e)
  }

  const handleGridDoubleClick = (e: React.MouseEvent) => {
    if (isOnScrollbar(e)) return
    const p = pointerPos(e); if (!p) return
    // Double-click a column-header right edge → autofit that column to its content.
    if (p.screenY < COL_HEADER_HEIGHT && p.screenX >= ROW_HEADER_WIDTH) {
      const ce = colEdgeAt(p.contentX); if (ce >= 0) { autofitColumns(ce, ce); e.preventDefault(); return }
    }
    if (p.screenX < ROW_HEADER_WIDTH || p.screenY < COL_HEADER_HEIGHT) return
    // Double-clicking a picture enters interactive crop mode.
    const imgHit = imageHitTest(p.contentX, p.contentY)
    if (imgHit) { setSelectedImage(imgHit.idx); setCropMode(imgHit.idx); return }
    const c = colAtX(p.contentX), r = rowAtY(p.contentY)
    if (c >= 0 && r >= 1) handleCellDoubleClick(COLS[c], r)
  }

  // ── Menu contextuel des cellules (clic droit) ────────────────────────────────
  // `stopPropagation` court-circuite le ContextMenuProvider du core (qui sinon
  // supprime le menu natif puis affiche un menu vide car le drive est actif).
  const [cellMenu, setCellMenu] = useState<{ x: number; y: number; kind: 'cell' | 'col' | 'row' | 'image' } | null>(null)
  const onGridContextMenu = (e: React.MouseEvent) => {
    if (isOnScrollbar(e)) return // let the browser handle a right-click on the scrollbar
    e.preventDefault(); e.stopPropagation()
    const p = pointerPos(e)
    if (!p) { setCellMenu({ x: e.clientX, y: e.clientY, kind: 'cell' }); return }
    // Right-click on a picture → object menu (crop / rotate / order / delete).
    const imgHit = imageHitTest(p.contentX, p.contentY)
    if (imgHit) { setSelectedImage(imgHit.idx); setSelectedCell(null); setRangeEnd(null); setCellMenu({ x: e.clientX, y: e.clientY, kind: 'image' }); return }
    const inColHdr = p.screenY < COL_HEADER_HEIGHT, inRowHdr = p.screenX < ROW_HEADER_WIDTH
    if (inColHdr && !inRowHdr) {
      const c = colAtX(p.contentX)
      if (c >= 0 && !isInSelection(COLS[c], 1)) { setSelectedCell({ col: COLS[c], row: 1 }); setRangeEnd({ col: COLS[c], row: MAX_ROWS }) }
      setCellMenu({ x: e.clientX, y: e.clientY, kind: 'col' }); return
    }
    if (inRowHdr && !inColHdr) {
      const r = rowAtY(p.contentY)
      if (r >= 1 && !isInSelection(COLS[0], r)) { setSelectedCell({ col: COLS[0], row: r }); setRangeEnd({ col: COLS[MAX_COLS - 1], row: r }) }
      setCellMenu({ x: e.clientX, y: e.clientY, kind: 'row' }); return
    }
    if (inColHdr || inRowHdr) return // corner
    const c = colAtX(p.contentX), r = rowAtY(p.contentY)
    if (c >= 0 && r >= 1 && !isInSelection(COLS[c], r)) { setSelectedCell({ col: COLS[c], row: r }); setRangeEnd(null) }
    setCellMenu({ x: e.clientX, y: e.clientY, kind: 'cell' })
  }
  const menuSoon = () => void appAlert(t('sheet_coming_soon', { defaultValue: 'Fonctionnalité bientôt disponible.' }))
  // Submenus shared by the cell / column / row context menus.
  const pasteSpecialItems: MenuItem[] = [
    { type: 'action', label: t('sheet_paste_values', { defaultValue: 'Valeurs uniquement' }), shortcut: 'Ctrl+Maj+V', onClick: () => pasteSpecial('values') },
    { type: 'action', label: t('sheet_paste_format', { defaultValue: 'Format uniquement' }), shortcut: 'Ctrl+Alt+V', onClick: () => pasteSpecial('format') },
    { type: 'action', label: t('sheet_paste_formula', { defaultValue: 'Formule uniquement' }), onClick: () => pasteSpecial('formula') },
    { type: 'action', label: t('sheet_paste_cf', { defaultValue: 'Mise en forme conditionnelle uniquement' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_paste_validation', { defaultValue: 'Validation des données uniquement' }), onClick: menuSoon },
    { type: 'separator' },
    { type: 'action', label: t('sheet_paste_transpose', { defaultValue: 'Transposé' }), onClick: () => pasteSpecial('transpose') },
    { type: 'separator' },
    { type: 'action', label: t('sheet_paste_colwidth', { defaultValue: 'Largeur de colonne uniquement' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_paste_noborders', { defaultValue: 'Tout sauf les bordures' }), onClick: () => pasteSpecial('noborders') },
  ]
  const smartChipsItems: MenuItem[] = [
    { type: 'action', label: t('sheet_chip_contacts', { defaultValue: 'Contacts' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_chip_file', { defaultValue: 'Fichier' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_chip_event', { defaultValue: 'Événements du calendrier' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_chip_place', { defaultValue: 'Carte' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_chip_finance', { defaultValue: 'Finance' }), onClick: menuSoon },
  ]
  const selColIdx = colToIndex(selectedCell?.col ?? 'A')
  const cellMenuItems: MenuItem[] = cellMenu?.kind === 'image' ? [
    { type: 'action', label: t('sheet_img_crop', { defaultValue: 'Rogner' }), onClick: () => selectedImage != null && setCropMode(selectedImage) },
    { type: 'action', label: t('sheet_img_rot_right', { defaultValue: 'Pivoter à droite 90°' }), onClick: () => rotateSelectedImage(90) },
    { type: 'action', label: t('sheet_img_rot_left', { defaultValue: 'Pivoter à gauche 90°' }), onClick: () => rotateSelectedImage(-90) },
    { type: 'separator' },
    { type: 'action', label: t('sheet_img_front', { defaultValue: 'Mettre au premier plan' }), onClick: () => reorderSelectedImage(true) },
    { type: 'action', label: t('sheet_img_back', { defaultValue: 'Mettre en arrière-plan' }), onClick: () => reorderSelectedImage(false) },
    { type: 'separator' },
    { type: 'action', label: t('sheet_img_reset', { defaultValue: 'Réinitialiser' }), onClick: () => resetSelectedImage() },
    { type: 'action', label: t('sheet_img_delete', { defaultValue: 'Supprimer l’image' }), danger: true, onClick: () => deleteSelectedImage() },
  ] : cellMenu?.kind === 'col' ? [
    { type: 'action', icon: <Scissors size={15} />, label: t('common_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', onClick: () => copySelection(true) },
    { type: 'action', icon: <Copy size={15} />, label: t('common_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', onClick: () => copySelection(false) },
    { type: 'action', icon: <ClipboardPaste size={15} />, label: t('common_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', onClick: () => pasteSelection() },
    { type: 'submenu', icon: <ClipboardPaste size={15} />, label: t('sheet_paste_special', { defaultValue: 'Effectuer un collage spécial' }), items: pasteSpecialItems },
    { type: 'separator' },
    { type: 'action', icon: <Plus size={15} />, label: t('sheet_insert_col_left', { defaultValue: 'Insérer une colonne à gauche' }), onClick: () => insertColsAction() },
    { type: 'action', icon: <Plus size={15} />, label: t('sheet_insert_col_right', { defaultValue: 'Insérer une colonne à droite' }), onClick: () => { const b = bounds(); if (b) structuralEdit('col', b.c2 + 1, b.c2 - b.c1 + 1) } },
    { type: 'action', icon: <Trash2 size={15} />, label: t('sheet_delete_col', { defaultValue: 'Supprimer la colonne' }), onClick: () => deleteColsAction() },
    { type: 'action', icon: <X size={15} />, label: t('sheet_clear_col', { defaultValue: 'Effacer la colonne' }), onClick: () => clearSelection() },
    { type: 'action', icon: <ImageOff size={15} />, label: t('sheet_hide_col', { defaultValue: 'Masquer la colonne' }), onClick: () => hideCols() },
    { type: 'action', icon: <Columns3 size={15} />, label: t('sheet_resize_col', { defaultValue: 'Redimensionner la colonne' }), onClick: () => promptColWidth() },
    { type: 'separator' },
    { type: 'action', icon: <Filter size={15} />, label: t('sheet_create_filter', { defaultValue: 'Créer un filtre' }), onClick: () => setFilterMode(true) },
    { type: 'separator' },
    { type: 'action', icon: <ArrowUpAZ size={15} />, label: t('sheet_sort_sheet_az', { defaultValue: 'Trier la feuille de A à Z' }), onClick: () => sortRange(true) },
    { type: 'action', icon: <ArrowDownAZ size={15} />, label: t('sheet_sort_sheet_za', { defaultValue: 'Trier la feuille de Z à A' }), onClick: () => sortRange(false) },
    { type: 'separator' },
    { type: 'action', label: t('sheet_cond_format', { defaultValue: 'Mise en forme conditionnelle' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_data_validation', { defaultValue: 'Validation des données' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_col_stats', { defaultValue: 'Statistiques de colonne' }), onClick: menuSoon },
    { type: 'action', label: t('sheet_dropdown', { defaultValue: 'Menu déroulant' }), onClick: menuSoon },
    { type: 'submenu', icon: <Smile size={15} />, label: t('sheet_smart_chips', { defaultValue: 'Chips intelligents' }), items: smartChipsItems },
    { type: 'separator' },
    { type: 'submenu', icon: <MoreVertical size={15} />, label: t('sheet_more_actions_cols', { defaultValue: 'Afficher plus d’actions sur les colonnes' }), items: [
      { type: 'action', label: t('sheet_freeze_to_col', { defaultValue: 'Figer jusqu’à la colonne {{c}}', c: COLS[selColIdx] }), onClick: () => applyFreeze(frozenRows, selColIdx + 1) },
      { type: 'action', label: t('sheet_group_col', { defaultValue: 'Regrouper une colonne' }), onClick: menuSoon },
      { type: 'separator' },
      { type: 'action', label: t('sheet_get_range_link', { defaultValue: 'Obtenir le lien vers cette plage' }), onClick: () => copyCellLink() },
      { type: 'action', label: t('sheet_random_range', { defaultValue: 'Plage aléatoire' }), onClick: menuSoon },
      { type: 'action', label: t('names_define_sel', { defaultValue: 'Définir une plage nommée' }), onClick: () => setNameManagerOpen(true) },
      { type: 'action', label: t('sheet_protect_range', { defaultValue: 'Protéger la plage' }), onClick: menuSoon },
    ] },
  ] : cellMenu?.kind === 'row' ? [
    { type: 'action', label: t('common_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', onClick: () => copySelection(true) },
    { type: 'action', label: t('common_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', onClick: () => copySelection(false) },
    { type: 'action', label: t('common_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', onClick: () => pasteSelection() },
    { type: 'separator' },
    { type: 'action', label: t('sheet_insert_rows', { defaultValue: 'Insérer des lignes' }), onClick: () => insertRowsAction() },
    { type: 'action', label: t('sheet_delete_rows', { defaultValue: 'Supprimer des lignes' }), onClick: () => deleteRowsAction() },
    { type: 'separator' },
    { type: 'action', label: t('sheet_set_row_height_menu', { defaultValue: 'Hauteur de ligne…' }), onClick: () => promptRowHeight() },
    { type: 'action', label: t('sheet_hide_rows', { defaultValue: 'Masquer les lignes' }), onClick: () => hideRows() },
    { type: 'action', label: t('sheet_unhide_rows', { defaultValue: 'Afficher les lignes' }), onClick: () => unhideRows() },
  ] : [
    { type: 'action', icon: <Scissors size={15} />, label: t('common_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', onClick: () => copySelection(true) },
    { type: 'action', icon: <Copy size={15} />, label: t('common_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', onClick: () => copySelection(false) },
    { type: 'action', icon: <ClipboardPaste size={15} />, label: t('common_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', onClick: () => pasteSelection() },
    { type: 'submenu', icon: <ClipboardPaste size={15} />, label: t('sheet_paste_special', { defaultValue: 'Effectuer un collage spécial' }), items: pasteSpecialItems },
    { type: 'separator' },
    { type: 'action', icon: <Plus size={15} />, label: t('sheet_insert_row_above', { defaultValue: 'Insérer une ligne au-dessus' }), onClick: () => insertRowsAction() },
    { type: 'action', icon: <Plus size={15} />, label: t('sheet_insert_col_left', { defaultValue: 'Insérer une colonne à gauche' }), onClick: () => insertColsAction() },
    { type: 'submenu', icon: <Plus size={15} />, label: t('sheet_insert_cells', { defaultValue: 'Insérer des cellules' }), items: [
      { type: 'action', label: t('sheet_insert_cells_right', { defaultValue: 'Insérer des cellules et décaler vers la droite' }), onClick: () => insertCellsShift('right') },
      { type: 'action', label: t('sheet_insert_cells_down', { defaultValue: 'Insérer des cellules et décaler vers le bas' }), onClick: () => insertCellsShift('down') },
    ] },
    { type: 'separator' },
    { type: 'action', icon: <Trash2 size={15} />, label: t('sheet_delete_row', { defaultValue: 'Supprimer la ligne' }), onClick: () => deleteRowsAction() },
    { type: 'action', icon: <Trash2 size={15} />, label: t('sheet_delete_col', { defaultValue: 'Supprimer la colonne' }), onClick: () => deleteColsAction() },
    { type: 'submenu', icon: <Trash2 size={15} />, label: t('sheet_delete_cells', { defaultValue: 'Supprimer des cellules' }), items: [
      { type: 'action', label: t('sheet_delete_cells_left', { defaultValue: 'Supprimer des cellules et décaler vers la gauche' }), onClick: () => deleteCellsShift('left') },
      { type: 'action', label: t('sheet_delete_cells_up', { defaultValue: 'Supprimer des cellules et décaler vers le haut' }), onClick: () => deleteCellsShift('up') },
    ] },
    { type: 'separator' },
    { type: 'action', icon: <Table2 size={15} />, label: t('sheet_clear_contents', { defaultValue: 'Effacer le contenu' }), onClick: () => clearSelection() },
    { type: 'action', icon: <Filter size={15} />, label: t('sheet_create_filter', { defaultValue: 'Créer un filtre' }), onClick: () => setFilterMode(true) },
    { type: 'separator' },
    selectionHasMerge
      ? { type: 'action', icon: <TableCellsMerge size={15} />, label: t('sheet_unmerge', { defaultValue: 'Dissocier les cellules' }), onClick: () => unmergeSelection() }
      : { type: 'action', icon: <TableCellsMerge size={15} />, label: t('sheet_merge', { defaultValue: 'Fusionner les cellules' }), onClick: () => mergeSelection() },
    { type: 'action', icon: <ExternalLink size={15} />, label: t('sheet_insert_link', { defaultValue: 'Insérer un lien' }), onClick: menuSoon },
    { type: 'submenu', icon: <Smile size={15} />, label: t('sheet_smart_chips', { defaultValue: 'Chips intelligents' }), items: smartChipsItems },
    { type: 'separator' },
    { type: 'submenu', icon: <MoreVertical size={15} />, label: t('sheet_more_actions', { defaultValue: 'Afficher plus d’actions sur les cellules' }), items: [
      { type: 'action', label: t('sheet_cond_format', { defaultValue: 'Mise en forme conditionnelle' }), onClick: menuSoon },
      { type: 'action', label: t('sheet_data_validation', { defaultValue: 'Validation des données' }), onClick: menuSoon },
      { type: 'separator' },
      { type: 'action', label: t('sheet_get_cell_link', { defaultValue: 'Obtenir le lien vers cette cellule' }), onClick: () => copyCellLink() },
      { type: 'action', label: t('names_define_sel', { defaultValue: 'Définir une plage nommée' }), onClick: () => setNameManagerOpen(true) },
      { type: 'action', label: t('sheet_protect_range', { defaultValue: 'Protéger la plage' }), onClick: menuSoon },
    ] },
  ]

  // ── API de macro (Kubuno.Sheet) — agit sur la feuille ACTIVE et vivante ──────────
  // Construite à la demande (clic « Exécuter »). Les écritures vont dans une copie de
  // travail `work` (lecture-après-écriture correcte au sein d'une macro) puis sont
  // committées en lot via microtask (un seul commit/save par salve d'écritures).
  const makeSheetApi = () => {
    const work: Record<string, CellData> = { ...sheetDataRef.current.cells }
    // Commit synchrone à chaque écriture : la lecture-après-écriture reste correcte
    // (via `work`) et l'état/affichage se met à jour immédiatement.
    const flush = () => { commitData({ ...sheetDataRef.current, cells: { ...work } }) }
    const keyOf = (c: number, r: number) => `${indexToCol(c)}${r}`
    const computed = (key: string): string | number | boolean | null => {
      const cell = work[key]; if (!cell) return null
      if (cell.f && cell.f.startsWith('=')) { const v = evaluate(cell.f, { cells: work }); const s = Array.isArray(v) ? v[0]?.[0] : v; return (typeof s === 'number' || typeof s === 'string' || typeof s === 'boolean') ? s : null }
      return cell.v ?? null
    }
    const setRaw = (c: number, r: number, value: unknown) => {
      const key = keyOf(c, r)
      if (value == null || value === '') { delete work[key]; flush(); return }
      const str = String(value)
      const isF = str.startsWith('=')
      const num = !isF && str.trim() !== '' && !isNaN(Number(str)) ? Number(str) : undefined
      work[key] = { ...work[key], v: isF ? undefined : (num ?? str), f: isF ? str : undefined }
      flush()
    }
    const mergeStyle = (c1: number, r1: number, c2: number, r2: number, patch: Partial<NonNullable<CellData['s']>>) => {
      for (let r = r1; r <= r2; r++) for (let c = c1; c <= c2; c++) {
        const key = keyOf(c, r); const cur = work[key] ?? {}
        work[key] = { ...cur, s: { ...cur.s, ...patch } }
      }
      flush()
    }
    // ── Helpers internes ────────────────────────────────────────────────────────
    const readRange = (range: string): (string | number | boolean | null)[][] => {
      const b = parseRangeAddr(range); if (!b) return []
      const out: (string | number | boolean | null)[][] = []
      for (let r = b.r1; r <= b.r2; r++) { const row: (string | number | boolean | null)[] = []; for (let c = b.c1; c <= b.c2; c++) row.push(computed(keyOf(c, r))); out.push(row) }
      return out
    }
    const numbersIn = (range: string): number[] => readRange(range).flat()
      .map(v => typeof v === 'number' ? v : (typeof v === 'string' && v.trim() !== '' && !isNaN(Number(v)) ? Number(v) : null))
      .filter((v): v is number => v != null)
    const targetB = (range?: string) => range ? parseRangeAddr(range) : bounds()
    const style = (range: string | undefined, patch: Partial<NonNullable<CellData['s']>>) => { const b = targetB(range); if (b) mergeStyle(b.c1, b.r1, b.c2, b.r2, patch) }
    // Plage utilisée calculée sur la COPIE DE TRAVAIL `work` (l'état React usedBounds
    // est figé pendant l'exécution de la macro → ne pas l'utiliser ici).
    const usedFromWork = () => {
      let maxRow = 1, maxCol = 0
      for (const k of Object.keys(work)) { const cell = work[k]; if (cell.v == null && !cell.f) continue; const m = /^([A-Za-z]+)([0-9]+)$/.exec(k); if (!m) continue; const r = +m[2]; if (r > maxRow) maxRow = r; const c = colToIndex(m[1]); if (c > maxCol) maxCol = c }
      return { maxRow: Math.min(maxRow, MAX_ROWS), maxCol: Math.min(maxCol, MAX_COLS - 1) }
    }
    const usedRangeA1 = () => { const u = usedFromWork(); return `A1:${indexToCol(u.maxCol)}${u.maxRow}` }

    const Sheet = {
      // ── Sélection / navigation ──────────────────────────────────────────────
      /** Adresse de la cellule active, ex. "B3" (ou null). */
      getActiveCell: () => (selectedCell ? `${selectedCell.col}${selectedCell.row}` : null),
      /** Plage sélectionnée { from, to } en notation A1. */
      getSelection: () => { const b = bounds(); return b ? { from: `${indexToCol(b.c1)}${b.r1}`, to: `${indexToCol(b.c2)}${b.r2}` } : null },
      /** Sélectionne une cellule ou une plage (ex. "B2" ou "A1:C5"). */
      select: (range: string) => { const b = parseRangeAddr(range); if (!b) return; setSelectedCell({ col: indexToCol(b.c1), row: b.r1 }); setRangeEnd((b.c1 !== b.c2 || b.r1 !== b.r2) ? { col: indexToCol(b.c2), row: b.r2 } : null) },

      // ── Lecture ─────────────────────────────────────────────────────────────
      /** Valeur calculée d'une cellule (formule évaluée). */
      getValue: (ref: string) => { const a = parseRefAddr(ref); return a ? computed(keyOf(a.c, a.r)) : null },
      /** Formule brute d'une cellule (ex. "=A1+1"), sinon null. */
      getFormula: (ref: string) => { const a = parseRefAddr(ref); return a ? (work[keyOf(a.c, a.r)]?.f ?? null) : null },
      /** Détail d'une cellule : { value, formula, bold, italic, color, background, align, numberFormat }. */
      getCell: (ref: string) => { const a = parseRefAddr(ref); if (!a) return null; const cell = work[keyOf(a.c, a.r)]; const s = cell?.s ?? {}; return { value: computed(keyOf(a.c, a.r)), formula: cell?.f ?? null, bold: !!s.bold, italic: !!s.italic, color: s.color ?? null, background: s.bg ?? null, align: s.align ?? null, numberFormat: s.numFmt ?? null } },
      /** Matrice des valeurs calculées d'une plage (lignes × colonnes). */
      getRangeValues: (range: string) => readRange(range),
      /** Valeurs de toute la plage utilisée. */
      getValues: () => readRange(usedRangeA1()),
      /** Valeurs d'une ligne (1-based) sur la largeur utilisée. */
      getRow: (row: number) => readRange(`A${row}:${indexToCol(usedFromWork().maxCol)}${row}`)[0] ?? [],
      /** Valeurs d'une colonne (lettre "B" ou index 1-based) sur la hauteur utilisée. */
      getColumn: (col: string | number) => { const ci = typeof col === 'number' ? col - 1 : colToIndex(String(col).toUpperCase()); const L = indexToCol(Math.max(0, ci)); return readRange(`${L}1:${L}${usedFromWork().maxRow}`).map(r => r[0]) },
      /** Cherche un texte (insensible à la casse) ; renvoie la 1ʳᵉ référence ou null. */
      find: (text: string) => { const needle = String(text).toLowerCase(); const u = usedFromWork(); for (let r = 1; r <= u.maxRow; r++) for (let c = 0; c <= u.maxCol; c++) { const v = computed(keyOf(c, r)); if (v != null && String(v).toLowerCase().includes(needle)) return `${indexToCol(c)}${r}` } return null },
      /** Dernière ligne/colonne contenant des données. */
      getUsedRange: () => { const u = usedFromWork(); return { rows: u.maxRow, cols: u.maxCol + 1 } },
      getLastRow: () => usedFromWork().maxRow,
      getLastColumn: () => usedFromWork().maxCol + 1,

      // ── Écriture ────────────────────────────────────────────────────────────
      /** Écrit une valeur OU une formule (chaîne commençant par "="). */
      setValue: (ref: string, value: unknown) => { const a = parseRefAddr(ref); if (a) setRaw(a.c, a.r, value) },
      /** Écrit une formule (ajoute "=" si absent). */
      setFormula: (ref: string, formula: string) => { const a = parseRefAddr(ref); if (a) setRaw(a.c, a.r, formula.startsWith('=') ? formula : '=' + formula) },
      /** Écrit une matrice de valeurs à partir du coin haut-gauche de la plage/cellule. */
      setRangeValues: (range: string, values: unknown[][]) => { const b = parseRangeAddr(range); if (!b || !Array.isArray(values)) return; for (let i = 0; i < values.length; i++) for (let j = 0; j < (values[i]?.length ?? 0); j++) { const c = b.c1 + j, r = b.r1 + i; if (c < MAX_COLS && r <= MAX_ROWS) setRaw(c, r, values[i][j]) } },
      /** Ajoute une ligne de valeurs juste après la dernière ligne utilisée. */
      appendRow: (values: unknown[]) => { const r = usedFromWork().maxRow + 1; for (let j = 0; j < values.length; j++) setRaw(j, r, values[j]) },
      /** Vide le contenu d'une cellule/plage. */
      clear: (range: string) => { const b = parseRangeAddr(range); if (!b) return; for (let r = b.r1; r <= b.r2; r++) for (let c = b.c1; c <= b.c2; c++) delete work[keyOf(c, r)]; flush() },

      // ── Mise en forme (range optionnel = sélection courante) ─────────────────
      setBold: (range?: string, on = true) => style(range, { bold: on }),
      setItalic: (range?: string, on = true) => style(range, { italic: on }),
      setUnderline: (range?: string, on = true) => style(range, { underline: on }),
      setStrikethrough: (range?: string, on = true) => style(range, { strike: on }),
      setFontSize: (size: number, range?: string) => style(range, { fontSize: size }),
      setFontFamily: (family: string, range?: string) => style(range, { fontFamily: family }),
      setColor: (color: string, range?: string) => style(range, { color }),
      setBackground: (color: string, range?: string) => style(range, { bg: color }),
      /** Alignement horizontal : 'left' | 'center' | 'right'. */
      setAlign: (align: 'left' | 'center' | 'right', range?: string) => style(range, { align }),
      /** Format numérique : 'number' | 'currency' | 'percent' | 'scientific' (+ décimales). */
      setNumberFormat: (fmt: 'number' | 'currency' | 'percent' | 'scientific', decimals?: number, range?: string) => style(range, { numFmt: fmt, ...(decimals != null ? { decimals } : {}) }),
      /** Applique un style arbitraire ({ bold, bg, color, align, … }). */
      setStyle: (range: string, patch: Partial<NonNullable<CellData['s']>>) => style(range, patch),
      /** Retire toute la mise en forme d'une plage. */
      clearFormat: (range: string) => { const b = parseRangeAddr(range); if (!b) return; for (let r = b.r1; r <= b.r2; r++) for (let c = b.c1; c <= b.c2; c++) { const k = keyOf(c, r); if (work[k]) work[k] = { ...work[k], s: undefined } } flush() },

      // ── Agrégats (sur une plage) ─────────────────────────────────────────────
      sum: (range: string) => numbersIn(range).reduce((a, b) => a + b, 0),
      average: (range: string) => { const n = numbersIn(range); return n.length ? n.reduce((a, b) => a + b, 0) / n.length : 0 },
      min: (range: string) => { const n = numbersIn(range); return n.length ? Math.min(...n) : 0 },
      max: (range: string) => { const n = numbersIn(range); return n.length ? Math.max(...n) : 0 },
      count: (range: string) => numbersIn(range).length,
      countA: (range: string) => readRange(range).flat().filter(v => v != null && v !== '').length,

      // ── Feuilles ─────────────────────────────────────────────────────────────
      getSheetName: () => sheetMetas.find(m => m.id === activeSheetId)?.name ?? '',
      getSheetNames: () => sheetMetas.map(m => m.name),
      getSheetCount: () => sheetMetas.length,
    }

    const Utils = {
      uuid: () => crypto.randomUUID(),
      /** Formate un nombre selon la locale (décimales optionnelles). */
      formatNumber: (n: number, decimals?: number) => Number(n).toLocaleString(i18n.language, decimals != null ? { minimumFractionDigits: decimals, maximumFractionDigits: decimals } : {}),
      /** Formate une date avec des jetons yyyy/MM/dd/HH/mm/ss. */
      formatDate: (date: Date | string | number, fmt = 'yyyy-MM-dd') => { const d = date instanceof Date ? date : new Date(date); const p = (x: number, n = 2) => String(x).padStart(n, '0'); return fmt.replace(/yyyy/g, String(d.getFullYear())).replace(/MM/g, p(d.getMonth() + 1)).replace(/dd/g, p(d.getDate())).replace(/HH/g, p(d.getHours())).replace(/mm/g, p(d.getMinutes())).replace(/ss/g, p(d.getSeconds())) },
      today: () => new Date(),
      now: () => new Date(),
      /** Lettre de colonne depuis un index 1-based (1→"A"). */
      columnLetter: (n: number) => indexToCol(Math.max(0, n - 1)),
      /** Index 1-based depuis une lettre de colonne ("A"→1). */
      columnNumber: (letter: string) => colToIndex(String(letter).toUpperCase()) + 1,
    }

    const App = {
      getType: () => 'spreadsheet',
      getId: () => ssId,
      toast: (msg: unknown) => { try { window.dispatchEvent(new CustomEvent('kubuno-toast', { detail: String(msg) })) } catch { /* noop */ } console.log(String(msg)) },
      log: (...args: unknown[]) => console.log(...args.map(a => typeof a === 'string' ? a : JSON.stringify(a))),
      /** Boîte d'alerte (modale). À `await`. */
      alert: (msg: unknown) => appAlert(msg),
      /** Confirmation (OK/Annuler) → booléen. À `await`. */
      confirm: (msg: unknown) => appConfirm(msg),
      /** Saisie (champ + OK/Annuler) → chaîne ou null. À `await`. */
      prompt: (msg: unknown, def?: unknown) => appPrompt(msg, def),
      /** Pause de `ms` millisecondes. À `await`. */
      sleep: (ms: number) => new Promise<void>(res => setTimeout(res, Math.max(0, ms))),
    }
    return { Sheet, Utils, App }
  }

  // ── Ruban (façon MS Office) — construit ici (actions + style sélectionné), remonté
  // au parent qui le rend dans OfficeShell. Remplace l'ancienne toolbar. ───────────
  const st = selectedCellStyle
  const textColorRender = (
    <div key="tc">
      <button ref={textColorBtnRef} onClick={() => setTextColorOpen(o => !o)} className="w-8 h-[52px] flex flex-col items-center justify-center rounded hover:bg-[#e8eaed] gap-0.5" title={t('sheet_text_color')}>
        <Type size={16} />
        <div className="w-5 h-1 rounded-sm" style={{ background: st.color ?? '#000' }} />
      </button>
      <AnchoredPopover anchorRef={textColorBtnRef} open={textColorOpen} onClose={() => setTextColorOpen(false)}>
        <ColorSwatchPicker color={st.color ?? '#000000'} t={t} onChange={hex => applyToSelection({ color: hex })} onClose={() => setTextColorOpen(false)} customLabel={t('sheet_text_color')} />
      </AnchoredPopover>
    </div>
  )
  const fillRender = (
    <div key="fill">
      <button ref={fillBtnRef} onClick={() => setFillOpen(o => !o)} className="w-8 h-[52px] flex flex-col items-center justify-center rounded hover:bg-[#e8eaed] gap-0.5" title={t('sheet_fill_color')}>
        <PaintBucket size={16} />
        <div className="w-5 h-1 rounded-sm" style={{ background: st.bgGradient ? gradientToCss(st.bgGradient) : (st.bg ?? 'transparent'), border: '1px solid #dadce0' }} />
      </button>
      <AnchoredPopover anchorRef={fillBtnRef} open={fillOpen} onClose={() => setFillOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-lg p-2 flex flex-col gap-2" style={{ width: 200 }}>
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs text-text-secondary">{t('sheet_fill_solid')}</span>
            <div className="flex items-center gap-1">
              <ColorField width={26} height={20} color={st.bg ?? '#ffffff'} onChange={hex => applyToSelection({ bg: hex, bgGradient: undefined })} />
              <button onClick={() => applyToSelection({ bg: undefined, bgGradient: undefined })} className="px-1.5 h-5 text-[10px] rounded border border-[#dadce0] hover:bg-[#f1f3f4]">{t('sheet_fill_none')}</button>
            </div>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs text-text-secondary">{t('sheet_fill_gradient')}</span>
            <GradientField width={40} height={20} value={st.bgGradient ?? DEFAULT_GRADIENT} onChange={g => applyToSelection({ bgGradient: g, bg: undefined })} />
          </div>
        </div>
      </AnchoredPopover>
    </div>
  )
  const bordersRender = (
    <div key="borders">
      <button ref={bordersBtnRef} onClick={() => setBordersOpen(o => !o)} title={t('sheet_borders')} className={`h-[52px] px-2 flex flex-col items-center justify-center gap-1 rounded hover:bg-[#e8eaed] ${bordersOpen ? 'bg-[#e8f0fe] text-primary' : ''}`}>
        <Grid2x2 size={16} /><span className="text-[10px] flex items-center">{t('sheet_borders')} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={bordersBtnRef} open={bordersOpen} onClose={() => setBordersOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-lg p-1.5" style={{ width: 168 }}>
          <div className="grid grid-cols-4 gap-0.5">
            {([['all', t('sheet_border_all'), BorderIcon.all], ['inner', t('sheet_border_inner'), BorderIcon.inner], ['outer', t('sheet_border_outer'), BorderIcon.outer], ['none', t('sheet_border_none'), BorderIcon.none], ['top', t('sheet_border_top'), BorderIcon.top], ['bottom', t('sheet_border_bottom'), BorderIcon.bottom], ['left', t('sheet_border_left'), BorderIcon.left], ['right', t('sheet_border_right'), BorderIcon.right]] as const).map(([kind, label, glyph]) => (
              <button key={kind} title={label} onClick={() => applyBorders(kind)} className="w-9 h-9 flex items-center justify-center rounded hover:bg-[#e8eaed] text-text-secondary">{glyph}</button>
            ))}
          </div>
        </div>
      </AnchoredPopover>
    </div>
  )
  const freezeRender = (
    <div key="freeze">
      <button ref={freezeBtnRef} onClick={() => setFreezeOpen(o => !o)} title={t('sheet_freeze', 'Figer')} className={`h-[52px] px-2 flex flex-col items-center justify-center gap-1 rounded hover:bg-[#e8eaed] ${(frozenRows > 0 || frozenCols > 0) ? 'bg-[#e8f0fe] text-primary' : ''}`}>
        <Snowflake size={16} /><span className="text-[10px] flex items-center">{t('sheet_freeze', 'Figer')} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={freezeBtnRef} open={freezeOpen} onClose={() => setFreezeOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-lg py-1 text-xs" style={{ width: 230 }}>
          {([[t('sheet_freeze_none', 'Aucune ligne/colonne figée'), () => applyFreeze(0, 0)], [t('sheet_freeze_1row', 'Figer 1 ligne'), () => applyFreeze(1, frozenCols)], [t('sheet_freeze_2rows', 'Figer 2 lignes'), () => applyFreeze(2, frozenCols)], ...(selectedCell ? [[t('sheet_freeze_uptorow', 'Figer jusqu’à la ligne {{n}}').replace('{{n}}', String(selectedCell.row)), () => applyFreeze(selectedCell.row, frozenCols)] as [string, () => void]] : []), ['---', null], [t('sheet_freeze_1col', 'Figer 1 colonne'), () => applyFreeze(frozenRows, 1)], [t('sheet_freeze_2cols', 'Figer 2 colonnes'), () => applyFreeze(frozenRows, 2)], ...(selectedCell ? [[t('sheet_freeze_uptocol', 'Figer jusqu’à la colonne {{c}}').replace('{{c}}', selectedCell.col), () => applyFreeze(frozenRows, COLS.indexOf(selectedCell.col) + 1)] as [string, () => void]] : [])] as [string, (() => void) | null][]).map(([label, fn], i) =>
            label === '---' ? <div key={i} className="my-1 border-t border-[#eee]" /> : <button key={i} onClick={() => { fn?.(); setFreezeOpen(false) }} className="w-full text-left px-3 py-1.5 hover:bg-[#f1f3f4]">{label}</button>)}
        </div>
      </AnchoredPopover>
    </div>
  )
  const macrosRender =<div key="macros" className="self-center"><MacrosMenu docType="spreadsheet" docId={ssId} buildApi={makeSheetApi} /></div>

  // ── Ribbon actions (Insertion / Formules / Révision) ─────────────────────────
  // Clear only the formatting of the selection (keep values/formulas).
  const clearFormats = () => {
    const b = bounds(); if (!b) return
    const newData: SheetData = { ...sheetData }
    const wholeCols = b.r1 === 1 && b.r2 >= MAX_ROWS
    const wholeRows = b.c1 === 0 && b.c2 >= MAX_COLS - 1
    if (wholeCols && sheetData.colStyles) {
      const cs = { ...sheetData.colStyles }; for (let ci = b.c1; ci <= b.c2; ci++) delete cs[COLS[ci]]; newData.colStyles = cs
    }
    if (wholeRows && sheetData.rowStyles) {
      const rs = { ...sheetData.rowStyles }; for (let ri = b.r1; ri <= b.r2; ri++) delete rs[String(ri)]; newData.rowStyles = rs
    }
    // Only a whole-column/row selection is capped (avoid MAX_ROWS×MAX_COLS iteration);
    // a bounded manual selection is cleared in full.
    const c2 = wholeRows ? Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1)) : b.c2
    const r2 = wholeCols ? Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1)) : b.r2
    const cells = { ...sheetData.cells }
    for (let ci = b.c1; ci <= c2; ci++) for (let ri = b.r1; ri <= r2; ri++) {
      const k = cellKey(COLS[ci], ri); const cell = cells[k]; if (cell?.s) cells[k] = { ...cell, s: undefined }
    }
    newData.cells = cells
    commitData(newData)
  }
  // Start editing the active cell with a leading '=' (insert a function).
  const insertFunction = () => { if (selectedCell) startEditCell(selectedCell.col, selectedCell.row, '=') }
  // AutoSum: write =SUM() over the contiguous numeric run above (else to the left).
  const autoSum = () => {
    if (!selectedCell) return
    const c = COLS.indexOf(selectedCell.col), r = selectedCell.row
    const numAt = (cc: number, rr: number) => numericValue(sheetData.cells[cellKey(COLS[cc], rr)], sheetData, cellKey(COLS[cc], rr), spill) != null
    let top = r - 1; while (top >= 1 && numAt(c, top)) top--
    if (top + 1 <= r - 1) { updateCell(selectedCell.col, r, `=SUM(${selectedCell.col}${top + 1}:${selectedCell.col}${r - 1})`); return }
    let left = c - 1; while (left >= 0 && numAt(left, r)) left--
    if (left + 1 <= c - 1) updateCell(selectedCell.col, r, `=SUM(${COLS[left + 1]}${r}:${COLS[c - 1]}${r})`)
    else startEditCell(selectedCell.col, r, '=SUM(')
  }
  // Insert a picture from a local file (embedded as a base64 data URL, like imports).
  const insertImageFromFile = () => {
    const input = document.createElement('input')
    input.type = 'file'; input.accept = 'image/*'
    input.onchange = () => {
      const file = input.files?.[0]; if (!file) return
      const reader = new FileReader()
      reader.onload = () => {
        const src = String(reader.result)
        const col = selectedCell ? COLS.indexOf(selectedCell.col) : 0
        const row = selectedCell ? selectedCell.row : 1
        const bx = (geom.colLeft[col] - ROW_HEADER_WIDTH) / zoom, by = (geom.rowTop[row] - COL_HEADER_HEIGHT) / zoom
        const probe = new Image()
        probe.onload = () => {
          const scale = probe.naturalWidth > 240 ? 240 / probe.naturalWidth : 1
          const idx = localImagesRef.current.length
          setLocalImages(prev => {
            const next = [...prev, { fromCol: col, fromRow: row - 1, bx, by, bw: probe.naturalWidth * scale, bh: probe.naturalHeight * scale, src }]
            saveImagesMut.mutate(next); return next
          })
          setSelectedImage(idx)
        }
        probe.src = src
      }
      reader.readAsDataURL(file)
    }
    input.click()
  }

  // ── Insert / delete rows & columns (structural) ──────────────────────────────
  // Shift cell references in a formula when a band is inserted/deleted on an axis.
  // `at`: row axis = 1-based row; col axis = 0-based col index. delta = +n insert / -n delete.
  const shiftRefs = (f: string, axis: 'row' | 'col', at: number, delta: number): string =>
    f.replace(/(\$?)([A-Za-z]{1,3})(\$?)(\d+)/g, (_m, ad: string, col: string, ar: string, row: string) => {
      let c = colToIndex(col.toUpperCase()); let r = parseInt(row)
      if (axis === 'col') {
        if (delta < 0 && c >= at && c < at - delta) return '#REF!'
        if (c >= at) c += delta
      } else {
        if (delta < 0 && r >= at && r < at - delta) return '#REF!'
        if (r >= at) r += delta
      }
      if (c < 0 || r < 1) return '#REF!'
      return `${ad}${indexToCol(c)}${ar}${r}`
    })
  // Insert (count>0) or delete (count<0) `|count|` bands at `at` on the given axis,
  // rebuilding the cell map (shifted keys + adjusted formulas), merges and sizes.
  const structuralEdit = (axis: 'row' | 'col', at: number, count: number) => {
    if (count === 0) return
    const cells: Record<string, CellData> = {}
    for (const [key, cell] of Object.entries(sheetData.cells)) {
      const m = /^([A-Z]+)(\d+)$/.exec(key); if (!m) continue
      let c = colToIndex(m[1]); let r = parseInt(m[2])
      const pos = axis === 'col' ? c : r
      if (count < 0 && pos >= at && pos < at - count) continue // inside deleted band → drop
      if (pos >= at) { if (axis === 'col') c += count; else r += count }
      const nc: CellData = { ...cell }
      if (nc.f) nc.f = shiftRefs(nc.f, axis, at, count)
      cells[`${indexToCol(c)}${r}`] = nc
    }
    // Coordinate shift on the edited axis (null = the band/value is removed).
    const shift = (v: number, isStart: boolean): number | null => {
      if (count >= 0) return v >= at ? v + count : v
      const delEnd = at - count
      if (v < at) return v
      if (v >= delEnd) return v + count
      return isStart ? at : at - 1 // clamp a corner that fell inside the deleted band
    }
    // Shift the column/row default styles along the edited axis (keyed by letter/number).
    const nextData: SheetData = { ...sheetData, cells }
    if (axis === 'col' && sheetData.colStyles) {
      const cs: Record<string, NonNullable<CellData['s']>> = {}
      for (const [k, v] of Object.entries(sheetData.colStyles)) { const nci = count < 0 && colToIndex(k) >= at && colToIndex(k) < at - count ? null : shift(colToIndex(k), true); if (nci != null) cs[indexToCol(nci)] = v }
      nextData.colStyles = cs
    }
    if (axis === 'row' && sheetData.rowStyles) {
      const rs: Record<string, NonNullable<CellData['s']>> = {}
      for (const [k, v] of Object.entries(sheetData.rowStyles)) { const nr = count < 0 && +k >= at && +k < at - count ? null : shift(+k, true); if (nr != null) rs[String(nr)] = v }
      nextData.rowStyles = rs
    }
    commitData(nextData)
    // Merges
    const newMerges = (localMerges ?? []).map(ref => {
      const b = parseRangeAddr(ref); if (!b) return null
      let { c1, c2, r1, r2 } = b
      if (axis === 'col') { const a = shift(c1, true), z = shift(c2, false); if (a == null || z == null || a > z) return null; c1 = a; c2 = z }
      else { const a = shift(r1, true), z = shift(r2, false); if (a == null || z == null || a > z) return null; r1 = a; r2 = z }
      if (c1 === c2 && r1 === r2) return null
      return `${indexToCol(c1)}${r1}:${indexToCol(c2)}${r2}`
    }).filter((x): x is string => !!x)
    setLocalMerges(newMerges)
    // Sizes (row_heights keyed by row number; col_widths keyed by letter)
    let newWidths = localColWidths, newHeights = localRowHeights
    if (axis === 'row') {
      newHeights = {}
      for (const [k, v] of Object.entries(localRowHeights)) { const nr = shift(parseInt(k), true); if (nr != null) newHeights[String(nr)] = v }
      setLocalRowHeights(newHeights)
    } else {
      newWidths = {}
      for (const [k, v] of Object.entries(localColWidths)) { const nc = shift(colToIndex(k), true); if (nc != null) newWidths[indexToCol(nc)] = v }
      setLocalColWidths(newWidths)
    }
    saveDimensionsMut.mutate({ merges: newMerges, col_widths: newWidths, row_heights: newHeights })
  }
  const insertRowsAction = () => { const b = bounds(); if (b) structuralEdit('row', b.r1, b.r2 - b.r1 + 1) }
  const deleteRowsAction = () => { const b = bounds(); if (b) structuralEdit('row', b.r1, -(b.r2 - b.r1 + 1)) }
  const insertColsAction = () => { const b = bounds(); if (b) structuralEdit('col', b.c1, b.c2 - b.c1 + 1) }
  const deleteColsAction = () => { const b = bounds(); if (b) structuralEdit('col', b.c1, -(b.c2 - b.c1 + 1)) }

  // ── Sort the selected range by its key column ─────────────────────────────────
  const sortRange = (asc: boolean) => {
    const b = bounds(); if (!b || b.r2 <= b.r1) return
    // A whole-column sort only needs the populated rows; cap so we never sort a
    // million empty cells. A whole-row sort caps the column extent likewise.
    const r2 = Math.min(b.r2, Math.max(usedBounds.maxRow, b.r1)), c2 = Math.min(b.c2, Math.max(usedBounds.maxCol, b.c1))
    if (r2 <= b.r1) return
    const keyIdx = (selectedCell ? COLS.indexOf(selectedCell.col) : b.c1) - b.c1
    const rows: (CellData | undefined)[][] = []
    for (let r = b.r1; r <= r2; r++) {
      const rc: (CellData | undefined)[] = []
      for (let c = b.c1; c <= c2; c++) rc.push(sheetData.cells[cellKey(COLS[c], r)])
      rows.push(rc)
    }
    const keyOf = (rc: (CellData | undefined)[]) => {
      const cell = rc[keyIdx]
      const n = numericValue(cell, sheetData, undefined, spill)
      return n != null ? { n, s: '' } : { n: null as number | null, s: resolveValue(cell, sheetData, undefined, spill).toLowerCase() }
    }
    rows.sort((ra, rb) => {
      const ka = keyOf(ra), kb = keyOf(rb)
      if (ka.n != null && kb.n != null) return ka.n - kb.n
      if (ka.n != null) return -1
      if (kb.n != null) return 1
      return ka.s < kb.s ? -1 : ka.s > kb.s ? 1 : 0
    })
    if (!asc) rows.reverse()
    const cells = { ...sheetData.cells }
    for (let i = 0; i < rows.length; i++) for (let c = b.c1; c <= c2; c++) {
      const k = cellKey(COLS[c], b.r1 + i); const cell = rows[i][c - b.c1]
      if (cell) cells[k] = cell; else delete cells[k]
    }
    commitData({ ...sheetData, cells })
  }

  const insertCellsRender = (
    <div key="insertcells">
      <button ref={insertBtnRef} onClick={() => setInsertCellsOpen(o => !o)} title={t('sheet_insert', { defaultValue: 'Insérer' })} className={`h-[52px] px-2 flex flex-col items-center justify-center gap-1 rounded hover:bg-[#e8eaed] ${insertCellsOpen ? 'bg-[#e8f0fe] text-primary' : ''}`}>
        <Plus size={16} /><span className="text-[10px] flex items-center">{t('sheet_insert', { defaultValue: 'Insérer' })} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={insertBtnRef} open={insertCellsOpen} onClose={() => setInsertCellsOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-lg py-1 text-xs" style={{ width: 220 }}>
          <button onClick={() => { insertRowsAction(); setInsertCellsOpen(false) }} className="w-full text-left px-3 py-1.5 hover:bg-[#f1f3f4] flex items-center gap-2"><Rows3 size={14} /> {t('sheet_insert_rows', { defaultValue: 'Insérer des lignes' })}</button>
          <button onClick={() => { insertColsAction(); setInsertCellsOpen(false) }} className="w-full text-left px-3 py-1.5 hover:bg-[#f1f3f4] flex items-center gap-2"><Columns3 size={14} /> {t('sheet_insert_cols', { defaultValue: 'Insérer des colonnes' })}</button>
        </div>
      </AnchoredPopover>
    </div>
  )
  const deleteCellsRender = (
    <div key="deletecells">
      <button ref={deleteBtnRef} onClick={() => setDeleteCellsOpen(o => !o)} title={t('sheet_delete', { defaultValue: 'Supprimer' })} className={`h-[52px] px-2 flex flex-col items-center justify-center gap-1 rounded hover:bg-[#e8eaed] ${deleteCellsOpen ? 'bg-[#e8f0fe] text-primary' : ''}`}>
        <Trash2 size={16} /><span className="text-[10px] flex items-center">{t('sheet_delete', { defaultValue: 'Supprimer' })} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={deleteBtnRef} open={deleteCellsOpen} onClose={() => setDeleteCellsOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-lg py-1 text-xs" style={{ width: 220 }}>
          <button onClick={() => { deleteRowsAction(); setDeleteCellsOpen(false) }} className="w-full text-left px-3 py-1.5 hover:bg-[#f1f3f4] flex items-center gap-2"><Rows3 size={14} /> {t('sheet_delete_rows', { defaultValue: 'Supprimer des lignes' })}</button>
          <button onClick={() => { deleteColsAction(); setDeleteCellsOpen(false) }} className="w-full text-left px-3 py-1.5 hover:bg-[#f1f3f4] flex items-center gap-2"><Columns3 size={14} /> {t('sheet_delete_cols', { defaultValue: 'Supprimer des colonnes' })}</button>
        </div>
      </AnchoredPopover>
    </div>
  )

  // Insert a special character into the active cell (Insertion › Symboles › Symbole).
  const SYMBOLS = ['±', '×', '÷', '≠', '≤', '≥', '≈', '√', '∑', '∏', '∞', 'π', 'µ', 'Ω', '°', '€', '£', '¢', '©', '®', '™', '→', '←', '↑', '↓', '•', '½', '¼', '¾', '²', '³', '‰']
  const insertSymbol = (sym: string) => {
    if (!selectedCell) return
    const k = cellKey(selectedCell.col, selectedCell.row); const cell = sheetData.cells[k]
    const cur = cell?.f ?? (cell?.v != null ? String(cell.v) : '')
    updateCell(selectedCell.col, selectedCell.row, cur + sym)
    setSymbolsOpen(false)
  }
  const symbolsRender = (
    <div key="symbols">
      <button ref={symbolsBtnRef} onClick={() => setSymbolsOpen(o => !o)} title={t('sheet_symbol', { defaultValue: 'Symbole' })} className={`h-[52px] px-2 flex flex-col items-center justify-center gap-1 rounded hover:bg-[#e8eaed] ${symbolsOpen ? 'bg-[#e8f0fe] text-primary' : ''}`}>
        <span className="text-base">Ω</span><span className="text-[10px] flex items-center">{t('sheet_symbol', { defaultValue: 'Symbole' })} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={symbolsBtnRef} open={symbolsOpen} onClose={() => setSymbolsOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-lg p-2" style={{ width: 248 }}>
          <div className="grid grid-cols-8 gap-0.5">
            {SYMBOLS.map(s => <button key={s} onClick={() => insertSymbol(s)} className="w-7 h-7 flex items-center justify-center rounded hover:bg-[#e8eaed] text-sm">{s}</button>)}
          </div>
        </div>
      </AnchoredPopover>
    </div>
  )
  // Predefined equation gallery (Insertion › Équation), like Excel.
  const PREDEFINED_EQUATIONS: { key: string; name: string; latex: string }[] = [
    { key: 'circle', name: t('sheet_eq_circle', { defaultValue: 'Surface du cercle' }), latex: 'A = \\pi r^2' },
    { key: 'binomial', name: t('sheet_eq_binomial', { defaultValue: 'Formule du binôme' }), latex: '(x+a)^n = \\sum_{k=0}^{n} \\binom{n}{k} x^k a^{n-k}' },
    { key: 'expansion', name: t('sheet_eq_expansion', { defaultValue: 'Élévation d’une somme' }), latex: '(1+x)^n = 1 + \\frac{nx}{1!} + \\frac{n(n-1)x^2}{2!} + \\cdots' },
    { key: 'fourier', name: t('sheet_eq_fourier', { defaultValue: 'Série de Fourier' }), latex: 'f(x) = a_0 + \\sum_{n=1}^{\\infty} \\left( a_n \\cos \\frac{n\\pi x}{L} + b_n \\sin \\frac{n\\pi x}{L} \\right)' },
    { key: 'pythagore', name: t('sheet_eq_pythagore', { defaultValue: 'Théorème de Pythagore' }), latex: 'a^2 + b^2 = c^2' },
    { key: 'quadratic', name: t('sheet_eq_quadratic', { defaultValue: 'Formule quadratique' }), latex: 'x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}' },
    { key: 'taylor', name: t('sheet_eq_taylor', { defaultValue: 'Développement de Taylor' }), latex: 'e^x = 1 + \\frac{x}{1!} + \\frac{x^2}{2!} + \\frac{x^3}{3!} + \\cdots, \\quad -\\infty < x < \\infty' },
    { key: 'trig1', name: t('sheet_eq_trig1', { defaultValue: 'Identité trigonométrique 1' }), latex: '\\sin\\alpha \\pm \\sin\\beta = 2 \\sin\\tfrac{1}{2}(\\alpha \\pm \\beta) \\cos\\tfrac{1}{2}(\\alpha \\mp \\beta)' },
    { key: 'trig2', name: t('sheet_eq_trig2', { defaultValue: 'Identité trigonométrique 2' }), latex: '\\cos\\alpha + \\cos\\beta = 2 \\cos\\tfrac{1}{2}(\\alpha + \\beta) \\cos\\tfrac{1}{2}(\\alpha - \\beta)' },
  ]
  const equationRender = (
    <div key="equation">
      <button ref={equationBtnRef} onClick={() => setEquationOpen(o => !o)} title={t('sheet_equation', { defaultValue: 'Équation' })} className={`h-[52px] px-2 flex flex-col items-center justify-center gap-0.5 rounded hover:bg-[#e8eaed] ${equationOpen ? 'bg-[#e8f0fe] text-primary' : ''}`}>
        <span className="text-lg font-serif italic leading-none" style={{ color: '#1a73e8' }}>π</span>
        <span className="text-[10px] flex items-center">{t('sheet_equation', { defaultValue: 'Équation' })} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={equationBtnRef} open={equationOpen} onClose={() => setEquationOpen(false)}>
        <div className="bg-white border border-[#dadce0] rounded-lg shadow-xl py-1" style={{ width: 360, maxHeight: 520, overflowY: 'auto' }}>
          {PREDEFINED_EQUATIONS.map(eq => (
            <button key={eq.key} onClick={() => { insertEquation(eq.latex); setEquationOpen(false) }} className="w-full text-left hover:bg-[#e8f0fe] focus:bg-[#e8f0fe] outline-none">
              <div className="px-3 pt-2 text-[11px] font-semibold text-text-secondary bg-[#f1f3f4]">{eq.name}</div>
              <div className="px-3 py-3 flex items-center justify-center" style={{ fontSize: 15 }} dangerouslySetInnerHTML={{ __html: renderKatex(eq.latex) }} />
            </button>
          ))}
          <div className="border-t border-[#e2e4e6] mt-1 pt-1">
            <button onClick={() => { insertEquation('', true); setEquationOpen(false) }} className="w-full text-left px-3 py-2 hover:bg-[#f1f3f4] flex items-center gap-3 text-sm"><span className="text-base font-serif italic" style={{ color: '#1a73e8' }}>π</span> {t('sheet_eq_new', { defaultValue: 'Insérer une nouvelle équation' })}</button>
            <button onClick={() => { setEquationOpen(false); soon() }} className="w-full text-left px-3 py-2 hover:bg-[#f1f3f4] flex items-center gap-3 text-sm text-text-secondary"><span className="text-base">✎</span> {t('sheet_eq_handwritten', { defaultValue: 'Équation manuscrite' })}</button>
          </div>
        </div>
      </AnchoredPopover>
    </div>
  )
  // Honest placeholder for Insertion features still being built (pivot, charts, …).
  const soon = () => { void appAlert(t('sheet_coming_soon', { defaultValue: 'Fonctionnalité bientôt disponible.' })) }

  const fontGroup = { id: 'font', label: t('sheet_grp_font', { defaultValue: 'Police' }), items: [
    { id: 'family', kind: 'dropdown' as const, value: st.fontFamily ?? 'Arial', options: fontFamilies.map(f => ({ value: f, label: f })), onChange: (v: string) => applyToSelection({ fontFamily: v }), width: 130, tooltip: t('sheet_font') },
    { id: 'size', kind: 'dropdown' as const, value: String(st.fontSize ?? 11), options: SHEET_FONT_SIZES.map(s => ({ value: s, label: s })), onChange: (v: string) => applyToSelection({ fontSize: Number(v) }), width: 56, tooltip: t('sheet_font_size', { defaultValue: 'Taille' }) },
    { id: 'bold', kind: 'toggle' as const, icon: <Bold size={15} />, active: !!st.bold, onClick: () => toggleStyle('bold'), tooltip: t('sheet_bold') },
    { id: 'italic', kind: 'toggle' as const, icon: <Italic size={15} />, active: !!st.italic, onClick: () => toggleStyle('italic'), tooltip: t('sheet_italic') },
    { id: 'underline', kind: 'toggle' as const, icon: <Underline size={15} />, active: !!st.underline, onClick: () => toggleStyle('underline'), tooltip: t('sheet_underline') },
    { id: 'strike', kind: 'toggle' as const, icon: <span className="text-sm font-bold line-through">S</span>, active: !!st.strike, onClick: () => toggleStyle('strike'), tooltip: t('sheet_strikethrough') },
    { id: 'textColor', kind: 'custom' as const, render: textColorRender },
    { id: 'fill', kind: 'custom' as const, render: fillRender },
  ] }
  const alignGroup = { id: 'align', label: t('sheet_grp_align', { defaultValue: 'Alignement' }), items: [
    { id: 'al', kind: 'toggle' as const, icon: <AlignLeft size={15} />, active: st.align === 'left' || !st.align, onClick: () => setAlign('left'), tooltip: t('sheet_align_left') },
    { id: 'ac', kind: 'toggle' as const, icon: <AlignCenter size={15} />, active: st.align === 'center', onClick: () => setAlign('center'), tooltip: t('sheet_align_center') },
    { id: 'ar', kind: 'toggle' as const, icon: <AlignRight size={15} />, active: st.align === 'right', onClick: () => setAlign('right'), tooltip: t('sheet_align_right') },
    { id: 'wrap', kind: 'toggle' as const, icon: <WrapText size={15} />, active: !!st.wrap, onClick: () => toggleStyle('wrap'), tooltip: t('sheet_wrap_text') },
    { id: 'merge', kind: 'toggle' as const, icon: <TableCellsMerge size={15} />, active: selectionHasMerge, onClick: toggleMerge, tooltip: t('sheet_merge_cells', { defaultValue: 'Fusionner / dissocier' }) },
  ] }
  const numberGroup = { id: 'number', label: t('sheet_grp_number', { defaultValue: 'Nombre' }), items: [
    { id: 'cur', kind: 'toggle' as const, icon: <Euro size={15} />, active: st.numFmt === 'currency', onClick: () => setNumFmt('currency'), tooltip: t('sheet_format_currency') },
    { id: 'pct', kind: 'toggle' as const, icon: <Percent size={15} />, active: st.numFmt === 'percent', onClick: () => setNumFmt('percent'), tooltip: t('sheet_format_percent') },
    { id: 'num', kind: 'toggle' as const, icon: <Hash size={15} />, active: st.numFmt === 'number', onClick: () => setNumFmt('number'), tooltip: t('sheet_number_format') },
    { id: 'decless', kind: 'button' as const, icon: <span className="text-[11px] font-mono">.0&lt;</span>, onClick: () => adjustDecimals(-1), tooltip: t('sheet_format_dec_less') },
    { id: 'decmore', kind: 'button' as const, icon: <span className="text-[11px] font-mono">.00&gt;</span>, onClick: () => adjustDecimals(1), tooltip: t('sheet_format_dec_more') },
  ] }
  const namesGroup = { id: 'names', label: t('names_grp', { defaultValue: 'Noms définis' }), items: [
    { id: 'namemgr', kind: 'button' as const, icon: <Tag size={15} />, label: t('names_button', { defaultValue: 'Gestionnaire' }), size: 'large' as const, onClick: () => setNameManagerOpen(true) },
  ] }
  const ribbon: RibbonTab[] = [
    {
      id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
      groups: [
        { id: 'undo', label: t('sheet_grp_undo', { defaultValue: 'Annuler' }), items: [
          { id: 'undo', kind: 'button', icon: <Undo2 size={15} />, label: t('sheet_undo', { defaultValue: 'Annuler' }), disabled: !canUndo, onClick: undo },
          { id: 'redo', kind: 'button', icon: <Redo2 size={15} />, label: t('sheet_redo', { defaultValue: 'Rétablir' }), disabled: !canRedo, onClick: redo },
        ] },
        fileGroup(t, { onNew, onDuplicate }),
        { id: 'clip', label: t('sheet_grp_clipboard', { defaultValue: 'Presse-papiers' }), items: [
          { id: 'paste', kind: 'button', icon: <ClipboardPaste size={18} />, label: t('common_paste', { defaultValue: 'Coller' }), size: 'large', onClick: () => pasteSelection() },
          { id: 'cut', kind: 'button', icon: <Scissors size={15} />, label: t('common_cut', { defaultValue: 'Couper' }), onClick: () => copySelection(true) },
          { id: 'copy', kind: 'button', icon: <Copy size={15} />, label: t('common_copy', { defaultValue: 'Copier' }), onClick: () => copySelection(false) },
          { id: 'painter', kind: 'toggle', icon: <Paintbrush size={15} />, active: painterStyle !== null, label: t('sheet_format_painter', { defaultValue: 'Reproduire la mise en forme' }), tooltip: t('sheet_format_painter', { defaultValue: 'Reproduire la mise en forme' }), onClick: () => setPainterStyle(p => p !== null ? null : { ...selectedCellStyle }) },
        ] },
        fontGroup, alignGroup, numberGroup,
        { id: 'cells', label: t('sheet_grp_cells', { defaultValue: 'Cellules' }), items: [
          { id: 'insertcells', kind: 'custom', render: insertCellsRender },
          { id: 'deletecells', kind: 'custom', render: deleteCellsRender },
          { id: 'borders', kind: 'custom', render: bordersRender },
        ] },
        { id: 'editing', label: t('sheet_grp_editing', { defaultValue: 'Édition' }), items: [
          { id: 'sum', kind: 'button', icon: <Sigma size={15} />, label: t('sheet_autosum', { defaultValue: 'Somme' }), onClick: autoSum },
          { id: 'clearfmt', kind: 'button', icon: <Eraser size={15} />, label: t('sheet_clear_formats', { defaultValue: 'Effacer format' }), onClick: clearFormats },
          { id: 'filter', kind: 'toggle', icon: <Filter size={15} />, label: t('sheet_filter', 'Filtrer'), active: filterMode || Object.keys(colFilters).length > 0, onClick: () => setFilterMode(m => !m) },
          { id: 'find', kind: 'button', icon: <Search size={15} />, label: t('sheet_find_replace', { defaultValue: 'Rechercher' }), onClick: () => setFrOpen('find') },
        ] },
      ],
    },
    {
      id: 'insert', label: t('sheet_tab_insert', { defaultValue: 'Insertion' }),
      groups: [
        { id: 'tables', label: t('sheet_grp_tables', { defaultValue: 'Tableaux' }), items: [
          { id: 'pivot', kind: 'button', icon: <Table2 size={18} />, label: t('sheet_pivot', { defaultValue: 'Tableau croisé dynamique' }), size: 'large', onClick: soon },
          { id: 'table', kind: 'button', icon: <Grid2x2 size={18} />, label: t('sheet_table', { defaultValue: 'Tableau' }), size: 'large', onClick: soon },
        ] },
        { id: 'illus', label: t('sheet_grp_illus', { defaultValue: 'Illustrations' }), items: [
          { id: 'image', kind: 'button', icon: <ImagePlus size={18} />, label: t('sheet_insert_image', { defaultValue: 'Images' }), size: 'large', onClick: insertImageFromFile },
          { id: 'shapes', kind: 'button', icon: <Shapes size={15} />, label: t('sheet_shapes', { defaultValue: 'Formes' }), onClick: soon },
          { id: 'icons', kind: 'button', icon: <Smile size={15} />, label: t('sheet_icons', { defaultValue: 'Icônes' }), onClick: soon },
          { id: 'smartart', kind: 'button', icon: <Workflow size={15} />, label: t('sheet_smartart', { defaultValue: 'SmartArt' }), onClick: soon },
        ] },
        { id: 'charts', label: t('sheet_grp_charts', { defaultValue: 'Graphiques' }), items: [
          { id: 'chartreco', kind: 'button', icon: <BarChart3 size={18} />, label: t('sheet_chart_recommended', { defaultValue: 'Graphiques recommandés' }), size: 'large', onClick: () => setChartDialog({ type: 'bar' }) },
          { id: 'chartcol', kind: 'button', icon: <BarChart3 size={15} />, label: t('sheet_chart_bar', { defaultValue: 'Histogramme' }), onClick: () => setChartDialog({ type: 'bar' }) },
          { id: 'chartline', kind: 'button', icon: <LineChartIcon size={15} />, label: t('sheet_chart_line', { defaultValue: 'Courbe' }), onClick: () => setChartDialog({ type: 'line' }) },
          { id: 'chartpie', kind: 'button', icon: <PieChartIcon size={15} />, label: t('sheet_chart_pie', { defaultValue: 'Secteurs' }), onClick: () => setChartDialog({ type: 'pie' }) },
          { id: 'charthbar', kind: 'button', icon: <BarChartHorizontal size={15} />, label: t('sheet_chart_hbar', { defaultValue: 'Barres' }), onClick: () => setChartDialog({ type: 'hbar' }) },
          { id: 'chartscatter', kind: 'button', icon: <ScatterChartIcon size={15} />, label: t('sheet_chart_scatter', { defaultValue: 'Nuage de points' }), onClick: () => setChartDialog({ type: 'scatter' }) },
        ] },
        { id: 'sparkline', label: t('sheet_grp_sparkline', { defaultValue: 'Graphiques sparkline' }), items: [
          { id: 'spline', kind: 'button', icon: <Activity size={15} />, label: t('sheet_spark_line', { defaultValue: 'Courbe' }), onClick: soon },
          { id: 'spcol', kind: 'button', icon: <BarChart3 size={15} />, label: t('sheet_spark_col', { defaultValue: 'Histogramme' }), onClick: soon },
        ] },
        { id: 'links', label: t('sheet_grp_links', { defaultValue: 'Liens' }), items: [
          { id: 'link', kind: 'button', icon: <ExternalLink size={18} />, label: t('sheet_link', { defaultValue: 'Lien' }), size: 'large', onClick: soon },
        ] },
        { id: 'text', label: t('sheet_grp_text', { defaultValue: 'Texte' }), items: [
          { id: 'textbox', kind: 'button', icon: <Type size={15} />, label: t('sheet_textbox', { defaultValue: 'Zone de texte' }), onClick: soon },
          { id: 'wordart', kind: 'button', icon: <span className="text-base font-bold italic" style={{ color: '#1a73e8' }}>A</span>, label: t('sheet_wordart', { defaultValue: 'WordArt' }), onClick: soon },
        ] },
        { id: 'symbols', label: t('sheet_grp_symbols', { defaultValue: 'Symboles' }), items: [
          { id: 'equation', kind: 'custom', render: equationRender },
          { id: 'symbol', kind: 'custom', render: symbolsRender },
        ] },
      ],
    },
    {
      id: 'layout', label: t('sheet_tab_layout', { defaultValue: 'Mise en page' }),
      groups: [
        { id: 'sheetopt', label: t('sheet_grp_sheet_options', { defaultValue: 'Options de la feuille' }), items: [
          { id: 'gridlines_l', kind: 'toggle', icon: <Grid3x3 size={15} />, label: t('sheet_gridlines_short', { defaultValue: 'Quadrillage' }), size: 'large', active: showGridlines, onClick: toggleGridlines },
          { id: 'freeze_l', kind: 'custom', render: freezeRender },
        ] },
      ],
    },
    {
      id: 'formulas', label: t('sheet_tab_formulas', { defaultValue: 'Formules' }),
      groups: [
        { id: 'fnlib', label: t('sheet_grp_fn_lib', { defaultValue: 'Bibliothèque de fonctions' }), items: [
          { id: 'sum3', kind: 'button', icon: <Sigma size={18} />, label: t('sheet_autosum', { defaultValue: 'Somme auto' }), size: 'large', onClick: autoSum },
          { id: 'fx2', kind: 'button', icon: <span className="text-[13px] italic font-serif">fx</span>, label: t('sheet_insert_function', { defaultValue: 'Fonction' }), size: 'large', onClick: insertFunction },
        ] },
        namesGroup,
      ],
    },
    {
      id: 'data', label: t('sheet_tab_data', { defaultValue: 'Données' }),
      groups: [
        { id: 'sortfilter', label: t('sheet_grp_sort_filter', { defaultValue: 'Trier et filtrer' }), items: [
          { id: 'sortasc', kind: 'button', icon: <ArrowUpAZ size={18} />, label: t('sheet_sort_asc', { defaultValue: 'Croissant' }), size: 'large', tooltip: t('sheet_sort_asc', { defaultValue: 'Croissant' }), onClick: () => sortRange(true) },
          { id: 'sortdesc', kind: 'button', icon: <ArrowDownAZ size={18} />, label: t('sheet_sort_desc', { defaultValue: 'Décroissant' }), size: 'large', tooltip: t('sheet_sort_desc', { defaultValue: 'Décroissant' }), onClick: () => sortRange(false) },
          { id: 'filter2', kind: 'toggle', icon: <Filter size={15} />, label: t('sheet_filter', 'Filtrer'), active: filterMode || Object.keys(colFilters).length > 0, onClick: () => setFilterMode(m => !m) },
        ] },
        { id: 'datatools', label: t('sheet_grp_data_tools', { defaultValue: 'Outils de données' }), items: [
          { id: 'dedup', kind: 'button', icon: <CopyMinus size={18} />, size: 'large', label: t('sheet_remove_dup', { defaultValue: 'Supprimer les doublons' }), onClick: removeDuplicates },
        ] },
        namesGroup,
      ],
    },
    {
      id: 'review', label: t('sheet_tab_review', { defaultValue: 'Révision' }),
      groups: [
        { id: 'changes', label: t('sheet_grp_changes', { defaultValue: 'Modifications' }), items: [
          { id: 'clearfmt2', kind: 'button', icon: <Eraser size={15} />, label: t('sheet_clear_formats', { defaultValue: 'Effacer format' }), onClick: clearFormats },
          { id: 'clearall', kind: 'button', icon: <Trash2 size={15} />, label: t('sheet_clear_contents', { defaultValue: 'Effacer contenu' }), onClick: clearSelection },
        ] },
      ],
    },
    {
      id: 'view', label: t('sheet_tab_view', { defaultValue: 'Affichage' }),
      groups: [
        { id: 'show', label: t('sheet_grp_show', { defaultValue: 'Afficher' }), items: [
          { id: 'gridlines', kind: 'toggle', icon: <Grid3x3 size={15} />, label: t('sheet_gridlines_short', { defaultValue: 'Quadrillage' }), size: 'large', active: showGridlines, onClick: toggleGridlines },
          { id: 'freeze', kind: 'custom', render: freezeRender },
        ] },
        { id: 'macrosg', label: t('sheet_grp_macros', { defaultValue: 'Macros' }), items: [
          { id: 'macros', kind: 'custom', render: macrosRender },
        ] },
      ],
    },
    // Contextual tab — shown only while a picture is selected (Excel "Format de l'image").
    {
      id: 'image', label: t('sheet_tab_image', { defaultValue: 'Image' }),
      contextual: { accent: '#1a73e8', groupLabel: t('sheet_img_tools', { defaultValue: 'Outils Image' }) },
      visible: selectedImage != null,
      groups: [
        { id: 'img_crop', label: t('sheet_grp_crop', { defaultValue: 'Rogner' }), items: [
          { id: 'crop', kind: 'toggle' as const, icon: <Crop size={15} />, label: t('sheet_img_crop', { defaultValue: 'Rogner' }), size: 'large' as const, active: cropMode === selectedImage, onClick: () => setCropMode(cropMode === selectedImage ? null : selectedImage) },
        ] },
        { id: 'img_rotate', label: t('sheet_grp_rotate', { defaultValue: 'Pivoter' }), items: [
          { id: 'rotl', kind: 'button' as const, icon: <RotateCcw size={15} />, label: t('sheet_img_rot_left', { defaultValue: 'Gauche 90°' }), onClick: () => rotateSelectedImage(-90) },
          { id: 'rotr', kind: 'button' as const, icon: <RotateCw size={15} />, label: t('sheet_img_rot_right', { defaultValue: 'Droite 90°' }), onClick: () => rotateSelectedImage(90) },
        ] },
        { id: 'img_order', label: t('sheet_grp_order', { defaultValue: 'Ordre' }), items: [
          { id: 'front', kind: 'button' as const, icon: <BringToFront size={15} />, label: t('sheet_img_front', { defaultValue: 'Avancer' }), onClick: () => reorderSelectedImage(true) },
          { id: 'back', kind: 'button' as const, icon: <SendToBack size={15} />, label: t('sheet_img_back', { defaultValue: 'Reculer' }), onClick: () => reorderSelectedImage(false) },
        ] },
        { id: 'img_edit', label: t('sheet_grp_img_edit', { defaultValue: 'Édition' }), items: [
          { id: 'reset', kind: 'button' as const, icon: <RefreshCw size={15} />, label: t('sheet_img_reset', { defaultValue: 'Réinitialiser' }), size: 'large' as const, onClick: resetSelectedImage },
          { id: 'del', kind: 'button' as const, icon: <ImageOff size={15} />, label: t('sheet_img_delete', { defaultValue: 'Supprimer' }), size: 'large' as const, onClick: deleteSelectedImage },
        ] },
      ],
    },
  ]
  // Le ruban est reconstruit à chaque rendu (closures fraîches), mais on ne le REMONTE
  // que quand son contenu VISIBLE change — sinon boucle de rendu infinie avec le parent.
  const ribbonSig = JSON.stringify([st.bold, st.italic, st.underline, st.strike, st.align, st.numFmt, st.fontFamily, st.fontSize, st.wrap, st.color, st.bg, !!st.bgGradient, textColorOpen, fillOpen, bordersOpen, freezeOpen, insertCellsOpen, deleteCellsOpen, symbolsOpen, equationOpen, frozenRows, frozenCols, filterMode, Object.keys(colFilters).length, Object.keys(definedNames).length, selectionHasMerge, showGridlines, selectedCell?.col, selectedCell?.row, rangeEnd?.col, rangeEnd?.row, sheetMetas.length, selectedImage, cropMode, canUndo, canRedo, painterStyle !== null])
  const lastRibbonSig = useRef('')
  useEffect(() => {
    if (ribbonSig !== lastRibbonSig.current) { lastRibbonSig.current = ribbonSig; onRibbonChange?.(ribbon) }
  })

  return (
    <div
      className="flex flex-col h-full bg-white select-none"
      style={{ cursor: resizeCursor }}
      onMouseMove={onGridMouseMove}
      onMouseUp={onGridMouseUp}
      // Empêche le ContextMenuProvider du core de capturer (et casser) le clic droit
      // du tableur ; la grille montre son propre menu, le reste garde le menu natif.
      onContextMenu={e => e.stopPropagation()}
    >
      {cellMenu && <MenuDropdown items={cellMenuItems} pos={{ top: cellMenu.y, left: cellMenu.x }} onClose={() => setCellMenu(null)} />}

      {/* Formula bar */}
      <div className="relative flex-shrink-0">
        <div className="flex items-center border-b border-[#e2e4e6] bg-white" style={{ height: 26 }}>
          <input
            ref={nameBoxRef}
            value={nameBox}
            title={t('sheet_name_box', { defaultValue: 'Zone Nom — aller à une cellule (ex. A1, XFD1048576)' })}
            className="flex items-center text-center border-r border-[#e2e4e6] text-xs text-text-secondary font-mono bg-white flex-shrink-0 outline-none focus:bg-[#e8f0fe]"
            style={{ width: 80, height: '100%' }}
            onChange={e => setNameBox(e.target.value)}
            onFocus={e => { nameBoxFocused.current = true; e.currentTarget.select() }}
            onBlur={() => { nameBoxFocused.current = false; setNameBox(cellAddressLabel) }}
            onKeyDown={e => {
              if (e.key === 'Enter') { if (jumpToRef(nameBox)) e.currentTarget.blur(); e.preventDefault() }
              else if (e.key === 'Escape') { setNameBox(cellAddressLabel); e.currentTarget.blur() }
              e.stopPropagation()
            }}
          />
          <div className="flex items-center justify-center border-r border-[#e2e4e6] flex-shrink-0 px-2 text-xs italic text-text-tertiary" style={{ height: '100%' }}>
            fx
          </div>
          <FormulaInput
            inputRef={formulaBarRef}
            knownFunctions={FN_NAMES}
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

      {/* Grid (canvas-rendered) */}
      <div
        ref={gridRef}
        className="flex-1 overflow-auto relative"
        style={{ fontFamily: 'Arial, sans-serif', fontSize: 13, cursor: resizeCursor }}
        onScroll={e => { trackViewEnd(e.currentTarget); drawGrid(); drawSelection() }}
        onMouseMove={handleGridMouseMove}
        onMouseDown={handleGridMouseDown}
        onClick={handleGridClick}
        onDoubleClick={handleGridDoubleClick}
        onContextMenu={onGridContextMenu}
        onMouseLeave={() => { publishCursor(null); if (hoverEdge) setHoverEdge(null) }}
      >
        {sheetQuery.isLoading ? (
          <div className="flex items-center justify-center h-full text-text-tertiary text-sm">{t('common_loading')}</div>
        ) : (
          // The sized spacer drives the native scrollbars; the canvas is `sticky` so
          // it stays pinned to the viewport while overlays scroll in content coords.
          // Taille = étendue DYNAMIQUE (plage utilisée + marge, croît au défilement) →
          // barre de défilement utilisable façon Excel, pas un thumb microscopique.
          <div style={{ position: 'relative', width: geom.colLeft[extentCols], height: geom.rowYof(extentRows + 1) }}>
            <canvas
              ref={canvasRef}
              style={{ position: 'sticky', top: 0, left: 0, zIndex: 1, pointerEvents: 'none' }}
            />
            {/* Selection overlay — transparent, stacked above floating objects (z > equations/
                charts) so the selection shows over them; still below the fill handle (z22). Kept
                in the viewport via a transform updated on scroll (two stacked `sticky` canvases
                would offset each other in flow). */}
            <canvas
              ref={selCanvasRef}
              style={{ position: 'absolute', top: 0, left: 0, zIndex: 20, pointerEvents: 'none' }}
            />

            {/* Inline editor — only the edited cell needs a real DOM input */}
            {editingCell && (() => {
              const c = COLS.indexOf(editingCell.col)
              if (c < 0) return null
              return (
                <CellEditor
                  col={editingCell.col}
                  row={editingCell.row}
                  left={geom.colLeft[c]}
                  top={geom.rowTop[editingCell.row]}
                  width={geom.colLeft[c + 1] - geom.colLeft[c]}
                  height={geom.rowTop[editingCell.row + 1] - geom.rowTop[editingCell.row]}
                  fontSize={(styleAt(editingCell.col, editingCell.row, sheetData.cells[cellKey(editingCell.col, editingCell.row)]?.s).fontSize ?? 13) * zoom}
                  value={cellDraft}
                  onChange={(v, el) => { setCellDraft(v); refreshAssist(v, el.selectionStart ?? v.length, el, setCellDraft) }}
                  onSelect={el => refreshAssist(el.value, el.selectionStart ?? el.value.length, el, setCellDraft)}
                  onCommit={handleEditCommit}
                  onAbort={handleEditAbort}
                  assistKeyDown={assistKeyDown}
                  onArrow={moveSelection}
                  onTab={handleTab}
                />
              )
            })()}

            {/* La poignée de recopie est dessinée sur le canvas overlay (drawSelection) pour
                rester collée au remplissage/bordure animés ; son clic est géré par hit-test. */}

            {/* Objets équation (LaTeX → KaTeX) flottant au-dessus de la grille. */}
            {localEquations.map(eq => {
              const left = ROW_HEADER_WIDTH + eq.bx * zoom, top = COL_HEADER_HEIGHT + eq.by * zoom
              return (
                <div
                  key={eq.id}
                  onMouseDown={e => startEqDrag(eq, e)}
                  onDoubleClick={e => { e.stopPropagation(); e.preventDefault(); setSelectedEq(eq.id); setEditingEq({ id: eq.id, latex: eq.latex }) }}
                  onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setSelectedEq(eq.id); setEqMenu({ x: e.clientX, y: e.clientY, id: eq.id }) }}
                  title={t('sheet_equation_hint', { defaultValue: 'Double-cliquez pour modifier' })}
                  style={{
                    position: 'absolute', left, top, zIndex: 8, cursor: 'move',
                    padding: 4, borderRadius: 4, background: 'white',
                    border: selectedEq === eq.id ? '1.5px solid #1a73e8' : '1px solid #e2e4e6',
                    boxShadow: '0 1px 3px rgba(0,0,0,0.08)',
                  }}
                >
                  <div style={{ fontSize: 18 * zoom, pointerEvents: 'none' }} dangerouslySetInnerHTML={{ __html: renderKatex(eq.latex) }} />
                </div>
              )
            })}

            {/* Objets graphique (SVG calculé depuis une plage) flottant sur la grille. */}
            {localCharts.map(chart => {
              const r = chartRect(chart)
              const left = r.x, top = r.y, w = r.w, h = r.h
              const sel = selectedChart === chart.id
              return (
                <div
                  key={chart.id}
                  onMouseDown={e => startChartDrag(chart, 'move', e)}
                  onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setSelectedChart(chart.id ?? null); setChartMenu({ x: e.clientX, y: e.clientY, id: chart.id! }) }}
                  style={{
                    position: 'absolute', left, top, width: w, height: h, zIndex: 7, cursor: 'move',
                    background: 'white', borderRadius: 4, padding: 6,
                    border: sel ? '1.5px solid #1a73e8' : '1px solid #e2e4e6',
                    boxShadow: '0 1px 4px rgba(0,0,0,0.1)', boxSizing: 'border-box',
                  }}
                >
                  <div style={{ width: '100%', height: '100%', pointerEvents: 'none' }}>{renderChart(chart)}</div>
                  {sel && (
                    <div
                      onMouseDown={e => startChartDrag(chart, 'se', e)}
                      style={{ position: 'absolute', right: -5, bottom: -5, width: 11, height: 11, background: '#1a73e8', border: '1.5px solid white', borderRadius: 2, cursor: 'nwse-resize' }}
                    />
                  )}
                </div>
              )
            })}

            {/* Encadrés colorés des plages référencées par la formule en édition */}
            {refHighlights.map((h, i) => (
              <div
                key={i}
                style={{
                  position: 'absolute', left: h.left, top: h.top, width: h.width, height: h.height,
                  border: `2px solid ${h.color}`, background: `${h.color}14`,
                  pointerEvents: 'none', zIndex: 5, boxSizing: 'border-box',
                }}
              />
            ))}

            {/* Sélections des autres participants (présence collaborative) */}
            {remoteSelections.map((s, i) => (
              <div
                key={`sel${i}`}
                style={{
                  position: 'absolute', left: s.left, top: s.top, width: s.width, height: s.height,
                  border: `2px solid ${s.color}`, pointerEvents: 'none', zIndex: 6, boxSizing: 'border-box',
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
        )}
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

      {/* ── Barre de statut (réf. cellule, agrégats type Excel, feuille active) ── */}
      {(() => {
        const b = bounds()
        const multi = b && (b.c1 !== b.c2 || b.r1 !== b.r2)
        const refLabel = b
          ? (multi ? `${COLS[b.c1]}${b.r1}:${COLS[b.c2]}${b.r2}` : `${COLS[b.c1]}${b.r1}`)
          : '—'
        const agg = selAgg
        const dims = b ? `${b.r2 - b.r1 + 1}L × ${b.c2 - b.c1 + 1}C` : ''
        const sheetIdx = sheetMetas.findIndex(m => m.id === activeSheetId)
        const fmt = (n: number) => n.toLocaleString(i18n.language, { maximumFractionDigits: 2 })
        return (
          <StatusBar>
            <StatusButton title={t('sheet_status_cell', { defaultValue: 'Cellule active' })}>{refLabel}</StatusButton>
            {multi && <><StatusSep /><StatusButton title={t('sheet_status_dims', { defaultValue: 'Dimensions de la sélection' })}>{dims}</StatusButton></>}
            {agg && agg.num >= 1 && (
              <>
                <StatusSep />
                <StatusButton title={t('sheet_status_avg', { defaultValue: 'Moyenne' })}>{t('sheet_status_avg', { defaultValue: 'Moyenne' })} : {fmt(agg.avg)}</StatusButton>
                <StatusButton title={t('sheet_status_count', { defaultValue: 'Nombre' })}>{t('sheet_status_count', { defaultValue: 'Nombre' })} : {agg.count}</StatusButton>
                <StatusButton title={t('sheet_status_sum', { defaultValue: 'Somme' })}>{t('sheet_status_sum', { defaultValue: 'Somme' })} : {fmt(agg.sum)}</StatusButton>
                {agg.num >= 2 && <>
                  <StatusButton title={t('sheet_status_min', { defaultValue: 'Min' })}>{t('sheet_status_min', { defaultValue: 'Min' })} : {fmt(agg.min)}</StatusButton>
                  <StatusButton title={t('sheet_status_max', { defaultValue: 'Max' })}>{t('sheet_status_max', { defaultValue: 'Max' })} : {fmt(agg.max)}</StatusButton>
                </>}
              </>
            )}
            <StatusSpacer />
            <StatusButton title={t('sheet_status_sheet', { defaultValue: 'Feuille active' })}>
              {t('sheet_status_sheet_n', { current: sheetIdx >= 0 ? sheetIdx + 1 : 1, total: sheetMetas.length, defaultValue: `Feuille ${sheetIdx >= 0 ? sheetIdx + 1 : 1} / ${sheetMetas.length}` })}
            </StatusButton>
            <StatusSep />
            <StatusZoom zoom={zoom} onZoom={setZoom} />
          </StatusBar>
        )
      })()}

      {/* Popup de filtre de colonne */}
      {filterPopup && (() => {
        const seen = new Set<string>(); const vals: string[] = []
        // Borné à la plage utilisée (les lignes vides au-delà n'apportent aucune valeur).
        for (let r = 1; r <= usedBounds.maxRow; r++) { const v = cellText(COLS[filterPopup.col], r); if (!seen.has(v)) { seen.add(v); vals.push(v) } }
        vals.sort((a, b) => a.localeCompare(b, i18n.language, { numeric: true }))
        return (
          <ColumnFilterPopup
            x={filterPopup.x} y={filterPopup.y}
            values={vals}
            initialAllowed={colFilters[filterPopup.col] ?? null}
            t={t}
            onClose={() => setFilterPopup(null)}
            onApply={(allowed) => setColFilters(prev => {
              const next = { ...prev }
              if (allowed === null) delete next[filterPopup.col]
              else next[filterPopup.col] = allowed
              return next
            })}
          />
        )
      })()}

      {nameManagerOpen && (
        <NameManagerDialog
          names={definedNames}
          selectionRef={cellAddressLabel || undefined}
          onSet={setDefinedName}
          onDelete={deleteDefinedName}
          onClose={() => setNameManagerOpen(false)}
        />
      )}

      {/* Menu contextuel d'un graphique */}
      {chartMenu && <MenuDropdown items={[
        { type: 'action', label: t('sheet_chart_t_bar', { defaultValue: 'Histogramme' }), onClick: () => setChartType(chartMenu.id, 'bar') },
        { type: 'action', label: t('sheet_chart_t_hbar', { defaultValue: 'Barres' }), onClick: () => setChartType(chartMenu.id, 'hbar') },
        { type: 'action', label: t('sheet_chart_t_line', { defaultValue: 'Courbe' }), onClick: () => setChartType(chartMenu.id, 'line') },
        { type: 'action', label: t('sheet_chart_t_pie', { defaultValue: 'Secteurs' }), onClick: () => setChartType(chartMenu.id, 'pie') },
        { type: 'action', label: t('sheet_chart_t_scatter', { defaultValue: 'Nuage de points' }), onClick: () => setChartType(chartMenu.id, 'scatter') },
        { type: 'separator' },
        { type: 'action', label: t('sheet_chart_delete', { defaultValue: 'Supprimer le graphique' }), danger: true, onClick: () => deleteChart(chartMenu.id) },
      ]} pos={{ top: chartMenu.y, left: chartMenu.x }} onClose={() => setChartMenu(null)} />}

      {/* Menu contextuel d'une équation */}
      {eqMenu && <MenuDropdown items={[
        { type: 'action', label: t('sheet_eq_edit', { defaultValue: 'Modifier l’équation' }), onClick: () => { const e = localEquationsRef.current.find(x => x.id === eqMenu.id); if (e) setEditingEq({ id: e.id, latex: e.latex }) } },
        { type: 'separator' },
        { type: 'action', label: t('sheet_eq_delete', { defaultValue: 'Supprimer l’équation' }), danger: true, onClick: () => deleteEquation(eqMenu.id) },
      ]} pos={{ top: eqMenu.y, left: eqMenu.x }} onClose={() => setEqMenu(null)} />}

      {/* Éditeur d'équation (LaTeX + aperçu KaTeX) */}
      {editingEq && (
        <FloatingWindow title={<span className="flex items-center gap-2"><span className="font-serif italic">∑</span> {t('sheet_equation', { defaultValue: 'Équation' })}</span>} onClose={() => setEditingEq(null)} defaultWidth={560} backdrop>
          <div className="p-4 space-y-3">
            <div className="border border-border rounded p-4 min-h-[64px] flex items-center justify-center bg-[#fafafa] overflow-auto" dangerouslySetInnerHTML={{ __html: renderKatex(editingEq.latex) }} />
            <LatexEditor value={editingEq.latex} onChange={v => setEditingEq(e => e ? { ...e, latex: v } : e)} />
            <div className="flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setEditingEq(null)}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
              <Button onClick={() => { persistEquations(localEquationsRef.current.map(x => x.id === editingEq.id ? { ...x, latex: editingEq.latex } : x)); setEditingEq(null) }}>{t('common_validate', { defaultValue: 'Valider' })}</Button>
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* Rechercher & Remplacer */}
      {frOpen && (
        <FloatingWindow title={frOpen === 'replace' ? t('sheet_fr_replace_title', { defaultValue: 'Rechercher et remplacer' }) : t('sheet_fr_find_title', { defaultValue: 'Rechercher' })} onClose={() => setFrOpen(false)} defaultWidth={420}>
          <div className="p-4 space-y-3 text-sm">
            <div className="flex items-center gap-2">
              <label className="w-20 text-text-secondary">{t('sheet_fr_find', { defaultValue: 'Rechercher' })}</label>
              <input autoFocus value={frFind} onChange={e => setFrFind(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') frFindNext(); e.stopPropagation() }} className="flex-1 h-8 px-2 border border-[#dadce0] rounded outline-none focus:border-primary" />
            </div>
            {frOpen === 'replace' && (
              <div className="flex items-center gap-2">
                <label className="w-20 text-text-secondary">{t('sheet_fr_with', { defaultValue: 'Remplacer par' })}</label>
                <input value={frReplace} onChange={e => setFrReplace(e.target.value)} onKeyDown={e => e.stopPropagation()} className="flex-1 h-8 px-2 border border-[#dadce0] rounded outline-none focus:border-primary" />
              </div>
            )}
            <label className="flex items-center gap-2 text-text-secondary"><input type="checkbox" checked={frCase} onChange={e => setFrCase(e.target.checked)} /> {t('sheet_fr_case', { defaultValue: 'Respecter la casse' })}</label>
            <div className="flex justify-end gap-2 pt-1">
              <Button variant="ghost" onClick={() => frFindNext(true)}>{t('sheet_fr_prev', { defaultValue: 'Précédent' })}</Button>
              <Button variant="ghost" onClick={() => frFindNext()}>{t('sheet_fr_next', { defaultValue: 'Suivant' })}</Button>
              {frOpen === 'replace' && <Button variant="ghost" onClick={frReplaceOne}>{t('sheet_fr_replace_one', { defaultValue: 'Remplacer' })}</Button>}
              {frOpen === 'replace' && <Button onClick={frReplaceAll}>{t('sheet_fr_replace_all', { defaultValue: 'Tout remplacer' })}</Button>}
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* Fenêtre d'insertion de graphique (façon Excel « Insérer un graphique ») */}
      {chartDialog && (() => {
        const range = chartSourceRange()
        const TYPES: { type: ChartType; label: string; icon: React.ReactNode }[] = [
          { type: 'bar', label: t('sheet_chart_bar', { defaultValue: 'Histogramme' }), icon: <BarChart3 size={16} /> },
          { type: 'hbar', label: t('sheet_chart_hbar', { defaultValue: 'Barres' }), icon: <BarChartHorizontal size={16} /> },
          { type: 'line', label: t('sheet_chart_line', { defaultValue: 'Courbe' }), icon: <LineChartIcon size={16} /> },
          { type: 'area', label: t('sheet_chart_area', { defaultValue: 'Aires' }), icon: <Activity size={16} /> },
          { type: 'pie', label: t('sheet_chart_pie', { defaultValue: 'Secteurs' }), icon: <PieChartIcon size={16} /> },
          { type: 'scatter', label: t('sheet_chart_scatter', { defaultValue: 'Nuage de points' }), icon: <ScatterChartIcon size={16} /> },
        ]
        const sel = chartDialog.type
        return (
          <FloatingWindow title={<span className="flex items-center gap-2"><BarChart3 size={16} /> {t('sheet_chart_insert_title', { defaultValue: 'Insérer un graphique' })}</span>} onClose={() => setChartDialog(null)} defaultWidth={680} backdrop>
            <div className="flex" style={{ height: 380 }}>
              <div className="w-48 border-r border-border overflow-auto py-1 flex-shrink-0">
                {TYPES.map(ty => (
                  <button key={ty.type} onClick={() => setChartDialog({ type: ty.type })}
                    className={`w-full text-left px-3 py-2 flex items-center gap-2.5 text-sm ${sel === ty.type ? 'bg-[#e8f0fe] text-primary font-medium' : 'hover:bg-[#f1f3f4]'}`}>
                    {ty.icon} {ty.label}
                  </button>
                ))}
              </div>
              <div className="flex-1 p-4 flex flex-col min-w-0">
                <div className="text-[13px] font-semibold mb-2">{TYPES.find(x => x.type === sel)?.label}</div>
                <div className="flex-1 border border-border rounded bg-white p-2 min-h-0">{renderChart({ id: 'preview', type: sel, range })}</div>
                <div className="flex justify-end gap-2 mt-3">
                  <Button variant="ghost" onClick={() => setChartDialog(null)}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
                  <Button onClick={() => { insertChart(sel); setChartDialog(null) }}>{t('common_insert', { defaultValue: 'Insérer' })}</Button>
                </div>
              </div>
            </div>
          </FloatingWindow>
        )
      })()}
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
  // Ruban construit par l'éditeur (où vivent les actions) et remonté ici pour OfficeShell.
  const [editorRibbon, setEditorRibbon] = useState<RibbonTab[]>([])
  const [ribbonTab, setRibbonTab] = useState('home')

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
        ribbon={editorRibbon.length ? editorRibbon : [{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
          groups: [fileGroup(t, { onNew: () => createMut.mutate(), onDuplicate: () => duplicateMut.mutate(id) })] }]}
        activeTabId={ribbonTab}
        onTabChange={setRibbonTab}
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
              onRibbonChange={setEditorRibbon}
              onNew={() => createMut.mutate()}
              onDuplicate={() => duplicateMut.mutate(id)}
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
