// Moved to `shapes/adjust.ts` — the adjustment knobs are shared by every office
// sub-editor now, not a spreadsheet feature. Kept as a re-export so the sheet's
// call sites (and its `SheetShapeKind`, a superset of `ShapeKind`) keep working.
export {
  SHAPE_ADJUSTMENTS, adjValues, adjustHandles, adjustFromDrag, hitAdjust,
  type AdjustSpec, type AdjustHandle,
} from '../shapes/adjust'
