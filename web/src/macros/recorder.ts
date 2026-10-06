// Macro RECORDER — Word's « Enregistrer une macro ».
//
// Word records COMMANDS, not keystrokes: typing a word produces one insertion,
// and pressing Bold produces one command. The same idea here: we listen to the
// editor's transactions and translate them into calls of the `Kubuno.Doc` API,
// so a recorded macro is ordinary, readable, re-runnable code.
//
// Deliberate scope: what the Doc API can REPLAY — typed text, deletions, and the
// bold/italic/underline/alignment commands. Anything else is skipped rather than
// recorded wrongly: a macro that silently does something different from what the
// user did would be worse than one that does less.
import type { Editor } from '@tiptap/react'
import type { Transaction } from '@tiptap/pm/state'
import { ReplaceStep, ReplaceAroundStep, AddMarkStep, RemoveMarkStep } from '@tiptap/pm/transform'

export type RecorderState = 'idle' | 'recording' | 'paused'

/** Marks we know how to replay, mapped to their API call. */
const MARK_CALLS: Record<string, string> = {
  bold: 'setBold',
  italic: 'setItalic',
  underline: 'setUnderline',
  strike: 'setStrike',
}

interface Line { code: string }

let _state: RecorderState = 'idle'
let _lines: Line[] = []
let _textBuffer = ''
let _editor: Editor | null = null
let _handler: ((props: { transaction: Transaction }) => void) | null = null
let _listeners: Array<(s: RecorderState) => void> = []

const notify = () => { for (const fn of _listeners) fn(_state) }

export function onRecorderState(fn: (s: RecorderState) => void): () => void {
  _listeners.push(fn)
  return () => { _listeners = _listeners.filter(x => x !== fn) }
}

export function recorderState(): RecorderState { return _state }

const esc = (s: string) => JSON.stringify(s)

/** Flush the pending typing run into a single `insertText` call, as Word does. */
function flushText(): void {
  if (!_textBuffer) return
  _lines.push({ code: `Kubuno.Doc.insertText(${esc(_textBuffer)})` })
  _textBuffer = ''
}

function record(tr: Transaction): void {
  if (_state !== 'recording') return
  for (const step of tr.steps) {
    if (step instanceof AddMarkStep || step instanceof RemoveMarkStep) {
      const call = MARK_CALLS[step.mark.type.name]
      if (!call) continue
      flushText()
      _lines.push({ code: `Kubuno.Doc.${call}(${step instanceof AddMarkStep})` })
      continue
    }
    if (step instanceof ReplaceStep || step instanceof ReplaceAroundStep) {
      const slice = step.slice
      const text = slice.content.textBetween(0, slice.content.size, '\n')
      const removed = (step.to as number) - (step.from as number)
      if (text && slice.content.childCount <= 2) {
        _textBuffer += text
      } else if (!text && removed > 0) {
        flushText()
        _lines.push({ code: `Kubuno.Doc.deleteBackward(${removed})` })
      }
    }
  }
}

/** Begin recording on `editor`. Any previous take is discarded. */
export function startRecording(editor: Editor | null | undefined): void {
  if (!editor) return
  stopListening()
  _lines = []
  _textBuffer = ''
  _editor = editor
  _handler = ({ transaction }) => { if (transaction.docChanged || transaction.steps.length) record(transaction) }
  editor.on('transaction', _handler)
  _state = 'recording'
  notify()
}

export function pauseRecording(): void {
  if (_state !== 'recording') return
  flushText()
  _state = 'paused'
  notify()
}

export function resumeRecording(): void {
  if (_state !== 'paused') return
  _state = 'recording'
  notify()
}

function stopListening(): void {
  if (_editor && _handler) _editor.off('transaction', _handler)
  _editor = null
  _handler = null
}

/**
 * End the take and return the generated source. An empty take still returns a
 * valid (commented) macro rather than nothing, so the user never ends up with a
 * macro that fails to open.
 */
export function stopRecording(name: string): string {
  flushText()
  stopListening()
  _state = 'idle'
  notify()
  const body = _lines.length
    ? _lines.map(l => l.code).join('\n')
    : '// (aucune action enregistrée)'
  return `// Macro « ${name} » — enregistrée le ${new Date().toLocaleString()}\n${body}\n`
}

/** Number of commands captured so far (shown while recording). */
export function recordedCount(): number {
  return _lines.length + (_textBuffer ? 1 : 0)
}
