// ── Tableau croisé dynamique (pivot) - moteur d'agrégation pur ────────────────
// Agrège des lignes sources (matrice de valeurs) selon des champs Lignes (imbriqués),
// un champ Colonnes optionnel, PLUSIEURS champs Valeurs (chacun avec sa fonction,
// comme Excel) et des filtres de rapport. Renvoie une grille 2D de chaînes prête
// à écrire dans la feuille (en-têtes + totaux généraux).

export type PivotAgg = 'sum' | 'count' | 'countNum' | 'avg' | 'min' | 'max'

export interface PivotValueSpec { field: number; agg: PivotAgg }
export interface PivotFilterSpec { field: number; selected: string[] }

export interface PivotConfig {
  rowFields: number[]           // indices de colonnes (relatifs à la matrice source) - imbriqués
  colField:  number | null      // une colonne de ventilation, ou null
  values:    PivotValueSpec[]   // champs agrégés (≥ 1)
  filters?:  PivotFilterSpec[]  // filtres de rapport : la ligne doit matcher chaque filtre
  headers:   string[]           // libellé de chaque colonne source (par index)
}

const isBlank = (v: unknown) => v == null || v === ''
const num = (v: unknown): number | null => {
  if (typeof v === 'number') return isFinite(v) ? v : null
  if (typeof v === 'string' && v.trim() !== '' && !isNaN(+v)) return +v
  return null
}

function aggregate(values: unknown[], agg: PivotAgg): number {
  const nums = values.map(num).filter((n): n is number => n != null)
  switch (agg) {
    case 'count':    return values.filter(v => !isBlank(v)).length
    case 'countNum': return nums.length
    case 'sum':      return nums.reduce((a, b) => a + b, 0)
    case 'avg':      return nums.length ? nums.reduce((a, b) => a + b, 0) / nums.length : 0
    case 'min':      return nums.length ? Math.min(...nums) : 0
    case 'max':      return nums.length ? Math.max(...nums) : 0
  }
}

const fmt = (n: number): string => {
  const r = Math.round(n * 1e10) / 1e10
  return String(r)
}
// Tri « naturel » : nombres avant texte, sinon alphabétique.
const cmpVals = (a: string, b: string): number => {
  const na = num(a), nb = num(b)
  if (na != null && nb != null) return na - nb
  if (na != null) return -1
  if (nb != null) return 1
  return a < b ? -1 : a > b ? 1 : 0
}

export const AGG_LABEL: Record<PivotAgg, string> = {
  sum: 'Somme', count: 'Nombre', countNum: 'Nombre (num.)', avg: 'Moyenne', min: 'Min', max: 'Max',
}

// Séparateur de clé composite : un caractère de contrôle qui ne peut pas
// apparaître dans une valeur de cellule (l'ancien « espace » cassait les
// libellés contenant des espaces).
const SEP = '\u0001'

/** Construit la grille 2D (chaînes) du tableau croisé. */
export function buildPivot(rows: unknown[][], cfg: PivotConfig): string[][] {
  const { rowFields, colField, headers } = cfg
  const values = cfg.values.length ? cfg.values : [{ field: 0, agg: 'sum' as PivotAgg }]
  const filters = cfg.filters ?? []

  const kept = filters.length
    ? rows.filter(r => filters.every(f => !f.selected.length || f.selected.includes(String(r[f.field] ?? ''))))
    : rows

  const rowKey = (r: unknown[]) => rowFields.map(f => String(r[f] ?? '')).join(SEP)
  const colKey = (r: unknown[]) => colField != null ? String(r[colField] ?? '') : '__total__'

  // Valeurs distinctes ordonnées des combinaisons de lignes et des colonnes.
  const rowKeys: string[] = []; const rowSeen = new Set<string>()
  const colKeys: string[] = []; const colSeen = new Set<string>()
  // bucket[rowKey][colKey] = lignes sources du croisement
  const bucket = new Map<string, Map<string, unknown[][]>>()

  for (const r of kept) {
    const rk = rowKey(r), ck = colKey(r)
    if (!rowSeen.has(rk)) { rowSeen.add(rk); rowKeys.push(rk) }
    if (!colSeen.has(ck)) { colSeen.add(ck); colKeys.push(ck) }
    let m = bucket.get(rk); if (!m) { m = new Map(); bucket.set(rk, m) }
    const arr = m.get(ck) ?? []; arr.push(r); m.set(ck, arr)
  }

  rowKeys.sort((a, b) => {
    const aa = a.split(SEP), bb = b.split(SEP)
    for (let i = 0; i < aa.length; i++) { const d = cmpVals(aa[i], bb[i]); if (d) return d }
    return 0
  })
  const cols = colKeys.filter(c => c !== '__total__').sort(cmpVals)
  const hasColField = colField != null

  const vLabel = (v: PivotValueSpec) => `${AGG_LABEL[v.agg]} de ${headers[v.field] || `Col ${v.field + 1}`}`.trim()
  const cellOf = (srcRows: unknown[][], v: PivotValueSpec): string =>
    srcRows.length ? fmt(aggregate(srcRows.map(r => r[v.field]), v.agg)) : ''

  const grid: string[][] = []
  // En-tête : champs de lignes, puis (colonnes × valeurs), puis totaux.
  const header: string[] = rowFields.map(f => headers[f] || `Col ${f + 1}`)
  if (hasColField) {
    for (const ck of cols) {
      if (values.length === 1) header.push(ck)
      else for (const v of values) header.push(`${ck} - ${vLabel(v)}`)
    }
    for (const v of values) header.push(values.length === 1 ? 'Total général' : `Total - ${vLabel(v)}`)
  } else {
    for (const v of values) header.push(vLabel(v))
  }
  grid.push(header)

  const allRows: unknown[][] = []
  const colRows: unknown[][][] = cols.map(() => [])

  for (const rk of rowKeys) {
    const m = bucket.get(rk)!
    const line = rk.split(SEP)
    if (hasColField) {
      const rowSrc: unknown[][] = []
      cols.forEach((ck, ci) => {
        const srcRows = m.get(ck) ?? []
        colRows[ci].push(...srcRows); rowSrc.push(...srcRows)
        for (const v of values) line.push(cellOf(srcRows, v))
      })
      allRows.push(...rowSrc)
      for (const v of values) line.push(cellOf(rowSrc, v))
    } else {
      const srcRows = m.get('__total__') ?? []
      allRows.push(...srcRows)
      for (const v of values) line.push(cellOf(srcRows, v))
    }
    grid.push(line)
  }

  // Ligne « Total général ».
  const totalRow: string[] = ['Total général', ...rowFields.slice(1).map(() => '')]
  if (hasColField) {
    cols.forEach((_, ci) => { for (const v of values) totalRow.push(cellOf(colRows[ci], v)) })
    for (const v of values) totalRow.push(cellOf(allRows, v))
  } else {
    for (const v of values) totalRow.push(cellOf(allRows, v))
  }
  grid.push(totalRow)
  return grid
}

/** Valeurs distinctes (triées) d'une colonne source - pour l'UI des filtres. */
export function distinctValues(rows: unknown[][], field: number): string[] {
  const seen = new Set<string>()
  for (const r of rows) seen.add(String(r[field] ?? ''))
  return [...seen].sort(cmpVals)
}
