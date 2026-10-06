// Geometry of the formula bar: the draggable grip that shares horizontal space
// between the Name Box and the formula field, and the vertical expansion of the
// bar for multi-line formula editing (Excel's chevron + draggable bottom edge).
//
// Kept pure so the drag maths can be unit-tested and so both the pointer handlers
// and the persisted user preferences agree on what a valid size is.

/** Name Box width limits, in px. Excel's default is ~80. */
export const NAME_BOX_MIN = 48
export const NAME_BOX_MAX = 320
export const NAME_BOX_DEFAULT = 80

/** Bar height limits, in px. One line is the collapsed state. */
export const BAR_ROW_HEIGHT = 26
export const BAR_MIN_HEIGHT = BAR_ROW_HEIGHT
export const BAR_MAX_HEIGHT = 360
/** Height applied when the chevron expands the bar (about four lines). */
export const BAR_EXPANDED_HEIGHT = 96

/**
 * Space the formula field must keep, in px: below this the grip stops eating into
 * it, whatever the pointer does.
 */
export const FIELD_MIN_WIDTH = 160

export function clampNameBoxWidth(w: number, barWidth?: number): number {
  const hardMax = barWidth != null ? Math.max(NAME_BOX_MIN, barWidth - FIELD_MIN_WIDTH) : NAME_BOX_MAX
  return Math.round(Math.min(Math.min(NAME_BOX_MAX, hardMax), Math.max(NAME_BOX_MIN, w)))
}

export function clampBarHeight(h: number): number {
  return Math.round(Math.min(BAR_MAX_HEIGHT, Math.max(BAR_MIN_HEIGHT, h)))
}

/** Name Box width after dragging the grip by `dx` from `startWidth`. */
export function dragNameBox(startWidth: number, dx: number, barWidth?: number): number {
  return clampNameBoxWidth(startWidth + dx, barWidth)
}

/** Bar height after dragging its bottom edge by `dy` from `startHeight`. */
export function dragBarHeight(startHeight: number, dy: number): number {
  return clampBarHeight(startHeight + dy)
}

/** Is the bar showing more than one line? */
export function isExpanded(height: number): boolean {
  return height > BAR_ROW_HEIGHT + 1
}

/**
 * Height the chevron should switch to: collapse to one line when expanded,
 * otherwise restore the last expanded height (or the default).
 */
export function toggleHeight(height: number, lastExpanded?: number): number {
  if (isExpanded(height)) return BAR_ROW_HEIGHT
  return clampBarHeight(lastExpanded && isExpanded(lastExpanded) ? lastExpanded : BAR_EXPANDED_HEIGHT)
}

/** Rows of text that fit in a bar of this height, for the textarea sizing. */
export function visibleRows(height: number, lineHeight = 18): number {
  return Math.max(1, Math.floor((height - 6) / lineHeight))
}
