// The spreadsheet's OBJECT clipboard — the floating objects (charts, pictures,
// shapes, equations) a copy/cut carries, as plain JSON.
//
// Why a dedicated store: the editor's `clipboard` ref carries CELLS only, and the
// system clipboard cannot hold our object model. Objects therefore get their own
// slot, module-wide (so a copy survives switching sheets) and observable, because
// the Paste button of the context menus must go from disabled to enabled the moment
// something is copied.
//
// The slot holds a LIST: a multi-selection is cut/copied as a whole and pasted as a
// whole, keeping the objects' relative positions (Excel's behaviour). A single
// object is just a list of one, which is why the one-object helpers below are kept
// as thin wrappers instead of a second code path.
//
// Excel semantics, reproduced here: a COPY can be pasted any number of times; a CUT
// can be pasted once and then the slot is empty.

import { useSyncExternalStore } from 'react'
import type { ObjKind, ObjRef } from './types'

/** One object in the slot: its nature, its detached data, and where it came from. */
export interface ObjClipItem<T = unknown> {
  kind: ObjKind
  /** Deep copy of the object, without its identity (the paste mints a fresh id). */
  data: T
  /** Where it came from, so a cut can delete the source on paste. */
  source: ObjRef | null
}

export interface ObjClipEntry<T = unknown> {
  /** Every copied object, in paint order. Never empty. */
  items: ObjClipItem<T>[]
  /** True when the sources must disappear once pasted. */
  cut: boolean
  /**
   * Nature of the FIRST item — kept for the callers that only ask « can I paste a
   * chart here? ». A mixed entry reports its first item's kind, and `kinds` below
   * carries the whole truth.
   */
  kind: ObjKind
  /** Every nature present, deduplicated. */
  kinds: ObjKind[]
  /** First item's data — the one-object shorthand the simple call sites use. */
  data: T
  /** First item's source. */
  source: ObjRef | null
}

let entry: ObjClipEntry | null = null
const listeners = new Set<() => void>()

function emit() { for (const l of Array.from(listeners)) l() }
function subscribe(l: () => void): () => void { listeners.add(l); return () => { listeners.delete(l) } }
function snapshot(): ObjClipEntry | null { return entry }

/** Detach the payload from live state — the slot must never alias the sheet's arrays. */
function clone<T>(data: T): T {
  try { return structuredClone(data) } catch { return JSON.parse(JSON.stringify(data)) as T }
}

/** Build the derived fields, so every producer below fills the entry the same way. */
function makeEntry(items: ObjClipItem[], cut: boolean): ObjClipEntry {
  const copied = items.map(i => ({ ...i, data: clone(i.data) }))
  return {
    items: copied,
    cut,
    kind: copied[0].kind,
    kinds: [...new Set(copied.map(i => i.kind))],
    data: copied[0].data,
    source: copied[0].source,
  }
}

/** Put SEVERAL objects on the clipboard, leaving the originals in place. */
export function copyObjs(items: ObjClipItem[]): void {
  if (!items.length) return
  entry = makeEntry(items, false)
  emit()
}

/**
 * Put several objects on the clipboard as a CUT. The caller keeps them visible: it
 * is the paste that removes the sources (Excel behaviour), so an abandoned cut is a
 * no-op rather than a data loss.
 */
export function cutObjs(items: ObjClipItem[]): void {
  if (!items.length) return
  entry = makeEntry(items, true)
  emit()
}

/** Single-object shorthand. */
export function copyObj<T>(kind: ObjKind, data: T, source: ObjRef | null = null): void {
  copyObjs([{ kind, data, source }])
}

/** Single-object shorthand (cut). */
export function cutObj<T>(kind: ObjKind, data: T, source: ObjRef | null = null): void {
  cutObjs([{ kind, data, source }])
}

/** The current entry, without consuming it (for menu state / previews). */
export function peekObj(): ObjClipEntry | null { return entry }

/** How many objects a paste would insert. */
export function clipCount(): number { return entry ? entry.items.length : 0 }

/** Is there something to paste — optionally, something of a given kind? */
export function canPaste(kind?: ObjKind): boolean {
  return entry != null && (kind == null || entry.kinds.includes(kind))
}

/**
 * Consume the clipboard: returns a fresh deep copy of the entry, and empties the
 * slot when it was a cut.
 */
export function takeObj(): ObjClipEntry | null {
  if (!entry) return null
  const taken = makeEntry(entry.items, entry.cut)
  if (entry.cut) { entry = null; emit() }
  return taken
}

export function clearObjClipboard(): void { if (entry) { entry = null; emit() } }

/** Subscribe a component to the slot (re-renders when it fills or empties). */
export function useObjClipboard(): ObjClipEntry | null {
  return useSyncExternalStore(subscribe, snapshot, snapshot)
}
