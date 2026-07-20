// Clean printing for the Maths module: renders every page of the document (formulas via
// KaTeX, graphs rasterised to PNG, text blocks as styled paragraphs) into a hidden
// same-origin iframe sized like the chosen paper format, then triggers the browser's
// print dialog on that iframe only. Much better than window.print() on the whole shell.
import { compile } from './mathExpr'
import { drawGraph, type GraphSpec } from './mathGraph'
import { renderLatex } from './latexRender'
import { paperPx, type DocPage, type MathBlock } from './mathPages'

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

// Rasterise a graph block at 2× for crisp printing.
function graphDataUrl(spec: GraphSpec, cssW: number): string {
  const cssH = Math.max(60, spec.height)
  const scale = 2
  const cv = document.createElement('canvas')
  cv.width = Math.round(cssW * scale); cv.height = Math.round(cssH * scale)
  const ctx = cv.getContext('2d')
  if (!ctx) return ''
  ctx.setTransform(scale, 0, 0, scale, 0, 0)
  ctx.fillStyle = '#ffffff'; ctx.fillRect(0, 0, cssW, cssH)
  drawGraph(ctx, spec, cssW, cssH, spec.fns.map(f => ({ ...compile(f.expr), color: f.color })))
  try { return cv.toDataURL('image/png') } catch { return '' }
}

function blockHtml(b: MathBlock, contentW: number, eqNo: number | null): string {
  // Blocks with a frame are placed absolutely on the sheet (free layout); legacy blocks
  // without one flow vertically like the old stacked layout.
  const f = b.frame
  const rot = f?.r ? `transform:rotate(${f.r}deg);` : ''
  const wrap = (inner: string, extra = ''): string => f
    ? `<div style="position:absolute;left:${f.x}px;top:${f.y}px;width:${f.w}px;${f.h ? `height:${f.h}px;overflow:hidden;` : ''}${rot}${extra}">${inner}</div>`
    : `<div class="blk" style="${extra}">${inner}</div>`
  if (b.type === 'graph') {
    const w = f?.w ?? contentW
    const url = graphDataUrl(b.spec, w)
    return url ? wrap(`<img src="${url}" style="width:100%;height:auto"/>`) : ''
  }
  if (b.type === 'text') {
    const style = b.style ?? 'p'
    const scale = b.scale ?? 1
    const base = style === 'h1' ? 24 : style === 'h2' ? 18 : 14
    const weight = style === 'h1' ? 'font-weight:700;' : style === 'h2' ? 'font-weight:600;' : ''
    return wrap(`<div style="font-size:${base * scale}px;${weight}white-space:pre-wrap">${esc(b.text)}</div>`)
  }
  const align = b.align ?? 'center'
  const num = eqNo != null ? `<span style="color:#5f6368;font-size:14px;margin-left:12px;flex-shrink:0">(${eqNo})</span>` : ''
  return wrap(`<div style="display:flex;align-items:center;${f?.h ? 'height:100%;' : ''}">
    <div style="flex:1;min-width:0;overflow:hidden;text-align:${align};font-size:${(b.scale ?? 1) * 100}%">${renderLatex(b.latex).html}</div>${num}
  </div>`)
}

export interface MathPrintOpts { colorize?: boolean; cssVars?: string }

export function printMathDoc(title: string, pages: DocPage[], opts: MathPrintOpts = {}): void {
  // Sequential equation numbers across the whole document.
  const eqNo = new Map<MathBlock, number>()
  let n = 0
  for (const p of pages) for (const b of p.blocks) if (b.type === 'formula' && b.numbered) eqNo.set(b, ++n)

  const sheets = pages.map(p => {
    const { w, h } = paperPx(p.format ?? 'a4', p.orientation ?? 'portrait')
    const pad = 32
    const contentW = w - pad * 2
    // Framed blocks use sheet coordinates (padding is part of the frame), legacy ones flow
    // inside the padding — position:relative anchors both.
    const anyFrame = p.blocks.some(b => b.frame)
    const body = p.blocks.map(b => blockHtml(b, contentW, eqNo.get(b) ?? null)).join('\n')
    return `<section class="sheet" style="position:relative;width:${w}px;min-height:${h}px;padding:${anyFrame ? 0 : pad}px">${body}</section>`
  }).join('\n')

  // Reuse the host page's stylesheets (KaTeX classes + fonts live in the module CSS).
  const headStyles = Array.from(document.querySelectorAll('link[rel="stylesheet"], style'))
    .map(el => el.outerHTML).join('\n')

  const wrapCls = opts.colorize ? 'mc-colorize' : ''
  const wrapStyle = opts.cssVars ? ` style="${opts.cssVars}"` : ''
  const html = `<!doctype html><html><head><meta charset="utf-8"><title>${esc(title)}</title>
${headStyles}
<style>
  html, body { margin:0; padding:0; background:#fff; color:#202124; font-family:system-ui, sans-serif }
  .sheet { box-sizing:border-box; margin:0 auto; page-break-after:always; break-after:page }
  .sheet:last-child { page-break-after:auto; break-after:auto }
  .blk { margin:0 0 14px }
  .mc-caret { display:none !important }
</style>
</head><body class="${wrapCls}"${wrapStyle}>${sheets}</body></html>`

  const iframe = document.createElement('iframe')
  iframe.style.position = 'fixed'
  iframe.style.right = '0'
  iframe.style.bottom = '0'
  iframe.style.width = '0'
  iframe.style.height = '0'
  iframe.style.border = '0'
  iframe.style.visibility = 'hidden'
  document.body.appendChild(iframe)
  const doc = iframe.contentDocument
  if (!doc) { iframe.remove(); return }
  doc.open(); doc.write(html); doc.close()
  // Give stylesheets/fonts/images a moment to settle before opening the dialog.
  window.setTimeout(() => {
    try { iframe.contentWindow?.focus(); iframe.contentWindow?.print() } catch { /* ignore */ }
    window.setTimeout(() => iframe.remove(), 2000)
  }, 350)
}
