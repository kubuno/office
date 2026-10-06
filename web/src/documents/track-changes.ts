// Track changes (Word « Suivi des modifications ») — the whole model lives here:
// the two ProseMirror marks, the transaction interceptor that turns edits into
// revision marks, the accept/reject commands and the read-only helpers used by the
// review pane and by the canvas renderer.
//
// Deliberately React-free (the canvas engine imports this module) and DOM-free apart
// from the mark (de)serialization callbacks.
//
// Model (see the shared contract):
//   `insertion` — text typed while tracking is on.
//   `deletion`  — text the user tried to delete: the text STAYS in the document,
//                 carrying the mark, until the change is accepted (text goes away)
//                 or rejected (mark goes away).
// Both marks carry the same attributes: author / authorId / date / id.

import { Extension, Mark as TipTapMark } from '@tiptap/core'
import { Plugin, PluginKey, TextSelection } from '@tiptap/pm/state'
import type { EditorState, Transaction } from '@tiptap/pm/state'
import { Fragment, Slice } from '@tiptap/pm/model'
import type { Mark as PMMark, MarkType, Node as PMNode } from '@tiptap/pm/model'
import { Mapping } from '@tiptap/pm/transform'
import type { Step, StepMap } from '@tiptap/pm/transform'
import { ySyncPluginKey, yUndoPluginKey } from '@tiptap/y-tiptap'
import { i18n } from '@kubuno/sdk'

// ── i18n helper ────────────────────────────────────────────────────────────────

// Non-React module: use the shared i18next instance, never a hardcoded string.
function t(key: string, fallback: string): string {
  const v: unknown = i18n.t(key, { ns: 'office', defaultValue: fallback })
  return typeof v === 'string' ? v : fallback
}

// ── Model ──────────────────────────────────────────────────────────────────────

/** Mark names — also the OOXML pair `w:ins` / `w:del`. */
export const MARK_INSERTION = 'insertion'
export const MARK_DELETION = 'deletion'

export type ChangeKind = 'insertion' | 'deletion'

/** Author of the changes produced by the local editor. Both fields may be empty. */
export interface TrackChangesUser {
  /** Stable identifier, may be empty (unattributed change). */
  id: string
  /** Display name ("Jean Dupont"), may be empty. */
  name: string
}

/** One revision, as reported by `documentChanges()`. */
export interface TrackedChange {
  /** Change identifier, the handle for accept/reject. */
  id: string
  kind: ChangeKind
  /** Display name of the author, may be empty (unattributed change). */
  author: string
  /** Stable identifier of the author, may be empty. */
  authorId: string
  /** ISO-8601 UTC date. */
  date: string
  /** First document position covered by the change. */
  from: number
  /** Last document position covered by the change. */
  to: number
  /** Text of the change (paragraph breaks become "\n"). */
  text: string
}

/** Anything holding a ProseMirror state we can dispatch onto (a TipTap `Editor` fits). */
export interface ChangeTarget {
  state: EditorState
  view: { dispatch: (tr: Transaction) => void }
}

const DEFAULT_USER: TrackChangesUser = { id: '', name: '' }

/** ISO-8601 UTC, second precision (the OOXML `w:date` form). */
function nowIso(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z')
}

// `crypto.randomUUID` throws in an insecure context (plain HTTP, non-localhost).
function uid(): string {
  return Math.random().toString(36).slice(2, 10) + Date.now().toString(36).slice(-4)
}

/** Two attribute sets belong to the same author (empty ids fall back to the name). */
function sameAuthor(attrs: Record<string, unknown>, user: TrackChangesUser): boolean {
  const aid = String(attrs.authorId ?? '')
  const mine = user.id ?? ''
  if (aid || mine) return aid === mine
  return String(attrs.author ?? '') === (user.name ?? '')
}

function changeAttrs(user: TrackChangesUser, reuse: { id: string; date: string } | null): Record<string, string> {
  return {
    author: user.name ?? '',
    authorId: user.id ?? '',
    date: reuse ? reuse.date : nowIso(),
    id: reuse ? reuse.id : uid(),
  }
}

// ── Author colours ─────────────────────────────────────────────────────────────

// Same palette as the collaborative presence (vivid hues, readable on a light page).
const AUTHOR_PALETTE = [
  '#1a73e8', '#d93025', '#1e8e3e', '#f9ab00', '#9334e6',
  '#e8710a', '#12b5cb', '#d01884', '#7cb342', '#3949ab',
]

/** Stable colour for an author (pure: same key ⇒ same colour). Empty key ⇒ first hue. */
export function authorColor(key: string): string {
  const s = key || ''
  let h = 0
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0
  return AUTHOR_PALETTE[h % AUTHOR_PALETTE.length]
}

/** Colour of a change (keyed on the author id, or on the name when there is none). */
export function changeColor(change: Pick<TrackedChange, 'author' | 'authorId'>): string {
  return authorColor(change.authorId || change.author)
}

// ── Marks ──────────────────────────────────────────────────────────────────────

// `data-ins-*` / `data-del-*` per the contract, plus `-aid` so the author id survives
// a copy/paste round-trip through the hidden ProseMirror DOM.
function attrsFor(prefix: 'ins' | 'del') {
  const a = `data-${prefix}-author`, i = `data-${prefix}-id`, d = `data-${prefix}-date`, x = `data-${prefix}-aid`
  return {
    author: {
      default: '',
      parseHTML: (el: HTMLElement) => el.getAttribute(a) ?? '',
      renderHTML: (attrs: Record<string, unknown>) => (attrs.author ? { [a]: String(attrs.author) } : {}),
    },
    authorId: {
      default: '',
      parseHTML: (el: HTMLElement) => el.getAttribute(x) ?? '',
      renderHTML: (attrs: Record<string, unknown>) => (attrs.authorId ? { [x]: String(attrs.authorId) } : {}),
    },
    date: {
      default: '',
      parseHTML: (el: HTMLElement) => el.getAttribute(d) ?? '',
      renderHTML: (attrs: Record<string, unknown>) => (attrs.date ? { [d]: String(attrs.date) } : {}),
    },
    id: {
      default: '',
      parseHTML: (el: HTMLElement) => el.getAttribute(i) ?? '',
      renderHTML: (attrs: Record<string, unknown>) => (attrs.id ? { [i]: String(attrs.id) } : {}),
    },
  }
}

/** `insertion` ⇄ OOXML `<w:ins>`: text added while tracking was on. */
export const InsertionMark = TipTapMark.create({
  name: MARK_INSERTION,
  inclusive: false,   // typing next to a revision must not silently join it (the plugin decides)
  excludes: '',       // coexists with every other mark, including `deletion`
  addAttributes() { return attrsFor('ins') },
  parseHTML() { return [{ tag: 'span[data-ins-id]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', { ...HTMLAttributes, class: 'kb-ins' }, 0] },
})

/** `deletion` ⇄ OOXML `<w:del>`: text kept in the document but shown struck through. */
export const DeletionMark = TipTapMark.create({
  name: MARK_DELETION,
  inclusive: false,
  excludes: '',
  addAttributes() { return attrsFor('del') },
  parseHTML() { return [{ tag: 'span[data-del-id]' }] },
  renderHTML({ HTMLAttributes }) { return ['span', { ...HTMLAttributes, class: 'kb-del' }, 0] },
})

// ── Plugin state ───────────────────────────────────────────────────────────────

interface TrackChangesState {
  enabled: boolean
  user: TrackChangesUser
}

export interface TrackChangesOptions {
  /** Tracking active at creation time (document setting, persisted in the envelope). */
  enabled: boolean
  /** Author stamped on the marks; can be updated later with `setTrackChangesUser`. */
  user: TrackChangesUser
}

export const trackChangesPluginKey = new PluginKey<TrackChangesState>('trackChanges')

/**
 * Transaction meta flag: "this transaction is already expressed in revision marks,
 * do not re-interpret it". Set it on any programmatic edit that must escape tracking
 * (document import, accept/reject, conversions…).
 */
export const TRACK_SKIP_META = 'trackChangesSkip'

/** Marks a transaction so the tracker leaves it alone. Returns the same transaction. */
export function untracked(tr: Transaction): Transaction {
  return tr.setMeta(TRACK_SKIP_META, true)
}

// prosemirror-history has no exported key; its PluginKey is created as `history$`.
const HISTORY_META = 'history$'

/** Transactions we must never re-interpret: remote (Yjs), undo/redo, already tracked. */
function isIgnored(tr: Transaction): boolean {
  if (tr.getMeta(TRACK_SKIP_META)) return true
  // Collaboration: y-prosemirror stamps every transaction it derives from a remote
  // update with `{ isChangeOrigin: true }` — those already carry their own marks.
  const ysync = tr.getMeta(ySyncPluginKey) as { isChangeOrigin?: boolean } | undefined
  if (ysync && ysync.isChangeOrigin) return true
  if (tr.getMeta(yUndoPluginKey) !== undefined) return true
  if (tr.getMeta(HISTORY_META) !== undefined) return true
  return false
}

// ── Step inspection ────────────────────────────────────────────────────────────

interface ReplaceLike { from: number; to: number; slice: Slice }

// Duck-typed instead of `instanceof`: a duplicated prosemirror-transform copy in the
// bundle would silently break an `instanceof ReplaceStep` test.
// `ReplaceAroundStep` (gapFrom/gapTo — wrap, lift, list changes) is structural: it
// neither adds nor removes text, so it is not tracked.
function asReplace(step: Step): ReplaceLike | null {
  const s = step as unknown as { from?: unknown; to?: unknown; gapFrom?: unknown; slice?: unknown }
  if (typeof s.from !== 'number' || typeof s.to !== 'number') return null
  if (typeof s.gapFrom === 'number') return null
  if (!s.slice || !(s.slice instanceof Slice)) return null
  return { from: s.from, to: s.to, slice: s.slice }
}

/** Does this fragment hold any inline content (text or inline leaf such as an image)? */
function hasInline(frag: Fragment): boolean {
  let found = false
  frag.forEach(child => {
    if (found) return
    if (child.isInline) found = true
    else if (child.content.size) found = hasInline(child.content)
  })
  return found
}

/** True when the inline node is an insertion made by the very same author. */
function isOwnInsertion(marks: readonly PMMark[], insType: MarkType, user: TrackChangesUser): boolean {
  return marks.some(m => m.type === insType && sameAuthor(m.attrs, user))
}

/**
 * Rewrites a fragment about to be deleted into the fragment that stays in the document:
 * every inline node gets the `deletion` mark, EXCEPT the author's own pending insertions
 * which are dropped for good (otherwise typing then erasing would pile up ghosts).
 * `kept` is false when nothing survived — the deletion is then a real one.
 */
function markDeleted(
  frag: Fragment,
  insType: MarkType,
  delType: MarkType,
  delMark: PMMark,
  user: TrackChangesUser,
): { frag: Fragment; kept: boolean } {
  const out: PMNode[] = []
  let kept = false
  frag.forEach(child => {
    if (child.isInline) {
      if (isOwnInsertion(child.marks, insType, user)) return           // really remove it
      kept = true
      // Already-deleted text keeps its original change id (do not re-stamp it).
      if (child.marks.some(m => m.type === delType)) { out.push(child); return }
      out.push(child.mark(delMark.addToSet(child.marks)))
      return
    }
    const inner = markDeleted(child.content, insType, delType, delMark, user)
    if (inner.kept) kept = true
    out.push(child.copy(inner.frag))
  })
  return { frag: Fragment.fromArray(out), kept }
}

/**
 * Change id/date to reuse at `pos` so that consecutive keystrokes form ONE revision
 * instead of one per character. Looks at the inline node just before and just after.
 */
function reuseAt(doc: PMNode, pos: number, type: MarkType, user: TrackChangesUser): { id: string; date: string } | null {
  if (pos < 0 || pos > doc.content.size) return null
  const $p = doc.resolve(pos)
  for (const node of [$p.nodeBefore, $p.nodeAfter]) {
    if (!node) continue
    const m = node.marks.find(mk => mk.type === type && sameAuthor(mk.attrs, user))
    if (m && m.attrs.id) return { id: String(m.attrs.id), date: String(m.attrs.date || nowIso()) }
  }
  return null
}

// ── The interceptor ────────────────────────────────────────────────────────────

interface ChangeRec {
  kind: ChangeKind
  /** Positions in the coordinates of the new state (before our own transaction). */
  from: number
  to: number
  /** For a deletion: the content to put back, marked. */
  slice?: Slice
}

/**
 * Rebuilds every tracked transaction as revision marks:
 *  - inserted content (typing, paste, drag & drop, `replaceWith`, input rules…) gets
 *    the `insertion` mark;
 *  - deleted content is put BACK where it was, carrying the `deletion` mark — except
 *    the author's own pending insertions, which are really removed.
 *
 * Everything goes through ProseMirror steps, so any command reaches this code path.
 * Transactions coming from Yjs, from undo/redo or already flagged `TRACK_SKIP_META`
 * are left untouched.
 */
export function trackChangesPlugin(options: TrackChangesOptions): Plugin<TrackChangesState> {
  return new Plugin<TrackChangesState>({
    key: trackChangesPluginKey,
    state: {
      init: () => ({ enabled: !!options.enabled, user: options.user || DEFAULT_USER }),
      apply(tr, value) {
        const meta = tr.getMeta(trackChangesPluginKey) as Partial<TrackChangesState> | undefined
        if (!meta) return value
        return {
          enabled: typeof meta.enabled === 'boolean' ? meta.enabled : value.enabled,
          user: meta.user ?? value.user,
        }
      },
    },
    appendTransaction(trs, _oldState, newState) {
      const st = trackChangesPluginKey.getState(newState)
      if (!st || !st.enabled) return null
      const insType = newState.schema.marks[MARK_INSERTION]
      const delType = newState.schema.marks[MARK_DELETION]
      if (!insType || !delType) return null

      // Flatten every step, remembering the doc it applied to and whether it is tracked.
      // Untracked steps still take part in the position mapping.
      const maps: StepMap[] = []
      const entries: Array<{ step: Step; doc: PMNode; tracked: boolean }> = []
      for (const tr of trs) {
        const tracked = tr.docChanged && !isIgnored(tr)
        for (let i = 0; i < tr.steps.length; i++) {
          entries.push({ step: tr.steps[i], doc: tr.docs[i], tracked })
          maps.push(tr.steps[i].getMap())
        }
      }
      if (!entries.some(e => e.tracked)) return null

      const recs: ChangeRec[] = []
      for (let g = 0; g < entries.length; g++) {
        const e = entries[g]
        if (!e.tracked) continue
        const rep = asReplace(e.step)
        if (!rep) continue
        // Whole-document replacement = a load / import, never a user edit.
        if (rep.from === 0 && rep.to === e.doc.content.size) continue
        const fwd = new Mapping(maps.slice(g + 1))
        if (rep.to > rep.from) {
          const removed = e.doc.slice(rep.from, rep.to)
          // A purely structural removal (joining two paragraphs with Backspace) holds no
          // inline content: let it happen instead of freezing the caret.
          if (hasInline(removed.content)) {
            recs.push({ kind: 'deletion', from: fwd.map(rep.from, -1), to: fwd.map(rep.from, -1), slice: removed })
          }
        }
        if (rep.slice.size > 0) {
          // In the doc produced by this step the new content spans [from, from + slice.size].
          const a = fwd.map(rep.from, 1), b = fwd.map(rep.from + rep.slice.size, -1)
          if (b > a) recs.push({ kind: 'insertion', from: a, to: b })
        }
      }
      if (!recs.length) return null

      const tr = newState.tr
      untracked(tr)
      let changed = false
      let caretRaw: number | null = null
      let inserted = false

      for (const rec of recs) {
        if (rec.kind === 'insertion') {
          const from = tr.mapping.map(rec.from, 1), to = tr.mapping.map(rec.to, -1)
          if (to <= from) continue
          const attrs = changeAttrs(st.user, reuseAt(tr.doc, from, insType, st.user))
          tr.addMark(from, to, insType.create(attrs))
          inserted = true
          changed = true
          continue
        }
        const slice = rec.slice
        if (!slice) continue
        const at = tr.mapping.map(rec.from, -1)
        const delMark = delType.create(changeAttrs(st.user, reuseAt(tr.doc, at, delType, st.user)))
        const back = markDeleted(slice.content, insType, delType, delMark, st.user)
        if (!back.kept) continue    // the author erased their own pending insertion: really gone
        try {
          tr.replace(at, at, new Slice(back.frag, slice.openStart, slice.openEnd))
        } catch {
          continue                  // content no longer fits here: let the deletion be a real one
        }
        if (caretRaw === null) caretRaw = rec.from
        changed = true
      }

      if (!changed) return null
      // Pure deletion (Backspace / Delete): the caret must stay where the text used to
      // start, i.e. BEFORE the struck-through text — as in Word.
      if (!inserted && caretRaw !== null) {
        const pos = Math.min(tr.mapping.map(caretRaw, -1), tr.doc.content.size)
        try { tr.setSelection(TextSelection.near(tr.doc.resolve(pos), -1)) } catch { /* keep the mapped selection */ }
      }
      return tr
    },
  })
}

/**
 * The TipTap extension to register on the editor. Pulls in both marks.
 * `TrackChangesExt.configure({ enabled, user })`, then `setTrackChangesEnabled()` /
 * `setTrackChangesUser()` at runtime.
 */
export const TrackChangesExt = Extension.create<TrackChangesOptions>({
  name: 'trackChanges',
  addOptions() { return { enabled: false, user: DEFAULT_USER } },
  addExtensions() { return [InsertionMark, DeletionMark] },
  addProseMirrorPlugins() { return [trackChangesPlugin(this.options)] },
})

/** Marks + plugin, for editors registering their extensions by hand. */
export const trackChangesExtensions = [InsertionMark, DeletionMark, TrackChangesExt]

// ── Runtime toggles ────────────────────────────────────────────────────────────

/** Is tracking currently on for this editor state? */
export function isTrackChangesEnabled(state: EditorState | null | undefined): boolean {
  if (!state) return false
  return trackChangesPluginKey.getState(state)?.enabled ?? false
}

/** Author currently stamped on new revisions. */
export function trackChangesUser(state: EditorState | null | undefined): TrackChangesUser {
  if (!state) return DEFAULT_USER
  return trackChangesPluginKey.getState(state)?.user ?? DEFAULT_USER
}

function dispatchMeta(target: ChangeTarget | null | undefined, meta: Partial<TrackChangesState>): boolean {
  if (!target) return false
  const tr = target.state.tr.setMeta(trackChangesPluginKey, meta).setMeta('addToHistory', false)
  untracked(tr)
  target.view.dispatch(tr)
  return true
}

/** Turns tracking on/off (a DOCUMENT setting: persist it in the envelope). */
export function setTrackChangesEnabled(target: ChangeTarget | null | undefined, on: boolean): boolean {
  return dispatchMeta(target, { enabled: on })
}

/** Sets the author stamped on the revisions produced from now on. */
export function setTrackChangesUser(target: ChangeTarget | null | undefined, user: TrackChangesUser): boolean {
  return dispatchMeta(target, { user })
}

// ── Reading the changes ────────────────────────────────────────────────────────

interface Run {
  kind: ChangeKind
  id: string
  attrs: Record<string, unknown>
  from: number
  to: number
  text: string
}

// Contiguous runs of inline nodes sharing one mark (kind + change id). A run may span a
// paragraph boundary; the gap shows up as a hole between `to` and the next `from`.
function collectRuns(doc: PMNode, ids: Set<string> | null): Run[] {
  const runs: Run[] = []
  const open = new Map<string, Run>()   // key `kind:id` → run still growing
  doc.descendants((node, pos) => {
    if (!node.isInline) return true
    for (const kind of [MARK_INSERTION, MARK_DELETION] as ChangeKind[]) {
      const mark = node.marks.find(m => m.type.name === kind)
      if (!mark) continue
      const id = String(mark.attrs.id ?? '')
      if (ids && !ids.has(id)) continue
      const key = `${kind}:${id}`
      const cur = open.get(key)
      if (cur && cur.to === pos) {
        cur.to = pos + node.nodeSize
        cur.text += node.text ?? ''
      } else {
        const run: Run = { kind, id, attrs: mark.attrs, from: pos, to: pos + node.nodeSize, text: node.text ?? '' }
        runs.push(run)
        open.set(key, run)
      }
    }
    return true
  })
  runs.sort((a, b) => a.from - b.from)
  return runs
}

/**
 * Every revision in the document, in document order. Runs sharing a change id are
 * merged into a single entry (a paragraph break inside a change becomes "\n").
 */
export function documentChanges(doc: PMNode): TrackedChange[] {
  const runs = collectRuns(doc, null)
  const byKey = new Map<string, TrackedChange>()
  const out: TrackedChange[] = []
  for (const run of runs) {
    const key = `${run.kind}:${run.id}`
    const prev = run.id ? byKey.get(key) : undefined
    if (prev) {
      prev.text += (run.from > prev.to ? '\n' : '') + run.text
      prev.to = run.to
      continue
    }
    const change: TrackedChange = {
      id: run.id,
      kind: run.kind,
      author: String(run.attrs.author ?? ''),
      authorId: String(run.attrs.authorId ?? ''),
      date: String(run.attrs.date ?? ''),
      from: run.from,
      to: run.to,
      text: run.text,
    }
    if (run.id) byKey.set(key, change)
    out.push(change)
  }
  return out
}

/** The revision with this id, or null. */
export function findChange(doc: PMNode, id: string): TrackedChange | null {
  return documentChanges(doc).find(c => c.id === id) ?? null
}

/** Revisions covering a document position (a text can be inserted AND deleted). */
export function changesAt(doc: PMNode, pos: number): TrackedChange[] {
  return documentChanges(doc).filter(c => pos >= c.from && pos <= c.to)
}

/** Next revision strictly after `pos` (wraps around), or null when there is none. */
export function nextChange(doc: PMNode, pos: number): TrackedChange | null {
  const all = documentChanges(doc)
  if (!all.length) return null
  return all.find(c => c.from > pos) ?? all[0]
}

/** Previous revision strictly before `pos` (wraps around), or null. */
export function prevChange(doc: PMNode, pos: number): TrackedChange | null {
  const all = documentChanges(doc)
  if (!all.length) return null
  for (let i = all.length - 1; i >= 0; i--) if (all[i].from < pos) return all[i]
  return all[all.length - 1]
}

/** Does the document hold at least one revision? */
export function hasChanges(doc: PMNode): boolean {
  return collectRuns(doc, null).length > 0
}

/** Marks of an inline node, read as revision info (for the canvas renderer). */
export function changeInfoOfMarks(marks: readonly PMMark[]): { insertion?: TrackedChange; deletion?: TrackedChange } {
  const res: { insertion?: TrackedChange; deletion?: TrackedChange } = {}
  for (const m of marks) {
    const kind = m.type.name
    if (kind !== MARK_INSERTION && kind !== MARK_DELETION) continue
    res[kind] = {
      id: String(m.attrs.id ?? ''),
      kind,
      author: String(m.attrs.author ?? ''),
      authorId: String(m.attrs.authorId ?? ''),
      date: String(m.attrs.date ?? ''),
      from: 0, to: 0, text: '',
    }
  }
  return res
}

// ── Accept / reject ────────────────────────────────────────────────────────────

// The four rules of the contract, and they are NOT symmetric:
//   accept + insertion → drop the mark      accept + deletion → drop the text
//   reject + insertion → drop the text      reject + deletion → drop the mark
function removesText(mode: 'accept' | 'reject', kind: ChangeKind): boolean {
  return (mode === 'accept') === (kind === MARK_DELETION)
}

function applyDecision(target: ChangeTarget | null | undefined, ids: Set<string> | null, mode: 'accept' | 'reject'): boolean {
  if (!target) return false
  const state = target.state
  const insType = state.schema.marks[MARK_INSERTION]
  const delType = state.schema.marks[MARK_DELETION]
  if (!insType || !delType) return false
  const runs = collectRuns(state.doc, ids)
  if (!runs.length) return false
  const tr = state.tr
  untracked(tr)
  // Runs of the two kinds may cover the same text (inserted by A, deleted by B), so
  // every range is remapped: dropping the text makes the other run collapse.
  for (const run of runs) {
    const from = tr.mapping.map(run.from, 1), to = tr.mapping.map(run.to, -1)
    if (to <= from) continue
    const type = run.kind === MARK_INSERTION ? insType : delType
    if (removesText(mode, run.kind)) tr.delete(from, to)
    else tr.removeMark(from, to, type)
  }
  if (!tr.steps.length) return false
  target.view.dispatch(tr)
  return true
}

/** Accepts one revision: an insertion loses its mark, a deletion loses its text. */
export function acceptChange(target: ChangeTarget | null | undefined, id: string): boolean {
  return applyDecision(target, new Set([id]), 'accept')
}

/** Rejects one revision: an insertion loses its text, a deletion loses its mark. */
export function rejectChange(target: ChangeTarget | null | undefined, id: string): boolean {
  return applyDecision(target, new Set([id]), 'reject')
}

/** Accepts every revision of the document. */
export function acceptAll(target: ChangeTarget | null | undefined): boolean {
  return applyDecision(target, null, 'accept')
}

/** Rejects every revision of the document. */
export function rejectAll(target: ChangeTarget | null | undefined): boolean {
  return applyDecision(target, null, 'reject')
}

// ── Labels ─────────────────────────────────────────────────────────────────────

/** Human label of a revision kind. */
export function changeLabel(kind: ChangeKind): string {
  return kind === MARK_DELETION
    ? t('doc_tc_deleted', 'Supprimé')
    : t('doc_tc_inserted', 'Inséré')
}

/** Author of a revision, never empty (unattributed changes are legal). */
export function changeAuthor(change: Pick<TrackedChange, 'author'>): string {
  return change.author || t('doc_tc_unknown_author', 'Auteur inconnu')
}
