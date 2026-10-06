// THE shape component of the office module — one geometry, one renderer, used by
// the spreadsheet, documents, presentations, whiteboard and diagrams.
//
// Props in, SVG out: no state, no store, no knowledge of the host. The caller
// decides where the box is and how big; this draws the geometry inside it, with
// the fill, outline, adjustments (yellow-knob values), rotation and flips it is
// given. Editors that paint on a CANVAS use `paintShapeView()` from `canvas.ts`
// instead — same geometry, same adjustments, different output surface.

import type { CSSProperties } from 'react'
import { shapeSvgView } from './svg-view'

export interface ShapeGlyphProps {
  /** Geometry name — a catalogue kind, or a legacy name the aliases resolve. */
  kind: string
  width: number
  height: number
  /** "#RRGGBB" or 'none'. */
  fill?: string
  /** Outline colour, "#RRGGBB" or 'none'. */
  stroke?: string
  /** Outline width in px (0 or a 'none' stroke = no outline). */
  strokeWidth?: number
  /** Adjustment values (OOXML `avLst`); absent = the geometry's own defaults. */
  adj?: number[]
  /** Degrees, clockwise, around the box centre. */
  rotation?: number
  flipH?: boolean
  flipV?: boolean
  /** Tooltip text; rendered as an ATTRIBUTE so the shell's own tooltip picks it up. */
  title?: string
  className?: string
  style?: CSSProperties
}

/**
 * The shape's outline as an `<svg>` exactly `width` × `height`.
 *
 * Rotation and flips are applied by a CSS transform rather than baked into the
 * path: the geometry stays the canonical one (so exports, hit-testing and the
 * adjustment knobs all agree), and the browser composites the transform.
 */
export function ShapeGlyph({
  kind, width, height,
  fill = '#ffffff', stroke = '#000000', strokeWidth = 1,
  adj, rotation, flipH, flipV, title, className, style,
}: ShapeGlyphProps) {
  const w = Math.max(0, width), h = Math.max(0, height)
  const paintStroke = stroke === 'none' || strokeWidth <= 0 ? 'none' : stroke
  // Through `shapeSvgView`, never `shapeSvg`: it is the router that tells the two
  // adjustment conventions apart AND that knows the geometries modules registered.
  // Calling the raw renderer here drew an adjusted rounded rectangle with square
  // corners, and nothing at all for a contributed shape.
  const svg = shapeSvgView(kind, w, h, {
    fill, stroke: paintStroke, strokeWidth: paintStroke === 'none' ? 0 : strokeWidth, adj,
  })

  const transforms: string[] = []
  if (rotation) transforms.push(`rotate(${rotation}deg)`)
  if (flipH) transforms.push('scaleX(-1)')
  if (flipV) transforms.push('scaleY(-1)')

  return (
    <span
      className={className}
      style={{
        display: 'block',
        lineHeight: 0,
        ...(transforms.length ? { transform: transforms.join(' '), transformOrigin: 'center center' } : null),
        ...style,
      }}
      role={title ? 'img' : 'presentation'}
      aria-label={title || undefined}
      {...(title ? { title } : {})}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  )
}
