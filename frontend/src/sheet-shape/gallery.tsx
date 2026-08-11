// The shapes INSERTION gallery — the panel behind the ribbon's Insertion ▸ Formes
// button, laid out like Excel's: small groups of thumbnails, one click inserts.
//
// Panel content only: the host owns the trigger button and the `AnchoredPopover`
// around it (exactly as `chartsGalleryRender` does for diagrams), so this file has
// no floating-layer logic of its own.
//
// Thumbnails are the real geometry drawn by `ShapeGlyph` at thumbnail size, in a
// NEUTRAL preview pair (white on grey, like the documents editor's gallery). They
// deliberately do NOT use the user's default style: a saved bright-yellow default
// made the whole panel unreadable, and a gallery is a shape catalogue, not a colour
// preview.

import type { TFunction } from 'i18next'
import type { SheetShapeKind } from '../api'
import { ShapeGlyph } from './ShapeView'
import { DEFAULT_SHAPE_STYLE, type DefaultShapeStyle } from './defaults'
import { SHAPE_CATALOG } from '../shapes/catalog'
import { ShapeGallery as SharedShapeGallery } from '../shapes/ShapeGallery'

/** Same neutral pair as a freshly inserted shape: white fill, black outline. */
const THUMB_STYLE: DefaultShapeStyle = { fill: '#ffffff', border: '#000000', borderWidth: 1 }

export interface ShapeGroup {
  id: string
  /** i18n key of the group heading; `labelDefault` is the French fallback. */
  labelKey: string
  labelDefault: string
  kinds: readonly SheetShapeKind[]
}

/**
 * Groups come from the catalogue SHARED with the documents editor: the sheet used
 * to offer ten shapes against its 144, for no reason other than history. The text
 * box entry is dropped (it belongs to the text editor, not to a grid).
 */
export const SHAPE_GROUPS: readonly ShapeGroup[] = SHAPE_CATALOG.map(c => ({
  id: c.id,
  labelKey: `sheet_shape_grp_${c.id}`,
  labelDefault: c.title,
  kinds: c.shapes.filter(x => x.kind !== 'textBox').map(x => x.kind) as SheetShapeKind[],
})).filter(g => g.kinds.length > 0)

/**
 * Name of one geometry. The keys that already exist in the module's bundle (the
 * presentation editor's shape names, same 13 languages, same words) are reused
 * rather than duplicated; only `line` needs a sheet-specific key.
 */
const KIND_LABELS: Partial<Record<SheetShapeKind, { key: string; def: string }>> = {
  rect: { key: 'pres_shape_rect', def: 'Rectangle' },
  roundRect: { key: 'pres_shape_round', def: 'Rectangle arrondi' },
  ellipse: { key: 'pres_shape_ellipse', def: 'Ellipse' },
  triangle: { key: 'pres_shape_triangle', def: 'Triangle' },
  diamond: { key: 'pres_shape_diamond', def: 'Losange' },
  arrow: { key: 'pres_shape_arrow', def: 'Flèche' },
  line: { key: 'sheet_shape_line', def: 'Trait' },
  star: { key: 'pres_shape_star', def: 'Étoile' },
  callout: { key: 'pres_shape_speech', def: 'Bulle' },
  plus: { key: 'pres_shape_plus', def: 'Croix' },
}

export function shapeKindLabel(kind: SheetShapeKind, t: TFunction): string {
  const l = KIND_LABELS[kind]
  if (l) return t(l.key, { defaultValue: l.def })
  // Catalogue shapes carry their French label with them; no key to invent.
  return SHAPE_CATALOG.flatMap(c => c.shapes).find(x => x.kind === kind)?.label ?? kind
}

export interface ShapeThumbProps {
  kind: SheetShapeKind
  width?: number
  height?: number
  style?: DefaultShapeStyle
}

/** One geometry at thumbnail size — also reusable in a ribbon split button. */
export function ShapeThumb({ kind, width = 34, height = 26, style = THUMB_STYLE }: ShapeThumbProps) {
  return (
    <ShapeGlyph
      kind={kind}
      width={width}
      height={height}
      fill={style.fill}
      border={style.border}
      borderWidth={Math.min(style.borderWidth, 1.5)}
    />
  )
}

export interface ShapeGalleryProps {
  /** Insert that geometry; the host decides where and closes the popover. */
  onPick: (kind: SheetShapeKind) => void
  t: TFunction
  /** The user's default style, so the previews match the result. */
  style?: DefaultShapeStyle
}

// La galerie du tableur EST la galerie PARTAGÉE du module office (présentation
// façon LibreOffice : récemment utilisées + sections + contours gris). Elle est
// juste ré-exportée ici pour ne pas toucher le point d'appel du tableur ; `style`
// reste accepté pour compat mais ignoré (le panneau est un catalogue de formes,
// pas un aperçu de la couleur choisie). `ShapeThumb`/`shapeKindLabel` ci-dessus
// restent définis : le menu contextuel des formes (shapeMenu.tsx) s'en sert.
export function ShapeGallery({ onPick, t }: ShapeGalleryProps) {
  return <SharedShapeGallery onPick={onPick as (k: SheetShapeKind) => void} t={t} />
}
