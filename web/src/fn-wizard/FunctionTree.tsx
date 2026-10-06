// The wizard's left pane: functions grouped by CATEGORY in a collapsible tree,
// the way LibreOffice's Assistant Fonction presents them.
//
// A flat alphabetical list answers "I know the name"; the tree answers "I know
// what I want to do but not what it is called" — which is the reason the dialog
// exists. While a search is running every matching group is expanded, so the hits
// stay visible under their category instead of the user having to open each one.

import { ChevronDown, ChevronRight } from 'lucide-react'
import { CAT_COLOR } from '../formula-catalog'
import type { FnGroup, WizardFn } from './catalog'

export interface FunctionTreeProps {
  groups: FnGroup[]
  /** Categories currently open (controlled by the dialog). */
  expanded: Set<string>
  onToggle: (cat: string) => void
  selected: string | null
  onSelect: (fn: WizardFn) => void
  /** Double-click / Enter equivalent: take the function and close. */
  onConfirm: (fn: WizardFn) => void
  emptyLabel: string
}

export function FunctionTree({
  groups, expanded, onToggle, selected, onSelect, onConfirm, emptyLabel,
}: FunctionTreeProps) {
  if (groups.length === 0) {
    return <div className="p-4 text-center text-text-tertiary">{emptyLabel}</div>
  }
  return (
    <div role="tree" className="py-0.5">
      {groups.map(g => {
        const open = expanded.has(g.cat)
        return (
          <div key={g.cat}>
            <button
              type="button"
              role="treeitem"
              aria-expanded={open}
              onClick={() => onToggle(g.cat)}
              className="w-full flex items-center gap-1 px-2 py-1 text-left hover:bg-surface-1"
            >
              {open ? <ChevronDown size={13} className="text-text-tertiary" /> : <ChevronRight size={13} className="text-text-tertiary" />}
              <span className="w-1.5 h-1.5 rounded-full flex-shrink-0" style={{ background: CAT_COLOR[g.cat] }} />
              <span className="text-text-secondary">{g.label}</span>
              <span className="ml-auto text-text-tertiary">{g.fns.length}</span>
            </button>
            {open && g.fns.map(f => (
              <button
                key={f.name}
                type="button"
                role="treeitem"
                onClick={() => onSelect(f)}
                onDoubleClick={() => onConfirm(f)}
                className={`w-full flex items-center gap-2 py-1 pr-2 text-left hover:bg-surface-1 ${selected === f.name ? 'bg-primary-light' : ''}`}
                style={{ paddingLeft: 30 }}
              >
                <span className="font-mono" style={{ color: CAT_COLOR[f.cat] }}>{f.name}</span>
                <span className="ml-auto text-text-tertiary truncate">{f.syntax}</span>
              </button>
            ))}
          </div>
        )
      })}
    </div>
  )
}
