// The two preview panes of Word's references dialog: « Aperçu avant impression »
// (what the printed table looks like) and « Aperçu web » (hyperlinks instead of
// page numbers). Both read `entryStyle`, so they cannot drift from what the
// generator actually produces.
import type { CSSProperties, ReactNode } from 'react'

import { entryStyle, leaderSample } from './formats'
import type { CommonTableSettings, TableFormat } from './types'

export interface PreviewRow {
  text: string
  level: number
  page: string
}

const styleOf = (format: TableFormat, level: number): CSSProperties => {
  const s = entryStyle(format, level)
  return {
    fontWeight: s.bold ? 600 : 400,
    fontStyle: s.italic ? 'italic' : 'normal',
    fontFamily: s.fontFamily,
    fontSize: s.fontSizePt ? `${s.fontSizePt}pt` : undefined,
    color: s.color,
    textTransform: s.uppercase ? 'uppercase' : undefined,
    paddingLeft: (level - 1) * 18,
  }
}

function Pane({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex-1 min-w-0">
      <div className="text-text-secondary mb-1">{label}</div>
      <div className="border border-border rounded-[--kb-radius-sm] bg-surface h-[164px] overflow-auto p-2">
        {children}
      </div>
    </div>
  )
}

/** Print pane: text, leader, right-aligned page number. */
export function PrintPreview({ label, rows, settings }: {
  label: string
  rows: PreviewRow[]
  settings: CommonTableSettings
}) {
  return (
    <Pane label={label}>
      {rows.map((r, i) => (
        <div key={i} className="flex items-baseline whitespace-nowrap leading-6" style={styleOf(settings.format, r.level)}>
          <span className="shrink-0">{r.text}</span>
          {settings.showPageNumbers && (
            <>
              <span className="flex-1 min-w-0 overflow-hidden text-text-tertiary px-1">
                {leaderSample(settings.leader, 60)}
              </span>
              <span className={settings.rightAlign ? 'shrink-0' : 'shrink-0 pl-1'}>{r.page}</span>
            </>
          )}
        </div>
      ))}
    </Pane>
  )
}

/** Web pane: the entry becomes a link, no page number. */
export function WebPreview({ label, rows, hyperlinks, format }: {
  label: string
  rows: PreviewRow[]
  hyperlinks: boolean
  format: TableFormat
}) {
  return (
    <Pane label={label}>
      {rows.map((r, i) => (
        <div key={i} className="leading-7" style={styleOf(format, r.level)}>
          {hyperlinks
            ? <span className="text-primary underline cursor-pointer">{r.text}</span>
            : <span>{r.text}</span>}
        </div>
      ))}
    </Pane>
  )
}
