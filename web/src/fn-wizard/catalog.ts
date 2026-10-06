// Catalogue view used by the function wizard: the flat function list, grouped by
// category and enriched with what the wizard needs — the argument NAMES parsed out
// of each syntax string, and the description key when one exists.
//
// LibreOffice's Assistant Fonction drives everything from these three facts: which
// category a function belongs to (the tree), what its arguments are called (one
// field per argument) and what it does (the description pane).

import { FUNCTION_CATALOG, CAT_LABEL, type CatalogFn, type FnCat } from '../formula-catalog'

export interface FnArg {
  /** Argument name as written in the syntax, e.g. "nombre1". */
  name: string
  /** True for the trailing "..." of variadic functions. */
  variadic: boolean
  /** Excel/Calc mark optional arguments with brackets. */
  optional: boolean
}

export interface WizardFn extends CatalogFn {
  args: FnArg[]
  descKey?: string
}

/**
 * Split "SUM(nombre1, ...)" into its argument names. The catalogue's syntax
 * strings are the single source of truth: parsing them keeps the wizard in sync
 * with the autocomplete helper instead of duplicating a second argument table.
 */
export function parseArgs(syntax: string): FnArg[] {
  const open = syntax.indexOf('(')
  if (open < 0) return []
  const inner = syntax.slice(open + 1, syntax.lastIndexOf(')') > open ? syntax.lastIndexOf(')') : undefined).trim()
  if (!inner || inner === '…' || inner === '...') return []
  return inner.split(',').map(raw => {
    const s = raw.trim()
    const variadic = s === '...' || s === '…'
    const optional = /^\[.*\]$/.test(s)
    return { name: optional ? s.slice(1, -1).trim() : s, variadic, optional }
  }).filter(a => a.name.length > 0)
}

/** The wizard's own view of the catalogue (built once). */
export function buildWizardCatalog(descByName: Map<string, string>): WizardFn[] {
  return FUNCTION_CATALOG.map(f => ({ ...f, args: parseArgs(f.syntax), descKey: descByName.get(f.name) }))
}

export interface FnGroup { cat: FnCat; label: string; fns: WizardFn[] }

/** Group functions by category, dropping empty groups, categories sorted by label. */
export function groupByCategory(fns: WizardFn[]): FnGroup[] {
  const map = new Map<FnCat, WizardFn[]>()
  for (const f of fns) {
    const arr = map.get(f.cat); if (arr) arr.push(f); else map.set(f.cat, [f])
  }
  return [...map.entries()]
    .map(([cat, list]) => ({ cat, label: CAT_LABEL[cat], fns: list }))
    .sort((a, b) => a.label.localeCompare(b.label))
}

/**
 * Filter for the tree. `similar` widens a plain "starts with / contains" match to
 * a subsequence match (LibreOffice's « Similaire »): typing "sme" then finds
 * "SOMME", which is what you want when you only half-remember a name.
 */
export function matches(fn: WizardFn, query: string, similar: boolean): boolean {
  const q = query.trim().toUpperCase()
  if (!q) return true
  const name = fn.name.toUpperCase()
  if (name.includes(q)) return true
  if (!similar) return false
  let i = 0
  for (const ch of name) { if (ch === q[i]) i++; if (i === q.length) return true }
  return false
}

/** Order matches the way a user reads them: exact, then prefix, then the rest. */
export function rankByRelevance(fns: WizardFn[], query: string): WizardFn[] {
  const q = query.trim().toUpperCase()
  if (!q) return fns
  const score = (f: WizardFn) => (f.name === q ? 0 : f.name.startsWith(q) ? 1 : f.name.includes(q) ? 2 : 3)
  return [...fns].sort((a, b) => score(a) - score(b) || a.name.localeCompare(b.name))
}

/** Build "NAME(a, b)" from the argument values the user typed. */
export function composeFormula(fn: WizardFn, values: string[]): string {
  const given = values.map(v => v.trim())
  // Trailing empties are dropped: an untouched optional argument must not leave a
  // dangling separator behind (`SUM(A1;;)` is an error, `SUM(A1)` is not).
  while (given.length && given[given.length - 1] === '') given.pop()
  return `${fn.name}(${given.join('; ')})`
}
