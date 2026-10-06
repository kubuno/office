// Outline level — the single source of truth for "what is a table-of-contents
// entry", shared by the TOC generator and the navigation panel.
//
// Word's rule, which this file implements: an EXPLICIT outline level always wins
// over the paragraph style. A "Titre 1" whose outline level was set back to
// "Corps de texte" does NOT appear in the table of contents, and a plain
// paragraph promoted to level 2 DOES. That is exactly what the ribbon's
// « Ajouter le texte » menu changes — it writes `outlineLevel`, nothing else.
//
// `outlineLevel` already round-trips to `w:outlineLvl` in both converters
// (docx/read/paragraph.rs, docx/write/paragraph.rs, doc/pap.rs), so a level set
// here survives a save to .docx and back.
import type { Editor } from '@tiptap/react'
import type { Node as PMNode } from '@tiptap/pm/model'

export interface OutlineItem {
  text: string
  level: number
  pos: number
  page: number
}

/** Blocks that can carry an outline level (same list as `ParagraphFormatExt`). */
const OUTLINE_TYPES = ['paragraph', 'heading']

/**
 * Effective outline level of a block: 0 = body text (never an entry),
 * 1..9 = a table-of-contents entry at that level.
 */
export function outlineLevelOf(node: PMNode): number {
  return outlineLevelOfJson({ type: node.type.name, attrs: node.attrs as Record<string, unknown> })
}

/**
 * Same rule on a PLAIN JSON node — the form the canvas engine works with.
 * Collapsible headings (Word « Développer/Réduire ») key on this too: Word
 * drives both the table of contents and the collapse triangles from the
 * outline level, never from the style name.
 */
export function outlineLevelOfJson(node: { type?: string; attrs?: Record<string, unknown> | null }): number {
  const a = (node.attrs ?? {}) as Record<string, unknown>
  const explicit = a.outlineLevel
  if (explicit != null && Number.isFinite(Number(explicit))) {
    const n = Math.trunc(Number(explicit))
    return n >= 0 && n <= 9 ? n : 0
  }
  if (node.type !== 'heading') return 0
  const lvl = Math.trunc(Number(a.level ?? 1))
  return lvl >= 1 && lvl <= 9 ? lvl : 1
}

/** Is this block a table-of-contents entry? */
export function isOutlineEntry(node: PMNode): boolean {
  return outlineLevelOf(node) > 0
}

/**
 * Every outline entry of the document, in reading order.
 * `pageOf` maps a document position to a 1-based page number.
 */
export function collectOutline(doc: PMNode, pageOf: (pos: number) => number): OutlineItem[] {
  const items: OutlineItem[] = []
  doc.descendants((node, pos) => {
    const level = outlineLevelOf(node)
    if (level > 0) {
      items.push({ text: node.textContent || '…', level, pos, page: pageOf(pos) })
      return false   // an entry never nests another
    }
    return true
  })
  return items
}

/** Outline level of the block holding the caret (0 when there is none). */
export function currentOutlineLevel(ed: Editor | null | undefined): number {
  if (!ed) return 0
  const $from = ed.state.selection.$from
  for (let d = $from.depth; d >= 0; d--) {
    const node = $from.node(d)
    if (OUTLINE_TYPES.includes(node.type.name)) return outlineLevelOf(node)
  }
  return 0
}

/**
 * Word's « Ajouter le texte » : set the outline level of the current block.
 * `0` means body text — the explicit value is kept (rather than cleared) so a
 * heading can be removed from the table of contents without losing its style.
 */
export function setOutlineLevel(ed: Editor | null | undefined, level: number): void {
  if (!ed) return
  const $from = ed.state.selection.$from
  let typeName: string | null = null
  for (let d = $from.depth; d >= 0; d--) {
    const name = $from.node(d).type.name
    if (OUTLINE_TYPES.includes(name)) { typeName = name; break }
  }
  if (!typeName) return
  const clamped = Math.max(0, Math.min(9, Math.trunc(level)))
  ed.chain().focus().updateAttributes(typeName, { outlineLevel: clamped }).run()
}
