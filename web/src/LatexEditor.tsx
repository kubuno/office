import { useEffect, useRef, useState } from 'react'
import katex from 'katex'
import { getContext, getCompletions, type AcContext, type Completion } from './mathAutocomplete'
import { CARET } from './mathSymbols'

// Éditeur de code LaTeX avec coloration syntaxique. Technique « overlay » : un
// <textarea> transparent (curseur + saisie) au-dessus d'un <pre> coloré, parfaitement
// alignés (même police/taille/marge) et défilement synchronisé.
// Autocomplétion contextuelle : \commande, \begin{env}/\end{env}, fonctions de graphe.

const FONT = "13px 'Fira Code', 'DM Mono', ui-monospace, SFMono-Regular, Menlo, monospace"
const LINE_H = 20
const PAD_T = 12
const PAD_L = 14

// Cached KaTeX previews for the completion popup.
const previewCache = new Map<string, string>()
function texPreview(tex: string): string {
  let html = previewCache.get(tex)
  if (html == null) {
    try { html = katex.renderToString(tex, { throwOnError: false, output: 'html', strict: false }) } catch { html = '' }
    previewCache.set(tex, html)
  }
  return html
}

// Caret x/y (container px) of a text offset — monospace-free measurement via canvas.
let measureCtx: CanvasRenderingContext2D | null = null
function caretXY(el: HTMLTextAreaElement, offset: number): { x: number; y: number } {
  if (!measureCtx) measureCtx = document.createElement('canvas').getContext('2d')
  const lines = el.value.slice(0, offset).split('\n')
  let w = 0
  if (measureCtx) {
    measureCtx.font = getComputedStyle(el).font || FONT
    w = measureCtx.measureText(lines[lines.length - 1]).width
  }
  return { x: PAD_L + w - el.scrollLeft, y: PAD_T + (lines.length - 1) * LINE_H - el.scrollTop }
}

interface AcState { items: Completion[]; sel: number; x: number; y: number; ctx: AcContext }

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

// Coloration LaTeX : commandes \cmd, accolades/crochets, indices/exposants & opérateurs,
// nombres, commentaires %…, environnements \begin{…}/\end{…}.
function highlight(src: string): string {
  // On découpe en jetons via une regex globale, en échappant le HTML par jeton.
  const re = /(%[^\n]*)|(\\(?:begin|end)\b)|(\\[a-zA-Z]+)|(\\[^a-zA-Z])|([{}\[\]()])|([_^&])|([+\-*/=<>|])|(\d+(?:\.\d+)?)/g
  let out = ''
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(src)) !== null) {
    out += escapeHtml(src.slice(last, m.index))
    const tok = m[0]
    let cls = ''
    if (m[1]) cls = 'tk-com'        // commentaire
    else if (m[2]) cls = 'tk-env'   // \begin \end
    else if (m[3]) cls = 'tk-cmd'   // \commande
    else if (m[4]) cls = 'tk-esc'   // \{  \\  etc.
    else if (m[5]) cls = 'tk-brace' // { } [ ] ( )
    else if (m[6]) cls = 'tk-sub'   // _ ^ &
    else if (m[7]) cls = 'tk-op'    // opérateurs
    else if (m[8]) cls = 'tk-num'   // nombres
    out += cls ? `<span class="${cls}">${escapeHtml(tok)}</span>` : escapeHtml(tok)
    last = re.lastIndex
  }
  out += escapeHtml(src.slice(last))
  // garde une ligne vide finale visible
  return out + '\n'
}

export default function LatexEditor({
  value, onChange, taRef,
}: {
  value: string
  onChange: (v: string) => void
  taRef?: React.RefObject<HTMLTextAreaElement | null>
}) {
  const innerRef = useRef<HTMLTextAreaElement>(null)
  const ta = taRef ?? innerRef
  const preRef = useRef<HTMLPreElement>(null)
  const [ac, setAc] = useState<AcState | null>(null)
  const suppressRef = useRef<number | null>(null)   // Échap : jeton (start) à ne pas rouvrir
  // Caret à poser APRÈS que React a réellement écrit la nouvelle valeur dans le textarea
  // (un setSelectionRange posé avant le commit serait écrasé, le caret sauterait en fin).
  const pendingCaretRef = useRef<number | null>(null)
  useEffect(() => {
    const el = ta.current
    if (pendingCaretRef.current != null && el && el.value === value) {
      el.focus()
      el.setSelectionRange(pendingCaretRef.current, pendingCaretRef.current)
      pendingCaretRef.current = null
    }
  }, [value]) // eslint-disable-line react-hooks/exhaustive-deps

  // Synchronise le défilement de l'overlay coloré avec le textarea.
  const sync = () => {
    if (preRef.current && ta.current) {
      preRef.current.scrollTop  = ta.current.scrollTop
      preRef.current.scrollLeft = ta.current.scrollLeft
    }
  }
  useEffect(sync, [value]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Autocomplétion ──────────────────────────────────────────────────────────────
  const refreshAc = () => {
    const el = ta.current
    if (!el || el.selectionStart !== el.selectionEnd) { setAc(null); return }
    const caret = el.selectionStart
    const ctx = getContext(el.value, caret)
    if (!ctx) { suppressRef.current = null; setAc(null); return }
    if (suppressRef.current === ctx.start) { setAc(null); return }
    suppressRef.current = null
    const items = getCompletions(el.value, caret, ctx)
    if (!items.length) { setAc(null); return }
    const { x, y } = caretXY(el, ctx.start)
    const listH = Math.min(items.length, 9) * 26 + 8
    const flip = el.clientHeight - y < listH + 28
    setAc(prev => ({
      items,
      sel: prev && prev.ctx.kind === ctx.kind && prev.ctx.start === ctx.start ? Math.min(prev.sel, items.length - 1) : 0,
      x: Math.max(4, Math.min(x, el.clientWidth - 300)),
      y: flip ? Math.max(4, y - listH - 4) : y + LINE_H + 2,
      ctx,
    }))
  }

  const accept = (c: Completion) => {
    const el = ta.current
    if (!el || !ac) return
    const caret = el.selectionStart
    // Recompute the token from the live value: the popup state may lag one keystroke.
    const ctx = getContext(el.value, caret) ?? ac.ctx
    const caretRel = c.insert.indexOf(CARET)
    const ins = c.insert.replace(CARET, '')
    onChange(el.value.slice(0, ctx.start) + ins + el.value.slice(caret))
    pendingCaretRef.current = ctx.start + (caretRel >= 0 ? caretRel : ins.length)
    setAc(null)
  }

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (!ac) return
    if (e.key === 'ArrowDown') { e.preventDefault(); setAc(a => a && { ...a, sel: (a.sel + 1) % a.items.length }) }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setAc(a => a && { ...a, sel: (a.sel - 1 + a.items.length) % a.items.length }) }
    else if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); accept(ac.items[ac.sel]) }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); suppressRef.current = ac.ctx.start; setAc(null) }
  }

  const shared: React.CSSProperties = {
    margin: 0,
    border: 0,
    padding: '12px 14px',
    font: FONT,
    lineHeight: '20px',
    whiteSpace: 'pre',
    overflowWrap: 'normal',
    tabSize: 2,
  }

  return (
    <div className="relative flex-1 min-h-0 overflow-hidden bg-white" style={{ contain: 'strict' }}>
      <style>{`
        .latex-hl .tk-cmd   { color:#0b66c3; }
        .latex-hl .tk-env   { color:#8250df; font-weight:600; }
        .latex-hl .tk-esc   { color:#0a7ea4; }
        .latex-hl .tk-brace { color:#9a6700; }
        .latex-hl .tk-sub   { color:#d6336c; font-weight:600; }
        .latex-hl .tk-op    { color:#c2410c; }
        .latex-hl .tk-num   { color:#1e7e34; }
        .latex-hl .tk-com   { color:#6a737d; font-style:italic; }
      `}</style>
      <pre
        ref={preRef}
        aria-hidden
        className="latex-hl absolute inset-0 overflow-auto pointer-events-none"
        style={{ ...shared, color: '#202124' }}
        dangerouslySetInnerHTML={{ __html: highlight(value) }}
      />
      <textarea
        ref={ta}
        value={value}
        onChange={e => { onChange(e.target.value); refreshAc() }}
        onKeyDown={onKeyDown}
        onClick={refreshAc}
        onBlur={() => setAc(null)}
        onScroll={() => { sync(); if (ac) refreshAc() }}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        className="absolute inset-0 w-full h-full resize-none overflow-auto bg-transparent outline-none"
        style={{ ...shared, color: 'transparent', caretColor: '#202124' }}
        placeholder="LaTeX…"
      />
      {/* Popup d'autocomplétion (↑/↓ naviguer · Entrée/Tab insérer · Échap fermer) */}
      {ac && (
        <div className="absolute z-50 w-72 max-h-[242px] overflow-y-auto bg-white border border-border rounded-lg shadow-xl py-1"
          style={{ left: ac.x, top: ac.y }}>
          {ac.items.map((c, i) => (
            <div key={`${c.kind}:${c.label}`}
              onMouseDown={e => { e.preventDefault(); accept(c) }}
              onMouseEnter={() => setAc(a => a && { ...a, sel: i })}
              className={`flex items-center gap-2 h-[26px] px-2.5 cursor-pointer text-xs ${i === ac.sel ? 'bg-primary/10' : ''}`}>
              <span className="w-11 flex items-center justify-center flex-shrink-0 overflow-hidden text-[11px] text-[#202124]"
                dangerouslySetInnerHTML={c.tex ? { __html: texPreview(c.tex) } : undefined}>
              </span>
              <span className="font-mono text-text-primary truncate">{c.kind === 'cmd' ? '\\' : ''}{c.label}</span>
              {c.detail && <span className="ml-auto pl-2 text-text-tertiary text-[10px] truncate flex-shrink-0">{c.detail}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
