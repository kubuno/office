import { useEffect, useLayoutEffect, useCallback, useRef, useState, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import type { Editor } from '@tiptap/core'
import type { JSONContent } from '@tiptap/react'
import { useNavigate, useLocation } from 'react-router-dom'
import { useEditor, EditorContent } from '@tiptap/react'
import { Extension, Node as TipTapNode, Mark as TipTapMark } from '@tiptap/core'
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
  FileText, RotateCcw,
  Minus, Plus, Printer, Star, UserPlus,
  IndentIncrease, IndentDecrease, Image as ImageIcon, ChevronDown, X,
  LayoutTemplate,
  Scissors, Copy, ClipboardPaste, Table as TableIcon, Square, Search, Hash,
  Eye, Ruler as RulerIcon, PanelLeft, Sigma, ListTree, FileDown,
  SplitSquareVertical, Superscript, Subscript, CopyPlus, SpellCheck,
  MessageSquare, MessageSquarePlus, Check, Trash2, Send, CornerDownRight,
  Rows3, Columns3, Combine, Paintbrush, Pencil, BookMarked,
  Languages, Accessibility, BookOpen, SlidersHorizontal, Monitor,
} from 'lucide-react'
import { Dropdown, MenuDropdown, Button, Checkbox, ColorField, GradientField, gradientToCss, DEFAULT_GRADIENT, ColorSwatchPicker, AnchoredPopover, useAppPickerTheme } from '@ui'
import type { MenuItem, Gradient } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
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
  paginateMulti, parseRichTextBox, RICH_TB_PAD,
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
}
// Champs persistables d'un style nommé (sans le libellé i18n, recalculé à l'usage).
interface NamedStyleMeta { block: 'paragraph' | 'heading'; level?: number; font?: string; size?: number; bold?: boolean; italic?: boolean; color?: string; align?: 'left' | 'center' | 'right' | 'justify'; lineHeight?: number; spaceBefore?: number; spaceAfter?: number; name?: string }

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
    styles: meta.styles }
}

// Substitue les champs dynamiques ({page}…) dans les nœuds texte d'un doc HF et
// retourne un NOUVEAU doc (l'original n'est pas muté) pour le rendu d'une page.
function expandHFDoc(doc: HFContent, page: number, pages: number, title: string, lang: string): HFContent {
  const sub = (s: string) => s
    .replace(/\{page\}/gi, String(page))
    .replace(/\{pages\}/gi, String(pages))
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
    }
  },
  parseHTML() { return [{ tag: 'img[src]' }] },
  renderHTML({ HTMLAttributes }) { return ['img', HTMLAttributes] },
})

// ── Formes & zones de texte (SVG vectoriel porté par le nœud image) ───────────
// Une forme = image SVG data-URL paramétrique → bénéficie de TOUTE la machinerie
// image existante (sélection, redimensionnement, rotation, alignement, export).
export type ShapeKind = 'rect' | 'roundRect' | 'ellipse' | 'triangle' | 'diamond' | 'arrow' | 'line' | 'star'

interface ShapeParams { kind: ShapeKind; fill: string; stroke: string }

function shapeSvg(kind: ShapeKind, w: number, h: number, fill = '#dbe7ff', stroke = '#1a73e8'): string {
  const sw = 2
  const cx = w / 2, cy = h / 2
  let body = ''
  switch (kind) {
    case 'rect':      body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}"/>`; break
    case 'roundRect': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${Math.min(w, h) * 0.12}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}"/>`; break
    case 'ellipse':   body = `<ellipse cx="${cx}" cy="${cy}" rx="${cx - sw}" ry="${cy - sw}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}"/>`; break
    case 'triangle':  body = `<polygon points="${cx},${sw} ${w - sw},${h - sw} ${sw},${h - sw}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" stroke-linejoin="round"/>`; break
    case 'diamond':   body = `<polygon points="${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" stroke-linejoin="round"/>`; break
    case 'arrow': {
      const sh = h * 0.32
      body = `<polygon points="${sw},${cy - sh / 2} ${w * 0.62},${cy - sh / 2} ${w * 0.62},${sw} ${w - sw},${cy} ${w * 0.62},${h - sw} ${w * 0.62},${cy + sh / 2} ${sw},${cy + sh / 2}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" stroke-linejoin="round"/>`
      break
    }
    case 'line': body = `<line x1="${sw}" y1="${h - sw}" x2="${w - sw}" y2="${sw}" stroke="${stroke}" stroke-width="3" stroke-linecap="round"/>`; break
    case 'star': {
      const spikes = 5, oR = Math.min(w, h) / 2 - sw, iR = oR * 0.45
      let pts = ''
      for (let i = 0; i < spikes * 2; i++) {
        const r = i % 2 === 0 ? oR : iR
        const a = (Math.PI / spikes) * i - Math.PI / 2
        pts += `${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)} `
      }
      body = `<polygon points="${pts.trim()}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" stroke-linejoin="round"/>`
      break
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`
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
  FontMarksExt,
  InheritFontExt,
  SuperscriptExt,
  SubscriptExt,
  ImageExt,
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
  StarterKit.configure({ undoRedo: false }),
  SectionBreakExt,
  PageBreakExt,
  CommentMark,
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
  StarterKit,                       // avec son propre undo/redo (pas de Yjs ici)
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
        <Dropdown value={curFont} onChange={f => applyInlineFormat(editor, { ff: f })}
          options={availableFonts.map(f => ({ value: f, label: f }))} width={132} />
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
  zoom:         number
  marginTop:    number
  marginBottom: number
  pageH:        number
  pageGap:      number
  onMarginsChange?:   (top: number, bottom: number) => void
  onDragGuideChange?: (guide: { clientY: number } | null) => void
}

function VerticalRuler({ scrollRef, activePage, zoom, marginTop, marginBottom, pageH, pageGap, onMarginsChange, onDragGuideChange }: VerticalRulerProps) {
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
    const paperTopY   = paperTop0 + activeP * opsh - st             // écran
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
  }, [redraw, marginTop, marginBottom, activePage])

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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30" onClick={onClose}>
      <div className="bg-white rounded-xl shadow-xl p-6 min-w-[300px]" onClick={e => e.stopPropagation()}>
        <h3 className="text-sm font-semibold text-text-primary mb-3">{t('doc_document_details')}</h3>
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
    </div>
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
        onClick={async () => { await restoreDoc(docId); navigate(`/office/${docId}`) }}
        className="text-xs text-primary hover:underline flex items-center gap-1"
      >
        <RotateCcw size={11} /> {t('doc_restore')}
      </button>
      <button
        onClick={async () => { await deleteDoc(docId); navigate('/office') }}
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

function PaginatedEditor({ initialDoc, ydoc, awareness, collabEmpty, section, zoom, scrollContainerRef, onEditor, onSave, onBaseChange, onActiveSection, onRegisterOps, pageNumbers = 'none', header, footer, hfFirstPage = false, paper = 'a4', docTitle = '', pageBg, onHFActive, onCommitHF, onTbActive, spellCheck = true, onSpellCount, onStats, spellVersion = 0, searchRanges, searchActive = 0, activeCommentId = null, onCommentActivate, onCommentRanges, onAddComment, onTableSel }: PaginatedEditorProps) {
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
  const drawPageDecorations = useCallback((cx: CanvasRenderingContext2D, gg: PageGeometry, idx: number, total: number, secIdx = 0, skipBand?: 'header' | 'footer') => {
    const pn = pnRef.current
    const { first, title } = hfRef.current
    const skipFirst = first && idx === 0   // « 1ʳᵉ page différente » : marges vierges
    const lang = i18nInst.language
    const cw = gg.pageW - 2 * gg.marginH
    // Rend un doc d'en-tête/pied dans sa bande (haut de marge / bas de marge).
    const renderBand = (doc: HFContent, bandTop: number) => {
      if (isHFEmpty(doc)) return
      const expanded = expandHFDoc(doc, idx + 1, total, title, lang)
      const layout = layoutDocument(expanded, cw)
      paintLayoutAt(cx, layout, gg.marginH, bandTop)
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
      const label = String(idx + 1)
      const tw = cx.measureText(label).width
      const yy = pn.startsWith('header') ? gg.marginV * 0.55 : gg.pageH - gg.marginBottom * 0.5
      const xx = pn.endsWith('center') ? (gg.pageW - tw) / 2 : gg.pageW - gg.marginH - tw
      cx.fillText(label, xx, yy)
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
    onRegisterOps?.({ setOrientation, setColumns, insertBreak, insertPageBreak, pageCount, outline, scrollToPos, exportPageCanvases, hfContext, setSectionHF, setSectionBg, enterHF: (k) => enterHFEdit(k), exitHF: exitHFEdit, switchHF: switchHFBand, insertHFField, insertTextBox: insertTextBoxOp, editTextBox: enterTextBoxEdit, commentAnchor })
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
        let top = CANVAS_PAD_Y
        for (let k = 0; k < idx; k++) top += geomOf(pgs[k]).pageH * z + PAGE_GAP
        const yAbs = top + (geomOf(pgs[idx]).marginV + (c.y - (pgs[idx]?.startY ?? 0))) * z
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
    // top = somme des hauteurs des pages précédentes (hauteurs variables par section)
    let pageTop = CANVAS_PAD_Y
    for (let k = 0; k < idx; k++) pageTop += geomOf(pgs[k]).pageH * z + PAGE_GAP
    // offset horizontal : page centrée dans un conteneur de largeur maxPageW
    const leftOffset = (maxPageW() - geom.pageW) * z / 2
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
    caret.style.display = 'block'
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
    let pageTop = CANVAS_PAD_Y
    for (let k = 0; k < idx; k++) pageTop += geomOf(pgs[k]).pageH * z + PAGE_GAP
    const leftOffset = (maxPageW() - geom.pageW) * z / 2
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
    if (!ed || !layout || !(sel instanceof NodeSelection) || sel.node.type.name !== 'image') {
      setImgSel(null); return
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
    let pageTop = CANVAS_PAD_Y
    for (let k = 0; k < idx; k++) pageTop += geomOf(pgs[k]).pageH * z + PAGE_GAP
    const leftOffset = (maxPageW() - geom.pageW) * z / 2
    const localY = imgLine.y - (pgs[idx]?.startY ?? 0)
    // Pour un flottant (derrière/devant), l'image est dessinée à line.y + wrapY.
    const floatDy = (imgLine.image.wrap === 'behind' || imgLine.image.wrap === 'front') ? (imgLine.image.wrapY || 0) : 0
    setImgSel({
      pos: sel.from,
      cx: leftOffset + (geom.marginH + imgLine.image.x + imgLine.image.w / 2) * z,
      cy: pageTop + (geom.marginV + localY + floatDy + imgLine.image.h / 2) * z,
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
        drawPageDecorations(cx, gg, idx, pagesRef.current.length, pg.secIdx, skipBand)
        cx.restore()
      }
    })
  }, [drawPageDecorations])

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

  const onPageMouseDown = useCallback((pageIdx: number, e: React.MouseEvent) => {
    const ed = editorRef.current; if (!ed) return

    // Clic DROIT : ne pas déplacer le curseur ni collapser la sélection — le menu
    // contextuel (onPageContextMenu) gère le placement et préserve la sélection.
    if (e.button === 2) return
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
      const pageUnder = (cy: number): number => {
        let idx = pageIdx
        canvasRefs.current.forEach((cv, k) => { const rr = cv.getBoundingClientRect(); if (cy >= rr.top && cy <= rr.bottom) idx = k })
        return idx
      }
      const rectOf = (h: { r: number; c: number; colspan: number; rowspan: number }): TableRect => ({
        r0: Math.min(anchor.r, h.r), c0: Math.min(anchor.c, h.c),
        r1: Math.max(anchor.r + anchor.rowspan - 1, h.r + h.rowspan - 1),
        c1: Math.max(anchor.c + anchor.colspan - 1, h.c + h.colspan - 1),
      })
      const onMove = (me: MouseEvent) => {
        const h = hitTableCell(pageUnder(me.clientY), me.clientX, me.clientY)
        if (!h || h.tableStart !== anchor.tableStart) return
        const rect = rectOf(h)
        setTableSel((rect.r0 === rect.r1 && rect.c0 === rect.c1) ? null : { tableStart: anchor.tableStart, ...rect })
      }
      const onUp = () => { document.removeEventListener('mousemove', onMove); document.removeEventListener('mouseup', onUp) }
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
      return
    }

    // Clic sur une image → sélection du nœud image (affiche la barre image).
    const hitNode = ed.state.doc.nodeAt(pos)
    if (hitNode?.type.name === 'image') {
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

    const pageOfY = (clientY: number): number => {
      // quelle page est sous le pointeur (par bornes des canvas)
      let idx = pageIdx
      canvasRefs.current.forEach((cv, k) => {
        const rr = cv.getBoundingClientRect()
        if (clientY >= rr.top && clientY <= rr.bottom) idx = k
      })
      return idx
    }
    const extend = (clientX: number, clientY: number) => {
      const idx = pageOfY(clientY)
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
  }, [posFromEvent, scrollContainerRef, enterTextBoxEdit, exitTextBoxEdit])

  // ── Menu contextuel (clic droit) — contextuel selon la sélection ────────────
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null)

  const rootRef = useRef<HTMLDivElement>(null)

  // Actions sur l'image sélectionnée (NodeSelection courante).
  const imgUpdate = (attrs: Record<string, unknown>) => { editorRef.current?.chain().updateAttributes('image', attrs).run() }
  const imgAlign = (align: 'left' | 'center' | 'right') => { imgUpdate({ align }); requestAnimationFrame(updateImgSel) }
  const imgReset = () => { imgUpdate({ width: 0, height: 0, rotation: 0 }); requestAnimationFrame(updateImgSel) }
  const imgSetWrap = (wrap: string) => { imgUpdate({ wrap }); requestAnimationFrame(updateImgSel) }

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
  }, [posFromEvent])

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
    ]
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
      { type: 'separator' },
      { type: 'action', label: t('doc_select_all'), shortcut: `${MOD}A`, onClick: () => ed?.chain().focus().selectAll().run() },
      ...objectItems(),
      ...tableItems(),
    ]
  }

  // Items contextuels d'un objet sélectionné (zone de texte / forme SVG rééditables).
  const objectItems = (): MenuItem[] => {
    const ed = editorRef.current
    const sel = ed?.state.selection
    const node = sel instanceof NodeSelection && sel.node.type.name === 'image' ? sel.node : null
    if (!node) return []
    const items: MenuItem[] = []
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
            ed?.chain().focus().updateAttributes('image', { src: svgToDataUrl(shapeSvg(sp.kind, w, h, fill, stroke)), alt: shapeAlt(params) }).run()
          },
        })),
      })
    }
    return items
  }

  const containerW = Math.max(g.pageW, ...geomsRef.current.map(x => x.pageW))
  return (
    <div ref={rootRef} className="relative" style={{ width: containerW * zoom }}
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
      <div className="relative" style={{ paddingTop: CANVAS_PAD_Y }}>
        {pages.map((pg, idx) => {
          const geom = geomsRef.current[pg.secIdx] || geomsRef.current[0] || g
          return (
            <canvas
              key={idx}
              ref={el => { if (el) canvasRefs.current.set(idx, el); else canvasRefs.current.delete(idx) }}
              onMouseDown={e => onPageMouseDown(idx, e)}
              onContextMenu={e => onPageContextMenu(idx, e)}
              className="block bg-white shadow-sm mx-auto"
              style={{ width: geom.pageW * zoom, height: geom.pageH * zoom, marginBottom: PAGE_GAP, cursor: 'text',
                       background: secMetaRef.current[pg.secIdx]?.pageColor ?? (pageBg || undefined) }}
            />
          )
        })}
      </div>

      {/* ── Surbrillance de la sélection de cellules de tableau (overlay bleu) ── */}
      {tableSel && (() => {
        const z = zoom, sel = tableSel
        const cells: React.ReactNode[] = []
        let top = CANVAS_PAD_Y
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const pageTop = top
          top += geom.pageH * z + PAGE_GAP
          const left = (maxPageW() - geom.pageW) * z / 2
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
        let top = CANVAS_PAD_Y
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const pageTop = top
          top += geom.pageH * z + PAGE_GAP
          const left = (maxPageW() - geom.pageW) * z / 2
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
        const lo = (geom: PageGeometry) => (maxPageW() - geom.pageW) * z / 2
        let top = CANVAS_PAD_Y
        const overlays: React.ReactNode[] = []
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const left = lo(geom)
          const pageTop = top
          top += geom.pageH * z + PAGE_GAP
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

      {/* Barre flottante d'image (alignement + redimensionnement) — masquée pendant
          l'édition in-place d'une zone de texte (le cadre d'édition prend le relais). */}
      {imgSel && !tbEdit && (() => {
        const HANDLES: Array<{ k: string; l: number; t: number; cur: string }> = [
          { k: 'nw', l: 0,   t: 0,   cur: 'nwse-resize' }, { k: 'n', l: 0.5, t: 0, cur: 'ns-resize' }, { k: 'ne', l: 1, t: 0, cur: 'nesw-resize' },
          { k: 'w',  l: 0,   t: 0.5, cur: 'ew-resize' },                                               { k: 'e',  l: 1, t: 0.5, cur: 'ew-resize' },
          { k: 'sw', l: 0,   t: 1,   cur: 'nesw-resize' }, { k: 's', l: 0.5, t: 1, cur: 'ns-resize' }, { k: 'se', l: 1, t: 1, cur: 'nwse-resize' },
        ]
        return (
          <>
            {/* Boîte de poignées (tournée autour du centre comme l'image) */}
            <div style={{
              position: 'absolute', left: imgSel.cx, top: imgSel.cy, width: imgSel.w, height: imgSel.h,
              transform: `translate(-50%,-50%) rotate(${imgSel.rotation}deg)`, zIndex: 30, pointerEvents: 'none',
              border: '1.5px solid #1a73e8',
            }}>
              {/* Zone de déplacement (corps) — seulement pour un objet FLOTTANT. */}
              {(imgSel.wrap === 'behind' || imgSel.wrap === 'front' || imgSel.wrap === 'square') && (
                <div onPointerDown={startHandleDrag('move')}
                  style={{ position: 'absolute', inset: 6, pointerEvents: 'auto', cursor: 'move' }} />
              )}
              {HANDLES.map(h => (
                <div key={h.k} onPointerDown={startHandleDrag(h.k)}
                  style={{
                    position: 'absolute', left: `${h.l * 100}%`, top: `${h.t * 100}%`,
                    width: 10, height: 10, marginLeft: -5, marginTop: -5,
                    background: '#fff', border: '1.5px solid #1a73e8', borderRadius: 2,
                    pointerEvents: 'auto', cursor: h.cur,
                  }} />
              ))}
              {/* Poignée de rotation (au-dessus du centre haut) */}
              <div onPointerDown={startHandleDrag('rot')}
                style={{ position: 'absolute', left: '50%', top: -26, width: 12, height: 12, marginLeft: -6,
                  background: '#fff', border: '1.5px solid #1a73e8', borderRadius: '50%', pointerEvents: 'auto', cursor: 'grab' }} />
              <div style={{ position: 'absolute', left: '50%', top: -16, width: 1.5, height: 16, marginLeft: -0.75, background: '#1a73e8' }} />
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
function setCellsBg(ed: Editor, rect: TableRect, hex: string): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    const seen = new Set<string>()
    for (let r = rect.r0; r <= rect.r1; r++) for (let c = rect.c0; c <= rect.c1; c++) {
      const g = grid[r]?.[c]; if (!g) continue
      const k = g.ri + ',' + g.ci; if (seen.has(k)) continue; seen.add(k)
      const cell = (rows[g.ri].content as JSONContent[])[g.ci]
      cell.attrs = { ...cellAttrs(cell), cellBg: hex }
    }
  })
}
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
  zoom: number; onZoom: (z: number) => void
  orientation: Orientation; onOrientation: (o: Orientation) => void
  columns: number; onColumns: (n: number) => void
  paperSize: PaperSize; onPaperSize: (p: PaperSize) => void
  pageNumbers: PageNumbers; onPageNumbers: (p: PageNumbers) => void
  onPageBreak: () => void; onSectionBreak: () => void
  onUploadImage: () => void; onImageUrl: () => void
  onInsertShape: (k: ShapeKind) => void; onInsertTextBox: () => void; onInsertTable: () => void
  onSetHeader: () => void; onSetFooter: () => void; onInsertToc: () => void; onSpecialChars: () => void
  onLink: () => void
  mode: 'edit' | 'read'; onMode: (m: 'edit' | 'read') => void
  showRuler: boolean; onToggleRuler: () => void; navOpen: boolean; onToggleNav: () => void
  onDetails: () => void
  spellOn: boolean; onToggleSpell: () => void; spellCount: number; onSpellDictionary: () => void
  onAddComment: () => void; onToggleComments: () => void; commentsOpen: boolean; commentCount: number
  onApplyStyle: (id: string) => void; onEditStyles: () => void; styleList: NamedStyle[]; curStyleId: string
  table: TableRibbonCtx | null
  onNew: () => void; onDuplicate: () => void; onPrint: () => void
  onExportPdf: () => void; onExportTxt: () => void; onExportServer: (fmt: 'docx' | 'odt') => void
  pageColorNode: React.ReactNode
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
            { id: 'edocx', kind: 'button', label: 'Word (DOCX)', onClick: () => c.onExportServer('docx') },
            { id: 'eodt', kind: 'button', label: 'OpenDocument (ODT)', onClick: () => c.onExportServer('odt') },
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
        { id: 'family', kind: 'dropdown', value: curFont, width: 132, options: c.fonts.map(f => ({ value: f, label: f })), onChange: v => fmt && applyInlineFormat(fmt, { ff: v }, cr) },
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
          splitItems: [[t('doc_spacing_single'), 1.0], ['1,15', 1.15], ['1,5', 1.5], [t('doc_spacing_double'), 2.0]].map(([lbl, lh]) => ({ id: 'ls' + lh, kind: 'button' as const, label: String(lbl), active: curLs === lh, onClick: () => setLs(lh as number) })) },
        { id: 'sep3', kind: 'separator' },
        align('left', <AlignLeft size={15} />, t('doc_align_left')),
        align('center', <AlignCenter size={15} />, t('doc_align_center')),
        align('right', <AlignRight size={15} />, t('doc_align_right')),
        align('justify', <AlignJustify size={15} />, t('doc_align_justify')),
      ] },
      { id: 'styles', label: t('doc_grp_styles', { defaultValue: 'Styles' }), items: [
        { id: 'style', kind: 'dropdown', value: c.curStyleId, width: 140,
          options: c.styleList.map(s => ({ value: s.id, label: styleLabel(s, t) })),
          onChange: v => c.onApplyStyle(v) },
        { id: 'editstyle', kind: 'button', icon: <Pencil size={15} />, tooltip: t('doc_edit_styles', { defaultValue: 'Modifier les styles' }), onClick: c.onEditStyles },
      ] },
      { id: 'edit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'link', kind: 'button', icon: <LinkIcon size={15} />, label: t('doc_insert_link'), active: isA('link'), onClick: c.onLink },
        { id: 'details', kind: 'button', icon: <FileText size={15} />, label: t('doc_details', { defaultValue: 'Détails' }), onClick: c.onDetails },
      ] },
    ],
  }

  const insert: RibbonTab = {
    id: 'insert', label: t('doc_tab_insert', { defaultValue: 'Insertion' }),
    groups: [
      { id: 'pages', label: t('doc_grp_pages', { defaultValue: 'Pages' }), items: [
        { id: 'pb', kind: 'button', icon: <FileText size={15} />, label: t('doc_page_break'), onClick: c.onPageBreak },
        { id: 'sb', kind: 'button', icon: <SplitSquareVertical size={15} />, label: t('doc_section_break_next_page', { defaultValue: 'Saut de section' }), onClick: c.onSectionBreak },
      ] },
      { id: 'tables', label: t('doc_grp_tables', { defaultValue: 'Tableaux' }), items: [
        { id: 'table', kind: 'button', size: 'large', icon: <TableIcon size={22} />, label: t('doc_table', { defaultValue: 'Tableau' }), onClick: c.onInsertTable },
      ] },
      { id: 'illus', label: t('doc_grp_illustrations', { defaultValue: 'Illustrations' }), items: [
        { id: 'img', kind: 'button', size: 'large', icon: <ImageIcon size={22} />, label: t('doc_image', { defaultValue: 'Image' }), onClick: c.onUploadImage },
        { id: 'imgurl', kind: 'button', icon: <LinkIcon size={14} />, label: t('doc_insert_image_url', { defaultValue: 'Depuis une URL' }), onClick: c.onImageUrl },
        { id: 'shapes', kind: 'dropdown', value: '', width: 96, options: ([['rect', 'Rectangle'], ['roundRect', 'Rect. arrondi'], ['ellipse', 'Ellipse'], ['triangle', 'Triangle'], ['diamond', 'Losange'], ['arrow', 'Flèche'], ['line', 'Trait'], ['star', 'Étoile']] as Array<[ShapeKind, string]>).map(([v, l]) => ({ value: v, label: l })), onChange: v => v && c.onInsertShape(v as ShapeKind) },
        { id: 'textbox', kind: 'button', icon: <Square size={14} />, label: t('doc_text_box', { defaultValue: 'Zone de texte' }), onClick: c.onInsertTextBox },
      ] },
      { id: 'hf', label: t('doc_grp_header_footer', { defaultValue: 'En-tête et pied' }), items: [
        { id: 'header', kind: 'button', icon: <PanelLeft size={15} className="rotate-90" />, label: t('doc_header', { defaultValue: 'En-tête' }), onClick: c.onSetHeader },
        { id: 'footer', kind: 'button', icon: <PanelLeft size={15} className="-rotate-90" />, label: t('doc_footer', { defaultValue: 'Pied de page' }), onClick: c.onSetFooter },
        { id: 'pgnum', kind: 'dropdown', value: c.pageNumbers, width: 120, options: ([['none', 'Aucun'], ['footer-right', 'Pied · droite'], ['footer-center', 'Pied · centre'], ['header-right', 'Haut · droite'], ['header-center', 'Haut · centre']] as Array<[PageNumbers, string]>).map(([v, l]) => ({ value: v, label: l })), onChange: v => c.onPageNumbers(v as PageNumbers) },
      ] },
      { id: 'text', label: t('doc_grp_text', { defaultValue: 'Texte' }), items: [
        { id: 'toc', kind: 'button', icon: <ListTree size={15} />, label: t('doc_toc', { defaultValue: 'Table des matières' }), onClick: c.onInsertToc },
        { id: 'special', kind: 'button', icon: <Sigma size={15} />, label: t('doc_special_chars', { defaultValue: 'Caractères spéciaux' }), onClick: c.onSpecialChars },
      ] },
    ],
  }

  const layout: RibbonTab = {
    id: 'layout', label: t('doc_tab_layout', { defaultValue: 'Mise en page' }),
    groups: [
      { id: 'setup', label: t('doc_grp_pagesetup', { defaultValue: 'Mise en page' }), items: [
        { id: 'orient', kind: 'dropdown', value: c.orientation, width: 110, options: [{ value: 'portrait', label: t('doc_portrait', { defaultValue: 'Portrait' }) }, { value: 'landscape', label: t('doc_landscape', { defaultValue: 'Paysage' }) }], onChange: v => c.onOrientation(v as Orientation) },
        { id: 'paper', kind: 'dropdown', value: c.paperSize, width: 84, options: (['a4', 'a5', 'a3', 'letter', 'legal'] as PaperSize[]).map(p => ({ value: p, label: p.toUpperCase() })), onChange: v => c.onPaperSize(v as PaperSize) },
        { id: 'cols', kind: 'dropdown', value: String(c.columns), width: 96, options: [1, 2, 3].map(n => ({ value: String(n), label: `${n} ${n > 1 ? t('doc_columns', { defaultValue: 'colonnes' }) : t('doc_column', { defaultValue: 'colonne' })}` })), onChange: v => c.onColumns(parseInt(v)) },
      ] },
      { id: 'bg', label: t('doc_grp_background', { defaultValue: 'Arrière-plan' }), items: [
        { id: 'pagecolor', kind: 'custom', render: c.pageColorNode },
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
      ] },
      { id: 'zoom', label: t('doc_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'zout', kind: 'button', icon: <Minus size={15} />, tooltip: t('doc_zoom_out'), onClick: () => c.onZoom(Math.max(0.25, Math.round((c.zoom - 0.25) * 100) / 100)) },
        { id: 'zlvl', kind: 'dropdown', value: String(c.zoom), width: 70, options: ZOOM_PRESETS.map(p => ({ value: String(p), label: `${Math.round(p * 100)}%` })), onChange: v => c.onZoom(Number(v)) },
        { id: 'zin', kind: 'button', icon: <Plus size={15} />, tooltip: t('doc_zoom_in'), onClick: () => c.onZoom(Math.min(3, Math.round((c.zoom + 0.25) * 100) / 100)) },
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

  return [home, insert, layout, view, review, imageTab, hfTab, tableTab]
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
function RibbonCellColorBtn({ editor, onPick }: { editor: Editor | null; onPick: (hex: string) => void }) {
  const { t } = useTranslation('office')
  const ref = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const cur = (editor?.getAttributes('tableCell').cellBg as string) || '#ffffff'
  return (
    <>
      <button ref={ref} onMouseDown={e => e.preventDefault()} onClick={() => setOpen(o => !o)}
        className="w-7 h-[22px] flex flex-col items-center justify-center rounded hover:bg-surface-2 text-text-secondary"
        title={t('doc_cell_color', { defaultValue: 'Couleur de cellule' })}>
        <Paintbrush size={14} />
        <div className="w-4 h-1 rounded-sm" style={{ background: cur }} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <ColorSwatchPicker color={cur} t={t}
          onChange={hex => onPick(hex)}
          onClose={() => setOpen(false)} customLabel={t('doc_custom_color', { defaultValue: 'Personnalisé' })} />
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

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/30" onMouseDown={onClose}>
      <div className="bg-white rounded-xl shadow-2xl w-[480px] max-w-[92vw] p-5" onMouseDown={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-base font-semibold text-text-primary">{t('doc_edit_styles', { defaultValue: 'Modifier les styles' })}</h2>
          <button onClick={onClose} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><X size={16} /></button>
        </div>
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
    </div>,
    document.body,
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
  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/30" onMouseDown={onClose}>
      <div className="bg-white rounded-xl shadow-2xl w-[420px] max-w-[92vw] p-5" onMouseDown={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-base font-semibold text-text-primary">{t('doc_spell_dictionary', { defaultValue: 'Dictionnaire personnel' })}</h2>
          <button onClick={onClose} className="p-1 rounded hover:bg-surface-2 text-text-secondary"><X size={16} /></button>
        </div>
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
    </div>,
    document.body,
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
        <input type="range" min={50} max={200} step={10} value={Math.min(200, Math.max(50, pct))}
          onChange={e => onZoom(Number(e.target.value) / 100)}
          title={`${pct} %`} className="w-28 cursor-pointer h-1"
          style={{ accentColor: 'var(--color-primary, #1a73e8)' }} />
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
  const backPath  = (location.state as { from?: string } | null)?.from ?? '/office'
  const { activeDoc, openDoc, saveDoc, trashDoc, starDoc, isSaving, createDoc, duplicateDoc } = useOfficeStore()
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
  const saveTimerRef    = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const doSave = useCallback(() => {
    const ed      = activeEditorRef.current
    const content = ed ? ed.getJSON() : docRef.current
    const sec: SectionDef  = { id: sectionIdRef.current, orientation: baseOrientationRef.current, margins: marginsRef.current, columns: baseColumnsRef.current }
    const page: PageData   = { id: pageIdRef.current, sectionId: sectionIdRef.current, content }
    saveDoc(docId, { content_json: serializeDoc([sec], [page], { pageNumbers: pageNumbersRef.current, header: headerRef.current, footer: footerRef.current, hfFirstPage: hfFirstRef.current, pageColor: pageColorRef.current, pageGrad: pageGradRef.current, paperSize: paperSizeRef.current, styles: Object.keys(styleOverridesRef.current).length ? styleOverridesRef.current : undefined }) })
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

  const activeGeo = getGeometry({ id: '', orientation: activeOrientation, margins: activeMargins, columns: baseColumns }, paperSize)

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
    const { sections, pages, pageNumbers: pn, header: hdr, footer: ftr, hfFirstPage: hf1, pageColor: pc, pageGrad: pg, paperSize: ps, styles: stl } = parseDocContent(activeDoc.content_json as object | null)
    setStyleOverrides(stl ?? {}); styleOverridesRef.current = stl ?? {}
    setHeader(hdr); headerRef.current = hdr
    setFooter(ftr); footerRef.current = ftr
    setHfFirstPage(!!hf1); hfFirstRef.current = !!hf1
    setPaperSize(ps ?? 'a4'); paperSizeRef.current = ps ?? 'a4'
    setPageColor(pc); pageColorRef.current = pc
    setPageGrad(pg);  pageGradRef.current = pg
    if (sections[0]) {
      setActiveMargins(sections[0].margins)
      setActiveOrientation(sections[0].orientation)
      setBaseOrientation(sections[0].orientation)
      setBaseColumns(sections[0].columns ?? 1); baseColumnsRef.current = sections[0].columns ?? 1
      sectionIdRef.current = sections[0].id
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
  const handleExportTxt = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    const text = ed.getText({ blockSeparator: '\n' })
    downloadBlob(new Blob([text], { type: 'text/plain;charset=utf-8' }), `${titleRef2.current || 'document'}.txt`)
  }, [])
  // Export serveur (DOCX/ODT) : téléchargement AUTHENTIFIÉ via axios (les anciennes
  // navigations location.href vers /api/v1/documents/... étaient un 404 + sans token).
  const handleExportServer = useCallback(async (fmt: 'docx' | 'odt') => {
    try {
      const r = await api.get(`/office/${docId}/export/${fmt}`, { responseType: 'blob' })
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
    const isLine = kind === 'line'
    const w = 320, h = isLine ? 80 : kind === 'star' ? 220 : 200
    const params: ShapeParams = { kind, fill: '#dbe7ff', stroke: '#1a73e8' }
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
    navigate(`/office/${doc.id}`)
  }

  const handleDuplicate = useCallback(async () => {
    const d = await duplicateDoc(docId)
    navigate(`/office/${d.id}`)
  }, [duplicateDoc, docId, navigate])

  // Éditeur ciblé par la mise en forme du ruban : bande HF / zone de texte si en édition, sinon le corps.
  const fmtEditor = (tbBar && tbZoneEditor ? tbZoneEditor : hfBar && hfZoneEditor ? hfZoneEditor : activeEditor) as Editor | null
  useEditorTick(fmtEditor)
  useEditorTick(activeEditor as Editor | null)
  const ribbonFonts = useAvailableFonts()

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
    t, fmt: fmtEditor, body: activeEditor as Editor | null, fonts: ribbonFonts,
    zoom, onZoom: setZoom,
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
    onNew: handleNew, onDuplicate: handleDuplicate, onPrint: () => window.print(),
    onExportPdf: handleExportPdf, onExportTxt: handleExportTxt, onExportServer: handleExportServer,
    pageColorNode: <RibbonPageColorBtn pageColor={pageColor} pageGrad={pageGrad} onColor={onPageColorHex} onGrad={onPageGradient} />,
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
          {/* Macros (sous-module Script) */}
          <MacrosMenu docType="document" docId={docId} buildApi={makeApi} defaultLabel={title} />
          <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
          <button onClick={() => setShareOpen(true)}
            className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-primary text-white text-sm font-medium hover:bg-primary-hover transition-colors">
            <UserPlus size={15} /> {t('share_button', 'Partager')}
          </button>
        </div>
      }
      titleActions={
        <button onClick={() => starDoc(docId, !activeDoc.is_starred)}
          className="p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0"
          title={activeDoc.is_starred ? t('doc_remove_favorite') : t('doc_add_favorite')}>
          <Star size={15}
            fill={activeDoc.is_starred ? 'currentColor' : 'none'}
            className={activeDoc.is_starred ? 'text-warning' : 'text-white/90'} />
        </button>
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
        <div style={{ width: RULER_SZ, flexShrink: 0, display: 'flex', flexDirection: 'column' }}>
          <CornerCell tabType={tabType} onCycle={() => setTabType(tt => TAB_CYCLE[(TAB_CYCLE.indexOf(tt) + 1) % TAB_CYCLE.length])} />
          <VerticalRuler
            scrollRef={scrollRef}
            activePage={Math.max(0, docStats.current - 1)}
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
          <div className="flex-shrink-0 overflow-hidden bg-[#f1f3f4] border-b border-[#dadce0]" style={{ height: RULER_SZ }}>
            <div className="h-full flex justify-center">
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
      {stylesEditorOpen && (
        <StylesEditorDialog styles={styleList} initialId={curStyleId}
          onSave={handleSaveStyle} onClose={() => setStylesEditorOpen(false)} />
      )}
      {spellDictOpen && (
        <SpellDictionaryDialog onChange={() => setSpellVersion(v => v + 1)} onClose={() => setSpellDictOpen(false)} />
      )}
    </OfficeShell>
  )
}

export { DocumentEditorArea }
