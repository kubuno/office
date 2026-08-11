// « Effets de texte et typographie » (Word: Text Effects and Typography) — shared
// data contract for the whole feature: the ProseMirror mark that stores the
// effects, the preset galleries, the typography option lists, and the OOXML w14
// mapping constants. The ribbon UI, the canvas renderer and the DOCX converter
// all import from HERE so the four surfaces stay in lock-step.
//
// One dedicated mark `textEffect` carries every effect as independent, nullable
// attributes (null = « none »), so a submenu can change just the outline color
// without disturbing the shadow, exactly like Word.
import { Mark, mergeAttributes } from '@tiptap/core'
import type { Editor } from '@tiptap/core'

// ── Effect value shapes ──────────────────────────────────────────────────────

/** Text outline (Word « Contour »): a stroke around each glyph. width in px. */
export interface TextOutline {
  color: string
  width: number
  dash: 'solid' | 'dash' | 'dot' | 'dashDot'
}
/** Drop shadow (Word « Ombre »): outer/inner/perspective share this shape. */
export interface TextShadow {
  color: string
  blur: number    // px
  dx: number      // px offset
  dy: number      // px offset
  inner?: boolean // inner shadow (clipped inside glyphs)
}
/** Glow (Word « Éclat »): a soft colored halo. */
export interface TextGlow {
  color: string
  radius: number  // px
}
/** Reflection (Word « Reflet »): a fading mirror image below the text. */
export interface TextReflection {
  opacity: number   // 0..1 starting alpha at the top of the reflection
  size: number      // 0..1 fraction of glyph height that is reflected
  blur: number      // px
  distance: number  // px gap between text and its reflection
}

/** OpenType typography (rendered by DOCX / hidden DOM; canvas 2D cannot preview). */
export type LigatureMode = 'none' | 'standard' | 'standardContextual' | 'historicalDiscretional' | 'all'
export type NumForm = 'default' | 'lining' | 'oldStyle'
export type NumSpacing = 'default' | 'proportional' | 'tabular'

/** Full attribute set of the `textEffect` mark. Every field nullable (null = off). */
export interface TextEffectAttrs {
  fill: string | null
  outline: TextOutline | null
  shadow: TextShadow | null
  glow: TextGlow | null
  reflection: TextReflection | null
  ligatures: LigatureMode | null
  numForm: NumForm | null
  numSpacing: NumSpacing | null
  stylisticSet: number | null   // 1..20, or null
}

export const EMPTY_EFFECT: TextEffectAttrs = {
  fill: null, outline: null, shadow: null, glow: null, reflection: null,
  ligatures: null, numForm: null, numSpacing: null, stylisticSet: null,
}

/** The visual subset carried onto a canvas span (see canvas-engine `TextMark`). */
export interface TextEffectVisual {
  fill?: string
  outline?: TextOutline
  shadow?: TextShadow
  glow?: TextGlow
  reflection?: TextReflection
}

/** True when the attrs carry at least one VISUAL effect the canvas must paint. */
export function hasVisualEffect(a: Partial<TextEffectAttrs> | null | undefined): boolean {
  return !!a && !!(a.fill || a.outline || a.shadow || a.glow || a.reflection)
}

/** Extract only the visual part (for the canvas renderer). */
export function visualOf(a: Partial<TextEffectAttrs>): TextEffectVisual {
  const v: TextEffectVisual = {}
  if (a.fill) v.fill = a.fill
  if (a.outline) v.outline = a.outline
  if (a.shadow) v.shadow = a.shadow
  if (a.glow) v.glow = a.glow
  if (a.reflection) v.reflection = a.reflection
  return v
}

// ── OpenType → CSS font-feature-settings (hidden DOM / clipboard fidelity) ─────

const LIGATURE_FEATURES: Record<LigatureMode, string[]> = {
  none: ['liga 0', 'clig 0', 'dlig 0', 'hlig 0'],
  standard: ['liga 1', 'clig 0', 'dlig 0', 'hlig 0'],
  standardContextual: ['liga 1', 'clig 1', 'dlig 0', 'hlig 0'],
  historicalDiscretional: ['dlig 1', 'hlig 1'],
  all: ['liga 1', 'clig 1', 'dlig 1', 'hlig 1'],
}
const NUMFORM_FEATURES: Record<NumForm, string[]> = {
  default: [], lining: ['lnum 1'], oldStyle: ['onum 1'],
}
const NUMSPACING_FEATURES: Record<NumSpacing, string[]> = {
  default: [], proportional: ['pnum 1'], tabular: ['tnum 1'],
}

/** CSS `font-feature-settings` value for the typography attrs (or '' if none). */
export function fontFeatureSettings(a: Partial<TextEffectAttrs>): string {
  const feats: string[] = []
  if (a.ligatures) feats.push(...LIGATURE_FEATURES[a.ligatures])
  if (a.numForm) feats.push(...NUMFORM_FEATURES[a.numForm])
  if (a.numSpacing) feats.push(...NUMSPACING_FEATURES[a.numSpacing])
  if (a.stylisticSet && a.stylisticSet >= 1 && a.stylisticSet <= 20) {
    feats.push(`"ss${String(a.stylisticSet).padStart(2, '0')}" 1`)
  }
  return feats.map(f => (f.includes('"') ? f : `"${f.split(' ')[0]}" ${f.split(' ')[1]}`)).join(', ')
}

// ── ProseMirror mark ──────────────────────────────────────────────────────────

function jsonAttr(dataName: string) {
  return {
    default: null as unknown,
    parseHTML: (el: HTMLElement) => { const v = el.getAttribute(dataName); if (!v) return null; try { return JSON.parse(v) } catch { return null } },
    renderHTML: (attrs: Record<string, unknown>) => {
      const key = dataName.replace(/^data-te-/, '')
      const val = (attrs as Record<string, unknown>)[MAP_BACK[key] ?? key]
      return val ? { [dataName]: JSON.stringify(val) } : {}
    },
  }
}
// data-attr suffix → attribute name (for renderHTML lookups)
const MAP_BACK: Record<string, string> = {
  fill: 'fill', outline: 'outline', shadow: 'shadow', glow: 'glow', reflection: 'reflection',
  lig: 'ligatures', numform: 'numForm', numspacing: 'numSpacing', ss: 'stylisticSet',
}

function strAttr(dataName: string) {
  const key = MAP_BACK[dataName.replace(/^data-te-/, '')]
  return {
    default: null as string | null,
    parseHTML: (el: HTMLElement) => el.getAttribute(dataName) || null,
    renderHTML: (attrs: Record<string, unknown>) => { const v = attrs[key]; return v ? { [dataName]: String(v) } : {} },
  }
}
function intAttr(dataName: string) {
  const key = MAP_BACK[dataName.replace(/^data-te-/, '')]
  return {
    default: null as number | null,
    parseHTML: (el: HTMLElement) => { const v = el.getAttribute(dataName); return v ? parseInt(v, 10) : null },
    renderHTML: (attrs: Record<string, unknown>) => { const v = attrs[key]; return v != null ? { [dataName]: String(v) } : {} },
  }
}

export const TextEffect = Mark.create({
  name: 'textEffect',
  // Continue the effect while typing at its edge (a formatting mark, like bold).
  inclusive: true,
  addAttributes() {
    return {
      fill: jsonAttr('data-te-fill'),
      outline: jsonAttr('data-te-outline'),
      shadow: jsonAttr('data-te-shadow'),
      glow: jsonAttr('data-te-glow'),
      reflection: jsonAttr('data-te-reflection'),
      ligatures: strAttr('data-te-lig'),
      numForm: strAttr('data-te-numform'),
      numSpacing: strAttr('data-te-numspacing'),
      stylisticSet: intAttr('data-te-ss'),
    }
  },
  parseHTML() { return [{ tag: 'span[data-te]' }] },
  renderHTML({ HTMLAttributes, mark }) {
    // Inline CSS so a DOM render / clipboard keeps a faithful-enough look, even
    // though the on-screen document is painted on <canvas>.
    const a = mark.attrs as unknown as TextEffectAttrs
    const style: string[] = []
    if (a.fill) style.push(`color:${a.fill}`)
    if (a.outline) style.push(`-webkit-text-stroke:${a.outline.width}px ${a.outline.color}`)
    if (a.shadow) style.push(`text-shadow:${a.shadow.dx}px ${a.shadow.dy}px ${a.shadow.blur}px ${a.shadow.color}`)
    else if (a.glow) style.push(`text-shadow:0 0 ${a.glow.radius}px ${a.glow.color}`)
    const ff = fontFeatureSettings(a)
    if (ff) style.push(`font-feature-settings:${ff}`)
    return ['span', mergeAttributes({ 'data-te': '', ...(style.length ? { style: style.join(';') } : {}) }, HTMLAttributes), 0]
  },
})

// ── Editor helpers ─────────────────────────────────────────────────────────────

/** Current textEffect attrs at the selection (merged from the active mark). */
export function currentEffect(editor: Editor | null): TextEffectAttrs {
  if (!editor) return { ...EMPTY_EFFECT }
  const a = editor.getAttributes('textEffect') as Partial<TextEffectAttrs>
  return { ...EMPTY_EFFECT, ...a }
}

/** Merge a partial patch into the current textEffect mark over the selection. */
export function applyEffect(editor: Editor | null, patch: Partial<TextEffectAttrs>): void {
  if (!editor) return
  const cur = currentEffect(editor)
  const next: TextEffectAttrs = { ...cur, ...patch }
  // If every field is off, drop the mark entirely.
  const anySet = Object.values(next).some(v => v != null)
  const chain = editor.chain().focus()
  if (!anySet) chain.unsetMark('textEffect')
  else chain.setMark('textEffect', next as unknown as Record<string, unknown>)
  chain.run()
}

/** Clear all text effects on the selection. */
export function clearEffect(editor: Editor | null): void {
  editor?.chain().focus().unsetMark('textEffect').run()
}

// ── Preset galleries ───────────────────────────────────────────────────────────

/** A WordArt Quick-Style preset (the 3×5 grid at the top of the menu). */
export interface WordArtPreset {
  id: string
  effect: Partial<TextEffectAttrs>
}

/**
 * Null patch for every VISUAL effect. A WordArt Quick Style is EXCLUSIVE, like
 * in Word: picking one REPLACES fill/outline/shadow/glow/reflection wholesale
 * (spread this first, then the preset). Typography attrs are untouched — the
 * gallery never changes ligatures or number styles.
 */
export const VISUAL_RESET: Partial<TextEffectAttrs> = {
  fill: null, outline: null, shadow: null, glow: null, reflection: null,
}

// Office theme palette used by the presets.
const C = {
  text1: '#000000', white: '#ffffff', accent1: '#4472c4', accent2: '#ed7d31',
  accent3: '#a5a5a5', accent4: '#ffc000', accent5: '#5b9bd5', accent6: '#70ad47',
  dk: '#1f1f1f',
}

/** The 15 WordArt Quick Styles, mirroring Word's Office-theme gallery order. */
export const WORDART_PRESETS: WordArtPreset[] = [
  // Row 1
  { id: 'fill-black-shadow', effect: { fill: C.text1, shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: 1, dy: 1 } } },
  { id: 'fill-blue-shadow', effect: { fill: C.accent1, shadow: { color: 'rgba(0,0,0,0.35)', blur: 3, dx: 1, dy: 1 } } },
  { id: 'outline-accent2', effect: { fill: C.white, outline: { color: C.accent2, width: 1.2, dash: 'solid' } } },
  { id: 'outline-accent5', effect: { fill: C.white, outline: { color: C.accent5, width: 1.2, dash: 'solid' } } },
  { id: 'fill-gold-shadow', effect: { fill: C.accent4, shadow: { color: 'rgba(0,0,0,0.35)', blur: 3, dx: 1, dy: 1 } } },
  // Row 2
  { id: 'fill-gray', effect: { fill: C.accent3 } },
  { id: 'outline-accent5-fill-white-glow', effect: { fill: C.white, outline: { color: C.accent5, width: 1.2, dash: 'solid' }, glow: { color: C.accent5, radius: 6 } } },
  { id: 'outline-gold', effect: { fill: C.white, outline: { color: C.accent4, width: 1.2, dash: 'solid' } } },
  { id: 'outline-accent1-shadow', effect: { fill: C.white, outline: { color: C.accent1, width: 1.2, dash: 'solid' }, shadow: { color: 'rgba(0,0,0,0.3)', blur: 3, dx: 1, dy: 1 } } },
  { id: 'fill-gray-dark', effect: { fill: '#7f7f7f' } },
  // Row 3
  { id: 'fill-black-strong-shadow', effect: { fill: C.text1, shadow: { color: 'rgba(0,0,0,0.5)', blur: 5, dx: 2, dy: 2 } } },
  { id: 'fill-black-reflection', effect: { fill: C.text1, reflection: { opacity: 0.5, size: 0.5, blur: 1, distance: 2 } } },
  { id: 'fill-blue-reflection', effect: { fill: C.accent1, reflection: { opacity: 0.5, size: 0.5, blur: 1, distance: 2 } } },
  { id: 'outline-accent2-2', effect: { fill: C.white, outline: { color: C.accent2, width: 1.4, dash: 'solid' } } },
  { id: 'fill-silver-reflection', effect: { fill: C.accent3, reflection: { opacity: 0.45, size: 0.5, blur: 1, distance: 2 } } },
]

/** Shadow gallery, grouped like Word (Externe / Interne / Perspective). */
export interface ShadowPreset { id: string; group: 'outer' | 'inner' | 'perspective'; shadow: TextShadow }
export const SHADOW_PRESETS: ShadowPreset[] = [
  // Outer (8 directions)
  { id: 'o-br', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: 2, dy: 2 } },
  { id: 'o-b', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: 0, dy: 2 } },
  { id: 'o-bl', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: -2, dy: 2 } },
  { id: 'o-r', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: 2, dy: 0 } },
  { id: 'o-l', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: -2, dy: 0 } },
  { id: 'o-tr', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: 2, dy: -2 } },
  { id: 'o-t', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: 0, dy: -2 } },
  { id: 'o-tl', group: 'outer', shadow: { color: 'rgba(0,0,0,0.4)', blur: 3, dx: -2, dy: -2 } },
  // Inner
  { id: 'i-br', group: 'inner', shadow: { color: 'rgba(0,0,0,0.45)', blur: 3, dx: 2, dy: 2, inner: true } },
  { id: 'i-b', group: 'inner', shadow: { color: 'rgba(0,0,0,0.45)', blur: 3, dx: 0, dy: 2, inner: true } },
  { id: 'i-r', group: 'inner', shadow: { color: 'rgba(0,0,0,0.45)', blur: 3, dx: 2, dy: 0, inner: true } },
  { id: 'i-c', group: 'inner', shadow: { color: 'rgba(0,0,0,0.5)', blur: 4, dx: 0, dy: 0, inner: true } },
  // Perspective (approximated with a soft, offset, blurred shadow)
  { id: 'p-bl', group: 'perspective', shadow: { color: 'rgba(0,0,0,0.35)', blur: 6, dx: -3, dy: 5 } },
  { id: 'p-br', group: 'perspective', shadow: { color: 'rgba(0,0,0,0.35)', blur: 6, dx: 3, dy: 5 } },
]

/** Reflection gallery (Word « Variations de reflet »). */
export interface ReflectionPreset { id: string; reflection: TextReflection }
export const REFLECTION_PRESETS: ReflectionPreset[] = [
  { id: 'tight', reflection: { opacity: 0.55, size: 0.5, blur: 0, distance: 0 } },
  { id: 'half', reflection: { opacity: 0.5, size: 0.5, blur: 0.5, distance: 4 } },
  { id: 'full', reflection: { opacity: 0.5, size: 1, blur: 0.5, distance: 4 } },
  { id: 'tight-8', reflection: { opacity: 0.5, size: 0.5, blur: 1, distance: 8 } },
  { id: 'half-8', reflection: { opacity: 0.45, size: 0.5, blur: 1.5, distance: 8 } },
  { id: 'full-8', reflection: { opacity: 0.45, size: 1, blur: 1.5, distance: 8 } },
  { id: 'tight-strong', reflection: { opacity: 0.7, size: 0.5, blur: 0, distance: 0 } },
  { id: 'full-soft', reflection: { opacity: 0.4, size: 1, blur: 2.5, distance: 6 } },
  { id: 'half-far', reflection: { opacity: 0.4, size: 0.5, blur: 2, distance: 12 } },
]

/** Glow gallery: Word offers radius × theme color. We expose a compact 6×? grid. */
export const GLOW_RADII = [4, 6, 8, 11, 16]
export const GLOW_COLORS = [C.accent1, C.accent2, C.accent3, C.accent4, C.accent5, C.accent6]

// ── Typography option lists (labels match Word FR) ───────────────────────────

export interface OptionRow<T> { value: T; title: string; desc?: string }
export const LIGATURE_OPTIONS: OptionRow<LigatureMode>[] = [
  { value: 'none', title: 'Aucune', desc: 'Désactiver toutes les ligatures dans le texte sélectionné' },
  { value: 'standard', title: 'Standard uniquement', desc: 'Activer les ligatures pour améliorer la lisibilité du texte' },
  { value: 'standardContextual', title: 'Standard et contextuelles', desc: 'Activer les ligatures supplémentaires trouvées dans l’impression contextuelle' },
  { value: 'historicalDiscretional', title: 'Historiques et discrétionnaires', desc: 'Activer les ligatures trouvées dans l’impression historique ainsi que celles produisant un effet stylistique' },
  { value: 'all', title: 'Toutes les ligatures', desc: 'Activer toutes les ligatures disponibles dans le texte sélectionné' },
]
export const NUMFORM_OPTIONS: OptionRow<NumForm | 'reset'>[] = [
  { value: 'reset', title: 'Toujours utiliser', desc: 'Style recommandé pour votre police' },
  { value: 'lining', title: 'Alignement proportionnel', desc: 'Numéros en pleine hauteur espacés de façon proportionnelle' },
  { value: 'oldStyle', title: 'À l’ancienne proportionnel', desc: 'Numéros lus correctement avec texte' },
]
export const NUMSPACING_OPTIONS: OptionRow<NumSpacing>[] = [
  { value: 'proportional', title: 'Alignement proportionnel', desc: 'Espacés de façon proportionnelle' },
  { value: 'tabular', title: 'Alignement tabulaire', desc: 'Espacés de façon égale' },
]
