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
    // ── Basic shapes (extended) ──
    case 'shp_pentagon':    drawRegularPolygon(ctx, x, y, w, h, 5, -90, fill, stroke); break
    case 'shp_octagon':     drawRegularPolygon(ctx, x, y, w, h, 8, 22.5, fill, stroke); break
    case 'shp_arrow_right': drawBlockArrow(ctx, x, y, w, h, 'right', fill, stroke); break
    case 'shp_arrow_left':  drawBlockArrow(ctx, x, y, w, h, 'left', fill, stroke); break
    case 'shp_arrow_up':    drawBlockArrow(ctx, x, y, w, h, 'up', fill, stroke); break
    case 'shp_arrow_down':  drawBlockArrow(ctx, x, y, w, h, 'down', fill, stroke); break
    case 'shp_cube':
    case 'uml_node':        drawCube(ctx, x, y, w, h, fill, stroke); break
    case 'shp_step':        drawStep(ctx, x, y, w, h, fill, stroke); break
    case 'shp_card':
    case 'flow_card':       drawCard(ctx, x, y, w, h, fill, stroke); break
    case 'shp_note':
    case 'uml_note':        drawNote(ctx, x, y, w, h, fill, stroke); break
    // ── Flowchart (extended) ──
    case 'flow_terminator':
    case 'bpmn_task':       drawRect(ctx, x, y, w, h, Math.min(h / 2, style.rounded || 999), fill, stroke); break
    case 'flow_predefined': drawPredefinedProcess(ctx, x, y, w, h, fill, stroke); break
    case 'flow_internal_storage': drawInternalStorage(ctx, x, y, w, h, fill, stroke); break
    case 'flow_display':    drawDisplay(ctx, x, y, w, h, fill, stroke); break
    case 'flow_delay':      drawDelay(ctx, x, y, w, h, fill, stroke); break
    case 'flow_offpage':    drawOffPage(ctx, x, y, w, h, fill, stroke); break
    case 'flow_or':         drawOpSign(ctx, x, y, w, h, fill, stroke, 'or'); break
    case 'flow_summing':    drawOpSign(ctx, x, y, w, h, fill, stroke, 'sum'); break
    case 'flow_manual_op':  drawTrapezoid(ctx, x, y, w, h, fill, stroke, true); break
    // ── Entity-relation ──
    case 'er_entity':       drawRect(ctx, x, y, w, h, 0, fill, stroke); break
    case 'er_weak_entity':  drawDoubleRect(ctx, x, y, w, h, fill, stroke); break
    case 'er_attribute':
    case 'er_key_attribute':drawEllipse(ctx, x, y, w, h, fill, stroke); break
    case 'er_relationship':
    case 'bpmn_gateway':    drawDiamond(ctx, x, y, w, h, fill, stroke); break
    // ── BPMN ──
    case 'bpmn_start':      drawEllipse(ctx, x, y, w, h, fill, stroke); break
    case 'bpmn_end':
    case 'bpmn_event':      drawDoubleEllipse(ctx, x, y, w, h, fill, stroke); break
    // ── UML (extended) ──
    case 'uml_package':     drawUmlPackage(ctx, x, y, w, h, fill, stroke); break
    case 'uml_component':   drawUmlComponent(ctx, x, y, w, h, fill, stroke); break
    case 'uml_object':      drawRect(ctx, x, y, w, h, 0, fill, stroke); break
    // ── Network (extended) ──
    case 'net_user':        drawPerson(ctx, x, y, w, h, fill, stroke); break
    case 'net_laptop':      drawLaptop(ctx, x, y, w, h, fill, stroke); break
    case 'net_mobile':      drawMobile(ctx, x, y, w, h, fill, stroke); break
    case 'net_switch':      drawSwitch(ctx, x, y, w, h, fill, stroke); break
    case 'net_printer':     drawPrinter(ctx, x, y, w, h, fill, stroke); break
    case 'net_wifi':        drawWifi(ctx, x, y, w, h, fill, stroke); break
    case 'net_internet':    drawCloud(ctx, x, y, w, h, fill, stroke); break
    // ── UI mockups ──
    case 'ui_button':       drawRect(ctx, x, y, w, h, 6, fill, stroke); break
    case 'ui_input':        drawInputField(ctx, x, y, w, h, fill, stroke); break
    case 'ui_checkbox':     drawCheckbox(ctx, x, y, w, h, fill, stroke); break
    case 'ui_radio':        drawRadio(ctx, x, y, w, h, fill, stroke); break
    case 'ui_dropdown':     drawDropdownField(ctx, x, y, w, h, fill, stroke); break
    case 'ui_browser':      drawBrowser(ctx, x, y, w, h, fill, stroke); break
    case 'ui_image':        drawImagePlaceholder(ctx, x, y, w, h, fill, stroke); break
    // ── Containers / swimlanes (own title, skip generic label) ──
    case 'container':
    case 'swimlane_v':      drawSwimlane(ctx, x, y, w, h, fill, stroke, 'v', label, labelStyle); ctx.restore(); return
    case 'swimlane_h':      drawSwimlane(ctx, x, y, w, h, fill, stroke, 'h', label, labelStyle); ctx.restore(); return
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

// ── Extended primitives (flowchart / UML / ER / BPMN / network / UI mockups) ──

function fs(ctx: CanvasRenderingContext2D, fill: string, stroke: string) {
  if (fill !== 'none') { ctx.fillStyle = fill; ctx.fill() }
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}
function polyPath(ctx: CanvasRenderingContext2D, pts: Array<[number, number]>) {
  ctx.beginPath()
  pts.forEach(([px, py], i) => (i ? ctx.lineTo(px, py) : ctx.moveTo(px, py)))
  ctx.closePath()
}

function drawRegularPolygon(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, n: number, rotDeg: number, fill: string, stroke: string) {
  const cx = x + w / 2, cy = y + h / 2, rx = w / 2, ry = h / 2
  const rot = (rotDeg * Math.PI) / 180
  ctx.beginPath()
  for (let i = 0; i < n; i++) {
    const a = rot + (i * 2 * Math.PI) / n
    const px = cx + rx * Math.cos(a), py = cy + ry * Math.sin(a)
    i ? ctx.lineTo(px, py) : ctx.moveTo(px, py)
  }
  ctx.closePath(); fs(ctx, fill, stroke)
}

function drawBlockArrow(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, dir: 'right' | 'left' | 'up' | 'down', fill: string, stroke: string) {
  const t = 0.35, head = 0.45 // tail thickness ratio, head length ratio
  let pts: Array<[number, number]>
  if (dir === 'right' || dir === 'left') {
    const hx = x + w * (dir === 'right' ? 1 - head : head)
    const ty0 = y + h * (0.5 - t / 2), ty1 = y + h * (0.5 + t / 2)
    pts = dir === 'right'
      ? [[x, ty0], [hx, ty0], [hx, y], [x + w, y + h / 2], [hx, y + h], [hx, ty1], [x, ty1]]
      : [[x + w, ty0], [hx, ty0], [hx, y], [x, y + h / 2], [hx, y + h], [hx, ty1], [x + w, ty1]]
  } else {
    const hy = y + h * (dir === 'down' ? 1 - head : head)
    const tx0 = x + w * (0.5 - t / 2), tx1 = x + w * (0.5 + t / 2)
    pts = dir === 'down'
      ? [[tx0, y], [tx0, hy], [x, hy], [x + w / 2, y + h], [x + w, hy], [tx1, hy], [tx1, y]]
      : [[tx0, y + h], [tx0, hy], [x, hy], [x + w / 2, y], [x + w, hy], [tx1, hy], [tx1, y + h]]
  }
  polyPath(ctx, pts); fs(ctx, fill, stroke)
}

function drawCube(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const d = Math.min(w, h) * 0.22
  polyPath(ctx, [[x, y + d], [x + w - d, y + d], [x + w - d, y + h], [x, y + h]]); fs(ctx, fill, stroke)         // front
  polyPath(ctx, [[x, y + d], [x + d, y], [x + w, y], [x + w - d, y + d]]); fs(ctx, fill, stroke)                 // top
  polyPath(ctx, [[x + w - d, y + d], [x + w, y], [x + w, y + h - d], [x + w - d, y + h]]); fs(ctx, fill, stroke) // side
}

function drawStep(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const s = h * 0.5
  polyPath(ctx, [[x, y], [x + w - s, y], [x + w, y + h / 2], [x + w - s, y + h], [x, y + h], [x + s, y + h / 2]])
  fs(ctx, fill, stroke)
}

function drawCard(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const c = Math.min(w, h) * 0.25
  polyPath(ctx, [[x + c, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y + c]]); fs(ctx, fill, stroke)
}

function drawNote(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const c = Math.min(w, h) * 0.25
  polyPath(ctx, [[x, y], [x + w - c, y], [x + w, y + c], [x + w, y + h], [x, y + h]]); fs(ctx, fill, stroke)
  polyPath(ctx, [[x + w - c, y], [x + w - c, y + c], [x + w, y + c]]); fs(ctx, 'none', stroke) // folded corner
}

function drawPredefinedProcess(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 0, fill, stroke)
  const i = w * 0.1
  ctx.beginPath(); ctx.moveTo(x + i, y); ctx.lineTo(x + i, y + h)
  ctx.moveTo(x + w - i, y); ctx.lineTo(x + w - i, y + h)
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawDoubleRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 0, fill, stroke)
  drawRect(ctx, x + 4, y + 4, w - 8, h - 8, 0, 'none', stroke)
}

function drawInternalStorage(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 0, fill, stroke)
  ctx.beginPath(); ctx.moveTo(x + w * 0.18, y); ctx.lineTo(x + w * 0.18, y + h)
  ctx.moveTo(x, y + h * 0.25); ctx.lineTo(x + w, y + h * 0.25)
  if (stroke !== 'none') { ctx.strokeStyle = stroke; ctx.stroke() }
}

function drawDisplay(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  ctx.beginPath()
  ctx.moveTo(x, y + h / 2)
  ctx.lineTo(x + w * 0.18, y)
  ctx.lineTo(x + w * 0.8, y)
  ctx.quadraticCurveTo(x + w, y, x + w, y + h / 2)
  ctx.quadraticCurveTo(x + w, y + h, x + w * 0.8, y + h)
  ctx.lineTo(x + w * 0.18, y + h)
  ctx.closePath(); fs(ctx, fill, stroke)
}

function drawDelay(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  ctx.beginPath()
  ctx.moveTo(x, y); ctx.lineTo(x + w * 0.6, y)
  ctx.quadraticCurveTo(x + w, y, x + w, y + h / 2)
  ctx.quadraticCurveTo(x + w, y + h, x + w * 0.6, y + h)
  ctx.lineTo(x, y + h); ctx.closePath(); fs(ctx, fill, stroke)
}

function drawOffPage(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  polyPath(ctx, [[x, y], [x + w, y], [x + w, y + h * 0.6], [x + w / 2, y + h], [x, y + h * 0.6]])
  fs(ctx, fill, stroke)
}

function drawOpSign(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string, kind: 'or' | 'sum') {
  drawEllipse(ctx, x, y, w, h, fill, stroke)
  const cx = x + w / 2, cy = y + h / 2, r = Math.min(w, h) / 2
  ctx.strokeStyle = stroke
  ctx.beginPath()
  if (kind === 'or') { ctx.moveTo(cx - r, cy); ctx.lineTo(cx + r, cy); ctx.moveTo(cx, cy - r); ctx.lineTo(cx, cy + r) }
  else { const d = r * 0.7; ctx.moveTo(cx - d, cy - d); ctx.lineTo(cx + d, cy + d); ctx.moveTo(cx + d, cy - d); ctx.lineTo(cx - d, cy + d) }
  ctx.stroke()
}

function drawDoubleEllipse(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawEllipse(ctx, x, y, w, h, fill, stroke)
  drawEllipse(ctx, x + 4, y + 4, w - 8, h - 8, 'none', stroke)
}

function drawUmlPackage(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const th = Math.min(h * 0.22, 20), tw = w * 0.4
  drawRect(ctx, x, y, tw, th, 0, fill, stroke)
  drawRect(ctx, x, y + th, w, h - th, 0, fill, stroke)
}

function drawUmlComponent(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x + 8, y, w - 8, h, 0, fill, stroke)
  drawRect(ctx, x, y + h * 0.2, 16, h * 0.2, 0, fill, stroke)
  drawRect(ctx, x, y + h * 0.6, 16, h * 0.2, 0, fill, stroke)
}

function drawPerson(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const cx = x + w / 2
  ctx.beginPath(); ctx.arc(cx, y + h * 0.28, Math.min(w, h) * 0.22, 0, Math.PI * 2); fs(ctx, fill, stroke)
  ctx.beginPath()
  ctx.moveTo(x + w * 0.15, y + h)
  ctx.quadraticCurveTo(x + w * 0.15, y + h * 0.55, cx, y + h * 0.55)
  ctx.quadraticCurveTo(x + w * 0.85, y + h * 0.55, x + w * 0.85, y + h)
  ctx.closePath(); fs(ctx, fill, stroke)
}

function drawLaptop(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  const sh = h * 0.7
  drawRect(ctx, x + w * 0.1, y, w * 0.8, sh, 3, fill, stroke)
  ctx.fillStyle = '#aecbfa'; ctx.fillRect(x + w * 0.1 + 3, y + 3, w * 0.8 - 6, sh - 6)
  polyPath(ctx, [[x, y + h], [x + w * 0.1, y + sh], [x + w * 0.9, y + sh], [x + w, y + h]]); fs(ctx, fill, stroke)
}

function drawMobile(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x + w * 0.25, y, w * 0.5, h, 6, fill, stroke)
  ctx.fillStyle = '#aecbfa'; ctx.fillRect(x + w * 0.28, y + h * 0.1, w * 0.44, h * 0.72)
  ctx.beginPath(); ctx.arc(x + w / 2, y + h * 0.9, 2.5, 0, Math.PI * 2); ctx.fillStyle = stroke; ctx.fill()
}

function drawSwitch(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 4, fill, stroke)
  ctx.strokeStyle = stroke
  for (let i = 0; i < 4; i++) {
    const ax = x + w * (0.2 + i * 0.2)
    ctx.beginPath(); ctx.moveTo(ax, y + h * 0.35); ctx.lineTo(ax + w * 0.08, y + h * 0.35)
    ctx.moveTo(ax, y + h * 0.65); ctx.lineTo(ax + w * 0.08, y + h * 0.65); ctx.stroke()
  }
}

function drawPrinter(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x + w * 0.15, y, w * 0.7, h * 0.3, 2, '#ffffff', stroke)
  drawRect(ctx, x, y + h * 0.3, w, h * 0.45, 3, fill, stroke)
  drawRect(ctx, x + w * 0.2, y + h * 0.6, w * 0.6, h * 0.4, 0, '#ffffff', stroke)
  ctx.fillStyle = '#34a853'; ctx.beginPath(); ctx.arc(x + w * 0.85, y + h * 0.45, 2.5, 0, Math.PI * 2); ctx.fill()
}

function drawWifi(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, _fill: string, stroke: string) {
  const cx = x + w / 2, cy = y + h * 0.85
  ctx.strokeStyle = stroke; ctx.lineWidth = 2
  for (let i = 1; i <= 3; i++) {
    ctx.beginPath(); ctx.arc(cx, cy, (Math.min(w, h) * 0.28) * i, Math.PI * 1.25, Math.PI * 1.75); ctx.stroke()
  }
  ctx.beginPath(); ctx.arc(cx, cy, 3, 0, Math.PI * 2); ctx.fillStyle = stroke; ctx.fill()
}

function drawBrowser(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 4, fill, stroke)
  const bar = Math.min(h * 0.22, 22)
  ctx.fillStyle = '#e8eaed'; ctx.fillRect(x + 1, y + 1, w - 2, bar)
  ctx.strokeStyle = stroke; ctx.beginPath(); ctx.moveTo(x, y + bar); ctx.lineTo(x + w, y + bar); ctx.stroke()
  ;['#ea4335', '#fbbc04', '#34a853'].forEach((c, i) => { ctx.fillStyle = c; ctx.beginPath(); ctx.arc(x + 10 + i * 12, y + bar / 2, 3, 0, Math.PI * 2); ctx.fill() })
}

function drawInputField(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, _fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 4, '#ffffff', stroke)
}

function drawCheckbox(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, _fill: string, stroke: string) {
  const s = Math.min(h, 18)
  drawRect(ctx, x, y + (h - s) / 2, s, s, 3, '#ffffff', stroke)
  ctx.strokeStyle = '#34a853'; ctx.lineWidth = 2
  ctx.beginPath(); ctx.moveTo(x + s * 0.2, y + (h - s) / 2 + s * 0.5); ctx.lineTo(x + s * 0.45, y + (h - s) / 2 + s * 0.75); ctx.lineTo(x + s * 0.8, y + (h - s) / 2 + s * 0.25); ctx.stroke()
}

function drawRadio(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, _fill: string, stroke: string) {
  const s = Math.min(h, 18), cy = y + h / 2
  ctx.beginPath(); ctx.arc(x + s / 2, cy, s / 2, 0, Math.PI * 2); ctx.fillStyle = '#ffffff'; ctx.fill(); ctx.strokeStyle = stroke; ctx.stroke()
  ctx.beginPath(); ctx.arc(x + s / 2, cy, s * 0.25, 0, Math.PI * 2); ctx.fillStyle = stroke; ctx.fill()
}

function drawDropdownField(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, _fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 4, '#ffffff', stroke)
  ctx.fillStyle = stroke
  const cx = x + w - 12, cy = y + h / 2
  ctx.beginPath(); ctx.moveTo(cx - 4, cy - 2); ctx.lineTo(cx + 4, cy - 2); ctx.lineTo(cx, cy + 3); ctx.closePath(); ctx.fill()
}

function drawImagePlaceholder(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string) {
  drawRect(ctx, x, y, w, h, 0, fill, stroke)
  ctx.strokeStyle = stroke; ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x + w, y + h); ctx.moveTo(x + w, y); ctx.lineTo(x, y + h); ctx.stroke()
}

function drawSwimlane(
  ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, fill: string, stroke: string,
  orient: 'v' | 'h', label: string,
  ls: { color: string; fontSize: number },
) {
  drawRect(ctx, x, y, w, h, 0, fill === 'none' ? '#ffffff' : fill, stroke)
  const band = 28
  ctx.save()
  ctx.fillStyle = '#eef1f5'
  if (orient === 'v') ctx.fillRect(x + 0.5, y + 0.5, w - 1, Math.min(band, h) - 1)
  else ctx.fillRect(x + 0.5, y + 0.5, Math.min(band, w) - 1, h - 1)
  ctx.strokeStyle = stroke
  ctx.beginPath()
  if (orient === 'v') { ctx.moveTo(x, y + band); ctx.lineTo(x + w, y + band) }
  else { ctx.moveTo(x + band, y); ctx.lineTo(x + band, y + h) }
  ctx.stroke()
  ctx.restore()
  if (label) {
    ctx.save()
    ctx.fillStyle = ls.color || '#202124'
    ctx.font = `bold ${ls.fontSize || 12}px "Google Sans", Inter, sans-serif`
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
    if (orient === 'v') ctx.fillText(label, x + w / 2, y + band / 2)
    else { ctx.translate(x + band / 2, y + h / 2); ctx.rotate(-Math.PI / 2); ctx.fillText(label, 0, 0) }
    ctx.restore()
  }
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
  // Formes basiques (étendu)
  { id: 'shp_pentagon',    name: 'Pentagone',    category: 'Formes basiques', defaultW: 100, defaultH: 90, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'shp_octagon',     name: 'Octogone',     category: 'Formes basiques', defaultW: 100, defaultH: 90, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'shp_arrow_right', name: 'Flèche droite',category: 'Formes basiques', defaultW: 120, defaultH: 60, style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'shp_arrow_left',  name: 'Flèche gauche',category: 'Formes basiques', defaultW: 120, defaultH: 60, style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'shp_arrow_up',    name: 'Flèche haut',  category: 'Formes basiques', defaultW: 60,  defaultH: 120,style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'shp_arrow_down',  name: 'Flèche bas',   category: 'Formes basiques', defaultW: 60,  defaultH: 120,style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'shp_cube',        name: 'Cube',         category: 'Formes basiques', defaultW: 100, defaultH: 90, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'shp_step',        name: 'Chevron',      category: 'Formes basiques', defaultW: 120, defaultH: 60, style: { fillColor: '#ffe6cc', strokeColor: '#d79b00' } },
  { id: 'shp_card',        name: 'Carte',        category: 'Formes basiques', defaultW: 120, defaultH: 70, style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'shp_note',        name: 'Note',         category: 'Formes basiques', defaultW: 100, defaultH: 90, style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  // Flux (étendu)
  { id: 'flow_terminator',      name: 'Terminaison',     category: 'Flux', defaultW: 120, defaultH: 50,  style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'flow_predefined',      name: 'Sous-programme',  category: 'Flux', defaultW: 120, defaultH: 60,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'flow_internal_storage',name: 'Stockage interne',category: 'Flux', defaultW: 100, defaultH: 80,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'flow_display',         name: 'Affichage',       category: 'Flux', defaultW: 120, defaultH: 70,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'flow_delay',           name: 'Délai',           category: 'Flux', defaultW: 110, defaultH: 60,  style: { fillColor: '#ffe6cc', strokeColor: '#d79b00' } },
  { id: 'flow_offpage',         name: 'Hors page',       category: 'Flux', defaultW: 90,  defaultH: 90,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'flow_or',              name: 'Ou (logique)',    category: 'Flux', defaultW: 60,  defaultH: 60,  style: { fillColor: '#ffffff', strokeColor: '#000000' } },
  { id: 'flow_summing',         name: 'Jonction',        category: 'Flux', defaultW: 60,  defaultH: 60,  style: { fillColor: '#ffffff', strokeColor: '#000000' } },
  { id: 'flow_manual_op',       name: 'Opération manuelle',category: 'Flux', defaultW: 120, defaultH: 60, style: { fillColor: '#ffe6cc', strokeColor: '#d79b00' } },
  { id: 'flow_card',            name: 'Carte (perfo)',   category: 'Flux', defaultW: 120, defaultH: 70,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  // Entité-association
  { id: 'er_entity',        name: 'Entité',           category: 'Entité-association', defaultW: 140, defaultH: 60, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'er_weak_entity',   name: 'Entité faible',    category: 'Entité-association', defaultW: 140, defaultH: 60, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'er_attribute',     name: 'Attribut',         category: 'Entité-association', defaultW: 110, defaultH: 50, style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'er_key_attribute', name: 'Attribut clé',     category: 'Entité-association', defaultW: 110, defaultH: 50, style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'er_relationship',  name: 'Relation',         category: 'Entité-association', defaultW: 120, defaultH: 70, style: { fillColor: '#ffe6cc', strokeColor: '#d79b00' } },
  // BPMN
  { id: 'bpmn_task',     name: 'Tâche',     category: 'BPMN', defaultW: 120, defaultH: 70, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf', rounded: 10 } },
  { id: 'bpmn_start',    name: 'Début',     category: 'BPMN', defaultW: 50,  defaultH: 50, style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'bpmn_end',      name: 'Fin',       category: 'BPMN', defaultW: 50,  defaultH: 50, style: { fillColor: '#f8cecc', strokeColor: '#b85450' } },
  { id: 'bpmn_event',    name: 'Événement', category: 'BPMN', defaultW: 50,  defaultH: 50, style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  { id: 'bpmn_gateway',  name: 'Passerelle',category: 'BPMN', defaultW: 70,  defaultH: 70, style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  // UML (étendu)
  { id: 'uml_package',   name: 'Paquetage', category: 'UML', defaultW: 160, defaultH: 110, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'uml_component', name: 'Composant', category: 'UML', defaultW: 140, defaultH: 80,  style: { fillColor: '#d5e8d4', strokeColor: '#82b366' } },
  { id: 'uml_node',      name: 'Nœud',      category: 'UML', defaultW: 120, defaultH: 90,  style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'uml_object',    name: 'Objet',     category: 'UML', defaultW: 140, defaultH: 60,  style: { fillColor: '#e1d5e7', strokeColor: '#9673a6' } },
  { id: 'uml_note',      name: 'Note',      category: 'UML', defaultW: 120, defaultH: 80,  style: { fillColor: '#fff2cc', strokeColor: '#d6b656' } },
  // Réseau (étendu)
  { id: 'net_user',     name: 'Utilisateur', category: 'Réseau', defaultW: 60,  defaultH: 80, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'net_laptop',   name: 'Portable',    category: 'Réseau', defaultW: 100, defaultH: 70, style: { fillColor: '#e8eaed', strokeColor: '#5f6368' } },
  { id: 'net_mobile',   name: 'Mobile',      category: 'Réseau', defaultW: 60,  defaultH: 90, style: { fillColor: '#e8eaed', strokeColor: '#5f6368' } },
  { id: 'net_switch',   name: 'Commutateur', category: 'Réseau', defaultW: 110, defaultH: 50, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf' } },
  { id: 'net_printer',  name: 'Imprimante',  category: 'Réseau', defaultW: 80,  defaultH: 80, style: { fillColor: '#e8eaed', strokeColor: '#5f6368' } },
  { id: 'net_wifi',     name: 'Wi-Fi',       category: 'Réseau', defaultW: 70,  defaultH: 60, style: { fillColor: 'none', strokeColor: '#1a73e8' } },
  { id: 'net_internet', name: 'Internet',    category: 'Réseau', defaultW: 120, defaultH: 80, style: { fillColor: '#e8f0fe', strokeColor: '#1a73e8' } },
  // AWS (étendu — rendu en carré arrondi coloré)
  { id: 'aws_dynamodb',  name: 'DynamoDB',     category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#dae8fc', strokeColor: '#1a73e8', rounded: 8 } },
  { id: 'aws_cloudfront',name: 'CloudFront',   category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#e1d5e7', strokeColor: '#9673a6', rounded: 8 } },
  { id: 'aws_sqs',       name: 'SQS',          category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#e1d5e7', strokeColor: '#9673a6', rounded: 8 } },
  { id: 'aws_sns',       name: 'SNS',          category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#e1d5e7', strokeColor: '#9673a6', rounded: 8 } },
  { id: 'aws_apigw',     name: 'API Gateway',  category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#e1d5e7', strokeColor: '#9673a6', rounded: 8 } },
  { id: 'aws_elb',       name: 'Load Balancer',category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#fce8d8', strokeColor: '#e07624', rounded: 8 } },
  { id: 'aws_route53',   name: 'Route 53',     category: 'AWS', defaultW: 80, defaultH: 80, style: { fillColor: '#e1d5e7', strokeColor: '#9673a6', rounded: 8 } },
  // Maquettes (UI)
  { id: 'ui_button',   name: 'Bouton',     category: 'Maquettes', defaultW: 100, defaultH: 36, style: { fillColor: '#1a73e8', strokeColor: '#1557b0' } },
  { id: 'ui_input',    name: 'Champ',      category: 'Maquettes', defaultW: 160, defaultH: 34, style: { fillColor: '#ffffff', strokeColor: '#bdc1c6' } },
  { id: 'ui_checkbox', name: 'Case',       category: 'Maquettes', defaultW: 120, defaultH: 24, style: { fillColor: 'none', strokeColor: '#5f6368' } },
  { id: 'ui_radio',    name: 'Radio',      category: 'Maquettes', defaultW: 120, defaultH: 24, style: { fillColor: 'none', strokeColor: '#5f6368' } },
  { id: 'ui_dropdown', name: 'Liste',      category: 'Maquettes', defaultW: 160, defaultH: 34, style: { fillColor: '#ffffff', strokeColor: '#bdc1c6' } },
  { id: 'ui_browser',  name: 'Navigateur', category: 'Maquettes', defaultW: 240, defaultH: 160,style: { fillColor: '#ffffff', strokeColor: '#5f6368' } },
  { id: 'ui_image',    name: 'Image',      category: 'Maquettes', defaultW: 120, defaultH: 90, style: { fillColor: '#f1f3f4', strokeColor: '#9aa0a6' } },
  // Conteneurs / couloirs (déplacer le conteneur déplace son contenu)
  { id: 'container',   name: 'Conteneur',          category: 'Conteneurs', defaultW: 240, defaultH: 160, style: { fillColor: '#ffffff', strokeColor: '#666666' } },
  { id: 'swimlane_v',  name: 'Couloir vertical',   category: 'Conteneurs', defaultW: 160, defaultH: 240, style: { fillColor: '#ffffff', strokeColor: '#666666' } },
  { id: 'swimlane_h',  name: 'Couloir horizontal', category: 'Conteneurs', defaultW: 280, defaultH: 120, style: { fillColor: '#ffffff', strokeColor: '#666666' } },
  // Ordinateur et Matériel (icônes SVG Repo, collection « Computer and Hardware Duotone », MIT)
  ...HW_STENCILS,
]

export const STENCIL_MAP: Record<string, StencilDef> = Object.fromEntries(STENCILS.map(s => [s.id, s]))

// Stable category identifiers (used for filtering / tab state) decoupled from
// the French fallback labels stored on each StencilDef.
const CATEGORY_ID: Record<string, string> = {
  'Formes basiques':     'basic',
  'Flux':                'flow',
  'Entité-association':  'er',
  'BPMN':                'bpmn',
  'Réseau':              'network',
  'UML':                 'uml',
  'AWS':                 'aws',
  'Kubernetes':          'k8s',
  'Maquettes':           'mockup',
  'Conteneurs':          'container',
  'Matériel':            'hardware',
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
