// Clean printing for the spreadsheet: renders the chosen print area as a real,
// paginated HTML <table> inside a hidden same-origin iframe (paper-sized via @page),
// then prints only that iframe. The on-screen grid is a <canvas>, so window.print()
// alone would dump the whole app shell — this reproduces the sheet as print-ready HTML,
// honouring cell styles, borders, column widths, row heights and merges. It also powers
// the live preview in PrintDialog (same generator → WYSIWYG).
import type { CellStyle } from './api'
import { paperPx, PAGE_FORMATS } from './mathPages'

export type PrintScaleMode = 'normal' | 'fitWidth' | 'fitPage' | 'custom'

export interface PrintOptions {
  orientation: 'portrait' | 'landscape'
  paper:       string          // 'a4' | 'a3' | 'letter' | 'legal' (PAGE_FORMATS id)
  scaleMode:   PrintScaleMode
  scalePct:    number          // used when scaleMode === 'custom'
  gridlines:   boolean
  headings:    boolean         // print A/B/C column + 1/2/3 row headers
  repeatRows:  number          // number of top rows repeated on every page (title rows)
  marginMm:    number          // uniform page margin
  centerH:     boolean         // centre the table horizontally on the page
  title:       string
}

export const DEFAULT_PRINT_OPTIONS: PrintOptions = {
  orientation: 'portrait', paper: 'a4', scaleMode: 'normal', scalePct: 100,
  gridlines: true, headings: false, repeatRows: 0, marginMm: 12, centerH: false, title: '',
}

// One printable cell resolved by the caller (value already formatted, style attached).
export interface PrintCell {
  text: string
  style?: CellStyle
  num?: boolean          // right-align default when no explicit align
  cs?: number            // colspan (merge anchor), default 1
  rs?: number            // rowspan (merge anchor), default 1
}

// A floating object (chart SVG serialized by the caller) overlaid on the table.
// x/y are CSS px relative to the top-left DATA cell of the printed window
// (heading bands, when enabled, are added by the page builder).
export interface PrintObject {
  x: number; y: number; w: number; h: number
  svg: string
}

export interface PrintGrid {
  cols: { letter: string; width: number }[]   // in CSS px (screen units)
  rows: { index: number; height: number }[]   // 1-based sheet row number + px height
  // cells[rowIdx][colIdx]: a PrintCell, or null when covered by a merge above/left.
  cells: (PrintCell | null)[][]
  objects?: PrintObject[]                     // floating charts over the grid
}

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

const HEAD_W = 42   // row-header column width (px)
const HEAD_H = 22   // column-header row height (px)

function borderCss(color: string | undefined, weight: number | undefined, dflt: string): string {
  if (!color) return dflt
  const w = weight === 3 ? 2.5 : weight === 2 ? 1.6 : 1
  return `${w}px solid ${color}`
}

function cellStyleCss(c: PrintCell, gridlines: boolean): string {
  const s = c.style ?? {}
  const parts: string[] = []
  const grid = gridlines ? '1px solid #d0d0d0' : '1px solid transparent'
  // Per-edge borders win over the gridline default.
  parts.push(`border-top:${borderCss(s.bt, s.btw, grid)}`)
  parts.push(`border-right:${borderCss(s.br, s.brw, grid)}`)
  parts.push(`border-bottom:${borderCss(s.bb, s.bbw, grid)}`)
  parts.push(`border-left:${borderCss(s.bl, s.blw, grid)}`)
  if (s.bg) parts.push(`background:${s.bg}`)
  if (s.color) parts.push(`color:${s.color}`)
  if (s.bold) parts.push('font-weight:700')
  if (s.italic) parts.push('font-style:italic')
  if (s.underline && s.strike) parts.push('text-decoration:underline line-through')
  else if (s.underline) parts.push('text-decoration:underline')
  else if (s.strike) parts.push('text-decoration:line-through')
  if (s.fontSize) parts.push(`font-size:${s.fontSize}px`)
  if (s.fontFamily) parts.push(`font-family:${s.fontFamily.replace(/[<>"]/g, '')},sans-serif`)
  const align = s.align ?? (c.num ? 'right' : 'left')
  parts.push(`text-align:${align}`)
  const valign = s.valign === 'center' ? 'middle' : s.valign === 'bottom' ? 'bottom' : s.valign ?? 'bottom'
  parts.push(`vertical-align:${valign}`)
  parts.push(s.wrap ? 'white-space:normal;word-break:break-word' : 'white-space:nowrap;overflow:hidden')
  return parts.join(';')
}

/** Build the printable <table> for a grid (no <html> wrapper — used by page builder). */
function tableHtml(grid: PrintGrid, opt: PrintOptions): string {
  const showHead = opt.headings
  const colgroup = [
    showHead ? `<col style="width:${HEAD_W}px">` : '',
    ...grid.cols.map(c => `<col style="width:${c.width}px">`),
  ].join('')

  const headBg = '#f1f3f4', headColor = '#5f6368'
  const colHeaderRow = showHead
    ? `<tr style="height:${HEAD_H}px">` +
      `<th style="width:${HEAD_W}px;background:${headBg};border:1px solid #c0c4c8"></th>` +
      grid.cols.map(c => `<th style="background:${headBg};color:${headColor};border:1px solid #c0c4c8;font-weight:600;font-size:11px;text-align:center">${esc(c.letter)}</th>`).join('') +
      '</tr>'
    : ''

  const rowHtml = (rIdx: number): string => {
    const r = grid.rows[rIdx]
    const rowHead = showHead
      ? `<th style="background:${headBg};color:${headColor};border:1px solid #c0c4c8;font-weight:600;font-size:11px;text-align:center">${r.index}</th>`
      : ''
    const tds = grid.cells[rIdx].map((cell, cIdx) => {
      if (cell === null) return '' // covered by a merge
      const span = (cell.cs && cell.cs > 1 ? ` colspan="${cell.cs}"` : '') + (cell.rs && cell.rs > 1 ? ` rowspan="${cell.rs}"` : '')
      void cIdx
      return `<td${span} style="${cellStyleCss(cell, opt.gridlines)};padding:1px 4px;font-size:13px;line-height:1.25">${esc(cell.text)}</td>`
    }).join('')
    return `<tr style="height:${r.height}px">${rowHead}${tds}</tr>`
  }

  const repeat = Math.min(opt.repeatRows, grid.rows.length)
  const theadRows = colHeaderRow + Array.from({ length: repeat }, (_, i) => rowHtml(i)).join('')
  const bodyRows = grid.rows.map((_, i) => i).filter(i => i >= repeat).map(rowHtml).join('')

  return `<table style="border-collapse:collapse;table-layout:fixed;font-family:Outfit,Inter,Arial,sans-serif;color:#202124">
<colgroup>${colgroup}</colgroup>
${theadRows ? `<thead>${theadRows}</thead>` : ''}
<tbody>${bodyRows}</tbody>
</table>`
}

/** Total table width in px (headers + columns) — used to compute the fit scale. */
function tableWidth(grid: PrintGrid, opt: PrintOptions): number {
  return (opt.headings ? HEAD_W : 0) + grid.cols.reduce((a, c) => a + c.width, 0)
}
function tableHeight(grid: PrintGrid, opt: PrintOptions): number {
  return (opt.headings ? HEAD_H : 0) + grid.rows.reduce((a, r) => a + r.height, 0)
}

/** Effective scale factor (0..n) applied to the whole table. */
export function printScale(grid: PrintGrid, opt: PrintOptions): number {
  const { w, h } = paperPx(opt.paper, opt.orientation, 3.78)
  const mm = opt.marginMm * 3.78
  const availW = w - mm * 2, availH = h - mm * 2
  if (opt.scaleMode === 'custom') return Math.max(0.1, Math.min(4, opt.scalePct / 100))
  if (opt.scaleMode === 'fitWidth') return Math.min(1, availW / tableWidth(grid, opt))
  if (opt.scaleMode === 'fitPage') return Math.min(1, availW / tableWidth(grid, opt), availH / tableHeight(grid, opt))
  return 1
}

/** Full HTML document (paper-sized sheets) for printing OR previewing. */
export function buildSheetPrintDoc(grid: PrintGrid, opt: PrintOptions, forPreview = false): string {
  const fmt = PAGE_FORMATS.find(p => p.id === opt.paper) ?? PAGE_FORMATS[1]
  const scale = printScale(grid, opt)
  const table = tableHtml(grid, opt)
  // Floating charts: absolutely positioned over the table (their SVG strings are
  // produced by the app from its own rendered charts — trusted markup).
  const ox = opt.headings ? HEAD_W : 0, oy = opt.headings ? HEAD_H : 0
  const objectsHtml = (grid.objects ?? []).map(o =>
    `<div style="position:absolute;left:${ox + o.x}px;top:${oy + o.y}px;width:${o.w}px;height:${o.h}px;overflow:hidden">${o.svg}</div>`).join('')
  const content = objectsHtml ? `<div style="position:relative;width:max-content">${table}${objectsHtml}</div>` : table
  const scaledWrap = `<div style="transform:scale(${scale});transform-origin:top ${opt.centerH ? 'center' : 'left'};width:max-content${opt.centerH ? ';margin:0 auto' : ''}">${content}</div>`

  // For the preview we mimic a single paper sheet with a drop shadow; the real print
  // relies on @page + the browser's automatic pagination of the tall table.
  // a3/a4/a5/b5/letter/legal are all valid CSS @page size keywords (case-insensitive).
  const pageCss = `@page { size: ${opt.paper} ${opt.orientation}; margin: ${opt.marginMm}mm; }`
  const previewSheetStyle = forPreview
    ? `width:${(opt.orientation === 'landscape' ? fmt.h : fmt.w)}mm;min-height:${(opt.orientation === 'landscape' ? fmt.w : fmt.h)}mm;padding:${opt.marginMm}mm;margin:0 auto 16px;background:#fff;box-shadow:0 1px 6px rgba(0,0,0,.25);box-sizing:border-box;overflow:hidden`
    : `padding:0`

  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(opt.title || 'Feuille')}</title>
<style>
  ${pageCss}
  * { -webkit-print-color-adjust: exact; print-color-adjust: exact; box-sizing: border-box; }
  html,body { margin:0; padding:0; background:${forPreview ? '#e8eaed' : '#fff'}; }
  ${forPreview ? '.sheet-wrap{padding:16px 0;}' : ''}
</style></head>
<body><div class="${forPreview ? 'sheet-wrap' : ''}"><div style="${previewSheetStyle}">${scaledWrap}</div></div></body></html>`
}

/** Render to a hidden iframe and fire the browser print dialog on it only. */
export function printSheet(grid: PrintGrid, opt: PrintOptions): void {
  const prev = document.getElementById('kb-sheet-print-frame')
  if (prev) prev.remove()
  const iframe = document.createElement('iframe')
  iframe.id = 'kb-sheet-print-frame'
  iframe.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0;visibility:hidden'
  document.body.appendChild(iframe)
  const doc = iframe.contentWindow?.document
  if (!doc) return
  doc.open()
  doc.write(buildSheetPrintDoc(grid, opt, false))
  doc.close()
  const go = () => {
    const win = iframe.contentWindow
    if (!win) return
    win.focus()
    win.print()
    // Give the print dialog time to grab the document before tearing it down.
    setTimeout(() => iframe.remove(), 1000)
  }
  // Wait for layout/fonts before printing.
  if (iframe.contentWindow?.document.readyState === 'complete') setTimeout(go, 120)
  else iframe.onload = () => setTimeout(go, 120)
}
