// « Formes récemment utilisées » — la liste que la galerie partagée affiche en
// tête, façon LibreOffice / Word.
//
// Stockée en localStorage, sous UNE clé pour tout le module office : une forme
// choisie dans le tableur remonte dans la galerie des présentations, du tableau
// blanc et des documents. C'est une commodité d'interface, par appareil — pas un
// réglage utilisateur à synchroniser côté serveur.

const KEY = 'kubuno.office.recentShapes'
/** Nombre de formes conservées (deux rangées environ). */
const MAX = 12

/** Les formes récentes, la plus récente en premier. */
export function loadRecentShapes(): string[] {
  try {
    const a: unknown = JSON.parse(localStorage.getItem(KEY) || '[]')
    return Array.isArray(a) ? a.filter((x): x is string => typeof x === 'string') : []
  } catch {
    return []
  }
}

/** Remonte `kind` en tête de la liste (dédoublonné, plafonné à MAX). */
export function pushRecentShape(kind: string): void {
  try {
    const cur = loadRecentShapes().filter(x => x !== kind)
    cur.unshift(kind)
    localStorage.setItem(KEY, JSON.stringify(cur.slice(0, MAX)))
  } catch {
    /* localStorage indisponible (mode privé strict) : la galerie marche sans récents. */
  }
}
