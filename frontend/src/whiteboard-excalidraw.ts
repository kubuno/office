// Excalidraw (.excalidraw JSON) import parser for the Kubuno whiteboard.
// Maps Excalidraw scene elements onto the whiteboard's WbElement / Stroke model.

import type {
  WbElement,
  Stroke,
  ShapeElement,
  TextBox,
  ArrowElement,
  FrameElement,
  ImageElement,
  ShapeKind,
  ArrowHead,
} from './whiteboard-types'

// Minimal shape of the Excalidraw element subset we consume.
interface ExElement {
  type?: string
  x?: number
  y?: number
  width?: number
  height?: number
  angle?: number
  strokeColor?: string
  backgroundColor?: string
  fillStyle?: string
  strokeWidth?: number
  opacity?: number
  roughness?: number
  points?: Array<[number, number]>
  text?: string
  fontSize?: number
  fontFamily?: number
  textAlign?: string
  startArrowhead?: string | null
  endArrowhead?: string | null
  fileId?: string
  name?: string | null
  isDeleted?: boolean
}

interface ExFile {
  dataURL?: string
}

interface ExScene {
  type?: string
  elements?: ExElement[]
  files?: Record<string, ExFile>
}

const DEFAULT_STROKE = '#1e1e1e'
const DEFAULT_FILL = 'transparent'

// Round radians to degrees for the whiteboard rotation field.
function radToDeg(angle: number | undefined): number {
  if (typeof angle !== 'number' || !isFinite(angle)) return 0
  return (angle * 180) / Math.PI
}

// Excalidraw opacity is 0-100; the whiteboard uses 0-1.
function normalizeOpacity(opacity: number | undefined): number {
  if (typeof opacity !== 'number' || !isFinite(opacity)) return 1
  const o = opacity / 100
  if (o < 0) return 0
  if (o > 1) return 1
  return o
}

// Map an Excalidraw arrowhead name onto the whiteboard's ArrowHead union.
function mapArrowhead(head: string | null | undefined, fallback: ArrowHead): ArrowHead {
  if (head === null) return 'none'
  switch (head) {
    case 'arrow':
    case 'triangle':
      return 'triangle'
    case 'bar':
    case 'open':
      return 'open'
    case 'dot':
    case 'circle':
      return 'circle'
    case 'none':
      return 'none'
    default:
      return fallback
  }
}

// Map an Excalidraw text alignment onto the whiteboard's allowed values.
function mapTextAlign(align: string | undefined): 'left' | 'center' | 'right' {
  if (align === 'center') return 'center'
  if (align === 'right') return 'right'
  return 'left'
}

export function excalidrawToWhiteboard(
  data: string | object,
  genId: () => string,
): { elements: WbElement[]; strokes: Stroke[]; error?: string } {
  const elements: WbElement[] = []
  const strokes: Stroke[] = []

  try {
    let scene: ExScene
    if (typeof data === 'string') {
      scene = JSON.parse(data) as ExScene
    } else {
      scene = data as ExScene
    }

    if (!scene || typeof scene !== 'object') {
      return { elements: [], strokes: [], error: 'Invalid Excalidraw data' }
    }

    const rawElements = scene.elements
    if (!Array.isArray(rawElements)) {
      // Accept the file if it self-identifies, otherwise reject it.
      if (scene.type === 'excalidraw') {
        return { elements: [], strokes: [] }
      }
      return { elements: [], strokes: [], error: 'No Excalidraw elements found' }
    }

    const files = scene.files || {}
    const now = Date.now()
    let zIndex = 0

    for (const el of rawElements) {
      if (!el || typeof el !== 'object' || el.isDeleted) continue

      const x = typeof el.x === 'number' ? el.x : 0
      const y = typeof el.y === 'number' ? el.y : 0
      const width = typeof el.width === 'number' ? el.width : 0
      const height = typeof el.height === 'number' ? el.height : 0
      const rotation = radToDeg(el.angle)
      const opacity = normalizeOpacity(el.opacity)
      const stroke = el.strokeColor || DEFAULT_STROKE
      const strokeWidth = typeof el.strokeWidth === 'number' ? el.strokeWidth : 1

      switch (el.type) {
        case 'rectangle':
        case 'ellipse':
        case 'diamond': {
          let kind: ShapeKind = 'rect'
          if (el.type === 'ellipse') kind = 'circle'
          else if (el.type === 'diamond') kind = 'diamond'

          // 'transparent' stays as a no-fill marker.
          const fill = el.backgroundColor && el.backgroundColor !== 'transparent'
            ? el.backgroundColor
            : DEFAULT_FILL

          const shape: ShapeElement = {
            id: genId(),
            type: 'shape',
            x,
            y,
            width,
            height,
            rotation,
            opacity,
            zIndex: zIndex++,
            locked: false,
            kind,
            fill,
            stroke,
            strokeWidth,
          }
          elements.push(shape)
          break
        }

        case 'text': {
          const textBox: TextBox = {
            id: genId(),
            type: 'text',
            x,
            y,
            width,
            height,
            rotation,
            opacity,
            zIndex: zIndex++,
            locked: false,
            text: typeof el.text === 'string' ? el.text : '',
            fontSize: typeof el.fontSize === 'number' ? el.fontSize : 20,
            fontWeight: 'normal',
            color: stroke,
            textAlign: mapTextAlign(el.textAlign),
          }
          elements.push(textBox)
          break
        }

        case 'arrow':
        case 'line': {
          const pts = Array.isArray(el.points) ? el.points : []
          const first = pts.length > 0 ? pts[0] : [0, 0]
          const last = pts.length > 0 ? pts[pts.length - 1] : [width, height]
          // Excalidraw points are relative to the element's x/y origin.
          const startX = x + (Array.isArray(first) ? (first[0] || 0) : 0)
          const startY = y + (Array.isArray(first) ? (first[1] || 0) : 0)
          const endX = x + (Array.isArray(last) ? (last[0] || 0) : 0)
          const endY = y + (Array.isArray(last) ? (last[1] || 0) : 0)

          const isArrow = el.type === 'arrow'
          const arrow: ArrowElement = {
            id: genId(),
            type: 'arrow',
            startX,
            startY,
            endX,
            endY,
            color: stroke,
            width: strokeWidth,
            style: isArrow && pts.length > 2 ? 'curved' : 'straight',
            startArrow: isArrow ? mapArrowhead(el.startArrowhead, 'none') : 'none',
            endArrow: isArrow ? mapArrowhead(el.endArrowhead, 'triangle') : 'none',
            zIndex: zIndex++,
            opacity,
          }
          elements.push(arrow)
          break
        }

        case 'freedraw': {
          const pts = Array.isArray(el.points) ? el.points : []
          const flat: number[] = []
          for (const p of pts) {
            if (Array.isArray(p)) {
              flat.push(x + (p[0] || 0), y + (p[1] || 0))
            }
          }
          if (flat.length >= 4) {
            const s: Stroke = {
              id: genId(),
              points: flat,
              color: stroke,
              width: strokeWidth,
              opacity,
              tool: 'pen',
              userId: 'import',
              createdAt: now,
            }
            strokes.push(s)
            zIndex++
          }
          break
        }

        case 'image': {
          const fileId = el.fileId
          const file = fileId ? files[fileId] : undefined
          const dataURL = file?.dataURL
          if (typeof dataURL === 'string' && dataURL) {
            const img: ImageElement = {
              id: genId(),
              type: 'image',
              x,
              y,
              width,
              height,
              rotation,
              opacity,
              zIndex: zIndex++,
              locked: false,
              src: dataURL,
              natural_width: width || 0,
              natural_height: height || 0,
            }
            elements.push(img)
          }
          // Without embedded data we cannot resolve the image; skip it.
          break
        }

        case 'frame': {
          const frame: FrameElement = {
            id: genId(),
            type: 'frame',
            x,
            y,
            width,
            height,
            rotation,
            opacity,
            zIndex: zIndex++,
            locked: false,
            title: typeof el.name === 'string' && el.name ? el.name : 'Frame',
            color: stroke,
          }
          elements.push(frame)
          break
        }

        default:
          // Unknown / unsupported Excalidraw element types are ignored.
          break
      }
    }

    return { elements, strokes }
  } catch (err) {
    return {
      elements: [],
      strokes: [],
      error: err instanceof Error ? err.message : 'Failed to parse Excalidraw data',
    }
  }
}
