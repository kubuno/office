// Review pane (Word's « Volet Vérifications ») — the list of tracked changes.
//
// One card per change: who made it, when, what kind (inserted / deleted), an
// excerpt of the text, and the two actions (accept / reject). The header carries
// the change count plus "accept all" / "reject all".
//
// Layout follows the two side panes of `DocumentEditorPage.tsx`: the same left
// column as `NavPane` (fixed width, right border, own scroller) and the same
// card look as the comment gutter (rounded box, 1px border, subtle background,
// author colour). Opening/closing is a plain boolean owned by the page, exactly
// like `navOpen` / `commentsOpen`.
//
// The model lives in `track-changes.ts` (marks `insertion` / `deletion` carrying
// author / authorId / date / id). This file only reads it and calls its commands.

import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { Editor } from '@tiptap/core'
import { useConfirm } from '@kubuno/sdk'
import { ConfirmDialog, MenuDropdown } from '@ui'
import type { MenuItem } from '@ui'
import { Check, CheckCheck, X, Undo2, ListChecks } from 'lucide-react'
import {
  documentChanges, changeColor,
  acceptChange, rejectChange, acceptAll, rejectAll,
  type TrackedChange,
} from './track-changes'

export type { TrackedChange }

/** Width of the pane — the comment cards' width (268) plus the card gutter. */
export const REVIEW_PANE_W = 288

/** `#rrggbb` → `rgba(r,g,b,a)`, for the card's background tint. */
function withAlpha(hex: string, a: number): string {
  let s = (hex || '').replace('#', '')
  if (s.length === 3) s = s.split('').map(c => c + c).join('')
  if (s.length !== 6) return `rgba(0,0,0,${a})`
  const n = parseInt(s, 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`
}

/** Relative date, same wording as the comment cards. */
function relDate(iso: string, t: (k: string, o?: Record<string, unknown>) => string): string {
  const ts = Date.parse(iso)
  if (!Number.isFinite(ts)) return ''
  const s = Math.max(0, Math.floor((Date.now() - ts) / 1000))
  if (s < 60) return t('doc_time_now', { defaultValue: "à l'instant" })
  const m = Math.floor(s / 60); if (m < 60) return t('doc_time_min', { defaultValue: '{{n}} min', n: m })
  const h = Math.floor(m / 60); if (h < 24) return t('doc_time_hour', { defaultValue: '{{n}} h', n: h })
  return new Date(ts).toLocaleDateString()
}

export interface ReviewPaneProps {
  /** Active editor — source of the change list and target of the commands. */
  editor: Editor | null
  /** Closes the pane (the page owns the boolean). */
  onClose: () => void
  /** Card click: the page scrolls the canvas to `change.from`. */
  onScrollToChange?: (change: TrackedChange) => void
  /** Highlighted change, if the page tracks one (e.g. the caret's). */
  activeId?: string | null
  /** Card click, before scrolling. */
  onActivate?: (id: string) => void
  /** Optional override of the list — otherwise read from the editor. */
  changes?: TrackedChange[]
}

export function ReviewPane({ editor, onClose, onScrollToChange, activeId = null, onActivate, changes: override }: ReviewPaneProps) {
  const { t } = useTranslation('office')
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [version, setVersion] = useState(0)
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null)
  const bump = useCallback(() => setVersion(v => (v + 1) & 0xffff), [])

  // The list is derived from the document: recompute on every editor update
  // (accepting/rejecting dispatches a transaction, so this covers our own
  // commands as well as remote collaborative edits).
  useEffect(() => {
    if (!editor) return
    editor.on('update', bump)
    return () => { editor.off('update', bump) }
  }, [editor, bump])

  const items = useMemo<TrackedChange[]>(() => {
    if (override) return override
    void version
    if (!editor) return []
    return documentChanges(editor.state.doc)
  }, [editor, version, override])

  const accept = useCallback((id: string) => { acceptChange(editor, id); bump() }, [editor, bump])
  const reject = useCallback((id: string) => { rejectChange(editor, id); bump() }, [editor, bump])
  const doAcceptAll = useCallback(() => { acceptAll(editor); bump() }, [editor, bump])

  // Rejecting everything throws away every contribution at once: confirm first
  // (project rule — never the browser's own confirm()).
  const doRejectAll = useCallback(async () => {
    const ok = await confirm({
      title:        t('doc_review_reject_all_title', { defaultValue: 'Refuser toutes les modifications' }),
      message:      t('doc_review_reject_all_msg', { defaultValue: 'Toutes les modifications suivies seront annulées. Cette action peut être annulée avec Ctrl+Z.' }),
      confirmLabel: t('doc_review_reject_all', { defaultValue: 'Tout refuser' }),
      variant:      'danger',
    })
    if (!ok) return
    rejectAll(editor); bump()
  }, [confirm, editor, t, bump])

  const openCard = useCallback((c: TrackedChange) => {
    onActivate?.(c.id)
    onScrollToChange?.(c)
  }, [onActivate, onScrollToChange])

  const menuItems: MenuItem[] = menu ? [
    { type: 'action', label: t('doc_review_accept', { defaultValue: 'Accepter' }), icon: <Check size={13} />, onClick: () => accept(menu.id) },
    { type: 'action', label: t('doc_review_reject', { defaultValue: 'Refuser' }), icon: <Undo2 size={13} />, onClick: () => reject(menu.id) },
    { type: 'separator' },
    { type: 'action', label: t('doc_review_accept_all', { defaultValue: 'Tout accepter' }), onClick: doAcceptAll },
    { type: 'action', label: t('doc_review_reject_all', { defaultValue: 'Tout refuser' }), danger: true, onClick: () => { void doRejectAll() } },
  ] : []

  return (
    <div className="flex-shrink-0 border-r border-border bg-white flex flex-col min-h-0" style={{ width: REVIEW_PANE_W }}>
      {/* Header: title + count, then the two bulk actions. */}
      <div className="px-3 py-2 border-b border-border">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold text-text-primary flex items-center gap-1.5">
            <ListChecks size={13} />
            {t('doc_review_pane', { defaultValue: 'Volet Vérifications' })}
          </span>
          <button onClick={onClose} className="p-1 rounded hover:bg-surface-2 text-text-secondary"
            title={t('common_close', { defaultValue: 'Fermer' })}><X size={13} /></button>
        </div>
        <div className="mt-0.5 text-[11px] text-text-tertiary">
          {t('doc_review_count', { defaultValue: '{{n}} modification(s)', n: items.length })}
        </div>
        <div className="mt-2 flex items-center gap-1.5">
          <button onClick={doAcceptAll} disabled={!items.length}
            className="flex-1 flex items-center justify-center gap-1 h-7 px-2 rounded border border-border text-[11px] text-text-primary hover:bg-surface-2 disabled:opacity-40 disabled:hover:bg-transparent">
            <CheckCheck size={12} /> {t('doc_review_accept_all', { defaultValue: 'Tout accepter' })}
          </button>
          <button onClick={() => { void doRejectAll() }} disabled={!items.length}
            className="flex-1 flex items-center justify-center gap-1 h-7 px-2 rounded border border-border text-[11px] text-danger hover:bg-surface-2 disabled:opacity-40 disabled:hover:bg-transparent">
            <Undo2 size={12} /> {t('doc_review_reject_all', { defaultValue: 'Tout refuser' })}
          </button>
        </div>
      </div>

      {/* Cards */}
      <div className="flex-1 overflow-y-auto p-2 flex flex-col gap-1.5">
        {items.length === 0 && (
          <p className="px-1 py-3 text-xs text-text-tertiary leading-snug">
            {t('doc_review_empty', { defaultValue: 'Aucune modification suivie dans ce document.' })}
          </p>
        )}
        {items.map(c => {
          const col = changeColor(c)
          const active = activeId === c.id
          return (
            <div key={c.id}
              onClick={() => openCard(c)}
              onContextMenu={e => { e.preventDefault(); e.stopPropagation(); openCard(c); setMenu({ id: c.id, x: e.clientX, y: e.clientY }) }}
              className="rounded cursor-pointer transition-colors px-2.5 py-2"
              style={{
                // Background tint carries the author colour — no left accent bar.
                background: withAlpha(col, active ? 0.18 : 0.07),
                border: `1px solid ${active ? col : withAlpha(col, 0.28)}`,
              }}>
              <div className="flex items-center gap-1.5 min-w-0">
                <span className="rounded-full flex-shrink-0" style={{ width: 8, height: 8, background: col }} />
                <span className="text-xs text-text-primary truncate min-w-0 flex-1">
                  {c.author || t('doc_review_unknown_author', { defaultValue: 'Auteur inconnu' })}
                </span>
                <span className="text-[10px] text-text-tertiary flex-shrink-0">{relDate(c.date, t)}</span>
              </div>
              <div className="mt-1 text-[11px]" style={{ color: col }}>
                {c.kind === 'insertion'
                  ? t('doc_review_inserted', { defaultValue: 'Inséré' })
                  : t('doc_review_deleted', { defaultValue: 'Supprimé' })}
              </div>
              {c.text
                ? <p className="mt-0.5 text-xs text-text-secondary break-words line-clamp-3"
                    style={{ textDecoration: c.kind === 'deletion' ? 'line-through' : 'none' }}>{c.text}</p>
                // A change with no text (e.g. a paragraph mark) still needs a card.
                : <p className="mt-0.5 text-xs text-text-tertiary italic">{t('doc_review_no_text', { defaultValue: '(marque de paragraphe)' })}</p>}
              <div className="mt-1.5 flex items-center gap-1" onClick={e => e.stopPropagation()}>
                <button onClick={() => accept(c.id)}
                  className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded text-text-secondary hover:bg-surface-2">
                  <Check size={12} /> {t('doc_review_accept', { defaultValue: 'Accepter' })}
                </button>
                <button onClick={() => reject(c.id)}
                  className="flex items-center gap-1 text-[11px] px-1.5 py-1 rounded text-text-secondary hover:bg-surface-2">
                  <Undo2 size={12} /> {t('doc_review_reject', { defaultValue: 'Refuser' })}
                </button>
              </div>
            </div>
          )
        })}
      </div>

      {menu && <MenuDropdown items={menuItems} pos={{ top: menu.y, left: menu.x, minWidth: 200 }} onClose={() => setMenu(null)} />}
      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}

export default ReviewPane
