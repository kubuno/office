// Format Painter (« Reproduire la mise en forme »), modelled on MS Word.
//
// Word's tool copies the formatting of the current selection and lets you paint
// it onto other content: a single click applies it ONCE (then turns off), a
// double click makes it STICKY (apply to many places until Esc or a second
// click). Shortcuts: Alt+Ctrl+C copies the formatting, Alt+Ctrl+V applies it.
//
// What is copied:
//   • character (run) formatting — bold, italic, underline, strike, sub/super-
//     script, font family/size/color (the `textStyle` mark) and highlight;
//   • paragraph formatting — alignment, line spacing, spacing before/after and
//     the indents.
// Structural marks that are NOT formatting (hyperlinks, comments, tracked
// changes) are deliberately left untouched on the target, exactly like Word,
// which repaints the look without dropping a link or a comment.
import type { Editor } from '@tiptap/core'
import type { EditorState } from '@tiptap/pm/state'

/** Character marks the painter copies and, on apply, clears before repainting. */
const FORMAT_MARKS = ['bold', 'italic', 'underline', 'strike', 'subscript', 'superscript', 'textStyle', 'highlight']
/** Paragraph attributes the painter copies (canvas-engine reads them all). */
const PARA_ATTRS = ['textAlign', 'lineHeight', 'spaceBefore', 'spaceAfter', 'indent', 'indentLeft', 'indentFirstLine', 'indentRight']

export interface CapturedFormat {
  marks: Array<{ type: string; attrs: Record<string, unknown> }>
  para: Record<string, unknown>
}

/** Nearest textblock (paragraph/heading) ancestor of a resolved position. */
function textblockAt(state: EditorState, pos: number) {
  const $pos = state.doc.resolve(pos)
  for (let d = $pos.depth; d >= 0; d--) {
    const n = $pos.node(d)
    if (n.isTextblock) return { node: n, start: $pos.start(d), before: d > 0 ? $pos.before(d) : 0 }
  }
  return null
}

/** Capture the formatting of the current selection (null if there is nothing to read). */
export function captureFormat(editor: Editor): CapturedFormat | null {
  const { state } = editor
  const sel = state.selection
  const { $from, from, to, empty } = sel
  // Character marks: the run at the START of the selection carries the look the
  // user means to copy (Word does the same). For a caret, use the stored/typed marks.
  const raw = empty
    ? (state.storedMarks ?? $from.marks())
    : state.doc.resolve(Math.min(from + 1, to)).marks()
  const marks = raw
    .filter(m => FORMAT_MARKS.includes(m.type.name))
    .map(m => ({ type: m.type.name, attrs: { ...m.attrs } }))

  const block = textblockAt(state, from)
  const para: Record<string, unknown> = {}
  if (block) for (const k of PARA_ATTRS) if (block.node.attrs[k] != null) para[k] = block.node.attrs[k]

  if (!marks.length && !Object.keys(para).length) return { marks: [], para }
  return { marks, para }
}

/** Word boundary around a caret position, so a single click paints the whole word. */
function wordRangeAt(state: EditorState, pos: number): { from: number; to: number } | null {
  const block = textblockAt(state, pos)
  if (!block) return null
  const text = block.node.textContent
  if (!text) return null
  const off = Math.max(0, Math.min(text.length, pos - block.start))
  let a = off, b = off
  while (a > 0 && /\w/.test(text[a - 1])) a--
  while (b < text.length && /\w/.test(text[b])) b++
  if (a === b) return null
  return { from: block.start + a, to: block.start + b }
}

/**
 * Paint the captured formatting onto the current selection (or, for a caret, the
 * word under it). One transaction → one undo step.
 */
export function applyFormat(editor: Editor, cap: CapturedFormat): boolean {
  const { state } = editor
  const { from, to, empty } = state.selection
  let a = from, b = to
  if (empty) {
    const w = wordRangeAt(state, from)
    if (!w) return false
    a = w.from; b = w.to
  }
  const schema = state.schema
  const tr = state.tr
  // 1) Strip existing FORMATTING marks (links/comments/tracked changes survive).
  for (const name of FORMAT_MARKS) {
    const mt = schema.marks[name]
    if (mt) tr.removeMark(a, b, mt)
  }
  // 2) Repaint the captured character marks.
  for (const m of cap.marks) {
    const mt = schema.marks[m.type]
    if (mt) tr.addMark(a, b, mt.create(m.attrs))
  }
  // 3) Copy the paragraph look onto every textblock touched by the range.
  if (Object.keys(cap.para).length) {
    state.doc.nodesBetween(a, b, (node, pos) => {
      if (node.isTextblock) tr.setNodeMarkup(pos, undefined, { ...node.attrs, ...cap.para })
      return true
    })
  }
  if (!tr.docChanged) return false
  editor.view.dispatch(tr.scrollIntoView())
  editor.view.focus()
  return true
}

/**
 * Paintbrush mouse cursor shown while the painter is armed — a little brush over
 * an I-beam, echoing Word. The SVG must be percent-encoded: raw spaces/`<`/`>`
 * make the whole `cursor` value invalid (the browser silently drops it). Hotspot
 * set near the brush tip.
 */
const PAINT_SVG =
  "<svg xmlns='http://www.w3.org/2000/svg' width='28' height='28' viewBox='0 0 28 28'>" +
  "<path d='M4 4 h11 v3 h-11 z' fill='#2b579a'/>" +
  "<path d='M15 5 h3 v4 h-3' fill='none' stroke='#ffb900' stroke-width='1.6'/>" +
  "<path d='M6 7 h7 v6 a1.6 1.6 0 0 1 -1.6 1.6 h-3.8 a1.6 1.6 0 0 1 -1.6 -1.6 z' fill='#ffb900'/>" +
  "<path d='M8.4 14.2 h2.2 v2.2 h-2.2 z' fill='#ffd23f'/>" +
  "<path d='M9.5 16.4 v9 M8 19 h3' stroke='#111111' stroke-width='1.1'/>" +
  "</svg>"
export const PAINT_CURSOR = `url("data:image/svg+xml,${encodeURIComponent(PAINT_SVG)}") 9 25, copy`
