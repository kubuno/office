import { useEffect, useLayoutEffect, useCallback, useRef, useState, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import type { Editor } from '@tiptap/core'
import type { JSONContent } from '@tiptap/react'
import { useNavigate, useLocation } from 'react-router-dom'
import { useEditor, EditorContent } from '@tiptap/react'
import { Extension, Node as TipTapNode, Mark as TipTapMark, InputRule } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import Collaboration from '@tiptap/extension-collaboration'
import * as Y from 'yjs'
import {
  absolutePositionToRelativePosition,
  relativePositionToAbsolutePosition,
  ySyncPluginKey,
  type ProsemirrorBinding,
} from '@tiptap/y-tiptap'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { PresenceAvatars, userColor, usePublishCursor, RemoteCursors } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import DocumentShareDialog from './DocumentShareDialog'
import Placeholder from '@tiptap/extension-placeholder'
import Underline from '@tiptap/extension-underline'
import Link from '@tiptap/extension-link'
import TaskList from '@tiptap/extension-task-list'
import TaskItem from '@tiptap/extension-task-item'
import TextAlign from '@tiptap/extension-text-align'
import Highlight from '@tiptap/extension-highlight'
import { TextStyle } from '@tiptap/extension-text-style'
import Color from '@tiptap/extension-color'
import CharacterCount from '@tiptap/extension-character-count'
import {
  Bold, Italic, UnderlineIcon, Strikethrough, CheckSquare,
  List, ListOrdered, Type, Eraser,
  AlignLeft, AlignCenter, AlignRight, AlignJustify,
  Link as LinkIcon, Highlighter,
  FileText, RotateCcw, RotateCw,
  Minus, Plus, Printer, Star, UserPlus,
  IndentIncrease, IndentDecrease, Image as ImageIcon, ChevronDown, X,
  LayoutTemplate,
  Scissors, Copy, ClipboardPaste, Table as TableIcon, Square, Search, Hash,
  Eye, Ruler as RulerIcon, PanelLeft, Sigma, ListTree, FileDown,
  SplitSquareVertical, Superscript, Subscript, CopyPlus, SpellCheck,
  MessageSquare, MessageSquarePlus, Check, Trash2, Send, CornerDownRight,
  Rows3, Columns3, Combine, Paintbrush, Pencil, BookMarked,
  Languages, Accessibility, BookOpen, SlidersHorizontal, Monitor,
  ZoomIn, MoveHorizontal, Files, Shapes, CloudOff,
  Stamp, SquareDashed,
  CaseSensitive, CalendarClock, ArrowDownAZ, ArrowUpAZ, Bookmark, Pilcrow, Frame, Quote, WrapText, Omega,
} from 'lucide-react'
import { Dropdown, MenuDropdown, Button, Checkbox, Radio, NumberInput, ColorField, GradientField, gradientToCss, DEFAULT_GRADIENT, ColorSwatchPicker, AnchoredPopover, RangeSlider, FontPicker, FloatingWindow, useAppPickerTheme } from '@ui'
import type { MenuItem, Gradient } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { UndoRedoButtons } from './ribbon/UndoRedoButtons'
import { Backstage } from './ribbon/Backstage'
import { useDocumentsBackstageSections } from './DocumentsBackstage'
import { WORKSPACE_OFFICE } from '@kubuno/sdk'
import { MacrosMenu } from './macros/MacrosMenu'
import type { RibbonTab } from './ribbon/types'
import { findIssues, ignoreWord, ignoreWordSession, unignoreWord, personalDictionary, type SpellIssue } from './spellcheck'
import { loadSpeller, onSpellerReady, suggestWord } from './hunspell'
import { loadSystemFonts } from './systemAssets'
import { prompt, api } from '@kubuno/sdk'
import { i18n } from '@kubuno/sdk'
import { useOfficeStore } from './store'
import { fontsApi } from './api'
import { pagesToPdf, downloadBlob } from './pdfExport'
import { TextSelection, NodeSelection, Plugin } from '@tiptap/pm/state'
import {
  layoutDocumentMulti, layoutDocument, renderDocument, paintLayoutAt, posToCoords, coordsToPos,
  wordBoundariesAt, paragraphBoundariesAt, selectionRects,
  lineStartAt, lineEndAt, docStart, docEnd,
  paginateMulti, parseRichTextBox, RICH_TB_PAD, setShapeSrcResolver,
} from './canvas-engine'
import type { DocumentLayout, PageLayout, LayoutLine, CursorMetrics } from './canvas-engine'

// ── CSS for canvas cursor blink ───────────────────────────────────────────────

// Exact Google Docs blink keyframe (extracted from kix.css):
// .docs-text-ui-cursor-blink { animation-duration:1s; animation-delay:.5s; }
// @keyframes docs-text-ui-blink { 0%{opacity:1} 13%{opacity:0} 50%{opacity:0} 63%{opacity:1} to{opacity:1} }
// Cursor structure from .kix-cursor / .kix-cursor-caret / .kix-cursor-top:
//   container: position:absolute; width:0; z-index:27
//   caret:     position:absolute; width:0; border-left:2px solid  (the cursor IS the border)
//   top cap:   position:absolute; width:5px; height:5px; border-radius:0 2px 2px 0; top:-2px; left:0
const CURSOR_STYLE = `
  @keyframes _gdocs_blink {
    0%   { opacity: 1 }
    13%  { opacity: 0 }
    50%  { opacity: 0 }
    63%  { opacity: 1 }
    100% { opacity: 1 }
  }
  .office-cursor {
    position: absolute;
    width: 0;
    pointer-events: none;
    z-index: 27;
  }
  .office-cursor-caret {
    position: absolute;
    width: 0;
    border-left: 2px solid #1a73e8;
    top: 0;
    left: 0;
    height: 100%;
  }
  .office-cursor-top {
    position: absolute;
    border-radius: 0 2px 2px 0;
    top: -2px;
    left: 0;
    height: 5px;
    width: 5px;
    background: #1a73e8;
    font-size: 0;
  }
  .office-cursor.active .office-cursor-caret,
  .office-cursor.active .office-cursor-top {
    animation: _gdocs_blink 1s 0.5s infinite;
  }
  /* Zone d'édition riche en-tête/pied : marges nulles, police de base. */
  .kb-hf-zone .ProseMirror { outline: none; font-family: Arial, sans-serif; font-size: 13.3px; color: #202124; line-height: 1.2; }
  .kb-hf-zone .ProseMirror p { margin: 0; }
  .kb-hf-zone .ProseMirror img { max-width: 100%; }
  .kb-hf-zone .ProseMirror p.is-editor-empty:first-child::before { content: attr(data-placeholder); color: #9aa0a6; float: left; height: 0; pointer-events: none; }
  /* Poignées de redimensionnement de tableau (bordures colonnes/lignes/bord). */
  .kb-tbl-rz { position: absolute; z-index: 28; }
  .kb-tbl-rz-v { cursor: col-resize; }
  .kb-tbl-rz-h { cursor: row-resize; }
  .kb-tbl-rz::after { content: ''; position: absolute; inset: 0; background: transparent; transition: background .1s; }
  .kb-tbl-rz-v::after { left: 50%; width: 2px; margin-left: -1px; top: 0; bottom: 0; }
  .kb-tbl-rz-h::after { top: 50%; height: 2px; margin-top: -1px; left: 0; right: 0; }
  .kb-tbl-rz:hover::after { background: #1a73e8; }
`

// ── Layout constants ──────────────────────────────────────────────────────────

// Google Docs: margin-bottom:7.5pt + margin-top:3.75pt on adjacent pages = ~15px total inter-page gap
const PAGE_GAP  = 10   // écart entre pages, logique Google Docs (~10px)
const MIN_TBL_ROW_H = 22   // hauteur minimale d'une ligne de tableau (= MIN_ROW_H du moteur)
// Architecture : un seul modèle ProseMirror (caché) + canvas paginé en rendu.
const RULER_SZ  = 20
const PX_PER_CM = 96 / 2.54

// ── Section & page model ──────────────────────────────────────────────────────

type Orientation = 'portrait' | 'landscape'

interface SectionDef {
  id: string
  orientation: Orientation
  margins: { top: number; right: number; bottom: number; left: number }
  columns?: number   // nombre de colonnes (1 par défaut)
  // Mise en page avancée (dialogue « Mise en page ») — optionnels (rétro-compat).
  gutter?: number
  headerDist?: number
  footerDist?: number
  vAlign?: 'top' | 'center' | 'bottom' | 'both'
  sectionStart?: 'nextPage' | 'continuous' | 'evenPage' | 'oddPage'
}

interface PageData {
  id: string
  sectionId: string
  content: JSONContent
}

interface PageGeometry {
  pageW: number; pageH: number
  contentW: number; contentH: number
  marginH: number; marginV: number   // marges gauche / HAUT
  marginBottom: number               // marge du bas (≠ haut quand l'utilisateur la règle)
  columns: number    // nb colonnes
  colW: number       // largeur d'une colonne
  colGap: number     // gouttière entre colonnes
}

// Formats de papier (cm, portrait). A4 par défaut ; le format est global au
// document (docMeta.pageSize), l'orientation reste par section.
export type PaperSize = 'a4' | 'a5' | 'a3' | 'letter' | 'legal'
const PAPER_SIZES: Record<PaperSize, { w: number; h: number; label: string }> = {
  a4:     { w: 21,    h: 29.7,  label: 'A4 (21 × 29,7 cm)' },
  a5:     { w: 14.8,  h: 21,    label: 'A5 (14,8 × 21 cm)' },
  a3:     { w: 29.7,  h: 42,    label: 'A3 (29,7 × 42 cm)' },
  letter: { w: 21.59, h: 27.94, label: 'Letter (21,6 × 27,9 cm)' },
  legal:  { w: 21.59, h: 35.56, label: 'Legal (21,6 × 35,6 cm)' },
}

const COL_GAP = 36   // gouttière entre colonnes (px), ~0.95cm
function getGeometry(section: SectionDef, paper: PaperSize = 'a4'): PageGeometry {
  const landscape = section.orientation === 'landscape'
  const sz        = PAPER_SIZES[paper] ?? PAPER_SIZES.a4
  const pageW     = Math.round((landscape ? sz.h : sz.w) * PX_PER_CM)
  const pageH     = Math.round((landscape ? sz.w : sz.h) * PX_PER_CM)
  const contentW  = pageW - section.margins.left - section.margins.right
  const columns   = Math.max(1, Math.min(3, section.columns ?? 1))
  const colGap    = columns > 1 ? COL_GAP : 0
  const colW      = (contentW - (columns - 1) * colGap) / columns
  return {
    pageW, pageH,
    contentW,
    contentH: pageH - section.margins.top  - section.margins.bottom,
    marginH:  section.margins.left,
    marginV:  section.margins.top,
    marginBottom: section.margins.bottom,
    columns, colW, colGap,
  }
}

// Construit la géométrie de chaque section du document : index 0 = section de
// base (prop `section`) ; chaque nœud `sectionBreak` ajoute une section dont la
// géométrie vient de ses attributs (orientation + marges). L'ordre suit le
// document → cohérent avec le `secIdx` calculé par le moteur de layout.
function buildSectionGeoms(doc: JSONContent, base: SectionDef, paper: PaperSize = 'a4'): PageGeometry[] {
  const geoms: PageGeometry[] = [getGeometry(base, paper)]
  for (const node of (doc as { content?: JSONContent[] }).content ?? []) {
    if (node.type === 'sectionBreak') {
      const a = (node.attrs ?? {}) as Record<string, number | string>
      // MVP : seule l'ORIENTATION varie par section ; les marges restent celles
      // de la section de base (la règle affiche/édite donc des marges cohérentes).
      geoms.push(getGeometry({
        id: '',
        orientation: (a.orientation as Orientation) ?? 'portrait',
        margins: base.margins,
      }, paper))
    }
  }
  return geoms
}

// Ordonnée (px, repère page) du HAUT de la bande d'en-tête / de pied — utilisée
// à la fois pour le rendu canvas et pour positionner la zone d'édition.
function headerBandTop(gg: { marginV: number }) { return gg.marginV * 0.28 }
function footerBandTop(gg: { pageH: number; marginBottom: number }) { return gg.pageH - gg.marginBottom * 0.72 }

function newSectionId() { return crypto.randomUUID() }
function newPageId()    { return crypto.randomUUID() }

function defaultSection(id: string): SectionDef {
  return { id, orientation: 'portrait', margins: { top: 96, right: 96, bottom: 96, left: 96 } }
}

function emptyDoc(): JSONContent {
  return { type: 'doc', content: [{ type: 'paragraph' }] }
}

// Position des numéros de page (ou 'none'). Décoration de rendu, par page.
type PageNumbers = 'none' | 'footer-right' | 'footer-center' | 'header-right' | 'header-center'

// En-tête / pied de page RICHES (façon Word) : un vrai document ProseMirror
// (mise en forme, images, tableaux…), avec champs dynamiques {page} {pages}
// {date} {titre} et option « 1ʳᵉ page différente ».
export type HFContent = JSONContent
const emptyHF = (): HFContent => ({ type: 'doc', content: [{ type: 'paragraph' }] })
const isHFEmpty = (d: HFContent | null | undefined): boolean => {
  const c = (d as { content?: JSONContent[] })?.content
  if (!c || !c.length) return true
  return c.every(n => n.type === 'paragraph' && !(n.content && n.content.length))
}

interface DocMeta {
  pageNumbers: PageNumbers
  header: HFContent; footer: HFContent
  hfFirstPage?: boolean          // true = pas d'en-tête/pied sur la 1ʳᵉ page
  pageColor?: string; pageGrad?: Gradient
  paperSize?: PaperSize
  styles?: Record<string, Partial<NamedStyleMeta>>   // surcharges de styles nommés par document
  watermark?: WatermarkDef | null    // filigrane (texte diagonal/horizontal derrière le contenu)
  pageBorder?: PageBorderDef | null  // bordure de page (cadre dans la marge)
  lineNumbers?: LineNumbersDef | null // numéros de lignes (marge gauche)
  pageNumFormat?: PageNumFormat       // format des numéros de page (arabe/romain/lettres)
  pageNumStart?: number               // premier numéro de page
}
// Champs persistables d'un style nommé (sans le libellé i18n, recalculé à l'usage).
interface NamedStyleMeta { block: 'paragraph' | 'heading'; level?: number; font?: string; size?: number; bold?: boolean; italic?: boolean; color?: string; align?: 'left' | 'center' | 'right' | 'justify'; lineHeight?: number; spaceBefore?: number; spaceAfter?: number; name?: string }

// Filigrane du document (façon Word « Filigrane ») : un texte estompé peint DERRIÈRE
// le contenu de chaque page. `size = 0` (ou absent) ⇒ taille auto-ajustée à la page.
interface WatermarkDef { text: string; color: string; opacity: number; diagonal: boolean; size?: number; font?: string }
// Bordure de page : cadre tracé dans la marge, à `margin` px du bord de la page.
interface PageBorderDef { color: string; width: number; style: 'solid' | 'dashed' | 'dotted' | 'double'; margin: number }
// Numéros de lignes (Word « Numéros de lignes ») : peints dans la marge gauche.
// mode 'continuous' = compteur continu sur tout le document ; 'page' = redémarre à
// chaque page. `interval` = n'affiche qu'un numéro toutes les N lignes (1 = toutes).
interface LineNumbersDef { mode: 'continuous' | 'page'; interval: number }
// Encadré de paragraphe (« Bordures » de Word), porté par l'attribut `paraBorder`.
interface ParaBorderDef { color: string; width: number; style: 'solid' | 'dashed' | 'dotted' | 'double' }

const DEFAULT_WATERMARK: WatermarkDef = { text: 'CONFIDENTIEL', color: '#bdbdbd', opacity: 0.45, diagonal: true, size: 0 }
const DEFAULT_PAGE_BORDER: PageBorderDef = { color: '#1a73e8', width: 2, style: 'solid', margin: 24 }
const DEFAULT_LINE_NUMBERS: LineNumbersDef = { mode: 'continuous', interval: 1 }
const DEFAULT_PARA_SHADING = '#fff2cc'                                                          // jaune doux par défaut
const DEFAULT_PARA_BORDER: ParaBorderDef = { color: '#9aa0a6', width: 1, style: 'solid' }

// Compte les lignes « de corps » d'une page (pour la numérotation des lignes) :
// ignore les lignes-image et les lignes de cellules de tableau, comme Word.
function countBodyLines(pg: PageLayout | undefined, doc?: import('@tiptap/pm/model').Node | null): number {
  if (!pg) return 0
  let n = 0
  for (const para of pg.layout.paragraphs) {
    if (doc && doc.nodeAt(para.pmStart)?.attrs?.suppressLineNumbers) continue
    for (const ln of para.lines) { if (ln.image || ln.cellX != null) continue; n++ }
  }
  return n
}

// Peint un filigrane sur la page courante (repère page, origine coin haut-gauche).
// Appelé en composite `destination-over` pour passer SOUS le texte déjà rendu.
function paintWatermark(cx: CanvasRenderingContext2D, gg: PageGeometry, wm: WatermarkDef) {
  const txt = (wm.text || '').trim()
  if (!txt) return
  cx.save()
  cx.globalAlpha = Math.max(0, Math.min(1, wm.opacity))
  cx.fillStyle = wm.color
  cx.textAlign = 'center'
  cx.textBaseline = 'middle'
  cx.translate(gg.pageW / 2, gg.pageH / 2)
  if (wm.diagonal) cx.rotate(-Math.atan2(gg.pageH, gg.pageW))
  // Largeur cible : ~85 % de la diagonale (diagonal) ou de la largeur de contenu.
  const target = wm.diagonal ? Math.hypot(gg.pageW, gg.pageH) * 0.85 : (gg.pageW - 2 * gg.marginH)
  const family = wm.font || 'Arial, sans-serif'
  let size = wm.size && wm.size > 0 ? wm.size * (96 / 72) : 120
  if (!wm.size || wm.size <= 0) {
    cx.font = `bold 100px ${family}`
    const w100 = cx.measureText(txt).width || 1
    size = Math.max(24, (target / w100) * 100)
  }
  cx.font = `bold ${size}px ${family}`
  cx.fillText(txt, 0, 0)
  cx.restore()
}

// Trace une bordure de page (cadre) dans la marge. `double` = deux traits parallèles.
function paintPageBorder(cx: CanvasRenderingContext2D, gg: PageGeometry, pb: PageBorderDef) {
  if (pb.width <= 0) return
  const m = Math.max(2, pb.margin)
  cx.save()
  cx.strokeStyle = pb.color
  cx.lineWidth = pb.width
  if (pb.style === 'dashed') cx.setLineDash([pb.width * 3, pb.width * 2])
  else if (pb.style === 'dotted') { cx.setLineDash([1, pb.width * 2]); cx.lineCap = 'round' }
  const rect = (inset: number) => cx.strokeRect(inset, inset, gg.pageW - 2 * inset, gg.pageH - 2 * inset)
  rect(m)
  if (pb.style === 'double') rect(m + pb.width * 2)
  cx.restore()
}

// Migration : ancien format string OU 3 zones {l,c,r} → document ProseMirror.
// Un doc existant ({type:'doc'}) est conservé tel quel.
function toHFContent(v: unknown): HFContent {
  if (v && typeof v === 'object') {
    const o = v as Record<string, unknown>
    if (o.type === 'doc') return v as HFContent
    // Anciennes 3 zones → paragraphes alignés (gauche/centre/droite) non vides.
    if ('l' in o || 'c' in o || 'r' in o) {
      const paras: JSONContent[] = []
      const add = (txt: unknown, align: 'left' | 'center' | 'right') => {
        const s = String(txt ?? '')
        if (s) paras.push({ type: 'paragraph', attrs: { textAlign: align }, content: [{ type: 'text', text: s }] })
      }
      add(o.l, 'left'); add(o.c, 'center'); add(o.r, 'right')
      return paras.length ? { type: 'doc', content: paras } : emptyHF()
    }
  }
  if (typeof v === 'string' && v) return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: v }] }] }
  return emptyHF()
}

function parseDocContent(raw: object | null): { sections: SectionDef[]; pages: PageData[] } & DocMeta {
  if (!raw) {
    const sid = newSectionId()
    return { sections: [defaultSection(sid)], pages: [{ id: newPageId(), sectionId: sid, content: emptyDoc() }], pageNumbers: 'none', header: emptyHF(), footer: emptyHF() }
  }
  const r = raw as Record<string, unknown>
  if (r._type === 'multi-page') {
    return {
      sections: r.sections as SectionDef[], pages: r.pages as PageData[],
      pageNumbers: (r.pageNumbers as PageNumbers) ?? 'none',
      header: toHFContent(r.header), footer: toHFContent(r.footer),
      hfFirstPage: !!r.hfFirstPage,
      pageColor: r.pageColor as string | undefined,
      pageGrad:  r.pageGrad as Gradient | undefined,
      paperSize: (r.paperSize as PaperSize) ?? 'a4',
      styles: (r.styles as Record<string, Partial<NamedStyleMeta>>) ?? undefined,
      watermark: (r.watermark as WatermarkDef | null) ?? null,
      pageBorder: (r.pageBorder as PageBorderDef | null) ?? null,
      lineNumbers: (r.lineNumbers as LineNumbersDef | null) ?? null,
      pageNumFormat: (r.pageNumFormat as PageNumFormat) ?? 'arabic',
      pageNumStart: (r.pageNumStart as number) ?? 1,
    }
  }
  const sid = newSectionId()
  return {
    sections: [defaultSection(sid)],
    pages:    [{ id: newPageId(), sectionId: sid, content: raw as JSONContent }],
    pageNumbers: 'none', header: emptyHF(), footer: emptyHF(),
  }
}

function serializeDoc(sections: SectionDef[], pages: PageData[], meta: Partial<DocMeta> = {}): object {
  return { _type: 'multi-page', sections, pages, pageNumbers: meta.pageNumbers ?? 'none',
    header: meta.header ?? emptyHF(), footer: meta.footer ?? emptyHF(),
    hfFirstPage: meta.hfFirstPage ?? false,
    pageColor: meta.pageColor, pageGrad: meta.pageGrad, paperSize: meta.paperSize ?? 'a4',
    styles: meta.styles, watermark: meta.watermark ?? null, pageBorder: meta.pageBorder ?? null,
    lineNumbers: meta.lineNumbers ?? null, pageNumFormat: meta.pageNumFormat ?? 'arabic', pageNumStart: meta.pageNumStart ?? 1 }
}

// Substitue les champs dynamiques ({page}…) dans les nœuds texte d'un doc HF et
// retourne un NOUVEAU doc (l'original n'est pas muté) pour le rendu d'une page.
function expandHFDoc(doc: HFContent, page: number, pages: number, title: string, lang: string, numFmt: PageNumFormat = 'arabic'): HFContent {
  const sub = (s: string) => s
    .replace(/\{page\}/gi, formatPageNumber(page, numFmt))
    .replace(/\{pages\}/gi, formatPageNumber(pages, numFmt))
    .replace(/\{date\}/gi, new Date().toLocaleDateString(lang))
    .replace(/\{titre\}|\{title\}/gi, title)
  const walk = (n: JSONContent): JSONContent => {
    if (n.type === 'text') return { ...n, text: sub(n.text ?? '') }
    if (n.content) return { ...n, content: n.content.map(walk) }
    return n
  }
  return walk(doc)
}

// Substitution des champs dynamiques d'en-tête/pied ({page}, {pages}, {date}, {titre}).
// ── Custom TipTap extensions ───────────────────────────────────────────────────

const FontFamilyExt = Extension.create({
  name: 'fontFamily',
  addGlobalAttributes() {
    return [{
      types: ['textStyle'],
      attributes: {
        fontFamily: {
          default: null,
          parseHTML: el => (el as HTMLElement).style.fontFamily?.replace(/['"]+/g, '') || null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.fontFamily ? { style: `font-family: ${attrs.fontFamily}` } : {},
        },
      },
    }]
  },
})

const FontSizeExt = Extension.create({
  name: 'fontSize',
  addGlobalAttributes() {
    return [{
      types: ['textStyle'],
      attributes: {
        fontSize: {
          default: null,
          parseHTML: el => (el as HTMLElement).style.fontSize || null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.fontSize ? { style: `font-size: ${attrs.fontSize}` } : {},
        },
      },
    }]
  },
})

// Saut de section : nœud-bloc atomique. Il borne les sections du document et
// porte la géométrie (orientation + marges) de la section qui le SUIT. Le canvas
// le lit pour paginer/dimensionner ; l'éditeur caché ne le rend pas visuellement.
const SectionBreakExt = TipTapNode.create({
  name: 'sectionBreak',
  group: 'block',
  atom: true,
  selectable: false,
  draggable: false,
  addAttributes() {
    return {
      orientation: { default: 'portrait' as Orientation },
      top:    { default: 96 },
      right:  { default: 96 },
      bottom: { default: 96 },
      left:   { default: 96 },
      // En-tête / pied PROPRES à la section (Word : « Lier au précédent » désactivé).
      // hfLinked=true (défaut) → hérite de la section précédente / de la base.
      hfLinked:  { default: true },
      header:    { default: null },   // HFContent | null
      footer:    { default: null },
      // Arrière-plan propre à la section (null = couleur globale du document).
      pageColor: { default: null },
    }
  },
  parseHTML() { return [{ tag: 'div[data-section-break]' }] },
  renderHTML({ HTMLAttributes }) {
    return ['div', { 'data-section-break': 'true', style: 'height:0', ...HTMLAttributes }]
  },
})

// Saut de page : nœud-bloc atomique qui force le contenu suivant sur une
// nouvelle page (même section / même géométrie), comme Insertion → Saut → Saut de page.
const PageBreakExt = TipTapNode.create({
  name: 'pageBreak',
  group: 'block',
  atom: true,
  selectable: false,
  draggable: false,
  parseHTML() { return [{ tag: 'div[data-page-break]' }] },
  renderHTML({ HTMLAttributes }) {
    return ['div', { 'data-page-break': 'true', style: 'height:0', ...HTMLAttributes }]
  },
})

// Interligne : attribut `lineHeight` (multiplicateur) sur paragraphes et titres.
// Lu par le moteur canvas (parseDoc → lineSpacing). Réglé via Format → Interligne.
const LineHeightExt = Extension.create({
  name: 'lineHeight',
  addGlobalAttributes() {
    return [{
      types: ['paragraph', 'heading'],
      attributes: {
        lineHeight: {
          default: null,
          parseHTML: el => parseFloat((el as HTMLElement).style.lineHeight) || null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.lineHeight ? { style: `line-height: ${attrs.lineHeight}` } : {},
        },
      },
    }]
  },
})

// Retrait : niveau d'indentation (entier) sur paragraphes/titres. Lu par le
// moteur (parseDoc → indent px). Réglé via les boutons Retrait de la barre d'outils.
const IndentExt = Extension.create({
  name: 'indent',
  addGlobalAttributes() {
    return [{
      types: ['paragraph', 'heading'],
      attributes: {
        indent: {
          default: 0,
          parseHTML: el => Number((el as HTMLElement).dataset.indent) || 0,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.indent ? { 'data-indent': String(attrs.indent) } : {},
        },
      },
    }]
  },
})

// Héritage de mise en forme façon Word : quand le curseur est sur une position
// SANS aucune marque (paragraphe vide, ligne vide d'une sélection…), on pré-charge
// dans les « stored marks » TOUTES les marques de caractère (police, taille,
// couleur, gras, italique, souligné, barré, exposant/indice) du texte précédent
// le plus proche. Ainsi la frappe reprend la mise en forme ambiante au lieu de
// retomber sur du normal/Arial. Corrige « la police/le style change tout seul à
// la ligne » et « le gras appliqué aux lignes vides ne s'applique pas à la frappe ».
const INHERIT_MARK_TYPES = ['textStyle', 'bold', 'italic', 'underline', 'strike', 'superscript', 'subscript']
const InheritFontExt = Extension.create({
  name: 'inheritFont',
  addProseMirrorPlugins() {
    return [new Plugin({
      appendTransaction(_trs, _oldState, newState) {
        const sel = newState.selection
        if (!sel.empty || newState.storedMarks) return null
        // Position déjà marquée (au milieu de texte mis en forme) → ProseMirror
        // hérite naturellement, rien à faire.
        const here = sel.$from.marks()
        if (here.length) return null
        const schema = newState.schema
        // 1) Le paragraphe vide porte-t-il une mise en forme EXPLICITE (fontMarks,
        //    posée via la toolbar sur une sélection de lignes vides) ? → priorité.
        const fm = sel.$from.parent?.attrs?.fontMarks as FontMarks | null | undefined
        if (fm && (fm.ff || fm.fs || fm.b || fm.i || fm.u || fm.s)) {
          const marks = []
          const tsAttrs: Record<string, unknown> = {}
          if (fm.ff) tsAttrs.fontFamily = fm.ff
          if (fm.fs) tsAttrs.fontSize = fm.fs
          if (Object.keys(tsAttrs).length && schema.marks.textStyle) marks.push(schema.marks.textStyle.create(tsAttrs))
          if (fm.b && schema.marks.bold) marks.push(schema.marks.bold.create())
          if (fm.i && schema.marks.italic) marks.push(schema.marks.italic.create())
          if (fm.u && schema.marks.underline) marks.push(schema.marks.underline.create())
          if (fm.s && schema.marks.strike) marks.push(schema.marks.strike.create())
          if (marks.length) return newState.tr.setStoredMarks(marks)
        }
        // 2) Sinon, reprendre les marques du dernier texte AVANT le curseur.
        let inherited: readonly import('@tiptap/pm/model').Mark[] = []
        newState.doc.nodesBetween(0, sel.from, node => {
          if (node.isText && node.marks.length) inherited = node.marks
        })
        const keep = inherited.filter(m => INHERIT_MARK_TYPES.includes(m.type.name))
        if (!keep.length) return null
        return newState.tr.setStoredMarks(keep)
      },
    })]
  },
})

// Mise en forme « portée » par un paragraphe VIDE (façon Word) : un paragraphe
// sans texte ne peut pas porter de marques ; on stocke donc la police/taille/style
// choisie sur l'attribut `fontMarks` du bloc. Le moteur canvas l'utilise pour la
// hauteur de la ligne vide, et la frappe en hérite (cf. InheritFontExt).
// Forme : { ff?, fs?, b?, i?, u?, s? } ou null.
interface FontMarks { ff?: string; fs?: string; b?: boolean; i?: boolean; u?: boolean; s?: boolean }
const FontMarksExt = Extension.create({
  name: 'fontMarks',
  addGlobalAttributes() {
    return [{
      types: ['paragraph', 'heading'],
      attributes: {
        fontMarks: {
          default: null,
          parseHTML: (el: HTMLElement) => { try { return el.dataset.fontMarks ? JSON.parse(el.dataset.fontMarks) : null } catch { return null } },
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.fontMarks ? { 'data-font-marks': JSON.stringify(attrs.fontMarks) } : {},
        },
      },
    }]
  },
})

// Spécification de marques de caractère à appliquer sur des PLAGES (sélection de
// cellules de tableau). `ts` = attributs textStyle (fusionnés par nœud texte).
interface MarkSpec { ts?: { fontFamily?: string; fontSize?: string; color?: string }; bold?: boolean; italic?: boolean; underline?: boolean; strike?: boolean; highlight?: string | null }

// Applique des marques de caractère à plusieurs plages disjointes (cellules) en une
// transaction. Fusionne les attributs textStyle existants (police/taille/couleur)
// au lieu de les écraser, et mémorise la mise en forme sur les paragraphes vides.
function applyMarksAcross(ed: Editor, ranges: Array<{ from: number; to: number }>, spec: MarkSpec): void {
  if (!ranges.length) return
  ed.chain().focus().command(({ tr, state }) => {
    const schema = state.schema
    const tsType = schema.marks.textStyle, hlType = schema.marks.highlight
    const boolTypes: Record<string, import('@tiptap/pm/model').MarkType | undefined> = { bold: schema.marks.bold, italic: schema.marks.italic, underline: schema.marks.underline, strike: schema.marks.strike }
    // 1) textStyle : lire les attrs existants par nœud texte (depuis le doc d'origine) puis fusionner.
    const tsOps: Array<{ from: number; to: number; attrs: Record<string, unknown> }> = []
    if (spec.ts && tsType) {
      for (const r of ranges) state.doc.nodesBetween(r.from, r.to, (node, pos) => {
        if (!node.isText) return
        const f = Math.max(r.from, pos), t = Math.min(r.to, pos + node.nodeSize); if (f >= t) return
        const ex = node.marks.find(m => m.type === tsType)
        tsOps.push({ from: f, to: t, attrs: { ...(ex?.attrs ?? {}), ...spec.ts } })
      })
    }
    for (const o of tsOps) tr.addMark(o.from, o.to, tsType!.create(o.attrs))
    // 2) Marques booléennes (gras/italique/souligné/barré).
    for (const k of Object.keys(boolTypes)) {
      const v = (spec as Record<string, unknown>)[k] as boolean | undefined; const mt = boolTypes[k]
      if (v === undefined || !mt) continue
      for (const r of ranges) { if (v) tr.addMark(r.from, r.to, mt.create()); else tr.removeMark(r.from, r.to, mt) }
    }
    // 3) Surlignage.
    if (spec.highlight !== undefined && hlType) {
      for (const r of ranges) { if (spec.highlight) tr.addMark(r.from, r.to, hlType.create({ color: spec.highlight })); else tr.removeMark(r.from, r.to, hlType) }
    }
    // 4) Paragraphes VIDES de la plage → fontMarks (sinon la mise en forme « se perd »).
    const fm: FontMarks = {}
    if (spec.ts?.fontFamily) fm.ff = spec.ts.fontFamily
    if (spec.ts?.fontSize) fm.fs = spec.ts.fontSize
    if (spec.bold !== undefined) fm.b = spec.bold
    if (spec.italic !== undefined) fm.i = spec.italic
    if (spec.underline !== undefined) fm.u = spec.underline
    if (spec.strike !== undefined) fm.s = spec.strike
    if (Object.keys(fm).length) {
      const empties: Array<{ pos: number; attrs: Record<string, unknown> }> = []
      for (const r of ranges) state.doc.nodesBetween(r.from, r.to, (node, pos) => {
        if ((node.type.name === 'paragraph' || node.type.name === 'heading') && node.content.size === 0) {
          empties.push({ pos, attrs: { ...node.attrs, fontMarks: { ...((node.attrs.fontMarks as FontMarks) ?? {}), ...fm } } })
        }
      })
      for (const e of empties) tr.setNodeMarkup(e.pos, undefined, e.attrs)
    }
    return true
  }).run()
}
// Applique des attributs de bloc (alignement, interligne, espacement) à tous les
// paragraphes/titres contenus dans les plages (cellules sélectionnées).
function applyParaAcross(ed: Editor, ranges: Array<{ from: number; to: number }>, attrs: Record<string, unknown>): void {
  if (!ranges.length) return
  ed.chain().focus().command(({ tr, state }) => {
    const ops: Array<{ pos: number; node: import('@tiptap/pm/model').Node }> = []
    for (const r of ranges) state.doc.nodesBetween(r.from, r.to, (node, pos) => {
      if (node.type.name === 'paragraph' || node.type.name === 'heading') ops.push({ pos, node })
    })
    for (const o of ops) tr.setNodeMarkup(o.pos, undefined, { ...o.node.attrs, ...attrs })
    return true
  }).run()
}

// ── Titres repliables (Word « Développer/Réduire ») ────────────────────────────
// Position du titre cible : celui qui contient le curseur, sinon le plus proche au-dessus.
function headingPosAt(ed: Editor): number | null {
  const $f = ed.state.selection.$from
  for (let d = $f.depth; d >= 0; d--) if ($f.node(d).type.name === 'heading') return $f.before(d)
  let found = -1
  ed.state.doc.descendants((node, pos) => { if (node.type.name === 'heading' && pos < $f.pos) found = pos })
  return found >= 0 ? found : null
}
function setHeadingCollapsed(ed: Editor, pos: number, val: boolean): void {
  const node = ed.state.doc.nodeAt(pos)
  if (node?.type.name !== 'heading') return
  ed.view.dispatch(ed.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, collapsed: val }))
}
function setAllHeadingsCollapsed(ed: Editor, val: boolean): void {
  const tr = ed.state.tr
  ed.state.doc.descendants((node, pos) => { if (node.type.name === 'heading') tr.setNodeMarkup(pos, undefined, { ...node.attrs, collapsed: val }) })
  if (tr.docChanged) ed.view.dispatch(tr)
}

// ── Modifier la casse (Word « Aa ») ─────────────────────────────────────────────
type CaseMode = 'upper' | 'lower' | 'title' | 'sentence' | 'toggle'
// Toutes les transformations préservent la LONGUEUR (donc les positions PM ne
// dérivent pas → on peut remplacer chaque run en place sans recalcul d'offset).
function transformCaseText(s: string, mode: CaseMode): string {
  switch (mode) {
    case 'upper':  return s.toLocaleUpperCase()
    case 'lower':  return s.toLocaleLowerCase()
    case 'title':  return s.replace(/\p{L}[\p{L}'’]*/gu, w => w.charAt(0).toLocaleUpperCase() + w.slice(1).toLocaleLowerCase())
    case 'sentence': return s.toLocaleLowerCase().replace(/(^\s*\p{L})|([.!?…]\s+\p{L})/gu, m => m.toLocaleUpperCase())
    case 'toggle': return Array.from(s).map(c => (c === c.toLocaleLowerCase() ? c.toLocaleUpperCase() : c.toLocaleLowerCase())).join('')
  }
}
function applyCaseTransform(ed: Editor, mode: CaseMode): void {
  const { from, to } = ed.state.selection
  if (from === to) return
  const tr = ed.state.tr
  ed.state.doc.nodesBetween(from, to, (node, pos) => {
    if (!node.isText || !node.text) return
    const a = Math.max(from, pos), b = Math.min(to, pos + node.nodeSize)
    const slice = node.text.slice(a - pos, b - pos)
    const rep = transformCaseText(slice, mode)
    if (rep !== slice) tr.replaceWith(a, b, ed.state.schema.text(rep, node.marks))   // longueur égale → pas de dérive
  })
  if (tr.docChanged) ed.view.dispatch(tr)
}

// Tri alphabétique des paragraphes de la sélection (ou de tout le document).
function sortParagraphs(ed: Editor, dir: 'asc' | 'desc'): void {
  const { from, to } = ed.state.selection
  const wholeDoc = from === to
  const lo = wholeDoc ? 0 : from, hi = wholeDoc ? ed.state.doc.content.size : to
  const blocks: Array<{ pos: number; node: import('@tiptap/pm/model').Node }> = []
  ed.state.doc.nodesBetween(lo, hi, (node, pos) => {
    if (node.type.name === 'paragraph' || node.type.name === 'heading') { blocks.push({ pos, node }); return false }
    return true
  })
  if (blocks.length < 2) return
  const order = blocks.map((b, i) => ({ i, key: (b.node.textContent || '').toLocaleLowerCase() }))
  order.sort((x, y) => (x.key < y.key ? -1 : x.key > y.key ? 1 : 0) * (dir === 'asc' ? 1 : -1))
  const tr = ed.state.tr
  // Remplace chaque emplacement (du dernier au premier pour ne pas dériver) par le
  // contenu du paragraphe trié correspondant.
  for (let k = blocks.length - 1; k >= 0; k--) {
    const target = blocks[k], src = blocks[order[k].i].node
    tr.replaceWith(target.pos, target.pos + target.node.nodeSize, src)
  }
  ed.view.dispatch(tr)
}

// Applique une transformation texte à TOUS les runs (de la fin vers le début pour
// que les longueurs variables ne fassent pas dériver les positions PM restantes).
function transformTextNodes(ed: Editor, fn: (s: string) => string): void {
  const ops: Array<{ from: number; to: number; text: string; marks: readonly import('@tiptap/pm/model').Mark[] }> = []
  ed.state.doc.descendants((node, pos) => {
    if (node.isText && node.text) { const r = fn(node.text); if (r !== node.text) ops.push({ from: pos, to: pos + node.nodeSize, text: r, marks: node.marks }) }
  })
  if (!ops.length) return
  const tr = ed.state.tr
  for (let i = ops.length - 1; i >= 0; i--) { const o = ops[i]; tr.replaceWith(o.from, o.to, o.text ? ed.state.schema.text(o.text, o.marks) : []) }
  ed.view.dispatch(tr)
}
// Supprime les paragraphes vides de premier niveau (en gardant au moins un).
function removeEmptyParagraphs(ed: Editor): void {
  const ops: Array<{ from: number; size: number }> = []
  ed.state.doc.forEach((node, offset) => { if (node.type.name === 'paragraph' && node.content.size === 0) ops.push({ from: offset, size: node.nodeSize }) })
  if (ops.length >= ed.state.doc.childCount) ops.pop()
  if (!ops.length) return
  const tr = ed.state.tr
  for (let i = ops.length - 1; i >= 0; i--) tr.delete(ops[i].from, ops[i].from + ops[i].size)
  ed.view.dispatch(tr)
}
// Guillemets typographiques français : "x" → « x », ' → ’.
function smartQuotes(s: string): string {
  return s.replace(/"([^"]*)"/g, '« $1 »').replace(/'/g, '’')
}

// Blocs (paragraphes/titres) de premier niveau intersectant la sélection (ou tout
// le document si la sélection est vide). Utilitaire commun aux outils de paragraphe.
function selectedBlocks(ed: Editor): Array<{ pos: number; node: import('@tiptap/pm/model').Node }> {
  const { from, to } = ed.state.selection
  const whole = from === to
  const lo = whole ? 0 : from, hi = whole ? ed.state.doc.content.size : to
  const blocks: Array<{ pos: number; node: import('@tiptap/pm/model').Node }> = []
  ed.state.doc.nodesBetween(lo, hi, (node, pos) => {
    if (node.type.name === 'paragraph' || node.type.name === 'heading') { blocks.push({ pos, node }); return false }
    return true
  })
  return blocks
}
// Numérote les paragraphes (« 1. », « 2. »… insérés au début de chaque bloc).
function numberParagraphs(ed: Editor): void {
  const blocks = selectedBlocks(ed).filter(b => b.node.textContent.trim())
  if (!blocks.length) return
  const tr = ed.state.tr
  for (let i = blocks.length - 1; i >= 0; i--) tr.insertText(`${i + 1}. `, blocks[i].pos + 1)
  ed.view.dispatch(tr)
}
// Inverse l'ordre des paragraphes (déplace les nœuds entiers → marques préservées).
function reverseParagraphs(ed: Editor): void {
  const blocks = selectedBlocks(ed)
  if (blocks.length < 2) return
  const tr = ed.state.tr
  for (let k = blocks.length - 1; k >= 0; k--) {
    const target = blocks[k], src = blocks[blocks.length - 1 - k].node
    tr.replaceWith(target.pos, target.pos + target.node.nodeSize, src)
  }
  ed.view.dispatch(tr)
}
// Supprime les paragraphes consécutifs identiques (garde le premier de chaque série).
function dedupeParagraphs(ed: Editor): void {
  const blocks = selectedBlocks(ed)
  const dups: Array<{ pos: number; size: number }> = []
  for (let i = 1; i < blocks.length; i++) {
    if (blocks[i].node.textContent.trim() && blocks[i].node.textContent === blocks[i - 1].node.textContent)
      dups.push({ pos: blocks[i].pos, size: blocks[i].node.nodeSize })
  }
  if (!dups.length) return
  const tr = ed.state.tr
  for (let i = dups.length - 1; i >= 0; i--) tr.delete(dups[i].pos, dups[i].pos + dups[i].size)
  ed.view.dispatch(tr)
}

// Convertit les paragraphes sélectionnés en tableau (1 ligne par paragraphe ;
// colonnes séparées par tabulation, sinon « ; », sinon « , »). Texte brut (façon Word).
function textToTable(ed: Editor): void {
  const blocks = selectedBlocks(ed)
  if (!blocks.length) return
  const lines = blocks.map(b => b.node.textContent)
  const sep = lines.some(l => l.includes('\t')) ? '\t' : lines.some(l => l.includes(';')) ? ';' : ','
  const rows = lines.map(l => l.split(sep).map(c => c.trim()))
  const cols = Math.max(1, ...rows.map(r => r.length))
  const table: JSONContent = {
    type: 'table',
    content: rows.map(r => ({
      type: 'tableRow',
      content: Array.from({ length: cols }, (_, i) => ({ type: 'tableCell', content: [{ type: 'paragraph', content: r[i] ? [{ type: 'text', text: r[i] }] : [] }] })),
    })),
  }
  const from = blocks[0].pos, to = blocks[blocks.length - 1].pos + blocks[blocks.length - 1].node.nodeSize
  ed.chain().focus().insertContentAt({ from, to }, [table, { type: 'paragraph' }]).run()
}
// Convertit le tableau contenant le curseur en paragraphes (cellules séparées par tab).
function tableToText(ed: Editor): void {
  const $f = ed.state.selection.$from
  let tableNode: import('@tiptap/pm/model').Node | null = null, tablePos = -1
  for (let d = $f.depth; d > 0; d--) { if ($f.node(d).type.name === 'table') { tableNode = $f.node(d); tablePos = $f.before(d); break } }
  if (!tableNode || tablePos < 0) return
  const paras: JSONContent[] = []
  tableNode.forEach(row => {
    const cells: string[] = []
    row.forEach(cell => cells.push(cell.textContent))
    const line = cells.join('\t')
    paras.push({ type: 'paragraph', content: line ? [{ type: 'text', text: line }] : [] })
  })
  ed.chain().focus().insertContentAt({ from: tablePos, to: tablePos + tableNode.nodeSize }, paras).run()
}

// ── Format des numéros de page (Word « Format des numéros de page ») ────────────
type PageNumFormat = 'arabic' | 'roman-lower' | 'roman-upper' | 'alpha-lower' | 'alpha-upper'
function toRoman(n: number): string {
  const map: Array<[number, string]> = [[1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'], [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'], [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I']]
  let s = ''
  for (const [v, sym] of map) while (n >= v) { s += sym; n -= v }
  return s
}
function toAlpha(n: number): string { let s = ''; while (n > 0) { n--; s = String.fromCharCode(97 + (n % 26)) + s; n = Math.floor(n / 26) } return s }
function formatPageNumber(n: number, fmt: PageNumFormat = 'arabic'): string {
  if (n < 1) n = 1
  switch (fmt) {
    case 'roman-lower': return toRoman(n).toLowerCase()
    case 'roman-upper': return toRoman(n)
    case 'alpha-lower': return toAlpha(n)
    case 'alpha-upper': return toAlpha(n).toUpperCase()
    default: return String(n)
  }
}

// Date/heure localisée pour l'insertion d'un champ statique dans le corps.
function nowFieldText(kind: 'date' | 'time' | 'datetime', lang: string): string {
  const d = new Date()
  if (kind === 'time') return d.toLocaleTimeString(lang, { hour: '2-digit', minute: '2-digit' })
  if (kind === 'datetime') return `${d.toLocaleDateString(lang)} ${d.toLocaleTimeString(lang, { hour: '2-digit', minute: '2-digit' })}`
  return d.toLocaleDateString(lang, { day: 'numeric', month: 'long', year: 'numeric' })
}

// Applique une mise en forme de caractère à la sélection ET, pour les paragraphes
// VIDES de la plage, l'enregistre dans leur attribut `fontMarks` (sinon rien ne
// s'appliquerait à une sélection de lignes vides — bug Word manquant).
// Si `ranges` est fourni (sélection de cellules de tableau), applique à ces plages.
function applyInlineFormat(editor: Editor, patch: FontMarks, ranges?: Array<{ from: number; to: number }> | null) {
  if (ranges && ranges.length) {
    applyMarksAcross(editor, ranges, { ts: { ...(patch.ff !== undefined ? { fontFamily: patch.ff } : {}), ...(patch.fs !== undefined ? { fontSize: patch.fs } : {}) }, bold: patch.b, italic: patch.i, underline: patch.u, strike: patch.s })
    return
  }
  const chain = editor.chain().focus()
  // 1) Marques normales sur le texte sélectionné.
  if (patch.ff !== undefined) chain.setMark('textStyle', { fontFamily: patch.ff })
  if (patch.fs !== undefined) chain.setMark('textStyle', { fontSize: patch.fs })
  if (patch.b !== undefined) { if (patch.b) chain.setMark('bold'); else chain.unsetMark('bold') }
  if (patch.i !== undefined) { if (patch.i) chain.setMark('italic'); else chain.unsetMark('italic') }
  if (patch.u !== undefined) { if (patch.u) chain.setMark('underline'); else chain.unsetMark('underline') }
  if (patch.s !== undefined) { if (patch.s) chain.setMark('strike'); else chain.unsetMark('strike') }
  // 2) `fontMarks` sur chaque paragraphe VIDE de la plage (ou du curseur seul).
  chain.command(({ tr, state }) => {
    const { from, to, $from } = state.selection
    const tag = (node: import('@tiptap/pm/model').Node, pos: number) => {
      if ((node.type.name === 'paragraph' || node.type.name === 'heading') && node.content.size === 0) {
        const cur = (node.attrs.fontMarks as FontMarks) ?? {}
        tr.setNodeMarkup(pos, undefined, { ...node.attrs, fontMarks: { ...cur, ...patch } })
      }
    }
    state.doc.nodesBetween(from, to, (node, pos) => { tag(node, pos); return true })
    // Curseur seul (sélection vide) dans un paragraphe vide : nodesBetween ne le
    // visite pas toujours → on le marque explicitement.
    if (from === to && $from.parent.content.size === 0) tag($from.parent, $from.before())
    return true
  })
  chain.run()
}

// Espacement de paragraphe (avant/après, px CSS) — lu par le moteur canvas ;
// null = défauts du type de bloc (titre/paragraphe/liste).
const ParagraphSpacingExt = Extension.create({
  name: 'paragraphSpacing',
  addGlobalAttributes() {
    return [{
      types: ['paragraph', 'heading'],
      attributes: {
        spaceBefore: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.spaceBefore != null ? Number(el.dataset.spaceBefore) : null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.spaceBefore != null ? { 'data-space-before': String(attrs.spaceBefore) } : {},
        },
        spaceAfter: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.spaceAfter != null ? Number(el.dataset.spaceAfter) : null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.spaceAfter != null ? { 'data-space-after': String(attrs.spaceAfter) } : {},
        },
        // Retraits de paragraphe (px), façon Word : gauche (toutes lignes sauf 1ʳᵉ),
        // 1ʳᵉ ligne (offset relatif au retrait gauche ; négatif = retrait suspendu), droite.
        indentLeft: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.indentLeft != null ? Number(el.dataset.indentLeft) : null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.indentLeft != null ? { 'data-indent-left': String(attrs.indentLeft) } : {},
        },
        indentFirstLine: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.indentFirstLine != null ? Number(el.dataset.indentFirstLine) : null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.indentFirstLine != null ? { 'data-indent-first-line': String(attrs.indentFirstLine) } : {},
        },
        indentRight: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.indentRight != null ? Number(el.dataset.indentRight) : null,
          renderHTML: (attrs: Record<string, unknown>) =>
            attrs.indentRight != null ? { 'data-indent-right': String(attrs.indentRight) } : {},
        },
        // Taquets de tabulation : tableau de positions (px depuis la marge gauche) +
        // type, sérialisé en JSON dans data-tab-stops. Forme : [{pos, type}].
        tabStops: {
          default: null,
          parseHTML: (el: HTMLElement) => { try { return el.dataset.tabStops ? JSON.parse(el.dataset.tabStops) : null } catch { return null } },
          renderHTML: (attrs: Record<string, unknown>) =>
            Array.isArray(attrs.tabStops) && attrs.tabStops.length ? { 'data-tab-stops': JSON.stringify(attrs.tabStops) } : {},
        },
      },
    }]
  },
})

// Mise en forme avancée du paragraphe (dialogue « Paragraphe… » façon Word) :
// interligne typé (multiple/au moins/exactement), niveau hiérarchique, enchaînements
// (saut de page avant, lignes/paragraphes solidaires, veuves/orphelines), espacement
// contextuel, retraits inversés et exceptions de mise en forme. Tous lus par le moteur
// canvas (interligne + saut + espacement contextuel) ou conservés en métadonnée (le
// reste round-trip et reste fidèle à l'aller-retour DOCX/ODT).
const boolAttr = (data: string) => ({
  default: false,
  parseHTML: (el: HTMLElement) => el.dataset[data] === 'true',
  renderHTML: (attrs: Record<string, unknown>) => (attrs[data] ? { [`data-${data.replace(/[A-Z]/g, m => '-' + m.toLowerCase())}`]: 'true' } : {}),
})
const ParagraphFormatExt = Extension.create({
  name: 'paragraphFormat',
  addGlobalAttributes() {
    return [{
      types: ['paragraph', 'heading'],
      attributes: {
        // Interligne typé : 'multiple' (× via lineHeight), 'atLeast' / 'exactly' (px).
        lineSpacingMode: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.lineSpacingMode || null,
          renderHTML: (a: Record<string, unknown>) => a.lineSpacingMode ? { 'data-line-spacing-mode': String(a.lineSpacingMode) } : {},
        },
        lineSpacingPt: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.lineSpacingPt != null ? Number(el.dataset.lineSpacingPt) : null,
          renderHTML: (a: Record<string, unknown>) => a.lineSpacingPt != null ? { 'data-line-spacing-pt': String(a.lineSpacingPt) } : {},
        },
        // Niveau hiérarchique (0 = Corps de texte ; 1..9 = niveaux de plan).
        outlineLevel: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.outlineLevel != null ? Number(el.dataset.outlineLevel) : null,
          renderHTML: (a: Record<string, unknown>) => a.outlineLevel != null ? { 'data-outline-level': String(a.outlineLevel) } : {},
        },
        // Enchaînements (pagination) + exceptions + options.
        pageBreakBefore:     boolAttr('pageBreakBefore'),
        keepNext:            boolAttr('keepNext'),
        keepLines:           boolAttr('keepLines'),
        // Veuves/orphelines : activé par défaut (Word) → ne se sérialise que si désactivé.
        widowControl: {
          default: true,
          parseHTML: (el: HTMLElement) => el.dataset.widowControl !== 'false',
          renderHTML: (a: Record<string, unknown>) => (a.widowControl === false ? { 'data-widow-control': 'false' } : {}),
        },
        contextualSpacing:   boolAttr('contextualSpacing'),
        mirrorIndents:       boolAttr('mirrorIndents'),
        suppressLineNumbers: boolAttr('suppressLineNumbers'),
        dontHyphenate:       boolAttr('dontHyphenate'),
        // Trame de fond du paragraphe (couleur hex, peinte derrière le texte).
        shading: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.shading || null,
          renderHTML: (a: Record<string, unknown>) => (a.shading ? { 'data-shading': String(a.shading) } : {}),
        },
        // Encadré du paragraphe ({color,width,style} sérialisé JSON) ; null = aucun.
        paraBorder: {
          default: null,
          parseHTML: (el: HTMLElement) => { try { return el.dataset.paraBorder ? JSON.parse(el.dataset.paraBorder) : null } catch { return null } },
          renderHTML: (a: Record<string, unknown>) => (a.paraBorder ? { 'data-para-border': JSON.stringify(a.paraBorder) } : {}),
        },
      },
    }]
  },
})

// Exposant / Indice : marques mutuellement exclusives, lues par le moteur canvas
// (taille réduite + décalage de ligne de base).
const SuperscriptExt = TipTapMark.create({
  name: 'superscript',
  excludes: 'subscript',
  parseHTML() { return [{ tag: 'sup' }] },
  renderHTML() { return ['sup', 0] },
  addKeyboardShortcuts() { return { 'Mod-.': () => this.editor.commands.toggleMark('superscript') } },
})
const SubscriptExt = TipTapMark.create({
  name: 'subscript',
  excludes: 'superscript',
  parseHTML() { return [{ tag: 'sub' }] },
  renderHTML() { return ['sub', 0] },
  addKeyboardShortcuts() { return { 'Mod-,': () => this.editor.commands.toggleMark('subscript') } },
})

// Commentaire : marque ancrée à un intervalle de texte, portant l'id du fil de
// discussion (les données du fil — auteur/texte/réponses — vivent dans une Y.Map
// collaborative séparée). La surbrillance est peinte sur le canvas (cf. moteur) ;
// la marque ne sert qu'à mémoriser l'ancrage, robuste aux éditions concurrentes.
const CommentMark = TipTapMark.create({
  name: 'comment',
  inclusive: false,
  excludes: '',     // peut coexister avec toute autre marque
  addAttributes() {
    return { commentId: { default: null, parseHTML: (el: HTMLElement) => el.dataset.commentId || null, renderHTML: (a: Record<string, unknown>) => a.commentId ? { 'data-comment-id': String(a.commentId) } : {} } }
  },
  parseHTML() { return [{ tag: 'span[data-comment-id]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', { ...HTMLAttributes, class: 'kb-comment' }, 0] },
})

// Signet (Word « Signet ») : marque nommée ancrée à un intervalle, sert de cible de
// navigation (Atteindre / renvois). N'affecte pas le rendu.
const BookmarkMark = TipTapMark.create({
  name: 'bookmark',
  inclusive: false,
  excludes: '',
  addAttributes() {
    return { name: { default: null, parseHTML: (el: HTMLElement) => el.dataset.bookmark || null, renderHTML: (a: Record<string, unknown>) => (a.name ? { 'data-bookmark': String(a.name) } : {}) } }
  },
  parseHTML() { return [{ tag: 'span[data-bookmark]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', { ...HTMLAttributes, class: 'kb-bookmark' }, 0] },
})

// Correction automatique à la frappe (Word « Correction automatique ») : substitue
// des séquences ASCII par leur équivalent typographique dès qu'elles sont tapées.
// L'ordre compte (`-->` avant `->`). Le handler mute `state.tr` (que le plugin
// d'InputRules dispatche). Chaque motif est ancré en fin (`$`) = juste avant le caret.
const AUTOCORRECT_RULES: Array<[RegExp, string]> = [
  [/\(c\)$/i, '©'], [/\(r\)$/i, '®'], [/\(tm\)$/i, '™'],
  [/-->$/, '→'], [/<--$/, '←'], [/->$/, '→'], [/<-$/, '←'],
  [/\.\.\.$/, '…'],
  [/\b1\/2$/, '½'], [/\b1\/4$/, '¼'], [/\b3\/4$/, '¾'],
  [/!=$/, '≠'], [/>=$/, '≥'], [/<=$/, '≤'], [/\+-$/, '±'],
]
const AutoCorrectExt = Extension.create({
  name: 'autoCorrect',
  addInputRules() {
    return AUTOCORRECT_RULES.map(([find, replace]) => new InputRule({
      find,
      handler: ({ state, range }) => { state.tr.insertText(replace, range.from, range.to) },
    }))
  },
})

// Style nommé (façon Word) : attribut `styleName` sur les paragraphes/titres. Il ne
// modifie PAS le rendu (les marques concrètes sont appliquées en même temps que le
// style) ; il sert à retrouver les blocs d'un style donné pour une mise à jour de
// la définition. Survit à la sérialisation via data-style.
const StyleNameExt = Extension.create({
  name: 'styleName',
  addGlobalAttributes() {
    return [{
      types: ['paragraph', 'heading'],
      attributes: {
        styleName: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.style || null,
          renderHTML: (attrs: Record<string, unknown>) => attrs.styleName ? { 'data-style': String(attrs.styleName) } : {},
        },
      },
    }]
  },
})

// Titres repliables (Word « Développer/Réduire ») : attribut `collapsed` sur les
// titres ; le moteur canvas masque le contenu suivant jusqu'au prochain titre de
// niveau ≤. Le triangle ▶/▼ (dessiné dans la marge) bascule l'état.
const HeadingCollapseExt = Extension.create({
  name: 'headingCollapse',
  addGlobalAttributes() {
    return [{
      types: ['heading'],
      attributes: {
        collapsed: {
          default: false,
          parseHTML: (el: HTMLElement) => el.dataset.collapsed === '1',
          renderHTML: (a: Record<string, unknown>) => (a.collapsed ? { 'data-collapsed': '1' } : {}),
        },
      },
    }]
  },
})

// Image : nœud-bloc atomique (src + dimensions optionnelles). Rendu sur le canvas
// par le moteur (chargement async + mise à l'échelle).
const ImageExt = TipTapNode.create({
  name: 'image',
  group: 'block',
  atom: true,
  draggable: false,
  selectable: true,
  addAttributes() {
    return {
      src:      { default: null },
      width:    { default: 0 },
      height:   { default: 0 },
      align:    { default: 'left' },
      rotation: { default: 0 },
      // Habillage du texte (Word « Options de disposition ») :
      //  inline   = aligné sur le texte (bloc pleine ligne, défaut)
      //  square   = habillage carré (le texte coule à côté)
      //  topBottom= haut et bas (texte au-dessus/dessous, image pleine largeur réservée)
      //  behind   = derrière le texte (flottant, texte par-dessus)
      //  front    = devant le texte (flottant, par-dessus le texte)
      wrap:     { default: 'inline' },
      // Décalage du flottant (behind/front/square) p/r au coin haut-gauche de la
      // zone de contenu, en px doc. Posé au glisser.
      wrapX:    { default: 0 },
      wrapY:    { default: 0 },
      // Métadonnées Kubuno (formes / zones de texte SVG rééditables) : 'kbshape:…'
      // ou 'kbtext:…' — sinon texte alternatif standard.
      alt:      { default: null },
      // Zone de texte riche : couleur de remplissage / de bordure (null = défaut).
      tbFill:   { default: null },
      tbStroke: { default: null },
      // ── Options de disposition avancées (dialogue « Mise en page », façon Word) ──
      // Habillage carré : côté où le texte s'écoule + distances objet↔texte (px doc).
      wrapSide: { default: 'both' },     // both | left | right | largest
      wrapDistT:{ default: 0 }, wrapDistB:{ default: 0 },
      wrapDistL:{ default: 10 }, wrapDistR:{ default: 10 },
      // Référentiels de position (affichage façon Word) + options d'ancrage.
      posHRel:  { default: 'column' },   // column | margin | page | character
      posVRel:  { default: 'paragraph' },// paragraph | margin | page | line
      moveWithText:  { default: true },
      allowOverlap:  { default: true },
      lockAnchor:    { default: false },
    }
  },
  parseHTML() { return [{ tag: 'img[src]' }] },
  renderHTML({ HTMLAttributes }) { return ['img', HTMLAttributes] },
})

// Image/forme « alignée sur le texte » (inline) : nœud INLINE atomique → traité comme
// un caractère dans le flux (le moteur canvas réserve sa largeur ET sa hauteur sur la
// ligne). `alt` peut porter `kbshape:…` (forme inline). Distinct du nœud `image` (bloc).
const InlineImageExt = TipTapNode.create({
  name: 'inlineImage',
  group: 'inline',
  inline: true,
  atom: true,
  draggable: true,
  selectable: true,
  addAttributes() {
    return {
      src:      { default: '' },
      width:    { default: 0 },
      height:   { default: 0 },
      alt:      { default: null },
      rotation: { default: 0 },
    }
  },
  parseHTML() { return [{ tag: 'img[data-inline-image]' }] },
  renderHTML({ HTMLAttributes }) { return ['img', { ...HTMLAttributes, 'data-inline-image': '' }] },
})

// ── Formes & zones de texte (SVG vectoriel porté par le nœud image) ───────────
// Une forme = image SVG data-URL paramétrique → bénéficie de TOUTE la machinerie
// image existante (sélection, redimensionnement, rotation, alignement, export).
// Galerie de formes façon Word (Insertion → Formes), réparties par catégorie.
export type ShapeKind =
  // Traits
  | 'line' | 'lineArrow' | 'lineDouble' | 'elbowConnector' | 'elbowArrow' | 'elbowDoubleArrow'
  | 'curveConnector' | 'curveArrow' | 'curveDoubleArrow' | 'curve'
  // Rectangles
  | 'rect' | 'roundRect' | 'snipRect' | 'snip2SameRect' | 'snip2DiagRect' | 'snipRoundRect'
  | 'roundRect1' | 'round2SameRect' | 'round2DiagRect' | 'plaque' | 'frame'
  // Formes de base
  | 'ellipse' | 'triangle' | 'rtTriangle' | 'parallelogram' | 'trapezoid' | 'diamond'
  | 'pentagon' | 'hexagon' | 'heptagon' | 'octagon' | 'decagon' | 'dodecagon'
  | 'pie' | 'chord' | 'teardrop' | 'halfFrame' | 'corner' | 'diagStripe'
  | 'cross' | 'bevel' | 'cylinder' | 'cube' | 'blockArc' | 'foldedCorner'
  | 'heart' | 'lightning' | 'sun' | 'moon' | 'cloud' | 'smiley' | 'arc' | 'donut' | 'noSymbol'
  | 'leftBrace' | 'rightBrace' | 'leftBracket' | 'rightBracket' | 'doubleBrace' | 'doubleBracket'
  // Flèches pleines
  | 'arrow' | 'arrowLeft' | 'arrowUp' | 'arrowDown' | 'arrowLeftRight' | 'arrowUpDown'
  | 'arrowQuad' | 'leftRightUpArrow' | 'chevron' | 'pentagonArrow' | 'bentArrow' | 'bentUpArrow'
  | 'uTurnArrow' | 'curvedRightArrow' | 'curvedLeftArrow' | 'curvedUpArrow' | 'curvedDownArrow'
  | 'stripedRightArrow' | 'notchedArrow' | 'circularArrow'
  | 'rightArrowCallout' | 'leftArrowCallout' | 'upArrowCallout' | 'downArrowCallout'
  // Formes d'équation
  | 'mathPlus' | 'mathMinus' | 'mathMultiply' | 'mathDivide' | 'mathEqual' | 'mathNotEqual'
  // Organigrammes
  | 'flowProcess' | 'flowAltProcess' | 'flowDecision' | 'flowData' | 'flowPredefined'
  | 'flowInternal' | 'flowDocument' | 'flowMultidoc' | 'flowTerminator' | 'flowPreparation'
  | 'flowManualInput' | 'flowManualOp' | 'flowConnector' | 'flowCard' | 'flowPunchedTape' | 'flowOr'
  | 'flowSumming' | 'flowCollate' | 'flowSort' | 'flowExtract' | 'flowMerge' | 'flowStored'
  | 'flowSequential' | 'flowMagneticDisk' | 'flowDirectAccess' | 'flowDisplay'
  | 'flowDelay' | 'flowOffPage'
  // Étoiles et bannières
  | 'star4' | 'star' | 'star6' | 'star7' | 'star8' | 'star10' | 'star12' | 'star16' | 'star24' | 'star32'
  | 'explosion1' | 'explosion2' | 'ribbon' | 'ribbonDown' | 'ribbonCurved'
  | 'scrollH' | 'scrollV' | 'wave' | 'doubleWave'
  // Bulles et légendes
  | 'calloutRect' | 'calloutRoundRect' | 'calloutOval' | 'calloutCloud'
  | 'lineCallout' | 'calloutLine2' | 'calloutLineAccent'

// `sw` = épaisseur de contour en FRACTION de la plus petite dimension (sans unité),
// pour rester nette à toute résolution de génération. Absente = épaisseur par défaut.
interface ShapeParams { kind: ShapeKind; fill: string; stroke: string; sw?: number }

// Sommets d'un polygone régulier à n côtés (rotDeg = orientation du 1er sommet).
function polyPts(n: number, cx: number, cy: number, rx: number, ry: number, rotDeg: number): string {
  let p = ''
  for (let i = 0; i < n; i++) {
    const a = (Math.PI * 2 * i) / n + (rotDeg * Math.PI) / 180
    p += `${(cx + rx * Math.cos(a)).toFixed(1)},${(cy + ry * Math.sin(a)).toFixed(1)} `
  }
  return p.trim()
}
// Sommets d'une étoile à `spikes` branches (rayons externe oR / interne iR).
function starPts(spikes: number, cx: number, cy: number, oR: number, iR: number): string {
  let p = ''
  for (let i = 0; i < spikes * 2; i++) {
    const r = i % 2 === 0 ? oR : iR
    const a = (Math.PI / spikes) * i - Math.PI / 2
    p += `${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)} `
  }
  return p.trim()
}

function shapeSvg(kind: ShapeKind, w: number, h: number, fill = '#dbe7ff', stroke = '#1a73e8', swFrac?: number): string {
  const cx = w / 2, cy = h / 2
  const f = fill, s = stroke
  const m = Math.min(w, h)
  // Épaisseur de contour proportionnelle à la taille : l'apparence reste identique
  // que le SVG soit généré à la taille du nœud ou à la résolution périphérique
  // (rendu net à tout zoom). `swFrac` (fourni par l'import DOCX `<a:ln w>`) prime,
  // sinon ≈1 pt façon Word (avant : ≈2 px, jugé trop épais à l'import).
  const sw = swFrac != null ? Math.max(0.5, m * swFrac) : Math.max(1, m * 0.0075)
  const oR = m / 2 - sw
  const A = `fill="${f}" stroke="${s}" stroke-width="${sw}" stroke-linejoin="round"`
  // Épaisseur des connecteurs : fine et proportionnelle à la taille (façon Word).
  const cs = Math.max(1.5, Math.min(m * 0.045, 3.5))
  const AL = `fill="none" stroke="${s}" stroke-width="${cs.toFixed(2)}" stroke-linecap="round" stroke-linejoin="round"`
  const poly = (pts: string) => `<polygon points="${pts}" ${A}/>`
  const path = (d: string) => `<path d="${d}" ${A}/>`
  const circ = (ccx: number, ccy: number, r: number) => `<circle cx="${ccx.toFixed(1)}" cy="${ccy.toFixed(1)}" r="${r.toFixed(1)}" ${A}/>`
  const sline = (x1: number, y1: number, x2: number, y2: number, sw2 = sw) => `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="${s}" stroke-width="${sw2}" stroke-linecap="round"/>`
  // Croix (signe +) couvrant la boîte, épaisseur de bras `tk`.
  const crossPoly = (tk: number) => {
    const x1 = cx - tk / 2, x2 = cx + tk / 2, y1 = cy - tk / 2, y2 = cy + tk / 2
    return `${x1},${sw} ${x2},${sw} ${x2},${y1} ${w - sw},${y1} ${w - sw},${y2} ${x2},${y2} ${x2},${h - sw} ${x1},${h - sw} ${x1},${y2} ${sw},${y2} ${sw},${y1} ${x1},${y1}`
  }
  // Têtes de flèche FINES pour les connecteurs (traits) : taille absolue
  // (`userSpaceOnUse`) proportionnelle à la forme et plafonnée, donc nettes en
  // vignette comme à taille réelle (au lieu de têtes géantes « blob »).
  const ahS = Math.max(5, Math.min(m * 0.3, 20))
  const arrowHead = `<defs><marker id="ah" markerUnits="userSpaceOnUse" markerWidth="${ahS.toFixed(1)}" markerHeight="${ahS.toFixed(1)}" refX="${(ahS * 0.88).toFixed(1)}" refY="${(ahS / 2).toFixed(1)}" orient="auto"><path d="M0,0 L${ahS.toFixed(1)},${(ahS / 2).toFixed(1)} L0,${ahS.toFixed(1)} Z" fill="${s}"/></marker><marker id="ah0" markerUnits="userSpaceOnUse" markerWidth="${ahS.toFixed(1)}" markerHeight="${ahS.toFixed(1)}" refX="${(ahS * 0.12).toFixed(1)}" refY="${(ahS / 2).toFixed(1)}" orient="auto"><path d="M${ahS.toFixed(1)},0 L0,${(ahS / 2).toFixed(1)} L${ahS.toFixed(1)},${ahS.toFixed(1)} Z" fill="${s}"/></marker></defs>`
  // Tête ÉPAISSE pour les flèches courbes/circulaires (proportionnelle au fût épais).
  const arrowHeadThick = `<defs><marker id="aht" markerWidth="2.6" markerHeight="2.6" refX="2" refY="1.3" orient="auto"><path d="M0,0 L2.6,1.3 L0,2.6 Z" fill="${s}"/></marker></defs>`
  let body = ''
  switch (kind) {
    // ── Traits ────────────────────────────────────────────────────────────────
    case 'line':     body = sline(sw, h - sw, w - sw, sw, cs); break
    case 'lineArrow': body = `${arrowHead}<line x1="${sw}" y1="${h - sw}" x2="${w - sw}" y2="${sw}" ${AL} marker-end="url(#ah)"/>`; break
    case 'lineDouble': body = `${arrowHead}<line x1="${sw}" y1="${h - sw}" x2="${w - sw}" y2="${sw}" ${AL} marker-start="url(#ah0)" marker-end="url(#ah)"/>`; break
    case 'elbowConnector': body = `<polyline points="${sw},${h - sw} ${cx},${h - sw} ${cx},${sw} ${w - sw},${sw}" ${AL}/>`; break
    case 'curveConnector': body = `<path d="M ${sw},${h - sw} C ${w * 0.1},${h * 0.15} ${w * 0.9},${h * 0.85} ${w - sw},${sw}" ${AL}/>`; break
    case 'curve': body = `<path d="M ${sw},${h - sw} Q ${cx},${sw} ${w - sw},${h - sw}" ${AL}/>`; break
    // ── Rectangles ────────────────────────────────────────────────────────────
    case 'rect':      body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>`; break
    case 'roundRect': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${m * 0.14}" ${A}/>`; break
    case 'snipRect': { const c = m * 0.22; body = poly(`${sw},${sw} ${w - c},${sw} ${w - sw},${c} ${w - sw},${h - sw} ${sw},${h - sw}`); break }
    case 'roundRect1': { const c = m * 0.28; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - sw},${sw} L ${w - sw},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'frame': { const t = m * 0.16; body = `<path fill-rule="evenodd" d="M ${sw},${sw} H ${w - sw} V ${h - sw} H ${sw} Z M ${sw + t},${sw + t} H ${w - sw - t} V ${h - sw - t} H ${sw + t} Z" ${A}/>`; break }
    // ── Formes de base ────────────────────────────────────────────────────────
    case 'ellipse':   body = `<ellipse cx="${cx}" cy="${cy}" rx="${cx - sw}" ry="${cy - sw}" ${A}/>`; break
    case 'triangle':  body = poly(`${cx},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break
    case 'rtTriangle': body = poly(`${sw},${sw} ${sw},${h - sw} ${w - sw},${h - sw}`); break
    case 'parallelogram': { const o = w * 0.22; body = poly(`${o},${sw} ${w - sw},${sw} ${w - o},${h - sw} ${sw},${h - sw}`); break }
    case 'trapezoid': { const o = w * 0.22; body = poly(`${o},${sw} ${w - o},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break }
    case 'diamond':   body = poly(`${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}`); break
    case 'pentagon':  body = poly(polyPts(5, cx, cy, cx - sw, cy - sw, -90)); break
    case 'hexagon':   body = poly(polyPts(6, cx, cy, cx - sw, cy - sw, 0)); break
    case 'heptagon':  body = poly(polyPts(7, cx, cy, cx - sw, cy - sw, -90)); break
    case 'octagon':   body = poly(polyPts(8, cx, cy, cx - sw, cy - sw, 22.5)); break
    case 'cross':     body = poly(crossPoly(m * 0.36)); break
    case 'cylinder': { const ry = Math.min(h * 0.14, cy - sw), topY = sw + ry, botY = h - sw - ry; body = path(`M ${sw},${topY} L ${sw},${botY} A ${cx - sw} ${ry} 0 0 0 ${w - sw},${botY} L ${w - sw},${topY}`) + `<ellipse cx="${cx}" cy="${topY}" rx="${cx - sw}" ry="${ry}" ${A}/>`; break }
    case 'cube': { const d = m * 0.26; body = poly(`${sw},${d} ${w - d},${d} ${w - d},${h - sw} ${sw},${h - sw}`) + poly(`${sw},${d} ${d},${sw} ${w - sw},${sw} ${w - d},${d}`) + poly(`${w - d},${d} ${w - sw},${sw} ${w - sw},${h - d} ${w - d},${h - sw}`); break }
    case 'heart': body = path(`M ${cx},${h * 0.85} C ${w * 0.1},${h * 0.55} ${w * 0.18},${h * 0.12} ${cx},${h * 0.32} C ${w * 0.82},${h * 0.12} ${w * 0.9},${h * 0.55} ${cx},${h * 0.85} Z`); break
    case 'lightning': body = poly(`${w * 0.42},${sw} ${w * 0.66},${h * 0.42} ${w * 0.5},${h * 0.46} ${w * 0.7},${h - sw} ${w * 0.32},${h * 0.56} ${w * 0.48},${h * 0.52} ${w * 0.3},${sw}`); break
    case 'sun': { const r = m * 0.26; let rays = ''; for (let i = 0; i < 12; i++) { const a = (Math.PI * 2 * i) / 12; rays += sline(cx + r * 1.18 * Math.cos(a), cy + r * 1.18 * Math.sin(a), cx + r * 1.5 * Math.cos(a), cy + r * 1.5 * Math.sin(a)) } body = rays + circ(cx, cy, r); break }
    case 'moon': body = path(`M ${w * 0.72},${sw} A ${cx} ${cy - sw} 0 1 0 ${w * 0.72},${h - sw} A ${cx * 0.66} ${cy - sw} 0 1 1 ${w * 0.72},${sw} Z`); break
    case 'cloud': body = path(`M ${w * 0.30},${h * 0.80} C ${w * 0.12},${h * 0.80} ${w * 0.10},${h * 0.55} ${w * 0.24},${h * 0.50} C ${w * 0.20},${h * 0.30} ${w * 0.45},${h * 0.22} ${w * 0.50},${h * 0.38} C ${w * 0.58},${h * 0.20} ${w * 0.85},${h * 0.26} ${w * 0.80},${h * 0.48} C ${w * 0.95},${h * 0.50} ${w * 0.93},${h * 0.78} ${w * 0.74},${h * 0.80} Z`); break
    case 'smiley': { const r = m / 2 - sw; body = circ(cx, cy, r) + `<circle cx="${(cx - r * 0.35).toFixed(1)}" cy="${(cy - r * 0.2).toFixed(1)}" r="${(r * 0.09).toFixed(1)}" fill="${s}"/>` + `<circle cx="${(cx + r * 0.35).toFixed(1)}" cy="${(cy - r * 0.2).toFixed(1)}" r="${(r * 0.09).toFixed(1)}" fill="${s}"/>` + `<path d="M ${cx - r * 0.42},${cy + r * 0.18} Q ${cx},${cy + r * 0.6} ${cx + r * 0.42},${cy + r * 0.18}" fill="none" stroke="${s}" stroke-width="${sw}" stroke-linecap="round"/>`; break }
    case 'arc': body = path(`M ${w - sw},${cy} A ${cx - sw} ${cy - sw} 0 0 0 ${cx},${sw} L ${cx},${cy} Z`); break
    case 'donut': { const ro = oR, ri = ro * 0.55; body = `<path fill-rule="evenodd" ${A} d="M ${cx - ro},${cy} a ${ro} ${ro} 0 1 0 ${ro * 2},0 a ${ro} ${ro} 0 1 0 ${-ro * 2},0 Z M ${cx - ri},${cy} a ${ri} ${ri} 0 1 1 ${ri * 2},0 a ${ri} ${ri} 0 1 1 ${-ri * 2},0 Z"/>`; break }
    case 'noSymbol': { const ro = oR, ri = ro * 0.72, off = ri * Math.SQRT1_2; body = `<path fill-rule="evenodd" ${A} d="M ${cx - ro},${cy} a ${ro} ${ro} 0 1 0 ${ro * 2},0 a ${ro} ${ro} 0 1 0 ${-ro * 2},0 Z M ${cx - ri},${cy} a ${ri} ${ri} 0 1 1 ${ri * 2},0 a ${ri} ${ri} 0 1 1 ${-ri * 2},0 Z"/>` + sline(cx - off, cy + off, cx + off, cy - off, ro - ri); break }
    case 'leftBrace':  body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w - sw},${sw} Q ${cx},${sw} ${cx},${cy * 0.5} Q ${cx},${cy} ${sw},${cy} Q ${cx},${cy} ${cx},${cy * 1.5} Q ${cx},${h - sw} ${w - sw},${h - sw}"/>`; break
    case 'rightBrace': body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${sw},${sw} Q ${cx},${sw} ${cx},${cy * 0.5} Q ${cx},${cy} ${w - sw},${cy} Q ${cx},${cy} ${cx},${cy * 1.5} Q ${cx},${h - sw} ${sw},${h - sw}"/>`; break
    case 'leftBracket':  body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.6},${sw} L ${sw},${sw} L ${sw},${h - sw} L ${w * 0.6},${h - sw}"/>`; break
    case 'rightBracket': body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.4},${sw} L ${w - sw},${sw} L ${w - sw},${h - sw} L ${w * 0.4},${h - sw}"/>`; break
    // ── Flèches pleines ───────────────────────────────────────────────────────
    case 'arrow': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${sw},${cy - sh / 2} ${w - hw},${cy - sh / 2} ${w - hw},${sw} ${w - sw},${cy} ${w - hw},${h - sw} ${w - hw},${cy + sh / 2} ${sw},${cy + sh / 2}`); break }
    case 'arrowLeft': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${w - sw},${cy - sh / 2} ${hw},${cy - sh / 2} ${hw},${sw} ${sw},${cy} ${hw},${h - sw} ${hw},${cy + sh / 2} ${w - sw},${cy + sh / 2}`); break }
    case 'arrowUp': { const sv = w * 0.34, hh = h * 0.4; body = poly(`${cx - sv / 2},${h - sw} ${cx - sv / 2},${hh} ${sw},${hh} ${cx},${sw} ${w - sw},${hh} ${cx + sv / 2},${hh} ${cx + sv / 2},${h - sw}`); break }
    case 'arrowDown': { const sv = w * 0.34, hh = h * 0.6; body = poly(`${cx - sv / 2},${sw} ${cx - sv / 2},${hh} ${sw},${hh} ${cx},${h - sw} ${w - sw},${hh} ${cx + sv / 2},${hh} ${cx + sv / 2},${sw}`); break }
    case 'arrowLeftRight': { const sh = h * 0.34, hw = w * 0.24, sv = h * 0.17; body = poly(`${sw},${cy} ${hw},${cy - sh / 2} ${hw},${cy - sv} ${w - hw},${cy - sv} ${w - hw},${cy - sh / 2} ${w - sw},${cy} ${w - hw},${cy + sh / 2} ${w - hw},${cy + sv} ${hw},${cy + sv} ${hw},${cy + sh / 2}`); break }
    case 'arrowUpDown': { const hw = w * 0.34, ss = w * 0.17, vh = h * 0.24; body = poly(`${cx},${sw} ${cx + hw},${vh} ${cx + ss},${vh} ${cx + ss},${h - vh} ${cx + hw},${h - vh} ${cx},${h - sw} ${cx - hw},${h - vh} ${cx - ss},${h - vh} ${cx - ss},${vh} ${cx - hw},${vh}`); break }
    case 'arrowQuad': { const sh = m * 0.13, hl = m * 0.24, hx = w * 0.28, hy = h * 0.28; body = poly(`${cx},${sw} ${cx + hl},${hy} ${cx + sh},${hy} ${cx + sh},${cy - sh} ${w - hx},${cy - sh} ${w - hx},${cy - hl} ${w - sw},${cy} ${w - hx},${cy + hl} ${w - hx},${cy + sh} ${cx + sh},${cy + sh} ${cx + sh},${h - hy} ${cx + hl},${h - hy} ${cx},${h - sw} ${cx - hl},${h - hy} ${cx - sh},${h - hy} ${cx - sh},${cy + sh} ${hx},${cy + sh} ${hx},${cy + hl} ${sw},${cy} ${hx},${cy - hl} ${hx},${cy - sh} ${cx - sh},${cy - sh} ${cx - sh},${hy} ${cx - hl},${hy}`); break }
    case 'chevron': { const o = w * 0.28; body = poly(`${sw},${sw} ${w - o},${sw} ${w - sw},${cy} ${w - o},${h - sw} ${sw},${h - sw} ${o},${cy}`); break }
    case 'pentagonArrow': { const o = w * 0.3; body = poly(`${sw},${sw} ${w - o},${sw} ${w - sw},${cy} ${w - o},${h - sw} ${sw},${h - sw}`); break }
    case 'bentArrow': { const yTop = h * 0.16, t = m * 0.2, mid = yTop + t / 2, hh = t * 1.5, bx = w * 0.64; body = poly(`${sw},${h - sw} ${sw},${yTop} ${bx},${yTop} ${bx},${mid - hh} ${w - sw},${mid} ${bx},${mid + hh} ${bx},${yTop + t} ${sw + t},${yTop + t} ${sw + t},${h - sw}`); break }
    // ── Formes d'équation ─────────────────────────────────────────────────────
    case 'mathPlus':  body = poly(crossPoly(m * 0.28)); break
    case 'mathMinus': body = `<rect x="${w * 0.14}" y="${cy - m * 0.1}" width="${w * 0.72}" height="${m * 0.2}" rx="${m * 0.05}" ${A}/>`; break
    case 'mathMultiply': { const tw = m * 0.2; body = sline(w * 0.22, h * 0.22, w * 0.78, h * 0.78, tw) + sline(w * 0.78, h * 0.22, w * 0.22, h * 0.78, tw); break }
    case 'mathDivide': { const bh = m * 0.14, r = m * 0.09; body = `<rect x="${w * 0.16}" y="${cy - bh / 2}" width="${w * 0.68}" height="${bh}" rx="${bh * 0.3}" ${A}/>` + circ(cx, cy - h * 0.26, r) + circ(cx, cy + h * 0.26, r); break }
    case 'mathEqual': { const bh = m * 0.15, g = m * 0.16; body = `<rect x="${w * 0.14}" y="${cy - g / 2 - bh}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/><rect x="${w * 0.14}" y="${cy + g / 2}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/>`; break }
    case 'mathNotEqual': { const bh = m * 0.14, g = m * 0.16; body = `<rect x="${w * 0.14}" y="${cy - g / 2 - bh}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/><rect x="${w * 0.14}" y="${cy + g / 2}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/>` + sline(w * 0.62, h * 0.16, w * 0.38, h * 0.84, m * 0.1); break }
    // ── Organigrammes ─────────────────────────────────────────────────────────
    case 'flowProcess':    body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>`; break
    case 'flowAltProcess': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${m * 0.22}" ${A}/>`; break
    case 'flowDecision':   body = poly(`${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}`); break
    case 'flowData':       { const o = w * 0.22; body = poly(`${o},${sw} ${w - sw},${sw} ${w - o},${h - sw} ${sw},${h - sw}`); break }
    case 'flowPredefined': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>` + sline(w * 0.12, sw, w * 0.12, h - sw) + sline(w * 0.88, sw, w * 0.88, h - sw); break
    case 'flowInternal':   body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>` + sline(sw, h * 0.26, w - sw, h * 0.26) + sline(w * 0.2, sw, w * 0.2, h - sw); break
    case 'flowDocument':   body = path(`M ${sw},${sw} L ${w - sw},${sw} L ${w - sw},${h * 0.8} Q ${w * 0.75},${h * 0.97} ${cx},${h * 0.8} Q ${w * 0.25},${h * 0.63} ${sw},${h * 0.8} Z`); break
    case 'flowMultidoc': { const o = m * 0.1; body = `<rect x="${sw + 2 * o}" y="${sw}" width="${w - 2 * sw - 2 * o}" height="${h * 0.55}" ${A}/>` + `<rect x="${sw + o}" y="${sw + o}" width="${w - 2 * sw - 2 * o}" height="${h * 0.58}" ${A}/>` + path(`M ${sw},${sw + 2 * o} L ${w - sw - 2 * o},${sw + 2 * o} L ${w - sw - 2 * o},${h * 0.78} Q ${(w - 2 * o) * 0.62},${h * 0.95} ${(w - 2 * o) / 2},${h * 0.78} Q ${w * 0.25},${h * 0.62} ${sw},${h * 0.78} Z`); break }
    case 'flowTerminator': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${(h - 2 * sw) / 2}" ${A}/>`; break
    case 'flowPreparation': body = poly(`${w * 0.16},${sw} ${w - w * 0.16},${sw} ${w - sw},${cy} ${w - w * 0.16},${h - sw} ${w * 0.16},${h - sw} ${sw},${cy}`); break
    case 'flowManualInput': body = poly(`${sw},${h * 0.28} ${w - sw},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break
    case 'flowManualOp': { const o = w * 0.18; body = poly(`${sw},${sw} ${w - sw},${sw} ${w - o},${h - sw} ${o},${h - sw}`); break }
    case 'flowConnector': body = circ(cx, cy, m / 2 - sw); break
    case 'flowCard': { const c = m * 0.22; body = poly(`${c},${sw} ${w - sw},${sw} ${w - sw},${h - sw} ${sw},${h - sw} ${sw},${c}`); break }
    case 'flowOr': { const r = m / 2 - sw; body = circ(cx, cy, r) + sline(cx - r, cy, cx + r, cy) + sline(cx, cy - r, cx, cy + r); break }
    case 'flowSumming': { const r = m / 2 - sw, o = r * Math.SQRT1_2; body = circ(cx, cy, r) + sline(cx - o, cy - o, cx + o, cy + o) + sline(cx + o, cy - o, cx - o, cy + o); break }
    case 'flowCollate': body = poly(`${sw},${sw} ${w - sw},${sw} ${cx},${cy}`) + poly(`${sw},${h - sw} ${w - sw},${h - sw} ${cx},${cy}`); break
    case 'flowExtract': body = poly(`${cx},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break
    case 'flowMerge':   body = poly(`${sw},${sw} ${w - sw},${sw} ${cx},${h - sw}`); break
    case 'flowStored':  body = path(`M ${w * 0.14},${sw} L ${w - sw},${sw} Q ${w - w * 0.16},${cy} ${w - sw},${h - sw} L ${w * 0.14},${h - sw} Q ${w * 0.26},${cy} ${w * 0.14},${sw} Z`); break
    case 'flowDelay':   body = path(`M ${sw},${sw} L ${cx},${sw} A ${cx - sw} ${cy - sw} 0 0 1 ${cx},${h - sw} L ${sw},${h - sw} Z`); break
    case 'flowOffPage': body = poly(`${sw},${sw} ${w - sw},${sw} ${w - sw},${h * 0.65} ${cx},${h - sw} ${sw},${h * 0.65}`); break
    // ── Étoiles et bannières ──────────────────────────────────────────────────
    case 'star4':  body = poly(starPts(4, cx, cy, oR, oR * 0.38)); break
    case 'star':   body = poly(starPts(5, cx, cy, oR, oR * 0.42)); break
    case 'star6':  body = poly(starPts(6, cx, cy, oR, oR * 0.5)); break
    case 'star8':  body = poly(starPts(8, cx, cy, oR, oR * 0.6)); break
    case 'star16': body = poly(starPts(16, cx, cy, oR, oR * 0.78)); break
    case 'star24': body = poly(starPts(24, cx, cy, oR, oR * 0.82)); break
    case 'star32': body = poly(starPts(32, cx, cy, oR, oR * 0.85)); break
    case 'explosion1': body = poly(starPts(10, cx, cy, oR, oR * 0.42)); break
    case 'explosion2': body = poly(starPts(14, cx, cy, oR, oR * 0.55)); break
    case 'ribbon': { const ty = h * 0.25, by = h * 0.75, notch = w * 0.08; body = poly(`${sw},${ty} ${w - sw},${ty} ${w - sw - notch},${cy} ${w - sw},${by} ${sw},${by} ${sw + notch},${cy}`); break }
    case 'ribbonDown': body = `<rect x="${w * 0.2}" y="${sw}" width="${w * 0.6}" height="${h * 0.55}" ${A}/>` + poly(`${w * 0.2},${h * 0.42} ${w * 0.1},${h - sw} ${w * 0.2},${h * 0.78}`) + poly(`${w * 0.8},${h * 0.42} ${w * 0.9},${h - sw} ${w * 0.8},${h * 0.78}`); break
    case 'wave': { const a = h * 0.16; body = path(`M ${sw},${h * 0.3} Q ${w * 0.25},${h * 0.3 - a} ${cx},${h * 0.3} T ${w - sw},${h * 0.3} L ${w - sw},${h * 0.7} Q ${w * 0.75},${h * 0.7 + a} ${cx},${h * 0.7} T ${sw},${h * 0.7} Z`); break }
    // ── Bulles et légendes ────────────────────────────────────────────────────
    case 'calloutRect': body = path(`M ${sw},${sw} L ${w - sw},${sw} L ${w - sw},${h * 0.7} L ${w * 0.42},${h * 0.7} L ${w * 0.2},${h - sw} L ${w * 0.26},${h * 0.7} L ${sw},${h * 0.7} Z`); break
    case 'calloutRoundRect': { const c = m * 0.16; body = path(`M ${sw + c},${sw} L ${w - sw - c},${sw} Q ${w - sw},${sw} ${w - sw},${sw + c} L ${w - sw},${h * 0.7 - c} Q ${w - sw},${h * 0.7} ${w - sw - c},${h * 0.7} L ${w * 0.42},${h * 0.7} L ${w * 0.2},${h - sw} L ${w * 0.26},${h * 0.7} L ${sw + c},${h * 0.7} Q ${sw},${h * 0.7} ${sw},${h * 0.7 - c} L ${sw},${sw + c} Q ${sw},${sw} ${sw + c},${sw} Z`); break }
    case 'calloutOval': body = `<ellipse cx="${cx}" cy="${h * 0.4}" rx="${cx - sw}" ry="${h * 0.36}" ${A}/>` + poly(`${w * 0.3},${h * 0.68} ${w * 0.2},${h - sw} ${w * 0.45},${h * 0.72}`); break
    case 'calloutCloud': body = path(`M ${w * 0.30},${h * 0.66} C ${w * 0.12},${h * 0.66} ${w * 0.10},${h * 0.42} ${w * 0.24},${h * 0.38} C ${w * 0.20},${h * 0.2} ${w * 0.45},${h * 0.12} ${w * 0.50},${h * 0.28} C ${w * 0.58},${h * 0.1} ${w * 0.85},${h * 0.16} ${w * 0.80},${h * 0.38} C ${w * 0.95},${h * 0.4} ${w * 0.93},${h * 0.64} ${w * 0.74},${h * 0.66} Z`) + circ(w * 0.3, h * 0.78, m * 0.05) + circ(w * 0.22, h * 0.9, m * 0.032); break
    case 'lineCallout': body = `<rect x="${w * 0.28}" y="${sw}" width="${w - sw - w * 0.28}" height="${h * 0.6}" ${A}/>` + sline(w * 0.28, h * 0.5, sw, h - sw, sw) + circ(sw + 2, h - sw - 2, 2.5); break
    case 'calloutLine2': body = `<rect x="${w * 0.35}" y="${sw}" width="${w - sw - w * 0.35}" height="${h * 0.55}" ${A}/>` + `<polyline points="${w * 0.35},${h * 0.4} ${w * 0.18},${h * 0.75} ${sw},${h - sw}" fill="none" stroke="${s}" stroke-width="${sw}"/>` + circ(sw + 2, h - sw - 2, 2.5); break
    case 'calloutLineAccent': body = `<rect x="${w * 0.35}" y="${sw}" width="${w - sw - w * 0.35}" height="${h * 0.55}" ${A}/>` + `<rect x="${w * 0.35}" y="${sw}" width="${m * 0.045}" height="${h * 0.55}" fill="${s}"/>` + sline(w * 0.35, h * 0.5, sw, h - sw, sw) + circ(sw + 2, h - sw - 2, 2.5); break
    // ── Traits (flèches sur connecteurs) ──────────────────────────────────────
    case 'elbowArrow': body = `${arrowHead}<polyline points="${sw},${h - sw} ${cx},${h - sw} ${cx},${sw} ${w - sw},${sw}" ${AL} marker-end="url(#ah)"/>`; break
    case 'elbowDoubleArrow': body = `${arrowHead}<polyline points="${sw},${h - sw} ${cx},${h - sw} ${cx},${sw} ${w - sw},${sw}" ${AL} marker-start="url(#ah0)" marker-end="url(#ah)"/>`; break
    case 'curveArrow': body = `${arrowHead}<path d="M ${sw},${h - sw} C ${w * 0.1},${h * 0.15} ${w * 0.9},${h * 0.85} ${w - sw},${sw}" ${AL} marker-end="url(#ah)"/>`; break
    case 'curveDoubleArrow': body = `${arrowHead}<path d="M ${sw},${h - sw} C ${w * 0.1},${h * 0.15} ${w * 0.9},${h * 0.85} ${w - sw},${sw}" ${AL} marker-start="url(#ah0)" marker-end="url(#ah)"/>`; break
    // ── Rectangles (variantes coins) ──────────────────────────────────────────
    case 'snip2SameRect': { const c = m * 0.22; body = poly(`${c},${sw} ${w - c},${sw} ${w - sw},${c} ${w - sw},${h - sw} ${sw},${h - sw} ${sw},${c}`); break }
    case 'snip2DiagRect': { const c = m * 0.22; body = poly(`${sw},${sw} ${w - c},${sw} ${w - sw},${c} ${w - sw},${h - sw} ${c},${h - sw} ${sw},${h - c}`); break }
    case 'snipRoundRect': { const c = m * 0.26; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - c},${sw} L ${w - sw},${c} L ${w - sw},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'round2SameRect': { const c = m * 0.26; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - c},${sw} A ${c} ${c} 0 0 1 ${w - sw},${c} L ${w - sw},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'round2DiagRect': { const c = m * 0.26; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - sw},${sw} L ${w - sw},${h - c} A ${c} ${c} 0 0 1 ${w - c},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'plaque': { const c = m * 0.2; body = path(`M ${c},${sw} L ${w - c},${sw} A ${c} ${c} 0 0 0 ${w - sw},${c} L ${w - sw},${h - c} A ${c} ${c} 0 0 0 ${w - c},${h - sw} L ${c},${h - sw} A ${c} ${c} 0 0 0 ${sw},${h - c} L ${sw},${c} A ${c} ${c} 0 0 0 ${c},${sw} Z`); break }
    // ── Formes de base (suite) ────────────────────────────────────────────────
    case 'decagon':   body = poly(polyPts(10, cx, cy, cx - sw, cy - sw, -90)); break
    case 'dodecagon': body = poly(polyPts(12, cx, cy, cx - sw, cy - sw, 0)); break
    case 'pie':   body = path(`M ${cx},${cy} L ${w - sw},${cy} A ${cx - sw} ${cy - sw} 0 1 1 ${cx},${sw} Z`); break
    case 'chord': body = path(`M ${(cx + (cx - sw) * Math.cos(-Math.PI / 4)).toFixed(1)},${(cy + (cy - sw) * Math.sin(-Math.PI / 4)).toFixed(1)} A ${cx - sw} ${cy - sw} 0 1 1 ${(cx + (cx - sw) * Math.cos(Math.PI * 0.75)).toFixed(1)},${(cy + (cy - sw) * Math.sin(Math.PI * 0.75)).toFixed(1)} Z`); break
    case 'teardrop': body = path(`M ${cx},${sw} C ${w * 0.86},${sw} ${w - sw},${h * 0.14} ${w - sw},${cy} A ${cx - sw} ${cy - sw} 0 1 1 ${cx},${sw} Z`); break
    case 'halfFrame': { const t = m * 0.18; body = poly(`${sw},${sw} ${w - sw},${sw} ${w - t},${t} ${t},${t} ${t},${h - t} ${sw},${h - sw}`); break }
    case 'corner': { const t = m * 0.4; body = poly(`${sw},${sw} ${t},${sw} ${t},${h - t} ${w - sw},${h - t} ${w - sw},${h - sw} ${sw},${h - sw}`); break }
    case 'diagStripe': body = poly(`${sw},${h - sw} ${sw},${h * 0.45} ${w * 0.55},${h - sw}`); break
    case 'bevel': { const t = m * 0.16; body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>` + `<rect x="${sw + t}" y="${sw + t}" width="${w - 2 * sw - 2 * t}" height="${h - 2 * sw - 2 * t}" fill="none" stroke="${s}" stroke-width="${sw}"/>` + sline(sw, sw, sw + t, sw + t) + sline(w - sw, sw, w - sw - t, sw + t) + sline(sw, h - sw, sw + t, h - sw - t) + sline(w - sw, h - sw, w - sw - t, h - sw - t); break }
    case 'blockArc': { const t = m * 0.24; body = path(`M ${sw},${cy} A ${cx - sw} ${cy - sw} 0 0 1 ${w - sw},${cy} L ${w - sw - t},${cy} A ${cx - sw - t} ${cy - sw - t} 0 0 0 ${sw + t},${cy} Z`); break }
    case 'foldedCorner': { const c = m * 0.22; body = path(`M ${sw},${sw} L ${w - sw},${sw} L ${w - sw},${h - c} L ${w - c},${h - sw} L ${sw},${h - sw} Z`) + poly(`${w - c},${h - sw} ${w - c},${h - c} ${w - sw},${h - c}`); break }
    case 'doubleBracket': { const c = m * 0.18; body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.32},${sw} L ${sw + c},${sw} Q ${sw},${sw} ${sw},${sw + c} L ${sw},${h - sw - c} Q ${sw},${h - sw} ${sw + c},${h - sw} L ${w * 0.32},${h - sw}"/>` + `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.68},${sw} L ${w - sw - c},${sw} Q ${w - sw},${sw} ${w - sw},${sw + c} L ${w - sw},${h - sw - c} Q ${w - sw},${h - sw} ${w - sw - c},${h - sw} L ${w * 0.68},${h - sw}"/>`; break }
    case 'doubleBrace': body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.34},${sw} Q ${w * 0.18},${sw} ${w * 0.18},${cy * 0.5} Q ${w * 0.18},${cy} ${w * 0.06},${cy} Q ${w * 0.18},${cy} ${w * 0.18},${cy * 1.5} Q ${w * 0.18},${h - sw} ${w * 0.34},${h - sw}"/>` + `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.66},${sw} Q ${w * 0.82},${sw} ${w * 0.82},${cy * 0.5} Q ${w * 0.82},${cy} ${w * 0.94},${cy} Q ${w * 0.82},${cy} ${w * 0.82},${cy * 1.5} Q ${w * 0.82},${h - sw} ${w * 0.66},${h - sw}"/>`; break
    // ── Flèches pleines (suite) ───────────────────────────────────────────────
    case 'leftRightUpArrow': { const sh = m * 0.12, hl = m * 0.24, hy = h * 0.32, hx = w * 0.26; body = poly(`${cx},${sw} ${cx + hl},${hy} ${cx + sh},${hy} ${cx + sh},${cy - sh} ${w - hx},${cy - sh} ${w - hx},${cy - hl} ${w - sw},${cy} ${w - hx},${cy + hl} ${w - hx},${cy + sh} ${cx + sh},${cy + sh} ${cx + sh},${h - sw} ${cx - sh},${h - sw} ${cx - sh},${cy + sh} ${hx},${cy + sh} ${hx},${cy + hl} ${sw},${cy} ${hx},${cy - hl} ${hx},${cy - sh} ${cx - sh},${cy - sh} ${cx - sh},${hy} ${cx - hl},${hy}`); break }
    case 'bentUpArrow': { const t = m * 0.2, xUp = w * 0.7, hh = h * 0.34, hw = t * 1.4; body = poly(`${sw},${h - sw - t} ${xUp - t / 2},${h - sw - t} ${xUp - t / 2},${hh} ${xUp - hw},${hh} ${xUp},${sw} ${xUp + hw},${hh} ${xUp + t / 2},${hh} ${xUp + t / 2},${h - sw} ${sw},${h - sw}`); break }
    case 'uTurnArrow': { const t = m * 0.18; body = path(`M ${sw},${h - sw} L ${sw},${h * 0.42} A ${w * 0.28} ${h * 0.3} 0 0 1 ${w * 0.62},${h * 0.42} L ${w * 0.62},${h * 0.55} L ${w * 0.8},${h * 0.55} L ${w * 0.56},${h - sw} L ${w * 0.32},${h * 0.55} L ${w * 0.5},${h * 0.55} L ${w * 0.5},${h * 0.42} A ${w * 0.16} ${h * 0.18} 0 0 0 ${sw + t},${h * 0.42} L ${sw + t},${h - sw} Z`); break }
    case 'curvedRightArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${sw},${h - sw} Q ${sw},${sw} ${w - sw},${sw}" marker-end="url(#aht)"/>`; break
    case 'curvedLeftArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${w - sw},${h - sw} Q ${w - sw},${sw} ${sw},${sw}" marker-end="url(#aht)"/>`; break
    case 'curvedUpArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${sw},${h - sw} Q ${w - sw},${h - sw} ${w - sw},${sw}" marker-end="url(#aht)"/>`; break
    case 'curvedDownArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${sw},${sw} Q ${sw},${h - sw} ${w - sw},${h - sw}" marker-end="url(#aht)"/>`; break
    case 'stripedRightArrow': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${w * 0.1},${cy - sh / 2} ${w - hw},${cy - sh / 2} ${w - hw},${sw} ${w - sw},${cy} ${w - hw},${h - sw} ${w - hw},${cy + sh / 2} ${w * 0.1},${cy + sh / 2}`) + sline(sw, cy - sh / 2, sw, cy + sh / 2, sw + 1) + sline(w * 0.05, cy - sh / 2, w * 0.05, cy + sh / 2, sw + 1); break }
    case 'notchedArrow': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${sw},${cy - sh / 2} ${w - hw},${cy - sh / 2} ${w - hw},${sw} ${w - sw},${cy} ${w - hw},${h - sw} ${w - hw},${cy + sh / 2} ${sw},${cy + sh / 2} ${w * 0.12},${cy}`); break }
    case 'circularArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.13)}" d="M ${(cx + oR * Math.cos(-0.4)).toFixed(1)},${(cy + oR * Math.sin(-0.4)).toFixed(1)} A ${oR} ${oR} 0 1 1 ${(cx + oR * Math.cos(-1.1)).toFixed(1)},${(cy + oR * Math.sin(-1.1)).toFixed(1)}" marker-end="url(#aht)"/>`; break
    case 'rightArrowCallout': { const sh = h * 0.16, hh = h * 0.3, bx = w * 0.52, hx = w * 0.78; body = poly(`${sw},${sw} ${bx},${sw} ${bx},${cy - sh} ${hx},${cy - sh} ${hx},${cy - hh} ${w - sw},${cy} ${hx},${cy + hh} ${hx},${cy + sh} ${bx},${cy + sh} ${bx},${h - sw} ${sw},${h - sw}`); break }
    case 'leftArrowCallout': { const sh = h * 0.16, hh = h * 0.3, bx = w * 0.48, hx = w * 0.22; body = poly(`${w - sw},${sw} ${bx},${sw} ${bx},${cy - sh} ${hx},${cy - sh} ${hx},${cy - hh} ${sw},${cy} ${hx},${cy + hh} ${hx},${cy + sh} ${bx},${cy + sh} ${bx},${h - sw} ${w - sw},${h - sw}`); break }
    case 'upArrowCallout': { const sv = w * 0.16, hh = w * 0.3, by = h * 0.52, hy = h * 0.22; body = poly(`${sw},${h - sw} ${sw},${by} ${cx - sv},${by} ${cx - sv},${hy} ${cx - hh},${hy} ${cx},${sw} ${cx + hh},${hy} ${cx + sv},${hy} ${cx + sv},${by} ${w - sw},${by} ${w - sw},${h - sw}`); break }
    case 'downArrowCallout': { const sv = w * 0.16, hh = w * 0.3, by = h * 0.48, hy = h * 0.78; body = poly(`${sw},${sw} ${sw},${by} ${cx - sv},${by} ${cx - sv},${hy} ${cx - hh},${hy} ${cx},${h - sw} ${cx + hh},${hy} ${cx + sv},${hy} ${cx + sv},${by} ${w - sw},${by} ${w - sw},${sw}`); break }
    // ── Organigrammes (suite) ─────────────────────────────────────────────────
    case 'flowPunchedTape': body = path(`M ${sw},${h * 0.18} Q ${w * 0.25},${sw} ${cx},${h * 0.18} Q ${w * 0.75},${h * 0.36} ${w - sw},${h * 0.18} L ${w - sw},${h * 0.82} Q ${w * 0.75},${h - sw} ${cx},${h * 0.82} Q ${w * 0.25},${h * 0.64} ${sw},${h * 0.82} Z`); break
    case 'flowSort': body = poly(`${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}`) + sline(sw, cy, w - sw, cy); break
    case 'flowSequential': { const r = m / 2 - sw; body = circ(cx, cy, r) + sline(cx, cy + r, cx + r, cy + r); break }
    case 'flowMagneticDisk': { const ry = Math.min(h * 0.14, cy - sw), topY = sw + ry, botY = h - sw - ry; body = path(`M ${sw},${topY} L ${sw},${botY} A ${cx - sw} ${ry} 0 0 0 ${w - sw},${botY} L ${w - sw},${topY}`) + `<ellipse cx="${cx}" cy="${topY}" rx="${cx - sw}" ry="${ry}" ${A}/>`; break }
    case 'flowDirectAccess': { const rx = Math.min(w * 0.16, cx - sw), leftX = sw + rx, rightX = w - sw - rx; body = path(`M ${leftX},${sw} L ${rightX},${sw} A ${rx} ${cy - sw} 0 0 1 ${rightX},${h - sw} L ${leftX},${h - sw} A ${rx} ${cy - sw} 0 0 1 ${leftX},${sw}`) + `<path fill="none" stroke="${s}" stroke-width="${sw}" d="M ${rightX},${sw} A ${rx} ${cy - sw} 0 0 0 ${rightX},${h - sw}"/>`; break }
    case 'flowDisplay': body = path(`M ${sw},${cy} L ${w * 0.18},${sw} L ${w * 0.82},${sw} A ${w * 0.18} ${cy - sw} 0 0 1 ${w * 0.82},${h - sw} L ${w * 0.18},${h - sw} Z`); break
    // ── Étoiles et bannières (suite) ──────────────────────────────────────────
    case 'star7':  body = poly(starPts(7, cx, cy, oR, oR * 0.55)); break
    case 'star10': body = poly(starPts(10, cx, cy, oR, oR * 0.7)); break
    case 'star12': body = poly(starPts(12, cx, cy, oR, oR * 0.72)); break
    case 'ribbonCurved': body = path(`M ${sw},${h * 0.3} Q ${cx},${h * 0.12} ${w - sw},${h * 0.3} L ${w - sw},${h * 0.7} Q ${cx},${h * 0.52} ${sw},${h * 0.7} Z`); break
    case 'scrollH': body = `<rect x="${m * 0.16}" y="${h * 0.24}" width="${w - 2 * m * 0.16}" height="${h * 0.52}" rx="${m * 0.05}" ${A}/>` + sline(m * 0.16, h * 0.32, m * 0.16, h * 0.68, sw) + sline(w - m * 0.16, h * 0.32, w - m * 0.16, h * 0.68, sw); break
    case 'scrollV': body = `<rect x="${w * 0.24}" y="${m * 0.16}" width="${w * 0.52}" height="${h - 2 * m * 0.16}" rx="${m * 0.05}" ${A}/>` + sline(w * 0.32, m * 0.16, w * 0.68, m * 0.16, sw) + sline(w * 0.32, h - m * 0.16, w * 0.68, h - m * 0.16, sw); break
    case 'doubleWave': { const a = h * 0.14; body = path(`M ${sw},${h * 0.3} Q ${w * 0.14},${h * 0.3 - a} ${w * 0.28},${h * 0.3} T ${w * 0.56},${h * 0.3} T ${w * 0.84},${h * 0.3} T ${w - sw},${h * 0.3} L ${w - sw},${h * 0.7} Q ${w * 0.86},${h * 0.7 + a} ${w * 0.72},${h * 0.7} T ${w * 0.44},${h * 0.7} T ${w * 0.16},${h * 0.7} T ${sw},${h * 0.7} Z`); break }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`
}

// Une forme de la galerie : soit une vraie forme SVG, soit la zone de texte
// (`textBox`, qui route vers l'insertion de zone de texte riche du canvas).
type GalleryKind = ShapeKind | 'textBox'

// Taille d'insertion par défaut selon la forme (lignes plates, flèches hautes, …).
function shapeDefaultSize(kind: ShapeKind): { w: number; h: number } {
  if (kind === 'line' || kind === 'lineArrow' || kind === 'lineDouble' || kind === 'elbowConnector' || kind === 'elbowArrow' || kind === 'elbowDoubleArrow' || kind === 'curveConnector' || kind === 'curveArrow' || kind === 'curveDoubleArrow' || kind === 'curve') return { w: 280, h: 90 }
  if (kind === 'arrowUp' || kind === 'arrowDown' || kind === 'arrowUpDown' || kind === 'upArrowCallout' || kind === 'downArrowCallout' || kind === 'bentUpArrow') return { w: 160, h: 240 }
  if (kind === 'leftBrace' || kind === 'rightBrace' || kind === 'leftBracket' || kind === 'rightBracket' || kind === 'doubleBrace' || kind === 'doubleBracket') return { w: 70, h: 220 }
  if (kind === 'scrollV') return { w: 170, h: 230 }
  if (kind.startsWith('star') || kind.startsWith('explosion') || kind === 'cross' || kind === 'noSymbol' || kind === 'sun' || kind === 'smiley' || kind === 'arrowQuad' || kind === 'leftRightUpArrow' || kind === 'circularArrow' || kind === 'flowConnector' || kind === 'donut' || kind === 'flowOr' || kind === 'flowSumming') return { w: 200, h: 200 }
  return { w: 240, h: 180 }
}

// Catalogue des formes par catégorie (façon ruban Word). Libellés FR par défaut.
interface ShapeDef { kind: GalleryKind; label: string }
interface ShapeCat { id: string; title: string; shapes: ShapeDef[] }
const SHAPE_CATALOG: ShapeCat[] = [
  { id: 'lines', title: 'Traits', shapes: [
    { kind: 'line', label: 'Trait' }, { kind: 'lineArrow', label: 'Flèche' }, { kind: 'lineDouble', label: 'Double flèche' },
    { kind: 'elbowConnector', label: 'Connecteur coudé' }, { kind: 'elbowArrow', label: 'Connecteur coudé fléché' }, { kind: 'elbowDoubleArrow', label: 'Connecteur coudé double flèche' },
    { kind: 'curveConnector', label: 'Connecteur courbe' }, { kind: 'curveArrow', label: 'Connecteur courbe fléché' }, { kind: 'curveDoubleArrow', label: 'Connecteur courbe double flèche' }, { kind: 'curve', label: 'Courbe' },
  ] },
  { id: 'rectangles', title: 'Rectangles', shapes: [
    { kind: 'rect', label: 'Rectangle' }, { kind: 'roundRect', label: 'Rectangle arrondi' }, { kind: 'snipRect', label: 'Coin coupé' },
    { kind: 'snip2SameRect', label: 'Deux coins coupés (même côté)' }, { kind: 'snip2DiagRect', label: 'Deux coins coupés (diagonale)' }, { kind: 'snipRoundRect', label: 'Coin coupé et arrondi' },
    { kind: 'roundRect1', label: 'Un coin arrondi' }, { kind: 'round2SameRect', label: 'Deux coins arrondis (même côté)' }, { kind: 'round2DiagRect', label: 'Deux coins arrondis (diagonale)' }, { kind: 'plaque', label: 'Plaque' }, { kind: 'frame', label: 'Cadre' },
  ] },
  { id: 'basic', title: 'Formes de base', shapes: [
    { kind: 'textBox', label: 'Zone de texte' },
    { kind: 'ellipse', label: 'Ellipse' }, { kind: 'triangle', label: 'Triangle isocèle' }, { kind: 'rtTriangle', label: 'Triangle rectangle' },
    { kind: 'parallelogram', label: 'Parallélogramme' }, { kind: 'trapezoid', label: 'Trapèze' }, { kind: 'diamond', label: 'Losange' },
    { kind: 'pentagon', label: 'Pentagone' }, { kind: 'hexagon', label: 'Hexagone' }, { kind: 'heptagon', label: 'Heptagone' }, { kind: 'octagon', label: 'Octogone' },
    { kind: 'decagon', label: 'Décagone' }, { kind: 'dodecagon', label: 'Dodécagone' }, { kind: 'pie', label: 'Camembert' }, { kind: 'chord', label: 'Corde' }, { kind: 'teardrop', label: 'Goutte' },
    { kind: 'halfFrame', label: 'Demi-cadre' }, { kind: 'corner', label: 'Coin' }, { kind: 'diagStripe', label: 'Bande diagonale' },
    { kind: 'cross', label: 'Croix' }, { kind: 'bevel', label: 'Biseau' }, { kind: 'cylinder', label: 'Cylindre' }, { kind: 'cube', label: 'Cube' }, { kind: 'blockArc', label: 'Arc plein' }, { kind: 'foldedCorner', label: 'Coin replié' },
    { kind: 'heart', label: 'Cœur' }, { kind: 'lightning', label: 'Éclair' }, { kind: 'sun', label: 'Soleil' }, { kind: 'moon', label: 'Lune' }, { kind: 'cloud', label: 'Nuage' },
    { kind: 'smiley', label: 'Visage souriant' }, { kind: 'arc', label: 'Arc' }, { kind: 'donut', label: 'Anneau' }, { kind: 'noSymbol', label: 'Symbole interdit' },
    { kind: 'leftBracket', label: 'Crochet gauche' }, { kind: 'rightBracket', label: 'Crochet droit' }, { kind: 'doubleBracket', label: 'Crochets' },
    { kind: 'leftBrace', label: 'Accolade gauche' }, { kind: 'rightBrace', label: 'Accolade droite' }, { kind: 'doubleBrace', label: 'Accolades' },
  ] },
  { id: 'arrows', title: 'Flèches pleines', shapes: [
    { kind: 'arrow', label: 'Flèche droite' }, { kind: 'arrowLeft', label: 'Flèche gauche' }, { kind: 'arrowUp', label: 'Flèche haut' }, { kind: 'arrowDown', label: 'Flèche bas' },
    { kind: 'arrowLeftRight', label: 'Flèche gauche-droite' }, { kind: 'arrowUpDown', label: 'Flèche haut-bas' }, { kind: 'arrowQuad', label: 'Flèche quadruple' }, { kind: 'leftRightUpArrow', label: 'Flèche gauche-droite-haut' },
    { kind: 'bentArrow', label: 'Flèche coudée' }, { kind: 'bentUpArrow', label: 'Flèche coudée vers le haut' }, { kind: 'uTurnArrow', label: 'Flèche demi-tour' },
    { kind: 'curvedRightArrow', label: 'Flèche courbe droite' }, { kind: 'curvedLeftArrow', label: 'Flèche courbe gauche' }, { kind: 'curvedUpArrow', label: 'Flèche courbe haut' }, { kind: 'curvedDownArrow', label: 'Flèche courbe bas' },
    { kind: 'stripedRightArrow', label: 'Flèche rayée' }, { kind: 'notchedArrow', label: 'Flèche en V' }, { kind: 'pentagonArrow', label: 'Flèche pentagone' }, { kind: 'chevron', label: 'Chevron' }, { kind: 'circularArrow', label: 'Flèche circulaire' },
    { kind: 'rightArrowCallout', label: 'Légende flèche droite' }, { kind: 'leftArrowCallout', label: 'Légende flèche gauche' }, { kind: 'upArrowCallout', label: 'Légende flèche haut' }, { kind: 'downArrowCallout', label: 'Légende flèche bas' },
  ] },
  { id: 'equation', title: "Formes d'équation", shapes: [
    { kind: 'mathPlus', label: 'Plus' }, { kind: 'mathMinus', label: 'Moins' }, { kind: 'mathMultiply', label: 'Multiplier' },
    { kind: 'mathDivide', label: 'Diviser' }, { kind: 'mathEqual', label: 'Égal' }, { kind: 'mathNotEqual', label: 'Différent' },
  ] },
  { id: 'flowchart', title: 'Organigrammes', shapes: [
    { kind: 'flowProcess', label: 'Processus' }, { kind: 'flowAltProcess', label: 'Autre processus' }, { kind: 'flowDecision', label: 'Décision' },
    { kind: 'flowData', label: 'Données' }, { kind: 'flowPredefined', label: 'Processus prédéfini' }, { kind: 'flowInternal', label: 'Stockage interne' },
    { kind: 'flowDocument', label: 'Document' }, { kind: 'flowMultidoc', label: 'Plusieurs documents' }, { kind: 'flowTerminator', label: 'Terminaison' },
    { kind: 'flowPreparation', label: 'Préparation' }, { kind: 'flowManualInput', label: 'Saisie manuelle' }, { kind: 'flowManualOp', label: 'Opération manuelle' },
    { kind: 'flowConnector', label: 'Connecteur' }, { kind: 'flowOffPage', label: 'Renvoi de page' }, { kind: 'flowCard', label: 'Carte' }, { kind: 'flowPunchedTape', label: 'Bande perforée' },
    { kind: 'flowOr', label: 'Ou' }, { kind: 'flowSumming', label: 'Jonction de sommation' }, { kind: 'flowCollate', label: 'Assemblage' }, { kind: 'flowSort', label: 'Tri' },
    { kind: 'flowExtract', label: 'Extraction' }, { kind: 'flowMerge', label: 'Fusion' }, { kind: 'flowStored', label: 'Données stockées' }, { kind: 'flowDelay', label: 'Délai' },
    { kind: 'flowSequential', label: 'Accès séquentiel' }, { kind: 'flowMagneticDisk', label: 'Disque magnétique' }, { kind: 'flowDirectAccess', label: 'Accès direct' }, { kind: 'flowDisplay', label: 'Affichage' },
  ] },
  { id: 'stars', title: 'Étoiles et bannières', shapes: [
    { kind: 'explosion1', label: 'Explosion 1' }, { kind: 'explosion2', label: 'Explosion 2' },
    { kind: 'star4', label: 'Étoile à 4 branches' }, { kind: 'star', label: 'Étoile à 5 branches' }, { kind: 'star6', label: 'Étoile à 6 branches' }, { kind: 'star7', label: 'Étoile à 7 branches' },
    { kind: 'star8', label: 'Étoile à 8 branches' }, { kind: 'star10', label: 'Étoile à 10 branches' }, { kind: 'star12', label: 'Étoile à 12 branches' }, { kind: 'star16', label: 'Étoile à 16 branches' }, { kind: 'star24', label: 'Étoile à 24 branches' }, { kind: 'star32', label: 'Étoile à 32 branches' },
    { kind: 'ribbon', label: 'Bannière' }, { kind: 'ribbonDown', label: 'Bannière vers le bas' }, { kind: 'ribbonCurved', label: 'Bannière courbe' },
    { kind: 'scrollH', label: 'Parchemin horizontal' }, { kind: 'scrollV', label: 'Parchemin vertical' }, { kind: 'wave', label: 'Vague' }, { kind: 'doubleWave', label: 'Double vague' },
  ] },
  { id: 'callouts', title: 'Bulles et légendes', shapes: [
    { kind: 'calloutRect', label: 'Bulle rectangulaire' }, { kind: 'calloutRoundRect', label: 'Bulle arrondie' }, { kind: 'calloutOval', label: 'Bulle ovale' }, { kind: 'calloutCloud', label: 'Bulle nuage' },
    { kind: 'lineCallout', label: 'Légende avec trait' }, { kind: 'calloutLine2', label: 'Légende trait à 2 segments' }, { kind: 'calloutLineAccent', label: 'Légende trait avec barre' },
  ] },
]
const SHAPE_LABEL_MAP: Record<string, string> = Object.fromEntries(SHAPE_CATALOG.flatMap(c => c.shapes.map(sp => [sp.kind, sp.label])))
function shapeLabel(k: GalleryKind, t: (key: string, o?: Record<string, unknown>) => string): string {
  return t(`doc_shape_${k}`, { defaultValue: SHAPE_LABEL_MAP[k] || k })
}
// Aperçu de vignette : forme SVG, ou cadre « A » pour la zone de texte.
function galleryThumbSvg(k: GalleryKind): string {
  if (k === 'textBox') {
    return '<svg xmlns="http://www.w3.org/2000/svg" width="26" height="22" viewBox="0 0 26 22"><rect x="1.2" y="1.2" width="23.6" height="19.6" fill="#ffffff" stroke="#5f6470" stroke-width="1.3"/><text x="13" y="16" font-size="13" text-anchor="middle" fill="#5f6470" font-family="Georgia, serif">A</text></svg>'
  }
  return shapeSvg(k, 26, 22, '#ffffff', '#5f6470')
}

const svgToDataUrl = (svg: string) => 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg)

// L'attribut alt transporte les paramètres de réédition.
const shapeAlt   = (p: ShapeParams) => `kbshape:${encodeURIComponent(JSON.stringify(p))}`
function parseTextBoxAlt(alt: string | null | undefined): string | null {
  if (!alt?.startsWith('kbtext:')) return null
  try { return decodeURIComponent(alt.slice(7)) } catch { return null }
}
// Zone de texte RICHE : l'alt porte un document ProseMirror complet (canvas).
const textBoxRichAlt = (doc: HFContent) => `kbtextrich:${encodeURIComponent(JSON.stringify(doc))}`
const parseTextBoxRichAlt = (alt: string | null | undefined): HFContent | null => parseRichTextBox(alt) as HFContent | null
// Texte (ancien `kbtext:` mono-chaîne) → document riche équivalent.
function textToHFDoc(text: string): HFContent {
  const lines = (text || '').split('\n')
  return { type: 'doc', content: lines.map(l => l ? { type: 'paragraph', content: [{ type: 'text', text: l }] } : { type: 'paragraph' }) }
}
// SVG « cadre » (fond blanc + bordure) servant de `src` de repli pour une zone de
// texte riche (le contenu réel est peint sur le canvas depuis l'alt). Le src ne
// sert qu'à garder le nœud image valide et donner une vignette hors-canvas.
function richTextBoxFrameSvg(w: number, h: number): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><rect x="0.75" y="0.75" width="${w - 1.5}" height="${h - 1.5}" fill="#ffffff" stroke="#9aa0a6" stroke-width="1.5"/></svg>`
}
function parseShapeAlt(alt: string | null | undefined): ShapeParams | null {
  if (!alt?.startsWith('kbshape:')) return null
  try { return JSON.parse(decodeURIComponent(alt.slice(8))) as ShapeParams } catch { return null }
}

// Permet au moteur canvas de régénérer le `src` SVG des formes importées (qui ne
// portent que l'alt `kbshape:…`, ex. depuis DOCX) sans dupliquer `shapeSvg` côté moteur.
setShapeSrcResolver((alt, w, h) => {
  const sp = parseShapeAlt(alt)
  if (!sp) return null
  return svgToDataUrl(shapeSvg(sp.kind, w || 240, h || 180, sp.fill, sp.stroke, sp.sw))
})

// Tableaux : structure table > tableRow > tableCell > block+. Rendu sur le canvas
// par le moteur (layoutTable). Édition de cellule native (le contenu est du PM).
const TableCellExt = TipTapNode.create({
  name: 'tableCell', content: 'block+', isolating: true,
  addAttributes() {
    return {
      // Fusion de cellules : portée horizontale/verticale (1 = pas de fusion).
      colspan: { default: 1, parseHTML: (el: HTMLElement) => Number(el.getAttribute('colspan')) || 1, renderHTML: (a: Record<string, unknown>) => (Number(a.colspan) > 1 ? { colspan: String(a.colspan) } : {}) },
      rowspan: { default: 1, parseHTML: (el: HTMLElement) => Number(el.getAttribute('rowspan')) || 1, renderHTML: (a: Record<string, unknown>) => (Number(a.rowspan) > 1 ? { rowspan: String(a.rowspan) } : {}) },
      // Cellule absorbée par une fusion voisine → masquée du rendu (gardée dans le PM).
      merged: { default: false, parseHTML: (el: HTMLElement) => el.dataset.merged === '1', renderHTML: (a: Record<string, unknown>) => (a.merged ? { 'data-merged': '1' } : {}) },
      cellBg: { default: null, parseHTML: (el: HTMLElement) => el.dataset.bg || null, renderHTML: (a: Record<string, unknown>) => (a.cellBg ? { 'data-bg': String(a.cellBg) } : {}) },
      // Alignement vertical du contenu (Haut/Centré/Bas) + orientation du texte
      // (0 = horizontal ; 90 = vertical haut→bas ; 270 = vertical bas→haut), façon Word.
      cellVAlign: { default: 'top', parseHTML: (el: HTMLElement) => el.dataset.valign || 'top', renderHTML: (a: Record<string, unknown>) => (a.cellVAlign && a.cellVAlign !== 'top' ? { 'data-valign': String(a.cellVAlign) } : {}) },
      cellDir:    { default: 0, parseHTML: (el: HTMLElement) => Number(el.dataset.dir) || 0, renderHTML: (a: Record<string, unknown>) => (a.cellDir ? { 'data-dir': String(a.cellDir) } : {}) },
    }
  },
  parseHTML() { return [{ tag: 'td' }] },
  renderHTML({ HTMLAttributes }) { return ['td', HTMLAttributes, 0] },
})
const TableRowExt = TipTapNode.create({
  name: 'tableRow', content: 'tableCell+',
  parseHTML() { return [{ tag: 'tr' }] },
  renderHTML() { return ['tr', 0] },
})
const TableExt = TipTapNode.create({
  name: 'table', group: 'block', content: 'tableRow+', isolating: true,
  addAttributes() {
    return {
      // Style de tableau : 'plain' | 'grid' | 'striped' | 'header'. Pilote le rendu
      // (bande d'en-tête colorée, lignes alternées, bordures) dans le moteur canvas.
      tableStyle: { default: 'grid', parseHTML: (el: HTMLElement) => el.dataset.tableStyle || 'grid', renderHTML: (a: Record<string, unknown>) => ({ 'data-table-style': String(a.tableStyle || 'grid') }) },
      // Couleur d'accent (en-tête + bandes) : dérivée de l'accent du module si absente.
      accent: { default: null, parseHTML: (el: HTMLElement) => el.dataset.accent || null, renderHTML: (a: Record<string, unknown>) => (a.accent ? { 'data-accent': String(a.accent) } : {}) },
      // Largeurs de colonne (px) / hauteurs MIN de ligne (px). null = automatique
      // (colonnes uniformes / lignes selon le contenu). Réglés par glisser des bordures.
      colWidths:  { default: null, parseHTML: (el: HTMLElement) => { try { return JSON.parse(el.dataset.colw || 'null') } catch { return null } }, renderHTML: (a: Record<string, unknown>) => (a.colWidths ? { 'data-colw': JSON.stringify(a.colWidths) } : {}) },
      rowHeights: { default: null, parseHTML: (el: HTMLElement) => { try { return JSON.parse(el.dataset.rowh || 'null') } catch { return null } }, renderHTML: (a: Record<string, unknown>) => (a.rowHeights ? { 'data-rowh': JSON.stringify(a.rowHeights) } : {}) },
      // Propriétés du tableau (Word) : alignement sur la page + retrait gauche (px) +
      // mode des hauteurs de ligne ('atleast'|'exactly' par ligne) + texte de remplacement.
      tableAlign:     { default: 'left', parseHTML: (el: HTMLElement) => el.dataset.talign || 'left', renderHTML: (a: Record<string, unknown>) => (a.tableAlign && a.tableAlign !== 'left' ? { 'data-talign': String(a.tableAlign) } : {}) },
      tableIndent:    { default: 0, parseHTML: (el: HTMLElement) => Number(el.dataset.tindent) || 0, renderHTML: (a: Record<string, unknown>) => (a.tableIndent ? { 'data-tindent': String(a.tableIndent) } : {}) },
      rowHeightModes: { default: null, parseHTML: (el: HTMLElement) => { try { return JSON.parse(el.dataset.rhm || 'null') } catch { return null } }, renderHTML: (a: Record<string, unknown>) => (a.rowHeightModes ? { 'data-rhm': JSON.stringify(a.rowHeightModes) } : {}) },
      altTitle:       { default: null, parseHTML: (el: HTMLElement) => el.dataset.altTitle || null, renderHTML: (a: Record<string, unknown>) => (a.altTitle ? { 'data-alt-title': String(a.altTitle) } : {}) },
      altDesc:        { default: null, parseHTML: (el: HTMLElement) => el.dataset.altDesc || null, renderHTML: (a: Record<string, unknown>) => (a.altDesc ? { 'data-alt-desc': String(a.altDesc) } : {}) },
    }
  },
  parseHTML() { return [{ tag: 'table' }] },
  renderHTML({ HTMLAttributes }) { return ['table', HTMLAttributes, ['tbody', 0]] },
})

function makeTableNode(rows: number, cols: number): JSONContent {
  const cell = (): JSONContent => ({ type: 'tableCell', content: [{ type: 'paragraph' }] })
  const row  = (): JSONContent => ({ type: 'tableRow', content: Array.from({ length: cols }, cell) })
  return { type: 'table', content: Array.from({ length: rows }, row) }
}

// ── Socle commun de mise en forme (le « cœur » riche partagé) ───────────────────
// Capacités identiques pour le corps de page, l'en-tête/pied et les zones de texte :
// interligne/indentation/espacement, héritage de marques sur lignes vides, exposant/
// indice, images, tableaux, souligné, liens, listes de tâches, alignement, surlignage,
// couleur/police/taille, comptage. Le corps de page SURCHARGE ce socle (sauts de
// section/page + Placeholder + Collaboration Yjs) ; la RichEditZone l'utilise tel quel
// avec son propre undo. Garder l'ORDRE (priorité/schéma ProseMirror) inchangé.
const BASE_DOC_EXTENSIONS = [
  LineHeightExt,
  IndentExt,
  ParagraphSpacingExt,
  ParagraphFormatExt,
  FontMarksExt,
  InheritFontExt,
  SuperscriptExt,
  SubscriptExt,
  ImageExt,
  InlineImageExt,
  TableExt,
  TableRowExt,
  TableCellExt,
  Underline,
  Link.configure({ openOnClick: false }),
  TaskList,
  TaskItem.configure({ nested: true }),
  TextAlign.configure({ types: ['heading', 'paragraph'] }),
  Highlight.configure({ multicolor: true }),
  TextStyle,
  Color,
  FontFamilyExt,
  FontSizeExt,
  CharacterCount,
]

// Corps de page = socle commun SURCHARGÉ : sauts de section/page (pagination) +
// Placeholder, StarterKit sans undo (la Collaboration Yjs fournit l'historique).
const PAGE_EXTENSIONS = [
  // link/underline disabled here: BASE_DOC_EXTENSIONS provides them with custom config
  // (Link openOnClick:false). StarterKit v3 bundles both → would duplicate otherwise.
  StarterKit.configure({ undoRedo: false, link: false, underline: false }),
  SectionBreakExt,
  PageBreakExt,
  CommentMark,
  BookmarkMark,
  HeadingCollapseExt,
  AutoCorrectExt,
  StyleNameExt,
  ...BASE_DOC_EXTENSIONS,
  Placeholder.configure({ placeholder: () => i18n.t('doc_placeholder', { ns: 'office' }) }),
]

// ── Zone d'édition riche RÉUTILISABLE ───────────────────────────────────────────
// Même socle de mise en forme que le corps (gras/italique/police/taille/couleur/
// alignement, images, tableaux, listes…) SANS les éléments propres à la page (sauts
// de section/page) ni la collaboration Yjs : StarterKit garde son undo/redo local.
// Sert à l'en-tête/pied de page et aux zones de texte.
const RICH_ZONE_EXTENSIONS = [
  // own undo/redo (no Yjs here); link/underline come from BASE_DOC_EXTENSIONS.
  StarterKit.configure({ link: false, underline: false }),
  AutoCorrectExt,
  ...BASE_DOC_EXTENSIONS,
]

// Items de menu contextuel d'une zone d'édition riche (en-tête/pied, zone de texte).
// Agnostique de l'éditeur : couper/copier/coller, lien, mise en forme, alignement,
// tout sélectionner — mêmes intitulés que le corps mais ciblant l'éditeur passé.
function buildZoneCtxItems(ed: Editor, t: ReturnType<typeof useTranslation>['t']): MenuItem[] {
  const exec = (cmd: string) => { ed.view.focus(); document.execCommand(cmd) }
  const has = ed.state.selection.from < ed.state.selection.to
  const onLink = ed.isActive('link')
  return [
    { type: 'action', label: t('common_cut'),  shortcut: `${MOD}X`, disabled: !has, onClick: () => exec('cut') },
    { type: 'action', label: t('common_copy'), shortcut: `${MOD}C`, disabled: !has, onClick: () => exec('copy') },
    { type: 'action', label: t('common_paste'), shortcut: `${MOD}V`, onClick: async () => {
        try { const txt = await navigator.clipboard.readText(); ed.chain().focus().insertContent(txt).run() } catch { exec('paste') } } },
    { type: 'action', label: t('doc_paste_without_formatting'), shortcut: `${MOD}${SHIFT}V`, onClick: async () => {
        try { const txt = await navigator.clipboard.readText(); ed.chain().focus().insertContent(txt).run() } catch { /* ignore */ } } },
    { type: 'separator' },
    { type: 'action', label: onLink ? t('doc_edit_link') : t('doc_insert_link_ellipsis'), shortcut: `${MOD}K`, onClick: async () => {
        const url = await prompt({ title: t('doc_insert_link'), placeholder: 'https://exemple.com', defaultValue: ed.getAttributes('link').href ?? '', allowEmpty: true, confirmLabel: t('doc_apply') })
        if (url === null) return
        if (url === '') ed.chain().focus().unsetLink().run()
        else ed.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
      } },
    { type: 'separator' },
    { type: 'submenu', label: t('doc_text'), items: [
      { type: 'action', label: t('doc_bold'),      shortcut: `${MOD}B`, checked: ed.isActive('bold'),      onClick: () => ed.chain().focus().toggleBold().run() },
      { type: 'action', label: t('doc_italic'),    shortcut: `${MOD}I`, checked: ed.isActive('italic'),    onClick: () => ed.chain().focus().toggleItalic().run() },
      { type: 'action', label: t('doc_underline'), shortcut: `${MOD}U`, checked: ed.isActive('underline'), onClick: () => ed.chain().focus().toggleUnderline().run() },
      { type: 'action', label: t('doc_strikethrough', { defaultValue: 'Barré' }), checked: ed.isActive('strike'), onClick: () => ed.chain().focus().toggleStrike().run() },
    ] },
    { type: 'submenu', label: t('doc_align'), items:
      ([['left', t('doc_align_left')], ['center', t('doc_align_center')], ['right', t('doc_align_right')], ['justify', t('doc_align_justify')]] as Array<[string, string]>)
        .map(([a, lbl]) => ({ type: 'action' as const, label: lbl, checked: ed.isActive({ textAlign: a }), onClick: () => ed.chain().focus().setTextAlign(a).run() })) },
    { type: 'separator' },
    { type: 'action', label: t('doc_select_all'), shortcut: `${MOD}A`, onClick: () => ed.chain().focus().selectAll().run() },
  ]
}

// Mini-barre de mise en forme flottante (façon Word) — RÉUTILISABLE par toute zone
// canvas (RichEditZone : en-tête/pied, zones de texte) ET par le corps de page.
// `left/top` = coordonnées ÉCRAN (viewport) du haut de la sélection ; la barre se
// place juste au-dessus. 2 lignes : police/taille/couleur, puis styles/align/listes.
function FormattingMiniBar({ editor, left, top }: { editor: Editor; left: number; top: number }) {
  const { t } = useTranslation('office')
  const availableFonts = useAvailableFonts()
  const [colorOpen, setColorOpen] = useState(false)
  const ts = editor.getAttributes('textStyle')
  const curFont  = (ts.fontFamily as string) || 'Arial'
  const curSizeN = ts.fontSize ? Math.round(parseFloat(String(ts.fontSize))) : 11
  const curColor = (ts.color as string) || '#202124'
  const bump = (d: number) => applyInlineFormat(editor, { fs: `${Math.max(6, Math.min(96, curSizeN + d))}pt` })
  const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72]
  const SWATCHES = ['#202124', '#d93025', '#1a73e8', '#1e8e3e', '#f9ab00', '#9334e6', '#5f6368', '#ffffff']
  const MiniBtn = ({ active, onDo, title, children }: { active?: boolean; onDo: () => void; title: string; children: React.ReactNode }) => (
    <button title={title} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={onDo}
      className={`flex items-center justify-center w-7 h-7 rounded ${active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}>{children}</button>
  )
  return createPortal(
    <div style={{ position: 'fixed', left, top: top - 86, transform: 'translateX(-50%)', zIndex: 60 }}
      onMouseDown={e => e.preventDefault()}
      className="flex flex-col gap-1 bg-white border border-border rounded-lg shadow-lg px-1.5 py-1.5">
      {/* Ligne 1 : police, taille, couleur du texte */}
      <div className="flex items-center gap-1">
        <FontPicker value={curFont} onChange={f => applyInlineFormat(editor, { ff: f })}
          fonts={availableFonts} width={132} height={28} />
        <Dropdown value={String(curSizeN)} onChange={v => applyInlineFormat(editor, { fs: `${v}pt` })}
          options={SIZES.map(s => ({ value: String(s), label: String(s) }))} width={62} />
        <span style={{ position: 'relative' }}>
          <MiniBtn title={t('doc_text_color', { defaultValue: 'Couleur du texte' })} onDo={() => setColorOpen(o => !o)}>
            <span className="flex flex-col items-center justify-center leading-none"><span style={{ fontSize: 13, lineHeight: 1 }}>A</span><span style={{ width: 15, height: 3, background: curColor, marginTop: 1 }} /></span>
          </MiniBtn>
          {colorOpen && (
            <div className="absolute bg-white border border-border rounded-lg shadow-lg p-1.5 grid grid-cols-4 gap-1" style={{ top: '110%', left: 0, zIndex: 61 }}>
              {SWATCHES.map(c => (
                <button key={c} title={c} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }}
                  onClick={() => { editor.chain().focus().setColor(c).run(); setColorOpen(false) }}
                  className="w-5 h-5 rounded border border-border" style={{ background: c }} />
              ))}
            </div>
          )}
        </span>
      </div>
      {/* Ligne 2 : taille +/-, styles, surlignage, alignement, listes */}
      <div className="flex items-center gap-0.5">
        <MiniBtn title={t('doc_increase_font', { defaultValue: 'Agrandir la police' })} onDo={() => bump(1)}><span style={{ fontSize: 16, lineHeight: 1 }}>A</span></MiniBtn>
        <MiniBtn title={t('doc_decrease_font', { defaultValue: 'Réduire la police' })} onDo={() => bump(-1)}><span style={{ fontSize: 11, lineHeight: 1 }}>A</span></MiniBtn>
        <div className="w-px h-5 bg-border mx-0.5" />
        <MiniBtn title={t('doc_bold')}      active={editor.isActive('bold')}      onDo={() => editor.chain().focus().toggleBold().run()}><Bold size={15} /></MiniBtn>
        <MiniBtn title={t('doc_italic')}    active={editor.isActive('italic')}    onDo={() => editor.chain().focus().toggleItalic().run()}><Italic size={15} /></MiniBtn>
        <MiniBtn title={t('doc_underline')} active={editor.isActive('underline')} onDo={() => editor.chain().focus().toggleUnderline().run()}><UnderlineIcon size={15} /></MiniBtn>
        <MiniBtn title={t('doc_strikethrough', { defaultValue: 'Barré' })} active={editor.isActive('strike')} onDo={() => editor.chain().focus().toggleStrike().run()}><Strikethrough size={15} /></MiniBtn>
        <div className="w-px h-5 bg-border mx-0.5" />
        <MiniBtn title={t('doc_highlight', { defaultValue: 'Surlignage' })} active={editor.isActive('highlight')} onDo={() => editor.chain().focus().toggleHighlight({ color: '#fff475' }).run()}><Highlighter size={15} /></MiniBtn>
        <div className="w-px h-5 bg-border mx-0.5" />
        <MiniBtn title={t('doc_align_left')}   active={editor.isActive({ textAlign: 'left' })}   onDo={() => editor.chain().focus().setTextAlign('left').run()}><AlignLeft size={15} /></MiniBtn>
        <MiniBtn title={t('doc_align_center')} active={editor.isActive({ textAlign: 'center' })} onDo={() => editor.chain().focus().setTextAlign('center').run()}><AlignCenter size={15} /></MiniBtn>
        <MiniBtn title={t('doc_align_right')}  active={editor.isActive({ textAlign: 'right' })}  onDo={() => editor.chain().focus().setTextAlign('right').run()}><AlignRight size={15} /></MiniBtn>
        <div className="w-px h-5 bg-border mx-0.5" />
        <MiniBtn title={t('doc_bullet_list', { defaultValue: 'Liste à puces' })}   active={editor.isActive('bulletList')}  onDo={() => editor.chain().focus().toggleBulletList().run()}><List size={15} /></MiniBtn>
        <MiniBtn title={t('doc_ordered_list', { defaultValue: 'Liste numérotée' })} active={editor.isActive('orderedList')} onDo={() => editor.chain().focus().toggleOrderedList().run()}><ListOrdered size={15} /></MiniBtn>
      </div>
    </div>, document.body)
}

// Zone d'édition riche réutilisable — RENDU CANVAS (comme le corps de la page).
// L'éditeur ProseMirror reste caché (1px hors-écran) et ne sert QU'à capter le
// clavier et porter l'état (source de vérité) ; tout l'affichage (texte, images,
// tableaux, mise en forme, sélection) est peint sur un <canvas> via le moteur de
// mise en page (`layoutDocument`/`renderDocument`), et le caret est projeté en
// overlay. La barre d'outils agit sur l'éditeur exposé par `onEditor`. Réutilisée
// pour l'en-tête/pied et, à terme, les zones de texte / le corps (par surcharge).
function RichEditZone({ doc, width, zoom = 1, minHeight, placeholder, autoFocus, onChange, onEditor, onHeight, className, style }: {
  doc: HFContent
  width: number                 // largeur LOGIQUE de la zone de contenu (px, hors zoom)
  zoom?: number                 // facteur d'échelle appliqué au rendu canvas (netteté)
  minHeight?: number            // hauteur logique minimale (zone cliquable même vide)
  placeholder?: string
  autoFocus?: boolean
  onChange?: (doc: HFContent) => void
  onEditor?: (ed: Editor | null) => void
  onHeight?: (contentHeight: number) => void   // hauteur LOGIQUE du contenu (px) → auto-grandir la boîte
  className?: string
  style?: React.CSSProperties
}) {
  const editor = useEditor({
    extensions: RICH_ZONE_EXTENSIONS,
    content: doc,
    autofocus: autoFocus ? 'end' : false,
    onUpdate: ({ editor: ed }) => onChange?.(ed.getJSON()),
  })
  const { t } = useTranslation('office')
  const wrapRef   = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const caretRef  = useRef<HTMLDivElement>(null)
  const layoutRef = useRef<DocumentLayout | null>(null)
  const dragRef   = useRef<{ anchor: number } | null>(null)
  const wRef = useRef(width); wRef.current = width
  const zRef = useRef(zoom);  zRef.current = zoom
  // Menu contextuel (clic droit) + mini-barre de mise en forme (sur sélection, façon Word).
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null)
  const [miniBar, setMiniBar] = useState<{ left: number; top: number } | null>(null)

  // Projette le caret (curseur clignotant) en overlay DOM sur le canvas.
  const placeCaret = useCallback(() => {
    const ed = editor, caret = caretRef.current, layout = layoutRef.current
    if (!ed || !caret || !layout) return
    const sel = ed.state.selection
    if (!ed.isFocused || sel.from !== sel.to) { caret.style.display = 'none'; return }
    const z = zRef.current
    const cm = posToCoords(layout, sel.head)
    let caretH = cm.height
    const sm = ed.state.storedMarks
    if (sm && sm.length) {
      const ts = sm.find(m => m.type.name === 'textStyle' && m.attrs.fontSize)
      if (ts) { const pt = parseFloat(String(ts.attrs.fontSize)); if (!isNaN(pt)) caretH = pt * (96 / 72) * 1.2 }
    }
    caret.style.display = 'block'
    caret.style.left    = `${cm.x * z}px`
    caret.style.top     = `${cm.y * z}px`
    caret.style.height  = `${caretH * z}px`
    let lean = cm.italicAngle || 0
    if (!lean) {
      const italicActive = (sm && sm.some(m => m.type.name === 'italic'))
        || !!(sel.$from.parent?.attrs?.fontMarks as { i?: boolean } | undefined)?.i
      if (italicActive) lean = 0.21
    }
    caret.style.transformOrigin = 'bottom'
    caret.style.transform = lean ? `skewX(${-(Math.atan(lean) * 180 / Math.PI)}deg)` : 'none'
    caret.style.animation = 'none'; void caret.offsetHeight; caret.style.animation = '_gdocs_blink 1s 0.5s infinite'
  }, [editor])

  // (Re)peint le canvas à partir du modèle ProseMirror courant.
  const paint = useCallback(() => {
    const ed = editor, canvas = canvasRef.current
    if (!ed || !canvas) return
    const w = wRef.current, z = zRef.current
    const json = ed.getJSON()
    const layout = layoutDocument(json, w)
    layoutRef.current = layout
    const dpr = Math.max(2, window.devicePixelRatio || 1) // supersampling (cf. renderAllPages)
    const h = Math.max(minHeight ?? 0, layout.totalHeight)
    canvas.style.width  = `${w * z}px`
    canvas.style.height = `${h * z}px`
    canvas.width  = Math.max(1, Math.round(w * z * dpr))
    canvas.height = Math.max(1, Math.round(h * z * dpr))
    const sel = ed.state.selection
    renderDocument(canvas, layout, 0, 0, dpr, z, sel.from !== sel.to ? { from: sel.from, to: sel.to } : undefined, ed.isFocused)
    // Placeholder (doc vide) — texte grisé peint sur le canvas.
    if (placeholder && isHFEmpty(json)) {
      const ctx = canvas.getContext('2d')!
      ctx.save(); ctx.scale(dpr * z, dpr * z)
      const cm = posToCoords(layout, 1)
      ctx.font = '13.3px Arial, sans-serif'; ctx.fillStyle = '#9aa0a6'; ctx.textBaseline = 'alphabetic'
      ctx.fillText(placeholder, cm.x, cm.y + cm.height * 0.78)
      ctx.restore()
    }
    placeCaret()
    onHeight?.(layout.totalHeight)
    // Mini-barre flottante (façon Word) : au-dessus de la sélection non vide.
    if (ed.isFocused && sel.from < sel.to) {
      const rects = selectionRects(layout, sel.from, sel.to)
      if (rects.length) {
        const top = rects.reduce((a, b) => (b.y < a.y ? b : a), rects[0])   // rect le plus HAUT
        const cr = canvas.getBoundingClientRect()
        setMiniBar({ left: cr.left + (top.x + top.w / 2) * z, top: cr.top + top.y * z })
      } else setMiniBar(null)
    } else setMiniBar(null)
  }, [editor, minHeight, placeholder, placeCaret, onHeight])

  // Repeindre sur changements de l'éditeur + ressources externes (images/polices).
  useEffect(() => {
    if (!editor) return
    paint()
    const onTx = () => paint()
    editor.on('transaction', onTx)
    editor.on('selectionUpdate', onTx)
    editor.on('focus', onTx)
    editor.on('blur', onTx)
    window.addEventListener('kubuno-image-loaded', onTx)
    window.addEventListener('kubuno-font-loaded', onTx)
    return () => {
      editor.off('transaction', onTx); editor.off('selectionUpdate', onTx)
      editor.off('focus', onTx); editor.off('blur', onTx)
      window.removeEventListener('kubuno-image-loaded', onTx)
      window.removeEventListener('kubuno-font-loaded', onTx)
    }
  }, [editor, paint])

  // Repeindre quand la largeur ou le zoom changent.
  useEffect(() => { paint() }, [width, zoom, paint])

  useEffect(() => { onEditor?.(editor as Editor | null); return () => onEditor?.(null) }, [editor]) // eslint-disable-line react-hooks/exhaustive-deps

  const posFromEvent = (e: React.PointerEvent): number => {
    const canvas = canvasRef.current, layout = layoutRef.current
    if (!canvas || !layout) return 1
    const r = canvas.getBoundingClientRect()
    const z = zRef.current
    return coordsToPos(layout, (e.clientX - r.left) / z, (e.clientY - r.top) / z)
  }
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return
    e.preventDefault()
    const ed = editor; if (!ed) return
    const pos = posFromEvent(e)
    if (e.detail >= 2 && layoutRef.current) {
      const wb = wordBoundariesAt(layoutRef.current, pos)
      ed.chain().focus().setTextSelection({ from: wb.from, to: wb.to }).run()
      return
    }
    dragRef.current = { anchor: pos }
    ed.chain().focus().setTextSelection(pos).run()
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
  }
  const onPointerMove = (e: React.PointerEvent) => {
    const d = dragRef.current, ed = editor; if (!d || !ed) return
    const pos = posFromEvent(e)
    ed.chain().setTextSelection({ from: Math.min(d.anchor, pos), to: Math.max(d.anchor, pos) }).run()
  }
  const onPointerUp = (e: React.PointerEvent) => {
    dragRef.current = null
    ;(e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId)
  }
  // Clic droit : positionner le curseur hors sélection (comme Word/Docs), ouvrir le menu.
  const onContextMenu = (e: React.MouseEvent) => {
    e.preventDefault(); e.stopPropagation()
    const ed = editor, layout = layoutRef.current, canvas = canvasRef.current
    if (!ed || !layout || !canvas) return
    const r = canvas.getBoundingClientRect(); const z = zRef.current
    const pos = coordsToPos(layout, (e.clientX - r.left) / z, (e.clientY - r.top) / z)
    const sel = ed.state.selection
    const insideSel = sel.from < sel.to && pos >= sel.from && pos <= sel.to
    if (!insideSel) ed.chain().focus().setTextSelection(pos).run()
    else ed.view.focus()
    setCtxMenu({ x: e.clientX, y: e.clientY })
  }

  return (
    <div ref={wrapRef} className={className} style={{ position: 'relative', ...style }}>
      <canvas ref={canvasRef} style={{ display: 'block', cursor: 'text' }}
        onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onContextMenu={onContextMenu} />
      <div ref={caretRef} style={{ position: 'absolute', width: 2, background: '#202124', display: 'none', pointerEvents: 'none', zIndex: 1 }} />
      {/* ProseMirror caché — capte le clavier, jamais affiché. */}
      <div style={{ position: 'absolute', width: 1, height: 1, overflow: 'hidden', opacity: 0, top: 0, left: 0, pointerEvents: 'none' }}>
        <EditorContent editor={editor} />
      </div>

      {/* Mini-barre de mise en forme (façon Word) — composant partagé. */}
      {miniBar && editor && <FormattingMiniBar editor={editor as Editor} left={miniBar.left} top={miniBar.top} />}

      {/* Menu contextuel (clic droit). */}
      {ctxMenu && editor && (
        <MenuDropdown
          items={buildZoneCtxItems(editor as Editor, t)}
          pos={{ top: ctxMenu.y, left: ctxMenu.x, minWidth: 220 }}
          onClose={() => setCtxMenu(null)}
        />
      )}
    </div>
  )
}

// ── Platform-aware shortcut modifier ──────────────────────────────────────────

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/.test(navigator.userAgent)
const MOD   = isMac ? '⌘' : 'Ctrl+'
const SHIFT = isMac ? '⇧' : 'Shift+'

// ── Menu bar ──────────────────────────────────────────────────────────────────


// ── Formatting toolbar ─────────────────────────────────────────────────────────

const BUILTIN_FONTS = ['Arial', 'Times New Roman', 'Courier New', 'Georgia', 'Verdana', 'Trebuchet MS']

const _loadedFonts = new Set<string>()
function loadCustomFont(importUrl: string, cssFamily: string, source?: string) {
  if (_loadedFonts.has(cssFamily)) return
  _loadedFonts.add(cssFamily)

  const injectLink = () => {
    const id = `kfont-${cssFamily.replace(/[^a-z0-9]/gi, '-')}`
    if (!document.getElementById(id)) {
      const link = document.createElement('link')
      link.id = id; link.rel = 'stylesheet'; link.href = importUrl
      document.head.appendChild(link)
    }
    // S'assurer que le canvas peut l'utiliser (sinon il ne charge qu'au rendu DOM).
    document.fonts?.load?.(`16px "${cssFamily}"`).catch(() => {})
  }

  // Google Fonts = feuille CSS → <link>.
  if (source === 'google') { injectLink(); return }
  if (typeof FontFace === 'undefined' || !document.fonts) { injectLink(); return }

  // Fichier de police uploadé (ex. Bookerly) servi par le module drive sous
  // authentification. DEUX pièges :
  //  1. l'`import_url` stocké peut pointer sur l'ANCIEN chemin `/api/v1/files/…`
  //     (mort depuis le renommage files→drive) → on le normalise vers `/drive/…`.
  //  2. une `FontFace url()` native est fetchée par le navigateur SANS l'en-tête
  //     Authorization (les interceptors axios ne s'y appliquent pas) → 401/HTML.
  //     On récupère donc les octets via axios (token injecté), puis on charge la
  //     police depuis une URL d'objet (Blob).
  const apiPath = importUrl.replace(/^\/api\/v1\/(files|drive)\//, '/drive/')
  const isInternal = apiPath.startsWith('/drive/')
  const load = async () => {
    let loaded: FontFace
    if (isInternal) {
      // Octets récupérés via axios (token injecté), passés à FontFace sous forme
      // BINAIRE (BufferSource) : aucun fetch d'URL → ni problème d'auth, ni
      // contrainte CSP `font-src` (un blob:/objectURL est refusé par la CSP).
      const resp = await api.get<ArrayBuffer>(apiPath, { responseType: 'arraybuffer' })
      loaded = await new FontFace(cssFamily, resp.data).load()
    } else {
      // URL externe directe (police hébergée publiquement, ex. CDN autorisé par la CSP).
      loaded = await new FontFace(cssFamily, `url("${importUrl}")`).load()
    }
    document.fonts.add(loaded)
    // Prévenir l'éditeur canvas qu'une police est prête → purge du cache de
    // largeurs (cf. canvas-engine) + re-rendu.
    window.dispatchEvent(new Event('kubuno-font-loaded'))
  }
  load().catch(() => { _loadedFonts.delete(cssFamily); injectLink() })
}

function useAvailableFonts(): string[] {
  const { data = [] } = useQuery({
    queryKey: ['office-fonts'],
    queryFn:  fontsApi.list,
    staleTime: 60_000,
  })
  // Polices PARTAGÉES déposées par un admin dans System/Fonts : enregistrées via
  // FontFace (dans loadSystemFonts) et proposées à TOUS les utilisateurs. Best-effort.
  const { data: systemFonts = [] } = useQuery({
    queryKey: ['system-fonts'],
    queryFn:  loadSystemFonts,
    staleTime: 60_000,
  })
  useEffect(() => { data.forEach(f => loadCustomFont(f.import_url, f.css_family, (f as { source?: string }).source)) }, [data])
  return useMemo(() => {
    const extra = [...data.map(f => f.css_family), ...systemFonts]
    const seen = new Set(BUILTIN_FONTS)
    return [...BUILTIN_FONTS, ...extra.filter(f => !seen.has(f) && (seen.add(f), true))]
  }, [data, systemFonts])
}

function ToolBtn({
  onClick, active, title, shortcut, children, disabled,
}: {
  onClick: () => void
  active?: boolean
  title?: string
  shortcut?: string
  children: React.ReactNode
  disabled?: boolean
}) {
  const [showTip, setShowTip] = useState(false)
  const tipTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  return (
    <div className="relative flex-shrink-0">
      <button
        onClick={onClick}
        disabled={disabled}
        onMouseDown={e => e.preventDefault()}
        onMouseEnter={() => { tipTimer.current = setTimeout(() => setShowTip(true), 500) }}
        onMouseLeave={() => { clearTimeout(tipTimer.current); setShowTip(false) }}
        className={`
          w-7 h-7 flex items-center justify-center rounded transition-colors
          ${active ? 'bg-[#e8f0fe] text-primary' : 'hover:bg-surface-2 text-text-secondary'}
          ${disabled ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'}
        `}
      >
        {children}
      </button>
      {showTip && title && (
        <div
          className="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 z-50
                     text-white whitespace-nowrap pointer-events-none select-none"
          style={{
            background: '#303134',
            borderRadius: 4,
            padding: '5px 8px',
            fontSize: 12,
            lineHeight: '16px',
            boxShadow: '0 2px 6px rgba(0,0,0,0.3)',
          }}
        >
          {title}
          {shortcut && (
            <span style={{ color: 'rgba(255,255,255,0.6)', marginLeft: 6 }}>({shortcut})</span>
          )}
        </div>
      )}
    </div>
  )
}

function Sep() {
  return <div className="w-px h-5 bg-border mx-1 self-center flex-shrink-0" />
}


const ZOOM_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2]


// ── Rulers (canvas-based, DPR-aware, Google Docs style) ───────────────────────
//
// Handles:  draggable triangular tab stops at each margin boundary
// Colors:   #4285f4 (Google blue), hit zone ±8 px, tooltip during drag
// Snap:     cursor switches to ew-resize / ns-resize near a handle

const CANVAS_PAD_Y   = 32
const PAGE_MARGIN_TOP = 5
const HANDLE_SZ       = 8   // triangle size in px
const RULER_SNAP      = 8   // drag activation radius in px

// ── Drag guide line (fixed overlay rendered while a ruler handle is dragged) ──

type DragGuide =
  | { type: 'vertical';   clientX: number }
  | { type: 'horizontal'; clientY: number }
  | null

function DragGuideLine({ guide }: { guide: DragGuide }) {
  if (!guide) return null
  return createPortal(
    <div
      style={{
        position: 'fixed',
        inset: 0,
        pointerEvents: 'none',
        zIndex: 9998,
      }}
    >
      {guide.type === 'vertical' && (
        <div style={{
          position: 'absolute',
          top: 0, bottom: 0,
          left: guide.clientX,
          width: 0,
          borderLeft: '1px dashed #4285f4',
        }} />
      )}
      {guide.type === 'horizontal' && (
        <div style={{
          position: 'absolute',
          left: 0, right: 0,
          top: guide.clientY,
          height: 0,
          borderTop: '1px dashed #4285f4',
        }} />
      )}
    </div>,
    document.body,
  )
}

// ── Horizontal ruler ──────────────────────────────────────────────────────────

interface HorizontalRulerProps {
  pageW:       number
  marginLeft:  number
  marginRight: number
  zoom:        number
  columns?:    number
  colGap?:     number
  // Retraits (px) du paragraphe du curseur (marqueurs façon Word).
  indentLeft?:      number
  indentFirstLine?: number
  indentRight?:     number
  // Taquets de tabulation du paragraphe du curseur + type courant (coin).
  tabStops?:        Array<{ pos: number; type: TabType }>
  tabType?:         TabType
  onMarginsChange?:   (left: number, right: number) => void
  onIndentsChange?:   (ind: { left: number; first: number; right: number }, commit: boolean) => void
  onTabStopsChange?:  (tabs: Array<{ pos: number; type: TabType }>) => void
  onDragGuideChange?: (guide: { clientX: number } | null) => void
}

type HRHit = 'left' | 'right' | 'i-first' | 'i-hang' | 'i-left' | 'i-right'

function HorizontalRuler({ pageW, marginLeft, marginRight, zoom, columns = 1, colGap = 0, indentLeft = 0, indentFirstLine = 0, indentRight = 0, tabStops = [], tabType = 'left', onMarginsChange, onIndentsChange, onTabStopsChange, onDragGuideChange }: HorizontalRulerProps) {
  const { t } = useTranslation('office')
  const canvasRef   = useRef<HTMLCanvasElement>(null)
  const [cursor, setCursor]   = useState('default')
  const [tooltip, setTooltip] = useState<{ x: number; label: string } | null>(null)
  const draggingRef  = useRef<HRHit | null>(null)
  const liveL        = useRef(marginLeft)
  const liveR        = useRef(marginRight)
  const liveIL       = useRef(indentLeft)
  const liveIF       = useRef(indentFirstLine)
  const liveIR       = useRef(indentRight)
  const liveTabs     = useRef(tabStops)
  const didDragRef   = useRef(false)

  const w = Math.round(pageW * zoom)
  const h = RULER_SZ

  const drawRuler = useCallback((ml: number, mr: number) => {
    const canvas = canvasRef.current
    if (!canvas) return
    const dpr = window.devicePixelRatio || 1
    const cw  = w * dpr, ch = h * dpr
    if (canvas.width !== cw) canvas.width = cw
    if (canvas.height !== ch) canvas.height = ch
    canvas.style.width  = `${w}px`
    canvas.style.height = `${h}px`

    const ctx  = canvas.getContext('2d')!
    const mlPx = ml * zoom
    const mrPx = mr * zoom
    const pxCm = PX_PER_CM * zoom

    ctx.clearRect(0, 0, cw, ch)
    ctx.save()
    ctx.scale(dpr, dpr)

    // Gray background (margin zones)
    ctx.fillStyle = '#f1f3f4'
    ctx.fillRect(0, 0, w, h)
    // White content zone
    ctx.fillStyle = '#fff'
    ctx.fillRect(mlPx, 0, w - mlPx - mrPx, h)
    // Gouttières entre colonnes (zones grises) + bornes
    if (columns > 1) {
      const contentWpx = w - mlPx - mrPx
      const gapPx = colGap * zoom
      const colWpx = (contentWpx - (columns - 1) * gapPx) / columns
      for (let i = 0; i < columns - 1; i++) {
        const gx = mlPx + (i + 1) * colWpx + i * gapPx
        ctx.fillStyle = '#f1f3f4'
        ctx.fillRect(gx, 0, gapPx, h)
        ctx.fillStyle = '#bdc1c6'
        ctx.fillRect(Math.round(gx) - 0.5, 0, 1, h)
        ctx.fillRect(Math.round(gx + gapPx) - 0.5, 0, 1, h)
      }
    }
    // Margin boundary lines
    ctx.fillStyle = '#bdc1c6'
    ctx.fillRect(Math.round(mlPx) - 0.5,       0, 1, h)
    ctx.fillRect(Math.round(w - mrPx) - 0.5,   0, 1, h)

    // Ticks and labels (origin = left margin)
    ctx.fillStyle = '#5f6368'
    ctx.font      = '9px Arial'
    ctx.textBaseline = 'top'
    const startMm = Math.floor(-mlPx / pxCm * 10) - 10
    const endMm   = Math.ceil((w - mlPx) / pxCm * 10) + 10
    for (let mm = startMm; mm <= endMm; mm++) {
      if (mm % 5 !== 0) continue
      const x = mlPx + (mm / 10) * pxCm
      if (x < -1 || x > w + 1) continue
      const isCm  = mm % 10 === 0
      ctx.fillRect(Math.round(x) - 0.5, h - (isCm ? 8 : 4), 1, isCm ? 8 : 4)
      // Labels en valeur absolue (marge gauche en positif comme Google Docs :
      // « 2 1 » puis « 1 2 3 … »), et pas de « 0 » à l'origine.
      if (isCm && mm !== 0) { ctx.textAlign = 'center'; ctx.fillText(String(Math.abs(mm / 10)), x, 1) }
    }

    // ── Marqueurs de retrait (façon Word) ────────────────────────────────────
    // 1ʳᵉ ligne ▽ (haut), retrait suspendu △ (bas-milieu), retrait gauche ▭ (tout en
    // bas, sous le △), retrait droit △ (bas, à droite). Positions = retraits du
    // paragraphe du curseur, depuis la marge gauche / vers la marge droite.
    const leftX  = mlPx + liveIL.current * zoom
    const firstX = mlPx + (liveIL.current + liveIF.current) * zoom
    const rightX = (w - mrPx) - liveIR.current * zoom
    ctx.fillStyle = '#4285f4'
    // 1ʳᵉ ligne : triangle pointe vers le BAS, en haut de la règle
    ctx.beginPath(); ctx.moveTo(firstX - 4, 1); ctx.lineTo(firstX + 4, 1); ctx.lineTo(firstX, 8); ctx.closePath(); ctx.fill()
    // Retrait suspendu : triangle pointe vers le HAUT, en bas-milieu
    ctx.beginPath(); ctx.moveTo(leftX - 4, 13); ctx.lineTo(leftX + 4, 13); ctx.lineTo(leftX, 7); ctx.closePath(); ctx.fill()
    // Retrait gauche : petit rectangle tout en bas (sous le suspendu)
    ctx.fillRect(leftX - 4, 13, 8, 6)
    // Retrait droit : triangle pointe vers le HAUT, en bas, côté droit
    ctx.beginPath(); ctx.moveTo(rightX - 4, 13); ctx.lineTo(rightX + 4, 13); ctx.lineTo(rightX, 7); ctx.closePath(); ctx.fill()

    // ── Taquets de tabulation (symboles façon Word, en bas de la règle) ────────
    ctx.fillStyle = '#3c4043'
    ctx.font = '11px Arial'; ctx.textBaseline = 'bottom'; ctx.textAlign = 'center'
    for (const tab of liveTabs.current) {
      const tx = mlPx + tab.pos * zoom
      if (tx < mlPx - 2 || tx > w - mrPx + 2) continue
      ctx.fillText(TAB_SYMBOL[tab.type] ?? '⌞', tx, h - 1)
    }

    ctx.restore()
  }, [w, h, zoom, columns, colGap])

  useLayoutEffect(() => {
    liveL.current = marginLeft; liveR.current = marginRight
    liveIL.current = indentLeft; liveIF.current = indentFirstLine; liveIR.current = indentRight
    liveTabs.current = tabStops
    drawRuler(marginLeft, marginRight)
  }, [drawRuler, marginLeft, marginRight, indentLeft, indentFirstLine, indentRight, tabStops])

  // Clic sur la règle : pose un taquet (type courant) dans la zone de contenu ; un clic sur
  // un taquet EXISTANT le retire. Ignoré juste après un glisser de marqueur.
  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (didDragRef.current) { didDragRef.current = false; return }
    const r = e.currentTarget.getBoundingClientRect()
    const mx = e.clientX - r.left
    const mlPx = liveL.current * zoom, mrPx = liveR.current * zoom
    const near = (a: number, b: number) => Math.abs(a - b) <= RULER_SNAP
    const existing = liveTabs.current.findIndex(tb => near(mlPx + tb.pos * zoom, mx))
    if (existing >= 0) { onTabStopsChange?.(liveTabs.current.filter((_, i) => i !== existing)); return }
    if (mx < mlPx + 2 || mx > w - mrPx - 2) return   // hors zone de contenu
    const pos = Math.round((mx - mlPx) / zoom)
    onTabStopsChange?.([...liveTabs.current, { pos, type: tabType }])
  }

  // Quel élément sous (mx,my) ? Marqueurs de retrait d'abord (par zone verticale), puis
  // bords de marge (saisis depuis le côté GRIS, pour ne pas entrer en conflit avec les
  // marqueurs qui occupent le côté contenu).
  const getHit = useCallback((mx: number, my: number): HRHit | null => {
    const mlPx = liveL.current * zoom, mrPx = liveR.current * zoom
    const leftX  = mlPx + liveIL.current * zoom
    const firstX = mlPx + (liveIL.current + liveIF.current) * zoom
    const rightX = (w - mrPx) - liveIR.current * zoom
    const near = (a: number, b: number) => Math.abs(a - b) <= RULER_SNAP
    if (my <= 9 && near(mx, firstX)) return 'i-first'
    if (my >= 13 && near(mx, leftX)) return 'i-left'
    if (my > 9  && near(mx, leftX))  return 'i-hang'
    if (my > 9  && near(mx, rightX)) return 'i-right'
    if (mx < mlPx - 1 && near(mx, mlPx)) return 'left'          // marge gauche (côté gris)
    if (mx > (w - mrPx) + 1 && near(mx, w - mrPx)) return 'right' // marge droite (côté gris)
    return null
  }, [zoom, w])

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (draggingRef.current) return
    const r = e.currentTarget.getBoundingClientRect()
    setCursor(getHit(e.clientX - r.left, e.clientY - r.top) ? 'ew-resize' : 'default')
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return
    const r0  = e.currentTarget.getBoundingClientRect()
    const hit = getHit(e.clientX - r0.left, e.clientY - r0.top)
    if (!hit) return
    e.preventDefault()
    draggingRef.current = hit
    didDragRef.current = true   // évite de poser un taquet au clic qui suit le drag
    setCursor('ew-resize')
    const MIN_CONTENT = 96, MINGAP = 16
    const isIndent = hit.startsWith('i-')

    const onMove = (me: MouseEvent) => {
      const canvas = canvasRef.current
      if (!canvas) return
      const rulerRect = canvas.getBoundingClientRect()
      const mx   = me.clientX - rulerRect.left
      const mlPx = liveL.current * zoom, mrPx = liveR.current * zoom
      if (!isIndent) {
        // ── Marges ────────────────────────────────────────────────────────────
        if (hit === 'left') {
          const newL = Math.max(0, Math.min(pageW - MIN_CONTENT - liveR.current, mx / zoom))
          liveL.current = newL
          setTooltip({ x: mx, label: t('doc_margin_left_cm', { value: (newL / PX_PER_CM).toFixed(2) }) })
          onDragGuideChange?.({ clientX: rulerRect.left + newL * zoom })
        } else {
          const newR = Math.max(0, Math.min(pageW - MIN_CONTENT - liveL.current, (w - mx) / zoom))
          liveR.current = newR
          setTooltip({ x: mx, label: t('doc_margin_right_cm', { value: (newR / PX_PER_CM).toFixed(2) }) })
          onDragGuideChange?.({ clientX: rulerRect.left + (w - newR * zoom) })
        }
        drawRuler(liveL.current, liveR.current)
        onMarginsChange?.(liveL.current, liveR.current)
        return
      }
      // ── Retraits ──────────────────────────────────────────────────────────────
      const rightXpx = (w - mrPx) - liveIR.current * zoom
      const oldIL = liveIL.current, oldIF = liveIF.current
      const clampX = (x: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, x))
      if (hit === 'i-first') {
        const fx = clampX(mx, mlPx, rightXpx - MINGAP)
        liveIF.current = (fx - mlPx) / zoom - liveIL.current
      } else if (hit === 'i-hang') {
        const lx = clampX(mx, mlPx, rightXpx - MINGAP)
        const newIL = (lx - mlPx) / zoom
        liveIL.current = newIL
        liveIF.current = (oldIL + oldIF) - newIL          // garde la 1ʳᵉ ligne fixe
      } else if (hit === 'i-left') {
        const lx = clampX(mx, mlPx, rightXpx - MINGAP)
        liveIL.current = (lx - mlPx) / zoom                 // déplace tout le bloc (offset conservé)
      } else { // i-right
        const minX = mlPx + (Math.max(liveIL.current, liveIL.current + liveIF.current)) * zoom + MINGAP
        const rx = clampX(mx, minX, w - mrPx)
        liveIR.current = ((w - mrPx) - rx) / zoom
      }
      drawRuler(liveL.current, liveR.current)
      onIndentsChange?.({ left: liveIL.current, first: liveIF.current, right: liveIR.current }, false)
    }

    const onUp = () => {
      draggingRef.current = null
      setTooltip(null)
      setCursor('default')
      onDragGuideChange?.(null)
      if (isIndent) onIndentsChange?.({ left: liveIL.current, first: liveIF.current, right: liveIR.current }, true)
      else onMarginsChange?.(liveL.current, liveR.current)
      document.removeEventListener('mousemove', onMove)
      document.removeEventListener('mouseup', onUp)
    }

    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
  }

  return (
    <div className="relative select-none" style={{ width: w, height: h, overflow: 'visible' }}>
      <canvas
        ref={canvasRef}
        onMouseMove={handleMouseMove}
        onMouseLeave={() => { if (!draggingRef.current) setCursor('default') }}
        onMouseDown={handleMouseDown}
        onClick={handleClick}
        style={{ display: 'block', cursor }}
      />
      {tooltip && (
        <div style={{
          position: 'absolute',
          top: h + 4,
          left: Math.min(Math.max(0, tooltip.x - 44), w - 110),
          background: '#303134', color: '#fff',
          fontSize: 11, padding: '3px 8px', borderRadius: 3,
          whiteSpace: 'nowrap', pointerEvents: 'none', zIndex: 100,
        }}>
          {tooltip.label}
        </div>
      )}
    </div>
  )
}

// ── Vertical ruler ────────────────────────────────────────────────────────────

interface VerticalRulerProps {
  scrollRef:    React.RefObject<HTMLDivElement | null>   // conteneur défilant (lecture DIRECTE de scrollTop)
  activePage:   number                                   // index (0-based) de la page du CURSEUR (= page courante)
  activePageTop?: number                                 // top RÉEL (px, repère contenu) de la page active — disposition grille
  zoom:         number
  marginTop:    number
  marginBottom: number
  pageH:        number
  pageGap:      number
  onMarginsChange?:   (top: number, bottom: number) => void
  onDragGuideChange?: (guide: { clientY: number } | null) => void
}

function VerticalRuler({ scrollRef, activePage, activePageTop, zoom, marginTop, marginBottom, pageH, pageGap, onMarginsChange, onDragGuideChange }: VerticalRulerProps) {
  const { t } = useTranslation('office')
  const canvasRef    = useRef<HTMLCanvasElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const [cursor, setCursor]   = useState('default')
  const [tooltip, setTooltip] = useState<{ y: number; label: string } | null>(null)
  const draggingRef   = useRef<'top' | 'bottom' | null>(null)
  const liveT         = useRef(marginTop)
  const liveB         = useRef(marginBottom)
  const scrollTopRef  = useRef(0)   // dernier scrollTop connu (mis à jour par l'écouteur de scroll direct)
  const activePageRef = useRef(activePage); activePageRef.current = Math.max(0, activePage)
  // Top RÉEL de la page active (px, repère contenu) ; en grille les pages ne sont pas
  // empilées → on ne peut pas le déduire de `activePage × hauteur_page`.
  const activeTopRef  = useRef<number | undefined>(activePageTop); activeTopRef.current = activePageTop

  const drawRuler = useCallback((mt: number, mb: number, st: number, canvasH: number) => {
    const canvas = canvasRef.current
    if (!canvas || canvasH <= 0) return
    const dpr = window.devicePixelRatio || 1
    const w = RULER_SZ, h = canvasH
    const cw = w * dpr, ch = h * dpr
    if (canvas.width !== cw) canvas.width = cw
    if (canvas.height !== ch) canvas.height = ch
    canvas.style.width  = `${w}px`
    canvas.style.height = `${h}px`

    const ctx   = canvas.getContext('2d')!
    const pxCm  = PX_PER_CM * zoom
    const cH    = (pageH - mt - mb) * zoom                           // content height in scroll-space
    const opsh  = pageH * zoom + pageGap * zoom                      // one-page scroll height

    ctx.clearRect(0, 0, cw, ch)
    ctx.save()
    ctx.scale(dpr, dpr)

    // Fond gris uni
    ctx.fillStyle = '#f1f3f4'
    ctx.fillRect(0, 0, w, h)

    // ── Page ACTIVE uniquement (façon Word) ───────────────────────────────────
    // La règle ne gradue QUE la page la plus en vue ; sa hauteur utile = celle de
    // cette page (marges comprises). 0 au bord HAUT du contenu ; |cm| vers le haut
    // (marge haute) et vers le bas (contenu + marge basse). Hors page : gris uni.
    const paperTop0   = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom        // haut du papier (page 0)
    const activeP     = activePageRef.current                       // page du CURSEUR (page courante)
    // En grille, on utilise le top RÉEL de la page active (mesuré sur le canvas) ;
    // sinon (colonne) on le déduit de l'index × hauteur de page.
    const paperTopC   = activeTopRef.current != null ? activeTopRef.current + PAGE_MARGIN_TOP * zoom
                                                     : paperTop0 + activeP * opsh
    const paperTopY   = paperTopC - st                              // écran
    const contentTopY = paperTopY + mt * zoom
    const contentBotY = contentTopY + cH
    const paperBotY   = paperTopY + pageH * zoom

    // Zone blanche = contenu de la page active
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, contentTopY, w, cH)

    // Filets de bordure de marge (haut/bas du contenu)
    ctx.fillStyle = '#bdc1c6'
    ctx.fillRect(0, Math.round(contentTopY), w, 1)
    ctx.fillRect(0, Math.round(contentBotY), w, 1)

    // Graduations de la page active
    ctx.fillStyle = '#5f6368'
    ctx.font = '9px Arial'; ctx.textAlign = 'right'; ctx.textBaseline = 'middle'
    const yTop = Math.max(paperTopY, -20), yBot = Math.min(paperBotY, h + 20)
    const startMm = Math.floor((yTop - contentTopY) / pxCm * 10)
    const endMm   = Math.ceil((yBot - contentTopY) / pxCm * 10)
    for (let mm = startMm; mm <= endMm; mm++) {
      if (mm % 5 !== 0) continue
      const yScr = contentTopY + (mm / 10) * pxCm
      if (yScr < paperTopY - 1 || yScr > paperBotY + 1 || yScr < -10 || yScr > h + 10) continue
      const isCm = mm % 10 === 0
      ctx.fillRect(w - (isCm ? 8 : 4), Math.round(yScr) - 0.5, isCm ? 8 : 4, 1)
      if (isCm && mm !== 0) ctx.fillText(String(Math.abs(mm / 10)), w - 10, yScr)
    }

    // Poignées de marge (page active uniquement)
    ctx.fillStyle = '#4285f4'
    for (const hy of [contentTopY, contentBotY]) {
      if (hy > -HANDLE_SZ && hy < h + HANDLE_SZ) {
        ctx.beginPath()
        ctx.moveTo(w, hy - HANDLE_SZ / 2)
        ctx.lineTo(w, hy + HANDLE_SZ / 2)
        ctx.lineTo(w - HANDLE_SZ, hy)
        ctx.closePath()
        ctx.fill()
      }
    }

    ctx.restore()
  }, [zoom, pageH, pageGap])

  // Redessine avec le dernier scrollTop connu (lu directement sur le conteneur).
  const redraw = useCallback(() => {
    drawRuler(liveT.current, liveB.current, scrollTopRef.current, containerRef.current?.clientHeight ?? 0)
  }, [drawRuler])

  // Redessin sur changement de marges / zoom / PAGE COURANTE (curseur).
  useLayoutEffect(() => {
    liveT.current = marginTop
    liveB.current = marginBottom
    redraw()
  }, [redraw, marginTop, marginBottom, activePage, activePageTop])

  // ── Suivi DIRECT du défilement (découplé de React) ──────────────────────────
  // La règle se redessine en `requestAnimationFrame` à CHAQUE scroll du conteneur, en lisant
  // `scrollTop` en direct — SANS passer par un setState parent. Avant, chaque scroll faisait
  // `setScrollTop` → re-rendu de TOUT l'éditeur → la règle (redessinée via la prop React)
  // traînait derrière le contenu (défilement natif lisse) de façon variable → « tremblement »,
  // d'autant plus marqué en profondeur (re-rendu plus lourd). En rAF, la règle est synchrone
  // au contenu, image par image.
  useEffect(() => {
    const sc = scrollRef.current
    if (!sc) return
    scrollTopRef.current = sc.scrollTop
    redraw()
    let raf = 0
    const onScroll = () => {
      if (raf) return
      raf = requestAnimationFrame(() => { raf = 0; scrollTopRef.current = sc.scrollTop; redraw() })
    }
    sc.addEventListener('scroll', onScroll, { passive: true })
    return () => { if (raf) cancelAnimationFrame(raf); sc.removeEventListener('scroll', onScroll) }
  }, [scrollRef, redraw])

  // Resize observer — re-draw when container height changes
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const obs = new ResizeObserver(() => redraw())
    obs.observe(el)
    return () => obs.disconnect()
  }, [redraw])

  // Hit detection (poignées de la PAGE ACTIVE uniquement, cohérent avec le tracé).
  const getHit = useCallback((mouseY: number): 'top' | 'bottom' | null => {
    const cH   = (pageH - liveT.current - liveB.current) * zoom
    const opsh = pageH * zoom + pageGap * zoom
    const st   = scrollTopRef.current
    const h    = containerRef.current?.clientHeight ?? 0
    const paperTop0   = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom
    const activeP     = activePageRef.current
    const contentTopY = paperTop0 + activeP * opsh - st + liveT.current * zoom
    const contentBotY = contentTopY + cH
    if (Math.abs(mouseY - contentTopY) <= RULER_SNAP) return 'top'
    if (Math.abs(mouseY - contentBotY) <= RULER_SNAP) return 'bottom'
    return null
  }, [zoom, pageH, pageGap])

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (draggingRef.current) return
    const y = e.clientY - e.currentTarget.getBoundingClientRect().top
    setCursor(getHit(y) ? 'ns-resize' : 'default')
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    const rect = e.currentTarget.getBoundingClientRect()
    const y    = e.clientY - rect.top
    const hit  = getHit(y)
    if (!hit) return
    e.preventDefault()
    draggingRef.current = hit
    setCursor('ns-resize')

    // Origine Y (scroll-space) du haut du papier de la PAGE ACTIVE (la seule graduée).
    const fcy0 = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom   // page-box top without margin
    const opsh = pageH * zoom + pageGap * zoom
    const activeP = activePageRef.current
    const pageOriginY = fcy0 + activeP * opsh

    const MIN_CONTENT_H = 96
    const container = containerRef.current

    const onMove = (me: MouseEvent) => {
      if (!container) return
      const r    = container.getBoundingClientRect()
      const my   = me.clientY - r.top
      const st_now = scrollTopRef.current
      const docY = my + st_now
      if (hit === 'top') {
        const newT = Math.max(0, Math.min(pageH - MIN_CONTENT_H - liveB.current, (docY - pageOriginY) / zoom))
        liveT.current = newT
        drawRuler(newT, liveB.current, st_now, container.clientHeight)
        setTooltip({ y: my, label: t('doc_margin_top_cm', { value: (newT / PX_PER_CM).toFixed(2) }) })
        // Guide Y = top content boundary on the dragged page, in viewport coords
        onDragGuideChange?.({ clientY: r.top + (pageOriginY + newT * zoom) - st_now })
      } else {
        const newB = Math.max(0, Math.min(pageH - MIN_CONTENT_H - liveT.current, pageH - (docY - pageOriginY) / zoom))
        liveB.current = newB
        drawRuler(liveT.current, newB, st_now, container.clientHeight)
        setTooltip({ y: my, label: t('doc_margin_bottom_cm', { value: (newB / PX_PER_CM).toFixed(2) }) })
        onDragGuideChange?.({ clientY: r.top + (pageOriginY + (pageH - newB) * zoom) - st_now })
      }
      onMarginsChange?.(liveT.current, liveB.current)
    }

    const onUp = () => {
      draggingRef.current = null
      setTooltip(null)
      setCursor('default')
      onDragGuideChange?.(null)
      onMarginsChange?.(liveT.current, liveB.current)
      document.removeEventListener('mousemove', onMove)
      document.removeEventListener('mouseup', onUp)
    }

    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
  }

  return (
    <div
      ref={containerRef}
      className="border-r border-[#dadce0] bg-[#f1f3f4] select-none"
      style={{ width: RULER_SZ, flex: 1, position: 'relative', overflow: 'visible', minHeight: 0, cursor }}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!draggingRef.current) setCursor('default') }}
      onMouseDown={handleMouseDown}
    >
      <canvas ref={canvasRef} style={{ position: 'absolute', top: 0, left: 0 }} />
      {tooltip && (
        <div style={{
          position: 'absolute',
          left: RULER_SZ + 4,
          top: Math.max(0, tooltip.y - 12),
          background: '#303134', color: '#fff',
          fontSize: 11, padding: '3px 8px', borderRadius: 3,
          whiteSpace: 'nowrap', pointerEvents: 'none', zIndex: 100,
        }}>
          {tooltip.label}
        </div>
      )}
    </div>
  )
}

// Types de taquets (façon Word) + symbole affiché dans le coin.
type TabType = 'left' | 'center' | 'right' | 'decimal' | 'bar'
const TAB_CYCLE: TabType[] = ['left', 'center', 'right', 'decimal', 'bar']
const TAB_SYMBOL: Record<TabType, string> = { left: '⌞', center: '⊥', right: '⌟', decimal: '⊿', bar: '│' }

// Coin (en haut à gauche) : sélecteur de TYPE de taquet — un clic fait défiler les types.
function CornerCell({ tabType, onCycle }: { tabType: TabType; onCycle: () => void }) {
  const { t } = useTranslation('office')
  const labels: Record<TabType, string> = {
    left: t('doc_tab_left', { defaultValue: 'Tabulation gauche' }),
    center: t('doc_tab_center', { defaultValue: 'Tabulation centrée' }),
    right: t('doc_tab_right', { defaultValue: 'Tabulation droite' }),
    decimal: t('doc_tab_decimal', { defaultValue: 'Tabulation décimale' }),
    bar: t('doc_tab_bar', { defaultValue: 'Barre' }),
  }
  return (
    <button type="button" onClick={onCycle} title={labels[tabType]}
      style={{ width: RULER_SZ, height: RULER_SZ, flexShrink: 0, lineHeight: `${RULER_SZ}px`, fontSize: 13 }}
      className="bg-[#f1f3f4] border-r border-b border-[#dadce0] text-[#3c4043] hover:bg-[#e8eaed] flex items-center justify-center select-none">
      {TAB_SYMBOL[tabType]}
    </button>
  )
}


// ── Volet de navigation (plan du document, comme Word) ────────────────────────

function NavPane({ editor, opsRef, onClose }: {
  editor: Editor | null
  opsRef: React.RefObject<PaginatedOps | null>
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const [items, setItems] = useState<Array<{ text: string; level: number; pos: number; page: number }>>([])

  useEffect(() => {
    const refresh = () => setItems(opsRef.current?.outline() ?? [])
    refresh()
    if (!editor) return
    editor.on('update', refresh)
    return () => { editor.off('update', refresh) }
  }, [editor, opsRef])

  return (
    <div className="w-60 flex-shrink-0 border-r border-[#dadce0] bg-white flex flex-col min-h-0">
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <span className="text-xs font-semibold text-text-primary">{t('doc_nav_pane', { defaultValue: 'Volet de navigation' })}</span>
        <button onClick={onClose} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><X size={13} /></button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {items.length === 0 && (
          <p className="px-3 py-2 text-xs text-text-tertiary">{t('doc_toc_empty', { defaultValue: 'Aucun titre dans le document.' })}</p>
        )}
        {items.map((it, i) => (
          <button key={i}
            onClick={() => opsRef.current?.scrollToPos(it.pos)}
            className="w-full text-left px-3 py-1.5 text-sm hover:bg-surface-1 flex items-center gap-2"
            style={{ paddingLeft: 12 + (it.level - 1) * 14 }}
            title={`${it.text} — p.${it.page}`}>
            <span className="truncate flex-1 text-text-primary" style={{ fontWeight: it.level === 1 ? 600 : 400, fontSize: it.level === 1 ? 13 : 12.5 }}>{it.text}</span>
            <span className="text-[10px] text-text-tertiary shrink-0">{it.page}</span>
          </button>
        ))}
      </div>
    </div>
  )
}

// ── Édition en-tête / pied de page (3 zones façon Word + champs dynamiques) ───

// Panneau « Options de disposition » d'un objet (image/forme) — habillage du
// texte façon Word. Petites vignettes SVG illustrant chaque mode.
const WRAP_THUMB: Record<string, React.ReactNode> = {
  inline:    <svg width="40" height="30" viewBox="0 0 40 30"><line x1="3" y1="8" x2="37" y2="8" stroke="#9aa0a6"/><rect x="3" y="13" width="10" height="9" fill="#1a73e8" opacity="0.5"/><line x1="15" y1="18" x2="37" y2="18" stroke="#9aa0a6"/><line x1="3" y1="26" x2="37" y2="26" stroke="#9aa0a6"/></svg>,
  square:    <svg width="40" height="30" viewBox="0 0 40 30"><rect x="3" y="6" width="13" height="18" fill="#1a73e8" opacity="0.5"/><line x1="18" y1="8" x2="37" y2="8" stroke="#9aa0a6"/><line x1="18" y1="13" x2="37" y2="13" stroke="#9aa0a6"/><line x1="18" y1="18" x2="37" y2="18" stroke="#9aa0a6"/><line x1="3" y1="27" x2="37" y2="27" stroke="#9aa0a6"/></svg>,
  topBottom: <svg width="40" height="30" viewBox="0 0 40 30"><line x1="3" y1="5" x2="37" y2="5" stroke="#9aa0a6"/><rect x="10" y="10" width="20" height="9" fill="#1a73e8" opacity="0.5"/><line x1="3" y1="25" x2="37" y2="25" stroke="#9aa0a6"/></svg>,
  behind:    <svg width="40" height="30" viewBox="0 0 40 30"><rect x="12" y="6" width="16" height="18" fill="#1a73e8" opacity="0.35"/><line x1="3" y1="9" x2="37" y2="9" stroke="#5f6368"/><line x1="3" y1="15" x2="37" y2="15" stroke="#5f6368"/><line x1="3" y1="21" x2="37" y2="21" stroke="#5f6368"/></svg>,
  front:     <svg width="40" height="30" viewBox="0 0 40 30"><line x1="3" y1="9" x2="37" y2="9" stroke="#9aa0a6"/><line x1="3" y1="15" x2="37" y2="15" stroke="#9aa0a6"/><line x1="3" y1="21" x2="37" y2="21" stroke="#9aa0a6"/><rect x="12" y="6" width="16" height="18" fill="#1a73e8" opacity="0.85"/></svg>,
}
function WrapOptionsPanel({ wrap, left, top, onChange, onClose }: {
  wrap: string; left: number; top: number
  onChange: (w: string) => void; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const Item = ({ mode, label }: { mode: string; label: string }) => (
    <button onMouseDown={e => { e.preventDefault(); onChange(mode) }}
      className={`flex flex-col items-center gap-1 p-1.5 rounded-lg border ${wrap === mode ? 'border-primary bg-primary-light/40' : 'border-transparent hover:bg-surface-2'}`}
      title={label}>
      <span className="border border-border rounded bg-white">{WRAP_THUMB[mode]}</span>
      <span className="text-[10px] text-text-secondary leading-tight text-center w-14">{label}</span>
    </button>
  )
  return (
    <div style={{ position: 'absolute', left, top, zIndex: 33, width: 230 }}
      className="bg-white border border-border rounded-xl shadow-xl p-3"
      onMouseDown={e => e.stopPropagation()}>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-semibold text-primary">{t('doc_layout_options', { defaultValue: 'Options de disposition' })}</span>
        <button onMouseDown={e => { e.preventDefault(); onClose() }} className="p-0.5 rounded hover:bg-surface-2 text-text-secondary"><X size={14} /></button>
      </div>
      <div className="text-[11px] font-medium text-text-tertiary mb-1">{t('doc_wrap_inline_group', { defaultValue: 'Aligné sur le texte' })}</div>
      <div className="grid grid-cols-3 gap-1 mb-2">
        <Item mode="inline" label={t('doc_wrap_inline', { defaultValue: 'Aligné sur le texte' })} />
      </div>
      <div className="text-[11px] font-medium text-text-tertiary mb-1">{t('doc_wrap_with_text', { defaultValue: 'Avec habillage du texte' })}</div>
      <div className="grid grid-cols-3 gap-1">
        <Item mode="square"    label={t('doc_wrap_square',    { defaultValue: 'Carré' })} />
        <Item mode="topBottom" label={t('doc_wrap_topbottom', { defaultValue: 'Haut et bas' })} />
        <Item mode="behind"    label={t('doc_wrap_behind',    { defaultValue: 'Derrière le texte' })} />
        <Item mode="front"     label={t('doc_wrap_front',     { defaultValue: 'Devant le texte' })} />
      </div>
    </div>
  )
}

// Barre contextuelle d'édition en-tête/pied (façon Word) : remplace la barre de
// mise en forme pendant le mode inline. Options (1ʳᵉ page diff., lier section),
// insertion de champs dynamiques au niveau de la zone focalisée, et Fermeture.

// ── Boîte de dialogue Zoom (façon Word) ────────────────────────────────────────
// Presets (200/150/100/75/50/25), ajustements à la fenêtre (largeur / une page /
// plusieurs pages) et pourcentage personnalisé.
function ZoomDialog({ zoom, onPick, onFit, onClose }: {
  zoom: number
  onPick: (z: number) => void
  onFit: (mode: 'width' | 'page' | 'multi') => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const [custom, setCustom] = useState(Math.round(zoom * 100))
  const presets = [200, 150, 100, 75, 50, 25]
  const apply = (z: number) => { onPick(Math.min(3, Math.max(0.25, z))); onClose() }
  const btn = 'rounded-md border border-border px-3 py-2 text-sm text-text-primary hover:border-primary hover:bg-primary/5 transition-colors'
  return (
    <FloatingWindow title={<span className="flex items-center gap-2"><ZoomIn size={16} className="text-primary" /> {t('doc_zoom', { defaultValue: 'Zoom' })}</span>} onClose={onClose} defaultWidth={360} backdrop>
      <div data-module="office">
        <div className="flex flex-col gap-3 p-4">
          <div className="grid grid-cols-3 gap-2">
            {presets.map(p => (
              <button key={p} className={`${btn} text-center ${Math.round(zoom * 100) === p ? 'border-primary bg-primary/5 text-primary' : ''}`} onClick={() => apply(p / 100)}>{p} %</button>
            ))}
          </div>
          <div className="flex flex-col gap-1.5">
            <button className={`${btn} text-left`} onClick={() => { onFit('width'); onClose() }}>{t('doc_zoom_page_width', { defaultValue: 'Largeur de la page' })}</button>
            <button className={`${btn} text-left`} onClick={() => { onFit('page'); onClose() }}>{t('doc_zoom_one_page', { defaultValue: 'Une page' })}</button>
            <button className={`${btn} text-left`} onClick={() => { onFit('multi'); onClose() }}>{t('doc_zoom_multi_page', { defaultValue: 'Plusieurs pages' })}</button>
          </div>
          <div className="flex items-center gap-2 pt-1">
            <label className="text-sm text-text-secondary">{t('doc_zoom_custom', { defaultValue: 'Pourcentage' })}</label>
            <input
              type="number" min={25} max={300} value={custom}
              onChange={e => setCustom(Number(e.target.value))}
              onKeyDown={e => { if (e.key === 'Enter') apply(custom / 100) }}
              className="w-20 rounded border border-border px-2 py-1 text-sm outline-none focus:border-primary"
            />
            <span className="text-sm text-text-secondary">%</span>
            <div className="flex-1" />
            <Button size="sm" onClick={() => apply(custom / 100)}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}

// ── Boîte de dialogue « Mise en page » d'un objet (façon Word) ──────────────────
// 3 onglets (Position / Habillage du texte / Taille) qui pilotent les attrs du nœud
// image sélectionné (largeur/hauteur/rotation/habillage/décalage). cm ⇄ px via PX_PER_CM.
interface LayoutInit {
  width: number; height: number; rotation: number; wrap: string; wrapX: number; wrapY: number
  wrapSide: string; wrapDistT: number; wrapDistB: number; wrapDistL: number; wrapDistR: number
  align: string; posHRel: string; posVRel: string
  moveWithText: boolean; allowOverlap: boolean; lockAnchor: boolean
  pageW: number; pageH: number; contentW: number; contentH: number
}
// Champs réutilisables, déclarés au NIVEAU MODULE (sinon remontés à chaque frappe →
// perte de focus). Tous bâtis sur les PRIMITIVES DU CORE (@ui) : NumberInput,
// Dropdown, Radio, Checkbox — au lieu d'éléments natifs maison.
function LDNum({ value, on, suffix, step = 1, disabled, width = 'w-24' }: { value: number; on: (n: number) => void; suffix?: string; step?: number; disabled?: boolean; width?: string }) {
  return <span className="flex items-center gap-1"><NumberInput value={value} onChange={on} step={step} disabled={disabled} className={`${width} h-8`} />{suffix && <span className="text-sm text-text-secondary">{suffix}</span>}</span>
}
function LDSel({ value, on, opts, disabled, width = 'w-32' }: { value: string; on: (v: string) => void; opts: Array<[string, string]>; disabled?: boolean; width?: string }) {
  return <div className={width}><Dropdown value={value} onChange={on} options={opts.map(([v, l]) => ({ value: v, label: l }))} disabled={disabled} width="100%" height={32} /></div>
}
function LDRadio({ checked, on, label, disabled }: { checked: boolean; on: () => void; label: string; disabled?: boolean }) {
  return <Radio checked={checked} onChange={() => on()} label={label} disabled={disabled} />
}
function LDCheck({ checked, on, label, disabled }: { checked: boolean; on: (b: boolean) => void; label: string; disabled?: boolean }) {
  return <Checkbox checked={checked} onChange={on} label={label} disabled={disabled} />
}

// ── Dialogue « Paragraphe… » (parité Word : Retrait et espacement + Enchaînements) ──
type ParaSpecial = 'none' | 'firstLine' | 'hanging'
type ParaLineMode = 'single' | '1.5' | 'double' | 'atLeast' | 'exactly' | 'multiple'
interface ParaDraft {
  align: 'left' | 'center' | 'right' | 'justify'
  outlineLevel: number       // 0 = Corps de texte ; 1..9
  indentLeftCm: number
  indentRightCm: number
  special: ParaSpecial
  specialByCm: number        // « De » du retrait spécial (1ʳᵉ ligne / suspendu)
  mirrorIndents: boolean
  spaceBeforePt: number
  spaceAfterPt: number
  lineMode: ParaLineMode
  lineValue: number          // « De » : multiplicateur (multiple) ou pt (au moins/exactement)
  contextualSpacing: boolean
  widowControl: boolean
  keepNext: boolean
  keepLines: boolean
  pageBreakBefore: boolean
  suppressLineNumbers: boolean
  dontHyphenate: boolean
}
const PT_TO_PX = 96 / 72, PX_TO_PT = 72 / 96
function paraDraftFromAttrs(a: Record<string, unknown>): ParaDraft {
  const num = (v: unknown) => (typeof v === 'number' ? v : 0)
  const fl = num(a.indentFirstLine)
  const lsMode = a.lineSpacingMode as string | undefined
  const lh = typeof a.lineHeight === 'number' ? a.lineHeight as number : null
  const lsPt = typeof a.lineSpacingPt === 'number' ? a.lineSpacingPt as number : null
  let lineMode: ParaLineMode = 'multiple', lineValue = lh ?? 1.15
  if (lsMode === 'atLeast') { lineMode = 'atLeast'; lineValue = Math.round((lsPt ?? 14 / PX_TO_PT) * PX_TO_PT) }
  else if (lsMode === 'exactly') { lineMode = 'exactly'; lineValue = Math.round((lsPt ?? 14 / PX_TO_PT) * PX_TO_PT) }
  else if (lh == null || Math.abs(lh - 1) < 0.02) { lineMode = 'single'; lineValue = 1 }
  else if (Math.abs(lh - 1.5) < 0.02) { lineMode = '1.5'; lineValue = 1.5 }
  else if (Math.abs(lh - 2) < 0.02) { lineMode = 'double'; lineValue = 2 }
  else { lineMode = 'multiple'; lineValue = lh }
  return {
    align: (a.textAlign as ParaDraft['align']) ?? 'left',
    outlineLevel: num(a.outlineLevel),
    indentLeftCm: +(num(a.indentLeft) / PX_PER_CM).toFixed(2),
    indentRightCm: +(num(a.indentRight) / PX_PER_CM).toFixed(2),
    special: fl > 0 ? 'firstLine' : fl < 0 ? 'hanging' : 'none',
    specialByCm: +(Math.abs(fl) / PX_PER_CM).toFixed(2),
    mirrorIndents: !!a.mirrorIndents,
    spaceBeforePt: Math.round(num(a.spaceBefore) * PX_TO_PT),
    spaceAfterPt: Math.round(num(a.spaceAfter) * PX_TO_PT),
    lineMode, lineValue: +lineValue.toFixed(2),
    contextualSpacing: !!a.contextualSpacing,
    widowControl: a.widowControl !== false,   // Word : activé par défaut
    keepNext: !!a.keepNext,
    keepLines: !!a.keepLines,
    pageBreakBefore: !!a.pageBreakBefore,
    suppressLineNumbers: !!a.suppressLineNumbers,
    dontHyphenate: !!a.dontHyphenate,
  }
}
function paraAttrsFromDraft(d: ParaDraft): Record<string, unknown> {
  const cm = (v: number) => (v ? Math.round(v * PX_PER_CM) : null)
  const fl = d.special === 'firstLine' ? Math.round(d.specialByCm * PX_PER_CM)
           : d.special === 'hanging' ? -Math.round(d.specialByCm * PX_PER_CM) : null
  // Interligne : multiple/simple/1.5/double → lineHeight ; au moins/exactement → px absolus.
  let lineHeight: number | null = null, lineSpacingMode: string | null = null, lineSpacingPt: number | null = null
  if (d.lineMode === 'single') lineHeight = 1
  else if (d.lineMode === '1.5') lineHeight = 1.5
  else if (d.lineMode === 'double') lineHeight = 2
  else if (d.lineMode === 'multiple') lineHeight = d.lineValue || 1.15
  else { lineSpacingMode = d.lineMode; lineSpacingPt = Math.round(d.lineValue * PT_TO_PX) }
  return {
    textAlign: d.align,
    outlineLevel: d.outlineLevel || null,
    indentLeft: cm(d.indentLeftCm),
    indentRight: cm(d.indentRightCm),
    indentFirstLine: fl,
    mirrorIndents: d.mirrorIndents,
    spaceBefore: Math.round(d.spaceBeforePt * PT_TO_PX),
    spaceAfter: Math.round(d.spaceAfterPt * PT_TO_PX),
    lineHeight, lineSpacingMode, lineSpacingPt,
    contextualSpacing: d.contextualSpacing,
    widowControl: d.widowControl,
    keepNext: d.keepNext,
    keepLines: d.keepLines,
    pageBreakBefore: d.pageBreakBefore,
    suppressLineNumbers: d.suppressLineNumbers,
    dontHyphenate: d.dontHyphenate,
  }
}
function ParagraphDialog({ init, onApply, onClose }: { init: ParaDraft; onApply: (d: ParaDraft) => void; onClose: () => void }) {
  const { t } = useTranslation('office')
  const [d, setD] = useState<ParaDraft>(init)
  const [tab, setTab] = useState<'indent' | 'flow'>('indent')
  const up = (p: Partial<ParaDraft>) => setD(s => ({ ...s, ...p }))
  const multipleVal = d.lineMode === 'multiple' || d.lineMode === 'atLeast' || d.lineMode === 'exactly'
  const Tab = ({ id, label }: { id: 'indent' | 'flow'; label: string }) => (
    <button onClick={() => setTab(id)} className={`px-3 py-1.5 text-sm border-b-2 ${tab === id ? 'border-primary text-primary font-medium' : 'border-transparent text-text-secondary'}`}>{label}</button>
  )
  // Aperçu : 3 lignes témoin reflétant alignement/retraits/espacement.
  const previewAlign = d.align === 'justify' ? 'justify' : d.align
  const sample = t('doc_para_sample', { defaultValue: 'Texte exemple' })
  const sampleLine = Array(14).fill(sample).join(' ')
  return (
    <FloatingWindow title={t('doc_paragraph_dialog', { defaultValue: 'Paragraphe' })} onClose={onClose} defaultWidth={640} backdrop className="max-h-[92vh]">
      <div className="p-5 overflow-auto" data-module="office">
        <div className="flex items-center gap-1 border-b border-border mb-4">
          <Tab id="indent" label={t('doc_para_tab_indent', { defaultValue: 'Retrait et espacement' })} />
          <Tab id="flow" label={t('doc_para_tab_flow', { defaultValue: 'Enchaînements' })} />
        </div>

        {tab === 'indent' && (
          <div className="space-y-4">
            <section>
              <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_para_general', { defaultValue: 'Général' })}</h3>
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary w-28">{t('doc_align', { defaultValue: 'Alignement' })}</span>
                  <LDSel value={d.align} on={v => up({ align: v as ParaDraft['align'] })} opts={[['left', t('doc_align_left', { defaultValue: 'À gauche' })], ['center', t('doc_align_center', { defaultValue: 'Centré' })], ['right', t('doc_align_right', { defaultValue: 'À droite' })], ['justify', t('doc_align_justify', { defaultValue: 'Justifié' })]]} /></label>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_para_outline', { defaultValue: 'Niveau hiérarchique' })}</span>
                  <LDSel value={String(d.outlineLevel)} on={v => up({ outlineLevel: Number(v) })} opts={[['0', t('doc_para_body', { defaultValue: 'Corps de texte' })], ...Array.from({ length: 9 }, (_, i) => [String(i + 1), t('doc_para_level', { defaultValue: `Niveau ${i + 1}`, n: i + 1 })] as [string, string])]} /></label>
              </div>
            </section>
            <section>
              <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_para_indent', { defaultValue: 'Retrait' })}</h3>
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary w-28">{t('doc_para_indent_left', { defaultValue: 'Gauche' })}</span><LDNum value={d.indentLeftCm} on={n => up({ indentLeftCm: n })} step={0.1} suffix="cm" width="w-20" /></label>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_para_special', { defaultValue: 'Spécial' })}</span>
                  <LDSel value={d.special} on={v => up({ special: v as ParaSpecial })} opts={[['none', t('doc_para_special_none', { defaultValue: '(aucun)' })], ['firstLine', t('doc_para_special_first', { defaultValue: 'Première ligne' })], ['hanging', t('doc_para_special_hang', { defaultValue: 'Suspendu' })]]} /></label>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_para_by', { defaultValue: 'De' })}</span><LDNum value={d.specialByCm} on={n => up({ specialByCm: n })} step={0.1} suffix="cm" width="w-20" disabled={d.special === 'none'} /></label>
              </div>
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2 mt-2">
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary w-28">{t('doc_para_indent_right', { defaultValue: 'Droite' })}</span><LDNum value={d.indentRightCm} on={n => up({ indentRightCm: n })} step={0.1} suffix="cm" width="w-20" /></label>
                <LDCheck checked={d.mirrorIndents} on={b => up({ mirrorIndents: b })} label={t('doc_para_mirror', { defaultValue: 'Retraits inversés' })} />
              </div>
            </section>
            <section>
              <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_para_spacing', { defaultValue: 'Espacement' })}</h3>
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary w-28">{t('doc_para_before', { defaultValue: 'Avant' })}</span><LDNum value={d.spaceBeforePt} on={n => up({ spaceBeforePt: n })} suffix="pt" width="w-20" /></label>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_line_spacing', { defaultValue: 'Interligne' })}</span>
                  <LDSel value={d.lineMode} on={v => up({ lineMode: v as ParaLineMode, lineValue: v === 'single' ? 1 : v === '1.5' ? 1.5 : v === 'double' ? 2 : v === 'multiple' ? 1.15 : 14 })} opts={[['single', t('doc_ls_single', { defaultValue: 'Simple' })], ['1.5', t('doc_ls_15', { defaultValue: '1,5 ligne' })], ['double', t('doc_ls_double', { defaultValue: 'Double' })], ['atLeast', t('doc_ls_atleast', { defaultValue: 'Au moins' })], ['exactly', t('doc_ls_exactly', { defaultValue: 'Exactement' })], ['multiple', t('doc_ls_multiple', { defaultValue: 'Multiple' })]]} /></label>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_para_by', { defaultValue: 'De' })}</span><LDNum value={d.lineValue} on={n => up({ lineValue: n })} step={d.lineMode === 'multiple' ? 0.01 : 1} suffix={d.lineMode === 'atLeast' || d.lineMode === 'exactly' ? 'pt' : ''} width="w-20" disabled={!multipleVal} /></label>
              </div>
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2 mt-2">
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary w-28">{t('doc_para_after', { defaultValue: 'Après' })}</span><LDNum value={d.spaceAfterPt} on={n => up({ spaceAfterPt: n })} suffix="pt" width="w-20" /></label>
                <LDCheck checked={d.contextualSpacing} on={b => up({ contextualSpacing: b })} label={t('doc_para_contextual', { defaultValue: 'Ne pas ajouter d’espace entre les paragraphes du même style' })} />
              </div>
            </section>
          </div>
        )}

        {tab === 'flow' && (
          <div className="space-y-4">
            <section>
              <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_para_pagination', { defaultValue: 'Pagination' })}</h3>
              <div className="space-y-1.5">
                <LDCheck checked={d.widowControl} on={b => up({ widowControl: b })} label={t('doc_para_widow', { defaultValue: 'Éviter veuves et orphelines' })} />
                <LDCheck checked={d.keepNext} on={b => up({ keepNext: b })} label={t('doc_para_keepnext', { defaultValue: 'Paragraphes solidaires' })} />
                <LDCheck checked={d.keepLines} on={b => up({ keepLines: b })} label={t('doc_para_keeplines', { defaultValue: 'Lignes solidaires' })} />
                <LDCheck checked={d.pageBreakBefore} on={b => up({ pageBreakBefore: b })} label={t('doc_para_breakbefore', { defaultValue: 'Saut de page avant' })} />
              </div>
            </section>
            <section>
              <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_para_exceptions', { defaultValue: 'Exceptions de mise en forme' })}</h3>
              <div className="space-y-1.5">
                <LDCheck checked={d.suppressLineNumbers} on={b => up({ suppressLineNumbers: b })} label={t('doc_para_suppress_ln', { defaultValue: 'Supprimer les numéros de ligne' })} />
                <LDCheck checked={d.dontHyphenate} on={b => up({ dontHyphenate: b })} label={t('doc_para_no_hyphen', { defaultValue: 'Ne pas couper les mots' })} />
              </div>
            </section>
          </div>
        )}

        {/* Aperçu */}
        <div className="mt-4 border border-border rounded-lg p-3 bg-surface-1">
          <div className="text-[11px] text-text-tertiary mb-1">{t('doc_para_preview', { defaultValue: 'Aperçu' })}</div>
          <div style={{ paddingLeft: d.indentLeftCm * 12, paddingRight: d.indentRightCm * 12 }}>
            <p style={{ textAlign: previewAlign, textIndent: d.special === 'firstLine' ? d.specialByCm * 12 : d.special === 'hanging' ? -d.specialByCm * 12 : 0, marginLeft: d.special === 'hanging' ? d.specialByCm * 12 : 0, marginTop: d.spaceBeforePt * 0.5, marginBottom: d.spaceAfterPt * 0.5, lineHeight: d.lineMode === 'single' ? 1 : d.lineMode === '1.5' ? 1.5 : d.lineMode === 'double' ? 2 : d.lineMode === 'multiple' ? d.lineValue : 1.3, fontSize: 8 }}
              className="text-text-primary">{sampleLine}</p>
          </div>
        </div>

        <div className="flex items-center justify-between gap-2 mt-4">
          <Button variant="secondary" size="sm" onClick={() => { onApply(d); onClose() }}>{t('doc_para_set_default', { defaultValue: 'Définir par défaut' })}</Button>
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
            <Button variant="primary" size="sm" onClick={() => { onApply(d); onClose() }}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}

// ── Dialogue « Mise en page » (parité Word : Marges / Papier / Mise en page) ──────
interface PageSetupInit {
  margins: { top: number; right: number; bottom: number; left: number } // px
  orientation: Orientation
  paper: PaperSize
  gutter: number          // px
  headerDist: number      // px
  footerDist: number      // px
  vAlign: 'top' | 'center' | 'bottom' | 'both'
  sectionStart: 'nextPage' | 'continuous' | 'evenPage' | 'oddPage'
  evenOdd: boolean
  firstPageDiff: boolean
}
// Dimensions de papier en cm (portrait : largeur × hauteur).
const PAPER_CM: Record<PaperSize, [number, number]> = {
  a4: [21, 29.7], a5: [14.8, 21], a3: [29.7, 42], letter: [21.59, 27.94], legal: [21.59, 35.56],
}
function PageSetupDialog({ init, onApply, onClose }: { init: PageSetupInit; onApply: (v: PageSetupInit) => void; onClose: () => void }) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<'margins' | 'paper' | 'layout'>('margins')
  const cm = (px: number) => +(px / PX_PER_CM).toFixed(2)
  const px = (c: number) => Math.round(c * PX_PER_CM)
  const [mTop, setMTop] = useState(cm(init.margins.top))
  const [mBottom, setMBottom] = useState(cm(init.margins.bottom))
  const [mLeft, setMLeft] = useState(cm(init.margins.left))
  const [mRight, setMRight] = useState(cm(init.margins.right))
  const [gut, setGut] = useState(cm(init.gutter))
  const [orient, setOrient] = useState<Orientation>(init.orientation)
  const [paper, setPaper] = useState<PaperSize>(init.paper)
  const [hDist, setHDist] = useState(cm(init.headerDist))
  const [fDist, setFDist] = useState(cm(init.footerDist))
  const [vAlign, setVAlign] = useState(init.vAlign)
  const [secStart, setSecStart] = useState(init.sectionStart)
  const [evenOdd, setEvenOdd] = useState(init.evenOdd)
  const [firstDiff, setFirstDiff] = useState(init.firstPageDiff)
  const [w0, h0] = PAPER_CM[paper] ?? PAPER_CM.a4
  const paperW = orient === 'landscape' ? h0 : w0
  const paperH = orient === 'landscape' ? w0 : h0
  const apply = () => {
    onApply({
      margins: { top: px(mTop), bottom: px(mBottom), left: px(mLeft), right: px(mRight) },
      orientation: orient, paper, gutter: px(gut),
      headerDist: px(hDist), footerDist: px(fDist),
      vAlign, sectionStart: secStart, evenOdd, firstPageDiff: firstDiff,
    })
    onClose()
  }
  const Tab = ({ id, label }: { id: 'margins' | 'paper' | 'layout'; label: string }) => (
    <button onClick={() => setTab(id)} className={`px-3 py-1.5 text-sm border-b-2 ${tab === id ? 'border-primary text-primary font-medium' : 'border-transparent text-text-secondary'}`}>{label}</button>
  )
  // Mini-aperçu de la page (orientation + marges + alignement vertical).
  const pvW = orient === 'landscape' ? 150 : 110, pvH = orient === 'landscape' ? 110 : 150
  return (
    <FloatingWindow title={t('doc_layout_dialog', { defaultValue: 'Mise en page' })} onClose={onClose} defaultWidth={680} backdrop className="max-h-[92vh]">
      <div className="p-5 overflow-auto" data-module="office">
        <div className="flex items-center gap-1 border-b border-border mb-4">
          <Tab id="margins" label={t('doc_ps_margins', { defaultValue: 'Marges' })} />
          <Tab id="paper" label={t('doc_ps_paper', { defaultValue: 'Papier' })} />
          <Tab id="layout" label={t('doc_ps_layout', { defaultValue: 'Mise en page' })} />
        </div>

        <div className="flex gap-6">
          <div className="flex-1 space-y-4">
            {tab === 'margins' && (<>
              <section>
                <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_ps_margins', { defaultValue: 'Marges' })}</h3>
                <div className="grid grid-cols-2 gap-x-6 gap-y-2">
                  <label className="flex items-center justify-between gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_top', { defaultValue: 'Haut' })}</span><LDNum value={mTop} on={setMTop} step={0.1} suffix="cm" width="w-20" /></label>
                  <label className="flex items-center justify-between gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_bottom', { defaultValue: 'Bas' })}</span><LDNum value={mBottom} on={setMBottom} step={0.1} suffix="cm" width="w-20" /></label>
                  <label className="flex items-center justify-between gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_left', { defaultValue: 'Gauche' })}</span><LDNum value={mLeft} on={setMLeft} step={0.1} suffix="cm" width="w-20" /></label>
                  <label className="flex items-center justify-between gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_right', { defaultValue: 'Droite' })}</span><LDNum value={mRight} on={setMRight} step={0.1} suffix="cm" width="w-20" /></label>
                  <label className="flex items-center justify-between gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_gutter', { defaultValue: 'Reliure' })}</span><LDNum value={gut} on={setGut} step={0.1} suffix="cm" width="w-20" /></label>
                </div>
              </section>
              <section>
                <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_orientation', { defaultValue: 'Orientation' })}</h3>
                <div className="flex items-center gap-3">
                  {(['portrait', 'landscape'] as Orientation[]).map(o => (
                    <button key={o} onClick={() => setOrient(o)} className={`flex flex-col items-center gap-1 px-4 py-2 rounded border ${orient === o ? 'border-primary bg-primary/10 text-primary' : 'border-border text-text-secondary'}`}>
                      <div className="border-2 border-current" style={{ width: o === 'landscape' ? 30 : 22, height: o === 'landscape' ? 22 : 30 }} />
                      <span className="text-xs">{o === 'portrait' ? t('doc_portrait', { defaultValue: 'Portrait' }) : t('doc_landscape', { defaultValue: 'Paysage' })}</span>
                    </button>
                  ))}
                </div>
              </section>
            </>)}

            {tab === 'paper' && (<>
              <section>
                <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_ps_paper_format', { defaultValue: 'Format du papier' })}</h3>
                <div className="space-y-2">
                  <LDSel value={paper} on={v => setPaper(v as PaperSize)} opts={[['a4', 'A4'], ['a5', 'A5'], ['a3', 'A3'], ['letter', 'Letter'], ['legal', 'Legal']]} width="w-48" />
                  <div className="flex items-center gap-6">
                    <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_width', { defaultValue: 'Largeur' })}</span><span className="text-text-primary">{paperW} cm</span></label>
                    <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_height', { defaultValue: 'Hauteur' })}</span><span className="text-text-primary">{paperH} cm</span></label>
                  </div>
                </div>
              </section>
            </>)}

            {tab === 'layout' && (<>
              <section>
                <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_ps_section', { defaultValue: 'Section' })}</h3>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_sec_start', { defaultValue: 'Début de la section' })}</span>
                  <LDSel value={secStart} on={v => setSecStart(v as PageSetupInit['sectionStart'])} opts={[['nextPage', t('doc_ps_next_page', { defaultValue: 'Nouvelle page' })], ['continuous', t('doc_ps_continuous', { defaultValue: 'Continu' })], ['evenPage', t('doc_ps_even', { defaultValue: 'Page paire' })], ['oddPage', t('doc_ps_odd', { defaultValue: 'Page impaire' })]]} width="w-44" /></label>
              </section>
              <section>
                <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_ps_hf', { defaultValue: 'En-têtes et pieds de page' })}</h3>
                <div className="space-y-1.5">
                  <LDCheck checked={evenOdd} on={setEvenOdd} label={t('doc_ps_even_odd', { defaultValue: 'Paires et impaires différentes' })} />
                  <LDCheck checked={firstDiff} on={setFirstDiff} label={t('doc_ps_first_diff', { defaultValue: 'Première page différente' })} />
                  <div className="flex items-center gap-6 pt-1">
                    <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_header', { defaultValue: 'En-tête' })}</span><LDNum value={hDist} on={setHDist} step={0.1} suffix="cm" width="w-20" /></label>
                    <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_footer', { defaultValue: 'Pied de page' })}</span><LDNum value={fDist} on={setFDist} step={0.1} suffix="cm" width="w-20" /></label>
                  </div>
                </div>
              </section>
              <section>
                <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_ps_page', { defaultValue: 'Page' })}</h3>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_ps_valign', { defaultValue: 'Alignement vertical' })}</span>
                  <LDSel value={vAlign} on={v => setVAlign(v as PageSetupInit['vAlign'])} opts={[['top', t('doc_ps_valign_top', { defaultValue: 'Haut' })], ['center', t('doc_ps_valign_center', { defaultValue: 'Centré' })], ['bottom', t('doc_ps_valign_bottom', { defaultValue: 'Bas' })], ['both', t('doc_ps_valign_justify', { defaultValue: 'Justifié' })]]} width="w-40" /></label>
              </section>
            </>)}
          </div>

          {/* Aperçu */}
          <div className="w-44 flex-shrink-0">
            <div className="text-[11px] text-text-tertiary mb-1">{t('doc_para_preview', { defaultValue: 'Aperçu' })}</div>
            <div className="flex justify-center bg-surface-1 border border-border rounded-lg p-3">
              <div className="bg-white border border-border-strong shadow-sm relative" style={{ width: pvW, height: pvH }}>
                <div className="absolute bg-text-tertiary/15" style={{ left: pvW * (mLeft + gut) / 21, right: pvW * mRight / 21, top: pvH * mTop / 29.7, bottom: pvH * mBottom / 29.7,
                  display: 'flex', flexDirection: 'column', justifyContent: vAlign === 'center' ? 'center' : vAlign === 'bottom' ? 'flex-end' : 'flex-start', gap: 2, padding: 2 }}>
                  {Array.from({ length: 6 }).map((_, i) => <div key={i} className="h-[2px] bg-text-tertiary/50" />)}
                </div>
              </div>
            </div>
          </div>
        </div>

        <div className="flex items-center justify-between gap-2 mt-5 pt-3 border-t border-border">
          <Button variant="secondary" size="sm" onClick={apply}>{t('doc_para_set_default', { defaultValue: 'Définir par défaut' })}</Button>
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
            <Button variant="primary" size="sm" onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}

function LayoutDialog({ init, onApply, onClose }: {
  init: LayoutInit
  onApply: (a: Record<string, unknown>) => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<'position' | 'wrap' | 'size'>('position')
  const baseW = Math.max(8, init.width || 240), baseH = Math.max(8, init.height || 180)
  const ratio = baseW / baseH
  const toCm = (v: number) => Number((v / PX_PER_CM).toFixed(2))
  const cmToPx = (cm: number) => Math.round(cm * PX_PER_CM)

  // ── Taille ──────────────────────────────────────────────────────────────
  const [w, setW] = useState(baseW)
  const [h, setH] = useState(baseH)
  const [hMode, setHMode] = useState<'abs' | 'rel'>('abs')
  const [wMode, setWMode] = useState<'abs' | 'rel'>('abs')
  const [hRelPct, setHRelPct] = useState(100), [wRelPct, setWRelPct] = useState(100)
  const [hRelRef, setHRelRef] = useState('margin'), [wRelRef, setWRelRef] = useState('margin')
  const [rot, setRot] = useState(init.rotation || 0)
  const [lock, setLock] = useState(false)
  const setWAbs = (cm: number) => { const nw = Math.max(8, cmToPx(cm)); setW(nw); if (lock) setH(Math.round(nw / ratio)) }
  const setHAbs = (cm: number) => { const nh = Math.max(8, cmToPx(cm)); setH(nh); if (lock) setW(Math.round(nh * ratio)) }
  const scaleW = Math.round((w / baseW) * 100), scaleH = Math.round((h / baseH) * 100)
  const setScaleW = (p: number) => { const nw = Math.max(8, Math.round(baseW * p / 100)); setW(nw); if (lock) setH(Math.round(nw / ratio)) }
  const setScaleH = (p: number) => { const nh = Math.max(8, Math.round(baseH * p / 100)); setH(nh); if (lock) setW(Math.round(nh * ratio)) }

  // ── Habillage ───────────────────────────────────────────────────────────
  const WRAP_STYLES: Array<{ key: string; mode: string; label: string }> = [
    { key: 'inline',    mode: 'inline',    label: t('doc_wrap_inline',    { defaultValue: 'Aligné sur le texte' }) },
    { key: 'square',    mode: 'square',    label: t('doc_wrap_square',    { defaultValue: 'Encadré' }) },
    { key: 'tight',     mode: 'square',    label: t('doc_wrap_tight',     { defaultValue: 'Adapté' }) },
    { key: 'through',   mode: 'square',    label: t('doc_wrap_through',   { defaultValue: 'Au travers' }) },
    { key: 'topBottom', mode: 'topBottom', label: t('doc_wrap_topbottom', { defaultValue: 'Haut et bas' }) },
    { key: 'behind',    mode: 'behind',    label: t('doc_wrap_behind',    { defaultValue: 'Derrière le texte' }) },
    { key: 'front',     mode: 'front',     label: t('doc_wrap_front',     { defaultValue: 'Devant le texte' }) },
  ]
  const [wrap, setWrap] = useState(init.wrap || 'inline')
  const [wrapKey, setWrapKey] = useState(() => WRAP_STYLES.find(s => s.mode === (init.wrap || 'inline'))?.key || 'inline')
  const [wrapSide, setWrapSide] = useState(init.wrapSide || 'both')
  const [dT, setDT] = useState(init.wrapDistT || 0), [dB, setDB] = useState(init.wrapDistB || 0)
  const [dL, setDL] = useState(init.wrapDistL ?? 10), [dR, setDR] = useState(init.wrapDistR ?? 10)
  const sideEnabled = wrap !== 'inline'

  // ── Position ────────────────────────────────────────────────────────────
  const [hPos, setHPos] = useState<'align' | 'abs' | 'rel'>(init.wrap === 'inline' ? 'align' : 'abs')
  const [vPos, setVPos] = useState<'align' | 'abs' | 'rel'>(init.wrap === 'inline' ? 'align' : 'abs')
  const [hAlign, setHAlign] = useState(init.align === 'center' ? 'center' : init.align === 'right' ? 'right' : 'left')
  const [vAlign, setVAlign] = useState('top')
  const [px, setPx] = useState(init.wrapX || 0), [py, setPy] = useState(init.wrapY || 0)
  const [hRelPos, setHRelPos] = useState(0), [vRelPos, setVRelPos] = useState(0)
  const [hPosRef, setHPosRef] = useState(init.posHRel || 'column')
  const [vPosRef, setVPosRef] = useState(init.posVRel || 'paragraph')
  const [moveWithText, setMoveWithText] = useState(init.moveWithText)
  const [allowOverlap, setAllowOverlap] = useState(init.allowOverlap)
  const [lockAnchor, setLockAnchor] = useState(init.lockAnchor)
  const posDisabled = wrap === 'inline'

  const apply = () => {
    const fw = wMode === 'rel' ? Math.round((wRelPct / 100) * (wRelRef === 'page' ? init.pageW : init.contentW)) : w
    const fh = hMode === 'rel' ? Math.round((hRelPct / 100) * (hRelRef === 'page' ? init.pageH : init.contentH)) : h
    let wrapX = px, wrapY = py
    if (hPos === 'rel') wrapX = Math.round((hRelPos / 100) * (hPosRef === 'page' ? init.pageW : init.contentW))
    else if (hPos === 'align') wrapX = hAlign === 'center' ? Math.round((init.contentW - fw) / 2) : hAlign === 'right' ? Math.round(init.contentW - fw) : 0
    if (vPos === 'rel') wrapY = Math.round((vRelPos / 100) * (vPosRef === 'page' ? init.pageH : init.contentH))
    else if (vPos === 'align') wrapY = vAlign === 'center' ? Math.round((init.contentH - fh) / 2) : vAlign === 'bottom' ? Math.round(init.contentH - fh) : 0
    onApply({
      width: Math.max(8, fw), height: Math.max(8, fh), rotation: ((Math.round(rot) % 360) + 360) % 360,
      wrap, align: hAlign, wrapX: Math.round(wrapX), wrapY: Math.round(wrapY),
      wrapSide, wrapDistT: Math.round(dT), wrapDistB: Math.round(dB), wrapDistL: Math.round(dL), wrapDistR: Math.round(dR),
      posHRel: hPosRef, posVRel: vPosRef, moveWithText, allowOverlap, lockAnchor,
    })
    onClose()
  }

  const H = ({ children }: { children: React.ReactNode }) => <div className="text-[13px] font-semibold text-text-secondary border-b border-border/60 pb-1">{children}</div>
  const Tab = ({ id, label }: { id: typeof tab; label: string }) => (
    <button onClick={() => setTab(id)} className={`px-3 py-1.5 text-sm border-b-2 -mb-px ${tab === id ? 'border-primary text-primary font-medium' : 'border-transparent text-text-secondary hover:text-text-primary'}`}>{label}</button>
  )
  const REF_H: Array<[string, string]> = [['column', t('doc_layout_column', { defaultValue: 'Colonne' })], ['margin', t('doc_layout_margin', { defaultValue: 'Marge' })], ['page', t('doc_layout_page', { defaultValue: 'Page' })], ['character', t('doc_layout_char', { defaultValue: 'Caractère' })]]
  const REF_V: Array<[string, string]> = [['paragraph', t('doc_layout_paragraph', { defaultValue: 'Paragraphe' })], ['margin', t('doc_layout_margin', { defaultValue: 'Marge' })], ['page', t('doc_layout_page', { defaultValue: 'Page' })], ['line', t('doc_layout_line', { defaultValue: 'Ligne' })]]
  const REL_REF: Array<[string, string]> = [['margin', t('doc_layout_margin', { defaultValue: 'Marge' })], ['page', t('doc_layout_page', { defaultValue: 'Page' })]]
  const ALIGN_H: Array<[string, string]> = [['left', t('doc_align_left', { defaultValue: 'À gauche' })], ['center', t('doc_align_center', { defaultValue: 'Centré' })], ['right', t('doc_align_right', { defaultValue: 'À droite' })]]
  const ALIGN_V: Array<[string, string]> = [['top', t('doc_layout_top', { defaultValue: 'Haut' })], ['center', t('doc_layout_middle', { defaultValue: 'Centré' })], ['bottom', t('doc_layout_bottom', { defaultValue: 'Bas' })]]

  return (
    <FloatingWindow title={t('doc_layout_dialog', { defaultValue: 'Mise en page' })} onClose={onClose} defaultWidth={640} backdrop>
      <div data-module="office">
        <div className="flex gap-1 border-b border-border px-4 pt-1">
          <Tab id="position" label={t('doc_layout_position', { defaultValue: 'Position' })} />
          <Tab id="wrap" label={t('doc_layout_wrap', { defaultValue: 'Habillage du texte' })} />
          <Tab id="size" label={t('doc_layout_size', { defaultValue: 'Taille' })} />
        </div>
        <div className="p-4 min-h-[320px]">
          {/* ── POSITION ── */}
          {tab === 'position' && (
            <div className="flex flex-col gap-3 text-sm">
              <H>{t('doc_layout_horizontal', { defaultValue: 'Horizontal' })}</H>
              <div className="grid grid-cols-[150px_1fr_auto_auto] items-center gap-x-2 gap-y-2 pl-1">
                <LDRadio checked={hPos === 'align'} on={() => setHPos('align')} label={t('doc_layout_alignment', { defaultValue: 'Alignement' })} />
                <LDSel value={hAlign} on={setHAlign} opts={ALIGN_H} disabled={hPos !== 'align'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={hPosRef} on={setHPosRef} opts={REF_H} disabled={hPos !== 'align'} />
                <LDRadio checked={hPos === 'abs'} on={() => setHPos('abs')} label={t('doc_layout_abs_pos', { defaultValue: 'Position absolue' })} />
                <LDNum value={toCm(px)} on={n => setPx(cmToPx(n))} suffix="cm" step={0.1} disabled={hPos !== 'abs'} />
                <span className="text-text-secondary text-right">{t('doc_layout_right_of', { defaultValue: 'à droite de' })}</span>
                <LDSel value={hPosRef} on={setHPosRef} opts={REF_H} disabled={hPos !== 'abs'} />
                <LDRadio checked={hPos === 'rel'} on={() => setHPos('rel')} label={t('doc_layout_rel_pos', { defaultValue: 'Position relative' })} />
                <LDNum value={hRelPos} on={setHRelPos} suffix="%" disabled={hPos !== 'rel'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={hPosRef} on={setHPosRef} opts={REL_REF} disabled={hPos !== 'rel'} />
              </div>
              <H>{t('doc_layout_vertical', { defaultValue: 'Vertical' })}</H>
              <div className="grid grid-cols-[150px_1fr_auto_auto] items-center gap-x-2 gap-y-2 pl-1">
                <LDRadio checked={vPos === 'align'} on={() => setVPos('align')} label={t('doc_layout_alignment', { defaultValue: 'Alignement' })} />
                <LDSel value={vAlign} on={setVAlign} opts={ALIGN_V} disabled={vPos !== 'align'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={vPosRef} on={setVPosRef} opts={REF_V} disabled={vPos !== 'align'} />
                <LDRadio checked={vPos === 'abs'} on={() => setVPos('abs')} label={t('doc_layout_abs_pos', { defaultValue: 'Position absolue' })} />
                <LDNum value={toCm(py)} on={n => setPy(cmToPx(n))} suffix="cm" step={0.1} disabled={vPos !== 'abs'} />
                <span className="text-text-secondary text-right">{t('doc_layout_below', { defaultValue: 'au-dessous de' })}</span>
                <LDSel value={vPosRef} on={setVPosRef} opts={REF_V} disabled={vPos !== 'abs'} />
                <LDRadio checked={vPos === 'rel'} on={() => setVPos('rel')} label={t('doc_layout_rel_pos', { defaultValue: 'Position relative' })} />
                <LDNum value={vRelPos} on={setVRelPos} suffix="%" disabled={vPos !== 'rel'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={vPosRef} on={setVPosRef} opts={REL_REF} disabled={vPos !== 'rel'} />
              </div>
              <H>{t('doc_layout_options', { defaultValue: 'Options' })}</H>
              <div className="grid grid-cols-2 gap-2 pl-1">
                <LDCheck checked={moveWithText} on={setMoveWithText} label={t('doc_layout_move_with_text', { defaultValue: 'Déplacer avec le texte' })} />
                <LDCheck checked={allowOverlap} on={setAllowOverlap} label={t('doc_layout_allow_overlap', { defaultValue: 'Autoriser le chevauchement de texte' })} />
                <LDCheck checked={lockAnchor} on={setLockAnchor} label={t('doc_layout_lock_anchor', { defaultValue: 'Ancrer' })} />
                <LDCheck checked={false} on={() => {}} disabled label={t('doc_layout_in_cell', { defaultValue: 'Disposition dans la cellule du tableau' })} />
              </div>
              {posDisabled && <div className="text-[12px] text-text-tertiary pl-1">{t('doc_layout_pos_note', { defaultValue: "La position ne s'applique qu'aux objets flottants (carré, derrière ou devant le texte)." })}</div>}
            </div>
          )}
          {/* ── HABILLAGE ── */}
          {tab === 'wrap' && (
            <div className="flex flex-col gap-3 text-sm">
              <H>{t('doc_layout_wrap_style', { defaultValue: "Style d'habillage" })}</H>
              <div className="grid grid-cols-4 gap-2">
                {WRAP_STYLES.map(s => (
                  <button key={s.key} onClick={() => { setWrapKey(s.key); setWrap(s.mode) }}
                    className={`flex flex-col items-center gap-1 p-2 rounded-lg border ${wrapKey === s.key ? 'border-primary bg-primary-light/40' : 'border-border hover:bg-surface-2'}`}>
                    <span className="bg-white rounded">{WRAP_THUMB[s.mode]}</span>
                    <span className="text-[10px] text-text-secondary leading-tight text-center">{s.label}</span>
                  </button>
                ))}
              </div>
              <H>{t('doc_layout_wrap_text', { defaultValue: 'Habiller le texte' })}</H>
              <div className="grid grid-cols-2 gap-2 pl-1">
                <LDRadio checked={wrapSide === 'both'} on={() => setWrapSide('both')} disabled={!sideEnabled} label={t('doc_layout_both_sides', { defaultValue: 'Des deux côtés' })} />
                <LDRadio checked={wrapSide === 'left'} on={() => setWrapSide('left')} disabled={!sideEnabled} label={t('doc_layout_left_only', { defaultValue: 'Seulement à gauche' })} />
                <LDRadio checked={wrapSide === 'right'} on={() => setWrapSide('right')} disabled={!sideEnabled} label={t('doc_layout_right_only', { defaultValue: 'Seulement à droite' })} />
                <LDRadio checked={wrapSide === 'largest'} on={() => setWrapSide('largest')} disabled={!sideEnabled} label={t('doc_layout_largest_only', { defaultValue: 'Seulement le plus grand' })} />
              </div>
              <H>{t('doc_layout_text_dist', { defaultValue: 'Distance du texte' })}</H>
              <div className="grid grid-cols-2 gap-x-8 gap-y-2 pl-1 w-fit">
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_top', { defaultValue: 'Haut' })}</span><LDNum value={toCm(dT)} on={n => setDT(cmToPx(n))} suffix="cm" step={0.05} disabled={!sideEnabled} width="w-20" /></span>
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_left', { defaultValue: 'Gauche' })}</span><LDNum value={toCm(dL)} on={n => setDL(cmToPx(n))} suffix="cm" step={0.05} disabled={!sideEnabled} width="w-20" /></span>
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_bottom', { defaultValue: 'Bas' })}</span><LDNum value={toCm(dB)} on={n => setDB(cmToPx(n))} suffix="cm" step={0.05} disabled={!sideEnabled} width="w-20" /></span>
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_right', { defaultValue: 'Droite' })}</span><LDNum value={toCm(dR)} on={n => setDR(cmToPx(n))} suffix="cm" step={0.05} disabled={!sideEnabled} width="w-20" /></span>
              </div>
            </div>
          )}
          {/* ── TAILLE ── */}
          {tab === 'size' && (
            <div className="flex flex-col gap-3 text-sm">
              <H>{t('doc_layout_height', { defaultValue: 'Hauteur' })}</H>
              <div className="grid grid-cols-[110px_auto_auto_auto] items-center gap-x-2 gap-y-2 pl-1">
                <LDRadio checked={hMode === 'abs'} on={() => setHMode('abs')} label={t('doc_layout_absolute', { defaultValue: 'Absolue' })} />
                <LDNum value={toCm(h)} on={setHAbs} suffix="cm" step={0.1} disabled={hMode !== 'abs'} /><span /><span />
                <LDRadio checked={hMode === 'rel'} on={() => setHMode('rel')} label={t('doc_layout_relative', { defaultValue: 'Relative' })} />
                <LDNum value={hRelPct} on={setHRelPct} suffix="%" disabled={hMode !== 'rel'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={hRelRef} on={setHRelRef} opts={REL_REF} disabled={hMode !== 'rel'} />
              </div>
              <H>{t('doc_layout_width', { defaultValue: 'Largeur' })}</H>
              <div className="grid grid-cols-[110px_auto_auto_auto] items-center gap-x-2 gap-y-2 pl-1">
                <LDRadio checked={wMode === 'abs'} on={() => setWMode('abs')} label={t('doc_layout_absolute', { defaultValue: 'Absolue' })} />
                <LDNum value={toCm(w)} on={setWAbs} suffix="cm" step={0.1} disabled={wMode !== 'abs'} /><span /><span />
                <LDRadio checked={wMode === 'rel'} on={() => setWMode('rel')} label={t('doc_layout_relative', { defaultValue: 'Relative' })} />
                <LDNum value={wRelPct} on={setWRelPct} suffix="%" disabled={wMode !== 'rel'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={wRelRef} on={setWRelRef} opts={REL_REF} disabled={wMode !== 'rel'} />
              </div>
              <H>{t('doc_layout_rotate', { defaultValue: 'Faire pivoter' })}</H>
              <div className="flex items-center gap-2 pl-1"><span className="w-24">{t('doc_layout_rotation', { defaultValue: 'Rotation' })}</span><LDNum value={rot} on={setRot} suffix="°" /></div>
              <H>{t('doc_layout_scale', { defaultValue: 'Échelle' })}</H>
              <div className="flex items-center gap-6 pl-1">
                <span className="flex items-center gap-2"><span className="w-16">{t('doc_layout_height', { defaultValue: 'Hauteur' })}</span><LDNum value={scaleH} on={setScaleH} suffix="%" /></span>
                <span className="flex items-center gap-2"><span>{t('doc_layout_width', { defaultValue: 'Largeur' })}</span><LDNum value={scaleW} on={setScaleW} suffix="%" /></span>
              </div>
              <LDCheck checked={lock} on={setLock} label={t('doc_layout_keep_ratio', { defaultValue: 'Conserver les proportions' })} />
              <div className="flex items-center justify-between border-t border-border/60 pt-2">
                <span className="text-text-secondary">{t('doc_layout_orig_size', { defaultValue: "Taille d'origine" })} : {toCm(baseW)} × {toCm(baseH)} cm</span>
                <button onClick={() => { setW(baseW); setH(baseH); setWMode('abs'); setHMode('abs'); setRot(init.rotation || 0) }}
                  className="rounded-md border border-border px-3 py-1 text-sm text-text-secondary hover:bg-surface-2">{t('doc_layout_reset', { defaultValue: 'Rétablir' })}</button>
              </div>
            </div>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
          <button onClick={onClose} className="rounded-md border border-border px-3 py-1.5 text-sm text-text-secondary hover:bg-surface-2">{t('common_cancel', { defaultValue: 'Annuler' })}</button>
          <Button size="sm" onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

// ── Détails du document ────────────────────────────────────────────────────────

function DocDetailsDialog({ editor, opsRef, title, createdAt, updatedAt, onClose }: {
  editor: Editor | null
  opsRef: React.RefObject<PaginatedOps | null>
  title: string
  createdAt?: string
  updatedAt?: string
  onClose: () => void
}) {
  const { t, i18n: i18nInst } = useTranslation('office')
  const words = editor?.storage.characterCount?.words() ?? 0
  const chars = editor?.storage.characterCount?.characters() ?? 0
  const pages = opsRef.current?.pageCount() ?? 1
  const fmt = (d?: string) => d ? new Date(d).toLocaleString(i18nInst.language) : '—'
  const rows: Array<[string, string | number]> = [
    [t('doc_details_title', { defaultValue: 'Titre' }), title || '—'],
    [t('doc_pages_count', { defaultValue: 'Pages' }), pages],
    [t('doc_words'), words],
    [t('doc_characters'), chars],
    [t('doc_details_created', { defaultValue: 'Créé le' }), fmt(createdAt)],
    [t('doc_details_updated', { defaultValue: 'Modifié le' }), fmt(updatedAt)],
  ]
  return (
    <FloatingWindow title={t('doc_document_details')} onClose={onClose} defaultWidth={340} backdrop>
      <div className="p-5" data-module="office">
        <div className="space-y-1 mb-4">
          {rows.map(([k, v]) => (
            <div key={k} className="flex justify-between gap-6 text-sm">
              <span className="text-text-secondary">{k}</span>
              <span className="font-medium text-text-primary text-right">{v}</span>
            </div>
          ))}
        </div>
        <Button className="w-full" onClick={onClose}>{t('common_close')}</Button>
      </div>
    </FloatingWindow>
  )
}

// Statistiques détaillées (Word « Statistiques ») : mots, caractères (avec/sans
// espaces), paragraphes, phrases, pages + temps de lecture estimé (~200 mots/min).
function DocWordCountDialog({ editor, opsRef, onClose }: {
  editor: Editor | null; opsRef: React.RefObject<PaginatedOps | null>; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const cc = editor?.storage.characterCount as { words?: () => number; characters?: () => number } | undefined
  const words = cc?.words?.() ?? 0
  const chars = cc?.characters?.() ?? 0
  let text = '', paras = 0
  editor?.state.doc.descendants(n => {
    if (n.type.name === 'paragraph' || n.type.name === 'heading') { if (n.textContent.trim()) paras++; text += n.textContent + '\n' }
  })
  const noSpaces = text.replace(/\s/g, '').length
  const sentences = (text.match(/[.!?…]+/g) || []).length
  const pages = opsRef.current?.pageCount() ?? 1
  const readMin = Math.max(1, Math.round(words / 200))
  const sel = editor?.state.selection
  let selWords = 0
  if (editor && sel && sel.to > sel.from) selWords = (editor.state.doc.textBetween(sel.from, sel.to, ' ').match(/\S+/g) || []).length
  const rows: Array<[string, string | number]> = [
    [t('doc_pages_count', { defaultValue: 'Pages' }), pages],
    [t('doc_words'), words],
    [t('doc_wc_chars_spaces', { defaultValue: 'Caractères (avec espaces)' }), chars],
    [t('doc_wc_chars_nospaces', { defaultValue: 'Caractères (sans espaces)' }), noSpaces],
    [t('doc_wc_paragraphs', { defaultValue: 'Paragraphes' }), paras],
    [t('doc_wc_sentences', { defaultValue: 'Phrases' }), sentences],
    [t('doc_wc_reading_time', { defaultValue: 'Temps de lecture' }), `≈ ${readMin} min`],
  ]
  if (selWords) rows.splice(2, 0, [t('doc_wc_sel_words', { defaultValue: 'Mots sélectionnés' }), selWords])
  return (
    <FloatingWindow title={t('doc_word_count', { defaultValue: 'Statistiques' })} onClose={onClose} defaultWidth={360} backdrop>
      <div className="p-5" data-module="office">
        <div className="space-y-1 mb-4">
          {rows.map(([k, v]) => (
            <div key={k} className="flex justify-between gap-6 text-sm">
              <span className="text-text-secondary">{k}</span>
              <span className="font-medium text-text-primary text-right">{v}</span>
            </div>
          ))}
        </div>
        <Button className="w-full" onClick={onClose}>{t('common_close')}</Button>
      </div>
    </FloatingWindow>
  )
}

// Atteindre (Word « Atteindre ») : liste les titres et signets ; clic → défilement.
function DocGoToDialog({ editor, opsRef, onClose }: {
  editor: Editor | null; opsRef: React.RefObject<PaginatedOps | null>; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const headings = opsRef.current?.outline() ?? []
  const bookmarks: Array<{ name: string; pos: number }> = []
  editor?.state.doc.descendants((node, pos) => {
    if (node.isText) { const m = node.marks.find(mk => mk.type.name === 'bookmark'); if (m && m.attrs.name) bookmarks.push({ name: String(m.attrs.name), pos }) }
  })
  const go = (pos: number) => { opsRef.current?.scrollToPos(pos); onClose() }
  return (
    <FloatingWindow title={t('doc_go_to', { defaultValue: 'Atteindre' })} onClose={onClose} defaultWidth={360} backdrop>
      <div className="p-3 max-h-[60vh] overflow-auto" data-module="office">
        {!headings.length && !bookmarks.length && (
          <p className="text-xs text-text-tertiary text-center py-6">{t('doc_goto_empty', { defaultValue: 'Aucun titre ni signet.' })}</p>
        )}
        {!!headings.length && <div className="px-1 py-1 text-[11px] uppercase tracking-wide text-text-tertiary">{t('doc_goto_headings', { defaultValue: 'Titres' })}</div>}
        {headings.map((h, i) => (
          <button key={'h' + i} onClick={() => go(h.pos + 1)} style={{ paddingLeft: 8 + (h.level - 1) * 12 }}
            className="w-full text-left px-2 py-1.5 rounded hover:bg-hover text-sm text-text-secondary flex justify-between gap-2">
            <span className="truncate">{h.text}</span><span className="text-text-tertiary text-xs">p.{h.page}</span>
          </button>
        ))}
        {!!bookmarks.length && <div className="px-1 py-1 mt-1 text-[11px] uppercase tracking-wide text-text-tertiary border-t border-border">{t('doc_goto_bookmarks', { defaultValue: 'Signets' })}</div>}
        {bookmarks.map((b, i) => (
          <button key={'b' + i} onClick={() => go(b.pos)} className="w-full text-left px-2 py-1.5 rounded hover:bg-hover text-sm text-text-secondary flex items-center gap-2">
            <Bookmark size={13} className="text-accent flex-shrink-0" /><span className="truncate">{b.name}</span>
          </button>
        ))}
      </div>
    </FloatingWindow>
  )
}

// Orientation du texte d'une cellule (Word « Orientation du texte - Cellule de
// tableau ») : 3 orientations (horizontal / vertical bas→haut / vertical haut→bas) +
// aperçu. Applique l'attribut cellDir (0 / 270 / 90) à la plage de cellules.
function TextOrientationDialog({ editor, rect, onClose }: {
  editor: Editor | null; rect: TableRect; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const init = ((editor?.getAttributes('tableCell').cellDir as number) || 0) as 0 | 90 | 270
  const [dir, setDir] = useState<0 | 90 | 270>(init)
  const SAMPLE = t('doc_orient_sample', { defaultValue: 'Servez à ce monsieur une bière et des kiwis.' })
  const tile = (d: 0 | 90 | 270, vertical: boolean) => (
    <button onClick={() => setDir(d)}
      className={`flex items-center justify-center bg-white ${d === 0 ? 'w-[200px] h-12' : 'w-14 h-32'} border-2 rounded ${dir === d ? 'border-accent' : 'border-border'}`}>
      <span style={d === 0 ? undefined : { transform: `rotate(${d === 270 ? -90 : 90}deg)` }} className="text-text-primary text-sm whitespace-nowrap">{t('doc_orient_text', { defaultValue: 'Texte' })}</span>
    </button>
  )
  const apply = () => { if (editor) setCellsAttr(editor, rect, { cellDir: dir }); onClose() }
  return (
    <FloatingWindow title={t('doc_text_orientation_title', { defaultValue: 'Orientation du texte - Cellule de tableau' })} onClose={onClose} defaultWidth={560} backdrop>
      <div className="p-5 flex flex-col gap-4" data-module="office">
        <div className="flex gap-6">
          <fieldset className="border border-border rounded p-3 flex-1">
            <legend className="px-1 text-sm text-text-secondary">{t('doc_orientation', { defaultValue: 'Orientation' })}</legend>
            <div className="flex flex-col items-center gap-3 py-2">
              {tile(0, false)}
              <div className="flex gap-3">{tile(270, true)}{tile(90, true)}</div>
            </div>
          </fieldset>
          <fieldset className="border border-border rounded p-3 w-[200px]">
            <legend className="px-1 text-sm text-text-secondary">{t('doc_preview', { defaultValue: 'Aperçu' })}</legend>
            <div className="bg-white border border-border h-40 overflow-hidden flex p-2" style={{ writingMode: dir === 0 ? 'horizontal-tb' : 'vertical-rl', transform: dir === 270 ? 'rotate(180deg)' : undefined }}>
              <span className="text-sm text-text-primary leading-relaxed">{SAMPLE}</span>
            </div>
          </fieldset>
        </div>
        <label className="flex items-center gap-2 text-sm opacity-50">
          <span className="text-text-secondary">{t('doc_apply_to', { defaultValue: 'Appliquer à :' })}</span>
          <Dropdown width={220} value="sel" disabled options={[{ value: 'sel', label: t('doc_selected_cells', { defaultValue: 'Cellules sélectionnées' }) }]} onChange={() => {}} />
        </label>
        <div className="flex justify-end gap-2 pt-1">
          <Button onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          <Button variant="secondary" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

// Propriétés du tableau (Word) : onglets Tableau / Ligne / Colonne / Cellule / Texte
// de remplacement. Modifie les attributs du tableau (alignement, retrait, largeurs de
// colonne, hauteurs de ligne + mode, texte alt) et des cellules (alignement vertical).
// Les réglages sont appliqués à la validation (OK) ; Annuler ferme sans rien changer.
const CM_PX = 96 / 2.54
function TablePropertiesDialog({ editor, rect, onClose }: { editor: Editor | null; rect: TableRect; onClose: () => void }) {
  const { t } = useTranslation('office')
  const ctx = editor ? tableCtxOf(editor) : null
  const node = ctx?.tableNode
  const colCount = (() => { let n = 0; node?.child(0)?.forEach(c => { n += (c.attrs.colspan as number) || 1 }); return Math.max(1, n) })()
  const rowCount = node?.childCount ?? 1
  const a = (node?.attrs ?? {}) as Record<string, unknown>
  const [tab, setTab] = useState<'table' | 'row' | 'col' | 'cell' | 'alt'>('table')
  const [align, setAlign] = useState<'left' | 'center' | 'right'>((a.tableAlign as 'left' | 'center' | 'right') || 'left')
  const [indentCm, setIndentCm] = useState(((a.tableIndent as number) || 0) / CM_PX)
  const [tblWOn, setTblWOn] = useState(!!a.colWidths)
  const [colW, setColW] = useState<number[]>(() => (Array.isArray(a.colWidths) ? (a.colWidths as number[]).slice() : new Array(colCount).fill(0)))
  const [rowH, setRowH] = useState<number[]>(() => (Array.isArray(a.rowHeights) ? (a.rowHeights as number[]).slice() : new Array(rowCount).fill(0)))
  const [rowModes, setRowModes] = useState<Array<'atleast' | 'exactly'>>(() => (Array.isArray(a.rowHeightModes) ? (a.rowHeightModes as Array<'atleast' | 'exactly'>).slice() : new Array(rowCount).fill('atleast')))
  const [allowBreak, setAllowBreak] = useState(true)
  const [valign, setVAlign] = useState<'top' | 'center' | 'bottom'>((editor?.getAttributes('tableCell').cellVAlign as 'top' | 'center' | 'bottom') || 'top')
  const [altTitle, setAltTitle] = useState((a.altTitle as string) || '')
  const [altDesc, setAltDesc] = useState((a.altDesc as string) || '')
  const [curRow, setCurRow] = useState(ctx?.rowIndex ?? 0)
  const [curCol, setCurCol] = useState(ctx?.colStart ?? 0)
  if (!editor || !ctx || !node) return null

  const cmField = (val: number, on: (n: number) => void, disabled = false) => (
    <NumberInput className="w-[110px] h-8" min={0} max={100} step={0.1} disabled={disabled} value={Math.round(val * 100) / 100} onChange={n => on(n)} />
  )
  const measuredIn = (
    <div className="flex items-center gap-2"><span className="text-text-secondary text-sm">{t('doc_tp_measure', { defaultValue: 'Mesurer en :' })}</span>
      <Dropdown width={150} value="cm" options={[{ value: 'cm', label: t('doc_tp_cm', { defaultValue: 'Centimètres' }) }]} onChange={() => {}} /></div>
  )
  const alignTile = (v: 'left' | 'center' | 'right', label: string) => (
    <button onClick={() => setAlign(v)} className={`flex flex-col items-center gap-1`}>
      <span className={`w-20 h-16 border-2 rounded flex items-center justify-center ${align === v ? 'border-accent' : 'border-border'}`}>
        <span className={`w-10 h-9 border border-text-tertiary flex ${v === 'center' ? 'justify-center' : v === 'right' ? 'justify-end' : 'justify-start'} items-start p-0.5`}><span className="w-5 border-t border-text-tertiary" /></span>
      </span>
      <span className="text-xs text-text-secondary">{label}</span>
    </button>
  )
  const vAlignTile = (v: 'top' | 'center' | 'bottom', label: string) => (
    <button onClick={() => setVAlign(v)} className="flex flex-col items-center gap-1">
      <span className={`w-20 h-16 border-2 rounded flex ${v === 'center' ? 'items-center' : v === 'bottom' ? 'items-end' : 'items-start'} justify-center p-1 ${valign === v ? 'border-accent' : 'border-border'}`}>
        <span className="w-12 border-t border-text-tertiary" />
      </span>
      <span className="text-xs text-text-secondary">{label}</span>
    </button>
  )
  const apply = () => {
    const attrs: Record<string, unknown> = {
      tableAlign: align, tableIndent: align === 'left' ? Math.round(indentCm * CM_PX) : 0,
      colWidths: tblWOn && colW.some(w => w > 0) ? colW : null,
      rowHeights: rowH.some(h => h > 0) ? rowH : null,
      rowHeightModes: rowModes.some(m => m === 'exactly') ? rowModes : null,
      altTitle: altTitle.trim() || null, altDesc: altDesc.trim() || null,
    }
    setTableAttrAt(editor, ctx.tablePos, attrs)
    setCellsAttr(editor, rect, { cellVAlign: valign })
    onClose()
  }
  const TABS: Array<[typeof tab, string]> = [
    ['table', t('doc_tp_tab_table', { defaultValue: 'Tableau' })], ['row', t('doc_tp_tab_row', { defaultValue: 'Ligne' })],
    ['col', t('doc_tp_tab_col', { defaultValue: 'Colonne' })], ['cell', t('doc_tp_tab_cell', { defaultValue: 'Cellule' })],
    ['alt', t('doc_tp_tab_alt', { defaultValue: 'Texte de remplacement' })],
  ]
  return (
    <FloatingWindow title={t('doc_table_properties_title', { defaultValue: 'Propriétés du tableau' })} onClose={onClose} defaultWidth={560} backdrop>
      <div className="p-4 flex flex-col gap-3 text-sm" data-module="office">
        <div className="flex gap-1 border-b border-border">
          {TABS.map(([k, l]) => (
            <button key={k} onClick={() => setTab(k)} className={`px-3 py-1.5 text-sm border-b-2 -mb-px ${tab === k ? 'border-accent text-accent' : 'border-transparent text-text-secondary hover:bg-hover'}`}>{l}</button>
          ))}
        </div>
        <div className="min-h-[300px]">
          {tab === 'table' && (
            <div className="flex flex-col gap-4">
              <div>
                <div className="text-text-secondary mb-1">{t('doc_tp_size', { defaultValue: 'Taille' })}</div>
                <div className="flex items-center gap-3">
                  <label className="flex items-center gap-2"><Checkbox checked={tblWOn} onChange={setTblWOn} /><span className="text-text-secondary">{t('doc_tp_pref_width', { defaultValue: 'Largeur préférée :' })}</span></label>
                  {cmField(colW.reduce((s, w) => s + w, 0) / CM_PX, cm => { const total = cm * CM_PX; const cur = colW.reduce((s, w) => s + w, 0) || 1; setColW(colW.map(w => (w || cur / colCount) * (total / cur))) }, !tblWOn)}
                  {measuredIn}
                </div>
              </div>
              <div>
                <div className="text-text-secondary mb-1">{t('doc_tp_align', { defaultValue: 'Alignement' })}</div>
                <div className="flex items-end gap-6">
                  {alignTile('left', t('doc_align_left', { defaultValue: 'Gauche' }))}
                  {alignTile('center', t('doc_align_center', { defaultValue: 'Centré' }))}
                  {alignTile('right', t('doc_align_right', { defaultValue: 'Droite' }))}
                  <label className="flex flex-col gap-1 text-xs text-text-secondary">{t('doc_tp_indent_left', { defaultValue: 'Retrait à gauche :' })}{cmField(indentCm, setIndentCm, align !== 'left')}</label>
                </div>
              </div>
            </div>
          )}
          {tab === 'row' && (
            <div className="flex flex-col gap-4">
              <div className="text-text-secondary">{t('doc_tp_row_n', { defaultValue: 'Ligne {{n}} :', n: curRow + 1 })}</div>
              <div className="flex items-center gap-3">
                <label className="flex items-center gap-2"><Checkbox checked={(rowH[curRow] || 0) > 0} onChange={v => setRowH(rowH.map((h, i) => i === curRow ? (v ? Math.max(0.5 * CM_PX, h) : 0) : h))} /><span className="text-text-secondary">{t('doc_tp_specify_height', { defaultValue: 'Spécifier la hauteur :' })}</span></label>
                {cmField((rowH[curRow] || 0) / CM_PX, cm => setRowH(rowH.map((h, i) => i === curRow ? cm * CM_PX : h)), (rowH[curRow] || 0) <= 0)}
                <div className="flex items-center gap-2"><span className="text-text-secondary">{t('doc_tp_row_height', { defaultValue: 'Hauteur :' })}</span>
                  <Dropdown width={120} value={rowModes[curRow] || 'atleast'} options={[{ value: 'atleast', label: t('doc_tp_atleast', { defaultValue: 'Au moins' }) }, { value: 'exactly', label: t('doc_tp_exactly', { defaultValue: 'Exactement' }) }]} onChange={v => setRowModes(rowModes.map((m, i) => i === curRow ? v as 'atleast' | 'exactly' : m))} /></div>
              </div>
              <label className="flex items-center gap-2"><Checkbox checked={allowBreak} onChange={setAllowBreak} /><span className="text-text-secondary">{t('doc_tp_allow_break', { defaultValue: 'Autoriser le fractionnement des lignes sur plusieurs pages' })}</span></label>
              <div className="flex gap-2">
                <Button variant="secondary" disabled={curRow <= 0} onClick={() => setCurRow(r => Math.max(0, r - 1))}>▲ {t('doc_tp_prev_row', { defaultValue: 'Ligne précédente' })}</Button>
                <Button variant="secondary" disabled={curRow >= rowCount - 1} onClick={() => setCurRow(r => Math.min(rowCount - 1, r + 1))}>▼ {t('doc_tp_next_row', { defaultValue: 'Ligne suivante' })}</Button>
              </div>
            </div>
          )}
          {tab === 'col' && (
            <div className="flex flex-col gap-4">
              <div className="text-text-secondary">{t('doc_tp_col_n', { defaultValue: 'Colonne {{n}} :', n: curCol + 1 })}</div>
              <div className="flex items-center gap-3">
                <label className="flex items-center gap-2"><Checkbox checked={(colW[curCol] || 0) > 0} onChange={v => setColW(colW.map((w, i) => i === curCol ? (v ? Math.max(CM_PX, w) : 0) : w))} /><span className="text-text-secondary">{t('doc_tp_pref_width', { defaultValue: 'Largeur préférée :' })}</span></label>
                {cmField((colW[curCol] || 0) / CM_PX, cm => setColW(colW.map((w, i) => i === curCol ? cm * CM_PX : w)), (colW[curCol] || 0) <= 0)}
                {measuredIn}
              </div>
              <div className="flex gap-2">
                <Button variant="secondary" disabled={curCol <= 0} onClick={() => setCurCol(c => Math.max(0, c - 1))}>← {t('doc_tp_prev_col', { defaultValue: 'Colonne précédente' })}</Button>
                <Button variant="secondary" disabled={curCol >= colCount - 1} onClick={() => setCurCol(c => Math.min(colCount - 1, c + 1))}>→ {t('doc_tp_next_col', { defaultValue: 'Colonne suivante' })}</Button>
              </div>
            </div>
          )}
          {tab === 'cell' && (
            <div className="flex flex-col gap-4">
              <div>
                <div className="text-text-secondary mb-1">{t('doc_tp_size', { defaultValue: 'Taille' })}</div>
                <div className="flex items-center gap-3">
                  <label className="flex items-center gap-2"><Checkbox checked={(colW[curCol] || 0) > 0} onChange={v => setColW(colW.map((w, i) => i === curCol ? (v ? Math.max(CM_PX, w) : 0) : w))} /><span className="text-text-secondary">{t('doc_tp_pref_width', { defaultValue: 'Largeur préférée :' })}</span></label>
                  {cmField((colW[curCol] || 0) / CM_PX, cm => setColW(colW.map((w, i) => i === curCol ? cm * CM_PX : w)), (colW[curCol] || 0) <= 0)}
                  {measuredIn}
                </div>
              </div>
              <div>
                <div className="text-text-secondary mb-1">{t('doc_tp_valign', { defaultValue: 'Alignement vertical' })}</div>
                <div className="flex gap-6">
                  {vAlignTile('top', t('doc_ps_valign_top', { defaultValue: 'Haut' }))}
                  {vAlignTile('center', t('doc_ps_valign_center', { defaultValue: 'Centré' }))}
                  {vAlignTile('bottom', t('doc_ps_valign_bottom', { defaultValue: 'Bas' }))}
                </div>
              </div>
            </div>
          )}
          {tab === 'alt' && (
            <div className="flex flex-col gap-3">
              <label className="flex flex-col gap-1"><span className="text-text-secondary">{t('doc_tp_alt_title', { defaultValue: 'Titre' })}</span>
                <input value={altTitle} onChange={e => setAltTitle(e.target.value)} className="px-2 py-1.5 rounded border border-border bg-surface outline-none focus:border-accent" /></label>
              <label className="flex flex-col gap-1"><span className="text-text-secondary">{t('doc_tp_alt_desc', { defaultValue: 'Description' })}</span>
                <textarea value={altDesc} onChange={e => setAltDesc(e.target.value)} rows={5} className="px-2 py-1.5 rounded border border-border bg-surface outline-none focus:border-accent resize-none" /></label>
              <p className="text-xs text-text-tertiary">{t('doc_tp_alt_help', { defaultValue: 'Les titres et descriptions fournissent des représentations textuelles des informations contenues dans le tableau, pour les personnes en situation de handicap visuel ou cognitif.' })}</p>
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border pt-3">
          <Button onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          <Button variant="secondary" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

// ── Trashed document banner ────────────────────────────────────────────────────

function TrashedDocActions({ docId }: { docId: string }) {
  const { t } = useTranslation('office')
  const navigate = useNavigate()
  const { restoreDoc, deleteDoc } = useOfficeStore()
  return (
    <div className="flex gap-2 px-4 py-2 border-b border-border bg-warning-light flex-shrink-0 items-center">
      <span className="text-xs text-text-secondary flex-1">{t('doc_in_trash')}</span>
      <button
        onClick={async () => { await restoreDoc(docId); navigate(`/office/documents/${docId}`) }}
        className="text-xs text-primary hover:underline flex items-center gap-1"
      >
        <RotateCcw size={11} /> {t('doc_restore')}
      </button>
      <button
        onClick={async () => { await deleteDoc(docId); navigate('/office/documents') }}
        className="text-xs text-danger hover:underline"
      >
        {t('doc_delete_permanently')}
      </button>
    </div>
  )
}

function TrashedDocBanner({ docId }: { docId: string }) {
  const { activeDoc } = useOfficeStore()
  if (!activeDoc?.is_trashed) return null
  return <TrashedDocActions docId={docId} />
}

// ════════════════════════════════════════════════════════════════════════════
// NOUVELLE ARCHITECTURE : un seul modèle ProseMirror + rendu canvas paginé.
// L'éditeur (caché) est l'unique source de vérité (contenu + curseur + sélection).
// Le canvas n'est QUE le rendu, paginé via paginate(). Sélection/curseur/Suppr/
// copier/Ctrl+A deviennent natifs et corrects, sans logique inter-pages.
// ════════════════════════════════════════════════════════════════════════════

function flattenToDoc(raw: object | null): JSONContent {
  const { pages } = parseDocContent(raw)
  const content: JSONContent[] = []
  for (const pg of pages) {
    const c = (pg.content as JSONContent).content
    if (Array.isArray(c)) content.push(...c)
  }
  return { type: 'doc', content: content.length ? content : [{ type: 'paragraph' }] }
}

interface PaginatedOps {
  setOrientation:    (o: Orientation) => void
  setColumns:        (n: number) => void
  insertBreak:       () => void
  insertPageBreak:   () => void
  pageCount:         () => number
  /** Plan du document : titres avec niveau, position PM et numéro de page. */
  outline:           () => Array<{ text: string; level: number; pos: number; page: number }>
  /** Place le curseur à `pos` et amène la page correspondante à l'écran. */
  scrollToPos:       (pos: number) => void
  /** Rend chaque page sur un canvas hors écran (échelle ×n) — export PDF. */
  exportPageCanvases: (scale?: number) => Array<{ canvas: HTMLCanvasElement; wPx: number; hPx: number }>
  /** En-tête/pied : contexte de la section du curseur (liaison Word). */
  hfContext:    (kind: 'header' | 'footer') => { secIdx: number; linked: boolean; zones: HFContent; firstPage: boolean }
  setSectionHF: (kind: 'header' | 'footer', zones: HFContent, linked: boolean) => { applyBase: boolean }
  /** Fond de page de la section du curseur ; false = base (fond global). */
  setSectionBg: (color: string | null) => boolean
  /** Édition INLINE en-tête/pied (façon Word). */
  enterHF:  (kind: 'header' | 'footer') => void
  exitHF:   () => void
  switchHF: () => void
  insertHFField: (token: string) => void
  /** Zones de texte riches (canvas) : insérer une nouvelle boîte / éditer celle à `pos`. */
  insertTextBox: () => void
  editTextBox:   (pos: number) => void
  /** Position PM d'ancrage d'un commentaire (début de sa marque) ou null si l'ancre a disparu. */
  commentAnchor: (id: string) => number | null
  /** Dimensions (px @ zoom 1) de la page courante — pour le zoom « ajuster ». */
  pageGeom: () => { pageW: number; pageH: number }
  /** Position/taille (px, repère du contenu défilable) du canvas de la page `idx` —
   *  pour recaler les règles sur la page active en disposition grille. */
  pageContentBox: (idx: number) => { left: number; top: number; w: number; h: number } | null
}

// Curseur d'un participant distant, projeté en coordonnées écran (overlay).
interface RemoteCursor {
  clientId: number
  name:     string
  color:    string
  left:     number
  top:      number
  height:   number
}

// Métadonnées en-tête/pied/fond PROPRES à une section (depuis sectionBreak).
interface SectionHFMeta {
  hfLinked:  boolean
  header:    HFContent | null
  footer:    HFContent | null
  pageColor: string | null
}

// Page d'affichage d'une position PM : la DERNIÈRE page contenant la position
// (priorité au début de page aux limites) ; à défaut (layout en retard d'une
// frappe, position en toute fin de doc), la dernière page commençant avant la
// position — jamais la page 1 par défaut (sinon caret « téléporté »).
function pageIndexForHead(pgs: PageLayout[], head: number): number {
  let idx = -1
  for (let k = 0; k < pgs.length; k++) {
    const has = pgs[k].layout.paragraphs.some(p => p.lines.some(ln => head >= ln.pmStart && head <= ln.pmEnd))
    if (has) idx = k
  }
  if (idx < 0) {
    for (let k = 0; k < pgs.length; k++) {
      const first = pgs[k].layout.paragraphs[0]
      if (first && head >= first.pmStart) idx = k
    }
  }
  return Math.max(0, idx)
}

// Localisation ROBUSTE du caret (page + coords locales) à partir d'une position PM.
// Évite toute « téléportation » multi-pages : on prend d'abord la page qui contient
// RÉELLEMENT `head` dans une ligne rendue (priorité au début de page aux limites) ;
// sinon (nœud de saut, gap, layout transitoire) on retombe sur les coordonnées du
// layout CONTINU (toujours définies) puis on mappe le y global → page. Ainsi le caret
// n'atterrit jamais en (0,0) d'une mauvaise page.
function caretLocation(pgs: PageLayout[], contLayout: DocumentLayout | null, head: number, preferEnd = false): { idx: number; cm: CursorMetrics } {
  for (let k = pgs.length - 1; k >= 0; k--) {
    for (const para of pgs[k].layout.paragraphs) {
      for (const ln of para.lines) {
        if (head >= ln.pmStart && head <= ln.pmEnd) return { idx: k, cm: posToCoords(pgs[k].layout, head, preferEnd) }
      }
    }
  }
  if (contLayout && pgs.length) {
    const gc = posToCoords(contLayout, head, preferEnd)
    let idx = 0
    for (let k = 0; k < pgs.length; k++) if (gc.y >= pgs[k].startY - 0.5) idx = k
    return { idx, cm: { ...gc, y: gc.y - (pgs[idx]?.startY ?? 0) } }
  }
  return { idx: 0, cm: { x: 0, y: 0, height: 0, italicAngle: 0 } }
}

interface PaginatedEditorProps {
  initialDoc:         JSONContent
  ydoc:               Y.Doc           // document Yjs partagé (collaboration temps réel)
  awareness:          Awareness       // présence + curseurs des autres participants
  collabEmpty:        boolean | null  // null=pas encore sync ; true=salle vide (→ seed depuis initialDoc)
  section:            SectionDef
  zoom:               number
  scrollContainerRef: React.RefObject<HTMLDivElement | null>
  onEditor:           (ed: Editor | null) => void
  onSave:             (doc: JSONContent) => void
  onBaseChange?:      (patch: { orientation?: Orientation; columns?: number }) => void
  onActiveSection?:   (orientation: Orientation) => void
  onRegisterOps?:     (ops: PaginatedOps | null) => void
  pageNumbers?:       PageNumbers
  header?:            HFContent
  footer?:            HFContent
  hfFirstPage?:       boolean
  paper?:             PaperSize
  docTitle?:          string
  pageBg?:            string
  /** Filigrane du document (peint derrière le contenu de chaque page). */
  watermark?:         WatermarkDef | null
  /** Bordure de page (cadre dans la marge). */
  pageBorder?:        PageBorderDef | null
  /** Numéros de lignes (marge gauche). */
  lineNumbers?:       LineNumbersDef | null
  /** Affiche les limites de la zone de texte (cadre pointillé dans la marge). */
  showBoundaries?:    boolean
  /** Affiche les marques de paragraphe (¶) en fin de paragraphe. */
  showMarks?:         boolean
  /** Format des numéros de page (arabe / romain / lettres). */
  pageNumFormat?:     PageNumFormat
  /** Premier numéro de page (par défaut 1). */
  pageNumStart?:      number
  /** Entrée/sortie du mode d'édition inline en-tête/pied (barre contextuelle). */
  onHFActive?:        (active: boolean, ctx: HFBarCtx | null, hfEditor: Editor | null) => void
  /** Écriture d'une zone d'en-tête/pied depuis l'édition inline. */
  onCommitHF?:        (kind: 'header' | 'footer', zones: HFContent) => void
  /** Entrée/sortie de l'édition in-place d'une zone de texte riche (routage barre d'outils). */
  onTbActive?:        (active: boolean, tbEditor: Editor | null) => void
  /** Correcteur orthographe/grammaire activé. */
  spellCheck?:        boolean
  /** Remonte le nombre de fautes détectées (badge ruban). */
  onSpellCount?:      (n: number) => void
  /** Remonte les stats de pagination (barre de statut) : total + page du curseur. */
  onStats?:           (s: { pages: number; current: number }) => void
  /** Bump pour forcer une re-vérification orthographe (ex. dictionnaire modifié). */
  spellVersion?:      number
  /** Occurrences de recherche à surligner (positions PM globales) + index actif. */
  searchRanges?:      Array<{ from: number; to: number }>
  searchActive?:      number
  /** Commentaire actuellement sélectionné (surligné plus fort) + activation au clic. */
  activeCommentId?:   string | null
  onCommentActivate?: (id: string | null) => void
  /** Remonte la liste des commentaires ancrés présents dans le document. */
  onCommentRanges?:   (ids: string[]) => void
  /** Crée un commentaire sur la sélection (menu contextuel). */
  onAddComment?:      () => void
  /** Remonte la sélection de plage de cellules de tableau (null = aucune). */
  onTableSel?:        (sel: (TableRect & { tableStart: number }) | null) => void
}

// Contexte transmis à la barre contextuelle d'en-tête/pied (options Word).
export interface HFBarCtx { band: 'header' | 'footer'; secIdx: number; linked: boolean; canLink: boolean; firstPage: boolean }

function PaginatedEditor({ initialDoc, ydoc, awareness, collabEmpty, section, zoom, scrollContainerRef, onEditor, onSave, onBaseChange, onActiveSection, onRegisterOps, pageNumbers = 'none', header, footer, hfFirstPage = false, paper = 'a4', docTitle = '', pageBg, watermark = null, pageBorder = null, lineNumbers = null, showBoundaries = false, showMarks = false, pageNumFormat = 'arabic', pageNumStart = 1, onHFActive, onCommitHF, onTbActive, spellCheck = true, onSpellCount, onStats, spellVersion = 0, searchRanges, searchActive = 0, activeCommentId = null, onCommentActivate, onCommentRanges, onAddComment, onTableSel }: PaginatedEditorProps) {
  const { t, i18n: i18nInst } = useTranslation('office')
  const g = getGeometry(section, paper)
  const cbRef = useRef({ onBaseChange, onActiveSection, onHFActive, onCommitHF, onTbActive, onCommentActivate, onCommentRanges, onAddComment, onTableSel, onStats })
  cbRef.current = { onBaseChange, onActiveSection, onHFActive, onCommitHF, onTbActive, onCommentActivate, onCommentRanges, onAddComment, onTableSel, onStats }
  // Sélection de plage de cellules de tableau (rectangle de grille + table d'ancrage).
  const [tableSel, setTableSel] = useState<(TableRect & { tableStart: number }) | null>(null)
  const tableSelRef = useRef<(TableRect & { tableStart: number }) | null>(null); tableSelRef.current = tableSel
  // Surbrillances de recherche + commentaire actif : refs lues par renderAllPages.
  const searchRef = useRef<{ ranges: Array<{ from: number; to: number }>; active: number }>({ ranges: [], active: 0 })
  searchRef.current = { ranges: searchRanges ?? [], active: searchActive }
  const activeCommentRef = useRef<string | null>(activeCommentId)
  activeCommentRef.current = activeCommentId
  // Plages de commentaires (id → from/to global) recalculées à chaque transaction.
  const commentRangesRef = useRef<Array<{ id: string; from: number; to: number }>>([])
  const pnRef = useRef(pageNumbers); pnRef.current = pageNumbers
  const hfRef = useRef({ header: header ?? emptyHF(), footer: footer ?? emptyHF(), first: hfFirstPage, title: docTitle })
  hfRef.current = { header: header ?? emptyHF(), footer: footer ?? emptyHF(), first: hfFirstPage, title: docTitle }
  const wmRef = useRef(watermark); wmRef.current = watermark
  const pbRef2 = useRef(pageBorder); pbRef2.current = pageBorder
  const lnRef = useRef(lineNumbers); lnRef.current = lineNumbers
  const boundRef = useRef(showBoundaries); boundRef.current = showBoundaries
  const marksRef = useRef(showMarks); marksRef.current = showMarks
  const pnFmtRef = useRef(pageNumFormat); pnFmtRef.current = pageNumFormat
  const pnStartRef = useRef(pageNumStart); pnStartRef.current = pageNumStart
  const paperRef = useRef(paper); paperRef.current = paper
  const secMetaRef = useRef<SectionHFMeta[]>([{ hfLinked: true, header: null, footer: null, pageColor: null }])
  const [pages, setPages]   = useState<PageLayout[]>([])
  // Sélection d'image : centre (px écran) + dimensions non tournées + rotation (la
  // boîte de poignées est tournée autour de son centre comme dans Google Docs).
  const [imgSel, setImgSel] = useState<
    { pos: number; cx: number; cy: number; w: number; h: number; rotation: number; wrap: string } | null
  >(null)
  // Mini-barre flottante (composant partagé FormattingMiniBar) sur sélection du corps.
  const [bodyMiniBar, setBodyMiniBar] = useState<{ left: number; top: number } | null>(null)
  // Panneau « Options de disposition » de l'objet sélectionné (ouvert/fermé).
  const [wrapPanel, setWrapPanel] = useState(false)
  // Curseurs distants (présence collaborative) projetés en coordonnées écran.
  const [remoteCursors, setRemoteCursors] = useState<RemoteCursor[]>([])
  // Mode d'édition INLINE en-tête/pied (façon Word) : zone + page d'ancrage + texte.
  // Édition en-tête/pied : bande + page d'ancrage + doc initial de la bande.
  const [hfEdit, setHfEdit] = useState<{ band: 'header' | 'footer'; pageIdx: number; initial: HFContent } | null>(null)
  const hfEditRef = useRef(hfEdit); hfEditRef.current = hfEdit
  // Éditeur riche de la bande active (RichEditZone) — exposé à la barre d'outils.
  const hfZoneEditorRef = useRef<Editor | null>(null)
  // Édition in-place d'une zone de texte riche : position du nœud + doc initial.
  const [tbEdit, setTbEdit] = useState<{ pos: number; initial: HFContent } | null>(null)
  const tbEditRef = useRef(tbEdit); tbEditRef.current = tbEdit
  const tbZoneEditorRef = useRef<Editor | null>(null)
  const pagesRef            = useRef<PageLayout[]>([])
  const contLayoutRef       = useRef<DocumentLayout | null>(null)
  const canvasRefs          = useRef<Map<number, HTMLCanvasElement>>(new Map())
  const caretRef            = useRef<HTMLDivElement>(null)
  const zoomRef             = useRef(zoom); zoomRef.current = zoom
  const gRef                = useRef(g);    gRef.current = g
  const sectionRef          = useRef(section); sectionRef.current = section
  const geomsRef            = useRef<PageGeometry[]>([g])   // géométrie par section
  const saveTimer           = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const spellTimer          = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const dragAnchorRef       = useRef<number | null>(null)
  const autoScrollRef       = useRef<number | null>(null)
  const lastMouseRef        = useRef<{ x: number; y: number }>({ x: 0, y: 0 })
  const editorRef           = useRef<Editor | null>(null)
  const goalXRef            = useRef<number | null>(null)   // colonne cible (goal column) pour ↑/↓ et Page↑/↓
  const goalXKeepRef        = useRef(false)                 // true = la prochaine MAJ de sélection PRÉSERVE la colonne cible (déplacement vertical) ; sinon elle la réinitialise
  const caretAtEndRef       = useRef(false)                 // affinité curseur : true = fin de ligne visuelle (touche Fin)

  // Géométrie d'une page donnée (selon sa section) avec repli sûr.
  const geomOf = (pg: PageLayout | undefined): PageGeometry =>
    (pg && geomsRef.current[pg.secIdx]) || geomsRef.current[0] || gRef.current
  // Largeur du conteneur = page la plus large (les pages plus étroites sont centrées).
  const maxPageW = () => Math.max(gRef.current.pageW, ...geomsRef.current.map(x => x.pageW))

  // Coin haut-gauche (px) de la page `idx` dans le conteneur d'overlays — lu sur la
  // position RÉELLE du canvas (offsetLeft/Top). Les overlays (caret, sélection,
  // tableaux, curseurs distants) suivent ainsi N'IMPORTE QUELLE disposition : colonne
  // unique OU grille qui s'enroule (« plusieurs pages »). En colonne, vaut exactement
  // l'ancien calcul (somme des hauteurs + centrage), donc transparent.
  const pageOrigin = (idx: number): { left: number; top: number } => {
    const cv = canvasRefs.current.get(idx)
    if (cv) return { left: cv.offsetLeft, top: cv.offsetTop }
    const pgs = pagesRef.current, z = zoomRef.current
    let top = CANVAS_PAD_Y
    for (let k = 0; k < idx; k++) top += geomOf(pgs[k]).pageH * z + PAGE_GAP
    const geom = geomOf(pgs[idx] ?? pgs[0])
    return { left: (maxPageW() - geom.pageW) * z / 2, top }
  }

  // Doc PM pour lequel le layout courant a été calculé. `onSelectionUpdate` peut
  // arriver AVANT le `recompute` de `onUpdate` (ordre d'émission TipTap) : dessiner
  // le caret avec un layout périmé le téléportait (et faisait défiler la vue) vers
  // une mauvaise page. On recompute donc à la demande, sans jamais le faire deux
  // fois pour le même doc (identité du nœud ProseMirror).
  const lastLayoutDocRef = useRef<unknown>(null)
  const recompute = useCallback((ed: Editor, force = false) => {
    if (!force && lastLayoutDocRef.current === ed.state.doc) return
    lastLayoutDocRef.current = ed.state.doc
    const json   = ed.getJSON()
    const geoms  = buildSectionGeoms(json, sectionRef.current, paperRef.current)
    geomsRef.current = geoms
    // Métadonnées par section (en-tête/pied propres + liaison + fond) depuis les
    // nœuds sectionBreak — index 0 = base (en-tête/pied du document via props).
    const metas: SectionHFMeta[] = [{ hfLinked: true, header: null, footer: null, pageColor: null }]
    for (const node of (json as { content?: JSONContent[] }).content ?? []) {
      if (node.type === 'sectionBreak') {
        const a = (node.attrs ?? {}) as Record<string, unknown>
        metas.push({
          hfLinked:  a.hfLinked !== false,
          header:    a.header ? toHFContent(a.header) : null,
          footer:    a.footer ? toHFContent(a.footer) : null,
          pageColor: (a.pageColor as string) ?? null,
        })
      }
    }
    secMetaRef.current = metas
    // Le texte est réagencé à la largeur de COLONNE (= contentW si 1 colonne).
    const layout = layoutDocumentMulti(json, geoms.map(x => x.colW))
    contLayoutRef.current = layout
    const pgs = paginateMulti(layout, geoms.map(x => ({ contentH: x.contentH, columns: x.columns, colW: x.colW, colGap: x.colGap })))
    pagesRef.current = pgs
    setPages(pgs)
  }, [])

  // Zones effectives d'une section : remonte la chaîne « lié au précédent »
  // (Word). Une section DÉLIÉE impose ses zones (même vides) ; sinon héritage
  // jusqu'à la base du document (props header/footer).
  const effectiveHF = useCallback((secIdx: number, kind: 'header' | 'footer'): HFContent => {
    const metas = secMetaRef.current
    for (let s = Math.min(secIdx, metas.length - 1); s >= 1; s--) {
      const m = metas[s]
      if (!m.hfLinked) return m[kind] ?? emptyHF()
    }
    return hfRef.current[kind]
  }, [])

  // Décorations de marge : en-tête / pied RICHES (document ProseMirror rendu via
  // le moteur canvas, mise en forme + images) + numéro de page. Repère page (px CSS).
  const drawPageDecorations = useCallback((cx: CanvasRenderingContext2D, gg: PageGeometry, idx: number, total: number, secIdx = 0, skipBand?: 'header' | 'footer', dimHF = false) => {
    const pn = pnRef.current
    const { first, title } = hfRef.current
    const skipFirst = first && idx === 0   // « 1ʳᵉ page différente » : marges vierges
    const lang = i18nInst.language
    const cw = gg.pageW - 2 * gg.marginH
    // Rend un doc d'en-tête/pied dans sa bande (haut de marge / bas de marge).
    // Hors édition (`dimHF`), on l'estompe (opacité réduite) comme dans Word — la
    // bande reste lisible mais en retrait du corps.
    const renderBand = (doc: HFContent, bandTop: number) => {
      if (isHFEmpty(doc)) return
      const start = pnStartRef.current ?? 1
      const expanded = expandHFDoc(doc, idx + start, total + start - 1, title, lang, pnFmtRef.current)
      const layout = layoutDocument(expanded, cw)
      if (dimHF) {
        cx.save()
        cx.globalAlpha = 0.45
        paintLayoutAt(cx, layout, gg.marginH, bandTop)
        cx.restore()
      } else {
        paintLayoutAt(cx, layout, gg.marginH, bandTop)
      }
    }
    if (!skipFirst) {
      if (skipBand !== 'header') renderBand(effectiveHF(secIdx, 'header'), headerBandTop(gg))
      if (skipBand !== 'footer') renderBand(effectiveHF(secIdx, 'footer'), footerBandTop(gg))
    }
    // Numéro de page « simple » (séparé du contenu riche) si activé.
    if (pn !== 'none' && !skipFirst) {
      cx.font = `${10 * (96 / 72)}px Arial, sans-serif`
      cx.fillStyle = '#5f6368'
      cx.textBaseline = 'alphabetic'
      const label = formatPageNumber(idx + (pnStartRef.current ?? 1), pnFmtRef.current)
      const tw = cx.measureText(label).width
      const yy = pn.startsWith('header') ? gg.marginV * 0.55 : gg.pageH - gg.marginBottom * 0.5
      const xx = pn.endsWith('center') ? (gg.pageW - tw) / 2 : gg.pageW - gg.marginH - tw
      cx.fillText(label, xx, yy)
    }
    // Trame de fond + encadré de paragraphe (Word « Bordures et trame »). On lit les
    // attributs `shading`/`paraBorder` du nœud via le doc, et on peint d'après la
    // géométrie de la page : trame en `destination-over` (sous le texte), bordure
    // au-dessus. Couvre la zone de contenu (mono-colonne) ; tableaux exclus.
    const pgD = pagesRef.current[idx]
    const docD = editorRef.current?.state.doc
    if (pgD && docD) {
      const padX = 4, padY = 2
      for (const para of pgD.layout.paragraphs) {
        if (para.table) continue
        const first = para.lines[0], last = para.lines[para.lines.length - 1]
        if (!first || !last) continue
        const node = docD.nodeAt(para.pmStart)
        const shading = node?.attrs?.shading as string | undefined
        const pBorder = node?.attrs?.paraBorder as ParaBorderDef | undefined
        if (!shading && !pBorder) continue
        const top = gg.marginV + first.y - padY
        const height = (last.y + last.height + padY) - first.y + padY
        const left = gg.marginH - padX, width = gg.contentW + padX * 2
        if (shading) {
          const prevOp = cx.globalCompositeOperation
          cx.globalCompositeOperation = 'destination-over'
          cx.fillStyle = shading
          cx.fillRect(left, top, width, height)
          cx.globalCompositeOperation = prevOp
        }
        if (pBorder && pBorder.width > 0) {
          cx.save()
          cx.strokeStyle = pBorder.color
          cx.lineWidth = pBorder.width
          if (pBorder.style === 'dashed') cx.setLineDash([pBorder.width * 3, pBorder.width * 2])
          else if (pBorder.style === 'dotted') { cx.setLineDash([1, pBorder.width * 2]); cx.lineCap = 'round' }
          cx.strokeRect(left, top, width, height)
          if (pBorder.style === 'double') cx.strokeRect(left + pBorder.width * 2, top + pBorder.width * 2, width - pBorder.width * 4, height - pBorder.width * 4)
          cx.restore()
        }
      }
      // Triangle Développer/Réduire (Word) dans la marge gauche, devant chaque titre.
      // ▶ = replié (gris foncé) ; ▼ = développé (gris clair).
      for (const para of pgD.layout.paragraphs) {
        if (para.table) continue
        const first = para.lines[0]; if (!first) continue
        const node = docD.nodeAt(para.pmStart)
        if (node?.type.name !== 'heading') continue
        const collapsed = !!node.attrs?.collapsed
        const cyT = gg.marginV + first.y + first.height / 2
        const tx = gg.marginH - 13
        cx.save()
        cx.fillStyle = collapsed ? '#5f6368' : '#9aa0a6'
        cx.beginPath()
        if (collapsed) { cx.moveTo(tx, cyT - 4); cx.lineTo(tx + 6, cyT); cx.lineTo(tx, cyT + 4) }
        else { cx.moveTo(tx - 1, cyT - 2); cx.lineTo(tx + 7, cyT - 2); cx.lineTo(tx + 3, cyT + 4) }
        cx.closePath(); cx.fill(); cx.restore()
      }
    }
    // Limites de la zone de texte (cadre pointillé) — aide visuelle, façon Word.
    if (boundRef.current) {
      cx.save()
      cx.strokeStyle = '#9aa0a6'
      cx.lineWidth = 0.5
      cx.setLineDash([2, 2])
      cx.strokeRect(gg.marginH, gg.marginV, gg.contentW, gg.pageH - gg.marginV - gg.marginBottom)
      cx.restore()
    }
    // Marques de paragraphe (¶) en fin de chaque paragraphe — façon « Afficher tout ».
    if (marksRef.current && pgD) {
      cx.save()
      cx.font = `${12 * (96 / 72)}px Arial, sans-serif`
      cx.fillStyle = 'rgba(26,115,232,0.55)'
      cx.textBaseline = 'alphabetic'
      cx.textAlign = 'left'
      for (const para of pgD.layout.paragraphs) {
        if (para.table) continue
        const last = para.lines[para.lines.length - 1]
        if (!last) continue
        const lastSpan = last.spans[last.spans.length - 1]
        const endX = lastSpan ? lastSpan.x + lastSpan.width : (last.caretX ?? 0)
        cx.fillText('¶', gg.marginH + endX + 1, gg.marginV + last.baseline)
      }
      cx.restore()
    }
    // Numéros de lignes (marge gauche) — façon Word. Compteur continu sur tout le
    // document ('continuous') ou redémarrant à chaque page ('page').
    const ln = lnRef.current
    const pg = pagesRef.current[idx]
    if (ln && pg && !skipFirst) {
      const step = Math.max(1, Math.round(ln.interval))
      const lnDoc = editorRef.current?.state.doc
      let n = 0
      if (ln.mode === 'continuous') for (let k = 0; k < idx; k++) n += countBodyLines(pagesRef.current[k], lnDoc)
      cx.save()
      cx.font = `${9 * (96 / 72)}px Arial, sans-serif`
      cx.fillStyle = '#9aa0a6'
      cx.textBaseline = 'alphabetic'
      cx.textAlign = 'right'
      const xx = Math.max(8, gg.marginH - 10)
      for (const para of pg.layout.paragraphs) {
        // Paragraphe « Supprimer les numéros de ligne » → ni compté, ni numéroté.
        if (lnDoc && lnDoc.nodeAt(para.pmStart)?.attrs?.suppressLineNumbers) continue
        for (const line of para.lines) {
          if (line.image || line.cellX != null) continue
          n++
          if (n % step === 0) cx.fillText(String(n), xx, gg.marginV + line.baseline)
        }
      }
      cx.restore()
    }
    // Bordure de page (cadre dans la marge) — au-dessus du contenu, façon Word.
    const pb = pbRef2.current
    if (pb) paintPageBorder(cx, gg, pb)
    // Filigrane — peint SOUS le contenu déjà rendu : `destination-over` le glisse
    // derrière le texte tout en restant au-dessus du fond (page blanche / couleur).
    const wm = wmRef.current
    if (wm && wm.text.trim()) {
      const prevOp = cx.globalCompositeOperation
      cx.globalCompositeOperation = 'destination-over'
      paintWatermark(cx, gg, wm)
      cx.globalCompositeOperation = prevOp
    }
  }, [i18nInst, effectiveHF])


  // ── Sections : repérage du break courant + édition d'orientation ────────────
  const breakPositions = (doc: import('@tiptap/pm/model').Node): number[] => {
    const ps: number[] = []
    doc.descendants((node, pos) => { if (node.type.name === 'sectionBreak') { ps.push(pos); return false } return true })
    return ps
  }
  // Index de section où se trouve le curseur (= nb de sectionBreak avant la tête).
  const currentSecIdx = (): number => {
    const ed = editorRef.current; if (!ed) return 0
    const head = ed.state.selection.head
    return breakPositions(ed.state.doc).filter(p => p < head).length
  }
  // Orientation de la section sec (0 = base ; sinon attr du (sec-1)e break).
  const orientationOfSec = (sec: number): Orientation => {
    const ed = editorRef.current
    if (!ed || sec === 0) return sectionRef.current.orientation
    const pos = breakPositions(ed.state.doc)[sec - 1]
    return (ed.state.doc.nodeAt(pos)?.attrs.orientation as Orientation) ?? 'portrait'
  }
  const reportActiveSection = () => cbRef.current.onActiveSection?.(orientationOfSec(currentSecIdx()))
  const lastStatsRef = useRef({ pages: 0, current: 0 })
  // Remonte les stats de pagination (barre de statut) : nombre total de pages +
  // page contenant le curseur (sémantique Word : « Page X sur Y » suit le caret).
  const reportStats = () => {
    const pgs = pagesRef.current
    const head = editorRef.current?.state.selection.head ?? 0
    const pages = Math.max(1, pgs.length), current = Math.max(1, pageIndexForHead(pgs, head) + 1)
    // Idempotent : ne notifie (→ setState parent) QUE si ça change, sinon chaque frappe
    // recrée un objet {pages,current} identique → re-rendu inutile (aggrave les rafales).
    if (lastStatsRef.current.pages === pages && lastStatsRef.current.current === current) return
    lastStatsRef.current = { pages, current }
    cbRef.current.onStats?.({ pages, current })
  }

  const setOrientation = useCallback((o: Orientation) => {
    const ed = editorRef.current; if (!ed) return
    const sec = currentSecIdx()
    if (sec === 0) { cbRef.current.onBaseChange?.({ orientation: o }); cbRef.current.onActiveSection?.(o); return }
    const pos = breakPositions(ed.state.doc)[sec - 1]
    if (pos == null) return
    ed.chain().command(({ tr }) => {
      const node = ed.state.doc.nodeAt(pos)
      if (!node) return false
      tr.setNodeMarkup(pos, undefined, { ...node.attrs, orientation: o })
      return true
    }).run()
    cbRef.current.onActiveSection?.(o)
  }, [])

  const insertBreak = useCallback(() => {
    const ed = editorRef.current; if (!ed) return
    // Le break continue l'orientation courante ; l'utilisateur la change ensuite
    // via Format → Portrait/Paysage (qui cible la section du curseur).
    const o = orientationOfSec(currentSecIdx())
    ed.chain().focus().insertContent([
      { type: 'sectionBreak', attrs: { orientation: o, top: 96, right: 96, bottom: 96, left: 96 } },
      { type: 'paragraph' },
    ]).run()
  }, [])

  const insertPageBreak = useCallback(() => {
    const ed = editorRef.current; if (!ed) return
    ed.chain().focus().insertContent([
      { type: 'pageBreak' },
      { type: 'paragraph' },
    ]).run()
  }, [])

  const setColumns = useCallback((n: number) => { cbRef.current.onBaseChange?.({ columns: n }) }, [])

  // Contexte d'édition en-tête/pied de la section du curseur (dialog Word).
  const hfContext = useCallback((kind: 'header' | 'footer') => {
    const sec = currentSecIdx()
    const m = secMetaRef.current[sec]
    const linked = sec === 0 ? true : (m?.hfLinked ?? true)
    return {
      secIdx: sec, linked,
      zones: sec > 0 && !linked ? (m?.[kind] ?? emptyHF()) : effectiveHF(sec, kind),
      firstPage: hfRef.current.first,
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [effectiveHF])

  // Applique l'en-tête/pied à la section du curseur. Retourne applyBase=true si
  // l'édition doit retomber sur l'en-tête/pied de BASE du document (section 0 ou
  // section liée au précédent — comportement Word).
  const setSectionHF = useCallback((kind: 'header' | 'footer', zones: HFContent, linked: boolean): { applyBase: boolean } => {
    const ed = editorRef.current; if (!ed) return { applyBase: true }
    const sec = currentSecIdx()
    if (sec === 0) return { applyBase: true }
    const pos = breakPositions(ed.state.doc)[sec - 1]
    if (pos == null) return { applyBase: true }
    ed.chain().command(({ tr }) => {
      const node = ed.state.doc.nodeAt(pos); if (!node) return false
      tr.setNodeMarkup(pos, undefined, { ...node.attrs, hfLinked: linked, [kind]: linked ? null : zones })
      return true
    }).run()
    return { applyBase: linked }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Fond de page de la section du curseur. false = section de base (fond global).
  const setSectionBg = useCallback((color: string | null): boolean => {
    const ed = editorRef.current; if (!ed) return false
    const sec = currentSecIdx()
    if (sec === 0) return false
    const pos = breakPositions(ed.state.doc)[sec - 1]
    if (pos == null) return false
    ed.chain().command(({ tr }) => {
      const node = ed.state.doc.nodeAt(pos); if (!node) return false
      tr.setNodeMarkup(pos, undefined, { ...node.attrs, pageColor: color })
      return true
    }).run()
    return true
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // ── Édition INLINE en-tête / pied (façon Word) ──────────────────────────────
  // Contexte de section pour la barre contextuelle (1ʳᵉ page diff. / lier).
  const hfBarCtx = useCallback((pageIdx: number, band: 'header' | 'footer') => {
    const sec = pagesRef.current[pageIdx]?.secIdx ?? 0
    const linked = sec === 0 ? true : (secMetaRef.current[sec]?.hfLinked ?? true)
    return { band, secIdx: sec, linked, canLink: sec > 0, firstPage: hfRef.current.first }
  }, [])
  const enterHFEdit = useCallback((band: 'header' | 'footer', pageIdx?: number) => {
    // Sans page précisée (menu) : ancrer sur la 1ʳᵉ page contenant le curseur, sinon 0.
    const idx = pageIdx ?? Math.max(0, pageIndexForHead(pagesRef.current, editorRef.current?.state.selection.head ?? 0))
    const sec = pagesRef.current[idx]?.secIdx ?? 0
    setHfEdit({ band, pageIdx: idx, initial: effectiveHF(sec, band) })
    cbRef.current.onHFActive?.(true, hfBarCtx(idx, band), hfZoneEditorRef.current)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hfBarCtx, effectiveHF])
  const exitHFEdit = useCallback(() => {
    setHfEdit(null)
    cbRef.current.onHFActive?.(false, null, null)
  }, [])
  // Bascule en-tête↔pied dans le même mode (bouton « Position »).
  const switchHFBand = useCallback(() => {
    setHfEdit(e => {
      if (!e) return e
      const band = e.band === 'header' ? 'footer' : 'header'
      const sec = pagesRef.current[e.pageIdx]?.secIdx ?? 0
      cbRef.current.onHFActive?.(true, hfBarCtx(e.pageIdx, band), hfZoneEditorRef.current)
      return { ...e, band, initial: effectiveHF(sec, band) }
    })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hfBarCtx, effectiveHF])
  // L'éditeur riche de la bande remonte son doc → persistance (commitHF).
  const onHFZoneChange = useCallback((doc: HFContent) => {
    const e = hfEditRef.current; if (!e) return
    cbRef.current.onCommitHF?.(e.band, doc)
  }, [])
  // Insère un champ dynamique ({page}…) dans l'éditeur de la bande au curseur.
  const insertHFField = useCallback((token: string) => {
    hfZoneEditorRef.current?.chain().focus().insertContent(token).run()
  }, [])

  // ── Zones de texte RICHES (édition in-place sur canvas via RichEditZone) ─────
  // Entrer en édition de la boîte à `pos` : la sélectionner (→ imgSel calcule son
  // rectangle écran), charger son doc (migration de l'ancien `kbtext:` si besoin).
  const enterTextBoxEdit = useCallback((pos: number) => {
    const ed = editorRef.current; if (!ed) return
    const node = ed.state.doc.nodeAt(pos)
    if (!node || node.type.name !== 'image') return
    const alt = node.attrs.alt as string | undefined
    let initial = parseTextBoxRichAlt(alt)
    if (!initial) {
      const legacy = parseTextBoxAlt(alt)
      initial = legacy != null ? textToHFDoc(legacy) : emptyHF()
      ed.view.dispatch(ed.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, alt: textBoxRichAlt(initial) }))
    }
    if (hfEditRef.current) exitHFEdit()
    ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, pos)))
    setTbEdit({ pos, initial })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [exitHFEdit])
  const exitTextBoxEdit = useCallback(() => {
    setTbEdit(null)
    cbRef.current.onTbActive?.(false, null)
    tbZoneEditorRef.current = null
    requestAnimationFrame(() => editorRef.current?.view.focus())
  }, [])
  // L'éditeur de la boîte remonte son doc → persisté dans l'attribut alt du nœud.
  const onTbZoneChange = useCallback((doc: HFContent) => {
    const e = tbEditRef.current, ed = editorRef.current
    if (!e || !ed) return
    const node = ed.state.doc.nodeAt(e.pos)
    if (!node || node.type.name !== 'image') return
    ed.view.dispatch(ed.state.tr.setNodeMarkup(e.pos, undefined, { ...node.attrs, alt: textBoxRichAlt(doc) }))
  }, [])
  // Auto-grandir la boîte pour contenir le texte (façon Word « ajuster à la forme »).
  // Grandit seulement (jamais de réduction surprise) : la frappe ne tronque jamais.
  const onTbHeight = useCallback((contentH: number) => {
    const e = tbEditRef.current, ed = editorRef.current
    if (!e || !ed) return
    const node = ed.state.doc.nodeAt(e.pos)
    if (!node || node.type.name !== 'image') return
    const want = Math.ceil(contentH) + 2 * RICH_TB_PAD
    const cur = (node.attrs.height as number) || 0
    if (want > cur + 1) ed.view.dispatch(ed.state.tr.setNodeMarkup(e.pos, undefined, { ...node.attrs, height: want }))
  }, [])
  // Insérer une nouvelle zone de texte vide puis entrer directement en édition.
  const insertTextBoxOp = useCallback(() => {
    const ed = editorRef.current; if (!ed) return
    if (hfEditRef.current) exitHFEdit()
    const w = 340, h = 140
    const doc = emptyHF()
    const at = ed.state.selection.from
    ed.chain().focus().insertContent([
      { type: 'image', attrs: { src: svgToDataUrl(richTextBoxFrameSvg(w, h)), width: w, height: h, align: 'left', alt: textBoxRichAlt(doc) } },
      { type: 'paragraph' },
    ]).run()
    requestAnimationFrame(() => {
      const e2 = editorRef.current; if (!e2) return
      let found = -1
      const lo = Math.max(0, at - 2), hi = Math.min(e2.state.doc.content.size, at + 4)
      e2.state.doc.nodesBetween(lo, hi, (n, p) => { if (found < 0 && n.type.name === 'image' && String(n.attrs.alt || '').startsWith('kbtextrich:')) found = p })
      if (found >= 0) enterTextBoxEdit(found)
    })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enterTextBoxEdit, exitHFEdit])

  const pageCount = useCallback(() => pagesRef.current.length, [])

  // Plan du document (volet de navigation + table des matières).
  const outline = useCallback(() => {
    const ed = editorRef.current; if (!ed) return []
    const items: Array<{ text: string; level: number; pos: number; page: number }> = []
    ed.state.doc.descendants((node, pos) => {
      if (node.type.name === 'heading') {
        items.push({
          text:  node.textContent || '…',
          level: (node.attrs.level as number) ?? 1,
          pos,
          page:  pageIndexForHead(pagesRef.current, pos + 1) + 1,
        })
        return false
      }
      return true
    })
    return items
  }, [])

  const scrollToPos = useCallback((pos: number) => {
    const ed = editorRef.current; if (!ed) return
    const p = Math.min(pos + 1, ed.state.doc.content.size)
    ed.chain().focus().setTextSelection(p).run()   // onSelectionUpdate → drawCaret(true) → scroll
  }, [])

  const pageBgRef = useRef(pageBg); pageBgRef.current = pageBg

  // Export : chaque page rendue sur un canvas hors écran (échelle ×n), avec fond
  // opaque (blanc / couleur de page) + décorations de marge — base de l'export PDF.
  const exportPageCanvases = useCallback((scale = 2) => {
    const out: Array<{ canvas: HTMLCanvasElement; wPx: number; hPx: number }> = []
    const pgs = pagesRef.current
    pgs.forEach((pg, idx) => {
      const gg = geomOf(pg)
      const cv = document.createElement('canvas')
      cv.width = Math.round(gg.pageW * scale)
      cv.height = Math.round(gg.pageH * scale)
      renderDocument(cv, pg.layout, gg.marginH, gg.marginV, scale, 1, undefined, false)
      const cx = cv.getContext('2d')!
      cx.save()
      cx.scale(scale, scale)
      drawPageDecorations(cx, gg, idx, pgs.length, pg.secIdx)
      cx.restore()
      // Fond opaque DESSOUS le contenu (destination-over) : couleur de page puis blanc.
      cx.globalCompositeOperation = 'destination-over'
      if (pageBgRef.current) {
        try { cx.fillStyle = pageBgRef.current; cx.fillRect(0, 0, cv.width, cv.height) } catch { /* gradient CSS → ignoré */ }
      }
      cx.fillStyle = '#ffffff'
      cx.fillRect(0, 0, cv.width, cv.height)
      cx.globalCompositeOperation = 'source-over'
      out.push({ canvas: cv, wPx: gg.pageW, hPx: gg.pageH })
    })
    return out
  }, [drawPageDecorations])

  useEffect(() => {
    // Ancre d'un commentaire : balaye le doc à la demande (autonome, pas de dépendance).
    const commentAnchor = (id: string): number | null => {
      const ed = editorRef.current; if (!ed) return null
      let found: number | null = null
      ed.state.doc.descendants((node, pos) => {
        if (found != null) return false
        if (node.isText && node.marks.some(m => m.type.name === 'comment' && m.attrs.commentId === id)) { found = pos; return false }
        return true
      })
      return found
    }
    const pageGeom = () => ({ pageW: gRef.current.pageW, pageH: gRef.current.pageH })
    const pageContentBox = (idx: number) => {
      const cv = canvasRefs.current.get(idx)
      return cv ? { left: cv.offsetLeft, top: cv.offsetTop, w: cv.offsetWidth, h: cv.offsetHeight } : null
    }
    onRegisterOps?.({ setOrientation, setColumns, insertBreak, insertPageBreak, pageCount, outline, scrollToPos, exportPageCanvases, hfContext, setSectionHF, setSectionBg, enterHF: (k) => enterHFEdit(k), exitHF: exitHFEdit, switchHF: switchHFBand, insertHFField, insertTextBox: insertTextBoxOp, editTextBox: enterTextBoxEdit, commentAnchor, pageGeom, pageContentBox })
    return () => onRegisterOps?.(null)
  }, [onRegisterOps, setOrientation, setColumns, insertBreak, insertPageBreak, pageCount, outline, scrollToPos, exportPageCanvases, hfContext, setSectionHF, setSectionBg, enterHFEdit, exitHFEdit, switchHFBand, insertHFField, insertTextBoxOp, enterTextBoxEdit])

  // Place le caret (curseur) sur la bonne page selon la position head de l'éditeur.
  // scrollIntoView=true (frappe/navigation) → amène le caret dans le champ de vision.
  const drawCaret = useCallback((scrollIntoView = false) => {
    const ed = editorRef.current, layout = contLayoutRef.current, caret = caretRef.current
    if (!ed || !layout || !caret) return
    // En édition en-tête/pied : le corps n'est pas édité → caret du corps masqué.
    if (hfEditRef.current) { caret.style.display = 'none'; return }
    // Comme un champ de saisie : pas de focus éditeur → pas de curseur visible.
    if (!ed.view.hasFocus()) { caret.style.display = 'none'; return }
    const sel = ed.state.selection
    if (sel.from !== sel.to) {
      caret.style.display = 'none'
      // Sélection (ex. résultat de recherche) → amener sa tête dans le champ de vision.
      if (scrollIntoView) {
        const c = posToCoords(layout, sel.head)
        const z = zoomRef.current
        const pgs = pagesRef.current
        let idx = 0
        for (let k = 0; k < pgs.length; k++) { if (c.y >= pgs[k].startY - 0.5) idx = k }
        const yAbs = pageOrigin(idx).top + (geomOf(pgs[idx]).marginV + (c.y - (pgs[idx]?.startY ?? 0))) * z
        const scEl = scrollContainerRef.current
        if (scEl) {
          const M = 80
          if (yAbs < scEl.scrollTop + M) scEl.scrollTop = Math.max(0, yAbs - M)
          else if (yAbs > scEl.scrollTop + scEl.clientHeight - M) scEl.scrollTop = yAbs - scEl.clientHeight + M
        }
      }
      return
    }
    const z = zoomRef.current
    const pgs = pagesRef.current
    // Localisation ROBUSTE (page + coords locales) — jamais de téléportation, même
    // aux frontières de page ou sur un layout transitoire (cf. caretLocation).
    const head = sel.head
    const { idx, cm } = caretLocation(pgs, layout, head, caretAtEndRef.current)
    const geom = geomOf(pgs[idx])
    // Origine de la page lue sur la position RÉELLE du canvas (suit colonne OU grille).
    const { left: leftOffset, top: pageTop } = pageOrigin(idx)
    // Hauteur du caret : par défaut celle de la ligne au curseur. MAIS si des
    // marques stockées (police/taille choisies sans sélection) sont actives, le
    // prochain caractère aura CETTE taille → on prévisualise sa hauteur tout de
    // suite (sinon le caret gardait l'ancienne taille jusqu'à la frappe).
    let caretH = cm.height
    const sm = editorRef.current?.state.storedMarks
    if (sm && sm.length) {
      const ts = sm.find(m => m.type.name === 'textStyle' && m.attrs.fontSize)
      if (ts) {
        const pt = parseFloat(String(ts.attrs.fontSize))
        if (!isNaN(pt)) caretH = pt * (96 / 72) * 1.2   // NATURAL_EM du moteur canvas
      }
    }
    // Cellule à texte vertical : caret = barre HORIZONTALE (perpendiculaire au flux),
    // longueur = hauteur de ligne, ancrée au bord supérieur local projeté.
    if (cm.rot) {
      const cl = leftOffset + (geom.marginH + cm.x) * z
      const ct = pageTop + (geom.marginV + cm.y) * z
      caret.style.display = 'block'
      caret.style.transform = 'none'; caret.style.transformOrigin = 'top left'
      caret.style.height = `${Math.max(1.5, 1.5 * z)}px`
      caret.style.width  = `${caretH * z}px`
      caret.style.top    = `${ct}px`
      caret.style.left   = `${cm.rot === 270 ? cl : cl - caretH * z}px`
      caret.style.animation = 'none'; void caret.offsetHeight; caret.style.animation = '_gdocs_blink 1s 0.5s infinite'
      if (scrollIntoView) { const sc = scrollContainerRef.current; if (sc) { const M = 48; if (ct < sc.scrollTop + M) sc.scrollTop = Math.max(0, ct - M); else if (ct > sc.scrollTop + sc.clientHeight - M) sc.scrollTop = ct - sc.clientHeight + M } }
      return
    }
    caret.style.display = 'block'
    caret.style.width  = '2px'   // restaure la largeur par défaut (la branche texte vertical la remplace par la longueur de ligne)
    caret.style.left   = `${leftOffset + (geom.marginH + cm.x) * z}px`
    caret.style.top    = `${pageTop + (geom.marginV + cm.y) * z}px`
    caret.style.height = `${caretH * z}px`
    // Curseur incliné sur du texte en italique (comme Google) : on penche la
    // barre du même angle que le glyphe (sommet vers la droite), pied ancré.
    // Sur une LIGNE VIDE, le glyphe n'existe pas → on regarde si l'italique est
    // actif via les marques stockées (choix toolbar) ou l'attribut fontMarks du
    // paragraphe → le caret s'incline quand même.
    let lean = cm.italicAngle || 0
    if (!lean) {
      const sm2 = editorRef.current?.state.storedMarks
      const italicActive = (sm2 && sm2.some(m => m.type.name === 'italic'))
        || !!(sel.$from.parent?.attrs?.fontMarks as { i?: boolean } | undefined)?.i
      if (italicActive) lean = 0.21   // ~12° comme l'italique synthétique du moteur
    }
    caret.style.transformOrigin = 'bottom'
    caret.style.transform = lean ? `skewX(${-(Math.atan(lean) * 180 / Math.PI)}deg)` : 'none'
    // Redémarrer le clignotement → solide juste après un déplacement, puis clignote (comme Google)
    caret.style.animation = 'none'
    void caret.offsetHeight
    caret.style.animation = '_gdocs_blink 1s 0.5s infinite'

    if (scrollIntoView) {
      const sc = scrollContainerRef.current
      if (sc) {
        // Position du caret en coordonnées CONTENU (mêmes que scrollTop), calculées à
        // partir du MODÈLE (cm, pageTop) qu'on vient de poser — STABLES. On NE relit PAS
        // `caret.getBoundingClientRect()` : pendant la re-pagination (setPages async), un
        // appel transitoire peut placer le caret à une mauvaise position ; relire le DOM
        // ferait défiler la page « pour suivre » ce caret fantôme (saut intempestif).
        const M = 48
        const caretTop = pageTop + (geom.marginV + cm.y) * z
        const caretBot = caretTop + caretH * z
        const ch = sc.clientHeight
        // Cible calculée UNE SEULE FOIS, sur le scrollTop COURANT (avant tout reflow) :
        //  - caret sous la vue → on l'amène en bas avec marge ; au-dessus → en haut ;
        //  - caret DÉJÀ visible → cible = scroll inchangé.
        // On RÉAFFIRME cette même cible en rAF SANS recalculer : pendant la re-pagination
        // (setPages), le navigateur réinitialise transitoirement scrollTop à 0 ; recalculer
        // alors croirait le caret « hors vue » et ferait sauter la page. Réaffirmer la cible
        // d'origine annule ce reset (caret visible → on remet exactement le scroll d'avant).
        const cur = sc.scrollTop
        const target = caretBot > cur + ch ? caretBot - ch + M
                     : caretTop < cur       ? Math.max(0, caretTop - M)
                     : cur
        sc.scrollTop = target
        requestAnimationFrame(() => { if (sc.scrollTop !== target) sc.scrollTop = target })
      }
    }
  }, [scrollContainerRef])

  // ── Curseurs distants (présence) ────────────────────────────────────────────
  // Projette une position PM absolue (head d'un participant) en coordonnées écran,
  // avec EXACTEMENT la même logique que `drawCaret` (page courante, offsets, zoom).
  const screenPosForHead = useCallback((head: number): { left: number; top: number; height: number } | null => {
    const layout = contLayoutRef.current
    const pgs = pagesRef.current
    if (!layout || pgs.length === 0) return null
    const z = zoomRef.current
    const { idx, cm } = caretLocation(pgs, layout, head)
    const pg = pgs[idx]; if (!pg) return null
    const geom = geomOf(pg)
    const { left: leftOffset, top: pageTop } = pageOrigin(idx)
    return {
      left:   leftOffset + (geom.marginH + cm.x) * z,
      top:    pageTop + (geom.marginV + cm.y) * z,
      height: cm.height * z,
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // Position ÉCRAN (viewport) du haut de la sélection du corps → mini-barre partagée
  // `FormattingMiniBar` (même composant que les zones de texte / en-têtes-pieds).
  // Masquée si pas de sélection de texte, ou en édition en-tête/pied / zone de texte
  // (qui ont leur propre mini-barre via RichEditZone).
  const recomputeBodyMiniBar = useCallback(() => {
    const ed = editorRef.current
    if (!ed || hfEditRef.current || tbEditRef.current) { setBodyMiniBar(null); return }
    const sel = ed.state.selection
    if (!(sel instanceof TextSelection) || sel.from >= sel.to) { setBodyMiniBar(null); return }
    const pgs = pagesRef.current
    const idx = pageIndexForHead(pgs, sel.from)
    const pg = pgs[idx]; const cv = canvasRefs.current.get(idx)
    if (!pg || !cv) { setBodyMiniBar(null); return }
    const z = zoomRef.current, geom = geomOf(pg)
    const cm = posToCoords(pg.layout, sel.from)
    const r = cv.getBoundingClientRect()
    setBodyMiniBar({ left: r.left + (geom.marginH + cm.x) * z, top: r.top + (geom.marginV + cm.y) * z })
  }, [])

  // Binding y-tiptap (mapping Yjs ↔ ProseMirror) pour convertir des positions.
  const bindingOf = (): ProsemirrorBinding | undefined => {
    const ed = editorRef.current
    if (!ed) return undefined
    return (ySyncPluginKey.getState(ed.state) as { binding?: ProsemirrorBinding } | undefined)?.binding
  }
  // Position PM absolue → position RELATIVE Yjs (JSON) : survit aux éditions concurrentes.
  const absToRelJson = (pos: number): unknown | null => {
    const b = bindingOf()
    if (!b) return null
    return Y.relativePositionToJSON(absolutePositionToRelativePosition(pos, b.type, b.mapping))
  }
  // Position relative Yjs (JSON) → position PM absolue (ou null si supprimée).
  const relJsonToAbs = (relJson: unknown): number | null => {
    const b = bindingOf()
    if (!b) return null
    const rel = Y.createRelativePositionFromJSON(relJson)
    return relativePositionToAbsolutePosition(ydoc, b.type, rel, b.mapping)
  }

  const recomputeRemoteCursors = useCallback(() => {
    const next: RemoteCursor[] = []
    awareness.getStates().forEach((state, clientId) => {
      if (clientId === awareness.clientID) return
      const s = state as { user?: { name: string; color: string }; cursor?: { head: unknown } }
      if (!s.user || !s.cursor || s.cursor.head == null) return
      const head = relJsonToAbs(s.cursor.head)
      if (head == null) return
      const pos = screenPosForHead(head)
      if (!pos) return
      next.push({ clientId, name: s.user.name, color: s.user.color, ...pos })
    })
    // Idempotent : renvoyer le MÊME tableau si rien n'a changé. Sinon `setRemoteCursors`
    // crée un nouveau tableau (même vide) à chaque appel → re-rendu ; or cette fonction est
    // appelée depuis un useEffect([pages,…]) → boucle de mises à jour → React #185 quand on
    // crée des pages en rafale (autorépétition). La comparaison casse la boucle.
    setRemoteCursors(prev => {
      if (prev.length === next.length && next.every((c, i) =>
        prev[i] && prev[i].clientId === c.clientId && prev[i].left === c.left &&
        prev[i].top === c.top && prev[i].height === c.height && prev[i].name === c.name && prev[i].color === c.color)) return prev
      return next
    })
  }, [awareness, screenPosForHead]) // eslint-disable-line react-hooks/exhaustive-deps

  // Présence : curseur souris (coords doc non-zoomées, relatives au contenu rootRef).
  const publishMouse = usePublishCursor(awareness, 'mouse')
  const onRootMouseMove = useCallback((e: React.MouseEvent) => {
    const root = rootRef.current
    if (!root) return
    const r = root.getBoundingClientRect()
    const z = zoomRef.current || 1
    publishMouse({ x: (e.clientX - r.left) / z, y: (e.clientY - r.top) / z })
  }, [publishMouse]) // eslint-disable-line react-hooks/exhaustive-deps

  // Met à jour la barre flottante d'image quand une image est sélectionnée (NodeSelection).
  const updateImgSel = useCallback(() => {
    const ed = editorRef.current, layout = contLayoutRef.current
    const sel = ed?.state.selection
    if (!ed || !layout || !(sel instanceof NodeSelection) || (sel.node.type.name !== 'image' && sel.node.type.name !== 'inlineImage')) {
      setImgSel(null); return
    }
    const zz = zoomRef.current, pgz = pagesRef.current
    // ── Image INLINE (atom dans le flux) : cadre calé sur son span-image ──────────
    if (sel.node.type.name === 'inlineImage') {
      let fLine: LayoutLine | null = null, fSpanX = 0, fSpanW = 0
      outer: for (const para of layout.paragraphs) for (const ln of para.lines) for (const sp of ln.spans) {
        if (sp.img && sp.pmPos === sel.from) { fLine = ln; fSpanX = sp.x; fSpanW = sp.width; break outer }
      }
      if (!fLine) { setImgSel(null); return }
      let i2 = 0
      for (let k = 0; k < pgz.length; k++) { if (fLine.y >= pgz[k].startY - 0.5) i2 = k }
      const g2 = geomOf(pgz[i2])
      const { left: lo2, top: pt2 } = pageOrigin(i2)
      const a = sel.node.attrs
      const w = Number(a.width) || 1, h = Number(a.height) || 1, rot = Number(a.rotation) || 0
      const rad = rot * Math.PI / 180
      const ah = Math.abs(w * Math.sin(rad)) + Math.abs(h * Math.cos(rad))   // hauteur boîte tournée
      const localBaseline = fLine.baseline - (pgz[i2]?.startY ?? 0)
      setImgSel({
        pos: sel.from,
        cx: lo2 + (g2.marginH + fSpanX + fSpanW / 2) * zz,
        cy: pt2 + (g2.marginV + localBaseline - ah / 2) * zz,
        w: w * zz, h: h * zz, rotation: rot, wrap: 'inline',
      })
      return
    }
    // Retrouver la ligne-image dans le layout continu pour ses dimensions d'affichage.
    let imgLine: LayoutLine | null = null
    for (const para of layout.paragraphs) for (const ln of para.lines) {
      if (ln.image && ln.pmStart === sel.from) { imgLine = ln; break }
    }
    if (!imgLine || !imgLine.image) { setImgSel(null); return }
    const z = zoomRef.current, pgs = pagesRef.current
    let idx = 0
    for (let k = 0; k < pgs.length; k++) { if (imgLine.y >= pgs[k].startY - 0.5) idx = k }
    const geom = geomOf(pgs[idx])
    const { left: leftOffset, top: pageTop } = pageOrigin(idx)
    const localY = imgLine.y - (pgs[idx]?.startY ?? 0)
    // Le centre de la boîte de sélection DOIT coïncider avec le centre de tracé du
    // canvas (cf. paintLayout) sinon, dès qu'on tourne la forme, boîte et forme se
    // désynchronisent. Canvas : flottant (behind/front/square) → centré sur
    // line.y + wrapY + image.h/2 ; sinon (inline/haut-bas) → centré sur la ligne,
    // line.y + line.height/2 (line.height = boîte englobante TOURNÉE = aabbH).
    const wrapName = imgLine.image.wrap || 'inline'
    const isFloat = wrapName === 'behind' || wrapName === 'front' || wrapName === 'square'
    const centerYLocal = isFloat
      ? localY + (imgLine.image.wrapY || 0) + imgLine.image.h / 2
      : localY + imgLine.height / 2
    setImgSel({
      pos: sel.from,
      cx: leftOffset + (geom.marginH + imgLine.image.x + imgLine.image.w / 2) * z,
      cy: pageTop + (geom.marginV + centerYLocal) * z,
      w: imgLine.image.w * z,
      h: imgLine.image.h * z,
      rotation: imgLine.image.rotation || 0,
      wrap: (sel.node.attrs.wrap as string) || 'inline',
    })
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const editor = useEditor({
    // Le rendu visuel passe par le CANVAS (recompute/drawCaret sur onUpdate/onSelectionUpdate),
    // jamais par le DOM de l'éditeur (caché). Le re-rendu React automatique de tiptap à CHAQUE
    // transaction est donc inutile ET nuisible : maintenir une touche (autorépétition) en
    // déclenchait un par frappe → cascade synchrone → React #185. Désactivé ; le ruban se
    // rafraîchit via `useEditorTick` (coalescé en rAF).
    shouldRerenderOnTransaction: false,
    immediatelyRender: true,
    // Le contenu provient du Y.Doc partagé (Collaboration) — PAS de `content:` ici
    // (sinon chaque client dupliquerait le contenu dans le doc Yjs). Le seed se fait
    // une seule fois, sur le 1er ouvreur d'une salle vide (effet plus bas).
    extensions: [...PAGE_EXTENSIONS, Collaboration.configure({ document: ydoc })],
    editorProps: {
      attributes: { class: 'focus:outline-none' },
      handleScrollToSelection: () => true,
      // Navigation verticale visuelle : l'éditeur caché (1px) ne sait pas calculer
      // ↑/↓ → on le fait via le layout canvas, avec une colonne cible conservée.
      handleKeyDown: (view, event) => {
        const layout = contLayoutRef.current
        if (!layout) return false
        const { state } = view
        const head = state.selection.head
        const mod  = event.ctrlKey || event.metaKey

        // Ctrl/⌘ + Entrée → saut de page (comme Google Docs).
        if (mod && event.key === 'Enter') {
          insertPageBreak()
          event.preventDefault()
          return true
        }

        // Backspace en DÉBUT de paragraphe avec un retrait → efface la « tabulation »
        // (retrait de 1ʳᵉ ligne d'abord, puis retrait gauche), au lieu de fusionner.
        if (event.key === 'Backspace' && state.selection.empty && !mod) {
          const $f = state.selection.$from
          if ($f.parentOffset === 0) {
            const para = $f.parent
            const fl = (para.attrs.indentFirstLine as number) || 0
            const il = (para.attrs.indentLeft as number) || 0
            if (fl > 0 || il > 0) {
              const STEP = 48
              const patch = fl > 0
                ? { indentFirstLine: Math.max(0, (Math.ceil(fl / STEP) - 1) * STEP) || null }
                : { indentLeft:      Math.max(0, (Math.ceil(il / STEP) - 1) * STEP) || null }
              view.dispatch(state.tr.setNodeMarkup($f.before($f.depth), undefined, { ...para.attrs, ...patch }))
              event.preventDefault()
              return true
            }
          }
        }

        // Tab / Maj+Tab dans un tableau → cellule suivante / précédente.
        if (event.key === 'Tab') {
          const $f = state.selection.$from
          let td = -1
          for (let d = $f.depth; d > 0; d--) if ($f.node(d).type.name === 'tableCell') { td = d; break }
          if (td >= 0) {
            const tableDepth = td - 2
            const tablePos = $f.before(tableDepth)
            const tableNode = $f.node(tableDepth)
            const cellAbs: number[] = []
            tableNode.descendants((node, pos) => { if (node.type.name === 'tableCell') { cellAbs.push(tablePos + 1 + pos); return false } return true })
            let cur = 0
            for (let i = 0; i < cellAbs.length; i++) if (cellAbs[i] < $f.pos) cur = i
            const target = cur + (event.shiftKey ? -1 : 1)
            event.preventDefault()
            if (target >= 0 && target < cellAbs.length) {
              const sel = TextSelection.near(state.doc.resolve(cellAbs[target] + 1), 1)
              view.dispatch(state.tr.setSelection(sel).scrollIntoView())
            }
            return true
          }
          // Dans une LISTE : laisser ProseMirror gérer (Tab = imbriquer l'élément).
          for (let d = $f.depth; d > 0; d--) if ($f.node(d).type.name === 'listItem') return false
          event.preventDefault()
          // Façon Word : Tab en DÉBUT de paragraphe = retrait de 1ʳᵉ LIGNE (décale UNIQUEMENT
          // la première ligne, cohérent avec le marqueur ▽ de la règle) ; ailleurs = vraie
          // tabulation (`\t`, avance au prochain taquet).
          const para = $f.parent
          const paraPos = $f.before($f.depth)
          const atStart = state.selection.empty && $f.parentOffset === 0
          const STEP = 48
          const cur = (para.attrs.indentFirstLine as number) || 0
          if (event.shiftKey) {
            if (atStart || cur > 0) {
              const next = Math.max(0, (Math.ceil(cur / STEP) - 1) * STEP)   // grille en dessous
              view.dispatch(state.tr.setNodeMarkup(paraPos, undefined, { ...para.attrs, indentFirstLine: next || null }))
            } else {
              const off = $f.parentOffset
              const prev = off > 0 ? para.textBetween(off - 1, off) : ''
              if (prev === '\t') view.dispatch(state.tr.delete(state.selection.from - 1, state.selection.from))
            }
          } else {
            if (atStart) {
              const next = Math.floor(cur / STEP + 1) * STEP
              view.dispatch(state.tr.setNodeMarkup(paraPos, undefined, { ...para.attrs, indentFirstLine: next }))
            } else {
              view.dispatch(state.tr.insertText('\t'))
            }
          }
          return true
        }

        // Home / End (+ Ctrl) → début/fin de ligne VISUELLE (ou du doc) via le layout
        // canvas, car l'éditeur caché 1px n'a pas de lignes visuelles. Shift étend.
        if (event.key === 'Home' || event.key === 'End') {
          goalXRef.current = null
          // L'affinité ACTUELLE (avant changement) identifie la ligne visuelle où est le
          // caret (sur une frontière d'enroulement). On la passe à lineStart/EndAt, PUIS on
          // pose la nouvelle affinité : Fin → bout de ligne ; Début → début de ligne.
          const wasAtEnd = caretAtEndRef.current
          caretAtEndRef.current = event.key === 'End' && !mod
          const newPos = event.key === 'Home'
            ? (mod ? docStart(layout) : lineStartAt(layout, head, wasAtEnd))
            : (mod ? docEnd(layout)   : lineEndAt(layout, head, wasAtEnd))
          const anchor = event.shiftKey ? state.selection.anchor : newPos
          view.dispatch(state.tr.setSelection(TextSelection.create(state.doc, anchor, newPos)))
          event.preventDefault()
          return true
        }

        // Page précédente / suivante : déplace le curseur d'environ une HAUTEUR DE VUE,
        // en conservant la colonne cible (goalX). L'éditeur caché 1px n'a pas de pages
        // visuelles → on calcule via le layout canvas (sinon PM faisait n'importe quoi).
        if (event.key === 'PageDown' || event.key === 'PageUp') {
          const isPgDown = event.key === 'PageDown'
          const cm = posToCoords(layout, head, caretAtEndRef.current)
          if (goalXRef.current == null) goalXRef.current = cm.x
          const vh = (scrollContainerRef.current?.clientHeight ?? 600) / (zoomRef.current || 1)
          const targetY = cm.y + (isPgDown ? vh : -vh)
          const newPos = coordsToPos(layout, goalXRef.current, targetY)
          caretAtEndRef.current = false
          if (newPos !== head) {
            const anchor = event.shiftKey ? state.selection.anchor : newPos
            goalXKeepRef.current = true   // déplacement vertical → garder la colonne cible
            view.dispatch(state.tr.setSelection(TextSelection.create(state.doc, anchor, newPos)))
          }
          event.preventDefault()
          return true
        }

        const isUp = event.key === 'ArrowUp', isDown = event.key === 'ArrowDown'
        if (!isUp && !isDown) {
          if (!['Shift', 'Control', 'Alt', 'Meta'].includes(event.key)) { goalXRef.current = null; caretAtEndRef.current = false }
          return false
        }
        // cm calculé avec l'AFFINITÉ courante : si le caret est au bout d'une ligne enroulée
        // (Fin), ↓ doit partir de CETTE ligne (sinon il sauterait une ligne). Après le
        // déplacement, l'affinité repasse à « début » (la nouvelle ligne se lit à gauche).
        const cm = posToCoords(layout, head, caretAtEndRef.current)
        if (goalXRef.current == null) goalXRef.current = cm.x
        const targetY = isUp ? cm.y - 2 : cm.y + cm.height + 2
        const newPos  = coordsToPos(layout, goalXRef.current, targetY)
        caretAtEndRef.current = false
        if (newPos === head) return true   // déjà sur la 1ʳᵉ/dernière ligne
        const anchor = event.shiftKey ? state.selection.anchor : newPos
        goalXKeepRef.current = true   // déplacement vertical → garder la colonne cible
        view.dispatch(state.tr.setSelection(TextSelection.create(state.doc, anchor, newPos)))
        event.preventDefault()
        return true
      },
    },
    onUpdate: ({ editor: ed, transaction: tr }) => {
      // Yjs Collaboration applies the initial document as a transaction DURING editor
      // construction → this fires before the const handlers declared below are
      // initialised (TDZ: "Cannot access X before initialization"). editorRef is only
      // assigned after useEditor() returns, so bail on that first construction-time call;
      // the post-mount effects (deps: [editor]) perform the initial layout/spell/comments.
      if (!editorRef.current) return
      recompute(ed as Editor)
      // Les soulignés du correcteur (spellRef) portent des positions PM. Le recalcul des
      // fautes est différé (350ms), mais le canvas est redessiné à CHAQUE frappe : sans
      // remappage, les positions périmées tombent sur du texte décalé → les vaguelettes
      // des mots SUIVANTS dérivent/zigzaguent à chaque caractère. On remappe les positions
      // à travers la transaction pour qu'elles restent collées à leur mot jusqu'au recalcul.
      if (tr.docChanged && spellRef.current.length) {
        spellRef.current = spellRef.current
          .map(i => ({ ...i, from: tr.mapping.map(i.from, -1), to: tr.mapping.map(i.to, 1) }))
          .filter(i => i.to > i.from)
      }
      computeCommentRanges()
      // La sélection de cellules (coordonnées de grille) reste valide tant que la
      // STRUCTURE ne change pas : on ne l'efface que si la taille du doc change
      // (frappe / insertion / suppression / fusion). Une simple mise en forme
      // (marques, attributs de paragraphe) garde la même taille → sélection conservée
      // pour pouvoir enchaîner gras + couleur + alignement, etc.
      if (tableSelRef.current && tr && tr.before.content.size !== tr.doc.content.size) setTableSel(null)
      // Le contenu a changé → les positions des curseurs distants se décalent.
      recomputeRemoteCursors()
      // Une image/zone-de-texte sélectionnée a pu changer de dimensions (ex. zone de
      // texte qui auto-grandit) → recaler la barre/le cadre d'édition sur le rect.
      updateImgSel()
      recomputeBodyMiniBar()
      reportStats()
      // Correcteur : recalcule les fautes (débit léger) puis redessine les soulignés.
      clearTimeout(spellTimer.current)
      spellTimer.current = setTimeout(() => { computeSpell(); renderAllPages() }, 350)
      clearTimeout(saveTimer.current)
      saveTimer.current = setTimeout(() => onSave((ed as Editor).getJSON()), 1200)
    },
    onSelectionUpdate: ({ editor: ed }) => {
      // See onUpdate: skip the construction-time fire (handlers below not yet initialised).
      if (!editorRef.current) return
      // ── Colonne cible (goal column) ───────────────────────────────────────────
      // Source UNIQUE de vérité du reset : on ne PRÉSERVE la colonne cible que pour une
      // MAJ de sélection issue d'un déplacement VERTICAL (↑/↓/Page, qui a posé le drapeau).
      // TOUTE autre MAJ de sélection — frappe, édition de texte, clic souris, collage (même
      // par menu contextuel), flèches gauche/droite, recherche, action de barre d'outils… —
      // la réinitialise ; la prochaine abscisse deviendra alors la nouvelle colonne cible.
      if (goalXKeepRef.current) goalXKeepRef.current = false
      else goalXRef.current = null
      // Layout frais AVANT de dessiner (no-op si déjà calculé pour ce doc).
      recompute(ed as Editor)
      computeCommentRanges()
      renderAllPages(); drawCaret(true); reportActiveSection(); updateImgSel(); recomputeBodyMiniBar(); reportStats()
      // Publier notre curseur en position RELATIVE Yjs (robuste aux éditions concurrentes).
      const sel = (ed as Editor).state.selection
      const head = absToRelJson(sel.head)
      if (head != null) awareness.setLocalStateField('cursor', { head, anchor: absToRelJson(sel.anchor) })
    },
  })
  editorRef.current = editor as Editor | null

  // `onEditor` est souvent une arrow INLINE côté parent → identité changeante à CHAQUE
  // rendu. En la mettant dans les deps, l'effet se ré-exécutait à chaque rendu du parent :
  // cleanup `onEditor(null)` (→ setActiveEditor(null)) puis `onEditor(editor)` → oscillation
  // d'état → sous une rafale de frappes (parent re-rendu vite) la cascade dépassait la
  // profondeur de mise à jour de React (#185). On passe par une ref → effet lié au SEUL
  // `editor` (stable), tout en appelant toujours la dernière version de `onEditor`.
  const onEditorRef = useRef(onEditor); onEditorRef.current = onEditor
  useEffect(() => { onEditorRef.current(editor as Editor | null); return () => onEditorRef.current(null) }, [editor])

  // Marques stockées modifiées sans changer la sélection (ex. choix d'une taille
  // de police sans sélection) → ni onUpdate ni onSelectionUpdate ne se déclenchent.
  // On redessine le caret pour qu'il prenne tout de suite la nouvelle hauteur.
  useEffect(() => {
    const ed = editor as Editor | null; if (!ed) return
    let prev = ed.state.storedMarks
    const onTr = () => {
      const cur = ed.state.storedMarks
      if (cur !== prev) { prev = cur; drawCaret() }
    }
    ed.on('transaction', onTr)
    return () => { ed.off('transaction', onTr) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor])

  // Focus/blur de l'éditeur : comme un champ de saisie, le curseur n'est visible que
  // quand l'éditeur a le focus. Au blur on le masque, au focus on le redessine.
  useEffect(() => {
    const ed = editor as Editor | null; if (!ed) return
    const onFocus = () => drawCaret()
    const onBlur  = () => { if (caretRef.current) caretRef.current.style.display = 'none' }
    ed.on('focus', onFocus)
    ed.on('blur', onBlur)
    return () => { ed.off('focus', onFocus); ed.off('blur', onBlur) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor])

  // Seed : si la salle Yjs est VIDE (1er ouvreur), initialiser le doc partagé
  // depuis le contenu JSON existant (.kbdoc). Une seule fois ; les ouvreurs
  // suivants reçoivent le contenu via Yjs (collabEmpty === false → pas de seed).
  const seededRef = useRef(false)
  useEffect(() => {
    if (seededRef.current || collabEmpty !== true || !editor) return
    seededRef.current = true
    editor.commands.setContent(initialDoc)
  }, [collabEmpty, editor]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (editor) recompute(editor as Editor, true) }, [editor, recompute])
  useEffect(() => { if (editor) recompute(editor as Editor, true) }, [zoom, g.contentW, g.contentH]) // eslint-disable-line react-hooks/exhaustive-deps
  // Le rectangle de sélection d'objet est positionné en px × zoom (cf. updateImgSel) :
  // au changement de zoom il faut le recalculer APRÈS le relayout des pages (offsets
  // canvas à jour) — sinon la boîte reste à l'échelle précédente et « décroche ».
  useEffect(() => { const r = requestAnimationFrame(() => updateImgSel()); return () => cancelAnimationFrame(r) }, [zoom, pages]) // eslint-disable-line react-hooks/exhaustive-deps
  // Barre de statut : reporter le total de pages au montage / re-pagination.
  useEffect(() => { reportStats() }, [pages]) // eslint-disable-line react-hooks/exhaustive-deps

  // Présence : recalcule les curseurs distants quand l'awareness change (un pair
  // bouge/rejoint/part) ou quand la mise en page change (pagination, zoom).
  useEffect(() => {
    awareness.on('change', recomputeRemoteCursors)
    return () => awareness.off('change', recomputeRemoteCursors)
  }, [awareness, recomputeRemoteCursors])
  useEffect(() => { recomputeRemoteCursors() }, [pages, zoom, recomputeRemoteCursors])

  // Redessine TOUS les canvas avec la sélection courante (appelé sur changement
  // de pages/zoom ET sur changement de sélection → le surlignage s'affiche).
  // ── Correcteur orthographe/grammaire ────────────────────────────────────────
  const spellRef = useRef<SpellIssue[]>([])
  const spellCheckRef = useRef(spellCheck); spellCheckRef.current = spellCheck
  const computeSpell = useCallback(() => {
    const ed = editorRef.current
    if (!ed || !spellCheckRef.current) { if (spellRef.current.length) { spellRef.current = []; onSpellCount?.(0) } return }
    const issues: SpellIssue[] = []
    ed.state.doc.descendants((node, pos) => {
      if (node.isText && node.text) issues.push(...findIssues(node.text, pos))
      return true
    })
    spellRef.current = issues
    onSpellCount?.(issues.length)
  }, [onSpellCount])

  // Plages de commentaires : balaye le doc pour les marques `comment`, regroupe par
  // id (min/max). Sert à peindre la surbrillance et au clic vers le fil.
  const computeCommentRanges = useCallback(() => {
    const ed = editorRef.current
    if (!ed) return
    const map = new Map<string, { from: number; to: number }>()
    ed.state.doc.descendants((node, pos) => {
      if (node.isText) {
        const m = node.marks.find(mk => mk.type.name === 'comment' && mk.attrs.commentId)
        if (m) {
          const id = String(m.attrs.commentId), from = pos, to = pos + node.nodeSize
          const e = map.get(id)
          if (e) { e.from = Math.min(e.from, from); e.to = Math.max(e.to, to) }
          else map.set(id, { from, to })
        }
      }
      return true
    })
    const arr = [...map.entries()].map(([id, r]) => ({ id, ...r }))
    const prev = commentRangesRef.current
    commentRangesRef.current = arr
    // Remonte la liste d'ids quand elle change (le volet retire les fils orphelins).
    if (prev.length !== arr.length || arr.some((a, i) => a.id !== prev[i]?.id)) {
      cbRef.current.onCommentRanges?.(arr.map(a => a.id))
    }
  }, [])

  const renderAllPages = useCallback(() => {
    const ed = editorRef.current
    if (!ed) return
    // Supersampling : on rend la grille de pixels du canvas à au moins 2× la taille
    // CSS (RENDER_FLOOR), même sur écran non-HiDPI. Le navigateur ré-échantillonne en
    // descente → bords de glyphes plus nets et lissage plus doux, approchant le rendu
    // « ClearType » du texte natif (le canvas 2D ne fournit pas l'anticrénage sous-pixel).
    const dpr = Math.max(2, window.devicePixelRatio || 1)
    const sel = ed.state.selection
    const range = sel.from < sel.to ? { from: sel.from, to: sel.to } : undefined
    const spell = spellCheckRef.current && spellRef.current.length
      ? spellRef.current.map(i => ({ from: i.from, to: i.to, grammar: i.type === 'grammar' })) : undefined
    // Surbrillances : commentaires (jaune doux, plus fort si actif) + occurrences de
    // recherche (jaune, orange pour l'occurrence courante).
    const highlights: Array<{ from: number; to: number; color: string }> = []
    for (const c of commentRangesRef.current)
      highlights.push({ from: c.from, to: c.to, color: c.id === activeCommentRef.current ? 'rgba(255,167,38,0.45)' : 'rgba(255,213,79,0.32)' })
    const sr = searchRef.current
    // L'occurrence courante est déjà montrée par la sélection bleue → on ne surligne
    // que les AUTRES en jaune (évite un double calque sur l'occurrence active).
    sr.ranges.forEach((m, i) => { if (i !== sr.active) highlights.push({ from: m.from, to: m.to, color: 'rgba(255,235,59,0.5)' }) })
    // L'éditeur reçoit la saisie au niveau fenêtre (le contenteditable ne garde pas
    // le focus DOM). « Focus éditeur » = fenêtre focus ET aucun autre champ de saisie
    // actif (input/textarea/select ou contenteditable étranger).
    const a = document.activeElement as HTMLElement | null
    const otherField = !!a && a !== document.body &&
      (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.tagName === 'SELECT' ||
       (a.isContentEditable && !a.closest('.ProseMirror')))
    const focused = document.hasFocus() && !otherField
    const z = zoomRef.current
    pagesRef.current.forEach((pg, idx) => {
      const cv = canvasRefs.current.get(idx)
      if (!cv) return
      const gg = geomOf(pg)
      const bw = Math.round(gg.pageW * z * dpr), bh = Math.round(gg.pageH * z * dpr)
      if (cv.width !== bw) cv.width = bw
      if (cv.height !== bh) cv.height = bh
      cv.style.width = `${gg.pageW * z}px`
      cv.style.height = `${gg.pageH * z}px`
      renderDocument(cv, pg.layout, gg.marginH, gg.marginV, dpr, z, range, focused, spell, highlights.length ? highlights : undefined)

      // Décorations de marge : en-tête / pied (3 zones, champs dynamiques) + numéro.
      const cx = cv.getContext('2d')
      if (cx) {
        cx.save()
        cx.scale(dpr * z, dpr * z)
        // En édition inline, la bande active de la page d'ancrage est affichée par
        // l'input → on ne la dessine PAS sur le canvas (sinon texte en doublon).
        const hfe = hfEditRef.current
        const skipBand = hfe && hfe.pageIdx === idx ? hfe.band : undefined
        // À l'écran (hors export PDF), les bandes non éditées sont estompées (Word).
        drawPageDecorations(cx, gg, idx, pagesRef.current.length, pg.secIdx, skipBand, true)
        cx.restore()
      }
    })
  }, [drawPageDecorations])

  // ── Dialogue « Paragraphe… » (clic droit → Paragraphe…) ──────────────────────
  const [paraDlg, setParaDlg] = useState<ParaDraft | null>(null)
  const openParagraphDialog = useCallback(() => {
    const ed = editorRef.current; if (!ed) return
    const a = { ...ed.getAttributes('paragraph'), ...ed.getAttributes('heading') } as Record<string, unknown>
    setParaDlg(paraDraftFromAttrs(a))
  }, [])
  const applyParagraphDraft = useCallback((d: ParaDraft) => {
    const ed = editorRef.current; if (!ed) return
    const { from, to } = ed.state.selection
    applyParaAcross(ed, [{ from, to }], paraAttrsFromDraft(d))
    requestAnimationFrame(() => { computeSpell(); renderAllPages() })
  }, [computeSpell, renderAllPages])

  // Entrée/sortie d'édition en-tête/pied : redessiner (masque/affiche le caret du
  // corps + applique/retire le skipBand sur le canvas pour éviter le doublon).
  useEffect(() => { renderAllPages(); drawCaret(); recomputeBodyMiniBar() }, [hfEdit, renderAllPages, drawCaret, recomputeBodyMiniBar])
  useEffect(() => { recomputeBodyMiniBar() }, [tbEdit, recomputeBodyMiniBar])
  // Correcteur : charge les dictionnaires Hunspell à l'activation (paresseux) ; recalcule
  // quand ils sont prêts, puis à chaque (dés)activation/montage, et redessine.
  useEffect(() => {
    if (spellCheck) { loadSpeller(); onSpellerReady(() => { computeSpell(); renderAllPages() }) }
    computeSpell(); renderAllPages()
  }, [spellCheck, computeSpell, renderAllPages])
  // Re-vérification forcée (dictionnaire personnel modifié depuis le ruban Révision).
  useEffect(() => { computeSpell(); renderAllPages() }, [spellVersion]) // eslint-disable-line react-hooks/exhaustive-deps

  // Ferme le panneau d'options de disposition quand l'objet est désélectionné.
  useEffect(() => { if (!imgSel) setWrapPanel(false) }, [imgSel])

  // Re-rendu quand les surbrillances (recherche / commentaire actif) changent.
  useEffect(() => { renderAllPages() }, [searchRanges, searchActive, activeCommentId, renderAllPages])
  // Re-rendu quand le filigrane / la bordure / les numéros de lignes / les aides
  // visuelles (limites, marques ¶) / le format des numéros de page changent.
  useEffect(() => { renderAllPages() }, [watermark, pageBorder, lineNumbers, showBoundaries, showMarks, pageNumFormat, pageNumStart, renderAllPages])
  // Remonte la sélection de cellules de tableau (pour les actions du ruban).
  useEffect(() => { cbRef.current.onTableSel?.(tableSel) }, [tableSel])
  // Recalcul des plages de commentaires à l'arrivée de l'éditeur / changement de pages.
  useEffect(() => { computeCommentRanges(); renderAllPages() }, [editor, pages, computeCommentRanges, renderAllPages])

  // Couleur de la sélection selon le focus : redessine sur changement de focus
  // (fenêtre ou champ de saisie) — #ABC2FE focus, #D9D9D9 sinon.
  useEffect(() => {
    const rerender = () => renderAllPages()
    window.addEventListener('focus', rerender)
    window.addEventListener('blur', rerender)
    document.addEventListener('focusin', rerender)
    document.addEventListener('focusout', rerender)
    return () => {
      window.removeEventListener('focus', rerender)
      window.removeEventListener('blur', rerender)
      document.removeEventListener('focusin', rerender)
      document.removeEventListener('focusout', rerender)
    }
  }, [renderAllPages])

  // Redessine quand l'en-tête / le pied de page (zones, 1ʳᵉ page, titre) change.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { renderAllPages() }, [JSON.stringify(header), JSON.stringify(footer), hfFirstPage, docTitle, renderAllPages])

  // Redessine quand la config des numéros de page change.
  useEffect(() => { renderAllPages() }, [pageNumbers, renderAllPages])

  // Rendu des canvas après mise à jour des pages / zoom.
  useLayoutEffect(() => {
    if (!editor) return
    renderAllPages()
    drawCaret()
    recomputeBodyMiniBar()
  }, [pages, zoom, editor, drawCaret, renderAllPages, recomputeBodyMiniBar])

  // Repositionner/masquer la mini-barre du corps quand on défile (position fixe).
  useEffect(() => {
    const sc = scrollContainerRef.current
    if (!sc) return
    const onScroll = () => recomputeBodyMiniBar()
    sc.addEventListener('scroll', onScroll, { passive: true })
    return () => sc.removeEventListener('scroll', onScroll)
  }, [scrollContainerRef, recomputeBodyMiniBar])

  // Quand une police (perso, ex. Bookerly) finit de charger de façon asynchrone,
  // les mesures faites avec la police de repli sont fausses → on recalcule le
  // layout et on redessine, sinon le canvas reste sur la police de repli.
  useEffect(() => {
    if (typeof document === 'undefined' || !document.fonts) return
    const onFonts = () => {
      const ed = editorRef.current
      if (ed) recompute(ed, true)   // métriques changées (police/image) → relayout forcé
      renderAllPages()
      drawCaret()
    }
    document.fonts.addEventListener?.('loadingdone', onFonts)
    window.addEventListener('kubuno-font-loaded', onFonts)
    // Idem quand une image finit de charger (taille naturelle alors connue).
    window.addEventListener('kubuno-image-loaded', onFonts)
    document.fonts.ready.then(onFonts).catch(() => {})
    return () => {
      document.fonts.removeEventListener?.('loadingdone', onFonts)
      window.removeEventListener('kubuno-font-loaded', onFonts)
      window.removeEventListener('kubuno-image-loaded', onFonts)
    }
  }, [recompute, renderAllPages, drawCaret])

  // ── Souris : clic place le curseur, glisser sélectionne (inter-pages natif) ──
  const posFromEvent = useCallback((pageIdx: number, clientX: number, clientY: number): number | null => {
    const cv = canvasRefs.current.get(pageIdx)
    const pg = pagesRef.current[pageIdx]
    if (!cv || !pg) return null
    const r = cv.getBoundingClientRect()
    const z = zoomRef.current, gg = geomOf(pg)
    const x = (clientX - r.left) / z - gg.marginH
    const y = (clientY - r.top)  / z - gg.marginV
    return coordsToPos(pg.layout, x, y)
  }, [])

  // Page sous (ou la plus proche de) un point écran — hit-test 2D (X ET Y). Indispensable
  // en disposition GRILLE : plusieurs pages partagent le même Y → un test par Y seul
  // choisirait la mauvaise → la sélection au drag s'étendrait sur toutes les pages d'une rangée.
  const pageAtPoint = useCallback((clientX: number, clientY: number, fallback: number): number => {
    let best = fallback, bestD = Infinity
    canvasRefs.current.forEach((cv, k) => {
      const r = cv.getBoundingClientRect()
      const dx = clientX < r.left ? r.left - clientX : clientX > r.right ? clientX - r.right : 0
      const dy = clientY < r.top  ? r.top  - clientY : clientY > r.bottom ? clientY - r.bottom : 0
      const d = dx * dx + dy * dy
      if (d < bestD) { bestD = d; best = k }
    })
    return best
  }, [])

  // Hit-test d'une cellule de tableau sous le pointeur → { tableStart (pmStart du
  // tableau), r, c, colspan, rowspan } en coordonnées de grille. Sert à la sélection.
  const hitTableCell = useCallback((pageIdx: number, clientX: number, clientY: number):
    { tableStart: number; r: number; c: number; colspan: number; rowspan: number } | null => {
    const cv = canvasRefs.current.get(pageIdx)
    const pg = pagesRef.current[pageIdx]
    if (!cv || !pg) return null
    const r = cv.getBoundingClientRect()
    const z = zoomRef.current, gg = geomOf(pg)
    const x = (clientX - r.left) / z - gg.marginH
    const y = (clientY - r.top)  / z - gg.marginV
    for (const para of pg.layout.paragraphs) {
      if (!para.table) continue
      for (const cell of para.table.cells) {
        if (x >= cell.x && x <= cell.x + cell.w && y >= cell.y && y <= cell.y + cell.h) {
          return { tableStart: para.pmStart, r: cell.r, c: cell.c, colspan: cell.colspan, rowspan: cell.rowspan }
        }
      }
    }
    return null
  }, [])

  // Triangle Développer/Réduire (marge gauche, devant un titre) sous le pointeur →
  // position du nœud titre à basculer, sinon null.
  const hitHeadingTriangle = useCallback((pageIdx: number, clientX: number, clientY: number): number | null => {
    const cv = canvasRefs.current.get(pageIdx)
    const pg = pagesRef.current[pageIdx]
    const ed = editorRef.current
    if (!cv || !pg || !ed) return null
    const r = cv.getBoundingClientRect()
    const z = zoomRef.current, gg = geomOf(pg)
    const x = (clientX - r.left) / z - gg.marginH
    const y = (clientY - r.top) / z - gg.marginV
    if (x < -22 || x > 2) return null   // marge gauche uniquement
    for (const para of pg.layout.paragraphs) {
      if (para.table) continue
      const first = para.lines[0]; if (!first) continue
      if (y < first.y - 3 || y > first.y + first.height + 3) continue
      const node = ed.state.doc.nodeAt(para.pmStart)
      if (node?.type.name === 'heading') return para.pmStart
    }
    return null
  }, [])

  // Image FLOTTANTE (devant le texte / habillage carré) sous le point écran → sa
  // position PM. Le hit-test texte (`posFromEvent`) tomberait sur le texte derrière ;
  // ces objets ne sont pas dans le flux, on les teste donc géométriquement.
  const floatingImageAt = useCallback((clientX: number, clientY: number): number | null => {
    const root = rootRef.current, layout = contLayoutRef.current
    if (!root || !layout) return null
    const rect = root.getBoundingClientRect()
    const z = zoomRef.current, pgs = pagesRef.current
    const px = clientX - rect.left, py = clientY - rect.top
    // 'front' = couche du dessus (priorité) ; 'square' = à côté du texte (non ambigu).
    for (const want of ['front', 'square'] as const) {
      for (const para of layout.paragraphs) for (const ln of para.lines) {
        const im = ln.image
        if (!im || (im.wrap || 'inline') !== want) continue
        let idx = 0
        for (let k = 0; k < pgs.length; k++) if (ln.y >= pgs[k].startY - 0.5) idx = k
        const geom = geomOf(pgs[idx]); const o = pageOrigin(idx)
        const localY = ln.y - (pgs[idx]?.startY ?? 0)
        const cyLocal = localY + (im.wrapY || 0) + im.h / 2
        const cx = o.left + (geom.marginH + im.x + im.w / 2) * z
        const cy = o.top + (geom.marginV + cyLocal) * z
        const hw = (im.w * z) / 2, hh = (im.h * z) / 2
        let lx = px - cx, ly = py - cy
        const rot = ((im.rotation || 0) * Math.PI) / 180
        if (rot) { const c = Math.cos(-rot), s = Math.sin(-rot); const nx = lx * c - ly * s, ny = lx * s + ly * c; lx = nx; ly = ny }
        if (Math.abs(lx) <= hw && Math.abs(ly) <= hh) return ln.pmStart
      }
    }
    return null
  }, [])

  const onPageMouseDown = useCallback((pageIdx: number, e: React.MouseEvent) => {
    const ed = editorRef.current; if (!ed) return

    // Clic DROIT : ne pas déplacer le curseur ni collapser la sélection — le menu
    // contextuel (onPageContextMenu) gère le placement et préserve la sélection.
    if (e.button === 2) return
    // Clic sur le triangle Développer/Réduire d'un titre (marge gauche) → bascule.
    if (e.button === 0) {
      const hpos = hitHeadingTriangle(pageIdx, e.clientX, e.clientY)
      if (hpos != null) {
        e.preventDefault()
        const node = ed.state.doc.nodeAt(hpos)
        if (node?.type.name === 'heading') setHeadingCollapsed(ed, hpos, !node.attrs.collapsed)
        requestAnimationFrame(() => renderAllPages())
        return
      }
    }
    // Un clic réinitialise l'affinité « fin de ligne » (sinon héritée d'un End précédent).
    caretAtEndRef.current = false
    goalXRef.current = null

    // Double-clic dans la marge haute/basse → édition INLINE de l'en-tête / du pied
    // (comme Word : on tape directement dans la zone, pas de formulaire).
    if (e.detail === 2) {
      const cv = canvasRefs.current.get(pageIdx)
      const pg = pagesRef.current[pageIdx]
      if (cv && pg) {
        const gg = geomOf(pg)
        const r  = cv.getBoundingClientRect()
        const y  = (e.clientY - r.top) / zoomRef.current
        if (y < gg.marginV * 0.85)                    { e.preventDefault(); enterHFEdit('header', pageIdx); return }
        if (y > gg.pageH - gg.marginBottom * 0.85)    { e.preventDefault(); enterHFEdit('footer', pageIdx); return }
      }
    }

    // En mode édition en-tête/pied : un clic DANS le corps en sort (comme Word).
    if (hfEditRef.current) {
      const cv = canvasRefs.current.get(pageIdx)
      const pg = pagesRef.current[pageIdx]
      if (cv && pg) {
        const gg = geomOf(pg)
        const y2 = (e.clientY - cv.getBoundingClientRect().top) / zoomRef.current
        if (y2 > gg.marginV && y2 < gg.pageH - gg.marginBottom) exitHFEdit()
      }
    }

    // En édition in-place d'une zone de texte : tout clic atteignant le canvas du
    // corps est, par construction, HORS de la boîte (overlay au-dessus) → on sort.
    if (tbEditRef.current) exitTextBoxEdit()

    // Clic sur une forme/image FLOTTANTE (devant le texte / carré) → la sélectionner
    // (sinon le hit-test texte sélectionnerait le texte derrière).
    if (e.button === 0) {
      const fpos = floatingImageAt(e.clientX, e.clientY)
      if (fpos != null) {
        e.preventDefault()
        const n = ed.state.doc.nodeAt(fpos)
        if (e.detail >= 2 && n && parseTextBoxRichAlt(n.attrs.alt as string)) { enterTextBoxEdit(fpos); return }
        ed.view.focus()
        ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, fpos)))
        return
      }
    }

    const pos = posFromEvent(pageIdx, e.clientX, e.clientY)
    if (pos == null) return
    e.preventDefault()

    // Clic dans un texte commenté → active le fil correspondant (volet commentaires).
    if (e.button === 0) {
      const c = commentRangesRef.current.find(cr => pos >= cr.from && pos <= cr.to)
      cbRef.current.onCommentActivate?.(c ? c.id : null)
    }

    // Tout clic gauche réinitialise la sélection de cellules ; un glisser dans un
    // tableau la (re)construit.
    if (e.button === 0) setTableSel(null)

    // ── Sélection de plage de cellules d'un tableau (glisser) ───────────────────
    const startCell = e.button === 0 && e.detail < 2 ? hitTableCell(pageIdx, e.clientX, e.clientY) : null
    if (startCell) {
      const anchor = startCell
      ed.chain().focus().setTextSelection(pos).run()   // curseur dans la cellule (édition possible)
      const rectOf = (h: { r: number; c: number; colspan: number; rowspan: number }): TableRect => ({
        r0: Math.min(anchor.r, h.r), c0: Math.min(anchor.c, h.c),
        r1: Math.max(anchor.r + anchor.rowspan - 1, h.r + h.rowspan - 1),
        c1: Math.max(anchor.c + anchor.colspan - 1, h.c + h.colspan - 1),
      })
      const onMove = (me: MouseEvent) => {
        const h = hitTableCell(pageAtPoint(me.clientX, me.clientY, pageIdx), me.clientX, me.clientY)
        if (!h || h.tableStart !== anchor.tableStart) return
        const rect = rectOf(h)
        setTableSel((rect.r0 === rect.r1 && rect.c0 === rect.c1) ? null : { tableStart: anchor.tableStart, ...rect })
      }
      const onUp = () => { document.removeEventListener('mousemove', onMove); document.removeEventListener('mouseup', onUp) }
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
      return
    }

    // Clic sur une image (bloc OU inline) → sélection du nœud (affiche la barre image).
    const hitNode = ed.state.doc.nodeAt(pos)
    if (hitNode?.type.name === 'image' || hitNode?.type.name === 'inlineImage') {
      // Double-clic sur une zone de texte riche → édition in-place (canvas).
      if (e.detail >= 2 && parseTextBoxRichAlt(hitNode.attrs.alt as string)) { enterTextBoxEdit(pos); return }
      ed.view.focus()
      ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, pos)))
      return
    }

    // Double-clic = mot ; triple-clic = paragraphe (sur le layout continu)
    const layout = contLayoutRef.current
    if (layout && e.detail >= 2) {
      const { from, to } = e.detail === 2
        ? wordBoundariesAt(layout, pos)
        : paragraphBoundariesAt(layout, pos)
      ed.chain().focus().setTextSelection({ from, to }).run()
      return
    }

    dragAnchorRef.current = pos
    ed.chain().focus().setTextSelection(pos).run()

    const extend = (clientX: number, clientY: number) => {
      const idx = pageAtPoint(clientX, clientY, pageIdx)   // hit-test 2D : bonne page même en grille
      const p2 = posFromEvent(idx, clientX, clientY)
      const a = dragAnchorRef.current
      if (p2 == null || a == null) return
      ed.chain().setTextSelection({ from: Math.min(a, p2), to: Math.max(a, p2) }).run()
    }
    const stopAuto = () => { if (autoScrollRef.current !== null) { cancelAnimationFrame(autoScrollRef.current); autoScrollRef.current = null } }
    const EDGE = 48
    const tick = () => {
      autoScrollRef.current = null
      const sc = scrollContainerRef.current; if (!sc) return
      const rect = sc.getBoundingClientRect()
      const { x, y } = lastMouseRef.current
      let dy = 0
      if (y > rect.bottom - EDGE)   dy =  Math.min(30, (y - (rect.bottom - EDGE)) / 2 + 5)
      else if (y < rect.top + EDGE) dy = -Math.min(30, ((rect.top + EDGE) - y) / 2 + 5)
      if (dy === 0) return
      const before = sc.scrollTop; sc.scrollTop += dy
      extend(x, y)
      if (sc.scrollTop !== before) autoScrollRef.current = requestAnimationFrame(tick)
    }
    const onMove = (me: MouseEvent) => {
      lastMouseRef.current = { x: me.clientX, y: me.clientY }
      extend(me.clientX, me.clientY)
      const sc = scrollContainerRef.current
      if (sc) {
        const rect = sc.getBoundingClientRect()
        const near = me.clientY > rect.bottom - EDGE || me.clientY < rect.top + EDGE
        if (near) { if (autoScrollRef.current === null) autoScrollRef.current = requestAnimationFrame(tick) }
        else stopAuto()
      }
    }
    const onUp = () => {
      stopAuto()
      document.removeEventListener('mousemove', onMove)
      document.removeEventListener('mouseup', onUp)
    }
    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [posFromEvent, scrollContainerRef, enterTextBoxEdit, exitTextBoxEdit, floatingImageAt])

  // ── Pointeur souris contextuel selon la zone survolée ───────────────────────
  // I-beam sur le texte · main sur les liens · déplacement sur les images ·
  // flèche sur les marges (hors zone de contenu). Écrit directement `style.cursor`
  // du canvas (pas de re-render React) et ne change que si la valeur diffère.
  const onPageMouseMove = useCallback((pageIdx: number, e: React.MouseEvent<HTMLCanvasElement>) => {
    const ed = editorRef.current
    const pg = pagesRef.current[pageIdx]
    const cv = e.currentTarget
    if (!ed || !pg) return
    // En édition en-tête/pied ou zone de texte : on garde le caret texte (zones gérées à part).
    if (hfEditRef.current || tbEditRef.current) {
      if (cv.style.cursor !== 'text') cv.style.cursor = 'text'
      return
    }
    const z = zoomRef.current
    const g = geomOf(pg)
    const r = cv.getBoundingClientRect()
    const x = (e.clientX - r.left) / z
    const y = (e.clientY - r.top) / z
    let cursor = 'text'
    const inContent = x >= g.marginH && x <= g.pageW - g.marginH
                   && y >= g.marginV && y <= g.pageH - g.marginBottom
    if (!inContent) {
      cursor = 'default'                       // marges / hors-texte → flèche
    } else {
      const pos = posFromEvent(pageIdx, e.clientX, e.clientY)
      if (pos != null) {
        const node = ed.state.doc.nodeAt(pos)
        if (node?.type.name === 'image') cursor = 'move'                         // image → déplacement
        else if (node?.marks.some(m => m.type.name === 'link')) cursor = 'pointer' // lien → main
      }
    }
    if (cv.style.cursor !== cursor) cv.style.cursor = cursor
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [posFromEvent])

  // ── Menu contextuel (clic droit) — contextuel selon la sélection ────────────
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null)

  const rootRef = useRef<HTMLDivElement>(null)

  // Actions sur l'image sélectionnée (NodeSelection courante). `image` (bloc) OU
  // `inlineImage` (aligné sur le texte) — on met à jour le bon type de nœud.
  const selImgType = (): 'image' | 'inlineImage' | null => {
    const s = editorRef.current?.state.selection
    if (s instanceof NodeSelection && (s.node.type.name === 'image' || s.node.type.name === 'inlineImage')) return s.node.type.name as 'image' | 'inlineImage'
    return null
  }
  const imgUpdate = (attrs: Record<string, unknown>) => { const t = selImgType(); if (t) editorRef.current?.chain().updateAttributes(t, attrs).run() }
  const imgAlign = (align: 'left' | 'center' | 'right') => { imgUpdate({ align }); requestAnimationFrame(updateImgSel) }
  const imgReset = () => { imgUpdate({ width: 0, height: 0, rotation: 0 }); requestAnimationFrame(updateImgSel) }
  // Changement d'habillage : « Aligné sur le texte » ⇄ flottant convertit le NŒUD
  // (bloc `image` ↔ `inlineImage`) pour que tous les positionnements marchent.
  const imgSetWrap = (wrap: string) => {
    const ed = editorRef.current; if (!ed) return
    const cur = selImgType()
    if (wrap === 'inline' && cur === 'image') { convertImageNode(true); return }
    if (wrap !== 'inline' && cur === 'inlineImage') { convertImageNode(false, wrap); return }
    imgUpdate({ wrap }); requestAnimationFrame(updateImgSel)
  }
  // Convertit le nœud image sélectionné bloc→inline (toInline) ou inline→bloc.
  const convertImageNode = (toInline: boolean, wrap = 'square') => {
    const ed = editorRef.current; if (!ed) return
    const sel = ed.state.selection
    if (!(sel instanceof NodeSelection)) return
    const a = sel.node.attrs
    const sch = ed.state.schema
    if (toInline) {
      // Bloc → inline : l'image devient un caractère ; on l'insère dans un paragraphe.
      const inlineNode = sch.nodes.inlineImage.create({ src: a.src, width: a.width, height: a.height, alt: a.alt, rotation: a.rotation })
      const para = sch.nodes.paragraph.create(null, inlineNode)
      const tr = ed.state.tr.replaceWith(sel.from, sel.to, para)
      tr.setSelection(NodeSelection.create(tr.doc, sel.from + 1))
      ed.view.dispatch(tr); ed.view.focus()
    } else {
      // Inline → bloc : un bloc ne peut pas vivre DANS un paragraphe → on retire le
      // caractère-image et on insère le nœud image bloc APRÈS son paragraphe.
      const blockNode = sch.nodes.image.create({ src: a.src, width: a.width, height: a.height, alt: a.alt, rotation: a.rotation, wrap, align: 'left' })
      const $f = ed.state.doc.resolve(sel.from)
      const paraEnd = $f.after($f.depth)
      let tr = ed.state.tr.delete(sel.from, sel.to)
      const insertAt = tr.mapping.map(paraEnd)
      tr = tr.insert(insertAt, blockNode)
      tr.setSelection(NodeSelection.create(tr.doc, insertAt))
      ed.view.dispatch(tr); ed.view.focus()
    }
    requestAnimationFrame(() => { renderAllPages(); updateImgSel() })
  }
  // Dialogue « Mise en page » (Position/Habillage/Taille) de l'objet sélectionné.
  const [layoutDlg, setLayoutDlg] = useState<LayoutInit | null>(null)
  // Dialogues de tableau (clic droit) : orientation du texte + propriétés du tableau.
  const [tableDlg, setTableDlg] = useState<{ kind: 'orient' | 'props'; rect: TableRect } | null>(null)
  // Ouvre le dialogue avec un instantané des attrs du nœud image courant + la
  // géométrie de page (pour les positions/tailles relatives).
  const openLayoutDialog = () => {
    const ed = editorRef.current; const sel = ed?.state.selection
    const node = sel instanceof NodeSelection && (sel.node.type.name === 'image' || sel.node.type.name === 'inlineImage') ? sel.node : null
    if (!node) return
    const a = node.attrs as Record<string, unknown>
    const geom = gRef.current
    setLayoutDlg({
      width: (a.width as number) || 240,
      height: (a.height as number) || 180,
      rotation: (a.rotation as number) || 0,
      wrap: (a.wrap as string) || 'inline',
      wrapX: (a.wrapX as number) || 0,
      wrapY: (a.wrapY as number) || 0,
      wrapSide: (a.wrapSide as string) || 'both',
      wrapDistT: (a.wrapDistT as number) || 0,
      wrapDistB: (a.wrapDistB as number) || 0,
      wrapDistL: a.wrapDistL != null ? (a.wrapDistL as number) : 10,
      wrapDistR: a.wrapDistR != null ? (a.wrapDistR as number) : 10,
      align: (a.align as string) || 'left',
      posHRel: (a.posHRel as string) || 'column',
      posVRel: (a.posVRel as string) || 'paragraph',
      moveWithText: a.moveWithText !== false,
      allowOverlap: a.allowOverlap !== false,
      lockAnchor: a.lockAnchor === true,
      pageW: geom.pageW, pageH: geom.pageH,
      contentW: geom.pageW - 2 * geom.marginH,
      contentH: geom.pageH - 2 * geom.marginV,
    })
  }

  // Drag d'une poignée : redimensionnement (symétrique autour du centre, dans le
  // repère non tourné) ou rotation. `kind` = nw/n/ne/e/se/s/sw/w/rot.
  const startHandleDrag = (kind: string) => (e: React.PointerEvent) => {
    e.preventDefault(); e.stopPropagation()
    const sel = imgSel, root = rootRef.current
    if (!sel || !root) return
    const z = zoomRef.current
    const w0 = sel.w / z, h0 = sel.h / z, ratio = w0 / h0
    const rad = sel.rotation * Math.PI / 180
    const cos = Math.cos(rad), sin = Math.sin(rad)
    // Déplacement d'un flottant : point de départ + décalage de base (wrapX/wrapY).
    const startCX = e.clientX, startCY = e.clientY
    const node0 = editorRef.current?.state.doc.nodeAt(sel.pos)
    const baseWX = Number(node0?.attrs.wrapX) || 0
    const baseWY = Number(node0?.attrs.wrapY) || 0
    const onMove = (me: PointerEvent) => {
      const rect = root.getBoundingClientRect()
      const sx = me.clientX - rect.left - sel.cx
      const sy = me.clientY - rect.top  - sel.cy
      if (kind === 'move') {
        // Repositionne le flottant : décalage cumulé en px doc.
        imgUpdate({ wrapX: Math.round(baseWX + (me.clientX - startCX) / z), wrapY: Math.round(baseWY + (me.clientY - startCY) / z) })
      } else if (kind === 'rot') {
        let ang = Math.atan2(sy, sx) * 180 / Math.PI + 90
        if (me.shiftKey) ang = Math.round(ang / 15) * 15   // accrochage 15° avec Maj
        imgUpdate({ rotation: Math.round(ang) })
      } else {
        const lx = (sx * cos + sy * sin) / z      // pointeur dans le repère image (px doc)
        const ly = (-sx * sin + sy * cos) / z
        let nw = w0, nh = h0
        if (kind.length === 2) { nw = Math.max(40, 2 * Math.abs(lx)); nh = nw / ratio }  // coin → ratio conservé
        else if (kind === 'e' || kind === 'w') nw = Math.max(40, 2 * Math.abs(lx))         // étirement horizontal
        else nh = Math.max(20, 2 * Math.abs(ly))                                            // étirement vertical
        imgUpdate({ width: Math.round(nw), height: Math.round(nh) })
      }
      updateImgSel()
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      updateImgSel()
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }

  // Redimensionnement d'un tableau (poignées de bordure). `kind` :
  //   col     = bordure interne i (redistribue entre colonnes i-1 et i)
  //   colEdge = bord droit (largeur du tableau, colonnes mises à l'échelle)
  //   row     = bordure interne j (hauteur min de la ligne j-1)
  //   rowEdge = bord bas (hauteur min de la dernière ligne)
  const startTableResize = (kind: 'col' | 'colEdge' | 'row' | 'rowEdge', tableStart: number, index: number) => (e: React.PointerEvent) => {
    e.preventDefault(); e.stopPropagation()
    const ed = editorRef.current; if (!ed) return
    const z = zoomRef.current
    // Géométrie de départ : largeurs/hauteurs dérivées des bornes du layout.
    let colX: number[] | undefined, rowY: number[] | undefined, contentW = g.contentW
    for (const pg of pagesRef.current) for (const para of pg.layout.paragraphs) {
      if (para.table && para.pmStart === tableStart && para.table.colX) { colX = para.table.colX; rowY = para.table.rowY; contentW = geomOf(pg).contentW }
    }
    const widths = colX ? colX.slice(1).map((x, i) => x - colX![i]) : []
    const heights = rowY ? rowY.slice(1).map((y, i) => y - rowY![i]) : []
    const colCount = widths.length
    const node0 = ed.state.doc.nodeAt(tableStart)
    const startRH = (((node0?.attrs.rowHeights as number[] | null) ?? []).slice())
    const startX = e.clientX, startY = e.clientY
    let raf = 0
    const apply = (me: PointerEvent) => {
      raf = 0
      const dx = (me.clientX - startX) / z, dy = (me.clientY - startY) / z
      if (kind === 'col' && colCount >= 2) {
        if (widths[index - 1] + dx < 30 || widths[index] - dx < 30) return
        const nw = widths.slice(); nw[index - 1] += dx; nw[index] -= dx
        setTableAttrAt(ed, tableStart, { colWidths: nw })
      } else if (kind === 'colEdge') {
        const total = widths.reduce((s, w) => s + w, 0)
        const nt = Math.max(colCount * 30, Math.min(contentW, total + dx))
        const f = nt / total
        setTableAttrAt(ed, tableStart, { colWidths: widths.map(w => w * f) })
      } else {
        const ri = kind === 'rowEdge' ? heights.length - 1 : index - 1
        const nh = new Array(heights.length).fill(0).map((_v, i) => startRH[i] || 0)
        nh[ri] = Math.max(MIN_TBL_ROW_H, (startRH[ri] || heights[ri]) + dy)
        setTableAttrAt(ed, tableStart, { rowHeights: nh })
      }
    }
    const onMove = (me: PointerEvent) => { if (!raf) raf = requestAnimationFrame(() => apply(me)) }
    const onUp = () => { window.removeEventListener('pointermove', onMove); window.removeEventListener('pointerup', onUp) }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }

  const ctxSpellRef = useRef<SpellIssue | null>(null)
  const onPageContextMenu = useCallback((pageIdx: number, e: React.MouseEvent) => {
    e.preventDefault()
    const ed = editorRef.current; if (!ed) return
    const sel = ed.state.selection
    // Clic droit sur une forme/image FLOTTANTE (devant le texte / carré) → la
    // sélectionner avant d'ouvrir le menu, sinon le hit-test texte sélectionnerait
    // le texte derrière et afficherait le menu du texte (pas celui de l'objet).
    const fpos = floatingImageAt(e.clientX, e.clientY)
    if (fpos != null) {
      ctxSpellRef.current = null
      ed.view.focus()
      ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, fpos)))
      setCtxMenu({ x: e.clientX, y: e.clientY })
      return
    }
    const pos = posFromEvent(pageIdx, e.clientX, e.clientY)
    // Faute sous le clic ? → suggestions en tête du menu contextuel. Bornes
    // INCLUSIVES (>=/<=) pour attraper un clic au bord du mot (sinon « impossible
    // de cliquer sur la faute »).
    ctxSpellRef.current = (pos != null && spellCheckRef.current && spellRef.current.find(i => pos >= i.from && pos <= i.to)) || null
    const insideSel = sel.from < sel.to && pos != null && pos >= sel.from && pos <= sel.to
    // Comme Google : un clic droit hors sélection déplace le curseur ; dans la
    // sélection, on la conserve.
    if (!insideSel && pos != null) ed.chain().focus().setTextSelection(pos).run()
    else ed.view.focus()
    setCtxMenu({ x: e.clientX, y: e.clientY })
  }, [posFromEvent, floatingImageAt])

  // Contexte tableau : si le curseur est dans une cellule, renvoie indices + position.
  const tableCtx = () => {
    const ed = editorRef.current; if (!ed) return null
    const $f = ed.state.selection.$from
    for (let d = $f.depth; d > 0; d--) {
      if ($f.node(d).type.name === 'tableCell') {
        const td = d - 2
        if (td < 0) return null
        return { colIndex: $f.index(d - 1), rowIndex: $f.index(td), tablePos: $f.before(td), tableNode: $f.node(td) }
      }
    }
    return null
  }
  const emptyCell = () => ({ type: 'tableCell', content: [{ type: 'paragraph' }] })
  // Reconstruit le tableau après mutation de sa structure JSON.
  const tableMutate = (fn: (rows: JSONContent[], ctx: NonNullable<ReturnType<typeof tableCtx>>) => void) => {
    const ed = editorRef.current; const ctx = tableCtx(); if (!ed || !ctx) return
    const json = ctx.tableNode.toJSON() as JSONContent
    const rows = (json.content ?? []) as JSONContent[]
    fn(rows, ctx)
    const newNode = ed.state.schema.nodeFromJSON(json)
    const tr = ed.state.tr.replaceWith(ctx.tablePos, ctx.tablePos + ctx.tableNode.nodeSize, newNode)
    ed.view.focus(); ed.view.dispatch(tr)
  }
  const tableItems = (): MenuItem[] => {
    if (!tableCtx()) return []
    const colCount = (r: JSONContent[]) => ((r[0]?.content as JSONContent[])?.length ?? 1)
    return [
      { type: 'separator' },
      { type: 'submenu', label: t('doc_table'), items: [
        { type: 'action', label: t('doc_insert_row_above'), onClick: () => tableMutate((rows, c) => rows.splice(c.rowIndex, 0, { type: 'tableRow', content: Array.from({ length: colCount(rows) }, emptyCell) })) },
        { type: 'action', label: t('doc_insert_row_below'), onClick: () => tableMutate((rows, c) => rows.splice(c.rowIndex + 1, 0, { type: 'tableRow', content: Array.from({ length: colCount(rows) }, emptyCell) })) },
        { type: 'action', label: t('doc_insert_column_left'), onClick: () => tableMutate((rows, c) => rows.forEach(r => (r.content as JSONContent[]).splice(c.colIndex, 0, emptyCell()))) },
        { type: 'action', label: t('doc_insert_column_right'), onClick: () => tableMutate((rows, c) => rows.forEach(r => (r.content as JSONContent[]).splice(c.colIndex + 1, 0, emptyCell()))) },
        { type: 'separator' },
        { type: 'action', label: t('doc_delete_row'), onClick: () => tableMutate((rows, c) => { if (rows.length > 1) rows.splice(c.rowIndex, 1) }) },
        { type: 'action', label: t('doc_delete_column'), onClick: () => tableMutate((rows, c) => { if (colCount(rows) > 1) rows.forEach(r => (r.content as JSONContent[]).splice(c.colIndex, 1)) }) },
        { type: 'action', label: t('doc_delete_table'), onClick: () => { const ed = editorRef.current, c = tableCtx(); if (ed && c) ed.chain().focus().deleteRange({ from: c.tablePos, to: c.tablePos + c.tableNode.nodeSize }).run() } },
      ] },
      // Plage de cellules courante : sélection multi-cellules sinon la cellule du curseur.
      { type: 'action', label: t('doc_text_orientation_menu', { defaultValue: 'Orientation du texte…' }), onClick: () => setTableDlg({ kind: 'orient', rect: currentCellRect() }) },
      { type: 'action', label: t('doc_table_properties_menu', { defaultValue: 'Propriétés du tableau…' }), onClick: () => setTableDlg({ kind: 'props', rect: currentCellRect() }) },
    ]
  }
  // Rect des cellules ciblées par les dialogues de tableau (sélection ou cellule seule).
  const currentCellRect = (): TableRect => {
    if (tableSelRef.current) { const s = tableSelRef.current; return { r0: s.r0, c0: s.c0, r1: s.r1, c1: s.c1 } }
    const ed = editorRef.current, c = ed && tableCtxOf(ed)
    if (c) return { r0: c.rowIndex, c0: c.colStart, r1: c.rowIndex, c1: c.colStart }
    return { r0: 0, c0: 0, r1: 0, c1: 0 }
  }

  const buildCtxItems = (): MenuItem[] => {
    const ed = editorRef.current
    const exec = (cmd: string) => { ed?.view.focus(); document.execCommand(cmd) }
    const has = !!ed && ed.state.selection.from < ed.state.selection.to
    const onLink = !!ed?.isActive('link')
    // En tête : suggestions du correcteur si clic droit sur une faute.
    const spell = ctxSpellRef.current
    const refreshSpell = () => { computeSpell(); renderAllPages() }
    // Remplace la plage de la faute par un texte (suggestion) puis recalcule.
    const replaceWith = (s: string) => ed?.chain().focus().insertContentAt({ from: spell!.from, to: spell!.to }, s).run()
    // Suggestions : liste fréquente si dispo, sinon calculées à la demande (Hunspell).
    const sugg = spell ? (spell.suggestions.length ? spell.suggestions : (spell.type === 'spelling' ? suggestWord(spell.word) : [])) : []
    // Supprime un mot répété (la faute marque le 2ᵉ mot) avec l'espace qui le précède.
    const removeRepeat = () => {
      if (!ed || !spell) return
      const before = ed.state.doc.textBetween(Math.max(0, spell.from - 1), spell.from, '', '')
      const from = before === ' ' ? spell.from - 1 : spell.from   // mange l'espace séparateur
      ed.chain().focus().deleteRange({ from, to: spell.to }).run()
    }
    const isRepeat = spell?.type === 'grammar' && spell.message === 'Mot répété'
    const spellItems: MenuItem[] = spell ? [
      // 1) Suggestions de remplacement ou « Aucune suggestion ». Une espace seule
      //    (correction de double espace) reçoit un libellé lisible.
      ...(isRepeat ? [] : (sugg.length ? sugg : [{ noSugg: true } as never]).map((s: string | { noSugg: true }) =>
        typeof s === 'string'
          ? ({ type: 'action' as const, label: s === ' ' ? t('doc_spell_single_space', { defaultValue: 'Une seule espace' }) : s, onClick: () => replaceWith(s) })
          : ({ type: 'action' as const, label: t('doc_spell_no_suggestion', { defaultValue: 'Aucune suggestion' }), disabled: true, onClick: () => {} }))),
      // 2) Correction directe d'un mot répété.
      ...(isRepeat ? [{ type: 'action' as const, label: t('doc_spell_remove_repeat', { defaultValue: 'Supprimer le mot en double' }), onClick: removeRepeat }] : []),
      { type: 'separator' },
      // 3) Options d'ignorance, façon Word.
      { type: 'action', label: t('doc_spell_ignore', { defaultValue: 'Ignorer' }),
        onClick: () => { ignoreWordSession(spell.type === 'grammar' ? '§rep§' + spell.word : spell.word); refreshSpell() } },
      ...(spell.type === 'spelling' ? [{ type: 'action' as const, label: t('doc_spell_add_dict', { defaultValue: 'Ajouter au dictionnaire' }),
        onClick: () => { ignoreWord(spell.word); refreshSpell() } }] : []),
      { type: 'separator' },
    ] : []
    return [
      ...spellItems,
      { type: 'action', label: t('common_cut'), shortcut: `${MOD}X`, disabled: !has, onClick: () => exec('cut') },
      { type: 'action', label: t('common_copy'), shortcut: `${MOD}C`, disabled: !has, onClick: () => exec('copy') },
      { type: 'action', label: t('common_paste'), shortcut: `${MOD}V`, onClick: async () => {
          try { const txt = await navigator.clipboard.readText(); ed?.chain().focus().insertContent(txt).run() } catch { exec('paste') }
        } },
      { type: 'action', label: t('doc_paste_without_formatting'), shortcut: `${MOD}${SHIFT}V`, onClick: async () => {
          try { const txt = await navigator.clipboard.readText(); ed?.chain().focus().insertContent(txt).run() } catch { /* ignore */ }
        } },
      { type: 'separator' },
      { type: 'action', label: onLink ? t('doc_edit_link') : t('doc_insert_link_ellipsis'), shortcut: `${MOD}K`,
        onClick: async () => {
          const url = await prompt({ title: t('doc_insert_link'), placeholder: 'https://exemple.com', defaultValue: ed?.getAttributes('link').href ?? '', allowEmpty: true, confirmLabel: t('doc_apply') })
          if (url === null) return
          if (url === '') ed?.chain().focus().unsetLink().run()
          else ed?.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
        } },
      { type: 'action', label: t('doc_new_comment', { defaultValue: 'Nouveau commentaire' }), disabled: !has, onClick: () => cbRef.current.onAddComment?.() },
      { type: 'separator' },
      { type: 'submenu', label: t('doc_break'), items: [
        { type: 'action', label: t('doc_page_break'), shortcut: `${MOD}↵`, onClick: () => insertPageBreak() },
        { type: 'action', label: t('doc_section_break_next_page'), onClick: () => insertBreak() },
      ] },
      { type: 'separator' },
      {
        type: 'submenu', label: t('doc_text'),
        items: [
          { type: 'action', label: t('doc_bold'),      shortcut: `${MOD}B`, checked: !!ed?.isActive('bold'),      onClick: () => ed?.chain().focus().toggleBold().run() },
          { type: 'action', label: t('doc_italic'),    shortcut: `${MOD}I`, checked: !!ed?.isActive('italic'),    onClick: () => ed?.chain().focus().toggleItalic().run() },
          { type: 'action', label: t('doc_underline'), shortcut: `${MOD}U`, checked: !!ed?.isActive('underline'), onClick: () => ed?.chain().focus().toggleUnderline().run() },
        ],
      },
      {
        type: 'submenu', label: t('doc_align'),
        items: ([['left', t('doc_align_left')], ['center', t('doc_align_center')], ['right', t('doc_align_right')], ['justify', t('doc_align_justify')]] as Array<[string, string]>)
          .map(([a, lbl]) => ({ type: 'action' as const, label: lbl, checked: !!ed?.isActive({ textAlign: a }), onClick: () => ed?.chain().focus().setTextAlign(a).run() })),
      },
      { type: 'action', label: t('doc_paragraph_dialog', { defaultValue: 'Paragraphe…' }), onClick: () => openParagraphDialog() },
      { type: 'separator' },
      { type: 'action', label: t('doc_select_all'), shortcut: `${MOD}A`, onClick: () => ed?.chain().focus().selectAll().run() },
      ...headingCollapseItems(),
      ...objectItems(),
      ...tableItems(),
    ]
  }
  // Sous-menu « Développer/Réduire » (Word) : affiché si le curseur est dans/sous un titre.
  const headingCollapseItems = (): MenuItem[] => {
    const ed = editorRef.current
    const pos = ed ? headingPosAt(ed) : null
    const redraw = () => requestAnimationFrame(() => renderAllPages())
    return [
      { type: 'separator' },
      { type: 'submenu', label: t('doc_collapse_expand', { defaultValue: 'Développer/Réduire' }), items: [
        { type: 'action', label: t('doc_expand_heading', { defaultValue: 'Développer le titre' }), disabled: pos == null, onClick: () => { if (ed && pos != null) { setHeadingCollapsed(ed, pos, false); redraw() } } },
        { type: 'action', label: t('doc_collapse_heading', { defaultValue: 'Réduire le titre' }), disabled: pos == null, onClick: () => { if (ed && pos != null) { setHeadingCollapsed(ed, pos, true); redraw() } } },
        { type: 'action', label: t('doc_expand_all_headings', { defaultValue: 'Développer tous les titres' }), onClick: () => { if (ed) { setAllHeadingsCollapsed(ed, false); redraw() } } },
        { type: 'action', label: t('doc_collapse_all_headings', { defaultValue: 'Réduire tous les titres' }), onClick: () => { if (ed) { setAllHeadingsCollapsed(ed, true); redraw() } } },
      ] },
    ]
  }

  // Items contextuels d'un objet sélectionné (zone de texte / forme SVG rééditables).
  const objectItems = (): MenuItem[] => {
    const ed = editorRef.current
    const sel = ed?.state.selection
    const node = sel instanceof NodeSelection && (sel.node.type.name === 'image' || sel.node.type.name === 'inlineImage') ? sel.node : null
    if (!node) return []
    const items: MenuItem[] = []
    // L'image inline EST « Aligné sur le texte » ; sinon l'attribut `wrap` du bloc.
    const curWrap = node.type.name === 'inlineImage' ? 'inline' : ((node.attrs.wrap as string) || 'inline')
    // `imgSetWrap` convertit le nœud (bloc ↔ inline) selon l'habillage choisi.
    const setWrap = (w: string) => imgSetWrap(w)
    // ── Items communs à tout objet (façon Word) : plan, habillage, disposition ──
    items.push(
      { type: 'separator' },
      { type: 'action', label: t('doc_bring_front', { defaultValue: 'Devant le texte' }), checked: curWrap === 'front', onClick: () => setWrap('front') },
      { type: 'action', label: t('doc_send_behind', { defaultValue: 'Derrière le texte' }), checked: curWrap === 'behind', onClick: () => setWrap('behind') },
      {
        type: 'submenu', label: t('doc_wrap_text', { defaultValue: 'Habillage du texte' }),
        items: ([
          ['inline', t('doc_wrap_inline', { defaultValue: 'Aligné sur le texte' })],
          ['square', t('doc_wrap_square', { defaultValue: 'Carré' })],
          ['topBottom', t('doc_wrap_topbottom', { defaultValue: 'Haut et bas' })],
          ['behind', t('doc_wrap_behind', { defaultValue: 'Derrière le texte' })],
          ['front', t('doc_wrap_front', { defaultValue: 'Devant le texte' })],
        ] as Array<[string, string]>).map(([m, lbl]) => ({ type: 'action' as const, label: lbl, checked: curWrap === m, onClick: () => setWrap(m) })),
      },
      { type: 'action', label: t('doc_layout_more', { defaultValue: 'Autres options de disposition…' }), onClick: () => requestAnimationFrame(openLayoutDialog) },
    )
    // Zone de texte (riche kbtextrich: OU ancienne kbtext:) → édition in-place canvas.
    const isTextBox = parseTextBoxRichAlt(node.attrs.alt as string) != null || parseTextBoxAlt(node.attrs.alt as string) != null
    if (isTextBox && sel instanceof NodeSelection) {
      const tbPos = sel.from
      const setTbAttr = (attrs: Record<string, unknown>) => {
        const n = ed?.state.doc.nodeAt(tbPos); if (!ed || !n) return
        ed.view.dispatch(ed.state.tr.setNodeMarkup(tbPos, undefined, { ...n.attrs, ...attrs }))
        requestAnimationFrame(updateImgSel)
      }
      const FILLS: Array<[string, string]> = [
        [t('doc_color_white',  { defaultValue: 'Blanc' }),  '#ffffff'],
        [t('doc_color_blue',   { defaultValue: 'Bleu' }),   '#e8f0fe'],
        [t('doc_color_green',  { defaultValue: 'Vert' }),   '#e6f4ea'],
        [t('doc_color_yellow', { defaultValue: 'Jaune' }),  '#fef7e0'],
        [t('doc_color_red',    { defaultValue: 'Rouge' }),  '#fce8e6'],
        [t('doc_color_gray',   { defaultValue: 'Gris' }),   '#f1f3f4'],
      ]
      const STROKES: Array<[string, string]> = [
        [t('doc_color_gray',   { defaultValue: 'Gris' }),   '#9aa0a6'],
        [t('doc_color_blue',   { defaultValue: 'Bleu' }),   '#1a73e8'],
        [t('doc_color_green',  { defaultValue: 'Vert' }),   '#1e8e3e'],
        [t('doc_color_red',    { defaultValue: 'Rouge' }),  '#d93025'],
        [t('doc_color_black',  { defaultValue: 'Noir' }),   '#202124'],
      ]
      const curFill = (node.attrs.tbFill as string) || '#ffffff'
      const curStroke = (node.attrs.tbStroke as string) || '#9aa0a6'
      items.push({ type: 'separator' }, {
        type: 'action', label: t('doc_edit_text_box', { defaultValue: 'Modifier la zone de texte' }),
        onClick: () => enterTextBoxEdit(tbPos),
      }, {
        type: 'submenu', label: t('doc_tb_fill', { defaultValue: 'Remplissage' }),
        items: [
          ...FILLS.map(([lbl, c]) => ({ type: 'action' as const, label: lbl, checked: curFill === c, onClick: () => setTbAttr({ tbFill: c }) })),
          { type: 'separator' as const },
          { type: 'action' as const, label: t('doc_tb_none', { defaultValue: 'Aucun' }), checked: node.attrs.tbFill === 'none', onClick: () => setTbAttr({ tbFill: 'none' }) },
        ],
      }, {
        type: 'submenu', label: t('doc_tb_border', { defaultValue: 'Contour' }),
        items: [
          ...STROKES.map(([lbl, c]) => ({ type: 'action' as const, label: lbl, checked: curStroke === c, onClick: () => setTbAttr({ tbStroke: c }) })),
          { type: 'separator' as const },
          { type: 'action' as const, label: t('doc_tb_none', { defaultValue: 'Aucun' }), checked: node.attrs.tbStroke === 'none', onClick: () => setTbAttr({ tbStroke: 'none' }) },
        ],
      })
    }
    const sp = parseShapeAlt(node.attrs.alt as string)
    if (sp) {
      const FILLS: Array<[string, string, string]> = [
        [t('doc_color_blue',   { defaultValue: 'Bleu' }),   '#dbe7ff', '#1a73e8'],
        [t('doc_color_green',  { defaultValue: 'Vert' }),   '#d8f3e3', '#1e8e3e'],
        [t('doc_color_yellow', { defaultValue: 'Jaune' }),  '#fef3d0', '#f9ab00'],
        [t('doc_color_red',    { defaultValue: 'Rouge' }),  '#fde0dd', '#d93025'],
        [t('doc_color_gray',   { defaultValue: 'Gris' }),   '#eceff1', '#5f6368'],
        [t('doc_color_white',  { defaultValue: 'Blanc' }),  '#ffffff', '#202124'],
      ]
      items.push({ type: 'separator' }, {
        type: 'submenu', label: t('doc_shape_color', { defaultValue: 'Couleur de la forme' }),
        items: FILLS.map(([lbl, fill, stroke]) => ({
          type: 'action' as const, label: lbl, checked: sp.fill === fill,
          onClick: () => {
            const w = (node.attrs.width as number) || 320, h = (node.attrs.height as number) || 200
            const params: ShapeParams = { ...sp, fill, stroke }
            ed?.chain().focus().updateAttributes('image', { src: svgToDataUrl(shapeSvg(sp.kind, w, h, fill, stroke, sp.sw)), alt: shapeAlt(params) }).run()
          },
        })),
      })
    }
    return items
  }

  const containerW = Math.max(g.pageW, ...geomsRef.current.map(x => x.pageW))
  return (
    <div ref={rootRef} className="relative" style={{ width: '100%', minWidth: containerW * zoom }}
      onMouseMove={onRootMouseMove} onMouseLeave={() => publishMouse(null)}>
      {/* Éditeur ProseMirror caché — reçoit toute la saisie clavier. Rendu via un
          PORTAIL vers <body>, HORS du conteneur scrollé : sinon, à chaque frappe, le
          navigateur défile le plus proche ancêtre scrollable (DOM) pour amener le caret
          natif (invisible, en haut du contenu) « dans la vue » → scrollTop saute à 0 puis
          est restauré → la page sautait d'une ligne à chaque Espace. Hors du conteneur,
          ce défilement natif ne touche plus le document. */}
      {createPortal(
        <div style={{ position: 'fixed', width: 1, height: 1, overflow: 'hidden', opacity: 0, top: 0, left: 0, pointerEvents: 'none', zIndex: -1 }}>
          <EditorContent editor={editor} />
        </div>,
        document.body,
      )}

      {/* Pages canvas (rendu pur du modèle unique) — taille propre à chaque section */}
      <div className="relative flex flex-wrap justify-center content-start" style={{ paddingTop: CANVAS_PAD_Y, rowGap: PAGE_GAP, columnGap: 24 }}>
        {pages.map((pg, idx) => {
          const geom = geomsRef.current[pg.secIdx] || geomsRef.current[0] || g
          return (
            <canvas
              key={idx}
              ref={el => { if (el) canvasRefs.current.set(idx, el); else canvasRefs.current.delete(idx) }}
              onMouseDown={e => onPageMouseDown(idx, e)}
              onMouseMove={e => onPageMouseMove(idx, e)}
              onContextMenu={e => onPageContextMenu(idx, e)}
              className="block bg-white shadow-sm"
              style={{ width: geom.pageW * zoom, height: geom.pageH * zoom, flex: '0 0 auto', cursor: 'text',
                       background: secMetaRef.current[pg.secIdx]?.pageColor ?? (pageBg || undefined) }}
            />
          )
        })}
      </div>

      {/* ── Surbrillance de la sélection de cellules de tableau (overlay bleu) ── */}
      {tableSel && (() => {
        const z = zoom, sel = tableSel
        const cells: React.ReactNode[] = []
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const { left, top: pageTop } = pageOrigin(idx)
          for (const para of pg.layout.paragraphs) {
            if (!para.table || para.pmStart !== sel.tableStart) continue
            for (const cell of para.table.cells) {
              const inRect = cell.c <= sel.c1 && cell.c + cell.colspan - 1 >= sel.c0 && cell.r <= sel.r1 && cell.r + cell.rowspan - 1 >= sel.r0
              if (!inRect) continue
              cells.push(<div key={`ts${idx}-${cell.r}-${cell.c}`} style={{
                position: 'absolute', pointerEvents: 'none', zIndex: 22,
                left: left + (geom.marginH + cell.x) * z, top: pageTop + (geom.marginV + cell.y) * z,
                width: cell.w * z, height: cell.h * z,
                background: 'rgba(87,133,253,0.28)', border: '1px solid rgba(26,115,232,0.6)',
              }} />)
            }
          }
        })
        return <>{cells}</>
      })()}

      {/* ── Poignées de redimensionnement des tableaux (colonnes / lignes / bord) ──
          Bandes fines sur les bordures : glisser pour redimensionner. Visibles au
          survol (filet bleu). Au-dessus du canvas, pointer-events sur la bande. */}
      {!hfEdit && (() => {
        const z = zoom
        const handles: React.ReactNode[] = []
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const { left, top: pageTop } = pageOrigin(idx)
          for (const para of pg.layout.paragraphs) {
            const tb = para.table
            if (!tb || !tb.colX || !tb.rowY || tb.colX.length < 2 || tb.rowY.length < 2) continue
            const ts = para.pmStart
            const ox = left + geom.marginH * z, oy = pageTop + geom.marginV * z
            const x0 = tb.colX[0], x1 = tb.colX[tb.colX.length - 1]
            const y0 = tb.rowY[0], y1 = tb.rowY[tb.rowY.length - 1]
            const HW = 7   // largeur de zone de préhension (px écran)
            // Bordures de colonnes internes + bord droit (largeur du tableau).
            tb.colX.forEach((cx, ci) => {
              if (ci === 0) return
              const isEdge = ci === tb.colX!.length - 1
              handles.push(<div key={`cz${idx}-${ts}-${ci}`} className="kb-tbl-rz kb-tbl-rz-v"
                onPointerDown={startTableResize(isEdge ? 'colEdge' : 'col', ts, ci)}
                style={{ left: ox + cx * z - HW / 2, top: oy + y0 * z, width: HW, height: (y1 - y0) * z, pointerEvents: 'auto' }} />)
            })
            // Bordures de lignes internes + bord bas (hauteur dernière ligne).
            tb.rowY.forEach((cy, ri) => {
              if (ri === 0) return
              const isEdge = ri === tb.rowY!.length - 1
              handles.push(<div key={`rz${idx}-${ts}-${ri}`} className="kb-tbl-rz kb-tbl-rz-h"
                onPointerDown={startTableResize(isEdge ? 'rowEdge' : 'row', ts, ri)}
                style={{ left: ox + x0 * z, top: oy + cy * z - HW / 2, width: (x1 - x0) * z, height: HW, pointerEvents: 'auto' }} />)
            })
          }
        })
        return <>{handles}</>
      })()}

      {/* ── Édition INLINE en-tête / pied (façon Word) ─────────────────────────
          Overlay absolu calé sur les pages : libellés En-tête/Pied + lignes
          pointillées sur toutes les pages, zones éditables (gauche/centre/droite)
          sur la bande active de la page d'ancrage, voile gris sur le corps. */}
      {hfEdit && (() => {
        const z = zoom
        const overlays: React.ReactNode[] = []
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const { left, top: pageTop } = pageOrigin(idx)
          const band = hfEdit.band
          // Ligne pointillée à la LIMITE de la zone de contenu, EXACTEMENT comme
          // la règle : haut de contenu = marge haute ; bas de contenu = marge basse.
          const sepY = band === 'header' ? geom.marginV : geom.pageH - geom.marginBottom
          const labelTxt = band === 'header' ? t('doc_header', { defaultValue: 'En-tête' }) : t('doc_footer', { defaultValue: 'Pied de page' })
          overlays.push(
            <div key={`hl${idx}`} style={{ position: 'absolute', left, top: pageTop + sepY * z, width: geom.pageW * z,
              borderTop: '1px dashed #5f6368', pointerEvents: 'none', zIndex: 24 }} />,
            <div key={`lb${idx}`} style={{ position: 'absolute', left: left + geom.marginH * z,
              top: pageTop + (band === 'header' ? sepY * z - 16 : sepY * z + 2), fontSize: 11, color: '#5f6368',
              background: '#fff', padding: '0 4px', pointerEvents: 'none', zIndex: 25 }}>{labelTxt}</div>,
          )
          // Voile gris sur la zone de CONTENU (corps non éditable pendant ce mode).
          overlays.push(
            <div key={`veil${idx}`} style={{ position: 'absolute', left, top: pageTop + geom.marginV * z,
              width: geom.pageW * z, height: geom.contentH * z,
              background: 'rgba(241,243,244,0.45)', pointerEvents: 'none', zIndex: 23 }} />,
          )
          // Zone d'édition RICHE (RichEditZone) UNIQUEMENT sur la page d'ancrage,
          // calée sur la bande, à l'échelle du zoom. Toolbar = cet éditeur.
          if (idx === hfEdit.pageIdx) {
            const bandTop = band === 'header' ? headerBandTop(geom) : footerBandTop(geom)
            const zoneW = geom.pageW - 2 * geom.marginH
            overlays.push(
              <div key={`hfzone${idx}`}
                onKeyDownCapture={ev => { if (ev.key === 'Escape') exitHFEdit() }}
                style={{ position: 'absolute', left: left + geom.marginH * z, top: pageTop + bandTop * z, zIndex: 26 }}>
                <RichEditZone
                  key={`${hfEdit.band}-${hfEdit.pageIdx}`}
                  doc={hfEdit.initial}
                  width={zoneW}
                  zoom={z}
                  minHeight={20}
                  autoFocus
                  placeholder={labelTxt}
                  onEditor={ed => { hfZoneEditorRef.current = ed; if (ed) cbRef.current.onHFActive?.(true, hfBarCtx(hfEdit.pageIdx, hfEdit.band), ed) }}
                  onChange={onHFZoneChange}
                />
              </div>,
            )
          }
        })
        return <>{overlays}</>
      })()}

      {/* Caret unique, positionné sur la bonne page */}
      <div ref={caretRef}
        className="docs-text-ui-cursor-blink"
        style={{ position: 'absolute', width: 2, background: '#202124', display: 'none', pointerEvents: 'none' }} />

      {/* Curseurs des autres participants (présence collaborative) */}
      {remoteCursors.map(c => (
        <div key={c.clientId}
          style={{ position: 'absolute', left: c.left, top: c.top, height: c.height,
                   width: 2, background: c.color, pointerEvents: 'none', zIndex: 20 }}>
          <div style={{ position: 'absolute', top: -16, left: -1, background: c.color, color: '#fff',
                        fontSize: 10, lineHeight: '14px', padding: '0 4px', borderRadius: 3,
                        whiteSpace: 'nowrap', fontWeight: 600 }}>
            {c.name}
          </div>
        </div>
      ))}

      {/* Curseurs souris distants (présence) — coords doc ×zoom */}
      <RemoteCursors awareness={awareness} selfClientId={awareness.clientID} field="mouse"
        toScreen={c => ({ left: c.x * zoom, top: c.y * zoom })} />

      {/* Menu contextuel (clic droit) */}
      {ctxMenu && (
        <MenuDropdown
          items={buildCtxItems()}
          pos={{ top: ctxMenu.y, left: ctxMenu.x, minWidth: 240 }}
          onClose={() => setCtxMenu(null)}
        />
      )}

      {/* Dialogues de tableau (clic droit) : orientation du texte / propriétés. */}
      {tableDlg?.kind === 'orient' && (
        <TextOrientationDialog editor={editorRef.current} rect={tableDlg.rect} onClose={() => setTableDlg(null)} />
      )}
      {tableDlg?.kind === 'props' && (
        <TablePropertiesDialog editor={editorRef.current} rect={tableDlg.rect} onClose={() => setTableDlg(null)} />
      )}

      {/* Dialogue « Paragraphe… » (Retrait et espacement / Enchaînements) */}
      {paraDlg && (
        <ParagraphDialog init={paraDlg} onApply={applyParagraphDraft} onClose={() => setParaDlg(null)} />
      )}

      {/* Dialogue « Mise en page » de l'objet (Position / Habillage / Taille) */}
      {layoutDlg && (
        <LayoutDialog
          init={layoutDlg}
          onApply={a => { imgUpdate(a); requestAnimationFrame(updateImgSel) }}
          onClose={() => setLayoutDlg(null)}
        />
      )}

      {/* Barre flottante d'image (alignement + redimensionnement) — masquée pendant
          l'édition in-place d'une zone de texte (le cadre d'édition prend le relais). */}
      {imgSel && !tbEdit && (() => {
        // type: 'corner' = cercle ; 'h' = pastille horizontale (haut/bas) ; 'v' = pastille verticale (gauche/droite).
        const HANDLES: Array<{ k: string; l: number; t: number; cur: string; type: 'corner' | 'h' | 'v' }> = [
          { k: 'nw', l: 0,   t: 0,   cur: 'nwse-resize', type: 'corner' }, { k: 'n', l: 0.5, t: 0, cur: 'ns-resize', type: 'h' }, { k: 'ne', l: 1, t: 0, cur: 'nesw-resize', type: 'corner' },
          { k: 'w',  l: 0,   t: 0.5, cur: 'ew-resize', type: 'v' },                                                               { k: 'e',  l: 1, t: 0.5, cur: 'ew-resize', type: 'v' },
          { k: 'sw', l: 0,   t: 1,   cur: 'nesw-resize', type: 'corner' }, { k: 's', l: 0.5, t: 1, cur: 'ns-resize', type: 'h' }, { k: 'se', l: 1, t: 1, cur: 'nwse-resize', type: 'corner' },
        ]
        const HANDLE_BLUE = '#1a73e8'
        return (
          <>
            {/* Boîte de poignées (tournée autour du centre comme l'image) */}
            <div style={{
              position: 'absolute', left: imgSel.cx, top: imgSel.cy, width: imgSel.w, height: imgSel.h,
              transform: `translate(-50%,-50%) rotate(${imgSel.rotation}deg)`, zIndex: 30, pointerEvents: 'none',
              border: `1.5px solid ${HANDLE_BLUE}`,
            }}>
              {/* Zone de déplacement (corps) — seulement pour un objet FLOTTANT.
                  `onContextMenu` : un clic droit sur l'objet SÉLECTIONNÉ tombe sur cet
                  overlay (pas le canvas) → on ouvre nous-mêmes le menu (objet déjà
                  sélectionné ⇒ items d'objet). Sinon le clic droit n'affichait rien. */}
              {(imgSel.wrap === 'behind' || imgSel.wrap === 'front' || imgSel.wrap === 'square') && (
                <div onPointerDown={startHandleDrag('move')}
                  onContextMenu={e => { e.preventDefault(); e.stopPropagation(); ctxSpellRef.current = null; setCtxMenu({ x: e.clientX, y: e.clientY }) }}
                  style={{ position: 'absolute', inset: 6, pointerEvents: 'auto', cursor: 'move' }} />
              )}
              {HANDLES.map(h => {
                const corner = h.type === 'corner'
                const hw = corner ? 10 : h.type === 'h' ? 16 : 7
                const hh = corner ? 10 : h.type === 'h' ? 7 : 16
                return (
                  <div key={h.k} onPointerDown={startHandleDrag(h.k)}
                    style={{
                      position: 'absolute', left: `${h.l * 100}%`, top: `${h.t * 100}%`,
                      // box-sizing border-box : la bordure est incluse dans width/height,
                      // donc marginLeft/Top = -taille/2 centre EXACTEMENT sur la position.
                      boxSizing: 'border-box',
                      width: hw, height: hh, marginLeft: -hw / 2, marginTop: -hh / 2,
                      background: '#fff', border: `1.5px solid ${HANDLE_BLUE}`,
                      borderRadius: corner ? '50%' : 999, boxShadow: '0 1px 2px rgba(0,0,0,0.18)',
                      pointerEvents: 'auto', cursor: h.cur,
                    }} />
                )
              })}
              {/* Trait reliant la poignée de rotation au BORD SUPÉRIEUR de la pastille
                  haute (s'arrête à -3.5 = demi-hauteur de la pastille) — sans la traverser. */}
              <div style={{ position: 'absolute', left: '50%', top: -22, width: 1.5, height: 18.5, marginLeft: -0.75, background: HANDLE_BLUE }} />
              {/* Poignée de rotation : cercle + icône ↻ (au-dessus du centre haut) */}
              <div onPointerDown={startHandleDrag('rot')}
                style={{
                  position: 'absolute', left: '50%', top: -22, boxSizing: 'border-box',
                  width: 22, height: 22, marginLeft: -11, marginTop: -22,
                  background: '#fff', border: `1.5px solid ${HANDLE_BLUE}`, borderRadius: '50%',
                  boxShadow: '0 1px 3px rgba(0,0,0,0.22)', pointerEvents: 'auto', cursor: 'grab',
                  display: 'flex', alignItems: 'center', justifyContent: 'center', color: HANDLE_BLUE,
                }}>
                <RotateCw size={13} style={{ pointerEvents: 'none' }} />
              </div>
            </div>

            {/* Petite barre d'alignement / réinitialisation (non tournée, sous l'image) */}
            <div style={{ position: 'absolute', left: imgSel.cx, top: imgSel.cy + imgSel.h / 2 + 10, transform: 'translateX(-50%)', zIndex: 31 }}
              className="flex items-center gap-0.5 bg-white border border-border rounded-lg shadow px-1 py-0.5"
              onMouseDown={e => e.preventDefault()}>
              <ToolBtn onClick={() => imgAlign('left')}   title={t('doc_align_left')}><AlignLeft size={14} /></ToolBtn>
              <ToolBtn onClick={() => imgAlign('center')} title={t('doc_align_center')}><AlignCenter size={14} /></ToolBtn>
              <ToolBtn onClick={() => imgAlign('right')}  title={t('doc_align_right')}><AlignRight size={14} /></ToolBtn>
              <Sep />
              <ToolBtn onClick={imgReset} title={t('doc_reset_image')}><RotateCcw size={13} /></ToolBtn>
            </div>

            {/* Bouton « Options de disposition » (façon Word) — coin haut-droit de l'objet */}
            <button
              onMouseDown={e => { e.preventDefault(); e.stopPropagation(); setWrapPanel(v => !v) }}
              style={{ position: 'absolute', left: imgSel.cx + imgSel.w / 2 + 8, top: imgSel.cy - imgSel.h / 2, zIndex: 32 }}
              className="flex items-center justify-center w-7 h-7 bg-white border border-border rounded-md shadow hover:bg-surface-2 text-text-secondary"
              title={t('doc_layout_options', { defaultValue: 'Options de disposition' })}>
              <LayoutTemplate size={15} />
            </button>

            {/* Panneau d'habillage du texte */}
            {wrapPanel && (
              <WrapOptionsPanel
                wrap={imgSel.wrap}
                left={imgSel.cx + imgSel.w / 2 + 40}
                top={imgSel.cy - imgSel.h / 2}
                onChange={w => imgSetWrap(w)}
                onClose={() => setWrapPanel(false)}
              />
            )}
          </>
        )
      })()}

      {/* ── Édition in-place d'une zone de texte riche (canvas RichEditZone) ──────
          Cadre blanc calé sur le rectangle de la boîte (rect fourni par `imgSel`),
          contenu édité par une RichEditZone (rendu canvas) ; la barre d'outils est
          routée sur son éditeur. Le contenu réel reste peint sur le canvas du corps
          (le cadre blanc le recouvre sans clignotement pendant la frappe). */}
      {tbEdit && imgSel && imgSel.pos === tbEdit.pos && (() => {
        const z = zoom
        const boxLeft = imgSel.cx - imgSel.w / 2
        const boxTop  = imgSel.cy - imgSel.h / 2
        const innerW  = Math.max(8, imgSel.w / z - 2 * RICH_TB_PAD)
        const innerH  = Math.max(8, imgSel.h / z - 2 * RICH_TB_PAD)
        // Fond de l'overlay = couleur de remplissage de la boîte (cohérence WYSIWYG).
        const tbNode = editorRef.current?.state.doc.nodeAt(tbEdit.pos)
        const fill = (tbNode?.attrs.tbFill as string) || '#ffffff'
        return (
          <div
            onKeyDownCapture={ev => { if (ev.key === 'Escape') { ev.stopPropagation(); exitTextBoxEdit() } }}
            style={{
              position: 'absolute', left: boxLeft, top: boxTop, width: imgSel.w, height: imgSel.h,
              background: fill === 'none' ? '#ffffff' : fill, border: '1.5px solid #1a73e8', boxSizing: 'border-box',
              padding: RICH_TB_PAD * z, overflow: 'hidden', zIndex: 33,
            }}>
            <RichEditZone
              key={`tb-${tbEdit.pos}`}
              doc={tbEdit.initial}
              width={innerW}
              zoom={z}
              minHeight={innerH}
              autoFocus
              placeholder={t('doc_text_box_placeholder', { defaultValue: 'Saisissez du texte…' })}
              onEditor={ed => { tbZoneEditorRef.current = ed; if (ed) cbRef.current.onTbActive?.(true, ed) }}
              onChange={onTbZoneChange}
              onHeight={onTbHeight}
            />
          </div>
        )
      })()}

      {/* Mini-barre flottante sur sélection du corps — MÊME composant partagé que
          les zones de texte / en-têtes-pieds (FormattingMiniBar). */}
      {bodyMiniBar && editorRef.current && <FormattingMiniBar editor={editorRef.current} left={bodyMiniBar.left} top={bodyMiniBar.top} />}
    </div>
  )
}

// ── Rechercher et remplacer ─────────────────────────────────────────────────

// Construit une chaîne plate du document + une carte index→position ProseMirror.
// Un '\n' (pos -1) sépare les runs de texte non contigus (limites de blocs) afin
// d'éviter les correspondances à cheval sur deux paragraphes.
function buildTextIndex(doc: import('@tiptap/pm/model').Node): { flat: string; map: number[] } {
  let flat = ''
  const map: number[] = []
  let lastEnd = -1
  doc.descendants((node, pos) => {
    if (node.isText) {
      if (lastEnd >= 0 && pos > lastEnd) { flat += '\n'; map.push(-1) }
      const t = node.text ?? ''
      for (let k = 0; k < t.length; k++) { flat += t[k]; map.push(pos + k) }
      lastEnd = pos + t.length
    }
    return true
  })
  return { flat, map }
}

interface FindOpts { matchCase: boolean; wholeWord: boolean; regex: boolean }
function isWordChar(c: string | undefined): boolean { return !!c && /[\p{L}\p{N}_]/u.test(c) }
function findMatches(doc: import('@tiptap/pm/model').Node, query: string, opts: FindOpts): Array<{ from: number; to: number }> {
  if (!query) return []
  const { flat, map } = buildTextIndex(doc)
  const res: Array<{ from: number; to: number }> = []
  const pushRange = (start: number, len: number) => {
    if (len <= 0) return
    if (opts.wholeWord && (isWordChar(flat[start - 1]) || isWordChar(flat[start + len]))) return
    const from = map[start], last = map[start + len - 1]
    if (from >= 0 && last >= 0) res.push({ from, to: last + 1 })
  }
  if (opts.regex) {
    let re: RegExp
    try { re = new RegExp(query, opts.matchCase ? 'gu' : 'giu') } catch { return [] }
    let m: RegExpExecArray | null
    while ((m = re.exec(flat)) !== null) {
      if (m[0].length === 0) { re.lastIndex++; continue }
      pushRange(m.index, m[0].length)
    }
    return res
  }
  const hay = opts.matchCase ? flat : flat.toLowerCase()
  const needle = opts.matchCase ? query : query.toLowerCase()
  let i = 0
  while (true) {
    const j = hay.indexOf(needle, i)
    if (j < 0) break
    pushRange(j, needle.length)
    i = j + needle.length
  }
  return res
}

// Barre de recherche du DOCUMENT, intégrée au topbar du shell (surcharge la barre
// « Rechercher dans Kubuno » du core quand un document est ouvert — façon Google
// Docs). Pastille blanche : champ + compteur + préc/suiv, et un chevron qui déplie
// un popover Remplacer + options (casse / mot entier / regex).
function DocSearchBar({ editor, onRanges, focusSignal }: {
  editor: Editor | null
  onRanges: (ranges: Array<{ from: number; to: number }>, active: number) => void
  focusSignal: number
}) {
  const { t } = useTranslation('office')
  const [query, setQuery]         = useState('')
  const [replaceText, setRepl]    = useState('')
  const [matchCase, setMatchCase] = useState(false)
  const [wholeWord, setWholeWord] = useState(false)
  const [useRegex, setUseRegex]   = useState(false)
  const [open, setOpen]           = useState(false)   // popover Remplacer + options
  const [idx, setIdx]             = useState(0)
  const [, setTick]               = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const barRef   = useRef<HTMLDivElement>(null)

  // Ctrl/⌘+F (signal) → focus + sélection du champ.
  useEffect(() => { if (focusSignal) { inputRef.current?.focus(); inputRef.current?.select() } }, [focusSignal])
  useEffect(() => {
    if (!editor) return
    const fn = () => setTick(n => (n + 1) & 0xffff)
    editor.on('transaction', fn)
    return () => { editor.off('transaction', fn) }
  }, [editor])

  const matches = editor ? findMatches(editor.state.doc, query, { matchCase, wholeWord, regex: useRegex }) : []
  const safeIdx = matches.length ? Math.min(idx, matches.length - 1) : 0

  // Remonte les occurrences au moteur de rendu (surbrillance jaune) ; nettoie au démontage.
  useEffect(() => { onRanges(matches, safeIdx) }, [query, matchCase, wholeWord, useRegex, safeIdx, editor?.state.doc]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => () => onRanges([], 0), []) // eslint-disable-line react-hooks/exhaustive-deps

  const go = (n: number) => {
    if (!editor || !matches.length) return
    const i = ((n % matches.length) + matches.length) % matches.length
    setIdx(i)
    const m = matches[i]
    editor.chain().setTextSelection({ from: m.from, to: m.to }).scrollIntoView().run()
  }
  const replaceOne = () => {
    if (!editor || !matches.length) return
    const m = matches[safeIdx]
    editor.chain().focus().insertContentAt({ from: m.from, to: m.to }, replaceText).setTextSelection(m.from + replaceText.length).run()
  }
  const replaceAll = () => {
    if (!editor || !matches.length) return
    let chain = editor.chain().focus()
    for (let k = matches.length - 1; k >= 0; k--) chain = chain.insertContentAt({ from: matches[k].from, to: matches[k].to }, replaceText)
    chain.run()
  }
  const optBtn = (on: boolean, set: (v: boolean) => void, label: string, title: string) => (
    <button onMouseDown={e => e.preventDefault()} onClick={() => set(!on)} title={title}
      className={`w-8 h-8 flex items-center justify-center rounded text-xs font-semibold ${on ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}>
      {label}
    </button>
  )

  return (
    <div ref={barRef} className="relative w-full">
      {/* Pastille blanche dans le topbar (remplace « Rechercher dans Kubuno »). */}
      <div className="flex items-center h-9 w-full bg-white rounded-full border border-[#e0e0e0] pl-3 pr-1"
        onKeyDown={e => { if (e.key === 'Escape') { setQuery(''); setOpen(false) } }}>
        <Search size={16} className="text-text-secondary flex-shrink-0" />
        <input ref={inputRef} value={query} onChange={e => { setQuery(e.target.value); setIdx(0) }}
          onKeyDown={e => { if (e.key === 'Enter') go(e.shiftKey ? safeIdx - 1 : safeIdx + 1) }}
          placeholder={t('doc_find_in_document', { defaultValue: 'Rechercher dans le document' })}
          className="flex-1 min-w-0 h-full px-2 bg-transparent text-sm text-text-primary placeholder:text-text-tertiary outline-none" />
        {query !== '' && (
          <span className="text-xs text-text-tertiary px-1 flex-shrink-0 tabular-nums">
            {matches.length ? `${safeIdx + 1}/${matches.length}` : '0/0'}
          </span>
        )}
        <button onClick={() => go(safeIdx - 1)} disabled={!matches.length} title={t('doc_previous')}
          className="w-7 h-7 flex items-center justify-center rounded-full text-text-secondary hover:bg-surface-2 disabled:opacity-30 flex-shrink-0"><ChevronUpIcon /></button>
        <button onClick={() => go(safeIdx + 1)} disabled={!matches.length} title={t('doc_next')}
          className="w-7 h-7 flex items-center justify-center rounded-full text-text-secondary hover:bg-surface-2 disabled:opacity-30 flex-shrink-0"><ChevronDown size={14} /></button>
        <button onClick={() => setOpen(o => !o)} title={t('doc_replace', { defaultValue: 'Remplacer' })}
          className={`w-7 h-7 flex items-center justify-center rounded-full flex-shrink-0 ${open ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}>
          <ChevronDown size={14} className={`transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </div>

      {/* Popover Remplacer + options, ancré sous la pastille. */}
      <AnchoredPopover anchorRef={barRef} open={open} onClose={() => setOpen(false)}>
        <div className="bg-white rounded-lg p-2.5 flex flex-col gap-2" style={{ width: 360 }}>
          <div className="flex items-center gap-1.5">
            <input value={replaceText} onChange={e => setRepl(e.target.value)}
              placeholder={t('doc_replace_with', { defaultValue: 'Remplacer par' })}
              className="flex-1 min-w-0 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
            <Button variant="secondary" size="sm" onClick={replaceOne} disabled={!matches.length} className="px-2 text-xs flex-shrink-0">{t('doc_replace', { defaultValue: 'Remplacer' })}</Button>
            <Button variant="secondary" size="sm" onClick={replaceAll} disabled={!matches.length} className="px-2 text-xs flex-shrink-0">{t('doc_replace_all', { defaultValue: 'Tout' })}</Button>
          </div>
          <div className="flex items-center gap-1.5">
            {optBtn(matchCase, setMatchCase, 'Aa', t('doc_match_case'))}
            {optBtn(wholeWord, setWholeWord, '[W]', t('doc_whole_word', { defaultValue: 'Mot entier' }))}
            {optBtn(useRegex, setUseRegex, '.*', t('doc_use_regex', { defaultValue: 'Expression régulière' }))}
            <span className="ml-auto text-xs text-text-tertiary">{matches.length ? `${safeIdx + 1} / ${matches.length}` : t('doc_no_match', { defaultValue: 'Aucun résultat' })}</span>
          </div>
        </div>
      </AnchoredPopover>
    </div>
  )
}

// Caractères spéciaux : palette de symboles courants insérés au curseur.
const SPECIAL_CHARS = '… — – « » “ ” ‘ ’ • · © ® ™ ° § ¶ † ‡ € £ ¥ ¢ ± × ÷ ≠ ≈ ≤ ≥ ∞ √ π ∑ ∆ µ ← → ↑ ↓ ↔ ⇒ ⇔ ★ ☆ ♦ ♥ ♠ ♣ ✓ ✗ → α β γ δ θ λ Ω'.split(' ')
function SpecialCharsBar({ editor, onClose }: { editor: Editor; onClose: () => void }) {
  const { t } = useTranslation('office')
  return (
    <div className="absolute top-2 right-4 z-40 bg-white rounded-lg border border-border shadow-lg p-2" style={{ width: 300 }}
      onKeyDown={e => { if (e.key === 'Escape') onClose() }}>
      <div className="flex items-center justify-between mb-1.5 px-1">
        <span className="text-xs font-medium text-text-secondary">{t('doc_special_chars')}</span>
        <button onClick={onClose} className="p-0.5 rounded hover:bg-surface-2 text-text-secondary"><X size={14} /></button>
      </div>
      <div className="grid gap-0.5" style={{ gridTemplateColumns: 'repeat(10, 1fr)' }}>
        {SPECIAL_CHARS.map((c, i) => (
          <button key={i} title={c}
            onMouseDown={e => e.preventDefault()}
            onClick={() => editor.chain().focus().insertContent(c).run()}
            className="h-7 flex items-center justify-center rounded hover:bg-surface-2 text-text-primary"
            style={{ fontSize: 15 }}>
            {c}
          </button>
        ))}
      </div>
    </div>
  )
}

// petite flèche haut (lucide n'est pas importé pour ChevronUp → SVG inline)
function ChevronUpIcon() {
  return <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m18 15-6-6-6 6" /></svg>
}

// ── Document editor area ──────────────────────────────────────────────────────

// ── Ruban Documents (remplace DocMenuBar + EditorToolbar + HFContextBar) ─────────
// Bouton « couleur » du ruban (texte ou surlignage) → popover ColorSwatchPicker.
function RibbonColorBtn({ editor, kind, cellRanges }: { editor: Editor | null; kind: 'text' | 'highlight'; cellRanges?: Array<{ from: number; to: number }> | null }) {
  const { t } = useTranslation('office')
  const ref = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const cur = kind === 'text'
    ? (editor?.getAttributes('textStyle').color as string) || '#202124'
    : (editor?.getAttributes('highlight').color as string) || '#ffff00'
  const apply = (hex: string) => {
    if (!editor) return
    // Sélection de cellules → applique à toutes leurs plages de contenu.
    if (cellRanges && cellRanges.length) {
      if (kind === 'text') applyMarksAcross(editor, cellRanges, { ts: { color: hex } })
      else applyMarksAcross(editor, cellRanges, { highlight: hex })
      return
    }
    if (kind === 'text') editor.chain().focus().setColor(hex).run()
    else editor.chain().focus().setHighlight({ color: hex }).run()
  }
  return (
    <>
      <button ref={ref} onMouseDown={e => e.preventDefault()} onClick={() => setOpen(o => !o)}
        className="w-7 h-[22px] flex flex-col items-center justify-center rounded hover:bg-surface-2 text-text-secondary"
        title={kind === 'text' ? t('doc_text_color') : t('doc_highlight')}>
        {kind === 'text' ? <Type size={14} /> : <Highlighter size={14} />}
        <div className="w-4 h-1 rounded-sm" style={{ background: cur }} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <ColorSwatchPicker color={cur} t={t}
          onChange={apply}
          onClose={() => setOpen(false)} customLabel={t('doc_custom_color', { defaultValue: 'Personnalisé' })} />
      </AnchoredPopover>
    </>
  )
}

function RibbonPageColorBtn({ pageColor, pageGrad, onColor, onGrad }: {
  pageColor?: string; pageGrad?: Gradient; onColor: (hex: string) => void; onGrad: (g: Gradient) => void
}) {
  const pickerTheme = useAppPickerTheme()
  return (
    <div className="flex items-center gap-1">
      <ColorField width={26} height={24} C={pickerTheme} color={pageColor ?? '#ffffff'} onChange={onColor} />
      <GradientField width={34} height={24} C={pickerTheme} value={pageGrad ?? DEFAULT_GRADIENT} onChange={onGrad} />
    </div>
  )
}

// Bouton vertical de ruban (icône au-dessus, libellé dessous) — reproduit le rendu
// d'un item « large » du ruban pour les contrôles custom Filigrane / Bordure.
function RibbonLargeBtn({ icon, label, active, btnRef, onClick }: {
  icon: React.ReactNode; label: string; active?: boolean; btnRef?: React.Ref<HTMLButtonElement>; onClick: () => void
}) {
  return (
    <button ref={btnRef} onClick={onClick}
      className={`flex flex-col items-center justify-center gap-0.5 px-2 py-1 rounded min-w-[56px] h-[58px] text-[11px] leading-tight transition-colors ${active ? 'bg-accent/15 text-accent' : 'text-text-secondary hover:bg-hover'}`}>
      {icon}
      <span className="text-center">{label}</span>
    </button>
  )
}

// Filigrane (Mise en page → Arrière-plan) : popover avec présélections (CONFIDENTIEL,
// BROUILLON, URGENT…) + texte libre, couleur, opacité, orientation. « Aucun » retire.
function RibbonWatermarkBtn({ value, onChange }: { value: WatermarkDef | null; onChange: (v: WatermarkDef | null) => void }) {
  const { t } = useTranslation('office')
  const pickerTheme = useAppPickerTheme()
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const wm = value ?? DEFAULT_WATERMARK
  const set = (patch: Partial<WatermarkDef>) => onChange({ ...wm, ...patch })
  const PRESETS = ['CONFIDENTIEL', 'BROUILLON', 'URGENT', 'NE PAS COPIER', 'ÉCHANTILLON', 'ORIGINAL']
  return (
    <>
      <RibbonLargeBtn btnRef={ref} icon={<Stamp size={20} />} active={!!value}
        label={t('doc_watermark', { defaultValue: 'Filigrane' })} onClick={() => setOpen(o => !o)} />
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-3 w-72 flex flex-col gap-2.5 text-sm bg-white border border-border rounded-lg shadow-lg">
          <div className="font-medium text-text-primary">{t('doc_watermark', { defaultValue: 'Filigrane' })}</div>
          <div className="grid grid-cols-2 gap-1">
            {PRESETS.map(p => (
              <button key={p} onClick={() => onChange({ ...wm, text: p })}
                className="px-2 py-1 rounded border border-border text-xs text-text-secondary hover:bg-hover truncate">{p}</button>
            ))}
          </div>
          <input value={value ? wm.text : ''} placeholder={t('doc_watermark_text', { defaultValue: 'Texte du filigrane' })}
            onChange={e => set({ text: e.target.value })}
            className="px-2 py-1.5 rounded border border-border bg-surface text-text-primary outline-none focus:border-accent" />
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_color', { defaultValue: 'Couleur' })}</span>
            <ColorField width={26} height={24} C={pickerTheme} color={wm.color} onChange={hex => set({ color: hex })} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_opacity', { defaultValue: 'Opacité' })}</span>
            <div className="flex-1 max-w-[150px]"><RangeSlider min={10} max={100} value={Math.round(wm.opacity * 100)} onChange={(v: number) => set({ opacity: v / 100 })} /></div>
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <Checkbox checked={wm.diagonal} onChange={v => set({ diagonal: v })} />
            <span className="text-text-secondary">{t('doc_watermark_diagonal', { defaultValue: 'En diagonale' })}</span>
          </label>
          {value && (
            <button onClick={() => { onChange(null); setOpen(false) }}
              className="mt-1 px-2 py-1.5 rounded text-xs text-danger hover:bg-hover border border-border">
              {t('doc_watermark_remove', { defaultValue: 'Supprimer le filigrane' })}
            </button>
          )}
        </div>
      </AnchoredPopover>
    </>
  )
}

// Bordure de page (Mise en page → Arrière-plan) : popover couleur / épaisseur / style
// / distance au bord. « Aucune » retire la bordure.
function RibbonPageBorderBtn({ value, onChange }: { value: PageBorderDef | null; onChange: (v: PageBorderDef | null) => void }) {
  const { t } = useTranslation('office')
  const pickerTheme = useAppPickerTheme()
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const pb = value ?? DEFAULT_PAGE_BORDER
  const set = (patch: Partial<PageBorderDef>) => onChange({ ...pb, ...patch })
  const STYLES: Array<[PageBorderDef['style'], string]> = [
    ['solid', t('doc_border_solid', { defaultValue: 'Trait plein' })],
    ['dashed', t('doc_border_dashed', { defaultValue: 'Tirets' })],
    ['dotted', t('doc_border_dotted', { defaultValue: 'Pointillés' })],
    ['double', t('doc_border_double', { defaultValue: 'Double' })],
  ]
  return (
    <>
      <RibbonLargeBtn btnRef={ref} icon={<SquareDashed size={20} />} active={!!value}
        label={t('doc_page_border', { defaultValue: 'Bordure' })} onClick={() => setOpen(o => !o)} />
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-3 w-64 flex flex-col gap-2.5 text-sm bg-white border border-border rounded-lg shadow-lg">
          <div className="font-medium text-text-primary">{t('doc_page_border', { defaultValue: 'Bordure de page' })}</div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_color', { defaultValue: 'Couleur' })}</span>
            <ColorField width={26} height={24} C={pickerTheme} color={pb.color} onChange={hex => set({ color: hex })} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_border_style', { defaultValue: 'Style' })}</span>
            <Dropdown width={130} value={pb.style} options={STYLES.map(([v, l]) => ({ value: v, label: l }))} onChange={v => set({ style: v as PageBorderDef['style'] })} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_border_width', { defaultValue: 'Épaisseur' })}</span>
            <NumberInput className="w-[72px] h-8" min={0.5} max={8} step={0.5} value={pb.width} onChange={v => set({ width: v })} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_border_margin', { defaultValue: 'Marge (px)' })}</span>
            <NumberInput className="w-[72px] h-8" min={4} max={72} step={2} value={pb.margin} onChange={v => set({ margin: v })} />
          </div>
          {!value
            ? <button onClick={() => { onChange(DEFAULT_PAGE_BORDER); }} className="mt-1 px-2 py-1.5 rounded text-xs text-accent hover:bg-hover border border-border">{t('doc_border_apply', { defaultValue: 'Appliquer une bordure' })}</button>
            : <button onClick={() => { onChange(null); setOpen(false) }} className="mt-1 px-2 py-1.5 rounded text-xs text-danger hover:bg-hover border border-border">{t('doc_border_remove', { defaultValue: 'Supprimer la bordure' })}</button>}
        </div>
      </AnchoredPopover>
    </>
  )
}

// Numéros de lignes (Mise en page) : popover Aucun / Continu / Recommencer à chaque
// page + intervalle d'affichage (toutes les N lignes), façon Word.
function RibbonLineNumbersBtn({ value, onChange }: { value: LineNumbersDef | null; onChange: (v: LineNumbersDef | null) => void }) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const cur = value ?? DEFAULT_LINE_NUMBERS
  const MODES: Array<[LineNumbersDef['mode'] | 'none', string]> = [
    ['none', t('doc_linenum_none', { defaultValue: 'Aucun' })],
    ['continuous', t('doc_linenum_continuous', { defaultValue: 'Continu' })],
    ['page', t('doc_linenum_page', { defaultValue: 'Recommencer à chaque page' })],
  ]
  return (
    <>
      <RibbonLargeBtn btnRef={ref} icon={<ListOrdered size={20} />} active={!!value}
        label={t('doc_line_numbers', { defaultValue: 'Numéros de lignes' })} onClick={() => setOpen(o => !o)} />
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-2 w-60 flex flex-col gap-0.5 text-sm bg-white border border-border rounded-lg shadow-lg">
          {MODES.map(([m, label]) => {
            const active = m === 'none' ? !value : (!!value && cur.mode === m)
            return (
              <button key={m} onClick={() => { onChange(m === 'none' ? null : { ...cur, mode: m as LineNumbersDef['mode'] }); if (m === 'none') setOpen(false) }}
                className={`flex items-center gap-2 px-2 py-1.5 rounded text-left ${active ? 'bg-accent/15 text-accent' : 'text-text-secondary hover:bg-hover'}`}>
                <Check size={14} className={active ? 'opacity-100' : 'opacity-0'} />
                <span>{label}</span>
              </button>
            )
          })}
          {value && (
            <div className="flex items-center justify-between gap-2 px-2 py-1.5 mt-1 border-t border-border">
              <span className="text-text-secondary text-xs">{t('doc_linenum_interval', { defaultValue: 'Afficher toutes les' })}</span>
              <NumberInput className="w-[64px] h-8" min={1} max={50} step={1} value={cur.interval} onChange={v => onChange({ ...cur, interval: Math.max(1, Math.round(v)) })} />
            </div>
          )}
        </div>
      </AnchoredPopover>
    </>
  )
}

// Trame de fond + encadré du paragraphe (Accueil → Paragraphe, façon Word « Bordures
// et trame »). Bouton compact + popover : nuancier de trame + style/couleur/épaisseur
// d'encadré. Les valeurs reflètent le paragraphe courant ; null = retire.
function RibbonParaShadeBtn({ shading, border, onShading, onBorder }: {
  shading?: string; border?: ParaBorderDef; onShading: (c: string | null) => void; onBorder: (b: ParaBorderDef | null) => void
}) {
  const { t } = useTranslation('office')
  const pickerTheme = useAppPickerTheme()
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const bd = border ?? DEFAULT_PARA_BORDER
  const SWATCHES = ['#fff2cc', '#fce5cd', '#d9ead3', '#cfe2f3', '#f4cccc', '#d9d2e9', '#ffffff', '#efefef', '#d9d9d9', '#000000']
  const STYLES: Array<[ParaBorderDef['style'], string]> = [['solid', 'Trait plein'], ['dashed', 'Tirets'], ['dotted', 'Pointillés'], ['double', 'Double']]
  return (
    <>
      <button ref={ref} onClick={() => setOpen(o => !o)} title={t('doc_para_shading', { defaultValue: 'Trame et bordure' })}
        className="flex items-center gap-0.5 h-7 px-1.5 rounded text-text-secondary hover:bg-hover">
        <Paintbrush size={15} />
        <span className="w-3 h-1 rounded-sm" style={{ background: shading || '#dadce0' }} />
        <ChevronDown size={12} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-3 w-64 flex flex-col gap-2 text-sm bg-white border border-border rounded-lg shadow-lg">
          <div className="font-medium text-text-primary">{t('doc_para_fill', { defaultValue: 'Trame de fond' })}</div>
          <div className="grid grid-cols-10 gap-1">
            {SWATCHES.map(col => (
              <button key={col} onClick={() => onShading(col)} title={col}
                className={`w-5 h-5 rounded border ${shading === col ? 'border-accent ring-1 ring-accent' : 'border-border'}`} style={{ background: col }} />
            ))}
          </div>
          <div className="flex items-center justify-between gap-2">
            <button onClick={() => onShading(null)} className="px-2 py-1 rounded text-xs text-text-secondary hover:bg-hover border border-border">{t('doc_para_no_fill', { defaultValue: 'Aucune trame' })}</button>
            <ColorField width={26} height={24} C={pickerTheme} color={shading ?? DEFAULT_PARA_SHADING} onChange={col => onShading(col)} />
          </div>
          <div className="font-medium text-text-primary mt-1 border-t border-border pt-2">{t('doc_para_border', { defaultValue: 'Encadré' })}</div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_border_style', { defaultValue: 'Style' })}</span>
            <Dropdown width={120} value={bd.style} options={STYLES.map(([v, l]) => ({ value: v, label: t('doc_border_' + v, { defaultValue: l }) }))} onChange={v => onBorder({ ...bd, style: v as ParaBorderDef['style'] })} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_color', { defaultValue: 'Couleur' })}</span>
            <ColorField width={26} height={24} C={pickerTheme} color={bd.color} onChange={col => onBorder({ ...bd, color: col })} />
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_border_width', { defaultValue: 'Épaisseur' })}</span>
            <NumberInput className="w-[64px] h-8" min={0.5} max={6} step={0.5} value={bd.width} onChange={v => onBorder({ ...bd, width: v })} />
          </div>
          {border
            ? <button onClick={() => onBorder(null)} className="mt-1 px-2 py-1.5 rounded text-xs text-danger hover:bg-hover border border-border">{t('doc_para_no_border', { defaultValue: "Supprimer l'encadré" })}</button>
            : <button onClick={() => onBorder(DEFAULT_PARA_BORDER)} className="mt-1 px-2 py-1.5 rounded text-xs text-accent hover:bg-hover border border-border">{t('doc_para_add_border', { defaultValue: 'Ajouter un encadré' })}</button>}
        </div>
      </AnchoredPopover>
    </>
  )
}

// Retraits & espacement numériques du paragraphe (onglet Mise en page, façon Word).
// Valeurs en points (pt) ; conversion ×PT_PX vers les attributs px du moteur.
function RibbonParaMetricsBox({ attrs, onSet }: { attrs: Record<string, unknown>; onSet: (patch: Record<string, unknown>) => void }) {
  const { t } = useTranslation('office')
  const PX = 96 / 72
  const il = Math.round((((attrs.indentLeft as number) ?? 0)) / PX)
  const ir = Math.round((((attrs.indentRight as number) ?? 0)) / PX)
  const sb = Math.round((((attrs.spaceBefore as number) ?? 0)) / PX)
  const sa = Math.round((((attrs.spaceAfter as number) ?? 0)) / PX)
  const row = (label: string, val: number, key: string) => (
    <div className="flex items-center justify-between gap-1">
      <span className="text-text-secondary text-[11px] w-20">{label}</span>
      <NumberInput className="w-[58px] h-7" min={0} max={400} step={6} value={val} onChange={n => onSet({ [key]: Math.max(0, Math.round(n)) * PX })} />
    </div>
  )
  return (
    <div className="flex flex-col gap-0.5">
      {row(t('doc_indent_left', { defaultValue: 'Retrait g.' }), il, 'indentLeft')}
      {row(t('doc_indent_right', { defaultValue: 'Retrait d.' }), ir, 'indentRight')}
      {row(t('doc_space_before', { defaultValue: 'Espace avant' }), sb, 'spaceBefore')}
      {row(t('doc_space_after', { defaultValue: 'Espace après' }), sa, 'spaceAfter')}
    </div>
  )
}

// Format des numéros de page (Word) : format (1/i/I/a/A) + premier numéro.
function RibbonPageNumFormatBtn({ format, start, onFormat, onStart }: {
  format: PageNumFormat; start: number; onFormat: (f: PageNumFormat) => void; onStart: (n: number) => void
}) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const FORMATS: Array<[PageNumFormat, string]> = [
    ['arabic', '1, 2, 3'], ['roman-lower', 'i, ii, iii'], ['roman-upper', 'I, II, III'], ['alpha-lower', 'a, b, c'], ['alpha-upper', 'A, B, C'],
  ]
  return (
    <>
      <button ref={ref} onClick={() => setOpen(o => !o)} title={t('doc_pagenum_format', { defaultValue: 'Format des numéros de page' })}
        className="flex items-center gap-0.5 h-7 px-1.5 rounded text-text-secondary hover:bg-hover text-xs">
        <Hash size={14} />{formatPageNumber(start, format)}<ChevronDown size={12} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-3 w-56 flex flex-col gap-2 text-sm bg-white border border-border rounded-lg shadow-lg">
          <div className="font-medium text-text-primary">{t('doc_pagenum_format', { defaultValue: 'Format des numéros de page' })}</div>
          <Dropdown width="100%" value={format} options={FORMATS.map(([v, l]) => ({ value: v, label: l }))} onChange={v => onFormat(v as PageNumFormat)} />
          <div className="flex items-center justify-between gap-2">
            <span className="text-text-secondary">{t('doc_pagenum_start', { defaultValue: 'Commencer à' })}</span>
            <NumberInput className="w-[72px] h-8" min={0} max={9999} step={1} value={start} onChange={n => onStart(Math.max(0, Math.round(n)))} />
          </div>
        </div>
      </AnchoredPopover>
    </>
  )
}

// Enchaînements de paragraphe (Word « Enchaînements ») : cases à cocher des
// attributs de pagination/numérotation portés par le paragraphe courant.
function RibbonParaFlowBtn({ attrs, onSet }: { attrs: Record<string, unknown>; onSet: (patch: Record<string, unknown>) => void }) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const FLAGS: Array<[string, string]> = [
    ['keepNext', t('doc_keep_next', { defaultValue: 'Solidaire du paragraphe suivant' })],
    ['keepLines', t('doc_keep_lines', { defaultValue: 'Lignes solidaires' })],
    ['pageBreakBefore', t('doc_pbb', { defaultValue: 'Saut de page avant' })],
    ['contextualSpacing', t('doc_contextual', { defaultValue: 'Pas d’espace entre paragraphes de même style' })],
    ['suppressLineNumbers', t('doc_suppress_lnum', { defaultValue: 'Supprimer les numéros de ligne' })],
    ['dontHyphenate', t('doc_dont_hyphenate', { defaultValue: 'Ne pas couper les mots' })],
  ]
  return (
    <>
      <button ref={ref} onClick={() => setOpen(o => !o)} title={t('doc_para_flow', { defaultValue: 'Enchaînements' })}
        className="flex items-center gap-0.5 h-7 px-1.5 rounded text-text-secondary hover:bg-hover">
        <WrapText size={15} /><ChevronDown size={12} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-2 w-72 flex flex-col gap-0.5 text-sm bg-white border border-border rounded-lg shadow-lg">
          {FLAGS.map(([k, label]) => (
            <label key={k} className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-hover cursor-pointer">
              <Checkbox checked={!!attrs[k]} onChange={v => onSet({ [k]: v })} />
              <span className="text-text-secondary">{label}</span>
            </label>
          ))}
        </div>
      </AnchoredPopover>
    </>
  )
}

// ── Galerie de formes (Insertion → Formes, façon Word) ──────────────────────
const SHAPES_RECENT_KEY = 'kubuno.doc.recentShapes'
const VALID_GALLERY_KINDS = new Set<string>([...SHAPE_CATALOG.flatMap(c => c.shapes.map(s => s.kind))])
function loadRecentShapes(): GalleryKind[] {
  try {
    const a = JSON.parse(localStorage.getItem(SHAPES_RECENT_KEY) || '[]')
    // Filtre les kinds obsolètes (versions antérieures de la galerie).
    return Array.isArray(a) ? (a as string[]).filter(k => VALID_GALLERY_KINDS.has(k)).slice(0, 12) as GalleryKind[] : []
  } catch { return [] }
}
function pushRecentShape(k: GalleryKind) {
  try { const cur = loadRecentShapes().filter(x => x !== k); cur.unshift(k); localStorage.setItem(SHAPES_RECENT_KEY, JSON.stringify(cur.slice(0, 12))) } catch { /* localStorage indisponible */ }
}

// Une vignette cliquable de la galerie : aperçu SVG (contour gris façon Word).
function ShapeThumb({ kind, onPick, t }: { kind: GalleryKind; onPick: (k: GalleryKind) => void; t: (key: string, o?: Record<string, unknown>) => string }) {
  const label = shapeLabel(kind, t)
  return (
    <button type="button" title={label} aria-label={label} onMouseDown={e => e.preventDefault()} onClick={() => onPick(kind)}
      className="w-7 h-7 flex items-center justify-center rounded hover:bg-surface-2 active:bg-surface-3">
      <img src={svgToDataUrl(galleryThumbSvg(kind))} alt="" width={22} height={19} draggable={false} />
    </button>
  )
}

// Bouton « Formes » du ruban Insertion : ouvre une galerie ancrée par catégorie.
function RibbonShapesBtn({ onInsert, onInsertTextBox }: { onInsert: (k: ShapeKind) => void; onInsertTextBox: () => void }) {
  const { t } = useTranslation('office')
  const ref = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const [recent, setRecent] = useState<GalleryKind[]>(() => loadRecentShapes())
  const pick = (k: GalleryKind) => {
    pushRecentShape(k); setRecent(loadRecentShapes()); setOpen(false)
    if (k === 'textBox') onInsertTextBox()
    else onInsert(k)
  }
  return (
    <>
      <button ref={ref} onMouseDown={e => e.preventDefault()} onClick={() => setOpen(o => !o)}
        className="flex flex-col items-center justify-center gap-0.5 px-2 py-1 rounded hover:bg-surface-2 text-text-secondary min-w-[3.4rem]"
        title={t('doc_shapes', { defaultValue: 'Formes' })}>
        <Shapes size={22} />
        <span className="text-[11px] leading-none flex items-center gap-0.5">{t('doc_shapes', { defaultValue: 'Formes' })}<ChevronDown size={10} /></span>
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="w-[268px] max-h-[64vh] overflow-y-auto p-2 select-none bg-surface-1 border border-border rounded-xl shadow-2xl">
          {recent.length > 0 && (
            <div className="mb-1.5">
              <div className="text-[11px] font-semibold text-text-secondary px-1 pb-1">{t('doc_shapes_recent', { defaultValue: 'Formes récemment utilisées' })}</div>
              <div className="flex flex-wrap gap-0.5">
                {recent.map((k, i) => <ShapeThumb key={`r${i}`} kind={k} onPick={pick} t={t} />)}
              </div>
            </div>
          )}
          {SHAPE_CATALOG.map(cat => (
            <div key={cat.id} className="mb-1.5">
              <div className="text-[11px] font-semibold text-text-secondary px-1 pt-0.5 pb-1">{t(`doc_shapes_cat_${cat.id}`, { defaultValue: cat.title })}</div>
              <div className="flex flex-wrap gap-0.5">
                {cat.shapes.map(sp => <ShapeThumb key={sp.kind} kind={sp.kind} onPick={pick} t={t} />)}
              </div>
            </div>
          ))}
        </div>
      </AnchoredPopover>
    </>
  )
}

// ── Styles nommés (façon Word) ──────────────────────────────────────────────
// Une définition de style = type de bloc + mise en forme concrète. Appliquer un
// style pose ces marques sur le paragraphe ET mémorise son id (attr styleName) pour
// pouvoir le mettre à jour. Les définitions intégrées sont surchargeables par
// document (persistées dans content_json.styles).
interface NamedStyle {
  id: string
  nameKey: string          // clé i18n du libellé (vide pour un style personnalisé)
  name?: string            // libellé libre (styles personnalisés)
  block: 'paragraph' | 'heading'
  level?: number
  font?: string
  size?: number            // pt
  bold?: boolean
  italic?: boolean
  color?: string
  align?: 'left' | 'center' | 'right' | 'justify'
  lineHeight?: number
  spaceBefore?: number
  spaceAfter?: number
  builtin?: boolean
}

const DEFAULT_STYLES: NamedStyle[] = [
  { id: 'normal',    nameKey: 'doc_style_normal',    block: 'paragraph', font: 'Arial', size: 11, lineHeight: 1.15, builtin: true },
  { id: 'noSpacing', nameKey: 'doc_style_no_spacing', block: 'paragraph', font: 'Arial', size: 11, lineHeight: 1.0, spaceBefore: 0, spaceAfter: 0, builtin: true },
  { id: 'title',     nameKey: 'doc_style_title',     block: 'paragraph', font: 'Arial', size: 28, color: '#202124', spaceBefore: 4, spaceAfter: 6, builtin: true },
  { id: 'subtitle',  nameKey: 'doc_style_subtitle',  block: 'paragraph', font: 'Arial', size: 15, italic: true, color: '#5f6368', spaceAfter: 12, builtin: true },
  { id: 'heading1',  nameKey: 'doc_heading_1',       block: 'heading', level: 1, font: 'Arial', size: 24, bold: true, color: '#202124', builtin: true },
  { id: 'heading2',  nameKey: 'doc_heading_2',       block: 'heading', level: 2, font: 'Arial', size: 18, bold: true, color: '#202124', builtin: true },
  { id: 'heading3',  nameKey: 'doc_heading_3',       block: 'heading', level: 3, font: 'Arial', size: 14, bold: true, color: '#434649', builtin: true },
  { id: 'heading4',  nameKey: 'doc_heading_4',       block: 'heading', level: 4, font: 'Arial', size: 13, bold: true, color: '#434649', builtin: true },
  { id: 'quote',     nameKey: 'doc_style_quote',     block: 'paragraph', font: 'Georgia', size: 11, italic: true, color: '#5f6368', align: 'left', spaceBefore: 8, spaceAfter: 8, builtin: true },
]

// Libellés par défaut (FR) des styles intégrés — sert de defaultValue i18n pour que
// le menu n'affiche jamais la clé brute si la traduction n'existe pas encore.
const STYLE_LABELS: Record<string, string> = {
  doc_style_normal: 'Normal', doc_style_no_spacing: 'Sans interligne', doc_style_title: 'Titre',
  doc_style_subtitle: 'Sous-titre', doc_heading_1: 'Titre 1', doc_heading_2: 'Titre 2',
  doc_heading_3: 'Titre 3', doc_heading_4: 'Titre 4', doc_style_quote: 'Citation',
}
function styleLabel(s: NamedStyle, t: (k: string, o?: Record<string, unknown>) => string): string {
  return s.nameKey ? t(s.nameKey, { defaultValue: STYLE_LABELS[s.nameKey] || s.id }) : (s.name || s.id)
}

// Fusionne les définitions intégrées avec les surcharges par document.
function mergeStyles(overrides: Record<string, Partial<NamedStyleMeta>> | undefined): NamedStyle[] {
  const ov = overrides ?? {}
  const list: NamedStyle[] = DEFAULT_STYLES.map(s => ({ ...s, ...(ov[s.id] || {}) }))
  // Styles personnalisés (non intégrés) ajoutés par l'utilisateur.
  for (const [id, def] of Object.entries(ov)) if (!DEFAULT_STYLES.some(s => s.id === id) && def.block) list.push({ id, nameKey: '', builtin: false, block: def.block, ...def })
  return list
}

// Applique un style nommé aux blocs de la sélection (type + marques concrètes).
function applyNamedStyle(ed: Editor, s: NamedStyle): void {
  const { state } = ed
  const from = state.selection.$from.start()
  const to = state.selection.$to.end()
  let chain = ed.chain().focus().setTextSelection({ from, to })
  const attrs: Record<string, unknown> = { styleName: s.id, lineHeight: s.lineHeight ?? null, spaceBefore: s.spaceBefore ?? null, spaceAfter: s.spaceAfter ?? null }
  if (s.block === 'heading' && s.level) chain = chain.setNode('heading', { level: s.level, ...attrs })
  else chain = chain.setNode('paragraph', attrs)
  chain = chain.setTextAlign(s.align ?? 'left')
  // Marques de caractère : on repart d'une base propre puis on pose la mise en forme.
  chain = chain.unsetAllMarks()
  const ts: Record<string, unknown> = {}
  if (s.font) ts.fontFamily = s.font
  if (s.size) ts.fontSize = `${s.size}pt`
  if (Object.keys(ts).length) chain = chain.setMark('textStyle', ts)
  if (s.color) chain = chain.setColor(s.color)
  if (s.bold) chain = chain.setMark('bold')
  if (s.italic) chain = chain.setMark('italic')
  chain.run()
}

// Réapplique une définition (après édition) à TOUS les blocs portant ce styleName.
function reapplyStyle(ed: Editor, s: NamedStyle): void {
  const targets: number[] = []
  ed.state.doc.descendants((node, pos) => {
    if ((node.type.name === 'paragraph' || node.type.name === 'heading') && node.attrs.styleName === s.id) targets.push(pos)
    return true
  })
  if (!targets.length) return
  // Traite du dernier au premier (les positions des suivants ne bougent pas).
  for (let i = targets.length - 1; i >= 0; i--) {
    const pos = targets[i]
    ed.chain().setTextSelection(pos + 1).run()
    applyNamedStyle(ed, s)
  }
}

// ── Opérations de tableau (sur l'éditeur actif) ─────────────────────────────
const emptyCellJSON = (): JSONContent => ({ type: 'tableCell', content: [{ type: 'paragraph' }] })

// Contexte tableau du curseur : position du tableau/cellule + indices + colonne de départ.
function tableCtxOf(ed: Editor) {
  const $f = ed.state.selection.$from
  for (let d = $f.depth; d > 0; d--) {
    if ($f.node(d).type.name === 'tableCell') {
      const td = d - 2
      if (td < 0) return null
      const rowIndex = $f.index(td)
      const cellIndexInRow = $f.index(d - 1)
      const tablePos = $f.before(td)
      const tableNode = $f.node(td)
      const rowNode = tableNode.child(rowIndex)
      let colStart = 0
      for (let i = 0; i < cellIndexInRow; i++) colStart += rowNode.child(i).attrs.colspan || 1
      return { tablePos, tableNode, rowIndex, cellIndexInRow, colStart }
    }
  }
  return null
}
function isInTable(ed: Editor | null): boolean { return !!ed && !!tableCtxOf(ed) }

// Reconstruit le tableau après mutation de sa structure JSON (insertion/suppression/fusion).
function tableMutateOn(ed: Editor, fn: (rows: JSONContent[], ctx: NonNullable<ReturnType<typeof tableCtxOf>>) => void): void {
  const ctx = tableCtxOf(ed); if (!ctx) return
  const json = ctx.tableNode.toJSON() as JSONContent
  const rows = (json.content ?? []) as JSONContent[]
  fn(rows, ctx)
  if (!rows.length) return
  const newNode = ed.state.schema.nodeFromJSON(json)
  const tr = ed.state.tr.replaceWith(ctx.tablePos, ctx.tablePos + ctx.tableNode.nodeSize, newNode)
  // Replace le curseur DANS le nouveau tableau (1ʳᵉ cellule) pour garder l'onglet
  // contextuel actif après l'opération (sinon le curseur peut atterrir hors tableau).
  const inside = Math.min(tr.doc.content.size - 1, ctx.tablePos + 3)
  tr.setSelection(TextSelection.near(tr.doc.resolve(inside), 1))
  ed.view.focus(); ed.view.dispatch(tr)
}
const cellAttrs = (c: JSONContent) => (c.attrs ?? {}) as Record<string, unknown>

// Fractionne la cellule courante fusionnée → réinsère les cellules vides manquantes.
function splitCell(ed: Editor): void {
  tableMutateOn(ed, (rows, ctx) => {
    const row = rows[ctx.rowIndex], cells = row.content as JSONContent[]
    const anchor = cells[ctx.cellIndexInRow]
    const cs = Number(cellAttrs(anchor).colspan) || 1, rs = Number(cellAttrs(anchor).rowspan) || 1
    if (cs <= 1 && rs <= 1) return
    anchor.attrs = { ...cellAttrs(anchor), colspan: 1, rowspan: 1 }
    for (let k = 1; k < cs; k++) cells.splice(ctx.cellIndexInRow + 1, 0, emptyCellJSON())
    for (let r = 1; r < rs; r++) {
      const rr = rows[ctx.rowIndex + r]; if (!rr) continue
      const rc = rr.content as JSONContent[]
      let csum = 0, ins = rc.length
      for (let i = 0; i < rc.length; i++) { if (csum >= ctx.colStart) { ins = i; break } csum += Number(cellAttrs(rc[i]).colspan) || 1 }
      for (let k = 0; k < cs; k++) rc.splice(ins, 0, emptyCellJSON())
    }
  })
}

// ── Sélection de plage de cellules (modèle maison, rendu canvas) ─────────────
// Rectangle en coordonnées de GRILLE (lignes/colonnes), indépendant des fusions.
export interface TableRect { r0: number; c0: number; r1: number; c1: number }

// Construit la grille d'occupation : grid[r][c] = { ri, ci } (cellule JSON occupant
// la case), en tenant compte des colspan/rowspan. Base des opérations sur plage.
type GridSlot = { ri: number; ci: number }
function buildGrid(rows: JSONContent[]): Array<Array<GridSlot | undefined>> {
  const grid: Array<Array<GridSlot | undefined>> = []
  rows.forEach((row, ri) => {
    const cells = (row.content ?? []) as JSONContent[]
    grid[ri] = grid[ri] || []
    let c = 0
    cells.forEach((cell, ci) => {
      while (grid[ri][c] !== undefined) c++
      const cs = Number(cellAttrs(cell).colspan) || 1
      const rs = Number(cellAttrs(cell).rowspan) || 1
      for (let dr = 0; dr < rs; dr++) for (let dc = 0; dc < cs; dc++) {
        const r2 = ri + dr; (grid[r2] = grid[r2] || [])[c + dc] = { ri, ci }
      }
      c += cs
    })
  })
  return grid
}
const gridCols = (grid: Array<Array<GridSlot | undefined>>) => Math.max(1, ...grid.map(g => g.length))

// Plages PM (contenu) des cellules dont la case de grille intersecte `rect`. Sert à
// appliquer une mise en forme sur la sélection de cellules. `tableStart` = pos du nœud table.
function cellContentRanges(ed: Editor, tableStart: number, rect: TableRect): Array<{ from: number; to: number }> {
  const table = ed.state.doc.nodeAt(tableStart)
  if (!table || table.type.name !== 'table') return []
  const ranges: Array<{ from: number; to: number }> = []
  const grid: Array<Array<boolean | undefined>> = []
  let ri = -1
  table.forEach((row, rowOff) => {
    ri++
    grid[ri] = grid[ri] || []
    let c = 0
    row.forEach((cell, cellOff) => {
      while (grid[ri][c] !== undefined) c++
      const cs = Number(cell.attrs.colspan) || 1, rs = Number(cell.attrs.rowspan) || 1
      for (let dr = 0; dr < rs; dr++) for (let dc = 0; dc < cs; dc++) (grid[ri + dr] = grid[ri + dr] || [])[c + dc] = true
      // Intersection avec le rectangle de sélection ?
      if (c + cs - 1 >= rect.c0 && c <= rect.c1 && ri + rs - 1 >= rect.r0 && ri <= rect.r1) {
        const cellPos = tableStart + 1 + rowOff + 1 + cellOff   // pos du nœud cellule
        ranges.push({ from: cellPos + 1, to: cellPos + cell.nodeSize - 1 })
      }
      c += cs
    })
  })
  return ranges
}

// Fusionne le rectangle de cellules dans la cellule haut-gauche (colspan/rowspan),
// concatène les contenus, supprime les autres cellules JSON.
function mergeRect(ed: Editor, rect: TableRect): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    const tl = grid[rect.r0]?.[rect.c0]; if (!tl) return
    const seen = new Set<string>(); const others: GridSlot[] = []
    for (let r = rect.r0; r <= rect.r1; r++) for (let c = rect.c0; c <= rect.c1; c++) {
      const g = grid[r]?.[c]; if (!g) continue
      const k = g.ri + ',' + g.ci; if (seen.has(k)) continue; seen.add(k)
      if (!(g.ri === tl.ri && g.ci === tl.ci)) others.push(g)
    }
    if (!others.length) return
    const anchor = (rows[tl.ri].content as JSONContent[])[tl.ci]
    anchor.attrs = { ...cellAttrs(anchor), colspan: rect.c1 - rect.c0 + 1, rowspan: rect.r1 - rect.r0 + 1 }
    for (const g of others) {
      const cc = ((rows[g.ri].content as JSONContent[])[g.ci].content ?? []) as JSONContent[]
      // N'absorbe que le contenu réel (ignore les paragraphes vides).
      const real = cc.filter(n => !(n.type === 'paragraph' && !(n.content?.length)))
      if (real.length) anchor.content = [...(anchor.content ?? []), ...real]
    }
    others.sort((a, b) => b.ri - a.ri || b.ci - a.ci)
    for (const g of others) (rows[g.ri].content as JSONContent[]).splice(g.ci, 1)
  })
}
// Modifie des attributs d'un tableau identifié par sa position PM (sans dépendre du
// curseur) — pour le redimensionnement des colonnes/lignes piloté par les poignées.
function setTableAttrAt(ed: Editor, tableStart: number, attrs: Record<string, unknown>): void {
  const node = ed.state.doc.nodeAt(tableStart)
  if (!node || node.type.name !== 'table') return
  ed.view.dispatch(ed.state.tr.setNodeMarkup(tableStart, undefined, { ...node.attrs, ...attrs }))
}

// Applique une couleur de fond à toutes les cellules du rectangle.
// Applique des attributs à toutes les cellules d'une plage (dédoublonnage des
// cellules fusionnées via la grille). Base commune : trame, alignement vertical,
// orientation du texte, etc.
function setCellsAttr(ed: Editor, rect: TableRect, attrs: Record<string, unknown>): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    const seen = new Set<string>()
    for (let r = rect.r0; r <= rect.r1; r++) for (let c = rect.c0; c <= rect.c1; c++) {
      const g = grid[r]?.[c]; if (!g) continue
      const k = g.ri + ',' + g.ci; if (seen.has(k)) continue; seen.add(k)
      const cell = (rows[g.ri].content as JSONContent[])[g.ci]
      cell.attrs = { ...cellAttrs(cell), ...attrs }
    }
  })
}
// hex = couleur de trame ; null = retirer la trame (« Aucune couleur », façon Word).
function setCellsBg(ed: Editor, rect: TableRect, hex: string | null): void { setCellsAttr(ed, rect, { cellBg: hex }) }
// Supprime une colonne de grille (décrémente les colspan, retire les cellules 1×).
function deleteOneCol(ed: Editor, col: number): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    if (gridCols(grid) <= 1) return
    const handled = new Set<string>()
    for (let r = 0; r < rows.length; r++) {
      const g = grid[r]?.[col]; if (!g) continue
      const k = g.ri + ',' + g.ci; if (handled.has(k)) continue; handled.add(k)
      const cell = (rows[g.ri].content as JSONContent[])[g.ci]
      const cs = Number(cellAttrs(cell).colspan) || 1
      if (cs > 1) cell.attrs = { ...cellAttrs(cell), colspan: cs - 1 }
      else (rows[g.ri].content as JSONContent[]).splice(g.ci, 1)
    }
  })
}
// Supprime une ligne de grille (décrémente les rowspan venant d'au-dessus).
function deleteOneRow(ed: Editor, row: number): void {
  tableMutateOn(ed, rows => {
    if (rows.length <= 1) return
    const grid = buildGrid(rows)
    const handled = new Set<string>()
    for (let c = 0; c < gridCols(grid); c++) {
      const g = grid[row]?.[c]; if (!g) continue
      const k = g.ri + ',' + g.ci; if (handled.has(k)) continue; handled.add(k)
      const cell = (rows[g.ri].content as JSONContent[])[g.ci]
      const rs = Number(cellAttrs(cell).rowspan) || 1
      if (g.ri < row && rs > 1) cell.attrs = { ...cellAttrs(cell), rowspan: rs - 1 }   // venant d'au-dessus
    }
    rows.splice(row, 1)
  })
}
// Suppression d'une PLAGE de lignes/colonnes (de la fin vers le début → indices stables).
function deleteRowsRange(ed: Editor, r0: number, r1: number): void { for (let r = r1; r >= r0; r--) deleteOneRow(ed, r) }
function deleteColsRange(ed: Editor, c0: number, c1: number): void { for (let c = c1; c >= c0; c--) deleteOneCol(ed, c) }

// Insertion d'une ligne/colonne à une position de grille (cellules 1× simples).
function insertRowAt(ed: Editor, atRow: number): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    rows.splice(atRow, 0, { type: 'tableRow', content: Array.from({ length: gridCols(grid) }, emptyCellJSON) })
  })
}
function insertColAt(ed: Editor, atCol: number): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    rows.forEach((row, ri) => {
      const cells = row.content as JSONContent[]
      // index d'insertion = 1ʳᵉ cellule JSON dont la colonne de grille >= atCol.
      let ins = cells.length, cc = 0
      for (let i = 0; i < cells.length; i++) { if (cc >= atCol) { ins = i; break } cc += Number(cellAttrs(cells[i]).colspan) || 1 }
      // Si une cellule fusionnée chevauche atCol, on l'élargit au lieu d'insérer.
      const g = grid[ri]?.[atCol]
      const gLeft = grid[ri]?.[atCol - 1]
      if (g && gLeft && g.ri === gLeft.ri && g.ci === gLeft.ci) {
        const cell = cells[g.ci]; cell.attrs = { ...cellAttrs(cell), colspan: (Number(cellAttrs(cell).colspan) || 1) + 1 }
      } else cells.splice(ins, 0, emptyCellJSON())
    })
  })
}

// Contexte du ruban contextuel « Tableau ».
interface TableRibbonCtx {
  onRowAbove: () => void; onRowBelow: () => void; onColLeft: () => void; onColRight: () => void
  onDeleteRow: () => void; onDeleteCol: () => void; onDeleteTable: () => void
  onMerge: () => void; onSplit: () => void; canMerge: boolean
  onStyle: (s: string) => void; curStyle: string
  cellColorNode: React.ReactNode
}

interface DocRibbonCtx {
  t: (k: string, o?: Record<string, unknown>) => string
  fmt: Editor | null
  body: Editor | null
  fonts: string[]
  // Contenu du backstage de l'onglet « Fichier » (façon Office), fourni par l'éditeur.
  fileBackstage?: React.ReactNode
  zoom: number; onZoom: (z: number) => void
  /** Ajuste le zoom à la fenêtre : largeur de page · page entière · plusieurs pages. */
  onZoomFit: (mode: 'width' | 'page' | 'multi') => void
  /** Ouvre la boîte de dialogue Zoom (presets + personnalisé). */
  onZoomDialog: () => void
  orientation: Orientation; onOrientation: (o: Orientation) => void
  columns: number; onColumns: (n: number) => void
  paperSize: PaperSize; onPaperSize: (p: PaperSize) => void
  pageNumbers: PageNumbers; onPageNumbers: (p: PageNumbers) => void
  onPageBreak: () => void; onSectionBreak: () => void
  onUploadImage: () => void; onImageUrl: () => void
  onInsertShape: (k: ShapeKind) => void; onInsertTextBox: () => void; onInsertTable: () => void
  onSetHeader: () => void; onSetFooter: () => void; onInsertToc: () => void; onSpecialChars: () => void
  onLink: () => void
  // Vague « +50 » : transformations & insertions
  onChangeCase: (m: CaseMode) => void
  onSortParas: (d: 'asc' | 'desc') => void
  onInsertField: (k: 'date' | 'time' | 'datetime') => void
  onWordCount: () => void
  onInsertBookmark: () => void
  onGoTo: () => void
  onInsertCaption: () => void
  onInsertHr: () => void
  onTextTool: (k: 'empties' | 'spaces' | 'tabs' | 'quotes' | 'number' | 'reverse' | 'dedupe') => void
  onInsertTitle: () => void
  onClearAllFormatting: () => void
  onMarginsPreset: (p: 'normal' | 'narrow' | 'moderate' | 'wide') => void
  onInsertCoverPage: (v: 1 | 2) => void
  onEmailLink: () => void
  onRemoveLinks: () => void
  onConvertTextTable: () => void
  onConvertTableText: () => void
  onSignatureLine: () => void
  onPageXofY: () => void
  showBoundaries: boolean; onToggleBoundaries: () => void
  showMarks: boolean; onToggleMarks: () => void
  pageNumFormatNode: React.ReactNode
  mode: 'edit' | 'read'; onMode: (m: 'edit' | 'read') => void
  showRuler: boolean; onToggleRuler: () => void; navOpen: boolean; onToggleNav: () => void
  onDetails: () => void
  spellOn: boolean; onToggleSpell: () => void; spellCount: number; onSpellDictionary: () => void
  onAddComment: () => void; onToggleComments: () => void; commentsOpen: boolean; commentCount: number
  onApplyStyle: (id: string) => void; onEditStyles: () => void; styleList: NamedStyle[]; curStyleId: string
  table: TableRibbonCtx | null
  onNew: () => void; onDuplicate: () => void; onPrint: () => void
  onExportPdf: () => void; onExportTxt: () => void; onExportServer: (fmt: 'docx' | 'odt') => void
  /** Connexion réseau : si false, l'export serveur (DOCX/ODT) est désactivé. */
  online: boolean
  pageColorNode: React.ReactNode
  watermarkNode: React.ReactNode
  pageBorderNode: React.ReactNode
  lineNumbersNode: React.ReactNode
  shapesNode: React.ReactNode
  hf: HFBarCtx | null; onHFField: (tok: string) => void; onHFSwitch: () => void
  onHFFirstPage: (v: boolean) => void; onHFLinked: (v: boolean) => void; onHFClose: () => void
  /** Plages de contenu des cellules sélectionnées (mise en forme multi-cellules). */
  cellRanges: Array<{ from: number; to: number }> | null
}

function buildDocumentRibbon(c: DocRibbonCtx): RibbonTab[] {
  const { t, fmt, body } = c
  const cr = c.cellRanges   // sélection de cellules de tableau (mise en forme groupée)
  const isA = (n: string, a?: Record<string, unknown>) => !!fmt?.isActive(n, a)
  const curSize = fmt?.getAttributes('textStyle').fontSize ? Math.round(parseFloat(String(fmt.getAttributes('textStyle').fontSize))) : 11
  const curFont = (fmt?.getAttributes('textStyle').fontFamily as string) || 'Arial'
  const setSize = (n: number) => fmt && applyInlineFormat(fmt, { fs: `${Math.max(6, Math.min(96, n))}pt` }, cr)
  const curLs = (fmt?.getAttributes('paragraph').lineHeight ?? fmt?.getAttributes('heading').lineHeight ?? 1.15) as number
  const setLs = (lh: number) => { if (!fmt) return; if (cr) applyParaAcross(fmt, cr, { lineHeight: lh }); else fmt.chain().focus().updateAttributes('paragraph', { lineHeight: lh }).updateAttributes('heading', { lineHeight: lh }).run() }
  const indent = (d: number) => { const cur = (fmt?.getAttributes('paragraph').indent ?? 0) as number; fmt?.chain().focus().updateAttributes('paragraph', { indent: Math.max(0, Math.min(10, cur + d)) }).updateAttributes('heading', { indent: Math.max(0, Math.min(10, cur + d)) }).run() }
  const tog = (id: string, icon: React.ReactNode, label: string, mark: string, key: 'b' | 'i' | 'u' | 's') =>
    ({ id, kind: 'toggle' as const, icon, tooltip: label, active: isA(mark), onClick: () => fmt && applyInlineFormat(fmt, { [key]: !isA(mark) }, cr) })
  const align = (a: string, icon: React.ReactNode, label: string) =>
    ({ id: 'al-' + a, kind: 'toggle' as const, icon, tooltip: label, active: !!fmt?.isActive({ textAlign: a }), onClick: () => { if (cr) applyParaAcross(fmt!, cr, { textAlign: a }); else fmt?.chain().focus().setTextAlign(a).run() } })
  // Trame de fond / encadré du paragraphe courant (ou des paragraphes sélectionnés).
  const curShading = (fmt?.getAttributes('paragraph').shading ?? fmt?.getAttributes('heading').shading) as string | undefined
  const curParaBorder = (fmt?.getAttributes('paragraph').paraBorder ?? fmt?.getAttributes('heading').paraBorder) as ParaBorderDef | undefined
  const paraAttrs = { ...(fmt?.getAttributes('paragraph') ?? {}), ...(fmt?.getAttributes('heading') ?? {}) } as Record<string, unknown>
  const spBefore = (paraAttrs.spaceBefore as number | null | undefined) ?? 0
  const spAfter  = (paraAttrs.spaceAfter as number | null | undefined) ?? 0
  const setParaAttr = (attrs: Record<string, unknown>) => {
    if (!fmt) return
    if (cr) { applyParaAcross(fmt, cr, attrs); return }
    const { from, to } = fmt.state.selection
    applyParaAcross(fmt, [{ from, to }], attrs)
  }

  const home: RibbonTab = {
    id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
    groups: [
      { id: 'file', label: t('doc_grp_file', { defaultValue: 'Fichier' }), items: [
        { id: 'new', kind: 'button', icon: <FileText size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: c.onNew },
        { id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: c.onDuplicate },
        { id: 'print', kind: 'button', icon: <Printer size={15} />, label: t('common_print'), onClick: c.onPrint },
        { id: 'export', kind: 'split', icon: <FileDown size={15} />, tooltip: t('doc_export', { defaultValue: 'Exporter' }),
          splitItems: [
            { id: 'epdf', kind: 'button', label: 'PDF', onClick: c.onExportPdf },
            { id: 'etxt', kind: 'button', label: 'TXT', onClick: c.onExportTxt },
            { id: 'edocx', kind: 'button', label: 'Word (DOCX)', disabled: !c.online,
              tooltip: c.online ? undefined : t('doc_export_offline', { defaultValue: 'Indisponible hors-ligne' }),
              onClick: () => c.onExportServer('docx') },
            { id: 'eodt', kind: 'button', label: 'OpenDocument (ODT)', disabled: !c.online,
              tooltip: c.online ? undefined : t('doc_export_offline', { defaultValue: 'Indisponible hors-ligne' }),
              onClick: () => c.onExportServer('odt') },
          ] },
      ] },
      { id: 'clip', label: t('doc_grp_clipboard', { defaultValue: 'Presse-papiers' }), items: [
        { id: 'paste', kind: 'button', size: 'large', icon: <ClipboardPaste size={22} />, label: t('common_paste'),
          onClick: async () => { try { const txt = await navigator.clipboard.readText(); body?.chain().focus().insertContent(txt).run() } catch { body?.view.focus(); document.execCommand('paste') } } },
        { id: 'cut', kind: 'button', icon: <Scissors size={15} />, label: t('common_cut'), onClick: () => { fmt?.view.focus(); document.execCommand('cut') } },
        { id: 'copy', kind: 'button', icon: <Copy size={15} />, label: t('common_copy'), onClick: () => { fmt?.view.focus(); document.execCommand('copy') } },
        { id: 'clear', kind: 'button', icon: <Eraser size={15} />, label: t('doc_clear_formatting'), onClick: () => fmt?.chain().focus().clearNodes().unsetAllMarks().run() },
      ] },
      { id: 'font', label: t('doc_grp_font', { defaultValue: 'Police' }), items: [
        { id: 'family', kind: 'custom', render: <FontPicker value={curFont} fonts={c.fonts} onChange={v => fmt && applyInlineFormat(fmt, { ff: v }, cr)} width={150} height={30} /> },
        { id: 'size', kind: 'dropdown', value: String(curSize), width: 56, options: [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72].map(s => ({ value: String(s), label: String(s) })), onChange: v => setSize(parseInt(v)) },
        { id: 'grow', kind: 'button', icon: <span style={{ fontSize: 15 }}>A</span>, tooltip: t('doc_increase_font_size'), onClick: () => setSize(curSize + 1) },
        { id: 'shrink', kind: 'button', icon: <span style={{ fontSize: 11 }}>A</span>, tooltip: t('doc_decrease_font_size'), onClick: () => setSize(curSize - 1) },
        { id: 'sep1', kind: 'separator' },
        tog('b', <Bold size={15} />, t('doc_bold'), 'bold', 'b'),
        tog('i', <Italic size={15} />, t('doc_italic'), 'italic', 'i'),
        tog('u', <UnderlineIcon size={15} />, t('doc_underline'), 'underline', 'u'),
        tog('s', <Strikethrough size={15} />, t('doc_strikethrough'), 'strike', 's'),
        { id: 'sup', kind: 'toggle', icon: <Superscript size={15} />, tooltip: t('doc_superscript', { defaultValue: 'Exposant' }), active: isA('superscript'), onClick: () => fmt?.chain().focus().toggleMark('superscript').run() },
        { id: 'sub', kind: 'toggle', icon: <Subscript size={15} />, tooltip: t('doc_subscript', { defaultValue: 'Indice' }), active: isA('subscript'), onClick: () => fmt?.chain().focus().toggleMark('subscript').run() },
        { id: 'case', kind: 'split', icon: <CaseSensitive size={16} />, tooltip: t('doc_change_case', { defaultValue: 'Modifier la casse' }),
          splitItems: ([['upper', t('doc_case_upper', { defaultValue: 'MAJUSCULES' })], ['lower', t('doc_case_lower', { defaultValue: 'minuscules' })], ['title', t('doc_case_title', { defaultValue: '1re Lettre De Chaque Mot' })], ['sentence', t('doc_case_sentence', { defaultValue: 'Casse de la phrase' })], ['toggle', t('doc_case_toggle', { defaultValue: 'iNVERSER LA cASSE' })]] as Array<[CaseMode, string]>).map(([m, lbl]) => ({ id: 'case-' + m, kind: 'button' as const, label: lbl, onClick: () => c.onChangeCase(m) })) },
        { id: 'sep2', kind: 'separator' },
        { id: 'color', kind: 'custom', render: <RibbonColorBtn editor={fmt} kind="text" cellRanges={cr} /> },
        { id: 'hl', kind: 'custom', render: <RibbonColorBtn editor={fmt} kind="highlight" cellRanges={cr} /> },
      ] },
      { id: 'para', label: t('doc_grp_paragraph', { defaultValue: 'Paragraphe' }), items: [
        { id: 'ul', kind: 'toggle', icon: <List size={15} />, tooltip: t('doc_bullet_list'), active: isA('bulletList'), onClick: () => fmt?.chain().focus().toggleBulletList().run() },
        { id: 'ol', kind: 'toggle', icon: <ListOrdered size={15} />, tooltip: t('doc_numbered_list'), active: isA('orderedList'), onClick: () => fmt?.chain().focus().toggleOrderedList().run() },
        { id: 'task', kind: 'toggle', icon: <CheckSquare size={15} />, tooltip: t('doc_task_list'), active: isA('taskList'), onClick: () => fmt?.chain().focus().toggleTaskList().run() },
        { id: 'ind-', kind: 'button', icon: <IndentDecrease size={15} />, tooltip: t('doc_decrease_indent'), onClick: () => indent(-1) },
        { id: 'ind+', kind: 'button', icon: <IndentIncrease size={15} />, tooltip: t('doc_increase_indent'), onClick: () => indent(1) },
        { id: 'ls', kind: 'split', icon: <SplitSquareVertical size={15} />, tooltip: t('doc_line_spacing'),
          splitItems: [
            ...[[t('doc_spacing_single'), 1.0], ['1,15', 1.15], ['1,5', 1.5], [t('doc_spacing_double'), 2.0], ['2,5', 2.5], ['3,0', 3.0]].map(([lbl, lh]) => ({ id: 'ls' + lh, kind: 'button' as const, label: String(lbl), active: curLs === lh, onClick: () => setLs(lh as number) })),
            { id: 'ls-sep', kind: 'separator' as const },
            { id: 'sp-before', kind: 'button' as const, label: spBefore > 0 ? t('doc_remove_space_before', { defaultValue: 'Supprimer l’espace avant' }) : t('doc_add_space_before', { defaultValue: 'Ajouter un espace avant' }), onClick: () => setParaAttr({ spaceBefore: spBefore > 0 ? 0 : 12 }) },
            { id: 'sp-after', kind: 'button' as const, label: spAfter > 0 ? t('doc_remove_space_after', { defaultValue: 'Supprimer l’espace après' }) : t('doc_add_space_after', { defaultValue: 'Ajouter un espace après' }), onClick: () => setParaAttr({ spaceAfter: spAfter > 0 ? 0 : 12 }) },
          ] },
        { id: 'sep3', kind: 'separator' },
        align('left', <AlignLeft size={15} />, t('doc_align_left')),
        align('center', <AlignCenter size={15} />, t('doc_align_center')),
        align('right', <AlignRight size={15} />, t('doc_align_right')),
        align('justify', <AlignJustify size={15} />, t('doc_align_justify')),
        { id: 'sep4', kind: 'separator' },
        { id: 'parashade', kind: 'custom', render: <RibbonParaShadeBtn shading={curShading} border={curParaBorder} onShading={col => setParaAttr({ shading: col })} onBorder={b => setParaAttr({ paraBorder: b })} /> },
        { id: 'sortasc', kind: 'button', icon: <ArrowDownAZ size={15} />, tooltip: t('doc_sort_asc', { defaultValue: 'Trier A → Z' }), onClick: () => c.onSortParas('asc') },
        { id: 'sortdesc', kind: 'button', icon: <ArrowUpAZ size={15} />, tooltip: t('doc_sort_desc', { defaultValue: 'Trier Z → A' }), onClick: () => c.onSortParas('desc') },
        { id: 'paraflow', kind: 'custom', render: <RibbonParaFlowBtn attrs={paraAttrs} onSet={setParaAttr} /> },
        { id: 'firstline', kind: 'toggle', icon: <Pilcrow size={14} className="opacity-70" />, tooltip: t('doc_first_line_indent', { defaultValue: 'Retrait de première ligne' }), active: ((paraAttrs.indentFirstLine as number) ?? 0) > 0, onClick: () => setParaAttr({ indentFirstLine: ((paraAttrs.indentFirstLine as number) ?? 0) > 0 ? 0 : 36 }) },
        { id: 'hanging', kind: 'toggle', icon: <IndentDecrease size={14} className="opacity-70" />, tooltip: t('doc_hanging_indent', { defaultValue: 'Retrait négatif (suspendu)' }), active: ((paraAttrs.indentFirstLine as number) ?? 0) < 0, onClick: () => { const on = ((paraAttrs.indentFirstLine as number) ?? 0) < 0; setParaAttr({ indentFirstLine: on ? 0 : -36, indentLeft: on ? ((paraAttrs.indentLeft as number) ?? 0) : Math.max(36, (paraAttrs.indentLeft as number) ?? 0) }) } },
      ] },
      { id: 'styles', label: t('doc_grp_styles', { defaultValue: 'Styles' }), items: [
        { id: 'style', kind: 'dropdown', value: c.curStyleId, width: 140,
          options: c.styleList.map(s => ({ value: s.id, label: styleLabel(s, t) })),
          onChange: v => c.onApplyStyle(v) },
        { id: 'editstyle', kind: 'button', icon: <Pencil size={15} />, tooltip: t('doc_edit_styles', { defaultValue: 'Modifier les styles' }), onClick: c.onEditStyles },
      ] },
      { id: 'edit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'link', kind: 'split', icon: <LinkIcon size={15} />, label: t('doc_insert_link'), active: isA('link'), onClick: c.onLink,
          splitItems: [
            { id: 'link-web', kind: 'button' as const, label: t('doc_insert_link', { defaultValue: 'Lien hypertexte' }), onClick: c.onLink },
            { id: 'link-mail', kind: 'button' as const, label: t('doc_email_link', { defaultValue: 'Lien e-mail' }), onClick: c.onEmailLink },
            { id: 'link-rm', kind: 'button' as const, label: t('doc_remove_links', { defaultValue: 'Supprimer tous les liens' }), onClick: c.onRemoveLinks },
          ] },
        { id: 'bookmark', kind: 'button', icon: <Bookmark size={15} />, label: t('doc_bookmark', { defaultValue: 'Signet' }), onClick: c.onInsertBookmark },
        { id: 'goto', kind: 'button', icon: <CornerDownRight size={15} />, label: t('doc_go_to', { defaultValue: 'Atteindre' }), onClick: c.onGoTo },
        { id: 'wordcount', kind: 'button', icon: <Hash size={15} />, label: t('doc_word_count', { defaultValue: 'Statistiques' }), onClick: c.onWordCount },
        { id: 'texttools', kind: 'split', icon: <Eraser size={15} />, tooltip: t('doc_text_tools', { defaultValue: 'Outils de texte' }),
          splitItems: [
            { id: 'tt-empties', kind: 'button' as const, label: t('doc_tt_empties', { defaultValue: 'Supprimer les lignes vides' }), onClick: () => c.onTextTool('empties') },
            { id: 'tt-spaces', kind: 'button' as const, label: t('doc_tt_spaces', { defaultValue: 'Réduire les espaces multiples' }), onClick: () => c.onTextTool('spaces') },
            { id: 'tt-tabs', kind: 'button' as const, label: t('doc_tt_tabs', { defaultValue: 'Tabulations → espaces' }), onClick: () => c.onTextTool('tabs') },
            { id: 'tt-quotes', kind: 'button' as const, label: t('doc_tt_quotes', { defaultValue: 'Guillemets typographiques' }), onClick: () => c.onTextTool('quotes') },
            { id: 'tt-sep0', kind: 'separator' as const },
            { id: 'tt-number', kind: 'button' as const, label: t('doc_tt_number', { defaultValue: 'Numéroter les paragraphes' }), onClick: () => c.onTextTool('number') },
            { id: 'tt-reverse', kind: 'button' as const, label: t('doc_tt_reverse', { defaultValue: 'Inverser l’ordre des paragraphes' }), onClick: () => c.onTextTool('reverse') },
            { id: 'tt-dedupe', kind: 'button' as const, label: t('doc_tt_dedupe', { defaultValue: 'Supprimer les paragraphes en double' }), onClick: () => c.onTextTool('dedupe') },
            { id: 'tt-sep', kind: 'separator' as const },
            { id: 'tt-title', kind: 'button' as const, label: t('doc_insert_title_text', { defaultValue: 'Insérer le titre du document' }), onClick: c.onInsertTitle },
            { id: 'tt-clear', kind: 'button' as const, label: t('doc_clear_all_fmt', { defaultValue: 'Effacer toute la mise en forme' }), onClick: c.onClearAllFormatting },
          ] },
        { id: 'details', kind: 'button', icon: <FileText size={15} />, label: t('doc_details', { defaultValue: 'Détails' }), onClick: c.onDetails },
      ] },
    ],
  }

  const insert: RibbonTab = {
    id: 'insert', label: t('doc_tab_insert', { defaultValue: 'Insertion' }),
    groups: [
      { id: 'pages', label: t('doc_grp_pages', { defaultValue: 'Pages' }), items: [
        { id: 'cover', kind: 'split', icon: <FileText size={15} />, tooltip: t('doc_cover_page', { defaultValue: 'Page de garde' }),
          splitItems: [
            { id: 'cover1', kind: 'button' as const, label: t('doc_cover_centered', { defaultValue: 'Centrée' }), onClick: () => c.onInsertCoverPage(1) },
            { id: 'cover2', kind: 'button' as const, label: t('doc_cover_left', { defaultValue: 'Alignée à gauche' }), onClick: () => c.onInsertCoverPage(2) },
          ] },
        { id: 'pb', kind: 'button', icon: <FileText size={15} />, label: t('doc_page_break'), onClick: c.onPageBreak },
        { id: 'sb', kind: 'button', icon: <SplitSquareVertical size={15} />, label: t('doc_section_break_next_page', { defaultValue: 'Saut de section' }), onClick: c.onSectionBreak },
      ] },
      { id: 'tables', label: t('doc_grp_tables', { defaultValue: 'Tableaux' }), items: [
        { id: 'table', kind: 'button', size: 'large', icon: <TableIcon size={22} />, label: t('doc_table', { defaultValue: 'Tableau' }), onClick: c.onInsertTable },
        { id: 'txt2tbl', kind: 'button', icon: <TableIcon size={14} />, label: t('doc_text_to_table', { defaultValue: 'Texte → tableau' }), onClick: c.onConvertTextTable },
        { id: 'tbl2txt', kind: 'button', icon: <WrapText size={14} />, label: t('doc_table_to_text', { defaultValue: 'Tableau → texte' }), onClick: c.onConvertTableText },
      ] },
      { id: 'illus', label: t('doc_grp_illustrations', { defaultValue: 'Illustrations' }), items: [
        { id: 'img', kind: 'button', size: 'large', icon: <ImageIcon size={22} />, label: t('doc_image', { defaultValue: 'Image' }), onClick: c.onUploadImage },
        { id: 'imgurl', kind: 'button', icon: <LinkIcon size={14} />, label: t('doc_insert_image_url', { defaultValue: 'Depuis une URL' }), onClick: c.onImageUrl },
        { id: 'shapes', kind: 'custom', render: c.shapesNode },
        { id: 'textbox', kind: 'button', icon: <Square size={14} />, label: t('doc_text_box', { defaultValue: 'Zone de texte' }), onClick: c.onInsertTextBox },
      ] },
      { id: 'hf', label: t('doc_grp_header_footer', { defaultValue: 'En-tête et pied' }), items: [
        { id: 'header', kind: 'button', icon: <PanelLeft size={15} className="rotate-90" />, label: t('doc_header', { defaultValue: 'En-tête' }), onClick: c.onSetHeader },
        { id: 'footer', kind: 'button', icon: <PanelLeft size={15} className="-rotate-90" />, label: t('doc_footer', { defaultValue: 'Pied de page' }), onClick: c.onSetFooter },
        { id: 'pgnum', kind: 'dropdown', value: c.pageNumbers, width: 120, options: ([['none', 'Aucun'], ['footer-right', 'Pied · droite'], ['footer-center', 'Pied · centre'], ['header-right', 'Haut · droite'], ['header-center', 'Haut · centre']] as Array<[PageNumbers, string]>).map(([v, l]) => ({ value: v, label: l })), onChange: v => c.onPageNumbers(v as PageNumbers) },
        { id: 'pgnumfmt', kind: 'custom', render: c.pageNumFormatNode },
      ] },
      { id: 'text', label: t('doc_grp_text', { defaultValue: 'Texte' }), items: [
        { id: 'toc', kind: 'button', icon: <ListTree size={15} />, label: t('doc_toc', { defaultValue: 'Table des matières' }), onClick: c.onInsertToc },
        { id: 'special', kind: 'button', icon: <Sigma size={15} />, label: t('doc_special_chars', { defaultValue: 'Caractères spéciaux' }), onClick: c.onSpecialChars },
        { id: 'datetime', kind: 'split', icon: <CalendarClock size={15} />, tooltip: t('doc_insert_datetime', { defaultValue: 'Date et heure' }),
          splitItems: ([['date', t('doc_field_date', { defaultValue: 'Date' })], ['time', t('doc_field_time', { defaultValue: 'Heure' })], ['datetime', t('doc_field_datetime', { defaultValue: 'Date et heure' })]] as Array<['date' | 'time' | 'datetime', string]>).map(([k, lbl]) => ({ id: 'fld-' + k, kind: 'button' as const, label: lbl, onClick: () => c.onInsertField(k) })) },
        { id: 'caption', kind: 'button', icon: <Quote size={15} />, label: t('doc_caption', { defaultValue: 'Légende' }), onClick: c.onInsertCaption },
        { id: 'hr', kind: 'button', icon: <Minus size={15} />, label: t('doc_horizontal_rule', { defaultValue: 'Trait horizontal' }), onClick: c.onInsertHr },
        { id: 'quote', kind: 'toggle', icon: <Quote size={15} />, label: t('doc_blockquote', { defaultValue: 'Citation' }), active: isA('blockquote'), onClick: () => fmt?.chain().focus().toggleBlockquote().run() },
        { id: 'code', kind: 'toggle', icon: <Hash size={15} />, label: t('doc_code_block', { defaultValue: 'Bloc de code' }), active: isA('codeBlock'), onClick: () => fmt?.chain().focus().toggleCodeBlock().run() },
        { id: 'sign', kind: 'button', icon: <Pencil size={15} />, label: t('doc_signature_line', { defaultValue: 'Ligne de signature' }), onClick: c.onSignatureLine },
        { id: 'pagexy', kind: 'button', icon: <Hash size={15} />, label: t('doc_page_x_of_y_btn', { defaultValue: 'Page X sur Y' }), onClick: c.onPageXofY },
        { id: 'symbols', kind: 'split', icon: <Omega size={15} />, tooltip: t('doc_symbols', { defaultValue: 'Symboles' }),
          splitItems: ([
            ['—', t('doc_sym_emdash', { defaultValue: 'Tiret cadratin' })], ['–', t('doc_sym_endash', { defaultValue: 'Tiret demi-cadratin' })],
            ['…', t('doc_sym_ellipsis', { defaultValue: 'Points de suspension' })], [' ', t('doc_sym_nbsp', { defaultValue: 'Espace insécable' })],
            ['« »', t('doc_sym_guillemets', { defaultValue: 'Guillemets français' })], ['€', t('doc_sym_euro', { defaultValue: 'Euro' })],
            ['™', t('doc_sym_tm', { defaultValue: 'Marque (™)' })], ['©', t('doc_sym_copy', { defaultValue: 'Copyright (©)' })], ['®', t('doc_sym_reg', { defaultValue: 'Marque déposée (®)' })],
            ['§', t('doc_sym_section', { defaultValue: 'Paragraphe (§)' })], ['•', t('doc_sym_bullet', { defaultValue: 'Puce (•)' })], ['°', t('doc_sym_degree', { defaultValue: 'Degré (°)' })],
            ['×', t('doc_sym_times', { defaultValue: 'Multiplié (×)' })], ['÷', t('doc_sym_div', { defaultValue: 'Divisé (÷)' })], ['±', t('doc_sym_pm', { defaultValue: 'Plus ou moins (±)' })],
            ['→', t('doc_sym_arrow', { defaultValue: 'Flèche droite (→)' })], ['≠', t('doc_sym_ne', { defaultValue: 'Différent (≠)' })], ['½', t('doc_sym_half', { defaultValue: 'Un demi (½)' })],
            ['≈', t('doc_sym_approx', { defaultValue: 'Environ égal (≈)' })], ['∞', t('doc_sym_inf', { defaultValue: 'Infini (∞)' })], ['√', t('doc_sym_sqrt', { defaultValue: 'Racine (√)' })],
            ['∑', t('doc_sym_sum', { defaultValue: 'Somme (∑)' })], ['π', t('doc_sym_pi', { defaultValue: 'Pi (π)' })], ['Δ', t('doc_sym_delta', { defaultValue: 'Delta (Δ)' })],
          ] as Array<[string, string]>).map(([ch, lbl]) => ({ id: 'sym-' + ch, kind: 'button' as const, label: `${ch}  ${lbl}`, onClick: () => fmt?.chain().focus().insertContent(ch).run() })) },
      ] },
    ],
  }

  const layout: RibbonTab = {
    id: 'layout', label: t('doc_tab_layout', { defaultValue: 'Mise en page' }),
    groups: [
      { id: 'setup', label: t('doc_grp_pagesetup', { defaultValue: 'Mise en page' }), items: [
        { id: 'margins', kind: 'split', icon: <SquareDashed size={15} />, tooltip: t('doc_margins', { defaultValue: 'Marges' }),
          splitItems: ([['normal', t('doc_margins_normal', { defaultValue: 'Normales' })], ['narrow', t('doc_margins_narrow', { defaultValue: 'Étroites' })], ['moderate', t('doc_margins_moderate', { defaultValue: 'Modérées' })], ['wide', t('doc_margins_wide', { defaultValue: 'Larges' })]] as Array<['normal' | 'narrow' | 'moderate' | 'wide', string]>).map(([p, lbl]) => ({ id: 'mg-' + p, kind: 'button' as const, label: lbl, onClick: () => c.onMarginsPreset(p) })) },
        { id: 'orient', kind: 'dropdown', value: c.orientation, width: 110, options: [{ value: 'portrait', label: t('doc_portrait', { defaultValue: 'Portrait' }) }, { value: 'landscape', label: t('doc_landscape', { defaultValue: 'Paysage' }) }], onChange: v => c.onOrientation(v as Orientation) },
        { id: 'paper', kind: 'dropdown', value: c.paperSize, width: 84, options: (['a4', 'a5', 'a3', 'letter', 'legal'] as PaperSize[]).map(p => ({ value: p, label: p.toUpperCase() })), onChange: v => c.onPaperSize(v as PaperSize) },
        { id: 'cols', kind: 'dropdown', value: String(c.columns), width: 96, options: [1, 2, 3].map(n => ({ value: String(n), label: `${n} ${n > 1 ? t('doc_columns', { defaultValue: 'colonnes' }) : t('doc_column', { defaultValue: 'colonne' })}` })), onChange: v => c.onColumns(parseInt(v)) },
        { id: 'linenums', kind: 'custom', render: c.lineNumbersNode },
      ] },
      { id: 'l-para', label: t('doc_grp_paragraph', { defaultValue: 'Paragraphe' }), items: [
        { id: 'parametrics', kind: 'custom', render: <RibbonParaMetricsBox attrs={paraAttrs} onSet={setParaAttr} /> },
      ] },
      { id: 'bg', label: t('doc_grp_background', { defaultValue: 'Arrière-plan' }), items: [
        { id: 'pagecolor', kind: 'custom', render: c.pageColorNode },
        { id: 'watermark', kind: 'custom', render: c.watermarkNode },
        { id: 'pageborder', kind: 'custom', render: c.pageBorderNode },
      ] },
    ],
  }

  const view: RibbonTab = {
    id: 'view', label: t('doc_tab_view', { defaultValue: 'Affichage' }),
    groups: [
      { id: 'modes', label: t('doc_grp_views', { defaultValue: 'Modes' }), items: [
        { id: 'edit', kind: 'toggle', size: 'large', icon: <FileText size={22} />, label: t('doc_mode_edit', { defaultValue: 'Édition' }), active: c.mode === 'edit', onClick: () => c.onMode('edit') },
        { id: 'read', kind: 'toggle', size: 'large', icon: <Eye size={22} />, label: t('doc_mode_read', { defaultValue: 'Lecture' }), active: c.mode === 'read', onClick: () => c.onMode('read') },
      ] },
      { id: 'show', label: t('doc_grp_show', { defaultValue: 'Afficher' }), items: [
        { id: 'ruler', kind: 'toggle', icon: <RulerIcon size={15} />, label: t('doc_ruler', { defaultValue: 'Règle' }), active: c.showRuler, onClick: c.onToggleRuler },
        { id: 'nav', kind: 'toggle', icon: <PanelLeft size={15} />, label: t('doc_nav_pane', { defaultValue: 'Volet de navigation' }), active: c.navOpen, onClick: c.onToggleNav },
        { id: 'bounds', kind: 'toggle', icon: <Frame size={15} />, label: t('doc_text_boundaries', { defaultValue: 'Limites du texte' }), active: c.showBoundaries, onClick: c.onToggleBoundaries },
        { id: 'marks', kind: 'toggle', icon: <Pilcrow size={15} />, label: t('doc_formatting_marks', { defaultValue: 'Marques ¶' }), active: c.showMarks, onClick: c.onToggleMarks },
      ] },
      { id: 'zoom', label: t('doc_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'zdlg', kind: 'button', size: 'large', icon: <ZoomIn size={22} />, label: t('doc_zoom', { defaultValue: 'Zoom' }), tooltip: t('doc_zoom_dialog', { defaultValue: 'Boîte de dialogue Zoom' }), onClick: c.onZoomDialog },
        { id: 'zout', kind: 'button', icon: <Minus size={15} />, tooltip: t('doc_zoom_out'), onClick: () => c.onZoom(Math.max(0.25, Math.round((c.zoom - 0.25) * 100) / 100)) },
        { id: 'zlvl', kind: 'dropdown', value: String(c.zoom), width: 70, options: ZOOM_PRESETS.map(p => ({ value: String(p), label: `${Math.round(p * 100)}%` })), onChange: v => c.onZoom(Number(v)) },
        { id: 'zin', kind: 'button', icon: <Plus size={15} />, tooltip: t('doc_zoom_in'), onClick: () => c.onZoom(Math.min(3, Math.round((c.zoom + 0.25) * 100) / 100)) },
        { id: 'z100', kind: 'button', icon: <span style={{ fontSize: 12, fontWeight: 600 }}>100%</span>, label: t('doc_zoom_100', { defaultValue: '100 %' }), onClick: () => c.onZoom(1) },
        { id: 'zpw', kind: 'button', icon: <MoveHorizontal size={15} />, label: t('doc_zoom_page_width', { defaultValue: 'Largeur de la page' }), onClick: () => c.onZoomFit('width') },
        { id: 'z1p', kind: 'button', icon: <FileText size={15} />, label: t('doc_zoom_one_page', { defaultValue: 'Une page' }), onClick: () => c.onZoomFit('page') },
        { id: 'zmp', kind: 'button', icon: <Files size={15} />, label: t('doc_zoom_multi_page', { defaultValue: 'Plusieurs pages' }), onClick: () => c.onZoomFit('multi') },
      ] },
    ],
  }

  // Onglet contextuel IMAGE (objet sélectionné dans le corps).
  const sel = body?.state.selection
  const imgSel = !!(sel && sel instanceof NodeSelection && sel.node.type.name === 'image')
  const imgAttr = (a: Record<string, unknown>) => body?.chain().focus().updateAttributes('image', a).run()
  const imageTab: RibbonTab = {
    id: 'ctx-image', label: t('doc_tab_image', { defaultValue: "Format de l'image" }), contextual: { accent: '#9334e6' }, visible: imgSel,
    groups: [
      { id: 'arrange', label: t('doc_grp_arrange', { defaultValue: 'Disposition' }), items: [
        { id: 'ial', kind: 'button', icon: <AlignLeft size={15} />, label: t('doc_align_left'), onClick: () => imgAttr({ align: 'left' }) },
        { id: 'iac', kind: 'button', icon: <AlignCenter size={15} />, label: t('doc_align_center'), onClick: () => imgAttr({ align: 'center' }) },
        { id: 'iar', kind: 'button', icon: <AlignRight size={15} />, label: t('doc_align_right'), onClick: () => imgAttr({ align: 'right' }) },
        { id: 'iwrap', kind: 'dropdown', value: (sel && sel instanceof NodeSelection ? (sel.node.attrs.wrap as string) : 'inline') || 'inline', width: 120, options: [['inline', 'Aligné texte'], ['square', 'Carré'], ['topBottom', 'Haut et bas'], ['behind', 'Derrière'], ['front', 'Devant']].map(([v, l]) => ({ value: v, label: l })), onChange: v => imgAttr({ wrap: v }) },
        { id: 'ireset', kind: 'button', icon: <RotateCcw size={15} />, label: t('doc_reset_image', { defaultValue: 'Réinitialiser' }), onClick: () => imgAttr({ width: 0, height: 0, rotation: 0 }) },
      ] },
    ],
  }

  // Onglet contextuel EN-TÊTE/PIED (mode édition HF actif).
  const hfTab: RibbonTab = {
    id: 'ctx-hf', label: t('doc_tab_hf', { defaultValue: 'En-tête et pied de page' }), contextual: { accent: '#1a73e8' }, visible: !!c.hf,
    groups: [
      { id: 'hffields', label: t('doc_grp_fields', { defaultValue: 'Champs' }), items: [
        { id: 'fp', kind: 'button', icon: <Hash size={15} />, label: t('doc_field_page', { defaultValue: 'N° de page' }), onClick: () => c.onHFField('{page}') },
        { id: 'fpp', kind: 'button', icon: <Hash size={15} />, label: t('doc_field_pages', { defaultValue: 'Nb pages' }), onClick: () => c.onHFField('{pages}') },
        { id: 'fd', kind: 'button', icon: <FileText size={15} />, label: t('doc_field_date', { defaultValue: 'Date' }), onClick: () => c.onHFField('{date}') },
        { id: 'ft', kind: 'button', icon: <FileText size={15} />, label: t('doc_field_title', { defaultValue: 'Titre' }), onClick: () => c.onHFField('{titre}') },
      ] },
      { id: 'hfnav', label: t('doc_grp_navigation', { defaultValue: 'Navigation' }), items: [
        { id: 'hfswitch', kind: 'button', size: 'large', icon: <SplitSquareVertical size={22} />, label: c.hf?.band === 'header' ? t('doc_goto_footer', { defaultValue: 'Aller au pied' }) : t('doc_goto_header', { defaultValue: "Aller à l'en-tête" }), onClick: c.onHFSwitch },
        { id: 'hffirst', kind: 'toggle', icon: <FileText size={15} />, label: t('doc_first_page_diff', { defaultValue: '1ʳᵉ page différente' }), active: !!c.hf?.firstPage, onClick: () => c.onHFFirstPage(!c.hf?.firstPage) },
        { id: 'hflink', kind: 'toggle', icon: <LinkIcon size={15} />, label: t('doc_link_previous', { defaultValue: 'Lier au précédent' }), disabled: !c.hf?.canLink, active: !!c.hf?.linked, onClick: () => c.onHFLinked(!c.hf?.linked) },
      ] },
      { id: 'hfclose', label: t('doc_grp_close', { defaultValue: 'Fermer' }), items: [
        { id: 'hfx', kind: 'button', size: 'large', icon: <X size={22} />, label: t('doc_close_hf', { defaultValue: 'Fermer' }), onClick: c.onHFClose },
      ] },
    ],
  }

  const review: RibbonTab = {
    id: 'review', label: t('doc_tab_review', { defaultValue: 'Révision' }),
    groups: [
      { id: 'proofing', label: t('doc_grp_proofing', { defaultValue: 'Vérification' }), items: [
        { id: 'spell', kind: 'toggle', size: 'large', icon: <SpellCheck size={22} />,
          label: c.spellOn ? `${t('doc_spell', { defaultValue: 'Orthographe' })}${c.spellCount ? ` (${c.spellCount})` : ''}` : t('doc_spell_off', { defaultValue: 'Désactivé' }),
          active: c.spellOn, onClick: c.onToggleSpell },
        { id: 'spelldict', kind: 'button', icon: <BookMarked size={15} />, label: t('doc_spell_dictionary', { defaultValue: 'Dictionnaire personnel' }), onClick: c.onSpellDictionary },
      ] },
      { id: 'comments', label: t('doc_grp_comments', { defaultValue: 'Commentaires' }), items: [
        { id: 'addcomment', kind: 'button', size: 'large', icon: <MessageSquarePlus size={22} />, label: t('doc_new_comment', { defaultValue: 'Nouveau commentaire' }), onClick: c.onAddComment },
        { id: 'showcomments', kind: 'toggle', icon: <MessageSquare size={15} />,
          label: `${t('doc_comments', { defaultValue: 'Commentaires' })}${c.commentCount ? ` (${c.commentCount})` : ''}`,
          active: c.commentsOpen, onClick: c.onToggleComments },
      ] },
    ],
  }

  // Onglet contextuel TABLEAU (curseur dans une cellule).
  const tableTab: RibbonTab = {
    id: 'ctx-table', label: t('doc_tab_table', { defaultValue: 'Tableau' }), contextual: { accent: '#00897b' }, visible: !!c.table,
    groups: c.table ? [
      { id: 'trows', label: t('doc_grp_rows_cols', { defaultValue: 'Lignes et colonnes' }), items: [
        { id: 'rabove', kind: 'button', icon: <Rows3 size={15} />, label: t('doc_insert_row_above', { defaultValue: 'Ligne au-dessus' }), onClick: c.table.onRowAbove },
        { id: 'rbelow', kind: 'button', icon: <Rows3 size={15} />, label: t('doc_insert_row_below', { defaultValue: 'Ligne en dessous' }), onClick: c.table.onRowBelow },
        { id: 'cleft', kind: 'button', icon: <Columns3 size={15} />, label: t('doc_insert_column_left', { defaultValue: 'Colonne à gauche' }), onClick: c.table.onColLeft },
        { id: 'cright', kind: 'button', icon: <Columns3 size={15} />, label: t('doc_insert_column_right', { defaultValue: 'Colonne à droite' }), onClick: c.table.onColRight },
        { id: 'sep', kind: 'separator' },
        { id: 'drow', kind: 'button', icon: <Trash2 size={15} />, label: t('doc_delete_row', { defaultValue: 'Supprimer la ligne' }), onClick: c.table.onDeleteRow },
        { id: 'dcol', kind: 'button', icon: <Trash2 size={15} />, label: t('doc_delete_column', { defaultValue: 'Supprimer la colonne' }), onClick: c.table.onDeleteCol },
      ] },
      { id: 'tmerge', label: t('doc_grp_merge', { defaultValue: 'Fusion' }), items: [
        { id: 'merge', kind: 'button', size: 'large', icon: <Combine size={22} />, label: t('doc_merge_cells', { defaultValue: 'Fusionner' }), disabled: !c.table.canMerge, onClick: c.table.onMerge },
        { id: 'split', kind: 'button', icon: <SplitSquareVertical size={15} />, label: t('doc_split_cell', { defaultValue: 'Fractionner' }), onClick: c.table.onSplit },
      ] },
      { id: 'tstyle', label: t('doc_grp_table_style', { defaultValue: 'Style' }), items: [
        { id: 'tst', kind: 'dropdown', value: c.table.curStyle, width: 130, options: ([['grid', 'Grille'], ['header', 'En-tête'], ['striped', 'Bandes'], ['plain', 'Sans bordure']] as Array<[string, string]>).map(([v, l]) => ({ value: v, label: t(`doc_tstyle_${v}`, { defaultValue: l }) })), onChange: v => c.table!.onStyle(v) },
        { id: 'cellcolor', kind: 'custom', render: c.table.cellColorNode },
        { id: 'dtable', kind: 'button', icon: <Trash2 size={15} />, label: t('doc_delete_table', { defaultValue: 'Supprimer le tableau' }), onClick: c.table.onDeleteTable },
      ] },
    ] : [],
  }

  // Onglet « Fichier » (Backstage façon Office) en 1ʳᵉ position.
  const file: RibbonTab = {
    id: 'file', label: t('doc_bs_file', { defaultValue: 'Fichier' }), groups: [],
    backstage: c.fileBackstage,
  }
  return [file, home, insert, layout, view, review, imageTab, hfTab, tableTab]
}

// Force le re-rendu sur changement d'état de l'éditeur → le ruban (rebâti à chaque
// rendu) reflète les états actifs (gras, alignement, police/taille courantes…).
// COALESCÉ via requestAnimationFrame : sans cela, maintenir une touche (autorépétition)
// déclenche un `setState` par transaction → cascade synchrone → React #185 « Maximum
// update depth ». On ne force au plus QU'UN re-rendu par frame.
function useEditorTick(editor: Editor | null) {
  const [, set] = useState(0)
  useEffect(() => {
    if (!editor) return
    let raf = 0
    const on = () => {
      if (raf) return
      raf = requestAnimationFrame(() => { raf = 0; set(n => (n + 1) & 0xffff) })
    }
    editor.on('transaction', on); editor.on('selectionUpdate', on)
    return () => { if (raf) cancelAnimationFrame(raf); editor.off('transaction', on); editor.off('selectionUpdate', on) }
  }, [editor])
}

// ── Commentaires (annotations) ──────────────────────────────────────────────
// Données d'un fil stockées dans une Y.Map collaborative `comments` du même Y.Doc
// (persistées par le service collab) ; l'ancrage au texte est porté par la marque
// `comment` (id), robuste aux éditions concurrentes.
interface CommentReply { author: string; authorId: string; text: string; createdAt: number }
interface CommentThread {
  id: string; author: string; authorId: string; text: string
  createdAt: number; resolved: boolean; replies: CommentReply[]; quote?: string
}

const newId = (): string =>
  (typeof crypto !== 'undefined' && crypto.randomUUID) ? crypto.randomUUID() : `c${Date.now()}-${Math.floor(Math.random() * 1e9)}`

// Retire la marque `comment` (id donné) sur toutes ses occurrences (résolution/suppression).
function unsetCommentMark(ed: Editor, id: string): void {
  const markType = ed.state.schema.marks.comment
  if (!markType) return
  const tr = ed.state.tr
  let found = false
  ed.state.doc.descendants((node, pos) => {
    if (node.isText && node.marks.some(m => m.type === markType && m.attrs.commentId === id)) {
      tr.removeMark(pos, pos + node.nodeSize, markType); found = true
    }
    return true
  })
  if (found) ed.view.dispatch(tr)
}

function relTime(ts: number, t: (k: string, o?: Record<string, unknown>) => string): string {
  const s = Math.max(0, Math.floor((Date.now() - ts) / 1000))
  if (s < 60) return t('doc_time_now', { defaultValue: "à l'instant" })
  const m = Math.floor(s / 60); if (m < 60) return t('doc_time_min', { defaultValue: '{{n}} min', n: m })
  const h = Math.floor(m / 60); if (h < 24) return t('doc_time_hour', { defaultValue: '{{n}} h', n: h })
  return new Date(ts).toLocaleDateString()
}

// Volet latéral des commentaires (façon Google Docs) : liste les fils, surligne
// l'actif, permet de répondre / résoudre / supprimer.
function CommentsPanel({ commentsMap, editor, opsRef, activeId, setActiveId, anchoredIds, user, onClose }: {
  commentsMap: Y.Map<CommentThread>
  editor: Editor | null
  opsRef: React.RefObject<PaginatedOps | null>
  activeId: string | null
  setActiveId: (id: string | null) => void
  anchoredIds: string[]
  user: { id: string; name: string }
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const [, tick] = useState(0)
  const [replyFor, setReplyFor] = useState<string | null>(null)
  const [replyText, setReplyText] = useState('')
  useEffect(() => {
    const fn = () => tick(n => (n + 1) & 0xffff)
    commentsMap.observe(fn)
    return () => commentsMap.unobserve(fn)
  }, [commentsMap])

  const threads = [...commentsMap.values()].filter(Boolean) as CommentThread[]
  // Tri : non résolus d'abord, par position d'ancrage si connue, sinon par date.
  const anchorIdx = (id: string) => { const i = anchoredIds.indexOf(id); return i < 0 ? 1e9 : i }
  threads.sort((a, b) => (Number(a.resolved) - Number(b.resolved)) || (anchorIdx(a.id) - anchorIdx(b.id)) || (a.createdAt - b.createdAt))

  const select = (th: CommentThread) => {
    setActiveId(th.id)
    const pos = opsRef.current?.commentAnchor(th.id)
    if (pos != null) opsRef.current?.scrollToPos(pos)
  }
  const resolve = (th: CommentThread) => {
    commentsMap.set(th.id, { ...th, resolved: !th.resolved })
    if (!th.resolved && editor) unsetCommentMark(editor, th.id)   // résolution → retire la surbrillance
    if (activeId === th.id) setActiveId(null)
  }
  const remove = (th: CommentThread) => {
    commentsMap.delete(th.id)
    if (editor) unsetCommentMark(editor, th.id)
    if (activeId === th.id) setActiveId(null)
  }
  const sendReply = (th: CommentThread) => {
    const text = replyText.trim(); if (!text) return
    commentsMap.set(th.id, { ...th, replies: [...(th.replies ?? []), { author: user.name, authorId: user.id, text, createdAt: Date.now() }] })
    setReplyText(''); setReplyFor(null)
  }

  return (
    <div className="flex-shrink-0 border-l border-border bg-surface-1 flex flex-col" style={{ width: 300 }}>
      <div className="flex items-center justify-between px-3 h-10 border-b border-border flex-shrink-0">
        <span className="text-sm font-semibold text-text-primary flex items-center gap-1.5"><MessageSquare size={15} /> {t('doc_comments', { defaultValue: 'Commentaires' })}</span>
        <button onClick={onClose} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><X size={15} /></button>
      </div>
      <div className="flex-1 overflow-auto p-2 flex flex-col gap-2">
        {!threads.length && <p className="text-xs text-text-tertiary text-center mt-6 px-3">{t('doc_no_comments', { defaultValue: 'Aucun commentaire. Sélectionnez du texte puis « Nouveau commentaire ».' })}</p>}
        {threads.map(th => {
          const orphan = anchoredIds.indexOf(th.id) < 0 && !th.resolved
          return (
            <div key={th.id} onClick={() => select(th)}
              className={`rounded-lg border p-2.5 cursor-pointer transition-colors ${activeId === th.id ? 'border-primary bg-primary/5' : 'border-border bg-white hover:border-text-tertiary'} ${th.resolved ? 'opacity-60' : ''}`}>
              {th.quote && <div className="text-[11px] text-text-tertiary border-l-2 border-warning pl-1.5 mb-1.5 line-clamp-2 italic">{th.quote}</div>}
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-text-primary truncate">{th.author}</span>
                <span className="text-[10px] text-text-tertiary flex-shrink-0">{relTime(th.createdAt, t)}</span>
              </div>
              <p className="text-xs text-text-secondary whitespace-pre-wrap break-words">{th.text}</p>
              {orphan && <p className="text-[10px] text-warning mt-1">{t('doc_comment_orphan', { defaultValue: 'Texte commenté supprimé' })}</p>}
              {(th.replies ?? []).map((r, i) => (
                <div key={i} className="mt-1.5 pl-2 border-l border-border">
                  <div className="flex items-center justify-between"><span className="text-[11px] font-semibold text-text-primary truncate">{r.author}</span><span className="text-[10px] text-text-tertiary">{relTime(r.createdAt, t)}</span></div>
                  <p className="text-xs text-text-secondary whitespace-pre-wrap break-words">{r.text}</p>
                </div>
              ))}
              {replyFor === th.id ? (
                <div className="mt-2 flex items-center gap-1" onClick={e => e.stopPropagation()}>
                  <input autoFocus value={replyText} onChange={e => setReplyText(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') sendReply(th); if (e.key === 'Escape') { setReplyFor(null); setReplyText('') } }}
                    placeholder={t('doc_reply_placeholder', { defaultValue: 'Répondre…' })}
                    className="flex-1 min-w-0 h-7 px-2 text-xs border border-border rounded outline-none focus:border-primary" />
                  <button onClick={() => sendReply(th)} className="p-1.5 rounded bg-primary text-white hover:bg-primary-hover"><Send size={12} /></button>
                </div>
              ) : (
                <div className="mt-2 flex items-center gap-1 text-text-secondary" onClick={e => e.stopPropagation()}>
                  <button onClick={() => { setReplyFor(th.id); setReplyText('') }} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2"><CornerDownRight size={12} /> {t('doc_reply', { defaultValue: 'Répondre' })}</button>
                  <button onClick={() => resolve(th)} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2"><Check size={12} /> {th.resolved ? t('doc_reopen', { defaultValue: 'Rouvrir' }) : t('doc_resolve', { defaultValue: 'Résoudre' })}</button>
                  <button onClick={() => remove(th)} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2 text-danger ml-auto"><Trash2 size={12} /></button>
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// Bouton « couleur de cellule » du ruban contextuel Tableau. L'application est
// déléguée (`onPick`) pour cibler la SÉLECTION de cellules (ou la cellule du curseur).
// Mélange linéaire de deux couleurs hex (t ∈ [0,1]) — pour générer les tons clairs/
// foncés d'une couleur de thème (façon Word).
function hexToRgb(h: string): [number, number, number] {
  let s = h.replace('#', ''); if (s.length === 3) s = s.split('').map(c => c + c).join('')
  const n = parseInt(s, 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}
function mixHex(a: string, b: string, t: number): string {
  const [r1, g1, b1] = hexToRgb(a), [r2, g2, b2] = hexToRgb(b)
  const ch = (x: number, y: number) => Math.max(0, Math.min(255, Math.round(x + (y - x) * t))).toString(16).padStart(2, '0')
  return `#${ch(r1, r2)}${ch(g1, g2)}${ch(b1, b2)}`
}
// Colonnes de couleurs de thème (Office) ; chacune déclinée en 6 tons (clair→foncé).
const SHADE_THEME = ['#ffffff', '#000000', '#e7e6e6', '#44546a', '#4472c4', '#ed7d31', '#a5a5a5', '#ffc000', '#5b9bd5', '#70ad47']
const SHADE_STANDARD = ['#c00000', '#ff0000', '#ffc000', '#ffff00', '#92d050', '#00b050', '#00b0f0', '#0070c0', '#002060', '#7030a0']
const themeTints = (base: string): string[] => [base, mixHex(base, '#ffffff', 0.4), mixHex(base, '#ffffff', 0.6), mixHex(base, '#ffffff', 0.8), mixHex(base, '#000000', 0.25), mixHex(base, '#000000', 0.5)]

// Trame de fond de cellule façon Word : nuancier (thème en dégradés + standard) +
// « Aucune couleur » (retire la trame) + couleur personnalisée. S'applique à la plage
// de cellules sélectionnée (le préventDefault garde la sélection au clic du bouton).
function RibbonCellColorBtn({ editor, onPick }: { editor: Editor | null; onPick: (hex: string | null) => void }) {
  const { t } = useTranslation('office')
  const pickerTheme = useAppPickerTheme()
  const ref = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const cur = (editor?.getAttributes('tableCell').cellBg as string) || null
  const pick = (hex: string | null) => { onPick(hex); setOpen(false) }
  const swatch = (hex: string) => (
    <button key={hex} onMouseDown={e => e.preventDefault()} onClick={() => pick(hex)} title={hex}
      className={`w-5 h-5 rounded-sm border hover:ring-2 hover:ring-accent ${cur === hex ? 'ring-2 ring-accent' : 'border-border/60'}`}
      style={{ background: hex }} />
  )
  return (
    <>
      <button ref={ref} onMouseDown={e => e.preventDefault()} onClick={() => setOpen(o => !o)}
        className="w-7 h-[22px] flex flex-col items-center justify-center rounded hover:bg-surface-2 text-text-secondary"
        title={t('doc_cell_shading', { defaultValue: 'Trame de fond' })}>
        <Paintbrush size={14} />
        <div className="w-4 h-1 rounded-sm border border-border/40" style={{ background: cur ?? 'transparent' }} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-3 w-[244px] flex flex-col gap-2 text-sm bg-white border border-border rounded-lg shadow-lg" data-module="office">
          <div className="font-medium text-text-primary">{t('doc_cell_shading', { defaultValue: 'Trame de fond' })}</div>
          <div className="text-[11px] text-text-tertiary">{t('doc_theme_colors', { defaultValue: 'Couleurs du thème' })}</div>
          <div className="flex gap-1">
            {SHADE_THEME.map(base => <div key={base} className="flex flex-col gap-1">{themeTints(base).map(swatch)}</div>)}
          </div>
          <div className="text-[11px] text-text-tertiary mt-1">{t('doc_standard_colors', { defaultValue: 'Couleurs standard' })}</div>
          <div className="flex gap-1">{SHADE_STANDARD.map(swatch)}</div>
          <div className="flex items-center justify-between gap-2 pt-2 mt-1 border-t border-border">
            <button onMouseDown={e => e.preventDefault()} onClick={() => pick(null)}
              className="px-2 py-1 rounded text-xs text-text-secondary hover:bg-hover border border-border">
              {t('doc_no_fill', { defaultValue: 'Aucune couleur' })}
            </button>
            <div className="flex items-center gap-1">
              <span className="text-[11px] text-text-tertiary">{t('doc_more_colors', { defaultValue: 'Autres' })}</span>
              <ColorField width={28} height={24} C={pickerTheme} color={cur ?? '#ffffff'} onChange={hex => onPick(hex)} />
            </div>
          </div>
        </div>
      </AnchoredPopover>
    </>
  )
}

// Dialogue d'édition des styles nommés (façon Word) : choisir un style et ajuster
// police/taille/couleur/gras/italique/alignement/espacement ; « Appliquer » met à
// jour la définition (persistée) et réapplique à tous les blocs portant ce style.
function StylesEditorDialog({ styles, initialId, onSave, onClose }: {
  styles: NamedStyle[]; initialId: string
  onSave: (id: string, meta: Partial<NamedStyleMeta>) => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const fonts = useAvailableFonts()
  const [sid, setSid] = useState(initialId)
  const base = styles.find(s => s.id === sid) || styles[0]
  const [draft, setDraft] = useState<NamedStyle>(base)
  useEffect(() => { const b = styles.find(s => s.id === sid); if (b) setDraft(b) }, [sid]) // eslint-disable-line react-hooks/exhaustive-deps
  const upd = (p: Partial<NamedStyle>) => setDraft(d => ({ ...d, ...p }))
  const save = () => {
    onSave(draft.id, { block: draft.block, level: draft.level, font: draft.font, size: draft.size, bold: draft.bold, italic: draft.italic, color: draft.color, align: draft.align, lineHeight: draft.lineHeight, spaceBefore: draft.spaceBefore, spaceAfter: draft.spaceAfter })
    onClose()
  }
  const label = (s: NamedStyle) => styleLabel(s, t)

  return (
    <FloatingWindow title={t('doc_edit_styles', { defaultValue: 'Modifier les styles' })} onClose={onClose} defaultWidth={480} backdrop>
      <div className="p-5" data-module="office">
        <div className="flex flex-col gap-3">
          <label className="flex items-center justify-between gap-3 text-sm">
            <span className="text-text-secondary">{t('doc_style', { defaultValue: 'Style' })}</span>
            <Dropdown value={sid} options={styles.map(s => ({ value: s.id, label: label(s) }))} onChange={setSid} width={240} />
          </label>
          {/* Aperçu */}
          <div className="border border-border rounded-lg p-3 bg-surface-1">
            <span style={{ fontFamily: draft.font, fontSize: (draft.size ?? 11) + 'pt', fontWeight: draft.bold ? 700 : 400, fontStyle: draft.italic ? 'italic' : 'normal', color: draft.color ?? '#202124', textAlign: draft.align, display: 'block' }}>
              {label(draft)} — {t('doc_style_preview', { defaultValue: 'Exemple de texte' })}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <label className="flex items-center justify-between gap-2 text-sm">
              <span className="text-text-secondary">{t('doc_grp_font', { defaultValue: 'Police' })}</span>
              <Dropdown value={draft.font ?? 'Arial'} options={fonts.map(f => ({ value: f, label: f }))} onChange={v => upd({ font: v })} width={130} />
            </label>
            <label className="flex items-center justify-between gap-2 text-sm">
              <span className="text-text-secondary">{t('doc_font_size', { defaultValue: 'Taille' })}</span>
              <input type="number" min={6} max={96} value={draft.size ?? 11} onChange={e => upd({ size: Number(e.target.value) || 11 })}
                className="w-20 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
            </label>
          </div>
          <div className="flex items-center gap-3">
            <button onClick={() => upd({ bold: !draft.bold })} className={`w-8 h-8 rounded border ${draft.bold ? 'bg-primary/15 border-primary text-primary' : 'border-border text-text-secondary'}`}><Bold size={15} className="mx-auto" /></button>
            <button onClick={() => upd({ italic: !draft.italic })} className={`w-8 h-8 rounded border ${draft.italic ? 'bg-primary/15 border-primary text-primary' : 'border-border text-text-secondary'}`}><Italic size={15} className="mx-auto" /></button>
            <div className="flex items-center gap-1.5 ml-1">
              {(['left', 'center', 'right', 'justify'] as const).map(a => (
                <button key={a} onClick={() => upd({ align: a })} className={`w-8 h-8 rounded border ${draft.align === a ? 'bg-primary/15 border-primary text-primary' : 'border-border text-text-secondary'}`}>
                  {a === 'left' ? <AlignLeft size={15} className="mx-auto" /> : a === 'center' ? <AlignCenter size={15} className="mx-auto" /> : a === 'right' ? <AlignRight size={15} className="mx-auto" /> : <AlignJustify size={15} className="mx-auto" />}
                </button>
              ))}
            </div>
            <label className="flex items-center gap-2 text-sm ml-auto">
              <span className="text-text-secondary">{t('doc_text_color', { defaultValue: 'Couleur' })}</span>
              <ColorField width={28} height={28} color={draft.color ?? '#202124'} onChange={hex => upd({ color: hex })} />
            </label>
          </div>
          <div className="grid grid-cols-3 gap-3">
            <label className="flex flex-col gap-1 text-xs text-text-secondary">
              {t('doc_line_spacing', { defaultValue: 'Interligne' })}
              <input type="number" step={0.05} min={1} max={3} value={draft.lineHeight ?? 1.15} onChange={e => upd({ lineHeight: Number(e.target.value) || 1.15 })}
                className="h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
            </label>
            <label className="flex flex-col gap-1 text-xs text-text-secondary">
              {t('doc_space_before', { defaultValue: 'Espace avant' })}
              <input type="number" min={0} max={64} value={draft.spaceBefore ?? 0} onChange={e => upd({ spaceBefore: Number(e.target.value) || 0 })}
                className="h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
            </label>
            <label className="flex flex-col gap-1 text-xs text-text-secondary">
              {t('doc_space_after', { defaultValue: 'Espace après' })}
              <input type="number" min={0} max={64} value={draft.spaceAfter ?? 0} onChange={e => upd({ spaceAfter: Number(e.target.value) || 0 })}
                className="h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
            </label>
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 mt-4">
          <Button variant="secondary" size="sm" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" size="sm" onClick={save}>{t('doc_apply', { defaultValue: 'Appliquer' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

// Dialogue de gestion du dictionnaire personnel (mots ajoutés via « Ajouter au
// dictionnaire ») : lister, retirer, ajouter manuellement. `onChange` redéclenche
// la vérification orthographique.
function SpellDictionaryDialog({ onChange, onClose }: { onChange: () => void; onClose: () => void }) {
  const { t } = useTranslation('office')
  const [, tick] = useState(0)
  const [draft, setDraft] = useState('')
  const words = personalDictionary()
  const remove = (w: string) => { unignoreWord(w); tick(n => n + 1); onChange() }
  const add = () => { const w = draft.trim(); if (!w) return; ignoreWord(w); setDraft(''); tick(n => n + 1); onChange() }
  return (
    <FloatingWindow title={t('doc_spell_dictionary', { defaultValue: 'Dictionnaire personnel' })} onClose={onClose} defaultWidth={420} backdrop>
      <div className="p-5" data-module="office">
        <div className="flex items-center gap-1.5 mb-3">
          <input value={draft} onChange={e => setDraft(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') add() }}
            placeholder={t('doc_spell_add_word', { defaultValue: 'Ajouter un mot…' })}
            className="flex-1 min-w-0 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
          <Button variant="primary" size="sm" onClick={add} disabled={!draft.trim()}>{t('doc_spell_add', { defaultValue: 'Ajouter' })}</Button>
        </div>
        <div className="max-h-72 overflow-auto border border-border rounded-lg divide-y divide-border">
          {!words.length && <p className="text-xs text-text-tertiary text-center py-6">{t('doc_spell_dict_empty', { defaultValue: 'Aucun mot ajouté.' })}</p>}
          {words.map(w => (
            <div key={w} className="flex items-center justify-between px-3 py-1.5">
              <span className="text-sm text-text-primary truncate">{w}</span>
              <button onClick={() => remove(w)} title={t('doc_spell_remove_word', { defaultValue: 'Retirer' })}
                className="p-1 rounded hover:bg-danger-light text-text-tertiary hover:text-danger flex-shrink-0"><Trash2 size={14} /></button>
            </div>
          ))}
        </div>
      </div>
    </FloatingWindow>
  )
}

// ── Barre de statut (façon Word) ────────────────────────────────────────────
// Gauche : « Page X sur Y », nombre de mots (et mots sélectionnés), correcteur,
// langue de vérification, état d'accessibilité. Droite : paramètres d'affichage,
// modes d'affichage (Page/Lecture), curseur de zoom. Tout est réel et fonctionnel.
const STATUS_LANG_LABELS: Record<string, string> = {
  fr: 'Français (France)', en: 'English (US)', es: 'Español (España)', de: 'Deutsch',
  it: 'Italiano', pt: 'Português', nl: 'Nederlands', pl: 'Polski', ru: 'Русский',
  ar: 'العربية', zh: '中文', ja: '日本語', ko: '한국어',
}

function StatusButton({ onClick, active, title, children }: {
  onClick?: (e: React.MouseEvent) => void; active?: boolean; title?: string; children: React.ReactNode
}) {
  return (
    <button type="button" title={title} onClick={onClick}
      className={`flex items-center gap-1 h-full px-2 rounded-none whitespace-nowrap transition-colors
        ${active ? 'text-primary bg-primary/10' : 'text-text-secondary hover:bg-black/5'}`}>
      {children}
    </button>
  )
}

function DocStatusBar({ editor, pages, current, zoom, onZoom, mode, onMode,
  spellOn, spellCount, onToggleSpell, onOpenSpell, showRuler, onToggleRuler,
  pageNumbers, onTogglePageNumbers }: {
  editor: Editor | null
  pages: number; current: number
  zoom: number; onZoom: (z: number) => void
  mode: 'edit' | 'read'; onMode: (m: 'edit' | 'read') => void
  spellOn: boolean; spellCount: number; onToggleSpell: () => void; onOpenSpell: () => void
  showRuler: boolean; onToggleRuler: () => void
  pageNumbers: PageNumbers; onTogglePageNumbers: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const [counts, setCounts]   = useState({ words: 0, selWords: 0 })
  const [a11y, setA11y]       = useState(0)
  const [settingsAt, setSettingsAt] = useState<{ top: number; left: number } | null>(null)

  // Comptage des mots — réactif (frappe + sélection).
  useEffect(() => {
    if (!editor) { setCounts({ words: 0, selWords: 0 }); return }
    const refresh = () => {
      const words = (editor.storage.characterCount as { words?: () => number })?.words?.() ?? 0
      const { from, to } = editor.state.selection
      let selWords = 0
      if (to > from) {
        const txt = editor.state.doc.textBetween(from, to, ' ', ' ').trim()
        selWords = txt ? txt.split(/\s+/).length : 0
      }
      setCounts(prev => (prev.words === words && prev.selWords === selWords) ? prev : { words, selWords })
    }
    refresh()
    editor.on('update', refresh); editor.on('selectionUpdate', refresh)
    return () => { editor.off('update', refresh); editor.off('selectionUpdate', refresh) }
  }, [editor])

  // Vérification d'accessibilité légère (débit) : images sans texte alternatif.
  // Les zones de texte riches (alt technique « kbtext… ») ne sont pas comptées.
  useEffect(() => {
    if (!editor) { setA11y(0); return }
    let timer: ReturnType<typeof setTimeout>
    const run = () => {
      let issues = 0
      editor.state.doc.descendants(node => {
        if (node.type.name === 'image') {
          const alt = String(node.attrs.alt ?? '')
          if (!alt.startsWith('kbtext') && !alt.trim()) issues++
        }
      })
      setA11y(issues)
    }
    const onUpdate = () => { clearTimeout(timer); timer = setTimeout(run, 700) }
    run()
    editor.on('update', onUpdate)
    return () => { clearTimeout(timer); editor.off('update', onUpdate) }
  }, [editor])

  const langLabel = STATUS_LANG_LABELS[(i18n.language || 'fr').slice(0, 2)] ?? (i18n.language || '').toUpperCase()
  const pct = Math.round(zoom * 100)
  const stepZoom = (d: number) => onZoom(Math.min(3, Math.max(0.25, Math.round((zoom + d) * 100) / 100)))

  const settingsItems: MenuItem[] = [
    { type: 'action', label: t('doc_show_ruler', { defaultValue: 'Règle' }), checked: showRuler, onClick: onToggleRuler },
    { type: 'action', label: t('doc_status_pagenums', { defaultValue: 'Numéros de page' }), checked: pageNumbers !== 'none', onClick: onTogglePageNumbers },
    { type: 'action', label: t('doc_spellcheck', { defaultValue: 'Correcteur orthographique' }), checked: spellOn, onClick: onToggleSpell },
  ]

  return (
    <div className="flex items-stretch h-7 flex-shrink-0 text-xs bg-[#f8f9fa] border-t border-[#dadce0] select-none"
         data-doc-statusbar>
      {/* ── Gauche ─────────────────────────────────────────────────────────── */}
      <StatusButton title={t('doc_status_goto_page', { defaultValue: 'Page actuelle' })}>
        {t('doc_status_page', { current, pages, defaultValue: `Page ${current} sur ${pages}` })}
      </StatusButton>
      <div className="w-px my-1.5 bg-[#dadce0]" />
      <StatusButton title={t('doc_status_words_title', { defaultValue: 'Nombre de mots' })}>
        {counts.selWords > 0
          ? t('doc_status_words_sel', { sel: counts.selWords, total: counts.words, defaultValue: `${counts.selWords} sur ${counts.words} mots` })
          : t('doc_status_words', { count: counts.words, defaultValue: `${counts.words} mots` })}
      </StatusButton>
      <div className="w-px my-1.5 bg-[#dadce0]" />
      <StatusButton active={spellCount > 0} onClick={onOpenSpell}
        title={spellCount > 0
          ? t('doc_status_spell_n', { count: spellCount, defaultValue: `${spellCount} faute(s) — ouvrir le correcteur` })
          : t('doc_status_spell_ok', { defaultValue: 'Aucune faute — dictionnaire personnel' })}>
        <SpellCheck size={14} />
        {spellCount > 0 && <span className="font-medium">{spellCount}</span>}
      </StatusButton>
      <StatusButton onClick={onOpenSpell} title={t('doc_status_lang_title', { defaultValue: 'Langue de vérification' })}>
        <Languages size={14} /> {langLabel}
      </StatusButton>
      <StatusButton active={a11y > 0}
        title={a11y > 0
          ? t('doc_status_a11y_issues', { count: a11y, defaultValue: `${a11y} image(s) sans texte alternatif` })
          : t('doc_status_a11y_ok', { defaultValue: 'Aucun problème d\'accessibilité détecté' })}>
        <Accessibility size={14} />
        <span className="hidden sm:inline">
          {a11y > 0
            ? t('doc_status_a11y_short', { count: a11y, defaultValue: `Accessibilité : ${a11y} à corriger` })
            : t('doc_status_a11y_done', { defaultValue: 'Accessibilité : vérification terminée' })}
        </span>
      </StatusButton>

      <div className="flex-1" />

      {/* ── Droite ─────────────────────────────────────────────────────────── */}
      <StatusButton onClick={e => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); setSettingsAt({ top: r.top, left: r.left }) }}
        title={t('doc_status_display', { defaultValue: 'Paramètres d\'affichage' })}>
        <SlidersHorizontal size={14} /> <span className="hidden md:inline">{t('doc_status_display', { defaultValue: 'Paramètres d\'affichage' })}</span>
      </StatusButton>
      <div className="w-px my-1.5 bg-[#dadce0]" />
      {/* Modes d'affichage */}
      <StatusButton active={mode === 'edit'} onClick={() => onMode('edit')}
        title={t('doc_status_mode_print', { defaultValue: 'Mode Page' })}><FileText size={14} /></StatusButton>
      <StatusButton active={mode === 'read'} onClick={() => onMode('read')}
        title={t('doc_status_mode_read', { defaultValue: 'Mode Lecture' })}><BookOpen size={14} /></StatusButton>
      <div className="w-px my-1.5 bg-[#dadce0]" />
      {/* Zoom */}
      <button type="button" onClick={() => stepZoom(-0.1)} title={t('doc_zoom_out', { defaultValue: 'Zoom arrière' })}
        className="flex items-center px-1.5 text-text-secondary hover:bg-black/5"><Minus size={14} /></button>
      <div className="flex items-center px-1">
        <RangeSlider min={50} max={200} step={10} value={Math.min(200, Math.max(50, pct))}
          onChange={v => onZoom(v / 100)}
          className="w-28" aria-label={`${t('doc_zoom_label', { defaultValue: 'Zoom' })} ${pct} %`} />
      </div>
      <button type="button" onClick={() => stepZoom(0.1)} title={t('doc_zoom_in', { defaultValue: 'Zoom avant' })}
        className="flex items-center px-1.5 text-text-secondary hover:bg-black/5"><Plus size={14} /></button>
      <button type="button" onClick={() => onZoom(1)} title={t('doc_status_zoom_reset', { defaultValue: 'Rétablir à 100 %' })}
        className="flex items-center px-2 text-text-secondary hover:bg-black/5 tabular-nums w-14 justify-center">{pct} %</button>

      {settingsAt && (
        <MenuDropdown items={settingsItems} pos={{ ...settingsAt, minWidth: 220 }} onClose={() => setSettingsAt(null)} />
      )}
    </div>
  )
}

function DocumentEditorArea({ docId }: { docId: string }) {
  const { t } = useTranslation('office')
  const navigate  = useNavigate()
  const location  = useLocation()
  const backPath  = (location.state as { from?: string } | null)?.from ?? '/office/documents'
  const { activeDoc, openDoc, saveDoc, trashDoc, starDoc, isSaving, createDoc, duplicateDoc } = useOfficeStore()
  // Onglet de ruban actif (CONTRÔLÉ) : permet à l'onglet « Fichier » (backstage) de
  // revenir à l'onglet précédent via sa flèche de retour.
  const [activeTab, setActiveTab] = useState('home')
  const prevTabRef = useRef('home')
  const handleTabChange = useCallback((id: string) => {
    setActiveTab(prev => { if (id === 'file' && prev !== 'file') prevTabRef.current = prev; return id })
  }, [])
  // Collaboration temps réel : un Y.Doc par document, relié au service collab du core.
  const ydoc = useMemo(() => new Y.Doc(), [docId])
  // Awareness Yjs : présence + curseurs des autres participants.
  const awareness = useMemo(() => new Awareness(ydoc), [ydoc])
  const authUser = useAuthStore(s => s.user)
  useEffect(() => {
    if (!authUser) return
    awareness.setLocalStateField('user', {
      id:     authUser.id,
      name:   authUser.display_name || authUser.username || authUser.email,
      color:  userColor(authUser.id),
      avatar: authUser.avatar_url,
    })
  }, [awareness, authUser])
  useEffect(() => () => awareness.destroy(), [awareness])
  const [collabEmpty, setCollabEmpty] = useState<boolean | null>(null)
  const [shareOpen, setShareOpen]     = useState(false)
  useEffect(() => { setCollabEmpty(null) }, [docId])
  useCollab(`office-document:${docId}`, ydoc, !!docId, { onSync: setCollabEmpty, awareness })
  // État réseau : les éditions hors-ligne sont persistées localement (y-indexeddb)
  // et fusionnées au retour. On désactive les opérations purement serveur (export
  // DOCX/ODT) tant qu'on est hors-ligne et on affiche un bandeau d'information.
  const [online, setOnline] = useState(() => (typeof navigator === 'undefined' ? true : navigator.onLine))
  useEffect(() => {
    const up = () => setOnline(true)
    const down = () => setOnline(false)
    window.addEventListener('online', up)
    window.addEventListener('offline', down)
    return () => { window.removeEventListener('online', up); window.removeEventListener('offline', down) }
  }, [])
  const [title, setTitle]                         = useState('')
  const [showRuler, setShowRuler]                 = useState(true)
  // Signal de focus de la barre de recherche du topbar (incrémenté par Ctrl+F).
  const [searchFocusTick, setSearchFocusTick]     = useState(0)
  const [specialOpen, setSpecialOpen]             = useState(false)
  const [pageNumbers, setPageNumbers]             = useState<PageNumbers>('none')
  const pageNumbersRef                            = useRef<PageNumbers>('none'); pageNumbersRef.current = pageNumbers
  const [mode, setMode]                           = useState<'edit' | 'read'>('edit')
  const [header, setHeader]                       = useState<HFContent>(emptyHF())
  const [footer, setFooter]                       = useState<HFContent>(emptyHF())
  const [hfFirstPage, setHfFirstPage]             = useState(false)
  const headerRef = useRef<HFContent>(emptyHF()); headerRef.current = header
  const footerRef = useRef<HFContent>(emptyHF()); footerRef.current = footer
  const hfFirstRef = useRef(false); hfFirstRef.current = hfFirstPage
  // Barre contextuelle d'édition inline en-tête/pied (null = mode inactif).
  const [hfBar, setHfBar]                          = useState<HFBarCtx | null>(null)
  const hfBarRef = useRef<HFBarCtx | null>(null); hfBarRef.current = hfBar
  // Éditeur de la bande en-tête/pied en cours d'édition → cible de la toolbar.
  const [hfZoneEditor, setHfZoneEditor]           = useState<Editor | null>(null)
  // Édition in-place d'une zone de texte riche → cible de la toolbar.
  const [tbBar, setTbBar]                         = useState(false)
  const [tbZoneEditor, setTbZoneEditor]           = useState<Editor | null>(null)
  const [paperSize, setPaperSize]                 = useState<PaperSize>('a4')
  const paperSizeRef = useRef<PaperSize>('a4'); paperSizeRef.current = paperSize
  const [navOpen, setNavOpen]                     = useState(false)
  const [spellOn, setSpellOn]                     = useState(true)
  const [spellCount, setSpellCount]               = useState(0)
  // Dictionnaire personnel : dialogue + version (bump → re-vérification).
  const [spellDictOpen, setSpellDictOpen]         = useState(false)
  const [spellVersion, setSpellVersion]           = useState(0)
  const [detailsOpen, setDetailsOpen]             = useState(false)
  // Vague « +50 » : aides visuelles + dialogues.
  const [showBoundaries, setShowBoundaries]       = useState(false)
  const [showMarks, setShowMarks]                 = useState(false)
  const [wordCountOpen, setWordCountOpen]         = useState(false)
  const [goToOpen, setGoToOpen]                   = useState(false)
  // Recherche : occurrences surlignées + index actif (remontés par FindReplaceBar).
  const [searchHi, setSearchHi]                   = useState<{ ranges: Array<{ from: number; to: number }>; active: number }>({ ranges: [], active: 0 })
  // Commentaires : Y.Map collaborative ; volet + commentaire actif + ids ancrés.
  const commentsMap = useMemo(() => ydoc.getMap<CommentThread>('comments'), [ydoc])
  const [commentsOpen, setCommentsOpen]           = useState(false)
  const [activeCommentId, setActiveCommentId]     = useState<string | null>(null)
  const [commentIds, setCommentIds]               = useState<string[]>([])
  // Sélection de plage de cellules (remontée par PaginatedEditor) → actions tableau.
  const [tableSel, setTableSel]                   = useState<(TableRect & { tableStart: number }) | null>(null)
  // Styles nommés : surcharges par document + dialogue d'édition.
  const [styleOverrides, setStyleOverrides]       = useState<Record<string, Partial<NamedStyleMeta>>>({})
  const styleOverridesRef = useRef(styleOverrides); styleOverridesRef.current = styleOverrides
  const [stylesEditorOpen, setStylesEditorOpen]   = useState(false)
  const [pageColor, setPageColor]                 = useState<string | undefined>(undefined)
  const [pageGrad,  setPageGrad]                  = useState<Gradient | undefined>(undefined)
  const pageColorRef = useRef<string | undefined>(undefined); pageColorRef.current = pageColor
  const pageGradRef  = useRef<Gradient | undefined>(undefined); pageGradRef.current = pageGrad
  const [watermark,  setWatermark]                = useState<WatermarkDef | null>(null)
  const [pageBorder, setPageBorder]               = useState<PageBorderDef | null>(null)
  const [lineNumbers, setLineNumbers]             = useState<LineNumbersDef | null>(null)
  const [pageNumFormat, setPageNumFormat]         = useState<PageNumFormat>('arabic')
  const [pageNumStart, setPageNumStart]           = useState(1)
  const pageNumFormatRef = useRef<PageNumFormat>('arabic'); pageNumFormatRef.current = pageNumFormat
  const pageNumStartRef  = useRef(1); pageNumStartRef.current = pageNumStart
  const watermarkRef  = useRef<WatermarkDef | null>(null); watermarkRef.current = watermark
  const pageBorderRef = useRef<PageBorderDef | null>(null); pageBorderRef.current = pageBorder
  const lineNumbersRef = useRef<LineNumbersDef | null>(null); lineNumbersRef.current = lineNumbers
  // CSS background appliqué à chaque page (dégradé prioritaire sur couleur unie).
  const pageBgCss = pageGrad ? gradientToCss(pageGrad) : pageColor
  const imageFileRef = useRef<HTMLInputElement>(null)
  const titleRef2 = useRef(''); titleRef2.current = title

  // Ctrl/⌘+F (rechercher) et Ctrl/⌘+H (remplacer) → focus de la barre de recherche
  // du topbar (qui surcharge la recherche du core quand un document est ouvert).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'h')) {
        e.preventDefault()
        setSearchFocusTick(n => n + 1)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const [zoom, setZoom]                           = useState(1)
  const [docStats, setDocStats]                   = useState({ pages: 1, current: 1 })
  // Retraits (px) du paragraphe du curseur → marqueurs de la règle horizontale (façon Word).
  const [paraIndents, setParaIndents]             = useState({ left: 0, first: 0, right: 0 })
  // Taquets de tabulation du paragraphe du curseur + type sélectionné (coin).
  const [paraTabs, setParaTabs]                   = useState<Array<{ pos: number; type: TabType }>>([])
  const [tabType, setTabType]                     = useState<TabType>('left')
  const [activeEditor, setActiveEditor]           = useState<Editor | null>(null)
  // activeOrientation = AFFICHAGE (section où est le curseur) ; baseOrientation =
  // section de base du document (persistée, passée à PaginatedEditor).
  const [activeOrientation, setActiveOrientation] = useState<Orientation>('portrait')
  const [baseOrientation, setBaseOrientation]     = useState<Orientation>('portrait')
  const [baseColumns, setBaseColumns]             = useState(1)
  const [activeMargins, setActiveMargins]         = useState<SectionDef['margins']>({ top: 96, right: 96, bottom: 96, left: 96 })
  const [dragGuide, setDragGuide]                 = useState<DragGuide>(null)
  const scrollRef                                 = useRef<HTMLDivElement>(null)
  const opsRef                                    = useRef<PaginatedOps | null>(null)
  const [zoomDialogOpen, setZoomDialogOpen]       = useState(false)
  // Mise en page avancée (dialogue « Mise en page », ouvert au double-clic sur les règles).
  const [pageSetupOpen, setPageSetupOpen]         = useState(false)
  const [gutter, setGutter]                       = useState(0)
  const [headerDist, setHeaderDist]               = useState(48)
  const [footerDist, setFooterDist]               = useState(48)
  const [vAlignPage, setVAlignPage]               = useState<'top' | 'center' | 'bottom' | 'both'>('top')
  const [sectionStart, setSectionStart]           = useState<'nextPage' | 'continuous' | 'evenPage' | 'oddPage'>('nextPage')
  const [evenOdd, setEvenOdd]                     = useState(false)

  // Zoom « ajuster à la fenêtre » : largeur de page · page entière · plusieurs pages.
  // Calcule depuis la taille du conteneur de défilement + la géométrie de la page.
  const fitZoom = useCallback((mode: 'width' | 'page' | 'multi') => {
    const sc = scrollRef.current, g = opsRef.current?.pageGeom()
    if (!sc || !g) return
    const availW = Math.max(100, sc.clientWidth - 48)            // marges latérales + barre de défilement
    const availH = Math.max(100, sc.clientHeight - 2 * CANVAS_PAD_Y)
    const z = mode === 'width' ? availW / g.pageW
            : mode === 'page'  ? Math.min(availW / g.pageW, availH / g.pageH)
            :                    availW / (2 * g.pageW + 24)   // 2 pages côte à côte (la grille enroule le reste)
    setZoom(Math.min(3, Math.max(0.25, Math.round(z * 100) / 100)))
  }, [])

  // ── Ctrl/⌘ + molette → zoom du document (au lieu du zoom navigateur) ─────────
  // Écouteur `wheel` NON-PASSIF (sinon `preventDefault` est ignoré et le navigateur
  // zoome la page entière). Zoom centré sur le curseur : on garde le point du
  // document sous le pointeur fixe en ajustant le défilement après le re-layout.
  const zoomLiveRef = useRef(zoom); zoomLiveRef.current = zoom
  useEffect(() => {
    const el = scrollRef.current; if (!el) return
    const onWheel = (e: WheelEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return   // molette seule = défilement normal
      e.preventDefault()
      const old = zoomLiveRef.current
      const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY   // lignes → px
      const nz = Math.min(3, Math.max(0.25, Math.round(old * Math.exp(-dy * 0.0015) * 100) / 100))
      if (nz === old) return
      // Point du document (coords non zoomées) actuellement sous le curseur.
      const rect = el.getBoundingClientRect()
      const ox = e.clientX - rect.left, oy = e.clientY - rect.top
      const ux = (el.scrollLeft + ox) / old
      const uy = (el.scrollTop + oy) / old
      setZoom(nz)
      // Après re-layout (canvas redimensionnés au nouveau zoom), recaler le défilement
      // pour que le même point reste sous le pointeur.
      requestAnimationFrame(() => {
        el.scrollLeft = ux * nz - ox
        el.scrollTop = uy * nz - oy
      })
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
    // Re-attache une fois le scroller monté (le rendu de chargement précède le doc).
  }, [activeDoc?.id, docId])

  // ── Persistance unifiée (contenu + mise en page) ──────────────────────────
  // Un seul format sauvegardé : l'enveloppe multi-page { sections, pages }. Les
  // refs gardent la dernière valeur de chaque source (doc, marges, orientation)
  // pour qu'un save de marge ne piétine pas le contenu et inversement.
  const sectionIdRef    = useRef<string>(newSectionId())
  const pageIdRef       = useRef<string>(newPageId())
  const docRef          = useRef<JSONContent>(emptyDoc())
  const activeEditorRef = useRef<Editor | null>(null)

  const onPickImageFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    e.target.value = ''   // permet de re-sélectionner le même fichier
    const ed = activeEditorRef.current
    if (!file || !ed) return
    const reader = new FileReader()
    reader.onload = () => {
      const src = reader.result as string
      ed.chain().focus().insertContent([{ type: 'image', attrs: { src } }, { type: 'paragraph' }]).run()
    }
    reader.readAsDataURL(file)
  }

  const marginsRef        = useRef(activeMargins);   marginsRef.current = activeMargins
  const baseOrientationRef = useRef(baseOrientation); baseOrientationRef.current = baseOrientation
  const baseColumnsRef = useRef(baseColumns); baseColumnsRef.current = baseColumns
  const gutterRef = useRef(gutter); gutterRef.current = gutter
  const headerDistRef = useRef(headerDist); headerDistRef.current = headerDist
  const footerDistRef = useRef(footerDist); footerDistRef.current = footerDist
  const vAlignPageRef = useRef(vAlignPage); vAlignPageRef.current = vAlignPage
  const sectionStartRef = useRef(sectionStart); sectionStartRef.current = sectionStart
  const saveTimerRef    = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const doSave = useCallback(() => {
    const ed      = activeEditorRef.current
    const content = ed ? ed.getJSON() : docRef.current
    const sec: SectionDef  = { id: sectionIdRef.current, orientation: baseOrientationRef.current, margins: marginsRef.current, columns: baseColumnsRef.current, gutter: gutterRef.current, headerDist: headerDistRef.current, footerDist: footerDistRef.current, vAlign: vAlignPageRef.current, sectionStart: sectionStartRef.current }
    const page: PageData   = { id: pageIdRef.current, sectionId: sectionIdRef.current, content }
    saveDoc(docId, { content_json: serializeDoc([sec], [page], { pageNumbers: pageNumbersRef.current, header: headerRef.current, footer: footerRef.current, hfFirstPage: hfFirstRef.current, pageColor: pageColorRef.current, pageGrad: pageGradRef.current, paperSize: paperSizeRef.current, styles: Object.keys(styleOverridesRef.current).length ? styleOverridesRef.current : undefined, watermark: watermarkRef.current, pageBorder: pageBorderRef.current, lineNumbers: lineNumbersRef.current, pageNumFormat: pageNumFormatRef.current, pageNumStart: pageNumStartRef.current }) })
  }, [docId, saveDoc])

  const scheduleSave = useCallback(() => {
    clearTimeout(saveTimerRef.current)
    saveTimerRef.current = setTimeout(doSave, 700)
  }, [doSave])

  // Vide la sauvegarde différée (avant de quitter / masquer / démonter).
  const flushSave = useCallback(() => {
    if (saveTimerRef.current) { clearTimeout(saveTimerRef.current); saveTimerRef.current = undefined; doSave() }
  }, [doSave])

  useEffect(() => {
    const onHide = () => { if (document.visibilityState === 'hidden') flushSave() }
    document.addEventListener('visibilitychange', onHide)
    window.addEventListener('pagehide', flushSave)
    window.addEventListener('beforeunload', flushSave)
    return () => {
      document.removeEventListener('visibilitychange', onHide)
      window.removeEventListener('pagehide', flushSave)
      window.removeEventListener('beforeunload', flushSave)
      flushSave()
    }
  }, [flushSave])

  // La reliure (gutter) s'ajoute au bord intérieur (gauche) — réduit la zone de texte.
  const geoMargins = gutter ? { ...activeMargins, left: activeMargins.left + gutter } : activeMargins
  const activeGeo = getGeometry({ id: '', orientation: activeOrientation, margins: geoMargins, columns: baseColumns }, paperSize)
  // Position de la page ACTIVE dans le contenu défilable (repère des règles). Recalculée
  // à chaque changement de page courante / zoom → les règles suivent la page active,
  // y compris en disposition GRILLE (où les pages ne sont pas empilées verticalement).
  const [activePageBox, setActivePageBox] = useState<{ left: number; top: number } | null>(null)
  useLayoutEffect(() => {
    // Page ACTIVE = page du curseur, page 1 par défaut (docStats.current vaut 1 au départ).
    const b = opsRef.current?.pageContentBox(Math.max(0, docStats.current - 1))
    // Une fois une page calée, on ne repasse JAMAIS à null (sinon flash de centrage pendant
    // un reflow) → il y a toujours une page « sélectionnée ». `docStats.pages`/`activeEditor` :
    // au premier rendu la pagination n'est pas prête → recalcul dès qu'elle l'est (sinon en
    // grille la règle resterait centrée au lieu d'être sur la page 1).
    if (b) setActivePageBox({ left: b.left, top: b.top })
  }, [docStats.current, docStats.pages, activeEditor, zoom, activeGeo.pageW, activeGeo.pageH])

  useEffect(() => { openDoc(docId) }, [docId, openDoc])
  // Retraits du paragraphe du curseur → état pour les marqueurs de la règle horizontale.
  useEffect(() => {
    const ed = activeEditor; if (!ed) return
    const read = () => {
      const a = ed.state.selection.$from.parent.attrs as Record<string, unknown>
      const next = { left: Number(a.indentLeft) || 0, first: Number(a.indentFirstLine) || 0, right: Number(a.indentRight) || 0 }
      setParaIndents(prev => (prev.left === next.left && prev.first === next.first && prev.right === next.right) ? prev : next)
      const tabs = Array.isArray(a.tabStops) ? (a.tabStops as Array<{ pos: number; type: TabType }>) : []
      setParaTabs(prev => JSON.stringify(prev) === JSON.stringify(tabs) ? prev : tabs)
    }
    read()
    ed.on('selectionUpdate', read); ed.on('update', read)
    return () => { ed.off('selectionUpdate', read); ed.off('update', read) }
  }, [activeEditor])
  // Ajoute / retire un taquet sur le paragraphe du curseur (commit historique).
  const setParaTabStops = useCallback((tabs: Array<{ pos: number; type: TabType }>) => {
    const ed = activeEditorRef.current; if (!ed) return
    const sorted = [...tabs].sort((a, b) => a.pos - b.pos)
    ed.chain().focus()
      .updateAttributes('paragraph', { tabStops: sorted.length ? sorted : null })
      .updateAttributes('heading', { tabStops: sorted.length ? sorted : null })
      .run()
  }, [])
  // Applique des retraits (px) au paragraphe/titre du curseur. `commit=false` (pendant le
  // glisser) ne crée PAS d'entrée d'historique ; le dernier appel (mouseUp) commite.
  const setParaIndentAttrs = useCallback((patch: { indentLeft?: number; indentFirstLine?: number; indentRight?: number }, commit = true) => {
    const ed = activeEditorRef.current; if (!ed) return
    const norm: Record<string, number | null> = {}
    for (const k of Object.keys(patch) as (keyof typeof patch)[]) norm[k] = patch[k] ? Math.round(patch[k]!) : null
    let chain = ed.chain().focus()
    if (!commit) chain = chain.setMeta('addToHistory', false)
    chain.updateAttributes('paragraph', norm).updateAttributes('heading', norm).run()
  }, [])
  // Mode lecture : éditeur en lecture seule (barre d'outils/règles masquées au rendu).
  useEffect(() => { activeEditor?.setEditable(mode === 'edit') }, [activeEditor, mode])
  useEffect(() => {
    if (!activeDoc) return
    setTitle(activeDoc.title)
    // Initialise margins, orientation et ids stables depuis le document stocké.
    const { sections, pages, pageNumbers: pn, header: hdr, footer: ftr, hfFirstPage: hf1, pageColor: pc, pageGrad: pg, paperSize: ps, styles: stl, watermark: wmk, pageBorder: pbd, lineNumbers: lnm, pageNumFormat: pnf, pageNumStart: pns } = parseDocContent(activeDoc.content_json as object | null)
    setStyleOverrides(stl ?? {}); styleOverridesRef.current = stl ?? {}
    setWatermark(wmk ?? null); watermarkRef.current = wmk ?? null
    setPageBorder(pbd ?? null); pageBorderRef.current = pbd ?? null
    setLineNumbers(lnm ?? null); lineNumbersRef.current = lnm ?? null
    setPageNumFormat(pnf ?? 'arabic'); pageNumFormatRef.current = pnf ?? 'arabic'
    setPageNumStart(pns ?? 1); pageNumStartRef.current = pns ?? 1
    setHeader(hdr); headerRef.current = hdr
    setFooter(ftr); footerRef.current = ftr
    setHfFirstPage(!!hf1); hfFirstRef.current = !!hf1
    setPaperSize(ps ?? 'a4'); paperSizeRef.current = ps ?? 'a4'
    setPageColor(pc); pageColorRef.current = pc
    setPageGrad(pg);  pageGradRef.current = pg
    if (sections[0]) {
      const s0 = sections[0]
      setActiveMargins(s0.margins)
      setActiveOrientation(s0.orientation)
      setBaseOrientation(s0.orientation)
      setBaseColumns(s0.columns ?? 1); baseColumnsRef.current = s0.columns ?? 1
      sectionIdRef.current = s0.id
      // Mise en page avancée (importée du DOCX / sauvegarde).
      setGutter(s0.gutter ?? 0)
      setHeaderDist(s0.headerDist ?? 48)
      setFooterDist(s0.footerDist ?? 48)
      setVAlignPage(s0.vAlign ?? 'top')
      setSectionStart(s0.sectionStart ?? 'nextPage')
    }
    if (pages[0]) pageIdRef.current = pages[0].id
    pageNumbersRef.current = pn
    setPageNumbers(pn)
    docRef.current = flattenToDoc(activeDoc.content_json as object | null)
  }, [activeDoc?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleTitleBlur = useCallback(() => {
    if (title !== activeDoc?.title) saveDoc(docId, { title })
  }, [title, activeDoc, docId, saveDoc])


  // Cible la section du curseur (base → état du document ; sinon → nœud break).
  const handleOrientationChange = (o: Orientation) => opsRef.current?.setOrientation(o)

  // Changement de la section de BASE (depuis PaginatedEditor) : met à jour l'état
  // persistant + l'affichage, puis sauvegarde.
  const handleBaseChange = useCallback((patch: { orientation?: Orientation; columns?: number }) => {
    if (patch.orientation) {
      baseOrientationRef.current = patch.orientation
      setBaseOrientation(patch.orientation)
      setActiveOrientation(patch.orientation)
    }
    if (patch.columns != null) {
      baseColumnsRef.current = patch.columns
      setBaseColumns(patch.columns)
    }
    scheduleSave()
  }, [scheduleSave])

  const registerOps = useCallback((ops: PaginatedOps | null) => { opsRef.current = ops }, [])
  const handleInsertSectionBreak = useCallback(() => opsRef.current?.insertBreak(), [])
  const handleInsertPageBreak = useCallback(() => opsRef.current?.insertPageBreak(), [])
  const handleColumnsChange = useCallback((n: number) => opsRef.current?.setColumns(n), [])
  const handleSetPageNumbers = useCallback((pn: PageNumbers) => {
    pageNumbersRef.current = pn
    setPageNumbers(pn)
    scheduleSave()
  }, [scheduleSave])
  // Édition Word : dialog 3 zones (gauche/centre/droite) + champs dynamiques +
  // « première page différente » + liaison à la section précédente. Ouvert depuis
  // le menu Insertion OU par double-clic dans la marge haute/basse.
  // Édition INLINE (façon Word) : le menu entre dans le mode, la barre contextuelle
  // pilote les options. Plus de formulaire modal.
  const handleSetHeader = useCallback(() => opsRef.current?.enterHF('header'), [])
  const handleSetFooter = useCallback(() => opsRef.current?.enterHF('footer'), [])
  // Écriture des zones depuis l'édition inline (frappe dans les bandes) : route
  // vers la section du curseur si déliée, sinon vers l'en-tête/pied de BASE.
  const commitHF = useCallback((kind: 'header' | 'footer', zones: HFContent) => {
    const res = opsRef.current?.setSectionHF(kind, zones, hfBarRef.current?.linked ?? true)
    if (!res || res.applyBase) {
      if (kind === 'header') { headerRef.current = zones; setHeader(zones) }
      else                   { footerRef.current = zones; setFooter(zones) }
    }
    scheduleSave()
  }, [scheduleSave])
  // Options de la barre contextuelle : 1ʳᵉ page différente + liaison de section.
  const setHfFirstPageOpt = useCallback((v: boolean) => {
    hfFirstRef.current = v; setHfFirstPage(v); scheduleSave()
  }, [scheduleSave])
  const setHfLinkedOpt = useCallback((linked: boolean) => {
    const band = hfBarRef.current?.band ?? 'header'
    const zones = band === 'header' ? headerRef.current : footerRef.current
    opsRef.current?.setSectionHF(band, zones, linked)
    setHfBar(b => b ? { ...b, linked } : b)
    scheduleSave()
  }, [scheduleSave])

  const handlePaperSize = useCallback((p: PaperSize) => {
    paperSizeRef.current = p
    setPaperSize(p)
    scheduleSave()
  }, [scheduleSave])

  // ── Exports client (PDF via canvas hors écran, TXT via texte brut PM) ───────
  const handleExportPdf = useCallback(() => {
    const ops = opsRef.current; if (!ops) return
    const pages = ops.exportPageCanvases(2)
    if (!pages.length) return
    const blob = pagesToPdf(pages.map(p => ({ canvas: p.canvas, wPx: p.wPx, hPx: p.hPx })), titleRef2.current)
    downloadBlob(blob, `${titleRef2.current || 'document'}.pdf`)
  }, [])

  // Impression : le document est rendu sur <canvas> (zoom écran, scroll) qui
  // s'imprime mal directement. On génère le MÊME PDF que l'export, puis on
  // l'imprime via un iframe caché → pagination fidèle, fond blanc, sans chrome.
  const handlePrint = useCallback(() => {
    const ops = opsRef.current
    const pages = ops?.exportPageCanvases(2) ?? []
    if (!pages.length) { window.print(); return }   // repli
    const blob = pagesToPdf(pages.map(p => ({ canvas: p.canvas, wPx: p.wPx, hPx: p.hPx })), titleRef2.current)
    const url = URL.createObjectURL(blob)
    const iframe = document.createElement('iframe')
    iframe.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0'
    iframe.src = url
    iframe.onload = () => {
      try { iframe.contentWindow?.focus(); iframe.contentWindow?.print() }
      catch { window.open(url, '_blank') }
      setTimeout(() => { iframe.remove(); URL.revokeObjectURL(url) }, 60_000)
    }
    document.body.appendChild(iframe)
  }, [])

  // Ctrl/Cmd+P → impression PDF fidèle (sinon le navigateur imprimerait le canvas).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'p') { e.preventDefault(); handlePrint() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [handlePrint])
  const handleExportTxt = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    const text = ed.getText({ blockSeparator: '\n' })
    downloadBlob(new Blob([text], { type: 'text/plain;charset=utf-8' }), `${titleRef2.current || 'document'}.txt`)
  }, [])
  // Export serveur (DOCX/ODT) : téléchargement AUTHENTIFIÉ via axios (les anciennes
  // navigations location.href vers /api/v1/documents/... étaient un 404 + sans token).
  const handleExportServer = useCallback(async (fmt: 'docx' | 'odt') => {
    // Opération purement serveur : indisponible hors-ligne (garde-fou en plus de
    // la désactivation du bouton dans le ruban).
    if (typeof navigator !== 'undefined' && !navigator.onLine) return
    try {
      const r = await api.get(`/office/documents/${docId}/export/${fmt}`, { responseType: 'blob' })
      downloadBlob(r.data as Blob, `${titleRef2.current || 'document'}.${fmt}`)
    } catch (e) { console.error('export', fmt, e) }
  }, [docId])

  // ── Table des matières : titres + numéro de page, insérée au curseur ────────
  const handleInsertToc = useCallback(() => {
    const ed = activeEditorRef.current, ops = opsRef.current
    if (!ed || !ops) return
    const items = ops.outline()
    const content: JSONContent[] = [
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: t('doc_toc', { defaultValue: 'Table des matières' }) }] },
      ...(items.length
        ? items.map(it => ({
            type: 'paragraph',
            attrs: { indent: Math.max(0, it.level - 1) },
            content: [
              { type: 'text', text: it.text },
              { type: 'text', text: ` — ${it.page}`, marks: [{ type: 'textStyle', attrs: {} }] },
            ],
          }))
        : [{ type: 'paragraph', content: [{ type: 'text', text: t('doc_toc_empty', { defaultValue: 'Aucun titre dans le document.' }) }] }]),
      { type: 'paragraph' },
    ]
    ed.chain().focus().insertContent(content).run()
  }, [t])

  // ── Formes & zones de texte ─────────────────────────────────────────────────
  const handleInsertShape = useCallback((kind: ShapeKind) => {
    const ed = activeEditorRef.current; if (!ed) return
    const { w, h } = shapeDefaultSize(kind)
    // Les traits/connecteurs n'ont pas de remplissage : seul le contour porte la couleur.
    const isStroke = kind === 'line' || kind === 'lineArrow' || kind === 'lineDouble' || kind === 'elbowConnector' || kind === 'curveConnector' || kind === 'curve'
    const params: ShapeParams = { kind, fill: isStroke ? 'none' : '#dbe7ff', stroke: '#1a73e8' }
    const src = svgToDataUrl(shapeSvg(kind, w, h, params.fill, params.stroke))
    ed.chain().focus().insertContent([
      { type: 'image', attrs: { src, width: w, height: h, align: 'center', alt: shapeAlt(params) } },
      { type: 'paragraph' },
    ]).run()
  }, [])
  // Insère une zone de texte riche (canvas) et entre directement en édition in-place.
  const handleInsertTextBox = useCallback(() => { opsRef.current?.insertTextBox() }, [])

  // ── Commentaires ────────────────────────────────────────────────────────────
  // Nouveau commentaire : exige une sélection, applique la marque `comment` puis
  // crée le fil dans la Y.Map (collaboratif). Ouvre le volet et active le fil.
  const handleAddComment = useCallback(async () => {
    const ed = activeEditorRef.current; if (!ed) return
    const sel = ed.state.selection
    if (sel.empty) { await prompt({ title: t('doc_comment_select_first', { defaultValue: "Sélectionnez d'abord le texte à commenter." }), allowEmpty: true, confirmLabel: t('common_ok', { defaultValue: 'OK' }) }); return }
    const text = await prompt({ title: t('doc_new_comment', { defaultValue: 'Nouveau commentaire' }), placeholder: t('doc_comment_placeholder', { defaultValue: 'Votre commentaire…' }), confirmLabel: t('doc_comment_add', { defaultValue: 'Commenter' }) })
    if (!text || !text.trim()) return
    const id = newId()
    const quote = ed.state.doc.textBetween(sel.from, sel.to, ' ').slice(0, 160)
    ed.chain().focus().setMark('comment', { commentId: id }).run()
    commentsMap.set(id, { id, author: authUser?.display_name || authUser?.username || authUser?.email || 'Anonyme', authorId: authUser?.id || '', text: text.trim(), createdAt: Date.now(), resolved: false, replies: [], quote })
    setCommentsOpen(true); setActiveCommentId(id)
  }, [commentsMap, authUser, t])

  // Activation d'un commentaire (clic sur texte commenté) → ouvre le volet.
  const handleCommentActivate = useCallback((id: string | null) => {
    setActiveCommentId(id)
    if (id) setCommentsOpen(true)
  }, [])

  // ── Styles nommés ───────────────────────────────────────────────────────────
  const styleList = useMemo(() => mergeStyles(styleOverrides), [styleOverrides])
  const handleApplyStyle = useCallback((id: string) => {
    const ed = activeEditorRef.current; const s = styleList.find(x => x.id === id)
    if (!ed || !s) return
    applyNamedStyle(ed, s); scheduleSave()
  }, [styleList, scheduleSave])
  // Sauvegarde d'une définition de style (dialogue) : surcharge persistée + réapplication.
  const handleSaveStyle = useCallback((id: string, meta: Partial<NamedStyleMeta>) => {
    const next = { ...styleOverridesRef.current, [id]: { ...styleOverridesRef.current[id], ...meta } }
    styleOverridesRef.current = next
    setStyleOverrides(next)
    const ed = activeEditorRef.current
    if (ed) { const s = mergeStyles(next).find(x => x.id === id); if (s) reapplyStyle(ed, s) }
    scheduleSave()
  }, [scheduleSave])

  // ── Tableaux ────────────────────────────────────────────────────────────────
  const tableOp = useCallback((fn: (ed: Editor) => void) => {
    const ed = activeEditorRef.current; if (!ed) return
    fn(ed); scheduleSave()
  }, [scheduleSave])

  const handleNew = async () => {
    const doc = await createDoc()
    navigate(`/office/documents/${doc.id}`)
  }

  const handleDuplicate = useCallback(async () => {
    const d = await duplicateDoc(docId)
    navigate(`/office/documents/${d.id}`)
  }, [duplicateDoc, docId, navigate])

  // Éditeur ciblé par la mise en forme du ruban : bande HF / zone de texte si en édition, sinon le corps.
  const fmtEditor = (tbBar && tbZoneEditor ? tbZoneEditor : hfBar && hfZoneEditor ? hfZoneEditor : activeEditor) as Editor | null
  useEditorTick(fmtEditor)
  useEditorTick(activeEditor as Editor | null)
  const ribbonFonts = useAvailableFonts()

  // Onglet « Fichier » (backstage façon Office) : sections Accueil + Informations +
  // Exporter + Imprimer + Fermer (appelé AVANT le gate de chargement → ordre des hooks stable).
  const backstageSections = useDocumentsBackstageSections(activeDoc ? {
    title,
    pages: opsRef.current?.pageCount() ?? 1,
    words: (activeEditorRef.current?.storage.characterCount as { words?: () => number } | undefined)?.words?.() ?? 0,
    chars: (activeEditorRef.current?.storage.characterCount as { characters?: () => number } | undefined)?.characters?.() ?? 0,
    createdAt: activeDoc.created_at,
    updatedAt: activeDoc.updated_at,
    onPrint: handlePrint,
    onExportPdf: handleExportPdf,
    onExportTxt: handleExportTxt,
    onExportServer: handleExportServer,
    onClose: () => navigate('/office/documents'),
  } : undefined)
  const fileBackstage = <Backstage sections={backstageSections} theme={WORKSPACE_OFFICE} onBack={() => setActiveTab(prevTabRef.current)} />

  if (!activeDoc || activeDoc.id !== docId) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[#f1f3f4] text-text-tertiary text-sm">
        {t('common_loading')}
      </div>
    )
  }

  const onPageColorHex = (hex: string) => {
    if (opsRef.current?.setSectionBg(hex)) { scheduleSave(); return }
    setPageColor(hex); setPageGrad(undefined); pageColorRef.current = hex; pageGradRef.current = undefined; scheduleSave()
  }
  const onPageGradient = (g: Gradient) => { setPageGrad(g); setPageColor(undefined); pageGradRef.current = g; pageColorRef.current = undefined; scheduleSave() }
  const onWatermarkChange  = (wm: WatermarkDef | null)  => { setWatermark(wm);  watermarkRef.current = wm;  scheduleSave() }
  const onPageBorderChange = (pb: PageBorderDef | null) => { setPageBorder(pb); pageBorderRef.current = pb; scheduleSave() }
  const onLineNumbersChange = (ln: LineNumbersDef | null) => { setLineNumbers(ln); lineNumbersRef.current = ln; scheduleSave() }
  // ── Vague « +50 » : handlers ────────────────────────────────────────────────
  const handleChangeCase = (m: CaseMode) => { const ed = fmtEditor; if (ed) applyCaseTransform(ed, m) }
  const handleSortParas = (d: 'asc' | 'desc') => { const ed = activeEditorRef.current; if (ed) sortParagraphs(ed, d) }
  const handleInsertField = (k: 'date' | 'time' | 'datetime') => {
    const ed = fmtEditor; if (!ed) return
    ed.chain().focus().insertContent(nowFieldText(k, i18n.language)).run()
  }
  const handleInsertHr = () => { const ed = fmtEditor; if (ed) ed.chain().focus().setHorizontalRule().run() }
  const handleInsertCaption = () => {
    const ed = activeEditorRef.current; if (!ed) return
    let max = 0
    ed.state.doc.descendants(n => { const m = (n.isTextblock ? n.textContent : '').match(/^Figure\s+(\d+)/i); if (m) max = Math.max(max, +m[1]) })
    ed.chain().focus().insertContent([
      { type: 'paragraph', attrs: { textAlign: 'center' }, content: [{ type: 'text', marks: [{ type: 'italic' }, { type: 'textStyle', attrs: { fontSize: '9pt' } }], text: `Figure ${max + 1} : ` }] },
    ]).run()
  }
  const handleInsertBookmark = async () => {
    const ed = fmtEditor; if (!ed) return
    const name = await prompt({ title: t('doc_bookmark', { defaultValue: 'Signet' }), message: t('doc_bookmark_name', { defaultValue: 'Nom du signet' }), confirmLabel: t('doc_insert', { defaultValue: 'Insérer' }) })
    if (!name) return
    const { from, to } = ed.state.selection
    if (from === to) ed.chain().focus().insertContent({ type: 'text', text: '​', marks: [{ type: 'bookmark', attrs: { name } }] }).run()
    else ed.chain().focus().setMark('bookmark', { name }).run()
  }
  const handleTextTool = (k: 'empties' | 'spaces' | 'tabs' | 'quotes' | 'number' | 'reverse' | 'dedupe') => {
    const ed = activeEditorRef.current; if (!ed) return
    if (k === 'empties') removeEmptyParagraphs(ed)
    else if (k === 'spaces') transformTextNodes(ed, s => s.replace(/ {2,}/g, ' '))
    else if (k === 'tabs') transformTextNodes(ed, s => s.replace(/\t/g, '    '))
    else if (k === 'quotes') transformTextNodes(ed, smartQuotes)
    else if (k === 'number') numberParagraphs(ed)
    else if (k === 'reverse') reverseParagraphs(ed)
    else if (k === 'dedupe') dedupeParagraphs(ed)
  }
  const handleInsertTitle = () => { const ed = fmtEditor; if (ed && title) ed.chain().focus().insertContent(title).run() }
  const handleClearAllFormatting = () => {
    const ed = activeEditorRef.current; if (!ed) return
    ed.chain().focus().selectAll().unsetAllMarks().clearNodes().run()
  }
  const handleMarginsPreset = (preset: 'normal' | 'narrow' | 'moderate' | 'wide') => {
    const m = preset === 'narrow' ? { top: 48, right: 48, bottom: 48, left: 48 }
      : preset === 'moderate' ? { top: 96, right: 72, bottom: 96, left: 72 }
      : preset === 'wide' ? { top: 96, right: 192, bottom: 96, left: 192 }
      : { top: 96, right: 96, bottom: 96, left: 96 }
    setActiveMargins(m); marginsRef.current = m; scheduleSave()
  }
  const handleEmailLink = async () => {
    const ed = fmtEditor; if (!ed) return
    const addr = await prompt({ title: t('doc_email_link', { defaultValue: 'Lien e-mail' }), placeholder: 'nom@exemple.com', confirmLabel: t('doc_apply', { defaultValue: 'Appliquer' }) })
    if (!addr) return
    ed.chain().focus().extendMarkRange('link').setLink({ href: `mailto:${addr}` }).run()
  }
  const handleRemoveAllLinks = () => { const ed = activeEditorRef.current; if (ed) ed.chain().focus().selectAll().unsetLink().run() }
  const handleConvertTextTable = () => { const ed = activeEditorRef.current; if (ed) textToTable(ed) }
  const handleConvertTableText = () => { const ed = activeEditorRef.current; if (ed) tableToText(ed) }
  const handleSignatureLine = () => {
    const ed = fmtEditor; if (!ed) return
    ed.chain().focus().insertContent([
      { type: 'paragraph', attrs: { spaceBefore: 36 }, content: [{ type: 'text', text: '________________________' }] },
      { type: 'paragraph', content: [{ type: 'text', marks: [{ type: 'textStyle', attrs: { fontSize: '9pt', color: '#5f6368' } }], text: t('doc_signature', { defaultValue: 'Signature' }) }] },
    ]).run()
  }
  const handleInsertPageXofY = () => {
    const ed = fmtEditor; if (!ed) return
    const total = opsRef.current?.pageCount() ?? 1
    ed.chain().focus().insertContent(t('doc_page_x_of_y', { defaultValue: `Page ${docStats.current} sur ${total}`, x: docStats.current, y: total })).run()
  }
  const handleInsertCoverPage = (variant: 1 | 2) => {
    const ed = activeEditorRef.current; if (!ed) return
    const blocks: JSONContent[] = variant === 1
      ? [
          { type: 'paragraph', attrs: { spaceBefore: 120 } },
          { type: 'heading', attrs: { level: 1, textAlign: 'center' }, content: [{ type: 'text', text: title || t('doc_cover_title', { defaultValue: 'Titre du document' }) }] },
          { type: 'paragraph', attrs: { textAlign: 'center', spaceBefore: 8 }, content: [{ type: 'text', marks: [{ type: 'italic' }, { type: 'textStyle', attrs: { fontSize: '14pt' } }], text: t('doc_cover_subtitle', { defaultValue: 'Sous-titre' }) }] },
          { type: 'paragraph', attrs: { textAlign: 'center', spaceBefore: 160 }, content: [{ type: 'text', text: new Date().toLocaleDateString(i18n.language, { day: 'numeric', month: 'long', year: 'numeric' }) }] },
          { type: 'pageBreak' },
        ]
      : [
          { type: 'paragraph', attrs: { spaceBefore: 200 } },
          { type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: title || t('doc_cover_title', { defaultValue: 'Titre du document' }) }] },
          { type: 'paragraph', content: [{ type: 'text', marks: [{ type: 'textStyle', attrs: { color: '#5f6368' } }], text: t('doc_cover_subtitle', { defaultValue: 'Sous-titre' }) }] },
          { type: 'horizontalRule' },
          { type: 'paragraph', content: [{ type: 'text', marks: [{ type: 'bold' }], text: `${t('doc_cover_author', { defaultValue: 'Auteur' })} · ` }, { type: 'text', text: new Date().toLocaleDateString(i18n.language) }] },
          { type: 'pageBreak' },
        ]
    ed.chain().focus().insertContentAt(0, blocks).run()
  }

  // Style nommé courant : attr styleName du bloc, sinon déduit du niveau de titre.
  const curStyleId = (() => {
    const ed = fmtEditor
    if (!ed) return 'normal'
    const sn = (ed.getAttributes('paragraph').styleName ?? ed.getAttributes('heading').styleName) as string | undefined
    if (sn) return sn
    for (let lvl = 1; lvl <= 4; lvl++) if (ed.isActive('heading', { level: lvl })) return `heading${lvl}`
    return 'normal'
  })()

  // Contexte du ruban contextuel « Tableau » (présent si le curseur est dans un tableau).
  // Les actions opèrent sur la SÉLECTION de cellules si elle existe, sinon sur la
  // cellule du curseur (rect 1×1 en coordonnées de grille).
  const bodyEd = activeEditor as Editor | null
  const rectFor = (ed: Editor): TableRect | null => {
    if (tableSel) return tableSel
    const c = tableCtxOf(ed)
    return c ? { r0: c.rowIndex, r1: c.rowIndex, c0: c.colStart, c1: c.colStart } : null
  }
  const opRect = (apply: (ed: Editor, rect: TableRect) => void) => tableOp(ed => { const r = rectFor(ed); if (r) apply(ed, r) })
  const tableCtx: TableRibbonCtx | null = isInTable(bodyEd) ? {
    onRowAbove: () => opRect((ed, r) => insertRowAt(ed, r.r0)),
    onRowBelow: () => opRect((ed, r) => insertRowAt(ed, r.r1 + 1)),
    onColLeft:  () => opRect((ed, r) => insertColAt(ed, r.c0)),
    onColRight: () => opRect((ed, r) => insertColAt(ed, r.c1 + 1)),
    onDeleteRow: () => opRect((ed, r) => deleteRowsRange(ed, r.r0, r.r1)),
    onDeleteCol: () => opRect((ed, r) => deleteColsRange(ed, r.c0, r.c1)),
    onDeleteTable: () => tableOp(ed => { const c = tableCtxOf(ed); if (c) ed.chain().focus().deleteRange({ from: c.tablePos, to: c.tablePos + c.tableNode.nodeSize }).run() }),
    onMerge: () => opRect((ed, r) => { if (r.r1 > r.r0 || r.c1 > r.c0) mergeRect(ed, r) }),
    onSplit: () => tableOp(splitCell),
    canMerge: !!tableSel && (tableSel.r1 > tableSel.r0 || tableSel.c1 > tableSel.c0),
    onStyle: (s: string) => tableOp(ed => ed.chain().focus().updateAttributes('table', { tableStyle: s }).run()),
    curStyle: (bodyEd?.getAttributes('table').tableStyle as string) || 'grid',
    cellColorNode: <RibbonCellColorBtn editor={bodyEd} onPick={hex => opRect((ed, r) => setCellsBg(ed, r, hex))} />,
  } : null

  // Plages de contenu des cellules sélectionnées → mise en forme groupée (gras,
  // police, couleur, alignement…) appliquée à toute la sélection.
  const cellRanges = tableSel && bodyEd ? cellContentRanges(bodyEd, tableSel.tableStart, tableSel) : null

  // Macro API (`Kubuno` global) exposed to user macros. Built on demand so each run
  // reads the current editor state. Acts on the active TipTap editor (focused zone).
  const makeApi = () => {
    const ed = activeEditorRef.current
    const Doc = {
      /** Plain-text content of the document. */
      getText: () => ed?.getText() ?? '',
      /** HTML serialization of the document. */
      getHTML: () => ed?.getHTML() ?? '',
      /** Word count (TipTap CharacterCount, falling back to a text split). */
      getWordCount: () => {
        const w = (ed?.storage.characterCount as { words?: () => number } | undefined)?.words?.()
        if (typeof w === 'number') return w
        const txt = ed?.getText() ?? ''
        return txt.trim() ? txt.trim().split(/\s+/).length : 0
      },
      /** Insert text at the current caret position. */
      insertText: (text: unknown) => { ed?.chain().focus().insertContent(String(text)).run() },
      /** Replace the whole document with the given HTML. */
      setContent: (html: unknown) => { ed?.commands.setContent(String(html)) },
    }
    const App = {
      getType: () => 'document',
      getId: () => docId,
      toast: (m: unknown) => console.log(String(m)),
      log: (m: unknown) => console.log(String(m)),
    }
    return { Doc, App }
  }

  const ribbon = buildDocumentRibbon({
    t, fmt: fmtEditor, body: activeEditor as Editor | null, fonts: ribbonFonts, fileBackstage,
    zoom, onZoom: setZoom, onZoomFit: fitZoom, onZoomDialog: () => setZoomDialogOpen(true),
    orientation: activeOrientation, onOrientation: handleOrientationChange,
    columns: baseColumns, onColumns: handleColumnsChange,
    paperSize, onPaperSize: handlePaperSize,
    pageNumbers, onPageNumbers: handleSetPageNumbers,
    onPageBreak: handleInsertPageBreak, onSectionBreak: handleInsertSectionBreak,
    onUploadImage: () => imageFileRef.current?.click(),
    onImageUrl: async () => {
      const url = await prompt({ title: t('doc_insert_image'), message: t('doc_image_url'), placeholder: 'https://exemple.com/image.png', confirmLabel: t('doc_insert') })
      if (url) activeEditor?.chain().focus().insertContent([{ type: 'image', attrs: { src: url } }, { type: 'paragraph' }]).run()
    },
    onInsertShape: handleInsertShape, onInsertTextBox: handleInsertTextBox,
    onInsertTable: async () => {
      const v = await prompt({ title: t('doc_insert_table'), message: t('doc_table_dimensions'), defaultValue: '3 x 3', confirmLabel: t('doc_insert') })
      if (!v) return
      const m = v.match(/(\d+)\s*[x×]\s*(\d+)/i)
      if (m) activeEditor?.chain().focus().insertContent([makeTableNode(Math.min(20, +m[1]), Math.min(10, +m[2])), { type: 'paragraph' }]).run()
    },
    onSetHeader: handleSetHeader, onSetFooter: handleSetFooter,
    onInsertToc: handleInsertToc, onSpecialChars: () => setSpecialOpen(true),
    onChangeCase: handleChangeCase, onSortParas: handleSortParas, onInsertField: handleInsertField,
    onWordCount: () => setWordCountOpen(true), onInsertBookmark: handleInsertBookmark, onGoTo: () => setGoToOpen(true),
    onInsertCaption: handleInsertCaption, onInsertHr: handleInsertHr,
    onTextTool: handleTextTool, onInsertTitle: handleInsertTitle, onClearAllFormatting: handleClearAllFormatting,
    onMarginsPreset: handleMarginsPreset, onInsertCoverPage: handleInsertCoverPage,
    onEmailLink: handleEmailLink, onRemoveLinks: handleRemoveAllLinks,
    onConvertTextTable: handleConvertTextTable, onConvertTableText: handleConvertTableText,
    onSignatureLine: handleSignatureLine, onPageXofY: handleInsertPageXofY,
    showBoundaries, onToggleBoundaries: () => setShowBoundaries(v => !v),
    showMarks, onToggleMarks: () => setShowMarks(v => !v),
    pageNumFormatNode: <RibbonPageNumFormatBtn format={pageNumFormat} start={pageNumStart}
      onFormat={f => { setPageNumFormat(f); pageNumFormatRef.current = f; scheduleSave() }}
      onStart={n => { setPageNumStart(n); pageNumStartRef.current = n; scheduleSave() }} />,
    onLink: async () => {
      const ed = fmtEditor; if (!ed) return
      const url = await prompt({ title: t('doc_insert_link'), placeholder: 'https://exemple.com', defaultValue: ed.getAttributes('link').href ?? '', allowEmpty: true, confirmLabel: t('doc_apply') })
      if (url === null) return
      if (url === '') ed.chain().focus().extendMarkRange('link').unsetLink().run()
      else ed.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
    },
    mode, onMode: setMode,
    showRuler, onToggleRuler: () => setShowRuler(v => !v),
    navOpen, onToggleNav: () => setNavOpen(v => !v),
    onDetails: () => setDetailsOpen(true),
    spellOn, onToggleSpell: () => setSpellOn(v => !v), spellCount, onSpellDictionary: () => setSpellDictOpen(true),
    onAddComment: handleAddComment, onToggleComments: () => setCommentsOpen(v => !v), commentsOpen, commentCount: commentIds.length,
    onApplyStyle: handleApplyStyle, onEditStyles: () => setStylesEditorOpen(true), styleList, curStyleId,
    table: tableCtx,
    onNew: handleNew, onDuplicate: handleDuplicate, onPrint: handlePrint,
    onExportPdf: handleExportPdf, onExportTxt: handleExportTxt, onExportServer: handleExportServer, online,
    pageColorNode: <RibbonPageColorBtn pageColor={pageColor} pageGrad={pageGrad} onColor={onPageColorHex} onGrad={onPageGradient} />,
    watermarkNode: <RibbonWatermarkBtn value={watermark} onChange={onWatermarkChange} />,
    pageBorderNode: <RibbonPageBorderBtn value={pageBorder} onChange={onPageBorderChange} />,
    lineNumbersNode: <RibbonLineNumbersBtn value={lineNumbers} onChange={onLineNumbersChange} />,
    shapesNode: <RibbonShapesBtn onInsert={handleInsertShape} onInsertTextBox={handleInsertTextBox} />,
    hf: hfBar,
    onHFField: tok => opsRef.current?.insertHFField(tok),
    onHFSwitch: () => opsRef.current?.switchHF(),
    onHFFirstPage: setHfFirstPageOpt, onHFLinked: setHfLinkedOpt,
    onHFClose: () => opsRef.current?.exitHF(),
    cellRanges,
  })

  return (
    <OfficeShell
      ribbon={ribbon}
      activeTabId={activeTab}
      onTabChange={handleTabChange}
      chromeless
      topbarHeight={64}
      search={<DocSearchBar editor={activeEditor as Editor | null} focusSignal={searchFocusTick}
        onRanges={(ranges, active) => setSearchHi({ ranges, active })} />}
      onBack={() => navigate(backPath)}
      onDelete={async () => { await trashDoc(docId); navigate(backPath) }}
      deleteTitle={t('doc_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      titleIcon={<FileText size={16} className="text-white/90 flex-shrink-0" />}
      title={title}
      onTitleChange={setTitle}
      onTitleCommit={handleTitleBlur}
      titlePlaceholder={t('common_untitled')}
      saveStatus={isSaving ? t('doc_saving') : t('doc_saved')}
      topbarActions={
        <div className="flex items-center gap-2">
          {/* Bandeau hors-ligne : édition persistée localement, fusion au retour réseau. */}
          {!online && (
            <span className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-amber-500/15 text-amber-700 text-xs font-medium whitespace-nowrap"
              title={t('doc_offline_hint', { defaultValue: 'Vos modifications sont enregistrées localement et seront synchronisées au retour de la connexion.' })}>
              <CloudOff size={14} /> {t('doc_offline_badge', { defaultValue: 'Hors-ligne' })}
            </span>
          )}
          {/* Macros (sous-module Script) */}
          <MacrosMenu docType="document" docId={docId} buildApi={makeApi} defaultLabel={title} />
          <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
          <button onClick={() => setShareOpen(true)}
            className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-white/15 text-white text-sm font-medium border border-white/25 hover:bg-white/25 transition-colors">
            <UserPlus size={15} /> {t('share_button', 'Partager')}
          </button>
        </div>
      }
      titleActions={
        <>
          {/* Shared save button (before the star + trash) — forces an immediate save. */}
          <SaveButton onSave={flushSave} saving={isSaving} label={t('doc_save', { defaultValue: 'Enregistrer' })} />
          <UndoRedoButtons
            onUndo={() => activeEditorRef.current?.chain().focus().undo().run()}
            onRedo={() => activeEditorRef.current?.chain().focus().redo().run()}
            undoLabel={t('doc_undo', { defaultValue: 'Annuler' })} redoLabel={t('doc_redo', { defaultValue: 'Rétablir' })} />
          <button onClick={() => starDoc(docId, !activeDoc.is_starred)}
            className="p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0"
            title={activeDoc.is_starred ? t('doc_remove_favorite') : t('doc_add_favorite')}>
            <Star size={15}
              fill={activeDoc.is_starred ? 'currentColor' : 'none'}
              className={activeDoc.is_starred ? 'text-warning' : 'text-white/90'} />
          </button>
        </>
      }
    >
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
        {/* Cursor blink animation — injected once per editor session */}
        <style>{CURSOR_STYLE}</style>

        {/* Input fichier caché pour l'upload d'images local */}
        <input ref={imageFileRef} type="file" accept="image/*" className="hidden" onChange={onPickImageFile} />

      {/* ── Trashed banner ──────────────────────────────────────────────── */}
      <TrashedDocBanner docId={docId} />
      {/* La barre de menus + la toolbar sont remplacées par le RUBAN (OfficeShell). */}

      {/* ── Ruler row + scrollable canvas ───────────────────────────────── */}
      <div className="flex flex-1 min-h-0 overflow-hidden relative">

        {specialOpen && activeEditor && (
          <SpecialCharsBar editor={activeEditor as Editor} onClose={() => setSpecialOpen(false)} />
        )}

        {navOpen && (
          <NavPane editor={activeEditor} opsRef={opsRef} onClose={() => setNavOpen(false)} />
        )}

        {showRuler && mode === 'edit' && (
        <div style={{ width: RULER_SZ, flexShrink: 0, display: 'flex', flexDirection: 'column' }}
             onDoubleClick={() => setPageSetupOpen(true)} title={t('doc_page_setup_hint', { defaultValue: 'Double-cliquer : Mise en page' })}>
          <CornerCell tabType={tabType} onCycle={() => setTabType(tt => TAB_CYCLE[(TAB_CYCLE.indexOf(tt) + 1) % TAB_CYCLE.length])} />
          <VerticalRuler
            scrollRef={scrollRef}
            activePage={Math.max(0, docStats.current - 1)}
            activePageTop={activePageBox?.top}
            zoom={zoom}
            marginTop={activeMargins.top}
            marginBottom={activeMargins.bottom}
            pageH={activeGeo.pageH}
            pageGap={PAGE_GAP + PAGE_MARGIN_TOP}
            onMarginsChange={(top, bottom) => {
              const m = { ...marginsRef.current, top, bottom }
              marginsRef.current = m
              setActiveMargins(m)
              scheduleSave()
            }}
            onDragGuideChange={g => setDragGuide(g ? { type: 'horizontal', clientY: g.clientY } : null)}
          />
        </div>
        )}

        <div className="flex flex-col flex-1 min-w-0 overflow-hidden">

          {showRuler && mode === 'edit' && (
          <div className="flex-shrink-0 overflow-hidden bg-[#f1f3f4] border-b border-[#dadce0]" style={{ height: RULER_SZ }}
               onDoubleClick={() => setPageSetupOpen(true)} title={t('doc_page_setup_hint', { defaultValue: 'Double-cliquer : Mise en page' })}>
            {/* Calée sur la page ACTIVE (marginLeft = son x dans le contenu) ; en colonne
                unique cette valeur correspond au centrage → rétro-compatible. */}
            <div className={activePageBox ? 'h-full' : 'h-full flex justify-center'}
                 style={activePageBox ? { marginLeft: activePageBox.left } : undefined}>
              <HorizontalRuler
                pageW={activeGeo.pageW}
                marginLeft={activeMargins.left}
                marginRight={activeMargins.right}
                zoom={zoom}
                columns={activeGeo.columns}
                colGap={activeGeo.colGap}
                indentLeft={paraIndents.left}
                indentFirstLine={paraIndents.first}
                indentRight={paraIndents.right}
                tabStops={paraTabs}
                tabType={tabType}
                onTabStopsChange={setParaTabStops}
                onIndentsChange={(ind, commit) => setParaIndentAttrs({ indentLeft: ind.left, indentFirstLine: ind.first, indentRight: ind.right }, commit)}
                onMarginsChange={(left, right) => {
                  const m = { ...marginsRef.current, left, right }
                  marginsRef.current = m
                  setActiveMargins(m)
                  scheduleSave()
                }}
                onDragGuideChange={g => setDragGuide(g ? { type: 'vertical', clientX: g.clientX } : null)}
              />
            </div>
          </div>
          )}

          <div
            ref={scrollRef}
            className="flex-1 overflow-auto"
            style={{ background: '#f1f3f4' }}
          >
            <div className="flex justify-center">
              <PaginatedEditor
                key={docId}
                ydoc={ydoc}
                awareness={awareness}
                collabEmpty={collabEmpty}
                initialDoc={flattenToDoc(activeDoc.content_json as object | null)}
                section={{ id: sectionIdRef.current, orientation: baseOrientation, margins: activeMargins, columns: baseColumns }}
                zoom={zoom}
                scrollContainerRef={scrollRef}
                onEditor={(ed) => { activeEditorRef.current = ed; setActiveEditor(ed) }}
                onSave={(doc) => { docRef.current = doc; scheduleSave() }}
                onBaseChange={handleBaseChange}
                onActiveSection={setActiveOrientation}
                onRegisterOps={registerOps}
                pageNumbers={pageNumbers}
                header={header}
                footer={footer}
                hfFirstPage={hfFirstPage}
                paper={paperSize}
                docTitle={title}
                onHFActive={(active, ctx, ed) => { setHfBar(active ? ctx : null); setHfZoneEditor(active ? ed : null) }}
                onTbActive={(active, ed) => { setTbBar(active); setTbZoneEditor(active ? ed : null) }}
                onCommitHF={commitHF}
                pageBg={pageBgCss}
                watermark={watermark}
                pageBorder={pageBorder}
                lineNumbers={lineNumbers}
                showBoundaries={showBoundaries}
                showMarks={showMarks}
                pageNumFormat={pageNumFormat}
                pageNumStart={pageNumStart}
                spellCheck={spellOn}
                onSpellCount={setSpellCount}
                onStats={setDocStats}
                spellVersion={spellVersion}
                searchRanges={searchHi.ranges}
                searchActive={searchHi.active}
                activeCommentId={activeCommentId}
                onCommentActivate={handleCommentActivate}
                onCommentRanges={setCommentIds}
                onAddComment={handleAddComment}
                onTableSel={setTableSel}
              />
            </div>
          </div>
        </div>

        {commentsOpen && (
          <CommentsPanel
            commentsMap={commentsMap}
            editor={activeEditor as Editor | null}
            opsRef={opsRef}
            activeId={activeCommentId}
            setActiveId={setActiveCommentId}
            anchoredIds={commentIds}
            user={{ id: authUser?.id || '', name: authUser?.display_name || authUser?.username || authUser?.email || 'Anonyme' }}
            onClose={() => setCommentsOpen(false)}
          />
        )}
      </div>

      {/* ── Barre de statut (pagination, mots, correcteur, langue, a11y, zoom) ── */}
      <DocStatusBar
        editor={activeEditor as Editor | null}
        pages={docStats.pages}
        current={docStats.current}
        zoom={zoom}
        onZoom={setZoom}
        mode={mode}
        onMode={setMode}
        spellOn={spellOn}
        spellCount={spellCount}
        onToggleSpell={() => setSpellOn(v => !v)}
        onOpenSpell={() => setSpellDictOpen(true)}
        showRuler={showRuler}
        onToggleRuler={() => setShowRuler(v => !v)}
        pageNumbers={pageNumbers}
        onTogglePageNumbers={() => handleSetPageNumbers(pageNumbers === 'none' ? 'footer-center' : 'none')}
      />

      <DragGuideLine guide={dragGuide} />
      </div>
      {shareOpen && <DocumentShareDialog docId={docId} onClose={() => setShareOpen(false)} />}
      {zoomDialogOpen && (
        <ZoomDialog
          zoom={zoom}
          onPick={(z) => setZoom(z)}
          onFit={fitZoom}
          onClose={() => setZoomDialogOpen(false)}
        />
      )}
      {detailsOpen && (
        <DocDetailsDialog
          editor={activeEditor}
          opsRef={opsRef}
          title={title}
          createdAt={(activeDoc as { created_at?: string }).created_at}
          updatedAt={(activeDoc as { updated_at?: string }).updated_at}
          onClose={() => setDetailsOpen(false)}
        />
      )}
      {wordCountOpen && (
        <DocWordCountDialog editor={activeEditor} opsRef={opsRef} onClose={() => setWordCountOpen(false)} />
      )}
      {goToOpen && (
        <DocGoToDialog editor={activeEditor} opsRef={opsRef} onClose={() => setGoToOpen(false)} />
      )}
      {stylesEditorOpen && (
        <StylesEditorDialog styles={styleList} initialId={curStyleId}
          onSave={handleSaveStyle} onClose={() => setStylesEditorOpen(false)} />
      )}
      {spellDictOpen && (
        <SpellDictionaryDialog onChange={() => setSpellVersion(v => v + 1)} onClose={() => setSpellDictOpen(false)} />
      )}
      {pageSetupOpen && (
        <PageSetupDialog
          init={{ margins: activeMargins, orientation: activeOrientation, paper: paperSize, gutter, headerDist, footerDist, vAlign: vAlignPage, sectionStart, evenOdd, firstPageDiff: hfFirstPage }}
          onApply={v => {
            setActiveMargins(v.margins); marginsRef.current = v.margins
            setActiveOrientation(v.orientation); setBaseOrientation(v.orientation); baseOrientationRef.current = v.orientation
            handlePaperSize(v.paper)
            setGutter(v.gutter); setHeaderDist(v.headerDist); setFooterDist(v.footerDist)
            setVAlignPage(v.vAlign); setSectionStart(v.sectionStart); setEvenOdd(v.evenOdd)
            setHfFirstPage(v.firstPageDiff); hfFirstRef.current = v.firstPageDiff
            scheduleSave()
          }}
          onClose={() => setPageSetupOpen(false)}
        />
      )}
    </OfficeShell>
  )
}

export { DocumentEditorArea }
