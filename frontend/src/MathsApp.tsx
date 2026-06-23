import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { Plus, Sigma, Trash2, ExternalLink, Copy, Code2, MousePointerSquareDashed, Palette, ChevronDown, Check, ArrowUp, ArrowDown, LineChart, GripVertical, X as XIcon, FilePlus, CopyPlus } from 'lucide-react'
import katex from 'katex'
import 'katex/dist/katex.min.css'
import { Button, Dropdown, MenuDropdown, ColorField } from '@ui'
import type { StartPageRecentItem, MenuItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import { ModuleHome, useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'
import type { FileItem } from '@kubuno/drive'
import { getDateLocale } from '@kubuno/sdk'
import { useDebouncedAutosave } from '@kubuno/sdk'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { StatusBar, StatusSep, StatusSpacer } from './shell/StatusBar'
import { THEME_MATHS } from './ribbon/officeThemes'
import { formulasApi, type MathFormula } from './maths-api'
import { MATH_CATEGORIES, CARET, type MathTemplate } from './mathSymbols'
import LatexEditor from './LatexEditor'
import { renderLatex, isLatexDocument } from './latexRender'
import { MacrosMenu } from './macros/MacrosMenu'
import { appPrompt } from './macros/FormRuntime'
import { parseDoc, serializeDoc, PAGE_FORMATS, paperPx, DEFAULT_FORMAT, type MathBlock, type Orientation } from './mathPages'
import { defaultGraphSpec, type GraphSpec } from './mathGraph'
import GraphBlock from './GraphBlock'
import MathTreeEditor, { type MathEditorHandle } from './MathTreeEditor'
import {
  MATH_THEMES, paletteStyle, resolvePalette,
  loadColorize, saveColorize, loadThemeId, saveThemeId, loadCustomPalette, saveCustomPalette,
  type MathPalette,
} from './mathThemes'

function renderTex(tex: string, displayMode: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode, throwOnError: false, output: 'html', strict: false })
  } catch {
    return ''
  }
}

// Drop the in-session UI id before serialising a block back to storage.
const stripId = (b: MathBlock & { id: string }): MathBlock =>
  b.type === 'graph' ? { type: 'graph', spec: b.spec } : { type: 'formula', latex: b.latex }

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

function FormulaEditorView({ formula, onUpdate, formulaCount, saveRef }: { formula: MathFormula; onUpdate: (f: MathFormula) => void; formulaCount: number; saveRef?: React.MutableRefObject<(() => Promise<void>) | null> }) {
  const { t } = useTranslation('office')
  // A page is an ordered list of blocks (formula OR graph). Stored inside the single `latex`
  // field (backward-compatible envelope); each block gets a session id for stable React keys.
  type UIBlock = MathBlock & { id: string }
  type UIPage = { name: string; blocks: UIBlock[]; format: string; orientation: Orientation }
  const idc = useRef(0)
  const nid = () => `b${idc.current++}`
  const [pages, setPages] = useState<UIPage[]>(() =>
    parseDoc(formula.latex).map(p => ({ name: p.name, blocks: p.blocks.map(b => ({ ...b, id: nid() })), format: p.format ?? DEFAULT_FORMAT, orientation: p.orientation ?? 'portrait' })))
  const [active, setActive] = useState(0)
  const [activeBlockId, setActiveBlockId] = useState<string | null>(null)
  const [dragId, setDragId] = useState<string | null>(null)   // block being dragged
  const [overId, setOverId] = useState<string | null>(null)   // block hovered as drop target
  const [category, setCategory] = useState(MATH_CATEGORIES[0].id)
  const [mode, setMode] = useState<'visual' | 'code'>('code')
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

  // Per-block editor handles (visual) and textareas (code), so the palette targets the right block.
  const editorRefs = useRef<Record<string, MathEditorHandle | null>>({})
  const taRefs = useRef<Record<string, { current: HTMLTextAreaElement | null }>>({})
  const taRefOf = (id: string) => (taRefs.current[id] ??= { current: null })

  // ── Block mutations on the active page ──────────────────────────────────────────
  const setBlocks = (fn: (bs: UIBlock[]) => UIBlock[]) => setPages(prev => prev.map((p, i) => i === pageIdx ? { ...p, blocks: fn(p.blocks) } : p))
  const setBlockLatex = (id: string, v: string | ((p: string) => string)) =>
    setBlocks(bs => bs.map(b => b.id === id && b.type === 'formula' ? { ...b, latex: typeof v === 'function' ? v(b.latex) : v } : b))
  const setGraphSpec = (id: string, spec: GraphSpec) => setBlocks(bs => bs.map(b => b.id === id && b.type === 'graph' ? { ...b, spec } : b))
  const addFormulaBlock = () => { const id = nid(); setBlocks(bs => [...bs, { id, type: 'formula', latex: '' }]); setActiveBlockId(id) }
  const addGraphBlock = () => { const id = nid(); setBlocks(bs => [...bs, { id, type: 'graph', spec: defaultGraphSpec(bs.filter(b => b.type === 'graph').length) }]); setActiveBlockId(id) }
  const removeBlock = (id: string) => setBlocks(bs => bs.length > 1 ? bs.filter(b => b.id !== id) : bs)
  const moveBlock = (id: string, dir: -1 | 1) => setBlocks(bs => { const i = bs.findIndex(b => b.id === id); const j = i + dir; if (i < 0 || j < 0 || j >= bs.length) return bs; const n = [...bs]; [n[i], n[j]] = [n[j], n[i]]; return n })
  // Drag-and-drop reordering: drop the dragged block right before `targetId`, or at the end.
  const moveBlockBefore = (id: string, targetId: string) => setBlocks(bs => {
    const from = bs.findIndex(b => b.id === id); if (from < 0 || id === targetId) return bs
    const n = [...bs]; const [m] = n.splice(from, 1)
    const to = n.findIndex(b => b.id === targetId); if (to < 0) return bs
    n.splice(to, 0, m); return n
  })
  const moveBlockEnd = (id: string) => setBlocks(bs => { const from = bs.findIndex(b => b.id === id); if (from < 0) return bs; const n = [...bs]; const [m] = n.splice(from, 1); n.push(m); return n })

  // The formula block the palette / toolbar / context-menu act on: the focused one, else the first.
  const targetId = (activeBlockId && page.blocks.some(b => b.id === activeBlockId && b.type === 'formula'))
    ? activeBlockId
    : (page.blocks.find(b => b.type === 'formula')?.id ?? null)
  const ed = () => targetId ? editorRefs.current[targetId] ?? null : null
  const targetLatex = () => { const b = page.blocks.find(x => x.id === targetId); return b && b.type === 'formula' ? b.latex : '' }

  const serialized = useMemo(() => serializeDoc(pages.map(p => ({ name: p.name, blocks: p.blocks.map(stripId), format: p.format, orientation: p.orientation }))), [pages])
  useDebouncedAutosave(serialized, true, (v) => {
    formulasApi.update(formula.id, { latex: v }).then(d => onUpdate(d.formula)).catch(() => {})
  })

  // Expose an immediate save to the parent's title-bar SaveButton (same path as the autosave).
  useEffect(() => {
    if (!saveRef) return
    saveRef.current = () => formulasApi.update(formula.id, { latex: serialized }).then(d => onUpdate(d.formula)).catch(() => {})
    return () => { if (saveRef) saveRef.current = null }
  }, [saveRef, formula.id, serialized, onUpdate])

  const cat = MATH_CATEGORIES.find(c => c.id === category) ?? MATH_CATEGORIES[0]

  // Code-mode insertion into a block's textarea (with the ‸ caret marker).
  const insertCodeAt = (id: string, tpl: MathTemplate) => {
    const el = taRefs.current[id]?.current
    const text = tpl.ins.replace(CARET, '')
    const caretRel = tpl.ins.indexOf(CARET)
    if (!el) { setBlockLatex(id, l => l + text); return }
    const start = el.selectionStart, end = el.selectionEnd, cur = el.value
    setBlockLatex(id, cur.slice(0, start) + text + cur.slice(end))
    const caret = caretRel >= 0 ? start + caretRel : start + text.length
    requestAnimationFrame(() => { el.focus(); el.setSelectionRange(caret, caret) })
  }

  // ── Palette / quick-toolbar / context-menu actions (target the active formula block) ──
  const onInsert = (tpl: MathTemplate) => {
    if (!targetId) { const id = nid(); setBlocks(bs => [...bs, { id, type: 'formula', latex: mode === 'visual' ? tpl.tex : tpl.ins.replace(CARET, '') }]); setActiveBlockId(id); return }
    if (mode === 'visual') editorRefs.current[targetId]?.insertLatex(tpl.tex)
    else insertCodeAt(targetId, tpl)
  }
  const insertTemplate = (tex: string) => ed()?.insertLatex(tex)
  const wrapFrac = () => ed()?.frac()
  const wrapSup = () => ed()?.script('sup')
  const wrapSub = () => ed()?.script('sub')
  const deleteSlot = () => ed()?.deleteSlot()

  const onVisualContext = (e: React.MouseEvent) => { e.preventDefault(); e.stopPropagation(); setVMenu({ x: e.clientX, y: e.clientY }) }

  // ── Page operations ───────────────────────────────────────────────────────────
  const addPage = () => { const id = nid(); setPages(prev => [...prev, { name: `Page ${prev.length + 1}`, blocks: [{ id, type: 'formula', latex: '' }], format: page.format, orientation: page.orientation }]); setActive(pages.length) }
  const deletePage = (i: number) => { if (pages.length <= 1) return; setPages(prev => prev.filter((_, j) => j !== i)); setActive(a => Math.max(0, a >= i ? a - 1 : a)) }
  const renamePage = async (i: number) => { const v = await appPrompt(t('math_page_rename', { defaultValue: 'Nom de la page :' }), pages[i].name); if (v != null) setPages(prev => prev.map((p, j) => j === i ? { ...p, name: v || p.name } : p)) }

  const makeApi = () => {
    const Math = {
      getLatex: () => targetLatex(),
      setLatex: (src: unknown) => { if (targetId) setBlockLatex(targetId, String(src)) },
      getFormulaCount: () => formulaCount,
      getPageCount: () => pages.length,
      addPage: () => addPage(),
    }
    const App = { getType: () => 'math', getId: () => formula.id, toast: (m: unknown) => console.log(String(m)), log: (m: unknown) => console.log(String(m)) }
    return { Math, App }
  }

  return (
    <div className="flex flex-col flex-1 min-w-0 min-h-0 bg-surface-1 overflow-hidden">
      <style>{`
        .mc-h{cursor:text}
        .mc-empty{outline:1px dashed #9aa0a6;border-radius:3px;color:#9aa0a6;background:#f8f9fa}
        .mc-sel{background:#d2e3fc;border-radius:2px}
        .mc-caret{display:inline-block;width:0;border-left:2px solid #1a73e8;margin:0 -1px;animation:mc-blink 1.05s steps(1) infinite}
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
        {/* Palette de symboles */}
        <div className="flex-shrink-0 w-56 bg-white border-r border-border flex flex-col overflow-hidden">
          <div className="p-2 border-b border-border flex-shrink-0">
            <Dropdown className="w-full" value={category} onChange={setCategory}
              options={MATH_CATEGORIES.map(c => ({ value: c.id, label: t('math_cat_' + c.id, { defaultValue: c.label }) }))} />
          </div>
          <div className="flex-1 overflow-y-auto p-2">
            <div className="grid grid-cols-3 gap-1.5">
              {cat.items.map((tpl, i) => <TemplateButton key={i} tpl={tpl} onInsert={onInsert} />)}
            </div>
          </div>
        </div>

        {/* Zone centrale */}
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

          {/* Pile de blocs : formules + graphes sur une feuille au format choisi */}
          <div className="flex-1 min-h-0 overflow-auto bg-surface-2 p-6">
            <div className="mx-auto bg-white shadow-md" style={{ width: sheet.w, minHeight: sheet.h }}>
              <div className="flex flex-col gap-3 p-6">
              {page.blocks.map((b, bi) => (
                <div key={b.id}
                  className={`group relative transition-[opacity] ${dragId === b.id ? 'opacity-40' : ''} ${overId === b.id && dragId !== b.id ? "before:content-[''] before:absolute before:-top-1.5 before:left-0 before:right-0 before:h-0.5 before:bg-primary before:rounded" : ''}`}
                  onMouseDownCapture={() => setActiveBlockId(b.id)} onFocusCapture={() => setActiveBlockId(b.id)}
                  onDragOver={e => { if (dragId && dragId !== b.id) { e.preventDefault(); setOverId(b.id) } }}
                  onDrop={e => { if (dragId && dragId !== b.id) { e.preventDefault(); moveBlockBefore(dragId, b.id) } setDragId(null); setOverId(null) }}>
                  {/* Chrome au survol : déplacer (glisser) · monter / descendre · supprimer */}
                  <div className="absolute -top-2.5 right-2 z-10 hidden group-hover:flex items-center bg-white border border-border rounded shadow-sm">
                    <button draggable title={t('math_block_drag', { defaultValue: 'Glisser pour déplacer' })}
                      onDragStart={e => { setDragId(b.id); e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', b.id) }}
                      onDragEnd={() => { setDragId(null); setOverId(null) }}
                      className="p-1 text-text-tertiary hover:text-text-primary cursor-grab active:cursor-grabbing"><GripVertical size={13} /></button>
                    <button disabled={bi === 0} onClick={() => moveBlock(b.id, -1)} title={t('math_block_up', { defaultValue: 'Monter' })} className="p-1 text-text-tertiary hover:text-text-primary disabled:opacity-30"><ArrowUp size={13} /></button>
                    <button disabled={bi === page.blocks.length - 1} onClick={() => moveBlock(b.id, 1)} title={t('math_block_down', { defaultValue: 'Descendre' })} className="p-1 text-text-tertiary hover:text-text-primary disabled:opacity-30"><ArrowDown size={13} /></button>
                    <button disabled={page.blocks.length <= 1} onClick={() => removeBlock(b.id)} title={t('math_block_remove', { defaultValue: 'Supprimer le bloc' })} className="p-1 text-text-tertiary hover:text-danger disabled:opacity-30"><Trash2 size={13} /></button>
                  </div>

                  {b.type === 'graph' ? (
                    <GraphBlock spec={b.spec} onChange={spec => setGraphSpec(b.id, spec)} t={t} />
                  ) : mode === 'visual' ? (
                    <div onContextMenu={onVisualContext}
                      className={`rounded-lg border bg-white overflow-hidden ${targetId === b.id ? 'border-primary/50 ring-1 ring-primary/15' : 'border-border'} ${colorize ? 'mc-colorize' : ''}`}
                      style={colorize ? colorStyle : undefined}>
                      <MathTreeEditor key={b.id} ref={h => { editorRefs.current[b.id] = h }} value={b.latex} onChange={v => setBlockLatex(b.id, v)} />
                    </div>
                  ) : (
                    <div className={`rounded-lg border bg-white overflow-hidden ${targetId === b.id ? 'border-primary/50 ring-1 ring-primary/15' : 'border-border'}`}>
                      <div className={`p-4 min-h-[56px] flex items-center justify-center overflow-x-auto ${colorize ? 'mc-colorize' : ''}`} style={colorize ? colorStyle : undefined}
                        dangerouslySetInnerHTML={{ __html: renderLatex(b.latex).html || `<span style="color:#9aa0a6;font-size:13px">${t('math_empty_preview', { defaultValue: 'Formule vide' })}</span>` }} />
                      <div className="h-[130px] border-t border-border">
                        <LatexEditor value={b.latex} onChange={v => setBlockLatex(b.id, v)} taRef={taRefOf(b.id)} />
                      </div>
                    </div>
                  )}
                </div>
              ))}

              {/* Ajouter un bloc (et zone de dépôt en fin de page) */}
              <div className="flex items-center gap-2 pt-1"
                onDragOver={e => { if (dragId) { e.preventDefault(); setOverId('__end__') } }}
                onDrop={e => { if (dragId) { e.preventDefault(); moveBlockEnd(dragId) } setDragId(null); setOverId(null) }}>
                <button onClick={addFormulaBlock} className="flex items-center gap-1.5 h-8 px-3 rounded-lg border border-dashed border-border text-xs text-text-secondary bg-white hover:border-primary/50 hover:text-primary">
                  <Sigma size={14} /> {t('math_add_formula', { defaultValue: 'Formule' })}
                </button>
                <button onClick={addGraphBlock} className="flex items-center gap-1.5 h-8 px-3 rounded-lg border border-dashed border-border text-xs text-text-secondary bg-white hover:border-primary/50 hover:text-primary">
                  <LineChart size={14} /> {t('math_add_graph', { defaultValue: 'Graphe' })}
                </button>
              </div>
              </div>
            </div>
          </div>

          {mode === 'visual' && (
            <div className="flex items-center gap-3 px-3 h-8 border-t border-border bg-surface-1 flex-shrink-0 text-[11px] text-text-tertiary">
              <span>{t('math_visual_hint2', { defaultValue: 'Tapez comme dans un texte · ← → ↑ ↓ se déplacer · / fraction · ^ exposant · _ indice · ( ) [ ] | délimiteurs · \\nom symbole · clic droit = menu' })}</span>
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
            <StatusSpacer />
            {page.blocks.some(b => b.type === 'formula' && hasLatexError(b.latex)) && <div className="flex items-center px-2 text-[#d93025] whitespace-nowrap">{t('math_status_latex_error', { defaultValue: 'Erreur LaTeX' })}</div>}
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
        { type: 'separator' },
        { type: 'action', label: t('math_ctx_del_slot', { defaultValue: 'Supprimer l\'emplacement' }), onClick: deleteSlot },
        { type: 'action', label: t('math_ctx_clear', { defaultValue: 'Vider la formule' }), danger: true, onClick: () => ed()?.clear() },
        { type: 'separator' },
        { type: 'action', label: t('math_ctx_copy', { defaultValue: 'Copier le LaTeX' }), onClick: () => { navigator.clipboard?.writeText(targetLatex()).catch(() => {}) } },
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
  const { id: routeId } = useParams<{ id: string }>()
  const [formulas, setFormulas]   = useState<MathFormula[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [loading, setLoading]     = useState(true)

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

  // Immediate save wired to the title-bar SaveButton. The editor view fills `saveRef` with
  // a function that persists the current document (same path as the debounced autosave).
  const saveRef = useRef<(() => Promise<void>) | null>(null)
  const [saving, setSaving] = useState(false)
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

  const handleOpenFile = (file: FileItem): boolean => {
    formulasApi.openByFile(file.id).then(({ formula }) => {
      upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
    }).catch(() => {})
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
    doc: {
      info: (
        <InfoPanel
          title={selected?.name || t('common_untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('math_title', { defaultValue: 'Maths' })}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('math_title', { defaultValue: 'Maths' })],
            ...(selected?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(selected.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: () => setSelectedId(null),
    },
  })

  if (loading) {
    return <div className="flex h-full items-center justify-center text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</div>
  }

  // Accueil (aucune formule ouverte).
  if (!selected) {
    return (
      <ModuleHome
        theme={THEME_MATHS}
        title={t('math_title', { defaultValue: 'Maths' })}
        titleIcon={<Sigma size={16} className="text-white/90 flex-shrink-0" />}
        fileLabel={t('doc_bs_file', { defaultValue: 'Fichier' })}
        homeLabel={t('doc_bs_home', { defaultValue: 'Accueil' })}
        onBack={() => navigate('/office')}
        startContent={<MathsStartContent recentItems={recentItems} onNew={handleNew} onOpenFile={handleOpenFile} />}
      />
    )
  }

  return (
    <OfficeShell
      ribbon={[fileTab, { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
        groups: [{ id: 'formula', label: t('math_title', { defaultValue: 'Formule' }), items: [
          { id: 'new', kind: 'button', icon: <FilePlus size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: handleNew },
          { id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: handleDuplicate },
        ] }] }]}
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
      titleActions={<SaveButton onSave={handleSave} saving={saving} label={t('doc_save', { defaultValue: 'Enregistrer' })} />}
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
        ? <FormulaEditorView key={selected.id} formula={selected} onUpdate={handleUpdate} formulaCount={formulas.length} saveRef={saveRef} />
        : <div className="flex flex-1 items-center justify-center text-text-tertiary text-sm">{t('common_loading', { defaultValue: 'Chargement…' })}</div>}
    </OfficeShell>
  )
}
