// Générateur SVG des formes partagées (voir catalog.ts).

import type { ShapeKind } from './catalog'
import { presetOf, presetPath, presetAdjValues } from './preset-engine'

/** Luminance tweak for the DARKEN/LIGHTEN sub-paths (cube and can shading). */
function shadeColour(hex: string, k: number): string {
  if (k === 1 || !/^#[0-9a-fA-F]{6}$/.test(hex)) return hex
  const c = (o: number) => Math.max(0, Math.min(255, Math.round(parseInt(hex.slice(o, o + 2), 16) * k)))
  return `#${[c(1), c(3), c(5)].map(v => v.toString(16).padStart(2, '0')).join('')}`
}

// `sw` = épaisseur de contour en FRACTION de la plus petite dimension (sans unité),
// pour rester nette à toute résolution de génération. Absente = épaisseur par défaut.
export interface ShapeParams { kind: ShapeKind; fill: string; stroke: string; sw?: number }

// Sommets d'un polygone régulier à n côtés (rotDeg = orientation du 1er sommet).
function polyPts(n: number, cx: number, cy: number, rx: number, ry: number, rotDeg: number): string {
  let p = ''
  for (let i = 0; i < n; i++) {
    const a = (Math.PI * 2 * i) / n + (rotDeg * Math.PI) / 180
    p += `${(cx + rx * Math.cos(a)).toFixed(1)},${(cy + ry * Math.sin(a)).toFixed(1)} `
  }
  return p.trim()
}
// Sommets d'une étoile à `spikes` branches (rayons externe oR / interne iR).
function starPts(spikes: number, cx: number, cy: number, oR: number, iR: number): string {
  let p = ''
  for (let i = 0; i < spikes * 2; i++) {
    const r = i % 2 === 0 ? oR : iR
    const a = (Math.PI / spikes) * i - Math.PI / 2
    p += `${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)} `
  }
  return p.trim()
}

export function shapeSvg(
  kind: ShapeKind, w: number, h: number,
  fill = '#dbe7ff', stroke = '#1a73e8', swFrac?: number,
  /**
   * Adjustment fractions (OOXML `avLst`). Absent = the geometry's own defaults,
   * so every existing caller keeps its exact rendering.
   */
  adj?: number[],
): string {
  // `a(i, def)` = the i-th adjustment, or the historical hard-coded proportion.
  const a = (i: number, def: number): number => {
    const v = adj?.[i]
    return typeof v === 'number' && Number.isFinite(v) ? v : def
  }

  // Exact OOXML geometry from LibreOffice's preset data, whenever the kind maps
  // to one. The hand-drawn cases below then only serve lines and connectors.
  const preset = presetOf(kind)
  if (preset) {
    const sw0 = swFrac != null ? Math.max(0.5, Math.min(w, h) * swFrac) : Math.max(1, Math.min(w, h) * 0.0075)
    const subs = presetPath(preset, { w, h, adj: presetAdjValues(preset, adj) })
    const body = subs.map(sp => {
      const f = sp.fill ? shadeColour(fill, sp.shade) : 'none'
      const st = sp.stroke ? stroke : 'none'
      return `<path d="${sp.d.trim()}" fill="${f}" stroke="${st}" stroke-width="${sw0}" stroke-linejoin="round" stroke-linecap="round"/>`
    }).join('')
    // overflow visible: callout tails legitimately reach OUTSIDE the box.
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" style="overflow:visible">${body}</svg>`
  }
  const cx = w / 2, cy = h / 2
  const f = fill, s = stroke
  const m = Math.min(w, h)
  // Épaisseur de contour proportionnelle à la taille : l'apparence reste identique
  // que le SVG soit généré à la taille du nœud ou à la résolution périphérique
  // (rendu net à tout zoom). `swFrac` (fourni par l'import DOCX `<a:ln w>`) prime,
  // sinon ≈1 pt façon Word (avant : ≈2 px, jugé trop épais à l'import).
  const sw = swFrac != null ? Math.max(0.5, m * swFrac) : Math.max(1, m * 0.0075)
  const oR = m / 2 - sw
  const A = `fill="${f}" stroke="${s}" stroke-width="${sw}" stroke-linejoin="round"`
  // Épaisseur des connecteurs : fine et proportionnelle à la taille (façon Word).
  const cs = Math.max(1.5, Math.min(m * 0.045, 3.5))
  const AL = `fill="none" stroke="${s}" stroke-width="${cs.toFixed(2)}" stroke-linecap="round" stroke-linejoin="round"`
  const poly = (pts: string) => `<polygon points="${pts}" ${A}/>`
  const path = (d: string) => `<path d="${d}" ${A}/>`
  const circ = (ccx: number, ccy: number, r: number) => `<circle cx="${ccx.toFixed(1)}" cy="${ccy.toFixed(1)}" r="${r.toFixed(1)}" ${A}/>`
  const sline = (x1: number, y1: number, x2: number, y2: number, sw2 = sw) => `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="${s}" stroke-width="${sw2}" stroke-linecap="round"/>`
  // Croix (signe +) couvrant la boîte, épaisseur de bras `tk`.
  const crossPoly = (tk: number) => {
    const x1 = cx - tk / 2, x2 = cx + tk / 2, y1 = cy - tk / 2, y2 = cy + tk / 2
    return `${x1},${sw} ${x2},${sw} ${x2},${y1} ${w - sw},${y1} ${w - sw},${y2} ${x2},${y2} ${x2},${h - sw} ${x1},${h - sw} ${x1},${y2} ${sw},${y2} ${sw},${y1} ${x1},${y1}`
  }
  // Têtes de flèche FINES pour les connecteurs (traits) : taille absolue
  // (`userSpaceOnUse`) proportionnelle à la forme et plafonnée, donc nettes en
  // vignette comme à taille réelle (au lieu de têtes géantes « blob »).
  const ahS = Math.max(5, Math.min(m * 0.3, 20))
  const arrowHead = `<defs><marker id="ah" markerUnits="userSpaceOnUse" markerWidth="${ahS.toFixed(1)}" markerHeight="${ahS.toFixed(1)}" refX="${(ahS * 0.88).toFixed(1)}" refY="${(ahS / 2).toFixed(1)}" orient="auto"><path d="M0,0 L${ahS.toFixed(1)},${(ahS / 2).toFixed(1)} L0,${ahS.toFixed(1)} Z" fill="${s}"/></marker><marker id="ah0" markerUnits="userSpaceOnUse" markerWidth="${ahS.toFixed(1)}" markerHeight="${ahS.toFixed(1)}" refX="${(ahS * 0.12).toFixed(1)}" refY="${(ahS / 2).toFixed(1)}" orient="auto"><path d="M${ahS.toFixed(1)},0 L0,${(ahS / 2).toFixed(1)} L${ahS.toFixed(1)},${ahS.toFixed(1)} Z" fill="${s}"/></marker></defs>`
  // Tête ÉPAISSE pour les flèches courbes/circulaires (proportionnelle au fût épais).
  const arrowHeadThick = `<defs><marker id="aht" markerWidth="2.6" markerHeight="2.6" refX="2" refY="1.3" orient="auto"><path d="M0,0 L2.6,1.3 L0,2.6 Z" fill="${s}"/></marker></defs>`
  let body = ''
  switch (kind) {
    // ── Traits ────────────────────────────────────────────────────────────────
    case 'line':     body = sline(sw, h - sw, w - sw, sw, cs); break
    case 'lineArrow': body = `${arrowHead}<line x1="${sw}" y1="${h - sw}" x2="${w - sw}" y2="${sw}" ${AL} marker-end="url(#ah)"/>`; break
    case 'lineDouble': body = `${arrowHead}<line x1="${sw}" y1="${h - sw}" x2="${w - sw}" y2="${sw}" ${AL} marker-start="url(#ah0)" marker-end="url(#ah)"/>`; break
    case 'elbowConnector': body = `<polyline points="${sw},${h - sw} ${cx},${h - sw} ${cx},${sw} ${w - sw},${sw}" ${AL}/>`; break
    case 'curveConnector': body = `<path d="M ${sw},${h - sw} C ${w * 0.1},${h * 0.15} ${w * 0.9},${h * 0.85} ${w - sw},${sw}" ${AL}/>`; break
    case 'curve': body = `<path d="M ${sw},${h - sw} Q ${cx},${sw} ${w - sw},${h - sw}" ${AL}/>`; break
    // ── Rectangles ────────────────────────────────────────────────────────────
    case 'rect':      body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>`; break
    case 'roundRect': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${m * 0.14}" ${A}/>`; break
    case 'snipRect': { const c = m * a(0, 0.22); body = poly(`${sw},${sw} ${w - c},${sw} ${w - sw},${c} ${w - sw},${h - sw} ${sw},${h - sw}`); break }
    case 'roundRect1': { const c = m * a(0, 0.28); body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - sw},${sw} L ${w - sw},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'frame': { const t = m * a(0, 0.16); body = `<path fill-rule="evenodd" d="M ${sw},${sw} H ${w - sw} V ${h - sw} H ${sw} Z M ${sw + t},${sw + t} H ${w - sw - t} V ${h - sw - t} H ${sw + t} Z" ${A}/>`; break }
    // ── Formes de base ────────────────────────────────────────────────────────
    case 'ellipse':   body = `<ellipse cx="${cx}" cy="${cy}" rx="${cx - sw}" ry="${cy - sw}" ${A}/>`; break
    case 'triangle':  body = poly(`${cx},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break
    case 'rtTriangle': body = poly(`${sw},${sw} ${sw},${h - sw} ${w - sw},${h - sw}`); break
    case 'parallelogram': { const o = w * a(0, 0.22); body = poly(`${o},${sw} ${w - sw},${sw} ${w - o},${h - sw} ${sw},${h - sw}`); break }
    case 'trapezoid': { const o = w * a(0, 0.22); body = poly(`${o},${sw} ${w - o},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break }
    case 'diamond':   body = poly(`${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}`); break
    case 'pentagon':  body = poly(polyPts(5, cx, cy, cx - sw, cy - sw, -90)); break
    case 'hexagon':   body = poly(polyPts(6, cx, cy, cx - sw, cy - sw, 0)); break
    case 'heptagon':  body = poly(polyPts(7, cx, cy, cx - sw, cy - sw, -90)); break
    case 'octagon':   body = poly(polyPts(8, cx, cy, cx - sw, cy - sw, 22.5)); break
    case 'cross':     body = poly(crossPoly(m * a(0, 0.36))); break
    case 'cylinder': { const ry = Math.min(h * 0.14, cy - sw), topY = sw + ry, botY = h - sw - ry; body = path(`M ${sw},${topY} L ${sw},${botY} A ${cx - sw} ${ry} 0 0 0 ${w - sw},${botY} L ${w - sw},${topY}`) + `<ellipse cx="${cx}" cy="${topY}" rx="${cx - sw}" ry="${ry}" ${A}/>`; break }
    case 'cube': { const d = m * a(0, 0.26); body = poly(`${sw},${d} ${w - d},${d} ${w - d},${h - sw} ${sw},${h - sw}`) + poly(`${sw},${d} ${d},${sw} ${w - sw},${sw} ${w - d},${d}`) + poly(`${w - d},${d} ${w - sw},${sw} ${w - sw},${h - d} ${w - d},${h - sw}`); break }
    case 'heart': body = path(`M ${cx},${h * 0.85} C ${w * 0.1},${h * 0.55} ${w * 0.18},${h * 0.12} ${cx},${h * 0.32} C ${w * 0.82},${h * 0.12} ${w * 0.9},${h * 0.55} ${cx},${h * 0.85} Z`); break
    case 'lightning': body = poly(`${w * 0.42},${sw} ${w * 0.66},${h * 0.42} ${w * 0.5},${h * 0.46} ${w * 0.7},${h - sw} ${w * 0.32},${h * 0.56} ${w * 0.48},${h * 0.52} ${w * 0.3},${sw}`); break
    case 'sun': { const r = m * 0.26; let rays = ''; for (let i = 0; i < 12; i++) { const a = (Math.PI * 2 * i) / 12; rays += sline(cx + r * 1.18 * Math.cos(a), cy + r * 1.18 * Math.sin(a), cx + r * 1.5 * Math.cos(a), cy + r * 1.5 * Math.sin(a)) } body = rays + circ(cx, cy, r); break }
    case 'moon': body = path(`M ${w * 0.72},${sw} A ${cx} ${cy - sw} 0 1 0 ${w * 0.72},${h - sw} A ${cx * 0.66} ${cy - sw} 0 1 1 ${w * 0.72},${sw} Z`); break
    case 'cloud': body = path(`M ${w * 0.30},${h * 0.80} C ${w * 0.12},${h * 0.80} ${w * 0.10},${h * 0.55} ${w * 0.24},${h * 0.50} C ${w * 0.20},${h * 0.30} ${w * 0.45},${h * 0.22} ${w * 0.50},${h * 0.38} C ${w * 0.58},${h * 0.20} ${w * 0.85},${h * 0.26} ${w * 0.80},${h * 0.48} C ${w * 0.95},${h * 0.50} ${w * 0.93},${h * 0.78} ${w * 0.74},${h * 0.80} Z`); break
    case 'smiley': { const r = m / 2 - sw; body = circ(cx, cy, r) + `<circle cx="${(cx - r * 0.35).toFixed(1)}" cy="${(cy - r * 0.2).toFixed(1)}" r="${(r * 0.09).toFixed(1)}" fill="${s}"/>` + `<circle cx="${(cx + r * 0.35).toFixed(1)}" cy="${(cy - r * 0.2).toFixed(1)}" r="${(r * 0.09).toFixed(1)}" fill="${s}"/>` + `<path d="M ${cx - r * 0.42},${cy + r * 0.18} Q ${cx},${cy + r * 0.6} ${cx + r * 0.42},${cy + r * 0.18}" fill="none" stroke="${s}" stroke-width="${sw}" stroke-linecap="round"/>`; break }
    case 'arc': body = path(`M ${w - sw},${cy} A ${cx - sw} ${cy - sw} 0 0 0 ${cx},${sw} L ${cx},${cy} Z`); break
    case 'donut': { const ro = oR, ri = ro * a(0, 0.55); body = `<path fill-rule="evenodd" ${A} d="M ${cx - ro},${cy} a ${ro} ${ro} 0 1 0 ${ro * 2},0 a ${ro} ${ro} 0 1 0 ${-ro * 2},0 Z M ${cx - ri},${cy} a ${ri} ${ri} 0 1 1 ${ri * 2},0 a ${ri} ${ri} 0 1 1 ${-ri * 2},0 Z"/>`; break }
    case 'noSymbol': { const ro = oR, ri = ro * 0.72, off = ri * Math.SQRT1_2; body = `<path fill-rule="evenodd" ${A} d="M ${cx - ro},${cy} a ${ro} ${ro} 0 1 0 ${ro * 2},0 a ${ro} ${ro} 0 1 0 ${-ro * 2},0 Z M ${cx - ri},${cy} a ${ri} ${ri} 0 1 1 ${ri * 2},0 a ${ri} ${ri} 0 1 1 ${-ri * 2},0 Z"/>` + sline(cx - off, cy + off, cx + off, cy - off, ro - ri); break }
    case 'leftBrace':  body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w - sw},${sw} Q ${cx},${sw} ${cx},${cy * 0.5} Q ${cx},${cy} ${sw},${cy} Q ${cx},${cy} ${cx},${cy * 1.5} Q ${cx},${h - sw} ${w - sw},${h - sw}"/>`; break
    case 'rightBrace': body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${sw},${sw} Q ${cx},${sw} ${cx},${cy * 0.5} Q ${cx},${cy} ${w - sw},${cy} Q ${cx},${cy} ${cx},${cy * 1.5} Q ${cx},${h - sw} ${sw},${h - sw}"/>`; break
    case 'leftBracket':  body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.6},${sw} L ${sw},${sw} L ${sw},${h - sw} L ${w * 0.6},${h - sw}"/>`; break
    case 'rightBracket': body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.4},${sw} L ${w - sw},${sw} L ${w - sw},${h - sw} L ${w * 0.4},${h - sw}"/>`; break
    // ── Flèches pleines ───────────────────────────────────────────────────────
    case 'arrow': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${sw},${cy - sh / 2} ${w - hw},${cy - sh / 2} ${w - hw},${sw} ${w - sw},${cy} ${w - hw},${h - sw} ${w - hw},${cy + sh / 2} ${sw},${cy + sh / 2}`); break }
    case 'arrowLeft': { const sh = h * a(0, 0.34), hw = w * a(1, 0.4); body = poly(`${w - sw},${cy - sh / 2} ${hw},${cy - sh / 2} ${hw},${sw} ${sw},${cy} ${hw},${h - sw} ${hw},${cy + sh / 2} ${w - sw},${cy + sh / 2}`); break }
    case 'arrowUp': { const sv = w * 0.34, hh = h * 0.4; body = poly(`${cx - sv / 2},${h - sw} ${cx - sv / 2},${hh} ${sw},${hh} ${cx},${sw} ${w - sw},${hh} ${cx + sv / 2},${hh} ${cx + sv / 2},${h - sw}`); break }
    case 'arrowDown': { const sv = w * 0.34, hh = h * 0.6; body = poly(`${cx - sv / 2},${sw} ${cx - sv / 2},${hh} ${sw},${hh} ${cx},${h - sw} ${w - sw},${hh} ${cx + sv / 2},${hh} ${cx + sv / 2},${sw}`); break }
    case 'arrowLeftRight': { const sh = h * 0.34, hw = w * 0.24, sv = h * 0.17; body = poly(`${sw},${cy} ${hw},${cy - sh / 2} ${hw},${cy - sv} ${w - hw},${cy - sv} ${w - hw},${cy - sh / 2} ${w - sw},${cy} ${w - hw},${cy + sh / 2} ${w - hw},${cy + sv} ${hw},${cy + sv} ${hw},${cy + sh / 2}`); break }
    case 'arrowUpDown': { const hw = w * 0.34, ss = w * 0.17, vh = h * 0.24; body = poly(`${cx},${sw} ${cx + hw},${vh} ${cx + ss},${vh} ${cx + ss},${h - vh} ${cx + hw},${h - vh} ${cx},${h - sw} ${cx - hw},${h - vh} ${cx - ss},${h - vh} ${cx - ss},${vh} ${cx - hw},${vh}`); break }
    case 'arrowQuad': { const sh = m * 0.13, hl = m * 0.24, hx = w * 0.28, hy = h * 0.28; body = poly(`${cx},${sw} ${cx + hl},${hy} ${cx + sh},${hy} ${cx + sh},${cy - sh} ${w - hx},${cy - sh} ${w - hx},${cy - hl} ${w - sw},${cy} ${w - hx},${cy + hl} ${w - hx},${cy + sh} ${cx + sh},${cy + sh} ${cx + sh},${h - hy} ${cx + hl},${h - hy} ${cx},${h - sw} ${cx - hl},${h - hy} ${cx - sh},${h - hy} ${cx - sh},${cy + sh} ${hx},${cy + sh} ${hx},${cy + hl} ${sw},${cy} ${hx},${cy - hl} ${hx},${cy - sh} ${cx - sh},${cy - sh} ${cx - sh},${hy} ${cx - hl},${hy}`); break }
    case 'chevron': { const o = w * a(0, 0.28); body = poly(`${sw},${sw} ${w - o},${sw} ${w - sw},${cy} ${w - o},${h - sw} ${sw},${h - sw} ${o},${cy}`); break }
    case 'pentagonArrow': { const o = w * 0.3; body = poly(`${sw},${sw} ${w - o},${sw} ${w - sw},${cy} ${w - o},${h - sw} ${sw},${h - sw}`); break }
    case 'bentArrow': { const yTop = h * 0.16, t = m * 0.2, mid = yTop + t / 2, hh = t * 1.5, bx = w * 0.64; body = poly(`${sw},${h - sw} ${sw},${yTop} ${bx},${yTop} ${bx},${mid - hh} ${w - sw},${mid} ${bx},${mid + hh} ${bx},${yTop + t} ${sw + t},${yTop + t} ${sw + t},${h - sw}`); break }
    // ── Formes d'équation ─────────────────────────────────────────────────────
    case 'mathPlus':  body = poly(crossPoly(m * 0.28)); break
    case 'mathMinus': body = `<rect x="${w * 0.14}" y="${cy - m * 0.1}" width="${w * 0.72}" height="${m * 0.2}" rx="${m * 0.05}" ${A}/>`; break
    case 'mathMultiply': { const tw = m * 0.2; body = sline(w * 0.22, h * 0.22, w * 0.78, h * 0.78, tw) + sline(w * 0.78, h * 0.22, w * 0.22, h * 0.78, tw); break }
    case 'mathDivide': { const bh = m * 0.14, r = m * 0.09; body = `<rect x="${w * 0.16}" y="${cy - bh / 2}" width="${w * 0.68}" height="${bh}" rx="${bh * 0.3}" ${A}/>` + circ(cx, cy - h * 0.26, r) + circ(cx, cy + h * 0.26, r); break }
    case 'mathEqual': { const bh = m * 0.15, g = m * 0.16; body = `<rect x="${w * 0.14}" y="${cy - g / 2 - bh}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/><rect x="${w * 0.14}" y="${cy + g / 2}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/>`; break }
    case 'mathNotEqual': { const bh = m * 0.14, g = m * 0.16; body = `<rect x="${w * 0.14}" y="${cy - g / 2 - bh}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/><rect x="${w * 0.14}" y="${cy + g / 2}" width="${w * 0.72}" height="${bh}" rx="${bh * 0.3}" ${A}/>` + sline(w * 0.62, h * 0.16, w * 0.38, h * 0.84, m * 0.1); break }
    // ── Organigrammes ─────────────────────────────────────────────────────────
    case 'flowProcess':    body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>`; break
    case 'flowAltProcess': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${m * 0.22}" ${A}/>`; break
    case 'flowDecision':   body = poly(`${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}`); break
    case 'flowData':       { const o = w * 0.22; body = poly(`${o},${sw} ${w - sw},${sw} ${w - o},${h - sw} ${sw},${h - sw}`); break }
    case 'flowPredefined': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>` + sline(w * 0.12, sw, w * 0.12, h - sw) + sline(w * 0.88, sw, w * 0.88, h - sw); break
    case 'flowInternal':   body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>` + sline(sw, h * 0.26, w - sw, h * 0.26) + sline(w * 0.2, sw, w * 0.2, h - sw); break
    case 'flowDocument':   body = path(`M ${sw},${sw} L ${w - sw},${sw} L ${w - sw},${h * 0.8} Q ${w * 0.75},${h * 0.97} ${cx},${h * 0.8} Q ${w * 0.25},${h * 0.63} ${sw},${h * 0.8} Z`); break
    case 'flowMultidoc': { const o = m * 0.1; body = `<rect x="${sw + 2 * o}" y="${sw}" width="${w - 2 * sw - 2 * o}" height="${h * 0.55}" ${A}/>` + `<rect x="${sw + o}" y="${sw + o}" width="${w - 2 * sw - 2 * o}" height="${h * 0.58}" ${A}/>` + path(`M ${sw},${sw + 2 * o} L ${w - sw - 2 * o},${sw + 2 * o} L ${w - sw - 2 * o},${h * 0.78} Q ${(w - 2 * o) * 0.62},${h * 0.95} ${(w - 2 * o) / 2},${h * 0.78} Q ${w * 0.25},${h * 0.62} ${sw},${h * 0.78} Z`); break }
    case 'flowTerminator': body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" rx="${(h - 2 * sw) / 2}" ${A}/>`; break
    case 'flowPreparation': body = poly(`${w * 0.16},${sw} ${w - w * 0.16},${sw} ${w - sw},${cy} ${w - w * 0.16},${h - sw} ${w * 0.16},${h - sw} ${sw},${cy}`); break
    case 'flowManualInput': body = poly(`${sw},${h * 0.28} ${w - sw},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break
    case 'flowManualOp': { const o = w * 0.18; body = poly(`${sw},${sw} ${w - sw},${sw} ${w - o},${h - sw} ${o},${h - sw}`); break }
    case 'flowConnector': body = circ(cx, cy, m / 2 - sw); break
    case 'flowCard': { const c = m * 0.22; body = poly(`${c},${sw} ${w - sw},${sw} ${w - sw},${h - sw} ${sw},${h - sw} ${sw},${c}`); break }
    case 'flowOr': { const r = m / 2 - sw; body = circ(cx, cy, r) + sline(cx - r, cy, cx + r, cy) + sline(cx, cy - r, cx, cy + r); break }
    case 'flowSumming': { const r = m / 2 - sw, o = r * Math.SQRT1_2; body = circ(cx, cy, r) + sline(cx - o, cy - o, cx + o, cy + o) + sline(cx + o, cy - o, cx - o, cy + o); break }
    case 'flowCollate': body = poly(`${sw},${sw} ${w - sw},${sw} ${cx},${cy}`) + poly(`${sw},${h - sw} ${w - sw},${h - sw} ${cx},${cy}`); break
    case 'flowExtract': body = poly(`${cx},${sw} ${w - sw},${h - sw} ${sw},${h - sw}`); break
    case 'flowMerge':   body = poly(`${sw},${sw} ${w - sw},${sw} ${cx},${h - sw}`); break
    case 'flowStored':  body = path(`M ${w * 0.14},${sw} L ${w - sw},${sw} Q ${w - w * 0.16},${cy} ${w - sw},${h - sw} L ${w * 0.14},${h - sw} Q ${w * 0.26},${cy} ${w * 0.14},${sw} Z`); break
    case 'flowDelay':   body = path(`M ${sw},${sw} L ${cx},${sw} A ${cx - sw} ${cy - sw} 0 0 1 ${cx},${h - sw} L ${sw},${h - sw} Z`); break
    case 'flowOffPage': body = poly(`${sw},${sw} ${w - sw},${sw} ${w - sw},${h * 0.65} ${cx},${h - sw} ${sw},${h * 0.65}`); break
    // ── Étoiles et bannières ──────────────────────────────────────────────────
    case 'star4':  body = poly(starPts(4, cx, cy, oR, oR * a(0, 0.38))); break
    case 'star':   body = poly(starPts(5, cx, cy, oR, oR * 0.42)); break
    case 'star6':  body = poly(starPts(6, cx, cy, oR, oR * a(0, 0.5))); break
    case 'star8':  body = poly(starPts(8, cx, cy, oR, oR * a(0, 0.6))); break
    case 'star16':  body = poly(starPts(16, cx, cy, oR, oR * a(0, 0.78))); break
    case 'star24':  body = poly(starPts(24, cx, cy, oR, oR * a(0, 0.82))); break
    case 'star32':  body = poly(starPts(32, cx, cy, oR, oR * a(0, 0.85))); break
    case 'explosion1': body = poly(starPts(10, cx, cy, oR, oR * 0.42)); break
    case 'explosion2': body = poly(starPts(14, cx, cy, oR, oR * 0.55)); break
    case 'ribbon': { const ty = h * 0.25, by = h * 0.75, notch = w * 0.08; body = poly(`${sw},${ty} ${w - sw},${ty} ${w - sw - notch},${cy} ${w - sw},${by} ${sw},${by} ${sw + notch},${cy}`); break }
    case 'ribbonDown': body = `<rect x="${w * 0.2}" y="${sw}" width="${w * 0.6}" height="${h * 0.55}" ${A}/>` + poly(`${w * 0.2},${h * 0.42} ${w * 0.1},${h - sw} ${w * 0.2},${h * 0.78}`) + poly(`${w * 0.8},${h * 0.42} ${w * 0.9},${h - sw} ${w * 0.8},${h * 0.78}`); break
    case 'wave': { const a = h * 0.16; body = path(`M ${sw},${h * 0.3} Q ${w * 0.25},${h * 0.3 - a} ${cx},${h * 0.3} T ${w - sw},${h * 0.3} L ${w - sw},${h * 0.7} Q ${w * 0.75},${h * 0.7 + a} ${cx},${h * 0.7} T ${sw},${h * 0.7} Z`); break }
    // ── Bulles et légendes ────────────────────────────────────────────────────
    case 'calloutRect': { const by = h * a(0, 0.7), tip = w * a(1, 0.2)
      body = path(`M ${sw},${sw} L ${w - sw},${sw} L ${w - sw},${by} L ${tip + w * 0.22},${by} L ${tip},${h - sw} L ${tip + w * 0.06},${by} L ${sw},${by} Z`); break }
    case 'calloutRoundRect': { const c = m * 0.16; body = path(`M ${sw + c},${sw} L ${w - sw - c},${sw} Q ${w - sw},${sw} ${w - sw},${sw + c} L ${w - sw},${h * 0.7 - c} Q ${w - sw},${h * 0.7} ${w - sw - c},${h * 0.7} L ${w * 0.42},${h * 0.7} L ${w * 0.2},${h - sw} L ${w * 0.26},${h * 0.7} L ${sw + c},${h * 0.7} Q ${sw},${h * 0.7} ${sw},${h * 0.7 - c} L ${sw},${sw + c} Q ${sw},${sw} ${sw + c},${sw} Z`); break }
    case 'calloutOval': body = `<ellipse cx="${cx}" cy="${h * 0.4}" rx="${cx - sw}" ry="${h * 0.36}" ${A}/>` + poly(`${w * 0.3},${h * 0.68} ${w * 0.2},${h - sw} ${w * 0.45},${h * 0.72}`); break
    case 'calloutCloud': body = path(`M ${w * 0.30},${h * 0.66} C ${w * 0.12},${h * 0.66} ${w * 0.10},${h * 0.42} ${w * 0.24},${h * 0.38} C ${w * 0.20},${h * 0.2} ${w * 0.45},${h * 0.12} ${w * 0.50},${h * 0.28} C ${w * 0.58},${h * 0.1} ${w * 0.85},${h * 0.16} ${w * 0.80},${h * 0.38} C ${w * 0.95},${h * 0.4} ${w * 0.93},${h * 0.64} ${w * 0.74},${h * 0.66} Z`) + circ(w * 0.3, h * 0.78, m * 0.05) + circ(w * 0.22, h * 0.9, m * 0.032); break
    case 'lineCallout': body = `<rect x="${w * 0.28}" y="${sw}" width="${w - sw - w * 0.28}" height="${h * 0.6}" ${A}/>` + sline(w * 0.28, h * 0.5, sw, h - sw, sw) + circ(sw + 2, h - sw - 2, 2.5); break
    case 'calloutLine2': body = `<rect x="${w * 0.35}" y="${sw}" width="${w - sw - w * 0.35}" height="${h * 0.55}" ${A}/>` + `<polyline points="${w * 0.35},${h * 0.4} ${w * 0.18},${h * 0.75} ${sw},${h - sw}" fill="none" stroke="${s}" stroke-width="${sw}"/>` + circ(sw + 2, h - sw - 2, 2.5); break
    case 'calloutLineAccent': body = `<rect x="${w * 0.35}" y="${sw}" width="${w - sw - w * 0.35}" height="${h * 0.55}" ${A}/>` + `<rect x="${w * 0.35}" y="${sw}" width="${m * 0.045}" height="${h * 0.55}" fill="${s}"/>` + sline(w * 0.35, h * 0.5, sw, h - sw, sw) + circ(sw + 2, h - sw - 2, 2.5); break
    // ── Traits (flèches sur connecteurs) ──────────────────────────────────────
    case 'elbowArrow': body = `${arrowHead}<polyline points="${sw},${h - sw} ${cx},${h - sw} ${cx},${sw} ${w - sw},${sw}" ${AL} marker-end="url(#ah)"/>`; break
    case 'elbowDoubleArrow': body = `${arrowHead}<polyline points="${sw},${h - sw} ${cx},${h - sw} ${cx},${sw} ${w - sw},${sw}" ${AL} marker-start="url(#ah0)" marker-end="url(#ah)"/>`; break
    case 'curveArrow': body = `${arrowHead}<path d="M ${sw},${h - sw} C ${w * 0.1},${h * 0.15} ${w * 0.9},${h * 0.85} ${w - sw},${sw}" ${AL} marker-end="url(#ah)"/>`; break
    case 'curveDoubleArrow': body = `${arrowHead}<path d="M ${sw},${h - sw} C ${w * 0.1},${h * 0.15} ${w * 0.9},${h * 0.85} ${w - sw},${sw}" ${AL} marker-start="url(#ah0)" marker-end="url(#ah)"/>`; break
    // ── Rectangles (variantes coins) ──────────────────────────────────────────
    case 'snip2SameRect': { const c = m * 0.22; body = poly(`${c},${sw} ${w - c},${sw} ${w - sw},${c} ${w - sw},${h - sw} ${sw},${h - sw} ${sw},${c}`); break }
    case 'snip2DiagRect': { const c = m * 0.22; body = poly(`${sw},${sw} ${w - c},${sw} ${w - sw},${c} ${w - sw},${h - sw} ${c},${h - sw} ${sw},${h - c}`); break }
    case 'snipRoundRect': { const c = m * 0.26; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - c},${sw} L ${w - sw},${c} L ${w - sw},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'round2SameRect': { const c = m * 0.26; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - c},${sw} A ${c} ${c} 0 0 1 ${w - sw},${c} L ${w - sw},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'round2DiagRect': { const c = m * 0.26; body = path(`M ${sw},${c} A ${c} ${c} 0 0 1 ${c},${sw} L ${w - sw},${sw} L ${w - sw},${h - c} A ${c} ${c} 0 0 1 ${w - c},${h - sw} L ${sw},${h - sw} Z`); break }
    case 'plaque': { const c = m * 0.2; body = path(`M ${c},${sw} L ${w - c},${sw} A ${c} ${c} 0 0 0 ${w - sw},${c} L ${w - sw},${h - c} A ${c} ${c} 0 0 0 ${w - c},${h - sw} L ${c},${h - sw} A ${c} ${c} 0 0 0 ${sw},${h - c} L ${sw},${c} A ${c} ${c} 0 0 0 ${c},${sw} Z`); break }
    // ── Formes de base (suite) ────────────────────────────────────────────────
    case 'decagon':   body = poly(polyPts(10, cx, cy, cx - sw, cy - sw, -90)); break
    case 'dodecagon': body = poly(polyPts(12, cx, cy, cx - sw, cy - sw, 0)); break
    case 'pie':   body = path(`M ${cx},${cy} L ${w - sw},${cy} A ${cx - sw} ${cy - sw} 0 1 1 ${cx},${sw} Z`); break
    case 'chord': body = path(`M ${(cx + (cx - sw) * Math.cos(-Math.PI / 4)).toFixed(1)},${(cy + (cy - sw) * Math.sin(-Math.PI / 4)).toFixed(1)} A ${cx - sw} ${cy - sw} 0 1 1 ${(cx + (cx - sw) * Math.cos(Math.PI * 0.75)).toFixed(1)},${(cy + (cy - sw) * Math.sin(Math.PI * 0.75)).toFixed(1)} Z`); break
    case 'teardrop': body = path(`M ${cx},${sw} C ${w * 0.86},${sw} ${w - sw},${h * 0.14} ${w - sw},${cy} A ${cx - sw} ${cy - sw} 0 1 1 ${cx},${sw} Z`); break
    case 'halfFrame': { const t = m * 0.18; body = poly(`${sw},${sw} ${w - sw},${sw} ${w - t},${t} ${t},${t} ${t},${h - t} ${sw},${h - sw}`); break }
    case 'corner': { const t = m * 0.4; body = poly(`${sw},${sw} ${t},${sw} ${t},${h - t} ${w - sw},${h - t} ${w - sw},${h - sw} ${sw},${h - sw}`); break }
    case 'diagStripe': body = poly(`${sw},${h - sw} ${sw},${h * 0.45} ${w * 0.55},${h - sw}`); break
    case 'bevel': { const t = m * 0.16; body = `<rect x="${sw}" y="${sw}" width="${w - 2 * sw}" height="${h - 2 * sw}" ${A}/>` + `<rect x="${sw + t}" y="${sw + t}" width="${w - 2 * sw - 2 * t}" height="${h - 2 * sw - 2 * t}" fill="none" stroke="${s}" stroke-width="${sw}"/>` + sline(sw, sw, sw + t, sw + t) + sline(w - sw, sw, w - sw - t, sw + t) + sline(sw, h - sw, sw + t, h - sw - t) + sline(w - sw, h - sw, w - sw - t, h - sw - t); break }
    case 'blockArc': { const t = m * 0.24; body = path(`M ${sw},${cy} A ${cx - sw} ${cy - sw} 0 0 1 ${w - sw},${cy} L ${w - sw - t},${cy} A ${cx - sw - t} ${cy - sw - t} 0 0 0 ${sw + t},${cy} Z`); break }
    case 'foldedCorner': { const c = m * 0.22; body = path(`M ${sw},${sw} L ${w - sw},${sw} L ${w - sw},${h - c} L ${w - c},${h - sw} L ${sw},${h - sw} Z`) + poly(`${w - c},${h - sw} ${w - c},${h - c} ${w - sw},${h - c}`); break }
    case 'doubleBracket': { const c = m * 0.18; body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.32},${sw} L ${sw + c},${sw} Q ${sw},${sw} ${sw},${sw + c} L ${sw},${h - sw - c} Q ${sw},${h - sw} ${sw + c},${h - sw} L ${w * 0.32},${h - sw}"/>` + `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.68},${sw} L ${w - sw - c},${sw} Q ${w - sw},${sw} ${w - sw},${sw + c} L ${w - sw},${h - sw - c} Q ${w - sw},${h - sw} ${w - sw - c},${h - sw} L ${w * 0.68},${h - sw}"/>`; break }
    case 'doubleBrace': body = `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.34},${sw} Q ${w * 0.18},${sw} ${w * 0.18},${cy * 0.5} Q ${w * 0.18},${cy} ${w * 0.06},${cy} Q ${w * 0.18},${cy} ${w * 0.18},${cy * 1.5} Q ${w * 0.18},${h - sw} ${w * 0.34},${h - sw}"/>` + `<path fill="none" stroke="${s}" stroke-width="${sw + 0.5}" d="M ${w * 0.66},${sw} Q ${w * 0.82},${sw} ${w * 0.82},${cy * 0.5} Q ${w * 0.82},${cy} ${w * 0.94},${cy} Q ${w * 0.82},${cy} ${w * 0.82},${cy * 1.5} Q ${w * 0.82},${h - sw} ${w * 0.66},${h - sw}"/>`; break
    // ── Flèches pleines (suite) ───────────────────────────────────────────────
    case 'leftRightUpArrow': { const sh = m * 0.12, hl = m * 0.24, hy = h * 0.32, hx = w * 0.26; body = poly(`${cx},${sw} ${cx + hl},${hy} ${cx + sh},${hy} ${cx + sh},${cy - sh} ${w - hx},${cy - sh} ${w - hx},${cy - hl} ${w - sw},${cy} ${w - hx},${cy + hl} ${w - hx},${cy + sh} ${cx + sh},${cy + sh} ${cx + sh},${h - sw} ${cx - sh},${h - sw} ${cx - sh},${cy + sh} ${hx},${cy + sh} ${hx},${cy + hl} ${sw},${cy} ${hx},${cy - hl} ${hx},${cy - sh} ${cx - sh},${cy - sh} ${cx - sh},${hy} ${cx - hl},${hy}`); break }
    case 'bentUpArrow': { const t = m * 0.2, xUp = w * 0.7, hh = h * 0.34, hw = t * 1.4; body = poly(`${sw},${h - sw - t} ${xUp - t / 2},${h - sw - t} ${xUp - t / 2},${hh} ${xUp - hw},${hh} ${xUp},${sw} ${xUp + hw},${hh} ${xUp + t / 2},${hh} ${xUp + t / 2},${h - sw} ${sw},${h - sw}`); break }
    case 'uTurnArrow': { const t = m * 0.18; body = path(`M ${sw},${h - sw} L ${sw},${h * 0.42} A ${w * 0.28} ${h * 0.3} 0 0 1 ${w * 0.62},${h * 0.42} L ${w * 0.62},${h * 0.55} L ${w * 0.8},${h * 0.55} L ${w * 0.56},${h - sw} L ${w * 0.32},${h * 0.55} L ${w * 0.5},${h * 0.55} L ${w * 0.5},${h * 0.42} A ${w * 0.16} ${h * 0.18} 0 0 0 ${sw + t},${h * 0.42} L ${sw + t},${h - sw} Z`); break }
    case 'curvedRightArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${sw},${h - sw} Q ${sw},${sw} ${w - sw},${sw}" marker-end="url(#aht)"/>`; break
    case 'curvedLeftArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${w - sw},${h - sw} Q ${w - sw},${sw} ${sw},${sw}" marker-end="url(#aht)"/>`; break
    case 'curvedUpArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${sw},${h - sw} Q ${w - sw},${h - sw} ${w - sw},${sw}" marker-end="url(#aht)"/>`; break
    case 'curvedDownArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.12)}" d="M ${sw},${sw} Q ${sw},${h - sw} ${w - sw},${h - sw}" marker-end="url(#aht)"/>`; break
    case 'stripedRightArrow': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${w * 0.1},${cy - sh / 2} ${w - hw},${cy - sh / 2} ${w - hw},${sw} ${w - sw},${cy} ${w - hw},${h - sw} ${w - hw},${cy + sh / 2} ${w * 0.1},${cy + sh / 2}`) + sline(sw, cy - sh / 2, sw, cy + sh / 2, sw + 1) + sline(w * 0.05, cy - sh / 2, w * 0.05, cy + sh / 2, sw + 1); break }
    case 'notchedArrow': { const sh = h * 0.34, hw = w * 0.4; body = poly(`${sw},${cy - sh / 2} ${w - hw},${cy - sh / 2} ${w - hw},${sw} ${w - sw},${cy} ${w - hw},${h - sw} ${w - hw},${cy + sh / 2} ${sw},${cy + sh / 2} ${w * 0.12},${cy}`); break }
    case 'circularArrow': body = `${arrowHeadThick}<path fill="none" stroke="${s}" stroke-width="${Math.max(4, m * 0.13)}" d="M ${(cx + oR * Math.cos(-0.4)).toFixed(1)},${(cy + oR * Math.sin(-0.4)).toFixed(1)} A ${oR} ${oR} 0 1 1 ${(cx + oR * Math.cos(-1.1)).toFixed(1)},${(cy + oR * Math.sin(-1.1)).toFixed(1)}" marker-end="url(#aht)"/>`; break
    case 'rightArrowCallout': { const sh = h * 0.16, hh = h * 0.3, bx = w * 0.52, hx = w * 0.78; body = poly(`${sw},${sw} ${bx},${sw} ${bx},${cy - sh} ${hx},${cy - sh} ${hx},${cy - hh} ${w - sw},${cy} ${hx},${cy + hh} ${hx},${cy + sh} ${bx},${cy + sh} ${bx},${h - sw} ${sw},${h - sw}`); break }
    case 'leftArrowCallout': { const sh = h * 0.16, hh = h * 0.3, bx = w * 0.48, hx = w * 0.22; body = poly(`${w - sw},${sw} ${bx},${sw} ${bx},${cy - sh} ${hx},${cy - sh} ${hx},${cy - hh} ${sw},${cy} ${hx},${cy + hh} ${hx},${cy + sh} ${bx},${cy + sh} ${bx},${h - sw} ${w - sw},${h - sw}`); break }
    case 'upArrowCallout': { const sv = w * 0.16, hh = w * 0.3, by = h * 0.52, hy = h * 0.22; body = poly(`${sw},${h - sw} ${sw},${by} ${cx - sv},${by} ${cx - sv},${hy} ${cx - hh},${hy} ${cx},${sw} ${cx + hh},${hy} ${cx + sv},${hy} ${cx + sv},${by} ${w - sw},${by} ${w - sw},${h - sw}`); break }
    case 'downArrowCallout': { const sv = w * 0.16, hh = w * 0.3, by = h * 0.48, hy = h * 0.78; body = poly(`${sw},${sw} ${sw},${by} ${cx - sv},${by} ${cx - sv},${hy} ${cx - hh},${hy} ${cx},${h - sw} ${cx + hh},${hy} ${cx + sv},${hy} ${cx + sv},${by} ${w - sw},${by} ${w - sw},${sw}`); break }
    // ── Organigrammes (suite) ─────────────────────────────────────────────────
    case 'flowPunchedTape': body = path(`M ${sw},${h * 0.18} Q ${w * 0.25},${sw} ${cx},${h * 0.18} Q ${w * 0.75},${h * 0.36} ${w - sw},${h * 0.18} L ${w - sw},${h * 0.82} Q ${w * 0.75},${h - sw} ${cx},${h * 0.82} Q ${w * 0.25},${h * 0.64} ${sw},${h * 0.82} Z`); break
    case 'flowSort': body = poly(`${cx},${sw} ${w - sw},${cy} ${cx},${h - sw} ${sw},${cy}`) + sline(sw, cy, w - sw, cy); break
    case 'flowSequential': { const r = m / 2 - sw; body = circ(cx, cy, r) + sline(cx, cy + r, cx + r, cy + r); break }
    case 'flowMagneticDisk': { const ry = Math.min(h * 0.14, cy - sw), topY = sw + ry, botY = h - sw - ry; body = path(`M ${sw},${topY} L ${sw},${botY} A ${cx - sw} ${ry} 0 0 0 ${w - sw},${botY} L ${w - sw},${topY}`) + `<ellipse cx="${cx}" cy="${topY}" rx="${cx - sw}" ry="${ry}" ${A}/>`; break }
    case 'flowDirectAccess': { const rx = Math.min(w * 0.16, cx - sw), leftX = sw + rx, rightX = w - sw - rx; body = path(`M ${leftX},${sw} L ${rightX},${sw} A ${rx} ${cy - sw} 0 0 1 ${rightX},${h - sw} L ${leftX},${h - sw} A ${rx} ${cy - sw} 0 0 1 ${leftX},${sw}`) + `<path fill="none" stroke="${s}" stroke-width="${sw}" d="M ${rightX},${sw} A ${rx} ${cy - sw} 0 0 0 ${rightX},${h - sw}"/>`; break }
    case 'flowDisplay': body = path(`M ${sw},${cy} L ${w * 0.18},${sw} L ${w * 0.82},${sw} A ${w * 0.18} ${cy - sw} 0 0 1 ${w * 0.82},${h - sw} L ${w * 0.18},${h - sw} Z`); break
    // ── Étoiles et bannières (suite) ──────────────────────────────────────────
    case 'star7':  body = poly(starPts(7, cx, cy, oR, oR * a(0, 0.55))); break
    case 'star10':  body = poly(starPts(10, cx, cy, oR, oR * a(0, 0.7))); break
    case 'star12':  body = poly(starPts(12, cx, cy, oR, oR * a(0, 0.72))); break
    case 'ribbonCurved': body = path(`M ${sw},${h * 0.3} Q ${cx},${h * 0.12} ${w - sw},${h * 0.3} L ${w - sw},${h * 0.7} Q ${cx},${h * 0.52} ${sw},${h * 0.7} Z`); break
    case 'scrollH': body = `<rect x="${m * 0.16}" y="${h * 0.24}" width="${w - 2 * m * 0.16}" height="${h * 0.52}" rx="${m * 0.05}" ${A}/>` + sline(m * 0.16, h * 0.32, m * 0.16, h * 0.68, sw) + sline(w - m * 0.16, h * 0.32, w - m * 0.16, h * 0.68, sw); break
    case 'scrollV': body = `<rect x="${w * 0.24}" y="${m * 0.16}" width="${w * 0.52}" height="${h - 2 * m * 0.16}" rx="${m * 0.05}" ${A}/>` + sline(w * 0.32, m * 0.16, w * 0.68, m * 0.16, sw) + sline(w * 0.32, h - m * 0.16, w * 0.68, h - m * 0.16, sw); break
    case 'doubleWave': { const a = h * 0.14; body = path(`M ${sw},${h * 0.3} Q ${w * 0.14},${h * 0.3 - a} ${w * 0.28},${h * 0.3} T ${w * 0.56},${h * 0.3} T ${w * 0.84},${h * 0.3} T ${w - sw},${h * 0.3} L ${w - sw},${h * 0.7} Q ${w * 0.86},${h * 0.7 + a} ${w * 0.72},${h * 0.7} T ${w * 0.44},${h * 0.7} T ${w * 0.16},${h * 0.7} T ${sw},${h * 0.7} Z`); break }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`
}

