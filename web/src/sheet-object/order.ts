// Pure z-order helpers for the sheet's floating objects.
//
// Paint order = ARRAY order: the last element is painted last, hence on top. These
// helpers only ever move an item inside its own array, which is exactly how far the
// architecture lets ordering go:
//
//   • pictures are painted on the grid canvas (z-index 1),
//   • charts and shapes are DOM overlays living in their own z-index bands.
//
// So "bring to front" means "in front of the other objects OF THE SAME TYPE"; no
// amount of reordering can lift a picture above a chart. Excel's own four commands
// (front / forward / backward / back) map onto that faithfully within a type.
//
// Free of React and of any object model, so both the index-addressed pictures and
// the id-addressed charts/shapes go through the same code.

import type { ObjOrderOp } from './types'

export interface OrderResult<T> {
  /** The reordered array — the SAME reference when nothing moved. */
  items: T[]
  /** Where the moved item now sits (unchanged when nothing moved, -1 if not found). */
  index: number
  changed: boolean
}

/** Target slot for `op`, clamped to the array — equal to `index` when already there. */
export function orderTarget(length: number, index: number, op: ObjOrderOp): number {
  switch (op) {
    case 'front':    return length - 1
    case 'back':     return 0
    case 'forward':  return Math.min(length - 1, index + 1)
    case 'backward': return Math.max(0, index - 1)
  }
}

/** Would `op` actually move the item at `index`? Drives the menu items' disabled state. */
export function canOrder(length: number, index: number, op: ObjOrderOp): boolean {
  if (index < 0 || index >= length || length < 2) return false
  return orderTarget(length, index, op) !== index
}

/** Move the item at `index` (front / forward / backward / back). */
export function reorderAt<T>(items: T[], index: number, op: ObjOrderOp): OrderResult<T> {
  if (index < 0 || index >= items.length) return { items, index: -1, changed: false }
  const to = orderTarget(items.length, index, op)
  if (to === index) return { items, index, changed: false }
  const next = items.slice()
  const [moved] = next.splice(index, 1)
  next.splice(to, 0, moved)
  return { items: next, index: to, changed: true }
}

/** Same, for the id-addressed collections (charts, shapes). */
export function reorderById<T extends { id?: string }>(items: T[], id: string, op: ObjOrderOp): OrderResult<T> {
  return reorderAt(items, items.findIndex(o => o.id === id), op)
}

/**
 * Move SEVERAL items at once (a multi-selection), keeping their relative order.
 *
 * Applying `reorderAt` once per item would not do: sending three objects to the
 * front one after the other reverses them, and stepping them forward one by one
 * lets the first one jump over the second. So the group is treated as a block:
 * front/back extract it and re-insert it whole, forward/backward walk from the
 * edge and skip a neighbour that belongs to the group.
 */
export function reorderMany<T>(items: T[], indices: readonly number[], op: ObjOrderOp): T[] {
  const sel = [...new Set(indices)].filter(i => i >= 0 && i < items.length).sort((a, b) => a - b)
  if (!sel.length || sel.length === items.length) return items
  if (op === 'front' || op === 'back') {
    const inSel = new Set(sel)
    const block = sel.map(i => items[i])
    const rest = items.filter((_, i) => !inSel.has(i))
    return op === 'front' ? [...rest, ...block] : [...block, ...rest]
  }
  const next = items.slice()
  const inSel = new Set(sel)
  if (op === 'forward') {
    for (let k = sel.length - 1; k >= 0; k--) {
      const i = sel[k]
      if (i + 1 >= next.length || inSel.has(i + 1)) continue
      ;[next[i], next[i + 1]] = [next[i + 1], next[i]]
      inSel.delete(i); inSel.add(i + 1); sel[k] = i + 1
    }
  } else {
    for (let k = 0; k < sel.length; k++) {
      const i = sel[k]
      if (i - 1 < 0 || inSel.has(i - 1)) continue
      ;[next[i], next[i - 1]] = [next[i - 1], next[i]]
      inSel.delete(i); inSel.add(i - 1); sel[k] = i - 1
    }
  }
  return next
}

/** Convenience shorthands, mostly for tests and one-off call sites. */
export const toFront  = <T>(items: T[], index: number) => reorderAt(items, index, 'front')
export const forward  = <T>(items: T[], index: number) => reorderAt(items, index, 'forward')
export const backward = <T>(items: T[], index: number) => reorderAt(items, index, 'backward')
export const toBack   = <T>(items: T[], index: number) => reorderAt(items, index, 'back')
