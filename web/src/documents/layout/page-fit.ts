/**
 * page-fit.ts — what happens to a floating (anchored) object that does not fit
 * on the page it was laid out on.
 *
 * Pure functions, no DOM, no module state. Coordinates are document pixels at
 * 96 dpi, expressed in the CONTENT BOX of one page: y = 0 is the top of the
 * printable area, y = `contentH` its bottom.
 *
 * Neither Word nor Writer ever CUTS an anchored object at a page boundary:
 *
 *   - Writer gives up the object's position first, then (only if it still does
 *     not fit) its size — `SwFlyFreeFrame::CheckClip`,
 *     sw/source/core/layout/flylay.cxx:471-706 ("if the Fly doesn't fit into its
 *     surrounding ... the Fly gives up its position, then it's formatted. Only
 *     if it still doesn't fit ... the width or height are given up as well").
 *   - When the object follows the text flow, Writer instead moves it WHOLE to
 *     the next page — sw/source/core/objectpositioning/
 *     tocntntanchoredobjectposition.cxx:978-1083, the new Y being the print-area
 *     top of that page (:1025-1031).
 *   - Word does the same: the object is reported on the next page, never split
 *     and never shrunk.
 *
 * We implement the page report (Word's behaviour): it is the only one that
 * keeps the object at its authored SIZE, which is what a user checks first.
 *
 * The rule itself is not re-derived here — it is `resolveObjectPosition()` of
 * `anchor-position.ts`, fed with a degenerate page whose margins are zero, so
 * that the "page frame" and the "body frame" both coincide with the content box
 * we reason in. Two consequences fall out of Writer's own ordering and are
 * wanted here:
 *
 *   - an object TALLER than the content box is flushed to the top and simply
 *     spills past the bottom (the top clamp is applied last and
 *     unconditionally, anchoredobjectposition.cxx:602-605), and the page-move
 *     loop bails out because the object already sits at the top
 *     (tocntntanchoredobjectposition.cxx:1000-1004);
 *   - an object that starts ABOVE the content box is pulled back to its top,
 *     never reported.
 */

import { makePageGeom, resolveObjectPosition } from './anchor-position'

export interface PageFit {
  /**
   * Top of the object, in the content box of the page it ends up on
   * (i.e. of the NEXT page when `pageAdvance` is 1).
   */
  top: number
  /** 0 = stays on its page; 1 = reported whole on the next page. */
  pageAdvance: 0 | 1
}

/**
 * Resolve where a floating object whose bounding box is [`top`, `top + height`]
 * in the content box of a page of height `contentH` actually lands.
 *
 * `height` must be the height of the rotated bounding box (AABB), which is what
 * the reader sees and what Writer confines (`SwAnchoredObject::GetObjRect`).
 */
export function fitFloatingObject(top: number, height: number, contentH: number): PageFit {
  // Degenerate geometry: no page is smaller than its content box, so a
  // non-positive height means "we do not know" — leave the object alone.
  if (!(contentH > 0) || !isFinite(top) || !isFinite(height)) return { top, pageAdvance: 0 }
  // A page with zero margins: `pageH` == body bottom == contentH, `marginT` == 0
  // == body top. `pageW` is irrelevant (we ignore the horizontal result), but it
  // must be large enough not to trigger the horizontal clamp.
  const page = makePageGeom(1e6, contentH, 0, 0, 0, 0)
  const res = resolveObjectPosition(
    { relH: 'margin', relV: 'paragraph', offsetX: 0, offsetY: top, w: 0, h: height },
    page,
    { paraTop: 0, paraLeft: 0, paraWidth: 1e6 },
    // followTextFlow = true is what enables the page report and disables the
    // "clamp to the bottom" branch (tocntnt...:457 — bCheckBottom is the
    // negation of DoesObjFollowsTextFlow()).
    { followTextFlow: true, keepInsidePage: true },
  )
  return { top: res.y, pageAdvance: res.pageAdvance >= 1 ? 1 : 0 }
}

// ── Self test ─────────────────────────────────────────────────────────────────
// Run with:  node --experimental-strip-types src/documents/layout/page-fit.ts

export function selfTest(): boolean {
  let n = 0
  const eq = (got: PageFit, top: number, adv: 0 | 1, what: string): void => {
    n++
    if (Math.abs(got.top - top) > 1e-6 || got.pageAdvance !== adv) {
      throw new Error(`${what}: expected {top:${top}, pageAdvance:${adv}}, got {top:${got.top}, pageAdvance:${got.pageAdvance}}`)
    }
  }
  const H = 800

  // Fits entirely: untouched.
  eq(fitFloatingObject(0, 100, H), 0, 0, 'flush top')
  eq(fitFloatingObject(350, 100, H), 350, 0, 'middle')
  eq(fitFloatingObject(700, 100, H), 700, 0, 'flush bottom')
  eq(fitFloatingObject(699.5, 100, H), 699.5, 0, 'one half-pixel of slack')

  // Straddles the bottom edge: reported WHOLE on the next page, at its top.
  eq(fitFloatingObject(701, 100, H), 0, 1, 'one pixel over the edge')
  eq(fitFloatingObject(760, 420, H), 0, 1, 'the audit case (200x420 near the bottom)')
  eq(fitFloatingObject(799, 2, H), 0, 1, 'sliver hanging off the bottom')
  // Anchored below the page bottom altogether (huge wrapY): same report.
  eq(fitFloatingObject(1200, 50, H), 0, 1, 'anchored past the page bottom')

  // Taller than the content box: reported ONCE (it does not fit where it is),
  // then flushed to the top of that page where it stays and simply spills past
  // the bottom — "moving it would not help either anymore",
  // tocntntanchoredobjectposition.cxx:1000-1004.
  eq(fitFloatingObject(100, 900, H), 0, 1, 'taller than the page, anchored low')
  eq(fitFloatingObject(0, 900, H), 0, 0, 'taller than the page, already at the top')
  eq(fitFloatingObject(0, 800, H), 0, 0, 'exactly as tall as the content box')

  // Starts above the content box: pulled back down, never reported.
  eq(fitFloatingObject(-40, 100, H), 0, 0, 'dragged above the top edge')
  eq(fitFloatingObject(-1000, 100, H), 0, 0, 'dragged far above the top edge')

  // Degenerate inputs are inert.
  eq(fitFloatingObject(123, 45, 0), 123, 0, 'unknown page height')
  eq(fitFloatingObject(123, 45, -10), 123, 0, 'negative page height')
  n++
  const nan = fitFloatingObject(NaN, 45, H)
  if (!Number.isNaN(nan.top) || nan.pageAdvance !== 0) throw new Error('NaN top must be inert')

  // A reported object, re-examined on its new page, must be STABLE (the
  // renderer walks the pages in order and would otherwise loop).
  const once = fitFloatingObject(760, 420, H)
  eq(fitFloatingObject(once.top, 420, H), 0, 0, 'stable after one report')

  console.log(`page-fit selfTest: ${n} assertions OK`)
  return true
}

const _argv = (globalThis as { process?: { argv?: string[] } }).process?.argv
if (_argv?.[1]?.endsWith('page-fit.ts')) selfTest()
