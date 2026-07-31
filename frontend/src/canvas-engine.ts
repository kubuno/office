/**
 * Canvas-based text layout and rendering engine.
 * All layout coordinates are in unscaled CSS px.
 * The caller applies zoom + DPR scaling when rendering.
 */

import type { JSONContent } from '@tiptap/react'
import { outlineLevelOfJson } from './documents/references/outline'
import { FIELD_NODE, fieldNodeText, readFieldAttrs } from './documents/fields'
// Suivi des modifications : le moteur ne connaît que les DEUX marques du contrat
// (`insertion` / `deletion`) et la couleur par auteur, partagée avec le volet de
// révision — aucune autre dépendance vers l'UI.
import { changeColor } from './documents/track-changes'

// ── Public types ──────────────────────────────────────────────────────────────

export interface TextMark {
  bold?:            boolean
  italic?:          boolean
  underline?:       boolean
  strike?:          boolean
  code?:            boolean
  fontSize?:        number   // pt
  fontFamily?:      string
  color?:           string   // CSS color
  backgroundColor?: string
  script?:          'sub' | 'super'   // indice / exposant
  caps?:            boolean  // petites majuscules (font-variant small-caps, façon Word)
  letterSpacing?:   number   // espacement des caractères (px ; + = étendu, − = condensé)
  // Tracked changes (Word « Suivi des modifications »). Two ProseMirror marks,
  // never nodes: `insertion` = text typed while tracking was on, `deletion` = text
  // removed while tracking was on — the text STAYS in the document, carrying the
  // mark, until the change is accepted (text goes) or rejected (mark goes).
  // Both are painted in their author's ink: insertions underlined, deletions
  // struck through (cf. setRevisionDisplay / setRevisionColorResolver).
  ins?:             RevisionInfo
  del?:             RevisionInfo
}

/// Attributes shared by the `insertion` and `deletion` marks. An UNATTRIBUTED
/// change is legal (empty author / authorId): never assume any field is filled.
export interface RevisionInfo {
  author:   string   // display name ("Jean Dupont"), may be empty
  authorId: string   // stable id, may be empty
  date:     string   // ISO-8601 UTC, may be empty
  id:       string   // change id (accept / reject), may be empty
  // Encre résolue, mémoïsée SUR L'OBJET (il vit dans le layout, donc réutilisé à
  // chaque frame) : la peinture évite ainsi une recherche de Map par span révisé et
  // par frame. `_inkGen` la périme si le résolveur de couleurs change.
  _ink?:    string
  _inkGen?: number
}

export interface LayoutSpan {
  text:   string
  marks:  TextMark
  x:      number   // CSS px from content-area left
  width:  number   // CSS px
  pmPos:  number   // ProseMirror position of span's first char
  // Image/forme « alignée sur le texte » (inline) : occupe 1 position PM (atom) et
  // s'affiche comme un caractère de la taille `w`×`h` sur la ligne (bas sur la ligne de base).
  img?:   { src: string; w: number; h: number; alt?: string; rot?: number }
  // Appel de note de bas de page (atom) : numéro auto + texte de la note.
  fn?:    { n: number; text: string }
  // Endnote reference mark (atom): auto lowercase-roman number + the note text.
  // Same mechanics as `fn`, except the notes block is laid out at the END of the
  // document (cf. appendEndnotePages), not at the bottom of every page.
  en?:    { n: number; text: string }
  // Champ Word (atom) : le texte peint est le RÉSULTAT du champ (attribut `cached`,
  // cf. documents/fields.ts) et hérite de la mise en forme du run — il est donc
  // mesuré et peint comme du texte normal. `field` ne sert qu'à la trame grise
  // (écran uniquement, cf. setFieldShading).
  field?: { kind: string; instr: string }
  // Positions ProseMirror couvertes par ce span quand ce n'est PAS `text.length` :
  // un ATOME (champ, appel de note, image inline) peint N caractères mais n'occupe
  // qu'UNE position. Absent = span de texte ordinaire (1 caractère = 1 position).
  pmLen?: number
}

export interface LayoutLine {
  spans:    LayoutSpan[]
  y:        number   // CSS px from content-area top (top of line)
  height:   number   // CSS px (interligne inclus)
  naturalH?: number  // CSS px : hauteur naturelle du texte (sans interligne) — centrage
  ascent:   number   // CSS px from line top to baseline
  baseline: number   // CSS px from content-area top to baseline (y + ascent)
  pmStart:  number
  pmEnd:    number
  image?:   { src: string; w: number; h: number; x: number; rotation: number; wrap?: string; wrapY?: number; alt?: string; tbFill?: string; tbStroke?: string; wrapSide?: string; wrapDistT?: number; wrapDistB?: number; wrapDistL?: number; wrapDistR?: number }   // ligne-image (dims, décalage gauche, rotation°, habillage, alt, couleurs zone-texte, côté/distances d'habillage)
  cellX?:   number   // bornes horizontales de la cellule (tableaux) pour coordsToPos
  cellW?:   number
  caretX?:  number   // x du caret pour une ligne VIDE (selon alignement/indentation)
  // Texte vertical de cellule (Word « Orientation du texte ») : les coords de la
  // ligne (y, spans[].x, baseline) restent en repère LOCAL non tourné ; le rendu et
  // le mappage de position appliquent rotate(±90°) puis translate(rtx,rty).
  rot?:     90 | 270  // 90 = haut→bas (horaire) ; 270 = bas→haut (anti-horaire)
  rtx?:     number    // translation écran appliquée après rotation
  rty?:     number
  cellY?:   number    // bornes verticales de la cellule (repère écran) pour le hit-test
  cellH?:   number
  // Habillage multi-segments : cette ligne partage sa rangée visuelle avec la ligne
  // précédente (texte des deux côtés d'un objet flottant) → même y, pas d'avance.
  sameRow?: boolean
  // Lettrine (drop cap) : grande initiale peinte en marge des 3 premières rangées.
  dropCap?: { text: string; marks: TextMark; x: number; ascent: number }
  // Objet flottant scindé au saut de page : fenêtre de peinture verticale (repère
  // local page) + marqueur de clone de continuation (cf. splitFloatingImagesAcrossPages).
  imgClipTop?:    number
  imgClipBottom?: number
  imgSplitClone?: 'down' | 'up'
  // Ligne RÉPLIQUÉE (en-tête de tableau répété en haut de page) : rendue mais
  // invisible pour le caret/la sélection/le clic (les positions PM dupliquées
  // renverraient vers l'original).
  phantom?: boolean
}

// Per-side cell border (Word "Borders" gallery). An explicit `null` side means
// "no border here" and BEATS the table-level default; an absent key means
// "inherit the table default".
export interface CellBorderSpec { w: number; s: 'solid' | 'dashed' | 'dotted'; c: string }
export interface CellBorders { t?: CellBorderSpec | null; b?: CellBorderSpec | null; l?: CellBorderSpec | null; r?: CellBorderSpec | null }
// Géométrie d'un tableau pour le tracé des bordures (coords zone de contenu).
export interface LayoutTableCell { x: number; y: number; w: number; h: number; bg?: string; r: number; c: number; colspan: number; rowspan: number; borders?: CellBorders }
// colX/rowY = positions des bordures (px, repère contenu) : colX[colCount+1], rowY[rows+1].
// Sert au placement des poignées de redimensionnement.
export interface LayoutTable { cells: LayoutTableCell[]; style?: string; accent?: string; colX?: number[]; rowY?: number[]; headerRepeat?: boolean; headerRows?: number; borderColor?: string; borderWidth?: number; borderStyle?: 'solid' | 'dashed' | 'dotted' }

export interface LayoutParagraph {
  lines:   LayoutLine[]
  y:       number   // CSS px from content-area top (top, inc. spaceBefore)
  height:  number   // CSS px total (inc. spaceBefore + spaceAfter)
  pmStart: number
  pmEnd:   number
  docIdx:  number   // index in doc.content[] (for splitting)
  secIdx:  number   // index de section (0 = section de base ; +1 par sectionBreak)
  breakBefore: boolean  // saut de page forcé avant ce paragraphe
  keepLines?: boolean   // lignes solidaires (paragraphe insécable entre pages)
  keepNext?:  boolean   // solidaire du paragraphe suivant
  table?:  LayoutTable  // géométrie des bordures si ce paragraphe est un tableau
  tocPage?:   number    // entrée de TOC : numéro de page + points de suite (paint)
  tocPageText?: string  // forme imprimée du numéro (index : « 5, 7 » ; « passim »)
  tocLeaderKind?: 'none' | 'dots' | 'dashes' | 'underline'
  tocLeader?: boolean
}

export interface DocumentLayout {
  paragraphs:  LayoutParagraph[]
  totalHeight: number   // CSS px
  contentW:    number   // largeur de la zone de contenu (px) — pour la sélection pleine ligne
}

export interface CursorMetrics {
  x:           number   // CSS px from content-area left
  y:           number   // CSS px from content-area top (top of the caret box)
  height:      number   // CSS px — same box as a selection rect at this position (line box)
  italicAngle: number   // radians — 0 for upright, ~+0.13 for italic (CW = leans right)
  rot?:        90 | 270 // caret d'une cellule à texte vertical (barre tournée ±90°)
  baseline?:   number   // CSS px from content-area top to the text baseline (anchor)
  lineTop?:    number   // CSS px — top of the FULL line box (leading included)
  lineH?:      number   // CSS px — height of the FULL line box (for ↑/↓ navigation)
}

export interface SelectionRect {
  x: number   // CSS px from content-area left
  y: number   // CSS px from content-area top
  w: number   // CSS px
  h: number   // CSS px
}

// ── Internal render types ─────────────────────────────────────────────────────

interface RenderSpan {
  text:   string
  marks:  TextMark
  pmPos:  number
  img?:   { src: string; w: number; h: number; alt?: string; rot?: number }  // image inline (atom)
  fn?:    { n: number; text: string }                                        // appel de note de bas de page (atom)
  en?:    { n: number; text: string }                                        // endnote reference mark (atom)
  field?: { kind: string; instr: string }                                    // champ Word (atom)
  pmLen?: number                                                             // positions PM couvertes (atomes)
}

interface RenderParagraph {
  spans:       RenderSpan[]
  align:       'left' | 'center' | 'right' | 'justify'
  indent:      number    // CSS px (left indent : listes + retrait gauche paragraphe)
  firstLineIndent?: number  // CSS px : offset de la 1ʳᵉ ligne vs `indent` (négatif = suspendu)
  indentRight?:     number  // CSS px : retrait droit (réduit la largeur disponible)
  tabStops?:        number[] // taquets de tabulation perso (px depuis la marge gauche)
  marker?:     string    // '•' or '1.' etc.
  markerMarks?: TextMark // mise en forme du marqueur (numérotation de titres : gras + taille)
  spaceBefore: number    // CSS px
  spaceAfter:  number    // CSS px
  pmStart:     number
  pmEnd:       number
  docIdx:      number    // index in doc.content[]
  secIdx:      number    // index de section (0 = base ; +1 par sectionBreak)
  breakBefore: boolean   // saut de page forcé avant ce paragraphe (nœud pageBreak)
  lineSpacing: number    // interligne (multiplicateur ; défaut 1.15)
  lineSpacingMode?: 'multiple' | 'atLeast' | 'exactly'  // mode d'interligne (Word)
  lineSpacingPt?: number    // CSS px pour 'atLeast'/'exactly' (interligne absolu)
  contextualSpacing?: boolean // « ne pas ajouter d'espace entre paragraphes du même style »
  styleKey?: string         // identité de style (type+niveau) pour l'espacement contextuel
  keepLines?: boolean       // lignes solidaires (paragraphe insécable entre pages)
  keepNext?: boolean        // solidaire du paragraphe suivant
  emptyPt?:    number     // taille (pt) portée par un paragraphe VIDE (attr fontMarks) → hauteur de ligne
  dropCap?:    boolean    // lettrine : grande initiale habillée par les 3 premières rangées
  tocPage?:    number     // entrée de table des matières : numéro de page (aligné à droite)
  tocPageText?: string    // forme IMPRIMÉE du numéro (index : « 5, 7 » ; « passim »)
  tocLeader?:  boolean    // points de suite entre le texte et le numéro
  tocLeaderKind?: 'none' | 'dots' | 'dashes' | 'underline'   // style du trait de suite
  image?:      { src: string; width: number; height: number; align: 'left' | 'center' | 'right'; rotation: number; wrap?: string; wrapX?: number; wrapY?: number; alt?: string; tbFill?: string; tbStroke?: string; wrapSide?: string; wrapDistT?: number; wrapDistB?: number; wrapDistL?: number; wrapDistR?: number }   // bloc-image (0 = taille naturelle) + habillage + alt + couleurs zone-texte + côté/distances
  table?:      RenderTable   // tableau (lignes/cellules)
}

// ── Structures de tableau (parse) ───────────────────────────────────────────
interface RenderTableCell { paras: RenderParagraph[]; colspan: number; rowspan: number; merged: boolean; cellBg?: string; vAlign?: 'top' | 'center' | 'bottom'; dir?: 0 | 90 | 270; borders?: CellBorders }
interface RenderTableRow  { cells: RenderTableCell[] }
interface RenderTable     { rows: RenderTableRow[]; colCount: number; style: string; accent?: string; colWidths?: number[]; rowHeights?: number[]; align?: 'left' | 'center' | 'right'; indent?: number; rowHeightModes?: Array<'atleast' | 'exactly'>; wrap?: 'none' | 'around'; headerRepeat?: boolean; headerRows?: number; layout?: 'autofit' | 'fixed';
                            cellMargin?: { t: number; b: number; l: number; r: number }; wrapDist?: { t: number; b: number; l: number; r: number }; cellSpacing?: number; borderColor?: string; borderWidth?: number; borderStyle?: 'solid' | 'dashed' | 'dotted' }

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_PT   = 11
const DEFAULT_FAM  = 'Arial'
// Noir PUR, comme Word. L'ancien gris-noir Google (#202124) rendait le texte par
// défaut sensiblement plus « gris » que Word à police/taille égales (luminance d'encre
// mesurée 117.8 vs 102.5 à l'écran pour Arial 11).
const DEFAULT_CLR  = '#000000'
const PT_PX        = 96 / 72      // 1 pt = 1.3333 CSS px at 96 dpi
const LH_RATIO     = 1.15         // line-height multiplier (Google Docs default)
const LIST_INDENT  = 32           // CSS px per nesting level

// ── Table des matières (entrées `tocPage` / `tocLeader`) ──────────────────────
// Écart entre la fin du texte de l'entrée et le numéro de page (px) — réservé à
// la mise en page (indentRight) ET respecté au rendu, donc jamais de collision.
const TOC_NUM_GAP  = 16
const TOC_DOT_STEP = 6      // pas de la grille des points de suite (px)
const TOC_DOT_SIZE = 1.6    // côté d'un point (px)
const TOC_DOT_PAD  = 8      // blanc laissé de part et d'autre des points (px)
// Trame grise d'un champ (façon Word) : marqueur d'ÉCRAN, jamais imprimé.
const FIELD_SHADE  = 'rgba(0,0,0,0.10)'

// ── Barres de modification (« changed lines » de Word) ────────────────────────
// Trait vertical dans la marge GAUCHE de la page, en face de chaque ligne qui
// porte au moins une révision. Repère principal du relecteur : il survole la
// marge, pas le texte. Word l'imprime aussi (ce n'est pas un marqueur d'écran).
const CHANGE_BAR_DX  = 16          // distance à gauche du bord de la zone de contenu (px)
const CHANGE_BAR_W   = 1           // épaisseur (px CSS ; au moins 1 px machine)
const CHANGE_BAR_CLR = '#3c4043'   // « Auto » de Word = encre sombre neutre, pas la couleur de l'auteur

const H_SIZE:   Record<number, number> = { 1: 24, 2: 18, 3: 14, 4: 13, 5: 12, 6: 11 }
const H_BEFORE: Record<number, number> = { 1: 20, 2: 16, 3: 12, 4: 10, 5:  8, 6:  8 }
const H_AFTER:  Record<number, number> = { 1:  6, 2:  4, 3:  4, 4:  4, 5:  4, 6:  4 }

// ── Singleton measurement canvas ──────────────────────────────────────────────

// Qualité de rendu du texte (façon traitement de texte) : crénage activé + légère
// optimisation de lisibilité (crénage/ligatures). Appliqué AUSSI au contexte de MESURE
// pour que les largeurs concordent avec le rendu (sinon chevauchement/décalage).
function applyTextQuality(ctx: CanvasRenderingContext2D): void {
  try {
    ctx.fontKerning = 'normal'
    ;(ctx as unknown as { textRendering?: string }).textRendering = 'optimizeLegibility'
    // Ré-échantillonnage bicubique des images tramées réduites (photos insérées) :
    // bords plus lisses qu'en interpolation par défaut. Sans effet sur les formes
    // vectorielles (re-rastérisées à la résolution réelle) ni le texte.
    ctx.imageSmoothingEnabled = true
    ctx.imageSmoothingQuality = 'high'
  } catch { /* propriétés non supportées : ignorer */ }
}

let _mc: CanvasRenderingContext2D | null = null
function mc(): CanvasRenderingContext2D {
  if (!_mc) { _mc = document.createElement('canvas').getContext('2d')!; applyTextQuality(_mc) }
  return _mc
}

// ── Résolveur de forme (kbshape sans src) ──────────────────────────────────────
// Les formes importées (ex. depuis DOCX) ne portent que l'`alt` `kbshape:…` et pas
// de `src` SVG : le générateur SVG (`shapeSvg`) vit côté UI. L'éditeur enregistre ici
// un résolveur (alt + dimensions → data-URL SVG) que la mise en page utilise pour
// régénérer le `src` manquant. Idempotent (même alt+dims → même URL → cache stable).
let _shapeSrcResolver: ((alt: string, w: number, h: number) => string | null) | null = null
export function setShapeSrcResolver(fn: ((alt: string, w: number, h: number) => string | null) | null): void {
  _shapeSrcResolver = fn
}
// Renvoie le `src` effectif d'un nœud image : régénéré depuis l'alt `kbshape:` s'il
// est absent, sinon le `src` stocké tel quel.
function resolveImageSrc(a: Record<string, unknown>): string {
  const src = String(a.src ?? '')
  if (src) return src
  const alt = a.alt != null ? String(a.alt) : ''
  if (alt.startsWith('kbshape:') && _shapeSrcResolver) {
    return _shapeSrcResolver(alt, Number(a.width) || 0, Number(a.height) || 0) ?? src
  }
  return src
}

// ── Trame grise des champs (Word « Champs avec trame ») ────────────────────────
// Word ne colore JAMAIS le texte d'un champ : il pose une trame grise derrière lui
// pour signaler « ceci est calculé, pas de la frappe ». Trois modes, comme Word
// (Options → Avancé → « Champs avec trame ») ; c'est un marqueur d'ÉCRAN, donc
// JAMAIS peint à l'impression / à l'export (passer `{ print: true }`, cf. PaintOpts).
export type FieldShading = 'never' | 'selected' | 'always'
let _fieldShading: FieldShading = 'selected'   // défaut de Word
// Plage PM du caret / de la sélection, pour le mode 'selected'. L'éditeur la pose à
// chaque changement de sélection (le caret REPLIÉ n'est pas transmis à renderDocument,
// qui ne reçoit que les sélections non vides) ; `null` = aucune trame en 'selected'.
let _fieldCaret: { from: number; to: number } | null = null

export function setFieldShading(mode: FieldShading): void { _fieldShading = mode }
export function getFieldShading(): FieldShading { return _fieldShading }
/// Position (ou plage) PM du curseur — le champ qui la contient prend la trame.
export function setFieldCaret(from: number | null, to?: number): void {
  _fieldCaret = from == null ? null : { from, to: to ?? from }
}

// ── Affichage des révisions (suivi des modifications) ─────────────────────────
// Deux modes, comme le sélecteur « Affichage pour révision » de Word :
//   • 'markup' — les marques sont VISIBLES : insertions soulignées et suppressions
//     barrées dans la couleur de leur auteur, barre de modification en marge ;
//   • 'final'  — le document TEL QU'IL SERA une fois tout accepté : les marques
//     sont ignorées (aucune couleur, aucun souligné, aucune barre) et le texte
//     supprimé est MASQUÉ — il est retiré à la MISE EN PAGE (cf. parseDoc), pas
//     seulement à la peinture, sinon il occuperait encore sa place.
// ── Titres repliables (Word « Développer/Réduire ») ──────────────────────────
// Fonction DÉSACTIVÉE par défaut chez nous : tant qu'elle ne l'est pas, l'attribut
// `collapsed` est ignoré (rien n'est masqué) et les triangles ne sont pas peints.
// Réglage global du moteur, comme le mode de révision ci-dessous.
let _collapsibleHeadings = false

export function getCollapsibleHeadings(): boolean { return _collapsibleHeadings }
/// Bascule la fonction et demande un RELAYOUT : replier retire des blocs du flux,
/// donc la pagination change (même canal que `setRevisionDisplay`).
export function setCollapsibleHeadings(on: boolean): void {
  if (on === _collapsibleHeadings) return
  _collapsibleHeadings = on
  _tbLayoutCache.clear()
  try {
    window.dispatchEvent(new Event('kubuno-image-loaded'))
  } catch { /* SSR */ }
}

// Réglage global du moteur, comme la trame des champs (cf. setFieldShading).
export type RevisionDisplay = 'markup' | 'final'
let _revisionDisplay: RevisionDisplay = 'markup'

export function getRevisionDisplay(): RevisionDisplay { return _revisionDisplay }
/// Change le mode et demande un RELAYOUT à l'éditeur : en 'final' les suppressions
/// disparaissent du flux, donc la pagination change. On réutilise le canal
/// 'kubuno-image-loaded' (déjà écouté par l'éditeur pour « re-mesurer + repeindre »)
/// et on émet en plus un événement dédié pour un câblage explicite.
export function setRevisionDisplay(mode: RevisionDisplay): void {
  if (mode === _revisionDisplay) return
  _revisionDisplay = mode
  _tbLayoutCache.clear()   // les zones de texte riches ont été mises en page dans l'autre mode
  try {
    window.dispatchEvent(new Event('kubuno-revisions-display'))
    window.dispatchEvent(new Event('kubuno-image-loaded'))
  } catch { /* SSR */ }
}

// Encre d'un auteur : SOURCE UNIQUE = `changeColor` de documents/track-changes.ts,
// la même que les pastilles du volet de révision. Deux palettes se seraient
// désynchronisées, et le relecteur associe justement la couleur de la marge à
// l'auteur listé dans le volet. `setRevisionColorResolver` reste le point d'entrée
// pour surcharger (thème, tests) — même mécanique que `setShapeSrcResolver`.
type RevisionColorFn = (authorId: string, author: string) => string
let _revisionColorFn: RevisionColorFn | null = null
let _revInkGen = 0   // périme les encres mémoïsées sur les objets RevisionInfo
export function setRevisionColorResolver(fn: RevisionColorFn | null): void {
  _revisionColorFn = fn
  _revInkGen++
}
function revisionColor(r: RevisionInfo): string {
  // Une modification NON ATTRIBUÉE (auteur et identifiant vides) est légale :
  // `changeColor` retombe alors sur la première teinte, jamais sur une erreur.
  if (_revisionColorFn) {
    try { return _revisionColorFn(r.authorId, r.author) || changeColor(r) } catch { /* repli */ }
  }
  return changeColor(r)
}
// Couleur d'un run révisé : la SUPPRESSION prime sur l'insertion (texte inséré par
// A puis supprimé par B → encre de B, barré ET souligné, comme Word).
function revisionInk(m: TextMark): string | null {
  const r = m.del ?? m.ins
  if (!r) return null
  if (r._ink !== undefined && r._inkGen === _revInkGen) return r._ink
  r._ink = revisionColor(r)
  r._inkGen = _revInkGen
  return r._ink
}
// Lecture DÉFENSIVE des attributs d'une marque de révision : tout champ peut
// manquer (import DOCX partiel, changement non attribué) — ne jamais planter.
function readRevisionAttrs(a: Record<string, unknown> | null | undefined): RevisionInfo {
  return {
    author:   a?.author   != null ? String(a.author)   : '',
    authorId: a?.authorId != null ? String(a.authorId) : '',
    date:     a?.date     != null ? String(a.date)     : '',
    id:       a?.id       != null ? String(a.id)       : '',
  }
}
// Ce run doit-il disparaître de la mise en page ? Uniquement le texte SUPPRIMÉ,
// et uniquement en mode « document final ».
function isHiddenRevision(m: TextMark): boolean {
  return _revisionDisplay === 'final' && !!m.del
}

// Modes d'habillage FLOTTANTS : l'objet ne réserve pas de hauteur dans le flux
// (posé à wrapX/wrapY, le texte coule autour/dessus/dessous). Source unique —
// utilisée par la mise en page, le rendu et les post-traitements de pagination.
const FLOATING_WRAPS = new Set(['square', 'tight', 'through', 'topBottom', 'behind', 'front'])
export function isFloatingWrap(w?: string): boolean { return !!w && FLOATING_WRAPS.has(w) }

// Boîte englobante d'une image tournée (rot en degrés) — réservée sur la ligne pour
// une image inline (largeur avance le x, hauteur impose la hauteur de ligne).
function imgAABB(w: number, h: number, rot = 0): { w: number; h: number } {
  if (!rot) return { w, h }
  const r = (rot * Math.PI) / 180
  return {
    w: Math.abs(w * Math.cos(r)) + Math.abs(h * Math.sin(r)),
    h: Math.abs(w * Math.sin(r)) + Math.abs(h * Math.cos(r)),
  }
}

// ── Cache d'images (chargement async) ──────────────────────────────────────────
const _imgCache = new Map<string, HTMLImageElement>()
export function getImage(src: string): HTMLImageElement | null {
  if (!src) return null
  let img = _imgCache.get(src)
  if (img) return img
  img = new Image()
  // NB : pas de crossOrigin — sinon le navigateur REFUSE de charger les images
  // externes sans en-têtes CORS (la majorité des URL), et rien ne s'affiche.
  // Le canvas devient « tainted » (export/lecture pixels bloqués) mais l'affichage
  // via drawImage fonctionne, ce qui est l'objectif.
  // Au chargement, prévenir l'éditeur pour relayouter (taille naturelle connue) + redessiner.
  img.onload  = () => { try { window.dispatchEvent(new Event('kubuno-image-loaded')) } catch { /* SSR */ } }
  img.onerror = () => { try { window.dispatchEvent(new Event('kubuno-image-loaded')) } catch { /* SSR */ } }
  img.src = src
  _imgCache.set(src, img)
  return img
}
function imgReady(img: HTMLImageElement | null): boolean {
  return !!img && img.complete && img.naturalWidth > 0
}

// ── Contour alpha (habillage « rapproché » / « au travers ») ───────────────────
// Pour chaque image, on échantillonne le canal alpha sur une grille N rangées ×
// W colonnes : chaque rangée liste ses plages OPAQUES (x normalisés 0..1). Le
// texte épouse alors le contour réel (tight = enveloppe [min,max] ; through =
// chaque plage séparément, le texte passe dans les trous transparents).
// Cache par `src` ; null = indisponible (image opaque partout, pas encore chargée
// non mise en cache, ou canvas « tainted » CORS) → repli sur la boîte rectangulaire.
interface ImgContour { n: number; rows: Array<Array<[number, number]>> }
const _contourCache = new Map<string, ImgContour | null>()
// `rot`/`dispW`/`dispH` : pour un objet TOURNÉ, le contour est échantillonné sur
// l'image pivotée dans sa boîte englobante (AABB) — sinon le texte habillerait la
// silhouette d'origine alors que l'objet n'occupe plus le même espace.
function getContour(src: string, rot = 0, dispW = 0, dispH = 0): ImgContour | null {
  const key = rot ? `${src}|${Math.round(rot)}|${Math.round(dispW)}x${Math.round(dispH)}` : src
  const hit = _contourCache.get(key)
  if (hit !== undefined) return hit
  const img = getImage(src)
  if (!imgReady(img)) return null   // pas encore chargée : PAS de cache (re-tentera au relayout)
  let res: ImgContour | null = null
  try {
    const N = 64, W = 96
    const cv = document.createElement('canvas')
    cv.width = W; cv.height = N
    const cx = cv.getContext('2d', { willReadFrequently: true })!
    if (rot && dispW > 0 && dispH > 0) {
      // Grille = AABB de l'objet tourné : échelle AABB→grille (appliquée en dernier),
      // puis rotation autour du centre, puis image centrée à ses dimensions d'affichage.
      const ab = imgAABB(dispW, dispH, rot)
      cx.scale(W / ab.w, N / ab.h)
      cx.translate(ab.w / 2, ab.h / 2)
      cx.rotate((rot * Math.PI) / 180)
      cx.drawImage(img!, -dispW / 2, -dispH / 2, dispW, dispH)
      cx.setTransform(1, 0, 0, 1, 0, 0)
    } else {
      cx.drawImage(img!, 0, 0, W, N)
    }
    const data = cx.getImageData(0, 0, W, N).data
    const rows: Array<Array<[number, number]>> = []
    let partial = false   // au moins un pixel transparent → contour utile
    for (let r = 0; r < N; r++) {
      const runs: Array<[number, number]> = []
      let start = -1
      for (let x = 0; x <= W; x++) {
        const opaque = x < W && data[(r * W + x) * 4 + 3] > 32
        if (opaque && start < 0) start = x
        if (!opaque && start >= 0) { runs.push([start / W, x / W]); start = -1 }
      }
      if (runs.length !== 1 || runs[0][0] > 0.01 || runs[0][1] < 0.99) partial = true
      rows.push(runs)
    }
    res = partial ? { n: N, rows } : null   // image pleinement opaque → boîte simple
  } catch { res = null }   // canvas tainted (image cross-origin) → boîte simple
  if (_contourCache.size > 300) _contourCache.clear()   // clés par (src, rotation) : borner
  _contourCache.set(key, res)
  return res
}

// ── Font helpers ──────────────────────────────────────────────────────────────

const SCRIPT_SCALE = 0.66   // taille relative de l'exposant/indice (comme Google)

function fontStr(m: TextMark): string {
  let px  = (m.fontSize ?? DEFAULT_PT) * PT_PX
  if (m.script) px *= SCRIPT_SCALE
  const wt  = m.bold   ? 'bold'   : 'normal'
  const st  = m.italic ? 'italic' : 'normal'
  // Petites majuscules : font-variant CSS2 accepté dans le raccourci `font` du canvas.
  const va  = m.caps ? 'small-caps ' : ''
  const fam = m.fontFamily ?? DEFAULT_FAM
  return `${st} ${va}${wt} ${px}px ${fam}, sans-serif`
}

interface LineMetrics { ascent: number; descent: number; height: number }

// ── Caches de mesure ──────────────────────────────────────────────────────────
// layoutDocument re-mesure tout le document à chaque frappe ; mots et polices se
// répètent énormément. On mémoïse les largeurs (clé = police+texte) et les
// métriques de ligne (clé = police) pour éviter des milliers d'appels measureText.
const _widthCache = new Map<string, number>()
const _lineMetricsCache = new Map<string, LineMetrics>()
const WIDTH_CACHE_MAX = 100_000

// Génération des mesures : incrémentée à chaque purge (police chargée → largeurs
// obsolètes). Les caches de RENDU dérivés d'une mesure (géométrie de table des
// matières, cf. tocPaint) la mémorisent pour se recalculer au bon moment.
let _measEpoch = 0

/// À appeler si les polices personnalisées changent (largeurs obsolètes).
export function clearMeasureCaches(): void {
  _widthCache.clear()
  _lineMetricsCache.clear()
  _tbLayoutCache.clear()
  _measEpoch++
}

// ── Zones de texte RICHES (canvas) ──────────────────────────────────────────────
// Une zone de texte est un nœud image dont l'`alt` porte `kbtextrich:<doc JSON>`.
// Son contenu est un document ProseMirror complet (mise en forme, listes, images…)
// peint À L'INTÉRIEUR du rectangle de la boîte (réutilise tout le moteur).
export const RICH_TB_PAD = 12   // marge intérieure de la boîte (CSS px) — synchro avec l'overlay d'édition
export function parseRichTextBox(alt: string | null | undefined): JSONContent | null {
  if (!alt || !alt.startsWith('kbtextrich:')) return null
  try { return JSON.parse(decodeURIComponent(alt.slice('kbtextrich:'.length))) as JSONContent } catch { return null }
}
// Mise en page du sous-document, mémoïsée par (alt, largeur intérieure arrondie).
const _tbLayoutCache = new Map<string, DocumentLayout>()
export function richTextBoxLayout(alt: string, innerW: number): DocumentLayout | null {
  const key = `${alt}|${Math.round(innerW)}`
  let l = _tbLayoutCache.get(key)
  if (!l) {
    const doc = parseRichTextBox(alt); if (!doc) return null
    l = layoutDocument(doc, Math.max(8, innerW))
    if (_tbLayoutCache.size > 200) _tbLayoutCache.clear()
    _tbLayoutCache.set(key, l)
  }
  return l
}

// Quand une police finit de charger (@font-face / FontFace), les largeurs mesurées
// avec la police de repli deviennent fausses → on purge le cache.
if (typeof document !== 'undefined' && (document as Document).fonts) {
  (document as Document).fonts.addEventListener?.('loadingdone', () => clearMeasureCaches())
}
// `document.fonts.add(faceDéjàRésolue)` (police uploadée chargée via FontFace API,
// cf. loadCustomFont) NE déclenche PAS `loadingdone` → la clé de cache `fontStr+texte`
// est identique police de repli/police réelle, donc le relayout ré-utilise les
// largeurs de repli et le texte se chevauche. On purge donc aussi sur cet événement.
// Listener enregistré au chargement du module → s'exécute AVANT le `recompute` du
// composant (qui re-mesure alors à partir d'un cache vide).
if (typeof window !== 'undefined') {
  window.addEventListener('kubuno-font-loaded', () => clearMeasureCaches())
}

function measureW(text: string, marks: TextMark): number {
  const font = fontStr(marks)
  const ls   = marks.letterSpacing || 0
  const key  = (ls ? font + '|' + ls : font) + ' ' + text
  const hit  = _widthCache.get(key)
  if (hit !== undefined) return hit
  const c = mc()
  c.font = font
  // Espacement des caractères (Word « Étendu/Condensé ») : letterSpacing du canvas
  // affecte measureText ET fillText → mesure/rendu cohérents. Remis à 0 après.
  if (ls) c.letterSpacing = `${ls}px`
  const w = c.measureText(text).width
  if (ls) c.letterSpacing = '0px'
  if (_widthCache.size >= WIDTH_CACHE_MAX) _widthCache.clear()
  _widthCache.set(key, w)
  return w
}

function lineMetrics(marks: TextMark): LineMetrics {
  const font = fontStr(marks)
  const hit  = _lineMetricsCache.get(font)
  if (hit) return hit
  const c = mc()
  c.font = font
  const m   = c.measureText('Hgpjy|')
  // Utiliser les métriques NATURELLES de la police (fontBoundingBox, qui incluent
  // le leading intégré) comme Google Docs — et non la boîte serrée des glyphes
  // (actualBoundingBox), qui donnait une hauteur de ligne/sélection/curseur trop
  // petite (16px vs 20px chez Google).
  const fs  = (marks.fontSize ?? DEFAULT_PT) * PT_PX
  const asc = m.fontBoundingBoxAscent  || m.actualBoundingBoxAscent  || fs * 0.92
  const dsc = m.fontBoundingBoxDescent || m.actualBoundingBoxDescent || fs * 0.28
  // Hauteur de ligne = max( hauteur "normale" (≈1,2 × taille, comme la line-height
  // CSS normal / Google Docs ~20px à 11pt), métriques RÉELLES de la police
  // (fontBoundingBox, pour les polices à grandes métriques type Bookerly) ) × interligne.
  // Le max() garantit que le texte ne déborde jamais la boîte (sinon il rase le bas),
  // tout en gardant ~20px pour les polices standard.
  // height = hauteur de ligne NATURELLE (sans interligne) ; l'interligne du
  // paragraphe (lineSpacing, défaut LH_RATIO=1.15) est appliqué dans layoutParagraph.
  const NATURAL_EM = 1.20
  const lm: LineMetrics = { ascent: asc, descent: dsc, height: Math.max(fs * NATURAL_EM, asc + dsc) }
  _lineMetricsCache.set(font, lm)
  return lm
}

// ── Parse ProseMirror JSON ────────────────────────────────────────────────────

function extractMarks(node: JSONContent): TextMark {
  const m: TextMark = {}
  for (const mark of node.marks ?? []) {
    switch (mark.type) {
      case 'bold':      m.bold = true; break
      case 'italic':    m.italic = true; break
      case 'underline':   m.underline = true; break
      case 'strike':      m.strike = true; break
      case 'subscript':   m.script = 'sub'; break
      case 'superscript': m.script = 'super'; break
      case 'code':      m.code = true; m.fontFamily = 'Courier New'; m.backgroundColor = '#f1f3f4'; break
      case 'highlight': m.backgroundColor = mark.attrs?.color ?? '#fff176'; break   // surlignage (couleur ou jaune par défaut)
      // Suivi des modifications : marques PORTÉES PAR LE RUN, exactement comme le
      // surlignage. Elles ne changent ni la police ni la mesure — seulement l'encre
      // et les traits (cf. paintLayout) — donc aucune incidence sur les caches de
      // largeur. Un run peut porter les DEUX (inséré puis supprimé).
      case 'insertion': m.ins = readRevisionAttrs(mark.attrs as Record<string, unknown> | undefined); break
      case 'deletion':  m.del = readRevisionAttrs(mark.attrs as Record<string, unknown> | undefined); break
      case 'textStyle':
        if (mark.attrs?.fontSize)   { const n = parseFloat(mark.attrs.fontSize);  if (!isNaN(n)) m.fontSize = n }
        if (mark.attrs?.color)      m.color      = mark.attrs.color
        if (mark.attrs?.fontFamily) m.fontFamily = mark.attrs.fontFamily
        if (mark.attrs?.smallCaps)  m.caps = true
        if (mark.attrs?.letterSpacing != null) {
          const ls = parseFloat(String(mark.attrs.letterSpacing))
          if (!isNaN(ls) && ls) m.letterSpacing = ls * PT_PX   // stocké en pt → px
        }
        break
    }
  }
  return m
}

function nodeSize(n: JSONContent): number {
  if (n.type === 'text') return (n.text ?? '').length
  if (n.type === 'hardBreak') return 1
  if (n.type === 'sectionBreak' || n.type === 'pageBreak' || n.type === 'image' || n.type === 'footnote' || n.type === 'endnote' || n.type === FIELD_NODE) return 1   // feuilles (atom) : taille PM = 1
  let sz = 2
  for (const c of n.content ?? []) sz += nodeSize(c)
  return sz
}

// Mise en forme du numéro de page d'une entrée de table des matières (style « TM »
// de Word : le corps du document, pas celle du titre). Source UNIQUE, partagée par
// la réservation de place (parseDoc) et la peinture (tocPaint) — sinon un numéro plus
// large que réservé viendrait chevaucher le texte de l'entrée.
function tocNumMarks(): TextMark { return {} }

// Lowercase roman numeral (i, ii, iii, iv…) — Word's default numbering for
// ENDNOTES, where footnotes use arabic digits. Shared by the reference mark in the
// text and by the notes block, so both always read the same.
function romanLower(n: number): string {
  if (!isFinite(n) || n < 1) return String(n)
  const units: Array<[number, string]> = [
    [1000, 'm'], [900, 'cm'], [500, 'd'], [400, 'cd'], [100, 'c'], [90, 'xc'],
    [50, 'l'], [40, 'xl'], [10, 'x'], [9, 'ix'], [5, 'v'], [4, 'iv'], [1, 'i'],
  ]
  let rest = Math.floor(n), out = ''
  for (const [v, s] of units) while (rest >= v) { out += s; rest -= v }
  return out
}

// Options de mise en page au niveau document (métadonnées hors modèle PM).
export interface ParseOpts { headingNumbers?: boolean }

function parseDoc(doc: JSONContent, opts?: ParseOpts): RenderParagraph[] {
  const result: RenderParagraph[] = []
  let pos = 0  // ProseMirror: pos 0 is before the doc's first child opening tag
  let secIdx = 0       // section courante : +1 à chaque sectionBreak rencontré
  let pendingBreak = false  // saut de page en attente (à reporter sur le prochain bloc)
  let fnCounter = 0    // numérotation AUTOMATIQUE des notes de bas de page (ordre du document)
  let enCounter = 0    // endnotes: SEPARATE counter (lowercase roman numbering)
  const hCounters = [0, 0, 0, 0, 0, 0]   // numérotation multiniveau des titres (1., 1.1, …)

  function block(node: JSONContent, depth: number, dIdx: number, listCtx?: { type: 'bullet'|'ordered'; idx: number }, target: RenderParagraph[] = result) {
    // sectionBreak / pageBreak = nœuds feuilles (atom), taille PM = 1, aucun span.
    // sectionBreak borne les sections (bloc suivant en secIdx+1) ET force une page.
    // pageBreak force seulement une nouvelle page (même section).
    if (node.type === 'sectionBreak') {
      secIdx++
      pendingBreak = true
      pos += 1
      return
    }
    if (node.type === 'pageBreak') {
      pendingBreak = true
      pos += 1
      return
    }
    if (node.type === 'image') {
      const a = (node.attrs ?? {}) as Record<string, unknown>
      target.push({
        spans: [], align: 'left', indent: 0, spaceBefore: 6, spaceAfter: 6,
        pmStart: pos, pmEnd: pos + 1, docIdx: dIdx, secIdx,
        breakBefore: pendingBreak, lineSpacing: LH_RATIO,
        image: { src: resolveImageSrc(a), width: Number(a.width) || 0, height: Number(a.height) || 0, align: (a.align as 'left'|'center'|'right') ?? 'left', rotation: Number(a.rotation) || 0, wrap: (a.wrap as string) || 'inline', wrapX: Number(a.wrapX) || 0, wrapY: Number(a.wrapY) || 0, alt: a.alt != null ? String(a.alt) : undefined, tbFill: a.tbFill != null ? String(a.tbFill) : undefined, tbStroke: a.tbStroke != null ? String(a.tbStroke) : undefined, wrapSide: a.wrapSide != null ? String(a.wrapSide) : undefined, wrapDistT: Number(a.wrapDistT) || 0, wrapDistB: Number(a.wrapDistB) || 0, wrapDistL: a.wrapDistL != null ? Number(a.wrapDistL) : 10, wrapDistR: a.wrapDistR != null ? Number(a.wrapDistR) : 10 },
      })
      pendingBreak = false
      pos += 1   // nœud feuille (atom)
      return
    }
    if (node.type === 'table') {
      // table open(1) | pour chaque row : open(1) | pour chaque cell : open(1) + contenu + close(1) | row close(1) | table close(1)
      const tStart = pos
      const brk = pendingBreak
      pendingBreak = false
      pos++  // table open
      const rows: RenderTableRow[] = []
      let colCount = 1
      for (const rowNode of node.content ?? []) {
        pos++  // row open
        const cells: RenderTableCell[] = []
        let colsInRow = 0
        for (const cellNode of rowNode.content ?? []) {
          pos++  // cell open
          const cellParas: RenderParagraph[] = []
          let ci = 0
          for (const child of cellNode.content ?? []) block(child, 0, ci++, undefined, cellParas)
          pos++  // cell close
          const ca = (cellNode.attrs ?? {}) as Record<string, unknown>
          const colspan = Math.max(1, Number(ca.colspan) || 1)
          cells.push({ paras: cellParas, colspan, rowspan: Math.max(1, Number(ca.rowspan) || 1), merged: !!ca.merged, cellBg: ca.cellBg != null ? String(ca.cellBg) : undefined, vAlign: (ca.cellVAlign as 'top' | 'center' | 'bottom') || 'top', dir: (Number(ca.cellDir) as 0 | 90 | 270) || 0, borders: (ca.cellBorders as CellBorders | null) || undefined })
          colsInRow += colspan
        }
        pos++  // row close
        colCount = Math.max(colCount, colsInRow)
        rows.push({ cells })
      }
      pos++  // table close
      const ta = (node.attrs ?? {}) as Record<string, unknown>
      target.push({
        spans: [], align: 'left', indent: 0, spaceBefore: 6, spaceAfter: 6,
        pmStart: tStart, pmEnd: pos, docIdx: dIdx, secIdx,
        breakBefore: brk, lineSpacing: LH_RATIO,
        table: { rows, colCount, style: String(ta.tableStyle || 'grid'), accent: ta.accent != null ? String(ta.accent) : undefined, colWidths: Array.isArray(ta.colWidths) ? (ta.colWidths as number[]) : undefined, rowHeights: Array.isArray(ta.rowHeights) ? (ta.rowHeights as number[]) : undefined, align: (ta.tableAlign as 'left' | 'center' | 'right') || 'left', indent: Number(ta.tableIndent) || 0, rowHeightModes: Array.isArray(ta.rowHeightModes) ? (ta.rowHeightModes as Array<'atleast' | 'exactly'>) : undefined, wrap: ta.tableWrap === 'around' ? 'around' : 'none', headerRepeat: !!ta.headerRepeat,
          layout: ta.tableLayout === 'fixed' ? 'fixed' : 'autofit',
          cellSpacing: Math.max(0, Number(ta.cellSpacing) || 0),
          cellMargin: {
            t: ta.cellMarginTop    != null ? Number(ta.cellMarginTop)    : CELL_PAD_Y,
            b: ta.cellMarginBottom != null ? Number(ta.cellMarginBottom) : CELL_PAD_Y,
            l: ta.cellMarginLeft   != null ? Number(ta.cellMarginLeft)   : CELL_PAD_X,
            r: ta.cellMarginRight  != null ? Number(ta.cellMarginRight)  : CELL_PAD_X,
          },
          wrapDist: {
            t: ta.wrapDistTop    != null ? Number(ta.wrapDistTop)    : 4,
            b: ta.wrapDistBottom != null ? Number(ta.wrapDistBottom) : 8,
            l: ta.wrapDistLeft   != null ? Number(ta.wrapDistLeft)   : 12,
            r: ta.wrapDistRight  != null ? Number(ta.wrapDistRight)  : 12,
          },
          headerRows: Math.max(0, Number(ta.headerRows) || (ta.headerRepeat ? 1 : 0)), borderColor: ta.tableBorderColor != null ? String(ta.tableBorderColor) : undefined, borderWidth: ta.tableBorderWidth != null ? Number(ta.tableBorderWidth) : undefined, borderStyle: (ta.tableBorderStyle as 'solid' | 'dashed' | 'dotted') || undefined },
      })
      return
    }
    const bStart = pos
    // Saut de page : nœud pageBreak en amont OU attribut « Saut de page avant »
    // du paragraphe (dialogue Paragraphe → Enchaînements).
    const breakBefore = pendingBreak || !!node.attrs?.pageBreakBefore
    pendingBreak = false
    const lineSpacing = (node.attrs?.lineHeight as number) || LH_RATIO
    pos++  // opening tag

    if (node.type === 'paragraph' || node.type === 'heading') {
      const level = node.type === 'heading' ? (node.attrs?.level as number ?? 1) : 0
      const align = (node.attrs?.textAlign as 'left'|'center'|'right'|'justify') ?? 'left'
      const spans: RenderSpan[] = []

      for (const inline of node.content ?? []) {
        if (inline.type === 'text') {
          const marks = extractMarks(inline)
          // Mode « document final » : le texte supprimé n'est pas peint DU TOUT —
          // on n'émet aucun span mais on avance `pos` de sa longueur, sinon toutes
          // les positions ProseMirror suivantes (caret, sélection, clic, notes)
          // seraient décalées.
          if (isHiddenRevision(marks)) { pos += (inline.text ?? '').length; continue }
          if (level > 0) {
            if (!marks.fontSize) marks.fontSize = H_SIZE[level] ?? DEFAULT_PT
            if (!marks.bold && level <= 4) marks.bold = true
          }
          spans.push({ text: inline.text ?? '', marks, pmPos: pos })
          pos += (inline.text ?? '').length
        } else if (inline.type === 'hardBreak') {
          pos++
        } else if (inline.type === 'inlineImage') {
          // Image/forme « alignée sur le texte » : token-image dans le flux (atom, 1 pos).
          const a = inline.attrs ?? {}
          const imgMarks = extractMarks(inline)
          // Image supprimée + mode « final » : atome masqué, mais il occupe
          // toujours UNE position PM (cf. le texte supprimé ci-dessus).
          if (isHiddenRevision(imgMarks)) { pos += 1; continue }
          const src = resolveImageSrc(a as Record<string, unknown>)
          const w = Math.max(1, Number(a.width) || 0)
          const h = Math.max(1, Number(a.height) || 0)
          spans.push({ text: '​', marks: imgMarks, pmPos: pos,
            img: { src, w, h, alt: a.alt != null ? String(a.alt) : undefined, rot: Number(a.rotation) || 0 } })
          pos += 1
        } else if (inline.type === 'footnote') {
          // Appel de note de bas de page (atom, 1 pos) : numéro auto en exposant ;
          // le TEXTE de la note voyage avec le span (rendu en bas de page).
          // `pmLen: 1` : au-delà de la note n° 9 le numéro fait 2 caractères alors que
          // l'atome n'occupe TOUJOURS qu'une position → sans ça, caret/sélection/clic
          // dérivaient de `text.length` et sortaient du nœud.
          // Appel révisé : la marque voyage sur l'ATOME. Un appel supprimé ne
          // consomme PAS de numéro en mode « final » (les notes suivantes se
          // renumérotent, comme Word).
          const fnRev = extractMarks(inline)
          if (isHiddenRevision(fnRev)) { pos += 1; continue }
          const n = ++fnCounter
          spans.push({
            text: String(n), marks: { script: 'super', color: '#1a73e8', ins: fnRev.ins, del: fnRev.del }, pmPos: pos, pmLen: 1,
            fn: { n, text: String(inline.attrs?.text ?? '') },
          })
          pos += 1
        } else if (inline.type === 'endnote') {
          // Endnote reference mark (atom, 1 PM position): auto number in LOWERCASE
          // ROMAN (Word's default for endnotes) as a superscript. The note TEXT
          // travels with the span, but the notes block is laid out AFTER the last
          // content of the document (cf. appendEndnotePages), not at the page bottom.
          // `pmLen: 1`: from note « ii » on, the mark is 2 characters wide while the
          // atom still covers exactly ONE position — without it caret/selection/click
          // would derive from `text.length` and drift outside the node.
          const enRev = extractMarks(inline)
          if (isHiddenRevision(enRev)) { pos += 1; continue }   // appel supprimé, mode « final » : pas de numéro consommé
          const n = ++enCounter
          spans.push({
            text: romanLower(n), marks: { script: 'super', color: '#1a73e8', ins: enRev.ins, del: enRev.del }, pmPos: pos, pmLen: 1,
            en: { n, text: String(inline.attrs?.text ?? '') },
          })
          pos += 1
        } else if (inline.type === FIELD_NODE) {
          // Champ Word (atom, 1 pos) : on peint son RÉSULTAT (attribut `cached`, à jour
          // grâce à `refreshFields`) avec la mise en forme du run — donc mesuré et peint
          // exactement comme du texte. Jamais l'instruction : `fieldNodeText` retombe sur
          // le libellé du type si le cache est vide, un champ n'est jamais invisible.
          const a = readFieldAttrs(inline.attrs as Record<string, unknown> | null | undefined)
          const marks = extractMarks(inline)
          if (isHiddenRevision(marks)) { pos += 1; continue }   // champ supprimé, mode « final »
          if (level > 0) {
            if (!marks.fontSize) marks.fontSize = H_SIZE[level] ?? DEFAULT_PT
            if (!marks.bold && level <= 4) marks.bold = true
          }
          spans.push({
            text: fieldNodeText(inline), marks, pmPos: pos, pmLen: 1,
            field: { kind: a.kind, instr: a.instr },
          })
          pos += 1
        } else {
          pos += nodeSize(inline)
        }
      }

      // Numérotation automatique des titres (Word « liste multiniveau ») : compteur
      // par niveau, remise à zéro des niveaux plus profonds ; rendu en MARQUEUR
      // suspendu (le texte du titre est décalé de la largeur du numéro).
      let headMarker: string | undefined
      let headMarkerMarks: TextMark | undefined
      let headMarkerW = 0
      if (level > 0 && opts?.headingNumbers && !node.attrs?.tocTitle) {
        hCounters[level - 1]++
        for (let k = level; k < 6; k++) hCounters[k] = 0
        headMarker = hCounters.slice(0, level).map(c => Math.max(1, c)).join('.') + (level === 1 ? '.' : '')
        headMarkerMarks = { bold: true, fontSize: H_SIZE[level] ?? DEFAULT_PT }
        headMarkerW = measureW(headMarker + ' ', headMarkerMarks)
      }

      const indentLevel = (node.attrs?.indent as number) || 0
      // Espacement avant/après explicite (px) posé par l'UI « Espacement de
      // paragraphe » ; sinon défauts selon le type de bloc.
      const sbAttr = node.attrs?.spaceBefore as number | null | undefined
      const saAttr = node.attrs?.spaceAfter  as number | null | undefined
      // Retraits de paragraphe (px, façon Word) : gauche / 1ʳᵉ ligne / droite.
      const indL = (node.attrs?.indentLeft as number | null | undefined) || 0
      const indF = (node.attrs?.indentFirstLine as number | null | undefined) || 0
      const indR = (node.attrs?.indentRight as number | null | undefined) || 0
      // Entrée de table des matières : numéro de page à droite (+ points de suite).
      const tocPage = node.attrs?.tocPage as number | null | undefined
      const tocLeader = !!node.attrs?.tocLeader
      // An index entry prints a LIST of pages (« 5, 7 ») or « passim », so the
      // text form wins over the numeric one when both are present.
      const tocPageText = (node.attrs?.tocPageText as string | null | undefined) || undefined
      const tocLeaderKind = (node.attrs?.tocLeaderKind as string | null | undefined) || undefined
      // Largeur réservée à droite = largeur RÉELLE du numéro (donc « 128 » réserve plus
      // que « 9 ») + l'écart, jamais une valeur fixe. Mesurée avec les MÊMES marques
      // que le rendu (`tocNumMarks`) pour que réservation et peinture concordent.
      const tocNumStr = tocPageText ?? (tocPage != null ? String(tocPage) : undefined)
      const tocNumW = tocNumStr != null ? measureW(tocNumStr, tocNumMarks()) + TOC_NUM_GAP : 0
      const tabsRaw = node.attrs?.tabStops as Array<{ pos: number } | number> | null | undefined
      const tabsAttr = Array.isArray(tabsRaw) ? tabsRaw.map(t => typeof t === 'number' ? t : t?.pos).filter((n): n is number => typeof n === 'number') : undefined
      // Taille portée par un paragraphe VIDE (attr fontMarks.fs, ex. "16pt").
      const fm = node.attrs?.fontMarks as { fs?: string } | null | undefined
      const emptyPt = fm?.fs ? parseFloat(fm.fs) : undefined
      // Interligne typé (Word) + enchaînements + espacement contextuel.
      const lsMode = node.attrs?.lineSpacingMode as 'multiple' | 'atLeast' | 'exactly' | undefined
      const lsPt   = node.attrs?.lineSpacingPt as number | null | undefined
      target.push({
        spans, align,
        indent: depth * LIST_INDENT + indentLevel * LIST_INDENT + indL + headMarkerW,
        firstLineIndent: indF || undefined,
        indentRight: (indR + tocNumW) || undefined,
        tabStops: (Array.isArray(tabsAttr) && tabsAttr.length) ? tabsAttr : undefined,
        tocPage: tocPage ?? undefined,
        tocPageText: tocNumStr,
        tocLeader: tocLeader || undefined,
        tocLeaderKind: tocLeaderKind as RenderParagraph['tocLeaderKind'],
        marker: listCtx ? (listCtx.type === 'bullet' ? '•' : `${listCtx.idx}.`) : headMarker,
        markerMarks: headMarkerMarks,
        spaceBefore: typeof sbAttr === 'number' ? sbAttr : level > 0 ? (H_BEFORE[level] ?? 10) : listCtx ? 2 : 0,
        spaceAfter:  typeof saAttr === 'number' ? saAttr : level > 0 ? (H_AFTER[level]  ??  4) : listCtx ? 2 : 2,
        pmStart: bStart, pmEnd: pos, docIdx: dIdx, secIdx, breakBefore, lineSpacing,
        lineSpacingMode: lsMode || undefined,
        lineSpacingPt: typeof lsPt === 'number' ? lsPt : undefined,
        contextualSpacing: !!node.attrs?.contextualSpacing,
        styleKey: node.type === 'heading' ? `h${level}` : listCtx ? `li${depth}` : 'p',
        keepLines: !!node.attrs?.keepLines,
        keepNext: !!node.attrs?.keepNext,
        emptyPt: emptyPt && !isNaN(emptyPt) ? emptyPt : undefined,
        dropCap: !!node.attrs?.dropCap,
      })

    } else if (node.type === 'bulletList' || node.type === 'orderedList') {
      let idx = 1
      for (const item of node.content ?? []) {
        if (item.type === 'listItem') {
          pos++  // listItem open
          for (const child of item.content ?? []) block(child, depth + 1, dIdx, { type: node.type === 'bulletList' ? 'bullet' : 'ordered', idx }, target)
          pos++  // listItem close
          idx++
        }
      }

    } else if (node.type === 'horizontalRule') {
      target.push({
        spans: [{ text: '─'.repeat(60), marks: { color: '#dadce0', fontSize: 8 }, pmPos: pos }],
        align: 'left', indent: 0, spaceBefore: 8, spaceAfter: 8,
        pmStart: bStart, pmEnd: pos + 1, docIdx: dIdx, secIdx, breakBefore, lineSpacing,
      })

    } else if (node.type === 'codeBlock') {
      const codeMarks: TextMark = { fontFamily: 'Courier New', fontSize: 10, backgroundColor: '#f8f9fa' }
      const spans: RenderSpan[] = []
      for (const inline of node.content ?? []) {
        if (inline.type === 'text') { spans.push({ text: inline.text ?? '', marks: codeMarks, pmPos: pos }); pos += (inline.text ?? '').length }
      }
      target.push({ spans, align: 'left', indent: 8, spaceBefore: 4, spaceAfter: 4, pmStart: bStart, pmEnd: pos, docIdx: dIdx, secIdx, breakBefore, lineSpacing })

    } else {
      for (const child of node.content ?? []) pos += nodeSize(child)
    }

    pos++  // closing tag
  }

  // Titres repliables (Word « Développer/Réduire ») : un titre `collapsed` masque
  // tous les blocs suivants jusqu'au prochain titre de niveau ≤. Les blocs masqués
  // sont quand même parcourus (cible « poubelle ») pour que les positions PM des
  // blocs visibles suivants restent exactes.
  const discard: RenderParagraph[] = []
  let hideLevel = 0
  for (let i = 0; i < (doc.content?.length ?? 0); i++) {
    const node = doc.content![i]
    // Word replie sur le NIVEAU HIÉRARCHIQUE, jamais sur le style : un paragraphe
    // promu niveau 2 est repliable, un « Titre 1 » remis à « Corps de texte » ne
    // l'est plus. Même règle que la table des matières (references/outline.ts).
    const lvl = outlineLevelOfJson(node)
    const isHeading = lvl > 0
    if (hideLevel > 0) {
      if (isHeading && lvl <= hideLevel) {
        hideLevel = 0   // ce titre clôt la zone repliée → rendu normalement ci-dessous
      } else {
        block(node, 0, i, undefined, discard)
        discard.length = 0
        continue
      }
    }
    block(node, 0, i)
    if (_collapsibleHeadings && isHeading && node.attrs?.collapsed) hideLevel = lvl
  }
  return result
}

// ── Layout engine ─────────────────────────────────────────────────────────────

// Cœur partagé : pose les paragraphes en empilant les lignes ; la largeur de
// chaque paragraphe est choisie par section via `widthFor(secIdx)`.
// Segment horizontal disponible pour le texte (habillage multi-segments).
export interface TextSeg { left: number; width: number }

// Objet flottant que le texte doit habiller. `band` = « haut et bas » (bloque toute
// la largeur). `contourMode` = rapproché/au travers (suivre le canal alpha).
interface FloatBox {
  x0: number; x1: number; y0: number; y1: number
  distL: number; distR: number
  side: string             // both | left | right | largest
  band?: boolean
  contourMode?: 'tight' | 'through'
  src?: string
  imgY0?: number; imgH?: number; imgX0?: number; imgW?: number   // boîte AABB de l'objet (pour le contour)
  rot?: number; dispW?: number; dispH?: number                   // rotation + dims d'affichage (contour tourné)
  anchorIdx?: number; anchorTop?: number                         // paragraphe d'ancrage + son y (passes itératives)
}

const MIN_SEG_W = 40   // largeur minimale utilisable d'un segment de texte (px)

// Les flottants ne sont connus qu'au passage de leur paragraphe d'ANCRAGE : seul le
// texte SUIVANT les habillait. Un objet déplacé AU-DESSUS de son ancre (glissé vers
// le haut, wrapY négatif, grande distance haute) chevauchait donc titres et
// paragraphes précédents. → Passes itératives (façon Word) : chaque passe habille
// avec la liste COMPLÈTE des flottants de la passe précédente, jusqu'à stabilité
// (cap 4 passes). Le cas courant (aucun flottant au-dessus de son ancre) reste en
// UNE seule passe — aucun surcoût à la frappe.
function layoutParagraphs(
  paragraphs: RenderParagraph[],
  widthFor: (secIdx: number) => number,
): { out: LayoutParagraph[]; totalHeight: number } {
  const stable = (a: FloatBox[], b: FloatBox[]): boolean =>
    a.length === b.length && a.every((f, i) => Math.abs(f.x0 - b[i].x0) < 0.5 && Math.abs(f.x1 - b[i].x1) < 0.5 && Math.abs(f.y0 - b[i].y0) < 0.5 && Math.abs(f.y1 - b[i].y1) < 0.5)
  let prev: FloatBox[] = []
  let res = layoutParagraphsPass(paragraphs, widthFor, prev)
  for (let pass = 0; pass < 3; pass++) {
    const above = res.floats.some(f => f.y0 < (f.anchorTop ?? f.y0) - 8)
    if (!above || stable(res.floats, prev)) break
    prev = res.floats
    res = layoutParagraphsPass(paragraphs, widthFor, prev)
  }
  return { out: res.out, totalHeight: res.totalHeight }
}

function layoutParagraphsPass(
  paragraphs: RenderParagraph[],
  widthFor: (secIdx: number) => number,
  prevFloats: FloatBox[],
): { out: LayoutParagraph[]; totalHeight: number; floats: FloatBox[] } {
  const out: LayoutParagraph[] = []
  let y = 0
  // Flottants de la passe PRÉCÉDENTE dont l'ancre n'a pas encore été atteinte :
  // ils préfigurent (à leur ancienne position) les objets ancrés PLUS BAS, pour que
  // le texte AVANT l'ancre les habille aussi. Retirés dès que l'ancre est traitée.
  const pendingPrev = prevFloats.slice()

  // Flottants rencontrés (images carré/rapproché/au travers/haut-bas, tableaux
  // habillés) : le texte suivant les contourne. floatBottom = bas du plus bas
  // (la hauteur totale du document doit les couvrir même sans texte après).
  const floats: FloatBox[] = []
  let floatBottom = 0

  // Plages opaques (x doc) d'un flottant à contour sur la bande [yTop, yTop+h] :
  // union des plages des rangées d'échantillonnage couvertes, intervalles fusionnés.
  const contourRuns = (f: FloatBox, c: ImgContour, yTop: number, h: number): Array<[number, number]> => {
    const iy = f.imgY0!, ih = Math.max(1, f.imgH!)
    const r0 = Math.max(0, Math.floor((yTop - iy) / ih * c.n))
    const r1 = Math.min(c.n - 1, Math.ceil((yTop + h - iy) / ih * c.n) - 1)
    const all: Array<[number, number]> = []
    for (let r = r0; r <= r1; r++) {
      for (const [u0, u1] of c.rows[r] ?? []) all.push([f.imgX0! + u0 * f.imgW!, f.imgX0! + u1 * f.imgW!])
    }
    if (!all.length) return []
    all.sort((a, b) => a[0] - b[0])
    const merged: Array<[number, number]> = [[all[0][0], all[0][1]]]
    for (let i = 1; i < all.length; i++) {
      const [a, b] = all[i], last = merged[merged.length - 1]
      if (a <= last[1] + 4) last[1] = Math.max(last[1], b)
      else merged.push([a, b])
    }
    return merged
  }

  // Segments de texte disponibles pour une bande [yTop, yTop+h] (y global) : on part
  // de [0, cw] et on soustrait l'empreinte (étendue des distances) de chaque flottant
  // qui chevauche verticalement. [] = rien de disponible (la mise en page descend).
  const segsAt = (cw: number) => (yTop: number, h: number): TextSeg[] => {
    let iv: Array<[number, number]> = [[0, cw]]
    for (const f of (pendingPrev.length ? floats.concat(pendingPrev) : floats)) {
      if (yTop + h <= f.y0 || yTop >= f.y1) continue   // pas de chevauchement vertical
      const blocks: Array<[number, number]> = []
      if (f.band) {
        blocks.push([-1e9, 1e9])
      } else {
        let runs: Array<[number, number]> = [[f.x0, f.x1]]
        if (f.contourMode && f.src) {
          const c = getContour(f.src, f.rot || 0, f.dispW || 0, f.dispH || 0)
          if (c) {
            runs = contourRuns(f, c, yTop, h)
            // Rapproché = enveloppe (texte à l'extérieur du contour seulement) ;
            // au travers = chaque plage (le texte passe dans les trous intérieurs).
            if (f.contourMode === 'tight' && runs.length > 1) runs = [[runs[0][0], runs[runs.length - 1][1]]]
          }
        }
        for (const [rx0, rx1] of runs) {
          let lo = rx0 - f.distL, hi = rx1 + f.distR
          if (f.side === 'left') hi = 1e9          // « seulement à gauche » : rien à droite de l'objet
          else if (f.side === 'right') lo = -1e9   // « seulement à droite »
          else if (f.side === 'largest') {         // « le côté le plus large » uniquement
            if (f.x0 >= cw - f.x1) hi = 1e9; else lo = -1e9
          }
          blocks.push([lo, hi])
        }
      }
      for (const [b0, b1] of blocks) {
        const next: Array<[number, number]> = []
        for (const [a0, a1] of iv) {
          if (b1 <= a0 || b0 >= a1) { next.push([a0, a1]); continue }
          if (b0 > a0) next.push([a0, b0])
          if (b1 < a1) next.push([b1, a1])
        }
        iv = next
      }
    }
    return iv.filter(([a, b]) => b - a >= MIN_SEG_W).map(([a, b]) => ({ left: a, width: b - a }))
  }

  for (let pIdx = 0; pIdx < paragraphs.length; pIdx++) {
    const para = paragraphs[pIdx]
    // Ancre atteinte : le flottant re-sera enregistré à sa position de CETTE passe →
    // retirer sa préfiguration de la passe précédente (sinon double empreinte).
    if (pendingPrev.length) {
      for (let k = pendingPrev.length - 1; k >= 0; k--) if (pendingPrev[k].anchorIdx === pIdx) pendingPrev.splice(k, 1)
    }
    // Espacement contextuel (Word « ne pas ajouter d'espace entre paragraphes du même
    // style ») : collapse l'espace avant/après entre deux paragraphes de même style.
    const prev = paragraphs[pIdx - 1], next = paragraphs[pIdx + 1]
    const sameAsPrev = !!prev && prev.styleKey === para.styleKey && (para.contextualSpacing || prev.contextualSpacing)
    const sameAsNext = !!next && next.styleKey === para.styleKey && (para.contextualSpacing || next.contextualSpacing)
    const spaceBefore = sameAsPrev ? 0 : para.spaceBefore
    const spaceAfter  = sameAsNext ? 0 : para.spaceAfter
    y += spaceBefore
    const pY = y

    // ── Tableau : mise en page 2D dédiée (cellules côte à côte) ──────────────
    if (para.table) {
      const cwT = widthFor(para.secIdx)
      const { lines, table, height } = layoutTable(para.table, cwT, pY)
      // Habillage « autour » (Propriétés du tableau) : un tableau plus étroit que la
      // zone devient un flottant — le texte suivant coule à côté, y n'avance pas.
      const tx0 = table.colX?.[0] ?? 0
      const tw = (table.colX?.at(-1) ?? cwT) - tx0
      const around = para.table.wrap === 'around' && tw < cwT - MIN_SEG_W - 20
      out.push({
        lines, y: pY - spaceBefore,
        height: height + spaceBefore + spaceAfter,
        pmStart: para.pmStart, pmEnd: para.pmEnd, docIdx: para.docIdx,
        secIdx: para.secIdx, breakBefore: para.breakBefore, table,
      })
      if (around) {
        // Distances au texte environnant (Word : Positionnement du tableau).
        const wd = para.table.wrapDist
        floats.push({ x0: tx0, x1: tx0 + tw, y0: pY - (wd?.t ?? 4), y1: pY + height + (wd?.b ?? 8),
                      distL: wd?.l ?? 12, distR: wd?.r ?? 12, side: 'both', anchorIdx: pIdx, anchorTop: pY })
        floatBottom = Math.max(floatBottom, pY + height + 8)
        y = pY   // le texte suivant remonte à côté du tableau
      } else {
        y = pY + height + spaceAfter
      }
      continue
    }

    const cw = widthFor(para.secIdx)
    // layoutParagraph passe un yRel (relatif au paragraphe) → on le ramène au y global (pY + yRel).
    const segGlobal = segsAt(cw)
    const lines = layoutParagraph(para, cw, (yRel, h) => segGlobal(pY + yRel, h))
    // Enregistre le flottant (carré/rapproché/au travers = côtés ; haut-bas = bande
    // pleine largeur). L'image ne réserve pas de hauteur dans le flux : la zone
    // d'habillage démarre à sa position réelle (pY + wrapY) pour le texte qui suit.
    const imgLine = lines.find(l => l.image && (l.image.wrap === 'square' || l.image.wrap === 'tight' || l.image.wrap === 'through' || l.image.wrap === 'topBottom'))
    if (imgLine && imgLine.image) {
      const im = imgLine.image
      const dL = im.wrapDistL ?? 10, dR = im.wrapDistR ?? 10
      const dT = im.wrapDistT ?? 0,  dB = im.wrapDistB ?? 0
      const iy0 = pY + (im.wrapY || 0)
      // Objet TOURNÉ : l'empreinte habillée est sa boîte englobante (AABB) centrée
      // sur le centre de dessin — sinon le texte habille l'ancien emplacement.
      const rot = im.rotation || 0
      const ab = imgAABB(im.w, im.h, rot)
      const bx0 = im.x + im.w / 2 - ab.w / 2
      const by0 = iy0 + im.h / 2 - ab.h / 2
      if (im.wrap === 'topBottom') {
        floats.push({ band: true, x0: 0, x1: 0, y0: by0 - Math.max(4, dT), y1: by0 + ab.h + Math.max(4, dB), distL: 0, distR: 0, side: 'both', anchorIdx: pIdx, anchorTop: pY })
      } else {
        floats.push({
          x0: bx0, x1: bx0 + ab.w, y0: by0 - dT, y1: by0 + ab.h + dB,
          distL: dL, distR: dR,
          side: im.wrapSide || 'both',
          contourMode: im.wrap === 'tight' || im.wrap === 'through' ? im.wrap : undefined,
          src: im.src, imgY0: by0, imgH: ab.h, imgX0: bx0, imgW: ab.w,
          rot, dispW: im.w, dispH: im.h,
          anchorIdx: pIdx, anchorTop: pY,
        })
      }
      floatBottom = Math.max(floatBottom, by0 + ab.h + dB)
    }
    // y ABSOLU depuis le y RELATIF posé par layoutParagraph (les rangées peuvent
    // sauter des bandes d'exclusion ou partager un même y en multi-segments).
    let maxBottom = 0
    for (const line of lines) {
      const rel = line.y
      line.y = pY + rel
      // Répartir l'interligne (leading) : moitié au-dessus, moitié en dessous du texte
      // (centrage vertical, façon Google Docs). `line.height` inclut déjà l'interligne.
      // Leading = (hauteur de ligne − hauteur naturelle)/2 — vaut pour TOUS les modes
      // (multiple/au moins/exactement) ; repli au multiplicateur si naturalH inconnue.
      const topLead = line.naturalH != null
        ? (line.height - line.naturalH) / 2
        : (line.height * (1 - 1 / para.lineSpacing)) / 2
      line.baseline = line.y + topLead + line.ascent
      if (rel + line.height > maxBottom) maxBottom = rel + line.height
    }
    y = pY + maxBottom

    const pHeight = y - pY
    y += spaceAfter

    out.push({
      lines,
      y: pY - spaceBefore,
      height: pHeight + spaceBefore + spaceAfter,
      pmStart: para.pmStart,
      pmEnd: para.pmEnd,
      docIdx: para.docIdx,
      secIdx: para.secIdx,
      breakBefore: para.breakBefore,
      keepLines: para.keepLines,
      keepNext: para.keepNext,
      tocPage: para.tocPage,
      tocPageText: para.tocPageText,
      tocLeaderKind: para.tocLeaderKind,
      tocLeader: para.tocLeader,
    })
  }

  return { out, totalHeight: Math.max(y, floatBottom), floats }
}

// Mise en page d'un tableau : colonnes égales, texte des cellules réagencé à la
// largeur de colonne, hauteur de ligne = max des cellules. Renvoie les lignes de
// toutes les cellules (avec x absolu et y à partir de `yTop`), la géométrie des
// bordures et la hauteur totale. Les lignes portent cellX/cellW pour coordsToPos.
// Marge intérieure de cellule : horizontale généreuse (lisibilité), VERTICALE faible
// (Word utilise ~0 en haut/bas) → des hauteurs de ligne compactes, fidèles à Word.
// Marges intérieures de cellule PAR DÉFAUT (px). Word les rend réglables par tableau
// (Propriétés → Options…) ; `table.cellMargin*` les remplace quand elles sont posées.
const CELL_PAD_X = 6
const CELL_PAD_Y = 2
const MIN_ROW_H = 22
// Largeur minimale d'une colonne (px) : plancher de la disposition automatique.
const MIN_COL_W = 24
// Tinte un hex (#rrggbb) avec un alpha → rgba (fond d'en-tête / lignes alternées).
function tint(hex: string, alpha: number): string {
  const h = hex.replace('#', '')
  const n = h.length === 3 ? h.split('').map(c => c + c).join('') : h
  const r = parseInt(n.slice(0, 2), 16) || 0, g = parseInt(n.slice(2, 4), 16) || 0, b = parseInt(n.slice(4, 6), 16) || 0
  return `rgba(${r},${g},${b},${alpha})`
}
// Place une cellule éventuellement fusionnée dans une grille colCount×N. Gère
// colspan/rowspan via une carte d'occupation ; calcule x/y/w/h en px et la couleur
// de fond selon le style (bande d'en-tête, lignes alternées) ou la couleur propre.
function layoutTable(table: RenderTable, contentW: number, yTop: number): { lines: LayoutLine[]; table: LayoutTable; height: number } {
  const colCount = Math.max(1, table.colCount)
  const lines: LayoutLine[] = []
  const cells: LayoutTableCell[] = []
  const accent = table.accent || '#1a73e8'
  const style = table.style || 'grid'
  // Marges intérieures de cellule de CE tableau (Word : Options du tableau).
  const PADL = table.cellMargin?.l ?? CELL_PAD_X, PADR = table.cellMargin?.r ?? CELL_PAD_X
  const PADT = table.cellMargin?.t ?? CELL_PAD_Y, PADB = table.cellMargin?.b ?? CELL_PAD_Y
  // Espacement entre cellules (Word : Options du tableau) : chaque cellule est
  // rétrécie de SP/2 sur ses quatre côtés, les bordures cessent donc d'être
  // partagées et un vide apparaît entre les cellules.
  const SP = Math.max(0, table.cellSpacing ?? 0)

  // Largeurs de colonnes : explicites (réglées par glisser, capées à la largeur de
  // contenu) ou uniformes. colX = bornes cumulées (colCount+1 valeurs).
    // ── Disposition AUTOMATIQUE des colonnes (w:tblLayout autofit, le défaut de Word) ──
  // Word ne renvoie pas le texte à la ligne dès qu'il dépasse : il ÉLARGIT d'abord la
  // colonne en prenant sur les autres, et ne renvoie/ne coupe que lorsque les voisines
  // sont à leur minimum. C'est l'algorithme de disposition automatique au sens CSS :
  // chaque colonne a une largeur MINIMALE (son plus long mot insécable) et une largeur
  // MAXIMALE (son contenu sans aucun retour à la ligne) ; on répartit la largeur
  // disponible entre ces bornes.
  // Mesuré sur les spans (pas de mise en page complète) : une passe de `measureText`
  // par span, sans réagencer les cellules.
  const contentWidths = (paras: RenderParagraph[]): { min: number; max: number } => {
    let min = 0, max = 0
    for (const para of paras) {
      const pad = (para.indent || 0) + (para.indentRight || 0)
      let sum = 0, word = 0
      for (const sp of para.spans) {
        if (sp.img) { const w = sp.img.w || 0; sum += w; min = Math.max(min, w + pad); word = 0; continue }
        if (sp.fn || sp.en) { const w = measureW(sp.text, sp.marks); sum += w; word += w; continue }
        // Champ : INSÉCABLE (atome) → il borne le minimum de la colonne comme une image.
        if (sp.field) { const w = measureW(sp.text, sp.marks); sum += w; min = Math.max(min, w + pad); word = 0; continue }
        // Découpe sur les blancs : chaque mot borne le minimum, le total borne le maximum.
        const parts = sp.text.split(/(\s+)/)
        for (const part of parts) {
          if (!part) continue
          const w = measureW(part, sp.marks)
          sum += w
          if (/^\s+$/.test(part)) { min = Math.max(min, word + pad); word = 0 }
          else word += w
        }
      }
      min = Math.max(min, word + pad)
      max = Math.max(max, sum + pad)
    }
    return { min: min + PADL + PADR, max: max + PADL + PADR }
  }

  // Répartition façon Word : on part des largeurs PRÉFÉRÉES (celles réglées à la main
  // si elles existent, sinon la largeur du contenu sans retour), bornées en bas par le
  // minimum de chaque colonne, puis on ramène le total à la largeur disponible.
  const autofitWidths = (): number[] => {
    const mins: number[] = new Array(colCount).fill(MIN_COL_W)
    const maxs: number[] = new Array(colCount).fill(MIN_COL_W)
    // `table.rows` et non `rows` : la liste filtrée est déclarée plus bas dans la
    // fonction (accès avant initialisation = exception, layout entier perdu).
    for (const row of (table.rows ?? []).filter(Boolean)) {
      let c = 0
      for (const cell of row.cells) {
        const span = Math.max(1, cell.colspan)
        if (!cell.merged && span === 1 && c < colCount) {
          const m = contentWidths(cell.paras)
          mins[c] = Math.max(mins[c], Math.min(m.min, contentW))
          maxs[c] = Math.max(maxs[c], Math.min(m.max, contentW))
        }
        c += span
      }
    }
    const explicit = !!(table.colWidths && table.colWidths.length === colCount && table.colWidths.every(w => w > 4))
    const pref = explicit
      ? table.colWidths!.map((w, i) => Math.max(mins[i], w))
      : maxs.map((w, i) => Math.max(mins[i], w))
    const sum = pref.reduce((a, b) => a + b, 0)
    if (sum <= contentW) {
      // Largeurs RÉGLÉES à la main : on les respecte telles quelles — le tableau a le
      // droit d'être plus étroit que la zone de texte (ajustement au contenu, glissé du
      // bord droit). Sinon on étale le reste à parts égales pour que le tableau occupe
      // la largeur de la zone de texte, comme un tableau inséré dans Word.
      if (explicit) return pref
      const extra = (contentW - sum) / colCount
      return pref.map(w => w + extra)
    }
    // Déficit : on rogne au prorata de la marge de manœuvre (préféré − minimum), donc
    // les colonnes déjà au minimum ne bougent pas — c'est ce qui fait que le texte
    // finit par se renvoyer à la ligne plutôt que d'écraser une colonne à zéro.
    const slack = pref.map((w, i) => Math.max(0, w - mins[i]))
    const totalSlack = slack.reduce((a, b) => a + b, 0)
    const need = sum - contentW
    if (totalSlack <= 0) {
      const k = contentW / sum
      return pref.map(w => w * k)
    }
    const cut = Math.min(need, totalSlack)
    const out = pref.map((w, i) => w - (slack[i] / totalSlack) * cut)
    // Toujours trop large (toutes les colonnes au minimum) : réduction homothétique.
    const s2 = out.reduce((a, b) => a + b, 0)
    return s2 > contentW ? out.map(w => w * (contentW / s2)) : out
  }

  // Mode 'fixed' (« Largeur de colonne fixe » de Word / w:tblLayout type="fixed") :
  // les largeurs déclarées sont respectées et le texte se renvoie à la ligne. En
  // 'autofit' (le DÉFAUT, comme Word) les colonnes suivent le contenu.
  // Un tableau de largeurs de LONGUEUR INCOHÉRENTE (document ancien, ou insertion de
  // colonne d'une version qui ne le maintenait pas) est RÉPARÉ au lieu d'être ignoré
  // en bloc : on complète les manquantes par la moyenne et on jette les surnuméraires.
  // Ignorer tout faisait basculer la table entière en colonnes uniformes d'un coup.
  let widths: number[]
  const cw = table.colWidths
  if (table.layout !== 'fixed') {
    widths = autofitWidths()
  } else if (cw && cw.length && cw.some(w => w > 4)) {
    const known = cw.filter(w => w > 4)
    const avg = known.reduce((a, b) => a + b, 0) / known.length
    widths = Array.from({ length: colCount }, (_, i) => (cw[i] > 4 ? cw[i] : avg))
    const sum = widths.reduce((a, b) => a + b, 0)
    if (sum > contentW) { const k = contentW / sum; widths = widths.map(w => w * k) }
  } else {
    widths = new Array(colCount).fill(contentW / colCount)
  }
  const colX = [0]; for (let i = 0; i < colCount; i++) colX.push(colX[i] + widths[i])
  const cellW = (c0: number, cspan: number) => colX[Math.min(colCount, c0 + cspan)] - colX[c0]

  // Largeur de texte la plus longue d'un contenu mis en page (pour le texte vertical :
  // cette largeur devient l'EXTENT VERTICAL une fois la cellule tournée de 90°).
  const maxLineW = (cl: ReturnType<typeof layoutParagraphs>): number => {
    let m = 0
    for (const para of cl.out) for (const ln of para.lines) { const last = ln.spans.at(-1); if (last) m = Math.max(m, last.x + last.width) }
    return m
  }

  // 1) Placement dans la grille (colStart/colspan/rowspan) avec occupation des
  //    colonnes par les rowspans descendants. Cellule à texte vertical : mise en page
  //    à largeur quasi illimitée (pas de retour à la ligne), puis tournée au rendu.
  type Placed = { cell: RenderTableCell; r: number; c0: number; cspan: number; rspan: number; cl: ReturnType<typeof layoutParagraphs>; vert: boolean; wc: number; hc: number }
  const placed: Placed[] = []
  const occupied: number[] = new Array(colCount).fill(0)   // lignes restantes couvertes par colonne
  const rows = table.rows.filter(Boolean)
  rows.forEach((row, r) => {
    let c = 0
    for (const cell of row.cells) {
      if (cell.merged) continue   // cellule absorbée (défensif) : ignorée
      while (c < colCount && occupied[c] > 0) c++   // saute les colonnes déjà prises par un rowspan
      if (c >= colCount) break
      const cspan = Math.min(cell.colspan, colCount - c)
      const rspan = Math.max(1, cell.rowspan)
      const vert = cell.dir === 90 || cell.dir === 270
      const innerW = cellW(c, cspan) - PADL - PADR - SP
      const cl = layoutParagraphs(cell.paras, () => (vert ? 100000 : innerW))
      placed.push({ cell, r, c0: c, cspan, rspan, cl, vert, wc: vert ? maxLineW(cl) : 0, hc: cl.totalHeight })
      for (let k = c; k < c + cspan; k++) occupied[k] = rspan
      c += cspan
    }
    for (let k = 0; k < colCount; k++) if (occupied[k] > 0) occupied[k]--
  })

  // 2) Hauteurs de ligne. Mode 'exactly' = hauteur fixe (pas de croissance) ; sinon
  //    base = MIN réglée, puis contenu. Texte vertical : l'extent vertical = `wc`.
  const rhMode = (i: number) => table.rowHeightModes?.[i] || 'atleast'
  const contentH = (p: Placed) => (p.vert ? p.wc : p.cl.totalHeight) + PADT + PADB + SP
  const rowH: number[] = rows.map((_r, i) => {
    const spec = table.rowHeights?.[i] ?? 0
    return rhMode(i) === 'exactly' && spec > 0 ? spec : Math.max(MIN_ROW_H, spec)
  })
  for (const p of placed) if (p.rspan === 1 && rhMode(p.r) !== 'exactly') rowH[p.r] = Math.max(rowH[p.r], contentH(p))
  for (const p of placed) if (p.rspan > 1) {
    const need = contentH(p)
    let have = 0; for (let k = p.r; k < p.r + p.rspan && k < rows.length; k++) have += rowH[k]
    if (need > have && rhMode(Math.min(p.r + p.rspan - 1, rows.length - 1)) !== 'exactly') rowH[Math.min(p.r + p.rspan - 1, rows.length - 1)] += need - have
  }
  const rowTop: number[] = []; let acc = 0
  for (let r = 0; r < rows.length; r++) { rowTop[r] = acc; acc += rowH[r] }

  // Décalage horizontal du tableau (alignement sur la page + retrait gauche, façon Word).
  const tableW = colX[colCount]
  const xOff = table.align === 'center' ? Math.max(0, (contentW - tableW) / 2)
    : table.align === 'right' ? Math.max(0, contentW - tableW)
    : Math.max(0, table.indent || 0)
  const colXoff = colX.map(v => v + xOff)
  const rowY = [...rowTop.map(y => yTop + y), yTop + acc]

  // 3) Géométrie + couleur de fond + report des lignes de texte.
  for (const p of placed) {
    const x = colXoff[p.c0]
    const y = rowTop[p.r]
    const w = cellW(p.c0, p.cspan)
    let h = 0; for (let k = p.r; k < p.r + p.rspan && k < rows.length; k++) h += rowH[k]
    let bg: string | undefined = p.cell.cellBg || undefined
    if (!bg) {
      if (p.r === 0 && (style === 'header' || style === 'striped')) bg = tint(accent, 0.16)
      else if (style === 'striped' && p.r % 2 === 1) bg = tint(accent, 0.06)
    }
    const cellTop = yTop + y
    cells.push({ x: x + SP / 2, y: cellTop + SP / 2, w: Math.max(1, w - SP), h: Math.max(1, h - SP), bg, r: p.r, c: p.c0, colspan: p.cspan, rowspan: p.rspan, borders: p.cell.borders })
    const va = p.cell.vAlign || 'top'
    if (p.vert) {
      // Texte vertical : lignes gardées en LOCAL ; on calcule la transformation
      // (rotation ±90° + translation) pour le rendu et le mappage de position.
      const availW = w - PADL - PADR - SP, availH = h - PADT - PADB - SP
      const blockLeft = x + SP / 2 + PADL + Math.max(0, (availW - p.hc) / 2)
      const vOff = va === 'center' ? Math.max(0, (availH - p.wc) / 2) : va === 'bottom' ? Math.max(0, availH - p.wc) : 0
      const blockTop = cellTop + SP / 2 + PADT + vOff
      const dir = p.cell.dir as 90 | 270
      const rtx = dir === 270 ? blockLeft : blockLeft + p.hc
      const rty = dir === 270 ? blockTop + p.wc : blockTop
      for (const para of p.cl.out) for (const ln of para.lines) {
        ln.rot = dir; ln.rtx = rtx; ln.rty = rty
        ln.cellX = x; ln.cellW = w; ln.cellY = cellTop; ln.cellH = h
        lines.push(ln)
      }
    } else {
      // Alignement vertical du contenu dans la hauteur de cellule (Haut/Centré/Bas).
      const slack = Math.max(0, (h - PADT - PADB - SP) - p.cl.totalHeight)
      const vOff = va === 'center' ? slack / 2 : va === 'bottom' ? slack : 0
      for (const para of p.cl.out) for (const ln of para.lines) {
        for (const sp of ln.spans) sp.x += x + SP / 2 + PADL
        if (ln.caretX !== undefined) ln.caretX += x + SP / 2 + PADL
        ln.y        += cellTop + SP / 2 + PADT + vOff
        ln.baseline += cellTop + SP / 2 + PADT + vOff
        ln.cellX = x
        ln.cellW = w
        lines.push(ln)
      }
    }
  }

  return { lines, table: { cells, style, accent, colX: colXoff, rowY, headerRepeat: table.headerRepeat, headerRows: table.headerRows, borderColor: table.borderColor, borderWidth: table.borderWidth, borderStyle: table.borderStyle }, height: acc }
}

export function layoutDocument(doc: JSONContent, contentW: number): DocumentLayout {
  const { out, totalHeight } = layoutParagraphs(parseDoc(doc), () => contentW)
  return { paragraphs: out, totalHeight, contentW }
}

// Variante multi-sections : une largeur de contenu par section (indexée par secIdx).
export function layoutDocumentMulti(doc: JSONContent, widths: number[], opts?: ParseOpts): DocumentLayout {
  const widthFor = (s: number) => widths[s] ?? widths[widths.length - 1] ?? widths[0]
  const { out, totalHeight } = layoutParagraphs(parseDoc(doc, opts), widthFor)
  return { paragraphs: out, totalHeight, contentW: widths[0] }
}

// Notes de bas de page presentes sur une PAGE (appels rencontres dans ses lignes),
// triees par numero - pour le rendu du bloc de notes au bas de la page.
export function pageFootnotes(pg: PageLayout): Array<{ n: number; text: string; pos: number }> {
  const out: Array<{ n: number; text: string; pos: number }> = []
  for (const para of pg.layout.paragraphs) {
    for (const ln of para.lines) for (const sp of ln.spans) {
      if (sp.fn) out.push({ n: sp.fn.n, text: sp.fn.text, pos: sp.pmPos })
    }
  }
  out.sort((a, b) => a.n - b.n)
  return out
}

// ── ENDNOTES ─────────────────────────────────────────────────────────────────
// The essential difference with footnotes: a footnote block is reserved at the
// bottom of EVERY page (one block painted per page, cf. pageFootnotes), whereas
// endnotes are typeset ONLY ONCE, right after the LAST content of the document —
// on the last page when the remaining room is enough, spilling onto one or more
// extra pages otherwise.
// The block is injected as ORDINARY page content (extra LayoutParagraphs in the
// layout of the last pages) by `appendEndnotePages`, which pagination calls:
// screen, print, PDF export, page count and scrolling therefore see the notes
// without a single extra line of code on the caller's side.
// Every line produced is flagged `phantom`: PAINTED, but invisible to the caret,
// the selection, hit-testing and the spell checker (their PM positions point at
// the `endnote` atom, not at editable text in the flow).

const ENDNOTE_PT      = 9          // 12 CSS px at 96 dpi = the project's default text size
const ENDNOTE_GAP     = 12         // gap between the last body content and the separator rule
const ENDNOTE_SEP_MAX = 150        // separator rule max length (same as the footnote separator)
const ENDNOTE_SEP_H   = 10         // height of the separator row
const ENDNOTE_CLR     = '#3c4043'  // note text ink
const ENDNOTE_SEP_CLR = '#9aa0a6'  // separator rule ink

// Every endnote of the document, in reference-mark order (the roman number itself
// is assigned by parseDoc). Read from the CONTINUOUS layout — never from the
// pages, whose repeated table headers would duplicate reference marks.
export function docEndnotes(layout: DocumentLayout): Array<{ n: number; text: string; pos: number }> {
  const out: Array<{ n: number; text: string; pos: number }> = []
  for (const para of layout.paragraphs) {
    for (const ln of para.lines) for (const sp of ln.spans) {
      if (sp.en) out.push({ n: sp.en.n, text: sp.en.text, pos: sp.pmPos })
    }
  }
  out.sort((a, b) => a.n - b.n)
  return out
}

// Endnotes painted on a page (click boxes), empty when there is none.
export function pageEndnotes(pg: PageLayout): PageEndnoteBox[] { return pg.endnotes ?? [] }

// Separator rule above the first note: an EMPTY span carrying the `underline`
// mark — paintLayout draws the underline as a 1 px rule the width of the span, and
// `fillText('')` draws no glyph. No drawing primitive is needed in the layout.
function endnoteSeparatorLine(wrapW: number): LayoutLine {
  const w = Math.min(ENDNOTE_SEP_MAX, Math.max(40, wrapW / 3))
  return {
    spans: [{ text: '', marks: { underline: true, color: ENDNOTE_SEP_CLR, fontSize: ENDNOTE_PT }, x: 0, width: w, pmPos: 0, pmLen: 0 }],
    y: 0, height: ENDNOTE_SEP_H, ascent: Math.max(1, ENDNOTE_SEP_H - 4), baseline: 0,
    pmStart: 0, pmEnd: 0, phantom: true,
  }
}

// Lines of one note: « ii. » as a hanging marker, then the text wrapping UNDER the
// text (not under the number), the way Word does. Reuses the body word-wrap engine
// — hence the same breaks, the same measurements, the same caches.
// MEMOISED (key: measurement epoch + wrap width + number + text): a keystroke in
// the body re-lays out the whole document, while the notes themselves almost never
// change. Without this cache, paginating a 500-note document paid ~65-240 ms of
// re-composition on EVERY keystroke (CPU throttled ×4). Memoised lines are read
// only: placement clones the line (y/baseline) and never touches the spans.
// The TEMPLATE is composed WITHOUT any PM position (pmPos/pmStart = 0): the
// position of the `endnote` atom is stamped on the LINE at placement time (it
// shifts on every keystroke upstream, the note text does not) — so no stale
// position is ever handed out.
const _enLineCache = new Map<string, LayoutLine[]>()
const EN_CACHE_MAX = 4000
function endnoteLines(n: number, text: string, wrapW: number): LayoutLine[] {
  const key = `${_measEpoch}|${Math.round(wrapW)}|${n}|${text}`
  const hit = _enLineCache.get(key)
  if (hit) return hit
  const lines = buildEndnoteLines(n, text, wrapW)
  if (_enLineCache.size > EN_CACHE_MAX) _enLineCache.clear()
  _enLineCache.set(key, lines)
  return lines
}
function buildEndnoteLines(n: number, text: string, wrapW: number): LayoutLine[] {
  const marks: TextMark = { fontSize: ENDNOTE_PT, color: ENDNOTE_CLR }
  const marker  = romanLower(n) + '.'
  const markerW = measureW(marker + ' ', marks)
  const para: RenderParagraph = {
    spans: [{ text: text || '…', marks, pmPos: 0 }],
    align: 'left', indent: markerW, spaceBefore: 0, spaceAfter: 0,
    pmStart: 0, pmEnd: 0, docIdx: -1, secIdx: 0,
    breakBefore: false, lineSpacing: LH_RATIO, marker, markerMarks: marks,
  }
  return layoutParagraph(para, Math.max(markerW + MIN_SEG_W, wrapW))
}

// Bottom of the content already placed on a page (page-local coords): text lines,
// table cell boxes and the bounding box of a floating object included — otherwise
// the notes block would overlap a wrapped image or a table border.
function pageContentBottom(pg: PageLayout): number {
  let b = 0
  for (const para of pg.layout.paragraphs) {
    for (const ln of para.lines) {
      let bot = ln.y + ln.height
      const im = ln.image
      if (im) {
        const ab = imgAABB(im.w, im.h, im.rotation || 0)
        bot = Math.max(bot, isFloatingWrap(im.wrap) ? ln.y + (im.wrapY || 0) + im.h / 2 + ab.h / 2 : ln.y + ab.h)
      }
      if (bot > b) b = bot
    }
    if (para.table) for (const c of para.table.cells) { const bot = c.y + c.h; if (bot > b) b = bot }
  }
  return b
}

// Typesets the endnotes block after the last content and adds it to the pages
// (creating the extra pages it needs). Mutates in place; called by pagination,
// right before it hands its pages back.
export function appendEndnotePages(
  pgs: PageLayout[],
  layout: DocumentLayout,
  geomFor: (secIdx: number) => { contentH: number; colW: number },
): void {
  if (!pgs.length) return
  const notes = docEndnotes(layout)
  if (!notes.length) return

  const last     = pgs[pgs.length - 1]
  const secIdx   = last.secIdx
  const g        = geomFor(secIdx)
  const contentH = last.height || g.contentH
  if (!(contentH > 40)) return   // degenerate geometry: typeset nothing
  // Wrap width = width of ONE COLUMN of the section (= content width when single
  // column). The block is placed at x = 0, in the first column.
  const wrapW = Math.max(80, g.colW || layout.contentW)

  // Rows to place, in order: the separator, then the lines of every note.
  // `noteIdx` = -1 for the separator; used to group the click boxes.
  const rows: Array<{ line: LayoutLine; noteIdx: number }> = [{ line: endnoteSeparatorLine(wrapW), noteIdx: -1 }]
  notes.forEach((nt, i) => {
    for (const ln of endnoteLines(nt.n, nt.text, wrapW)) rows.push({ line: ln, noteIdx: i })
  })

  // ── Placement, page by page ─────────────────────────────────────────────────
  let pgIdx = pgs.length - 1
  let y     = pageContentBottom(last) + ENDNOTE_GAP
  let group: LayoutLine[] = []                 // lines placed on the current page
  let boxes: PageEndnoteBox[] = []             // click boxes of the current page
  let firstPos: number | null = null           // PM position carried by the fragment

  const flushGroup = () => {
    if (!group.length) return
    const pg = pgs[pgIdx]
    const y0 = group[0].y
    const y1 = group[group.length - 1].y + group[group.length - 1].height
    // pmStart === pmEnd: the fragment is INVISIBLE to every walk that bounds by PM
    // range (word selection, paragraph boundaries), while still pointing at the
    // `endnote` atom for those reading `doc.nodeAt(para.pmStart)`.
    const pos = firstPos ?? notes[0].pos
    pg.layout.paragraphs.push({
      lines: group, y: y0, height: y1 - y0, pmStart: pos, pmEnd: pos,
      docIdx: -1, secIdx: pg.secIdx, breakBefore: false,
    })
    if (boxes.length) pg.endnotes = [...(pg.endnotes ?? []), ...boxes]
    group = []; boxes = []; firstPos = null
  }
  const nextPage = () => {
    flushGroup()
    const prev = pgs[pgIdx]
    pgs.push({
      layout: { paragraphs: [], totalHeight: contentH, contentW: prev.layout.contentW },
      startY: prev.startY + prev.height, height: contentH, secIdx,
    })
    pgIdx = pgs.length - 1
    y = 0
  }

  // The separator is NEVER left alone at the bottom of a page: if it does not fit
  // together with the first line of the first note, the whole block moves to a fresh
  // page. This is also what settles the « body ends exactly at the page bottom » case.
  const need0 = rows[0].line.height + (rows[1] ? rows[1].line.height : 0)
  if (y + need0 > contentH + 0.5) nextPage()

  for (const row of rows) {
    // A row taller than a whole page is placed anyway on a blank page (otherwise
    // infinite loop); it gets clipped, as Word does.
    if (y > 0.5 && y + row.line.height > contentH + 0.5) nextPage()
    const ln  = row.line
    const nt  = row.noteIdx >= 0 ? notes[row.noteIdx] : null
    // Leading split like in the body (cf. layoutParagraphsPass): half above the
    // text, half below.
    const topLead = ln.naturalH != null ? (ln.height - ln.naturalH) / 2 : 0
    // The line is CLONED (the memoised template is shared) and gets its vertical
    // position, `phantom`, and the CURRENT PM position of the note's atom here.
    const pmPos = nt ? nt.pos : (notes[0]?.pos ?? 0)
    group.push({ ...ln, y, baseline: y + topLead + ln.ascent, phantom: true, pmStart: pmPos, pmEnd: pmPos })
    if (nt) {
      if (firstPos == null) firstPos = nt.pos
      const box = boxes[boxes.length - 1]
      // A note split across two pages gets one click box on each page.
      if (box && box.n === nt.n) box.y1 = y + ln.height
      else boxes.push({ n: nt.n, text: nt.text, pos: nt.pos, y0: y, y1: y + ln.height })
    }
    y += ln.height
  }
  flushGroup()
}

// Greedy word-wrap with full justification.
// `exclusion(yRel, h)` → { left, width } disponibles pour une ligne à l'ordonnée
// RELATIVE yRel (px depuis le haut du paragraphe) ; sert à l'habillage carré.
function layoutParagraph(
  para: RenderParagraph,
  contentW: number,
  exclusion?: (yRel: number, h: number) => TextSeg[],
): LayoutLine[] {
  const indentRight = para.indentRight ?? 0
  const firstLine   = para.firstLineIndent ?? 0
  const avail = contentW - para.indent - indentRight
  const lines: LayoutLine[] = []

  // Interligne façon Word : 'exactly' = hauteur fixe (px), 'atLeast' = au moins (px),
  // sinon multiplicateur (Simple/1.5/Double/Multiple via `lineSpacing`). Renvoie la
  // hauteur de ligne ET la hauteur naturelle (pour centrer le texte = `naturalH`).
  const lineH = (natural: number): { h: number; nat: number } => {
    if (para.lineSpacingMode === 'exactly' && para.lineSpacingPt) return { h: para.lineSpacingPt, nat: natural }
    if (para.lineSpacingMode === 'atLeast' && para.lineSpacingPt) return { h: Math.max(natural, para.lineSpacingPt), nat: natural }
    return { h: natural * para.lineSpacing, nat: natural }
  }

  // Taquets de tabulation : positions perso (px depuis la marge gauche) sinon grille par
  // défaut tous les DEFAULT_TAB px (façon Word, 1.27 cm). `nextTabStop(x)` = 1ᵉʳ taquet > x.
  const DEFAULT_TAB = 48
  const tabStops = (para.tabStops && para.tabStops.length) ? [...para.tabStops].sort((a, b) => a - b) : null
  const nextTabStop = (x: number): number => {
    if (tabStops) { for (const ts of tabStops) if (ts > x + 0.5) return ts }
    return Math.floor(x / DEFAULT_TAB + 1) * DEFAULT_TAB
  }

  // Bloc-image : une seule "ligne" de la hauteur de l'image (mise à l'échelle pour
  // tenir dans la largeur de contenu). Taille naturelle dès que l'image est chargée.
  if (para.image) {
    const img = getImage(para.image.src)
    const natW = (imgReady(img) ? img!.naturalWidth  : 0) || 320
    const natH = (imgReady(img) ? img!.naturalHeight : 0) || 200
    // Largeur d'affichage : explicite (clampée à la zone) sinon taille naturelle ajustée.
    let dispW = para.image.width ? Math.min(para.image.width, contentW) : Math.min(natW, contentW)
    // Hauteur : explicite (étirement libre) sinon proportionnelle.
    let dispH = para.image.height || (natH * (dispW / natW))
    if (!isFinite(dispW) || dispW <= 0) dispW = Math.min(natW, contentW)
    if (!isFinite(dispH) || dispH <= 0) dispH = natH * (dispW / natW)
    const rot = para.image.rotation || 0
    // Hauteur de ligne = boîte englobante de l'image tournée (réserve la place).
    const rad = rot * Math.PI / 180
    const aabbH = Math.abs(dispW * Math.sin(rad)) + Math.abs(dispH * Math.cos(rad))
    const wrap = para.image.wrap || 'inline'
    // Les modes d'habillage flottants ne réservent PAS la hauteur de l'image dans le
    // flux (le texte coule par-dessus/dessous, À CÔTÉ, ou au-dessus/en dessous via la
    // bande d'exclusion pour « haut et bas »).
    const floating = isFloatingWrap(wrap)
    // Position horizontale : décalage explicite (glisser) sinon selon l'alignement.
    const alignX = para.image.align === 'center' ? (contentW - dispW) / 2
                 : para.image.align === 'right'  ? (contentW - dispW) : 0
    const x = floating ? (para.image.wrapX || alignX) : alignX
    // Flottant (derrière/devant) : la ligne ne réserve PAS la hauteur de l'image
    // (le texte coule par-dessus/dessous) ; l'image est dessinée en z-order décalée
    // de wrapY. Sinon (aligné/haut-bas) : bloc pleine ligne réservant aabbH.
    const lineH = floating ? Math.max(2, lineMetrics({}).height) : aabbH
    // Stocke aussi dispH dans wrapY pour le carré (l'exclusion a besoin de la hauteur).
    lines.push({ spans: [], y: 0, baseline: 0, height: lineH, ascent: lineH,
      pmStart: para.pmStart, pmEnd: para.pmEnd,
      image: { src: para.image.src, w: dispW, h: dispH, x, rotation: rot, wrap, wrapY: para.image.wrapY || 0, alt: para.image.alt, tbFill: para.image.tbFill, tbStroke: para.image.tbStroke, wrapSide: para.image.wrapSide, wrapDistT: para.image.wrapDistT, wrapDistB: para.image.wrapDistB, wrapDistL: para.image.wrapDistL, wrapDistR: para.image.wrapDistR } })
    return lines
  }

  interface Token { text: string; marks: TextMark; width: number; pmPos: number; isSpace: boolean; isTab?: boolean; img?: { src: string; w: number; h: number; alt?: string; rot?: number }; fn?: { n: number; text: string }; en?: { n: number; text: string }; field?: { kind: string; instr: string }; pmLen?: number }

  // Tokenise into words + whitespace, en isolant CHAQUE tabulation (`\t`) comme un token
  // propre (largeur calculée à la pose, = distance jusqu'au prochain taquet).
  const tokens: Token[] = []
  for (const span of para.spans) {
    // Image inline = UN token de la largeur (boîte tournée) de l'image (insécable).
    if (span.img) {
      tokens.push({ text: span.text, marks: span.marks, width: imgAABB(span.img.w, span.img.h, span.img.rot).w, pmPos: span.pmPos, isSpace: false, img: span.img })
      continue
    }
    // Appel de note : UN token insécable (numéro en exposant) portant la note.
    if (span.fn) {
      tokens.push({ text: span.text, marks: span.marks, width: measureW(span.text, span.marks), pmPos: span.pmPos, isSpace: false, fn: span.fn, pmLen: span.pmLen })
      continue
    }
    // Endnote mark: same unbreakable token (« ii » superscript = 1 PM position).
    if (span.en) {
      tokens.push({ text: span.text, marks: span.marks, width: measureW(span.text, span.marks), pmPos: span.pmPos, isSpace: false, en: span.en, pmLen: span.pmLen })
      continue
    }
    // Champ : UN token INSÉCABLE (comme l'appel de note et l'image inline). Word peut
    // renvoyer à la ligne à l'intérieur du résultat d'un champ ; ici l'atome ne couvre
    // qu'une position PM, donc le scinder rendrait le caret ambigu — on garde un token
    // unique, mesuré avec les marques du run (le champ se lit comme du texte normal).
    if (span.field) {
      tokens.push({ text: span.text, marks: span.marks, width: measureW(span.text, span.marks), pmPos: span.pmPos, isSpace: false, field: span.field, pmLen: span.pmLen })
      continue
    }
    let p = span.pmPos
    for (const chunk of span.text.split(/(\t)/g)) {
      if (!chunk) continue
      if (chunk === '\t') {
        tokens.push({ text: '\t', marks: span.marks, width: 0, pmPos: p, isSpace: true, isTab: true })
        p += 1
      } else {
        for (const part of chunk.split(/(\s+)/g)) {
          if (!part) continue
          tokens.push({ text: part, marks: span.marks, width: measureW(part, span.marks), pmPos: p, isSpace: /^\s+$/.test(part) })
          p += part.length
        }
      }
    }
  }

  // Empty paragraph: the cursor lives at pmStart+1 (inside the opening tag).
  // Si le paragraphe vide porte une taille (fontMarks), la ligne prend CETTE
  // hauteur (sinon défaut) → la ligne vide reflète la mise en forme choisie.
  if (tokens.length === 0) {
    const lm      = lineMetrics(para.emptyPt ? { fontSize: para.emptyPt } : {})
    const innerPos = para.pmStart + 1
    // x du caret selon l'alignement (texte vide → largeur 0) : gauche=indent,
    // centre=milieu de la zone, droite=bord droit. Sinon le caret resterait à gauche
    // alors que la frappe serait centrée/à droite (caret « décalé »).
    const caretX = para.align === 'center' ? para.indent + avail / 2
                 : para.align === 'right'  ? para.indent + avail
                 : para.indent + firstLine
    lines.push({ spans: [], y: 0, baseline: 0, height: lineH(lm.height).h, naturalH: lm.height, ascent: lm.ascent, pmStart: innerPos, pmEnd: innerPos, caretX })
    return lines
  }

  let lineToks: Token[] = []
  let lineW   = 0
  let lStart  = para.spans[0]?.pmPos ?? para.pmStart

  // ── Lettrine : détacher la 1ʳᵉ lettre, la composer en grand (≈3 rangées) et
  // décaler les 3 premières rangées de sa largeur (le texte l'habille). ────────
  const CAP_ROWS = 3, CAP_GAP = 8
  let cap: { text: string; marks: TextMark; w: number; ascent: number } | null = null
  let capX = 0
  if (para.dropCap && tokens.length && !tokens[0].isSpace && !tokens[0].img && !tokens[0].isTab && tokens[0].text) {
    const t0 = tokens[0]
    const ch = String.fromCodePoint(t0.text.codePointAt(0)!)
    const lm0 = lineMetrics(t0.marks)
    const rowH0 = lineH(lm0.height).h
    // Façon Word : le HAUT du glyphe affleure le haut de la 1ʳᵉ rangée et sa ligne
    // de base coïncide avec celle de la 3ᵉ rangée. Hauteur RÉELLE du glyphe
    // (actualBoundingBoxAscent) mesurée à 100 pt puis mise à l'échelle.
    const baselineRel = 2 * rowH0 + (rowH0 - lm0.height) / 2 + lm0.ascent
    const cM = mc()
    cM.font = fontStr({ ...t0.marks, fontSize: 100 })
    const actual100 = cM.measureText(ch).actualBoundingBoxAscent || lineMetrics({ ...t0.marks, fontSize: 100 }).ascent
    const pt = Math.max(8, Math.round(100 * baselineRel / Math.max(1, actual100)))
    const capMarks: TextMark = { ...t0.marks, fontSize: pt }
    cap = { text: ch, marks: capMarks, w: measureW(ch, capMarks), ascent: baselineRel }
    // Consommer la lettre (elle ne participe plus au flux ; ses positions PM restent
    // couvertes par la 1ʳᵉ ligne → le caret retombe au début du texte).
    if (t0.text.length > ch.length) { t0.text = t0.text.slice(ch.length); t0.pmPos += ch.length; t0.width = measureW(t0.text, t0.marks) }
    else tokens.shift()
  }

  // ── Rangées & segments : une rangée visuelle peut comporter PLUSIEURS segments
  // (texte des deux côtés d'un flottant). Les lignes émises portent un y RELATIF
  // (haut de rangée) ; l'appelant le ramène au repère global. Une rangée sans
  // segment disponible (bande « haut et bas ») fait descendre la mise en page.
  const estH = lineH(lineMetrics({}).height).h
  let rowRel = 0, rowMaxH = 0, rowIdx = 0
  let rowSegs: TextSeg[] = [{ left: 0, width: contentW }]
  let segIdx = 0
  let curLeft = para.indent + firstLine, curAvail = avail
  const setSeg = () => {
    const seg = rowSegs[segIdx]
    // Retraits : gauche + 1ʳᵉ ligne sur le premier segment, droit sur le dernier.
    const firstL = rowIdx === 0 && segIdx === 0 ? firstLine : 0
    const indL = segIdx === 0 ? para.indent : 0
    const indR = segIdx === rowSegs.length - 1 ? indentRight : 0
    const capW = cap && rowIdx < CAP_ROWS && segIdx === 0 ? cap.w + CAP_GAP : 0
    if (cap && rowIdx === 0 && segIdx === 0) capX = seg.left + indL + firstL
    curLeft  = seg.left + indL + firstL + capW
    curAvail = Math.max(MIN_SEG_W, seg.width - indL - indR - firstL - capW)
  }
  const startRow = () => {
    let guard = 0
    for (;;) {
      rowSegs = exclusion ? exclusion(rowRel, estH) : [{ left: 0, width: contentW }]
      if (rowSegs.length || ++guard > 4000) break
      rowRel += 8   // bande entièrement bloquée : descendre jusqu'à l'ouverture
    }
    if (!rowSegs.length) rowSegs = [{ left: 0, width: contentW }]
    segIdx = 0
    setSeg()
  }
  startRow()
  // Segment suivant de la même rangée s'il en reste, sinon rangée suivante.
  const nextSegOrRow = () => {
    if (segIdx < rowSegs.length - 1) { segIdx++; setSeg() }
    else { rowRel += rowMaxH || estH; rowMaxH = 0; rowIdx++; startRow() }
  }

  function flush(isLast: boolean) {
    if (!lineToks.length) return
    // Les espaces de FIN de ligne sont conservés comme spans (le caret doit pouvoir
    // s'y placer — sinon appuyer sur Espace en fin de ligne ne déplace pas le curseur)
    // mais EXCLUS du calcul d'alignement (centre/droite/justifié) : visuellement le
    // texte reste calé comme s'il n'y avait pas d'espaces traînants. `trailStart` =
    // index du premier espace traînant ; les tokens >= trailStart sont « invisibles »
    // pour l'alignement.
    let trailStart = lineToks.length
    while (trailStart > 0 && lineToks[trailStart - 1].isSpace) trailStart--

    // Max metrics across all tokens. Une image inline impose SA hauteur à la ligne
    // (bas posé sur la ligne de base) → ascent = hauteur de l'image.
    let maxAsc = 0, maxDsc = 0, maxH = 0
    for (const t of lineToks) {
      if (t.img) {
        const ah = imgAABB(t.img.w, t.img.h, t.img.rot).h
        if (ah > maxAsc) maxAsc = ah
        if (ah > maxH)   maxH   = ah
        continue
      }
      const lm = lineMetrics(t.marks)
      if (lm.ascent  > maxAsc) maxAsc = lm.ascent
      if (lm.descent > maxDsc) maxDsc = lm.descent
      if (lm.height  > maxH)   maxH   = lm.height
    }

    // Largeur des tokens VISIBLES (hors espaces traînants) — base de l'alignement.
    const visW = (a: number, b: number) => { let s = 0; for (let i = a; i < b; i++) s += lineToks[i].width; return s }
    const tw = visW(0, trailStart)

    // Justification extra space per space token (espaces traînants exclus)
    let extraSp = 0
    if (para.align === 'justify' && !isLast) {
      let nSp = 0
      for (let i = 0; i < trailStart; i++) if (lineToks[i].isSpace) nSp++
      if (nSp > 0) extraSp = (curAvail - tw) / nSp
    }

    // Alignment offset (dans la zone disponible courante curLeft..curLeft+curAvail).
    let sx = curLeft
    if (para.align === 'center') sx = curLeft + (curAvail - tw) / 2
    else if (para.align === 'right') sx = curLeft + curAvail - tw

    const spans: LayoutSpan[] = []

    // List/heading marker on first line (heading numbering carries its own marks)
    if (para.marker && lines.length === 0) {
      const mm     = para.markerMarks ?? {}
      const mText  = para.marker + ' '
      const mWidth = measureW(mText, mm)
      spans.push({ text: para.marker, marks: mm, x: curLeft - mWidth, width: mWidth, pmPos: lStart })
    }

    let x = sx
    let lEnd = lStart
    for (let i = 0; i < lineToks.length; i++) {
      const t = lineToks[i]
      // Tabulation : avance jusqu'au prochain TAQUET (perso si défini, sinon grille par défaut).
      const w = t.isTab ? Math.max(2, nextTabStop(x) - x)
              : t.isSpace ? t.width + (i < trailStart ? extraSp : 0)
              : t.width
      spans.push({ text: t.text, marks: t.marks, x, width: w, pmPos: t.pmPos, img: t.img, fn: t.fn, en: t.en, field: t.field, pmLen: t.pmLen })
      x += w
      // ATOME : la fin PM est `pmPos + pmLen` (1), pas la longueur du texte peint —
      // le résultat d'un champ (« 28/07/2026 ») fait N caractères pour 1 position.
      lEnd = t.pmPos + (t.pmLen ?? t.text.length)
    }

    const h = lineH(maxH).h
    lines.push({
      spans, y: rowRel, baseline: 0, height: h, naturalH: maxH, ascent: maxAsc, pmStart: lStart, pmEnd: lEnd,
      // Multi-segments : les lignes suivantes de la rangée partagent le même y ;
      // cellX/cellW bornent le segment (départage horizontal de coordsToPos).
      ...(segIdx > 0 ? { sameRow: true } : {}),
      ...(rowSegs.length > 1 ? { cellX: rowSegs[segIdx].left, cellW: rowSegs[segIdx].width } : {}),
    })
    if (h > rowMaxH) rowMaxH = h
    lStart  = lEnd
    lineToks = []
    lineW   = 0
  }

  // ── Coupure de SECOURS au caractère (règle de Word) ──────────────────────────
  // Un mot plus large que l'espace disponible est COUPÉ (sans trait d'union) pour
  // qu'il ne sorte jamais de son conteneur : c'est ce qui empêche le texte d'une
  // cellule de déborder sur la cellule voisine. Sans ça, un token trop long était
  // posé tel quel et dépassait la bordure.
  // La coupure évite de tomber au milieu d'une paire de substitution ou devant une
  // marque combinante (sinon on casse un caractère en deux).
  const badCut = (str: string, i: number): boolean => {
    const c = str.charCodeAt(i)
    if (c >= 0xDC00 && c <= 0xDFFF) return true          // moitié basse d'une paire
    return /\p{M}/u.test(str[i] ?? '')                    // marque combinante
  }
  const breakToken = (tok: Token, avail: number): [Token, Token] | null => {
    if (tok.img || tok.isTab || tok.fn || tok.en || tok.field || tok.isSpace || tok.text.length < 2) return null
    // Plus long préfixe qui tient (au moins 1 caractère : sinon boucle infinie
    // quand même un seul caractère dépasse — cas d'une colonne très étroite).
    let lo = 1, hi = tok.text.length - 1, best = 1
    while (lo <= hi) {
      const mid = (lo + hi) >> 1
      if (measureW(tok.text.slice(0, mid), tok.marks) <= avail + 0.5) { best = mid; lo = mid + 1 }
      else hi = mid - 1
    }
    while (best > 1 && badCut(tok.text, best)) best--
    const head = tok.text.slice(0, best), rest = tok.text.slice(best)
    if (!rest) return null
    return [
      { ...tok, text: head, width: measureW(head, tok.marks) },
      { ...tok, text: rest, width: measureW(rest, tok.marks), pmPos: tok.pmPos + best },
    ]
  }

  for (let ti = 0; ti < tokens.length; ti++) {
    const tok = tokens[ti]
    // Skip leading whitespace on a WRAPPED line (continuation) : l'espace qui a
    // provoqué le retour ne se redessine pas au début de la ligne suivante. MAIS au
    // tout début du paragraphe (lines.length === 0 → 1ʳᵉ ligne), on GARDE les espaces
    // de début (contenu réel, façon Word) : sinon le caret placé après ces espaces
    // (ex. paragraphe vide où l'on tape Espace) n'appartient à aucune ligne et se
    // téléporte en fin de document.
    if (!lineToks.length && tok.isSpace && lines.length > 0) { lStart = tok.pmPos + tok.text.length; continue }

    if (lineW + tok.width <= curAvail + 0.5) { lineToks.push(tok); lineW += tok.width; continue }

    if (!lineToks.length) {
      // Ligne vide et le token ne tient pas : on le coupe. Insécable (image, tabulation,
      // appel de note, champ) → on le laisse dépasser, faute de mieux.
      const parts = breakToken(tok, curAvail)
      if (parts) { tokens.splice(ti, 1, parts[0], parts[1]); ti--; continue }
      lineToks.push(tok); lineW += tok.width; continue
    }
    flush(false)
    nextSegOrRow()
    // Le token est RÉ-ÉVALUÉ sur la nouvelle ligne (largeur disponible différente en
    // habillage/multi-segments, et il peut encore devoir être coupé).
    if (tok.isSpace) { lStart = tok.pmPos + tok.text.length }
    else ti--
  }
  flush(true)

  // Lettrine : accrochée à la 1ʳᵉ ligne — peinte par paintLayout avec sa baseline
  // calée sur le haut de la rangée + son ascent (l'initiale couvre ~3 rangées).
  if (cap && lines.length) {
    lines[0].dropCap = { text: cap.text, marks: cap.marks, x: capX, ascent: cap.ascent }
  }

  // Paragraphe dont tous les tokens sont des espaces (ex: p(' ') ou p('   ')) :
  // flush() n'a rien produit car les espaces de début sont skippés et les espaces
  // de fin sont trimés. On émet une ligne vide pour que le curseur ait un endroit
  // valide, exactement comme pour un paragraphe structurellement vide.
  if (lines.length === 0) {
    const lm      = lineMetrics({})
    const innerPos = para.pmStart + 1
    lines.push({ spans: [], y: rowRel, baseline: 0, height: lineH(lm.height).h, naturalH: lm.height, ascent: lm.ascent, pmStart: innerPos, pmEnd: innerPos })
  }

  return lines
}

// ── Rendering ─────────────────────────────────────────────────────────────────

// ── Selection helpers ─────────────────────────────────────────────────────────

/** X coordinate of `pos` within its containing line, in content-area px. */
// Avance horizontale d'un préfixe DANS un span, au prorata de sa largeur RÉELLE.
// Sur une ligne JUSTIFIÉE, les espaces sont ÉTIRÉS (span.width > largeur mesurée) :
// mesurer le préfixe seul faisait DÉRIVER caret / sélection / hit-test du clic par
// rapport aux caractères réellement peints, de plus en plus le long de la ligne.
// ── Atomes inline (tabulation, image, appel de note, champ) ───────────────────
// Leur largeur est portée par le span (elle ne se dérive pas du texte) et ils ne
// couvrent qu'UNE position PM quel que soit le nombre de caractères peints : le
// caret ne peut donc se placer QU'À leurs deux bords.
function isAtomSpan(span: { text: string; img?: unknown; fn?: unknown; en?: unknown; field?: unknown }): boolean {
  return span.text === '\t' || !!span.img || !!span.fn || !!span.en || !!span.field
}
// Positions PM couvertes par un span (= longueur du texte pour un span ordinaire).
function spanPmLen(span: { text: string; pmLen?: number }): number {
  return span.pmLen ?? span.text.length
}

function spanPrefixW(span: { text: string; marks: TextMark; width: number }, chars: number): number {
  if (chars <= 0) return 0
  if (chars >= span.text.length) return span.width
  const full = measureW(span.text, span.marks)
  const pre  = measureW(span.text.slice(0, chars), span.marks)
  return full > 0 ? pre * (span.width / full) : 0
}

function xAtPosInLine(line: LayoutLine, pos: number): number {
  const first = line.spans[0]
  // `pos` peut tomber AVANT le premier span quand un espace de début a été
  // rogné au word-wrap (line.pmStart < first.pmPos). On le ramène au début
  // visuel de la ligne au lieu de retomber sur textEnd (sinon la sélection des
  // lignes enveloppées s'effondre à droite).
  if (first && pos <= first.pmPos) return first.x
  for (const span of line.spans) {
    const spanEnd = span.pmPos + spanPmLen(span)
    if (pos >= span.pmPos && pos <= spanEnd) {
      // Tabulation / image inline / champ : la largeur du span EST l'avance (atom 1 pos) —
      // le caret après l'objet est à son bord droit, pas à 0 (measureW d'un ZWSP).
      if (isAtomSpan(span)) return span.x + (pos > span.pmPos ? span.width : 0)
      return span.x + spanPrefixW(span, pos - span.pmPos)
    }
  }
  const last = line.spans.at(-1)
  return last ? last.x + last.width : 0
}

/**
 * Compute axis-aligned selection rectangles for the range [from, to].
 * Returns one rect per line that overlaps the selection.
 * Coordinates are in unscaled content-area px (same space as LayoutLine).
 */
export function selectionRects(
  layout: DocumentLayout,
  from:   number,
  to:     number,
): SelectionRect[] {
  if (from >= to) return []

  // 1) Collecter les lignes sélectionnées, dans l'ordre du document.
  interface SelLine { x1: number; x2: number; y: number; height: number }
  const sel: SelLine[] = []
  const rotRects: SelectionRect[] = []   // surbrillances des cellules à texte vertical (déjà en écran)

  for (const para of layout.paragraphs) {
    for (const line of para.lines) {
      // Une ligne NON VIDE n'est retenue que si elle contient au moins un caractère
      // RÉEL de [from, to) — bornes strictes. Sinon, une plage commençant pile à
      // `pmEnd` (mot en début de ligne/segment suivant, l'espace traînant appartenant
      // à cette ligne) produisait un rect dégénéré gonflé en « ruban 8px » : le
      // correcteur soulignait des espaces vides en marge et autour des habillages.
      // Les lignes VIDES (paragraphe vide, pmStart == pmEnd) gardent les bornes
      // inclusives : leur ruban de sélection doit rester visible.
      if (line.phantom) continue   // en-tête répété : jamais surligné
      const emptyLine = line.pmStart === line.pmEnd
      if (emptyLine ? (line.pmEnd < from || line.pmStart > to) : (line.pmEnd <= from || line.pmStart >= to)) continue
      if (line.image) continue   // l'image gère son propre cadre de sélection

      // Texte vertical : la sélection est une bande tournée. On émet sa boîte
      // englobante écran (surbrillance approximative mais bien placée).
      if (line.rot) {
        const lx1 = xAtPosInLine(line, Math.max(from, line.pmStart))
        const last = line.spans.at(-1)
        const lx2 = to < line.pmEnd ? xAtPosInLine(line, to) : (last ? last.x + last.width : lx1)
        const corners = [rotToScreen(line, lx1, line.y), rotToScreen(line, lx2, line.y), rotToScreen(line, lx1, line.y + line.height), rotToScreen(line, lx2, line.y + line.height)]
        const xs = corners.map(c => c.x), ys = corners.map(c => c.y)
        const x = Math.min(...xs), yy = Math.min(...ys)
        rotRects.push({ x, y: yy, w: Math.max(2, Math.max(...xs) - x), h: Math.max(2, Math.max(...ys) - yy) })
        continue
      }

      const x1 = xAtPosInLine(line, Math.max(from, line.pmStart))
      const last = line.spans.at(-1)
      const textEnd = last ? last.x + last.width : x1

      // Largeur = jusqu'à la FIN DU TEXTE de la ligne (comme Google : bords en
      // escalier, pas de remplissage jusqu'à la marge). Si la sélection se termine
      // au milieu de la ligne, on s'arrête à `to`.
      let x2: number
      if (to < line.pmEnd) {
        x2 = xAtPosInLine(line, to)
      } else {
        x2 = textEnd
      }
      // Lignes vides / continuées : petit ruban visible à gauche (comme Google).
      if (x2 <= x1) x2 = x1 + 8

      sel.push({ x1, x2, y: line.y, height: line.height })
    }
  }

  // 2) Continuité verticale : chaque rectangle (sauf le dernier) s'étend jusqu'au
  //    haut de la ligne suivante pour combler les interlignes / espaces de
  //    paragraphe → bloc continu sans trou blanc, comme Google Docs.
  const rects: SelectionRect[] = []
  for (let i = 0; i < sel.length; i++) {
    const s = sel[i]
    // +1px de recouvrement sur la ligne suivante : sous `ctx.scale(dpr*zoom)`
    // deux rects adjacents qui se touchent pile sur une frontière fractionnaire
    // se font anti-aliaser des deux côtés → fin joint blanc entre chaque ligne.
    // Le chevauchement (même couleur opaque) supprime ce joint sans rien décaler.
    const h = i < sel.length - 1
      ? Math.max(s.height, sel[i + 1].y - s.y) + 1
      : s.height
    rects.push({ x: s.x1, y: s.y, w: s.x2 - s.x1, h })
  }

  return [...rects, ...rotRects]
}

// ── Rendering ─────────────────────────────────────────────────────────────────

// ── Table des matières : géométrie du numéro de page + points de suite ─────────
// MÉMOÏSÉE par paragraphe : sans cache, une TDM de 60 entrées recalculait la
// largeur du numéro et ~100 abscisses de points PAR ENTRÉE et PAR FRAME (et posait
// autant de `fillRect`), soit plus cher que le texte de la page. Les objets
// LayoutParagraph sont recréés à chaque mise en page ET à chaque pagination, donc le
// cache s'invalide de lui-même ; `epoch`/`contentW` couvrent les deux cas où le même
// objet devrait être recalculé (purge des mesures après chargement d'une police,
// peinture à une autre largeur de contenu).
interface TocPaint {
  epoch:    number
  contentW: number
  numText:  string   // '' = entrée sans numéro de page
  numX:     number   // x du numéro, aligné à DROITE du bord utile
  baseline: number
  dotsX:    number[] // abscisses des points de suite ([] = pas de points)
  dotsY:    number
  dotW:     number   // largeur d'un motif (point rond ≈ carré, tiret = segment)
  dotH:     number
  /** Trait CONTINU (« caractères de suite » = soulignement) : [x0, x1] ou null. */
  bar:      [number, number] | null
}
const _tocCache = new WeakMap<LayoutParagraph, TocPaint | null>()
const TOC_DOT_CLR = '#80868b'

function computeTocPaint(para: LayoutParagraph, contentW: number): TocPaint | null {
  const last = para.lines[para.lines.length - 1]
  // Numéro/points sur la DERNIÈRE ligne de l'entrée (comme Word quand une entrée
  // s'enroule sur plusieurs lignes). Ligne répliquée (en-tête de tableau) ou
  // ligne-image : rien à décorer.
  if (!last || last.phantom || last.image) return null
  // Bord droit utile : la COLONNE (multi-colonnes) ou le SEGMENT de texte (habillage
  // autour d'un flottant) quand la ligne en porte les bornes, sinon la zone de
  // contenu. Sans ça, en deux colonnes le numéro se posait au bord droit de la PAGE,
  // à des centaines de px du texte de son entrée.
  const rightEdge = (last.cellX != null && last.cellW != null) ? last.cellX + last.cellW : contentW
  const numText = para.tocPageText ?? (para.tocPage != null ? String(para.tocPage) : '')
  // Largeur mesurée du numéro (1, 2, 3… chiffres) : la place réservée à droite par
  // parseDoc (`tocNumW`) vient de la MÊME mesure, donc elle suit toujours le numéro.
  const numW = numText ? measureW(numText, tocNumMarks()) : 0
  const numX = rightEdge - numW
  const lastSpan = last.spans[last.spans.length - 1]
  const textEnd = lastSpan ? lastSpan.x + lastSpan.width : (last.caretX ?? 0)
  const dotsX: number[] = []
  // « Caractères de suite » de Word : points (défaut), tirets, ou soulignement
  // continu. Les trois occupent la même bande, entre la fin du texte et le numéro.
  const kind = para.tocLeaderKind && para.tocLeaderKind !== 'none' ? para.tocLeaderKind : 'dots'
  const step = kind === 'dashes' ? TOC_DOT_STEP * 2 : TOC_DOT_STEP
  const dotW = kind === 'dashes' ? TOC_DOT_STEP : TOC_DOT_SIZE
  const dotH = kind === 'dashes' ? 1 : TOC_DOT_SIZE
  let bar: [number, number] | null = null
  if (para.tocLeader && kind === 'underline') {
    const x0 = textEnd + TOC_DOT_PAD, x1 = numX - TOC_DOT_PAD
    if (x1 > x0) bar = [x0, x1]
  } else if (para.tocLeader) {
    // Points sur une grille ABSOLUE (multiple de TOC_DOT_STEP) : ils s'alignent
    // verticalement d'une entrée à l'autre, comme les taquets à points de Word.
    // Aucun point si le texte touche déjà le numéro (entrée longue, retrait profond).
    const x1 = numX - TOC_DOT_PAD
    for (let x = Math.ceil((textEnd + TOC_DOT_PAD) / step) * step; x < x1; x += step) dotsX.push(x)
  }
  return {
    epoch: _measEpoch, contentW, numText, numX, baseline: last.baseline,
    dotsX, dotsY: last.baseline - 1.5, dotW, dotH, bar,
  }
}

function tocPaint(para: LayoutParagraph, contentW: number): TocPaint | null {
  // Rejet O(1) de l'écrasante majorité des paragraphes : aucun accès au cache.
  if (para.tocPage == null && !para.tocLeader) return null
  const hit = _tocCache.get(para)
  if (hit !== undefined && (hit === null || (hit.epoch === _measEpoch && hit.contentW === contentW))) return hit
  const res = computeTocPaint(para, contentW)
  _tocCache.set(para, res)
  return res
}

/// Options de PEINTURE (marqueurs d'écran) — indépendantes de la mise en page.
export interface PaintOpts {
  /** Plage PM du caret / de la sélection : un champ qu'elle touche prend la trame
   *  grise (mode 'selected'). `null` = aucun. Absent = valeur de `setFieldCaret`. */
  caret?: { from: number; to: number } | null
  /** Impression / export : AUCUNE trame de champ — c'est un marqueur d'écran, Word
   *  ne l'imprime jamais, quel que soit le mode choisi. */
  print?: boolean
}

// Peint un layout (images derrière + texte + images devant) dans le contexte
// COURANT (déjà mis à l'échelle/translaté par l'appelant), SANS effacer ni gérer
// la sélection. Réutilisé pour le corps ET pour l'en-tête/pied (rendu riche).
export function paintLayout(ctx: CanvasRenderingContext2D, layout: DocumentLayout, frontPhase: 'with' | 'skip' | 'only' = 'with', opts?: PaintOpts): void {
  const drawImgLine = (line: LayoutLine) => {
    const im = line.image!
    const { w, h, x: ix, rotation } = im
    const floating = isFloatingWrap(im.wrap)
    const cx2 = ix + w / 2
    const cy2 = floating ? line.y + (im.wrapY || 0) + h / 2 : line.y + line.height / 2
    // Zone de texte riche : peindre la boîte (fond + bordure) puis le sous-document.
    const tbAlt = im.alt && im.alt.startsWith('kbtextrich:') ? im.alt : null
    if (tbAlt) {
      ctx.save()
      ctx.translate(cx2, cy2)
      if (rotation) ctx.rotate(rotation * Math.PI / 180)
      if (im.tbFill !== 'none') { ctx.fillStyle = im.tbFill || '#ffffff'; ctx.fillRect(-w / 2, -h / 2, w, h) }
      if (im.tbStroke !== 'none') { ctx.strokeStyle = im.tbStroke || '#9aa0a6'; ctx.lineWidth = 1.5; ctx.strokeRect(-w / 2 + 0.75, -h / 2 + 0.75, w - 1.5, h - 1.5) }
      const inner = richTextBoxLayout(tbAlt, w - 2 * RICH_TB_PAD)
      if (inner) {
        ctx.save(); ctx.beginPath(); ctx.rect(-w / 2, -h / 2, w, h); ctx.clip()
        paintLayoutAt(ctx, inner, -w / 2 + RICH_TB_PAD, -h / 2 + RICH_TB_PAD)
        ctx.restore()
      }
      ctx.restore()
      return
    }
    // Formes vectorielles (`kbshape:`) : régénérer le SVG à la résolution RÉELLE
    // du périphérique (zoom × DPR) pour un rendu net à tout zoom — au lieu de
    // rasteriser une fois à la taille du nœud puis d'étirer le bitmap.
    let src = im.src
    if (im.alt && im.alt.startsWith('kbshape:') && _shapeSrcResolver && w > 0 && h > 0) {
      const sc = ctx.getTransform().a || 1
      const W = Math.min(2400, Math.max(8, Math.round(w * sc)))
      const H = Math.min(2400, Math.max(8, Math.round(h * sc)))
      const hi = _shapeSrcResolver(im.alt, W, H)
      if (hi) src = hi
    }
    const img = getImage(src)
    ctx.save()
    ctx.translate(cx2, cy2)
    if (rotation) ctx.rotate(rotation * Math.PI / 180)
    if (imgReady(img)) ctx.drawImage(img!, -w / 2, -h / 2, w, h)
    else { ctx.fillStyle = '#f1f3f4'; ctx.fillRect(-w / 2, -h / 2, w, h); ctx.strokeStyle = '#dadce0'; ctx.strokeRect(-w / 2 + 0.5, -h / 2 + 0.5, w - 1, h - 1) }
    ctx.restore()
  }
  // Objet scindé au saut de page : ne peindre que dans sa fenêtre verticale
  // (repère contenu) — la partie hors fenêtre est dessinée sur la page voisine.
  const drawImgLineClipped = (line: LayoutLine) => {
    const ct = line.imgClipTop, cb = line.imgClipBottom
    if (ct == null && cb == null) { drawImgLine(line); return }
    ctx.save()
    ctx.beginPath()
    const y0 = ct ?? -1e6
    ctx.rect(-1e6, y0, 2e6, (cb ?? 1e6) - y0)
    ctx.clip()
    drawImgLine(line)
    ctx.restore()
  }
  // Phase 'only' : ne dessiner QUE les images « devant le texte » (couche du dessus,
  // appelée APRÈS les fautes/curseur par renderDocument). 'skip' : tout sauf elles.
  if (frontPhase === 'only') {
    for (const para of layout.paragraphs) for (const line of para.lines) {
      if (line.image && line.image.wrap === 'front') drawImgLineClipped(line)
    }
    return
  }
  // Images flottantes DERRIÈRE le texte.
  for (const para of layout.paragraphs) for (const line of para.lines) {
    if (line.image && line.image.wrap === 'behind') drawImgLineClipped(line)
  }

  // ── Entrées de TABLE DES MATIÈRES ────────────────────────────────────────────
  // Numéro de page calé à droite + points de suite depuis la fin du texte (Word).
  // Toute la page est peinte en UN passage : une seule police posée pour les numéros
  // et UN SEUL chemin pour TOUS les points (un `fillRect` par point coûtait des
  // milliers d'appels par frame sur une longue TDM).
  {
    const tocs: TocPaint[] = []
    for (const para of layout.paragraphs) {
      const tp = tocPaint(para, layout.contentW)
      if (tp) tocs.push(tp)
    }
    if (tocs.length) {
      ctx.font = fontStr(tocNumMarks())
      ctx.fillStyle = DEFAULT_CLR
      for (const tp of tocs) if (tp.numText) ctx.fillText(tp.numText, tp.numX, tp.baseline)
      let dots = false
      ctx.beginPath()
      for (const tp of tocs) {
        for (const x of tp.dotsX) { ctx.rect(x, tp.dotsY, tp.dotW, tp.dotH); dots = true }
        // « Soulignement » : une bande continue, même passe de remplissage.
        if (tp.bar) { ctx.rect(tp.bar[0], tp.dotsY, tp.bar[1] - tp.bar[0], 1); dots = true }
      }
      if (dots) { ctx.fillStyle = TOC_DOT_CLR; ctx.fill() }
    }
  }

  // Trame grise des champs : marqueur d'ÉCRAN (jamais à l'impression). Prédicat
  // calculé UNE fois par page, pas par span.
  const caret = opts?.caret !== undefined ? opts.caret : _fieldCaret
  const shadeAll = !opts?.print && _fieldShading === 'always'
  const shadeSel = !opts?.print && _fieldShading === 'selected' && !!caret
  const shadeField = (span: LayoutSpan): boolean => {
    if (shadeAll) return true
    if (!shadeSel || !caret) return false
    // Le champ occupe [pmPos, pmPos+1] : bornes INCLUSIVES pour que le caret posé
    // sur l'un de ses deux bords compte comme « dans le champ », comme Word.
    return caret.from <= span.pmPos + spanPmLen(span) && caret.to >= span.pmPos
  }

  // Barres de modification : collectées PENDANT la passe de texte (aucun balayage
  // supplémentaire des spans) puis peintes en UN SEUL chemin — des rectangles
  // adjacents se soudent en un trait continu sur plusieurs lignes, comme Word.
  const showRev = _revisionDisplay === 'markup'
  const revBars: Array<{ y: number; h: number }> = []

  // Texte (+ images inline / bloc / carré).
  for (const para of layout.paragraphs) {
    if (para.table) {
      const tstyle = para.table.style || 'grid'
      // L'encre du tableau (fonds + bordures) est CLIPPÉE à la bande de contenu
      // de la page : une cellule scindée sur plusieurs pages a une boîte qui
      // déborde au-dessus/au-dessous — sans clip, ses bordures/fonds se
      // peignaient dans les MARGES, par-dessus l'en-tête et le pied de page
      // (Word arrête l'encre à la marge et la reprend sous l'en-tête suivant,
      // sans trait de fermeture au saut de page).
      // Tolérance de CELL_PAD_Y en haut/bas : le repère de page est calé sur la
      // première LIGNE de texte prise, or le haut d'une cellule est CELL_PAD_Y
      // au-dessus de sa première ligne → la rangée 0 d'un tableau qui commence en
      // haut de page a un y légèrement NÉGATIF et sa bordure haute tombait hors
      // clip (bordure supérieure invisible, quel que soit le style). 3px ne peuvent
      // pas atteindre l'en-tête (marge haute ~96px).
      const BLEED = CELL_PAD_Y + 1
      ctx.save()
      ctx.beginPath()
      ctx.rect(-1e6, -BLEED - 0.5, 2e6, layout.totalHeight + 2 * BLEED + 1)
      ctx.clip()
      // Fonds de cellule (en-tête / lignes alternées / couleur propre) d'abord.
      for (const cell of para.table.cells) {
        if (cell.bg) { ctx.fillStyle = cell.bg; ctx.fillRect(cell.x, cell.y, cell.w, cell.h) }
      }
      paintTableBorders(ctx, para.table, tstyle)
      ctx.restore()
    }
    for (const line of para.lines) {
      if (line.image) {
        if (line.image.wrap !== 'behind' && line.image.wrap !== 'front') drawImgLineClipped(line)
        continue
      }
      // Lettrine : grande initiale, baseline calée sur le haut de la 1ʳᵉ rangée +
      // son ascent (elle descend sur ~3 rangées, habillées par le texte).
      if (line.dropCap) {
        const dc = line.dropCap
        ctx.font = fontStr(dc.marks)
        ctx.fillStyle = dc.marks.color ?? DEFAULT_CLR
        ctx.fillText(dc.text, dc.x, line.y + dc.ascent)
      }
      // ── Débordement VERTICAL d'une cellule (règle de Word) ───────────────────
      // Normalement la rangée grandit avec son contenu ; en hauteur « Exactement »
      // elle ne grandit pas et Word ROGNE ce qui dépasse (il ne le laisse pas couler
      // sur la rangée suivante). Ligne entièrement hors de la cellule → non peinte ;
      // ligne à cheval → clip sur la boîte de la cellule (coût payé seulement là).
      let cellClip = false
      if (line.cellH != null && line.cellY != null) {
        const cy0 = line.cellY, cy1 = line.cellY + line.cellH
        if (line.y >= cy1 - 0.5) continue
        if (line.y + line.height > cy1 + 0.5 || line.y < cy0 - 0.5) {
          cellClip = true
          ctx.save()
          ctx.beginPath()
          ctx.rect(line.cellX ?? -1e6, cy0, line.cellW ?? 2e6, line.cellH)
          ctx.clip()
        }
      }
      // Cellule à texte vertical : coords des spans en LOCAL ; on applique la
      // rotation (±90°) + translation puis on peint normalement.
      const rotated = line.rot
      if (rotated) { ctx.save(); ctx.translate(line.rtx ?? 0, line.rty ?? 0); ctx.rotate(rotated === 90 ? Math.PI / 2 : -Math.PI / 2) }
      let revLine = false   // cette ligne porte au moins une révision → barre en marge
      for (const span of line.spans) {
        if (showRev && !revLine && (span.marks.ins || span.marks.del)) revLine = true
        // Image/forme inline : dessinée comme un caractère, boîte (tournée) posée sur la
        // ligne de base. Pour une image tournée, on pivote autour de son centre.
        if (span.img) {
          const im = getImage(span.img.src)
          if (imgReady(im)) {
            const ab = imgAABB(span.img.w, span.img.h, span.img.rot)
            const cx = span.x + ab.w / 2, cy = line.baseline - ab.h / 2
            if (span.img.rot) {
              ctx.save()
              ctx.translate(cx, cy)
              ctx.rotate((span.img.rot * Math.PI) / 180)
              ctx.drawImage(im!, -span.img.w / 2, -span.img.h / 2, span.img.w, span.img.h)
              ctx.restore()
            } else {
              ctx.drawImage(im!, span.x, line.baseline - span.img.h, span.img.w, span.img.h)
            }
          }
          continue
        }
        // Suivi des modifications : le run révisé est encré dans la couleur de son
        // AUTEUR (et plus dans la sienne), insertion soulignée / suppression barrée.
        // Test à double détente : un booléen de page, puis deux lectures de propriété —
        // rien de plus sur un document sans révision (chemin chaud).
        const rev = showRev && (span.marks.ins || span.marks.del) ? revisionInk(span.marks) : null
        ctx.font      = fontStr(span.marks)
        ctx.fillStyle = rev ?? span.marks.color ?? DEFAULT_CLR
        if (span.marks.backgroundColor) {
          const prev = ctx.fillStyle
          ctx.fillStyle = span.marks.backgroundColor
          ctx.fillRect(span.x, line.y, span.width, line.height)
          ctx.fillStyle = prev
        }
        // Champ Word : trame grise DERRIÈRE le résultat (le texte, lui, garde
        // exactement la mise en forme du run — Word ne le colore pas). Marqueur
        // d'écran : cf. shadeField / setFieldShading / PaintOpts.print.
        if (span.field && shadeField(span)) {
          const prev = ctx.fillStyle
          ctx.fillStyle = FIELD_SHADE
          ctx.fillRect(span.x, line.y, span.width, line.height)
          ctx.fillStyle = prev
        }
        const basePx = (span.marks.fontSize ?? DEFAULT_PT) * PT_PX
        const scriptDy = span.marks.script === 'super' ? -basePx * 0.36
                        : span.marks.script === 'sub'  ?  basePx * 0.18 : 0
        const drawBaseline = line.baseline + scriptDy
        if (span.marks.letterSpacing) ctx.letterSpacing = `${span.marks.letterSpacing}px`
        // NB : pas de strokeText « stem darkening » — essayé (contour 0.25 de la même
        // couleur, façon ClearType) puis retiré : rendait le texte pseudo-GRAS partout.
        // La densité façon Word vient du noir pur par défaut (DEFAULT_CLR #000).
        ctx.fillText(span.text, span.x, drawBaseline)
        if (span.marks.letterSpacing) ctx.letterSpacing = '0px'
        // Traits : ceux du run (souligné / barré) ET ceux de la révision. Un run
        // inséré PUIS supprimé reçoit les deux, comme Word. Même géométrie que les
        // traits ordinaires → une insertion déjà soulignée ne double pas son trait.
        if (span.marks.underline || (rev && span.marks.ins)) ctx.fillRect(span.x, drawBaseline + 2, span.width, 1)
        if (span.marks.strike    || (rev && span.marks.del)) ctx.fillRect(span.x, drawBaseline - line.ascent * 0.35, span.width, 1)
      }
      // Repère du relecteur : la barre couvre toute la HAUTEUR de la ligne. Pour une
      // cellule à texte vertical, les coords de ligne sont locales (non tournées) :
      // on prend alors les bornes ÉCRAN de la cellule.
      if (revLine) {
        revBars.push(rotated
          ? { y: line.cellY ?? rotToScreen(line, 0, 0).y, h: line.cellH ?? line.height }
          : { y: line.y, h: line.height })
      }
      if (rotated) ctx.restore()
      if (cellClip) ctx.restore()
    }
  }
  // ── Barres de modification en MARGE (Word « lignes modifiées ») ──────────────
  // Peintes en marge GAUCHE de la zone de contenu (x négatif) : elles ne recouvrent
  // jamais le texte, y compris dans un tableau — le repère est en marge de PAGE,
  // pas de cellule. Un seul chemin pour toute la page.
  if (revBars.length) {
    // Au moins un pixel MACHINE : à fort dézoom un trait de 1 px CSS disparaîtrait.
    const sx = Math.abs(ctx.getTransform().a) || 1
    const bw = Math.max(CHANGE_BAR_W, 1 / sx)
    ctx.fillStyle = CHANGE_BAR_CLR
    ctx.beginPath()
    for (const b of revBars) ctx.rect(-CHANGE_BAR_DX, b.y, bw, b.h)
    ctx.fill()
  }

  // Images flottantes DEVANT le texte (sauf en phase 'skip' où renderDocument les
  // dessine plus tard, par-dessus les fautes et le curseur).
  if (frontPhase !== 'skip') {
    for (const para of layout.paragraphs) for (const line of para.lines) {
      if (line.image && line.image.wrap === 'front') drawImgLineClipped(line)
    }
  }
}

// ── Bordures de tableau : résolution par ARÊTE puis tracé ────────────────────
// Chaque arête interne est partagée par deux cellules qui peuvent en demander des
// bordures différentes (voire aucune). On la résout donc UNE fois, façon Word :
//   • une bordure posée explicitement sur une cellule bat le défaut du tableau —
//     y compris « aucune bordure » (sinon retirer les bordures d'une cellule
//     laisserait le défaut du voisin redessiner l'arête partagée) ;
//   • entre deux bordures explicites, la plus épaisse gagne ; à égalité, une
//     bordure bat « aucune » (Word ne laisse pas un côté vide effacer le voisin).
// Résoudre avant de tracer supprime aussi le DOUBLE tracé des arêtes partagées
// (l'ancien code parcourait la boîte pleine de chaque cellule, donc deux fois
// chaque arête interne — visible en tirets/pointillés et en couleur translucide).
type EdgeWin = { spec: CellBorderSpec; weight: number }
function pickEdge(a: EdgeWin | undefined, b: EdgeWin | undefined): EdgeWin | undefined {
  if (!a) return b
  if (!b) return a
  if (a.weight !== b.weight) return a.weight > b.weight ? a : b
  return b.spec.w > a.spec.w ? b : a
}
function paintTableBorders(ctx: CanvasRenderingContext2D, table: LayoutTable, tstyle: string): void {
  // Défaut du tableau (style 'plain' = aucun défaut, seules les bordures explicites sortent).
  const def: CellBorderSpec | null = tstyle === 'plain' ? null : {
    w: table.borderWidth || 1,
    s: table.borderStyle || 'solid',
    c: table.borderColor || '#bdc1c6',
  }
  // Arêtes indexées par géométrie arrondie au 1/100 px : deux cellules contiguës
  // partagent exactement les mêmes bornes colX/rowY, donc la même clé.
  const k = (v: number) => Math.round(v * 100)
  const edges = new Map<string, { x0: number; y0: number; x1: number; y1: number; win: EdgeWin }>()
  const add = (x0: number, y0: number, x1: number, y1: number, side: CellBorderSpec | null | undefined) => {
    // undefined = hériter du défaut (poids 0) ; null = « aucune » explicite (poids 1) ;
    // objet = bordure explicite (poids 2). Une arête sans vainqueur n'est pas tracée.
    const cand: EdgeWin | undefined = side === undefined
      ? (def ? { spec: def, weight: 0 } : undefined)
      : side === null ? undefined
      : { spec: side, weight: 2 }
    const key = `${k(x0)}:${k(y0)}:${k(x1)}:${k(y1)}`
    const prev = edges.get(key)
    // « Aucune » explicite doit pouvoir ÉVINCER le défaut hérité du voisin : on
    // mémorise son poids même sans bordure à tracer.
    if (side === null) {
      const veto: EdgeWin = { spec: { w: 0, s: 'solid', c: 'transparent' }, weight: 1 }
      const win = pickEdge(prev?.win, veto)
      if (win) edges.set(key, { x0, y0, x1, y1, win })
      return
    }
    if (!cand) return
    const win = pickEdge(prev?.win, cand)
    if (win) edges.set(key, { x0, y0, x1, y1, win })
  }
  for (const cell of table.cells) {
    const bd = cell.borders
    const x0 = cell.x, y0 = cell.y, x1 = cell.x + cell.w, y1 = cell.y + cell.h
    add(x0, y0, x1, y0, bd ? bd.t : undefined)   // haut
    add(x0, y1, x1, y1, bd ? bd.b : undefined)   // bas
    add(x0, y0, x0, y1, bd ? bd.l : undefined)   // gauche
    add(x1, y0, x1, y1, bd ? bd.r : undefined)   // droite
  }
  // Regroupées par (couleur, épaisseur, style) : un seul chemin par pinceau.
  const groups = new Map<string, { spec: CellBorderSpec; segs: Array<[number, number, number, number]> }>()
  for (const e of edges.values()) {
    if (e.win.weight === 1 || e.win.spec.w <= 0) continue   // veto « aucune bordure »
    const gk = `${e.win.spec.c}|${e.win.spec.w}|${e.win.spec.s}`
    const g = groups.get(gk) ?? { spec: e.win.spec, segs: [] }
    g.segs.push([e.x0, e.y0, e.x1, e.y1])
    groups.set(gk, g)
  }
  // Échelle du contexte (dpr × zoom) : les traits sont calés sur la grille de
  // pixels MACHINE, pas sur les px CSS. Les bornes de colonne/ligne tombent sur
  // des valeurs fractionnaires (ex. x = 200.67), donc un simple « +0.5 » étalait
  // chaque trait sur deux pixels — d'où des bordures grises et molles au lieu de
  // franches. On centre donc le trait sur un demi-pixel machine.
  const tf = ctx.getTransform()
  const sx = Math.abs(tf.a) || 1, sy = Math.abs(tf.d) || 1
  for (const g of groups.values()) {
    ctx.save()
    ctx.strokeStyle = g.spec.c
    // Minimum un pixel machine : à fort dézoom, une bordure de ½ pt deviendrait
    // invisible (Word garde le filet visible à tout zoom).
    ctx.lineWidth = Math.max(g.spec.w, 1 / Math.min(sx, sy))
    if (g.spec.s === 'dashed') ctx.setLineDash([6, 4])
    else if (g.spec.s === 'dotted') { ctx.setLineDash([1.5, 3]); ctx.lineCap = 'round' }
    // Épaisseur machine impaire → centre sur un demi-pixel ; paire → sur un entier.
    const snap = (v: number, s: number) => {
      const dev = v * s
      const wDev = Math.round(ctx.lineWidth * s)
      return (wDev % 2 === 1 ? Math.round(dev - 0.5) + 0.5 : Math.round(dev)) / s
    }
    ctx.beginPath()
    for (const [x0, y0, x1, y1] of g.segs) {
      // Un segment est soit horizontal, soit vertical : on ne cale que l'axe
      // perpendiculaire au trait (caler les extrémités raccourcirait les traits).
      if (y0 === y1) { const y = snap(y0, sy); ctx.moveTo(x0, y); ctx.lineTo(x1, y) }
      else           { const x = snap(x0, sx); ctx.moveTo(x, y0); ctx.lineTo(x, y1) }
    }
    ctx.stroke()
    ctx.restore()
  }
}

// Peint un layout à un décalage (px non-scalés) dans le contexte courant —
// utilisé pour l'en-tête / le pied (rendu riche dans la marge).
export function paintLayoutAt(ctx: CanvasRenderingContext2D, layout: DocumentLayout, ox: number, oy: number, opts?: PaintOpts): void {
  ctx.save(); ctx.translate(ox, oy); paintLayout(ctx, layout, 'with', opts); ctx.restore()
}

/**
 * Render the layout onto a canvas.
 * @param zoom           CSS zoom factor (already applied to canvas CSS dimensions)
 * @param dpr            device pixel ratio
 * @param selectionRange optional [from, to] ProseMirror positions — drawn behind text
 * All layout values are in unscaled CSS px.
 */
export function renderDocument(
  canvas:          HTMLCanvasElement,
  layout:          DocumentLayout,
  marginLeft:      number,
  marginTop:       number,
  dpr:             number,
  zoom:            number,
  selectionRange?: { from: number; to: number },
  focused:         boolean = true,
  spellRanges?:    Array<{ from: number; to: number; grammar?: boolean }>,
  highlightRanges?: Array<{ from: number; to: number; color: string }>,
  // Rects de sélection PRÉCALCULÉS (ex. interpolés pour animer le glissement du bord
  // de la surbrillance) — remplacent le calcul depuis `selectionRange` s'ils sont fournis.
  selRectsOverride?: SelectionRect[],
  // Solid page background. When provided, the canvas context is created OPAQUE
  // (alpha: false) and the background is painted here instead of relying on the
  // CSS background behind a transparent canvas. This is what enables subpixel
  // (LCD) text antialiasing: on a transparent surface Skia falls back to
  // grayscale AA, which reads as "washed-out / blurry ink" vs DOM text or Word.
  pageBg?: string,
  // Marqueurs d'écran (trame des champs). L'export PDF / l'impression doivent passer
  // `{ print: true }` ; à défaut, le repli ci-dessous suffit déjà : un rendu SANS
  // sélection et NON focalisé (exactement le cas de l'export) ne trame rien.
  opts?: PaintOpts,
): void {
  const ctx   = canvas.getContext('2d', pageBg ? { alpha: false } : undefined)!
  const scale = dpr * zoom
  const paintOpts: PaintOpts = {
    caret: opts?.caret !== undefined ? opts.caret
         : selectionRange ? { from: selectionRange.from, to: selectionRange.to }
         : focused ? _fieldCaret : null,
    print: opts?.print,
  }

  if (pageBg) { ctx.fillStyle = pageBg; ctx.fillRect(0, 0, canvas.width, canvas.height) }
  else ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.save()
  ctx.scale(scale, scale)
  ctx.translate(marginLeft, marginTop)
  ctx.textBaseline = 'alphabetic'
  // Same text-quality hints as the measurement context, so rendered glyphs
  // match measured widths and benefit from kerning/legibility shaping.
  applyTextQuality(ctx)

  // ── 0. Surbrillances (recherche / commentaires) — SOUS le texte pour rester
  // lisibles. Les plages sont REGROUPÉES PAR COULEUR et chaque couleur peinte en UN
  // SEUL chemin (beginPath + tous les rects + un fill) : là où plusieurs plages de
  // même couleur se chevauchent ou se touchent (ex. commentaires distincts sur des
  // portions voisines), l'alpha n'est ainsi composé qu'UNE fois par pixel → plus de
  // bande plus foncée ni de « ligne de démarcation » à leurs frontières. L'ordre
  // d'apparition des couleurs est préservé (les dernières passent au-dessus).
  if (highlightRanges && highlightRanges.length) {
    const byColor = new Map<string, SelectionRect[]>()
    for (const hr of highlightRanges) {
      let bucket = byColor.get(hr.color)
      if (!bucket) { bucket = []; byColor.set(hr.color, bucket) }
      for (const r of selectionRects(layout, hr.from, hr.to)) bucket.push(r)
    }
    for (const [color, rects] of byColor) {
      ctx.fillStyle = color
      ctx.beginPath()
      for (const r of rects) ctx.rect(r.x, r.y, r.w, r.h)
      ctx.fill()
    }
  }

  // ── 1. Images + texte (passe partagée) — SANS les images « devant le texte »
  // (dessinées en dernier, par-dessus fautes/curseur, comme dans Word). ───────────
  paintLayout(ctx, layout, 'skip', paintOpts)

  // ── 2. Sélection — dessinée EN DERNIER, par-dessus TOUT (texte inclus), en
  // semi-transparent pour laisser transparaître texte/surlignage. α=0.5 ; la base
  // est choisie pour composer sur fond blanc exactement #ABC2FE (focus) /
  // #D9D9D9 (sans focus).
  if (selectionRange && selectionRange.from < selectionRange.to) {
    ctx.fillStyle = focused ? 'rgba(87,133,253,0.5)' : 'rgba(179,179,179,0.5)'
    // UN SEUL fill() d'un chemin combiné : l'alpha n'est composité qu'une fois
    // par pixel, même là où les rectangles se chevauchent (anti-joint inter-lignes,
    // polices/tailles mélangées) → sélection parfaitement uniforme, sans bandes
    // sombres ni coutures claires.
    ctx.beginPath()
    for (const r of selRectsOverride ?? selectionRects(layout, selectionRange.from, selectionRange.to)) {
      ctx.rect(r.x, r.y, r.w, r.h)
    }
    ctx.fill()
  }

  // ── 2b. Soulignés ondulés du correcteur (orthographe rouge / grammaire bleu) ──
  if (spellRanges && spellRanges.length) {
    // Bornes ProseMirror de CETTE page. L'appelant passe TOUTES les fautes du document
    // (pas seulement celles de la page) : sans ce garde-fou, `selectionRects` — qui balaie
    // toutes les lignes de la page — était appelé pour CHAQUE faute sur CHAQUE page. Sur un
    // gros document truffé de fautes (ex. texte latin de test → chaque mot signalé : des
    // milliers de plages × N pages) cela GELAIT le thread. On rejette en O(1) toute plage
    // hors de la page avant le balayage coûteux.
    let pmLo = Infinity, pmHi = -Infinity
    for (const para of layout.paragraphs) { if (para.pmStart < pmLo) pmLo = para.pmStart; if (para.pmEnd > pmHi) pmHi = para.pmEnd }
    // BATCH par couleur : un document truffé de fautes (texte étranger) compte des
    // MILLIERS de soulignés ; un save/beginPath/stroke/restore PAR souligné passait
    // ~90 % du temps dans `restore()`/`stroke()` natifs (profil CPU) et GELAIT le
    // thread sur les gros documents. On accumule tous les segments d'une couleur
    // dans UN SEUL path → 2 strokes par page au total, zéro save/restore par faute.
    ctx.save()
    ctx.lineWidth = 1
    for (const grammarPass of [false, true]) {
      ctx.strokeStyle = grammarPass ? '#1a73e8' : '#d93025'
      ctx.beginPath()
      // Flush périodique : un path 2D de dizaines de milliers de segments rend le
      // `stroke()` natif pathologiquement lent (rasterisation Skia) — le thread reste
      // bloqué DANS l'appel natif (ininterruptible). On trace par tranches bornées.
      let segs = 0
      for (const sr of spellRanges) {
        if (!!sr.grammar !== grammarPass) continue
        if (sr.to <= pmLo || sr.from >= pmHi) continue   // plage entièrement hors de cette page
        // Garde-fou : une faute d'orthographe/grammaire est un mot ou un signe — jamais
        // des centaines de caractères. Une plage anormalement large (issue corrompue par
        // un remap de transaction, plage multi-pages…) produirait un rect PAR LIGNE sur
        // CHAQUE page → path monstre → gel. On l'ignore.
        if (sr.to - sr.from > 200) continue
        for (const r of selectionRects(layout, sr.from, sr.to)) {
          squigglePath(ctx, r.x, r.x + r.w, r.y + r.h - 1)
          segs += Math.ceil(Math.max(0, r.w) / 2) + 1
          if (segs > 2500) { ctx.stroke(); ctx.beginPath(); segs = 0 }
        }
      }
      ctx.stroke()
    }
    ctx.restore()
  }

  // ── 3. Cursor / caret (drawn on top of text) ───────────────────────────────
  if (selectionRange && selectionRange.from === selectionRange.to) {
    const c = posToCoords(layout, selectionRange.from)
    ctx.fillStyle = '#202124'
    // Le contexte est mis à l'échelle dpr×zoom : 1 unité = dpr×zoom pixels machine.
    // Épaisseur = 1 pixel PHYSIQUE exact, comme Word (et comme le caret DOM).
    const w = 1 / (dpr * zoom)
    if (c.italicAngle !== 0) {
      // Slanted caret for italic text: draw a parallelogram
      const lean = c.italicAngle * c.height
      ctx.beginPath()
      ctx.moveTo(c.x - lean,     c.y)
      ctx.lineTo(c.x - lean + w, c.y)
      ctx.lineTo(c.x + w,        c.y + c.height)
      ctx.lineTo(c.x,            c.y + c.height)
      ctx.closePath()
      ctx.fill()
    } else {
      ctx.fillRect(c.x, c.y, w, c.height)
    }
  }

  // ── 4. Images « DEVANT le texte » — couche la plus haute : elles masquent le
  // texte, les soulignés du correcteur ET le curseur (comportement Word). ─────────
  paintLayout(ctx, layout, 'only', paintOpts)

  ctx.restore()
}

// Trait ondulé (style correcteur) entre x1 et x2 à la base y — AJOUTE le zigzag au
// path COURANT sans le tracer. L'appelant regroupe tous les soulignés d'une couleur
// dans un seul beginPath/stroke (les save/stroke/restore par souligné gelaient les
// gros documents, cf. renderDocument).
function squigglePath(ctx: CanvasRenderingContext2D, x1: number, x2: number, y: number): void {
  if (x2 <= x1) return
  const amp = 1.1, wl = 4
  ctx.moveTo(x1, y)
  let up = true
  for (let x = x1; x <= x2; x += wl / 2) { ctx.lineTo(Math.min(x + wl / 2, x2), y + (up ? -amp : amp)); up = !up }
}

// ── Position mapping ──────────────────────────────────────────────────────────

// `preferEnd` = AFFINITÉ du curseur sur une frontière d'enroulement (où la position PM est
// à la fois la fin d'une ligne visuelle et le début de la suivante). false (défaut) → début
// de la ligne suivante (cas ↓/clic/frappe) ; true → fin de la ligne courante (cas touche Fin).
// Transforme un point LOCAL d'une ligne tournée vers les coordonnées écran (rotation
// ±90° autour de l'origine puis translation rtx/rty), et l'inverse.
function rotToScreen(line: LayoutLine, lx: number, ly: number): { x: number; y: number } {
  return line.rot === 90 ? { x: (line.rtx ?? 0) - ly, y: (line.rty ?? 0) + lx } : { x: (line.rtx ?? 0) + ly, y: (line.rty ?? 0) - lx }
}
function screenToRotLocalX(line: LayoutLine, qx: number, qy: number): number {
  return line.rot === 90 ? qy - (line.rty ?? 0) : (line.rty ?? 0) - qy   // composante le long du texte (axe local x)
}

export function posToCoords(layout: DocumentLayout, pos: number, preferEnd = false): CursorMetrics {
  for (const para of layout.paragraphs) {
    for (let li = 0; li < para.lines.length; li++) {
      const line = para.lines[li]
      if (line.phantom) continue
      if (pos < line.pmStart || pos > line.pmEnd) continue

      // Cellule à texte vertical : on calcule le x LOCAL du caret puis on le projette
      // à l'écran ; le caret est une barre tournée (champ `rot`).
      if (line.rot) {
        let lx: number | null = null
        for (const span of line.spans) {
          const spanEnd = span.pmPos + spanPmLen(span)
          if (pos < span.pmPos || pos > spanEnd) continue
          const dx = isAtomSpan(span) ? (pos > span.pmPos ? span.width : 0) : spanPrefixW(span, pos - span.pmPos)
          lx = span.x + dx; break
        }
        if (lx === null) { const last = line.spans.at(-1); lx = last ? last.x + last.width : (line.caretX ?? 0) }
        const p = rotToScreen(line, lx, line.y)
        return { x: p.x, y: p.y, height: line.height, italicAngle: 0, rot: line.rot }
      }

      // Frontière de RETOUR-À-LA-LIGNE automatique : `pos` est la fin de cette ligne ET le
      // début de la ligne suivante (même paragraphe, même position PM). Sans affinité « fin »,
      // on préfère le DÉBUT de la ligne suivante (caret à gauche). Sinon, après un ↓ (ou un
      // clic) au point d'enroulement, le caret se logeait à l'extrême droite et la navigation
      // verticale restait bloquée (cm.y = ligne du dessus → ↓ retombe au même pos).
      const nxt = para.lines[li + 1]
      if (!preferEnd && pos === line.pmEnd && nxt && nxt.pmStart === pos) continue

      for (const span of line.spans) {
        const spanEnd = span.pmPos + spanPmLen(span)
        if (pos < span.pmPos || pos > spanEnd) continue
        // Tabulation / image inline / champ : la largeur du span EST l'avance (atom 1 pos).
        const dx = isAtomSpan(span) ? (pos > span.pmPos ? span.width : 0) : spanPrefixW(span, pos - span.pmPos)
        // Hauteur du caret = EXACTEMENT celle du rectangle de SÉLECTION à cette position
        // (cf. selectionRects : y = line.y, h = line.height) — cohérence caret/surbrillance,
        // comme Word. `baseline` sert à ancrer la prévisualisation des marques stockées.
        return { x: span.x + dx, y: line.y, height: line.height, italicAngle: span.marks.italic ? 0.13 : 0, baseline: line.baseline, lineTop: line.y, lineH: line.height }
      }

      // pos is at end of line — use marks of last span to determine italic angle.
      // Empty line: caret au x mémorisé (`caretX`, selon alignement/indentation) ;
      // à défaut, bord gauche de la cellule (tableau) ou marge de page.
      const last = line.spans.at(-1)
      const emptyX = line.caretX ?? (line.cellX !== undefined ? line.cellX + CELL_PAD_X : 0)
      return {
        x: last ? last.x + last.width : emptyX,
        y: line.y,
        height: line.height,   // = hauteur du rect de sélection (cf. selectionRects)
        italicAngle: last?.marks.italic ? 0.13 : 0,
        baseline: line.baseline,
        lineTop: line.y, lineH: line.height,
      }
    }
  }

  // Fallback — after all content
  const lastLine = layout.paragraphs.at(-1)?.lines.at(-1)
  if (lastLine) {
    const last = lastLine.spans.at(-1)
    return {
      x: last ? last.x + last.width : (lastLine.caretX ?? (lastLine.cellX !== undefined ? lastLine.cellX + CELL_PAD_X : 0)),
      y: lastLine.y,
      height: lastLine.height,   // = hauteur du rect de sélection (cf. selectionRects)
      italicAngle: last?.marks.italic ? 0.13 : 0,
      baseline: lastLine.baseline,
      lineTop: lastLine.y, lineH: lastLine.height,
    }
  }
  return { x: 0, y: 0, height: DEFAULT_PT * PT_PX * LH_RATIO, italicAngle: 0 }
}

// Centre vertical de la ligne visuelle ADJACENTE (au-dessus / en-dessous d'un top donné).
// Sert à la navigation ↑/↓ : viser « bord de ligne ± 2 px » tombait dans les ESPACEMENTS
// de paragraphe (spaceBefore/After), où la ligne d'origine restait la plus proche →
// caret bloqué ; viser le CENTRE de la vraie ligne adjacente est sans ambiguïté.
// Renvoie null en première/dernière ligne.
export function adjacentLineCenter(layout: DocumentLayout, fromTop: number, dir: 1 | -1): number | null {
  let bestTop: number | null = null, bestH = 0
  for (const para of layout.paragraphs) {
    for (const line of para.lines) {
      if (line.phantom) continue
      const t = line.y
      if (dir === 1 ? t > fromTop + 0.5 : t < fromTop - 0.5) {
        if (bestTop === null || (dir === 1 ? t < bestTop : t > bestTop)) { bestTop = t; bestH = line.height }
      }
    }
  }
  return bestTop === null ? null : bestTop + bestH / 2
}

export function coordsToPos(layout: DocumentLayout, x: number, y: number): number {
  // Trouver la meilleure ligne : priorité verticale, puis (pour les cellules de
  // tableau partageant un même y) départage horizontal par les bornes de cellule.
  let best: LayoutLine | null = null
  let bestScore = Infinity

  for (const para of layout.paragraphs) {
    for (const line of para.lines) {
      if (line.phantom) continue
      // Cellule à texte vertical : on classe par distance à la BOÎTE écran de la
      // cellule (cellX/cellY/cellW/cellH), puis on choisit la « colonne » de texte
      // la plus proche le long de l'axe tourné.
      if (line.rot && line.cellX !== undefined && line.cellY !== undefined && line.cellW !== undefined && line.cellH !== undefined) {
        const ddx = x < line.cellX ? line.cellX - x : x > line.cellX + line.cellW ? x - (line.cellX + line.cellW) : 0
        const ddy = y < line.cellY ? line.cellY - y : y > line.cellY + line.cellH ? y - (line.cellY + line.cellH) : 0
        const score = (ddx + ddy) * 100000 + Math.abs((rotToScreen(line, 0, line.y).x) - x) + Math.abs((rotToScreen(line, 0, line.y).y) - y)
        if (score < bestScore) { bestScore = score; best = line }
        continue
      }
      const dy = (y >= line.y && y <= line.y + line.height)
        ? 0 : Math.min(Math.abs(y - line.y), Math.abs(y - line.y - line.height))
      let dx = 0
      if (line.cellX !== undefined && line.cellW !== undefined) {
        if (x < line.cellX) dx = line.cellX - x
        else if (x > line.cellX + line.cellW) dx = x - (line.cellX + line.cellW)
      }
      // Dans un tableau, l'appartenance HORIZONTALE à la colonne prime sur la
      // proximité verticale (sinon cliquer dans le bas d'une cellule courte renvoie
      // vers une cellule voisine plus haute). dx pèse donc bien plus que dy ; pour le
      // texte normal dx=0, le classement par dy est inchangé.
      const score = dx * 100000 + dy
      if (score < bestScore) { bestScore = score; best = line }
    }
  }

  if (!best) return layout.paragraphs.at(-1)?.lines.at(-1)?.pmEnd ?? 1

  // Position cible le long du texte : x écran pour les lignes normales, ou
  // composante locale tournée pour une cellule à texte vertical.
  const target = best.rot ? screenToRotLocalX(best, x, y) : x

  // Find nearest character in that line
  let bestPos  = best.pmStart
  let bestXd   = Infinity

  for (const span of best.spans) {
    const wideAtom = isAtomSpan(span)   // largeur portée par le span (atom)
    const len = spanPmLen(span)         // positions PM offertes au clic
    for (let i = 0; i <= len; i++) {
      const cx = span.x + (wideAtom ? (i > 0 ? span.width : 0) : spanPrefixW(span, i))
      const d  = Math.abs(target - cx)
      if (d < bestXd) { bestXd = d; bestPos = span.pmPos + i }
    }
  }

  return bestPos
}

// ── Word / paragraph boundary helpers ────────────────────────────────────────

// Caractère de remplacement d'un ATOME dans le texte plat d'un paragraphe :
// invisible et NON-mot (\w ne le matche pas), il agit donc comme une frontière de
// mot — un champ ou un appel de note ne se sélectionne pas « avec » le mot voisin.
const ATOM_CHAR = '\u2063'   // INVISIBLE SEPARATOR (non-word, zero width)

// Texte plat d'un paragraphe + position PM de chaque caractère. Un atome (champ,
// appel de note, image inline) compte pour UNE position, quel que soit le nombre de
// caractères qu'il peint : sans ça, les positions dérivées de l'index plat sortaient
// du nœud (ex. un champ date « 28/07/2026 » = 10 caractères pour 1 position PM) et la
// sélection de mot / la navigation Ctrl+←→ sautaient au-delà du paragraphe.
function paraFlatText(para: LayoutParagraph): { text: string; pmPosOf: number[] } {
  let text = ''
  const pmPosOf: number[] = []
  for (const line of para.lines) {
    for (const span of line.spans) {
      const len = spanPmLen(span)
      if (len !== span.text.length) {
        for (let i = 0; i < len; i++) { pmPosOf.push(span.pmPos + i); text += ATOM_CHAR }
        continue
      }
      for (let i = 0; i < span.text.length; i++) {
        pmPosOf.push(span.pmPos + i)
        text += span.text[i]
      }
    }
  }
  return { text, pmPosOf }
}

/**
 * Returns the ProseMirror {from, to} range for the word under the cursor.
 * "Word" = contiguous run of \w characters.
 * If `pos` is on a space, selects the word to the left (matches browser double-click).
 * Returns { from: pos, to: pos } when pos is not near any word.
 */
export function wordBoundariesAt(layout: DocumentLayout, pos: number): { from: number; to: number } {
  for (const para of layout.paragraphs) {
    if (pos < para.pmStart + 1 || pos > para.pmEnd) continue

    // Build flat char list for the whole paragraph
    const { text: fullText, pmPosOf } = paraFlatText(para)
    if (fullText.length === 0) return { from: pos, to: pos }

    // Index PLAT du caractère au niveau/après le curseur. ⚠️ NE PAS calculer
    // `pos - (pmStart + 1)` : dans un TABLEAU les positions PM SAUTENT entre
    // cellules (tokens d'ouverture/fermeture de cellule et de paragraphe) → le
    // texte plat n'est pas contigu avec les positions PM. On dérive l'index du
    // mapping `pmPosOf` (correct pour paragraphe normal ET cellules).
    let offset = pmPosOf.findIndex(pp => pp >= pos)
    if (offset < 0) offset = fullText.length
    const isW = (i: number) => i >= 0 && i < fullText.length && /\w/.test(fullText[i])
    // Contiguïté PM entre le caractère i-1 et i : une DISCONTINUITÉ (>1) marque
    // une frontière de cellule/nœud → le mot ne doit PAS la franchir (sinon la
    // sélection s'étendrait à une autre cellule = plage PM invalide).
    const contig = (i: number) => i > 0 && i < pmPosOf.length && pmPosOf[i] === pmPosOf[i - 1] + 1

    const extend = (start: number): { lo: number; hi: number } => {
      let lo = start, hi = start
      while (lo > 0 && isW(lo - 1) && contig(lo)) lo--
      while (hi < fullText.length && isW(hi) && (hi === start || contig(hi))) hi++
      return { lo, hi }
    }
    let { lo, hi } = extend(offset)
    if (lo === hi) {
      // Curseur hors d'un mot → tente le mot à gauche (même cellule).
      ({ lo, hi } = extend(offset > 0 && contig(offset) ? offset - 1 : offset))
      if (lo === hi) return { from: pos, to: pos }
    }

    const from = pmPosOf[lo]
    const to   = pmPosOf[hi - 1] + 1   // fin du dernier caractère inclus (borne cellule sûre)
    return { from, to }
  }
  return { from: pos, to: pos }
}

/**
 * Returns the ProseMirror {from, to} range for the full paragraph containing `pos`.
 * Equivalent to triple-click selection in a word processor.
 */
export function paragraphBoundariesAt(layout: DocumentLayout, pos: number): { from: number; to: number } {
  for (const para of layout.paragraphs) {
    if (pos < para.pmStart || pos > para.pmEnd) continue
    return { from: para.pmStart + 1, to: para.pmEnd }
  }
  return { from: pos, to: pos }
}

// ── Keyboard navigation helpers ───────────────────────────────────────────────

/** Position at the start of the visual line containing `pos` (Home key).
 *  Returns the first span's pmPos rather than pmStart — they differ when a
 *  leading space was stripped during word-wrap (pmStart points before it).
 */
export function lineStartAt(layout: DocumentLayout, pos: number, preferEnd = false): number {
  for (const para of layout.paragraphs) {
    for (let li = 0; li < para.lines.length; li++) {
      const line = para.lines[li]
      if (line.phantom) continue
      if (pos < line.pmStart || pos > line.pmEnd) continue
      // Frontière d'enroulement (pos = fin de cette ligne = début de la suivante) : sans
      // affinité « fin », le caret est sur la ligne SUIVANTE → on saute cette ligne pour
      // viser le bon début visuel (sinon Début/Fin agiraient sur la ligne du dessus).
      const nxt = para.lines[li + 1]
      if (!preferEnd && pos === line.pmEnd && nxt && nxt.pmStart === pos) continue
      return line.spans[0]?.pmPos ?? line.pmStart
    }
  }
  const first = layout.paragraphs[0]?.lines[0]
  return first?.spans[0]?.pmPos ?? first?.pmStart ?? 1
}

/** Position at the end of the visual line containing `pos` (End key). */
export function lineEndAt(layout: DocumentLayout, pos: number, preferEnd = false): number {
  for (const para of layout.paragraphs) {
    for (let li = 0; li < para.lines.length; li++) {
      const line = para.lines[li]
      if (line.phantom) continue
      if (pos < line.pmStart || pos > line.pmEnd) continue
      const nxt = para.lines[li + 1]
      if (!preferEnd && pos === line.pmEnd && nxt && nxt.pmStart === pos) continue
      return line.pmEnd
    }
  }
  return layout.paragraphs.at(-1)?.lines.at(-1)?.pmEnd ?? 1
}

/** First valid cursor position in the document (Ctrl+Home). */
export function docStart(layout: DocumentLayout): number {
  return layout.paragraphs[0]?.lines[0]?.pmStart ?? 1
}

/** Last valid cursor position in the document (Ctrl+End). */
export function docEnd(layout: DocumentLayout): number {
  return layout.paragraphs.at(-1)?.lines.at(-1)?.pmEnd ?? 1
}

/**
 * Position after moving one word to the left (Ctrl+←).
 * Skips spaces then the word to the left. Returns para.pmStart+1 at start of paragraph.
 */
export function prevWordPos(layout: DocumentLayout, pos: number): number {
  for (const para of layout.paragraphs) {
    if (pos < para.pmStart + 1 || pos > para.pmEnd) continue

    const { text: fullText, pmPosOf } = paraFlatText(para)

    const offset = pos - (para.pmStart + 1)
    const isW    = (i: number) => i >= 0 && i < fullText.length && /\w/.test(fullText[i])

    let i = offset
    while (i > 0 && !isW(i - 1)) i--   // skip spaces left
    while (i > 0 && isW(i - 1))  i--   // skip word left

    return i === 0 ? para.pmStart + 1 : pmPosOf[i]
  }
  return layout.paragraphs[0]?.lines[0]?.pmStart ?? 1
}

/**
 * Position after moving one word to the right (Ctrl+→).
 * Skips the current word then any trailing spaces. Returns para.pmEnd at end of paragraph.
 */
export function nextWordPos(layout: DocumentLayout, pos: number): number {
  for (const para of layout.paragraphs) {
    if (pos < para.pmStart + 1 || pos > para.pmEnd) continue

    const { text: fullText, pmPosOf } = paraFlatText(para)

    const offset = pos - (para.pmStart + 1)
    const isW    = (i: number) => i >= 0 && i < fullText.length && /\w/.test(fullText[i])

    let i = offset
    while (isW(i))  i++   // skip current word right
    while (i < fullText.length && !isW(i)) i++   // skip spaces right

    return i >= fullText.length ? para.pmEnd : pmPosOf[i]
  }
  return layout.paragraphs.at(-1)?.lines.at(-1)?.pmEnd ?? 1
}

// ── Split helper ──────────────────────────────────────────────────────────────

/**
 * Given the layout for a full page doc, return indices of fitting / overflow
 * top-level nodes (suitable for slicing doc.content).
 */
// ── Pagination (modèle unique → bandes de page) ────────────────────────────────
// Découpe un layout CONTINU (issu d'un seul document) en pages : chaque page est
// un sous-layout dont les lignes ont un y LOCAL (relatif au haut de la page) mais
// conservent leurs positions ProseMirror GLOBALES (pmStart/pmEnd). Ainsi
// renderDocument / selectionRects / posToCoords / coordsToPos fonctionnent par page
// avec des coordonnées locales tout en parlant en positions globales du document.
export interface PageLayout {
  layout: DocumentLayout
  startY: number   // y global (px) du haut du contenu de cette page
  height: number   // hauteur de la zone de contenu (px)
  secIdx: number   // section à laquelle appartient la page (géométrie)
  // Endnotes typeset on this page (the end-of-document block): vertical boxes in
  // the LOCAL content-area coords (same space as LayoutLine.y), for click → edit.
  // Absent = no endnote on this page.
  endnotes?: PageEndnoteBox[]
}

// Box of one endnote painted on a page (page-local content-area coords). It spans
// the whole content width, like the block itself.
export interface PageEndnoteBox { n: number; text: string; pos: number; y0: number; y1: number }

// Reconstruit les LayoutParagraph d'une page à partir d'une tranche de lignes,
// en ramenant les y au repère local de la page (0 = haut du contenu).
function rebuildPageParas(
  taken: Array<{ para: LayoutParagraph; line: LayoutLine }>,
  startY: number,
  xShift = 0,
  colW = 0,
  multiCol = false,
  contentH = Infinity,
): LayoutParagraph[] {
  const paras: LayoutParagraph[] = []
  // Paragraphe SOURCE et DERNIÈRE ligne source reprise, par fragment construit —
  // sert à ne garder le numéro de table des matières que sur le fragment qui porte
  // vraiment la fin de l'entrée (cf. plus bas).
  const srcOf: LayoutParagraph[] = []
  const srcLastLine: LayoutLine[] = []
  let curSrc: LayoutParagraph | null = null
  let cur: LayoutParagraph | null = null
  for (const { para, line } of taken) {
    let shifted: LayoutLine = { ...line, y: line.y - startY, baseline: line.baseline - startY }
    // Multi-colonnes : décaler horizontalement (clone des spans) + bornes de colonne.
    if (multiCol) {
      shifted.spans = line.spans.map(s => ({ ...s, x: s.x + xShift }))
      shifted.cellX = (line.cellX ?? 0) + xShift
      shifted.cellW = line.cellW ?? colW
      if (line.caretX !== undefined) shifted.caretX = line.caretX + xShift
      if (line.image) shifted.image = { ...line.image, x: line.image.x + xShift }
      if (line.dropCap) shifted.dropCap = { ...line.dropCap, x: line.dropCap.x + xShift }
    }
    if (cur && curSrc === para) {
      cur.lines.push(shifted)
      srcLastLine[srcLastLine.length - 1] = line
    } else {
      // Géométrie de tableau : ramener les rectangles de cellule au repère local de page (+ décalage colonne).
      // Cellules totalement HORS page exclues : sans cela, les rangées des autres
      // pages d'un tableau scindé peignaient bordures/fonds dans les marges.
      const table = para.table
        ? { cells: para.table.cells.map(c => ({ ...c, x: c.x + xShift, y: c.y - startY })).filter(c => c.y + c.h > 0.5 && c.y < contentH - 0.5),
            style: para.table.style, accent: para.table.accent,
            colX: para.table.colX?.map(x => x + xShift), rowY: para.table.rowY?.map(y => y - startY),
            headerRepeat: para.table.headerRepeat, headerRows: para.table.headerRows, borderColor: para.table.borderColor, borderWidth: para.table.borderWidth, borderStyle: para.table.borderStyle }
        : undefined
      cur = { lines: [shifted], y: para.y - startY, height: para.height, pmStart: para.pmStart, pmEnd: para.pmEnd, docIdx: para.docIdx, secIdx: para.secIdx, breakBefore: para.breakBefore, table, tocPage: para.tocPage, tocLeader: para.tocLeader }
      curSrc = para
      paras.push(cur)
      srcOf.push(para)
      srcLastLine.push(line)
    }
  }
  // Entrée de table des matières SCINDÉE entre deux pages : le numéro de page et les
  // points de suite se posent sur la dernière ligne de l'entrée — donc uniquement sur
  // le fragment qui la contient. Sans ce filtre, chaque fragment recevait son propre
  // numéro (un numéro parasite en bas de la page précédente).
  for (let i = 0; i < paras.length; i++) {
    const p = paras[i]
    if (p.tocPage == null && !p.tocLeader) continue
    const src = srcOf[i]
    if (srcLastLine[i] !== src.lines[src.lines.length - 1]) { p.tocPage = undefined; p.tocLeader = undefined }
  }
  return paras
}

interface SectionPageGeom { contentH: number; columns: number; colW: number; colGap: number }

// « Enchaînements » (Word) : hauteur de la grappe SOLIDAIRE débutant à `start` —
// toutes les lignes du paragraphe courant + (si « solidaire du suivant ») la 1ʳᵉ
// ligne du paragraphe suivant de la même section. Sert à décider si la grappe tient
// dans l'espace restant ; sinon on la repousse en bloc à la colonne/page suivante.
function keepRunHeight(refs: Array<{ para: LayoutParagraph; line: LayoutLine }>, start: number): number {
  const p = refs[start].para
  let h = 0, j = start
  while (j < refs.length && refs[j].para === p) { h += refs[j].line.height; j++ }
  if (p.keepNext && j < refs.length && refs[j].para.secIdx === p.secIdx) h += refs[j].line.height
  return h
}

// Faut-il repousser la grappe solidaire commençant à `i` (espace restant `remaining`,
// hauteur de page `contentH`) ? Oui si elle dépasse mais tiendrait sur une page neuve
// (sinon on la laisse couler pour éviter une page vide / boucle infinie).
function shouldKeepBreak(refs: Array<{ para: LayoutParagraph; line: LayoutLine }>, i: number, remaining: number, contentH: number): boolean {
  const p = refs[i].para
  if (!p.keepLines && !p.keepNext) return false
  const need = keepRunHeight(refs, i)
  return need > remaining && need <= contentH
}

// Réplique de la rangée d'en-tête d'un tableau (repère LOCAL page, y ∈ [0, headerH]) :
// lignes de texte clonées marquées `phantom` + géométrie des cellules r=0 — dessinée
// en haut des pages de continuation quand `headerRepeat` est actif.
function buildRepeatedHeader(para: LayoutParagraph, headerH: number): LayoutParagraph {
  const t = para.table!
  const rY0 = t.rowY![0]
  const lines: LayoutLine[] = []
  for (const ln of para.lines) {
    if (ln.image) continue
    if (ln.y < rY0 - 0.5 || ln.y + ln.height > rY0 + headerH + 6) continue
    lines.push({ ...ln, y: ln.y - rY0, baseline: ln.baseline - rY0, phantom: true, spans: ln.spans.map(sp => ({ ...sp })) })
  }
  const nHdr = Math.max(1, t.headerRows || 1)
  const cells = (t.cells ?? []).filter(c => c.r < nHdr).map(c => ({ ...c, y: c.y - rY0 }))
  return {
    lines, y: 0, height: headerH, pmStart: para.pmStart, pmEnd: para.pmStart,
    docIdx: para.docIdx, secIdx: para.secIdx, breakBefore: false,
    table: { cells, style: t.style, accent: t.accent, colX: t.colX, rowY: [0, headerH], borderColor: t.borderColor, borderWidth: t.borderWidth, borderStyle: t.borderStyle },
  }
}

// Pagination multi-sections + multi-colonnes : par page, on remplit `columns`
// Fragment de tableau SANS lignes de texte : porte uniquement la géométrie
// (bordures, fonds de cellules) d'une tranche de tableau — utilisé pour les
// pages traversées par le VIDE d'une grande ligne de tableau (ex. ligne
// redimensionnée bien au-delà de son contenu). Sans cela, la pagination par
// lignes écrase l'espace vide : la bordure basse restait collée à la couture
// de page, impossible à glisser d'une page à l'autre.
function tableVoidFragment(para: LayoutParagraph, startY: number, contentH: number): LayoutParagraph {
  const t = para.table!
  const table: LayoutTable = {
    cells: t.cells.map(c => ({ ...c, y: c.y - startY })).filter(c => c.y + c.h > 0.5 && c.y < contentH - 0.5),
    style: t.style, accent: t.accent, colX: t.colX, rowY: t.rowY?.map(y => y - startY),
    headerRepeat: t.headerRepeat, headerRows: t.headerRows, borderColor: t.borderColor, borderWidth: t.borderWidth, borderStyle: t.borderStyle,
  }
  return { lines: [], y: para.y - startY, height: para.height, pmStart: para.pmStart, pmEnd: para.pmEnd,
           docIdx: para.docIdx, secIdx: para.secIdx, breakBefore: para.breakBefore, table,
           tocPage: para.tocPage, tocLeader: para.tocLeader }
}

// colonnes de hauteur `contentH` (le texte coule colonne 1 → 2 → 3 → page suivante).
// Chaque changement de section ou saut de page force une nouvelle page.
export function paginateMulti(layout: DocumentLayout, geoms: SectionPageGeom[]): PageLayout[] {
  const geomFor = (s: number): SectionPageGeom =>
    geoms[s] ?? geoms[geoms.length - 1] ?? geoms[0] ?? { contentH: 0, columns: 1, colW: layout.contentW, colGap: 0 }
  type Ref = { para: LayoutParagraph; line: LayoutLine }
  const refs: Ref[] = []
  for (const para of layout.paragraphs) for (const line of para.lines) refs.push({ para, line })

  if (refs.length === 0) {
    return [{ layout: { paragraphs: [], totalHeight: 0, contentW: layout.contentW }, startY: 0, height: geomFor(0).contentH, secIdx: 0 }]
  }

  const pages: PageLayout[] = []
  let i = 0
  while (i < refs.length) {
    const pageSec  = refs[i].para.secIdx
    const g        = geomFor(pageSec)
    const cols     = Math.max(1, g.columns)
    const contentH = g.contentH
    let pageStartY = refs[i].line.y
    // VIDE d'un tableau à cheval sur des pages : la page précédente s'est
    // terminée DANS un tableau (la prochaine ligne n'est pas la première de son
    // paragraphe) et la prochaine ligne est plus bas que la fin de page → la
    // pagination par lignes sauterait l'espace vide (grande ligne sans texte).
    // On pagine en CONTINU : la page suivante démarre à la fin de la
    // précédente, avec des pages intermédiaires ne portant que la géométrie du
    // tableau si le vide dépasse une page entière.
    if (pages.length && cols === 1 && contentH > 50) {
      const prev = pages[pages.length - 1]
      const prevEnd = prev.startY + prev.height
      const para = refs[i].para
      if (para.table && refs[i].line !== para.lines[0] && para.secIdx === prev.secIdx && refs[i].line.y > prevEnd + 1) {
        pageStartY = prevEnd
        while (refs[i].line.y + refs[i].line.height - pageStartY > contentH) {
          pages.push({
            layout: { paragraphs: [tableVoidFragment(para, pageStartY, contentH)], totalHeight: contentH, contentW: layout.contentW },
            startY: pageStartY, height: contentH, secIdx: pageSec,
          })
          pageStartY += contentH
        }
      }
    }
    const pageParas: LayoutParagraph[] = []
    let stop = false

    for (let col = 0; col < cols && !stop; col++) {
      if (i >= refs.length || refs[i].para.secIdx !== pageSec) break
      const colStartY = col === 0 ? pageStartY : refs[i].line.y
      // ── Répétition de la ligne d'en-tête (Word) : la page démarre DANS un tableau
      // (continuation) dont l'attr headerRepeat est actif → réserver la hauteur de la
      // rangée 0 en haut, décaler le contenu d'autant, et répliquer l'en-tête (lignes
      // « phantom » + géométrie des cellules r=0). Mono-colonne uniquement.
      let hdrH = 0
      const firstRef = refs[i]
      const ft = firstRef.para.table
      // `headerRows` = nombre de rangées épinglées (1 = « répéter la ligne d'en-tête »
      // de Word ; N = « épingler jusqu'à cette ligne » façon Google Docs).
      const nHdr = Math.max(0, ft?.headerRows ?? (ft?.headerRepeat ? 1 : 0))
      if (cols === 1 && nHdr > 0 && ft?.rowY && ft.rowY.length > nHdr && firstRef.para.lines[0] !== firstRef.line) {
        const hh = ft.rowY[nHdr] - ft.rowY[0]
        if (hh > 0 && hh < contentH * 0.5) hdrH = hh
      }
      const capacity = contentH - hdrH
      const taken: Ref[] = []
      let lastPara: LayoutParagraph | null = null
      while (i < refs.length) {
        if (refs[i].para.secIdx !== pageSec) { stop = true; break }
        const startingPara = refs[i].para !== lastPara
        if (taken.length > 0 && startingPara && refs[i].para.breakBefore) { stop = true; break }
        // Enchaînements : lignes/paragraphe solidaires → repousser la grappe entière.
        if (taken.length > 0 && startingPara && shouldKeepBreak(refs, i, capacity - (refs[i].line.y - colStartY), capacity)) break
        const ln = refs[i].line
        if (taken.length > 0 && (ln.y + ln.height - colStartY) > capacity) break
        lastPara = refs[i].para
        taken.push(refs[i]); i++
      }
      const xShift = col * (g.colW + g.colGap)
      pageParas.push(...rebuildPageParas(taken, colStartY - hdrH, xShift, g.colW, cols > 1, contentH))
      if (hdrH > 0) pageParas.unshift(buildRepeatedHeader(firstRef.para, hdrH))
    }

    pages.push({
      layout: { paragraphs: pageParas, totalHeight: contentH, contentW: layout.contentW },
      startY: pageStartY, height: contentH, secIdx: pageSec,
    })
  }
  // ENDNOTES: one single block after the last content of the document (extra pages
  // created when needed) — cf. appendEndnotePages.
  appendEndnotePages(pages, layout, s => { const gg = geomFor(s); return { contentH: gg.contentH, colW: gg.colW } })
  return pages
}

// ── Objet flottant à cheval sur un saut de page : rendu SCINDÉ ────────────────
// La partie qui déborde du bas (ou du haut) de la bande de contenu d'une page est
// ROGNÉE au bord, et RE-DESSINÉE en continuation sur la page voisine : clone de la
// ligne-image, re-basé dans le repère de la page cible, avec fenêtre de rognage
// complémentaire (imgClipTop/imgClipBottom). Un objet plus haut qu'une page se
// scinde en chaîne ; une page vide est ajoutée s'il déborde de la dernière page.
// Mutations en place ; à appeler juste après paginateMulti.
export function splitFloatingImagesAcrossPages(pgs: PageLayout[], contentHFor: (secIdx: number) => number): void {
  for (let i = 0; i < pgs.length; i++) {
    const pg = pgs[i]
    // `paragraphs.length` relu à chaque tour : les clones ajoutés sur les pages
    // SUIVANTES seront traités quand la boucle externe les atteindra (chaîne).
    for (let pi = 0; pi < pg.layout.paragraphs.length; pi++) {
      const para = pg.layout.paragraphs[pi]
      for (const ln of para.lines) {
        const im = ln.image
        if (!im || !isFloatingWrap(im.wrap)) continue
        const ab = imgAABB(im.w, im.h, im.rotation || 0)
        const topL = ln.y + (im.wrapY || 0) + im.h / 2 - ab.h / 2   // haut AABB, repère local page
        const botL = topL + ab.h
        const clonePara = (dst: PageLayout, clone: LayoutLine) => {
          dst.layout.paragraphs.push({ lines: [clone], y: clone.y, height: para.height, pmStart: para.pmStart, pmEnd: para.pmEnd, docIdx: para.docIdx, secIdx: para.secIdx, breakBefore: false })
        }
        // ── Déborde en HAUT (objet remonté au-dessus de la bande) : rogner ici,
        // dessiner la partie cachée en bas de la page précédente.
        if (topL < -0.5 && !ln.imgSplitClone && i > 0) {
          ln.imgClipTop = Math.max(ln.imgClipTop ?? -Infinity, 0)
          const prev = pgs[i - 1]
          const cy = pg.startY + ln.y - prev.startY
          clonePara(prev, {
            ...ln, y: cy, baseline: cy, imgSplitClone: 'up',
            imgClipTop: undefined, imgClipBottom: Math.min(prev.height, pg.startY - prev.startY),
          })
        }
        // ── Déborde en BAS : rogner ici, dessiner la partie cachée en haut de la
        // page suivante (créée vide si l'objet dépasse la dernière page).
        if (botL > pg.height + 0.5 && ln.imgSplitClone !== 'up') {
          ln.imgClipBottom = Math.min(ln.imgClipBottom ?? Infinity, pg.height)
          if (i + 1 >= pgs.length) {
            const h = Math.max(1, contentHFor(pg.secIdx))
            pgs.push({ layout: { paragraphs: [], totalHeight: h, contentW: pg.layout.contentW }, startY: pg.startY + pg.height, height: h, secIdx: pg.secIdx })
          }
          const nxt = pgs[i + 1]
          const cy = pg.startY + ln.y - nxt.startY
          clonePara(nxt, {
            ...ln, y: cy, baseline: cy, imgSplitClone: 'down',
            imgClipBottom: undefined, imgClipTop: Math.max(0, pg.startY + pg.height - nxt.startY),
          })
        }
      }
    }
  }
}

export function paginate(layout: DocumentLayout, contentH: number): PageLayout[] {
  type Ref = { para: LayoutParagraph; line: LayoutLine }
  const refs: Ref[] = []
  for (const para of layout.paragraphs) for (const line of para.lines) refs.push({ para, line })

  if (refs.length === 0) {
    return [{ layout: { paragraphs: [], totalHeight: 0, contentW: layout.contentW }, startY: 0, height: contentH, secIdx: 0 }]
  }

  const pages: PageLayout[] = []
  let i = 0
  while (i < refs.length) {
    const startY = refs[i].line.y
    const taken: Ref[] = []
    let lastPara: LayoutParagraph | null = null
    while (i < refs.length) {
      const startingPara = refs[i].para !== lastPara
      // Saut de page avant + enchaînements (lignes/paragraphe solidaires).
      if (taken.length > 0 && startingPara && refs[i].para.breakBefore) break
      if (taken.length > 0 && startingPara && shouldKeepBreak(refs, i, contentH - ((refs[i].line.y) - startY), contentH)) break
      const ln = refs[i].line
      const bottomRel = (ln.y + ln.height) - startY
      // une ligne ne se coupe pas : si elle dépasse et que la page n'est pas vide → page suivante
      if (taken.length > 0 && bottomRel > contentH) break
      lastPara = refs[i].para
      taken.push(refs[i]); i++
    }
    pages.push({
      layout: { paragraphs: rebuildPageParas(taken, startY), totalHeight: contentH, contentW: layout.contentW },
      startY,
      height: contentH,
      secIdx: 0,
    })
  }
  appendEndnotePages(pages, layout, () => ({ contentH, colW: layout.contentW }))
  return pages
}

export function splitAtHeight(
  layout: DocumentLayout,
  contentH: number,
): { fitUntil: number } {
  // fitUntil is a doc.content[] index (via docIdx), not a layout paragraph index.
  // List nodes expand into multiple layout paragraphs sharing the same docIdx,
  // so we track the maximum docIdx whose entire block fits within contentH.
  let lastFitDocIdx = -1
  for (const p of layout.paragraphs) {
    const lastLine = p.lines.at(-1)
    if (!lastLine) { lastFitDocIdx = Math.max(lastFitDocIdx, p.docIdx); continue }
    if (lastLine.y + lastLine.height <= contentH) {
      lastFitDocIdx = Math.max(lastFitDocIdx, p.docIdx)
    } else {
      // This block overflows — stop (docIdx is monotonically non-decreasing)
      break
    }
  }
  return { fitUntil: Math.max(0, lastFitDocIdx) }
}
