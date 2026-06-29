// SmartArt layout: positions (fraction coords 0..1) for boxes + connectors of a
// diagram. Pure geometry so it can be unit-tested; the caller turns it into shape
// and line elements. Region covers the central area of the slide.

export type SmartArtKind = 'process' | 'list' | 'cycle' | 'hierarchy' | 'pyramid' | 'matrix'

export interface SmartBox { x: number; y: number; w: number; h: number; shape?: string }
export interface SmartConn { x: number; y: number; x2: number; y2: number }
export interface SmartLayout { boxes: SmartBox[]; connectors: SmartConn[]; shape: string }

const REGION = { x: 0.08, y: 0.24, w: 0.84, h: 0.52 }

export function smartArtLayout(kind: SmartArtKind, n: number): SmartLayout {
  const R = REGION
  const boxes: SmartBox[] = []
  const connectors: SmartConn[] = []
  n = Math.max(1, Math.min(8, n))

  if (kind === 'process') {
    const gap = 0.03
    const bw = (R.w - gap * (n - 1)) / n
    const bh = Math.min(0.22, R.h * 0.6)
    const cy = R.y + (R.h - bh) / 2
    for (let i = 0; i < n; i++) {
      const x = R.x + i * (bw + gap)
      boxes.push({ x, y: cy, w: bw, h: bh })
      if (i > 0) { const px = R.x + (i - 1) * (bw + gap) + bw; connectors.push({ x: px, y: cy + bh / 2, x2: x, y2: cy + bh / 2 }) }
    }
    return { boxes, connectors, shape: 'roundRect' }
  }
  if (kind === 'list') {
    const gap = 0.03
    const bh = (R.h - gap * (n - 1)) / n
    for (let i = 0; i < n; i++) boxes.push({ x: R.x, y: R.y + i * (bh + gap), w: R.w, h: bh })
    return { boxes, connectors, shape: 'roundRect' }
  }
  if (kind === 'cycle') {
    const cxc = R.x + R.w / 2, cyc = R.y + R.h / 2
    const rx = R.w / 2 - 0.09, ry = R.h / 2 - 0.06
    const bw = 0.16, bh = 0.12
    const cpts: { x: number; y: number }[] = []
    for (let i = 0; i < n; i++) {
      const a = -Math.PI / 2 + (i / n) * Math.PI * 2
      const px = cxc + rx * Math.cos(a), py = cyc + ry * Math.sin(a)
      boxes.push({ x: px - bw / 2, y: py - bh / 2, w: bw, h: bh })
      cpts.push({ x: px, y: py })
    }
    for (let i = 0; i < n; i++) { const a = cpts[i], b = cpts[(i + 1) % n]; connectors.push({ x: a.x, y: a.y, x2: b.x, y2: b.y }) }
    return { boxes, connectors, shape: 'ellipse' }
  }
  if (kind === 'hierarchy') {
    const topW = 0.26, topH = 0.14
    const tx = R.x + (R.w - topW) / 2, ty = R.y
    boxes.push({ x: tx, y: ty, w: topW, h: topH })
    const kids = Math.max(1, n - 1)
    const gap = 0.03
    const bw = (R.w - gap * (kids - 1)) / kids
    const bh = 0.14
    const ky = R.y + R.h - bh
    for (let i = 0; i < kids; i++) {
      const x = R.x + i * (bw + gap)
      boxes.push({ x, y: ky, w: bw, h: bh })
      connectors.push({ x: tx + topW / 2, y: ty + topH, x2: x + bw / 2, y2: ky })
    }
    return { boxes, connectors, shape: 'roundRect' }
  }
  if (kind === 'matrix') {
    // Matrice 2×2 (les 4 premiers éléments).
    const gap = 0.02
    const bw = (R.w - gap) / 2, bh = (R.h - gap) / 2
    const pos = [[0, 0], [1, 0], [0, 1], [1, 1]]
    for (let i = 0; i < Math.min(4, n); i++) {
      const [cxi, ryi] = pos[i]
      boxes.push({ x: R.x + cxi * (bw + gap), y: R.y + ryi * (bh + gap), w: bw, h: bh })
    }
    return { boxes, connectors, shape: 'rect' }
  }
  // pyramid : trapèzes empilés, larges en bas.
  const ph = R.h / n
  for (let i = 0; i < n; i++) {
    const frac = (i + 1) / n
    const bw = R.w * frac
    boxes.push({ x: R.x + (R.w - bw) / 2, y: R.y + i * ph, w: bw, h: ph - 0.012, shape: 'trapezoid' })
  }
  return { boxes, connectors, shape: 'trapezoid' }
}
