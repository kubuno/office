// THE shape picker of the office module — the same panel in every sub-editor.
//
// Groups and labels come from the shared catalogue, thumbnails are the REAL
// geometry (drawn by `ShapeGlyph`), so what the user picks is what they get,
// whether they are in a slide, a sheet, a document, a board or a diagram.
//
// Thumbnails deliberately use a neutral white-on-black pair rather than the
// user's default style: a gallery is a catalogue of shapes, and a saved bright
// default once made the whole panel unreadable.

import type { TFunction } from 'i18next'
import { SHAPE_CATALOG, type ShapeKind } from './catalog'
import { ShapeGlyph } from './ShapeGlyph'

export interface ShapeGalleryGroup {
  id: string
  /** i18n key of the heading; `labelDefault` is the French fallback. */
  labelKey: string
  labelDefault: string
  kinds: readonly ShapeKind[]
}

/** Catalogue groups, minus the text box (it belongs to the text editors). */
export const SHAPE_GALLERY_GROUPS: readonly ShapeGalleryGroup[] = SHAPE_CATALOG.map(c => ({
  id: c.id,
  labelKey: `sheet_shape_grp_${c.id}`,
  labelDefault: c.title,
  kinds: c.shapes.filter(s => s.kind !== 'textBox').map(s => s.kind) as ShapeKind[],
})).filter(g => g.kinds.length > 0)

/** Human name of a geometry — the catalogue carries the French label. */
export function shapeLabel(kind: string, t?: TFunction): string {
  const known = SHAPE_CATALOG.flatMap(c => c.shapes).find(s => s.kind === kind)
  if (!known) return kind
  // A few names already exist in the bundle (the presentations wording); reuse
  // them so the same shape is not called two different things in two editors.
  const shared: Record<string, { key: string; def: string }> = {
    rect: { key: 'pres_shape_rect', def: 'Rectangle' },
    roundRect: { key: 'pres_shape_round', def: 'Rectangle arrondi' },
    ellipse: { key: 'pres_shape_ellipse', def: 'Ellipse' },
    triangle: { key: 'pres_shape_triangle', def: 'Triangle' },
    diamond: { key: 'pres_shape_diamond', def: 'Losange' },
    arrow: { key: 'pres_shape_arrow', def: 'Flèche' },
    star: { key: 'pres_shape_star', def: 'Étoile' },
  }
  const s = shared[kind]
  return s && t ? t(s.key, { defaultValue: s.def }) : known.label
}

export interface ShapeGalleryProps {
  /** Pick a geometry; the host inserts it and closes its own popover. */
  onPick: (kind: ShapeKind) => void
  t?: TFunction
  /** Currently armed geometry, highlighted in the panel. */
  current?: string
  /** Panel width in px (default 236). */
  width?: number
  /** Panel max height in px (default 420). */
  maxHeight?: number
}

export function ShapeGallery({ onPick, t, current, width = 236, maxHeight = 420 }: ShapeGalleryProps) {
  return (
    <div
      className="bg-surface-0 border border-border rounded-lg shadow-xl p-2"
      style={{ width, maxHeight, overflowY: 'auto' }}
    >
      {SHAPE_GALLERY_GROUPS.map(g => (
        <div key={g.id} className="mb-1.5 last:mb-0">
          <div className="px-1 pb-0.5 text-[10px] uppercase tracking-wide text-text-tertiary">
            {t ? t(g.labelKey, { defaultValue: g.labelDefault }) : g.labelDefault}
          </div>
          <div className="flex flex-wrap gap-1">
            {g.kinds.map(kind => (
              <button
                key={kind}
                type="button"
                title={shapeLabel(kind, t)}
                aria-label={shapeLabel(kind, t)}
                onClick={() => onPick(kind)}
                className={`p-1 rounded border hover:border-primary hover:bg-primary-light ${
                  kind === current ? 'border-primary bg-primary-light' : 'border-transparent'
                }`}
              >
                <ShapeGlyph kind={kind} width={26} height={20} fill="#ffffff" stroke="#000000" strokeWidth={1} />
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
