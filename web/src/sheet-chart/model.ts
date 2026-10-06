// Spreadsheet chart engine v2 — normalized in-memory model.
//
// The persisted document model stays `SheetChart` (api.ts): a loose, additive,
// backward-compatible JSON shape shared with the xlsx converter (Rust side).
// `normalize.ts` turns any historical or v2 `SheetChart` into the `ChartSpec`
// defined here; everything downstream (data resolution, scales, layout,
// rendering) only ever sees a `ChartSpec`.
//
// Geometry (box / EMU anchor) is deliberately NOT part of the spec: it stays
// on `SheetChart` and is handled by the grid overlay, exactly like images.

// ── Enums ────────────────────────────────────────────────────────────────────

/** Normalized chart kind (LibreOffice chart2 naming: column = vertical bars). */
export type ChartKind =
  | 'column'   // vertical bars (legacy persisted type 'bar')
  | 'bar'      // horizontal bars (legacy persisted type 'hbar')
  | 'line'
  | 'area'
  | 'pie'      // holeSize > 0 → donut
  | 'scatter'
  | 'radar'
  | 'combo'    // column + line (per-series `kind` override, or last `numLines` series)

export type ChartStacking = 'none' | 'stacked' | 'percent'
export type LegendPosition = 'right' | 'left' | 'top' | 'bottom'
export type DataLabelMode = 'value' | 'percent'

// ── Spec pieces ──────────────────────────────────────────────────────────────

export interface AxisSpec {
  title?: string
  /** Manual bounds; undefined = automatic (ScaleAutomatism-style nice scale). */
  min?: number
  max?: number
  /** Number format pattern for tick labels (same dialect as cell formats). */
  format?: string
}

/** One data series. Either flat cell refs (xlsx import) or A1 ranges (UI v2). */
export interface SeriesSpec {
  /** Literal name (import strCache or user text). */
  name?: string
  /** Cell ref holding the series name, e.g. "B1" (kept live). */
  nameRef?: string
  /** Flat value refs, e.g. ["B2","B3","B4"] (xlsx import shape). */
  vals?: string[]
  /** Rectangular value range, e.g. "B2:B10" (UI-authored shape). */
  valsRange?: string
  /** Per-series category refs (xlsx import `cats`). */
  catRefs?: string[]
  /** X-values range (scatter). */
  xRange?: string
  /** Explicit series color "#RRGGBB"; default = palette by index. */
  color?: string
  /** Combo override: draw this series as column or line. */
  kind?: 'column' | 'line'
  /** Reserved for a secondary value axis (combo). Default 'primary'. */
  axis?: 'primary' | 'secondary'
}

/**
 * "Simple range" data source (wizard step 2). Series/categories are derived
 * from the rectangle at data-resolution time; undefined flags = auto-detect
 * with the Calc heuristics (fuins2.cxx / ScTable::HasColHeader).
 */
export interface RangeSource {
  range: string                       // "A1:D8"
  seriesIn?: 'cols' | 'rows'          // undefined = auto (cols; single data row → rows)
  firstRowHeader?: boolean            // undefined = auto
  firstColHeader?: boolean            // undefined = auto
}

// ── The normalized spec ──────────────────────────────────────────────────────

export interface ChartSpec {
  kind: ChartKind
  stacking: ChartStacking
  title?: string

  /** Explicit series (imports & advanced wizard). Empty when `source` is set. */
  series: SeriesSpec[]
  /** Global category refs (flat, import shape). */
  catRefs?: string[]
  /** Global categories range (UI shape). */
  catsRange?: string
  /** Simple-range source; when present it supersedes `series`/`catRefs`. */
  source?: RangeSource

  legend: { show: boolean | 'auto'; position: LegendPosition }
  /** Major gridlines. chart2 default: Y major only. */
  grid: { x: boolean; y: boolean }
  axisX: AxisSpec
  axisY: AxisSpec
  dataLabels?: DataLabelMode
  /** Explicit per-point colors (pie slices / single-series charts). */
  pointColors?: string[]

  // Line / scatter styling
  lines: boolean       // draw connecting lines
  symbols: boolean     // draw point markers
  smooth: boolean      // cubic-spline interpolation instead of straight segments

  // Pie / donut
  holeSize: number     // 0 = pie; 0 < h < 1 = donut inner-radius fraction
  explode: number      // slice offset as a fraction of the radius (chart2 default 0.5 when exploded)

  // Radar
  filled: boolean      // FilledNet

  // Combo fallback when no per-series `kind`: last N series drawn as lines.
  numLines: number
}

// ── chart2 defaults & palette ────────────────────────────────────────────────

/**
 * LibreOffice chart2 default series palette (officecfg Chart.xcs), cycled
 * modulo 12 by series index.
 */
export const CHART2_PALETTE: readonly string[] = [
  '#004586', '#ff420e', '#ffd320', '#579d1c', '#7e0021', '#83caff',
  '#314004', '#aecf00', '#4b1f6f', '#ff950e', '#c5000b', '#0084d1',
]

/** chart2 "gray30": axis lines, gridlines, legend border, plot outline. */
export const CHART_NEUTRAL = '#b3b3b3'

/** Color for series `i`, honouring an explicit override. */
export function seriesColor(i: number, explicit?: string): string {
  return explicit ?? CHART2_PALETTE[i % CHART2_PALETTE.length]
}

/** Marker shapes cycled by series index (chart2 standard symbols 0..7). */
export const SYMBOL_CYCLE: readonly string[] = [
  'square', 'diamond', 'arrow-down', 'arrow-up',
  'arrow-right', 'arrow-left', 'bowtie', 'hourglass',
]

// Typography (SVG px at zoom 1).
export const FONT_TITLE = 13
export const FONT_AXIS_TITLE = 11
export const FONT_LEGEND = 11
export const FONT_AXIS_LABEL = 10
export const FONT_DATA_LABEL = 10

// Geometry defaults (chart2).
export const BAR_GAP_RATIO = 1.0        // gap between category groups = 100% of one bar width
export const DONUT_HOLE_DEFAULT = 0.4   // inner-radius fraction when type 'donut' has no holeSize
export const PIE_EXPLODE_DEFAULT = 0.5  // chart2 PROP_PIE_TEMPLATE_DEFAULT_OFFSET
export const PAGE_MARGIN_RATIO = 0.02   // chart2 constPageLayoutDistancePercentage
export const LEGEND_MAX_WIDTH_RATIO = 0.3 // one legend entry text ≤ 30% of remaining width

/** Hard cap on points per range/series (mirrors backend MAX_RANGE_POINTS). */
export const MAX_CHART_POINTS = 4096
