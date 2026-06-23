// WidgetRenderer — thin wrapper around the visual engine (data/visuals.tsx) that
// adds loading / error states. The full catalog of ~35 BI visuals lives in
// data/visuals.tsx; this file stays for import stability across the editor.
import { useTranslation } from 'react-i18next'
import { renderVisual } from './data/visuals'
import { toNum, formatValue } from './data/format'

// ── Legacy standalone charts (consumed by the spreadsheet's in-cell charts) ───
// Kept with their original prop shapes for backward compatibility.

const PALETTE = ['#1a73e8', '#ea4335', '#fbbc04', '#34a853', '#ff6d00', '#a142f4', '#0097a7', '#e91e63']
const lcolor = (i: number, p?: string[]) => (p ?? PALETTE)[i % (p ?? PALETTE).length]
const fmt = (n: number, f?: string) => formatValue(n, (f as never) ?? 'auto')

interface LegacyChartProps { data: Record<string, unknown>[]; dimension: string; metric: string; title?: string; palette?: string[]; horizontal?: boolean }

function LegacyEmpty({ title }: { title?: string }) {
  return <div className="flex flex-col items-center justify-center h-full text-[#9aa0a6] text-xs gap-1">{title && <p className="text-[#5f6368] font-medium">{title}</p>}<p>Aucune donnée</p></div>
}

export function BarChart({ data, dimension, metric, title, palette, horizontal }: LegacyChartProps) {
  if (!data.length) return <LegacyEmpty title={title} />
  const values = data.map(r => toNum(r[metric]))
  const max = Math.max(...values, 1)
  if (horizontal) {
    return (
      <div className="flex flex-col h-full p-3">
        {title && <p className="text-xs font-medium text-[#5f6368] mb-2">{title}</p>}
        <div className="flex-1 overflow-hidden">
          {data.map((row, i) => { const v = toNum(row[metric]); return (
            <div key={i} className="flex items-center gap-2 mb-1.5">
              <span className="text-xs text-[#5f6368] w-20 truncate shrink-0 text-right">{String(row[dimension] ?? '')}</span>
              <div className="flex-1 h-5 bg-[#f1f3f4] rounded overflow-hidden"><div className="h-full rounded" style={{ width: `${(v / max) * 100}%`, background: lcolor(i, palette) }} /></div>
              <span className="text-xs text-[#5f6368] w-12 text-right shrink-0">{fmt(v)}</span>
            </div>
          )})}
        </div>
      </div>
    )
  }
  const W = 400, H = 220, PL = 40, PR = 10, PT = 30, PB = 40
  const chartW = W - PL - PR, chartH = H - PT - PB
  const barW = Math.max(6, Math.min(40, (chartW / Math.max(data.length, 1)) - 4))
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={15} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      {[0, 0.25, 0.5, 0.75, 1].map(t => { const y = PT + chartH * (1 - t); return (
        <g key={t}><line x1={PL} y1={y} x2={PL + chartW} y2={y} stroke="#e8eaed" strokeWidth={0.5} /><text x={PL - 4} y={y + 3} textAnchor="end" fontSize={8} fill="#9aa0a6">{fmt(max * t)}</text></g>
      )})}
      {data.map((row, i) => { const v = toNum(row[metric]); const barH = (v / max) * chartH; const x = PL + i * (chartW / data.length) + (chartW / data.length - barW) / 2; const lbl = String(data[i][dimension] ?? ''); return (
        <g key={i}><rect x={x} y={PT + chartH - barH} width={barW} height={barH} rx={2} fill={lcolor(i, palette)} /><text x={x + barW / 2} y={H - PB + 12} textAnchor="middle" fontSize={8} fill="#5f6368">{lbl.length > 8 ? lbl.slice(0, 8) + '…' : lbl}</text></g>
      )})}
    </svg>
  )
}

export function LineChart({ data, dimension, metric, title, palette }: LegacyChartProps) {
  if (!data.length) return <LegacyEmpty title={title} />
  const values = data.map(r => toNum(r[metric]))
  const labels = data.map(r => String(r[dimension] ?? ''))
  const min = Math.min(...values), max = Math.max(...values, min + 1)
  const W = 400, H = 220, PL = 44, PR = 10, PT = 30, PB = 40
  const chartW = W - PL - PR, chartH = H - PT - PB
  const pts = values.map((v, i) => [PL + (i / (values.length - 1 || 1)) * chartW, PT + chartH - ((v - min) / (max - min)) * chartH] as [number, number])
  const linePath = pts.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x},${y}`).join(' ')
  const base = lcolor(0, palette)
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={15} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      {[0, 0.25, 0.5, 0.75, 1].map(t => { const y = PT + chartH * (1 - t); return (
        <g key={t}><line x1={PL} y1={y} x2={PL + chartW} y2={y} stroke="#e8eaed" strokeWidth={0.5} /><text x={PL - 4} y={y + 3} textAnchor="end" fontSize={8} fill="#9aa0a6">{fmt(min + (max - min) * t)}</text></g>
      )})}
      <path d={pts.length ? `M${pts[0][0]},${PT + chartH} ${linePath} L${pts[pts.length - 1][0]},${PT + chartH} Z` : ''} fill={base} fillOpacity={0.1} />
      <path d={linePath} fill="none" stroke={base} strokeWidth={2} />
      {pts.map(([x, y], i) => (<g key={i}><circle cx={x} cy={y} r={3} fill={base} />{data.length <= 12 && <text x={x} y={H - PB + 12} textAnchor="middle" fontSize={8} fill="#5f6368">{labels[i].length > 6 ? labels[i].slice(0, 6) + '…' : labels[i]}</text>}</g>))}
    </svg>
  )
}

export function PieChart({ data, dimension, metric, title, palette, donut, legend = true }: LegacyChartProps & { donut?: boolean; legend?: boolean; dataLabels?: 'value' | 'percent' }) {
  if (!data.length) return <LegacyEmpty title={title} />
  const values = data.map(r => toNum(r[metric]))
  const total = values.reduce((a, b) => a + b, 0) || 1
  const labels = data.map(r => String(r[dimension] ?? ''))
  const W = 280, H = 200, CX = legend ? 100 : W / 2, CY = 100, R = legend ? 80 : 88, RInner = donut ? (legend ? 45 : 50) : 0
  let a = -Math.PI / 2
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={12} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      {values.map((v, i) => {
        const ang = (v / total) * 2 * Math.PI
        const x1 = CX + R * Math.cos(a), y1 = CY + R * Math.sin(a); a += ang
        const x2 = CX + R * Math.cos(a), y2 = CY + R * Math.sin(a)
        const xi1 = CX + RInner * Math.cos(a), yi1 = CY + RInner * Math.sin(a)
        const xi2 = CX + RInner * Math.cos(a - ang), yi2 = CY + RInner * Math.sin(a - ang)
        const large = ang > Math.PI ? 1 : 0
        const d = donut ? `M${x1},${y1} A${R},${R} 0 ${large},1 ${x2},${y2} L${xi1},${yi1} A${RInner},${RInner} 0 ${large},0 ${xi2},${yi2} Z` : `M${CX},${CY} L${x1},${y1} A${R},${R} 0 ${large},1 ${x2},${y2} Z`
        return <path key={i} d={d} fill={lcolor(i, palette)} />
      })}
      {donut && <text x={CX} y={CY + 4} textAnchor="middle" fontSize={11} fontWeight="bold" fill="#202124">{fmt(total)}</text>}
      {legend && labels.map((l, i) => (<g key={i} transform={`translate(210, ${20 + i * 18})`}><rect width={10} height={10} rx={2} fill={lcolor(i, palette)} /><text x={14} y={9} fontSize={9} fill="#5f6368">{l.length > 12 ? l.slice(0, 12) + '…' : l} {((values[i] / total) * 100).toFixed(1)}%</text></g>))}
    </svg>
  )
}

export function ScatterChart({ data, xColumn, yColumn, title, palette }: { data: Record<string, unknown>[]; xColumn: string; yColumn: string; title?: string; palette?: string[] }) {
  if (!data.length) return <LegacyEmpty title={title} />
  const xs = data.map(r => toNum(r[xColumn])), ys = data.map(r => toNum(r[yColumn]))
  const minX = Math.min(...xs), maxX = Math.max(...xs, minX + 1), minY = Math.min(...ys), maxY = Math.max(...ys, minY + 1)
  const W = 400, H = 220, PL = 44, PR = 10, PT = 20, PB = 30, cW = W - PL - PR, cH = H - PT - PB
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {title && <text x={W / 2} y={12} textAnchor="middle" fontSize={10} fill="#5f6368">{title}</text>}
      <line x1={PL} y1={PT} x2={PL} y2={PT + cH} stroke="#dadce0" />
      <line x1={PL} y1={PT + cH} x2={PL + cW} y2={PT + cH} stroke="#dadce0" />
      {data.map((row, i) => (<circle key={i} cx={PL + ((toNum(row[xColumn]) - minX) / (maxX - minX)) * cW} cy={PT + cH - ((toNum(row[yColumn]) - minY) / (maxY - minY)) * cH} r={4} fill={lcolor(0, palette)} fillOpacity={0.7} />))}
    </svg>
  )
}

export { renderVisual, VISUALS, visualDef, STATIC_VISUALS, SLICER_VISUALS } from './data/visuals'
export type { VisualDef, VisualCategory } from './data/visuals'

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
        {t('common_loading', { defaultValue: 'Chargement…' })}
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
  return <>{renderVisual(widgetType, config, data)}</>
}
