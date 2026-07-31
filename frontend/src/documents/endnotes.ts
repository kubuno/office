// Endnotes (« notes de fin », Word's "Insérer une note de fin").
//
// An endnote is the exact twin of the `footnote` node of `DocumentEditorPage.tsx`
// — an inline ATOM carrying a single `text` attribute — with two differences that
// match Word's defaults:
//   * numbering is LOWERCASE ROMAN (i, ii, iii…) instead of arabic;
//   * the note text is collected at the END OF THE DOCUMENT instead of at the
//     bottom of the page it is called from.
//
// This module owns the MODEL and the commands. The rendering (the superscript
// call inside the text run and the note block itself) belongs to the canvas
// engine, and the persistence to the backend: both match on `ENDNOTE_NODE` and
// number with `endnoteMarker`, so neither hardcodes a string nor a numeral
// system. Nothing here touches React — see `EndnoteDialog.tsx` for the UI.
//
// Wire format (shared with the backend and with a DOCX round trip):
//   <sup data-endnote="the note text">‡</sup>
// A single `data-endnote` attribute does both jobs — it MARKS the element as an
// endnote (an empty value still matches `sup[data-endnote]`) and it CARRIES the
// text. `data-en-text` is also accepted on parse, symmetrical with the
// footnote's `data-fn-text`, so hand-written or converted HTML round-trips.

import { Node as TipTapNode } from '@tiptap/core'
import type { Editor } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'

/// ProseMirror node name. Exported so the canvas engine and the converters can
/// match on it without hardcoding a string.
export const ENDNOTE_NODE = 'endnote'

/// Character shown in the (hidden) ProseMirror DOM and in copied HTML. The
/// on-canvas call shows `endnoteMarker(n)` instead — this is only the fallback
/// glyph for plain-HTML consumers. Double dagger = the traditional second-order
/// note mark, the footnote using the single dagger.
export const ENDNOTE_CHAR = '‡'

/// Attributes of an `endnote` node. One single attribute, by contract.
export interface EndnoteAttrs {
  /** The note's text. Empty right after insertion (the dialog fills it in). */
  text: string
}

/// One endnote resolved in document order, ready to be drawn or listed.
export interface EndnoteRef {
  /** 1-based rank in document order. */
  n: number
  /** `n` rendered the Word way for an endnote: lowercase roman. */
  marker: string
  text: string
  /** Position of the atom in the ProseMirror document. */
  pos: number
}

/// Lowercase roman numeral — Word's default endnote numbering ("i, ii, iii…").
/// Kept self-contained so the canvas engine can number without importing the
/// page-number formatter of the editor page.
export function endnoteMarker(n: number): string {
  if (!Number.isFinite(n) || n < 1) return 'i'
  const map: Array<[number, string]> = [
    [1000, 'm'], [900, 'cm'], [500, 'd'], [400, 'cd'], [100, 'c'], [90, 'xc'],
    [50, 'l'], [40, 'xl'], [10, 'x'], [9, 'ix'], [5, 'v'], [4, 'iv'], [1, 'i'],
  ]
  let v = Math.floor(n), s = ''
  for (const [w, sym] of map) while (v >= w) { s += sym; v -= w }
  return s
}

// ── The node ───────────────────────────────────────────────────────────────────

export const EndnoteExt = TipTapNode.create({
  name: ENDNOTE_NODE,
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  addAttributes() {
    return {
      text: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-endnote') || el.getAttribute('data-en-text') || '',
        renderHTML: (a: Record<string, unknown>) => ({ 'data-endnote': String(a.text ?? '') }),
      },
    }
  },
  parseHTML() { return [{ tag: 'sup[data-endnote]' }] },
  renderHTML({ HTMLAttributes }) { return ['sup', HTMLAttributes, ENDNOTE_CHAR] },
})

// ── Queries ────────────────────────────────────────────────────────────────────

/// Every endnote of the document, in document order, already numbered. This is
/// the list the end-of-document note block is drawn from, and the one a "notes"
/// pane would show.
export function documentEndnotes(doc: PMNode | null | undefined): EndnoteRef[] {
  const out: EndnoteRef[] = []
  if (!doc) return out
  doc.descendants((node, pos) => {
    if (node.type.name !== ENDNOTE_NODE) return
    const n = out.length + 1
    out.push({ n, marker: endnoteMarker(n), text: String(node.attrs.text ?? ''), pos })
  })
  return out
}

/// Text of the endnote at `pos`, or null when that position holds something
/// else — the guard every command and the dialog use before touching the doc
/// (positions move under collaborative edits).
export function endnoteTextAt(ed: Editor | null | undefined, pos: number): string | null {
  const node = ed?.state.doc.nodeAt(pos)
  return node?.type.name === ENDNOTE_NODE ? String(node.attrs.text ?? '') : null
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// Inserts an empty endnote call at the caret and returns its position, or null
/// when the insertion did not land an `endnote` there (no editor, or the schema
/// of the active editor — header/footer, text box — has no endnote). The caller
/// then opens the editor on that position, exactly like the footnote does.
export function insertEndnote(ed: Editor | null | undefined): number | null {
  if (!ed || !ed.schema.nodes[ENDNOTE_NODE]) return null
  ed.chain().focus().insertContent({ type: ENDNOTE_NODE, attrs: { text: '' } }).run()
  const pos = ed.state.selection.from - 1
  return endnoteTextAt(ed, pos) == null ? null : pos
}

/// Replaces the text of the endnote at `pos`. No-op if the node moved away.
export function setEndnoteText(ed: Editor | null | undefined, pos: number, text: string): void {
  if (!ed || endnoteTextAt(ed, pos) == null) return
  ed.view.dispatch(ed.state.tr.setNodeMarkup(pos, undefined, { text }))
}

/// Deletes the endnote at `pos` (Word: deleting the call deletes the note). The
/// remaining notes renumber themselves, numbering being positional.
export function deleteEndnote(ed: Editor | null | undefined, pos: number): void {
  if (!ed || endnoteTextAt(ed, pos) == null) return
  ed.view.dispatch(ed.state.tr.delete(pos, pos + 1))
}
