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
}

export interface LayoutLine {
  spans:    LayoutSpan[]
  y:        number   // CSS px from content-area top (top of line)
  height:   number   // CSS px
  ascent:   number   // CSS px from line top to baseline
  baseline: number   // CSS px from content-area top to baseline (y + ascent)
  pmStart:  number
  pmEnd:    number
  image?:   { src: string; w: number; h: number; x: number; rotation: number; wrap?: string; wrapY?: number; alt?: string; tbFill?: string; tbStroke?: string }   // ligne-image (dims, décalage gauche, rotation°, habillage, alt, couleurs zone-texte)
  cellX?:   number   // bornes horizontales de la cellule (tableaux) pour coordsToPos
  cellW?:   number
  caretX?:  number   // x du caret pour une ligne VIDE (selon alignement/indentation)
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
  emptyPt?:    number     // taille (pt) portée par un paragraphe VIDE (attr fontMarks) → hauteur de ligne
  image?:      { src: string; width: number; height: number; align: 'left' | 'center' | 'right'; rotation: number; wrap?: string; wrapX?: number; wrapY?: number; alt?: string; tbFill?: string; tbStroke?: string }   // bloc-image (0 = taille naturelle) + habillage + alt + couleurs zone-texte
  table?:      RenderTable   // tableau (lignes/cellules)
}

// ── Structures de tableau (parse) ───────────────────────────────────────────
interface RenderTableCell { paras: RenderParagraph[]; colspan: number; rowspan: number; merged: boolean; cellBg?: string }
interface RenderTableRow  { cells: RenderTableCell[] }
interface RenderTable     { rows: RenderTableRow[]; colCount: number; style: string; accent?: string; colWidths?: number[]; rowHeights?: number[] }

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
        image: { src: String(a.src ?? ''), width: Number(a.width) || 0, height: Number(a.height) || 0, align: (a.align as 'left'|'center'|'right') ?? 'left', rotation: Number(a.rotation) || 0, wrap: (a.wrap as string) || 'inline', wrapX: Number(a.wrapX) || 0, wrapY: Number(a.wrapY) || 0, alt: a.alt != null ? String(a.alt) : undefined, tbFill: a.tbFill != null ? String(a.tbFill) : undefined, tbStroke: a.tbStroke != null ? String(a.tbStroke) : undefined },
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
          cells.push({ paras: cellParas, colspan, rowspan: Math.max(1, Number(ca.rowspan) || 1), merged: !!ca.merged, cellBg: ca.cellBg != null ? String(ca.cellBg) : undefined })
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
        table: { rows, colCount, style: String(ta.tableStyle || 'grid'), accent: ta.accent != null ? String(ta.accent) : undefined, colWidths: Array.isArray(ta.colWidths) ? (ta.colWidths as number[]) : undefined, rowHeights: Array.isArray(ta.rowHeights) ? (ta.rowHeights as number[]) : undefined },
      })
      return
    }
    const bStart = pos
    const breakBefore = pendingBreak
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

  for (let i = 0; i < (doc.content?.length ?? 0); i++) block(doc.content![i], 0, i)
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
  const squares: Array<{ y0: number; y1: number; xEdge: number; side: 'left' | 'right' }> = []
  const GAP = 10   // marge entre l'objet et le texte
  // Largeur/décalage disponibles pour une ligne à [yTop, yTop+h] (global).
  const exclusionAt = (cw: number) => (yTop: number, h: number) => {
    let left = 0, right = cw
    for (const s of squares) {
      if (yTop + h <= s.y0 || yTop >= s.y1) continue   // pas de chevauchement vertical
      if (s.side === 'left')  left  = Math.max(left,  s.xEdge + GAP)
      else                    right = Math.min(right, s.xEdge - GAP)
    }
    return { left, width: Math.max(40, right - left) }
  }

  for (const para of paragraphs) {
    y += para.spaceBefore
    const pY = y

    // ── Tableau : mise en page 2D dédiée (cellules côte à côte) ──────────────
    if (para.table) {
      const { lines, table, height } = layoutTable(para.table, widthFor(para.secIdx), pY)
      out.push({
        lines, y: pY - para.spaceBefore,
        height: height + para.spaceBefore + para.spaceAfter,
        pmStart: para.pmStart, pmEnd: para.pmEnd, docIdx: para.docIdx,
        secIdx: para.secIdx, breakBefore: para.breakBefore, table,
      })
      y = pY + height + para.spaceAfter
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
      const onRight = im.x + im.w / 2 > cw / 2
      squares.push({
        y0: pY, y1: pY + im.h,
        xEdge: onRight ? im.x : im.x + im.w,
        side: onRight ? 'right' : 'left',
      })
    }
    for (const line of lines) {
      line.y = y
      // Répartir l'interligne (leading) : moitié au-dessus, moitié en dessous du
      // texte, pour qu'il soit centré verticalement dans le bloc de ligne (comme
      // Google Docs). `line.height` inclut déjà l'interligne du paragraphe.
      const topLead = (line.height * (1 - 1 / para.lineSpacing)) / 2
      line.baseline = y + topLead + line.ascent
      y += line.height
    }

    const pHeight = y - pY
    y += para.spaceAfter

    out.push({
      lines,
      y: pY - para.spaceBefore,
      height: pHeight + para.spaceBefore + para.spaceAfter,
      pmStart: para.pmStart,
      pmEnd: para.pmEnd,
      docIdx: para.docIdx,
      secIdx: para.secIdx,
      breakBefore: para.breakBefore,
    })
  }

  return { out, totalHeight: y }
}

// Mise en page d'un tableau : colonnes égales, texte des cellules réagencé à la
// largeur de colonne, hauteur de ligne = max des cellules. Renvoie les lignes de
// toutes les cellules (avec x absolu et y à partir de `yTop`), la géométrie des
// bordures et la hauteur totale. Les lignes portent cellX/cellW pour coordsToPos.
const CELL_PAD = 6
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

  // 1) Placement dans la grille (colStart/colspan/rowspan) avec occupation des
  //    colonnes par les rowspans descendants.
  type Placed = { cell: RenderTableCell; r: number; c0: number; cspan: number; rspan: number; cl: ReturnType<typeof layoutParagraphs> }
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
      const innerW = cellW(c, cspan) - 2 * CELL_PAD
      placed.push({ cell, r, c0: c, cspan, rspan, cl: layoutParagraphs(cell.paras, () => innerW) })
      for (let k = c; k < c + cspan; k++) occupied[k] = rspan
      c += cspan
    }
    for (let k = 0; k < colCount; k++) if (occupied[k] > 0) occupied[k]--
  })

  // 2) Hauteurs de ligne : base = hauteur MIN réglée (rowHeights) ou MIN_ROW_H, puis
  //    contenu des cellules non-spanées, puis report du surplus des rowspans.
  const rowH: number[] = rows.map((_r, i) => Math.max(MIN_ROW_H, table.rowHeights?.[i] ?? 0))
  for (const p of placed) if (p.rspan === 1) rowH[p.r] = Math.max(rowH[p.r], p.cl.totalHeight + 2 * CELL_PAD)
  for (const p of placed) if (p.rspan > 1) {
    const need = p.cl.totalHeight + 2 * CELL_PAD
    let have = 0; for (let k = p.r; k < p.r + p.rspan && k < rows.length; k++) have += rowH[k]
    if (need > have) rowH[Math.min(p.r + p.rspan - 1, rows.length - 1)] += need - have
  }
  const rowTop: number[] = []; let acc = 0
  for (let r = 0; r < rows.length; r++) { rowTop[r] = acc; acc += rowH[r] }
  const rowY = [...rowTop.map(y => yTop + y), yTop + acc]

  // 3) Géométrie + couleur de fond + report des lignes de texte.
  for (const p of placed) {
    const x = colX[p.c0]
    const y = rowTop[p.r]
    const w = cellW(p.c0, p.cspan)
    let h = 0; for (let k = p.r; k < p.r + p.rspan && k < rows.length; k++) h += rowH[k]
    let bg: string | undefined = p.cell.cellBg || undefined
    if (!bg) {
      if (p.r === 0 && (style === 'header' || style === 'striped')) bg = tint(accent, 0.16)
      else if (style === 'striped' && p.r % 2 === 1) bg = tint(accent, 0.06)
    }
    cells.push({ x, y: yTop + y, w, h, bg, r: p.r, c: p.c0, colspan: p.cspan, rowspan: p.rspan })
    for (const para of p.cl.out) for (const ln of para.lines) {
      for (const sp of ln.spans) sp.x += x + CELL_PAD
      if (ln.caretX !== undefined) ln.caretX += x + CELL_PAD
      ln.y        += yTop + y + CELL_PAD
      ln.baseline += yTop + y + CELL_PAD
      ln.cellX = x
      ln.cellW = w
      lines.push(ln)
    }
  }

  return { lines, table: { cells, style, accent, colX, rowY }, height: acc }
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
      image: { src: para.image.src, w: dispW, h: dispH, x, rotation: rot, wrap, wrapY: para.image.wrapY || 0, alt: para.image.alt, tbFill: para.image.tbFill, tbStroke: para.image.tbStroke } })
    return lines
  }

  interface Token { text: string; marks: TextMark; width: number; pmPos: number; isSpace: boolean; isTab?: boolean }

  // Tokenise into words + whitespace, en isolant CHAQUE tabulation (`\t`) comme un token
  // propre (largeur calculée à la pose, = distance jusqu'au prochain taquet).
  const tokens: Token[] = []
  for (const span of para.spans) {
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
    lines.push({ spans: [], y: 0, baseline: 0, height: lm.height * para.lineSpacing, ascent: lm.ascent, pmStart: innerPos, pmEnd: innerPos, caretX })
    return lines
  }

  let lineToks: Token[] = []
  let lineW   = 0
  let lStart  = para.spans[0]?.pmPos ?? para.pmStart

  // Largeur DISPONIBLE par ligne (varie quand un objet « carré » exclut une zone).
  // Sans exclusion, curLeft=indent et curAvail=avail → comportement inchangé.
  let lineYRel = 0
  const estH = lineMetrics({}).height * para.lineSpacing
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

    // Max metrics across all tokens
    let maxAsc = 0, maxDsc = 0, maxH = 0
    for (const t of lineToks) {
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
      spans.push({ text: t.text, marks: t.marks, x, width: w, pmPos: t.pmPos })
      x += w
      lEnd = t.pmPos + t.text.length
    }

    const h = maxH * para.lineSpacing
    lines.push({ spans, y: 0, baseline: 0, height: h, ascent: maxAsc, pmStart: lStart, pmEnd: lEnd })
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
    lines.push({ spans: [], y: 0, baseline: 0, height: lm.height * para.lineSpacing, ascent: lm.ascent, pmStart: innerPos, pmEnd: innerPos })
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
      // Tabulation : la largeur du span EST l'avance (≠ measureW d'un '\t').
      if (span.text === '\t') return span.x + (pos > span.pmPos ? span.width : 0)
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

  for (const para of layout.paragraphs) {
    for (const line of para.lines) {
      if (line.pmEnd < from || line.pmStart > to) continue
      if (line.image) continue   // l'image gère son propre cadre de sélection

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

  return rects
}

// ── Rendering ─────────────────────────────────────────────────────────────────

// Peint un layout (images derrière + texte + images devant) dans le contexte
// COURANT (déjà mis à l'échelle/translaté par l'appelant), SANS effacer ni gérer
// la sélection. Réutilisé pour le corps ET pour l'en-tête/pied (rendu riche).
export function paintLayout(ctx: CanvasRenderingContext2D, layout: DocumentLayout): void {
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
    const img = getImage(im.src)
    ctx.save()
    ctx.translate(cx2, cy2)
    if (rotation) ctx.rotate(rotation * Math.PI / 180)
    if (imgReady(img)) ctx.drawImage(img!, -w / 2, -h / 2, w, h)
    else { ctx.fillStyle = '#f1f3f4'; ctx.fillRect(-w / 2, -h / 2, w, h); ctx.strokeStyle = '#dadce0'; ctx.strokeRect(-w / 2 + 0.5, -h / 2 + 0.5, w - 1, h - 1) }
    ctx.restore()
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
      if (tstyle !== 'plain') {
        ctx.strokeStyle = '#bdc1c6'; ctx.lineWidth = 1
        for (const cell of para.table.cells) ctx.strokeRect(cell.x + 0.5, cell.y + 0.5, cell.w - 1, cell.h - 1)
      }
    }
    for (const line of para.lines) {
      if (line.image) {
        if (line.image.wrap !== 'behind' && line.image.wrap !== 'front') drawImgLine(line)
        continue
      }
      for (const span of line.spans) {
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
    }
  }
  // Images flottantes DEVANT le texte.
  for (const para of layout.paragraphs) for (const line of para.lines) {
    if (line.image && line.image.wrap === 'front') drawImgLine(line)
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

  // ── 1. Images + texte (passe partagée) ──────────────────────────────────────
  paintLayout(ctx, layout)

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
export function posToCoords(layout: DocumentLayout, pos: number, preferEnd = false): CursorMetrics {
  for (const para of layout.paragraphs) {
    for (let li = 0; li < para.lines.length; li++) {
      const line = para.lines[li]
      if (pos < line.pmStart || pos > line.pmEnd) continue

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
        // Tabulation : la largeur du span EST l'avance (≠ measureW d'un '\t').
        const dx = span.text === '\t' ? (pos > span.pmPos ? span.width : 0) : measureW(span.text.slice(0, pos - span.pmPos), span.marks)
        return { x: span.x + dx, y: line.y, height: line.height, italicAngle: span.marks.italic ? 0.13 : 0 }
      }

      // pos is at end of line — use marks of last span to determine italic angle.
      // Empty line: caret au x mémorisé (`caretX`, selon alignement/indentation) ;
      // à défaut, bord gauche de la cellule (tableau) ou marge de page.
      const last = line.spans.at(-1)
      const emptyX = line.caretX ?? (line.cellX !== undefined ? line.cellX + CELL_PAD : 0)
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
      x: last ? last.x + last.width : (lastLine.caretX ?? (lastLine.cellX !== undefined ? lastLine.cellX + CELL_PAD : 0)),
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

  // Find nearest character in that line
  let bestPos  = best.pmStart
  let bestXd   = Infinity

  for (const span of best.spans) {
    const len = span.text.length
    const isTabSpan = span.text === '\t'
    for (let i = 0; i <= len; i++) {
      const cx = span.x + (isTabSpan ? (i > 0 ? span.width : 0) : measureW(span.text.slice(0, i), span.marks))
      const d  = Math.abs(x - cx)
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
        if (taken.length > 0 && refs[i].para !== lastPara && refs[i].para.breakBefore) { stop = true; break }
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
    while (i < refs.length) {
      const ln = refs[i].line
      const bottomRel = (ln.y + ln.height) - startY
      // une ligne ne se coupe pas : si elle dépasse et que la page n'est pas vide → page suivante
      if (taken.length > 0 && bottomRel > contentH) break
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
