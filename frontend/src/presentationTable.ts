// Table rendering + structural helpers for presentation table elements. Pure
// functions so they can be unit-tested and validated with a headless screenshot.
import type { TableElement, TableCell } from './api'

interface Ctx2D {
  fillStyle: string | CanvasGradient | CanvasPattern; strokeStyle: string | CanvasGradient | CanvasPattern; lineWidth: number; font: string
  textAlign: CanvasTextAlign; textBaseline: CanvasTextBaseline
  fillRect(x: number, y: number, w: number, h: number): void
  strokeRect(x: number, y: number, w: number, h: number): void
  beginPath(): void; moveTo(x: number, y: number): void; lineTo(x: number, y: number): void; stroke(): void
  fillText(t: string, x: number, y: number): void
  save(): void; restore(): void
  rect(x: number, y: number, w: number, h: number): void; clip(): void
}

const emptyCell = (): TableCell => ({ text: '' })

export function makeTableCells(rows: number, cols: number): TableCell[][] {
  return Array.from({ length: rows }, () => Array.from({ length: cols }, emptyCell))
}

// Column / row pixel boundaries from fractional widths (or equal).
export function colEdges(el: TableElement, w: number): number[] {
  const widths = el.colWidths?.length === el.cols ? el.colWidths : Array(el.cols).fill(1 / el.cols)
  const sum = widths.reduce((a, b) => a + b, 0) || 1
  const edges = [0]
  for (let c = 0; c < el.cols; c++) edges.push(edges[c] + (widths[c] / sum) * w)
  return edges
}
export function rowEdges(el: TableElement, h: number): number[] {
  const heights = el.rowHeights?.length === el.rows ? el.rowHeights : Array(el.rows).fill(1 / el.rows)
  const sum = heights.reduce((a, b) => a + b, 0) || 1
  const edges = [0]
  for (let r = 0; r < el.rows; r++) edges.push(edges[r] + (heights[r] / sum) * h)
  return edges
}

// Returns the [row,col] for a point (fraction inside the table box), or null.
export function cellAt(el: TableElement, fx: number, fy: number): { row: number; col: number } | null {
  if (fx < 0 || fx > 1 || fy < 0 || fy > 1) return null
  const cx = colEdges(el, 1), ry = rowEdges(el, 1)
  let col = -1, row = -1
  for (let c = 0; c < el.cols; c++) if (fx >= cx[c] && fx < cx[c + 1]) { col = c; break }
  for (let r = 0; r < el.rows; r++) if (fy >= ry[r] && fy < ry[r + 1]) { row = r; break }
  if (col < 0 || row < 0) return null
  return { row, col }
}

export function renderTable(ctx: Ctx2D, el: TableElement, x: number, y: number, w: number, h: number, sf = 1) {
  const cx = colEdges(el, w), ry = rowEdges(el, h)
  const border = el.borderColor ?? '#9aa0a6'
  const headerBg = el.headerBg ?? '#1a73e8'
  const bandBg = el.bandBg ?? '#f1f3f4'
  const fs = (el.fontSize ?? 14) * sf
  ctx.save()
  ctx.beginPath(); ctx.rect(x, y, w, h); ctx.clip()
  // Remplissages (en-tête, bandes, cellule).
  for (let r = 0; r < el.rows; r++) {
    for (let c = 0; c < el.cols; c++) {
      const cell = el.cells?.[r]?.[c]
      let bg: string | undefined = cell?.bg
      if (!bg && el.headerRow && r === 0) bg = headerBg
      else if (!bg && el.banded && (el.headerRow ? r % 2 === 0 : r % 2 === 1)) bg = bandBg
      if (bg) { ctx.fillStyle = bg; ctx.fillRect(x + cx[c], y + ry[r], cx[c + 1] - cx[c], ry[r + 1] - ry[r]) }
    }
  }
  // Lignes de grille.
  ctx.strokeStyle = border; ctx.lineWidth = 1 * sf
  ctx.beginPath()
  for (let c = 0; c <= el.cols; c++) { ctx.moveTo(x + cx[c], y); ctx.lineTo(x + cx[c], y + h) }
  for (let r = 0; r <= el.rows; r++) { ctx.moveTo(x, y + ry[r]); ctx.lineTo(x + w, y + ry[r]) }
  ctx.stroke()
  // Texte des cellules.
  ctx.textBaseline = 'middle'
  for (let r = 0; r < el.rows; r++) {
    for (let c = 0; c < el.cols; c++) {
      const cell = el.cells?.[r]?.[c]
      if (!cell?.text) continue
      const isHeader = el.headerRow && r === 0
      const bold = cell.bold || isHeader || (!!el.firstCol && c === 0)
      ctx.font = `${bold ? 'bold ' : ''}${fs}px Arial, sans-serif`
      ctx.fillStyle = cell.color ?? (isHeader ? '#ffffff' : '#202124')
      const align = cell.align ?? 'left'
      ctx.textAlign = align
      const pad = 6 * sf
      const tx = align === 'center' ? x + (cx[c] + cx[c + 1]) / 2 : align === 'right' ? x + cx[c + 1] - pad : x + cx[c] + pad
      const ty = y + (ry[r] + ry[r + 1]) / 2
      // Tronque grossièrement au besoin (pas de wrap pour rester simple).
      ctx.fillText(cell.text, tx, ty)
    }
  }
  ctx.restore()
}

// ── Structural operations (pure, return new cells/dims) ─────────────────────────

export function addRow(el: TableElement, at?: number): Partial<TableElement> {
  const idx = at ?? el.rows
  const cells = el.cells.map(r => r.slice())
  cells.splice(idx, 0, Array.from({ length: el.cols }, emptyCell))
  return { rows: el.rows + 1, cells, rowHeights: undefined }
}
export function addCol(el: TableElement, at?: number): Partial<TableElement> {
  const idx = at ?? el.cols
  const cells = el.cells.map(r => { const nr = r.slice(); nr.splice(idx, 0, emptyCell()); return nr })
  return { cols: el.cols + 1, cells, colWidths: undefined }
}
export function delRow(el: TableElement, at: number): Partial<TableElement> {
  if (el.rows <= 1) return {}
  const cells = el.cells.map(r => r.slice()); cells.splice(at, 1)
  return { rows: el.rows - 1, cells, rowHeights: undefined }
}
export function delCol(el: TableElement, at: number): Partial<TableElement> {
  if (el.cols <= 1) return {}
  const cells = el.cells.map(r => { const nr = r.slice(); nr.splice(at, 1); return nr })
  return { cols: el.cols - 1, cells, colWidths: undefined }
}
export function setCell(el: TableElement, row: number, col: number, patch: Partial<TableCell>): TableCell[][] {
  return el.cells.map((r, ri) => r.map((c, ci) => ri === row && ci === col ? { ...c, ...patch } : c))
}

// Table style presets (header / band colours + border).
export const TABLE_STYLES: { name: string; headerBg: string; bandBg: string; borderColor: string }[] = [
  { name: 'Bleu', headerBg: '#1a73e8', bandBg: '#e8f0fe', borderColor: '#a8c7fa' },
  { name: 'Vert', headerBg: '#34a853', bandBg: '#e6f4ea', borderColor: '#a8dab5' },
  { name: 'Gris', headerBg: '#5f6368', bandBg: '#f1f3f4', borderColor: '#bdc1c6' },
  { name: 'Rouge', headerBg: '#ea4335', bandBg: '#fce8e6', borderColor: '#f5b3ac' },
  { name: 'Minimal', headerBg: '#202124', bandBg: '#ffffff', borderColor: '#dadce0' },
]
