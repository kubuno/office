// The row of ICON BUTTONS at the top of an object's context menu — the Windows 11
// Explorer pattern: clipboard-family commands are not text entries buried in the
// list but a compact strip of icons in the very same card, above a separator.
//
// Rendered as the FIRST `custom` item of a `MenuDropdown` (see `iconRowItems`), so
// there is no hand-rolled floating div anywhere. Each button is icon-only and gets a
// @ui Tooltip carrying its label and its keyboard shortcut — that tooltip IS the
// button's label, so it is never optional.

import type { ReactNode } from 'react'
import { Tooltip, type MenuItem, type MenuTheme } from '@ui'

export interface MenuIconAction {
  id: string
  /** Shown in the tooltip; also the accessible name. */
  label: string
  /** Shown after the label, in parentheses. */
  shortcut?: string
  icon: ReactNode
  disabled?: boolean
  /** Destructive tint (delete). */
  danger?: boolean
  /** Keep the menu open after the click (toggles); by default the menu closes. */
  keepOpen?: boolean
  onClick: () => void
}

const PALETTE: Record<MenuTheme, { text: string; hover: string; danger: string; disabled: string }> = {
  light: { text: '#202124', hover: 'rgba(0,0,0,0.06)', danger: '#d93025', disabled: '#9aa0a6' },
  dark:  { text: '#d6d6d6', hover: '#454545',          danger: '#e84a4a', disabled: '#6f6f6f' },
}

export function MenuIconRow({ actions, close, theme = 'light' }: {
  actions: MenuIconAction[]
  /** Provided by MenuDropdown to the `custom` renderer. */
  close: () => void
  theme?: MenuTheme
}) {
  const c = PALETTE[theme]
  return (
    <div className="flex items-center gap-1 px-2 py-1.5" role="group">
      {actions.map(a => (
        <Tooltip key={a.id} label={a.shortcut ? `${a.label} (${a.shortcut})` : a.label}>
          <button
            type="button"
            aria-label={a.label}
            disabled={a.disabled}
            onClick={() => { if (a.disabled) return; a.onClick(); if (!a.keepOpen) close() }}
            className="flex items-center justify-center rounded transition-colors"
            style={{
              width: 30, height: 28,
              color: a.disabled ? c.disabled : a.danger ? c.danger : c.text,
              cursor: a.disabled ? 'default' : 'pointer',
              opacity: a.disabled ? 0.55 : 1,
              background: 'transparent',
            }}
            onMouseEnter={e => { if (!a.disabled) e.currentTarget.style.background = c.hover }}
            onMouseLeave={e => { e.currentTarget.style.background = 'transparent' }}
          >
            {a.icon}
          </button>
        </Tooltip>
      ))}
    </div>
  )
}

/**
 * Wrap the row as the leading items of a `MenuDropdown`: the row itself plus the
 * separator that detaches it from the text entries. Returns an empty list when there
 * is nothing to show, so it can be spread unconditionally.
 */
export function iconRowItems(actions: MenuIconAction[], theme: MenuTheme = 'light'): MenuItem[] {
  if (!actions.length) return []
  return [
    { type: 'custom', render: close => <MenuIconRow actions={actions} close={close} theme={theme} /> },
    { type: 'separator' },
  ]
}
