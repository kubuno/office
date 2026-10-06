import { adjValues } from './adjust'
// Pure geometry of the office "native" drawing SHAPES — the handful the editors
// draw themselves (fraction adjustments + a caption box + an xlsx preset), as
// opposed to the 144 catalogue presets rendered by `preset-engine.ts` / `svg.ts`.
//
// This lives in `shapes/` so EVERY sub-editor shares one geometry: the sheet's
// SVG overlay, the presentations/whiteboard/diagrams canvas (via `canvas.ts`), the
// insertion gallery's thumbnails and the inline text editor all read the same path
// here, so a shape looks the same at every size and on every surface. `sheet-shape/
// geometry.ts` re-exports this module for the spreadsheet's own call sites.
//
// One function per concern, no React, no state: `shapeGeometry()` turns a
// (kind, width, height) triple into an SVG path plus the rectangle a caption may
// be laid out in.
//
// Coordinate space: the shape's OWN box, from (0, 0) to (width, height). The host
// is responsible for placing that box (and for the rotation, which is a transform
// on the box, never baked into the path).
//
// Stroke handling: an outline is centred on the path, so half of it would fall
// outside the box and get clipped by the SVG viewport. Callers therefore pass
// `inset = strokeWidth / 2` and the path is built inside that margin — the same
// trick PowerPoint uses to keep a shape flush with its selection frame.

/** The native geometries drawn HERE (the others come from the shared catalogue). */
export type NativeShapeKind =
  | 'rect' | 'roundRect' | 'ellipse' | 'triangle' | 'diamond'
  | 'arrow' | 'line' | 'star' | 'callout' | 'plus'
type SheetShapeKind = NativeShapeKind
/** A catalogue shape not drawn here falls back to the rectangle: only its TEXT
 *  FRAME and box are then used (its outline comes from shapes/svg). */
const NATIVE = new Set<string>(['rect', 'roundRect', 'ellipse', 'triangle', 'diamond', 'arrow', 'line', 'star', 'callout', 'plus'])
export const asNative = (k: string): NativeShapeKind => (NATIVE.has(k) ? k : 'rect') as NativeShapeKind
/** True when this module draws the kind itself (fraction adjustments) rather than
 *  deferring to the preset engine. Lets a canvas host pick the right renderer. */
export const hasNativeGeometry = (k: string): boolean => NATIVE.has(k)

/** Every geometry this module can draw, in gallery order. */
export const SHAPE_KINDS: readonly SheetShapeKind[] = [
  'rect', 'roundRect', 'ellipse', 'triangle', 'diamond',
  'arrow', 'line', 'star', 'callout', 'plus',
] as const

export interface ShapeRect { x: number; y: number; w: number; h: number }

export interface ShapeGeometry {
  /** SVG path data, in the shape's own [0,0]→[w,h] space. */
  d: string
  /**
   * False for OPEN geometries (`line`): they are stroked only and must never be
   * filled, and their outline is what makes them visible at all.
   */
  closed: boolean
  /** Where a caption fits inside the shape (already padded). */
  text: ShapeRect
}

// ── Preset adjustments ────────────────────────────────────────────────────────
// Named after the OOXML `a:avLst` adjustment they stand for, with the value Excel
// uses when the shape is drawn fresh, so an exported shape keeps its look.

/** `roundRect` corner radius, as a fraction of the SHORTER side (PowerPoint-like). */
const ROUND_FRAC = 0.14
/** `rightArrow` shaft thickness, as a fraction of the height (adj1 = 50000). */
const ARROW_SHAFT = 0.5
/** `rightArrow` head length, as a fraction of the shorter side (adj2 = 50000). */
const ARROW_HEAD = 0.5
/** `star5` inner/outer radius ratio (adj = 19098/50000 ≈ golden ratio²). */
const STAR_INNER = 0.382
/** `star5` spike count. */
const STAR_SPIKES = 5
/** `wedgeRectCallout`: share of the height taken by the body, the rest is the tail. */
const CALLOUT_BODY = 0.78
/** `wedgeRectCallout` tail: left edge, tip and right edge as fractions of the width. */
const CALLOUT_TAIL: readonly [number, number, number] = [0.22, 0.3, 0.42]
/** `mathPlus` arm thickness, as a fraction of the shorter side. */
const PLUS_ARM = 0.36
/** Breathing room between a caption and the shape's own edges, in px. */
const TEXT_PAD = 3

/**
 * Corner radius of a rounded rectangle of that size — proportional to the shorter
 * side the way PowerPoint does it, so the corners keep their look when the shape is
 * resized, and never collapse the shape when it gets small.
 */
export function cornerRadius(w: number, h: number, frac: number = ROUND_FRAC): number {
  const m = Math.max(0, Math.min(w, h))
  return Math.max(0, Math.min(m * frac, m / 2))
}

/** True when the geometry is a stroke rather than an outlined area. */
export function isOpenShape(kind: SheetShapeKind): boolean {
  return kind === 'line'
}

/** Two decimals is plenty at screen scale and keeps the path strings short. */
function n(v: number): string {
  return String(Math.round(v * 100) / 100)
}

/** Shrink a rect by `pad` on every side, never past emptiness. */
function padRect(r: ShapeRect, pad: number): ShapeRect {
  const dx = Math.min(pad, r.w / 2)
  const dy = Math.min(pad, r.h / 2)
  return { x: r.x + dx, y: r.y + dy, w: Math.max(0, r.w - 2 * dx), h: Math.max(0, r.h - 2 * dy) }
}

/**
 * Path + caption box of one shape.
 *
 * @param inset half the outline width, so the stroke stays inside the box.
 */
export function shapeGeometry(
  kind: SheetShapeKind,
  w: number,
  h: number,
  inset = 0,
  /** Adjustment fractions (see adjust.ts); absent = the geometry's defaults. */
  adj?: number[],
): ShapeGeometry {
  // Adjustment values: the shape's own when set, the geometry's defaults otherwise.
  const av = adjValues(kind, adj)

  const width = Math.max(0, w)
  const height = Math.max(0, h)
  // Never let the inset eat the whole box (a 1 px shape with a 4 px outline).
  const p = Math.max(0, Math.min(inset, (Math.min(width, height) - 1) / 2))
  const x0 = p, y0 = p
  const x1 = Math.max(p, width - p), y1 = Math.max(p, height - p)
  const iw = x1 - x0, ih = y1 - y0
  const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2

  /** Full inner box — the caption default. */
  const full: ShapeRect = { x: x0, y: y0, w: iw, h: ih }
  /** Caption box from fractions of the inner box. */
  const frac = (fx: number, fy: number, fw: number, fh: number): ShapeRect =>
    ({ x: x0 + iw * fx, y: y0 + ih * fy, w: iw * fw, h: ih * fh })

  let d: string
  let text: ShapeRect = full
  let closed = true

  switch (kind) {
    case 'rect':
      d = `M ${n(x0)},${n(y0)} H ${n(x1)} V ${n(y1)} H ${n(x0)} Z`
      break

    case 'roundRect': {
      const r = cornerRadius(iw, ih, av[0] ?? ROUND_FRAC)
      d = `M ${n(x0 + r)},${n(y0)} H ${n(x1 - r)} A ${n(r)},${n(r)} 0 0 1 ${n(x1)},${n(y0 + r)}`
        + ` V ${n(y1 - r)} A ${n(r)},${n(r)} 0 0 1 ${n(x1 - r)},${n(y1)}`
        + ` H ${n(x0 + r)} A ${n(r)},${n(r)} 0 0 1 ${n(x0)},${n(y1 - r)}`
        + ` V ${n(y0 + r)} A ${n(r)},${n(r)} 0 0 1 ${n(x0 + r)},${n(y0)} Z`
      // The corners bite into the caption box only slightly.
      text = padRect(full, r * 0.25)
      break
    }

    case 'ellipse': {
      const rx = iw / 2, ry = ih / 2
      d = `M ${n(x0)},${n(cy)} A ${n(rx)},${n(ry)} 0 0 1 ${n(x1)},${n(cy)}`
        + ` A ${n(rx)},${n(ry)} 0 0 1 ${n(x0)},${n(cy)} Z`
      // Largest rectangle inscribed in an ellipse: half-sides scaled by 1/√2.
      text = frac(0.1464, 0.1464, 0.7072, 0.7072)
      break
    }

    case 'triangle':
      d = `M ${n(cx)},${n(y0)} L ${n(x1)},${n(y1)} L ${n(x0)},${n(y1)} Z`
      // Lower middle band: the highest rectangle whose top corners still clear the
      // two slopes, so a caption never spills outside the triangle.
      text = frac(0.28, 0.46, 0.44, 0.54)
      break

    case 'diamond':
      d = `M ${n(cx)},${n(y0)} L ${n(x1)},${n(cy)} L ${n(cx)},${n(y1)} L ${n(x0)},${n(cy)} Z`
      text = frac(0.25, 0.25, 0.5, 0.5)
      break

    case 'arrow': {
      const head = Math.min(iw, Math.min(iw, ih) * (av[1] ?? ARROW_HEAD))
      const shaft = ih * (av[0] ?? ARROW_SHAFT)
      const hx = x1 - head
      const sy0 = cy - shaft / 2, sy1 = cy + shaft / 2
      d = `M ${n(x0)},${n(sy0)} L ${n(hx)},${n(sy0)} L ${n(hx)},${n(y0)} L ${n(x1)},${n(cy)}`
        + ` L ${n(hx)},${n(y1)} L ${n(hx)},${n(sy1)} L ${n(x0)},${n(sy1)} Z`
      // Inside the shaft only — text must not spill over the head.
      text = { x: x0, y: sy0, w: Math.max(0, hx - x0), h: Math.max(0, sy1 - sy0) }
      break
    }

    case 'line':
      // Bottom-left to top-right, the orientation a fresh connector takes.
      d = `M ${n(x0)},${n(y1)} L ${n(x1)},${n(y0)}`
      closed = false
      break

    case 'star': {
      const rx = iw / 2, ry = ih / 2
      const pts: string[] = []
      for (let i = 0; i < STAR_SPIKES * 2; i++) {
        const r = i % 2 === 0 ? 1 : (av[0] ?? STAR_INNER)
        const a = (Math.PI / STAR_SPIKES) * i - Math.PI / 2
        pts.push(`${n(cx + rx * r * Math.cos(a))},${n(cy + ry * r * Math.sin(a))}`)
      }
      d = `M ${pts.join(' L ')} Z`
      // The star's body around its centre. The exact inscribed rectangle would be
      // ~0.36 × 0.35 — too cramped for a caption — so the box leans a little into
      // the arms, the way PowerPoint's own star text rectangle does.
      text = frac(0.24, 0.32, 0.52, 0.4)
      break
    }

    case 'callout': {
      const by = y0 + ih * (av[0] ?? CALLOUT_BODY)
      const tipF = av[1] ?? CALLOUT_TAIL[1]
      const half = (CALLOUT_TAIL[2] - CALLOUT_TAIL[0]) / 2
      const [tl, tip, tr] = [Math.max(0, tipF - half), tipF, Math.min(1, tipF + half)]
      d = `M ${n(x0)},${n(y0)} H ${n(x1)} V ${n(by)} H ${n(x0 + iw * tr)}`
        + ` L ${n(x0 + iw * tip)},${n(y1)} L ${n(x0 + iw * tl)},${n(by)} H ${n(x0)} Z`
      // The body only; the tail is too narrow to hold text.
      text = { x: x0, y: y0, w: iw, h: Math.max(0, by - y0) }
      break
    }

    case 'plus': {
      const arm = Math.min(iw, ih) * (av[0] ?? PLUS_ARM)
      const ax0 = cx - arm / 2, ax1 = cx + arm / 2
      const ay0 = cy - arm / 2, ay1 = cy + arm / 2
      d = `M ${n(ax0)},${n(y0)} H ${n(ax1)} V ${n(ay0)} H ${n(x1)} V ${n(ay1)} H ${n(ax1)}`
        + ` V ${n(y1)} H ${n(ax0)} V ${n(ay1)} H ${n(x0)} V ${n(ay0)} H ${n(ax0)} Z`
      // The horizontal bar — the only band that spans the whole width.
      text = { x: x0, y: ay0, w: iw, h: Math.max(0, ay1 - ay0) }
      break
    }

    default: {
      // Exhaustiveness guard: a new kind in `SheetShapeKind` must be drawn here.
      const never: never = kind
      throw new Error(`unhandled shape kind: ${String(never)}`)
    }
  }

  return { d, closed, text: padRect(text, TEXT_PAD) }
}

/** Just the path data — for callers that do not care about the caption box. */
export function shapePath(kind: SheetShapeKind, w: number, h: number, inset = 0): string {
  return shapeGeometry(kind, w, h, inset).d
}

/** Just the caption box — used by the view and the inline text editor alike. */
export function shapeTextBox(kind: SheetShapeKind, w: number, h: number, inset = 0): ShapeRect {
  return shapeGeometry(kind, w, h, inset).text
}
