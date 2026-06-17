import { useCallback, useEffect, useRef, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useConfirm } from '@kubuno/sdk'
import { ConfirmDialog } from '@ui'
import {
  ZoomIn, ZoomOut, RotateCcw, Plus, Trash2, Network,
  AlignLeft, AlignCenter, AlignRight, AlignStartVertical,
  AlignCenterVertical, AlignEndVertical,
  Search, Download,
  Pencil, ArrowLeftRight, Copy, ArrowUp, ArrowDown, Minus,
} from 'lucide-react'
import { Dropdown, Button, Spinner, MenuDropdown, type MenuItem } from '@ui'
import { diagramsApi } from './api'
import { OfficeShell } from './shell/OfficeShell'
import { StatusBar, StatusButton, StatusSep, StatusSpacer, StatusZoom } from './shell/StatusBar'
import { THEME_DIAGRAMS, OFFICE_TONE } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'
import type { RibbonTab } from './ribbon/types'
import {
  renderShape, drawArrow, drawLabel, getCategories, getStencilsByCategory,
  searchStencils, mergeStyle, type ShapeStyle, type StencilDef,
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
}

interface ConnectorStyle {
  strokeColor: string
  strokeWidth: number
  strokeStyle: 'solid' | 'dashed' | 'dotted'
  arrowStart:  string
  arrowEnd:    string
  orthogonal:  boolean
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
}

interface DiagramData {
  shapes:     DiagramShape[]
  connectors: DiagramConnector[]
}

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_LABEL_STYLE: LabelStyle = {
  fontFamily: 'Inter', fontSize: 12, bold: false, italic: false,
  color: '#000000', align: 'center', verticalAlign: 'middle',
}

const DEFAULT_CONN_STYLE: ConnectorStyle = {
  strokeColor: '#6c8ebf', strokeWidth: 1.5, strokeStyle: 'solid',
  arrowStart: 'none', arrowEnd: 'block', orthogonal: false,
}

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
    if (wx >= s.x && wx <= s.x + s.w && wy >= s.y && wy <= s.y + s.h) return s
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

// Plus de magnétisme sur une grille : positionnement libre (arrondi au pixel).
function snapToGrid(v: number) {
  return Math.round(v)
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

// Centre (monde) du label d'un connecteur : milieu du segment central + décalage.
function connectorLabelCenter(conn: DiagramConnector, shapes: DiagramShape[]) {
  const pts = connectorPoints(conn, shapes)
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
    const pts = connectorPoints(conns[i], shapes)
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
  const pts = conn.waypoints?.length > 0 ? [from, ...conn.waypoints, to] : [from, to]

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
) {
  const dpr = window.devicePixelRatio || 1
  const ctx = canvas.getContext('2d')!
  const W = canvas.width / dpr
  const H = canvas.height / dpr

  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.save()
  ctx.scale(dpr, dpr)

  // Background (pas de grille)
  ctx.fillStyle = bgColor || '#ffffff'
  ctx.fillRect(0, 0, W, H)

  ctx.save()
  ctx.translate(panX, panY)
  ctx.scale(zoom, zoom)

  // Connectors — précalcul des « sauts de ligne » : pour chaque connecteur, les points
  // où il croise un connecteur PLUS ANCIEN (indice inférieur). Le plus récent enjambe.
  const polys = data.connectors.map((c) => connectorPoints(c, data.shapes))
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
    renderShape(
      ctx, shape.type, shape.x, shape.y, shape.w, shape.h,
      drawStyle, shape.label, { ...DEFAULT_LABEL_STYLE, ...shape.labelStyle },
    )

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

function StencilThumbnail({ stencil }: { stencil: StencilDef }) {
  const ref = useRef<HTMLCanvasElement>(null)

  const draw = useCallback(() => {
    const c = ref.current
    if (!c) return
    const dpr = window.devicePixelRatio || 1
    const W = 60, H = 40
    c.width = W * dpr; c.height = H * dpr
    const ctx = c.getContext('2d')!
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.scale(dpr, dpr)
    ctx.clearRect(0, 0, W, H)
    const margin = 6
    renderShape(
      ctx, stencil.id,
      margin, margin, W - margin * 2, H - margin * 2,
      mergeStyle(stencil.style),
      '', DEFAULT_LABEL_STYLE,
    )
  }, [stencil.id])

  useEffect(() => { draw() }, [draw])
  // Redessine quand une icône SVG (matériel) finit de charger.
  useEffect(() => onHwIconLoaded(draw), [draw])

  return <canvas ref={ref} style={{ width: 60, height: 40 }} className="block" />
}

// ── Main component ────────────────────────────────────────────────────────────

export default function DiagramEditorPage() {
  const { t } = useTranslation('office')
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

  const mutateData = useCallback((updater: (prev: DiagramData) => DiagramData) => {
    setData((prev) => {
      const next = updater(prev)
      saveData(next)
      return next
    })
  }, [saveData])

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

  const dataRef    = useRef(data)
  const zoomRef    = useRef(zoom)
  const panXRef    = useRef(panX)
  const panYRef    = useRef(panY)
  const selRef     = useRef(selectedIds)
  const selConnRef = useRef(selectedConnIds)
  const hovRef     = useRef(hoveredShapeId)
  const lassoRefV  = useRef(lasso)
  const dcRef      = useRef(drawingConn)

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
        c, dataRef.current,
        zoomRef.current, panXRef.current, panYRef.current,
        selRef.current, selConnRef.current,
        hovRef.current, dcRef.current, lassoRefV.current, bgColor,
        magnetSegsRef.current,
      )
    })
  }, [bgColor])

  renderAfterResizeRef.current = requestRender

  useEffect(() => { requestRender() }, [data, zoom, panX, panY, selectedIds, selectedConnIds, hoveredShapeId, drawingConn, lasso, bgColor])
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
    const shape = getShapeAt(data.shapes, w.x, w.y)
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
    const hit = getConnectorAt(data.connectors, data.shapes, w.x, w.y, 10 / zoom)
    if (!hit) { setCtxMenu(null); return }
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

    // Check if drawing connector
    const port = getPortAt(data.shapes, w.x, w.y)
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
    const shape = getShapeAt(data.shapes, w.x, w.y)
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
      let selSet: Set<string>
      if (selectedIds.has(shape.id)) {
        selSet = e.shiftKey ? new Set([...selectedIds]) : selectedIds
      } else {
        selSet = e.shiftKey ? new Set([...selectedIds, shape.id]) : new Set([shape.id])
        setSelectedIds(selSet)
        setSelectedConnIds(new Set())
        selRef.current = selSet
      }
      // Start move (déplace toutes les formes sélectionnées simultanément)
      const ids = [...selSet]
      dragRef.current = {
        type: 'move', shapeId: shape.id,
        startWx: w.x, startWy: w.y,
        origX: shape.x, origY: shape.y, origW: shape.w, origH: shape.h,
        offsets: ids.map((sid) => {
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
    const hitConn = getConnectorAt(data.connectors, data.shapes, w.x, w.y, 10 / zoom)
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

      mutateData((d) => ({
        ...d,
        shapes: d.shapes.map((s) => {
          const off = offsets.find((o) => o.id === s.id)
          if (!off) return s
          let nx = Math.round(w.x + off.dx), ny = Math.round(w.y + off.dy)
          if (s.id === dr.shapeId) {
            if (snapX != null) nx = Math.round(snapX - s.w / 2)
            if (snapY != null) ny = Math.round(snapY - s.h / 2)
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
    const shape = getShapeAt(data.shapes, w.x, w.y)
    setHoveredShapeId(shape?.id ?? null)

    // Cursor
    if (shape) {
      canvasRef.current!.style.cursor = 'move'
    } else {
      const handle = getHandleAt(data.shapes, selectedIds, w.x, w.y, HANDLE_MARGIN / zoom)
      if (handle) {
        canvasRef.current!.style.cursor = handle.cursor
      } else if (selectedConnIds.size > 0 && getConnectorAt(data.connectors, data.shapes, w.x, w.y, 8 / zoom)) {
        canvasRef.current!.style.cursor = 'move' // déplacer la portion du connecteur
      } else {
        canvasRef.current!.style.cursor = 'default'
      }
    }
  }

  const handleMouseUp = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const w = getWorldPos(e)

    panRef.current = null

    // Fin de déplacement : le surlignage vert provisoire disparaît (positionnement
    // accepté), on revient à la couleur normale.
    const hadGreen = magnetSegsRef.current !== null
    magnetSegsRef.current = null

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
      const port = getPortAt(data.shapes, w.x, w.y)
      const targetId = port ? port.shapeId : (getShapeAt(data.shapes, w.x, w.y)?.id ?? null)
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
            .filter((s) => s.x >= x1 && s.x + s.w <= x2 && s.y >= y1 && s.y + s.h <= y2)
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

    const shape = getShapeAt(data.shapes, w.x, w.y)
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
    const hitConn = getConnectorAt(data.connectors, data.shapes, w.x, w.y, 10 / zoom)
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
  const [activeCategory, setActiveCategory] = useState('basic')
  const categories = getCategories()

  const displayedStencils = stencilSearch
    ? searchStencils(stencilSearch, s => t('stencil_' + s.id, { defaultValue: s.name }))
    : getStencilsByCategory(activeCategory)

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

  const diagRibbon: RibbonTab[] = [
    { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
      fileGroup(t, { onNew: () => createDiagMut.mutate(), onDuplicate: () => duplicateDiagMut.mutate(), extra: [
        { id: 'export', kind: 'button', icon: <Download size={15} />, label: t('diag_export_json'), onClick: exportJson },
      ] }),
      { id: 'view', label: t('doc_grp_zoom', { defaultValue: 'Zoom' }), items: [
        { id: 'zin', kind: 'button', icon: <ZoomIn size={15} />, tooltip: t('diag_zoom_in', { defaultValue: 'Zoom avant' }), onClick: () => setZoom(z => Math.min(MAX_ZOOM, +(z * 1.2).toFixed(2))) },
        { id: 'zout', kind: 'button', icon: <ZoomOut size={15} />, tooltip: t('diag_zoom_out', { defaultValue: 'Zoom arrière' }), onClick: () => setZoom(z => Math.max(MIN_ZOOM, +(z / 1.2).toFixed(2))) },
        { id: 'zreset', kind: 'button', icon: <RotateCcw size={15} />, tooltip: t('diag_reset_view', { defaultValue: 'Réinitialiser la vue' }), onClick: () => { setZoom(1); setPanX(60); setPanY(60) } },
      ] },
      { id: 'sel', label: t('doc_grp_editing', { defaultValue: 'Édition' }), items: [
        { id: 'props', kind: 'toggle', icon: <Network size={15} />, label: t('diag_properties'), active: showProps, onClick: () => setShowProps(v => !v) },
        { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete', { defaultValue: 'Supprimer' }), disabled: !hasSel, onClick: deleteSelected },
      ] },
    ] },
    { id: 'ctx-arrange', label: t('diag_tab_arrange', { defaultValue: 'Disposition' }), contextual: { accent: OFFICE_TONE.diagrams }, visible: selectedIds.size >= 2, groups: [
      { id: 'align', label: t('doc_grp_arrange', { defaultValue: 'Alignement' }), items: ([
        ['left', <AlignLeft size={15} />, t('diag_align_left')], ['center', <AlignCenter size={15} />, t('diag_align_center_h')], ['right', <AlignRight size={15} />, t('diag_align_right')],
        ['top', <AlignStartVertical size={15} />, t('diag_align_top')], ['middle', <AlignCenterVertical size={15} />, t('diag_align_center_v')], ['bottom', <AlignEndVertical size={15} />, t('diag_align_bottom')],
      ] as Array<[string, React.ReactNode, string]>).map(([a, icon, label]) => ({ id: 'al-' + a, kind: 'button' as const, icon, label, onClick: () => align(a) })) },
    ] },
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

  return (
    <OfficeShell
      ribbon={diagRibbon}
      theme={THEME_DIAGRAMS}
      chromeless
      topbarHeight={64}
      onBack={() => navigate('/office/diagrams')}
      titleIcon={<Network size={16} className="text-white/90 flex-shrink-0" />}
      title={title}
      onTitleChange={handleTitleChange}
      titlePlaceholder={t('common_untitled')}
      saveStatus={saveStatus === 'saving' ? t('diag_saving') : saveStatus === 'unsaved' ? t('diag_unsaved') : t('doc_saved')}
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

      {/* ── Body ── */}
      <div className="flex flex-1 overflow-hidden">

        {/* ── Stencil panel ── */}
        <div className="flex-shrink-0 w-52 bg-white border-r border-border flex flex-col overflow-hidden">
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

          {/* Sélecteur de catégorie (liste déroulante) */}
          {!stencilSearch && (
            <div className="flex-shrink-0 border-b border-border p-2">
              <Dropdown
                className="w-full"
                value={activeCategory}
                onChange={setActiveCategory}
                options={categories.map(cat => ({ value: cat, label: t('stencil_cat_' + cat, { defaultValue: cat === 'hardware' ? 'Ordinateur et Matériel' : cat }) }))}
              />
            </div>
          )}

          {/* Shape list */}
          <div className="flex-1 overflow-y-auto p-2">
            <div className="grid grid-cols-2 gap-1.5">
              {displayedStencils.map((stencil) => (
                <div
                  key={stencil.id}
                  draggable
                  onDragStart={() => handleStencilDragStart(stencil)}
                  onClick={() => setPendingStencil(pendingStencil?.id === stencil.id ? null : stencil)}
                  className={`
                    flex flex-col items-center gap-1 p-1.5 rounded cursor-pointer
                    border transition-all select-none
                    ${pendingStencil?.id === stencil.id
                      ? 'border-primary bg-primary/5'
                      : 'border-transparent hover:bg-surface-2 hover:border-border'}
                  `}
                >
                  <StencilThumbnail stencil={stencil} />
                  <span className="text-[10px] text-text-secondary text-center leading-tight line-clamp-1">
                    {t('stencil_' + stencil.id, { defaultValue: stencil.name })}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {pendingStencil && (
            <div className="flex-shrink-0 px-2 py-1.5 border-t border-border bg-primary/5 text-xs text-primary text-center">
              {t('diag_click_to_place')}
            </div>
          )}
        </div>

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
                { type: 'label', text: t('diag_ctx_line', { defaultValue: 'Trait' }) },
                { type: 'action', label: t('diag_line_solid'),  checked: conn.style.strokeStyle === 'solid',  onClick: () => updateConnStyle(conn.id, { strokeStyle: 'solid' }) },
                { type: 'action', label: t('diag_line_dashed'), checked: conn.style.strokeStyle === 'dashed', onClick: () => updateConnStyle(conn.id, { strokeStyle: 'dashed' }) },
                { type: 'action', label: t('diag_line_dotted'), checked: conn.style.strokeStyle === 'dotted', onClick: () => updateConnStyle(conn.id, { strokeStyle: 'dotted' }) },
                { type: 'action', label: t('diag_ctx_arrow_end', { defaultValue: 'Flèche de fin' }), checked: conn.style.arrowEnd !== 'none', onClick: () => updateConnStyle(conn.id, { arrowEnd: conn.style.arrowEnd === 'none' ? 'block' : 'none' }) },
                { type: 'separator' },
                { type: 'action', label: t('diag_ctx_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => connDuplicate(conn.id) },
                { type: 'action', label: t('diag_ctx_to_front', { defaultValue: 'Mettre au premier plan' }), icon: <ArrowUp size={14} />, onClick: () => connReorder(conn.id, true) },
                { type: 'action', label: t('diag_ctx_to_back', { defaultValue: 'Mettre à l’arrière-plan' }), icon: <ArrowDown size={14} />, onClick: () => connReorder(conn.id, false) },
                { type: 'separator' },
                { type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={14} />, onClick: () => deleteConnector(conn.id) },
              )
            } else {
              const shape = data.shapes.find((s) => s.id === ctxMenu.shapeId)
              if (!shape) return null
              items.push(
                { type: 'action', label: t('diag_ctx_edit_text', { defaultValue: 'Modifier le texte' }), icon: <Pencil size={14} />, onClick: () => startLabelEdit(shape.id, false, shape.label) },
                { type: 'action', label: t('diag_ctx_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => shapeDuplicate(shape.id) },
                { type: 'separator' },
                { type: 'submenu', label: t('diag_ctx_arrange', { defaultValue: 'Disposition' }), items: [
                  { type: 'action', label: t('diag_ctx_to_front', { defaultValue: 'Mettre au premier plan' }), icon: <ArrowUp size={14} />, onClick: () => shapeReorder(shape.id, 'front') },
                  { type: 'action', label: t('diag_ctx_forward', { defaultValue: 'Avancer' }), onClick: () => shapeReorder(shape.id, 'forward') },
                  { type: 'action', label: t('diag_ctx_backward', { defaultValue: 'Reculer' }), onClick: () => shapeReorder(shape.id, 'backward') },
                  { type: 'action', label: t('diag_ctx_to_back', { defaultValue: 'Mettre à l’arrière-plan' }), icon: <ArrowDown size={14} />, onClick: () => shapeReorder(shape.id, 'back') },
                ] },
              )
              if (selectedIds.size >= 2 && selectedIds.has(shape.id)) {
                items.push({ type: 'submenu', label: t('diag_ctx_align', { defaultValue: 'Aligner' }), items: [
                  { type: 'action', label: t('diag_align_left'),     icon: <AlignLeft size={14} />,           onClick: () => align('left') },
                  { type: 'action', label: t('diag_align_center_h'), icon: <AlignCenter size={14} />,         onClick: () => align('center') },
                  { type: 'action', label: t('diag_align_right'),    icon: <AlignRight size={14} />,          onClick: () => align('right') },
                  { type: 'action', label: t('diag_align_top'),      icon: <AlignStartVertical size={14} />,  onClick: () => align('top') },
                  { type: 'action', label: t('diag_align_center_v'), icon: <AlignCenterVertical size={14} />, onClick: () => align('middle') },
                  { type: 'action', label: t('diag_align_bottom'),   icon: <AlignEndVertical size={14} />,    onClick: () => align('bottom') },
                ] })
              }
              items.push(
                { type: 'separator' },
                { type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={14} />, onClick: () => deleteShape(shape.id) },
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
        </div>

        {/* ── Properties panel ── */}
        {showProps && (
          <div className="flex-shrink-0 w-56 bg-white border-l border-border flex flex-col overflow-y-auto">
            <div className="px-3 py-2.5 border-b border-border">
              <p className="text-xs font-medium text-text-secondary uppercase tracking-wide">{t('diag_properties')}</p>
            </div>

            {selectedShape && (
              <div className="p-3 space-y-4">
                {/* Dimensions */}
                <div>
                  <p className="text-xs text-text-tertiary mb-2">{t('diag_dimensions')}</p>
                  <div className="grid grid-cols-2 gap-2">
                    {(['x', 'y', 'w', 'h'] as const).map((k) => (
                      <div key={k}>
                        <label className="text-[10px] text-text-tertiary uppercase">{k === 'w' ? t('diag_width') : k === 'h' ? t('diag_height') : k.toUpperCase()}</label>
                        <input
                          type="number"
                          value={Math.round(selectedShape[k])}
                          onChange={(e) => {
                            const v = parseInt(e.target.value)
                            if (!isNaN(v)) mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => s.id === selectedShape.id ? { ...s, [k]: v } : s) }))
                          }}
                          className="w-full mt-0.5 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary"
                        />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Fill */}
                <div>
                  <p className="text-xs text-text-tertiary mb-2">{t('diag_fill')}</p>
                  <div className="flex items-center gap-2">
                    <input
                      type="color"
                      value={mergeStyle(selectedShape.style).fillColor}
                      onChange={(e) => updateShapeStyle(selectedShape.id, { fillColor: e.target.value })}
                      className="w-7 h-7 rounded cursor-pointer border border-border"
                    />
                    <input
                      type="text"
                      value={mergeStyle(selectedShape.style).fillColor}
                      onChange={(e) => updateShapeStyle(selectedShape.id, { fillColor: e.target.value })}
                      className="flex-1 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary font-mono"
                    />
                  </div>
                </div>

                {/* Stroke */}
                <div>
                  <p className="text-xs text-text-tertiary mb-2">{t('diag_stroke')}</p>
                  <div className="flex items-center gap-2 mb-2">
                    <input
                      type="color"
                      value={mergeStyle(selectedShape.style).strokeColor}
                      onChange={(e) => updateShapeStyle(selectedShape.id, { strokeColor: e.target.value })}
                      className="w-7 h-7 rounded cursor-pointer border border-border"
                    />
                    <input
                      type="number"
                      min={0.5} max={10} step={0.5}
                      value={mergeStyle(selectedShape.style).strokeWidth}
                      onChange={(e) => updateShapeStyle(selectedShape.id, { strokeWidth: parseFloat(e.target.value) })}
                      className="w-14 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary"
                    />
                  </div>
                  <Dropdown
                    className="w-full"
                    height={24}
                    fontSize={12}
                    value={mergeStyle(selectedShape.style).strokeStyle}
                    onChange={v => updateShapeStyle(selectedShape.id, { strokeStyle: v as ShapeStyle['strokeStyle'] })}
                    options={[
                      { value: 'solid',  label: t('diag_line_solid') },
                      { value: 'dashed', label: t('diag_line_dashed') },
                      { value: 'dotted', label: t('diag_line_dotted') },
                    ]}
                  />
                </div>

                {/* Text */}
                <div>
                  <p className="text-xs text-text-tertiary mb-2">{t('diag_text')}</p>
                  <input
                    type="text"
                    value={selectedShape.label}
                    onChange={(e) => mutateData((d) => ({ ...d, shapes: d.shapes.map((s) => s.id === selectedShape.id ? { ...s, label: e.target.value } : s) }))}
                    className="w-full px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary mb-2"
                    placeholder={t('diag_label_placeholder')}
                  />
                  <div className="flex items-center gap-2">
                    <input
                      type="color"
                      value={(selectedShape.labelStyle as LabelStyle)?.color ?? '#000000'}
                      onChange={(e) => updateShapeLabelStyle(selectedShape.id, { color: e.target.value })}
                      className="w-7 h-7 rounded cursor-pointer border border-border"
                    />
                    <input
                      type="number" min={8} max={72}
                      value={(selectedShape.labelStyle as LabelStyle)?.fontSize ?? 12}
                      onChange={(e) => updateShapeLabelStyle(selectedShape.id, { fontSize: parseInt(e.target.value) })}
                      className="w-14 px-1.5 py-0.5 text-xs border border-border rounded outline-none focus:border-primary"
                    />
                  </div>
                </div>

                {/* Opacity */}
                <div>
                  <p className="text-xs text-text-tertiary mb-2">{t('diag_opacity', { value: mergeStyle(selectedShape.style).opacity })}</p>
                  <input
                    type="range" min={0} max={100}
                    value={mergeStyle(selectedShape.style).opacity}
                    onChange={(e) => updateShapeStyle(selectedShape.id, { opacity: parseInt(e.target.value) })}
                    className="w-full"
                  />
                </div>

                {/* Delete */}
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
        )}
      </div>

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
      </div>
    </OfficeShell>
  )
}
