// Miniature inline-SVG previews for the chart type/subtype gallery (wizard
// step 1 and the ribbon gallery). Pure presentational, no data involved.

import { CHART2_PALETTE, CHART_NEUTRAL } from '../model'
import type { CatalogTypeId } from './catalog'

const C0 = CHART2_PALETTE[0], C1 = CHART2_PALETTE[1], C2 = CHART2_PALETTE[2]
// Drawing area inside the 64x44 viewBox (a small margin plus baseline axis).
const W = 64, H = 44, X0 = 4, Y0 = 3, YB = 40, XE = 61

function polyline(pts: [number, number][]): string {
  return pts.map(p => `${p[0]},${p[1]}`).join(' ')
}

/** Pie/donut slice path centered at (cx,cy). Angles in radians. */
function slice(cx: number, cy: number, r: number, a0: number, a1: number, hole = 0, dx = 0, dy = 0): string {
  const x0 = cx + dx + r * Math.cos(a0), y0 = cy + dy + r * Math.sin(a0)
  const x1 = cx + dx + r * Math.cos(a1), y1 = cy + dy + r * Math.sin(a1)
  const large = a1 - a0 > Math.PI ? 1 : 0
  if (hole <= 0) return `M${cx + dx},${cy + dy} L${x0},${y0} A${r},${r} 0 ${large} 1 ${x1},${y1} Z`
  const h = r * hole
  const hx0 = cx + dx + h * Math.cos(a0), hy0 = cy + dy + h * Math.sin(a0)
  const hx1 = cx + dx + h * Math.cos(a1), hy1 = cy + dy + h * Math.sin(a1)
  return `M${hx0},${hy0} L${x0},${y0} A${r},${r} 0 ${large} 1 ${x1},${y1} L${hx1},${hy1} A${h},${h} 0 ${large} 0 ${hx0},${hy0} Z`
}

function Columns({ mode }: { mode: 'normal' | 'stacked' | 'percent' }) {
  const groups = [[14, 20, 9], [22, 12, 15], [10, 26, 12]]
  const out: React.ReactNode[] = []
  groups.forEach((g, gi) => {
    if (mode === 'normal') {
      g.forEach((v, si) => out.push(
        <rect key={`${gi}-${si}`} x={7 + gi * 19 + si * 5} y={YB - v} width={4} height={v} fill={[C0, C1, C2][si]} />))
    } else {
      const total = g[0] + g[1] + g[2]
      const scale = mode === 'percent' ? (YB - Y0 - 2) / total : 1
      let y = YB
      g.forEach((v, si) => {
        const h = v * scale
        y -= h
        out.push(<rect key={`${gi}-${si}`} x={10 + gi * 18} y={y} width={9} height={h} fill={[C0, C1, C2][si]} />)
      })
    }
  })
  return <>{out}</>
}

function Bars({ mode }: { mode: 'normal' | 'stacked' | 'percent' }) {
  const groups = [[26, 14, 20], [16, 30, 10], [34, 20, 14]]
  const out: React.ReactNode[] = []
  groups.forEach((g, gi) => {
    if (mode === 'normal') {
      g.forEach((v, si) => out.push(
        <rect key={`${gi}-${si}`} x={X0 + 1} y={5 + gi * 13 + si * 4} width={v} height={3} fill={[C0, C1, C2][si]} />))
    } else {
      const total = g[0] + g[1] + g[2]
      const scale = mode === 'percent' ? 54 / total : 0.8
      let x = X0 + 1
      g.forEach((v, si) => {
        const w = v * scale
        out.push(<rect key={`${gi}-${si}`} x={x} y={7 + gi * 12} width={w} height={8} fill={[C0, C1, C2][si]} />)
        x += w
      })
    }
  })
  return <>{out}</>
}

function Pie({ hole = 0, exploded = false }: { hole?: number; exploded?: boolean }) {
  const cx = 32, cy = 22, r = 17
  const angles = [-Math.PI / 2, 0.4, 1.9, 3.4, Math.PI * 1.5]
  const colors = [C0, C1, C2, CHART2_PALETTE[3]]
  return <>{colors.map((col, i) => {
    const mid = (angles[i] + angles[i + 1]) / 2
    const off = exploded && i === 1 ? 4 : exploded ? 1.5 : 0
    return <path key={i} d={slice(cx, cy, r - (exploded ? 2 : 0), angles[i], angles[i + 1], hole, off * Math.cos(mid), off * Math.sin(mid))} fill={col} />
  })}</>
}

const LINE_PTS: [number, number][][] = [
  [[6, 30], [19, 20], [32, 26], [45, 12], [58, 16]],
  [[6, 36], [19, 33], [32, 28], [45, 31], [58, 24]],
]
function Lines({ lines, symbols }: { lines: boolean; symbols: boolean }) {
  return <>{LINE_PTS.map((pts, si) => <g key={si}>
    {lines && <polyline points={polyline(pts)} fill="none" stroke={[C0, C1][si]} strokeWidth={2} />}
    {symbols && pts.map((p, i) => <rect key={i} x={p[0] - 2} y={p[1] - 2} width={4} height={4} fill={[C0, C1][si]} />)}
  </g>)}</>
}

function Areas({ mode }: { mode: 'normal' | 'stacked' | 'percent' }) {
  if (mode === 'percent') {
    return <>
      <rect x={X0} y={Y0 + 2} width={XE - X0} height={YB - Y0 - 2} fill={C2} />
      <path d={`M${X0},28 L20,20 L38,25 L${XE},16 L${XE},${YB} L${X0},${YB} Z`} fill={C1} />
      <path d={`M${X0},34 L20,30 L38,33 L${XE},28 L${XE},${YB} L${X0},${YB} Z`} fill={C0} />
    </>
  }
  if (mode === 'stacked') {
    return <>
      <path d={`M${X0},18 L20,8 L38,14 L${XE},6 L${XE},${YB} L${X0},${YB} Z`} fill={C2} />
      <path d={`M${X0},27 L20,20 L38,25 L${XE},18 L${XE},${YB} L${X0},${YB} Z`} fill={C1} />
      <path d={`M${X0},34 L20,30 L38,33 L${XE},28 L${XE},${YB} L${X0},${YB} Z`} fill={C0} />
    </>
  }
  return <>
    <path d={`M${X0},20 L22,10 L40,18 L${XE},8 L${XE},${YB} L${X0},${YB} Z`} fill={C1} opacity={0.85} />
    <path d={`M${X0},30 L22,24 L40,28 L${XE},20 L${XE},${YB} L${X0},${YB} Z`} fill={C0} opacity={0.9} />
  </>
}

const SCATTER_PTS: [number, number, number][] = [
  [10, 30, 0], [16, 24, 1], [22, 28, 0], [27, 18, 1], [33, 22, 0],
  [38, 12, 1], [44, 17, 0], [50, 9, 1], [56, 14, 0], [13, 36, 1], [47, 26, 0],
]
function Scatter({ lines }: { lines: boolean; symbols: boolean }) {
  if (lines) return <Lines lines symbols={false} />
  return <>{SCATTER_PTS.map((p, i) => <circle key={i} cx={p[0]} cy={p[1]} r={2} fill={p[2] ? C1 : C0} />)}</>
}

function Radar({ filled }: { filled: boolean }) {
  const cx = 32, cy = 23, spokes = 5
  const pt = (r: number, i: number): [number, number] => {
    const a = -Math.PI / 2 + i * 2 * Math.PI / spokes
    return [cx + r * Math.cos(a), cy + r * Math.sin(a)]
  }
  const web: React.ReactNode[] = []
  for (let ring = 1; ring <= 2; ring++) {
    const r = ring * 9.5
    web.push(<polygon key={`w${ring}`} points={polyline(Array.from({ length: spokes }, (_, i) => pt(r, i)))} fill="none" stroke={CHART_NEUTRAL} strokeWidth={0.8} />)
  }
  for (let i = 0; i < spokes; i++) web.push(<line key={`s${i}`} x1={cx} y1={cy} x2={pt(19, i)[0]} y2={pt(19, i)[1]} stroke={CHART_NEUTRAL} strokeWidth={0.8} />)
  const s1 = [16, 10, 17, 13, 18].map((r, i) => pt(r, i))
  const s2 = [9, 16, 8, 17, 10].map((r, i) => pt(r, i))
  return <>
    {web}
    <polygon points={polyline(s1)} fill={filled ? C0 : 'none'} opacity={filled ? 0.75 : 1} stroke={C0} strokeWidth={1.5} />
    <polygon points={polyline(s2)} fill={filled ? C1 : 'none'} opacity={filled ? 0.75 : 1} stroke={C1} strokeWidth={1.5} />
  </>
}

function Combo({ stacked }: { stacked: boolean }) {
  const groups = [[16, 10], [24, 13], [12, 18], [28, 9]]
  return <>
    {groups.map((g, gi) => stacked
      ? <g key={gi}>
          <rect x={8 + gi * 14} y={YB - g[0] - g[1]} width={8} height={g[1]} fill={C1} />
          <rect x={8 + gi * 14} y={YB - g[0]} width={8} height={g[0]} fill={C0} />
        </g>
      : <g key={gi}>
          <rect x={7 + gi * 14} y={YB - g[0]} width={5} height={g[0]} fill={C0} />
          <rect x={13 + gi * 14} y={YB - g[1]} width={5} height={g[1]} fill={C1} />
        </g>)}
    <polyline points={polyline([[11, 18], [25, 10], [39, 22], [53, 8]])} fill="none" stroke={C2} strokeWidth={2} />
    {[[11, 18], [25, 10], [39, 22], [53, 8]].map((p, i) => <circle key={i} cx={p[0]} cy={p[1]} r={2} fill={C2} />)}
  </>
}

/** 64x44 inline-SVG miniature for a catalog (type, subtype) pair. */
export function ChartThumb({ typeId, subtypeId, width = 64, height = 44 }: {
  typeId: CatalogTypeId; subtypeId: string; width?: number; height?: number
}) {
  const cartesianAxes = typeId !== 'pie' && typeId !== 'radar'
  let body: React.ReactNode
  switch (typeId) {
    case 'column': body = <Columns mode={subtypeId as 'normal' | 'stacked' | 'percent'} />; break
    case 'bar': body = <Bars mode={subtypeId as 'normal' | 'stacked' | 'percent'} />; break
    case 'pie': body = <Pie hole={subtypeId.startsWith('donut') ? 0.55 : 0} exploded={subtypeId.endsWith('exploded')} />; break
    case 'area': body = <Areas mode={subtypeId as 'normal' | 'stacked' | 'percent'} />; break
    case 'line': body = <Lines lines={subtypeId !== 'points'} symbols={subtypeId !== 'lines'} />; break
    case 'scatter': body = <Scatter lines={subtypeId === 'lines'} symbols={subtypeId !== 'lines'} />; break
    case 'radar': body = <Radar filled={subtypeId === 'filled'} />; break
    case 'combo': body = <Combo stacked={subtypeId === 'col_stacked_line'} />; break
  }
  return (
    <svg width={width} height={height} viewBox={`0 0 ${W} ${H}`} aria-hidden="true">
      {cartesianAxes && <>
        <line x1={X0} y1={Y0} x2={X0} y2={YB} stroke={CHART_NEUTRAL} strokeWidth={1} />
        <line x1={X0} y1={YB} x2={XE} y2={YB} stroke={CHART_NEUTRAL} strokeWidth={1} />
      </>}
      {body}
    </svg>
  )
}
