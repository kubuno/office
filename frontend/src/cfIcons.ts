// Canvas glyphs for conditional-formatting icon sets (Excel-style).
// Icons are drawn as small vector shapes at the left of the cell; the caller
// offsets left-aligned text by the returned width.

type Ctx2D = CanvasRenderingContext2D

const GREEN = '#188038'
const AMBER = '#f9ab00'
const RED = '#d93025'
const GRAY = '#5f6368'
const RAMP3 = [RED, AMBER, GREEN]
const RAMP4 = [RED, '#f56c00', AMBER, GREEN]
const RAMP5 = [RED, '#f56c00', AMBER, '#7cb342', GREEN]
const RED_TO_BLACK = ['#202124', GRAY, '#e67c73', RED]

const ramp = (n: number): string[] => n === 5 ? RAMP5 : n === 4 ? RAMP4 : RAMP3

// Arrow rotation per position (0 = lowest): 0 rad points up, PI points down.
function arrowAngles(n: number): number[] {
  if (n === 4) return [Math.PI, Math.PI * 0.75, Math.PI * 0.25, 0]
  if (n === 5) return [Math.PI, Math.PI * 0.75, Math.PI * 0.5, Math.PI * 0.25, 0]
  return [Math.PI, Math.PI * 0.5, 0]
}

function drawArrow(ctx: Ctx2D, cx: number, cy: number, s: number, angle: number, color: string): void {
  ctx.save()
  ctx.translate(cx, cy)
  ctx.rotate(angle)
  ctx.fillStyle = color
  const w = s * 0.32, head = s * 0.48
  ctx.beginPath()
  ctx.moveTo(0, -s / 2)
  ctx.lineTo(s * 0.44, -s / 2 + head)
  ctx.lineTo(w / 2, -s / 2 + head)
  ctx.lineTo(w / 2, s / 2)
  ctx.lineTo(-w / 2, s / 2)
  ctx.lineTo(-w / 2, -s / 2 + head)
  ctx.lineTo(-s * 0.44, -s / 2 + head)
  ctx.closePath()
  ctx.fill()
  ctx.restore()
}

function drawCircle(ctx: Ctx2D, cx: number, cy: number, s: number, color: string): void {
  ctx.fillStyle = color
  ctx.beginPath()
  ctx.arc(cx, cy, s / 2, 0, Math.PI * 2)
  ctx.fill()
  ctx.strokeStyle = 'rgba(0,0,0,.25)'
  ctx.lineWidth = 1
  ctx.stroke()
}

function drawFlag(ctx: Ctx2D, cx: number, cy: number, s: number, color: string): void {
  const x0 = cx - s * 0.32, top = cy - s / 2
  ctx.strokeStyle = GRAY
  ctx.lineWidth = Math.max(1, s / 9)
  ctx.beginPath()
  ctx.moveTo(x0, top)
  ctx.lineTo(x0, cy + s / 2)
  ctx.stroke()
  ctx.fillStyle = color
  ctx.beginPath()
  ctx.moveTo(x0, top)
  ctx.lineTo(x0 + s * 0.68, top + s * 0.22)
  ctx.lineTo(x0, top + s * 0.44)
  ctx.closePath()
  ctx.fill()
}

// Signal-strength bars (4Rating / 5Rating): idx+1 filled bars out of n.
function drawRating(ctx: Ctx2D, cx: number, cy: number, s: number, idx: number, n: number): void {
  const gap = Math.max(1, s * 0.12)
  const bw = (s - gap * (n - 1)) / n
  for (let i = 0; i < n; i++) {
    const bh = s * (0.3 + 0.7 * i / (n - 1))
    ctx.fillStyle = i <= idx ? '#1a73e8' : '#dadce0'
    ctx.fillRect(cx - s / 2 + i * (bw + gap), cy + s / 2 - bh, bw, bh)
  }
}

// Pie quarters (5Quarters): filled fraction idx/(n-1) of a disc.
function drawQuarters(ctx: Ctx2D, cx: number, cy: number, s: number, idx: number, n: number): void {
  const r = s / 2
  ctx.fillStyle = '#ffffff'
  ctx.beginPath()
  ctx.arc(cx, cy, r, 0, Math.PI * 2)
  ctx.fill()
  const frac = n > 1 ? idx / (n - 1) : 1
  if (frac > 0) {
    ctx.fillStyle = GRAY
    ctx.beginPath()
    ctx.moveTo(cx, cy)
    ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * frac)
    ctx.closePath()
    ctx.fill()
  }
  ctx.strokeStyle = GRAY
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.arc(cx, cy, r, 0, Math.PI * 2)
  ctx.stroke()
}

// 3Signs: diamond (low), triangle (mid), circle (high).
function drawSign(ctx: Ctx2D, cx: number, cy: number, s: number, idx: number): void {
  const color = RAMP3[Math.min(idx, 2)]
  ctx.fillStyle = color
  if (idx === 0) {
    ctx.beginPath()
    ctx.moveTo(cx, cy - s / 2)
    ctx.lineTo(cx + s / 2, cy)
    ctx.lineTo(cx, cy + s / 2)
    ctx.lineTo(cx - s / 2, cy)
    ctx.closePath()
    ctx.fill()
  } else if (idx === 1) {
    ctx.beginPath()
    ctx.moveTo(cx, cy - s / 2)
    ctx.lineTo(cx + s / 2, cy + s / 2)
    ctx.lineTo(cx - s / 2, cy + s / 2)
    ctx.closePath()
    ctx.fill()
  } else {
    drawCircle(ctx, cx, cy, s, color)
  }
}

// 3Symbols / 3Symbols2: cross (low), exclamation (mid), check (high).
function drawSymbol(ctx: Ctx2D, cx: number, cy: number, s: number, idx: number, circled: boolean): void {
  const color = RAMP3[Math.min(idx, 2)]
  const stroke = circled ? '#ffffff' : color
  if (circled) drawCircle(ctx, cx, cy, s, color)
  ctx.strokeStyle = stroke
  ctx.fillStyle = stroke
  ctx.lineWidth = Math.max(1.2, s / 7)
  ctx.lineCap = 'round'
  const r = s * (circled ? 0.24 : 0.36)
  if (idx === 0) { // cross
    ctx.beginPath()
    ctx.moveTo(cx - r, cy - r)
    ctx.lineTo(cx + r, cy + r)
    ctx.moveTo(cx + r, cy - r)
    ctx.lineTo(cx - r, cy + r)
    ctx.stroke()
  } else if (idx === 1) { // exclamation mark
    ctx.beginPath()
    ctx.moveTo(cx, cy - r)
    ctx.lineTo(cx, cy + r * 0.35)
    ctx.stroke()
    ctx.beginPath()
    ctx.arc(cx, cy + r * 0.95, Math.max(1, s / 10), 0, Math.PI * 2)
    ctx.fill()
  } else { // check mark
    ctx.beginPath()
    ctx.moveTo(cx - r, cy + r * 0.1)
    ctx.lineTo(cx - r * 0.25, cy + r * 0.8)
    ctx.lineTo(cx + r, cy - r * 0.7)
    ctx.stroke()
  }
  ctx.lineCap = 'butt'
}

/**
 * Draw one conditional-formatting icon at the left of a cell rect and return
 * the horizontal space consumed (used to inset left-aligned text).
 * `idx` is the icon position (0 = lowest) among `n` icons of `set`.
 */
export function drawCfIcon(
  ctx: Ctx2D,
  icon: { set: string; idx: number; n: number },
  x: number, y: number, h: number, zoom: number,
): number {
  const { set } = icon
  const n = Math.max(3, Math.min(5, icon.n))
  const idx = Math.max(0, Math.min(n - 1, icon.idx))
  const s = Math.max(8, Math.min(12 * zoom, h - 4))
  const cx = x + 3 + s / 2, cy = y + h / 2
  const gray = set.includes('Gray')

  ctx.save()
  if (set.includes('Arrows')) {
    const angle = arrowAngles(n)[idx]
    const color = gray ? GRAY : angle === 0 ? GREEN : angle === Math.PI ? RED : AMBER
    drawArrow(ctx, cx, cy, s, angle, color)
  } else if (set.includes('Rating')) {
    drawRating(ctx, cx, cy, s, idx, n)
  } else if (set.includes('Quarters')) {
    drawQuarters(ctx, cx, cy, s, idx, n)
  } else if (set.includes('Flags')) {
    drawFlag(ctx, cx, cy, s, ramp(n)[idx])
  } else if (set === '3Signs') {
    drawSign(ctx, cx, cy, s, idx)
  } else if (set.startsWith('3Symbols')) {
    drawSymbol(ctx, cx, cy, s, idx, set === '3Symbols')
  } else if (set.includes('RedToBlack')) {
    drawCircle(ctx, cx, cy, s, RED_TO_BLACK[Math.min(idx, 3)])
  } else {
    // Traffic lights and any unknown set: coloured disc on the low→high ramp.
    drawCircle(ctx, cx, cy, s, ramp(n)[idx])
  }
  ctx.restore()
  return s + 6
}
