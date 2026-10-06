// Co-placement of a context menu and its secondary block (the Excel-style mini
// toolbar / icon card that accompanies an object menu).
//
// Three invariants, in priority order:
//   1. BOTH cards stay fully inside the viewport (a margin is kept on every side).
//   2. They NEVER overlap.
//   3. A gap is always preserved between them.
//
// The pair is laid out as a SINGLE unit rather than hanging the block off a fixed
// menu position: when the menu is tall enough to fill the screen there is no side
// left to hang the block on, and treating the menu as immovable can only end in an
// overlap or an off-screen card. Instead the unit is placed near the click and
// clamped as a whole, and the menu's height is capped (it scrolls internally) when
// that is what it takes to keep the gap.
//
// `@kubuno/ui`'s MenuDropdown re-clamps itself after mount (8px margin, height
// capped to the viewport), so the caller must (a) open the menu at `menu`, then
// (b) apply `menuMaxHeight` when present, and (c) re-measure and re-run this
// helper if the mounted menu turned out a different size than estimated.

export interface Size { w: number; h: number }
export interface Point { x: number; y: number }
export interface Rect { left: number; top: number; w: number; h: number }

/** Distance kept between the two cards (invariant 3). */
export const BLOCK_GAP = 8
/** Distance kept between a card and the viewport edge (invariant 1). */
export const VIEWPORT_MARGIN = 8
/** Below this height a scrolling menu is unusable; the pair then degrades. */
export const MIN_MENU_HEIGHT = 96

/** Which side of the menu the secondary block ended up on. */
export type BlockSide = 'above' | 'below' | 'right' | 'left'

export interface PairPlacement {
  menu: Point
  block: Point
  side: BlockSide
  /** Set when the menu must be capped (and scroll) for the pair to fit. */
  menuMaxHeight?: number
}

export interface PlaceOptions {
  gap?: number
  margin?: number
  /** Side tried first; the remaining sides are tried in a fixed fallback order. */
  preferred?: BlockSide
}

function clamp(v: number, lo: number, hi: number): number {
  // When the available span is smaller than the card, `lo` wins: keeping the
  // top-left corner on screen beats centring something that cannot fit.
  return hi < lo ? lo : Math.min(hi, Math.max(lo, v))
}

/**
 * Lay out a context menu and its secondary block together.
 *
 * `click` is the pointer position, `menu`/`block` their natural sizes (the menu's
 * may exceed the viewport: it will be capped), `vp` the viewport.
 */
export function placePair(
  click: Point, menu: Size, block: Size, vp: Size, opts: PlaceOptions = {},
): PairPlacement {
  const gap = opts.gap ?? BLOCK_GAP
  const margin = opts.margin ?? VIEWPORT_MARGIN
  const availW = vp.w - 2 * margin
  const availH = vp.h - 2 * margin
  // A card bigger than the viewport is pinned at the margin and allowed to bleed:
  // nothing better exists, and it keeps the arithmetic below monotonic.
  const bw = Math.min(block.w, availW)
  const bh = Math.min(block.h, availH)
  const preferred = opts.preferred ?? 'above'
  const order: BlockSide[] = [preferred, ...(['above', 'below', 'right', 'left'] as BlockSide[]).filter(s => s !== preferred)]

  for (const side of order) {
    if (side === 'above' || side === 'below') {
      const roomForMenu = availH - gap - bh
      if (roomForMenu < MIN_MENU_HEIGHT) continue          // no usable vertical layout
      const menuH = Math.min(menu.h, roomForMenu)
      const unitH = menuH + gap + bh
      // Both cards share one left edge, clamped on the widest of the two.
      const left = clamp(click.x, margin, vp.w - margin - Math.max(menu.w, bw))
      // Open downwards from the click, then clamp the whole unit.
      const unitTop = clamp(side === 'above' ? click.y - bh - gap : click.y, margin, vp.h - margin - unitH)
      const blockTop = side === 'above' ? unitTop : unitTop + menuH + gap
      const menuTop = side === 'above' ? unitTop + bh + gap : unitTop
      return {
        menu: { x: left, y: menuTop },
        block: { x: left, y: blockTop },
        side,
        ...(menuH < menu.h ? { menuMaxHeight: menuH } : {}),
      }
    }
    const unitW = menu.w + gap + bw
    if (unitW > availW) continue                            // no room side by side
    const unitLeft = clamp(click.x, margin, vp.w - margin - unitW)
    const menuLeft = side === 'right' ? unitLeft : unitLeft + bw + gap
    const blockLeft = side === 'right' ? unitLeft + menu.w + gap : unitLeft
    const menuH = Math.min(menu.h, availH)
    return {
      menu: { x: menuLeft, y: clamp(click.y, margin, vp.h - margin - menuH) },
      block: { x: blockLeft, y: clamp(click.y, margin, vp.h - margin - bh) },
      side,
      ...(menuH < menu.h ? { menuMaxHeight: menuH } : {}),
    }
  }

  // Degenerate viewport (smaller than the block plus a usable menu): stack them
  // from the top margin and let the menu take whatever is left, still gapped.
  const menuH = Math.max(0, availH - gap - bh)
  return {
    menu: { x: margin, y: margin + bh + gap },
    block: { x: margin, y: margin },
    side: 'above',
    menuMaxHeight: menuH,
  }
}

/**
 * Place the block against an ALREADY-MEASURED menu rect.
 *
 * Used when the menu positions itself (MenuDropdown does), so the pair cannot be
 * laid out as one unit: the block hangs off whichever side has room, tried in the
 * usual order. When no side has room — a menu tall enough to fill the screen —
 * the answer is NOT to overlap it or to push the block off-screen but to cap the
 * menu (it scrolls): `menuMaxHeight` is then returned for the caller to apply.
 */
export function placeBlock(
  menu: Rect, block: Size, vp: Size, opts: PlaceOptions = {},
): { left: number; top: number; side: BlockSide; menuTop?: number; menuMaxHeight?: number } {
  const gap = opts.gap ?? BLOCK_GAP
  const margin = opts.margin ?? VIEWPORT_MARGIN
  const preferred = opts.preferred ?? 'above'
  const order: BlockSide[] = [preferred, ...(['above', 'below', 'right', 'left'] as BlockSide[]).filter(s => s !== preferred)]
  const maxLeft = vp.w - margin - block.w
  const maxTop = vp.h - margin - block.h
  const room = {
    above: menu.top - margin,
    below: vp.h - margin - (menu.top + menu.h),
    right: vp.w - margin - (menu.left + menu.w),
    left: menu.left - margin,
  }
  const need = (s: BlockSide) => (s === 'above' || s === 'below' ? block.h : block.w) + gap

  for (const side of order) {
    if (room[side] < need(side)) continue
    if (side === 'above' || side === 'below') {
      const top = side === 'above' ? menu.top - gap - block.h : menu.top + menu.h + gap
      return { left: clamp(menu.left, margin, maxLeft), top: clamp(top, margin, maxTop), side }
    }
    const left = side === 'right' ? menu.left + menu.w + gap : menu.left - gap - block.w
    return { left: clamp(left, margin, maxLeft), top: clamp(menu.top, margin, maxTop), side }
  }

  // Nothing fits as-is. Capping the menu is only enough when it is its HEIGHT that
  // overflows: keep it where it is and drop the block underneath.
  const capBelow = vp.h - margin - block.h - gap - menu.top
  if (capBelow >= MIN_MENU_HEIGHT) {
    return {
      left: clamp(menu.left, margin, maxLeft),
      top: menu.top + capBelow + gap,
      side: 'below',
      menuMaxHeight: capBelow,
    }
  }
  // The menu also sits too low: it has to move. The block takes the top margin and
  // the menu is pushed just below it, capped to the remaining height.
  const top = margin
  const menuTop = top + block.h + gap
  return {
    left: clamp(menu.left, margin, maxLeft),
    top,
    side: 'above',
    menuTop,
    menuMaxHeight: Math.max(MIN_MENU_HEIGHT, vp.h - margin - menuTop),
  }
}

/** True when two rects intersect — a placement returning true here is a bug. */
export function overlaps(a: Rect, b: Rect): boolean {
  return a.left < b.left + b.w && b.left < a.left + a.w && a.top < b.top + b.h && b.top < a.top + a.h
}

/** Rects implied by a placement, for assertions and tests. */
export function pairRects(p: PairPlacement, menu: Size, block: Size): { menu: Rect; block: Rect } {
  return {
    menu: { left: p.menu.x, top: p.menu.y, w: menu.w, h: Math.min(menu.h, p.menuMaxHeight ?? menu.h) },
    block: { left: p.block.x, top: p.block.y, w: block.w, h: block.h },
  }
}
