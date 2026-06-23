// Visual catalog + renderers for the Data (BI) sub-module. ~35 Power BI-style
// visuals built on responsive SVG / DOM. All visuals read a free-form
// `WidgetConfig` (persisted as JSONB) so new options never need a schema change.
import { type ReactNode } from 'react'
import {
  TrendingUp, TrendingDown, BarChart3, BarChartHorizontal, LineChart as LineIcon,
  AreaChart, PieChart as PieIcon, Donut, Gauge as GaugeIcon, Table as TableIcon,
  Grid3x3, ScatterChart as ScatterIcon, Activity, Filter, Type, Image as ImageIcon,
  Square, Hash, Layers, Radar as RadarIcon, Funnel, Waypoints, Rows3, SquareStack,
  ChartColumnBig, ChartColumnStacked, Sigma, Minus,
} from 'lucide-react'
import clsx from 'clsx'
import { formatValue, formatAxis, formatLabel, toNum, type NumberFormat } from './format'
import { paletteById, colorScale } from './palettes'
import { shapeSeries, sumMetric, histogram, quartiles, linregress, movingAverage, treemapLayout, type Series } from './shape'

// ── Config accessors ─────────────────────────────────────────────────────────

export interface VConfig { [k: string]: unknown }

const axisOf   = (c: VConfig) => (c.dimensions as string[] | undefined)?.[0] ?? ''
const legendOf = (c: VConfig) => (c.dimensions as string[] | undefined)?.[1] ?? ''
const metricsOf = (c: VConfig) => {
  const m = c.metrics as { column: string; function: string; alias?: string }[] | undefined
  return (m ?? []).map(x => x.alias || x.column).filter(Boolean)
}
const fmtOf = (c: VConfig) => (c.format as NumberFormat | undefined) ?? 'auto'
const palOf = (c: VConfig) => (c.palette as string[] | undefined) ?? paletteById(c.paletteId as string | undefined)
const showLabels = (c: VConfig) => c.dataLabels !== false && !!c.dataLabels
const showLegend = (c: VConfig) => c.legend !== false
const showGrid   = (c: VConfig) => c.gridlines !== false

function col(i: number, pal: string[]) { return pal[i % pal.length] }

// ── Visual catalog (drives the Visualizations pane + add dialog) ─────────────

export type VisualCategory = 'cards' | 'bars' | 'lines' | 'parts' | 'tables' | 'distribution' | 'filters' | 'other'

export interface VisualDef { type: string; label: string; icon: ReactNode; category: VisualCategory }

export const VISUALS: VisualDef[] = [
  // Cards
  { type: 'card',           label: 'Carte',                 icon: <Hash size={18} />,            category: 'cards' },
  { type: 'kpi_card',       label: 'Indicateur (KPI)',      icon: <TrendingUp size={18} />,      category: 'cards' },
  { type: 'multi_row_card', label: 'Carte multi-lignes',    icon: <Rows3 size={18} />,           category: 'cards' },
  { type: 'gauge',          label: 'Jauge',                 icon: <GaugeIcon size={18} />,       category: 'cards' },
  { type: 'bullet',         label: 'Jauge linéaire',        icon: <Minus size={18} />,           category: 'cards' },
  { type: 'kpi_grid',       label: 'Grille de KPI',         icon: <Grid3x3 size={18} />,         category: 'cards' },
  // Bars / columns
  { type: 'bar_chart',      label: 'Histogramme groupé',    icon: <BarChart3 size={18} />,       category: 'bars' },
  { type: 'column_h',       label: 'Barres horizontales',   icon: <BarChartHorizontal size={18} />, category: 'bars' },
  { type: 'stacked_bar',    label: 'Histogramme empilé',    icon: <ChartColumnStacked size={18} />, category: 'bars' },
  { type: 'stacked_bar_100',label: 'Empilé 100 %',          icon: <ChartColumnBig size={18} />,  category: 'bars' },
  { type: 'ribbon_chart',   label: 'Graphique ruban',       icon: <Layers size={18} />,          category: 'bars' },
  // Lines / areas
  { type: 'line_chart',     label: 'Courbe',                icon: <LineIcon size={18} />,        category: 'lines' },
  { type: 'smooth_line',    label: 'Courbe lissée',         icon: <Activity size={18} />,        category: 'lines' },
  { type: 'step_line',      label: 'Courbe en escalier',    icon: <LineIcon size={18} />,        category: 'lines' },
  { type: 'area_chart',     label: 'Aires',                 icon: <AreaChart size={18} />,       category: 'lines' },
  { type: 'stacked_area',   label: 'Aires empilées',        icon: <AreaChart size={18} />,       category: 'lines' },
  { type: 'combo_chart',    label: 'Histogramme + courbe',  icon: <BarChart3 size={18} />,       category: 'lines' },
  { type: 'sparkline',      label: 'Sparkline',             icon: <Activity size={18} />,        category: 'lines' },
  // Part-to-whole
  { type: 'pie_chart',      label: 'Secteurs',              icon: <PieIcon size={18} />,         category: 'parts' },
  { type: 'donut_chart',    label: 'Anneau',                icon: <Donut size={18} />,           category: 'parts' },
  { type: 'funnel',         label: 'Entonnoir',             icon: <Funnel size={18} />,          category: 'parts' },
  { type: 'waterfall',      label: 'Cascade',               icon: <SquareStack size={18} />,     category: 'parts' },
  { type: 'treemap',        label: 'Compartimentage',       icon: <Grid3x3 size={18} />,         category: 'parts' },
  // Tables
  { type: 'data_table',     label: 'Table',                 icon: <TableIcon size={18} />,       category: 'tables' },
  { type: 'matrix',         label: 'Matrice (TCD)',         icon: <Grid3x3 size={18} />,         category: 'tables' },
  // Distribution / correlation
  { type: 'scatter_chart',  label: 'Nuage de points',       icon: <ScatterIcon size={18} />,     category: 'distribution' },
  { type: 'bubble_chart',   label: 'Bulles',                icon: <ScatterIcon size={18} />,     category: 'distribution' },
  { type: 'histogram',      label: 'Distribution',          icon: <BarChart3 size={18} />,       category: 'distribution' },
  { type: 'box_plot',       label: 'Boîte à moustaches',    icon: <Sigma size={18} />,           category: 'distribution' },
  { type: 'radar_chart',    label: 'Radar',                 icon: <RadarIcon size={18} />,       category: 'distribution' },
  { type: 'heatmap',        label: 'Carte de chaleur',      icon: <Grid3x3 size={18} />,         category: 'distribution' },
  // Filters
  { type: 'slicer',         label: 'Segment (liste)',       icon: <Filter size={18} />,          category: 'filters' },
  { type: 'slicer_dropdown',label: 'Segment (menu)',        icon: <Filter size={18} />,          category: 'filters' },
  { type: 'slicer_range',   label: 'Segment numérique',     icon: <Filter size={18} />,          category: 'filters' },
  // Other
  { type: 'text',           label: 'Zone de texte',         icon: <Type size={18} />,            category: 'other' },
  { type: 'image',          label: 'Image',                 icon: <ImageIcon size={18} />,       category: 'other' },
  { type: 'shape',          label: 'Forme',                 icon: <Square size={18} />,          category: 'other' },
  { type: 'progress_ring',  label: 'Anneau de progression', icon: <Waypoints size={18} />,       category: 'other' },
]

export function visualDef(type: string): VisualDef | undefined { return VISUALS.find(v => v.type === type) }

// ── Shared SVG frame ─────────────────────────────────────────────────────────

const W = 400, H = 240
function Frame({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full" preserveAspectRatio="xMidYMid meet">
      {title && <text x={W / 2} y={14} textAnchor="middle" fontSize={11} fontWeight={600} fill="#5f6368">{title}</text>}
      {children}
    </svg>
  )
}

function Empty({ title }: { title?: string }) {
  return (
    <div className="flex flex-col items-center justify-center h-full text-[#9aa0a6] text-xs gap-1 p-2">
      {title && <p className="text-[#5f6368] font-medium">{title}</p>}
      <p>Aucune donnée — choisissez un jeu de données et des champs</p>
    </div>
  )
}

function Legend({ series, pal }: { series: Series[]; pal: string[] }) {
  return (
    <div className="flex flex-wrap gap-x-3 gap-y-1 px-3 pb-2 justify-center shrink-0">
      {series.map((s, i) => (
        <span key={i} className="flex items-center gap-1 text-[10px] text-[#5f6368]">
          <span className="w-2.5 h-2.5 rounded-sm" style={{ background: s.color ?? col(i, pal) }} />
          {formatLabel(s.name, 16)}
        </span>
      ))}
    </div>
  )
}

// ── Cards ─────────────────────────────────────────────────────────────────────

function Card({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const met = metricsOf(c)[0] ?? ''
  const value = sumMetric(data, met)
  return (
    <div className="flex flex-col items-center justify-center h-full p-4 gap-1">
      <p className="font-medium text-[#202124] leading-none" style={{ fontSize: 'clamp(20px, 6vw, 44px)', color: c.valueColor as string | undefined }}>
        {formatValue(value, fmtOf(c), { prefix: c.prefix as string, suffix: c.suffix as string })}
      </p>
      {(c.title as string) && <p className="text-xs text-[#5f6368] font-medium text-center">{c.title as string}</p>}
    </div>
  )
}

function KpiCard({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const met = metricsOf(c)[0] ?? ''
  const value = sumMetric(data, met)
  const target = c.target != null ? toNum(c.target) : (c.compare_metric ? sumMetric(data, c.compare_metric as string) : null)
  const goodUp = c.goodDirection !== 'down'
  const delta = target != null && target !== 0 ? ((value - target) / Math.abs(target)) * 100 : null
  const positive = delta != null && (goodUp ? delta >= 0 : delta < 0)
  const spark = (data.map(r => toNum(r[met])))
  return (
    <div className="flex flex-col justify-center h-full p-4 gap-1 relative overflow-hidden">
      {(c.title as string) && <p className="text-xs text-[#5f6368] font-medium truncate">{c.title as string}</p>}
      <p className="text-3xl font-medium text-[#202124] leading-tight">{formatValue(value, fmtOf(c), { prefix: c.prefix as string, suffix: c.suffix as string })}</p>
      {delta !== null && (
        <div className={clsx('flex items-center gap-1 text-sm font-medium', positive ? 'text-[#1e8e3e]' : 'text-[#d93025]')}>
          {delta >= 0 ? <TrendingUp size={15} /> : <TrendingDown size={15} />}
          <span>{delta >= 0 ? '+' : ''}{delta.toFixed(1)} %</span>
          {(c.compare_label as string) && <span className="text-[#9aa0a6] font-normal">{c.compare_label as string}</span>}
        </div>
      )}
      {!!c.show_sparkline && spark.length > 1 && <Sparkline c={{ ...c, _values: spark }} data={data} mini />}
    </div>
  )
}

function MultiRowCard({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  return (
    <div className="flex flex-col h-full overflow-auto p-2 gap-1.5">
      {(c.title as string) && <p className="text-xs font-medium text-[#5f6368] px-1">{c.title as string}</p>}
      {data.slice(0, 60).map((r, i) => (
        <div key={i} className="flex items-baseline justify-between px-2 py-1.5 rounded bg-[#f8f9fa]">
          <span className="text-xs text-[#5f6368] truncate">{formatLabel(r[dim], 24)}</span>
          <span className="text-sm font-semibold text-[#202124]">{formatValue(r[met], fmtOf(c))}</span>
        </div>
      ))}
    </div>
  )
}

function Gauge({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const met = metricsOf(c)[0] ?? ''
  const value = sumMetric(data, met)
  const min = toNum(c.gaugeMin ?? 0), max = toNum(c.gaugeMax ?? (value > 0 ? value * 1.5 : 100))
  const frac = max > min ? Math.max(0, Math.min(1, (value - min) / (max - min))) : 0
  const pal = palOf(c)
  const cx = 100, cy = 95, r = 70
  const a0 = Math.PI, a1 = Math.PI * (1 - frac)
  const polar = (ang: number) => [cx + r * Math.cos(ang), cy - r * Math.sin(ang)]
  const [sx, sy] = polar(a0), [ex, ey] = polar(a1)
  return (
    <Frame title={c.title as string}>
      <path d={`M${polar(Math.PI)[0]},${polar(Math.PI)[1]} A${r},${r} 0 0,1 ${polar(0)[0]},${polar(0)[1]}`} fill="none" stroke="#e8eaed" strokeWidth={16} strokeLinecap="round" />
      <path d={`M${sx},${sy} A${r},${r} 0 ${frac > 0.5 ? 1 : 0},1 ${ex},${ey}`} fill="none" stroke={col(0, pal)} strokeWidth={16} strokeLinecap="round" />
      <text x={cx} y={cy - 6} textAnchor="middle" fontSize={26} fontWeight={600} fill="#202124">{formatValue(value, fmtOf(c))}</text>
      <text x={cx} y={cy + 14} textAnchor="middle" fontSize={9} fill="#9aa0a6">{formatAxis(min)} – {formatAxis(max)}</text>
    </Frame>
  )
}

function Bullet({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const met = metricsOf(c)[0] ?? ''
  const value = sumMetric(data, met)
  const max = toNum(c.gaugeMax ?? (value > 0 ? value * 1.4 : 100))
  const target = c.target != null ? toNum(c.target) : max * 0.8
  const pal = palOf(c)
  const frac = max > 0 ? Math.min(1, value / max) : 0
  const tfrac = max > 0 ? Math.min(1, target / max) : 0
  return (
    <div className="flex flex-col justify-center h-full p-4 gap-2">
      {(c.title as string) && <p className="text-xs text-[#5f6368] font-medium">{c.title as string}</p>}
      <p className="text-2xl font-medium text-[#202124]">{formatValue(value, fmtOf(c))}</p>
      <div className="relative h-4 rounded bg-[#f1f3f4] overflow-hidden">
        <div className="absolute inset-y-0 left-0 rounded" style={{ width: `${frac * 100}%`, background: col(0, pal) }} />
        <div className="absolute inset-y-0 w-0.5 bg-[#202124]" style={{ left: `${tfrac * 100}%` }} title="Objectif" />
      </div>
      <p className="text-[10px] text-[#9aa0a6]">Objectif : {formatValue(target, fmtOf(c))}</p>
    </div>
  )
}

function KpiGrid({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  return (
    <div className="h-full overflow-auto p-2">
      <div className="grid gap-2" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(96px, 1fr))' }}>
        {data.slice(0, 40).map((r, i) => (
          <div key={i} className="rounded-lg border border-[#e8eaed] p-2 bg-white">
            <p className="text-[10px] text-[#9aa0a6] truncate">{formatLabel(r[dim], 18)}</p>
            <p className="text-base font-semibold text-[#202124]">{formatValue(r[met], fmtOf(c))}</p>
          </div>
        ))}
      </div>
    </div>
  )
}

// ── Column / bar family ────────────────────────────────────────────────────────

type BarMode = 'grouped' | 'stacked' | 'stacked100' | 'horizontal'

function ColumnFamily({ c, data, mode }: { c: VConfig; data: Record<string, unknown>[]; mode: BarMode }) {
  const dim = axisOf(c), legend = legendOf(c), metrics = metricsOf(c)
  const shaped = shapeSeries(data, dim, metrics, legend || undefined)
  if (!shaped.categories.length || !shaped.series.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const { categories, series } = shaped
  const PL = 44, PR = 12, PT = 22, PB = 34
  const cW = W - PL - PR, cH = H - PT - PB

  // Totals per category (for stacked / 100%).
  const totals = categories.map((_, ci) => series.reduce((a, s) => a + Math.max(0, s.values[ci]), 0))
  const stacked = mode === 'stacked' || mode === 'stacked100'
  const maxVal = stacked
    ? (mode === 'stacked100' ? 1 : Math.max(...totals, 1))
    : Math.max(...series.flatMap(s => s.values), 1)

  const horizontal = mode === 'horizontal'
  const slot = (horizontal ? cH : cW) / categories.length
  const groupCount = stacked ? 1 : series.length
  const band = Math.min(horizontal ? 26 : 42, (slot - 6) / groupCount)

  const labels = (
    <>
      {showGrid(c) && [0, 0.25, 0.5, 0.75, 1].map(t => {
        if (horizontal) {
          const x = PL + cW * t
          return <g key={t}><line x1={x} y1={PT} x2={x} y2={PT + cH} stroke="#eef0f2" strokeWidth={0.5} /><text x={x} y={PT + cH + 12} textAnchor="middle" fontSize={8} fill="#9aa0a6">{formatAxis(maxVal * t)}</text></g>
        }
        const y = PT + cH * (1 - t)
        return <g key={t}><line x1={PL} y1={y} x2={PL + cW} y2={y} stroke="#eef0f2" strokeWidth={0.5} /><text x={PL - 4} y={y + 3} textAnchor="end" fontSize={8} fill="#9aa0a6">{mode === 'stacked100' ? `${Math.round(t * 100)}%` : formatAxis(maxVal * t)}</text></g>
      })}
    </>
  )

  return (
    <div className="flex flex-col h-full">
      <Frame title={c.title as string}>
        {labels}
        {categories.map((cat, ci) => {
          const base = (horizontal ? PT : PL) + ci * slot + (slot - band * groupCount) / 2
          let acc = 0
          return (
            <g key={ci}>
              {series.map((s, si) => {
                const raw = Math.max(0, s.values[ci])
                const v = mode === 'stacked100' && totals[ci] > 0 ? raw / totals[ci] : raw
                const len = (v / maxVal) * (horizontal ? cW : cH)
                let rect
                if (horizontal) {
                  const y = base + (stacked ? 0 : si * band)
                  const x = stacked ? PL + (acc / maxVal) * cW : PL
                  rect = <rect key={si} x={x} y={y} width={Math.max(0, len)} height={band - 2} rx={2} fill={s.color ?? col(si, pal)} />
                } else {
                  const x = base + (stacked ? 0 : si * band)
                  const y = stacked ? PT + cH - len - (acc / maxVal) * cH : PT + cH - len
                  rect = <rect key={si} x={x} y={y} width={band - 2} height={Math.max(0, len)} rx={2} fill={s.color ?? col(si, pal)} />
                }
                acc += v
                return rect
              })}
              {!horizontal && (
                <text x={PL + ci * slot + slot / 2} y={PT + cH + 12} textAnchor="middle" fontSize={8} fill="#5f6368">{formatLabel(cat, 8)}</text>
              )}
              {horizontal && (
                <text x={PL - 4} y={base + band * groupCount / 2 + 3} textAnchor="end" fontSize={8} fill="#5f6368">{formatLabel(cat, 9)}</text>
              )}
            </g>
          )
        })}
      </Frame>
      {showLegend(c) && series.length > 1 && <Legend series={series} pal={pal} />}
    </div>
  )
}

// Ribbon chart: stacked columns whose ranks connect with ribbons (approx).
function RibbonChart({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  return <ColumnFamily c={{ ...c, gridlines: false }} data={data} mode="stacked" />
}

// ── Line / area family ──────────────────────────────────────────────────────────

type LineMode = 'line' | 'smooth' | 'step' | 'area' | 'stacked_area'

function LineFamily({ c, data, mode }: { c: VConfig; data: Record<string, unknown>[]; mode: LineMode }) {
  const dim = axisOf(c), legend = legendOf(c), metrics = metricsOf(c)
  const shaped = shapeSeries(data, dim, metrics, legend || undefined)
  if (!shaped.categories.length || !shaped.series.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  let { series } = shaped
  const { categories } = shaped
  const PL = 44, PR = 12, PT = 22, PB = 34
  const cW = W - PL - PR, cH = H - PT - PB
  const n = categories.length

  const stackedArea = mode === 'stacked_area'
  // Compute cumulative for stacked area.
  let cumulative: number[][] = []
  if (stackedArea) {
    const acc = new Array(n).fill(0)
    cumulative = series.map(s => s.values.map((v, i) => (acc[i] += Math.max(0, v))))
  }
  const allVals = stackedArea ? cumulative.flat() : series.flatMap(s => s.values)
  const min = Math.min(0, ...allVals), max = Math.max(...allVals, min + 1)
  const xAt = (i: number) => PL + (n === 1 ? cW / 2 : (i / (n - 1)) * cW)
  const yAt = (v: number) => PT + cH - ((v - min) / (max - min)) * cH

  const pathFor = (vals: number[]) => {
    if (mode === 'step') return vals.map((v, i) => i === 0 ? `M${xAt(i)},${yAt(v)}` : `H${xAt(i)} V${yAt(v)}`).join(' ').replace('H', 'L').replace(/H/g, 'L')
    if (mode === 'smooth') {
      const pts = vals.map((v, i) => [xAt(i), yAt(v)] as [number, number])
      return smoothPath(pts)
    }
    return vals.map((v, i) => `${i === 0 ? 'M' : 'L'}${xAt(i)},${yAt(v)}`).join(' ')
  }

  return (
    <div className="flex flex-col h-full">
      <Frame title={c.title as string}>
        {showGrid(c) && [0, 0.25, 0.5, 0.75, 1].map(t => {
          const y = PT + cH * (1 - t)
          return <g key={t}><line x1={PL} y1={y} x2={PL + cW} y2={y} stroke="#eef0f2" strokeWidth={0.5} /><text x={PL - 4} y={y + 3} textAnchor="end" fontSize={8} fill="#9aa0a6">{formatAxis(min + (max - min) * t)}</text></g>
        })}
        {(stackedArea ? cumulative : series.map(s => s.values)).map((vals, si) => {
          const cc = series[si].color ?? col(si, pal)
          const line = pathFor(vals)
          const area = `M${xAt(0)},${PT + cH} ${line.replace(/^M/, 'L')} L${xAt(n - 1)},${PT + cH} Z`
          return (
            <g key={si}>
              {(mode === 'area' || stackedArea) && <path d={area} fill={cc} fillOpacity={stackedArea ? 0.55 : 0.18} />}
              <path d={line} fill="none" stroke={cc} strokeWidth={2} strokeLinejoin="round" />
              {n <= 24 && vals.map((v, i) => <circle key={i} cx={xAt(i)} cy={yAt(v)} r={2.5} fill={cc} />)}
            </g>
          )
        })}
        {categories.map((cat, i) => (n <= 14 || i % Math.ceil(n / 14) === 0) && (
          <text key={i} x={xAt(i)} y={PT + cH + 12} textAnchor="middle" fontSize={8} fill="#5f6368">{formatLabel(cat, 7)}</text>
        ))}
      </Frame>
      {showLegend(c) && series.length > 1 && <Legend series={series} pal={pal} />}
    </div>
  )
}

function smoothPath(pts: [number, number][]): string {
  if (pts.length < 2) return pts.length ? `M${pts[0][0]},${pts[0][1]}` : ''
  let d = `M${pts[0][0]},${pts[0][1]}`
  for (let i = 0; i < pts.length - 1; i++) {
    const [x0, y0] = pts[i], [x1, y1] = pts[i + 1]
    const cx = (x0 + x1) / 2
    d += ` C${cx},${y0} ${cx},${y1} ${x1},${y1}`
  }
  return d
}

function ComboChart({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), metrics = metricsOf(c)
  if (metrics.length < 1 || !data.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const categories = data.map(r => String(r[dim] ?? ''))
  const barVals = data.map(r => toNum(r[metrics[0]]))
  const lineVals = data.map(r => toNum(r[metrics[1] ?? metrics[0]]))
  const PL = 44, PR = 40, PT = 22, PB = 34
  const cW = W - PL - PR, cH = H - PT - PB
  const maxBar = Math.max(...barVals, 1), maxLine = Math.max(...lineVals, 1)
  const n = categories.length
  const band = Math.min(34, (cW / n) - 6)
  const xAt = (i: number) => PL + (n === 1 ? cW / 2 : (i / (n - 1)) * cW)
  return (
    <div className="flex flex-col h-full">
      <Frame title={c.title as string}>
        {[0, 0.5, 1].map(t => { const y = PT + cH * (1 - t); return <line key={t} x1={PL} y1={y} x2={PL + cW} y2={y} stroke="#eef0f2" strokeWidth={0.5} /> })}
        {barVals.map((v, i) => { const h = (v / maxBar) * cH; const x = PL + i * (cW / n) + (cW / n - band) / 2; return <rect key={i} x={x} y={PT + cH - h} width={band} height={h} rx={2} fill={col(0, pal)} /> })}
        <path d={lineVals.map((v, i) => `${i === 0 ? 'M' : 'L'}${xAt(i)},${PT + cH - (v / maxLine) * cH}`).join(' ')} fill="none" stroke={col(1, pal)} strokeWidth={2} />
        {lineVals.map((v, i) => <circle key={i} cx={xAt(i)} cy={PT + cH - (v / maxLine) * cH} r={2.5} fill={col(1, pal)} />)}
        {categories.map((cat, i) => (n <= 14) && <text key={i} x={PL + i * (cW / n) + (cW / n) / 2} y={PT + cH + 12} textAnchor="middle" fontSize={8} fill="#5f6368">{formatLabel(cat, 7)}</text>)}
      </Frame>
      <Legend series={[{ name: metrics[0], values: [] }, { name: metrics[1] ?? metrics[0], values: [] }]} pal={pal} />
    </div>
  )
}

function Sparkline({ c, data, mini }: { c: VConfig; data: Record<string, unknown>[]; mini?: boolean }) {
  const met = metricsOf(c)[0] ?? ''
  const vals = (c._values as number[] | undefined) ?? data.map(r => toNum(r[met]))
  if (vals.length < 2) return mini ? null : <Empty title={c.title as string} />
  const pal = palOf(c)
  const w = 120, h = mini ? 28 : 60
  const min = Math.min(...vals), max = Math.max(...vals, min + 1)
  const pts = vals.map((v, i) => [(i / (vals.length - 1)) * w, h - ((v - min) / (max - min)) * h] as [number, number])
  const cc = col(0, pal)
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className={mini ? 'w-full h-7 mt-1' : 'w-full h-full p-2'} preserveAspectRatio="none">
      <path d={`M${pts[0][0]},${h} ${pts.map(p => `L${p[0]},${p[1]}`).join(' ')} L${w},${h} Z`} fill={cc} fillOpacity={0.12} />
      <path d={smoothPath(pts)} fill="none" stroke={cc} strokeWidth={1.5} />
      <circle cx={pts[pts.length - 1][0]} cy={pts[pts.length - 1][1]} r={2} fill={cc} />
    </svg>
  )
}

// ── Part-to-whole ────────────────────────────────────────────────────────────

function Pie({ c, data, donut }: { c: VConfig; data: Record<string, unknown>[]; donut?: boolean }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const values = data.map(r => toNum(r[met]))
  const labels = data.map(r => String(r[dim] ?? ''))
  const total = values.reduce((a, b) => a + b, 0) || 1
  const legend = showLegend(c)
  const CX = legend ? 110 : W / 2, CY = 124, R = 86, RI = donut ? 46 : 0
  let a = -Math.PI / 2
  return (
    <Frame title={c.title as string}>
      {values.map((v, i) => {
        const ang = (v / total) * 2 * Math.PI
        const x1 = CX + R * Math.cos(a), y1 = CY + R * Math.sin(a)
        const mid = a + ang / 2
        a += ang
        const x2 = CX + R * Math.cos(a), y2 = CY + R * Math.sin(a)
        const xi1 = CX + RI * Math.cos(a), yi1 = CY + RI * Math.sin(a)
        const xi2 = CX + RI * Math.cos(a - ang), yi2 = CY + RI * Math.sin(a - ang)
        const large = ang > Math.PI ? 1 : 0
        const d = donut
          ? `M${x1},${y1} A${R},${R} 0 ${large},1 ${x2},${y2} L${xi1},${yi1} A${RI},${RI} 0 ${large},0 ${xi2},${yi2} Z`
          : `M${CX},${CY} L${x1},${y1} A${R},${R} 0 ${large},1 ${x2},${y2} Z`
        const pct = ((v / total) * 100)
        const lr = donut ? (R + RI) / 2 : R * 0.62
        return (
          <g key={i}>
            <path d={d} fill={col(i, pal)} />
            {showLabels(c) && pct > 5 && <text x={CX + lr * Math.cos(mid)} y={CY + lr * Math.sin(mid) + 3} textAnchor="middle" fontSize={9} fontWeight={600} fill="#fff">{pct.toFixed(0)}%</text>}
          </g>
        )
      })}
      {donut && <text x={CX} y={CY + 4} textAnchor="middle" fontSize={13} fontWeight={700} fill="#202124">{formatValue(total, fmtOf(c))}</text>}
      {legend && labels.map((l, i) => (
        <g key={i} transform={`translate(${W - 86}, ${36 + i * 16})`}>
          <rect width={9} height={9} rx={2} fill={col(i, pal)} />
          <text x={13} y={8} fontSize={9} fill="#5f6368">{formatLabel(l, 11)} {((values[i] / total) * 100).toFixed(0)}%</text>
        </g>
      ))}
    </Frame>
  )
}

function FunnelChart({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const rows = [...data].sort((a, b) => toNum(b[met]) - toNum(a[met]))
  const max = toNum(rows[0]?.[met]) || 1
  return (
    <div className="flex flex-col h-full justify-center p-3 gap-1.5">
      {(c.title as string) && <p className="text-xs font-medium text-[#5f6368] mb-1">{c.title as string}</p>}
      {rows.slice(0, 12).map((r, i) => {
        const v = toNum(r[met]); const pct = (v / max) * 100
        return (
          <div key={i} className="flex items-center gap-2">
            <span className="text-[10px] text-[#5f6368] w-20 truncate text-right shrink-0">{formatLabel(r[dim], 14)}</span>
            <div className="flex-1 flex justify-center">
              <div className="h-5 rounded flex items-center justify-center text-[10px] font-medium text-white transition-all" style={{ width: `${Math.max(8, pct)}%`, background: col(i, pal) }}>
                {formatValue(v, fmtOf(c))}
              </div>
            </div>
            <span className="text-[10px] text-[#9aa0a6] w-9 shrink-0">{pct.toFixed(0)}%</span>
          </div>
        )
      })}
    </div>
  )
}

function Waterfall({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const steps = data.map(r => ({ label: String(r[dim] ?? ''), v: toNum(r[met]) }))
  let run = 0
  const bars = steps.map(s => { const start = run; run += s.v; return { ...s, start, end: run } })
  bars.push({ label: 'Total', v: run, start: 0, end: run })
  const min = Math.min(0, ...bars.map(b => Math.min(b.start, b.end)))
  const max = Math.max(...bars.map(b => Math.max(b.start, b.end)), 1)
  const PL = 44, PR = 12, PT = 22, PB = 34
  const cW = W - PL - PR, cH = H - PT - PB
  const slot = cW / bars.length, band = Math.min(34, slot - 6)
  const yAt = (v: number) => PT + cH - ((v - min) / (max - min)) * cH
  return (
    <Frame title={c.title as string}>
      {bars.map((b, i) => {
        const isTotal = i === bars.length - 1
        const y = yAt(Math.max(b.start, b.end)), hgt = Math.abs(yAt(b.start) - yAt(b.end))
        const fill = isTotal ? col(2, pal) : b.v >= 0 ? '#34a853' : '#ea4335'
        return (
          <g key={i}>
            <rect x={PL + i * slot + (slot - band) / 2} y={y} width={band} height={Math.max(1, hgt)} rx={2} fill={fill} />
            <text x={PL + i * slot + slot / 2} y={PT + cH + 12} textAnchor="middle" fontSize={7.5} fill="#5f6368">{formatLabel(b.label, 8)}</text>
          </g>
        )
      })}
    </Frame>
  )
}

function Treemap({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const items = data.map(r => ({ label: String(r[dim] ?? ''), value: Math.max(0, toNum(r[met])) }))
  const rects = treemapLayout(items, W, H - 18)
  return (
    <Frame title={c.title as string}>
      <g transform="translate(0, 18)">
        {rects.map((rc, i) => (
          <g key={i}>
            <rect x={rc.x + 1} y={rc.y + 1} width={Math.max(0, rc.w - 2)} height={Math.max(0, rc.h - 2)} rx={2} fill={col(i, pal)} />
            {rc.w > 44 && rc.h > 24 && (
              <text x={rc.x + 6} y={rc.y + 16} fontSize={9} fontWeight={600} fill="#fff">{formatLabel(items[i].label, Math.floor(rc.w / 7))}</text>
            )}
            {rc.w > 44 && rc.h > 36 && (
              <text x={rc.x + 6} y={rc.y + 28} fontSize={8} fill="#ffffffcc">{formatValue(items[i].value, fmtOf(c))}</text>
            )}
          </g>
        ))}
      </g>
    </Frame>
  )
}

// ── Distribution / correlation ───────────────────────────────────────────────

function Scatter({ c, data, bubble }: { c: VConfig; data: Record<string, unknown>[]; bubble?: boolean }) {
  const dims = (c.dimensions as string[] | undefined) ?? []
  const mets = (c.metrics as { column: string; alias?: string }[] | undefined)?.map(m => m.alias || m.column) ?? []
  const xCol = dims[0] || mets[0] || ''
  const yCol = mets[0] && mets[0] !== xCol ? mets[0] : (mets[1] || dims[1] || '')
  const sizeCol = bubble ? (mets[1] || mets[0]) : ''
  if (!data.length || !xCol || !yCol) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const xs = data.map(r => toNum(r[xCol])), ys = data.map(r => toNum(r[yCol]))
  const minX = Math.min(...xs), maxX = Math.max(...xs, minX + 1)
  const minY = Math.min(...ys), maxY = Math.max(...ys, minY + 1)
  const sizes = sizeCol ? data.map(r => toNum(r[sizeCol])) : []
  const maxS = sizes.length ? Math.max(...sizes, 1) : 1
  const PL = 44, PR = 12, PT = 22, PB = 30
  const cW = W - PL - PR, cH = H - PT - PB
  const xAt = (v: number) => PL + ((v - minX) / (maxX - minX)) * cW
  const yAt = (v: number) => PT + cH - ((v - minY) / (maxY - minY)) * cH
  const reg = c.trendline ? linregress(xs, ys) : null
  return (
    <Frame title={c.title as string}>
      {[0, 0.5, 1].map(t => { const y = PT + cH * t; return <line key={t} x1={PL} y1={y} x2={PL + cW} y2={y} stroke="#eef0f2" strokeWidth={0.5} /> })}
      <line x1={PL} y1={PT} x2={PL} y2={PT + cH} stroke="#dadce0" />
      <line x1={PL} y1={PT + cH} x2={PL + cW} y2={PT + cH} stroke="#dadce0" />
      {data.map((r, i) => (
        <circle key={i} cx={xAt(toNum(r[xCol]))} cy={yAt(toNum(r[yCol]))}
          r={bubble ? 3 + (sizes[i] / maxS) * 16 : 4} fill={col(0, pal)} fillOpacity={0.6} stroke={col(0, pal)} strokeOpacity={0.9} />
      ))}
      {reg && <line x1={xAt(minX)} y1={yAt(reg.slope * minX + reg.intercept)} x2={xAt(maxX)} y2={yAt(reg.slope * maxX + reg.intercept)} stroke="#ea4335" strokeWidth={1.5} strokeDasharray="4 3" />}
      <text x={PL + cW / 2} y={H - 4} textAnchor="middle" fontSize={8} fill="#9aa0a6">{xCol}</text>
    </Frame>
  )
}

function Histogram({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const met = metricsOf(c)[0] || axisOf(c)
  const vals = data.map(r => toNum(r[met])).filter(v => Number.isFinite(v))
  if (!vals.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const bins = histogram(vals, toNum(c.bins ?? 12))
  const max = Math.max(...bins.map(b => b.count), 1)
  const PL = 36, PR = 12, PT = 22, PB = 28
  const cW = W - PL - PR, cH = H - PT - PB
  const band = cW / bins.length
  return (
    <Frame title={c.title as string}>
      {bins.map((b, i) => { const h = (b.count / max) * cH; return (
        <g key={i}>
          <rect x={PL + i * band + 1} y={PT + cH - h} width={band - 2} height={h} fill={col(0, pal)} rx={1} />
          {i % Math.ceil(bins.length / 8) === 0 && <text x={PL + i * band + band / 2} y={PT + cH + 12} textAnchor="middle" fontSize={7} fill="#9aa0a6">{b.label}</text>}
        </g>
      )})}
    </Frame>
  )
}

function BoxPlot({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  // Group values by dimension.
  const groups = new Map<string, number[]>()
  for (const r of data) { const k = String(r[dim] ?? '∅'); if (!groups.has(k)) groups.set(k, []); groups.get(k)!.push(toNum(r[met])) }
  const entries = [...groups.entries()].slice(0, 10)
  const allVals = data.map(r => toNum(r[met]))
  const min = Math.min(...allVals), max = Math.max(...allVals, min + 1)
  const PL = 44, PR = 12, PT = 22, PB = 34
  const cW = W - PL - PR, cH = H - PT - PB
  const slot = cW / Math.max(1, entries.length)
  const yAt = (v: number) => PT + cH - ((v - min) / (max - min)) * cH
  return (
    <Frame title={c.title as string}>
      {entries.map(([label, vals], i) => {
        const q = quartiles(vals); const cx = PL + i * slot + slot / 2; const bw = Math.min(28, slot - 10)
        return (
          <g key={i}>
            <line x1={cx} y1={yAt(q.max)} x2={cx} y2={yAt(q.min)} stroke="#9aa0a6" strokeWidth={1} />
            <rect x={cx - bw / 2} y={yAt(q.q3)} width={bw} height={Math.max(1, yAt(q.q1) - yAt(q.q3))} fill={col(i, pal)} fillOpacity={0.5} stroke={col(i, pal)} />
            <line x1={cx - bw / 2} y1={yAt(q.med)} x2={cx + bw / 2} y2={yAt(q.med)} stroke="#202124" strokeWidth={1.5} />
            <text x={cx} y={PT + cH + 12} textAnchor="middle" fontSize={8} fill="#5f6368">{formatLabel(label, 8)}</text>
          </g>
        )
      })}
    </Frame>
  )
}

function Radar({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), metrics = metricsOf(c)
  if (!data.length || !metrics.length) return <Empty title={c.title as string} />
  const pal = palOf(c)
  const axes = data.map(r => String(r[dim] ?? '')).slice(0, 12)
  const n = axes.length
  if (n < 3) return <Empty title={c.title as string} />
  const CX = W / 2, CY = 128, R = 86
  const maxV = Math.max(...metrics.flatMap(m => data.map(r => toNum(r[m]))), 1)
  const pt = (i: number, frac: number) => { const a = -Math.PI / 2 + (i / n) * 2 * Math.PI; return [CX + R * frac * Math.cos(a), CY + R * frac * Math.sin(a)] }
  return (
    <Frame title={c.title as string}>
      {[0.25, 0.5, 0.75, 1].map(t => <polygon key={t} points={axes.map((_, i) => pt(i, t).join(',')).join(' ')} fill="none" stroke="#eef0f2" strokeWidth={0.5} />)}
      {axes.map((_, i) => { const [x, y] = pt(i, 1); return <line key={i} x1={CX} y1={CY} x2={x} y2={y} stroke="#eef0f2" strokeWidth={0.5} /> })}
      {metrics.slice(0, 4).map((m, mi) => {
        const poly = data.slice(0, n).map((r, i) => pt(i, toNum(r[m]) / maxV).join(',')).join(' ')
        return <polygon key={mi} points={poly} fill={col(mi, pal)} fillOpacity={0.18} stroke={col(mi, pal)} strokeWidth={1.5} />
      })}
      {axes.map((a, i) => { const [x, y] = pt(i, 1.12); return <text key={i} x={x} y={y} textAnchor="middle" fontSize={7.5} fill="#5f6368">{formatLabel(a, 7)}</text> })}
    </Frame>
  )
}

function Heatmap({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), legend = legendOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length || !legend) return <Empty title={c.title as string} />
  const rowsK: string[] = [], colsK: string[] = []
  const map = new Map<string, number>()
  for (const r of data) {
    const rk = String(r[dim] ?? ''), ck = String(r[legend] ?? '')
    if (!rowsK.includes(rk)) rowsK.push(rk)
    if (!colsK.includes(ck)) colsK.push(ck)
    map.set(`${rk}|${ck}`, toNum(r[met]))
  }
  const vals = [...map.values()]
  const min = Math.min(...vals), max = Math.max(...vals, min + 1)
  return (
    <div className="h-full overflow-auto p-2">
      {(c.title as string) && <p className="text-xs font-medium text-[#5f6368] mb-1">{c.title as string}</p>}
      <table className="border-collapse text-[10px]">
        <thead><tr><th></th>{colsK.map(ck => <th key={ck} className="px-1 py-0.5 text-[#5f6368] font-medium">{formatLabel(ck, 8)}</th>)}</tr></thead>
        <tbody>
          {rowsK.map(rk => (
            <tr key={rk}>
              <td className="pr-2 text-[#5f6368] whitespace-nowrap">{formatLabel(rk, 12)}</td>
              {colsK.map(ck => { const v = map.get(`${rk}|${ck}`); const t = v == null ? 0 : (v - min) / (max - min); return (
                <td key={ck} className="text-center px-1.5 py-1 text-[#202124]" style={{ background: v == null ? '#fff' : colorScale(t, '#e8f0fe', '#aecbfa', '#1a73e8'), color: t > 0.6 ? '#fff' : '#202124' }} title={v == null ? '' : formatValue(v, fmtOf(c))}>
                  {v == null ? '' : formatValue(v, 'compact')}
                </td>
              )})}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

// ── Tables ───────────────────────────────────────────────────────────────────

function DataTable({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const cols = data.length ? Object.keys(data[0]) : []
  const cfFmt = c.conditionalFormat as { column: string } | undefined
  const cfCol = cfFmt?.column
  const numVals = cfCol ? data.map(r => toNum(r[cfCol])) : []
  const cfMin = cfCol ? Math.min(...numVals) : 0, cfMax = cfCol ? Math.max(...numVals, cfMin + 1) : 1
  const stripe = c.banded !== false
  return (
    <div className="flex flex-col h-full overflow-hidden">
      {(c.title as string) && <p className="text-xs font-medium text-[#5f6368] px-3 pt-2 pb-1 shrink-0">{c.title as string}</p>}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-[#f1f3f4] z-10">
            <tr>{cols.map(co => <th key={co} className="text-left px-3 py-1.5 text-[#5f6368] font-semibold border-b border-[#e8eaed] whitespace-nowrap">{co}</th>)}</tr>
          </thead>
          <tbody>
            {data.map((row, i) => (
              <tr key={i} className={stripe && i % 2 ? 'bg-[#f8f9fa]' : 'bg-white'}>
                {cols.map(co => {
                  const isCf = co === cfCol
                  const t = isCf ? (toNum(row[co]) - cfMin) / (cfMax - cfMin) : 0
                  const num = typeof row[co] === 'number'
                  return (
                    <td key={co} className={clsx('px-3 py-1.5 border-b border-[#f1f3f4] max-w-[220px] truncate', num && 'text-right tabular-nums')}
                      style={isCf ? { background: colorScale(t), fontWeight: 500 } : undefined}>
                      {num ? formatValue(row[co], fmtOf(c)) : String(row[co] ?? '')}
                    </td>
                  )
                })}
              </tr>
            ))}
          </tbody>
        </table>
        {!data.length && <div className="flex items-center justify-center h-20 text-[#9aa0a6] text-xs">Aucune donnée</div>}
      </div>
    </div>
  )
}

function Matrix({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const dim = axisOf(c), legend = legendOf(c), met = metricsOf(c)[0] ?? ''
  if (!data.length || !legend) return <DataTable c={c} data={data} />
  const rowsK: string[] = [], colsK: string[] = []
  const map = new Map<string, number>()
  for (const r of data) {
    const rk = String(r[dim] ?? ''), ck = String(r[legend] ?? '')
    if (!rowsK.includes(rk)) rowsK.push(rk)
    if (!colsK.includes(ck)) colsK.push(ck)
    map.set(`${rk}|${ck}`, (map.get(`${rk}|${ck}`) ?? 0) + toNum(r[met]))
  }
  const rowTotal = (rk: string) => colsK.reduce((a, ck) => a + (map.get(`${rk}|${ck}`) ?? 0), 0)
  const colTotal = (ck: string) => rowsK.reduce((a, rk) => a + (map.get(`${rk}|${ck}`) ?? 0), 0)
  const grand = rowsK.reduce((a, rk) => a + rowTotal(rk), 0)
  return (
    <div className="flex flex-col h-full overflow-hidden">
      {(c.title as string) && <p className="text-xs font-medium text-[#5f6368] px-3 pt-2 pb-1 shrink-0">{c.title as string}</p>}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-[#f1f3f4]">
            <tr>
              <th className="text-left px-3 py-1.5 text-[#5f6368] font-semibold border-b border-[#e8eaed]">{dim}</th>
              {colsK.map(ck => <th key={ck} className="text-right px-3 py-1.5 text-[#5f6368] font-semibold border-b border-[#e8eaed] whitespace-nowrap">{formatLabel(ck, 12)}</th>)}
              {c.showTotals !== false && <th className="text-right px-3 py-1.5 text-[#202124] font-bold border-b border-[#e8eaed]">Total</th>}
            </tr>
          </thead>
          <tbody>
            {rowsK.map((rk, i) => (
              <tr key={rk} className={i % 2 ? 'bg-[#f8f9fa]' : 'bg-white'}>
                <td className="px-3 py-1.5 border-b border-[#f1f3f4] font-medium text-[#202124]">{formatLabel(rk, 22)}</td>
                {colsK.map(ck => <td key={ck} className="px-3 py-1.5 border-b border-[#f1f3f4] text-right tabular-nums text-[#202124]">{formatValue(map.get(`${rk}|${ck}`) ?? 0, fmtOf(c))}</td>)}
                {c.showTotals !== false && <td className="px-3 py-1.5 border-b border-[#f1f3f4] text-right tabular-nums font-semibold">{formatValue(rowTotal(rk), fmtOf(c))}</td>}
              </tr>
            ))}
            {c.showTotals !== false && (
              <tr className="bg-[#e8f0fe] font-bold">
                <td className="px-3 py-1.5 text-[#202124]">Total</td>
                {colsK.map(ck => <td key={ck} className="px-3 py-1.5 text-right tabular-nums">{formatValue(colTotal(ck), fmtOf(c))}</td>)}
                <td className="px-3 py-1.5 text-right tabular-nums">{formatValue(grand, fmtOf(c))}</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}

// ── Other ────────────────────────────────────────────────────────────────────

function ProgressRing({ c, data }: { c: VConfig; data: Record<string, unknown>[] }) {
  const met = metricsOf(c)[0] ?? ''
  const value = sumMetric(data, met)
  const max = toNum(c.gaugeMax ?? (value > 0 ? value * 1.25 : 100))
  const frac = max > 0 ? Math.min(1, value / max) : 0
  const pal = palOf(c)
  const R = 64, C = 2 * Math.PI * R
  return (
    <Frame title={c.title as string}>
      <g transform={`translate(${W / 2}, 130)`}>
        <circle r={R} fill="none" stroke="#e8eaed" strokeWidth={14} />
        <circle r={R} fill="none" stroke={col(0, pal)} strokeWidth={14} strokeLinecap="round" strokeDasharray={`${C * frac} ${C}`} transform="rotate(-90)" />
        <text y={2} textAnchor="middle" fontSize={24} fontWeight={700} fill="#202124">{Math.round(frac * 100)}%</text>
        <text y={22} textAnchor="middle" fontSize={9} fill="#9aa0a6">{formatValue(value, fmtOf(c))}</text>
      </g>
    </Frame>
  )
}

function TextVisual({ c }: { c: VConfig }) {
  const content = String(c.content ?? 'Double-cliquez pour modifier le texte')
  return (
    <div className="h-full w-full overflow-auto p-3 flex"
      style={{ alignItems: (c.vAlign as string) === 'center' ? 'center' : (c.vAlign as string) === 'bottom' ? 'flex-end' : 'flex-start',
               justifyContent: (c.align as string) === 'center' ? 'center' : (c.align as string) === 'right' ? 'flex-end' : 'flex-start' }}>
      <p style={{ fontSize: toNum(c.fontSize ?? 14), fontWeight: (c.bold ? 700 : 400), color: (c.textColor as string) ?? '#202124',
                  textAlign: (c.align as 'left' | 'center' | 'right') ?? 'left', whiteSpace: 'pre-wrap', lineHeight: 1.4 }}>
        {content}
      </p>
    </div>
  )
}

function ImageVisual({ c }: { c: VConfig }) {
  const src = c.imageUrl as string | undefined
  if (!src) return <div className="h-full flex flex-col items-center justify-center text-[#9aa0a6] text-xs gap-1"><ImageIcon size={28} className="opacity-40" /><span>Définissez une URL d'image</span></div>
  return <div className="h-full w-full overflow-hidden flex items-center justify-center bg-[#f8f9fa]"><img src={src} alt="" className="max-w-full max-h-full" style={{ objectFit: (c.fit as 'contain' | 'cover') ?? 'contain' }} /></div>
}

function ShapeVisual({ c }: { c: VConfig }) {
  const kind = (c.shapeKind as string) ?? 'rectangle'
  const fill = (c.fillColor as string) ?? '#e8f0fe'
  const stroke = (c.strokeColor as string) ?? '#1a73e8'
  return (
    <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="w-full h-full p-1">
      {kind === 'rectangle' && <rect x={4} y={4} width={92} height={92} rx={toNum(c.radius ?? 6)} fill={fill} stroke={stroke} strokeWidth={1.5} />}
      {kind === 'ellipse' && <ellipse cx={50} cy={50} rx={46} ry={46} fill={fill} stroke={stroke} strokeWidth={1.5} />}
      {kind === 'line' && <line x1={4} y1={50} x2={96} y2={50} stroke={stroke} strokeWidth={2} />}
      {kind === 'triangle' && <polygon points="50,6 94,94 6,94" fill={fill} stroke={stroke} strokeWidth={1.5} />}
    </svg>
  )
}

function SlicerVisual({ c, data, mode }: { c: VConfig; data: Record<string, unknown>[]; mode: 'list' | 'dropdown' | 'range' }) {
  const dim = axisOf(c)
  const values = [...new Set(data.map(r => String(r[dim] ?? '')))].filter(Boolean).slice(0, 200)
  const selected = (c._selected as string[] | undefined) ?? []
  if (mode === 'range') {
    return (
      <div className="h-full p-3 flex flex-col gap-2">
        <p className="text-xs font-medium text-[#5f6368]">{(c.title as string) || dim || 'Segment'}</p>
        <input type="range" className="w-full accent-[#1a73e8]" readOnly />
        <div className="flex justify-between text-[10px] text-[#9aa0a6]"><span>min</span><span>max</span></div>
      </div>
    )
  }
  if (mode === 'dropdown') {
    return (
      <div className="h-full p-3 flex flex-col gap-2">
        <p className="text-xs font-medium text-[#5f6368]">{(c.title as string) || dim || 'Segment'}</p>
        <select className="border border-[#dadce0] rounded px-2 py-1.5 text-xs text-[#202124]"><option>Tous</option>{values.map(v => <option key={v}>{v}</option>)}</select>
      </div>
    )
  }
  return (
    <div className="h-full flex flex-col overflow-hidden">
      <p className="text-xs font-medium text-[#5f6368] px-3 pt-2 pb-1 shrink-0">{(c.title as string) || dim || 'Segment'}</p>
      <div className="flex-1 overflow-auto px-2 pb-2">
        {values.map(v => (
          <label key={v} className="flex items-center gap-2 px-2 py-1 rounded hover:bg-[#f1f3f4] cursor-pointer text-xs text-[#202124]">
            <input type="checkbox" readOnly checked={selected.includes(v)} className="accent-[#1a73e8]" />
            <span className="truncate">{v}</span>
          </label>
        ))}
        {!values.length && <p className="text-xs text-[#9aa0a6] px-2 py-2">Aucune valeur — choisissez un champ</p>}
      </div>
    </div>
  )
}

// ── Master renderer ──────────────────────────────────────────────────────────

export function renderVisual(type: string, config: VConfig, data: Record<string, unknown>[]): ReactNode {
  switch (type) {
    case 'card':            return <Card c={config} data={data} />
    case 'kpi_card':
    case 'scorecard':       return <KpiCard c={config} data={data} />
    case 'multi_row_card':  return <MultiRowCard c={config} data={data} />
    case 'gauge':           return <Gauge c={config} data={data} />
    case 'bullet':          return <Bullet c={config} data={data} />
    case 'kpi_grid':        return <KpiGrid c={config} data={data} />
    case 'bar_chart':       return <ColumnFamily c={config} data={data} mode="grouped" />
    case 'column_h':        return <ColumnFamily c={config} data={data} mode="horizontal" />
    case 'stacked_bar':     return <ColumnFamily c={config} data={data} mode="stacked" />
    case 'stacked_bar_100': return <ColumnFamily c={config} data={data} mode="stacked100" />
    case 'ribbon_chart':    return <RibbonChart c={config} data={data} />
    case 'line_chart':      return <LineFamily c={config} data={data} mode="line" />
    case 'smooth_line':     return <LineFamily c={config} data={data} mode="smooth" />
    case 'step_line':       return <LineFamily c={config} data={data} mode="step" />
    case 'area_chart':      return <LineFamily c={config} data={data} mode="area" />
    case 'stacked_area':    return <LineFamily c={config} data={data} mode="stacked_area" />
    case 'combo_chart':     return <ComboChart c={config} data={data} />
    case 'sparkline':       return <Sparkline c={config} data={data} />
    case 'pie_chart':       return <Pie c={config} data={data} />
    case 'donut_chart':     return <Pie c={config} data={data} donut />
    case 'funnel':          return <FunnelChart c={config} data={data} />
    case 'waterfall':       return <Waterfall c={config} data={data} />
    case 'treemap':         return <Treemap c={config} data={data} />
    case 'scatter_chart':   return <Scatter c={config} data={data} />
    case 'bubble_chart':    return <Scatter c={config} data={data} bubble />
    case 'histogram':       return <Histogram c={config} data={data} />
    case 'box_plot':        return <BoxPlot c={config} data={data} />
    case 'radar_chart':     return <Radar c={config} data={data} />
    case 'heatmap':         return <Heatmap c={config} data={data} />
    case 'data_table':      return <DataTable c={config} data={data} />
    case 'matrix':          return <Matrix c={config} data={data} />
    case 'progress_ring':   return <ProgressRing c={config} data={data} />
    case 'text':            return <TextVisual c={config} />
    case 'image':           return <ImageVisual c={config} />
    case 'shape':           return <ShapeVisual c={config} />
    case 'slicer':          return <SlicerVisual c={config} data={data} mode="list" />
    case 'slicer_dropdown': return <SlicerVisual c={config} data={data} mode="dropdown" />
    case 'slicer_range':    return <SlicerVisual c={config} data={data} mode="range" />
    default:                return <div className="flex items-center justify-center h-full text-[#9aa0a6] text-xs">{type}</div>
  }
}

/** Visuals that don't need a dataset/query to render. */
export const STATIC_VISUALS = new Set(['text', 'image', 'shape'])
/** Visuals that act as filters (no aggregated query — raw distinct values). */
export const SLICER_VISUALS = new Set(['slicer', 'slicer_dropdown', 'slicer_range', 'filter_date', 'filter_dropdown'])
