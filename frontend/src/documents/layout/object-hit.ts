/**
 * object-hit.ts — Object designation & selection rules (click / hover / z-order).
 *
 * Pure functions, no DOM, no module state. Modelled 1:1 on LibreOffice Writer +
 * svx. Every rule below carries the `file:line` of the LibreOffice source that
 * justifies it (tree: /home/martinien/libreoffice/core-master).
 *
 * Coordinate system
 * -----------------
 * All geometry is expressed in DOCUMENT pixels at 96 dpi (the same space as
 * `canvas-engine.ts`): the zoom is applied at PAINT time only, never to layout.
 * `x`/`y` are the top-left corner of the UNROTATED box, `rotation` is in degrees
 * (clockwise, around the box centre), exactly like the `image` node attributes.
 *
 * Summary of the imported rules
 * -----------------------------
 * 1. Hit tolerance — Writer raises svx's 2 px default to `MarkHdlSizePixel/2`
 *    = (3*2+1)/2 = 3 SCREEN pixels
 *    (feshview.cxx:1343 / svdmrkv.cxx:2472 / svdhdl.cxx:2180, svdpntv.cxx:139).
 *    It is a screen quantity converted to logical units by `PixelToLogic`
 *    (svdpntv.cxx:338, svdpntv.cxx:355) → in document pixels it is `3 / zoom`.
 *    OLE objects and text frames get DOUBLE tolerance (svdmrkv.cxx:2589-2593).
 * 2. Rough pass then exact pass — the bounding range grown by the tolerance is
 *    tested first (svdmrkv.cxx:2596, hittestprocessor2d.cxx:130-136), the exact
 *    geometry second.
 * 3. Rotation — the exact test transforms the HIT POINT by the inverse object
 *    transformation and tests it against the untransformed geometry
 *    (hittestprocessor2d.cxx:104-110); never against the axis-aligned box.
 * 4. Z-order — the object list is walked from the TOP (highest ordnum) down, the
 *    first hit wins (svdmrkv.cxx:2644-2646, sdrhittesthelper.cxx:101-110).
 * 5. Already-selected object wins — `PickAnything` runs a `SdrSearchOptions::MARKED`
 *    pass BEFORE the unmarked pass (svdview.cxx:339-341), and Writer refuses to
 *    re-pick while the point is inside the selected object (edtwin.cxx:3465-3467).
 * 6. Reaching the object underneath — NOT repeated clicks: it is Alt (`IsMod2`)
 *    + click, which calls `SdrMarkView::MarkNextObj(rPnt, nTol, bPrev)`
 *    (fusel.cxx:418-420 → svdmrkv.cxx:2265). That function starts at the ordnum
 *    of the topmost hit object and scans DOWNWARDS only — there is no wrap-around
 *    (svdmrkv.cxx:2331-2341, 2343-2365); when nothing is found the selection is
 *    left untouched. If the current object is not hit at all, it degrades to a
 *    plain pick (svdmrkv.cxx:2287).
 * 7. Object BEHIND the text — #i89920#: an object on the Hell layer with
 *    `WrapTextMode_THROUGH` (our `wrap: 'behind'`) is NOT selected when the click
 *    lands on a text character that overlaps it; the click goes to the text
 *    instead (feshview.cxx:1392-1444).
 * 8. Covered by a higher object — even after a hit, Writer scans every object
 *    ABOVE it in z-order; if one of their bounding rects contains the point, the
 *    selection is refused and the click falls back to the text
 *    (feshview.cxx:1458-1472).
 * 9. Contour — the exact fill test uses the even-odd rule
 *    (b2dpolypolygontools.cxx:171-183, b2dpolygontools.cxx:332-370), so a click
 *    in the hole of a ring does NOT select it; edges additionally hit within the
 *    tolerance (`isInEpsilonRange`, hittestprocessor2d.cxx:130-137,
 *    b2dpolygontools.cxx:1502-1542).
 * 10. Keyboard order (Tab / Shift+Tab) — `SwFEShell::GetBestObject` orders by
 *    DOCUMENT POSITION (top-left, y then x), not by z-order, and wraps around to
 *    the topmost-leftmost object (feshview.cxx:1699-1764).
 */

export interface HitObject {
  id: string
  /** Top-left of the unrotated box, document px. */
  x: number
  y: number
  w: number
  h: number
  /** Degrees, clockwise, around the box centre. */
  rotation: number
  /** `inline | square | tight | through | topBottom | behind | front`. */
  wrap: string
  /** Paint order: the higher, the closer to the viewer. Ties broken by array order. */
  z: number
  /**
   * Optional exact outline, in the object's LOCAL frame: origin = top-left of the
   * unrotated box, so a plain rectangle would be (0,0)(w,0)(w,h)(0,h). Implicitly
   * closed. A single self-intersecting path (outer ring + inner ring) works: the
   * fill rule is even-odd, cf. b2dpolypolygontools.cxx:171-183.
   */
  contour?: Array<{ x: number; y: number }>
  /**
   * Text frame / OLE object: doubles the tolerance, cf. svdmrkv.cxx:2589-2593
   * ("double tolerance for OLE, text frames and objects in active text edit").
   */
  textFrame?: boolean
}

export interface HitOpts {
  /**
   * Tolerance in SCREEN pixels. Default 3 = Writer's `MarkHdlSizePixel/2`
   * (feshview.cxx:1343). It is divided by `zoom` to reach document pixels,
   * exactly like `OutputDevice::PixelToLogic` (svdpntv.cxx:355).
   */
  tolerance?: number
  /** Painting zoom; the tolerance is a screen quantity, so it is divided by it. */
  zoom?: number
  /** Alt (`IsMod2`) held: walk to the object underneath, cf. fusel.cxx:418-420. */
  altKey?: boolean
  /** Currently selected object, used by the MARKED pass and by Alt cycling. */
  currentId?: string | null
  /**
   * Is there a text character painted at that point? Only consulted for objects
   * BEHIND the text (rule 7). Default `true` — i.e. the text wins, which is what
   * Writer does everywhere a paragraph is laid out (feshview.cxx:1406-1444).
   */
  hasTextAt?: (x: number, y: number) => boolean
}

/**
 * Writer's click tolerance in screen pixels.
 * `SwFEShell` sets `SetHitTolerancePixel(GetMarkHdlSizePixel()/2)` (feshview.cxx:1343,
 * 1360, 1381); `GetMarkHdlSizePixel()` is `HdlSize*2+1` (svdmrkv.cxx:2472) and
 * `HdlSize` defaults to 3 (svdhdl.cxx:2180) → 7/2 = 3. `SwFEShell::SelectObj` uses
 * the very same value as `nMinMove` (feshview.cxx:299-306). svx alone would use 2
 * (svdpntv.cxx:139), which is also what `SwEditWin` uses for the mouse POINTER only
 * (edtwin.cxx:318 `HIT_PIX = 2`).
 */
export const DEFAULT_HIT_TOLERANCE_PX = 3

interface Pt {
  x: number
  y: number
}

/** Objects behind the text: Hell layer + WrapTextMode_THROUGH (feshview.cxx:1393-1404). */
function isBehindText(obj: HitObject): boolean {
  return obj.wrap === 'behind'
}

/**
 * Hit point brought back into the object's local frame (origin = top-left of the
 * unrotated box) by the INVERSE rotation, cf. hittestprocessor2d.cxx:104-110
 * ("cheaper to transform hit tolerance and position than to transform [...] the
 * polypolygon"). The transformation is a pure rotation, so distances — hence the
 * tolerance — are preserved and can be applied directly in the local frame.
 */
function toLocal(obj: HitObject, x: number, y: number): Pt {
  const cx = obj.x + obj.w / 2
  const cy = obj.y + obj.h / 2
  const dx = x - cx
  const dy = y - cy
  const rad = (-(obj.rotation || 0) * Math.PI) / 180
  const c = Math.cos(rad)
  const s = Math.sin(rad)
  return { x: dx * c - dy * s + obj.w / 2, y: dx * s + dy * c + obj.h / 2 }
}

/**
 * Axis-aligned bounding rect of the ROTATED object — svx's `GetCurrentBoundRect()`,
 * used both for the rough pass (svdmrkv.cxx:2596) and for the "covered by a higher
 * object" rule (feshview.cxx:1466).
 */
export function boundRect(obj: HitObject): { x0: number; y0: number; x1: number; y1: number } {
  const rad = ((obj.rotation || 0) * Math.PI) / 180
  const c = Math.abs(Math.cos(rad))
  const s = Math.abs(Math.sin(rad))
  const hw = (obj.w * c + obj.h * s) / 2
  const hh = (obj.w * s + obj.h * c) / 2
  const cx = obj.x + obj.w / 2
  const cy = obj.y + obj.h / 2
  return { x0: cx - hw, y0: cy - hh, x1: cx + hw, y1: cy + hh }
}

/** Squared distance from a point to a segment (basis of `isInEpsilonRange`). */
function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const vx = b.x - a.x
  const vy = b.y - a.y
  const len2 = vx * vx + vy * vy
  let t = 0
  if (len2 > 0) {
    t = ((p.x - a.x) * vx + (p.y - a.y) * vy) / len2
    t = t < 0 ? 0 : t > 1 ? 1 : t
  }
  const dx = p.x - (a.x + t * vx)
  const dy = p.y - (a.y + t * vy)
  return Math.hypot(dx, dy)
}

/**
 * Even-odd containment, port of `basegfx::utils::isInside(B2DPolygon, ...)`
 * (b2dpolygontools.cxx:332-370) — the crossing count parity. Combined with a
 * single path holding an outer and an inner ring, a click inside the hole of a
 * ring is correctly reported as OUTSIDE (b2dpolypolygontools.cxx:171-183).
 */
function isInsideContour(poly: readonly Pt[], p: Pt): boolean {
  const n = poly.length
  if (n < 3) return false
  let inside = false
  let cur = poly[n - 1]
  for (let i = 0; i < n; i++) {
    const prev = cur
    cur = poly[i]
    // cross-over in Y? (full precision, no epsilon — tdf#130150)
    if (prev.y > p.y !== cur.y > p.y) {
      const xCross = cur.x - ((cur.y - p.y) * (prev.x - cur.x)) / (prev.y - cur.y)
      if (xCross > p.x) inside = !inside
    }
  }
  return inside
}

/** Within `tol` of any edge of the (implicitly closed) contour — `isInEpsilonRange`. */
function isNearContour(poly: readonly Pt[], p: Pt, tol: number): boolean {
  if (tol <= 0 || poly.length < 2) return false
  for (let i = 0; i < poly.length; i++) {
    if (distToSegment(p, poly[i], poly[(i + 1) % poly.length]) <= tol) return true
  }
  return false
}

/**
 * Exact hit test of ONE object — svx's `CheckSingleSdrObjectHit`
 * (svdmrkv.cxx:2560-2630) reduced to what our objects can be.
 * `tol` is already expressed in document pixels.
 */
export function hitTestObject(obj: HitObject, x: number, y: number, tol: number): boolean {
  if (obj.w <= 0 || obj.h <= 0) return false
  // "double tolerance for OLE, text frames [...]" — svdmrkv.cxx:2589-2593.
  const t = Math.max(0, tol) * (obj.textFrame ? 2 : 1)

  // Rough pass on the bound rect grown by the tolerance — svdmrkv.cxx:2596 /
  // hittestprocessor2d.cxx:130-136.
  const b = boundRect(obj)
  if (x < b.x0 - t || x > b.x1 + t || y < b.y0 - t || y > b.y1 + t) return false

  const p = toLocal(obj, x, y)

  if (obj.contour && obj.contour.length >= 3) {
    // Edge-in-epsilon first, then even-odd fill — hittestprocessor2d.cxx:130-144.
    if (isNearContour(obj.contour, p, t)) return true
    return isInsideContour(obj.contour, p)
  }
  return p.x >= -t && p.x <= obj.w + t && p.y >= -t && p.y <= obj.h + t
}

/** Topmost first: z descending, ties broken by array order (= ordnum) descending. */
function topDown(objects: readonly HitObject[]): HitObject[] {
  return objects
    .map((o, i) => ({ o, i }))
    .sort((a, b) => b.o.z - a.o.z || b.i - a.i)
    .map(e => e.o)
}

/**
 * Rule 8 — feshview.cxx:1458-1472: after a hit, every object ABOVE the found one
 * is scanned; if one of their BOUND RECTS (axis-aligned, no tolerance) contains
 * the point, the selection is refused and the click goes back to the text.
 */
function isCoveredByHigherObject(found: HitObject, ordered: readonly HitObject[], x: number, y: number): boolean {
  for (const o of ordered) {
    if (o.id === found.id) break // `ordered` is top-down: we reached the found object
    const b = boundRect(o)
    if (x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1) return true
  }
  return false
}

/**
 * The object designated by a click at (x,y), or `null` when the click belongs to
 * the TEXT (caret placement).
 *
 * Order of the passes, mirroring `SdrView::PickAnything` (svdview.cxx:294-390):
 *   a. Alt held → walk down from the current object (svdmrkv.cxx:2265-2374);
 *   b. MARKED pass: the already-selected object wins (svdview.cxx:339-341);
 *   c. unmarked pass, topmost first (svdmrkv.cxx:2644-2646);
 *   d. Writer vetoes: object behind the text (feshview.cxx:1392-1444) and object
 *      covered by a higher one (feshview.cxx:1458-1472).
 */
export function pickObject(
  x: number,
  y: number,
  objects: readonly HitObject[],
  opts: HitOpts = {},
): HitObject | null {
  const zoom = opts.zoom && opts.zoom > 0 ? opts.zoom : 1
  // Screen tolerance → document pixels, cf. `PixelToLogic` (svdpntv.cxx:355).
  const tol = Math.max(0, opts.tolerance ?? DEFAULT_HIT_TOLERANCE_PX) / zoom
  const hasTextAt = opts.hasTextAt ?? (() => true)

  const ordered = topDown(objects)
  const hits = (o: HitObject) => hitTestObject(o, x, y, tol)
  // Rule 7 — an object behind the text yields to the text it overlaps.
  const selectable = (o: HitObject) => !(isBehindText(o) && hasTextAt(x, y))

  const current = opts.currentId ? (objects.find(o => o.id === opts.currentId) ?? null) : null

  // (a) Alt+click: next object DOWNWARDS from the current one, no wrap-around
  //     (svdmrkv.cxx:2331-2341 sets nSearchBeg = ordnum of the top hit, then the
  //     loop only decrements; svdmrkv.cxx:2365 leaves the mark untouched when
  //     nothing is found). If the current object is not hit, degrade to a plain
  //     pick (svdmrkv.cxx:2287 "nothing found, in this case, just select an object").
  if (opts.altKey && current && hits(current) && selectable(current)) {
    const start = ordered.findIndex(o => o.id === current.id)
    for (let i = start + 1; i < ordered.length; i++) {
      if (hits(ordered[i]) && selectable(ordered[i])) return ordered[i]
    }
    return current
  }

  // (b) MARKED pass — the selected object keeps the click even if another object
  //     is painted above it (svdview.cxx:339-341, edtwin.cxx:3465-3467).
  if (current && hits(current) && selectable(current)) return current

  // (c) unmarked pass, topmost first.
  for (const o of ordered) {
    if (!hits(o)) continue
    if (!selectable(o)) return null // rule 7: the click belongs to the text
    // (d) rule 8.
    if (isCoveredByHigherObject(o, ordered, x, y)) return null
    return o
  }
  return null
}

/**
 * Keyboard traversal order (Tab / Shift+Tab in Writer), port of
 * `SwFEShell::GetBestObject` (feshview.cxx:1699-1764). NOT the z-order: objects
 * are ordered by DOCUMENT POSITION — top-left corner, y first then x — with an
 * absolute priority for objects sharing the current y (feshview.cxx:1700-1731),
 * and a wrap-around to the topmost-leftmost object (feshview.cxx:1759-1764).
 */
export function nextObject(
  objects: readonly HitObject[],
  currentId: string | null,
  dir: 1 | -1 = 1,
): HitObject | null {
  if (objects.length === 0) return null
  const next = dir >= 0

  const current = currentId ? (objects.find(o => o.id === currentId) ?? null) : null
  // LibreOffice starts from the caret rect centre when nothing is selected
  // (feshview.cxx:1637); we start from outside the document so that every object
  // lies "after" (resp. "before") it.
  const SENT = Number.MAX_SAFE_INTEGER
  const pos: Pt = current
    ? { x: current.x, y: current.y }
    : next
      ? { x: -SENT, y: -SENT }
      : { x: SENT, y: SENT }

  let best: Pt = { x: next ? SENT : -SENT, y: next ? SENT : -SENT }
  let pBest: HitObject | null = null
  let top: Pt = { x: next ? SENT : -SENT, y: next ? SENT : -SENT }
  let pTop: HitObject | null = null

  for (const obj of objects) {
    const cur: Pt = { x: obj.x, y: obj.y }
    const isSelf = cur.x === pos.x && cur.y === pos.y

    // Special case: another object on the same y — it wins outright and the scan
    // stops (feshview.cxx:1699-1731).
    if (!isSelf && cur.y === pos.y && (next ? cur.x > pos.x : cur.x < pos.x)) {
      best = { x: next ? SENT : -SENT, y: next ? SENT : -SENT }
      pBest = null
      for (const t of objects) {
        const c: Pt = { x: t.x, y: t.y }
        if (
          !(c.x === pos.x && c.y === pos.y) &&
          c.y === pos.y &&
          (next ? c.x > pos.x : c.x < pos.x) &&
          (next ? c.x < best.x : c.x > best.x)
        ) {
          best = c
          pBest = t
        }
      }
      break
    }

    // Closest object below (resp. above), leftmost on ties (feshview.cxx:1733-1746).
    if (
      ((next ? pos.y < cur.y : pos.y > cur.y) && (next ? best.y > cur.y : best.y < cur.y)) ||
      (best.y === cur.y && (next ? best.x > cur.x : best.x < cur.x))
    ) {
      best = cur
      pBest = obj
    }

    // Global extremum, used for the wrap-around (feshview.cxx:1748-1756).
    if (
      (next ? top.y > cur.y : top.y < cur.y) ||
      (top.y === cur.y && (next ? top.x > cur.x : top.x < cur.x))
    ) {
      top = cur
      pTop = obj
    }
  }

  // "unfortunately nothing found" → wrap around (feshview.cxx:1758-1764).
  return pBest ?? pTop
}

// ---------------------------------------------------------------------------
// Self-test — `node --experimental-strip-types src/documents/layout/object-hit.ts`
// ---------------------------------------------------------------------------

function mk(id: string, x: number, y: number, w: number, h: number, extra: Partial<HitObject> = {}): HitObject {
  return { id, x, y, w, h, rotation: 0, wrap: 'front', z: 0, ...extra }
}

export function selfTest(): number {
  let n = 0
  const ok = (cond: boolean, label: string) => {
    if (!cond) throw new Error(`FAIL: ${label}`)
    n++
  }
  const eq = (got: HitObject | null, want: string | null, label: string) =>
    ok((got ? got.id : null) === want, `${label} (got ${got ? got.id : 'null'}, want ${want})`)

  // ── 1. Click at the centre ────────────────────────────────────────────────
  const a = mk('a', 100, 100, 200, 100)
  eq(pickObject(200, 150, [a]), 'a', 'centre')

  // ── 2. Click inside the tolerance, outside the edge (3 px, zoom 1) ────────
  eq(pickObject(302, 150, [a]), 'a', 'tolerance +2px on the right edge')
  eq(pickObject(100, 97, [a]), 'a', 'tolerance -3px on the top edge')
  // A text frame gets double tolerance — svdmrkv.cxx:2589-2593.
  const tb = mk('tb', 100, 100, 200, 100, { textFrame: true })
  eq(pickObject(305, 150, [tb]), 'tb', 'doubled tolerance for a text frame')
  eq(pickObject(305, 150, [a]), null, 'no doubling for a plain object')
  // The tolerance is a SCREEN quantity: at 200 % it is worth half as many
  // document px (svdpntv.cxx:355).
  eq(pickObject(301, 150, [a], { zoom: 2 }), 'a', '+1px @zoom2 (tol=1.5px doc)')
  eq(pickObject(302, 150, [a], { zoom: 2 }), null, '+2px @zoom2 out of tolerance')

  // ── 3. Click outside the tolerance ────────────────────────────────────────
  eq(pickObject(308, 150, [a]), null, 'out of tolerance → text')
  eq(pickObject(200, 250, [a]), null, 'far below → text')

  // ── 4. Object rotated 45° ─────────────────────────────────────────────────
  // A 100x100 square rotated 45° centred on (100,100): its AABB corners are
  // empty, only the diamond is hit.
  const r45 = mk('r45', 50, 50, 100, 100, { rotation: 45 })
  eq(pickObject(100, 100, [r45]), 'r45', 'rotated 45° — centre')
  eq(pickObject(100, 40, [r45]), 'r45', 'rotated 45° — diamond apex (in local frame)')
  eq(pickObject(58, 58, [r45]), null, 'rotated 45° — AABB corner is NOT a hit')
  ok(boundRect(r45).x0 < 50 - 20, 'rotated 45° — the AABB really is larger than the box')

  // ── 5. Two overlapping objects: the topmost wins ──────────────────────────
  const lo = mk('lo', 0, 0, 100, 100, { z: 1 })
  const hi = mk('hi', 50, 50, 100, 100, { z: 2 })
  eq(pickObject(75, 75, [lo, hi]), 'hi', 'z-order: the topmost wins')
  eq(pickObject(75, 75, [hi, lo]), 'hi', 'z-order independent of the array order')
  // Equal z → the later one in the list is on top (= ordnum).
  const e1 = mk('e1', 0, 0, 100, 100)
  const e2 = mk('e2', 0, 0, 100, 100)
  eq(pickObject(50, 50, [e1, e2]), 'e2', 'equal z → the last one is on top')
  // The already-selected object keeps the click (svdview.cxx:339-341).
  eq(pickObject(75, 75, [lo, hi], { currentId: 'lo' }), 'lo', 'MARKED pass: the selection wins')

  // ── 6. Alt+click reaches the object underneath ────────────────────────────
  eq(pickObject(75, 75, [lo, hi], { altKey: true, currentId: 'hi' }), 'lo', 'alt+click → the object below')
  // No wrap-around: nothing under 'lo' → the selection is kept (svdmrkv.cxx:2365).
  eq(pickObject(75, 75, [lo, hi], { altKey: true, currentId: 'lo' }), 'lo', 'alt+click with nothing below → unchanged')
  // Alt+click while the current object is not hit → plain pick (svdmrkv.cxx:2287).
  eq(pickObject(75, 75, [lo, hi], { altKey: true, currentId: 'none' }), 'hi', 'alt+click off the selection → plain pick')
  // Three superposed objects: two alt+clicks reach the bottom one.
  const t3 = [mk('c', 0, 0, 200, 200, { z: 1 }), mk('b', 0, 0, 200, 200, { z: 2 }), mk('t', 0, 0, 200, 200, { z: 3 })]
  eq(pickObject(100, 100, t3), 't', 'stack of 3 — plain click')
  eq(pickObject(100, 100, t3, { altKey: true, currentId: 't' }), 'b', 'stack of 3 — 1st alt+click')
  eq(pickObject(100, 100, t3, { altKey: true, currentId: 'b' }), 'c', 'stack of 3 — 2nd alt+click')

  // ── 7. Object behind the text ─────────────────────────────────────────────
  const wm = mk('wm', 0, 0, 400, 200, { wrap: 'behind' })
  eq(pickObject(200, 100, [wm]), null, 'behind the text + text present → the text wins')
  eq(pickObject(200, 100, [wm], { hasTextAt: () => false }), 'wm', 'behind the text, no character there → the object')
  // A 'through' (in front) object is NOT concerned by the rule.
  const th = mk('th', 0, 0, 400, 200, { wrap: 'through' })
  eq(pickObject(200, 100, [th]), 'th', "'through' (in front) is not affected by the behind-text rule")
  // Rule 8 — covered by a higher object: the hole of the ring is inside the
  // bound rect of a fly above it → no selection (feshview.cxx:1458-1472).
  const under = mk('under', 0, 0, 400, 200, { z: 1 })
  const ring = mk('ring', 100, 50, 100, 100, {
    z: 2,
    contour: [
      { x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }, { x: 0, y: 100 }, { x: 0, y: 0 },
      { x: 30, y: 30 }, { x: 30, y: 70 }, { x: 70, y: 70 }, { x: 70, y: 30 }, { x: 30, y: 30 },
    ],
  })
  eq(pickObject(150, 100, [under, ring]), null, 'hole of the ring above another object → text (rule 8)')

  // ── 8. Click in the hole of a concave contour ─────────────────────────────
  eq(pickObject(150, 100, [ring]), null, 'click in the hole of the ring → nothing')
  eq(pickObject(110, 100, [ring]), 'ring', 'click on the band of the ring → hit')
  eq(pickObject(98, 100, [ring]), 'ring', 'click 2px OUTSIDE the contour → hit (edge tolerance)')
  eq(pickObject(90, 100, [ring]), null, 'click 10px outside the contour → nothing')
  // The contour follows the rotation (it lives in the local frame).
  const ringR = { ...ring, rotation: 90 }
  eq(pickObject(150, 100, [ringR]), null, 'ring rotated 90° — the hole stays a hole')
  eq(pickObject(150, 60, [ringR]), 'ring', 'ring rotated 90° — band hit')

  // ── 9. Keyboard traversal order (Tab / Shift+Tab) ─────────────────────────
  // Positions on purpose NOT in z-order, to prove the order is positional.
  const k1 = mk('k1', 10, 10, 50, 50, { z: 9 })
  const k2 = mk('k2', 200, 10, 50, 50, { z: 1 }) // same y as k1, further right
  const k3 = mk('k3', 30, 300, 50, 50, { z: 5 })
  const objs = [k3, k2, k1]
  eq(nextObject(objs, null, 1), 'k1', 'Tab without selection → the topmost-leftmost')
  eq(nextObject(objs, 'k1', 1), 'k2', 'Tab: same y → the one to the right first')
  eq(nextObject(objs, 'k2', 1), 'k3', 'Tab: then the next line down')
  eq(nextObject(objs, 'k3', 1), 'k1', 'Tab: wrap-around to the first')
  eq(nextObject(objs, 'k3', -1), 'k2', 'Shift+Tab: backwards')
  eq(nextObject(objs, 'k2', -1), 'k1', 'Shift+Tab: same y, to the left')
  eq(nextObject([], null, 1), null, 'no object → null')
  eq(nextObject(objs, 'unknown', 1), 'k1', 'unknown id → behaves like no selection')

  // ── 10. Degenerate cases ──────────────────────────────────────────────────
  eq(pickObject(50, 50, [mk('z0', 0, 0, 0, 0)]), null, 'zero-sized object is never hit')
  eq(pickObject(50, 50, []), null, 'empty list → text')
  eq(pickObject(302, 150, [a], { tolerance: 0 }), null, 'tolerance 0 → strict edge')

  return n
}

// Direct execution: `node --experimental-strip-types .../object-hit.ts`.
const runtime = globalThis as unknown as { process?: { argv?: string[] } }
if (runtime.process?.argv?.[1]?.endsWith('object-hit.ts')) {
  const green = selfTest()
  console.log(`object-hit: ${green} assertions vertes`)
}
