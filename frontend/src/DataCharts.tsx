import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { TrendingUp, TrendingDown } from 'lucide-react'
import clsx from 'clsx'

// ── Palette de couleurs par défaut ───────────────────────────────────────────
const PALETTE = ['#1a73e8', '#ea4335', '#fbbc04', '#34a853', '#ff6d00', '#a142f4', '#0097a7', '#e91e63']

function color(i: number, palette?: string[]) {
  const p = palette ?? PALETTE
  return p[i % p.length]
}

// ── Utilitaires ───────────────────────────────────────────────────────────────

function toNum(v: unknown): number {
  if (typeof v === 'number') return v
  if (typeof v === 'string') return parseFloat(v) || 0
  return 0
}

function formatNum(n: number, fmt?: string): string {
  if (!fmt || fmt === 'number') return n.toLocaleString('fr-FR', { maximumFractionDigits: 2 })
  if (fmt === 'percent' || fmt === '0.0%') return `${(n * 100).toFixed(1)}%`
  if (fmt === 'currency' || fmt.includes('€')) return `${n.toLocaleString('fr-FR', { maximumFractionDigits: 0 })}€`
  return n.toLocaleString('fr-FR', { maximumFractionDigits: 2 })
}

// ── KPI Card ──────────────────────────────────────────────────────────────────

interface KpiProps {
  title?: string
  value: number
  prevValue?: number
  format?: string
  palette?: string[]
}

export function KpiCard({ title, value, prevValue, format }: KpiProps) {
  const delta = prevValue != null && prevValue !== 0
    ? ((value - prevValue) / Math.abs(prevValue)) * 100
    : null

  return (
    <div className="flex flex-col justify-center h-full p-4 gap-1">
      {title && <p className="text-xs text-[#5f6368] font-medium truncate">{title}</p>}
      <p className="text-3xl font-medium text-[#202124] leading-tight">
        {formatNum(value, format)}
      </p>
      {delta !== null && (
        <div className={clsx('flex items-center gap-1 text-sm font-medium',
          delta >= 0 ? 'text-[#1e8e3e]' : 'text-[#d93025]')}>
          {delta >= 0 ? <TrendingUp size={15} /> : <TrendingDown size={15} />}
          <span>{delta >= 0 ? '+' : ''}{delta.toFixed(1)}%</span>
        </div>
      )}
    </div>
  )
}

// ── Bar Chart ─────────────────────────────────────────────────────────────────

interface ChartProps {
  data: Record<string, unknown>[]
  dimension: string
  metric: string
  title?: string
  palette?: string[]
  horizontal?: boolean
}

export function BarChart({ data, dimension, metric, title, palette, horizontal }: ChartProps) {
  const values = data.map(r => toNum(r[metric]))
  const labels = data.map(r => String(r[dimension] ?? ''))
  const max = Math.max(...values, 1)

  const W = 400, H = 220, PL = 40, PR = 10, PT = 30, PB = 40
  const chartW = W - PL - PR
  const chartH = H - PT - PB
  const barW = Math.max(6, Math.min(40, (chartW / Math.max(data.length, 1)) - 4))

  if (horizontal) {
    return (
      <div className="flex flex-col h-full p-3">
        {title && <p className="text-xs font-medium text-[#5f6368] mb-2">{title}</p>}
        <div className="flex-1 overflow-hidden">
          {data.map((row, i) => {
            const v = toNum(row[metric])
            const pct = (v / max) * 100
            return (
              <div key={i} className="flex items-center gap-2 mb-1.5">
                <span className="text-xs text-[#5f6368] w-20 truncate shrink-0 text-right">{String(row[dimension] ?? '')}</span>
                <div className="flex-1 h-5 bg-[#f1f3f4] rounded overflow-hidden">
                  <div className="h-full rounded transition-all" style={{ width: `${pct}%`, background: color(i, palette) }} />
                </div>
                <span className="text-xs text-[#5f6368] w-12 text-right shrink-0">{formatNum(v)}</span>
              </div>
            )
          })}
        </div>
      </div>
    )
  }

  if (data.length === 0) return <EmptyChart title={title} />

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={15} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      {/* Y axis lines */}
      {[0, 0.25, 0.5, 0.75, 1].map(t => {
        const y = PT + chartH * (1 - t)
        return (
          <g key={t}>
            <line x1={PL} y1={y} x2={PL + chartW} y2={y} stroke="#e8eaed" strokeWidth={0.5} />
            <text x={PL - 4} y={y + 3} textAnchor="end" fontSize={8} fill="#9aa0a6">
              {formatNum(max * t)}
            </text>
          </g>
        )
      })}
      {/* Bars */}
      {data.map((row, i) => {
        const v = toNum(row[metric])
        const barH = (v / max) * chartH
        const x = PL + i * (chartW / data.length) + (chartW / data.length - barW) / 2
        const y = PT + chartH - barH
        const lbl = labels[i]
        return (
          <g key={i}>
            <rect x={x} y={y} width={barW} height={barH} rx={2} fill={color(i, palette)} />
            <text x={x + barW / 2} y={H - PB + 12} textAnchor="middle" fontSize={8} fill="#5f6368">
              {lbl.length > 8 ? lbl.slice(0, 8) + '…' : lbl}
            </text>
          </g>
        )
      })}
    </svg>
  )
}

// ── Line Chart ────────────────────────────────────────────────────────────────

export function LineChart({ data, dimension, metric, title, palette }: ChartProps) {
  if (data.length === 0) return <EmptyChart title={title} />

  const values = data.map(r => toNum(r[metric]))
  const labels = data.map(r => String(r[dimension] ?? ''))
  const min = Math.min(...values)
  const max = Math.max(...values, min + 1)

  const W = 400, H = 220, PL = 44, PR = 10, PT = 30, PB = 40
  const chartW = W - PL - PR
  const chartH = H - PT - PB

  const pts = values.map((v, i) => {
    const x = PL + (i / (values.length - 1 || 1)) * chartW
    const y = PT + chartH - ((v - min) / (max - min)) * chartH
    return [x, y] as [number, number]
  })

  const linePath = pts.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x},${y}`).join(' ')
  const areaPath = pts.length > 0
    ? `M${pts[0][0]},${PT + chartH} ${linePath} L${pts[pts.length - 1][0]},${PT + chartH} Z`
    : ''

  const baseColor = color(0, palette)

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={15} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      {[0, 0.25, 0.5, 0.75, 1].map(t => {
        const y = PT + chartH * (1 - t)
        const val = min + (max - min) * t
        return (
          <g key={t}>
            <line x1={PL} y1={y} x2={PL + chartW} y2={y} stroke="#e8eaed" strokeWidth={0.5} />
            <text x={PL - 4} y={y + 3} textAnchor="end" fontSize={8} fill="#9aa0a6">{formatNum(val)}</text>
          </g>
        )
      })}
      <defs>
        <linearGradient id="areaGrad" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={baseColor} stopOpacity={0.2} />
          <stop offset="100%" stopColor={baseColor} stopOpacity={0.02} />
        </linearGradient>
      </defs>
      <path d={areaPath} fill="url(#areaGrad)" />
      <path d={linePath} fill="none" stroke={baseColor} strokeWidth={2} />
      {pts.map(([x, y], i) => (
        <g key={i}>
          <circle cx={x} cy={y} r={3} fill={baseColor} />
          {data.length <= 12 && (
            <text x={x} y={H - PB + 12} textAnchor="middle" fontSize={8} fill="#5f6368">
              {labels[i].length > 6 ? labels[i].slice(0, 6) + '…' : labels[i]}
            </text>
          )}
        </g>
      ))}
    </svg>
  )
}

// ── Pie / Donut Chart ─────────────────────────────────────────────────────────

export function PieChart({ data, dimension, metric, title, palette, donut, legend = true, dataLabels }: ChartProps & { donut?: boolean; legend?: boolean; dataLabels?: 'value' | 'percent' }) {
  if (data.length === 0) return <EmptyChart title={title} />

  const values = data.map(r => toNum(r[metric]))
  const total = values.reduce((a, b) => a + b, 0) || 1
  const labels = data.map(r => String(r[dimension] ?? ''))
  // Values that are all fractions (0..1) read as percentages (e.g. 0.5 → "50%").
  const asPercent = values.length > 0 && values.every(v => v >= 0 && v <= 1)

  const W = 280, H = 200, CX = legend ? 100 : W / 2, CY = 100, R = legend ? 80 : 88, RInner = donut ? (legend ? 45 : 50) : 0

  let startAngle = -Math.PI / 2
  const slices = values.map((v, i) => {
    const angle = (v / total) * 2 * Math.PI
    const mid = startAngle + angle / 2
    const x1 = CX + R * Math.cos(startAngle)
    const y1 = CY + R * Math.sin(startAngle)
    startAngle += angle
    const x2 = CX + R * Math.cos(startAngle)
    const y2 = CY + R * Math.sin(startAngle)
    const xi1 = CX + RInner * Math.cos(startAngle - angle)
    const yi1 = CY + RInner * Math.sin(startAngle - angle)
    const xi2 = CX + RInner * Math.cos(startAngle)
    const yi2 = CY + RInner * Math.sin(startAngle)
    const large = angle > Math.PI ? 1 : 0
    const pct = ((v / total) * 100).toFixed(1)
    return { x1, y1, x2, y2, xi1, yi1, xi2, yi2, large, mid, i, label: labels[i], pct }
  })

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={12} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      {slices.map(s => {
        const d = donut
          ? `M${s.x1},${s.y1} A${R},${R} 0 ${s.large},1 ${s.x2},${s.y2} L${s.xi2},${s.yi2} A${RInner},${RInner} 0 ${s.large},0 ${s.xi1},${s.yi1} Z`
          : `M${CX},${CY} L${s.x1},${s.y1} A${R},${R} 0 ${s.large},1 ${s.x2},${s.y2} Z`
        return <path key={s.i} d={d} fill={color(s.i, palette)} />
      })}
      {donut && (
        <text x={CX} y={CY + 4} textAnchor="middle" fontSize={11} fontWeight="bold" fill="#202124">
          {formatNum(total)}
        </text>
      )}
      {/* Data labels on each slice */}
      {dataLabels && slices.map(s => {
        const lr = donut ? (R + RInner) / 2 : R * 0.6
        const lx = CX + lr * Math.cos(s.mid), ly = CY + lr * Math.sin(s.mid)
        const txt = dataLabels === 'percent' ? `${s.pct}%`
          : asPercent ? `${Math.round(values[s.i] * 100)}%` : formatNum(values[s.i])
        return <text key={s.i} x={lx} y={ly + 3} textAnchor="middle" fontSize={9} fontWeight="bold" fill="#ffffff" stroke="#00000022" strokeWidth={0.3}>{txt}</text>
      })}
      {/* Legend */}
      {legend && slices.map((s, i) => (
        <g key={i} transform={`translate(210, ${20 + i * 18})`}>
          <rect width={10} height={10} rx={2} fill={color(s.i, palette)} />
          <text x={14} y={9} fontSize={9} fill="#5f6368">
            {s.label.length > 12 ? s.label.slice(0, 12) + '…' : s.label} {s.pct}%
          </text>
        </g>
      ))}
    </svg>
  )
}

// ── Data Table ────────────────────────────────────────────────────────────────

interface TableProps {
  data: Record<string, unknown>[]
  columns?: string[]
  title?: string
}

export function DataTableWidget({ data, columns: cols, title }: TableProps) {
  const { t } = useTranslation('office')
  const columns = useMemo(() => {
    if (cols && cols.length > 0) return cols
    if (data.length > 0) return Object.keys(data[0])
    return []
  }, [data, cols])

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {title && <p className="text-xs font-medium text-[#5f6368] px-3 pt-2 pb-1 shrink-0">{title}</p>}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-[#f8f9fa]">
            <tr>
              {columns.map(c => (
                <th key={c} className="text-left px-3 py-1.5 text-[#5f6368] font-medium border-b border-[#e8eaed] whitespace-nowrap">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.map((row, i) => (
              <tr key={i} className={i % 2 === 0 ? 'bg-white' : 'bg-[#f8f9fa]'}>
                {columns.map(c => (
                  <td key={c} className="px-3 py-1.5 text-[#202124] border-b border-[#f1f3f4] max-w-[200px] truncate">
                    {String(row[c] ?? '')}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {data.length === 0 && (
          <div className="flex items-center justify-center h-20 text-[#9aa0a6] text-xs">
            {t('datachart_no_data')}
          </div>
        )}
      </div>
    </div>
  )
}

// ── Scatter Chart ─────────────────────────────────────────────────────────────

interface ScatterProps {
  data: Record<string, unknown>[]
  xColumn: string
  yColumn: string
  title?: string
  palette?: string[]
}

export function ScatterChart({ data, xColumn, yColumn, title, palette }: ScatterProps) {
  if (data.length === 0) return <EmptyChart title={title} />

  const xs = data.map(r => toNum(r[xColumn]))
  const ys = data.map(r => toNum(r[yColumn]))
  const minX = Math.min(...xs), maxX = Math.max(...xs, minX + 1)
  const minY = Math.min(...ys), maxY = Math.max(...ys, minY + 1)

  const W = 400, H = 220, PL = 44, PR = 10, PT = 20, PB = 30
  const cW = W - PL - PR, cH = H - PT - PB

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={12} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      <line x1={PL} y1={PT} x2={PL} y2={PT + cH} stroke="#dadce0" />
      <line x1={PL} y1={PT + cH} x2={PL + cW} y2={PT + cH} stroke="#dadce0" />
      {data.map((row, i) => {
        const x = PL + ((toNum(row[xColumn]) - minX) / (maxX - minX)) * cW
        const y = PT + cH - ((toNum(row[yColumn]) - minY) / (maxY - minY)) * cH
        return <circle key={i} cx={x} cy={y} r={4} fill={color(0, palette)} fillOpacity={0.7} />
      })}
    </svg>
  )
}

// ── Empty state ───────────────────────────────────────────────────────────────

function EmptyChart({ title }: { title?: string }) {
  const { t } = useTranslation('office')
  return (
    <div className="flex flex-col items-center justify-center h-full text-[#9aa0a6] text-xs gap-1">
      {title && <p className="text-[#5f6368] text-xs font-medium">{title}</p>}
      <p>{t('datachart_no_data')}</p>
    </div>
  )
}

// ── Widget Renderer ───────────────────────────────────────────────────────────

interface WidgetRendererProps {
  widgetType: string
  config: Record<string, unknown>
  data: Record<string, unknown>[]
  isLoading?: boolean
  error?: string
}

export function WidgetRenderer({ widgetType, config, data, isLoading, error }: WidgetRendererProps) {
  const { t } = useTranslation('office')
  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-[#9aa0a6] text-xs">
        <div className="animate-spin w-4 h-4 border-2 border-[#1a73e8] border-t-transparent rounded-full mr-2" />
        {t('common_loading')}
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full text-[#d93025] text-xs px-3 text-center">
        {error}
      </div>
    )
  }

  const dim = (config.dimensions as string[] | undefined)?.[0] ?? ''
  const met = (config.metrics as { column: string; function: string }[] | undefined)?.[0]?.column ?? ''
  const title = config.title as string | undefined
  const palette = config.palette as string[] | undefined

  switch (widgetType) {
    case 'kpi_card': {
      const total = data.reduce((sum, r) => sum + toNum(r[met]), 0)
      return <KpiCard title={title} value={total} format={config.format as string} palette={palette} />
    }
    case 'bar_chart':
      return <BarChart data={data} dimension={dim} metric={met} title={title} palette={palette} />
    case 'line_chart':
    case 'area_chart':
      return <LineChart data={data} dimension={dim} metric={met} title={title} palette={palette} />
    case 'pie_chart':
      return <PieChart data={data} dimension={dim} metric={met} title={title} palette={palette} />
    case 'donut_chart':
      return <PieChart data={data} dimension={dim} metric={met} title={title} palette={palette} donut />
    case 'data_table':
      return <DataTableWidget data={data} title={title} />
    case 'scatter_chart': {
      const x = (config.dimensions as string[] | undefined)?.[0] ?? ''
      const y = (config.dimensions as string[] | undefined)?.[1] ?? met
      return <ScatterChart data={data} xColumn={x} yColumn={y} title={title} palette={palette} />
    }
    case 'text':
      return (
        <div className="flex items-center justify-center h-full p-4 text-sm text-[#202124]">
          {String(config.content ?? t('datachart_text_placeholder'))}
        </div>
      )
    default:
      return (
        <div className="flex items-center justify-center h-full text-[#9aa0a6] text-xs">
          {widgetType}
        </div>
      )
  }
}
