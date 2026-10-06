/**
 * wrap-bands — exclusion bands and line segments for text wrapping around
 * floating objects, modelled on LibreOffice Writer's `SwTextFly`.
 *
 * All functions here are PURE: no DOM, no module state, no side effects.
 * Coordinates are document pixels at 96 dpi (zoom is a paint-time concern),
 * with x = 0 at the LEFT edge of the content area and x = contentW at its right
 * edge — exactly the convention used by `canvas-engine.ts`.
 *
 * LibreOffice reference (read, not guessed):
 *   sw/source/core/text/txtfly.cxx      — SwTextFly
 *   sw/source/core/inc/txtfly.hxx       — SwTextFly / SwContourCache declarations
 *   sw/source/core/text/itrform2.cxx    — SwTextFormatter::CalcFlyWidth
 *
 * Model, in Writer terms:
 *   - Every anchored object is turned into an "object rect with spaces"
 *     (`SwAnchoredObject::GetObjRectWithSpaces()`), i.e. its box enlarged by its
 *     own L/R/T/B distances.  Sorting, margin computation and the "ideal wrap"
 *     decision all use THAT rect — cf. txtfly.cxx:80-81 (AnchoredObjOrder),
 *     txtfly.cxx:1159, txtfly.cxx:1508.
 *   - Objects are sorted by left edge asc, then top asc, then right desc
 *     (txtfly.cxx:66-119, `AnchoredObjOrder::operator()`).
 *   - Each object yields ONE horizontal exclusion interval per line band; the
 *     union of those intervals is subtracted from [0, contentW] and the
 *     remaining segments are the text segments of the line.
 *   - Writer never computes the union explicitly: it returns the leftmost fly
 *     rect (`SwTextFly::GetFrame_`, txtfly.cxx:380) and the formatter loops.
 *     The union is equivalent BECAUSE Writer clamps each object's exclusion to
 *     its neighbours (`CalcLeftMargin`/`CalcRightMargin`) — without that
 *     clamping a "left-only" object plus a "right-only" object would swallow
 *     the whole line, which is NOT what Writer does.  See `calcRightMargin` /
 *     `calcLeftMargin` below.
 */

// ── Units ────────────────────────────────────────────────────────────────────

/** 1 twip = 1/1440 inch; document pixels are 96 dpi → 1 twip = 96/1440 px. */
const TWIP_PX = 96 / 1440

/**
 * "Wrap only on sides with at least 2cm space for the text" — `#define TEXT_MIN 1134`
 * (txtfly.cxx:1466-1467).  Used by the "ideal / largest side" decision only.
 */
export const TEXT_MIN_PX = 1134 * TWIP_PX // = 75.6 px

/**
 * "MS Word wraps on sides with even less space (value guessed)" —
 * `#define TEXT_MIN_SMALL 300` (txtfly.hxx:89-90).  Writer uses it instead of
 * TEXT_MIN when the document has the `SURROUND_TEXT_WRAP_SMALL` compat flag
 * (txtfly.cxx:1525-1527), and as the "no usable space left on this line" limit
 * in `CalcFlyWidth` (itrform2.cxx:3071-3086).  This is our default minimum
 * usable segment width.
 */
export const TEXT_MIN_SMALL_PX = 300 * TWIP_PX // = 20 px

/**
 * "Wrap on both sides up to a frame width of 1.5cm" — `#define FRAME_MAX 850`
 * (txtfly.cxx:1469-1470).  Objects NARROWER than this keep both sides usable in
 * "ideal" mode; wider ones only keep the wider side.
 */
export const FRAME_MAX_PX = 850 * TWIP_PX // ≈ 56.67 px

/**
 * `#define MINLAY 23` — "Minimal size for other Frames" (sw/inc/swtypes.hxx:60).
 * Writer's non-Word-compatible floor for "the line is effectively full"
 * (itrform2.cxx:3077).  Exported for callers that want Writer's own (very
 * permissive) behaviour instead of the Word-like default.
 */
export const MINLAY_PX = 23 * TWIP_PX // ≈ 1.53 px

/** Numeric slack, in px, used when merging/comparing intervals. */
const EPS = 1e-6

// ── Public types ─────────────────────────────────────────────────────────────

/** Horizontal span [x0, x1). Used both for exclusions and for text segments. */
export interface Segment { x0: number; x1: number }

/**
 * A floating object competing with the text for horizontal room.
 *
 * `x`/`y`/`w`/`h` describe the object's own box (already rotated to its AABB by
 * the caller if needed); `dist*` are its text distances.  `wrap` and `side` use
 * our document model's vocabulary (see CONTRAT_OBJETS / canvas-engine):
 *
 *   wrap: inline | square | tight | through | topBottom | behind | front
 *   side: both | left | right | largest
 */
export interface FloatBox {
  x: number
  y: number
  w: number
  h: number
  wrap: string
  side: string
  distL: number
  distR: number
  distT: number
  distB: number
  /**
   * Optional contour spans for `tight`/`through` wrapping: the horizontal runs
   * covered by the object ON THIS LINE, in document px, WITHOUT the L/R
   * distances (they are added here).  Writer computes the equivalent in
   * `SwContourCache::CalcBoundRect` (txtfly.cxx:158-308) and, for genuine
   * contour wrapping, emits one fly portion per run (txtfly.cxx:1212-1216).
   * When absent the object's plain box is used.
   */
  runs?: Segment[]
}

/**
 * Writer's `css::text::WrapTextMode`, mapped to our names:
 *   parallel = WrapTextMode_PARALLEL (text on both sides)
 *   left     = WrapTextMode_LEFT     (text flows on the LEFT of the object)
 *   right    = WrapTextMode_RIGHT    (text flows on the RIGHT of the object)
 *   none     = WrapTextMode_NONE     (no text beside the object: top/bottom)
 *   through  = WrapTextMode_THROUGH  (object ignored by the formatter)
 */
export type Surround = 'parallel' | 'left' | 'right' | 'none' | 'through'

export interface WrapOpts {
  /**
   * Segments narrower than this are dropped (px).  Default `TEXT_MIN_SMALL_PX`
   * (20 px) — Writer's Word-compatible floor, itrform2.cxx:3081.  Pass
   * `MINLAY_PX` for Writer's own legacy behaviour.
   */
  minTextWidth?: number
  /**
   * Threshold for the "ideal wrap / largest side" decision (px).  Default
   * `TEXT_MIN_PX` (2 cm, txtfly.cxx:1467); Writer switches it to
   * `TEXT_MIN_SMALL_PX` when `DocumentSettingId::SURROUND_TEXT_WRAP_SMALL` is
   * on (txtfly.cxx:1525-1527) — i.e. for documents imported from Word.
   */
  idealMinTextWidth?: number
}

// ── Internals ────────────────────────────────────────────────────────────────

/** An object that actually intersects the current line band. */
interface Active {
  /** Outer left of the object rect WITH spaces (`GetObjRectWithSpaces`). */
  x0: number
  /** Outer right of the object rect WITH spaces. */
  x1: number
  /** Top/right used only for the sort order (txtfly.cxx:89-98). */
  top: number
  /** One padded span per contour run; a single span for plain boxes. */
  spans: Segment[]
  sur: Surround
}

/** Wrap modes that make the object invisible to the formatter (SURROUND_THROUGH). */
const THROUGH_WRAPS = new Set(['behind', 'front'])
/** Wrap mode that forbids text beside the object (SURROUND_NONE). */
const NONE_WRAPS = new Set(['topBottom'])
/** Wrap modes that push text aside according to the chosen side. */
const SIDE_WRAPS = new Set(['square', 'tight', 'through'])

/** Object rect with spaces, horizontally (`SwAnchoredObject::GetObjRectWithSpaces`). */
function outerX(f: FloatBox): Segment {
  return { x0: f.x - f.distL, x1: f.x + f.w + f.distR }
}

/** Object rect with spaces, vertically. */
function outerY(f: FloatBox): Segment {
  return { x0: f.y - f.distT, x1: f.y + f.h + f.distB }
}

/**
 * `SwTextFly::GetSurroundForTextWrap` (txtfly.cxx:1472-1547).
 *
 * Resolves our `wrap`/`side` pair into Writer's WrapTextMode, including the
 * "ideal page wrap" (`WrapTextMode_DYNAMIC`, our `side: 'largest'`) heuristic:
 *
 *   - object entirely outside the column → PARALLEL (txtfly.cxx:1512-1513)
 *   - object wider than FRAME_MAX → only the wider side survives; on a TIE the
 *     `else` branch wins, killing the RIGHT space, so text flows on the LEFT
 *     (txtfly.cxx:1518-1524)
 *   - no space at all on either side → PARALLEL rather than NONE
 *     (txtfly.cxx:1529-1533)
 *   - a side with less than TEXT_MIN is dropped (txtfly.cxx:1535-1538)
 *   - then: both → PARALLEL, left only → LEFT, right only → RIGHT, none → NONE
 *     (txtfly.cxx:1539-1542)
 */
export function resolveSurround(f: FloatBox, contentW: number, opts?: WrapOpts): Surround {
  if (THROUGH_WRAPS.has(f.wrap)) return 'through'
  if (NONE_WRAPS.has(f.wrap)) return 'none'
  // `inline` (and anything unknown) is not a floating object: it takes part in
  // the text flow, so it never produces an exclusion band.
  if (!SIDE_WRAPS.has(f.wrap)) return 'through'

  if (f.side === 'left') return 'left'
  if (f.side === 'right') return 'right'
  if (f.side !== 'largest') return 'parallel'

  // ── WrapTextMode_DYNAMIC, "ideal page wrap" ────────────────────────────────
  const { x0: flyLeft, x1: flyRight } = outerX(f)
  const currLeft = 0
  const currRight = contentW
  if (flyRight < currLeft || flyLeft > currRight) return 'parallel' // txtfly.cxx:1512

  let nLeft = flyLeft - currLeft
  let nRight = currRight - flyRight
  if (flyRight - flyLeft > FRAME_MAX_PX) {
    // Keep only the wider side. Tie → `else` → the right space is dropped, so
    // the text ends up on the LEFT of the object. cf. txtfly.cxx:1518-1524
    if (nLeft < nRight) nLeft = 0
    else nRight = 0
  }
  // Checked BEFORE the TEXT_MIN clamp, exactly like Writer. txtfly.cxx:1532
  if (nLeft === 0 && nRight === 0) return 'parallel'

  const textMin = opts?.idealMinTextWidth ?? TEXT_MIN_PX
  if (nLeft < textMin) nLeft = 0
  if (nRight < textMin) nRight = 0
  if (nLeft) return nRight ? 'parallel' : 'left'
  return nRight ? 'right' : 'none'
}

/**
 * `SwTextFly::CalcRightMargin` (txtfly.cxx:1238-1324).
 *
 * For an object whose text flows on its LEFT, the exclusion normally reaches
 * the right edge of the print area — UNLESS another object further right lets
 * text flow on ITS right side (RIGHT or PARALLEL), in which case the exclusion
 * stops at that object's right edge and the text resumes there.
 */
function calcRightMargin(list: Active[], self: number, contentW: number): number {
  let right = contentW
  let flyRight = list[self].x1
  for (let i = 0; i < list.length; i++) {
    if (i === self) continue
    const o = list[i]
    if (o.sur === 'through') continue // txtfly.cxx:1278-1279
    // Writer intersects the other object with aLine = [rFly.Left(), printRight]
    // (txtfly.cxx:1249-1251) and requires a real overlap (txtfly.cxx:1310).
    if (o.x1 <= list[self].x0 || o.x0 >= contentW) continue
    if (o.x1 > flyRight) {
      flyRight = o.x1
      if (o.sur === 'right' || o.sur === 'parallel') {
        if (right > flyRight) right = flyRight
        break // bStop — txtfly.cxx:1319
      }
    }
  }
  return right
}

/**
 * `SwTextFly::CalcLeftMargin` (txtfly.cxx:1327-1388).
 *
 * Symmetric case, with Writer's asymmetry faithfully kept: the backward scan
 * stops at the FIRST preceding object that overlaps, whatever its surround
 * (only THROUGH objects are skipped), and takes its right edge as the left
 * bound.
 */
function calcLeftMargin(list: Active[], self: number, contentW: number): number {
  const flyLeft = list[self].x0
  // "if( nLeft > nFlyLeft ) nLeft = rFly.Left();" — an object protruding into
  // the left margin extends the exclusion out there. txtfly.cxx:1338-1339
  let left = Math.min(0, flyLeft)

  // Advance past the objects that start at/after us, then walk backwards.
  let i = self
  while (++i < list.length) {
    if (list[i].x0 >= flyLeft) break
  }
  while (i > 0) {
    if (--i === self) continue
    const o = list[i]
    if (o.sur === 'through') continue
    if (o.x0 < flyLeft && o.x1 > left && o.x0 < contentW) {
      if (left <= o.x1) left = o.x1
      break // txtfly.cxx:1384
    }
  }
  return left
}

/**
 * Horizontal exclusion intervals produced by the floating objects on the
 * vertical band [yTop, yBottom).  This is the union step of
 * `SwTextFly::GetFrame_`/`AnchoredObjToRect` (txtfly.cxx:380-396, 1391-1462).
 *
 * The returned intervals are sorted, merged and clipped to [0, contentW].
 */
export function exclusionBands(
  yTop: number,
  yBottom: number,
  contentW: number,
  floats: FloatBox[],
  opts?: WrapOpts,
): Segment[] {
  const list = activeFloats(yTop, yBottom, contentW, floats, opts)
  const raw: Segment[] = []
  for (let i = 0; i < list.length; i++) {
    const a = list[i]
    switch (a.sur) {
      case 'through':
        continue
      case 'parallel':
        // No growing: only the object's own footprint (one interval per
        // contour run). txtfly.cxx:1458-1459 (`default: break`).
        for (const s of a.spans) raw.push({ x0: s.x0, x1: s.x1 })
        continue
      case 'left':
        raw.push({ x0: a.x0, x1: calcRightMargin(list, i, contentW) }) // txtfly.cxx:1442-1446
        continue
      case 'right':
        raw.push({ x0: calcLeftMargin(list, i, contentW), x1: a.x1 }) // txtfly.cxx:1447-1451
        continue
      case 'none':
        raw.push({ // txtfly.cxx:1452-1457
          x0: calcLeftMargin(list, i, contentW),
          x1: calcRightMargin(list, i, contentW),
        })
        continue
    }
  }
  return mergeClip(raw, contentW)
}

/**
 * Text segments available on the vertical band [yTop, yBottom) of one line.
 *
 * Returns the complement of {@link exclusionBands} inside [0, contentW], with
 * segments narrower than `minTextWidth` dropped.  An EMPTY array means "no room
 * at all on this band": the caller must move the line down, exactly like
 * Writer's dummy line / `bFullLine` handling (itrform2.cxx:3071-3090).
 */
export function lineSegments(
  yTop: number,
  yBottom: number,
  contentW: number,
  floats: FloatBox[],
  opts?: { minTextWidth?: number; idealMinTextWidth?: number },
): Segment[] {
  const minW = opts?.minTextWidth ?? TEXT_MIN_SMALL_PX
  const bands = exclusionBands(yTop, yBottom, contentW, floats, opts)
  const out: Segment[] = []
  let cursor = 0
  for (const b of bands) {
    if (b.x0 - cursor >= minW - EPS && b.x0 > cursor) out.push({ x0: cursor, x1: b.x0 })
    cursor = Math.max(cursor, b.x1)
  }
  if (contentW - cursor >= minW - EPS && contentW > cursor) out.push({ x0: cursor, x1: contentW })
  return out
}

/** Selects, resolves and sorts the objects intersecting the band. */
function activeFloats(
  yTop: number,
  yBottom: number,
  contentW: number,
  floats: FloatBox[],
  opts?: WrapOpts,
): Active[] {
  const list: Active[] = []
  // Our own guard (not a Writer rule: SwRect::Overlaps, swrect.hxx:248-254, is
  // inclusive and ignores emptiness): a degenerate band has no line to lay out,
  // so callers get the full width instead of a meaningless exclusion.
  if (yBottom <= yTop) return list
  for (const f of floats) {
    const sur = resolveSurround(f, contentW, opts)
    if (sur === 'through') continue // ForEach skips SURROUND_THROUGH — txtfly.cxx:1176-1183
    const v = outerY(f)
    // Half-open overlap: a line that merely touches the object's bottom edge is
    // not pushed aside (Writer's SwRect::Overlaps works on inclusive twips).
    if (v.x1 <= yTop || v.x0 >= yBottom) continue
    const h = outerX(f)
    const runs = f.runs && f.runs.length ? f.runs : [{ x0: f.x, x1: f.x + f.w }]
    const spans = runs.map(r => ({ x0: r.x0 - f.distL, x1: r.x1 + f.distR }))
    // For neighbour margins and for the sort we use the OUTER edges, the way
    // Writer uses GetObjRectWithSpaces (txtfly.cxx:1159, 1356).
    list.push({ x0: h.x0, x1: h.x1, top: v.x0, spans, sur })
  }
  // AnchoredObjOrder: left asc, then top asc, then right desc. txtfly.cxx:66-119
  list.sort((a, b) => (a.x0 - b.x0) || (a.top - b.top) || (b.x1 - a.x1))
  return list
}

/** Sorts, merges and clips a set of intervals to [0, contentW]. */
function mergeClip(raw: Segment[], contentW: number): Segment[] {
  const kept: Segment[] = []
  for (const s of raw) {
    const x0 = Math.max(0, Math.min(s.x0, contentW))
    const x1 = Math.max(0, Math.min(s.x1, contentW))
    if (x1 - x0 > EPS) kept.push({ x0, x1 })
  }
  kept.sort((a, b) => a.x0 - b.x0)
  const merged: Segment[] = []
  for (const s of kept) {
    const last = merged[merged.length - 1]
    if (last && s.x0 <= last.x1 + EPS) last.x1 = Math.max(last.x1, s.x1)
    else merged.push({ x0: s.x0, x1: s.x1 })
  }
  return merged
}

// ── Self-test ────────────────────────────────────────────────────────────────
// Run with:  node --experimental-strip-types src/documents/layout/wrap-bands.ts

let passed = 0
function ok(cond: boolean, label: string): void {
  if (!cond) throw new Error(`ASSERT FAILED: ${label}`)
  passed++
}
function eqSegs(got: Segment[], want: Array<[number, number]>, label: string): void {
  const fmt = (s: Segment[]): string => s.map(v => `[${v.x0.toFixed(2)},${v.x1.toFixed(2)}]`).join(' ')
  const same = got.length === want.length
    && got.every((g, i) => Math.abs(g.x0 - want[i][0]) < 1e-6 && Math.abs(g.x1 - want[i][1]) < 1e-6)
  if (!same) throw new Error(`ASSERT FAILED: ${label}\n  got  ${fmt(got)}\n  want ${fmt(want.map(w => ({ x0: w[0], x1: w[1] })))}`)
  passed++
}

export function selfTest(): number {
  passed = 0
  const box = (x: number, w: number, over: Partial<FloatBox> = {}): FloatBox => ({
    x, y: 0, w, h: 100, wrap: 'square', side: 'both',
    distL: 0, distR: 0, distT: 0, distB: 0, ...over,
  })
  const W = 600
  // Line band well inside the objects' vertical extent (y 0..100).
  const seg = (floats: FloatBox[], w = W, opts?: WrapOpts): Segment[] =>
    lineSegments(10, 30, w, floats, opts)

  // ── Constants match LibreOffice ──────────────────────────────────────────
  ok(Math.abs(TEXT_MIN_PX - 75.6) < 1e-9, 'TEXT_MIN 1134 twips = 75.6 px')
  ok(Math.abs(TEXT_MIN_SMALL_PX - 20) < 1e-9, 'TEXT_MIN_SMALL 300 twips = 20 px')
  ok(Math.abs(FRAME_MAX_PX - 850 / 15) < 1e-9, 'FRAME_MAX 850 twips')

  // ── Baseline ─────────────────────────────────────────────────────────────
  eqSegs(seg([]), [[0, 600]], 'no float → whole content width')
  eqSegs(seg([box(200, 100, { wrap: 'inline' })]), [[0, 600]], 'inline is not a float')

  // ── SURROUND_THROUGH: behind / front exclude nothing ─────────────────────
  eqSegs(seg([box(200, 100, { wrap: 'behind' })]), [[0, 600]], 'behind excludes nothing')
  eqSegs(seg([box(200, 100, { wrap: 'front' })]), [[0, 600]], 'front excludes nothing')
  ok(resolveSurround(box(0, 10, { wrap: 'behind' }), W) === 'through', 'behind → THROUGH')

  // ── SURROUND_NONE: topBottom blocks the full width ───────────────────────
  eqSegs(seg([box(200, 100, { wrap: 'topBottom' })]), [], 'topBottom → no segment')
  ok(resolveSurround(box(200, 100, { wrap: 'topBottom' }), W) === 'none', 'topBottom → NONE')

  // ── Sides ────────────────────────────────────────────────────────────────
  eqSegs(seg([box(200, 100)]), [[0, 200], [300, 600]], 'square/both → two segments')
  eqSegs(seg([box(200, 100, { side: 'left' })]), [[0, 200]], "side 'left' → text on the left only")
  eqSegs(seg([box(200, 100, { side: 'right' })]), [[300, 600]], "side 'right' → text on the right only")
  eqSegs(seg([box(200, 100, { distL: 10, distR: 20 })]), [[0, 190], [320, 600]], 'L/R distances widen the band')
  eqSegs(seg([box(-50, 100)]), [[50, 600]], 'object protruding into the left margin')
  eqSegs(seg([box(200, 100, { wrap: 'tight' })]), [[0, 200], [300, 600]], 'tight uses the side rules too')

  // ── Vertical band selection ──────────────────────────────────────────────
  eqSegs(seg([box(200, 100, { y: 200 })]), [[0, 600]], 'float below the line is ignored')
  eqSegs(seg([box(200, 100, { y: -200 })]), [[0, 600]], 'float above the line is ignored')
  eqSegs(seg([box(200, 100, { y: 30 })]), [[0, 600]], 'float starting exactly at yBottom is ignored')
  eqSegs(seg([box(200, 100, { y: -90 })]), [[0, 600]], 'float ending exactly at yTop is ignored')
  eqSegs(seg([box(200, 100, { y: 20 })]), [[0, 200], [300, 600]], 'partial vertical overlap still wraps')
  eqSegs(lineSegments(10, 10, W, [box(200, 100)]), [[0, 600]], 'empty band → nothing excluded')
  eqSegs(seg([box(200, 100, { y: -100, distB: 15 })]), [[0, 200], [300, 600]], 'bottom distance extends the band')

  // ── "largest" = WrapTextMode_DYNAMIC (txtfly.cxx:1502-1544) ──────────────
  eqSegs(seg([box(100, 100, { side: 'largest' })]), [[200, 600]],
    "largest: wide object near the left → text on the (larger) right side")
  eqSegs(seg([box(400, 100, { side: 'largest' })]), [[0, 400]],
    'largest: wide object near the right → text on the left side')
  eqSegs(seg([box(250, 100, { side: 'largest' })]), [[0, 250]],
    'largest TIE → Writer keeps the LEFT space (else-branch kills nRight)')
  ok(resolveSurround(box(250, 100, { side: 'largest' }), W) === 'left', 'largest tie resolves to LEFT')
  eqSegs(seg([box(280, 40, { side: 'largest' })]), [[0, 280], [320, 600]],
    'largest: object narrower than FRAME_MAX keeps BOTH sides (PARALLEL)')
  ok(resolveSurround(box(70, 60, { side: 'largest' }), 200) === 'none',
    'largest: both sides under TEXT_MIN → NONE')
  eqSegs(lineSegments(10, 30, 200, [box(70, 60, { side: 'largest' })]), [],
    'largest: no side wide enough → line pushed below')
  ok(resolveSurround(box(700, 100, { side: 'largest' }), W) === 'parallel',
    'largest: object entirely outside the column → PARALLEL')
  eqSegs(seg([box(700, 100, { side: 'largest' })]), [[0, 600]],
    'largest: object outside the column excludes nothing inside it')
  ok(resolveSurround(box(0, 300, { side: 'largest' }), 600, { idealMinTextWidth: TEXT_MIN_SMALL_PX }) === 'right',
    'idealMinTextWidth override (SURROUND_TEXT_WRAP_SMALL)')

  // ── Object wider than the column ─────────────────────────────────────────
  eqSegs(seg([box(-20, 650)]), [], 'object wider than the column → no segment')
  eqSegs(seg([box(-20, 650, { side: 'left' })]), [], 'wider than the column, side left → no segment')

  // ── Several objects on the same band ─────────────────────────────────────
  eqSegs(seg([box(100, 100), box(150, 100)]), [[0, 100], [250, 600]],
    'two overlapping objects → merged exclusion')
  eqSegs(seg([box(100, 100), box(300, 100)]), [[0, 100], [200, 300], [400, 600]],
    'two disjoint objects → three segments')
  const lr = [box(100, 100, { side: 'left' }), box(300, 100, { side: 'right' })]
  eqSegs(seg(lr), [[0, 100], [400, 600]],
    "left-only + right-only: Writer clamps each band to its neighbour (CalcLeft/RightMargin)")
  eqSegs(seg([lr[1], lr[0]]), [[0, 100], [400, 600]], 'input order does not matter (objects are sorted)')
  eqSegs(seg([box(100, 100), box(300, 100, { wrap: 'topBottom' })]), [[0, 100]],
    'NONE object clamped on its left by a PARALLEL neighbour')
  eqSegs(seg([box(100, 100, { side: 'left' }), box(300, 100, { side: 'left' })]), [[0, 100]],
    'two left-only objects: the first band runs to the right edge')
  eqSegs(seg([box(100, 100, { side: 'right' }), box(300, 100, { side: 'right' })]), [[400, 600]],
    'two right-only objects: the last band runs from the left edge')
  eqSegs(seg([box(100, 100, { wrap: 'behind' }), box(300, 100)]), [[0, 300], [400, 600]],
    'a THROUGH object never clamps its neighbours')

  // ── Minimum usable width ─────────────────────────────────────────────────
  eqSegs(seg([box(10, 580)]), [], 'slivers under TEXT_MIN_SMALL are dropped')
  eqSegs(seg([box(10, 580)], W, { minTextWidth: 5 }), [[0, 10], [590, 600]], 'minTextWidth override keeps them')
  eqSegs(seg([box(20, 570)]), [[0, 20]], 'exactly TEXT_MIN_SMALL wide → kept')
  eqSegs(seg([box(19, 571)]), [], 'just under TEXT_MIN_SMALL → dropped')

  // ── Contour runs (tight / through) ───────────────────────────────────────
  eqSegs(seg([box(100, 300, { wrap: 'through', runs: [{ x0: 100, x1: 180 }, { x0: 320, x1: 400 }] })]),
    [[0, 100], [180, 320], [400, 600]], 'through: text flows between the contour runs')
  eqSegs(seg([box(100, 300, { wrap: 'tight', runs: [{ x0: 100, x1: 400 }] })]),
    [[0, 100], [400, 600]], 'tight: single envelope run')

  // ── exclusionBands is exported and consistent ────────────────────────────
  eqSegs(exclusionBands(10, 30, W, [box(200, 100)]), [[200, 300]], 'exclusionBands returns the raw band')
  eqSegs(exclusionBands(10, 30, W, [box(100, 100), box(150, 100)]), [[100, 250]], 'exclusionBands merges')

  return passed
}

const argv = (globalThis as { process?: { argv?: string[] } }).process?.argv
if (argv && argv[1] && argv[1].endsWith('wrap-bands.ts')) {
  const n = selfTest()
  console.log(`wrap-bands selfTest: ${n} assertions OK`)
}
