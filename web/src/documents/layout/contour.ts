// Wrap CONTOUR of a floating object — what makes « Rapproché » and « Au travers »
// differ from « Carré ».
//
// Today the layout engine excludes the object's BOUNDING BOX in every wrap mode,
// so `tight` and `through` are indistinguishable from `square`. Writer does not
// work that way: for a contour wrap it asks the object for a POLYGON and, for each
// text line, turns that polygon into the horizontal intervals it occupies on that
// line's vertical band. This module is the pure-geometry half of that machinery.
//
// LibreOffice map (paths relative to core-master/):
//   sw/source/core/text/txtfly.cxx:158  SwContourCache::CalcBoundRect — contour is
//       used only when SwFormatSurround::IsContour(); otherwise the object rect.
//   sw/source/core/text/txtfly.cxx:207  SwContourCache::ContourRect — builds a
//       TextRanger from the object's PolyPolygon then queries it for the line band.
//   sw/source/core/text/txtfly.cxx:230  the FALLBACK: when GetContour() fails, the
//       polygon IS the frame rectangle. We do exactly the same everywhere below.
//   editeng/source/misc/txtrange.cxx:635 TextRanger::GetTextRanges — the per-band
//       query; returns a FLAT sorted array x0,x1,x0,x1… of occupied intervals.
//   editeng/source/misc/txtrange.cxx:303 SvxBoundArgs::Calc — the scan itself.
//   editeng/source/misc/txtrange.cxx:574 SvxBoundArgs::Area — how a point is
//       classified against the band (inside / above / below).
//   editeng/source/misc/txtrange.cxx:485 `if (IsSimple() && size() > 2) erase the
//       middle` — the ONE line that separates our two modes, see below.
//   sw/source/core/docnode/ndnotxt.cxx:98 SwNoTextNode::CreateContour → the image
//       silhouette, ultimately vcl/source/bitmap/bitmap.cxx:2650 Bitmap::GetContour.
//
// TIGHT vs THROUGH — the whole point:
//   Writer's TextRanger carries a `bSimple` flag ("Just outside edge",
//   include/editeng/txtrange.hxx:51) fed from SwFormatSurround::IsOutside()
//   (txtfly.cxx:252). When it is set, txtrange.cxx:485 DELETES every interval but
//   the first and the last — text then only flows around the OUTSIDE of the
//   silhouette. When it is not set, all intervals survive and text may enter the
//   concavities. That is precisely Word's « Rapproché » (outside only) versus
//   « Au travers » (enter the openings):
//       tight   → outerSpan(contourSpans(...))   — one interval, the silhouette;
//       through → contourSpans(...)              — every interval.
//   `wrapSpans()` below wraps that choice.
//
// Everything here is PURE: no DOM, no module state, no canvas. Coordinates are
// document pixels at 96 dpi, LOCAL to the object's box (origin = its top-left
// corner); the caller translates by the object's position and adds wrapDistL/R
// (see `padLeft`/`padRight`) and wrapDistT/B (by widening the band it asks for).
//
// Self-test at the bottom:
//   cd ~/projects/kubuno/office/frontend
//   node --experimental-strip-types src/documents/layout/contour.ts

/** A ring of points. Implicitly closed: the last point joins the first. */
export type Polygon = Array<{ x: number; y: number }>

/** A horizontal interval occupied by the contour, `x0` <= `x1`. */
export interface Span { x0: number; x1: number }

/** Options of `contourSpans()`. */
export interface SpanOptions {
  /**
   * Distance kept free on the LEFT of the contour (the object's `wrapDistL`).
   * Writer applies it per noted point before merging (`SvxBoundArgs::NotePoint`,
   * txtrange.cxx:129); padding each span then re-merging is equivalent, so two
   * arms closer than padLeft+padRight correctly collapse into ONE obstacle.
   */
  padLeft?: number
  /** Same on the right (`wrapDistR`). */
  padRight?: number
}

const EPS = 1e-9

// ---------------------------------------------------------------------------
// 1. Band query — the heart of the module
// ---------------------------------------------------------------------------

/**
 * Horizontal intervals occupied by `poly` on the band [yTop, yBottom).
 *
 * Returns them sorted by `x0`, non-overlapping, in the polygon's own coordinate
 * space. A CONCAVE polygon yields SEVERAL intervals on the same band (the two
 * arms of a U, the legs of a star) — that is what « au travers » needs and what
 * a bounding box can never express. An empty array means the band misses the
 * polygon entirely (Writer sets the rect width to 0, txtfly.cxx:197).
 *
 * Method — exact, no rasterising: the answer is the x-projection of
 * `poly ∩ band`. Within a sub-band containing NO vertex, every edge crossing it
 * crosses it entirely and no two crossings can swap order (a simple polygon has
 * no self-intersection), so each interval's bounds are linear in y and its
 * extent over the sub-band is just the min/max of its two ends. We therefore cut
 * the band at every vertex y, solve each slice in closed form, and merge.
 * Writer reaches the same result differently (it walks the ring accumulating
 * min/max between the two crossings of the band edges, txtrange.cxx:303-409) but
 * the projection is identical, including the union over the band height that
 * makes a circle's obstacle as wide as the WIDEST of the two band edges.
 *
 * Complexity: O(E) to select the active edges, then O(k·A + k·A·log A) where A
 * is the number of active edges and k the number of slices — both bounded by the
 * vertices whose y falls in the band, i.e. a handful for a text line. Full-ring
 * work is linear, never quadratic in the polygon size.
 */
export function contourSpans(
  poly: Polygon,
  yTop: number,
  yBottom: number,
  opts?: SpanOptions,
): Span[] {
  const padL = opts?.padLeft ?? 0
  const padR = opts?.padRight ?? 0
  if (poly.length < 3) return []

  let y0 = Math.min(yTop, yBottom)
  let y1 = Math.max(yTop, yBottom)

  // Active edges: those that actually meet the band. Horizontal edges never
  // cross a scanline; their extent is always carried by the two edges meeting
  // them at their endpoints, so dropping them changes nothing.
  const ax: number[] = []
  const ay: number[] = []
  const bx: number[] = []
  const by: number[] = []
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i]
    const q = poly[(i + 1) % poly.length]
    if (Math.abs(p.y - q.y) < EPS) continue
    if (Math.min(p.y, q.y) >= y1 - EPS) continue
    if (Math.max(p.y, q.y) <= y0 + EPS) continue
    ax.push(p.x); ay.push(p.y); bx.push(q.x); by.push(q.y)
  }
  if (ax.length === 0) return []

  const xAt = (i: number, y: number): number =>
    ax[i] + ((y - ay[i]) / (by[i] - ay[i])) * (bx[i] - ax[i])

  // A zero-height band (never produced by a text line, but callers probe) is a
  // single scanline: the even-odd rule on its crossings.
  if (y1 - y0 <= EPS) return padAndMerge(scanlineSpans(ax, ay, bx, by, xAt, y0), padL, padR)

  // Cut the band at every vertex y strictly inside it.
  const cuts = [y0, y1]
  for (let i = 0; i < ax.length; i++) {
    if (ay[i] > y0 + EPS && ay[i] < y1 - EPS) cuts.push(ay[i])
    if (by[i] > y0 + EPS && by[i] < y1 - EPS) cuts.push(by[i])
  }
  cuts.sort((a, b) => a - b)

  const raw: Span[] = []
  for (let c = 0; c + 1 < cuts.length; c++) {
    const sa = cuts[c]
    const sb = cuts[c + 1]
    if (sb - sa <= EPS) continue
    const sm = (sa + sb) / 2

    // Every edge meeting this slice spans it whole (no vertex inside).
    const cross: Array<{ lo: number; hi: number; mid: number }> = []
    for (let i = 0; i < ax.length; i++) {
      if (Math.min(ay[i], by[i]) > sa + EPS) continue
      if (Math.max(ay[i], by[i]) < sb - EPS) continue
      const xa = xAt(i, sa)
      const xb = xAt(i, sb)
      cross.push({ lo: Math.min(xa, xb), hi: Math.max(xa, xb), mid: xAt(i, sm) })
    }
    if (cross.length < 2) continue
    cross.sort((p, q) => p.mid - q.mid)
    // Even-odd pairing, as Writer reads its flat array two by two
    // (txtfly.cxx:303-304). An odd count can only come from a malformed ring;
    // dropping the orphan keeps us from inventing an interval.
    const n = cross.length - (cross.length % 2)
    for (let i = 0; i < n; i += 2) raw.push({ x0: cross[i].lo, x1: cross[i + 1].hi })
  }

  return padAndMerge(raw, padL, padR)
}

/** Even-odd crossings of one scanline — used for a degenerate (zero-height) band. */
function scanlineSpans(
  ax: number[], ay: number[], bx: number[], by: number[],
  xAt: (i: number, y: number) => number, y: number,
): Span[] {
  const xs: number[] = []
  for (let i = 0; i < ax.length; i++) {
    // Half-open rule: a vertex belongs to the edge going down, never to both.
    const down = ay[i] <= y && by[i] > y
    const up = by[i] <= y && ay[i] > y
    if (down || up) xs.push(xAt(i, y))
  }
  xs.sort((a, b) => a - b)
  const out: Span[] = []
  for (let i = 0; i + 1 < xs.length; i += 2) out.push({ x0: xs[i], x1: xs[i + 1] })
  return out
}

/** Pads every span then merges those that touch or overlap. */
function padAndMerge(spans: Span[], padLeft: number, padRight: number): Span[] {
  if (spans.length === 0) return []
  const sorted = spans
    .map(s => ({ x0: s.x0 - padLeft, x1: s.x1 + padRight }))
    .sort((a, b) => a.x0 - b.x0)
  const out: Span[] = [sorted[0]]
  for (let i = 1; i < sorted.length; i++) {
    const last = out[out.length - 1]
    if (sorted[i].x0 <= last.x1 + EPS) last.x1 = Math.max(last.x1, sorted[i].x1)
    else out.push(sorted[i])
  }
  return out
}

/**
 * Collapses a span list to its outer silhouette — Writer's `bSimple` case,
 * "Just outside edge" (txtrange.hxx:51, applied at txtrange.cxx:485). This is
 * « Rapproché »: the contour shapes the outline but text never enters an opening.
 */
export function outerSpan(spans: Span[]): Span[] {
  if (spans.length === 0) return []
  return [{ x0: spans[0].x0, x1: spans[spans.length - 1].x1 }]
}

/** Wrap modes that consume a contour. `square` keeps the box and never comes here. */
export type ContourWrap = 'tight' | 'through'

/**
 * The spans an object excludes on a band, for the two contour wrap modes.
 * `tight` = outside only, `through` = openings are usable. Anything else in the
 * document's `wrap` attribute (square, topBottom, behind, front, inline) must
 * keep using the bounding box and must not call this.
 */
export function wrapSpans(
  poly: Polygon,
  yTop: number,
  yBottom: number,
  mode: ContourWrap,
  opts?: SpanOptions,
): Span[] {
  const spans = contourSpans(poly, yTop, yBottom, opts)
  return mode === 'tight' ? outerSpan(spans) : spans
}

/** Complement of `spans` inside [xMin, xMax] — the room actually left for text. */
export function freeSpans(spans: Span[], xMin: number, xMax: number): Span[] {
  const out: Span[] = []
  let cursor = xMin
  for (const s of spans) {
    if (s.x1 <= xMin || s.x0 >= xMax) continue
    if (s.x0 > cursor + EPS) out.push({ x0: cursor, x1: Math.min(s.x0, xMax) })
    cursor = Math.max(cursor, s.x1)
    if (cursor >= xMax) break
  }
  if (cursor < xMax - EPS) out.push({ x0: cursor, x1: xMax })
  return out
}

/** Axis-aligned bounds of a ring. Returns a zero box for an empty ring. */
export function polygonBounds(poly: Polygon): { x0: number; y0: number; x1: number; y1: number } {
  if (poly.length === 0) return { x0: 0, y0: 0, x1: 0, y1: 0 }
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity
  for (const p of poly) {
    if (p.x < x0) x0 = p.x
    if (p.x > x1) x1 = p.x
    if (p.y < y0) y0 = p.y
    if (p.y > y1) y1 = p.y
  }
  return { x0, y0, x1, y1 }
}

/**
 * THE FALLBACK, used everywhere a real contour cannot be produced: the bounding
 * box. Writer does the same — `if (!GetFlyFrame()->GetContour(aPoly)) aPoly =
 * PolyPolygon(frameArea)` (txtfly.cxx:230-232). It never UNDER-covers, so a
 * missing contour degrades to today's behaviour (a square wrap) instead of
 * letting text run over the object.
 */
export function boxContour(w: number, h: number): Polygon {
  return [{ x: 0, y: 0 }, { x: w, y: 0 }, { x: w, y: h }, { x: 0, y: h }]
}

// ---------------------------------------------------------------------------
// 2. Contour of a SHAPE
// ---------------------------------------------------------------------------

/**
 * Legacy / alias kind names, mirrored from `src/shapes/paths.ts` (KIND_ALIASES)
 * and `src/shapes/preset-data.ts` (KIND_TO_PRESET). Vendored on purpose: this
 * file must stay importable by `node --experimental-strip-types`, and the shapes
 * package resolves its imports through the bundler only.
 */
const ALIASES: Record<string, string> = {
  square: 'rect', rectangle: 'rect', circle: 'ellipse', oval: 'ellipse', rounded: 'roundRect',
  process: 'rect', flowProcess: 'rect', flowAltProcess: 'roundRect',
  decision: 'diamond', flowDecision: 'diamond',
  data: 'parallelogram', flowData: 'parallelogram',
  terminator: 'stadium', flowTerminator: 'stadium',
  flowConnector: 'ellipse', flowInternal: 'rect', flowCard: 'rect',
  plus: 'cross', mathPlus: 'cross',
  rightArrow: 'arrow', leftArrow: 'arrowLeft', upArrow: 'arrowUp', downArrow: 'arrowDown',
  star: 'star5',
  // Presets whose outline fills its box: a rectangle over-covers them exactly or
  // very nearly, which is safe (see boxContour's contract).
  cube: 'rect', bevel: 'rect', frame: 'rect', plaque: 'rect', cylinder: 'rect',
  foldedCorner: 'rect', snipRect: 'rect', snip2SameRect: 'rect', snip2DiagRect: 'rect',
  snipRoundRect: 'rect', roundRect1: 'roundRect', round2SameRect: 'roundRect',
  round2DiagRect: 'roundRect',
  // Presets inscribed in the box's ellipse: the ellipse over-covers them.
  donut: 'ellipse', noSymbol: 'ellipse', pie: 'ellipse', chord: 'ellipse',
  arc: 'ellipse', blockArc: 'ellipse', smiley: 'ellipse',
}

/**
 * Ratio inner/outer radius of the n-branch stars, read from our own preset data
 * (`src/shapes/preset-data.ts`, PRESETS[starN].a = [["adj", …]]); the preset
 * expresses it in 1/50000ths of the outer radius, hence adj/50000. star5's 19098
 * is 1/φ², Word's classic five-point star.
 */
const STAR_ADJ: Record<string, { n: number; adj: number }> = {
  star4: { n: 4, adj: 12500 }, star5: { n: 5, adj: 19098 }, star6: { n: 6, adj: 28868 },
  star7: { n: 7, adj: 34601 }, star8: { n: 8, adj: 37500 }, star10: { n: 10, adj: 42533 },
  star12: { n: 12, adj: 37500 }, star16: { n: 16, adj: 37500 },
  star24: { n: 24, adj: 37500 }, star32: { n: 32, adj: 37500 },
}

/** Regular polygons approximated by their inscribed n-gon (hf/vf factors ignored). */
const REGULAR_NGON: Record<string, number> = {
  pentagon: 5, heptagon: 7, decagon: 10, dodecagon: 12,
}

/**
 * Contour of a simple SHAPE inside its `w`×`h` box, origin top-left.
 *
 * Geometry and adjustment defaults come from the OOXML presets we already ship
 * in `src/shapes/preset-data.ts` (themselves LibreOffice's
 * `presetShapeDefinitions`), so a wrap contour matches what is painted. Only the
 * shapes whose outline we can state exactly are modelled; EVERY other kind —
 * unknown, hand-drawn, curved beyond a plain ellipse — falls back to
 * `boxContour()`, which over-covers and therefore degrades gracefully into a
 * square wrap rather than letting text cross the drawing.
 *
 * Note the API returns ONE ring: a donut's hole or a frame's inside cannot be
 * expressed (Writer uses a PolyPolygon there). This costs nothing in practice —
 * a hole that no band reaches from the side is unusable by text anyway.
 */
export function shapeContour(kind: string, w: number, h: number): Polygon {
  if (!(w > 0) || !(h > 0)) return boxContour(Math.max(w, 0), Math.max(h, 0))
  const k = ALIASES[kind] ?? kind
  const ss = Math.min(w, h) // OOXML's `ss`: adjustments are relative to the short side

  switch (k) {
    case 'rect':
      return boxContour(w, h)

    case 'ellipse':
      return ellipseRing(w / 2, h / 2, w / 2, h / 2, ringSteps(w, h))

    case 'roundRect': {
      // PRESETS.roundRect.a = [["adj", 16667]] → r = ss*adj/100000.
      const r = Math.min((ss * 16667) / 100000, ss / 2)
      return roundRectRing(w, h, r)
    }

    case 'stadium':
      // flowChartTerminator: fully rounded ends.
      return roundRectRing(w, h, Math.min(w, h) / 2)

    case 'triangle': {
      // PRESETS.triangle.a = [["adj", 50000]] → apex at w*adj/100000.
      const apex = (w * 50000) / 100000
      return [{ x: apex, y: 0 }, { x: w, y: h }, { x: 0, y: h }]
    }

    case 'rtTriangle':
      return [{ x: 0, y: 0 }, { x: w, y: h }, { x: 0, y: h }]

    case 'diamond':
      return [{ x: w / 2, y: 0 }, { x: w, y: h / 2 }, { x: w / 2, y: h }, { x: 0, y: h / 2 }]

    case 'parallelogram': {
      // PRESETS.parallelogram.a = [["adj", 25000]] → x1 = ss*adj/100000, capped
      // at w so a very flat box degenerates into a triangle rather than folding.
      const x1 = Math.min((ss * 25000) / 100000, w)
      return [{ x: x1, y: 0 }, { x: w, y: 0 }, { x: w - x1, y: h }, { x: 0, y: h }]
    }

    case 'trapezoid': {
      // PRESETS.trapezoid.a = [["adj", 25000]].
      const x1 = Math.min((ss * 25000) / 100000, w / 2)
      return [{ x: x1, y: 0 }, { x: w - x1, y: 0 }, { x: w, y: h }, { x: 0, y: h }]
    }

    case 'hexagon': {
      // PRESETS.hexagon.a = [["adj", 25000], …] → the two flat sides start at x1.
      const x1 = Math.min((ss * 25000) / 100000, w / 2)
      return [
        { x: x1, y: 0 }, { x: w - x1, y: 0 }, { x: w, y: h / 2 },
        { x: w - x1, y: h }, { x: x1, y: h }, { x: 0, y: h / 2 },
      ]
    }

    case 'octagon': {
      // PRESETS.octagon.a = [["adj", 29289]] — the 1-1/√2 corner cut.
      const c = Math.min((ss * 29289) / 100000, ss / 2)
      return [
        { x: c, y: 0 }, { x: w - c, y: 0 }, { x: w, y: c }, { x: w, y: h - c },
        { x: w - c, y: h }, { x: c, y: h }, { x: 0, y: h - c }, { x: 0, y: c },
      ]
    }

    case 'cross': {
      // PRESETS.plus.a = [["adj", 25000]] → arm thickness ss*adj/100000.
      const t = Math.min((ss * 25000) / 100000, ss / 2)
      const x2 = w - t
      const y2 = h - t
      return [
        { x: 0, y: t }, { x: t, y: t }, { x: t, y: 0 }, { x: x2, y: 0 },
        { x: x2, y: t }, { x: w, y: t }, { x: w, y: y2 }, { x: x2, y: y2 },
        { x: x2, y: h }, { x: t, y: h }, { x: t, y: y2 }, { x: 0, y: y2 },
      ]
    }

    case 'chevron': {
      // PRESETS.chevron.a = [["adj", 50000]] → notch depth ss*adj/100000.
      const x1 = Math.min((ss * 50000) / 100000, w)
      return [
        { x: 0, y: 0 }, { x: w - x1, y: 0 }, { x: w, y: h / 2 },
        { x: w - x1, y: h }, { x: 0, y: h }, { x: x1, y: h / 2 },
      ]
    }

    case 'arrow':
    case 'arrowLeft':
    case 'arrowUp':
    case 'arrowDown': {
      // PRESETS.rightArrow.a = [["adj1", 50000], ["adj2", 50000]]:
      // adj1 = shaft thickness / ss, adj2 = head length / ss.
      const horizontal = k === 'arrow' || k === 'arrowLeft'
      const along = horizontal ? w : h
      const across = horizontal ? h : w
      const shaft = Math.min((Math.min(along, across) * 50000) / 100000, across)
      const head = Math.min((Math.min(along, across) * 50000) / 100000, along)
      const a0 = (across - shaft) / 2
      const a1 = a0 + shaft
      // Canonical right-pointing arrow in (along, across) coordinates.
      const ring: Array<[number, number]> = [
        [0, a0], [along - head, a0], [along - head, 0],
        [along, across / 2], [along - head, across], [along - head, a1], [0, a1],
      ]
      return ring.map(([u, v]) => {
        if (k === 'arrow') return { x: u, y: v }
        if (k === 'arrowLeft') return { x: w - u, y: v }
        if (k === 'arrowDown') return { x: v, y: u }
        return { x: v, y: h - u } // arrowUp
      })
    }

    default: {
      const star = STAR_ADJ[k]
      if (star) return starRing(w, h, star.n, star.adj / 50000)
      const n = REGULAR_NGON[k]
      if (n) return ellipseRing(w / 2, h / 2, w / 2, h / 2, n)
      // Unknown or non-modelled preset: the documented bounding-box fallback.
      return boxContour(w, h)
    }
  }
}

/**
 * Vertex count used to flatten an ellipse. LibreOffice hands TextRanger an
 * adaptively subdivided polygon (`getDefaultAdaptiveSubdivision`,
 * txtrange.cxx:48); one vertex per ~4 px of the larger side, clamped to
 * [24, 96], keeps the same visual fidelity at a text line's scale while bounding
 * the per-band work.
 */
function ringSteps(w: number, h: number): number {
  return Math.max(24, Math.min(96, Math.round(Math.max(w, h) / 4)))
}

function ellipseRing(cx: number, cy: number, rx: number, ry: number, steps: number): Polygon {
  const out: Polygon = []
  for (let i = 0; i < steps; i++) {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / steps
    out.push({ x: cx + rx * Math.cos(a), y: cy + ry * Math.sin(a) })
  }
  return out
}

function starRing(w: number, h: number, points: number, innerRatio: number): Polygon {
  const cx = w / 2
  const cy = h / 2
  const out: Polygon = []
  for (let i = 0; i < points * 2; i++) {
    const a = -Math.PI / 2 + (Math.PI * i) / points
    const f = i % 2 === 0 ? 1 : innerRatio
    out.push({ x: cx + cx * f * Math.cos(a), y: cy + cy * f * Math.sin(a) })
  }
  return out
}

function roundRectRing(w: number, h: number, r: number): Polygon {
  if (r <= 0) return boxContour(w, h)
  const per = Math.max(3, Math.round(ringSteps(w, h) / 4))
  const out: Polygon = []
  const corner = (cx: number, cy: number, from: number): void => {
    for (let i = 0; i <= per; i++) {
      const a = from + (Math.PI / 2) * (i / per)
      out.push({ x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) })
    }
  }
  corner(w - r, r, -Math.PI / 2)     // top-right
  corner(w - r, h - r, 0)            // bottom-right
  corner(r, h - r, Math.PI / 2)      // bottom-left
  corner(r, r, Math.PI)              // top-left
  return out
}

// ---------------------------------------------------------------------------
// 3. Contour of an IMAGE, from its alpha channel
// ---------------------------------------------------------------------------

/** Options of `alphaContour()`. */
export interface AlphaOptions {
  /**
   * Alpha (0-255) from which a pixel counts as opaque. Default 1: anything not
   * FULLY transparent is solid — Writer's tdf#161833 fix (bitmap.cxx:2680-2684),
   * without which a soft shadow or an antialiased edge gets clipped away.
   */
  threshold?: number
  /**
   * Sampling pitch in rows. Default: `ceil(h / 48)`, i.e. at most 48 sampled
   * rows whatever the image height, so a 2000 px photo costs the same as a
   * thumbnail. A row stands for the whole block it opens (see below).
   */
  step?: number
}

/**
 * Silhouette of an image, from its RGBA pixels, in the image's PIXEL space
 * (0..w, 0..h) — the caller scales it to the object's displayed size.
 *
 * Same principle as `Bitmap::GetContour` (vcl/source/bitmap/bitmap.cxx:2650),
 * the routine behind `SwNoTextNode::CreateContour` (ndnotxt.cxx:98): for a row,
 * find the first opaque pixel scanning from the LEFT and the first scanning from
 * the RIGHT, and build one ring going down the left silhouette then back up the
 * right one. Consequence, shared with Writer: the ring has ONE span per row, so
 * a hole inside a picture is never seen. Only its outline shapes the wrap.
 *
 * Two deliberate differences from Writer:
 *   * we sample one row out of `step` instead of every row, and each sampled row
 *     is emitted as a BLOCK [y, y+step) (a staircase) so the rows we skipped are
 *     covered by the span we measured rather than by an interpolation that could
 *     cut into the picture;
 *   * we scan the full width, where vcl skips a 1 px border (bitmap.cxx:2691) —
 *     that border exists to absorb its edge-detection pass, which we do not run
 *     since we read a true alpha channel.
 *
 * Complexity: O((h/step) · s) where s is the number of pixels scanned before the
 * first opaque one on each side — bounded by w but in practice the transparent
 * margin only. Never the O(w·h) of a full pixel walk, and memory stays O(h/step).
 *
 * Falls back to `boxContour(w, h)` when there is nothing usable: a degenerate
 * size (vcl bails under 5 px, bitmap.cxx:2660), a short buffer, a fully
 * transparent image, or fewer than two sampled rows with content. Same fallback
 * as Writer's `if (!GetContour(aPoly)) aPoly = frameArea` (txtfly.cxx:230).
 */
export function alphaContour(
  rgba: Uint8ClampedArray,
  w: number,
  h: number,
  opts?: AlphaOptions,
): Polygon {
  if (w <= 4 || h <= 4) return boxContour(w, h)
  if (rgba.length < w * h * 4) return boxContour(w, h)

  const threshold = opts?.threshold ?? 1
  const step = Math.max(1, Math.floor(opts?.step ?? Math.ceil(h / 48)))

  const rows: Array<{ y0: number; y1: number; l: number; r: number }> = []
  for (let y = 0; y < h; y += step) {
    const base = y * w * 4
    let l = -1
    for (let x = 0; x < w; x++) {
      if (rgba[base + x * 4 + 3] >= threshold) { l = x; break }
    }
    if (l < 0) continue
    let r = l
    for (let x = w - 1; x > l; x--) {
      if (rgba[base + x * 4 + 3] >= threshold) { r = x; break }
    }
    // +1 on the right so the span covers the whole opaque pixel, not its origin.
    rows.push({ y0: y, y1: Math.min(y + step, h), l, r: r + 1 })
  }
  if (rows.length < 2) return boxContour(w, h)

  const left: Polygon = []
  const right: Polygon = []
  for (const row of rows) {
    left.push({ x: row.l, y: row.y0 }, { x: row.l, y: row.y1 })
    right.push({ x: row.r, y: row.y0 }, { x: row.r, y: row.y1 })
  }
  right.reverse()
  return simplifyRing(left.concat(right))
}

/** Drops duplicate and collinear vertices — a staircase is mostly redundant. */
function simplifyRing(poly: Polygon): Polygon {
  const out: Polygon = []
  for (const p of poly) {
    const last = out[out.length - 1]
    if (last && Math.abs(last.x - p.x) < 1e-7 && Math.abs(last.y - p.y) < 1e-7) continue
    out.push(p)
  }
  while (out.length > 1) {
    const a = out[0]
    const b = out[out.length - 1]
    if (Math.abs(a.x - b.x) < 1e-7 && Math.abs(a.y - b.y) < 1e-7) out.pop()
    else break
  }
  const res: Polygon = []
  for (let i = 0; i < out.length; i++) {
    const prev = out[(i - 1 + out.length) % out.length]
    const cur = out[i]
    const next = out[(i + 1) % out.length]
    const cross = (cur.x - prev.x) * (next.y - prev.y) - (cur.y - prev.y) * (next.x - prev.x)
    if (Math.abs(cross) < 1e-7) continue
    res.push(cur)
  }
  return res.length >= 3 ? res : out
}

// ---------------------------------------------------------------------------
// 4. Self-test — node --experimental-strip-types src/documents/layout/contour.ts
// ---------------------------------------------------------------------------

/** Runs the assertions and returns how many passed. Throws on the first failure. */
export function selfTest(): number {
  let n = 0
  const ok = (cond: boolean, label: string): void => {
    if (!cond) throw new Error(`contour selfTest FAILED: ${label}`)
    n++
  }
  const near = (a: number, b: number, tol: number, label: string): void =>
    ok(Math.abs(a - b) <= tol, `${label} (got ${a}, expected ~${b})`)
  const width = (s: Span[]): number => s.reduce((acc, v) => acc + (v.x1 - v.x0), 0)

  // --- rectangle: a contour wrap must reproduce the box exactly -------------
  const rect = boxContour(100, 50)
  let s = contourSpans(rect, 10, 20)
  ok(s.length === 1, 'rect: one span')
  near(s[0].x0, 0, 1e-9, 'rect: x0')
  near(s[0].x1, 100, 1e-9, 'rect: x1')
  s = contourSpans(rect, 0, 50)
  ok(s.length === 1 && Math.abs(s[0].x0) < 1e-9 && Math.abs(s[0].x1 - 100) < 1e-9, 'rect: full band')
  ok(contourSpans(rect, 60, 70).length === 0, 'rect: band below → no span')
  ok(contourSpans(rect, -20, -10).length === 0, 'rect: band above → no span')
  s = contourSpans(rect, -10, 10)
  ok(s.length === 1 && Math.abs(s[0].x1 - 100) < 1e-9, 'rect: band straddling the top')
  s = contourSpans(rect, 25, 25)
  ok(s.length === 1 && Math.abs(s[0].x1 - 100) < 1e-9, 'rect: degenerate band = scanline')

  // --- circle: the width must FOLLOW y (this is what square cannot do) ------
  const circle = shapeContour('ellipse', 100, 100)
  const wTop = width(contourSpans(circle, 0, 10))
  const wMid = width(contourSpans(circle, 45, 55))
  const wLow = width(contourSpans(circle, 20, 30))
  ok(contourSpans(circle, 45, 55).length === 1, 'circle: convex → one span')
  ok(wTop < wLow && wLow < wMid, 'circle: width grows towards the middle')
  near(wMid, 100, 1.0, 'circle: full width at the equator')
  near(wTop, 2 * Math.sqrt(50 * 50 - 40 * 40), 2.0, 'circle: chord at y=10')
  const mid = contourSpans(circle, 45, 55)[0]
  near(mid.x0 + mid.x1, 100, 1e-6, 'circle: symmetric about the centre')
  ok(contourSpans(circle, 99.9, 110).length <= 1, 'circle: band past the bottom')
  ok(JSON.stringify(shapeContour('circle', 100, 100)) === JSON.stringify(circle), 'circle alias = ellipse')

  // --- U shape: TWO spans on the arms — tight vs through --------------------
  // Arms x∈[0,2] and x∈[8,10] above y=8, solid base below.
  const u: Polygon = [
    { x: 0, y: 0 }, { x: 2, y: 0 }, { x: 2, y: 8 }, { x: 8, y: 8 },
    { x: 8, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }, { x: 0, y: 10 },
  ]
  const arms = contourSpans(u, 2, 4)
  ok(arms.length === 2, 'U: two spans on the arms')
  near(arms[0].x0, 0, 1e-9, 'U: left arm x0')
  near(arms[0].x1, 2, 1e-9, 'U: left arm x1')
  near(arms[1].x0, 8, 1e-9, 'U: right arm x0')
  near(arms[1].x1, 10, 1e-9, 'U: right arm x1')
  const base = contourSpans(u, 8.5, 9.5)
  ok(base.length === 1 && Math.abs(base[0].x1 - 10) < 1e-9, 'U: one span in the base')
  const straddle = contourSpans(u, 7, 9)
  ok(straddle.length === 1 && Math.abs(straddle[0].x0) < 1e-9 && Math.abs(straddle[0].x1 - 10) < 1e-9,
    'U: a band straddling the base is solid')
  // THE distinction: same band, two answers.
  ok(wrapSpans(u, 2, 4, 'through').length === 2, 'U: through keeps both arms')
  const tight = wrapSpans(u, 2, 4, 'tight')
  ok(tight.length === 1 && Math.abs(tight[0].x0) < 1e-9 && Math.abs(tight[0].x1 - 10) < 1e-9,
    'U: tight collapses to the silhouette')
  ok(width(tight) > width(arms), 'U: tight excludes strictly more than through')
  // and neither may be confused with the box on that band
  ok(width(arms) < 10, 'U: through leaves the opening free (≠ square)')

  // --- wrapDist padding merges arms that are too close ----------------------
  const padded = contourSpans(u, 2, 4, { padLeft: 10, padRight: 10 })
  ok(padded.length === 1, 'U: padding merges the two arms')
  near(padded[0].x0, -10, 1e-9, 'U: padded x0')
  near(padded[0].x1, 20, 1e-9, 'U: padded x1')
  const padSmall = contourSpans(u, 2, 4, { padLeft: 1, padRight: 1 })
  ok(padSmall.length === 2, 'U: a small padding keeps the opening')

  // --- free space, the complement the engine actually lays text into --------
  const free = freeSpans(arms, 0, 10)
  ok(free.length === 1 && Math.abs(free[0].x0 - 2) < 1e-9 && Math.abs(free[0].x1 - 8) < 1e-9,
    'U: free room = the opening')
  ok(freeSpans(outerSpan(arms), 0, 10).length === 0, 'U: tight leaves no room inside the box')

  // --- shapes --------------------------------------------------------------
  ok(shapeContour('rect', 100, 50).length === 4, 'shape rect: 4 vertices')
  const unknown = shapeContour('someShapeWeDoNotModel', 100, 50)
  const ub = polygonBounds(unknown)
  ok(unknown.length === 4 && ub.x1 === 100 && ub.y1 === 50, 'shape: unknown kind → box fallback')
  const diamond = shapeContour('diamond', 100, 100)
  ok(width(contourSpans(diamond, 0, 5)) < width(contourSpans(diamond, 45, 55)), 'diamond narrows upwards')
  ok(JSON.stringify(shapeContour('flowDecision', 60, 40)) === JSON.stringify(shapeContour('diamond', 60, 40)),
    'flowDecision alias = diamond')
  const star5 = shapeContour('star', 100, 100)
  let maxSpans = 0
  for (let y = 0; y < 100; y += 2) maxSpans = Math.max(maxSpans, contourSpans(star5, y, y + 2).length)
  ok(maxSpans >= 2, 'star5: at least one band with several spans')
  ok(outerSpan(contourSpans(star5, 70, 74)).length === 1, 'star5: tight is always one span')
  const cross = shapeContour('cross', 100, 100)
  ok(cross.length === 12, 'cross: 12 vertices')
  ok(width(contourSpans(cross, 0, 5)) < width(contourSpans(cross, 48, 52)), 'cross: narrow arm at the top')
  const arrow = shapeContour('arrow', 100, 100)
  const ab = polygonBounds(arrow)
  ok(ab.x0 === 0 && ab.y0 === 0 && Math.abs(ab.x1 - 100) < 1e-9 && Math.abs(ab.y1 - 100) < 1e-9,
    'arrow: fills its box')
  ok(width(contourSpans(arrow, 0, 2)) < width(contourSpans(arrow, 49, 51)), 'arrow: head narrow at the top')
  const oct = shapeContour('octagon', 100, 100)
  ok(oct.length === 8 && width(contourSpans(oct, 0, 2)) < 100, 'octagon: cut corners')

  // --- image alpha ---------------------------------------------------------
  const img = (iw: number, ih: number, f: (x: number, y: number) => number): Uint8ClampedArray => {
    const buf = new Uint8ClampedArray(iw * ih * 4)
    for (let y = 0; y < ih; y++) {
      for (let x = 0; x < iw; x++) buf[(y * iw + x) * 4 + 3] = f(x, y)
    }
    return buf
  }
  const empty = alphaContour(img(40, 40, () => 0), 40, 40)
  ok(JSON.stringify(empty) === JSON.stringify(boxContour(40, 40)), 'alpha: transparent image → box')
  const tiny = alphaContour(img(4, 4, () => 255), 4, 4)
  ok(JSON.stringify(tiny) === JSON.stringify(boxContour(4, 4)), 'alpha: 4 px image → box (vcl bails too)')
  ok(JSON.stringify(alphaContour(new Uint8ClampedArray(8), 40, 40)) === JSON.stringify(boxContour(40, 40)),
    'alpha: short buffer → box')

  const solid = alphaContour(img(40, 40, () => 255), 40, 40, { step: 1 })
  const sb = polygonBounds(solid)
  ok(sb.x0 === 0 && sb.x1 === 40 && sb.y0 === 0 && sb.y1 === 40, 'alpha: opaque image = its box')
  near(width(contourSpans(solid, 10, 12)), 40, 1e-9, 'alpha: opaque image spans the width')

  // A disc: the silhouette must widen towards the middle, like the ellipse.
  const R = 20
  const disc = img(40, 40, (x, y) => ((x - 20 + 0.5) ** 2 + (y - 20 + 0.5) ** 2 <= R * R ? 255 : 0))
  const discPoly = alphaContour(disc, 40, 40, { step: 2 })
  ok(discPoly.length > 4, 'alpha disc: a real silhouette, not the box')
  const dTop = width(contourSpans(discPoly, 1, 3))
  const dMid = width(contourSpans(discPoly, 19, 21))
  ok(dTop < dMid, 'alpha disc: wider at the equator')
  near(dMid, 40, 2, 'alpha disc: full width at the equator')
  const db = polygonBounds(discPoly)
  ok(db.x0 >= 0 && db.x1 <= 40 && db.y0 >= 0 && db.y1 <= 40, 'alpha disc: stays inside the image')

  // Two opaque bars: a row silhouette CANNOT see the gap — documented limit,
  // identical to Bitmap::GetContour.
  const bars = img(40, 40, x => (x < 8 || x >= 32 ? 255 : 0))
  const barsPoly = alphaContour(bars, 40, 40, { step: 4 })
  const barSpans = contourSpans(barsPoly, 10, 14)
  ok(barSpans.length === 1, 'alpha bars: one span (silhouette has no hole)')
  near(barSpans[0].x1 - barSpans[0].x0, 40, 1e-9, 'alpha bars: the span covers both bars')

  // Threshold: semi-transparent counts as opaque by default, not above it.
  const soft = img(40, 40, (x, y) => (x >= 10 && x < 30 && y >= 10 && y < 30 ? 100 : 0))
  const softDefault = alphaContour(soft, 40, 40, { step: 2 })
  ok(polygonBounds(softDefault).x0 === 10, 'alpha: alpha=100 is opaque by default (tdf#161833)')
  const softStrict = alphaContour(soft, 40, 40, { step: 2, threshold: 200 })
  ok(JSON.stringify(softStrict) === JSON.stringify(boxContour(40, 40)),
    'alpha: threshold 200 discards it → box fallback')

  // Sampling really is cheap: a tall image costs the default 48 rows.
  const tall = alphaContour(img(20, 960, () => 255), 20, 960)
  ok(tall.length <= 8, 'alpha: a 960 px image collapses to a handful of vertices')

  return n
}

const argv = (globalThis as { process?: { argv?: string[] } }).process?.argv
if (argv && argv[1] && argv[1].endsWith('contour.ts')) {
  const count = selfTest()
  console.log(`contour.ts — ${count} assertions vertes`)
}
