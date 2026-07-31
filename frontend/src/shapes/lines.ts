// LINE-LIKE shapes: selection chrome and hit-testing.
//
// LibreOffice does NOT frame a line, an arrow, a connector or a curve with the
// 8-handle rectangle it puts around a filled shape. `SdrPathObj` (svx/source/
// svdraw/svdopath.cxx) builds its handle list from the PATH POINTS
// (`SdrHdlKind::Poly`, one handle per point) instead of the eight
// `SdrHdlKind::UpperLeft…LowerRight` handles `SdrObject::AddToHdlList` gives a
// rectangular object — and Impress/Draw therefore show a selected line as two dots
// at its ends, with no bounding box and no rotation knob (rotation of a segment is
// indistinguishable from moving its ends).
//
// This module reproduces that for the sheet's shapes: which geometries are
// path-like, where their endpoints are, how close the pointer must be to grab the
// stroke, and how to paint that chrome.

import type { ObjHandle, ObjRect } from '../sheet-chart/interaction'
import { CHROME_BORDER, CORNER_R } from '../sheet-chart/interaction'

/**
 * The geometries drawn as an open path — every entry of the catalogue's « Traits »
 * category. They are the ones that get endpoint handles instead of a frame.
 */
const LINE_KINDS = new Set<string>([
  'line', 'lineArrow', 'lineDouble',
  'elbowConnector', 'elbowArrow', 'elbowDoubleArrow',
  'curveConnector', 'curveArrow', 'curveDoubleArrow', 'curve',
])

export function isLineShape(kind: string): boolean {
  return LINE_KINDS.has(kind)
}

/** An endpoint, and the resize handle that drags it. */
export interface LineEnd { handle: ObjHandle; x: number; y: number }

/**
 * The two ends of a line-like shape, in content space.
 *
 * Every one of them is drawn from the box's BOTTOM-LEFT to its TOP-RIGHT corner
 * (see shapes/svg.ts), except `curve`, a quadratic whose two ends sit on the
 * bottom edge. Dragging an end therefore IS resizing the box from the matching
 * corner, which is why each end carries that corner's handle.
 */
export function lineEnds(kind: string, r: ObjRect): LineEnd[] {
  const l = r.x, t = r.y, right = r.x + r.w, b = r.y + r.h
  if (kind === 'curve') {
    return [{ handle: 'sw', x: l, y: b }, { handle: 'se', x: right, y: b }]
  }
  return [{ handle: 'sw', x: l, y: b }, { handle: 'ne', x: right, y: t }]
}

/** Distance from a point to the segment [a, b]. */
function distToSegment(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax, dy = by - ay
  const len2 = dx * dx + dy * dy
  const u = len2 > 0 ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len2)) : 0
  return Math.hypot(px - (ax + u * dx), py - (ay + u * dy))
}

/**
 * Is the pointer ON the stroke? A straight line is hit near its segment (Draw uses
 * the pen width plus a tolerance), never anywhere in its bounding box — otherwise a
 * long diagonal would swallow every click in a huge empty rectangle. Elbows and
 * curves keep the box: their path wanders across it, and an approximation that
 * missed the stroke would make them unselectable.
 */
export function hitLineBody(kind: string, r: ObjRect, px: number, py: number, tol = 7): boolean {
  const inBox = px >= r.x - tol && px <= r.x + r.w + tol && py >= r.y - tol && py <= r.y + r.h + tol
  if (!inBox) return false
  if (kind === 'line' || kind === 'lineArrow' || kind === 'lineDouble') {
    return distToSegment(px, py, r.x, r.y + r.h, r.x + r.w, r.y) <= tol
  }
  return true
}

/**
 * Chrome of a selected line: its two endpoints, as the same white discs with an
 * accent border the corner handles use — and nothing else. No frame, no edge
 * pills, no rotation knob, exactly like Draw.
 */
export function drawLineChrome(
  ctx: CanvasRenderingContext2D, kind: string, r: ObjRect, offX = 0, offY = 0, accent = '#1a73e8',
): void {
  ctx.save()
  ctx.strokeStyle = accent; ctx.lineWidth = CHROME_BORDER; ctx.setLineDash([]); ctx.fillStyle = '#fff'
  for (const e of lineEnds(kind, r)) {
    ctx.beginPath()
    ctx.arc(e.x - offX, e.y - offY, CORNER_R, 0, Math.PI * 2)
    ctx.fill(); ctx.stroke()
  }
  ctx.restore()
}

/**
 * Which part of a selected line is under the pointer: one of its ends, its stroke,
 * or nothing. Mirrors `hitObject` for the rectangular objects.
 */
export function hitLineShape(
  kind: string, r: ObjRect, px: number, py: number, selected: boolean, slop = 7,
): ObjHandle | null {
  if (selected) {
    for (const e of lineEnds(kind, r)) {
      if (Math.abs(px - e.x) <= CORNER_R + slop / 2 && Math.abs(py - e.y) <= CORNER_R + slop / 2) return e.handle
    }
  }
  return hitLineBody(kind, r, px, py, slop) ? 'move' : null
}
