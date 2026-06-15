import type { WbElement, StickyNote, TextBox, ShapeElement, ArrowElement, FrameElement, Stroke } from './whiteboard-types'
import { STICKY_COLORS } from './whiteboard-types'

// ── Viewport ──────────────────────────────────────────────────────────────────

export class Viewport {
  private _scale = 1.0
  private _tx    = 0
  private _ty    = 0

  readonly MIN_ZOOM = 0.05
  readonly MAX_ZOOM = 20.0

  apply(ctx: CanvasRenderingContext2D) {
    ctx.setTransform(this._scale, 0, 0, this._scale, this._tx, this._ty)
  }

  reset(ctx: CanvasRenderingContext2D) {
    ctx.setTransform(1, 0, 0, 1, 0, 0)
  }

  screenToCanvas(sx: number, sy: number) {
    return { x: (sx - this._tx) / this._scale, y: (sy - this._ty) / this._scale }
  }

  canvasToScreen(cx: number, cy: number) {
    return { x: cx * this._scale + this._tx, y: cy * this._scale + this._ty }
  }

  pan(dx: number, dy: number) {
    this._tx += dx
    this._ty += dy
  }

  zoomAt(factor: number, cx: number, cy: number) {
    const ns    = Math.max(this.MIN_ZOOM, Math.min(this.MAX_ZOOM, this._scale * factor))
    const ratio = ns / this._scale
    this._tx    = cx - ratio * (cx - this._tx)
    this._ty    = cy - ratio * (cy - this._ty)
    this._scale = ns
  }

  setZoom(z: number) {
    this._scale = Math.max(this.MIN_ZOOM, Math.min(this.MAX_ZOOM, z))
  }

  getBounds(w: number, h: number) {
    const tl = this.screenToCanvas(0, 0)
    const br = this.screenToCanvas(w, h)
    return { x: tl.x, y: tl.y, width: br.x - tl.x, height: br.y - tl.y }
  }

  fitToElements(elements: WbElement[], cw: number, ch: number) {
    const positioned = elements.filter(e => 'width' in e) as (WbElement & { x: number; y: number; width: number; height: number })[]
    if (positioned.length === 0) return
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
    for (const e of positioned) {
      minX = Math.min(minX, e.x)
      minY = Math.min(minY, e.y)
      maxX = Math.max(maxX, e.x + e.width)
      maxY = Math.max(maxY, e.y + e.height)
    }
    const bw = maxX - minX || 400, bh = maxY - minY || 400
    const PAD = 80
    this._scale = Math.min((cw - PAD * 2) / bw, (ch - PAD * 2) / bh, this.MAX_ZOOM)
    this._tx    = (cw - bw * this._scale) / 2 - minX * this._scale
    this._ty    = (ch - bh * this._scale) / 2 - minY * this._scale
  }

  get scale()          { return this._scale }
  get tx()             { return this._tx }
  get ty()             { return this._ty }
  get zoomPercent()    { return Math.round(this._scale * 100) }
}

// ── Helpers de rendu ──────────────────────────────────────────────────────────

function wrapText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): string[] {
  const lines: string[] = []
  for (const para of text.split('\n')) {
    if (ctx.measureText(para).width <= maxWidth) {
      lines.push(para)
      continue
    }
    const words = para.split(' ')
    let cur = ''
    for (const w of words) {
      const test = cur ? `${cur} ${w}` : w
      if (ctx.measureText(test).width <= maxWidth) {
        cur = test
      } else {
        if (cur) lines.push(cur)
        cur = w
      }
    }
    if (cur) lines.push(cur)
  }
  return lines
}

// ── Renderers ─────────────────────────────────────────────────────────────────

export function renderStickyNote(ctx: CanvasRenderingContext2D, note: StickyNote, zoom: number) {
  const { x, y, width, height, color, text, fontSize = 14 } = note
  const FOLD = 14, PAD = 10

  ctx.save()
  ctx.shadowColor = 'rgba(0,0,0,0.15)'
  ctx.shadowBlur  = 6
  ctx.shadowOffsetY = 2
  ctx.fillStyle   = STICKY_COLORS[color] ?? color

  ctx.beginPath()
  ctx.moveTo(x, y)
  ctx.lineTo(x + width - FOLD, y)
  ctx.lineTo(x + width, y + FOLD)
  ctx.lineTo(x + width, y + height)
  ctx.lineTo(x, y + height)
  ctx.closePath()
  ctx.fill()
  ctx.restore()

  // Coin replié
  ctx.save()
  ctx.fillStyle = 'rgba(0,0,0,0.1)'
  ctx.beginPath()
  ctx.moveTo(x + width - FOLD, y)
  ctx.lineTo(x + width, y + FOLD)
  ctx.lineTo(x + width - FOLD, y + FOLD)
  ctx.closePath()
  ctx.fill()
  ctx.restore()

  if (!text) return
  const efs = Math.max(fontSize, 7 / zoom)
  ctx.save()
  ctx.fillStyle   = color === 'dark' ? '#ffffff' : '#1a1a1a'
  ctx.font        = `${efs}px Arial, sans-serif`
  ctx.textBaseline = 'top'
  const lines = wrapText(ctx, text, width - PAD * 2)
  const lh    = efs * 1.4
  lines.forEach((line, i) => {
    const ly = y + PAD + i * lh
    if (ly + lh > y + height - PAD) return
    ctx.fillText(line, x + PAD, ly)
  })
  ctx.restore()
}

export function renderTextBox(ctx: CanvasRenderingContext2D, tb: TextBox, zoom: number) {
  if (!tb.text) return
  const efs = Math.max(tb.fontSize ?? 16, 6 / zoom)
  ctx.save()
  ctx.fillStyle    = tb.color ?? '#202124'
  ctx.font         = `${tb.fontWeight === 'bold' ? 'bold ' : ''}${efs}px Arial, sans-serif`
  ctx.textBaseline = 'top'
  ctx.textAlign    = tb.textAlign ?? 'left'
  const lines = wrapText(ctx, tb.text, tb.width)
  const lh    = efs * 1.4
  const ox    = tb.textAlign === 'center' ? tb.x + tb.width / 2 : tb.textAlign === 'right' ? tb.x + tb.width : tb.x
  lines.forEach((line, i) => {
    ctx.fillText(line, ox, tb.y + i * lh)
  })
  ctx.restore()
}

export function renderShape(ctx: CanvasRenderingContext2D, shape: ShapeElement) {
  const { x, y, width: w, height: h, kind, fill, stroke, strokeWidth = 2 } = shape
  ctx.save()
  ctx.fillStyle   = fill ?? '#BBDEFB'
  ctx.strokeStyle = stroke ?? '#1a73e8'
  ctx.lineWidth   = strokeWidth

  ctx.beginPath()
  switch (kind) {
    case 'rect':
      ctx.roundRect(x, y, w, h, 6)
      break
    case 'circle':
      ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2)
      break
    case 'triangle':
      ctx.moveTo(x + w / 2, y)
      ctx.lineTo(x + w, y + h)
      ctx.lineTo(x, y + h)
      ctx.closePath()
      break
    case 'diamond':
      ctx.moveTo(x + w / 2, y)
      ctx.lineTo(x + w, y + h / 2)
      ctx.lineTo(x + w / 2, y + h)
      ctx.lineTo(x, y + h / 2)
      ctx.closePath()
      break
    case 'star': {
      const cx = x + w / 2, cy = y + h / 2, outerR = Math.min(w, h) / 2, innerR = outerR * 0.4, pts = 5
      for (let i = 0; i < pts * 2; i++) {
        const r = i % 2 === 0 ? outerR : innerR
        const a = (i * Math.PI) / pts - Math.PI / 2
        i === 0 ? ctx.moveTo(cx + r * Math.cos(a), cy + r * Math.sin(a)) : ctx.lineTo(cx + r * Math.cos(a), cy + r * Math.sin(a))
      }
      ctx.closePath()
      break
    }
  }
  ctx.fill()
  if (strokeWidth > 0) ctx.stroke()
  ctx.restore()
}

export function renderArrow(ctx: CanvasRenderingContext2D, arrow: ArrowElement, elements: Map<string, WbElement>) {
  const getCenter = (id: string) => {
    const el = elements.get(id)
    if (!el || !('width' in el)) return null
    return { x: (el as StickyNote).x + (el as StickyNote).width / 2, y: (el as StickyNote).y + (el as StickyNote).height / 2 }
  }

  const start = arrow.startElementId ? (getCenter(arrow.startElementId) ?? { x: arrow.startX, y: arrow.startY }) : { x: arrow.startX, y: arrow.startY }
  const end   = arrow.endElementId   ? (getCenter(arrow.endElementId)   ?? { x: arrow.endX,   y: arrow.endY   }) : { x: arrow.endX,   y: arrow.endY   }

  ctx.save()
  ctx.strokeStyle = arrow.color ?? '#444746'
  ctx.lineWidth   = arrow.width ?? 2
  ctx.lineCap     = 'round'
  ctx.lineJoin    = 'round'

  ctx.beginPath()
  if (arrow.style === 'curved') {
    const dx = end.x - start.x
    ctx.moveTo(start.x, start.y)
    ctx.bezierCurveTo(start.x + dx * 0.4, start.y, end.x - dx * 0.4, end.y, end.x, end.y)
  } else {
    ctx.moveTo(start.x, start.y)
    ctx.lineTo(end.x, end.y)
  }
  ctx.stroke()

  // Pointe de flèche
  if (arrow.endArrow !== 'none') {
    const angle = Math.atan2(end.y - start.y, end.x - start.x)
    const size  = (arrow.width ?? 2) * 4 + 6
    const sp    = Math.PI / 6
    ctx.fillStyle = arrow.color ?? '#444746'
    ctx.beginPath()
    ctx.moveTo(end.x, end.y)
    ctx.lineTo(end.x - size * Math.cos(angle - sp), end.y - size * Math.sin(angle - sp))
    ctx.lineTo(end.x - size * Math.cos(angle + sp), end.y - size * Math.sin(angle + sp))
    ctx.closePath()
    ctx.fill()
  }
  ctx.restore()
}

export function renderFrame(ctx: CanvasRenderingContext2D, frame: FrameElement, zoom: number) {
  const { x, y, width: w, height: h, title, color = '#1a73e8' } = frame
  ctx.save()

  // Bordure
  ctx.strokeStyle = color
  ctx.lineWidth   = Math.max(2 / zoom, 1.5)
  ctx.setLineDash([8 / zoom, 4 / zoom])
  ctx.strokeRect(x, y, w, h)
  ctx.setLineDash([])

  // Titre
  const fs = Math.max(13 / zoom, 10)
  ctx.fillStyle = color
  ctx.font      = `bold ${fs}px Arial, sans-serif`
  ctx.textBaseline = 'bottom'
  ctx.fillText(title, x + 4 / zoom, y - 4 / zoom)
  ctx.restore()
}

export function renderStroke(ctx: CanvasRenderingContext2D, stroke: Stroke) {
  const pts = stroke.points
  if (pts.length < 4) return
  ctx.save()
  ctx.strokeStyle = stroke.color ?? '#202124'
  ctx.lineWidth   = stroke.width ?? 2
  ctx.lineCap     = 'round'
  ctx.lineJoin    = 'round'
  ctx.globalAlpha = stroke.opacity ?? 1
  ctx.beginPath()
  ctx.moveTo(pts[0], pts[1])
  for (let i = 2; i < pts.length; i += 2) {
    ctx.lineTo(pts[i], pts[i + 1])
  }
  ctx.stroke()
  ctx.restore()
}

export function renderSelectionHandles(ctx: CanvasRenderingContext2D, el: WbElement, zoom: number) {
  if (!('width' in el)) return
  const e = el as StickyNote
  const PAD = 4 / zoom, HS = 7 / zoom
  const x = e.x - PAD, y = e.y - PAD, w = e.width + PAD * 2, h = e.height + PAD * 2

  ctx.save()
  ctx.strokeStyle = '#1a73e8'
  ctx.lineWidth   = 1.5 / zoom
  ctx.setLineDash([4 / zoom, 2 / zoom])
  ctx.strokeRect(x, y, w, h)
  ctx.setLineDash([])

  ctx.fillStyle   = '#ffffff'
  ctx.strokeStyle = '#1a73e8'
  ctx.lineWidth   = 1.5 / zoom
  for (const [hx, hy] of [
    [x, y], [x + w / 2, y], [x + w, y],
    [x, y + h / 2],         [x + w, y + h / 2],
    [x, y + h], [x + w / 2, y + h], [x + w, y + h],
  ]) {
    ctx.fillRect(hx - HS / 2, hy - HS / 2, HS, HS)
    ctx.strokeRect(hx - HS / 2, hy - HS / 2, HS, HS)
  }
  ctx.restore()
}

export type ResizeHandle = 'nw' | 'n' | 'ne' | 'w' | 'e' | 'sw' | 's' | 'se'

// Renvoie la poignée de redimensionnement sous le point (coords canvas), ou null.
// Même géométrie que renderSelectionHandles ; zone de préhension généreuse.
export function hitHandle(x: number, y: number, el: WbElement, zoom: number): ResizeHandle | null {
  if (!('width' in el)) return null
  const e = el as StickyNote
  const PAD = 4 / zoom, TOL = 8 / zoom
  const bx = e.x - PAD, by = e.y - PAD, bw = e.width + PAD * 2, bh = e.height + PAD * 2
  const pts: [ResizeHandle, number, number][] = [
    ['nw', bx, by], ['n', bx + bw / 2, by], ['ne', bx + bw, by],
    ['w', bx, by + bh / 2], ['e', bx + bw, by + bh / 2],
    ['sw', bx, by + bh], ['s', bx + bw / 2, by + bh], ['se', bx + bw, by + bh],
  ]
  for (const [h, hx, hy] of pts) {
    if (Math.abs(x - hx) <= TOL && Math.abs(y - hy) <= TOL) return h
  }
  return null
}

// Curseur CSS adapté à la poignée.
export function handleCursor(h: ResizeHandle): string {
  if (h === 'n' || h === 's') return 'ns-resize'
  if (h === 'e' || h === 'w') return 'ew-resize'
  if (h === 'nw' || h === 'se') return 'nwse-resize'
  return 'nesw-resize'
}

export function renderBackground(ctx: CanvasRenderingContext2D, type: string, viewport: Viewport, cw: number, ch: number) {
  if (type === 'white') return
  const zoom   = viewport.scale
  const GRID   = 24
  const bounds = viewport.getBounds(cw, ch)
  const sx     = Math.floor(bounds.x / GRID) * GRID
  const sy     = Math.floor(bounds.y / GRID) * GRID

  ctx.save()
  viewport.apply(ctx)
  ctx.fillStyle   = '#b8bcc8'
  ctx.strokeStyle = '#e0e0e0'
  ctx.lineWidth   = 0.5 / zoom

  if (type === 'dots') {
    const r = Math.max(0.5, 1.2 / zoom)
    for (let x = sx; x < bounds.x + bounds.width + GRID; x += GRID) {
      for (let y = sy; y < bounds.y + bounds.height + GRID; y += GRID) {
        ctx.beginPath()
        ctx.arc(x, y, r, 0, Math.PI * 2)
        ctx.fill()
      }
    }
  } else if (type === 'grid') {
    for (let x = sx; x < bounds.x + bounds.width + GRID; x += GRID) {
      ctx.beginPath(); ctx.moveTo(x, bounds.y); ctx.lineTo(x, bounds.y + bounds.height); ctx.stroke()
    }
    for (let y = sy; y < bounds.y + bounds.height + GRID; y += GRID) {
      ctx.beginPath(); ctx.moveTo(bounds.x, y); ctx.lineTo(bounds.x + bounds.width, y); ctx.stroke()
    }
  } else if (type === 'lines') {
    for (let y = sy; y < bounds.y + bounds.height + GRID; y += GRID) {
      ctx.beginPath(); ctx.moveTo(bounds.x, y); ctx.lineTo(bounds.x + bounds.width, y); ctx.stroke()
    }
  }
  ctx.restore()
}

// ── Hit testing simple ────────────────────────────────────────────────────────

export function hitTest(x: number, y: number, elements: WbElement[], TOL = 4): string | null {
  // Parcourir en ordre inverse pour prendre le dessus
  for (let i = elements.length - 1; i >= 0; i--) {
    const el = elements[i]
    if (!('width' in el)) continue
    const e = el as StickyNote
    if (x >= e.x - TOL && x <= e.x + e.width + TOL && y >= e.y - TOL && y <= e.y + e.height + TOL) {
      return e.id
    }
  }
  return null
}
