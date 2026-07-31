// Multi-line formula editor, used when the formula bar is expanded.
//
// FormulaInput (single line) paints its coloured text on a canvas, which cannot
// wrap: an <input> never breaks lines. Here a transparent <textarea> sits on top
// of a MIRROR <div> holding the same text as coloured spans. Because the mirror
// has the textarea's exact box and typography, the browser wraps both identically
// — the caret always lands on the character you see, with no wrap algorithm of our
// own to keep in sync. Native selection stays visible (no ::selection override),
// and error runs use CSS wavy underlines instead of canvas strokes.

import { useLayoutEffect, useRef, type CSSProperties, type RefObject } from 'react'
import { colorSegments } from '../formula-refs'

const ERROR_COLOR = '#d93025'

export interface FormulaAreaProps {
  value: string
  onChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => void
  onKeyDown?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void
  onFocus?: (e: React.FocusEvent<HTMLTextAreaElement>) => void
  onBlur?: (e: React.FocusEvent<HTMLTextAreaElement>) => void
  onSelect?: (e: React.SyntheticEvent<HTMLTextAreaElement>) => void
  placeholder?: string
  areaRef?: RefObject<HTMLTextAreaElement | null>
  containerStyle?: CSSProperties
  textColor?: string
  knownFunctions?: Set<string>
  names?: Set<string>
}

// Typography shared by the textarea and its mirror — any divergence here would
// desynchronise the wrapping, so both read from this single object.
const TYPO: CSSProperties = {
  font: '14px/18px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
  padding: '4px 8px',
  whiteSpace: 'pre-wrap',
  overflowWrap: 'anywhere',
  wordBreak: 'normal',
  tabSize: 4,
  letterSpacing: 'normal',
}

export function FormulaArea({
  value, onChange, onKeyDown, onFocus, onBlur, onSelect, placeholder,
  areaRef, containerStyle, textColor = '#202124', knownFunctions, names,
}: FormulaAreaProps) {
  const localRef = useRef<HTMLTextAreaElement>(null)
  const ref = areaRef ?? localRef
  const mirrorRef = useRef<HTMLDivElement>(null)

  // Keep the mirror scrolled with the textarea so long formulas stay aligned.
  useLayoutEffect(() => {
    const area = ref.current, mirror = mirrorRef.current
    if (!area || !mirror) return
    const sync = () => { mirror.scrollTop = area.scrollTop; mirror.scrollLeft = area.scrollLeft }
    sync()
    area.addEventListener('scroll', sync)
    return () => area.removeEventListener('scroll', sync)
  }, [ref, value])

  const isFormula = value.startsWith('=')
  const segs = isFormula ? colorSegments(value, knownFunctions, names) : [{ text: value } as { text: string; color?: string; wavy?: boolean }]

  return (
    <div style={{ position: 'relative', overflow: 'hidden', ...containerStyle }}>
      <div
        ref={mirrorRef}
        aria-hidden
        style={{
          ...TYPO,
          position: 'absolute', inset: 0, overflow: 'hidden',
          pointerEvents: 'none', color: textColor, background: 'transparent',
        }}
      >
        {value === ''
          ? <span style={{ color: '#9aa0a6' }}>{placeholder}</span>
          : segs.map((s, i) => (
            <span
              key={i}
              style={{
                color: s.color ?? textColor,
                ...(s.wavy ? { textDecoration: `underline wavy ${ERROR_COLOR}`, textDecorationSkipInk: 'none' } : {}),
              }}
            >
              {s.text}
            </span>
          ))}
        {/* Trailing newline needs a filler or the mirror loses its last line box. */}
        {value.endsWith('\n') && <span>&nbsp;</span>}
      </div>
      <textarea
        ref={ref}
        value={value}
        onChange={onChange}
        onKeyDown={onKeyDown}
        onFocus={onFocus}
        onBlur={onBlur}
        onSelect={onSelect}
        placeholder={placeholder}
        spellCheck={false}
        wrap="soft"
        style={{
          ...TYPO,
          position: 'absolute', inset: 0, width: '100%', height: '100%',
          margin: 0, border: 'none', outline: 'none', resize: 'none',
          background: 'transparent',
          // The mirror shows the text; the textarea keeps the caret and selection.
          color: 'transparent',
          caretColor: textColor,
          boxSizing: 'border-box',
          overflowY: 'auto',
        }}
      />
    </div>
  )
}
