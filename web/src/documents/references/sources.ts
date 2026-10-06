// Bibliographic sources and their formatting.
//
// One source, several renderings: the in-text citation (« (Dupont, 2024) ») and
// the bibliography entry. Styles differ mostly in punctuation and in where the
// year goes, which is all this module encodes — enough for APA/MLA/Chicago/ISO
// 690/IEEE/Harvard to be recognisably right.
export type SourceType = 'book' | 'article' | 'website' | 'report'

export interface Source {
  id: string
  type: SourceType
  author: string
  title: string
  year: string
  publisher: string
  url: string
  /** Journal / site name, when it differs from the publisher. */
  container: string
}

export const EMPTY_SOURCE: Omit<Source, 'id'> = {
  type: 'book', author: '', title: '', year: '', publisher: '', url: '', container: '',
}

export const SOURCE_TYPES: SourceType[] = ['book', 'article', 'website', 'report']

/** « Dupont, Jean » → « Dupont ». Used by the short in-text citation. */
export function familyName(author: string): string {
  const a = author.trim()
  if (!a) return ''
  if (a.includes(',')) return a.split(',')[0].trim()
  const parts = a.split(/\s+/)
  return parts[parts.length - 1]
}

/** In-text citation, e.g. « (Dupont, 2024) » or « [1] » for IEEE. */
export function citationText(s: Source, style: string, indexInDoc: number): string {
  const name = familyName(s.author) || s.title || '?'
  const year = s.year.trim()
  switch (style) {
    case 'IEEE':
      return `[${indexInDoc}]`
    case 'MLA':
      return `(${name})`
    case 'Chicago':
      return `(${name} ${year})`.replace(/\s+\)/, ')')
    case 'ISO 690':
      return `(${name.toUpperCase()}, ${year})`.replace(/,\s*\)/, ')')
    case 'Harvard':
    case 'APA':
    default:
      return year ? `(${name}, ${year})` : `(${name})`
  }
}

const join = (parts: Array<string | undefined>, sep: string): string =>
  parts.map(p => (p ?? '').trim()).filter(Boolean).join(sep)

/** One line of the bibliography. */
export function bibliographyEntry(s: Source, style: string): string {
  const author = s.author.trim()
  const title = s.title.trim()
  const year = s.year.trim()
  const container = s.container.trim()
  const publisher = s.publisher.trim()
  const url = s.url.trim()

  switch (style) {
    case 'MLA':
      // Auteur. « Titre ». Contenant, éditeur, année, URL.
      return join([author ? author + '.' : '', title ? `« ${title} ».` : '', join([container, publisher, year], ', ') + '.', url], ' ')
    case 'IEEE':
      return join([author, title ? `"${title},"` : '', container, publisher, year], ', ') + '.'
    case 'Chicago':
      return join([author ? author + '.' : '', title ? `${title}.` : '', join([publisher, year], ', ') + '.', url], ' ')
    case 'ISO 690':
      return join([author ? author.toUpperCase() + '.' : '', title ? `${title}.` : '', publisher, year], ' ') + (url ? ` Disponible sur : ${url}` : '')
    case 'Harvard':
      return join([author, year ? `(${year})` : '', title ? `${title}.` : '', publisher], ' ') + (url ? ` ${url}` : '')
    case 'APA':
    default:
      // Auteur. (Année). Titre. Contenant. Éditeur. URL
      return join([
        author ? `${author}.` : '',
        year ? `(${year}).` : '',
        title ? `${title}.` : '',
        container ? `${container}.` : '',
        publisher ? `${publisher}.` : '',
        url,
      ], ' ')
  }
}

/** Alphabetical order by family name, the order every style expects. */
export function sortSources(list: Source[]): Source[] {
  return [...list].sort((a, b) =>
    (familyName(a.author) || a.title).localeCompare(familyName(b.author) || b.title, 'fr'))
}

export function sourceLabel(s: Source): string {
  return join([familyName(s.author) || s.author, s.title, s.year], ' — ') || '(sans titre)'
}
