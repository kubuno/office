// Golden geometry recorded from the WEB shape engine (office/web/src/shapes), the reference the Rust
// port (kubuno-office-shapes-core) is tested against: tests/golden/shapes.json.
//
// Regenerate (Node + the web's node_modules, e.g. on the build host):
//   cd office/web && npx rolldown ../common/shapes-core/tools/golden.ts --format esm -o /tmp/golden.mjs \
//     && node /tmp/golden.mjs > ../common/shapes-core/tests/golden/shapes.json
import { SHAPE_CATALOG } from '../../../web/src/shapes/catalog'
import { shapePaths, shapeTextBox } from '../../../web/src/shapes/paths'
import { shapeGeometry, asNative, hasNativeGeometry } from '../../../web/src/shapes/native-geometry'
import { SHAPE_ADJUSTMENTS, adjustHandles } from '../../../web/src/shapes/adjust'

const FRACTION_KINDS = new Set(Object.keys(SHAPE_ADJUSTMENTS))
// The kinds a presentation stores (PresentationEditorPage renderShape), legacy names included.
const SLIDE_KINDS = ['rect', 'ellipse', 'triangle', 'roundRect', 'cylinder', 'star', 'pentagon', 'hexagon', 'diamond',
  'rightArrow', 'chevron', 'plus', 'speech', 'heart', 'parallelogram', 'trapezoid', 'octagon', 'leftArrow',
  'upArrow', 'downArrow', 'lightning', 'cloud', 'donut']

const kinds = new Set<string>(SLIDE_KINDS)
for (const c of SHAPE_CATALOG) for (const s of c.shapes) if (s.kind !== 'textBox') kinds.add(s.kind)

const boxes: [number, number][] = [[240, 180], [90, 300]]
const out: Record<string, unknown>[] = []
for (const kind of [...kinds].sort()) {
  for (const [w, h] of boxes) {
    for (const stroke of [0, 4]) {
      let paths: { d: string; fill: boolean; stroke: boolean; shade: number }[] | null
      if (FRACTION_KINDS.has(kind) && hasNativeGeometry(kind)) {
        paths = [{ d: shapeGeometry(asNative(kind), w, h, stroke / 2).d, fill: true, stroke: true, shade: 1 }]
      } else {
        if (stroke) continue // preset geometry ignores the stroke width
        paths = shapePaths(kind, w, h)
      }
      out.push({
        kind, w, h, stroke, paths,
        text: shapeTextBox(kind, w, h),
        handles: adjustHandles(kind, { x: 0, y: 0, w, h }),
      })
    }
  }
}
console.log(JSON.stringify(out, null, 1))
