// Default look of a freshly inserted shape, and the quick-style presets the mini
// toolbar offers.
//
// « Définir comme style de forme par défaut » stores the current shape's fill /
// outline as a PER-USER preference on the BACKEND (`core.users.preferences.office`
// via `useModulePrefs`), never in localStorage: the setting follows the user across
// browsers and devices, like every other Kubuno per-user module setting.

import { useCallback } from 'react'
import type { SheetShape, SheetShapeKind } from '../api'
import { useModulePrefs } from '../userPrefs'

/**
 * The office module's accent. Shapes are model data (a shape's colours are stored,
 * not themed), so the accent has to be resolved to a literal at insertion time —
 * this is the same blue `--color-primary` carries in theme.css.
 */
export const SHAPE_ACCENT = '#1a73e8'

/** Fill / outline / outline width of a shape — the slice a default style covers. */
export interface DefaultShapeStyle {
  /** "#RRGGBB" or 'none'. */
  fill: string
  /** "#RRGGBB" or 'none'. */
  border: string
  /** px. */
  borderWidth: number
}

/**
 * Factory default: WHITE fill, BLACK outline — the neutral pair a drawing starts
 * from, and what LibreOffice and Office give a freshly inserted shape. A tinted
 * default decides for the user; white on black decides nothing and prints well.
 */
export const DEFAULT_SHAPE_STYLE: DefaultShapeStyle = {
  fill: '#ffffff',
  border: '#000000',
  borderWidth: 1,
}

/** Geometry and size a shape gets when inserted from the gallery, in base px. */
export const DEFAULT_SHAPE_KIND: SheetShapeKind = 'rect'
export const DEFAULT_SHAPE_WIDTH = 160
export const DEFAULT_SHAPE_HEIGHT = 96

/** Quick styles of the mini toolbar's [Style ▾] panel. */
export interface ShapeStylePreset extends DefaultShapeStyle {
  id: string
  /** i18n key; `labelDefault` is the French fallback. */
  labelKey: string
  labelDefault: string
}

export const SHAPE_STYLE_PRESETS: readonly ShapeStylePreset[] = [
  // First entry = the factory default, so the gallery opens on what a new shape looks like.
  { id: 'default', labelKey: 'sheet_shape_style_default', labelDefault: 'Blanc, contour noir', fill: '#ffffff', border: '#000000', borderWidth: 1 },
  { id: 'accent-tint', labelKey: 'sheet_shape_style_tint', labelDefault: 'Bleu clair', fill: '#dbe7ff', border: SHAPE_ACCENT, borderWidth: 1 },
  { id: 'accent-solid', labelKey: 'sheet_shape_style_solid', labelDefault: 'Bleu plein', fill: SHAPE_ACCENT, border: '#1557b0', borderWidth: 1 },
  { id: 'outline', labelKey: 'sheet_shape_style_outline', labelDefault: 'Contour seul', fill: 'none', border: SHAPE_ACCENT, borderWidth: 2 },
  { id: 'grey', labelKey: 'sheet_shape_style_grey', labelDefault: 'Gris clair', fill: '#f1f3f4', border: '#9aa0a6', borderWidth: 1 },
  { id: 'green', labelKey: 'sheet_shape_style_green', labelDefault: 'Vert', fill: '#e6f4ea', border: '#1e8e3e', borderWidth: 1 },
  { id: 'red', labelKey: 'sheet_shape_style_red', labelDefault: 'Rouge', fill: '#fce8e6', border: '#d93025', borderWidth: 1 },
  { id: 'yellow', labelKey: 'sheet_shape_style_yellow', labelDefault: 'Jaune', fill: '#fef7e0', border: '#f9ab00', borderWidth: 1 },
  { id: 'ink', labelKey: 'sheet_shape_style_ink', labelDefault: 'Encre', fill: '#ffffff', border: '#202124', borderWidth: 2 },
] as const

// ── Per-user default style ────────────────────────────────────────────────────

/** Preference key inside `preferences.office`. */
const PREF_KEY = 'sheetShapeStyle'

/**
 * The office preference bag, narrowed to the key this module owns. The index
 * signature is what `useModulePrefs` needs, and writing through it never clobbers
 * the settings page's own keys (the helper merges the stored object first).
 */
interface ShapePrefs {
  [PREF_KEY]: DefaultShapeStyle
  [key: string]: unknown
}

const PREF_DEFAULTS: ShapePrefs = { [PREF_KEY]: DEFAULT_SHAPE_STYLE }

/** A colour token is either 'none' or a hex triplet; anything else is ignored. */
function isColour(v: unknown): v is string {
  return typeof v === 'string' && (v === 'none' || /^#[0-9a-fA-F]{6}$/.test(v))
}

/** Harden whatever came back from the backend into a usable style. */
export function normalizeShapeStyle(raw: unknown): DefaultShapeStyle {
  const o = (raw ?? {}) as Partial<DefaultShapeStyle>
  const width = typeof o.borderWidth === 'number' && Number.isFinite(o.borderWidth)
    ? Math.max(0, Math.min(24, o.borderWidth))
    : DEFAULT_SHAPE_STYLE.borderWidth
  return {
    fill: isColour(o.fill) ? o.fill : DEFAULT_SHAPE_STYLE.fill,
    border: isColour(o.border) ? o.border : DEFAULT_SHAPE_STYLE.border,
    borderWidth: width,
  }
}

/**
 * Read / write the user's default shape style.
 *
 * `setStyle` is what « Définir comme style de forme par défaut » calls with the
 * selected shape's current look; `newShape()` then applies it to every insertion.
 */
export function useDefaultShapeStyle(): {
  style: DefaultShapeStyle
  setStyle: (patch: Partial<DefaultShapeStyle>) => Promise<void>
  reset: () => Promise<void>
} {
  const { prefs, update } = useModulePrefs<ShapePrefs>('office', PREF_DEFAULTS)
  const style = normalizeShapeStyle(prefs[PREF_KEY])

  const setStyle = useCallback(
    (patch: Partial<DefaultShapeStyle>) =>
      update({ [PREF_KEY]: normalizeShapeStyle({ ...style, ...patch }) } as Partial<ShapePrefs>),
    [update, style.fill, style.border, style.borderWidth], // eslint-disable-line react-hooks/exhaustive-deps
  )

  const reset = useCallback(
    () => update({ [PREF_KEY]: DEFAULT_SHAPE_STYLE } as Partial<ShapePrefs>),
    [update],
  )

  return { style, setStyle, reset }
}

// ── Shape factory ─────────────────────────────────────────────────────────────

/**
 * Random id for a new shape. Copied rather than imported from the editor on
 * purpose (importing SpreadsheetApp here would close a cycle), and NOT
 * `crypto.randomUUID`, which is undefined over plain HTTP.
 */
function shapeUid(): string {
  return Math.random().toString(36).slice(2, 10) + Math.random().toString(36).slice(2, 6)
}

/** Box of a shape, in base (zoom = 1) px from the grid's data origin. */
export interface ShapeBox { bx: number; by: number; bw?: number; bh?: number }

/**
 * Build a shape ready to be pushed into `SheetData.shapes`. The host only has to
 * decide WHERE it goes (`box`); the look comes from the user's default style.
 */
export function newShape(
  kind: SheetShapeKind,
  box: ShapeBox,
  style: DefaultShapeStyle = DEFAULT_SHAPE_STYLE,
): SheetShape {
  const s = normalizeShapeStyle(style)
  return {
    id: shapeUid(),
    kind,
    bx: Math.round(box.bx),
    by: Math.round(box.by),
    bw: Math.max(8, Math.round(box.bw ?? DEFAULT_SHAPE_WIDTH)),
    bh: Math.max(8, Math.round(box.bh ?? DEFAULT_SHAPE_HEIGHT)),
    fill: s.fill,
    border: s.border,
    borderWidth: s.borderWidth,
  }
}
