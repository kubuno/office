// In-place editing of a shape's caption — the « Modifier le texte » entry, and the
// double-click gesture on a shape.
//
// A transparent `contentEditable` overlay pinned to the shape's OWN caption box
// (`geometry.shapeTextBox`) with the very same typography `ShapeView` uses, so the
// text does not shift by a pixel when editing starts or ends. No dialog is involved:
// the user types straight inside the shape, Excel-style.
//
// Commit rules (Excel's): Escape and an outside click both KEEP the text — leaving
// text editing is not a cancellation. Enter inserts a line break, since a shape's
// caption is multi-line. The host owns the shape's selection and only has to persist
// what `onCommit` hands back.
//
// Usage — a SIBLING of the ShapeView inside the host's shape overlay:
//   <ShapeView … hideText />
//   <ShapeTextEditor kind={s.kind} width={r.w} height={r.h} value={s.text ?? ''}
//                    textStyle={s.textStyle} textScale={zoom} onCommit={…} />

import { useCallback, useEffect, useLayoutEffect, useRef } from 'react'
import type { SheetShapeKind, SheetShapeTextStyle } from '../api'
import { shapeGeometry, asNative} from './geometry'
import { shapeTextCss, shapeTextJustify } from './ShapeView'

export interface ShapeTextEditorProps {
  kind: SheetShapeKind
  /** Rendered size of the shape in px — the same values `ShapeView` gets. */
  width: number
  height: number
  /** Outline width in px before zoom, so the caption box matches the glyph's. */
  borderWidth?: number
  /** Multiplies the outline width (the host's zoom). */
  strokeScale?: number
  value: string
  textStyle?: SheetShapeTextStyle
  /** Multiplies the caption's font size (the host's zoom). */
  textScale?: number
  /** Called with the final text on Escape, on an outside click, or on unmount. */
  onCommit: (text: string) => void
}

/**
 * Plain text of the editable node: `innerText` already turns the browser's
 * <div>/<br> soup into newlines; the non-breaking spaces contentEditable inserts
 * while typing are normalised back to plain ones, and the trailing newline some
 * browsers append is dropped.
 */
function readText(el: HTMLElement): string {
  return el.innerText.replace(/\u00a0/g, ' ').replace(/\n$/, '')
}

export function ShapeTextEditor({
  kind, width, height, borderWidth, strokeScale,
  value, textStyle, textScale, onCommit,
}: ShapeTextEditorProps) {
  const boxRef = useRef<HTMLDivElement>(null)
  const editRef = useRef<HTMLDivElement>(null)
  // Guard so the text is written back exactly once, whichever exit path fires.
  const committed = useRef(false)
  // Kept in a ref so the effects below never need `onCommit` in their deps (a new
  // closure each render would otherwise re-arm the listeners and steal the caret).
  const commitRef = useRef(onCommit)
  commitRef.current = onCommit

  const commit = useCallback(() => {
    if (committed.current) return
    committed.current = true
    const el = editRef.current
    commitRef.current(el ? readText(el) : value)
  }, [value])

  // Seed the text ONCE and place the caret at the end. The node is deliberately not
  // React-controlled: re-rendering its children on every keystroke would reset the
  // selection on each input event.
  useLayoutEffect(() => {
    const el = editRef.current
    if (!el) return
    el.innerText = value
    el.focus()
    const sel = window.getSelection()
    if (sel) {
      const range = document.createRange()
      range.selectNodeContents(el)
      range.collapse(false)
      sel.removeAllRanges()
      sel.addRange(range)
    }
    // Mount-only: `value` is the initial text, later keystrokes live in the DOM.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Outside click and Escape both commit. Both listeners are captured on `document`
  // so they win over the grid's own mousedown handling, and Escape is stopped from
  // travelling on (the sheet would otherwise clear the selection at the same time).
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (boxRef.current?.contains(e.target as Node)) return
      commit()
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      commit()
    }
    document.addEventListener('mousedown', onDown, true)
    document.addEventListener('keydown', onKey, true)
    return () => {
      document.removeEventListener('mousedown', onDown, true)
      document.removeEventListener('keydown', onKey, true)
    }
  }, [commit])

  // Unmounted from the outside (shape deleted, sheet switched): keep the typing.
  useEffect(() => () => { commit() }, [commit])

  const sw = Math.max(0, borderWidth ?? 1) * (strokeScale ?? 1)
  const box = shapeGeometry(asNative(kind), width, height, sw / 2).text

  return (
    <div
      ref={boxRef}
      data-kubuno-floating=""
      style={{
        position: 'absolute',
        left: box.x, top: box.y, width: box.w, height: box.h,
        display: 'flex',
        alignItems: 'center',
        justifyContent: shapeTextJustify(textStyle),
        // The grid must not treat clicks in here as cell selection.
        cursor: 'text',
      }}
      onMouseDown={e => e.stopPropagation()}
      onClick={e => e.stopPropagation()}
      onDoubleClick={e => e.stopPropagation()}
      onContextMenu={e => e.stopPropagation()}
    >
      <div
        ref={editRef}
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        aria-multiline="true"
        spellCheck={false}
        // Plain text only: a paste from a rich source must not smuggle markup in.
        onPaste={e => {
          e.preventDefault()
          const plain = e.clipboardData.getData('text/plain')
          if (plain) document.execCommand('insertText', false, plain)
        }}
        // Enter is a line break inside the caption, never a commit.
        onKeyDown={e => { if (e.key === 'Enter') e.stopPropagation() }}
        style={{
          width: '100%',
          outline: 'none',
          background: 'transparent',
          caretColor: 'currentColor',
          ...shapeTextCss(textStyle, textScale ?? 1),
        }}
      />
    </div>
  )
}
