// THE shapes package of the office module — public surface.
//
// Everything a sub-editor needs to offer the SAME shapes, drawn the SAME way,
// with the same adjustment knobs and the same picker:
//
//   • catalogue  — the 144 geometries, grouped, with French labels;
//   • rendering  — `ShapeGlyph` (React/SVG) or `paintShape` (canvas), one engine;
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
