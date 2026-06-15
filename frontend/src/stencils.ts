// Stencil registry — all diagram shape definitions for the office diagrams sub-module
import { HW_STENCILS, drawHwImage } from './hardwareIcons'

export interface ShapeStyle {
  fillColor:   string
  strokeColor: string
  strokeWidth: number
  strokeStyle: 'solid' | 'dashed' | 'dotted'
  opacity:     number
  shadow:      boolean
  rounded:     number
}

export interface StencilDef {
  id:       string
  name:     string
  category: string
  defaultW: number
  defaultH: number
  style:    Partial<ShapeStyle>
}

const DEF_STYLE: ShapeStyle = {
  fillColor:   '#dae8fc',
  strokeColor: '#6c8ebf',
  strokeWidth: 1.5,
  strokeStyle: 'solid',
  opacity:     100,
  shadow:      false,
  rounded:     0,
}

export function mergeStyle(partial: Partial<ShapeStyle>): ShapeStyle {
  return { ...DEF_STYLE, ...partial }
}

// ── Shape rendering on canvas ─────────────────────────────────────────────────

export function renderShape(
  ctx:   CanvasRenderingContext2D,
  type:  string,
  x: number, y: number, w: number, h: number,
  style: ShapeStyle,
  label: string,
  labelStyle: { fontFamily: string; fontSize: number; bold: boolean; italic: boolean; color: string; align: string; verticalAlign: string },
) {
  ctx.save()
  if (style.opacity < 100) ctx.globalAlpha = style.opacity / 100
  if (style.shadow) {
    ctx.shadowColor = 'rgba(0,0,0,0.25)'; ctx.shadowBlur = 5; ctx.shadowOffsetX = 2; ctx.shadowOffsetY = 2
  }

  const fill   = style.fillColor
  const stroke = style.strokeColor
  const lw     = style.strokeWidth
  const dash   = style.strokeStyle === 'dashed' ? [6, 3] : style.strokeStyle === 'dotted' ? [2, 3] : []

  ctx.setLineDash(dash)
  ctx.lineWidth = lw

  switch (type) {
    case 'rect':
    case 'flow_process':
      drawRect(ctx, x, y, w, h, 0, fill, stroke)
      break
    case 'rounded_rect':
      drawRect(ctx, x, y, w, h, style.rounded || 10, fill, stroke)
      break
    case 'ellipse':
    case 'flow_start':
    case 'uml_initial':
    case 'uml_usecase':
      drawEllipse(ctx, x, y, w, h, fill, stroke)
      break
    case 'diamond':
    case 'flow_decision':
      drawDiamond(ctx, x, y, w, h, fill, stroke)
      break
    case 'cylinder':
    case 'flow_db':
    case 'net_database':
      drawCylinder(ctx, x, y, w, h, fill, stroke)
      break
    case 'parallelogram':
    case 'flow_data':
      drawParallelogram(ctx, x, y, w, h, fill, stroke)
      break
    case 'triangle':
      drawTriangle(ctx, x, y, w, h, fill, stroke)
      break
    case 'hexagon':
    case 'flow_prep':
      drawHexagon(ctx, x, y, w, h, fill, stroke)
      break
    case 'cloud':
    case 'net_cloud':
      drawCloud(ctx, x, y, w, h, fill, stroke)
      break
    case 'cross':
      drawCross(ctx, x, y, w, h, fill, stroke)
      break
    case 'star':
      drawStar(ctx, x, y, w, h, fill, stroke)
      break
    case 'callout':
      drawCallout(ctx, x, y, w, h, fill, stroke)
      break
    case 'text':
      break
    case 'flow_document':
      drawDocument(ctx, x, y, w, h, fill, stroke)
      break
    case 'flow_manual':
      drawTrapezoid(ctx, x, y, w, h, fill, stroke, true)
      break
    case 'trapezoid':
      drawTrapezoid(ctx, x, y, w, h, fill, stroke, false)
      break
    case 'net_server':
    case 'aws_ec2':
      drawServer(ctx, x, y, w, h, fill, stroke)
      break
    case 'net_router':
      drawRouter(ctx, x, y, w, h, fill, stroke)
      break
    case 'net_desktop':
      drawDesktop(ctx, x, y, w, h, fill, stroke)
      break
    case 'net_firewall':
      drawFirewall(ctx, x, y, w, h, fill, stroke)
      break
    case 'uml_class':
    case 'uml_interface':
    case 'uml_abstract':
      drawUmlClass(ctx, x, y, w, h, fill, stroke, label)
      ctx.restore()
      return
    case 'uml_actor':
      drawActor(ctx, x, y, w, h, fill, stroke)
      break
    case 'uml_state':
      drawRect(ctx, x, y, w, h, 20, fill, stroke)
      break
    case 'uml_final':
      drawFinalState(ctx, x, y, w, h, fill, stroke)
      break
    case 'flow_connector':
      drawEllipse(ctx, x + w/4, y + h/4, w/2, h/2, fill, stroke)
      break
    case 'k8s_pod':
    case 'k8s_service':
      drawHexagon(ctx, x, y, w, h, fill, stroke)
      break
    case 'aws_vpc':
    case 'uml_system':
      ctx.strokeStyle = stroke; ctx.lineWidth = lw
      ctx.setLineDash([6, 4])
      ctx.strokeRect(x, y, w, h)
      ctx.setLineDash([])
      break
    default:
      if (type.startsWith('hw_')) drawHwImage(ctx, type, x, y, w, h)  // SVG rasterisé (laissé vide pendant le chargement, redessiné via onHwIconLoaded)
      else drawRect(ctx, x, y, w, h, style.rounded || 0, fill, stroke)
  }

  ctx.shadowColor = 'transparent'; ctx.shadowBlur = 0
  ctx.setLineDash([])

  // Label (skip for uml_class as it handles its own text)
  if (label && type !== 'uml_class' && type !== 'uml_interface' && type !== 'uml_abstract') {
    drawLabel(ctx, label, x, y, w, h, labelStyle)
  }

  ctx.restore()
}

// ── Shape drawing primitives ──────────────────────────────────────────────────

function drawRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number, fill: string, stroke: string) {
  ctx.beginPath()
  if (r > 0) ctx.roundRect(x, y, w, h, r)
  else ctx.rect(x, y, w, h)
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawEllipse(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  ctx.beginPath()
  ctx.ellipse(x + w/2, y + h/2, w/2, h/2, 0, 0, Math.PI * 2)
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawDiamond(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  ctx.beginPath()
  ctx.moveTo(x + w/2, y)
  ctx.lineTo(x + w, y + h/2)
  ctx.lineTo(x + w/2, y + h)
  ctx.lineTo(x, y + h/2)
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawCylinder(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const ry = Math.min(h * 0.15, 15)
  ctx.beginPath()
  ctx.ellipse(x + w/2, y + ry, w/2, ry, 0, 0, Math.PI * 2)
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
  ctx.beginPath()
  ctx.moveTo(x, y + ry)
  ctx.lineTo(x, y + h - ry)
  ctx.ellipse(x + w/2, y + h - ry, w/2, ry, 0, 0, Math.PI)
  ctx.lineTo(x + w, y + ry)
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawParallelogram(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const s = w * 0.15
  ctx.beginPath()
  ctx.moveTo(x + s, y); ctx.lineTo(x + w, y)
  ctx.lineTo(x + w - s, y + h); ctx.lineTo(x, y + h)
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawTriangle(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  ctx.beginPath()
  ctx.moveTo(x + w/2, y); ctx.lineTo(x + w, y + h); ctx.lineTo(x, y + h); ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawHexagon(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const s = w * 0.25
  ctx.beginPath()
  ctx.moveTo(x + s, y); ctx.lineTo(x + w - s, y); ctx.lineTo(x + w, y + h/2)
  ctx.lineTo(x + w - s, y + h); ctx.lineTo(x + s, y + h); ctx.lineTo(x, y + h/2)
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawCloud(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  ctx.beginPath()
  const cx = x + w/2, cy = y + h/2
  ctx.arc(cx - w*0.2, cy + h*0.1, h*0.28, 0, Math.PI * 2)
  ctx.arc(cx, cy - h*0.05, h*0.32, 0, Math.PI * 2)
  ctx.arc(cx + w*0.2, cy + h*0.05, h*0.25, 0, Math.PI * 2)
  ctx.arc(cx - w*0.3, cy + h*0.15, h*0.2, 0, Math.PI * 2)
  ctx.arc(cx + w*0.32, cy + h*0.18, h*0.22, 0, Math.PI * 2)
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawCross(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const a = w * 0.3, b = h * 0.3
  ctx.beginPath()
  ctx.moveTo(x + a, y); ctx.lineTo(x + w - a, y)
  ctx.lineTo(x + w - a, y + b); ctx.lineTo(x + w, y + b)
  ctx.lineTo(x + w, y + h - b); ctx.lineTo(x + w - a, y + h - b)
  ctx.lineTo(x + w - a, y + h); ctx.lineTo(x + a, y + h)
  ctx.lineTo(x + a, y + h - b); ctx.lineTo(x, y + h - b)
  ctx.lineTo(x, y + b); ctx.lineTo(x + a, y + b)
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawStar(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const cx = x + w/2, cy = y + h/2, ro = Math.min(w, h)/2, ri = ro * 0.4, n = 5
  ctx.beginPath()
  for (let i = 0; i < n * 2; i++) {
    const r = i % 2 === 0 ? ro : ri
    const a = (i * Math.PI) / n - Math.PI / 2
    if (i === 0) ctx.moveTo(cx + r * Math.cos(a), cy + r * Math.sin(a))
    else ctx.lineTo(cx + r * Math.cos(a), cy + r * Math.sin(a))
  }
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawCallout(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const bh = h * 0.8
  ctx.beginPath()
  ctx.roundRect(x, y, w, bh, 6)
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
  ctx.beginPath()
  ctx.moveTo(x + w * 0.2, y + bh)
  ctx.lineTo(x + w * 0.12, y + h)
  ctx.lineTo(x + w * 0.32, y + bh)
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawDocument(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const wave = h * 0.12
  ctx.beginPath()
  ctx.moveTo(x, y)
  ctx.lineTo(x + w, y)
  ctx.lineTo(x + w, y + h - wave)
  ctx.quadraticCurveTo(x + w * 0.75, y + h - wave * 0.5, x + w * 0.5, y + h - wave)
  ctx.quadraticCurveTo(x + w * 0.25, y + h - wave * 1.5, x, y + h - wave)
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawTrapezoid(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string, inverted: boolean) {
  const s = w * 0.15
  ctx.beginPath()
  if (inverted) {
    ctx.moveTo(x + s, y); ctx.lineTo(x + w - s, y); ctx.lineTo(x + w, y + h); ctx.lineTo(x, y + h)
  } else {
    ctx.moveTo(x, y); ctx.lineTo(x + w, y); ctx.lineTo(x + w - s, y + h); ctx.lineTo(x + s, y + h)
  }
  ctx.closePath()
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawServer(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const rows = 3, rh = h / (rows + 0.5)
  for (let i = 0; i < rows; i++) {
    const ry = y + i * rh + rh * 0.1
    ctx.beginPath(); ctx.rect(x, ry, w, rh * 0.8)
    ctx.fillStyle = fill; ctx.fill()
    ctx.strokeStyle = stroke; ctx.stroke()
    ctx.beginPath(); ctx.arc(x + w - 12, ry + rh * 0.4, 3, 0, Math.PI * 2)
    ctx.fillStyle = i === 0 ? '#34a853' : '#9aa0a6'; ctx.fill()
  }
}

function drawRouter(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawEllipse(ctx, x + w * 0.1, y + h * 0.2, w * 0.8, h * 0.5, fill, stroke)
  const lw = 1.5
  ctx.strokeStyle = stroke; ctx.lineWidth = lw
  ;[[0.2, 0.75, 0.1, 1], [0.5, 0.75, 0.5, 1], [0.8, 0.75, 0.9, 1]].forEach(([x1, y1, x2, y2]) => {
    ctx.beginPath(); ctx.moveTo(x + w * x1, y + h * y1); ctx.lineTo(x + w * x2, y + h * y2); ctx.stroke()
  })
}

function drawDesktop(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const mh = h * 0.65
  drawRect(ctx, x, y, w, mh, 4, fill, stroke)
  ctx.fillStyle = '#aecbfa'; ctx.fillRect(x + 4, y + 4, w - 8, mh - 8)
  ctx.beginPath(); ctx.moveTo(x + w * 0.4, y + mh); ctx.lineTo(x + w * 0.35, y + h)
  ctx.moveTo(x + w * 0.6, y + mh); ctx.lineTo(x + w * 0.65, y + h)
  ctx.moveTo(x + w * 0.25, y + h); ctx.lineTo(x + w * 0.75, y + h)
  ctx.strokeStyle = stroke; ctx.stroke()
}

function drawFirewall(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 0, fill, stroke)
  ctx.strokeStyle = '#d93025'; ctx.lineWidth = 2
  ;[0.25, 0.5, 0.75].forEach(xr => {
    ctx.beginPath(); ctx.moveTo(x + w * xr, y + 4); ctx.lineTo(x + w * xr, y + h - 4); ctx.stroke()
  })
}

function drawUmlClass(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string, label: string) {
  const hh = Math.min(h * 0.3, 28)
  drawRect(ctx, x, y, w, hh, 0, fill, stroke)
  ctx.fillStyle = '#202124'; ctx.font = 'bold 11px "Google Sans", Arial'
  ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
  ctx.fillText('«interface»', x + w/2, y + hh/2 - 6)
  ctx.fillText(label || 'ClassName', x + w/2, y + hh/2 + 6)
  drawRect(ctx, x, y + hh, w, (h - hh) / 2, 0, fill === 'none' ? '#f8f9fa' : fill, stroke)
  drawRect(ctx, x, y + hh + (h - hh) / 2, w, (h - hh) / 2, 0, fill === 'none' ? '#f8f9fa' : fill, stroke)
}

function drawActor(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const hw = w * 0.35, hh = h * 0.22
  ctx.strokeStyle = stroke; ctx.lineWidth = 1.5; ctx.fillStyle = fill
  ctx.beginPath(); ctx.ellipse(x + w/2, y + hh * 0.6, hw * 0.45, hh * 0.55, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke()
  ctx.beginPath()
  ctx.moveTo(x + w/2, y + hh * 1.2)
  ctx.lineTo(x + w/2, y + h * 0.68)
  ctx.moveTo(x + w/2 - hw, y + h * 0.45); ctx.lineTo(x + w/2 + hw, y + h * 0.45)
  ctx.moveTo(x + w/2, y + h * 0.68); ctx.lineTo(x + w/2 - hw * 0.8, y + h)
  ctx.moveTo(x + w/2, y + h * 0.68); ctx.lineTo(x + w/2 + hw * 0.8, y + h)
  ctx.stroke()
}

function drawFinalState(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, _fill: string, stroke: string) {
  const r = Math.min(w, h) / 2
  const cx = x + w/2, cy = y + h/2
  ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2)
  ctx.fillStyle = stroke; ctx.fill()
  ctx.beginPath(); ctx.arc(cx, cy, r * 0.6, 0, Math.PI * 2)
  ctx.fillStyle = '#ffffff'; ctx.fill()
}

// ── Label drawing ─────────────────────────────────────────────────────────────

export function drawLabel(
  ctx: CanvasRenderingContext2D,
  label: string,
  x: number, y: number, w: number, h: number,
  ls: { fontFamily: string; fontSize: number; bold: boolean; italic: boolean; color: string; align: string; verticalAlign: string },
) {
  if (!label) return
  ctx.fillStyle    = ls.color
  const weight     = ls.bold ? 'bold ' : ''
  const style      = ls.italic ? 'italic ' : ''
  ctx.font         = `${style}${weight}${ls.fontSize}px "${ls.fontFamily}", Arial, sans-serif`
  ctx.textAlign    = ls.align as CanvasTextAlign
  ctx.textBaseline = ls.verticalAlign === 'top' ? 'top' : ls.verticalAlign === 'bottom' ? 'bottom' : 'middle'

  const tx = ls.align === 'left' ? x + 8 : ls.align === 'right' ? x + w - 8 : x + w / 2
  const ty = ls.verticalAlign === 'top' ? y + 8 : ls.verticalAlign === 'bottom' ? y + h - 8 : y + h / 2

  const maxW = w - 16
  const words = label.split(' ')
  let line = ''
  const lh = ls.fontSize * 1.3
  let cy = ty

  for (const word of words) {
    const test = line ? `${line} ${word}` : word
    if (ctx.measureText(test).width > maxW && line) {
      ctx.fillText(line, tx, cy); line = word; cy += lh
    } else { line = test }
  }
  ctx.fillText(line, tx, cy)
}
// ── Arrow drawing ─────────────────────────────────────────────────────────────

export function drawArrow(
  ctx: CanvasRenderingContext2D,
  from: { x: number; y: number },
  to: { x: number; y: number },
  type: string,
  color: string, lw: number,
) {
  const angle = Math.atan2(to.y - from.y, to.x - from.x)
  const size  = Math.max(lw * 4, 8)

  ctx.fillStyle = color; ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.setLineDash([])

  switch (type) {
    case 'classic':
    case 'block': {
      ctx.beginPath()
      ctx.moveTo(to.x, to.y)
      ctx.lineTo(to.x - size * Math.cos(angle - Math.PI/6), to.y - size * Math.sin(angle - Math.PI/6))
      ctx.lineTo(to.x - size * Math.cos(angle + Math.PI/6), to.y - size * Math.sin(angle + Math.PI/6))
      ctx.closePath()
      if (type === 'block') ctx.fill(); else ctx.stroke()
      break
    }
    case 'open': {
      ctx.beginPath()
      ctx.moveTo(to.x - size * Math.cos(angle - Math.PI/6), to.y - size * Math.sin(angle - Math.PI/6))
      ctx.lineTo(to.x, to.y)
      ctx.lineTo(to.x - size * Math.cos(angle + Math.PI/6), to.y - size * Math.sin(angle + Math.PI/6))
      ctx.stroke(); break
    }
    case 'oval': {
      const cx = to.x - (size/2) * Math.cos(angle), cy = to.y - (size/2) * Math.sin(angle)
      ctx.beginPath(); ctx.arc(cx, cy, size/2, 0, Math.PI * 2); ctx.fill(); break
    }
    case 'diamond': {
      ctx.beginPath()
      ctx.moveTo(to.x, to.y)
      ctx.lineTo(to.x - size*0.6 * Math.cos(angle - Math.PI/6), to.y - size*0.6 * Math.sin(angle - Math.PI/6))
      ctx.lineTo(to.x - size * Math.cos(angle), to.y - size * Math.sin(angle))
      ctx.lineTo(to.x - size*0.6 * Math.cos(angle + Math.PI/6), to.y - size*0.6 * Math.sin(angle + Math.PI/6))
      ctx.closePath(); ctx.fill(); break
    }
  }
}

// ── Stencil catalogue ─────────────────────────────────────────────────────────

export const STENCILS: StencilDef[] = [
  // Formes basiques
  { id: 'rect',          name: 'Rectangle',           category: 'Formes basiques', defaultW: 120, defaultH: 60,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'rounded_rect',  name: 'Rect. arrondi',        category: 'Formes basiques', defaultW: 120, defaultH: 60,  style: { fillColor: '#d5e8d4', strokeColor: '#82b366', rounded: 12 } },
  { id: 'ellipse',       name: 'Ellipse',              category: 'Formes basiques', defaultW: 120, defaultH: 70,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'diamond',       name: 'Losange',              category: 'Formes basiques', defaultW: 120, defaultH: 80,  style: { fillColor: '#ffe6cc', strokeColor: '#d79b00' } },
  { id: 'cylinder',      name: 'Cylindre',             category: 'Formes basiques', defaultW: 80,  defaultH: 100, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'parallelogram', name: 'Parallélogramme',      category: 'Formes basiques', defaultW: 120, defaultH: 60,  style: { fillColor: '#e1d5e7', strokeColor: '#9673a6' } },
  { id: 'triangle',      name: 'Triangle',             category: 'Formes basiques', defaultW: 100, defaultH: 80,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'hexagon',       name: 'Hexagone',             category: 'Formes basiques', defaultW: 120, defaultH: 70,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'cloud',         name: 'Nuage',                category: 'Formes basiques', defaultW: 120, defaultH: 80,  style: { fillColor: '#f5f5f5', strokeColor: '#666666' } },
  { id: 'cross',         name: 'Croix',                category: 'Formes basiques', defaultW: 80,  defaultH: 80,  style: { fillColor: '#f8cecc', strokeColor: '#b85450' } },
  { id: 'star',          name: 'Étoile',               category: 'Formes basiques', defaultW: 80,  defaultH: 80,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'callout',       name: 'Bulle',                category: 'Formes basiques', defaultW: 120, defaultH: 80,  style: { fillColor: '#ffffff', strokeColor: '#000000' } },
  { id: 'trapezoid',     name: 'Trapèze',              category: 'Formes basiques', defaultW: 120, defaultH: 60,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'text',          name: 'Texte',                category: 'Formes basiques', defaultW: 120, defaultH: 40,  style: { fillColor: 'none', strokeColor: 'none' } },
  // Flux
  { id: 'flow_start',    name: 'Début/Fin',            category: 'Flux',            defaultW: 120, defaultH: 50,  style: { fillColor: '#d5e8d4', strokeColor: '#82b366', rounded: 30 } },
  { id: 'flow_process',  name: 'Processus',            category: 'Flux',            defaultW: 120, defaultH: 60,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'flow_decision', name: 'Décision',             category: 'Flux',            defaultW: 120, defaultH: 80,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'flow_data',     name: 'Données',              category: 'Flux',            defaultW: 120, defaultH: 60,  style: { fillColor: '#e1d5e7', strokeColor: '#9673a6' } },
  { id: 'flow_document', name: 'Document',             category: 'Flux',            defaultW: 120, defaultH: 70,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'flow_db',       name: 'Base de données',      category: 'Flux',            defaultW: 80,  defaultH: 100, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'flow_prep',     name: 'Préparation',          category: 'Flux',            defaultW: 120, defaultH: 60,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'flow_connector',name: 'Connecteur',           category: 'Flux',            defaultW: 40,  defaultH: 40,  style: { fillColor: '#ffffff', strokeColor: '#000000' } },
  { id: 'flow_manual',   name: 'Saisie manuelle',      category: 'Flux',            defaultW: 120, defaultH: 60,  style: { fillColor: '#ffe6cc', strokeColor: '#d79b00' } },
  // Réseau
  { id: 'net_server',    name: 'Serveur',              category: 'Réseau',          defaultW: 100, defaultH: 70,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'net_router',    name: 'Routeur',              category: 'Réseau',          defaultW: 80,  defaultH: 70,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'net_database',  name: 'Base de données',      category: 'Réseau',          defaultW: 80,  defaultH: 100, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'net_cloud',     name: 'Cloud',                category: 'Réseau',          defaultW: 120, defaultH: 80,  style: { fillColor: '#e8f0fe', strokeColor: '#1a73e8' } },
  { id: 'net_firewall',  name: 'Pare-feu',             category: 'Réseau',          defaultW: 80,  defaultH: 80,  style: { fillColor: '#fce8e6', strokeColor: '#d93025' } },
  { id: 'net_desktop',   name: 'Ordinateur',           category: 'Réseau',          defaultW: 80,  defaultH: 80,  style: { fillColor: '#e8eaed', strokeColor: '#5f6368' } },
  // UML
  { id: 'uml_class',     name: 'Classe',               category: 'UML',             defaultW: 140, defaultH: 100, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'uml_interface', name: 'Interface',            category: 'UML',             defaultW: 140, defaultH: 100, style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'uml_actor',     name: 'Acteur',               category: 'UML',             defaultW: 60,  defaultH: 100, style: { fillColor: '#ffffff', strokeColor: '#000000' } },
  { id: 'uml_usecase',   name: "Cas d'utilisation",    category: 'UML',             defaultW: 140, defaultH: 60,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'uml_state',     name: 'État',                 category: 'UML',             defaultW: 120, defaultH: 60,  style: { fillColor: '#e1d5e7', strokeColor: '#9673a6', rounded: 20 } },
  { id: 'uml_initial',   name: 'État initial',         category: 'UML',             defaultW: 30,  defaultH: 30,  style: { fillColor: '#000000', strokeColor: '#000000' } },
  { id: 'uml_final',     name: 'État final',           category: 'UML',             defaultW: 30,  defaultH: 30,  style: { fillColor: '#000000', strokeColor: '#000000' } },
  { id: 'uml_system',    name: 'Système',              category: 'UML',             defaultW: 200, defaultH: 150, style: { fillColor: 'none', strokeColor: '#000000' } },
  // Cloud / infra
  { id: 'aws_ec2',       name: 'EC2',                  category: 'AWS',             defaultW: 80,  defaultH: 80,  style: { fillColor: '#fce8d8', strokeColor: '#e07624' } },
  { id: 'aws_s3',        name: 'S3',                   category: 'AWS',             defaultW: 80,  defaultH: 80,  style: { fillColor: '#d5e8d4', strokeColor: '#3b803c' } },
  { id: 'aws_rds',       name: 'RDS',                  category: 'AWS',             defaultW: 80,  defaultH: 80,  style: { fillColor: '#dae8fc', strokeColor: '#1a73e8' } },
  { id: 'aws_lambda',    name: 'Lambda',               category: 'AWS',             defaultW: 80,  defaultH: 80,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'aws_vpc',       name: 'VPC',                  category: 'AWS',             defaultW: 200, defaultH: 150, style: { fillColor: 'none', strokeColor: '#1a73e8' } },
  { id: 'k8s_pod',       name: 'Pod',                  category: 'Kubernetes',      defaultW: 80,  defaultH: 80,  style: { fillColor: '#e8f0fe', strokeColor: '#1a73e8' } },
  { id: 'k8s_service',   name: 'Service',              category: 'Kubernetes',      defaultW: 80,  defaultH: 80,  style: { fillColor: '#e6f4ea', strokeColor: '#34a853' } },
  { id: 'k8s_deployment',name: 'Deployment',           category: 'Kubernetes',      defaultW: 100, defaultH: 60,  style: { fillColor: '#fce8e6', strokeColor: '#d93025' } },
  // Ordinateur et Matériel (icônes SVG Repo, collection « Computer and Hardware Duotone », MIT)
  ...HW_STENCILS,
]

export const STENCIL_MAP: Record<string, StencilDef> = Object.fromEntries(STENCILS.map(s => [s.id, s]))

// Stable category identifiers (used for filtering / tab state) decoupled from
// the French fallback labels stored on each StencilDef.
const CATEGORY_ID: Record<string, string> = {
  'Formes basiques': 'basic',
  'Flux':            'flow',
  'Réseau':          'network',
  'UML':             'uml',
  'AWS':             'aws',
  'Kubernetes':      'k8s',
  'Matériel':        'hardware',
}

export function categoryIdOf(s: StencilDef): string {
  return CATEGORY_ID[s.category] ?? 'basic'
}

export function getCategories(): string[] {
  const seen = new Set<string>()
  const cats: string[] = []
  for (const s of STENCILS) {
    const cid = categoryIdOf(s)
    if (!seen.has(cid)) { seen.add(cid); cats.push(cid) }
  }
  return cats
}

export function getStencilsByCategory(catId: string): StencilDef[] {
  return STENCILS.filter(s => categoryIdOf(s) === catId)
}

export function searchStencils(q: string, nameOf?: (s: StencilDef) => string): StencilDef[] {
  const lq = q.toLowerCase()
  if (nameOf) {
    return STENCILS.filter(s => nameOf(s).toLowerCase().includes(lq) || s.id.toLowerCase().includes(lq))
  }
  return STENCILS.filter(s => s.name.toLowerCase().includes(lq) || s.category.toLowerCase().includes(lq))
}
