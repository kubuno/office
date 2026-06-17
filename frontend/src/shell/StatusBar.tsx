import React from 'react'
import { Minus, Plus } from 'lucide-react'

// Shared status-bar primitives for the Office sub-modules (Word-like bottom bar).
// Mirrors the look of the Documents status bar so every sub-editor is consistent:
// a thin 28px row at the bottom, left = contextual info, right = view/zoom tools.

/** Bottom status-bar container. Place it as the LAST flex child of the editor's
 *  flex-col area (after the scrolling canvas), like Documents does. */
export function StatusBar({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-stretch h-7 flex-shrink-0 text-xs bg-[#f8f9fa] border-t border-[#dadce0] select-none overflow-x-auto"
         data-office-statusbar>
      {children}
    </div>
  )
}

/** A clickable (or static) status-bar cell. */
export function StatusButton({ onClick, active, title, children }: {
  onClick?: (e: React.MouseEvent) => void; active?: boolean; title?: string; children: React.ReactNode
}) {
  return (
    <button type="button" title={title} onClick={onClick}
      className={`flex items-center gap-1 h-full px-2 rounded-none whitespace-nowrap transition-colors
        ${active ? 'text-primary bg-primary/10' : 'text-text-secondary hover:bg-black/5'}`}>
      {children}
    </button>
  )
}

/** Vertical separator between status-bar groups. */
export function StatusSep() {
  return <div className="w-px my-1.5 bg-[#dadce0]" />
}

/** Flexible spacer pushing the following items to the right. */
export function StatusSpacer() {
  return <div className="flex-1" />
}

/** Reusable zoom control (− slider + reset%). `zoom` is a 0..n scale (1 = 100%). */
export function StatusZoom({ zoom, onZoom, min = 0.25, max = 3, sliderMin = 50, sliderMax = 200 }: {
  zoom: number; onZoom: (z: number) => void
  min?: number; max?: number; sliderMin?: number; sliderMax?: number
}) {
  const pct = Math.round(zoom * 100)
  const step = (d: number) => onZoom(Math.min(max, Math.max(min, Math.round((zoom + d) * 100) / 100)))
  return (
    <>
      <button type="button" onClick={() => step(-0.1)} title="Zoom arrière"
        className="flex items-center px-1.5 text-text-secondary hover:bg-black/5"><Minus size={14} /></button>
      <div className="flex items-center px-1">
        <input type="range" min={sliderMin} max={sliderMax} step={10} value={Math.min(sliderMax, Math.max(sliderMin, pct))}
          onChange={e => onZoom(Number(e.target.value) / 100)}
          title={`${pct} %`} className="w-28 cursor-pointer h-1"
          style={{ accentColor: 'var(--color-primary, #1a73e8)' }} />
      </div>
      <button type="button" onClick={() => step(0.1)} title="Zoom avant"
        className="flex items-center px-1.5 text-text-secondary hover:bg-black/5"><Plus size={14} /></button>
      <button type="button" onClick={() => onZoom(1)} title="Rétablir à 100 %"
        className="flex items-center px-2 text-text-secondary hover:bg-black/5 tabular-nums w-14 justify-center">{pct} %</button>
    </>
  )
}
