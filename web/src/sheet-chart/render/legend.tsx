// Legend renderer: draws a LegendLayout computed by layout.ts. The bullet
// mirrors the series' mark (filled box for bars/areas, line + marker for
// lines, marker for scatter). Text wears the theme ink, never a series color.

import type { ReactNode } from 'react'
import type { ChartData } from '../data'
import { approxMeasure } from '../layout'
import type { LegendLayout, TextMeasurer } from '../layout'
import { truncateToWidth } from './format'
import { Marker, symbolFor } from './marks'

export interface LegendProps {
  legend: LegendLayout
  /** Resolved data: lets bullets mirror each series' drawing kind. */
  data?: ChartData
  measure?: TextMeasurer
}

export function Legend({ legend, data, measure = approxMeasure }: LegendProps): ReactNode {
  const { rect, entries, symbolSize: s, fontSize: fs } = legend
  const gap = Math.max(4, fs * 0.3) // must match layoutLegend's symbol→text gap
  return (
    <g>
      {entries.map((e, i) => {
        const kind = data?.series[i]?.kind
        const sx = rect.x + e.x
        const sy = rect.y + e.y
        const cx = sx + s / 2
        const cy = sy + s / 2
        const textX = sx + s + gap
        const label = truncateToWidth(e.label, Math.max(rect.x + rect.w - textX - 4, 12), fs, measure)
        let bullet: ReactNode
        if (kind === 'line') {
          bullet = (
            <>
              <line x1={sx - 1} x2={sx + s + 1} y1={cy} y2={cy} stroke={e.color} strokeWidth={2} />
              <Marker shape={symbolFor(i)} cx={cx} cy={cy} r={s * 0.32} fill={e.color} />
            </>
          )
        } else if (kind === 'scatter') {
          bullet = <Marker shape={symbolFor(i)} cx={cx} cy={cy} r={s * 0.42} fill={e.color} />
        } else {
          bullet = <rect x={sx} y={sy} width={s} height={s} rx={2} fill={e.color} />
        }
        return (
          <g key={i}>
            {bullet}
            <text
              x={textX} y={cy + fs * 0.35} fontSize={fs}
              fill="var(--color-text-secondary, #5f6368)"
            >
              {label}
            </text>
          </g>
        )
      })}
    </g>
  )
}
