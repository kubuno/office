import { useCallback, useEffect, useRef, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useConfirm, DockArea, getDateLocale, type DockPanel, type DockController } from '@kubuno/sdk'
import { format } from 'date-fns'
import { ConfirmDialog } from '@ui'
import {
  ZoomIn, ZoomOut, RotateCcw, Plus, Trash2, Network,
  AlignLeft, AlignCenter, AlignRight, AlignStartVertical,
  AlignCenterVertical, AlignEndVertical,
  Search, Download,
  Pencil, ArrowLeftRight, Copy, ArrowUp, ArrowDown, Minus,
  Undo2, Redo2, Scissors, ClipboardPaste, Group, Ungroup,
  FlipHorizontal2, FlipVertical2, Grid3x3, Magnet, Maximize2,
  ArrowUpToLine, ArrowDownToLine, ChevronUp, ChevronDown,
  AlignHorizontalDistributeCenter, AlignVerticalDistributeCenter,
  Square, Circle, Diamond, Type, Hexagon, Triangle, Cloud, Database, Spline,
  Workflow, GitBranch, LayoutGrid, CircleDot, Map as MapIcon, ChevronRight,
  Layers, Eye, EyeOff, Lock, LockOpen, Ruler as RulerIcon, Upload, Palette,
} from 'lucide-react'
import { Dropdown, Button, Spinner, MenuDropdown, RangeSlider, FontPicker, type MenuItem } from '@ui'
import { diagramsApi } from './api'
import { toDrawioXml, fromDrawioXml, fromCsv, type IoData } from './diagramIo'
import { TEMPLATES } from './diagramTemplates'
import { buildJpegPdf } from './diagramPdf'
import { OfficeShell } from './shell/OfficeShell'
import { StatusBar, StatusButton, StatusSep, StatusSpacer, StatusZoom } from './shell/StatusBar'
import { THEME_DIAGRAMS, OFFICE_TONE } from './ribbon/officeThemes'
import { SaveButton } from './ribbon/SaveButton'
import { useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'
import { DiagramsStartContent } from './DiagramsStartContent'
import type { RibbonTab } from './ribbon/types'
import {
  renderShape, drawArrow, drawLabel, getCategories, getStencilsByCategory,
  searchStencils, mergeStyle, STENCIL_MAP, type ShapeStyle, type StencilDef,
} from './stencils'
import { onHwIconLoaded } from './hardwareIcons'
import { MacrosMenu } from './macros/MacrosMenu'

// ── Types ─────────────────────────────────────────────────────────────────────

interface LabelStyle {
  fontFamily:    string
  fontSize:      number
  bold:          boolean
  italic:        boolean
  color:         string
  align:         string
  verticalAlign: string
}

interface DiagramShape {
  id:         string
  type:       string
  x:          number
  y:          number
  w:          number
  h:          number
  label:      string
  style:      Partial<ShapeStyle>
  labelStyle: Partial<LabelStyle>
  zIndex:     number
  flipH?:     boolean
  flipV?:     boolean
  rotation?:  number   // degrees
  groupId?:   string | null
  layerId?:   string
}

interface LayerDef {
  id:      string
  name:    string
  visible: boolean
  locked:  boolean
}

interface ConnectorStyle {
  strokeColor: string
  strokeWidth: number
  strokeStyle: 'solid' | 'dashed' | 'dotted'
  arrowStart:  string
  arrowEnd:    string
  orthogonal:  boolean
  routing?:    'straight' | 'orthogonal' | 'curved'
}

interface DiagramConnector {
  id:          string
  sourceId:    string | null
  targetId:    string | null
  sourcePoint: { x: number; y: number } | null
  targetPoint: { x: number; y: number } | null
  waypoints:   { x: number; y: number }[]
  label:       string
  labelOffset?: { x: number; y: number } | null // décalage du label / milieu (drag)
  style:       ConnectorStyle
  layerId?:    string
}

interface DiagramData {
  shapes:     DiagramShape[]
  connectors: DiagramConnector[]
  layers?:    LayerDef[]
}

const DEFAULT_LAYER_ID = 'default'
function getLayers(d: DiagramData): LayerDef[] {
  return d.layers && d.layers.length ? d.layers : [{ id: DEFAULT_LAYER_ID, name: 'Calque 1', visible: true, locked: false }]
}

// Container shapes carry their geometric children when moved.
function isContainer(type: string) {
  return type === 'container' || type === 'swimlane_v' || type === 'swimlane_h'
}

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_LABEL_STYLE: LabelStyle = {
  fontFamily: 'Inter', fontSize: 12, bold: false, italic: false,
  color: '#000000', align: 'center', verticalAlign: 'middle',
}

const DEFAULT_CONN_STYLE: ConnectorStyle = {
  strokeColor: '#6c8ebf', strokeWidth: 1.5, strokeStyle: 'solid',
  arrowStart: 'none', arrowEnd: 'block', orthogonal: false, routing: 'straight',
}

// draw.io-style preset swatches: [fillColor, strokeColor].
const STYLE_PRESETS: Array<[string, string]> = [
  ['#dae8fc', '#6c8ebf'], ['#d5e8d4', '#82b366'], ['#ffe6cc', '#d79b00'], ['#fff2cc', '#d6b656'],
  ['#f8cecc', '#b85450'], ['#e1d5e7', '#9673a6'], ['#f5f5f5', '#666666'], ['#ffffff', '#000000'],
  ['none', '#000000'], ['#1a73e8', '#1557b0'], ['#0b5394', '#073763'], ['#202124', '#000000'],
]

const FONT_FAMILIES = ['Inter', 'Arial', 'Helvetica', 'Times New Roman', 'Georgia', 'Courier New', 'Verdana', 'Comic Sans MS']

const HANDLE_R   = 5   // resize handle radius (world px)
const PORT_R     = 5   // port point radius
const MIN_ZOOM   = 0.1
const MAX_ZOOM   = 4

function makeId() {
  return Math.random().toString(36).slice(2, 10)
}

// ── Coordinate helpers ────────────────────────────────────────────────────────

function canvasToWorld(cx: number, cy: number, panX: number, panY: number, zoom: number) {
  return { x: (cx - panX) / zoom, y: (cy - panY) / zoom }
}

function worldToCanvas(wx: number, wy: number, panX: number, panY: number, zoom: number) {
  return { x: wx * zoom + panX, y: wy * zoom + panY }
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

// Ordre des ancres : [haut, droite, bas, gauche].
function getPortPoints(s: DiagramShape) {
  return [
    { x: s.x + s.w / 2, y: s.y },
    { x: s.x + s.w, y: s.y + s.h / 2 },
    { x: s.x + s.w / 2, y: s.y + s.h },
    { x: s.x, y: s.y + s.h / 2 },
  ]
}

function shapeCenter(s: DiagramShape) {
  return { x: s.x + s.w / 2, y: s.y + s.h / 2 }
}

// Point d'ancrage « le plus logique » d'une forme vers un point cible : on sort par
// le côté dont la direction domine (Δx vs Δy) — droite/gauche si l'écart horizontal
// l'emporte, sinon bas/haut.
function anchorToward(s: DiagramShape, toward: { x: number; y: number }) {
  const c = shapeCenter(s)
  const dx = toward.x - c.x, dy = toward.y - c.y
  const ports = getPortPoints(s) // [haut, droite, bas, gauche]
  if (Math.abs(dx) >= Math.abs(dy)) return dx >= 0 ? ports[1] : ports[3]
  return dy >= 0 ? ports[2] : ports[0]
}

// Marge (px monde) dont la boîte de redimensionnement déborde la forme. Évite que
// les poignées de milieu de bord (n/e/s/w) tombent sur les points d'ancrage (ports),
// eux situés sur le bord de la forme.
const HANDLE_MARGIN = 14

function getResizeHandles(s: DiagramShape, margin = 0) {
  const m = margin
  const x0 = s.x - m, y0 = s.y - m, x1 = s.x + s.w + m, y1 = s.y + s.h + m
  const cx = s.x + s.w / 2, cy = s.y + s.h / 2
  return [
    { x: x0, y: y0, cursor: 'nw-resize', type: 'nw' as const },
    { x: cx, y: y0, cursor: 'n-resize',  type: 'n'  as const },
    { x: x1, y: y0, cursor: 'ne-resize', type: 'ne' as const },
    { x: x1, y: cy, cursor: 'e-resize',  type: 'e'  as const },
    { x: x1, y: y1, cursor: 'se-resize', type: 'se' as const },
    { x: cx, y: y1, cursor: 's-resize',  type: 's'  as const },
    { x: x0, y: y1, cursor: 'sw-resize', type: 'sw' as const },
    { x: x0, y: cy, cursor: 'w-resize',  type: 'w'  as const },
  ]
}

function getShapeAt(shapes: DiagramShape[], wx: number, wy: number): DiagramShape | null {
  for (let i = shapes.length - 1; i >= 0; i--) {
    const s = shapes[i]
    let px = wx, py = wy
    if (s.rotation) {
      // Rotate the click point into the shape's local (unrotated) frame.
      const cx = s.x + s.w / 2, cy = s.y + s.h / 2
      const a = (-s.rotation * Math.PI) / 180
      const dx = wx - cx, dy = wy - cy
      px = cx + dx * Math.cos(a) - dy * Math.sin(a)
      py = cy + dx * Math.sin(a) + dy * Math.cos(a)
    }
    if (px >= s.x && px <= s.x + s.w && py >= s.y && py <= s.y + s.h) return s
  }
  return null
}

function getHandleAt(shapes: DiagramShape[], selectedIds: Set<string>, wx: number, wy: number, margin = 0) {
  const r = HANDLE_R
  for (const sid of selectedIds) {
    const s = shapes.find((sh) => sh.id === sid)
    if (!s) continue
    for (const h of getResizeHandles(s, margin)) {
      if (Math.abs(wx - h.x) <= r && Math.abs(wy - h.y) <= r) {
        return { shapeId: sid, handleType: h.type, cursor: h.cursor }
      }
    }
  }
  return null
}

function getPortAt(shapes: DiagramShape[], wx: number, wy: number) {
  const r = PORT_R + 4
  for (const s of shapes) {
    const pts = getPortPoints(s)
    for (let i = 0; i < pts.length; i++) {
      const p = pts[i]
      if (Math.abs(wx - p.x) <= r && Math.abs(wy - p.y) <= r) {
        return { shapeId: s.id, portIdx: i, x: p.x, y: p.y }
      }
    }
  }
  return null
}

// Grid snapping. `GRID_SNAP` is a module-level mutable kept in sync by the editor
// (toggled from the View tab): 1 = free positioning (pixel rounding), >1 = snap to
// that grid step. Module-level because the geometry helpers below are pure.
let GRID_SNAP = 1
function setGridSnap(step: number) { GRID_SNAP = step > 1 ? step : 1 }
function snapToGrid(v: number) {
  return GRID_SNAP > 1 ? Math.round(v / GRID_SNAP) * GRID_SNAP : Math.round(v)
}

function resolveConnectorEndpoints(conn: DiagramConnector, shapes: DiagramShape[]) {
  const srcShape = conn.sourceId ? shapes.find((s) => s.id === conn.sourceId) : null
  const tgtShape = conn.targetId ? shapes.find((s) => s.id === conn.targetId) : null
  const wps = conn.waypoints ?? []
  // Une extrémité liée à une forme vise toujours l'ancre du bord la plus logique :
  // celle qui fait face au point suivant du tracé (1er/dernier waypoint, sinon le
  // centre de la forme opposée). Recalculé à chaque rendu → re-branchement auto.
  const srcToward = wps.length ? wps[0]
    : (tgtShape ? shapeCenter(tgtShape) : conn.targetPoint ?? { x: 0, y: 0 })
  const tgtToward = wps.length ? wps[wps.length - 1]
    : (srcShape ? shapeCenter(srcShape) : conn.sourcePoint ?? { x: 0, y: 0 })
  const from = srcShape ? anchorToward(srcShape, srcToward) : conn.sourcePoint ?? { x: 0, y: 0 }
  const to   = tgtShape ? anchorToward(tgtShape, tgtToward) : conn.targetPoint ?? { x: 0, y: 0 }
  return { from, to }
}

// Points de la polyligne d'un connecteur : [départ, ...waypoints, arrivée].
function connectorPoints(conn: DiagramConnector, shapes: DiagramShape[]) {
  const { from, to } = resolveConnectorEndpoints(conn, shapes)
  return conn.waypoints?.length ? [from, ...conn.waypoints, to] : [from, to]
}

// Routing mode of a connector (legacy `orthogonal` boolean still honoured).
function connRouting(conn: DiagramConnector): 'straight' | 'orthogonal' | 'curved' {
  return conn.style.routing ?? (conn.style.orthogonal ? 'orthogonal' : 'straight')
}

// Insert axis-aligned elbows so the path is only made of horizontal/vertical
// segments. Elbow orientation follows the dominant axis of each segment.
function orthogonalize(base: { x: number; y: number }[]): { x: number; y: number }[] {
  if (base.length < 2) return base
  const out = [base[0]]
  for (let i = 0; i < base.length - 1; i++) {
    const a = out[out.length - 1], b = base[i + 1]
    const dx = b.x - a.x, dy = b.y - a.y
    if (Math.abs(dx) > 1 && Math.abs(dy) > 1) {
      if (Math.abs(dx) >= Math.abs(dy)) out.push({ x: b.x, y: a.y }) // H then V
      else out.push({ x: a.x, y: b.y })                              // V then H
    }
    out.push(b)
  }
  // Drop near-duplicate consecutive points.
  return out.filter((p, i, arr) => i === 0 || Math.abs(p.x - arr[i - 1].x) > 0.01 || Math.abs(p.y - arr[i - 1].y) > 0.01)
}

function catmullRom(p0: { x: number; y: number }, p1: { x: number; y: number }, p2: { x: number; y: number }, p3: { x: number; y: number }, t: number) {
  const t2 = t * t, t3 = t2 * t
  return {
    x: 0.5 * ((2 * p1.x) + (-p0.x + p2.x) * t + (2 * p0.x - 5 * p1.x + 4 * p2.x - p3.x) * t2 + (-p0.x + 3 * p1.x - 3 * p2.x + p3.x) * t3),
    y: 0.5 * ((2 * p1.y) + (-p0.y + p2.y) * t + (2 * p0.y - 5 * p1.y + 4 * p2.y - p3.y) * t2 + (-p0.y + 3 * p1.y - 3 * p2.y + p3.y) * t3),
  }
}

// Smooth Catmull-Rom sampling through the control points (needs ≥3 points; with
// just endpoints the connector stays straight).
function sampleCurve(pts: { x: number; y: number }[], per = 16): { x: number; y: number }[] {
  if (pts.length < 3) return pts
  const out = [pts[0]]
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] ?? pts[i], p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2] ?? pts[i + 1]
    for (let t = 1; t <= per; t++) out.push(catmullRom(p0, p1, p2, p3, t / per))
  }
  return out
}

// The polyline actually drawn / hit-tested for a connector (routing applied).
function displayPoints(conn: DiagramConnector, shapes: DiagramShape[]) {
  const base = connectorPoints(conn, shapes)
  const r = connRouting(conn)
  if (r === 'orthogonal') return orthogonalize(base)
  if (r === 'curved') return sampleCurve(base, 16)
  return base
}

// Centre (monde) du label d'un connecteur : milieu du segment central + décalage.
function connectorLabelCenter(conn: DiagramConnector, shapes: DiagramShape[]) {
  const pts = displayPoints(conn, shapes)
  const si = Math.floor((pts.length - 1) / 2)
  const mid = { x: (pts[si].x + pts[si + 1].x) / 2, y: (pts[si].y + pts[si + 1].y) / 2 }
  const off = conn.labelOffset ?? { x: 0, y: 0 }
  return { x: mid.x + off.x, y: mid.y + off.y }
}

// Intersection STRICTEMENT intérieure de deux segments [a,b] et [c,d] (null sinon /
// si parallèles / si le croisement est à une extrémité). Sert aux « sauts de ligne ».
function segIntersect(
  a: { x: number; y: number }, b: { x: number; y: number },
  c: { x: number; y: number }, d: { x: number; y: number },
): { x: number; y: number } | null {
  const r1x = b.x - a.x, r1y = b.y - a.y
  const r2x = d.x - c.x, r2y = d.y - c.y
  const den = r1x * r2y - r1y * r2x
  if (Math.abs(den) < 1e-9) return null
  const t = ((c.x - a.x) * r2y - (c.y - a.y) * r2x) / den
  const u = ((c.x - a.x) * r1y - (c.y - a.y) * r1x) / den
  const eps = 1e-3
  if (t <= eps || t >= 1 - eps || u <= eps || u >= 1 - eps) return null
  return { x: a.x + t * r1x, y: a.y + t * r1y }
}

function distToSeg(px: number, py: number, ax: number, ay: number, bx: number, by: number) {
  const dx = bx - ax, dy = by - ay
  const len2 = dx * dx + dy * dy
  let t = len2 ? ((px - ax) * dx + (py - ay) * dy) / len2 : 0
  t = Math.max(0, Math.min(1, t))
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy))
}

// Connecteur sous le point (tolérance en px monde) + index de segment.
function getConnectorAt(conns: DiagramConnector[], shapes: DiagramShape[], wx: number, wy: number, tol: number): { id: string; seg: number } | null {
  for (let i = conns.length - 1; i >= 0; i--) {
    const pts = displayPoints(conns[i], shapes)
    for (let s = 0; s < pts.length - 1; s++) {
      if (distToSeg(wx, wy, pts[s].x, pts[s].y, pts[s + 1].x, pts[s + 1].y) <= tol) return { id: conns[i].id, seg: s }
    }
  }
  return null
}

// Aimante la position d'un nœud déplacé vers une équerre ALIGNÉE SUR LES AXES :
// uniquement les configurations où une portion devient horizontale et l'autre
// verticale. Deux équerres possibles autour de W (voisins P et N) :
//   E1 : P→W horizontale + W→N verticale   → W = (N.x, P.y)
//   E2 : P→W verticale   + W→N horizontale  → W = (P.x, N.y)
// On ne cale que si les DEUX portions sont déjà à moins de `thresholdDeg` de leur axe.
function magnetizeRightAngle(
  W: { x: number; y: number },
  P: { x: number; y: number },
  N: { x: number; y: number },
  thresholdDeg: number,
): { x: number; y: number; snapped: boolean } {
  const deg = (r: number) => (r * 180) / Math.PI
  // Écarts angulaires : à l'horizontale = atan2(|Δy|,|Δx|) ; à la verticale = atan2(|Δx|,|Δy|).
  const ah1 = deg(Math.atan2(Math.abs(W.y - P.y), Math.abs(W.x - P.x))) // P→W vs horizontale
  const av1 = deg(Math.atan2(Math.abs(W.x - N.x), Math.abs(W.y - N.y))) // W→N vs verticale
  const av2 = deg(Math.atan2(Math.abs(W.x - P.x), Math.abs(W.y - P.y))) // P→W vs verticale
  const ah2 = deg(Math.atan2(Math.abs(W.y - N.y), Math.abs(W.x - N.x))) // W→N vs horizontale
  const ok1 = ah1 <= thresholdDeg && av1 <= thresholdDeg
  const ok2 = av2 <= thresholdDeg && ah2 <= thresholdDeg
  if (!ok1 && !ok2) return { x: W.x, y: W.y, snapped: false }
  const e1 = { x: N.x, y: P.y }
  const e2 = { x: P.x, y: N.y }
  let target = e1
  if (ok1 && ok2) {
    const d1 = Math.hypot(W.x - e1.x, W.y - e1.y)
    const d2 = Math.hypot(W.x - e2.x, W.y - e2.y)
    target = d1 <= d2 ? e1 : e2
  } else {
    target = ok1 ? e1 : e2
  }
  return { x: target.x, y: target.y, snapped: true }
}

// ── Auto-layout ───────────────────────────────────────────────────────────────
// Pure: returns a Map<shapeId,{x,y}> of new positions. Connectors are treated as
// directed edges (source→target). `ids` limits the layout to a selection.
type LayoutKind = 'hier_tb' | 'hier_lr' | 'tree' | 'circle' | 'grid'

function computeLayout(
  kind: LayoutKind,
  shapes: DiagramShape[],
  conns: DiagramConnector[],
  ids: Set<string> | null,
): Map<string, { x: number; y: number }> {
  const nodes = shapes.filter((s) => !ids || ids.has(s.id))
  const pos = new Map<string, { x: number; y: number }>()
  if (!nodes.length) return pos
  const baseX = 80, baseY = 80

  if (kind === 'circle') {
    const r = Math.max(140, (nodes.length * 50) / (2 * Math.PI))
    const cx = baseX + r, cy = baseY + r
    nodes.forEach((n, i) => {
      const a = (i / nodes.length) * 2 * Math.PI - Math.PI / 2
      pos.set(n.id, { x: Math.round(cx + r * Math.cos(a) - n.w / 2), y: Math.round(cy + r * Math.sin(a) - n.h / 2) })
    })
    return pos
  }

  if (kind === 'grid') {
    const cols = Math.ceil(Math.sqrt(nodes.length))
    const cw = Math.max(...nodes.map((n) => n.w)) + 40
    const ch = Math.max(...nodes.map((n) => n.h)) + 40
    nodes.forEach((n, i) => { pos.set(n.id, { x: baseX + (i % cols) * cw, y: baseY + Math.floor(i / cols) * ch }) })
    return pos
  }

  // Hierarchical (layered). 'tree' is an alias of top-bottom hierarchical.
  const horizontal = kind === 'hier_lr'
  const idset = new Set(nodes.map((n) => n.id))
  const edges = conns.filter((c) => c.sourceId && c.targetId && idset.has(c.sourceId) && idset.has(c.targetId))
  const level = new Map(nodes.map((n) => [n.id, 0]))
  // Longest-path layering via relaxation (cycle-safe: bounded iterations).
  let changed = true, iter = 0
  while (changed && iter++ < nodes.length + 2) {
    changed = false
    for (const e of edges) {
      const nl = (level.get(e.sourceId!) ?? 0) + 1
      if (nl > (level.get(e.targetId!) ?? 0)) { level.set(e.targetId!, nl); changed = true }
    }
  }
  const byLevel = new Map<number, DiagramShape[]>()
  for (const n of nodes) {
    const l = level.get(n.id) ?? 0
    if (!byLevel.has(l)) byLevel.set(l, [])
    byLevel.get(l)!.push(n)
  }
  const GAP_MAIN = 90, GAP_CROSS = 50
  let cursorMain = horizontal ? baseX : baseY
  for (const l of [...byLevel.keys()].sort((a, b) => a - b)) {
    const arr = byLevel.get(l)!
    const mainSize = Math.max(...arr.map((n) => (horizontal ? n.w : n.h)))
    let cross = horizontal ? baseY : baseX
    for (const n of arr) {
      pos.set(n.id, horizontal ? { x: cursorMain, y: cross } : { x: cross, y: cursorMain })
      cross += (horizontal ? n.h : n.w) + GAP_CROSS
    }
    cursorMain += mainSize + GAP_MAIN
  }
  return pos
}

// ── Canvas renderer ───────────────────────────────────────────────────────────

function renderConnector(
  ctx: CanvasRenderingContext2D,
  conn: DiagramConnector,
  shapes: DiagramShape[],
  selected: boolean,
  greenSegs?: number[],
  hops?: { x: number; y: number }[][], // points de croisement par segment (sauts de ligne)
) {
  const { from, to } = resolveConnectorEndpoints(conn, shapes)
  // Apply routing: straight = control polyline, orthogonal = axis-aligned elbows
  // (the corner-rounding below turns them into rounded corners), curved = a dense
  // Catmull-Rom sampling (short segments ≈ a smooth curve through the rounding code).
  const base = conn.waypoints?.length > 0 ? [from, ...conn.waypoints, to] : [from, to]
  const routing = connRouting(conn)
  const pts = routing === 'orthogonal' ? orthogonalize(base) : routing === 'curved' ? sampleCurve(base, 16) : base

  ctx.save()
  ctx.strokeStyle = selected ? '#1a73e8' : conn.style.strokeColor
  ctx.lineWidth = conn.style.strokeWidth
  ctx.setLineDash(
    conn.style.strokeStyle === 'dashed' ? [6, 3] :
    conn.style.strokeStyle === 'dotted' ? [2, 3] : [],
  )

  // Polyligne avec coudes arrondis (petit rayon de raccordement à chaque nœud) et
  // « sauts de ligne » (petit arc qui enjambe) aux croisements avec des connecteurs
  // plus anciens.
  const BEND_R = 8 // rayon d'arrondi des coudes (unités monde)
  const HOP_R  = 5 // rayon du saut de ligne (unités monde)
  const dist = (p: { x: number; y: number }, q: { x: number; y: number }) => Math.hypot(q.x - p.x, q.y - p.y)
  const rad = pts.map(() => 0)
  for (let i = 1; i < pts.length - 1; i++) {
    rad[i] = Math.min(BEND_R, dist(pts[i - 1], pts[i]) / 2, dist(pts[i], pts[i + 1]) / 2)
  }
  ctx.beginPath()
  ctx.moveTo(pts[0].x, pts[0].y)
  for (let s = 0; s < pts.length - 1; s++) {
    const p0 = pts[s], p1 = pts[s + 1]
    const segLen = dist(p0, p1) || 1
    const dx = (p1.x - p0.x) / segLen, dy = (p1.y - p0.y) / segLen
    const startCut = s > 0 ? rad[s] : 0
    const endCut   = (s + 1 < pts.length - 1) ? rad[s + 1] : 0
    const aPt = { x: p0.x + dx * startCut, y: p0.y + dy * startCut } // début partie droite
    const bPt = { x: p1.x - dx * endCut,   y: p1.y - dy * endCut }   // fin partie droite
    const straight = dist(aPt, bPt)
    // sauts de ligne sur la portion droite, ordonnés le long du segment
    const segHops = (hops?.[s] ?? [])
      .map((P) => ({ P, t: (P.x - aPt.x) * dx + (P.y - aPt.y) * dy }))
      .filter((h) => h.t > HOP_R && h.t < straight - HOP_R)
      .sort((u, v) => u.t - v.t)
    for (const h of segHops) {
      const A = { x: aPt.x + dx * (h.t - HOP_R), y: aPt.y + dy * (h.t - HOP_R) }
      const B = { x: aPt.x + dx * (h.t + HOP_R), y: aPt.y + dy * (h.t + HOP_R) }
      ctx.lineTo(A.x, A.y)
      const a0 = Math.atan2(A.y - h.P.y, A.x - h.P.x)
      const a1 = Math.atan2(B.y - h.P.y, B.x - h.P.x)
      ctx.arc(h.P.x, h.P.y, HOP_R, a0, a1, true)
    }
    ctx.lineTo(bPt.x, bPt.y)
    // arrondi du coin au sommet p1 (si interne)
    if (s + 1 < pts.length - 1) {
      const p2 = pts[s + 2]
      const l2 = dist(p1, p2) || 1
      const cEnd = { x: p1.x + ((p2.x - p1.x) / l2) * rad[s + 1], y: p1.y + ((p2.y - p1.y) / l2) * rad[s + 1] }
      ctx.arcTo(p1.x, p1.y, cEnd.x, cEnd.y, rad[s + 1])
    }
  }
  ctx.stroke()
  ctx.setLineDash([])

  // Surlignage vert provisoire des portions venant de s'aimanter à 90° (pendant le
  // déplacement d'un nœud / d'une portion). Tracé par-dessus le trait normal.
  if (greenSegs && greenSegs.length) {
    ctx.save()
    ctx.strokeStyle = '#1e8e3e'
    ctx.lineWidth = conn.style.strokeWidth + 1
    ctx.lineCap = 'round'
    for (const s of greenSegs) {
      if (s < 0 || s + 1 >= pts.length) continue
      ctx.beginPath()
      ctx.moveTo(pts[s].x, pts[s].y)
      ctx.lineTo(pts[s + 1].x, pts[s + 1].y)
      ctx.stroke()
    }
    ctx.restore()
  }

  // Arrow ends
  if (pts.length >= 2) {
    const last = pts[pts.length - 1]
    const prev = pts[pts.length - 2]
    if (conn.style.arrowEnd && conn.style.arrowEnd !== 'none') {
      drawArrow(ctx, prev, last, conn.style.arrowEnd,
        selected ? '#1a73e8' : conn.style.strokeColor, conn.style.strokeWidth)
    }
    if (conn.style.arrowStart && conn.style.arrowStart !== 'none') {
      drawArrow(ctx, pts[1], pts[0], conn.style.arrowStart,
        selected ? '#1a73e8' : conn.style.strokeColor, conn.style.strokeWidth)
    }
  }

  // Label (milieu du segment central + décalage repositionnable par glisser-déposer)
  if (conn.label) {
    const mid = connectorLabelCenter(conn, shapes)
    ctx.font = '12px Inter, sans-serif'
    const fm = ctx.measureText(conn.label)
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(mid.x - fm.width / 2 - 4, mid.y - 9, fm.width + 8, 18)
    drawLabel(ctx, conn.label, mid.x - 40, mid.y - 10, 80, 20, DEFAULT_LABEL_STYLE)
  }

  ctx.restore()
}

function renderCanvas(
  canvas: HTMLCanvasElement,
  data: DiagramData,
  zoom: number, panX: number, panY: number,
  selectedIds: Set<string>,
  selectedConnIds: Set<string>,
  hoveredShapeId: string | null,
  drawingConn: { startX: number; startY: number; currentX: number; currentY: number } | null,
  lasso: { x1: number; y1: number; x2: number; y2: number } | null,
  bgColor: string,
  magnetSegs: { connId: string; segs: number[] }[] | null,
  showGrid: boolean,
  gridSize: number,
  alignGuides: { v: number[]; h: number[] } | null,
) {
  const dpr = window.devicePixelRatio || 1
  const ctx = canvas.getContext('2d')!
  const W = canvas.width / dpr
  const H = canvas.height / dpr

  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.save()
  ctx.scale(dpr, dpr)

  // Background
  ctx.fillStyle = bgColor || '#ffffff'
  ctx.fillRect(0, 0, W, H)

  // Grid (drawn in screen space at the zoomed step; minor + major lines like draw.io)
  if (showGrid && gridSize > 0) {
    const step = gridSize * zoom
    if (step >= 4) {
      const startX = panX % step
      const startY = panY % step
      ctx.save()
      ctx.lineWidth = 1
      ctx.strokeStyle = 'rgba(0,0,0,0.05)'
      ctx.beginPath()
      for (let x = startX; x < W; x += step) { ctx.moveTo(x, 0); ctx.lineTo(x, H) }
      for (let y = startY; y < H; y += step) { ctx.moveTo(0, y); ctx.lineTo(W, y) }
      ctx.stroke()
      // Major grid every 5 cells
      const major = step * 5
      const mStartX = panX % major
      const mStartY = panY % major
      ctx.strokeStyle = 'rgba(0,0,0,0.10)'
      ctx.beginPath()
      for (let x = mStartX; x < W; x += major) { ctx.moveTo(x, 0); ctx.lineTo(x, H) }
      for (let y = mStartY; y < H; y += major) { ctx.moveTo(0, y); ctx.lineTo(W, y) }
      ctx.stroke()
      ctx.restore()
    }
  }

  ctx.save()
  ctx.translate(panX, panY)
  ctx.scale(zoom, zoom)

  // Connectors — précalcul des « sauts de ligne » : pour chaque connecteur, les points
  // où il croise un connecteur PLUS ANCIEN (indice inférieur). Le plus récent enjambe.
  const polys = data.connectors.map((c) => displayPoints(c, data.shapes))
  for (let i = 0; i < data.connectors.length; i++) {
    const conn = data.connectors[i]
    const pi = polys[i]
    const hops: { x: number; y: number }[][] = []
    for (let s = 0; s < pi.length - 1; s++) {
      const crossings: { x: number; y: number }[] = []
      for (let j = 0; j < i; j++) {
        const pj = polys[j]
        for (let t = 0; t < pj.length - 1; t++) {
          const X = segIntersect(pi[s], pi[s + 1], pj[t], pj[t + 1])
          if (X) crossings.push(X)
        }
      }
      hops[s] = crossings
    }
    const green = magnetSegs?.find((m) => m.connId === conn.id)?.segs
    renderConnector(ctx, conn, data.shapes, selectedConnIds.has(conn.id), green, hops)
  }

  // Poignées d'édition des nœuds du connecteur sélectionné : waypoints (pleins,
  // déplaçables / double-clic = retirer) + milieux de segments (creux = ajouter).
  for (const conn of data.connectors) {
    if (!selectedConnIds.has(conn.id)) continue
    const pts = connectorPoints(conn, data.shapes)
    // poignées d'ajout (milieu de chaque segment)
    for (let s = 0; s < pts.length - 1; s++) {
      const mx = (pts[s].x + pts[s + 1].x) / 2, my = (pts[s].y + pts[s + 1].y) / 2
      ctx.beginPath(); ctx.arc(mx, my, 4 / zoom, 0, Math.PI * 2)
      ctx.fillStyle = '#ffffff'; ctx.fill()
      ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 1.5 / zoom; ctx.stroke()
      ctx.fillStyle = '#1a73e8'; ctx.font = `${7 / zoom}px sans-serif`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
      ctx.fillText('+', mx, my + 0.5 / zoom)
      ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic'
    }
    // poignées de waypoints existants
    for (const wp of conn.waypoints ?? []) {
      ctx.beginPath(); ctx.arc(wp.x, wp.y, 5 / zoom, 0, Math.PI * 2)
      ctx.fillStyle = '#1a73e8'; ctx.fill()
      ctx.strokeStyle = '#ffffff'; ctx.lineWidth = 1.5 / zoom; ctx.stroke()
    }
  }

  // Rubber-band connector
  if (drawingConn) {
    ctx.save()
    ctx.strokeStyle = '#1a73e8'
    ctx.lineWidth = 1.5
    ctx.setLineDash([6, 3])
    ctx.beginPath()
    ctx.moveTo(drawingConn.startX, drawingConn.startY)
    ctx.lineTo(drawingConn.currentX, drawingConn.currentY)
    ctx.stroke()
    ctx.setLineDash([])
    ctx.restore()
  }

  // Shapes
  for (const shape of data.shapes) {
    const sel = selectedIds.has(shape.id)
    const style = mergeStyle(shape.style)
    const drawStyle = sel ? { ...style, strokeColor: '#1a73e8', strokeWidth: 2 } : style
    // Rotation + horizontal/vertical mirroring around the shape centre.
    const rot = shape.rotation || 0
    const transformed = shape.flipH || shape.flipV || rot
    if (transformed) {
      const cx = shape.x + shape.w / 2, cy = shape.y + shape.h / 2
      ctx.save()
      ctx.translate(cx, cy)
      if (rot) ctx.rotate((rot * Math.PI) / 180)
      ctx.scale(shape.flipH ? -1 : 1, shape.flipV ? -1 : 1)
      ctx.translate(-cx, -cy)
    }
    renderShape(
      ctx, shape.type, shape.x, shape.y, shape.w, shape.h,
      drawStyle, shape.label, { ...DEFAULT_LABEL_STYLE, ...shape.labelStyle },
    )
    if (transformed) ctx.restore()

    // Selection handles — boîte décalée vers l'extérieur (HANDLE_MARGIN) pour ne pas
    // recouvrir les points d'ancrage situés sur le bord.
    if (sel) {
      const m = HANDLE_MARGIN / zoom
      ctx.save()
      ctx.strokeStyle = '#1a73e8'
      ctx.lineWidth = 1 / zoom
      ctx.setLineDash([4 / zoom, 2 / zoom])
      ctx.strokeRect(shape.x - m, shape.y - m, shape.w + 2 * m, shape.h + 2 * m)
      ctx.setLineDash([])
      for (const h of getResizeHandles(shape, m)) {
        ctx.fillStyle = '#ffffff'
        ctx.strokeStyle = '#1a73e8'
        ctx.lineWidth = 1.5 / zoom
        ctx.fillRect(h.x - HANDLE_R / zoom, h.y - HANDLE_R / zoom, (HANDLE_R * 2) / zoom, (HANDLE_R * 2) / zoom)
        ctx.strokeRect(h.x - HANDLE_R / zoom, h.y - HANDLE_R / zoom, (HANDLE_R * 2) / zoom, (HANDLE_R * 2) / zoom)
      }
      ctx.restore()
    }

    // Port points
    if (shape.id === hoveredShapeId || sel) {
      ctx.save()
      for (const p of getPortPoints(shape)) {
        ctx.beginPath()
        ctx.arc(p.x, p.y, PORT_R / zoom, 0, Math.PI * 2)
        ctx.fillStyle = '#1a73e8'
        ctx.fill()
        ctx.strokeStyle = '#ffffff'
        ctx.lineWidth = 1.5 / zoom
        ctx.stroke()
      }
      ctx.restore()
    }
  }

  // Smart alignment guides (magenta dashed lines spanning the viewport).
  if (alignGuides && (alignGuides.v.length || alignGuides.h.length)) {
    const wLeft = (0 - panX) / zoom, wRight = (W - panX) / zoom
    const wTop = (0 - panY) / zoom, wBottom = (H - panY) / zoom
    ctx.save()
    ctx.strokeStyle = '#ff3399'
    ctx.lineWidth = 1 / zoom
    ctx.setLineDash([4 / zoom, 3 / zoom])
    ctx.beginPath()
    for (const vx of alignGuides.v) { ctx.moveTo(vx, wTop); ctx.lineTo(vx, wBottom) }
    for (const hy of alignGuides.h) { ctx.moveTo(wLeft, hy); ctx.lineTo(wRight, hy) }
    ctx.stroke()
    ctx.setLineDash([])
    ctx.restore()
  }

  ctx.restore()

  // Lasso
  if (lasso) {
    ctx.save()
    ctx.strokeStyle = '#1a73e8'
    ctx.lineWidth = 1
    ctx.setLineDash([4, 3])
    ctx.fillStyle = 'rgba(26, 115, 232, 0.06)'
    const rx = Math.min(lasso.x1, lasso.x2) * zoom + panX
    const ry = Math.min(lasso.y1, lasso.y2) * zoom + panY
    const rw = Math.abs(lasso.x2 - lasso.x1) * zoom
    const rh = Math.abs(lasso.y2 - lasso.y1) * zoom
    ctx.fillRect(rx, ry, rw, rh)
    ctx.strokeRect(rx, ry, rw, rh)
    ctx.setLineDash([])
    ctx.restore()
  }

  ctx.restore()
}

// ── Stencil thumbnail ─────────────────────────────────────────────────────────

function StencilThumbnail({ stencil, w = 60, h = 40 }: { stencil: StencilDef; w?: number; h?: number }) {
  const ref = useRef<HTMLCanvasElement>(null)

  const draw = useCallback(() => {
    const c = ref.current
    if (!c) return
    const dpr = window.devicePixelRatio || 1
    c.width = w * dpr; c.height = h * dpr
    const ctx = c.getContext('2d')!
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, w, h)
    const margin = Math.max(4, Math.round(Math.min(w, h) * 0.12))
    renderShape(
      ctx, stencil.id,
      margin, margin, w - margin * 2, h - margin * 2,
      mergeStyle(stencil.style),
      '', DEFAULT_LABEL_STYLE,
    )
  }, [stencil.id, w, h])

  useEffect(() => { draw() }, [draw])
  // Redessine quand une icône SVG (matériel) finit de charger.
  useEffect(() => onHwIconLoaded(draw), [draw])

  return <canvas ref={ref} style={{ width: w, height: h }} className="block" />
}

// ── Minimap navigator ─────────────────────────────────────────────────────────

function Minimap({ data, zoom, panX, panY, canvasRef, onJump }: {
  data: DiagramData
  zoom: number; panX: number; panY: number
  canvasRef: React.RefObject<HTMLCanvasElement | null>
  onJump: (wx: number, wy: number) => void
}) {
  const ref = useRef<HTMLCanvasElement>(null)
  const transformRef = useRef<{ minX: number; minY: number; scale: number; ox: number; oy: number } | null>(null)
  const MW = 180, MH = 120

  useEffect(() => {
    const c = ref.current
    if (!c) return
    const dpr = window.devicePixelRatio || 1
    c.width = MW * dpr; c.height = MH * dpr
    const ctx = c.getContext('2d')!
    ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, MW, MH)
    ctx.fillStyle = '#fafafa'; ctx.fillRect(0, 0, MW, MH)

    const vw = canvasRef.current?.clientWidth ?? 800
    const vh = canvasRef.current?.clientHeight ?? 600
    const vx0 = (0 - panX) / zoom, vy0 = (0 - panY) / zoom
    const vx1 = (vw - panX) / zoom, vy1 = (vh - panY) / zoom

    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
    for (const s of data.shapes) { minX = Math.min(minX, s.x); minY = Math.min(minY, s.y); maxX = Math.max(maxX, s.x + s.w); maxY = Math.max(maxY, s.y + s.h) }
    minX = Math.min(minX, vx0); minY = Math.min(minY, vy0); maxX = Math.max(maxX, vx1); maxY = Math.max(maxY, vy1)
    if (!isFinite(minX)) { minX = 0; minY = 0; maxX = vw; maxY = vh }
    const pad = 20; minX -= pad; minY -= pad; maxX += pad; maxY += pad
    const cw = maxX - minX || 1, ch = maxY - minY || 1
    const scale = Math.min(MW / cw, MH / ch)
    const ox = (MW - cw * scale) / 2, oy = (MH - ch * scale) / 2
    transformRef.current = { minX, minY, scale, ox, oy }
    const tx = (wx: number) => ox + (wx - minX) * scale
    const ty = (wy: number) => oy + (wy - minY) * scale

    for (const s of data.shapes) {
      const st = mergeStyle(s.style)
      ctx.fillStyle = st.fillColor === 'none' ? '#e8eaed' : st.fillColor
      ctx.strokeStyle = st.strokeColor === 'none' ? '#9aa0a6' : st.strokeColor
      ctx.lineWidth = 0.5
      ctx.fillRect(tx(s.x), ty(s.y), s.w * scale, s.h * scale)
      ctx.strokeRect(tx(s.x), ty(s.y), s.w * scale, s.h * scale)
    }
    // Viewport rectangle
    ctx.fillStyle = 'rgba(26,115,232,0.10)'
    ctx.fillRect(tx(vx0), ty(vy0), (vx1 - vx0) * scale, (vy1 - vy0) * scale)
    ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 1.5
    ctx.strokeRect(tx(vx0), ty(vy0), (vx1 - vx0) * scale, (vy1 - vy0) * scale)
  }, [data, zoom, panX, panY, canvasRef])

  const jump = (e: React.MouseEvent) => {
    const c = ref.current, tf = transformRef.current
    if (!c || !tf) return
    const rect = c.getBoundingClientRect()
    const wx = (e.clientX - rect.left - tf.ox) / tf.scale + tf.minX
    const wy = (e.clientY - rect.top - tf.oy) / tf.scale + tf.minY
    onJump(wx, wy)
  }

  return (
    <canvas
      ref={ref}
      style={{ width: MW, height: MH }}
      className="block cursor-pointer"
      onMouseDown={jump}
      onMouseMove={(e) => { if (e.buttons === 1) jump(e) }}
    />
  )
}

// ── Ruler (top / left) ────────────────────────────────────────────────────────

const RULER_THICK = 18
function Ruler({ orientation, pan, zoom, length }: { orientation: 'h' | 'v'; pan: number; zoom: number; length: number }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const isH = orientation === 'h'
  useEffect(() => {
    const c = ref.current
    if (!c || length <= 0) return
    const dpr = window.devicePixelRatio || 1
    const W = isH ? length : RULER_THICK
    const H = isH ? RULER_THICK : length
    c.width = W * dpr; c.height = H * dpr
    const ctx = c.getContext('2d')!
    ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, W, H)
    ctx.fillStyle = '#f8f9fa'; ctx.fillRect(0, 0, W, H)
    ctx.strokeStyle = '#dadce0'; ctx.lineWidth = 1
    ctx.beginPath(); if (isH) { ctx.moveTo(0, H - 0.5); ctx.lineTo(W, H - 0.5) } else { ctx.moveTo(W - 0.5, 0); ctx.lineTo(W - 0.5, H) } ctx.stroke()
    // Choose a "nice" world step so labelled ticks are ~70px apart on screen.
    const bases = [5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000]
    let step = bases.find((b) => b * zoom >= 70) ?? 10000
    ctx.fillStyle = '#80868b'; ctx.font = '8px "Google Sans", sans-serif'
    ctx.strokeStyle = '#bdc1c6'
    const span = isH ? W : H
    const startWorld = Math.floor((-pan) / (step * zoom)) * step
    for (let world = startWorld; ; world += step) {
      const screen = pan + world * zoom
      if (screen > span) break
      if (screen >= 0) {
        ctx.beginPath()
        if (isH) { ctx.moveTo(screen + 0.5, RULER_THICK); ctx.lineTo(screen + 0.5, RULER_THICK - 8) }
        else { ctx.moveTo(RULER_THICK, screen + 0.5); ctx.lineTo(RULER_THICK - 8, screen + 0.5) }
        ctx.stroke()
        if (isH) ctx.fillText(String(world), screen + 2, 3)
        else { ctx.save(); ctx.translate(3, screen + 2); ctx.rotate(-Math.PI / 2); ctx.textAlign = 'right'; ctx.fillText(String(world), 0, 6); ctx.restore() }
      }
      // minor ticks
      for (let k = 1; k < 5; k++) {
        const ms = pan + (world + (step / 5) * k) * zoom
        if (ms < 0 || ms > span) continue
        ctx.beginPath()
        if (isH) { ctx.moveTo(ms + 0.5, RULER_THICK); ctx.lineTo(ms + 0.5, RULER_THICK - 4) }
        else { ctx.moveTo(RULER_THICK, ms + 0.5); ctx.lineTo(RULER_THICK - 4, ms + 0.5) }
        ctx.stroke()
      }
    }
  }, [orientation, pan, zoom, length, isH])
  return <canvas ref={ref} style={{ width: isH ? length : RULER_THICK, height: isH ? RULER_THICK : length }} className="block" />
}

// ── Main component ────────────────────────────────────────────────────────────

export default function DiagramEditorPage() {
  const { t, i18n } = useTranslation('office')
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()

  // ── Data fetching ──────────────────────────────────────────────────────────

  const { data: diagramMeta, isLoading: loadingDiagram } = useQuery({
    queryKey: ['diagram', id],
    queryFn:  () => diagramsApi.get(id!),
    enabled:  !!id,
    staleTime: 30_000,
  })

  const diagram = diagramMeta?.diagram
  const pageList = diagramMeta?.pages ?? []

  const [currentPageId, setCurrentPageId] = useState<string | null>(null)

  useEffect(() => {
    if (pageList.length > 0 && !currentPageId) setCurrentPageId(pageList[0].id)
  }, [pageList.length, currentPageId])

  const { data: currentPage, isLoading: loadingPage } = useQuery({
    queryKey: ['diagram-page', id, currentPageId],
    queryFn:  () => diagramsApi.getPage(id!, currentPageId!),
    enabled:  !!id && !!currentPageId,
    staleTime: 30_000,
  })

  const bgColor = currentPage?.bg_color ?? '#ffffff'

  // ── Local diagram state ────────────────────────────────────────────────────

  const [data, setData] = useState<DiagramData>({ shapes: [], connectors: [] })
  const dataRef = useRef(data) // kept in sync below; read by history/gesture helpers
  const prevPageId = useRef<string | null>(null)

  useEffect(() => {
    if (!currentPage) return
    if (currentPage.id === prevPageId.current) return
    prevPageId.current = currentPage.id
    const raw = currentPage.data as { shapes?: DiagramShape[]; connectors?: DiagramConnector[] } | null
    setData({ shapes: raw?.shapes ?? [], connectors: raw?.connectors ?? [] })
  }, [currentPage?.id])

  // ── Auto-save ──────────────────────────────────────────────────────────────

  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const [saveStatus, setSaveStatus] = useState<'saved' | 'saving' | 'unsaved'>('saved')

  const saveMut = useMutation({
    mutationFn: (d: DiagramData) => diagramsApi.updatePageData(id!, currentPageId!, d),
    onSuccess: () => setSaveStatus('saved'),
    onError:   () => setSaveStatus('unsaved'),
  })

  const pendingDataRef = useRef<DiagramData | null>(null)
  const saveData = useCallback((d: DiagramData) => {
    if (!currentPageId) return
    pendingDataRef.current = d
    setSaveStatus('saving')
    clearTimeout(saveTimer.current)
    saveTimer.current = setTimeout(() => {
      if (pendingDataRef.current) { saveMut.mutate(pendingDataRef.current); pendingDataRef.current = null }
    }, 1500)
  }, [currentPageId]) // eslint-disable-line react-hooks/exhaustive-deps

  // Vide la sauvegarde différée immédiatement (avant de quitter / masquer l'onglet
  // / démonter) pour ne pas perdre les dernières modifications.
  const flushSave = useCallback(() => {
    clearTimeout(saveTimer.current)
    if (pendingDataRef.current) { saveMut.mutate(pendingDataRef.current); pendingDataRef.current = null }
  }, [saveMut])

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

  // ── Undo / redo history ──────────────────────────────────────────────────────
  // Snapshots of `data`. A discrete action pushes one entry; a drag gesture pushes
  // a single entry (its pre-gesture snapshot) — coalesced via the gesture flags.
  const pastRef   = useRef<DiagramData[]>([])
  const futureRef = useRef<DiagramData[]>([])
  const [, setHistTick] = useState(0)
  const inGestureRef     = useRef(false)
  const gesturePushedRef = useRef(false)

  const cloneData = (d: DiagramData): DiagramData => ({
    shapes: d.shapes.map((s) => ({ ...s, style: { ...s.style }, labelStyle: { ...s.labelStyle } })),
    connectors: d.connectors.map((c) => ({ ...c, style: { ...c.style }, waypoints: (c.waypoints ?? []).map((p) => ({ ...p })) })),
  })
  const pushHistory = useCallback((snap: DiagramData) => {
    pastRef.current.push(cloneData(snap))
    if (pastRef.current.length > 100) pastRef.current.shift()
    futureRef.current = []
    setHistTick((t) => t + 1)
  }, [])

  const mutateData = useCallback((updater: (prev: DiagramData) => DiagramData) => {
    // Record history before applying (dataRef.current is the pre-action state).
    if (inGestureRef.current) {
      if (!gesturePushedRef.current) { pushHistory(dataRef.current); gesturePushedRef.current = true }
    } else {
      pushHistory(dataRef.current)
    }
    setData((prev) => {
      const next = updater(prev)
      saveData(next)
      return next
    })
  }, [saveData, pushHistory]) // eslint-disable-line react-hooks/exhaustive-deps

  const undo = useCallback(() => {
    if (!pastRef.current.length) return
    const target = pastRef.current.pop()!
    futureRef.current.push(cloneData(dataRef.current))
    setSelectedIds(new Set()); setSelectedConnIds(new Set())
    setData(target); saveData(target)
    setHistTick((t) => t + 1)
  }, [saveData]) // eslint-disable-line react-hooks/exhaustive-deps

  const redo = useCallback(() => {
    if (!futureRef.current.length) return
    const target = futureRef.current.pop()!
    pastRef.current.push(cloneData(dataRef.current))
    setSelectedIds(new Set()); setSelectedConnIds(new Set())
    setData(target); saveData(target)
    setHistTick((t) => t + 1)
  }, [saveData]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Clipboard (internal) ─────────────────────────────────────────────────────
  const clipboardRef = useRef<{ shapes: DiagramShape[]; connectors: DiagramConnector[] } | null>(null)

  // ── Layers ───────────────────────────────────────────────────────────────────
  const [activeLayerId, setActiveLayerId] = useState<string>(DEFAULT_LAYER_ID)
  const [renamingLayer, setRenamingLayer] = useState<string | null>(null)
  const [renameLayerVal, setRenameLayerVal] = useState('')
  const layers = getLayers(data)
  const hiddenLayers = new Set(layers.filter((l) => !l.visible).map((l) => l.id))
  const lockedLayers = new Set(layers.filter((l) => l.locked).map((l) => l.id))
  const layerOf = (it: { layerId?: string }) => it.layerId ?? layers[0].id
  const isPickable = (it: { layerId?: string }) => { const lid = layerOf(it); return !hiddenLayers.has(lid) && !lockedLayers.has(lid) }
  // Stable sort by layer order (lower index drawn first / underneath). Within a
  // layer the original array order (z-order) is preserved.
  const layerIndex = new Map(layers.map((l, i) => [l.id, i]))
  const byLayer = <T extends { layerId?: string }>(arr: T[]) =>
    [...arr].sort((a, b) => (layerIndex.get(layerOf(a)) ?? 0) - (layerIndex.get(layerOf(b)) ?? 0))
  // Hit-test only against shapes/connectors on visible, unlocked layers (layer-ordered).
  const pickShapes = byLayer(data.shapes.filter(isPickable))
  const pickConns  = byLayer(data.connectors.filter(isPickable))
  // Render data with hidden layers removed (visual only), layer-ordered.
  const visibleData: DiagramData = {
    ...data,
    shapes: byLayer(data.shapes.filter((s) => !hiddenLayers.has(layerOf(s)))),
    connectors: byLayer(data.connectors.filter((c) => !hiddenLayers.has(layerOf(c)))),
  }
  const visibleDataRef = useRef(visibleData)
  visibleDataRef.current = visibleData
  // Keep the active layer valid (e.g. after deleting a layer).
  useEffect(() => { if (!layers.some((l) => l.id === activeLayerId)) setActiveLayerId(layers[0].id) }, [layers, activeLayerId])
  const activeLayerIdRef = useRef(activeLayerId)
  activeLayerIdRef.current = activeLayerId

  // ── Layer operations ─────────────────────────────────────────────────────────
  const addLayer = () => {
    const id = makeId()
    mutateData((d) => {
      const ls = getLayers(d)
      return { ...d, layers: [...ls, { id, name: `Calque ${ls.length + 1}`, visible: true, locked: false }] }
    })
    setActiveLayerId(id)
  }
  const deleteLayer = (lid: string) => {
    if (getLayers(data).length <= 1) return
    mutateData((d) => {
      const ls = getLayers(d).filter((l) => l.id !== lid)
      return {
        layers: ls,
        shapes: d.shapes.filter((s) => (s.layerId ?? getLayers(d)[0].id) !== lid),
        connectors: d.connectors.filter((c) => (c.layerId ?? getLayers(d)[0].id) !== lid),
      }
    })
  }
  const renameLayer = (lid: string, name: string) => mutateData((d) => ({ ...d, layers: getLayers(d).map((l) => l.id === lid ? { ...l, name } : l) }))
  const toggleLayerVisible = (lid: string) => mutateData((d) => ({ ...d, layers: getLayers(d).map((l) => l.id === lid ? { ...l, visible: !l.visible } : l) }))
  const toggleLayerLocked = (lid: string) => mutateData((d) => ({ ...d, layers: getLayers(d).map((l) => l.id === lid ? { ...l, locked: !l.locked } : l) }))
  // 'up' = toward the top of the stack (higher array index = drawn later/on top).
  const moveLayer = (lid: string, dir: 'up' | 'down') => mutateData((d) => {
    const ls = [...getLayers(d)]
    const i = ls.findIndex((l) => l.id === lid)
    const j = dir === 'up' ? i + 1 : i - 1
    if (i < 0 || j < 0 || j >= ls.length) return d
    ;[ls[i], ls[j]] = [ls[j], ls[i]]
    return { ...d, layers: ls }
  })
  const moveSelectionToLayer = (lid: string) => {
    if (!selectedIds.size && !selectedConnIds.size) return
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) => selectedIds.has(s.id) ? { ...s, layerId: lid } : s),
      connectors: d.connectors.map((c) => selectedConnIds.has(c.id) ? { ...c, layerId: lid } : c),
    }))
  }

  // ── Title editing ──────────────────────────────────────────────────────────

  const [title, setTitle] = useState('')

  useEffect(() => {
    if (diagram) setTitle(diagram.title)
  }, [diagram?.id])

  const titleSaveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const handleTitleChange = (v: string) => {
    setTitle(v)
    clearTimeout(titleSaveTimer.current)
    titleSaveTimer.current = setTimeout(() => {
      diagramsApi.update(id!, { title: v })
        .then(() => qc.invalidateQueries({ queryKey: ['diagram', id] }))
    }, 800)
  }

  // Chrome standard (WorkspaceShell) : corbeille, nouveau, dupliquer.
  const trashDiagMut     = useMutation({ mutationFn: () => diagramsApi.trash(id!), onSuccess: () => navigate('/office/diagrams') })
  const createDiagMut    = useMutation({ mutationFn: () => diagramsApi.create({ title: t('common_untitled') }), onSuccess: (d) => navigate(`/office/diagrams/${d.id}`) })
  const duplicateDiagMut = useMutation({ mutationFn: () => diagramsApi.duplicate(id!), onSuccess: (d) => navigate(`/office/diagrams/${d.id}`) })

  // ── Canvas refs ────────────────────────────────────────────────────────────

  const canvasRef   = useRef<HTMLCanvasElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  const [zoom, setZoom]   = useState(1)
  const [panX, setPanX]   = useState(60)
  const [panY, setPanY]   = useState(60)

  // View options (View tab)
  const GRID_SIZE = 10
  const [showGrid, setShowGrid]     = useState(true)
  const [snapEnabled, setSnapEnabled] = useState(false)
  const [showMinimap, setShowMinimap] = useState(true)
  const [showLayers, setShowLayers]   = useState(false)
  const [showRulers, setShowRulers]   = useState(false)
  const [showTemplates, setShowTemplates] = useState(false)
  const [containerSize, setContainerSize] = useState({ w: 0, h: 0 })
  useEffect(() => { setGridSnap(snapEnabled ? GRID_SIZE : 1) }, [snapEnabled])

  // ── Interaction state ──────────────────────────────────────────────────────

  const [selectedIds,     setSelectedIds]     = useState<Set<string>>(new Set())
  const [selectedConnIds, setSelectedConnIds] = useState<Set<string>>(new Set())
  const [hoveredShapeId,  setHoveredShapeId]  = useState<string | null>(null)

  // Dragging shapes
  const dragRef = useRef<{
    type: 'move' | 'resize'
    shapeId: string
    handleType?: string
    startWx: number; startWy: number
    origX: number; origY: number; origW: number; origH: number
    offsets: { id: string; dx: number; dy: number }[]
  } | null>(null)

  // Déplacement d'un nœud (waypoint) de connecteur
  const connDragRef = useRef<{ connId: string; index: number } | null>(null)
  // Ajout d'un nœud via une poignée « + » : armé au mousedown, créé seulement si on
  // glisse (sinon un double-clic au milieu ajouterait/retirerait un nœud au lieu
  // d'éditer le label).
  const pendingNodeRef = useRef<{ connId: string; seg: number; downX: number; downY: number } | null>(null)
  // Déplacement d'une portion (segment entre deux coudes) d'un connecteur. Armé au
  // mousedown sur le corps d'un segment ; les nœuds à déplacer (et l'éventuelle
  // création de coudes pour un segment droit) sont résolus au premier glissement.
  const segDragRef = useRef<{
    connId: string; seg: number; downX: number; downY: number
    started: boolean; movingIdx: number[] | null; orig: { x: number; y: number }[] | null
    base: { x: number; y: number }[] | null
  } | null>(null)
  // Glisser-déposer du label d'un connecteur (repositionnement).
  const labelDragRef = useRef<{ connId: string; downX: number; downY: number; origX: number; origY: number } | null>(null)
  // Segments (indices de polyligne) actuellement « aimantés » à 90° pendant un drag,
  // surlignés en vert. Réévalué à chaque frame ; vidé au relâchement. (ref pur : pas
  // de re-render dédié, lu par la boucle de rendu.)
  const magnetSegsRef = useRef<{ connId: string; segs: number[] }[] | null>(null)
  // Smart alignment guide lines (world coords) shown while dragging a single shape.
  const alignGuidesRef = useRef<{ v: number[]; h: number[] } | null>(null)
  // Docking controller (Formes / Format / Calques panels live in the DockArea).
  const dockRef = useRef<DockController | null>(null)

  // Panning
  const panRef = useRef<{ startX: number; startY: number; startPanX: number; startPanY: number } | null>(null)

  // Drawing connector
  const [drawingConn, setDrawingConn] = useState<{
    sourceId: string | null
    startX: number; startY: number
    currentX: number; currentY: number
  } | null>(null)
  const drawingConnRef = useRef(drawingConn)
  drawingConnRef.current = drawingConn

  // Lasso
  const [lasso, setLasso] = useState<{ x1: number; y1: number; x2: number; y2: number } | null>(null)
  const lassoRef = useRef<typeof lasso>(null)
  lassoRef.current = lasso

  // ── Stencil drag-from-panel ────────────────────────────────────────────────

  const [pendingStencil, setPendingStencil] = useState<StencilDef | null>(null)
  const dragStencilRef = useRef<StencilDef | null>(null)

  // ── Canvas resize ──────────────────────────────────────────────────────────

  const renderAfterResizeRef = useRef<(() => void) | null>(null)

  useEffect(() => {
    const c = canvasRef.current
    const container = containerRef.current
    if (!c || !container) return
    const ro = new ResizeObserver(() => {
      const dpr = window.devicePixelRatio || 1
      c.width  = container.offsetWidth  * dpr
      c.height = container.offsetHeight * dpr
      c.style.width  = `${container.offsetWidth}px`
      c.style.height = `${container.offsetHeight}px`
      setContainerSize({ w: container.offsetWidth, h: container.offsetHeight })
      renderAfterResizeRef.current?.()
    })
    ro.observe(container)
    return () => ro.disconnect()
    // Réattacher après la fin du chargement : tant que `loadingDiagram` est vrai, le
    // canvas n'est pas monté (early-return spinner) → les refs sont nulles et le
    // ResizeObserver ne se branche jamais (canvas figé à 300×150). On relance l'effet
    // une fois le canvas présent pour qu'il prenne la taille réelle du conteneur.
  }, [loadingDiagram])

  // ── Render ─────────────────────────────────────────────────────────────────

  const zoomRef    = useRef(zoom)
  const panXRef    = useRef(panX)
  const panYRef    = useRef(panY)
  const selRef     = useRef(selectedIds)
  const selConnRef = useRef(selectedConnIds)
  const hovRef     = useRef(hoveredShapeId)
  const lassoRefV  = useRef(lasso)
  const dcRef      = useRef(drawingConn)
  const showGridRef = useRef(showGrid)
  showGridRef.current = showGrid

  dataRef.current    = data
  zoomRef.current    = zoom
  panXRef.current    = panX
  panYRef.current    = panY
  selRef.current     = selectedIds
  selConnRef.current = selectedConnIds
  hovRef.current     = hoveredShapeId
  lassoRefV.current  = lasso
  dcRef.current      = drawingConn

  const rafRef = useRef<number | undefined>(undefined)

  const requestRender = useCallback(() => {
    cancelAnimationFrame(rafRef.current!)
    rafRef.current = requestAnimationFrame(() => {
      const c = canvasRef.current
      if (!c) return
      renderCanvas(
        c, visibleDataRef.current,
        zoomRef.current, panXRef.current, panYRef.current,
        selRef.current, selConnRef.current,
        hovRef.current, dcRef.current, lassoRefV.current, bgColor,
        magnetSegsRef.current,
        showGridRef.current, GRID_SIZE,
        alignGuidesRef.current,
      )
    })
  }, [bgColor]) // eslint-disable-line react-hooks/exhaustive-deps

  renderAfterResizeRef.current = requestRender

  useEffect(() => { requestRender() }, [data, zoom, panX, panY, selectedIds, selectedConnIds, hoveredShapeId, drawingConn, lasso, bgColor, showGrid])
  // Redessine le canvas quand une icône SVG (matériel) finit de charger.
  useEffect(() => onHwIconLoaded(requestRender), [requestRender])

  // ── Label editing ──────────────────────────────────────────────────────────

  const [editingLabel, setEditingLabel] = useState<{ id: string; isConnector: boolean } | null>(null)
  const [labelText,    setLabelText]    = useState('')
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const labelInputRef = useRef<HTMLInputElement>(null)

  // Menu contextuel (clic droit) — rendu via le composant MenuDropdown de @ui.
  // Pour un connecteur : seg/worldX/worldY = point cliqué (« ajouter un nœud ici »).
  const [ctxMenu, setCtxMenu] = useState<
    | { kind: 'connector'; x: number; y: number; connId: string; worldX: number; worldY: number; seg: number }
    | { kind: 'shape'; x: number; y: number; shapeId: string }
    | { kind: 'canvas'; x: number; y: number; worldX: number; worldY: number }
    | null
  >(null)

  // Fermeture sur Échap / molette (MenuDropdown gère déjà le clic extérieur).
  useEffect(() => {
    if (!ctxMenu) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setCtxMenu(null) }
    const onWheel = () => setCtxMenu(null)
    window.addEventListener('keydown', onKey)
    window.addEventListener('wheel', onWheel, { passive: true })
    return () => { window.removeEventListener('keydown', onKey); window.removeEventListener('wheel', onWheel) }
  }, [ctxMenu])

  const startLabelEdit = (id: string, isConnector: boolean, current: string) => {
    setEditingLabel({ id, isConnector })
    setLabelText(current)
    setTimeout(() => labelInputRef.current?.focus(), 0)
  }

  const commitLabel = () => {
    if (!editingLabel) return
    if (editingLabel.isConnector) {
      mutateData((d) => ({
        ...d,
        connectors: d.connectors.map((c) =>
          c.id === editingLabel.id ? { ...c, label: labelText } : c,
        ),
      }))
    } else {
      mutateData((d) => ({
        ...d,
        shapes: d.shapes.map((s) =>
          s.id === editingLabel.id ? { ...s, label: labelText } : s,
        ),
      }))
    }
    setEditingLabel(null)
  }

  // ── Properties panel state ─────────────────────────────────────────────────

  const [showProps, setShowProps] = useState(true)
  const [formatTab, setFormatTab] = useState<'style' | 'text' | 'arrange'>('style')
  const [showStyleEditor, setShowStyleEditor] = useState(false)
  const [styleText, setStyleText] = useState('')

  const selectedShape = selectedIds.size === 1
    ? data.shapes.find((s) => s.id === [...selectedIds][0]) ?? null
    : null
  const selectedConn = selectedConnIds.size === 1
    ? data.connectors.find((c) => c.id === [...selectedConnIds][0]) ?? null
    : null

  const updateShapeStyle = (sid: string, patch: Partial<ShapeStyle>) => {
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) =>
        s.id === sid ? { ...s, style: { ...s.style, ...patch } } : s,
      ),
    }))
  }

  const updateShapeLabelStyle = (sid: string, patch: Partial<LabelStyle>) => {
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) =>
        s.id === sid ? { ...s, labelStyle: { ...s.labelStyle, ...patch } } : s,
      ),
    }))
  }

  const updateConnStyle = (cid: string, patch: Partial<ConnectorStyle>) => {
    mutateData((d) => ({
      ...d,
      connectors: d.connectors.map((c) =>
        c.id === cid ? { ...c, style: { ...c.style, ...patch } } : c,
      ),
    }))
  }

  // ── Connector actions (menu contextuel) ────────────────────────────────────

  const connAddNode = (cid: string, seg: number, wx: number, wy: number) => {
    mutateData((d) => ({ ...d, connectors: d.connectors.map((c) => c.id === cid
      ? { ...c, waypoints: [
          ...(c.waypoints ?? []).slice(0, seg),
          { x: snapToGrid(wx), y: snapToGrid(wy) },
          ...(c.waypoints ?? []).slice(seg),
        ] }
      : c) }))
  }

  const connClearNodes = (cid: string) => {
    mutateData((d) => ({ ...d, connectors: d.connectors.map((c) => c.id === cid ? { ...c, waypoints: [] } : c) }))
  }

  // Inverse le sens : permute source/cible (donc les flèches) et retourne les nœuds.
  const connReverse = (cid: string) => {
    mutateData((d) => ({ ...d, connectors: d.connectors.map((c) => c.id === cid
      ? { ...c, sourceId: c.targetId, targetId: c.sourceId, sourcePoint: c.targetPoint, targetPoint: c.sourcePoint, waypoints: [...(c.waypoints ?? [])].reverse() }
      : c) }))
  }

  const connDuplicate = (cid: string) => {
    const src = data.connectors.find((c) => c.id === cid)
    if (!src) return
    const copy: DiagramConnector = { ...src, id: makeId(), style: { ...src.style }, waypoints: (src.waypoints ?? []).map((p) => ({ x: p.x + 16, y: p.y + 16 })) }
    mutateData((d) => ({ ...d, connectors: [...d.connectors, copy] }))
    const sel = new Set([copy.id]); setSelectedConnIds(sel); setSelectedIds(new Set())
  }

  // Réordonne un connecteur dans la pile (les connecteurs se dessinent avant les
  // formes ; n'influe que sur le recouvrement connecteur/connecteur).
  const connReorder = (cid: string, toFront: boolean) => {
    mutateData((d) => {
      const rest = d.connectors.filter((c) => c.id !== cid)
      const me = d.connectors.find((c) => c.id === cid)
      if (!me) return d
      return { ...d, connectors: toFront ? [...rest, me] : [me, ...rest] }
    })
  }

  const deleteConnector = (cid: string) => {
    mutateData((d) => ({ ...d, connectors: d.connectors.filter((c) => c.id !== cid) }))
    setSelectedConnIds(new Set())
  }

  // ── Shape actions (menu contextuel objet) ──────────────────────────────────

  const shapeDuplicate = (sid: string) => {
    const src = data.shapes.find((s) => s.id === sid)
    if (!src) return
    const copy: DiagramShape = { ...src, id: makeId(), x: src.x + 16, y: src.y + 16, style: { ...src.style }, labelStyle: { ...src.labelStyle }, zIndex: data.shapes.length }
    mutateData((d) => ({ ...d, shapes: [...d.shapes, copy] }))
    const sel = new Set([copy.id]); setSelectedIds(sel); selRef.current = sel; setSelectedConnIds(new Set())
  }

  // Réordonne une forme dans la pile (l'ordre du tableau = ordre de rendu / z).
  const shapeReorder = (sid: string, mode: 'front' | 'back' | 'forward' | 'backward') => {
    mutateData((d) => {
      const arr = [...d.shapes]
      const i = arr.findIndex((s) => s.id === sid)
      if (i < 0) return d
      const [s] = arr.splice(i, 1)
      if (mode === 'front') arr.push(s)
      else if (mode === 'back') arr.unshift(s)
      else if (mode === 'forward') arr.splice(Math.min(arr.length, i + 1), 0, s)
      else arr.splice(Math.max(0, i - 1), 0, s)
      return { ...d, shapes: arr }
    })
  }

  const deleteShape = (sid: string) => {
    mutateData((d) => ({
      shapes: d.shapes.filter((s) => s.id !== sid),
      connectors: d.connectors.filter((c) => c.sourceId !== sid && c.targetId !== sid),
    }))
    setSelectedIds(new Set())
  }

  const handleContextMenu = (e: React.MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault()
    if (editingLabel) commitLabel()
    const w = getWorldPos(e)

    // Clic droit sur une forme → menu objet (conserve la sélection multiple si la
    // forme cliquée en fait partie).
    const shape = getShapeAt(pickShapes, w.x, w.y)
    if (shape) {
      if (!selectedIds.has(shape.id)) {
        const sel = new Set([shape.id])
        setSelectedIds(sel); selRef.current = sel
        setSelectedConnIds(new Set()); selConnRef.current = new Set()
      }
      setCtxMenu({ kind: 'shape', x: e.clientX, y: e.clientY, shapeId: shape.id })
      return
    }

    // Sinon, clic droit sur un connecteur → menu connecteur.
    const hit = getConnectorAt(pickConns, data.shapes, w.x, w.y, 10 / zoom)
    if (!hit) {
      // Clic droit dans le vide → menu du canevas.
      setSelectedIds(new Set()); selRef.current = new Set()
      setSelectedConnIds(new Set()); selConnRef.current = new Set()
      setCtxMenu({ kind: 'canvas', x: e.clientX, y: e.clientY, worldX: w.x, worldY: w.y })
      return
    }
    const sel = new Set([hit.id])
    setSelectedConnIds(sel); selConnRef.current = sel
    setSelectedIds(new Set()); selRef.current = new Set()
    setCtxMenu({ kind: 'connector', x: e.clientX, y: e.clientY, connId: hit.id, worldX: w.x, worldY: w.y, seg: hit.seg })
  }

  // ── Canvas mouse events ────────────────────────────────────────────────────

  const getWorldPos = (e: React.MouseEvent) => {
    const rect = canvasRef.current!.getBoundingClientRect()
    return canvasToWorld(e.clientX - rect.left, e.clientY - rect.top, panX, panY, zoom)
  }

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (editingLabel) { commitLabel(); return }
    const w = getWorldPos(e)

    // Middle mouse or Space+left: pan
    if (e.button === 1 || (e.button === 0 && e.altKey)) {
      panRef.current = { startX: e.clientX, startY: e.clientY, startPanX: panX, startPanY: panY }
      e.preventDefault()
      return
    }
    if (e.button !== 0) return

    // Arm gesture coalescing: a drag started here records a single history entry.
    inGestureRef.current = true
    gesturePushedRef.current = false

    // Check if drawing connector
    const port = getPortAt(pickShapes, w.x, w.y)
    if (port && !pendingStencil) {
      setDrawingConn({ sourceId: port.shapeId, startX: port.x, startY: port.y, currentX: port.x, currentY: port.y })
      return
    }

    // Check resize handles
    const handle = getHandleAt(data.shapes, selectedIds, w.x, w.y, HANDLE_MARGIN / zoom)
    if (handle) {
      const s = data.shapes.find((sh) => sh.id === handle.shapeId)!
      dragRef.current = {
        type: 'resize', shapeId: handle.shapeId, handleType: handle.handleType,
        startWx: w.x, startWy: w.y,
        origX: s.x, origY: s.y, origW: s.w, origH: s.h,
        offsets: [],
      }
      return
    }

    // Glisser le label d'un connecteur pour le repositionner (priorité sur le reste).
    {
      const mctx = canvasRef.current?.getContext('2d')
      for (let i = data.connectors.length - 1; i >= 0; i--) {
        const c = data.connectors[i]
        if (!c.label) continue
        const center = connectorLabelCenter(c, data.shapes)
        let halfW = 24
        if (mctx) { mctx.save(); mctx.font = '12px Inter, sans-serif'; halfW = mctx.measureText(c.label).width / 2 + 6; mctx.restore() }
        if (Math.abs(w.x - center.x) <= halfW && Math.abs(w.y - center.y) <= 11) {
          const sel = new Set([c.id]); setSelectedConnIds(sel); selConnRef.current = sel
          setSelectedIds(new Set()); selRef.current = new Set()
          labelDragRef.current = { connId: c.id, downX: w.x, downY: w.y, origX: c.labelOffset?.x ?? 0, origY: c.labelOffset?.y ?? 0 }
          return
        }
      }
    }

    // Poignées de nœuds du connecteur sélectionné : déplacer un waypoint, ou en
    // ajouter via une poignée de milieu de segment (puis le déplacer aussitôt).
    if (selectedConnIds.size > 0) {
      const tol = (HANDLE_R + 8) / zoom
      for (const c of data.connectors) {
        if (!selectedConnIds.has(c.id)) continue
        const wps = c.waypoints ?? []
        const wi = wps.findIndex((p) => Math.hypot(p.x - w.x, p.y - w.y) <= tol)
        if (wi >= 0) { connDragRef.current = { connId: c.id, index: wi }; return }   // déplacer un nœud
        const pts = connectorPoints(c, data.shapes)
        for (let s = 0; s < pts.length - 1; s++) {
          const mx = (pts[s].x + pts[s + 1].x) / 2, my = (pts[s].y + pts[s + 1].y) / 2
          if (Math.hypot(mx - w.x, my - w.y) <= tol) {
            // Armer l'ajout ; le nœud n'est créé que si l'on glisse (cf. handleMouseMove).
            pendingNodeRef.current = { connId: c.id, seg: s, downX: w.x, downY: w.y }
            return
          }
        }
      }
    }

    // Check shape
    const shape = getShapeAt(pickShapes, w.x, w.y)
    if (shape) {
      if (pendingStencil) {
        // Place stencil on click
        placeStencil(pendingStencil, w.x, w.y)
        setPendingStencil(null)
        return
      }
      // Sélection à déplacer : si la forme cliquée fait déjà partie de la sélection
      // (multi), on déplace TOUTE la sélection ; sinon on (re)sélectionne — shift =
      // ajout à la sélection, sans shift = sélection unique.
      // Group-aware: clicking a grouped shape selects all members of its group.
      const members = shape.groupId
        ? data.shapes.filter((s) => s.groupId === shape.groupId).map((s) => s.id)
        : [shape.id]
      let selSet: Set<string>
      if (selectedIds.has(shape.id)) {
        selSet = e.shiftKey ? new Set([...selectedIds]) : selectedIds
      } else {
        selSet = e.shiftKey ? new Set([...selectedIds, ...members]) : new Set(members)
        setSelectedIds(selSet)
        setSelectedConnIds(new Set())
        selRef.current = selSet
      }
      // Start move (déplace toutes les formes sélectionnées simultanément). Un
      // conteneur entraîne aussi son contenu géométrique (formes dont le centre est
      // à l'intérieur au début du glissement).
      const moving = new Set(selSet)
      for (const sid of selSet) {
        const cs = data.shapes.find((s) => s.id === sid)
        if (!cs || !isContainer(cs.type)) continue
        for (const o of data.shapes) {
          if (moving.has(o.id) || isContainer(o.type)) continue
          const ocx = o.x + o.w / 2, ocy = o.y + o.h / 2
          if (ocx > cs.x && ocx < cs.x + cs.w && ocy > cs.y && ocy < cs.y + cs.h) moving.add(o.id)
        }
      }
      dragRef.current = {
        type: 'move', shapeId: shape.id,
        startWx: w.x, startWy: w.y,
        origX: shape.x, origY: shape.y, origW: shape.w, origH: shape.h,
        offsets: [...moving].map((sid) => {
          const s = data.shapes.find((sh) => sh.id === sid)!
          return { id: sid, dx: s.x - w.x, dy: s.y - w.y }
        }),
      }
      return
    }

    if (pendingStencil) {
      placeStencil(pendingStencil, w.x, w.y)
      setPendingStencil(null)
      return
    }

    // Clic sur le corps d'un connecteur : le sélectionner ET armer le déplacement de
    // la portion saisie (le segment ne bouge qu'au-delà d'un petit seuil de glissement,
    // pour ne pas gêner un simple clic ou un double-clic d'édition de label).
    const hitConn = getConnectorAt(pickConns, data.shapes, w.x, w.y, 10 / zoom)
    if (hitConn) {
      const sel = new Set([hitConn.id])
      setSelectedConnIds(sel); selConnRef.current = sel
      setSelectedIds(new Set()); selRef.current = new Set()
      segDragRef.current = { connId: hitConn.id, seg: hitConn.seg, downX: w.x, downY: w.y, started: false, movingIdx: null, orig: null, base: null }
      return
    }

    // Click on empty → deselect + start lasso
    setSelectedIds(new Set())
    setSelectedConnIds(new Set())
    selRef.current = new Set()
    setLasso({ x1: w.x, y1: w.y, x2: w.x, y2: w.y })
  }

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const w = getWorldPos(e)

    // Pan
    if (panRef.current) {
      const dx = e.clientX - panRef.current.startX
      const dy = e.clientY - panRef.current.startY
      setPanX(panRef.current.startPanX + dx)
      setPanY(panRef.current.startPanY + dy)
      return
    }

    // Repositionnement du label d'un connecteur
    if (labelDragRef.current) {
      const ld = labelDragRef.current
      const nx = ld.origX + (w.x - ld.downX), ny = ld.origY + (w.y - ld.downY)
      mutateData((d) => ({ ...d, connectors: d.connectors.map((c) => c.id === ld.connId ? { ...c, labelOffset: { x: nx, y: ny } } : c) }))
      return
    }

    // Resize
    if (dragRef.current?.type === 'resize') {
      const dr = dragRef.current
      const dx = w.x - dr.startWx
      const dy = w.y - dr.startWy
      mutateData((d) => ({
        ...d,
        shapes: d.shapes.map((s) => {
          if (s.id !== dr.shapeId) return s
          const h = dr.handleType!
          let x = s.x, y = s.y, sw = s.w, sh = s.h
          if (h.includes('e')) sw = Math.max(20, dr.origW + dx)
          if (h.includes('s')) sh = Math.max(20, dr.origH + dy)
          if (h.includes('w')) { x = dr.origX + dx; sw = Math.max(20, dr.origW - dx) }
          if (h.includes('n')) { y = dr.origY + dy; sh = Math.max(20, dr.origH - dy) }
          return { ...s, x, y, w: sw, h: sh }
        }),
      }))
      return
    }

    // Move
    if (dragRef.current?.type === 'move') {
      const dr = dragRef.current
      const { offsets } = dr

      // Magnétisme H/V au déplacement d'une forme. Une position de forme a deux degrés
      // de liberté (x, y) → on peut caler PLUSIEURS portions à la fois : le meilleur
      // candidat horizontal fixe le y, le meilleur candidat vertical fixe le x (axes
      // indépendants). Toutes les portions effectivement alignées sont surlignées.
      // Seuil en pixels (et non angulaire) → distance d'accroche/sortie constante.
      const SNAP_PX = 8 / zoom
      let snapX: number | null = null // centre.x cible (portion verticale)
      let snapY: number | null = null // centre.y cible (portion horizontale)
      let green: { connId: string; segs: number[] }[] | null = null
      if (offsets.length === 1) {
        const s = dataRef.current.shapes.find((x) => x.id === dr.shapeId)
        const off = offsets[0]
        if (s) {
          const cx = Math.round(w.x + off.dx) + s.w / 2
          const cy = Math.round(w.y + off.dy) + s.h / 2
          const conns = dataRef.current.connectors.filter((c) => c.sourceId === s.id || c.targetId === s.id)
          const items: { connId: string; seg: number; R: { x: number; y: number } }[] = []
          for (const c of conns) {
            const wps = c.waypoints ?? []
            const isSource = c.sourceId === s.id
            let R: { x: number; y: number } | null
            let seg: number
            if (isSource) {
              seg = 0
              R = wps.length ? wps[0]
                : (c.targetId ? (() => { const o = dataRef.current.shapes.find((x) => x.id === c.targetId); return o ? shapeCenter(o) : null })() : c.targetPoint)
            } else {
              seg = wps.length // dernier segment de la polyligne
              R = wps.length ? wps[wps.length - 1]
                : (c.sourceId ? (() => { const o = dataRef.current.shapes.find((x) => x.id === c.sourceId); return o ? shapeCenter(o) : null })() : c.sourcePoint)
            }
            if (R) items.push({ connId: c.id, seg, R })
          }
          // Meilleur candidat sur chaque axe, indépendamment.
          let bestH: { val: number; off: number } | null = null
          let bestV: { val: number; off: number } | null = null
          for (const it of items) {
            const offY = Math.abs(it.R.y - cy) // écart vertical → portion horizontale
            const offX = Math.abs(it.R.x - cx) // écart horizontal → portion verticale
            if (offY <= SNAP_PX && (!bestH || offY < bestH.off)) bestH = { val: it.R.y, off: offY }
            if (offX <= SNAP_PX && (!bestV || offX < bestV.off)) bestV = { val: it.R.x, off: offX }
          }
          if (bestH) snapY = bestH.val
          if (bestV) snapX = bestV.val
          const finalCY = snapY != null ? snapY : cy
          const finalCX = snapX != null ? snapX : cx
          // Surligne toutes les portions effectivement alignées avec la position finale.
          const gmap = new Map<string, number[]>()
          for (const it of items) {
            const aligned = (snapY != null && Math.abs(it.R.y - finalCY) < 0.5) || (snapX != null && Math.abs(it.R.x - finalCX) < 0.5)
            if (!aligned) continue
            const arr = gmap.get(it.connId) ?? []
            if (!arr.includes(it.seg)) arr.push(it.seg)
            gmap.set(it.connId, arr)
          }
          if (gmap.size) green = [...gmap].map(([connId, segs]) => ({ connId, segs }))
        }
      }
      magnetSegsRef.current = green

      // Smart alignment guides: snap the dragged shape's edges/centres to other
      // shapes' edges/centres (only when not already snapped by connector magnetism).
      let alignX: number | null = null, alignY: number | null = null
      let guides: { v: number[]; h: number[] } | null = null
      if (offsets.length === 1) {
        const s = dataRef.current.shapes.find((x) => x.id === dr.shapeId)
        const off = offsets[0]
        if (s) {
          const px = Math.round(w.x + off.dx), py = Math.round(w.y + off.dy)
          const sX = [px, px + s.w / 2, px + s.w]
          const sY = [py, py + s.h / 2, py + s.h]
          let bestV: { line: number; adjust: number; off: number } | null = null
          let bestH: { line: number; adjust: number; off: number } | null = null
          for (const o of dataRef.current.shapes) {
            if (o.id === s.id) continue
            const oX = [o.x, o.x + o.w / 2, o.x + o.w]
            const oY = [o.y, o.y + o.h / 2, o.y + o.h]
            for (const sx of sX) for (const ox of oX) { const d = Math.abs(sx - ox); if (d <= SNAP_PX && (!bestV || d < bestV.off)) bestV = { line: ox, adjust: ox - sx, off: d } }
            for (const sy of sY) for (const oy of oY) { const d = Math.abs(sy - oy); if (d <= SNAP_PX && (!bestH || d < bestH.off)) bestH = { line: oy, adjust: oy - sy, off: d } }
          }
          const gv: number[] = [], gh: number[] = []
          if (snapX == null && bestV) { alignX = px + bestV.adjust; gv.push(bestV.line) }
          if (snapY == null && bestH) { alignY = py + bestH.adjust; gh.push(bestH.line) }
          if (gv.length || gh.length) guides = { v: gv, h: gh }
        }
      }
      alignGuidesRef.current = guides

      mutateData((d) => ({
        ...d,
        shapes: d.shapes.map((s) => {
          const off = offsets.find((o) => o.id === s.id)
          if (!off) return s
          let nx = Math.round(w.x + off.dx), ny = Math.round(w.y + off.dy)
          if (s.id === dr.shapeId) {
            if (snapX != null) nx = Math.round(snapX - s.w / 2)
            else if (alignX != null) nx = Math.round(alignX)
            if (snapY != null) ny = Math.round(snapY - s.h / 2)
            else if (alignY != null) ny = Math.round(alignY)
          }
          return { ...s, x: nx, y: ny }
        }),
      }))
      return
    }

    // Déplacement d'une portion de connecteur (glisser le segment entre deux coudes).
    // Translate les nœuds aux extrémités du segment ; le magnétisme cale chaque coude
    // déplacé sur 90° pendant le glissement.
    if (segDragRef.current) {
      const sd = segDragRef.current
      const ddx = w.x - sd.downX, ddy = w.y - sd.downY
      const c = dataRef.current.connectors.find((x) => x.id === sd.connId)
      if (!c) { segDragRef.current = null; return }
      if (!sd.started) {
        if (Math.hypot(ddx, ddy) <= 3 / zoom) return
        const wps = c.waypoints ?? []
        const leftWpi = sd.seg - 1, rightWpi = sd.seg
        const leftIs = leftWpi >= 0 && leftWpi < wps.length
        const rightIs = rightWpi >= 0 && rightWpi < wps.length
        if (leftIs || rightIs) {
          sd.movingIdx = [leftIs ? leftWpi : -1, rightIs ? rightWpi : -1].filter((v) => v >= 0)
          sd.orig = sd.movingIdx.map((i) => ({ ...wps[i] }))
          sd.base = wps
        } else {
          // Segment droit sans coude : matérialiser deux coudes à ses extrémités pour
          // pouvoir déplacer la portion (le connecteur prend alors une forme en « Z »).
          const pts = connectorPoints(c, dataRef.current.shapes)
          // `seg` may index the DISPLAYED polyline (orthogonal/curved has more
          // segments than control points) → guard against out-of-range access.
          if (!pts[sd.seg] || !pts[sd.seg + 1]) { segDragRef.current = null; return }
          const a  = { x: snapToGrid(pts[sd.seg].x),     y: snapToGrid(pts[sd.seg].y) }
          const bb = { x: snapToGrid(pts[sd.seg + 1].x), y: snapToGrid(pts[sd.seg + 1].y) }
          sd.base = [...wps.slice(0, sd.seg), a, bb, ...wps.slice(sd.seg)]
          sd.movingIdx = [sd.seg, sd.seg + 1]
          sd.orig = [{ ...a }, { ...bb }]
        }
        sd.started = true
      }
      const movingIdx = sd.movingIdx!, orig = sd.orig!, base = sd.base!
      // Positions candidates (translation des nœuds saisis depuis leur position d'origine).
      const cand = base.map((p, i) => {
        const k = movingIdx.indexOf(i)
        return k >= 0 ? { x: snapToGrid(orig[k].x + ddx), y: snapToGrid(orig[k].y + ddy) } : p
      })
      // Magnétisme 90° par coude déplacé + collecte des segments aimantés (vert).
      const poly = connectorPoints({ ...c, waypoints: cand }, dataRef.current.shapes)
      const green: number[] = []
      const finalWps = cand.map((p, i) => {
        if (movingIdx.indexOf(i) < 0) return p
        const P = poly[i], N = poly[i + 2] // voisins (poly[i+1] == waypoint i)
        if (P && N) {
          const m = magnetizeRightAngle(poly[i + 1], P, N, 15)
          if (m.snapped) green.push(i, i + 1)
          return { x: m.x, y: m.y }
        }
        return p
      })
      magnetSegsRef.current = green.length ? [{ connId: sd.connId, segs: green }] : null
      mutateData((d) => ({ ...d, connectors: d.connectors.map((cc) => cc.id === sd.connId ? { ...cc, waypoints: finalWps } : cc) }))
      return
    }

    // Ajout d'un nœud armé : créé seulement quand on commence à glisser.
    if (pendingNodeRef.current) {
      const p = pendingNodeRef.current
      if (Math.hypot(w.x - p.downX, w.y - p.downY) > 3 / zoom) {
        mutateData((d) => ({ ...d, connectors: d.connectors.map((cc) => cc.id === p.connId
          ? { ...cc, waypoints: [...(cc.waypoints ?? []).slice(0, p.seg), { x: snapToGrid(w.x), y: snapToGrid(w.y) }, ...(cc.waypoints ?? []).slice(p.seg)] }
          : cc) }))
        connDragRef.current = { connId: p.connId, index: p.seg }
        pendingNodeRef.current = null
      }
      return
    }

    // Déplacement d'un nœud (waypoint) de connecteur. L'angle du coude s'aimante
    // vers 90° dès qu'on en approche (cf. magnetizeRightAngle).
    if (connDragRef.current) {
      const { connId, index } = connDragRef.current
      const c = dataRef.current.connectors.find((x) => x.id === connId)
      let np = { x: snapToGrid(w.x), y: snapToGrid(w.y) }
      let green: number[] | null = null
      if (c) {
        const pts = connectorPoints(c, dataRef.current.shapes) // [from, ...waypoints, to]
        const P = pts[index]      // voisin précédent du nœud (= polyline[index])
        const N = pts[index + 2]  // voisin suivant      (= polyline[index+2])
        if (P && N) {
          const m = magnetizeRightAngle(np, P, N, 15)
          np = { x: m.x, y: m.y }
          if (m.snapped) green = [index, index + 1] // les 2 segments du coude
        }
      }
      magnetSegsRef.current = green ? [{ connId, segs: green }] : null
      mutateData((d) => ({ ...d, connectors: d.connectors.map((cc) => cc.id === connId
        ? { ...cc, waypoints: (cc.waypoints ?? []).map((p, i) => i === index ? np : p) }
        : cc) }))
      return
    }

    // Drawing connector
    if (drawingConn) {
      setDrawingConn({ ...drawingConn, currentX: w.x, currentY: w.y })
      return
    }

    // Lasso
    if (lasso) {
      setLasso({ ...lasso, x2: w.x, y2: w.y })
      return
    }

    // Hover
    const shape = getShapeAt(pickShapes, w.x, w.y)
    setHoveredShapeId(shape?.id ?? null)

    // Cursor
    if (shape) {
      canvasRef.current!.style.cursor = 'move'
    } else {
      const handle = getHandleAt(data.shapes, selectedIds, w.x, w.y, HANDLE_MARGIN / zoom)
      if (handle) {
        canvasRef.current!.style.cursor = handle.cursor
      } else if (selectedConnIds.size > 0 && getConnectorAt(pickConns, data.shapes, w.x, w.y, 8 / zoom)) {
        canvasRef.current!.style.cursor = 'move' // déplacer la portion du connecteur
      } else {
        canvasRef.current!.style.cursor = 'default'
      }
    }
  }

  const handleMouseUp = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const w = getWorldPos(e)

    // End gesture coalescing. Any mutate occurring later in this handler (e.g. the
    // connector just drawn) is recorded as its own history entry.
    inGestureRef.current = false
    gesturePushedRef.current = false

    panRef.current = null

    // Fin de déplacement : le surlignage vert provisoire disparaît (positionnement
    // accepté), on revient à la couleur normale.
    const hadGreen = magnetSegsRef.current !== null || alignGuidesRef.current !== null
    magnetSegsRef.current = null
    alignGuidesRef.current = null

    pendingNodeRef.current = null
    if (labelDragRef.current) { labelDragRef.current = null; return }
    if (segDragRef.current) { segDragRef.current = null; if (hadGreen) requestRender(); return }
    if (connDragRef.current) { connDragRef.current = null; if (hadGreen) requestRender(); return }

    if (dragRef.current) {
      dragRef.current = null
      if (hadGreen) requestRender()
      return
    }

    // Finish connector — un connecteur DOIT relier deux formes distinctes.
    // Relâché dans le vide (ou sans forme source) → annulé, rien n'est créé.
    if (drawingConn) {
      // Cible = port sous le curseur, sinon n'importe quelle forme survolée (corps
      // entier) → relier deux objets ne demande pas de viser un port au pixel près.
      const port = getPortAt(pickShapes, w.x, w.y)
      const targetId = port ? port.shapeId : (getShapeAt(pickShapes, w.x, w.y)?.id ?? null)
      if (drawingConn.sourceId && targetId && targetId !== drawingConn.sourceId) {
        const conn: DiagramConnector = {
          id: makeId(),
          sourceId: drawingConn.sourceId,
          targetId,
          sourcePoint: null,
          targetPoint: null,
          waypoints: [],
          label: '',
          style: { ...DEFAULT_CONN_STYLE },
          layerId: activeLayerId,
        }
        mutateData((d) => ({ ...d, connectors: [...d.connectors, conn] }))
      }
      setDrawingConn(null)
      return
    }

    // Finish lasso
    if (lasso) {
      const x1 = Math.min(lasso.x1, lasso.x2)
      const y1 = Math.min(lasso.y1, lasso.y2)
      const x2 = Math.max(lasso.x1, lasso.x2)
      const y2 = Math.max(lasso.y1, lasso.y2)
      if (Math.abs(x2 - x1) > 5 || Math.abs(y2 - y1) > 5) {
        const inside = new Set(
          data.shapes
            .filter((s) => isPickable(s) && s.x >= x1 && s.x + s.w <= x2 && s.y >= y1 && s.y + s.h <= y2)
            .map((s) => s.id),
        )
        setSelectedIds(inside)
        selRef.current = inside
      }
      setLasso(null)
      return
    }
  }

  const handleDblClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const w = getWorldPos(e)

    // Double-clic sur le label d'un connecteur (même déplacé) → l'éditer.
    const mctx = canvasRef.current?.getContext('2d')
    for (let i = data.connectors.length - 1; i >= 0; i--) {
      const c = data.connectors[i]
      if (!c.label) continue
      const center = connectorLabelCenter(c, data.shapes)
      let halfW = 24
      if (mctx) { mctx.save(); mctx.font = '12px Inter, sans-serif'; halfW = mctx.measureText(c.label).width / 2 + 6; mctx.restore() }
      if (Math.abs(w.x - center.x) <= halfW && Math.abs(w.y - center.y) <= 11) {
        const sel = new Set([c.id]); setSelectedConnIds(sel); selConnRef.current = sel
        startLabelEdit(c.id, true, c.label)
        return
      }
    }

    const shape = getShapeAt(pickShapes, w.x, w.y)
    if (shape) { startLabelEdit(shape.id, false, shape.label); return }

    // Double-clic sur un nœud (waypoint) d'un connecteur sélectionné → le retirer.
    const tol = (HANDLE_R + 8) / zoom
    for (const c of data.connectors) {
      if (!selectedConnIds.has(c.id)) continue
      const wi = (c.waypoints ?? []).findIndex((p) => Math.hypot(p.x - w.x, p.y - w.y) <= tol)
      if (wi >= 0) {
        mutateData((d) => ({ ...d, connectors: d.connectors.map((cc) => cc.id === c.id
          ? { ...cc, waypoints: (cc.waypoints ?? []).filter((_, i) => i !== wi) } : cc) }))
        return
      }
    }

    // Double-clic sur la ligne d'un connecteur → éditer son label.
    const hitConn = getConnectorAt(pickConns, data.shapes, w.x, w.y, 10 / zoom)
    if (hitConn) {
      const conn = data.connectors.find((c) => c.id === hitConn.id)!
      const sel = new Set([conn.id]); setSelectedConnIds(sel); selConnRef.current = sel
      startLabelEdit(conn.id, true, conn.label)
    }
  }

  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault()
    const rect = canvasRef.current!.getBoundingClientRect()
    const cx = e.clientX - rect.left
    const cy = e.clientY - rect.top
    const delta = e.deltaY > 0 ? 0.9 : 1.1
    const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * delta))
    setPanX((p) => cx - (cx - p) * (newZoom / zoom))
    setPanY((p) => cy - (cy - p) * (newZoom / zoom))
    setZoom(newZoom)
  }

  // ── Keyboard shortcuts ─────────────────────────────────────────────────────

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (editingLabel) return
      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (document.activeElement !== document.body) return
        mutateData((d) => ({
          shapes: d.shapes.filter((s) => !selectedIds.has(s.id)),
          connectors: d.connectors.filter(
            (c) => !selectedConnIds.has(c.id) && !selectedIds.has(c.sourceId ?? '') && !selectedIds.has(c.targetId ?? ''),
          ),
        }))
        setSelectedIds(new Set())
        setSelectedConnIds(new Set())
      }
      if (e.key === 'Escape') {
        setSelectedIds(new Set())
        setSelectedConnIds(new Set())
        setDrawingConn(null)
        setLasso(null)
        setPendingStencil(null)
      }
      if (e.key === 'a' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault()
        setSelectedIds(new Set(data.shapes.map((s) => s.id)))
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [selectedIds, selectedConnIds, editingLabel, data.shapes, mutateData])

  // ── Alignment actions ──────────────────────────────────────────────────────

  const align = (axis: string) => {
    if (selectedIds.size < 2) return
    const shapes = data.shapes.filter((s) => selectedIds.has(s.id))
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) => {
        if (!selectedIds.has(s.id)) return s
        switch (axis) {
          case 'left':   return { ...s, x: Math.min(...shapes.map((sh) => sh.x)) }
          case 'center': return { ...s, x: Math.round((Math.min(...shapes.map((sh) => sh.x)) + Math.max(...shapes.map((sh) => sh.x + sh.w))) / 2) - Math.round(s.w / 2) }
          case 'right':  return { ...s, x: Math.max(...shapes.map((sh) => sh.x + sh.w)) - s.w }
          case 'top':    return { ...s, y: Math.min(...shapes.map((sh) => sh.y)) }
          case 'middle': return { ...s, y: Math.round((Math.min(...shapes.map((sh) => sh.y)) + Math.max(...shapes.map((sh) => sh.y + sh.h))) / 2) - Math.round(s.h / 2) }
          case 'bottom': return { ...s, y: Math.max(...shapes.map((sh) => sh.y + sh.h)) - s.h }
          default: return s
        }
      }),
    }))
  }

  // Distribute selected shapes evenly (≥3) on an axis: equal gaps between centres.
  const distribute = (axis: 'h' | 'v') => {
    if (selectedIds.size < 3) return
    const sel = data.shapes.filter((s) => selectedIds.has(s.id))
    const sorted = [...sel].sort((a, b) => axis === 'h' ? (a.x + a.w / 2) - (b.x + b.w / 2) : (a.y + a.h / 2) - (b.y + b.h / 2))
    const first = sorted[0], last = sorted[sorted.length - 1]
    const c0 = axis === 'h' ? first.x + first.w / 2 : first.y + first.h / 2
    const c1 = axis === 'h' ? last.x + last.w / 2 : last.y + last.h / 2
    const stepGap = (c1 - c0) / (sorted.length - 1)
    const target = new Map<string, number>()
    sorted.forEach((s, i) => target.set(s.id, c0 + stepGap * i))
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) => {
        if (!target.has(s.id)) return s
        const c = target.get(s.id)!
        return axis === 'h' ? { ...s, x: Math.round(c - s.w / 2) } : { ...s, y: Math.round(c - s.h / 2) }
      }),
    }))
  }

  // ── Order (z-order) for the whole selection ────────────────────────────────
  const reorderSelection = (mode: 'front' | 'back' | 'forward' | 'backward') => {
    if (!selectedIds.size) return
    mutateData((d) => {
      const sel = d.shapes.filter((s) => selectedIds.has(s.id))
      const rest = d.shapes.filter((s) => !selectedIds.has(s.id))
      if (mode === 'front') return { ...d, shapes: [...rest, ...sel] }
      if (mode === 'back')  return { ...d, shapes: [...sel, ...rest] }
      // forward / backward: shift each selected shape by one within the array.
      const arr = [...d.shapes]
      const indices = arr.map((s, i) => ({ s, i })).filter((o) => selectedIds.has(o.s.id)).map((o) => o.i)
      const order = mode === 'forward' ? [...indices].reverse() : indices
      for (const i of order) {
        const j = mode === 'forward' ? i + 1 : i - 1
        if (j < 0 || j >= arr.length || selectedIds.has(arr[j].id)) continue
        ;[arr[i], arr[j]] = [arr[j], arr[i]]
      }
      return { ...d, shapes: arr }
    })
  }

  // ── Group / ungroup ─────────────────────────────────────────────────────────
  const groupSelection = () => {
    if (selectedIds.size < 2) return
    const gid = makeId()
    mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => selectedIds.has(s.id) ? { ...s, groupId: gid } : s) }))
  }
  const ungroupSelection = () => {
    if (!selectedIds.size) return
    mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => selectedIds.has(s.id) ? { ...s, groupId: null } : s) }))
  }
  const selectionHasGroup = data.shapes.some((s) => selectedIds.has(s.id) && s.groupId)

  // ── Flip (mirror) ─────────────────────────────────────────────────────────
  const flipSelection = (axis: 'h' | 'v') => {
    if (!selectedIds.size) return
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) => selectedIds.has(s.id)
        ? (axis === 'h' ? { ...s, flipH: !s.flipH } : { ...s, flipV: !s.flipV })
        : s),
    }))
  }

  // ── Clipboard (copy / cut / paste / duplicate) ──────────────────────────────
  const copySelection = useCallback(() => {
    const shapes = dataRef.current.shapes.filter((s) => selRef.current.has(s.id))
    if (!shapes.length && !selConnRef.current.size) return
    const ids = new Set(shapes.map((s) => s.id))
    // Keep connectors whose BOTH endpoints are within the copied shape set, plus any
    // explicitly selected connectors with both endpoints copied.
    const connectors = dataRef.current.connectors.filter(
      (c) => c.sourceId && c.targetId && ids.has(c.sourceId) && ids.has(c.targetId),
    )
    clipboardRef.current = cloneData({ shapes, connectors })
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const pasteClipboard = useCallback((dx = 20, dy = 20, at?: { x: number; y: number }) => {
    const clip = clipboardRef.current
    if (!clip || !clip.shapes.length) return
    if (at) {
      const minX = Math.min(...clip.shapes.map((s) => s.x))
      const minY = Math.min(...clip.shapes.map((s) => s.y))
      dx = Math.round(at.x - minX); dy = Math.round(at.y - minY)
    }
    const idMap = new Map<string, string>()
    const groupMap = new Map<string, string>()
    const baseZ = dataRef.current.shapes.length
    const newShapes = clip.shapes.map((s, i) => {
      const nid = makeId(); idMap.set(s.id, nid)
      let gid = s.groupId ?? null
      if (gid) { if (!groupMap.has(gid)) groupMap.set(gid, makeId()); gid = groupMap.get(gid)! }
      return { ...s, id: nid, x: s.x + dx, y: s.y + dy, style: { ...s.style }, labelStyle: { ...s.labelStyle }, groupId: gid, zIndex: baseZ + i }
    })
    const newConns = clip.connectors
      .filter((c) => c.sourceId && c.targetId && idMap.has(c.sourceId) && idMap.has(c.targetId))
      .map((c) => ({ ...c, id: makeId(), sourceId: idMap.get(c.sourceId!)!, targetId: idMap.get(c.targetId!)!, style: { ...c.style }, waypoints: (c.waypoints ?? []).map((p) => ({ x: p.x + dx, y: p.y + dy })) }))
    mutateData((d) => ({ shapes: [...d.shapes, ...newShapes], connectors: [...d.connectors, ...newConns] }))
    const sel = new Set(newShapes.map((s) => s.id))
    setSelectedIds(sel); selRef.current = sel
    setSelectedConnIds(new Set()); selConnRef.current = new Set()
  }, [mutateData])

  const cutSelection = useCallback(() => {
    copySelection()
    if (!selRef.current.size && !selConnRef.current.size) return
    mutateData((d) => ({
      shapes: d.shapes.filter((s) => !selRef.current.has(s.id)),
      connectors: d.connectors.filter((c) => !selConnRef.current.has(c.id) && !selRef.current.has(c.sourceId ?? '') && !selRef.current.has(c.targetId ?? '')),
    }))
    setSelectedIds(new Set()); selRef.current = new Set()
    setSelectedConnIds(new Set()); selConnRef.current = new Set()
  }, [copySelection, mutateData])

  const duplicateSelection = useCallback(() => {
    copySelection()
    pasteClipboard(20, 20)
  }, [copySelection, pasteClipboard])

  // Quick-insert a shape by stencil id at the centre of the visible canvas.
  const insertShape = (type: string) => {
    const st = STENCIL_MAP[type]
    if (!st) return
    const rect = containerRef.current?.getBoundingClientRect()
    const cx = rect ? rect.width / 2 : 300
    const cy = rect ? rect.height / 2 : 200
    const w = canvasToWorld(cx, cy, panX, panY, zoom)
    placeStencil(st, w.x, w.y)
  }

  // Zoom & pan so all content fits the viewport (draw.io's "Fit page").
  const zoomToFit = () => {
    const all = data.shapes
    const rect = containerRef.current?.getBoundingClientRect()
    if (!rect) return
    if (!all.length) { setZoom(1); setPanX(60); setPanY(60); return }
    const minX = Math.min(...all.map((s) => s.x))
    const minY = Math.min(...all.map((s) => s.y))
    const maxX = Math.max(...all.map((s) => s.x + s.w))
    const maxY = Math.max(...all.map((s) => s.y + s.h))
    const pad = 50
    const z = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.min(
      (rect.width - 2 * pad) / Math.max(1, maxX - minX),
      (rect.height - 2 * pad) / Math.max(1, maxY - minY),
    )))
    setZoom(z)
    setPanX(pad - minX * z)
    setPanY(pad - minY * z)
  }

  // Auto-layout (applies to the selection if ≥2 shapes are selected, else all).
  const applyLayout = (kind: LayoutKind) => {
    const ids = selectedIds.size >= 2 ? selectedIds : null
    const pos = computeLayout(kind, data.shapes, data.connectors, ids)
    if (!pos.size) return
    mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => pos.has(s.id) ? { ...s, x: pos.get(s.id)!.x, y: pos.get(s.id)!.y } : s) }))
  }

  // Render the diagram (visible layers) to an offscreen canvas at content bounds,
  // reusing renderCanvas so it's pixel-identical to the editor (no grid/handles).
  const renderExportCanvas = (opaqueBg: boolean): { c: HTMLCanvasElement; w: number; h: number } | null => {
    const shapes = visibleData.shapes
    const pts: { x: number; y: number }[] = []
    for (const s of shapes) { pts.push({ x: s.x, y: s.y }, { x: s.x + s.w, y: s.y + s.h }) }
    for (const c of visibleData.connectors) for (const p of displayPoints(c, shapes)) pts.push(p)
    if (!pts.length) return null
    const minX = Math.min(...pts.map((p) => p.x)), minY = Math.min(...pts.map((p) => p.y))
    const maxX = Math.max(...pts.map((p) => p.x)), maxY = Math.max(...pts.map((p) => p.y))
    const pad = 24, scale = 2
    const Wc = (maxX - minX) + 2 * pad, Hc = (maxY - minY) + 2 * pad
    const dpr = window.devicePixelRatio || 1
    const c = document.createElement('canvas')
    c.width = Math.ceil(Wc * scale * dpr); c.height = Math.ceil(Hc * scale * dpr)
    renderCanvas(
      c, visibleData, scale, (pad - minX) * scale, (pad - minY) * scale,
      new Set(), new Set(), null, null, null,
      opaqueBg ? (bgColor || '#ffffff') : (bgColor || '#ffffff'),
      null, false, GRID_SIZE, null,
    )
    return { c, w: c.width, h: c.height }
  }

  const exportImage = (format: 'png' | 'jpeg') => {
    const r = renderExportCanvas(format === 'jpeg')
    if (!r) return
    r.c.toBlob((blob) => {
      if (!blob) return
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url; a.download = `${title || 'diagramme'}.${format === 'jpeg' ? 'jpg' : 'png'}`; a.click()
      URL.revokeObjectURL(url)
    }, format === 'png' ? 'image/png' : 'image/jpeg', 0.92)
  }

  // Export a single-page PDF embedding the rasterised diagram (JPEG/DCTDecode).
  const exportPdf = () => {
    const r = renderExportCanvas(true)
    if (!r) return
    r.c.toBlob(async (blob) => {
      if (!blob) return
      const jpeg = new Uint8Array(await blob.arrayBuffer())
      const pdf = buildJpegPdf(jpeg, r.w, r.h)
      const url = URL.createObjectURL(new Blob([pdf as unknown as BlobPart], { type: 'application/pdf' }))
      const a = document.createElement('a')
      a.href = url; a.download = `${title || 'diagramme'}.pdf`; a.click()
      URL.revokeObjectURL(url)
    }, 'image/jpeg', 0.92)
  }

  // Apply a coordinated colour theme to the selection (or the whole diagram).
  const applyTheme = (fill: string, stroke: string, textColor: string, connColor: string) => {
    const targetShapes = (s: DiagramShape) => selectedIds.size ? selectedIds.has(s.id) : true
    const targetConns = (c: DiagramConnector) => selectedConnIds.size ? selectedConnIds.has(c.id) : !selectedIds.size
    mutateData((d) => ({
      ...d,
      shapes: d.shapes.map((s) => targetShapes(s) ? { ...s, style: { ...s.style, fillColor: fill, strokeColor: stroke }, labelStyle: { ...s.labelStyle, color: textColor } } : s),
      connectors: d.connectors.map((c) => targetConns(c) ? { ...c, style: { ...c.style, strokeColor: connColor } } : c),
    }))
  }

  // Export to draw.io / mxGraph XML (uncompressed, openable in draw.io).
  const exportDrawio = () => {
    const io: IoData = { shapes: visibleData.shapes as unknown as IoData['shapes'], connectors: visibleData.connectors as unknown as IoData['connectors'] }
    const xml = toDrawioXml(io, title || 'diagramme')
    const blob = new Blob([xml], { type: 'application/xml' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url; a.download = `${title || 'diagramme'}.drawio`; a.click()
    URL.revokeObjectURL(url)
  }

  // Merge imported shapes/connectors into the current page (fresh ids, active layer).
  const importIoData = (io: IoData) => {
    const idMap = new Map<string, string>()
    const baseZ = data.shapes.length
    const shapes = io.shapes.map((s, i) => {
      const nid = makeId(); idMap.set(s.id, nid)
      return { ...s, id: nid, labelStyle: {}, zIndex: baseZ + i, layerId: activeLayerId } as unknown as DiagramShape
    })
    const conns = io.connectors.map((c) => ({
      ...c, id: makeId(),
      sourceId: c.sourceId ? idMap.get(c.sourceId) ?? null : null,
      targetId: c.targetId ? idMap.get(c.targetId) ?? null : null,
      layerId: activeLayerId,
    }) as unknown as DiagramConnector)
    mutateData((d) => ({ ...d, shapes: [...d.shapes, ...shapes], connectors: [...d.connectors, ...conns] }))
    const sel = new Set(shapes.map((s) => s.id)); setSelectedIds(sel); selRef.current = sel
  }

  const importFileRef = useRef<HTMLInputElement>(null)
  const onImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0]
    e.target.value = ''
    if (!f) return
    const text = await f.text()
    const io = f.name.toLowerCase().endsWith('.csv') ? fromCsv(text) : fromDrawioXml(text)
    if (!io || !io.shapes.length) {
      await confirm({
        title: t('diag_import_failed_title', { defaultValue: 'Import impossible' }),
        message: t('diag_import_failed_msg', { defaultValue: 'Fichier non reconnu ou vide. Formats acceptés : .drawio / .xml (mxGraph, compressé ou non) et .csv.' }),
        confirmLabel: 'OK',
      })
      return
    }
    importIoData(io)
  }

  // Render an IoData to a small preview data URL (for the templates gallery).
  const renderIoPreview = (io: IoData, W = 168, H = 112): string | null => {
    if (!io.shapes.length) return null
    const pts: { x: number; y: number }[] = []
    for (const s of io.shapes) pts.push({ x: s.x, y: s.y }, { x: s.x + s.w, y: s.y + s.h })
    const minX = Math.min(...pts.map((p) => p.x)), minY = Math.min(...pts.map((p) => p.y))
    const maxX = Math.max(...pts.map((p) => p.x)), maxY = Math.max(...pts.map((p) => p.y))
    const pad = 10, cw = (maxX - minX) + 2 * pad, ch = (maxY - minY) + 2 * pad
    const scale = Math.min(W / cw, H / ch)
    const dpr = window.devicePixelRatio || 1
    const c = document.createElement('canvas'); c.width = W * dpr; c.height = H * dpr
    renderCanvas(
      c, io as unknown as DiagramData, scale,
      (pad - minX) * scale + (W - cw * scale) / 2, (pad - minY) * scale + (H - ch * scale) / 2,
      new Set(), new Set(), null, null, null, '#ffffff', null, false, GRID_SIZE, null,
    )
    return c.toDataURL()
  }

  // Apply a fill/stroke preset to the whole current selection.
  const applyStyleToSelection = (patch: Partial<ShapeStyle>) => {
    if (!selectedIds.size) return
    mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => selectedIds.has(s.id) ? { ...s, style: { ...s.style, ...patch } } : s) }))
  }

  // Centre the viewport on a world point clicked in the minimap.
  const minimapJump = (wx: number, wy: number) => {
    const vw = canvasRef.current?.clientWidth ?? 800
    const vh = canvasRef.current?.clientHeight ?? 600
    setPanX(vw / 2 - wx * zoom)
    setPanY(vh / 2 - wy * zoom)
  }

  // Undo/redo + clipboard keyboard shortcuts (ignored while typing in a field).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return
      if (editingLabel) return
      const ae = document.activeElement as HTMLElement | null
      if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || ae.isContentEditable)) return
      const k = e.key.toLowerCase()
      if (k === 'z')      { e.preventDefault(); if (e.shiftKey) redo(); else undo() }
      else if (k === 'y') { e.preventDefault(); redo() }
      else if (k === 'c') { e.preventDefault(); copySelection() }
      else if (k === 'x') { e.preventDefault(); cutSelection() }
      else if (k === 'v') { e.preventDefault(); pasteClipboard() }
      else if (k === 'd') { e.preventDefault(); duplicateSelection() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [editingLabel, undo, redo, copySelection, cutSelection, pasteClipboard, duplicateSelection])

  // ── Place stencil ──────────────────────────────────────────────────────────

  const placeStencil = useCallback((stencil: StencilDef, wx: number, wy: number) => {
    const shape: DiagramShape = {
      id:         makeId(),
      type:       stencil.id,
      x:          snapToGrid(wx - stencil.defaultW / 2),
      y:          snapToGrid(wy - stencil.defaultH / 2),
      w:          stencil.defaultW,
      h:          stencil.defaultH,
      // Les icônes matériel se suffisent à elles-mêmes → pas de libellé par défaut.
      label:      stencil.id.startsWith('hw_') ? '' : t('stencil_' + stencil.id, { defaultValue: stencil.name }),
      style:      { ...stencil.style },
      labelStyle: {},
      zIndex:     dataRef.current.shapes.length,
      layerId:    activeLayerIdRef.current,
    }
    mutateData((d) => ({ ...d, shapes: [...d.shapes, shape] }))
    const newSel = new Set([shape.id])
    setSelectedIds(newSel)
    selRef.current = newSel
  }, [mutateData, t])

  // Drag from stencil panel
  const handleStencilDragStart = (stencil: StencilDef) => {
    dragStencilRef.current = stencil
  }

  const handleCanvasDragOver = (e: React.DragEvent<HTMLCanvasElement>) => {
    e.preventDefault()
  }

  const handleCanvasDrop = (e: React.DragEvent<HTMLCanvasElement>) => {
    e.preventDefault()
    if (!dragStencilRef.current) return
    const rect = canvasRef.current!.getBoundingClientRect()
    const w = canvasToWorld(e.clientX - rect.left, e.clientY - rect.top, panX, panY, zoom)
    placeStencil(dragStencilRef.current, w.x, w.y)
    dragStencilRef.current = null
  }

  // ── Delete button ──────────────────────────────────────────────────────────

  const deleteSelected = () => {
    mutateData((d) => ({
      shapes: d.shapes.filter((s) => !selectedIds.has(s.id)),
      connectors: d.connectors.filter(
        (c) => !selectedConnIds.has(c.id) && !selectedIds.has(c.sourceId ?? '') && !selectedIds.has(c.targetId ?? ''),
      ),
    }))
    setSelectedIds(new Set())
    setSelectedConnIds(new Set())
  }

  // ── Page management ────────────────────────────────────────────────────────

  const [renamingPage, setRenamingPage] = useState<string | null>(null)
  const [renameVal,    setRenameVal]    = useState('')

  const createPageMut = useMutation({
    mutationFn: () => diagramsApi.createPage(id!, { name: t('diag_page_n', { n: pageList.length + 1 }) }),
    onSuccess: (page) => {
      qc.invalidateQueries({ queryKey: ['diagram', id] })
      setCurrentPageId(page.id)
    },
  })

  const deletePageMut = useMutation({
    mutationFn: (pid: string) => diagramsApi.deletePage(id!, pid),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['diagram', id] })
    },
  })

  const renamePageMut = useMutation({
    mutationFn: ({ pid, name }: { pid: string; name: string }) =>
      diagramsApi.updatePageMeta(id!, pid, { name }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['diagram', id] }),
  })

  const commitPageRename = (pid: string) => {
    if (renameVal.trim()) renamePageMut.mutate({ pid, name: renameVal.trim() })
    setRenamingPage(null)
  }

  // ── Stencil search ─────────────────────────────────────────────────────────

  const [stencilSearch, setStencilSearch] = useState('')
  const categories = getCategories()
  const CAT_LABELS: Record<string, string> = {
    basic: 'Formes basiques', flow: 'Flux', er: 'Entité-association', bpmn: 'BPMN',
    network: 'Réseau', uml: 'UML', aws: 'AWS', k8s: 'Kubernetes',
    mockup: 'Maquettes', container: 'Conteneurs', hardware: 'Ordinateur et Matériel',
  }
  // Accordion: a set of expanded category ids (the first one open by default).
  const [expandedCats, setExpandedCats] = useState<Set<string>>(() => new Set([getCategories()[0]]))
  const toggleCat = (c: string) => setExpandedCats((prev) => { const n = new Set(prev); n.has(c) ? n.delete(c) : n.add(c); return n })
  const stencilPanelRef = useRef<HTMLDivElement>(null)
  // Hover preview (draw.io-style large preview card to the right of the panel).
  const [hoverStencil, setHoverStencil] = useState<{ s: StencilDef; top: number } | null>(null)

  const searchResults = stencilSearch
    ? searchStencils(stencilSearch, s => t('stencil_' + s.id, { defaultValue: s.name }))
    : []

  // ── Export ─────────────────────────────────────────────────────────────────

  const exportJson = () => {
    diagramsApi.exportJson(id!).then((d) => {
      const blob = new Blob([JSON.stringify(d, null, 2)], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url; a.download = `${title}.json`; a.click()
      URL.revokeObjectURL(url)
    })
  }

  // ── Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du
  //    ruban. Doit être appelé AVANT tout return anticipé (règles des hooks). ──────
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_DIAGRAMS,
    labels: backstageLabels(t),
    startContent: <DiagramsStartContent onOpen={(did) => navigate(`/office/diagrams/${did}`)} />,
    defaultTab: 'home',
    doc: {
      info: (
        <InfoPanel
          title={diagram?.title || t('common_untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('diagrams_title', { defaultValue: 'Diagramme' })}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('diagrams_title', { defaultValue: 'Diagramme' })],
            [t('diag_grp_shapes', { defaultValue: 'Formes' }), data.shapes.length],
            [t('diag_connector', { defaultValue: 'Connecteurs' }), data.connectors.length],
            [t('diag_panel_pages', { defaultValue: 'Pages' }), pageList.length],
            ...(diagram?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(diagram.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: () => navigate('/office/diagrams'),
    },
  })

  // ── Render ─────────────────────────────────────────────────────────────────

  if (loadingDiagram) {
    return (
      <div className="flex items-center justify-center h-full text-text-tertiary">
        <Spinner size="md" className="mr-2" />
        {t('diag_loading')}
      </div>
    )
  }

  const hasSel = selectedIds.size > 0 || selectedConnIds.size > 0

  const canUndo = pastRef.current.length > 0
  const canRedo = futureRef.current.length > 0
  const hasClip = !!clipboardRef.current?.shapes.length
  const nSel    = selectedIds.size

  const alignItems = ([
    ['left', <AlignLeft size={15} />, t('diag_align_left')], ['center', <AlignCenter size={15} />, t('diag_align_center_h')], ['right', <AlignRight size={15} />, t('diag_align_right')],
    ['top', <AlignStartVertical size={15} />, t('diag_align_top')], ['middle', <AlignCenterVertical size={15} />, t('diag_align_center_v')], ['bottom', <AlignEndVertical size={15} />, t('diag_align_bottom')],
  ] as Array<[string, React.ReactNode, string]>).map(([a, icon, label]) => ({ id: 'al-' + a, kind: 'button' as const, icon, label, disabled: nSel < 2, onClick: () => align(a) }))

  const insertShapes: Array<[string, React.ReactNode, string]> = [
    ['rect', <Square size={15} />, t('stencil_rect', { defaultValue: 'Rectangle' })],
    ['rounded_rect', <Square size={15} />, t('stencil_rounded_rect', { defaultValue: 'Rect. arrondi' })],
    ['ellipse', <Circle size={15} />, t('stencil_ellipse', { defaultValue: 'Ellipse' })],
    ['diamond', <Diamond size={15} />, t('stencil_diamond', { defaultValue: 'Losange' })],
    ['hexagon', <Hexagon size={15} />, t('stencil_hexagon', { defaultValue: 'Hexagone' })],
    ['triangle', <Triangle size={15} />, t('stencil_triangle', { defaultValue: 'Triangle' })],
    ['cylinder', <Database size={15} />, t('stencil_cylinder', { defaultValue: 'Cylindre' })],
    ['cloud', <Cloud size={15} />, t('stencil_cloud', { defaultValue: 'Nuage' })],
  ]

  const diagRibbon: RibbonTab[] = [
    // ── Accueil ──
    // Le groupe « Fichier » historique a été retiré : les opérations sur le fichier
    // (Nouveau/Dupliquer/Importer/Exporter) vivent désormais dans le backstage
    // (onglet « Fichier »). On conserve Nouveau/Dupliquer + Importer/Exporter dans un
    // groupe « Diagramme » du 1ᵉʳ onglet visible (Accueil), handlers inchangés.
    { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
      { id: 'diagram', label: t('diagrams_title', { defaultValue: 'Diagramme' }), items: [
        { id: 'new', kind: 'button', size: 'large', icon: <Plus size={18} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: () => createDiagMut.mutate() },
        { id: 'dup', kind: 'button', icon: <Copy size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: () => duplicateDiagMut.mutate() },
        { id: 'import',   kind: 'button', icon: <Upload size={15} />, label: t('diag_import', { defaultValue: 'Importer' }), tooltip: t('diag_import_tip', { defaultValue: 'Importer .drawio / .csv' }), onClick: () => importFileRef.current?.click() },
        { id: 'exp-png',  kind: 'button', icon: <Download size={15} />, label: t('diag_export_png', { defaultValue: 'Export PNG' }), onClick: () => exportImage('png') },
        { id: 'exp-jpg',  kind: 'button', icon: <Download size={15} />, label: t('diag_export_jpg', { defaultValue: 'Export JPEG' }), onClick: () => exportImage('jpeg') },
        { id: 'exp-pdf',  kind: 'button', icon: <Download size={15} />, label: t('diag_export_pdf', { defaultValue: 'Export PDF' }), onClick: exportPdf },
        { id: 'exp-drawio', kind: 'button', icon: <Download size={15} />, label: t('diag_export_drawio', { defaultValue: 'Export .drawio' }), onClick: exportDrawio },
        { id: 'exp-json', kind: 'button', icon: <Download size={15} />, label: t('diag_export_json', { defaultValue: 'Export JSON' }), onClick: exportJson },
      ] },
      { id: 'history', label: t('diag_grp_history', { defaultValue: 'Historique' }), items: [
        { id: 'undo', kind: 'button', icon: <Undo2 size={15} />, label: t('diag_undo', { defaultValue: 'Annuler' }), shortcut: 'Ctrl+Z', disabled: !canUndo, onClick: undo },
        { id: 'redo', kind: 'button', icon: <Redo2 size={15} />, label: t('diag_redo', { defaultValue: 'Rétablir' }), shortcut: 'Ctrl+Y', disabled: !canRedo, onClick: redo },
      ] },
      { id: 'clip', label: t('diag_grp_clipboard', { defaultValue: 'Presse-papiers' }), items: [
        { id: 'paste', kind: 'button', size: 'large', icon: <ClipboardPaste size={18} />, label: t('diag_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', disabled: !hasClip, onClick: () => pasteClipboard() },
        { id: 'cut', kind: 'button', icon: <Scissors size={15} />, label: t('diag_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', disabled: !hasSel, onClick: cutSelection },
        { id: 'copy', kind: 'button', icon: <Copy size={15} />, label: t('diag_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', disabled: !hasSel, onClick: copySelection },
        { id: 'dup', kind: 'button', icon: <Copy size={15} />, label: t('diag_ctx_duplicate', { defaultValue: 'Dupliquer' }), shortcut: 'Ctrl+D', disabled: nSel < 1, onClick: duplicateSelection },
      ] },
      { id: 'edit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete', { defaultValue: 'Supprimer' }), shortcut: 'Suppr', disabled: !hasSel, onClick: deleteSelected },
        { id: 'props', kind: 'button', icon: <Network size={15} />, label: t('diag_properties'), onClick: () => dockRef.current?.open('format') },
      ] },
    ] },
    // ── Insertion ──
    { id: 'insert', label: t('diag_tab_insert', { defaultValue: 'Insertion' }), groups: [
      { id: 'ins-shapes', label: t('diag_grp_shapes', { defaultValue: 'Formes' }), items:
        insertShapes.map(([type, icon, label]) => ({ id: 'ins-' + type, kind: 'button' as const, icon, label, onClick: () => insertShape(type) })) },
      { id: 'ins-text', label: t('diag_text', { defaultValue: 'Texte' }), items: [
        { id: 'ins-text-b', kind: 'button', size: 'large', icon: <Type size={18} />, label: t('stencil_text', { defaultValue: 'Texte' }), onClick: () => insertShape('text') },
      ] },
      { id: 'ins-templates', label: t('diag_templates', { defaultValue: 'Modèles' }), items: [
        { id: 'templates', kind: 'button', size: 'large', icon: <LayoutGrid size={18} />, label: t('diag_templates', { defaultValue: 'Modèles' }), onClick: () => setShowTemplates(true) },
      ] },
    ] },
    // ── Disposition ──
    { id: 'arrange', label: t('diag_tab_arrange', { defaultValue: 'Disposition' }), groups: [
      { id: 'align', label: t('doc_grp_arrange', { defaultValue: 'Alignement' }), items: alignItems },
      { id: 'distribute', label: t('diag_grp_distribute', { defaultValue: 'Distribuer' }), items: [
        { id: 'dist-h', kind: 'button', icon: <AlignHorizontalDistributeCenter size={15} />, label: t('diag_distribute_h', { defaultValue: 'Horizontalement' }), disabled: nSel < 3, onClick: () => distribute('h') },
        { id: 'dist-v', kind: 'button', icon: <AlignVerticalDistributeCenter size={15} />, label: t('diag_distribute_v', { defaultValue: 'Verticalement' }), disabled: nSel < 3, onClick: () => distribute('v') },
      ] },
      { id: 'order', label: t('diag_grp_order', { defaultValue: 'Ordre' }), items: [
        { id: 'front', kind: 'button', icon: <ArrowUpToLine size={15} />, label: t('diag_ctx_to_front', { defaultValue: 'Premier plan' }), disabled: !nSel, onClick: () => reorderSelection('front') },
        { id: 'forward', kind: 'button', icon: <ChevronUp size={15} />, label: t('diag_ctx_forward', { defaultValue: 'Avancer' }), disabled: !nSel, onClick: () => reorderSelection('forward') },
        { id: 'backward', kind: 'button', icon: <ChevronDown size={15} />, label: t('diag_ctx_backward', { defaultValue: 'Reculer' }), disabled: !nSel, onClick: () => reorderSelection('backward') },
        { id: 'back', kind: 'button', icon: <ArrowDownToLine size={15} />, label: t('diag_ctx_to_back', { defaultValue: 'Arrière-plan' }), disabled: !nSel, onClick: () => reorderSelection('back') },
      ] },
      { id: 'group', label: t('diag_grp_group', { defaultValue: 'Grouper' }), items: [
        { id: 'grp', kind: 'button', icon: <Group size={15} />, label: t('diag_group', { defaultValue: 'Grouper' }), disabled: nSel < 2, onClick: groupSelection },
        { id: 'ungrp', kind: 'button', icon: <Ungroup size={15} />, label: t('diag_ungroup', { defaultValue: 'Dégrouper' }), disabled: !selectionHasGroup, onClick: ungroupSelection },
      ] },
      { id: 'flip', label: t('diag_grp_flip', { defaultValue: 'Miroir' }), items: [
        { id: 'flip-h', kind: 'button', icon: <FlipHorizontal2 size={15} />, label: t('diag_flip_h', { defaultValue: 'Horizontal' }), disabled: !nSel, onClick: () => flipSelection('h') },
        { id: 'flip-v', kind: 'button', icon: <FlipVertical2 size={15} />, label: t('diag_flip_v', { defaultValue: 'Vertical' }), disabled: !nSel, onClick: () => flipSelection('v') },
      ] },
      { id: 'autolayout', label: t('diag_grp_autolayout', { defaultValue: 'Disposition auto' }), items: [
        { id: 'lay-tb',     kind: 'button', icon: <Workflow size={15} />,   label: t('diag_layout_tb', { defaultValue: 'Hiérarchique ↓' }),   disabled: data.shapes.length < 2, onClick: () => applyLayout('hier_tb') },
        { id: 'lay-lr',     kind: 'button', icon: <GitBranch size={15} />,  label: t('diag_layout_lr', { defaultValue: 'Hiérarchique →' }),   disabled: data.shapes.length < 2, onClick: () => applyLayout('hier_lr') },
        { id: 'lay-circle', kind: 'button', icon: <CircleDot size={15} />,  label: t('diag_layout_circle', { defaultValue: 'Circulaire' }),   disabled: data.shapes.length < 2, onClick: () => applyLayout('circle') },
        { id: 'lay-grid',   kind: 'button', icon: <LayoutGrid size={15} />, label: t('diag_layout_grid', { defaultValue: 'Grille' }),         disabled: data.shapes.length < 2, onClick: () => applyLayout('grid') },
      ] },
    ] },
    // ── Affichage ──
    { id: 'view', label: t('diag_tab_view', { defaultValue: 'Affichage' }), groups: [
      { id: 'zoom', label: t('doc_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'zin', kind: 'button', icon: <ZoomIn size={15} />, label: t('diag_zoom_in', { defaultValue: 'Zoom avant' }), onClick: () => setZoom(z => Math.min(MAX_ZOOM, +(z * 1.2).toFixed(2))) },
        { id: 'zout', kind: 'button', icon: <ZoomOut size={15} />, label: t('diag_zoom_out', { defaultValue: 'Zoom arrière' }), onClick: () => setZoom(z => Math.max(MIN_ZOOM, +(z / 1.2).toFixed(2))) },
        { id: 'zfit', kind: 'button', icon: <Maximize2 size={15} />, label: t('diag_zoom_fit', { defaultValue: 'Ajuster' }), onClick: zoomToFit },
        { id: 'zreset', kind: 'button', icon: <RotateCcw size={15} />, label: t('diag_reset_view', { defaultValue: 'Réinitialiser' }), onClick: () => { setZoom(1); setPanX(60); setPanY(60) } },
      ] },
      { id: 'view-opts', label: t('diag_tab_view', { defaultValue: 'Affichage' }), items: [
        { id: 'grid', kind: 'toggle', icon: <Grid3x3 size={15} />, label: t('diag_grid', { defaultValue: 'Grille' }), active: showGrid, onClick: () => setShowGrid(v => !v) },
        { id: 'snap', kind: 'toggle', icon: <Magnet size={15} />, label: t('diag_snap', { defaultValue: 'Magnétisme' }), active: snapEnabled, onClick: () => setSnapEnabled(v => !v) },
        { id: 'minimap', kind: 'toggle', icon: <MapIcon size={15} />, label: t('diag_minimap', { defaultValue: 'Minimap' }), active: showMinimap, onClick: () => setShowMinimap(v => !v) },
        { id: 'layers', kind: 'button', icon: <Layers size={15} />, label: t('diag_layers', { defaultValue: 'Calques' }), onClick: () => dockRef.current?.open('layers') },
        { id: 'rulers', kind: 'toggle', icon: <RulerIcon size={15} />, label: t('diag_rulers', { defaultValue: 'Règles' }), active: showRulers, onClick: () => setShowRulers(v => !v) },
        { id: 'props2', kind: 'button', icon: <Network size={15} />, label: t('diag_properties'), onClick: () => dockRef.current?.open('format') },
      ] },
      { id: 'themes', label: t('diag_grp_themes', { defaultValue: 'Thèmes' }), items: [
        { id: 'th-blue', kind: 'button', icon: <Palette size={15} className="text-[#6c8ebf]" />, label: t('diag_theme_blue', { defaultValue: 'Bleu' }), onClick: () => applyTheme('#dae8fc', '#6c8ebf', '#000000', '#6c8ebf') },
        { id: 'th-green', kind: 'button', icon: <Palette size={15} className="text-[#82b366]" />, label: t('diag_theme_green', { defaultValue: 'Vert' }), onClick: () => applyTheme('#d5e8d4', '#82b366', '#000000', '#82b366') },
        { id: 'th-gray', kind: 'button', icon: <Palette size={15} className="text-[#666666]" />, label: t('diag_theme_gray', { defaultValue: 'Gris' }), onClick: () => applyTheme('#f5f5f5', '#666666', '#000000', '#666666') },
        { id: 'th-mono', kind: 'button', icon: <Palette size={15} />, label: t('diag_theme_mono', { defaultValue: 'Mono' }), onClick: () => applyTheme('#ffffff', '#000000', '#000000', '#000000') },
        { id: 'th-dark', kind: 'button', icon: <Palette size={15} className="text-[#b0b0b0]" />, label: t('diag_theme_dark', { defaultValue: 'Sombre' }), onClick: () => applyTheme('#2b2b2b', '#b0b0b0', '#ffffff', '#b0b0b0') },
      ] },
    ] },
    // ── Contextuel : connecteur ──
    { id: 'ctx-conn', label: t('diag_connector', { defaultValue: 'Connecteur' }), contextual: { accent: OFFICE_TONE.diagrams }, visible: !!selectedConn, groups: selectedConn ? [
      { id: 'conn-route', label: t('diag_grp_routing', { defaultValue: 'Routage' }), items: [
        { id: 'r-straight', kind: 'toggle', icon: <ArrowLeftRight size={15} />, label: t('diag_route_straight', { defaultValue: 'Direct' }), active: connRouting(selectedConn) === 'straight', onClick: () => updateConnStyle(selectedConn.id, { routing: 'straight' }) },
        { id: 'r-ortho', kind: 'toggle', icon: <Network size={15} />, label: t('diag_route_orthogonal', { defaultValue: 'Orthogonal' }), active: connRouting(selectedConn) === 'orthogonal', onClick: () => updateConnStyle(selectedConn.id, { routing: 'orthogonal' }) },
        { id: 'r-curved', kind: 'toggle', icon: <Spline size={15} />, label: t('diag_route_curved', { defaultValue: 'Courbe' }), active: connRouting(selectedConn) === 'curved', onClick: () => updateConnStyle(selectedConn.id, { routing: 'curved' }) },
      ] },
      { id: 'conn-line', label: t('diag_ctx_line', { defaultValue: 'Trait' }), items: [
        { id: 'l-solid', kind: 'toggle', icon: <Minus size={15} />, label: t('diag_line_solid'), active: selectedConn.style.strokeStyle === 'solid', onClick: () => updateConnStyle(selectedConn.id, { strokeStyle: 'solid' }) },
        { id: 'l-dashed', kind: 'toggle', icon: <Minus size={15} />, label: t('diag_line_dashed'), active: selectedConn.style.strokeStyle === 'dashed', onClick: () => updateConnStyle(selectedConn.id, { strokeStyle: 'dashed' }) },
        { id: 'l-dotted', kind: 'toggle', icon: <Minus size={15} />, label: t('diag_line_dotted'), active: selectedConn.style.strokeStyle === 'dotted', onClick: () => updateConnStyle(selectedConn.id, { strokeStyle: 'dotted' }) },
      ] },
      { id: 'conn-arrow', label: t('diag_ctx_arrow_end', { defaultValue: 'Flèche' }), items: [
        { id: 'arrow-end', kind: 'toggle', icon: <ArrowLeftRight size={15} />, label: t('diag_ctx_arrow_end', { defaultValue: 'Flèche de fin' }), active: selectedConn.style.arrowEnd !== 'none', onClick: () => updateConnStyle(selectedConn.id, { arrowEnd: selectedConn.style.arrowEnd === 'none' ? 'block' : 'none' }) },
        { id: 'reverse', kind: 'button', icon: <ArrowLeftRight size={15} />, label: t('diag_ctx_reverse', { defaultValue: 'Inverser' }), onClick: () => connReverse(selectedConn.id) },
      ] },
      { id: 'conn-edit', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'conn-dup', kind: 'button', icon: <Copy size={15} />, label: t('diag_ctx_duplicate', { defaultValue: 'Dupliquer' }), onClick: () => connDuplicate(selectedConn.id) },
        { id: 'conn-del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete', { defaultValue: 'Supprimer' }), onClick: () => deleteConnector(selectedConn.id) },
      ] },
    ] : [] },
  ]
  // ── Macros API (sous-module Script) ────────────────────────────────────────
  // Read-only surface exposed to macros as the global `Kubuno` object. The macro
  // runs client-side against this live snapshot of the current diagram page.
  const makeApi = () => ({
    Diagram: {
      /** Number of shapes on the current page. */
      getShapeCount: () => data.shapes.length,
      /** Number of connectors on the current page. */
      getConnectorCount: () => data.connectors.length,
      /** Current selection as arrays of shape and connector ids. */
      getSelection: () => ({ shapes: [...selectedIds], connectors: [...selectedConnIds] }),
      /** Shapes as plain { id, type, label } records. */
      getShapes: () => data.shapes.map((s) => ({ id: s.id, type: s.type, label: s.label ?? '' })),
    },
    App: {
      getType: () => 'diagram',
      getId: () => id,
      toast: (m: unknown) => console.log(String(m)),
      log: (m: unknown) => console.log(String(m)),
    },
  })

  // One stencil icon cell (small, no label — label shows in the hover preview).
  const stencilCell = (stencil: StencilDef) => (
    <div
      key={stencil.id}
      draggable
      onDragStart={() => handleStencilDragStart(stencil)}
      onClick={() => setPendingStencil(pendingStencil?.id === stencil.id ? null : stencil)}
      onMouseEnter={(e) => setHoverStencil({ s: stencil, top: (e.currentTarget as HTMLElement).getBoundingClientRect().top })}
      onMouseLeave={() => setHoverStencil(null)}
      title={t('stencil_' + stencil.id, { defaultValue: stencil.name })}
      className={`flex items-center justify-center aspect-square p-1 rounded cursor-pointer border transition-all select-none ${
        pendingStencil?.id === stencil.id ? 'border-primary bg-primary/5' : 'border-transparent hover:bg-surface-2 hover:border-border'
      }`}
    >
      <StencilThumbnail stencil={stencil} w={38} h={30} />
    </div>
  )

  // Docking panels (Formes / Format / Calques) replacing the fixed side panels.
  const shapesPanel = (
    <>
        <div ref={stencilPanelRef} className="h-full w-full bg-white flex flex-col overflow-hidden">
          {/* Search */}
          <div className="p-2 flex-shrink-0 border-b border-border">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary" />
              <input
                value={stencilSearch}
                onChange={(e) => setStencilSearch(e.target.value)}
                placeholder={t('diag_search_shape')}
                className="w-full pl-7 pr-2 py-1 text-xs bg-surface-2 rounded border border-transparent
                           outline-none focus:border-primary focus:bg-white"
              />
            </div>
          </div>

          {stencilSearch ? (
            /* Flat search results */
            <div className="flex-1 overflow-y-auto p-2">
              <div className="grid grid-cols-5 gap-1">
                {searchResults.map((stencil) => stencilCell(stencil))}
              </div>
              {!searchResults.length && (
                <p className="text-[11px] text-text-tertiary text-center py-4">{t('diag_no_shape', { defaultValue: 'Aucune forme' })}</p>
              )}
            </div>
          ) : (
            /* Collapsible category sections */
            <div className="flex-1 overflow-y-auto">
              {categories.map((cat) => {
                const open = expandedCats.has(cat)
                return (
                  <div key={cat} className="border-b border-border">
                    <button
                      onClick={() => toggleCat(cat)}
                      className="w-full flex items-center gap-1.5 px-2 py-2 text-xs font-medium text-text-secondary hover:bg-surface-2 transition-colors"
                    >
                      <ChevronRight size={14} className={`transition-transform ${open ? 'rotate-90' : ''}`} />
                      {t('stencil_cat_' + cat, { defaultValue: CAT_LABELS[cat] ?? cat })}
                    </button>
                    {open && (
                      <div className="px-2 pb-2 grid grid-cols-5 gap-1">
                        {getStencilsByCategory(cat).map((stencil) => stencilCell(stencil))}
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )}

          {pendingStencil && (
            <div className="flex-shrink-0 px-2 py-1.5 border-t border-border bg-primary/5 text-xs text-primary text-center">
              {t('diag_click_to_place')}
            </div>
          )}
        </div>

        {/* Hover preview card (draw.io-style), to the right of the panel */}
        {hoverStencil && (() => {
          const pr = stencilPanelRef.current?.getBoundingClientRect()
          const left = (pr?.right ?? 220) + 8
          const top = Math.max(8, Math.min(hoverStencil.top, window.innerHeight - 230))
          return (
            <div className="fixed z-[1000] bg-white rounded-lg border border-border shadow-xl pointer-events-none" style={{ left, top, width: 220 }}>
              <div className="p-4 flex items-center justify-center">
                <StencilThumbnail stencil={hoverStencil.s} w={180} h={120} />
              </div>
              <div className="border-t border-border text-center text-sm text-text-secondary py-2">
                {t('stencil_' + hoverStencil.s.id, { defaultValue: hoverStencil.s.name })}
              </div>
            </div>
          )
        })()}
    </>
  )
  const layersPanel = (
      <div className="h-full w-full bg-white flex flex-col">
              <div className="flex items-center justify-between px-2 py-1.5 border-b border-border flex-shrink-0">
                <span className="text-xs font-medium text-text-secondary">{t('diag_layers', { defaultValue: 'Calques' })}</span>
                <button onClick={addLayer} title={t('diag_layer_add', { defaultValue: 'Nouveau calque' })} className="p-1 hover:bg-surface-2 rounded text-text-secondary"><Plus size={14} /></button>
              </div>
              <div className="overflow-y-auto">
                {layers.map((l, idx) => idx).reverse().map((idx) => {
                  const l = layers[idx]
                  const active = l.id === activeLayerId
                  return (
                    <div
                      key={l.id}
                      onClick={() => setActiveLayerId(l.id)}
                      className={`flex items-center gap-1 px-2 py-1.5 cursor-pointer border-l-2 ${active ? 'border-primary bg-primary/5' : 'border-transparent hover:bg-surface-1'}`}
                    >
                      <button onClick={(e) => { e.stopPropagation(); toggleLayerVisible(l.id) }} title={t('diag_layer_visible', { defaultValue: 'Visibilité' })} className="text-text-tertiary hover:text-text-primary flex-shrink-0">{l.visible ? <Eye size={13} /> : <EyeOff size={13} />}</button>
                      <button onClick={(e) => { e.stopPropagation(); toggleLayerLocked(l.id) }} title={t('diag_layer_lock', { defaultValue: 'Verrouiller' })} className="text-text-tertiary hover:text-text-primary flex-shrink-0">{l.locked ? <Lock size={13} /> : <LockOpen size={13} />}</button>
                      {renamingLayer === l.id ? (
                        <input
                          autoFocus
                          value={renameLayerVal}
                          onChange={(e) => setRenameLayerVal(e.target.value)}
                          onClick={(e) => e.stopPropagation()}
                          onBlur={() => { renameLayer(l.id, renameLayerVal.trim() || l.name); setRenamingLayer(null) }}
                          onKeyDown={(e) => { if (e.key === 'Enter') { renameLayer(l.id, renameLayerVal.trim() || l.name); setRenamingLayer(null) } if (e.key === 'Escape') setRenamingLayer(null) }}
                          className="flex-1 min-w-0 text-xs px-1 py-0.5 border border-primary rounded outline-none"
                        />
                      ) : (
                        <span onDoubleClick={(e) => { e.stopPropagation(); setRenamingLayer(l.id); setRenameLayerVal(l.name) }} className="flex-1 min-w-0 truncate text-xs text-text-primary">{l.name}</span>
                      )}
                      <button onClick={(e) => { e.stopPropagation(); moveLayer(l.id, 'up') }} disabled={idx === layers.length - 1} className="text-text-tertiary hover:text-text-primary disabled:opacity-30 flex-shrink-0"><ChevronUp size={13} /></button>
                      <button onClick={(e) => { e.stopPropagation(); moveLayer(l.id, 'down') }} disabled={idx === 0} className="text-text-tertiary hover:text-text-primary disabled:opacity-30 flex-shrink-0"><ChevronDown size={13} /></button>
                      <button onClick={(e) => { e.stopPropagation(); deleteLayer(l.id) }} disabled={layers.length <= 1} className="text-text-tertiary hover:text-danger disabled:opacity-30 flex-shrink-0"><Trash2 size={13} /></button>
                    </div>
                  )
                })}
              </div>
              {(selectedIds.size > 0 || selectedConnIds.size > 0) && (
                <button onClick={() => moveSelectionToLayer(activeLayerId)} className="text-xs text-primary hover:bg-primary/5 px-2 py-1.5 border-t border-border text-left flex-shrink-0">
                  {t('diag_move_to_active_layer', { defaultValue: 'Déplacer la sélection ici' })}
                </button>
              )}
            </div>
  )
  const formatPanel = (
      <div className="h-full w-full bg-white flex flex-col overflow-y-auto">
            <div className="px-3 py-2.5 border-b border-border">
              <p className="text-xs font-medium text-text-secondary uppercase tracking-wide">{t('diag_properties')}</p>
            </div>

            {selectedShape && (() => {
              const ss = mergeStyle(selectedShape.style)
              const ls = { ...DEFAULT_LABEL_STYLE, ...(selectedShape.labelStyle as LabelStyle) }
              const sid = selectedShape.id
              const tabBtn = (id: 'style' | 'text' | 'arrange', label: string) => (
                <button onClick={() => setFormatTab(id)} className={`flex-1 py-1.5 text-[11px] border-b-2 transition-colors ${formatTab === id ? 'border-primary text-primary font-medium' : 'border-transparent text-text-secondary hover:bg-surface-1'}`}>{label}</button>
              )
              const setLS = (patch: Partial<LabelStyle>) => updateShapeLabelStyle(sid, patch)
              return (
              <div>
                <div className="flex border-b border-border">
                  {tabBtn('style', t('diag_tab_style', { defaultValue: 'Style' }))}
                  {tabBtn('text', t('diag_text', { defaultValue: 'Texte' }))}
                  {tabBtn('arrange', t('diag_ctx_arrange', { defaultValue: 'Disposition' }))}
                </div>
                <div className="p-3 space-y-4">

                {formatTab === 'style' && (<>
                  <div>
                    <p className="text-xs text-text-tertiary mb-2">{t('diag_styles', { defaultValue: 'Styles' })}</p>
                    <div className="grid grid-cols-6 gap-1.5">
                      {STYLE_PRESETS.map(([fill, stroke], i) => (
                        <button key={i} title={t('diag_apply_style', { defaultValue: 'Appliquer ce style' })} onClick={() => applyStyleToSelection({ fillColor: fill, strokeColor: stroke })} className="h-6 rounded border" style={{ background: fill === 'none' ? '#fff' : fill, borderColor: stroke, backgroundImage: fill === 'none' ? 'linear-gradient(45deg,transparent 45%,#d93025 45%,#d93025 55%,transparent 55%)' : undefined }} />
                      ))}
                    </div>
                  </div>
                  <div>
                    <p className="text-xs text-text-tertiary mb-2">{t('diag_fill')}</p>
                    <div className="flex items-center gap-2">
                      <input type="color" value={ss.fillColor === 'none' ? '#ffffff' : ss.fillColor} onChange={(e) => updateShapeStyle(sid, { fillColor: e.target.value })} className="w-7 h-7 rounded cursor-pointer border border-border" />
                      <input type="text" value={ss.fillColor} onChange={(e) => updateShapeStyle(sid, { fillColor: e.target.value })} className="flex-1 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary font-mono" />
                      <button title={t('diag_no_fill', { defaultValue: 'Sans remplissage' })} onClick={() => updateShapeStyle(sid, { fillColor: 'none' })} className="px-2 py-1 text-[10px] border border-border rounded hover:bg-surface-2">∅</button>
                    </div>
                  </div>
                  <div>
                    <p className="text-xs text-text-tertiary mb-2">{t('diag_stroke')}</p>
                    <div className="flex items-center gap-2 mb-2">
                      <input type="color" value={ss.strokeColor === 'none' ? '#000000' : ss.strokeColor} onChange={(e) => updateShapeStyle(sid, { strokeColor: e.target.value })} className="w-7 h-7 rounded cursor-pointer border border-border" />
                      <input type="number" min={0.5} max={10} step={0.5} value={ss.strokeWidth} onChange={(e) => updateShapeStyle(sid, { strokeWidth: parseFloat(e.target.value) })} className="w-14 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary" />
                    </div>
                    <Dropdown className="w-full" height={24} fontSize={12} value={ss.strokeStyle} onChange={v => updateShapeStyle(sid, { strokeStyle: v as ShapeStyle['strokeStyle'] })} options={[{ value: 'solid', label: t('diag_line_solid') }, { value: 'dashed', label: t('diag_line_dashed') }, { value: 'dotted', label: t('diag_line_dotted') }]} />
                  </div>
                  <div>
                    <p className="text-xs text-text-tertiary mb-2">{t('diag_rounded', { defaultValue: 'Arrondi' })}: {ss.rounded || 0}</p>
                    <RangeSlider min={0} max={40} value={ss.rounded || 0} onChange={(v) => updateShapeStyle(sid, { rounded: v })} className="w-full" aria-label={t('diag_rounded', { defaultValue: 'Arrondi' })} />
                  </div>
                  <label className="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
                    <input type="checkbox" checked={ss.shadow} onChange={(e) => updateShapeStyle(sid, { shadow: e.target.checked })} /> {t('diag_shadow', { defaultValue: 'Ombre' })}
                  </label>
                  <div>
                    <p className="text-xs text-text-tertiary mb-2">{t('diag_opacity', { value: ss.opacity })}</p>
                    <RangeSlider min={0} max={100} value={ss.opacity} onChange={(v) => updateShapeStyle(sid, { opacity: v })} className="w-full" aria-label={t('diag_opacity', { value: ss.opacity })} />
                  </div>
                  <Button variant="secondary" size="sm" onClick={() => { setStyleText(JSON.stringify(selectedShape.style, null, 2)); setShowStyleEditor(true) }} className="w-full text-xs">{t('diag_edit_style', { defaultValue: 'Modifier le style…' })}</Button>
                </>)}

                {formatTab === 'text' && (<>
                  <textarea value={selectedShape.label} onChange={(e) => mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => s.id === sid ? { ...s, label: e.target.value } : s) }))} placeholder={t('diag_label_placeholder')} rows={2} className="w-full px-1.5 py-1 text-xs border border-border rounded outline-none focus:border-primary resize-none" />
                  <FontPicker className="w-full" height={24} fontSize={12} value={ls.fontFamily} onChange={v => setLS({ fontFamily: v })} fonts={FONT_FAMILIES} />
                  <div className="flex items-center gap-2">
                    <input type="color" value={ls.color} onChange={(e) => setLS({ color: e.target.value })} className="w-7 h-7 rounded cursor-pointer border border-border" />
                    <input type="number" min={8} max={72} value={ls.fontSize} onChange={(e) => setLS({ fontSize: parseInt(e.target.value) || 12 })} className="w-14 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary" />
                    <button onClick={() => setLS({ bold: !ls.bold })} className={`w-7 h-7 rounded border text-xs font-bold ${ls.bold ? 'bg-primary/10 border-primary text-primary' : 'border-border'}`}>B</button>
                    <button onClick={() => setLS({ italic: !ls.italic })} className={`w-7 h-7 rounded border text-xs italic ${ls.italic ? 'bg-primary/10 border-primary text-primary' : 'border-border'}`}>I</button>
                  </div>
                  <div>
                    <p className="text-[10px] text-text-tertiary uppercase mb-1">{t('diag_align_h', { defaultValue: 'Alignement horizontal' })}</p>
                    <div className="flex gap-1">
                      {(['left', 'center', 'right'] as const).map((a, i) => (
                        <button key={a} onClick={() => setLS({ align: a })} className={`flex-1 h-7 flex items-center justify-center rounded border ${ls.align === a ? 'bg-primary/10 border-primary text-primary' : 'border-border text-text-secondary'}`}>{[<AlignLeft size={14} />, <AlignCenter size={14} />, <AlignRight size={14} />][i]}</button>
                      ))}
                    </div>
                  </div>
                  <div>
                    <p className="text-[10px] text-text-tertiary uppercase mb-1">{t('diag_align_v', { defaultValue: 'Alignement vertical' })}</p>
                    <div className="flex gap-1">
                      {(['top', 'middle', 'bottom'] as const).map((a, i) => (
                        <button key={a} onClick={() => setLS({ verticalAlign: a })} className={`flex-1 h-7 flex items-center justify-center rounded border ${ls.verticalAlign === a ? 'bg-primary/10 border-primary text-primary' : 'border-border text-text-secondary'}`}>{[<AlignStartVertical size={14} />, <AlignCenterVertical size={14} />, <AlignEndVertical size={14} />][i]}</button>
                      ))}
                    </div>
                  </div>
                </>)}

                {formatTab === 'arrange' && (<>
                  <div>
                    <p className="text-xs text-text-tertiary mb-2">{t('diag_dimensions')}</p>
                    <div className="grid grid-cols-2 gap-2">
                      {(['x', 'y', 'w', 'h'] as const).map((k) => (
                        <div key={k}>
                          <label className="text-[10px] text-text-tertiary uppercase">{k === 'w' ? t('diag_width') : k === 'h' ? t('diag_height') : k.toUpperCase()}</label>
                          <input type="number" value={Math.round(selectedShape[k])} onChange={(e) => { const v = parseInt(e.target.value); if (!isNaN(v)) mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => s.id === sid ? { ...s, [k]: v } : s) })) }} className="w-full mt-0.5 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary" />
                        </div>
                      ))}
                    </div>
                  </div>
                  <div>
                    <label className="text-[10px] text-text-tertiary uppercase">{t('diag_rotation', { defaultValue: 'Rotation (°)' })}</label>
                    <input type="number" value={Math.round(selectedShape.rotation || 0)} onChange={(e) => { const v = parseInt(e.target.value) || 0; mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => s.id === sid ? { ...s, rotation: ((v % 360) + 360) % 360 } : s) })) }} className="w-full mt-0.5 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary" />
                  </div>
                  <div>
                    <p className="text-[10px] text-text-tertiary uppercase mb-1">{t('diag_grp_flip', { defaultValue: 'Miroir' })}</p>
                    <div className="flex gap-1">
                      <button onClick={() => flipSelection('h')} className="flex-1 h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1"><FlipHorizontal2 size={14} /></button>
                      <button onClick={() => flipSelection('v')} className="flex-1 h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1"><FlipVertical2 size={14} /></button>
                    </div>
                  </div>
                  <div>
                    <p className="text-[10px] text-text-tertiary uppercase mb-1">{t('diag_grp_order', { defaultValue: 'Ordre' })}</p>
                    <div className="flex gap-1">
                      <button onClick={() => reorderSelection('front')} title={t('diag_ctx_to_front', { defaultValue: 'Premier plan' })} className="flex-1 h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1"><ArrowUpToLine size={14} /></button>
                      <button onClick={() => reorderSelection('forward')} className="flex-1 h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1"><ChevronUp size={14} /></button>
                      <button onClick={() => reorderSelection('backward')} className="flex-1 h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1"><ChevronDown size={14} /></button>
                      <button onClick={() => reorderSelection('back')} title={t('diag_ctx_to_back', { defaultValue: 'Arrière-plan' })} className="flex-1 h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1"><ArrowDownToLine size={14} /></button>
                    </div>
                  </div>
                  {selectedIds.size >= 2 && (
                    <div>
                      <p className="text-[10px] text-text-tertiary uppercase mb-1">{t('diag_ctx_align', { defaultValue: 'Aligner' })}</p>
                      <div className="grid grid-cols-3 gap-1">
                        {([['left', <AlignLeft size={14} />], ['center', <AlignCenter size={14} />], ['right', <AlignRight size={14} />], ['top', <AlignStartVertical size={14} />], ['middle', <AlignCenterVertical size={14} />], ['bottom', <AlignEndVertical size={14} />]] as Array<[string, React.ReactNode]>).map(([a, ic]) => (
                          <button key={a} onClick={() => align(a)} className="h-7 flex items-center justify-center rounded border border-border text-text-secondary hover:bg-surface-1">{ic}</button>
                        ))}
                      </div>
                    </div>
                  )}
                  <div className="flex gap-1">
                    <button onClick={groupSelection} disabled={selectedIds.size < 2} className="flex-1 h-7 flex items-center justify-center gap-1 rounded border border-border text-text-secondary hover:bg-surface-1 disabled:opacity-40 text-[11px]"><Group size={13} /> {t('diag_group', { defaultValue: 'Grouper' })}</button>
                    <button onClick={ungroupSelection} disabled={!selectionHasGroup} className="flex-1 h-7 flex items-center justify-center gap-1 rounded border border-border text-text-secondary hover:bg-surface-1 disabled:opacity-40 text-[11px]"><Ungroup size={13} /> {t('diag_ungroup', { defaultValue: 'Dégrouper' })}</button>
                  </div>
                </>)}

                <Button variant="danger" size="sm" icon={<Trash2 size={13} />} onClick={deleteSelected} className="w-full text-xs">{t('common_delete')}</Button>
                </div>
              </div>
              ) })()}

            {selectedConn && (
              <div className="p-3 space-y-4">
                <div>
                  <p className="text-xs text-text-tertiary mb-2">{t('diag_connector')}</p>
                  <div className="flex items-center gap-2 mb-2">
                    <input
                      type="color"
                      value={selectedConn.style.strokeColor}
                      onChange={(e) => updateConnStyle(selectedConn.id, { strokeColor: e.target.value })}
                      className="w-7 h-7 rounded cursor-pointer border border-border"
                    />
                    <input
                      type="number" min={0.5} max={10} step={0.5}
                      value={selectedConn.style.strokeWidth}
                      onChange={(e) => updateConnStyle(selectedConn.id, { strokeWidth: parseFloat(e.target.value) })}
                      className="w-14 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary"
                    />
                  </div>
                  <Dropdown
                    className="w-full mb-2"
                    height={24}
                    fontSize={12}
                    value={selectedConn.style.strokeStyle}
                    onChange={v => updateConnStyle(selectedConn.id, { strokeStyle: v as ConnectorStyle['strokeStyle'] })}
                    options={[
                      { value: 'solid',  label: t('diag_line_solid') },
                      { value: 'dashed', label: t('diag_line_dashed') },
                      { value: 'dotted', label: t('diag_line_dotted') },
                    ]}
                  />
                  <Dropdown
                    className="w-full mb-2"
                    height={24}
                    fontSize={12}
                    value={connRouting(selectedConn)}
                    onChange={v => updateConnStyle(selectedConn.id, { routing: v as NonNullable<ConnectorStyle['routing']> })}
                    options={[
                      { value: 'straight',   label: t('diag_route_straight', { defaultValue: 'Direct' }) },
                      { value: 'orthogonal', label: t('diag_route_orthogonal', { defaultValue: 'Orthogonal' }) },
                      { value: 'curved',     label: t('diag_route_curved', { defaultValue: 'Courbe' }) },
                    ]}
                  />
                  <Dropdown
                    className="w-full"
                    height={24}
                    fontSize={12}
                    value={selectedConn.style.arrowEnd}
                    onChange={v => updateConnStyle(selectedConn.id, { arrowEnd: v })}
                    options={[
                      { value: 'none',    label: t('diag_arrow_none') },
                      { value: 'block',   label: t('diag_arrow_block') },
                      { value: 'classic', label: t('diag_arrow_classic') },
                      { value: 'open',    label: t('diag_arrow_open') },
                    ]}
                  />
                </div>
                <div>
                  <input
                    type="text"
                    value={selectedConn.label}
                    onChange={(e) => mutateData((d) => ({ ...d, connectors: d.connectors.map((c) => c.id === selectedConn.id ? { ...c, label: e.target.value } : c) }))}
                    placeholder={t('diag_connector_label_placeholder')}
                    className="w-full px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary"
                  />
                </div>
                <Button
                  variant="danger"
                  size="sm"
                  icon={<Trash2 size={13} />}
                  onClick={deleteSelected}
                  className="w-full text-xs"
                >
                  {t('common_delete')}
                </Button>
              </div>
            )}

            {!selectedShape && !selectedConn && (
              <div className="p-4 text-xs text-text-tertiary text-center">
                {t('diag_select_element')}
              </div>
            )}
          </div>
  )
  const diagPanels: Record<string, DockPanel> = {
    shapes: { label: t('diag_panel_shapes', { defaultValue: 'Formes' }), render: () => shapesPanel },
    format: { label: t('diag_properties', { defaultValue: 'Propriétés' }), render: () => formatPanel },
    layers: { label: t('diag_layers', { defaultValue: 'Calques' }), render: () => layersPanel },
  }

  return (
    <OfficeShell
      ribbon={[fileTab, ...diagRibbon]}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_DIAGRAMS}
      chromeless
      topbarHeight={64}
      onBack={() => navigate('/office/diagrams')}
      titleIcon={<Network size={16} className="text-white/90 flex-shrink-0" />}
      title={title}
      onTitleChange={handleTitleChange}
      titlePlaceholder={t('common_untitled')}
      saveStatus={saveStatus === 'saving' ? t('diag_saving') : saveStatus === 'unsaved' ? t('diag_unsaved') : t('doc_saved')}
      titleActions={<SaveButton onSave={flushSave} saving={saveMut.isPending} label={t('doc_save', { defaultValue: 'Enregistrer' })} />}
      onDelete={() => trashDiagMut.mutate()}
      deleteTitle={t('diag_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('diag_delete_confirm_title', { defaultValue: 'Supprimer ce diagramme ?' }),
        message: t('diag_delete_confirm_msg', { defaultValue: 'Le diagramme sera déplacé dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
    >
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">

      {/* ── Body (docking) ── */}
      <DockArea
        panels={diagPanels}
        storageKey="kubuno:office:diagramDock"
        defaultArrangement={{ left: [['shapes']], right: [['format'], ['layers']] }}
        controllerRef={dockRef}
        viewportBg="#ffffff"
        className="flex flex-1 min-w-0 overflow-hidden"
      >
        {/* ── Canvas area ── */}
        <div
          ref={containerRef}
          className="relative flex-1 overflow-hidden"
          style={{ cursor: pendingStencil ? 'crosshair' : 'default' }}
        >
          {loadingPage && (
            <div className="absolute inset-0 flex items-center justify-center bg-white/70 z-10">
              <Spinner size="md" />
            </div>
          )}

          <canvas
            ref={canvasRef}
            className="absolute inset-0"
            onMouseDown={handleMouseDown}
            onMouseMove={handleMouseMove}
            onMouseUp={handleMouseUp}
            onMouseLeave={handleMouseUp}
            onDoubleClick={handleDblClick}
            onContextMenu={handleContextMenu}
            onWheel={handleWheel}
            onDragOver={handleCanvasDragOver}
            onDrop={handleCanvasDrop}
            style={{ touchAction: 'none' }}
          />

          {/* Inline label editor */}
          {editingLabel && (() => {
            const isConn = editingLabel.isConnector
            const shape = isConn ? null : data.shapes.find((s) => s.id === editingLabel.id)
            const conn  = isConn ? data.connectors.find((c) => c.id === editingLabel.id) : null
            if (!shape && !conn) return null
            let pos: { x: number; y: number }
            if (shape) {
              pos = worldToCanvas(shape.x + shape.w / 2, shape.y + shape.h / 2, panX, panY, zoom)
            } else {
              const mid = connectorLabelCenter(conn!, data.shapes)
              pos = worldToCanvas(mid.x, mid.y, panX, panY, zoom)
            }
            return (
              <input
                ref={labelInputRef}
                value={labelText}
                onChange={(e) => setLabelText(e.target.value)}
                onBlur={commitLabel}
                onKeyDown={(e) => { if (e.key === 'Enter') commitLabel(); if (e.key === 'Escape') setEditingLabel(null) }}
                className="absolute z-20 bg-white border border-primary rounded px-2 py-0.5 text-sm outline-none shadow-md"
                style={{
                  left:      pos.x - 60,
                  top:       pos.y - 14,
                  width:     120,
                  transform: 'none',
                }}
              />
            )
          })()}

          {/* Menu contextuel (connecteur ou objet) — composant MenuDropdown de @ui */}
          {ctxMenu && (() => {
            const items: MenuItem[] = []
            if (ctxMenu.kind === 'connector') {
              const conn = data.connectors.find((c) => c.id === ctxMenu.connId)
              if (!conn) return null
              items.push(
                { type: 'action', label: t('diag_ctx_edit_label', { defaultValue: 'Modifier le label' }), icon: <Pencil size={14} />, onClick: () => startLabelEdit(conn.id, true, conn.label) },
                { type: 'action', label: t('diag_ctx_add_node', { defaultValue: 'Ajouter un nœud ici' }), icon: <Plus size={14} />, onClick: () => connAddNode(conn.id, ctxMenu.seg, ctxMenu.worldX, ctxMenu.worldY) },
                { type: 'action', label: t('diag_ctx_clear_nodes', { defaultValue: 'Effacer les nœuds' }), icon: <Minus size={14} />, disabled: !conn.waypoints?.length, onClick: () => connClearNodes(conn.id) },
                { type: 'action', label: t('diag_ctx_reverse', { defaultValue: 'Inverser le sens' }), icon: <ArrowLeftRight size={14} />, onClick: () => connReverse(conn.id) },
                { type: 'separator' },
                { type: 'submenu', label: t('diag_grp_routing', { defaultValue: 'Routage' }), items: [
                  { type: 'action', label: t('diag_route_straight', { defaultValue: 'Direct' }),     checked: connRouting(conn) === 'straight',   onClick: () => updateConnStyle(conn.id, { routing: 'straight' }) },
                  { type: 'action', label: t('diag_route_orthogonal', { defaultValue: 'Orthogonal' }), checked: connRouting(conn) === 'orthogonal', onClick: () => updateConnStyle(conn.id, { routing: 'orthogonal' }) },
                  { type: 'action', label: t('diag_route_curved', { defaultValue: 'Courbe' }),       checked: connRouting(conn) === 'curved',     onClick: () => updateConnStyle(conn.id, { routing: 'curved' }) },
                ] },
                { type: 'separator' },
                { type: 'label', text: t('diag_ctx_line', { defaultValue: 'Trait' }) },
                { type: 'action', label: t('diag_line_solid'),  checked: conn.style.strokeStyle === 'solid',  onClick: () => updateConnStyle(conn.id, { strokeStyle: 'solid' }) },
                { type: 'action', label: t('diag_line_dashed'), checked: conn.style.strokeStyle === 'dashed', onClick: () => updateConnStyle(conn.id, { strokeStyle: 'dashed' }) },
                { type: 'action', label: t('diag_line_dotted'), checked: conn.style.strokeStyle === 'dotted', onClick: () => updateConnStyle(conn.id, { strokeStyle: 'dotted' }) },
                { type: 'action', label: t('diag_ctx_arrow_end', { defaultValue: 'Flèche de fin' }), checked: conn.style.arrowEnd !== 'none', onClick: () => updateConnStyle(conn.id, { arrowEnd: conn.style.arrowEnd === 'none' ? 'block' : 'none' }) },
                { type: 'action', label: t('diag_ctx_arrow_start', { defaultValue: 'Flèche de début' }), checked: conn.style.arrowStart !== 'none', onClick: () => updateConnStyle(conn.id, { arrowStart: conn.style.arrowStart === 'none' ? 'block' : 'none' }) },
                { type: 'submenu', label: t('diag_stroke_width', { defaultValue: 'Épaisseur' }), items: [1, 2, 3, 4].map((wd) => ({ type: 'action' as const, label: `${wd} px`, checked: Math.round(conn.style.strokeWidth) === wd, onClick: () => updateConnStyle(conn.id, { strokeWidth: wd }) })) },
                { type: 'separator' },
                { type: 'action', label: t('diag_ctx_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => connDuplicate(conn.id) },
                { type: 'action', label: t('diag_ctx_to_front', { defaultValue: 'Mettre au premier plan' }), icon: <ArrowUp size={14} />, onClick: () => connReorder(conn.id, true) },
                { type: 'action', label: t('diag_ctx_to_back', { defaultValue: 'Mettre à l’arrière-plan' }), icon: <ArrowDown size={14} />, onClick: () => connReorder(conn.id, false) },
                { type: 'separator' },
                { type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={14} />, onClick: () => deleteConnector(conn.id) },
              )
            } else if (ctxMenu.kind === 'shape') {
              const shape = data.shapes.find((s) => s.id === ctxMenu.shapeId)
              if (!shape) return null
              const multi = selectedIds.size >= 2
              const sStyle = mergeStyle(shape.style)
              items.push(
                { type: 'action', label: t('diag_ctx_edit_text', { defaultValue: 'Modifier le texte' }), icon: <Pencil size={14} />, onClick: () => startLabelEdit(shape.id, false, shape.label) },
                { type: 'separator' },
                { type: 'action', label: t('diag_copy', { defaultValue: 'Copier' }), icon: <Copy size={14} />, onClick: copySelection },
                { type: 'action', label: t('diag_cut', { defaultValue: 'Couper' }), icon: <Scissors size={14} />, onClick: cutSelection },
                { type: 'action', label: t('diag_ctx_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: duplicateSelection },
                { type: 'separator' },
                { type: 'submenu', label: t('diag_ctx_arrange', { defaultValue: 'Disposition' }), items: [
                  { type: 'action', label: t('diag_ctx_to_front', { defaultValue: 'Mettre au premier plan' }), icon: <ArrowUpToLine size={14} />, onClick: () => reorderSelection('front') },
                  { type: 'action', label: t('diag_ctx_forward', { defaultValue: 'Avancer' }), icon: <ChevronUp size={14} />, onClick: () => reorderSelection('forward') },
                  { type: 'action', label: t('diag_ctx_backward', { defaultValue: 'Reculer' }), icon: <ChevronDown size={14} />, onClick: () => reorderSelection('backward') },
                  { type: 'action', label: t('diag_ctx_to_back', { defaultValue: 'Mettre à l’arrière-plan' }), icon: <ArrowDownToLine size={14} />, onClick: () => reorderSelection('back') },
                ] },
                { type: 'submenu', label: t('diag_grp_flip', { defaultValue: 'Miroir' }), items: [
                  { type: 'action', label: t('diag_flip_h', { defaultValue: 'Horizontal' }), icon: <FlipHorizontal2 size={14} />, checked: !!shape.flipH, onClick: () => flipSelection('h') },
                  { type: 'action', label: t('diag_flip_v', { defaultValue: 'Vertical' }), icon: <FlipVertical2 size={14} />, checked: !!shape.flipV, onClick: () => flipSelection('v') },
                ] },
                { type: 'submenu', label: t('diag_ctx_line', { defaultValue: 'Trait' }), items: [
                  { type: 'action', label: t('diag_line_solid'),  checked: sStyle.strokeStyle === 'solid',  onClick: () => updateShapeStyle(shape.id, { strokeStyle: 'solid' }) },
                  { type: 'action', label: t('diag_line_dashed'), checked: sStyle.strokeStyle === 'dashed', onClick: () => updateShapeStyle(shape.id, { strokeStyle: 'dashed' }) },
                  { type: 'action', label: t('diag_line_dotted'), checked: sStyle.strokeStyle === 'dotted', onClick: () => updateShapeStyle(shape.id, { strokeStyle: 'dotted' }) },
                  { type: 'separator' },
                  { type: 'action', label: t('diag_shadow', { defaultValue: 'Ombre' }), checked: sStyle.shadow, onClick: () => updateShapeStyle(shape.id, { shadow: !sStyle.shadow }) },
                ] },
              )
              if (multi) {
                items.push({ type: 'submenu', label: t('diag_ctx_align', { defaultValue: 'Aligner' }), items: [
                  { type: 'action', label: t('diag_align_left'),     icon: <AlignLeft size={14} />,           onClick: () => align('left') },
                  { type: 'action', label: t('diag_align_center_h'), icon: <AlignCenter size={14} />,         onClick: () => align('center') },
                  { type: 'action', label: t('diag_align_right'),    icon: <AlignRight size={14} />,          onClick: () => align('right') },
                  { type: 'action', label: t('diag_align_top'),      icon: <AlignStartVertical size={14} />,  onClick: () => align('top') },
                  { type: 'action', label: t('diag_align_center_v'), icon: <AlignCenterVertical size={14} />, onClick: () => align('middle') },
                  { type: 'action', label: t('diag_align_bottom'),   icon: <AlignEndVertical size={14} />,    onClick: () => align('bottom') },
                  { type: 'separator' },
                  { type: 'action', label: t('diag_distribute_h', { defaultValue: 'Distribuer horizontalement' }), disabled: selectedIds.size < 3, onClick: () => distribute('h') },
                  { type: 'action', label: t('diag_distribute_v', { defaultValue: 'Distribuer verticalement' }), disabled: selectedIds.size < 3, onClick: () => distribute('v') },
                ] })
              }
              items.push(
                { type: 'action', label: t('diag_group', { defaultValue: 'Grouper' }), icon: <Group size={14} />, disabled: !multi, onClick: groupSelection },
                { type: 'action', label: t('diag_ungroup', { defaultValue: 'Dégrouper' }), icon: <Ungroup size={14} />, disabled: !selectionHasGroup, onClick: ungroupSelection },
              )
              if (layers.length > 1) {
                items.push({ type: 'submenu', label: t('diag_move_to_layer', { defaultValue: 'Déplacer vers le calque' }), items:
                  layers.map((l) => ({ type: 'action' as const, label: l.name, checked: layerOf(shape) === l.id, onClick: () => moveSelectionToLayer(l.id) })) })
              }
              items.push(
                { type: 'separator' },
                { type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={14} />, onClick: deleteSelected },
              )
            } else {
              // Canvas (empty) context menu
              const cm = ctxMenu
              items.push(
                { type: 'action', label: t('diag_paste', { defaultValue: 'Coller' }), icon: <ClipboardPaste size={14} />, disabled: !clipboardRef.current?.shapes.length, onClick: () => pasteClipboard(20, 20, { x: cm.worldX, y: cm.worldY }) },
                { type: 'action', label: t('diag_select_all', { defaultValue: 'Tout sélectionner' }), disabled: !data.shapes.length, onClick: () => { const all = new Set(data.shapes.filter(isPickable).map((s) => s.id)); setSelectedIds(all); selRef.current = all } },
                { type: 'separator' },
                { type: 'submenu', label: t('diag_grp_autolayout', { defaultValue: 'Disposition auto' }), items: [
                  { type: 'action', label: t('diag_layout_tb', { defaultValue: 'Hiérarchique ↓' }), icon: <Workflow size={14} />, disabled: data.shapes.length < 2, onClick: () => applyLayout('hier_tb') },
                  { type: 'action', label: t('diag_layout_lr', { defaultValue: 'Hiérarchique →' }), icon: <GitBranch size={14} />, disabled: data.shapes.length < 2, onClick: () => applyLayout('hier_lr') },
                  { type: 'action', label: t('diag_layout_circle', { defaultValue: 'Circulaire' }), icon: <CircleDot size={14} />, disabled: data.shapes.length < 2, onClick: () => applyLayout('circle') },
                  { type: 'action', label: t('diag_layout_grid', { defaultValue: 'Grille' }), icon: <LayoutGrid size={14} />, disabled: data.shapes.length < 2, onClick: () => applyLayout('grid') },
                ] },
                { type: 'separator' },
                { type: 'action', label: t('diag_zoom_fit', { defaultValue: 'Ajuster' }), icon: <Maximize2 size={14} />, onClick: zoomToFit },
                { type: 'action', label: t('diag_reset_view', { defaultValue: 'Réinitialiser la vue' }), icon: <RotateCcw size={14} />, onClick: () => { setZoom(1); setPanX(60); setPanY(60) } },
                { type: 'separator' },
                { type: 'action', label: t('diag_grid', { defaultValue: 'Grille' }), checked: showGrid, onClick: () => setShowGrid((v) => !v) },
                { type: 'action', label: t('diag_minimap', { defaultValue: 'Minimap' }), checked: showMinimap, onClick: () => setShowMinimap((v) => !v) },
                { type: 'action', label: t('diag_layers', { defaultValue: 'Calques' }), onClick: () => dockRef.current?.open('layers') },
              )
            }
            return (
              <MenuDropdown
                items={items}
                pos={{ top: ctxMenu.y, left: ctxMenu.x }}
                onClose={() => setCtxMenu(null)}
              />
            )
          })()}

          {/* ── Minimap navigator (bottom-right) ── */}
          {showMinimap && (
            <div className="absolute bottom-3 right-3 z-10 rounded-md border border-border bg-white/95 shadow-md overflow-hidden no-print">
              <Minimap data={data} zoom={zoom} panX={panX} panY={panY} canvasRef={canvasRef} onJump={minimapJump} />
            </div>
          )}

          {/* ── Rulers (top / left) ── */}
          {showRulers && (
            <>
              <div className="absolute top-0 z-20 no-print" style={{ left: RULER_THICK }}>
                <Ruler orientation="h" pan={panX - RULER_THICK} zoom={zoom} length={Math.max(0, containerSize.w - RULER_THICK)} />
              </div>
              <div className="absolute left-0 z-20 no-print" style={{ top: RULER_THICK }}>
                <Ruler orientation="v" pan={panY - RULER_THICK} zoom={zoom} length={Math.max(0, containerSize.h - RULER_THICK)} />
              </div>
              <div className="absolute top-0 left-0 z-20 bg-surface-1 border-r border-b border-border no-print" style={{ width: RULER_THICK, height: RULER_THICK }} />
            </>
          )}
        </div>

      </DockArea>

      {/* ── Page tabs ── */}
      <div className="flex-shrink-0 h-9 bg-white border-t border-border flex items-center overflow-x-auto">
        <div className="flex items-center h-full">
          {pageList.map((p) => (
            <div key={p.id} className="flex items-center h-full group">
              {renamingPage === p.id ? (
                <input
                  autoFocus
                  value={renameVal}
                  onChange={(e) => setRenameVal(e.target.value)}
                  onBlur={() => commitPageRename(p.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') commitPageRename(p.id)
                    if (e.key === 'Escape') setRenamingPage(null)
                  }}
                  className="h-full px-3 text-xs outline-none border-b-2 border-primary bg-primary/5 min-w-16"
                />
              ) : (
                <button
                  onClick={() => setCurrentPageId(p.id)}
                  onDoubleClick={() => { setRenamingPage(p.id); setRenameVal(p.name) }}
                  className={`h-full px-3 text-xs border-r border-border transition-colors flex items-center gap-2 ${
                    p.id === currentPageId
                      ? 'text-primary font-medium border-b-2 border-primary bg-primary/5'
                      : 'text-text-secondary hover:bg-surface-2'
                  }`}
                >
                  {p.name}
                  {pageList.length > 1 && (
                    <span
                      onClick={async (e) => {
                        e.stopPropagation()
                        const ok = await confirm({
                          title:        t('diag_delete_page_title', { name: p.name }),
                          message:      t('diag_delete_page_message'),
                          confirmLabel: t('common_delete'),
                          variant:      'danger',
                        })
                        if (ok) {
                          deletePageMut.mutate(p.id)
                          if (p.id === currentPageId) setCurrentPageId(pageList.find((pg) => pg.id !== p.id)?.id ?? null)
                        }
                      }}
                      className="opacity-0 group-hover:opacity-100 text-text-tertiary hover:text-danger leading-none ml-1 text-base"
                    >
                      ×
                    </span>
                  )}
                </button>
              )}
            </div>
          ))}
          <button
            onClick={() => createPageMut.mutate(undefined)}
            className="h-full px-2 text-text-tertiary hover:text-primary hover:bg-surface-2 transition-colors"
            title={t('diag_add_page')}
          >
            <Plus size={14} />
          </button>
        </div>

        {/* Macros (sous-module Script) */}
        <div className="ml-auto flex items-center pr-2">
          {id && (
            <MacrosMenu docType="diagram" docId={id} buildApi={makeApi} defaultLabel={title} />
          )}
        </div>
      </div>

      {/* ── Barre de statut (nombre de formes/connecteurs, sélection, zoom) ── */}
      {(() => {
        const shapeCount = data.shapes.length
        const connCount  = data.connectors.length
        const selShapes  = selectedIds.size
        const selConns   = selectedConnIds.size
        const selTotal   = selShapes + selConns
        // Libellé de sélection : type/nom d'une seule forme, sinon le nombre d'éléments.
        let selLabel: string | null = null
        if (selectedShape) {
          const typeName = t('stencil_' + selectedShape.type, { defaultValue: selectedShape.type })
          selLabel = selectedShape.label
            ? `${selectedShape.label} (${typeName})`
            : typeName
        } else if (selTotal > 1) {
          selLabel = t('diag_status_selected_n', { count: selTotal, defaultValue: `${selTotal} élément(s) sélectionné(s)` })
        }
        return (
          <StatusBar>
            <StatusButton title={t('diag_status_shapes', { defaultValue: 'Formes' })}>
              {t('diag_status_shapes_n', { count: shapeCount, defaultValue: `${shapeCount} forme(s)` })}
            </StatusButton>
            <StatusSep />
            <StatusButton title={t('diag_status_connectors', { defaultValue: 'Connecteurs' })}>
              {t('diag_status_connectors_n', { count: connCount, defaultValue: `${connCount} connecteur(s)` })}
            </StatusButton>
            {selLabel && (
              <>
                <StatusSep />
                <StatusButton title={t('diag_status_selection', { defaultValue: 'Sélection' })}>{selLabel}</StatusButton>
              </>
            )}
            <StatusSpacer />
            <StatusZoom zoom={zoom} onZoom={(z) => setZoom(Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z)))} />
          </StatusBar>
        )
      })()}

      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}

      {/* Hidden import file picker (.drawio / .xml / .csv) */}
      <input
        ref={importFileRef}
        type="file"
        accept=".drawio,.xml,.csv"
        className="hidden"
        onChange={onImportFile}
      />

      {/* Edit Style (raw style JSON) */}
      {showStyleEditor && selectedShape && (
        <div className="fixed inset-0 z-[1000] bg-black/30 flex items-center justify-center p-4" onClick={() => setShowStyleEditor(false)}>
          <div className="bg-white rounded-xl shadow-xl w-[420px] max-w-[94vw] p-4" onClick={(e) => e.stopPropagation()}>
            <h2 className="text-sm font-semibold text-text-primary mb-2">{t('diag_edit_style', { defaultValue: 'Modifier le style' })}</h2>
            <textarea value={styleText} onChange={(e) => setStyleText(e.target.value)} rows={10} spellCheck={false} className="w-full font-mono text-xs border border-border rounded p-2 outline-none focus:border-primary resize-none" />
            <div className="flex justify-end gap-2 mt-3">
              <Button variant="secondary" size="sm" onClick={() => setShowStyleEditor(false)}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
              <Button size="sm" onClick={() => {
                try {
                  const parsed = JSON.parse(styleText)
                  const sid = selectedShape.id
                  mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => s.id === sid ? { ...s, style: parsed } : s) }))
                  setShowStyleEditor(false)
                } catch { /* invalid JSON — keep dialog open */ }
              }}>{t('common_apply', { defaultValue: 'Appliquer' })}</Button>
            </div>
          </div>
        </div>
      )}

      {/* Templates gallery */}
      {showTemplates && (
        <div className="fixed inset-0 z-[1000] bg-black/30 flex items-center justify-center p-4" onClick={() => setShowTemplates(false)}>
          <div className="bg-white rounded-xl shadow-xl w-[680px] max-w-[94vw] max-h-[82vh] overflow-y-auto p-4" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between mb-3">
              <h2 className="text-sm font-semibold text-text-primary">{t('diag_templates_title', { defaultValue: 'Choisir un modèle' })}</h2>
              <button onClick={() => setShowTemplates(false)} className="w-7 h-7 flex items-center justify-center rounded hover:bg-surface-2 text-text-tertiary"><Minus size={16} className="rotate-45" /></button>
            </div>
            <div className="grid grid-cols-3 gap-3">
              {TEMPLATES.map((tpl) => {
                const io = tpl.build()
                const prev = renderIoPreview(io)
                return (
                  <button
                    key={tpl.id}
                    onClick={() => { if (io.shapes.length) importIoData(io); setShowTemplates(false) }}
                    className="border border-border rounded-lg p-2 hover:border-primary hover:bg-primary/5 transition-colors flex flex-col items-center gap-2"
                  >
                    <div className="w-full h-[112px] bg-surface-1 rounded flex items-center justify-center overflow-hidden">
                      {prev ? <img src={prev} alt={tpl.name} className="max-w-full max-h-full" /> : <span className="text-xs text-text-tertiary">{t('diag_blank', { defaultValue: 'Vierge' })}</span>}
                    </div>
                    <span className="text-xs text-text-secondary">{tpl.name}</span>
                  </button>
                )
              })}
            </div>
          </div>
        </div>
      )}
      </div>
    </OfficeShell>
  )
}
