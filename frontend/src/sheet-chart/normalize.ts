// SheetChart (persisted, any vintage) → ChartSpec (normalized, in-memory).
//
// Backward compatibility contract:
//  - legacy UI charts: { type, range, bx/by/bw/bh } only;
//  - xlsx imports: { type, vals/cats flat, series?, grouping?, legend?,
//    dataLabels?, colors?, EMU anchor } — `series`/`grouping` were historically
//    persisted although absent from the old TS type;
//  - v2 charts: any of the additive fields typed in api.ts.
// Normalization is read-only: it never mutates the SheetChart, and editing UIs
// write v2 fields back onto the SheetChart object (spread-preserving unknowns).

import type { SheetChart } from '../api'
import {
  ChartKind, ChartSpec, ChartStacking, SeriesSpec,
  DONUT_HOLE_DEFAULT,
} from './model'

/** Persisted `type` → normalized kind (legacy 'bar' = vertical → 'column'). */
export function normalizeKind(type: string | undefined): ChartKind {
  switch (type) {
    case 'hbar':    return 'bar'
    case 'line':    return 'line'
    case 'area':    return 'area'
    case 'pie':     return 'pie'
    case 'donut':   return 'pie'
    case 'scatter': return 'scatter'
    case 'radar':   return 'radar'
    case 'combo':   return 'combo'
    case 'bar':
    default:        return 'column'
  }
}

function normalizeStacking(grouping: string | undefined): ChartStacking {
  if (grouping === 'stacked') return 'stacked'
  if (grouping === 'percentStacked') return 'percent'
  return 'none'
}

const CARTESIAN: ReadonlySet<ChartKind> = new Set(['column', 'bar', 'line', 'area', 'scatter', 'combo'])

export function isCartesian(kind: ChartKind): boolean { return CARTESIAN.has(kind) }

/** Normalize a persisted chart of any vintage into a ChartSpec. */
export function normalizeChart(chart: SheetChart): ChartSpec {
  const kind = normalizeKind(chart.type)

  // Data source precedence: explicit series > flat vals/cats > simple range.
  let series: SeriesSpec[] = []
  let source: ChartSpec['source']
  if (chart.series && chart.series.length > 0) {
    series = chart.series.map(s => ({
      name: s.name,
      nameRef: s.nameRef,
      vals: s.vals,
      valsRange: s.valsRange,
      catRefs: s.cats,
      xRange: s.xRange,
      color: s.color,
      kind: s.kind,
      axis: s.axis,
    }))
  } else if (chart.vals && chart.vals.length > 0) {
    series = [{ vals: chart.vals, catRefs: chart.cats }]
  } else if (chart.range) {
    source = {
      range: chart.range,
      seriesIn: chart.seriesIn,
      firstRowHeader: chart.firstRowHeader,
      firstColHeader: chart.firstColHeader,
    }
  }

  const isPieLike = kind === 'pie'
  return {
    kind,
    stacking: kind === 'pie' || kind === 'scatter' ? 'none' : normalizeStacking(chart.grouping),
    title: chart.title,
    series,
    catRefs: chart.cats && series.length > 0 && !series[0]?.catRefs ? chart.cats : undefined,
    catsRange: chart.catsRange,
    source,
    // Legacy semantics: only pie honoured `legend` (shown unless === false);
    // 'auto' lets the renderer show the legend for pies and multi-series charts.
    legend: {
      show: chart.legend ?? 'auto',
      position: chart.legendPos ?? 'right',
    },
    // chart2 default: Y major gridlines only, none for pie/radar.
    grid: {
      x: chart.grid?.x ?? false,
      y: chart.grid?.y ?? isCartesian(kind),
    },
    axisX: { ...chart.axisX },
    axisY: { ...chart.axisY },
    dataLabels: chart.dataLabels,
    pointColors: chart.colors,
    lines: chart.lines ?? (kind === 'scatter' ? false : true),
    symbols: chart.symbols ?? (kind === 'scatter'),
    smooth: chart.smooth ?? false,
    holeSize: isPieLike ? (chart.holeSize ?? (chart.type === 'donut' ? DONUT_HOLE_DEFAULT : 0)) : 0,
    explode: chart.explode ?? 0,
    filled: chart.filled ?? false,
    numLines: Math.max(1, chart.numLines ?? 1),
  }
}
