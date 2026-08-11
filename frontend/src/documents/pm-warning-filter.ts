// One-shot suppressor for a benign ProseMirror console warning.
//
// prosemirror-state logs, exactly ONCE per page load (guarded by its module-level
// `warnedAboutTextSelection` flag):
//   "TextSelection endpoint not pointing into a node with inline content (table)"
// y-prosemirror emits it while BUILDING the collaborative editor: the editor is
// mounted empty (its content comes from the Y.Doc), so its initial caret sits at
// pos 1; when the first Yjs sync replaces the doc with one that STARTS with a
// table, that pos maps into the table node, which has no inline content.
//
// The functional consequence is already handled (`normalizeSelection` re-seats the
// caret into the first cell). Only the warning remains, and it fires during editor
// construction — before any React effect can run — so it cannot be prevented from
// our code without patching y-prosemirror. We therefore drop this ONE message.
//
// The filter self-uninstalls the moment it drops the message: ProseMirror only
// ever warns once, so afterwards `console.warn` is restored untouched.
const PREFIX = 'TextSelection endpoint not pointing into a node with inline content'

let installed = false

export function suppressTableSelectionWarning(): void {
  if (installed || typeof console === 'undefined' || typeof console.warn !== 'function') return
  installed = true
  const original = console.warn.bind(console)
  console.warn = (...args: unknown[]) => {
    if (typeof args[0] === 'string' && args[0].startsWith(PREFIX)) {
      console.warn = original // one-shot: restore immediately, PM never warns again
      return
    }
    original(...args)
  }
}
