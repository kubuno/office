import { useRef, useState, useImperativeHandle, forwardRef, useEffect, useCallback } from 'react'
import katex from 'katex'
import {
  type MRow, type Cursor, ser, parse, renderTree, slots,
  insertAtom, insertLatex, backspace, deleteFwd, moveH, selectH, moveHome, moveV,
  makeFrac, makeScript, insertStruct, deleteSelection, COMBO,
  enclosingMatrix, matAddRow, matAddCol, matDelRow, matDelCol,
} from './mathTree'

export type MatrixOp = 'addRow' | 'addCol' | 'delRow' | 'delCol'

// Imperative API the palette / toolbar / context menu drive.
export interface MathEditorHandle {
  insertLatex: (tex: string) => void
  struct: (kind: string, opt?: Record<string, unknown>) => void
  frac: () => void
  script: (k: 'sup' | 'sub') => void
  deleteSlot: () => void
  clear: () => void
  focus: () => void
  getLatex: () => string
  inMatrix: () => boolean
  matrixOp: (op: MatrixOp) => void
}

function render(root: MRow, cur: Cursor): { html: string; map: ReturnType<typeof renderTree>['map'] } {
  const { latex, map } = renderTree(root, cur)
  try {
    return { html: katex.renderToString(latex || '\\square', { displayMode: true, throwOnError: false, output: 'html', strict: false, trust: true }), map }
  } catch { return { html: '', map } }
}

const isLetter = (k: string) => /^[a-zA-Z]$/.test(k)

const MathTreeEditor = forwardRef<MathEditorHandle, { value: string; onChange: (latex: string) => void; align?: 'left' | 'center' | 'right'; scale?: number }>(function MathTreeEditor({ value, onChange, align = 'center', scale = 1 }, ref) {
  const rootRef = useRef<MRow>(parse(value))
  const curRef = useRef<Cursor>({ row: rootRef.current, idx: rootRef.current.length, anchor: null })
  const mapRef = useRef<ReturnType<typeof renderTree>['map']>([])
  const cmdRef = useRef<string | null>(null)          // pending `\command` buffer
  const lastEmit = useRef<string>(value)
  const surfRef = useRef<HTMLDivElement>(null)
  const [, setTick] = useState(0)
  const rerender = () => setTick(t => t + 1)

  // Re-parse when the source changes EXTERNALLY (page switch / code-mode edit), not from us.
  useEffect(() => {
    if (value === lastEmit.current) return
    rootRef.current = parse(value)
    curRef.current = { row: rootRef.current, idx: rootRef.current.length, anchor: null }
    lastEmit.current = value
    rerender()
  }, [value])

  const emit = useCallback(() => { const s = ser(rootRef.current); lastEmit.current = s; onChange(s) }, [onChange])
  const commitCmd = () => {
    const c = cmdRef.current; cmdRef.current = null
    if (c) insertAtom(curRef.current, '\\' + c)
  }
  const afterEdit = () => { rerender(); emit() }

  // Jump to the next/previous editable hole (□ placeholder or empty row) — the “Tab” flow.
  const tabTo = (back: boolean) => {
    const sl = slots(rootRef.current), cur = curRef.current
    const k = sl.findIndex(s => s.row === cur.row && s.idx === cur.idx)
    const order = back ? [...sl.keys()].reverse() : [...sl.keys()]
    const start = order.indexOf(k)
    for (let j = 1; j <= order.length; j++) {
      const s = sl[order[(start + j) % order.length]]
      const isHole = s.row.length === 0 || (s.idx < s.row.length && s.row[s.idx]?.k === 'atom' && (s.row[s.idx] as { t: string }).t === '\\square')
      if (isHole) { cur.row = s.row; cur.idx = s.idx; cur.anchor = s.row.length === 0 ? null : s.idx + 1; afterEdit(); return }
    }
    moveH(rootRef.current, cur, back ? -1 : 1); afterEdit()
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    const cur = curRef.current, root = rootRef.current
    if (e.metaKey || e.ctrlKey) {
      const k = e.key.toLowerCase()
      if (k === 'a') { e.preventDefault(); cur.anchor = 0; cur.idx = cur.row.length; rerender(); return }
      // Ctrl+←/→ : jump to the current field's edge, then step out of the enclosing structure.
      if (e.key === 'ArrowRight') { e.preventDefault(); cur.anchor = null; if (cur.idx < cur.row.length) cur.idx = cur.row.length; else moveH(root, cur, 1); afterEdit(); return }
      if (e.key === 'ArrowLeft') { e.preventDefault(); cur.anchor = null; if (cur.idx > 0) cur.idx = 0; else moveH(root, cur, -1); afterEdit(); return }
      return
    }
    const key = e.key
    // ── Pending command buffer (\alpha …) ──
    if (cmdRef.current !== null) {
      if (isLetter(key)) { cmdRef.current += key; e.preventDefault(); rerender(); return }
      if (key === ' ' || key === 'Enter') { e.preventDefault(); commitCmd(); afterEdit(); return }
      commitCmd() // any other key terminates the command, then is handled below
    }
    if (key === 'Tab') { e.preventDefault(); tabTo(e.shiftKey); return }
    if (key === 'ArrowRight') { e.preventDefault(); e.shiftKey ? selectH(cur, 1) : moveH(root, cur, 1); afterEdit(); return }
    if (key === 'ArrowLeft') { e.preventDefault(); e.shiftKey ? selectH(cur, -1) : moveH(root, cur, -1); afterEdit(); return }
    if (key === 'ArrowUp') { e.preventDefault(); if (!moveVGeom(-1)) moveV(root, cur, -1); afterEdit(); return }
    if (key === 'ArrowDown') { e.preventDefault(); if (!moveVGeom(1)) moveV(root, cur, 1); afterEdit(); return }
    if (key === 'Home') { e.preventDefault(); moveHome(cur, false); afterEdit(); return }
    if (key === 'End') { e.preventDefault(); moveHome(cur, true); afterEdit(); return }
    if (key === 'Backspace') { e.preventDefault(); backspace(root, cur); afterEdit(); return }
    if (key === 'Delete') { e.preventDefault(); deleteFwd(root, cur); afterEdit(); return }
    if (key === 'Escape') { e.preventDefault(); cur.anchor = null; rerender(); return }
    if (key === 'Enter') {
      // Inside a matrix/cases environment, Enter appends a row below (spreadsheet-like).
      e.preventDefault()
      const m = enclosingMatrix(root, cur.row)
      if (m) { const dest = matAddRow(m.node, m.ri); cur.row = dest; cur.idx = 0; cur.anchor = null; afterEdit() }
      return
    }
    if (key.length !== 1) return
    e.preventDefault()
    // ── Structure triggers ──
    if (key === '/') { makeFrac(cur); afterEdit(); return }
    if (key === '^') { makeScript(cur, 'sup'); afterEdit(); return }
    if (key === '_') { makeScript(cur, 'sub'); afterEdit(); return }
    if (key === '(') { insertStruct(cur, 'delim', { o: '(', c: ')' }); afterEdit(); return }
    if (key === '[') { insertStruct(cur, 'delim', { o: '[', c: ']' }); afterEdit(); return }
    if (key === '{') { insertStruct(cur, 'delim', { o: '\\{', c: '\\}' }); afterEdit(); return }
    if (key === '|') { insertStruct(cur, 'delim', { o: '|', c: '|' }); afterEdit(); return }
    if (key === '\\') { cmdRef.current = ''; rerender(); return }
    if (key === '*') { insertAtom(cur, '\\cdot'); afterEdit(); return }
    if (key === ')' || key === ']') {
      // step out of the enclosing delimiter if we're at the end of its body, else literal
      if (cur.idx === cur.row.length) { moveH(root, cur, 1); afterEdit(); return }
    }
    // ── Two-key auto-conversion (->, <=, !=, …) ──
    const prev = cur.idx > 0 ? cur.row[cur.idx - 1] : null
    if (prev && prev.k === 'atom' && prev.t.length === 1) {
      const combo = COMBO[prev.t + key]
      if (combo) { cur.row.splice(cur.idx - 1, 1, { k: 'atom', t: combo }); afterEdit(); return }
    }
    insertAtom(cur, key); afterEdit()
  }

  // Geometric hit-testing: find the caret position closest to a screen point. Rather than
  // requiring an exact hit on a marker (which dropped the caret to the end on any near-miss),
  // this picks the smallest marker that CONTAINS the point (handles nested structures) or, if
  // the click is in empty space, the nearest marker — then places the caret before/after the
  // atom depending on which half was clicked. This is what makes clicking feel precise.
  type Hit = { row: MRow; idx: number }
  const hitSlot = (cx: number, cy: number): Hit | null => {
    const surf = surfRef.current
    if (!surf) return null
    const els = surf.querySelectorAll<HTMLElement>('.mc-h')
    if (!els.length) return null
    let inSlot: Hit | null = null, inArea = Infinity, inBefore = false
    let nrSlot: Hit | null = null, nrDist = Infinity, nrBefore = false
    for (const el of els) {
      const m = /mc-h-(\d+)/.exec(el.className)
      if (!m) continue
      const slot = mapRef.current[Number(m[1])]
      if (!slot) continue
      const r = el.getBoundingClientRect()
      if (r.width === 0 && r.height === 0) continue
      const before = cx < (r.left + r.right) / 2
      const inside = cx >= r.left - 0.5 && cx <= r.right + 0.5 && cy >= r.top && cy <= r.bottom
      if (inside) {
        const area = r.width * r.height
        if (area < inArea) { inArea = area; inSlot = { row: slot.row, idx: slot.idx }; inBefore = before }
      } else {
        const dx = cx < r.left ? r.left - cx : cx > r.right ? cx - r.right : 0
        const dy = cy < r.top ? r.top - cy : cy > r.bottom ? cy - r.bottom : 0
        const dist = dx + dy * 4 // strongly prefer the same visual line
        if (dist < nrDist) { nrDist = dist; nrSlot = { row: slot.row, idx: slot.idx }; nrBefore = before }
      }
    }
    const s = inSlot ?? nrSlot
    if (!s) return null
    const before = inSlot ? inBefore : nrBefore
    return { row: s.row, idx: Math.min(s.row.length, Math.max(0, before ? s.idx - 1 : s.idx)) }
  }

  const dragging = useRef(false)
  const dragStart = useRef<Hit | null>(null)
  // Double-click is detected manually (time + distance): the KaTeX HTML is re-rendered on every
  // pointerdown, so the second click lands on a fresh node and the browser never fires `dblclick`.
  const lastDown = useRef({ t: 0, x: 0, y: 0 })
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return
    surfRef.current?.focus()
    const cur = curRef.current
    const hit = hitSlot(e.clientX, e.clientY)
    const now = Date.now()
    const dbl = now - lastDown.current.t < 350 && Math.hypot(e.clientX - lastDown.current.x, e.clientY - lastDown.current.y) < 6
    lastDown.current = { t: now, x: e.clientX, y: e.clientY }
    if (cmdRef.current !== null) commitCmd()
    if (dbl && hit) { // second click → select the atom (or the whole group at a row start)
      if (hit.idx > 0) { cur.row = hit.row; cur.anchor = hit.idx - 1; cur.idx = hit.idx }
      else if (hit.row.length > 0) { cur.row = hit.row; cur.anchor = 0; cur.idx = hit.row.length }
      dragging.current = false
      afterEdit()
      return
    }
    if (!hit) { cur.row = rootRef.current; cur.idx = rootRef.current.length; cur.anchor = null }
    else if (e.shiftKey && cur.row === hit.row) { if (cur.anchor == null) cur.anchor = cur.idx; cur.idx = hit.idx }
    else { cur.row = hit.row; cur.idx = hit.idx; cur.anchor = null }
    dragging.current = true
    dragStart.current = hit
    try { surfRef.current?.setPointerCapture(e.pointerId) } catch { /* not supported */ }
    afterEdit()
  }
  const onPointerMove = (e: React.PointerEvent) => {
    if (!dragging.current || !dragStart.current) return
    const hit = hitSlot(e.clientX, e.clientY)
    if (!hit) return
    const cur = curRef.current, start = dragStart.current
    if (hit.row === start.row) { cur.row = hit.row; cur.anchor = hit.idx === start.idx ? null : start.idx; cur.idx = hit.idx }
    else { cur.row = hit.row; cur.idx = hit.idx; cur.anchor = null }
    rerender() // caret-only change; no need to re-serialise to the parent
  }
  const onPointerUp = (e: React.PointerEvent) => { dragging.current = false; try { surfRef.current?.releasePointerCapture(e.pointerId) } catch { /* ignore */ } }

  // Screen points of every caret position (before/after each atom, or an empty hole). Used for
  // geometry-aware Up/Down so vertical motion enters exponents, fractions and matrix rows the way
  // a human expects — instead of only stepping between an immediate parent's paired fields.
  type Pt = { x: number; y: number; row: MRow; idx: number }
  const slotPoints = (): Pt[] => {
    const surf = surfRef.current
    if (!surf) return []
    const pts: Pt[] = []
    for (const el of surf.querySelectorAll<HTMLElement>('.mc-h')) {
      const m = /mc-h-(\d+)/.exec(el.className)
      if (!m) continue
      const s = mapRef.current[Number(m[1])]
      if (!s) continue
      const r = el.getBoundingClientRect()
      if (r.width === 0 && r.height === 0) continue
      const y = (r.top + r.bottom) / 2
      if (el.className.includes('mc-empty')) pts.push({ x: (r.left + r.right) / 2, y, row: s.row, idx: 0 })
      else { pts.push({ x: r.left, y, row: s.row, idx: Math.max(0, s.idx - 1) }); pts.push({ x: r.right, y, row: s.row, idx: s.idx }) }
    }
    return pts
  }
  const moveVGeom = (dir: 1 | -1): boolean => {
    const pts = slotPoints()
    if (!pts.length) return false
    const cur = curRef.current
    const here = pts.find(p => p.row === cur.row && p.idx === cur.idx)
    if (!here) return false
    const LINE = 6 // px: a point must be on a clearly different line to count as up/down
    let best: Pt | null = null, bestScore = Infinity
    for (const p of pts) {
      const dy = p.y - here.y
      if (dir < 0 ? dy > -LINE : dy < LINE) continue
      const score = Math.abs(p.x - here.x) + Math.abs(dy) * 0.4 // nearest column, then nearest line
      if (score < bestScore) { bestScore = score; best = p }
    }
    if (!best) return false
    cur.row = best.row; cur.idx = Math.min(best.row.length, Math.max(0, best.idx)); cur.anchor = null
    return true
  }

  useImperativeHandle(ref, () => ({
    insertLatex: (tex: string) => { commitCmd(); insertLatex(rootRef.current, curRef.current, tex); afterEdit(); surfRef.current?.focus() },
    struct: (kind, opt) => { commitCmd(); insertStruct(curRef.current, kind, opt as never); afterEdit(); surfRef.current?.focus() },
    frac: () => { commitCmd(); makeFrac(curRef.current); afterEdit(); surfRef.current?.focus() },
    script: (k) => { commitCmd(); makeScript(curRef.current, k); afterEdit(); surfRef.current?.focus() },
    deleteSlot: () => { deleteSelection(curRef.current) || backspace(rootRef.current, curRef.current); afterEdit() },
    clear: () => { rootRef.current = []; curRef.current = { row: rootRef.current, idx: 0, anchor: null }; afterEdit() },
    focus: () => surfRef.current?.focus(),
    getLatex: () => ser(rootRef.current),
    inMatrix: () => !!enclosingMatrix(rootRef.current, curRef.current.row),
    matrixOp: (op) => {
      const cur = curRef.current
      const m = enclosingMatrix(rootRef.current, cur.row)
      if (!m) return
      const dest = op === 'addRow' ? matAddRow(m.node, m.ri)
        : op === 'addCol' ? matAddCol(m.node, m.ci, m.ri)
        : op === 'delRow' ? matDelRow(m.node, m.ri, m.ci)
        : matDelCol(m.node, m.ci, m.ri)
      if (dest) { cur.row = dest; cur.idx = dest.length; cur.anchor = null }
      afterEdit(); surfRef.current?.focus()
    },
  }), [emit]) // eslint-disable-line react-hooks/exhaustive-deps

  const { html, map } = render(rootRef.current, curRef.current)
  mapRef.current = map

  return (
    <div ref={surfRef} tabIndex={0} onKeyDown={onKeyDown} onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp}
      className="flex-1 min-h-0 overflow-hidden bg-white p-8 flex flex-col justify-center outline-none cursor-text select-none"
      style={{ alignItems: align === 'left' ? 'flex-start' : align === 'right' ? 'flex-end' : 'safe center', touchAction: 'none' }}>
      {/* Base size 1.875rem (text-3xl) × the block's content scale (set by corner-resizing). */}
      <div className="mc-render text-[#202124]" style={{ fontSize: `${1.875 * scale}rem` }} dangerouslySetInnerHTML={{ __html: html }} />
      {cmdRef.current !== null && <div className="mt-3 text-xs font-mono text-primary bg-primary/10 px-2 py-1 rounded">\{cmdRef.current}<span className="opacity-50">│</span></div>}
    </div>
  )
})

export default MathTreeEditor
