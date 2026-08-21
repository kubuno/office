import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { Plus, Minus, Sigma, Trash2, ExternalLink, Copy, Code2, MousePointerSquareDashed, Palette, ChevronDown, Check, ArrowUp, ArrowDown, LineChart, RotateCw, X as XIcon, FilePlus, CopyPlus, Star, Search, Type, AlignLeft, AlignCenter, AlignRight, ListOrdered, Heading1, Heading2, Grid3x3, Maximize2, Calculator, Sparkles, Target, Table, Expand, Parentheses, AreaChart, Spline, Baseline, Waypoints, Package } from 'lucide-react'
import type { RibbonTab, RibbonItem } from './ribbon/types'
import katex from 'katex'
import 'katex/dist/katex.min.css'
import { Button, Dropdown, MenuDropdown, ColorField, Checkbox, useSaveShortcut } from '@ui'
import type { StartPageRecentItem, MenuItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import { ModuleHome, useFileTab, backstageLabels, BackstageInfo } from './ribbon/ModuleBackstage'
import { useOpenError } from './ribbon/useOpenError'
import type { FileItem } from '@kubuno/drive'
import { getDateLocale } from '@kubuno/sdk'
import { useDebouncedAutosave } from '@kubuno/sdk'
import { useOfficeInstance } from './useOfficeInstance'
import { DockArea, WORKSPACE_LIGHT } from '@kubuno/sdk'
import type { DockPanel } from '@kubuno/sdk'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { UndoRedoButtons } from './ribbon/UndoRedoButtons'
import { useSnapshotHistory } from './ribbon/useSnapshotHistory'
import { StatusBar, StatusSep, StatusSpacer } from './shell/StatusBar'
import { THEME_MATHS } from './ribbon/officeThemes'
import { formulasApi, type MathFormula } from './maths-api'
import { MATH_CATEGORIES, CARET, type MathTemplate } from './mathSymbols'
import LatexEditor from './LatexEditor'
import { renderLatex, isLatexDocument } from './latexRender'
import { MacrosMenu } from './macros/MacrosMenu'
import { appPrompt } from './macros/FormRuntime'
import { parseDoc, serializeDoc, PAGE_FORMATS, paperPx, DEFAULT_FORMAT, SHEET_PAD, type MathBlock, type Orientation, type BlockAlign, type TextStyle, type BlockFrame } from './mathPages'
import { defaultGraphSpec, GRAPH_COLORS, type GraphSpec } from './mathGraph'
import GraphBlock from './GraphBlock'
import { genericClipboardGroup } from './ribbon/clipboardGroup'
import MathTreeEditor, { type MathEditorHandle } from './MathTreeEditor'
import { evalLatex, fmtResult, resultLatex, latexToExpr } from './mathEval'
import { derivativeLatex, simplifyLatex, solveRoots, valueTableLatex, integralLatex, definiteIntegral, expandLatex, factorLatex, taylorLatex, nthDerivativeLatex, tangentAt, extremaLatex, limitLatex, solveExactLatex, inflectionLatex, asymptotesLatex, sumLatex, productLatex, evalPointLatex } from './mathCas'
import { determinantLatex, transposeLatex, inverseLatex, rankLatex, traceLatex, powerLatex, charPolyLatex, eigenvaluesLatex, rrefLatex, solveSystemLatex } from './mathMatrix'
import { statsLatex, fiveNumberLatex, regressionLatex } from './mathStats'
import { primeFactorsLatex, gcdLcmLatex, factorialLatex, binomialLatex, baseConvertLatex } from './mathNumber'
import { printMathDoc } from './mathPrint'
import {
  MATH_THEMES, paletteStyle, resolvePalette,
  loadColorize, saveColorize, loadThemeId, saveThemeId, loadCustomPalette, saveCustomPalette,
  type MathPalette,
} from './mathThemes'
import { copyKubunoData } from './kubunoData'
import { mathEnvelope } from './MathDataCard'

function renderTex(tex: string, displayMode: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode, throwOnError: false, output: 'html', strict: false })
  } catch {
    return ''
  }
}

// A small rendered-KaTeX icon used as ribbon gallery thumbnails (structures/symbols).
const texIcon = (tex: string) => (
  <span className="inline-flex items-center justify-center leading-none" style={{ fontSize: 12 }}
    dangerouslySetInnerHTML={{ __html: renderTex(tex, false) }} />
)

// Insertable structure/symbol, with a visual template (\square holes) and a code
// template (‸ = caret). `p` is the KaTeX preview shown in the ribbon gallery.
type EqItem = { p: string; tex: string; code: string; label: string }
const EQ_STRUCTURES: EqItem[] = [
  { p: '\\frac{a}{b}',   tex: '\\frac{\\square}{\\square}',  code: '\\frac{‸}{}',  label: 'Fraction' },
  { p: '\\dfrac{a}{b}',  tex: '\\dfrac{\\square}{\\square}', code: '\\dfrac{‸}{}', label: 'Grande fraction' },
  { p: '\\sqrt{x}',      tex: '\\sqrt{\\square}',            code: '\\sqrt{‸}',   label: 'Racine carrée' },
  { p: '\\sqrt[n]{x}',   tex: '\\sqrt[\\square]{\\square}',  code: '\\sqrt[‸]{}', label: 'Racine n-ième' },
  { p: 'x^{n}',          tex: '\\square^{\\square}',         code: '^{‸}',        label: 'Exposant' },
  { p: 'x_{n}',          tex: '\\square_{\\square}',         code: '_{‸}',        label: 'Indice' },
  { p: 'x_{n}^{m}',      tex: '\\square_{\\square}^{\\square}', code: '_{‸}^{}',  label: 'Indice + exposant' },
]
const EQ_BIGOPS: EqItem[] = [
  { p: '\\sum',   tex: '\\sum_{\\square}^{\\square}',  code: '\\sum_{‸}^{}',  label: 'Somme' },
  { p: '\\prod',  tex: '\\prod_{\\square}^{\\square}', code: '\\prod_{‸}^{}', label: 'Produit' },
  { p: '\\int',   tex: '\\int_{\\square}^{\\square}',  code: '\\int_{‸}^{}',  label: 'Intégrale' },
  { p: '\\iint',  tex: '\\iint_{\\square}',            code: '\\iint_{‸}',    label: 'Intégrale double' },
  { p: '\\oint',  tex: '\\oint_{\\square}',            code: '\\oint_{‸}',    label: 'Intégrale de contour' },
  { p: '\\lim',   tex: '\\lim_{\\square\\to\\square}', code: '\\lim_{‸\\to}', label: 'Limite' },
  { p: '\\bigcup',  tex: '\\bigcup_{\\square}^{\\square}',  code: '\\bigcup_{‸}^{}',  label: 'Union' },
  { p: '\\bigcap',  tex: '\\bigcap_{\\square}^{\\square}',  code: '\\bigcap_{‸}^{}',  label: 'Intersection' },
]
const EQ_BRACKETS: EqItem[] = [
  { p: '(\\square)',   tex: '\\left(\\square\\right)',  code: '\\left(‸\\right)',  label: 'Parenthèses' },
  { p: '[\\square]',   tex: '\\left[\\square\\right]',  code: '\\left[‸\\right]',  label: 'Crochets' },
  { p: '\\{\\square\\}', tex: '\\left\\{\\square\\right\\}', code: '\\left\\{‸\\right\\}', label: 'Accolades' },
  { p: '|\\square|',   tex: '\\left|\\square\\right|',  code: '\\left|‸\\right|',  label: 'Valeur absolue' },
  { p: '\\|\\square\\|', tex: '\\left\\|\\square\\right\\|', code: '\\left\\|‸\\right\\|', label: 'Norme' },
  { p: '\\binom{n}{k}', tex: '\\binom{\\square}{\\square}', code: '\\binom{‸}{}', label: 'Coefficient binomial' },
]
const EQ_MATRICES: EqItem[] = [
  { p: '\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}', tex: '\\begin{pmatrix}\\square&\\square\\\\\\square&\\square\\end{pmatrix}', code: '\\begin{pmatrix}‸&\\\\&\\end{pmatrix}', label: 'Matrice ( )' },
  { p: '\\begin{bmatrix}a&b\\\\c&d\\end{bmatrix}', tex: '\\begin{bmatrix}\\square&\\square\\\\\\square&\\square\\end{bmatrix}', code: '\\begin{bmatrix}‸&\\\\&\\end{bmatrix}', label: 'Matrice [ ]' },
  { p: '\\begin{vmatrix}a&b\\\\c&d\\end{vmatrix}', tex: '\\begin{vmatrix}\\square&\\square\\\\\\square&\\square\\end{vmatrix}', code: '\\begin{vmatrix}‸&\\\\&\\end{vmatrix}', label: 'Déterminant' },
  { p: '\\begin{cases}a\\\\b\\end{cases}', tex: '\\begin{cases}\\square\\\\\\square\\end{cases}', code: '\\begin{cases}‸\\\\\\end{cases}', label: 'Système (accolade)' },
]
const EQ_ACCENTS: EqItem[] = [
  { p: '\\vec{a}',   tex: '\\vec{\\square}',   code: '\\vec{‸}',   label: 'Vecteur' },
  { p: '\\hat{a}',   tex: '\\hat{\\square}',   code: '\\hat{‸}',   label: 'Chapeau' },
  { p: '\\bar{a}',   tex: '\\bar{\\square}',   code: '\\bar{‸}',   label: 'Barre' },
  { p: '\\overline{ab}', tex: '\\overline{\\square}', code: '\\overline{‸}', label: 'Surligne' },
  { p: '\\underline{ab}', tex: '\\underline{\\square}', code: '\\underline{‸}', label: 'Souligne' },
  { p: '\\dot{a}',   tex: '\\dot{\\square}',   code: '\\dot{‸}',   label: 'Point' },
  { p: '\\tilde{a}', tex: '\\tilde{\\square}', code: '\\tilde{‸}', label: 'Tilde' },
  { p: '\\overbrace{ab}', tex: '\\overbrace{\\square}', code: '\\overbrace{‸}', label: 'Accolade dessus' },
  { p: '\\underbrace{ab}', tex: '\\underbrace{\\square}', code: '\\underbrace{‸}', label: 'Accolade dessous' },
]
// Symbol galleries (single tokens, same insertion in both modes).
const SYM_GREEK = ['\\alpha','\\beta','\\gamma','\\delta','\\epsilon','\\theta','\\lambda','\\mu','\\pi','\\rho','\\sigma','\\phi','\\omega','\\Gamma','\\Delta','\\Theta','\\Lambda','\\Pi','\\Sigma','\\Phi','\\Omega']
const SYM_OPS = ['\\times','\\div','\\pm','\\mp','\\cdot','\\ast','\\circ','\\oplus','\\otimes','\\nabla','\\partial','\\infty','\\forall','\\exists','\\neg']
const SYM_RELS = ['\\leq','\\geq','\\neq','\\approx','\\equiv','\\sim','\\propto','\\in','\\notin','\\subset','\\subseteq','\\cup','\\cap','\\to','\\Rightarrow','\\Leftrightarrow','\\mapsto','\\perp','\\parallel','\\angle']

// Drop the in-session UI id before serialising a block back to storage.
const stripId = (b: MathBlock & { id: string }): MathBlock =>
  b.type === 'graph' ? { type: 'graph', spec: b.spec, frame: b.frame }
  : b.type === 'text' ? { type: 'text', text: b.text, style: b.style, frame: b.frame, scale: b.scale }
  : { type: 'formula', latex: b.latex, align: b.align, numbered: b.numbered, frame: b.frame, scale: b.scale }

// Rough block height used for default placement / sheet growth before the DOM is measured.
const estBlockH = (b: MathBlock): number =>
  b.type === 'graph' ? b.spec.height + 150
  : b.type === 'text' ? ((b.style ?? 'p') === 'h1' ? 64 : b.style === 'h2' ? 52 : 44)
  : (b.frame?.h ?? 190)

// ── Smart guides / magnetism (same behaviour as the presentations editor, in sheet px) ──
type SnapGuide = { axis: 'v' | 'h'; pos: number; a: number; b: number }
type SnapTarget = { pos: number; lo: number; hi: number }
type Box = { x: number; y: number; w: number; h: number }

// Snap candidates (edges + centers) from the sheet bounds and every other block.
function buildSnapTargets(others: Box[], sheetW: number, sheetH: number): { xs: SnapTarget[]; ys: SnapTarget[] } {
  const xs: SnapTarget[] = [
    { pos: 0, lo: 0, hi: sheetH }, { pos: sheetW / 2, lo: 0, hi: sheetH }, { pos: sheetW, lo: 0, hi: sheetH },
  ]
  const ys: SnapTarget[] = [
    { pos: 0, lo: 0, hi: sheetW }, { pos: sheetH / 2, lo: 0, hi: sheetW }, { pos: sheetH, lo: 0, hi: sheetW },
  ]
  for (const b of others) {
    xs.push({ pos: b.x, lo: b.y, hi: b.y + b.h }, { pos: b.x + b.w / 2, lo: b.y, hi: b.y + b.h }, { pos: b.x + b.w, lo: b.y, hi: b.y + b.h })
    ys.push({ pos: b.y, lo: b.x, hi: b.x + b.w }, { pos: b.y + b.h / 2, lo: b.x, hi: b.x + b.w }, { pos: b.y + b.h, lo: b.x, hi: b.x + b.w })
  }
  return { xs, ys }
}

// Align any of the moving box's `edges` on that axis to the nearest target within `thresh`.
function snapAxis(
  edges: { v: number; lo: number; hi: number }[],
  targets: SnapTarget[],
  thresh: number,
  axis: 'v' | 'h',
): { delta: number; guide: SnapGuide } | null {
  let best: { d: number; delta: number; guide: SnapGuide } | null = null
  for (const e of edges) {
    for (const tgt of targets) {
      const d = Math.abs(e.v - tgt.pos)
      if (d > thresh || (best && d >= best.d)) continue
      best = {
        d,
        delta: tgt.pos - e.v,
        guide: { axis, pos: tgt.pos, a: Math.min(e.lo, tgt.lo), b: Math.max(e.hi, tgt.hi) },
      }
    }
  }
  return best
}

// Snap a moving box (edges + centers on both axes); returns corrected origin + guides.
function snapBox(box: Box, targets: { xs: SnapTarget[]; ys: SnapTarget[] }, thresh: number): { x: number; y: number; guides: SnapGuide[] } {
  const guides: SnapGuide[] = []
  const sx = snapAxis(
    [{ v: box.x, lo: box.y, hi: box.y + box.h }, { v: box.x + box.w / 2, lo: box.y, hi: box.y + box.h }, { v: box.x + box.w, lo: box.y, hi: box.y + box.h }],
    targets.xs, thresh, 'v',
  )
  const sy = snapAxis(
    [{ v: box.y, lo: box.x, hi: box.x + box.w }, { v: box.y + box.h / 2, lo: box.x, hi: box.x + box.w }, { v: box.y + box.h, lo: box.x, hi: box.x + box.w }],
    targets.ys, thresh, 'h',
  )
  let { x, y } = box
  if (sx) { x += sx.delta; guides.push(sx.guide) }
  if (sy) { y += sy.delta; guides.push(sy.guide) }
  return { x, y, guides }
}

// Small labelled number input for the Properties dock panel.
function PropNum({ label, value, onChange, placeholder }: { label: string; value: number | ''; onChange: (v: number) => void; placeholder?: string }) {
  return (
    <label className="flex items-center justify-between gap-2 text-xs text-text-secondary">
      <span className="truncate">{label}</span>
      <input type="number" value={value} placeholder={placeholder}
        onChange={e => { const v = parseFloat(e.target.value); if (isFinite(v)) onChange(v) }}
        className="w-20 h-7 px-1.5 text-xs border border-border rounded bg-white text-text-primary flex-shrink-0" />
    </label>
  )
}

// ── Bloc texte (titre / sous-titre / paragraphe) — textarea auto-dimensionnée ────

// Base font sizes (px) for the three text styles; multiplied by the block's content scale.
export const TEXT_STYLE_PX: Record<TextStyle, number> = { h1: 24, h2: 18, p: 14 }

function TextBlockEditor({ text, textStyle, scale, placeholder, onChange }: { text: string; textStyle: TextStyle; scale: number; placeholder: string; onChange: (v: string) => void }) {
  const ref = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const el = ref.current
    if (el) { el.style.height = 'auto'; el.style.height = el.scrollHeight + 'px' }
  }, [text, textStyle, scale])
  const cls = textStyle === 'h1' ? 'font-bold' : textStyle === 'h2' ? 'font-semibold' : ''
  return (
    <textarea ref={ref} value={text} rows={1} placeholder={placeholder}
      onChange={e => onChange(e.target.value)}
      style={{ fontSize: TEXT_STYLE_PX[textStyle] * scale, lineHeight: 1.35 }}
      className={`w-full resize-none outline-none bg-transparent text-[#202124] placeholder:text-text-tertiary ${cls}`} />
  )
}

// ── Bouton de modèle dans la palette (aperçu KaTeX) ─────────────────────────────

function TemplateButton({ tpl, onInsert }: { tpl: MathTemplate; onInsert: (t: MathTemplate) => void }) {
  const html = useMemo(() => renderTex(tpl.tex, false), [tpl.tex])
  return (
    <button
      title={tpl.title ?? tpl.ins.replace(CARET, '')}
      onClick={() => onInsert(tpl)}
      className="flex items-center justify-center h-10 rounded border border-border bg-white
                 hover:bg-primary/5 hover:border-primary/40 transition-colors overflow-hidden px-1"
    >
      <span className="text-[#202124] scale-90" dangerouslySetInnerHTML={{ __html: html }} />
    </button>
  )
}

// ── Vue d'édition d'une formule ─────────────────────────────────────────────────

// Returns true when the source contains a LaTeX expression KaTeX cannot compile.
// We only inspect plain-math sources (the common case for this module): re-rendering
// with `throwOnError: true` surfaces the failure that the preview silently swallows.
function hasLatexError(src: string): boolean {
  const s = src.trim()
  if (!s || isLatexDocument(s)) return false
  try {
    katex.renderToString(s, { displayMode: true, throwOnError: true, output: 'html', strict: false })
    return false
  } catch {
    return true
  }
}

export type HistoryState = { undo: () => void; redo: () => void; canUndo: boolean; canRedo: boolean }
function FormulaEditorView({ formula, onUpdate, formulaCount, saveRef, printRef, onNew, onDuplicate, onRibbonChange, onHistory }: { formula: MathFormula; onUpdate: (f: MathFormula) => void; formulaCount: number; saveRef?: React.MutableRefObject<(() => Promise<void>) | null>; printRef?: React.MutableRefObject<(() => void) | null>; onNew?: () => void; onDuplicate?: () => void; onRibbonChange?: (r: RibbonTab[]) => void; onHistory?: (h: HistoryState) => void }) {
  const { t } = useTranslation('office')
  // A page holds blocks (formula, graph or text) freely placed on the sheet: every UI block
  // carries a `frame` (position/size in sheet px) and a session id for stable React keys.
  // Legacy stacked documents (no frames) get full-width frames stacked like before.
  type UIBlock = MathBlock & { id: string; frame: BlockFrame }
  type UIPage = { name: string; blocks: UIBlock[]; format: string; orientation: Orientation }
  const idc = useRef(0)
  const nid = () => `b${idc.current++}`
  const [pages, setPages] = useState<UIPage[]>(() =>
    parseDoc(formula.latex).map(p => {
      const format = p.format ?? DEFAULT_FORMAT
      const orientation = p.orientation ?? 'portrait'
      const sw = paperPx(format, orientation).w
      let y = SHEET_PAD
      const blocks = p.blocks.map(b => {
        const frame = b.frame ?? { x: SHEET_PAD, y, w: sw - SHEET_PAD * 2 }
        y = Math.max(y, frame.y + (frame.h ?? estBlockH(b)) + 16)
        return { ...b, id: nid(), frame }
      })
      return { name: p.name, blocks, format, orientation }
    }))
  const [active, setActive] = useState(0)
  const [activeBlockId, setActiveBlockId] = useState<string | null>(null)
  const [category, setCategory] = useState(MATH_CATEGORIES[0].id)
  const [mode, setMode] = useState<'visual' | 'code'>('code')
  // Palette search across every category (null = no active search, browse by category).
  const [query, setQuery] = useState('')
  const searchResults = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return null
    const out: MathTemplate[] = []
    for (const c of MATH_CATEGORIES) {
      for (const tpl of c.items) {
        if ((tpl.title ?? '').toLowerCase().includes(q) || tpl.ins.toLowerCase().includes(q) || c.label.toLowerCase().includes(q)) {
          out.push(tpl)
          if (out.length >= 60) return out
        }
      }
    }
    return out
  }, [query])
  // Sheet zoom (persisted). Applied with the CSS `zoom` property so layout stays simple.
  const [zoom, setZoom] = useState<number>(() => {
    const v = Number(localStorage.getItem('kubuno.maths.zoom'))
    return v >= 40 && v <= 250 ? v : 100
  })
  const applyZoom = (nz: number) => { const z = Math.max(40, Math.min(250, nz)); localStorage.setItem('kubuno.maths.zoom', String(z)); setZoom(z) }
  // Optional layout grid on the sheet: draws a light grid and snaps block moves to it.
  const GRID_STEP = 16
  const [showGrid, setShowGrid] = useState<boolean>(() => localStorage.getItem('kubuno.maths.grid') === '1')
  const toggleGrid = () => setShowGrid(v => { const nv = !v; localStorage.setItem('kubuno.maths.grid', nv ? '1' : '0'); return nv })
  const snapG = (v: number) => Math.round(v / GRID_STEP) * GRID_STEP
  // Fit the sheet to the visible width of the scroll viewport (like Word's "page width").
  const fitToWidth = () => {
    const el = scrollRef.current
    if (!el) return
    const avail = el.clientWidth - 48 // p-6 padding both sides
    applyZoom(Math.round((avail / sheet.w) * 100))
  }
  // Semantic syntax-coloring of the formula (variables / numbers / operators / … each a distinct
  // hue, Mathcha-style). Pure display preference — the stored LaTeX is unchanged. Persisted per app.
  const [colorize, setColorize] = useState<boolean>(loadColorize)
  const [themeId, setThemeId] = useState<string>(loadThemeId)
  const [customPal, setCustomPal] = useState<MathPalette>(loadCustomPalette)
  const [themeOpen, setThemeOpen] = useState(false)
  const palette = useMemo(() => resolvePalette(themeId, customPal), [themeId, customPal])
  const colorStyle = useMemo(() => paletteStyle(palette), [palette])
  const toggleColorize = () => setColorize(v => { const nv = !v; saveColorize(nv); return nv })
  const pickTheme = (id: string) => { setThemeId(id); saveThemeId(id); if (!colorize) { setColorize(true); saveColorize(true) } }
  // Editing any swatch forks the active palette into a personal "custom" theme.
  const editColor = (role: keyof MathPalette, hex: string) => {
    const base = themeId === 'custom' ? customPal : palette
    const next = { ...base, [role]: hex }
    setCustomPal(next); saveCustomPalette(next)
    setThemeId('custom'); saveThemeId('custom')
    if (!colorize) { setColorize(true); saveColorize(true) }
  }
  const [vMenu, setVMenu] = useState<{ x: number; y: number } | null>(null)
  const pageIdx = Math.min(active, pages.length - 1)
  const page: UIPage = pages[pageIdx] ?? { name: '', blocks: [], format: DEFAULT_FORMAT, orientation: 'portrait' }
  const sheet = paperPx(page.format, page.orientation)
  const setPageMeta = (meta: Partial<{ format: string; orientation: Orientation }>) => setPages(prev => prev.map((p, i) => i === pageIdx ? { ...p, ...meta } : p))

  // Undo/Redo at the real editing level: the document IS `pages` (every mutation — typing,
  // structures, moves, block add/remove, page ops — routes through setPages). Snapshot history
  // coalesces bursts (a drag, a run of keystrokes) into one step and rebaselines per document.
  const hist = useSnapshotHistory(pages, setPages, formula.id)
  useEffect(() => { onHistory?.({ undo: hist.undo, redo: hist.redo, canUndo: hist.canUndo, canRedo: hist.canRedo }) }, [hist.canUndo, hist.canRedo]) // eslint-disable-line react-hooks/exhaustive-deps
  // Ctrl/Cmd+Z = undo, Ctrl/Cmd+Y or Ctrl/Cmd+Shift+Z = redo. Plain <input> (title, search) keep
  // their native undo; the visual editor and the code textarea both feed `pages`, so we drive it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return
      const k = e.key.toLowerCase()
      if (k !== 'z' && k !== 'y') return
      if ((document.activeElement as HTMLElement | null)?.tagName === 'INPUT') return
      e.preventDefault()
      if (k === 'y' || e.shiftKey) hist.redo(); else hist.undo()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }) // re-bound every render so the closure sees the current history stacks

  // Per-block editor handles (visual) and textareas (code), so the palette targets the right block.
  const editorRefs = useRef<Record<string, MathEditorHandle | null>>({})
  const taRefs = useRef<Record<string, { current: HTMLTextAreaElement | null }>>({})
  const taRefOf = (id: string) => (taRefs.current[id] ??= { current: null })

  // ── Block mutations on the active page ──────────────────────────────────────────
  const setBlocks = (fn: (bs: UIBlock[]) => UIBlock[]) => setPages(prev => prev.map((p, i) => i === pageIdx ? { ...p, blocks: fn(p.blocks) } : p))
  const setBlockLatex = (id: string, v: string | ((p: string) => string)) =>
    setBlocks(bs => bs.map(b => b.id === id && b.type === 'formula' ? { ...b, latex: typeof v === 'function' ? v(b.latex) : v } : b))
  const setGraphSpec = (id: string, spec: GraphSpec) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'graph' ? { ...b, spec } : b))
  const setFrame = (id: string, patch: Partial<BlockFrame>) => setBlocks(bs => bs.map(b => b.id === id ? { ...b, frame: { ...b.frame, ...patch } } : b))

  // DOM nodes of the rendered blocks (real heights for placement / sheet growth).
  const blockEls = useRef<Record<string, HTMLDivElement | null>>({})
  const blockH = (b: UIBlock) => blockEls.current[b.id]?.offsetHeight ?? b.frame.h ?? estBlockH(b)
  const contentBottom = () => page.blocks.reduce((m, b) => Math.max(m, b.frame.y + blockH(b)), SHEET_PAD)
  // New blocks land below everything already on the sheet.
  const newFrame = (w: number): BlockFrame => ({ x: SHEET_PAD, y: page.blocks.length ? contentBottom() + 12 : SHEET_PAD, w: Math.min(w, sheet.w - SHEET_PAD * 2) })
  // `kind` is a block type or a text style ('formula' | 'graph' | 'h1' | 'h2' | 'p').
  type ObjectKind = 'formula' | 'graph' | TextStyle
  const createBlock = (kind: ObjectKind, id: string, frame: BlockFrame, bs: UIBlock[]): UIBlock =>
    kind === 'graph' ? { id, type: 'graph', spec: defaultGraphSpec(bs.filter(b => b.type === 'graph').length), frame }
    : kind === 'formula' ? { id, type: 'formula', latex: '', frame }
    : { id, type: 'text', text: '', style: kind, frame }
  const addBlockBottom = (kind: ObjectKind) => {
    const id = nid()
    const frame = newFrame(kind === 'graph' ? 560 : sheet.w)
    setBlocks(bs => [...bs, createBlock(kind, id, frame, bs)])
    setActiveBlockId(id)
  }
  // Drop from the "Objets" dock panel: place the new block where the pointer landed.
  const addBlockAt = (kind: ObjectKind, clientX: number, clientY: number) => {
    const id = nid()
    const z = zoom / 100
    const rect = sheetRef.current?.getBoundingClientRect()
    const w = kind === 'graph' ? 520 : kind === 'formula' ? 420 : 380
    const x = rect ? Math.max(0, Math.min(sheet.w - w, Math.round((clientX - rect.left) / z - w / 2))) : SHEET_PAD
    const y = rect ? Math.max(0, Math.round((clientY - rect.top) / z - 20)) : SHEET_PAD
    setBlocks(bs => [...bs, createBlock(kind, id, { x, y, w }, bs)])
    setActiveBlockId(id)
  }
  const addFormulaBlock = () => addBlockBottom('formula')
  const addGraphBlock = () => addBlockBottom('graph')
  const addTextBlock = () => addBlockBottom('p')
  const setBlockText = (id: string, text: string) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'text' ? { ...b, text } : b))
  const setBlockScale = (id: string, scale: number) =>
    setBlocks(bs => bs.map(b => b.id === id && b.type !== 'graph' ? { ...b, scale: Math.abs(scale - 1) < 0.01 ? undefined : scale } : b))
  const setBlockTextStyle = (id: string, style: TextStyle) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'text' ? { ...b, style } : b))
  // Formula alignment cycles centre → left → right; numbering assigns a document-wide (n).
  const nextAlign: Record<BlockAlign, BlockAlign> = { center: 'left', left: 'right', right: 'center' }
  const cycleAlign = (id: string) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'formula' ? { ...b, align: nextAlign[b.align ?? 'center'] } : b))
  const setBlockAlign = (id: string, align: BlockAlign) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'formula' ? { ...b, align } : b))
  const resetRotation = (id: string) => setFrame(id, { r: undefined })
  const toggleNumbered = (id: string) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'formula' ? { ...b, numbered: !b.numbered } : b))
  const removeBlock = (id: string) => setBlocks(bs => bs.length > 1 ? bs.filter(b => b.id !== id) : bs)
  // Duplicate a block in place (offset by 16px) and select the copy — like Presentation's Ctrl+D.
  // The source is read inside the updater so it is never a stale copy of the block.
  const duplicateBlock = (id: string) => {
    const cid = nid()
    setBlocks(bs => {
      const i = bs.findIndex(b => b.id === id); if (i < 0) return bs
      const src = bs[i]
      const clone = { ...src, id: cid, frame: { ...src.frame, x: src.frame.x + 16, y: src.frame.y + 16 } } as UIBlock
      const n = [...bs]; n.splice(i + 1, 0, clone); return n
    })
    setActiveBlockId(cid)
  }
  // Blocks overlap freely; array order is the z-order (later = on top).
  const moveBlock = (id: string, dir: -1 | 1) => setBlocks(bs => { const i = bs.findIndex(b => b.id === id); const j = i + dir; if (i < 0 || j < 0 || j >= bs.length) return bs; const n = [...bs]; [n[i], n[j]] = [n[j], n[i]]; return n })

  // ── Sélection + manipulation façon presentations : déplacement par les bords, 8 poignées
  // de redimensionnement (Maj = ratio conservé), rotation, guides magnétiques (Alt désactive) ──
  type ManipMode = 'move' | 'rotate' | 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw'
  const dragRef = useRef<{ id: string; mode: ManipMode; sx: number; sy: number; f: BlockFrame; h0: number; gh: number; s0: number } | null>(null)
  const sheetRef = useRef<HTMLDivElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)

  // Ctrl/Cmd + molette = zoom de la feuille, ancré sur le curseur (listener natif
  // {passive:false} : preventDefault doit bloquer le zoom pleine-page du navigateur).
  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return
      e.preventDefault()
      const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1
      const nz = Math.round(Math.max(40, Math.min(250, zoom * factor)))
      if (nz === zoom) return
      const rect = el.getBoundingClientRect()
      const offX = e.clientX - rect.left, offY = e.clientY - rect.top
      const sx = el.scrollLeft + offX, sy = el.scrollTop + offY
      applyZoom(nz)
      // Keep the sheet point under the cursor stationary once the new zoom is laid out.
      requestAnimationFrame(() => {
        const k = nz / zoom
        el.scrollLeft = sx * k - offX
        el.scrollTop = sy * k - offY
      })
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }) // re-bound every render: the closure must see the current `zoom`

  // Ctrl/Cmd+D duplicates the selected block (skipped while typing in a text field).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'd' && activeBlockId) {
        const ae = document.activeElement
        if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA')) return
        e.preventDefault()
        duplicateBlock(activeBlockId)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }) // re-bound every render so the closure sees the current activeBlockId/page
  const [snapGuides, setSnapGuides] = useState<SnapGuide[]>([])
  const clampN = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v))
  const otherBoxes = (excludeId: string): Box[] =>
    page.blocks.filter(b => b.id !== excludeId).map(b => ({ x: b.frame.x, y: b.frame.y, w: b.frame.w, h: blockH(b) }))
  const startManip = (e: React.PointerEvent, id: string, mode: ManipMode) => {
    const b = page.blocks.find(x => x.id === id); if (!b) return
    e.preventDefault(); e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    dragRef.current = {
      id, mode, sx: e.clientX, sy: e.clientY, f: { ...b.frame }, h0: blockH(b),
      gh: b.type === 'graph' ? b.spec.height : 0,
      s0: b.type !== 'graph' ? (b.scale ?? 1) : 1,
    }
    setActiveBlockId(id)
  }
  const onManipMove = (e: React.PointerEvent) => {
    const d = dragRef.current; if (!d) return
    const z = zoom / 100                                  // pointer deltas are in visual px
    const b = page.blocks.find(x => x.id === d.id); if (!b) return
    const sheetH = Math.max(sheet.h, contentBottom() + SHEET_PAD)
    if (d.mode === 'rotate') {
      const rect = sheetRef.current?.getBoundingClientRect(); if (!rect) return
      const cx = rect.left + (d.f.x + d.f.w / 2) * z
      const cy = rect.top + (d.f.y + d.h0 / 2) * z
      let deg = Math.atan2(e.clientY - cy, e.clientX - cx) * 180 / Math.PI + 90
      if (e.shiftKey) deg = Math.round(deg / 15) * 15
      deg = Math.round(((deg % 360) + 360) % 360)
      setFrame(d.id, { r: deg === 0 ? undefined : deg })
      return
    }
    const dx = (e.clientX - d.sx) / z, dy = (e.clientY - d.sy) / z
    if (d.mode === 'move') {
      let box: Box = { x: d.f.x + dx, y: d.f.y + dy, w: d.f.w, h: d.h0 }
      let guides: SnapGuide[] = []
      if (!e.altKey) {
        const s = snapBox(box, buildSnapTargets(otherBoxes(d.id), sheet.w, sheetH), 7 / z)
        box = { ...box, x: s.x, y: s.y }; guides = s.guides
        // Grid magnetism (only on axes the smart guides didn't already lock).
        if (showGrid) {
          if (!guides.some(g => g.axis === 'v')) box.x = snapG(box.x)
          if (!guides.some(g => g.axis === 'h')) box.y = snapG(box.y)
        }
      }
      setSnapGuides(guides)
      setFrame(d.id, {
        x: clampN(Math.round(box.x), 0, Math.max(0, sheet.w - d.f.w)),
        y: Math.max(0, Math.round(box.y)),
      })
      return
    }
    // Redimensionnement par poignées (n/s/e/w + coins), même logique que presentations.
    const base = b.type === 'graph' ? d.gh : d.h0
    const minW = 120, minH = b.type === 'text' ? 40 : b.type === 'graph' ? 120 : 90
    let x = d.f.x, y = d.f.y, w = d.f.w, h = base
    const m = d.mode
    if (m.includes('e')) w = Math.max(minW, d.f.w + dx)
    if (m.includes('s')) h = Math.max(minH, base + dy)
    if (m.includes('w')) { w = Math.max(minW, d.f.w - dx); x = d.f.x + (d.f.w - w) }
    if (m.includes('n')) { h = Math.max(minH, base - dy); y = d.f.y + (base - h) }
    // Contrainte d'aspect : Maj conserve le ratio largeur/hauteur d'origine.
    if (e.shiftKey && d.f.w > 0 && base > 0) {
      const ar = d.f.w / base
      if (w / h > ar) w = h * ar; else h = w / ar
      if (m.includes('w')) x = d.f.x + (d.f.w - w)
      if (m.includes('n')) y = d.f.y + (base - h)
    }
    // Magnétisme des bords redimensionnés (repères intelligents).
    if (!e.altKey && !e.shiftKey) {
      const targets = buildSnapTargets(otherBoxes(d.id), sheet.w, sheetH)
      const th = 7 / z
      const gs: SnapGuide[] = []
      if (m.includes('e')) { const s = snapAxis([{ v: x + w, lo: y, hi: y + h }], targets.xs, th, 'v'); if (s) { w += s.delta; gs.push(s.guide) } }
      if (m.includes('w')) { const s = snapAxis([{ v: x, lo: y, hi: y + h }], targets.xs, th, 'v'); if (s) { x += s.delta; w -= s.delta; gs.push(s.guide) } }
      if (m.includes('s')) { const s = snapAxis([{ v: y + h, lo: x, hi: x + w }], targets.ys, th, 'h'); if (s) { h += s.delta; gs.push(s.guide) } }
      if (m.includes('n')) { const s = snapAxis([{ v: y, lo: x, hi: x + w }], targets.ys, th, 'h'); if (s) { y += s.delta; h -= s.delta; gs.push(s.guide) } }
      setSnapGuides(gs)
    } else setSnapGuides([])
    x = Math.round(x); y = Math.round(y); w = Math.round(w); h = Math.round(h)
    if (b.type === 'graph') {
      if (h !== b.spec.height) setGraphSpec(d.id, { ...b.spec, height: h })
      setFrame(d.id, { x, y, w })
    } else {
      const patch: Partial<BlockFrame> = { x, y, w }
      if (m.includes('n') || m.includes('s') || e.shiftKey) patch.h = h
      // Corner handles scale the CONTENT with the box (object-like resize); edge handles
      // only reshape the box (a font cannot be stretched on a single axis).
      const corner = m.length === 2
      const ns = corner ? Math.max(0.2, Math.min(8, d.s0 * (w / Math.max(1, d.f.w)))) : null
      setBlocks(bs => bs.map(xb => xb.id === d.id
        ? {
            ...xb,
            frame: { ...xb.frame, ...patch },
            ...(ns != null && xb.type !== 'graph' ? { scale: Math.abs(ns - 1) < 0.01 ? undefined : ns } : {}),
          } as UIBlock
        : xb))
    }
  }
  const endManip = () => { dragRef.current = null; setSnapGuides([]) }
  const manip = (id: string, mode: ManipMode) => ({
    onPointerDown: (e: React.PointerEvent) => startManip(e, id, mode),
    onPointerMove: onManipMove,
    onPointerUp: endManip,
  })

  // The formula block the palette / toolbar / context-menu act on: the focused one, else the first.
  const targetId = (activeBlockId && page.blocks.some(b => b.id === activeBlockId && b.type === 'formula'))
    ? activeBlockId
    : (page.blocks.find(b => b.type === 'formula')?.id ?? null)
  const ed = () => targetId ? editorRefs.current[targetId] ?? null : null
  const targetLatex = () => { const b = page.blocks.find(x => x.id === targetId); return b && b.type === 'formula' ? b.latex : '' }
  // The ribbon is only re-pushed to the parent when `ribbonSig` changes (not on every content
  // edit), so ribbon-button closures can be stale. CAS actions read the live target through this
  // ref (refreshed every render) instead of the captured `targetLatex`.
  const targetLatexRef = useRef('')
  targetLatexRef.current = targetLatex()

  // Cross-module copy: the selected formula travels as an `office.math` JSON
  // envelope (LaTeX + full block), rendered by office itself in the consumer.
  const copyForKubuno = () => {
    const latex = targetLatexRef.current
    if (!latex.trim()) return
    const block = page.blocks.find(b => b.id === targetId)
    copyKubunoData(mathEnvelope(latex, block, formula.id)).catch(() => {})
  }
  // Ribbon closures can be stale (the ribbon is only re-pushed on signature
  // change) — buttons call through this ref, refreshed every render.
  const copyForKubunoRef = useRef(copyForKubuno)
  copyForKubunoRef.current = copyForKubuno

  // Sequential equation numbers across the whole document (only numbered formula blocks).
  const eqNumbers = useMemo(() => {
    const m = new Map<string, number>()
    let k = 0
    for (const p of pages) for (const b of p.blocks) if (b.type === 'formula' && b.numbered) m.set(b.id, ++k)
    return m
  }, [pages])

  // Live numeric evaluation of the active formula (constant expressions only).
  const evalValue = useMemo(() => {
    const b = page.blocks.find(x => x.id === targetId)
    return b && b.type === 'formula' ? evalLatex(b.latex) : null
  }, [page.blocks, targetId])
  // KaTeX validation is expensive — never run it inline on every render.
  const anyLatexError = useMemo(
    () => page.blocks.some(b => b.type === 'formula' && hasLatexError(b.latex)),
    [page.blocks],
  )
  const insertResult = () => {
    if (evalValue == null || !targetId) return
    if (mode === 'visual') ed()?.insertLatex(`=${resultLatex(evalValue)}`)
    else setBlockLatex(targetId, l => `${l} = ${resultLatex(evalValue)}`)
  }

  // ── Calcul (CAS-lite) : dérivée / simplification / résolution / table / traçage ──────
  // Feedback façon toast (les erreurs ne doivent jamais être silencieuses).
  const [casNote, setCasNote] = useState<{ text: string; err: boolean } | null>(null)
  const casTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const notify = (text: string, err = false) => {
    setCasNote({ text, err })
    if (casTimer.current) clearTimeout(casTimer.current)
    casTimer.current = setTimeout(() => setCasNote(null), 4000)
  }
  useEffect(() => () => { if (casTimer.current) clearTimeout(casTimer.current) }, [])
  // Insert a new formula block below the current content and select it.
  const insertFormulaBlock = (latex: string) => {
    const id = nid(); const frame = newFrame(sheet.w)
    setBlocks(bs => [...bs, { id, type: 'formula', latex, frame }]); setActiveBlockId(id)
  }
  const casSource = () => targetLatexRef.current
  const doDerivative = () => {
    const r = derivativeLatex(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`\\frac{d}{dx}\\left(${casSource()}\\right) = ${r.latex}`)
    notify(t('math_cas_derived', { defaultValue: 'Dérivée insérée' }))
  }
  const doSimplify = () => {
    const r = simplifyLatex(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`${casSource()} = ${r.latex}`)
    notify(t('math_cas_simplified', { defaultValue: 'Forme simplifiée insérée' }))
  }
  const doSolve = () => {
    const r = solveRoots(casSource())
    if (r.error) return notify(r.error, true)
    if (!r.roots.length) return notify(t('math_cas_no_root', { defaultValue: 'Aucune racine réelle sur [-10, 10]' }), true)
    const list = r.roots.map(x => `x \\approx ${fmtResult(Number(x.toPrecision(6)))}`).join(',\\quad ')
    insertFormulaBlock(list)
    notify(t('math_cas_solved', { count: r.roots.length, defaultValue: `${r.roots.length} racine(s) trouvée(s)` }))
  }
  const doTable = () => {
    const r = valueTableLatex(casSource(), [-3, -2, -1, 0, 1, 2, 3])
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(r.latex)
    notify(t('math_cas_table', { defaultValue: 'Table de valeurs insérée' }))
  }
  const plotFormula = () => {
    const expr = latexToExpr(casSource())
    if (!expr) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x traçable." }), true)
    const id = nid()
    const base = defaultGraphSpec(page.blocks.filter(b => b.type === 'graph').length)
    const spec: GraphSpec = { ...base, fns: [{ expr, color: base.fns[0]?.color ?? GRAPH_COLORS[0] }] }
    const frame = newFrame(560)
    setBlocks(bs => [...bs, { id, type: 'graph', spec, frame }]); setActiveBlockId(id)
    notify(t('math_cas_plotted', { defaultValue: 'Graphe de la formule créé' }))
  }
  const doIntegral = () => {
    const r = integralLatex(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`\\int ${casSource()}\\,dx = ${r.latex}`)
    notify(t('math_cas_integrated', { defaultValue: 'Primitive insérée' }))
  }
  const doArea = async () => {
    const src = casSource()
    if (!latexToExpr(src)) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x." }), true)
    const raw = await appPrompt(t('math_cas_area_prompt', { defaultValue: 'Bornes de l\'intégrale (a ; b) :' }), '0 ; 1')
    if (raw == null) return
    const m = raw.split(/[;,]/).map(s => Number(s.trim()))
    if (m.length !== 2 || !m.every(Number.isFinite)) return notify(t('math_cas_area_bad', { defaultValue: 'Bornes invalides (format « a ; b »).' }), true)
    const r = definiteIntegral(src, m[0], m[1])
    if (r.error || r.value == null) return notify(r.error ?? 'Erreur', true)
    insertFormulaBlock(`\\int_{${fmtResult(m[0])}}^{${fmtResult(m[1])}} ${src}\\,dx \\approx ${resultLatex(r.value)}`)
    notify(t('math_cas_area_done', { defaultValue: 'Aire calculée' }))
  }
  const doExpand = () => {
    const r = expandLatex(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`${casSource()} = ${r.latex}`)
    notify(t('math_cas_expanded', { defaultValue: 'Développement inséré' }))
  }
  const doFactor = () => {
    const r = factorLatex(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`${casSource()} = ${r.latex}`)
    notify(t('math_cas_factored', { defaultValue: 'Factorisation insérée' }))
  }
  const doTaylor = async () => {
    const src = casSource()
    if (!latexToExpr(src)) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x." }), true)
    const raw = await appPrompt(t('math_cas_taylor_prompt', { defaultValue: 'Ordre (et centre optionnel : « ordre ; a ») :' }), '5 ; 0')
    if (raw == null) return
    const parts = String(raw).split(/[;,]/).map(s => Number(s.trim()))
    const order = Math.max(1, Math.min(12, Math.round(parts[0]) || 5))
    const center = Number.isFinite(parts[1]) ? parts[1] : 0
    const r = taylorLatex(src, order, center)
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`${src} \\approx ${r.latex}`)
    notify(t('math_cas_taylor_done', { defaultValue: 'Série de Taylor insérée' }))
  }
  // ── Étude de fonction : dérivée seconde, tangente, extrema, limite ──────────────────
  const doSecondDeriv = () => {
    const r = nthDerivativeLatex(casSource(), 2)
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(`\\frac{d^2}{dx^2}\\left(${casSource()}\\right) = ${r.latex}`)
    notify(t('math_cas_d2_done', { defaultValue: 'Dérivée seconde insérée' }))
  }
  const doTangent = async () => {
    const src = casSource()
    if (!latexToExpr(src)) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x." }), true)
    const raw = await appPrompt(t('math_cas_tangent_prompt', { defaultValue: 'Abscisse du point de tangence (x =) :' }), '0')
    if (raw == null) return
    const a = Number(String(raw).trim())
    if (!Number.isFinite(a)) return notify(t('math_cas_bad_point', { defaultValue: 'Abscisse invalide.' }), true)
    const r = tangentAt(src, a)
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(r.latex)
    // Also plot the curve together with its tangent for the visual payoff.
    const fexpr = latexToExpr(src)
    if (fexpr) {
      const id = nid()
      const base = defaultGraphSpec(page.blocks.filter(b => b.type === 'graph').length)
      const spec: GraphSpec = { ...base, fns: [{ expr: fexpr, color: GRAPH_COLORS[0] }, { expr: r.expr, color: GRAPH_COLORS[1 % GRAPH_COLORS.length] }] }
      setBlocks(bs => [...bs, { id, type: 'graph', spec, frame: newFrame(560) }]); setActiveBlockId(id)
    }
    notify(t('math_cas_tangent_done', { defaultValue: 'Tangente insérée et tracée' }))
  }
  const doExtrema = () => {
    const r = extremaLatex(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(r.latex)
    notify(t('math_cas_extrema_done', { defaultValue: 'Points critiques insérés' }))
  }
  const doLimit = async () => {
    const src = casSource()
    if (!latexToExpr(src)) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x." }), true)
    const raw = await appPrompt(t('math_cas_limit_prompt', { defaultValue: 'Limite quand x tend vers (nombre, inf ou -inf) :' }), '0')
    if (raw == null) return
    const s = String(raw).trim().toLowerCase()
    const at = /^[+]?inf/.test(s) ? Infinity : /^-inf/.test(s) ? -Infinity : Number(s)
    if (!Number.isFinite(at) && at !== Infinity && at !== -Infinity) return notify(t('math_cas_bad_point', { defaultValue: 'Point invalide.' }), true)
    const r = limitLatex(src, at)
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(r.latex)
    notify(t('math_cas_limit_done', { defaultValue: 'Limite insérée' }))
  }
  // ── Matrices / statistiques : opèrent sur casSource() et insèrent le résultat ────────
  const doMatrix = (fn: (l: string) => { latex: string; error: string | null }, ok: string) => {
    const r = fn(casSource())
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(r.latex)
    notify(ok)
  }
  // Prompt for `count` numbers separated by ; (or ,) and run `fn(...nums)`.
  const doNumeric = async (msg: string, def: string, count: number, fn: (...n: number[]) => { latex: string; error: string | null }, ok: string) => {
    const raw = await appPrompt(msg, def)
    if (raw == null) return
    const nums = String(raw).split(/[;,]/).map(s => Number(s.trim()))
    if (nums.length !== count || !nums.every(Number.isFinite)) return notify(t('math_cas_bad_input', { defaultValue: 'Entrée invalide.' }), true)
    const r = fn(...nums)
    if (r.error) return notify(r.error, true)
    insertFormulaBlock(r.latex)
    notify(ok)
  }
  const doSolveExact = () => doMatrix(solveExactLatex, t('math_cas_exact_done', { defaultValue: 'Racines exactes insérées' }))
  const doInflection = () => doMatrix(inflectionLatex, t('math_cas_infl_done', { defaultValue: "Points d'inflexion insérés" }))
  const doAsymptotes = () => doMatrix(asymptotesLatex, t('math_cas_asym_done', { defaultValue: 'Asymptotes insérées' }))
  const doEvalPoint = async () => {
    if (!latexToExpr(casSource())) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x." }), true)
    const raw = await appPrompt(t('math_cas_evalpt_prompt', { defaultValue: 'Évaluer f en x =' }), '1')
    if (raw == null) return
    const a = Number(String(raw).trim())
    if (!Number.isFinite(a)) return notify(t('math_cas_bad_point', { defaultValue: 'Valeur invalide.' }), true)
    doMatrix(l => evalPointLatex(l, a), t('math_cas_evalpt_done', { defaultValue: 'Valeur insérée' }))
  }
  const doAccumulate = async (fn: (l: string, a: number, b: number) => { latex: string; error: string | null }, ok: string) => {
    if (!latexToExpr(casSource())) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x." }), true)
    const raw = await appPrompt(t('math_cas_range_prompt', { defaultValue: 'Bornes (a ; b) sur la variable x :' }), '1 ; 10')
    if (raw == null) return
    const m = String(raw).split(/[;,]/).map(s => Number(s.trim()))
    if (m.length !== 2 || !m.every(Number.isFinite)) return notify(t('math_cas_bad_input', { defaultValue: 'Bornes invalides.' }), true)
    doMatrix(l => fn(l, m[0], m[1]), ok)
  }
  const doMatrixPower = async () => {
    const raw = await appPrompt(t('math_mat_power_prompt', { defaultValue: 'Exposant n :' }), '2')
    if (raw == null) return
    const n = Math.round(Number(String(raw).trim()))
    if (!Number.isFinite(n)) return notify(t('math_cas_bad_input', { defaultValue: 'Exposant invalide.' }), true)
    doMatrix(l => powerLatex(l, n), t('math_mat_power_done', { defaultValue: 'Puissance calculée' }))
  }
  const plotWithDerivative = () => {
    const fexpr = latexToExpr(casSource())
    if (!fexpr) return notify(t('math_cas_not_expr', { defaultValue: "Ce bloc n'est pas une fonction de x traçable." }), true)
    const dr = derivativeLatex(casSource())
    if (dr.error) return notify(dr.error, true)
    const dexpr = latexToExpr(dr.latex)
    if (!dexpr) return notify(t('math_cas_not_expr', { defaultValue: 'Dérivée non traçable.' }), true)
    const id = nid()
    const base = defaultGraphSpec(page.blocks.filter(b => b.type === 'graph').length)
    const spec: GraphSpec = { ...base, fns: [{ expr: fexpr, color: GRAPH_COLORS[0] }, { expr: dexpr, color: GRAPH_COLORS[1 % GRAPH_COLORS.length] }] }
    setBlocks(bs => [...bs, { id, type: 'graph', spec, frame: newFrame(560) }]); setActiveBlockId(id)
    notify(t('math_cas_plot_deriv', { defaultValue: 'f et f′ tracées' }))
  }

  const serialized = useMemo(() => serializeDoc(pages.map(p => ({ name: p.name, blocks: p.blocks.map(stripId), format: p.format, orientation: p.orientation }))), [pages])
  // Autosave cadence is the instance default (seconds → ms); 0 disables it.
  const officeInstance = useOfficeInstance()
  useDebouncedAutosave(serialized, officeInstance.mathsAutosaveS > 0, (v) => {
    formulasApi.update(formula.id, { latex: v }).then(d => onUpdate(d.formula)).catch(() => {})
  }, officeInstance.mathsAutosaveS * 1000)

  // Expose an immediate save to the parent's title-bar SaveButton (same path as the autosave).
  useEffect(() => {
    if (!saveRef) return
    saveRef.current = () => formulasApi.update(formula.id, { latex: serialized }).then(d => onUpdate(d.formula)).catch(() => {})
    return () => { if (saveRef) saveRef.current = null }
  }, [saveRef, formula.id, serialized, onUpdate])

  // Expose clean printing (all pages, paper-sized sheets) to the backstage "Print" entry.
  useEffect(() => {
    if (!printRef) return
    printRef.current = () => {
      const cssVars = Object.entries(colorStyle as Record<string, string>).map(([k, v]) => `${k}:${v}`).join(';')
      printMathDoc(
        formula.name,
        pages.map(p => ({ name: p.name, blocks: p.blocks.map(stripId), format: p.format, orientation: p.orientation })),
        { colorize, cssVars },
      )
    }
    return () => { if (printRef) printRef.current = null }
  }, [printRef, pages, colorize, colorStyle, formula.name])

  const cat = MATH_CATEGORIES.find(c => c.id === category) ?? MATH_CATEGORIES[0]

  // ── Panneaux ancrables (DockArea du core, comme le module App) : Symboles / Propriétés ──
  const dockTheme = {
    panel: WORKSPACE_LIGHT.panel, header: WORKSPACE_LIGHT.toolbar,
    border: WORKSPACE_LIGHT.border, text: WORKSPACE_LIGHT.text, textDim: WORKSPACE_LIGHT.textDim,
    accent: WORKSPACE_LIGHT.accent,
  }
  const selBlock = activeBlockId ? page.blocks.find(x => x.id === activeBlockId) ?? null : null
  const secTitle = (label: string) => <div className="text-[10px] font-semibold uppercase tracking-wide text-text-tertiary">{label}</div>
  const renderProperties = () => {
    const b = selBlock
    if (!b) {
      return <div className="h-full bg-white p-3 text-xs text-text-tertiary">{t('math_props_empty', { defaultValue: 'Sélectionnez un bloc sur la feuille pour modifier ses propriétés.' })}</div>
    }
    const bi = page.blocks.findIndex(x => x.id === b.id)
    const typeLabel = b.type === 'graph' ? t('math_add_graph', { defaultValue: 'Graphe' }) : b.type === 'text' ? t('math_add_text', { defaultValue: 'Texte' }) : t('math_add_formula', { defaultValue: 'Formule' })
    const hVal = b.type === 'graph' ? b.spec.height : (b.frame.h ?? '')
    const btn = 'h-7 px-2 rounded border border-border bg-white hover:bg-surface-1 text-text-secondary disabled:opacity-40'
    return (
      <div className="h-full overflow-y-auto bg-white p-3 flex flex-col gap-3 text-xs">
        <div className="font-semibold text-text-primary text-sm">{typeLabel}</div>
        <div className="flex flex-col gap-1.5">
          {secTitle(t('math_props_geometry', { defaultValue: 'Position & taille' }))}
          <PropNum label="X" value={b.frame.x} onChange={v => setFrame(b.id, { x: Math.round(v) })} />
          <PropNum label="Y" value={b.frame.y} onChange={v => setFrame(b.id, { y: Math.max(0, Math.round(v)) })} />
          <PropNum label={t('math_props_w', { defaultValue: 'Largeur' })} value={b.frame.w} onChange={v => setFrame(b.id, { w: Math.max(120, Math.round(v)) })} />
          <PropNum label={t('math_props_h', { defaultValue: 'Hauteur' })} value={hVal} placeholder="auto"
            onChange={v => b.type === 'graph' ? setGraphSpec(b.id, { ...b.spec, height: Math.max(120, Math.round(v)) }) : setFrame(b.id, { h: Math.max(40, Math.round(v)) })} />
          {b.type !== 'graph' && b.frame.h != null && (
            <button onClick={() => setFrame(b.id, { h: undefined })} className={`self-end ${btn}`}>
              {t('math_props_h_auto', { defaultValue: 'Hauteur auto' })}
            </button>
          )}
          <PropNum label={t('math_props_rotation', { defaultValue: 'Rotation (°)' })} value={b.frame.r ?? 0}
            onChange={v => setFrame(b.id, { r: (((Math.round(v) % 360) + 360) % 360) || undefined })} />
          {b.type !== 'graph' && (
            <PropNum label={t('math_props_scale', { defaultValue: 'Échelle (%)' })} value={Math.round((b.scale ?? 1) * 100)}
              onChange={v => setBlockScale(b.id, Math.max(20, Math.min(800, Math.round(v))) / 100)} />
          )}
        </div>
        {b.type === 'formula' && (
          <div className="flex flex-col gap-1.5">
            {secTitle(t('math_props_formula', { defaultValue: 'Formule' }))}
            <div className="flex items-center gap-1">
              {(['left', 'center', 'right'] as BlockAlign[]).map(a => (
                <button key={a} title={a} onClick={() => setBlocks(bs => bs.map(x => x.id === b.id && x.type === 'formula' ? { ...x, align: a } : x))}
                  className={`p-1.5 rounded border ${(b.align ?? 'center') === a ? 'border-primary/50 bg-primary/5 text-primary' : 'border-border text-text-secondary hover:bg-surface-1'}`}>
                  {a === 'left' ? <AlignLeft size={13} /> : a === 'right' ? <AlignRight size={13} /> : <AlignCenter size={13} />}
                </button>
              ))}
              <label className="flex items-center gap-1.5 ml-2 cursor-pointer text-text-secondary">
                <Checkbox checked={!!b.numbered} onChange={() => toggleNumbered(b.id)} />
                {t('math_props_numbered', { defaultValue: 'Numérotée' })}
              </label>
            </div>
          </div>
        )}
        {b.type === 'text' && (
          <div className="flex flex-col gap-1.5">
            {secTitle(t('math_props_text_style', { defaultValue: 'Style du texte' }))}
            <Dropdown value={b.style ?? 'p'} onChange={(v: string) => setBlockTextStyle(b.id, v as TextStyle)}
              options={[
                { value: 'h1', label: t('math_text_h1', { defaultValue: 'Titre' }) },
                { value: 'h2', label: t('math_text_h2', { defaultValue: 'Sous-titre' }) },
                { value: 'p', label: t('math_text_p', { defaultValue: 'Paragraphe' }) },
              ]} />
          </div>
        )}
        <div className="flex flex-col gap-1.5">
          {secTitle(t('math_props_arrange', { defaultValue: 'Disposition' }))}
          <div className="flex items-center gap-1">
            <button disabled={bi === page.blocks.length - 1} onClick={() => moveBlock(b.id, 1)} className={btn}>{t('math_block_front_short', { defaultValue: 'Avancer' })}</button>
            <button disabled={bi === 0} onClick={() => moveBlock(b.id, -1)} className={btn}>{t('math_block_back_short', { defaultValue: 'Reculer' })}</button>
          </div>
          <button disabled={page.blocks.length <= 1} onClick={() => removeBlock(b.id)}
            className="h-7 px-2 rounded border border-danger/40 bg-white hover:bg-danger/5 text-danger disabled:opacity-40 self-start">
            {t('math_block_remove', { defaultValue: 'Supprimer le bloc' })}
          </button>
        </div>
      </div>
    )
  }
  // Draggable object catalogue: drag a card onto the sheet to create the block there.
  const OBJECT_ITEMS: { kind: ObjectKind; icon: React.ReactNode; label: string }[] = [
    { kind: 'formula', icon: <Sigma size={18} />, label: t('math_add_formula', { defaultValue: 'Formule' }) },
    { kind: 'graph', icon: <LineChart size={18} />, label: t('math_add_graph', { defaultValue: 'Graphe' }) },
    { kind: 'h1', icon: <Heading1 size={18} />, label: t('math_text_h1', { defaultValue: 'Titre' }) },
    { kind: 'h2', icon: <Heading2 size={18} />, label: t('math_text_h2', { defaultValue: 'Sous-titre' }) },
    { kind: 'p', icon: <Type size={18} />, label: t('math_text_p', { defaultValue: 'Paragraphe' }) },
  ]
  const dockPanels: Record<string, DockPanel> = {
    objects: {
      label: t('math_dock_objects', { defaultValue: 'Objets' }),
      render: () => (
        <div className="h-full overflow-y-auto bg-white p-2 flex flex-col gap-2">
          <div className="text-[11px] text-text-tertiary px-0.5">
            {t('math_objects_hint', { defaultValue: 'Glissez un objet sur la feuille (ou cliquez pour l\'ajouter en bas).' })}
          </div>
          <div className="grid grid-cols-2 gap-1.5">
            {OBJECT_ITEMS.map(o => (
              <div key={o.kind} draggable
                onDragStart={e => {
                  e.dataTransfer.setData('application/x-kubuno-math-object', o.kind)
                  e.dataTransfer.setData('text/plain', o.kind)
                  e.dataTransfer.effectAllowed = 'copy'
                }}
                onClick={() => addBlockBottom(o.kind)}
                title={t('math_objects_drag_tip', { defaultValue: 'Glisser sur la feuille' })}
                className="flex flex-col items-center gap-1.5 p-2.5 rounded-lg border border-border bg-white hover:border-primary/50 hover:bg-primary/5
                           cursor-grab active:cursor-grabbing select-none text-text-secondary hover:text-primary">
                {o.icon}
                <span className="text-[11px] leading-tight text-center">{o.label}</span>
              </div>
            ))}
          </div>
        </div>
      ),
    },
    symbols: {
      label: t('math_dock_symbols', { defaultValue: 'Symboles' }),
      render: () => (
        <div className="h-full flex flex-col overflow-hidden bg-white">
          <div className="p-2 border-b border-border flex-shrink-0 flex flex-col gap-2">
            <div className="relative">
              <Search size={13} className="absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary pointer-events-none" />
              <input value={query} onChange={e => setQuery(e.target.value)}
                placeholder={t('math_search', { defaultValue: 'Rechercher un symbole…' })}
                className="w-full h-7 pl-7 pr-6 text-xs border border-border rounded bg-surface-1 focus:bg-white outline-none focus:border-primary/50 text-text-primary" />
              {query && (
                <button onClick={() => setQuery('')} className="absolute right-1.5 top-1/2 -translate-y-1/2 text-text-tertiary hover:text-text-primary">
                  <XIcon size={12} />
                </button>
              )}
            </div>
            {!searchResults && (
              <Dropdown className="w-full" value={category} onChange={setCategory}
                options={MATH_CATEGORIES.map(c => ({ value: c.id, label: t('math_cat_' + c.id, { defaultValue: c.label }) }))} />
            )}
          </div>
          <div className="flex-1 overflow-y-auto p-2">
            {searchResults && searchResults.length === 0 ? (
              <div className="text-xs text-text-tertiary text-center py-4">{t('math_search_none', { defaultValue: 'Aucun résultat' })}</div>
            ) : (
              <div className="grid grid-cols-3 gap-1.5">
                {(searchResults ?? cat.items).map((tpl, i) => <TemplateButton key={i} tpl={tpl} onInsert={onInsert} />)}
              </div>
            )}
          </div>
        </div>
      ),
    },
    properties: {
      label: t('math_dock_props', { defaultValue: 'Propriétés' }),
      render: renderProperties,
    },
  }

  // ── Mode Code : source de la PAGE COMPLÈTE, une section %%[…] par bloc ──────────
  // Les formules et textes s'éditent directement ; un graphe = une fonction par ligne.
  // Le texte est appliqué en direct tant que la structure des sections correspond aux
  // blocs de la page (sinon, avertissement et pas d'application).
  const [codeText, setCodeText] = useState('')
  const [codeErr, setCodeErr] = useState(false)
  const fromCodeRef = useRef(false)
  const textStyleName = (s: TextStyle) => s === 'h1' ? 'titre' : s === 'h2' ? 'sous-titre' : 'paragraphe'
  // Every block parameter is exposed (and editable) in the section header:
  //   %%[formule 1 | x=24 y=24 w=746 h=auto r=0 échelle=100% align=center num=non]
  const fmtAttrs = (b: UIBlock): string => {
    const f = b.frame
    const parts = [`x=${f.x}`, `y=${f.y}`, `w=${f.w}`]
    if (b.type === 'graph') parts.push(`h=${b.spec.height}`)
    else parts.push(`h=${f.h ?? 'auto'}`)
    parts.push(`r=${f.r ?? 0}`)
    if (b.type !== 'graph') parts.push(`échelle=${Math.round((b.scale ?? 1) * 100)}%`)
    if (b.type === 'formula') {
      parts.push(`align=${b.align ?? 'center'}`, `num=${b.numbered ? 'oui' : 'non'}`)
    }
    if (b.type === 'graph') {
      const s = b.spec
      parts.push(`xmin=${s.xmin}`, `xmax=${s.xmax}`, `ymin=${s.ymin ?? 'auto'}`, `ymax=${s.ymax ?? 'auto'}`,
        `grille=${s.grid ? 'oui' : 'non'}`, `axes=${s.axisNumbers ? 'oui' : 'non'}`)
    }
    return parts.join(' ')
  }
  const genPageCode = (p: UIPage): string =>
    p.blocks.map((b, i) => {
      const head = b.type === 'formula' ? `formule ${i + 1}`
        : b.type === 'text' ? `texte ${i + 1} · ${textStyleName(b.style ?? 'p')}`
        : `graphe ${i + 1}`
      const body = b.type === 'formula' ? b.latex : b.type === 'text' ? b.text : b.spec.fns.map(f => f.expr).join('\n')
      return `%%[${head} | ${fmtAttrs(b)}]\n${body}\n`
    }).join('\n')
  // Apply the header attributes back to a block (absent / unparseable ⇒ unchanged).
  const applyAttrs = (b: UIBlock, rest: string): UIBlock => {
    const attrs: Record<string, string> = {}
    const re = /([a-zà-ÿ]+)\s*=\s*([-\d.a-z%]+)/gi
    let m: RegExpExecArray | null
    while ((m = re.exec(rest)) !== null) attrs[m[1].toLowerCase()] = m[2].toLowerCase()
    const num = (k: string): number | null => { const v = parseFloat(attrs[k] ?? ''); return isFinite(v) ? v : null }
    const nb: UIBlock = { ...b, frame: { ...b.frame } }
    const w = num('w'); if (w != null) nb.frame.w = clampN(Math.round(w), 120, sheet.w)
    const x = num('x'); if (x != null) nb.frame.x = clampN(Math.round(x), 0, Math.max(0, sheet.w - nb.frame.w))
    const y = num('y'); if (y != null) nb.frame.y = Math.max(0, Math.round(y))
    const r = num('r'); if (r != null) nb.frame.r = (((Math.round(r) % 360) + 360) % 360) || undefined
    if (nb.type === 'graph') {
      const spec = { ...nb.spec }
      const h = num('h'); if (h != null) spec.height = Math.max(120, Math.round(h))
      const xmin = num('xmin'); if (xmin != null) spec.xmin = xmin
      const xmax = num('xmax'); if (xmax != null) spec.xmax = xmax
      if (attrs.ymin === 'auto') spec.ymin = null; else { const v = num('ymin'); if (v != null) spec.ymin = v }
      if (attrs.ymax === 'auto') spec.ymax = null; else { const v = num('ymax'); if (v != null) spec.ymax = v }
      if (attrs.grille) spec.grid = attrs.grille !== 'non'
      if (attrs.axes) spec.axisNumbers = attrs.axes !== 'non'
      nb.spec = spec
    } else {
      if (attrs.h === 'auto') nb.frame.h = undefined
      else { const h = num('h'); if (h != null) nb.frame.h = Math.max(40, Math.round(h)) }
      const sc = num('échelle') ?? num('echelle') ?? num('scale')
      if (sc != null) nb.scale = Math.abs(sc / 100 - 1) < 0.01 ? undefined : Math.max(0.2, Math.min(8, sc / 100))
      if (nb.type === 'formula') {
        if (attrs.align === 'left' || attrs.align === 'right' || attrs.align === 'center') nb.align = attrs.align === 'center' ? undefined : attrs.align
        if (attrs.num) nb.numbered = attrs.num === 'oui' || attrs.num === '1' || undefined
      }
      if (nb.type === 'text') {
        const st = /(sous-titre|titre|paragraphe)/i.exec(rest)?.[1]?.toLowerCase()
        if (st) nb.style = st === 'titre' ? 'h1' : st === 'sous-titre' ? 'h2' : 'p'
      }
    }
    return nb
  }
  // Regenerated when entering code mode, switching page, or when the blocks change from
  // OUTSIDE the code editor (e.g. Properties panel) — never while typing here, so the
  // caret does not jump.
  useEffect(() => {
    if (mode !== 'code') return
    if (fromCodeRef.current) { fromCodeRef.current = false; return }
    setCodeText(genPageCode(page)); setCodeErr(false)
  }, [mode, pageIdx, page.blocks]) // eslint-disable-line react-hooks/exhaustive-deps
  // The textarea stays responsive: `codeText` updates on every keystroke, while the heavy
  // propagation to the blocks (setBlocks → sheet/eval/statusbar) is debounced. Without this,
  // long renders make the controlled textarea drop keystrokes typed mid-render.
  const applyTimerRef = useRef<number | null>(null)
  const pageBlocksRef = useRef(page.blocks)
  pageBlocksRef.current = page.blocks
  useEffect(() => () => { if (applyTimerRef.current) window.clearTimeout(applyTimerRef.current) }, [])
  const applyCode = (v: string) => {
    setCodeText(v)
    if (applyTimerRef.current) window.clearTimeout(applyTimerRef.current)
    applyTimerRef.current = window.setTimeout(() => doApplyCode(v), 180)
  }
  const doApplyCode = (v: string) => {
    const sections: { type: string; rest: string; lines: string[] }[] = []
    for (const line of v.split('\n')) {
      const m = /^%%\[(formule|texte|graphe)\b([^\]]*)\]\s*$/i.exec(line.trim())
      if (m) { sections.push({ type: m[1].toLowerCase(), rest: m[2], lines: [] }); continue }
      if (sections.length) sections[sections.length - 1].lines.push(line)
    }
    const blocks = pageBlocksRef.current
    const ok = sections.length === blocks.length && sections.every((s, i) =>
      (s.type === 'formule' && blocks[i].type === 'formula')
      || (s.type === 'texte' && blocks[i].type === 'text')
      || (s.type === 'graphe' && blocks[i].type === 'graph'))
    setCodeErr(!ok)
    if (!ok) return
    const content = (s: { lines: string[] }) => {
      let end = s.lines.length
      while (end > 0 && s.lines[end - 1].trim() === '') end--
      return s.lines.slice(0, end).join('\n')
    }
    fromCodeRef.current = true
    setBlocks(bs => bs.map((b, i) => {
      const c = content(sections[i])
      let nb = b
      if (b.type === 'formula') nb = { ...b, latex: c.trim() }
      else if (b.type === 'text') nb = { ...b, text: c }
      else {
        const exprs = c.split('\n').map(x => x.trim()).filter(Boolean)
        if (exprs.length) nb = { ...b, spec: { ...b.spec, fns: exprs.map((expr, k) => ({ expr, color: b.spec.fns[k]?.color ?? GRAPH_COLORS[k % GRAPH_COLORS.length] })) } }
      }
      return applyAttrs(nb, sections[i].rest)
    }))
  }
  // Palette insertion at the caret of the page-code editor (with the ‸ marker).
  const insertIntoPageCode = (tpl: MathTemplate) => {
    const el = taRefs.current['__page__']?.current
    const text = tpl.ins.replace(CARET, '')
    const caretRel = tpl.ins.indexOf(CARET)
    if (!el) { applyCode(codeText + text); return }
    const start = el.selectionStart, end = el.selectionEnd, cur = el.value
    applyCode(cur.slice(0, start) + text + cur.slice(end))
    const caret = caretRel >= 0 ? start + caretRel : start + text.length
    requestAnimationFrame(() => { el.focus(); el.setSelectionRange(caret, caret) })
  }

  // ── Palette / quick-toolbar / context-menu actions (target the active formula block) ──
  const onInsert = (tpl: MathTemplate) => {
    if (mode === 'code') { insertIntoPageCode(tpl); return }
    if (!targetId) { const id = nid(); const frame = newFrame(sheet.w); setBlocks(bs => [...bs, { id, type: 'formula', latex: tpl.tex, frame }]); setActiveBlockId(id); return }
    editorRefs.current[targetId]?.insertLatex(tpl.tex)
  }
  const insertTemplate = (tex: string) => ed()?.insertLatex(tex)
  // Insert a structure that works in BOTH modes: `tex` (visual, with \square holes) and
  // `code` (raw LaTeX, ‸ marks the caret). Routes through onInsert (creates a block if none).
  const insertBoth = (tex: string, code: string) => onInsert({ title: '', tex, ins: code } as MathTemplate)
  const wrapFrac = () => ed()?.frac()
  const wrapSup = () => ed()?.script('sup')
  const wrapSub = () => ed()?.script('sub')
  const deleteSlot = () => ed()?.deleteSlot()

  const onVisualContext = (e: React.MouseEvent) => { e.preventDefault(); e.stopPropagation(); setVMenu({ x: e.clientX, y: e.clientY }) }

  // ── Page operations ───────────────────────────────────────────────────────────
  const addPage = () => {
    const id = nid()
    const frame: BlockFrame = { x: SHEET_PAD, y: SHEET_PAD, w: paperPx(page.format, page.orientation).w - SHEET_PAD * 2 }
    setPages(prev => [...prev, { name: `Page ${prev.length + 1}`, blocks: [{ id, type: 'formula', latex: '', frame }], format: page.format, orientation: page.orientation }])
    setActive(pages.length)
  }
  const deletePage = (i: number) => { if (pages.length <= 1) return; setPages(prev => prev.filter((_, j) => j !== i)); setActive(a => Math.max(0, a >= i ? a - 1 : a)) }
  const renamePage = async (i: number) => { const v = await appPrompt(t('math_page_rename', { defaultValue: 'Nom de la page :' }), pages[i].name); if (v != null) setPages(prev => prev.map((p, j) => j === i ? { ...p, name: v || p.name } : p)) }

  const makeApi = () => {
    const Math = {
      getLatex: () => targetLatex(),
      setLatex: (src: unknown) => { if (targetId) setBlockLatex(targetId, String(src)) },
      getFormulaCount: () => formulaCount,
      getPageCount: () => pages.length,
      addPage: () => addPage(),
      evaluate: () => evalLatex(targetLatex()),
    }
    const App = { getType: () => 'math', getId: () => formula.id, toast: (m: unknown) => console.log(String(m)), log: (m: unknown) => console.log(String(m)) }
    return { Math, App }
  }

  // ── Ruban (façon MS Office) — construit ICI (l'éditeur détient l'état/les actions) et
  // remonté à MathsApp via onRibbonChange. Les vignettes KaTeX des galleries sont
  // mémoïsées (statiques) ; le ruban est reconstruit à chaque rendu (closures fraîches)
  // mais n'est POUSSÉ que lorsque sa signature change (comme le tableur). ─────────────
  const eqOpts = useMemo(() => ({
    struct:   EQ_STRUCTURES.map((it, i) => ({ value: String(i), label: it.label, icon: texIcon(it.p) })),
    bigops:   EQ_BIGOPS.map((it, i) => ({ value: String(i), label: it.label, icon: texIcon(it.p) })),
    brackets: EQ_BRACKETS.map((it, i) => ({ value: String(i), label: it.label, icon: texIcon(it.p) })),
    matrices: EQ_MATRICES.map((it, i) => ({ value: String(i), label: it.label, icon: texIcon(it.p) })),
    accents:  EQ_ACCENTS.map((it, i) => ({ value: String(i), label: it.label, icon: texIcon(it.p) })),
    greek:    SYM_GREEK.map(s => ({ value: s, label: s, icon: texIcon(s) })),
    ops:      SYM_OPS.map(s => ({ value: s, label: s, icon: texIcon(s) })),
    rels:     SYM_RELS.map(s => ({ value: s, label: s, icon: texIcon(s) })),
  }), [])
  const eqGal = (id: string, opts: { value: string; label: string; icon: React.ReactNode }[], list: EqItem[]): RibbonItem =>
    ({ id, kind: 'gallery', options: opts, onChange: (v) => { const it = list[Number(v)]; if (it) insertBoth(it.tex, it.code) } })
  const symGal = (id: string, opts: { value: string; label: string; icon: React.ReactNode }[]): RibbonItem =>
    ({ id, kind: 'gallery', options: opts, onChange: (s) => insertBoth(s, s) })

  const hasSel = !!activeBlockId
  const selFormula = selBlock && selBlock.type === 'formula' ? selBlock : null
  const curAlign: BlockAlign = selFormula?.align ?? 'center'
  const selScale = selBlock && selBlock.type !== 'graph' ? (selBlock.scale ?? 1) : 1
  const textSplit: RibbonItem = {
    id: 'ins-text', kind: 'split', icon: <Type size={15} />, label: t('math_ins_text', { defaultValue: 'Texte' }),
    onClick: () => addBlockBottom('p'),
    splitItems: [
      { id: 'ins-h1', kind: 'button', icon: <Heading1 size={15} />, label: t('math_style_title', { defaultValue: 'Titre' }), onClick: () => addBlockBottom('h1') },
      { id: 'ins-h2', kind: 'button', icon: <Heading2 size={15} />, label: t('math_style_subtitle', { defaultValue: 'Sous-titre' }), onClick: () => addBlockBottom('h2') },
      { id: 'ins-p', kind: 'button', icon: <Type size={15} />, label: t('math_style_paragraph', { defaultValue: 'Paragraphe' }), onClick: () => addBlockBottom('p') },
    ],
  }
  const insertBlocksGroup = {
    id: 'ins-blocks', label: t('math_grp_blocks', { defaultValue: 'Blocs' }), items: [
      { id: 'ins-formula', kind: 'button' as const, size: 'large' as const, icon: <Sigma size={20} />, label: t('math_ins_formula', { defaultValue: 'Formule' }), onClick: addFormulaBlock },
      { id: 'ins-graph', kind: 'button' as const, icon: <LineChart size={15} />, label: t('math_ins_graph', { defaultValue: 'Graphe' }), onClick: addGraphBlock },
      textSplit,
    ],
  }
  const viewToggles = [
    { id: 'm-visual', kind: 'toggle' as const, icon: <MousePointerSquareDashed size={15} />, label: t('math_mode_visual', { defaultValue: 'Visuel' }), active: mode === 'visual', onClick: () => setMode('visual') },
    { id: 'm-code', kind: 'toggle' as const, icon: <Code2 size={15} />, label: t('math_mode_code', { defaultValue: 'Code' }), active: mode === 'code', onClick: () => setMode('code') },
  ]
  const zoomLevels = [50, 75, 100, 125, 150, 200].map(z => ({ value: String(z), label: `${z} %` }))

  const editorRibbon: RibbonTab[] = [
    // Accueil : document, blocs, bloc sélectionné, vue.
    { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
      genericClipboardGroup(t),
      { id: 'doc', label: t('math_grp_doc', { defaultValue: 'Document' }), items: [
        { id: 'new', kind: 'button', size: 'large', icon: <FilePlus size={20} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: () => onNew?.() },
        { id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: () => onDuplicate?.() },
      ] },
      insertBlocksGroup,
      { id: 'block', label: t('math_grp_block', { defaultValue: 'Bloc' }), items: [
        { id: 'b-dup', kind: 'button', icon: <Copy size={15} />, label: t('math_block_dup', { defaultValue: 'Dupliquer' }), disabled: !hasSel, shortcut: 'Ctrl+D', onClick: () => activeBlockId && duplicateBlock(activeBlockId) },
        { id: 'b-kb', kind: 'button', icon: <Package size={15} />, label: t('math_block_copy_kubuno', { defaultValue: 'Copier pour Kubuno' }), onClick: () => copyForKubunoRef.current() },
        { id: 'b-del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete', { defaultValue: 'Supprimer' }), disabled: !hasSel, onClick: () => activeBlockId && removeBlock(activeBlockId) },
        { id: 'b-fwd', kind: 'button', icon: <ArrowUp size={15} />, label: t('math_block_forward', { defaultValue: 'Avancer' }), disabled: !hasSel, onClick: () => activeBlockId && moveBlock(activeBlockId, 1) },
        { id: 'b-bwd', kind: 'button', icon: <ArrowDown size={15} />, label: t('math_block_backward', { defaultValue: 'Reculer' }), disabled: !hasSel, onClick: () => activeBlockId && moveBlock(activeBlockId, -1) },
      ] },
      { id: 'view', label: t('math_grp_view', { defaultValue: 'Vue' }), items: viewToggles },
    ] },
    // Insertion : blocs + structures courantes + symboles + page.
    { id: 'insert', label: t('math_tab_insert', { defaultValue: 'Insertion' }), groups: [
      insertBlocksGroup,
      { id: 'ins-struct', label: t('math_grp_structures', { defaultValue: 'Structures' }), items: [eqGal('g-struct', eqOpts.struct, EQ_STRUCTURES)] },
      { id: 'ins-sym', label: t('math_grp_symbols', { defaultValue: 'Symboles' }), items: [symGal('g-greek0', eqOpts.greek)] },
      { id: 'ins-page', label: t('math_grp_page', { defaultValue: 'Page' }), items: [
        { id: 'add-page', kind: 'button', size: 'large', icon: <Plus size={20} />, label: t('math_add_page', { defaultValue: 'Ajouter une page' }), onClick: addPage },
      ] },
    ] },
    // Équation : galleries complètes de structures et symboles (façon onglet Équation de Word).
    { id: 'equation', label: t('math_tab_equation', { defaultValue: 'Équation' }), groups: [
      { id: 'eq-struct', label: t('math_grp_structures', { defaultValue: 'Structures' }), items: [eqGal('g-struct2', eqOpts.struct, EQ_STRUCTURES)] },
      { id: 'eq-bigops', label: t('math_grp_bigops', { defaultValue: 'Grands opérateurs' }), items: [eqGal('g-bigops', eqOpts.bigops, EQ_BIGOPS)] },
      { id: 'eq-brackets', label: t('math_grp_brackets', { defaultValue: 'Crochets' }), items: [eqGal('g-brackets', eqOpts.brackets, EQ_BRACKETS)] },
      { id: 'eq-matrices', label: t('math_grp_matrices', { defaultValue: 'Matrices' }), items: [eqGal('g-matrices', eqOpts.matrices, EQ_MATRICES)] },
      { id: 'eq-accents', label: t('math_grp_accents', { defaultValue: 'Accents' }), items: [eqGal('g-accents', eqOpts.accents, EQ_ACCENTS)] },
      { id: 'eq-greek', label: t('math_grp_greek', { defaultValue: 'Lettres grecques' }), items: [symGal('g-greek', eqOpts.greek)] },
      { id: 'eq-ops', label: t('math_grp_ops', { defaultValue: 'Opérateurs' }), items: [symGal('g-ops', eqOpts.ops)] },
      { id: 'eq-rels', label: t('math_grp_rels', { defaultValue: 'Relations' }), items: [symGal('g-rels', eqOpts.rels)] },
    ] },
    // Calcul : moteur symbolique (dérivée/intégrale/simplif/dev/factor/résolution/Taylor) + table + traçage.
    { id: 'calc', label: t('math_tab_calc', { defaultValue: 'Calcul' }), groups: [
      { id: 'calc-analysis', label: t('math_grp_analysis', { defaultValue: 'Analyse' }), items: [
        { id: 'c-deriv', kind: 'button', size: 'large', icon: texIcon("\\tfrac{d}{dx}"), label: t('math_cas_derivative', { defaultValue: 'Dérivée' }), tooltip: t('math_cas_derivative_tip', { defaultValue: 'Dérivée symbolique par rapport à x' }), onClick: doDerivative },
        { id: 'c-integ', kind: 'button', icon: texIcon('\\int'), label: t('math_cas_integral', { defaultValue: 'Primitive' }), tooltip: t('math_cas_integral_tip', { defaultValue: 'Intégrale indéfinie ∫ f dx' }), onClick: doIntegral },
        { id: 'c-area', kind: 'button', icon: <AreaChart size={15} />, label: t('math_cas_area', { defaultValue: 'Aire' }), tooltip: t('math_cas_area_tip', { defaultValue: 'Intégrale définie (aire sous la courbe)' }), onClick: doArea },
      ] },
      { id: 'calc-algebra', label: t('math_grp_algebra', { defaultValue: 'Algèbre' }), items: [
        { id: 'c-simp', kind: 'button', icon: <Sparkles size={15} />, label: t('math_cas_simplify', { defaultValue: 'Simplifier' }), onClick: doSimplify },
        { id: 'c-expand', kind: 'button', icon: <Expand size={15} />, label: t('math_cas_expand', { defaultValue: 'Développer' }), tooltip: t('math_cas_expand_tip', { defaultValue: 'Développer un produit de polynômes' }), onClick: doExpand },
        { id: 'c-factor', kind: 'button', icon: <Parentheses size={15} />, label: t('math_cas_factor', { defaultValue: 'Factoriser' }), tooltip: t('math_cas_factor_tip', { defaultValue: 'Factoriser un polynôme (degré ≤ 2)' }), onClick: doFactor },
      ] },
      { id: 'calc-solve', label: t('math_grp_solve', { defaultValue: 'Équations & séries' }), items: [
        { id: 'c-solve', kind: 'button', icon: <Target size={15} />, label: t('math_cas_solve', { defaultValue: 'Résoudre' }), tooltip: t('math_cas_solve_tip', { defaultValue: 'Racines réelles de f(x)=0 sur [-10, 10]' }), onClick: doSolve },
        { id: 'c-exact', kind: 'button', icon: texIcon('\\sqrt{\\Delta}'), label: t('math_cas_exact', { defaultValue: 'Racines exactes' }), tooltip: t('math_cas_exact_tip', { defaultValue: 'Racines exactes (radicaux/complexes, degré ≤ 2)' }), onClick: doSolveExact },
        { id: 'c-taylor', kind: 'button', icon: <Spline size={15} />, label: t('math_cas_taylor', { defaultValue: 'Taylor' }), tooltip: t('math_cas_taylor_tip', { defaultValue: 'Développement de Taylor (ordre ; centre)' }), onClick: doTaylor },
      ] },
      { id: 'calc-series', label: t('math_grp_sums', { defaultValue: 'Sommation' }), items: [
        { id: 'c-sum', kind: 'button', icon: texIcon('\\sum'), label: t('math_cas_sum', { defaultValue: 'Somme' }), tooltip: t('math_cas_sum_tip', { defaultValue: 'Somme finie Σ sur x de a à b' }), onClick: () => doAccumulate(sumLatex, t('math_cas_sum_done', { defaultValue: 'Somme calculée' })) },
        { id: 'c-prod', kind: 'button', icon: texIcon('\\prod'), label: t('math_cas_prod', { defaultValue: 'Produit' }), tooltip: t('math_cas_prod_tip', { defaultValue: 'Produit fini Π sur x de a à b' }), onClick: () => doAccumulate(productLatex, t('math_cas_prod_done', { defaultValue: 'Produit calculé' })) },
      ] },
      { id: 'calc-values', label: t('math_grp_values', { defaultValue: 'Valeurs' }), items: [
        { id: 'c-eval', kind: 'button', icon: <Calculator size={15} />, label: t('math_cas_eval', { defaultValue: 'Évaluer' }), tooltip: t('math_cas_eval_tip', { defaultValue: 'Insérer la valeur numérique' }), disabled: evalValue == null, onClick: insertResult },
        { id: 'c-evalpt', kind: 'button', icon: texIcon('f(a)'), label: t('math_cas_evalpt', { defaultValue: 'f(a)' }), tooltip: t('math_cas_evalpt_tip', { defaultValue: 'Évaluer f en un point donné' }), onClick: doEvalPoint },
        { id: 'c-table', kind: 'button', icon: <Table size={15} />, label: t('math_cas_table_btn', { defaultValue: 'Table' }), tooltip: t('math_cas_table_tip', { defaultValue: 'Table de valeurs de −3 à 3' }), onClick: doTable },
      ] },
      { id: 'calc-plot', label: t('math_grp_graph', { defaultValue: 'Graphe' }), items: [
        { id: 'c-plot', kind: 'button', size: 'large', icon: <LineChart size={20} />, label: t('math_cas_plot', { defaultValue: 'Tracer la formule' }), onClick: plotFormula },
      ] },
    ] },
    // Fonction : étude de fonction (dérivée 2nde, tangente, extrema, limite, comparaison f/f′).
    { id: 'func', label: t('math_tab_func', { defaultValue: 'Fonction' }), groups: [
      { id: 'fn-deriv', label: t('math_grp_derivation', { defaultValue: 'Dérivation' }), items: [
        { id: 'fn-d2', kind: 'button', size: 'large', icon: texIcon('\\tfrac{d^2}{dx^2}'), label: t('math_cas_d2', { defaultValue: 'Dérivée 2ⁿᵈᵉ' }), tooltip: t('math_cas_d2_tip', { defaultValue: 'Dérivée seconde' }), onClick: doSecondDeriv },
        { id: 'fn-tan', kind: 'button', icon: <Baseline size={15} />, label: t('math_cas_tangent', { defaultValue: 'Tangente' }), tooltip: t('math_cas_tangent_tip', { defaultValue: 'Équation de la tangente en un point + tracé' }), onClick: doTangent },
      ] },
      { id: 'fn-var', label: t('math_grp_variations', { defaultValue: 'Variations' }), items: [
        { id: 'fn-ext', kind: 'button', icon: <Waypoints size={15} />, label: t('math_cas_extrema', { defaultValue: 'Extrema' }), tooltip: t('math_cas_extrema_tip', { defaultValue: 'Points critiques (min/max) sur [-10, 10]' }), onClick: doExtrema },
        { id: 'fn-infl', kind: 'button', icon: texIcon("f''"), label: t('math_cas_inflection', { defaultValue: 'Inflexion' }), tooltip: t('math_cas_inflection_tip', { defaultValue: "Points d'inflexion (racines de f'')" }), onClick: doInflection },
        { id: 'fn-asym', kind: 'button', icon: texIcon('\\infty'), label: t('math_cas_asymptotes', { defaultValue: 'Asymptotes' }), tooltip: t('math_cas_asymptotes_tip', { defaultValue: 'Asymptotes horizontales et verticales' }), onClick: doAsymptotes },
        { id: 'fn-lim', kind: 'button', icon: texIcon('\\lim'), label: t('math_cas_limit', { defaultValue: 'Limite' }), tooltip: t('math_cas_limit_tip', { defaultValue: 'Limite en un point ou à l\'infini' }), onClick: doLimit },
      ] },
      { id: 'fn-cmp', label: t('math_grp_compare', { defaultValue: 'Comparaison' }), items: [
        { id: 'fn-ff', kind: 'button', size: 'large', icon: <Spline size={20} />, label: t('math_cas_plot_ff', { defaultValue: 'Tracer f et f′' }), tooltip: t('math_cas_plot_ff_tip', { defaultValue: 'Courbe de f et de sa dérivée' }), onClick: plotWithDerivative },
      ] },
    ] },
    // Matrice : algèbre linéaire numérique sur le bloc matrice sélectionné.
    { id: 'matrix', label: t('math_tab_matrix', { defaultValue: 'Matrice' }), groups: [
      { id: 'mat-ops', label: t('math_grp_matops', { defaultValue: 'Opérations' }), items: [
        { id: 'm-det', kind: 'button', size: 'large', icon: texIcon('\\left|A\\right|'), label: t('math_mat_det', { defaultValue: 'Déterminant' }), tooltip: t('math_mat_det_tip', { defaultValue: 'Déterminant d\'une matrice carrée' }), onClick: () => doMatrix(determinantLatex, t('math_mat_det_done', { defaultValue: 'Déterminant calculé' })) },
        { id: 'm-inv', kind: 'button', icon: texIcon('A^{-1}'), label: t('math_mat_inverse', { defaultValue: 'Inverse' }), tooltip: t('math_mat_inverse_tip', { defaultValue: 'Matrice inverse' }), onClick: () => doMatrix(inverseLatex, t('math_mat_inverse_done', { defaultValue: 'Inverse calculée' })) },
        { id: 'm-tr', kind: 'button', icon: texIcon('A^{T}'), label: t('math_mat_transpose', { defaultValue: 'Transposée' }), onClick: () => doMatrix(transposeLatex, t('math_mat_transpose_done', { defaultValue: 'Transposée calculée' })) },
        { id: 'm-rk', kind: 'button', icon: texIcon('\\operatorname{rg}'), label: t('math_mat_rank', { defaultValue: 'Rang' }), onClick: () => doMatrix(rankLatex, t('math_mat_rank_done', { defaultValue: 'Rang calculé' })) },
        { id: 'm-trace', kind: 'button', icon: texIcon('\\operatorname{tr}'), label: t('math_mat_trace', { defaultValue: 'Trace' }), onClick: () => doMatrix(traceLatex, t('math_mat_trace_done', { defaultValue: 'Trace calculée' })) },
      ] },
      { id: 'mat-adv', label: t('math_grp_matadv', { defaultValue: 'Avancé' }), items: [
        { id: 'm-pow', kind: 'button', icon: texIcon('A^{n}'), label: t('math_mat_power', { defaultValue: 'Puissance' }), tooltip: t('math_mat_power_tip', { defaultValue: 'Puissance Aⁿ (n saisi)' }), onClick: doMatrixPower },
        { id: 'm-eig', kind: 'button', icon: texIcon('\\lambda'), label: t('math_mat_eigen', { defaultValue: 'Valeurs propres' }), tooltip: t('math_mat_eigen_tip', { defaultValue: 'Valeurs propres réelles' }), onClick: () => doMatrix(eigenvaluesLatex, t('math_mat_eigen_done', { defaultValue: 'Valeurs propres calculées' })) },
        { id: 'm-chi', kind: 'button', icon: texIcon('\\chi'), label: t('math_mat_charpoly', { defaultValue: 'Poly. caract.' }), tooltip: t('math_mat_charpoly_tip', { defaultValue: 'Polynôme caractéristique det(λI − A)' }), onClick: () => doMatrix(charPolyLatex, t('math_mat_charpoly_done', { defaultValue: 'Polynôme caractéristique inséré' })) },
        { id: 'm-rref', kind: 'button', icon: texIcon('\\sim'), label: t('math_mat_rref', { defaultValue: 'Échelonnée' }), tooltip: t('math_mat_rref_tip', { defaultValue: 'Forme échelonnée réduite (RREF)' }), onClick: () => doMatrix(rrefLatex, t('math_mat_rref_done', { defaultValue: 'Forme échelonnée insérée' })) },
        { id: 'm-sys', kind: 'button', icon: texIcon('\\{\\,'), label: t('math_mat_system', { defaultValue: 'Résoudre système' }), tooltip: t('math_mat_system_tip', { defaultValue: 'Système linéaire depuis une matrice augmentée [A | b]' }), onClick: () => doMatrix(solveSystemLatex, t('math_mat_system_done', { defaultValue: 'Système résolu' })) },
      ] },
    ] },
    // Données : statistiques descriptives, régression, arithmétique / combinatoire.
    { id: 'data', label: t('math_tab_data', { defaultValue: 'Données' }), groups: [
      { id: 'data-stats', label: t('math_grp_stats', { defaultValue: 'Statistiques' }), items: [
        { id: 'd-stats', kind: 'button', size: 'large', icon: texIcon('\\bar{x},\\sigma'), label: t('math_stat_summary', { defaultValue: 'Statistiques' }), tooltip: t('math_stat_summary_tip', { defaultValue: 'Moyenne, écart-type, min/max… d\'une liste' }), onClick: () => doMatrix(statsLatex, t('math_stat_summary_done', { defaultValue: 'Statistiques insérées' })) },
        { id: 'd-five', kind: 'button', icon: texIcon('Q_1,Q_3'), label: t('math_stat_five', { defaultValue: 'Quartiles' }), tooltip: t('math_stat_five_tip', { defaultValue: 'Min, Q1, médiane, Q3, max' }), onClick: () => doMatrix(fiveNumberLatex, t('math_stat_five_done', { defaultValue: 'Quartiles insérés' })) },
        { id: 'd-reg', kind: 'button', icon: texIcon('\\hat{y}'), label: t('math_stat_reg', { defaultValue: 'Régression' }), tooltip: t('math_stat_reg_tip', { defaultValue: 'Régression linéaire y = ax + b sur des points (x, y)' }), onClick: () => doMatrix(regressionLatex, t('math_stat_reg_done', { defaultValue: 'Régression insérée' })) },
      ] },
      { id: 'data-num', label: t('math_grp_numbers', { defaultValue: 'Nombres' }), items: [
        { id: 'd-prime', kind: 'button', icon: texIcon('p^{k}'), label: t('math_num_prime', { defaultValue: 'Facteurs premiers' }), tooltip: t('math_num_prime_tip', { defaultValue: 'Décomposition en facteurs premiers' }), onClick: () => doNumeric(t('math_num_prime_prompt', { defaultValue: 'Entier à décomposer :' }), '360', 1, primeFactorsLatex, t('math_num_prime_done', { defaultValue: 'Décomposition insérée' })) },
        { id: 'd-gcd', kind: 'button', icon: texIcon('\\gcd'), label: t('math_num_gcd', { defaultValue: 'PGCD / PPCM' }), tooltip: t('math_num_gcd_tip', { defaultValue: 'PGCD et PPCM de deux entiers' }), onClick: () => doNumeric(t('math_num_gcd_prompt', { defaultValue: 'Deux entiers (a ; b) :' }), '12 ; 18', 2, gcdLcmLatex, t('math_num_gcd_done', { defaultValue: 'PGCD/PPCM insérés' })) },
        { id: 'd-binom', kind: 'button', icon: texIcon('\\binom{n}{k}'), label: t('math_num_binom', { defaultValue: 'Combinaisons' }), tooltip: t('math_num_binom_tip', { defaultValue: 'C(n,k) et A(n,k)' }), onClick: () => doNumeric(t('math_num_binom_prompt', { defaultValue: 'n et k (n ; k) :' }), '5 ; 2', 2, binomialLatex, t('math_num_binom_done', { defaultValue: 'Coefficients insérés' })) },
        { id: 'd-fact', kind: 'button', icon: texIcon('n!'), label: t('math_num_fact', { defaultValue: 'Factorielle' }), tooltip: t('math_num_fact_tip', { defaultValue: 'n!' }), onClick: () => doNumeric(t('math_num_fact_prompt', { defaultValue: 'Entier n :' }), '6', 1, factorialLatex, t('math_num_fact_done', { defaultValue: 'Factorielle insérée' })) },
        { id: 'd-base', kind: 'button', icon: texIcon('101_2'), label: t('math_num_base', { defaultValue: 'Bases' }), tooltip: t('math_num_base_tip', { defaultValue: 'Conversion binaire / octal / hexadécimal' }), onClick: () => doNumeric(t('math_num_base_prompt', { defaultValue: 'Entier à convertir :' }), '255', 1, baseConvertLatex, t('math_num_base_done', { defaultValue: 'Conversions insérées' })) },
      ] },
    ] },
    // Disposition : page, alignement/numérotation, organisation, taille.
    { id: 'layout', label: t('math_tab_layout', { defaultValue: 'Disposition' }), groups: [
      { id: 'lay-page', label: t('math_grp_page', { defaultValue: 'Page' }), items: [
        { id: 'fmt', kind: 'dropdown', width: 140, value: page.format, options: PAGE_FORMATS.map(f => ({ value: f.id, label: f.name })), onChange: (v) => setPageMeta({ format: v }) },
        { id: 'orient', kind: 'dropdown', width: 120, value: page.orientation, options: [
          { value: 'portrait', label: t('math_portrait', { defaultValue: 'Portrait' }) },
          { value: 'landscape', label: t('math_landscape', { defaultValue: 'Paysage' }) },
        ], onChange: (v) => setPageMeta({ orientation: v as Orientation }) },
        { id: 'lay-addpage', kind: 'button', icon: <Plus size={15} />, label: t('math_add_page', { defaultValue: 'Ajouter une page' }), onClick: addPage },
        { id: 'lay-delpage', kind: 'button', icon: <Trash2 size={15} />, label: t('math_del_page', { defaultValue: 'Supprimer la page' }), disabled: pages.length <= 1, onClick: () => deletePage(pageIdx) },
      ] },
      { id: 'lay-align', label: t('math_grp_align', { defaultValue: 'Alignement' }), items: [
        { id: 'al-l', kind: 'toggle', icon: <AlignLeft size={15} />, tooltip: t('math_align_left', { defaultValue: 'Aligner à gauche' }), active: !!selFormula && curAlign === 'left', disabled: !selFormula, onClick: () => activeBlockId && setBlockAlign(activeBlockId, 'left') },
        { id: 'al-c', kind: 'toggle', icon: <AlignCenter size={15} />, tooltip: t('math_align_center', { defaultValue: 'Centrer' }), active: !!selFormula && curAlign === 'center', disabled: !selFormula, onClick: () => activeBlockId && setBlockAlign(activeBlockId, 'center') },
        { id: 'al-r', kind: 'toggle', icon: <AlignRight size={15} />, tooltip: t('math_align_right', { defaultValue: 'Aligner à droite' }), active: !!selFormula && curAlign === 'right', disabled: !selFormula, onClick: () => activeBlockId && setBlockAlign(activeBlockId, 'right') },
        { id: 'al-num', kind: 'toggle', icon: <ListOrdered size={15} />, label: t('math_block_number', { defaultValue: 'Numéroter' }), active: !!selFormula && !!selFormula.numbered, disabled: !selFormula, onClick: () => activeBlockId && toggleNumbered(activeBlockId) },
      ] },
      { id: 'lay-arrange', label: t('math_grp_arrange', { defaultValue: 'Organiser' }), items: [
        { id: 'ar-fwd', kind: 'button', icon: <ArrowUp size={15} />, label: t('math_block_forward', { defaultValue: 'Avancer' }), disabled: !hasSel, onClick: () => activeBlockId && moveBlock(activeBlockId, 1) },
        { id: 'ar-bwd', kind: 'button', icon: <ArrowDown size={15} />, label: t('math_block_backward', { defaultValue: 'Reculer' }), disabled: !hasSel, onClick: () => activeBlockId && moveBlock(activeBlockId, -1) },
        { id: 'ar-rot', kind: 'button', icon: <RotateCw size={15} />, label: t('math_reset_rotation', { defaultValue: 'Rotation 0°' }), disabled: !hasSel, onClick: () => activeBlockId && resetRotation(activeBlockId) },
      ] },
      { id: 'lay-size', label: t('math_grp_size', { defaultValue: 'Taille' }), items: [
        { id: 'scale', kind: 'dropdown', width: 96, value: String(Math.round(selScale * 100)), options: [50, 75, 100, 125, 150, 200].map(s => ({ value: String(s), label: `${s} %` })), onChange: (v) => activeBlockId && setBlockScale(activeBlockId, Number(v) / 100) },
      ] },
    ] },
    // Affichage : vues, zoom, options d'affichage.
    { id: 'display', label: t('math_tab_view', { defaultValue: 'Affichage' }), groups: [
      { id: 'disp-views', label: t('math_grp_view', { defaultValue: 'Vue' }), items: viewToggles },
      { id: 'disp-zoom', label: t('math_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'z-out', kind: 'button', icon: <Minus size={15} />, tooltip: t('math_zoom_out', { defaultValue: 'Zoom arrière' }), onClick: () => applyZoom(zoom - 10) },
        { id: 'z-lvl', kind: 'dropdown', width: 90, value: String(zoom), options: zoomLevels, onChange: (v) => applyZoom(Number(v)) },
        { id: 'z-in', kind: 'button', icon: <Plus size={15} />, tooltip: t('math_zoom_in', { defaultValue: 'Zoom avant' }), onClick: () => applyZoom(zoom + 10) },
        { id: 'z-fit', kind: 'button', icon: <Maximize2 size={15} />, label: t('math_fit_width', { defaultValue: 'Ajuster' }), onClick: fitToWidth },
      ] },
      { id: 'disp-show', label: t('math_grp_show', { defaultValue: 'Afficher' }), items: [
        { id: 'colorize', kind: 'toggle', icon: <Palette size={15} />, label: t('math_colorize', { defaultValue: 'Coloration' }), active: colorize, onClick: toggleColorize },
        { id: 'theme', kind: 'dropdown', width: 150, value: themeId, options: MATH_THEMES.map(th => ({ value: th.id, label: th.name })), onChange: (v) => pickTheme(v) },
        { id: 'grid', kind: 'toggle', icon: <Grid3x3 size={15} />, label: t('math_grid', { defaultValue: 'Grille' }), active: showGrid, onClick: toggleGrid },
      ] },
    ] },
  ]
  // Push the ribbon up only when its signature changes (avoids a render loop).
  const ribbonSig = JSON.stringify([mode, activeBlockId, selBlock?.type, curAlign, selFormula?.numbered, selScale, zoom, colorize, themeId, showGrid, page.format, page.orientation, pages.length, hasSel, evalValue == null])
  const lastRibbonSig = useRef('')
  useEffect(() => {
    if (ribbonSig !== lastRibbonSig.current) { lastRibbonSig.current = ribbonSig; onRibbonChange?.(editorRibbon) }
  }) // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="relative flex flex-col flex-1 min-w-0 min-h-0 bg-surface-1 overflow-hidden">
      {/* Toast de retour du moteur de calcul (dérivée/résolution…) — jamais silencieux. */}
      {casNote && (
        <div className="absolute left-1/2 -translate-x-1/2 top-3 z-[60] flex items-center gap-2 px-3.5 py-2 rounded-lg shadow-lg text-sm text-white"
          style={{ background: casNote.err ? '#d93025' : '#1a73e8' }}>
          {casNote.err ? <XIcon size={15} /> : <Check size={15} />}
          <span className="whitespace-nowrap">{casNote.text}</span>
        </div>
      )}
      <style>{`
        .mc-h{cursor:text}
        .mc-empty{outline:1.5px dashed #aab0b8;outline-offset:1px;border-radius:4px;color:#aab0b8;background:#f1f3f4;padding:.05em .28em;transition:outline .12s,background .12s,color .12s}
        .mc-empty:hover{outline:2px solid #1a73e8;background:#dbeafe;color:#1a73e8}
        /* KaTeX stacks cells (fractions, matrices…) under a transparent .vlist scaffold that overlays
           the actual atoms and swallows :hover. Let the pointer reach the real cells/atoms. */
        .mc-render .katex .vlist-t,.mc-render .katex .vlist-r,.mc-render .katex .vlist,.mc-render .katex .pstrut{pointer-events:none}
        .mc-render .mc-h,.mc-render .mc-empty{pointer-events:auto}
        .mc-sel{background:#d2e3fc;border-radius:2px}
        /* Caret drawn out of flow (absolute ::after) so it never enlarges the line box. The zero-size
           anchor sits on the baseline; the bar straddles it and scales with the local font-size. */
        .mc-caret{display:inline-block;width:0;position:relative;vertical-align:baseline}
        .mc-caret::after{content:"";position:absolute;left:-1px;bottom:0.08em;height:1.15em;border-left:2px solid #1a73e8;animation:mc-blink 1.05s steps(1) infinite}
        @keyframes mc-blink{50%{opacity:0}}
        /* Semantic formula coloring (toggle) — colour each kind of atom KaTeX already tags.
           Hues come from the active theme via CSS custom properties (--mc-*). */
        .mc-colorize .mord.mathnormal,.mc-colorize .mord.mathbb,.mc-colorize .mord.mathcal,.mc-colorize .mord.mathfrak,.mc-colorize .mord.mathscr{color:var(--mc-v,#1a73e8)} /* variables */
        .mc-colorize .mord:not(.mathnormal):not(.mathbb):not(.mathcal):not(.mathfrak):not(.mathscr):not(.accent){color:var(--mc-n,#188038)} /* numbers */
        .mc-colorize .mbin{color:var(--mc-op,#d93025)}                                      /* + − · × */
        .mc-colorize .mrel{color:var(--mc-rel,#9334e6)}                                     /* = < > → ≤ */
        .mc-colorize .mop{color:var(--mc-fn,#e8710a)}                                       /* sin lim ∑ ∫ */
        .mc-colorize .mopen,.mc-colorize .mclose,.mc-colorize .mpunct{color:var(--mc-del,#607d8b)} /* ( ) [ ] | , */
        .mc-colorize .mc-empty .mord{color:#9aa0a6}                                         /* keep □ placeholders grey */
      `}</style>
      <div className="flex flex-1 min-w-0 min-h-0 overflow-hidden">
        {/* Zone centrale — les panneaux Symboles / Propriétés sont ancrables (DockArea) */}
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          {/* Bascule Visuel / Code */}
          <div className="flex items-center gap-1 px-3 h-9 bg-surface-2 border-b border-border flex-shrink-0">
            <button onClick={() => setMode('visual')} className={`flex items-center gap-1.5 h-7 px-2.5 rounded text-xs ${mode === 'visual' ? 'bg-white shadow-sm text-primary font-medium' : 'text-text-secondary hover:bg-white/60'}`}>
              <MousePointerSquareDashed size={14} /> {t('math_mode_visual', { defaultValue: 'Visuel' })}
            </button>
            <button onClick={() => setMode('code')} className={`flex items-center gap-1.5 h-7 px-2.5 rounded text-xs ${mode === 'code' ? 'bg-white shadow-sm text-primary font-medium' : 'text-text-secondary hover:bg-white/60'}`}>
              <Code2 size={14} /> {t('math_mode_code', { defaultValue: 'Code' })}
            </button>
            <div className="ml-auto relative flex items-center">
              <button onClick={toggleColorize} title={t('math_colorize_tip', { defaultValue: 'Activer / désactiver la coloration des formules' })}
                aria-pressed={colorize}
                className={`flex items-center gap-1.5 h-7 pl-2.5 pr-2 rounded-l text-xs ${colorize ? 'bg-white shadow-sm text-primary font-medium' : 'text-text-secondary hover:bg-white/60'}`}>
                <Palette size={14} /> {t('math_colorize', { defaultValue: 'Coloration' })}
              </button>
              <button onClick={() => setThemeOpen(o => !o)} title={t('math_color_theme', { defaultValue: 'Choisir un thème de coloris' })}
                aria-expanded={themeOpen}
                className={`flex items-center h-7 px-1 rounded-r border-l border-black/5 ${themeOpen ? 'bg-white shadow-sm text-primary' : colorize ? 'bg-white shadow-sm text-primary/70 hover:text-primary' : 'text-text-secondary hover:bg-white/60'}`}>
                <ChevronDown size={13} />
              </button>
              {themeOpen && (
                <>
                  <div className="fixed inset-0 z-40" onPointerDown={() => setThemeOpen(false)} />
                  <div className="absolute right-0 top-9 z-50 w-72 max-h-[70vh] overflow-y-auto bg-white border border-border rounded-lg shadow-xl p-3">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs font-semibold text-text-secondary uppercase tracking-wide">{t('math_color_theme', { defaultValue: 'Thème de coloris' })}</span>
                      <button onClick={() => setThemeOpen(false)} className="text-text-tertiary hover:text-text-primary"><XIcon size={14} /></button>
                    </div>
                    <div className="grid grid-cols-1 gap-1">
                      {MATH_THEMES.map(th => (
                        <button key={th.id} onClick={() => pickTheme(th.id)}
                          className={`flex items-center gap-2 px-2 py-1.5 rounded text-left text-xs hover:bg-surface-1 ${themeId === th.id ? 'bg-primary/5 ring-1 ring-primary/30' : ''}`}>
                          <span className="flex flex-shrink-0 rounded overflow-hidden border border-border">
                            {[th.pal.v, th.pal.n, th.pal.op, th.pal.rel, th.pal.fn, th.pal.del].map((c, i) => (
                              <span key={i} style={{ background: c, width: 9, height: 16 }} />
                            ))}
                          </span>
                          <span className="flex-1 truncate text-text-primary">{th.name}</span>
                          {themeId === th.id && <Check size={13} className="text-primary flex-shrink-0" />}
                        </button>
                      ))}
                    </div>
                    <div className="mt-3 pt-2 border-t border-border">
                      <div className="flex items-center justify-between mb-1.5">
                        <span className="text-xs font-semibold text-text-secondary">{t('math_color_custom', { defaultValue: 'Personnaliser' })}</span>
                        {themeId === 'custom' && <span className="text-[10px] text-primary font-medium">{t('math_color_custom_active', { defaultValue: 'Personnalisé' })}</span>}
                      </div>
                      <div className="grid grid-cols-2 gap-x-3 gap-y-1.5">
                        {([
                          { key: 'v', label: t('math_role_var', { defaultValue: 'Variables' }) },
                          { key: 'n', label: t('math_role_num', { defaultValue: 'Nombres' }) },
                          { key: 'op', label: t('math_role_op', { defaultValue: 'Opérateurs' }) },
                          { key: 'rel', label: t('math_role_rel', { defaultValue: 'Relations' }) },
                          { key: 'fn', label: t('math_role_fn', { defaultValue: 'Fonctions' }) },
                          { key: 'del', label: t('math_role_del', { defaultValue: 'Délimiteurs' }) },
                        ] as { key: keyof MathPalette; label: string }[]).map(r => (
                          <div key={r.key} className="flex items-center gap-2">
                            <ColorField t={t} color={palette[r.key]} onChange={(hex) => editColor(r.key, hex)} width={22} height={18} />
                            <span className="text-[11px] text-text-secondary truncate">{r.label}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                </>
              )}
            </div>
            <div><MacrosMenu docType="math" docId={formula.id} buildApi={makeApi} defaultLabel={formula.name} /></div>
          </div>

          {/* Barre d'outils rapide (mode Visuel) — agit sur le bloc formule actif */}
          {mode === 'visual' && (
            <div className="flex items-center gap-1 px-2 h-9 border-b border-border bg-white flex-shrink-0 overflow-x-auto">
              {([
                { k: 'frac', lbl: '\\frac{a}{b}', fn: wrapFrac, tip: 'Fraction (/)' },
                { k: 'sup', lbl: 'x^{2}', fn: wrapSup, tip: 'Exposant (^)' },
                { k: 'sub', lbl: 'x_{n}', fn: wrapSub, tip: 'Indice (_)' },
                { k: 'sqrt', lbl: '\\sqrt{x}', fn: () => insertTemplate('\\sqrt{\\square}'), tip: 'Racine carrée' },
                { k: 'nroot', lbl: '\\sqrt[n]{x}', fn: () => insertTemplate('\\sqrt[\\square]{\\square}'), tip: 'Racine n-ième' },
                { k: 'paren', lbl: '(\\square)', fn: () => insertTemplate('\\left(\\square\\right)'), tip: 'Parenthèses' },
                { k: 'abs', lbl: '|\\square|', fn: () => insertTemplate('\\left|\\square\\right|'), tip: 'Valeur absolue' },
                { k: 'sum', lbl: '\\sum', fn: () => insertTemplate('\\sum_{\\square}^{\\square}'), tip: 'Somme' },
                { k: 'int', lbl: '\\int', fn: () => insertTemplate('\\int_{\\square}^{\\square}'), tip: 'Intégrale' },
                { k: 'lim', lbl: '\\lim', fn: () => insertTemplate('\\lim_{\\square\\to\\square}'), tip: 'Limite' },
                { k: 'mat', lbl: '\\begin{pmatrix}a\\end{pmatrix}', fn: () => insertTemplate('\\begin{pmatrix}\\square&\\square\\\\\\square&\\square\\end{pmatrix}'), tip: 'Matrice' },
                { k: 'vec', lbl: '\\vec{v}', fn: () => insertTemplate('\\vec{\\square}'), tip: 'Vecteur' },
              ] as const).map(b => (
                <button key={b.k} title={b.tip} onClick={b.fn}
                  className="flex items-center justify-center h-7 min-w-[36px] px-1.5 rounded border border-transparent hover:border-border hover:bg-surface-1"
                  dangerouslySetInnerHTML={{ __html: renderTex(b.lbl, false) }} />
              ))}
            </div>
          )}

          {/* Blocs librement positionnés / redimensionnés / pivotés sur la feuille au format choisi.
              La feuille est le viewport du DockArea ; Symboles et Propriétés s'ancrent autour. */}
          <DockArea
            panels={dockPanels}
            storageKey="kubuno:maths:dockLayout"
            defaultArrangement={{ left: [['objects'], ['symbols']], right: [['properties']] }}
            theme={dockTheme}
            viewportBg="#f1f3f4"
            moveTitle={t('math_dock_move', { defaultValue: 'Glisser pour déplacer / détacher' })}
            className="flex flex-1 min-w-0 min-h-0"
          >
          {mode === 'visual' ? (
          <div ref={scrollRef} className="flex-1 min-h-0 overflow-auto bg-surface-2 p-6"
            onDragOver={e => { if (e.dataTransfer.types.includes('application/x-kubuno-math-object')) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy' } }}
            onDrop={e => {
              const kind = e.dataTransfer.getData('application/x-kubuno-math-object')
              if (!kind) return
              e.preventDefault()
              addBlockAt(kind as 'formula' | 'graph' | TextStyle, e.clientX, e.clientY)
            }}>
            <div ref={sheetRef} className="relative mx-auto bg-white shadow-md"
              style={{
                width: sheet.w, minHeight: Math.max(sheet.h, contentBottom() + SHEET_PAD), zoom: zoom / 100,
                ...(showGrid ? {
                  backgroundImage: `linear-gradient(to right, #eceef1 1px, transparent 1px), linear-gradient(to bottom, #eceef1 1px, transparent 1px)`,
                  backgroundSize: `${GRID_STEP}px ${GRID_STEP}px`,
                } : {}),
              }}
              onMouseDown={e => { if (e.target === e.currentTarget) setActiveBlockId(null) }}>
              {page.blocks.map((b, bi) => (
                <div key={b.id} ref={el => { blockEls.current[b.id] = el }}
                  className="absolute"
                  style={{
                    left: b.frame.x, top: b.frame.y, width: b.frame.w,
                    zIndex: activeBlockId === b.id ? 40 : bi + 1,
                    transform: b.frame.r ? `rotate(${b.frame.r}deg)` : undefined,
                  }}
                  onMouseDownCapture={() => setActiveBlockId(b.id)} onFocusCapture={() => setActiveBlockId(b.id)}>
                  {/* Chrome du bloc sélectionné : réglages · plan avant/arrière · supprimer */}
                  <div className={`absolute -top-2.5 right-8 z-30 ${activeBlockId === b.id ? 'flex' : 'hidden'} items-center bg-white border border-border rounded shadow-sm`}>
                    {b.type === 'formula' && (
                      <>
                        <button onClick={() => cycleAlign(b.id)} title={t('math_block_align', { defaultValue: 'Alignement' })}
                          className="p-1 text-text-tertiary hover:text-text-primary">
                          {(b.align ?? 'center') === 'left' ? <AlignLeft size={13} /> : (b.align ?? 'center') === 'right' ? <AlignRight size={13} /> : <AlignCenter size={13} />}
                        </button>
                        <button onClick={() => toggleNumbered(b.id)} title={t('math_block_number', { defaultValue: 'Numéroter l\'équation' })}
                          className={`p-1 hover:text-text-primary ${b.numbered ? 'text-primary' : 'text-text-tertiary'}`}>
                          <ListOrdered size={13} />
                        </button>
                      </>
                    )}
                    {b.type === 'text' && (
                      <>
                        {([['h1', 'T1'], ['h2', 'T2'], ['p', '¶']] as [TextStyle, string][]).map(([st, lbl]) => (
                          <button key={st} onClick={() => setBlockTextStyle(b.id, st)}
                            title={st === 'h1' ? t('math_text_h1', { defaultValue: 'Titre' }) : st === 'h2' ? t('math_text_h2', { defaultValue: 'Sous-titre' }) : t('math_text_p', { defaultValue: 'Paragraphe' })}
                            className={`p-1 text-[10px] font-semibold leading-none hover:text-text-primary ${(b.style ?? 'p') === st ? 'text-primary' : 'text-text-tertiary'}`}>
                            {lbl}
                          </button>
                        ))}
                      </>
                    )}
                    <button disabled={bi === page.blocks.length - 1} onClick={() => moveBlock(b.id, 1)} title={t('math_block_front', { defaultValue: 'Avancer (premier plan)' })} className="p-1 text-text-tertiary hover:text-text-primary disabled:opacity-30"><ArrowUp size={13} /></button>
                    <button disabled={bi === 0} onClick={() => moveBlock(b.id, -1)} title={t('math_block_back', { defaultValue: 'Reculer (arrière-plan)' })} className="p-1 text-text-tertiary hover:text-text-primary disabled:opacity-30"><ArrowDown size={13} /></button>
                    <button disabled={page.blocks.length <= 1} onClick={() => removeBlock(b.id)} title={t('math_block_remove', { defaultValue: 'Supprimer le bloc' })} className="p-1 text-text-tertiary hover:text-danger disabled:opacity-30"><Trash2 size={13} /></button>
                  </div>

                  {b.type === 'graph' ? (
                    <GraphBlock spec={b.spec} onChange={spec => setGraphSpec(b.id, spec)} t={t} />
                  ) : b.type === 'text' ? (
                    <div className={`rounded-lg border bg-white px-4 py-2 ${activeBlockId === b.id ? 'border-primary/50 ring-1 ring-primary/15' : 'border-border'}`}
                      style={{ minHeight: b.frame.h }}>
                      <TextBlockEditor text={b.text} textStyle={b.style ?? 'p'} scale={b.scale ?? 1}
                        placeholder={t('math_text_ph', { defaultValue: 'Texte…' })}
                        onChange={v => setBlockText(b.id, v)} />
                    </div>
                  ) : (
                    <div onContextMenu={onVisualContext}
                      className={`relative rounded-lg border bg-white overflow-hidden flex flex-col ${targetId === b.id ? 'border-primary/50 ring-1 ring-primary/15' : 'border-border'} ${colorize ? 'mc-colorize' : ''}`}
                      style={{ ...(colorize ? colorStyle : {}), height: b.frame.h }}>
                      <MathTreeEditor key={b.id} ref={h => { editorRefs.current[b.id] = h }} value={b.latex} align={b.align ?? 'center'} scale={b.scale ?? 1} onChange={v => setBlockLatex(b.id, v)} />
                      {b.numbered && <span className="absolute right-3 top-1/2 -translate-y-1/2 text-sm text-text-secondary pointer-events-none">({eqNumbers.get(b.id)})</span>}
                    </div>
                  )}

                  {/* Overlay de sélection : contour + bords déplaçables + rotation + 8 poignées */}
                  {activeBlockId === b.id && (
                    <div className="absolute -inset-px z-20 pointer-events-none">
                      <div className="absolute inset-0 border-2 border-primary" />
                      {/* Bords = zones de déplacement (curseur move) */}
                      <div {...manip(b.id, 'move')} className="absolute -top-1 left-3 right-3 h-2.5 cursor-move touch-none" style={{ pointerEvents: 'auto' }} />
                      <div {...manip(b.id, 'move')} className="absolute -bottom-1 left-3 right-3 h-2.5 cursor-move touch-none" style={{ pointerEvents: 'auto' }} />
                      <div {...manip(b.id, 'move')} className="absolute -left-1 top-3 bottom-3 w-2.5 cursor-move touch-none" style={{ pointerEvents: 'auto' }} />
                      <div {...manip(b.id, 'move')} className="absolute -right-1 top-3 bottom-3 w-2.5 cursor-move touch-none" style={{ pointerEvents: 'auto' }} />
                      {/* Rotation : tige + poignée au-dessus du bord supérieur */}
                      <div className="absolute left-1/2 -translate-x-1/2 -top-6 w-0.5 h-6 bg-primary" />
                      <div {...manip(b.id, 'rotate')} title={t('math_block_rotate', { defaultValue: 'Pivoter (Maj = pas de 15°)' })}
                        className="absolute left-1/2 -translate-x-1/2 -top-9 w-5 h-5 rounded-full bg-white border-2 border-primary flex items-center justify-center cursor-grab active:cursor-grabbing touch-none"
                        style={{ pointerEvents: 'auto' }}>
                        <RotateCw size={11} className="text-primary" />
                      </div>
                      {/* Coins (cercles) */}
                      {([['nw', 'nwse-resize', { top: -5, left: -5 }], ['ne', 'nesw-resize', { top: -5, right: -5 }],
                         ['sw', 'nesw-resize', { bottom: -5, left: -5 }], ['se', 'nwse-resize', { bottom: -5, right: -5 }]] as [ManipMode, string, React.CSSProperties][]).map(([hm, cur, pos]) => (
                        <div key={hm} {...manip(b.id, hm)} className="absolute w-2.5 h-2.5 rounded-full bg-white border-2 border-primary touch-none"
                          style={{ ...pos, cursor: cur, pointerEvents: 'auto' }} />
                      ))}
                      {/* Bords (pilules) */}
                      <div {...manip(b.id, 'n')} className="absolute left-1/2 -translate-x-1/2 h-2 w-5 rounded-full bg-white border-2 border-primary touch-none" style={{ top: -4, cursor: 'ns-resize', pointerEvents: 'auto' }} />
                      <div {...manip(b.id, 's')} className="absolute left-1/2 -translate-x-1/2 h-2 w-5 rounded-full bg-white border-2 border-primary touch-none" style={{ bottom: -4, cursor: 'ns-resize', pointerEvents: 'auto' }} />
                      <div {...manip(b.id, 'w')} className="absolute top-1/2 -translate-y-1/2 w-2 h-5 rounded-full bg-white border-2 border-primary touch-none" style={{ left: -4, cursor: 'ew-resize', pointerEvents: 'auto' }} />
                      <div {...manip(b.id, 'e')} className="absolute top-1/2 -translate-y-1/2 w-2 h-5 rounded-full bg-white border-2 border-primary touch-none" style={{ right: -4, cursor: 'ew-resize', pointerEvents: 'auto' }} />
                    </div>
                  )}
                </div>
              ))}

              {/* Repères intelligents pendant déplacement / redimensionnement */}
              {snapGuides.map((g, i) => (
                <div key={`snap-${i}`} className="absolute pointer-events-none" style={{
                  zIndex: 60,
                  ...(g.axis === 'v'
                    ? { left: g.pos, top: Math.min(g.a, g.b), height: Math.abs(g.b - g.a), width: 1, background: '#e1149e', transform: 'translateX(-0.5px)' }
                    : { top: g.pos, left: Math.min(g.a, g.b), width: Math.abs(g.b - g.a), height: 1, background: '#e1149e', transform: 'translateY(-0.5px)' }),
                }} />
              ))}
            </div>

            {/* Ajouter un bloc (placé sous le contenu existant) */}
            <div className="flex items-center justify-center gap-2 pt-3 pb-1">
              <button onClick={addFormulaBlock} className="flex items-center gap-1.5 h-8 px-3 rounded-lg border border-dashed border-border text-xs text-text-secondary bg-white hover:border-primary/50 hover:text-primary">
                <Sigma size={14} /> {t('math_add_formula', { defaultValue: 'Formule' })}
              </button>
              <button onClick={addGraphBlock} className="flex items-center gap-1.5 h-8 px-3 rounded-lg border border-dashed border-border text-xs text-text-secondary bg-white hover:border-primary/50 hover:text-primary">
                <LineChart size={14} /> {t('math_add_graph', { defaultValue: 'Graphe' })}
              </button>
              <button onClick={addTextBlock} className="flex items-center gap-1.5 h-8 px-3 rounded-lg border border-dashed border-border text-xs text-text-secondary bg-white hover:border-primary/50 hover:text-primary">
                <Type size={14} /> {t('math_add_text', { defaultValue: 'Texte' })}
              </button>
            </div>
          </div>
          ) : (
          /* Mode Code : source de la PAGE COMPLÈTE (une section %%[…] par bloc) */
          <div className="flex-1 min-w-0 min-h-0 flex flex-col bg-white">
            <div className="flex items-center gap-2 px-3 h-9 border-b border-border flex-shrink-0 bg-surface-1">
              <span className="text-xs text-text-secondary flex-shrink-0">
                {t('math_code_page', { defaultValue: 'Code de la page' })} · {page.name}
              </span>
              {codeErr ? (
                <span className="ml-auto text-[11px] text-[#d93025] truncate">
                  {t('math_code_mismatch', { defaultValue: 'Sections %%[…] ≠ blocs de la page : modifications non appliquées (rétablissez les délimiteurs).' })}
                </span>
              ) : (
                <span className="ml-auto text-[11px] text-text-tertiary truncate">
                  {t('math_code_hint3', { defaultValue: 'Une section %%[…] par bloc de la feuille — appliqué en direct. Graphe : une fonction par ligne.' })}
                </span>
              )}
            </div>
            <LatexEditor value={codeText} onChange={applyCode} taRef={taRefOf('__page__')} />
          </div>
          )}
          </DockArea>

          {mode === 'visual' && (
            <div className="flex items-center gap-3 px-3 h-8 border-t border-border bg-surface-1 flex-shrink-0 text-[11px] text-text-tertiary">
              <span>{t('math_visual_hint4', { defaultValue: 'Tapez comme dans un texte · ← → ↑ ↓ se déplacer · / fraction · ^ exposant · _ indice · ( ) [ ] | délimiteurs · \\nom symbole · Entrée = ligne de matrice · clic droit = menu · Ctrl+molette = zoom' })}</span>
            </div>
          )}

          {/* Onglets de pages (façon feuilles de calcul) + format/orientation + barre de statut */}
          <div className="flex items-center gap-1 px-2 h-9 bg-surface-2 border-t border-border flex-shrink-0">
            <div className="flex items-center gap-1 flex-1 overflow-x-auto">
              {pages.map((p, i) => (
                <div key={i} onClick={() => setActive(i)} onDoubleClick={() => renamePage(i)}
                  className={`group flex items-center gap-1 h-6 pl-2.5 pr-1 rounded-t cursor-pointer whitespace-nowrap text-xs ${i === pageIdx ? 'bg-white text-primary font-medium shadow-sm' : 'text-text-secondary hover:bg-white/60'}`}>
                  {p.name}
                  {pages.length > 1 && <button onClick={e => { e.stopPropagation(); deletePage(i) }} className="opacity-0 group-hover:opacity-100 hover:text-danger"><XIcon size={11} /></button>}
                </div>
              ))}
              <button onClick={addPage} title={t('math_add_page', { defaultValue: 'Ajouter une page' })} className="flex items-center justify-center w-6 h-6 rounded hover:bg-white/70 text-text-secondary"><Plus size={14} /></button>
            </div>
            <div className="flex items-center gap-2 flex-shrink-0 pl-2">
              <Dropdown value={page.format} onChange={(v: string) => setPageMeta({ format: v })}
                options={PAGE_FORMATS.map(f => ({ value: f.id, label: f.name }))} />
              <Dropdown value={page.orientation} onChange={(v: string) => setPageMeta({ orientation: v as Orientation })}
                options={[
                  { value: 'portrait', label: t('math_portrait', { defaultValue: 'Portrait' }) },
                  { value: 'landscape', label: t('math_landscape', { defaultValue: 'Paysage' }) },
                ]} />
            </div>
          </div>
          <StatusBar>
            <div className="flex items-center px-2 text-text-secondary whitespace-nowrap">{page.name}</div>
            <StatusSep />
            <div className="flex items-center px-2 text-text-secondary whitespace-nowrap">{t('math_status_blocks_n', { count: page.blocks.length, defaultValue: `${page.blocks.length} bloc(s)` })}</div>
            {evalValue != null && (
              <>
                <StatusSep />
                <div className="flex items-center px-2 text-primary font-medium whitespace-nowrap" title={t('math_status_eval_tip', { defaultValue: 'Valeur numérique de la formule active' })}>
                  = {fmtResult(evalValue)}
                </div>
              </>
            )}
            <StatusSpacer />
            {anyLatexError && <div className="flex items-center px-2 text-[#d93025] whitespace-nowrap">{t('math_status_latex_error', { defaultValue: 'Erreur LaTeX' })}</div>}
            <StatusSep />
            <div className="flex items-center gap-0.5 px-1">
              <button onClick={() => applyZoom(zoom - 10)} title={t('math_zoom_out', { defaultValue: 'Zoom arrière' })}
                className="flex items-center justify-center w-5 h-5 rounded hover:bg-surface-2 text-text-secondary"><Minus size={12} /></button>
              <button onClick={() => applyZoom(100)} title={t('math_zoom_reset', { defaultValue: 'Réinitialiser le zoom' })}
                className="px-1.5 h-5 rounded hover:bg-surface-2 text-text-secondary whitespace-nowrap tabular-nums">{zoom} %</button>
              <button onClick={() => applyZoom(zoom + 10)} title={t('math_zoom_in', { defaultValue: 'Zoom avant' })}
                className="flex items-center justify-center w-5 h-5 rounded hover:bg-surface-2 text-text-secondary"><Plus size={12} /></button>
            </div>
          </StatusBar>
        </div>
      </div>
      {vMenu && <MenuDropdown pos={{ top: vMenu.y, left: vMenu.x }} onClose={() => setVMenu(null)} items={[
        { type: 'submenu', label: t('math_ctx_insert', { defaultValue: 'Insérer ici' }), items: [
          { type: 'action', label: t('math_t_frac', { defaultValue: 'Fraction' }), onClick: wrapFrac },
          { type: 'action', label: t('math_t_sup', { defaultValue: 'Exposant' }), onClick: wrapSup },
          { type: 'action', label: t('math_t_sub', { defaultValue: 'Indice' }), onClick: wrapSub },
          { type: 'action', label: t('math_t_sqrt', { defaultValue: 'Racine carrée' }), onClick: () => insertTemplate('\\sqrt{\\square}') },
          { type: 'action', label: t('math_t_nroot', { defaultValue: 'Racine n-ième' }), onClick: () => insertTemplate('\\sqrt[\\square]{\\square}') },
          { type: 'action', label: t('math_t_paren', { defaultValue: 'Parenthèses' }), onClick: () => insertTemplate('\\left(\\square\\right)') },
          { type: 'action', label: t('math_t_bracket', { defaultValue: 'Crochets' }), onClick: () => insertTemplate('\\left[\\square\\right]') },
          { type: 'action', label: t('math_t_abs', { defaultValue: 'Valeur absolue' }), onClick: () => insertTemplate('\\left|\\square\\right|') },
          { type: 'action', label: t('math_t_vec', { defaultValue: 'Vecteur' }), onClick: () => insertTemplate('\\vec{\\square}') },
          { type: 'action', label: t('math_t_hat', { defaultValue: 'Chapeau' }), onClick: () => insertTemplate('\\hat{\\square}') },
          { type: 'action', label: t('math_t_overline', { defaultValue: 'Surligne' }), onClick: () => insertTemplate('\\overline{\\square}') },
          { type: 'action', label: t('math_t_sum', { defaultValue: 'Somme' }), onClick: () => insertTemplate('\\sum_{\\square}^{\\square}') },
          { type: 'action', label: t('math_t_prod', { defaultValue: 'Produit' }), onClick: () => insertTemplate('\\prod_{\\square}^{\\square}') },
          { type: 'action', label: t('math_t_int', { defaultValue: 'Intégrale' }), onClick: () => insertTemplate('\\int_{\\square}^{\\square}') },
          { type: 'action', label: t('math_t_lim', { defaultValue: 'Limite' }), onClick: () => insertTemplate('\\lim_{\\square\\to\\square}') },
          { type: 'action', label: t('math_t_matrix', { defaultValue: 'Matrice 2×2' }), onClick: () => insertTemplate('\\begin{pmatrix}\\square&\\square\\\\\\square&\\square\\end{pmatrix}') },
          { type: 'action', label: t('math_t_cases', { defaultValue: 'Système (accolade)' }), onClick: () => insertTemplate('\\begin{cases}\\square\\\\\\square\\end{cases}') },
        ] },
        ...(ed()?.inMatrix() ? [
          { type: 'separator' },
          { type: 'submenu', label: t('math_ctx_matrix', { defaultValue: 'Matrice' }), items: [
            { type: 'action', label: t('math_mat_add_row', { defaultValue: 'Ajouter une ligne' }), onClick: () => ed()?.matrixOp('addRow') },
            { type: 'action', label: t('math_mat_add_col', { defaultValue: 'Ajouter une colonne' }), onClick: () => ed()?.matrixOp('addCol') },
            { type: 'separator' },
            { type: 'action', label: t('math_mat_del_row', { defaultValue: 'Supprimer la ligne' }), danger: true, onClick: () => ed()?.matrixOp('delRow') },
            { type: 'action', label: t('math_mat_del_col', { defaultValue: 'Supprimer la colonne' }), danger: true, onClick: () => ed()?.matrixOp('delCol') },
          ] },
        ] satisfies MenuItem[] : []),
        ...(evalValue != null ? [
          { type: 'separator' },
          { type: 'action', label: `${t('math_ctx_insert_result', { defaultValue: 'Insérer le résultat' })} (= ${fmtResult(evalValue)})`, onClick: insertResult },
        ] satisfies MenuItem[] : []),
        { type: 'separator' },
        { type: 'action', label: t('math_ctx_del_slot', { defaultValue: 'Supprimer l\'emplacement' }), onClick: deleteSlot },
        { type: 'action', label: t('math_ctx_clear', { defaultValue: 'Vider la formule' }), danger: true, onClick: () => ed()?.clear() },
        { type: 'separator' },
        { type: 'action', label: t('math_ctx_copy', { defaultValue: 'Copier le LaTeX' }), onClick: () => { navigator.clipboard?.writeText(targetLatex()).catch(() => {}) } },
        // Cross-module copy: JSON envelope pasteable as a rendered formula in
        // chat, notes, documents… (`core.data-card`).
        { type: 'action', label: t('math_ctx_copy_kubuno', { defaultValue: 'Copier pour Kubuno' }), onClick: copyForKubuno },
      ] satisfies MenuItem[]} />}
    </div>
  )
}

// ── Contenu d'accueil (réutilisé par la landing ET le backstage de l'éditeur) ────

function MathsStartContent({ recentItems, onNew, onOpenFile }: {
  recentItems: StartPageRecentItem[]
  onNew: () => void
  onOpenFile: (file: FileItem) => boolean
}) {
  const { t } = useTranslation('office')
  return (
    <ModuleStartPage
      recentTitle={t('math_recent', { defaultValue: 'Récents' })}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <Sigma size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
          <p className="text-text-tertiary text-xs">{t('math_select_or_create', { defaultValue: 'Créez une formule ou ouvrez-en une existante' })}</p>
        </div>
      }
      browse={{
        folderPathPrefix: 'Office/Maths',
        title: t('math_title', { defaultValue: 'Maths' }),
        fileTypeModuleId: 'office-maths',
        onOpenFile,
        toolbarContent: (
          <Button icon={<Plus size={15} />} onClick={onNew}>
            {t('math_new', { defaultValue: 'Nouvelle formule' })}
          </Button>
        ),
      }}
    />
  )
}

// ── Application Maths ───────────────────────────────────────────────────────────

export default function MathsApp() {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const { showOpenError, openErrorDialog } = useOpenError(t)
  const { id: routeId } = useParams<{ id: string }>()
  const [formulas, setFormulas]   = useState<MathFormula[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [loading, setLoading]     = useState(true)
  // The open editor builds its own rich ribbon (structures, layout, view…) and pushes it up.
  const [editorRibbon, setEditorRibbon] = useState<RibbonTab[]>([])

  const load = useCallback(async () => {
    const data = await formulasApi.list()
    setFormulas(data.formulas)
    setLoading(false)
  }, [])
  useEffect(() => { load() }, [load])

  // `formulasApi.list()` ne renvoie PAS le LaTeX (le contenu vit dans le .kbmath, lu
  // seulement par get/open-by-file). On suit donc quelles formules ont leur contenu
  // chargé pour n'ouvrir l'éditeur qu'avec le vrai LaTeX (sinon on l'écraserait à vide).
  const [loadedIds, setLoadedIds] = useState<Set<string>>(() => new Set())
  const upsert      = (f: MathFormula) => setFormulas(prev => prev.some(x => x.id === f.id) ? prev.map(x => x.id === f.id ? f : x) : [f, ...prev])
  const markLoaded  = (id: string) => setLoadedIds(s => s.has(id) ? s : new Set(s).add(id))

  const selected = formulas.find(f => f.id === selectedId) ?? null

  // Charge le contenu complet de la formule sélectionnée (si pas déjà chargé).
  useEffect(() => {
    if (!selectedId || loadedIds.has(selectedId)) return
    let cancel = false
    formulasApi.get(selectedId).then(({ formula }) => {
      if (cancel) return
      upsert(formula); markLoaded(formula.id)
    }).catch(() => {})
    return () => { cancel = true }
  }, [selectedId, loadedIds]) // eslint-disable-line react-hooks/exhaustive-deps

  // Ouverture par URL (/office/maths/:id) — ex. double-clic d'un .kbmath dans files.
  useEffect(() => {
    if (!routeId) return
    formulasApi.get(routeId).then(({ formula }) => {
      upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
    }).catch(() => {})
  }, [routeId]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleUpdate = (u: MathFormula) => setFormulas(prev => prev.map(f => f.id === u.id ? { ...f, ...u } : f))
  // Annuler/Rétablir : l'historique vit DANS l'éditeur (au niveau de son état `pages`, le vrai
  // état édité). L'éditeur remonte ses commandes ici pour piloter les boutons de l'en-tête.
  const [hist, setHist] = useState<HistoryState>({ undo: () => {}, redo: () => {}, canUndo: false, canRedo: false })

  // Immediate save wired to the title-bar SaveButton. The editor view fills `saveRef` with
  // a function that persists the current document (same path as the debounced autosave).
  const saveRef = useRef<(() => Promise<void>) | null>(null)
  // Clean print of the open document (filled by the editor view; falls back to window.print).
  const printRef = useRef<(() => void) | null>(null)
  const [saving, setSaving] = useState(false)
  // Ctrl+S / ⌘S saves immediately.
  useSaveShortcut(() => { void handleSave() })

  const handleSave = async () => {
    if (!saveRef.current || saving) return
    setSaving(true)
    try { await saveRef.current() } finally { setSaving(false) }
  }

  async function handleNew() {
    const { formula } = await formulasApi.create({ name: t('math_new', { defaultValue: 'Nouvelle formule' }) })
    upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
  }
  async function handleTrash() {
    if (!selected) return
    await formulasApi.trash(selected.id)
    setFormulas(prev => prev.filter(f => f.id !== selected.id))
    setSelectedId(null)
  }
  async function handleDuplicate() {
    if (!selected) return
    const { formula } = await formulasApi.duplicate(selected.id)
    upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
  }
  // Toggle favorite (star) — persists is_starred then patches local state.
  async function handleStar() {
    if (!selected) return
    const { formula } = await formulasApi.update(selected.id, { is_starred: !selected.is_starred })
    handleUpdate(formula)
  }

  const handleOpenFile = (file: FileItem): boolean => {
    formulasApi.openByFile(file.id).then(({ formula }) => {
      upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
    }).catch(showOpenError)
    return true
  }

  // Titre éditable (WorkspaceShell) — nom de la formule sélectionnée.
  const [titleDraft, setTitleDraft] = useState('')
  useEffect(() => { setTitleDraft(selected?.name ?? '') }, [selected?.name])
  const commitTitle = async () => {
    if (!selected) return
    const v = titleDraft.trim()
    if (v && v !== selected.name) {
      const data = await formulasApi.update(selected.id, { name: v })
      handleUpdate(data.formula)
    } else if (!v) {
      setTitleDraft(selected.name)
    }
  }

  // Liste « Récents » de l'accueil — partagée par la landing ET le backstage.
  const recentItems: StartPageRecentItem[] = formulas.slice(0, 12).map(f => ({
    id:       f.id,
    name:     f.name,
    subtitle: format(new Date(f.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
    icon:     <Sigma size={18} className="text-text-tertiary" strokeWidth={1.5} />,
    onClick:  () => setSelectedId(f.id),
    actions: [
      { id: 'open',  label: t('common_open', { defaultValue: 'Ouvrir' }), icon: <ExternalLink size={15} />, onClick: () => setSelectedId(f.id) },
      { id: 'dup',   label: t('common_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={15} />, onClick: () => { formulasApi.duplicate(f.id).then(d => { upsert(d.formula); markLoaded(d.formula.id); setSelectedId(d.formula.id) }) } },
      { id: 'trash', label: t('math_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => { formulasApi.trash(f.id).then(() => setFormulas(prev => prev.filter(x => x.id !== f.id))) } },
    ],
  }))

  // Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du ruban
  // de l'éditeur ouvert. Appelé avant tout return anticipé (`selected` peut être null ici).
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_MATHS,
    labels: backstageLabels(t),
    startContent: <MathsStartContent recentItems={recentItems} onNew={handleNew} onOpenFile={handleOpenFile} />,
    defaultTab: 'home',
    openKey: selectedId,
    doc: {
      info: (
        <BackstageInfo
          title={titleDraft}
          onTitleChange={setTitleDraft}
          onTitleCommit={commitTitle}
          extension=".kbmath"
          subtitle={t('math_title', { defaultValue: 'Maths' })}
          general={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('math_title', { defaultValue: 'Maths' })],
            ...(selected?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(selected.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
        />
      ),
      onPrint: () => { (printRef.current ?? (() => window.print()))() },
      onClose: () => setSelectedId(null),
    },
  })

  if (loading) {
    return <div className="flex h-full items-center justify-center text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</div>
  }

  // Accueil (aucune formule ouverte).
  if (!selected) {
    return (
      <>
        {openErrorDialog}
        <ModuleHome
          theme={THEME_MATHS}
          title={t('math_title', { defaultValue: 'Maths' })}
          titleIcon={<Sigma size={16} className="text-white/90 flex-shrink-0" />}
          fileLabel={t('doc_bs_file', { defaultValue: 'Fichier' })}
          homeLabel={t('doc_bs_home', { defaultValue: 'Accueil' })}
          onBack={() => navigate('/office')}
          startContent={<MathsStartContent recentItems={recentItems} onNew={handleNew} onOpenFile={handleOpenFile} />}
        />
      </>
    )
  }

  return (
    <>
    {openErrorDialog}
    <OfficeShell
      ribbon={[fileTab, ...(editorRibbon.length ? editorRibbon : [{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
        groups: [{ id: 'formula', label: t('math_title', { defaultValue: 'Formule' }), items: [
          { id: 'new', kind: 'button' as const, icon: <FilePlus size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: handleNew },
          { id: 'dup', kind: 'button' as const, icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: handleDuplicate },
        ] }] }])]}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_MATHS}
      chromeless
      topbarHeight={64}
      titleIcon={<Sigma size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onBack={() => setSelectedId(null)}
      onTitleChange={setTitleDraft}
      onTitleCommit={commitTitle}
      titlePlaceholder={t('common_untitled', { defaultValue: 'Sans titre' })}
      titleActions={<>
        <SaveButton onSave={handleSave} saving={saving} label={t('doc_save', { defaultValue: 'Enregistrer' })} />
        {selected && (
          <UndoRedoButtons onUndo={hist.undo} onRedo={hist.redo} canUndo={hist.canUndo} canRedo={hist.canRedo}
            undoLabel={t('doc_undo', { defaultValue: 'Annuler' })} redoLabel={t('doc_redo', { defaultValue: 'Rétablir' })} />
        )}
        {selected && (
          <button onClick={handleStar}
            className={`p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 ${selected.is_starred ? 'text-warning' : 'text-white/90'}`}
            title={selected.is_starred ? t('math_unstar', { defaultValue: 'Retirer des favoris' }) : t('math_star', { defaultValue: 'Ajouter aux favoris' })}>
            <Star size={15} className={selected.is_starred ? 'fill-warning text-warning' : ''} />
          </button>
        )}
      </>}
      onDelete={handleTrash}
      deleteTitle={t('math_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('math_delete_confirm_title', { defaultValue: 'Supprimer cette formule ?' }),
        message: t('math_delete_confirm_msg', { defaultValue: 'La formule sera déplacée dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
    >
      {loadedIds.has(selected.id)
        ? <FormulaEditorView key={selected.id} formula={selected} onUpdate={handleUpdate} formulaCount={formulas.length} saveRef={saveRef} printRef={printRef} onNew={handleNew} onDuplicate={handleDuplicate} onRibbonChange={setEditorRibbon} onHistory={setHist} />

        : <div className="flex flex-1 items-center justify-center text-text-tertiary text-sm">{t('common_loading', { defaultValue: 'Chargement…' })}</div>}
    </OfficeShell>
    </>
  )
}
