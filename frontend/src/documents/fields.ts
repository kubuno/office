// Word fields (« champs ») for the document editor.
//
// A Word field is not text: it is an instruction (`PAGE`, `NUMPAGES \* ROMAN`,
// `PAGEREF _Ref42 \h`…) plus the *cached result* Word computed the last time it
// recalculated. This module gives the frontend the model the backend already
// speaks — see `src/converters/docx/read/fields.rs` (`FieldKind`) and
// `src/converters/doc/field.rs` — as an inline atom node so a field survives a
// DOCX round trip without being flattened to plain text.
//
// GOLDEN RULE, identical to the backend and to LibreOffice: a field we cannot
// honestly recompute shows its CACHED result — never its raw instruction, and
// never nothing.
//
// Modelled on the `footnote` node of `DocumentEditorPage.tsx`: inline atom,
// attributes serialised through `data-*`.

import { Node as TipTapNode } from '@tiptap/core'
import type { Editor, JSONContent } from '@tiptap/core'
import { i18n } from '@kubuno/sdk'

// ── Kinds ──────────────────────────────────────────────────────────────────────

/// The field kinds we model. One-to-one with the backend `FieldKind`:
/// `Page → page`, `NumPages → pages`, `Date → date`, `Title → title`,
/// `PageRef → ref`, `Frozen → other`. `Toc` and `Hyperlink` are deliberately
/// absent: the readers turn them into paragraphs carrying `tocLevel`/`tocPage`
/// and into `link` marks, so they never reach this node.
export type FieldKind = 'page' | 'pages' | 'date' | 'title' | 'ref' | 'other'

/// Insertion order used by menus.
export const FIELD_KINDS: readonly FieldKind[] = ['page', 'pages', 'date', 'title', 'ref', 'other']

/// ProseMirror node name. Exported so the canvas engine can match on it without
/// hardcoding a string.
export const FIELD_NODE = 'field'

/// Attributes of a `field` node. The NAMES are the wire format shared with the
/// backend: `kind`, `instr` (raw Word instruction, kept verbatim for a
/// non-destructive round trip) and `cached` (last known result = what shows).
export interface FieldAttrs {
  kind: FieldKind
  instr: string
  cached: string
}

/// Everything needed to recompute the computable kinds. All optional: an absent
/// value simply means "cannot compute", which falls back to the cached result.
export interface FieldContext {
  /** Current page number, already offset by `pageNumStart`. */
  page?: number | null
  /** Total page count of the document. */
  pages?: number | null
  /** Document title (the `.kbdoc` file name). */
  title?: string | null
  /** BCP-47 locale for date formatting; defaults to the i18next language. */
  locale?: string
  /** Injectable clock, for tests and for a stable value across one repaint. */
  now?: Date
  /** Page-number formatter (arabic/roman/alpha) — `formatPageNumber` of the editor. */
  formatNumber?: (n: number) => string
  /** Date formatter; defaults to `toLocaleDateString(locale)`, like `expandHFDoc`. */
  formatDate?: (d: Date, locale: string) => string
  /** Resolved `PAGEREF`/bookmark targets, by bookmark name. */
  refs?: Readonly<Record<string, string>>
  /** Page number of the field at a given ProseMirror position (body fields). */
  pageAt?: (pos: number) => number | null | undefined
}

// ── i18n helper ────────────────────────────────────────────────────────────────

// Non-React module: use the shared i18next instance, never a hardcoded string.
function tr(key: string, fallback: string): string {
  const v: unknown = i18n.t(key, { ns: 'office', defaultValue: fallback })
  return typeof v === 'string' ? v : fallback
}

/// Human label of a kind — used by menus AND as the last-resort display text of
/// a field with an empty cached result (a field is never invisible).
export function fieldLabel(kind: FieldKind): string {
  switch (kind) {
    case 'page':  return tr('doc_field_page',  'N° de page')
    case 'pages': return tr('doc_field_pages', 'Nb pages')
    case 'date':  return tr('doc_field_date',  'Date')
    case 'title': return tr('doc_field_title', 'Titre')
    case 'ref':   return tr('doc_field_ref',   'Renvoi')
    case 'other': return tr('doc_field_other', 'Champ')
  }
}

// ── Instruction parsing ────────────────────────────────────────────────────────

/// Coerce an unknown attribute into a valid kind. Unknown kinds become `other`,
/// which is exactly the backend's `Frozen`: keep the cached result.
export function asFieldKind(v: unknown): FieldKind {
  return typeof v === 'string' && (FIELD_KINDS as readonly string[]).includes(v) ? (v as FieldKind) : 'other'
}

/// Field type name of an instruction (`PAGEREF _Ref1 \h` → `PAGEREF`).
export function fieldNameOf(instr: string): string {
  const m = /^\s*([A-Za-z=]+)/.exec(instr)
  return m && m[1] ? m[1].toUpperCase() : ''
}

/// Word field name → kind. Mirrors `field_kind` in
/// `src/converters/docx/read/fields.rs` EXACTLY: only `PAGEREF` is a `ref`
/// (plain `REF` stays frozen there too), and only `DATE`/`TIME` are recomputed
/// dates — `CREATEDATE` & co. keep their cached value.
export function fieldKindFromName(name: string): FieldKind {
  switch (name.toUpperCase()) {
    case 'PAGE': return 'page'
    case 'NUMPAGES':
    case 'SECTIONPAGES': return 'pages'
    case 'DATE':
    case 'TIME': return 'date'
    case 'TITLE': return 'title'
    case 'PAGEREF': return 'ref'
    default: return 'other'
  }
}

/// Kind of a raw instruction.
export function fieldKindFromInstr(instr: string): FieldKind {
  return fieldKindFromName(fieldNameOf(instr))
}

/// Canonical instruction emitted when the USER inserts a field (imported files
/// keep their own instruction verbatim instead).
export function fieldInstrFor(kind: FieldKind, arg?: string): string {
  switch (kind) {
    case 'page':  return 'PAGE'
    case 'pages': return 'NUMPAGES'
    case 'date':  return 'DATE'
    case 'title': return 'TITLE'
    case 'ref':   return arg ? `PAGEREF ${quoteArg(arg)} \\h` : 'PAGEREF'
    case 'other': return arg ?? ''
  }
}

function quoteArg(s: string): string {
  return /[\s"]/.test(s) ? `"${s.replace(/"/g, '')}"` : s
}

/// Bookmark name targeted by a `PAGEREF`/`REF` instruction, `''` when absent.
export function fieldRefName(instr: string): string {
  const m = /^\s*[A-Za-z]+\s+(?:"([^"]*)"|([^\s\\]+))/.exec(instr)
  return (m && (m[1] ?? m[2])) || ''
}

// A `\@ "d MMMM yyyy"` picture switch asks for a format we do not implement.
// Rather than silently changing the user's date format we keep the cached
// result — same policy as the backend for everything it cannot honour.
function hasDatePicture(instr: string): boolean {
  return /\\@/.test(instr)
}

// ── Computation ────────────────────────────────────────────────────────────────

function fmtNum(n: number, ctx: FieldContext): string {
  return ctx.formatNumber ? ctx.formatNumber(n) : String(n)
}

/// The value we can honestly compute, or `null` when we cannot.
function computeField(kind: FieldKind, ctx: FieldContext, instr: string): string | null {
  switch (kind) {
    case 'page':
      return ctx.page != null ? fmtNum(ctx.page, ctx) : null
    case 'pages':
      return ctx.pages != null ? fmtNum(ctx.pages, ctx) : null
    case 'date': {
      if (hasDatePicture(instr)) return null
      const locale = ctx.locale || i18n.language || 'fr'
      const d = ctx.now ?? new Date()
      return ctx.formatDate ? ctx.formatDate(d, locale) : d.toLocaleDateString(locale)
    }
    case 'title':
      return ctx.title ? ctx.title : null
    case 'ref': {
      const name = fieldRefName(instr)
      const hit = name && ctx.refs ? ctx.refs[name] : undefined
      return hit != null && hit !== '' ? hit : null
    }
    case 'other':
      return null
  }
}

/// TEXT TO DISPLAY for a field. `page`/`pages`/`date` (and `title`/`ref` when
/// the context provides them) are recomputed; everything else falls back to the
/// cached result, then — only if the cache is empty — to the kind's label, so a
/// field is never rendered as nothing and never as its instruction.
///
/// `instr` is optional and only used by the kinds whose computation depends on
/// it (`ref` target, `date` picture switch).
export function fieldText(kind: FieldKind, cached: string, ctx: FieldContext = {}, instr = ''): string {
  const computed = computeField(kind, ctx, instr)
  if (computed != null && computed !== '') return computed
  if (cached) return cached
  return fieldLabel(kind)
}

/// Read a loose attribute bag (JSON document, ProseMirror `node.attrs`) into
/// typed attributes.
export function readFieldAttrs(attrs: Record<string, unknown> | null | undefined): FieldAttrs {
  const a = attrs ?? {}
  return {
    kind: asFieldKind(a.kind),
    instr: typeof a.instr === 'string' ? a.instr : '',
    cached: typeof a.cached === 'string' ? a.cached : '',
  }
}

/// Display text of a JSON/ProseMirror `field` node — the convenience the canvas
/// engine wants: `fieldNodeText(inline, ctx)`.
export function fieldNodeText(node: { attrs?: Record<string, unknown> | null } | null | undefined, ctx: FieldContext = {}): string {
  const a = readFieldAttrs(node?.attrs)
  return fieldText(a.kind, a.cached, ctx, a.instr)
}

/// True when a JSON node is a field node.
export function isFieldNode(node: JSONContent | null | undefined): boolean {
  return node?.type === FIELD_NODE
}

// ── Header/footer tokens ───────────────────────────────────────────────────────

// The ONLY tokens the editor understands, i.e. those produced by the backend
// (`hf_token` in `read/fields.rs`) and substituted by `expandHFDoc` in
// `DocumentEditorPage.tsx`: `{page}`, `{pages}`, `{date}`, `{titre}`.
// `{title}` is accepted on input because `expandHFDoc` also accepts it.
const TOKEN_KINDS: Readonly<Record<string, FieldKind>> = {
  page: 'page',
  pages: 'pages',
  date: 'date',
  titre: 'title',
  title: 'title',
}

// `pages` first: alternation is ordered, `page` would otherwise win on `{pages}`.
const TOKEN_SOURCE = '\\{(pages|page|date|titre|title)\\}'

/// Canonical token of a kind, `null` when the kind has no token form (`ref`,
/// `other`). Used when writing a header back out in token form.
export function fieldToken(kind: FieldKind): string | null {
  switch (kind) {
    case 'page':  return '{page}'
    case 'pages': return '{pages}'
    case 'date':  return '{date}'
    case 'title': return '{titre}'
    default: return null
  }
}

function textNode(text: string, marks?: JSONContent['marks']): JSONContent {
  return marks && marks.length ? { type: 'text', text, marks } : { type: 'text', text }
}

/// Build a `field` JSON node.
export function fieldJson(kind: FieldKind, opts: { instr?: string; cached?: string; marks?: JSONContent['marks'] } = {}): JSONContent {
  const node: JSONContent = {
    type: FIELD_NODE,
    attrs: { kind, instr: opts.instr ?? fieldInstrFor(kind), cached: opts.cached ?? '' },
  }
  if (opts.marks && opts.marks.length) node.marks = opts.marks
  return node
}

/// Split a header/footer string on its dynamic tokens and return the inline
/// content: text runs (carrying `marks`, so formatting survives) interleaved
/// with real `field` nodes. Returns a single text node when there is no token.
export function fieldsFromTokens(text: string, marks?: JSONContent['marks']): JSONContent[] {
  const out: JSONContent[] = []
  const re = new RegExp(TOKEN_SOURCE, 'gi')
  let last = 0
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > last) out.push(textNode(text.slice(last, m.index), marks))
    const kind = TOKEN_KINDS[(m[1] ?? '').toLowerCase()] ?? 'other'
    out.push(fieldJson(kind, { marks }))
    last = m.index + m[0].length
  }
  if (!out.length) return text ? [textNode(text, marks)] : []
  if (last < text.length) out.push(textNode(text.slice(last), marks))
  return out
}

/// Convert every text node of a JSON document (typically a header/footer) into
/// text + `field` nodes. Pure: returns a new document.
export function fieldsFromTokensInDoc<T extends JSONContent>(doc: T): T {
  const walk = (n: JSONContent): JSONContent[] => {
    if (n.type === 'text') return fieldsFromTokens(n.text ?? '', n.marks)
    if (n.content) return [{ ...n, content: n.content.flatMap(walk) }]
    return [n]
  }
  return (doc.content ? { ...doc, content: doc.content.flatMap(walk) } : doc) as T
}

/// Refresh the `cached` attribute of every `field` node of a JSON document
/// (header/footer rendered by the canvas, exported document…). Pure.
export function resolveFieldsInDoc<T extends JSONContent>(doc: T, ctx: FieldContext): T {
  const walk = (n: JSONContent): JSONContent => {
    if (isFieldNode(n)) {
      const a = readFieldAttrs(n.attrs)
      const next = fieldText(a.kind, a.cached, ctx, a.instr)
      return next === a.cached ? n : { ...n, attrs: { ...a, cached: next } }
    }
    if (n.content) return { ...n, content: n.content.map(walk) }
    return n
  }
  return walk(doc) as T
}

// ── TipTap extension ───────────────────────────────────────────────────────────

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    field: {
      /** Insert a Word field at the caret (replacing the selection). */
      insertField: (kind: FieldKind, opts?: { instr?: string; cached?: string }) => ReturnType
      /** Recompute the cached result of every field of the document. */
      refreshFields: (ctx?: FieldContext) => ReturnType
    }
  }
}

/// Inline atom holding a Word field. Same shape as `FootnoteExt`: the payload
/// lives in attributes and is serialised through `data-*`.
export const FieldExt = TipTapNode.create({
  name: FIELD_NODE,
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      kind: {
        default: 'other' as FieldKind,
        parseHTML: (el: HTMLElement) => asFieldKind(el.getAttribute('data-field')),
        renderHTML: (a: Record<string, unknown>) => ({ 'data-field': asFieldKind(a.kind) }),
      },
      // Raw Word instruction, kept verbatim for a lossless round trip.
      instr: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-field-instr') || '',
        renderHTML: (a: Record<string, unknown>) => (a.instr ? { 'data-field-instr': String(a.instr) } : {}),
      },
      // Last known result: what shows when we cannot recompute.
      cached: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-field-cached') ?? el.textContent ?? '',
        renderHTML: (a: Record<string, unknown>) => (a.cached ? { 'data-field-cached': String(a.cached) } : {}),
      },
    }
  },

  parseHTML() { return [{ tag: 'span[data-field]' }] },

  renderHTML({ node, HTMLAttributes }) {
    const a = readFieldAttrs(node.attrs as Record<string, unknown>)
    // No context here: the static form of a field is its cached result.
    return ['span', { ...HTMLAttributes, class: 'kb-field' }, fieldText(a.kind, a.cached)]
  },

  // Plain-text serialisation (getText, spell check, clipboard as text).
  renderText({ node }) {
    const a = readFieldAttrs(node.attrs as Record<string, unknown>)
    return fieldText(a.kind, a.cached)
  },

  addCommands() {
    return {
      insertField: (kind: FieldKind, opts: { instr?: string; cached?: string } = {}) => ({ commands }) =>
        commands.insertContent({
          type: FIELD_NODE,
          attrs: { kind, instr: opts.instr ?? fieldInstrFor(kind), cached: opts.cached ?? '' },
        }),

      refreshFields: (ctx: FieldContext = {}) => ({ tr, state, dispatch }) => {
        let changed = false
        state.doc.descendants((node, pos) => {
          if (node.type.name !== FIELD_NODE) return true
          const a = readFieldAttrs(node.attrs as Record<string, unknown>)
          // A body field's page number depends on where it sits: ask the
          // paginator when it can tell us, fall back to the global context.
          const page = ctx.pageAt?.(pos) ?? ctx.page
          const next = fieldText(a.kind, a.cached, { ...ctx, page }, a.instr)
          if (next !== a.cached) {
            // setNodeMarkup keeps the atom's size (1), so positions stay valid.
            tr.setNodeMarkup(pos, undefined, { ...node.attrs, cached: next })
            changed = true
          }
          return false
        })
        if (changed && dispatch) dispatch(tr)
        return true
      },
    }
  },
})

// ── Editor-level helpers (what the page calls) ─────────────────────────────────

/// Insert a field at the caret.
export function insertField(editor: Editor, kind: FieldKind, opts?: { instr?: string; cached?: string }): boolean {
  return editor.chain().focus().insertField(kind, opts).run()
}

/// « Actualiser les champs » (Word's F9): recompute every field of the document.
export function refreshFields(editor: Editor, ctx: FieldContext = {}): boolean {
  return editor.chain().refreshFields(ctx).run()
}

/// Bookmark name → page number map, ready for `FieldContext.refs`, built from
/// whatever the paginator knows about bookmark positions.
export function refsFromBookmarks(
  bookmarks: ReadonlyArray<{ name: string; pos: number }>,
  pageAt: (pos: number) => number | null | undefined,
  format?: (n: number) => string,
): Record<string, string> {
  const out: Record<string, string> = {}
  for (const b of bookmarks) {
    const p = pageAt(b.pos)
    if (p != null) out[b.name] = format ? format(p) : String(p)
  }
  return out
}
