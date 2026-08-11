/**
 * anchor-position.ts — absolute position of an anchored (floating) object.
 *
 * Pure functions, no DOM, no module state. Everything is expressed in document
 * pixels at 96 dpi (zoom is a paint-time concern, never a layout one), in the
 * PAGE coordinate system: (0, 0) is the top-left corner of the page sheet.
 *
 * The model is a direct transcription of LibreOffice Writer's object
 * positioning, found in `sw/source/core/objectpositioning/`:
 *
 *   - anchoredobjectposition.cxx      SwAnchoredObjectPosition — alignment areas
 *                                     (GetHoriAlignmentValues / GetVertAlignmentValues),
 *                                     CalcRelPosX, ImplAdjustHoriRelPos,
 *                                     ImplAdjustVertRelPos, ToggleHoriOrientAndAlign
 *   - tocntntanchoredobjectposition.cxx  at-paragraph / at-character anchoring
 *   - tolayoutanchoredobjectposition.cxx at-page / at-frame anchoring
 *   - environmentofanchoredobject.cxx    which frame confines the object
 *   - ../layout/anchoreddrawobject.cxx   formatting order (MakeObjPos)
 *
 * Order of operations, as in Writer: the VERTICAL position is resolved first,
 * then the HORIZONTAL one (tocntntanchoredobjectposition.cxx:279 then :1094);
 * the horizontal pass receives the already-resolved vertical position because
 * "draw aside" needs it (anchoredobjectposition.cxx:1127-1130 — not modelled
 * here, it needs the list of the other objects on the page).
 *
 * Out of scope on purpose (they need more than one object or a full layout):
 *   - AdjustHoriRelPosForDrawAside (anchoredobjectposition.cxx:995) — pushing an
 *     object sideways when another one already occupies the same slot;
 *   - CalcOverlap (tocntntanchoredobjectposition.cxx:1188) — `allowOverlap`;
 *   - frame growing in web/browse mode and inside table cells
 *     (tocntntanchoredobjectposition.cxx:95-137 lcl_DoesVertPosFits).
 */

// ── Public model ──────────────────────────────────────────────────────────────

/** Horizontal reference frame. Word `wp:positionH/@relativeFrom`. */
export type RelH = 'column' | 'margin' | 'page' | 'character'
/** Vertical reference frame. Word `wp:positionV/@relativeFrom`. */
export type RelV = 'paragraph' | 'margin' | 'page' | 'line'
/** Horizontal alignment inside the reference frame. Word `wp:align`. */
export type AlignH = 'left' | 'center' | 'right' | 'inside' | 'outside'
/** Vertical alignment inside the reference frame. Word `wp:align`. */
export type AlignV = 'top' | 'center' | 'bottom' | 'inside' | 'outside'

/** Geometry of the page the object is laid out on. All values in document px. */
export interface PageGeom {
  pageW: number
  pageH: number
  marginL: number
  marginR: number
  marginT: number
  marginB: number
  /** Width of the printable area: pageW - marginL - marginR. */
  contentW: number
  /** Height of the printable area: pageH - marginT - marginB. */
  contentH: number
}

/**
 * Everything the anchor paragraph contributes, in PAGE coordinates.
 *
 * `paraHeight`, `lineHeight` and `charX` are optional: when omitted they
 * degenerate to exactly what Writer computes when the corresponding rectangle
 * has no extent (the TEXT_LINE alignment area has height 0 by construction —
 * anchoredobjectposition.cxx:332-337 — and the CHAR alignment area has width 0
 * — anchoredobjectposition.cxx:745-756).
 */
export interface AnchorCtx {
  /** Top of the anchor paragraph frame (== `GetTopForObjPos`, ...:176). */
  paraTop: number
  /** Left of the anchor paragraph frame. */
  paraLeft: number
  /** Width of the anchor paragraph frame (the column width, in practice). */
  paraWidth: number
  /** Height of the anchor paragraph frame. Default 0 (= Writer's degenerate case). */
  paraHeight?: number
  /** Top of the line holding the anchor character. Default: `paraTop`. */
  lineTop?: number
  /**
   * Height of that line. Default 0, which reproduces LibreOffice's zero-height
   * TEXT_LINE reference; pass the real line height to get Word's true
   * "bottom of the line" anchoring.
   */
  lineHeight?: number
  /** X of the anchor character. Default: `paraLeft`. */
  charX?: number
}

/** The object's own positioning attributes plus its size. */
export interface ObjPos {
  relH: RelH
  relV: RelV
  /** Manual horizontal offset, used when `alignH` is undefined. */
  offsetX: number
  /** Manual vertical offset, used when `alignV` is undefined. */
  offsetY: number
  alignH?: AlignH
  alignV?: AlignV
  w: number
  h: number
}

export interface ResolveOpts {
  /**
   * The object sits on a left-hand (even) page of a mirrored spread.
   * Writer: `bEvenPage = !rPageAlignLayFrame.OnRightPage()`
   * (anchoredobjectposition.cxx:889).
   */
  evenPage?: boolean
  /**
   * Apply the confinement to the printable/page area. Default true.
   * Writer skips it only through `IsDraggingOffPageAllowed`
   * (anchoredobjectposition.cxx:457-461 and :621-622).
   */
  keepInsidePage?: boolean
  /**
   * The anchor follows the text flow (our `moveWithText`; Writer's
   * `SwFormatFollowTextFlow`, imported from OOXML `layoutInCell` —
   * DomainMapper_Impl.cxx:9748). Default true. See `resolveObjectPosition`.
   */
  followTextFlow?: boolean
  /** wrap === 'through' (`WrapTextMode_THROUGH`). */
  wrapThrough?: boolean
  /** wrap === 'topBottom' (`WrapTextMode_NONE`, Writer's `bNoSurround`). */
  wrapNone?: boolean
  /**
   * Word 2013+ layout: an object that is neither page- nor margin-relative and
   * does not wrap through is confined to the TEXT BODY, not to the whole sheet
   * (anchoredobjectposition.cxx:492-514). Default true — that is what Word does.
   */
  compat15?: boolean
  /**
   * Mirror a MANUAL offset on even pages. This is Writer's `IsPosToggle()`,
   * set by OOXML `relativeFrom="insideMargin"` (GraphicHelpers.cxx:109-112) and
   * consumed at anchoredobjectposition.cxx:926-931. Default false.
   */
  mirrorOffsetOnEvenPage?: boolean
  /**
   * Let an object that overflows the bottom of the text area move to the next
   * page (Writer's follow-text-flow loop, tocntntanchoredobjectposition.cxx:978-1083).
   * Default true.
   */
  allowPageMove?: boolean
}

export interface ResolvedPos {
  /** Absolute X in the page frame. */
  x: number
  /** Absolute Y in the page frame — of the page `pageAdvance` pages further. */
  y: number
  /**
   * 0 = same page, 1 = Writer moved the object to the NEXT page (it never clips
   * and never truncates an anchored object). `x`/`y` are then expressed in that
   * next page's coordinate system.
   */
  pageAdvance: number
  /** The horizontal confinement actually moved the object. */
  clampedX: boolean
  /** The vertical confinement actually moved the object. */
  clampedY: boolean
}

/** An alignment area: an origin plus an extent, as Writer's nOffset/nWidth pair. */
export interface AlignArea {
  /** Absolute origin of the area, in page coordinates. */
  pos: number
  /** Extent of the area (0 for the degenerate CHAR / TEXT_LINE references). */
  size: number
}

// ── Alignment areas ───────────────────────────────────────────────────────────

/**
 * Horizontal alignment area.
 *
 * Writer computes a (width, offset-from-the-anchor-frame) pair in
 * `GetHoriAlignmentValues` (anchoredobjectposition.cxx:666-816) and later turns
 * it into an absolute X by adding the anchor frame's left
 * (tocntntanchoredobjectposition.cxx:1179). We short-circuit that and return
 * the absolute origin straight away.
 *
 *  - `column`    → RelOrientation::FRAME, the anchor text frame itself
 *                  (GraphicHelpers.cxx:114-116; default branch ...:797-811,
 *                  nWidth = width of the orient frame, nOffset = 0)
 *  - `margin`    → RelOrientation::PAGE_PRINT_AREA (GraphicHelpers.cxx:101-103;
 *                  ...:759-787, nWidth = page print area width, nOffset = its left)
 *  - `page`      → RelOrientation::PAGE_FRAME (GraphicHelpers.cxx:105-107;
 *                  ...:788-796, nWidth = page frame width, nOffset = its left)
 *  - `character` → RelOrientation::CHAR (GraphicHelpers.cxx:118-120;
 *                  ...:745-756, nWidth = 0, nOffset = left of the char rect)
 */
export function horizontalArea(relH: RelH, page: PageGeom, anchor: AnchorCtx): AlignArea {
  switch (relH) {
    case 'column':    return { pos: anchor.paraLeft, size: anchor.paraWidth }
    case 'margin':    return { pos: page.marginL, size: page.contentW }
    case 'page':      return { pos: 0, size: page.pageW }
    case 'character': return { pos: anchor.charX ?? anchor.paraLeft, size: 0 }
  }
}

/**
 * Vertical alignment area — `GetVertAlignmentValues`
 * (anchoredobjectposition.cxx:211-368).
 *
 *  - `paragraph` → RelOrientation::FRAME (GraphicHelpers.cxx:82-84;
 *                  ...:232-238, nHeight = anchor frame height, nOffset = 0)
 *  - `margin`    → RelOrientation::PAGE_PRINT_AREA (GraphicHelpers.cxx:66-68;
 *                  ...:277-304, nHeight = page print area height, nOffset = its top)
 *  - `page`      → RelOrientation::PAGE_FRAME (GraphicHelpers.cxx:70-72;
 *                  ...:268-275, nHeight = page frame height, nOffset = its top)
 *  - `line`      → RelOrientation::TEXT_LINE (GraphicHelpers.cxx:86-88;
 *                  ...:332-343, nHeight = 0, nOffset = top of the line)
 */
export function verticalArea(relV: RelV, page: PageGeom, anchor: AnchorCtx): AlignArea {
  switch (relV) {
    case 'paragraph': return { pos: anchor.paraTop, size: anchor.paraHeight ?? 0 }
    case 'margin':    return { pos: page.marginT, size: page.contentH }
    case 'page':      return { pos: 0, size: page.pageH }
    case 'line':      return { pos: anchor.lineTop ?? anchor.paraTop, size: anchor.lineHeight ?? 0 }
  }
}

// ── Mirrored (facing) pages ───────────────────────────────────────────────────

/**
 * Resolve `inside`/`outside` into `left`/`right` for the current page parity.
 *
 * Writer has no INSIDE/OUTSIDE at layout time: the OOXML importer rewrites
 * `inside` → LEFT + PosToggle and `outside` → RIGHT + PosToggle
 * (GraphicImport.cxx:504-529), and the layout then swaps LEFT ⇄ RIGHT on even
 * pages in `ToggleHoriOrientAndAlign` (anchoredobjectposition.cxx:819-871),
 * gated by `bEvenPage` (anchoredobjectposition.cxx:889-890).
 */
export function resolveHoriAlign(alignH: AlignH, evenPage: boolean): 'left' | 'center' | 'right' {
  if (alignH === 'center') return 'center'
  // inside → left, outside → right (GraphicImport.cxx:509 and :525)
  let base: 'left' | 'right' =
    alignH === 'inside' ? 'left' : alignH === 'outside' ? 'right' : alignH
  const toggles = alignH === 'inside' || alignH === 'outside'
  // ToggleHoriOrientAndAlign, anchoredobjectposition.cxx:829-843
  if (toggles && evenPage) base = base === 'left' ? 'right' : 'left'
  return base
}

/**
 * Resolve `inside`/`outside` vertically.
 *
 * There is NO page mirroring in the vertical direction: `SwFormatVertOrient`
 * simply has no `m_bPosToggle` member (fmtornt.hxx:57-72 vs :82/:110 for the
 * horizontal one). The OOXML importer maps `inside` → TOP and `outside` →
 * BOTTOM once and for all (GraphicHelpers.cxx:183-186).
 */
export function resolveVertAlign(alignV: AlignV): 'top' | 'center' | 'bottom' {
  if (alignV === 'inside') return 'top'
  if (alignV === 'outside') return 'bottom'
  return alignV
}

// ── Main entry point ──────────────────────────────────────────────────────────

/**
 * Absolute position (in the PAGE frame) of an anchored object.
 *
 * What `followTextFlow` (our `moveWithText`) changes — Writer builds a
 * `SwEnvironmentOfAnchoredObject(DoesObjFollowsTextFlow())` at
 * tocntntanchoredobjectposition.cxx:171 and that single flag drives four things:
 *
 *  1. the confining frame: false → the PAGE frame, true → the innermost
 *     cell/fly/header/footer/body frame (environmentofanchoredobject.cxx:39-44
 *     vertically :69-74, the loops at :47-55 and :77-89). Note the horizontal
 *     list (:47-49) only knows Cell|Fly|Page — never Header/Footer/Body — so for
 *     a body paragraph the HORIZONTAL confinement is the page either way;
 *  2. the bottom clamp: `bCheckBottom = !DoesObjFollowsTextFlow()`
 *     (tocntntanchoredobjectposition.cxx:457, :678, :718) — an object that
 *     follows the text flow is NOT pushed back up, it is allowed to overflow…
 *  3. …because instead it may MOVE TO THE NEXT PAGE (the loop at
 *     tocntntanchoredobjectposition.cxx:978-1083, guarded at :905-907 by
 *     `followTextFlow && relV ∉ {page, margin}`);
 *  4. the frame whose left/width serves as the `column` reference, which becomes
 *     the "virtual" anchor in the new environment
 *     (tocntntanchoredobjectposition.cxx:1121-1123, GetHoriVirtualAnchor :1342).
 *     Point 4 is the caller's job: after a page move, re-supply an `AnchorCtx`
 *     that describes the paragraph fragment on the new page.
 *
 * What happens when the object overflows the BOTTOM of the page: Writer never
 * clips and never truncates. Either it moves the object to the next page (case
 * 3 above; the new Y is the print-area top of that page,
 * tocntntanchoredobjectposition.cxx:1025-1031), or — when the object does not
 * follow the text flow — it clamps it back inside the area
 * (anchoredobjectposition.cxx:594-601). An object TALLER than the area is left
 * flush with the area's top and simply spills past the bottom, because the top
 * clamp is applied last and unconditionally (anchoredobjectposition.cxx:602-605),
 * and because the page-move loop bails out as soon as the object already sits at
 * the top of the text area (tocntntanchoredobjectposition.cxx:1000-1004).
 */
export function resolveObjectPosition(
  pos: ObjPos,
  page: PageGeom,
  anchor: AnchorCtx,
  opts: ResolveOpts = {},
): ResolvedPos {
  const evenPage = opts.evenPage === true
  const keepInside = opts.keepInsidePage !== false
  const followTextFlow = opts.followTextFlow !== false
  const compat15 = opts.compat15 !== false
  const allowPageMove = opts.allowPageMove !== false

  // ── 1. vertical position (Writer resolves it first) ────────────────────────
  const vArea = verticalArea(pos.relV, page, anchor)
  let y: number
  if (pos.alignV == null) {
    // VertOrientation::NONE — "manual" position: area origin + attribute value
    // (tocntntanchoredobjectposition.cxx:629, absolute at :1181).
    y = vArea.pos + pos.offsetY
  } else {
    const a = resolveVertAlign(pos.alignV)
    if (a === 'center') {
      // anchoredobjectposition.cxx:409-412 / tocntnt...:365-371
      y = vArea.pos + vArea.size / 2 - pos.h / 2
    } else if (a === 'bottom') {
      if (opts.wrapNone === true && (pos.relV === 'paragraph' || pos.relV === 'margin')) {
        // bNoSurround + FRAME|PRINT_AREA: the object goes BELOW the area
        // instead of flush with its bottom (tocntntanchoredobjectposition.cxx:389-395).
        y = vArea.pos + vArea.size
      } else {
        // anchoredobjectposition.cxx:414-422 / tocntnt...:407-411
        y = vArea.pos + vArea.size - pos.h
      }
    } else {
      // VertOrientation::TOP — anchoredobjectposition.cxx:401-407.
      // With a zero-height TEXT_LINE area this yields "top of the object on the
      // top of the line", which is what Word's align="top" relativeFrom="line"
      // means once the importer swap (GraphicHelpers.cxx:193-200) is undone.
      y = vArea.pos
    }
  }

  // ── 2. vertical confinement — ImplAdjustVertRelPos, anchoredobjectposition.cxx:445 ──
  let clampedY = false
  const offPageAllowed = opts.wrapThrough === true && !keepInside
  if (keepInside && !offPageAllowed) {
    // Confining area (anchoredobjectposition.cxx:472-531). With
    // CONSIDER_WRAP_ON_OBJECT_POSITION — always on for Word documents — the area
    // is the whole page frame (:489-490); Word-2013+ compatibility then narrows
    // it to the body frame for objects that are neither page- nor margin-relative
    // and do not wrap through (:493-514). That narrowing is guarded by
    // `rPageAlignLayFrame.IsPageFrame()`, i.e. it only applies when the object
    // does NOT follow the text flow (environmentofanchoredobject.cxx:69-74).
    const narrowToBody =
      compat15 &&
      !followTextFlow &&
      opts.wrapThrough !== true &&
      pos.relV !== 'page' &&
      pos.relV !== 'margin'
    const areaTop = narrowToBody ? page.marginT : 0
    const areaBottom = narrowToBody ? page.marginT + page.contentH : page.pageH
    // bCheckBottom = !DoesObjFollowsTextFlow() (tocntnt...:457, :678, :718)
    const checkBottom = !followTextFlow
    if (checkBottom && y + pos.h > areaBottom) {
      y = areaBottom - pos.h                       // ...:594-601
      clampedY = true
    }
    if (y < areaTop) {
      y = areaTop                                  // ...:602-605 — applied LAST,
      clampedY = true                              // so it wins for oversized objects
    }
  }

  // ── 3. page move (follow-text-flow only) ──────────────────────────────────
  // tocntntanchoredobjectposition.cxx:905-907 gates it, :978-1083 performs it.
  let pageAdvance = 0
  const bodyTop = page.marginT
  const bodyBottom = page.marginT + page.contentH
  if (
    allowPageMove &&
    followTextFlow &&
    pos.relV !== 'page' &&
    pos.relV !== 'margin' &&
    y + pos.h > bodyBottom &&
    // "It doesn't fit, moving it would not help either anymore" (:1000-1004)
    y > bodyTop
  ) {
    pageAdvance = 1
    y = bodyTop                                    // PrtTop of the next leaf (:1026)
  }

  // ── 4. horizontal position — CalcRelPosX, anchoredobjectposition.cxx:874 ───
  const hArea = horizontalArea(pos.relH, page, anchor)
  let x: number
  if (pos.alignH == null) {
    // HoriOrientation::NONE (anchoredobjectposition.cxx:912-936)
    if (opts.mirrorOffsetOnEvenPage === true && evenPage) {
      // ...:926-931 — mirrored manual offset on left-hand pages
      x = hArea.pos + hArea.size - pos.w - pos.offsetX
    } else {
      x = hArea.pos + pos.offsetX                  // ...:934
    }
  } else {
    const a = resolveHoriAlign(pos.alignH, evenPage)
    if (a === 'center') x = hArea.pos + hArea.size / 2 - pos.w / 2   // ...:937-938
    else if (a === 'right') x = hArea.pos + hArea.size - pos.w       // ...:939-943
    else x = hArea.pos                                               // ...:945
  }

  // ── 5. horizontal confinement — ImplAdjustHoriRelPos, ...:615-663 ─────────
  let clampedX = false
  if (keepInside && !offPageAllowed) {
    // The horizontal environment is the PAGE frame: the loop at
    // environmentofanchoredobject.cxx:47-49 stops on Cell|Fly|Page only.
    if (x + pos.w > page.pageW) {
      x = page.pageW - pos.w                       // ...:647-653
      clampedX = true
    }
    if (x < 0) {
      x = 0                                        // ...:654-659 — applied LAST
      clampedX = true
    }
  }

  return { x, y, pageAdvance, clampedX, clampedY }
}

// ── Bridge with the current attribute model ───────────────────────────────────

/**
 * Convert an absolute page position back into the legacy `wrapX`/`wrapY` pair
 * the renderer uses today (`wrapX` from the LEFT EDGE OF THE CONTENT AREA,
 * `wrapY` from the TOP OF THE ANCHOR PARAGRAPH — see canvas-engine.ts).
 */
export function toWrapOffsets(
  abs: { x: number; y: number },
  page: PageGeom,
  anchor: AnchorCtx,
): { wrapX: number; wrapY: number } {
  return { wrapX: abs.x - page.marginL, wrapY: abs.y - anchor.paraTop }
}

/** Inverse of `toWrapOffsets`. */
export function fromWrapOffsets(
  off: { wrapX: number; wrapY: number },
  page: PageGeom,
  anchor: AnchorCtx,
): { x: number; y: number } {
  return { x: off.wrapX + page.marginL, y: off.wrapY + anchor.paraTop }
}

/**
 * Position an object whose attributes are the legacy `wrapX`/`wrapY` only.
 * Equivalent to `relH: 'margin'`, `relV: 'paragraph'`, no alignment — which is
 * exactly what the renderer does today, minus the confinement.
 */
export function legacyObjPos(wrapX: number, wrapY: number, w: number, h: number): ObjPos {
  return { relH: 'margin', relV: 'paragraph', offsetX: wrapX, offsetY: wrapY, w, h }
}

/** Build a `PageGeom` from a sheet size and its four margins. */
export function makePageGeom(
  pageW: number, pageH: number,
  marginL: number, marginR: number, marginT: number, marginB: number,
): PageGeom {
  return {
    pageW, pageH, marginL, marginR, marginT, marginB,
    contentW: pageW - marginL - marginR,
    contentH: pageH - marginT - marginB,
  }
}

// ── Self test ─────────────────────────────────────────────────────────────────
// Run with:  node --experimental-strip-types src/documents/layout/anchor-position.ts

let _pass = 0
const _fails: string[] = []

function eq(actual: number, expected: number, what: string): void {
  if (Math.abs(actual - expected) < 1e-6) _pass++
  else _fails.push(`${what}: expected ${expected}, got ${actual}`)
}

function is(actual: unknown, expected: unknown, what: string): void {
  if (actual === expected) _pass++
  else _fails.push(`${what}: expected ${String(expected)}, got ${String(actual)}`)
}

export function selfTest(): boolean {
  // A4 at 96 dpi with 1 inch margins.
  const P = makePageGeom(794, 1123, 96, 96, 96, 96)          // contentW 602, contentH 931
  const A: AnchorCtx = {
    paraTop: 300, paraLeft: 96, paraWidth: 602, paraHeight: 60,
    lineTop: 320, lineHeight: 16, charX: 250,
  }
  // Neutral options: no confinement, no page move — we test the raw referentials.
  const RAW: ResolveOpts = { keepInsidePage: false, allowPageMove: false }
  const OBJ = (o: Partial<ObjPos>): ObjPos => ({
    relH: 'column', relV: 'paragraph', offsetX: 0, offsetY: 0, w: 200, h: 100, ...o,
  })
  const X = (o: Partial<ObjPos>, opt: ResolveOpts = RAW): number =>
    resolveObjectPosition(OBJ(o), P, A, { ...RAW, ...opt }).x
  const Y = (o: Partial<ObjPos>, opt: ResolveOpts = RAW): number =>
    resolveObjectPosition(OBJ(o), P, A, { ...RAW, ...opt }).y

  // ── 4 horizontal referentials × 5 alignments (20 assertions) ──────────────
  // column: area = [96, 602]
  eq(X({ relH: 'column', alignH: 'left' }), 96, 'H column/left')
  eq(X({ relH: 'column', alignH: 'center' }), 96 + 301 - 100, 'H column/center')
  eq(X({ relH: 'column', alignH: 'right' }), 96 + 602 - 200, 'H column/right')
  eq(X({ relH: 'column', alignH: 'inside' }), 96, 'H column/inside (odd page)')
  eq(X({ relH: 'column', alignH: 'outside' }), 498, 'H column/outside (odd page)')
  // margin: area = [96, 602] (same numbers here, different meaning)
  eq(X({ relH: 'margin', alignH: 'left' }), 96, 'H margin/left')
  eq(X({ relH: 'margin', alignH: 'center' }), 96 + 301 - 100, 'H margin/center')
  eq(X({ relH: 'margin', alignH: 'right' }), 498, 'H margin/right')
  eq(X({ relH: 'margin', alignH: 'inside' }), 96, 'H margin/inside')
  eq(X({ relH: 'margin', alignH: 'outside' }), 498, 'H margin/outside')
  // page: area = [0, 794]
  eq(X({ relH: 'page', alignH: 'left' }), 0, 'H page/left')
  eq(X({ relH: 'page', alignH: 'center' }), 397 - 100, 'H page/center')
  eq(X({ relH: 'page', alignH: 'right' }), 594, 'H page/right')
  eq(X({ relH: 'page', alignH: 'inside' }), 0, 'H page/inside')
  eq(X({ relH: 'page', alignH: 'outside' }), 594, 'H page/outside')
  // character: zero-width area at charX (anchoredobjectposition.cxx:745-756)
  eq(X({ relH: 'character', alignH: 'left' }), 250, 'H char/left')
  eq(X({ relH: 'character', alignH: 'center' }), 150, 'H char/center')
  eq(X({ relH: 'character', alignH: 'right' }), 50, 'H char/right')
  eq(X({ relH: 'character', alignH: 'inside' }), 250, 'H char/inside')
  eq(X({ relH: 'character', alignH: 'outside' }), 50, 'H char/outside')

  // ── the same 4 × manual offset (4 assertions) ─────────────────────────────
  eq(X({ relH: 'column', offsetX: 30 }), 126, 'H column/manual')
  eq(X({ relH: 'margin', offsetX: 30 }), 126, 'H margin/manual')
  eq(X({ relH: 'page', offsetX: 30 }), 30, 'H page/manual')
  eq(X({ relH: 'character', offsetX: 30 }), 280, 'H char/manual')

  // ── 4 vertical referentials × 5 alignments (20 assertions) ────────────────
  // paragraph: area = [300, 60]
  eq(Y({ relV: 'paragraph', alignV: 'top' }), 300, 'V para/top')
  eq(Y({ relV: 'paragraph', alignV: 'center' }), 300 + 30 - 50, 'V para/center')
  eq(Y({ relV: 'paragraph', alignV: 'bottom' }), 300 + 60 - 100, 'V para/bottom')
  eq(Y({ relV: 'paragraph', alignV: 'inside' }), 300, 'V para/inside → top')
  eq(Y({ relV: 'paragraph', alignV: 'outside' }), 260, 'V para/outside → bottom')
  // margin: area = [96, 931]
  eq(Y({ relV: 'margin', alignV: 'top' }), 96, 'V margin/top')
  eq(Y({ relV: 'margin', alignV: 'center' }), 96 + 465.5 - 50, 'V margin/center')
  eq(Y({ relV: 'margin', alignV: 'bottom' }), 96 + 931 - 100, 'V margin/bottom')
  eq(Y({ relV: 'margin', alignV: 'inside' }), 96, 'V margin/inside → top')
  eq(Y({ relV: 'margin', alignV: 'outside' }), 927, 'V margin/outside → bottom')
  // page: area = [0, 1123]
  eq(Y({ relV: 'page', alignV: 'top' }), 0, 'V page/top')
  eq(Y({ relV: 'page', alignV: 'center' }), 561.5 - 50, 'V page/center')
  eq(Y({ relV: 'page', alignV: 'bottom' }), 1023, 'V page/bottom')
  eq(Y({ relV: 'page', alignV: 'inside' }), 0, 'V page/inside → top')
  eq(Y({ relV: 'page', alignV: 'outside' }), 1023, 'V page/outside → bottom')
  // line: area = [320, 16]
  eq(Y({ relV: 'line', alignV: 'top' }), 320, 'V line/top')
  eq(Y({ relV: 'line', alignV: 'center' }), 320 + 8 - 50, 'V line/center')
  eq(Y({ relV: 'line', alignV: 'bottom' }), 320 + 16 - 100, 'V line/bottom')
  eq(Y({ relV: 'line', alignV: 'inside' }), 320, 'V line/inside → top')
  eq(Y({ relV: 'line', alignV: 'outside' }), 236, 'V line/outside → bottom')

  // ── the same 4 × manual offset (4 assertions) ─────────────────────────────
  eq(Y({ relV: 'paragraph', offsetY: 25 }), 325, 'V para/manual')
  eq(Y({ relV: 'margin', offsetY: 25 }), 121, 'V margin/manual')
  eq(Y({ relV: 'page', offsetY: 25 }), 25, 'V page/manual')
  eq(Y({ relV: 'line', offsetY: 25 }), 345, 'V line/manual')

  // ── zero-extent defaults (LibreOffice's degenerate references) ────────────
  const bare: AnchorCtx = { paraTop: 300, paraLeft: 96, paraWidth: 602 }
  // TEXT_LINE with no height: LO's nHeight = 0 (anchoredobjectposition.cxx:332-337).
  // "bottom" then lands the object entirely ABOVE the line top, which is exactly
  // LO's VertOrientation::TOP + TEXT_LINE branch (tocntnt...:340-344).
  eq(resolveObjectPosition(OBJ({ relV: 'line', alignV: 'bottom' }), P, bare, RAW).y,
     200, 'V line/bottom with no lineHeight → lineTop - h')
  eq(resolveObjectPosition(OBJ({ relV: 'line', alignV: 'top' }), P, bare, RAW).y,
     300, 'V line/top falls back on paraTop')
  // CHAR with no charX falls back on the paragraph left.
  eq(resolveObjectPosition(OBJ({ relH: 'character', alignH: 'left' }), P, bare, RAW).x,
     96, 'H char/left falls back on paraLeft')

  // ── even page (facing pages): inside/outside swap, others do not ──────────
  const EVEN: ResolveOpts = { ...RAW, evenPage: true }
  eq(X({ relH: 'page', alignH: 'inside' }, EVEN), 594, 'H page/inside on even page → right')
  eq(X({ relH: 'page', alignH: 'outside' }, EVEN), 0, 'H page/outside on even page → left')
  eq(X({ relH: 'margin', alignH: 'inside' }, EVEN), 498, 'H margin/inside on even page')
  eq(X({ relH: 'margin', alignH: 'outside' }, EVEN), 96, 'H margin/outside on even page')
  eq(X({ relH: 'page', alignH: 'left' }, EVEN), 0, 'H page/left is NOT mirrored')
  eq(X({ relH: 'page', alignH: 'right' }, EVEN), 594, 'H page/right is NOT mirrored')
  eq(X({ relH: 'page', alignH: 'center' }, EVEN), 297, 'H page/center is NOT mirrored')
  // Vertical inside/outside never mirrors (SwFormatVertOrient has no PosToggle).
  eq(Y({ relV: 'page', alignV: 'inside' }, EVEN), 0, 'V page/inside not mirrored')
  eq(Y({ relV: 'page', alignV: 'outside' }, EVEN), 1023, 'V page/outside not mirrored')
  // Manual offset mirroring, opt-in (anchoredobjectposition.cxx:926-931).
  eq(X({ relH: 'page', offsetX: 30 }, { ...EVEN, mirrorOffsetOnEvenPage: true }),
     794 - 200 - 30, 'H page/manual mirrored on even page')
  eq(X({ relH: 'page', offsetX: 30 }, { ...RAW, mirrorOffsetOnEvenPage: true }),
     30, 'H page/manual NOT mirrored on odd page')

  // ── wrapNone (SURROUND_NONE) + bottom relative to the paragraph ───────────
  eq(Y({ relV: 'paragraph', alignV: 'bottom' }, { ...RAW, wrapNone: true }),
     360, 'V para/bottom with wrapNone → below the paragraph')

  // ── horizontal confinement (ImplAdjustHoriRelPos) ─────────────────────────
  const KEEP: ResolveOpts = { keepInsidePage: true, allowPageMove: false }
  {
    const r = resolveObjectPosition(OBJ({ relH: 'page', offsetX: 700 }), P, A, KEEP)
    eq(r.x, 794 - 200, 'H clamp right')
    is(r.clampedX, true, 'H clamp right flagged')
  }
  {
    const r = resolveObjectPosition(OBJ({ relH: 'page', offsetX: -50 }), P, A, KEEP)
    eq(r.x, 0, 'H clamp left (negative offset)')
    is(r.clampedX, true, 'H clamp left flagged')
  }
  {
    // Wider than the page: the LEFT clamp is applied last, so the object is
    // flush left and overflows on the right (anchoredobjectposition.cxx:654-659).
    const r = resolveObjectPosition(OBJ({ relH: 'page', offsetX: 100, w: 1000 }), P, A, KEEP)
    eq(r.x, 0, 'H object wider than the page → flush left')
  }
  {
    const r = resolveObjectPosition(OBJ({ relH: 'page', offsetX: 300 }), P, A, KEEP)
    eq(r.x, 300, 'H no clamp when it fits')
    is(r.clampedX, false, 'H no clamp flagged')
  }

  // ── vertical confinement, object NOT following the text flow ──────────────
  const FIXED: ResolveOpts = { keepInsidePage: true, followTextFlow: false, allowPageMove: false }
  {
    // relV = page → not narrowed to the body: clamped to the sheet.
    const r = resolveObjectPosition(OBJ({ relV: 'page', offsetY: 1100 }), P, A, FIXED)
    eq(r.y, 1123 - 100, 'V clamp bottom on the sheet (relV=page)')
    is(r.clampedY, true, 'V clamp bottom flagged')
  }
  {
    const r = resolveObjectPosition(OBJ({ relV: 'page', offsetY: -40 }), P, A, FIXED)
    eq(r.y, 0, 'V clamp top on the sheet (negative offset)')
  }
  {
    // relV = paragraph + compat15 + not wrap-through → confined to the TEXT BODY
    // (anchoredobjectposition.cxx:493-514).
    const r = resolveObjectPosition(OBJ({ relV: 'paragraph', offsetY: 900 }), P, A, FIXED)
    eq(r.y, 96 + 931 - 100, 'V clamp bottom on the body (Word 2013+)')
  }
  {
    const r = resolveObjectPosition(OBJ({ relV: 'paragraph', offsetY: -400 }), P, A, FIXED)
    eq(r.y, 96, 'V clamp top on the body (Word 2013+)')
  }
  {
    // Same, wrap-through → back to the whole sheet (:503, !bWrapThrough).
    const r = resolveObjectPosition(
      OBJ({ relV: 'paragraph', offsetY: -400 }), P, A, { ...FIXED, wrapThrough: true })
    eq(r.y, 0, 'V wrap-through is clamped to the sheet, not to the body')
  }
  {
    // compat15 off → LibreOffice's own behaviour: the whole sheet.
    const r = resolveObjectPosition(
      OBJ({ relV: 'paragraph', offsetY: -400 }), P, A, { ...FIXED, compat15: false })
    eq(r.y, 0, 'V compat15 off → clamped to the sheet')
  }
  {
    // Taller than the area: the top clamp wins (:602-605 is applied last).
    const r = resolveObjectPosition(OBJ({ relV: 'page', offsetY: 200, h: 2000 }), P, A, FIXED)
    eq(r.y, 0, 'V object taller than the page → flush top')
  }

  // ── vertical confinement, object FOLLOWING the text flow ──────────────────
  const FLOW: ResolveOpts = { keepInsidePage: true, followTextFlow: true, allowPageMove: false }
  {
    // bCheckBottom is false → NO bottom clamp (tocntnt...:457).
    const r = resolveObjectPosition(OBJ({ relV: 'paragraph', offsetY: 900 }), P, A, FLOW)
    eq(r.y, 1200, 'V follow-text-flow → the bottom clamp does not apply')
    is(r.clampedY, false, 'V follow-text-flow bottom not flagged')
  }
  {
    // …but the top clamp still does, on the whole sheet.
    const r = resolveObjectPosition(OBJ({ relV: 'paragraph', offsetY: -400 }), P, A, FLOW)
    eq(r.y, 0, 'V follow-text-flow → the top clamp still applies (sheet)')
  }

  // ── overflow at the bottom: page move, never clipping ─────────────────────
  {
    const r = resolveObjectPosition(
      OBJ({ relV: 'paragraph', offsetY: 900 }), P, A,
      { keepInsidePage: true, followTextFlow: true })
    is(r.pageAdvance, 1, 'overflow → moved to the next page')
    eq(r.y, 96, 'moved object sits at the top of the next text area')
  }
  {
    // relV = page or margin → never moves (tocntnt...:905-907).
    const r = resolveObjectPosition(
      OBJ({ relV: 'page', offsetY: 1100 }), P, A,
      { keepInsidePage: true, followTextFlow: true })
    is(r.pageAdvance, 0, 'page-relative object never moves page')
  }
  {
    // Does not follow the text flow → clamped, not moved.
    const r = resolveObjectPosition(
      OBJ({ relV: 'paragraph', offsetY: 900 }), P, A,
      { keepInsidePage: true, followTextFlow: false })
    is(r.pageAdvance, 0, 'fixed object is clamped, not moved')
    eq(r.y, 927, 'fixed object clamped to the body bottom')
  }
  {
    // Already at the top of the text area and still too tall: no move
    // (tocntnt...:1000-1004).
    const r = resolveObjectPosition(
      OBJ({ relV: 'margin', alignV: 'top', h: 2000 }), P, A,
      { keepInsidePage: true, followTextFlow: true, allowPageMove: true })
    is(r.pageAdvance, 0, 'object taller than the text area does not move page')
  }

  // ── legacy bridge ─────────────────────────────────────────────────────────
  {
    const r = resolveObjectPosition(legacyObjPos(40, 12, 200, 100), P, A, RAW)
    eq(r.x, 136, 'legacy wrapX is relative to the content left')
    eq(r.y, 312, 'legacy wrapY is relative to the paragraph top')
    const back = toWrapOffsets(r, P, A)
    eq(back.wrapX, 40, 'toWrapOffsets round-trip X')
    eq(back.wrapY, 12, 'toWrapOffsets round-trip Y')
    const fwd = fromWrapOffsets(back, P, A)
    eq(fwd.x, r.x, 'fromWrapOffsets round-trip X')
    eq(fwd.y, r.y, 'fromWrapOffsets round-trip Y')
  }
  {
    const g = makePageGeom(794, 1123, 96, 96, 96, 96)
    eq(g.contentW, 602, 'makePageGeom contentW')
    eq(g.contentH, 931, 'makePageGeom contentH')
  }

  // ── area helpers ──────────────────────────────────────────────────────────
  eq(horizontalArea('page', P, A).size, 794, 'horizontalArea page size')
  eq(verticalArea('margin', P, A).pos, 96, 'verticalArea margin pos')
  is(resolveHoriAlign('inside', false), 'left', 'resolveHoriAlign inside odd')
  is(resolveHoriAlign('inside', true), 'right', 'resolveHoriAlign inside even')
  is(resolveVertAlign('outside'), 'bottom', 'resolveVertAlign outside')

  const ok = _fails.length === 0
  if (!ok) for (const f of _fails) console.error('FAIL  ' + f)
  console.log(`${_pass} assertions vertes` + (ok ? '' : `, ${_fails.length} ÉCHECS`))
  return ok
}

const _argv = (globalThis as { process?: { argv?: string[] } }).process?.argv
if (_argv?.[1]?.endsWith('anchor-position.ts')) selfTest()
