// The small controls of the formula bar, in Excel's order:
//   [Name Box] |grip| [x] [check] [fx] [formula field] [chevron]
//
// - Grip: drags the boundary between the Name Box and the formula field.
// - Cancel / Confirm: only live while a value is being edited (Excel greys them
//   out otherwise); they mirror Escape and Enter.
// - Chevron: expands the bar for multi-line editing; the bar's bottom edge can
//   also be dragged (see BarResizeEdge).

import { Check, ChevronDown, ChevronUp, GripVertical, X } from 'lucide-react'

const BTN = 'flex items-center justify-center h-full px-1.5 disabled:opacity-40 disabled:hover:bg-transparent'

export interface BarGripProps {
  onDrag: (dx: number) => void
  onDragEnd?: () => void
  title?: string
}

/** Vertical grip between the Name Box and the formula field. */
export function BarGrip({ onDrag, onDragEnd, title }: BarGripProps) {
  const start = (e: React.MouseEvent) => {
    e.preventDefault()
    const x0 = e.clientX
    const move = (ev: MouseEvent) => onDrag(ev.clientX - x0)
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      onDragEnd?.()
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }
  return (
    <div
      onMouseDown={start}
      title={title}
      className="flex h-full flex-shrink-0 cursor-col-resize items-center justify-center px-0.5 text-text-tertiary hover:text-primary"
    >
      <GripVertical size={12} />
    </div>
  )
}

export interface EditActionsProps {
  /** A value is being edited: the two buttons become active. */
  editing: boolean
  onCancel: () => void
  onConfirm: () => void
  onFx: () => void
  t?: (key: string, opts?: { defaultValue?: string }) => string
}

/** Cancel (x), Confirm (check) and Insert function (fx). */
export function EditActions({ editing, onCancel, onConfirm, onFx, t }: EditActionsProps) {
  const tr = (k: string, d: string) => (t ? t(k, { defaultValue: d }) : d)
  return (
    <div className="flex h-full flex-shrink-0 items-center border-r border-[#e2e4e6]">
      <button
        type="button" disabled={!editing} onMouseDown={e => e.preventDefault()} onClick={onCancel}
        title={tr('sheet_fb_cancel', 'Annuler (Échap)')} aria-label={tr('sheet_fb_cancel', 'Annuler')}
        className={`${BTN} text-text-tertiary hover:bg-[#fce8e6] hover:text-[#d93025]`}
      >
        <X size={14} />
      </button>
      <button
        type="button" disabled={!editing} onMouseDown={e => e.preventDefault()} onClick={onConfirm}
        title={tr('sheet_fb_confirm', 'Valider (Entrée)')} aria-label={tr('sheet_fb_confirm', 'Valider')}
        className={`${BTN} text-text-tertiary hover:bg-[#e6f4ea] hover:text-[#188038]`}
      >
        <Check size={14} />
      </button>
      <button
        type="button" onClick={onFx}
        title={tr('fnb_title', 'Insérer une fonction')} aria-label={tr('fnb_title', 'Insérer une fonction')}
        className={`${BTN} italic text-text-tertiary hover:bg-[#e8f0fe] hover:text-primary`}
      >
        fx
      </button>
    </div>
  )
}

export interface ExpandToggleProps {
  expanded: boolean
  onToggle: () => void
  t?: (key: string, opts?: { defaultValue?: string }) => string
}

/** Chevron expanding / collapsing the formula field. */
export function ExpandToggle({ expanded, onToggle, t }: ExpandToggleProps) {
  const tr = (k: string, d: string) => (t ? t(k, { defaultValue: d }) : d)
  const label = expanded
    ? tr('sheet_fb_collapse', 'Réduire la barre de formule (Ctrl+Maj+U)')
    : tr('sheet_fb_expand', 'Développer la barre de formule (Ctrl+Maj+U)')
  return (
    <button
      type="button" onMouseDown={e => e.preventDefault()} onClick={onToggle}
      title={label} aria-label={label} aria-expanded={expanded}
      className="flex h-full flex-shrink-0 items-center justify-center border-l border-[#e2e4e6] px-1.5 text-text-tertiary hover:bg-[#e8f0fe] hover:text-primary"
    >
      {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
    </button>
  )
}

export interface BarResizeEdgeProps {
  onDrag: (dy: number) => void
  onDragEnd?: () => void
  title?: string
}

/** Draggable bottom edge of the bar (free height, like Excel). */
export function BarResizeEdge({ onDrag, onDragEnd, title }: BarResizeEdgeProps) {
  const start = (e: React.MouseEvent) => {
    e.preventDefault()
    const y0 = e.clientY
    const move = (ev: MouseEvent) => onDrag(ev.clientY - y0)
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      onDragEnd?.()
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }
  return (
    <div
      onMouseDown={start}
      title={title}
      // Thin strip overlapping the bottom border, with a taller hit area.
      className="absolute inset-x-0 bottom-0 z-10 h-1 cursor-row-resize hover:bg-[#e8f0fe]"
      style={{ marginBottom: -1 }}
    />
  )
}
