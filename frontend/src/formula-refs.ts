// Analyse des RÉFÉRENCES (cellules/plages) dans une formule en cours d'édition,
// pour les coloriser dans le texte ET encadrer les plages sur la grille (façon
// Excel/Google Sheets). Partagé par la barre de formule et l'édition in-cell.

export interface FormulaRef {
  start:      number   // index de début dans la formule
  end:        number   // index de fin (exclu)
  text:       string   // texte brut (ex. "C7:C16", "$A$1")
  normalized: string   // clé d'unicité (majuscules, sans $) → même réf = même couleur
  color:      string
}

// Palette distincte et lisible (≠ couleurs de sélection de la grille).
export const REF_PALETTE = [
  '#1a73e8', '#d93025', '#188038', '#9334e6',
  '#e8710a', '#12b5cb', '#c5221f', '#7cb342',
]

const REF_RE = /\$?[A-Za-z]{1,2}\$?\d{1,4}(?::\$?[A-Za-z]{1,2}\$?\d{1,4})?/g

// Repère les plages couvertes par des chaînes "..." pour les ignorer.
function stringMask(formula: string): boolean[] {
  const mask = new Array(formula.length).fill(false)
  let inStr = false
  for (let i = 0; i < formula.length; i++) {
    if (formula[i] === '"') { inStr = !inStr; mask[i] = true; continue }
    mask[i] = inStr
  }
  return mask
}

/**
 * Extrait les références d'une formule (commençant par `=`) avec une couleur stable
 * par référence distincte (1ʳᵉ apparition = 1ʳᵉ couleur de la palette, etc.).
 */
export function parseRefs(formula: string): FormulaRef[] {
  if (!formula.startsWith('=')) return []
  const mask = stringMask(formula)
  const out: FormulaRef[] = []
  const colorByNorm = new Map<string, string>()
  let m: RegExpExecArray | null
  REF_RE.lastIndex = 0
  while ((m = REF_RE.exec(formula)) !== null) {
    const start = m.index
    const end = start + m[0].length
    if (mask[start]) continue                                  // dans une chaîne
    const before = formula[start - 1]
    const after = formula[end]
    if (before && /[A-Za-z0-9_]/.test(before)) continue        // suffixe d'un identifiant (ex. LOG10)
    if (after === '(') continue                                // c'est un nom de fonction
    if (after && /[A-Za-z_]/.test(after)) continue             // suivi de lettres → pas une réf pure
    const normalized = m[0].replace(/\$/g, '').toUpperCase()
    let color = colorByNorm.get(normalized)
    if (!color) { color = REF_PALETTE[colorByNorm.size % REF_PALETTE.length]; colorByNorm.set(normalized, color) }
    out.push({ start, end, text: m[0], normalized, color })
  }
  return out
}

export interface CellBounds { c1: number; r1: number; c2: number; r2: number }

// "C7:C16" → {c1:2,r1:7,c2:2,r2:16} (col 0-indexée). null si invalide.
export function refBounds(ref: string): CellBounds | null {
  const parts = ref.replace(/\$/g, '').toUpperCase().split(':')
  const one = (s: string): { c: number; r: number } | null => {
    const m = s.match(/^([A-Z]{1,2})(\d{1,4})$/)
    if (!m) return null
    let c = 0
    for (const ch of m[1]) c = c * 26 + (ch.charCodeAt(0) - 64)
    return { c: c - 1, r: parseInt(m[2], 10) }
  }
  const a = one(parts[0]); if (!a) return null
  const b = parts[1] ? one(parts[1]) : a; if (!b) return null
  return {
    c1: Math.min(a.c, b.c), r1: Math.min(a.r, b.r),
    c2: Math.max(a.c, b.c), r2: Math.max(a.r, b.r),
  }
}

// ── Assistance à la saisie (caret-aware) ─────────────────────────────────────

/** Nom de fonction partiel juste avant le caret (pour l'autocomplétion des noms). */
export function nameTokenAt(text: string, caret: number): string {
  if (!text.startsWith('=')) return ''
  if (text[caret] === '(') return ''            // déjà un appel → pas de suggestion de nom
  let i = caret
  while (i > 0 && /[A-Za-z]/.test(text[i - 1])) i--
  const token = text.slice(i, caret)
  return /^[A-Za-z]+$/.test(token) ? token.toUpperCase() : ''
}

export interface ArgContext { name: string; argIndex: number }

/** Fonction dont les parenthèses contiennent le caret + index de l'argument courant. */
export function argContextAt(text: string, caret: number): ArgContext | null {
  if (!text.startsWith('=')) return null
  const stack: { name: string; arg: number }[] = []
  let i = 0, inStr = false
  while (i < caret && i < text.length) {
    const ch = text[i]
    if (inStr) { if (ch === '"') inStr = false; i++; continue }
    if (ch === '"') { inStr = true; i++; continue }
    if (/[A-Za-z]/.test(ch)) {
      let j = i
      while (j < text.length && /[A-Za-z0-9_]/.test(text[j])) j++
      if (text[j] === '(') { stack.push({ name: text.slice(i, j).toUpperCase(), arg: 0 }); i = j + 1; continue }
      i = j; continue
    }
    if (ch === '(') { stack.push({ name: '', arg: 0 }); i++; continue }
    if (ch === ')') { stack.pop(); i++; continue }
    if ((ch === ',' || ch === ';') && stack.length) { stack[stack.length - 1].arg++; i++; continue }
    i++
  }
  for (let k = stack.length - 1; k >= 0; k--) if (stack[k].name) return { name: stack[k].name, argIndex: stack[k].arg }
  return null
}

export interface ArgSpec { name: string; optional?: boolean; repeat?: boolean }

/** Déduit les arguments depuis une chaîne de syntaxe (ex. "FLOOR(nombre, [multiple])"). */
export function parseSyntaxArgs(syntax: string): ArgSpec[] {
  const m = syntax.match(/\(([^)]*)\)/)
  if (!m || !m[1].trim()) return []
  const args: ArgSpec[] = []
  for (const raw of m[1].split(',').map(s => s.trim())) {
    if (raw === '...' || raw === '…') { if (args.length) args[args.length - 1].repeat = true; continue }
    const repeat = /\.\.\.|…/.test(raw)
    let t = raw.replace(/\.\.\.|…/g, '').trim()
    const optional = t.startsWith('[')
    t = t.replace(/^\[|\]$/g, '').trim()
    if (t) args.push({ name: t, optional, repeat })
  }
  return args
}

// Segments d'une formule pour rendu coloré (réfs colorées, reste en couleur par défaut).
export interface Segment { text: string; color?: string }
export function colorSegments(formula: string): Segment[] {
  const refs = parseRefs(formula)
  if (refs.length === 0) return [{ text: formula }]
  const segs: Segment[] = []
  let pos = 0
  for (const r of refs) {
    if (r.start > pos) segs.push({ text: formula.slice(pos, r.start) })
    segs.push({ text: formula.slice(r.start, r.end), color: r.color })
    pos = r.end
  }
  if (pos < formula.length) segs.push({ text: formula.slice(pos) })
  return segs
}
