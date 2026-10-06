// Small shared controls for the three GENERIC object dialogs of this folder
// (AltTextDialog, SizePropsDialog, FormatObjectDialog).
//
// Kept in one place so the three dialogs stay visually identical and none of them
// re-implements a number field, a section header or a button footer. Nothing here
// knows about charts, pictures or shapes — nor about the editor: every control is
// plain props in / callback out.

import { useCallback, useState, type ReactNode } from 'react'
import type { TFunction } from 'i18next'
import { Button } from '@ui'
import { DLG_BTN } from '../lib'

/** Rounded to 0.1 px so the fields stay readable after a free resize. */
export const r1 = (n: number) => Math.round(n * 10) / 10

/** Degrees folded back into [0, 360). */
export const norm360 = (deg: number) => ((Math.round(deg) % 360) + 360) % 360

/**
 * Draft state for a dialog that edits an object of type `T`.
 *
 * Two modes, chosen by the presence of `onChange`:
 *   • `onChange` given  → fully CONTROLLED: the draft IS `value`, every edit is
 *     reported upwards (use it for a live-preview pane);
 *   • `onChange` omitted → the draft lives here and only `onSubmit` commits (the
 *     usual OK / Annuler dialog).
 *
 * In uncontrolled mode the draft is seeded ONCE, on mount: the host mounts one
 * dialog per object and unmounts it on close, so re-seeding from the prop could
 * only ever fight the user's typing when the parent re-renders with a fresh object
 * literal. To edit another object, remount (change the React `key`).
 */
export function useDraft<T extends object>(value: T, onChange?: (next: T) => void) {
  const [local, setLocal] = useState<T>(value)
  const draft = onChange ? value : local
  const patch = useCallback((p: Partial<T>) => {
    if (onChange) onChange(Object.assign({}, value, p) as T)
    else setLocal(prev => Object.assign({}, prev, p) as T)
  }, [onChange, value])
  return [draft, patch] as const
}

/** Titled block of fields — the dialogs' only sectioning device. */
export function Section({ title, children, className = '' }: {
  title?: string
  children: ReactNode
  className?: string
}) {
  return (
    <div className={`space-y-2 ${className}`}>
      {title && <div className="text-text-secondary">{title}</div>}
      {children}
    </div>
  )
}

/**
 * Inline-labelled numeric field ("Largeur   [ 320 ] px").
 *
 * A raw `<input type="number">` rather than `@ui`'s `NumberInput`, which stacks its
 * label above the field and takes no unit suffix — the compact Excel-pane rows this
 * dialog family needs are not expressible with it. Styling matches the module's
 * other dialogs.
 */
export function NumField({ label, value, onChange, suffix, min, max, step = 1, disabled, labelWidth = 112 }: {
  label: string
  value: number
  onChange: (n: number) => void
  suffix?: string
  min?: number
  max?: number
  step?: number
  disabled?: boolean
  labelWidth?: number
}) {
  return (
    <label className="flex items-center gap-2" style={{ opacity: disabled ? 0.5 : 1 }}>
      <span className="text-text-secondary shrink-0" style={{ width: labelWidth }}>{label}</span>
      <input
        type="number"
        value={Number.isFinite(value) ? value : 0}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={e => { const n = Number(e.target.value); if (Number.isFinite(n)) onChange(n) }}
        // The sheet listens for keys on the window; a field must never feed it.
        onKeyDown={e => e.stopPropagation()}
        className="w-24 h-8 px-2 border border-[#dadce0] rounded outline-none focus:border-primary text-right"
      />
      {suffix && <span className="text-text-secondary">{suffix}</span>}
    </label>
  )
}

/**
 * Colour chip. An empty / 'none' colour draws the red diagonal used everywhere else
 * in the module (same marker as the object mini toolbar's swatches).
 */
export function SwatchDot({ color, size = 14 }: { color?: string; size?: number }) {
  const empty = !color || color === 'none'
  return (
    <span
      className="inline-block rounded-[2px] border shrink-0"
      style={{
        width: size, height: size,
        borderColor: 'var(--kb-float-border, rgba(0,0,0,0.12))',
        background: empty
          ? 'linear-gradient(135deg, transparent 45%, #d93025 45%, #d93025 55%, transparent 55%)'
          : color,
      }}
    />
  )
}

/**
 * Dialog footer: action first, cancel second, SAME width (module rule, cf. DLG_BTN).
 * `left` takes a secondary command (e.g. "Rétablir la taille d'origine"), pushed to
 * the opposite edge.
 */
export function DialogFooter({ onOk, onCancel, okLabel, cancelLabel, left, okDisabled, t }: {
  onOk: () => void
  onCancel: () => void
  okLabel?: string
  cancelLabel?: string
  left?: ReactNode
  okDisabled?: boolean
  t: TFunction
}) {
  return (
    <div className="pt-2 mt-1 border-t border-border flex items-center justify-end gap-2">
      {left && <div className="mr-auto">{left}</div>}
      <Button className={DLG_BTN} variant="primary" disabled={okDisabled} onClick={onOk}>
        {okLabel ?? t('common_ok', { defaultValue: 'OK' })}
      </Button>
      <Button className={DLG_BTN} variant="ghost" onClick={onCancel}>
        {cancelLabel ?? t('common_cancel', { defaultValue: 'Annuler' })}
      </Button>
    </div>
  )
}
