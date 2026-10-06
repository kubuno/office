// Multi-page support for the Maths module. A document's pages are stored INSIDE the single
// `latex` field (no backend change needed; only this module reads it): a single page stays
// plain LaTeX (backward compatible), several pages are wrapped in a small JSON envelope.
import { type GraphSpec, normalizeGraphSpec } from './mathGraph'

export interface MathPage { name: string; latex: string }

const HEADER = '%%KBMATH-MULTIPAGE%%'
const DOC_HEADER = '%%KBMATH-DOC%%'

// ── Paper formats (mm, portrait) for the page sheet ──────────────────────────────
export interface PaperFormat { id: string; name: string; w: number; h: number }
export const PAGE_FORMATS: PaperFormat[] = [
  { id: 'a3', name: 'A3', w: 297, h: 420 },
  { id: 'a4', name: 'A4', w: 210, h: 297 },
  { id: 'a5', name: 'A5', w: 148, h: 210 },
  { id: 'b5', name: 'B5', w: 176, h: 250 },
  { id: 'letter', name: 'Letter', w: 216, h: 279 },
  { id: 'legal', name: 'Legal', w: 216, h: 356 },
]
export const DEFAULT_FORMAT = 'a4'
export type Orientation = 'portrait' | 'landscape'

// Pixel size of a sheet at ~96 dpi (3.78 px/mm), honouring orientation.
export function paperPx(format: string, orientation: Orientation, pxPerMm = 3.78): { w: number; h: number } {
  const f = PAGE_FORMATS.find(p => p.id === format) ?? PAGE_FORMATS[1]
  const w = orientation === 'landscape' ? f.h : f.w
  const h = orientation === 'landscape' ? f.w : f.h
  return { w: Math.round(w * pxPerMm), h: Math.round(h * pxPerMm) }
}

// ── Block document model: a page holds blocks (formula, graph or text) freely placed ──
// on the sheet. `frame` is the block's position/size in sheet px (y grows downward);
// array order is the z-order (later = on top) . Legacy documents without frames are
// laid out as the old vertical stack when opened.
export type BlockAlign = 'left' | 'center' | 'right'
export type TextStyle = 'h1' | 'h2' | 'p'
export interface BlockFrame { x: number; y: number; w: number; h?: number; r?: number }  // r = rotation (deg)
// `scale` multiplies the rendered content size (formula font / text font); corner-resizing a
// block updates it so the content visually follows the box, like an object.
export type MathBlock =
  | { type: 'formula'; latex: string; align?: BlockAlign; numbered?: boolean; frame?: BlockFrame; scale?: number }
  | { type: 'graph'; spec: GraphSpec; frame?: BlockFrame }
  | { type: 'text'; text: string; style?: TextStyle; frame?: BlockFrame; scale?: number }
export interface DocPage { name: string; blocks: MathBlock[]; format?: string; orientation?: Orientation }

// Margin used for default block placement on the sheet.
export const SHEET_PAD = 24

function normalizeFrame(f: unknown): BlockFrame | undefined {
  if (!f || typeof f !== 'object') return undefined
  const o = f as Record<string, unknown>
  const num = (v: unknown) => (typeof v === 'number' && isFinite(v)) ? v : null
  const x = num(o.x), y = num(o.y), w = num(o.w)
  if (x == null || y == null || w == null) return undefined
  const h = num(o.h), r = num(o.r)
  return { x, y, w, ...(h != null ? { h } : {}), ...(r ? { r } : {}) }
}

// Read a document into pages-of-blocks, accepting every historical format:
//   plain LaTeX            → one page, one formula block
//   %%KBMATH-MULTIPAGE%%   → one formula block per page
//   %%KBMATH-DOC%%         → the block format itself
export function parseDoc(src: string | null | undefined): DocPage[] {
  const s = src ?? ''
  if (s.startsWith(DOC_HEADER)) {
    try {
      const j = JSON.parse(s.slice(DOC_HEADER.length))
      if (j && Array.isArray(j.pages) && j.pages.length) {
        return j.pages.map((p: { name?: unknown; blocks?: unknown; format?: unknown; orientation?: unknown }, i: number): DocPage => ({
          name: typeof p.name === 'string' ? p.name : `Page ${i + 1}`,
          blocks: Array.isArray(p.blocks) ? (p.blocks as unknown[]).map(normalizeBlock).filter(Boolean) as MathBlock[] : [],
          format: typeof p.format === 'string' ? p.format : undefined,
          orientation: p.orientation === 'landscape' ? 'landscape' : p.orientation === 'portrait' ? 'portrait' : undefined,
        })).map((p: DocPage) => p.blocks.length ? p : { ...p, blocks: [{ type: 'formula', latex: '' }] })
      }
    } catch { /* fall through */ }
  }
  // Legacy formats → one formula block per page.
  return parsePages(s).map(p => ({ name: p.name, blocks: [{ type: 'formula', latex: p.latex }] as MathBlock[] }))
}

function normalizeBlock(b: unknown): MathBlock | null {
  if (!b || typeof b !== 'object') return null
  const o = b as Record<string, unknown>
  const frame = normalizeFrame(o.frame)
  const scale = (typeof o.scale === 'number' && isFinite(o.scale) && o.scale > 0 && o.scale !== 1)
    ? Math.max(0.2, Math.min(8, o.scale)) : undefined
  if (o.type === 'graph') return { type: 'graph', spec: normalizeGraphSpec(o.spec), frame }
  if (o.type === 'text') {
    return {
      type: 'text',
      text: typeof o.text === 'string' ? o.text : '',
      style: o.style === 'h1' || o.style === 'h2' ? o.style : 'p',
      frame,
      scale,
    }
  }
  return {
    type: 'formula',
    latex: typeof o.latex === 'string' ? o.latex : '',
    align: o.align === 'left' || o.align === 'right' ? o.align : undefined,
    numbered: o.numbered === true || undefined,
    frame,
    scale,
  }
}

// Serialise pages-of-blocks. The trivial "one page, one formula" case stays plain LaTeX so a
// simple formula keeps a clean source (and renders in file previews); anything richer uses the
// block envelope.
export function serializeDoc(pages: DocPage[]): string {
  const p0 = pages[0]
  const b0 = p0.blocks[0]
  // A frame equal to the default full-width placement carries no information: dropping it
  // keeps the simple one-formula case stored as plain LaTeX (clean previews, back-compat).
  const defaultFrame = (f: BlockFrame | undefined): boolean => {
    if (!f) return true
    const sw = paperPx(p0.format ?? DEFAULT_FORMAT, p0.orientation ?? 'portrait').w
    return f.x === SHEET_PAD && f.y === SHEET_PAD && f.w === sw - SHEET_PAD * 2 && f.h == null && !f.r
  }
  const trivial = pages.length === 1 && p0.blocks.length === 1 && b0.type === 'formula'
    && (!b0.align || b0.align === 'center') && !b0.numbered && defaultFrame(b0.frame)
    && (!b0.scale || b0.scale === 1)
    && (!p0.format || p0.format === DEFAULT_FORMAT) && (!p0.orientation || p0.orientation === 'portrait')
  if (trivial) return (b0 as { latex: string }).latex
  return DOC_HEADER + JSON.stringify({ v: 1, pages })
}

export function parsePages(src: string | null | undefined): MathPage[] {
  const s = src ?? ''
  if (s.startsWith(HEADER)) {
    try {
      const j = JSON.parse(s.slice(HEADER.length))
      if (j && Array.isArray(j.pages) && j.pages.length) {
        return j.pages.map((p: { name?: unknown; latex?: unknown }, i: number) => ({
          name: typeof p.name === 'string' ? p.name : `Page ${i + 1}`,
          latex: typeof p.latex === 'string' ? p.latex : '',
        }))
      }
    } catch { /* fall through to single page */ }
  }
  return [{ name: 'Page 1', latex: s }]
}

export function serializePages(pages: MathPage[]): string {
  if (pages.length <= 1) return pages[0]?.latex ?? ''
  return HEADER + JSON.stringify({ pages })
}

// ── Visual (WYSIWYG) helpers — placeholders are `\square` tokens in the source ─────
const SQ = '\\square'

export function countSquares(src: string): number {
  let n = 0, i = -1
  while ((i = src.indexOf(SQ, i + 1)) >= 0) n++
  return n
}

// Index in the source string of the n-th `\square` (-1 if absent).
export function nthSquarePos(src: string, n: number): number {
  let idx = 0, i = -1
  while ((i = src.indexOf(SQ, i + 1)) >= 0) { if (idx === n) return i; idx++ }
  return -1
}

// Replace the n-th `\square` with `replacement` (appends if there is none).
export function replaceNthSquare(src: string, n: number, replacement: string): string {
  const at = nthSquarePos(src, n)
  if (at < 0) return src + replacement
  return src.slice(0, at) + replacement + src.slice(at + SQ.length)
}

// Wrap every `\square` in an `\htmlClass` so the rendered boxes are clickable + indexable;
// the active one gets an extra class for highlighting. (KaTeX needs `trust` for \htmlClass.)
export function markPlaceholders(src: string, active: number): string {
  let idx = 0
  return src.replace(/\\square/g, () => {
    const cls = `kbph kbph-${idx}${idx === active ? ' kbph-active' : ''}`
    idx++
    return `\\htmlClass{${cls}}{\\square}`
  })
}

// Like markPlaceholders, but the ACTIVE placeholder shows the live typing buffer (so the user
// sees what they type before committing). Invalid mid-typing LaTeX renders as an inline error
// (throwOnError:false) without blanking the whole formula.
export function markPlaceholdersBuf(src: string, active: number, buf: string): string {
  let idx = 0
  return src.replace(/\\square/g, () => {
    const i = idx++
    const cls = `kbph kbph-${i}${i === active ? ' kbph-active' : ''}`
    const inner = (i === active && buf) ? buf : '\\square'
    return `\\htmlClass{${cls}}{${inner}}`
  })
}
