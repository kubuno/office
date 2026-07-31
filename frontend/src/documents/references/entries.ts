// Index entries and legal citations — Word's « Marquer entrée » (XE field) and
// « Marquer citation » (TA field).
//
// Both are MARKS rather than nodes: Word hides them as field codes inside the
// text, and the marked words must keep flowing with the paragraph. A mark also
// survives editing around it, which a hidden atom would not.
import { Mark as TipTapMark } from '@tiptap/core'
import type { Editor } from '@tiptap/react'
import type { Node as PMNode } from '@tiptap/pm/model'

export const INDEX_MARK = 'indexEntry'
export const CITATION_MARK = 'citationEntry'

const strAttr = (name: string, data: string) => ({
  default: '',
  parseHTML: (el: HTMLElement) => el.getAttribute(data) || '',
  renderHTML: (a: Record<string, unknown>) => (a[name] ? { [data]: String(a[name]) } : {}),
})

/** `XE "entrée:sous-entrée"` — the text is the mark's, not the document's. */
export const IndexEntryMark = TipTapMark.create({
  name: INDEX_MARK,
  inclusive: false,
  addAttributes() {
    return { text: strAttr('text', 'data-xe'), sub: strAttr('sub', 'data-xe-sub') }
  },
  parseHTML() { return [{ tag: 'span[data-xe]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', HTMLAttributes, 0] },
})

/** `TA \l "citation longue" \s "courte" \c catégorie`. */
export const CitationEntryMark = TipTapMark.create({
  name: CITATION_MARK,
  inclusive: false,
  addAttributes() {
    return {
      short: strAttr('short', 'data-ta-short'),
      long: strAttr('long', 'data-ta-long'),
      category: strAttr('category', 'data-ta-cat'),
    }
  },
  parseHTML() { return [{ tag: 'span[data-ta-long]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', HTMLAttributes, 0] },
})

export const referenceEntryExtensions = [IndexEntryMark, CitationEntryMark]

export interface IndexHit { text: string; sub: string; page: number }
export interface CitationHit { short: string; long: string; category: string; page: number }

/** Every index mark of the document, with the page it falls on. */
export function collectIndexHits(doc: PMNode, pageOf: (pos: number) => number): IndexHit[] {
  const out: IndexHit[] = []
  doc.descendants((node, pos) => {
    if (!node.isText) return true
    for (const m of node.marks) {
      if (m.type.name !== INDEX_MARK) continue
      const text = String(m.attrs.text ?? '').trim() || node.textContent.trim()
      if (text) out.push({ text, sub: String(m.attrs.sub ?? '').trim(), page: pageOf(pos) })
    }
    return true
  })
  return out
}

export function collectCitationHits(doc: PMNode, pageOf: (pos: number) => number): CitationHit[] {
  const out: CitationHit[] = []
  doc.descendants((node, pos) => {
    if (!node.isText) return true
    for (const m of node.marks) {
      if (m.type.name !== CITATION_MARK) continue
      const long = String(m.attrs.long ?? '').trim() || node.textContent.trim()
      if (long) {
        out.push({
          long,
          short: String(m.attrs.short ?? '').trim(),
          category: String(m.attrs.category ?? 'cases').trim() || 'cases',
          page: pageOf(pos),
        })
      }
    }
    return true
  })
  return out
}

/** Text currently selected — the default entry Word proposes when marking. */
export function selectedText(ed: Editor | null | undefined): string {
  if (!ed) return ''
  const { from, to } = ed.state.selection
  return from === to ? '' : ed.state.doc.textBetween(from, to, ' ').trim()
}

export function markIndexEntry(ed: Editor | null | undefined, text: string, sub: string): void {
  if (!ed || !text.trim()) return
  const { from, to } = ed.state.selection
  if (from === to) {
    // Nothing selected: Word still records the entry, anchored where the caret is.
    ed.chain().focus().insertContent({
      type: 'text', text: '​',
      marks: [{ type: INDEX_MARK, attrs: { text: text.trim(), sub: sub.trim() } }],
    }).run()
    return
  }
  ed.chain().focus().setMark(INDEX_MARK, { text: text.trim(), sub: sub.trim() }).run()
}

export function markCitation(
  ed: Editor | null | undefined,
  long: string, short: string, category: string,
): void {
  if (!ed || !long.trim()) return
  const { from, to } = ed.state.selection
  const attrs = { long: long.trim(), short: short.trim(), category }
  if (from === to) {
    ed.chain().focus().insertContent({ type: 'text', text: '​', marks: [{ type: CITATION_MARK, attrs }] }).run()
    return
  }
  ed.chain().focus().setMark(CITATION_MARK, attrs).run()
}
