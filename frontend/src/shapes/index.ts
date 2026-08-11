// THE shapes package of the office module — public surface.
//
// Everything a sub-editor needs to offer the SAME shapes, drawn the SAME way,
// with the same adjustment knobs and the same picker:
//
//   • catalogue  — the 144 geometries, grouped, with French labels;
//   • rendering  — `ShapeGlyph` (React), `shapeSvgView` (markup) or
//                  `paintShapeView` (canvas): three surfaces, ONE geometry. Never
//                  call `paintShape`/`shapeSvg` directly — the *View helpers route
//                  the two adjustment conventions (see canvas.ts) and skipping them
//                  silently flattens an adjusted shape;
//   • drawing    — `drawBoxFrom` / `finalizeDrawBox` / `paintShapeGhost`: the
//                  draw-to-create gesture, identical in every sub-editor;
//   • picking    — `ShapeGallery`;
//   • editing    — `adjustHandles` / `adjustFromDrag` / `hitAdjust` (yellow knobs),
//                  `isLineShape` & friends (lines get endpoints, not a frame).
//
// The geometry itself comes from LibreOffice's preset data (preset-engine.ts),
// so a star is the same star in a slide, a sheet, a document and a board.

export { SHAPE_CATALOG, shapeDefaultSize, type ShapeKind, type ShapeDef, type ShapeCat } from './catalog'
export { shapeSvg, type ShapeParams } from './svg'
export {
  shapePaths, shapeCanvasPaths, shapeTextBox, paintShape, shadeColour,
  hasShapeGeometry, resolveShapeKind, type ShapeSubPath,
} from './paths'
export { paintShapeView, type PaintShapeViewOpts } from './canvas'
export {
  shapeSvgView, shapeOutline, isFractionKind,
  type ShapeSvgViewOpts, type ShapeOutline,
} from './svg-view'
export {
  asNative, hasNativeGeometry, cornerRadius, isOpenShape, shapeGeometry,
  type NativeShapeKind, type ShapeRect, type ShapeGeometry,
} from './native-geometry'
export {
  drawBoxFrom, finalizeDrawBox, paintShapeGhost,
  type DrawBox, type DrawBoxOpts, type FinalizeOpts, type GhostOpts,
} from './draw'
// EXTENSION POINT — a module contributes its own geometries here and gets the whole
// pipeline (painting, SVG, yellow knobs, draw-to-create, gallery) for free.
export {
  registerShapes, getShapeProvider, isRegisteredShape, registeredShapes,
  onRegisteredShapesChange, registeredShapesVersion,
  type ShapeProvider, type ProvidedGeometry, type ProvidedSubPath,
} from './registry'
export { ShapeGlyph, type ShapeGlyphProps } from './ShapeGlyph'
export {
  ShapeGallery, SHAPE_GALLERY_GROUPS, shapeLabel,
  type ShapeGalleryProps, type ShapeGalleryGroup,
} from './ShapeGallery'
export {
  SHAPE_ADJUSTMENTS, adjValues, adjustHandles, adjustFromDrag, hitAdjust,
  type AdjustSpec, type AdjustHandle,
} from './adjust'
export {
  isLineShape, lineEnds, hitLineBody, hitLineShape, drawLineChrome, type LineEnd,
} from './lines'
export { presetOf, presetHandlePositions, presetAdjValues } from './preset-engine'
export {
  ShapesInsertButton, shapesIllustrationGroup,
  type ShapesInsertButtonProps, type ShapesIllustrationArgs,
} from './ShapesInsertButton'
export {
  shapeRibbonTab, arrangeGroup, objectGroup, styleGroup, colourItem,
  SHAPE_TAB_ACCENT, SHAPE_TAB_GROUP,
  type ShapeRibbonAdapter, type ShapeTabOverride, type ShapeAlignMode, type ShapeOrderOp,
  type ShapeStyleLike, type ArrangeToolArgs, type ObjectToolArgs, type StyleToolArgs,
} from './shapeRibbonTab'
