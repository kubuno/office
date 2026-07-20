// A function-graph block: plots one or more y=f(x) curves on a <canvas> with axes, a "nice" grid,
// numeric labels, auto or fixed Y-range, mouse pan (drag) and wheel zoom. Pure client rendering,
// zero dependencies — the expressions are compiled by ./mathExpr. Edited in place via `onChange`.
import { useRef, useEffect, useMemo, useCallback } from 'react'
import type { TFunction } from 'i18next'
import { ColorField } from '@ui'
import { Plus, Trash2, RotateCcw } from 'lucide-react'
import { compile } from './mathExpr'
import { type GraphSpec, GRAPH_COLORS, drawGraph, defaultGraphSpec } from './mathGraph'

export default function GraphBlock({ spec, onChange, t }: { spec: GraphSpec; onChange: (s: GraphSpec) => void; t?: TFunction }) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const tr = (k: string, d: string) => (t ? t(k, { defaultValue: d }) : d)

  const compiled = useMemo(() => spec.fns.map(f => { const c = compile(f.expr); return { fn: c.fn, error: c.error, color: f.color } }), [spec.fns])

  const patch = (p: Partial<GraphSpec>) => onChange({ ...spec, ...p })
  const patchFn = (i: number, pf: Partial<{ expr: string; color: string }>) => onChange({ ...spec, fns: spec.fns.map((f, j) => j === i ? { ...f, ...pf } : f) })
  const addFn = () => onChange({ ...spec, fns: [...spec.fns, { expr: '', color: GRAPH_COLORS[spec.fns.length % GRAPH_COLORS.length] }] })
  const removeFn = (i: number) => onChange({ ...spec, fns: spec.fns.length > 1 ? spec.fns.filter((_, j) => j !== i) : spec.fns })

  const draw = useCallback(() => {
    const cv = canvasRef.current, wrap = wrapRef.current
    if (!cv || !wrap) return
    const cssW = Math.max(80, wrap.clientWidth), cssH = spec.height
    const dpr = window.devicePixelRatio || 1
    cv.width = Math.round(cssW * dpr); cv.height = Math.round(cssH * dpr)
    cv.style.width = cssW + 'px'; cv.style.height = cssH + 'px'
    const ctx = cv.getContext('2d'); if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, cssW, cssH)
    drawGraph(ctx, spec, cssW, cssH, compiled)
  }, [spec, compiled])

  // Redraw on spec change and on container resize.
  useEffect(() => { draw() }, [draw])
  useEffect(() => {
    const wrap = wrapRef.current; if (!wrap || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => draw()); ro.observe(wrap)
    return () => ro.disconnect()
  }, [draw])

  // Wheel zoom (native listener so preventDefault works) — zoom X around the cursor; also Y if fixed.
  useEffect(() => {
    const cv = canvasRef.current; if (!cv) return
    const onWheel = (e: WheelEvent) => {
      // Ctrl/Cmd + wheel belongs to the sheet zoom (Maths editor) — let it bubble.
      if (e.ctrlKey || e.metaKey) return
      e.preventDefault()
      const r = cv.getBoundingClientRect()
      const f = e.deltaY < 0 ? 0.85 : 1 / 0.85
      const cx = spec.xmin + (e.clientX - r.left) / r.width * (spec.xmax - spec.xmin)
      const next: Partial<GraphSpec> = { xmin: cx - (cx - spec.xmin) * f, xmax: cx + (spec.xmax - cx) * f }
      if (spec.ymin != null && spec.ymax != null) {
        const cy = spec.ymax - (e.clientY - r.top) / r.height * (spec.ymax - spec.ymin)
        next.ymin = cy - (cy - spec.ymin) * f; next.ymax = cy + (spec.ymax - cy) * f
      }
      patch(next)
    }
    cv.addEventListener('wheel', onWheel, { passive: false })
    return () => cv.removeEventListener('wheel', onWheel)
  }) // eslint-disable-line react-hooks/exhaustive-deps

  // Drag to pan.
  const drag = useRef<{ x: number; y: number; spec: GraphSpec } | null>(null)
  const onPointerDown = (e: React.PointerEvent) => { (e.target as HTMLElement).setPointerCapture(e.pointerId); drag.current = { x: e.clientX, y: e.clientY, spec } }
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current; if (!d) return
    const cv = canvasRef.current; if (!cv) return
    const r = cv.getBoundingClientRect()
    const dx = (e.clientX - d.x) / r.width * (d.spec.xmax - d.spec.xmin)
    const next: Partial<GraphSpec> = { xmin: d.spec.xmin - dx, xmax: d.spec.xmax - dx }
    if (d.spec.ymin != null && d.spec.ymax != null) {
      const dy = (e.clientY - d.y) / r.height * (d.spec.ymax - d.spec.ymin)
      next.ymin = d.spec.ymin + dy; next.ymax = d.spec.ymax + dy
    }
    patch(next)
  }
  const onPointerUp = () => { drag.current = null }
  const reset = () => { const d = defaultGraphSpec(); patch({ xmin: d.xmin, xmax: d.xmax, ymin: null, ymax: null }) }

  const numInput = (val: number, on: (v: number) => void, w = 'w-16') => (
    <input type="number" value={Number.isFinite(val) ? val : ''} step="any"
      onChange={e => { const v = parseFloat(e.target.value); if (isFinite(v)) on(v) }}
      className={`${w} h-7 px-1.5 text-xs border border-border rounded bg-white text-text-primary`} />
  )
  const yauto = spec.ymin == null || spec.ymax == null

  return (
    <div className="border border-border rounded-lg overflow-hidden bg-white">
      <div ref={wrapRef} className="w-full" style={{ height: spec.height }}>
        <canvas ref={canvasRef} className="block touch-none cursor-grab active:cursor-grabbing"
          onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onDoubleClick={reset} />
      </div>
      {/* Controls */}
      <div className="border-t border-border bg-surface-1 p-2 flex flex-col gap-1.5">
        {spec.fns.map((f, i) => (
          <div key={i} className="flex items-center gap-2">
            <span className="text-xs font-mono text-text-tertiary w-12 flex-shrink-0">{tr('graph_fx', 'f(x) =')}</span>
            <ColorField t={t} color={f.color} onChange={hex => patchFn(i, { color: hex })} width={20} height={18} />
            <input value={f.expr} onChange={e => patchFn(i, { expr: e.target.value })}
              placeholder={tr('graph_expr_ph', 'ex : sin(x) + x^2')}
              className={`flex-1 min-w-0 h-7 px-2 text-sm font-mono border rounded bg-white ${compiled[i]?.error ? 'border-danger text-danger' : 'border-border text-text-primary'}`} />
            {compiled[i]?.error && <span className="text-[10px] text-danger max-w-[140px] truncate" title={compiled[i].error!}>{compiled[i].error}</span>}
            {spec.fns.length > 1 && <button onClick={() => removeFn(i)} title={tr('graph_remove_fn', 'Retirer')} className="text-text-tertiary hover:text-danger flex-shrink-0"><Trash2 size={14} /></button>}
          </div>
        ))}
        <div className="flex items-center flex-wrap gap-x-3 gap-y-1.5 pt-1">
          <button onClick={addFn} className="flex items-center gap-1 h-7 px-2 text-xs rounded border border-border bg-white hover:bg-surface-2 text-text-secondary">
            <Plus size={13} /> {tr('graph_add_fn', 'Fonction')}
          </button>
          <div className="flex items-center gap-1 text-xs text-text-secondary">
            <span className="font-mono">x ∈ [</span>{numInput(spec.xmin, v => patch({ xmin: v }))}<span>,</span>{numInput(spec.xmax, v => patch({ xmax: v }))}<span className="font-mono">]</span>
          </div>
          <label className="flex items-center gap-1 text-xs text-text-secondary cursor-pointer">
            <input type="checkbox" checked={!yauto} onChange={e => patch(e.target.checked ? { ymin: -10, ymax: 10 } : { ymin: null, ymax: null })} />
            {tr('graph_y_fixed', 'Y fixe')}
          </label>
          {!yauto && (
            <div className="flex items-center gap-1 text-xs text-text-secondary">
              <span className="font-mono">y ∈ [</span>{numInput(spec.ymin as number, v => patch({ ymin: v }))}<span>,</span>{numInput(spec.ymax as number, v => patch({ ymax: v }))}<span className="font-mono">]</span>
            </div>
          )}
          <label className="flex items-center gap-1 text-xs text-text-secondary cursor-pointer">
            <input type="checkbox" checked={spec.grid} onChange={e => patch({ grid: e.target.checked })} /> {tr('graph_grid', 'Grille')}
          </label>
          <button onClick={reset} title={tr('graph_reset', 'Réinitialiser la vue')} className="flex items-center gap-1 h-7 px-2 text-xs rounded border border-border bg-white hover:bg-surface-2 text-text-secondary">
            <RotateCcw size={13} /> {tr('graph_reset_short', 'Vue')}
          </button>
        </div>
      </div>
    </div>
  )
}
