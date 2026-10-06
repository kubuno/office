// Named colour and line presets for shape menus.
//
// Presentation follows the documents editor: a submenu of NAMED colours with a
// check mark on the current one, rather than an anonymous swatch grid. Naming the
// colours makes the menu readable, keyboard-navigable and translatable, and the
// check mark answers "what is applied right now?" without opening a dialog.
// The palette itself is the document editor's, so a shape looks the same whether
// it was drawn in a text document or in a sheet.

import type { TFunction } from 'i18next'

export interface ShapePreset {
  /** i18n key + French default, resolved by the caller. */
  key: string
  fallback: string
  fill: string
  stroke: string
}

/** Fill + matching outline pairs (documents' `FILLS`, same values). */
export const SHAPE_PRESETS: ShapePreset[] = [
  { key: 'doc_color_blue',   fallback: 'Bleu',   fill: '#dbe7ff', stroke: '#1a73e8' },
  { key: 'doc_color_green',  fallback: 'Vert',   fill: '#d8f3e3', stroke: '#1e8e3e' },
  { key: 'doc_color_yellow', fallback: 'Jaune',  fill: '#fef3d0', stroke: '#f9ab00' },
  { key: 'doc_color_red',    fallback: 'Rouge',  fill: '#fde0dd', stroke: '#d93025' },
  { key: 'doc_color_gray',   fallback: 'Gris',   fill: '#eceff1', stroke: '#5f6368' },
  { key: 'doc_color_white',  fallback: 'Blanc',  fill: '#ffffff', stroke: '#202124' },
]

/** Outline-only colours (documents' `STROKES`). */
export const STROKE_PRESETS: ShapePreset[] = [
  { key: 'doc_color_gray',  fallback: 'Gris',  fill: '', stroke: '#9aa0a6' },
  { key: 'doc_color_blue',  fallback: 'Bleu',  fill: '', stroke: '#1a73e8' },
  { key: 'doc_color_green', fallback: 'Vert',  fill: '', stroke: '#1e8e3e' },
  { key: 'doc_color_red',   fallback: 'Rouge', fill: '', stroke: '#d93025' },
  { key: 'doc_color_black', fallback: 'Noir',  fill: '', stroke: '#202124' },
]

/** Outline widths offered in the Line submenu, in px. */
export const LINE_WIDTHS: { w: number; key: string; fallback: string }[] = [
  { w: 1, key: 'sheet_shape_line_thin',   fallback: 'Fin (1 px)' },
  { w: 2, key: 'sheet_shape_line_medium', fallback: 'Moyen (2 px)' },
  { w: 3, key: 'sheet_shape_line_thick',  fallback: 'Épais (3 px)' },
  { w: 6, key: 'sheet_shape_line_xthick', fallback: 'Très épais (6 px)' },
]

/** How a shape follows the cells underneath it — LibreOffice Calc's anchoring. */
export type ShapeAnchorMode = 'cell' | 'cellResize' | 'page'

export const ANCHOR_MODES: { mode: ShapeAnchorMode; key: string; fallback: string }[] = [
  { mode: 'cell',       key: 'sheet_shape_anchor_cell',        fallback: 'À la cellule' },
  { mode: 'cellResize', key: 'sheet_shape_anchor_cell_resize', fallback: 'À la cellule (redimensionner avec)' },
  { mode: 'page',       key: 'sheet_shape_anchor_page',        fallback: 'À la page' },
]

/** Resolve a preset's label through i18n. */
export function presetLabel(t: TFunction, p: { key: string; fallback: string }): string {
  return t(p.key, { defaultValue: p.fallback })
}

/** Same-colour test that tolerates case and the `#` prefix. */
export function sameColour(a?: string | null, b?: string | null): boolean {
  const n = (v?: string | null) => (v ?? '').trim().replace(/^#/, '').toLowerCase()
  return n(a) === n(b) && n(a) !== ''
}
