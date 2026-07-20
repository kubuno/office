// Generic « Presse-papiers » ribbon group (Word-style) for Office sub-editors that
// have no bespoke clipboard model of their own (text/data editors). It operates on the
// currently-focused text field. This works because ribbon buttons preventDefault on
// mousedown, so the editing field keeps focus/selection when a button is clicked.
import { ClipboardPaste, Scissors, Copy } from 'lucide-react'
import type { RibbonGroup } from './types'

// Paste at the caret of the focused input/textarea (Clipboard API), else execCommand.
async function pasteIntoFocused(): Promise<void> {
  const el = document.activeElement as HTMLElement | null
  if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {
    try {
      const text = await navigator.clipboard.readText()
      const input = el as HTMLInputElement | HTMLTextAreaElement
      const s = input.selectionStart ?? input.value.length
      const e = input.selectionEnd ?? input.value.length
      input.setRangeText(text, s, e, 'end')
      input.dispatchEvent(new Event('input', { bubbles: true }))
      return
    } catch { /* fall through to execCommand */ }
  }
  document.execCommand('paste')
}

// Same layout/labels as the Document group, WITHOUT « Effacer la mise en forme ».
export function genericClipboardGroup(t: (k: string, o?: Record<string, unknown>) => string): RibbonGroup {
  return {
    id: 'clip', label: t('doc_grp_clipboard', { defaultValue: 'Presse-papiers' }),
    items: [
      { id: 'paste', kind: 'button', size: 'large', icon: <ClipboardPaste size={18} />, label: t('common_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', onClick: () => { void pasteIntoFocused() } },
      { id: 'cut', kind: 'button', icon: <Scissors size={15} />, label: t('common_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', onClick: () => document.execCommand('cut') },
      { id: 'copy', kind: 'button', icon: <Copy size={15} />, label: t('common_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', onClick: () => document.execCommand('copy') },
    ],
  }
}
