// Canvas rendering of the visual text effects (outline / fill / shadow / glow /
// reflection). Called by canvas-engine's paint loop for any span that carries a
// `textEffect`, and by the ribbon menu's preview tiles (so the demo IS the
// result). The caller has already set `ctx.font` (and letterSpacing); we
// save/restore everything we touch.
//
// Techniques (classic 2D-canvas compositing):
//  • Glyph sprite: the run is first drawn on an offscreen canvas at the current
//    device scale (crisp under zoom×DPR), fill + centered stroke outline.
//  • Inner shadow: the sprite's INVERSE (paint everywhere, punch the glyphs out
//    with `destination-out`), blurred and offset, composited into the glyphs
//    with `source-atop` — the canonical inner-shadow algorithm.
//  • Reflection: the sprite mirrored below, faded with a linear-gradient alpha
//    mask applied through `destination-in` (smooth, no banding).
//  • Glow / outer shadow: the "shadow offset trick" — draw the sprite far
//    off-screen with shadowOffset bringing only its shadow into view; repeated
//    passes densify the halo.
//  • ⚠️ shadowOffsetX/Y and shadowBlur are in DEVICE space (they ignore the
//    current transform), so every shadow parameter is multiplied by the current
//    scale — otherwise effects would shrink as the user zooms in.
import type { TextEffectVisual, TextOutline } from './model'

type Metrics = { ascent: number; descent: number }

/** Current device scale of the context (zoom × devicePixelRatio, rotation-safe). */
function scaleOf(ctx: CanvasRenderingContext2D): number {
  const t = ctx.getTransform()
  const s = Math.hypot(t.a, t.b)
  return s > 0.01 ? Math.min(8, s) : 1
}

function applyDash(ctx: CanvasRenderingContext2D, dash: TextOutline['dash'], w: number): void {
  switch (dash) {
    case 'dash': ctx.setLineDash([w * 3, w * 2]); break
    case 'dot': ctx.setLineDash([w, w * 1.5]); break
    case 'dashDot': ctx.setLineDash([w * 3, w * 1.5, w, w * 1.5]); break
    default: ctx.setLineDash([])
  }
}

/**
 * Outline + fill, in the Word/LibreOffice order: STROKE first, FILL on top. The
 * stroke is centered on the glyph edge, so the fill covers its inner half and
 * the visible outline is the outer half — exactly how Word renders textOutline.
 */
function drawGlyphs(c: CanvasRenderingContext2D, text: string, x: number, baseline: number, eff: TextEffectVisual, fallback: string): void {
  if (eff.outline) {
    // OOXML width is the full (centered) stroke width; the fill will cover the
    // inner half, leaving width/2 visible outside — same as Word.
    c.lineWidth = Math.max(0.5, eff.outline.width)
    c.strokeStyle = eff.outline.color
    c.lineJoin = 'round'   // default miter(10) spikes on acute glyph joints
    c.miterLimit = 2
    applyDash(c, eff.outline.dash, c.lineWidth)
    c.strokeText(text, x, baseline)
    c.setLineDash([])
  }
  c.fillStyle = eff.fill ?? fallback
  c.fillText(text, x, baseline)
}

interface Sprite { cv: HTMLCanvasElement; pad: number; w: number; h: number; scale: number }

/**
 * Draw the run on an offscreen canvas at the current device scale. The inner
 * shadow (if any) is BAKED into the sprite so the reflection mirrors it too.
 */
function makeSprite(ctx: CanvasRenderingContext2D, text: string, eff: TextEffectVisual, fallback: string, m: Metrics): Sprite | null {
  const scale = scaleOf(ctx)
  const textW = ctx.measureText(text).width
  const pad = Math.ceil(4 + (eff.outline ? eff.outline.width * 2 : 0))
  const w = Math.ceil(textW + pad * 2)
  const h = Math.ceil(m.ascent + m.descent + pad * 2)
  const cv = document.createElement('canvas')
  cv.width = Math.max(1, Math.ceil(w * scale))
  cv.height = Math.max(1, Math.ceil(h * scale))
  const c = cv.getContext('2d')
  if (!c) return null
  c.scale(scale, scale)
  c.font = ctx.font
  c.fontKerning = ctx.fontKerning   // engine forces 'normal'; offscreen defaults 'auto'
  // letterSpacing must carry over or the sprite's advance drifts from layout.
  if (ctx.letterSpacing && ctx.letterSpacing !== '0px') c.letterSpacing = ctx.letterSpacing
  drawGlyphs(c, text, pad, pad + m.ascent, eff, fallback)

  if (eff.shadow?.inner) {
    // Inverse mask: shadow color everywhere EXCEPT the glyphs.
    const inv = document.createElement('canvas')
    inv.width = cv.width; inv.height = cv.height
    const ic = inv.getContext('2d')
    if (ic) {
      ic.fillStyle = eff.shadow.color
      ic.fillRect(0, 0, inv.width, inv.height)
      ic.globalCompositeOperation = 'destination-out'
      ic.drawImage(cv, 0, 0)
      // Blurred inverse, offset by (dx,dy), clipped INSIDE the glyphs.
      c.save()
      c.setTransform(1, 0, 0, 1, 0, 0) // device px from here on
      c.globalCompositeOperation = 'source-atop'
      c.globalAlpha = 0.9
      c.shadowColor = eff.shadow.color
      c.shadowBlur = eff.shadow.blur * scale
      // Offset trick: park the inverse off-canvas, let only its shadow land.
      const OFF = inv.width + 64
      c.shadowOffsetX = OFF + eff.shadow.dx * scale
      c.shadowOffsetY = eff.shadow.dy * scale
      c.drawImage(inv, -OFF, 0)
      c.restore()
    }
  }
  return { cv, pad, w, h, scale }
}

/** Mirrored sprite with a smooth top-to-bottom alpha fade (no banding). */
function makeReflection(sp: Sprite, m: Metrics, opacity: number, size: number): HTMLCanvasElement | null {
  const rcv = document.createElement('canvas')
  rcv.width = sp.cv.width; rcv.height = sp.cv.height
  const rc = rcv.getContext('2d')
  if (!rc) return null
  rc.translate(0, rcv.height)
  rc.scale(1, -1)
  rc.drawImage(sp.cv, 0, 0)
  rc.setTransform(1, 0, 0, 1, 0, 0)
  // After the flip the glyphs' BOTTOM edge sits at y = pad·scale (nearest the
  // real text). Fade from `opacity` there to 0 over `size` of the glyph height.
  const y0 = sp.pad * sp.scale
  const y1 = y0 + Math.max(1, (m.ascent + m.descent) * size * sp.scale)
  const g = rc.createLinearGradient(0, y0, 0, y1)
  g.addColorStop(0, `rgba(0,0,0,${Math.max(0, Math.min(1, opacity))})`)
  g.addColorStop(1, 'rgba(0,0,0,0)')
  rc.globalCompositeOperation = 'destination-in'
  rc.fillStyle = g
  rc.fillRect(0, 0, rcv.width, rcv.height)
  return rcv
}

/** Off-screen offset used by the shadow trick (user-space units). */
const TRICK_OFF = 4096

/**
 * Draw `text` with its visual effects at (x, baseline). `ctx.font` (and
 * letterSpacing) must already be set by the caller.
 */
export function paintEffectText(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  baseline: number,
  eff: TextEffectVisual,
  fallback: string,
  metrics: Metrics,
): void {
  const scale = scaleOf(ctx)

  // Fast path — fill/outline/outer shadow only: no offscreen needed.
  const needsSprite = !!(eff.glow || eff.reflection || eff.shadow?.inner)
  if (!needsSprite) {
    ctx.save()
    if (eff.shadow) {
      ctx.shadowColor = eff.shadow.color
      ctx.shadowBlur = eff.shadow.blur * scale     // device-space: compensate zoom
      ctx.shadowOffsetX = eff.shadow.dx * scale
      ctx.shadowOffsetY = eff.shadow.dy * scale
    }
    drawGlyphs(ctx, text, x, baseline, eff, fallback)
    ctx.restore()
    return
  }

  const sp = makeSprite(ctx, text, eff, fallback, metrics)
  if (!sp) return
  const top = baseline - metrics.ascent - sp.pad
  const left = x - sp.pad

  ctx.save()

  // 1) Reflection first (sits under everything).
  if (eff.reflection) {
    const r = eff.reflection
    const rcv = makeReflection(sp, metrics, r.opacity, r.size)
    // The reflected glyph top (its bottom edge) is at `pad` inside the image:
    // place it `distance` below the descenders.
    if (rcv) ctx.drawImage(rcv, left, baseline + metrics.descent + r.distance - sp.pad, sp.w, sp.h)
  }

  // 2) Glow halo — LibreOffice's algorithm (GlowPrimitive2D): DILATE the glyph
  //    silhouette by radius/2, then blur by radius/2. The dilation comes from a
  //    round-joined stroke of lineWidth=radius (extends radius/2 outward); the
  //    blur from the shadow trick (spec: gaussian σ = shadowBlur/2). Two passes
  //    densify the halo edge (50% → 75% intensity).
  if (eff.glow) {
    const r = Math.max(1, eff.glow.radius)
    const gpad = Math.ceil(sp.pad + r)
    const gw = Math.ceil(sp.w + 2 * (gpad - sp.pad))
    const gh = Math.ceil(sp.h + 2 * (gpad - sp.pad))
    const sil = document.createElement('canvas')
    sil.width = Math.max(1, Math.ceil(gw * scale))
    sil.height = Math.max(1, Math.ceil(gh * scale))
    const sc = sil.getContext('2d')
    if (sc) {
      sc.scale(scale, scale)
      sc.font = ctx.font
      sc.fontKerning = ctx.fontKerning
      if (ctx.letterSpacing && ctx.letterSpacing !== '0px') sc.letterSpacing = ctx.letterSpacing
      sc.fillStyle = eff.glow.color
      sc.strokeStyle = eff.glow.color
      sc.lineWidth = r
      sc.lineJoin = 'round'
      sc.strokeText(text, gpad, gpad + metrics.ascent)
      sc.fillText(text, gpad, gpad + metrics.ascent)
      ctx.save()
      ctx.shadowColor = eff.glow.color
      ctx.shadowBlur = r * scale
      ctx.shadowOffsetX = TRICK_OFF * scale
      ctx.shadowOffsetY = 0
      const gleft = x - gpad
      const gtop = baseline - metrics.ascent - gpad
      for (let i = 0; i < 2; i++) ctx.drawImage(sil, gleft - TRICK_OFF, gtop, gw, gh)
      ctx.restore()
    }
  }

  // 3) Outer drop shadow (can coexist with glow, e.g. WordArt presets).
  if (eff.shadow && !eff.shadow.inner) {
    ctx.save()
    ctx.shadowColor = eff.shadow.color
    ctx.shadowBlur = eff.shadow.blur * scale
    ctx.shadowOffsetX = (TRICK_OFF + eff.shadow.dx) * scale
    ctx.shadowOffsetY = eff.shadow.dy * scale
    ctx.drawImage(sp.cv, left - TRICK_OFF, top, sp.w, sp.h)
    ctx.restore()
  }

  // 4) The glyphs themselves (inner shadow already baked in).
  ctx.drawImage(sp.cv, left, top, sp.w, sp.h)

  ctx.restore()
}
