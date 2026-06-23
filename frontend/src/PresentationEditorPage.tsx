import {
  useState, useEffect, useRef, useCallback, useMemo,
  type ReactNode, type MouseEvent as ReactMouseEvent,
} from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Star, Plus, Trash2, Copy, ChevronDown,
  MousePointer, Type, Square, Minus, Image as ImageIcon,
  Play, X, Eye, EyeOff, ChevronRight, ChevronLeft, LayoutTemplate,
  AlignLeft, Bold, Italic, Underline as UnderlineIcon, UserPlus,
  MoveUpRight, CornerDownRight, Spline, Cable, Waypoints, PenTool,
  Scissors, ClipboardPaste, ClipboardX, CopyPlus, FilePlus2, Droplet,
  Paintbrush, Replace, MessageSquarePlus,
  RotateCw, AlignVerticalSpaceAround, Ban, Shrink, Hash,
  Accessibility, Focus, Link as LinkIcon, Sparkles, Maximize2,
  AlignCenter, AlignRight, List, ListOrdered, IndentIncrease, IndentDecrease,
  RemoveFormatting, Crop, Highlighter,
  ArrowUpToLine, AlignHorizontalSpaceAround, Undo2, Redo2, FileDown,
  BringToFront, SendToBack, AlignStartVertical, AlignCenterVertical, AlignEndVertical,
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignHorizontalDistributeCenter, AlignVerticalDistributeCenter, Wand2, Film, Shapes,
} from 'lucide-react'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { useSystemFonts } from './systemAssets'
import { THEME_PRESENTATION } from './ribbon/officeThemes'
import { useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'
import { PresentationStartContent } from './PresentationStartContent'
import type { FileItem } from '@kubuno/drive'
import { StatusBar, StatusButton, StatusSep, StatusSpacer } from './shell/StatusBar'
import {
  presentationsApi, officeApi, Presentation, Slide, SlideSummary,
  SlideElement, TextElement, ShapeElement, ImageElement, LineElement,
  SlideBackground, LineKind,
} from './api'
import CollaboratorsDialog from './CollaboratorsDialog'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { usePresenceUsers, PresenceAvatarList, userColor, initials, usePublishCursor, RemoteCursors, type PresenceUser } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import { Button, ColorField, GradientField, rgbaFromHex, DEFAULT_GRADIENT, type Gradient, ResizeHandle, useResizableWidth, Dropdown, FontPicker, MenuDropdown, type MenuItem } from '@ui'
import { prompt } from '@kubuno/sdk'
import { pagesToPdf, downloadBlob } from './pdfExport'
import type { RibbonTab } from './ribbon/types'
import { MacrosMenu } from './macros/MacrosMenu'

// Build a canvas gradient (linear with angle, or radial) from the core Gradient model.
function buildCanvasGradient(ctx: CanvasRenderingContext2D, g: Gradient, bx: number, by: number, w: number, h: number): CanvasGradient {
  const cx = bx + w / 2, cy = by + h / 2
  let grad: CanvasGradient
  if (g.type === 'radial') {
    grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(w, h) / 2)
  } else {
    const ang = (g.angle ?? 0) * Math.PI / 180
    const dx = Math.cos(ang), dy = Math.sin(ang)
    const half = (Math.abs(dx) * w + Math.abs(dy) * h) / 2
    grad = ctx.createLinearGradient(cx - dx * half, cy - dy * half, cx + dx * half, cy + dy * half)
  }
  for (const s of [...g.stops].sort((a, b) => a.position - b.position))
    grad.addColorStop(Math.max(0, Math.min(1, s.position)), rgbaFromHex(s.color, s.opacity))
  return grad
}

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_THEME = {
  name: 'Défaut',
  primaryColor: '#1a73e8',
  bgColor: '#ffffff',
  fontFamily: 'Google Sans, Arial, sans-serif',
  accentColor: '#ea4335',
  textColor: '#202124',
}

const SLIDE_W = 960
const SLIDE_H = 540

// Animations d'entrée d'élément (jouées en mode diaporama, au clic).
const PRES_ANIMATIONS: { type: string; nameKey: string; label: string }[] = [
  { type: 'none',   nameKey: 'pres_anim_none',   label: 'Aucune' },
  { type: 'fade',   nameKey: 'pres_anim_fade',   label: 'Fondu' },
  { type: 'flyL',   nameKey: 'pres_anim_flyl',   label: 'Entrée par la gauche' },
  { type: 'flyR',   nameKey: 'pres_anim_flyr',   label: 'Entrée par la droite' },
  { type: 'flyT',   nameKey: 'pres_anim_flyt',   label: 'Entrée par le haut' },
  { type: 'flyB',   nameKey: 'pres_anim_flyb',   label: 'Entrée par le bas' },
  { type: 'zoom',   nameKey: 'pres_anim_zoom',   label: 'Zoom' },
  { type: 'rise',   nameKey: 'pres_anim_rise',   label: 'Apparition' },
]
// Formes insérables (rendu canvas paramétrique). label = libellé de repli.
const SHAPE_KINDS: { kind: string; nameKey: string; label: string }[] = [
  { kind: 'rect', nameKey: 'pres_shape_rect', label: 'Rectangle' },
  { kind: 'roundRect', nameKey: 'pres_shape_round', label: 'Rect. arrondi' },
  { kind: 'ellipse', nameKey: 'pres_shape_ellipse', label: 'Ellipse' },
  { kind: 'triangle', nameKey: 'pres_shape_triangle', label: 'Triangle' },
  { kind: 'diamond', nameKey: 'pres_shape_diamond', label: 'Losange' },
  { kind: 'pentagon', nameKey: 'pres_shape_pentagon', label: 'Pentagone' },
  { kind: 'hexagon', nameKey: 'pres_shape_hexagon', label: 'Hexagone' },
  { kind: 'star', nameKey: 'pres_shape_star', label: 'Étoile' },
  { kind: 'rightArrow', nameKey: 'pres_shape_arrow', label: 'Flèche' },
  { kind: 'chevron', nameKey: 'pres_shape_chevron', label: 'Chevron' },
  { kind: 'plus', nameKey: 'pres_shape_plus', label: 'Croix' },
  { kind: 'speech', nameKey: 'pres_shape_speech', label: 'Bulle' },
  { kind: 'heart', nameKey: 'pres_shape_heart', label: 'Cœur' },
]

// Transitions entre diapositives (rendues en mode diaporama).
const PRES_TRANSITIONS: { type: string; nameKey: string; label: string }[] = [
  { type: 'none',     nameKey: 'pres_trans_none',   label: 'Aucune' },
  { type: 'fade',     nameKey: 'pres_trans_fade',   label: 'Fondu' },
  { type: 'slideL',   nameKey: 'pres_trans_slidel', label: 'Glissement ←' },
  { type: 'slideR',   nameKey: 'pres_trans_slider', label: 'Glissement →' },
  { type: 'slideU',   nameKey: 'pres_trans_slideu', label: 'Glissement ↑' },
  { type: 'zoom',     nameKey: 'pres_trans_zoom',   label: 'Zoom' },
  { type: 'flip',     nameKey: 'pres_trans_flip',   label: 'Retournement' },
]

// ── Unique ID generator ───────────────────────────────────────────────────────

function uid() {
  return Math.random().toString(36).slice(2, 10)
}

// Éléments par défaut d'une nouvelle diapositive : placeholders titre + sous-titre
// (façon Google Slides). Contenu vide → le placeholder s'affiche jusqu'à la saisie.
function placeholderSlideElements(t: (k: string) => string): TextElement[] {
  return [
    {
      id: uid(), type: 'text', x: 0.08, y: 0.30, w: 0.84, h: 0.28,
      rotation: 0, zIndex: 1, locked: false, hidden: false,
      content: null, padding: 8, verticalAlign: 'middle',
      background: null, borderRadius: 0,
      placeholder: t('pres_ph_title'), fontSize: 44, align: 'center', color: '#3c4043',
    },
    {
      id: uid(), type: 'text', x: 0.08, y: 0.60, w: 0.84, h: 0.13,
      rotation: 0, zIndex: 2, locked: false, hidden: false,
      content: null, padding: 8, verticalAlign: 'middle',
      background: null, borderRadius: 0,
      placeholder: t('pres_ph_subtitle'), fontSize: 22, align: 'center',
    },
  ]
}

// ── Images de diapositive : cache + résolution des références ──────────────────
//
// Les images ne sont PLUS stockées en base64 dans le document Yjs (qui explosait
// en taille) mais comme fichiers cachés côté serveur. Un élément image stocke une
// référence compacte « kbfile:<id> » dans `storagePath` ; on la résout ici en blob
// authentifié servi par l'API office. Cache GLOBAL partagé par tous les canevas
// (1 seul fetch par asset) + notification pour redessiner au chargement.
const slideImageCache = new Map<string, HTMLImageElement>()
const slideImageListeners = new Set<() => void>()
function onSlideImageLoaded(cb: () => void): () => void {
  slideImageListeners.add(cb)
  return () => { slideImageListeners.delete(cb) }
}
function notifySlideImageLoaded() {
  slideImageListeners.forEach(cb => { try { cb() } catch { /* ignore */ } })
}

// Présentation actuellement ouverte (pour résoudre/uploader les assets sans avoir
// à propager l'id à travers tous les composants/canevas). Une seule à la fois.
let currentPresId: string | undefined

// Résout un `storagePath` (kbfile:<id> | data: | http) en HTMLImageElement chargé
// de façon asynchrone. Renvoie immédiatement l'élément (peut-être pas encore prêt).
function resolveSlideImage(src: string): HTMLImageElement {
  const cached = slideImageCache.get(src)
  if (cached) return cached
  const img = new window.Image()
  slideImageCache.set(src, img)
  if (src.startsWith('kbfile:')) {
    const fileId = src.slice('kbfile:'.length)
    if (currentPresId) {
      presentationsApi.fetchAssetBlob(currentPresId, fileId)
        .then(blob => { img.onload = notifySlideImageLoaded; img.src = URL.createObjectURL(blob) })
        .catch(() => { slideImageCache.delete(src) }) // permet une nouvelle tentative
    } else {
      slideImageCache.delete(src) // pas de présentation courante → réessayer plus tard
    }
  } else {
    img.onload = notifySlideImageLoaded
    img.src = src // data: (héritée) ou http(s):
  }
  return img
}

// Lit un fichier image → blob (réduit si trop grand) + dimensions natives.
function loadImageFile(file: File, maxDim = 1600): Promise<{ blob: Blob; w: number; h: number }> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const dataUrl = String(reader.result)
      const img = new window.Image()
      img.onload = () => {
        const w = img.naturalWidth, h = img.naturalHeight
        const scale = Math.min(1, maxDim / Math.max(w, h || 1))
        if (scale < 1) {
          const cw = Math.max(1, Math.round(w * scale)), ch = Math.max(1, Math.round(h * scale))
          const c = document.createElement('canvas'); c.width = cw; c.height = ch
          const cx = c.getContext('2d')
          if (cx) {
            cx.drawImage(img, 0, 0, cw, ch)
            const type = file.type === 'image/png' ? 'image/png' : 'image/jpeg'
            c.toBlob(b => resolve({ blob: b ?? file, w, h }), type, 0.9)
            return
          }
        }
        resolve({ blob: file, w, h })
      }
      img.onerror = () => reject(new Error('image decode failed'))
      img.src = dataUrl
    }
    reader.onerror = () => reject(new Error('file read failed'))
    reader.readAsDataURL(file)
  })
}

// Décode une data URL en Blob SANS fetch() — la CSP (connect-src) bloque le fetch
// des data: URLs ; on parse donc le base64 nous-mêmes.
function dataUrlToBlob(dataUrl: string): Blob | null {
  const m = /^data:([^;,]+)?(;base64)?,([\s\S]*)$/.exec(dataUrl)
  if (!m) return null
  const mime = m[1] || 'application/octet-stream'
  const body = m[3]
  if (m[2]) {
    const bin = atob(body)
    const u8 = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
    return new Blob([u8], { type: mime })
  }
  return new Blob([decodeURIComponent(body)], { type: mime })
}

// Upload une image (réduite) et renvoie sa référence compacte « kbfile:<id> ».
// Repli sur une data URL inline si aucune présentation n'est ouverte (improbable).
async function uploadImageRef(file: File): Promise<{ ref: string; w: number; h: number }> {
  const { blob, w, h } = await loadImageFile(file)
  if (currentPresId) {
    const { ref } = await presentationsApi.uploadAsset(currentPresId, blob, file.name || 'image')
    return { ref, w, h }
  }
  const url: string = await new Promise(res => {
    const r = new FileReader(); r.onload = () => res(String(r.result)); r.readAsDataURL(blob)
  })
  return { ref: url, w, h }
}

// Construit un ImageElement dimensionné au ratio natif, centré sur (cx, cy).
function makeImageElement(url: string, natW: number, natH: number, cx: number, cy: number, zIndex: number): ImageElement {
  const aspect = (natH / (natW || 1)) * (SLIDE_W / SLIDE_H) // h/w en coordonnées normalisées
  let w = 0.5, h = w * aspect
  if (h > 0.85) { h = 0.85; w = h / aspect }
  if (w > 0.9) { w = 0.9; h = w * aspect }
  const x = Math.max(0, Math.min(1 - w, cx - w / 2))
  const y = Math.max(0, Math.min(1 - h, cy - h / 2))
  return { id: uid(), type: 'image', x, y, w, h, rotation: 0, zIndex, locked: false, hidden: false, storagePath: url, alt: '', opacity: 1 }
}

// ── Mises en page (layouts) — façon Google Slides ───────────────────────────────

function mkText(o: Partial<TextElement> & { x: number; y: number; w: number; h: number }): TextElement {
  return {
    id: uid(), type: 'text', rotation: 0, zIndex: 1, locked: false, hidden: false,
    content: null, padding: 8, verticalAlign: 'middle', background: null, borderRadius: 0,
    placeholder: null, ...o,
  }
}

type LayoutDef = { id: string; nameKey: string; build: (t: (k: string) => string) => TextElement[] }

const SLIDE_LAYOUTS: LayoutDef[] = [
  { id: 'title', nameKey: 'pres_lay_title', build: t => [
    mkText({ x: 0.08, y: 0.34, w: 0.84, h: 0.20, placeholder: t('pres_ph_title'), fontSize: 44, align: 'center', color: '#3c4043', zIndex: 1 }),
    mkText({ x: 0.08, y: 0.57, w: 0.84, h: 0.10, placeholder: t('pres_ph_subtitle'), fontSize: 20, align: 'center', zIndex: 2 }),
  ] },
  { id: 'section', nameKey: 'pres_lay_section', build: t => [
    mkText({ x: 0.1, y: 0.40, w: 0.8, h: 0.18, placeholder: t('pres_ph_title'), fontSize: 36, align: 'left', color: '#3c4043' }),
  ] },
  { id: 'title_body', nameKey: 'pres_lay_title_body', build: t => [
    mkText({ x: 0.06, y: 0.06, w: 0.88, h: 0.16, placeholder: t('pres_ph_title'), fontSize: 30, align: 'left', color: '#3c4043' }),
    mkText({ x: 0.06, y: 0.26, w: 0.88, h: 0.66, placeholder: t('pres_ph_body'), fontSize: 18, align: 'left', verticalAlign: 'top', zIndex: 2 }),
  ] },
  { id: 'two_col', nameKey: 'pres_lay_two_col', build: t => [
    mkText({ x: 0.06, y: 0.06, w: 0.88, h: 0.14, placeholder: t('pres_ph_title'), fontSize: 28, align: 'left', color: '#3c4043' }),
    mkText({ x: 0.06, y: 0.24, w: 0.43, h: 0.68, placeholder: t('pres_ph_body'), fontSize: 16, align: 'left', verticalAlign: 'top', zIndex: 2 }),
    mkText({ x: 0.51, y: 0.24, w: 0.43, h: 0.68, placeholder: t('pres_ph_body'), fontSize: 16, align: 'left', verticalAlign: 'top', zIndex: 3 }),
  ] },
  { id: 'title_only', nameKey: 'pres_lay_title_only', build: t => [
    mkText({ x: 0.06, y: 0.06, w: 0.88, h: 0.16, placeholder: t('pres_ph_title'), fontSize: 30, align: 'left', color: '#3c4043' }),
  ] },
  { id: 'one_col', nameKey: 'pres_lay_one_col', build: t => [
    mkText({ x: 0.1, y: 0.1, w: 0.8, h: 0.8, placeholder: t('pres_ph_body'), fontSize: 18, align: 'left', verticalAlign: 'top' }),
  ] },
  { id: 'main', nameKey: 'pres_lay_main', build: t => [
    mkText({ x: 0.08, y: 0.34, w: 0.84, h: 0.30, placeholder: t('pres_ph_title'), fontSize: 48, align: 'left', color: '#3c4043' }),
  ] },
  { id: 'section_desc', nameKey: 'pres_lay_section_desc', build: t => [
    mkText({ x: 0.05, y: 0.18, w: 0.42, h: 0.18, placeholder: t('pres_ph_title'), fontSize: 26, align: 'left', color: '#3c4043' }),
    mkText({ x: 0.05, y: 0.40, w: 0.42, h: 0.42, placeholder: t('pres_ph_body'), fontSize: 15, align: 'left', verticalAlign: 'top', zIndex: 2 }),
  ] },
  { id: 'caption', nameKey: 'pres_lay_caption', build: t => [
    mkText({ x: 0.1, y: 0.78, w: 0.8, h: 0.12, placeholder: t('pres_ph_body'), fontSize: 16, align: 'left' }),
  ] },
  { id: 'number', nameKey: 'pres_lay_number', build: t => [
    mkText({ x: 0.1, y: 0.28, w: 0.8, h: 0.30, placeholder: 'xx%', fontSize: 72, align: 'center', color: '#3c4043' }),
    mkText({ x: 0.1, y: 0.62, w: 0.8, h: 0.10, placeholder: t('pres_ph_body'), fontSize: 18, align: 'center', zIndex: 2 }),
  ] },
  { id: 'blank', nameKey: 'pres_lay_blank', build: () => [] },
]

// Aperçu miniature d'une mise en page (rectangles = placeholders).
function LayoutPreview({ els }: { els: TextElement[] }) {
  return (
    <div className="relative w-full aspect-[16/9] bg-white border border-border rounded-sm overflow-hidden">
      {els.map(el => (
        <div
          key={el.id}
          className="absolute border border-border/70 flex items-center px-0.5 overflow-hidden"
          style={{
            left: `${el.x * 100}%`, top: `${el.y * 100}%`,
            width: `${el.w * 100}%`, height: `${el.h * 100}%`,
            justifyContent: el.align === 'center' ? 'center' : el.align === 'right' ? 'flex-end' : 'flex-start',
            alignItems: el.verticalAlign === 'top' ? 'flex-start' : el.verticalAlign === 'bottom' ? 'flex-end' : 'center',
          }}
        >
          <span className="text-[5px] leading-tight text-text-tertiary truncate" style={{ fontWeight: (el.fontSize ?? 0) >= 26 ? 600 : 400 }}>
            {el.placeholder}
          </span>
        </div>
      ))}
    </div>
  )
}

// Texte brut d'un élément texte (doc ProseMirror-like → lignes).
function textElementPlainText(el: TextElement): string {
  const c = el.content as { type?: string; content?: Array<{ content?: Array<{ text?: string }> }> } | null
  if (c?.type === 'doc' && Array.isArray(c.content)) {
    return c.content.map(p => Array.isArray(p.content) ? p.content.map(n => n.text ?? '').join('') : '').join('\n')
  }
  return ''
}

// Construit un doc ProseMirror-like minimal depuis un texte brut (1 paragraphe/ligne).
function textDocFromString(val: string) {
  return {
    type: 'doc',
    content: val.split('\n').map(line => ({
      type: 'paragraph',
      content: line ? [{ type: 'text', text: line }] : [],
    })),
  }
}

// Mesure (en px espace-diapositive) la hauteur d'un texte une fois habillé dans
// une largeur donnée — pour « Redimensionner la forme pour l'adapter au texte ».
let _measureCtx: CanvasRenderingContext2D | null = null
function measureTextHeightSlide(text: string, fontSizeSlide: number, widthSlidePx: number, fontFamily: string): number {
  if (!_measureCtx) { const c = document.createElement('canvas'); _measureCtx = c.getContext('2d') }
  const ctx = _measureCtx
  if (!ctx) return fontSizeSlide * 1.3
  ctx.font = `${fontSizeSlide}px ${fontFamily}`
  let lines = 0
  for (const raw of (text || ' ').split('\n')) {
    const words = raw.split(' ')
    let cur = ''
    let n = 1
    for (const word of words) {
      const test = cur ? `${cur} ${word}` : word
      if (ctx.measureText(test).width > widthSlidePx && cur) { n++; cur = word }
      else cur = test
    }
    lines += n
  }
  return lines * fontSizeSlide * 1.3
}

// ── Outils « trait/connecteur » (façon Google Slides) ───────────────────────────

const LINE_KINDS: { kind: LineKind; Icon: typeof Minus; labelKey: string }[] = [
  { kind: 'straight', Icon: Minus,           labelKey: 'pres_line_straight' },
  { kind: 'arrow',    Icon: MoveUpRight,     labelKey: 'pres_line_arrow' },
  { kind: 'elbow',    Icon: CornerDownRight, labelKey: 'pres_line_elbow' },
  { kind: 'curved',   Icon: Spline,          labelKey: 'pres_line_curved' },
  { kind: 'arc',      Icon: Cable,           labelKey: 'pres_line_arc' },
  { kind: 'polyline', Icon: Waypoints,       labelKey: 'pres_line_polyline' },
  { kind: 'freehand', Icon: PenTool,         labelKey: 'pres_line_freehand' },
]

// Points absolus (en pixels canvas) du tracé d'une ligne, selon sa variante.
function linePathPoints(el: LineElement, width: number, height: number): { x: number; y: number }[] {
  const kind = el.lineType ?? 'straight'
  const P = (nx: number, ny: number) => ({ x: nx * width, y: ny * height })
  if ((kind === 'polyline' || kind === 'freehand') && el.points && el.points.length >= 2) {
    return el.points.map(p => P(p.x, p.y))
  }
  const p1 = P(el.x, el.y)
  const p2 = P(el.x2, el.y2)
  if (kind === 'elbow') {
    const midX = (p1.x + p2.x) / 2
    return [p1, { x: midX, y: p1.y }, { x: midX, y: p2.y }, p2]
  }
  return [p1, p2]
}

// Boîte englobante normalisée (0..1) d'une ligne — pour sélection & hit-test.
function lineBBox(el: LineElement): { x: number; y: number; w: number; h: number } {
  const pts = (el.points && el.points.length >= 2)
    ? el.points
    : [{ x: el.x, y: el.y }, { x: el.x2, y: el.y2 }]
  const xs = pts.map(p => p.x), ys = pts.map(p => p.y)
  const minX = Math.min(...xs), minY = Math.min(...ys)
  return { x: minX, y: minY, w: Math.max(...xs) - minX, h: Math.max(...ys) - minY }
}

// Distance d'un point à un segment (en pixels).
function distToSegment(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax, dy = by - ay
  const len2 = dx * dx + dy * dy
  let t = len2 ? ((px - ax) * dx + (py - ay) * dy) / len2 : 0
  t = Math.max(0, Math.min(1, t))
  const cx = ax + t * dx, cy = ay + t * dy
  return Math.hypot(px - cx, py - cy)
}

// ── SlideRenderer class ───────────────────────────────────────────────────────

class SlideRenderer {
  private ctx: CanvasRenderingContext2D
  private width: number
  private height: number
  private dpr: number
  // Facteur d'échelle entre ce canevas et la diapositive canonique (960×540).
  // Les tailles ABSOLUES (police, épaisseur de trait, flèches) sont définies dans
  // l'espace de la diapositive → on les multiplie par `sf` (1 sur le canevas
  // principal, < 1 sur les miniatures) pour un rendu fidèle quelle que soit la taille.
  private sf: number

  constructor(canvas: HTMLCanvasElement, width: number, height: number) {
    this.dpr = window.devicePixelRatio || 1
    this.width = width
    this.height = height
    this.sf = width / SLIDE_W
    canvas.width = width * this.dpr
    canvas.height = height * this.dpr
    canvas.style.width = `${width}px`
    canvas.style.height = `${height}px`
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('Could not get 2d context')
    this.ctx = ctx
    this.ctx.scale(this.dpr, this.dpr)
  }

  render(
    slide: Partial<Slide> & { background?: SlideBackground; elements?: SlideElement[] },
    theme: Presentation['theme'],
    options: { selection?: string[]; mode?: 'edit' | 'thumbnail' } = {},
  ) {
    const { ctx, width, height } = this
    ctx.clearRect(0, 0, width, height)
    this.renderBackground(slide.background, theme)

    const elements = slide.elements ?? []
    const sorted = [...elements].sort((a, b) => (a.zIndex ?? 0) - (b.zIndex ?? 0))
    for (const el of sorted) {
      if (el.hidden) continue
      this.renderElement(el, theme, options.mode !== 'thumbnail')
    }

    if (options.mode === 'edit' && options.selection?.length) {
      for (const id of options.selection) {
        const el = elements.find(e => e.id === id)
        if (el) this.renderSelectionHandles(el)
      }
    }
  }

  // Rendu DIAPORAMA : masque les éléments animés non encore révélés, et applique
  // la transformation d'entrée (opacité/translation/zoom) à l'élément en cours
  // d'animation (progress 0→1).
  renderPresent(
    slide: Partial<Slide> & { background?: SlideBackground; elements?: SlideElement[] },
    theme: Presentation['theme'],
    opts: { hidden?: Set<string>; animating?: { id: string; t: number } } = {},
  ) {
    const { ctx, width, height } = this
    ctx.clearRect(0, 0, width, height)
    this.renderBackground(slide.background, theme)
    const sorted = [...(slide.elements ?? [])].sort((a, b) => (a.zIndex ?? 0) - (b.zIndex ?? 0))
    for (const el of sorted) {
      if (el.hidden) continue
      const animating = opts.animating?.id === el.id
      if (opts.hidden?.has(el.id) && !animating) continue   // animé, pas encore révélé
      if (animating) {
        const t = Math.max(0, Math.min(1, opts.animating!.t))
        const type = (el.anim?.type) || 'fade'
        ctx.save()
        ctx.globalAlpha = type === 'fade' || type === 'rise' || type === 'zoom' ? t : 1
        const cx = (el.x + el.w / 2) * width, cy = (el.y + el.h / 2) * height
        if (type === 'flyL') ctx.translate(-(1 - t) * width * 0.6, 0)
        else if (type === 'flyR') ctx.translate((1 - t) * width * 0.6, 0)
        else if (type === 'flyT') ctx.translate(0, -(1 - t) * height * 0.6)
        else if (type === 'flyB') ctx.translate(0, (1 - t) * height * 0.6)
        else if (type === 'rise') ctx.translate(0, (1 - t) * height * 0.08)
        else if (type === 'zoom') { ctx.translate(cx, cy); ctx.scale(0.6 + 0.4 * t, 0.6 + 0.4 * t); ctx.translate(-cx, -cy) }
        this.renderElement(el, theme, true)
        ctx.restore()
      } else {
        this.renderElement(el, theme, true)
      }
    }
  }

  private renderBackground(
    bg: SlideBackground | undefined,
    theme: Presentation['theme'],
  ) {
    const { ctx, width, height } = this
    if (!bg || bg.type === 'color') {
      ctx.fillStyle = bg?.color ?? theme?.bgColor ?? '#ffffff'
      ctx.fillRect(0, 0, width, height)
    } else if (bg.type === 'gradient' && (bg.grad || bg.gradient)) {
      if (bg.grad) {
        ctx.fillStyle = buildCanvasGradient(ctx, bg.grad, 0, 0, width, height)
      } else {
        const g = ctx.createLinearGradient(0, 0, width, height)
        g.addColorStop(0, bg.gradient!.from)
        g.addColorStop(1, bg.gradient!.to)
        ctx.fillStyle = g
      }
      ctx.fillRect(0, 0, width, height)
    } else if (bg.type === 'image' && bg.imagePath) {
      const img = resolveSlideImage(bg.imagePath)
      if (img.complete && img.naturalWidth > 0) {
        ctx.drawImage(img, 0, 0, width, height)
      } else {
        ctx.fillStyle = '#f1f3f4'
        ctx.fillRect(0, 0, width, height)
      }
    }
  }

  private renderElement(el: SlideElement, theme: Presentation['theme'], showPlaceholders: boolean) {
    const { ctx, width, height } = this
    const x = el.x * width
    const y = el.y * height
    const w = el.w * width
    const h = el.h * height

    ctx.save()
    if (el.rotation) {
      ctx.translate(x + w / 2, y + h / 2)
      ctx.rotate((el.rotation * Math.PI) / 180)
      ctx.translate(-(x + w / 2), -(y + h / 2))
    }
    if (el.flipX || el.flipY) {
      ctx.translate(x + w / 2, y + h / 2)
      ctx.scale(el.flipX ? -1 : 1, el.flipY ? -1 : 1)
      ctx.translate(-(x + w / 2), -(y + h / 2))
    }

    switch (el.type) {
      case 'text':
        this.renderText(el as TextElement, x, y, w, h, theme, showPlaceholders)
        break
      case 'shape':
        this.renderShape(el as ShapeElement, x, y, w, h)
        break
      case 'image':
        this.renderImage(el as ImageElement, x, y, w, h)
        break
      case 'line':
        this.renderLine(el as LineElement)
        break
    }

    ctx.restore()
  }

  private renderText(
    el: TextElement,
    x: number, y: number, w: number, h: number,
    theme: Presentation['theme'],
    showPlaceholders: boolean,
  ) {
    const { ctx } = this

    if (el.background) {
      ctx.fillStyle = el.background
      if (el.borderRadius) {
        this.roundRect(x, y, w, h, el.borderRadius)
        ctx.fill()
      } else {
        ctx.fillRect(x, y, w, h)
      }
    }

    // Try to extract text from ProseMirror-like content
    let text = ''
    let fontSize = 24
    let fontWeight = 'normal'
    let fontStyle = 'normal'
    let color = theme?.textColor ?? '#202124'

    if (el.content && typeof el.content === 'object') {
      const c = el.content as Record<string, unknown>
      if (c.type === 'doc' && Array.isArray(c.content)) {
        text = (c.content as Array<Record<string, unknown>>)
          .flatMap(p =>
            Array.isArray(p.content)
              ? (p.content as Array<Record<string, unknown>>).map(n => String(n.text ?? ''))
              : [],
          )
          .join('\n')
        // Look for marks
        const firstBlock = (c.content as Array<Record<string, unknown>>)[0]
        if (firstBlock && Array.isArray(firstBlock.content)) {
          const firstNode = (firstBlock.content as Array<Record<string, unknown>>)[0]
          if (firstNode && Array.isArray(firstNode.marks)) {
            for (const mark of firstNode.marks as Array<Record<string, unknown>>) {
              if (mark.type === 'textStyle' && mark.attrs) {
                const attrs = mark.attrs as Record<string, unknown>
                if (attrs.fontSize) fontSize = parseInt(String(attrs.fontSize), 10)
                if (attrs.color) color = String(attrs.color)
              }
              if (mark.type === 'bold') fontWeight = 'bold'
              if (mark.type === 'italic') fontStyle = 'italic'
            }
          }
        }
      }
    }

    // Surcharges de style (placeholders + barre de mise en forme).
    if (el.fontSize) fontSize = el.fontSize
    if (el.color) color = el.color
    if (el.bold) fontWeight = 'bold'
    if (el.italic) fontStyle = 'italic'
    // « Réduire le texte pour l'adapter à la forme » : police réduite jusqu'à tenir.
    if (el.autofit === 'shrink' && text) {
      const fam = theme?.fontFamily ?? 'Arial, sans-serif'
      const padS = (el.padding ?? 8) * (el.w * SLIDE_W / 100)
      const wS = el.w * SLIDE_W - 2 * padS
      const hS = el.h * SLIDE_H - 2 * padS
      while (fontSize > 8 && measureTextHeightSlide(text, fontSize, wS, fam) > hS) fontSize -= 1
    }
    // La police est définie dans l'espace diapositive → mise à l'échelle du canevas.
    fontSize *= this.sf

    const pad = (el.padding ?? 8) * (w / 100)
    const fontFamily = el.fontFamily ?? theme?.fontFamily ?? 'Arial, sans-serif'
    const align = el.align ?? 'left'
    const maxW = w - 2 * pad
    ctx.font = `${fontStyle} ${fontWeight} ${fontSize}px ${fontFamily}`
    ctx.textAlign = align === 'center' ? 'center' : align === 'right' ? 'right' : 'left'
    const tx = align === 'center' ? x + w / 2 : align === 'right' ? x + w - pad : x + pad

    const paint = (str: string) => {
      const lines = str.split('\n').flatMap(l => this.wrapLine(ctx, l, maxW))
      const lineH = fontSize * 1.3
      let ty = y + pad + fontSize
      if (el.verticalAlign === 'middle') ty = y + h / 2 - (lines.length * lineH) / 2 + fontSize
      else if (el.verticalAlign === 'bottom') ty = y + h - pad - (lines.length - 1) * lineH
      for (const line of lines) {
        ctx.fillText(line, tx, ty)
        if (el.underline && line) {
          const lw = ctx.measureText(line).width
          const ux = align === 'center' ? tx - lw / 2 : align === 'right' ? tx - lw : tx
          ctx.save()
          ctx.strokeStyle = ctx.fillStyle as string
          ctx.lineWidth = Math.max(1, fontSize / 16)
          ctx.beginPath(); ctx.moveTo(ux, ty + fontSize * 0.12); ctx.lineTo(ux + lw, ty + fontSize * 0.12); ctx.stroke()
          ctx.restore()
        }
        ty += lineH
      }
    }

    if (!text && showPlaceholders && el.placeholder) {
      ctx.fillStyle = el.color ?? '#9aa0a6'
      paint(el.placeholder)
    } else if (text) {
      ctx.fillStyle = color
      paint(text)
    }
    ctx.textAlign = 'left'
  }

  // Découpe une ligne en sous-lignes qui tiennent dans `maxW` (retour à la ligne mots).
  private wrapLine(ctx: CanvasRenderingContext2D, line: string, maxW: number): string[] {
    if (!line) return ['']
    const words = line.split(' ')
    const out: string[] = []
    let cur = ''
    for (const word of words) {
      const test = cur ? `${cur} ${word}` : word
      if (ctx.measureText(test).width > maxW && cur) {
        out.push(cur); cur = word
      } else {
        cur = test
      }
    }
    if (cur) out.push(cur)
    return out.length ? out : ['']
  }

  private renderShape(el: ShapeElement, x: number, y: number, w: number, h: number) {
    const { ctx } = this

    // Fill
    if (el.fill?.type === 'color' && el.fill.color) {
      ctx.fillStyle = el.fill.color
    } else if (el.fill?.type === 'gradient' && (el.fill.grad || el.fill.gradient)) {
      if (el.fill.grad) {
        ctx.fillStyle = buildCanvasGradient(ctx, el.fill.grad, x, y, w, h)
      } else {
        const g = ctx.createLinearGradient(x, y, x + w, y + h)
        g.addColorStop(0, el.fill.gradient!.from)
        g.addColorStop(1, el.fill.gradient!.to)
        ctx.fillStyle = g
      }
    } else {
      ctx.fillStyle = 'transparent'
    }

    // Stroke
    if (el.stroke?.width > 0) {
      ctx.strokeStyle = el.stroke.color ?? '#000'
      ctx.lineWidth = el.stroke.width * this.sf
      if (el.stroke.style === 'dashed') ctx.setLineDash([8, 4])
      else if (el.stroke.style === 'dotted') ctx.setLineDash([2, 4])
      else ctx.setLineDash([])
    }

    switch (el.shape) {
      case 'ellipse':
        ctx.beginPath()
        ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2)
        ctx.fill()
        if (el.stroke?.width > 0) ctx.stroke()
        break
      case 'triangle':
        ctx.beginPath()
        ctx.moveTo(x + w / 2, y)
        ctx.lineTo(x + w, y + h)
        ctx.lineTo(x, y + h)
        ctx.closePath()
        ctx.fill()
        if (el.stroke?.width > 0) ctx.stroke()
        break
      case 'roundRect':
        this.roundRect(x, y, w, h, 12)
        ctx.fill()
        if (el.stroke?.width > 0) ctx.stroke()
        break
      case 'star': case 'pentagon': case 'hexagon': case 'diamond':
      case 'rightArrow': case 'chevron': case 'plus': case 'speech': case 'heart': {
        this.shapePath(el.shape, x, y, w, h)
        ctx.fill()
        if (el.stroke?.width > 0) ctx.stroke()
        break
      }
      default: // rect
        ctx.fillRect(x, y, w, h)
        if (el.stroke?.width > 0) ctx.strokeRect(x, y, w, h)
        break
    }
  }

  // Tracé des formes paramétriques (étoile, polygones, flèche, etc.) dans la boîte.
  private shapePath(shape: string, x: number, y: number, w: number, h: number) {
    const { ctx } = this
    const cx = x + w / 2, cy = y + h / 2
    const poly = (pts: [number, number][]) => { ctx.beginPath(); pts.forEach(([px, py], i) => i ? ctx.lineTo(px, py) : ctx.moveTo(px, py)); ctx.closePath() }
    if (shape === 'star') {
      const pts: [number, number][] = []
      for (let i = 0; i < 10; i++) { const r = i % 2 ? Math.min(w, h) * 0.2 : Math.min(w, h) / 2; const a = (Math.PI / 5) * i - Math.PI / 2; pts.push([cx + (w / Math.min(w, h)) * r * Math.cos(a), cy + (h / Math.min(w, h)) * r * Math.sin(a)]) }
      poly(pts)
    } else if (shape === 'pentagon' || shape === 'hexagon') {
      const n = shape === 'pentagon' ? 5 : 6
      const pts: [number, number][] = []
      for (let i = 0; i < n; i++) { const a = (2 * Math.PI / n) * i - Math.PI / 2; pts.push([cx + (w / 2) * Math.cos(a), cy + (h / 2) * Math.sin(a)]) }
      poly(pts)
    } else if (shape === 'diamond') {
      poly([[cx, y], [x + w, cy], [cx, y + h], [x, cy]])
    } else if (shape === 'rightArrow') {
      const sh = h * 0.34, t = x + w * 0.62
      poly([[x, cy - sh / 2], [t, cy - sh / 2], [t, y], [x + w, cy], [t, y + h], [t, cy + sh / 2], [x, cy + sh / 2]])
    } else if (shape === 'chevron') {
      const sh = w * 0.3
      poly([[x, y], [x + w - sh, y], [x + w, cy], [x + w - sh, y + h], [x, y + h], [x + sh, cy]])
    } else if (shape === 'plus') {
      const tx = w * 0.33, ty = h * 0.33
      poly([[x + tx, y], [x + w - tx, y], [x + w - tx, y + ty], [x + w, y + ty], [x + w, y + h - ty], [x + w - tx, y + h - ty], [x + w - tx, y + h], [x + tx, y + h], [x + tx, y + h - ty], [x, y + h - ty], [x, y + ty], [x + tx, y + ty]])
    } else if (shape === 'speech') {
      const r = Math.min(w, h) * 0.16, bh = h * 0.78
      ctx.beginPath()
      ctx.moveTo(x + r, y); ctx.lineTo(x + w - r, y); ctx.quadraticCurveTo(x + w, y, x + w, y + r)
      ctx.lineTo(x + w, y + bh - r); ctx.quadraticCurveTo(x + w, y + bh, x + w - r, y + bh)
      ctx.lineTo(x + w * 0.34, y + bh); ctx.lineTo(x + w * 0.18, y + h); ctx.lineTo(x + w * 0.24, y + bh)
      ctx.lineTo(x + r, y + bh); ctx.quadraticCurveTo(x, y + bh, x, y + bh - r)
      ctx.lineTo(x, y + r); ctx.quadraticCurveTo(x, y, x + r, y); ctx.closePath()
    } else if (shape === 'heart') {
      ctx.beginPath()
      ctx.moveTo(cx, y + h * 0.28)
      ctx.bezierCurveTo(cx, y + h * 0.08, x + w * 0.08, y, x + w * 0.02, y + h * 0.32)
      ctx.bezierCurveTo(x - w * 0.04, y + h * 0.55, cx, y + h * 0.8, cx, y + h)
      ctx.bezierCurveTo(cx, y + h * 0.8, x + w * 1.04, y + h * 0.55, x + w * 0.98, y + h * 0.32)
      ctx.bezierCurveTo(x + w * 0.92, y, cx, y + h * 0.08, cx, y + h * 0.28)
      ctx.closePath()
    }
  }

  private renderImage(el: ImageElement, x: number, y: number, w: number, h: number) {
    const { ctx } = this
    const img = resolveSlideImage(el.storagePath)
    ctx.globalAlpha = el.opacity ?? 1
    if (img.complete && img.naturalWidth > 0) {
      const c = el.crop
      if (c) {
        const nw = img.naturalWidth, nh = img.naturalHeight
        ctx.drawImage(img, c.x * nw, c.y * nh, c.w * nw, c.h * nh, x, y, w, h)
      } else {
        ctx.drawImage(img, x, y, w, h)
      }
    } else {
      ctx.fillStyle = '#e8eaed'
      ctx.fillRect(x, y, w, h)
      ctx.fillStyle = '#9aa0a6'
      ctx.font = '14px Arial'
      ctx.fillText('Image', x + 8, y + 24)
    }
    ctx.globalAlpha = 1
  }

  private renderLine(el: LineElement) {
    const { ctx, width, height } = this
    const kind = el.lineType ?? 'straight'

    ctx.strokeStyle = el.stroke?.color ?? '#000'
    ctx.lineWidth = (el.stroke?.width ?? 2) * this.sf
    ctx.lineJoin = 'round'
    ctx.lineCap = 'round'
    if (el.stroke?.style === 'dashed') ctx.setLineDash([8, 4])
    else if (el.stroke?.style === 'dotted') ctx.setLineDash([2, 4])
    else ctx.setLineDash([])

    const p1 = { x: el.x * width, y: el.y * height }
    const p2 = { x: el.x2 * width, y: el.y2 * height }

    ctx.beginPath()
    let tipFrom = p1
    let tip = p2

    if (kind === 'curved' || kind === 'arc') {
      // Courbe quadratique avec point de contrôle perpendiculaire au segment.
      const mx = (p1.x + p2.x) / 2, my = (p1.y + p2.y) / 2
      const dx = p2.x - p1.x, dy = p2.y - p1.y
      const len = Math.hypot(dx, dy) || 1
      const nx = -dy / len, ny = dx / len
      const bow = (kind === 'arc' ? 0.5 : 0.28) * len
      const cx = mx + nx * bow, cy = my + ny * bow
      ctx.moveTo(p1.x, p1.y)
      ctx.quadraticCurveTo(cx, cy, p2.x, p2.y)
      tipFrom = { x: cx, y: cy }
      tip = p2
    } else {
      const pts = linePathPoints(el, width, height)
      ctx.moveTo(pts[0].x, pts[0].y)
      for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y)
      tip = pts[pts.length - 1]
      tipFrom = pts[pts.length - 2] ?? p1
    }
    ctx.stroke()

    if (kind === 'arrow' || el.arrowEnd) {
      ctx.setLineDash([])
      this.drawArrowHead(ctx, tipFrom.x, tipFrom.y, tip.x, tip.y)
    }
  }

  private drawArrowHead(
    ctx: CanvasRenderingContext2D,
    x1: number, y1: number, x2: number, y2: number,
  ) {
    const angle = Math.atan2(y2 - y1, x2 - x1)
    const size = 12 * this.sf
    ctx.beginPath()
    ctx.moveTo(x2, y2)
    ctx.lineTo(x2 - size * Math.cos(angle - Math.PI / 6), y2 - size * Math.sin(angle - Math.PI / 6))
    ctx.lineTo(x2 - size * Math.cos(angle + Math.PI / 6), y2 - size * Math.sin(angle + Math.PI / 6))
    ctx.closePath()
    ctx.fillStyle = ctx.strokeStyle
    ctx.fill()
  }

  private renderSelectionHandles(el: SlideElement) {
    const { ctx, width, height } = this
    const bb = el.type === 'line' ? lineBBox(el as LineElement) : el
    const x = bb.x * width
    const y = bb.y * height
    const w = bb.w * width
    const h = bb.h * height
    const pad = 2

    ctx.strokeStyle = '#1a73e8'
    ctx.lineWidth = 1.5
    ctx.setLineDash([])
    ctx.strokeRect(x - pad, y - pad, w + pad * 2, h + pad * 2)

    const handles = [
      [x - pad, y - pad], [x + w / 2, y - pad], [x + w + pad, y - pad],
      [x - pad, y + h / 2], [x + w + pad, y + h / 2],
      [x - pad, y + h + pad], [x + w / 2, y + h + pad], [x + w + pad, y + h + pad],
    ]
    ctx.fillStyle = '#fff'
    for (const [hx, hy] of handles) {
      ctx.beginPath()
      ctx.arc(hx, hy, 4, 0, Math.PI * 2)
      ctx.fill()
      ctx.stroke()
    }
  }

  hitTest(x: number, y: number, elements: SlideElement[], width: number, height: number): SlideElement | null {
    const sorted = [...elements]
      .filter(e => !e.hidden && !e.locked)
      .sort((a, b) => (b.zIndex ?? 0) - (a.zIndex ?? 0))

    for (const el of sorted) {
      if (el.type === 'line') {
        const pts = linePathPoints(el as LineElement, width, height)
        const tol = Math.max(6, (el as LineElement).stroke?.width ?? 2 + 4)
        for (let i = 1; i < pts.length; i++) {
          if (distToSegment(x, y, pts[i - 1].x, pts[i - 1].y, pts[i].x, pts[i].y) <= tol) {
            return el
          }
        }
        continue
      }
      const ex = el.x * width
      const ey = el.y * height
      const ew = el.w * width
      const eh = el.h * height
      if (x >= ex && x <= ex + ew && y >= ey && y <= ey + eh) {
        return el
      }
    }
    return null
  }

  private roundRect(x: number, y: number, w: number, h: number, r: number) {
    const { ctx } = this
    ctx.beginPath()
    ctx.moveTo(x + r, y)
    ctx.lineTo(x + w - r, y)
    ctx.quadraticCurveTo(x + w, y, x + w, y + r)
    ctx.lineTo(x + w, y + h - r)
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h)
    ctx.lineTo(x + r, y + h)
    ctx.quadraticCurveTo(x, y + h, x, y + h - r)
    ctx.lineTo(x, y + r)
    ctx.quadraticCurveTo(x, y, x + r, y)
    ctx.closePath()
  }
}

// ── SlideThumbnail ────────────────────────────────────────────────────────────

function SlideThumbnail({
  slide,
  theme,
  active,
  index,
  onClick,
}: {
  slide: Partial<Slide>
  theme: Presentation['theme']
  active: boolean
  index: number
  onClick: (e: ReactMouseEvent) => void
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const wrapRef = useRef<HTMLDivElement>(null)
  const rendererRef = useRef<SlideRenderer | null>(null)
  const sizeRef = useRef(0)

  // La miniature suit la largeur du panneau (ratio 16:9) et reste nette : on
  // recrée le renderer à la taille pixel réelle quand la largeur change.
  useEffect(() => {
    const wrap = wrapRef.current
    const canvas = canvasRef.current
    if (!wrap || !canvas) return
    const draw = () => {
      const w = Math.max(1, Math.round(wrap.clientWidth))
      const h = Math.round((w * SLIDE_H) / SLIDE_W)
      if (w !== sizeRef.current || !rendererRef.current) {
        sizeRef.current = w
        try { rendererRef.current = new SlideRenderer(canvas, w, h) } catch { return }
      }
      rendererRef.current?.render(slide, theme, { mode: 'thumbnail' })
    }
    draw()
    const ro = new ResizeObserver(draw)
    ro.observe(wrap)
    const unsub = onSlideImageLoaded(draw) // redessine quand un asset image charge
    return () => { ro.disconnect(); unsub() }
  }, [slide, theme])

  return (
    <button
      onClick={onClick}
      className={`group w-full flex flex-col items-center gap-1 p-1 rounded transition-colors
                  ${active ? 'bg-primary-light' : 'hover:bg-surface-2'}`}
    >
      <div
        ref={wrapRef}
        className={`relative w-full aspect-[16/9] rounded overflow-hidden border-2 transition-colors
                    ${active ? 'border-primary' : 'border-border group-hover:border-border-strong'}`}
      >
        <canvas ref={canvasRef} className="block" />
      </div>
      <span className="text-[10px] text-text-tertiary">{index + 1}</span>
    </button>
  )
}

// ── Menu contextuel de la liste des diapositives ────────────────────────────────

// Modèle d'item de menu contextuel interne au module (sections → grilles). Il est
// converti en `MenuItem[]` du composant partagé `@ui` `MenuDropdown` via
// `ctxToMenuItems`. `customSubmenu` permet d'embarquer un sous-menu à contenu
// arbitraire (ex. la grille de mises en page) — c'est une fonction qui reçoit la
// fonction de fermeture du menu pour pouvoir le fermer après un choix.
type CtxItem = {
  icon?: ReactNode
  label: string
  shortcut?: string
  onClick?: () => void
  disabled?: boolean
  danger?: boolean
  submenu?: CtxItem[]
  /** Contenu de sous-menu arbitraire (ex. grille de mises en page). */
  customSubmenu?: (close: () => void) => ReactNode
}

// Convertit nos sections d'items maison en `MenuItem[]` pour `@ui` `MenuDropdown` :
// insère un séparateur entre les sections, mappe chaque entrée vers une action / un
// sous-menu (texte ou contenu personnalisé via `custom`). Conserve ordre, icônes,
// états disabled, raccourcis et le drapeau `danger`.
function ctxItemToMenuItem(it: CtxItem): MenuItem {
  if (it.customSubmenu) {
    const renderCustom = it.customSubmenu
    return {
      type: 'submenu', label: it.label, icon: it.icon, disabled: it.disabled,
      items: [{ type: 'custom', render: close => renderCustom(close) }],
    }
  }
  if (it.submenu) {
    return {
      type: 'submenu', label: it.label, icon: it.icon, disabled: it.disabled,
      items: it.submenu.map(ctxItemToMenuItem),
    }
  }
  return {
    type: 'action', label: it.label, icon: it.icon, shortcut: it.shortcut,
    disabled: it.disabled, danger: it.danger,
    onClick: () => { it.onClick?.() },
  }
}

function ctxToMenuItems(sections: CtxItem[][]): MenuItem[] {
  const out: MenuItem[] = []
  for (const items of sections.filter(s => s.length)) {
    if (out.length) out.push({ type: 'separator' })
    for (const it of items) out.push(ctxItemToMenuItem(it))
  }
  return out
}

// ── SlidePanel ────────────────────────────────────────────────────────────────

function SlidePanel({
  slides,
  fullSlides,
  selection,
  theme,
  canPaste,
  slidePresence,
  onSelectSlide,
  onClearSelection,
  onAddSlide,
  onNewSlideAfter,
  onReorderSlide,
  onDeleteSelected,
  onDuplicateSelected,
  onCopySelected,
  onCutSelected,
  onPasteAfter,
  onToggleHiddenSelected,
  onCreateImageSelected,
  onEditBackground,
}: {
  slides: SlideSummary[]
  fullSlides: Record<string, Slide>
  selection: string[]
  theme: Presentation['theme']
  canPaste: boolean
  slidePresence: Record<string, PresenceUser[]>
  onSelectSlide: (id: string, mods: { ctrl: boolean; shift: boolean }) => void
  onClearSelection: () => void
  onAddSlide: () => void
  onNewSlideAfter: (id: string | null) => void
  onReorderSlide: (dragIdx: number, dropIdx: number) => void
  onDeleteSelected: () => void
  onDuplicateSelected: () => void
  onCopySelected: () => void
  onCutSelected: () => void
  onPasteAfter: (afterId: string | null) => void
  onToggleHiddenSelected: () => void
  onCreateImageSelected: () => void
  onEditBackground: (id: string) => void
}) {
  const { t } = useTranslation('office')
  const [dragging, setDragging] = useState<number | null>(null)
  const [dragOver, setDragOver] = useState<number | null>(null)
  // Menu contextuel positionné au curseur. sid = null → clic dans le vide.
  const [menu, setMenu] = useState<{ x: number; y: number; sid: string | null } | null>(null)

  const buildSections = (sid: string | null): CtxItem[][] => {
    const target = sid ? slides.find(s => s.id === sid) : null
    const hasTarget = !!target
    return [
      [
        { icon: <Scissors size={15} />, label: t('pres_ctx_cut'), shortcut: 'Ctrl+X', disabled: !hasTarget, onClick: onCutSelected },
        { icon: <Copy size={15} />, label: t('pres_ctx_copy'), shortcut: 'Ctrl+C', disabled: !hasTarget, onClick: onCopySelected },
        { icon: <ClipboardPaste size={15} />, label: t('pres_ctx_paste'), shortcut: 'Ctrl+V', disabled: !canPaste, onClick: () => onPasteAfter(sid) },
        { icon: <ClipboardX size={15} />, label: t('pres_ctx_paste_plain'), shortcut: 'Ctrl+Maj+V', disabled: true },
        { icon: <Trash2 size={15} />, label: t('common_delete'), danger: true, disabled: !hasTarget, onClick: onDeleteSelected },
      ],
      [
        { icon: <Plus size={15} />, label: t('pres_new_slide'), shortcut: 'Ctrl+M', onClick: () => onNewSlideAfter(sid) },
        {
          icon: <FilePlus2 size={15} />, label: t('pres_ctx_create'), submenu: [
            { icon: <FilePlus2 size={15} />, label: t('pres_ctx_create_editable'), onClick: () => onNewSlideAfter(sid) },
            { icon: <ImageIcon size={15} />, label: `${t('pres_ctx_create_image')} 🍌`, disabled: !hasTarget, onClick: onCreateImageSelected },
          ],
        },
        { icon: <CopyPlus size={15} />, label: t('pres_ctx_dup_slide'), disabled: !hasTarget, onClick: onDuplicateSelected },
        {
          icon: target?.is_hidden ? <Eye size={15} /> : <EyeOff size={15} />,
          label: target?.is_hidden ? t('pres_show') : t('pres_hide'),
          disabled: !hasTarget, onClick: onToggleHiddenSelected,
        },
      ],
      [
        { icon: <Droplet size={15} />, label: t('pres_ctx_background'), disabled: !hasTarget, onClick: () => sid && onEditBackground(sid) },
        { icon: <LayoutTemplate size={15} />, label: t('pres_ctx_layout'), disabled: true, submenu: [] },
        { icon: <Paintbrush size={15} />, label: t('pres_ctx_theme'), disabled: true },
      ],
      [
        { icon: <Replace size={15} />, label: t('pres_ctx_transition'), disabled: true },
      ],
      [
        { icon: <MessageSquarePlus size={15} />, label: t('pres_ctx_comment'), shortcut: 'Ctrl+Alt+M', disabled: true },
      ],
    ]
  }

  return (
    <div className="flex flex-col h-full">
      <div
        className="flex-1 overflow-y-auto p-2 space-y-1"
        onClick={() => onClearSelection()}
        onContextMenu={e => { e.preventDefault(); onClearSelection(); setMenu({ x: e.clientX, y: e.clientY, sid: null }) }}
      >
        {slides.map((slide, idx) => (
          <div
            key={slide.id}
            draggable
            onDragStart={() => setDragging(idx)}
            onDragOver={e => { e.preventDefault(); setDragOver(idx) }}
            onDrop={() => {
              if (dragging !== null && dragging !== idx) {
                onReorderSlide(dragging, idx)
              }
              setDragging(null)
              setDragOver(null)
            }}
            onDragEnd={() => { setDragging(null); setDragOver(null) }}
            className={`relative transition-all ${dragOver === idx ? 'border-t-2 border-primary' : ''}`}
            onContextMenu={e => {
              e.preventDefault(); e.stopPropagation()
              if (!selection.includes(slide.id)) onSelectSlide(slide.id, { ctrl: false, shift: false })
              setMenu({ x: e.clientX, y: e.clientY, sid: slide.id })
            }}
          >
            <SlideThumbnail
              slide={fullSlides[slide.id] ?? slide}
              theme={theme}
              active={selection.includes(slide.id)}
              index={idx}
              onClick={e => {
                e.stopPropagation()
                onSelectSlide(slide.id, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey })
              }}
            />
            {!!slidePresence[slide.id]?.length && (
              <div className="absolute top-1 right-1 flex -space-x-1.5 pointer-events-none z-10">
                {slidePresence[slide.id].slice(0, 3).map((u, i) => (
                  <div key={i} title={u.name}
                    className="w-5 h-5 rounded-full flex items-center justify-center text-[8px] font-bold text-white ring-2 ring-white overflow-hidden"
                    style={{ backgroundColor: u.color, zIndex: 5 - i }}>
                    {u.avatar ? <img src={u.avatar} alt="" className="w-full h-full object-cover" /> : initials(u.name)}
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
      <div className="p-2 border-t border-border flex-shrink-0">
        <Button
          variant="ghost"
          size="sm"
          icon={<Plus size={14} />}
          onClick={onAddSlide}
          className="w-full justify-start"
        >
          {t('pres_new_slide')}
        </Button>
      </div>

      {menu && (
        <MenuDropdown
          pos={{ top: menu.y, left: menu.x, minWidth: 260 }}
          items={ctxToMenuItems(buildSections(menu.sid))}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  )
}

// ── EmptySlideArea — état « aucune diapositive » / « aucune sélection » ──────────

function EmptySlideArea({ hasSlides, onAdd }: { hasSlides: boolean; onAdd: () => void }) {
  const { t } = useTranslation('office')
  return (
    <div className="flex-1 flex items-center justify-center bg-surface-2 overflow-hidden p-8">
      <button
        type="button"
        onClick={hasSlides ? undefined : onAdd}
        disabled={hasSlides}
        className={`w-full max-w-[960px] aspect-[16/9] rounded-xl border-2 border-dashed
                    flex items-center justify-center text-base italic select-none
                    ${hasSlides
                      ? 'border-border text-text-tertiary cursor-default'
                      : 'border-border-strong text-text-tertiary cursor-pointer hover:border-primary hover:text-primary transition-colors'}`}
      >
        {hasSlides ? t('pres_no_selection') : t('pres_add_slide')}
      </button>
    </div>
  )
}

// ── SlideCanvas ───────────────────────────────────────────────────────────────

// API impérative exposée par SlideCanvas au parent (ruban Disposition / Animations).
interface CanvasApi {
  align: (mode: 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom') => void
  distribute: (axis: 'h' | 'v') => void
  zorder: (op: 'front' | 'back' | 'forward' | 'backward') => void
  duplicate: () => void
  remove: () => void
  setAnim: (anim: { type: string; duration?: number } | null) => void
  selCount: () => number
  curAnim: () => string
}

function SlideCanvas({
  slide,
  theme,
  tool,
  lineKind,
  shapeKind,
  onElementsChange,
  onToolChange,
  onEditBackground,
  onInsertImage,
  onSelectionChange,
  cropSignal,
  remoteSelections,
  awareness,
  onApi,
}: {
  slide: Slide | null
  theme: Presentation['theme']
  tool: string
  lineKind: LineKind
  shapeKind: string
  onElementsChange: (elements: SlideElement[]) => void
  onToolChange: (t: string) => void
  onEditBackground: () => void
  onInsertImage: (file: File, cx?: number, cy?: number) => void
  onSelectionChange: (el: SlideElement | null) => void
  cropSignal: number
  remoteSelections: Record<string, PresenceUser[]>
  awareness: Awareness | null
  onApi?: (api: CanvasApi) => void
}) {
  const publishCursor = usePublishCursor(awareness)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const rendererRef = useRef<SlideRenderer | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const wrapperRef = useRef<HTMLDivElement>(null)
  const [selection, setSelection] = useState<string[]>([])
  const [scale, setScale] = useState(1)
  // Bumpé quand une image d'asset finit de charger → redessine le canevas.
  const [imgGen, setImgGen] = useState(0)
  useEffect(() => onSlideImageLoaded(() => setImgGen(n => n + 1)), [])
  const [fitMenuOpen, setFitMenuOpen] = useState(false)
  // Menu contextuel : sur un élément (elementId) ou sur le fond (null).
  const [canvasMenu, setCanvasMenu] = useState<{ x: number; y: number; elementId: string | null } | null>(null)
  // Presse-papiers d'éléments (copier/couper/coller dans la diapositive).
  const elementClipRef = useRef<SlideElement[] | null>(null)
  // Guides (repères) déplaçables, locaux à l'éditeur.
  const [guides, setGuides] = useState<{ id: string; axis: 'v' | 'h'; pos: number }[]>([])
  const [showGuides, setShowGuides] = useState(true)
  const guideDragRef = useRef<{ id: string } | null>(null)
  // Glisser-déposer d'images + remplacement d'image.
  const [dragOver, setDragOver] = useState(false)
  const replaceInputRef = useRef<HTMLInputElement>(null)
  const replaceTargetRef = useRef<string | null>(null)
  // Rognage d'image : cadre (px wrapper) + étendue complète figée + drag.
  const [cropId, setCropId] = useState<string | null>(null)
  const [cropFrame, setCropFrame] = useState<{ l: number; t: number; w: number; h: number } | null>(null)
  const cropFullRef = useRef<{ l: number; t: number; w: number; h: number } | null>(null)
  const cropDragRef = useRef<{ handle: string; sx: number; sy: number; f: { l: number; t: number; w: number; h: number } } | null>(null)
  const resizeRef = useRef<{ id: string; handle: string; sx: number; sy: number; ox: number; oy: number; ow: number; oh: number } | null>(null)
  const rotateRef = useRef<{ id: string } | null>(null)
  const { t } = useTranslation('office')
  // Édition de texte en cours (double-clic sur un élément texte) → textarea overlay.
  // editText = source de vérité synchrone du textarea (évite les pertes de frappe
  // dues à l'aller-retour d'état asynchrone).
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editText, setEditText] = useState('')
  useEffect(() => { setEditingId(null) }, [slide?.id])
  // Ferme le menu d'ajustement quand la sélection/édition change.
  useEffect(() => { setFitMenuOpen(false) }, [selection.join(','), editingId])
  // Remonte l'élément sélectionné (1 seul) au parent → barre d'outils contextuelle.
  useEffect(() => {
    const el = selection.length === 1 ? (slide?.elements?.find(e => e.id === selection[0]) ?? null) : null
    onSelectionChange(el)
  }, [selection, slide, onSelectionChange])
  // ── Rognage (crop) d'image ────────────────────────────────────────────────────
  const enterCrop = useCallback((id: string) => {
    const el = elementsRef.current.find(e => e.id === id) as ImageElement | undefined
    if (!el || el.type !== 'image') return
    const er = { l: el.x * SLIDE_W * scale, t: el.y * SLIDE_H * scale, w: el.w * SLIDE_W * scale, h: el.h * SLIDE_H * scale }
    const c = el.crop ?? { x: 0, y: 0, w: 1, h: 1 }
    cropFullRef.current = { l: er.l - (c.x / c.w) * er.w, t: er.t - (c.y / c.h) * er.h, w: er.w / c.w, h: er.h / c.h }
    setSelection([id]); setEditingId(null); setCropFrame(er); setCropId(id)
  }, [scale])

  const confirmCrop = useCallback(() => {
    const id = cropId, frame = cropFrame, full = cropFullRef.current
    setCropId(null); setCropFrame(null); cropFullRef.current = null
    if (!id || !frame || !full) return
    const crop = { x: (frame.l - full.l) / full.w, y: (frame.t - full.t) / full.h, w: frame.w / full.w, h: frame.h / full.h }
    const box = { x: frame.l / (SLIDE_W * scale), y: frame.t / (SLIDE_H * scale), w: frame.w / (SLIDE_W * scale), h: frame.h / (SLIDE_H * scale) }
    onElementsChange(elementsRef.current.map(e => e.id === id ? ({ ...e, ...box, crop } as SlideElement) : e))
  }, [cropId, cropFrame, scale, onElementsChange])

  const cancelCrop = useCallback(() => { setCropId(null); setCropFrame(null); cropFullRef.current = null }, [])

  // Pendant le rognage, valider (Entrée) / annuler (Échap) au clavier même si le
  // focus n'est pas sur le conteneur : le clic sur « Rogner » garde le focus sur
  // le bouton, donc l'écouteur du conteneur ne reçoit pas l'événement.
  useEffect(() => {
    if (!cropId) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Enter') { e.preventDefault(); confirmCrop() }
      else if (e.key === 'Escape') { e.preventDefault(); cancelCrop() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [cropId, confirmCrop, cancelCrop])

  const onCropMove = useCallback((ev: globalThis.MouseEvent) => {
    const r = cropDragRef.current, full = cropFullRef.current
    if (!r || !full) return
    const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v))
    const dx = ev.clientX - r.sx, dy = ev.clientY - r.sy
    let { l, t, w, h } = r.f
    const min = 24
    if (r.handle === 'move') {
      l = clamp(r.f.l + dx, full.l, full.l + full.w - w)
      t = clamp(r.f.t + dy, full.t, full.t + full.h - h)
    } else {
      if (r.handle.includes('e')) w = clamp(r.f.w + dx, min, full.l + full.w - l)
      if (r.handle.includes('s')) h = clamp(r.f.h + dy, min, full.t + full.h - t)
      if (r.handle.includes('w')) { const nl = clamp(r.f.l + dx, full.l, r.f.l + r.f.w - min); w = r.f.w + (r.f.l - nl); l = nl }
      if (r.handle.includes('n')) { const nt = clamp(r.f.t + dy, full.t, r.f.t + r.f.h - min); h = r.f.h + (r.f.t - nt); t = nt }
    }
    setCropFrame({ l, t, w, h })
  }, [])
  const endCropDrag = useCallback(() => {
    cropDragRef.current = null
    document.removeEventListener('mousemove', onCropMove)
    document.removeEventListener('mouseup', endCropDrag)
  }, [onCropMove])
  const startCropDrag = useCallback((e: React.MouseEvent, handle: string) => {
    e.preventDefault(); e.stopPropagation()
    if (!cropFrame) return
    cropDragRef.current = { handle, sx: e.clientX, sy: e.clientY, f: cropFrame }
    document.addEventListener('mousemove', onCropMove)
    document.addEventListener('mouseup', endCropDrag)
  }, [cropFrame, onCropMove, endCropDrag])

  // « Rogner » depuis la barre du haut : déclenche le crop de l'image sélectionnée.
  const cropSignalRef = useRef(cropSignal)
  useEffect(() => {
    if (cropSignal !== cropSignalRef.current) {
      cropSignalRef.current = cropSignal
      const el = selection.length === 1 ? slide?.elements?.find(e => e.id === selection[0]) : null
      if (el?.type === 'image') enterCrop(el.id)
    }
  }, [cropSignal, selection, slide, enterCrop])
  const dragRef = useRef<{
    el: SlideElement
    startX: number
    startY: number
    origX: number
    origY: number
  } | null>(null)
  // Tracé de ligne en cours. On mémorise `base` (les autres éléments au début du
  // tracé) + `el` (l'élément en cours) DANS le ref, pour ne jamais dépendre d'un
  // `slide` périmé pendant le geste (sinon le mousemove écraserait l'élément créé).
  const drawRef = useRef<{ mode: 'segment' | 'freehand'; base: SlideElement[]; el: LineElement } | null>(null)
  // Polyligne en cours (clics successifs ; le dernier point suit le curseur).
  const polyRef = useRef<{ base: SlideElement[]; el: LineElement } | null>(null)
  // Miroir TOUJOURS à jour des éléments de la slide (évite les fermetures périmées
  // dans les handlers d'événements : capture fiable de l'état au moment du clic).
  const elementsRef = useRef<SlideElement[]>([])
  elementsRef.current = slide?.elements ?? []

  // Échelle pour remplir le conteneur (la diapositive grandit au-delà de 960px sur
  // grand écran → marges réduites). Petite marge pour ne pas coller aux bords.
  useEffect(() => {
    const container = containerRef.current
    if (!container) return
    const update = () => {
      const cw = container.clientWidth - 24
      const ch = container.clientHeight - 24
      setScale(Math.max(0.1, Math.min(cw / SLIDE_W, ch / SLIDE_H)))
    }
    update()
    const ro = new ResizeObserver(update)
    ro.observe(container)
    return () => ro.disconnect()
  }, [])

  // (Re)crée le renderer à la taille pixel AFFICHÉE (net même agrandi) puis rend.
  const sizeRef = useRef(0)
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !slide) return
    const w = Math.max(1, Math.round(SLIDE_W * scale))
    if (w !== sizeRef.current || !rendererRef.current) {
      sizeRef.current = w
      try { rendererRef.current = new SlideRenderer(canvas, w, Math.round(SLIDE_H * scale)) } catch { return }
    }
    // La sélection (poignées) est dessinée en DOM (overlay), pas sur le canevas.
    // Pendant le rognage, on masque l'image rognée sur le canevas (l'overlay de crop
    // la représente entièrement) — sinon la partie hors-cadre resterait visible.
    const rs = cropId ? { ...slide, elements: (slide.elements ?? []).filter(e => e.id !== cropId) } : slide
    rendererRef.current.render(rs, theme, { selection: [], mode: 'edit' })
  }, [slide, theme, selection, scale, cropId, imgGen])

  const getCanvasPos = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current
    if (!canvas) return { x: 0, y: 0 }
    const rect = canvas.getBoundingClientRect()
    return {
      x: ((e.clientX - rect.left) / scale) / SLIDE_W,
      y: ((e.clientY - rect.top) / scale) / SLIDE_H,
    }
  }, [scale])

  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (cropId) { confirmCrop(); return }
    if (!slide || !rendererRef.current) return
    const pos = getCanvasPos(e)
    const px = pos.x * SLIDE_W
    const py = pos.y * SLIDE_H

    if (tool === 'select') {
      const hit = rendererRef.current.hitTest(px, py, elementsRef.current, SLIDE_W, SLIDE_H)
      if (hit) {
        setSelection([hit.id])
        dragRef.current = {
          el: hit,
          startX: px,
          startY: py,
          origX: hit.x,
          origY: hit.y,
        }
      } else {
        setSelection([])
      }
    } else if (tool === 'text') {
      const newEl: TextElement = {
        id: uid(),
        type: 'text',
        x: pos.x,
        y: pos.y,
        w: 0.3,
        h: 0.15,
        rotation: 0,
        zIndex: (elementsRef.current.length + 1),
        locked: false,
        hidden: false,
        content: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Texte' }] }] },
        padding: 8,
        verticalAlign: 'top',
        background: null,
        borderRadius: 0,
        placeholder: null,
      }
      const updated = [...elementsRef.current, newEl]
      onElementsChange(updated)
      setSelection([newEl.id])
    } else if (tool === 'shape') {
      const newEl: ShapeElement = {
        id: uid(),
        type: 'shape',
        x: pos.x,
        y: pos.y,
        w: 0.2,
        h: 0.15,
        rotation: 0,
        zIndex: elementsRef.current.length + 1,
        locked: false,
        hidden: false,
        shape: shapeKind || 'rect',
        fill: { type: 'color', color: '#1a73e8' },
        stroke: { color: '#1557b0', width: 0, style: 'solid' },
        content: null,
      }
      const updated = [...elementsRef.current, newEl]
      onElementsChange(updated)
      setSelection([newEl.id])
    } else if (tool === 'line') {
      const els = elementsRef.current
      const mk = (extra: Partial<LineElement>): LineElement => ({
        id: uid(), type: 'line', x: pos.x, y: pos.y, w: 0, h: 0,
        x2: pos.x, y2: pos.y, rotation: 0, locked: false, hidden: false,
        stroke: { color: '#202124', width: 2, style: 'solid' },
        arrowEnd: lineKind === 'arrow' ? 'triangle' : null,
        zIndex: els.length + 1, lineType: lineKind, ...extra,
      })
      if (lineKind === 'polyline') {
        if (polyRef.current) {
          // Fixe un nouveau sommet ; le dernier point continue de suivre le curseur.
          const pr = polyRef.current
          pr.el = { ...pr.el, points: [...(pr.el.points ?? []), { x: pos.x, y: pos.y }] }
          onElementsChange([...pr.base, pr.el])
        } else {
          const newEl = mk({ points: [{ x: pos.x, y: pos.y }, { x: pos.x, y: pos.y }] })
          polyRef.current = { base: els, el: newEl }
          onElementsChange([...els, newEl])
          setSelection([newEl.id])
        }
      } else if (lineKind === 'freehand') {
        const newEl = mk({ points: [{ x: pos.x, y: pos.y }] })
        drawRef.current = { mode: 'freehand', base: els, el: newEl }
        onElementsChange([...els, newEl])
        setSelection([newEl.id])
      } else {
        const newEl = mk({})
        drawRef.current = { mode: 'segment', base: els, el: newEl }
        onElementsChange([...els, newEl])
        setSelection([newEl.id])
      }
    }
  }, [slide, tool, lineKind, shapeKind, getCanvasPos, onElementsChange, cropId, confirmCrop])

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!slide) return
    const pos = getCanvasPos(e)
    publishCursor({ x: pos.x, y: pos.y }) // présence : curseur souris (coords fraction)

    // Tracé d'un segment : l'extrémité suit le curseur.
    if (drawRef.current?.mode === 'segment') {
      const d = drawRef.current
      d.el = { ...d.el, x2: pos.x, y2: pos.y }
      onElementsChange([...d.base, d.el])
      return
    }
    // Dessin à main levée : on échantillonne par distance.
    if (drawRef.current?.mode === 'freehand') {
      const d = drawRef.current
      const pts = [...(d.el.points ?? [])]
      const last = pts[pts.length - 1]
      if (!last || Math.hypot(pos.x - last.x, pos.y - last.y) > 0.004) pts.push({ x: pos.x, y: pos.y })
      d.el = { ...d.el, points: pts, x2: pos.x, y2: pos.y }
      onElementsChange([...d.base, d.el])
      return
    }
    // Polyligne en cours : le dernier point suit le curseur.
    if (polyRef.current) {
      const pr = polyRef.current
      const pts = [...(pr.el.points ?? [])]
      if (pts.length) pts[pts.length - 1] = { x: pos.x, y: pos.y }
      pr.el = { ...pr.el, points: pts }
      onElementsChange([...pr.base, pr.el])
      return
    }
    // Déplacement d'un élément sélectionné.
    if (!dragRef.current) return
    const px = pos.x * SLIDE_W
    const py = pos.y * SLIDE_H
    const dx = (px - dragRef.current.startX) / SLIDE_W
    const dy = (py - dragRef.current.startY) / SLIDE_H
    const moveId = dragRef.current.el.id
    const updated = elementsRef.current.map(el => {
      if (el.id !== moveId) return el
      if (el.type === 'line') {
        const l = el as LineElement
        const ddx = (dragRef.current!.origX + dx) - l.x
        const ddy = (dragRef.current!.origY + dy) - l.y
        return {
          ...l,
          x: l.x + ddx, y: l.y + ddy, x2: l.x2 + ddx, y2: l.y2 + ddy,
          points: l.points?.map(p => ({ x: p.x + ddx, y: p.y + ddy })),
        }
      }
      return { ...el, x: Math.max(0, dragRef.current!.origX + dx), y: Math.max(0, dragRef.current!.origY + dy) }
    })
    onElementsChange(updated)
  }, [slide, getCanvasPos, onElementsChange, publishCursor])

  const handleMouseUp = useCallback(() => {
    if (drawRef.current) {
      drawRef.current = null
      onToolChange('select') // un tracé terminé → retour à la sélection
    }
    dragRef.current = null
  }, [onToolChange])

  // Termine une polyligne (double-clic ou Échap) : retire le point « fantôme ».
  const finishPolyline = useCallback(() => {
    if (!polyRef.current) return
    const pr = polyRef.current
    polyRef.current = null
    const pts = [...(pr.el.points ?? [])]
    if (pts.length > 2) pts.pop()
    pr.el = { ...pr.el, points: pts }
    onElementsChange([...pr.base, pr.el])
    onToolChange('select')
  }, [onElementsChange, onToolChange])

  // ── Redimensionnement par poignées ───────────────────────────────────────────
  const onResizeMove = useCallback((ev: globalThis.MouseEvent) => {
    const r = resizeRef.current
    if (!r) return
    const dx = (ev.clientX - r.sx) / (SLIDE_W * scale)
    const dy = (ev.clientY - r.sy) / (SLIDE_H * scale)
    let x = r.ox, y = r.oy, w = r.ow, h = r.oh
    if (r.handle.includes('e')) w = Math.max(0.02, r.ow + dx)
    if (r.handle.includes('s')) h = Math.max(0.02, r.oh + dy)
    if (r.handle.includes('w')) { w = Math.max(0.02, r.ow - dx); x = r.ox + (r.ow - w) }
    if (r.handle.includes('n')) { h = Math.max(0.02, r.oh - dy); y = r.oy + (r.oh - h) }
    onElementsChange(elementsRef.current.map(el => el.id === r.id ? { ...el, x, y, w, h } : el))
  }, [scale, onElementsChange])

  const endResize = useCallback(() => {
    resizeRef.current = null
    document.removeEventListener('mousemove', onResizeMove)
    document.removeEventListener('mouseup', endResize)
    document.body.style.userSelect = ''
  }, [onResizeMove])

  const startResize = useCallback((e: React.MouseEvent, handle: string) => {
    e.preventDefault(); e.stopPropagation()
    const el = elementsRef.current.find(x => selection.includes(x.id))
    if (!el) return
    const geo = el.type === 'line' ? lineBBox(el as LineElement) : el
    resizeRef.current = { id: el.id, handle, sx: e.clientX, sy: e.clientY, ox: geo.x, oy: geo.y, ow: geo.w, oh: geo.h }
    document.addEventListener('mousemove', onResizeMove)
    document.addEventListener('mouseup', endResize)
    document.body.style.userSelect = 'none'
  }, [selection, onResizeMove, endResize])

  // ── Rotation ──────────────────────────────────────────────────────────────────
  const onRotateMove = useCallback((ev: globalThis.MouseEvent) => {
    const r = rotateRef.current
    if (!r || !wrapperRef.current) return
    const el = elementsRef.current.find(x => x.id === r.id)
    if (!el) return
    const geo = el.type === 'line' ? lineBBox(el as LineElement) : el
    const wr = wrapperRef.current.getBoundingClientRect()
    const cx = wr.left + (geo.x + geo.w / 2) * SLIDE_W * scale
    const cy = wr.top + (geo.y + geo.h / 2) * SLIDE_H * scale
    let deg = Math.atan2(ev.clientY - cy, ev.clientX - cx) * 180 / Math.PI + 90
    if (ev.shiftKey) deg = Math.round(deg / 15) * 15
    deg = Math.round(((deg % 360) + 360) % 360)
    onElementsChange(elementsRef.current.map(x => x.id === r.id ? { ...x, rotation: deg } : x))
  }, [scale, onElementsChange])

  const endRotate = useCallback(() => {
    rotateRef.current = null
    document.removeEventListener('mousemove', onRotateMove)
    document.removeEventListener('mouseup', endRotate)
    document.body.style.userSelect = ''
  }, [onRotateMove])

  const startRotate = useCallback((e: React.MouseEvent) => {
    e.preventDefault(); e.stopPropagation()
    const el = elementsRef.current.find(x => selection.includes(x.id))
    if (!el) return
    rotateRef.current = { id: el.id }
    document.addEventListener('mousemove', onRotateMove)
    document.addEventListener('mouseup', endRotate)
    document.body.style.userSelect = 'none'
  }, [selection, onRotateMove, endRotate])

  // ── Ajustement texte ↔ forme ──────────────────────────────────────────────────
  const applyAutofit = useCallback((mode: 'none' | 'shape' | 'shrink') => {
    setFitMenuOpen(false)
    const el = elementsRef.current.find(x => selection.includes(x.id))
    if (!el || el.type !== 'text') return
    const te = el as TextElement
    let next: TextElement = { ...te, autofit: mode }
    if (mode === 'shape') {
      // Hauteur de la forme ajustée au texte (espace diapositive).
      const fam = theme?.fontFamily ?? 'Arial, sans-serif'
      const txt = textElementPlainText(te) || (te.placeholder ?? '')
      const fs = te.fontSize ?? 24
      const padS = (te.padding ?? 8) * (te.w * SLIDE_W / 100)
      const hPx = measureTextHeightSlide(txt, fs, te.w * SLIDE_W - 2 * padS, fam) + 2 * padS
      next = { ...next, h: Math.max(0.04, hPx / SLIDE_H) }
    }
    onElementsChange(elementsRef.current.map(x => x.id === te.id ? next : x))
  }, [selection, theme, onElementsChange])

  // Double-clic sur un texte → édition inline ; sinon termine une polyligne.
  const handleDoubleClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (polyRef.current) { finishPolyline(); return }
    if (!rendererRef.current) return
    const pos = getCanvasPos(e)
    const hit = rendererRef.current.hitTest(pos.x * SLIDE_W, pos.y * SLIDE_H, elementsRef.current, SLIDE_W, SLIDE_H)
    if (hit && hit.type === 'text') {
      setSelection([hit.id])
      setEditText(textElementPlainText(hit as TextElement))
      setEditingId(hit.id)
    } else if (hit && hit.type === 'image') {
      enterCrop(hit.id)
    }
  }, [finishPolyline, getCanvasPos, enterCrop])

  const setEditingText = useCallback((val: string) => {
    setEditText(val)
    if (!editingId) return
    const content = textDocFromString(val)
    onElementsChange(elementsRef.current.map(el => el.id === editingId ? { ...el, content } : el))
  }, [editingId, onElementsChange])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (cropId) {
      if (e.key === 'Enter') { e.preventDefault(); confirmCrop() }
      else if (e.key === 'Escape') { e.preventDefault(); cancelCrop() }
      return
    }
    if (editingId) return
    if (e.key === 'Escape' && polyRef.current) {
      finishPolyline()
      return
    }
    const mod = e.ctrlKey || e.metaKey
    if ((e.key === 'Delete' || e.key === 'Backspace') && selection.length > 0 && slide) {
      e.preventDefault()
      const updated = elementsRef.current.filter(el => !selection.includes(el.id))
      onElementsChange(updated)
      setSelection([])
    } else if (mod && (e.key === 'd' || e.key === 'D') && selection.length > 0) {
      e.preventDefault(); duplicateSel()
    } else if (mod && selection.length > 0 && (e.key === ']' || e.key === '[')) {
      e.preventDefault(); arrangeZ(e.shiftKey ? (e.key === ']' ? 'front' : 'back') : (e.key === ']' ? 'forward' : 'backward'))
    } else if (selection.length > 0 && (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      // Déplacement fin des éléments sélectionnés (flèches ; pas large avec Maj).
      e.preventDefault()
      const step = (e.shiftKey ? 0.02 : 0.004)
      const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0
      const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0
      const ids = new Set(selection)
      onElementsChange(elementsRef.current.map(el => ids.has(el.id) ? translateEl(el, dx, dy) : el))
    }
  }, [selection, slide, onElementsChange, finishPolyline, editingId, cropId, confirmCrop, cancelCrop]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Mise en page (remplace les éléments de la diapositive) ───────────────────
  const applyLayout = useCallback((els: SlideElement[]) => {
    setCanvasMenu(null)
    onElementsChange(els)
    setSelection([])
  }, [onElementsChange])

  // ── Guides (repères) déplaçables ──────────────────────────────────────────────
  const onGuideMove = useCallback((ev: globalThis.MouseEvent) => {
    const r = guideDragRef.current
    if (!r || !wrapperRef.current) return
    const wr = wrapperRef.current.getBoundingClientRect()
    setGuides(gs => gs.map(g => {
      if (g.id !== r.id) return g
      const pos = g.axis === 'v' ? (ev.clientX - wr.left) / (SLIDE_W * scale) : (ev.clientY - wr.top) / (SLIDE_H * scale)
      return { ...g, pos: Math.max(0, Math.min(1, pos)) }
    }))
  }, [scale])
  const endGuide = useCallback(() => {
    guideDragRef.current = null
    document.removeEventListener('mousemove', onGuideMove)
    document.removeEventListener('mouseup', endGuide)
  }, [onGuideMove])
  const startGuideDrag = useCallback((e: React.MouseEvent, gid: string) => {
    e.preventDefault(); e.stopPropagation()
    guideDragRef.current = { id: gid }
    document.addEventListener('mousemove', onGuideMove)
    document.addEventListener('mouseup', endGuide)
  }, [onGuideMove, endGuide])
  const addGuide = useCallback((axis: 'v' | 'h') => {
    setGuides(g => [...g, { id: uid(), axis, pos: 0.5 }])
    setShowGuides(true)
  }, [])

  // ── Menu contextuel du fond de la zone de travail ─────────────────────────────
  // Grille des mises en page, embarquée dans le sous-menu « Mise en page » du
  // MenuDropdown via un item `custom` : reçoit `close` pour fermer le menu après
  // application de la mise en page choisie.
  const layoutGrid = (close: () => void) => (
    <div className="grid grid-cols-3 gap-3 p-3 w-[440px] max-h-[440px] overflow-y-auto bg-white border border-border rounded-lg shadow-xl">
      {SLIDE_LAYOUTS.map(L => {
        const els = L.build(t)
        return (
          <button key={L.id} type="button" onClick={() => { applyLayout(els); close() }} className="text-center group">
            <div className="rounded-sm ring-1 ring-transparent group-hover:ring-2 group-hover:ring-primary transition">
              <LayoutPreview els={els} />
            </div>
            <div className="text-[11px] mt-1 leading-tight text-text-secondary">{t(L.nameKey)}</div>
          </button>
        )
      })}
    </div>
  )
  // ── Opérations sur un élément (menu contextuel élément) ──────────────────────
  const updateEl = (id: string, fn: (e: SlideElement) => SlideElement) =>
    onElementsChange(elementsRef.current.map(e => e.id === id ? fn(e) : e))
  const rotateEl = (id: string, deg: number) => updateEl(id, e => ({ ...e, rotation: ((((e.rotation || 0) + deg) % 360) + 360) % 360 }))
  const flipEl = (id: string, axis: 'h' | 'v') => updateEl(id, e => axis === 'h' ? { ...e, flipX: !e.flipX } : { ...e, flipY: !e.flipY })
  const centerEl = (id: string, axis: 'h' | 'v') => updateEl(id, e => axis === 'h' ? { ...e, x: (1 - e.w) / 2 } : { ...e, y: (1 - e.h) / 2 })
  const deleteEl = (id: string) => { onElementsChange(elementsRef.current.filter(e => e.id !== id)); setSelection([]) }
  const copyEl = (id: string) => { const e = elementsRef.current.find(x => x.id === id); if (e) elementClipRef.current = [e] }
  const cutEl = (id: string) => { copyEl(id); deleteEl(id) }
  const pasteEl = () => {
    const clip = elementClipRef.current
    if (!clip?.length) return
    const news = clip.map(e => ({ ...e, id: uid(), x: Math.min(0.92, (e.x || 0) + 0.03), y: Math.min(0.92, (e.y || 0) + 0.03), zIndex: elementsRef.current.length + 1 }))
    onElementsChange([...elementsRef.current, ...news])
    setSelection(news.map(n => n.id))
  }
  const altEl = async (id: string) => {
    const e = elementsRef.current.find(x => x.id === id)
    const v = await prompt({ title: t('pres_ctx_alt'), message: t('pres_alt_msg'), defaultValue: e?.alt ?? '', allowEmpty: true })
    if (v !== null) updateEl(id, x => ({ ...x, alt: v }))
  }
  // Hyperlien d'un élément : URL (https://…) ou « #N » pour aller à la diapo N (suivi au diaporama).
  const linkEl = async (id: string) => {
    const e = elementsRef.current.find(x => x.id === id) as { link?: string } | undefined
    const v = await prompt({ title: t('pres_ctx_link', { defaultValue: 'Lien' }), message: t('pres_link_msg', { defaultValue: 'URL (https://…) ou « #3 » pour aller à la diapositive 3' }), placeholder: 'https://exemple.com', defaultValue: e?.link ?? '', allowEmpty: true })
    if (v !== null) updateEl(id, x => ({ ...x, link: v || undefined } as SlideElement))
  }

  // ── Opérations spécifiques aux images ─────────────────────────────────────────
  const replaceImage = (id: string) => { replaceTargetRef.current = id; replaceInputRef.current?.click() }
  const resetImageAspect = (id: string) => {
    const e = elementsRef.current.find(x => x.id === id) as ImageElement | undefined
    if (!e) return
    const img = resolveSlideImage(e.storagePath)
    const apply = () => {
      const aspect = (img.naturalHeight / (img.naturalWidth || 1)) * (SLIDE_W / SLIDE_H)
      updateEl(id, x => ({ ...x, flipX: false, flipY: false, rotation: 0, h: x.w * aspect }))
    }
    if (img.complete && img.naturalWidth > 0) { apply() }
    else { const prev = img.onload; img.onload = (ev) => { (prev as ((e: Event) => void) | null)?.call(img, ev as Event); apply() } }
  }
  const onReplacePicked = (file: File | undefined) => {
    const id = replaceTargetRef.current
    if (!file || !id || !file.type.startsWith('image/')) return
    uploadImageRef(file).then(({ ref }) => updateEl(id, x => ({ ...x, storagePath: ref }))).catch(() => {})
  }

  // ── Disposition : ordre Z, alignement, répartition, animation ─────────────────
  const selectionRef = useRef<string[]>([]); selectionRef.current = selection
  const bboxOf = (el: SlideElement) => el.type === 'line' ? lineBBox(el as LineElement) : { x: el.x, y: el.y, w: el.w, h: el.h }
  const translateEl = (el: SlideElement, dx: number, dy: number): SlideElement =>
    el.type === 'line'
      ? { ...el, x: el.x + dx, y: el.y + dy, x2: (el as LineElement).x2 + dx, y2: (el as LineElement).y2 + dy } as SlideElement
      : { ...el, x: el.x + dx, y: el.y + dy }
  // Réordonne l'ordre de superposition (zIndex) des éléments sélectionnés.
  const arrangeZ = (op: 'front' | 'back' | 'forward' | 'backward') => {
    const sel = new Set(selectionRef.current); if (!sel.size) return
    let order = [...elementsRef.current].sort((a, b) => (a.zIndex || 0) - (b.zIndex || 0)).map(e => e.id)
    if (op === 'front') order = [...order.filter(i => !sel.has(i)), ...order.filter(i => sel.has(i))]
    else if (op === 'back') order = [...order.filter(i => sel.has(i)), ...order.filter(i => !sel.has(i))]
    else if (op === 'forward') { for (let i = order.length - 2; i >= 0; i--) if (sel.has(order[i]) && !sel.has(order[i + 1])) [order[i], order[i + 1]] = [order[i + 1], order[i]] }
    else if (op === 'backward') { for (let i = 1; i < order.length; i++) if (sel.has(order[i]) && !sel.has(order[i - 1])) [order[i], order[i - 1]] = [order[i - 1], order[i]] }
    const z = new Map(order.map((id, i) => [id, i]))
    onElementsChange(elementsRef.current.map(e => ({ ...e, zIndex: z.get(e.id) ?? e.zIndex })))
  }
  // Aligne les éléments sélectionnés : sur leur boîte commune (≥2) ou sur la diapo (1).
  const alignSel = (mode: 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom') => {
    const ids = selectionRef.current; if (!ids.length) return
    const els = elementsRef.current.filter(e => ids.includes(e.id))
    let L = 0, T = 0, R = 1, B = 1
    if (els.length >= 2) { const bs = els.map(bboxOf); L = Math.min(...bs.map(b => b.x)); T = Math.min(...bs.map(b => b.y)); R = Math.max(...bs.map(b => b.x + b.w)); B = Math.max(...bs.map(b => b.y + b.h)) }
    onElementsChange(elementsRef.current.map(e => {
      if (!ids.includes(e.id)) return e
      const b = bboxOf(e); let dx = 0, dy = 0
      if (mode === 'left') dx = L - b.x; else if (mode === 'center') dx = (L + R) / 2 - (b.x + b.w / 2); else if (mode === 'right') dx = R - (b.x + b.w)
      else if (mode === 'top') dy = T - b.y; else if (mode === 'middle') dy = (T + B) / 2 - (b.y + b.h / 2); else if (mode === 'bottom') dy = B - (b.y + b.h)
      return translateEl(e, dx, dy)
    }))
  }
  // Répartit l'espacement des éléments sélectionnés (≥3) sur un axe.
  const distributeSel = (axis: 'h' | 'v') => {
    const ids = selectionRef.current; if (ids.length < 3) return
    const arr = elementsRef.current.filter(e => ids.includes(e.id)).map(e => ({ e, b: bboxOf(e) }))
    arr.sort((a, b) => axis === 'h' ? a.b.x - b.b.x : a.b.y - b.b.y)
    const start = axis === 'h' ? arr[0].b.x : arr[0].b.y
    const end = axis === 'h' ? arr[arr.length - 1].b.x : arr[arr.length - 1].b.y
    const gap = (end - start) / (arr.length - 1)
    const moves = new Map<string, number>()
    arr.forEach((o, i) => { const target = start + gap * i; moves.set(o.e.id, target - (axis === 'h' ? o.b.x : o.b.y)) })
    onElementsChange(elementsRef.current.map(e => { const d = moves.get(e.id); return d != null ? translateEl(e, axis === 'h' ? d : 0, axis === 'h' ? 0 : d) : e }))
  }
  const duplicateSel = () => {
    const ids = selectionRef.current
    const news = elementsRef.current.filter(e => ids.includes(e.id)).map(e => ({ ...e, id: uid(), x: Math.min(0.92, (e.x || 0) + 0.03), y: Math.min(0.92, (e.y || 0) + 0.03), zIndex: elementsRef.current.length + 1 }))
    if (news.length) { onElementsChange([...elementsRef.current, ...news]); setSelection(news.map(n => n.id)) }
  }
  const removeSel = () => { const ids = new Set(selectionRef.current); if (!ids.size) return; onElementsChange(elementsRef.current.filter(e => !ids.has(e.id))); setSelection([]) }
  // Affecte une animation d'entrée aux éléments sélectionnés (jouée au diaporama).
  const setAnimSel = (anim: { type: string; duration?: number } | null) => {
    const ids = new Set(selectionRef.current); if (!ids.size) return
    onElementsChange(elementsRef.current.map(e => ids.has(e.id) ? { ...e, anim: anim ?? undefined } as SlideElement : e))
  }
  // Expose l'API au parent (ruban Disposition / Animations) à chaque rendu.
  useEffect(() => {
    onApi?.({
      align: alignSel, distribute: distributeSel, zorder: arrangeZ,
      duplicate: duplicateSel, remove: removeSel, setAnim: setAnimSel,
      selCount: () => selectionRef.current.length,
      curAnim: () => { const s = selectionRef.current; const e = s.length === 1 ? elementsRef.current.find(x => x.id === s[0]) : null; return (e as { anim?: { type: string } } | undefined)?.anim?.type ?? 'none' },
    })
  })


  const elementMenuSections = (id: string): CtxItem[][] => [
    [
      { icon: <Scissors size={15} />, label: t('pres_ctx_cut'), shortcut: 'Ctrl+X', onClick: () => cutEl(id) },
      { icon: <Copy size={15} />, label: t('pres_ctx_copy'), shortcut: 'Ctrl+C', onClick: () => copyEl(id) },
      { icon: <ClipboardPaste size={15} />, label: t('pres_ctx_paste'), shortcut: 'Ctrl+V', disabled: !elementClipRef.current?.length, onClick: () => pasteEl() },
      { icon: <ClipboardX size={15} />, label: t('pres_ctx_paste_plain'), shortcut: 'Ctrl+Maj+V', disabled: true },
      { icon: <Trash2 size={15} />, label: t('common_delete'), danger: true, onClick: () => deleteEl(id) },
    ],
    [
      { icon: <Accessibility size={15} />, label: t('pres_ctx_alt'), shortcut: 'Ctrl+Alt+Y', onClick: () => altEl(id) },
      { icon: <RotateCw size={15} />, label: t('pres_ctx_rotate'), submenu: [
        { label: t('pres_rot_right'), onClick: () => rotateEl(id, 90) },
        { label: t('pres_rot_left'), onClick: () => rotateEl(id, -90) },
        { label: t('pres_flip_h'), onClick: () => flipEl(id, 'h') },
        { label: t('pres_flip_v'), onClick: () => flipEl(id, 'v') },
      ] },
      { icon: <Focus size={15} />, label: t('pres_ctx_center'), submenu: [
        { label: t('pres_center_h'), onClick: () => centerEl(id, 'h') },
        { label: t('pres_center_v'), onClick: () => centerEl(id, 'v') },
      ] },
      { icon: <CopyPlus size={15} />, label: t('pres_ctx_duplicate', { defaultValue: 'Dupliquer' }), shortcut: 'Ctrl+D', onClick: () => duplicateSel() },
    ],
    [
      { icon: <ArrowUpToLine size={15} />, label: t('pres_arrange', { defaultValue: 'Ordre' }), submenu: [
        { label: t('pres_z_front', { defaultValue: 'Mettre au premier plan' }), onClick: () => arrangeZ('front') },
        { label: t('pres_z_forward', { defaultValue: 'Avancer' }), onClick: () => arrangeZ('forward') },
        { label: t('pres_z_backward', { defaultValue: 'Reculer' }), onClick: () => arrangeZ('backward') },
        { label: t('pres_z_back', { defaultValue: "Mettre à l'arrière-plan" }), onClick: () => arrangeZ('back') },
      ] },
      { icon: <AlignHorizontalSpaceAround size={15} />, label: t('pres_align', { defaultValue: 'Aligner' }), submenu: [
        { label: t('pres_align_left', { defaultValue: 'Gauche' }), onClick: () => alignSel('left') },
        { label: t('pres_align_centerh', { defaultValue: 'Centrer (h)' }), onClick: () => alignSel('center') },
        { label: t('pres_align_right', { defaultValue: 'Droite' }), onClick: () => alignSel('right') },
        { label: t('pres_align_top', { defaultValue: 'Haut' }), onClick: () => alignSel('top') },
        { label: t('pres_align_middle', { defaultValue: 'Centrer (v)' }), onClick: () => alignSel('middle') },
        { label: t('pres_align_bottom', { defaultValue: 'Bas' }), onClick: () => alignSel('bottom') },
        { label: t('pres_distribute_h', { defaultValue: 'Répartir horizontalement' }), onClick: () => distributeSel('h') },
        { label: t('pres_distribute_v', { defaultValue: 'Répartir verticalement' }), onClick: () => distributeSel('v') },
      ] },
    ],
    [
      { icon: <Sparkles size={15} />, label: t('pres_ctx_animate', { defaultValue: 'Animation' }), submenu: PRES_ANIMATIONS.map(a => (
        { label: t(a.nameKey, { defaultValue: a.label }), onClick: () => setAnimSel(a.type === 'none' ? null : { type: a.type }) }
      )) },
      { icon: <LinkIcon size={15} />, label: t('pres_ctx_link', { defaultValue: 'Lien' }), shortcut: 'Ctrl+K', onClick: () => linkEl(id) },
    ],
  ]

  const canvasMenuSections: CtxItem[][] = [
    [
      { icon: <Scissors size={15} />, label: t('pres_ctx_cut'), shortcut: 'Ctrl+X', disabled: true },
      { icon: <Copy size={15} />, label: t('pres_ctx_copy'), shortcut: 'Ctrl+C', disabled: true },
      { icon: <ClipboardPaste size={15} />, label: t('pres_ctx_paste'), shortcut: 'Ctrl+V', disabled: true },
      { icon: <ClipboardX size={15} />, label: t('pres_ctx_paste_plain'), shortcut: 'Ctrl+Maj+V', disabled: true },
      { icon: <Trash2 size={15} />, label: t('common_delete'), disabled: true },
    ],
    [
      { icon: <Droplet size={15} />, label: t('pres_ctx_background'), onClick: () => onEditBackground() },
      { icon: <LayoutTemplate size={15} />, label: t('pres_ctx_layout'), customSubmenu: layoutGrid },
      { icon: <Paintbrush size={15} />, label: t('pres_ctx_theme'), disabled: true },
    ],
    [{ icon: <Replace size={15} />, label: t('pres_ctx_transition'), disabled: true }],
    [{ icon: <MessageSquarePlus size={15} />, label: t('pres_ctx_comment'), shortcut: 'Ctrl+Alt+M', disabled: true }],
    [{
      icon: <Hash size={15} />, label: t('pres_guides'), submenu: [
        { icon: <Eye size={15} />, label: t('pres_guides_show'), onClick: () => setShowGuides(s => !s) },
        { icon: <Plus size={15} />, label: t('pres_guides_add_v'), onClick: () => addGuide('v') },
        { icon: <Plus size={15} />, label: t('pres_guides_add_h'), onClick: () => addGuide('h') },
        { icon: <AlignVerticalSpaceAround size={15} />, label: t('pres_guides_edit'), onClick: () => setShowGuides(true) },
        { icon: <Trash2 size={15} />, label: t('pres_guides_clear'), onClick: () => setGuides([]) },
      ],
    }],
  ]

  return (
    <div
      ref={containerRef}
      className={`relative flex-1 flex items-center justify-center bg-surface-2 overflow-hidden outline-none ${dragOver ? 'ring-2 ring-inset ring-primary' : ''}`}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onDragOver={e => { if (e.dataTransfer?.types?.includes('Files')) { e.preventDefault(); setDragOver(true) } }}
      onDragLeave={e => { if (e.currentTarget === e.target) setDragOver(false) }}
      onDrop={e => {
        e.preventDefault(); setDragOver(false)
        const file = e.dataTransfer?.files?.[0]
        if (!file) return
        const pos = getCanvasPos(e as unknown as React.MouseEvent<HTMLCanvasElement>)
        const cx = pos.x >= 0 && pos.x <= 1 ? pos.x : 0.5
        const cy = pos.y >= 0 && pos.y <= 1 ? pos.y : 0.5
        onInsertImage(file, cx, cy)
      }}
      onContextMenu={e => {
        e.preventDefault()
        let elementId: string | null = null
        if (rendererRef.current) {
          const pos = getCanvasPos(e as unknown as React.MouseEvent<HTMLCanvasElement>)
          const hit = rendererRef.current.hitTest(pos.x * SLIDE_W, pos.y * SLIDE_H, elementsRef.current, SLIDE_W, SLIDE_H)
          // Conserve la multi-sélection si on clique droit sur un élément déjà sélectionné.
          if (hit) { elementId = hit.id; if (!selection.includes(hit.id)) setSelection([hit.id]) }
        }
        setCanvasMenu({ x: e.clientX, y: e.clientY, elementId })
      }}
    >
      <div
        ref={wrapperRef}
        className="relative shadow-xl"
        style={{
          width: SLIDE_W * scale,
          height: SLIDE_H * scale,
        }}
      >
        <canvas
          ref={canvasRef}
          style={{
            display: 'block',
            width: SLIDE_W * scale,
            height: SLIDE_H * scale,
            cursor: tool === 'select' ? 'default' : 'crosshair',
          }}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={() => publishCursor(null)}
          onDoubleClick={handleDoubleClick}
        />
        {/* Curseurs souris distants (présence) */}
        <RemoteCursors
          awareness={awareness}
          selfClientId={awareness?.clientID}
          toScreen={c => ({ left: c.x * SLIDE_W * scale, top: c.y * SLIDE_H * scale })}
        />
        {editingId && (() => {
          const el = slide?.elements?.find(e => e.id === editingId)
          if (!el || el.type !== 'text') return null
          const te = el as TextElement
          const bg = slide?.background?.type === 'color' ? (slide.background.color ?? '#ffffff') : '#ffffff'
          return (
            <textarea
              autoFocus
              value={editText}
              placeholder={te.placeholder ?? ''}
              onChange={ev => setEditingText(ev.target.value)}
              onBlur={() => setEditingId(null)}
              onKeyDown={ev => { if (ev.key === 'Escape') { ev.preventDefault(); setEditingId(null) } ev.stopPropagation() }}
              style={{
                position: 'absolute',
                left: te.x * SLIDE_W * scale,
                top: te.y * SLIDE_H * scale,
                width: te.w * SLIDE_W * scale,
                height: te.h * SLIDE_H * scale,
                fontSize: (te.fontSize ?? 24) * scale,
                lineHeight: 1.3,
                textAlign: te.align ?? 'left',
                color: te.color ?? theme?.textColor ?? '#202124',
                fontFamily: te.fontFamily ?? theme?.fontFamily ?? 'Arial, sans-serif',
                fontWeight: te.bold ? 'bold' : 'normal',
                fontStyle: te.italic ? 'italic' : 'normal',
                textDecoration: te.underline ? 'underline' : 'none',
                background: bg,
                border: 'none',
                outline: '2px solid #1a73e8',
                resize: 'none',
                padding: 2,
                margin: 0,
                overflow: 'hidden',
                boxSizing: 'border-box',
              }}
            />
          )
        })()}

        {/* Sélections distantes (présence par objet) : cadre coloré + nom */}
        {slide?.elements?.map(el => {
          const users = remoteSelections[el.id]
          if (!users?.length) return null
          const geo = el.type === 'line' ? lineBBox(el as LineElement) : { x: el.x, y: el.y, w: el.w, h: el.h }
          const u = users[0]
          return (
            <div key={`rs-${el.id}`} className="absolute pointer-events-none" style={{
              left: geo.x * SLIDE_W * scale, top: geo.y * SLIDE_H * scale,
              width: geo.w * SLIDE_W * scale, height: geo.h * SLIDE_H * scale,
              transform: el.rotation ? `rotate(${el.rotation}deg)` : undefined,
              outline: `2px solid ${u.color}`,
            }}>
              <div className="absolute -top-5 left-0 px-1.5 py-0.5 rounded text-[10px] font-medium text-white whitespace-nowrap" style={{ backgroundColor: u.color }}>
                {users.map(x => x.name).join(', ')}
              </div>
            </div>
          )
        })}

        {/* Overlay de sélection : poignées de redimensionnement + rotation + menu d'ajustement */}
        {!editingId && !cropId && selection.length === 1 && (() => {
          const el = slide?.elements?.find(e => e.id === selection[0])
          if (!el) return null
          const geo = el.type === 'line' ? lineBBox(el as LineElement) : { x: el.x, y: el.y, w: el.w, h: el.h }
          const corners: Record<string, { cur: string; pos: React.CSSProperties }> = {
            nw: { cur: 'nwse-resize', pos: { top: -5, left: -5 } },
            ne: { cur: 'nesw-resize', pos: { top: -5, right: -5 } },
            sw: { cur: 'nesw-resize', pos: { bottom: -5, left: -5 } },
            se: { cur: 'nwse-resize', pos: { bottom: -5, right: -5 } },
          }
          return (
            <div
              style={{
                position: 'absolute',
                left: geo.x * SLIDE_W * scale,
                top: geo.y * SLIDE_H * scale,
                width: geo.w * SLIDE_W * scale,
                height: geo.h * SLIDE_H * scale,
                transform: el.rotation ? `rotate(${el.rotation}deg)` : undefined,
                pointerEvents: 'none',
              }}
            >
              <div className="absolute inset-0 border-2 border-primary" />
              {el.type !== 'line' && (
                <>
                  {/* Rotation : tige + poignée au-dessus du bord supérieur */}
                  <div className="absolute left-1/2 -translate-x-1/2 -top-6 w-0.5 h-6 bg-primary" />
                  <div
                    onMouseDown={startRotate}
                    title={t('apex_rotate', { defaultValue: 'Pivoter' })}
                    className="absolute left-1/2 -translate-x-1/2 -top-9 w-5 h-5 rounded-full bg-white border-2 border-primary flex items-center justify-center cursor-grab active:cursor-grabbing"
                    style={{ pointerEvents: 'auto' }}
                  >
                    <RotateCw size={11} className="text-primary" />
                  </div>
                  {/* Coins (cercles) */}
                  {Object.entries(corners).map(([h, { cur, pos }]) => (
                    <div
                      key={h}
                      onMouseDown={e => startResize(e, h)}
                      className="absolute w-2.5 h-2.5 rounded-full bg-white border-2 border-primary"
                      style={{ ...pos, cursor: cur, pointerEvents: 'auto' }}
                    />
                  ))}
                  {/* Bords (pilules) */}
                  <div onMouseDown={e => startResize(e, 'n')} className="absolute left-1/2 -translate-x-1/2 h-2 w-5 rounded-full bg-white border-2 border-primary" style={{ top: -4, cursor: 'ns-resize', pointerEvents: 'auto' }} />
                  <div onMouseDown={e => startResize(e, 's')} className="absolute left-1/2 -translate-x-1/2 h-2 w-5 rounded-full bg-white border-2 border-primary" style={{ bottom: -4, cursor: 'ns-resize', pointerEvents: 'auto' }} />
                  <div onMouseDown={e => startResize(e, 'w')} className="absolute top-1/2 -translate-y-1/2 w-2 h-5 rounded-full bg-white border-2 border-primary" style={{ left: -4, cursor: 'ew-resize', pointerEvents: 'auto' }} />
                  <div onMouseDown={e => startResize(e, 'e')} className="absolute top-1/2 -translate-y-1/2 w-2 h-5 rounded-full bg-white border-2 border-primary" style={{ right: -4, cursor: 'ew-resize', pointerEvents: 'auto' }} />
                  {/* Menu d'ajustement texte ↔ forme (zones de texte) */}
                  {el.type === 'text' && (
                    <div className="absolute -left-2 -bottom-10" style={{ pointerEvents: 'auto' }}>
                      <button
                        onMouseDown={e => e.stopPropagation()}
                        onClick={e => { e.stopPropagation(); setFitMenuOpen(o => !o) }}
                        title={t('pres_fit_shape')}
                        className="w-8 h-8 rounded-full bg-white border border-border shadow flex items-center justify-center text-text-secondary hover:bg-surface-1"
                      >
                        <AlignVerticalSpaceAround size={15} />
                      </button>
                      {fitMenuOpen && (
                        <div className="absolute left-0 top-9 z-50 w-[340px] bg-white border border-border rounded-lg shadow-xl py-1.5 text-sm">
                          {[
                            { m: 'none' as const, icon: <Ban size={15} />, label: t('pres_fit_none') },
                            { m: 'shape' as const, icon: <AlignVerticalSpaceAround size={15} />, label: t('pres_fit_shape') },
                            { m: 'shrink' as const, icon: <Shrink size={15} />, label: t('pres_fit_text') },
                          ].map(opt => (
                            <button
                              key={opt.m}
                              onClick={() => applyAutofit(opt.m)}
                              className={`flex items-center gap-3 w-full px-3 py-2 text-left transition-colors hover:bg-surface-1
                                          ${(el as TextElement).autofit === opt.m || (!(el as TextElement).autofit && opt.m === 'none') ? 'text-primary' : 'text-text-primary'}`}
                            >
                              <span className="text-text-secondary flex-shrink-0">{opt.icon}</span>
                              {opt.label}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                  {/* Barre d'options d'image (en bas à gauche, façon Google Slides) */}
                  {el.type === 'image' && (
                    <div
                      className="absolute left-0 -bottom-12 flex items-center gap-0.5 bg-white border border-border rounded-full shadow-md px-1.5 py-1"
                      style={{ pointerEvents: 'auto', transform: el.rotation ? `rotate(${-el.rotation}deg)` : undefined, transformOrigin: 'left top' }}
                    >
                      <button onMouseDown={e => e.stopPropagation()} onClick={e => { e.stopPropagation(); replaceImage(el.id) }}
                        title={t('pres_img_replace')} className="w-7 h-7 rounded-full flex items-center justify-center text-text-secondary hover:bg-surface-1">
                        <ImageIcon size={15} />
                      </button>
                      <button onMouseDown={e => e.stopPropagation()} onClick={e => { e.stopPropagation(); enterCrop(el.id) }}
                        title={t('pres_crop')} className="w-7 h-7 rounded-full flex items-center justify-center text-text-secondary hover:bg-surface-1">
                        <Crop size={15} />
                      </button>
                      <button onMouseDown={e => e.stopPropagation()} onClick={e => { e.stopPropagation(); resetImageAspect(el.id) }}
                        title={t('pres_img_reset')} className="w-7 h-7 rounded-full flex items-center justify-center text-text-secondary hover:bg-surface-1">
                        <Maximize2 size={15} />
                      </button>
                      <button onMouseDown={e => e.stopPropagation()} onClick={e => { e.stopPropagation(); altEl(el.id) }}
                        title={t('pres_ctx_alt')} className="w-7 h-7 rounded-full flex items-center justify-center text-text-secondary hover:bg-surface-1">
                        <Accessibility size={15} />
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          )
        })()}

        {/* Rognage d'image : image complète atténuée + cadre + poignées */}
        {cropId && cropFrame && cropFullRef.current && (() => {
          const el = slide?.elements?.find(e => e.id === cropId) as ImageElement | undefined
          if (!el) return null
          const full = cropFullRef.current!
          const cf = cropFrame
          // `storagePath` est un `kbfile:<id>` (non chargeable par <img>) : on réutilise
          // l'URL résolue (object-URL) du cache, sinon l'image disparaît pendant le crop.
          const imgSrc = resolveSlideImage(el.storagePath).src
          const hc = 'absolute w-3 h-3 bg-white border-2 border-text-primary'
          const hStyle = (p: React.CSSProperties): React.CSSProperties => ({ ...p, pointerEvents: 'auto' })
          return (
            <>
              <div className="absolute inset-0" style={{ pointerEvents: 'auto' }} onMouseDown={confirmCrop} />
              <img src={imgSrc} draggable={false} alt="" style={{ position: 'absolute', left: full.l, top: full.t, width: full.w, height: full.h, maxWidth: 'none', maxHeight: 'none', opacity: 0.35, pointerEvents: 'none' }} />
              <div style={{ position: 'absolute', left: cf.l, top: cf.t, width: cf.w, height: cf.h, overflow: 'hidden', pointerEvents: 'none', outline: '2px solid #1a73e8' }}>
                <img src={imgSrc} draggable={false} alt="" style={{ position: 'absolute', left: full.l - cf.l, top: full.t - cf.t, width: full.w, height: full.h, maxWidth: 'none', maxHeight: 'none' }} />
              </div>
              <div onMouseDown={e => startCropDrag(e, 'move')} style={{ position: 'absolute', left: cf.l, top: cf.t, width: cf.w, height: cf.h, cursor: 'move', pointerEvents: 'auto' }} />
              <div onMouseDown={e => startCropDrag(e, 'nw')} className={hc} style={hStyle({ left: cf.l - 6, top: cf.t - 6, cursor: 'nwse-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 'ne')} className={hc} style={hStyle({ left: cf.l + cf.w - 6, top: cf.t - 6, cursor: 'nesw-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 'sw')} className={hc} style={hStyle({ left: cf.l - 6, top: cf.t + cf.h - 6, cursor: 'nesw-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 'se')} className={hc} style={hStyle({ left: cf.l + cf.w - 6, top: cf.t + cf.h - 6, cursor: 'nwse-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 'n')} className={hc} style={hStyle({ left: cf.l + cf.w / 2 - 6, top: cf.t - 6, cursor: 'ns-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 's')} className={hc} style={hStyle({ left: cf.l + cf.w / 2 - 6, top: cf.t + cf.h - 6, cursor: 'ns-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 'w')} className={hc} style={hStyle({ left: cf.l - 6, top: cf.t + cf.h / 2 - 6, cursor: 'ew-resize' })} />
              <div onMouseDown={e => startCropDrag(e, 'e')} className={hc} style={hStyle({ left: cf.l + cf.w - 6, top: cf.t + cf.h / 2 - 6, cursor: 'ew-resize' })} />
            </>
          )
        })()}

        {/* Guides (repères) déplaçables */}
        {showGuides && guides.map(g => (
          <div
            key={g.id}
            onMouseDown={e => startGuideDrag(e, g.id)}
            className={`absolute ${g.axis === 'v' ? 'top-0 bottom-0 cursor-ew-resize' : 'left-0 right-0 cursor-ns-resize'}`}
            style={g.axis === 'v'
              ? { left: g.pos * SLIDE_W * scale - 3, width: 6, pointerEvents: 'auto' }
              : { top: g.pos * SLIDE_H * scale - 3, height: 6, pointerEvents: 'auto' }}
          >
            <div className={g.axis === 'v'
              ? 'absolute left-1/2 top-0 bottom-0 w-px bg-red-400 -translate-x-1/2'
              : 'absolute top-1/2 left-0 right-0 h-px bg-red-400 -translate-y-1/2'} />
          </div>
        ))}
      </div>

      <input
        ref={replaceInputRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={e => { onReplacePicked(e.target.files?.[0]); if (e.target) e.target.value = '' }}
      />

      {canvasMenu && (
        <MenuDropdown
          pos={{ top: canvasMenu.y, left: canvasMenu.x, minWidth: 260 }}
          items={ctxToMenuItems(canvasMenu.elementId ? elementMenuSections(canvasMenu.elementId) : canvasMenuSections)}
          onClose={() => setCanvasMenu(null)}
        />
      )}
    </div>
  )
}

// ── LineToolDropdown — sélecteur de trait/connecteur (façon Google Slides) ──────

function LineToolDropdown({
  active,
  lineKind,
  onPick,
}: {
  active: boolean
  lineKind: LineKind
  onPick: (k: LineKind) => void
}) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDoc)
    return () => document.removeEventListener('mousedown', onDoc)
  }, [open])

  const current = LINE_KINDS.find(k => k.kind === lineKind) ?? LINE_KINDS[0]
  const CurIcon = current.Icon
  const cls = active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'

  return (
    <div ref={ref} className="relative flex items-center">
      <button
        title={t(current.labelKey)}
        onClick={() => onPick(lineKind)}
        className={`w-8 h-8 flex items-center justify-center rounded-l transition-colors ${cls}`}
      >
        <CurIcon size={16} />
      </button>
      <button
        title={t('pres_tool_line')}
        onClick={() => setOpen(o => !o)}
        className={`w-4 h-8 flex items-center justify-center rounded-r transition-colors ${cls}`}
      >
        <ChevronDown size={12} />
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-1 z-50 w-56 bg-white border border-border rounded-lg shadow-lg py-1">
          {LINE_KINDS.map(({ kind, Icon, labelKey }) => (
            <button
              key={kind}
              onClick={() => { onPick(kind); setOpen(false) }}
              className={`w-full flex items-center gap-3 px-3 py-2 text-sm text-left transition-colors hover:bg-surface-2
                          ${kind === lineKind ? 'text-primary' : 'text-text-primary'}`}
            >
              <Icon size={16} className={kind === lineKind ? 'text-primary' : 'text-text-secondary'} />
              {t(labelKey)}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

// Aperçu SVG miniature d'une forme (pour le sélecteur de formes).
function ShapeMini({ kind, size = 18 }: { kind: string; size?: number }) {
  const common = { fill: 'currentColor', stroke: 'none' }
  const inner = (() => {
    switch (kind) {
      case 'ellipse': return <ellipse cx="12" cy="12" rx="9" ry="7" {...common} />
      case 'roundRect': return <rect x="3" y="5" width="18" height="14" rx="4" {...common} />
      case 'triangle': return <polygon points="12,4 21,20 3,20" {...common} />
      case 'diamond': return <polygon points="12,3 21,12 12,21 3,12" {...common} />
      case 'pentagon': return <polygon points="12,3 21,10 17,21 7,21 3,10" {...common} />
      case 'hexagon': return <polygon points="7,4 17,4 22,12 17,20 7,20 2,12" {...common} />
      case 'star': return <polygon points="12,2 14.5,9 22,9 16,13.5 18,21 12,16.5 6,21 8,13.5 2,9 9.5,9" {...common} />
      case 'rightArrow': return <polygon points="3,9 14,9 14,5 22,12 14,19 14,15 3,15" {...common} />
      case 'chevron': return <polygon points="3,4 15,4 21,12 15,20 3,20 9,12" {...common} />
      case 'plus': return <polygon points="9,3 15,3 15,9 21,9 21,15 15,15 15,21 9,21 9,15 3,15 3,9 9,9" {...common} />
      case 'speech': return <path d="M4 4h16v11H10l-4 5 1-5H4z" {...common} />
      case 'heart': return <path d="M12 21C7 17 3 13 3 8.5 3 6 5 4 7.5 4 9.5 4 11 5.5 12 7c1-1.5 2.5-3 4.5-3C19 4 21 6 21 8.5 21 13 17 17 12 21z" {...common} />
      default: return <rect x="3" y="5" width="18" height="14" {...common} />
    }
  })()
  return <svg width={size} height={size} viewBox="0 0 24 24">{inner}</svg>
}

// Sélecteur de formes (grille déroulante), calqué sur LineToolDropdown.
function ShapeToolDropdown({ active, shapeKind, onPick }: { active: boolean; shapeKind: string; onPick: (k: string) => void }) {
  const { t } = useTranslation('office')
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const onDoc = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false) }
    document.addEventListener('mousedown', onDoc)
    return () => document.removeEventListener('mousedown', onDoc)
  }, [open])
  const cls = active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'
  return (
    <div ref={ref} className="relative flex items-center">
      <button title={t('pres_tool_shape')} onClick={() => onPick(shapeKind)} className={`w-8 h-8 flex items-center justify-center rounded-l transition-colors ${cls}`}>
        <ShapeMini kind={shapeKind} />
      </button>
      <button title={t('pres_tool_shape')} onClick={() => setOpen(o => !o)} className={`w-4 h-8 flex items-center justify-center rounded-r transition-colors ${cls}`}>
        <ChevronDown size={12} />
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-1 z-50 w-[232px] bg-white border border-border rounded-lg shadow-lg p-2 grid grid-cols-6 gap-1">
          {SHAPE_KINDS.map(s => (
            <button key={s.kind} title={t(s.nameKey, { defaultValue: s.label })} onClick={() => { onPick(s.kind); setOpen(false) }}
              className={`w-8 h-8 flex items-center justify-center rounded transition-colors hover:bg-surface-2 ${s.kind === shapeKind ? 'text-primary bg-primary-light' : 'text-text-secondary'}`}>
              <ShapeMini kind={s.kind} />
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

// ── SlideToolbar ──────────────────────────────────────────────────────────────

const FONT_FAMILIES = ['Arial', 'Google Sans', 'Times New Roman', 'Georgia', 'Courier New', 'Verdana', 'Trebuchet MS', 'Comic Sans MS']

// Barre de mise en forme du texte (affichée quand une zone de texte est sélectionnée).
function TextFormatControls({ te, onUpdate }: { te: TextElement; onUpdate: (p: Record<string, unknown>) => void }) {
  const { t } = useTranslation('office')
  const fontFamilies = useSystemFonts(FONT_FAMILIES)
  const fontSize = te.fontSize ?? 24
  const toggleBtn = (active: boolean) =>
    `w-7 h-7 flex items-center justify-center rounded transition-colors ${active ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`
  const ph = 'w-7 h-7 flex items-center justify-center rounded text-text-tertiary/60 cursor-default'
  return (
    <>
      <FontPicker
        value={te.fontFamily ?? 'Arial'}
        onChange={v => onUpdate({ fontFamily: v })}
        fonts={fontFamilies}
        width={120} height={28} fontSize={13}
      />
      <div className="w-px h-5 bg-border mx-1" />
      <button title="−" onClick={() => onUpdate({ fontSize: Math.max(6, Math.round(fontSize) - 1) })}
        className="w-6 h-7 flex items-center justify-center rounded hover:bg-surface-2 text-text-secondary"><Minus size={14} /></button>
      <input
        type="text" value={Math.round(fontSize)}
        onChange={e => { const n = parseInt(e.target.value, 10); if (!isNaN(n)) onUpdate({ fontSize: Math.max(6, Math.min(400, n)) }) }}
        className="w-9 h-7 text-center text-sm border border-border rounded mx-0.5"
      />
      <button title="+" onClick={() => onUpdate({ fontSize: Math.min(400, Math.round(fontSize) + 1) })}
        className="w-6 h-7 flex items-center justify-center rounded hover:bg-surface-2 text-text-secondary"><Plus size={14} /></button>
      <div className="w-px h-5 bg-border mx-1" />
      <button title={t('pres_bold')} onClick={() => onUpdate({ bold: !te.bold })} className={toggleBtn(!!te.bold)}><Bold size={15} /></button>
      <button title={t('pres_italic')} onClick={() => onUpdate({ italic: !te.italic })} className={toggleBtn(!!te.italic)}><Italic size={15} /></button>
      <button title={t('pres_underline')} onClick={() => onUpdate({ underline: !te.underline })} className={toggleBtn(!!te.underline)}><UnderlineIcon size={15} /></button>
      <div className="flex items-center justify-center w-7 h-7" title={t('pres_text_color')}>
        <ColorField width={20} height={20} color={te.color ?? '#202124'} onChange={hex => onUpdate({ color: hex })} />
      </div>
      <button title={t('pres_highlight')} className={ph}><Highlighter size={15} /></button>
      <div className="w-px h-5 bg-border mx-1" />
      <button title={t('pres_ctx_link')} className={ph}><LinkIcon size={15} /></button>
      <div className="w-px h-5 bg-border mx-1" />
      {(['left', 'center', 'right'] as const).map(a => (
        <button key={a} title={t('pres_align')} onClick={() => onUpdate({ align: a })} className={toggleBtn((te.align ?? 'left') === a)}>
          {a === 'left' ? <AlignLeft size={15} /> : a === 'center' ? <AlignCenter size={15} /> : <AlignRight size={15} />}
        </button>
      ))}
      <button title={t('pres_line_spacing')} className={ph}><AlignVerticalSpaceAround size={15} /></button>
      <button title={t('pres_highlight')} className={ph}><List size={15} /></button>
      <button className={ph}><ListOrdered size={15} /></button>
      <button className={ph}><IndentDecrease size={15} /></button>
      <button className={ph}><IndentIncrease size={15} /></button>
      <button title={t('pres_clear_fmt')} onClick={() => onUpdate({ bold: false, italic: false, underline: false })}
        className="w-7 h-7 flex items-center justify-center rounded hover:bg-surface-2 text-text-secondary"><RemoveFormatting size={15} /></button>
    </>
  )
}

function SlideToolbar({
  tool,
  lineKind,
  onToolChange,
  onLineKindChange,
  onPickImage,
  selectedEl,
  onUpdateSelected,
  onReplaceImage,
  onCrop,
  shapeKind,
  onShapeKindChange,
  macrosSlot,
}: {
  tool: string
  lineKind: LineKind
  onToolChange: (t: string) => void
  onLineKindChange: (k: LineKind) => void
  onPickImage: () => void
  selectedEl: SlideElement | null
  onUpdateSelected: (patch: Record<string, unknown>) => void
  onReplaceImage: () => void
  onCrop: () => void
  shapeKind: string
  onShapeKindChange: (k: string) => void
  // Macros button (Script sub-module), rendered at the trailing edge of the toolbar.
  macrosSlot?: ReactNode
}) {
  const { t } = useTranslation('office')
  const tools = [
    { id: 'select', icon: <MousePointer size={16} />, title: t('pres_tool_select') },
    { id: 'text', icon: <Type size={16} />, title: t('pres_tool_text') },
  ]
  const isText = selectedEl?.type === 'text'
  const isImage = selectedEl?.type === 'image'
  const lbl = 'flex items-center gap-1.5 h-8 px-2 text-xs text-text-secondary hover:bg-surface-2 rounded transition-colors'
  const phBtn = 'w-8 h-8 flex items-center justify-center rounded text-text-tertiary/60 cursor-default'

  return (
    <div className="flex items-center gap-0.5 px-3 py-1 border-b border-border bg-white flex-shrink-0 overflow-x-auto">
      {tools.map(tl => (
        <button
          key={tl.id}
          title={tl.title}
          onClick={() => onToolChange(tl.id)}
          className={`w-8 h-8 flex-shrink-0 flex items-center justify-center rounded transition-colors
                      ${tool === tl.id ? 'bg-primary-light text-primary' : 'text-text-secondary hover:bg-surface-2'}`}
        >
          {tl.icon}
        </button>
      ))}
      <ShapeToolDropdown
        active={tool === 'shape'}
        shapeKind={shapeKind}
        onPick={k => { onShapeKindChange(k); onToolChange('shape') }}
      />
      <LineToolDropdown
        active={tool === 'line'}
        lineKind={lineKind}
        onPick={k => { onLineKindChange(k); onToolChange('line') }}
      />
      <button
        title={t('pres_tool_image')}
        onClick={onPickImage}
        className="w-8 h-8 flex-shrink-0 flex items-center justify-center rounded transition-colors text-text-secondary hover:bg-surface-2"
      >
        <ImageIcon size={16} />
      </button>
      <div className="w-px h-5 bg-border mx-1 flex-shrink-0" />

      {isText ? (
        <div className="flex items-center gap-0.5 flex-shrink-0">
          <TextFormatControls te={selectedEl as TextElement} onUpdate={onUpdateSelected} />
          <div className="w-px h-5 bg-border mx-1" />
          <button className={lbl}>{t('pres_ctx_format')}</button>
          <button className={lbl}>{t('pres_ctx_animate')}</button>
        </div>
      ) : isImage ? (
        <div className="flex items-center gap-0.5 flex-shrink-0">
          <button title={t('pres_crop')} onClick={onCrop} className="w-8 h-8 flex items-center justify-center rounded text-text-secondary hover:bg-surface-2"><Crop size={16} /></button>
          <button title={t('pres_recolor')} className={phBtn}><ImageIcon size={16} /></button>
          <div className="w-px h-5 bg-border mx-1" />
          <button onClick={onReplaceImage} className={lbl}><ImageIcon size={14} /> {t('pres_img_replace')}</button>
          <button className={lbl}>{t('pres_ctx_format')}</button>
          <button className={lbl}>{t('pres_ctx_animate')}</button>
        </div>
      ) : (
        <>
          <button className={lbl} title={t('pres_slide_background')}>
            <Square size={14} className="fill-white stroke-border" />
            {t('pres_background')}
          </button>
          <button className={lbl} title={t('pres_theme')}>
            {t('pres_theme')} <ChevronDown size={12} />
          </button>
          <button className={lbl} title={t('pres_transition')}>
            {t('pres_transition')} <ChevronDown size={12} />
          </button>
        </>
      )}
      {macrosSlot && (
        <>
          <div className="flex-1 min-w-2" />
          <div className="w-px h-5 bg-border mx-1 flex-shrink-0" />
          {macrosSlot}
        </>
      )}
    </div>
  )
}

// ── PresenterNotes ────────────────────────────────────────────────────────────

function PresenterNotes({
  notes,
  onChange,
}: {
  notes: string
  onChange: (notes: string) => void
}) {
  const { t } = useTranslation('office')
  return (
    <div className="flex-shrink-0 border-t border-border bg-white" style={{ height: 100 }}>
      <div className="h-full flex flex-col">
        <div className="px-3 pt-1.5 pb-0.5">
          <span className="text-[10px] font-medium text-text-tertiary uppercase tracking-wide">
            {t('pres_presenter_notes')}
          </span>
        </div>
        <textarea
          value={notes}
          onChange={e => onChange(e.target.value)}
          className="flex-1 px-3 pb-2 text-sm text-text-primary resize-none focus:outline-none bg-transparent"
          placeholder={t('pres_notes_placeholder')}
        />
      </div>
    </div>
  )
}

// ── PresenterMode ─────────────────────────────────────────────────────────────

function PresenterMode({
  slides,
  fullSlides,
  theme,
  startIndex,
  onClose,
}: {
  slides: SlideSummary[]
  fullSlides: Record<string, Slide>
  theme: Presentation['theme']
  startIndex: number
  onClose: () => void
}) {
  // Diapositives VISIBLES uniquement (masquées exclues du diaporama).
  const visible = useMemo(() => slides.filter(s => !s.is_hidden), [slides])
  const startVis = Math.max(0, visible.findIndex(s => s.id === slides[startIndex]?.id))
  const [current, setCurrent] = useState(startVis < 0 ? 0 : startVis)
  const [step, setStep] = useState(0)            // nb d'éléments animés révélés
  const [elapsed, setElapsed] = useState(0)
  const [black, setBlack] = useState(false)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const wrapRef = useRef<HTMLDivElement>(null)
  const rendererRef = useRef<SlideRenderer | null>(null)
  const animRef = useRef<number | null>(null)

  const slide = fullSlides[visible[current]?.id ?? ''] ?? null
  // Éléments animés de la diapositive, dans l'ordre de superposition (= ordre de révélation).
  const animIds = useMemo(() => (slide?.elements ?? [])
    .filter(e => e.anim && e.anim.type !== 'none')
    .sort((a, b) => (a.zIndex ?? 0) - (b.zIndex ?? 0))
    .map(e => e.id), [slide])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    try { rendererRef.current = new SlideRenderer(canvas, SLIDE_W, SLIDE_H) } catch { /* ignore */ }
  }, [])

  // Rendu statique de l'état courant (hors animation d'élément en cours).
  const renderStatic = useCallback(() => {
    if (!rendererRef.current || !slide) return
    rendererRef.current.renderPresent(slide, theme, { hidden: new Set(animIds.slice(step)) })
  }, [slide, theme, animIds, step])
  useEffect(() => { renderStatic() }, [renderStatic])

  // Nouvelle diapositive → réinitialise les étapes.
  useEffect(() => { setStep(0) }, [current])

  // Minuteur d'exposé.
  useEffect(() => { const iv = setInterval(() => setElapsed(e => e + 1), 1000); return () => clearInterval(iv) }, [])

  // Joue l'animation d'entrée de l'élément `idx` (rAF), puis incrémente l'étape.
  const playReveal = useCallback((idx: number) => {
    if (!rendererRef.current || !slide) return
    const elId = animIds[idx]
    const el = slide.elements?.find(e => e.id === elId)
    const dur = el?.anim?.duration ?? 450
    const hidden = new Set(animIds.slice(idx))   // celui-ci + suivants masqués (l'animé est dessiné)
    const t0 = performance.now()
    const frame = (now: number) => {
      const t = Math.min(1, (now - t0) / dur)
      rendererRef.current!.renderPresent(slide!, theme, { hidden, animating: { id: elId, t } })
      if (t < 1) { animRef.current = requestAnimationFrame(frame) }
      else { animRef.current = null; setStep(idx + 1) }
    }
    animRef.current = requestAnimationFrame(frame)
  }, [animIds, slide, theme])

  const next = useCallback(() => {
    if (animRef.current) return
    if (step < animIds.length) { playReveal(step) }
    else if (current < visible.length - 1) setCurrent(c => c + 1)
  }, [step, animIds.length, current, visible.length, playReveal])
  const prev = useCallback(() => {
    if (animRef.current) { cancelAnimationFrame(animRef.current); animRef.current = null }
    if (step > 0) setStep(s => s - 1)
    else if (current > 0) { setCurrent(c => c - 1); setStep(0) }
  }, [step, current])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'ArrowRight' || e.key === ' ' || e.key === 'PageDown' || e.key === 'Enter') { e.preventDefault(); next() }
      else if (e.key === 'ArrowLeft' || e.key === 'PageUp') { e.preventDefault(); prev() }
      else if (e.key === 'Escape') onClose()
      else if (e.key === 'b' || e.key === 'B') setBlack(b => !b)
      else if (e.key === 'Home') { setCurrent(0); setStep(0) }
      else if (e.key === 'End') { setCurrent(visible.length - 1); setStep(0) }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [next, prev, onClose, visible.length])

  // Transition de la diapositive ENTRANTE : (re)déclenchée IMPÉRATIVEMENT sur le
  // wrapper (sans remonter le canvas, sinon le renderer perd sa référence → noir).
  const transAnim: Record<string, string> = { fade: 'kbp_fade', slideL: 'kbp_slideL', slideR: 'kbp_slideR', slideU: 'kbp_slideU', zoom: 'kbp_zoom', flip: 'kbp_flip' }
  useEffect(() => {
    const el = wrapRef.current; if (!el) return
    const type = slide?.transition?.type ?? 'none'
    const dur = slide?.transition?.duration ?? 500
    if (type === 'none' || !transAnim[type]) { el.style.animation = ''; return }
    el.style.animation = 'none'; void el.offsetHeight   // reflow pour rejouer
    el.style.animation = `${transAnim[type]} ${dur}ms ease both`
  }, [current, slide]) // eslint-disable-line react-hooks/exhaustive-deps
  const mmss = `${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`

  return (
    <div className="fixed inset-0 z-50 bg-black flex flex-col" onClick={next} onContextMenu={e => { e.preventDefault(); prev() }}>
      <style>{`
        @keyframes kbp_fade { from { opacity: 0 } to { opacity: 1 } }
        @keyframes kbp_slideL { from { transform: translateX(100%) } to { transform: translateX(0) } }
        @keyframes kbp_slideR { from { transform: translateX(-100%) } to { transform: translateX(0) } }
        @keyframes kbp_slideU { from { transform: translateY(100%) } to { transform: translateY(0) } }
        @keyframes kbp_zoom { from { transform: scale(.7); opacity: 0 } to { transform: scale(1); opacity: 1 } }
        @keyframes kbp_flip { from { transform: perspective(1200px) rotateY(90deg); opacity: 0 } to { transform: rotateY(0); opacity: 1 } }
      `}</style>
      <div className="flex-1 flex items-center justify-center overflow-hidden">
        <div ref={wrapRef}>
          <canvas ref={canvasRef} style={{ maxWidth: '100vw', maxHeight: '82vh', display: black ? 'none' : 'block' }} />
        </div>
      </div>

      {slide?.notes && !black && (
        <div className="px-8 py-4 bg-black/80 text-white text-sm max-h-32 overflow-y-auto" onClick={e => e.stopPropagation()}>{slide.notes}</div>
      )}

      <div className="flex items-center justify-center gap-4 py-3 bg-black/60" onClick={e => e.stopPropagation()}>
        <button onClick={prev} disabled={current === 0 && step === 0} className="text-white disabled:opacity-30 hover:opacity-80"><ChevronLeft size={24} /></button>
        <span className="text-white text-sm tabular-nums">{current + 1} / {visible.length}</span>
        <button onClick={next} disabled={current === visible.length - 1 && step >= animIds.length} className="text-white disabled:opacity-30 hover:opacity-80"><ChevronRight size={24} /></button>
        <span className="text-white/70 text-xs tabular-nums ml-4">{mmss}</span>
        <button onClick={onClose} className="ml-6 text-white hover:opacity-80"><X size={20} /></button>
      </div>
    </div>
  )
}

// ── PresentationEditorPage ────────────────────────────────────────────────────

export default function PresentationEditorPage() {
  const { t } = useTranslation('office')
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()

  // Présentation courante pour la résolution/upload des assets image (module-level).
  currentPresId = id
  useEffect(() => () => { if (currentPresId === id) currentPresId = undefined }, [id])

  const [activeSlideId, setActiveSlideId] = useState<string | null>(null)
  // Sélection (multi) de diapositives dans la liste. Peut être vide.
  const [selection, setSelection] = useState<string[]>([])
  const [shareOpen, setShareOpen] = useState(false)
  const [tool, setTool] = useState('select')
  const [lineKind, setLineKind] = useState<LineKind>('straight')
  const [shapeKind, setShapeKind] = useState('rect')
  // Panneaux plus étroits par défaut → la zone de travail prend plus de place.
  const [slidePanelW, setSlidePanelW] = useResizableWidth('kubuno.pres.slidePanelW', 150, 120, 360)
  // Presse-papiers de diapositives (copier/couper/coller — multi possible).
  const slideClipboardRef = useRef<{ elements: SlideElement[]; background: SlideBackground; notes: string; transition: Slide['transition'] }[] | null>(null)
  const [canPasteSlide, setCanPasteSlide] = useState(false)
  const [bgEditorOpen, setBgEditorOpen] = useState(false)
  // Élément sélectionné (remonté par SlideCanvas) → barre d'outils contextuelle.
  const [selectedEl, setSelectedEl] = useState<SlideElement | null>(null)
  const [cropSignal, setCropSignal] = useState(0)
  const replaceImgInputRef = useRef<HTMLInputElement>(null)

  // ── Collaboration temps réel (Yjs) ──────────────────────────────────────────
  // Un Y.Doc par présentation ; le contenu de chaque slide (éléments + notes) vit
  // dans un Y.Map `slides` keyé par slideId. Granularité = slide (deux participants
  // sur des slides différentes fusionnent ; même slide = dernier sauveur gagne).
  const ydoc = useMemo(() => new Y.Doc(), [id])
  const awareness = useMemo(() => new Awareness(ydoc), [ydoc])
  const slidesYMap = useMemo(() => ydoc.getMap<{ elements: SlideElement[]; notes: string }>('slides'), [ydoc])
  const authUser = useAuthStore(s => s.user)
  const [collabReady, setCollabReady] = useState(false)
  useEffect(() => { setCollabReady(false) }, [id])
  useCollab(`office-presentation:${id}`, ydoc, !!id, { awareness, onSync: () => setCollabReady(true) })
  useEffect(() => () => awareness.destroy(), [awareness])
  useEffect(() => {
    if (!authUser) return
    awareness.setLocalStateField('user', {
      id: authUser.id, name: authUser.display_name || authUser.username || authUser.email,
      color: userColor(authUser.id), avatar: authUser.avatar_url,
    })
  }, [awareness, authUser])
  const presenceUsers = usePresenceUsers(awareness, awareness.clientID)
  // Diffuse en temps réel la slide courante + l'objet sélectionné (présence par objet).
  useEffect(() => {
    awareness.setLocalStateField('sel', { slide: activeSlideId, el: selectedEl?.id ?? null })
  }, [awareness, activeSlideId, selectedEl])
  // Présence distante agrégée : qui est sur quelle slide / sélectionne quel objet.
  const slidePresence: Record<string, PresenceUser[]> = {}
  const elementPresence: Record<string, PresenceUser[]> = {}
  awareness.getStates().forEach((st, cid) => {
    if (cid === awareness.clientID) return
    const s = st as { user?: PresenceUser; sel?: { slide?: string | null; el?: string | null } }
    if (!s.user) return
    const sl = s.sel?.slide
    if (sl) (slidePresence[sl] ??= []).push(s.user)
    if (sl && sl === activeSlideId && s.sel?.el) (elementPresence[s.sel.el] ??= []).push(s.user)
  })
  const applyingRemoteRef = useRef(false)
  // Diffuse le contenu d'une slide (éléments + notes) dans le Y.Map (sauf application distante).
  const pushSlideToYjs = useCallback((sid: string, elements: SlideElement[], slideNotes: string) => {
    if (applyingRemoteRef.current) return
    const val = JSON.parse(JSON.stringify({ elements, notes: slideNotes }))
    if (JSON.stringify(slidesYMap.get(sid)) !== JSON.stringify(val)) {
      ydoc.transact(() => slidesYMap.set(sid, val), 'local')
    }
  }, [slidesYMap, ydoc])
  const [presenterMode, setPresenterMode] = useState(false)
  const [titleDraft, setTitleDraft] = useState('')
  const [fullSlides, setFullSlides] = useState<Record<string, Slide>>({})
  const [notes, setNotes] = useState('')

  // API impérative de la canvas (disposition / animations, pilotée par le ruban).
  const canvasApiRef = useRef<CanvasApi | null>(null)
  // Historique annuler/rétablir (par diapositive ; bursts coalescés).
  const undoRef = useRef<{ sid: string; els: SlideElement[] }[]>([])
  const redoRef = useRef<{ sid: string; els: SlideElement[] }[]>([])
  const lastHistRef = useRef<{ sid: string; t: number }>({ sid: '', t: 0 })

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const notesTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingSlideRef = useRef<{ sid: string; data: { elements: SlideElement[] } } | null>(null)
  const pendingNotesRef = useRef<{ sid: string; data: { notes: string } } | null>(null)

  // ── Queries ──────────────────────────────────────────────────────────────────

  const { data: presData, isLoading } = useQuery({
    queryKey: ['presentation', id],
    queryFn: () => presentationsApi.get(id!),
    enabled: !!id,
  })

  const pres = presData?.presentation ?? null
  const slides: SlideSummary[] = presData?.slides ?? []

  // Sélection initiale : la 1ʳᵉ diapositive (une seule fois, au chargement).
  const initRef = useRef(false)
  useEffect(() => {
    if (!initRef.current && slides.length > 0) {
      initRef.current = true
      setActiveSlideId(slides[0].id)
      setSelection([slides[0].id])
    }
  }, [slides])

  // Élague la sélection des ids qui n'existent plus (après suppression/refetch).
  useEffect(() => {
    if (!selection.length) return
    const valid = selection.filter(sid => slides.some(s => s.id === sid))
    if (valid.length !== selection.length) setSelection(valid)
  }, [slides, selection])

  // Diapositive initiale vide → on l'amorce avec les placeholders titre/sous-titre.
  const seededFirstRef = useRef(false)
  useEffect(() => {
    if (seededFirstRef.current || !id || slides.length !== 1) return
    const sid = slides[0].id
    const full = fullSlides[sid]
    if (full && (full.elements?.length ?? 0) === 0) {
      seededFirstRef.current = true
      presentationsApi.updateSlide(id, sid, { elements: placeholderSlideElements(t) })
        .then(s => setFullSlides(prev => ({ ...prev, [s.id]: s })))
        .catch(() => {})
    }
  }, [slides, fullSlides, id, t])

  // Fetch full slide when selected
  useEffect(() => {
    if (!activeSlideId || !id) return
    if (fullSlides[activeSlideId]) {
      setNotes(fullSlides[activeSlideId].notes ?? '')
      return
    }
    presentationsApi.getSlide(id, activeSlideId).then(slide => {
      setFullSlides(prev => ({ ...prev, [slide.id]: slide }))
      setNotes(slide.notes ?? '')
    }).catch(() => {})
  }, [activeSlideId, id, fullSlides])

  const activeSlideIdRef = useRef(activeSlideId); activeSlideIdRef.current = activeSlideId

  // Collab : changements distants d'une slide → mise à jour de fullSlides.
  useEffect(() => {
    const handler = (e: Y.YMapEvent<{ elements: SlideElement[]; notes: string }>, txn: Y.Transaction) => {
      if (txn.origin === 'local') return
      const changed = Array.from(e.keysChanged)
      applyingRemoteRef.current = true
      setFullSlides(prev => {
        const next = { ...prev }
        for (const sid of changed) {
          const v = slidesYMap.get(sid)
          if (v) next[sid] = { ...(next[sid] ?? {}), elements: v.elements, notes: v.notes } as Slide
        }
        return next
      })
      const cur = activeSlideIdRef.current
      if (cur && changed.includes(cur)) { const v = slidesYMap.get(cur); if (v) setNotes(v.notes ?? '') }
      applyingRemoteRef.current = false
    }
    slidesYMap.observe(handler)
    return () => slidesYMap.unobserve(handler)
  }, [slidesYMap])

  // Collab : à la 1ʳᵉ ouverture d'une slide (après sync), adopte la version Yjs si
  // elle existe, sinon seed le Y.Map depuis le backend. Une fois par slide.
  const reconciledSlides = useRef<Set<string>>(new Set())
  useEffect(() => {
    if (!collabReady || !activeSlideId) return
    const sid = activeSlideId
    const loaded = fullSlides[sid]
    if (!loaded || reconciledSlides.current.has(sid)) return
    reconciledSlides.current.add(sid)
    const remote = slidesYMap.get(sid)
    if (remote) {
      applyingRemoteRef.current = true
      setFullSlides(prev => ({ ...prev, [sid]: { ...prev[sid], elements: remote.elements, notes: remote.notes } }))
      if (sid === activeSlideIdRef.current) setNotes(remote.notes ?? '')
      applyingRemoteRef.current = false
    } else {
      pushSlideToYjs(sid, loaded.elements ?? [], loaded.notes ?? '')
    }
  }, [collabReady, activeSlideId, fullSlides, slidesYMap, pushSlideToYjs])

  // ── Migration ponctuelle : images base64 héritées → assets fichiers (kbfile) ──
  // À l'ouverture, on sort les data: URLs du document (qui gonflaient le snapshot
  // Yjs) vers des fichiers cachés. Réécrites en référence, elles disparaissent du
  // doc/.kbsld à la prochaine consolidation GC du core. Idempotent (une passe/ouverture).
  const migrationStartedRef = useRef(false)
  useEffect(() => {
    if (!id || !collabReady || migrationStartedRef.current || slides.length === 0) return
    migrationStartedRef.current = true
    let cancelled = false
    const isData = (s: unknown): s is string => typeof s === 'string' && s.startsWith('data:')
    const toRef = async (dataUrl: string, name: string): Promise<string | null> => {
      try {
        const blob = dataUrlToBlob(dataUrl)
        if (!blob) return null
        const { ref } = await presentationsApi.uploadAsset(id, blob, name)
        return ref
      } catch { return null }
    }
    void (async () => {
      for (const s of slides) {
        if (cancelled) break
        const sid = s.id
        const live = slidesYMap.get(sid)
        let full: Slide | undefined = live
          ? ({ elements: live.elements, notes: live.notes } as Slide)
          : undefined
        if (!full) { try { full = await presentationsApi.getSlide(id, sid) } catch { continue } }
        const els = full.elements ?? []
        let changed = false
        const newEls: SlideElement[] = []
        for (const el of els) {
          const sp = (el as ImageElement).storagePath
          if (el.type === 'image' && isData(sp)) {
            const ref = await toRef(sp, 'migrated')
            if (ref) { newEls.push({ ...el, storagePath: ref } as SlideElement); changed = true; continue }
          }
          newEls.push(el)
        }
        let bg = full.background
        if (bg && bg.type === 'image' && isData(bg.imagePath)) {
          const ref = await toRef(bg.imagePath, 'bg')
          if (ref) { bg = { ...bg, imagePath: ref }; changed = true }
        }
        if (changed && !cancelled) {
          setFullSlides(prev => ({ ...prev, [sid]: { ...(prev[sid] ?? full), elements: newEls, background: bg } as Slide }))
          pushSlideToYjs(sid, newEls, full.notes ?? '')
          try { await presentationsApi.updateSlide(id, sid, { elements: newEls }) } catch { /* ignore */ }
          if (bg !== full.background) { try { await presentationsApi.updateSlideMeta(id, sid, { background: bg }) } catch { /* ignore */ } }
        }
      }
    })()
    return () => { cancelled = true }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, collabReady, slides.length, slidesYMap, pushSlideToYjs])

  const theme = useMemo(() => pres?.theme ?? DEFAULT_THEME, [pres])

  const activeSlide = activeSlideId ? (fullSlides[activeSlideId] ?? null) : null
  const activeSlideIndex = slides.findIndex(s => s.id === activeSlideId)

  // Builds the live `Kubuno` API exposed to a macro (read-only for this first
  // version). Re-created on each run so it reads the current slide state.
  const makeApi = () => {
    // Plain text of all text elements on a slide (joined by line breaks).
    const slideText = (slide: Slide | null | undefined): string =>
      (slide?.elements ?? [])
        .filter((e): e is TextElement => e.type === 'text')
        .map(e => textElementPlainText(e))
        .filter(Boolean)
        .join('\n')
    const Slides = {
      /** Number of slides in the presentation. */
      count: () => slides.length,
      /** 1-based index of the active slide (0 if none). */
      getActiveIndex: () => (activeSlideIndex >= 0 ? activeSlideIndex + 1 : 0),
      /** Number of elements on the active slide. */
      getElementCount: () => activeSlide?.elements?.length ?? 0,
      /** Concatenated text of every loaded slide; falls back to the active slide. */
      getText: () => {
        const loaded = slides.map(s => fullSlides[s.id]).filter((s): s is Slide => !!s)
        const all = loaded.map(s => slideText(s)).filter(Boolean).join('\n\n')
        return all || slideText(activeSlide)
      },
    }
    const App = {
      getType: () => 'presentation',
      getId: () => id ?? '',
      toast: (m: unknown) => console.log(String(m)),
      log: (m: unknown) => console.log(String(m)),
    }
    return { Slides, App }
  }

  // Slide background: update local state immediately (live canvas), persist debounced.
  const bgSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const setSlideBg = useCallback((bg: SlideBackground) => {
    if (!activeSlideId || !id) return
    const sid = activeSlideId
    setFullSlides(prev => prev[sid] ? { ...prev, [sid]: { ...prev[sid], background: bg } } : prev)
    if (bgSaveTimer.current) clearTimeout(bgSaveTimer.current)
    bgSaveTimer.current = setTimeout(() => {
      presentationsApi.updateSlideMeta(id, sid, { background: bg }).catch(() => {})
    }, 350)
  }, [activeSlideId, id])

  // ── Mutations ────────────────────────────────────────────────────────────────

  const updatePresMut = useMutation({
    mutationFn: (data: { title?: string; is_starred?: boolean }) =>
      presentationsApi.update(id!, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentation', id] }),
  })

  // Corbeille (bouton standard WorkspaceShell). Présentation garde ses propres menus
  // riches (SlideMenuBar) en surcharge → pas de menus par défaut.
  const trashPresMut = useMutation({ mutationFn: () => presentationsApi.trash(id!), onSuccess: () => navigate('/office/presentations') })

  const updateSlideMut = useMutation({
    mutationFn: ({ sid, data }: { sid: string; data: { elements?: SlideElement[]; notes?: string } }) =>
      presentationsApi.updateSlide(id!, sid, data),
    // Pas de setFullSlides ici : l'état local est la source de vérité pendant
    // l'édition. Réappliquer l'écho (potentiellement périmé) faisait « trembloter »
    // les éléments en cours de déplacement quand la sauvegarde auto se déclenchait.
  })

  const addSlideMut = useMutation({
    mutationFn: (position?: number) => presentationsApi.createSlide(id!, position),
    onSuccess: async (slide) => {
      // Nouvelle diapositive → placeholders titre + sous-titre.
      try { await presentationsApi.updateSlide(id!, slide.id, { elements: placeholderSlideElements(t) }) } catch { /* ignore */ }
      await qc.invalidateQueries({ queryKey: ['presentation', id] })
      const full = await presentationsApi.getSlide(id!, slide.id)
      setFullSlides(prev => ({ ...prev, [full.id]: full }))
      setActiveSlideId(slide.id)
      setSelection([slide.id])
    },
  })

  const deleteSlideMut = useMutation({
    mutationFn: (sid: string) => presentationsApi.deleteSlide(id!, sid),
    onSuccess: (_, sid) => {
      setFullSlides(prev => {
        const next = { ...prev }
        delete next[sid]
        return next
      })
      qc.invalidateQueries({ queryKey: ['presentation', id] })
      if (activeSlideId === sid) {
        const remaining = slides.filter(s => s.id !== sid)
        const idx = slides.findIndex(s => s.id === sid)
        const next = remaining[Math.min(idx, remaining.length - 1)] ?? null
        setActiveSlideId(next ? next.id : null)
        setSelection(next ? [next.id] : [])
      }
    },
  })

  const duplicateSlideMut = useMutation({
    mutationFn: (sid: string) => presentationsApi.duplicateSlide(id!, sid),
    onSuccess: async (slide) => {
      await qc.invalidateQueries({ queryKey: ['presentation', id] })
      const full = await presentationsApi.getSlide(id!, slide.id)
      setFullSlides(prev => ({ ...prev, [full.id]: full }))
      setActiveSlideId(slide.id)
      setSelection([slide.id])
    },
  })

  const reorderMut = useMutation({
    mutationFn: (order: { id: string; position: number }[]) =>
      presentationsApi.reorderSlides(id!, order),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentation', id] }),
  })

  // ── Handlers ─────────────────────────────────────────────────────────────────

  // Applique des éléments à une diapositive (état + Yjs + sauvegarde différée) SANS
  // toucher à l'historique (utilisé par handleElementsChange et undo/redo).
  const commitElements = useCallback((sid: string, elements: SlideElement[]) => {
    setFullSlides(prev => ({ ...prev, [sid]: { ...(prev[sid] ?? {}), elements } as Slide }))
    pushSlideToYjs(sid, elements, fullSlides[sid]?.notes ?? '')
    pendingSlideRef.current = { sid, data: { elements } }
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    saveTimerRef.current = setTimeout(() => {
      if (pendingSlideRef.current) { updateSlideMut.mutate(pendingSlideRef.current); pendingSlideRef.current = null }
    }, 1000)
  }, [updateSlideMut, pushSlideToYjs, fullSlides])

  const handleElementsChange = useCallback((elements: SlideElement[]) => {
    if (!activeSlideId) return
    // Empile l'état PRÉCÉDENT (bursts d'un même geste coalescés sur 500 ms).
    const now = Date.now(), last = lastHistRef.current
    if (!(last.sid === activeSlideId && now - last.t < 500)) {
      undoRef.current.push({ sid: activeSlideId, els: fullSlides[activeSlideId]?.elements ?? [] })
      if (undoRef.current.length > 80) undoRef.current.shift()
      redoRef.current = []
    }
    lastHistRef.current = { sid: activeSlideId, t: now }
    commitElements(activeSlideId, elements)
  }, [activeSlideId, commitElements, fullSlides])

  // Précharge toutes les diapositives à l'ouverture du diaporama (sinon les non
  // encore chargées s'affichent vides).
  useEffect(() => {
    if (!presenterMode || !id) return
    for (const s of slides) if (!fullSlides[s.id]) presentationsApi.getSlide(id, s.id).then(f => setFullSlides(p => ({ ...p, [f.id]: f }))).catch(() => {})
  }, [presenterMode, id, slides, fullSlides])

  const undo = useCallback(() => {
    const entry = undoRef.current.pop(); if (!entry) return
    redoRef.current.push({ sid: entry.sid, els: fullSlides[entry.sid]?.elements ?? [] })
    lastHistRef.current = { sid: '', t: 0 }
    if (entry.sid !== activeSlideId) { setActiveSlideId(entry.sid); setSelection([entry.sid]) }
    commitElements(entry.sid, entry.els)
  }, [fullSlides, activeSlideId, commitElements])
  const redo = useCallback(() => {
    const entry = redoRef.current.pop(); if (!entry) return
    undoRef.current.push({ sid: entry.sid, els: fullSlides[entry.sid]?.elements ?? [] })
    lastHistRef.current = { sid: '', t: 0 }
    if (entry.sid !== activeSlideId) { setActiveSlideId(entry.sid); setSelection([entry.sid]) }
    commitElements(entry.sid, entry.els)
  }, [fullSlides, activeSlideId, commitElements])

  // Annuler / rétablir au clavier (hors saisie texte).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return
      const a = document.activeElement as HTMLElement | null
      if (a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.isContentEditable)) return
      if (e.key === 'z' || e.key === 'Z') { e.preventDefault(); if (e.shiftKey) redo(); else undo() }
      else if (e.key === 'y' || e.key === 'Y') { e.preventDefault(); redo() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [undo, redo])

  // Transition de la diapositive active (persistée + rendue au diaporama).
  const setSlideTransition = useCallback((type: string, duration?: number) => {
    if (!activeSlideId || !id) return
    const sid = activeSlideId
    const tr = { type, duration: duration ?? (fullSlides[sid]?.transition?.duration ?? 500) }
    setFullSlides(prev => prev[sid] ? { ...prev, [sid]: { ...prev[sid], transition: tr } } : prev)
    presentationsApi.updateSlideMeta(id, sid, { transition: tr }).catch(() => {})
  }, [activeSlideId, id, fullSlides])

  // ── Export PDF (rendu de chaque diapositive sur un canevas hors-écran) ─────────
  const handleExportPdf = useCallback(async () => {
    if (!id || !slides.length) return
    const datas: Slide[] = []
    for (const s of slides) {
      if (s.is_hidden) continue
      const f = fullSlides[s.id] ?? await presentationsApi.getSlide(id, s.id).catch(() => null)
      if (f) datas.push(f)
    }
    // Préchargement des images (sinon rendu vide pour les non encore chargées).
    const imgs: HTMLImageElement[] = []
    for (const d of datas) {
      for (const el of d.elements ?? []) if (el.type === 'image') imgs.push(resolveSlideImage((el as ImageElement).storagePath))
      if (d.background?.type === 'image' && d.background.imagePath) imgs.push(resolveSlideImage(d.background.imagePath))
    }
    await Promise.all(imgs.map(im => im.complete ? Promise.resolve() : new Promise<void>(res => { im.onload = () => res(); im.onerror = () => res(); setTimeout(res, 2500) })))
    const W = 1920, H = 1080
    const pages = datas.map(d => {
      const cv = document.createElement('canvas')
      const r = new SlideRenderer(cv, W, H)
      r.render({ background: d.background, elements: d.elements }, theme, { mode: 'thumbnail' })
      return { canvas: cv, wPx: W, hPx: H }
    })
    if (pages.length) downloadBlob(pagesToPdf(pages, pres?.title || 'presentation'), `${pres?.title || 'presentation'}.pdf`)
  }, [id, slides, fullSlides, theme, pres])

  const handleNotesChange = useCallback((value: string) => {
    setNotes(value)
    if (!activeSlideId) return
    pushSlideToYjs(activeSlideId, fullSlides[activeSlideId]?.elements ?? [], value)
    pendingNotesRef.current = { sid: activeSlideId, data: { notes: value } }
    if (notesTimerRef.current) clearTimeout(notesTimerRef.current)
    notesTimerRef.current = setTimeout(() => {
      if (pendingNotesRef.current) { updateSlideMut.mutate(pendingNotesRef.current); pendingNotesRef.current = null }
    }, 1000)
  }, [activeSlideId, updateSlideMut, pushSlideToYjs, fullSlides])

  // Vide les sauvegardes différées (avant de quitter / masquer / démonter).
  const flushPresSave = useCallback(() => {
    if (saveTimerRef.current) { clearTimeout(saveTimerRef.current); saveTimerRef.current = null }
    if (notesTimerRef.current) { clearTimeout(notesTimerRef.current); notesTimerRef.current = null }
    if (pendingSlideRef.current) { updateSlideMut.mutate(pendingSlideRef.current); pendingSlideRef.current = null }
    if (pendingNotesRef.current) { updateSlideMut.mutate(pendingNotesRef.current); pendingNotesRef.current = null }
  }, [updateSlideMut])

  useEffect(() => {
    const onHide = () => { if (document.visibilityState === 'hidden') flushPresSave() }
    document.addEventListener('visibilitychange', onHide)
    window.addEventListener('pagehide', flushPresSave)
    window.addEventListener('beforeunload', flushPresSave)
    return () => {
      document.removeEventListener('visibilitychange', onHide)
      window.removeEventListener('pagehide', flushPresSave)
      window.removeEventListener('beforeunload', flushPresSave)
      flushPresSave()
    }
  }, [flushPresSave])

  const handleReorderSlide = useCallback((dragIdx: number, dropIdx: number) => {
    const newSlides = [...slides]
    const [moved] = newSlides.splice(dragIdx, 1)
    newSlides.splice(dropIdx, 0, moved)
    const order = newSlides.map((s, i) => ({ id: s.id, position: i }))
    reorderMut.mutate(order)
  }, [slides, reorderMut])

  // ── Sélection (multi) de diapositives ────────────────────────────────────────
  const selectSlide = useCallback((sid: string, mods: { ctrl: boolean; shift: boolean }) => {
    let next: string[]
    if (mods.ctrl) {
      next = selection.includes(sid) ? selection.filter(x => x !== sid) : [...selection, sid]
    } else if (mods.shift && activeSlideId) {
      const a = slides.findIndex(s => s.id === activeSlideId)
      const b = slides.findIndex(s => s.id === sid)
      next = (a >= 0 && b >= 0)
        ? slides.slice(Math.min(a, b), Math.max(a, b) + 1).map(s => s.id)
        : [sid]
    } else {
      next = [sid]
    }
    setSelection(next)
    setActiveSlideId(next.includes(sid) ? sid : (next[next.length - 1] ?? null))
  }, [selection, activeSlideId, slides])

  const clearSelection = useCallback(() => { setSelection([]); setActiveSlideId(null) }, [])

  // Ids sélectionnés, dans l'ordre de la présentation.
  const orderedSelection = useCallback(
    () => slides.filter(s => selection.includes(s.id)).map(s => s.id),
    [slides, selection])

  const getFullSlide = useCallback(async (sid: string) => {
    const f = fullSlides[sid]
    if (f && f.elements) return f
    return id ? presentationsApi.getSlide(id, sid) : null
  }, [fullSlides, id])

  // ── Opérations groupées (menu contextuel) ────────────────────────────────────
  const handleNewSlideAfter = useCallback((sid: string | null) => {
    const idx = sid ? slides.findIndex(s => s.id === sid) : -1
    addSlideMut.mutate(idx >= 0 ? idx + 1 : undefined)
  }, [slides, addSlideMut])

  const handleCopySelected = useCallback(async () => {
    if (!id) return
    const ids = orderedSelection(); if (!ids.length) return
    const clips: NonNullable<typeof slideClipboardRef.current> = []
    for (const sid of ids) {
      try { const f = await getFullSlide(sid); if (f) clips.push({ elements: f.elements ?? [], background: f.background, notes: f.notes ?? '', transition: f.transition }) } catch { /* skip */ }
    }
    if (clips.length) { slideClipboardRef.current = clips; setCanPasteSlide(true) }
  }, [id, orderedSelection, getFullSlide])

  const handleDeleteSelected = useCallback(async () => {
    if (!id) return
    const ids = orderedSelection(); if (!ids.length) return
    const firstIdx = slides.findIndex(s => ids.includes(s.id))
    for (const sid of ids) { try { await presentationsApi.deleteSlide(id, sid) } catch { /* skip */ } }
    setFullSlides(prev => { const n = { ...prev }; ids.forEach(s => delete n[s]); return n })
    await qc.invalidateQueries({ queryKey: ['presentation', id] })
    const remaining = slides.filter(s => !ids.includes(s.id))
    const next = remaining[Math.min(firstIdx, remaining.length - 1)] ?? null
    setSelection(next ? [next.id] : [])
    setActiveSlideId(next ? next.id : null)
  }, [id, slides, orderedSelection, qc])

  const handleCutSelected = useCallback(async () => {
    await handleCopySelected()
    await handleDeleteSelected()
  }, [handleCopySelected, handleDeleteSelected])

  const handlePasteAfter = useCallback(async (afterSid: string | null) => {
    const clips = slideClipboardRef.current
    if (!clips || !clips.length || !id) return
    const idx = afterSid ? slides.findIndex(s => s.id === afterSid) : -1
    let pos = idx >= 0 ? idx + 1 : slides.length
    let lastId: string | null = null
    for (const clip of clips) {
      try {
        const summary = await presentationsApi.createSlide(id, pos)
        await presentationsApi.updateSlide(id, summary.id, { elements: clip.elements, notes: clip.notes })
        await presentationsApi.updateSlideMeta(id, summary.id, { background: clip.background, transition: clip.transition })
        lastId = summary.id; pos += 1
      } catch { /* skip */ }
    }
    await qc.invalidateQueries({ queryKey: ['presentation', id] })
    if (lastId) {
      const full = await presentationsApi.getSlide(id, lastId)
      setFullSlides(prev => ({ ...prev, [full.id]: full }))
      setActiveSlideId(lastId); setSelection([lastId])
    }
  }, [id, slides, qc])

  const handleDuplicateSelected = useCallback(async () => {
    if (!id) return
    const ids = orderedSelection(); if (!ids.length) return
    let lastId: string | null = null
    for (const sid of ids) { try { const s = await presentationsApi.duplicateSlide(id, sid); lastId = s.id } catch { /* skip */ } }
    await qc.invalidateQueries({ queryKey: ['presentation', id] })
    if (lastId) {
      const full = await presentationsApi.getSlide(id, lastId)
      setFullSlides(prev => ({ ...prev, [full.id]: full }))
      setActiveSlideId(lastId); setSelection([lastId])
    }
  }, [id, orderedSelection, qc])

  const handleToggleHiddenSelected = useCallback(async () => {
    if (!id) return
    const ids = orderedSelection(); if (!ids.length) return
    const anyVisible = slides.some(s => ids.includes(s.id) && !s.is_hidden)
    for (const sid of ids) { try { await presentationsApi.updateSlideMeta(id, sid, { is_hidden: anyVisible }) } catch { /* skip */ } }
    qc.invalidateQueries({ queryKey: ['presentation', id] })
  }, [id, slides, orderedSelection, qc])

  const handleEditBackground = useCallback((sid: string) => { setSelection([sid]); setActiveSlideId(sid) }, [])

  // Insère une image (data URL) dans la diapositive active, centrée sur (cx, cy).
  const imageInputRef = useRef<HTMLInputElement>(null)
  const insertImageFromFile = useCallback(async (file: File, cx = 0.5, cy = 0.5) => {
    if (!activeSlideId || !file.type.startsWith('image/')) return
    try {
      const { ref, w, h } = await uploadImageRef(file)
      const cur = fullSlides[activeSlideId]?.elements ?? []
      handleElementsChange([...cur, makeImageElement(ref, w, h, cx, cy, cur.length + 1)])
    } catch { /* ignore */ }
  }, [activeSlideId, fullSlides, handleElementsChange])

  // Met à jour l'élément sélectionné (barre d'outils contextuelle).
  const updateSelectedEl = useCallback((patch: Record<string, unknown>) => {
    if (!selectedEl || !activeSlideId) return
    const els = fullSlides[activeSlideId]?.elements ?? []
    handleElementsChange(els.map(e => e.id === selectedEl.id ? ({ ...e, ...patch } as SlideElement) : e))
  }, [selectedEl, activeSlideId, fullSlides, handleElementsChange])
  const replaceSelectedImage = useCallback((file: File | undefined) => {
    if (!file || !file.type.startsWith('image/')) return
    uploadImageRef(file).then(({ ref }) => updateSelectedEl({ storagePath: ref })).catch(() => {})
  }, [updateSelectedEl])

  const handleTitleSave = useCallback(() => {
    if (titleDraft && titleDraft !== pres?.title) {
      updatePresMut.mutate({ title: titleDraft })
    }
  }, [titleDraft, pres?.title, updatePresMut])

  // Champ titre TOUJOURS éditable (WorkspaceShell) : le brouillon suit le titre serveur.
  useEffect(() => { if (pres?.title != null) setTitleDraft(pres.title) }, [pres?.title])

  // Ouvre une présentation par id (route éditeur) depuis l'onglet « Fichier ».
  const openPresentationById = useCallback((pid: string) => navigate(`/office/presentations/${pid}`), [navigate])
  // Résout un fichier Drive vers une présentation et l'ouvre (onglet « Parcourir »).
  const openPresentationFile = useCallback((file: FileItem): boolean => {
    const meta = file.metadata as Record<string, unknown> | undefined
    const presId = meta?.office_presentation_id as string | undefined
    if (presId) { navigate(`/office/presentations/${presId}`); return true }
    if (file.mime_type !== 'application/vnd.oasis.opendocument.presentation') return false
    presentationsApi.openByFile(file.id)
      .then(p => navigate(`/office/presentations/${p.id}`))
      .catch(() => { /* silently ignore */ })
    return true
  }, [navigate])

  // Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du ruban.
  // Le hook doit être appelé avant tout return anticipé (loading).
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_PRESENTATION,
    labels: backstageLabels(t),
    startContent: <PresentationStartContent onOpen={openPresentationById} onOpenFile={openPresentationFile} />,
    defaultTab: 'home',
    doc: {
      info: (
        <InfoPanel
          title={pres?.title || t('common_untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('presentation_title', { defaultValue: 'Présentation' })}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('presentation_title', { defaultValue: 'Présentation' })],
            [t('presentations_slide_count', { count: pres?.slide_count ?? 0 }), pres?.slide_count ?? 0],
            ...(pres?.aspect_ratio ? [[t('pres_aspect_ratio', { defaultValue: 'Format' }), pres.aspect_ratio] as [string, string]] : []),
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: () => navigate('/office/presentations'),
    },
  })

  // ── Loading ───────────────────────────────────────────────────────────────────

  if (isLoading || !pres) {
    return (
      <div className="flex items-center justify-center h-full text-text-tertiary text-sm">
        {t('common_loading')}
      </div>
    )
  }

  const activeIdx = slides.findIndex(s => s.id === activeSlideId)

  const api = () => canvasApiRef.current
  const alignItem = (id: string, icon: React.ReactNode, label: string, mode: 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom') =>
    ({ id, kind: 'button' as const, icon, tooltip: label, onClick: () => api()?.align(mode) })
  const presRibbon: RibbonTab[] = [
    { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
      { id: 'hist', label: t('pres_grp_history', { defaultValue: 'Annuler' }), items: [
        { id: 'undo', kind: 'button', icon: <Undo2 size={15} />, label: t('pres_undo', { defaultValue: 'Annuler' }), onClick: undo },
        { id: 'redo', kind: 'button', icon: <Redo2 size={15} />, label: t('pres_redo', { defaultValue: 'Rétablir' }), onClick: redo },
      ] },
      // « Nouveau »/« Dupliquer » (jadis dans un groupe « Fichier » redondant) déplacés
      // ici ; les opérations sur le fichier vivent désormais dans le backstage (Fichier).
      { id: 'pfile', label: t('doc_grp_slide', { defaultValue: 'Diapositive' }), items: [
        { id: 'new', kind: 'button', icon: <FilePlus2 size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: () => handleNewSlideAfter(null) },
        { id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: handleDuplicateSelected },
        { id: 'pdf', kind: 'button', icon: <FileDown size={15} />, label: t('pres_export_pdf', { defaultValue: 'Exporter en PDF' }), onClick: handleExportPdf },
      ] },
      { id: 'show', label: t('pres_grp_view', { defaultValue: 'Affichage' }), items: [
        { id: 'slideshow', kind: 'button', size: 'large', icon: <Play size={22} />, label: t('pres_slideshow'), onClick: () => setPresenterMode(true) },
      ] },
    ] },
    { id: 'arrange', label: t('pres_tab_arrange', { defaultValue: 'Disposition' }), groups: [
      { id: 'align', label: t('pres_align', { defaultValue: 'Aligner' }), items: [
        alignItem('al', <AlignStartVertical size={15} />, t('pres_align_left', { defaultValue: 'Gauche' }), 'left'),
        alignItem('ac', <AlignCenterVertical size={15} />, t('pres_align_centerh', { defaultValue: 'Centrer (h)' }), 'center'),
        alignItem('ar', <AlignEndVertical size={15} />, t('pres_align_right', { defaultValue: 'Droite' }), 'right'),
        alignItem('at', <AlignStartHorizontal size={15} />, t('pres_align_top', { defaultValue: 'Haut' }), 'top'),
        alignItem('am', <AlignCenterHorizontal size={15} />, t('pres_align_middle', { defaultValue: 'Centrer (v)' }), 'middle'),
        alignItem('ab', <AlignEndHorizontal size={15} />, t('pres_align_bottom', { defaultValue: 'Bas' }), 'bottom'),
      ] },
      { id: 'dist', label: t('pres_distribute', { defaultValue: 'Répartir' }), items: [
        { id: 'dh', kind: 'button', icon: <AlignHorizontalDistributeCenter size={15} />, tooltip: t('pres_distribute_h', { defaultValue: 'Répartir horizontalement' }), onClick: () => api()?.distribute('h') },
        { id: 'dv', kind: 'button', icon: <AlignVerticalDistributeCenter size={15} />, tooltip: t('pres_distribute_v', { defaultValue: 'Répartir verticalement' }), onClick: () => api()?.distribute('v') },
      ] },
      { id: 'order', label: t('pres_arrange', { defaultValue: 'Ordre' }), items: [
        { id: 'zf', kind: 'button', size: 'large', icon: <BringToFront size={22} />, label: t('pres_z_front', { defaultValue: 'Premier plan' }), onClick: () => api()?.zorder('front') },
        { id: 'zb', kind: 'button', size: 'large', icon: <SendToBack size={22} />, label: t('pres_z_back', { defaultValue: 'Arrière-plan' }), onClick: () => api()?.zorder('back') },
        { id: 'zfw', kind: 'button', icon: <ChevronRight size={15} />, label: t('pres_z_forward', { defaultValue: 'Avancer' }), onClick: () => api()?.zorder('forward') },
        { id: 'zbw', kind: 'button', icon: <ChevronLeft size={15} />, label: t('pres_z_backward', { defaultValue: 'Reculer' }), onClick: () => api()?.zorder('backward') },
      ] },
      { id: 'objops', label: t('pres_grp_object', { defaultValue: 'Objet' }), items: [
        { id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('pres_ctx_duplicate', { defaultValue: 'Dupliquer' }), onClick: () => api()?.duplicate() },
        { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete', { defaultValue: 'Supprimer' }), onClick: () => api()?.remove() },
      ] },
    ] },
    { id: 'animtab', label: t('pres_tab_animations', { defaultValue: 'Animations' }), groups: [
      { id: 'anim', label: t('pres_animation', { defaultValue: 'Animation' }), items: [
        { id: 'an', kind: 'dropdown', icon: <Wand2 size={15} />, value: api()?.curAnim() ?? 'none', width: 180,
          options: PRES_ANIMATIONS.map(x => ({ value: x.type, label: t(x.nameKey, { defaultValue: x.label }) })),
          onChange: (v: string) => api()?.setAnim(v === 'none' ? null : { type: v }) },
      ] },
    ] },
    { id: 'transtab', label: t('pres_tab_transitions', { defaultValue: 'Transitions' }), groups: [
      { id: 'trans', label: t('pres_transition', { defaultValue: 'Transition' }), items: [
        { id: 'tr', kind: 'dropdown', icon: <Film size={15} />, value: activeSlide?.transition?.type ?? 'none', width: 160,
          options: PRES_TRANSITIONS.map(x => ({ value: x.type, label: t(x.nameKey, { defaultValue: x.label }) })),
          onChange: (v: string) => setSlideTransition(v) },
      ] },
    ] },
  ]

  return (
    <OfficeShell
      ribbon={[fileTab, ...presRibbon]}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_PRESENTATION}
      chromeless
      topbarHeight={64}
      onBack={() => navigate('/office/presentations')}
      titleIcon={<LayoutTemplate size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onTitleChange={setTitleDraft}
      onTitleCommit={handleTitleSave}
      titlePlaceholder={t('common_untitled')}
      saveStatus={updateSlideMut.isPending ? t('pres_saving') : t('doc_saved')}
      titleActions={
        <>
          <SaveButton
            onSave={flushPresSave}
            saving={updateSlideMut.isPending}
            label={t('doc_save', { defaultValue: 'Enregistrer' })}
          />
          <button
            onClick={() => updatePresMut.mutate({ is_starred: !pres.is_starred })}
            className={`p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 ${pres.is_starred ? 'text-warning' : 'text-white/90'}`}
            title={pres.is_starred ? t('pres_unstar', { defaultValue: 'Retirer des favoris' }) : t('pres_star', { defaultValue: 'Ajouter aux favoris' })}
          >
            <Star size={15} className={pres.is_starred ? 'fill-warning' : ''} />
          </button>
        </>
      }
      topbarActions={
        <div className="flex items-center gap-2">
          {activeIdx >= 0 && (
            <div className="flex items-center gap-1 text-xs text-white/90">
              <button disabled={activeIdx <= 0} onClick={() => { const s = slides[activeIdx - 1]; if (s) { setActiveSlideId(s.id); setSelection([s.id]) } }} className="disabled:opacity-30"><ChevronLeft size={16} /></button>
              <span>{activeIdx + 1} / {slides.length}</span>
              <button disabled={activeIdx >= slides.length - 1} onClick={() => { const s = slides[activeIdx + 1]; if (s) { setActiveSlideId(s.id); setSelection([s.id]) } }} className="disabled:opacity-30"><ChevronRight size={16} /></button>
            </div>
          )}
          <Button size="sm" icon={<Play size={14} />} onClick={() => setPresenterMode(true)}>{t('pres_slideshow')}</Button>
          <PresenceAvatarList users={presenceUsers} />
          <button onClick={() => setShareOpen(true)}
            className="flex items-center gap-1.5 h-8 px-3 rounded-full bg-primary text-white text-sm font-medium hover:bg-primary-hover transition-colors">
            <UserPlus size={15} /> {t('share_button', 'Partager')}
          </button>
        </div>
      }
      onDelete={() => trashPresMut.mutate()}
      deleteTitle={t('pres_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('pres_delete_confirm_title', { defaultValue: 'Supprimer cette présentation ?' }),
        message: t('pres_delete_confirm_msg', { defaultValue: 'La présentation sera déplacée dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
    >
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
        {/* Presenter Mode */}
        {presenterMode && (
          <PresenterMode
            slides={slides}
            fullSlides={fullSlides}
            theme={theme}
            startIndex={activeIdx >= 0 ? activeIdx : 0}
            onClose={() => setPresenterMode(false)}
          />
        )}

      {/* Barre de menus remplacée par le RUBAN (OfficeShell). Outils dessin = SlideToolbar. */}
      <SlideToolbar
        tool={tool} lineKind={lineKind} onToolChange={setTool} onLineKindChange={setLineKind}
        shapeKind={shapeKind} onShapeKindChange={setShapeKind}
        onPickImage={() => imageInputRef.current?.click()}
        selectedEl={selectedEl}
        onUpdateSelected={updateSelectedEl}
        onReplaceImage={() => replaceImgInputRef.current?.click()}
        onCrop={() => setCropSignal(s => s + 1)}
        macrosSlot={id ? (
          <MacrosMenu docType="presentation" docId={id} buildApi={makeApi} defaultLabel={pres?.title} />
        ) : null}
      />
      <input
        ref={imageInputRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={e => { const f = e.target.files?.[0]; if (f) insertImageFromFile(f); if (e.target) e.target.value = '' }}
      />
      <input
        ref={replaceImgInputRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={e => { replaceSelectedImage(e.target.files?.[0]); if (e.target) e.target.value = '' }}
      />

      {/* Editor body */}
      <div className="relative flex flex-1 overflow-hidden">
        {/* Left panel — slides (largeur redimensionnable) */}
        <div style={{ width: slidePanelW }} className="flex-shrink-0 bg-surface-1 overflow-hidden">
          <SlidePanel
            slides={slides}
            fullSlides={fullSlides}
            selection={selection}
            theme={theme}
            canPaste={canPasteSlide}
            slidePresence={slidePresence}
            onSelectSlide={selectSlide}
            onClearSelection={clearSelection}
            onAddSlide={() => addSlideMut.mutate(undefined)}
            onNewSlideAfter={handleNewSlideAfter}
            onReorderSlide={handleReorderSlide}
            onDeleteSelected={handleDeleteSelected}
            onDuplicateSelected={handleDuplicateSelected}
            onCopySelected={handleCopySelected}
            onCutSelected={handleCutSelected}
            onPasteAfter={handlePasteAfter}
            onToggleHiddenSelected={handleToggleHiddenSelected}
            onCreateImageSelected={handleDuplicateSelected}
            onEditBackground={handleEditBackground}
          />
        </div>
        <ResizeHandle
          position={slidePanelW}
          onResize={setSlidePanelW}
          min={120}
          max={360}
          onReset={() => setSlidePanelW(150)}
          title={t('pres_slide')}
        />

        {/* Center — canvas (ou état vide : aucune diapositive / aucune sélection) */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {activeSlide ? (
            <SlideCanvas
              slide={activeSlide}
              theme={theme}
              tool={tool}
              lineKind={lineKind}
              shapeKind={shapeKind}
              onElementsChange={handleElementsChange}
              onToolChange={setTool}
              onEditBackground={() => setBgEditorOpen(true)}
              onInsertImage={insertImageFromFile}
              onSelectionChange={setSelectedEl}
              cropSignal={cropSignal}
              remoteSelections={elementPresence}
              awareness={awareness}
              onApi={api => { canvasApiRef.current = api }}
            />
          ) : (
            <EmptySlideArea hasSlides={slides.length > 0} onAdd={() => addSlideMut.mutate(undefined)} />
          )}
          {activeSlide && <PresenterNotes notes={notes} onChange={handleNotesChange} />}
        </div>

        {/* Right panel */}
        <div className="w-48 flex-shrink-0 border-l border-border bg-white overflow-y-auto p-3">
          <p className="text-xs font-medium text-text-tertiary uppercase tracking-wide mb-3">
            {t('pres_slide')}
          </p>
          {activeSlide && (
            <>
              <div className="mb-3">
                <p className="text-xs text-text-secondary mb-1">{t('pres_position')}</p>
                <p className="text-sm text-text-primary">{t('pres_position_value', { index: activeSlideIndex + 1, total: slides.length })}</p>
              </div>
              <div className="mb-3">
                <p className="text-xs text-text-secondary mb-1">{t('pres_elements')}</p>
                <p className="text-sm text-text-primary">{t('pres_element_count', { count: activeSlide.elements?.length ?? 0 })}</p>
              </div>
              <div className="mb-3">
                <p className="text-xs text-text-secondary mb-1.5">{t('pres_background')}</p>
                <div className="flex items-center gap-2">
                  <ColorField width={28} height={22}
                    color={activeSlide.background?.type === 'color' ? (activeSlide.background.color ?? '#ffffff') : '#ffffff'}
                    onChange={hex => setSlideBg({ type: 'color', color: hex })} />
                  <GradientField width={52} height={22}
                    value={activeSlide.background?.grad ?? DEFAULT_GRADIENT}
                    onChange={g => setSlideBg({ type: 'gradient', grad: g })} />
                </div>
              </div>
              <div className="mb-3">
                <button
                  className="flex items-center gap-2 w-full text-xs text-text-secondary hover:text-danger transition-colors"
                  onClick={() => deleteSlideMut.mutate(activeSlide.id)}
                >
                  <Trash2 size={12} />
                  {t('pres_delete_slide')}
                </button>
              </div>
              <div className="mb-3">
                <button
                  className="flex items-center gap-2 w-full text-xs text-text-secondary hover:text-primary transition-colors"
                  onClick={() => duplicateSlideMut.mutate(activeSlide.id)}
                >
                  <Copy size={12} />
                  {t('common_duplicate')}
                </button>
              </div>
              <div>
                <button
                  className="flex items-center gap-2 w-full text-xs text-text-secondary hover:text-primary transition-colors"
                  onClick={() => {
                    const isHidden = !activeSlide.is_hidden
                    presentationsApi.updateSlideMeta(id!, activeSlide.id, { is_hidden: isHidden })
                      .then(slide => setFullSlides(prev => ({ ...prev, [slide.id]: slide })))
                  }}
                >
                  {activeSlide.is_hidden ? <Eye size={12} /> : <EyeOff size={12} />}
                  {activeSlide.is_hidden ? t('pres_show') : t('pres_hide')}
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Bottom status bar (Word-like) — slide position, element count, view mode. */}
      <StatusBar>
        <StatusButton title={t('pres_status_slide_title', { defaultValue: 'Diapositive active' })}>
          {t('pres_status_slide_n', {
            current: activeSlideIndex >= 0 ? activeSlideIndex + 1 : 0,
            total: slides.length,
            defaultValue: `Diapositive ${activeSlideIndex >= 0 ? activeSlideIndex + 1 : 0} sur ${slides.length}`,
          })}
        </StatusButton>
        {activeSlide && (
          <>
            <StatusSep />
            <StatusButton title={t('pres_status_elements_title', { defaultValue: 'Objets sur la diapositive' })}>
              {t('pres_status_elements', {
                count: activeSlide.elements?.length ?? 0,
                defaultValue: `${activeSlide.elements?.length ?? 0} objet(s)`,
              })}
            </StatusButton>
          </>
        )}
        <StatusSpacer />
        <StatusButton title={t('pres_status_mode_title', { defaultValue: 'Mode' })}>
          {presenterMode
            ? t('pres_status_mode_slideshow', { defaultValue: 'Diaporama' })
            : t('pres_status_mode_edit', { defaultValue: 'Édition' })}
        </StatusButton>
      </StatusBar>
      </div>
      {bgEditorOpen && activeSlide && (
        <>
          <div className="fixed inset-0 z-[9998]" onClick={() => setBgEditorOpen(false)} />
          <div className="fixed z-[9999] left-1/2 top-24 -translate-x-1/2 w-72 bg-white border border-border rounded-lg shadow-xl p-4">
            <p className="text-sm font-medium text-text-primary mb-3">{t('pres_ctx_background')}</p>
            <div className="mb-2">
              <p className="text-xs text-text-secondary mb-1.5">{t('pres_bg_color', { defaultValue: 'Couleur' })}</p>
              <ColorField width={40} height={26}
                color={activeSlide.background?.type === 'color' ? (activeSlide.background.color ?? '#ffffff') : '#ffffff'}
                onChange={hex => setSlideBg({ type: 'color', color: hex })} />
            </div>
            <div>
              <p className="text-xs text-text-secondary mb-1.5">{t('pres_bg_gradient', { defaultValue: 'Dégradé' })}</p>
              <GradientField width={80} height={26}
                value={activeSlide.background?.grad ?? DEFAULT_GRADIENT}
                onChange={g => setSlideBg({ type: 'gradient', grad: g })} />
            </div>
            <div className="mt-4 flex justify-end">
              <Button size="sm" onClick={() => setBgEditorOpen(false)}>{t('common_close', { defaultValue: 'Fermer' })}</Button>
            </div>
          </div>
        </>
      )}
      {shareOpen && id && (
        <CollaboratorsDialog
          entityId={id}
          cacheKey="pres-collab"
          title={t('share_title_pres', 'Partager la présentation')}
          onClose={() => setShareOpen(false)}
          api={{
            listCollaborators:  presentationsApi.listCollaborators,
            addCollaborator:    presentationsApi.addCollaborator,
            updateCollaborator: presentationsApi.updateCollaborator,
            removeCollaborator: presentationsApi.removeCollaborator,
            searchRecipients:   officeApi.searchRecipients,
          }}
        />
      )}
    </OfficeShell>
  )
}
