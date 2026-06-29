/**
 * Canvas-based text layout and rendering engine.
 * All layout coordinates are in unscaled CSS px.
 * The caller applies zoom + DPR scaling when rendering.
 */

import type { JSONContent } from '@tiptap/react'

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
}

// Géométrie d'un tableau pour le tracé des bordures (coords zone de contenu).
export interface LayoutTableCell { x: number; y: number; w: number; h: number; bg?: string; r: number; c: number; colspan: number; rowspan: number }
// colX/rowY = positions des bordures (px, repère contenu) : colX[colCount+1], rowY[rows+1].
// Sert au placement des poignées de redimensionnement.
export interface LayoutTable { cells: LayoutTableCell[]; style?: string; accent?: string; colX?: number[]; rowY?: number[] }

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
}

export interface DocumentLayout {
  paragraphs:  LayoutParagraph[]
  totalHeight: number   // CSS px
  contentW:    number   // largeur de la zone de contenu (px) — pour la sélection pleine ligne
}

export interface CursorMetrics {
  x:           number   // CSS px from content-area left
  y:           number   // CSS px from content-area top (top of line)
  height:      number   // CSS px
  italicAngle: number   // radians — 0 for upright, ~+0.13 for italic (CW = leans right)
  rot?:        90 | 270 // caret d'une cellule à texte vertical (barre tournée ±90°)
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
}

interface RenderParagraph {
  spans:       RenderSpan[]
  align:       'left' | 'center' | 'right' | 'justify'
  indent:      number    // CSS px (left indent : listes + retrait gauche paragraphe)
  firstLineIndent?: number  // CSS px : offset de la 1ʳᵉ ligne vs `indent` (négatif = suspendu)
  indentRight?:     number  // CSS px : retrait droit (réduit la largeur disponible)
  tabStops?:        number[] // taquets de tabulation perso (px depuis la marge gauche)
  marker?:     string    // '•' or '1.' etc.
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
  image?:      { src: string; width: number; height: number; align: 'left' | 'center' | 'right'; rotation: number; wrap?: string; wrapX?: number; wrapY?: number; alt?: string; tbFill?: string; tbStroke?: string; wrapSide?: string; wrapDistT?: number; wrapDistB?: number; wrapDistL?: number; wrapDistR?: number }   // bloc-image (0 = taille naturelle) + habillage + alt + couleurs zone-texte + côté/distances
  table?:      RenderTable   // tableau (lignes/cellules)
}

// ── Structures de tableau (parse) ───────────────────────────────────────────
interface RenderTableCell { paras: RenderParagraph[]; colspan: number; rowspan: number; merged: boolean; cellBg?: string; vAlign?: 'top' | 'center' | 'bottom'; dir?: 0 | 90 | 270 }
interface RenderTableRow  { cells: RenderTableCell[] }
interface RenderTable     { rows: RenderTableRow[]; colCount: number; style: string; accent?: string; colWidths?: number[]; rowHeights?: number[]; align?: 'left' | 'center' | 'right'; indent?: number; rowHeightModes?: Array<'atleast' | 'exactly'> }

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_PT   = 11
const DEFAULT_FAM  = 'Arial'
const DEFAULT_CLR  = '#202124'
const PT_PX        = 96 / 72      // 1 pt = 1.3333 CSS px at 96 dpi
const LH_RATIO     = 1.15         // line-height multiplier (Google Docs default)
const LIST_INDENT  = 32           // CSS px per nesting level

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

// ── Font helpers ──────────────────────────────────────────────────────────────

const SCRIPT_SCALE = 0.66   // taille relative de l'exposant/indice (comme Google)

function fontStr(m: TextMark): string {
  let px  = (m.fontSize ?? DEFAULT_PT) * PT_PX
  if (m.script) px *= SCRIPT_SCALE
  const wt  = m.bold   ? 'bold'   : 'normal'
  const st  = m.italic ? 'italic' : 'normal'
  const fam = m.fontFamily ?? DEFAULT_FAM
  return `${st} ${wt} ${px}px ${fam}, sans-serif`
}

interface LineMetrics { ascent: number; descent: number; height: number }

// ── Caches de mesure ──────────────────────────────────────────────────────────
// layoutDocument re-mesure tout le document à chaque frappe ; mots et polices se
// répètent énormément. On mémoïse les largeurs (clé = police+texte) et les
// métriques de ligne (clé = police) pour éviter des milliers d'appels measureText.
const _widthCache = new Map<string, number>()
const _lineMetricsCache = new Map<string, LineMetrics>()
const WIDTH_CACHE_MAX = 100_000

/// À appeler si les polices personnalisées changent (largeurs obsolètes).
export function clearMeasureCaches(): void {
  _widthCache.clear()
  _lineMetricsCache.clear()
  _tbLayoutCache.clear()
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
  const key  = font + ' ' + text
  const hit  = _widthCache.get(key)
  if (hit !== undefined) return hit
  const c = mc()
  c.font = font
  const w = c.measureText(text).width
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
      case 'textStyle':
        if (mark.attrs?.fontSize)   { const n = parseFloat(mark.attrs.fontSize);  if (!isNaN(n)) m.fontSize = n }
        if (mark.attrs?.color)      m.color      = mark.attrs.color
        if (mark.attrs?.fontFamily) m.fontFamily = mark.attrs.fontFamily
        break
    }
  }
  return m
}

function nodeSize(n: JSONContent): number {
  if (n.type === 'text') return (n.text ?? '').length
  if (n.type === 'hardBreak') return 1
  if (n.type === 'sectionBreak' || n.type === 'pageBreak' || n.type === 'image') return 1   // feuilles (atom) : taille PM = 1
  let sz = 2
  for (const c of n.content ?? []) sz += nodeSize(c)
  return sz
}

function parseDoc(doc: JSONContent): RenderParagraph[] {
  const result: RenderParagraph[] = []
  let pos = 0  // ProseMirror: pos 0 is before the doc's first child opening tag
  let secIdx = 0       // section courante : +1 à chaque sectionBreak rencontré
  let pendingBreak = false  // saut de page en attente (à reporter sur le prochain bloc)

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
          cells.push({ paras: cellParas, colspan, rowspan: Math.max(1, Number(ca.rowspan) || 1), merged: !!ca.merged, cellBg: ca.cellBg != null ? String(ca.cellBg) : undefined, vAlign: (ca.cellVAlign as 'top' | 'center' | 'bottom') || 'top', dir: (Number(ca.cellDir) as 0 | 90 | 270) || 0 })
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
        table: { rows, colCount, style: String(ta.tableStyle || 'grid'), accent: ta.accent != null ? String(ta.accent) : undefined, colWidths: Array.isArray(ta.colWidths) ? (ta.colWidths as number[]) : undefined, rowHeights: Array.isArray(ta.rowHeights) ? (ta.rowHeights as number[]) : undefined, align: (ta.tableAlign as 'left' | 'center' | 'right') || 'left', indent: Number(ta.tableIndent) || 0, rowHeightModes: Array.isArray(ta.rowHeightModes) ? (ta.rowHeightModes as Array<'atleast' | 'exactly'>) : undefined },
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
          const src = resolveImageSrc(a as Record<string, unknown>)
          const w = Math.max(1, Number(a.width) || 0)
          const h = Math.max(1, Number(a.height) || 0)
          spans.push({ text: '​', marks: extractMarks(inline), pmPos: pos,
            img: { src, w, h, alt: a.alt != null ? String(a.alt) : undefined, rot: Number(a.rotation) || 0 } })
          pos += 1
        } else {
          pos += nodeSize(inline)
        }
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
        indent: depth * LIST_INDENT + indentLevel * LIST_INDENT + indL,
        firstLineIndent: indF || undefined,
        indentRight: indR || undefined,
        tabStops: (Array.isArray(tabsAttr) && tabsAttr.length) ? tabsAttr : undefined,
        marker: listCtx ? (listCtx.type === 'bullet' ? '•' : `${listCtx.idx}.`) : undefined,
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
    const isHeading = node.type === 'heading'
    const lvl = isHeading ? (Number(node.attrs?.level) || 1) : 0
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
    if (isHeading && node.attrs?.collapsed) hideLevel = lvl
  }
  return result
}

// ── Layout engine ─────────────────────────────────────────────────────────────

// Cœur partagé : pose les paragraphes en empilant les lignes ; la largeur de
// chaque paragraphe est choisie par section via `widthFor(secIdx)`.
function layoutParagraphs(
  paragraphs: RenderParagraph[],
  widthFor: (secIdx: number) => number,
): { out: LayoutParagraph[]; totalHeight: number } {
  const out: LayoutParagraph[] = []
  let y = 0

  // Flottants « carré » rencontrés : zones rectangulaires (y global) que le texte
  // suivant doit contourner. side = côté occupé par l'objet (left/right).
  const squares: Array<{ y0: number; y1: number; xEdge: number; side: 'left' | 'right'; gap: number }> = []
  // Largeur/décalage disponibles pour une ligne à [yTop, yTop+h] (global).
  // La marge objet↔texte est portée par chaque carré (`gap` = distance Word).
  const exclusionAt = (cw: number) => (yTop: number, h: number) => {
    let left = 0, right = cw
    for (const s of squares) {
      if (yTop + h <= s.y0 || yTop >= s.y1) continue   // pas de chevauchement vertical
      if (s.side === 'left')  left  = Math.max(left,  s.xEdge + s.gap)
      else                    right = Math.min(right, s.xEdge - s.gap)
    }
    return { left, width: Math.max(40, right - left) }
  }

  for (let pIdx = 0; pIdx < paragraphs.length; pIdx++) {
    const para = paragraphs[pIdx]
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
      const { lines, table, height } = layoutTable(para.table, widthFor(para.secIdx), pY)
      out.push({
        lines, y: pY - spaceBefore,
        height: height + spaceBefore + spaceAfter,
        pmStart: para.pmStart, pmEnd: para.pmEnd, docIdx: para.docIdx,
        secIdx: para.secIdx, breakBefore: para.breakBefore, table,
      })
      y = pY + height + spaceAfter
      continue
    }

    const cw = widthFor(para.secIdx)
    // layoutParagraph passe un yRel (relatif au paragraphe) → on le ramène au y global (pY + yRel).
    const exGlobal = exclusionAt(cw)
    const lines = layoutParagraph(para, cw, (yRel, h) => exGlobal(pY + yRel, h))
    // Enregistre un flottant carré (l'image ne réserve pas de hauteur → la zone
    // d'exclusion démarre à pY pour le TEXTE qui suit, sur `image.h`).
    const imgLine = lines.find(l => l.image && l.image.wrap === 'square')
    if (imgLine && imgLine.image) {
      const im = imgLine.image
      // Côté où le texte s'écoule : auto (selon la position) ou forcé par `wrapSide`
      // (Word : « Seulement à gauche » = texte à gauche ⇒ l'objet exclut sa DROITE).
      const auto = im.x + im.w / 2 > cw / 2
      const onRight = im.wrapSide === 'left' ? true : im.wrapSide === 'right' ? false : auto
      // Distances objet↔texte (Word « Distance du texte »).
      const dL = im.wrapDistL ?? 10, dR = im.wrapDistR ?? 10
      const dT = im.wrapDistT ?? 0,  dB = im.wrapDistB ?? 0
      squares.push({
        y0: pY - dT, y1: pY + im.h + dB,
        xEdge: onRight ? im.x : im.x + im.w,
        side: onRight ? 'right' : 'left',
        gap: onRight ? dL : dR,
      })
    }
    for (const line of lines) {
      line.y = y
      // Répartir l'interligne (leading) : moitié au-dessus, moitié en dessous du texte
      // (centrage vertical, façon Google Docs). `line.height` inclut déjà l'interligne.
      // Leading = (hauteur de ligne − hauteur naturelle)/2 — vaut pour TOUS les modes
      // (multiple/au moins/exactement) ; repli au multiplicateur si naturalH inconnue.
      const topLead = line.naturalH != null
        ? (line.height - line.naturalH) / 2
        : (line.height * (1 - 1 / para.lineSpacing)) / 2
      line.baseline = y + topLead + line.ascent
      y += line.height
    }

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
    })
  }

  return { out, totalHeight: y }
}

// Mise en page d'un tableau : colonnes égales, texte des cellules réagencé à la
// largeur de colonne, hauteur de ligne = max des cellules. Renvoie les lignes de
// toutes les cellules (avec x absolu et y à partir de `yTop`), la géométrie des
// bordures et la hauteur totale. Les lignes portent cellX/cellW pour coordsToPos.
// Marge intérieure de cellule : horizontale généreuse (lisibilité), VERTICALE faible
// (Word utilise ~0 en haut/bas) → des hauteurs de ligne compactes, fidèles à Word.
const CELL_PAD_X = 6
const CELL_PAD_Y = 2
const MIN_ROW_H = 22
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

  // Largeurs de colonnes : explicites (réglées par glisser, capées à la largeur de
  // contenu) ou uniformes. colX = bornes cumulées (colCount+1 valeurs).
  let widths: number[]
  if (table.colWidths && table.colWidths.length === colCount && table.colWidths.every(w => w > 4)) {
    widths = table.colWidths.slice()
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
      const innerW = cellW(c, cspan) - 2 * CELL_PAD_X
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
  const contentH = (p: Placed) => (p.vert ? p.wc : p.cl.totalHeight) + 2 * CELL_PAD_Y
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
    cells.push({ x, y: cellTop, w, h, bg, r: p.r, c: p.c0, colspan: p.cspan, rowspan: p.rspan })
    const va = p.cell.vAlign || 'top'
    if (p.vert) {
      // Texte vertical : lignes gardées en LOCAL ; on calcule la transformation
      // (rotation ±90° + translation) pour le rendu et le mappage de position.
      const availW = w - 2 * CELL_PAD_X, availH = h - 2 * CELL_PAD_Y
      const blockLeft = x + CELL_PAD_X + Math.max(0, (availW - p.hc) / 2)
      const vOff = va === 'center' ? Math.max(0, (availH - p.wc) / 2) : va === 'bottom' ? Math.max(0, availH - p.wc) : 0
      const blockTop = cellTop + CELL_PAD_Y + vOff
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
      const slack = Math.max(0, (h - 2 * CELL_PAD_Y) - p.cl.totalHeight)
      const vOff = va === 'center' ? slack / 2 : va === 'bottom' ? slack : 0
      for (const para of p.cl.out) for (const ln of para.lines) {
        for (const sp of ln.spans) sp.x += x + CELL_PAD_X
        if (ln.caretX !== undefined) ln.caretX += x + CELL_PAD_X
        ln.y        += cellTop + CELL_PAD_Y + vOff
        ln.baseline += cellTop + CELL_PAD_Y + vOff
        ln.cellX = x
        ln.cellW = w
        lines.push(ln)
      }
    }
  }

  return { lines, table: { cells, style, accent, colX: colXoff, rowY }, height: acc }
}

export function layoutDocument(doc: JSONContent, contentW: number): DocumentLayout {
  const { out, totalHeight } = layoutParagraphs(parseDoc(doc), () => contentW)
  return { paragraphs: out, totalHeight, contentW }
}

// Variante multi-sections : une largeur de contenu par section (indexée par secIdx).
export function layoutDocumentMulti(doc: JSONContent, widths: number[]): DocumentLayout {
  const widthFor = (s: number) => widths[s] ?? widths[widths.length - 1] ?? widths[0]
  const { out, totalHeight } = layoutParagraphs(parseDoc(doc), widthFor)
  return { paragraphs: out, totalHeight, contentW: widths[0] }
}

// Greedy word-wrap with full justification.
// `exclusion(yRel, h)` → { left, width } disponibles pour une ligne à l'ordonnée
// RELATIVE yRel (px depuis le haut du paragraphe) ; sert à l'habillage carré.
function layoutParagraph(
  para: RenderParagraph,
  contentW: number,
  exclusion?: (yRel: number, h: number) => { left: number; width: number },
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
    // behind/front/square ne réservent PAS la hauteur de l'image dans le flux
    // (le texte coule par-dessus/dessous ou À CÔTÉ pour le carré).
    const floating = wrap === 'behind' || wrap === 'front' || wrap === 'square'
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

  interface Token { text: string; marks: TextMark; width: number; pmPos: number; isSpace: boolean; isTab?: boolean; img?: { src: string; w: number; h: number; alt?: string; rot?: number } }

  // Tokenise into words + whitespace, en isolant CHAQUE tabulation (`\t`) comme un token
  // propre (largeur calculée à la pose, = distance jusqu'au prochain taquet).
  const tokens: Token[] = []
  for (const span of para.spans) {
    // Image inline = UN token de la largeur (boîte tournée) de l'image (insécable).
    if (span.img) {
      tokens.push({ text: span.text, marks: span.marks, width: imgAABB(span.img.w, span.img.h, span.img.rot).w, pmPos: span.pmPos, isSpace: false, img: span.img })
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

  // Largeur DISPONIBLE par ligne (varie quand un objet « carré » exclut une zone).
  // Sans exclusion, curLeft=indent et curAvail=avail → comportement inchangé.
  let lineYRel = 0
  const estH = lineH(lineMetrics({}).height).h
  let curLeft = para.indent + firstLine, curAvail = avail
  const startLine = () => {
    const ex = exclusion ? exclusion(lineYRel, estH) : { left: 0, width: contentW }
    // La 1ʳᵉ ligne du paragraphe porte le retrait « 1ʳᵉ ligne » (peut être négatif =
    // retrait suspendu) ; les lignes suivantes non. Le retrait droit réduit la largeur.
    const first = lines.length === 0 ? firstLine : 0
    curLeft  = ex.left + para.indent + first
    curAvail = Math.max(40, ex.width - para.indent - indentRight - first)
  }
  startLine()

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

    // List marker on first line
    if (para.marker && lines.length === 0) {
      const mText  = para.marker + ' '
      const mWidth = measureW(mText, {})
      spans.push({ text: para.marker, marks: {}, x: curLeft - mWidth, width: mWidth, pmPos: lStart })
    }

    let x = sx
    let lEnd = lStart
    for (let i = 0; i < lineToks.length; i++) {
      const t = lineToks[i]
      // Tabulation : avance jusqu'au prochain TAQUET (perso si défini, sinon grille par défaut).
      const w = t.isTab ? Math.max(2, nextTabStop(x) - x)
              : t.isSpace ? t.width + (i < trailStart ? extraSp : 0)
              : t.width
      spans.push({ text: t.text, marks: t.marks, x, width: w, pmPos: t.pmPos, img: t.img })
      x += w
      lEnd = t.pmPos + t.text.length
    }

    const h = lineH(maxH).h
    lines.push({ spans, y: 0, baseline: 0, height: h, naturalH: maxH, ascent: maxAsc, pmStart: lStart, pmEnd: lEnd })
    lStart  = lEnd
    lineToks = []
    lineW   = 0
    lineYRel += h
    startLine()
  }

  for (const tok of tokens) {
    // Skip leading whitespace on a WRAPPED line (continuation) : l'espace qui a
    // provoqué le retour ne se redessine pas au début de la ligne suivante. MAIS au
    // tout début du paragraphe (lines.length === 0 → 1ʳᵉ ligne), on GARDE les espaces
    // de début (contenu réel, façon Word) : sinon le caret placé après ces espaces
    // (ex. paragraphe vide où l'on tape Espace) n'appartient à aucune ligne et se
    // téléporte en fin de document.
    if (!lineToks.length && tok.isSpace && lines.length > 0) { lStart = tok.pmPos + tok.text.length; continue }

    if (lineW + tok.width <= curAvail + 0.5 || !lineToks.length) {
      lineToks.push(tok); lineW += tok.width
    } else {
      flush(false)
      if (tok.isSpace) { lStart = tok.pmPos + tok.text.length }
      else { lineToks = [tok]; lineW = tok.width }
    }
  }
  flush(true)

  // Paragraphe dont tous les tokens sont des espaces (ex: p(' ') ou p('   ')) :
  // flush() n'a rien produit car les espaces de début sont skippés et les espaces
  // de fin sont trimés. On émet une ligne vide pour que le curseur ait un endroit
  // valide, exactement comme pour un paragraphe structurellement vide.
  if (lines.length === 0) {
    const lm      = lineMetrics({})
    const innerPos = para.pmStart + 1
    lines.push({ spans: [], y: 0, baseline: 0, height: lineH(lm.height).h, naturalH: lm.height, ascent: lm.ascent, pmStart: innerPos, pmEnd: innerPos })
  }

  return lines
}

// ── Rendering ─────────────────────────────────────────────────────────────────

// ── Selection helpers ─────────────────────────────────────────────────────────

/** X coordinate of `pos` within its containing line, in content-area px. */
function xAtPosInLine(line: LayoutLine, pos: number): number {
  const first = line.spans[0]
  // `pos` peut tomber AVANT le premier span quand un espace de début a été
  // rogné au word-wrap (line.pmStart < first.pmPos). On le ramène au début
  // visuel de la ligne au lieu de retomber sur textEnd (sinon la sélection des
  // lignes enveloppées s'effondre à droite).
  if (first && pos <= first.pmPos) return first.x
  for (const span of line.spans) {
    const spanEnd = span.pmPos + span.text.length
    if (pos >= span.pmPos && pos <= spanEnd) {
      // Tabulation / image inline : la largeur du span EST l'avance (atom 1 pos) —
      // le caret après l'objet est à son bord droit, pas à 0 (measureW d'un ZWSP).
      if (span.text === '\t' || span.img) return span.x + (pos > span.pmPos ? span.width : 0)
      return span.x + measureW(span.text.slice(0, pos - span.pmPos), span.marks)
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
      if (line.pmEnd < from || line.pmStart > to) continue
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

// Peint un layout (images derrière + texte + images devant) dans le contexte
// COURANT (déjà mis à l'échelle/translaté par l'appelant), SANS effacer ni gérer
// la sélection. Réutilisé pour le corps ET pour l'en-tête/pied (rendu riche).
export function paintLayout(ctx: CanvasRenderingContext2D, layout: DocumentLayout, frontPhase: 'with' | 'skip' | 'only' = 'with'): void {
  const drawImgLine = (line: LayoutLine) => {
    const im = line.image!
    const { w, h, x: ix, rotation } = im
    const floating = im.wrap === 'behind' || im.wrap === 'front' || im.wrap === 'square'
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
  // Phase 'only' : ne dessiner QUE les images « devant le texte » (couche du dessus,
  // appelée APRÈS les fautes/curseur par renderDocument). 'skip' : tout sauf elles.
  if (frontPhase === 'only') {
    for (const para of layout.paragraphs) for (const line of para.lines) {
      if (line.image && line.image.wrap === 'front') drawImgLine(line)
    }
    return
  }
  // Images flottantes DERRIÈRE le texte.
  for (const para of layout.paragraphs) for (const line of para.lines) {
    if (line.image && line.image.wrap === 'behind') drawImgLine(line)
  }
  // Texte (+ images inline / bloc / carré).
  for (const para of layout.paragraphs) {
    if (para.table) {
      const tstyle = para.table.style || 'grid'
      // Fonds de cellule (en-tête / lignes alternées / couleur propre) d'abord.
      for (const cell of para.table.cells) {
        if (cell.bg) { ctx.fillStyle = cell.bg; ctx.fillRect(cell.x, cell.y, cell.w, cell.h) }
      }
      // Bordures : aucune en 'plain' ; sinon trait fin gris.
      // Chaque cellule est tracée sur sa boîte PLEINE (de `x` à `x+w`, sans inset) :
      // comme `cellA.x + cellA.w === cellB.x` (mêmes bornes `colX`/`rowY`), l'arête
      // partagée par deux cellules contiguës tombe exactement à la même coordonnée →
      // une seule bordure commune (re-tracée à l'identique, sans dédoublement). Un
      // inset (`w - 1`) décalerait les deux arêtes de 1px et dessinerait un double trait.
      if (tstyle !== 'plain') {
        ctx.strokeStyle = '#bdc1c6'; ctx.lineWidth = 1
        ctx.beginPath()
        for (const cell of para.table.cells) {
          const x0 = cell.x + 0.5, y0 = cell.y + 0.5, x1 = cell.x + cell.w + 0.5, y1 = cell.y + cell.h + 0.5
          ctx.moveTo(x0, y0); ctx.lineTo(x1, y0); ctx.lineTo(x1, y1); ctx.lineTo(x0, y1); ctx.lineTo(x0, y0)
        }
        ctx.stroke()
      }
    }
    for (const line of para.lines) {
      if (line.image) {
        if (line.image.wrap !== 'behind' && line.image.wrap !== 'front') drawImgLine(line)
        continue
      }
      // Cellule à texte vertical : coords des spans en LOCAL ; on applique la
      // rotation (±90°) + translation puis on peint normalement.
      const rotated = line.rot
      if (rotated) { ctx.save(); ctx.translate(line.rtx ?? 0, line.rty ?? 0); ctx.rotate(rotated === 90 ? Math.PI / 2 : -Math.PI / 2) }
      for (const span of line.spans) {
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
        ctx.font      = fontStr(span.marks)
        ctx.fillStyle = span.marks.color ?? DEFAULT_CLR
        if (span.marks.backgroundColor) {
          const prev = ctx.fillStyle
          ctx.fillStyle = span.marks.backgroundColor
          ctx.fillRect(span.x, line.y, span.width, line.height)
          ctx.fillStyle = prev
        }
        const basePx = (span.marks.fontSize ?? DEFAULT_PT) * PT_PX
        const scriptDy = span.marks.script === 'super' ? -basePx * 0.36
                        : span.marks.script === 'sub'  ?  basePx * 0.18 : 0
        const drawBaseline = line.baseline + scriptDy
        ctx.fillText(span.text, span.x, drawBaseline)
        if (span.marks.underline) ctx.fillRect(span.x, drawBaseline + 2, span.width, 1)
        if (span.marks.strike)    ctx.fillRect(span.x, drawBaseline - line.ascent * 0.35, span.width, 1)
      }
      if (rotated) ctx.restore()
    }
  }
  // Images flottantes DEVANT le texte (sauf en phase 'skip' où renderDocument les
  // dessine plus tard, par-dessus les fautes et le curseur).
  if (frontPhase !== 'skip') {
    for (const para of layout.paragraphs) for (const line of para.lines) {
      if (line.image && line.image.wrap === 'front') drawImgLine(line)
    }
  }
}

// Peint un layout à un décalage (px non-scalés) dans le contexte courant —
// utilisé pour l'en-tête / le pied (rendu riche dans la marge).
export function paintLayoutAt(ctx: CanvasRenderingContext2D, layout: DocumentLayout, ox: number, oy: number): void {
  ctx.save(); ctx.translate(ox, oy); paintLayout(ctx, layout); ctx.restore()
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
): void {
  const ctx   = canvas.getContext('2d')!
  const scale = dpr * zoom

  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.save()
  ctx.scale(scale, scale)
  ctx.translate(marginLeft, marginTop)
  ctx.textBaseline = 'alphabetic'
  // Same text-quality hints as the measurement context, so rendered glyphs
  // match measured widths and benefit from kerning/legibility shaping.
  applyTextQuality(ctx)

  // ── 0. Surbrillances (recherche / commentaires) — SOUS le texte pour rester
  // lisibles. Chaque plage est peinte d'un seul fill() (alpha composé une fois).
  if (highlightRanges && highlightRanges.length) {
    for (const hr of highlightRanges) {
      ctx.fillStyle = hr.color
      ctx.beginPath()
      for (const r of selectionRects(layout, hr.from, hr.to)) ctx.rect(r.x, r.y, r.w, r.h)
      ctx.fill()
    }
  }

  // ── 1. Images + texte (passe partagée) — SANS les images « devant le texte »
  // (dessinées en dernier, par-dessus fautes/curseur, comme dans Word). ───────────
  paintLayout(ctx, layout, 'skip')

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
    for (const r of selectionRects(layout, selectionRange.from, selectionRange.to)) {
      ctx.rect(r.x, r.y, r.w, r.h)
    }
    ctx.fill()
  }

  // ── 2b. Soulignés ondulés du correcteur (orthographe rouge / grammaire bleu) ──
  if (spellRanges && spellRanges.length) {
    for (const sr of spellRanges) {
      ctx.strokeStyle = sr.grammar ? '#1a73e8' : '#d93025'
      for (const r of selectionRects(layout, sr.from, sr.to)) {
        drawSquiggle(ctx, r.x, r.x + r.w, r.y + r.h - 1)
      }
    }
  }

  // ── 3. Cursor / caret (drawn on top of text) ───────────────────────────────
  if (selectionRange && selectionRange.from === selectionRange.to) {
    const c = posToCoords(layout, selectionRange.from)
    ctx.fillStyle = '#202124'
    if (c.italicAngle !== 0) {
      // Slanted caret for italic text: draw a parallelogram
      const w = 1.5
      const lean = c.italicAngle * c.height
      ctx.beginPath()
      ctx.moveTo(c.x - lean,     c.y)
      ctx.lineTo(c.x - lean + w, c.y)
      ctx.lineTo(c.x + w,        c.y + c.height)
      ctx.lineTo(c.x,            c.y + c.height)
      ctx.closePath()
      ctx.fill()
    } else {
      ctx.fillRect(c.x, c.y, 1.5, c.height)
    }
  }

  // ── 4. Images « DEVANT le texte » — couche la plus haute : elles masquent le
  // texte, les soulignés du correcteur ET le curseur (comportement Word). ─────────
  paintLayout(ctx, layout, 'only')

  ctx.restore()
}

// Trait ondulé (style correcteur) entre x1 et x2 à la base y. La couleur/épaisseur
// sont fixées par l'appelant (ctx.strokeStyle).
function drawSquiggle(ctx: CanvasRenderingContext2D, x1: number, x2: number, y: number): void {
  if (x2 <= x1) return
  const amp = 1.1, wl = 4
  ctx.save()
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.moveTo(x1, y)
  let up = true
  for (let x = x1; x <= x2; x += wl / 2) { ctx.lineTo(Math.min(x + wl / 2, x2), y + (up ? -amp : amp)); up = !up }
  ctx.stroke()
  ctx.restore()
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
      if (pos < line.pmStart || pos > line.pmEnd) continue

      // Cellule à texte vertical : on calcule le x LOCAL du caret puis on le projette
      // à l'écran ; le caret est une barre tournée (champ `rot`).
      if (line.rot) {
        let lx: number | null = null
        for (const span of line.spans) {
          const spanEnd = span.pmPos + span.text.length
          if (pos < span.pmPos || pos > spanEnd) continue
          const dx = (span.text === '\t' || span.img) ? (pos > span.pmPos ? span.width : 0) : measureW(span.text.slice(0, pos - span.pmPos), span.marks)
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
        const spanEnd = span.pmPos + span.text.length
        if (pos < span.pmPos || pos > spanEnd) continue
        // Tabulation / image inline : la largeur du span EST l'avance (atom 1 pos).
        const dx = (span.text === '\t' || span.img) ? (pos > span.pmPos ? span.width : 0) : measureW(span.text.slice(0, pos - span.pmPos), span.marks)
        // Ligne bien plus haute que le texte (image inline) → caret à la hauteur du
        // TEXTE, ancré sur la ligne de base (au BAS), pas sur toute la hauteur de l'image.
        const lm = lineMetrics(span.marks)
        if (line.height > lm.height * 1.6) {
          return { x: span.x + dx, y: line.baseline - lm.ascent, height: lm.height, italicAngle: span.marks.italic ? 0.13 : 0 }
        }
        return { x: span.x + dx, y: line.y, height: line.height, italicAngle: span.marks.italic ? 0.13 : 0 }
      }

      // pos is at end of line — use marks of last span to determine italic angle.
      // Empty line: caret au x mémorisé (`caretX`, selon alignement/indentation) ;
      // à défaut, bord gauche de la cellule (tableau) ou marge de page.
      const last = line.spans.at(-1)
      const emptyX = line.caretX ?? (line.cellX !== undefined ? line.cellX + CELL_PAD_X : 0)
      return {
        x: last ? last.x + last.width : emptyX,
        y: line.y,
        height: line.height,
        italicAngle: last?.marks.italic ? 0.13 : 0,
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
      height: lastLine.height,
      italicAngle: last?.marks.italic ? 0.13 : 0,
    }
  }
  return { x: 0, y: 0, height: DEFAULT_PT * PT_PX * LH_RATIO, italicAngle: 0 }
}

export function coordsToPos(layout: DocumentLayout, x: number, y: number): number {
  // Trouver la meilleure ligne : priorité verticale, puis (pour les cellules de
  // tableau partageant un même y) départage horizontal par les bornes de cellule.
  let best: LayoutLine | null = null
  let bestScore = Infinity

  for (const para of layout.paragraphs) {
    for (const line of para.lines) {
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
    const len = span.text.length
    const wideAtom = span.text === '\t' || !!span.img  // largeur portée par le span
    for (let i = 0; i <= len; i++) {
      const cx = span.x + (wideAtom ? (i > 0 ? span.width : 0) : measureW(span.text.slice(0, i), span.marks))
      const d  = Math.abs(target - cx)
      if (d < bestXd) { bestXd = d; bestPos = span.pmPos + i }
    }
  }

  return bestPos
}

// ── Word / paragraph boundary helpers ────────────────────────────────────────

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
    let fullText = ''
    const pmPosOf: number[] = []
    for (const line of para.lines) {
      for (const span of line.spans) {
        for (let i = 0; i < span.text.length; i++) {
          pmPosOf.push(span.pmPos + i)
          fullText += span.text[i]
        }
      }
    }
    if (fullText.length === 0) return { from: pos, to: pos }

    const offset = pos - (para.pmStart + 1)  // char index just after cursor
    const isW    = (i: number) => i >= 0 && i < fullText.length && /\w/.test(fullText[i])

    let lo = offset, hi = offset
    // Extend left while previous char is a word char
    while (lo > 0 && isW(lo - 1)) lo--
    // Extend right while current char is a word char
    while (isW(hi)) hi++

    if (lo === hi) {
      // Cursor is not inside a word — try word to the left
      lo = offset > 0 ? offset - 1 : 0
      hi = lo
      while (lo > 0 && isW(lo - 1)) lo--
      while (isW(hi)) hi++
      if (lo === hi) return { from: pos, to: pos }
    }

    const from = pmPosOf[lo]
    const to   = hi < pmPosOf.length ? pmPosOf[hi] : pmPosOf[pmPosOf.length - 1] + 1
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

    let fullText = ''
    const pmPosOf: number[] = []
    for (const line of para.lines) {
      for (const span of line.spans) {
        for (let i = 0; i < span.text.length; i++) {
          pmPosOf.push(span.pmPos + i)
          fullText += span.text[i]
        }
      }
    }

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

    let fullText = ''
    const pmPosOf: number[] = []
    for (const line of para.lines) {
      for (const span of line.spans) {
        for (let i = 0; i < span.text.length; i++) {
          pmPosOf.push(span.pmPos + i)
          fullText += span.text[i]
        }
      }
    }

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
}

// Reconstruit les LayoutParagraph d'une page à partir d'une tranche de lignes,
// en ramenant les y au repère local de la page (0 = haut du contenu).
function rebuildPageParas(
  taken: Array<{ para: LayoutParagraph; line: LayoutLine }>,
  startY: number,
  xShift = 0,
  colW = 0,
  multiCol = false,
): LayoutParagraph[] {
  const paras: LayoutParagraph[] = []
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
    }
    if (cur && curSrc === para) {
      cur.lines.push(shifted)
    } else {
      // Géométrie de tableau : ramener les rectangles de cellule au repère local de page (+ décalage colonne).
      const table = para.table
        ? { cells: para.table.cells.map(c => ({ ...c, x: c.x + xShift, y: c.y - startY })), style: para.table.style, accent: para.table.accent,
            colX: para.table.colX?.map(x => x + xShift), rowY: para.table.rowY?.map(y => y - startY) }
        : undefined
      cur = { lines: [shifted], y: para.y - startY, height: para.height, pmStart: para.pmStart, pmEnd: para.pmEnd, docIdx: para.docIdx, secIdx: para.secIdx, breakBefore: para.breakBefore, table }
      curSrc = para
      paras.push(cur)
    }
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

// Pagination multi-sections + multi-colonnes : par page, on remplit `columns`
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
    const pageStartY = refs[i].line.y
    const pageParas: LayoutParagraph[] = []
    let stop = false

    for (let col = 0; col < cols && !stop; col++) {
      if (i >= refs.length || refs[i].para.secIdx !== pageSec) break
      const colStartY = refs[i].line.y
      const taken: Ref[] = []
      let lastPara: LayoutParagraph | null = null
      while (i < refs.length) {
        if (refs[i].para.secIdx !== pageSec) { stop = true; break }
        const startingPara = refs[i].para !== lastPara
        if (taken.length > 0 && startingPara && refs[i].para.breakBefore) { stop = true; break }
        // Enchaînements : lignes/paragraphe solidaires → repousser la grappe entière.
        if (taken.length > 0 && startingPara && shouldKeepBreak(refs, i, contentH - (refs[i].line.y - colStartY), contentH)) break
        const ln = refs[i].line
        if (taken.length > 0 && (ln.y + ln.height - colStartY) > contentH) break
        lastPara = refs[i].para
        taken.push(refs[i]); i++
      }
      const xShift = col * (g.colW + g.colGap)
      pageParas.push(...rebuildPageParas(taken, colStartY, xShift, g.colW, cols > 1))
    }

    pages.push({
      layout: { paragraphs: pageParas, totalHeight: contentH, contentW: layout.contentW },
      startY: pageStartY, height: contentH, secIdx: pageSec,
    })
  }
  return pages
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
