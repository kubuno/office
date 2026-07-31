// Canvas painting of a picture's FRAME — the drop shadow behind it and the outline
// around it (xlsx `xdr:pic/xdr:spPr/a:ln` for the outline, an outer shadow effect for
// the shadow). Pictures are painted on the grid canvas, not as DOM overlays, so their
// formatting has to be drawn by hand around the existing `drawImage` call.
//
// ── CALL CONTRACT (read before wiring) ───────────────────────────────────────────
// Both functions are PURE drawing helpers: they touch nothing but the 2-D context,
// save/restore it themselves, and read no state.
//
//  1. `rect` is in the SAME coordinate space as the `drawImage` call they wrap, i.e.
//     the picture's content rect ALREADY SHIFTED BY THE SCROLL:
//         const r = imageRect(im)
//         const rect = { x: r.x - scrollLeft, y: r.y - scrollTop, w: r.w, h: r.h }
//     Never pass base (zoom = 1) coordinates: `imageRect` already applies the zoom.
//
//  2. ROTATION IS THE CALLER'S. When the picture is rotated the caller has already
//     translated/rotated/untranslated the context around the rect's centre (exactly
//     what the grid renderer does before `drawImage`). These helpers therefore draw
//     an AXIS-ALIGNED rect and inherit the rotation, so the frame and the shadow turn
//     with the picture — Excel's `rotWithShape` behaviour. Do not pre-rotate `rect`.
//
//  3. ORDER around the picture:
//         drawImageShadow(ctx, rect, { zoom })      // BEFORE  — casts behind
//         ctx.drawImage(el, …)                      //          the picture itself
//         drawImageBorder(ctx, rect, im.border, im.borderWidth ?? 1, zoom)   // AFTER
//     Selection chrome (handles, rotation knob) comes last, as it does today.
//
//  4. Sizes given in MODEL pixels; pass the sheet `zoom` so the frame scales with the
//     grid. Device-pixel-ratio scaling is already in the context transform.

/** A picture's rect in canvas coordinates (scroll already subtracted). */
export interface FrameRect { x: number; y: number; w: number; h: number }

/** Look of the outer shadow. Defaults approximate Excel's "Ombre portée" preset. */
export interface ImageShadowOpts {
  /** Sheet zoom; scales blur and offset so the shadow tracks the picture. Default 1. */
  zoom?: number
  /** Shadow colour, any CSS colour. Default a soft black. */
  colour?: string
  /** Blur radius in model px. Default 8. */
  blur?: number
  /** Offset in model px, along the picture's LOCAL axes (see contract note 2). */
  offsetX?: number
  offsetY?: number
}

const DEFAULT_SHADOW_COLOUR = 'rgba(0, 0, 0, 0.35)'
const DEFAULT_SHADOW_BLUR = 8
const DEFAULT_SHADOW_DX = 3
const DEFAULT_SHADOW_DY = 4

/**
 * Casts an outer shadow around `rect`, to be called BEFORE the picture is drawn.
 *
 * The shadow is produced by filling the picture's own rect with the context's shadow
 * settings on, while a clip cuts the rect itself away (an even-odd path: a padded
 * outer rect minus the picture rect). Only the halo outside the picture survives, so
 * a picture with transparent areas — a logo cut out of a PNG — does not end up
 * sitting on a grey block, and the caster never tints the pixels it is meant to
 * highlight.
 */
export function drawImageShadow(ctx: CanvasRenderingContext2D, rect: FrameRect, opts: ImageShadowOpts = {}): void {
  const { x, y, w, h } = rect
  if (w <= 0 || h <= 0) return
  const zoom = opts.zoom ?? 1
  const blur = Math.max(0, (opts.blur ?? DEFAULT_SHADOW_BLUR) * zoom)
  const dx = (opts.offsetX ?? DEFAULT_SHADOW_DX) * zoom
  const dy = (opts.offsetY ?? DEFAULT_SHADOW_DY) * zoom
  if (blur === 0 && dx === 0 && dy === 0) return

  // Room for the blurred, offset halo on every side.
  const pad = blur * 2 + Math.max(Math.abs(dx), Math.abs(dy)) + 2

  ctx.save()
  ctx.beginPath()
  ctx.rect(x - pad, y - pad, w + pad * 2, h + pad * 2)
  ctx.rect(x, y, w, h)
  ctx.clip('evenodd')
  ctx.shadowColor = opts.colour ?? DEFAULT_SHADOW_COLOUR
  ctx.shadowBlur = blur
  ctx.shadowOffsetX = dx
  ctx.shadowOffsetY = dy
  // Opaque caster: the shadow's strength comes from `shadowColor` alone.
  ctx.fillStyle = '#000'
  ctx.fillRect(x, y, w, h)
  ctx.restore()
}

/**
 * Strokes the picture's outline, to be called AFTER the picture is drawn.
 *
 * No-op when the picture has no outline (`undefined`, `''` or the string 'none', the
 * shared `ObjStyle` convention). The stroke is INSET by half its width so a thick
 * frame stays inside the picture's box — the box the hit-test, the selection chrome
 * and the xlsx anchor all agree on — instead of bleeding over the neighbouring cells.
 * The half-pixel inset that follows from it also keeps a 1 px frame crisp.
 */
export function drawImageBorder(
  ctx: CanvasRenderingContext2D,
  rect: FrameRect,
  colour: string | undefined,
  width = 1,
  zoom = 1,
): void {
  if (!colour || colour === 'none') return
  const { x, y, w, h } = rect
  if (w <= 0 || h <= 0) return
  const lw = Math.max(0.5, width * zoom)
  const inset = lw / 2
  // A frame thicker than the picture would cross itself; clamp to a filled rect.
  if (lw >= Math.min(w, h)) {
    ctx.save()
    ctx.fillStyle = colour
    ctx.fillRect(x, y, w, h)
    ctx.restore()
    return
  }
  ctx.save()
  ctx.strokeStyle = colour
  ctx.lineWidth = lw
  ctx.setLineDash([])
  ctx.lineJoin = 'miter'
  ctx.strokeRect(x + inset, y + inset, w - lw, h - lw)
  ctx.restore()
}
