// One input per argument, as in LibreOffice's wizard: the formula is BUILT rather
// than typed, so the user never has to remember the order or the separator.
//
// Variadic functions grow on demand (a new empty field appears once the last one
// is filled), and each field shows the live value of what it currently holds —
// that immediate feedback is what makes the wizard worth opening.

import type { FnArg } from './catalog'

export interface ArgumentFieldsProps {
  args: FnArg[]
  values: string[]
  onChange: (index: number, value: string) => void
  /** Live evaluation of one argument, for the value column. */
  preview?: (expr: string) => string
  optionalLabel: string
  emptyLabel: string
}

export function ArgumentFields({ args, values, onChange, preview, optionalLabel, emptyLabel }: ArgumentFieldsProps) {
  if (args.length === 0) {
    return <div className="text-text-tertiary py-2">{emptyLabel}</div>
  }
  // A variadic tail shows as many fields as are filled, plus one spare.
  const variadic = args[args.length - 1]?.variadic
  const base = variadic ? args.slice(0, -1) : args
  const extra = variadic ? Math.max(1, values.length - base.length + 1) : 0
  const rows = [
    ...base.map((a, i) => ({ key: `a${i}`, label: a.name, optional: a.optional, index: i })),
    ...Array.from({ length: extra }, (_, k) => ({
      key: `v${k}`,
      label: `${base[base.length - 1]?.name.replace(/\d+$/, '') ?? 'valeur'}${base.length + k + 1}`,
      optional: true,
      index: base.length + k,
    })),
  ]

  return (
    <div className="flex flex-col gap-1.5">
      {rows.map(r => {
        const v = values[r.index] ?? ''
        const val = v.trim() && preview ? preview(v.trim()) : ''
        return (
          <label key={r.key} className="flex items-center gap-2">
            <span className="w-28 flex-shrink-0 text-text-secondary truncate" title={r.label}>
              {r.label}{r.optional ? <span className="text-text-tertiary"> ({optionalLabel})</span> : null}
            </span>
            <input
              value={v}
              onChange={e => onChange(r.index, e.target.value)}
              className="flex-1 min-w-0 h-7 px-2 border border-border rounded bg-surface-0 font-mono outline-none focus:border-primary"
            />
            <span className="w-24 flex-shrink-0 text-right font-mono text-text-tertiary truncate" title={val}>{val}</span>
          </label>
        )
      })}
    </div>
  )
}
