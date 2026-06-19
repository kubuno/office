import { useRef, useState, useImperativeHandle, forwardRef, useEffect, useCallback } from 'react'
import katex from 'katex'
import {
  type MRow, type Cursor, ser, parse, renderTree, slots,
  insertAtom, insertLatex, backspace, deleteFwd, moveH, selectH, moveHome, moveV,
  makeFrac, makeScript, insertStruct, deleteSelection, COMBO,
} from './mathTree'

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
}

function render(root: MRow, cur: Cursor): { html: string; map: ReturnType<typeof renderTree>['map'] } {
  const { latex, map } = renderTree(root, cur)
  try {
    return { html: katex.renderToString(latex || '\\square', { displayMode: true, throwOnError: false, output: 'html', strict: false, trust: true }), map }
  } catch { return { html: '', map } }
}

const isLetter = (k: string) => /^[a-zA-Z]$/.test(k)

const MathTreeEditor = forwardRef<MathEditorHandle, { value: string; onChange: (latex: string) => void }>(function MathTreeEditor({ value, onChange }, ref) {
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
    if (key === 'ArrowUp') { e.preventDefault(); moveV(root, cur, -1); afterEdit(); return }
    if (key === 'ArrowDown') { e.preventDefault(); moveV(root, cur, 1); afterEdit(); return }
    if (key === 'Home') { e.preventDefault(); moveHome(cur, false); afterEdit(); return }
    if (key === 'End') { e.preventDefault(); moveHome(cur, true); afterEdit(); return }
    if (key === 'Backspace') { e.preventDefault(); backspace(root, cur); afterEdit(); return }
    if (key === 'Delete') { e.preventDefault(); deleteFwd(root, cur); afterEdit(); return }
    if (key === 'Escape') { e.preventDefault(); cur.anchor = null; rerender(); return }
    if (key === 'Enter') { e.preventDefault(); return }
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

  const onClick = (e: React.MouseEvent) => {
    surfRef.current?.focus()
    const el = (e.target as HTMLElement).closest('.mc-h') as HTMLElement | null
    if (!el) { const cur = curRef.current; cur.row = rootRef.current; cur.idx = rootRef.current.length; cur.anchor = null; afterEdit(); return }
    const m = /mc-h-(\d+)/.exec(el.className); if (!m) return
    const s = mapRef.current[Number(m[1])]; if (!s) return
    const cur = curRef.current; cur.row = s.row; cur.idx = s.idx; cur.anchor = null
    if (cmdRef.current !== null) commitCmd()
    afterEdit()
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
  }), [emit]) // eslint-disable-line react-hooks/exhaustive-deps

  const { html, map } = render(rootRef.current, curRef.current)
  mapRef.current = map

  return (
    <div ref={surfRef} tabIndex={0} onKeyDown={onKeyDown} onClick={onClick}
      className="flex-1 min-h-0 overflow-auto bg-white p-8 flex flex-col items-center justify-center outline-none cursor-text select-none">
      <div className="text-[#202124] text-3xl" dangerouslySetInnerHTML={{ __html: html }} />
      {cmdRef.current !== null && <div className="mt-3 text-xs font-mono text-primary bg-primary/10 px-2 py-1 rounded">\{cmdRef.current}<span className="opacity-50">│</span></div>}
    </div>
  )
})

export default MathTreeEditor
