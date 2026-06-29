// ── Data validation (validation de données) ──────────────────────────────────
// Règles attachées à des plages (stockées dans sheet.data.validations). Types
// couverts : liste déroulante (valeurs explicites OU plage source), case à
// cocher, nombre (opérateur), longueur de texte. Pures (sans dépendance React).

export type NumOp = 'between' | 'notBetween' | 'gt' | 'lt' | 'ge' | 'le' | 'eq' | 'ne'

export type DVCriterion =
  | { kind: 'list';      values: string[];        strict?: boolean; dropdown?: boolean }
  | { kind: 'listRange'; source: string;          strict?: boolean; dropdown?: boolean }
  | { kind: 'number';    op: NumOp; v1: number; v2?: number }
  | { kind: 'textLen';   op: NumOp; v1: number; v2?: number }
  | { kind: 'checkbox';  on?: string; off?: string }

export interface DataValidation {
  crit:   DVCriterion
  /** true = refuser la saisie invalide ; false = simple avertissement. */
  reject?: boolean
  help?:   string
}
export interface DVBlock { ranges: string[]; rule: DataValidation }

export const isCheckbox = (r?: DataValidation | null): boolean => r?.crit.kind === 'checkbox'
export const hasDropdown = (r?: DataValidation | null): boolean =>
  !!r && (r.crit.kind === 'list' || r.crit.kind === 'listRange') && r.crit.dropdown !== false

const cmp = (n: number, op: NumOp, a: number, b: number): boolean => {
  switch (op) {
    case 'between':    return n >= a && n <= b
    case 'notBetween': return n < a || n > b
    case 'gt':         return n > a
    case 'lt':         return n < a
    case 'ge':         return n >= a
    case 'le':         return n <= a
    case 'eq':         return n === a
    case 'ne':         return n !== a
  }
}

/** Valide une saisie brute contre un critère. `allowed` = valeurs résolues d'une
 *  liste depuis une plage (le composant les fournit). Une cellule vide est tolérée. */
export function validateValue(crit: DVCriterion, raw: string, allowed?: string[]): boolean {
  const s = String(raw ?? '')
  switch (crit.kind) {
    case 'checkbox':
      return true // géré par le widget (TRUE/FALSE)
    case 'list':
    case 'listRange': {
      if (s.trim() === '') return true
      const set = (crit.kind === 'list' ? crit.values : (allowed ?? [])).map(x => String(x).trim())
      return set.includes(s.trim())
    }
    case 'number': {
      if (s.trim() === '') return true
      const n = Number(s)
      if (isNaN(n)) return false
      return cmp(n, crit.op, crit.v1, crit.v2 ?? crit.v1)
    }
    case 'textLen':
      return cmp(s.length, crit.op, crit.v1, crit.v2 ?? crit.v1)
  }
}

/** Libellé court d'un critère (pour la liste des règles + l'aide). */
export function criterionLabel(crit: DVCriterion): string {
  switch (crit.kind) {
    case 'list':      return `Liste : ${crit.values.slice(0, 4).join(', ')}${crit.values.length > 4 ? '…' : ''}`
    case 'listRange': return `Liste depuis ${crit.source}`
    case 'checkbox':  return 'Case à cocher'
    case 'number':    return `Nombre ${opLabel(crit.op)} ${crit.v1}${crit.op === 'between' || crit.op === 'notBetween' ? `–${crit.v2}` : ''}`
    case 'textLen':   return `Longueur ${opLabel(crit.op)} ${crit.v1}${crit.op === 'between' || crit.op === 'notBetween' ? `–${crit.v2}` : ''}`
  }
}
const opLabel = (op: NumOp): string => ({
  between: 'entre', notBetween: 'hors de', gt: '>', lt: '<', ge: '≥', le: '≤', eq: '=', ne: '≠',
}[op])
