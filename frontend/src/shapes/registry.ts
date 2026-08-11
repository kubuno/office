// EXTENSION POINT of the shapes package — how a module or sub-editor adds its OWN
// geometries to the shared machinery.
//
// The catalogue's 144 LibreOffice presets are a fixed set, and the handful of native
// geometries are the suite's own. Neither is meant to grow with domain shapes: an
// electrical symbol, a UML box, a company logo have no business in an OOXML preset
// table. So instead of forcing a module to fork the renderer — which is exactly how
// a sub-editor ends up with shapes that cannot be exported, adjusted or picked from
// the gallery — it REGISTERS them here, and gets the whole pipeline for free:
//
//   • painted by `paintShapeView` (canvas) and `shapeSvgView` (markup);
//   • adjustable, if it declares `adjustments` (the yellow knobs);
//   • drawable with the shared draw-to-create gesture (default size honoured);
//   • pickable from `ShapeGallery`, in its own section.
//
// A provider describes its geometry as SVG path data in the shape's own box, which
// is the ONE representation every surface can consume (Path2D for a canvas, `<path>`
// for markup) — so a module never writes rendering code twice, and never writes it
// for a surface it did not think of.
//
// Registration is a runtime act (a module registers when it loads and unregisters
// when it unloads), so nothing here is baked into the catalogue and a module that is
// not installed simply contributes nothing — the dynamic-discovery rule of the
// platform, applied to shapes.

import type { AdjustSpec } from './adjust'
import type { ShapeRect } from './native-geometry'

/** One sub-path of a registered geometry — same shape as the preset engine's. */
export interface ProvidedSubPath {
  /** SVG path data in the shape's own [0,0]→[w,h] box. */
  d: string
  /** Fill it with the caller's fill (default true). */
  fill?: boolean
  /** Stroke it with the caller's outline (default true). */
  stroke?: boolean
  /**
   * Luminance factor applied to the fill of THIS sub-path (the shaded faces of a
   * cube, the top of a cylinder). 1 = untouched.
   */
  shade?: number
}

export interface ProvidedGeometry {
  /** Sub-paths, painted in order. */
  paths: ProvidedSubPath[]
  /** Where a caption belongs inside the shape. Defaults to the whole box. */
  text?: ShapeRect
}

export interface ShapeProvider {
  /**
   * Unique geometry id, stored in documents. NAMESPACE IT (`mymodule.gear`): a bare
   * name risks colliding with a catalogue kind, and a registered kind WINS over the
   * catalogue — which is a useful escape hatch, but a terrible accident.
   */
  kind: string
  /** Human label, shown in the gallery and as the thumbnail's tooltip. */
  label: string
  /** Gallery section id. A new id creates a new section; omitted = a shared one. */
  category?: string
  /** Human label of that section. */
  categoryLabel?: string
  /** Size a plain click produces, in px. Defaults to the catalogue's generic size. */
  defaultSize?: { w: number; h: number }
  /**
   * Yellow adjustment knobs. Values are FRACTIONS (0..1), like the suite's native
   * geometries — NOT the OOXML 1/100000ths of the preset engine. `geometry()`
   * receives exactly what the knobs produce.
   */
  adjustments?: AdjustSpec[]
  /** The outline, for a box of this size and these adjustment values. */
  geometry(w: number, h: number, adj?: number[]): ProvidedGeometry
}

const providers = new Map<string, ShapeProvider>()
const listeners = new Set<() => void>()
let version = 0

function notify(): void {
  version++
  for (const l of listeners) l()
}

/**
 * Add geometries to the shared shapes machinery. Returns the function that removes
 * them again — call it when the module unloads, so a disabled module leaves no
 * unpaintable kinds behind in the gallery.
 *
 * Registering an id that already exists REPLACES it (last registration wins), which
 * is how a sub-editor can deliberately specialise a shape for its own surface.
 */
export function registerShapes(defs: readonly ShapeProvider[]): () => void {
  const added: string[] = []
  for (const d of defs) {
    providers.set(d.kind, d)
    added.push(d.kind)
  }
  if (added.length) notify()
  return () => {
    let removed = false
    for (const k of added) {
      // Only drop it if it is still OURS: a later registration of the same id
      // belongs to whoever registered it, and must survive our unload.
      if (providers.get(k) && defs.some(d => d.kind === k && providers.get(k) === d)) {
        providers.delete(k)
        removed = true
      }
    }
    if (removed) notify()
  }
}

/** The provider for a kind, or null when nobody registered it. */
export function getShapeProvider(kind: string): ShapeProvider | null {
  return providers.get(kind) ?? null
}

/** True when a module contributed this geometry. */
export function isRegisteredShape(kind: string): boolean {
  return providers.has(kind)
}

/** Every registered geometry, in registration order. */
export function registeredShapes(): ShapeProvider[] {
  return [...providers.values()]
}

/**
 * Subscribe to registrations — the gallery re-renders through this, so a module that
 * registers after the panel was first opened still shows up. Returns the unsubscribe.
 */
export function onRegisteredShapesChange(cb: () => void): () => void {
  listeners.add(cb)
  return () => { listeners.delete(cb) }
}

/**
 * Snapshot for `useSyncExternalStore`: bumped whenever the set of providers changes,
 * stable otherwise — so a subscribed component re-renders exactly once per change.
 */
export function registeredShapesVersion(): number {
  return version
}
