// BLOCK 1 of an object's context menu, Excel-style: a small floating card of quick
// formatting controls that appears NEXT TO the context menu (a separate card, above
// the click point), while the menu itself is block 2.
//
// Generic on purpose — it takes a list of controls and knows nothing about charts,
// pictures or shapes. Excel shows: [Remplissage ▾][Contour ▾] on a chart,
// [Style ▾][Rogner] on a picture, [Style ▾][Remplissage ▾][Contour ▾] on a shape.
//
// Coexistence with the open MenuDropdown (which closes itself on any `mousedown`
// landing outside its own root):
//   • the card swallows `mousedown` (stopPropagation) so touching it never closes
//     the menu, and preventDefault keeps the focus where it was — no focus theft;
//   • every panel it opens is a CHILD of the card, so palettes are covered by the
//     same guard instead of counting as an outside click;
//   • the two blocks share ONE lifetime: the host holds a single piece of state and
//     `onClose` tears both down together, so acting on either dismisses both.

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { placeBlock, placePair } from './placement'
import { createPortal } from 'react-dom'
import type { TFunction } from 'i18next'
import { ColorSwatchPicker } from '@ui'

export type MiniControl =
  | { id: string; kind: 'button'; label: string; icon?: ReactNode; disabled?: boolean; active?: boolean; onClick: () => void }
  | {
      id: string; kind: 'color'; label: string; icon?: ReactNode; disabled?: boolean
      /** Current colour shown in the swatch ('none' / '' draws the empty marker). */
      color: string
      onPick: (hex: string) => void
      /** Adds a "no fill / no outline" row above the palette when both are given. */
      noneLabel?: string
      onNone?: () => void
    }
  | {
      id: string; kind: 'menu'; label: string; icon?: ReactNode; disabled?: boolean
      /** Free content (style gallery, thickness list…) — receives a closer. */
      render: (close: () => void) => ReactNode
    }
  | { id: string; kind: 'separator' }

export interface MiniToolbarProps {
  controls: MiniControl[]
  /** Click point in CLIENT coordinates — the same origin as MenuDropdown's `pos`. */
  x: number
  y: number
  /** Closes both blocks (the card and the context menu). */
  onClose: () => void
  t?: TFunction
}

export function MiniToolbar({ controls, x, y, onClose, t }: MiniToolbarProps) {
  const cardRef = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState<string | null>(null)
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)

  // Placement is delegated to `placement.ts`, which guarantees three invariants:
  // both cards fully on screen, never overlapping, always separated by a gap.
  // The click point alone is not enough — the menu positions itself and flips when
  // it would not fit — so the menu is located (probe) and MEASURED first.
  useLayoutEffect(() => {
    const el = cardRef.current; if (!el) return
    const place = () => {
      const r = el.getBoundingClientRect()
      const vp = { w: window.innerWidth, h: window.innerHeight }
      const block = { w: r.width, h: r.height }

      // Find the context menu opened alongside us. The card is still
      // `visibility: hidden` here, so it cannot be its own probe target.
      const probe = document.elementFromPoint(
        Math.min(vp.w - 1, Math.max(0, x)),
        Math.min(vp.h - 1, Math.max(0, y)),
      )
      let layer: HTMLElement | null = null
      for (let n = probe as HTMLElement | null; n && n !== document.body; n = n.parentElement) {
        const cs = getComputedStyle(n)
        if (cs.position === 'fixed' && Number(cs.zIndex || 0) >= 1000) { layer = n; break }
      }

      // No menu next to us: plain placement above the click point.
      if (!layer) {
        const p = placePair({ x, y }, { w: 0, h: 0 }, block, vp)
        setPos({ left: p.block.x, top: p.block.y })
        return
      }
      // With a menu: place against its MEASURED rect. The helper guarantees the
      // card stays on screen, never overlaps the menu and keeps the gap — moving
      // or capping the menu itself when that is the only way (both are applied
      // here, the menu positions itself otherwise).
      // Measured on the portal ROOT. It is transparent and stretches to the right
      // viewport edge, but it hugs the visible card vertically — which is the axis
      // the gap is read on. Descending to the painted card instead was measured to
      // push the block ~85px away, so the simpler rect is also the better one.
      const lr = layer.getBoundingClientRect()
      const p = placeBlock({ left: lr.left, top: lr.top, w: lr.width, h: lr.height }, block, vp)
      if (p.menuTop != null) layer.style.top = `${p.menuTop}px`
      if (p.menuMaxHeight != null) {
        layer.style.maxHeight = `${p.menuMaxHeight}px`
        layer.style.overflowY = 'auto'
      }
      setPos({ left: p.left, top: p.top })
    }
    place()
    window.addEventListener('resize', place)
    return () => window.removeEventListener('resize', place)
  }, [x, y, controls.length])

  // The card owns its OWN lifetime, independent of the context menu next to it — the
  // two blocks are opened together but dismissed on their own terms, exactly like
  // Excel: touching the mini bar dismisses the menu (whose own `document` mousedown
  // listener sees a click outside itself) while the bar stays up to show its palette.
  // Trying instead to shield the menu from that click is a losing game: the menu
  // listens on `document`, and a React-level `stopPropagation` does not reliably beat
  // a listener registered there.
  useEffect(() => {
    const onDown = (e: MouseEvent) => { if (!cardRef.current?.contains(e.target as Node)) onClose() }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [onClose])

  // Escape closes the open panel first, then the whole thing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.stopPropagation()
      if (open) setOpen(null); else onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onClose])

  const swatch = (color: string) => (
    <span
      className="inline-block rounded-[2px] border"
      style={{
        width: 12, height: 12,
        borderColor: 'var(--kb-float-border, rgba(0,0,0,0.12))',
        background: !color || color === 'none'
          ? 'linear-gradient(135deg, transparent 45%, #d93025 45%, #d93025 55%, transparent 55%)'
          : color,
      }}
    />
  )

  const btnClass = 'flex items-center gap-1.5 h-7 px-2 rounded hover:bg-hover text-text-primary whitespace-nowrap'

  return createPortal(
    <div
      ref={cardRef}
      data-kubuno-floating=""
      // preventDefault only — no focus theft, while letting the click travel so the
      // context menu next door dismisses itself (see the lifetime note above).
      onMouseDown={e => e.preventDefault()}
      onContextMenu={e => { e.preventDefault(); e.stopPropagation() }}
      // Above the context menu's own layer (9999): the two cards are siblings and the
      // card must stay readable even when the menu grows over the intended spot.
      className="fixed z-[10000] flex items-center gap-0.5 px-1 py-1"
      style={{
        left: pos?.left ?? x, top: pos?.top ?? y,
        visibility: pos ? 'visible' : 'hidden',
        borderRadius: 'var(--kb-float-radius, 10px)',
        background: 'var(--kb-float-surface, rgba(255,255,255,0.72))',
        backdropFilter: 'var(--kb-float-filter, blur(24px) saturate(180%))',
        WebkitBackdropFilter: 'var(--kb-float-filter, blur(24px) saturate(180%))',
        border: '1px solid var(--kb-float-border, rgba(0,0,0,0.08))',
        boxShadow: 'var(--kb-shadow-float, 0 2px 6px rgba(0,0,0,0.08))',
      }}
    >
      {controls.map(c => {
        if (c.kind === 'separator') {
          return <span key={c.id} className="self-stretch w-px my-1" style={{ background: 'var(--kb-float-border, rgba(0,0,0,0.08))' }} />
        }
        const isOpen = open === c.id
        return (
          <div key={c.id} className="relative">
            <button
              type="button"
              disabled={c.disabled}
              aria-label={c.label}
              className={btnClass}
              style={{
                opacity: c.disabled ? 0.5 : 1,
                cursor: c.disabled ? 'default' : 'pointer',
                background: (c.kind === 'button' && c.active) || isOpen ? 'var(--kb-black-08, rgba(0,0,0,0.08))' : undefined,
              }}
              onClick={() => {
                if (c.disabled) return
                if (c.kind === 'button') { c.onClick(); onClose(); return }
                setOpen(isOpen ? null : c.id)
              }}
            >
              {c.icon}
              <span>{c.label}</span>
              {c.kind === 'color' && swatch(c.color)}
              {c.kind !== 'button' && <span aria-hidden className="text-text-secondary">▾</span>}
            </button>

            {isOpen && c.kind === 'color' && (
              <div
                className="absolute left-0 top-full mt-1 rounded border border-border bg-white shadow-lg p-1 z-10"
                style={{ borderRadius: 'var(--kb-float-radius, 10px)' }}
              >
                {c.noneLabel && c.onNone && (
                  <button
                    type="button"
                    className="flex items-center gap-2 w-full h-7 px-2 rounded hover:bg-hover text-left"
                    onClick={() => { c.onNone?.(); setOpen(null); onClose() }}
                  >
                    {swatch('none')}<span>{c.noneLabel}</span>
                  </button>
                )}
                <ColorSwatchPicker
                  color={c.color && c.color !== 'none' ? c.color : '#ffffff'}
                  t={t}
                  onChange={hex => { c.onPick(hex); setOpen(null); onClose() }}
                  onClose={() => setOpen(null)}
                />
              </div>
            )}

            {isOpen && c.kind === 'menu' && (
              <div
                className="absolute left-0 top-full mt-1 rounded border border-border bg-white shadow-lg p-1 z-10"
                style={{ borderRadius: 'var(--kb-float-radius, 10px)' }}
              >
                {c.render(() => { setOpen(null); onClose() })}
              </div>
            )}
          </div>
        )
      })}
    </div>,
    document.body,
  )
}
