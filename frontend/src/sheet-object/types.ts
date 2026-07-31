// Shared vocabulary for the spreadsheet's FLOATING OBJECTS — charts, pictures and
// shapes. Nothing here knows about any one of them: the editor builds an
// `ObjActions` / `ObjBinding` per selected object, and the generic menus, mini
// toolbar and dialogs of this folder work off those two interfaces alone.
//
// Geometry convention (identical for the three types, see sheet-chart/interaction.ts):
// a box in BASE pixels (zoom = 1) measured from the grid's data origin, plus a
// rotation in degrees.

/**
 * The kinds of floating object the sheet can hold. Equations (KaTeX overlays) are
 * objects like the rest: same selection chrome, same group commands, same rule
 * that a click on them never reaches the cell underneath.
 */
export type ObjKind = 'chart' | 'image' | 'shape' | 'equation'

/**
 * Identity of one floating object.
 *
 * Charts and shapes carry their own `id`. Pictures do NOT (`SheetImage` has no id
 * field): they are addressed by their position in `SheetData.images`, so their ref
 * id is the stringified index — always build it with `imageRef()` and read it back
 * with `refIndex()` rather than converting by hand.
 */
export interface ObjRef { kind: ObjKind; id: string }

/** Ref of the picture at `index` in the sheet's picture array. */
export function imageRef(index: number): ObjRef { return { kind: 'image', id: String(index) } }
/** Array index behind a picture ref (-1 when the ref is not a picture / not numeric). */
export function refIndex(ref: ObjRef): number {
  if (ref.kind !== 'image') return -1
  const n = Number(ref.id)
  return Number.isInteger(n) && n >= 0 ? n : -1
}
/** Structural equality — refs are plain data, never compared by identity. */
export function sameRef(a: ObjRef | null, b: ObjRef | null): boolean {
  return !!a && !!b && a.kind === b.kind && a.id === b.id
}

/** Fill / outline of an object, as the mini toolbar's swatches read and write it. */
export interface ObjStyle {
  /** "#RRGGBB" or 'none'. */
  fill?: string
  /** Outline colour, "#RRGGBB" or 'none'. */
  border?: string
  /** Outline width in px. */
  borderWidth?: number
}

/** Where in its own type's paint order an object should move. */
export type ObjOrderOp = 'front' | 'forward' | 'backward' | 'back'

/**
 * Everything a generic object dialog needs: the current state of one object, and
 * one way to write part of it back (the host persists).
 */
export interface ObjBinding {
  ref: ObjRef
  /** Box in base px from the data origin + rotation in degrees. */
  bx: number; by: number; bw: number; bh: number; rot: number
  style: ObjStyle
  altText: string
  /** Hyperlink target; '' when the object has none (or cannot have one). */
  link: string
  /** True when this kind supports a hyperlink at all (charts do not). */
  linkable: boolean
  /** Commit a partial update and persist it. */
  apply: (patch: ObjPatch) => void
}

/** The subset of an object every shared dialog / mini toolbar is allowed to write. */
export interface ObjPatch {
  bx?: number; by?: number; bw?: number; bh?: number; rot?: number
  fill?: string
  border?: string
  borderWidth?: number
  altText?: string
  link?: string
}

/**
 * The generic operations the three context menus are built from. Everything
 * type-specific (edit the chart, crop the picture, edit the shape's text…) stays
 * out of here and lives in the per-kind menu builder.
 *
 * NOTE on `order`: paint order is per TYPE, not global — pictures are painted on
 * the grid canvas, charts and shapes are DOM overlays in their own z-index bands.
 * "Bring to front" therefore reorders the object inside ITS OWN array only, and
 * can never lift a picture above a chart. Documented behaviour, not a bug.
 */
export interface ObjActions {
  ref: ObjRef
  /** Human label of the object ("Graphique 2", "Image", "Forme") for dialog titles. */
  label: string
  cut: () => void
  copy: () => void
  /** Paste whatever the object clipboard holds — may be of another kind. */
  paste: () => void
  /** True when the object clipboard is non-empty (drives the Paste button's state). */
  canPaste: boolean
  duplicate?: () => void
  order: (op: ObjOrderOp) => void
  /** True when the object can still move that way inside its own array. */
  canOrder: (op: ObjOrderOp) => boolean
  remove: () => void
  /** "Format de la zone de graphique… / de l'objet… / de la forme…" (per-type pane). */
  openFormat: () => void
  /** "Afficher le texte de remplacement…" (shared dialog). */
  openAltText: () => void
  /** "Taille et propriétés…" (shared dialog). */
  openSizeProps: () => void
  /** "Lien" — absent for charts, which Excel does not let you hyperlink. */
  openLink?: () => void
  /** "Ouvrir le lien" — absent, or disabled, when the object carries no link. */
  followLink?: () => void
  hasLink: boolean
  /** Current fill/outline, for the mini toolbar's swatches. */
  style: ObjStyle
  /** Write fill/outline back. */
  setStyle: (patch: ObjStyle) => void
}
