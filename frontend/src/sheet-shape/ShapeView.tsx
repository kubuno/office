// Presentational rendering of one shape. Props in, SVG out — no state, no store,
// no knowledge of the grid: the host sizes and places the box, this draws inside it.
//
// Two levels:
//   • `ShapeGlyph` — the bare <svg> outline (geometry + fill + stroke). Used on its
//     own by the insertion gallery's thumbnails.
//   • `ShapeView`  — the glyph plus the caption laid out in the shape's own text
//     box, with the run-wide formatting `SheetShapeTextStyle` carries.
//
// The caption is HTML in a sibling layer rather than SVG <text>: real line wrapping
// and vertical centring come for free, the inline editor can sit on the very same
// box with the very same typography (so text never jumps when editing starts), and
// no <foreignObject> quirk is involved.

import type { CSSProperties } from 'react'
import type { SheetShapeKind, SheetShapeTextStyle } from '../api'
import { shapeSvg } from '../shapes/svg'
import { shapeGeometry, isOpenShape, asNative } from './geometry'
import { adjValues } from './adjust'
import { presetOf, presetTextFrame, presetAdjValues } from '../shapes/preset-engine'
import { DEFAULT_SHAPE_STYLE, SHAPE_ACCENT } from './defaults'

/** Caption size when the shape does not specify one, in pt (Excel's shape default). */
export const DEFAULT_SHAPE_FONT_PT = 11
/** pt → px. */
const PT_TO_PX = 4 / 3

export interface ShapeGlyphProps {
  /** Adjustment fractions (yellow knobs). */
  adj?: number[]
  kind: SheetShapeKind
  /** Rendered size in px — already zoomed by the host. */
  width: number
  height: number
  /** "#RRGGBB" or 'none'; falls back to the factory default when undefined. */
  fill?: string
  /** "#RRGGBB" or 'none'; falls back to the factory default when undefined. */
  border?: string
  /** Outline width in px, BEFORE zoom; pass `strokeScale` to zoom it. */
  borderWidth?: number
  /** Multiplies the outline width (the host's zoom). */
  strokeScale?: number
  /** Accessible name — the shape's alt text. */
  title?: string
  className?: string
  style?: CSSProperties
}

/** Resolved paint of a shape: what actually reaches the SVG attributes. */
function paintOf(p: ShapeGlyphProps): { fill: string; stroke: string; sw: number } {
  const open = isOpenShape(asNative(p.kind))
  const rawFill = p.fill ?? DEFAULT_SHAPE_STYLE.fill
  const rawBorder = p.border ?? DEFAULT_SHAPE_STYLE.border
  const scale = p.strokeScale ?? 1
  const width = Math.max(0, p.borderWidth ?? DEFAULT_SHAPE_STYLE.borderWidth) * scale

  if (open) {
    // A line IS its outline: a zero width or a 'none' colour would render nothing
    // at all, so both fall back — the shape stays visible and selectable.
    return {
      fill: 'none',
      stroke: rawBorder === 'none' ? (rawFill === 'none' ? SHAPE_ACCENT : rawFill) : rawBorder,
      sw: Math.max(1 * scale, width),
    }
  }
  return {
    fill: rawFill === 'none' ? 'none' : rawFill,
    stroke: rawBorder === 'none' ? 'none' : rawBorder,
    sw: rawBorder === 'none' ? 0 : width,
  }
}

/** The shape's outline alone, as an <svg> exactly `width` × `height`. */
/** Geometries the sheet draws itself (they carry a caption box and an xlsx preset). */
const NATIVE_KINDS = new Set(['rect', 'roundRect', 'ellipse', 'triangle', 'diamond', 'arrow', 'line', 'star', 'callout', 'plus'])

export function ShapeGlyph(props: ShapeGlyphProps) {
  const { kind, width, height, title, className, style, adj } = props
  const { fill, stroke, sw } = paintOf(props)
  // The ten native geometries keep the sheet's own renderer (caption box, xlsx
  // presets); every other kind comes from the catalogue SHARED with the documents
  // editor — 144 shapes in all, instead of maintaining a second, poorer set.
  if (!NATIVE_KINDS.has(kind)) {
    const svg = shapeSvg(kind as never, Math.max(0, width), Math.max(0, height), fill, stroke, undefined, adjValues(kind, adj))
    return (
      <span
        className={className}
        style={{ display: 'block', lineHeight: 0, ...style }}
        role={title ? 'img' : 'presentation'}
        aria-label={title || undefined}
        {...(title ? { title } : {})}
        dangerouslySetInnerHTML={{ __html: svg }}
      />
    )
  }
  const { d } = shapeGeometry(asNative(kind), width, height, sw / 2, adj)

  return (
    <svg
      width={Math.max(0, width)}
      height={Math.max(0, height)}
      viewBox={`0 0 ${Math.max(0, width)} ${Math.max(0, height)}`}
      className={className}
      style={{ display: 'block', overflow: 'visible', ...style }}
      role={title ? 'img' : 'presentation'}
      aria-label={title || undefined}
      focusable="false"
      // Attribute, never an SVG <title> child: the shell upgrades `title`
      // attributes to the project tooltip, a child renders the browser's grey one.
      {...(title ? { title } : {})}
    >
      <path
        d={d}
        fill={fill}
        stroke={stroke}
        strokeWidth={sw}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  )
}

/**
 * CSS of a shape's caption. Exported so the inline editor renders the text it is
 * about to replace with pixel-identical metrics.
 */
export function shapeTextCss(ts: SheetShapeTextStyle | undefined, scale = 1): CSSProperties {
  return {
    fontWeight: ts?.bold ? 700 : 400,
    fontStyle: ts?.italic ? 'italic' : 'normal',
    // Theme-aware default: follows the host's text colour in light and dark alike.
    color: ts?.color ?? 'var(--color-text-primary, #202124)',
    fontSize: (ts?.size ?? DEFAULT_SHAPE_FONT_PT) * PT_TO_PX * scale,
    lineHeight: 1.25,
    textAlign: ts?.align ?? 'center',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    // Visible, not hidden: a caption too long for its shape SPILLS the way Excel's
    // does. Clipping it would silently swallow text the user typed.
    overflow: 'visible',
  }
}

/** Flexbox alignment matching a text style's horizontal alignment. */
export function shapeTextJustify(ts: SheetShapeTextStyle | undefined): CSSProperties['justifyContent'] {
  switch (ts?.align ?? 'center') {
    case 'left': return 'flex-start'
    case 'right': return 'flex-end'
    default: return 'center'
  }
}

export interface ShapeViewProps extends ShapeGlyphProps {
  /** Caption drawn inside the shape. */
  text?: string
  textStyle?: SheetShapeTextStyle
  /** Hide the caption — set while the inline editor covers the shape. */
  hideText?: boolean
  /** Multiplies the caption's font size (the host's zoom). */
  textScale?: number
  /** Mirror the outline (LibreOffice's Flip); the caption stays readable. */
  flipH?: boolean
  flipV?: boolean
}

/**
 * A shape with its caption: a box of exactly `width` × `height` holding the glyph
 * and, on top of it, the text laid out in the geometry's own caption rectangle.
 *
 * While the user edits the text, the host renders `<ShapeTextEditor>` as a SIBLING
 * (both are absolutely placed inside the host's shape overlay) and passes
 * `hideText` here, so the caption is drawn exactly once.
 */
export function ShapeView(props: ShapeViewProps) {
  const { text, textStyle, hideText, textScale, className, style, flipH, flipV, ...glyph } = props
  const adj = glyph.adj
  const { kind, width, height } = glyph
  const { sw } = paintOf(glyph)
  // Preset-backed kinds carry their own text frame in LibreOffice's data; the
  // native ten keep the sheet's hand-tuned caption boxes.
  const preset = NATIVE_KINDS.has(kind) ? null : presetOf(kind)
  const box = preset
    ? presetTextFrame(preset, { w: width, h: height, adj: presetAdjValues(preset, adj) })
    : shapeGeometry(asNative(kind), width, height, sw / 2, adj).text
  const showText = !hideText && !!text

  return (
    <div
      className={className}
      style={{ position: 'relative', width: Math.max(0, width), height: Math.max(0, height), ...style }}
    >
      {/* Mirroring applies to the OUTLINE only: LibreOffice flips the geometry but
          keeps the caption readable, and a mirrored caption would be unusable. */}
      <ShapeGlyph
        {...glyph}
        style={{
          position: 'absolute', inset: 0,
          transform: flipH || flipV ? `scale(${flipH ? -1 : 1}, ${flipV ? -1 : 1})` : undefined,
        }}
      />
      {showText && (
        <div
          style={{
            position: 'absolute',
            left: box.x, top: box.y, width: box.w, height: box.h,
            display: 'flex',
            alignItems: 'center',
            justifyContent: shapeTextJustify(textStyle),
            // The caption is decoration: dragging or right-clicking a shape must
            // reach the host's hit test, and must not start a text selection.
            pointerEvents: 'none',
            userSelect: 'none',
          }}
        >
          <div style={{ width: '100%', ...shapeTextCss(textStyle, textScale ?? 1) }}>
            {text}
          </div>
        </div>
      )}
    </div>
  )
}
