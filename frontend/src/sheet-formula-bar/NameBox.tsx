// Name Box: an EDITABLE combo box, like Excel's.
//
// Typing a reference and pressing Enter jumps to it; the arrow opens the list of
// the workbook's defined names (and any extra entries the host supplies, e.g.
// tables) and picking one jumps there too. While an object is selected the host
// passes its label ("Graphique 1"), so the box doubles as an object read-out.

import { useEffect, useRef, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { NAME_BOX_DEFAULT } from './geometry'

export interface NameBoxEntry {
  /** Text shown in the list. */
  name: string
  /** What the box resolves to (defaults to `name`). */
  target?: string
  /** Optional right-aligned hint, e.g. the range a name points to. */
  hint?: string
}

export interface NameBoxProps {
  /** Current text (controlled by the host: address, range or object label). */
  value: string
  onChange: (v: string) => void
  /** Enter / list pick. Return true when the reference resolved, to blur. */
  onSubmit: (ref: string) => boolean
  /** Escape / blur: the host restores the canonical label. */
  onRevert: () => void
  entries?: NameBoxEntry[]
  width?: number
  t?: (key: string, opts?: { defaultValue?: string }) => string
  inputRef?: React.RefObject<HTMLInputElement | null>
}

export function NameBox({
  value, onChange, onSubmit, onRevert, entries = [], width = NAME_BOX_DEFAULT, t, inputRef,
}: NameBoxProps) {
  const tr = (k: string, d: string) => (t ? t(k, { defaultValue: d }) : d)
  const [open, setOpen] = useState(false)
  const wrapRef = useRef<HTMLDivElement>(null)
  const localRef = useRef<HTMLInputElement>(null)
  const ref = inputRef ?? localRef

  // A click anywhere else closes the list (the list is inline, not a portal, so a
  // plain document listener is enough and keeps the input's focus behaviour).
  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [open])

  const pick = (entry: NameBoxEntry) => {
    setOpen(false)
    const target = entry.target ?? entry.name
    onChange(target)
    onSubmit(target)
  }

  return (
    <div ref={wrapRef} className="relative flex-shrink-0 h-full" style={{ width }}>
      <div className="flex items-center h-full border-r border-[#e2e4e6] bg-white">
        <input
          ref={ref}
          value={value}
          title={tr('sheet_name_box', 'Zone Nom — aller à une cellule ou à un nom défini')}
          className="min-w-0 flex-1 h-full px-1 text-center font-mono text-text-secondary bg-transparent outline-none focus:bg-[#e8f0fe]"
          onChange={e => onChange(e.target.value)}
          onFocus={e => e.currentTarget.select()}
          onBlur={onRevert}
          onKeyDown={e => {
            if (e.key === 'Enter') { if (onSubmit(value)) e.currentTarget.blur(); e.preventDefault() }
            else if (e.key === 'Escape') { onRevert(); e.currentTarget.blur() }
            else if (e.key === 'ArrowDown' && entries.length) { setOpen(true); e.preventDefault() }
            e.stopPropagation()
          }}
        />
        <button
          type="button"
          aria-label={tr('sheet_name_box_open', 'Noms définis')}
          title={tr('sheet_name_box_open', 'Noms définis')}
          disabled={!entries.length}
          onMouseDown={e => { e.preventDefault(); if (entries.length) setOpen(o => !o) }}
          className="flex items-center justify-center h-full px-1 text-text-tertiary hover:text-primary disabled:opacity-40 disabled:hover:text-text-tertiary"
        >
          <ChevronDown size={12} />
        </button>
      </div>

      {open && entries.length > 0 && (
        <div
          className="absolute left-0 top-full z-30 max-h-64 overflow-y-auto rounded-b border border-[#e2e4e6] bg-white shadow-[var(--kb-shadow-menu,0_2px_6px_2px_rgba(0,0,0,.15))]"
          style={{ minWidth: Math.max(width, 180) }}
        >
          {entries.map(entry => (
            <button
              key={entry.name}
              type="button"
              onMouseDown={e => { e.preventDefault(); pick(entry) }}
              className="flex w-full items-center justify-between gap-3 px-2 py-1 text-left hover:bg-[#f1f3f4]"
            >
              <span className="truncate">{entry.name}</span>
              {entry.hint && <span className="flex-shrink-0 font-mono text-text-tertiary">{entry.hint}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
