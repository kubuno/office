// Rich text for presentation text boxes: per-span bold/italic/underline/strike/
// color/size, stored in a ProseMirror-like doc, edited in a contentEditable and
// laid out span-by-span on the canvas.
//
// The model: doc → paragraphs → runs. A "run" is a maximal span sharing the same
// formatting marks. Marks left unset fall back to the element-level defaults at
// render time (so a plain doc keeps rendering with the text box's own style).

export interface RichRun {
  text: string
  b?: boolean
  i?: boolean
  u?: boolean
  s?: boolean
  color?: string
  /** Highlight (surlignage) background colour. */
  hl?: string
  /** Superscript / subscript. */
  sup?: boolean
  sub?: boolean
  /** Font size in slide-space px (960×540 reference). */
  size?: number
}
export interface ParaAttrs {
  align?: 'left' | 'center' | 'right' | 'justify'
  list?: 'bullet' | 'number'
  /** Indent in slide-space px. */
  indent?: number
  /** Line-height multiplier (default 1.3). */
  lineHeight?: number
}
export interface RichPara { runs: RichRun[]; attrs?: ParaAttrs }

// ── doc (ProseMirror-like JSON) ⇄ paragraphs of runs ───────────────────────────

type PMMark = { type?: string; attrs?: Record<string, unknown> }
type PMNode = { type?: string; text?: string; marks?: PMMark[]; content?: PMNode[] }

function marksToRun(text: string, marks: PMMark[] | undefined): RichRun {
  const run: RichRun = { text }
  for (const m of marks ?? []) {
    if (m.type === 'bold') run.b = true
    else if (m.type === 'italic') run.i = true
    else if (m.type === 'underline') run.u = true
    else if (m.type === 'strike') run.s = true
    else if (m.type === 'superscript') run.sup = true
    else if (m.type === 'subscript') run.sub = true
    else if (m.type === 'highlight') run.hl = m.attrs?.color ? String(m.attrs.color) : '#fff176'
    else if (m.type === 'textStyle' && m.attrs) {
      if (m.attrs.color) run.color = String(m.attrs.color)
      if (m.attrs.fontSize != null) { const n = parseFloat(String(m.attrs.fontSize)); if (!isNaN(n)) run.size = n }
    }
  }
  return run
}

export function docToParas(doc: unknown): RichPara[] {
  const d = doc as PMNode | null
  if (!d || d.type !== 'doc' || !Array.isArray(d.content)) return [{ runs: [] }]
  return d.content.map(p => {
    const para: RichPara = {
      runs: Array.isArray(p.content)
        ? p.content.filter(n => n.type === 'text' && n.text).map(n => marksToRun(n.text as string, n.marks))
        : [],
    }
    const a = (p as PMNode & { attrs?: ParaAttrs }).attrs
    if (a && (a.align || a.list || a.indent || a.lineHeight)) para.attrs = { ...a }
    return para
  })
}

function runMarks(run: RichRun): PMMark[] {
  const marks: PMMark[] = []
  if (run.b) marks.push({ type: 'bold' })
  if (run.i) marks.push({ type: 'italic' })
  if (run.u) marks.push({ type: 'underline' })
  if (run.s) marks.push({ type: 'strike' })
  if (run.sup) marks.push({ type: 'superscript' })
  if (run.sub) marks.push({ type: 'subscript' })
  if (run.hl) marks.push({ type: 'highlight', attrs: { color: run.hl } })
  const attrs: Record<string, unknown> = {}
  if (run.color) attrs.color = run.color
  if (run.size != null) attrs.fontSize = run.size
  if (Object.keys(attrs).length) marks.push({ type: 'textStyle', attrs })
  return marks
}

const sameStyle = (a: RichRun, b: RichRun) =>
  !!a.b === !!b.b && !!a.i === !!b.i && !!a.u === !!b.u && !!a.s === !!b.s && !!a.sup === !!b.sup && !!a.sub === !!b.sub && (a.hl ?? '') === (b.hl ?? '') && (a.color ?? '') === (b.color ?? '') && (a.size ?? 0) === (b.size ?? 0)

// Merge adjacent runs that share styling, drop empties.
export function mergeRuns(runs: RichRun[]): RichRun[] {
  const out: RichRun[] = []
  for (const r of runs) {
    if (!r.text) continue
    const last = out[out.length - 1]
    if (last && sameStyle(last, r)) last.text += r.text
    else out.push({ ...r })
  }
  return out
}

export function parasToDoc(paras: RichPara[]): PMNode {
  return {
    type: 'doc',
    content: paras.map(p => {
      const runs = mergeRuns(p.runs)
      const node: PMNode & { attrs?: ParaAttrs } = {
        type: 'paragraph',
        content: runs.map(r => {
          const marks = runMarks(r)
          return marks.length ? { type: 'text', text: r.text, marks } : { type: 'text', text: r.text }
        }),
      }
      if (p.attrs && (p.attrs.align || p.attrs.list || p.attrs.indent || p.attrs.lineHeight)) node.attrs = { ...p.attrs }
      return node
    }),
  }
}

// ── HTML (contentEditable) generation ───────────────────────────────────────────

export function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function runStyle(run: RichRun, scale: number): string {
  const st: string[] = []
  if (run.b) st.push('font-weight:bold')
  if (run.i) st.push('font-style:italic')
  const deco: string[] = []
  if (run.u) deco.push('underline')
  if (run.s) deco.push('line-through')
  if (deco.length) st.push(`text-decoration:${deco.join(' ')}`)
  if (run.color) st.push(`color:${run.color}`)
  if (run.hl) st.push(`background-color:${run.hl}`)
  if (run.size != null) st.push(`font-size:${run.size * scale}px`)
  return st.join(';')
}

// HTML for the editable overlay. Paragraphs are separated by literal newlines
// (the editor uses white-space:pre-wrap), so there are no block <div> wrappers to
// confuse parsing back.
export function parasToHtml(paras: RichPara[], scale: number): string {
  return paras.map(p => {
    const runs = mergeRuns(p.runs)
    if (!runs.length) return ''
    return runs.map(r => {
      const style = runStyle(r, scale)
      let text = escapeHtml(r.text)
      if (r.sup) text = `<sup>${text}</sup>`
      else if (r.sub) text = `<sub>${text}</sub>`
      return style ? `<span style="${style}">${text}</span>` : `<span>${text}</span>`
    }).join('')
  }).join('\n')
}

// ── HTML (contentEditable DOM) → paragraphs of runs ─────────────────────────────
// Uses a minimal DOM surface so it can be unit-tested with plain mock nodes.

export interface MiniNode {
  nodeType: number
  textContent?: string | null
  childNodes?: ArrayLike<MiniNode>
  tagName?: string
  style?: Record<string, string>
  getAttribute?: (name: string) => string | null
}

type Marks = { b?: boolean; i?: boolean; u?: boolean; s?: boolean; color?: string; size?: number; hl?: string; sup?: boolean; sub?: boolean }

function rgbToHex(c: string): string {
  const m = c.match(/^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/i)
  if (!m) return c
  const h = (n: string) => Math.max(0, Math.min(255, parseInt(n, 10))).toString(16).padStart(2, '0')
  return `#${h(m[1])}${h(m[2])}${h(m[3])}`
}

function marksForEl(parent: Marks, el: MiniNode, scale: number): Marks {
  const m: Marks = { ...parent }
  const tag = (el.tagName ?? '').toLowerCase()
  if (tag === 'b' || tag === 'strong') m.b = true
  if (tag === 'i' || tag === 'em') m.i = true
  if (tag === 'u') m.u = true
  if (tag === 's' || tag === 'strike' || tag === 'del') m.s = true
  if (tag === 'sup') m.sup = true
  if (tag === 'sub') m.sub = true
  const st = el.style ?? {}
  // Les styles explicites « normal » / « none » DÉSACTIVENT l'héritage (toggle off).
  const fw = st.fontWeight || ''
  if (fw === 'bold' || fw === 'bolder' || (parseInt(fw, 10) >= 600)) m.b = true
  else if (fw === 'normal' || fw === 'lighter' || (fw && parseInt(fw, 10) < 600)) m.b = false
  if (st.fontStyle === 'italic') m.i = true
  else if (st.fontStyle === 'normal') m.i = false
  const td = `${st.textDecorationLine || ''} ${st.textDecoration || ''}`
  if (/none/.test(td)) { m.u = false; m.s = false }
  if (/underline/.test(td)) m.u = true
  if (/line-through/.test(td)) m.s = true
  const col = st.color || el.getAttribute?.('color') || ''
  if (col) m.color = rgbToHex(col)
  const bg = st.backgroundColor || st.background || ''
  if (bg && bg !== 'transparent' && !/^rgba\(0, 0, 0, 0\)/.test(bg)) m.hl = rgbToHex(bg)
  const va = st.verticalAlign || ''
  if (va === 'super') m.sup = true
  else if (va === 'sub') m.sub = true
  const fs = st.fontSize || ''
  if (fs.endsWith('px')) { const n = parseFloat(fs); if (!isNaN(n)) m.size = n / scale }
  return m
}

export function htmlToParas(root: MiniNode, scale: number): RichPara[] {
  // Flatten to a sequence of runs with embedded '\n', then split on '\n'. Block
  // elements (div/p) and <br> are treated as line breaks, which keeps parsing
  // robust against whatever the browser produced.
  const flat: RichRun[] = []
  const walk = (node: MiniNode, marks: Marks) => {
    const kids = node.childNodes ? Array.from(node.childNodes) : []
    for (const child of kids) {
      if (child.nodeType === 3) {
        const t = child.textContent ?? ''
        if (t) flat.push({ ...marks, text: t })
      } else if (child.nodeType === 1) {
        const tag = (child.tagName ?? '').toLowerCase()
        if (tag === 'br') { flat.push({ ...marks, text: '\n' }) }
        else if (tag === 'div' || tag === 'p') {
          if (flat.length && !flat[flat.length - 1].text.endsWith('\n')) flat.push({ ...marks, text: '\n' })
          walk(child, marks)
        } else {
          walk(child, marksForEl(marks, child, scale))
        }
      }
    }
  }
  walk(root, {})
  const paras: RichPara[] = [{ runs: [] }]
  for (const r of flat) {
    const parts = r.text.split('\n')
    parts.forEach((part, i) => {
      if (i > 0) paras.push({ runs: [] })
      if (part) { const { text, ...marks } = r; void text; paras[paras.length - 1].runs.push({ ...marks, text: part }) }
    })
  }
  return paras.map(p => ({ runs: mergeRuns(p.runs) }))
}

// ── Layout (word wrap across mixed-style runs) ──────────────────────────────────

export interface ResolvedStyle { bold: boolean; italic: boolean; underline: boolean; strike: boolean; color: string; size: number; family: string; hl?: string; rise: number }
export interface LaidSeg { text: string; style: ResolvedStyle; width: number }
export interface LaidLine { segs: LaidSeg[]; height: number; ascent: number; align: 'left' | 'center' | 'right' | 'justify'; indent: number; justify: boolean; marker?: { text: string; style: ResolvedStyle; width: number; x: number } }

export interface RichDefaults { bold: boolean; italic: boolean; underline: boolean; color: string; size: number; family: string; align: 'left' | 'center' | 'right' | 'justify' }

export function resolveStyle(run: RichRun, d: RichDefaults): ResolvedStyle {
  const sup = !!run.sup, sub = !!run.sub
  const baseSize = run.size ?? d.size
  return {
    bold: run.b ?? d.bold,
    italic: run.i ?? d.italic,
    underline: run.u ?? d.underline,
    strike: !!run.s,
    color: run.color ?? d.color,
    size: (sup || sub) ? baseSize * 0.66 : baseSize,
    family: d.family,
    hl: run.hl,
    // Décalage de ligne de base, en fraction de la taille (négatif = vers le haut).
    rise: sup ? -0.42 : sub ? 0.16 : 0,
  }
}

// Lay paragraphs out into lines fitting `maxW`. `measure(text, style)` returns the
// width of `text` in the given resolved style (canvas px). `sizeScale` multiplies
// every font size (used by autofit-shrink and the canvas device scale).
export function layoutRich(
  paras: RichPara[],
  defaults: RichDefaults,
  maxW: number,
  measure: (text: string, style: ResolvedStyle) => number,
  sizeScale = 1,
): LaidLine[] {
  const lines: LaidLine[] = []
  const scaleStyle = (s: ResolvedStyle): ResolvedStyle => ({ ...s, size: s.size * sizeScale })
  let numberCounter = 0

  for (const para of paras) {
    const attrs = para.attrs ?? {}
    const pAlign = attrs.align ?? defaults.align
    const indentPx = (attrs.indent ?? 0) * sizeScale
    const lhMul = attrs.lineHeight ?? 1.3
    // Marqueur de liste (puce / numéro), basé sur le style par défaut.
    let marker: { text: string; style: ResolvedStyle; width: number } | undefined
    if (attrs.list === 'bullet') {
      const style = scaleStyle(resolveStyle({ text: '' }, defaults))
      marker = { text: '•  ', style, width: measure('•  ', style) }
      numberCounter = 0
    } else if (attrs.list === 'number') {
      numberCounter += 1
      const style = scaleStyle(resolveStyle({ text: '' }, defaults))
      const text = `${numberCounter}.  `
      marker = { text, style, width: measure(text, style) }
    } else {
      numberCounter = 0
    }
    const leftPad = indentPx + (marker ? marker.width : 0)
    const lineMaxW = Math.max(1, maxW - leftPad)

    const runs = para.runs.length ? para.runs : [{ text: '' }]
    const tokens: { text: string; style: ResolvedStyle }[] = []
    for (const run of runs) {
      const style = scaleStyle(resolveStyle(run, defaults))
      if (!run.text) { tokens.push({ text: '', style }); continue }
      for (const tk of run.text.split(/(\s+)/)) if (tk !== '') tokens.push({ text: tk, style })
    }
    const paraLineStart = lines.length
    let cur: LaidSeg[] = []
    let curW = 0
    let maxSize = defaults.size * sizeScale
    const flush = () => {
      lines.push({ segs: cur, height: maxSize * lhMul, ascent: maxSize, align: pAlign, indent: leftPad, justify: false })
      cur = []; curW = 0; maxSize = defaults.size * sizeScale
    }
    const pushSeg = (text: string, style: ResolvedStyle, w: number) => {
      const last = cur[cur.length - 1]
      if (last && sameResolved(last.style, style)) { last.text += text; last.width += w }
      else cur.push({ text, style, width: w })
      curW += w
      if (style.size > maxSize) maxSize = style.size
    }
    for (const tok of tokens) {
      if (tok.text === '') continue
      const w = measure(tok.text, tok.style)
      const isSpace = /^\s+$/.test(tok.text)
      if (!isSpace && curW > 0 && curW + w > lineMaxW) flush()
      if (isSpace && curW === 0) continue
      pushSeg(tok.text, tok.style, w)
    }
    flush()
    // Marqueur de liste sur la 1re ligne du paragraphe ; justification sauf dernière ligne.
    if (marker && lines[paraLineStart]) lines[paraLineStart].marker = { ...marker, x: indentPx }
    if (pAlign === 'justify') for (let i = paraLineStart; i < lines.length - 1; i++) lines[i].justify = true
  }
  return lines
}

const sameResolved = (a: ResolvedStyle, b: ResolvedStyle) =>
  a.bold === b.bold && a.italic === b.italic && a.underline === b.underline && a.strike === b.strike && a.color === b.color && a.size === b.size && a.family === b.family && (a.hl ?? '') === (b.hl ?? '') && a.rise === b.rise

// Plain text of a doc (newline per paragraph).
export function parasToPlain(paras: RichPara[]): string {
  return paras.map(p => p.runs.map(r => r.text).join('')).join('\n')
}
