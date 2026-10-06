// Endnote editor — the UI half of `endnotes.ts`.
//
// Same window as the footnote editor of `DocumentEditorPage.tsx` (a small
// `FloatingWindow` with a textarea, "Supprimer la note" on the left and
// OK / Annuler on the right), so the two kinds of notes feel identical. It is
// packaged as a HOOK returning both the opener and the rendered window: the
// editor page only wires two lines instead of holding one more piece of state.

import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { Editor } from '@tiptap/core'
import { Button, FloatingWindow } from '@ui'
import { BookOpen } from 'lucide-react'
import { DLG_BTN } from '../lib'
import { deleteEndnote, documentEndnotes, endnoteTextAt, setEndnoteText } from './endnotes'

/// Any ref holding the editor — accepts both a mutable and a readonly ref.
type EditorRef = { readonly current: Editor | null }

export interface EndnoteEditor {
  /** Opens the editor on the endnote at `pos`; no-op if it is not one. */
  openEndnote: (pos: number) => void
  /** Render this in the page: it is null while no endnote is being edited. */
  endnoteDialog: React.ReactNode
}

export function useEndnoteEditor(editorRef: EditorRef): EndnoteEditor {
  const { t } = useTranslation('office')
  // Endnote being edited: PM position of the call + the text being typed.
  const [dlg, setDlg] = useState<{ pos: number; text: string } | null>(null)

  const openEndnote = useCallback((pos: number) => {
    const text = endnoteTextAt(editorRef.current, pos)
    if (text != null) setDlg({ pos, text })
  }, [editorRef])

  let endnoteDialog: React.ReactNode = null
  if (dlg) {
    // Rank shown in the title so the note matches its call on the page (roman).
    const marker = documentEndnotes(editorRef.current?.state.doc).find(e => e.pos === dlg.pos)?.marker
    const title = t('doc_endnote', { defaultValue: 'Note de fin' })
    endnoteDialog = (
      <FloatingWindow
        title={marker ? `${title} — ${marker}` : title}
        icon={<BookOpen size={16} />}
        onClose={() => setDlg(null)}
        defaultWidth={440}
        backdrop
      >
        <div className="p-4 flex flex-col gap-3" data-module="office">
          <textarea
            autoFocus
            className="w-full h-28 border border-border rounded-lg p-2 text-xs resize-none focus:outline-none focus:border-accent bg-surface text-text-primary"
            placeholder={t('doc_endnote_placeholder', { defaultValue: 'Texte de la note…' })}
            value={dlg.text}
            onChange={e => setDlg({ ...dlg, text: e.target.value })}
          />
          <div className="flex justify-between">
            <Button variant="secondary" onClick={() => { deleteEndnote(editorRef.current, dlg.pos); setDlg(null) }}>
              {t('doc_endnote_delete', { defaultValue: 'Supprimer la note' })}
            </Button>
            <div className="flex gap-2">
              <Button className={DLG_BTN} onClick={() => { setEndnoteText(editorRef.current, dlg.pos, dlg.text); setDlg(null) }}>
                {t('common_ok', { defaultValue: 'OK' })}
              </Button>
              <Button className={DLG_BTN} variant="secondary" onClick={() => setDlg(null)}>
                {t('common_cancel', { defaultValue: 'Annuler' })}
              </Button>
            </div>
          </div>
        </div>
      </FloatingWindow>
    )
  }

  return { openEndnote, endnoteDialog }
}
