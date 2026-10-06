// Direct-manipulation geometry for floating sheet objects (charts today, any
// boxed overlay tomorrow): hit-testing, resize/rotate handles and the selection
// chrome. Kept free of React so it can be unit-tested and shared by the canvas
// painter and the pointer handlers alike.
//
// All coordinates are grid CONTENT-space pixels (origin = grid data origin,
// zoom already applied), matching what `chartRect`/`imageRect` return.

export type ObjHandle = 'move' | 'rotate' | 'nw' | 'n' | 'ne' | 'w' | 'e' | 'sw' | 's' | 'se'

export interface ObjRect { x: number; y: number; w: number; h: number; rot: number }
/** Persisted box in base pixels (zoom = 1) from the grid data origin. */
export interface ObjBox { bx: number; by: number; bw: number; bh: number }

/** Pointer slop around a handle centre, and the rotation handle's arm length. */
export const HANDLE_HIT = 7
/**
 * Selection chrome, transcribed from the PRESENTATIONS editor's interactive
 * overlay (PresentationEditorPage, the `corners` map and the edge divs) so the
 * suite has ONE look:
 *   • frame: 2 px accent border, drawn just OUTSIDE the box. The presentations
 *     overlay uses `inset-0`, but a CSS border paints inward over an element that
 *     has no outline of its own; on a sheet a white shape with a thin black stroke
 *     would simply disappear under it, so the frame is offset by its own width;
 *   • corners: 10 px white circles, 2 px accent border (`w-2.5`, offset -5);
 *   • edges: PILLS, not circles — 20×8 top/bottom, 8×20 left/right (`w-5 h-2`);
 *   • rotation: a 2 px stem 24 px long, then a 20 px disc holding the icon.
 */
export const CHROME_BORDER = 2
/** Gap between the object's edge and the frame, so its own outline stays visible. */
export const CHROME_GAP = 2
export const CORNER_R = 5
/** Long and short half-extents of an edge pill. */
export const PILL_LONG = 10
export const PILL_SHORT = 4
export const ROTATE_KNOB_R = 10
export const ROTATE_ARM = 24
/** Smallest object side, in base pixels. */
export const MIN_SIDE = 24

/** The eight resize handles, in content space (rotation NOT applied). */
export function handlePoints(r: ObjRect): [ObjHandle, number, number][] {
  const cx = r.x + r.w / 2, cy = r.y + r.h / 2
  // Centred ON the box edges, like the presentations overlay.
  const l = r.x - CHROME_GAP, t = r.y - CHROME_GAP
  const rt = r.x + r.w + CHROME_GAP, b = r.y + r.h + CHROME_GAP
  return [
    ['nw', l, t], ['n', cx, t], ['ne', rt, t],
    ['w', l, cy], ['e', rt, cy],
    ['sw', l, b], ['s', cx, b], ['se', rt, b],
  ]
}

/** Half-extents of a handle's hit area: pills are wide, corners are round. */
export function handleExtent(h: ObjHandle): { hx: number; hy: number } {
  if (h === 'n' || h === 's') return { hx: PILL_LONG, hy: PILL_SHORT }
  if (h === 'w' || h === 'e') return { hx: PILL_SHORT, hy: PILL_LONG }
  return { hx: CORNER_R, hy: CORNER_R }
}

/**
 * Map a content-space point into the object's own (unrotated) frame, so every
 * hit test below can work on a plain axis-aligned rectangle.
 */
export function toLocal(r: ObjRect, px: number, py: number): { lx: number; ly: number } {
  const cx = r.x + r.w / 2, cy = r.y + r.h / 2
  const a = -(r.rot || 0) * Math.PI / 180
  const dx = px - cx, dy = py - cy
  return { lx: cx + dx * Math.cos(a) - dy * Math.sin(a), ly: cy + dx * Math.sin(a) + dy * Math.cos(a) }
}

/**
 * Where the rotation knob sits, in the object's own frame. Normally above the top
 * edge; it FLIPS below the object when there is not enough room above (`minTop` =
 * first usable content row, i.e. just under the column headers). Without the flip
 * the knob would land in the header band, which the grid handles before any object
 * hit test — making it unreachable for objects near the top of the sheet.
 */
export function knobPoint(r: ObjRect, minTop = -Infinity): { kx: number; ky: number; below: boolean } {
  const kx = r.x + r.w / 2
  const below = r.y - CHROME_GAP - ROTATE_ARM - ROTATE_KNOB_R < minTop
  return {
    kx,
    ky: below
      ? r.y + r.h + CHROME_GAP + ROTATE_ARM + ROTATE_KNOB_R
      : r.y - CHROME_GAP - ROTATE_ARM - ROTATE_KNOB_R,
    below,
  }
}

/**
 * Which part of the object is under a point: a handle (selected objects only,
 * they are the only ones showing chrome), the body, or nothing.
 */
export function hitObject(r: ObjRect, px: number, py: number, selected: boolean, minTop?: number): ObjHandle | null {
  if (r.w <= 0 || r.h <= 0) return null
  const { lx, ly } = toLocal(r, px, py)
  if (selected) {
    const { kx, ky } = knobPoint(r, minTop)
    if (Math.hypot(lx - kx, ly - ky) <= ROTATE_KNOB_R + 2) return 'rotate'
    for (const [h, hx, hy] of handlePoints(r)) {
      const e = handleExtent(h)
      if (Math.abs(lx - hx) <= e.hx + 2 && Math.abs(ly - hy) <= e.hy + 2) return h
    }
  }
  if (lx >= r.x && lx <= r.x + r.w && ly >= r.y && ly <= r.y + r.h) return 'move'
  return null
}

/** Mouse cursor for a handle, rotated with the object so it keeps pointing the right way. */
export function cursorFor(handle: ObjHandle, rot = 0): string {
  if (handle === 'move') return 'move'
  if (handle === 'rotate') return 'grab'
  // Compass angle of the handle, turned by the object's rotation, snapped to the
  // four available bidirectional resize cursors.
  const base: Record<string, number> = { n: 0, ne: 45, e: 90, se: 135, s: 180, sw: 225, w: 270, nw: 315 }
  const a = (((base[handle] ?? 0) + rot) % 180 + 180) % 180
  if (a < 22.5 || a >= 157.5) return 'ns-resize'
  if (a < 67.5) return 'nesw-resize'
  if (a < 112.5) return 'ew-resize'
  return 'nwse-resize'
}

/**
 * Apply a drag to a box. `wdx`/`wdy` are the pointer delta in content pixels;
 * resizing happens along the object's LOCAL axes so a rotated object grows the
 * way it looks like it should.
 */
export function applyDrag(
  box: ObjBox, handle: ObjHandle, wdx: number, wdy: number, rot: number, zoom: number,
): ObjBox {
  if (handle === 'move') return { ...box, bx: box.bx + wdx / zoom, by: box.by + wdy / zoom }
  const a = -(rot || 0) * Math.PI / 180
  const ldx = wdx * Math.cos(a) - wdy * Math.sin(a)
  const ldy = wdx * Math.sin(a) + wdy * Math.cos(a)
  let { bx, by, bw, bh } = box
  if (handle.includes('e')) bw = Math.max(MIN_SIDE, box.bw + ldx / zoom)
  if (handle.includes('s')) bh = Math.max(MIN_SIDE, box.bh + ldy / zoom)
  if (handle.includes('w')) { const n = Math.max(MIN_SIDE, box.bw - ldx / zoom); bx = box.bx + (box.bw - n); bw = n }
  if (handle.includes('n')) { const n = Math.max(MIN_SIDE, box.bh - ldy / zoom); by = box.by + (box.bh - n); bh = n }
  return { bx, by, bw, bh }
}

/** Rotation (degrees) for a pointer at (px, py) around the box centre, 0 = handle up. */
export function rotationFor(centerX: number, centerY: number, px: number, py: number, snap = false): number {
  const deg = Math.atan2(py - centerY, px - centerX) * 180 / Math.PI + 90
  const r = Math.round(deg)
  return snap ? Math.round(r / 15) * 15 : r
}

/**
 * Paint the selection chrome (outline, eight handles, rotation arm) for an
 * object. Drawn on the top overlay canvas so it sits ABOVE the DOM object.
 * `offX`/`offY` are the pane's scroll offset.
 */
export function drawObjectChrome(
  ctx: CanvasRenderingContext2D, r: ObjRect, offX = 0, offY = 0, accent = '#1a73e8', minTop?: number,
): void {
  const x = r.x - offX, y = r.y - offY
  const cx = x + r.w / 2, cy = y + r.h / 2
  // Knob side is decided in unscrolled content space, then drawn at the scrolled y.
  const { ky, below } = knobPoint(r, minTop)
  const knobY = ky - offY
  const armFrom = below ? y + r.h + CHROME_GAP : y - CHROME_GAP
  /** Rounded rectangle centred on (px, py) — the edge pills. */
  const pill = (px: number, py: number, halfX: number, halfY: number) => {
    const rad = Math.min(halfX, halfY)
    ctx.beginPath()
    ctx.roundRect(px - halfX, py - halfY, halfX * 2, halfY * 2, rad)
    ctx.fill(); ctx.stroke()
  }
  ctx.save()
  if (r.rot) { ctx.translate(cx, cy); ctx.rotate(r.rot * Math.PI / 180); ctx.translate(-cx, -cy) }
  ctx.strokeStyle = accent; ctx.lineWidth = CHROME_BORDER; ctx.setLineDash([])
  ctx.strokeRect(x - CHROME_GAP, y - CHROME_GAP, r.w + CHROME_GAP * 2, r.h + CHROME_GAP * 2)
  // Rotation: stem, disc, and the circular arrow drawn inside it.
  ctx.beginPath(); ctx.moveTo(cx, armFrom); ctx.lineTo(cx, knobY); ctx.stroke()
  ctx.fillStyle = '#fff'
  ctx.beginPath(); ctx.arc(cx, knobY, ROTATE_KNOB_R, 0, Math.PI * 2); ctx.fill(); ctx.stroke()
  ctx.save()
  ctx.lineWidth = 1.5
  // Circular arrow: an arc with a gap, and the head ON the arc's end, aligned with
  // its TANGENT. Placing the head at hand-picked coordinates (what this did before)
  // leaves it floating beside the arc as soon as the radius or the sweep changes.
  const gr = 5
  const aStart = -Math.PI * 0.6, aEnd = Math.PI * 0.95
  ctx.beginPath(); ctx.arc(cx, knobY, gr, aStart, aEnd); ctx.stroke()
  const px = cx + gr * Math.cos(aStart), py = knobY + gr * Math.sin(aStart)
  // Tangent of a clockwise sweep at aStart points backwards along the circle.
  const tx = Math.sin(aStart), ty = -Math.cos(aStart)
  const nx = -ty, ny = tx
  const HEAD = 4.2, HALF = 2.6
  ctx.beginPath()
  ctx.moveTo(px + tx * HEAD, py + ty * HEAD)
  ctx.lineTo(px + nx * HALF, py + ny * HALF)
  ctx.lineTo(px - nx * HALF, py - ny * HALF)
  ctx.closePath(); ctx.fillStyle = accent; ctx.fill()
  ctx.restore()
  ctx.fillStyle = '#fff'
  for (const [h, hx, hy] of handlePoints({ ...r, x, y })) {
    const e = handleExtent(h)
    if (h === 'n' || h === 's' || h === 'w' || h === 'e') pill(hx, hy, e.hx, e.hy)
    else { ctx.beginPath(); ctx.arc(hx, hy, CORNER_R, 0, Math.PI * 2); ctx.fill(); ctx.stroke() }
  }
  ctx.restore()
}
