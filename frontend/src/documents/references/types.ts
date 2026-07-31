// Settings model behind Word's « Table des matières » dialog — the one window
// with four tabs (Index, Table des matières, Table des illustrations, Table des
// références). One tab = one kind of generated table, but they share most of
// their options, which is why they share a base type here.

/** Dots/dashes/underline drawn between the entry and its page number. */
export type LeaderKind = 'none' | 'dots' | 'dashes' | 'underline'

/** The character the canvas repeats to draw the leader. */
export const LEADER_CHAR: Record<LeaderKind, string> = {
  none: '',
  dots: '.',
  dashes: '-',
  underline: '_',
}

/** Word's « Formats » list. `template` = follow the document's own styles. */
export type TableFormat = 'template' | 'classic' | 'distinctive' | 'fancy' | 'modern' | 'formal' | 'simple'

export const TABLE_FORMATS: TableFormat[] = [
  'template', 'classic', 'distinctive', 'fancy', 'modern', 'formal', 'simple',
]

/** Which of the four tables the dialog is currently editing. */
export type TableKind = 'index' | 'toc' | 'figures' | 'authorities'

/** Per-level override set through the « Modifier… » sub-dialog (Word's TM 1..9). */
export interface LevelStyle {
  fontFamily?: string
  fontSizePt?: number
  bold?: boolean
  italic?: boolean
  color?: string
}

export interface CommonTableSettings {
  showPageNumbers: boolean
  rightAlign: boolean
  leader: LeaderKind
  format: TableFormat
  /** Level (1..9) → override on top of the chosen format. */
  levelStyles: Record<number, LevelStyle>
}

export interface TocSettings extends CommonTableSettings {
  /** Web preview: link to the heading instead of printing a page number. */
  useHyperlinks: boolean
  levels: number
  // ── « Options… » sub-dialog ──────────────────────────────────────────────
  /** Build from paragraph STYLES (heading 1..9 → level 1..9). */
  fromStyles: boolean
  /** Build from OUTLINE LEVELS (`w:outlineLvl`) — Word's default, with styles. */
  fromOutline: boolean
  /** Build from TC fields. Off by default, exactly like Word. */
  fromFields: boolean
  /** Style name → TOC level, for the Options dialog's per-style grid. */
  styleLevels: Record<string, number>
}

export interface FiguresSettings extends CommonTableSettings {
  /** Caption label the table collects: « Figure », « Tableau », « Équation »… */
  label: string
  /** Print « Figure 3 : » before the caption text, or the text alone. */
  includeLabel: boolean
}

export interface IndexSettings extends CommonTableSettings {
  /** Word's « En retrait » (sub-entries indented) vs « Sur le même niveau ». */
  type: 'indented' | 'runin'
  columns: number
  language: string
}

/** Word's fixed category list for a table of authorities. */
export const AUTHORITY_CATEGORIES = [
  'all', 'cases', 'statutes', 'other', 'rules', 'treatises', 'regulations', 'constitutional',
] as const
export type AuthorityCategory = (typeof AUTHORITY_CATEGORIES)[number]

export interface AuthoritiesSettings extends CommonTableSettings {
  category: AuthorityCategory
  /** Replace the page list of a source cited 5+ times with « passim ». */
  usePassim: boolean
  keepFormatting: boolean
}

export interface ReferencesSettings {
  toc: TocSettings
  figures: FiguresSettings
  index: IndexSettings
  authorities: AuthoritiesSettings
}

export const DEFAULT_TOC: TocSettings = {
  showPageNumbers: true,
  rightAlign: true,
  leader: 'dots',
  format: 'template',
  useHyperlinks: true,
  levels: 3,
  fromStyles: true,
  fromOutline: true,
  fromFields: false,
  styleLevels: {},
  levelStyles: {},
}

export const DEFAULT_FIGURES: FiguresSettings = {
  showPageNumbers: true,
  rightAlign: true,
  leader: 'dots',
  format: 'template',
  label: 'Figure',
  includeLabel: true,
  levelStyles: {},
}

export const DEFAULT_INDEX: IndexSettings = {
  showPageNumbers: true,
  rightAlign: false,
  leader: 'none',
  format: 'template',
  type: 'indented',
  columns: 2,
  language: '',
  levelStyles: {},
}

export const DEFAULT_AUTHORITIES: AuthoritiesSettings = {
  showPageNumbers: true,
  rightAlign: false,
  leader: 'dots',
  format: 'template',
  category: 'all',
  usePassim: true,
  keepFormatting: true,
  levelStyles: {},
}

export const DEFAULT_REFERENCES: ReferencesSettings = {
  toc: DEFAULT_TOC,
  figures: DEFAULT_FIGURES,
  index: DEFAULT_INDEX,
  authorities: DEFAULT_AUTHORITIES,
}
