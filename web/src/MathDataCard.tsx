/**
 * `office.math` envelopes: envelope builder, live card and static render for
 * maths blocks copied with "Copier pour Kubuno". Registered on `core.data-card`
 * from `entry.ts`, so ANY consumer (chat, notes, documents…) can display a
 * formula without importing office's code.
 *
 * Live card: KaTeX HTML (office's stylesheet is injected host-wide when the
 * module loads, so the markup renders anywhere in the shell).
 * Static render (canvas consumers like the documents editor): KaTeX MathML in
 * an SVG `<foreignObject>` — self-contained, no external CSS — rasterised to
 * PNG, with a plain-text SVG of the LaTeX source as a last-resort fallback.
 */
import { useMemo } from 'react'
import katex from 'katex'
import { Sigma } from 'lucide-react'
import { renderLatex } from './latexRender'
import type { MathBlock } from './mathPages'
import type { DataCardProps, DataCardStaticRender, KubunoDataEnvelope } from './kubunoData'

export interface MathData {
  /** LaTeX source of the copied formula (empty for a text/graph-only block). */
  latex: string
  /** Full block, so a future paste back into maths can restore style/frame. */
  block?: MathBlock
  /** Source document, for the deep link. */
  formula_id?: string
}

export function mathEnvelope(latex: string, block?: MathBlock, formulaId?: string): KubunoDataEnvelope {
  const href = formulaId ? `/office/maths/${formulaId}` : '/office/maths'
  const trimmed = latex.trim()
  return {
    kubuno: 1,
    type: 'office.math',
    module: 'office',
    title: trimmed.length > 60 ? `${trimmed.slice(0, 57)}…` : (trimmed || 'Formule'),
    // Plain-text flavor: the LaTeX source is the useful thing to paste elsewhere.
    text: trimmed,
    href,
    data: { latex: trimmed, block, formula_id: formulaId } satisfies MathData,
  }
}

function mathDataOf(envelope: KubunoDataEnvelope): MathData | null {
  const d = envelope.data as MathData | null
  return d && typeof d.latex === 'string' ? d : null
}

/** Live React card — KaTeX HTML, styled by office's host-injected stylesheet. */
export function MathDataCard({ envelope }: DataCardProps) {
  const d = mathDataOf(envelope)
  const html = useMemo(() => (d ? renderLatex(d.latex).html : ''), [d])
  if (!d) return null
  return (
    <div className="w-72 max-w-full rounded-xl border border-border bg-surface-0 overflow-hidden">
      <div className="px-3 py-3 bg-white overflow-x-auto text-text-primary" dangerouslySetInnerHTML={{ __html: html }} />
      <div className="px-3 py-1.5 flex items-center gap-1.5 border-t border-border">
        <Sigma size={13} className="text-primary flex-shrink-0" />
        <p className="text-[11px] text-text-tertiary truncate font-mono">{d.latex}</p>
      </div>
    </div>
  )
}

function escapeXml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

/** Measures the formula by rendering KaTeX HTML off-screen (real font metrics). */
function measure(latex: string): { width: number; height: number } {
  const host = document.createElement('div')
  host.style.cssText = 'position:fixed;left:-9999px;top:0;visibility:hidden'
  host.innerHTML = renderLatex(latex).html
  document.body.appendChild(host)
  const r = host.getBoundingClientRect()
  const size = { width: Math.max(24, Math.ceil(r.width) + 16), height: Math.max(20, Math.ceil(r.height) + 12) }
  document.body.removeChild(host)
  return size
}

/** Self-contained SVG: MathML inside a foreignObject (no external CSS/fonts). */
function mathSvg(latex: string, width: number, height: number): string {
  let inner: string
  try {
    inner = katex.renderToString(latex, { displayMode: true, throwOnError: false, output: 'mathml', strict: false })
  } catch {
    inner = ''
  }
  const body = inner
    ? `<div xmlns="http://www.w3.org/1999/xhtml" style="font-size:18px;color:#000">${inner}</div>`
    : `<div xmlns="http://www.w3.org/1999/xhtml" style="font:16px monospace;color:#000">${escapeXml(latex)}</div>`
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`
    + `<rect width="100%" height="100%" fill="#ffffff"/>`
    + `<foreignObject x="6" y="4" width="${width - 12}" height="${height - 8}">${body}</foreignObject>`
    + `</svg>`
}

/** Last resort when foreignObject cannot be rasterised: the LaTeX source as text. */
function textSvg(latex: string, width: number, height: number): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`
    + `<rect width="100%" height="100%" fill="#ffffff"/>`
    + `<text x="8" y="${Math.round(height / 2) + 5}" font-family="monospace" font-size="14" fill="#000">${escapeXml(latex)}</text>`
    + `</svg>`
}

function svgDataUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
}

function rasterize(svg: string, w: number, h: number): Promise<string | null> {
  return new Promise(resolve => {
    const img = new window.Image()
    img.onload = () => {
      const canvas = document.createElement('canvas')
      const scale = 2   // crisp on screen and in print
      canvas.width = Math.max(1, Math.round(w * scale))
      canvas.height = Math.max(1, Math.round(h * scale))
      const ctx = canvas.getContext('2d')
      if (!ctx) { resolve(null); return }
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height)
      try { resolve(canvas.toDataURL('image/png')) } catch { resolve(null) }
    }
    img.onerror = () => resolve(null)
    img.src = svgDataUrl(svg)
  })
}

/** `renderStatic` of the data-card renderer: PNG/SVG for canvas consumers. */
export async function renderMathStatic(envelope: KubunoDataEnvelope): Promise<DataCardStaticRender | null> {
  const d = mathDataOf(envelope)
  if (!d || !d.latex) return null
  const { width, height } = measure(d.latex)
  const svg = mathSvg(d.latex, width, height)
  const dataUrl = await rasterize(svg, width, height)
  if (dataUrl) return { svg, dataUrl, width, height }
  // foreignObject was not rasterisable: fall back to a text-only SVG.
  const fallback = textSvg(d.latex, Math.max(width, d.latex.length * 9), height)
  const fbUrl = await rasterize(fallback, Math.max(width, d.latex.length * 9), height)
  return { svg: fallback, dataUrl: fbUrl ?? undefined, width: Math.max(width, d.latex.length * 9), height }
}

export default MathDataCard
