// Color themes for the formula syntax-coloring feature. A palette assigns a hue to each kind
// of atom KaTeX tags: variables, numbers, binary operators, relations, functions/big operators
// and delimiters. The active palette drives CSS custom properties on the rendered formula, so a
// theme is a pure display preference — the stored LaTeX never changes. Users can pick one of the
// built-in themes below or tweak the six colors to build their own ("Personnalisé").
import type { CSSProperties } from 'react'

export interface MathPalette {
  v: string    // variables (italic letters, greek)
  n: string    // numbers
  op: string   // binary operators  + − · ×
  rel: string  // relations  = < > → ≤
  fn: string   // functions / big operators  sin lim ∑ ∫
  del: string  // delimiters / punctuation  ( ) [ ] | ,
}

export interface MathTheme { id: string; name: string; pal: MathPalette }

const p = (v: string, n: string, op: string, rel: string, fn: string, del: string): MathPalette => ({ v, n, op, rel, fn, del })

// ~20 named built-in themes (readable on a white background).
export const MATH_THEMES: MathTheme[] = [
  { id: 'classic',   name: 'Classique',         pal: p('#1a73e8', '#188038', '#d93025', '#9334e6', '#e8710a', '#607d8b') },
  { id: 'ink',       name: 'Encre',             pal: p('#202124', '#202124', '#5f6368', '#5f6368', '#3c4043', '#9aa0a6') },
  { id: 'rainbow',   name: 'Arc-en-ciel',       pal: p('#1565c0', '#2e7d32', '#c62828', '#6a1b9a', '#ef6c00', '#00838f') },
  { id: 'pastel',    name: 'Pastel',            pal: p('#5c9ce6', '#7cc47f', '#e88a8a', '#b08fd6', '#f0b27a', '#9fb4c2') },
  { id: 'ocean',     name: 'Océan',             pal: p('#0277bd', '#00838f', '#0097a7', '#283593', '#00acc1', '#5c8aa8') },
  { id: 'forest',    name: 'Forêt',             pal: p('#2e7d32', '#558b2f', '#33691e', '#1b5e20', '#9e7d0a', '#6d8c5a') },
  { id: 'sunset',    name: 'Coucher de soleil', pal: p('#e65100', '#ef6c00', '#d84315', '#ad1457', '#f9a825', '#8d6e63') },
  { id: 'neon',      name: 'Néon',              pal: p('#0091ea', '#00c853', '#ff1744', '#aa00ff', '#ff9100', '#00b8d4') },
  { id: 'earth',     name: 'Terre',             pal: p('#5d4037', '#827717', '#bf360c', '#4e342e', '#e65100', '#8d6e63') },
  { id: 'candy',     name: 'Bonbon',            pal: p('#e91e63', '#9c27b0', '#f06292', '#7b1fa2', '#ff7043', '#ba68c8') },
  { id: 'sepia',     name: 'Sépia',             pal: p('#704214', '#8a6d3b', '#a0522d', '#5b3a1a', '#b8860b', '#9c8466') },
  { id: 'solarized', name: 'Solarisé',          pal: p('#268bd2', '#859900', '#dc322f', '#6c71c4', '#cb4b16', '#2aa198') },
  { id: 'dracula',   name: 'Dracula',           pal: p('#6c3fd6', '#21a05a', '#e04488', '#b341d9', '#d98324', '#6272a4') },
  { id: 'monokai',   name: 'Monokai',           pal: p('#1e9fc4', '#6f9a00', '#e6196b', '#9d6cff', '#e07b00', '#75715e') },
  { id: 'nord',      name: 'Nord',              pal: p('#5e81ac', '#6a8d4f', '#bf616a', '#b48ead', '#d08770', '#4c566a') },
  { id: 'amethyst',  name: 'Améthyste',         pal: p('#7b1fa2', '#9c27b0', '#ab47bc', '#6a1b9a', '#8e24aa', '#b39ddb') },
  { id: 'citrus',    name: 'Agrumes',           pal: p('#ef6c00', '#9e9d24', '#f4511e', '#7cb342', '#fb8c00', '#afb42b') },
  { id: 'lavender',  name: 'Lavande',           pal: p('#7986cb', '#9575cd', '#ba68c8', '#5c6bc0', '#7e57c2', '#b39ddb') },
  { id: 'ruby',      name: 'Rubis',             pal: p('#c62828', '#d32f2f', '#e53935', '#b71c1c', '#f4511e', '#ef9a9a') },
  { id: 'emerald',   name: 'Émeraude',          pal: p('#00897b', '#2e7d32', '#00acc1', '#00695c', '#43a047', '#80cbc4') },
  { id: 'contrast',  name: 'Contraste élevé',   pal: p('#0000ee', '#006400', '#cc0000', '#8000ff', '#ff6600', '#000000') },
]

export const DEFAULT_THEME_ID = 'classic'
export const themeById = (id: string): MathTheme | undefined => MATH_THEMES.find(th => th.id === id)

// ── Persistence (localStorage; benign UI preference) ──────────────────────────────
const K_ON = 'kubuno.maths.colorize'
const K_ID = 'kubuno.maths.colortheme'
const K_CUSTOM = 'kubuno.maths.colorpalette'

export function loadColorize(): boolean { try { return localStorage.getItem(K_ON) !== '0' } catch { return true } }
export function saveColorize(on: boolean): void { try { localStorage.setItem(K_ON, on ? '1' : '0') } catch { /* ignore */ } }

export function loadThemeId(): string { try { return localStorage.getItem(K_ID) || DEFAULT_THEME_ID } catch { return DEFAULT_THEME_ID } }
export function saveThemeId(id: string): void { try { localStorage.setItem(K_ID, id) } catch { /* ignore */ } }

export function loadCustomPalette(): MathPalette {
  try {
    const raw = localStorage.getItem(K_CUSTOM)
    if (raw) {
      const j = JSON.parse(raw)
      if (j && typeof j.v === 'string') return { v: j.v, n: j.n, op: j.op, rel: j.rel, fn: j.fn, del: j.del }
    }
  } catch { /* ignore */ }
  return { ...(themeById(DEFAULT_THEME_ID) as MathTheme).pal }
}
export function saveCustomPalette(pal: MathPalette): void { try { localStorage.setItem(K_CUSTOM, JSON.stringify(pal)) } catch { /* ignore */ } }

// Resolve the active palette: a custom one (when themeId === 'custom') or a built-in theme.
export function resolvePalette(themeId: string, custom: MathPalette): MathPalette {
  return themeId === 'custom' ? custom : (themeById(themeId)?.pal ?? (themeById(DEFAULT_THEME_ID) as MathTheme).pal)
}

// CSS custom properties the coloring stylesheet reads (`var(--mc-v)` …).
export function paletteStyle(pal: MathPalette): CSSProperties {
  return {
    '--mc-v': pal.v, '--mc-n': pal.n, '--mc-op': pal.op,
    '--mc-rel': pal.rel, '--mc-fn': pal.fn, '--mc-del': pal.del,
  } as CSSProperties
}
