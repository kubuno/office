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
} from 'lucide-react'
import { Dropdown, MenuDropdown, Button, Checkbox, ColorField, GradientField, gradientToCss, DEFAULT_GRADIENT, ColorSwatchPicker, AnchoredPopover, useAppPickerTheme } from '@ui'
import type { MenuItem, Gradient } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
import type { RibbonTab } from './ribbon/types'
import { findIssues, ignoreWord, type SpellIssue } from './spellcheck'
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
import type { DocumentLayout, PageLayout, LayoutLine } from './canvas-engine'

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
`

// ── Layout constants ──────────────────────────────────────────────────────────

// Google Docs: margin-bottom:7.5pt + margin-top:3.75pt on adjacent pages = ~15px total inter-page gap
const PAGE_GAP  = 10   // écart entre pages, logique Google Docs (~10px)
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
    pageColor: meta.pageColor, pageGrad: meta.pageGrad, paperSize: meta.paperSize ?? 'a4' }
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

// Applique une mise en forme de caractère à la sélection ET, pour les paragraphes
// VIDES de la plage, l'enregistre dans leur attribut `fontMarks` (sinon rien ne
// s'appliquerait à une sélection de lignes vides — bug Word manquant).
function applyInlineFormat(editor: Editor, patch: FontMarks) {
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
  parseHTML() { return [{ tag: 'td' }] },
  renderHTML() { return ['td', 0] },
})
const TableRowExt = TipTapNode.create({
  name: 'tableRow', content: 'tableCell+',
  parseHTML() { return [{ tag: 'tr' }] },
  renderHTML() { return ['tr', 0] },
})
const TableExt = TipTapNode.create({
  name: 'table', group: 'block', content: 'tableRow+', isolating: true,
  parseHTML() { return [{ tag: 'table' }] },
  renderHTML() { return ['table', ['tbody', 0]] },
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
    const dpr = window.devicePixelRatio || 1
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
  onMarginsChange?:   (left: number, right: number) => void
  onDragGuideChange?: (guide: { clientX: number } | null) => void
}

function HorizontalRuler({ pageW, marginLeft, marginRight, zoom, columns = 1, colGap = 0, onMarginsChange, onDragGuideChange }: HorizontalRulerProps) {
  const { t } = useTranslation('office')
  const canvasRef   = useRef<HTMLCanvasElement>(null)
  const [cursor, setCursor]   = useState('default')
  const [tooltip, setTooltip] = useState<{ x: number; label: string } | null>(null)
  const draggingRef  = useRef<'left' | 'right' | null>(null)
  const liveL        = useRef(marginLeft)
  const liveR        = useRef(marginRight)

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

    // Left margin handle — downward triangle at left boundary
    ctx.fillStyle = '#4285f4'
    ctx.beginPath()
    ctx.moveTo(mlPx - HANDLE_SZ / 2, 0)
    ctx.lineTo(mlPx + HANDLE_SZ / 2, 0)
    ctx.lineTo(mlPx, HANDLE_SZ)
    ctx.closePath()
    ctx.fill()

    // Right margin handle — downward triangle at right boundary
    ctx.beginPath()
    ctx.moveTo(w - mrPx - HANDLE_SZ / 2, 0)
    ctx.lineTo(w - mrPx + HANDLE_SZ / 2, 0)
    ctx.lineTo(w - mrPx, HANDLE_SZ)
    ctx.closePath()
    ctx.fill()

    ctx.restore()
  }, [w, h, zoom, columns, colGap])

  useLayoutEffect(() => {
    liveL.current = marginLeft
    liveR.current = marginRight
    drawRuler(marginLeft, marginRight)
  }, [drawRuler, marginLeft, marginRight])

  const getHit = useCallback((mouseX: number): 'left' | 'right' | null => {
    const mlPx = liveL.current * zoom
    const mrPx = liveR.current * zoom
    if (Math.abs(mouseX - mlPx) <= RULER_SNAP) return 'left'
    if (Math.abs(mouseX - (w - mrPx)) <= RULER_SNAP) return 'right'
    return null
  }, [zoom, w])

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (draggingRef.current) return
    const x = e.clientX - e.currentTarget.getBoundingClientRect().left
    setCursor(getHit(x) ? 'ew-resize' : 'default')
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return
    const x   = e.clientX - e.currentTarget.getBoundingClientRect().left
    const hit = getHit(x)
    if (!hit) return
    e.preventDefault()
    draggingRef.current = hit
    setCursor('ew-resize')
    const MIN_CONTENT = 96

    const onMove = (me: MouseEvent) => {
      const canvas = canvasRef.current
      if (!canvas) return
      const rulerRect = canvas.getBoundingClientRect()
      const mx = me.clientX - rulerRect.left
      if (hit === 'left') {
        const newL = Math.max(0, Math.min(pageW - MIN_CONTENT - liveR.current, mx / zoom))
        liveL.current = newL
        drawRuler(newL, liveR.current)
        setTooltip({ x: mx, label: t('doc_margin_left_cm', { value: (newL / PX_PER_CM).toFixed(2) }) })
        onDragGuideChange?.({ clientX: rulerRect.left + newL * zoom })
      } else {
        const newR = Math.max(0, Math.min(pageW - MIN_CONTENT - liveL.current, (w - mx) / zoom))
        liveR.current = newR
        drawRuler(liveL.current, newR)
        setTooltip({ x: mx, label: t('doc_margin_right_cm', { value: (newR / PX_PER_CM).toFixed(2) }) })
        onDragGuideChange?.({ clientX: rulerRect.left + (w - newR * zoom) })
      }
      onMarginsChange?.(liveL.current, liveR.current)
    }

    const onUp = () => {
      draggingRef.current = null
      setTooltip(null)
      setCursor('default')
      onDragGuideChange?.(null)
      onMarginsChange?.(liveL.current, liveR.current)
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
  scrollTop:    number
  zoom:         number
  marginTop:    number
  marginBottom: number
  pageH:        number
  pageGap:      number
  onMarginsChange?:   (top: number, bottom: number) => void
  onDragGuideChange?: (guide: { clientY: number } | null) => void
}

function VerticalRuler({ scrollTop, zoom, marginTop, marginBottom, pageH, pageGap, onMarginsChange, onDragGuideChange }: VerticalRulerProps) {
  const { t } = useTranslation('office')
  const canvasRef    = useRef<HTMLCanvasElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const [cursor, setCursor]   = useState('default')
  const [tooltip, setTooltip] = useState<{ y: number; label: string } | null>(null)
  const draggingRef   = useRef<'top' | 'bottom' | null>(null)
  const liveT         = useRef(marginTop)
  const liveB         = useRef(marginBottom)
  const scrollTopRef  = useRef(scrollTop)
  scrollTopRef.current = scrollTop

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
    const fcy   = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom + mt * zoom  // first content top in scroll-space
    const cH    = (pageH - mt - mb) * zoom                           // content height in scroll-space
    const opsh  = pageH * zoom + pageGap * zoom                      // one-page scroll height

    ctx.clearRect(0, 0, cw, ch)
    ctx.save()
    ctx.scale(dpr, dpr)

    // Gray background
    ctx.fillStyle = '#f1f3f4'
    ctx.fillRect(0, 0, w, h)

    // White zones = content areas per page
    let ps = fcy
    for (let p = 0; p < 50; p++) {
      const ys = ps - st, ye = ys + cH
      if (ys > h) break
      if (ye > 0) {
        ctx.fillStyle = '#fff'
        ctx.fillRect(0, Math.max(0, ys), w, Math.min(h, ye) - Math.max(0, ys))
      }
      ps += opsh
    }

    // Margin boundary lines
    ctx.fillStyle = '#bdc1c6'
    ps = fcy
    for (let p = 0; p < 50; p++) {
      const ys = ps - st, ye = ys + cH
      if (ys > h) break
      if (ys >= -1 && ys <= h + 1) ctx.fillRect(0, Math.round(ys), w, 1)
      if (ye >= -1 && ye <= h + 1) ctx.fillRect(0, Math.round(ye), w, 1)
      ps += opsh
    }

    // Ticks (origin = first content top)
    ctx.fillStyle = '#5f6368'
    ctx.font = '9px Arial'; ctx.textAlign = 'right'; ctx.textBaseline = 'middle'
    const startMm = Math.floor((st - fcy - 20) / pxCm * 10) - 1
    const endMm   = Math.ceil((st + h - fcy + 20) / pxCm * 10) + 1
    for (let mm = startMm; mm <= endMm; mm++) {
      if (mm % 5 !== 0) continue
      const yScr = fcy + (mm / 10) * pxCm - st
      if (yScr < -10 || yScr > h + 10) continue
      const isCm = mm % 10 === 0
      ctx.fillRect(w - (isCm ? 8 : 4), Math.round(yScr) - 0.5, isCm ? 8 : 4, 1)
      if (isCm && mm >= 0) ctx.fillText(String(mm / 10), w - 10, yScr)
    }

    // Draw margin handles on ALL visible pages
    ctx.fillStyle = '#4285f4'
    ps = fcy
    for (let p = 0; p < 50; p++) {
      const topY = ps - st         // top content boundary (screen px)
      const botY = ps + cH - st   // bottom content boundary (screen px)
      if (topY > h + HANDLE_SZ) break

      // Top handle — triangle pointing RIGHT (into content), on right edge
      if (topY > -HANDLE_SZ && topY < h + HANDLE_SZ) {
        ctx.beginPath()
        ctx.moveTo(w, topY - HANDLE_SZ / 2)
        ctx.lineTo(w, topY + HANDLE_SZ / 2)
        ctx.lineTo(w - HANDLE_SZ, topY)
        ctx.closePath()
        ctx.fill()
      }

      // Bottom handle — same shape (pointing RIGHT toward content above)
      if (botY > -HANDLE_SZ && botY < h + HANDLE_SZ) {
        ctx.beginPath()
        ctx.moveTo(w, botY - HANDLE_SZ / 2)
        ctx.lineTo(w, botY + HANDLE_SZ / 2)
        ctx.lineTo(w - HANDLE_SZ, botY)
        ctx.closePath()
        ctx.fill()
      }

      ps += opsh
    }

    ctx.restore()
  }, [zoom, pageH, pageGap])

  useLayoutEffect(() => {
    liveT.current = marginTop
    liveB.current = marginBottom
    const h = containerRef.current?.clientHeight ?? 0
    drawRuler(marginTop, marginBottom, scrollTop, h)
  }, [drawRuler, marginTop, marginBottom, scrollTop])

  // Resize observer — re-draw when container height changes
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const obs = new ResizeObserver(() => {
      drawRuler(liveT.current, liveB.current, scrollTopRef.current, el.clientHeight)
    })
    obs.observe(el)
    return () => obs.disconnect()
  }, [drawRuler])

  // Hit detection: returns which type of handle is under mouseY (screen px from ruler top)
  const getHit = useCallback((mouseY: number): 'top' | 'bottom' | null => {
    const fcy  = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom + liveT.current * zoom
    const cH   = (pageH - liveT.current - liveB.current) * zoom
    const opsh = pageH * zoom + pageGap * zoom
    const st   = scrollTopRef.current
    let ps     = fcy
    for (let p = 0; p < 50; p++) {
      const topY = ps - st
      const botY = ps + cH - st
      if (topY > (containerRef.current?.clientHeight ?? 999) + RULER_SNAP) break
      if (Math.abs(mouseY - topY) <= RULER_SNAP) return 'top'
      if (Math.abs(mouseY - botY) <= RULER_SNAP) return 'bottom'
      ps += opsh
    }
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

    // Find which page's boundary was hit → compute page origin Y (scroll-space)
    const fcy0 = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom   // page-box top without margin
    const opsh = pageH * zoom + pageGap * zoom
    const st   = scrollTopRef.current
    let pageOriginY = fcy0  // Y in scroll space of the page box top (page 0)
    {
      let ps = fcy0 + liveT.current * zoom
      for (let p = 0; p < 50; p++) {
        const topY = ps - st, botY = ps + (pageH - liveT.current - liveB.current) * zoom - st
        if (Math.abs(y - topY) <= RULER_SNAP || Math.abs(y - botY) <= RULER_SNAP) {
          pageOriginY = fcy0 + p * opsh; break
        }
        ps += opsh
      }
    }

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

function CornerCell() {
  return <div style={{ width: RULER_SZ, height: RULER_SZ, flexShrink: 0 }} className="bg-[#f1f3f4] border-r border-b border-[#dadce0]" />
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
}

// Contexte transmis à la barre contextuelle d'en-tête/pied (options Word).
export interface HFBarCtx { band: 'header' | 'footer'; secIdx: number; linked: boolean; canLink: boolean; firstPage: boolean }

function PaginatedEditor({ initialDoc, ydoc, awareness, collabEmpty, section, zoom, scrollContainerRef, onEditor, onSave, onBaseChange, onActiveSection, onRegisterOps, pageNumbers = 'none', header, footer, hfFirstPage = false, paper = 'a4', docTitle = '', pageBg, onHFActive, onCommitHF, onTbActive, spellCheck = true, onSpellCount }: PaginatedEditorProps) {
  const { t, i18n: i18nInst } = useTranslation('office')
  const g = getGeometry(section, paper)
  const cbRef = useRef({ onBaseChange, onActiveSection, onHFActive, onCommitHF, onTbActive })
  cbRef.current = { onBaseChange, onActiveSection, onHFActive, onCommitHF, onTbActive }
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
  const goalXRef            = useRef<number | null>(null)   // colonne cible pour ↑/↓

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
    onRegisterOps?.({ setOrientation, setColumns, insertBreak, insertPageBreak, pageCount, outline, scrollToPos, exportPageCanvases, hfContext, setSectionHF, setSectionBg, enterHF: (k) => enterHFEdit(k), exitHF: exitHFEdit, switchHF: switchHFBand, insertHFField, insertTextBox: insertTextBoxOp, editTextBox: enterTextBoxEdit })
    return () => onRegisterOps?.(null)
  }, [onRegisterOps, setOrientation, setColumns, insertBreak, insertPageBreak, pageCount, outline, scrollToPos, exportPageCanvases, hfContext, setSectionHF, setSectionBg, enterHFEdit, exitHFEdit, switchHFBand, insertHFField, insertTextBoxOp, enterTextBoxEdit])

  // Place le caret (curseur) sur la bonne page selon la position head de l'éditeur.
  // scrollIntoView=true (frappe/navigation) → amène le caret dans le champ de vision.
  const drawCaret = useCallback((scrollIntoView = false) => {
    const ed = editorRef.current, layout = contLayoutRef.current, caret = caretRef.current
    if (!ed || !layout || !caret) return
    // En édition en-tête/pied : le corps n'est pas édité → caret du corps masqué.
    if (hfEditRef.current) { caret.style.display = 'none'; return }
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
    // Localiser le caret via les sous-layouts de PAGE (coords locales, gèrent
    // colonnes/tableaux/sections où le y global ne suffit pas). On prend la
    // DERNIÈRE page contenant la position (priorité au début de page aux limites).
    const head = sel.head
    const idx  = pageIndexForHead(pgs, head)
    const geom = geomOf(pgs[idx])
    const cm   = posToCoords(pgs[idx]?.layout ?? layout, head)   // coords LOCALES à la page
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
        const cr = caret.getBoundingClientRect()
        const sr = sc.getBoundingClientRect()
        const M = 40
        if (cr.bottom > sr.bottom - M)      sc.scrollTop += cr.bottom - (sr.bottom - M)
        else if (cr.top < sr.top + M)       sc.scrollTop -= (sr.top + M) - cr.top
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
    let idx = 0
    for (let k = 0; k < pgs.length; k++) {
      const has = pgs[k].layout.paragraphs.some(p => p.lines.some(ln => head >= ln.pmStart && head <= ln.pmEnd))
      if (has) idx = k
    }
    const pg = pgs[idx]; if (!pg) return null
    const geom = geomOf(pg)
    const cm = posToCoords(pg.layout ?? layout, head)
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
    setRemoteCursors(next)
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
          return false
        }

        // Home / End (+ Ctrl) → début/fin de ligne VISUELLE (ou du doc) via le layout
        // canvas, car l'éditeur caché 1px n'a pas de lignes visuelles. Shift étend.
        if (event.key === 'Home' || event.key === 'End') {
          goalXRef.current = null
          const newPos = event.key === 'Home'
            ? (mod ? docStart(layout) : lineStartAt(layout, head))
            : (mod ? docEnd(layout)   : lineEndAt(layout, head))
          const anchor = event.shiftKey ? state.selection.anchor : newPos
          view.dispatch(state.tr.setSelection(TextSelection.create(state.doc, anchor, newPos)))
          event.preventDefault()
          return true
        }

        const isUp = event.key === 'ArrowUp', isDown = event.key === 'ArrowDown'
        if (!isUp && !isDown) {
          if (!['Shift', 'Control', 'Alt', 'Meta'].includes(event.key)) goalXRef.current = null
          return false
        }
        const cm = posToCoords(layout, head)
        if (goalXRef.current == null) goalXRef.current = cm.x
        const targetY = isUp ? cm.y - 2 : cm.y + cm.height + 2
        const newPos  = coordsToPos(layout, goalXRef.current, targetY)
        if (newPos === head) return true   // déjà sur la 1ʳᵉ/dernière ligne
        const anchor = event.shiftKey ? state.selection.anchor : newPos
        view.dispatch(state.tr.setSelection(TextSelection.create(state.doc, anchor, newPos)))
        event.preventDefault()
        return true
      },
    },
    onUpdate: ({ editor: ed }) => {
      recompute(ed as Editor)
      // Le contenu a changé → les positions des curseurs distants se décalent.
      recomputeRemoteCursors()
      // Une image/zone-de-texte sélectionnée a pu changer de dimensions (ex. zone de
      // texte qui auto-grandit) → recaler la barre/le cadre d'édition sur le rect.
      updateImgSel()
      recomputeBodyMiniBar()
      // Correcteur : recalcule les fautes (débit léger) puis redessine les soulignés.
      clearTimeout(spellTimer.current)
      spellTimer.current = setTimeout(() => { computeSpell(); renderAllPages() }, 350)
      clearTimeout(saveTimer.current)
      saveTimer.current = setTimeout(() => onSave((ed as Editor).getJSON()), 1200)
    },
    onSelectionUpdate: ({ editor: ed }) => {
      // Layout frais AVANT de dessiner (no-op si déjà calculé pour ce doc).
      recompute(ed as Editor)
      renderAllPages(); drawCaret(true); reportActiveSection(); updateImgSel(); recomputeBodyMiniBar()
      // Publier notre curseur en position RELATIVE Yjs (robuste aux éditions concurrentes).
      const sel = (ed as Editor).state.selection
      const head = absToRelJson(sel.head)
      if (head != null) awareness.setLocalStateField('cursor', { head, anchor: absToRelJson(sel.anchor) })
    },
  })
  editorRef.current = editor as Editor | null

  useEffect(() => { onEditor(editor as Editor | null); return () => onEditor(null) }, [editor, onEditor])

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

  const renderAllPages = useCallback(() => {
    const ed = editorRef.current
    if (!ed) return
    const dpr = window.devicePixelRatio || 1
    const sel = ed.state.selection
    const range = sel.from < sel.to ? { from: sel.from, to: sel.to } : undefined
    const spell = spellCheckRef.current && spellRef.current.length
      ? spellRef.current.map(i => ({ from: i.from, to: i.to, grammar: i.type === 'grammar' })) : undefined
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
      renderDocument(cv, pg.layout, gg.marginH, gg.marginV, dpr, z, range, focused, spell)

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

  // Ferme le panneau d'options de disposition quand l'objet est désélectionné.
  useEffect(() => { if (!imgSel) setWrapPanel(false) }, [imgSel])

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

  const onPageMouseDown = useCallback((pageIdx: number, e: React.MouseEvent) => {
    const ed = editorRef.current; if (!ed) return

    // Clic DROIT : ne pas déplacer le curseur ni collapser la sélection — le menu
    // contextuel (onPageContextMenu) gère le placement et préserve la sélection.
    if (e.button === 2) return

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

  const ctxSpellRef = useRef<SpellIssue | null>(null)
  const onPageContextMenu = useCallback((pageIdx: number, e: React.MouseEvent) => {
    e.preventDefault()
    const ed = editorRef.current; if (!ed) return
    const sel = ed.state.selection
    const pos = posFromEvent(pageIdx, e.clientX, e.clientY)
    // Faute sous le clic ? → suggestions en tête du menu contextuel.
    ctxSpellRef.current = (pos != null && spellCheckRef.current && spellRef.current.find(i => pos > i.from && pos < i.to)) || null
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
    // Suggestions : liste fréquente si dispo, sinon calculées à la demande (Hunspell).
    const sugg = spell ? (spell.suggestions.length ? spell.suggestions : (spell.type === 'spelling' ? suggestWord(spell.word) : [])) : []
    const spellItems: MenuItem[] = spell ? [
      ...(sugg.length ? sugg : [{ noSugg: true } as never]).map((s: string | { noSugg: true }) =>
        typeof s === 'string'
          ? ({ type: 'action' as const, label: s, onClick: () => ed?.chain().focus().insertContentAt({ from: spell.from, to: spell.to }, s).run() })
          : ({ type: 'action' as const, label: t('doc_spell_no_suggestion', { defaultValue: 'Aucune suggestion' }), disabled: true, onClick: () => {} })),
      ...(spell.message ? [{ type: 'action' as const, label: spell.message, disabled: true, onClick: () => {} }] : []),
      { type: 'action', label: t('doc_spell_ignore', { defaultValue: 'Ignorer' }), onClick: () => { ignoreWord(spell.type === 'grammar' ? '§rep§' + spell.word : spell.word); computeSpell(); renderAllPages() } },
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
      {/* Éditeur ProseMirror caché — reçoit toute la saisie clavier */}
      <div style={{ position: 'absolute', width: 1, height: 1, overflow: 'hidden', opacity: 0, top: 0, left: 0, pointerEvents: 'none' }}>
        <EditorContent editor={editor} />
      </div>

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

function findMatches(doc: import('@tiptap/pm/model').Node, query: string, matchCase: boolean): Array<{ from: number; to: number }> {
  if (!query) return []
  const { flat, map } = buildTextIndex(doc)
  const hay = matchCase ? flat : flat.toLowerCase()
  const needle = matchCase ? query : query.toLowerCase()
  const res: Array<{ from: number; to: number }> = []
  let i = 0
  while (true) {
    const j = hay.indexOf(needle, i)
    if (j < 0) break
    const from = map[j], last = map[j + needle.length - 1]
    if (from >= 0 && last >= 0) res.push({ from, to: last + 1 })
    i = j + needle.length
  }
  return res
}

function FindReplaceBar({ editor, onClose }: { editor: Editor; onClose: () => void }) {
  const { t } = useTranslation('office')
  const [query, setQuery]       = useState('')
  const [replaceText, setRepl]  = useState('')
  const [matchCase, setMatchCase] = useState(false)
  const [idx, setIdx]           = useState(0)
  const [, setTick]             = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => { inputRef.current?.focus() }, [])
  useEffect(() => {
    const fn = () => setTick(t => t + 1)
    editor.on('transaction', fn)
    return () => { editor.off('transaction', fn) }
  }, [editor])

  const matches = findMatches(editor.state.doc, query, matchCase)

  const go = (n: number) => {
    if (!matches.length) return
    const i = ((n % matches.length) + matches.length) % matches.length
    setIdx(i)
    const m = matches[i]
    editor.chain().setTextSelection({ from: m.from, to: m.to }).run()
  }
  const safeIdx = matches.length ? Math.min(idx, matches.length - 1) : 0
  const replaceOne = () => {
    if (!matches.length) return
    const m = matches[safeIdx]
    editor.chain().focus().insertContentAt({ from: m.from, to: m.to }, replaceText).setTextSelection(m.from + replaceText.length).run()
  }
  const replaceAll = () => {
    if (!matches.length) return
    let chain = editor.chain().focus()
    for (let k = matches.length - 1; k >= 0; k--) chain = chain.insertContentAt({ from: matches[k].from, to: matches[k].to }, replaceText)
    chain.run()
  }

  return (
    <div className="absolute top-2 right-4 z-40 bg-white rounded-lg border border-border shadow-lg p-2 flex flex-col gap-1.5"
      style={{ width: 320 }}
      onKeyDown={e => { if (e.key === 'Escape') onClose() }}>
      <div className="flex items-center gap-1">
        <input ref={inputRef} value={query} onChange={e => { setQuery(e.target.value); setIdx(0) }}
          onKeyDown={e => { if (e.key === 'Enter') go(e.shiftKey ? safeIdx - 1 : safeIdx + 1) }}
          placeholder={t('doc_find_in_document')}
          className="flex-1 min-w-0 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
        <span className="text-xs text-text-tertiary w-14 text-center flex-shrink-0">
          {matches.length ? `${safeIdx + 1} / ${matches.length}` : '0 / 0'}
        </span>
        <ToolBtn onClick={() => go(safeIdx - 1)} disabled={!matches.length} title={t('doc_previous')}><ChevronUpIcon /></ToolBtn>
        <ToolBtn onClick={() => go(safeIdx + 1)} disabled={!matches.length} title={t('doc_next')}><ChevronDown size={14} /></ToolBtn>
        <ToolBtn onClick={onClose} title={t('common_close')}><X size={14} /></ToolBtn>
      </div>
      <div className="flex items-center gap-1">
        <input value={replaceText} onChange={e => setRepl(e.target.value)}
          placeholder={t('doc_replace_with')}
          className="flex-1 min-w-0 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
        <Button variant="secondary" size="sm" onClick={replaceOne} disabled={!matches.length}
          className="px-2 text-xs flex-shrink-0">{t('doc_replace')}</Button>
        <Button variant="secondary" size="sm" onClick={replaceAll} disabled={!matches.length}
          className="px-2 text-xs flex-shrink-0">{t('doc_replace_all')}</Button>
      </div>
      <Checkbox
        checked={matchCase}
        onChange={setMatchCase}
        label={t('doc_match_case')}
        className="px-1 items-center"
        labelClassName="text-xs text-text-secondary"
      />
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
function RibbonColorBtn({ editor, kind }: { editor: Editor | null; kind: 'text' | 'highlight' }) {
  const { t } = useTranslation('office')
  const ref = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const cur = kind === 'text'
    ? (editor?.getAttributes('textStyle').color as string) || '#202124'
    : (editor?.getAttributes('highlight').color as string) || '#ffff00'
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
          onChange={hex => { if (kind === 'text') editor?.chain().focus().setColor(hex).run(); else editor?.chain().focus().setHighlight({ color: hex }).run() }}
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
  onFind: () => void; onDetails: () => void
  spellOn: boolean; onToggleSpell: () => void; spellCount: number
  onNew: () => void; onDuplicate: () => void; onPrint: () => void
  onExportPdf: () => void; onExportTxt: () => void; onExportServer: (fmt: 'docx' | 'odt') => void
  pageColorNode: React.ReactNode
  hf: HFBarCtx | null; onHFField: (tok: string) => void; onHFSwitch: () => void
  onHFFirstPage: (v: boolean) => void; onHFLinked: (v: boolean) => void; onHFClose: () => void
}

function buildDocumentRibbon(c: DocRibbonCtx): RibbonTab[] {
  const { t, fmt, body } = c
  const isA = (n: string, a?: Record<string, unknown>) => !!fmt?.isActive(n, a)
  const curSize = fmt?.getAttributes('textStyle').fontSize ? Math.round(parseFloat(String(fmt.getAttributes('textStyle').fontSize))) : 11
  const curFont = (fmt?.getAttributes('textStyle').fontFamily as string) || 'Arial'
  const curStyle = isA('heading', { level: 1 }) ? 'h1' : isA('heading', { level: 2 }) ? 'h2' : isA('heading', { level: 3 }) ? 'h3' : isA('heading', { level: 4 }) ? 'h4' : 'paragraph'
  const setSize = (n: number) => fmt && applyInlineFormat(fmt, { fs: `${Math.max(6, Math.min(96, n))}pt` })
  const curLs = (fmt?.getAttributes('paragraph').lineHeight ?? fmt?.getAttributes('heading').lineHeight ?? 1.15) as number
  const setLs = (lh: number) => fmt?.chain().focus().updateAttributes('paragraph', { lineHeight: lh }).updateAttributes('heading', { lineHeight: lh }).run()
  const indent = (d: number) => { const cur = (fmt?.getAttributes('paragraph').indent ?? 0) as number; fmt?.chain().focus().updateAttributes('paragraph', { indent: Math.max(0, Math.min(10, cur + d)) }).updateAttributes('heading', { indent: Math.max(0, Math.min(10, cur + d)) }).run() }
  const tog = (id: string, icon: React.ReactNode, label: string, mark: string, key: 'b' | 'i' | 'u' | 's') =>
    ({ id, kind: 'toggle' as const, icon, tooltip: label, active: isA(mark), onClick: () => fmt && applyInlineFormat(fmt, { [key]: !isA(mark) }) })
  const align = (a: string, icon: React.ReactNode, label: string) =>
    ({ id: 'al-' + a, kind: 'toggle' as const, icon, tooltip: label, active: !!fmt?.isActive({ textAlign: a }), onClick: () => fmt?.chain().focus().setTextAlign(a).run() })

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
        { id: 'family', kind: 'dropdown', value: curFont, width: 132, options: c.fonts.map(f => ({ value: f, label: f })), onChange: v => fmt && applyInlineFormat(fmt, { ff: v }) },
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
        { id: 'color', kind: 'custom', render: <RibbonColorBtn editor={fmt} kind="text" /> },
        { id: 'hl', kind: 'custom', render: <RibbonColorBtn editor={fmt} kind="highlight" /> },
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
        { id: 'style', kind: 'dropdown', value: curStyle, width: 120, options: [
          { value: 'paragraph', label: t('doc_normal') }, { value: 'h1', label: t('doc_heading_1') },
          { value: 'h2', label: t('doc_heading_2') }, { value: 'h3', label: t('doc_heading_3') }, { value: 'h4', label: t('doc_heading_4') },
        ], onChange: v => { if (v === 'paragraph') fmt?.chain().focus().setParagraph().run(); else fmt?.chain().focus().toggleHeading({ level: parseInt(v.replace('h', '')) as 1 | 2 | 3 | 4 }).run() } },
      ] },
      { id: 'edit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'find', kind: 'button', size: 'large', icon: <Search size={22} />, label: t('doc_find', { defaultValue: 'Rechercher' }), onClick: c.onFind },
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
      ] },
    ],
  }

  return [home, insert, layout, view, review, imageTab, hfTab]
}

// Force le re-rendu sur changement d'état de l'éditeur → le ruban (rebâti à chaque
// rendu) reflète les états actifs (gras, alignement, police/taille courantes…).
function useEditorTick(editor: Editor | null) {
  const [, set] = useState(0)
  useEffect(() => {
    if (!editor) return
    const on = () => set(n => (n + 1) & 0xffff)
    editor.on('transaction', on); editor.on('selectionUpdate', on)
    return () => { editor.off('transaction', on); editor.off('selectionUpdate', on) }
  }, [editor])
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
  const [findOpen, setFindOpen]                   = useState(false)
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
  const [detailsOpen, setDetailsOpen]             = useState(false)
  const [pageColor, setPageColor]                 = useState<string | undefined>(undefined)
  const [pageGrad,  setPageGrad]                  = useState<Gradient | undefined>(undefined)
  const pageColorRef = useRef<string | undefined>(undefined); pageColorRef.current = pageColor
  const pageGradRef  = useRef<Gradient | undefined>(undefined); pageGradRef.current = pageGrad
  // CSS background appliqué à chaque page (dégradé prioritaire sur couleur unie).
  const pageBgCss = pageGrad ? gradientToCss(pageGrad) : pageColor
  const imageFileRef = useRef<HTMLInputElement>(null)
  const titleRef2 = useRef(''); titleRef2.current = title

  // Ctrl/⌘+F (rechercher) et Ctrl/⌘+H (remplacer) ouvrent la barre de recherche.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'h')) {
        e.preventDefault()
        setFindOpen(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const [zoom, setZoom]                           = useState(1)
  const [activeEditor, setActiveEditor]           = useState<Editor | null>(null)
  // activeOrientation = AFFICHAGE (section où est le curseur) ; baseOrientation =
  // section de base du document (persistée, passée à PaginatedEditor).
  const [activeOrientation, setActiveOrientation] = useState<Orientation>('portrait')
  const [baseOrientation, setBaseOrientation]     = useState<Orientation>('portrait')
  const [baseColumns, setBaseColumns]             = useState(1)
  const [activeMargins, setActiveMargins]         = useState<SectionDef['margins']>({ top: 96, right: 96, bottom: 96, left: 96 })
  const [dragGuide, setDragGuide]                 = useState<DragGuide>(null)
  const scrollRef                                 = useRef<HTMLDivElement>(null)
  const [scrollTop, setScrollTop]                 = useState(0)
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
    saveDoc(docId, { content_json: serializeDoc([sec], [page], { pageNumbers: pageNumbersRef.current, header: headerRef.current, footer: footerRef.current, hfFirstPage: hfFirstRef.current, pageColor: pageColorRef.current, pageGrad: pageGradRef.current, paperSize: paperSizeRef.current }) })
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
  // Mode lecture : éditeur en lecture seule (barre d'outils/règles masquées au rendu).
  useEffect(() => { activeEditor?.setEditable(mode === 'edit') }, [activeEditor, mode])
  useEffect(() => {
    if (!activeDoc) return
    setTitle(activeDoc.title)
    // Initialise margins, orientation et ids stables depuis le document stocké.
    const { sections, pages, pageNumbers: pn, header: hdr, footer: ftr, hfFirstPage: hf1, pageColor: pc, pageGrad: pg, paperSize: ps } = parseDocContent(activeDoc.content_json as object | null)
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

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => setScrollTop(e.currentTarget.scrollTop)

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
    onFind: () => setFindOpen(true), onDetails: () => setDetailsOpen(true),
    spellOn, onToggleSpell: () => setSpellOn(v => !v), spellCount,
    onNew: handleNew, onDuplicate: handleDuplicate, onPrint: () => window.print(),
    onExportPdf: handleExportPdf, onExportTxt: handleExportTxt, onExportServer: handleExportServer,
    pageColorNode: <RibbonPageColorBtn pageColor={pageColor} pageGrad={pageGrad} onColor={onPageColorHex} onGrad={onPageGradient} />,
    hf: hfBar,
    onHFField: tok => opsRef.current?.insertHFField(tok),
    onHFSwitch: () => opsRef.current?.switchHF(),
    onHFFirstPage: setHfFirstPageOpt, onHFLinked: setHfLinkedOpt,
    onHFClose: () => opsRef.current?.exitHF(),
  })

  return (
    <OfficeShell
      ribbon={ribbon}
      chromeless
      topbarHeight={64}
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

        {findOpen && activeEditor && (
          <FindReplaceBar editor={activeEditor as Editor} onClose={() => setFindOpen(false)} />
        )}
        {specialOpen && activeEditor && (
          <SpecialCharsBar editor={activeEditor as Editor} onClose={() => setSpecialOpen(false)} />
        )}

        {navOpen && (
          <NavPane editor={activeEditor} opsRef={opsRef} onClose={() => setNavOpen(false)} />
        )}

        {showRuler && mode === 'edit' && (
        <div style={{ width: RULER_SZ, flexShrink: 0, display: 'flex', flexDirection: 'column' }}>
          <CornerCell />
          <VerticalRuler
            scrollTop={scrollTop}
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
            onScroll={handleScroll}
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
              />
            </div>
          </div>
        </div>
      </div>

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
    </OfficeShell>
  )
}

export { DocumentEditorArea }
