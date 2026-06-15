import { useRef, useLayoutEffect, type CSSProperties, type RefObject } from 'react'
import { colorSegments } from './formula-refs'

// Input de formule avec coloration des références (technique « miroir ») : une
// couche colorée rend le texte (réfs colorées) derrière un <input> au texte
// transparent mais au caret visible. Réutilisable : barre de formule ET cellule.

const TYPO: (keyof CSSProperties)[] = [
  'fontSize', 'fontFamily', 'fontWeight', 'fontStyle', 'letterSpacing',
  'lineHeight', 'padding', 'paddingLeft', 'paddingRight', 'paddingTop', 'paddingBottom',
  'textAlign', 'textIndent',
]

interface Props {
  value:          string
  onChange:       (e: React.ChangeEvent<HTMLInputElement>) => void
  onKeyDown?:     (e: React.KeyboardEvent<HTMLInputElement>) => void
  onFocus?:       (e: React.FocusEvent<HTMLInputElement>) => void
  onBlur?:        (e: React.FocusEvent<HTMLInputElement>) => void
  onSelect?:      (e: React.SyntheticEvent<HTMLInputElement>) => void
  placeholder?:   string
  inputRef?:      RefObject<HTMLInputElement | null>
  inputStyle:     CSSProperties      // style appliqué à l'input ; la couche miroir copie la typo
  containerStyle?: CSSProperties
  textColor?:     string            // couleur du texte non-référence (défaut #202124)
  autoFocus?:     boolean
}

export function FormulaInput({
  value, onChange, onKeyDown, onFocus, onBlur, onSelect,
  placeholder, inputRef, inputStyle, containerStyle, textColor = '#202124', autoFocus,
}: Props) {
  const localRef = useRef<HTMLInputElement>(null)
  const ref = inputRef ?? localRef
  const mirrorRef = useRef<HTMLDivElement>(null)

  // Le miroir suit le défilement horizontal de l'input (texte long).
  const sync = () => { if (mirrorRef.current && ref.current) mirrorRef.current.scrollLeft = ref.current.scrollLeft }
  useLayoutEffect(sync)

  const typo: CSSProperties = {}
  for (const k of TYPO) { const v = inputStyle[k]; if (v != null) (typo as Record<string, unknown>)[k] = v }

  const isFormula = value.startsWith('=')
  const segs = isFormula ? colorSegments(value) : null

  return (
    <div style={{ position: 'relative', overflow: 'hidden', ...containerStyle }}>
      <div
        ref={mirrorRef}
        aria-hidden
        style={{
          position: 'absolute', inset: 0, ...typo,
          whiteSpace: 'pre', overflow: 'hidden', pointerEvents: 'none',
          display: 'flex', alignItems: 'center', boxSizing: 'border-box',
          color: textColor,
        }}
      >
        <span style={{ whiteSpace: 'pre' }}>
          {segs
            ? segs.map((s, i) => <span key={i} style={{ color: s.color ?? textColor }}>{s.text}</span>)
            : value}
        </span>
      </div>
      <input
        ref={ref}
        value={value}
        onChange={onChange}
        onKeyDown={onKeyDown}
        onFocus={onFocus}
        onBlur={onBlur}
        onSelect={onSelect}
        onScroll={sync}
        placeholder={placeholder}
        autoFocus={autoFocus}
        style={{
          ...inputStyle,
          position: 'relative',
          background: 'transparent',
          // Texte transparent (le miroir l'affiche), mais caret + sélection visibles.
          color: 'transparent',
          caretColor: textColor,
        }}
      />
    </div>
  )
}
