// Moved to `shapes/native-geometry.ts` — the native shape geometry is shared by
// every office sub-editor now (sheet SVG overlay, presentations/whiteboard/diagrams
// canvas, galleries, inline editor), not a spreadsheet feature. Kept as a re-export
// so the sheet's existing call sites keep importing from here unchanged.
export {
  asNative, hasNativeGeometry, SHAPE_KINDS, cornerRadius, isOpenShape,
  shapeGeometry, shapePath, shapeTextBox,
  type NativeShapeKind, type ShapeRect, type ShapeGeometry,
} from '../shapes/native-geometry'
