import { useRef, useLayoutEffect, useEffect, useCallback, type CSSProperties, type RefObject } from 'react'
import { colorSegments } from './formula-refs'
import { matchParen } from './formula-edit'

// Zone de saisie de formule à RENDU CANVAS. Le texte (références colorées,
// parenthèses arc-en-ciel, soulignés d'erreur ondulés, surbrillance de sélection)
// est dessiné au pixel près sur un <canvas>. Un <input> TRANSPARENT superposé
// capte toute la saisie native (caret, sélection, accents/IME, presse-papiers,
// défilement horizontal) — on garde donc l'édition irréprochable tout en ouvrant
// la surface canvas à des rendus arbitraires. Réutilisable : barre ET cellule.

const TYPO: (keyof CSSProperties)[] = [
  'fontSize', 'fontFamily', 'fontWeight', 'fontStyle', 'letterSpacing',
  'lineHeight', 'padding', 'paddingLeft', 'paddingRight', 'paddingTop', 'paddingBottom',
  'textAlign', 'textIndent',
]

// Le caret natif et le presse-papiers restent gérés par l'<input> ; en revanche
// on masque sa SÉLECTION et son PLACEHOLDER natifs (on les redessine sur le canvas).
const STYLE_ID = 'kbformula-canvas-style'
function ensureStyle() {
  if (typeof document === 'undefined' || document.getElementById(STYLE_ID)) return
  const el = document.createElement('style')
  el.id = STYLE_ID
  el.textContent = '.kbformula-input::selection{background:transparent}.kbformula-input::placeholder{color:transparent}'
  document.head.appendChild(el)
}

const SEL_COLOR    = 'rgba(160,195,255,0.55)'
const ERROR_COLOR  = '#d93025'
const PLACEHOLDER  = '#9aa0a6'

interface Props {
  value:          string
  onChange:       (e: React.ChangeEvent<HTMLInputElement>) => void
  onKeyDown?:     (e: React.KeyboardEvent<HTMLInputElement>) => void
  onFocus?:       (e: React.FocusEvent<HTMLInputElement>) => void
  onBlur?:        (e: React.FocusEvent<HTMLInputElement>) => void
  onSelect?:      (e: React.SyntheticEvent<HTMLInputElement>) => void
  placeholder?:   string
  inputRef?:      RefObject<HTMLInputElement | null>
  inputStyle:     CSSProperties      // style appliqué à l'input ; le canvas copie la typo
  containerStyle?: CSSProperties
  textColor?:     string            // couleur du texte non-référence (défaut #202124)
  autoFocus?:     boolean
  knownFunctions?: Set<string>      // noms de fonctions valides → souligne les fautes
  names?:          Set<string>      // plages nommées (MAJ) → colorées dans le texte
}

export function FormulaInput({
  value, onChange, onKeyDown, onFocus, onBlur, onSelect,
  placeholder, inputRef, inputStyle, containerStyle, textColor = '#202124', autoFocus, knownFunctions, names,
}: Props) {
  const localRef = useRef<HTMLInputElement>(null)
  const ref = inputRef ?? localRef
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(ensureStyle, [])

  // Dessine le contenu courant sur le canvas (texte coloré + sélection + erreurs).
  const draw = useCallback(() => {
    const canvas = canvasRef.current, input = ref.current, container = containerRef.current
    if (!canvas || !input || !container) return
    const rect = container.getBoundingClientRect()
    const w = Math.max(1, rect.width), h = Math.max(1, rect.height)
    const dpr = window.devicePixelRatio || 1
    const bw = Math.round(w * dpr), bh = Math.round(h * dpr)
    if (canvas.width !== bw)  { canvas.width = bw;  canvas.style.width  = `${w}px` }
    if (canvas.height !== bh) { canvas.height = bh; canvas.style.height = `${h}px` }
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, w, h)

    const cs = getComputedStyle(input)
    ctx.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize}/${cs.lineHeight} ${cs.fontFamily}`
    ctx.textBaseline = 'middle'
    const padLeft = parseFloat(cs.paddingLeft) || 0
    const baseY = h / 2
    const scroll = input.scrollLeft
    const x0 = padLeft - scroll
    const wAt = (s: string) => ctx.measureText(s).width

    // Placeholder (champ vide).
    if (value === '') {
      if (placeholder) { ctx.fillStyle = PLACEHOLDER; ctx.fillText(placeholder, padLeft, baseY) }
      return
    }

    // Surbrillance de sélection (dessinée SOUS le texte) quand l'input a le focus.
    const focused = document.activeElement === input
    const selA = input.selectionStart ?? 0, selB = input.selectionEnd ?? 0
    if (focused && selB > selA) {
      const xa = x0 + wAt(value.slice(0, selA))
      const xb = x0 + wAt(value.slice(0, selB))
      ctx.fillStyle = SEL_COLOR
      ctx.fillRect(xa, 1, xb - xa, h - 2)
    }

    // Surbrillance de la paire de parenthèses entourant le caret (façon IDE).
    if (focused && value.startsWith('=')) {
      const mp = matchParen(value, input.selectionStart ?? 0)
      if (mp) {
        ctx.fillStyle = 'rgba(26,115,232,0.18)'
        for (const i of [mp.a, mp.b]) {
          const px = x0 + wAt(value.slice(0, i)), pw = wAt(value[i])
          ctx.fillRect(px - 0.5, 2, pw + 1, h - 4)
        }
      }
    }

    // Texte coloré, segment par segment.
    const isFormula = value.startsWith('=')
    const segs = isFormula ? colorSegments(value, knownFunctions, names) : [{ text: value }]
    let x = x0
    for (const s of segs) {
      const segW = wAt(s.text)
      ctx.fillStyle = s.color ?? textColor
      ctx.fillText(s.text, x, baseY)
      if (s.wavy) drawWavy(ctx, x, baseY, segW)
      x += segW
    }
  }, [value, placeholder, textColor, knownFunctions, names, ref])

  // Redessine à chaque changement de valeur + au resize de la fenêtre.
  useLayoutEffect(() => { draw() })
  useEffect(() => {
    const onResize = () => draw()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [draw])

  return (
    <div ref={containerRef} style={{ position: 'relative', overflow: 'hidden', ...containerStyle }}>
      <canvas ref={canvasRef}
        style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }} />
      <input
        ref={ref}
        className="kbformula-input"
        value={value}
        onChange={onChange}
        onKeyDown={onKeyDown}
        onKeyUp={draw}
        onClick={draw}
        onFocus={e => { draw(); onFocus?.(e) }}
        onBlur={e => { draw(); onBlur?.(e) }}
        onSelect={e => { draw(); onSelect?.(e) }}
        onScroll={draw}
        placeholder={placeholder}
        autoFocus={autoFocus}
        style={{
          ...inputStyle,
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          background: 'transparent',
          // Texte transparent (le canvas l'affiche), mais caret natif visible.
          color: 'transparent',
          caretColor: textColor,
        }}
      />
    </div>
  )
}

// Souligné ondulé (façon correcteur) sous une portion en erreur.
function drawWavy(ctx: CanvasRenderingContext2D, x: number, baseY: number, width: number) {
  const fontPx = parseFloat((ctx.font.match(/(\d+(?:\.\d+)?)px/) || [])[1] || '13')
  const y = baseY + fontPx / 2 + 1.5
  const amp = 1.4, half = 2
  ctx.save()
  ctx.strokeStyle = ERROR_COLOR
  ctx.lineWidth = 1
  ctx.beginPath()
  for (let i = 0, k = 0; i <= width; i += half, k++) {
    const yy = y + (k % 2 === 0 ? -amp : amp)
    if (i === 0) ctx.moveTo(x, yy); else ctx.lineTo(x + i, yy)
  }
  ctx.stroke()
  ctx.restore()
}
