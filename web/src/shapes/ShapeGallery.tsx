// THE shape picker of the office module — the same panel in every sub-editor.
//
// Presentation transcribed from LibreOffice's / Word's shape gallery, so a user
// finds the same layout everywhere (spreadsheet, slides, board, documents):
//   • a « Formes récemment utilisées » section on top, filled as shapes are
//     picked (shared across the whole office suite — see recent.ts);
//   • one section per catalogue category, with a bold heading;
//   • thumbnails that are the REAL geometry, drawn as a thin GREY OUTLINE on a
//     white fill (no colour): a gallery is a catalogue of shapes, not a preview
//     of the user's fill, and the outline reads at thumbnail size.
//
// Groups, labels and geometry all come from the shared catalogue / renderer.

import { useState, useSyncExternalStore } from 'react'
import type { TFunction } from 'i18next'
import { SHAPE_CATALOG, type ShapeKind } from './catalog'
import { ShapeGlyph } from './ShapeGlyph'
import { loadRecentShapes, pushRecentShape } from './recent'
import {
  registeredShapes, onRegisteredShapesChange, registeredShapesVersion,
  type ShapeProvider,
} from './registry'

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

/** Every geometry the gallery can draw — used to filter the recents list. */
const KNOWN_KINDS = new Set<string>(SHAPE_GALLERY_GROUPS.flatMap(g => g.kinds))

/** Neutral thumbnail pair (grey outline on white), like LibreOffice / Word. */
const THUMB_STROKE = '#5f6470'
const THUMB_FILL = '#ffffff'

/** A gallery section built from the geometries modules contributed. */
interface ProvidedGroup {
  id: string
  label: string
  providers: ShapeProvider[]
}

/**
 * Group registered geometries into gallery sections. Providers that name no
 * category land together at the end, so a module contributing a single shape does
 * not have to invent a heading for it.
 */
function groupProvidedShapes(defs: readonly ShapeProvider[]): ProvidedGroup[] {
  const byId = new Map<string, ProvidedGroup>()
  for (const d of defs) {
    const id = d.category ?? '__extra'
    let g = byId.get(id)
    if (!g) {
      g = { id, label: d.categoryLabel ?? (id === '__extra' ? 'Autres formes' : id), providers: [] }
      byId.set(id, g)
    }
    // A later provider may be the one that names the section.
    if (d.categoryLabel) g.label = d.categoryLabel
    g.providers.push(d)
  }
  return [...byId.values()]
}

/** Human name of a geometry — the catalogue carries the French label. */
export function shapeLabel(kind: string, t?: TFunction): string {
  const provided = registeredShapes().find(p => p.kind === kind)
  if (provided) return provided.label
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

/** One clickable thumbnail: the real geometry as a grey outline. */
function Thumb({ kind, current, t, onPick }: {
  kind: ShapeKind
  current?: string
  t?: TFunction
  onPick: (kind: ShapeKind) => void
}) {
  const label = shapeLabel(kind, t)
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      // preventDefault: the ribbon buttons keep the editor's focus/selection so a
      // shape dropped into a slide or a sheet lands on the right target.
      onMouseDown={e => e.preventDefault()}
      onClick={() => onPick(kind)}
      className={`w-7 h-7 flex items-center justify-center rounded hover:bg-surface-2 active:bg-surface-3 ${
        kind === current ? 'bg-primary-light' : ''
      }`}
    >
      <ShapeGlyph kind={kind} width={22} height={19} fill={THUMB_FILL} stroke={THUMB_STROKE} strokeWidth={1} />
    </button>
  )
}

export interface ShapeGalleryProps {
  /** Pick a geometry; the host inserts it and closes its own popover. */
  onPick: (kind: ShapeKind) => void
  t?: TFunction
  /** Currently armed geometry, highlighted in the panel. */
  current?: string
  /** Panel width in px (default 268 → 8 thumbnails per row). */
  width?: number
  /** Panel max height in px (default: 64% of the viewport). */
  maxHeight?: number | string
}

export function ShapeGallery({ onPick, t, current, width = 268, maxHeight = '64vh' }: ShapeGalleryProps) {
  // Snapshot at mount: the panel closes on pick in every host, so it re-reads the
  // (freshly pushed) recents the next time it opens.
  const [recent] = useState<string[]>(() => loadRecentShapes().filter(k => KNOWN_KINDS.has(k)))
  // Geometries contributed by modules (shapes/registry). Subscribed rather than read
  // once: a module may register after this panel first mounted.
  useSyncExternalStore(onRegisteredShapesChange, registeredShapesVersion)
  const extraGroups = groupProvidedShapes(registeredShapes())

  const pick = (kind: ShapeKind) => {
    pushRecentShape(kind)
    onPick(kind)
  }

  return (
    <div
      // stopPropagation du mousedown : les popovers hôtes (AnchoredPopover, menus
      // du ruban, portails) se ferment sur un mousedown « extérieur » ; sans cela,
      // cliquer une vignette fermait la galerie AVANT que son onClick ne s'exécute,
      // donc rien n'était inséré/armé. Les boutons agissent au click, pas au mousedown.
      onMouseDown={e => e.stopPropagation()}
      className="bg-surface-1 border border-border rounded-xl shadow-2xl p-2 select-none"
      style={{ width, maxHeight, overflowY: 'auto' }}
    >
      {recent.length > 0 && (
        <div className="mb-1.5">
          <div className="text-[11px] font-semibold text-text-secondary px-1 pb-1">
            {t ? t('sheet_shape_grp_recent', { defaultValue: 'Formes récemment utilisées' }) : 'Formes récemment utilisées'}
          </div>
          <div className="flex flex-wrap gap-0.5">
            {recent.map((kind, i) => (
              <Thumb key={`r-${i}-${kind}`} kind={kind as ShapeKind} current={current} t={t} onPick={pick} />
            ))}
          </div>
        </div>
      )}

      {SHAPE_GALLERY_GROUPS.map(g => (
        <div key={g.id} className="mb-1.5 last:mb-0">
          <div className="text-[11px] font-semibold text-text-secondary px-1 pt-0.5 pb-1">
            {t ? t(g.labelKey, { defaultValue: g.labelDefault }) : g.labelDefault}
          </div>
          <div className="flex flex-wrap gap-0.5">
            {g.kinds.map(kind => (
              <Thumb key={kind} kind={kind} current={current} t={t} onPick={pick} />
            ))}
          </div>
        </div>
      ))}

      {/* Geometries contributed by modules — LAST, after the catalogue, so the
          shapes every editor shares stay where users learnt to find them. */}
      {extraGroups.map(g => (
        <div key={`x-${g.id}`} className="mb-1.5 last:mb-0">
          <div className="text-[11px] font-semibold text-text-secondary px-1 pt-0.5 pb-1">
            {g.label}
          </div>
          <div className="flex flex-wrap gap-0.5">
            {g.providers.map(p => (
              <Thumb key={p.kind} kind={p.kind as ShapeKind} current={current} t={t} onPick={pick} />
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
