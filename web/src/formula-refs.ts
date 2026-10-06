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

// Réfs A1 complètes : préfixe de feuille optionnel ('Ma feuille'! ou Feuille1!),
// cellules/plages (colonnes 1–3 lettres jusqu'à XFD, lignes 1–7 chiffres jusqu'à
// 1048576), ET colonnes/lignes ENTIÈRES (A:A, $A:$C, 1:1, 2:5).
const REF_RE = /(?:(?:'[^']*'|[A-Za-z_][A-Za-z0-9_.]*)!)?(?:\$?[A-Za-z]{1,3}\$?\d{1,7}(?::\$?[A-Za-z]{1,3}\$?\d{1,7})?|\$?[A-Za-z]{1,3}:\$?[A-Za-z]{1,3}|\$?\d{1,7}:\$?\d{1,7})/g

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

// "C7:C16" → {c1:2,r1:7,c2:2,r2:16} (col 0-indexée). Ignore un préfixe de feuille
// ('Feuille'!C7:C16). null si invalide.
const COL_MAX = 16383, ROW_MAX = 1048576
const colIdx = (s: string): number => { let c = 0; for (const ch of s) c = c * 26 + (ch.charCodeAt(0) - 64); return c - 1 }

export function refBounds(ref: string): CellBounds | null {
  const noSheet = ref.includes('!') ? ref.slice(ref.lastIndexOf('!') + 1) : ref
  const u = noSheet.replace(/\$/g, '').toUpperCase()
  // Colonnes entières A:C → toutes les lignes ; lignes entières 2:5 → toutes les colonnes.
  let w = u.match(/^([A-Z]{1,3}):([A-Z]{1,3})$/)
  if (w) { const a = colIdx(w[1]), b = colIdx(w[2]); return { c1: Math.min(a, b), r1: 1, c2: Math.max(a, b), r2: ROW_MAX } }
  w = u.match(/^(\d{1,7}):(\d{1,7})$/)
  if (w) { const a = +w[1], b = +w[2]; return { c1: 0, r1: Math.min(a, b), c2: COL_MAX, r2: Math.max(a, b) } }
  const parts = u.split(':')
  const one = (s: string): { c: number; r: number } | null => {
    const m = s.match(/^([A-Z]{1,3})(\d{1,7})$/)
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

// Couleurs des parenthèses par profondeur d'imbrication (arc-en-ciel, façon IDE).
export const PAREN_COLORS = ['#1a73e8', '#e8710a', '#188038', '#9334e6', '#12b5cb', '#c5221f']
const STRING_COLOR   = '#188038'   // littéraux entre guillemets (vert)
const ERROR_COLOR    = '#d93025'   // parenthèse non appariée / chaîne non fermée (rouge)
const FUNCTION_COLOR = '#8e24aa'   // noms de fonctions connus (violet)
const KEYWORD_COLOR  = '#00838f'   // mots-clés booléens TRUE/FALSE (teal, distinct des réfs)
const NAME_COLOR     = '#b06000'   // plages nommées (brun-orangé)
const OPERATOR_COLOR = '#5f6368'   // opérateurs + - * / ^ & < > = % (gris discret)

// Segments d'une formule pour rendu coloré : références colorées (couleur stable),
// parenthèses arc-en-ciel par profondeur, chaînes en vert, et `wavy` = souligné
// ondulé d'erreur (parenthèse non appariée, chaîne non fermée, fonction inconnue).
// `knownFns` (optionnel) : noms de fonctions valides (MAJ) pour détecter les fautes.
export interface Segment { text: string; color?: string; wavy?: boolean }
export function colorSegments(formula: string, knownFns?: Set<string>, names?: Set<string>): Segment[] {
  if (!formula.startsWith('=')) return [{ text: formula }]
  const n = formula.length
  const color: (string | undefined)[] = new Array(n).fill(undefined)
  const wavy: boolean[] = new Array(n).fill(false)

  // 1. Chaînes "..." (vert) + masque ; chaîne non terminée → erreur ondulée.
  const inStr: boolean[] = new Array(n).fill(false)
  let open = false
  for (let i = 0; i < n; i++) {
    if (formula[i] === '"') { open = !open; inStr[i] = true; color[i] = STRING_COLOR; continue }
    inStr[i] = open
    if (open) color[i] = STRING_COLOR
  }
  if (open) for (let i = 0; i < n; i++) if (inStr[i]) wavy[i] = true

  // 2. Références (couleur stable par réf) — hors chaînes (parseRefs masque déjà).
  for (const r of parseRefs(formula)) {
    for (let i = r.start; i < r.end; i++) color[i] = r.color
  }

  // 3. Parenthèses arc-en-ciel + détection des non-appariées (hors chaînes).
  const stack: number[] = []
  for (let i = 0; i < n; i++) {
    if (inStr[i]) continue
    const ch = formula[i]
    if (ch === '(') { color[i] = PAREN_COLORS[stack.length % PAREN_COLORS.length]; stack.push(i) }
    else if (ch === ')') {
      const o = stack.pop()
      if (o !== undefined) color[i] = color[o]            // ferme → même couleur que son ouvrante
      else { color[i] = ERROR_COLOR; wavy[i] = true }      // fermante orpheline
    }
  }
  for (const i of stack) { color[i] = ERROR_COLOR; wavy[i] = true }   // ouvrantes non fermées

  // 4. Noms de fonctions : connus → colorés (violet) ; inconnus → souligné d'erreur.
  if (knownFns) {
    const FN_RE = /[A-Za-z][A-Za-z0-9_.]*(?=\()/g
    let m: RegExpExecArray | null
    while ((m = FN_RE.exec(formula)) !== null) {
      if (inStr[m.index]) continue
      const known = knownFns.has(m[0].toUpperCase())
      for (let i = m.index; i < m.index + m[0].length; i++) {
        if (known) { if (color[i] === undefined) color[i] = FUNCTION_COLOR }
        else wavy[i] = true
      }
    }
  }

  // 5. Mots-clés booléens TRUE/FALSE (hors chaînes, non suivis de « ( »).
  const KW_RE = /\b(TRUE|FALSE|VRAI|FAUX)\b/gi
  let km: RegExpExecArray | null
  while ((km = KW_RE.exec(formula)) !== null) {
    if (inStr[km.index] || formula[km.index + km[0].length] === '(') continue
    for (let i = km.index; i < km.index + km[0].length; i++) if (color[i] === undefined) color[i] = KEYWORD_COLOR
  }

  // 6. Plages NOMMÉES (identifiant non suivi de « ( », présent dans `names`).
  if (names && names.size) {
    const ID_RE = /[A-Za-z_][A-Za-z0-9_.]*/g
    let nm: RegExpExecArray | null
    while ((nm = ID_RE.exec(formula)) !== null) {
      if (inStr[nm.index] || formula[nm.index + nm[0].length] === '(') continue
      if (color[nm.index] !== undefined) continue            // déjà coloré (réf/fonction)
      if (names.has(nm[0].toUpperCase()))
        for (let i = nm.index; i < nm.index + nm[0].length; i++) if (color[i] === undefined) color[i] = NAME_COLOR
    }
  }

  // 7. Opérateurs (hors chaînes) → gris discret pour la lisibilité.
  for (let i = 0; i < n; i++) {
    if (inStr[i] || color[i] !== undefined) continue
    if ('+-*/^&<>=%'.includes(formula[i])) color[i] = OPERATOR_COLOR
  }

  // Regroupe les caractères consécutifs de même (couleur, wavy) en segments.
  const segs: Segment[] = []
  let i = 0
  while (i < n) {
    const c = color[i], w = wavy[i]
    let j = i + 1
    while (j < n && color[j] === c && wavy[j] === w) j++
    segs.push(w ? { text: formula.slice(i, j), color: c, wavy: true } : { text: formula.slice(i, j), color: c })
    i = j
  }
  return segs
}
