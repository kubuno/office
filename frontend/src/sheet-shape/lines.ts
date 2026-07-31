// Moved to `shapes/lines.ts` — line/connector chrome is shared by every office
// sub-editor. Re-exported here for the spreadsheet's existing call sites.
export {
  isLineShape, lineEnds, hitLineBody, hitLineShape, drawLineChrome, type LineEnd,
} from '../shapes/lines'
