// MULTI-SELECTION of floating objects (charts, pictures, shapes).
//
// The editor used to hold one selected object per kind, mutually exclusive. This
// module replaces that with a single ORDERED list of `ObjRef`:
//
//   • the list IS the selection — one entry means the classic single selection;
//   • its LAST entry is the ACTIVE object: the one whose kind-specific menu, mini
//     bar and contextual ribbon tab are shown, and the one a resize/rotate handle
//     belongs to (Excel calls it the anchor of the selection);
//   • a plain click replaces the list, Ctrl/Cmd or Shift + click toggles one entry
//     in it, exactly like Excel and LibreOffice.
//
// Nothing here touches React or the sheet model: the host maps refs to objects.
// Group geometry (align / distribute) lives here too, since it is pure arithmetic
// on boxes and is what the group commands are made of.

import type { ObjKind, ObjRef } from './types'
import { sameRef } from './types'

/** The selection, oldest first; the last entry is the active object. */
export type ObjSelection = readonly ObjRef[]

/** Stable string identity of a ref — for Sets, React keys and signatures. */
export function refKey(ref: ObjRef): string {
  return `${ref.kind}:${ref.id}`
}

export function inSelection(sel: ObjSelection, ref: ObjRef | null): boolean {
  return !!ref && sel.some(r => sameRef(r, ref))
}

/** The active (anchor) object: the most recently added one. */
export function activeRef(sel: ObjSelection): ObjRef | null {
  return sel.length ? sel[sel.length - 1] : null
}

/**
 * Apply a click on `ref`.
 *
 * `additive` (Ctrl/Cmd or Shift held): the object joins the selection, or leaves it
 * when it was already in — and leaving it makes the previous entry active again.
 * Otherwise the click replaces the whole selection, UNLESS the object is already
 * part of a multi-selection: clicking inside a group must not blow it away, since
 * that click usually starts a group drag (Excel's behaviour, and the reason a plain
 * click on an already-selected object only re-anchors it).
 */
export function clickSelection(sel: ObjSelection, ref: ObjRef, additive: boolean): ObjRef[] {
  if (additive) {
    return inSelection(sel, ref)
      ? sel.filter(r => !sameRef(r, ref))
      : [...sel, ref]
  }
  if (sel.length > 1 && inSelection(sel, ref)) {
    // Re-anchor without shrinking: move the clicked ref to the end.
    return [...sel.filter(r => !sameRef(r, ref)), ref]
  }
  return [ref]
}

/** Drop refs the sheet no longer holds (an object was deleted or an index shifted). */
export function pruneSelection(sel: ObjSelection, exists: (ref: ObjRef) => boolean): ObjRef[] {
  return sel.filter(exists)
}

/** The kinds present in the selection, in a stable order. */
export function selectionKinds(sel: ObjSelection): ObjKind[] {
  const order: ObjKind[] = ['chart', 'image', 'shape', 'equation']
  return order.filter(k => sel.some(r => r.kind === k))
}

/** True when the selection holds objects of DIFFERENT natures. */
export function isMixedSelection(sel: ObjSelection): boolean {
  return selectionKinds(sel).length > 1
}

/**
 * The single kind of a homogeneous selection, or null when it is empty or mixed.
 * This is what decides which contextual ribbon tab and which kind-specific menu
 * entries may be shown.
 */
export function soleKind(sel: ObjSelection): ObjKind | null {
  const kinds = selectionKinds(sel)
  return kinds.length === 1 ? kinds[0] : null
}

/** Signature of the selection — cheap change detection for effects and memos. */
export function selectionSig(sel: ObjSelection): string {
  return sel.map(refKey).join('|')
}

// ── Group geometry ───────────────────────────────────────────────────────────

/** Box in base px from the data origin — the geometry contract of every object. */
export interface Boxed { bx: number; by: number; bw: number; bh: number }

/**
 * Alignment / distribution modes, Excel's « Aligner » submenu. Alignment is
 * relative to the BOUNDING BOX of the selection (Excel aligns to the selection,
 * not to the sheet, as soon as more than one object is selected).
 */
export type AlignMode =
  | 'left' | 'centerH' | 'right'
  | 'top' | 'middleV' | 'bottom'
  | 'distH' | 'distV'

/** Bounding box of a set of boxes, or null when empty. */
export function boundingBox(boxes: readonly Boxed[]): Boxed | null {
  if (!boxes.length) return null
  let x1 = Infinity, y1 = Infinity, x2 = -Infinity, y2 = -Infinity
  for (const b of boxes) {
    x1 = Math.min(x1, b.bx); y1 = Math.min(y1, b.by)
    x2 = Math.max(x2, b.bx + b.bw); y2 = Math.max(y2, b.by + b.bh)
  }
  return { bx: x1, by: y1, bw: x2 - x1, bh: y2 - y1 }
}

/**
 * New top-left corners after an align / distribute, in the SAME order as the input.
 * Sizes never change. Distribution keeps the two extreme objects where they are and
 * equalises the GAPS between the others (Excel's « Distribuer horizontalement »),
 * which is why it needs at least three objects to do anything.
 */
export function alignBoxes(boxes: readonly Boxed[], mode: AlignMode): { bx: number; by: number }[] {
  const bb = boundingBox(boxes)
  if (!bb) return []
  const out = boxes.map(b => ({ bx: b.bx, by: b.by }))
  if (mode === 'distH' || mode === 'distV') {
    if (boxes.length < 3) return out
    const horiz = mode === 'distH'
    const idx = boxes.map((_, i) => i).sort((a, b) => horiz
      ? boxes[a].bx - boxes[b].bx
      : boxes[a].by - boxes[b].by)
    const span = horiz ? bb.bw : bb.bh
    const sizes = idx.reduce((s, i) => s + (horiz ? boxes[i].bw : boxes[i].bh), 0)
    // Free space shared between the n-1 gaps; a negative value (overlapping
    // objects) still distributes, it just keeps them overlapping evenly.
    const gap = (span - sizes) / (idx.length - 1)
    let cursor = horiz ? bb.bx : bb.by
    for (const i of idx) {
      if (horiz) { out[i].bx = cursor; cursor += boxes[i].bw + gap }
      else { out[i].by = cursor; cursor += boxes[i].bh + gap }
    }
    return out
  }
  boxes.forEach((b, i) => {
    switch (mode) {
      case 'left': out[i].bx = bb.bx; break
      case 'centerH': out[i].bx = bb.bx + (bb.bw - b.bw) / 2; break
      case 'right': out[i].bx = bb.bx + bb.bw - b.bw; break
      case 'top': out[i].by = bb.by; break
      case 'middleV': out[i].by = bb.by + (bb.bh - b.bh) / 2; break
      case 'bottom': out[i].by = bb.by + bb.bh - b.bh; break
    }
  })
  return out
}
