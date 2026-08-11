// « Effets de texte et typographie » ribbon button (Word: Home ▸ Font ▸ Text
// Effects and Typography). A small ribbon button showing a stylized blue « A »
// plus a chevron; clicking it opens the full drop-down: the 3×5 WordArt Quick
// Styles grid on top, then flyout submenus for Outline / Shadow / Reflection /
// Glow / Number styles / Ligatures / Stylistic sets.
//
// This file is UI only. All effect state, presets and typography option lists
// live in ./model — we never redefine them here, we only read (`currentEffect`)
// and write (`applyEffect`). Effect previews are painted on small <canvas>
// elements through the SAME renderer as the document (`paintEffectText`), so
// the demo always matches the real result and can never overflow its tile.
// CSS previews remain only for the typography rows (font features), which the
// canvas renderer does not handle.
import { useEffect, useRef, useState } from 'react'
import type { Editor } from '@tiptap/core'
import { MenuDropdown, ColorSwatchPicker, useAppPickerTheme } from '@ui'
import type { MenuItem } from '@ui'
import { ChevronDown, PenTool, Copy, FlipVertical, Sparkles, Hash, Type, WandSparkles } from 'lucide-react'
import {
  applyEffect, currentEffect, fontFeatureSettings, visualOf, VISUAL_RESET,
  WORDART_PRESETS, SHADOW_PRESETS, REFLECTION_PRESETS, GLOW_RADII, GLOW_COLORS,
  LIGATURE_OPTIONS, NUMFORM_OPTIONS, NUMSPACING_OPTIONS,
  type TextEffectAttrs, type TextOutline, type NumForm,
} from './model'
import { paintEffectText } from './render'

// ── CSS preview (typography rows ONLY — visual effects preview on canvas) ────
/** Inline CSS for the typography option previews (« 123 », « ffi »…). */
function previewStyle(effect: Partial<TextEffectAttrs>): React.CSSProperties {
  const ff = fontFeatureSettings(effect)
  return ff ? { fontFeatureSettings: ff } : {}
}

// ── Canvas preview of a visual effect (same renderer as the document) ─────────
/**
 * A fixed-size <canvas> painting `char` through `paintEffectText` — the exact
 * engine used for the document itself, so what you see is what you get. The
 * canvas also clips its content: previews can never bleed outside their tile.
 */
function EffectPreviewCanvas({ effect, char = 'A', width, height, fontPx }: {
  effect: Partial<TextEffectAttrs>; char?: string; width: number; height: number; fontPx: number
}) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    if (!canvas) return
    const dpr = window.devicePixelRatio || 1
    canvas.width = Math.round(width * dpr)
    canvas.height = Math.round(height * dpr)
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.scale(dpr, dpr)
    ctx.font = `600 ${fontPx}px Arial`
    const m = ctx.measureText(char)
    const ascent = m.actualBoundingBoxAscent || fontPx * 0.75
    const descent = m.actualBoundingBoxDescent || fontPx * 0.05
    const x = (width - m.width) / 2
    // Center the glyph vertically; with a reflection, raise it so ~40% of the
    // height stays below the baseline for the mirror image.
    const baseline = effect.reflection
      ? Math.min(height * 0.6, (height - (ascent + descent)) / 2 + ascent)
      : (height - (ascent + descent)) / 2 + ascent
    paintEffectText(ctx, char, x, baseline, visualOf(effect), '#000', { ascent, descent })
  }, [effect, char, width, height, fontPx])
  return <canvas ref={ref} style={{ width, height }} aria-hidden="true" />
}

// Points → px for outline widths (Word offers 8 preset weights). 1pt ≈ 1.333px.
const PT_TO_PX = 1.333
const OUTLINE_WIDTHS: Array<{ label: string; pt: number }> = [
  { label: '½ pt', pt: 0.5 }, { label: '¾ pt', pt: 0.75 }, { label: '1 pt', pt: 1 },
  { label: '1½ pt', pt: 1.5 }, { label: '2¼ pt', pt: 2.25 }, { label: '3 pt', pt: 3 },
  { label: '4½ pt', pt: 4.5 }, { label: '6 pt', pt: 6 },
]
const OUTLINE_DASHES: Array<{ label: string; value: TextOutline['dash'] }> = [
  { label: 'Trait plein', value: 'solid' }, { label: 'Tirets', value: 'dash' },
  { label: 'Pointillés', value: 'dot' }, { label: 'Tiret-point', value: 'dashDot' },
]

// ── Small presentational tiles used inside the custom menu renders ────────────

/** A single clickable FIXED-SIZE tile previewing an effect on canvas. */
function EffectTile({ effect, onClick, label, w = 48, h = 44, fontPx = 24 }: {
  effect: Partial<TextEffectAttrs>; onClick: () => void; label?: string
  w?: number; h?: number; fontPx?: number
}) {
  return (
    <button type="button" onMouseDown={e => e.preventDefault()} onClick={onClick} title={label}
      className="flex items-center justify-center rounded hover:bg-hover hover:ring-1 hover:ring-primary/40 transition-colors overflow-hidden"
      style={{ width: w, height: h }}>
      <EffectPreviewCanvas effect={effect} width={w - 2} height={h - 2} fontPx={fontPx} />
    </button>
  )
}

// ── Custom render bodies (passed to MenuDropdown `type:'custom'` items) ────────

/** The 3×5 WordArt Quick Styles grid at the top of the drop-down. */
function WordArtGrid({ editor, close }: { editor: Editor | null; close: () => void }) {
  return (
    <div className="grid grid-cols-5 gap-1 p-2" role="group" aria-label="Styles rapides WordArt">
      {WORDART_PRESETS.map(p => (
        <EffectTile key={p.id} effect={p.effect} label={p.id}
          // EXCLUSIVE, like Word: a Quick Style replaces ALL visual effects —
          // never stacks on top of whatever was applied before.
          onClick={() => { applyEffect(editor, { ...VISUAL_RESET, ...p.effect }); close() }} />
      ))}
    </div>
  )
}

/** One group of shadow presets (Externe / Interne / Perspective). */
function ShadowGroup({ editor, close, group, title }: {
  editor: Editor | null; close: () => void; group: 'outer' | 'inner' | 'perspective'; title: string
}) {
  const items = SHADOW_PRESETS.filter(s => s.group === group)
  return (
    <div className="px-2 pt-1.5">
      <div className="text-[11px] text-text-secondary mb-1">{title}</div>
      <div className="grid grid-cols-4 gap-1">
        {items.map(s => (
          <EffectTile key={s.id} w={40} h={36} fontPx={20} label={s.id} effect={{ fill: '#404040', shadow: s.shadow }}
            onClick={() => { applyEffect(editor, { shadow: s.shadow }); close() }} />
        ))}
      </div>
    </div>
  )
}

/** The reflection variations gallery. */
function ReflectionGallery({ editor, close }: { editor: Editor | null; close: () => void }) {
  return (
    <div className="px-2 pt-1.5">
      <div className="text-[11px] text-text-secondary mb-1">Variations de reflet</div>
      <div className="grid grid-cols-3 gap-1">
        {REFLECTION_PRESETS.map(r => (
          <EffectTile key={r.id} w={44} h={44} fontPx={20} label={r.id} effect={{ fill: '#404040', reflection: r.reflection }}
            onClick={() => { applyEffect(editor, { reflection: r.reflection }); close() }} />
        ))}
      </div>
    </div>
  )
}

/** The glow variations grid: GLOW_COLORS × GLOW_RADII. */
function GlowGallery({ editor, close }: { editor: Editor | null; close: () => void }) {
  return (
    <div className="px-2 pt-1.5">
      <div className="text-[11px] text-text-secondary mb-1">Variations d’éclat</div>
      <div className="flex flex-col gap-1">
        {GLOW_RADII.map(radius => (
          <div key={radius} className="flex gap-1">
            {GLOW_COLORS.map(color => (
              <EffectTile key={color + radius} w={34} h={30} fontPx={20} label={`${radius}px`}
                effect={{ fill: '#404040', glow: { color, radius } }}
                onClick={() => { applyEffect(editor, { glow: { color, radius } }); close() }} />
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}

/** A rich option row: bold title + small description + a live CSS preview. */
function OptionRowButton({ title, desc, preview, previewAttrs, onClick }: {
  title: string; desc?: string; preview: string; previewAttrs: Partial<TextEffectAttrs>; onClick: () => void
}) {
  return (
    <button type="button" onMouseDown={e => e.preventDefault()} onClick={onClick}
      className="w-full flex items-center gap-3 px-2 py-1.5 rounded text-left hover:bg-hover">
      <span className="w-10 shrink-0 text-center text-base text-text-primary" style={previewStyle(previewAttrs)}>{preview}</span>
      <span className="flex flex-col min-w-0">
        <span className="text-sm text-text-primary">{title}</span>
        {desc && <span className="text-[11px] text-text-secondary leading-tight">{desc}</span>}
      </span>
    </button>
  )
}

/** Number-styles submenu body (form + spacing option rows). */
function NumberStylesList({ editor, close }: { editor: Editor | null; close: () => void }) {
  return (
    <div className="w-[320px] py-1 max-h-[70vh] overflow-y-auto">
      <div className="px-2 py-0.5 text-[11px] text-text-secondary">Format des nombres</div>
      {NUMFORM_OPTIONS.map(o => (
        <OptionRowButton key={String(o.value)} title={o.title} desc={o.desc} preview="123"
          previewAttrs={o.value === 'reset' ? {} : { numForm: o.value as NumForm }}
          onClick={() => {
            // « Toujours utiliser » resets both typography axes to the font default.
            if (o.value === 'reset') applyEffect(editor, { numForm: null, numSpacing: null })
            else applyEffect(editor, { numForm: o.value as NumForm })
            close()
          }} />
      ))}
      <div className="px-2 py-0.5 mt-1 text-[11px] text-text-secondary">Espacement des nombres</div>
      {NUMSPACING_OPTIONS.map(o => (
        <OptionRowButton key={o.value} title={o.title} desc={o.desc} preview="123"
          previewAttrs={{ numSpacing: o.value }}
          onClick={() => { applyEffect(editor, { numSpacing: o.value }); close() }} />
      ))}
    </div>
  )
}

/** Ligatures submenu body. */
function LigaturesList({ editor, close }: { editor: Editor | null; close: () => void }) {
  return (
    <div className="w-[320px] py-1 max-h-[70vh] overflow-y-auto">
      {LIGATURE_OPTIONS.map(o => (
        <OptionRowButton key={o.value} title={o.title} desc={o.desc} preview="ffi"
          previewAttrs={{ ligatures: o.value }}
          onClick={() => { applyEffect(editor, { ligatures: o.value }); close() }} />
      ))}
    </div>
  )
}

const STYLISTIC_SAMPLE = 'Servez à ce monsieur une bière et des kiwis.'

/** Stylistic-sets submenu body: « Toujours utiliser » (reset) + ss01..ss20. */
function StylisticSetsList({ editor, close }: { editor: Editor | null; close: () => void }) {
  return (
    <div className="w-[360px] py-1 max-h-[70vh] overflow-y-auto">
      <button type="button" onMouseDown={e => e.preventDefault()}
        onClick={() => { applyEffect(editor, { stylisticSet: null }); close() }}
        className="w-full px-2 py-1.5 rounded text-left text-sm text-text-primary hover:bg-hover">
        Toujours utiliser
      </button>
      <div className="px-2 py-0.5 text-[11px] text-text-secondary">Individuel</div>
      {Array.from({ length: 20 }, (_, i) => i + 1).map(n => (
        <button key={n} type="button" onMouseDown={e => e.preventDefault()}
          onClick={() => { applyEffect(editor, { stylisticSet: n }); close() }}
          className="w-full flex items-center gap-2 px-2 py-1 rounded text-left hover:bg-hover">
          <span className="w-9 shrink-0 text-[11px] text-text-secondary">{`ss${String(n).padStart(2, '0')}`}</span>
          <span className="text-sm text-text-primary truncate"
            style={{ fontFeatureSettings: `"ss${String(n).padStart(2, '0')}" 1` }}>{STYLISTIC_SAMPLE}</span>
        </button>
      ))}
    </div>
  )
}

/** A ColorSwatchPicker embedded in a submenu (outline / glow color pickers). */
function ColorSubPicker({ current, onPick }: { current: string; onPick: (hex: string) => void }) {
  const theme = useAppPickerTheme()
  return <ColorSwatchPicker color={current} theme={theme} onChange={onPick} />
}

// ── Menu tree ─────────────────────────────────────────────────────────────────

/** Build the full MenuDropdown item tree for the current selection state. */
function buildItems(editor: Editor | null, close: () => void): MenuItem[] {
  const cur = currentEffect(editor)
  // Outline patch helper: keep existing width/dash (defaults width 1, solid).
  const outlineWith = (patch: Partial<TextOutline>): TextOutline => ({
    color: cur.outline?.color ?? '#000000',
    width: cur.outline?.width ?? 1,
    dash: cur.outline?.dash ?? 'solid',
    ...patch,
  })

  return [
    // 1) WordArt Quick Styles grid.
    { type: 'custom', render: (c) => <WordArtGrid editor={editor} close={c} /> },
    { type: 'separator' },

    // 2) Outline (Contour).
    {
      type: 'submenu', label: 'Contour', icon: <PenTool size={15} />, items: [
        { type: 'custom', render: () => <ColorSubPicker current={cur.outline?.color ?? '#000000'} onPick={hex => { applyEffect(editor, { outline: outlineWith({ color: hex }) }); close() }} /> },
        { type: 'separator' },
        { type: 'action', label: 'Sans contour', onClick: () => { applyEffect(editor, { outline: null }); close() } },
        {
          type: 'submenu', label: 'Épaisseur', items: OUTLINE_WIDTHS.map(w => ({
            type: 'action' as const, label: w.label,
            checked: !!cur.outline && Math.abs(cur.outline.width - w.pt * PT_TO_PX) < 0.05,
            onClick: () => { applyEffect(editor, { outline: outlineWith({ width: +(w.pt * PT_TO_PX).toFixed(2) }) }); close() },
          })),
        },
        {
          type: 'submenu', label: 'Tirets', items: OUTLINE_DASHES.map(d => ({
            type: 'action' as const, label: d.label, checked: cur.outline?.dash === d.value,
            onClick: () => { applyEffect(editor, { outline: outlineWith({ dash: d.value }) }); close() },
          })),
        },
      ],
    },

    // 3) Shadow (Ombre).
    {
      type: 'submenu', label: 'Ombre', icon: <Copy size={15} />, items: [
        { type: 'action', label: 'Aucune ombre', onClick: () => { applyEffect(editor, { shadow: null }); close() } },
        { type: 'custom', render: (c) => <ShadowGroup editor={editor} close={c} group="outer" title="Externe" /> },
        { type: 'custom', render: (c) => <ShadowGroup editor={editor} close={c} group="inner" title="Interne" /> },
        { type: 'custom', render: (c) => <ShadowGroup editor={editor} close={c} group="perspective" title="Perspective" /> },
        { type: 'separator' },
        { type: 'action', label: 'Options d’ombres…', disabled: true, onClick: () => {} },
      ],
    },

    // 4) Reflection (Reflet).
    {
      type: 'submenu', label: 'Reflet', icon: <FlipVertical size={15} />, items: [
        { type: 'action', label: 'Aucun reflet', onClick: () => { applyEffect(editor, { reflection: null }); close() } },
        { type: 'custom', render: (c) => <ReflectionGallery editor={editor} close={c} /> },
        { type: 'separator' },
        { type: 'action', label: 'Options de reflet…', disabled: true, onClick: () => {} },
      ],
    },

    // 5) Glow (Éclat).
    {
      type: 'submenu', label: 'Éclat', icon: <Sparkles size={15} />, items: [
        { type: 'action', label: 'Aucun éclat', onClick: () => { applyEffect(editor, { glow: null }); close() } },
        { type: 'custom', render: (c) => <GlowGallery editor={editor} close={c} /> },
        { type: 'separator' },
        { type: 'submenu', label: 'Autres couleurs d’éclat', items: [{ type: 'custom', render: () => <ColorSubPicker current={cur.glow?.color ?? GLOW_COLORS[0]} onPick={hex => { applyEffect(editor, { glow: { color: hex, radius: cur.glow?.radius ?? GLOW_RADII[1] } }); close() }} /> }] },
        { type: 'action', label: 'Options d’éclat…', disabled: true, onClick: () => {} },
      ],
    },

    { type: 'separator' },

    // 6) Number styles (Styles de nombres).
    {
      type: 'submenu', label: 'Styles de nombres', icon: <Hash size={15} />,
      items: [{ type: 'custom', render: (c) => <NumberStylesList editor={editor} close={c} /> }],
    },

    // 7) Ligatures.
    {
      type: 'submenu', label: 'Ligatures', icon: <Type size={15} />,
      items: [{ type: 'custom', render: (c) => <LigaturesList editor={editor} close={c} /> }],
    },

    // 8) Stylistic sets (Jeux stylistiques).
    {
      type: 'submenu', label: 'Jeux stylistiques', icon: <WandSparkles size={15} />,
      items: [{ type: 'custom', render: (c) => <StylisticSetsList editor={editor} close={c} /> }],
    },
  ]
}

// ── Root ribbon button ──────────────────────────────────────────────────────

/** The « A » shown on the button face, styled like a WordArt sample. */
const BUTTON_A_STYLE: React.CSSProperties = {
  fontSize: 15, color: '#4472c4',
  textShadow: '1px 1px 1px rgba(0,0,0,0.35)',
}

export function TextEffectsButton({ editor }: { editor: Editor | null }): React.JSX.Element {
  const ref = useRef<HTMLButtonElement>(null)
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null)
  const openMenu = () => {
    const r = ref.current?.getBoundingClientRect()
    if (r) setPos({ top: r.bottom + 4, left: r.left })
  }
  const close = () => setPos(null)
  return (
    <>
      {/* Same gabarit as FGCaseMenu: small icon + chevron, hover surface. */}
      <button ref={ref} onMouseDown={e => e.preventDefault()} onClick={() => (pos ? close() : openMenu())}
        title="Effets de texte et typographie"
        className="flex items-center justify-center gap-0.5 h-[22px] px-1 rounded-xs text-text-secondary hover:bg-hover">
        <span style={BUTTON_A_STYLE}>A</span><ChevronDown size={11} />
      </button>
      {pos && (
        <MenuDropdown items={buildItems(editor, close)} pos={{ ...pos, minWidth: 300 }} onClose={close} />
      )}
    </>
  )
}
