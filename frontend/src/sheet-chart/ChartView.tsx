// Root chart component: SheetChart (persisted, any vintage) → normalize →
// resolve data → layout → dispatch to the plot renderer. Theme-aware SVG
// sized to the real box (no viewBox stretching: pass the actual pixel size).

import { useMemo } from 'react'
import type { ReactNode } from 'react'
import type { SheetChart } from '../api'
import { normalizeChart, isCartesian } from './normalize'
import { resolveChartData } from './data'
import type { CellValueGetter } from './data'
import { computeLayout, approxMeasure } from './layout'
import type { Rect, TextMeasurer } from './layout'
import { CHART_NEUTRAL } from './model'
import { CartesianPlot } from './render/cartesian'
import { CircularPlot, pieLegendData } from './render/circular'
import { RadarPlot } from './render/radar'
import { ComboPlot, hasSecondaryAxis } from './render/combo'
import { Legend } from './render/legend'

export interface ChartViewProps {
  chart: SheetChart
  /** Real box size in px (already multiplied by the grid zoom). */
  width: number
  height: number
  /** Bare-A1 cell accessor on the active sheet; formulas pre-evaluated by the
   *  host. Its identity must change with the sheet revision (memo key). */
  getCell: CellValueGetter
  /** i18n lookup (fallback series names, empty state). */
  t?: (key: string, opts?: { defaultValue?: string }) => string
  /** Optional precise text measurer (e.g. canvas-based); default heuristic. */
  measure?: TextMeasurer
}

function CenterMessage({ rect, text }: { rect: Rect; text: string }): ReactNode {
  return (
    <text
      x={rect.x + rect.w / 2} y={rect.y + rect.h / 2}
      textAnchor="middle" fontSize={11} fill={CHART_NEUTRAL}
    >
      {text}
    </text>
  )
}

/**
 * Renders a spreadsheet chart. Works without an id (wizard live preview).
 * Data resolution is memoized on (chart, getCell): the host passes a fresh
 * `getCell` when cells change so charts recompute with the grid.
 */
export function ChartView({ chart, width, height, getCell, t, measure }: ChartViewProps): ReactNode {
  const m = measure ?? approxMeasure
  const spec = useMemo(() => normalizeChart(chart), [chart])
  const data = useMemo(
    () => resolveChartData(spec, getCell, {
      seriesName: i => `${t?.('sheet_chart_series', { defaultValue: 'Series' }) ?? 'Series'} ${i + 1}`,
    }),
    [spec, getCell, t],
  )
  // Pie/donut legends list categories (per-point colors); others list series.
  const legendData = useMemo(() => spec.kind === 'pie' ? pieLegendData(data) : data, [spec.kind, data])
  const layout = useMemo(() => computeLayout(width, height, spec, legendData, m), [width, height, spec, legendData, m])

  const hasData = data.series.some(s => s.values.some(v => v != null))

  let plot: ReactNode
  if (!hasData) {
    plot = <CenterMessage rect={layout.plot} text={t?.('sheet_chart_no_data', { defaultValue: 'No data' }) ?? 'No data'} />
  } else if (spec.kind === 'combo' && hasSecondaryAxis(data)) {
    // Secondary-axis combos need their own right-hand value axis.
    plot = <ComboPlot spec={spec} data={data} layout={layout} measure={m} />
  } else if (isCartesian(spec.kind)) {
    plot = <CartesianPlot spec={spec} data={data} layout={layout} measure={m} />
  } else if (spec.kind === 'pie') {
    plot = <CircularPlot spec={spec} data={data} layout={layout} measure={m} />
  } else {
    plot = <RadarPlot spec={spec} data={data} layout={layout} measure={m} />
  }

  return (
    <svg
      width={width} height={height} viewBox={`0 0 ${width} ${height}`}
      role="img" aria-label={spec.title}
      style={{ display: 'block', fontFamily: 'var(--font-family-sans, system-ui, sans-serif)' }}
    >
      <rect x={0} y={0} width={width} height={height} fill="var(--color-surface-0, #ffffff)" />
      {layout.title && (
        <text
          x={layout.title.x} y={layout.title.y} fontSize={layout.title.fontSize}
          fontWeight={600} textAnchor="middle" fill="var(--color-text-primary, #202124)"
        >
          {layout.title.text}
        </text>
      )}
      {layout.axisTitleX && (
        <text
          x={layout.axisTitleX.x} y={layout.axisTitleX.y} fontSize={layout.axisTitleX.fontSize}
          textAnchor="middle" fill="var(--color-text-secondary, #5f6368)"
        >
          {layout.axisTitleX.text}
        </text>
      )}
      {layout.axisTitleY && (
        <text
          x={layout.axisTitleY.x} y={layout.axisTitleY.y} fontSize={layout.axisTitleY.fontSize}
          textAnchor="middle" fill="var(--color-text-secondary, #5f6368)"
          transform={`rotate(-90 ${layout.axisTitleY.x} ${layout.axisTitleY.y})`}
        >
          {layout.axisTitleY.text}
        </text>
      )}
      {layout.legend && <Legend legend={layout.legend} data={legendData} measure={m} />}
      {plot}
    </svg>
  )
}
