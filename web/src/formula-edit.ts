// ── Aides à l'ÉDITION des formules (pures, sans React) ───────────────────────
// Regroupe de nombreuses petites améliorations d'édition façon Excel/Sheets :
// bascule absolu/relatif (F4), auto-fermeture des parenthèses/guillemets,
// équilibrage des parenthèses, parenthèse appariée, aide sur les erreurs,
// suggestion « vouliez-vous dire », classement flou de l'autocomplétion, et
// découpage de la signature pour mettre l'argument courant en gras.

// Référence A1 : 1–3 lettres de colonne, 1–7 chiffres de ligne, $ optionnels,
// préfixe de feuille optionnel ('Ma feuille'! ou Feuille1!).
export const A1_RE = /(?:'[^']+'|[A-Za-z_][A-Za-z0-9_.]*)?!?\$?[A-Za-z]{1,3}\$?\d{1,7}(?::\$?[A-Za-z]{1,3}\$?\d{1,7})?/

// ── F4 : bascule absolu/relatif sur une référence ────────────────────────────
const CELL_RE = /^(\$?)([A-Za-z]{1,3})(\$?)(\d{1,7})$/
/** Cycle d'une cellule : A1 → $A$1 → A$1 → $A1 → A1 (comme Excel). */
export function cycleAbsolute(cell: string): string {
  const m = CELL_RE.exec(cell)
  if (!m) return cell
  const [, , col, , row] = m
  const state = (m[1] ? 1 : 0) + (m[3] ? 2 : 0) // bit0 = $col, bit1 = $row
  // ordre Excel : (0,0)→(1,1)→(0,1)→(1,0)→(0,0)
  const next = state === 0 ? 3 : state === 3 ? 2 : state === 2 ? 1 : 0
  return `${next & 1 ? '$' : ''}${col}${next & 2 ? '$' : ''}${row}`
}

export interface RefSpan { start: number; end: number; text: string }
const REF_G = /\$?[A-Za-z]{1,3}\$?\d{1,7}/g
/** La référence de cellule sous (ou juste avant) le caret, pour F4. */
export function refAtCaret(text: string, caret: number): RefSpan | null {
  REF_G.lastIndex = 0
  let m: RegExpExecArray | null
  while ((m = REF_G.exec(text)) !== null) {
    const s = m.index, e = s + m[0].length
    const before = text[s - 1]
    if (before && /[A-Za-z0-9_]/.test(before)) continue // suffixe d'identifiant
    if (caret >= s && caret <= e) return { start: s, end: e, text: m[0] }
  }
  return null
}
/** Applique F4 à la référence au caret. Renvoie le texte modifié + nouveau caret. */
export function applyF4(text: string, caret: number): { text: string; caret: number } | null {
  const r = refAtCaret(text, caret)
  if (!r) return null
  const cycled = cycleAbsolute(r.text)
  return { text: text.slice(0, r.start) + cycled + text.slice(r.end), caret: r.start + cycled.length }
}

// ── Auto-fermeture des parenthèses / guillemets ──────────────────────────────
/** Saisie d'un caractère : insère la paire fermante, ou « saute par-dessus » une
 *  fermante déjà présente. Renvoie le nouveau texte + caret, ou null (insertion normale). */
export function autoPair(text: string, caret: number, ch: string): { text: string; caret: number } | null {
  const next = text[caret]
  if (ch === '(') return { text: text.slice(0, caret) + '()' + text.slice(caret), caret: caret + 1 }
  if (ch === '"') {
    if (next === '"') return { text, caret: caret + 1 } // saute par-dessus
    return { text: text.slice(0, caret) + '""' + text.slice(caret), caret: caret + 1 }
  }
  if (ch === ')' && next === ')') return { text, caret: caret + 1 } // saute par-dessus
  return null
}

// ── Parenthèses : appariement + équilibrage ──────────────────────────────────
function strMask(text: string): boolean[] {
  const m = new Array(text.length).fill(false)
  let open = false
  for (let i = 0; i < text.length; i++) { if (text[i] === '"') { open = !open; m[i] = true } else m[i] = open }
  return m
}
/** Index de la parenthèse appariée à celle adjacente au caret, ou null. */
export function matchParen(text: string, caret: number): { a: number; b: number } | null {
  const mask = strMask(text)
  const at = (i: number) => i >= 0 && i < text.length && !mask[i] ? text[i] : ''
  // parenthèse juste avant ou juste après le caret
  let pos = -1, dir = 0
  if (at(caret) === '(') { pos = caret; dir = 1 }
  else if (at(caret - 1) === ')') { pos = caret - 1; dir = -1 }
  else if (at(caret) === ')') { pos = caret; dir = -1 }
  else if (at(caret - 1) === '(') { pos = caret - 1; dir = 1 }
  if (pos < 0) return null
  let depth = 0
  for (let i = pos; i >= 0 && i < text.length; i += dir) {
    if (mask[i]) continue
    if (text[i] === '(') depth += dir > 0 ? 1 : -1
    else if (text[i] === ')') depth += dir > 0 ? -1 : 1
    if (depth === 0) return { a: Math.min(pos, i), b: Math.max(pos, i) }
  }
  return null
}
/** Nombre de parenthèses ouvrantes/fermantes manquantes (hors chaînes). */
export function parenBalance(text: string): { missingClose: number; missingOpen: number } {
  const mask = strMask(text)
  let depth = 0, missingOpen = 0
  for (let i = 0; i < text.length; i++) {
    if (mask[i]) continue
    if (text[i] === '(') depth++
    else if (text[i] === ')') { if (depth > 0) depth--; else missingOpen++ }
  }
  return { missingClose: depth, missingOpen }
}
/** Ajoute les `)` manquantes en fin (comportement Excel à la validation). */
export function autoBalance(text: string): string {
  const { missingClose } = parenBalance(text)
  return missingClose > 0 ? text + ')'.repeat(missingClose) : text
}

/** Normalise une formule à la validation (comme Excel) : MAJUSCULES des noms de
 *  fonctions et des lettres de colonne des références — hors chaînes "...". */
export function normalizeFormula(text: string): string {
  if (!text.startsWith('=')) return text
  const mask = strMask(text)
  return text.replace(/\b[A-Za-z][A-Za-z0-9_.]*(?=\()|\b[A-Za-z]{1,3}(?=\$?\d)/g,
    (m, off: number) => mask[off] ? m : m.toUpperCase())
}

// ── Aide sur les erreurs ─────────────────────────────────────────────────────
export const ERROR_HELP: Record<string, string> = {
  '#DIV/0!': 'Division par zéro : le diviseur est 0 ou une cellule vide.',
  '#N/A':    'Valeur non disponible : une recherche (RECHERCHEV/EQUIV…) n’a rien trouvé.',
  '#NAME?':  'Nom non reconnu : nom de fonction mal orthographié, plage nommée inexistante ou texte sans guillemets.',
  '#NULL!':  'Intersection vide : les deux plages ne se croisent pas.',
  '#NUM!':   'Nombre non valide : argument numérique hors limites ou calcul impossible.',
  '#REF!':   'Référence non valide : une cellule référencée a été supprimée.',
  '#VALUE!': 'Type incorrect : du texte est utilisé là où un nombre est attendu.',
  '#SPILL!': 'Plage de déversement bloquée : une cellule sur le trajet du résultat n’est pas vide.',
  '#CALC!':  'Erreur de calcul (tableau vide ou opération non prise en charge).',
  '#ERROR!': 'Formule mal formée.',
}
/** Renvoie l'explication d'un code d'erreur présent dans une valeur de cellule. */
export function errorHelp(value: string): string | null {
  const code = Object.keys(ERROR_HELP).find(c => value === c || value.startsWith(c))
  return code ? ERROR_HELP[code] : null
}

// ── « Vouliez-vous dire ? » (distance de Levenshtein) ────────────────────────
export function levenshtein(a: string, b: string): number {
  const m = a.length, n = b.length
  if (!m) return n; if (!n) return m
  let prev = Array.from({ length: n + 1 }, (_, j) => j)
  for (let i = 1; i <= m; i++) {
    const cur = [i]
    for (let j = 1; j <= n; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
    }
    prev = cur
  }
  return prev[n]
}
/** Nom de fonction connu le plus proche d'un nom inconnu (ou null si trop loin). */
export function didYouMean(name: string, known: string[]): string | null {
  const up = name.toUpperCase()
  let best: string | null = null, bestD = Infinity
  for (const k of known) {
    const d = levenshtein(up, k)
    if (d < bestD) { bestD = d; best = k }
  }
  return best && bestD <= Math.max(1, Math.floor(up.length / 3)) ? best : null
}

// ── Autocomplétion floue (sous-séquence + bonus préfixe) ─────────────────────
/** Score un nom vis-à-vis d'une requête (sous-séquence) ; -1 = pas de correspondance. */
export function fuzzyScore(query: string, name: string): number {
  const q = query.toUpperCase(), s = name.toUpperCase()
  if (!q) return 0
  if (s.startsWith(q)) return 1000 - s.length          // préfixe = meilleur
  let qi = 0, score = 0, run = 0
  for (let i = 0; i < s.length && qi < q.length; i++) {
    if (s[i] === q[qi]) { qi++; run++; score += 1 + run } else run = 0
  }
  if (qi < q.length) return -1                          // pas une sous-séquence
  if (s.includes(q)) score += 50                        // sous-chaîne contiguë
  return score - s.length / 100
}
/** Classe une liste de noms par pertinence floue (filtre les non-correspondances). */
export function rankFunctions<T extends { name: string }>(query: string, list: T[], limit = 12): T[] {
  if (!query) return list.slice(0, limit)
  return list
    .map(f => ({ f, s: fuzzyScore(query, f.name) }))
    .filter(x => x.s >= 0)
    .sort((a, b) => b.s - a.s)
    .slice(0, limit)
    .map(x => x.f)
}

// ── Signature : découpe en parties pour mettre l'argument courant en gras ────
export interface SigPart { text: string; active: boolean }
/** Découpe "IF(condition, si_vrai, si_faux)" en parties, l'argument `argIndex`
 *  étant marqué actif (un argument « répété » `...` reste actif au-delà). */
export function splitSignature(syntax: string, argIndex: number): { fn: string; parts: SigPart[] } {
  const m = syntax.match(/^([^(]*)\(([^)]*)\)/)
  if (!m) return { fn: syntax, parts: [] }
  const fn = m[1]
  const argsRaw = m[2].split(',').map(s => s.trim()).filter(Boolean)
  const repeatAt = argsRaw.findIndex(a => /\.\.\.|…/.test(a))
  const parts: SigPart[] = argsRaw.map((a, i) => ({
    text: a,
    active: i === argIndex || (repeatAt >= 0 && i >= repeatAt && argIndex >= repeatAt),
  }))
  return { fn, parts }
}
