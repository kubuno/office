import { useEffect, useLayoutEffect, useCallback, useRef, useState, useMemo, Fragment, startTransition } from 'react'
import { type ShapeKind } from './shapes/catalog'
export type { ShapeKind } from './shapes/catalog'
// Shapes: the `kbshape:` payload, the draw-to-create gesture, the live preview,
// the yellow knobs and the ribbon surfaces all live in `documents/shapes/` —
// this file is far too big to grow a drawing subsystem inside it. Everything
// there is a thin layer over the SHARED `src/shapes/` primitives.
import {
  type DocShapeParams, svgToDataUrl, shapeAlt, parseShapeAlt, shapeSrcOf,
} from './documents/shapes/params'
import { beginShapeDraw, beginShapePointerDraw, insertDrawnShape } from './documents/shapes/draw-tool'
import { ShapeGhostLayer, type ShapeGhostHandle } from './documents/shapes/ShapeGhostLayer'
import type { DrawBox } from './shapes/draw'
import { ShapeAdjustHandles } from './documents/shapes/ShapeAdjustHandles'
import { docIllustrationsGroup, docShapeRibbonTab } from './documents/shapes/ribbon'
import type { ShapeOrderOp } from './shapes/shapeRibbonTab'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
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
  yUndoPluginKey,
  type ProsemirrorBinding,
} from '@tiptap/y-tiptap'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { PresenceAvatars, userColor, usePublishCursor, RemoteCursors } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import { openShare } from './shareSdk'
import { DLG_BTN } from './lib'
// Word fields live in their own module (this file is already far too big):
// `FieldExt` is the TipTap `field` node, `refreshFields` recomputes cached results.
import { FieldExt, refreshFields, refsFromBookmarks } from './documents/fields'
// Endnotes (Word « note de fin ») : model+commands in `documents/endnotes.ts`.
import { EndnoteExt, insertEndnote } from './documents/endnotes'
import { useEndnoteEditor } from './documents/EndnoteDialog'
// Track changes (Word « Suivi des modifications ») : marks + plugin in `documents/track-changes.ts`.
import { TrackChangesExt, setTrackChangesEnabled, setTrackChangesUser } from './documents/track-changes'
import { buildReferencesTab, type ReferencesRibbonCtx, type TocPreset } from './documents/references/ribbon'
import { TocControl, type TocControlRect } from './documents/references/TocControl'
import { UpdateTocDialog, type TocUpdateMode } from './documents/references/UpdateTocDialog'
import { ReferencesDialog } from './documents/references/ReferencesDialog'
import { SourcesDialog } from './documents/references/SourcesDialog'
import { MarkEntryDialog, type MarkMode } from './documents/references/MarkEntryDialog'
import { CrossRefDialog, type CrossRefTarget, type CrossRefWhat } from './documents/references/CrossRefDialog'
import { collectOutline, currentOutlineLevel, outlineLevelOf, setOutlineLevel } from './documents/references/outline'
import { pickObject, type HitObject } from './documents/layout/object-hit'
import {
  resolveObjectPosition, horizontalArea, verticalArea, makePageGeom,
  type AnchorCtx, type PageGeom, type RelH, type RelV, type AlignH, type AlignV,
} from './documents/layout/anchor-position'
import { gotoNote, hasNotes, noteToShow, type NoteDirection, type NoteKind } from './documents/references/notes-nav'
import {
  referenceEntryExtensions, collectIndexHits, collectCitationHits,
  markIndexEntry, markCitation, selectedText,
} from './documents/references/entries'
import {
  buildTable, tocEntries, figureEntries, indexEntries, authorityEntries,
} from './documents/references/generate'
import { bibliographyEntry, citationText, sortSources, type Source } from './documents/references/sources'
import {
  DEFAULT_REFERENCES, type AuthorityCategory, type ReferencesSettings, type TableKind, type TocSettings,
} from './documents/references/types'
import { ReviewPane } from './documents/ReviewPane'
import * as DocIcon from './documents/ribbon-icons'
import { captureFormat, applyFormat, PAINT_CURSOR, type CapturedFormat } from './documents/format-painter'
import { suppressTableSelectionWarning } from './documents/pm-warning-filter'
import { TextEffect } from './documents/text-effects/model'
import { TextEffectsButton } from './documents/text-effects/ribbon'

// Installé au chargement du module (avant tout montage d'éditeur) : neutralise
// l'avertissement bénin « TextSelection endpoint not pointing… (table) » émis par
// y-prosemirror pendant la construction de l'éditeur.
suppressTableSelectionWarning()
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
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignLeft, AlignCenter, AlignRight, AlignJustify,
  Link as LinkIcon, Highlighter,
  FileText, RotateCcw, RotateCw,
  Minus, Plus, Star, UserPlus,
  IndentIncrease, IndentDecrease, Image as ImageIcon, ChevronDown, X,
  LayoutTemplate,
  Scissors, Copy, ClipboardPaste, Table as TableIcon, Square, Hash,
  Eye, PenLine, Mic, Settings as SettingsIcon, Delete as DeleteIcon, Keyboard, ChevronLeft,
  Ruler as RulerIcon, PanelLeft, Sigma, ListTree, ListChecks,
  SplitSquareVertical, Superscript, Subscript, SpellCheck,
  MessageSquare, MessageSquarePlus, Check, Trash2, Send, CornerDownRight,
  Rows3, Columns3, Combine, Paintbrush, Pencil, BookMarked,
  Languages, Accessibility, BookOpen, SlidersHorizontal, Monitor, Volume2,
  ZoomIn, MoveHorizontal, Files, CloudOff,
  Stamp, SquareDashed,
  CaseSensitive, CalendarClock, ArrowDownAZ, ArrowUpAZ, Bookmark, Pilcrow, Frame, Quote, WrapText, Omega,
  GripHorizontal, ArrowDownWideNarrow, Pin, Move, ChevronRight, PlaySquare, Info,
} from 'lucide-react'
import { Dropdown, MenuDropdown, Button, Checkbox, Radio, Toggle, NumberInput, ColorField, GradientField, gradientToCss, DEFAULT_GRADIENT, ColorSwatchPicker, AnchoredPopover, RangeSlider, FontPicker, FontSizeField, FloatingWindow, Tabs, useAppPickerTheme, useIsMobile, useSaveShortcut } from '@ui'
import type { MenuItem, Gradient } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { ShareButton } from './ribbon/ShareButton'
import { UndoRedoButtons } from './ribbon/UndoRedoButtons'
import { Backstage } from './ribbon/Backstage'
import { clipboardGroup } from './ribbon/clipboardGroup'
import { useDocumentsBackstageSections } from './DocumentsBackstage'
import { WORKSPACE_OFFICE } from '@kubuno/sdk'
import { MacrosDialog } from './macros/MacrosDialog'
import { useDocumentMacros } from './macros/useDocumentMacros'
import {
  startRecording, pauseRecording, resumeRecording, stopRecording,
  onRecorderState, recorderState, type RecorderState,
} from './macros/recorder'
import type { RibbonTab } from './ribbon/types'
import { findIssues, ignoreWord, ignoreWordSession, unignoreWord, personalDictionary, grammarIgnoreKey, GRAMMAR_EXPLAIN, GRAMMAR_RULES, type SpellIssue } from './spellcheck'
import { loadSpeller, onSpellerReady, suggestWord, setActiveSpellLang, availableSpellLangs } from './hunspell'
import { loadSystemFonts } from './systemAssets'
import { pickImageSrc } from './imagePicker'
import { prompt, api } from '@kubuno/sdk'
import { i18n } from '@kubuno/sdk'
import { useSearchStore } from '@kubuno/sdk'
import { startVoiceSession, type VoiceSession, type VoiceErrorCode } from '@kubuno/sdk'
import { create } from 'zustand'
import { useOfficeStore } from './store'
import { readKubunoData, resolveDataCardEntry } from './kubunoData'
import { fontsApi, officeApi, type CollabPermission } from './api'
import { pagesToPdf, downloadBlob } from './pdfExport'
import { TextSelection, NodeSelection, Plugin } from '@tiptap/pm/state'
import {
  layoutDocumentMulti, layoutDocument, renderDocument, paintLayoutAt, posToCoords, coordsToPos, adjacentLineCenter,
  wordBoundariesAt, paragraphBoundariesAt, selectionRects,
  lineStartAt, lineEndAt, docStart, docEnd,
  paginateMulti, splitFloatingImagesAcrossPages, isFloatingWrap, pageFootnotes, parseRichTextBox, RICH_TB_PAD, setShapeSrcResolver,
  setRevisionDisplay, setCollapsibleHeadings, getCollapsibleHeadings,
} from './canvas-engine'
import type { DocumentLayout, PageLayout, LayoutLine, CursorMetrics, SelectionRect } from './canvas-engine'

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
    border-left: 1px solid #1a73e8;
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
const RULER_SZ  = 20   // hauteur/largeur VISIBLE des règles
const RULER_OVERHANG = 5   // les repères de retrait DÉBORDENT sous la règle horizontale (façon Word)
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
      // Chaque section porte SES marges (attrs top/right/bottom/left du nœud
      // sectionBreak) ; à défaut (attr absent), on retombe sur la section de base.
      const mAttr = (k: 'top' | 'right' | 'bottom' | 'left') =>
        typeof a[k] === 'number' ? (a[k] as number) : base.margins[k]
      geoms.push(getGeometry({
        id: '',
        orientation: (a.orientation as Orientation) ?? 'portrait',
        margins: { top: mAttr('top'), right: mAttr('right'), bottom: mAttr('bottom'), left: mAttr('left') },
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

// Réglages de page de la section de BASE stockés dans une Y.Map du même Y.Doc
// (`pageSetup`) → suivis par l'UndoManager Yjs = ANNULABLES (Ctrl+Z) dans le même
// fil que le texte + synchronisés en collaboration. `PAGE_SETUP_ORIGIN` = origine
// SUIVIE (changement utilisateur, annulable) ; `PAGE_INIT_ORIGIN` = seed initial
// NON suivi (ne crée pas d'entrée d'annulation). Les marges des sections de saut,
// elles, vivent dans les attrs du nœud `sectionBreak` (déjà annulables).
const PAGE_SETUP_ORIGIN = 'kb-page-setup'
const PAGE_INIT_ORIGIN  = 'kb-page-init'

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
  headingNumbers?: boolean            // numérotation automatique des titres (1., 1.1, …)
  spell?: SpellSettings               // réglages de vérification (langue/auto/ortho/grammaire) DU DOCUMENT
  /** Comment threads, mirrored out of the Yjs map so the server can export them. */
  comments?: CommentThread[]
  /** « Paires et impaires différentes » — a DOCUMENT setting, as in Word. */
  evenOdd?: boolean
  /** Track changes ON at open — Word's `w:trackChanges` of `settings.xml`. */
  trackChanges?: boolean
  /** « Références » tab: table settings, bibliographic sources, citation style. */
  refSettings?: ReferencesSettings
  sources?: Source[]
  citationStyle?: string
  /** Header and footer used on even pages when `evenOdd` is set. */
  headerEven?: HFContent
  footerEven?: HFContent
}
// Réglages de vérification persistés DANS le fichier (par document, façon Word).
interface SpellSettings { lang?: string; auto?: boolean; on?: boolean; grammar?: boolean; rules?: Record<string, boolean> }
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
    // `phantom` lines are INJECTED fragments (repeated table headers, the
    // endnote block): counting them numbers the same line twice.
    for (const ln of para.lines) { if (ln.image || ln.cellX != null || ln.phantom) continue; n++ }
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
      comments: (r.comments as CommentThread[]) ?? undefined,
      evenOdd: !!r.evenOdd,
      trackChanges: !!r.trackChanges,
      headerEven: r.headerEven ? toHFContent(r.headerEven) : undefined,
      footerEven: r.footerEven ? toHFContent(r.footerEven) : undefined,
      headingNumbers: !!r.headingNumbers,
      spell: (r.spell as SpellSettings) ?? undefined,
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
    lineNumbers: meta.lineNumbers ?? null, pageNumFormat: meta.pageNumFormat ?? 'arabic', pageNumStart: meta.pageNumStart ?? 1,
    headingNumbers: meta.headingNumbers || undefined, spell: meta.spell,
    // Comment threads live in the collaborative document, not in the
    // ProseMirror tree. Persisting them here is what lets the server export
    // them to DOCX at all — it cannot read the Yjs update log.
    comments: meta.comments && meta.comments.length ? meta.comments : undefined,
    // Was silently lost on save until now: the dialogue offered the setting and
    // the next reload forgot it.
    evenOdd: meta.evenOdd || undefined,
    trackChanges: meta.trackChanges || undefined,
    refSettings: meta.refSettings, sources: meta.sources?.length ? meta.sources : undefined,
    citationStyle: meta.citationStyle && meta.citationStyle !== 'APA' ? meta.citationStyle : undefined,
    headerEven: meta.headerEven, footerEven: meta.footerEven }
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
        // Uniquement dans un bloc VIDE. Au milieu d'un paragraphe qui CONTIENT du texte,
        // l'absence de marques EST la mise en forme (police/taille par défaut) : hériter
        // du dernier texte marqué du document (ex. le titre en 23 pt, des pages plus haut)
        // imposait sa taille au caret ET à la frappe après un simple clic dans le corps.
        if (sel.$from.parent.content.size > 0) return null
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
  for (let d = $f.depth; d >= 0; d--) if (outlineLevelOf($f.node(d)) > 0) return $f.before(d)
  let found = -1
  ed.state.doc.descendants((node, pos) => { if (outlineLevelOf(node) > 0 && pos < $f.pos) found = pos })
  return found >= 0 ? found : null
}
function setHeadingCollapsed(ed: Editor, pos: number, val: boolean): void {
  const node = ed.state.doc.nodeAt(pos)
  if (!node || outlineLevelOf(node) === 0) return
  ed.view.dispatch(ed.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, collapsed: val }))
}
function setAllHeadingsCollapsed(ed: Editor, val: boolean): void {
  const tr = ed.state.tr
  ed.state.doc.descendants((node, pos) => {
    if (outlineLevelOf(node) > 0) tr.setNodeMarkup(pos, undefined, { ...node.attrs, collapsed: val })
  })
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
        // Lettrine (Word « Lettrine » > Dans le texte) : grande initiale habillée.
        dropCap:             boolAttr('dropCap'),
        // Table des matières : marqueur du titre du bloc, niveau d'entrée, numéro de
        // page affiché (aligné à droite par le moteur) et points de suite.
        tocTitle:            boolAttr('tocTitle'),
        tocLevel: {
          default: null,
          parseHTML: (el: HTMLElement) => (el.dataset.tocLevel != null ? Number(el.dataset.tocLevel) : null),
          renderHTML: (a: Record<string, unknown>) => (a.tocLevel != null ? { 'data-toc-level': String(a.tocLevel) } : {}),
        },
        tocPage: {
          default: null,
          parseHTML: (el: HTMLElement) => (el.dataset.tocPage != null ? Number(el.dataset.tocPage) : null),
          renderHTML: (a: Record<string, unknown>) => (a.tocPage != null ? { 'data-toc-page': String(a.tocPage) } : {}),
        },
        tocLeader:           boolAttr('tocLeader'),
        // An index or a table of authorities lists SEVERAL pages for one entry
        // (« 5, 7 », « passim »), so the printed page is a string, not a number.
        tocPageText: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.tocPageText || null,
          renderHTML: (a: Record<string, unknown>) => (a.tocPageText ? { 'data-toc-page-text': String(a.tocPageText) } : {}),
        },
        // Leader drawn between the entry and its page: dots (Word's default),
        // dashes or a continuous underline.
        tocLeaderKind: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.tocLeaderKind || null,
          renderHTML: (a: Record<string, unknown>) => (a.tocLeaderKind ? { 'data-toc-leader-kind': String(a.tocLeaderKind) } : {}),
        },
        // Which generated table this block belongs to: 'toc', 'figures',
        // 'index' or 'authorities'. Without it, updating the index would
        // replace the table of contents — they share the same paragraph shape.
        tocKind: {
          default: null,
          parseHTML: (el: HTMLElement) => el.dataset.tocKind || null,
          renderHTML: (a: Record<string, unknown>) => (a.tocKind ? { 'data-toc-kind': String(a.tocKind) } : {}),
        },
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

// Langue d'une PLAGE de texte (Word « Définir la langue » sur une sélection) : marque
// inline portant un code langue 2 lettres. Le vérificateur d'orthographe l'emploie pour
// contrôler ce passage dans SA langue (cf. computeSpell). N'affecte pas le rendu visuel.
const SpellLangMark = TipTapMark.create({
  name: 'spellLang',
  inclusive: false,
  excludes: '',     // coexiste avec les autres marques (gras, etc.)
  addAttributes() {
    return { lang: { default: null, parseHTML: (el: HTMLElement) => el.dataset.spellLang || null, renderHTML: (a: Record<string, unknown>) => (a.lang ? { 'data-spell-lang': String(a.lang) } : {}) } }
  },
  parseHTML() { return [{ tag: 'span[data-spell-lang]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', HTMLAttributes, 0] },
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
      // Un PARAGRAPHE promu (niveau hiérarchique 1..9) est repliable comme un
      // titre chez Word : l'attribut vit donc sur les deux types de bloc.
      types: ['paragraph', 'heading'],
      attributes: {
        // État courant du repli — jamais écrit dans un .docx : Word rouvre un
        // document DÉVELOPPÉ, sauf pour les titres marqués « Réduire par défaut ».
        collapsed: {
          default: false,
          parseHTML: (el: HTMLElement) => el.dataset.collapsed === '1',
          renderHTML: (a: Record<string, unknown>) => (a.collapsed ? { 'data-collapsed': '1' } : {}),
        },
        // Word « Réduire par défaut » (dialogue Paragraphe) = `<w15:collapsed/>`
        // dans le `w:pPr` du titre. C'est CE drapeau qui voyage avec le fichier.
        collapsedDefault: {
          default: false,
          parseHTML: (el: HTMLElement) => el.dataset.collapsedDefault === '1',
          renderHTML: (a: Record<string, unknown>) => (a.collapsedDefault ? { 'data-collapsed-default': '1' } : {}),
        },
      },
    }]
  },
})

/**
 * Default object↔text distance on the LEFT and RIGHT of a floating object, in
 * document px. Writer falls back to 319 (1/100 mm) ≈ 12.06 px
 * (GraphicImport.cxx:346-349) and every Word file of the corpus encodes
 * 114300 EMU = exactly 12 px. Top and bottom default to 0 in both.
 */
const WRAP_DIST_SIDE = 12

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
      // Kubuno metadata (re-editable SVG shapes / text zones): 'kbshape:…',
      // 'kbtext:…', or a pasted cross-module JSON envelope 'kbenvelope:…' —
      // otherwise standard alt text.
      alt:      { default: null },
      // Zone de texte riche : couleur de remplissage / de bordure (null = défaut).
      tbFill:   { default: null },
      tbStroke: { default: null },
      // ── Options de disposition avancées (dialogue « Mise en page », façon Word) ──
      // Habillage carré : côté où le texte s'écoule + distances objet↔texte (px doc).
      wrapSide: { default: 'both' },     // both | left | right | largest
      // Fallback distances, aligned on Writer's: 0 top/bottom and 319 (1/100 mm)
      // left/right, i.e. 12.06 px (GraphicImport.cxx:346-349). Word files encode
      // 114300 EMU = exactly 12 px, so 12 matches both. Only objects created HERE
      // get it: the DOCX reader writes the attribute only when the file gives one.
      wrapDistT:{ default: 0 }, wrapDistB:{ default: 0 },
      wrapDistL:{ default: WRAP_DIST_SIDE }, wrapDistR:{ default: WRAP_DIST_SIDE },
      // Référentiels de position (affichage façon Word) + options d'ancrage.
      posHRel:  { default: 'column' },   // column | margin | page | character
      posVRel:  { default: 'paragraph' },// paragraph | margin | page | line
      moveWithText:  { default: true },
      allowOverlap:  { default: true },
      lockAnchor:    { default: false },
      // Alignement relatif au référentiel (Word : `wp:align`, exclusif de
      // l'offset). Absent = position manuelle par wrapX/wrapY.
      alignH:   { default: null },       // left | center | right | inside | outside
      alignV:   { default: null },       // top | center | bottom | inside | outside
      // Ordre de PLAN (`wp:anchor/@relativeHeight`) : décide quel objet gagne le
      // clic et lequel est peint au-dessus quand deux se superposent.
      zOrder:   { default: null },
      // Polygone d'habillage du fichier (`wp:wrapPolygon`), points en px dans la
      // boîte de l'objet. Absent = contour déduit de l'image, sinon boîte.
      wrapPolygon: { default: null },
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
// Sérialisation (`kbshape:`), génération du SVG et valeurs d'ajustement :
// `documents/shapes/params.ts`. La galerie, elle, est la galerie PARTAGÉE du
// module office (`shapes/ShapeGallery`) — plus de vignettes maison ici.
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
// Permet au moteur canvas de régénérer le `src` SVG des formes importées (qui ne
// portent que l'alt `kbshape:…`, ex. depuis DOCX) sans dupliquer `shapeSvg` côté moteur.
// Les valeurs d'ajustement du payload (`adj`) sont transmises telles quelles : le
// rendu suit donc les poignées jaunes à toute résolution.
setShapeSrcResolver((alt, w, h) => {
  const sp = parseShapeAlt(alt)
  if (!sp) return null
  return shapeSrcOf(sp, w || 240, h || 180)
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
      // Bordures par CÔTÉ (galerie Bordures façon Word) : { t, b, l, r } où chaque
      // côté vaut { w, s, c } (bordure explicite), null (« aucune », qui bat le
      // défaut du tableau) ou est absent (hériter du défaut du tableau).
      cellBorders: { default: null, parseHTML: (el: HTMLElement) => { try { return JSON.parse(el.dataset.cb || 'null') } catch { return null } }, renderHTML: (a: Record<string, unknown>) => (a.cellBorders ? { 'data-cb': JSON.stringify(a.cellBorders) } : {}) },
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
      // Algorithme de disposition (w:tblLayout) : 'autofit' (défaut, comme Word — les
      // colonnes suivent le contenu et s'élargissent en prenant sur les voisines) ou
      // 'fixed' (« Largeur de colonne fixe » : largeurs respectées, texte renvoyé).
      tableLayout:    { default: 'autofit', parseHTML: (el: HTMLElement) => (el.dataset.tlayout === 'fixed' ? 'fixed' : 'autofit'), renderHTML: (a: Record<string, unknown>) => (a.tableLayout === 'fixed' ? { 'data-tlayout': 'fixed' } : {}) },
      // Marges intérieures de cellule du tableau (Word : Propriétés → Options…), en px.
      // Absentes = valeurs par défaut du moteur (6 px horizontal, 2 px vertical).
      // Espacement entre cellules (w:tblCellSpacing), px. 0 = cellules jointives.
      cellSpacing:      { default: null, parseHTML: (el: HTMLElement) => (el.dataset.csp != null ? Number(el.dataset.csp) : null), renderHTML: (a: Record<string, unknown>) => (a.cellSpacing ? { 'data-csp': String(a.cellSpacing) } : {}) },
      cellMarginTop:    { default: null, parseHTML: (el: HTMLElement) => (el.dataset.cmt != null ? Number(el.dataset.cmt) : null), renderHTML: (a: Record<string, unknown>) => (a.cellMarginTop    != null ? { 'data-cmt': String(a.cellMarginTop) } : {}) },
      cellMarginBottom: { default: null, parseHTML: (el: HTMLElement) => (el.dataset.cmb != null ? Number(el.dataset.cmb) : null), renderHTML: (a: Record<string, unknown>) => (a.cellMarginBottom != null ? { 'data-cmb': String(a.cellMarginBottom) } : {}) },
      cellMarginLeft:   { default: null, parseHTML: (el: HTMLElement) => (el.dataset.cml != null ? Number(el.dataset.cml) : null), renderHTML: (a: Record<string, unknown>) => (a.cellMarginLeft   != null ? { 'data-cml': String(a.cellMarginLeft) } : {}) },
      cellMarginRight:  { default: null, parseHTML: (el: HTMLElement) => (el.dataset.cmr != null ? Number(el.dataset.cmr) : null), renderHTML: (a: Record<string, unknown>) => (a.cellMarginRight  != null ? { 'data-cmr': String(a.cellMarginRight) } : {}) },
      // Distances au texte environnant quand le tableau est flottant (habillage
      // « Autour ») — Word : Propriétés → Position…
      wrapDistTop:    { default: null, parseHTML: (el: HTMLElement) => (el.dataset.wdt != null ? Number(el.dataset.wdt) : null), renderHTML: (a: Record<string, unknown>) => (a.wrapDistTop    != null ? { 'data-wdt': String(a.wrapDistTop) } : {}) },
      wrapDistBottom: { default: null, parseHTML: (el: HTMLElement) => (el.dataset.wdb != null ? Number(el.dataset.wdb) : null), renderHTML: (a: Record<string, unknown>) => (a.wrapDistBottom != null ? { 'data-wdb': String(a.wrapDistBottom) } : {}) },
      wrapDistLeft:   { default: null, parseHTML: (el: HTMLElement) => (el.dataset.wdl != null ? Number(el.dataset.wdl) : null), renderHTML: (a: Record<string, unknown>) => (a.wrapDistLeft   != null ? { 'data-wdl': String(a.wrapDistLeft) } : {}) },
      wrapDistRight:  { default: null, parseHTML: (el: HTMLElement) => (el.dataset.wdr != null ? Number(el.dataset.wdr) : null), renderHTML: (a: Record<string, unknown>) => (a.wrapDistRight  != null ? { 'data-wdr': String(a.wrapDistRight) } : {}) },
      // Habillage du texte (Propriétés du tableau) : 'none' | 'around' — un tableau
      // plus étroit que la zone devient flottant, le texte coule à côté.
      tableWrap:      { default: 'none', parseHTML: (el: HTMLElement) => el.dataset.twrap || 'none', renderHTML: (a: Record<string, unknown>) => (a.tableWrap && a.tableWrap !== 'none' ? { 'data-twrap': String(a.tableWrap) } : {}) },
      // Répéter la rangée 0 en haut de chaque page (Word « ligne d'en-tête »).
      headerRepeat:   { default: false, parseHTML: (el: HTMLElement) => el.dataset.hrepeat === 'true', renderHTML: (a: Record<string, unknown>) => (a.headerRepeat ? { 'data-hrepeat': 'true' } : {}) },
      // Nombre de rangées épinglées répétées en haut des pages suivantes (« épingler
      // l'en-tête jusqu'à cette ligne »). 0 = aucune ; repli sur headerRepeat (= 1).
      headerRows:     { default: 0, parseHTML: (el: HTMLElement) => Number(el.dataset.hrows) || (el.dataset.hrepeat === 'true' ? 1 : 0), renderHTML: (a: Record<string, unknown>) => (Number(a.headerRows) > 0 ? { 'data-hrows': String(a.headerRows) } : {}) },
      // Bordures personnalisées : couleur / épaisseur (px) / style de trait.
      tableBorderColor: { default: null, parseHTML: (el: HTMLElement) => el.dataset.bcolor || null, renderHTML: (a: Record<string, unknown>) => (a.tableBorderColor ? { 'data-bcolor': String(a.tableBorderColor) } : {}) },
      tableBorderWidth: { default: null, parseHTML: (el: HTMLElement) => (el.dataset.bwidth ? Number(el.dataset.bwidth) : null), renderHTML: (a: Record<string, unknown>) => (a.tableBorderWidth ? { 'data-bwidth': String(a.tableBorderWidth) } : {}) },
      tableBorderStyle: { default: null, parseHTML: (el: HTMLElement) => el.dataset.bstyle || null, renderHTML: (a: Record<string, unknown>) => (a.tableBorderStyle ? { 'data-bstyle': String(a.tableBorderStyle) } : {}) },
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

// ── Police avancée (Word) : petites majuscules + espacement des caractères ──────
// Portés par la marque textStyle (mêmes attrs lus par le moteur canvas).
const TextStyleAdvanced = TextStyle.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      // Petites majuscules (font-variant small-caps).
      smallCaps: {
        default: null,
        parseHTML: (el: HTMLElement) => (el.style.fontVariant?.includes('small-caps') ? true : null),
        renderHTML: (a: Record<string, unknown>) => (a.smallCaps ? { style: 'font-variant: small-caps' } : {}),
      },
      // Espacement des caractères (pt ; + = étendu, − = condensé).
      letterSpacing: {
        default: null,
        parseHTML: (el: HTMLElement) => (el.style.letterSpacing ? parseFloat(el.style.letterSpacing) || null : null),
        renderHTML: (a: Record<string, unknown>) => (a.letterSpacing ? { style: `letter-spacing: ${a.letterSpacing}pt` } : {}),
      },
    }
  },
})

// ── Note de bas de page (Word « Insérer une note de bas de page ») ─────────────
// Atom inline : appel numéroté automatiquement par le moteur canvas (ordre du
// document) ; le TEXTE de la note vit dans l'attribut et se rend au bas de la page.
const FootnoteExt = TipTapNode.create({
  name: 'footnote',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  addAttributes() {
    return {
      text: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-fn-text') || '',
        renderHTML: (a: Record<string, unknown>) => ({ 'data-fn-text': String(a.text ?? '') }),
      },
    }
  },
  parseHTML() { return [{ tag: 'sup[data-footnote]' }] },
  renderHTML({ HTMLAttributes }) { return ['sup', { ...HTMLAttributes, 'data-footnote': '' }, '†'] },
})

// ── Socle commun de mise en forme (le « cœur » riche partagé) ───────────────────
// Capacités identiques pour le corps de page, l'en-tête/pied et les zones de texte :
// interligne/indentation/espacement, héritage de marques sur lignes vides, exposant/
// indice, images, tableaux, souligné, liens, listes de tâches, alignement, surlignage,
// couleur/police/taille, comptage. Le corps de page SURCHARGE ce socle (sauts de
// section/page + Placeholder + Collaboration Yjs) ; la RichEditZone l'utilise tel quel
// avec son propre undo. Garder l'ORDRE (priorité/schéma ProseMirror) inchangé.
const BASE_DOC_EXTENSIONS = [
  ...referenceEntryExtensions,   // index entries (XE) and legal citations (TA)
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
  TextStyleAdvanced,
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
  TrackChangesExt, // tracking plugin; pulls in the `insertion`/`deletion` marks itself
  BookmarkMark,
  SpellLangMark,
  HeadingCollapseExt,
  FootnoteExt,
  EndnoteExt, // twin of FootnoteExt: roman numbering, notes collected at doc end
  FieldExt,   // Word fields (PAGE, DATE, TOC…) — inline atom, hidden PM DOM only
  AutoCorrectExt,
  StyleNameExt,
  TextEffect, // Word « Effets de texte et typographie » (contour/ombre/reflet/éclat + OpenType)
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
  TextEffect,
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
function FormattingMiniBar({ editor, left, top, rootRef, onAddComment, caretMode, onSelectAll, onDismiss }: {
  editor: Editor; left: number; top: number; rootRef?: React.Ref<HTMLDivElement>; onAddComment?: () => void
  /** Menu d'insertion au caret (« tap again ») : Coller · Tout sélectionner. */
  caretMode?: boolean
  onSelectAll?: () => void
  /** Ferme le menu après une action (comportement plateforme : Copier ferme). */
  onDismiss?: () => void
}) {
  const { t } = useTranslation('office')
  // Mobile : barre contextuelle ALLÉGÉE façon Word (Couper/Copier/Coller/
  // Commentaire) — la mise en forme vit déjà dans la barre du bas du ruban.
  const isMobileBar = useIsMobile()
  const availableFonts = useAvailableFonts()
  const [openMenu, setOpenMenu] = useState<'color' | 'hl' | 'styles' | null>(null)
  // Clamp horizontal : la barre (centrée sur la sélection via translateX(-50%))
  // ne doit JAMAIS déborder de l'écran — mesurée puis recalée dans le viewport.
  const elRef = useRef<HTMLDivElement | null>(null)
  const [dx, setDx] = useState(0)
  const setRefs = (el: HTMLDivElement | null) => {
    elRef.current = el
    if (typeof rootRef === 'function') rootRef(el)
    else if (rootRef && typeof rootRef === 'object') (rootRef as React.MutableRefObject<HTMLDivElement | null>).current = el
  }
  useLayoutEffect(() => {
    const el = elRef.current
    if (!el) return
    const half = el.offsetWidth / 2
    const clamped = Math.min(Math.max(8 + half, left), window.innerWidth - 8 - half)
    setDx(prev => (Math.abs(prev - (clamped - left)) < 0.5 ? prev : clamped - left))
  }, [left, top, isMobileBar])
  // Lecture seule : SEULE la copie est permise. Couper/Coller/Commentaire et
  // toute la mise en forme MODIFIENT le document → proscrits hors édition.
  // `editor.isEditable` est posé par la bascule de mode (cf. setEditable).
  const readOnly = !editor.isEditable
  const doCut   = () => { editor.view.focus(); document.execCommand('cut') }
  const doCopy  = () => { editor.view.focus(); document.execCommand('copy') }
  const doPaste = async () => {
    try { const txt = await navigator.clipboard.readText(); editor.chain().focus().insertContent(txt).run() }
    catch { editor.view.focus(); document.execCommand('paste') }
  }
  const toggleMenu = (m: 'color' | 'hl' | 'styles') => setOpenMenu(o => (o === m ? null : m))
  const ts = editor.getAttributes('textStyle')
  const curFont  = (ts.fontFamily as string) || 'Arial'
  const curSizeN = ts.fontSize ? Math.round(parseFloat(String(ts.fontSize))) : 11
  const curColor = (ts.color as string) || '#202124'
  const curHl    = (editor.getAttributes('highlight').color as string) || '#fff475'
  const bump = (d: number) => applyInlineFormat(editor, { fs: `${Math.max(6, Math.min(96, curSizeN + d))}pt` })
  const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72]
  const SWATCHES = ['#202124', '#5f6368', '#d93025', '#e8710a', '#f9ab00', '#1e8e3e', '#1a73e8', '#9334e6', '#a52714', '#ffffff']
  const HL_SWATCHES = ['#fff475', '#ccff90', '#a7ffeb', '#cbf0f8', '#d7aefb', '#fdcfe8', '#e6c9a8', '#e8eaed']
  // Named paragraph styles (self-contained: acts on the editor's heading levels).
  const STYLES: Array<{ id: string; label: string; active: boolean; run: () => void }> = [
    { id: 'p',  label: t('doc_style_normal', { defaultValue: 'Normal' }),  active: editor.isActive('paragraph'),          run: () => editor.chain().focus().setParagraph().run() },
    { id: 'h1', label: t('doc_style_h1', { defaultValue: 'Titre 1' }),     active: editor.isActive('heading', { level: 1 }), run: () => editor.chain().focus().toggleHeading({ level: 1 }).run() },
    { id: 'h2', label: t('doc_style_h2', { defaultValue: 'Titre 2' }),     active: editor.isActive('heading', { level: 2 }), run: () => editor.chain().focus().toggleHeading({ level: 2 }).run() },
    { id: 'h3', label: t('doc_style_h3', { defaultValue: 'Titre 3' }),     active: editor.isActive('heading', { level: 3 }), run: () => editor.chain().focus().toggleHeading({ level: 3 }).run() },
  ]
  const curStyle = STYLES.find(s => s.active) ?? STYLES[0]

  const MiniBtn = ({ active, onDo, title, w = 7, children }: { active?: boolean; onDo: () => void; title: string; w?: number; children: React.ReactNode }) => (
    <button title={title} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={onDo}
      className={`flex items-center justify-center h-7 rounded ${active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}
      style={{ width: w * 4 }}>{children}</button>
  )
  const Sep = () => <div className="w-px h-5 bg-border mx-0.5" />
  // Letter "A" with a small up/down caret, like Word's grow/shrink buttons.
  const StepA = ({ up }: { up: boolean }) => (
    <span className="relative inline-flex items-center leading-none" style={{ fontSize: up ? 15 : 12 }}>A
      <span className="absolute" style={{ right: -6, top: up ? -1 : undefined, bottom: up ? undefined : -1, fontSize: 8, lineHeight: 1 }}>{up ? '▴' : '▾'}</span>
    </span>
  )
  // Split control: a main action button glued to a caret that opens `menu`.
  const Split = ({ title, active, onMain, menuKey, main, menu }: { title: string; active?: boolean; onMain: () => void; menuKey: 'color' | 'hl'; main: React.ReactNode; menu: React.ReactNode }) => (
    <span className="relative flex items-center">
      <button title={title} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={onMain}
        className={`flex items-center justify-center w-7 h-7 rounded-l ${active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}>{main}</button>
      <button title={title} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={() => toggleMenu(menuKey)}
        className="flex items-center justify-center w-3.5 h-7 rounded-r text-text-secondary hover:bg-surface-2"><ChevronDown size={10} /></button>
      {openMenu === menuKey && menu}
    </span>
  )
  const swatchGrid = (colors: string[], onPick: (c: string) => void) => (
    <div className="absolute bg-white border border-border rounded-lg shadow-lg p-1.5 grid grid-cols-4 gap-1" style={{ top: '112%', left: 0, zIndex: 61 }}>
      {colors.map(c => (
        <button key={c} title={c} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }}
          onClick={() => { onPick(c); setOpenMenu(null) }}
          className="w-5 h-5 rounded border border-border" style={{ background: c }} />
      ))}
    </div>
  )
  // ── Version MOBILE : une seule rangée contextuelle, libellée ────────────────
  if (isMobileBar) {
    // Action exécutée → le menu se FERME (comportement plateforme : AOSP ferme
    // sur Copier ; iOS ferme par défaut). `keep` pour les actions qui doivent le
    // garder/renouveler (Tout sélectionner ré-arme le menu de sélection).
    const run = (fn: () => void, keep = false) => () => { fn(); if (!keep) onDismiss?.() }
    const MobItem = ({ icon, label, onDo }: { icon?: React.ReactNode; label: string; onDo: () => void }) => (
      <button onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={onDo}
        className="flex items-center gap-1.5 h-11 px-3 rounded text-xs text-text-primary hover:bg-surface-2 active:bg-surface-3 whitespace-nowrap touch-manipulation"
        style={{ WebkitTapHighlightColor: 'transparent' }}>
        {icon && <span className="text-text-secondary">{icon}</span>}{label}
      </button>
    )
    // Menu d'INSERTION au caret (« tap again ») : Coller · Tout sélectionner.
    if (caretMode) {
      return createPortal(
        <div ref={setRefs} style={{ position: 'fixed', left: left + dx, top: Math.max(8, top - 58), transform: 'translateX(-50%)', zIndex: 60, transition: 'opacity 90ms linear' }}
          onMouseDown={e => e.preventDefault()}
          className="flex items-center gap-0.5 bg-white border border-border rounded-full shadow-lg px-1.5 py-1">
          {!readOnly && <MobItem icon={<ClipboardPaste size={15} />} label={t('common_paste', { defaultValue: 'Coller' })} onDo={run(() => { void doPaste() })} />}
          <MobItem label={t('doc_select_all', { defaultValue: 'Tout sélectionner' })}
            onDo={run(() => onSelectAll?.(), true)} />
        </div>, document.body)
    }
    return createPortal(
      <div ref={setRefs} style={{ position: 'fixed', left: left + dx, top: Math.max(8, top - 58), transform: 'translateX(-50%)', zIndex: 60, transition: 'opacity 90ms linear' }}
        onMouseDown={e => e.preventDefault()}
        className="flex items-center gap-0.5 bg-white border border-border rounded-full shadow-lg px-1.5 py-1">
        {!readOnly && <MobItem icon={<Scissors size={15} />} label={t('common_cut', { defaultValue: 'Couper' })} onDo={run(doCut)} />}
        <MobItem icon={<Copy size={15} />}           label={t('common_copy',  { defaultValue: 'Copier' })} onDo={run(doCopy)} />
        {!readOnly && <MobItem icon={<ClipboardPaste size={15} />} label={t('common_paste', { defaultValue: 'Coller' })} onDo={run(() => { void doPaste() })} />}
        {!readOnly && onAddComment && (
          <MobItem icon={<MessageSquarePlus size={15} />} label={t('doc_comment_short', { defaultValue: 'Commentaire' })} onDo={run(onAddComment)} />
        )}
      </div>, document.body)
  }

  // Desktop en lecture seule : la barre de mise en forme n'a AUCUN sens (toutes
  // ses actions mutent) → réduite à « Copier ».
  if (readOnly) {
    return createPortal(
      <div ref={setRefs} style={{ position: 'fixed', left: left + dx, top: top - 44, transform: 'translateX(-50%)', zIndex: 60, transition: 'opacity 90ms linear' }}
        onMouseDown={e => e.preventDefault()}
        className="flex items-center gap-0.5 bg-white border border-border rounded-lg shadow-lg px-1.5 py-1">
        <button onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={doCopy}
          className="flex items-center gap-1.5 h-8 px-2.5 rounded text-xs text-text-primary hover:bg-surface-2 whitespace-nowrap">
          <Copy size={15} className="text-text-secondary" />{t('common_copy', { defaultValue: 'Copier' })}
        </button>
      </div>, document.body)
  }

  return createPortal(
    // NB : `opacity` n'est PAS déclarée ici (pilotée en IMPÉRATIF via rootRef par le
    // parent pour le fondu de proximité) → React ne la réinitialise pas à chaque reflow
    // (repositionnement au scroll) ; seule la transition est déclarative.
    <div ref={setRefs} style={{ position: 'fixed', left: left + dx, top: top - 86, transform: 'translateX(-50%)', zIndex: 60, transition: 'opacity 90ms linear' }}
      onMouseDown={e => e.preventDefault()}
      className="flex flex-col gap-1 bg-white border border-border rounded-lg shadow-lg px-1.5 py-1.5">
      {/* Ligne 1 : police · taille · agrandir/réduire · effacer mise en forme · styles · commentaire */}
      <div className="flex items-center gap-0.5">
        <FontSizeField
          font={curFont} onFontChange={f => applyInlineFormat(editor, { ff: f })} fonts={availableFonts}
          size={String(curSizeN)} onSizeChange={v => applyInlineFormat(editor, { fs: `${v}pt` })}
          sizes={SIZES} height={28} fontWidth={132} sizeWidth={58} fontSize={13} />
        <MiniBtn title={t('doc_increase_font', { defaultValue: 'Agrandir la police' })} onDo={() => bump(1)}><StepA up /></MiniBtn>
        <MiniBtn title={t('doc_decrease_font', { defaultValue: 'Réduire la police' })} onDo={() => bump(-1)}><StepA up={false} /></MiniBtn>
        <MiniBtn title={t('doc_clear_formatting', { defaultValue: 'Effacer la mise en forme' })} onDo={() => editor.chain().focus().unsetAllMarks().clearNodes().run()}><Eraser size={15} /></MiniBtn>
        <Sep />
        {/* Styles */}
        <span className="relative flex items-center">
          <button title={t('doc_grp_styles', { defaultValue: 'Styles' })} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={() => toggleMenu('styles')}
            className="flex items-center gap-1 h-7 px-1.5 rounded text-text-secondary hover:bg-surface-2">
            <span className="font-semibold" style={{ fontSize: 13 }}>{curStyle.label}</span><ChevronDown size={11} />
          </button>
          {openMenu === 'styles' && (
            <div className="absolute bg-white border border-border rounded-lg shadow-lg py-1 min-w-36" style={{ top: '112%', left: 0, zIndex: 61 }}>
              {STYLES.map(s => (
                <button key={s.id} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }}
                  onClick={() => { s.run(); setOpenMenu(null) }}
                  className={`w-full text-left px-3 py-1.5 text-sm hover:bg-surface-2 ${s.active ? 'text-primary font-semibold' : 'text-text-primary'}`}
                  style={{ fontSize: s.id === 'h1' ? 17 : s.id === 'h2' ? 15 : s.id === 'h3' ? 13 : 13, fontWeight: s.id.startsWith('h') ? 600 : undefined }}>{s.label}</button>
              ))}
            </div>
          )}
        </span>
        {onAddComment && (
          <>
            <Sep />
            <button title={t('doc_new_comment', { defaultValue: 'Nouveau commentaire' })} onMouseDown={e => { e.preventDefault(); e.stopPropagation() }} onClick={onAddComment}
              className="flex items-center gap-1 h-7 px-1.5 rounded text-text-secondary hover:bg-surface-2">
              <MessageSquarePlus size={16} /><span style={{ fontSize: 12 }}>{t('doc_comment_short', { defaultValue: 'Commentaire' })}</span>
            </button>
          </>
        )}
      </div>
      {/* Ligne 2 : styles de caractère · surlignage · couleur · listes */}
      <div className="flex items-center gap-0.5">
        <MiniBtn title={t('doc_bold')}      active={editor.isActive('bold')}      onDo={() => editor.chain().focus().toggleBold().run()}><Bold size={15} /></MiniBtn>
        <MiniBtn title={t('doc_italic')}    active={editor.isActive('italic')}    onDo={() => editor.chain().focus().toggleItalic().run()}><Italic size={15} /></MiniBtn>
        <MiniBtn title={t('doc_underline')} active={editor.isActive('underline')} onDo={() => editor.chain().focus().toggleUnderline().run()}><UnderlineIcon size={15} /></MiniBtn>
        <MiniBtn title={t('doc_strikethrough', { defaultValue: 'Barré' })} active={editor.isActive('strike')} onDo={() => editor.chain().focus().toggleStrike().run()}><Strikethrough size={15} /></MiniBtn>
        <Sep />
        {/* Surlignage (split : bascule + palette) */}
        <Split title={t('doc_highlight', { defaultValue: 'Surlignage' })} active={editor.isActive('highlight')} menuKey="hl"
          onMain={() => editor.chain().focus().toggleHighlight({ color: curHl }).run()}
          main={<span className="flex flex-col items-center justify-center leading-none"><Highlighter size={14} /><span style={{ width: 15, height: 3, background: curHl, marginTop: 1 }} /></span>}
          menu={swatchGrid(HL_SWATCHES, c => editor.chain().focus().setHighlight({ color: c }).run())} />
        {/* Couleur du texte (split : applique la couleur courante + palette) */}
        <Split title={t('doc_text_color', { defaultValue: 'Couleur du texte' })} menuKey="color"
          onMain={() => editor.chain().focus().setColor(curColor).run()}
          main={<span className="flex flex-col items-center justify-center leading-none"><span style={{ fontSize: 13, lineHeight: 1 }}>A</span><span style={{ width: 15, height: 3, background: curColor, marginTop: 1 }} /></span>}
          menu={swatchGrid(SWATCHES, c => editor.chain().focus().setColor(c).run())} />
        <Sep />
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
    let caretY = cm.y
    const sm = ed.state.storedMarks
    if (sm && sm.length) {
      const ts = sm.find(m => m.type.name === 'textStyle' && m.attrs.fontSize)
      if (ts) {
        const pt = parseFloat(String(ts.attrs.fontSize))
        // Prévisualisation de la taille choisie : ancrée sur la BASELINE (cf. drawCaret du corps).
        if (!isNaN(pt)) { caretH = pt * (96 / 72) * 1.2; if (cm.baseline != null) caretY = cm.baseline - caretH * 0.8 }
      }
    }
    caret.style.display = 'block'
    // Snap au pixel physique + épaisseur 1 pixel machine (cf. drawCaret du corps).
    const dprC = window.devicePixelRatio || 1
    const snapC = (v: number) => Math.round(v * dprC) / dprC
    caret.style.width   = `${1 / dprC}px`
    caret.style.left    = `${snapC(cm.x * z)}px`
    caret.style.top     = `${snapC(caretY * z)}px`
    caret.style.height  = `${Math.max(1, Math.round(caretH * z * dprC) / dprC)}px`
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
    const dpr = window.devicePixelRatio || 1   // résolution native — pas de supersampling (cf. paintInputs)
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
      ctx.fillText(placeholder, cm.x, cm.baseline ?? (cm.y + cm.height * 0.78))
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
      <div ref={caretRef} style={{ position: 'absolute', width: 1, background: '#202124', display: 'none', pointerEvents: 'none', zIndex: 1 }} />
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
const CELL_BTN       = 14   // côté (px) de la poignée « bordures » de la cellule courante
const COLBAR_H       = 22   // hauteur (px) de la pastille d'outils de colonne (survol rangée 0)
const TBL_HANDLE     = 16   // côté (px) de la poignée de déplacement du tableau (coin haut-gauche)
const TBL_SIZER      = 9    // côté (px) de la poignée de redimensionnement (coin bas-droit)
const TBL_KEEP_VISIBLE = 60 // px de tableau qui restent au moins dans la zone de contenu
// Pointeur tactile : les affordances de SURVOL (pastille de colonne, poignée de
// bordures) n'y ont pas de sens et gêneraient la frappe.
const isCoarsePointer = () => typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches
const PAGE_MARGIN_TOP = 5
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
  onOpenIndents?:     () => void   // double-clic sur un repère de retrait → dialogue Paragraphe
  // Bornes de colonnes du tableau en cours d'édition (px, repère contenu) : repères
  // « Déplacer la colonne du tableau » de Word. `onTableBoundDown` démarre le glissé.
  tableCols?:          number[]
  onTableBoundDown?:   (index: number, e: React.PointerEvent) => void
  // Bornes de la CELLULE du curseur (px, repère contenu) : dans un tableau, Word
  // recale les marqueurs de retrait sur la cellule, pas sur la zone de contenu.
  tableCell?:          { x0: number; x1: number }
}

type HRHit = 'left' | 'right' | 'i-first' | 'i-hang' | 'i-left' | 'i-right'

function HorizontalRuler({ pageW, marginLeft, marginRight, zoom, columns = 1, colGap = 0, indentLeft = 0, indentFirstLine = 0, indentRight = 0, tabStops = [], tabType = 'left', onMarginsChange, onIndentsChange, onTabStopsChange, onDragGuideChange, onOpenIndents, tableCols, onTableBoundDown, tableCell }: HorizontalRulerProps) {
  const { t } = useTranslation('office')
  const canvasRef   = useRef<HTMLCanvasElement>(null)
  const [cursor, setCursor]   = useState('default')
  const [tooltip, setTooltip] = useState<{ x: number; label: string } | null>(null)
  // Libellé du tooltip natif : contextuel selon le repère survolé (sinon Mise en page).
  const pageSetupTitle = t('doc_page_setup_hint', { defaultValue: 'Double-cliquer : Mise en page' })
  const [titleAttr, setTitleAttr] = useState(pageSetupTitle)
  const hitTitle = (hit: HRHit | null): string => {
    const names: Partial<Record<HRHit, string>> = {
      'i-first': t('doc_ruler_indent_first',   { defaultValue: 'Retrait de première ligne' }),
      'i-hang':  t('doc_ruler_indent_hanging', { defaultValue: 'Retrait suspendu' }),
      'i-left':  t('doc_ruler_indent_left',    { defaultValue: 'Retrait gauche' }),
      'i-right': t('doc_ruler_indent_right',   { defaultValue: 'Retrait droit' }),
    }
    // Sur un repère de retrait, Word montre juste SON NOM (« Retrait suspendu »).
    // Le « Double-cliquer : … » n'est que pour la zone VIDE de la règle (→ Mise
    // en page) ; l'accoler au nom d'un repère était trompeur.
    const n = hit ? names[hit] : undefined
    return n ?? pageSetupTitle
  }
  const draggingRef  = useRef<HRHit | null>(null)
  const liveL        = useRef(marginLeft)
  const liveR        = useRef(marginRight)
  const liveIL       = useRef(indentLeft)
  const liveIF       = useRef(indentFirstLine)
  const liveIR       = useRef(indentRight)
  const liveTabs     = useRef(tabStops)
  const didDragRef   = useRef(false)

  const w = Math.round(pageW * zoom)
  const h = RULER_SZ + RULER_OVERHANG   // canvas plus haut que la règle : les repères débordent dessous

  // Ancres des marqueurs de retrait : la CELLULE du curseur dans un tableau
  // (façon Word), sinon les marges de la page.
  const tblCellRef = useRef<{ x0: number; x1: number } | undefined>(tableCell); tblCellRef.current = tableCell
  const indentAnchors = useCallback((mlPx: number, mrPx: number) => {
    const c = tblCellRef.current
    return c ? { o: mlPx + c.x0 * zoom, r: mlPx + c.x1 * zoom } : { o: mlPx, r: w - mrPx }
  }, [zoom, w])
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

    // La règle VISIBLE ne fait que RULER_SZ de haut ; le canvas est plus grand pour
    // laisser DÉBORDER les repères de retrait dessous (façon Word). Fond/graduations
    // limités à la bande visible ; en dessous = transparent (les repères planent).
    // SB = corps de la bande, 1px de MOINS que SH : la DERNIÈRE rangée reste
    // transparente pour laisser voir la bordure basse du conteneur (ligne de séparation
    // UNIFORME sur toute la largeur — le canvas ne la recouvre plus dans sa zone).
    const SH = RULER_SZ
    const SB = SH - 1
    // Gray background (margin zones)
    ctx.fillStyle = '#f1f3f4'
    ctx.fillRect(0, 0, w, SB)
    // White content zone
    ctx.fillStyle = '#fff'
    ctx.fillRect(mlPx, 0, w - mlPx - mrPx, SB)
    // Gouttières entre colonnes (zones grises) + bornes
    if (columns > 1) {
      const contentWpx = w - mlPx - mrPx
      const gapPx = colGap * zoom
      const colWpx = (contentWpx - (columns - 1) * gapPx) / columns
      for (let i = 0; i < columns - 1; i++) {
        const gx = mlPx + (i + 1) * colWpx + i * gapPx
        ctx.fillStyle = '#f1f3f4'
        ctx.fillRect(gx, 0, gapPx, SB)
        ctx.fillStyle = '#bdc1c6'
        ctx.fillRect(Math.round(gx) - 0.5, 0, 1, SB)
        ctx.fillRect(Math.round(gx + gapPx) - 0.5, 0, 1, SB)
      }
    }
    // Margin boundary lines
    ctx.fillStyle = '#bdc1c6'
    ctx.fillRect(Math.round(mlPx) - 0.5,       0, 1, SB)
    ctx.fillRect(Math.round(w - mrPx) - 0.5,   0, 1, SB)

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
      ctx.fillRect(Math.round(x) - 0.5, SB - (isCm ? 8 : 4), 1, isCm ? 8 : 4)
      // Labels en valeur absolue (marge gauche en positif comme Google Docs :
      // « 2 1 » puis « 1 2 3 … »), et pas de « 0 » à l'origine.
      if (isCm && mm !== 0) { ctx.textAlign = 'center'; ctx.fillText(String(Math.abs(mm / 10)), x, 1) }
    }

    // (Pas de ligne de séparation dessinée ici : c'est la bordure basse du CONTENEUR
    // qui la fournit, sur toute la largeur, UNIFORME — le canvas s'arrête à SB pour ne
    // pas la recouvrir.)

    // ── Marqueurs de retrait (façon Word) ─────────────────────────────────────
    // Pentagones « maison » ÉVIDÉS (fond blanc, fin contour bleu) : la 1ʳᵉ ligne
    // pointe vers le BAS (haut de règle) et le retrait suspendu pointe vers le HAUT
    // (bas), les deux pointes se faisant face (sablier) ; le retrait gauche est une
    // petite barre sous le suspendu ; le retrait droit est une maison vers le haut à
    // droite. Le fond BLANC les rend visibles sur la zone de contenu blanche, et le
    // contour + ombre douce les détache de la règle. Zones de préhension inchangées
    // (cf. getHit) — 1ʳᵉ ligne en haut (my≤9), suspendu au milieu, gauche en bas.
    const anD = indentAnchors(mlPx, mrPx)
    const leftX  = anD.o + liveIL.current * zoom
    const firstX = anD.o + (liveIL.current + liveIF.current) * zoom
    const rightX = anD.r - liveIR.current * zoom
    const IH = 4.5   // demi-largeur des repères
    ctx.lineJoin = 'round'; ctx.lineCap = 'round'
    // Trace la forme, remplit en blanc (avec une ombre douce) puis contour bleu net.
    const marker = (path: () => void) => {
      ctx.save()
      ctx.shadowColor = 'rgba(32,33,36,0.22)'; ctx.shadowBlur = 1.5; ctx.shadowOffsetY = 0.5
      ctx.fillStyle = '#ffffff'
      ctx.beginPath(); path(); ctx.fill()
      ctx.restore()
      ctx.beginPath(); path(); ctx.lineWidth = 1.2; ctx.strokeStyle = '#1a73e8'; ctx.stroke()
    }
    // Maison pointe vers le BAS (sommet plat en haut, pointe en bas)
    const houseDown = (cx: number, yTop: number, yMid: number, yBot: number) => {
      ctx.moveTo(cx - IH, yTop); ctx.lineTo(cx + IH, yTop); ctx.lineTo(cx + IH, yMid); ctx.lineTo(cx, yBot); ctx.lineTo(cx - IH, yMid); ctx.closePath()
    }
    // Maison pointe vers le HAUT (pointe en haut, base plate en bas)
    const houseUp = (cx: number, yTop: number, yMid: number, yBot: number) => {
      ctx.moveTo(cx, yTop); ctx.lineTo(cx + IH, yMid); ctx.lineTo(cx + IH, yBot); ctx.lineTo(cx - IH, yBot); ctx.lineTo(cx - IH, yMid); ctx.closePath()
    }
    // ── Repères « Déplacer la colonne du tableau » (Word) ──────────────────────
    // Un petit rectangle gris hachuré à CHAQUE borne de colonne du tableau en
    // cours d'édition, bords du tableau inclus : les glisser redimensionne la
    // colonne, exactement comme tirer la bordure dans le tableau.
    const cols = tblColsRef.current
    if (cols && cols.length >= 2) {
      const MW = 8, MH = 11, MY = Math.round((RULER_SZ - MH) / 2)
      for (const cx0 of cols) {
        const cx = Math.round(mlPx + cx0 * zoom)
        if (cx < -MW || cx > w + MW) continue
        ctx.fillStyle = '#9aa0a6'
        ctx.fillRect(cx - MW / 2, MY, MW, MH)
        ctx.fillStyle = '#f1f3f4'
        ctx.fillRect(cx - MW / 2 + 2, MY + 2, 1, MH - 4)
        ctx.fillRect(cx + MW / 2 - 3, MY + 2, 1, MH - 4)
      }
    }

    // Le sablier chevauche le bord bas de la règle (y=RULER_SZ) : maison ↓ (haut,
    // dans la règle) et maison ↑ (bas, débordant sous la règle) ; la barre pend
    // dessous. Grandes formes équilibrées SANS agrandir la règle visible.
    marker(() => houseDown(firstX, 3.5, 8, 12.5))               // 1ʳᵉ ligne (pointe ↓, milieu)
    marker(() => houseUp(leftX, 13, 17, 21))                     // retrait suspendu (pointe ↑, milieu)
    marker(() => ctx.roundRect(leftX - IH, 21.4, IH * 2, 2.2, 1)) // retrait gauche (barre, sous la règle)
    marker(() => houseUp(rightX, 13, 17, 21))                    // retrait droit

    // ── Taquets de tabulation (symboles façon Word, en bas de la règle) ────────
    ctx.fillStyle = '#3c4043'
    ctx.font = '11px Arial'; ctx.textBaseline = 'bottom'; ctx.textAlign = 'center'
    for (const tab of liveTabs.current) {
      const tx = mlPx + tab.pos * zoom
      if (tx < mlPx - 2 || tx > w - mrPx + 2) continue
      ctx.fillText(TAB_SYMBOL[tab.type] ?? '⌞', tx, RULER_SZ - 1)
    }


    ctx.restore()
  }, [w, h, zoom, columns, colGap])

  useLayoutEffect(() => {
    liveL.current = marginLeft; liveR.current = marginRight
    liveIL.current = indentLeft; liveIF.current = indentFirstLine; liveIR.current = indentRight
    liveTabs.current = tabStops
    drawRuler(marginLeft, marginRight)
  }, [drawRuler, marginLeft, marginRight, indentLeft, indentFirstLine, indentRight, tabStops, tableCols])

  // Clic sur la règle : pose un taquet (type courant) dans la zone de contenu ; un clic sur
  // un taquet EXISTANT le retire. Ignoré juste après un glisser de marqueur.
  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (didDragRef.current) { didDragRef.current = false; return }
    const r = e.currentTarget.getBoundingClientRect()
    const mx = e.clientX - r.left
    if (e.clientY - r.top >= RULER_SZ) return   // clic dans le débord (sous la règle) : ignorer
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
  // Index du repère de tableau sous l'abscisse `mx` (ou null).
  const tblColsRef = useRef<number[] | undefined>(tableCols); tblColsRef.current = tableCols
  const getTblHit = useCallback((mx: number, my: number): number | null => {
    const cols = tblColsRef.current
    if (!cols || cols.length < 2 || my > RULER_SZ) return null
    const mlPx = liveL.current * zoom
    let best: number | null = null, bestD = RULER_SNAP
    cols.forEach((c, i) => {
      const d = Math.abs(mlPx + c * zoom - mx)
      if (d <= bestD) { bestD = d; best = i }
    })
    return best
  }, [zoom])
  const getHit = useCallback((mx: number, my: number): HRHit | null => {
    const mlPx = liveL.current * zoom, mrPx = liveR.current * zoom
    const an = indentAnchors(mlPx, mrPx)
    const leftX  = an.o + liveIL.current * zoom
    const firstX = an.o + (liveIL.current + liveIF.current) * zoom
    const rightX = an.r - liveIR.current * zoom
    const near = (a: number, b: number) => Math.abs(a - b) <= RULER_SNAP
    if (my <= 12 && near(mx, firstX)) return 'i-first'
    if (my >= 21 && near(mx, leftX)) return 'i-left'
    if (my > 12  && near(mx, leftX))  return 'i-hang'
    if (my > 12  && near(mx, rightX)) return 'i-right'
    if (mx < mlPx - 1 && near(mx, mlPx)) return 'left'          // marge gauche (côté gris)
    if (mx > (w - mrPx) + 1 && near(mx, w - mrPx)) return 'right' // marge droite (côté gris)
    return null
  }, [zoom, w, indentAnchors])

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (draggingRef.current) return
    const r = e.currentTarget.getBoundingClientRect()
    const hit = getHit(e.clientX - r.left, e.clientY - r.top)
    if (hit) { setCursor('ew-resize'); setTitleAttr(hitTitle(hit)); return }
    // Les marqueurs de retrait sont AU-DESSUS des repères gris de colonnes : ils
    // gagnent donc le hit-test quand les deux se superposent (comme dans Word).
    const tblHit = getTblHit(e.clientX - r.left, e.clientY - r.top)
    if (tblHit != null) {
      setCursor('col-resize')
      setTitleAttr(t('doc_ruler_move_table_col', { defaultValue: 'Déplacer la colonne du tableau' }))
      return
    }
    setCursor('default')
    setTitleAttr(hitTitle(null))
  }

  // Double-clic sur un repère de RETRAIT → dialogue Paragraphe (retraits) ; ailleurs
  // (marges / zone vide) → laisser remonter au conteneur (Mise en page).
  const handleDoubleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect()
    const hit = getHit(e.clientX - r.left, e.clientY - r.top)
    if (hit && hit.startsWith('i-')) { e.stopPropagation(); onOpenIndents?.() }
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return
    const r0  = e.currentTarget.getBoundingClientRect()
    const hit = getHit(e.clientX - r0.left, e.clientY - r0.top)
    // Repère de tableau : même glissé que la bande posée sur la bordure de colonne.
    // Testé APRÈS les retraits (qui sont dessinés au-dessus).
    if (!hit) {
      const tblHit = getTblHit(e.clientX - r0.left, e.clientY - r0.top)
      if (tblHit != null && onTableBoundDown) {
        e.preventDefault()
        didDragRef.current = true
        onTableBoundDown(tblHit, e as unknown as React.PointerEvent)
      }
      return
    }
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
      const anM = indentAnchors(mlPx, mrPx)
      const originXpx = anM.o
      const rightXpx = anM.r - liveIR.current * zoom
      const oldIL = liveIL.current, oldIF = liveIF.current
      const clampX = (x: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, x))
      if (hit === 'i-first') {
        const fx = clampX(mx, originXpx, rightXpx - MINGAP)
        liveIF.current = (fx - originXpx) / zoom - liveIL.current
      } else if (hit === 'i-hang') {
        const lx = clampX(mx, originXpx, rightXpx - MINGAP)
        const newIL = (lx - originXpx) / zoom
        liveIL.current = newIL
        liveIF.current = (oldIL + oldIF) - newIL          // garde la 1ʳᵉ ligne fixe
      } else if (hit === 'i-left') {
        const lx = clampX(mx, originXpx, rightXpx - MINGAP)
        liveIL.current = (lx - originXpx) / zoom             // déplace tout le bloc (offset conservé)
      } else { // i-right
        const minX = originXpx + (Math.max(liveIL.current, liveIL.current + liveIF.current)) * zoom + MINGAP
        const rx = clampX(mx, minX, anM.r)
        liveIR.current = (anM.r - rx) / zoom
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
        title={titleAttr}
        onMouseMove={handleMouseMove}
        onMouseLeave={() => { if (!draggingRef.current) { setCursor('default'); setTitleAttr(pageSetupTitle) } }}
        onMouseDown={handleMouseDown}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
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
  // Bornes de lignes du tableau en cours d'édition (px, repère contenu de sa page) :
  // repères « Déplacer la ligne du tableau » de Word.
  tableRows?:          number[]
  onTableBoundDown?:   (index: number, e: React.PointerEvent) => void
}

function VerticalRuler({ scrollRef, activePage, activePageTop, zoom, marginTop, marginBottom, pageH, pageGap, onMarginsChange, onDragGuideChange, tableRows, onTableBoundDown }: VerticalRulerProps) {
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
    // VB = 1px de moins que la largeur : la dernière COLONNE reste transparente pour
    // laisser voir la bordure DROITE (#dadce0) du conteneur (le canvas la recouvrait) →
    // filet de séparation identique à celui, horizontal, du bas de la règle H.
    const VB = w - 1
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
    ctx.fillRect(0, 0, VB, h)

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
    ctx.fillRect(0, contentTopY, VB, cH)

    // Filets de bordure de marge (haut/bas du contenu)
    ctx.fillStyle = '#bdc1c6'
    ctx.fillRect(0, Math.round(contentTopY), VB, 1)
    ctx.fillRect(0, Math.round(contentBotY), VB, 1)

    // ── Repères « Déplacer la ligne du tableau » (Word) ────────────────────────
    // Un petit rectangle gris hachuré à chaque borne de ligne du tableau en cours
    // d'édition : les glisser change la hauteur de la ligne, comme tirer la bordure.
    const rowsMk = tblRowsRef.current
    if (rowsMk && rowsMk.length >= 2) {
      const MH = 8, MW = 11, MX = Math.round((VB - MW) / 2)
      for (const ry of rowsMk) {
        const y = Math.round(contentTopY + ry * zoom)
        if (y < -MH || y > h + MH) continue
        ctx.fillStyle = '#9aa0a6'
        ctx.fillRect(MX, y - MH / 2, MW, MH)
        ctx.fillStyle = '#f1f3f4'
        ctx.fillRect(MX + 2, y - MH / 2 + 2, MW - 4, 1)
        ctx.fillRect(MX + 2, y + MH / 2 - 3, MW - 4, 1)
      }
    }

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
      ctx.fillRect(VB - (isCm ? 8 : 4), Math.round(yScr) - 0.5, isCm ? 8 : 4, 1)
      if (isCm && mm !== 0) ctx.fillText(String(Math.abs(mm / 10)), w - 10, yScr)
    }

    // Pas de poignée dessinée (façon Word) : les marges haut/bas se règlent en
    // saisissant la bordure de marge (filets ci-dessus). Le glissé reste géré par
    // getHit (zones autour de contentTopY / contentBotY) — seul le repère visuel est
    // retiré.

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
  }, [redraw, marginTop, marginBottom, activePage, activePageTop, tableRows])

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

  // Repères de LIGNES du tableau (px écran) : mêmes bornes que le tracé.
  const tblRowsRef = useRef<number[] | undefined>(tableRows); tblRowsRef.current = tableRows
  const contentTopScreen = useCallback((): number => {
    const opsh = pageH * zoom + pageGap * zoom
    const paperTop0 = CANVAS_PAD_Y + PAGE_MARGIN_TOP * zoom
    const paperTopC = activeTopRef.current != null ? activeTopRef.current + PAGE_MARGIN_TOP * zoom
                                                   : paperTop0 + activePageRef.current * opsh
    return paperTopC - scrollTopRef.current + liveT.current * zoom
  }, [zoom, pageH, pageGap])
  const getTblHit = useCallback((mouseY: number): number | null => {
    const rows = tblRowsRef.current
    if (!rows || rows.length < 2) return null
    const top = contentTopScreen()
    let best: number | null = null, bestD = RULER_SNAP
    rows.forEach((ry, i) => {
      const d = Math.abs(top + ry * zoom - mouseY)
      if (d <= bestD) { bestD = d; best = i }
    })
    return best
  }, [zoom, contentTopScreen])

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
    if (getTblHit(y) != null) { setCursor('row-resize'); return }
    setCursor(getHit(y) ? 'ns-resize' : 'default')
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    const rect = e.currentTarget.getBoundingClientRect()
    const y    = e.clientY - rect.top
    // Repère de tableau : même glissé que la bande posée sur la bordure de ligne.
    const tblHit = getTblHit(y)
    if (tblHit != null && onTableBoundDown) {
      e.preventDefault()
      onTableBoundDown(tblHit, e as unknown as React.PointerEvent)
      return
    }
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
      <canvas ref={canvasRef} title={t('doc_page_setup_hint', { defaultValue: 'Double-cliquer : Mise en page' })}
        style={{ position: 'absolute', top: 0, left: 0 }} />
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
// texte façon Word. Vignettes CARRÉES calquées sur les siennes : lignes de texte
// bleues épaisses + « arc-en-ciel » gris plein (l'objet de substitution de Word).
// Chaque mode ne diffère que par le tracé des lignes et l'ordre de superposition.
const WRAP_W = 44
const WRAP_BASE = 33          // ligne de base de l'arche
const WRAP_RO = 13            // rayon extérieur
const WRAP_RI = 6             // rayon intérieur
const WRAP_CX = WRAP_W / 2
const WRAP_BLUE = '#4a89c7'

/** Une ligne de « texte ». */
function WL({ y, x1 = 5, x2 = WRAP_W - 5 }: { y: number; x1?: number; x2?: number }) {
  return <line x1={x1} y1={y} x2={x2} y2={y} stroke={WRAP_BLUE} strokeWidth={2.4} strokeLinecap="round" />
}

/** L'arche « arc-en-ciel ». `mask` blanchit le trou (l'objet MASQUE le texte). */
function WrapArch({ fill = '#c9cdd3', stroke = '#7e848c', cx = WRAP_CX, base = WRAP_BASE, ro = WRAP_RO, ri = WRAP_RI, mask = false }: {
  fill?: string; stroke?: string; cx?: number; base?: number; ro?: number; ri?: number; mask?: boolean
}) {
  return (
    <>
      {mask && <path d={`M ${cx - ro} ${base} A ${ro} ${ro} 0 0 1 ${cx + ro} ${base} Z`} fill="#fff" />}
      <path
        d={`M ${cx - ro} ${base} A ${ro} ${ro} 0 0 1 ${cx + ro} ${base} L ${cx + ri} ${base} A ${ri} ${ri} 0 0 0 ${cx - ri} ${base} Z`}
        fill={fill} stroke={stroke} strokeWidth={1.4} strokeLinejoin="round" />
    </>
  )
}

/** x du bord EXTÉRIEUR de l'arche à la hauteur `y` (gouttières rapproché/au travers). */
function archX(y: number, pad: number): number {
  const dy = WRAP_BASE - y
  if (dy >= WRAP_RO) return 0
  return Math.sqrt(WRAP_RO * WRAP_RO - dy * dy) + pad
}

function wrapGutterLines(through: boolean): React.ReactNode {
  const rows: React.ReactNode[] = [<WL key="t" y={8} />]
  for (const y of [15, 21, 27]) {
    const d = archX(y, 3)
    rows.push(<WL key={`l${y}`} x1={5} x2={WRAP_CX - d} y={y} />, <WL key={`r${y}`} x1={WRAP_CX + d} x2={WRAP_W - 5} y={y} />)
  }
  if (through) {
    // « Au travers » : le texte pénètre AUSSI le creux sous l'arche — le tiret
    // dans le trou est ce qui distingue ce mode de « Rapproché ».
    const dy = WRAP_BASE - 31
    const di = Math.sqrt(WRAP_RI * WRAP_RI - dy * dy) - 2.5
    rows.push(<WL key="c" x1={WRAP_CX - di} x2={WRAP_CX + di} y={31} />)
  }
  rows.push(<WL key="b" y={38} />)
  return <>{rows}</>
}

const WRAP_SQ_G = WRAP_RO + 3
const WRAP_LINES: Record<string, React.ReactNode> = {
  // Aligné sur le texte : l'objet est DANS le fil, calé à gauche sous les lignes.
  inline: <><WL y={7} /><WL y={13} /><WL y={19} /><WL y={25} /></>,
  // Carré : gouttière rectangulaire de part et d'autre.
  square: <>
    <WL y={8} />
    {[15, 21, 27].map(y => <Fragment key={y}>
      <WL y={y} x1={5} x2={WRAP_CX - WRAP_SQ_G} /><WL y={y} x1={WRAP_CX + WRAP_SQ_G} x2={WRAP_W - 5} />
    </Fragment>)}
    <WL y={38} />
  </>,
  tight: wrapGutterLines(false),
  through: wrapGutterLines(true),
  // Haut et bas : rien à côté de l'objet.
  topBottom: <><WL y={8} /><WL y={14} /><WL y={38} /></>,
  // Derrière / Devant : lignes pleine largeur, seul l'ordre de tracé change.
  behind: <>{[7, 13, 19, 25, 31, 37].map(y => <WL key={y} y={y} />)}</>,
  front: <>{[7, 13, 19, 25, 31, 37].map(y => <WL key={y} y={y} />)}</>,
}

function WrapThumb({ mode }: { mode: string }) {
  return (
    <svg width={WRAP_W} height={WRAP_W} viewBox={`0 0 ${WRAP_W} ${WRAP_W}`} style={{ display: 'block' }}>
      {mode === 'behind' && <WrapArch fill="#dfe2e6" stroke="#9aa0a6" />}
      {WRAP_LINES[mode] ?? WRAP_LINES.square}
      {mode === 'front' && <WrapArch mask />}
      {mode === 'inline' && <WrapArch cx={16} base={40} ro={11} ri={5} />}
      {mode !== 'behind' && mode !== 'front' && mode !== 'inline' && <WrapArch />}
    </svg>
  )
}

function WrapOptionsPanel({ wrap, moveWithText, left, top, onChange, onMoveMode, onMore, onClose }: {
  wrap: string; moveWithText: boolean; left: number; top: number
  onChange: (w: string) => void
  onMoveMode: (moveWithText: boolean) => void
  onMore: () => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  // Les deux options de position (façon Word) sont GRISÉES pour un objet « aligné
  // sur le texte » : elles n'ont de sens qu'avec un habillage (objet flottant).
  const posDisabled = wrap === 'inline'
  // Le nom du mode reste dans le `title` (infobulle) : plus de libellé sous la
  // vignette, comme Word — la grille est plus compacte et plus lisible.
  const Item = ({ mode, label }: { mode: string; label: string }) => (
    <button onMouseDown={e => { e.preventDefault(); onChange(mode) }}
      className={`p-1 rounded-lg border ${wrap === mode ? 'border-primary bg-primary-light/40' : 'border-transparent hover:bg-surface-2'}`}
      title={label}>
      <span className="block border border-border rounded bg-white overflow-hidden"><WrapThumb mode={mode} /></span>
    </button>
  )
  return (
    <div style={{ position: 'absolute', left, top, zIndex: 33, width: 190 }}
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
        <Item mode="tight"     label={t('doc_wrap_tight',     { defaultValue: 'Rapproché' })} />
        <Item mode="through"   label={t('doc_wrap_through',   { defaultValue: 'Au travers' })} />
        <Item mode="topBottom" label={t('doc_wrap_topbottom', { defaultValue: 'Haut et bas' })} />
        <Item mode="behind"    label={t('doc_wrap_behind',    { defaultValue: 'Derrière le texte' })} />
        <Item mode="front"     label={t('doc_wrap_front',     { defaultValue: 'Devant le texte' })} />
      </div>

      {/* Position de l'objet (façon Word) : deux options exclusives + « Afficher
          plus… » qui ouvre le dialogue « Mise en page » complet. */}
      <div className="border-t border-border mt-3 pt-2.5 flex flex-col gap-2">
        {([
          [true,  t('doc_layout_move_with_text', { defaultValue: 'Déplacer avec le texte' }), t('doc_layout_move_with_text_hint', { defaultValue: "L'objet reste ancré au paragraphe et se déplace avec le texte." })],
          [false, t('doc_layout_fix_on_page',    { defaultValue: 'Corriger la position sur la page' }), t('doc_layout_fix_on_page_hint', { defaultValue: "L'objet garde une position fixe sur la page, quel que soit le texte." })],
        ] as Array<[boolean, string, string]>).map(([val, lbl, hint]) => (
          <label key={String(val)}
            className={`flex items-center gap-2 text-xs ${posDisabled ? 'opacity-40 cursor-default' : 'cursor-pointer'}`}>
            <Radio checked={!posDisabled && moveWithText === val} disabled={posDisabled}
              onChange={() => { if (!posDisabled) onMoveMode(val) }} />
            <span className="flex-1 text-text-primary">{lbl}</span>
            <span title={hint} className="text-text-tertiary shrink-0 inline-flex"><Info size={13} /></span>
          </label>
        ))}
        <button type="button" onMouseDown={e => { e.preventDefault(); onMore() }}
          className="self-end text-xs text-primary hover:underline mt-0.5">
          {t('doc_layout_more_link', { defaultValue: 'Afficher plus…' })}
        </button>
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
  // Alignement dans le référentiel (`wp:align`), exclusif du décalage manuel.
  alignH: string | null; alignV: string | null
  moveWithText: boolean; allowOverlap: boolean; lockAnchor: boolean
  geom: PageGeometry
}

// ── Position d'un objet ancré : dialogue ⇄ attributs ──────────────────────────
// The dialog only ever WRITES attributes — `posHRel`/`posVRel` (reference frame),
// `alignH`/`alignV` (alignment inside it) and `wrapX`/`wrapY` (manual offset
// INSIDE that same frame, exactly what the DOCX reader stores for `wp:posOffset`).
// Resolving them into a page position is the engine's job, as in Writer
// (sw/source/core/objectpositioning/anchoredobjectposition.cxx). The two helpers
// below are the only conversions the UI needs, and they both go through
// `documents/layout/anchor-position.ts` rather than re-deriving the geometry.

/**
 * Page frame + anchor frame as the dialog sees them.
 *
 * The anchor frame ORIGIN is deliberately left at 0: every conversion here is a
 * DIFFERENCE against the origin of the alignment area, which therefore cancels
 * out. Only the EXTENT matters — the column width for `column`, and the
 * degenerate zero-extent areas Writer uses for `paragraph`/`line`/`character`
 * (anchoredobjectposition.cxx:332-337 and :745-756).
 */
function anchorFrames(g: PageGeometry): { page: PageGeom; anchor: AnchorCtx } {
  return {
    page: makePageGeom(g.pageW, g.pageH, g.marginH, g.pageW - g.marginH - g.contentW, g.marginV, g.marginBottom),
    anchor: { paraTop: 0, paraLeft: 0, paraWidth: g.colW || g.contentW },
  }
}

/**
 * Manual offset that reproduces an ALIGNED position, in the same reference
 * frame. Used whenever the user leaves the "alignment" mode (radio switch in the
 * dialog, or a drag of the object): the pre-filled value keeps the object where
 * it currently is instead of snapping it back to the frame origin.
 *
 * `evenPage` is false: the editor lays every page out as a right-hand page, so
 * `inside`/`outside` resolve to left/right (resolveHoriAlign).
 */
function offsetOfAlign(
  g: PageGeometry, relH: RelH, relV: RelV, w: number, h: number,
  alignH: AlignH | undefined, alignV: AlignV | undefined,
): { x: number; y: number } {
  const { page, anchor } = anchorFrames(g)
  // No confinement and no page move: we want the position the user ASKED for,
  // not the one the engine will keep once it knows which page the object is on.
  const r = resolveObjectPosition({ relH, relV, offsetX: 0, offsetY: 0, alignH, alignV, w, h },
    page, anchor, { keepInsidePage: false, allowPageMove: false })
  return {
    x: Math.round(r.x - horizontalArea(relH, page, anchor).pos),
    y: Math.round(r.y - verticalArea(relV, page, anchor).pos),
  }
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
  collapsedDefault: boolean  // Word « Réduire par défaut » (titres uniquement)
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
    collapsedDefault: !!a.collapsedDefault,
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
    // Word grise la case pour « Corps de texte » : pas de niveau, pas de repli.
    collapsedDefault: d.outlineLevel > 0 ? d.collapsedDefault : false,
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
  // Aperçu : 3 lignes témoin reflétant alignement/retraits/espacement.
  const previewAlign = d.align === 'justify' ? 'justify' : d.align
  const sample = t('doc_para_sample', { defaultValue: 'Texte exemple' })
  const sampleLine = Array(14).fill(sample).join(' ')
  return (
    <FloatingWindow title={t('doc_paragraph_dialog', { defaultValue: 'Paragraphe' })} onClose={onClose} defaultWidth={640} backdrop className="max-h-[92vh]">
      <div className="p-5 overflow-auto" data-module="office">
        <Tabs className="mb-4" size="sm" value={tab} onChange={v => setTab(v as 'indent' | 'flow')}
          tabs={[{ id: 'indent', label: t('doc_para_tab_indent', { defaultValue: 'Retrait et espacement' }) },
                 { id: 'flow', label: t('doc_para_tab_flow', { defaultValue: 'Enchaînements' }) }]} />

        {tab === 'indent' && (
          <div className="space-y-4">
            <section>
              <h3 className="text-xs font-semibold text-text-secondary uppercase mb-2">{t('doc_para_general', { defaultValue: 'Général' })}</h3>
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary w-28">{t('doc_align', { defaultValue: 'Alignement' })}</span>
                  <LDSel value={d.align} on={v => up({ align: v as ParaDraft['align'] })} opts={[['left', t('doc_align_left', { defaultValue: 'À gauche' })], ['center', t('doc_align_center', { defaultValue: 'Centré' })], ['right', t('doc_align_right', { defaultValue: 'À droite' })], ['justify', t('doc_align_justify', { defaultValue: 'Justifié' })]]} /></label>
                <label className="flex items-center gap-2 text-sm"><span className="text-text-secondary">{t('doc_para_outline', { defaultValue: 'Niveau hiérarchique' })}</span>
                  <LDSel value={String(d.outlineLevel)} on={v => up({ outlineLevel: Number(v) })} opts={[['0', t('doc_para_body', { defaultValue: 'Corps de texte' })], ...Array.from({ length: 9 }, (_, i) => [String(i + 1), t('doc_para_level', { defaultValue: `Niveau ${i + 1}`, n: i + 1 })] as [string, string])]} /></label>
                {/* Word : la case n'a de sens qu'avec un niveau hiérarchique — elle
                    est grisée pour « Corps de texte ». */}
                <LDCheck checked={d.collapsedDefault} disabled={d.outlineLevel === 0}
                  on={b => up({ collapsedDefault: b })}
                  label={t('doc_para_collapsed_default', { defaultValue: 'Réduire par défaut' })} />
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
            <Button className={DLG_BTN} variant="primary" size="sm" onClick={() => { onApply(d); onClose() }}>{t('common_ok', { defaultValue: 'OK' })}</Button>
            <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
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
  // Mini-aperçu de la page (orientation + marges + alignement vertical).
  const pvW = orient === 'landscape' ? 150 : 110, pvH = orient === 'landscape' ? 110 : 150
  return (
    <FloatingWindow title={t('doc_layout_dialog', { defaultValue: 'Mise en page' })} onClose={onClose} defaultWidth={680} backdrop className="max-h-[92vh]">
      <div className="p-5 overflow-auto" data-module="office">
        <Tabs className="mb-4" size="sm" value={tab} onChange={v => setTab(v as 'margins' | 'paper' | 'layout')}
          tabs={[{ id: 'margins', label: t('doc_ps_margins', { defaultValue: 'Marges' }) },
                 { id: 'paper', label: t('doc_ps_paper', { defaultValue: 'Papier' }) },
                 { id: 'layout', label: t('doc_ps_layout', { defaultValue: 'Mise en page' }) }]} />

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
            <Button className={DLG_BTN} variant="primary" size="sm" onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
            <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
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
    { key: 'tight',     mode: 'tight',     label: t('doc_wrap_tight',     { defaultValue: 'Rapproché' }) },
    { key: 'through',   mode: 'through',   label: t('doc_wrap_through',   { defaultValue: 'Au travers' }) },
    { key: 'topBottom', mode: 'topBottom', label: t('doc_wrap_topbottom', { defaultValue: 'Haut et bas' }) },
    { key: 'behind',    mode: 'behind',    label: t('doc_wrap_behind',    { defaultValue: 'Derrière le texte' }) },
    { key: 'front',     mode: 'front',     label: t('doc_wrap_front',     { defaultValue: 'Devant le texte' }) },
  ]
  const [wrap, setWrap] = useState(init.wrap || 'inline')
  const [wrapKey, setWrapKey] = useState(() => WRAP_STYLES.find(s => s.mode === (init.wrap || 'inline'))?.key || 'inline')
  const [wrapSide, setWrapSide] = useState(init.wrapSide || 'both')
  const [dT, setDT] = useState(init.wrapDistT || 0), [dB, setDB] = useState(init.wrapDistB || 0)
  const [dL, setDL] = useState(init.wrapDistL ?? WRAP_DIST_SIDE), [dR, setDR] = useState(init.wrapDistR ?? WRAP_DIST_SIDE)
  // Word grise ce qui n'a pas de sens pour le mode choisi : le CÔTÉ n'existe que
  // pour les habillages qui laissent couler le texte à côté de l'objet ; les
  // distances haut/bas n'ont d'effet que pour « carré » et « haut et bas », les
  // distances gauche/droite que pour les habillages latéraux. « Derrière/devant »
  // ignore les quatre (le texte passe sur l'objet — GraphicImport.cxx:1686-1694).
  const sideEnabled = wrap === 'square' || wrap === 'tight' || wrap === 'through'
  const distTBEnabled = wrap === 'square' || wrap === 'topBottom'
  const distLREnabled = sideEnabled

  // ── Position ────────────────────────────────────────────────────────────
  const [hPos, setHPos] = useState<'align' | 'abs' | 'rel'>(init.wrap === 'inline' ? 'align' : init.alignH ? 'align' : 'abs')
  const [vPos, setVPos] = useState<'align' | 'abs' | 'rel'>(init.wrap === 'inline' ? 'align' : init.alignV ? 'align' : 'abs')
  const [hAlign, setHAlign] = useState(init.alignH ?? (init.align === 'center' ? 'center' : init.align === 'right' ? 'right' : 'left'))
  const [vAlign, setVAlign] = useState(init.alignV ?? 'top')
  const [px, setPx] = useState(init.wrapX || 0), [py, setPy] = useState(init.wrapY || 0)
  const [hRelPos, setHRelPos] = useState(0), [vRelPos, setVRelPos] = useState(0)
  const [hPosRef, setHPosRef] = useState(init.posHRel || 'column')
  const [vPosRef, setVPosRef] = useState(init.posVRel || 'paragraph')
  const [moveWithText, setMoveWithText] = useState(init.moveWithText)
  const [allowOverlap, setAllowOverlap] = useState(init.allowOverlap)
  const [lockAnchor, setLockAnchor] = useState(init.lockAnchor)
  const posDisabled = wrap === 'inline'

  // Taille finale (les décalages « relatifs » et les alignements en dépendent).
  const fw = Math.max(8, wMode === 'rel' ? Math.round((wRelPct / 100) * (wRelRef === 'page' ? init.geom.pageW : init.geom.contentW)) : w)
  const fh = Math.max(8, hMode === 'rel' ? Math.round((hRelPct / 100) * (hRelRef === 'page' ? init.geom.pageH : init.geom.contentH)) : h)

  // Zones d'alignement du référentiel courant (anchor-position.ts). Leur ÉTENDUE
  // sert aux positions en pourcentage ; leur origine n'entre jamais dans un
  // attribut : c'est le moteur qui la connaît, pas le dialogue.
  const { page: pgGeom, anchor: pgAnchor } = anchorFrames(init.geom)
  const hArea = horizontalArea(hPosRef as RelH, pgGeom, pgAnchor)
  const vArea = verticalArea(vPosRef as RelV, pgGeom, pgAnchor)

  // Décalage manuel courant sur chaque axe, quel que soit le mode affiché — c'est
  // ce qui part dans `wrapX`/`wrapY` (sauf en mode « Alignement », où Word n'écrit
  // pas de décalage : `wp:align` et `wp:posOffset` sont exclusifs).
  const offX = () => hPos === 'rel' ? Math.round((hRelPos / 100) * hArea.size)
    : hPos === 'align' ? offsetOfAlign(init.geom, hPosRef as RelH, vPosRef as RelV, fw, fh, hAlign as AlignH, undefined).x
    : px
  const offY = () => vPos === 'rel' ? Math.round((vRelPos / 100) * vArea.size)
    : vPos === 'align' ? offsetOfAlign(init.geom, hPosRef as RelH, vPosRef as RelV, fw, fh, undefined, vAlign as AlignV).y
    : py
  // Changement de mode : on pré-remplit le champ visé avec la valeur équivalente,
  // pour que l'objet ne saute pas au simple fait de basculer un bouton radio.
  // Une position en POURCENTAGE n'a de sens que dans un référentiel à étendue :
  // « colonne » vaut encore, mais « caractère » et « ligne » sont des zones de
  // taille nulle chez Writer (anchoredobjectposition.cxx:332-337 et :745-756) →
  // on retombe sur la marge, comme le fait la liste réduite de Word.
  const pickHPos = (m: 'align' | 'abs' | 'rel') => {
    const cur = offX()
    if (m === 'abs') setPx(cur)
    else if (m === 'rel') {
      const ref = REL_REF.some(([v]) => v === hPosRef) ? hPosRef : 'margin'
      const size = horizontalArea(ref as RelH, pgGeom, pgAnchor).size
      if (ref !== hPosRef) setHPosRef(ref)
      setHRelPos(size ? Number((cur / size * 100).toFixed(1)) : 0)
    }
    setHPos(m)
  }
  const pickVPos = (m: 'align' | 'abs' | 'rel') => {
    const cur = offY()
    if (m === 'abs') setPy(cur)
    else if (m === 'rel') {
      const ref = REL_REF.some(([v]) => v === vPosRef) ? vPosRef : 'margin'
      const size = verticalArea(ref as RelV, pgGeom, pgAnchor).size
      if (ref !== vPosRef) setVPosRef(ref)
      setVRelPos(size ? Number((cur / size * 100).toFixed(1)) : 0)
    }
    setVPos(m)
  }

  const apply = () => {
    // Objet « aligné sur le texte » : ni référentiel ni alignement de flottant,
    // seul `align` (gauche/centre/droite) le positionne dans le flux.
    const floating = wrap !== 'inline'
    onApply({
      width: fw, height: fh, rotation: ((Math.round(rot) % 360) + 360) % 360,
      wrap,
      align: hAlign === 'center' || hAlign === 'right' ? hAlign : 'left',
      wrapX: floating && hPos !== 'align' ? offX() : 0,
      wrapY: floating && vPos !== 'align' ? offY() : 0,
      alignH: floating && hPos === 'align' ? hAlign : null,
      alignV: floating && vPos === 'align' ? vAlign : null,
      wrapSide, wrapDistT: Math.round(dT), wrapDistB: Math.round(dB), wrapDistL: Math.round(dL), wrapDistR: Math.round(dR),
      posHRel: hPosRef, posVRel: vPosRef, moveWithText, allowOverlap, lockAnchor,
    })
    onClose()
  }

  const H = ({ children }: { children: React.ReactNode }) => <div className="text-xs font-semibold text-text-secondary border-b border-border/60 pb-1">{children}</div>

  const REF_H: Array<[string, string]> = [['column', t('doc_layout_column', { defaultValue: 'Colonne' })], ['margin', t('doc_layout_margin', { defaultValue: 'Marge' })], ['page', t('doc_layout_page', { defaultValue: 'Page' })], ['character', t('doc_layout_char', { defaultValue: 'Caractère' })]]
  const REF_V: Array<[string, string]> = [['paragraph', t('doc_layout_paragraph', { defaultValue: 'Paragraphe' })], ['margin', t('doc_layout_margin', { defaultValue: 'Marge' })], ['page', t('doc_layout_page', { defaultValue: 'Page' })], ['line', t('doc_layout_line', { defaultValue: 'Ligne' })]]
  const REL_REF: Array<[string, string]> = [['margin', t('doc_layout_margin', { defaultValue: 'Marge' })], ['page', t('doc_layout_page', { defaultValue: 'Page' })]]
  // « Intérieur »/« Extérieur » : alignements de pages en vis-à-vis (Word
  // `inside`/`outside`, réécrits en gauche/droite selon la parité de la page —
  // GraphicImport.cxx:504-529, ToggleHoriOrientAndAlign :819-871).
  const ALIGN_H: Array<[string, string]> = [['left', t('doc_align_left', { defaultValue: 'À gauche' })], ['center', t('doc_align_center', { defaultValue: 'Centré' })], ['right', t('doc_align_right', { defaultValue: 'À droite' })], ['inside', t('doc_align_inside', { defaultValue: 'Intérieur' })], ['outside', t('doc_align_outside', { defaultValue: 'Extérieur' })]]
  const ALIGN_V: Array<[string, string]> = [['top', t('doc_layout_top', { defaultValue: 'Haut' })], ['center', t('doc_layout_middle', { defaultValue: 'Centré' })], ['bottom', t('doc_layout_bottom', { defaultValue: 'Bas' })], ['inside', t('doc_align_inside', { defaultValue: 'Intérieur' })], ['outside', t('doc_align_outside', { defaultValue: 'Extérieur' })]]

  return (
    <FloatingWindow title={t('doc_layout_dialog', { defaultValue: 'Mise en page' })} onClose={onClose} defaultWidth={640} backdrop>
      <div data-module="office">
        <Tabs className="px-4" size="sm" value={tab} onChange={v => setTab(v as 'position' | 'wrap' | 'size')}
          tabs={[{ id: 'position', label: t('doc_layout_position', { defaultValue: 'Position' }) },
                 { id: 'wrap', label: t('doc_layout_wrap', { defaultValue: 'Habillage du texte' }) },
                 { id: 'size', label: t('doc_layout_size', { defaultValue: 'Taille' }) }]} />
        <div className="p-4 min-h-[320px]">
          {/* ── POSITION ── */}
          {tab === 'position' && (
            <div className="flex flex-col gap-3 text-sm">
              <H>{t('doc_layout_horizontal', { defaultValue: 'Horizontal' })}</H>
              <div className="grid grid-cols-[150px_1fr_auto_auto] items-center gap-x-2 gap-y-2 pl-1">
                <LDRadio checked={hPos === 'align'} on={() => pickHPos('align')} label={t('doc_layout_alignment', { defaultValue: 'Alignement' })} />
                <LDSel value={hAlign} on={setHAlign} opts={ALIGN_H} disabled={hPos !== 'align'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={hPosRef} on={setHPosRef} opts={REF_H} disabled={hPos !== 'align'} />
                <LDRadio checked={hPos === 'abs'} on={() => pickHPos('abs')} label={t('doc_layout_abs_pos', { defaultValue: 'Position absolue' })} />
                <LDNum value={toCm(px)} on={n => setPx(cmToPx(n))} suffix="cm" step={0.1} disabled={hPos !== 'abs'} />
                <span className="text-text-secondary text-right">{t('doc_layout_right_of', { defaultValue: 'à droite de' })}</span>
                <LDSel value={hPosRef} on={setHPosRef} opts={REF_H} disabled={hPos !== 'abs'} />
                <LDRadio checked={hPos === 'rel'} on={() => pickHPos('rel')} label={t('doc_layout_rel_pos', { defaultValue: 'Position relative' })} />
                <LDNum value={hRelPos} on={setHRelPos} suffix="%" disabled={hPos !== 'rel'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={hPosRef} on={setHPosRef} opts={REL_REF} disabled={hPos !== 'rel'} />
              </div>
              <H>{t('doc_layout_vertical', { defaultValue: 'Vertical' })}</H>
              <div className="grid grid-cols-[150px_1fr_auto_auto] items-center gap-x-2 gap-y-2 pl-1">
                <LDRadio checked={vPos === 'align'} on={() => pickVPos('align')} label={t('doc_layout_alignment', { defaultValue: 'Alignement' })} />
                <LDSel value={vAlign} on={setVAlign} opts={ALIGN_V} disabled={vPos !== 'align'} />
                <span className="text-text-secondary text-right">{t('doc_layout_relative_to', { defaultValue: 'par rapport à' })}</span>
                <LDSel value={vPosRef} on={setVPosRef} opts={REF_V} disabled={vPos !== 'align'} />
                <LDRadio checked={vPos === 'abs'} on={() => pickVPos('abs')} label={t('doc_layout_abs_pos', { defaultValue: 'Position absolue' })} />
                <LDNum value={toCm(py)} on={n => setPy(cmToPx(n))} suffix="cm" step={0.1} disabled={vPos !== 'abs'} />
                <span className="text-text-secondary text-right">{t('doc_layout_below', { defaultValue: 'au-dessous de' })}</span>
                <LDSel value={vPosRef} on={setVPosRef} opts={REF_V} disabled={vPos !== 'abs'} />
                <LDRadio checked={vPos === 'rel'} on={() => pickVPos('rel')} label={t('doc_layout_rel_pos', { defaultValue: 'Position relative' })} />
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
              {posDisabled && <div className="text-xs text-text-tertiary pl-1">{t('doc_layout_pos_note', { defaultValue: "La position ne s'applique qu'aux objets flottants (carré, derrière ou devant le texte)." })}</div>}
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
                    <span className="bg-white rounded overflow-hidden"><WrapThumb mode={s.mode} /></span>
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
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_top', { defaultValue: 'Haut' })}</span><LDNum value={toCm(dT)} on={n => setDT(cmToPx(n))} suffix="cm" step={0.05} disabled={!distTBEnabled} width="w-20" /></span>
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_left', { defaultValue: 'Gauche' })}</span><LDNum value={toCm(dL)} on={n => setDL(cmToPx(n))} suffix="cm" step={0.05} disabled={!distLREnabled} width="w-20" /></span>
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_bottom', { defaultValue: 'Bas' })}</span><LDNum value={toCm(dB)} on={n => setDB(cmToPx(n))} suffix="cm" step={0.05} disabled={!distTBEnabled} width="w-20" /></span>
                <span className="flex items-center gap-2"><span className="w-14">{t('doc_layout_right', { defaultValue: 'Droite' })}</span><LDNum value={toCm(dR)} on={n => setDR(cmToPx(n))} suffix="cm" step={0.05} disabled={!distLREnabled} width="w-20" /></span>
              </div>
              {!sideEnabled && !distTBEnabled && (
                <div className="text-xs text-text-tertiary pl-1">{t('doc_layout_wrap_note', { defaultValue: "Le côté et les distances ne s'appliquent qu'aux habillages qui écartent le texte (encadré, rapproché, au travers, haut et bas)." })}</div>
              )}
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
          <Button className={DLG_BTN} size="sm" onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          <button onClick={onClose} className={`rounded-md border border-border px-3 py-1.5 text-sm text-text-secondary hover:bg-surface-2 ${DLG_BTN}`}>{t('common_cancel', { defaultValue: 'Annuler' })}</button>
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

// Bookmarks of a document, in document order. Single scan shared by « Atteindre »
// and by anchor-link navigation (`href="#name"`, what the DOCX import produces for
// a `w:hyperlink w:anchor`).
function docBookmarks(editor: Editor | null | undefined): Array<{ name: string; pos: number }> {
  const out: Array<{ name: string; pos: number }> = []
  editor?.state.doc.descendants((node, pos) => {
    if (node.isText) { const m = node.marks.find(mk => mk.type.name === 'bookmark'); if (m && m.attrs.name) out.push({ name: String(m.attrs.name), pos }) }
  })
  return out
}
// Position targeted by an `#anchor`. Word mangles bookmark names on the way out
// (no whitespace, 40 chars max, cf. `bookmark_ref` in the writer), so fall back to
// a loose comparison when the exact name is not found.
function bookmarkPos(editor: Editor | null | undefined, anchor: string): number | null {
  let raw = anchor
  try { raw = decodeURIComponent(anchor) } catch { /* keep the literal anchor */ }
  const key = (s: string) => s.replace(/[^0-9a-z]/gi, '').toLowerCase()
  const list = docBookmarks(editor)
  const hit = list.find(b => b.name === raw) ?? list.find(b => key(b.name) === key(raw))
  return hit ? hit.pos : null
}

// Atteindre (Word « Atteindre ») : liste les titres et signets ; clic → défilement.
function DocGoToDialog({ editor, opsRef, onClose }: {
  editor: Editor | null; opsRef: React.RefObject<PaginatedOps | null>; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const headings = opsRef.current?.outline() ?? []
  const bookmarks = docBookmarks(editor)
  const go = (pos: number) => { opsRef.current?.scrollToPos(pos); onClose() }
  return (
    <FloatingWindow title={t('doc_go_to', { defaultValue: 'Atteindre' })} onClose={onClose} defaultWidth={360} backdrop>
      <div className="max-h-[60vh] overflow-auto" data-module="office">
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
          <Button className={DLG_BTN} onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          <Button className={DLG_BTN} variant="secondary" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
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
// Dialogue « Espacement des caractères » (Word : Police → Paramètres avancés).
// Étendu (+) / Condensé (−) en points, appliqué à la sélection via textStyle.
function CharSpacingDialog({ initial, onApply, onClose }: { initial: number; onApply: (pt: number) => void; onClose: () => void }) {
  const { t } = useTranslation('office')
  const [mode, setMode] = useState<'normal' | 'expanded' | 'condensed'>(initial > 0 ? 'expanded' : initial < 0 ? 'condensed' : 'normal')
  const [amount, setAmount] = useState(Math.abs(initial) || 1)
  const preview = mode === 'normal' ? 0 : mode === 'expanded' ? amount : -amount
  return (
    <FloatingWindow title={t('doc_char_spacing_title', { defaultValue: 'Espacement des caractères' })} onClose={onClose} defaultWidth={420} backdrop>
      <div className="flex flex-col gap-4" data-module="office">
        <div className="flex items-center gap-3">
          <span className="text-sm text-text-secondary w-28">{t('doc_cs_spacing', { defaultValue: 'Espacement :' })}</span>
          <Dropdown width={150} value={mode} options={[
            { value: 'normal', label: t('doc_cs_normal', { defaultValue: 'Normal' }) },
            { value: 'expanded', label: t('doc_cs_expanded', { defaultValue: 'Étendu' }) },
            { value: 'condensed', label: t('doc_cs_condensed', { defaultValue: 'Condensé' }) },
          ]} onChange={v => setMode(v as 'normal' | 'expanded' | 'condensed')} />
          <span className="text-sm text-text-secondary">{t('doc_cs_of', { defaultValue: 'De :' })}</span>
          <NumberInput className="w-[90px] h-8" min={0.1} max={30} step={0.1} disabled={mode === 'normal'} value={amount} onChange={n => setAmount(n)} />
          <span className="text-sm text-text-secondary">pt</span>
        </div>
        <div className="border border-border rounded-lg p-3 text-center overflow-hidden">
          <span style={{ fontFamily: 'Arial', fontSize: 16, letterSpacing: `${preview}pt` }}>
            {t('doc_cs_preview', { defaultValue: 'Aa Bb Cc Dd Ee' })}
          </span>
        </div>
        <div className="flex justify-end gap-2">
          <Button className={DLG_BTN} onClick={() => onApply(preview)}>{t('common_ok', { defaultValue: 'OK' })}</Button>
          <Button className={DLG_BTN} variant="secondary" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

function TablePropertiesDialog({ editor, rect, onClose, pageBorder, onPageBorderChange }: {
  editor: Editor | null; rect: TableRect; onClose: () => void
  // Onglet « Bordure de page » du sous-dialogue Bordure et trame : la bordure est une
  // propriété du DOCUMENT, elle vit donc dans le composant parent.
  pageBorder?: PageBorderDef | null
  onPageBorderChange?: (pb: PageBorderDef | null) => void
}) {
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
  const [wrapAround, setWrapAround] = useState((a.tableWrap as string) === 'around')
  const [hdrRepeat, setHdrRepeat] = useState(!!a.headerRepeat)
  const [valign, setVAlign] = useState<'top' | 'center' | 'bottom'>((editor?.getAttributes('tableCell').cellVAlign as 'top' | 'center' | 'bottom') || 'top')
  const [altTitle, setAltTitle] = useState((a.altTitle as string) || '')
  const [altDesc, setAltDesc] = useState((a.altDesc as string) || '')
  const [curRow, setCurRow] = useState(ctx?.rowIndex ?? 0)
  const [curCol, setCurCol] = useState(ctx?.colStart ?? 0)
  // Sous-dialogues de l'onglet Tableau (façon Word) : Position… / Bordure et trame… /
  // Options… Rendus DANS la même fenêtre (page dédiée + retour), plutôt qu'en modal
  // imbriqué.
  const [sub, setSub] = useState<null | 'pos' | 'borders' | 'options'>(null)
  const DEF_CM = { t: 2, b: 2, l: 6, r: 6 }   // marges de cellule par défaut du moteur (px)
  const [cellM, setCellM] = useState({
    t: (a.cellMarginTop    as number | null) ?? DEF_CM.t,
    b: (a.cellMarginBottom as number | null) ?? DEF_CM.b,
    l: (a.cellMarginLeft   as number | null) ?? DEF_CM.l,
    r: (a.cellMarginRight  as number | null) ?? DEF_CM.r,
  })
  const [autofit, setAutofit] = useState((a.tableLayout as string) !== 'fixed')
  const [wrapD, setWrapD] = useState({
    t: (a.wrapDistTop    as number | null) ?? 4,
    b: (a.wrapDistBottom as number | null) ?? 8,
    l: (a.wrapDistLeft   as number | null) ?? 12,
    r: (a.wrapDistRight  as number | null) ?? 12,
  })
  const [bColor, setBColor] = useState((a.tableBorderColor as string) || '#bdc1c6')
  const [bWidth, setBWidth] = useState(Number(a.tableBorderWidth) || 1)
  const [bStyle, setBStyle] = useState((a.tableBorderStyle as string) || 'solid')
  const [shade, setShade] = useState((editor?.getAttributes('tableCell').cellBg as string) || '')
  const [bsTab, setBsTab] = useState<'b' | 'p' | 's'>('b')
  const [bsType, setBsType] = useState('all')
  const [bsScope, setBsScope] = useState<'table' | 'cell'>('table')
  const [pgB, setPgB] = useState<PageBorderDef | null>(pageBorder ?? null)
  const [cellSp, setCellSp] = useState(Number(a.cellSpacing) || 0)
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
      tableWrap: wrapAround ? 'around' : 'none',
      headerRepeat: hdrRepeat,
      tableLayout: autofit ? 'autofit' : 'fixed',
      cellMarginTop: cellM.t, cellMarginBottom: cellM.b, cellMarginLeft: cellM.l, cellMarginRight: cellM.r,
      wrapDistTop: wrapD.t, wrapDistBottom: wrapD.b, wrapDistLeft: wrapD.l, wrapDistRight: wrapD.r,
      tableBorderColor: bColor, tableBorderWidth: bWidth, tableBorderStyle: bStyle === 'solid' ? null : bStyle,
      cellSpacing: cellSp || null,
    }
    setTableAttrAt(editor, ctx.tablePos, attrs)
    const shadeRect: TableRect = bsScope === 'table' ? { r0: 0, c0: 0, r1: rowCount - 1, c1: colCount - 1 } : rect
    setCellsAttr(editor, rect, { cellVAlign: valign })
    if (shade !== ((editor.getAttributes('tableCell').cellBg as string) || '')) {
      setCellsAttr(editor, shadeRect, { cellBg: shade || null })
    }
    if (onPageBorderChange && JSON.stringify(pgB ?? null) !== JSON.stringify(pageBorder ?? null)) onPageBorderChange(pgB ?? null)
    onClose()
  }
  const TABS: Array<[typeof tab, string]> = [
    ['table', t('doc_tp_tab_table', { defaultValue: 'Tableau' })], ['row', t('doc_tp_tab_row', { defaultValue: 'Ligne' })],
    ['col', t('doc_tp_tab_col', { defaultValue: 'Colonne' })], ['cell', t('doc_tp_tab_cell', { defaultValue: 'Cellule' })],
    ['alt', t('doc_tp_tab_alt', { defaultValue: 'Texte de remplacement' })],
  ]
  return (
    <FloatingWindow title={t('doc_table_properties_title', { defaultValue: 'Propriétés du tableau' })} onClose={onClose} defaultWidth={560} backdrop>
      <div className="flex flex-col gap-3 text-sm" data-module="office">
        {/* ── Sous-dialogues de l'onglet Tableau (façon Word) ────────────────── */}
        {sub && (() => {
          const mmField = (val: number, on: (px: number) => void) => (
            <NumberInput className="w-[92px] h-8" min={0} max={200} step={0.01}
              value={Math.round((val / CM_PX) * 100) / 100} onChange={cm => on(Math.round(cm * CM_PX))} />
          )
          const back = (
            <div className="flex justify-end gap-2 pt-2">
              <Button className={DLG_BTN} onClick={() => setSub(null)}>{t('common_ok', { defaultValue: 'OK' })}</Button>
              <Button className={DLG_BTN} variant="secondary" onClick={() => setSub(null)}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
            </div>
          )
          const title = sub === 'pos' ? t('doc_tp_pos_title', { defaultValue: 'Positionnement du tableau' })
            : sub === 'borders' ? t('doc_tp_borders_title', { defaultValue: 'Bordure et trame' })
            : t('doc_tp_options_title', { defaultValue: 'Options du tableau' })
          return (
            <div className="flex flex-col gap-3">
              <div className="flex items-center gap-2 border-b border-border pb-2">
                <button onClick={() => setSub(null)} className="p-1 rounded hover:bg-hover text-text-secondary" aria-label={t('common_back', { defaultValue: 'Retour' })}>
                  <ChevronLeft size={16} />
                </button>
                <span className="font-medium">{title}</span>
              </div>
              <div className="min-h-[260px] flex flex-col gap-4">
                {sub === 'options' && (<>
                  <div>
                    <div className="text-text-secondary mb-2">{t('doc_tp_cell_margins', { defaultValue: 'Marges de cellule par défaut' })}</div>
                    <div className="grid grid-cols-2 gap-3 max-w-[380px]">
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_top', { defaultValue: 'Haut :' })}</span>{mmField(cellM.t, v => setCellM({ ...cellM, t: v }))}</label>
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_left', { defaultValue: 'Gauche :' })}</span>{mmField(cellM.l, v => setCellM({ ...cellM, l: v }))}</label>
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_bottom', { defaultValue: 'Bas :' })}</span>{mmField(cellM.b, v => setCellM({ ...cellM, b: v }))}</label>
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_right', { defaultValue: 'Droite :' })}</span>{mmField(cellM.r, v => setCellM({ ...cellM, r: v }))}</label>
                    </div>
                  </div>
                  <div>
                    <div className="text-text-secondary mb-2">{t('doc_tp_cell_spacing', { defaultValue: 'Espacement des cellules par défaut' })}</div>
                    <label className="flex items-center gap-3">
                      <Checkbox checked={cellSp > 0} onChange={v => setCellSp(v ? Math.max(2, cellSp) : 0)} />
                      <span className="text-text-secondary">{t('doc_tp_allow_spacing', { defaultValue: 'Autoriser l’espacement entre les cellules' })}</span>
                      {mmField(cellSp, setCellSp)}
                    </label>
                  </div>
                  <label className="flex items-center gap-2">
                    <Checkbox checked={autofit} onChange={setAutofit} />
                    <span className="text-text-secondary">{t('doc_tp_autofit_contents', { defaultValue: 'Redimensionner automatiquement pour ajuster au contenu' })}</span>
                  </label>
                  <div className="text-xs text-text-tertiary max-w-[420px]">
                    {t('doc_tp_autofit_hint', { defaultValue: 'Décoché : les largeurs de colonne sont fixes et le texte est renvoyé à la ligne au lieu d’élargir la colonne.' })}
                  </div>
                </>)}
                {sub === 'borders' && (<>
                  {/* Trois onglets, comme Word : Bordures / Bordure de page / Trame de fond. */}
                  <Tabs className="-mt-1" size="sm" value={bsTab} onChange={k => setBsTab(k as 'b' | 'p' | 's')}
                    tabs={[{ id: 'b', label: t('doc_bs_tab_borders', { defaultValue: 'Bordures' }) },
                           { id: 'p', label: t('doc_bs_tab_page', { defaultValue: 'Bordure de page' }) },
                           { id: 's', label: t('doc_bs_tab_shading', { defaultValue: 'Trame de fond' }) }]} />
                  {bsTab === 'b' && (
                    <div className="flex gap-6">
                      <div className="flex flex-col gap-1">
                        <div className="text-text-secondary mb-1">{t('doc_bs_type', { defaultValue: 'Type :' })}</div>
                        {([['none', t('doc_bs_none', { defaultValue: 'Aucune' })],
                           ['outside', t('doc_bs_box', { defaultValue: 'Encadrement' })],
                           ['all', t('doc_bs_all', { defaultValue: 'Tous' })],
                           ['grid', t('doc_bs_grid', { defaultValue: 'Quadrillage' })]] as Array<[string, string]>).map(([k, l]) => (
                          <button key={k} onClick={() => setBsType(k)}
                            className={`flex items-center gap-2 px-2 py-1 rounded border ${bsType === k ? 'border-accent' : 'border-transparent hover:bg-hover'}`}>
                            <BorderIcon preset={k === 'grid' ? 'all' : (k as BorderPreset)} />
                            <span className="text-text-secondary">{l}</span>
                          </button>
                        ))}
                      </div>
                      <div className="flex flex-col gap-3 flex-1">
                        <div className="flex items-center gap-3">
                          <span className="w-20 text-text-secondary">{t('doc_tp_border_style', { defaultValue: 'Style :' })}</span>
                          <Dropdown width={150} value={bStyle} options={[
                            { value: 'solid', label: t('doc_bstyle_solid', { defaultValue: 'Plein' }) },
                            { value: 'dashed', label: t('doc_bstyle_dashed', { defaultValue: 'Tirets' }) },
                            { value: 'dotted', label: t('doc_bstyle_dotted', { defaultValue: 'Points' }) },
                          ]} onChange={setBStyle} />
                        </div>
                        <div className="flex items-center gap-3">
                          <span className="w-20 text-text-secondary">{t('doc_tp_border_color', { defaultValue: 'Couleur :' })}</span>
                          <ColorField color={bColor} onChange={setBColor} />
                        </div>
                        <div className="flex items-center gap-3">
                          <span className="w-20 text-text-secondary">{t('doc_tp_border_width', { defaultValue: 'Largeur :' })}</span>
                          <Dropdown width={150} value={String(bWidth)} options={[[1, '½ pt'], [1.5, '1 pt'], [2, '1½ pt'], [3, '2¼ pt'], [4, '3 pt']].map(([v, l]) => ({ value: String(v), label: String(l) }))} onChange={v => setBWidth(parseFloat(v))} />
                        </div>
                        <div className="flex items-center gap-3">
                          <span className="w-20 text-text-secondary">{t('doc_bs_apply_to', { defaultValue: 'Appliquer à :' })}</span>
                          <Dropdown width={150} value={bsScope} options={[
                            { value: 'table', label: t('doc_bs_scope_table', { defaultValue: 'Tableau' }) },
                            { value: 'cell', label: t('doc_bs_scope_cell', { defaultValue: 'Cellule' }) },
                          ]} onChange={v => setBsScope(v as 'table' | 'cell')} />
                        </div>
                        <div className="flex items-center gap-2">
                          <Button variant="secondary" onClick={() => {
                            const pen = { w: bWidth, s: bStyle as 'solid' | 'dashed' | 'dotted', c: bColor }
                            const scope: TableRect = bsScope === 'table' ? { r0: 0, c0: 0, r1: rowCount - 1, c1: colCount - 1 } : rect
                            if (bsType === 'grid') {
                              applyBorderPreset(editor, scope, 'outside', pen)
                              applyBorderPreset(editor, scope, 'inside', { ...pen, w: 1 })
                            } else {
                              applyBorderPreset(editor, scope, bsType as BorderPreset, pen)
                            }
                          }}>{t('doc_bs_apply', { defaultValue: 'Appliquer' })}</Button>
                          <span className="text-xs text-text-tertiary">{t('doc_bs_custom_hint', { defaultValue: 'Personnalisé : choisir côté par côté ci-dessous.' })}</span>
                        </div>
                        <BorderGallery
                          onPick={p => {
                            const pen = { w: bWidth, s: bStyle as 'solid' | 'dashed' | 'dotted', c: bColor }
                            const scope: TableRect = bsScope === 'table' ? { r0: 0, c0: 0, r1: rowCount - 1, c1: colCount - 1 } : rect
                            applyBorderPreset(editor, scope, p, pen)
                          }}
                          label={p => t(`doc_border_${p}`, { defaultValue: BORDER_LABELS_FR[p] })} />
                      </div>
                    </div>
                  )}
                  {bsTab === 'p' && (
                    <div className="flex flex-col gap-3">
                      <div className="text-xs text-text-tertiary max-w-[440px]">
                        {t('doc_bs_page_hint', { defaultValue: 'Cadre tracé dans la marge de chaque page du document.' })}
                      </div>
                      <label className="flex items-center gap-2">
                        <Checkbox checked={!!pgB} onChange={v => setPgB(v ? (pgB ?? { ...DEFAULT_PAGE_BORDER }) : null)} />
                        <span className="text-text-secondary">{t('doc_bs_page_on', { defaultValue: 'Encadrement de page' })}</span>
                      </label>
                      <div className="flex items-center gap-3">
                        <span className="w-20 text-text-secondary">{t('doc_tp_border_style', { defaultValue: 'Style :' })}</span>
                        <Dropdown width={150} value={pgB?.style ?? 'solid'} options={[
                          { value: 'solid', label: t('doc_bstyle_solid', { defaultValue: 'Plein' }) },
                          { value: 'dashed', label: t('doc_bstyle_dashed', { defaultValue: 'Tirets' }) },
                          { value: 'dotted', label: t('doc_bstyle_dotted', { defaultValue: 'Points' }) },
                          { value: 'double', label: t('doc_bstyle_double', { defaultValue: 'Double' }) },
                        ]} onChange={v => pgB && setPgB({ ...pgB, style: v as PageBorderDef['style'] })} />
                      </div>
                      <div className="flex items-center gap-3">
                        <span className="w-20 text-text-secondary">{t('doc_tp_border_color', { defaultValue: 'Couleur :' })}</span>
                        <ColorField color={pgB?.color ?? '#1a73e8'} onChange={c => pgB && setPgB({ ...pgB, color: c })} />
                      </div>
                      <div className="flex items-center gap-3">
                        <span className="w-20 text-text-secondary">{t('doc_tp_border_width', { defaultValue: 'Largeur :' })}</span>
                        <NumberInput className="w-[92px] h-8" min={0.5} max={8} step={0.5} value={pgB?.width ?? 2} onChange={n => pgB && setPgB({ ...pgB, width: n })} />
                        <span className="text-text-secondary">{t('doc_bs_margin', { defaultValue: 'Distance au bord :' })}</span>
                        <NumberInput className="w-[92px] h-8" min={0} max={96} step={1} value={pgB?.margin ?? 24} onChange={n => pgB && setPgB({ ...pgB, margin: Math.round(n) })} />
                      </div>
                    </div>
                  )}
                  {bsTab === 's' && (
                    <div className="flex flex-col gap-3">
                      <div className="flex items-center gap-3">
                        <span className="w-24 text-text-secondary">{t('doc_bs_fill', { defaultValue: 'Remplissage :' })}</span>
                        <ColorField color={shade || '#ffffff'} onChange={setShade} />
                        <Button variant="secondary" onClick={() => setShade('')}>{t('doc_tp_no_fill', { defaultValue: 'Aucune couleur' })}</Button>
                      </div>
                      <div className="flex items-center gap-3">
                        <span className="w-24 text-text-secondary">{t('doc_bs_apply_to', { defaultValue: 'Appliquer à :' })}</span>
                        <Dropdown width={150} value={bsScope} options={[
                          { value: 'table', label: t('doc_bs_scope_table', { defaultValue: 'Tableau' }) },
                          { value: 'cell', label: t('doc_bs_scope_cell', { defaultValue: 'Cellule' }) },
                        ]} onChange={v => setBsScope(v as 'table' | 'cell')} />
                      </div>
                      <div className="text-xs text-text-tertiary max-w-[440px]">
                        {t('doc_bs_shading_hint', { defaultValue: 'La trame est appliquée à la validation (OK).' })}
                      </div>
                    </div>
                  )}
                </>)}
                {sub === 'pos' && (<>
                  <div className="text-xs text-text-tertiary max-w-[440px]">
                    {t('doc_tp_pos_hint', { defaultValue: 'Tableau flottant (habillage « Autour ») : la position horizontale se règle par l’alignement et le retrait de l’onglet Tableau.' })}
                  </div>
                  <div>
                    <div className="text-text-secondary mb-2">{t('doc_tp_dist_text', { defaultValue: 'Distance du texte environnant' })}</div>
                    <div className="grid grid-cols-2 gap-3 max-w-[380px]">
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_top', { defaultValue: 'Haut :' })}</span>{mmField(wrapD.t, v => setWrapD({ ...wrapD, t: v }))}</label>
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_left', { defaultValue: 'Gauche :' })}</span>{mmField(wrapD.l, v => setWrapD({ ...wrapD, l: v }))}</label>
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_bottom', { defaultValue: 'Bas :' })}</span>{mmField(wrapD.b, v => setWrapD({ ...wrapD, b: v }))}</label>
                      <label className="flex items-center gap-2"><span className="w-16 text-text-secondary">{t('doc_tp_right', { defaultValue: 'Droite :' })}</span>{mmField(wrapD.r, v => setWrapD({ ...wrapD, r: v }))}</label>
                    </div>
                  </div>
                </>)}
              </div>
              {back}
            </div>
          )
        })()}
        {!sub && <Tabs size="sm" tabs={TABS.map(([k, l]) => ({ id: k, label: l }))} value={tab} onChange={k => setTab(k as typeof tab)} />}
        <div className={sub ? "" : "min-h-[300px]"}>
          {!sub && tab === 'table' && (
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
              <div>
                <div className="text-text-secondary mb-1">{t('doc_tp_wrap', { defaultValue: 'Habillage du texte' })}</div>
                <div className="flex items-end gap-6">
                  {([['none', t('doc_tp_wrap_none', { defaultValue: 'Aucun' })], ['around', t('doc_tp_wrap_around', { defaultValue: 'Autour' })]] as Array<[string, string]>).map(([v, lbl]) => (
                    <button key={v} onClick={() => setWrapAround(v === 'around')} className="flex flex-col items-center gap-1">
                      <span className={`w-20 h-16 border-2 rounded flex items-center justify-center ${(v === 'around') === wrapAround ? 'border-accent' : 'border-border'}`}>
                        {v === 'none' ? (
                          <span className="w-12 h-10 flex flex-col gap-[3px]"><span className="border-t border-text-tertiary w-full" /><span className="w-7 h-5 border border-text-tertiary" /><span className="border-t border-text-tertiary w-full" /></span>
                        ) : (
                          <span className="w-12 h-10 flex gap-[3px]"><span className="w-6 h-6 border border-text-tertiary shrink-0" /><span className="flex-1 flex flex-col gap-[3px] pt-[2px]"><span className="border-t border-text-tertiary w-full" /><span className="border-t border-text-tertiary w-full" /><span className="border-t border-text-tertiary w-full" /></span></span>
                        )}
                      </span>
                      <span className="text-xs text-text-secondary">{lbl}</span>
                    </button>
                  ))}
                  <div className="text-xs text-text-tertiary max-w-[220px] pb-1">{t('doc_tp_wrap_hint', { defaultValue: 'Autour : le texte coule à côté d\'un tableau plus étroit que la page.' })}</div>
                </div>
              </div>
              {/* Trois sous-dialogues, comme dans Word. « Position… » n'a de sens
                  qu'avec un habillage « Autour » (tableau flottant). */}
              <div className="flex justify-end gap-2 pt-1">
                <Button variant="secondary" disabled={!wrapAround} onClick={() => setSub('pos')}>
                  {t('doc_tp_position', { defaultValue: 'Position…' })}
                </Button>
                <Button variant="secondary" onClick={() => setSub('borders')}>
                  {t('doc_tp_borders_shading', { defaultValue: 'Bordure et trame…' })}
                </Button>
                <Button variant="secondary" onClick={() => setSub('options')}>
                  {t('doc_tp_options', { defaultValue: 'Options…' })}
                </Button>
              </div>
            </div>
          )}
          {!sub && tab === 'row' && (
            <div className="flex flex-col gap-4">
              <div className="text-text-secondary">{t('doc_tp_row_n', { defaultValue: 'Ligne {{n}} :', n: curRow + 1 })}</div>
              <div className="flex items-center gap-3">
                <label className="flex items-center gap-2"><Checkbox checked={(rowH[curRow] || 0) > 0} onChange={v => setRowH(rowH.map((h, i) => i === curRow ? (v ? Math.max(0.5 * CM_PX, h) : 0) : h))} /><span className="text-text-secondary">{t('doc_tp_specify_height', { defaultValue: 'Spécifier la hauteur :' })}</span></label>
                {cmField((rowH[curRow] || 0) / CM_PX, cm => setRowH(rowH.map((h, i) => i === curRow ? cm * CM_PX : h)), (rowH[curRow] || 0) <= 0)}
                <div className="flex items-center gap-2"><span className="text-text-secondary">{t('doc_tp_row_height', { defaultValue: 'Hauteur :' })}</span>
                  <Dropdown width={120} value={rowModes[curRow] || 'atleast'} options={[{ value: 'atleast', label: t('doc_tp_atleast', { defaultValue: 'Au moins' }) }, { value: 'exactly', label: t('doc_tp_exactly', { defaultValue: 'Exactement' }) }]} onChange={v => setRowModes(rowModes.map((m, i) => i === curRow ? v as 'atleast' | 'exactly' : m))} /></div>
              </div>
              <label className="flex items-center gap-2"><Checkbox checked={allowBreak} onChange={setAllowBreak} /><span className="text-text-secondary">{t('doc_tp_allow_break', { defaultValue: 'Autoriser le fractionnement des lignes sur plusieurs pages' })}</span></label>
              <label className="flex items-center gap-2"><Checkbox checked={hdrRepeat} onChange={setHdrRepeat} /><span className="text-text-secondary">{t('doc_tp_header_repeat', { defaultValue: 'Répéter en haut de chaque page en tant que ligne d\u2019en-tête' })}</span></label>
              <div className="flex gap-2">
                <Button variant="secondary" disabled={curRow <= 0} onClick={() => setCurRow(r => Math.max(0, r - 1))}>▲ {t('doc_tp_prev_row', { defaultValue: 'Ligne précédente' })}</Button>
                <Button variant="secondary" disabled={curRow >= rowCount - 1} onClick={() => setCurRow(r => Math.min(rowCount - 1, r + 1))}>▼ {t('doc_tp_next_row', { defaultValue: 'Ligne suivante' })}</Button>
              </div>
            </div>
          )}
          {!sub && tab === 'col' && (
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
          {!sub && tab === 'cell' && (
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
          {!sub && tab === 'alt' && (
            <div className="flex flex-col gap-3">
              <label className="flex flex-col gap-1"><span className="text-text-secondary">{t('doc_tp_alt_title', { defaultValue: 'Titre' })}</span>
                <input value={altTitle} onChange={e => setAltTitle(e.target.value)} className="px-2 py-1.5 rounded border border-border bg-surface outline-none focus:border-accent" /></label>
              <label className="flex flex-col gap-1"><span className="text-text-secondary">{t('doc_tp_alt_desc', { defaultValue: 'Description' })}</span>
                <textarea value={altDesc} onChange={e => setAltDesc(e.target.value)} rows={5} className="px-2 py-1.5 rounded border border-border bg-surface outline-none focus:border-accent resize-none" /></label>
              <p className="text-xs text-text-tertiary">{t('doc_tp_alt_help', { defaultValue: 'Les titres et descriptions fournissent des représentations textuelles des informations contenues dans le tableau, pour les personnes en situation de handicap visuel ou cognitif.' })}</p>
            </div>
          )}
        </div>
        {!sub && (
          <div className="flex justify-end gap-2 border-t border-border pt-3">
            <Button className={DLG_BTN} onClick={apply}>{t('common_ok', { defaultValue: 'OK' })}</Button>
            <Button className={DLG_BTN} variant="secondary" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
          </div>
        )}
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
  // Word rouvre TOUJOURS un document développé : le seul état qui voyage avec le
  // fichier est « Réduire par défaut » (`<w15:collapsed/>`). On réaligne donc
  // l'état de repli sur ce drapeau à chaque ouverture.
  for (const node of content) {
    const a = node.attrs as Record<string, unknown> | undefined
    if (a && ('collapsed' in a || 'collapsedDefault' in a)) a.collapsed = !!a.collapsedDefault
  }
  return { type: 'doc', content: content.length ? content : [{ type: 'paragraph' }] }
}

interface PaginatedOps {
  setOrientation:    (o: Orientation) => void
  setColumns:        (n: number) => void
  openParagraph:     () => void
  insertBreak:       () => void
  insertPageBreak:   () => void
  pageCount:         () => number
  /** Plan du document : titres avec niveau, position PM et numéro de page. */
  outline:           () => Array<{ text: string; level: number; pos: number; page: number }>
  /** Place le curseur à `pos` et amène la page correspondante à l'écran. */
  scrollToPos:       (pos: number) => void
  /** Page (1-based, sans décalage de numérotation) contenant une position PM. */
  pageAt:            (pos: number) => number
  /** Rend chaque page sur un canvas hors écran (échelle ×n) — export PDF. */
  exportPageCanvases: (scale?: number) => Array<{ canvas: HTMLCanvasElement; wPx: number; hPx: number }>
  /** En-tête/pied : contexte de la section du curseur (liaison Word). */
  hfContext:    (kind: 'header' | 'footer') => { secIdx: number; linked: boolean; zones: HFContent; firstPage: boolean }
  setSectionHF: (kind: 'header' | 'footer', zones: HFContent, linked: boolean) => { applyBase: boolean }
  /** Marges de la section du curseur ; applyBase=true → section de base (parent). */
  setSectionMargins: (m: SectionDef['margins']) => { applyBase: boolean }
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
  /** Habillage de l'objet sélectionné — convertit le NŒUD (bloc ⇄ inline) au besoin. */
  setObjectWrap: (wrap: string) => void
  /** Ouvre « Mise en page » (Position / Habillage / Taille) de l'objet sélectionné. */
  openObjectLayout: () => void
  /** Position PM d'ancrage d'un commentaire (début de sa marque) ou null si l'ancre a disparu. */
  commentAnchor: (id: string) => number | null
  /** Dimensions (px @ zoom 1) de la page courante — pour le zoom « ajuster ». */
  pageGeom: () => { pageW: number; pageH: number }
  /** Position/taille (px, repère du contenu défilable) du canvas de la page `idx` —
   *  pour recaler les règles sur la page active en disposition grille. */
  pageContentBox: (idx: number) => { left: number; top: number; w: number; h: number } | null
  editFootnote?: (pos: number) => void
  editEndnote?: (pos: number) => void
  tableMetrics?: (tablePos: number) => { rowHeights: number[]; colWidths: number[] } | null
  /** Bornes du tableau du curseur (px, repère contenu de sa page) pour les règles. */
  tableRuler?: () => { cols: number[]; rows: number[]; cell?: { x0: number; x1: number } } | null
  /** Démarre le glissé d'une borne de tableau depuis un repère de règle. */
  tableBoundDown?: (axis: 'col' | 'row', index: number, e: React.PointerEvent) => void
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
    const dy = pgs[idx]?.startY ?? 0
    // baseline suit le même repère que y (repli layout continu → coordonnées de page).
    return { idx, cm: { ...gc, y: gc.y - dy, baseline: gc.baseline != null ? gc.baseline - dy : undefined } }
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
  // Section du curseur : orientation + index + SES marges (la règle suit la section active).
  onActiveSection?:   (info: { orientation: Orientation; secIdx: number; margins: SectionDef['margins'] }) => void
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
  // Modification de la bordure de page depuis l'onglet « Bordure de page » du
  // sous-dialogue Bordure et trame (la bordure est une propriété du document).
  onPageBorder?:      (pb: PageBorderDef | null) => void
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
  headingNumbers?:    boolean
  /** Entrée/sortie du mode d'édition inline en-tête/pied (barre contextuelle). */
  onHFActive?:        (active: boolean, ctx: HFBarCtx | null, hfEditor: Editor | null) => void
  /** Écriture d'une zone d'en-tête/pied depuis l'édition inline. */
  onCommitHF?:        (kind: 'header' | 'footer', zones: HFContent) => void
  /** Entrée/sortie de l'édition in-place d'une zone de texte riche (routage barre d'outils). */
  onTbActive?:        (active: boolean, tbEditor: Editor | null) => void
  /** Correcteur orthographe activé. */
  spellCheck?:        boolean
  /** Vérification grammaticale activée. */
  grammarCheck?:      boolean
  /** Catégories de grammaire activées (undefined = toutes). */
  grammarRules?:      Record<string, boolean>
  /** Ouvre le panneau « Vérification » sur une faute de grammaire (menu contextuel). */
  onOpenGrammarCheck?: (issue: SpellIssue) => void
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
  /** Word's content control around a table of contents (frame + small toolbar). */
  tocControl?: { onPreset: (p: TocPreset) => void; onRemove: () => void; onUpdate: () => void }
  /** « Flèches de plan » (Word pour Mac : Préférences → Affichage). Masquées, les
   *  triangles de repli disparaissent ; le menu contextuel reste disponible. */
  outlineArrows?: boolean
  /** Remonte la liste des commentaires ancrés présents dans le document. */
  onCommentRanges?:   (ids: string[]) => void
  /** Crée un commentaire sur la sélection (menu contextuel). */
  onAddComment?:      () => void
  /** Y.Map collaborative des fils de commentaires (marge ancrée). */
  commentsMap?:       Y.Map<CommentThread>
  /** Utilisateur courant (auteur des commentaires/réponses). */
  commentUser?:       { id: string; name: string }
  /** Affiche la marge de commentaires ancrée + réserve la gouttière (sort du multipage). */
  commentsVisible?:   boolean
  /** Remonte la sélection de plage de cellules de tableau (null = aucune). */
  onTableSel?:        (sel: (TableRect & { tableStart: number }) | null) => void
  /** Outil FORME armé (galerie du ruban) : le prochain glissé sur une page dessine
   *  cette géométrie au lieu de sélectionner du texte. */
  armedShape?:        ShapeKind | null
  /** Le geste de dessin est terminé (posé ou abandonné) → désarmer côté ruban. */
  onShapeDrawn?:      () => void
  /** « Reproduire la mise en forme » armé : curseur pinceau sur les pages. */
  paintMode?:         boolean
}

// Contexte transmis à la barre contextuelle d'en-tête/pied (options Word).
export interface HFBarCtx { band: 'header' | 'footer'; secIdx: number; linked: boolean; canLink: boolean; firstPage: boolean }

// Pont pincement→peinture entre DocumentEditorArea (geste tactile) et
// PaginatedEditor (pipeline canvas) : pendant un commit de zoom EN COURS de
// geste, on peint en demi-résolution (4× moins de pixels — un commit plein-res
// gèle ~500ms le fil principal sur machine lente, mesuré au banc CPU ×6). Le
// plein-res revient au commit de relâchement, ou via refine() si le geste se
// termine sans commit. Un seul éditeur actif à la fois → état module accepté.
const pinchPaint = { coarse: false, refine: null as (() => void) | null }

function PaginatedEditor({ initialDoc, ydoc, awareness, collabEmpty, section, zoom, scrollContainerRef, onEditor, onSave, onBaseChange, onActiveSection, onRegisterOps, pageNumbers = 'none', header, footer, hfFirstPage = false, paper = 'a4', docTitle = '', pageBg, watermark = null, pageBorder = null, onPageBorder, lineNumbers = null, showBoundaries = false, showMarks = false, pageNumFormat = 'arabic', pageNumStart = 1, headingNumbers = false, onHFActive, onCommitHF, onTbActive, spellCheck = true, grammarCheck = true, grammarRules, onOpenGrammarCheck, onSpellCount, onStats, spellVersion = 0, searchRanges, searchActive = 0, activeCommentId = null, onCommentActivate, onCommentRanges, onAddComment, commentsMap, commentUser, commentsVisible = false, onTableSel, tocControl, outlineArrows = true, armedShape = null, onShapeDrawn, paintMode = false }: PaginatedEditorProps) {
  const { t, i18n: i18nInst } = useTranslation('office')
  const g = getGeometry(section, paper)
  const cbRef = useRef({ onBaseChange, onActiveSection, onHFActive, onCommitHF, onTbActive, onCommentActivate, onCommentRanges, onAddComment, onTableSel, onStats })
  cbRef.current = { onBaseChange, onActiveSection, onHFActive, onCommitHF, onTbActive, onCommentActivate, onCommentRanges, onAddComment, onTableSel, onStats }
  // Sélection de plage de cellules de tableau (rectangle de grille + table d'ancrage).
  const [tableSel, setTableSel] = useState<(TableRect & { tableStart: number }) | null>(null)
  const tableSelRef = useRef<(TableRect & { tableStart: number }) | null>(null); tableSelRef.current = tableSel
  // Surbrillances de recherche : ref lue par renderAllPages.
  const searchRef = useRef<{ ranges: Array<{ from: number; to: number }>; active: number }>({ ranges: [], active: 0 })
  searchRef.current = { ranges: searchRanges ?? [], active: searchActive }
  const activeCommentRef = useRef<string | null>(activeCommentId)
  activeCommentRef.current = activeCommentId
  // Plages de commentaires (id → from/to global) recalculées à chaque transaction.
  const commentRangesRef = useRef<Array<{ id: string; from: number; to: number }>>([])
  // Recalcul des positions de la marge de commentaires (repagination/zoom/édition).
  const [commentTick, setCommentTick] = useState(0)
  const commentBumpRaf = useRef(0)
  const bumpComments = useCallback(() => {
    if (commentBumpRaf.current) return
    commentBumpRaf.current = requestAnimationFrame(() => { commentBumpRaf.current = 0; setCommentTick(n => (n + 1) & 0xffff) })
  }, [])
  // Commentaire visé par le clic droit sur du texte commenté (menu contextuel).
  const ctxCommentRef = useRef<string | null>(null)
  // Action « Modifier / Répondre » déclenchée depuis un menu contextuel → gouttière.
  const [gutterAction, setGutterAction] = useState<CommentExtAction | null>(null)
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
  // `kind`/`adj` : présents UNIQUEMENT pour un objet `kbshape:` — ils portent les
  // poignées jaunes d'ajustement (moteur partagé shapes/adjust). Absents pour une
  // image bitmap ou une zone de texte, qui n'en ont pas.
  const [imgSel, setImgSel] = useState<
    { pos: number; cx: number; cy: number; w: number; h: number; rotation: number; wrap: string; moveWithText: boolean; kind?: string; adj?: number[] } | null
  >(null)
  // Position PM de l'objet sélectionné, lue par le hit-test (passe « MARKED » de
  // svx : l'objet déjà sélectionné garde le clic — svdview.cxx:339-341).
  const imgSelRef = useRef<number | null>(null)
  imgSelRef.current = imgSel ? imgSel.pos : null
  // ── Outil FORME armé (dessin au glissement, façon présentations) ────────────
  // Armé par la galerie du ruban ; le prochain appui gauche sur une page DESSINE.
  const armedShapeRef = useRef<ShapeKind | null>(armedShape)
  armedShapeRef.current = armedShape
  const onShapeDrawnRef = useRef(onShapeDrawn)
  onShapeDrawnRef.current = onShapeDrawn
  // Actions d'objet définies BIEN PLUS BAS (elles dépendent de la conversion de
  // nœud et du dialogue de mise en page) : les ops passent par cette indirection
  // plutôt que de dupliquer la logique.
  const imgSetWrapRef = useRef<(wrap: string) => void>(() => {})
  const lateOpsRef = useRef<{ openLayout: () => void }>({ openLayout: () => {} })
  // Page où un geste de dessin est en cours (null = aucun) : monte le calque
  // d'aperçu, dont la boîte est ensuite peinte en IMPÉRATIF (pas de setState par
  // frame de glissé, cf. ShapeGhostLayer).
  // Un calque par PAGE (le tracé peut déborder de la feuille de départ, et même
  // en chevaucher deux) : chacun peint la portion qui lui revient et n'alloue de
  // pixels que s'il est effectivement touché — cf. ShapeGhostLayer.
  const [drawPage, setDrawPage] = useState<{ idx: number; kind: ShapeKind } | null>(null)
  const ghostRefs = useRef<Array<ShapeGhostHandle | null>>([])
  // Mini-barre flottante (composant partagé FormattingMiniBar) sur sélection du corps.
  // Elle n'apparaît QUE sur sélection à la SOURIS (miniBarMouseRef) ; son opacité suit
  // la PROXIMITÉ de la souris (fondu géré en impératif via barElRef) ; une fois fondue à
  // 0 (miniBarDismissedRef) elle ne revient qu'à la prochaine sélection ; elle reste
  // ancrée à la sélection (suit le scroll, ne se ferme pas au défilement).
  const [bodyMiniBar, setBodyMiniBar] = useState<{ left: number; top: number; caret?: boolean } | null>(null)
  const barElRef = useRef<HTMLDivElement | null>(null)
  const miniBarMouseRef = useRef(false)      // la sélection courante a été faite à la souris
  const miniBarDismissedRef = useRef(false)  // fondue à 0 → attendre la prochaine sélection
  const miniBarMoveRef = useRef({ x: -1, y: -1 })  // dernières coords souris traitées (filtre scroll)
  // Menu d'insertion au CARET (« tap again » sur le caret, façon Word/iOS) : le
  // drapeau autorise recomputeBodyMiniBar à afficher malgré une sélection VIDE.
  const miniBarCaretRef = useRef(false)
  // Masquage temporaire façon plateforme (AOSP FloatingActionMode) : pendant le
  // DÉFILEMENT et pendant le GLISSÉ des poignées, le menu disparaît puis revient
  // à l'arrêt du geste. (≠ dismissed : le suppress est réversible sans re-sélection.)
  const miniBarSuppressedRef = useRef(false)
  const miniBarScrollTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  // Panneau « Options de disposition » de l'objet sélectionné (ouvert/fermé).
  const [wrapPanel, setWrapPanel] = useState(false)
  // Curseurs distants (présence collaborative) projetés en coordonnées écran.
  const [remoteCursors, setRemoteCursors] = useState<RemoteCursor[]>([])
  // Poignées de sélection TACTILES (gouttes façon Android/Word mobile) : coords
  // overlay (repère rootRef, scrollent avec les pages) des deux extrémités de la
  // sélection. Null = pas de sélection texte ou pointeur fin (souris).
  const [selHandles, setSelHandles] = useState<{ from: { left: number; top: number; height: number }; to: { left: number; top: number; height: number } } | null>(null)
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
  // Indices des pages actuellement peintes ET à jour (culling par viewport) : une
  // page hors bande visible n'est pas repeinte et sort de cet ensemble ; le
  // défilement la repeint à la volée (cf. paintNewlyVisible).
  const renderedPagesRef    = useRef<Set<number>>(new Set())
  const caretRef            = useRef<HTMLDivElement>(null)
  // Glisser-déposer de la sélection (comme Word) : caret de DÉPÔT qui suit la souris
  // pendant le glissé + drapeau consulté par onPageMouseMove (curseur « move »).
  const dropCaretRef        = useRef<HTMLDivElement>(null)
  const textDragRef         = useRef(false)
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

  // Position ÉCRAN (repère du conteneur d'overlays) de l'ancre d'un commentaire :
  // haut de la ligne ancrée + bord droit de sa page → sert à poser la carte dans la
  // marge droite (cf. CommentGutter). Même repère que le caret (défile avec le contenu).
  const commentAnchorScreen = (id: string): CommentAnchorScreen | null => {
    const layout = contLayoutRef.current
    const r = commentRangesRef.current.find(c => c.id === id)
    if (!layout || !r) return null
    const c = posToCoords(layout, r.from)
    const pgs = pagesRef.current, z = zoomRef.current
    if (!pgs.length) return null
    let idx = 0
    for (let k = 0; k < pgs.length; k++) if (c.y >= pgs[k].startY - 0.5) idx = k
    const geom = geomOf(pgs[idx])
    const { left, top } = pageOrigin(idx)
    const dy = pgs[idx]?.startY ?? 0
    return {
      pageIdx: idx,
      top: top + (geom.marginV + (c.y - dy)) * z,
      lineH: (c.height || 16) * z,
      pageRight: left + geom.pageW * z,
      anchorX: left + (geom.marginH + c.x) * z,
    }
  }

  // ── Contrôle de contenu de la TABLE DES MATIÈRES (façon Word) ──────────────
  // Rectangle ÉCRAN du bloc de TDM qui contient le caret, ou null. Recalculé sur
  // sélection, repagination et zoom — les mêmes signaux que la marge de
  // commentaires, dont il reprend la conversion layout → écran.
  const [tocRect, setTocRect] = useState<TocControlRect | null>(null)
  const tocRangeRef = useRef<{ from: number; to: number } | null>(null)
  const computeTocRect = useCallback((): TocControlRect | null => {
    const ed = editorRef.current, layout = contLayoutRef.current
    if (!ed || !layout) return null
    const here = ed.state.selection.from
    // Bloc = suite CONTIGUË de blocs de la table qui contient le caret.
    let from = -1, to = -1, off = 0, inside = false
    const isToc = (n: { attrs?: Record<string, unknown> | null }) => {
      const a = n.attrs as Record<string, unknown>
      return a?.tocKind === 'toc' || (!a?.tocKind && (a?.tocTitle === true || a?.tocLevel != null))
    }
    ed.state.doc.forEach(node => {
      const end = off + node.nodeSize
      if (isToc(node)) {
        if (from < 0 || to !== off) { from = off; to = end } else { to = end }
        if (here >= from && here <= to) inside = true
      } else if (!inside && from >= 0) {
        from = -1; to = -1
      }
      off = end
    })
    if (!inside || from < 0) { tocRangeRef.current = null; return null }
    tocRangeRef.current = { from, to }
    const pgs = pagesRef.current, z = zoomRef.current
    if (!pgs.length) return null
    const a = posToCoords(layout, from + 1)
    const b = posToCoords(layout, Math.max(from + 1, to - 1))
    let idx = 0
    for (let k = 0; k < pgs.length; k++) if (a.y >= pgs[k].startY - 0.5) idx = k
    const geom = geomOf(pgs[idx])
    const { left, top } = pageOrigin(idx)
    const dy = pgs[idx]?.startY ?? 0
    const y0 = (geom.marginV + (a.y - dy)) * z
    // Une table à cheval sur deux pages : le cadre s'arrête au bas du contenu de
    // la page d'ancrage — Word dessine un contrôle par page, pas un cadre géant.
    const rawY1 = (geom.marginV + (b.y - dy) + (b.height || 16)) * z
    const y1 = Math.min(rawY1, (geom.marginV + geom.contentH) * z)
    return {
      left: left + geom.marginH * z,
      top: top + y0,
      // `contentW`, jamais `pageW - 2 * marginH` : la marge DROITE peut différer
      // de la gauche, et le numéro de page d'une entrée est peint au bord droit
      // de la zone de contenu — un cadre trop étroit le laissait dehors.
      width: geom.contentW * z,
      height: Math.max(12, y1 - y0),
    }
  }, [])
  useEffect(() => {
    const ed = editorRef.current
    if (!ed || !tocControl) return
    const refresh = () => setTocRect(computeTocRect())
    refresh()
    ed.on('selectionUpdate', refresh)
    ed.on('transaction', refresh)
    return () => { ed.off('selectionUpdate', refresh); ed.off('transaction', refresh) }
  }, [computeTocRect, tocControl, pages, zoom])

  // Doc PM pour lequel le layout courant a été calculé. `onSelectionUpdate` peut
  // arriver AVANT le `recompute` de `onUpdate` (ordre d'émission TipTap) : dessiner
  // le caret avec un layout périmé le téléportait (et faisait défiler la vue) vers
  // une mauvaise page. On recompute donc à la demande, sans jamais le faire deux
  // fois pour le même doc (identité du nœud ProseMirror).
  // Numérotation automatique des titres : option de document, hors modèle PM →
  // bascule = re-layout forcé + repaint.
  const headingNumbersRef2 = useRef(headingNumbers); headingNumbersRef2.current = headingNumbers
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
    const widths = geoms.map(x => x.colW)
    // La géométrie de FEUILLE (pageW/pageH/marges) accompagne celle du TEXTE : la
    // pagination en a besoin pour résoudre les référentiels « page » et « marges »
    // et pour confiner un objet ancré. Sans elle, elle retombait sur les marges
    // par défaut du moteur (1 pouce) et confinait donc à une feuille imaginaire —
    // et surtout à une feuille DIFFÉRENTE de celle du rattrapage ci-dessous.
    const pgGeoms = geoms.map(x => ({
      contentH: x.contentH, columns: x.columns, colW: x.colW, colGap: x.colGap,
      pageW: x.pageW, pageH: x.pageH, marginL: x.marginH, marginT: x.marginV,
    }))
    const layout = layoutDocumentMulti(json, widths, { headingNumbers: headingNumbersRef2.current })
    const pgs = paginateMulti(layout, pgGeoms)
    // Un objet flottant qui déborde du bas (ou du haut) de sa page est ROGNÉ au bord
    // du contenu et sa partie cachée est RE-DESSINÉE en continuation sur la page
    // voisine (page vide ajoutée en fin de document au besoin).
    // Même géométrie de feuille que la pagination (2ᵉ argument) : les deux étapes
    // confinent l'objet, elles doivent le faire sur LA MÊME feuille.
    const sheetFor = (s: number) => pgGeoms[s] ?? pgGeoms[pgGeoms.length - 1] ?? pgGeoms[0]
    splitFloatingImagesAcrossPages(pgs, s => sheetFor(s).contentH, sheetFor)
    contLayoutRef.current = layout
    pagesRef.current = pgs
    setPages(pgs)
  }, [])

  // Relayout forcé COALESCÉ (une frame) : plusieurs déclencheurs pilotés par effets
  // (marges, zoom, montage, police, numérotation des titres) qui surviennent dans la
  // même frame se réduisent à UN SEUL relayout complet. Sans cela, un glissé de marge
  // sur un gros document empile des dizaines de relayouts (chacun O(document)) et fige
  // le thread principal. Le rendu suit via l'effet [pages] (setPages → renderAllPages).
  const recomputeRafRef = useRef(0)
  const scheduleRecompute = useCallback(() => {
    if (recomputeRafRef.current) return
    recomputeRafRef.current = requestAnimationFrame(() => {
      recomputeRafRef.current = 0
      const ed = editorRef.current
      if (ed) recompute(ed, true)
    })
  }, [recompute])
  useEffect(() => () => { if (recomputeRafRef.current) cancelAnimationFrame(recomputeRafRef.current) }, [])

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
      // Triangle Développer/Réduire (Word) dans la marge gauche. Comme dans Word :
      // ▶ (replié) reste TOUJOURS visible ; ▼ (développé) n'apparaît qu'au SURVOL
      // du titre — pas de chevrons parasites sur tous les titres. Écran seulement
      // (`dimHF` = rendu à l'écran) : jamais dans l'export PDF/impression.
      for (const para of (dimHF && outlineArrowsRef.current) ? pgD.layout.paragraphs : []) {
        if (para.table) continue
        const first = para.lines[0]; if (!first) continue
        const node = docD.nodeAt(para.pmStart)
        if (!node || outlineLevelOf(node) === 0) continue
        const collapsed = !!node.attrs?.collapsed
        // Word montre le triangle au survol ET quand le curseur est DANS le titre
        // (seul moyen de le faire apparaître sur écran tactile).
        const caretIn = caretHeadingRef.current === para.pmStart
        if (!collapsed && para.pmStart !== hoverHeadingRef.current && !caretIn) continue
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
    // ── Notes de bas de page (façon Word) : bloc au bas de la page — court trait
    // séparateur puis « n. texte » (8-9 pt), avec retour à la ligne simple. Les
    // zones dessinées sont mémorisées (fnBoxesRef) pour le clic → édition.
    if (pgD) {
      const notes = pageFootnotes(pgD)
      if (notes.length) {
        const maxW = gg.pageW - 2 * gg.marginH
        const yTop = gg.pageH - gg.marginBottom + 4
        cx.save()
        cx.strokeStyle = '#9aa0a6'; cx.lineWidth = 1
        cx.beginPath(); cx.moveTo(gg.marginH, yTop + 0.5); cx.lineTo(gg.marginH + Math.min(150, maxW / 3), yTop + 0.5); cx.stroke()
        cx.font = '9px Arial'; cx.fillStyle = '#3c4043'; cx.textAlign = 'left'; cx.textBaseline = 'alphabetic'
        let y = yTop + 13
        const boxes: Array<{ x0: number; y0: number; x1: number; y1: number; pos: number }> = []
        for (const nt of notes) {
          if (y > gg.pageH - 6) break   // marge basse pleine → tronquer
          const y0 = y - 9
          const words = (nt.text || '…').split(/\s+/)
          let line = nt.n + '. '
          for (const w of words) {
            const test = line + w + ' '
            if (cx.measureText(test).width > maxW && line.trim()) {
              cx.fillText(line, gg.marginH, y); y += 11
              line = '    ' + w + ' '
              if (y > gg.pageH - 6) break
            } else line = test
          }
          if (line.trim() && y <= gg.pageH - 6) { cx.fillText(line, gg.marginH, y); y += 11 }
          boxes.push({ x0: gg.marginH, y0, x1: gg.marginH + maxW, y1: y - 8, pos: nt.pos })
        }
        fnBoxesRef.current.set(idx, boxes)
        cx.restore()
      } else fnBoxesRef.current.delete(idx)
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
        // Same reason as `countBodyLines`: an injected fragment is not the end
        // of a paragraph, so it must not get its own mark.
        if (para.lines.some(l => l.phantom)) continue
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
  // Marges de la section sec (0 = base ; sinon attrs du (sec-1)e break, défaut = base).
  const marginsOfSec = (sec: number): SectionDef['margins'] => {
    const ed = editorRef.current
    const base = sectionRef.current.margins
    if (!ed || sec === 0) return { ...base }
    const pos = breakPositions(ed.state.doc)[sec - 1]
    const a = pos != null ? ed.state.doc.nodeAt(pos)?.attrs : null
    if (!a) return { ...base }
    const num = (v: unknown, d: number) => typeof v === 'number' ? v : d
    return { top: num(a.top, base.top), right: num(a.right, base.right), bottom: num(a.bottom, base.bottom), left: num(a.left, base.left) }
  }
  const reportActiveSection = () => {
    const sec = currentSecIdx()
    cbRef.current.onActiveSection?.({ orientation: orientationOfSec(sec), secIdx: sec, margins: marginsOfSec(sec) })
  }
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
    if (sec === 0) { cbRef.current.onBaseChange?.({ orientation: o }); reportActiveSection(); return }
    const pos = breakPositions(ed.state.doc)[sec - 1]
    if (pos == null) return
    ed.chain().command(({ tr }) => {
      const node = ed.state.doc.nodeAt(pos)
      if (!node) return false
      tr.setNodeMarkup(pos, undefined, { ...node.attrs, orientation: o })
      return true
    }).run()
    reportActiveSection()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Applique des marges à la SECTION du curseur. Retourne applyBase=true pour la
  // section 0 (base gérée par le parent, cf. Y.Map annulable). Pour une section de
  // saut : setNodeMarkup → marges DANS le document → annulables (Ctrl+Z) + collab.
  const setSectionMargins = useCallback((m: SectionDef['margins']): { applyBase: boolean } => {
    const ed = editorRef.current; if (!ed) return { applyBase: true }
    const sec = currentSecIdx()
    if (sec === 0) return { applyBase: true }
    const pos = breakPositions(ed.state.doc)[sec - 1]
    if (pos == null) return { applyBase: true }
    ed.chain().command(({ tr }) => {
      const node = ed.state.doc.nodeAt(pos); if (!node) return false
      tr.setNodeMarkup(pos, undefined, { ...node.attrs, top: m.top, right: m.right, bottom: m.bottom, left: m.left })
      return true
    }).run()
    reportActiveSection()
    return { applyBase: false }
  // eslint-disable-next-line react-hooks/exhaustive-deps
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
  // Outline = headings AND paragraphs promoted through « Ajouter le texte »
  // (`outlineLevel`), minus the headings demoted to body text. One shared rule,
  // in `references/outline.ts`, so the TOC and the navigation panel agree.
  const outline = useCallback(() => {
    const ed = editorRef.current; if (!ed) return []
    return collectOutline(ed.state.doc, pos => pageIndexForHead(pagesRef.current, pos + 1) + 1)
  }, [])

  const scrollToPos = useCallback((pos: number) => {
    const ed = editorRef.current; if (!ed) return
    const p = Math.min(pos + 1, ed.state.doc.content.size)
    ed.chain().focus().setTextSelection(p).run()   // onSelectionUpdate → drawCaret(true) → scroll
  }, [])

  const pageBgRef = useRef(pageBg); pageBgRef.current = pageBg

  // Solid background of a page (section color, else document-level color), or
  // undefined when the background is a CSS gradient (rare) — gradients cannot be
  // painted into the canvas, so those pages keep a transparent canvas over a CSS
  // background. Solid pages get an OPAQUE canvas with the color painted in, which
  // is what lets Skia use subpixel (LCD) text antialiasing — the same rasterizer
  // path as DOM text / Word, instead of the washed-out grayscale AA of
  // transparent canvases.
  const solidPageBg = useCallback((secIdx: number): string | undefined => {
    const v = secMetaRef.current[secIdx]?.pageColor ?? pageBgRef.current ?? '#ffffff'
    return (typeof CSS !== 'undefined' && CSS.supports?.('color', v)) ? v : undefined
  }, [])

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
    // Dimensions RENDUES d'un tableau (géométrie du layout continu) : hauteurs de
    // rangées / largeurs de colonnes — pour « Uniformiser » (Word).
    const tableMetrics = (tablePos: number) => {
      const layout = contLayoutRef.current
      const t = layout?.paragraphs.find(p => p.table && p.pmStart === tablePos)?.table
      if (!t?.rowY || !t.colX || t.rowY.length < 2 || t.colX.length < 2) return null
      return {
        rowHeights: t.rowY.slice(1).map((y, k) => y - t.rowY![k]),
        colWidths:  t.colX.slice(1).map((x, k) => x - t.colX![k]),
      }
    }
    // Bornes du tableau du CURSEUR pour les repères des RÈGLES (façon Word :
    // « Déplacer la colonne du tableau » / « Déplacer la ligne du tableau »).
    // Repère : px de CONTENU de la page qui porte le fragment du curseur.
    const tableRuler = () => {
      const ed = editorRef.current
      const ctx = ed ? tableCtxOf(ed) : null
      if (!ctx) return null
      for (const pg of pagesRef.current) {
        for (const para of pg.layout.paragraphs) {
          const tb = para.table
          if (!tb || para.pmStart !== ctx.tablePos || !tb.colX || !tb.rowY) continue
          if (para.lines.some(l => l.phantom)) continue   // réplique d'en-tête épinglé
          // Cellule du curseur : ses bornes servent d'ANCRES aux marqueurs de retrait
          // (dans un tableau, Word les recale sur la cellule et non sur la page).
          const cc = tb.cells.find(c => c.r === ctx.rowIndex && c.c <= ctx.colStart && c.c + c.colspan - 1 >= ctx.colStart)
          return { cols: tb.colX.slice(), rows: tb.rowY.slice(),
                   cell: cc ? { x0: cc.x, x1: cc.x + cc.w } : undefined }
        }
      }
      return null
    }
    // Un repère de règle démarre le MÊME glissé que la bande sur la bordure.
    // ⚠️ L'indice 0 est le BORD du tableau, pas une bordure interne : l'envoyer au
    // redimensionnement indexait `[index - 1]` et écrivait une hauteur/largeur
    // aberrante. Comme dans Word, le repère de gauche DÉPLACE le tableau (retrait) ;
    // celui du haut n'a pas d'action ici (le déplacement vertical n'existe pas).
    const tableBoundDown = (axis: 'col' | 'row', index: number, e: React.PointerEvent) => {
      const ed = editorRef.current
      const ctx = ed ? tableCtxOf(ed) : null
      if (!ctx) return
      const m = tableRuler(); if (!m) return
      if (index === 0) {
        if (axis === 'col') tableMoveDragRef.current?.(e, ctx.tablePos)
        return
      }
      const n = axis === 'col' ? m.cols.length : m.rows.length
      const isEdge = index === n - 1
      startTableResize(axis === 'col' ? (isEdge ? 'colEdge' : 'col') : (isEdge ? 'rowEdge' : 'row'), ctx.tablePos, index)(e)
    }
    onRegisterOps?.({ setOrientation, setColumns, openParagraph: () => openParagraphDialog(), insertBreak, insertPageBreak, pageCount, outline, scrollToPos, pageAt: (p: number) => pageIndexForHead(pagesRef.current, p + 1) + 1, exportPageCanvases, hfContext, setSectionHF, setSectionMargins, setSectionBg, enterHF: (k) => enterHFEdit(k), exitHF: exitHFEdit, switchHF: switchHFBand, insertHFField, insertTextBox: insertTextBoxOp, editTextBox: enterTextBoxEdit, setObjectWrap: (w: string) => imgSetWrapRef.current(w), openObjectLayout: () => lateOpsRef.current.openLayout(), commentAnchor, pageGeom, pageContentBox, editFootnote: (pos: number) => openFootnoteEditor(pos), editEndnote: openEndnote, tableMetrics, tableRuler, tableBoundDown })
    return () => onRegisterOps?.(null)
  }, [onRegisterOps, setOrientation, setColumns, insertBreak, insertPageBreak, pageCount, outline, scrollToPos, exportPageCanvases, hfContext, setSectionHF, setSectionMargins, setSectionBg, enterHFEdit, exitHFEdit, switchHFBand, insertHFField, insertTextBoxOp, enterTextBoxEdit])

  // Hauteur (px) du bas du conteneur de défilement MASQUÉE par le clavier
  // virtuel : le clavier ne réduit PAS `clientHeight` (seul `visualViewport`
  // rétrécit), donc sans ça un caret « visible » selon le conteneur peut être
  // caché derrière le clavier. < 40px = repli de barre d'URL, ignoré.
  const keyboardOverlap = useCallback((): number => {
    const sc = scrollContainerRef.current
    const vv = typeof window !== 'undefined' ? window.visualViewport : null
    if (!sc || !vv) return 0
    const ov = sc.getBoundingClientRect().bottom - (vv.offsetTop + vv.height)
    return ov > 40 ? ov : 0
  }, [scrollContainerRef])

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
    // Hauteur du caret : la boîte de police du caractère au curseur (cm, ancrée sur la
    // ligne de base — cf. posToCoords). MAIS si des marques stockées (police/taille
    // choisies sans sélection) sont actives, le prochain caractère aura CETTE taille →
    // on prévisualise sa hauteur tout de suite, en restant ANCRÉ SUR LA BASELINE
    // (sinon le caret grandissait vers le bas au lieu de grandir comme le glyphe).
    let caretH = cm.height
    let caretY = cm.y
    const sm = editorRef.current?.state.storedMarks
    if (sm && sm.length) {
      const ts = sm.find(m => m.type.name === 'textStyle' && m.attrs.fontSize)
      if (ts) {
        const pt = parseFloat(String(ts.attrs.fontSize))
        if (!isNaN(pt)) {
          caretH = pt * (96 / 72) * 1.2   // NATURAL_EM du moteur canvas
          // ~80 % de la boîte au-dessus de la baseline (ratio ascent/em des polices latines).
          if (cm.baseline != null) caretY = cm.baseline - caretH * 0.8
        }
      }
    }
    // Cellule à texte vertical : caret = barre HORIZONTALE (perpendiculaire au flux),
    // longueur = hauteur de ligne, ancrée au bord supérieur local projeté.
    if (cm.rot) {
      const cl = leftOffset + (geom.marginH + cm.x) * z
      const ct = pageTop + (geom.marginV + cm.y) * z
      caret.style.display = 'block'
      caret.style.transform = 'none'; caret.style.transformOrigin = 'top left'
      caret.style.height = `${1 / (window.devicePixelRatio || 1)}px`   // épaisseur 1 pixel machine (caret tourné), comme Word
      caret.style.width  = `${caretH * z}px`
      caret.style.top    = `${ct}px`
      caret.style.left   = `${cm.rot === 270 ? cl : cl - caretH * z}px`
      caret.style.animation = 'none'; void caret.offsetHeight; caret.style.animation = '_gdocs_blink 1s 0.5s infinite'
      if (scrollIntoView) { const sc = scrollContainerRef.current; if (sc) { const M = 48; const visH = sc.clientHeight - keyboardOverlap(); if (ct < sc.scrollTop + M) sc.scrollTop = Math.max(0, ct - M); else if (ct > sc.scrollTop + visH - M) sc.scrollTop = ct - visH + M } }
      return
    }
    caret.style.display = 'block'
    // Épaisseur = 1 pixel PHYSIQUE exact, comme Word. `1px` CSS devient 1,25–2 pixels
    // machine dès que l'écran/le zoom navigateur est à 125–200 % (devicePixelRatio > 1) —
    // le caret paraissait « 2× plus épais que Word ». 1/dpr px CSS = 1 pixel machine.
    // (La branche texte vertical remplace la largeur par la longueur de ligne.)
    const dpr = window.devicePixelRatio || 1
    caret.style.width  = `${1 / dpr}px`
    // Position calée sur la grille de PIXELS PHYSIQUES : posé à un `left` fractionnaire,
    // le trait serait anti-aliasé à cheval sur deux pixels → épaisseur perçue variable.
    const snap = (v: number) => Math.round(v * dpr) / dpr
    caret.style.left   = `${snap(leftOffset + (geom.marginH + cm.x) * z)}px`
    caret.style.top    = `${snap(pageTop + (geom.marginV + caretY) * z)}px`
    caret.style.height = `${Math.max(1, Math.round(caretH * z * dpr) / dpr)}px`
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
        // Hauteur RÉELLEMENT visible = conteneur MOINS la zone masquée par le
        // clavier virtuel (cf. keyboardOverlap) → le caret reste au-dessus du
        // clavier, comme dans les éditeurs mobiles concurrents.
        const ch = sc.clientHeight - keyboardOverlap()
        // Cible calculée UNE SEULE FOIS, sur le scrollTop COURANT (avant tout reflow) :
        //  - caret sous la vue → on l'amène en bas avec marge ; au-dessus → en haut ;
        //  - caret DÉJÀ visible → cible = scroll inchangé.
        // On RÉAFFIRME cette même cible en rAF SANS recalculer : pendant la re-pagination
        // (setPages), le navigateur réinitialise transitoirement scrollTop à 0 ; recalculer
        // alors croirait le caret « hors vue » et ferait sauter la page. Réaffirmer la cible
        // d'origine annule ce reset (caret visible → on remet exactement le scroll d'avant).
        const cur = sc.scrollTop
        // Garde-fou : si la bande visible (au-dessus du clavier) est plus courte
        // que le caret+marge, on privilégie de garder le HAUT du caret visible
        // (min avec caretTop) plutôt que de le pousser au-dessus du conteneur.
        const target = caretBot > cur + ch ? Math.max(0, Math.min(caretBot - ch + M, caretTop))
                     : caretTop < cur       ? Math.max(0, caretTop - M)
                     : cur
        sc.scrollTop = target
        requestAnimationFrame(() => { if (sc.scrollTop !== target) sc.scrollTop = target })
      }
    }
  }, [scrollContainerRef, keyboardOverlap])

  // Apparition/agrandissement du CLAVIER virtuel → si le caret passe derrière le
  // clavier, la page se repositionne pour le garder visible (comme Word/Docs
  // mobile). Détecté via visualViewport (le layout viewport ne change pas). Un
  // léger différé laisse le clavier finir son animation avant de mesurer.
  useEffect(() => {
    const vv = typeof window !== 'undefined' ? window.visualViewport : null
    if (!vv) return
    let lastH = vv.height
    let t1: ReturnType<typeof setTimeout> | undefined, t2: ReturnType<typeof setTimeout> | undefined
    const onResize = () => {
      const shrank = vv.height < lastH - 60      // clavier qui apparaît/grandit
      const grew   = vv.height > lastH + 60      // clavier qui disparaît
      lastH = vv.height
      const sc = scrollContainerRef.current
      // REMBOURRAGE BAS = hauteur du clavier : sans lui, un caret en FIN de
      // document (scrollTop déjà au max) ne PEUT pas remonter au-dessus du
      // clavier (rien en dessous où défiler). Le padding crée cette marge, comme
      // les éditeurs mobiles concurrents. Retiré quand le clavier disparaît.
      if (sc) { const ov = keyboardOverlap(); sc.style.paddingBottom = ov > 0 ? `${ov}px` : '' }
      if (!shrank && !grew) return
      const ed = editorRef.current
      if (!ed || !ed.isEditable || !ed.state.selection.empty) return
      // Après pose du padding (nouveau scrollHeight) → amener le caret en vue.
      clearTimeout(t1); clearTimeout(t2)
      t1 = setTimeout(() => drawCaret(true), 100)
      t2 = setTimeout(() => drawCaret(true), 320)   // 2ᵉ passe (fin d'animation iOS)
    }
    vv.addEventListener('resize', onResize)
    return () => { vv.removeEventListener('resize', onResize); clearTimeout(t1); clearTimeout(t2) }
  }, [drawCaret])

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

  // Recalcule la position des poignées de sélection tactiles (idempotent, même
  // logique anti-boucle que recomputeRemoteCursors). Pointeur fin → jamais de
  // poignées (la souris redimensionne déjà la sélection par ses bords).
  const recomputeSelHandles = useCallback(() => {
    const ed = editorRef.current
    const coarse = typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches
    const clear = () => setSelHandles(prev => (prev ? null : prev))
    if (!ed || !coarse) { clear(); return }
    const sel = ed.state.selection
    if (!(sel instanceof TextSelection) || sel.empty) { clear(); return }
    const a = screenPosForHead(sel.from)
    const b = screenPosForHead(sel.to)
    if (!a || !b) { clear(); return }
    setSelHandles(prev => (prev
      && prev.from.left === a.left && prev.from.top === a.top && prev.from.height === a.height
      && prev.to.left === b.left && prev.to.top === b.top && prev.to.height === b.height)
      ? prev : { from: a, to: b })
  }, [screenPosForHead])

  // ── Poignée d'INSERTION (goutte UNIQUE sous le caret, façon Word Android /
  // goutte native AOSP) : apparaît après un tap qui pose le caret, se GLISSE
  // pour le placer précisément (loupe), un TAP dessus bascule le menu
  // d'insertion (Coller · Tout sélectionner). Éphémère : ~4s sans interaction,
  // masquée à la frappe/scroll ; remplacée par les poignées de sélection dès
  // qu'une sélection existe.
  // ── Barres d'outils de COLONNE / LIGNE (survol des en-têtes) ─────────────────
  // Survol de la 1ʳᵉ rangée → pastille AU-DESSUS de la cellule (déplacer la colonne,
  // trier, insérer à droite). Survol de la 1ʳᵉ colonne → pastille À GAUCHE de la
  // ligne (déplacer la ligne, épingler l'en-tête jusqu'à cette ligne, insérer en
  // dessous). Même idiome que Google Docs.
  type BandAxis = 'col' | 'row'
  interface BandBar { axis: BandAxis; tableStart: number; page: number; idx: number; span: number; left: number; top: number }
  // Une pastille PAR AXE : la cellule (0,0) est à la fois en 1ʳᵉ rangée et en 1ʳᵉ
  // colonne, elle affiche donc les DEUX (colonne au-dessus, ligne à gauche).
  const [bandBars, setBandBars] = useState<BandBar[]>([])
  const bandBarsRef = useRef<BandBar[]>([]); bandBarsRef.current = bandBars
  // Le pointeur passe du canvas à la pastille (qui est HORS du tableau) : on ne
  // referme donc pas sur `mouseleave` du canvas mais après un court délai, annulé
  // dès que le pointeur entre dans la pastille.
  const bandCloseRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  // Déclaré ici (avant closeBandSoon, qui le consulte) ; alimenté plus bas.
  const bandDragRef = useRef<{ axis: BandAxis; from: number; boundary: number; ghost: number; size: number; origin: number; tableStart: number; page: number } | null>(null)
  const keepBand = useCallback(() => { clearTimeout(bandCloseRef.current) }, [])
  const closeBandSoon = useCallback(() => {
    // Jamais pendant un glissé : le pointeur QUITTE la pastille dès qu'on commence à
    // tirer, ce qui armait la fermeture et faisait disparaître la pastille en cours
    // de déplacement.
    if (bandDragRef.current) return
    clearTimeout(bandCloseRef.current)
    bandCloseRef.current = setTimeout(() => setBandBars([]), 220)
  }, [])
  useEffect(() => () => clearTimeout(bandCloseRef.current), [])

  // Géométrie de la cellule d'en-tête d'une bande (rangée 0 pour 'col', colonne 0
  // pour 'row') dans le repère des OVERLAYS (celui du caret).
  const bandGeom = useCallback((axis: BandAxis, pageIdx: number, tableStart: number, idx: number): BandBar | null => {
    const pg = pagesRef.current[pageIdx]; if (!pg) return null
    const geom = geomOf(pg)
    const { left, top: pageTop } = pageOrigin(pageIdx)
    const z = zoomRef.current
    for (const para of pg.layout.paragraphs) {
      if (!para.table || para.pmStart !== tableStart) continue
      const cell = axis === 'col'
        ? para.table.cells.find(c => c.r === 0 && c.c === idx)
        : para.table.cells.find(c => c.c === 0 && c.r === idx)
      if (!cell) continue
      const ox = left + geom.marginH * z, oy = pageTop + geom.marginV * z
      const cx = ox + cell.x * z, cy = oy + cell.y * z
      const span = axis === 'col' ? cell.colspan : cell.rowspan
      return axis === 'col'
        ? { axis, tableStart, page: pageIdx, idx, span, left: cx + (cell.w * z) / 2, top: cy - COLBAR_H - 3 }
        : { axis, tableStart, page: pageIdx, idx, span, left: cx - 6, top: cy + (cell.h * z) / 2 }
    }
    return null
  }, [])
  // Recalage après reflow / zoom (la pastille suit sa cellule).
  useEffect(() => {
    const cur = bandBarsRef.current
    if (!cur.length) return
    const next = cur.map(b => bandGeom(b.axis, b.page, b.tableStart, b.idx)).filter((b): b is BandBar => !!b)
    const same = next.length === cur.length && next.every((n, i) =>
      Math.abs(n.left - cur[i].left) < 0.5 && Math.abs(n.top - cur[i].top) < 0.5)
    if (!same) setBandBars(next)
  }, [pages, zoom, bandGeom])

  const [bandSortMenu, setBandSortMenu] = useState<{ top: number; left: number; tableStart: number; col: number } | null>(null)

  // Les helpers de tableau (tableMutateOn → tableCtxOf) agissent sur le tableau qui
  // contient la SÉLECTION. Les pastilles, elles, agissent sur le tableau SURVOLÉ :
  // sans ce recalage la commande ne faisait rien, silencieusement, dès que le caret
  // était ailleurs. On place donc le caret dans le tableau visé avant de muter.
  const focusTableAt = useCallback((tableStart: number): Editor | null => {
    const ed = editorRef.current; if (!ed) return null
    const { doc } = ed.state
    if (tableStart < 0 || tableStart >= doc.content.size) return null
    if (doc.nodeAt(tableStart)?.type.name !== 'table') return null
    const $inside = doc.resolve(Math.min(doc.content.size, tableStart + 1))
    ed.view.dispatch(ed.state.tr.setSelection(TextSelection.near($inside)))
    return ed
  }, [])
  // Attributs du tableau SURVOLÉ (indépendants de la sélection).
  const hoveredTableAttrs = useCallback((tableStart: number): Record<string, unknown> => {
    const ed = editorRef.current; if (!ed) return {}
    const n = tableStart >= 0 && tableStart < ed.state.doc.content.size ? ed.state.doc.nodeAt(tableStart) : null
    return n?.type.name === 'table' ? (n.attrs as Record<string, unknown>) : {}
  }, [])

  // ── Poignées du tableau en cours d'édition (façon Word) ──────────────────────
  // Visibles dès que le caret est DANS un tableau :
  //  · coin haut-gauche = poignée de DÉPLACEMENT (clic = sélectionner tout le
  //    tableau ; glisser = régler le retrait gauche du tableau) ;
  //  · coin bas-droit  = poignée de REDIMENSIONNEMENT (glisser = mettre tout le
  //    tableau à l'échelle — largeurs de colonnes et hauteurs de lignes).
  // Un tableau scindé sur plusieurs pages : déplacement sur le PREMIER fragment,
  // redimensionnement sur le DERNIER (comme Word, qui les pose sur les extrémités).
  interface TableFrame { page: number; x0: number; x1: number; y0: number; y1: number }
  const tableFrames = useCallback((tableStart: number): TableFrame[] => {
    const out: TableFrame[] = []
    const z = zoomRef.current
    pagesRef.current.forEach((pg, idx) => {
      const geom = geomOf(pg)
      const { left, top: pageTop } = pageOrigin(idx)
      for (const para of pg.layout.paragraphs) {
        const tb = para.table
        if (!tb || para.pmStart !== tableStart || !tb.colX || !tb.rowY) continue
        // Fragment répliqué (en-tête épinglé) : pas une extrémité du tableau.
        if (para.lines.some(l => l.phantom)) continue
        const ox = left + geom.marginH * z, oy = pageTop + geom.marginV * z
        const yT = Math.max(tb.rowY[0], 0), yB = Math.min(tb.rowY[tb.rowY.length - 1], geom.contentH)
        out.push({ page: idx, x0: ox + tb.colX[0] * z, x1: ox + tb.colX[tb.colX.length - 1] * z,
                   y0: oy + yT * z, y1: oy + yB * z })
      }
    })
    return out
  }, [])

  // Ref vers le glissé de déplacement : les ops (enregistrées plus haut) doivent
  // pouvoir l'appeler alors qu'il est déclaré plus bas dans le composant.
  const tableMoveDragRef = useRef<((e: React.PointerEvent, tableStart: number) => void) | null>(null)
  const [tableHandles, setTableHandles] = useState<{ tableStart: number; move: { left: number; top: number }; size: { left: number; top: number } } | null>(null)
  const recomputeTableHandles = useCallback(() => {
    const clear = () => setTableHandles(prev => (prev ? null : prev))
    const ed = editorRef.current
    if (!ed || !ed.isEditable || isCoarsePointer()) { clear(); return }
    const sel = tableSelRef.current
    const ctx = tableCtxOf(ed)
    const tableStart = sel ? sel.tableStart : ctx?.tablePos
    if (tableStart == null) { clear(); return }
    const frames = tableFrames(tableStart)
    if (!frames.length) { clear(); return }
    const first = frames[0], last = frames[frames.length - 1]
    const next = { tableStart, move: { left: first.x0 - TBL_HANDLE - 4, top: first.y0 - TBL_HANDLE - 4 },
                   size: { left: last.x1 + 1, top: last.y1 + 1 } }
    setTableHandles(prev => (prev && prev.tableStart === next.tableStart
      && Math.abs(prev.move.left - next.move.left) < 0.5 && Math.abs(prev.move.top - next.move.top) < 0.5
      && Math.abs(prev.size.left - next.size.left) < 0.5 && Math.abs(prev.size.top - next.size.top) < 0.5 ? prev : next))
  }, [tableFrames])
  useEffect(() => { recomputeTableHandles() }, [pages, zoom, tableSel, recomputeTableHandles])

  // Glisser la poignée de déplacement : règle le RETRAIT gauche du tableau (l'aligne
  // à gauche au besoin, sinon le retrait serait ignoré). Un simple clic (sans
  // déplacement) sélectionne tout le tableau.
  const tableMoveDrag = useCallback((e: React.PointerEvent, tableStart: number) => {
    const ed = editorRef.current; if (!ed) return
    e.preventDefault(); e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    const frames = tableFrames(tableStart); if (!frames.length) return
    const z = zoomRef.current
    const attrs = hoveredTableAttrs(tableStart)
    const startIndent = Number(attrs.tableIndent) || 0
    const startAlign = String(attrs.tableAlign || 'left')
    const pg = pagesRef.current[frames[0].page]
    const geom = pg ? geomOf(pg) : null
    const { left: pgLeft } = pageOrigin(frames[0].page)
    // Position visuelle actuelle du bord gauche du tableau, en px de CONTENU : sert
    // de base pour un tableau centré/à droite (dont `tableIndent` ne décrit rien).
    const visualLeft = geom ? (frames[0].x0 - (pgLeft + geom.marginH * z)) / z : 0
    const base = startAlign === 'left' ? startIndent : visualLeft
    // Plafond : on garde au moins TBL_KEEP_VISIBLE px de tableau dans la zone de
    // contenu. Un tableau PLEINE LARGEUR (le défaut) doit pouvoir bouger — plafonner
    // à `contentW - largeurDuTableau` valait 0 et la poignée ne faisait rien. Comme
    // dans Word, le tableau peut alors dépasser la marge de droite.
    const maxIndent = geom ? Math.max(0, geom.contentW - TBL_KEEP_VISIBLE) : 0
    const x0 = e.clientX
    let moved = false
    const onMove = (me: PointerEvent) => {
      const dx = (me.clientX - x0) / z
      if (!moved && Math.abs(dx) < 3) return
      moved = true
      const next = Math.round(Math.max(0, Math.min(maxIndent, base + dx)))
      ed.chain().focus().updateAttributes('table', { tableAlign: 'left', tableIndent: next }).run()
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('pointercancel', onUp)
      // Clic simple = sélectionner TOUT le tableau (façon Word).
      if (!moved) {
        const node = ed.state.doc.nodeAt(tableStart)
        if (node?.type.name === 'table') {
          const rows = node.childCount
          let cols = 1
          node.forEach(row => { let c = 0; row.forEach(cell => { c += Number(cell.attrs.colspan) || 1 }); cols = Math.max(cols, c) })
          focusTableAt(tableStart)
          setTableSel({ tableStart, r0: 0, c0: 0, r1: rows - 1, c1: cols - 1 })
        }
      }
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', onUp)
  }, [tableFrames, hoveredTableAttrs, focusTableAt])
  tableMoveDragRef.current = tableMoveDrag

  // Glisser la poignée de redimensionnement : met TOUT le tableau à l'échelle —
  // largeurs de colonnes (horizontal) et hauteurs de lignes (vertical), façon Word.
  const tableSizeDrag = useCallback((e: React.PointerEvent, tableStart: number) => {
    const ed = editorRef.current; if (!ed) return
    e.preventDefault(); e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    const frames = tableFrames(tableStart); if (!frames.length) return
    const last = frames[frames.length - 1]
    const z = zoomRef.current
    const pg = pagesRef.current[last.page]
    const geom = pg ? geomOf(pg) : null
    const para = pg?.layout.paragraphs.find(p => p.table && p.pmStart === tableStart)
    const tb = para?.table
    if (!tb?.colX || !tb.rowY) return
    const baseW = tb.colX[tb.colX.length - 1] - tb.colX[0]
    const baseH = tb.rowY[tb.rowY.length - 1] - tb.rowY[0]
    const colW0 = tb.colX.slice(1).map((v, i) => v - tb.colX![i])
    const rowH0 = tb.rowY.slice(1).map((v, i) => v - tb.rowY![i])
    const maxW = geom ? geom.contentW - (Number(hoveredTableAttrs(tableStart).tableIndent) || 0) : baseW
    const x0 = e.clientX, y0 = e.clientY
    const MIN_COL = 16, MIN_ROW = 12
    const onMove = (me: PointerEvent) => {
      const dx = (me.clientX - x0) / z, dy = (me.clientY - y0) / z
      const kx = Math.max(MIN_COL * colW0.length / baseW, Math.min(maxW / baseW, (baseW + dx) / baseW))
      const ky = Math.max(MIN_ROW * rowH0.length / baseH, (baseH + dy) / baseH)
      ed.chain().focus().updateAttributes('table', {
        colWidths: colW0.map(w => Math.round(w * kx * 10) / 10),
        // Hauteurs explicites en mode « au moins » : le contenu peut toujours pousser.
        ...(Math.abs(dy) > 2 ? { rowHeights: rowH0.map(h => Math.round(h * ky * 10) / 10) } : {}),
      }).run()
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('pointercancel', onUp)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', onUp)
  }, [tableFrames, hoveredTableAttrs])

  // Glissé d'une bande. Deux repères DISTINCTS, façon Google Docs :
  //  · le FANTÔME teinté = la bande soulevée, qui suit le pointeur en CONTINU (il
  //    conserve l'écart de préhension, il ne saute donc pas de bande en bande) ;
  //  · la BARRE d'insertion = seule chose qui s'aligne, sur la frontière la plus
  //    proche — c'est elle qui indique où la bande atterrira.
  // `ghost` = début du fantôme, `size` sa longueur (px, repère contenu).
  const [bandDrag, setBandDrag] = useState<{ axis: BandAxis; from: number; boundary: number; ghost: number; size: number; origin: number; tableStart: number; page: number } | null>(null)
  bandDragRef.current = bandDrag
  const bandDragStart = useCallback((e: React.PointerEvent, b: BandBar) => {
    if (!editorRef.current) return
    e.preventDefault(); e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    const pg = pagesRef.current[b.page]; if (!pg) return
    const geom = geomOf(pg)
    const z = zoomRef.current
    const para = pg.layout.paragraphs.find(p => p.table && p.pmStart === b.tableStart)
    const bounds = b.axis === 'col' ? para?.table?.colX : para?.table?.rowY
    if (!bounds || bounds.length < 2) return
    // Origine en coordonnées CLIENT (pas celles des overlays) : on compare à
    // clientX/clientY. `pageOrigin()` renvoie des offsets de mise en page.
    const cv = canvasRefs.current.get(b.page); if (!cv) return
    const cr = cv.getBoundingClientRect()
    const o = b.axis === 'col' ? cr.left + geom.marginH * z : cr.top + geom.marginV * z
    // Frontière (0..n) la plus proche d'une coordonnée du repère CONTENU.
    const nearestBoundary = (v: number): number => {
      let best = 0, bestD = Infinity
      for (let i = 0; i < bounds.length; i++) {
        const d = Math.abs(bounds[i] - v)
        if (d < bestD) { bestD = d; best = i }
      }
      return best
    }
    // Le glissé ne doit PAS dépendre de l'état de survol : la pastille peut se
    // refermer en cours de route (minuteur de fermeture armé en quittant la cellule),
    // ce qui faisait disparaître le repère visuel. On fige donc la cible ici.
    keepBand()
    const target = { axis: b.axis, from: b.idx, tableStart: b.tableStart, page: b.page }
    // Coordonnée dans le repère CONTENU sur l'axe du glissé.
    const at = (client: number) => (client - o) / z
    const size = bounds[b.idx + 1] - bounds[b.idx]
    const lo = bounds[0], hi = bounds[bounds.length - 1]
    // Écart de préhension : on saisit la bande là où on a cliqué, le fantôme garde
    // donc sa position relative au pointeur (sinon il « collerait » d'un coup).
    const grab = at(b.axis === 'col' ? e.clientX : e.clientY) - bounds[b.idx]
    // On borne le CENTRE du fantôme aux frontières extrêmes, pas son bord : borné au
    // bord (`hi - size`), son centre plafonnait à mi-chemin de la DERNIÈRE frontière,
    // qui devenait donc inatteignable — impossible de déposer en dernière position.
    // Le fantôme dépasse ainsi le tableau d'une demi-bande à chaque extrémité, et
    // TOUTES les frontières (0..n) sont accessibles.
    const ghostAt = (client: number) =>
      Math.max(lo - size / 2, Math.min(hi - size / 2, at(client) - grab))
    const origin = bounds[b.idx]
    setBandDrag({ ...target, boundary: b.idx, ghost: origin, size, origin })
    const onMove = (me: PointerEvent) => {
      const gh = ghostAt(b.axis === 'col' ? me.clientX : me.clientY)
      const bd = nearestBoundary(gh + size / 2)
      setBandDrag(prev => (prev && prev.boundary === bd && Math.abs(prev.ghost - gh) < 0.25
        ? prev : { ...target, boundary: bd, ghost: gh, size, origin }))
    }
    const onUp = (ue: PointerEvent) => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('pointercancel', onUp)
      const bd = nearestBoundary(ghostAt(b.axis === 'col' ? ue.clientX : ue.clientY) + size / 2)
      setBandDrag(null)
      // Frontière → index d'arrivée après retrait de la bande source.
      const to = bd > b.idx ? bd - 1 : bd
      if (to !== b.idx) {
        const e2 = focusTableAt(b.tableStart)
        if (e2) { if (b.axis === 'col') moveColumn(e2, b.idx, to); else moveRow(e2, b.idx, to) }
        setBandBars([])
      }
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    // Un pointercancel manqué laisserait le repère visuel figé à l'écran.
    window.addEventListener('pointercancel', onUp)
  }, [focusTableAt, keepBand])

  // Sonde de test : état des pastilles et du glissé de bande.
  useEffect(() => {
    const w = window as unknown as Record<string, unknown>
    w.__kbBand = () => ({ bars: bandBarsRef.current, drag: bandDragRef.current })
  }, [])

  // ── Poignée « bordures » de la cellule courante ──────────────────────────────
  // Petit bouton dans le coin haut-droit de la cellule qui contient le caret (ou
  // de la dernière cellule d'une sélection de cellules) : il ouvre la galerie de
  // bordures. Position en repère des overlays (celui du caret), donc elle défile
  // avec le contenu.
  const [cellBorderBtn, setCellBorderBtn] = useState<{ left: number; top: number } | null>(null)
  const recomputeCellBorderBtn = useCallback(() => {
    const clear = () => setCellBorderBtn(prev => (prev ? null : prev))
    const ed = editorRef.current
    if (!ed || !ed.isEditable) { clear(); return }
    // Tactile : la poignée gênerait la frappe et l'édition au doigt (la galerie
    // reste accessible par le menu contextuel et le ruban).
    if (typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches) { clear(); return }
    const sel = tableSelRef.current
    const ctx = tableCtxOf(ed)
    if (!sel && !ctx) { clear(); return }
    const tableStart = sel ? sel.tableStart : ctx!.tablePos
    // Cellule porteuse : coin haut-droit de la PLAGE sélectionnée, sinon la cellule du caret.
    const wantR = sel ? Math.min(sel.r0, sel.r1) : ctx!.rowIndex
    const wantC = sel ? Math.max(sel.c0, sel.c1) : ctx!.colStart
    const z = zoomRef.current
    const pgs = pagesRef.current
    for (let idx = 0; idx < pgs.length; idx++) {
      const pg = pgs[idx]
      const geom = geomOf(pg)
      const { left, top: pageTop } = pageOrigin(idx)
      for (const para of pg.layout.paragraphs) {
        if (!para.table || para.pmStart !== tableStart) continue
        const cell = para.table.cells.find(c => c.r === wantR && c.c <= wantC && c.c + c.colspan - 1 >= wantC)
        if (!cell) continue
        // Cellule scindée par un saut de page : ignorer le fragment hors bande.
        if (cell.y + cell.h < 2 || cell.y > geom.contentH - 2) continue
        const ox = left + geom.marginH * z, oy = pageTop + geom.marginV * z
        const p = { left: ox + (cell.x + cell.w) * z - CELL_BTN - 3, top: oy + cell.y * z + 3 }
        setCellBorderBtn(prev => (prev && Math.abs(prev.left - p.left) < 0.5 && Math.abs(prev.top - p.top) < 0.5 ? prev : p))
        return
      }
    }
    clear()
  }, [])
  useEffect(() => { recomputeCellBorderBtn() }, [pages, zoom, tableSel, recomputeCellBorderBtn])

  // Galerie de bordures ouverte depuis la poignée : MenuDropdown de @ui avec un
  // item `custom` (grille d'icônes 3×3 + « Aucune bordure »), jamais un div
  // flottant maison.
  const [borderGalleryPos, setBorderGalleryPos] = useState<{ top: number; left: number } | null>(null)
  const openBorderGallery = useCallback((r: DOMRect) => {
    setBorderGalleryPos({ top: r.bottom + 4, left: r.left })
  }, [])
  const applyBorders = useCallback((preset: BorderPreset) => {
    const ed = editorRef.current; if (!ed) return
    const ta = ed.getAttributes('table') as Record<string, unknown>
    // Pinceau = réglages de bordure du ruban (crayon de Word) ; noir ½ pt par défaut.
    const pen = {
      w: Number(ta.tableBorderWidth) || 1,
      s: ((ta.tableBorderStyle as 'solid' | 'dashed' | 'dotted') || 'solid'),
      c: (ta.tableBorderColor as string) || '#000000',
    }
    const sel = tableSelRef.current
    const ctx = tableCtxOf(ed)
    const rect: TableRect = sel
      ? { r0: Math.min(sel.r0, sel.r1), c0: Math.min(sel.c0, sel.c1), r1: Math.max(sel.r0, sel.r1), c1: Math.max(sel.c0, sel.c1) }
      : ctx ? { r0: ctx.rowIndex, c0: ctx.colStart, r1: ctx.rowIndex, c1: ctx.colStart }
      : { r0: 0, c0: 0, r1: 0, c1: 0 }
    applyBorderPreset(ed, rect, preset, pen)
  }, [])

  const [caretHandle, setCaretHandle] = useState<{ left: number; top: number; height: number } | null>(null)
  const caretHandleUntilRef = useRef(0)
  const caretHandleTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const recomputeCaretHandle = useCallback(() => {
    const ed = editorRef.current
    const coarse = typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches
    const clear = () => setCaretHandle(prev => (prev ? null : prev))
    if (!ed || !coarse || !ed.isEditable) { clear(); return }
    const sel = ed.state.selection
    if (!(sel instanceof TextSelection) || !sel.empty || Date.now() > caretHandleUntilRef.current) { clear(); return }
    const p = screenPosForHead(sel.head)
    if (!p) { clear(); return }
    setCaretHandle(prev => (prev && prev.left === p.left && prev.top === p.top && prev.height === p.height) ? prev : p)
  }, [screenPosForHead])
  const armCaretHandle = useCallback(() => {
    caretHandleUntilRef.current = Date.now() + 4000
    recomputeCaretHandle()
    clearTimeout(caretHandleTimerRef.current)
    caretHandleTimerRef.current = setTimeout(recomputeCaretHandle, 4100)
  }, [recomputeCaretHandle])
  useEffect(() => () => clearTimeout(caretHandleTimerRef.current), [])
  useEffect(() => { recomputeCaretHandle() }, [pages, zoom, recomputeCaretHandle])
  // Glissé de la goutte = déplacement précis du CARET (avec loupe) ; tap bref =
  // bascule du menu d'insertion (comportement de la goutte native Android).
  const caretHandleDrag = useCallback((e: React.PointerEvent) => {
    const ed = editorRef.current; if (!ed) return
    e.preventDefault(); e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    const t0 = Date.now(), sx0 = e.clientX, sy0 = e.clientY
    let moved = false
    const FINGER_DY = 22
    const move = (me: PointerEvent) => {
      if (!moved && (Math.abs(me.clientX - sx0) > 8 || Math.abs(me.clientY - sy0) > 8)) moved = true
      if (!moved) return
      caretHandleUntilRef.current = Date.now() + 4000
      const cy = me.clientY - FINGER_DY
      const idx = pageAtPoint(me.clientX, cy, 0)
      const p = posFromEvent(idx, me.clientX, cy)
      if (p != null && ed.state.selection.head !== p) ed.commands.setTextSelection(p)
      showLoupe(me.clientX, cy)
    }
    const up = () => {
      hideLoupe()
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', up)
      if (!moved && Date.now() - t0 < 300) {
        // Tap sur la goutte → TOGGLE du menu d'insertion (AOSP InsertionHandleView).
        const caretBarVisible = miniBarCaretRef.current && miniBarMouseRef.current
          && !miniBarDismissedRef.current && !miniBarSuppressedRef.current
        if (caretBarVisible) { miniBarDismissedRef.current = true; recomputeBodyMiniBar() }
        else armCaretBar()
      }
      armCaretHandle()
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', up)
    // deps [] : les helpers référencés (pageAtPoint, posFromEvent, showLoupe,
    // armCaretBar…) sont déclarés PLUS BAS dans le composant — les citer dans les
    // deps les évaluerait au rendu (TDZ) ; le corps ne s'exécute qu'à l'événement,
    // après initialisation, et ils sont tous stables.
  }, [armCaretHandle]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Ligne de bordure de tableau SÉLECTIONNÉE (mobile) : un tap SUR une
  // bordure (verticale ou horizontale) la sélectionne — elle est surlignée et
  // porte la pastille de redimensionnement. Un tap ailleurs désélectionne.
  // `page`/`along` : fragment touché + coordonnée LE LONG de la ligne (px doc,
  // repère contenu de cette page) — le cercle apparaît SOUS le doigt, projeté
  // sur la ligne, et y reste (re-tap = il se replace sous le doigt).
  const [touchTblLine, setTouchTblLine] = useState<{ pos: number; kind: 'col' | 'colEdge' | 'row' | 'rowEdge'; index: number; page: number; along: number } | null>(null)
  // Bordure de tableau la plus proche du point (coords viewport), tolérance
  // ±14px écran, bornée au fragment visible. Renvoie null si aucune.
  const tableBorderAt = useCallback((clientX: number, clientY: number) => {
    const z = zoomRef.current
    // Tolérance de préhension SERRÉE : ~7px écran (bande de 14px) au lieu de 14
    // — une bande trop large faisait sélectionner une bordure au lieu de poser le
    // caret, surtout sur des cellules courtes (leurs bords haut/bas couvraient
    // presque toute la hauteur). En plus, par bordure, la tolérance est PLAFONNÉE
    // à ~40 % de la ½-dimension de la cellule adjacente → jamais consommer
    // l'intérieur d'une petite cellule (le caret gagne au centre).
    const TOL = 7 / z   // px document (≈ 7px écran)
    const h: { best: { pos: number; kind: 'col' | 'colEdge' | 'row' | 'rowEdge'; index: number; d: number; page: number; along: number } | null } = { best: null }
    canvasRefs.current.forEach((cv, idx) => {
      const rect = cv.getBoundingClientRect()
      if (clientX < rect.left - 20 || clientX > rect.right + 20 || clientY < rect.top || clientY > rect.bottom) return
      const pg = pagesRef.current[idx]; if (!pg) return
      const geom = geomOf(pg)
      const lx = (clientX - rect.left) / z - geom.marginH
      const ly = (clientY - rect.top) / z - geom.marginV
      for (const para of pg.layout.paragraphs) {
        const tb = para.table
        if (!tb || !tb.colX || !tb.rowY || tb.colX.length < 2 || tb.rowY.length < 2) continue
        const yTop = Math.max(tb.rowY[0], 0), yBot = Math.min(tb.rowY[tb.rowY.length - 1], geom.contentH)
        const x0 = tb.colX[0], x1 = tb.colX[tb.colX.length - 1]
        // Bordures verticales (colonnes) — tolérance plafonnée à 40 % de la
        // demi-largeur de la colonne la plus étroite adjacente.
        if (ly >= yTop && ly <= yBot) {
          tb.colX.forEach((cx, ci) => {
            if (ci === 0) return
            const halfMin = Math.min(cx - tb.colX![ci - 1], (tb.colX![ci + 1] ?? cx + 1e4) - cx) / 2
            const tol = Math.min(TOL, halfMin * 0.4)
            const d = Math.abs(lx - cx)
            if (d <= tol && (!h.best || d < h.best.d))
              h.best = { pos: para.pmStart, kind: ci === tb.colX!.length - 1 ? 'colEdge' : 'col', index: ci, d, page: idx, along: ly }
          })
        }
        // Bordures horizontales (lignes) — tolérance plafonnée à 40 % de la
        // demi-hauteur de la ligne la plus courte adjacente.
        if (lx >= x0 && lx <= x1) {
          tb.rowY.forEach((cy, ri) => {
            if (ri === 0 || cy < -1 || cy > geom.contentH + 1) return
            const halfMin = Math.min(cy - tb.rowY![ri - 1], (tb.rowY![ri + 1] ?? cy + 1e4) - cy) / 2
            const tol = Math.min(TOL, halfMin * 0.4)
            const d = Math.abs(ly - cy)
            if (d <= tol && (!h.best || d < h.best.d))
              h.best = { pos: para.pmStart, kind: ri === tb.rowY!.length - 1 ? 'rowEdge' : 'row', index: ri, d, page: idx, along: lx }
          })
        }
      }
    })
    return h.best ? { pos: h.best.pos, kind: h.best.kind, index: h.best.index, page: h.best.page, along: h.best.along } : null
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // Position ÉCRAN (viewport) du haut de la sélection du corps → mini-barre partagée
  // `FormattingMiniBar` (même composant que les zones de texte / en-têtes-pieds).
  // Masquée si pas de sélection de texte, ou en édition en-tête/pied / zone de texte
  // (qui ont leur propre mini-barre via RichEditZone).
  const recomputeBodyMiniBar = useCallback(() => {
    const ed = editorRef.current
    if (!ed || hfEditRef.current || tbEditRef.current) { setBodyMiniBar(null); return }
    const sel = ed.state.selection
    if (!(sel instanceof TextSelection)) { setBodyMiniBar(null); return }
    // Sélection vide autorisée UNIQUEMENT en mode menu-caret (« tap again »).
    const caretMode = miniBarCaretRef.current && sel.empty
    if (sel.from >= sel.to && !caretMode) { setBodyMiniBar(null); return }
    // N'affiche QUE pour une sélection à la souris/tactile, pas si fondue
    // (dismiss) ni pendant un masquage temporaire (scroll / glissé de poignée).
    if (!miniBarMouseRef.current || miniBarDismissedRef.current || miniBarSuppressedRef.current) { setBodyMiniBar(null); return }
    const pgs = pagesRef.current
    // Ancrée sur la TÊTE de sélection (l'extrémité que la souris vient de relâcher) →
    // la souris démarre PRÈS de la barre quelle que soit la longueur de la sélection.
    const anchor = sel.head
    const idx = pageIndexForHead(pgs, anchor)
    const pg = pgs[idx]; const cv = canvasRefs.current.get(idx)
    if (!pg || !cv) { setBodyMiniBar(null); return }
    const z = zoomRef.current, geom = geomOf(pg)
    const cm = posToCoords(pg.layout, anchor)
    const r = cv.getBoundingClientRect()
    setBodyMiniBar({ left: r.left + (geom.marginH + cm.x) * z, top: r.top + (geom.marginV + cm.y) * z, caret: caretMode })
  }, [])

  // Arme la mini-barre après une sélection À LA SOURIS (mouseup de glissé, ou
  // double/triple-clic) : réinitialise le verrou de fondu et l'opacité, puis affiche.
  const armBodyMiniBar = useCallback(() => {
    const ed = editorRef.current; if (!ed) return
    const sel = ed.state.selection
    if (sel instanceof TextSelection && sel.from < sel.to) {
      miniBarCaretRef.current = false
      miniBarMouseRef.current = true
      miniBarDismissedRef.current = false
      miniBarSuppressedRef.current = false
      recomputeBodyMiniBar()
      // Remet l'opacité à plein (l'élément peut persister d'un fondu précédent).
      const reset = () => { const el = barElRef.current; if (el) { el.style.opacity = '1'; el.style.pointerEvents = 'auto' } }
      reset(); requestAnimationFrame(reset)
    } else {
      miniBarMouseRef.current = false
      recomputeBodyMiniBar()
    }
  }, [recomputeBodyMiniBar])

  // Menu d'insertion au CARET (« tap again » sur le caret déjà posé — pattern
  // Word mobile / iOS ; Android l'offre via la goutte d'insertion) : Coller ·
  // Tout sélectionner, positionné sur le caret.
  const armCaretBar = useCallback(() => {
    const ed = editorRef.current; if (!ed) return
    const sel = ed.state.selection
    if (!(sel instanceof TextSelection) || !sel.empty) return
    miniBarCaretRef.current = true
    miniBarMouseRef.current = true
    miniBarDismissedRef.current = false
    miniBarSuppressedRef.current = false
    recomputeBodyMiniBar()
    const reset = () => { const el = barElRef.current; if (el) { el.style.opacity = '1'; el.style.pointerEvents = 'auto' } }
    reset(); requestAnimationFrame(reset)
  }, [recomputeBodyMiniBar])

  // Fondu par PROXIMITÉ de la souris (impératif → pas de re-render de la barre par
  // mousemove). Sous NEAR = plein ; au-delà de FAR = 0 → dismiss (revient à la
  // prochaine sélection seulement). N'est PILOTÉ que par mousemove : le scroll
  // repositionne la barre sans toucher l'opacité (donc ne la fait pas disparaître).
  useEffect(() => {
    const NEAR = 20, FAR = 65
    const onMove = (e: MouseEvent) => {
      const el = barElRef.current
      if (!el || miniBarDismissedRef.current) return
      // Le défilement de page émet des mousemove SYNTHÉTIQUES (le pointeur ne bouge
      // pas physiquement, le contenu glisse dessous) aux MÊMES coords écran. Comme la
      // barre suit le texte, ils feraient bondir la distance → dismiss au scroll. On
      // les ignore : seul un vrai déplacement du pointeur modifie l'opacité.
      const m = miniBarMoveRef.current
      if (Math.abs(e.clientX - m.x) < 1 && Math.abs(e.clientY - m.y) < 1) return
      m.x = e.clientX; m.y = e.clientY
      const b = el.getBoundingClientRect()
      const dx = Math.max(b.left - e.clientX, 0, e.clientX - b.right)
      const dy = Math.max(b.top - e.clientY, 0, e.clientY - b.bottom)
      const dist = Math.hypot(dx, dy)
      const op = dist <= NEAR ? 1 : dist >= FAR ? 0 : 1 - (dist - NEAR) / (FAR - NEAR)
      if (op <= 0.02) { miniBarDismissedRef.current = true; setBodyMiniBar(null) }
      else { el.style.opacity = String(op); el.style.pointerEvents = op > 0.5 ? 'auto' : 'none' }
    }
    document.addEventListener('mousemove', onMove)
    return () => document.removeEventListener('mousemove', onMove)
  }, [])

  // Toute frappe clavier DÉSARME la mini-barre (elle n'apparaît que sur sélection
  // souris) : la sélection clavier suivante ne la fera pas surgir, et taper la masque.
  useEffect(() => {
    const onKey = () => { if (miniBarMouseRef.current) { miniBarMouseRef.current = false; setBodyMiniBar(null) } }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
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
    // Forme (`kbshape:`) : on remonte sa géométrie et ses valeurs d'ajustement à
    // l'overlay, qui en dérive les poignées jaunes.
    const selShape = parseShapeAlt(sel.node.attrs.alt as string)
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
        moveWithText: a.moveWithText !== false,
        kind: selShape?.kind, adj: selShape?.adj,
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
    // Le centre de la boîte de sélection DOIT coïncider avec le centre de tracé du
    // canvas (cf. paintLayout) sinon, dès qu'on tourne la forme, boîte et forme se
    // désynchronisent. Canvas : flottant → centré sur line.y + wrapY + image.h/2 ;
    // sinon (aligné) → centré sur la ligne, line.y + line.height/2 (= aabbH).
    const wrapName = imgLine.image.wrap || 'inline'
    const isFloat = isFloatingWrap(wrapName)
    // Page d'affichage : celle qui contient le HAUT de l'OBJET (un flottant poussé
    // sous le bas de page est re-logé sur la page suivante), pas la ligne d'ancre.
    const gRefY = imgLine.y + (isFloat ? (imgLine.image.wrapY || 0) : 0)
    let idx = 0
    for (let k = 0; k < pgs.length; k++) { if (gRefY >= pgs[k].startY - 0.5) idx = k }
    const geom = geomOf(pgs[idx])
    const { left: leftOffset, top: pageTop } = pageOrigin(idx)
    const localY = imgLine.y - (pgs[idx]?.startY ?? 0)
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
      moveWithText: sel.node.attrs.moveWithText !== false,
      kind: selShape?.kind, adj: selShape?.adj,
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
    // `yUndoOptions.trackedOrigins` : les changements de réglages de page (Y.Map
    // `pageSetup`, écrits avec PAGE_SETUP_ORIGIN) entrent dans l'historique Yjs → Ctrl+Z
    // les annule dans le MÊME fil que le texte (cf. addToScope de la map plus bas).
    extensions: [...PAGE_EXTENSIONS, Collaboration.configure({ document: ydoc, yUndoOptions: { trackedOrigins: [PAGE_SETUP_ORIGIN] } })],
    editorProps: {
      attributes: { class: 'focus:outline-none' },
      handleScrollToSelection: () => true,
      // Cross-module data paste: a `data-kubuno` JSON envelope on the clipboard
      // is rendered BY ITS PRODUCER module (renderStatic of its `core.data-card`
      // entry) and inserted as an image block that keeps the original JSON in
      // `alt` (`kbenvelope:…`). Regular pastes fall through to ProseMirror.
      handlePaste: (view, event) => {
        const env = readKubunoData(event.clipboardData)
        if (!env) return false
        void (async () => {
          const entry = resolveDataCardEntry(env.type)
          const alt = 'kbenvelope:' + encodeURIComponent(JSON.stringify(env))
          const r = entry?.renderStatic ? await entry.renderStatic(env).catch(() => null) : null
          const src = r ? (r.dataUrl ?? (r.svg ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(r.svg)}` : null)) : null
          if (src && r) {
            const MAX_W = 480
            const scale = Math.min(1, MAX_W / Math.max(1, r.width))
            const node = view.state.schema.nodes.image.createAndFill({
              src, alt,
              width: Math.max(1, Math.round(r.width * scale)),
              height: Math.max(1, Math.round(r.height * scale)),
            })
            if (node) { view.dispatch(view.state.tr.replaceSelectionWith(node).scrollIntoView()); return }
          }
          // Producer not installed (or no static render): keep the readable summary.
          const text = env.text ?? env.title ?? JSON.stringify(env)
          view.dispatch(view.state.tr.insertText(text).scrollIntoView())
        })()
        return true
      },
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
        // Viser le CENTRE de la ligne visuelle adjacente — et non « bord de ligne ± 2px » :
        // dans les espacements de paragraphe, la ligne d'origine restait la plus proche
        // pour coordsToPos → le caret refusait de bouger.
        const targetY = adjacentLineCenter(layout, cm.lineTop ?? cm.y, isUp ? -1 : 1)
        if (targetY == null) { event.preventDefault(); return true }   // 1ʳᵉ/dernière ligne
        const newPos  = coordsToPos(layout, goalXRef.current, targetY)
        // Affinité d'ARRIVÉE : à une frontière d'enroulement, la même position PM vit sur
        // DEUX lignes (fin de la ligne N = début de la N+1) — après « Fin », goalX est à
        // droite et coordsToPos renvoie systématiquement ces frontières. Toujours retomber
        // sur l'affinité « début » affichait le caret une ligne PLUS BAS que la ligne visée
        // (lignes sautées en descendant, blocage en montant). On choisit l'affinité dont la
        // ligne est celle visée par targetY.
        const cmEnd = posToCoords(layout, newPos, true)
        const cmStart = posToCoords(layout, newPos, false)
        const tEnd = cmEnd.lineTop ?? cmEnd.y, tStart = cmStart.lineTop ?? cmStart.y
        const wantEnd = tEnd !== tStart &&
          Math.abs(tEnd + (cmEnd.lineH ?? cmEnd.height) / 2 - targetY) <
          Math.abs(tStart + (cmStart.lineH ?? cmStart.height) / 2 - targetY)
        const affinityChanged = caretAtEndRef.current !== wantEnd
        caretAtEndRef.current = wantEnd
        // Même position PM + même affinité → réellement sur la 1ʳᵉ/dernière ligne. Si seule
        // l'AFFINITÉ change, on dispatch quand même (le caret change de ligne visuelle).
        if (newPos === head && !affinityChanged) { event.preventDefault(); return true }
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
      recomputeSelHandles()
      // Une image/zone-de-texte sélectionnée a pu changer de dimensions (ex. zone de
      // texte qui auto-grandit) → recaler la barre/le cadre d'édition sur le rect.
      updateImgSel()
      recomputeBodyMiniBar()
      reportStats()
      // Une édition de document (frappe, mais aussi ANNULATION/RÉTABLISSEMENT ou collab
      // qui modifient les marges/orientation d'une section SANS déplacer le curseur) doit
      // resynchroniser la règle et les dialogues sur la section active. onSelectionUpdate
      // ne se déclenche pas dans ce cas → on le fait ici aussi (bon marché).
      reportActiveSection()
      // Frappe/édition → la goutte d'insertion disparaît (comportement Android).
      caretHandleUntilRef.current = 0
      recomputeCaretHandle()
      // Correcteur : recalcule les fautes (débit léger) puis redessine les soulignés.
      clearTimeout(spellTimer.current)
      spellTimer.current = setTimeout(() => { computeSpell(); renderAllPages() }, 350)
      clearTimeout(saveTimer.current)
      saveTimer.current = setTimeout(() => onSave((ed as Editor).getJSON()), 1200)
    },
    onSelectionUpdate: ({ editor: ed }) => {
      // See onUpdate: skip the construction-time fire (handlers below not yet initialised).
      if (!editorRef.current) return
      // Titre contenant le caret → triangle de repli visible (façon Word).
      {
        const $f = ed.state.selection.$from
        let head: number | null = null
        for (let d = $f.depth; d >= 0; d--) if (outlineLevelOf($f.node(d)) > 0) { head = $f.before(d); break }
        if (head !== caretHeadingRef.current) {
          caretHeadingRef.current = head
          requestAnimationFrame(() => renderAllPages())
        }
      }
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
      // Sélection non vide qui CHANGE → animer le glissement de la surbrillance
      // (le bord glisse vers sa nouvelle position au lieu de bondir) ; repliée → purger.
      const selA = (ed as Editor).state.selection
      if (selA.from < selA.to) { selAnimT0Ref.current = performance.now(); kickSelAnim() }
      else if (shownSelRectsRef.current.size) shownSelRectsRef.current.clear()
      // Tout changement de sélection referme le menu-caret (« tap again ») : il
      // ne se ré-affiche que par un tap explicite sur le caret.
      miniBarCaretRef.current = false
      renderAllPages(); drawCaret(true); reportActiveSection(); updateImgSel(); recomputeBodyMiniBar(); recomputeSelHandles(); recomputeCaretHandle(); recomputeCellBorderBtn(); recomputeTableHandles(); reportStats()
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

  // Réglages de page annulables : ajoute la Y.Map `pageSetup` à la PORTÉE de
  // l'UndoManager Yjs (créé par le plugin d'annulation de la Collaboration) → ses
  // changements (marges de base) entrent dans la même pile undo/redo que le texte.
  useEffect(() => {
    const ed = editor as Editor | null; if (!ed) return
    try {
      const um = (yUndoPluginKey.getState(ed.state) as { undoManager?: { addToScope: (t: unknown) => void } } | undefined)?.undoManager
      um?.addToScope(ydoc.getMap('pageSetup'))
    } catch { /* pas d'UndoManager (éditeur non collaboratif) : ignorer */ }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, ydoc])

  // Base d'annulation : le CHARGEMENT du document (seed du contenu initial ou 1ʳᵉ
  // synchro Yjs) ne doit pas être annulable — sinon le 1er Ctrl+Z « vide le document »
  // (et, sur un gros document, l'inversion de l'insertion massive gèle le thread). On
  // purge donc la pile d'annulation une fois le contenu établi (seed/sync résolus).
  const baselineClearedRef = useRef(false)
  useEffect(() => {
    const ed = editor as Editor | null
    if (!ed || collabEmpty === null || baselineClearedRef.current) return
    const id = setTimeout(() => {
      baselineClearedRef.current = true   // UNE SEULE fois : ne jamais purger d'éditions ultérieures
      try {
        const um = (yUndoPluginKey.getState(ed.state) as { undoManager?: { clear: () => void } } | undefined)?.undoManager
        um?.clear()
      } catch { /* ignorer */ }
    }, 400)
    return () => clearTimeout(id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, collabEmpty])

  // Seed : si la salle Yjs est VIDE (1er ouvreur), initialiser le doc partagé
  // depuis le contenu JSON existant (.kbdoc). Une seule fois ; les ouvreurs
  // suivants reçoivent le contenu via Yjs (collabEmpty === false → pas de seed).
  const seededRef = useRef(false)
  useEffect(() => {
    if (seededRef.current || collabEmpty !== true || !editor) return
    seededRef.current = true
    editor.commands.setContent(initialDoc)
  }, [collabEmpty, editor]) // eslint-disable-line react-hooks/exhaustive-deps
  // ── Sélection valide après la synchro Yjs ────────────────────────────────────
  // L'éditeur est monté VIDE (le contenu vient du Y.Doc) : sa sélection est alors en
  // position 1, dans l'unique paragraphe vide. Quand la synchro remplace le document,
  // y-prosemirror restaure la sélection à la même position — or si le document
  // commence par un TABLEAU, la position 1 tombe DANS le nœud table, qui n'a pas de
  // contenu inline. D'où l'avertissement de ProseMirror « TextSelection endpoint not
  // pointing into a node with inline content (table) » et, plus gênant, une première
  // frappe qui crée un paragraphe AU-DESSUS du tableau au lieu d'entrer dans la
  // première cellule. On recale donc la sélection sur la 1ʳᵉ position de texte valide.
  const normalizeSelection = useCallback((ed: Editor) => {
    const sel = ed.state.selection
    if (sel.$from.parent.inlineContent && sel.$to.parent.inlineContent) return
    const pos = Math.max(0, Math.min(sel.from, ed.state.doc.content.size))
    const near = TextSelection.near(ed.state.doc.resolve(pos), 1)
    if (near) ed.view.dispatch(ed.state.tr.setSelection(near))
  }, [])
  useEffect(() => { if (editor) normalizeSelection(editor as Editor) }, [editor, pages, normalizeSelection])
  useEffect(() => { if (editor) recompute(editor as Editor, true) }, [editor, recompute])
  // Géométrie (marges / zoom / taille de page) : relayout COALESCÉ — un glissé de
  // marge change contentW à chaque frame ; on ne relayoute qu'une fois par frame.
  useEffect(() => { if (editor) scheduleRecompute() }, [zoom, g.contentW, g.contentH]) // eslint-disable-line react-hooks/exhaustive-deps
  // Le rectangle de sélection d'objet est positionné en px × zoom (cf. updateImgSel) :
  // au changement de zoom il faut le recalculer APRÈS le relayout des pages (offsets
  // canvas à jour) — sinon la boîte reste à l'échelle précédente et « décroche ».
  useEffect(() => { const r = requestAnimationFrame(() => updateImgSel()); return () => cancelAnimationFrame(r) }, [zoom, pages]) // eslint-disable-line react-hooks/exhaustive-deps
  // Barre de statut : reporter le total de pages au montage / re-pagination.
  useEffect(() => { reportStats() }, [pages]) // eslint-disable-line react-hooks/exhaustive-deps
  // Numérotation des titres : option hors modèle PM → re-layout forcé + repaint.
  useEffect(() => {
    const ed = editorRef.current; if (!ed) return
    recompute(ed, true); renderAllPages()
  }, [headingNumbers]) // eslint-disable-line react-hooks/exhaustive-deps

  // Présence : recalcule les curseurs distants quand l'awareness change (un pair
  // bouge/rejoint/part) ou quand la mise en page change (pagination, zoom).
  useEffect(() => {
    awareness.on('change', recomputeRemoteCursors)
    return () => awareness.off('change', recomputeRemoteCursors)
  }, [awareness, recomputeRemoteCursors])
  useEffect(() => { recomputeRemoteCursors() }, [pages, zoom, recomputeRemoteCursors])
  useEffect(() => { recomputeSelHandles() }, [pages, zoom, recomputeSelHandles])
  // La mini-barre contextuelle suit aussi le zoom (zoom de confort, pincement) —
  // sinon elle reste figée à sa position d'avant-zoom.
  useEffect(() => { recomputeBodyMiniBar() }, [pages, zoom, recomputeBodyMiniBar])

  // Redessine TOUS les canvas avec la sélection courante (appelé sur changement
  // de pages/zoom ET sur changement de sélection → le surlignage s'affiche).
  // ── Correcteur orthographe/grammaire ────────────────────────────────────────
  const spellRef = useRef<SpellIssue[]>([])
  const spellCheckRef = useRef(spellCheck); spellCheckRef.current = spellCheck
  const grammarCheckRef = useRef(grammarCheck); grammarCheckRef.current = grammarCheck
  const grammarRulesRef = useRef(grammarRules); grammarRulesRef.current = grammarRules
  const computeSpell = useCallback(() => {
    const ed = editorRef.current
    const spelling = spellCheckRef.current, grammar = grammarCheckRef.current
    if (!ed || (!spelling && !grammar)) { if (spellRef.current.length) { spellRef.current = []; onSpellCount?.(0) } return }
    const issues: SpellIssue[] = []
    const rules = grammarRulesRef.current
    // Plafond de fautes. Un document en majorité dans une langue NON reconnue (ex. faux-texte
    // latin « Lorem ipsum », ou langue sans dictionnaire chargé) signale QUASI CHAQUE MOT :
    // des dizaines de milliers de plages → le recalcul (nspell par mot) ET surtout leur rendu
    // (un souligné ondulé par mot, sur chaque page) figeaient le thread. On arrête le balayage
    // au plafond : au-delà, souligner « tout » n'apporte rien à l'utilisateur (comme Word, qui
    // cesse de vérifier au-delà d'un seuil). Les documents normaux n'atteignent jamais ce seuil.
    const MAX_SPELL_ISSUES = 1500
    ed.state.doc.descendants((node, pos, _parent, index) => {
      if (issues.length >= MAX_SPELL_ISSUES) return false
      if (node.isText && node.text) {
        // Langue de la PLAGE (marque `spellLang` posée par « Texte sélectionné ») → contrôle
        // ce passage dans SA langue, sinon la langue active du document. `blockStart` =
        // 1er enfant inline de son bloc → vrai début de phrase (règle majuscule).
        const lm = node.marks.find(m => m.type.name === 'spellLang')
        issues.push(...findIssues(node.text, pos, { spelling, grammar, rules, lang: (lm?.attrs.lang as string | null) ?? null, blockStart: index === 0 }))
      }
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
    // Repositionne la marge de commentaires (l'ancre a pu bouger après reflow).
    bumpComments()
  }, [bumpComments])

  // Entrées de peinture partagées par toutes les pages d'une passe (sélection,
  // correcteur, surbrillances, focus, zoom). Recalculées une fois par passe.
  const paintInputs = useCallback(() => {
    const ed = editorRef.current
    if (!ed) return null
    // Résolution NATIVE (devicePixelRatio réel) — PAS de supersampling forcé. Rendre à
    // 2× puis laisser le navigateur réduire DILUAIT l'encre : chaque bord de glyphe
    // devenait une moyenne avec le blanc → texte « gris » (luminance d'encre mesurée
    // 123/255 contre le noir dense de Word/du texte DOM). À l'échelle 1:1, Skia
    // rasterise le texte avec hinting complet, comme le DOM → noir net, façon Word.
    // Demi-résolution le temps d'un commit de pincement (cf. pinchPaint).
    const dpr = (window.devicePixelRatio || 1) * (pinchPaint.coarse ? 0.5 : 1)
    const sel = ed.state.selection
    // Une image sélectionnée (NodeSelection) est signalée par ses POIGNÉES, pas
    // par le voile bleu de la sélection texte — sinon elle « paraît sélectionnée
    // comme du texte » (façon Word / Google Docs). On n'exclut QUE le nœud image.
    const isImgNode = sel instanceof NodeSelection && (sel.node.type.name === 'image' || sel.node.type.name === 'inlineImage')
    const range = sel.from < sel.to && !isImgNode ? { from: sel.from, to: sel.to, head: sel.head } : undefined
    const spell = spellCheckRef.current && spellRef.current.length
      ? spellRef.current.map(i => ({ from: i.from, to: i.to, grammar: i.type === 'grammar' })) : undefined
    // Surbrillances : commentaires (jaune doux, plus fort si actif) + occurrences de
    // recherche (jaune, orange pour l'occurrence courante).
    // Commentaires : les fils INACTIFS partagent UNE couleur jaune TRANSPARENTE →
    // peints en un seul chemin par le moteur → zone uniforme, AUCUNE ligne de
    // démarcation entre fils voisins/chevauchants. Le fil ACTIF est superposé EN
    // DERNIER dans un jaune OPAQUE (couvre proprement ce qu'il chevauche, pas de
    // couture translucide) → texte commenté actif nettement mis en avant.
    const highlights: Array<{ from: number; to: number; color: string }> = []
    let activeHl: { from: number; to: number; color: string } | null = null
    for (const c of commentRangesRef.current) {
      if (c.id === activeCommentRef.current) activeHl = { from: c.from, to: c.to, color: 'rgb(255,213,79)' }
      else highlights.push({ from: c.from, to: c.to, color: 'rgba(255,213,79,0.30)' })
    }
    if (activeHl) highlights.push(activeHl)
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
    return { dpr, range, spell, highlights: highlights.length ? highlights : undefined, focused, z: zoomRef.current }
  }, [])
  type PaintInputs = NonNullable<ReturnType<typeof paintInputs>>

  // ── Animation de la sélection (façon plage du tableur) ───────────────────────
  // SEULE la progression HORIZONTALE sur la ligne de la TÊTE de sélection (celle du
  // pointeur pendant un glissé) est animée : son bord glisse (~70 ms, ease-out) au
  // lieu de bondir de caractère en caractère. Tout le reste — changement de ligne,
  // autres lignes du bloc, rétrécissements verticaux — s'applique INSTANTANÉMENT.
  const shownSelRectsRef = useRef(new Map<number, SelectionRect[]>())
  const selAnimT0Ref = useRef(0)
  const renderAllPagesNowRef = useRef<(() => void) | null>(null)

  // ── Reactive devicePixelRatio ────────────────────────────────────────────────
  // The canvases' backing stores AND their CSS sizes depend on dpr. Without this,
  // changing the browser zoom (Ctrl +/−) or moving the window to a monitor with a
  // different scale left every untouched page painted at the OLD dpr — a bitmap
  // stretched by the compositor (e.g. 794px shown over 992 device px at 125%),
  // i.e. massive blur until the next repaint. matchMedia('resolution') is the
  // standard dpr-change signal; the listener re-arms itself after each change.
  const [dpr, setDpr] = useState(() => (typeof window !== 'undefined' ? window.devicePixelRatio : 1) || 1)
  useEffect(() => {
    // Re-armed via deps [dpr] on every committed change. devicePixelRatio is read
    // in a rAF, NOT in the change handler: the event can fire while the property
    // still reports the OLD value, which would re-arm on a stale resolution and
    // miss every later change.
    const mql = window.matchMedia(`(resolution: ${dpr}dppx)`)
    let raf = 0
    const onChange = () => {
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(() => {
        if ((window.devicePixelRatio || 1) === dpr) return   // resize sans changement de dpr
        setDpr(window.devicePixelRatio || 1)   // → re-render (tailles CSS) + ré-armement
        renderAllPagesNowRef.current?.()        // → repaint des bitmaps au nouveau dpr
      })
    }
    mql.addEventListener('change', onChange)
    // Ceinture : le zoom navigateur déclenche toujours un resize — couvre un
    // éventuel événement matchMedia manqué (observé sous émulation CDP).
    window.addEventListener('resize', onChange)
    return () => {
      cancelAnimationFrame(raf)
      mql.removeEventListener('change', onChange)
      window.removeEventListener('resize', onChange)
    }
  }, [dpr])

  // NOTE — pas de « calage sous-pixel » du bloc de pages ici. Une tentative
  // (nudge left/top du conteneur pour aligner l'origine sur un pixel machine)
  // a été retirée : la position naturelle tombait pile sur une demi-unité, donc
  // l'arrondi basculait d'une passe à l'autre et la page oscillait d'un pixel
  // machine en boucle (tremblement visible ~1 s après chaque chargement, à dpr
  // fractionnaire). Chrome cale déjà les couches composées sur la grille de
  // pixels ; ce qui compte vraiment — bitmap == taille CSS × dpr — est garanti
  // par la quantification des tailles de page (JSX + renderAllPagesNow).
  const SEL_ANIM_MS = 70
  const animatedSelRects = useCallback((pageIdx: number, pg: PageLayout, from: number, to: number, head: number): SelectionRect[] => {
    const target = selectionRects(pg.layout, from, to)
    const t = Math.min(1, (performance.now() - selAnimT0Ref.current) / SEL_ANIM_MS)
    const prev = shownSelRectsRef.current.get(pageIdx)
    let shown = target
    if (t < 1 && prev && prev.length) {
      // Ligne de la tête de sélection dans CETTE page (sinon : rien à animer ici).
      let pmLo = Infinity, pmHi = -Infinity
      for (const para of pg.layout.paragraphs) { if (para.pmStart < pmLo) pmLo = para.pmStart; if (para.pmEnd > pmHi) pmHi = para.pmEnd }
      if (head >= pmLo && head <= pmHi) {
        const headTop = posToCoords(pg.layout, head).lineTop
        if (headTop != null) {
          const k = 1 - Math.pow(1 - t, 3)   // ease-out cubic
          shown = target.map(tr => {
            if (Math.abs(tr.y - headTop) >= 3) return tr           // pas la ligne du pointeur → direct
            const p = prev.find(pp => Math.abs(pp.y - tr.y) < 3)
            if (!p) return tr                                      // tête vient d'arriver sur la ligne → direct
            return { x: p.x + (tr.x - p.x) * k, y: tr.y, w: p.w + (tr.w - p.w) * k, h: tr.h }
          })
        }
      }
    }
    shownSelRectsRef.current.set(pageIdx, shown)
    return shown
  }, [])
  // Boucle d'animation : repeint chaque frame jusqu'à la fin de la transition.
  const selAnimRafRef = useRef(0)
  const kickSelAnim = useCallback(() => {
    if (selAnimRafRef.current) return
    const loop = () => {
      selAnimRafRef.current = 0
      renderAllPagesNowRef.current?.()
      if (performance.now() - selAnimT0Ref.current < SEL_ANIM_MS) selAnimRafRef.current = requestAnimationFrame(loop)
    }
    selAnimRafRef.current = requestAnimationFrame(loop)
  }, [])
  useEffect(() => () => { if (selAnimRafRef.current) cancelAnimationFrame(selAnimRafRef.current) }, [])

  // Hooks de test E2E (CDP) — PERMANENTS et volontairement minuscules : le banc
  // (scripts scratchpad cdp_*.mjs) lit la sélection et la géométrie du tableau
  // sans dépendre du DOM interne. Ne pas retirer (les retraits/réajouts répétés
  // coûtaient un cycle build+deploy à chaque campagne de test).
  useEffect(() => {
    const w = window as unknown as Record<string, unknown>
    const cellOf = (pos: number): number | null => {
      const ed = editorRef.current; if (!ed) return null
      const $p = ed.state.doc.resolve(Math.max(0, Math.min(pos, ed.state.doc.content.size)))
      for (let d = $p.depth; d > 0; d--) if ($p.node(d).type.name === 'tableCell') return $p.before(d)
      return null
    }
    w.__kbSel = () => { const s = editorRef.current?.state.selection; if (!s) return null
      return { from: s.from, to: s.to, empty: s.empty, cellFrom: cellOf(s.from), cellTo: cellOf(Math.max(s.from, s.to - 1)), sameCell: cellOf(s.from) === cellOf(Math.max(s.from, s.to - 1)) } }
    w.__kbText = (a: number, b: number) => {
      const ed = editorRef.current; if (!ed) return null
      const M = ed.state.doc.content.size
      return ed.state.doc.textBetween(Math.max(0, Math.min(a, M)), Math.max(0, Math.min(b, M)), '⏎', '□')
    }
    // Vérité terrain : rect DOM réel du caret (le sondage par position PM diverge
    // aux frontières de pages). null si masqué.
    w.__kbCaretRect = () => {
      const el = caretRef.current
      if (!el || el.style.display === 'none') return null
      const r = el.getBoundingClientRect()
      return { top: Math.round(r.top), bottom: Math.round(r.bottom), left: Math.round(r.left) }
    }
    // Position écran (viewport) du centre du caractère à `pos` — pour viser un
    // mot précis dans les tests, indépendamment de l'état du document.
    w.__kbPosXY = (pos: number) => {
      const pgs = pagesRef.current
      const idx = pageIndexForHead(pgs, pos)
      const pg = pgs[idx]; const cv = canvasRefs.current.get(idx)
      if (!pg || !cv) return null
      const z = zoomRef.current, geom = geomOf(pg), r = cv.getBoundingClientRect()
      const cm = posToCoords(pg.layout, pos)
      return { x: Math.round(r.left + (geom.marginH + cm.x) * z), y: Math.round(r.top + (geom.marginV + cm.y + 8) * z) }
    }
    // Première occurrence d'un texte → position PM EXACTE via les nœuds texte
    // (les index du texte plat dérivent aux frontières de blocs).
    w.__kbFind = (needle: string) => {
      const ed = editorRef.current; if (!ed) return null
      let found: number | null = null
      ed.state.doc.descendants((n, p2) => {
        if (found != null) return false
        if (n.isText && n.text) { const i = n.text.indexOf(needle); if (i >= 0) { found = p2 + i; return false } }
        return true
      })
      return found
    }
    // Géométrie BRUTE des cellules (repère contenu) + bordures par côté : sonde de
    // test pour les bordures de tableau (les positions écran de __kbCells ne
    // permettent pas de distinguer un bord manquant d'un bord hors clip).
    w.__kbTableCells = () => {
      const out: unknown[] = []
      pagesRef.current.forEach((pg, page) => {
        for (const para of pg.layout.paragraphs) if (para.table) {
          for (const cell of para.table.cells) {
            out.push({ page, r: cell.r, c: cell.c, colspan: cell.colspan, rowspan: cell.rowspan,
                       x: +cell.x.toFixed(2), y: +cell.y.toFixed(2), w: +cell.w.toFixed(2), h: +cell.h.toFixed(2),
                       borders: cell.borders ?? null })
          }
        }
      })
      return out
    }
    w.__kbCells = () => {
      const out: Array<{ r: number; c: number; x: number; y: number }> = []
      canvasRefs.current.forEach((cv, idx) => {
        const pg = pagesRef.current[idx]; if (!pg) return
        const geom = geomOf(pg); const rect = cv.getBoundingClientRect(); const z = zoomRef.current
        for (const para of pg.layout.paragraphs) if (para.table) for (const cell of para.table!.cells)
          out.push({ r: cell.r, c: cell.c, x: Math.round(rect.left + (geom.marginH + cell.x + Math.min(cell.w, 90) / 2) * z),
                     y: Math.round(rect.top + (geom.marginV + cell.y + Math.min(cell.h / 2, 24)) * z) })
      })
      return out
    }
  }, [])

  // Peint UNE page : dimensionne le canvas au besoin (supersampling), dessine le
  // contenu puis les décorations de marge (en-tête/pied/numéro estompés à l'écran).
  const paintOnePage = useCallback((idx: number, pg: PageLayout, inp: PaintInputs) => {
    const cv = canvasRefs.current.get(idx); if (!cv) return
    const { dpr, range, spell, highlights, focused, z } = inp
    const gg = geomOf(pg)
    const bw = Math.round(gg.pageW * z * dpr), bh = Math.round(gg.pageH * z * dpr)
    if (cv.width !== bw) cv.width = bw
    if (cv.height !== bh) cv.height = bh
    renderDocument(cv, pg.layout, gg.marginH, gg.marginV, dpr, z, range, focused, spell, highlights,
      range ? animatedSelRects(idx, pg, range.from, range.to, range.head) : undefined,
      solidPageBg(pg.secIdx))
    const cx = cv.getContext('2d')
    if (cx) {
      cx.save()
      cx.scale(dpr * z, dpr * z)
      // En édition inline, la bande active de la page d'ancrage est affichée par
      // l'input → on ne la dessine PAS sur le canvas (sinon texte en doublon).
      const hfe = hfEditRef.current
      const skipBand = hfe && hfe.pageIdx === idx ? hfe.band : undefined
      drawPageDecorations(cx, gg, idx, pagesRef.current.length, pg.secIdx, skipBand, true)
      cx.restore()
    }
  }, [drawPageDecorations, animatedSelRects, solidPageBg])

  // Bande visible (px écran) élargie d'un viewport de pré-chargement de part et
  // d'autre. `null` = pas de conteneur défilant → aucune restriction (tout peint).
  const visibleBand = useCallback((): { top: number; bottom: number } | null => {
    const sc = scrollContainerRef.current
    if (!sc) return null
    const r = sc.getBoundingClientRect()
    const m = sc.clientHeight   // pré-charge une hauteur de viewport de chaque côté
    return { top: r.top - m, bottom: r.bottom + m }
  }, [scrollContainerRef])

  // Repeint uniquement les pages proches du viewport (culling). Un document de 50
  // pages ne coûte plus que ~2-3 pages peintes par frappe. Les pages hors bande
  // sortent de renderedPagesRef (« périmées ») et sont repeintes au défilement.
  const renderAllPagesNow = useCallback(() => {
    const inp = paintInputs(); if (!inp) return
    const z = inp.z
    const band = visibleBand()
    const pgs = pagesRef.current
    const rendered = renderedPagesRef.current
    // Passe 1 : dimensions CSS (idempotentes, aucune écriture en régime de frappe)
    // → la lecture de visibilité (passe 2) ne provoque qu'un seul reflow groupé.
    pgs.forEach((pg, idx) => {
      const cv = canvasRefs.current.get(idx); if (!cv) return
      const gg = geomOf(pg)
      // Quantized to whole DEVICE pixels, matching the backing-store rounding in
      // paintOnePage — a fractional mismatch (317.59 CSS vs 318px backing at 40%)
      // makes the compositor resample the whole page and smear the ink.
      const d = inp.dpr / (pinchPaint.coarse ? 0.5 : 1)   // dpr réel (inp.dpr est halvé pendant le pincement)
      const wStr = `${Math.round(gg.pageW * z * d) / d}px`, hStr = `${Math.round(gg.pageH * z * d) / d}px`
      if (cv.style.width !== wStr) cv.style.width = wStr
      if (cv.style.height !== hStr) cv.style.height = hStr
    })
    // Passe 2 : visibilité (lecture seule) puis peinture des pages retenues.
    pgs.forEach((pg, idx) => {
      const cv = canvasRefs.current.get(idx); if (!cv) return
      if (band) { const r = cv.getBoundingClientRect(); if (r.bottom < band.top || r.top > band.bottom) { rendered.delete(idx); return } }
      paintOnePage(idx, pg, inp)
      rendered.add(idx)
    })
  }, [paintInputs, paintOnePage, visibleBand])
  renderAllPagesNowRef.current = renderAllPagesNow   // pour la boucle d'animation de sélection
  // Raffinement plein-res après un geste terminé sur un commit demi-res.
  useEffect(() => {
    pinchPaint.refine = () => renderAllPagesNowRef.current?.()
    return () => { pinchPaint.refine = null }
  }, [])

  // Coalescence des repeints : de NOMBREUX chemins appellent `renderAllPages` de façon
  // SYNCHRONE dans un même cycle (plusieurs effets pendant un commit React, onUpdate +
  // onSelectionUpdate + effets de dépendances…). Sur un gros document, repeindre N fois
  // dans la même frame — chaque passe balayant/peignant les pages + soulignés du correcteur
  // — figeait le thread. On regroupe tous les appels d'une frame en UN SEUL paint (rAF).
  // Identité STABLE (dépend du seul `renderAllPagesNow`) → ne relance pas les effets qui
  // l'ont en dépendance. Le rAF est aussi throttlé par le navigateur pour un onglet caché.
  const renderRafRef = useRef(0)
  const renderAllPages = useCallback(() => {
    if (renderRafRef.current) return
    renderRafRef.current = requestAnimationFrame(() => { renderRafRef.current = 0; renderAllPagesNow() })
  }, [renderAllPagesNow])

  // Défilement : peint les pages ENTRÉES dans la bande visible et pas encore à jour
  // (périmées par une frappe hors-écran). Les pages déjà peintes sont ignorées.
  const paintNewlyVisible = useCallback(() => {
    const band = visibleBand(); if (!band) return
    const rendered = renderedPagesRef.current
    let inp: PaintInputs | null = null
    pagesRef.current.forEach((pg, idx) => {
      if (rendered.has(idx)) return
      const cv = canvasRefs.current.get(idx); if (!cv) return
      const r = cv.getBoundingClientRect()
      if (r.bottom < band.top || r.top > band.bottom) return
      if (!inp) { inp = paintInputs(); if (!inp) return }
      paintOnePage(idx, pg, inp)
      rendered.add(idx)
    })
  }, [paintInputs, paintOnePage, visibleBand])

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
  }, [spellCheck, grammarCheck, grammarRules, computeSpell, renderAllPages])
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

  // Défilement : (1) peindre les pages qui entrent dans la bande visible (culling) ;
  // (2) menu contextuel MASQUÉ pendant le défilement, ré-affiché à l'ARRÊT
  // (~350ms sans événement) — comportement des toolbars natives (AOSP
  // FloatingActionMode : cachée dès que le rect bouge, réaffichée à l'arrêt) et
  // de Word/Docs mobile. rAF-throttlé pour la peinture.
  useEffect(() => {
    const sc = scrollContainerRef.current
    if (!sc) return
    let raf = 0
    const onScroll = () => {
      if (!miniBarSuppressedRef.current && miniBarMouseRef.current && !miniBarDismissedRef.current) {
        miniBarSuppressedRef.current = true
      }
      recomputeBodyMiniBar()   // masque (suppressed) — et repositionnera au retour
      // La goutte d'insertion ne suit pas le défilement → masquée.
      if (caretHandleUntilRef.current) { caretHandleUntilRef.current = 0; recomputeCaretHandle() }
      clearTimeout(miniBarScrollTimerRef.current)
      miniBarScrollTimerRef.current = setTimeout(() => {
        if (miniBarSuppressedRef.current) { miniBarSuppressedRef.current = false; recomputeBodyMiniBar() }
      }, 350)
      if (!raf) raf = requestAnimationFrame(() => { raf = 0; paintNewlyVisible() })
    }
    sc.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      sc.removeEventListener('scroll', onScroll)
      if (raf) cancelAnimationFrame(raf)
      clearTimeout(miniBarScrollTimerRef.current)
    }
  }, [scrollContainerRef, recomputeBodyMiniBar, paintNewlyVisible])

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

  /**
   * Is a TEXT character painted at that screen point? An object sitting behind
   * the text only takes the click where there is no character — Writer's rule
   * for #i89920# (feshview.cxx:1388-1444). Tested on the painted extent of the
   * line, not on its full width: the right margin of a short line is empty.
   */
  const textHitAtScreen = useCallback((clientX: number, clientY: number): boolean => {
    let found = false
    canvasRefs.current.forEach((cv, k) => {
      if (found) return
      const r = cv.getBoundingClientRect()
      if (clientX < r.left || clientX > r.right || clientY < r.top || clientY > r.bottom) return
      const pg = pagesRef.current[k]; if (!pg) return
      const z = zoomRef.current, gg = geomOf(pg)
      const x = (clientX - r.left) / z - gg.marginH
      const y = (clientY - r.top) / z - gg.marginV
      for (const para of pg.layout.paragraphs) {
        for (const ln of para.lines) {
          if (ln.image || ln.phantom) continue
          if (y < ln.y || y > ln.y + ln.height) continue
          for (const sp of ln.spans) {
            if (x >= sp.x && x <= sp.x + sp.width) { found = true; return }
          }
        }
      }
    })
    return found
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

  // ── LOUPE tactile ─────────────────────────────────────────────────────────────
  // Médaillon grossi au-dessus du doigt pendant les manipulations PRÉCISES
  // (appui long, glissement des poignées de sélection) — la page, elle, ne bouge
  // JAMAIS (choix utilisateur : pas de zoom automatique, trop de déclenchements
  // involontaires). Rendu = blit du canvas de page déjà peint → quasi gratuit,
  // piloté en impératif (aucun re-render). Grossissement visé ≈ 130 % du document
  // quel que soit le zoom courant.
  const loupeRef   = useRef<HTMLDivElement | null>(null)
  const loupeCvRef = useRef<HTMLCanvasElement | null>(null)
  const LOUPE_W = 150, LOUPE_H = 64
  const showLoupe = useCallback((clientX: number, clientY: number) => {
    const wrap = loupeRef.current, lcv = loupeCvRef.current
    if (!wrap || !lcv) return
    const idx = pageAtPoint(clientX, clientY, 0)
    const cv = canvasRefs.current.get(idx)
    if (!cv) { wrap.style.display = 'none'; return }
    const rect = cv.getBoundingClientRect()
    if (rect.width <= 0) { wrap.style.display = 'none'; return }
    const bx = cv.width / rect.width                       // px écran → px de backing store
    const mag = Math.max(1.2, Math.min(3, 1.3 / (zoomRef.current || 1)))
    const dpr = window.devicePixelRatio || 1
    if (lcv.width !== Math.round(LOUPE_W * dpr)) { lcv.width = Math.round(LOUPE_W * dpr); lcv.height = Math.round(LOUPE_H * dpr) }
    const ctx = lcv.getContext('2d')
    if (!ctx) return
    const sw = (LOUPE_W / mag) * bx, sh = (LOUPE_H / mag) * bx
    const sx = (clientX - rect.left) * bx - sw / 2
    const sy = (clientY - rect.top) * bx - sh / 2
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(0, 0, lcv.width, lcv.height)
    ctx.imageSmoothingEnabled = true
    ctx.imageSmoothingQuality = 'high'
    ctx.drawImage(cv, sx, sy, sw, sh, 0, 0, lcv.width, lcv.height)
    const left = Math.min(Math.max(8, clientX - LOUPE_W / 2), window.innerWidth - LOUPE_W - 8)
    let top = clientY - LOUPE_H - 48                      // au-dessus du doigt…
    if (top < 8) top = clientY + 40                       // …sinon en dessous (haut d'écran)
    wrap.style.display = 'block'
    wrap.style.left = `${left}px`
    wrap.style.top  = `${top}px`
  }, [pageAtPoint])
  const hideLoupe = useCallback(() => { if (loupeRef.current) loupeRef.current.style.display = 'none' }, [])

  // Double-TAP = sélection du MOT · triple-TAP = sélection du PARAGRAPHE (guide
  // tactile Word). Détectés dans le flux TOUCH (les dblclick synthétiques sont
  // trop inégaux selon les navigateurs). PARTAGÉ avec les poignées : après un
  // double-tap, les poignées recouvrent le mot — le 3e tap atterrit dessus et
  // doit compter quand même (déclaré AVANT selHandleDrag : deps).
  const tapCountRef = useRef<{ t: number; x: number; y: number; n: number }>({ t: 0, x: 0, y: 0, n: 0 })
  // Triple-clic/tap : bornes du « paragraphe ». Dans une CELLULE de tableau, le
  // bloc le plus PROFOND (paragraphe/titre INTERNE à la cellule) — façon Word et
  // Google Docs — jamais le tableau entier (le layout ne voit qu'UN paragraphe
  // pour tout le tableau). Cellule vide : son contenu. Corps du texte lu via
  // refs → utilisable depuis des callbacks mémoïsés sans souci d'obsolescence.
  const paraBoundsAt = (pos: number): { from: number; to: number } => {
    const ed = editorRef.current
    const layout = contLayoutRef.current
    if (ed) {
      const $p = ed.state.doc.resolve(Math.max(0, Math.min(pos, ed.state.doc.content.size)))
      let cellD = -1
      for (let d = $p.depth; d > 0; d--) if ($p.node(d).type.name === 'tableCell') { cellD = d; break }
      if (cellD >= 0) {
        for (let d = $p.depth; d > cellD; d--) {
          const tn = $p.node(d).type.name
          if (tn === 'paragraph' || tn === 'heading') return { from: $p.start(d), to: $p.end(d) }
        }
        return { from: $p.start(cellD), to: $p.end(cellD) }
      }
    }
    return layout ? paragraphBoundariesAt(layout, pos) : { from: pos, to: pos }
  }

  const handleTapAt = useCallback((x: number, y: number) => {
    const now = Date.now()
    const prev = tapCountRef.current
    const near = Math.abs(x - prev.x) < 24 && Math.abs(y - prev.y) < 24
    const n = (now - prev.t < 450 && near) ? prev.n + 1 : 1
    tapCountRef.current = { t: now, x, y, n: n >= 3 ? 0 : n }
    // Tap simple : sélectionne la BORDURE de tableau touchée (contrôle de
    // redimensionnement dessus) — un tap ailleurs désélectionne. Multi-tap
    // (mot/paragraphe) : désélectionne la bordure.
    if (n === 1) {
      // Toujours prendre la nouvelle valeur si une bordure est touchée : un
      // re-tap sur la MÊME ligne replace le cercle sous le doigt (`along`).
      const line = editorRef.current?.isEditable ? tableBorderAt(x, y) : null
      setTouchTblLine(prev2 => (prev2 === null && line === null) ? prev2 : line)
      // Menus contextuels façon Word/iOS (hors bordure de tableau) :
      //  · tap SUR la sélection existante → menu ré-affiché, sélection PRÉSERVÉE
      //    (Android la détruit, Word/iOS la gardent — on suit Word) ;
      //  · tap SUR le caret déjà posé (« tap again ») → menu d'insertion
      //    Coller · Tout sélectionner.
      // Dans les deux cas on AVALE le clic souris synthétique qui suivrait
      // (il replierait la sélection / re-poserait le caret).
      if (!line) {
        const ed1 = editorRef.current
        const idx1 = pageAtPoint(x, y, 0)
        const p1 = posFromEvent(idx1, x, y)
        const s1 = ed1?.state.selection
        if (ed1 && p1 != null && s1 instanceof TextSelection) {
          if (!s1.empty && p1 >= s1.from && p1 <= s1.to) {
            lpFiredAtRef.current = now
            armBodyMiniBar()
          } else if (s1.empty && Math.abs(p1 - s1.head) <= 1) {
            lpFiredAtRef.current = now
            armCaretBar()
            armCaretHandle()   // la goutte reste sous le caret avec le menu
          } else {
            // Tap simple : le clic synthétique va poser le caret → la goutte
            // d'insertion apparaît dessous (fenêtre 4s, repositionnée par
            // l'onSelectionUpdate du placement).
            armCaretHandle()
          }
        }
      }
    } else setTouchTblLine(null)
    if (n < 2) return
    const ed = editorRef.current
    const layout = contLayoutRef.current
    if (!ed || !layout) return
    const idx = pageAtPoint(x, y, 0)
    const p = posFromEvent(idx, x, y)
    if (p == null) return
    const { from, to } = n === 2 ? wordBoundariesAt(layout, p) : paraBoundsAt(p)
    if (from < to) {
      ed.commands.setTextSelection({ from, to })
      armBodyMiniBar()
      lpFiredAtRef.current = now      // avale le clic souris synthétique qui suit
      navigator.vibrate?.(8)
    }
  }, [pageAtPoint, posFromEvent]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Glissement d'une poignée de sélection tactile ─────────────────────────────
  // L'extrémité opposée sert d'ANCRE (min/max → les poignées peuvent se croiser,
  // comme sur Android). Le point visé est décalé AU-DESSUS du doigt (la goutte
  // pend sous la ligne) ; auto-défilement près des bords du conteneur.
  const selHandleDrag = useCallback((edge: 'from' | 'to') => (e: React.PointerEvent) => {
    const ed = editorRef.current
    if (!ed) return
    e.preventDefault(); e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    const sel0 = ed.state.selection
    const anchor = edge === 'from' ? sel0.to : sel0.from
    // Si la sélection est DANS une cellule, l'extension par poignée reste
    // CLOISONNÉE à cette cellule (jamais à cheval sur une autre) — comme Word /
    // Google Docs mobile où la poignée bute au bord de la cellule.
    const anchorCell = cellContentRange(anchor)
    // Tap RAPIDE sans mouvement sur la poignée = un tap sur le texte dessous
    // (indispensable au triple-tap : après double-tap, la poignée couvre le mot).
    const t0 = Date.now(), sx0 = e.clientX, sy0 = e.clientY
    let moved = false
    const FINGER_DY = 22
    const apply = (cx: number, cyRaw: number) => {
      const cy = cyRaw - FINGER_DY
      const idx = pageAtPoint(cx, cy, 0)
      const p = posFromEvent(idx, cx, cy)
      if (p == null) return
      let from = Math.min(anchor, p), to = Math.max(anchor, p)
      if (anchorCell) { from = Math.max(anchorCell.from, from); to = Math.min(anchorCell.to, to) }
      const cur = ed.state.selection
      if (from < to && (cur.from !== from || cur.to !== to)) ed.commands.setTextSelection({ from, to })
    }
    let last = { x: e.clientX, y: e.clientY }
    let raf: number | null = null
    const EDGE = 48
    const tick = () => {
      raf = null
      const sc = scrollContainerRef.current
      if (!sc) return
      const r = sc.getBoundingClientRect()
      const dy = last.y < r.top + EDGE ? -(r.top + EDGE - last.y) : last.y > r.bottom - EDGE ? last.y - (r.bottom - EDGE) : 0
      if (dy !== 0) { sc.scrollTop += dy * 0.15; apply(last.x, last.y); raf = requestAnimationFrame(tick) }
    }
    const move = (me: PointerEvent) => {
      if (!moved && (Math.abs(me.clientX - sx0) > 8 || Math.abs(me.clientY - sy0) > 8)) {
        moved = true
        // Glissé de poignée : menu contextuel MASQUÉ pendant l'ajustement
        // (toolbars natives : cachée sur mouvement, réaffichée après relâchement).
        miniBarSuppressedRef.current = true
        recomputeBodyMiniBar()
      }
      if (!moved) return                              // pas encore un glissé : rien à faire
      last = { x: me.clientX, y: me.clientY }
      apply(me.clientX, me.clientY)
      showLoupe(me.clientX, me.clientY - FINGER_DY)   // loupe sur le point visé
      if (raf == null) raf = requestAnimationFrame(tick)
    }
    const up = () => {
      hideLoupe()
      if (raf != null) cancelAnimationFrame(raf)
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', up)
      // Tap bref immobile → compte comme un tap sur le texte sous la poignée.
      if (!moved && Date.now() - t0 < 300) handleTapAt(sx0, sy0)
      // Après un vrai glissé : le menu revient ~300ms après le relâchement
      // (délai AOSP anti double-tap), sur la sélection ajustée.
      if (moved) setTimeout(() => { miniBarSuppressedRef.current = false; armBodyMiniBar() }, 300)
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', up)
  }, [pageAtPoint, posFromEvent, handleTapAt]) // eslint-disable-line react-hooks/exhaustive-deps

  // Appui LONG tactile sur une page = sélection du MOT (les poignées prennent le
  // relais pour étendre). Annulé si le doigt bouge (défilement) ou se lève avant.
  const longPressRef = useRef<{ timer: number; x: number; y: number } | null>(null)
  // Horodatage du dernier appui long ABOUTI : les événements souris synthétiques
  // qui suivent le relâchement (tap → mousedown/click) sont ignorés pendant 700ms.
  const lpFiredAtRef = useRef(0)
  const onPagesTouchStart = useCallback((e: React.TouchEvent) => {
    // Outil FORME armé : le doigt DESSINE (cf. onPagePointerDown). Ni appui long
    // (sélection de mot + loupe), ni tap — les deux poseraient un caret ou une
    // sélection au beau milieu du tracé.
    if (armedShapeRef.current) return
    const t0 = e.touches[0]
    // 2e doigt (pincement zoom) → l'appui long en cours est annulé, loupe éteinte.
    if (e.touches.length > 1) { if (longPressRef.current) { clearTimeout(longPressRef.current.timer); longPressRef.current = null } ; hideLoupe(); return }
    if (!t0) return
    const x = t0.clientX, y = t0.clientY
    const timer = window.setTimeout(() => {
      longPressRef.current = null
      const ed = editorRef.current
      const layout = contLayoutRef.current
      if (!ed || !layout) return
      const idx = pageAtPoint(x, y, 0)
      const p = posFromEvent(idx, x, y)
      if (p == null) return
      const { from, to } = wordBoundariesAt(layout, p)
      if (from < to) {
        ed.commands.setTextSelection({ from, to })
        armBodyMiniBar()   // barre contextuelle (Couper/Copier/Coller) sur la sélection tactile
        navigator.vibrate?.(10)
        // Le CLIC SOURIS SYNTHÉTIQUE émis au relâchement replacerait le caret
        // → fenêtre d'immunité consommée par onPageMouseDown.
        lpFiredAtRef.current = Date.now()
        // Loupe sur le mot saisi (visible tant que le doigt reste posé).
        showLoupe(x, y)
      }
    }, 450)
    longPressRef.current = { timer, x, y }
  }, [pageAtPoint, posFromEvent])
  const cancelLongPress = useCallback(() => {
    if (longPressRef.current) { clearTimeout(longPressRef.current.timer); longPressRef.current = null }
    hideLoupe()
  }, [hideLoupe])

  const onPagesTouchEnd = useCallback(() => {
    const lp = longPressRef.current   // encore armé ⇒ ni glissé ni appui long ⇒ TAP
    cancelLongPress()
    if (lp) handleTapAt(lp.x, lp.y)
  }, [cancelLongPress, handleTapAt])
  const onPagesTouchMove = useCallback((e: React.TouchEvent) => {
    const lp = longPressRef.current
    const t0 = e.touches[0]
    if (lp && t0 && (Math.abs(t0.clientX - lp.x) > 10 || Math.abs(t0.clientY - lp.y) > 10)) cancelLongPress()
  }, [cancelLongPress])

  // Hit-test d'une cellule de tableau sous le pointeur → { tableStart (pmStart du
  // tableau), r, c, colspan, rowspan } en coordonnées de grille. Sert à la sélection.
  // Plage PM de CONTENU de la cellule de tableau contenant `pos` (façon Word /
  // Google Docs / ProseMirror : une sélection de TEXTE reste CLOISONNÉE à sa
  // cellule — jamais à cheval sur deux cellules). null hors d'un tableau.
  const cellContentRange = useCallback((pos: number): { from: number; to: number } | null => {
    const ed = editorRef.current; if (!ed) return null
    const $p = ed.state.doc.resolve(Math.max(0, Math.min(pos, ed.state.doc.content.size)))
    for (let d = $p.depth; d > 0; d--) {
      if ($p.node(d).type.name === 'tableCell') return { from: $p.start(d), to: $p.end(d) }
    }
    return null
  }, [])
  // Borne un intervalle [a,b] à la cellule d'ancrage si `anchor` est en cellule.
  const clampToCell = useCallback((anchor: number, a: number, b: number): { from: number; to: number } => {
    const cr = cellContentRange(anchor)
    let from = Math.min(a, b), to = Math.max(a, b)
    if (cr) { from = Math.max(cr.from, from); to = Math.min(cr.to, to) }
    return { from, to }
  }, [cellContentRange])

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
  // Titre actuellement SURVOLÉ (pmStart) : son chevron ▼ est affiché (façon Word,
  // le chevron n'apparaît qu'au survol ; ▶ replié reste toujours visible).
  const hoverHeadingRef = useRef<number | null>(null)
  const outlineArrowsRef = useRef(outlineArrows); outlineArrowsRef.current = outlineArrows
  useEffect(() => { renderAllPages() }, [outlineArrows, renderAllPages])
  // Titre contenant le CARET : Word y montre aussi le triangle (indispensable au
  // tactile, où il n'y a pas de survol). Mis à jour à chaque sélection.
  const caretHeadingRef = useRef<number | null>(null)

  const hitHeadingTriangle = useCallback((pageIdx: number, clientX: number, clientY: number): number | null => {
    if (!outlineArrowsRef.current) return null
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

  // Appel de note (exposant bleu) ou ligne du bloc de notes sous le point écran →
  // position PM du nœud footnote (pour ouvrir l'éditeur de note).
  const hitFootnote = useCallback((pageIdx: number, clientX: number, clientY: number): number | null => {
    const cv = canvasRefs.current.get(pageIdx)
    const pg = pagesRef.current[pageIdx]
    if (!cv || !pg) return null
    const r = cv.getBoundingClientRect()
    const z = zoomRef.current, gg = geomOf(pg)
    const x = (clientX - r.left) / z - gg.marginH
    const y = (clientY - r.top) / z - gg.marginV
    for (const para of pg.layout.paragraphs) {
      for (const ln of para.lines) {
        if (y < ln.y - 2 || y > ln.y + ln.height + 2) continue
        for (const sp of ln.spans) {
          if (sp.fn && x >= sp.x - 2 && x <= sp.x + Math.max(sp.width, 8) + 2) return sp.pmPos
        }
      }
    }
    // Bloc de notes au bas de la page (repère PAGE, pas contenu).
    const px = (clientX - r.left) / z, py = (clientY - r.top) / z
    for (const b of fnBoxesRef.current.get(pageIdx) ?? []) {
      if (px >= b.x0 && px <= b.x1 && py >= b.y0 && py <= b.y1) return b.pos
    }
    return null
  }, [])

  // Image FLOTTANTE (devant le texte / habillage carré) sous le point écran → sa
  // position PM. Le hit-test texte (`posFromEvent`) tomberait sur le texte derrière ;
  // ces objets ne sont pas dans le flux, on les teste donc géométriquement.
  const floatingImageAt = useCallback((clientX: number, clientY: number, altKey = false): number | null => {
    const root = rootRef.current, layout = contLayoutRef.current
    if (!root || !layout) return null
    const rect = root.getBoundingClientRect()
    const z = zoomRef.current, pgs = pagesRef.current
    const px = clientX - rect.left, py = clientY - rect.top
    // Désignation façon Writer/svx (documents/layout/object-hit.ts) : ordre de PLAN
    // (le dessus gagne), tolérance de clic, rotation, contour, priorité à l'objet
    // déjà sélectionné, alt+clic pour descendre d'un plan, et « derrière le texte »
    // qui cède au texte qu'il recouvre.
    //
    // Les boîtes sont construites en pixels ÉCRAN (déjà multipliés par le zoom),
    // donc `zoom: 1` : la tolérance de 3 px reste bien une grandeur d'écran.
    const objs: HitObject[] = []
    for (const para of layout.paragraphs) for (const ln of para.lines) {
      const im = ln.image
      const wrap = im?.wrap || 'inline'
      if (!im || !isFloatingWrap(wrap)) continue
      // Page d'affichage = celle du HAUT de l'objet (wrapY inclus), pas de l'ancre.
      const gRefY = ln.y + (im.wrapY || 0)
      let idx = 0
      for (let k = 0; k < pgs.length; k++) if (gRefY >= pgs[k].startY - 0.5) idx = k
      const geom = geomOf(pgs[idx]); const o = pageOrigin(idx)
      const localY = ln.y - (pgs[idx]?.startY ?? 0)
      objs.push({
        id: String(ln.pmStart),
        x: o.left + (geom.marginH + im.x) * z,
        y: o.top + (geom.marginV + localY + (im.wrapY || 0)) * z,
        w: im.w * z, h: im.h * z,
        rotation: im.rotation || 0,
        wrap,
        // Ordre de peinture réel (cf. paintLayout) : « derrière » sous le texte,
        // « devant » au-dessus, le reste dans le flux. À couche égale c'est le
        // `zOrder` du fichier (`wp:anchor/@relativeHeight`) qui tranche ; à
        // défaut, l'ordre du document — le plus tardif est peint en dernier.
        z: (wrap === 'behind' ? 0 : wrap === 'front' ? 2 : 1) * 1e6
           + (im.zOrder != null ? im.zOrder : ln.pmStart),
      })
    }
    if (!objs.length) return null
    const hit = pickObject(px, py, objs, {
      zoom: 1,
      altKey,
      currentId: imgSelRef.current != null ? String(imgSelRef.current) : null,
      // Un objet « derrière le texte » ne prend le clic que là où il n'y a PAS de
      // caractère (cf. feshview.cxx:1388-1444).
      hasTextAt: (hx, hy) => textHitAtScreen(hx + rect.left, hy + rect.top),
    })
    return hit ? Number(hit.id) : null
  }, [])

  /**
   * Image ALIGNÉE SUR LE TEXTE (as-char) sous un point écran, ou null.
   *
   * Une telle image reste un OBJET : chez Writer un clic la sélectionne (même
   * chemin `PickObj` qu'un objet flottant — sw/…/feshview.cxx). On ne peut PAS
   * s'en remettre à `coordsToPos`+`nodeAt` : l'atome peint une boîte large mais
   * n'occupe qu'UNE position PM, donc un clic sur sa moitié droite résout vers le
   * texte suivant et la sélection retombe sur du texte. On teste donc la boîte
   * PEINTE de l'image, comme pour les objets flottants — un clic qui COMMENCE
   * dessus sélectionne l'objet, un glissé qui la traverse en partant d'ailleurs
   * l'englobe dans la sélection de texte (son point de départ est hors boîte).
   */
  const inlineImageAt = useCallback((clientX: number, clientY: number): number | null => {
    const root = rootRef.current, layout = contLayoutRef.current
    if (!root || !layout) return null
    const rect = root.getBoundingClientRect()
    const z = zoomRef.current, pgs = pagesRef.current
    const px = clientX - rect.left, py = clientY - rect.top
    for (const para of layout.paragraphs) {
      for (const ln of para.lines) {
        for (const sp of ln.spans) {
          const im = sp.img
          if (!im) continue
          let idx = 0
          for (let k = 0; k < pgs.length; k++) if (ln.baseline >= pgs[k].startY - 0.5) idx = k
          const geom = geomOf(pgs[idx]); const o = pageOrigin(idx)
          const baseLocal = ln.baseline - (pgs[idx]?.startY ?? 0)
          // AABB de l'image tournée (même formule que `imgAABB` du moteur).
          const rr = ((im.rot || 0) * Math.PI) / 180
          const ab = {
            w: Math.abs(im.w * Math.cos(rr)) + Math.abs(im.h * Math.sin(rr)),
            h: Math.abs(im.w * Math.sin(rr)) + Math.abs(im.h * Math.cos(rr)),
          }
          // Centre de la boîte NON tournée (le rendu peint l'image centrée sur
          // ce point — cf. canvas-engine `span.img`), amené en coordonnées écran.
          const cx = o.left + (geom.marginH + sp.x + ab.w / 2) * z
          const cy = o.top + (geom.marginV + baseLocal - ab.h / 2) * z
          let lx = px - cx, ly = py - cy
          if (rr) { const c = Math.cos(-rr), s = Math.sin(-rr); const nx = lx * c - ly * s, ny = lx * s + ly * c; lx = nx; ly = ny }
          const hw = (im.w * z) / 2, hh = (im.h * z) / 2
          if (Math.abs(lx) <= hw && Math.abs(ly) <= hh) return sp.pmPos
        }
      }
    }
    return null
  }, [])

  /**
   * Glisser-déposer d'un CONTENU sélectionné vers une autre position du texte —
   * partagé par la sélection de TEXTE et l'objet ALIGNÉ SUR LE TEXTE (as-char).
   *
   * Chez Writer un objet ancré comme caractère ne se déplace PAS librement à
   * l'horizontale (edtwin.cxx:1328 : `bMoveAllowed = … AnchorId != FLY_AS_CHAR`) :
   * il se repositionne dans le flux, exactement comme du texte glissé-déposé. Le
   * même mécanisme sert donc aux deux — on coupe [from, to] et on le recolle au
   * point de dépôt, en UNE transaction (un seul Ctrl+Z).
   */
  /**
   * Le point écran est-il SUR la surbrillance de sélection (au-dessus d'un
   * glyphe sélectionné) ? Test GÉOMÉTRIQUE, pas une comparaison de positions PM :
   * une sélection d'UN caractère n'a aucune position PM « strictement à
   * l'intérieur » (`to = from+1`), donc `pos > from && pos < to` la ratait — c'est
   * pourtant ce point qu'il faut pouvoir saisir pour la déplacer, comme Word.
   */
  const pointOverSelection = useCallback((pageIdx: number, clientX: number, clientY: number, from: number, to: number): boolean => {
    if (to <= from) return false
    const idx = pageAtPoint(clientX, clientY, pageIdx)
    const cv = canvasRefs.current.get(idx); const pg = pagesRef.current[idx]
    if (!cv || !pg) return false
    const r = cv.getBoundingClientRect(); const z = zoomRef.current; const g = geomOf(pg)
    const x = (clientX - r.left) / z - g.marginH
    const y = (clientY - r.top) / z - g.marginV
    for (const rc of selectionRects(pg.layout, from, to)) {
      if (x >= rc.x && x <= rc.x + rc.w && y >= rc.y && y <= rc.y + rc.h) return true
    }
    return false
  }, [])

  const beginContentDrag = useCallback((from: number, to: number, isNode: boolean, startX: number, startY: number, pageIdx: number, onPlainClick: () => void) => {
    const ed = editorRef.current; if (!ed) return
    const slice = ed.state.doc.slice(from, to)   // capturé au départ (le doc ne change pas pendant le glissé)
    let dragging = false
    let dropPos: number | null = null
    const showDrop = (clientX: number, clientY: number) => {
      const idx = pageAtPoint(clientX, clientY, pageIdx)
      const p = posFromEvent(idx, clientX, clientY)
      const el = dropCaretRef.current
      if (p == null || !el) return
      dropPos = p
      const pgs = pagesRef.current
      const { idx: ci, cm } = caretLocation(pgs, contLayoutRef.current, p)
      const geom = geomOf(pgs[ci]); const o = pageOrigin(ci); const z = zoomRef.current
      const dprD = window.devicePixelRatio || 1
      const snapD = (v: number) => Math.round(v * dprD) / dprD
      el.style.display = 'block'
      el.style.width   = `${1 / dprD}px`
      el.style.left    = `${snapD(o.left + (geom.marginH + cm.x) * z)}px`
      el.style.top     = `${snapD(o.top + (geom.marginV + cm.y) * z)}px`
      el.style.height  = `${Math.max(4, snapD(cm.height * z))}px`
      el.style.opacity = p >= from && p <= to ? '0.35' : '1'   // cible dans le contenu déplacé → dépôt annulé
    }
    const stopAutoD = () => { if (autoScrollRef.current !== null) { cancelAnimationFrame(autoScrollRef.current); autoScrollRef.current = null } }
    const EDGE_D = 48
    const tickD = () => {
      autoScrollRef.current = null
      const sc = scrollContainerRef.current; if (!sc) return
      const rect = sc.getBoundingClientRect()
      const { x, y } = lastMouseRef.current
      let dy = 0
      if (y > rect.bottom - EDGE_D)   dy =  Math.min(30, (y - (rect.bottom - EDGE_D)) / 2 + 5)
      else if (y < rect.top + EDGE_D) dy = -Math.min(30, ((rect.top + EDGE_D) - y) / 2 + 5)
      if (dy === 0) return
      const before = sc.scrollTop; sc.scrollTop += dy
      showDrop(x, y)
      if (sc.scrollTop !== before) autoScrollRef.current = requestAnimationFrame(tickD)
    }
    const onMoveD = (me: MouseEvent) => {
      if (!dragging) {
        if (Math.abs(me.clientX - startX) < 4 && Math.abs(me.clientY - startY) < 4) return
        dragging = true
        textDragRef.current = true
      }
      lastMouseRef.current = { x: me.clientX, y: me.clientY }
      showDrop(me.clientX, me.clientY)
      const sc = scrollContainerRef.current
      if (sc) {
        const rect = sc.getBoundingClientRect()
        const near = me.clientY > rect.bottom - EDGE_D || me.clientY < rect.top + EDGE_D
        if (near) { if (autoScrollRef.current === null) autoScrollRef.current = requestAnimationFrame(tickD) }
        else stopAutoD()
      }
    }
    const onUpD = () => {
      stopAutoD()
      document.removeEventListener('mousemove', onMoveD)
      document.removeEventListener('mouseup', onUpD)
      const el = dropCaretRef.current; if (el) el.style.display = 'none'
      textDragRef.current = false
      if (!dragging) { onPlainClick(); return }
      const target = dropPos
      if (target == null || (target >= from && target <= to)) return   // dépôt invalide → annulé
      const state = ed.state
      const tr = state.tr
      tr.deleteRange(from, to)
      const ins = tr.mapping.map(target)
      const sizeAfterDel = tr.doc.content.size
      tr.replaceRange(ins, ins, slice)
      const added = tr.doc.content.size - sizeAfterDel
      // Re-sélectionner le contenu déposé (comme Word) : NodeSelection pour un
      // objet, TextSelection pour du texte ; repli sur un caret si la structure
      // a été remaniée.
      try {
        if (isNode) tr.setSelection(NodeSelection.create(tr.doc, ins))
        else tr.setSelection(TextSelection.create(tr.doc, ins, Math.min(ins + added, tr.doc.content.size)))
      } catch { const $i = tr.doc.resolve(Math.min(ins, tr.doc.content.size)); tr.setSelection(TextSelection.between($i, $i)) }
      miniBarMouseRef.current = false
      ed.view.focus()
      ed.view.dispatch(tr.scrollIntoView())
    }
    document.addEventListener('mousemove', onMoveD)
    document.addEventListener('mouseup', onUpD)
  }, [])

  // ── Démarrage d'un tracé de forme (souris OU doigt/stylet) ─────────────────
  // Un seul chemin pour les deux entrées : seule la boucle d'événements diffère
  // (`beginShapeDraw` pour la souris, `beginShapePointerDraw` pour le tactile,
  // qui abandonne le tracé dès qu'un 2ᵉ doigt se pose). Renvoie false si rien
  // n'était armé ou si la page n'est pas prête — l'appelant reprend alors son
  // traitement normal.
  const startShapeDraw = useCallback((pageIdx: number, e: { clientX: number; clientY: number }, pointerId?: number): boolean => {
    const armedKind = armedShapeRef.current
    if (!armedKind) return false
    const cvA = canvasRefs.current.get(pageIdx)
    const pgA = pagesRef.current[pageIdx]
    if (!cvA || !pgA || !editorRef.current) return false
    // Position PM sous le point de départ : elle donne l'ancre du futur objet,
    // donc la PAGE sur laquelle il sera mis en page.
    const anchorPos = posFromEvent(pageIdx, e.clientX, e.clientY)
    const gA = geomOf(pgA)
    // La boîte est en px doc LOCAUX à la page de départ ; les calques d'aperçu,
    // eux, couvrent chacun leur page et raisonnent donc dans le repère du
    // CONTENEUR — d'où la translation par l'origine de la page de départ.
    // L'origine est lue UNE fois : `offsetLeft/Top` sont relatifs au conteneur
    // d'overlays, donc insensibles au défilement, et rien ne remet la page en
    // page pendant un glissé — la relire à chaque frame forcerait un reflow.
    const o = pageOrigin(pageIdx)
    const paintGhost = (box: DrawBox | null) => {
      const z = zoomRef.current || 1
      const c = box ? { x: box.x + o.left / z, y: box.y + o.top / z, w: box.w, h: box.h } : null
      for (const h of ghostRefs.current) h?.paint(c)
    }
    // `clear` range le fantôme ; `finish` prévient en plus le parent que le geste
    // est consommé (il DÉSARME l'outil). Un tracé interrompu par un pincement
    // n'est pas un renoncement : l'outil doit rester armé, sinon zoomer avant de
    // dessiner obligerait à re-choisir la forme.
    const clear = () => { paintGhost(null); setDrawPage(null) }
    const finish = () => { clear(); onShapeDrawnRef.current?.() }
    setDrawPage({ idx: pageIdx, kind: armedKind })
    const handlers = {
      onPreview: (box: DrawBox) => paintGhost(box),
      onCommit: (box: DrawBox) => {
        finish()
        const ed2 = editorRef.current
        if (ed2) insertDrawnShape(ed2, anchorPos, armedKind, box, { left: gA.marginH, top: gA.marginV })
        requestAnimationFrame(() => { renderAllPages(); updateImgSel() })
      },
      onCancel: finish,
      onAbandon: clear,
    }
    if (pointerId == null) beginShapeDraw(armedKind, cvA, zoomRef.current, e, handlers)
    else beginShapePointerDraw(armedKind, cvA, zoomRef.current, { clientX: e.clientX, clientY: e.clientY, pointerId }, handlers)
    return true
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [posFromEvent, renderAllPages, updateImgSel])

  // Appui DOIGT/STYLET sur une page : seul le tracé de forme passe par ici — la
  // souris garde son propre chemin (`onPageMouseDown`), qu'un `pointerdown`
  // « mouse » doublerait sinon. Sans outil armé, le tactile est inchangé
  // (défilement, appui long, tap) : on ne fait rien.
  const onPagePointerDown = useCallback((pageIdx: number, e: React.PointerEvent) => {
    if (e.pointerType === 'mouse' || !armedShapeRef.current) return
    if (!startShapeDraw(pageIdx, e, e.pointerId)) return
    // Le doigt DESSINE : ni défilement, ni appui long, ni menu contextuel natif.
    e.preventDefault(); e.stopPropagation()
  }, [startShapeDraw])

  const onPageMouseDown = useCallback((pageIdx: number, e: React.MouseEvent) => {
    // ── Un outil FORME est armé : ce geste DESSINE, rien d'autre ──────────────
    // En TÊTE de la fonction, avant toute autre garde : ni le caret, ni le
    // hit-test d'objet, ni la sélection de texte ne doivent voir cet appui.
    if (armedShapeRef.current && e.button === 0) {
      if (startShapeDraw(pageIdx, e)) { e.preventDefault(); e.stopPropagation(); return }
    }
    // Clic synthétique consécutif à un appui long tactile (sélection de mot) :
    // il replacerait le caret et détruirait la sélection → ignoré.
    if (Date.now() - lpFiredAtRef.current < 700) return
    const ed = editorRef.current; if (!ed) return

    // Clic DROIT : ne pas déplacer le curseur ni collapser la sélection — le menu
    // contextuel (onPageContextMenu) gère le placement et préserve la sélection.
    if (e.button === 2) return
    // Clic sur le triangle Développer/Réduire d'un titre (marge gauche) → bascule.
    if (e.button === 0) {
      const fpos = hitFootnote(pageIdx, e.clientX, e.clientY)
      if (fpos != null) { openFootnoteEditor(fpos); return }
      // Ctrl/⌘+clic sur une ENTRÉE de table des matières → aller au titre visé
      // (résolution par texte : les positions bougent, façon lien de champ Word).
      if (e.ctrlKey || e.metaKey) {
        const pos = posFromEvent(pageIdx, e.clientX, e.clientY)
        if (pos != null) {
          const $p = ed.state.doc.resolve(Math.min(pos, ed.state.doc.content.size))
          // Internal link (`href="#name"`) → its bookmark, like Word's Ctrl+click.
          const isLink = (m: { type: { name: string } }) => m.type.name === 'link'
          const lk = $p.marks().find(isLink) ?? $p.nodeAfter?.marks.find(isLink)
          const href = lk ? String(lk.attrs.href ?? '') : ''
          if (href.startsWith('#')) {
            const bp = bookmarkPos(ed, href.slice(1))
            if (bp != null) { e.preventDefault(); scrollToPos(bp); return }
          }
          for (let d = $p.depth; d >= 1; d--) {
            const node = $p.node(d)
            if ((node.attrs as Record<string, unknown>)?.tocLevel != null) {
              const label = node.textContent
              let target: number | null = null
              ed.state.doc.descendants((n, p) => {
                if (target != null) return false
                if (n.type.name === 'heading' && !(n.attrs as Record<string, unknown>).tocTitle && (n.textContent || '…') === label) { target = p; return false }
                return true
              })
              if (target != null) { e.preventDefault(); scrollToPos(target + 1); return }
              break
            }
          }
        }
      }
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

    // Clic sur un OBJET (flottant OU aligné sur le texte) qui COMMENCE sur sa
    // boîte → le sélectionner comme objet, jamais comme texte. Un glissé qui
    // l'englobe en partant d'ailleurs passe par le chemin texte ci-dessous (son
    // point de départ est hors de la boîte de l'image).
    if (e.button === 0) {
      const floatPos = floatingImageAt(e.clientX, e.clientY, e.altKey)
      const inlinePos = floatPos == null ? inlineImageAt(e.clientX, e.clientY) : null
      const fpos = floatPos ?? inlinePos
      if (fpos != null) {
        e.preventDefault()
        const n = ed.state.doc.nodeAt(fpos)
        if (e.detail >= 2 && n && parseTextBoxRichAlt(n.attrs.alt as string)) { enterTextBoxEdit(fpos); return }
        ed.view.focus()
        ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, fpos)))
        // Image ALIGNÉE SUR LE TEXTE : elle se déplace dans le FLUX (as-char, pas
        // de position libre) → même glisser-déposer que le texte. L'objet FLOTTANT,
        // lui, se déplace librement par ses poignées (géré ailleurs).
        if (inlinePos != null && n) {
          beginContentDrag(fpos, fpos + n.nodeSize, true, e.clientX, e.clientY, pageIdx, () => {})
        }
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

    // ── Glissé dans un tableau : TEXTE si on reste dans la MÊME cellule (comme
    //    Word), PLAGE DE CELLULES si le glissé franchit une bordure de cellule. ──
    const startCell = e.button === 0 && e.detail < 2 ? hitTableCell(pageIdx, e.clientX, e.clientY) : null
    if (startCell) {
      // Glisser-déposer d'une sélection de TEXTE existante DANS une cellule : un
      // mousedown strictement dans la sélection l'arme (sinon la sélection de
      // cellule ci-dessous la défaisait toujours). Même mécanisme que hors
      // tableau — Word permet ce déplacement dans et entre les cellules.
      const selPre = ed.state.selection
      if (e.detail === 1 && selPre instanceof TextSelection && !selPre.empty
          && pointOverSelection(pageIdx, e.clientX, e.clientY, selPre.from, selPre.to)) {
        e.preventDefault()
        beginContentDrag(selPre.from, selPre.to, false, e.clientX, e.clientY, pageIdx,
          () => ed.chain().focus().setTextSelection(pos).run())
        return
      }
      const anchor = startCell
      const anchorPos = pos
      ed.chain().focus().setTextSelection(pos).run()   // curseur dans la cellule
      const rectOf = (h: { r: number; c: number; colspan: number; rowspan: number }): TableRect => ({
        r0: Math.min(anchor.r, h.r), c0: Math.min(anchor.c, h.c),
        r1: Math.max(anchor.r + anchor.rowspan - 1, h.r + h.rowspan - 1),
        c1: Math.max(anchor.c + anchor.colspan - 1, h.c + h.colspan - 1),
      })
      let didSelect = false   // vrai dès qu'on a réellement sélectionné (texte OU cellules)
      const onMove = (me: MouseEvent) => {
        const pIdx = pageAtPoint(me.clientX, me.clientY, pageIdx)
        const h = hitTableCell(pIdx, me.clientX, me.clientY)
        const sameCell = !!h && h.tableStart === anchor.tableStart && h.r === anchor.r && h.c === anchor.c
        if (h && h.tableStart === anchor.tableStart && !sameCell) {
          // Franchi une bordure → sélection de PLAGE DE CELLULES.
          const rect = rectOf(h)
          setTableSel((rect.r0 === rect.r1 && rect.c0 === rect.c1) ? null : { tableStart: anchor.tableStart, ...rect })
          didSelect = true
        } else {
          // Toujours dans la cellule d'ancrage → sélection de TEXTE, BORNÉE à la
          // cellule (jamais à cheval sur une autre cellule ; `coordsToPos` peut
          // renvoyer une position voisine près d'un bord).
          const p2 = posFromEvent(pIdx, me.clientX, me.clientY)
          if (p2 != null && p2 !== anchorPos) {
            setTableSel(null)
            const { from, to } = clampToCell(anchorPos, anchorPos, p2)
            if (from < to) { ed.commands.setTextSelection({ from, to }); didSelect = true }
          }
        }
      }
      const onUp = () => {
        document.removeEventListener('mousemove', onMove); document.removeEventListener('mouseup', onUp)
        // Glissé de texte terminé (sélection non vide) → arme la mini-barre, comme
        // une sélection à la souris hors tableau.
        const s = editorRef.current?.state.selection
        if (didSelect && s && s.from < s.to && !tableSelRef.current) armBodyMiniBar()
      }
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

    // Double-clic = mot ; triple-clic = paragraphe (paragraphe INTERNE en cellule)
    const layout = contLayoutRef.current
    if (layout && e.detail >= 2) {
      const { from, to } = e.detail === 2
        ? wordBoundariesAt(layout, pos)
        : paraBoundsAt(pos)
      ed.chain().focus().setTextSelection({ from, to }).run()
      armBodyMiniBar()   // double/triple-clic = sélection à la souris → arme la mini-barre
      return
    }

    // ── Redimensionnement de la sélection par ses BORDS (comme la plage du tableur) ──
    // Attraper l'EXTRÉMITÉ gauche/droite de la sélection (± quelques px) puis glisser :
    // la sélection s'étend/rétrécit, l'autre extrémité servant d'ancre. Prioritaire sur
    // le glisser-déposer (dont la zone de saisie est l'INTÉRIEUR de la sélection).
    const selNow = ed.state.selection
    // Le clic est-il SUR la sélection ? Si oui → glisser-déposer (priorité, y
    // compris pour un seul caractère) ; le redimensionnement par les bords ne
    // s'arme que HORS sélection (juste à côté d'un bord).
    const overSel = e.button === 0 && e.detail === 1 && selNow instanceof TextSelection && !selNow.empty
      && pointOverSelection(pageIdx, e.clientX, e.clientY, selNow.from, selNow.to)
    let resizeAnchor: number | null = null
    if (!overSel && e.button === 0 && e.detail === 1 && selNow instanceof TextSelection && !selNow.empty) {
      // Coordonnées VIEWPORT (getBoundingClientRect du canvas de la page — pas
      // pageOrigin/offsetLeft, qui est relatif au conteneur scrollé) pour comparer
      // avec e.clientX/e.clientY.
      const edgeAt = (p: number, preferEnd: boolean) => {
        const pgs = pagesRef.current
        const { idx, cm } = caretLocation(pgs, contLayoutRef.current, p, preferEnd)
        const cv2 = canvasRefs.current.get(idx); if (!cv2) return null
        const r2 = cv2.getBoundingClientRect()
        const geom = geomOf(pgs[idx]); const z = zoomRef.current
        return { x: r2.left + (geom.marginH + cm.x) * z,
                 y: r2.top + (geom.marginV + (cm.lineTop ?? cm.y)) * z,
                 h: (cm.lineH ?? cm.height) * z }
      }
      const TOL = 8
      const eF = edgeAt(selNow.from, false)
      const eT = edgeAt(selNow.to, true)
      const nearFrom = !!eF && Math.abs(e.clientX - eF.x) <= TOL && e.clientY >= eF.y - 2 && e.clientY <= eF.y + eF.h + 2
      const nearTo   = !nearFrom && !!eT && Math.abs(e.clientX - eT.x) <= TOL && e.clientY >= eT.y - 2 && e.clientY <= eT.y + eT.h + 2
      if (nearFrom || nearTo) resizeAnchor = nearFrom ? selNow.to : selNow.from
    }

    // ── Glisser-déposer de la sélection (comme Word) ──────────────────────────
    // Mousedown DANS la sélection texte existante → ne pas la défaire : armer un
    // déplacement. Pendant le glissé, un caret de dépôt suit la souris (avec
    // auto-défilement près des bords) ; au relâchement le texte est déplacé en UNE
    // transaction (un seul Ctrl+Z) puis re-sélectionné. Un dépôt à l'intérieur de la
    // sélection annule. Un simple clic sans mouvement place le caret comme avant.
    if (overSel && selNow instanceof TextSelection) {
      beginContentDrag(selNow.from, selNow.to, false, e.clientX, e.clientY, pageIdx,
        () => ed.chain().focus().setTextSelection(pos).run())   // clic sans glissé → replie la sélection
      return
    }

    // Bord de sélection attrapé → l'ANCRE est l'extrémité opposée (la sélection
    // s'étend/rétrécit en GLISSANT — seuil anti-tremblement : un clic sec près du
    // bord reste un clic normal qui replie la sélection) ; sinon comportement
    // normal : caret posé au clic.
    let resizeStarted = false
    const resizeSX = e.clientX, resizeSY = e.clientY
    dragAnchorRef.current = resizeAnchor ?? pos
    if (resizeAnchor == null) ed.chain().focus().setTextSelection(pos).run()
    else ed.view.focus()

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
      // Bord attrapé : n'engager le redimensionnement qu'après un VRAI mouvement
      // (sinon un clic imparfait près du bord déformait la sélection d'un chouia).
      if (resizeAnchor != null && !resizeStarted) {
        if (Math.abs(me.clientX - resizeSX) < 4 && Math.abs(me.clientY - resizeSY) < 4) return
        resizeStarted = true
      }
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
      // Clic sec près du bord (aucun glissé) → comportement de clic normal : replier
      // la sélection et poser le caret là où on a cliqué.
      if (resizeAnchor != null && !resizeStarted) { ed.chain().focus().setTextSelection(pos).run(); return }
      armBodyMiniBar()   // sélection À LA SOURIS terminée → (dés)arme la mini-barre
    }
    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [posFromEvent, scrollContainerRef, enterTextBoxEdit, exitTextBoxEdit, floatingImageAt, armBodyMiniBar, renderAllPages, updateImgSel, startShapeDraw])

  // ── Pointeur souris contextuel selon la zone survolée ───────────────────────
  // I-beam sur le texte · main sur les liens · déplacement sur les images ·
  // flèche sur les marges (hors zone de contenu). Écrit directement `style.cursor`
  // du canvas (pas de re-render React) et ne change que si la valeur diffère.
  const onPageMouseMove = useCallback((pageIdx: number, e: React.MouseEvent<HTMLCanvasElement>) => {
    const ed = editorRef.current
    const pg = pagesRef.current[pageIdx]
    const cv = e.currentTarget
    if (!ed || !pg) return
    // Outil FORME armé : le curseur de TRACÉ prime sur tout hit-test — le prochain
    // glissé dessine, il ne place ni caret ni sélection d'objet.
    if (armedShapeRef.current) {
      if (cv.style.cursor !== 'crosshair') cv.style.cursor = 'crosshair'
      return
    }
    // Glissé d'une sélection en cours → curseur « déplacement » partout, sans hit-test.
    if (textDragRef.current) {
      if (cv.style.cursor !== 'move') cv.style.cursor = 'move'
      return
    }
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
    // Survol et clic doivent passer par le MÊME test, sinon le curseur annonce
    // « texte » là où le clic sélectionnera un objet (Writer partage `PickObj`
    // entre les deux — edtwin.cxx:487,517).
    const overFloating = floatingImageAt(e.clientX, e.clientY) != null
                      || inlineImageAt(e.clientX, e.clientY) != null
    if (overFloating) {
      cursor = 'move'
    } else if (!inContent) {
      cursor = 'default'                       // marges / hors-texte → flèche
    } else {
      const pos = posFromEvent(pageIdx, e.clientX, e.clientY)
      if (pos != null) {
        const node = ed.state.doc.nodeAt(pos)
        const sel = ed.state.selection
        if (node?.type.name === 'image') cursor = 'move'                         // image → déplacement
        else if (node?.marks.some(m => m.type.name === 'link')) cursor = 'pointer' // lien → main
        // Survol de l'INTÉRIEUR de la sélection → flèche (glisser-déposer possible,
        // comme Word). NB : pas de curseur « ew-resize » sur les bords — essayé puis
        // retiré (il surgissait sans arrêt pendant la sélection, jugé envahissant) ;
        // le redimensionnement par les bords reste actif, silencieusement.
        else if (sel instanceof TextSelection && !sel.empty && pos > sel.from && pos < sel.to) cursor = 'default'
      }
    }
    if (cv.style.cursor !== cursor) cv.style.cursor = cursor

    // Chevron Développer/Réduire : afficher ▼ seulement pour le titre SURVOLÉ
    // (rangée complète du titre, marge gauche incluse) — façon Word.
    let hov: number | null = null
    const yc = y - g.marginV, xc = x - g.marginH
    if (xc >= -26) {
      for (const para of pg.layout.paragraphs) {
        if (para.table) continue
        const first = para.lines[0]; if (!first) continue
        if (yc < first.y - 2 || yc > first.y + first.height + 2) continue
        const nd = ed.state.doc.nodeAt(para.pmStart)
        if (nd && outlineLevelOf(nd) > 0) { hov = para.pmStart; break }
      }
    }
    if (hov !== hoverHeadingRef.current) {
      hoverHeadingRef.current = hov
      requestAnimationFrame(() => renderAllPages())
    }

    // Pastilles d'outils de bande : survol de la PREMIÈRE RANGÉE (outils de colonne)
    // et/ou de la PREMIÈRE COLONNE (outils de ligne), façon Google Docs. La cellule
    // (0,0) appartient aux DEUX en-têtes → elle affiche les deux pastilles.
    // Un glissé en cours fige les pastilles sur leur bande.
    if (!bandDragRef.current && !isCoarsePointer()) {
      const hitc = hitTableCell(pageIdx, e.clientX, e.clientY)
      const axes: BandAxis[] = []
      if (hitc) { if (hitc.r === 0) axes.push('col'); if (hitc.c === 0) axes.push('row') }
      if (hitc && axes.length) {
        const want = axes.map(ax => ({ ax, idx: ax === 'col' ? hitc.c : hitc.r }))
        const cur = bandBarsRef.current
        const same = cur.length === want.length && want.every(w =>
          cur.some(b => b.axis === w.ax && b.idx === w.idx && b.tableStart === hitc.tableStart && b.page === pageIdx))
        keepBand()
        if (!same) {
          const next = want.map(w => bandGeom(w.ax, pageIdx, hitc.tableStart, w.idx)).filter((b): b is BandBar => !!b)
          if (next.length) setBandBars(next)
        }
      } else if (bandBarsRef.current.length) closeBandSoon()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [posFromEvent, hitTableCell, bandGeom, keepBand, closeBandSoon])

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
  // Poignées jaunes : le glissé n'a fait qu'un APERÇU, c'est ici que la forme est
  // réellement réécrite — payload `kbshape:` et bitmap en UNE seule transaction,
  // pour qu'ils ne puissent jamais diverger (cf. documents/shapes/params.ts).
  const shapeSetAdj = (adj: number[]) => {
    const ed = editorRef.current
    const sel = ed?.state.selection
    if (!ed || !(sel instanceof NodeSelection)) return
    const p = parseShapeAlt(sel.node.attrs.alt as string)
    if (!p) return
    const next: DocShapeParams = { ...p, adj }
    const w = Number(sel.node.attrs.width) || 240, h = Number(sel.node.attrs.height) || 180
    imgUpdate({ src: shapeSrcOf(next, w, h), alt: shapeAlt(next) })
    requestAnimationFrame(() => { renderAllPages(); updateImgSel() })
  }
  // Changement d'habillage : « Aligné sur le texte » ⇄ flottant convertit le NŒUD
  // (bloc `image` ↔ `inlineImage`) pour que tous les positionnements marchent.
  const imgSetWrap = (wrap: string) => {
    const ed = editorRef.current; if (!ed) return
    const cur = selImgType()
    if (wrap === 'inline' && cur === 'image') { convertImageNode(true); return }
    if (wrap !== 'inline' && cur === 'inlineImage') { convertImageNode(false, wrap); return }
    imgUpdate({ wrap }); requestAnimationFrame(updateImgSel)
  }
  imgSetWrapRef.current = imgSetWrap
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
  // Note de bas de page en cours d'édition (pos PM de l'appel) + zones cliquables
  // du bloc de notes dessiné au bas de chaque page (pageIdx → boîtes).
  const [fnDlg, setFnDlg] = useState<{ pos: number; text: string } | null>(null)
  const fnBoxesRef = useRef(new Map<number, Array<{ x0: number; y0: number; x1: number; y1: number; pos: number }>>())
  const { openEndnote, endnoteDialog } = useEndnoteEditor(editorRef)
  // Single entry point for a click on a note call: dispatches on the node type so
  // one hit-test serves footnotes and endnotes alike.
  const openFootnoteEditor = useCallback((pos: number) => {
    const ed = editorRef.current
    const node = ed?.state.doc.nodeAt(pos)
    if (node?.type.name === 'endnote') openEndnote(pos)
    else if (node?.type.name === 'footnote') setFnDlg({ pos, text: String(node.attrs.text ?? '') })
  }, [openEndnote])
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
      wrapDistL: a.wrapDistL != null ? (a.wrapDistL as number) : WRAP_DIST_SIDE,
      wrapDistR: a.wrapDistR != null ? (a.wrapDistR as number) : WRAP_DIST_SIDE,
      align: (a.align as string) || 'left',
      posHRel: (a.posHRel as string) || 'column',
      posVRel: (a.posVRel as string) || 'paragraph',
      alignH: (a.alignH as string | null) ?? null,
      alignV: (a.alignV as string | null) ?? null,
      moveWithText: a.moveWithText !== false,
      allowOverlap: a.allowOverlap !== false,
      lockAnchor: a.lockAnchor === true,
      geom,
    })
  }
  lateOpsRef.current.openLayout = openLayoutDialog

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
    const a0 = (node0?.attrs ?? {}) as Record<string, unknown>
    // An ALIGNED object (Word `wp:align`) has no offset to start from: dragging it
    // switches it back to a manual position, as Word does, and the drag starts
    // from the offset that reproduces the alignment — so it does not jump.
    const aligned = a0.alignH != null || a0.alignV != null
    const alignOff = aligned
      ? offsetOfAlign(gRef.current, (a0.posHRel as RelH) || 'column', (a0.posVRel as RelV) || 'paragraph',
          sel.w / z, sel.h / z, (a0.alignH as AlignH) ?? undefined, (a0.alignV as AlignV) ?? undefined)
      : null
    const baseWX = alignOff && a0.alignH != null ? alignOff.x : Number(a0.wrapX) || 0
    const baseWY = alignOff && a0.alignV != null ? alignOff.y : Number(a0.wrapY) || 0
    const onMove = (me: PointerEvent) => {
      const rect = root.getBoundingClientRect()
      const sx = me.clientX - rect.left - sel.cx
      const sy = me.clientY - rect.top  - sel.cy
      if (kind === 'move') {
        // Repositionne le flottant : décalage cumulé en px doc.
        imgUpdate({
          wrapX: Math.round(baseWX + (me.clientX - startCX) / z),
          wrapY: Math.round(baseWY + (me.clientY - startCY) / z),
          ...(aligned ? { alignH: null, alignV: null } : {}),
        })
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
  // `thumb` (pastilles tactiles mobiles) : mode DIFFÉRÉ — pendant le glissé, la
  // pastille et une ligne guide pointillée suivent le doigt en temps réel,
  // VERROUILLÉES sur l'axe de redimensionnement et clampées aux limites ; le
  // tableau n'est re-layouté qu'au relâchement (un re-layout par frame saccade
  // sur mobile). Sans `thumb` (bandes souris desktop) : application en direct.
  // `alongMin/alongMax` : débattement autorisé du cercle LE LONG de la ligne
  // (px écran, relatif à sa position de départ) — il suit aussi le doigt dans
  // cette direction, sans jamais sortir du segment ; `onEnd` remonte le
  // coulissement final (l'état `along` de la ligne sélectionnée suit le doigt).
  const startTableResize = (kind: 'col' | 'colEdge' | 'row' | 'rowEdge', tableStart: number, index: number,
                            thumb?: { axis: 'x' | 'y'; guide?: { left: number; top: number; width: number; height: number }
                                      alongMin?: number; alongMax?: number; onEnd?: (alongPx: number) => void
                                      onTap?: (x: number, y: number) => void }) => (e: React.PointerEvent) => {
    e.preventDefault(); e.stopPropagation()
    const ed = editorRef.current; if (!ed) return
    const thumbEl = thumb ? (e.currentTarget as HTMLElement) : null
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
        // CLAMP (pas de retour sec) : le glissé agit jusqu'à la largeur minimale
        // de la colonne voisine — un refus silencieux donne l'impression d'une
        // poignée morte, surtout au doigt.
        const lo = -(widths[index - 1] - 30), hi = widths[index] - 30
        if (lo > hi) return   // les deux colonnes sont déjà sous le minimum
        const d2 = Math.max(lo, Math.min(hi, dx))
        const nw = widths.slice(); nw[index - 1] += d2; nw[index] -= d2
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
    // Delta CLAMPÉ le long de l'axe (px doc) — mêmes limites que apply().
    const clampD = (d: number): number => {
      if (kind === 'col') {
        const lo = -(widths[index - 1] - 30), hi = widths[index] - 30
        return lo > hi ? 0 : Math.max(lo, Math.min(hi, d))
      }
      if (kind === 'colEdge') {
        const total = widths.reduce((s, w) => s + w, 0)
        return Math.max(colCount * 30, Math.min(contentW, total + d)) - total
      }
      const ri = kind === 'rowEdge' ? heights.length - 1 : index - 1
      const base = startRH[ri] || heights[ri] || 0
      return Math.max(MIN_TBL_ROW_H, base + d) - base
    }
    // Ligne guide pointillée (mode différé) : créée au début du glissé dans le
    // même conteneur positionné que la pastille, suit le doigt avec elle.
    let guideEl: HTMLElement | null = null
    if (thumbEl && thumb?.guide) {
      guideEl = document.createElement('div')
      const gd = thumb.guide
      Object.assign(guideEl.style, {
        position: 'absolute', left: `${gd.left}px`, top: `${gd.top}px`,
        width: `${Math.max(gd.width, 0)}px`, height: `${Math.max(gd.height, 0)}px`,
        zIndex: '25', pointerEvents: 'none',
        borderLeft: thumb.axis === 'x' ? '1.5px dashed #1a73e8' : 'none',
        borderTop: thumb.axis === 'y' ? '1.5px dashed #1a73e8' : 'none',
      } as CSSStyleDeclaration)
      thumbEl.parentElement?.appendChild(guideEl)
    }
    let lastAlong = 0
    const moveThumb = (me: PointerEvent) => {
      if (!thumbEl) return
      const d = (thumb!.axis === 'x' ? me.clientX - startX : me.clientY - startY) / z
      const px = clampD(d) * z
      // Coulissement le long de la ligne : le cercle suit AUSSI le doigt dans
      // la direction de la ligne, borné au segment (jamais en dehors).
      const aRaw = thumb!.axis === 'x' ? me.clientY - startY : me.clientX - startX
      lastAlong = Math.max(thumb!.alongMin ?? 0, Math.min(thumb!.alongMax ?? 0, aRaw))
      const tr = thumb!.axis === 'x' ? `translate(${px}px, ${lastAlong}px)` : `translate(${lastAlong}px, ${px}px)`
      thumbEl.style.transform = `${tr} scale(var(--kb-pinch-inv, 1))`
      // La ligne guide, elle, ne bouge que sur l'axe de redimensionnement.
      if (guideEl) guideEl.style.transform = thumb!.axis === 'x' ? `translate(${px}px, 0)` : `translate(0, ${px}px)`
    }
    // Coalescence : TOUJOURS retenir le dernier événement (en jeter pendant un
    // rAF en attente perdait le bord de fuite du geste : sur un tableau lourd,
    // 3 pas sur 8 appliqués mesurés au banc), et flush au relâchement.
    let lastMe: PointerEvent | null = null
    // TAP-THROUGH (pastilles) : le cercle apparaît SOUS le doigt — un tap bref
    // sans mouvement dessus doit compter comme un tap sur le TEXTE dessous
    // (sinon il avale les double/triple-taps au même endroit), comme les
    // poignées de sélection.
    const tDown = Date.now()
    let movedFar = false
    const onMove = (me: PointerEvent) => {
      lastMe = me
      if (Math.abs(me.clientX - startX) > 8 || Math.abs(me.clientY - startY) > 8) movedFar = true
      if (thumbEl) { moveThumb(me); return }   // différé : aucun re-layout pendant le glissé
      if (!raf) raf = requestAnimationFrame(() => { if (lastMe) apply(lastMe) })
    }
    const onUp = () => {
      if (raf) { cancelAnimationFrame(raf); raf = 0 }
      if (guideEl) { guideEl.remove(); guideEl = null }
      if (thumbEl) thumbEl.style.transform = 'scale(var(--kb-pinch-inv, 1))'
      const tapThrough = !!thumb && !movedFar && Date.now() - tDown < 300
      if (!tapThrough && lastMe) apply(lastMe)
      if (!tapThrough) thumb?.onEnd?.(lastAlong)   // le cercle reste où le doigt l'a laissé
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('pointercancel', onUp)
      if (tapThrough) thumb?.onTap?.(startX, startY)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', onUp)
  }

  const ctxSpellRef = useRef<SpellIssue | null>(null)
  const onPageContextMenu = useCallback((pageIdx: number, e: React.MouseEvent) => {
    e.preventDefault()
    // Outil FORME armé : la page est un plan de tracé, pas une cible de menu —
    // au doigt, l'appui long du navigateur tomberait en plein glissé.
    if (armedShapeRef.current) return
    // Tactile : l'appui long SÉLECTIONNE LE MOT (cf. onPagesTouchStart) — le menu
    // contextuel synthétique du navigateur (déclenché par le même appui long)
    // marcherait dessus. Souris/stylet gardent le clic droit.
    if (typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches) return
    const ed = editorRef.current; if (!ed) return
    const sel = ed.state.selection
    // Clic droit sur une forme/image FLOTTANTE (devant le texte / carré) → la
    // sélectionner avant d'ouvrir le menu, sinon le hit-test texte sélectionnerait
    // le texte derrière et afficherait le menu du texte (pas celui de l'objet).
    const fpos = floatingImageAt(e.clientX, e.clientY, e.altKey)
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
    // Commentaire sous le clic (texte commenté) → entrées « …le commentaire ».
    ctxCommentRef.current = (pos != null && commentRangesRef.current.find(c => pos >= c.from && pos <= c.to)?.id) || null
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
      { type: 'submenu', label: t('doc_borders_menu', { defaultValue: 'Bordures' }),
        items: ([...BORDER_GRID, 'none'] as BorderPreset[]).map(p => ({
          type: 'action' as const, icon: <BorderIcon preset={p} />,
          label: t(`doc_border_${p}`, { defaultValue: BORDER_LABELS_FR[p] }),
          onClick: () => applyBorders(p),
        })) },
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

  // Entrées « …le commentaire » quand le clic droit tombe sur du texte commenté
  // (façon Word) : Modifier / Répondre / Résoudre-Rouvrir / Supprimer.
  const commentCtxItems = (): MenuItem[] => {
    const id = ctxCommentRef.current
    if (!id || !commentsMap) return []
    const th = commentsMap.get(id)
    if (!th) return []
    const mine = th.authorId === (commentUser?.id ?? '')
    const open = (mode: 'edit' | 'reply') => { cbRef.current.onCommentActivate?.(id); setGutterAction({ id, mode, ts: Date.now() }) }
    const toggleResolve = () => {
      commentsMap.set(id, { ...th, resolved: !th.resolved })
      if (!th.resolved && editorRef.current) unsetCommentMark(editorRef.current, id)
    }
    return [
      { type: 'separator' },
      ...(mine ? [{ type: 'action' as const, label: t('doc_comment_edit', { defaultValue: 'Modifier le commentaire' }), onClick: () => open('edit') }] : []),
      { type: 'action', label: t('doc_comment_reply_menu', { defaultValue: 'Répondre au commentaire' }), onClick: () => open('reply') },
      { type: 'action', label: th.resolved ? t('doc_comment_reopen_menu', { defaultValue: 'Rouvrir le commentaire' }) : t('doc_comment_resolve_menu', { defaultValue: 'Marquer comme résolu' }), onClick: toggleResolve },
      ...(mine ? [{ type: 'action' as const, label: t('doc_comment_delete_menu', { defaultValue: 'Supprimer le commentaire' }), onClick: () => { commentsMap.delete(id); if (editorRef.current) unsetCommentMark(editorRef.current, id) } }] : []),
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
      // 2b) « Grammaire… » (faute grammaticale) → panneau « Vérification » (façon Word).
      ...(spell.type === 'grammar' ? [{ type: 'action' as const, label: t('doc_grammar_panel', { defaultValue: 'Grammaire…' }), onClick: () => onOpenGrammarCheck?.(spell) }] : []),
      { type: 'separator' },
      // 3) Options d'ignorance, façon Word.
      { type: 'action', label: t('doc_spell_ignore', { defaultValue: 'Ignorer' }),
        onClick: () => { ignoreWordSession(spell.type === 'grammar' ? (spell.message === 'Majuscule en début de phrase' ? '§maj§' : '§rep§') + spell.word : spell.word); refreshSpell() } },
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
      ...commentCtxItems(),
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
      { type: 'submenu', label: t('doc_collapse_expand', { defaultValue: 'Développer/Réduire' }), disabled: !getCollapsibleHeadings(), items: [
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
          ['tight', t('doc_wrap_tight', { defaultValue: 'Rapproché' })],
          ['through', t('doc_wrap_through', { defaultValue: 'Au travers' })],
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
    // Une FORME (`kbshape:`) n'a plus de sous-menu « Couleur » ici : remplissage,
    // contour et épaisseur vivent dans l'onglet contextuel PARTAGÉ « Format de la
    // forme » (documents/shapes/ribbon.tsx), avec le vrai sélecteur de couleurs —
    // les six couples figés d'autrefois faisaient doublon et divergeaient du reste
    // de la suite.
    return items
  }

  const containerW = Math.max(g.pageW, ...geomsRef.current.map(x => x.pageW))
  return (
    <div ref={rootRef} className="relative" style={{ width: '100%', minWidth: containerW * zoom + (commentsVisible ? CARD_W + 40 : 0) }}
      onMouseMove={onRootMouseMove} onMouseLeave={() => publishMouse(null)}>
      {/* Éditeur ProseMirror caché — reçoit toute la saisie clavier. Rendu via un
          PORTAIL vers <body>, HORS du conteneur scrollé : sinon, à chaque frappe, le
          navigateur défile le plus proche ancêtre scrollable (DOM) pour amener le caret
          natif (invisible, en haut du contenu) « dans la vue » → scrollTop saute à 0 puis
          est restauré → la page sautait d'une ligne à chaque Espace. Hors du conteneur,
          ce défilement natif ne touche plus le document. */}
      {/* left: -10000 (et PAS 0) : l'overlay de sélection NATIF d'iOS/Android
          (poignées bleues + menu contextuel du navigateur) se dessine aux rects
          de sélection du contenu caché en IGNORANT overflow/opacity — visible au
          bord gauche de l'écran sinon. Hors viewport, il n'apparaît jamais ;
          `position: fixed` → aucun défilement induit par le focus/la frappe. */}
      {createPortal(
        <div style={{ position: 'fixed', width: 1, height: 1, overflow: 'hidden', opacity: 0, top: 0, left: -10000, pointerEvents: 'none', zIndex: -1,
                      caretColor: 'transparent', WebkitTouchCallout: 'none' }}>
          <EditorContent editor={editor} />
        </div>,
        document.body,
      )}

      {/* Pages canvas (rendu pur du modèle unique) — taille propre à chaque section.
          `safe center` : en cas de DÉBORDEMENT horizontal (fort zoom), le centrage
          bascule en alignement au début — sinon les pages débordent à gauche de
          façon inaccessible au défilement et les offsets des règles sont faussés. */}
      {/* commentsVisible : sort du multipage (colonne unique) + réserve une
          gouttière à droite pour les cartes de commentaires (façon Word). */}
      <div className="relative flex content-start"
        onTouchStart={onPagesTouchStart} onTouchMove={onPagesTouchMove}
        onTouchEnd={onPagesTouchEnd} onTouchCancel={cancelLongPress}
        style={{ flexWrap: commentsVisible ? 'nowrap' : 'wrap', flexDirection: commentsVisible ? 'column' : 'row', alignItems: commentsVisible ? 'center' : undefined, justifyContent: 'safe center', paddingTop: CANVAS_PAD_Y, paddingRight: commentsVisible ? (CARD_W + 40) : 0, rowGap: PAGE_GAP, columnGap: 24 }}>
        {pages.map((pg, idx) => {
          const geom = geomsRef.current[pg.secIdx] || geomsRef.current[0] || g
          // CSS size derived from the BACKING size (quantized to whole device
          // pixels): pageW*zoom and round(pageW*zoom*dpr) always differ a little
          // (e.g. 317.59 CSS vs 318px backing at 40%), so the compositor was
          // resampling every page by a fraction of a percent — smearing the ink.
          const cssW = Math.round(geom.pageW * zoom * dpr) / dpr
          const cssH = Math.round(geom.pageH * zoom * dpr) / dpr
          // Context attributes (alpha) are fixed at first getContext → remount
          // the canvas if a page switches solid ↔ gradient background.
          const opaque = solidPageBg(pg.secIdx) != null
          return (
            <canvas
              key={`${idx}:${opaque ? 'o' : 't'}`}
              ref={el => { if (el) canvasRefs.current.set(idx, el); else canvasRefs.current.delete(idx) }}
              onMouseDown={e => onPageMouseDown(idx, e)}
              onPointerDown={e => onPagePointerDown(idx, e)}
              onMouseMove={e => onPageMouseMove(idx, e)}
              onMouseLeave={() => { if (hoverHeadingRef.current != null) { hoverHeadingRef.current = null; requestAnimationFrame(() => renderAllPages()) } }}
              onContextMenu={e => onPageContextMenu(idx, e)}
              className="block bg-white shadow-sm"
              // Outil forme armé → curseur de TRACÉ (façon Word/LibreOffice) : il
              // annonce que le prochain glissé dessine, il ne sélectionne pas.
              // `touchAction: none` avec l'outil armé : au doigt, le glissé DESSINE
              // (le navigateur ne doit ni faire défiler, ni ouvrir son menu d'appui
              // long). Le pincement, lui, reste géré en JS par le conteneur défilant.
              style={{ width: cssW, height: cssH, flex: '0 0 auto', cursor: armedShape ? 'crosshair' : (paintMode ? PAINT_CURSOR : 'text'),
                       touchAction: armedShape ? 'none' : undefined,
                       background: secMetaRef.current[pg.secIdx]?.pageColor ?? (pageBg || undefined) }}
            />
          )
        })}
      </div>

      {/* ── Aperçu LIVE de la forme en cours de tracé ──────────────────────────
          UN calque par page : la VRAIE géométrie grandit sous le curseur (moteur
          partagé paintShapeGhost) et chaque page peint la portion qui lui revient,
          si bien qu'un tracé qui déborde de la feuille de départ — ou qui en
          chevauche deux — reste visible d'un bout à l'autre. Un calque que le
          tracé n'atteint jamais n'alloue aucun pixel (cf. ShapeGhostLayer) et
          rien ne passe par un état React pendant le glissé. */}
      {drawPage && pages.map((pg, idx) => {
        const geom = geomsRef.current[pg.secIdx] || geomsRef.current[0] || g
        const { left, top } = pageOrigin(idx)
        return (
          <ShapeGhostLayer
            key={`ghost-${idx}`}
            ref={el => { ghostRefs.current[idx] = el }}
            left={left} top={top}
            width={Math.round(geom.pageW * zoom * dpr) / dpr}
            height={Math.round(geom.pageH * zoom * dpr) / dpr}
            // Débord = la MOITIÉ de la gouttière qui sépare deux pages : les
            // débords se jointoient sans se recouvrir (le remplissage translucide
            // ne se peint donc jamais deux fois au même endroit).
            padX={12} padY={PAGE_GAP / 2}
            zoom={zoom} dpr={dpr} kind={drawPage.kind}
          />
        )
      })}

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
              // Cellule scindée sur plusieurs pages : la surbrillance est BORNÉE
              // à la bande de contenu (sinon elle couvre marges/en-tête/pied).
              const cyT = Math.max(cell.y, 0), cyB = Math.min(cell.y + cell.h, geom.contentH)
              if (cyB - cyT < 1) continue
              cells.push(<div key={`ts${idx}-${cell.r}-${cell.c}`} style={{
                position: 'absolute', pointerEvents: 'none', zIndex: 22,
                left: left + (geom.marginH + cell.x) * z, top: pageTop + (geom.marginV + cyT) * z,
                width: cell.w * z, height: (cyB - cyT) * z,
                background: 'rgba(87,133,253,0.28)', border: '1px solid rgba(26,115,232,0.6)',
              }} />)
            }
          }
        })
        return <>{cells}</>
      })()}

      {/* ── Poignées de redimensionnement des tableaux (colonnes / lignes / bord) ──
          Bandes fines sur les bordures : glisser pour redimensionner. Visibles au
          survol (filet bleu). Au-dessus du canvas, pointer-events sur la bande.
          SOURIS UNIQUEMENT : sur tactile elles volent les taps (7px invisibles
          par-dessus le canvas) et le :hover reste collé après un toucher — le
          mobile a ses pastilles sur la cellule active (bloc suivant). */}
      {!hfEdit && !(typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches) && (() => {
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
            // Tableau scindé : le fragment garde colX/rowY COMPLETS (décalés) —
            // borner les bandes au fragment VISIBLE (sinon elles débordent sur
            // les pages voisines, par-dessus leur contenu).
            const y0 = Math.max(tb.rowY[0], 0), y1 = Math.min(tb.rowY[tb.rowY.length - 1], geom.contentH)
            if (y1 - y0 < 4) continue
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
              if (cy < -1 || cy > geom.contentH + 1) return   // bordure sur une autre page
              const isEdge = ri === tb.rowY!.length - 1
              handles.push(<div key={`rz${idx}-${ts}-${ri}`} className="kb-tbl-rz kb-tbl-rz-h"
                onPointerDown={startTableResize(isEdge ? 'rowEdge' : 'row', ts, ri)}
                style={{ left: ox + x0 * z, top: oy + cy * z - HW / 2, width: (x1 - x0) * z, height: HW, pointerEvents: 'auto' }} />)
            })
          }
        })
        return <>{handles}</>
      })()}

      {/* ── Contrôle TACTILE de redimensionnement sur la LIGNE de bordure
          SÉLECTIONNÉE (mobile). Un tap sur une bordure de tableau la sélectionne
          (handleTapAt → tableBorderAt) : elle est SURLIGNÉE sur tous ses
          fragments visibles et porte un CERCLE BLEU PLEIN (même langage visuel
          que les poignées de sélection) à glisser — suivi temps réel verrouillé
          sur son axe + guide pointillé, application au relâchement (mode différé
          de startTableResize). Taille constante pendant le pincement
          (--kb-pinch-inv) ; clé STATIQUE (l'élément survit aux re-rendus de
          mi-glissé : la capture tactile implicite meurt avec l'élément). */}
      {!hfEdit && touchTblLine && editorRef.current?.isEditable && (() => {
        const z = zoom, Lk = touchTblLine
        const isCol = Lk.kind === 'col' || Lk.kind === 'colEdge'
        const segs: Array<{ left: number; top: number; width: number; height: number; page: number; base: number }> = []
        pages.forEach((pg, idx) => {
          const geom = geomOf(pg)
          const { left, top: pageTop } = pageOrigin(idx)
          for (const para of pg.layout.paragraphs) {
            const tb = para.table
            if (!tb || para.pmStart !== Lk.pos || !tb.colX || !tb.rowY) continue
            const ox = left + geom.marginH * z, oy = pageTop + geom.marginV * z
            if (isCol) {
              const cx = tb.colX[Lk.index]
              if (cx == null) continue
              const yTop = Math.max(tb.rowY[0], 0), yBot = Math.min(tb.rowY[tb.rowY.length - 1], geom.contentH)
              if (yBot - yTop < 4) continue
              segs.push({ left: ox + cx * z, top: oy + yTop * z, width: 0, height: (yBot - yTop) * z, page: idx, base: oy })
            } else {
              const cy = tb.rowY[Lk.index]
              // -8 : une scission à la couture des pages laisse la bordure à
              // ~-2px du haut du fragment suivant.
              if (cy == null || cy < -8 || cy > geom.contentH + 1) continue
              segs.push({ left: ox + tb.colX[0] * z, top: oy + Math.max(cy, 0) * z,
                          width: (tb.colX[tb.colX.length - 1] - tb.colX[0]) * z, height: 0, page: idx, base: ox })
            }
          }
        })
        if (!segs.length) return null
        // Le cercle apparaît SOUS le doigt : au point d'accroche (`along`, sur
        // le fragment touché), projeté sur la ligne et borné à son segment.
        const main = segs.find(s => s.page === Lk.page) ?? segs[0]
        const M = 14   // marge : le cercle ne dépasse pas les bouts du segment
        const gx = isCol ? main.left
          : Math.max(main.left + M, Math.min(main.left + main.width - M, main.base + Lk.along * z))
        const gy = isCol ? Math.max(main.top + M, Math.min(main.top + main.height - M, main.base + Lk.along * z))
          : main.top
        // Débattement du coulissement le long de la ligne pendant le glissé.
        const alongMin = (isCol ? main.top + M - gy : main.left + M - gx)
        const alongMax = (isCol ? main.top + main.height - M - gy : main.left + main.width - M - gx)
        const arrow = isCol
          ? <path d="M1 6h10M1 6l2.2-2.2M1 6l2.2 2.2M11 6L8.8 3.8M11 6l-2.2 2.2" stroke="#fff" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
          : <path d="M6 1v10M6 1L3.8 3.2M6 1l2.2 2.2M6 11l-2.2-2.2M6 11l2.2-2.2" stroke="#fff" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        return (
          <>
            {segs.map((s, i) => (
              <div key={`tbl-line-${i}`} style={{ position: 'absolute', zIndex: 25, pointerEvents: 'none',
                left: s.left - (isCol ? 1 : 0), top: s.top - (isCol ? 0 : 1),
                width: isCol ? 2 : s.width, height: isCol ? s.height : 2, background: '#1a73e8' }} />
            ))}
            <div key="tbl-line-grip"
              onPointerDown={startTableResize(Lk.kind, Lk.pos, Lk.index, {
                axis: isCol ? 'x' : 'y', guide: main, alongMin, alongMax,
                onEnd: (alongPx) => { if (alongPx) setTouchTblLine(l => l ? { ...l, along: l.along + alongPx / z } : l) },
                // Tap bref sur le cercle = tap sur le texte dessous (le cercle
                // est sous le doigt : sans cela il avale les double/triple-taps).
                onTap: (x, y) => handleTapAt(x, y),
              })}
              onTouchStart={e => e.stopPropagation()}
              style={{ position: 'absolute', zIndex: 26, pointerEvents: 'auto', touchAction: 'none',
                       left: gx - 17, top: gy - 17, width: 34, height: 34,
                       display: 'flex', alignItems: 'center', justifyContent: 'center',
                       transform: 'scale(var(--kb-pinch-inv, 1))', transformOrigin: '17px 17px' }}>
              <div style={{ width: 26, height: 26, borderRadius: 13, background: '#1a73e8',
                            boxShadow: '0 1px 4px rgba(0,0,0,.35)',
                            display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                <svg width="14" height="14" viewBox="0 0 12 12" style={{ display: 'block' }}>{arrow}</svg>
              </div>
            </div>
          </>
        )
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

      {/* Contrôle de contenu de la table des matières : cadre + petite barre. */}
      {tocControl && tocRect && (
        <TocControl rect={tocRect} t={t}
          onPreset={tocControl.onPreset} onRemove={tocControl.onRemove} onUpdate={tocControl.onUpdate} />
      )}

      {/* Marge de commentaires ancrée (cartes à droite de chaque page). */}
      {commentsVisible && commentsMap && commentUser && (
        <CommentGutter
          commentsMap={commentsMap}
          editor={editorRef.current}
          user={commentUser}
          activeId={activeCommentId}
          setActiveId={id => cbRef.current.onCommentActivate?.(id)}
          anchoredIds={commentRangesRef.current.map(c => c.id)}
          anchorScreen={commentAnchorScreen}
          tick={commentTick}
          action={gutterAction}
          onConsumeAction={() => setGutterAction(null)}
        />
      )}

      {/* Caret unique, positionné sur la bonne page — 1 px, comme Word */}
      <div ref={caretRef}
        className="docs-text-ui-cursor-blink"
        style={{ position: 'absolute', width: 1, background: '#202124', display: 'none', pointerEvents: 'none' }} />

      {/* Caret de DÉPÔT (glisser-déposer de la sélection) : suit la souris pendant le glissé */}
      <div ref={dropCaretRef}
        style={{ position: 'absolute', width: 1, background: '#202124', display: 'none', pointerEvents: 'none', zIndex: 20 }} />

      {/* Curseurs des autres participants (présence collaborative) */}
      {remoteCursors.map(c => (
        <div key={c.clientId} className="kb-collab-indicator"
          style={{ position: 'absolute', left: c.left, top: c.top, height: c.height,
                   width: 2, background: c.color, pointerEvents: 'none', zIndex: 20 }}>
          <div style={{ position: 'absolute', top: -16, left: -1, background: c.color, color: '#fff',
                        fontSize: 10, lineHeight: '14px', padding: '0 4px', borderRadius: 3,
                        whiteSpace: 'nowrap', fontWeight: 600,
                        // Taille constante pendant le pincement (le caret, lui, suit le texte).
                        transform: 'scale(var(--kb-pinch-inv, 1))', transformOrigin: 'left bottom' }}>
            {c.name}
          </div>
        </div>
      ))}

      {/* Poignées de sélection tactiles : GOUTTE D'EAU symétrique (pointe vers
          le HAUT au ras de la sélection, corps rond percé d'un trou blanc —
          référence visuelle fournie par l'utilisateur). Carré 22px arrondi sur
          3 coins pivoté à 45° = la pointe vise pile l'extrémité de sélection ;
          zone de saisie 34×46. Les deux poignées sont identiques. */}
      {selHandles && ([['from', selHandles.from], ['to', selHandles.to]] as const).map(([edge, p]) => (
        <div key={edge} onPointerDown={selHandleDrag(edge)}
          style={{ position: 'absolute', left: p.left - 22, top: p.top + p.height - 2, width: 44, height: 56,
                   zIndex: 25, touchAction: 'none', cursor: 'grab',
                   // Taille constante pendant le pincement (contre-échelle, ancrée à la pointe).
                   transform: 'scale(var(--kb-pinch-inv, 1))', transformOrigin: '22px 0' }}>
          {/* Goutte SVG ALLONGÉE VERTICALEMENT (26×38, ratio ~2:3 de la capture
              de référence) : pointe fine en haut au ras de la sélection, bulbe
              rond percé d'un trou blanc. */}
          <svg width={26} height={38} viewBox="0 0 26 38"
            style={{ position: 'absolute', top: 4, left: '50%', transform: 'translateX(-50%)',
                     filter: 'drop-shadow(0 1px 3px rgba(0,0,0,0.3))' }}>
            <path d="M13 0 C9 8 0 16 0 25 a13 13 0 1 0 26 0 C26 16 17 8 13 0 Z" fill="#1a73e8" />
            <circle cx="13" cy="25" r="5.5" fill="#fff" />
          </svg>
        </div>
      ))}

      {/* Pastilles d'outils de BANDE au survol des en-têtes (façon Google Docs).
          Rangée 0 → outils de colonne (déplacer · trier · insérer à droite) ;
          colonne 0 → outils de ligne (déplacer · épingler l'en-tête · insérer dessous). */}
      {!hfEdit && bandBars.map(bar => {
        const isCol = bar.axis === 'col'
        const btn = 'flex items-center justify-center w-[22px] h-[22px] rounded hover:bg-black/[0.06] active:bg-black/10 text-text-secondary'
        // Épinglage : nombre de rangées épinglées du tableau SURVOLÉ (pour la bascule).
        const pinned = Number(hoveredTableAttrs(bar.tableStart).headerRows) || 0
        const wantPin = bar.idx + bar.span
        // Pendant le glissé, la pastille de la bande soulevée suit son fantôme.
        const d = bandDrag && bandDrag.axis === bar.axis && bandDrag.from === bar.idx && bandDrag.tableStart === bar.tableStart
          ? (bandDrag.ghost - bandDrag.origin) * zoom : 0
        return (
          <div key={`${bar.axis}:${bar.tableStart}:${bar.idx}`}
            onMouseEnter={keepBand}
            onMouseLeave={closeBandSoon}
            style={{ position: 'absolute', left: bar.left + (isCol ? d : 0), top: bar.top + (isCol ? 0 : d),
                     transform: isCol ? 'translateX(-50%)' : 'translate(-100%, -50%)',
                     zIndex: 26, display: 'flex', alignItems: 'center', height: COLBAR_H,
                     background: '#fff', border: '1px solid #dadce0', borderRadius: 6,
                     boxShadow: '0 1px 3px rgba(60,64,67,0.24)', padding: '0 2px', gap: 1 }}>
            {/* Poignée : glisser pour déplacer la colonne / la ligne. */}
            <button type="button" className={btn} style={{ cursor: 'grab' }}
              aria-label={isCol ? t('doc_col_move', { defaultValue: 'Déplacer la colonne' }) : t('doc_row_move', { defaultValue: 'Déplacer la ligne' })}
              title={isCol ? t('doc_col_move', { defaultValue: 'Déplacer la colonne' }) : t('doc_row_move', { defaultValue: 'Déplacer la ligne' })}
              onMouseDown={e => e.preventDefault()}
              onPointerDown={e => bandDragStart(e, bar)}>
              <GripHorizontal size={14} />
            </button>
            {isCol ? (
              /* Tri de la colonne survolée. */
              <button type="button" className={btn}
                aria-label={t('doc_col_sort', { defaultValue: 'Trier' })}
                title={t('doc_col_sort', { defaultValue: 'Trier' })}
                onMouseDown={e => e.preventDefault()}
                onClick={e => { const r = e.currentTarget.getBoundingClientRect(); setBandSortMenu({ top: r.bottom + 4, left: r.left, tableStart: bar.tableStart, col: bar.idx }) }}>
                <ArrowDownWideNarrow size={14} />
              </button>
            ) : (
              /* Épingler l'en-tête jusqu'à cette ligne (bascule). */
              <button type="button" className={btn}
                style={{ color: pinned === wantPin ? 'var(--kbn-accent, #1a73e8)' : undefined }}
                aria-label={t('doc_row_pin', { defaultValue: 'Épingler l’en-tête jusqu’à cette ligne' })}
                title={t('doc_row_pin', { defaultValue: 'Épingler l’en-tête jusqu’à cette ligne' })}
                onMouseDown={e => e.preventDefault()}
                onClick={() => { const e2 = focusTableAt(bar.tableStart); if (e2) setHeaderRows(e2, pinned === wantPin ? 0 : wantPin) }}>
                <Pin size={14} />
              </button>
            )}
            {/* Insertion d'une colonne à DROITE / d'une ligne EN DESSOUS. */}
            <button type="button" className={btn}
              aria-label={isCol ? t('doc_col_insert_right', { defaultValue: 'Insérer une colonne à droite' }) : t('doc_row_insert_below', { defaultValue: 'Insérer une ligne en dessous' })}
              title={isCol ? t('doc_col_insert_right', { defaultValue: 'Insérer une colonne à droite' }) : t('doc_row_insert_below', { defaultValue: 'Insérer une ligne en dessous' })}
              onMouseDown={e => e.preventDefault()}
              onClick={() => {
                const e2 = focusTableAt(bar.tableStart); if (!e2) return
                const at = bar.idx + bar.span
                if (isCol) insertColAt(e2, at); else insertRowAt(e2, at)
                setBandBars([])
              }}>
              <Plus size={14} />
            </button>
          </div>
        )
      })}

      {/* Menu de tri de la colonne survolée. */}
      {bandSortMenu && (
        <MenuDropdown
          pos={{ top: bandSortMenu.top, left: bandSortMenu.left, minWidth: 210 }}
          onClose={() => setBandSortMenu(null)}
          items={(['asc', 'desc'] as const).map(dir => ({
            type: 'action' as const,
            label: dir === 'asc'
              ? t('doc_col_sort_asc', { defaultValue: 'Trier par ordre croissant' })
              : t('doc_col_sort_desc', { defaultValue: 'Trier par ordre décroissant' }),
            onClick: () => {
              const e2 = focusTableAt(bandSortMenu.tableStart)
              if (e2) sortTableRows(e2, bandSortMenu.col, dir)
              setBandBars([])
            },
          }))}
        />
      )}

      {/* Glissé de bande : FANTÔME teinté qui suit le pointeur en continu (position
          libre, il ne saute pas de bande en bande) + BARRE d'insertion alignée sur la
          frontière la plus proche du CENTRE du fantôme = point de chute réel. */}
      {bandDrag && (() => {
        const pg = pagesRef.current[bandDrag.page]; if (!pg) return null
        const geom = geomOf(pg)
        const { left, top: pageTop } = pageOrigin(bandDrag.page)
        const z = zoom
        const para = pg.layout.paragraphs.find(p => p.table && p.pmStart === bandDrag.tableStart)
        const tb = para?.table
        if (!tb?.colX || !tb.rowY) return null
        const ox = left + geom.marginH * z, oy = pageTop + geom.marginV * z
        const isCol = bandDrag.axis === 'col'
        const bounds = isCol ? tb.colX : tb.rowY
        const xL = tb.colX[0], xR = tb.colX[tb.colX.length - 1]
        const yT = Math.max(tb.rowY[0], 0), yB = Math.min(tb.rowY[tb.rowY.length - 1], geom.contentH)
        const line = bounds[bandDrag.boundary]
        if (line == null) return null
        const g0 = bandDrag.ghost, g1 = bandDrag.ghost + bandDrag.size
        const TINT = 'rgba(26,115,232,0.16)'
        return (
          <>
            <div style={{ position: 'absolute', zIndex: 25, pointerEvents: 'none', background: TINT,
                          left: ox + (isCol ? g0 : xL) * z, top: oy + (isCol ? yT : g0) * z,
                          width: (isCol ? g1 - g0 : xR - xL) * z, height: (isCol ? yB - yT : g1 - g0) * z }} />
            <div style={{ position: 'absolute', zIndex: 27, pointerEvents: 'none', background: '#1a73e8',
                          left: ox + (isCol ? line : xL) * z - (isCol ? 1.5 : 0),
                          top: oy + (isCol ? yT : line) * z - (isCol ? 0 : 1.5),
                          width: isCol ? 3 : (xR - xL) * z, height: isCol ? (yB - yT) * z : 3 }} />
          </>
        )
      })()}

      {/* Poignées du tableau en cours d'édition (façon Word) : déplacement au coin
          haut-gauche, redimensionnement au coin bas-droit. */}
      {tableHandles && !hfEdit && (
        <>
          <button type="button"
            aria-label={t('doc_table_move_handle', { defaultValue: 'Déplacer le tableau (clic : sélectionner le tableau)' })}
            title={t('doc_table_move_handle', { defaultValue: 'Déplacer le tableau (clic : sélectionner le tableau)' })}
            onMouseDown={e => e.preventDefault()}
            onPointerDown={e => tableMoveDrag(e, tableHandles.tableStart)}
            style={{ position: 'absolute', left: tableHandles.move.left, top: tableHandles.move.top,
                     width: TBL_HANDLE, height: TBL_HANDLE, zIndex: 24, padding: 0, lineHeight: 0,
                     display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'move',
                     background: '#fff', border: '1px solid #9aa0a6', color: '#3c4043' }}>
            <Move size={11} />
          </button>
          <div
            role="button"
            aria-label={t('doc_table_resize_handle', { defaultValue: 'Redimensionner le tableau' })}
            title={t('doc_table_resize_handle', { defaultValue: 'Redimensionner le tableau' })}
            onMouseDown={e => e.preventDefault()}
            onPointerDown={e => tableSizeDrag(e, tableHandles.tableStart)}
            style={{ position: 'absolute', left: tableHandles.size.left, top: tableHandles.size.top,
                     width: TBL_SIZER, height: TBL_SIZER, zIndex: 24, cursor: 'nwse-resize',
                     background: '#fff', border: '1px solid #9aa0a6' }} />
        </>
      )}

      {/* Poignée « bordures » de la cellule courante : ouvre la galerie de bordures. */}
      {cellBorderBtn && !hfEdit && (
        <button
          type="button"
          aria-label={t('doc_cell_borders', { defaultValue: 'Bordures de la cellule' })}
          onMouseDown={e => { e.preventDefault(); e.stopPropagation() }}
          onClick={e => { e.stopPropagation(); openBorderGallery(e.currentTarget.getBoundingClientRect()) }}
          style={{ position: 'absolute', left: cellBorderBtn.left, top: cellBorderBtn.top,
                   width: CELL_BTN, height: CELL_BTN, zIndex: 24, display: 'flex',
                   alignItems: 'center', justifyContent: 'center', cursor: 'pointer',
                   background: '#f1f3f4', border: '1px solid #9aa0a6', borderRadius: 2,
                   color: '#3c4043', padding: 0, lineHeight: 0 }}>
          <svg width="7" height="5" viewBox="0 0 7 5" aria-hidden="true">
            <path d="M0 0 L7 0 L3.5 5 Z" fill="currentColor" />
          </svg>
        </button>
      )}

      {borderGalleryPos && (
        <MenuDropdown
          pos={borderGalleryPos}
          minWidth={124}
          onClose={() => setBorderGalleryPos(null)}
          items={[{ type: 'custom', render: close => (
            <BorderGallery
              onPick={p => { applyBorders(p); close() }}
              label={p => t(`doc_border_${p}`, { defaultValue: BORDER_LABELS_FR[p] })}
            />
          ) }]}
        />
      )}

      {/* Poignée d'INSERTION : goutte UNIQUE sous le caret après un tap (façon
          Word Android). Glisser = placer le caret précisément (loupe) ; tap =
          bascule du menu Coller/Tout sélectionner. Éphémère (~4s). */}
      {caretHandle && !selHandles && (
        <div onPointerDown={caretHandleDrag}
          style={{ position: 'absolute', left: caretHandle.left - 22, top: caretHandle.top + caretHandle.height - 2, width: 44, height: 56,
                   zIndex: 25, touchAction: 'none', cursor: 'grab',
                   transform: 'scale(var(--kb-pinch-inv, 1))', transformOrigin: '22px 0' }}>
          <svg width={26} height={38} viewBox="0 0 26 38"
            style={{ position: 'absolute', top: 4, left: '50%', transform: 'translateX(-50%)',
                     filter: 'drop-shadow(0 1px 3px rgba(0,0,0,0.3))' }}>
            <path d="M13 0 C9 8 0 16 0 25 a13 13 0 1 0 26 0 C26 16 17 8 13 0 Z" fill="#1a73e8" />
            <circle cx="13" cy="25" r="5.5" fill="#fff" />
          </svg>
        </div>
      )}

      {/* Loupe tactile (médaillon grossi au-dessus du doigt, cf. showLoupe) */}
      {createPortal(
        <div ref={loupeRef}
          style={{ display: 'none', position: 'fixed', width: LOUPE_W, height: LOUPE_H, zIndex: 70,
                   borderRadius: 12, overflow: 'hidden', border: '1px solid rgba(0,0,0,0.15)',
                   boxShadow: '0 4px 16px rgba(0,0,0,0.28)', background: '#fff', pointerEvents: 'none' }}>
          <canvas ref={loupeCvRef} style={{ width: '100%', height: '100%', display: 'block' }} />
        </div>,
        document.body,
      )}

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
        <TablePropertiesDialog editor={editorRef.current} rect={tableDlg.rect} onClose={() => setTableDlg(null)}
          pageBorder={pageBorder} onPageBorderChange={onPageBorder} />
      )}

      {/* Dialogue « Paragraphe… » (Retrait et espacement / Enchaînements) */}
      {paraDlg && (
        <ParagraphDialog init={paraDlg} onApply={applyParagraphDraft} onClose={() => setParaDlg(null)} />
      )}

      {/* Éditeur de note de bas de page (clic sur l'appel ou sur la note) */}
      {fnDlg && (
        <FloatingWindow title={t('doc_footnote', { defaultValue: 'Note de bas de page' })} onClose={() => setFnDlg(null)} defaultWidth={440} backdrop>
          <div className="p-4 flex flex-col gap-3" data-module="office">
            <textarea
              autoFocus
              className="w-full h-28 border border-border rounded-lg p-2 text-sm resize-none focus:outline-none focus:border-accent bg-surface text-text-primary"
              placeholder={t('doc_footnote_placeholder', { defaultValue: 'Texte de la note…' })}
              value={fnDlg.text}
              onChange={e => setFnDlg({ ...fnDlg, text: e.target.value })}
            />
            <div className="flex justify-between">
              <Button variant="secondary" onClick={() => {
                const ed = editorRef.current
                if (ed && ed.state.doc.nodeAt(fnDlg.pos)?.type.name === 'footnote') {
                  ed.view.dispatch(ed.state.tr.delete(fnDlg.pos, fnDlg.pos + 1))
                }
                setFnDlg(null)
              }}>{t('doc_footnote_delete', { defaultValue: 'Supprimer la note' })}</Button>
              <div className="flex gap-2">
                <Button className={DLG_BTN} onClick={() => {
                  const ed = editorRef.current
                  const node = ed?.state.doc.nodeAt(fnDlg.pos)
                  if (ed && node?.type.name === 'footnote') {
                    ed.view.dispatch(ed.state.tr.setNodeMarkup(fnDlg.pos, undefined, { text: fnDlg.text }))
                  }
                  setFnDlg(null)
                }}>{t('common_ok', { defaultValue: 'OK' })}</Button>
                <Button className={DLG_BTN} variant="secondary" onClick={() => setFnDlg(null)}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
              </div>
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* Éditeur de note de fin (même déclencheurs que la note de bas de page) */}
      {endnoteDialog}

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
        // Aperçu de la ZONE D'HABILLAGE (modes avec habillage) : boîte pointillée =
        // empreinte que le texte contourne (image + distances objet↔texte).
        const wrapMode = imgSel.wrap
        const isWrapping = wrapMode === 'square' || wrapMode === 'tight' || wrapMode === 'through' || wrapMode === 'topBottom'
        const wrapPreview = (() => {
          if (!isWrapping) return null
          const nd = editorRef.current?.state.doc.nodeAt(imgSel.pos)
          const a = (nd?.attrs ?? {}) as Record<string, number>
          const z = zoomRef.current
          const dL = (a.wrapDistL ?? 10) * z, dR = (a.wrapDistR ?? 10) * z
          const dT = (a.wrapDistT ?? 0) * z,  dB = (a.wrapDistB ?? 0) * z
          const full = wrapMode === 'topBottom'
          // Objet tourné : l'empreinte habillée est sa boîte englobante (AABB).
          const rad = ((imgSel.rotation || 0) * Math.PI) / 180
          const aw = Math.abs(imgSel.w * Math.cos(rad)) + Math.abs(imgSel.h * Math.sin(rad))
          const ah = Math.abs(imgSel.w * Math.sin(rad)) + Math.abs(imgSel.h * Math.cos(rad))
          return (
            <div style={{
              position: 'absolute',
              left: full ? 8 : imgSel.cx - aw / 2 - dL,
              top: imgSel.cy - ah / 2 - dT,
              width: full ? 'calc(100% - 16px)' : aw + dL + dR,
              height: ah + dT + dB,
              zIndex: 29, pointerEvents: 'none',
              border: '1.5px dashed #9334e6', borderRadius: 2, opacity: 0.8,
            }} />
          )
        })()
        return (
          <>
            {wrapPreview}
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
              {(imgSel.wrap === 'behind' || imgSel.wrap === 'front' || isWrapping) && (
                <div onPointerDown={startHandleDrag('move')}
                  onContextMenu={e => { e.preventDefault(); e.stopPropagation(); ctxSpellRef.current = null; setCtxMenu({ x: e.clientX, y: e.clientY }) }}
                  // touchAction none : au doigt, le glissé DÉPLACE l'objet (guide
                  // tactile Word) au lieu de faire défiler la page.
                  style={{ position: 'absolute', inset: 6, pointerEvents: 'auto', cursor: 'move', touchAction: 'none' }} />
              )}
              {HANDLES.map(h => {
                const corner = h.type === 'corner'
                // Pointeur GROSSIER : pastilles plus grosses ET cible de saisie
                // élargie INVISIBLEMENT (enfant en débord) — 10 px d'encre ne
                // s'attrapent pas au doigt, et grossir l'encre à 40 px masquerait
                // l'objet. Même dispositif que l'éditeur de présentations.
                const coarseP = isCoarsePointer()
                const hw = corner ? (coarseP ? 15 : 10) : h.type === 'h' ? (coarseP ? 22 : 16) : (coarseP ? 10 : 7)
                const hh = corner ? (coarseP ? 15 : 10) : h.type === 'h' ? (coarseP ? 10 : 7) : (coarseP ? 22 : 16)
                const grab = coarseP ? 9 : 0
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
                      pointerEvents: 'auto', cursor: h.cur, touchAction: 'none',
                    }}>
                    {grab > 0 && <span style={{ position: 'absolute', inset: -grab, borderRadius: 999 }} />}
                  </div>
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
                  touchAction: 'none',
                  display: 'flex', alignItems: 'center', justifyContent: 'center', color: HANDLE_BLUE,
                }}>
                <RotateCw size={13} style={{ pointerEvents: 'none' }} />
              </div>
              {/* Poignées d'AJUSTEMENT (pastilles jaunes) — moteur partagé
                  shapes/adjust, comme le tableur et les présentations. Elles
                  vivent DANS la boîte tournée : sur une forme pivotée, elles
                  restent collées à la géométrie. */}
              {imgSel.kind && (
                <ShapeAdjustHandles
                  kind={imgSel.kind}
                  adj={imgSel.adj}
                  w={imgSel.w} h={imgSel.h} rotation={imgSel.rotation}
                  centre={() => {
                    const r = rootRef.current?.getBoundingClientRect()
                    return r ? { x: r.left + imgSel.cx, y: r.top + imgSel.cy } : null
                  }}
                  onCommit={shapeSetAdj}
                  coarse={isCoarsePointer()}
                  label={t('sheet_shape_adjust', { defaultValue: 'Ajuster la forme' })}
                />
              )}
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
                moveWithText={imgSel.moveWithText}
                left={imgSel.cx + imgSel.w / 2 + 40}
                top={imgSel.cy - imgSel.h / 2}
                onChange={w => imgSetWrap(w)}
                onMoveMode={v => { imgUpdate({ moveWithText: v }); requestAnimationFrame(updateImgSel) }}
                onMore={() => { setWrapPanel(false); requestAnimationFrame(openLayoutDialog) }}
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
      {bodyMiniBar && editorRef.current && <FormattingMiniBar editor={editorRef.current} left={bodyMiniBar.left} top={bodyMiniBar.top}
        caretMode={bodyMiniBar.caret}
        rootRef={(el) => { barElRef.current = el }} onAddComment={() => cbRef.current.onAddComment?.()}
        onDismiss={() => { miniBarDismissedRef.current = true; setBodyMiniBar(null) }}
        onSelectAll={() => {
          // Tout sélectionner → le menu de SÉLECTION prend le relais (iOS/Android :
          // après Select All le menu réapparaît avec Couper/Copier). En
          // TextSelection explicite : `selectAll()` produirait une AllSelection,
          // filtrée par la mini-barre (et par les poignées).
          const ed = editorRef.current
          if (ed) ed.commands.setTextSelection({ from: 0, to: ed.state.doc.content.size })
          armBodyMiniBar()
        }} />}
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
// Recherche dans le document — branchée sur la SearchBar STANDARD du core (via un
// `SearchConfig`), au lieu d'une barre maison. La saisie surligne les occurrences ;
// le bouton « filtres » de la SearchBar déroule le panneau Remplacer + options.
interface DocFindState {
  editor: Editor | null
  query: string; replace: string
  matchCase: boolean; wholeWord: boolean; useRegex: boolean
  idx: number; rev: number
  set: (p: Partial<DocFindState>) => void
}
const useDocFind = create<DocFindState>((set) => ({
  editor: null, query: '', replace: '',
  matchCase: false, wholeWord: false, useRegex: false, idx: 0, rev: 0,
  set: (p) => set(p),
}))

// Panneau injecté dans le DÉROULÉ de la SearchBar du core (compteur + préc/suiv +
// Remplacer + options casse / mot entier / regex). façon Google Docs.
function DocFindPanel({ onClose: _onClose }: { onClose: () => void }) {
  const { t } = useTranslation('office')
  const editor    = useDocFind(s => s.editor)
  const query     = useDocFind(s => s.query)
  const replace   = useDocFind(s => s.replace)
  const matchCase = useDocFind(s => s.matchCase)
  const wholeWord = useDocFind(s => s.wholeWord)
  const useRegex  = useDocFind(s => s.useRegex)
  const idx       = useDocFind(s => s.idx)
  const rev       = useDocFind(s => s.rev)
  const matches = useMemo(
    () => editor ? findMatches(editor.state.doc, query, { matchCase, wholeWord, regex: useRegex }) : [],
    [editor, query, matchCase, wholeWord, useRegex, rev],
  )
  const safeIdx = matches.length ? Math.min(idx, matches.length - 1) : 0
  const go = (n: number) => {
    if (!editor || !matches.length) return
    const i = ((n % matches.length) + matches.length) % matches.length
    useDocFind.getState().set({ idx: i })
    const m = matches[i]
    editor.chain().setTextSelection({ from: m.from, to: m.to }).scrollIntoView().run()
  }
  const replaceOne = () => {
    if (!editor || !matches.length) return
    const m = matches[safeIdx]
    editor.chain().focus().insertContentAt({ from: m.from, to: m.to }, replace).setTextSelection(m.from + replace.length).run()
  }
  const replaceAll = () => {
    if (!editor || !matches.length) return
    let chain = editor.chain().focus()
    for (let k = matches.length - 1; k >= 0; k--) chain = chain.insertContentAt({ from: matches[k].from, to: matches[k].to }, replace)
    chain.run()
  }
  const optBtn = (on: boolean, set: (v: boolean) => void, label: string, title: string) => (
    <button onMouseDown={e => e.preventDefault()} onClick={() => set(!on)} title={title}
      className={`w-8 h-8 flex items-center justify-center rounded text-xs font-semibold ${on ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}>
      {label}
    </button>
  )
  return (
    <div className="p-2.5 flex flex-col gap-2">
      {/* Navigation entre occurrences */}
      <div className="flex items-center gap-1.5">
        <span className="flex-1 text-sm text-text-secondary tabular-nums">
          {matches.length ? `${safeIdx + 1} / ${matches.length}` : t('doc_no_match', { defaultValue: 'Aucun résultat' })}
        </span>
        <button onClick={() => go(safeIdx - 1)} disabled={!matches.length} title={t('doc_previous')}
          className="w-8 h-8 flex items-center justify-center rounded-full text-text-secondary hover:bg-surface-2 disabled:opacity-30 flex-shrink-0"><ChevronUpIcon /></button>
        <button onClick={() => go(safeIdx + 1)} disabled={!matches.length} title={t('doc_next')}
          className="w-8 h-8 flex items-center justify-center rounded-full text-text-secondary hover:bg-surface-2 disabled:opacity-30 flex-shrink-0"><ChevronDown size={16} /></button>
      </div>
      {/* Remplacer */}
      <div className="flex items-center gap-1.5">
        <input value={replace} onChange={e => useDocFind.getState().set({ replace: e.target.value })}
          placeholder={t('doc_replace_with', { defaultValue: 'Remplacer par' })}
          className="flex-1 min-w-0 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
        <Button variant="secondary" size="sm" onClick={replaceOne} disabled={!matches.length} className="px-2 text-xs flex-shrink-0">{t('doc_replace', { defaultValue: 'Remplacer' })}</Button>
        <Button variant="secondary" size="sm" onClick={replaceAll} disabled={!matches.length} className="px-2 text-xs flex-shrink-0">{t('doc_replace_all', { defaultValue: 'Tout' })}</Button>
      </div>
      {/* Options */}
      <div className="flex items-center gap-1.5">
        {optBtn(matchCase, v => useDocFind.getState().set({ matchCase: v, idx: 0 }), 'Aa', t('doc_match_case'))}
        {optBtn(wholeWord, v => useDocFind.getState().set({ wholeWord: v, idx: 0 }), '[W]', t('doc_whole_word', { defaultValue: 'Mot entier' }))}
        {optBtn(useRegex, v => useDocFind.getState().set({ useRegex: v, idx: 0 }), '.*', t('doc_use_regex', { defaultValue: 'Expression régulière' }))}
      </div>
    </div>
  )
}

// Contrôleur invisible : lie l'éditeur courant au store, surligne les occurrences en
// direct (même déroulé fermé) et ENREGISTRE le `SearchConfig` pour la route document.
function DocFindController({ editor, highlight, focusSignal }: {
  editor: Editor | null
  highlight: (v: { ranges: Array<{ from: number; to: number }>; active: number }) => void
  focusSignal: number
}) {
  const { t } = useTranslation('office')
  const query     = useDocFind(s => s.query)
  const matchCase = useDocFind(s => s.matchCase)
  const wholeWord = useDocFind(s => s.wholeWord)
  const useRegex  = useDocFind(s => s.useRegex)
  const idx       = useDocFind(s => s.idx)
  const rev       = useDocFind(s => s.rev)

  // Expose l'éditeur courant au panneau (déroulé).
  useEffect(() => {
    useDocFind.getState().set({ editor })
    return () => useDocFind.getState().set({ editor: null })
  }, [editor])
  // Recalcule à chaque changement du document.
  useEffect(() => {
    if (!editor) return
    const fn = () => useDocFind.getState().set({ rev: (useDocFind.getState().rev + 1) & 0xffff })
    editor.on('transaction', fn)
    return () => { editor.off('transaction', fn) }
  }, [editor])
  // Surbrillance jaune (active même quand le panneau est fermé) ; nettoyage au démontage.
  useEffect(() => {
    const matches = editor ? findMatches(editor.state.doc, query, { matchCase, wholeWord, regex: useRegex }) : []
    const active = matches.length ? Math.min(idx, matches.length - 1) : 0
    highlight({ ranges: matches, active })
  }, [editor, highlight, query, matchCase, wholeWord, useRegex, idx, rev])
  useEffect(() => () => highlight({ ranges: [], active: 0 }), [highlight])
  // Ctrl/⌘+F → focus le champ de la SearchBar standard.
  useEffect(() => {
    if (!focusSignal) return
    const el = document.querySelector('input[type="search"]') as HTMLInputElement | null
    el?.focus(); el?.select()
  }, [focusSignal])
  // Branche la SearchBar du core : placeholder + recherche live + panneau déroulant.
  useEffect(() => {
    useSearchStore.getState().register({
      moduleId: 'office-doc-find',
      routePrefix: '/office/documents',
      placeholder: t('doc_find_in_document', { defaultValue: 'Rechercher dans le document' }),
      onSearch: (q: string) => useDocFind.getState().set({ query: q, idx: 0 }),
      FilterPanel: DocFindPanel,
    })
    return () => useSearchStore.getState().unregister('office-doc-find')
  }, [t])
  return null
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
        className="flex items-center gap-0.5 h-7 px-1.5 rounded text-text-secondary hover:bg-hover text-[11px]">
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

// « AaBbCc » rendu dans un style nommé (aperçu de la galerie de styles).
function StylePreviewText({ s, px }: { s: NamedStyle; px: number }) {
  return (
    <span style={{ fontFamily: s.font || 'Arial', fontWeight: s.bold ? 700 : 400, fontStyle: s.italic ? 'italic' : 'normal',
      color: s.color || '#3c4043', fontSize: px, lineHeight: 1, whiteSpace: 'nowrap' }}>AaBbCc</span>
  )
}

// ── Galerie de styles (Accueil → Styles, façon Word) ────────────────────────
// Rangée horizontale de cartes d'aperçu (les N premières en ligne) + un bouton
// chevron qui déroule TOUS les styles en grille, suivie des actions (créer /
// effacer / gérer). Un clic sur une carte applique le style ; la carte du style
// courant est surlignée.
function RibbonStyleGallery({ styles, curId, onApply, onManage, onClear }: {
  styles: NamedStyle[]; curId: string
  onApply: (id: string) => void; onManage: () => void; onClear: () => void
}) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const wrapRef = useRef<HTMLDivElement>(null)
  const INLINE = 6
  const inline = styles.slice(0, INLINE)

  const Card = ({ s, big }: { s: NamedStyle; big?: boolean }) => {
    const active = s.id === curId
    const px = big ? Math.min(20, Math.max(12, (s.size ?? 11) * 0.7)) : Math.min(15, Math.max(9, (s.size ?? 11) * 0.55))
    return (
      <button onMouseDown={e => e.preventDefault()} onClick={() => { onApply(s.id); if (big) setOpen(false) }} title={styleLabel(s, t)}
        className={`flex flex-col items-center justify-center gap-0.5 rounded-xs flex-shrink-0 border ${active ? 'bg-primary-light border-accent' : 'border-transparent hover:bg-hover'}`}
        style={{ width: big ? 84 : 56, padding: '2px 3px' }}>
        <span className="flex items-center justify-center overflow-hidden w-full" style={{ height: big ? 30 : 24 }}>
          <StylePreviewText s={s} px={px} />
        </span>
        <span className="text-[10px] leading-none truncate w-full text-center text-text-secondary">{styleLabel(s, t)}</span>
      </button>
    )
  }

  return (
    // Boîte façon Word : bordure fine à coins arrondis (2px) autour des cartes + chevron.
    <div ref={wrapRef} className="flex items-stretch gap-0.5 border border-border rounded-xs p-0.5">
      <div className="flex items-stretch gap-0.5">
        {inline.map(s => <Card key={s.id} s={s} />)}
      </div>
      <button onClick={() => setOpen(o => !o)} title={t('doc_styles_more', { defaultValue: 'Autres styles' })}
        className="flex items-center justify-center w-5 self-stretch rounded-xs border-l border-border text-text-secondary hover:bg-hover">
        <ChevronDown size={13} />
      </button>
      {/* Déroulé ancré sur la BOÎTE (bord gauche), pas sur le chevron → aligné à gauche. */}
      <AnchoredPopover anchorRef={wrapRef} open={open} onClose={() => setOpen(false)}>
        <div className="bg-white border border-border rounded-lg shadow-lg p-2" style={{ width: 580 }}>
          <div className="flex flex-wrap gap-1">
            {styles.map(s => <Card key={s.id} s={s} big />)}
          </div>
          <div className="border-t border-border mt-2 pt-1 flex flex-col">
            <button onClick={() => { setOpen(false); onManage() }} className="flex items-center gap-2 px-2 py-1.5 rounded text-sm text-text-primary hover:bg-hover text-left">
              <Plus size={15} /> {t('doc_create_style', { defaultValue: 'Créer un style' })}
            </button>
            <button onClick={() => { setOpen(false); onClear() }} className="flex items-center gap-2 px-2 py-1.5 rounded text-sm text-text-primary hover:bg-hover text-left">
              <Eraser size={15} /> {t('doc_clear_formatting', { defaultValue: 'Effacer la mise en forme' })}
            </button>
            <button onClick={() => { setOpen(false); onManage() }} className="flex items-center gap-2 px-2 py-1.5 rounded text-sm text-text-primary hover:bg-hover text-left">
              <Pencil size={15} /> {t('doc_apply_styles', { defaultValue: 'Appliquer les styles…' })}
            </button>
          </div>
        </div>
      </AnchoredPopover>
    </div>
  )
}

// Petit bouton du groupe Police 2 rangées (façon Word) : icône seule, compact.
function FGBtn({ icon, active, onClick, title }: { icon: React.ReactNode; active?: boolean; onClick?: () => void; title: string }) {
  return (
    <button onMouseDown={e => e.preventDefault()} onClick={onClick} title={title}
      className={`flex items-center justify-center w-6 h-[22px] rounded-xs ${active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-hover'}`}>
      {icon}
    </button>
  )
}

// Bouton « Modifier la casse » (Aa ▾) du groupe Police 2 rangées.
function FGCaseMenu({ onCase, onSmallCaps, onSpacing }: { onCase: (m: CaseMode) => void; onSmallCaps: () => void; onSpacing: () => void }) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  const items: Array<[string, string, () => void]> = [
    ['upper', t('doc_case_upper', { defaultValue: 'MAJUSCULES' }), () => onCase('upper')],
    ['lower', t('doc_case_lower', { defaultValue: 'minuscules' }), () => onCase('lower')],
    ['title', t('doc_case_title', { defaultValue: '1re Lettre De Chaque Mot' }), () => onCase('title')],
    ['sentence', t('doc_case_sentence', { defaultValue: 'Casse de la phrase' }), () => onCase('sentence')],
    ['toggle', t('doc_case_toggle', { defaultValue: 'iNVERSER LA cASSE' }), () => onCase('toggle')],
    ['smallcaps', t('doc_small_caps', { defaultValue: 'Petites majuscules' }), onSmallCaps],
    ['spacing', t('doc_char_spacing', { defaultValue: 'Espacement des caractères…' }), onSpacing],
  ]
  return (
    <>
      <button ref={ref} onMouseDown={e => e.preventDefault()} onClick={() => setOpen(o => !o)} title={t('doc_change_case', { defaultValue: 'Modifier la casse' })}
        className="flex items-center justify-center gap-0.5 h-[22px] px-1 rounded-xs text-text-secondary hover:bg-hover">
        <CaseSensitive size={16} /><ChevronDown size={11} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="bg-white border border-border rounded-lg shadow-lg p-1 flex flex-col min-w-[210px]">
          {items.map(([iid, label, fn]) => (
            <button key={iid} onClick={() => { setOpen(false); fn() }} className="px-2 py-1.5 rounded text-sm text-text-primary hover:bg-hover text-left">{label}</button>
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
function tableMutateOn(ed: Editor, fn: (rows: JSONContent[], ctx: NonNullable<ReturnType<typeof tableCtxOf>>, json: JSONContent) => void): void {
  const ctx = tableCtxOf(ed); if (!ctx) return
  const json = ctx.tableNode.toJSON() as JSONContent
  const rows = (json.content ?? []) as JSONContent[]
  fn(rows, ctx, json)
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

// ── Galerie Bordures : icônes ────────────────────────────────────────────────
// Chaque icône montre une grille 2×2 en pointillé clair, avec en trait plein
// sombre les arêtes que le préréglage pose (idiome de Word / Google Docs).
function BorderIcon({ preset }: { preset: BorderPreset }) {
  const A = 2, B = 15   // bornes de la grille dans un viewBox 17×17
  const M = 8.5         // médianes (arêtes intérieures)
  const on = 'currentColor'
  const off = '#b9bcc0'
  // Segments : [x1, y1, x2, y2, actif]
  const segs: Array<[number, number, number, number, boolean]> = [
    [A, A, B, A, ['all', 'outside', 'top'].includes(preset)],                 // haut
    [A, B, B, B, ['all', 'outside', 'bottom'].includes(preset)],              // bas
    [A, A, A, B, ['all', 'outside', 'left'].includes(preset)],                // gauche
    [B, A, B, B, ['all', 'outside', 'right'].includes(preset)],               // droite
    [A, M, B, M, ['all', 'inside', 'insideH'].includes(preset)],              // médiane horizontale
    [M, A, M, B, ['all', 'inside', 'insideV'].includes(preset)],              // médiane verticale
  ]
  return (
    <svg width="17" height="17" viewBox="0 0 17 17" aria-hidden="true">
      {segs.map(([x1, y1, x2, y2, active], i) => (
        <line key={i} x1={x1} y1={y1} x2={x2} y2={y2}
          stroke={active ? on : off} strokeWidth={active ? 1.6 : 1}
          strokeDasharray={active ? undefined : '1.5 1.5'} strokeLinecap="butt" />
      ))}
    </svg>
  )
}

// ── Grille de sélection « Insérer un tableau » (façon Word) ───────────────────
// Survol = aperçu des dimensions dans l'en-tête ; clic = insertion. Le clavier
// fonctionne aussi (flèches pour se déplacer, Entrée pour valider) : la grille est
// une vraie grille de boutons focusables, pas un tapis de div.
const GRID_COLS = 10
const GRID_ROWS = 8
function TableGridPicker({ onPick, title, hint }: {
  onPick: (rows: number, cols: number) => void
  title: (rows: number, cols: number) => string
  hint: string
}) {
  const [hover, setHover] = useState<{ r: number; c: number } | null>(null)
  const CELL = 17, GAP = 2
  return (
    <div className="px-2 pt-1.5 pb-1" onMouseLeave={() => setHover(null)}>
      <div className="text-xs font-medium text-text-primary mb-1.5 h-4">
        {hover ? title(hover.r + 1, hover.c + 1) : hint}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: `repeat(${GRID_COLS}, ${CELL}px)`, gap: GAP }}>
        {Array.from({ length: GRID_ROWS * GRID_COLS }, (_, i) => {
          const r = Math.floor(i / GRID_COLS), c = i % GRID_COLS
          const on = !!hover && r <= hover.r && c <= hover.c
          return (
            <button key={i} type="button" tabIndex={-1}
              aria-label={title(r + 1, c + 1)}
              onMouseEnter={() => setHover({ r, c })}
              onFocus={() => setHover({ r, c })}
              onMouseDown={e => e.preventDefault()}
              onClick={() => onPick(r + 1, c + 1)}
              style={{ width: CELL, height: CELL, padding: 0,
                       border: `1px solid ${on ? 'var(--kbn-accent, #1a73e8)' : '#c7c9cc'}`,
                       background: on ? 'color-mix(in srgb, var(--kbn-accent, #1a73e8) 18%, transparent)' : '#fff' }} />
          )
        })}
      </div>
    </div>
  )
}

const BORDER_LABELS_FR: Record<BorderPreset, string> = {
  all: 'Toutes les bordures',
  inside: 'Bordures intérieures',
  outside: 'Bordures extérieures',
  top: 'Bordure supérieure',
  insideH: 'Bordure horizontale intérieure',
  bottom: 'Bordure inférieure',
  left: 'Bordure gauche',
  insideV: 'Bordure verticale intérieure',
  right: 'Bordure droite',
  none: 'Aucune bordure',
}
// Grille 3×3 des préréglages + « Aucune bordure », dans l'ordre de Word.
const BORDER_GRID: BorderPreset[] = ['all', 'inside', 'outside', 'top', 'insideH', 'bottom', 'left', 'insideV', 'right']
function BorderGallery({ onPick, label }: { onPick: (p: BorderPreset) => void; label: (p: BorderPreset) => string }) {
  const cell = 'flex items-center justify-center w-7 h-7 rounded hover:bg-surface-hover active:bg-surface-active text-text-primary'
  return (
    <div className="p-1">
      <div className="grid grid-cols-3 gap-0.5">
        {BORDER_GRID.map(p => (
          <button key={p} type="button" className={cell} title={label(p)} aria-label={label(p)}
            onMouseDown={e => e.preventDefault()} onClick={() => onPick(p)}>
            <BorderIcon preset={p} />
          </button>
        ))}
      </div>
      <div className="mt-1 pt-1 border-t border-border">
        <button type="button" className="flex items-center gap-2 w-full px-1.5 py-1 rounded text-xs text-text-primary hover:bg-surface-hover text-left"
          onMouseDown={e => e.preventDefault()} onClick={() => onPick('none')}>
          <BorderIcon preset="none" />
          <span className="whitespace-nowrap">{label('none')}</span>
        </button>
      </div>
    </div>
  )
}

// ── Galerie Bordures (façon Word) ────────────────────────────────────────────
export type BorderPreset = 'all' | 'inside' | 'outside' | 'top' | 'bottom' | 'left' | 'right'
  | 'insideH' | 'insideV' | 'none'
type CellBorderSpec = { w: number; s: 'solid' | 'dashed' | 'dotted'; c: string }
type Sides = { t?: CellBorderSpec | null; b?: CellBorderSpec | null; l?: CellBorderSpec | null; r?: CellBorderSpec | null }

// Applique un préréglage à la PLAGE de cellules (une seule cellule = plage 1×1).
// Le « pinceau » est celui du ruban (couleur / épaisseur / style de trait du
// tableau), comme la couleur et l'épaisseur de crayon de l'onglet Création de Word.
// Seuls les côtés CONCERNÉS par le préréglage sont écrits : les autres gardent
// leur valeur, donc les préréglages se cumulent (Extérieures puis Horizontales
// intérieures, comme dans Word). « Aucune » efface les 4 côtés de la plage en
// posant un `null` explicite, qui bat le défaut du tableau.
function applyBorderPreset(ed: Editor, rect: TableRect, preset: BorderPreset, pen: CellBorderSpec): void {
  tableMutateOn(ed, rows => {
    const grid = buildGrid(rows)
    const nCols = gridCols(grid)
    const r0 = Math.max(0, Math.min(rect.r0, rect.r1)), r1 = Math.min(rows.length - 1, Math.max(rect.r0, rect.r1))
    const c0 = Math.max(0, Math.min(rect.c0, rect.c1)), c1 = Math.min(nCols - 1, Math.max(rect.c0, rect.c1))
    const seen = new Set<string>()
    for (let r = r0; r <= r1; r++) for (let c = c0; c <= c1; c++) {
      const g = grid[r]?.[c]; if (!g) continue
      const k = g.ri + ',' + g.ci; if (seen.has(k)) continue; seen.add(k)
      const cell = (rows[g.ri].content as JSONContent[])[g.ci]
      const attrs = cellAttrs(cell)
      const cur = (attrs.cellBorders as Sides | null) || {}
      // Position de la cellule DANS la plage (une cellule fusionnée s'étend :
      // son bord droit/bas est celui de sa dernière colonne/ligne couverte).
      const cs = Number(attrs.colspan) || 1, rs = Number(attrs.rowspan) || 1
      const isTop = r === r0, isLeft = c === c0
      const isBottom = r + rs - 1 >= r1, isRight = c + cs - 1 >= c1
      const next: Sides = { ...cur }
      const set = (side: keyof Sides, on: boolean) => { next[side] = on ? { ...pen } : null }
      switch (preset) {
        case 'all':
          set('t', true); set('b', true); set('l', true); set('r', true); break
        case 'none':
          set('t', false); set('b', false); set('l', false); set('r', false); break
        case 'outside':
          set('t', isTop); set('b', isBottom); set('l', isLeft); set('r', isRight); break
        case 'inside':
          if (!isTop) set('t', true)
          if (!isBottom) set('b', true)
          if (!isLeft) set('l', true)
          if (!isRight) set('r', true)
          break
        case 'insideH':
          if (!isTop) set('t', true)
          if (!isBottom) set('b', true)
          break
        case 'insideV':
          if (!isLeft) set('l', true)
          if (!isRight) set('r', true)
          break
        case 'top':    if (isTop) set('t', true); break
        case 'bottom': if (isBottom) set('b', true); break
        case 'left':   if (isLeft) set('l', true); break
        case 'right':  if (isRight) set('r', true); break
      }
      // Aucun côté renseigné → on retire l'attribut (retour au défaut du tableau).
      const empty = next.t === undefined && next.b === undefined && next.l === undefined && next.r === undefined
      cell.attrs = { ...attrs, cellBorders: empty ? null : next }
    }
  })
}
// Supprime une colonne de grille (décrémente les colspan, retire les cellules 1×).
function deleteOneCol(ed: Editor, col: number): void {
  tableMutateOn(ed, (rows, _ctx, json) => {
    const grid = buildGrid(rows)
    if (gridCols(grid) <= 1) return
    spliceBandAttrs(json, ['colWidths'], col, 1)
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
  tableMutateOn(ed, (rows, _ctx, json) => {
    if (rows.length <= 1) return
    spliceBandAttrs(json, ['rowHeights', 'rowHeightModes'], row, 1)
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

// ── Disposition (Word) : tri, répartition, ajustement, fractionnement, formule ──

// Valeur numérique « intelligente » d'une cellule (formats FR : espaces, virgule,
// €/$/%) — null si le texte n'est pas un nombre.
function cellNumber(text: string): number | null {
  const t = text.trim().replace(/[\s\u202f\u00a0]/g, '').replace(',', '.').replace(/[€$%]/g, '')
  if (!t || !/^[-+]?\d+(\.\d+)?$/.test(t)) return null
  return parseFloat(t)
}
function cellPlainText(c: JSONContent | undefined): string {
  if (!c) return ''
  let out = ''
  const walk = (n: JSONContent) => { if (n.type === 'text') out += n.text ?? ''; (n.content ?? []).forEach(walk) }
  walk(c)
  return out
}

// Trie les rangées par la colonne `col` (tri numérique si toutes les clés sont des
// nombres, sinon alphabétique fr). La rangée 0 reste en tête si le tableau a une
// ligne d'en-tête (style header/striped ou en-tête répété) — sémantique Word.
function sortTableRows(ed: Editor, col: number, dir: 'asc' | 'desc'): void {
  tableMutateOn(ed, (rows, ctx, json) => {
    const a = (json.attrs ?? {}) as Record<string, unknown>
    const hasHeader = a.tableStyle === 'header' || a.tableStyle === 'striped' || !!a.headerRepeat
    const start = hasHeader ? 1 : 0
    if (rows.length - start < 2) return
    const grid = buildGrid(rows)
    const body = rows.slice(start).map((row, k) => {
      const g = grid[start + k]?.[col]
      const txt = g ? cellPlainText((rows[g.ri].content as JSONContent[])[g.ci]) : ''
      return { row, txt, num: cellNumber(txt), k }
    })
    const allNum = body.every(b => b.num != null || !b.txt.trim())
    body.sort((x, y) => {
      const cmp = allNum
        ? (x.num ?? -Infinity) - (y.num ?? -Infinity)
        : x.txt.localeCompare(y.txt, 'fr', { sensitivity: 'base', numeric: true })
      return (dir === 'asc' ? cmp : -cmp) || x.k - y.k   // tri STABLE
    })
    rows.splice(start, rows.length - start, ...body.map(b => b.row))
    // Les hauteurs par rangée suivent la permutation.
    for (const key of ['rowHeights', 'rowHeightModes']) {
      const arr = a[key]
      if (Array.isArray(arr)) {
        const next = arr.slice()
        body.forEach((b, idx) => { next[start + idx] = arr[start + b.k] })
        a[key] = next
      }
    }
    json.attrs = a
  })
}

// Uniformise la hauteur des lignes / la largeur des colonnes (Word « Uniformiser »).
// Les dimensions ACTUELLES viennent de la géométrie rendue (rowY/colX du layout).
function distributeTable(ed: Editor, which: 'rows' | 'cols', metrics: { rowHeights: number[]; colWidths: number[] } | null): void {
  const ctx = tableCtxOf(ed); if (!ctx || !metrics) return
  if (which === 'cols') {
    const total = metrics.colWidths.reduce((s2, x) => s2 + x, 0)
    const n = Math.max(1, metrics.colWidths.length)
    setTableAttrAt(ed, ctx.tablePos, { colWidths: total > 0 ? new Array(n).fill(Math.round(total / n)) : null })
  } else {
    const total = metrics.rowHeights.reduce((s2, x) => s2 + x, 0)
    const n = Math.max(1, metrics.rowHeights.length)
    if (total > 0) setTableAttrAt(ed, ctx.tablePos, { rowHeights: new Array(n).fill(Math.round(total / n)), rowHeightModes: null })
  }
}

// Ajustement automatique (Word) : 'window' = colonnes uniformes pleine largeur ;
// 'content' = chaque colonne à la largeur de son contenu le plus large (mesure canvas,
// fusions ignorées ; le moteur re-cape la somme à la largeur de la zone).
let _fitMeasureCtx: CanvasRenderingContext2D | null = null
function autoFitTable(ed: Editor, mode: 'window' | 'content' | 'fixed'): void {
  const ctx = tableCtxOf(ed); if (!ctx) return
  // « Largeur de colonne fixe » : on FIGE les largeurs courantes et on passe en
  // disposition fixe (le texte se renverra à la ligne au lieu d'élargir la colonne).
  if (mode === 'fixed') { setTableAttrAt(ed, ctx.tablePos, { tableLayout: 'fixed' }); return }
  // Ajuster à la fenêtre / au contenu : on revient en disposition automatique.
  if (mode === 'window') { setTableAttrAt(ed, ctx.tablePos, { colWidths: null, tableLayout: 'autofit' }); return }
  if (!_fitMeasureCtx) _fitMeasureCtx = document.createElement('canvas').getContext('2d')
  const m = _fitMeasureCtx!
  const rows = ((ctx.tableNode.toJSON() as JSONContent).content ?? []) as JSONContent[]
  const grid = buildGrid(rows)
  const nCols = gridCols(grid)
  const widths: number[] = new Array(nCols).fill(40)
  const seen = new Set<string>()
  for (let r = 0; r < grid.length; r++) for (let c = 0; c < nCols; c++) {
    const g = grid[r]?.[c]; if (!g) continue
    const k = g.ri + ',' + g.ci; if (seen.has(k)) continue; seen.add(k)
    const cell = (rows[g.ri].content as JSONContent[])[g.ci]
    if ((Number(cellAttrs(cell).colspan) || 1) > 1) continue
    for (const p of (cell.content ?? []) as JSONContent[]) {
      let text = '', bold = false, size = 11, fam = 'Arial'
      const walk = (n: JSONContent) => {
        if (n.type === 'text') {
          text += n.text ?? ''
          for (const mk of n.marks ?? []) {
            if (mk.type === 'bold') bold = true
            if (mk.type === 'textStyle' && mk.attrs?.fontSize) size = parseFloat(String(mk.attrs.fontSize)) || 11
            if (mk.type === 'textStyle' && mk.attrs?.fontFamily) fam = String(mk.attrs.fontFamily)
          }
        }
        (n.content ?? []).forEach(walk)
      }
      walk(p)
      m.font = `${bold ? 'bold ' : ''}${size * (96 / 72)}px ${fam}, sans-serif`
      widths[c] = Math.max(widths[c], Math.ceil(m.measureText(text).width) + 16)
    }
  }
  setTableAttrAt(ed, ctx.tablePos, { colWidths: widths })
}

// Fractionne le tableau AVANT la rangée courante (Word « Fractionner le tableau »).
function splitTableAtRow(ed: Editor): void {
  const ctx = tableCtxOf(ed); if (!ctx || ctx.rowIndex <= 0) return
  const json = ctx.tableNode.toJSON() as JSONContent
  const rows = (json.content ?? []) as JSONContent[]
  const a = (json.attrs ?? {}) as Record<string, unknown>
  const splitArr = (arr: unknown): [unknown, unknown] =>
    Array.isArray(arr) ? [arr.slice(0, ctx.rowIndex), arr.slice(ctx.rowIndex)] : [null, null]
  const [h1, h2] = splitArr(a.rowHeights)
  const [m1, m2] = splitArr(a.rowHeightModes)
  const top    = { ...json, attrs: { ...a, rowHeights: h1, rowHeightModes: m1 }, content: rows.slice(0, ctx.rowIndex) }
  const bottom = { ...json, attrs: { ...a, rowHeights: h2, rowHeightModes: m2 }, content: rows.slice(ctx.rowIndex) }
  const nodes = [top, { type: 'paragraph' }, bottom].map(x => ed.state.schema.nodeFromJSON(x as JSONContent))
  let tr = ed.state.tr.replaceWith(ctx.tablePos, ctx.tablePos + ctx.tableNode.nodeSize, nodes)
  tr = tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(tr.doc.content.size - 1, ctx.tablePos + nodes[0].nodeSize + 1)), 1))
  ed.view.focus(); ed.view.dispatch(tr)
}

// Formule façon Word (=SUM(ABOVE) / =SUM(LEFT)) : somme des cellules numériques
// contiguës au-dessus / à gauche, insérée au curseur (valeur statique).
function insertTableSum(ed: Editor, dir: 'above' | 'left'): void {
  const ctx = tableCtxOf(ed); if (!ctx) return
  const rows = ((ctx.tableNode.toJSON() as JSONContent).content ?? []) as JSONContent[]
  const grid = buildGrid(rows)
  const r = ctx.rowIndex, c = ctx.colStart
  let sum = 0, found = false
  const seen = new Set<string>()
  const step = (gr: number, gc: number): boolean => {
    const g = grid[gr]?.[gc]; if (!g) return false
    const k = g.ri + ',' + g.ci; if (seen.has(k)) return true
    seen.add(k)
    const n = cellNumber(cellPlainText((rows[g.ri].content as JSONContent[])[g.ci]))
    if (n == null) return false   // Word s'arrête à la première cellule non numérique
    sum += n; found = true
    return true
  }
  if (dir === 'above') { for (let k = r - 1; k >= 0 && step(k, c); k--) { /* remonte */ } }
  else { for (let k = c - 1; k >= 0 && step(r, k); k--) { /* vers la gauche */ } }
  if (!found) return
  const out = (Math.round(sum * 100) / 100).toString().replace('.', ',')
  ed.chain().focus().insertContent(out).run()
}
function deleteColsRange(ed: Editor, c0: number, c1: number): void { for (let c = c1; c >= c0; c--) deleteOneCol(ed, c) }

// Insertion d'une ligne/colonne à une position de grille (cellules 1× simples).
// Les tableaux d'attributs INDEXÉS PAR BANDE (largeurs de colonne, hauteurs et modes
// de hauteur de ligne) doivent suivre les insertions/suppressions de bandes : sinon
// leur longueur ne correspond plus au nombre de bandes, le moteur les IGNORE en bloc
// (`colWidths.length === colCount`) et toute la mise en page saute — un déplacement de
// colonne perd alors ses largeurs et semble atterrir n'importe où.
function spliceBandAttrs(json: JSONContent, keys: string[], at: number, remove: number, insert?: (prev: unknown) => unknown): void {
  const a = (json.attrs ?? {}) as Record<string, unknown>
  const next: Record<string, unknown> = { ...a }
  let touched = false
  for (const key of keys) {
    const arr = a[key]
    if (!Array.isArray(arr)) continue
    const cp = arr.slice()
    if (remove > 0) cp.splice(at, remove)
    else cp.splice(at, 0, insert ? insert(arr[Math.min(Math.max(0, at), arr.length - 1)]) : 0)
    next[key] = cp
    touched = true
  }
  if (touched) json.attrs = next
}

function insertRowAt(ed: Editor, atRow: number): void {
  tableMutateOn(ed, (rows, _ctx, json) => {
    const grid = buildGrid(rows)
    rows.splice(atRow, 0, { type: 'tableRow', content: Array.from({ length: gridCols(grid) }, emptyCellJSON) })
    // Nouvelle ligne : hauteur AUTO ('atleast', 0) — pas la hauteur de sa voisine.
    spliceBandAttrs(json, ['rowHeights'], atRow, 0, () => 0)
    spliceBandAttrs(json, ['rowHeightModes'], atRow, 0, () => 'atleast')
  })
}
function insertColAt(ed: Editor, atCol: number): void {
  tableMutateOn(ed, (rows, _ctx, json) => {
    // Nouvelle colonne : même largeur que sa voisine (façon Word/Docs) ; le moteur
    // reproportionne si le total dépasse la zone de contenu.
    spliceBandAttrs(json, ['colWidths'], atCol, 0, prev => (typeof prev === 'number' && prev > 4 ? prev : 0))
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

// Déplace une colonne de grille de `from` vers `to` (index de grille APRÈS retrait,
// façon glisser-déposer d'en-tête). Abandonne si une cellule fusionnée chevauche la
// colonne déplacée ou la position d'arrivée : réorganiser les colonnes sous une
// fusion horizontale produirait un tableau incohérent (Word refuse aussi).
function moveColumn(ed: Editor, from: number, to: number): void {
  if (from === to) return
  tableMutateOn(ed, (rows, _ctx, json) => {
    const grid = buildGrid(rows)
    const nCols = gridCols(grid)
    if (from < 0 || from >= nCols || to < 0 || to >= nCols) return
    // Aucune fusion horizontale nulle part sur la plage traversée.
    const lo = Math.min(from, to), hi = Math.max(from, to)
    for (let r = 0; r < rows.length; r++) {
      for (let c = lo; c <= hi; c++) {
        const g = grid[r]?.[c]
        if (!g) return
        const cell = (rows[g.ri].content as JSONContent[])[g.ci]
        if ((Number(cellAttrs(cell).colspan) || 1) > 1) return
      }
    }
    for (let r = 0; r < rows.length; r++) {
      const g = grid[r]?.[from]; if (!g) return
      const cells = rows[g.ri].content as JSONContent[]
      const [moved] = cells.splice(g.ci, 1)
      // Les cellules JSON sont en ordre de colonne (aucune fusion sur la plage) :
      // l'index JSON d'arrivée est l'index de grille visé.
      cells.splice(Math.max(0, Math.min(cells.length, to)), 0, moved)
    }
    // Les largeurs de colonne suivent le déplacement (la colonne garde sa largeur).
    const a = (json.attrs ?? {}) as Record<string, unknown>
    const w = a.colWidths
    if (Array.isArray(w) && w.length === nCols) {
      const next = w.slice()
      const [wv] = next.splice(from, 1)
      next.splice(to, 0, wv)
      json.attrs = { ...a, colWidths: next }
    }
  })
}

// Déplace une LIGNE de `from` vers `to`. Abandonne si une fusion VERTICALE
// (rowspan > 1) chevauche la plage traversée : permuter des lignes sous une fusion
// casserait la grille.
function moveRow(ed: Editor, from: number, to: number): void {
  if (from === to) return
  tableMutateOn(ed, (rows, _ctx, json) => {
    if (from < 0 || from >= rows.length || to < 0 || to >= rows.length) return
    const grid = buildGrid(rows)
    const nCols = gridCols(grid)
    const lo = Math.min(from, to), hi = Math.max(from, to)
    for (let r = lo; r <= hi; r++) {
      for (let c = 0; c < nCols; c++) {
        const g = grid[r]?.[c]
        if (!g) return
        const cell = (rows[g.ri].content as JSONContent[])[g.ci]
        if ((Number(cellAttrs(cell).rowspan) || 1) > 1) return
      }
    }
    const [moved] = rows.splice(from, 1)
    rows.splice(to, 0, moved)
    // Hauteurs / modes de hauteur suivent la ligne déplacée.
    const a = (json.attrs ?? {}) as Record<string, unknown>
    const next: Record<string, unknown> = { ...a }
    let touched = false
    for (const key of ['rowHeights', 'rowHeightModes']) {
      const arr = a[key]
      if (Array.isArray(arr) && arr.length >= rows.length) {
        const cp = arr.slice()
        const [v] = cp.splice(from, 1)
        cp.splice(to, 0, v)
        next[key] = cp
        touched = true
      }
    }
    if (touched) json.attrs = next
  })
}

// « Épingler l'en-tête jusqu'à cette ligne » (façon Google Docs) : les `n` premières
// rangées sont répétées en haut des pages suivantes. `headerRepeat` reste écrit pour
// la compatibilité (une rangée épinglée = la case « ligne d'en-tête » de Word).
function setHeaderRows(ed: Editor, n: number): void {
  const ctx = tableCtxOf(ed); if (!ctx) return
  ed.chain().focus().updateAttributes('table', { headerRows: n, headerRepeat: n > 0 }).run()
}

// Contexte du ruban contextuel « Tableau ».
interface TableRibbonCtx {
  onRowAbove: () => void; onRowBelow: () => void; onColLeft: () => void; onColRight: () => void
  onDeleteRow: () => void; onDeleteCol: () => void; onDeleteTable: () => void
  onMerge: () => void; onSplit: () => void; canMerge: boolean
  onStyle: (s: string) => void; curStyle: string
  cellColorNode: React.ReactNode
  // Disposition (Word) : tri, répartition, ajustement, fractionnement, formule.
  onSort: (dir: 'asc' | 'desc') => void
  onCellVAlign: (v: 'top' | 'center' | 'bottom') => void
  curCellVAlign: string
  onDistribute: (which: 'rows' | 'cols') => void
  onAutoFit: (mode: 'window' | 'content' | 'fixed') => void
  onSplitTable: () => void
  onSum: (dir: 'above' | 'left') => void
  // Bordures personnalisées (couleur / épaisseur / trait).
  onBorder: (patch: Record<string, unknown>) => void
  curBorderWidth: number
  curBorderStyle: string
  borderColorNode: React.ReactNode
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
  onInsertImage: () => void
  onInsertTextBox: () => void; onInsertTable: () => void
  onInsertTableSize: (rows: number, cols: number) => void
  onSetHeader: () => void; onSetFooter: () => void; onInsertToc: () => void; onTocUpdate: () => void; onSpecialChars: () => void
  /** Word « Mettre à jour les champs » (F9) : champs `field` + table des matières. */
  onFieldsUpdate: () => void
  onLink: () => void
  // Vague « +50 » : transformations & insertions
  onChangeCase: (m: CaseMode) => void
  onToggleSmallCaps: () => void
  onCharSpacing: () => void
  onInsertFootnote: () => void
  onInsertEndnote: () => void
  headingNumbers: boolean
  onToggleHeadingNumbers: () => void
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
  outlineArrows: boolean; onToggleOutlineArrows: () => void
  recState: RecorderState; onShowMacros: () => void; onToggleRecord: () => void; onPauseRecord: () => void
  pageNumFormatNode: React.ReactNode
  mode: 'edit' | 'read'; onMode: (m: 'edit' | 'read') => void
  showRuler: boolean; onToggleRuler: () => void; navOpen: boolean; onToggleNav: () => void
  onDetails: () => void
  spellOn: boolean; onToggleSpell: () => void; spellCount: number; onSpellDictionary: () => void
  onAddComment: () => void; onToggleComments: () => void; commentsOpen: boolean; commentCount: number
  trackChanges: boolean; onToggleTrackChanges: () => void; reviewOpen: boolean; onToggleReview: () => void
  revFinal: boolean; onToggleRevFinal: () => void
  /** Everything the « Références » tab needs (built in its own module). */
  references: ReferencesRibbonCtx
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
  /** `t` non dégradé : les surfaces PARTAGÉES (galerie, onglet Forme) prennent une
   *  vraie `TFunction`, pas la signature simplifiée utilisée dans ce fichier. */
  tf: TFunction
  /** Géométrie armée par la galerie (dessin au glissement) + son armement. */
  armedShape: ShapeKind | null
  onArmShape: (k: ShapeKind) => void
  /** « Reproduire la mise en forme » : simple clic (une fois) / double clic (collant). */
  onFormatPainter: () => void
  onFormatPainterSticky: () => void
  formatPainterActive: boolean
  /** Habillage / ordre de plan / mise en page de la FORME sélectionnée. */
  onShapeWrap: (wrap: string) => void
  onShapeOrder: (op: ShapeOrderOp) => void
  onShapeLayout: () => void
  hf: HFBarCtx | null; onHFField: (tok: string) => void; onHFSwitch: () => void
  onHFFirstPage: (v: boolean) => void; onHFLinked: (v: boolean) => void; onHFClose: () => void
  /** Plages de contenu des cellules sélectionnées (mise en forme multi-cellules). */
  cellRanges: Array<{ from: number; to: number }> | null
}

// Common value of a textStyle attribute across the selection, or '' when the
// selection mixes several values (Word shows a blank font/size box then). An
// unstyled run counts as `def` (the effective default), so a uniform default
// selection still shows that default rather than blank.
function uniformTextStyle(editor: Editor | null | undefined, attr: 'fontFamily' | 'fontSize', def: string, normalize?: (v: string) => string): string {
  if (!editor) return def
  const norm = (v: unknown): string => {
    const s = v == null || v === '' ? def : String(v)
    return normalize ? normalize(s) : s
  }
  const { from, to, empty } = editor.state.selection
  if (empty) return norm(editor.getAttributes('textStyle')[attr])
  let val: string | undefined
  let mixed = false
  editor.state.doc.nodesBetween(from, to, node => {
    if (mixed || !node.isText) return
    const m = node.marks.find(mk => mk.type.name === 'textStyle')
    const cur = norm(m?.attrs[attr])
    if (val === undefined) val = cur
    else if (val !== cur) mixed = true
  })
  if (mixed) return ''
  return val ?? def
}

function buildDocumentRibbon(c: DocRibbonCtx): RibbonTab[] {
  const { t, fmt, body } = c
  const cr = c.cellRanges   // sélection de cellules de tableau (mise en forme groupée)
  const isA = (n: string, a?: Record<string, unknown>) => !!fmt?.isActive(n, a)
  const curSize = uniformTextStyle(fmt, 'fontSize', '11', v => String(Math.round(parseFloat(v))))   // '' when mixed
  const curSizeNum = parseInt(curSize, 10) || 11   // numeric fallback for +/- steppers
  const curFont = uniformTextStyle(fmt, 'fontFamily', 'Arial')   // '' when mixed
  const setSize = (n: number) => fmt && applyInlineFormat(fmt, { fs: `${Math.max(6, Math.min(96, n))}pt` }, cr)
  const curLs = (fmt?.getAttributes('paragraph').lineHeight ?? fmt?.getAttributes('heading').lineHeight ?? 1.15) as number
  const setLs = (lh: number) => { if (!fmt) return; if (cr) applyParaAcross(fmt, cr, { lineHeight: lh }); else fmt.chain().focus().updateAttributes('paragraph', { lineHeight: lh }).updateAttributes('heading', { lineHeight: lh }).run() }
  const indent = (d: number) => { const cur = (fmt?.getAttributes('paragraph').indent ?? 0) as number; fmt?.chain().focus().updateAttributes('paragraph', { indent: Math.max(0, Math.min(10, cur + d)) }).updateAttributes('heading', { indent: Math.max(0, Math.min(10, cur + d)) }).run() }
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
      // Clipboard — the SHARED group (ribbon/clipboardGroup), ALWAYS first on Home.
      // mobileQuick: [] — like Word mobile, the clipboard does not clutter the quick
      // bar (cut/copy/paste stay in the palette + the native context menu).
      clipboardGroup({
        t,
        mobileQuick: [],
        onPaste: async () => { try { const txt = await navigator.clipboard.readText(); body?.chain().focus().insertContent(txt).run() } catch { body?.view.focus(); document.execCommand('paste') } },
        onCut: () => { fmt?.view.focus(); document.execCommand('cut') },
        onCopy: () => { fmt?.view.focus(); document.execCommand('copy') },
        extraItems: [
          // « Reproduire la mise en forme » (Format Painter façon Word) : simple clic =
          // une fois, double clic = mode collant (jusqu'à Échap ou re-clic).
          { id: 'fmtpainter', kind: 'toggle', icon: <DocIcon.FormatPainterIcon size={15} />,
            label: t('doc_format_painter', { defaultValue: 'Reproduire la mise en forme' }),
            tooltip: t('doc_format_painter_tip', { defaultValue: 'Reproduire la mise en forme (Alt+Ctrl+C, Alt+Ctrl+V). Simple clic pour l’appliquer une fois, double clic pour l’appliquer à plusieurs endroits.' }),
            shortcut: 'Alt+Ctrl+C', active: c.formatPainterActive,
            onClick: c.onFormatPainter, onDoubleClick: c.onFormatPainterSticky },
          { id: 'clear', kind: 'button', icon: <Eraser size={15} />, label: t('doc_clear_formatting'), onClick: () => fmt?.chain().focus().clearNodes().unsetAllMarks().run() },
        ],
      }),
      // Barre rapide mobile façon Word : B/I/U/S + couleur/surlignage à un tap
      // (le rendu desktop du groupe reste le bloc custom 2 rangées ci-dessous).
      { id: 'font', label: t('doc_grp_font', { defaultValue: 'Police' }), mobileQuick: [
        { id: 'mq-b', kind: 'toggle', icon: <Bold size={16} />, tooltip: t('doc_bold'), active: isA('bold'), onClick: () => fmt && applyInlineFormat(fmt, { b: !isA('bold') }, cr) },
        { id: 'mq-i', kind: 'toggle', icon: <Italic size={16} />, tooltip: t('doc_italic'), active: isA('italic'), onClick: () => fmt && applyInlineFormat(fmt, { i: !isA('italic') }, cr) },
        { id: 'mq-u', kind: 'toggle', icon: <UnderlineIcon size={16} />, tooltip: t('doc_underline'), active: isA('underline'), onClick: () => fmt && applyInlineFormat(fmt, { u: !isA('underline') }, cr) },
        { id: 'mq-s', kind: 'toggle', icon: <Strikethrough size={16} />, tooltip: t('doc_strikethrough'), active: isA('strike'), onClick: () => fmt && applyInlineFormat(fmt, { s: !isA('strike') }, cr) },
        { id: 'mq-color', kind: 'custom', render: <RibbonColorBtn editor={fmt} kind="text" cellRanges={cr} /> },
        { id: 'mq-highlight', kind: 'custom', render: <RibbonColorBtn editor={fmt} kind="highlight" cellRanges={cr} /> },
      ], items: [
        // Groupe Police en 2 RANGÉES, façon Word (demande user) : à GAUCHE la
        // police+taille avec les styles de caractère (G I U barré indice exposant)
        // DESSOUS ; à DROITE agrandir/réduire + casse avec couleur & surlignage dessous.
        { id: 'fontgroup', kind: 'custom', render: (
          <div className="flex items-stretch gap-2 px-0.5">
            <div className="flex flex-col justify-center gap-1">
              <FontSizeField font={curFont} onFontChange={v => fmt && applyInlineFormat(fmt, { ff: v }, cr)} fonts={c.fonts}
                size={curSize} onSizeChange={v => setSize(parseInt(v, 10))}
                sizes={[8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72]}
                height={26} fontWidth={105} sizeWidth={54} fontSize={11} />
              <div className="flex items-center gap-0.5">
                <FGBtn icon={<Bold size={15} />} active={isA('bold')} title={t('doc_bold')} onClick={() => fmt && applyInlineFormat(fmt, { b: !isA('bold') }, cr)} />
                <FGBtn icon={<Italic size={15} />} active={isA('italic')} title={t('doc_italic')} onClick={() => fmt && applyInlineFormat(fmt, { i: !isA('italic') }, cr)} />
                <FGBtn icon={<UnderlineIcon size={15} />} active={isA('underline')} title={t('doc_underline')} onClick={() => fmt && applyInlineFormat(fmt, { u: !isA('underline') }, cr)} />
                <FGBtn icon={<Strikethrough size={15} />} active={isA('strike')} title={t('doc_strikethrough')} onClick={() => fmt && applyInlineFormat(fmt, { s: !isA('strike') }, cr)} />
                <FGBtn icon={<Subscript size={15} />} active={isA('subscript')} title={t('doc_subscript', { defaultValue: 'Indice' })} onClick={() => fmt?.chain().focus().toggleMark('subscript').run()} />
                <FGBtn icon={<Superscript size={15} />} active={isA('superscript')} title={t('doc_superscript', { defaultValue: 'Exposant' })} onClick={() => fmt?.chain().focus().toggleMark('superscript').run()} />
              </div>
            </div>
            <div className="flex flex-col justify-center gap-1">
              <div className="flex items-center gap-0.5">
                <FGBtn icon={<span style={{ fontSize: 15 }}>A</span>} title={t('doc_increase_font_size')} onClick={() => setSize(curSizeNum + 1)} />
                <FGBtn icon={<span style={{ fontSize: 11 }}>A</span>} title={t('doc_decrease_font_size')} onClick={() => setSize(curSizeNum - 1)} />
                <FGCaseMenu onCase={c.onChangeCase} onSmallCaps={c.onToggleSmallCaps} onSpacing={c.onCharSpacing} />
              </div>
              <div className="flex items-center gap-0.5">
                <RibbonColorBtn editor={fmt} kind="text" cellRanges={cr} />
                <RibbonColorBtn editor={fmt} kind="highlight" cellRanges={cr} />
                <TextEffectsButton editor={fmt} />
              </div>
            </div>
          </div>
        ) },
      ] },
      { id: 'para', label: t('doc_grp_paragraph', { defaultValue: 'Paragraphe' }), items: [
        // Réorganisé en colonnes propres de 3 (sans séparateurs, qui gaspillaient de la
        // largeur et créaient une colonne orpheline « justifier ») : listes · alignement
        // + retraits · interligne · tri/num · retraits 1ʳᵉ ligne · trame · flux.
        { id: 'ul', kind: 'toggle', icon: <DocIcon.BulletListIcon size={15} />, tooltip: t('doc_bullet_list'), active: isA('bulletList'), onClick: () => fmt?.chain().focus().toggleBulletList().run() },
        { id: 'ol', kind: 'toggle', icon: <DocIcon.NumberListIcon size={15} />, tooltip: t('doc_numbered_list'), active: isA('orderedList'), onClick: () => fmt?.chain().focus().toggleOrderedList().run() },
        { id: 'task', kind: 'toggle', icon: <CheckSquare size={15} />, tooltip: t('doc_task_list'), active: isA('taskList'), onClick: () => fmt?.chain().focus().toggleTaskList().run() },
        align('left', <AlignLeft size={15} />, t('doc_align_left')),
        align('center', <AlignCenter size={15} />, t('doc_align_center')),
        align('right', <AlignRight size={15} />, t('doc_align_right')),
        align('justify', <AlignJustify size={15} />, t('doc_align_justify')),
        { id: 'ind-', kind: 'button', icon: <IndentDecrease size={15} />, tooltip: t('doc_decrease_indent'), onClick: () => indent(-1) },
        { id: 'ind+', kind: 'button', icon: <IndentIncrease size={15} />, tooltip: t('doc_increase_indent'), onClick: () => indent(1) },
        { id: 'ls', kind: 'split', icon: <SplitSquareVertical size={15} />, tooltip: t('doc_line_spacing'),
          splitItems: [
            ...[[t('doc_spacing_single'), 1.0], ['1,15', 1.15], ['1,5', 1.5], [t('doc_spacing_double'), 2.0], ['2,5', 2.5], ['3,0', 3.0]].map(([lbl, lh]) => ({ id: 'ls' + lh, kind: 'button' as const, label: String(lbl), active: curLs === lh, onClick: () => setLs(lh as number) })),
            { id: 'ls-sep', kind: 'separator' as const },
            { id: 'sp-before', kind: 'button' as const, label: spBefore > 0 ? t('doc_remove_space_before', { defaultValue: 'Supprimer l’espace avant' }) : t('doc_add_space_before', { defaultValue: 'Ajouter un espace avant' }), onClick: () => setParaAttr({ spaceBefore: spBefore > 0 ? 0 : 12 }) },
            { id: 'sp-after', kind: 'button' as const, label: spAfter > 0 ? t('doc_remove_space_after', { defaultValue: 'Supprimer l’espace après' }) : t('doc_add_space_after', { defaultValue: 'Ajouter un espace après' }), onClick: () => setParaAttr({ spaceAfter: spAfter > 0 ? 0 : 12 }) },
          ] },
        { id: 'sortasc', kind: 'button', icon: <ArrowDownAZ size={15} />, tooltip: t('doc_sort_asc', { defaultValue: 'Trier A → Z' }), onClick: () => c.onSortParas('asc') },
        { id: 'sortdesc', kind: 'button', icon: <ArrowUpAZ size={15} />, tooltip: t('doc_sort_desc', { defaultValue: 'Trier Z → A' }), onClick: () => c.onSortParas('desc') },
        { id: 'hnum', kind: 'toggle', icon: <ListTree size={15} />, tooltip: t('doc_heading_numbers', { defaultValue: 'Numéroter les titres (1., 1.1, …)' }), active: c.headingNumbers, onClick: c.onToggleHeadingNumbers },
        { id: 'firstline', kind: 'toggle', icon: <Pilcrow size={14} className="opacity-70" />, tooltip: t('doc_first_line_indent', { defaultValue: 'Retrait de première ligne' }), active: ((paraAttrs.indentFirstLine as number) ?? 0) > 0, onClick: () => setParaAttr({ indentFirstLine: ((paraAttrs.indentFirstLine as number) ?? 0) > 0 ? 0 : 36 }) },
        { id: 'hanging', kind: 'toggle', icon: <IndentDecrease size={14} className="opacity-70" />, tooltip: t('doc_hanging_indent', { defaultValue: 'Retrait négatif (suspendu)' }), active: ((paraAttrs.indentFirstLine as number) ?? 0) < 0, onClick: () => { const on = ((paraAttrs.indentFirstLine as number) ?? 0) < 0; setParaAttr({ indentFirstLine: on ? 0 : -36, indentLeft: on ? ((paraAttrs.indentLeft as number) ?? 0) : Math.max(36, (paraAttrs.indentLeft as number) ?? 0) }) } },
        { id: 'parashade', kind: 'custom', render: <RibbonParaShadeBtn shading={curShading} border={curParaBorder} onShading={col => setParaAttr({ shading: col })} onBorder={b => setParaAttr({ paraBorder: b })} /> },
        { id: 'paraflow', kind: 'custom', render: <RibbonParaFlowBtn attrs={paraAttrs} onSet={setParaAttr} /> },
      ] },
      { id: 'styles', label: t('doc_grp_styles', { defaultValue: 'Styles' }), items: [
        { id: 'stylegallery', kind: 'custom', render: <RibbonStyleGallery
            styles={c.styleList} curId={c.curStyleId} onApply={c.onApplyStyle}
            onManage={c.onEditStyles} onClear={c.onClearAllFormatting} /> },
      ] },
      { id: 'edit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'link', kind: 'split', icon: <DocIcon.LinkIcon size={15} />, label: t('doc_insert_link'), active: isA('link'), onClick: c.onLink,
          splitItems: [
            { id: 'link-web', kind: 'button' as const, label: t('doc_insert_link', { defaultValue: 'Lien hypertexte' }), onClick: c.onLink },
            { id: 'link-mail', kind: 'button' as const, label: t('doc_email_link', { defaultValue: 'Lien e-mail' }), onClick: c.onEmailLink },
            { id: 'link-rm', kind: 'button' as const, label: t('doc_remove_links', { defaultValue: 'Supprimer tous les liens' }), onClick: c.onRemoveLinks },
          ] },
        { id: 'bookmark', kind: 'button', icon: <DocIcon.BookmarkIcon size={15} />, label: t('doc_bookmark', { defaultValue: 'Signet' }), onClick: c.onInsertBookmark },
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
        { id: 'cover', kind: 'split', icon: <DocIcon.CoverPageIcon size={15} />, tooltip: t('doc_cover_page', { defaultValue: 'Page de garde' }),
          splitItems: [
            { id: 'cover1', kind: 'button' as const, label: t('doc_cover_centered', { defaultValue: 'Centrée' }), onClick: () => c.onInsertCoverPage(1) },
            { id: 'cover2', kind: 'button' as const, label: t('doc_cover_left', { defaultValue: 'Alignée à gauche' }), onClick: () => c.onInsertCoverPage(2) },
          ] },
        { id: 'pb', kind: 'button', icon: <DocIcon.PageBreakIcon size={15} />, label: t('doc_page_break'), onClick: c.onPageBreak },
        { id: 'sb', kind: 'button', icon: <SplitSquareVertical size={15} />, label: t('doc_section_break_next_page', { defaultValue: 'Saut de section' }), onClick: c.onSectionBreak },
      ] },
      { id: 'tables', label: t('doc_grp_tables', { defaultValue: 'Tableaux' }), items: [
        // Gros bouton à menu (façon Word) : grille de survol pour les petits
        // tableaux + repli en saisie libre et conversion depuis du texte.
        { id: 'table', kind: 'menu', size: 'large', icon: <DocIcon.TableIcon size={32} />, label: t('doc_table', { defaultValue: 'Tableau' }),
          menuRender: close => (
            <div className="min-w-[210px]">
              <TableGridPicker
                hint={t('doc_insert_table', { defaultValue: 'Insérer un tableau' })}
                title={(r, cc) => t('doc_table_dims_preview', { defaultValue: 'Tableau {{r}}x{{c}}', r, c: cc })}
                onPick={(r, cc) => { close(); c.onInsertTableSize(r, cc) }} />
              <div className="mt-1 pt-1 border-t border-border flex flex-col">
                <button type="button" className="flex items-center gap-2 px-2 py-1.5 text-xs text-text-primary hover:bg-surface-hover text-left"
                  onMouseDown={e => e.preventDefault()} onClick={() => { close(); c.onInsertTable() }}>
                  <TableIcon size={14} /> {t('doc_insert_table_dots', { defaultValue: 'Insérer un tableau…' })}
                </button>
                <button type="button" className="flex items-center gap-2 px-2 py-1.5 text-xs text-text-primary hover:bg-surface-hover text-left"
                  onMouseDown={e => e.preventDefault()} onClick={() => { close(); c.onConvertTextTable() }}>
                  <TableIcon size={14} /> {t('doc_text_to_table_dots', { defaultValue: 'Convertir le texte en tableau…' })}
                </button>
              </div>
            </div>
          ) },
        { id: 'txt2tbl', kind: 'button', icon: <TableIcon size={14} />, label: t('doc_text_to_table', { defaultValue: 'Texte → tableau' }), onClick: c.onConvertTextTable },
        { id: 'tbl2txt', kind: 'button', icon: <WrapText size={14} />, label: t('doc_table_to_text', { defaultValue: 'Tableau → texte' }), onClick: c.onConvertTableText },
      ] },
      // Groupe « Illustrations » PARTAGÉ (galerie de formes commune à toute la
      // suite) : « Image » et « Zone de texte » l'encadrent, la galerie ARME le
      // tracé au lieu d'insérer une forme à taille fixe.
      docIllustrationsGroup({
        t: c.tf,
        armed: c.armedShape,
        onArm: c.onArmShape,
        groupLabel: t('doc_grp_illustrations', { defaultValue: 'Illustrations' }),
        imageItem: { id: 'img', kind: 'button', size: 'large', icon: <DocIcon.ImageIcon size={32} />, label: t('doc_image', { defaultValue: 'Image' }), onClick: c.onInsertImage },
        textBoxItem: { id: 'textbox', kind: 'button', icon: <DocIcon.TextBoxIcon size={14} />, label: t('doc_text_box', { defaultValue: 'Zone de texte' }), onClick: c.onInsertTextBox },
      }),
      { id: 'hf', label: t('doc_grp_header_footer', { defaultValue: 'En-tête et pied' }), items: [
        { id: 'header', kind: 'button', icon: <DocIcon.HeaderIcon size={15} />, label: t('doc_header', { defaultValue: 'En-tête' }), onClick: c.onSetHeader },
        { id: 'footer', kind: 'button', icon: <DocIcon.FooterIcon size={15} />, label: t('doc_footer', { defaultValue: 'Pied de page' }), onClick: c.onSetFooter },
        { id: 'pgnum', kind: 'dropdown', value: c.pageNumbers, width: 120, options: ([['none', 'Aucun'], ['footer-right', 'Pied · droite'], ['footer-center', 'Pied · centre'], ['header-right', 'Haut · droite'], ['header-center', 'Haut · centre']] as Array<[PageNumbers, string]>).map(([v, l]) => ({ value: v, label: l })), onChange: v => c.onPageNumbers(v as PageNumbers) },
        { id: 'pgnumfmt', kind: 'custom', render: c.pageNumFormatNode },
      ] },
      { id: 'text', label: t('doc_grp_text', { defaultValue: 'Texte' }), items: [
        { id: 'toc', kind: 'split', icon: <DocIcon.TocIcon size={15} />, tooltip: t('doc_toc', { defaultValue: 'Table des matières' }),
          splitItems: [
            { id: 'toc-ins', kind: 'button' as const, label: t('doc_toc_insert', { defaultValue: 'Insérer la table des matières…' }), onClick: c.onInsertToc },
            { id: 'toc-upd', kind: 'button' as const, label: t('doc_toc_update', { defaultValue: 'Mettre à jour la table' }), onClick: c.onTocUpdate },
          ] },
        { id: 'fld-upd', kind: 'button', icon: <RotateCw size={15} />, label: t('doc_fields_update', { defaultValue: 'Actualiser les champs' }), onClick: c.onFieldsUpdate },
        { id: 'special', kind: 'button', icon: <DocIcon.SymbolIcon size={15} />, label: t('doc_special_chars', { defaultValue: 'Caractères spéciaux' }), onClick: c.onSpecialChars },
        { id: 'datetime', kind: 'split', icon: <DocIcon.DateTimeIcon size={15} />, tooltip: t('doc_insert_datetime', { defaultValue: 'Date et heure' }),
          splitItems: ([['date', t('doc_field_date', { defaultValue: 'Date' })], ['time', t('doc_field_time', { defaultValue: 'Heure' })], ['datetime', t('doc_field_datetime', { defaultValue: 'Date et heure' })]] as Array<['date' | 'time' | 'datetime', string]>).map(([k, lbl]) => ({ id: 'fld-' + k, kind: 'button' as const, label: lbl, onClick: () => c.onInsertField(k) })) },
        { id: 'caption', kind: 'button', icon: <DocIcon.CaptionIcon size={15} />, label: t('doc_caption', { defaultValue: 'Légende' }), onClick: c.onInsertCaption },
        { id: 'hr', kind: 'button', icon: <DocIcon.HorizontalRuleIcon size={15} />, label: t('doc_horizontal_rule', { defaultValue: 'Trait horizontal' }), onClick: c.onInsertHr },
        { id: 'quote', kind: 'toggle', icon: <DocIcon.QuoteIcon size={15} />, label: t('doc_blockquote', { defaultValue: 'Citation' }), active: isA('blockquote'), onClick: () => fmt?.chain().focus().toggleBlockquote().run() },
        { id: 'code', kind: 'toggle', icon: <DocIcon.CodeBlockIcon size={15} />, label: t('doc_code_block', { defaultValue: 'Bloc de code' }), active: isA('codeBlock'), onClick: () => fmt?.chain().focus().toggleCodeBlock().run() },
        { id: 'footnote', kind: 'button', icon: <DocIcon.FootnoteIcon size={15} />, label: t('doc_footnote', { defaultValue: 'Note de bas de page' }), onClick: c.onInsertFootnote },
        { id: 'endnote', kind: 'button', icon: <DocIcon.EndnoteIcon size={15} />, label: t('doc_endnote', { defaultValue: 'Note de fin' }), onClick: c.onInsertEndnote },
        { id: 'dropcap', kind: 'toggle', icon: <Type size={15} />, label: t('doc_drop_cap', { defaultValue: 'Lettrine' }), active: !!fmt?.getAttributes('paragraph').dropCap, onClick: () => { const cur = !!fmt?.getAttributes('paragraph').dropCap; fmt?.chain().focus().updateAttributes('paragraph', { dropCap: !cur }).run() } },
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
        { id: 'edit', kind: 'toggle', size: 'large', icon: <FileText size={32} />, label: t('doc_mode_edit', { defaultValue: 'Édition' }), active: c.mode === 'edit', onClick: () => c.onMode('edit') },
        { id: 'read', kind: 'toggle', size: 'large', icon: <Eye size={32} />, label: t('doc_mode_read', { defaultValue: 'Lecture' }), active: c.mode === 'read', onClick: () => c.onMode('read') },
      ] },
      { id: 'show', label: t('doc_grp_show', { defaultValue: 'Afficher' }), items: [
        { id: 'ruler', kind: 'toggle', icon: <RulerIcon size={15} />, label: t('doc_ruler', { defaultValue: 'Règle' }), active: c.showRuler, onClick: c.onToggleRuler },
        { id: 'nav', kind: 'toggle', icon: <PanelLeft size={15} />, label: t('doc_nav_pane', { defaultValue: 'Volet de navigation' }), active: c.navOpen, onClick: c.onToggleNav },
        { id: 'bounds', kind: 'toggle', icon: <Frame size={15} />, label: t('doc_text_boundaries', { defaultValue: 'Limites du texte' }), active: c.showBoundaries, onClick: c.onToggleBoundaries },
        { id: 'marks', kind: 'toggle', icon: <Pilcrow size={15} />, label: t('doc_formatting_marks', { defaultValue: 'Marques ¶' }), active: c.showMarks, onClick: c.onToggleMarks },
        { id: 'outarrows', kind: 'toggle', icon: <ChevronRight size={15} />, label: t('doc_collapsible_headings', { defaultValue: 'Titres repliables' }), active: c.outlineArrows, onClick: c.onToggleOutlineArrows },
      ] },
      // Groupe Zoom façon Word : deux gros boutons (Zoom, 100 %) puis les trois
      // ajustements empilés. Le curseur −/+ n'y est pas — il vit dans la barre
      // d'état, exactement comme chez Word.
      { id: 'zoom', label: t('doc_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'zdlg', kind: 'button', size: 'large', icon: <ZoomIn size={32} />, label: t('doc_zoom', { defaultValue: 'Zoom' }), tooltip: t('doc_zoom_dialog', { defaultValue: 'Boîte de dialogue Zoom' }), onClick: c.onZoomDialog },
        { id: 'z100', kind: 'button', size: 'large', label: t('doc_zoom_100', { defaultValue: '100 %' }),
          icon: (
            <span className="relative inline-flex items-center justify-center" style={{ width: 32, height: 32 }}>
              <FileText size={32} />
              <span style={{ position: 'absolute', right: -4, bottom: -1, fontSize: 8, fontWeight: 700,
                             background: '#1a73e8', color: '#fff', borderRadius: 2, padding: '0 2px', lineHeight: '10px' }}>100</span>
            </span>
          ),
          onClick: () => c.onZoom(1) },
        { id: 'z1p', kind: 'button', icon: <FileText size={15} />, label: t('doc_zoom_one_page', { defaultValue: 'Une page' }), onClick: () => c.onZoomFit('page') },
        { id: 'zmp', kind: 'button', icon: <Files size={15} />, label: t('doc_zoom_multi_page', { defaultValue: 'Plusieurs pages' }), onClick: () => c.onZoomFit('multi') },
        { id: 'zpw', kind: 'button', icon: <MoveHorizontal size={15} />, label: t('doc_zoom_page_width', { defaultValue: 'Largeur de la page' }), onClick: () => c.onZoomFit('width') },
      ] },
      // Groupe « Macros » façon Word : un gros bouton à menu, dont la 3ᵉ entrée
      // n'est active QUE pendant un enregistrement.
      { id: 'macros', label: t('macros_title', { defaultValue: 'Macros' }), items: [
        { id: 'macros', kind: 'menu', size: 'large', icon: <PlaySquare size={32} />,
          label: t('macros_title', { defaultValue: 'Macros' }),
          splitItems: [
            { id: 'mac-show', kind: 'button' as const, label: t('macros_show', { defaultValue: 'Afficher les macros' }), onClick: c.onShowMacros },
            { id: 'mac-rec', kind: 'button' as const,
              label: c.recState === 'idle'
                ? t('macro_record', { defaultValue: 'Enregistrer une macro…' })
                : t('macro_stop_record', { defaultValue: "Arrêter l'enregistrement" }),
              onClick: c.onToggleRecord },
            { id: 'mac-pause', kind: 'button' as const, disabled: c.recState === 'idle',
              label: c.recState === 'paused'
                ? t('macro_resume_record', { defaultValue: "Reprendre l'enregistrement" })
                : t('macro_pause_record', { defaultValue: "Suspendre l'enregistrement" }),
              onClick: c.onPauseRecord },
          ] },
      ] },
    ],
  }

  // Onglet contextuel IMAGE (objet sélectionné dans le corps).
  // Le nœud porteur peut être `image` (bloc) OU `inlineImage` (aligné sur le
  // texte) : l'onglet ignorait le second, donc une image posée dans le flux
  // n'avait AUCUN onglet de format. On met à jour le nœud réellement sélectionné.
  const sel = body?.state.selection
  const selNode = sel instanceof NodeSelection ? sel.node : null
  const isInlineImg = selNode?.type.name === 'inlineImage'
  const isImgNode = selNode?.type.name === 'image' || isInlineImg
  const selNodeName = isInlineImg ? 'inlineImage' : 'image'
  // Une FORME (`kbshape:`) a son propre onglet, partagé avec toute la suite.
  const selShapeParams = isImgNode ? parseShapeAlt(selNode?.attrs.alt as string) : null
  const imgAttr = (a: Record<string, unknown>) => body?.chain().focus().updateAttributes(selNodeName, a).run()
  const imageTab: RibbonTab = {
    id: 'ctx-image', label: t('doc_tab_image', { defaultValue: "Format de l'image" }), contextual: { accent: '#9334e6' },
    visible: !!isImgNode && !selShapeParams,
    groups: [
      { id: 'arrange', label: t('doc_grp_arrange', { defaultValue: 'Disposition' }), items: [
        // Alignement et habillage n'ont de sens que pour un objet de BLOC : une
        // image alignée sur le texte est un caractère, elle suit son paragraphe.
        ...(isInlineImg ? [] : [
          { id: 'ial', kind: 'button' as const, icon: <AlignLeft size={15} />, label: t('doc_align_left'), onClick: () => imgAttr({ align: 'left' }) },
          { id: 'iac', kind: 'button' as const, icon: <AlignCenter size={15} />, label: t('doc_align_center'), onClick: () => imgAttr({ align: 'center' }) },
          { id: 'iar', kind: 'button' as const, icon: <AlignRight size={15} />, label: t('doc_align_right'), onClick: () => imgAttr({ align: 'right' }) },
          { id: 'iwrap', kind: 'dropdown' as const, value: (selNode?.attrs.wrap as string) || 'inline', width: 120, options: [['inline', 'Aligné texte'], ['square', 'Carré'], ['topBottom', 'Haut et bas'], ['behind', 'Derrière'], ['front', 'Devant']].map(([v, l]) => ({ value: v, label: l })), onChange: (v: string) => imgAttr({ wrap: v }) },
        ]),
        { id: 'ireset', kind: 'button', icon: <RotateCcw size={15} />, label: t('doc_reset_image', { defaultValue: 'Réinitialiser' }), onClick: () => imgAttr({ width: 0, height: 0, rotation: 0 }) },
      ] },
    ],
  }

  // Onglet contextuel FORME — le « Format de la forme » PARTAGÉ de la suite
  // (remplissage / contour / épaisseur, ordre de plan, rotation par quart de
  // tour, suppression), enrichi de l'habillage et de l'alignement propres au
  // texte. Il remplace le sous-menu contextuel « Couleur de la forme » maison.
  const shapeTab: RibbonTab = docShapeRibbonTab({
    t: c.tf,
    params: selShapeParams,
    size: { w: Number(selNode?.attrs.width) || 240, h: Number(selNode?.attrs.height) || 180 },
    rotation: Number(selNode?.attrs.rotation) || 0,
    wrap: (selNode?.attrs.wrap as string) || 'inline',
    inline: !!isInlineImg,
    update: imgAttr,
    setWrap: w => c.onShapeWrap(w),
    order: op => c.onShapeOrder(op),
    remove: () => body?.chain().focus().deleteSelection().run(),
    openLayout: c.onShapeLayout,
  })

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
        { id: 'hfswitch', kind: 'button', size: 'large', icon: <SplitSquareVertical size={32} />, label: c.hf?.band === 'header' ? t('doc_goto_footer', { defaultValue: 'Aller au pied' }) : t('doc_goto_header', { defaultValue: "Aller à l'en-tête" }), onClick: c.onHFSwitch },
        { id: 'hffirst', kind: 'toggle', icon: <FileText size={15} />, label: t('doc_first_page_diff', { defaultValue: '1ʳᵉ page différente' }), active: !!c.hf?.firstPage, onClick: () => c.onHFFirstPage(!c.hf?.firstPage) },
        { id: 'hflink', kind: 'toggle', icon: <LinkIcon size={15} />, label: t('doc_link_previous', { defaultValue: 'Lier au précédent' }), disabled: !c.hf?.canLink, active: !!c.hf?.linked, onClick: () => c.onHFLinked(!c.hf?.linked) },
      ] },
      { id: 'hfclose', label: t('doc_grp_close', { defaultValue: 'Fermer' }), items: [
        { id: 'hfx', kind: 'button', size: 'large', icon: <X size={32} />, label: t('doc_close_hf', { defaultValue: 'Fermer' }), onClick: c.onHFClose },
      ] },
    ],
  }

  const review: RibbonTab = {
    id: 'review', label: t('doc_tab_review', { defaultValue: 'Révision' }),
    groups: [
      { id: 'proofing', label: t('doc_grp_proofing', { defaultValue: 'Vérification' }), items: [
        { id: 'spell', kind: 'toggle', size: 'large', icon: <SpellCheck size={32} />,
          label: c.spellOn ? `${t('doc_spell', { defaultValue: 'Orthographe' })}${c.spellCount ? ` (${c.spellCount})` : ''}` : t('doc_spell_off', { defaultValue: 'Désactivé' }),
          active: c.spellOn, onClick: c.onToggleSpell },
        { id: 'spelldict', kind: 'button', icon: <BookMarked size={15} />, label: t('doc_spell_dictionary', { defaultValue: 'Dictionnaire personnel' }), onClick: c.onSpellDictionary },
      ] },
      { id: 'tracking', label: t('doc_grp_tracking', { defaultValue: 'Suivi' }), items: [
        { id: 'trackchanges', kind: 'toggle', size: 'large', icon: <PenLine size={32} />, label: t('doc_track_changes', { defaultValue: 'Suivi des modifications' }), active: c.trackChanges, onClick: c.onToggleTrackChanges },
        { id: 'reviewpane', kind: 'toggle', icon: <ListChecks size={15} />, label: t('doc_review_pane', { defaultValue: 'Volet Vérifications' }), active: c.reviewOpen, onClick: c.onToggleReview },
        { id: 'revfinal', kind: 'toggle', icon: <Eye size={15} />, label: t('doc_review_show_final', { defaultValue: 'Afficher le document final' }), active: c.revFinal, onClick: c.onToggleRevFinal },
      ] },
      { id: 'comments', label: t('doc_grp_comments', { defaultValue: 'Commentaires' }), items: [
        { id: 'addcomment', kind: 'button', size: 'large', icon: <DocIcon.CommentIcon size={32} />, label: t('doc_new_comment', { defaultValue: 'Nouveau commentaire' }), onClick: c.onAddComment },
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
        { id: 'merge', kind: 'button', size: 'large', icon: <Combine size={32} />, label: t('doc_merge_cells', { defaultValue: 'Fusionner' }), disabled: !c.table.canMerge, onClick: c.table.onMerge },
        { id: 'split', kind: 'button', icon: <SplitSquareVertical size={15} />, label: t('doc_split_cell', { defaultValue: 'Fractionner' }), onClick: c.table.onSplit },
      ] },
      { id: 'tstyle', label: t('doc_grp_table_style', { defaultValue: 'Style' }), items: [
        { id: 'tst', kind: 'dropdown', value: c.table.curStyle, width: 130, options: ([['grid', 'Grille'], ['header', 'En-tête'], ['striped', 'Bandes'], ['plain', 'Sans bordure']] as Array<[string, string]>).map(([v, l]) => ({ value: v, label: t(`doc_tstyle_${v}`, { defaultValue: l }) })), onChange: v => c.table!.onStyle(v) },
        { id: 'cellcolor', kind: 'custom', render: c.table.cellColorNode },
        { id: 'dtable', kind: 'button', icon: <Trash2 size={15} />, label: t('doc_delete_table', { defaultValue: 'Supprimer le tableau' }), onClick: c.table.onDeleteTable },
      ] },
      { id: 'tvalign', label: t('doc_grp_cell_align', { defaultValue: 'Alignement' }), items: [
        { id: 'vtop', kind: 'toggle', icon: <AlignStartHorizontal size={15} />, tooltip: t('doc_valign_top', { defaultValue: 'Aligner en haut' }), active: c.table.curCellVAlign === 'top', onClick: () => c.table!.onCellVAlign('top') },
        { id: 'vmid', kind: 'toggle', icon: <AlignCenterHorizontal size={15} />, tooltip: t('doc_valign_center', { defaultValue: 'Centrer verticalement' }), active: c.table.curCellVAlign === 'center', onClick: () => c.table!.onCellVAlign('center') },
        { id: 'vbot', kind: 'toggle', icon: <AlignEndHorizontal size={15} />, tooltip: t('doc_valign_bottom', { defaultValue: 'Aligner en bas' }), active: c.table.curCellVAlign === 'bottom', onClick: () => c.table!.onCellVAlign('bottom') },
      ] },
      { id: 'tborders', label: t('doc_grp_table_borders', { defaultValue: 'Bordures' }), items: [
        { id: 'bcolor', kind: 'custom', render: c.table.borderColorNode },
        { id: 'bwidth', kind: 'dropdown', value: String(c.table.curBorderWidth), width: 88, options: ([[1, '½ pt'], [1.5, '1 pt'], [2, '1½ pt'], [3, '2¼ pt'], [4, '3 pt']] as Array<[number, string]>).map(([v, l]) => ({ value: String(v), label: l })), onChange: v => c.table!.onBorder({ tableBorderWidth: parseFloat(v) }) },
        { id: 'bstyle', kind: 'dropdown', value: c.table.curBorderStyle, width: 96, options: ([['solid', 'Plein'], ['dashed', 'Tirets'], ['dotted', 'Points']] as Array<[string, string]>).map(([v, l]) => ({ value: v, label: t(`doc_bstyle_${v}`, { defaultValue: l }) })), onChange: v => c.table!.onBorder({ tableBorderStyle: v === 'solid' ? null : v }) },
      ] },
      { id: 'tlayout', label: t('doc_grp_table_layout', { defaultValue: 'Disposition' }), items: [
        { id: 'tsortaz', kind: 'button', icon: <ArrowDownAZ size={15} />, label: t('doc_table_sort_az', { defaultValue: 'Trier A → Z' }), onClick: () => c.table!.onSort('asc') },
        { id: 'tsortza', kind: 'button', icon: <ArrowUpAZ size={15} />, label: t('doc_table_sort_za', { defaultValue: 'Trier Z → A' }), onClick: () => c.table!.onSort('desc') },
        { id: 'tdistr', kind: 'button', icon: <Rows3 size={15} />, label: t('doc_distribute_rows', { defaultValue: 'Uniformiser les lignes' }), onClick: () => c.table!.onDistribute('rows') },
        { id: 'tdistc', kind: 'button', icon: <Columns3 size={15} />, label: t('doc_distribute_cols', { defaultValue: 'Uniformiser les colonnes' }), onClick: () => c.table!.onDistribute('cols') },
        { id: 'tfit', kind: 'split', icon: <SquareDashed size={15} />, tooltip: t('doc_autofit', { defaultValue: 'Ajustement automatique' }),
          splitItems: [
            { id: 'fitc', kind: 'button' as const, label: t('doc_autofit_content', { defaultValue: 'Ajuster au contenu' }), onClick: () => c.table!.onAutoFit('content') },
            { id: 'fitw', kind: 'button' as const, label: t('doc_autofit_window', { defaultValue: 'Ajuster à la fenêtre' }), onClick: () => c.table!.onAutoFit('window') },
            { id: 'fitf', kind: 'button' as const, label: t('doc_autofit_fixed', { defaultValue: 'Largeur de colonne fixe' }), onClick: () => c.table!.onAutoFit('fixed') },
          ] },
        { id: 'tsplit', kind: 'button', icon: <SplitSquareVertical size={15} />, label: t('doc_split_table', { defaultValue: 'Fractionner le tableau' }), onClick: c.table.onSplitTable },
        { id: 'tsum', kind: 'split', icon: <Sigma size={15} />, tooltip: t('doc_table_formula', { defaultValue: 'Formule' }),
          splitItems: [
            { id: 'suma', kind: 'button' as const, label: t('doc_sum_above', { defaultValue: 'Somme au-dessus (=SUM(ABOVE))' }), onClick: () => c.table!.onSum('above') },
            { id: 'suml', kind: 'button' as const, label: t('doc_sum_left', { defaultValue: 'Somme à gauche (=SUM(LEFT))' }), onClick: () => c.table!.onSum('left') },
          ] },
      ] },
    ] : [],
  }

  // Onglet « Fichier » (Backstage façon Office) en 1ʳᵉ position.
  const file: RibbonTab = {
    id: 'file', label: t('doc_bs_file', { defaultValue: 'Fichier' }), groups: [],
    backstage: c.fileBackstage,
  }
  // Word's order: Fichier, Accueil, Insertion, Mise en page, Références, …
  return [file, home, insert, layout, buildReferencesTab(c.references), view, review, shapeTab, imageTab, hfTab, tableTab]
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

// Position écran (repère du conteneur d'overlays, qui défile avec le contenu) de
// l'ancre d'un commentaire, calculée par PaginatedEditor.
interface CommentAnchorScreen { pageIdx: number; top: number; lineH: number; pageRight: number; anchorX: number }
// Action déclenchée depuis le menu contextuel (texte ou carte) : éditer / répondre.
interface CommentExtAction { id: string; mode: 'edit' | 'reply'; ts: number }

const CARD_W = 268           // largeur d'une carte de commentaire (px écran)
const CARD_STACK_GAP = 10    // espace vertical minimal entre deux cartes

// Édition en place d'un texte (commentaire principal ou réponse) : petite zone
// multi-lignes + Enregistrer / Annuler (façon Word « Modifier le commentaire »).
function CommentEditBox({ initial, onSave, onCancel, t }: {
  initial: string
  onSave: (text: string) => void
  onCancel: () => void
  t: (k: string, o?: Record<string, unknown>) => string
}) {
  const [text, setText] = useState(initial)
  return (
    <div onClick={e => e.stopPropagation()}>
      <textarea autoFocus value={text} onChange={e => setText(e.target.value)}
        onFocus={e => { const v = e.currentTarget.value; e.currentTarget.setSelectionRange(v.length, v.length) }}
        onKeyDown={e => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { const v = text.trim(); if (v) onSave(v) }
          if (e.key === 'Escape') onCancel()
        }}
        rows={Math.min(6, Math.max(2, text.split('\n').length))}
        className="w-full resize-none px-2 py-1.5 text-xs border border-primary rounded outline-none leading-snug" />
      <div className="mt-1.5 flex items-center justify-end gap-1.5">
        <button onClick={onCancel} className="text-[11px] px-2 py-1 rounded hover:bg-surface-2 text-text-secondary">{t('common_cancel', { defaultValue: 'Annuler' })}</button>
        <button onClick={() => { const v = text.trim(); if (v) onSave(v) }} disabled={!text.trim()}
          className="text-[11px] px-2.5 py-1 rounded bg-primary text-white hover:bg-primary-hover disabled:opacity-40">{t('common_save', { defaultValue: 'Enregistrer' })}</button>
      </div>
    </div>
  )
}

// Initiales (1-2 lettres) + couleur déterministe d'un auteur, pour la pastille
// d'avatar des cartes de commentaires (repérage visuel de l'auteur).
function authorInitials(name: string): string {
  const parts = (name || '?').trim().split(/\s+/).filter(Boolean)
  if (!parts.length) return '?'
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}
const AVATAR_COLORS = ['#5b8def', '#e8710a', '#12a150', '#d93025', '#8e24aa', '#00897b', '#f9a825', '#3949ab']
function authorColor(name: string): string {
  let h = 0
  for (let i = 0; i < (name || '').length; i++) h = (h * 31 + name.charCodeAt(i)) & 0xffff
  return AVATAR_COLORS[h % AVATAR_COLORS.length]
}
function CommentAvatar({ name, size = 22 }: { name: string; size?: number }) {
  return (
    <span className="inline-flex items-center justify-center rounded-full text-white font-semibold flex-shrink-0"
      style={{ width: size, height: size, background: authorColor(name), fontSize: size * 0.42 }}>
      {authorInitials(name)}
    </span>
  )
}

// Une carte de commentaire (fil) : ancrée dans la marge droite, façon Word/Google
// Docs. Édition en place du commentaire et des réponses, réponse, résolution/
// réouverture, suppression, et menu contextuel (clic droit).
function CommentCard({ th, commentsMap, editor, user, active, onActivate, autoMode, onConsumeAuto }: {
  th: CommentThread
  commentsMap: Y.Map<CommentThread>
  editor: Editor | null
  user: { id: string; name: string }
  active: boolean
  onActivate: () => void
  autoMode: 'edit' | 'reply' | null
  onConsumeAuto: () => void
}) {
  const { t } = useTranslation('office')
  const [replying, setReplying] = useState(false)
  const [replyText, setReplyText] = useState('')
  const [editingMain, setEditingMain] = useState(false)
  const [editingReply, setEditingReply] = useState<number | null>(null)
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)
  const mine = th.authorId === user.id

  // Réagit à une action venue du menu contextuel (Modifier / Répondre).
  useEffect(() => {
    if (!autoMode) return
    if (autoMode === 'edit' && mine) setEditingMain(true)
    if (autoMode === 'reply') { setReplying(true); setReplyText('') }
    onConsumeAuto()
  }, [autoMode, mine, onConsumeAuto])

  const patch = (next: Partial<CommentThread>) => commentsMap.set(th.id, { ...th, ...next })
  const resolve = () => {
    commentsMap.set(th.id, { ...th, resolved: !th.resolved })
    if (!th.resolved && editor) unsetCommentMark(editor, th.id)
  }
  const remove = () => { commentsMap.delete(th.id); if (editor) unsetCommentMark(editor, th.id) }
  const sendReply = () => {
    const text = replyText.trim(); if (!text) return
    patch({ replies: [...(th.replies ?? []), { author: user.name, authorId: user.id, text, createdAt: Date.now() }] })
    setReplyText(''); setReplying(false)
  }
  const saveReply = (i: number, text: string) => {
    const replies = (th.replies ?? []).map((r, k) => k === i ? { ...r, text } : r)
    patch({ replies }); setEditingReply(null)
  }
  const removeReply = (i: number) => patch({ replies: (th.replies ?? []).filter((_, k) => k !== i) })

  const menuItems: MenuItem[] = [
    ...(mine ? [{ type: 'action' as const, label: t('doc_comment_edit', { defaultValue: 'Modifier le commentaire' }), onClick: () => setEditingMain(true) }] : []),
    { type: 'action', label: t('doc_comment_reply_menu', { defaultValue: 'Répondre au commentaire' }), onClick: () => { setReplying(true); setReplyText('') } },
    { type: 'action', label: th.resolved ? t('doc_comment_reopen_menu', { defaultValue: 'Rouvrir le commentaire' }) : t('doc_comment_resolve_menu', { defaultValue: 'Marquer comme résolu' }), onClick: resolve },
    ...(mine ? [{ type: 'action' as const, label: t('doc_comment_delete_menu', { defaultValue: 'Supprimer le commentaire' }), onClick: remove }] : []),
  ]

  return (
    <div onClick={onActivate}
      onContextMenu={e => { e.preventDefault(); e.stopPropagation(); onActivate(); setMenu({ x: e.clientX, y: e.clientY }) }}
      className={`rounded cursor-pointer bg-white transition-all ${th.resolved ? 'opacity-70' : ''}`}
      style={{
        padding: 10,
        border: `1px solid ${active ? '#ef6c00' : '#e2e2e2'}`,
        background: active ? '#fffaf2' : '#fff',
        boxShadow: active ? 'none' : '0 1px 4px rgba(0,0,0,0.10)',
      }}>
      {th.quote && <div className="text-[11px] text-text-tertiary pl-1.5 mb-1.5 line-clamp-2 italic" style={{ borderLeft: '2px solid #ddd' }}>{th.quote}</div>}
      <div className="flex items-center gap-2 mb-1.5">
        <CommentAvatar name={th.author} />
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold text-text-primary truncate leading-tight">{th.author}</div>
          <div className="text-[10px] text-text-tertiary leading-tight">{relTime(th.createdAt, t)}</div>
        </div>
      </div>
      {editingMain
        ? <CommentEditBox initial={th.text} t={t} onCancel={() => setEditingMain(false)}
            onSave={txt => { patch({ text: txt }); setEditingMain(false) }} />
        : <p className="text-xs text-text-secondary whitespace-pre-wrap break-words">{th.text}</p>}
      {(th.replies ?? []).map((r, i) => (
        <div key={i} className="mt-2 pl-2 border-l border-border group/reply">
          <div className="flex items-center gap-1.5">
            <CommentAvatar name={r.author} size={16} />
            <span className="text-[11px] font-semibold text-text-primary truncate flex-1 min-w-0">{r.author}</span>
            <span className="text-[10px] text-text-tertiary flex items-center gap-1 flex-shrink-0">
              {relTime(r.createdAt, t)}
              {r.authorId === user.id && editingReply !== i && (
                <span className="hidden group-hover/reply:flex items-center gap-0.5" onClick={e => e.stopPropagation()}>
                  <button title={t('doc_comment_edit', { defaultValue: 'Modifier' })} onClick={() => setEditingReply(i)} className="p-0.5 rounded hover:bg-surface-2"><Pencil size={11} /></button>
                  <button title={t('common_delete', { defaultValue: 'Supprimer' })} onClick={() => removeReply(i)} className="p-0.5 rounded hover:bg-surface-2 text-danger"><Trash2 size={11} /></button>
                </span>
              )}
            </span>
          </div>
          {editingReply === i
            ? <CommentEditBox initial={r.text} t={t} onCancel={() => setEditingReply(null)} onSave={txt => saveReply(i, txt)} />
            : <p className="text-xs text-text-secondary whitespace-pre-wrap break-words">{r.text}</p>}
        </div>
      ))}
      {replying ? (
        <div className="mt-2 flex items-center gap-1" onClick={e => e.stopPropagation()}>
          <input autoFocus value={replyText} onChange={e => setReplyText(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') sendReply(); if (e.key === 'Escape') { setReplying(false); setReplyText('') } }}
            placeholder={t('doc_reply_placeholder', { defaultValue: 'Répondre…' })}
            className="flex-1 min-w-0 h-7 px-2 text-xs border border-border rounded outline-none focus:border-primary" />
          <button onClick={sendReply} className="p-1.5 rounded bg-primary text-white hover:bg-primary-hover"><Send size={12} /></button>
        </div>
      ) : !editingMain && (
        <div className="mt-2 flex items-center gap-0.5 text-text-secondary" onClick={e => e.stopPropagation()}>
          <button onClick={() => { setReplying(true); setReplyText('') }} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2"><CornerDownRight size={12} /> {t('doc_reply', { defaultValue: 'Répondre' })}</button>
          <button onClick={resolve} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2"><Check size={12} /> {th.resolved ? t('doc_reopen', { defaultValue: 'Rouvrir' }) : t('doc_resolve', { defaultValue: 'Résoudre' })}</button>
          {mine && <button title={t('doc_comment_edit', { defaultValue: 'Modifier' })} onClick={() => setEditingMain(true)} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2"><Pencil size={12} /></button>}
          {mine && <button onClick={remove} className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded hover:bg-surface-2 text-danger ml-auto"><Trash2 size={12} /></button>}
        </div>
      )}
      {menu && <MenuDropdown items={menuItems} pos={{ top: menu.y, left: menu.x, minWidth: 220 }} onClose={() => setMenu(null)} />}
    </div>
  )
}

// Marge de commentaires ancrée (façon Word) : chaque fil non résolu est posé à
// droite de sa page, aligné verticalement sur l'ancre, avec évitement des
// chevauchements et un trait de liaison pour le fil actif.
function CommentGutter({ commentsMap, editor, user, activeId, setActiveId, anchoredIds, anchorScreen, tick, action, onConsumeAction }: {
  commentsMap: Y.Map<CommentThread>
  editor: Editor | null
  user: { id: string; name: string }
  activeId: string | null
  setActiveId: (id: string | null) => void
  anchoredIds: string[]
  anchorScreen: (id: string) => CommentAnchorScreen | null
  tick: number
  action: CommentExtAction | null
  onConsumeAction: () => void
}) {
  const [, forceTick] = useState(0)
  useEffect(() => {
    const fn = () => forceTick(n => (n + 1) & 0xffff)
    commentsMap.observe(fn)
    return () => commentsMap.unobserve(fn)
  }, [commentsMap])

  const cardRefs = useRef<Map<string, HTMLDivElement>>(new Map())
  const [heights, setHeights] = useState<Record<string, number>>({})
  useLayoutEffect(() => {
    let changed = false
    const next = { ...heights }
    for (const [id, el] of cardRefs.current) { const h = el.offsetHeight; if (next[id] !== h) { next[id] = h; changed = true } }
    if (changed) setHeights(next)
  })

  // Fils visibles = non résolus et encore ancrés dans le texte (comme Word, un fil
  // résolu quitte la marge). Positionnés par ancre puis empilés sans chevauchement.
  void tick; void anchoredIds
  const placed: Array<{ th: CommentThread; anc: CommentAnchorScreen; top: number }> = []
  const items = ([...commentsMap.values()].filter(Boolean) as CommentThread[])
    .filter(th => !th.resolved)
    .map(th => ({ th, anc: anchorScreen(th.id) }))
    .filter((x): x is { th: CommentThread; anc: CommentAnchorScreen } => x.anc != null)
    .sort((a, b) => a.anc.top - b.anc.top)
  let cursor = -Infinity
  for (const { th, anc } of items) {
    const top = Math.max(anc.top, cursor)
    placed.push({ th, anc, top })
    cursor = top + (heights[th.id] ?? 96) + CARD_STACK_GAP
  }

  return (
    <>
      {placed.map(({ th, anc, top }) => {
        const left = anc.pageRight + 16
        const isActive = activeId === th.id
        // Trait de liaison EN COUDE (façon Google Docs) : de l'ancre du texte
        // (point ambre au bord de la page) jusqu'au bord gauche de la carte. Dessiné
        // pour TOUS les fils (fin/pâle) et renforcé pour le fil actif → repérage.
        const cx0 = anc.pageRight, cy0 = anc.top + anc.lineH / 2
        const cx1 = left, cy1 = top + 16
        const midX = (cx0 + cx1) / 2
        const boxL = Math.min(cx0, cx1) - 4, boxT = Math.min(cy0, cy1) - 4
        const boxW = Math.abs(cx1 - cx0) + 8, boxH = Math.abs(cy1 - cy0) + 8
        const col = isActive ? '#ef6c00' : 'rgba(249,168,37,0.55)'
        return (
          <Fragment key={th.id}>
            <svg style={{ position: 'absolute', left: boxL, top: boxT, width: boxW, height: boxH, overflow: 'visible', pointerEvents: 'none', zIndex: isActive ? 29 : 26 }}>
              <path d={`M ${cx0 - boxL} ${cy0 - boxT} H ${midX - boxL} V ${cy1 - boxT} H ${cx1 - boxL}`}
                fill="none" stroke={col} strokeWidth={isActive ? 2 : 1.25} strokeLinejoin="round" strokeLinecap="round" />
              <circle cx={cx0 - boxL} cy={cy0 - boxT} r={isActive ? 3.5 : 2.5} fill={col} />
            </svg>
            <div ref={el => { if (el) cardRefs.current.set(th.id, el); else cardRefs.current.delete(th.id) }}
              style={{ position: 'absolute', left, top, width: CARD_W, zIndex: isActive ? 29 : 28 }}>
              <CommentCard th={th} commentsMap={commentsMap} editor={editor} user={user} active={isActive}
                onActivate={() => setActiveId(th.id)}
                autoMode={action && action.id === th.id ? action.mode : null}
                onConsumeAuto={onConsumeAction} />
            </div>
          </Fragment>
        )
      })}
    </>
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
function RibbonCellColorBtn({ editor, onPick, title }: { editor: Editor | null; onPick: (hex: string | null) => void; title?: string }) {
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
        title={title ?? t('doc_cell_shading', { defaultValue: 'Trame de fond' })}>
        <Paintbrush size={14} />
        <div className="w-4 h-1 rounded-sm border border-border/40" style={{ background: cur ?? 'transparent' }} />
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <div className="p-3 w-[244px] flex flex-col gap-2 text-sm bg-white border border-border rounded-lg shadow-lg" data-module="office">
          <div className="font-medium text-text-primary">{title ?? t('doc_cell_shading', { defaultValue: 'Trame de fond' })}</div>
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
          <Button className={DLG_BTN} variant="primary" size="sm" onClick={save}>{t('doc_apply', { defaultValue: 'Appliquer' })}</Button>
          <Button className={DLG_BTN} variant="secondary" size="sm" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}

// Catalogue des langues de vérification (noms français, façon Word). Les langues
// disposant d'un dictionnaire Hunspell chargé (FR/EN bundlés + dictionnaires système)
// sont marquées d'une icône ✓ et proposées en tête du sélecteur.
const LANG_CATALOG: Record<string, string> = {
  af: 'Afrikaans', sq: 'Albanais', de: 'Allemand (Allemagne)', en: 'Anglais (États-Unis)',
  ar: 'Arabe', hy: 'Arménien', eu: 'Basque', bg: 'Bulgare', ca: 'Catalan', zh: 'Chinois (simplifié)',
  ko: 'Coréen', hr: 'Croate', da: 'Danois', es: 'Espagnol (Espagne)', et: 'Estonien', fi: 'Finnois',
  fr: 'Français (France)', el: 'Grec', he: 'Hébreu', hi: 'Hindi', hu: 'Hongrois', id: 'Indonésien',
  it: 'Italien', ja: 'Japonais', lv: 'Letton', lt: 'Lituanien', mk: 'Macédonien', ms: 'Malais',
  nl: 'Néerlandais', nb: 'Norvégien (Bokmål)', fa: 'Persan', pl: 'Polonais', pt: 'Portugais (Portugal)',
  ro: 'Roumain', ru: 'Russe', sr: 'Serbe', sk: 'Slovaque', sl: 'Slovène', sv: 'Suédois', cs: 'Tchèque',
  th: 'Thaï', tr: 'Turc', uk: 'Ukrainien', vi: 'Vietnamien',
}
const langCatalogLabel = (code: string): string => LANG_CATALOG[code] ?? code.toUpperCase()

// Onglet « Langue » : sélecteur de langue de vérification (façon Word). État CONTRÔLÉ
// par la fenêtre (le pied OK/Annuler est commun aux onglets). La langue s'applique au
// document actif (moteur Hunspell) ; « Détecter automatiquement » interroge tous les
// dictionnaires chargés (document multilingue).
function LanguageTab({ lang, onLang, auto, onAuto, check, onCheck, scope, onScope, hasSelection }: {
  lang: string; onLang: (v: string) => void
  auto: boolean; onAuto: (v: boolean) => void
  check: boolean; onCheck: (v: boolean) => void
  scope: 'selection' | 'document'; onScope: (s: 'selection' | 'document') => void
  hasSelection: boolean
}) {
  const { t } = useTranslation('office')

  // Liste : langues à dictionnaire en tête (avec ✓), puis le reste du catalogue,
  // chacune triée par nom français.
  const availSet = useMemo(() => new Set(availableSpellLangs().map(a => a.lang)), [])
  const list = useMemo(() => {
    const avail = [...availSet].map(l => ({ lang: l, has: true }))
    const rest  = Object.keys(LANG_CATALOG).filter(l => !availSet.has(l)).map(l => ({ lang: l, has: false }))
    const byName = (a: { lang: string }, b: { lang: string }) => langCatalogLabel(a.lang).localeCompare(langCatalogLabel(b.lang), 'fr')
    return [...avail.sort(byName), ...rest.sort(byName)]
  }, [availSet])

  return (
    <div className="flex flex-col gap-3">
      {/* Portée de la modification */}
      <div>
        <p className="text-sm text-text-primary mb-1.5">{t('doc_lang_scope', { defaultValue: 'Modifier la langue de vérification pour :' })}</p>
        <div className="flex items-center gap-5 text-sm"
          title={hasSelection ? undefined : t('doc_lang_scope_sel_hint', { defaultValue: 'Sélectionnez du texte pour appliquer une langue à une plage' })}>
          <Radio checked={scope === 'selection'} disabled={!hasSelection} onChange={() => onScope('selection')} label={t('doc_lang_scope_selection', { defaultValue: 'Texte sélectionné' })} />
          <Radio checked={scope === 'document'} onChange={() => onScope('document')} label={t('doc_lang_scope_document', { defaultValue: 'Document actif' })} />
        </div>
      </div>

      {/* Liste des langues */}
      <div>
        <p className="text-xs text-text-secondary mb-1">{scope === 'selection'
          ? t('doc_lang_selection_is', { defaultValue: 'La sélection est en :' })
          : t('doc_lang_document_is', { defaultValue: 'Le document est en :' })}</p>
        <div className="h-56 overflow-auto border border-border rounded-lg">
          {list.map(({ lang: code, has }) => (
            <button key={code} type="button" onClick={() => onLang(code)}
              className={`flex items-center gap-2 w-full text-left px-3 py-1.5 text-sm border-b border-border/60 last:border-b-0
                ${lang === code ? 'bg-primary text-white' : 'text-text-primary hover:bg-black/5'}`}>
              <span className="w-4 flex-shrink-0 flex justify-center">
                {has && <SpellCheck size={13} className={lang === code ? 'text-white' : 'text-primary'} />}
              </span>
              {langCatalogLabel(code)}
            </button>
          ))}
        </div>
      </div>

      <p className="text-xs text-text-tertiary leading-snug">
        {t('doc_lang_help', { defaultValue: 'Le vérificateur d\'orthographe utilise automatiquement le dictionnaire de la langue sélectionnée s\'il est disponible (icône). Les autres langues seront prises en charge dès qu\'un dictionnaire correspondant sera ajouté.' })}
      </p>

      {/* Options (document uniquement — sans objet pour une plage) */}
      {scope === 'document' && <>
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <Checkbox checked={!check} onChange={v => onCheck(!v)} />
          <span>{t('doc_lang_no_check', { defaultValue: 'Ne pas vérifier l\'orthographe' })}</span>
        </label>
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <Checkbox checked={auto} onChange={onAuto} />
          <span>{t('doc_lang_auto_detect', { defaultValue: 'Détecter automatiquement la langue (multilingue)' })}</span>
        </label>
      </>}
    </div>
  )
}

// Onglet « Dictionnaire personnel » : mots ajoutés via « Ajouter au dictionnaire » —
// lister, retirer, ajouter manuellement. `onChange` redéclenche la vérification.
function PersonalDictTab({ onChange }: { onChange: () => void }) {
  const { t } = useTranslation('office')
  const [, tick] = useState(0)
  const [draft, setDraft] = useState('')
  const words = personalDictionary()
  const remove = (w: string) => { unignoreWord(w); tick(n => n + 1); onChange() }
  const add = () => { const w = draft.trim(); if (!w) return; ignoreWord(w); setDraft(''); tick(n => n + 1); onChange() }
  return (
    <div className="flex flex-col flex-1 min-h-0">
      <div className="flex items-center gap-1.5 mb-3 flex-shrink-0">
        <input value={draft} onChange={e => setDraft(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') add() }}
          placeholder={t('doc_spell_add_word', { defaultValue: 'Ajouter un mot…' })}
          className="flex-1 min-w-0 h-8 px-2 text-sm border border-border rounded outline-none focus:border-primary" />
        <Button variant="primary" size="sm" onClick={add} disabled={!draft.trim()}>{t('doc_spell_add', { defaultValue: 'Ajouter' })}</Button>
      </div>
      <div className={`flex-1 min-h-0 overflow-auto border border-border rounded-lg ${words.length ? 'divide-y divide-border' : 'flex items-center justify-center'}`}>
        {!words.length && (
          <p className="text-xs text-text-tertiary text-center px-4">{t('doc_spell_dict_empty', { defaultValue: 'Aucun mot ajouté.' })}</p>
        )}
        {words.map(w => (
          <div key={w} className="flex items-center justify-between px-3 py-1.5">
            <span className="text-sm text-text-primary truncate">{w}</span>
            <button onClick={() => remove(w)} title={t('doc_spell_remove_word', { defaultValue: 'Retirer' })}
              className="p-1 rounded hover:bg-danger-light text-text-tertiary hover:text-danger flex-shrink-0"><Trash2 size={14} /></button>
          </div>
        ))}
      </div>
    </div>
  )
}

// Fenêtre « Langue et dictionnaire » : onglet Langue (sélecteur de langue de
// vérification) + onglet Dictionnaire personnel. `onChange` redéclenche la vérification.
function SpellDictionaryDialog({ editor, spellLang, spellAuto, spellOn, onApplyLang, onChange, onClose }: {
  editor: Editor | null
  spellLang: string; spellAuto: boolean; spellOn: boolean
  onApplyLang: (v: { lang: string; auto: boolean; check: boolean; scope: 'selection' | 'document' }, asDefault?: boolean) => void
  onChange: () => void; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<'lang' | 'dict'>('lang')
  // Brouillon de langue REMONTÉ ici : le pied OK/Annuler est commun aux onglets.
  const [lang, setLang]   = useState(spellLang)
  const [auto, setAuto]   = useState(spellAuto)
  const [check, setCheck] = useState(spellOn)
  // Portée : « Texte sélectionné » (langue par plage) n'est possible que s'il y a une
  // sélection non vide. On capture l'état au MONTAGE (le clic dans le dialogue ne modifie
  // pas la sélection de l'éditeur). Défaut « Document actif » (façon Word).
  const hasSelection = !!editor && editor.state.selection.from < editor.state.selection.to
  const [scope, setScope] = useState<'selection' | 'document'>('document')
  const ok = () => { onApplyLang({ lang, auto, check, scope }); onClose() }
  const setDefault = () => onApplyLang({ lang, auto, check, scope: 'document' }, true)
  return (
    <FloatingWindow title={t('doc_lang_dict_title', { defaultValue: 'Langue et dictionnaire' })} onClose={onClose} defaultWidth={460} backdrop>
      <div className="flex flex-col min-h-0 flex-1" data-module="office">
        {/* Onglets */}
        <Tabs className="px-5" size="sm" value={tab} onChange={v => setTab(v as 'lang' | 'dict')}
          tabs={[{ id: 'lang', label: t('doc_lang_tab', { defaultValue: 'Langue' }) },
                 { id: 'dict', label: t('doc_spell_dictionary', { defaultValue: 'Dictionnaire personnel' }) }]} />
        {/* Contenu de l'onglet (colonne flex → l'onglet peut remplir la hauteur) */}
        <div className="flex-1 min-h-0 overflow-auto px-5 py-4 flex flex-col">
          {tab === 'lang'
            ? <LanguageTab lang={lang} onLang={setLang} auto={auto} onAuto={setAuto} check={check} onCheck={setCheck}
                scope={scope} onScope={setScope} hasSelection={hasSelection} />
            : <PersonalDictTab onChange={onChange} />}
        </div>
        {/* Pied commun (HORS onglet) : Définir par défaut · Annuler · OK */}
        <div className="flex items-center justify-between gap-2 px-5 py-3 border-t border-border flex-shrink-0">
          <Button variant="secondary" size="sm" onClick={setDefault}>{t('doc_lang_set_default', { defaultValue: 'Définir par défaut' })}</Button>
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={onClose}>{t('common.cancel', { defaultValue: 'Annuler' })}</Button>
            <Button variant="primary" size="sm" onClick={ok}>{t('common.ok', { defaultValue: 'OK' })}</Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}

// Correspondance message de faute → catégorie de règle (pour « Ne pas rechercher ce problème »).
const MSG_TO_RULE: Record<string, string> = {
  'Espace avant la ponctuation':  'punctSpace',
  'Espace en trop':               'wordSpace',
  'Majuscule en début de phrase': 'sentenceCap',
  'Mot répété':                   'repeatedWord',
}

// Onglet « Grammaire » du panneau Vérification : explication de la règle, phrase fautive
// (avec lecture audio), suggestions cliquables, actions Ignorer / Ne pas rechercher.
function GrammarCheckTab({ issue, editor, onCorrect, onIgnoreOnce, onDisableRule }: {
  issue: SpellIssue; editor: Editor | null
  onCorrect: (s: string) => void; onIgnoreOnce: () => void; onDisableRule: () => void
}) {
  const { t } = useTranslation('office')
  // Contexte : le texte du bloc contenant la faute + position surlignée.
  const ctx = useMemo(() => {
    if (!editor) return null
    try {
      const $p = editor.state.doc.resolve(issue.from)
      const bStart = $p.start($p.depth), bEnd = $p.end($p.depth)
      const text = editor.state.doc.textBetween(bStart, bEnd, ' ', ' ')
      return { text, hFrom: Math.max(0, issue.from - bStart), hTo: Math.max(0, issue.to - bStart) }
    } catch { return { text: issue.word, hFrom: 0, hTo: issue.word.length } }
  }, [editor, issue])
  const speak = () => {
    try { const u = new SpeechSynthesisUtterance(ctx?.text ?? issue.word); u.lang = 'fr-FR'; speechSynthesis.cancel(); speechSynthesis.speak(u) } catch { /* pas de TTS */ }
  }
  const sugg = issue.suggestions.filter(s => s !== ' ')
  return (
    <div className="flex flex-col gap-3 h-full">
      <div>
        <h3 className="text-base font-semibold text-text-primary">{t('doc_grammar_category', { defaultValue: 'Grammaire' })}</h3>
        <p className="text-sm text-text-secondary mt-0.5">{GRAMMAR_EXPLAIN[issue.message ?? ''] ?? issue.message ?? ''}</p>
      </div>
      {/* Phrase avec la faute surlignée + lecture audio */}
      <div className="flex items-start gap-2 border border-border rounded-lg px-3 py-2.5">
        <p className="flex-1 text-sm text-text-primary leading-relaxed">
          {ctx && <>{ctx.text.slice(0, ctx.hFrom)}<span className="text-primary underline decoration-wavy">{ctx.text.slice(ctx.hFrom, ctx.hTo)}</span>{ctx.text.slice(ctx.hTo)}</>}
        </p>
        <button type="button" onClick={speak} title={t('doc_grammar_read', { defaultValue: 'Lire à voix haute' })}
          className="flex-shrink-0 p-1 rounded text-text-tertiary hover:text-text-primary hover:bg-surface-2"><Volume2 size={16} /></button>
      </div>
      {/* Suggestions */}
      <div className="flex-1 min-h-0 flex flex-col">
        <p className="text-xs text-text-secondary mb-1">{t('doc_grammar_suggestions', { defaultValue: 'Suggestions' })}</p>
        <div className="flex-1 min-h-0 overflow-auto border border-border rounded-lg divide-y divide-border">
          {sugg.length ? sugg.map((s, i) => (
            <button key={i} type="button" onClick={() => onCorrect(s)}
              className="flex items-center w-full text-left px-3 py-2 text-sm font-medium text-text-primary hover:bg-primary/10">
              {s === '' ? t('doc_grammar_remove_space', { defaultValue: 'Supprimer l\'espace' }) : s}
            </button>
          )) : <p className="text-xs text-text-tertiary text-center py-6">{t('doc_spell_no_suggestion', { defaultValue: 'Aucune suggestion' })}</p>}
        </div>
      </div>
      {/* Actions bas de l'onglet (façon Word) */}
      <div className="flex flex-col gap-1 text-sm">
        <button type="button" onClick={onIgnoreOnce} className="text-left text-text-secondary hover:text-text-primary hover:underline">{t('doc_grammar_ignore_once', { defaultValue: 'Ignorer une fois' })}</button>
        <button type="button" onClick={onDisableRule} className="text-left text-text-secondary hover:text-text-primary hover:underline">{t('doc_grammar_disable_rule', { defaultValue: 'Ne pas rechercher ce problème' })}</button>
      </div>
    </div>
  )
}

// Onglet « Options » du panneau Vérification : liste des règles de grammaire (cases),
// façon Word (« Règle de style : Grammaire »). État CONTRÔLÉ (pied OK/Annuler commun).
function GrammarOptionsTab({ draft, onDraft }: {
  draft: Record<string, boolean>; onDraft: (d: Record<string, boolean>) => void
}) {
  const { t } = useTranslation('office')
  const set = (key: string, on: boolean) => onDraft({ ...draft, [key]: on })
  return (
    <div className="flex flex-col gap-2 h-full">
      <div>
        <p className="text-sm text-text-primary mb-1">{t('doc_grammar_style_rule', { defaultValue: 'Règle de style :' })}</p>
        <div className="h-9 px-3 flex items-center text-sm bg-primary text-white rounded border border-primary">{t('doc_grammar_category', { defaultValue: 'Grammaire' })}</div>
      </div>
      <p className="text-sm text-text-primary mt-1">{t('doc_grammar_options', { defaultValue: 'Options :' })}</p>
      <div className="flex-1 min-h-0 overflow-auto border border-border rounded-lg p-2">
        {GRAMMAR_RULES.map(r => (
          <label key={r.key} className="flex items-center gap-2 px-1.5 py-1 text-sm cursor-pointer rounded hover:bg-black/5">
            <Checkbox checked={draft[r.key] !== false} onChange={v => set(r.key, v)} />
            <span className="text-text-primary">{r.label}</span>
          </label>
        ))}
      </div>
      <button type="button" onClick={() => onDraft({})} className="self-start text-sm text-primary hover:underline">{t('doc_grammar_reset_all', { defaultValue: 'Rétablir tout' })}</button>
    </div>
  )
}

// Fenêtre « Vérification » (façon Word) : onglet Grammaire (faute courante) + onglet
// Options (règles). OK/Annuler communs HORS des onglets. OK applique les règles.
function VerificationDialog({ issue, editor, rules, onApplyRules, onRecheck, onClose }: {
  issue: SpellIssue; editor: Editor | null
  rules: Record<string, boolean>
  onApplyRules: (r: Record<string, boolean>) => void
  onRecheck: () => void; onClose: () => void
}) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<'check' | 'options'>('check')
  const [draft, setDraft] = useState<Record<string, boolean>>(rules)
  const correct = (s: string) => { editor?.chain().focus().insertContentAt({ from: issue.from, to: issue.to }, s).run(); onRecheck(); onClose() }
  const ignoreOnce = () => { ignoreWordSession(grammarIgnoreKey(issue.message, issue.word)); onRecheck(); onClose() }
  const disableRule = () => { const k = MSG_TO_RULE[issue.message ?? '']; if (k) onApplyRules({ ...rules, [k]: false }); onRecheck(); onClose() }
  const ok = () => { onApplyRules(draft); onRecheck(); onClose() }
  return (
    <FloatingWindow title={t('doc_grammar_verify_title', { defaultValue: 'Vérification' })} onClose={onClose} defaultWidth={460} backdrop>
      <div className="flex flex-col min-h-0 flex-1" data-module="office">
        <Tabs className="px-5" size="sm" value={tab} onChange={v => setTab(v as 'check' | 'options')}
          tabs={[{ id: 'check', label: t('doc_grammar_category', { defaultValue: 'Grammaire' }) },
                 { id: 'options', label: t('doc_grammar_options_tab', { defaultValue: 'Options' }) }]} />
        <div className="flex-1 min-h-0 overflow-auto px-5 py-4 flex flex-col">
          {tab === 'check'
            ? <GrammarCheckTab issue={issue} editor={editor} onCorrect={correct} onIgnoreOnce={ignoreOnce} onDisableRule={disableRule} />
            : <GrammarOptionsTab draft={draft} onDraft={setDraft} />}
        </div>
        {/* Pied commun (HORS onglets) : Annuler · OK */}
        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border flex-shrink-0">
          <Button variant="secondary" size="sm" onClick={onClose}>{t('common.cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" size="sm" onClick={ok}>{t('common.ok', { defaultValue: 'OK' })}</Button>
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
  spellOn, spellCount, onToggleSpell, onOpenSpell, spellLang, spellAuto, grammarOn, onToggleGrammar, showRuler, onToggleRuler }: {
  editor: Editor | null
  pages: number; current: number
  zoom: number; onZoom: (z: number) => void
  mode: 'edit' | 'read'; onMode: (m: 'edit' | 'read') => void
  spellOn: boolean; spellCount: number; onToggleSpell: () => void; onOpenSpell: () => void
  spellLang: string; spellAuto: boolean
  grammarOn: boolean; onToggleGrammar: () => void
  showRuler: boolean; onToggleRuler: () => void
}) {
  const { t } = useTranslation('office')
  const isMobileStatus = useIsMobile()
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

  const langLabel = STATUS_LANG_LABELS[spellLang] ?? spellLang.toUpperCase()
  const pct = Math.round(zoom * 100)
  const stepZoom = (d: number) => onZoom(Math.min(3, Math.max(0.25, Math.round((zoom + d) * 100) / 100)))

  const settingsItems: MenuItem[] = [
    { type: 'action', label: t('doc_show_ruler', { defaultValue: 'Règle' }), checked: showRuler, onClick: onToggleRuler },
    { type: 'action', label: t('doc_spellcheck', { defaultValue: 'Correcteur orthographique' }), checked: spellOn, onClick: onToggleSpell },
    { type: 'action', label: t('doc_grammarcheck', { defaultValue: 'Grammaire' }), checked: grammarOn, onClick: onToggleGrammar },
  ]

  // Mobile: no status bar — the bottom edge belongs to the mobile ribbon.
  if (isMobileStatus) return null
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
      <StatusButton onClick={onOpenSpell}
        title={spellAuto
          ? t('doc_status_lang_auto', { lang: langLabel, defaultValue: `Langue de vérification : ${langLabel} (détection automatique)` })
          : t('doc_status_lang_title', { lang: langLabel, defaultValue: `Langue de vérification : ${langLabel}` })}>
        <Languages size={14} /> {langLabel}
      </StatusButton>
      {/* Accessibilité : affiché UNIQUEMENT quand il reste des problèmes (pas de
          « vérification terminée » qui encombre la barre quand tout est correct). */}
      {a11y > 0 && (
        <StatusButton active
          title={t('doc_status_a11y_issues', { count: a11y, defaultValue: `${a11y} image(s) sans texte alternatif` })}>
          <Accessibility size={14} />
          <span className="hidden sm:inline">
            {t('doc_status_a11y_short', { count: a11y, defaultValue: `Accessibilité : ${a11y} à corriger` })}
          </span>
        </StatusButton>
      )}

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

// ── Dictée vocale mobile (speech-to-text) façon Word ─────────────────────────
// Micro flottant (édition mobile) → PANNEAU de dictée : dicte EN DIRECT dans le
// document (segments finalisés insérés au curseur), avec les OUTILS de Word :
// statut (« À l'écoute… »), insertions rapides (virgule/point/?/espace/saut de
// ligne), réglages (langue parlée, ponctuation auto), gros micro (pause/reprise),
// retour arrière. STT bas niveau auto-hébergé (startVoiceSession), gating admin
// via /stt/status (bouton masqué si off).
// ⚠️ Le backend STT (Vosk) résout le modèle par code de langue à 2 LETTRES
// (cf. stt/catalog.rs `m.lang`, et resolve_for_lang) — PAS en BCP-47. Envoyer
// « fr-FR » échoue le matching (`"fr" == "fr-fr"` faux) → repli sur n'importe
// quel modèle installé = l'anglais. On travaille donc en 2 lettres partout.
const DICT_LANGS: Array<{ code: string; label: string }> = [
  { code: 'fr', label: 'Français' }, { code: 'en', label: 'English' },
  { code: 'es', label: 'Español' }, { code: 'de', label: 'Deutsch' },
  { code: 'it', label: 'Italiano' }, { code: 'pt', label: 'Português' },
  { code: 'ru', label: 'Русский' }, { code: 'ar', label: 'العربية' },
  { code: 'zh', label: '中文' }, { code: 'hi', label: 'हिन्दी' }, { code: 'ja', label: '日本語' },
  { code: 'el', label: 'Ελληνικά' },
]
const toLang2 = (lang: string) => (lang || 'fr').slice(0, 2).toLowerCase()

// Commandes vocales de dictée (FR + EN de base) : dites à voix haute, elles
// exécutent l'action au lieu de s'écrire. `helpers` = insertions de l'éditeur.
interface DictHelpers { insertPunct: (p: string) => void; insertBreak: () => void; backspace: () => void; ed: () => Editor | null }
const VOICE_COMMANDS: Array<{ re: RegExp; run: (h: DictHelpers) => void }> = [
  { re: /^(nouvelle ligne|à la ligne|saut de ligne|new line)$/,        run: h => h.insertBreak() },
  { re: /^(nouveau paragraphe|new paragraph)$/,                        run: h => h.ed()?.chain().splitBlock().run() },
  { re: /^(point|full stop|period)$/,                                  run: h => h.insertPunct('.') },
  { re: /^(virgule|comma)$/,                                           run: h => h.insertPunct(',') },
  { re: /^(point d'interrogation|point interrogation|question mark)$/, run: h => h.insertPunct('?') },
  { re: /^(point d'exclamation|point exclamation|exclamation mark)$/,  run: h => h.insertPunct('!') },
  { re: /^(deux points|colon)$/,                                       run: h => h.insertPunct(':') },
  { re: /^(point virgule|semicolon)$/,                                 run: h => h.insertPunct(';') },
  { re: /^(supprimer|effacer|delete|backspace)$/,                      run: h => h.backspace() },
]
// Filtre des expressions sensibles : masque les grossièretés courantes (FR/EN)
// par une initiale + astérisques. Liste volontairement minimale et sobre.
const PROFANITY_RE = /\b(merdes?|putains?|connards?|connasses?|salopes?|encul[ée]s?|bites?|couilles?|fuck(?:ing|ers?|ed)?|shit(?:ty)?|bitch(?:es)?|asshole|cunt|dick)\b/gi
const maskProfanity = (s: string) => s.replace(PROFANITY_RE, w => w[0] + '*'.repeat(Math.max(1, w.length - 1)))

function DictationFab({ editorRef, lang }: { editorRef: React.RefObject<Editor | null>; lang: string }) {
  const { t } = useTranslation('office')
  const { data: sttStatus } = useQuery({
    queryKey: ['stt-status', 'office-doc'],
    queryFn: () => api.get<{ enabled: boolean }>('/stt/status').then(r => r.data),
    retry: false, staleTime: 60_000,
  })
  const enabled = !!sttStatus?.enabled

  const [phase, setPhase] = useState<'idle' | 'connecting' | 'listening' | 'paused'>('idle')
  const [interim, setInterim] = useState('')
  const [level, setLevel] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const [showSettings, setShowSettings] = useState(false)
  // 2 lettres, avec migration d'une éventuelle valeur BCP-47 stockée (« fr-FR »).
  const [spoken, setSpoken] = useState<string>(() => toLang2(localStorage.getItem('kb.office.dictLang') || lang))
  const [autoPunct, setAutoPunct] = useState<boolean>(() => localStorage.getItem('kb.office.dictAutoPunct') !== '0')
  const [voiceCmds, setVoiceCmds] = useState<boolean>(() => localStorage.getItem('kb.office.dictVoiceCmds') === '1')
  const [filterProfanity, setFilterProfanity] = useState<boolean>(() => localStorage.getItem('kb.office.dictFilter') !== '0')
  const autoPunctRef = useRef(autoPunct); autoPunctRef.current = autoPunct
  const voiceCmdsRef = useRef(voiceCmds); voiceCmdsRef.current = voiceCmds
  const filterRef = useRef(filterProfanity); filterRef.current = filterProfanity
  const sessionRef = useRef<VoiceSession | null>(null)

  // Barre du bas (ruban) collée au clavier → le panneau se pose au même endroit.
  const [kbInset, setKbInset] = useState(0)
  useEffect(() => {
    const vv = window.visualViewport
    if (!vv) return
    const update = () => {
      const inset = Math.max(0, Math.round(window.innerHeight - vv.height - vv.offsetTop))
      setKbInset(inset < 60 ? 0 : inset)
    }
    vv.addEventListener('resize', update); vv.addEventListener('scroll', update); update()
    return () => { vv.removeEventListener('resize', update); vv.removeEventListener('scroll', update) }
  }, [])

  const ed = () => editorRef.current
  // Insertions SANS `.focus()` : refocaliser le PM masqué convoquerait le clavier
  // (le panneau de dictée le REMPLACE, façon Word). ProseMirror dispatche la
  // transaction sur l'état (éditable) sans focus DOM ; le rendu (canvas) suit via
  // onUpdate. La sélection posée par l'utilisateur (tap dans le doc) persiste dans
  // l'état, donc l'insertion tombe au bon endroit.
  const insertFinal = (seg: string) => {
    const e = ed(); let s = seg.trim(); if (!e || !s) return
    if (filterRef.current) s = maskProfanity(s)   // filtre des expressions sensibles
    const { from } = e.state.selection
    const before2 = e.state.doc.textBetween(Math.max(0, from - 2), from)
    let text = s
    if (autoPunctRef.current) {
      const atStart = from <= 1 || /[.!?]\s?$|\n$/.test(before2) || before2 === ''
      if (atStart) text = text.charAt(0).toUpperCase() + text.slice(1)
    }
    const needSpace = from > 1 && !/\s$/.test(before2)
    e.chain().insertContent((needSpace ? ' ' : '') + text).run()
  }
  const insertPunct = (p: string) => {
    const e = ed(); if (!e) return
    const { from } = e.state.selection
    const before = e.state.doc.textBetween(Math.max(0, from - 1), from)
    // Ponctuation collée au mot précédent : retire une espace de fin s'il y en a.
    e.chain().deleteRange({ from: before === ' ' ? from - 1 : from, to: from }).insertContent(p + ' ').run()
  }
  const insertSpace = () => ed()?.chain().insertContent(' ').run()
  const insertBreak = () => ed()?.chain().setHardBreak().run()
  const backspace = () => {
    const e = ed(); if (!e) return
    const { from, empty } = e.state.selection
    if (!empty) { e.chain().deleteSelection().run(); return }
    if (from > 1) e.chain().deleteRange({ from: from - 1, to: from }).run()
  }
  // Commandes vocales : un segment reconnu qui EST une commande (ex. « point »,
  // « nouvelle ligne », « supprimer ») exécute l'action au lieu de s'écrire. Le
  // reste est dicté normalement. Renvoie true si une commande a été exécutée.
  const runVoiceCommand = (seg: string): boolean => {
    const k = seg.trim().toLowerCase().replace(/[.,;:!?]+$/, '')
    for (const c of VOICE_COMMANDS) if (c.re.test(k)) { c.run({ insertPunct, insertBreak, backspace, ed }); return true }
    return false
  }

  const stopSession = () => { sessionRef.current?.stop(); sessionRef.current = null }
  const start = () => {
    setError(null); setInterim(''); setPhase('connecting')
    // Le panneau REMPLACE le clavier → on le referme (blur de la saisie active).
    ;(document.activeElement as HTMLElement | null)?.blur?.()
    void (async () => {
      try {
        const session = await startVoiceSession(spoken, {
          onReady:  () => setPhase('listening'),
          onLevel:  (lv) => setLevel(lv),
          onPartial: (p) => setInterim(p),
          onResult:  (f) => { setInterim(''); if (voiceCmdsRef.current && runVoiceCommand(f)) return; insertFinal(f) },
          onError:  (code: VoiceErrorCode) => {
            stopSession()
            setPhase('idle')
            setError(t(`doc_dictate_err_${code}`, { defaultValue:
              code === 'not-allowed' ? 'Micro non autorisé.' : code === 'audio-capture' ? 'Aucun micro détecté.'
              : code === 'connect' ? 'Service vocal injoignable.' : 'Reconnaissance vocale indisponible.' }))
          },
        })
        sessionRef.current = session
      } catch { /* onError déjà remonté */ }
    })()
  }
  const pause = () => { stopSession(); setInterim(''); setLevel(0); setPhase('paused') }
  const close = () => { stopSession(); setInterim(''); setLevel(0); setPhase('idle'); setShowSettings(false) }
  useEffect(() => () => stopSession(), [])
  const persistLang = (code: string) => { setSpoken(code); localStorage.setItem('kb.office.dictLang', code) }
  const persistAuto = (v: boolean) => { setAutoPunct(v); localStorage.setItem('kb.office.dictAutoPunct', v ? '1' : '0') }
  const persistCmds = (v: boolean) => { setVoiceCmds(v); localStorage.setItem('kb.office.dictVoiceCmds', v ? '1' : '0') }
  const persistFilter = (v: boolean) => { setFilterProfanity(v); localStorage.setItem('kb.office.dictFilter', v ? '1' : '0') }

  // Dictée active → masque le reste de la chrome du bas (ruban, waffle FAB…) :
  // le panneau la REMPLACE (règle CSS globale sur [data-app-chrome]). AVANT les
  // early-returns (jamais de hook conditionnel).
  const active = phase !== 'idle'
  useEffect(() => {
    const b = document.body
    if (active) b.setAttribute('data-kb-dictating', '1'); else b.removeAttribute('data-kb-dictating')
    return () => b.removeAttribute('data-kb-dictating')
  }, [active])

  if (!enabled) return null
  const stopMd = (e: React.MouseEvent) => e.preventDefault()   // garde la sélection éditeur

  // ── Micro flottant (repos) ────────────────────────────────────────────────
  if (!active) {
    const bottom = kbInset ? `${kbInset + 48 + 12}px` : 'calc(72px + env(safe-area-inset-bottom) + 60px)'
    return createPortal(
      <button onClick={start}
        aria-label={t('doc_dictate', { defaultValue: 'Dicter' })} title={t('doc_dictate', { defaultValue: 'Dicter' })}
        className="lg:hidden fixed right-4 z-[46] w-12 h-12 rounded-full flex items-center justify-center shadow-lg"
        style={{ bottom, background: error ? '#ea4335' : '#1a73e8', color: '#fff' }}>
        <Mic size={20} />
      </button>, document.body)
  }

  // ── Panneau de dictée (actif) ─────────────────────────────────────────────
  const status = error ? error
    : phase === 'connecting' ? t('doc_dictate_prep', { defaultValue: 'Préparation du micro…' })
    : phase === 'paused' ? t('doc_dictate_resume', { defaultValue: 'Appuyer sur le microphone pour reprendre' })
    : interim ? interim
    : t('doc_dictate_listening', { defaultValue: 'À l’écoute… Dites quelque chose pour commencer' })
  // Cibles tactiles ≥ 44px + retour de pression NET (fond + léger enfoncement)
  // pour qu'un tap au doigt soit sans ambiguïté. `touch-manipulation` retire le
  // délai de 300ms ; `stopMd` (preventDefault mousedown) garde la sélection.
  const Chip = ({ label, onDo, wide }: { label: string; onDo: () => void; wide?: boolean }) => (
    <button onMouseDown={stopMd} onClick={onDo} style={{ WebkitTapHighlightColor: 'transparent' }}
      className={`h-11 ${wide ? 'px-4' : 'min-w-11 px-3'} rounded-xl border border-border bg-white text-[15px] text-text-primary
        hover:bg-surface-2 active:bg-primary-light active:border-accent active:scale-95 transition-[transform,background] duration-75 touch-manipulation flex-shrink-0`}>
      {label}
    </button>
  )
  const RoundBtn = ({ onDo, label, children }: { onDo: () => void; label: string; children: React.ReactNode }) => (
    <button onMouseDown={stopMd} onClick={onDo} aria-label={label} title={label} style={{ WebkitTapHighlightColor: 'transparent' }}
      className="w-12 h-12 rounded-full flex items-center justify-center text-text-secondary hover:bg-surface-2 active:bg-surface-3 active:scale-90 transition-transform duration-75 touch-manipulation flex-shrink-0">
      {children}
    </button>
  )
  return createPortal(
    // Feuille du bas OPAQUE avec son PROPRE calque composité au-dessus du contenu.
    // ⚠️ iOS Safari : le contenu du document porte `will-change: transform`
    // permanent (pincement) → calque composité qui, sans cela, PASSE DEVANT un
    // `position: fixed` classique (le blanc du doc « bavait » sur le panneau).
    // `translateZ(0)` + `isolation: isolate` + fond opaque forcent le panneau
    // au-dessus. Coins arrondis + poignée = vraie bottom-sheet, nette sur la page.
    <div className="lg:hidden fixed left-0 right-0 z-[60] border-t border-border"
      style={{ bottom: 0, paddingBottom: 'env(safe-area-inset-bottom)',
        background: 'var(--color-surface-1, #f8f9fa)', transform: 'translateZ(0)', isolation: 'isolate',
        borderTopLeftRadius: 16, borderTopRightRadius: 16, boxShadow: '0 -6px 24px rgba(0,0,0,0.18)' }}>
      <div className="flex justify-center pt-2 pb-0.5"><div style={{ width: 36, height: 4, borderRadius: 2, background: 'var(--color-border)' }} /></div>
      {showSettings ? (
        <div className="flex flex-col">
          <button onMouseDown={stopMd} onClick={() => setShowSettings(false)}
            className="flex items-center gap-1.5 h-11 px-3 text-accent text-xs font-medium border-b border-border">
            <ChevronLeft size={18} /> {t('doc_dictate_settings', { defaultValue: 'Paramètres de dictée' })}
          </button>
          {/* Langue parlée : Dropdown primitif @ui (menu porté, thème cohérent). */}
          <div className="flex items-center justify-between px-4 h-14 border-b border-border" onMouseDown={stopMd}>
            <span className="text-xs text-text-primary">{t('doc_dictate_lang', { defaultValue: 'Langue parlée' })}</span>
            <Dropdown value={spoken} onChange={persistLang} height={32} fontSize={12}
              options={DICT_LANGS.map(l => ({ value: l.code, label: l.label }))} />
          </div>
          {/* Bascules : Toggle primitif @ui (label + description + interrupteur). */}
          <div className="px-4 py-1 border-b border-border" onMouseDown={stopMd}>
            <Toggle checked={autoPunct} onChange={e => persistAuto(e.target.checked)}
              label={t('doc_dictate_autopunct', { defaultValue: 'Activer la ponctuation automatique' })} />
          </div>
          <div className="px-4 py-1 border-b border-border" onMouseDown={stopMd}>
            <Toggle checked={voiceCmds} onChange={e => persistCmds(e.target.checked)}
              label={t('doc_dictate_cmds', { defaultValue: 'Commandes vocales' })}
              description={t('doc_dictate_cmds_desc', { defaultValue: 'Dites « point », « nouvelle ligne », « supprimer »…' })} />
          </div>
          <div className="px-4 py-1" onMouseDown={stopMd}>
            <Toggle checked={filterProfanity} onChange={e => persistFilter(e.target.checked)}
              label={t('doc_dictate_filter', { defaultValue: 'Filtrer les expressions sensibles' })} />
          </div>
        </div>
      ) : (
        <div className="flex flex-col">
          {/* Statut / transcription en cours */}
          <div className="px-4 pt-3 pb-1 text-center text-xs min-h-[2.5rem] flex items-center justify-center"
            style={{ color: error ? '#ea4335' : 'var(--color-text-secondary)' }}>
            {status}
          </div>
          {/* Insertions rapides (chips ≥ 44px, retour de pression net) */}
          <div className="flex items-center gap-2 px-3 pb-2 overflow-x-auto" style={{ scrollbarWidth: 'none' }}>
            <Chip label="," onDo={() => insertPunct(',')} />
            <Chip label="." onDo={() => insertPunct('.')} />
            <Chip label="?" onDo={() => insertPunct('?')} />
            <Chip label="␣" onDo={insertSpace} />
            <Chip label={t('doc_dictate_newline', { defaultValue: 'Saut de ligne' })} onDo={insertBreak} wide />
          </div>
          {/* Réglages · micro · retour arrière */}
          <div className="flex items-center justify-between px-6 pb-3 pt-1">
            <RoundBtn onDo={() => setShowSettings(true)} label={t('doc_dictate_settings', { defaultValue: 'Paramètres de dictée' })}>
              <SettingsIcon size={22} />
            </RoundBtn>
            <button onMouseDown={stopMd} onClick={() => phase === 'paused' ? start() : pause()}
              style={{ WebkitTapHighlightColor: 'transparent', background: phase === 'listening' ? '#1a73e8' : '#fff', border: phase === 'listening' ? 'none' : '2px solid #1a73e8' }}
              className="relative w-[68px] h-[68px] rounded-full flex items-center justify-center flex-shrink-0 active:scale-90 transition-transform duration-75 touch-manipulation shadow-md"
              aria-label={phase === 'listening' ? t('doc_dictate_pause', { defaultValue: 'Pause' }) : t('doc_dictate', { defaultValue: 'Dicter' })}>
              {phase === 'listening' && (
                <span className="absolute inset-0 rounded-full pointer-events-none" style={{ background: 'rgba(26,115,232,0.28)', transform: `scale(${1 + Math.min(level, 1) * 1.2})`, transition: 'transform 90ms ease-out' }} />
              )}
              <Mic size={28} className="relative" style={{ color: phase === 'listening' ? '#fff' : '#1a73e8' }} />
            </button>
            <RoundBtn onDo={backspace} label={t('doc_dictate_backspace', { defaultValue: 'Retour arrière' })}>
              <DeleteIcon size={24} />
            </RoundBtn>
          </div>
        </div>
      )}
      {/* Fermer la dictée (revenir au clavier), coin haut-droit du panneau. */}
      <button onMouseDown={stopMd} onClick={close} style={{ WebkitTapHighlightColor: 'transparent' }}
        className="absolute -top-14 right-4 w-12 h-12 rounded-full bg-white border border-border shadow-lg flex items-center justify-center text-text-secondary active:bg-surface-2 active:scale-90 transition-transform duration-75 touch-manipulation"
        aria-label={t('doc_dictate_close', { defaultValue: 'Fermer la dictée' })}>
        <Keyboard size={22} />
      </button>
    </div>, document.body)
}

function DocumentEditorArea({ docId }: { docId: string }) {
  const { t, i18n } = useTranslation('office')
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
  // Mobile : ouverture en mode LECTURE (façon Word mobile) — document épuré sans
  // ruban ; le crayon de la barre de titre bascule en édition. (matchMedia lu en
  // init paresseuse : `useIsMobile` est déclaré plus bas, l'ordre des hooks prime.)
  const [mode, setMode]                           = useState<'edit' | 'read'>(() =>
    typeof window !== 'undefined' && window.matchMedia('(max-width: 1023px)').matches ? 'read' : 'edit')
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
  // Réglages de vérification. Défaut = préférence GLOBALE (localStorage, « Définir par
  // défaut ») ; ils sont ensuite persistés PAR DOCUMENT dans le fichier (content_json.spell)
  // et rechargés à l'ouverture (cf. effet de chargement).
  const [spellOn, setSpellOn]                     = useState<boolean>(() => localStorage.getItem('kb.office.spellOn') !== '0')
  const [grammarOn, setGrammarOn]                 = useState<boolean>(() => localStorage.getItem('kb.office.grammarOn') !== '0')
  const [grammarRules, setGrammarRules]           = useState<Record<string, boolean>>({})  // catégories désactivées (false)
  const [spellCount, setSpellCount]               = useState(0)
  const [spellLang, setSpellLang]                 = useState<string>(() => localStorage.getItem('kb.office.spellLang') || (i18n.language || 'fr').slice(0, 2))
  const [spellAuto, setSpellAuto]                 = useState<boolean>(() => localStorage.getItem('kb.office.spellAuto') !== '0')
  const [grammarCheckIssue, setGrammarCheckIssue] = useState<SpellIssue | null>(null)  // panneau « Vérification »
  const spellOnRef   = useRef(spellOn);   spellOnRef.current = spellOn
  const grammarOnRef = useRef(grammarOn); grammarOnRef.current = grammarOn
  const grammarRulesRef = useRef(grammarRules); grammarRulesRef.current = grammarRules
  const spellLangRef = useRef(spellLang); spellLangRef.current = spellLang
  const spellAutoRef = useRef(spellAuto); spellAutoRef.current = spellAuto
  const spellFromFileRef = useRef(false)   // réglages spell du fichier déjà appliqués ?
  // Dictionnaire personnel : dialogue + version (bump → re-vérification).
  const [spellDictOpen, setSpellDictOpen]         = useState(false)
  const [spellVersion, setSpellVersion]           = useState(0)
  // Applique la langue active au moteur (null = auto) + re-vérifie les squiggles.
  // (La persistance — fichier + défaut global — est faite dans les handlers de changement.)
  useEffect(() => {
    setActiveSpellLang(spellAuto ? null : spellLang)
    setSpellVersion(v => v + 1)
  }, [spellLang, spellAuto])
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
  // Réglages de page (marges de base) collaboratifs + ANNULABLES (cf. PAGE_SETUP_ORIGIN).
  const pageMap = useMemo(() => ydoc.getMap<SectionDef['margins']>('pageSetup'), [ydoc])
  // Mirror of the Yjs threads, kept in a ref so `flushSave` can persist them
  // without depending on a render. The server cannot read the Yjs update log, so
  // this mirror is the ONLY way comments reach a DOCX export.
  const commentsRef = useRef<CommentThread[]>([])
  useEffect(() => {
    const sync = () => {
      commentsRef.current = [...commentsMap.values()].filter(Boolean)
    }
    sync()
    commentsMap.observeDeep(sync)
    return () => commentsMap.unobserveDeep(sync)
  }, [commentsMap])
  const [commentsOpen, setCommentsOpen]           = useState(false)
  const [activeCommentId, setActiveCommentId]     = useState<string | null>(null)
  const [commentIds, setCommentIds]               = useState<string[]>([])
  // Auto-enter comment view (exits multipage) once, when the document is found
  // to contain unresolved comments — including after collab sync populates the map.
  const commentsAutoOpenedRef = useRef(false)
  useEffect(() => {
    if (commentsAutoOpenedRef.current) return
    const check = () => {
      if (commentsAutoOpenedRef.current) return
      if ([...commentsMap.values()].some(th => th && !th.resolved)) {
        commentsAutoOpenedRef.current = true
        setCommentsOpen(true)
      }
    }
    check()
    commentsMap.observe(check)
    return () => commentsMap.unobserve(check)
  }, [commentsMap])
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
  const [headingNumbers, setHeadingNumbers]       = useState(false)
  const headingNumbersRef = useRef(false)
  const [charSpacingOpen, setCharSpacingOpen]     = useState(false)
  const [tocDlgOpen, setTocDlgOpen]               = useState(false)
  // ── Onglet « Références » ──────────────────────────────────────────────────
  // Settings of the four generated tables, the bibliographic sources and the
  // citation style are all properties OF THE DOCUMENT (persisted in the
  // envelope), like the comments or the track-changes flag.
  const [refTab, setRefTab]                       = useState<TableKind>('toc')
  const [refSettings, setRefSettings]             = useState<ReferencesSettings>(DEFAULT_REFERENCES)
  const refSettingsRef = useRef(refSettings); refSettingsRef.current = refSettings
  const [sources, setSources]                     = useState<Source[]>([])
  const sourcesRef = useRef<Source[]>([]); sourcesRef.current = sources
  const [citationStyle, setCitationStyle]         = useState('APA')
  const citationStyleRef = useRef('APA'); citationStyleRef.current = citationStyle
  const [sourcesOpen, setSourcesOpen]             = useState<false | 'manage' | 'pick' | 'add'>(false)
  const [markDlg, setMarkDlg]                     = useState<false | MarkMode>(false)
  const [crossRefOpen, setCrossRefOpen]           = useState(false)
  const [updateTocOpen, setUpdateTocOpen]         = useState(false)
  // ── Macros : liste du document + enregistrement ────────────────────────────
  const docMacros = useDocumentMacros(docId ?? '', title || 'Document')
  // `makeApi` est défini plus bas ; la ref évite de dépendre de l'ordre.
  const makeApiRef = useRef<() => unknown>(() => ({}))
  const [macroBusy, setMacroBusy] = useState(false)
  const handleToggleRecord = useCallback(async () => {
    if (recorderState() === 'idle') {
      startRecording(activeEditorRef.current)
      return
    }
    const name = await prompt({
      title: t('macro_record', { defaultValue: 'Enregistrer une macro…' }),
      message: t('macro_name', { defaultValue: 'Nom de la macro :' }),
      defaultValue: t('macro_new_label', { defaultValue: 'Ma macro' }),
      confirmLabel: t('common_save', { defaultValue: 'Enregistrer' }),
    })
    const source = stopRecording(name || 'Macro')
    if (!name) return                      // annulé : la prise est simplement jetée
    await docMacros.create.mutateAsync({ name, source })
  }, [docMacros.create, prompt, t])

  const runMacroByScript = useCallback(async (scriptId: string) => {
    setMacroBusy(true)
    try { await docMacros.run(scriptId, makeApiRef.current) }
    finally { setMacroBusy(false) }
  }, [docMacros])

  // ── Macros (groupe du ruban Affichage, façon Word) ─────────────────────────
  const [macrosDlgOpen, setMacrosDlgOpen]         = useState(false)
  const [recState, setRecState]                   = useState<RecorderState>(() => recorderState())
  useEffect(() => onRecorderState(setRecState), [])
  // « Titres repliables » : la fonction est DÉSACTIVÉE par défaut chez nous —
  // Word l'impose sans interrupteur, nous en faisons un choix de l'utilisateur.
  // Préférence d'affichage (pas du document), donc locale à la machine.
  const [outlineArrows, setOutlineArrows]         = useState<boolean>(() => {
    try { return localStorage.getItem('kb.office.collapsibleHeadings') === '1' } catch { return false }
  })
  // Le moteur canvas ignore l'attribut `collapsed` tant que la fonction est off.
  useEffect(() => { setCollapsibleHeadings(outlineArrows) }, [outlineArrows])
  const pageNumFormatRef = useRef<PageNumFormat>('arabic'); pageNumFormatRef.current = pageNumFormat
  const pageNumStartRef  = useRef(1); pageNumStartRef.current = pageNumStart
  const watermarkRef  = useRef<WatermarkDef | null>(null); watermarkRef.current = watermark
  const pageBorderRef = useRef<PageBorderDef | null>(null); pageBorderRef.current = pageBorder
  const lineNumbersRef = useRef<LineNumbersDef | null>(null); lineNumbersRef.current = lineNumbers
  // CSS background appliqué à chaque page (dégradé prioritaire sur couleur unie).
  const pageBgCss = pageGrad ? gradientToCss(pageGrad) : pageColor
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
  // Bornes du tableau en cours d'édition → repères des règles (façon Word).
  const [tblRulerMarks, setTblRulerMarks]         = useState<{ cols: number[]; rows: number[]; cell?: { x0: number; x1: number } } | null>(null)
  const [tabType, setTabType]                     = useState<TabType>('left')
  const [activeEditor, setActiveEditor]           = useState<Editor | null>(null)
  // Track changes is a setting OF THE DOCUMENT (persisted in the envelope); the
  // author of a mark is the current user, the same source the comments use.
  const [trackChanges, setTrackChanges]           = useState(false)
  const [reviewOpen, setReviewOpen]               = useState(false)
  // View-only: 'final' hides deletions at layout time. Never persisted — it is a
  // reading mode, not a property of the document.
  const [revFinal, setRevFinal]                   = useState(false)
  const trackChangesRef = useRef(false)
  useEffect(() => { trackChangesRef.current = trackChanges
    setTrackChangesEnabled(activeEditor, trackChanges)
    setTrackChangesUser(activeEditor, { id: authUser?.id || '', name: authUser?.display_name || authUser?.username || authUser?.email || 'Anonyme' })
  }, [activeEditor, trackChanges, authUser])
  // activeOrientation = AFFICHAGE (section où est le curseur) ; baseOrientation =
  // section de base du document (persistée, passée à PaginatedEditor).
  const [activeOrientation, setActiveOrientation] = useState<Orientation>('portrait')
  const [baseOrientation, setBaseOrientation]     = useState<Orientation>('portrait')
  const [baseColumns, setBaseColumns]             = useState(1)
  // activeMargins = marges de la section du CURSEUR (règle/dialogue) ; baseMargins =
  // marges de la section de BASE (prop `section`, sauvegarde, Y.Map annulable).
  // activeSecIdx = index de la section active (0 = base, sinon nœud sectionBreak).
  const [activeMargins, setActiveMargins]         = useState<SectionDef['margins']>({ top: 96, right: 96, bottom: 96, left: 96 })
  const [baseMargins, setBaseMargins]             = useState<SectionDef['margins']>({ top: 96, right: 96, bottom: 96, left: 96 })
  const [activeSecIdx, setActiveSecIdx]           = useState(0)
  const [dragGuide, setDragGuide]                 = useState<DragGuide>(null)
  const scrollRef                                 = useRef<HTMLDivElement>(null)
  // Élément scroller COURANT en state : les listeners wheel/touch/gesture sont
  // attachés dans un effet — si le div est REMONTÉ sans changement de deps, les
  // listeners partaient avec l'ancien nœud (Ctrl+molette mort jusqu'au reload).
  // Le ref-callback pousse chaque nouvel élément dans ce state → l'effet se
  // ré-exécute et se rattache. Le callback DOIT être stable (useCallback) : un
  // ref-callback inline change d'identité à chaque rendu, donc React le rappelle
  // avec null puis l'élément à CHAQUE rendu — un setState par rendu, soit un
  // re-rendu permanent de tout l'éditeur.
  const [scrollEl, setScrollEl]                   = useState<HTMLDivElement | null>(null)
  const setScrollNode = useCallback((el: HTMLDivElement | null) => {
    scrollRef.current = el
    setScrollEl(prev => (prev === el ? prev : el))
  }, [])
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
  const evenOddRef = useRef(false)
  useEffect(() => { evenOddRef.current = evenOdd }, [evenOdd])

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

  // Mobile : la page A4 (~794px) déborde largement d'un écran de téléphone —
  // zoom ajusté à la largeur dès que la géométrie de page est disponible
  // (petites tentatives : le document se charge après le montage).
  const isMobileView = useIsMobile()
  useEffect(() => {
    if (!isMobileView) return
    let tries = 0
    const id = setInterval(() => {
      tries++
      if (opsRef.current?.pageGeom() && scrollRef.current) { fitZoom('width'); clearInterval(id) }
      else if (tries > 30) clearInterval(id)
    }, 100)
    return () => clearInterval(id)
  }, [isMobileView, fitZoom])

  // ── Ctrl/⌘ + molette → zoom du document (au lieu du zoom navigateur) ─────────
  // Écouteur `wheel` NON-PASSIF (sinon `preventDefault` est ignoré et le navigateur
  // zoome la page entière). Zoom centré sur le curseur : on garde le point du
  // document sous le pointeur fixe en ajustant le défilement après le re-layout.
  const zoomLiveRef = useRef(zoom); zoomLiveRef.current = zoom
  // Recalage ATOMIQUE d'un commit de zoom du pincement : posé avant setZoom,
  // exécuté par useLayoutEffect APRÈS la mutation du DOM et AVANT la peinture —
  // aucune frame « nouvelle échelle × ancien aperçu » (pics visibles sinon).
  const zoomCommitCbRef = useRef<(() => void) | null>(null)
  useLayoutEffect(() => {
    const f = zoomCommitCbRef.current
    if (f) { zoomCommitCbRef.current = null; f() }
  }, [zoom])
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

    // ── Pincement à DEUX doigts → zoom du document (mobile) — STABILISÉ ────────
    // Pendant le geste : AUCUN re-layout (un re-layout des pages à chaque micro-
    // variation des doigts faisait TREMBLER l'affichage). La prévisualisation est
    // un simple `transform: translate+scale` CSS sur le contenu, LISSÉ par filtre
    // exponentiel (amortit le tremblement naturel de la main) avec une ZONE MORTE
    // de ±3 % (une prise immobile ne bouge pas d'un pixel). Le vrai zoom (re-
    // layout net) n'est appliqué qu'AU RELÂCHEMENT, ancré sur le dernier médian.
    let pinch: {
      d0: number; z0: number
      d: number                       // dernière distance mesurée (pour le rebasage)
      engaged: boolean                // vrai après ±10px d'écartement (seuil anti-tremblement)
      cx0: number; cy0: number        // médian initial (viewport)
      ux0: number; uy0: number        // le même point, en coordonnées du CONTENU défilable
      k: number; kS: number           // ratio brut / lissé
      mx: number; my: number          // dernier médian (viewport)
      dxS: number; dyS: number        // panoramique lissé
      lastKS: number; lastT: number   // pour la vitesse du geste (détection de pause)
      lastInv: number                 // dernière contre-échelle publiée (quantifiée)
      stillT: number                  // début de l'immobilité (0 = en mouvement)
      dPrev: number; pmx: number; pmy: number   // mesures de la frame précédente
      pageW: number                   // largeur CSS de la page (échelle committée)
      pcx: number                     // centre horizontal du contenu (coords contenu)
      vw: number; sl: number          // clientWidth / scrollLeft mis en CACHE (les
                                      // lire à chaque frame force un layout — 60+
                                      // frames >50ms mesurées au banc à fort zoom)
      raf: number | null
    } | null = null
    const content = () => el.firstElementChild as HTMLElement | null
    const dist = (t: TouchList) => Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY)
    const mid  = (t: TouchList) => ({ x: (t[0].clientX + t[1].clientX) / 2, y: (t[0].clientY + t[1].clientY) / 2 })
    // Sur tactile, le calque composité du contenu est promu EN PERMANENCE (payé
    // une fois ici, au montage) : le promouvoir au début de chaque geste coûtait
    // ~330ms de fil principal à fort zoom (mesuré au banc) — un temps mort avant
    // que le pincement ne réponde.
    const keepLayer = window.matchMedia('(pointer: coarse)').matches
    if (keepLayer) { const c0 = content(); if (c0) c0.style.willChange = 'transform' }
    // Retire l'aperçu transform en préservant la promotion permanente mobile.
    const clearPreview = (c: HTMLElement) => {
      c.style.transform = ''; c.style.transformOrigin = ''
      c.style.willChange = keepLayer ? 'transform' : ''
      c.style.removeProperty('--kb-pinch-inv')
    }
    const paint = () => {
      if (!pinch) return
      pinch.raf = null
      // Lissage exponentiel du zoom ET du panoramique : suit vite, sans bruit.
      pinch.kS  += 0.45 * (pinch.k - pinch.kS)
      // CENTRAGE FORCÉ : quand la page prévisualisée est plus étroite que
      // l'écran, elle reste STRICTEMENT centrée horizontalement — la cible du
      // pan horizontal devient « centre de page sur centre d'écran » au lieu de
      // suivre le médian des doigts (l'EMA assure la transition douce au
      // franchissement du seuil). Le commit retombe naturellement dessus
      // (scrollLeft clampé à 0 + mise en page centrée → résiduel ≈ 0).
      const dxTarget = pinch.pageW * pinch.kS <= pinch.vw
        ? pinch.sl + pinch.vw / 2 - pinch.ux0 - (pinch.pcx - pinch.ux0) * pinch.kS
        : pinch.mx - pinch.cx0
      pinch.dxS += 0.45 * (dxTarget - pinch.dxS)
      pinch.dyS += 0.45 * ((pinch.my - pinch.cy0) - pinch.dyS)
      const c = content()
      if (!c) return
      c.style.transformOrigin = `${pinch.ux0}px ${pinch.uy0}px`
      c.style.transform = `translate(${pinch.dxS}px, ${pinch.dyS}px) scale(${pinch.kS})`
      c.style.willChange = 'transform'
      // Les éléments d'interface ancrés au document (poignées, étiquettes de
      // collaborateurs) gardent leur TAILLE pendant le geste via cette variable —
      // QUANTIFIÉE (pas 2 %) : changer une variable CSS invalide les styles du
      // sous-arbre, pas question de le faire à chaque frame pour 2 étiquettes.
      const inv = Math.round((1 / pinch.kS) * 50) / 50
      if (inv !== pinch.lastInv) { pinch.lastInv = inv; c.style.setProperty('--kb-pinch-inv', String(inv)) }
      // RE-RENDU net uniquement quand les doigts sont RÉELLEMENT immobiles : sur
      // machine lente, un commit (re-rendu React + repeinture) gèle l'aperçu
      // plusieurs centaines de ms — invisible pendant une pause, saccade violente
      // pendant le mouvement (mesuré au banc : trous de 770ms + reculs de 160px
      // avec l'ancien déclencheur « au ralenti »). Vraie pause = distance des
      // doigts ET médian stables sur plusieurs frames consécutives, pas un simple
      // passage par v≈0 (une inversion de direction y passe aussi).
      const now = performance.now()
      const v = Math.abs(pinch.kS - pinch.lastKS) / Math.max(1, now - pinch.lastT)
      pinch.lastKS = pinch.kS
      pinch.lastT = now
      const drift = Math.max(pinch.kS, 1 / pinch.kS)
      // Immobilité mesurée en TEMPS réel (pas en frames : les événements
      // tactiles arrivent par paquets sous charge et 8 frames calmes peuvent
      // survenir en plein geste → commits parasites mesurés au banc).
      const still = v < 0.0003
        && Math.abs(pinch.d - pinch.dPrev) < 3
        && Math.abs(pinch.mx - pinch.pmx) < 3 && Math.abs(pinch.my - pinch.pmy) < 3
      if (still) { if (!pinch.stillT) pinch.stillT = now } else pinch.stillT = 0
      pinch.dPrev = pinch.d; pinch.pmx = pinch.mx; pinch.pmy = pinch.my
      // AUCUN commit pendant le mouvement (même très ample : le flou temporaire
      // est préférable à un gel — un garde-fou d'échelle committait en plein
      // geste, mesuré 285-440ms de gel au banc). Le zoom réel est borné, donc
      // l'aperçu aussi (k est clampé sur [0.25/z0, 3/z0]).
      if (!committing && drift > 1.2 && pinch.stillT && now - pinch.stillT > 220) { rebase(); return }
      // Boucle maintenue tant que l'aperçu converge OU qu'une pause peut mûrir
      // en commit (sinon le compteur d'immobilité s'arrête avec la boucle).
      if (Math.abs(pinch.k - pinch.kS) > 0.002
        || Math.abs(dxTarget - pinch.dxS) > 0.4
        || Math.abs((pinch.my - pinch.cy0) - pinch.dyS) > 0.4
        || (drift > 1.2 && !committing)) pinch.raf = requestAnimationFrame(paint)
    }
    // Commit SANS SAUT : l'aperçu transform reste en place jusqu'à la frame où
    // le DOM reflète la nouvelle échelle (canvas redimensionnés par React) ; là,
    // AVANT peinture (rAF), on pose le défilement d'ancrage et on retire
    // l'aperçu — aucune frame intermédiaire fausse, aucun clamp de scroll.
    let committing = false
    const applyCommit = (onSettle: () => void) => {
      // Exécuté par le useLayoutEffect([zoom]) : DOM déjà muté, peinture pas
      // encore faite → l'ancrage et le transform résiduel sont posés dans la
      // MÊME frame que la nouvelle échelle. À poser AVANT le setZoom.
      committing = true
      zoomCommitCbRef.current = () => { committing = false; onSettle() }
    }
    // Committe le zoom réel EN COURS de geste puis rebase l'état du pincement
    // sur la nouvelle échelle (même ancrage que endPinch, geste continu).
    const rebase = () => {
      if (!pinch || committing) return
      const c = content()
      if (!c) return
      const old = zoomLiveRef.current
      // PAS d'arrondi (l'affichage arrondit de son côté) : un commit arrondi
      // créerait une marche de ±0,5 % visible au relâchement / aux paliers.
      const nz = Math.min(3, Math.max(0.25, old * pinch.kS))
      if (nz === old) return
      const s = nz / old
      applyCommit(() => {
        if (!pinch) {
          // Doigts levés pendant le commit : nettoyage + retour plein-res.
          clearPreview(c)
          if (pinchPaint.coarse) { pinchPaint.coarse = false; setTimeout(() => { if (!pinch) pinchPaint.refine?.() }, 120) }
          return
        }
        // CONTINUITÉ EXACTE avec les valeurs VIVANTES du geste (le médian et
        // l'échelle ont bougé pendant le commit — les figer au déclenchement
        // produisait des micro-inversions à chaque palier) :
        // 1. point du CONTENU (ancienne échelle) actuellement sous le médian ;
        const rect = el.getBoundingClientRect()
        const vx = pinch.mx - rect.left, vy = pinch.my - rect.top
        const kS = pinch.kS
        const U = pinch.ux0 + ((el.scrollLeft + vx) - pinch.ux0 - pinch.dxS) / kS
        const V = pinch.uy0 + ((el.scrollTop + vy) - pinch.uy0 - pinch.dyS) / kS
        // 2. ce point, converti à la nouvelle échelle, reste SOUS le médian.
        //    ⚠️ Le navigateur CLAMPE scrollLeft/Top à la plage atteignable — près
        //    des bords, la position demandée est tronquée (c'était LA source des
        //    sauts restants) : le reliquat devient une TRANSLATION RÉSIDUELLE du
        //    transform → aucune perte visuelle, quel que soit le clamp.
        //    Page plus étroite que l'écran → la mise en page committée la centre
        //    d'elle-même (et l'aperçu est déjà centré) : on ABANDONNE l'ancrage
        //    horizontal (sx=0) — l'invariant homothétique U→U·s est FAUX dans ce
        //    régime (le conteneur garde sa largeur, les x ne se multiplient pas
        //    par s), il produisait une excursion latérale résorbée par le rappel.
        const sx = pinch.pageW * s <= el.clientWidth ? 0 : U * s - vx
        const sy = V * s - vy
        el.scrollLeft = sx
        el.scrollTop  = sy
        const tx = el.scrollLeft - sx   // reliquat de clamp (0 loin des bords)
        const ty = el.scrollTop  - sy
        // 3. mapping distance→zoom invariant (z0×(d/d0)) + échelle ET translation
        //    résiduelles ré-appliquées dans la même frame.
        pinch.z0 = nz
        pinch.d0 = pinch.d0 * s
        pinch.k  = Math.min(3 / nz, Math.max(0.25 / nz, pinch.k / s))
        pinch.kS = kS / s
        pinch.dxS = tx
        pinch.dyS = ty
        pinch.cx0 = pinch.mx - tx
        pinch.cy0 = pinch.my - ty
        pinch.ux0 = U * s
        pinch.uy0 = V * s
        pinch.stillT = 0
        pinch.dPrev = pinch.d; pinch.pmx = pinch.mx; pinch.pmy = pinch.my
        // La mise en page a changé d'échelle : références du centrage forcé.
        pinch.pageW *= s
        pinch.pcx = c.offsetWidth / 2
        pinch.vw = el.clientWidth
        pinch.sl = el.scrollLeft
        c.style.transformOrigin = `${pinch.ux0}px ${pinch.uy0}px`
        c.style.transform = `translate(${tx}px, ${ty}px) scale(${pinch.kS})`
        pinch.lastInv = Math.round((1 / pinch.kS) * 50) / 50
        c.style.setProperty('--kb-pinch-inv', String(pinch.lastInv))
        if (pinch.raf == null) pinch.raf = requestAnimationFrame(paint)
      })
      // Transition : le rendu (lourd) est INTERRUPTIBLE — les touchmove et les
      // frames d'aperçu continuent d'être traités pendant qu'il travaille, au
      // lieu d'un gel de plusieurs centaines de ms sur machine lente. Et le
      // commit se peint en DEMI-résolution (le geste est encore en cours, la
      // netteté totale attendra le relâchement).
      pinchPaint.coarse = true
      startTransition(() => setZoom(nz))
    }
    const endPinch = () => {
      if (!pinch) return
      const p = pinch
      pinch = null
      if (p.raf != null) cancelAnimationFrame(p.raf)
      const c = content()
      if (!c) return
      // Un rebasage est en vol → son onSettle (voyant pinch nul) nettoiera.
      if (committing) return
      const old = zoomLiveRef.current
      const nz = Math.min(3, Math.max(0.25, old * p.kS))   // sans arrondi : relâchement exact
      // Prise non engagée ou variation infime → convertir le PANORAMIQUE
      // d'aperçu en défilement réel (sinon retour sec) puis retirer l'aperçu.
      if (!p.engaged || Math.abs(p.kS - 1) < 0.02 || nz === old) {
        el.scrollLeft -= p.dxS
        el.scrollTop  -= p.dyS
        clearPreview(c)
        // Geste terminé sur un état demi-res (commis pendant une pause) sans
        // nouveau commit → repeindre en pleine résolution.
        if (pinchPaint.coarse) { pinchPaint.coarse = false; setTimeout(() => { if (!pinch) pinchPaint.refine?.() }, 120) }
        return
      }
      const rect = el.getBoundingClientRect()
      const s = nz / old
      // Doigts levés → l'état p est figé et exact : même invariant, sans résiduel.
      const vx = p.mx - rect.left, vy = p.my - rect.top
      const U = p.ux0 + ((el.scrollLeft + vx) - p.ux0 - p.dxS) / p.kS
      const V = p.uy0 + ((el.scrollTop + vy) - p.uy0 - p.dyS) / p.kS
      applyCommit(() => {
        // Page étroite → mise en page centrée = état voulu, ancrage horizontal
        // abandonné (sx=0) — même raison qu'au rebasage (invariant homothétique
        // faux quand le conteneur garde sa largeur).
        const sx = p.pageW * s <= el.clientWidth ? 0 : U * s - vx
        const sy = V * s - vy
        el.scrollLeft = sx
        el.scrollTop  = sy
        const tx = el.scrollLeft - sx   // reliquat de clamp aux bords
        const ty = el.scrollTop  - sy
        c.style.setProperty('--kb-pinch-inv', '1')
        if (Math.abs(tx) < 1 && Math.abs(ty) < 1) {
          clearPreview(c)
          return
        }
        // Rappel élastique : la partie inatteignable (hors bornes de défilement)
        // se résorbe en douceur (180ms ease-out) au lieu d'un saut sec.
        c.style.transformOrigin = `${U * s}px ${V * s}px`
        const t0 = performance.now(), DUR = 180
        const tick = (now: number) => {
          if (pinch) return   // un nouveau geste a repris la main sur le transform
          const t = Math.min(1, (now - t0) / DUR)
          const e2 = 1 - Math.pow(1 - t, 3)
          c.style.transform = `translate(${tx * (1 - e2)}px, ${ty * (1 - e2)}px)`
          if (t < 1) { requestAnimationFrame(tick); return }
          clearPreview(c)
        }
        c.style.transform = `translate(${tx}px, ${ty}px)`
        requestAnimationFrame(tick)
      })
      pinchPaint.coarse = false             // relâchement = netteté totale
      startTransition(() => setZoom(nz))    // interruptible, cf. rebase
    }
    const beginPinch = (touches: TouchList) => {
      const m = mid(touches), rect = el.getBoundingClientRect()
      // Largeur de page (échelle committée) pour le centrage forcé — lue dans la
      // géométrie connue, PAS dans le DOM (offsetWidth par canvas = layout forcé
      // à la pose des doigts, coûteux à fort zoom).
      const cEl = content()
      const pw = (opsRef.current?.pageGeom()?.pageW ?? 0) * zoomLiveRef.current
      pinch = {
        d0: dist(touches), d: dist(touches), z0: zoomLiveRef.current, engaged: false,
        cx0: m.x, cy0: m.y,
        ux0: el.scrollLeft + (m.x - rect.left), uy0: el.scrollTop + (m.y - rect.top),
        k: 1, kS: 1, mx: m.x, my: m.y, dxS: 0, dyS: 0,
        lastKS: 1, lastT: performance.now(), lastInv: 1,
        stillT: 0, dPrev: dist(touches), pmx: m.x, pmy: m.y,
        pageW: pw, pcx: cEl ? cEl.offsetWidth / 2 : 0,
        vw: el.clientWidth, sl: el.scrollLeft, raf: null,
      }
    }
    // Prise qui commence PENDANT un commit en vol (gestes enchaînés vite) : les
    // ancres seraient capturées dans l'ANCIENNE mise en page puis utilisées dans
    // la nouvelle → saut. On diffère l'initialisation au premier touchmove
    // APRÈS l'atterrissage du commit.
    let startPending = false
    const onTouchStart = (e: TouchEvent) => {
      if (e.touches.length !== 2) { if (pinch && e.touches.length > 2) endPinch(); return }
      e.preventDefault()
      if (committing) { startPending = true; return }
      beginPinch(e.touches)
    }
    const onTouchMove = (e: TouchEvent) => {
      if (e.touches.length !== 2) return
      if (!pinch) {
        if (!startPending) return
        e.preventDefault()
        if (committing) return
        startPending = false
        beginPinch(e.touches)
        return
      }
      e.preventDefault()
      const m = mid(e.touches)
      const d = dist(e.touches)
      // SEUIL D'ENGAGEMENT en pixels absolus : le zoom ne démarre qu'après ±10px
      // d'écartement (le tremblement d'une prise immobile reste en dessous), puis
      // la distance de référence est REBASÉE là → aucune marche visible.
      if (!pinch.engaged) {
        if (Math.abs(d - pinch.d0) > 10) { pinch.engaged = true; pinch.d0 = d }
      }
      if (pinch.engaged) {
        let k = d / pinch.d0
        k = Math.min(3 / pinch.z0, Math.max(0.25 / pinch.z0, k))   // z0×k ∈ [0.25, 3]
        pinch.k = k
      }
      pinch.d = d
      pinch.mx = m.x
      pinch.my = m.y
      if (pinch.raf == null) pinch.raf = requestAnimationFrame(paint)
    }
    const onTouchEnd = (e: TouchEvent) => { if (e.touches.length < 2) { startPending = false; endPinch() } }
    el.addEventListener('touchstart', onTouchStart, { passive: false })
    el.addEventListener('touchmove', onTouchMove, { passive: false })
    el.addEventListener('touchend', onTouchEnd)
    el.addEventListener('touchcancel', onTouchEnd)
    // iOS Safari : ses événements PROPRIÉTAIRES gesturestart/gesturechange pilotent
    // le zoom du viewport indépendamment des touch events — neutralisés ici (le
    // pincement est entièrement géré par les listeners touch ci-dessus).
    const onGesture = (e: Event) => e.preventDefault()
    el.addEventListener('gesturestart', onGesture)
    el.addEventListener('gesturechange', onGesture)
    el.addEventListener('gestureend', onGesture)

    return () => {
      pinchPaint.coarse = false   // jamais de demi-res persistante hors geste
      const c0 = content(); if (c0) c0.style.willChange = ''
      el.removeEventListener('wheel', onWheel)
      el.removeEventListener('touchstart', onTouchStart)
      el.removeEventListener('touchmove', onTouchMove)
      el.removeEventListener('touchend', onTouchEnd)
      el.removeEventListener('touchcancel', onTouchEnd)
      el.removeEventListener('gesturestart', onGesture)
      el.removeEventListener('gesturechange', onGesture)
      el.removeEventListener('gestureend', onGesture)
    }
    // Re-attache une fois le scroller monté (le rendu de chargement précède le doc),
    // au basculement lecture ↔ édition, ET à chaque REMONTAGE du div scroller
    // (scrollEl) — sinon les listeners restaient sur l'ancien nœud détaché.
  }, [activeDoc?.id, docId, mode, scrollEl])

  // ── Persistance unifiée (contenu + mise en page) ──────────────────────────
  // Un seul format sauvegardé : l'enveloppe multi-page { sections, pages }. Les
  // refs gardent la dernière valeur de chaque source (doc, marges, orientation)
  // pour qu'un save de marge ne piétine pas le contenu et inversement.
  const sectionIdRef    = useRef<string>(newSectionId())
  const pageIdRef       = useRef<string>(newPageId())
  const docRef          = useRef<JSONContent>(emptyDoc())
  const activeEditorRef = useRef<Editor | null>(null)

  const marginsRef        = useRef(activeMargins);   marginsRef.current = activeMargins   // marges de la section ACTIVE (règle)
  const baseMarginsRef    = useRef(baseMargins);     baseMarginsRef.current = baseMargins // marges de la section de BASE (sauvegarde)
  const activeSecIdxRef   = useRef(activeSecIdx);    activeSecIdxRef.current = activeSecIdx
  const baseOrientationRef = useRef(baseOrientation); baseOrientationRef.current = baseOrientation
  const baseColumnsRef = useRef(baseColumns); baseColumnsRef.current = baseColumns
  const gutterRef = useRef(gutter); gutterRef.current = gutter
  const headerDistRef = useRef(headerDist); headerDistRef.current = headerDist
  const footerDistRef = useRef(footerDist); footerDistRef.current = footerDist
  const vAlignPageRef = useRef(vAlignPage); vAlignPageRef.current = vAlignPage
  const sectionStartRef = useRef(sectionStart); sectionStartRef.current = sectionStart
  const saveTimerRef    = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  // Ctrl+S / ⌘S saves immediately.
  useSaveShortcut(() => { void doSave() })

  const doSave = useCallback(() => {
    const ed      = activeEditorRef.current
    const content = ed ? ed.getJSON() : docRef.current
    const sec: SectionDef  = { id: sectionIdRef.current, orientation: baseOrientationRef.current, margins: baseMarginsRef.current, columns: baseColumnsRef.current, gutter: gutterRef.current, headerDist: headerDistRef.current, footerDist: footerDistRef.current, vAlign: vAlignPageRef.current, sectionStart: sectionStartRef.current }
    const page: PageData   = { id: pageIdRef.current, sectionId: sectionIdRef.current, content }
    saveDoc(docId, { content_json: serializeDoc([sec], [page], { pageNumbers: pageNumbersRef.current, header: headerRef.current, footer: footerRef.current, hfFirstPage: hfFirstRef.current, pageColor: pageColorRef.current, pageGrad: pageGradRef.current, paperSize: paperSizeRef.current, styles: Object.keys(styleOverridesRef.current).length ? styleOverridesRef.current : undefined, watermark: watermarkRef.current, pageBorder: pageBorderRef.current, lineNumbers: lineNumbersRef.current, pageNumFormat: pageNumFormatRef.current, pageNumStart: pageNumStartRef.current, headingNumbers: headingNumbersRef.current, comments: commentsRef.current, evenOdd: evenOddRef.current, trackChanges: trackChangesRef.current, refSettings: refSettingsRef.current, sources: sourcesRef.current, citationStyle: citationStyleRef.current, spell: { lang: spellLangRef.current, auto: spellAutoRef.current, on: spellOnRef.current, grammar: grammarOnRef.current, rules: Object.keys(grammarRulesRef.current).length ? grammarRulesRef.current : undefined } }) })
  }, [docId, saveDoc])

  const scheduleSave = useCallback(() => {
    clearTimeout(saveTimerRef.current)
    saveTimerRef.current = setTimeout(doSave, 700)
  }, [doSave])

  // Applique un changement de marges en le COALESÇANT sur une frame : pendant un
  // glissé de règle, `onMarginsChange` se déclenche à chaque mousemove ; sans
  // throttle, chaque appel provoque un setState + un relayout complet du document.
  // La règle dessine son repère en direct (refs internes), donc throttler le reflux
  // ne dégrade pas le retour visuel du glissé. La valeur finale (mouseup) est toujours
  // appliquée à la frame suivante.
  const marginRafRef = useRef(0)
  const pendingMarginsRef = useRef<SectionDef['margins'] | null>(null)
  const commitMargins = useCallback((m: SectionDef['margins']) => {
    marginsRef.current = m
    pendingMarginsRef.current = m
    if (marginRafRef.current) return
    marginRafRef.current = requestAnimationFrame(() => {
      marginRafRef.current = 0
      const mm = pendingMarginsRef.current
      if (!mm) return
      setActiveMargins(mm)   // la règle reflète la section active tout de suite
      if (activeSecIdxRef.current === 0) {
        // Section de BASE → Y.Map suivie (annulable). L'observateur met à jour
        // baseMargins (→ prop `section` → reflow) et planifie la sauvegarde.
        ydoc.transact(() => { pageMap.set('margins', mm) }, PAGE_SETUP_ORIGIN)
      } else {
        // Section de SAUT → marges dans le nœud (annulables) ; reflow via onUpdate.
        opsRef.current?.setSectionMargins(mm)
      }
    })
  }, [ydoc, pageMap])
  useEffect(() => () => { if (marginRafRef.current) cancelAnimationFrame(marginRafRef.current) }, [])

  // Observe la Y.Map `pageSetup` : tout changement des marges de base (édition,
  // ANNULATION/RÉTABLISSEMENT, collaboration) se réinjecte dans l'état React →
  // reflow (prop `section`) + sauvegarde. Le seed initial (PAGE_INIT_ORIGIN) ne
  // déclenche pas de sauvegarde.
  useEffect(() => {
    const obs = (_e: unknown, tr: { origin?: unknown }) => {
      const m = pageMap.get('margins')
      if (!m) return
      setBaseMargins(m); baseMarginsRef.current = m
      if (activeSecIdxRef.current === 0) { setActiveMargins(m); marginsRef.current = m }
      if (tr?.origin !== PAGE_INIT_ORIGIN) scheduleSave()
    }
    pageMap.observe(obs)
    return () => pageMap.unobserve(obs)
  }, [pageMap, scheduleSave])

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
  // ── Page ACTIVE des règles = page la plus VISIBLE dans le viewport (façon Word),
  // avec une prime à la page du curseur quand elle est à l'écran. Avant, les règles
  // suivaient uniquement la page du CARET : en défilant vers une autre page sans
  // cliquer, la règle verticale graduait toujours l'ancienne page (toute grise) et
  // la règle horizontale restait calée sur sa colonne — « repositionnement raté ».
  const [rulerPage, setRulerPage] = useState(0)
  const rulerPageRef = useRef(0); rulerPageRef.current = rulerPage
  const caretPageRef = useRef(0); caretPageRef.current = Math.max(0, docStats.current - 1)
  const rulerUpdateRef = useRef<() => void>(() => {})
  // Position de la page active dans le contenu défilable (repère des règles).
  const [activePageBox, setActivePageBox] = useState<{ left: number; top: number } | null>(null)
  // Wrapper de la règle horizontale : suivi DIRECT du défilement horizontal
  // (translateX = -scrollLeft, hors React — même approche que la règle verticale).
  const hRulerBoxRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const sc = scrollRef.current
    if (!sc) return
    let raf = 0
    const update = () => {
      raf = 0
      const view = sc.getBoundingClientRect()
      // Les canvases de page (classe bg-white), dans l'ordre du document.
      const cvs = sc.querySelectorAll<HTMLCanvasElement>('canvas.bg-white')
      let best = -1, bestScore = 0
      const caret = caretPageRef.current
      let caretVisible = false
      cvs.forEach((cv, i) => {
        const r = cv.getBoundingClientRect()
        const w = Math.max(0, Math.min(r.right, view.right) - Math.max(r.left, view.left))
        const h = Math.max(0, Math.min(r.bottom, view.bottom) - Math.max(r.top, view.top))
        const score = w * h                     // aire visible de la page
        if (i === caret && score > 0) caretVisible = true
        if (score > bestScore) { bestScore = score; best = i }
      })
      // La règle suit la page du CURSEUR dès qu'elle est visible (comme Word), quelle que
      // soit son aire — sinon (curseur hors écran) on retombe sur la page la plus visible.
      if (caretVisible) best = caret
      if (best >= 0) {
        // Rafraîchir aussi la POSITION mesurée (offsets contenu) : un reflow
        // (redimensionnement, passage 2 pages/rangée → 1) déplace les pages sans
        // changer d'index → la règle doit suivre quand même.
        const cv = cvs[best]
        const left = cv.offsetLeft, top = cv.offsetTop
        setActivePageBox(prev => (prev && prev.left === left && prev.top === top) ? prev : { left, top })
        if (best !== rulerPageRef.current) setRulerPage(best)
      }
    }
    rulerUpdateRef.current = update
    // Défilement HORIZONTAL : la bande de la règle H est hors du conteneur défilant →
    // on la translate de -scrollLeft en direct (sans setState → pas de re-rendu par frame).
    const syncH = () => {
      const el = hRulerBoxRef.current
      if (el) el.style.transform = sc.scrollLeft ? `translateX(${-sc.scrollLeft}px)` : ''
    }
    const onScroll = () => { syncH(); if (!raf) raf = requestAnimationFrame(update) }
    update(); syncH()
    sc.addEventListener('scroll', onScroll, { passive: true })
    // Reflows sans scroll (fenêtre/panneaux redimensionnés) → re-mesurer aussi.
    const ro = new ResizeObserver(onScroll)
    ro.observe(sc)
    return () => { sc.removeEventListener('scroll', onScroll); ro.disconnect(); if (raf) cancelAnimationFrame(raf) }
  }, [activeEditor])
  // Déplacement du caret / re-pagination / zoom → re-évaluer la page des règles.
  useEffect(() => { rulerUpdateRef.current() }, [docStats.current, docStats.pages, zoom])

  // Recalage à chaque changement de page des règles / zoom — les règles suivent la
  // page active, y compris en disposition GRILLE (pages non empilées verticalement).
  useLayoutEffect(() => {
    // Une fois une page calée, on ne repasse JAMAIS à null (sinon flash de centrage pendant
    // un reflow) → il y a toujours une page « sélectionnée ». Si le canvas de la page n'est
    // pas encore monté (nouvelle page, re-pagination en cours), on re-essaie quelques
    // frames — avant, l'échec silencieux laissait la règle sur l'ancienne position.
    let raf = 0, tries = 0
    const apply = () => {
      const b = opsRef.current?.pageContentBox(rulerPage)
      if (b) setActivePageBox(prev => (prev && prev.left === b.left && prev.top === b.top) ? prev : { left: b.left, top: b.top })
      else if (++tries < 30) raf = requestAnimationFrame(apply)
    }
    apply()
    return () => { if (raf) cancelAnimationFrame(raf) }
  }, [rulerPage, docStats.pages, activeEditor, zoom, activeGeo.pageW, activeGeo.pageH])

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
      // Repères de tableau : lus APRÈS le layout (rAF), sinon les bornes datent de
      // l'état précédent juste après une frappe qui change la géométrie.
      requestAnimationFrame(() => {
        const m = opsRef.current?.tableRuler?.() ?? null
        setTblRulerMarks(prev => JSON.stringify(prev) === JSON.stringify(m) ? prev : m)
      })
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

  // Glissé d'un marqueur de RETRAIT : chaque mousemove dispatchait une transaction PM →
  // onUpdate → recompute SYNCHRONE. Sur un gros document (retrait étroit → explosion du
  // nombre de pages), la rafale de relayouts non throttlés PÈGUE le thread (même classe
  // que l'ancien gel de marge). On coalesce à ≤1 transaction/frame (rAF) ; la valeur
  // finale (mouseup, commit=true) est appliquée immédiatement et annule tout throttle.
  const indentRafRef = useRef(0)
  const pendingIndentsRef = useRef<{ left: number; first: number; right: number } | null>(null)
  const commitIndents = useCallback((ind: { left: number; first: number; right: number }, commit: boolean) => {
    if (commit) {
      if (indentRafRef.current) { cancelAnimationFrame(indentRafRef.current); indentRafRef.current = 0 }
      pendingIndentsRef.current = null
      setParaIndentAttrs({ indentLeft: ind.left, indentFirstLine: ind.first, indentRight: ind.right }, true)
      return
    }
    pendingIndentsRef.current = ind
    if (indentRafRef.current) return
    indentRafRef.current = requestAnimationFrame(() => {
      indentRafRef.current = 0
      const p = pendingIndentsRef.current
      if (p) setParaIndentAttrs({ indentLeft: p.left, indentFirstLine: p.first, indentRight: p.right }, false)
    })
  }, [setParaIndentAttrs])
  useEffect(() => () => { if (indentRafRef.current) cancelAnimationFrame(indentRafRef.current) }, [])
  // Mode lecture : éditeur en lecture seule (barre d'outils/règles masquées au rendu).
  useEffect(() => { activeEditor?.setEditable(mode === 'edit') }, [activeEditor, mode])
  useEffect(() => {
    if (!activeDoc) return
    setTitle(activeDoc.title)
    // Initialise margins, orientation et ids stables depuis le document stocké.
    const { sections, pages, pageNumbers: pn, header: hdr, footer: ftr, hfFirstPage: hf1, pageColor: pc, pageGrad: pg, paperSize: ps, styles: stl, watermark: wmk, pageBorder: pbd, lineNumbers: lnm, pageNumFormat: pnf, pageNumStart: pns, headingNumbers: hnum, spell: sp, evenOdd: eo, trackChanges: tc, refSettings: rs, sources: srcs, citationStyle: cst } = parseDocContent(activeDoc.content_json as object | null)
    // Réglages de vérification DU FICHIER (une seule fois par document) — sinon on garde
    // le défaut global. Ne pas ré-appliquer aux re-fetch (sinon écrase un changement en cours).
    if (sp && !spellFromFileRef.current) {
      spellFromFileRef.current = true
      if (sp.lang !== undefined)    { setSpellLang(sp.lang);       spellLangRef.current = sp.lang }
      if (sp.auto !== undefined)    { setSpellAuto(sp.auto);       spellAutoRef.current = sp.auto }
      if (sp.on !== undefined)      { setSpellOn(sp.on);           spellOnRef.current = sp.on }
      if (sp.grammar !== undefined) { setGrammarOn(sp.grammar);    grammarOnRef.current = sp.grammar }
      if (sp.rules !== undefined)   { setGrammarRules(sp.rules);   grammarRulesRef.current = sp.rules }
    }
    setStyleOverrides(stl ?? {}); styleOverridesRef.current = stl ?? {}
    setWatermark(wmk ?? null); watermarkRef.current = wmk ?? null
    setPageBorder(pbd ?? null); pageBorderRef.current = pbd ?? null
    setLineNumbers(lnm ?? null); lineNumbersRef.current = lnm ?? null
    setPageNumFormat(pnf ?? 'arabic'); pageNumFormatRef.current = pnf ?? 'arabic'
    setPageNumStart(pns ?? 1); pageNumStartRef.current = pns ?? 1
    setHeadingNumbers(!!hnum); headingNumbersRef.current = !!hnum
    setHeader(hdr); headerRef.current = hdr
    setFooter(ftr); footerRef.current = ftr
    setHfFirstPage(!!hf1); hfFirstRef.current = !!hf1
    setEvenOdd(!!eo); evenOddRef.current = !!eo
    setTrackChanges(!!tc); trackChangesRef.current = !!tc
    // « Références » — merged with the defaults so a document saved by an older
    // build (or by another editor) still opens with every option present.
    const refs: ReferencesSettings = rs
      ? { toc: { ...DEFAULT_REFERENCES.toc, ...rs.toc }, figures: { ...DEFAULT_REFERENCES.figures, ...rs.figures },
          index: { ...DEFAULT_REFERENCES.index, ...rs.index }, authorities: { ...DEFAULT_REFERENCES.authorities, ...rs.authorities } }
      : DEFAULT_REFERENCES
    setRefSettings(refs); refSettingsRef.current = refs
    setSources(srcs ?? []); sourcesRef.current = srcs ?? []
    setCitationStyle(cst ?? 'APA'); citationStyleRef.current = cst ?? 'APA'
    setPaperSize(ps ?? 'a4'); paperSizeRef.current = ps ?? 'a4'
    setPageColor(pc); pageColorRef.current = pc
    setPageGrad(pg);  pageGradRef.current = pg
    if (sections[0]) {
      const s0 = sections[0]
      // Marges de base = source de vérité pour le rendu/la sauvegarde ; la section
      // active démarre sur la base (curseur au début). Seed de la Y.Map annulable
      // depuis le JSON si la salle Yjs ne la porte pas encore (docs existants).
      const seed = pageMap.get('margins') ?? s0.margins
      setBaseMargins(seed); baseMarginsRef.current = seed
      setActiveMargins(seed); marginsRef.current = seed
      setActiveSecIdx(0); activeSecIdxRef.current = 0
      if (!pageMap.has('margins')) ydoc.transact(() => { pageMap.set('margins', s0.margins) }, PAGE_INIT_ORIGIN)
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

  /** Is the caret inside a generated table of contents? (F9, Word's shortcut.) */
  const caretInToc = useCallback((): boolean => {
    const ed = activeEditorRef.current; if (!ed) return false
    const $from = ed.state.selection.$from
    for (let d = $from.depth; d >= 0; d--) {
      const a = $from.node(d).attrs as Record<string, unknown> | undefined
      if (a?.tocKind === 'toc' || (a?.tocKind == null && (a?.tocTitle === true || a?.tocLevel != null))) return true
    }
    return false
  }, [])

  // Ctrl/Cmd+P → impression PDF fidèle (sinon le navigateur imprimerait le canvas).
  // F9 dans une table des matières → même invite que Word.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'p') { e.preventDefault(); handlePrint() }
      else if (e.key === 'F9' && caretInToc()) { e.preventDefault(); setUpdateTocOpen(true) }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [handlePrint, caretInToc])
  const handleExportTxt = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    const text = ed.getText({ blockSeparator: '\n' })
    downloadBlob(new Blob([text], { type: 'text/plain;charset=utf-8' }), `${titleRef2.current || 'document'}.txt`)
  }, [])
  // Export serveur (DOCX/ODT) : téléchargement AUTHENTIFIÉ via axios (les anciennes
  // navigations location.href vers /api/v1/documents/... étaient un 404 + sans token).
  // A document opened from a foreign file saves back INTO that file, in its own
  // format — the behaviour of every desktop word processor. The server refuses
  // formats it cannot write (`.doc`) and says which one to use instead.
  const handleSaveToSource = useCallback(async () => {
    if (typeof navigator !== 'undefined' && !navigator.onLine) return
    await flushSave()
    try {
      await api.post(`/office/documents/${docId}/save-source`)
    } catch (e) {
      console.error('save-source', e)
    }
  }, [docId, flushSave])

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
  // ── Table des matières (façon Word : champ régénérable) ──────────────────────
  // Construit les entrées depuis le plan (titres + page réelle), insère au caret ou
  // REMPLACE le bloc existant (titre marqué tocTitle + entrées tocLevel contiguës).
  const generateToc = useCallback((opts: TocSettings, silent = false) => {
    const ed = activeEditorRef.current, ops = opsRef.current
    if (!ed || !ops) return
    const doc = ed.state.doc
    // Bloc existant : [posTitre, fin de la dernière entrée contiguë).
    // A TOC built here starts with a tocTitle heading; one coming from a file has
    // only tocLevel paragraphs, sometimes under a plain heading acting as title.
    // Both must be recognised, otherwise « Mettre à jour » inserts a second TOC.
    let tocStart = -1, tocEnd = -1, withTitle = true, titlePos = -1
    let off = 0, prevPos = -1, prevHeading = false
    doc.forEach(node => {
      const a = node.attrs as Record<string, unknown>
      if (tocStart < 0) {
        if (a?.tocTitle) { tocStart = off; tocEnd = off + node.nodeSize }
        else if (a?.tocLevel != null) {
          // Imported TOC: replace the entries only, never the file's own title
          // heading above them — but do not list that heading as an entry either.
          tocStart = off; tocEnd = off + node.nodeSize
          withTitle = false
          if (prevHeading) titlePos = prevPos
        }
      } else if (tocEnd === off && (a?.tocLevel != null)) {
        tocEnd = off + node.nodeSize
      }
      prevHeading = node.type.name === 'heading'; prevPos = off
      off += node.nodeSize
    })
    // Les titres marqués tocTitle (le titre du bloc lui-même) ne sont pas des entrées.
    const excluded = new Set<number>()
    let off2 = 0
    doc.forEach(node => { if ((node.attrs as Record<string, unknown>)?.tocTitle) excluded.add(off2); off2 += node.nodeSize })
    // Same for an imported title heading: unmarked, yet not an entry of its own TOC.
    if (titlePos >= 0) excluded.add(titlePos)
    const entries = tocEntries(ops.outline(), opts, excluded)
    const content: JSONContent[] = buildTable(entries, opts, {
      title: withTitle ? t('doc_toc', { defaultValue: 'Table des matières' }) : undefined,
      emptyText: t('doc_toc_empty', { defaultValue: 'Aucun titre dans le document.' }),
    })
    const nodes = content.map(x => ed.state.schema.nodeFromJSON(x))
    if (tocStart >= 0) {
      let tr = ed.state.tr.replaceWith(tocStart, tocEnd, nodes)
      if (!silent) tr = tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(tr.doc.content.size - 1, tocStart + 1)), 1))
      ed.view.dispatch(tr)
    } else {
      // Insérer à la FRONTIÈRE du bloc courant (jamais au milieu d'un titre/texte —
      // sinon le caret scinde le paragraphe en deux).
      const $from = ed.state.selection.$from
      const insertAt = $from.depth >= 1 ? $from.before(1) : 0
      ed.chain().focus().insertContentAt(insertAt, content).run()
    }
    // 2e passe silencieuse : l'insertion décale la pagination → renuméroter.
    if (!silent) setTimeout(() => generateTocRef.current?.(opts, true), 400)
  }, [t])
  const generateTocRef = useRef<((opts: TocSettings, silent?: boolean) => void) | null>(null)
  generateTocRef.current = generateToc
  // Réglages détectés depuis le bloc existant (pour « Mettre à jour »).
  const detectTocOpts = useCallback((): TocSettings => {
    const ed = activeEditorRef.current
    const base = refSettingsRef.current.toc
    let levels = 0, pages = false, leader = false
    ed?.state.doc.forEach(node => {
      const a = node.attrs as Record<string, unknown>
      if (a?.tocLevel != null) {
        levels = Math.max(levels, Number(a.tocLevel) || 1)
        if (a.tocPage != null || a.tocPageText != null) pages = true
        if (a.tocLeader) leader = true
      }
    })
    // An existing block wins over the stored settings: « Mettre à jour » must
    // rebuild the table the document HAS, not the one last configured.
    return levels > 0
      ? { ...base, levels, showPageNumbers: pages, leader: leader ? base.leader === 'none' ? 'dots' : base.leader : 'none' }
      : base
  }, [])
  const handleTocUpdate = useCallback(() => generateToc(detectTocOpts()), [generateToc, detectTocOpts])

  // ── Onglet « Références » ──────────────────────────────────────────────────
  // Page of a document position, the way the TOC numbers its entries.
  const pageOfPos = useCallback((pos: number) => opsRef.current?.pageAt(pos) ?? 1, [])

  /** Replace the block of that kind if it exists, else insert at the caret. */
  const putTable = useCallback((kind: TableKind, content: JSONContent[]) => {
    const ed = activeEditorRef.current; if (!ed) return
    let start = -1, end = -1, off = 0
    ed.state.doc.forEach(node => {
      const a = node.attrs as Record<string, unknown>
      if (a?.tocKind === kind) {
        if (start < 0) start = off
        end = off + node.nodeSize
      }
      off += node.nodeSize
    })
    const nodes = content.map(x => ed.state.schema.nodeFromJSON(x))
    if (start >= 0) ed.view.dispatch(ed.state.tr.replaceWith(start, end, nodes))
    else {
      const $from = ed.state.selection.$from
      ed.chain().focus().insertContentAt($from.depth >= 1 ? $from.before(1) : 0, content).run()
    }
  }, [])

  const hasTable = useCallback((kind: TableKind): boolean => {
    const ed = activeEditorRef.current; if (!ed) return false
    let found = false
    ed.state.doc.forEach(node => { if ((node.attrs as Record<string, unknown>)?.tocKind === kind) found = true })
    return found
  }, [])

  /** Caption paragraphs of the document (« Figure 3 : légende »). */
  const collectCaptions = useCallback((): Array<{ text: string; page: number; label: string }> => {
    const ed = activeEditorRef.current; if (!ed) return []
    const out: Array<{ text: string; page: number; label: string }> = []
    ed.state.doc.descendants((node, pos) => {
      if (!node.isTextblock) return true
      if ((node.attrs as Record<string, unknown>)?.tocKind) return false   // never list a generated table
      const text = node.textContent.trim()
      const m = /^(\p{L}+)\s+\d+\s*[:.–-]?/u.exec(text)
      if (m) out.push({ text, page: pageOfPos(pos), label: m[1] })
      return false
    })
    return out
  }, [pageOfPos])

  const handleTocCustom = useCallback((kind: TableKind = 'toc') => { setRefTab(kind); setTocDlgOpen(true) }, [])

  /** Word's gallery: two automatic tables and a manual one. */
  const handleTocPreset = useCallback((preset: TocPreset) => {
    const base = refSettingsRef.current.toc
    if (preset === 'manual') {
      const content = [
        { type: 'heading', attrs: { level: 2, tocTitle: true, outlineLevel: 0, tocKind: 'toc' }, content: [{ type: 'text', text: t('doc_toc', { defaultValue: 'Table des matières' }) }] },
        ...[1, 2, 3].map(n => ({
          type: 'paragraph',
          attrs: { indent: n - 1, tocLevel: n, tocKind: 'toc', tocPage: n, tocLeader: true, tocLeaderKind: 'dots', spaceAfter: 2 },
          content: [{ type: 'text', text: t(`doc_toc_manual_${n}`, { defaultValue: `Tapez le titre du chapitre (niveau ${n})` }), ...(n === 1 ? { marks: [{ type: 'bold' }] } : {}) }],
        })),
      ]
      putTable('toc', content)
      return
    }
    generateToc({ ...base, levels: 3, showPageNumbers: true, rightAlign: true, leader: 'dots' })
  }, [generateToc, putTable, t])

  /**
   * « Mettre à jour les numéros de page uniquement » : les textes d'entrée sont
   * CONSERVÉS. C'est tout l'intérêt de l'option — un utilisateur qui a retouché
   * une entrée à la main la perdrait avec une régénération complète.
   */
  const updateTocPagesOnly = useCallback(() => {
    const ed = activeEditorRef.current, ops = opsRef.current
    if (!ed || !ops) return
    const pageByText = new Map<string, number>()
    for (const it of ops.outline()) pageByText.set(it.text.trim(), it.page)
    const tr = ed.state.tr
    let off = 0
    ed.state.doc.forEach(node => {
      const a = node.attrs as Record<string, unknown>
      const isEntry = a?.tocLevel != null && (a?.tocKind === 'toc' || a?.tocKind == null)
      if (isEntry) {
        const page = pageByText.get(node.textContent.trim())
        if (page != null && (a.tocPage !== page || a.tocPageText != null)) {
          tr.setNodeMarkup(off, undefined, {
            ...a,
            tocPage: page,
            tocPageText: a.tocPageText != null ? String(page) : null,
          })
        }
      }
      off += node.nodeSize
    })
    if (tr.docChanged) ed.view.dispatch(tr)
  }, [])

  const handleTocUpdateMode = useCallback((mode: TocUpdateMode) => {
    if (mode === 'pages') updateTocPagesOnly()
    else generateToc(detectTocOpts())
  }, [updateTocPagesOnly, generateToc, detectTocOpts])

  const handleTocRemove = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    let start = -1, end = -1, off = 0
    ed.state.doc.forEach(node => {
      const a = node.attrs as Record<string, unknown>
      if (a?.tocKind === 'toc' || a?.tocTitle || a?.tocLevel != null) {
        if (start < 0) start = off
        end = off + node.nodeSize
      }
      off += node.nodeSize
    })
    if (start >= 0) ed.view.dispatch(ed.state.tr.delete(start, end))
  }, [])

  const generateFigures = useCallback(() => {
    const s = refSettingsRef.current.figures
    putTable('figures', buildTable(figureEntries(collectCaptions(), s), s, {
      title: t('doc_tof_title', { defaultValue: 'Table des illustrations' }),
      emptyText: t('doc_tof_empty', { defaultValue: 'Aucune légende dans le document.' }),
      kind: 'figures',
    }))
  }, [collectCaptions, putTable, t])

  const generateIndex = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    const s = refSettingsRef.current.index
    putTable('index', buildTable(indexEntries(collectIndexHits(ed.state.doc, pageOfPos), s), s, {
      title: t('doc_index_title', { defaultValue: 'Index' }),
      emptyText: t('doc_index_empty', { defaultValue: "Aucune entrée d'index." }),
      kind: 'index',
    }))
  }, [pageOfPos, putTable, t])

  const generateAuthorities = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    const s = refSettingsRef.current.authorities
    putTable('authorities', buildTable(authorityEntries(collectCitationHits(ed.state.doc, pageOfPos), s), s, {
      title: t('doc_toa_title', { defaultValue: 'Table des références' }),
      emptyText: t('doc_toa_empty', { defaultValue: 'Aucune citation marquée.' }),
      kind: 'authorities',
    }))
  }, [pageOfPos, putTable, t])

  /** OK in the four-tab dialog: the ACTIVE tab decides which table is built. */
  const handleReferencesApply = useCallback((kind: TableKind, next: ReferencesSettings) => {
    setRefSettings(next); refSettingsRef.current = next
    scheduleSave()
    if (kind === 'toc') generateToc(next.toc)
    else if (kind === 'figures') generateFigures()
    else if (kind === 'index') generateIndex()
    else generateAuthorities()
  }, [generateToc, generateFigures, generateIndex, generateAuthorities, scheduleSave])

  // ── Renvois ────────────────────────────────────────────────────────────────
  const crossRefTargets = useCallback((): CrossRefTarget[] => {
    const ed = activeEditorRef.current, ops = opsRef.current
    if (!ed) return []
    const out: CrossRefTarget[] = (ops?.outline() ?? []).map(h => ({
      id: `h${h.pos}`, label: h.text, page: h.page, kind: 'heading' as const,
    }))
    for (const c of collectCaptions()) out.push({ id: `f${out.length}`, label: c.text, page: c.page, kind: 'figure' })
    ed.state.doc.descendants((node, pos) => {
      if (!node.isText) return true
      for (const m of node.marks) {
        if (m.type.name === 'bookmark' && m.attrs.name) {
          out.push({ id: `b${pos}`, label: String(m.attrs.name), page: pageOfPos(pos), kind: 'bookmark' })
        }
      }
      return true
    })
    return out
  }, [collectCaptions, pageOfPos])

  const handleCrossRefInsert = useCallback((target: CrossRefTarget, what: CrossRefWhat, asLink: boolean) => {
    const ed = activeEditorRef.current; if (!ed) return
    const text = what === 'page' ? String(target.page)
      : what === 'text' ? target.label
      : `${target.label}, ${t('doc_xref_page_word', { defaultValue: 'page' })} ${target.page}`
    const marks = asLink ? [{ type: 'link', attrs: { href: `#${target.id}` } }] : undefined
    ed.chain().focus().insertContent({ type: 'text', text, ...(marks ? { marks } : {}) }).run()
  }, [t])

  // ── Citations et bibliographie ─────────────────────────────────────────────
  const handleInsertCitationSource = useCallback((s: Source) => {
    const ed = activeEditorRef.current; if (!ed) return
    const idx = sortSources(sourcesRef.current).findIndex(x => x.id === s.id) + 1
    ed.chain().focus().insertContent(citationText(s, citationStyleRef.current, idx || 1)).run()
  }, [])

  const handleBibliography = useCallback((heading: 'bibliography' | 'references' | 'works') => {
    const list = sortSources(sourcesRef.current)
    const title = heading === 'references'
      ? t('doc_biblio_h2', { defaultValue: 'Références' })
      : heading === 'works'
        ? t('doc_biblio_h3', { defaultValue: 'Ouvrages cités' })
        : t('doc_biblio_h1', { defaultValue: 'Bibliographie' })
    const content: JSONContent[] = [
      { type: 'heading', attrs: { level: 2, tocKind: 'biblio', outlineLevel: 0 }, content: [{ type: 'text', text: title }] },
      ...(list.length
        ? list.map(s => ({
            type: 'paragraph',
            attrs: { tocKind: 'biblio', indentLeft: 24, indentFirstLine: -24, spaceAfter: 4 },
            content: [{ type: 'text', text: bibliographyEntry(s, citationStyleRef.current) }],
          }))
        : [{ type: 'paragraph', attrs: { tocKind: 'biblio' }, content: [{ type: 'text', text: t('doc_cite_empty', { defaultValue: 'Aucune source.' }) }] }]),
    ]
    const ed = activeEditorRef.current; if (!ed) return
    let start = -1, end = -1, off = 0
    ed.state.doc.forEach(node => {
      if ((node.attrs as Record<string, unknown>)?.tocKind === 'biblio') { if (start < 0) start = off; end = off + node.nodeSize }
      off += node.nodeSize
    })
    const nodes = content.map(x => ed.state.schema.nodeFromJSON(x))
    if (start >= 0) ed.view.dispatch(ed.state.tr.replaceWith(start, end, nodes))
    else {
      const $from = ed.state.selection.$from
      ed.chain().focus().insertContentAt($from.depth >= 1 ? $from.before(1) : 0, content).run()
    }
  }, [t])
  // Word « Mettre à jour les champs » : recompute every `field` node, then the TOC
  // (which is a field too, but one this page owns).
  const handleFieldsUpdate = useCallback(() => {
    const ed = activeEditorRef.current, ops = opsRef.current
    if (ed) {
      const fmtNum = (n: number) => formatPageNumber(n, pageNumFormatRef.current)
      // Page numbering starts at `pageNumStart`, exactly like the header/footer tokens.
      const pageAt = ops ? (p: number) => ops.pageAt(p) + (pageNumStartRef.current ?? 1) - 1 : undefined
      refreshFields(ed, {
        pages: ops?.pageCount(), title: titleRef2.current || null, formatNumber: fmtNum, pageAt,
        refs: pageAt ? refsFromBookmarks(docBookmarks(ed), pageAt, fmtNum) : undefined,
      })
    }
    handleTocUpdate()
  }, [handleTocUpdate])
  const handleInsertToc = useCallback(() => setTocDlgOpen(true), [])

  // ── Formes & zones de texte ─────────────────────────────────────────────────
  // Choisir une forme dans la galerie n'INSÈRE plus rien : ça ARME l'outil, et le
  // glissé suivant sur une page la dessine (aperçu live compris). C'est le geste
  // des présentations, généralisé — on voit la forme pendant qu'on la trace.
  const [armedShape, setArmedShape] = useState<ShapeKind | null>(null)
  const handleArmShape = useCallback((kind: ShapeKind) => { setArmedShape(kind) }, [])
  // Échap désarme (comme tout outil modal d'Office).
  useEffect(() => {
    if (!armedShape) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setArmedShape(null) }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [armedShape])

  // « Reproduire la mise en forme » (Format Painter façon Word). Simple clic = une
  // fois puis désarme ; double clic = mode collant (jusqu'à Échap ou re-clic).
  // Alt+Ctrl+C copie la mise en forme, Alt+Ctrl+V l'applique.
  const [paintMode, setPaintMode] = useState<false | 'once' | 'sticky'>(false)
  const paintModeRef = useRef<false | 'once' | 'sticky'>(false)
  paintModeRef.current = paintMode
  const paintCapRef = useRef<CapturedFormat | null>(null)
  const armPainter = useCallback((sticky: boolean) => {
    const ed = activeEditorRef.current; if (!ed) return
    const cap = captureFormat(ed); if (!cap) return
    paintCapRef.current = cap
    setPaintMode(sticky ? 'sticky' : 'once')
  }, [])
  const disarmPainter = useCallback(() => { paintCapRef.current = null; setPaintMode(false) }, [])
  // Simple clic : bascule (arme « une fois » / désarme). Double clic : mode collant.
  const handleFormatPainter = useCallback(() => {
    if (paintModeRef.current) disarmPainter(); else armPainter(false)
  }, [armPainter, disarmPainter])
  const handleFormatPainterSticky = useCallback(() => { armPainter(true) }, [armPainter])
  // Copie/applique la mise en forme par raccourci (Alt+Ctrl+C / Alt+Ctrl+V, façon Word).
  const copyFormat = useCallback(() => {
    const ed = activeEditorRef.current; if (!ed) return
    const cap = captureFormat(ed); if (cap) paintCapRef.current = cap
  }, [])
  const pasteFormat = useCallback(() => {
    const ed = activeEditorRef.current, cap = paintCapRef.current
    if (ed && cap) applyFormat(ed, cap)
  }, [])
  // Application sur la sélection SUIVANTE : on écoute le relâché souris (dans la zone
  // d'édition seulement — un clic sur le ruban ne doit pas déclencher l'application),
  // puis on lit la sélection que l'éditeur vient de poser. Échap annule.
  useEffect(() => {
    if (!paintMode) return
    const onUp = (e: MouseEvent) => {
      const sc = scrollRef.current
      if (!sc || !(e.target instanceof Node) || !sc.contains(e.target)) return
      // Laisse l'éditeur committer sa sélection (pointerup → PM selection) avant de peindre.
      setTimeout(() => {
        const ed = activeEditorRef.current, cap = paintCapRef.current
        if (ed && cap) applyFormat(ed, cap)
        if (paintModeRef.current === 'once') disarmPainter()
      }, 0)
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') disarmPainter() }
    document.addEventListener('mouseup', onUp, true)
    document.addEventListener('keydown', onKey, true)
    return () => {
      document.removeEventListener('mouseup', onUp, true)
      document.removeEventListener('keydown', onKey, true)
    }
  }, [paintMode, disarmPainter])
  // Raccourcis clavier globaux Alt+Ctrl+C / Alt+Ctrl+V (façon Word). Écoute en
  // phase CAPTURE + stopImmediatePropagation : sinon le keymap ProseMirror (qui lie
  // Mod-Alt-c) s'exécute d'abord et modifie le document. On le préempte.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey && e.altKey) || e.shiftKey || e.metaKey) return
      const k = e.key.toLowerCase()
      if (k !== 'c' && k !== 'v') return
      e.preventDefault(); e.stopImmediatePropagation()
      if (k === 'c') copyFormat(); else pasteFormat()
    }
    document.addEventListener('keydown', onKey, true)
    return () => document.removeEventListener('keydown', onKey, true)
  }, [copyFormat, pasteFormat])
  // Ordre de PLAN d'une forme : `zOrder` décide qui gagne le clic et qui est peint
  // au-dessus. On se place juste au-dessus du maximum (ou sous le minimum) des
  // objets du document — pas de renumérotation globale.
  const handleShapeOrder = useCallback((op: ShapeOrderOp) => {
    const ed = activeEditorRef.current; if (!ed) return
    let min = 0, max = 0
    ed.state.doc.descendants(n => {
      if (n.type.name !== 'image') return true
      const z = Number((n.attrs as Record<string, unknown>).zOrder)
      if (Number.isFinite(z)) { min = Math.min(min, z); max = Math.max(max, z) }
      return true
    })
    const front = op === 'front' || op === 'forward'
    ed.chain().focus().updateAttributes('image', { zOrder: front ? max + 1 : min - 1 }).run()
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
    onTitleChange: setTitle,
    onTitleCommit: handleTitleBlur,
    pages: opsRef.current?.pageCount() ?? 1,
    words: (activeEditorRef.current?.storage.characterCount as { words?: () => number } | undefined)?.words?.() ?? 0,
    chars: (activeEditorRef.current?.storage.characterCount as { characters?: () => number } | undefined)?.characters?.() ?? 0,
    createdAt: activeDoc.created_at,
    updatedAt: activeDoc.updated_at,
    onPrint: handlePrint,
    onExportPdf: handleExportPdf,
    onExportTxt: handleExportTxt,
    onExportServer: handleExportServer,
    sourceFormat: activeDoc.source_format ?? null,
    onSaveToSource: activeDoc.source_format ? handleSaveToSource : undefined,
    onClose: () => navigate('/office/documents'),
  } : undefined)
  // Doc ouvert → l'onglet Fichier s'ouvre sur « Informations » par défaut (le backstage
  // est démonté en quittant l'onglet → l'initial se ré-applique à chaque retour).
  const fileBackstage = <Backstage sections={backstageSections} theme={WORKSPACE_OFFICE} initial={activeDoc ? 'info' : undefined} onBack={() => setActiveTab(prevTabRef.current)} />

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
  // Petites majuscules (Police, façon Word) : bascule sur la sélection.
  const handleToggleSmallCaps = () => {
    const ed = fmtEditor; if (!ed) return
    const cur = !!ed.getAttributes('textStyle').smallCaps
    ed.chain().focus().setMark('textStyle', { smallCaps: cur ? null : true }).run()
  }
  // Espacement des caractères (Étendu/Condensé, en points) : dialogue dédié.
  const handleCharSpacing = () => setCharSpacingOpen(true)
  // Note de bas de page : insère l'appel (numéroté automatiquement) puis ouvre
  // l'éditeur de note sur le nœud fraîchement inséré.
  const handleInsertFootnote = () => {
    const ed = activeEditorRef.current; if (!ed) return
    ed.chain().focus().insertContent({ type: 'footnote', attrs: { text: '' } }).run()
    const pos = ed.state.selection.from - 1
    if (ed.state.doc.nodeAt(pos)?.type.name === 'footnote') requestAnimationFrame(() => opsRef.current?.editFootnote?.(pos))
  }
  // Note de fin : idem, numérotée en romains minuscules (voir `documents/endnotes.ts`).
  const handleInsertEndnote = () => {
    const pos = insertEndnote(activeEditorRef.current)
    if (pos != null) requestAnimationFrame(() => opsRef.current?.editEndnote?.(pos))
  }
  const handleToggleHeadingNumbers = () => {
    const v = !headingNumbersRef.current
    headingNumbersRef.current = v
    setHeadingNumbers(v)
    scheduleSave()
  }
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
    commitMargins(m)   // cible la section active (base annulable via Y.Map, sinon nœud)
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
    onSort: (dir) => opRect((ed, r) => sortTableRows(ed, r.c0, dir)),
    onCellVAlign: (v) => opRect((ed, r) => setCellsAttr(ed, r, { cellVAlign: v === 'top' ? null : v })),
    curCellVAlign: (bodyEd?.getAttributes('tableCell').cellVAlign as string) || 'top',
    onDistribute: (which) => tableOp(ed => {
      const c2 = tableCtxOf(ed); if (!c2) return
      distributeTable(ed, which, opsRef.current?.tableMetrics?.(c2.tablePos) ?? null)
    }),
    onAutoFit: (mode) => tableOp(ed => autoFitTable(ed, mode)),
    onSplitTable: () => tableOp(splitTableAtRow),
    onSum: (dir) => tableOp(ed => insertTableSum(ed, dir)),
    onBorder: (patch) => tableOp(ed => { const c2 = tableCtxOf(ed); if (c2) setTableAttrAt(ed, c2.tablePos, patch) }),
    curBorderWidth: Number(bodyEd?.getAttributes('table').tableBorderWidth) || 1,
    curBorderStyle: (bodyEd?.getAttributes('table').tableBorderStyle as string) || 'solid',
    borderColorNode: <RibbonCellColorBtn editor={bodyEd} title="Couleur des bordures" onPick={hex => tableOp(ed => { const c2 = tableCtxOf(ed); if (c2) setTableAttrAt(ed, c2.tablePos, { tableBorderColor: hex }) })} />,
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
      /** Delete `n` characters before the caret (what a recorded ⌫ replays). */
      deleteBackward: (n: unknown) => {
        const count = Math.max(0, Math.trunc(Number(n) || 0))
        for (let i = 0; i < count; i++) ed?.commands.deleteRange({ from: Math.max(0, (ed.state.selection.from) - 1), to: ed.state.selection.from })
      },
      // Character formatting, so a recording that toggles them replays exactly.
      setBold: (on: unknown) => { ed?.chain().focus()[on === false ? 'unsetBold' : 'setBold']().run() },
      setItalic: (on: unknown) => { ed?.chain().focus()[on === false ? 'unsetItalic' : 'setItalic']().run() },
      setUnderline: (on: unknown) => { ed?.chain().focus()[on === false ? 'unsetUnderline' : 'setUnderline']().run() },
      setStrike: (on: unknown) => { ed?.chain().focus()[on === false ? 'unsetStrike' : 'setStrike']().run() },
    }
    const App = {
      getType: () => 'document',
      getId: () => docId,
      toast: (m: unknown) => console.log(String(m)),
      log: (m: unknown) => console.log(String(m)),
    }
    return { Doc, App }
  }
  makeApiRef.current = makeApi

  const ribbon = buildDocumentRibbon({
    t, fmt: fmtEditor, body: activeEditor as Editor | null, fonts: ribbonFonts, fileBackstage,
    zoom, onZoom: setZoom, onZoomFit: fitZoom, onZoomDialog: () => setZoomDialogOpen(true),
    orientation: activeOrientation, onOrientation: handleOrientationChange,
    columns: baseColumns, onColumns: handleColumnsChange,
    paperSize, onPaperSize: handlePaperSize,
    pageNumbers, onPageNumbers: handleSetPageNumbers,
    onPageBreak: handleInsertPageBreak, onSectionBreak: handleInsertSectionBreak,
    // The core picker already offers every source (local file, URL, Drive…), so a
    // single entry replaces the former "upload" + "from a URL" pair.
    onInsertImage: async () => {
      const src = await pickImageSrc(t('doc_insert_image'))
      if (src) activeEditor?.chain().focus().insertContent([{ type: 'image', attrs: { src } }, { type: 'paragraph' }]).run()
    },
    onInsertTextBox: handleInsertTextBox,
    tf: t,
    armedShape, onArmShape: handleArmShape,
    onFormatPainter: handleFormatPainter, onFormatPainterSticky: handleFormatPainterSticky, formatPainterActive: paintMode !== false,
    onShapeWrap: (w: string) => opsRef.current?.setObjectWrap(w),
    onShapeOrder: handleShapeOrder,
    onShapeLayout: () => opsRef.current?.openObjectLayout(),
    // Insertion directe depuis la grille de survol du ruban.
    onInsertTableSize: (rows: number, cols: number) => {
      activeEditorRef.current?.chain().focus()
        .insertContent([makeTableNode(Math.min(50, rows), Math.min(20, cols)), { type: 'paragraph' }]).run()
    },
    // Repli « Insérer un tableau… » : saisie libre, pour dépasser la grille.
    onInsertTable: async () => {
      const v = await prompt({ title: t('doc_insert_table'), message: t('doc_table_dimensions'), defaultValue: '3 x 3', confirmLabel: t('doc_insert') })
      if (!v) return
      const m = v.match(/(\d+)\s*[x×]\s*(\d+)/i)
      if (m) activeEditor?.chain().focus().insertContent([makeTableNode(Math.min(50, +m[1]), Math.min(20, +m[2])), { type: 'paragraph' }]).run()
    },
    onSetHeader: handleSetHeader, onSetFooter: handleSetFooter,
    onInsertToc: handleInsertToc, onTocUpdate: handleTocUpdate, onSpecialChars: () => setSpecialOpen(true),
    onFieldsUpdate: handleFieldsUpdate,
    onChangeCase: handleChangeCase, onSortParas: handleSortParas, onInsertField: handleInsertField,
    onToggleSmallCaps: handleToggleSmallCaps, onCharSpacing: handleCharSpacing, onInsertFootnote: handleInsertFootnote, onInsertEndnote: handleInsertEndnote,
    headingNumbers, onToggleHeadingNumbers: handleToggleHeadingNumbers,
    onWordCount: () => setWordCountOpen(true), onInsertBookmark: handleInsertBookmark, onGoTo: () => setGoToOpen(true),
    onInsertCaption: handleInsertCaption, onInsertHr: handleInsertHr,
    onTextTool: handleTextTool, onInsertTitle: handleInsertTitle, onClearAllFormatting: handleClearAllFormatting,
    onMarginsPreset: handleMarginsPreset, onInsertCoverPage: handleInsertCoverPage,
    onEmailLink: handleEmailLink, onRemoveLinks: handleRemoveAllLinks,
    onConvertTextTable: handleConvertTextTable, onConvertTableText: handleConvertTableText,
    onSignatureLine: handleSignatureLine, onPageXofY: handleInsertPageXofY,
    showBoundaries, onToggleBoundaries: () => setShowBoundaries(v => !v),
    showMarks, onToggleMarks: () => setShowMarks(v => !v),
    recState,
    onShowMacros: () => setMacrosDlgOpen(true),
    onToggleRecord: handleToggleRecord,
    onPauseRecord: () => (recState === 'paused' ? resumeRecording() : pauseRecording()),
    outlineArrows,
    onToggleOutlineArrows: () => setOutlineArrows(v => {
      const next = !v
      try { localStorage.setItem('kb.office.collapsibleHeadings', next ? '1' : '0') } catch { /* stockage indisponible */ }
      return next
    }),
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
    trackChanges, onToggleTrackChanges: () => { setTrackChanges(v => !v); scheduleSave() }, reviewOpen, onToggleReview: () => setReviewOpen(v => !v),
    revFinal, onToggleRevFinal: () => { const next = !revFinal; setRevFinal(next); setRevisionDisplay(next ? 'final' : 'markup') },
    references: {
      t,
      onTocPreset: handleTocPreset,
      onTocCustom: () => handleTocCustom('toc'),
      onTocRemove: handleTocRemove,
      onTocUpdate: handleTocUpdate,
      hasToc: hasTable('toc'),
      outlineLevel: currentOutlineLevel(activeEditor),
      onSetOutlineLevel: n => setOutlineLevel(activeEditor, n),
      onInsertFootnote: handleInsertFootnote,
      onInsertEndnote: handleInsertEndnote,
      onGotoNote: (kind: NoteKind, dir: NoteDirection) => gotoNote(activeEditor, kind, dir),
      onShowNotes: () => {
        const pos = noteToShow(activeEditor, 'footnote') ?? noteToShow(activeEditor, 'endnote')
        if (pos == null) return
        const isFoot = noteToShow(activeEditor, 'footnote') === pos
        if (isFoot) opsRef.current?.editFootnote?.(pos)
        else opsRef.current?.editEndnote?.(pos)
      },
      hasNotes: hasNotes(activeEditor, 'footnote') || hasNotes(activeEditor, 'endnote'),
      citationStyle,
      onCitationStyle: s => { setCitationStyle(s); citationStyleRef.current = s; scheduleSave() },
      onInsertCitation: () => setSourcesOpen('pick'),
      onAddSource: () => setSourcesOpen('add'),
      onManageSources: () => setSourcesOpen('manage'),
      onBibliography: handleBibliography,
      onInsertCaption: handleInsertCaption,
      onInsertFigures: generateFigures,
      onUpdateFigures: generateFigures,
      onCrossRef: () => setCrossRefOpen(true),
      onMarkIndex: () => setMarkDlg('index'),
      onInsertIndex: generateIndex,
      onUpdateIndex: generateIndex,
      hasIndex: hasTable('index'),
      onMarkCitation: () => setMarkDlg('citation'),
      onInsertAuthorities: generateAuthorities,
      onUpdateAuthorities: generateAuthorities,
      hasAuthorities: hasTable('authorities'),
    },
    onApplyStyle: handleApplyStyle, onEditStyles: () => setStylesEditorOpen(true), styleList, curStyleId,
    table: tableCtx,
    onNew: handleNew, onDuplicate: handleDuplicate, onPrint: handlePrint,
    onExportPdf: handleExportPdf, onExportTxt: handleExportTxt, onExportServer: handleExportServer, online,
    pageColorNode: <RibbonPageColorBtn pageColor={pageColor} pageGrad={pageGrad} onColor={onPageColorHex} onGrad={onPageGradient} />,
    watermarkNode: <RibbonWatermarkBtn value={watermark} onChange={onWatermarkChange} />,
    pageBorderNode: <RibbonPageBorderBtn value={pageBorder} onChange={onPageBorderChange} />,
    lineNumbersNode: <RibbonLineNumbersBtn value={lineNumbers} onChange={onLineNumbersChange} />,
    hf: hfBar,
    onHFField: tok => opsRef.current?.insertHFField(tok),
    onHFSwitch: () => opsRef.current?.switchHF(),
    onHFFirstPage: setHfFirstPageOpt, onHFLinked: setHfLinkedOpt,
    onHFClose: () => opsRef.current?.exitHF(),
    cellRanges,
  })

  // Ouvre le dialogue de partage (réutilisé par le bouton plein et l'entrée du
  // menu ⋮ de la vue lecture mobile).
  const openDocShare = () => { void openShare?.({
    target: { moduleId: 'office', id: docId, kind: 'document' },
    api: {
      list:   officeApi.listCollaborators,
      add:    (i, u, p) => officeApi.addCollaborator(i, u, p as CollabPermission),
      update: (i, u, p) => officeApi.updateCollaborator(i, u, p as CollabPermission),
      remove: officeApi.removeCollaborator,
      searchRecipients: officeApi.searchRecipients,
    },
    title: title || t('doc_untitled', { defaultValue: 'Document sans titre' }),
    permissions: ['edit', 'comment', 'view'],
    permissionLabel: p => (p === 'edit' ? 'Éditeur' : p === 'comment' ? 'Commentateur' : 'Lecteur'),
  }) }
  // Vue LECTURE mobile = immersion : la rangée du haut se réduit à l'essentiel
  // (retour + « Modifier » + Partager) ; le cluster core (statut/notifs/réglages/
  // avatar) est masqué et les actions d'édition (Macros, présence) retirées —
  // sinon 7-8 contrôles se chevauchent sur la largeur d'un téléphone.
  const readMobile = isMobileView && mode === 'read'

  return (
    <OfficeShell
      hideHeaderActions={readMobile}
      ribbon={isMobileView && mode === 'read' ? [] : ribbon}
      activeTabId={activeTab}
      onTabChange={handleTabChange}
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
      topbarActions={
        readMobile ? (
          // Immersion lecture : Partager en icône seule (le reste passe en édition).
          <button onClick={openDocShare} title={t('share_button', 'Partager')}
            className="p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 text-white/90">
            <UserPlus size={16} />
          </button>
        ) : (
        <div className="flex items-center gap-2">
          {/* Bandeau hors-ligne : édition persistée localement, fusion au retour réseau. */}
          {!online && (
            <span className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-amber-500/15 text-amber-700 text-xs font-medium whitespace-nowrap"
              title={t('doc_offline_hint', { defaultValue: 'Vos modifications sont enregistrées localement et seront synchronisées au retour de la connexion.' })}>
              <CloudOff size={14} /> {t('doc_offline_badge', { defaultValue: 'Hors-ligne' })}
            </span>
          )}
          <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
          <ShareButton onShare={openDocShare} label={t('share_button', 'Partager')} />
        </div>
        )
      }
      titleActions={
        <>
          {/* Mobile : bascule lecture ↔ édition façon Word. En lecture, action
              PRIMAIRE = pastille « Modifier » (rangée épurée) ; en édition, simple
              œil (retour lecture) car la barre porte déjà les actions d'édition. */}
          {readMobile ? (
            <button onClick={() => setMode('edit')}
              className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-white/15 text-white text-sm font-medium border border-white/25 hover:bg-white/25 transition-colors flex-shrink-0"
              title={t('common_edit', { defaultValue: 'Modifier' })}>
              <PenLine size={15} /> {t('common_edit', { defaultValue: 'Modifier' })}
            </button>
          ) : isMobileView && (
            <button onClick={() => setMode(mode === 'read' ? 'edit' : 'read')}
              className="p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 text-white/90"
              title={mode === 'read' ? t('doc_mode_edit', { defaultValue: 'Modifier' }) : t('doc_mode_read', { defaultValue: 'Lecture' })}>
              {mode === 'read' ? <PenLine size={16} /> : <Eye size={16} />}
            </button>
          )}
          {(!isMobileView || mode === 'edit') && <>
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
          </>}
        </>
      }
    >
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
        {/* Cursor blink animation — injected once per editor session */}
        <style>{CURSOR_STYLE}</style>

        {/* Recherche dans le document → branchée sur la SearchBar standard du core. */}
        <DocFindController editor={activeEditor as Editor | null} highlight={setSearchHi} focusSignal={searchFocusTick} />


      {/* ── Trashed banner ──────────────────────────────────────────────── */}
      <TrashedDocBanner docId={docId} />

      {/* Dictée vocale (mobile, édition) : micro flottant + toast STT du core. */}
      {isMobileView && mode === 'edit' && <DictationFab editorRef={activeEditorRef} lang={spellLang} />}
      {/* La barre de menus + la toolbar sont remplacées par le RUBAN (OfficeShell). */}

      {/* ── Ruler row + scrollable canvas ───────────────────────────────── */}
      <div className="flex flex-1 min-h-0 overflow-hidden relative">

        {specialOpen && activeEditor && (
          <SpecialCharsBar editor={activeEditor as Editor} onClose={() => setSpecialOpen(false)} />
        )}

        {navOpen && (
          <NavPane editor={activeEditor} opsRef={opsRef} onClose={() => setNavOpen(false)} />
        )}

        {reviewOpen && (
          <ReviewPane editor={activeEditor} onClose={() => setReviewOpen(false)}
            onScrollToChange={c => { if (c.from != null) opsRef.current?.scrollToPos(c.from) }} />
        )}

        {/* Pas de `title` sur le CONTENEUR de la règle : le canvas porte déjà le
            sien (contextuel). Deux `title` imbriqués = deux bulles — l'intercepteur
            du shell affiche celle du canvas, le navigateur celle du conteneur. */}
        {showRuler && mode === 'edit' && !isMobileView && (
        <div style={{ width: RULER_SZ, flexShrink: 0, display: 'flex', flexDirection: 'column' }}
             onDoubleClick={() => setPageSetupOpen(true)}>
          <CornerCell tabType={tabType} onCycle={() => setTabType(tt => TAB_CYCLE[(TAB_CYCLE.indexOf(tt) + 1) % TAB_CYCLE.length])} />
          <VerticalRuler
            scrollRef={scrollRef}
            activePage={rulerPage}
            activePageTop={activePageBox?.top}
            zoom={zoom}
            marginTop={activeMargins.top}
            marginBottom={activeMargins.bottom}
            pageH={activeGeo.pageH}
            pageGap={PAGE_GAP + PAGE_MARGIN_TOP}
            onMarginsChange={(top, bottom) => commitMargins({ ...marginsRef.current, top, bottom })}
            onDragGuideChange={g => setDragGuide(g ? { type: 'horizontal', clientY: g.clientY } : null)}
            tableRows={tblRulerMarks?.rows}
            onTableBoundDown={(i, e) => opsRef.current?.tableBoundDown?.('row', i, e)}
          />
        </div>
        )}

        <div className="flex flex-col flex-1 min-w-0 overflow-hidden">

          {showRuler && mode === 'edit' && !isMobileView && (
          <div className="flex-shrink-0 bg-[#f1f3f4] border-b border-[#dadce0] relative z-10" style={{ height: RULER_SZ, overflow: 'visible' }}
               onDoubleClick={() => setPageSetupOpen(true)}>
            {/* Calée sur la page ACTIVE (marginLeft = son x dans le contenu) ; en colonne
                unique cette valeur correspond au centrage → rétro-compatible. */}
            <div ref={hRulerBoxRef} className={activePageBox ? 'h-full' : 'h-full flex justify-center'}
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
                onIndentsChange={(ind, commit) => commitIndents(ind, commit)}
                onMarginsChange={(left, right) => commitMargins({ ...marginsRef.current, left, right })}
                onDragGuideChange={g => setDragGuide(g ? { type: 'vertical', clientX: g.clientX } : null)}
                onOpenIndents={() => opsRef.current?.openParagraph()}
                tableCols={tblRulerMarks?.cols}
                tableCell={tblRulerMarks?.cell}
                onTableBoundDown={(i, e) => opsRef.current?.tableBoundDown?.('col', i, e)}
              />
            </div>
          </div>
          )}

          <div
            ref={setScrollNode}
            data-doc-scroll
            className="flex-1 overflow-auto"
            // touch-action pan-x pan-y : le navigateur ne garde QUE le défilement —
            // le pincement à 2 doigts nous revient (zoom du DOCUMENT, pas de la
            // page web). Indispensable sur iOS Safari, où preventDefault(touchmove)
            // seul ne suffit pas à bloquer le zoom natif du viewport.
            style={{ background: '#f1f3f4', touchAction: 'pan-x pan-y' }}
          >
            <div className="flex" style={{ justifyContent: 'safe center' }}>
              <PaginatedEditor
                key={docId}
                ydoc={ydoc}
                awareness={awareness}
                collabEmpty={collabEmpty}
                initialDoc={flattenToDoc(activeDoc.content_json as object | null)}
                section={{ id: sectionIdRef.current, orientation: baseOrientation, margins: baseMargins, columns: baseColumns }}
                zoom={zoom}
                scrollContainerRef={scrollRef}
                onEditor={(ed) => { activeEditorRef.current = ed; setActiveEditor(ed) }}
                onSave={(doc) => { docRef.current = doc; scheduleSave() }}
                onBaseChange={handleBaseChange}
                onActiveSection={({ orientation, secIdx, margins }) => {
                  // La règle/le dialogue suivent la section du curseur.
                  setActiveOrientation(orientation)
                  setActiveSecIdx(secIdx); activeSecIdxRef.current = secIdx
                  setActiveMargins(margins); marginsRef.current = margins
                }}
                onRegisterOps={registerOps}
                armedShape={armedShape}
                onShapeDrawn={() => setArmedShape(null)}
                paintMode={paintMode !== false}
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
                onPageBorder={onPageBorderChange}
                lineNumbers={lineNumbers}
                showBoundaries={showBoundaries}
                showMarks={showMarks}
                pageNumFormat={pageNumFormat}
                pageNumStart={pageNumStart}
                headingNumbers={headingNumbers}
                spellCheck={spellOn}
                grammarCheck={grammarOn}
                grammarRules={grammarRules}
                onOpenGrammarCheck={setGrammarCheckIssue}
                onSpellCount={setSpellCount}
                onStats={setDocStats}
                spellVersion={spellVersion}
                searchRanges={searchHi.ranges}
                searchActive={searchHi.active}
                activeCommentId={activeCommentId}
                onCommentActivate={handleCommentActivate}
                onCommentRanges={setCommentIds}
                onAddComment={handleAddComment}
                commentsMap={commentsMap}
                commentUser={{ id: authUser?.id || '', name: authUser?.display_name || authUser?.username || authUser?.email || 'Anonyme' }}
                commentsVisible={commentsOpen}
                onTableSel={setTableSel}
                outlineArrows={outlineArrows}
                tocControl={{
                  onPreset: handleTocPreset,
                  onRemove: handleTocRemove,
                  onUpdate: () => setUpdateTocOpen(true),
                }}
              />
            </div>
          </div>
        </div>

        {/* En-tête flottant des commentaires (les cartes sont ancrées dans la marge
            droite de chaque page via CommentGutter, façon Word). */}
        {commentsOpen && (
          <div className="absolute top-2 right-3 z-30 flex items-center gap-2 bg-surface-1 border border-border rounded-full shadow-sm pl-3 pr-1.5 h-8">
            <MessageSquare size={14} className="text-text-secondary" />
            <span className="text-xs font-medium text-text-primary">{t('doc_comments', { defaultValue: 'Commentaires' })}{commentIds.length ? ` (${commentIds.length})` : ''}</span>
            <button onClick={() => { setCommentsOpen(false); setActiveCommentId(null) }}
              className="p-1 rounded-full hover:bg-surface-2 text-text-secondary" title={t('common_close', { defaultValue: 'Fermer' })}><X size={14} /></button>
          </div>
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
        onToggleSpell={() => { setSpellOn(v => !v); scheduleSave() }}
        onOpenSpell={() => setSpellDictOpen(true)}
        spellLang={spellLang}
        spellAuto={spellAuto}
        grammarOn={grammarOn}
        onToggleGrammar={() => { setGrammarOn(v => !v); scheduleSave() }}
        showRuler={showRuler}
        onToggleRuler={() => setShowRuler(v => !v)}
      />

      <DragGuideLine guide={dragGuide} />
      </div>

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
        <SpellDictionaryDialog
          editor={activeEditor as Editor | null}
          spellLang={spellLang} spellAuto={spellAuto} spellOn={spellOn}
          onApplyLang={({ lang, auto, check, scope }, asDefault) => {
            if (scope === 'selection') {
              // Langue PAR PLAGE : pose la marque `spellLang` sur la sélection (persistée
              // dans le contenu → sauvegardée dans le fichier). Ne touche pas la langue du doc.
              const ed = activeEditor as Editor | null
              if (ed) ed.chain().focus().setMark('spellLang', { lang }).run()
              setSpellVersion(v => v + 1)
              return
            }
            setSpellLang(lang); setSpellAuto(auto); setSpellOn(check)
            scheduleSave()   // persistance PAR DOCUMENT (fichier)
            if (asDefault) {  // « Définir par défaut » → défaut GLOBAL des nouveaux documents
              localStorage.setItem('kb.office.spellLang', lang)
              localStorage.setItem('kb.office.spellAuto', auto ? '1' : '0')
              localStorage.setItem('kb.office.spellOn', check ? '1' : '0')
              localStorage.setItem('kb.office.grammarOn', grammarOnRef.current ? '1' : '0')
            }
          }}
          onChange={() => setSpellVersion(v => v + 1)}
          onClose={() => setSpellDictOpen(false)} />
      )}
      {grammarCheckIssue && (
        <VerificationDialog
          issue={grammarCheckIssue}
          editor={activeEditor as Editor | null}
          rules={grammarRules}
          onApplyRules={(r) => { setGrammarRules(r); grammarRulesRef.current = r; scheduleSave() }}
          onRecheck={() => setSpellVersion(v => v + 1)}
          onClose={() => setGrammarCheckIssue(null)} />
      )}
      {/* Word « Table des matières personnalisée… » : la fenêtre à 4 onglets
          (Index / Table des matières / Table des illustrations / Table des
          références) et ses sous-fenêtres Options… et Modifier…. */}
      {tocDlgOpen && (
        <ReferencesDialog
          initialTab={refTab}
          settings={refSettings}
          styleNames={styleList.map(s => styleLabel(s, t))}
          captionLabels={[...new Set(collectCaptions().map(c => c.label))].filter((x): x is string => !!x)}
          onApply={handleReferencesApply}
          onClose={() => setTocDlgOpen(false)}
        />
      )}
      {sourcesOpen && (
        <SourcesDialog
          sources={sources}
          pickMode={sourcesOpen === 'pick'}
          onSave={list => { setSources(list); sourcesRef.current = list; scheduleSave() }}
          onInsert={handleInsertCitationSource}
          onClose={() => setSourcesOpen(false)}
        />
      )}
      {markDlg && (
        <MarkEntryDialog
          mode={markDlg}
          initial={selectedText(activeEditor)}
          onMarkIndex={(text, sub) => markIndexEntry(activeEditor, text, sub)}
          onMarkCitation={(long, short, cat) => markCitation(activeEditor, long, short, cat)}
          onClose={() => setMarkDlg(false)}
        />
      )}
      {macrosDlgOpen && (
        <MacrosDialog
          macros={docMacros.macros.map(m => ({ key: m.scriptId, label: m.label, description: m.description }))}
          docTitle={title || 'Document'}
          busy={macroBusy}
          onRun={sid => { setMacrosDlgOpen(false); void runMacroByScript(sid) }}
          onEdit={sid => { setMacrosDlgOpen(false); navigate(`/office/script/${sid}`) }}
          onCreate={name => {
            setMacrosDlgOpen(false)
            void docMacros.create.mutateAsync({
              name,
              source: `// ${name}\nKubuno.Doc.insertText('Bonjour depuis une macro !')\n`,
            }).then(sid => navigate(`/office/script/${sid}`))
          }}
          onDelete={sid => {
            const row = docMacros.macros.find(m => m.scriptId === sid)
            if (row) void docMacros.remove.mutateAsync(row.key)
          }}
          onClose={() => setMacrosDlgOpen(false)}
        />
      )}
      {updateTocOpen && (
        <UpdateTocDialog onApply={handleTocUpdateMode} onClose={() => setUpdateTocOpen(false)} />
      )}
      {crossRefOpen && (
        <CrossRefDialog
          targets={crossRefTargets()}
          onInsert={handleCrossRefInsert}
          onClose={() => setCrossRefOpen(false)}
        />
      )}

      {/* Espacement des caractères (Police avancée, façon Word) */}
      {charSpacingOpen && (() => {
        const ed = fmtEditor
        const cur = ed ? parseFloat(String(ed.getAttributes('textStyle').letterSpacing ?? '0')) || 0 : 0
        return (
          <CharSpacingDialog
            initial={cur}
            onClose={() => setCharSpacingOpen(false)}
            onApply={(pt) => {
              ed?.chain().focus().setMark('textStyle', { letterSpacing: pt || null }).run()
              setCharSpacingOpen(false)
            }}
          />
        )
      })()}

      {pageSetupOpen && (
        <PageSetupDialog
          init={{ margins: activeMargins, orientation: activeOrientation, paper: paperSize, gutter, headerDist, footerDist, vAlign: vAlignPage, sectionStart, evenOdd, firstPageDiff: hfFirstPage }}
          onApply={v => {
            commitMargins(v.margins)   // marges de la section active (annulables)
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
