// Word's « Note de bas de page suivante » split button: move the caret from one
// note reference to the next (or previous), footnotes and endnotes being two
// independent sequences.
//
// A note reference is an INLINE ATOM, so "the caret is on it" means the caret
// sits just before it. Navigation therefore compares positions, never text.
import type { Editor } from '@tiptap/react'
import { TextSelection } from '@tiptap/pm/state'

import { ENDNOTE_NODE } from '../endnotes'

export const FOOTNOTE_NODE = 'footnote'

export type NoteKind = 'footnote' | 'endnote'
export type NoteDirection = 'next' | 'prev'

const nodeNameFor = (kind: NoteKind): string => (kind === 'endnote' ? ENDNOTE_NODE : FOOTNOTE_NODE)

/** Positions of every note reference of one kind, in document order. */
export function notePositions(ed: Editor | null | undefined, kind: NoteKind): number[] {
  if (!ed) return []
  const name = nodeNameFor(kind)
  const out: number[] = []
  ed.state.doc.descendants((node, pos) => {
    if (node.type.name === name) out.push(pos)
    return true
  })
  return out
}

export function hasNotes(ed: Editor | null | undefined, kind: NoteKind): boolean {
  return notePositions(ed, kind).length > 0
}

/**
 * Move to the next/previous note of that kind and return its position.
 * Wraps around, the way Word does, so the button never becomes a dead end.
 * Returns `null` when the document holds no note of that kind.
 */
export function gotoNote(
  ed: Editor | null | undefined,
  kind: NoteKind,
  dir: NoteDirection,
): number | null {
  if (!ed) return null
  const positions = notePositions(ed, kind)
  if (!positions.length) return null
  const here = ed.state.selection.from
  const target = dir === 'next'
    ? positions.find(p => p > here) ?? positions[0]
    : [...positions].reverse().find(p => p < here) ?? positions[positions.length - 1]
  const tr = ed.state.tr.setSelection(TextSelection.create(ed.state.doc, target))
  ed.view.dispatch(tr.scrollIntoView())
  ed.commands.focus()
  return target
}

/**
 * Word's « Afficher les notes »: open the note the caret is on, else the first
 * one of the document. Returns the position handed to the editor, or `null`
 * when there is nothing to show.
 */
export function noteToShow(ed: Editor | null | undefined, kind: NoteKind): number | null {
  const positions = notePositions(ed, kind)
  if (!positions.length) return null
  const here = ed?.state.selection.from ?? 0
  // The note the caret sits on (or just after) wins; otherwise the first one.
  return [...positions].reverse().find(p => p <= here) ?? positions[0]
}
