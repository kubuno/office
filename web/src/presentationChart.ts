// Chart rendering for presentation chart elements. Pure canvas drawing so it can
// be validated with a headless `data:` screenshot. Coordinates are device pixels.
import type { ChartElement } from './api'

export const CHART_PALETTE = ['#1a73e8', '#ea4335', '#fbbc04', '#34a853', '#9334e8', '#00acc1', '#ff7043', '#5f6368']

const niceMax = (v: number) => {
  if (v <= 0) return 1
  const pow = Math.pow(10, Math.floor(Math.log10(v)))
  const n = v / pow
  const step = n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10
  return step * pow
}

interface Ctx2D {
  fillStyle: string | CanvasGradient | CanvasPattern
  strokeStyle: string | CanvasGradient | CanvasPattern
  lineWidth: number
  font: string
  textAlign: CanvasTextAlign
  textBaseline: CanvasTextBaseline
  globalAlpha: number
  fillRect(x: number, y: number, w: number, h: number): void
  strokeRect(x: number, y: number, w: number, h: number): void
  beginPath(): void
  moveTo(x: number, y: number): void
  lineTo(x: number, y: number): void
  arc(x: number, y: number, r: number, a0: number, a1: number, ccw?: boolean): void
  closePath(): void
  fill(): void
  stroke(): void
  fillText(t: string, x: number, y: number): void
  save(): void
  restore(): void
}

export function renderChart(ctx: Ctx2D, el: ChartElement, x: number, y: number, w: number, h: number, sf = 1) {
  const palette = el.palette?.length ? el.palette : CHART_PALETTE
  const series = el.series?.length ? el.series : [{ name: 'Série 1', values: [3, 5, 2, 6] }]
  const cats = el.categories?.length ? el.categories : series[0].values.map((_, i) => `C${i + 1}`)
  const fs = 12 * sf
  ctx.save()
  ctx.textBaseline = 'alphabetic'
  ctx.font = `${fs}px Arial, sans-serif`

  let top = y + 8 * sf
  // Titre
  if (el.title) {
    ctx.fillStyle = '#202124'; ctx.font = `bold ${14 * sf}px Arial, sans-serif`; ctx.textAlign = 'center'
    ctx.fillText(el.title, x + w / 2, top + 14 * sf)
    top += 24 * sf
  }
  // Légende (en haut)
  let legendH = 0
  if (el.showLegend && (el.chartType === 'pie' || el.chartType === 'donut' ? cats.length : series.length) > 0) {
    ctx.font = `${fs}px Arial, sans-serif`; ctx.textAlign = 'left'
    const labels = (el.chartType === 'pie' || el.chartType === 'donut') ? cats : series.map(s => s.name)
    let lx = x + 10 * sf
    const ly = top + 10 * sf
    for (let i = 0; i < labels.length; i++) {
      ctx.fillStyle = palette[i % palette.length]
      ctx.fillRect(lx, ly - 8 * sf, 10 * sf, 10 * sf)
      ctx.fillStyle = '#5f6368'
      const tw = labels[i].length * fs * 0.55
      ctx.fillText(labels[i], lx + 14 * sf, ly)
      lx += 14 * sf + tw + 14 * sf
    }
    legendH = 22 * sf
    top += legendH
  }

  const plotX = x + 36 * sf
  const plotY = top
  const plotW = w - 46 * sf
  const plotH = y + h - plotY - 22 * sf

  if (el.chartType === 'pie' || el.chartType === 'donut') {
    const cx = x + w / 2, cy = plotY + plotH / 2
    const r = Math.max(4, Math.min(plotW, plotH) / 2 - 4 * sf)
    const total = cats.reduce((a, _, i) => a + Math.abs(series[0]?.values[i] ?? 0), 0) || 1
    let a0 = -Math.PI / 2
    for (let i = 0; i < cats.length; i++) {
      const val = Math.abs(series[0]?.values[i] ?? 0)
      const a1 = a0 + (val / total) * Math.PI * 2
      ctx.beginPath(); ctx.moveTo(cx, cy); ctx.arc(cx, cy, r, a0, a1); ctx.closePath()
      ctx.fillStyle = palette[i % palette.length]; ctx.fill()
      a0 = a1
    }
    if (el.chartType === 'donut') {
      ctx.beginPath(); ctx.arc(cx, cy, r * 0.55, 0, Math.PI * 2); ctx.closePath()
      ctx.fillStyle = '#ffffff'; ctx.fill()
    }
    ctx.restore(); return
  }

  // Axes (column/bar/line/area)
  const allVals = series.flatMap(s => s.values)
  const maxV = niceMax(Math.max(1, ...allVals.map(v => Math.abs(v))))
  ctx.strokeStyle = '#dadce0'; ctx.lineWidth = 1 * sf
  ctx.beginPath(); ctx.moveTo(plotX, plotY); ctx.lineTo(plotX, plotY + plotH); ctx.lineTo(plotX + plotW, plotY + plotH); ctx.stroke()
  // Graduations Y
  ctx.fillStyle = '#9aa0a6'; ctx.font = `${10 * sf}px Arial, sans-serif`; ctx.textAlign = 'right'
  for (let g = 0; g <= 4; g++) {
    const gy = plotY + plotH - (g / 4) * plotH
    ctx.strokeStyle = '#f1f3f4'; ctx.beginPath(); ctx.moveTo(plotX, gy); ctx.lineTo(plotX + plotW, gy); ctx.stroke()
    ctx.fillText(String(Math.round((g / 4) * maxV)), plotX - 4 * sf, gy + 3 * sf)
  }

  const n = cats.length
  if (el.chartType === 'column') {
    const groupW = plotW / n
    const barW = (groupW * 0.7) / series.length
    for (let i = 0; i < n; i++) {
      for (let s = 0; s < series.length; s++) {
        const v = series[s].values[i] ?? 0
        const bh = (Math.abs(v) / maxV) * plotH
        const bx = plotX + i * groupW + groupW * 0.15 + s * barW
        ctx.fillStyle = palette[s % palette.length]
        ctx.fillRect(bx, plotY + plotH - bh, barW * 0.9, bh)
      }
    }
  } else if (el.chartType === 'bar') {
    const groupH = plotH / n
    const barH = (groupH * 0.7) / series.length
    for (let i = 0; i < n; i++) {
      for (let s = 0; s < series.length; s++) {
        const v = series[s].values[i] ?? 0
        const bw = (Math.abs(v) / maxV) * plotW
        const by = plotY + i * groupH + groupH * 0.15 + s * barH
        ctx.fillStyle = palette[s % palette.length]
        ctx.fillRect(plotX, by, bw, barH * 0.9)
      }
    }
  } else { // line / area
    for (let s = 0; s < series.length; s++) {
      const col = palette[s % palette.length]
      const pts = cats.map((_, i) => {
        const v = series[s].values[i] ?? 0
        const px = plotX + (n === 1 ? plotW / 2 : (i / (n - 1)) * plotW)
        const py = plotY + plotH - (Math.abs(v) / maxV) * plotH
        return [px, py] as [number, number]
      })
      if (el.chartType === 'area') {
        ctx.beginPath(); ctx.moveTo(pts[0][0], plotY + plotH)
        pts.forEach(p => ctx.lineTo(p[0], p[1]))
        ctx.lineTo(pts[pts.length - 1][0], plotY + plotH); ctx.closePath()
        ctx.globalAlpha = 0.3; ctx.fillStyle = col; ctx.fill(); ctx.globalAlpha = 1
      }
      ctx.strokeStyle = col; ctx.lineWidth = 2 * sf
      ctx.beginPath(); pts.forEach((p, i) => i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1])); ctx.stroke()
      ctx.fillStyle = col
      for (const p of pts) { ctx.beginPath(); ctx.arc(p[0], p[1], 2.5 * sf, 0, Math.PI * 2); ctx.fill() }
    }
  }
  // Étiquettes X
  ctx.fillStyle = '#9aa0a6'; ctx.font = `${10 * sf}px Arial, sans-serif`; ctx.textAlign = 'center'
  if (el.chartType !== 'bar') {
    for (let i = 0; i < n; i++) {
      const cx2 = plotX + (n === 1 ? plotW / 2 : el.chartType === 'column' ? (i + 0.5) * (plotW / n) : (i / (n - 1)) * plotW)
      ctx.fillText(cats[i], cx2, plotY + plotH + 13 * sf)
    }
  }
  ctx.restore()
}

// Parse a textarea like "Cat,S1,S2\nA,3,5\nB,6,2" into categories + series.
export function parseChartData(text: string): { categories: string[]; series: { name: string; values: number[] }[] } {
  const rows = text.trim().split('\n').map(r => r.split(/[,\t;]/).map(c => c.trim()))
  if (!rows.length) return { categories: [], series: [] }
  const header = rows[0]
  const seriesNames = header.slice(1)
  const categories: string[] = []
  const series = seriesNames.map(name => ({ name, values: [] as number[] }))
  for (let r = 1; r < rows.length; r++) {
    if (!rows[r].length || rows[r].every(c => c === '')) continue
    categories.push(rows[r][0])
    for (let s = 0; s < series.length; s++) {
      const v = parseFloat(rows[r][s + 1])
      series[s].values.push(isNaN(v) ? 0 : v)
    }
  }
  return { categories, series }
}

export function chartDataToText(el: { categories: string[]; series: { name: string; values: number[] }[] }): string {
  const header = ['', ...el.series.map(s => s.name)].join(',')
  const rows = el.categories.map((c, i) => [c, ...el.series.map(s => s.values[i] ?? 0)].join(','))
  return [header, ...rows].join('\n')
}
