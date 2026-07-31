// Small shared form controls for the chart wizard steps.

export const WIZ_INPUT = 'h-8 px-2 border border-[#dadce0] rounded outline-none focus:border-primary bg-transparent text-xs w-full'

export function Field({ label, children, className }: { label: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={`flex items-center gap-2 ${className ?? ''}`}>
      <label className="w-36 shrink-0 text-text-secondary text-xs">{label}</label>
      <div className="flex-1 min-w-0">{children}</div>
    </div>
  )
}

/** Segmented control (radio-group replacement, no native inputs). */
export function Seg<T extends string>({ options, value, onChange, disabled }: {
  options: { value: T; label: string }[]
  value: T
  onChange: (v: T) => void
  disabled?: boolean
}) {
  return (
    <div className={`inline-flex rounded border border-[#dadce0] overflow-hidden ${disabled ? 'opacity-50 pointer-events-none' : ''}`}>
      {options.map((o, i) => (
        <button key={o.value} type="button" onClick={() => onChange(o.value)}
          className={`px-2.5 h-7 text-xs ${i > 0 ? 'border-l border-[#dadce0]' : ''} ${o.value === value ? 'bg-[#e8f0fe] text-primary' : 'hover:bg-[#f1f3f4]'}`}>
          {o.label}
        </button>
      ))}
    </div>
  )
}
