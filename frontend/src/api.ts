import { api } from '@kubuno/sdk'
import type { Gradient } from '@ui'

export interface Document {
  id: string
  owner_id: string
  title: string
  icon: string | null
  cover_url: string | null
  content_json: object
  content_text: string
  word_count: number
  is_starred: boolean
  is_trashed: boolean
  trashed_at: string | null
  parent_id: string | null
  position: number
  last_editor_id: string | null
  created_at: string
  updated_at: string
}

export interface DocumentSummary {
  id: string
  owner_id: string
  title: string
  icon: string | null
  word_count: number
  is_starred: boolean
  is_trashed: boolean
  parent_id: string | null
  created_at: string
  updated_at: string
}

export interface DocumentVersion {
  id: string
  document_id: string
  author_id: string
  content_json: object
  word_count: number
  label: string | null
  created_at: string
}

export interface Comment {
  id: string
  document_id: string
  author_id: string
  parent_id: string | null
  content: string
  is_resolved: boolean
  created_at: string
  updated_at: string
}

export interface Template {
  id: string
  name: string
  description: string | null
  category: string
  icon: string | null
  content_json: object
  is_builtin: boolean
  created_by: string | null
  created_at: string
}

export interface Share {
  id: string
  document_id: string
  token: string
  permission: string
  expires_at: string | null
  created_by: string
  created_at: string
  revoked_at: string | null
}

export interface ListDocumentsParams {
  parent_id?: string
  search?: string
  starred?: boolean
  trashed?: boolean
  recent?: boolean
  limit?: number
  offset?: number
}

// ── Fonts ─────────────────────────────────────────────────────────────────────

export interface UserFont {
  id:         string
  user_id:    string
  name:       string
  css_family: string
  source:     'google' | 'url'
  import_url: string
  created_at: string
}

export const officeInitApi = {
  ensureFolders: () =>
    api.post('/office/ensure-folders').then(() => undefined),
}

export const fontsApi = {
  list: () =>
    api.get<{ fonts: UserFont[] }>('/office/fonts').then(r => r.data.fonts),

  add: (dto: { name: string; css_family: string; source?: string; import_url: string }) =>
    api.post<{ font: UserFont }>('/office/fonts', dto).then(r => r.data.font),

  delete: (id: string) =>
    api.delete(`/office/fonts/${id}`),
}

// ── Tableurs ──────────────────────────────────────────────────────────────────

export interface Spreadsheet {
  id:         string
  owner_id:   string
  title:      string
  is_starred: boolean
  is_trashed: boolean
  trashed_at: string | null
  created_at: string
  updated_at: string
}

export interface SheetMeta {
  id:             string
  spreadsheet_id: string
  name:           string
  position:       number
  created_at:     string
  updated_at:     string
}

export interface CellStyle {
  bold?:      boolean
  italic?:    boolean
  underline?: boolean
  strike?:    boolean
  fontSize?:  number
  fontFamily?: string
  color?:     string
  bg?:        string
  align?:     'left' | 'center' | 'right'
  valign?:    'top' | 'center' | 'bottom'
  wrap?:      boolean
  // Raw number-format code (e.g. "00", "dd/mm/yyyy", "0.00%") applied at display.
  numFmtCode?: string
  numFmt?:    'number' | 'currency' | 'percent' | 'scientific'
  decimals?:  number
  thousands?: boolean
  // Bordures par arête (couleur hex ; absente = pas de bordure).
  bt?:        string
  br?:        string
  bb?:        string
  bl?:        string
  // Épaisseur par arête en px (absente = 1). 2 = medium, 3 = thick/double.
  btw?:       number
  brw?:       number
  bbw?:       number
  blw?:       number
  // Remplissage en dégradé (prioritaire sur `bg` quand présent).
  bgGradient?: Gradient
}

export interface CellData {
  v?: string | number | boolean | null  // raw value
  f?: string | null                     // formula (e.g. "=A1+B1")
  s?: CellStyle
}

// Outline group (rows OR columns) — Excel-style « Grouper ». `start`/`end` are
// 1-based row numbers, or 0-based column indices, inclusive.
export interface OutlineGroup { start: number; end: number; collapsed: boolean }

export interface SheetData {
  cells: Record<string, CellData>       // keyed by "A1", "B3", etc.
  // Workbook-level defined names (name → formula text, e.g. "=A1:A10" or
  // "=LAMBDA(n, n+1)"). Attached to the data the engine evaluates against so
  // formulas can reference named ranges / values / lambdas.
  names?: Record<string, string>
  // Merged cell ranges in A1 notation (e.g. "A1:B2") — the top-left cell holds
  // the content and spans the whole rectangle.
  merges?: string[]
  // Sibling sheets' cells, keyed by sheet name (for cross-sheet references like
  // 'Feuille'!A1). Only populated when the workbook has multiple sheets.
  sheets?: Record<string, Record<string, CellData>>
  // Conditional formatting blocks (imported or user-created). Each: { ranges, rules:[{type,op,formulas,dxf,stop,cs?}] }.
  // `cs` (colour scale) is present for type==='colorScale' rules.
  cf?: { ranges: string[]; rules: { type: string; op: string; formulas: string[]; dxf: { bg?: string; color?: string; bold?: boolean; italic?: boolean }; stop: boolean; cs?: { lo: string; mid?: string; hi: string } }[] }[]
  // Data-validation blocks (dropdown lists, checkboxes, number/text-length rules).
  validations?: import('./data-validation').DVBlock[]
  // Outline groups of rows / columns ({ start, end, collapsed }) — Excel-style grouping.
  rowGroups?: OutlineGroup[]
  colGroups?: OutlineGroup[]
  // Show the default cell gridlines (sheet-level; default true).
  gridlines?: boolean
  // Default row height in px for rows without an explicit height (imported).
  defaultRowHeight?: number
  // Default column width in px for columns without an explicit width (imported).
  defaultColWidth?: number
  // Column-level default styles, keyed by column letter ("A"). Applied to every
  // cell of the column that lacks a more specific style (whole-column formatting).
  colStyles?: Record<string, CellStyle>
  // Row-level default styles, keyed by row number ("1"). Applied to every cell of
  // the row that lacks a more specific style. Precedence: cell > row > column.
  rowStyles?: Record<string, CellStyle>
  // Embedded pictures (imported from .xlsx drawings). Each anchor is in grid
  // coordinates: `from` cell + EMU offset, plus either a `to` cell+offset
  // (twoCellAnchor) or an `ext` size in EMU (oneCellAnchor). 1 px = 9525 EMU.
  images?: SheetImage[]
  // Equation objects (LaTeX, rendered with KaTeX) floating over the grid.
  equations?: SheetEquation[]
  // Chart objects (rendered as SVG from a cell range) floating over the grid.
  charts?: SheetChart[]
}

export interface SheetEquation {
  id: string
  bx: number; by: number   // base (zoom=1) px from the grid data origin
  latex: string
}

export type ChartType = 'bar' | 'hbar' | 'line' | 'area' | 'pie' | 'scatter'
export interface SheetChart {
  id?: string
  type: ChartType
  title?: string
  // UI-created charts: an absolute box + a rectangular range.
  bx?: number; by?: number; bw?: number; bh?: number
  range?: string           // "A1:B6" (1st col = labels, 2nd = values)
  // Imported charts: a cell anchor (EMU, like images) + explicit cell refs.
  fromCol?: number; fromColOff?: number; fromRow?: number; fromRowOff?: number
  toCol?: number; toColOff?: number; toRow?: number; toRowOff?: number
  extCx?: number; extCy?: number
  vals?: string[]; cats?: string[]   // bare cell refs e.g. ["R18","R19","R20"]
  // Presentation options (imported from the chart part).
  legend?: boolean
  dataLabels?: 'value' | 'percent'
  colors?: string[]                  // explicit per-slice colours "#RRGGBB"
}

export interface SheetImage {
  fromCol: number; fromColOff?: number; fromRow: number; fromRowOff?: number
  toCol?: number; toColOff?: number; toRow?: number; toRowOff?: number
  extCx?: number; extCy?: number
  // Crop insets as fractions of the source image (a:srcRect), cut from each side.
  cropL?: number; cropT?: number; cropR?: number; cropB?: number
  // Manipulation override: an explicit box in base (zoom=1) pixels measured from the
  // grid's data origin, plus rotation in degrees. Set once the user moves/resizes/
  // rotates the picture; takes precedence over the cell anchor when present.
  bx?: number; by?: number; bw?: number; bh?: number; rot?: number
  src: string   // data:<mime>;base64,<...>
}

export interface SpreadsheetSheet {
  id:             string
  spreadsheet_id: string
  name:           string
  position:       number
  data:           SheetData
  col_widths:     Record<string, number>
  row_heights:    Record<string, number>
  frozen_rows:    number
  frozen_cols:    number
  created_at:     string
  updated_at:     string
  // Workbook-level defined names, delivered alongside each sheet (transient).
  names?:         Record<string, string>
}

export interface SpreadsheetVersion {
  id:             string
  spreadsheet_id: string
  author_id:      string
  snapshot:       object
  label:          string | null
  created_at:     string
}

export interface ListSpreadsheetsParams {
  search?:  string
  starred?: boolean
  trashed?: boolean
  recent?:  boolean
  limit?:   number
  offset?:  number
}

export const spreadsheetsApi = {
  list: (params?: ListSpreadsheetsParams) =>
    api.get<{ spreadsheets: Spreadsheet[] }>('/office/spreadsheets', { params }).then(r => r.data),

  create: (data?: { title?: string }) =>
    api.post<{ spreadsheet: Spreadsheet }>('/office/spreadsheets', data ?? {}).then(r => r.data.spreadsheet),

  get: (id: string) =>
    api.get<{ spreadsheet: Spreadsheet; sheets: SheetMeta[] }>(`/office/spreadsheets/${id}`).then(r => r.data),

  update: (id: string, data: { title?: string; is_starred?: boolean }) =>
    api.patch<{ spreadsheet: Spreadsheet }>(`/office/spreadsheets/${id}`, data).then(r => r.data.spreadsheet),

  trash: (id: string) =>
    api.post(`/office/spreadsheets/${id}/trash`),

  restore: (id: string) =>
    api.post(`/office/spreadsheets/${id}/restore`),

  delete: (id: string) =>
    api.delete(`/office/spreadsheets/${id}/delete`),

  duplicate: (id: string) =>
    api.post<{ id: string }>(`/office/spreadsheets/${id}/duplicate`).then(r => r.data.id),

  // Le serveur renvoie { sheet: <métadonnées>, data: <contenu {cells,…}> } —
  // on fusionne pour que `sheet.data.cells` soit toujours peuplé (sinon les
  // cellules disparaissent quand `onSuccess` remplace le cache par les seules
  // métadonnées).
  getSheet: (ssId: string, sheetId: string) =>
    api.get<{ sheet: SpreadsheetSheet; names?: Record<string, string>; data?: { cells?: Record<string, CellData>; col_widths?: Record<string, number>; row_heights?: Record<string, number>; frozen_rows?: number; frozen_cols?: number; merges?: string[]; cf?: SheetData['cf']; validations?: SheetData['validations']; row_groups?: SheetData['rowGroups']; col_groups?: SheetData['colGroups']; gridlines?: boolean; default_row_height?: number | null; default_col_width?: number | null; col_styles?: SheetData['colStyles']; row_styles?: SheetData['rowStyles']; images?: SheetData['images']; equations?: SheetData['equations']; charts?: SheetData['charts'] } }>(`/office/spreadsheets/${ssId}/sheets/${sheetId}`)
      .then(r => ({
        ...r.data.sheet,
        data: { cells: r.data.data?.cells ?? {}, merges: r.data.data?.merges ?? [], cf: r.data.data?.cf ?? [], validations: r.data.data?.validations ?? [], rowGroups: r.data.data?.row_groups ?? [], colGroups: r.data.data?.col_groups ?? [], gridlines: r.data.data?.gridlines !== false, defaultRowHeight: r.data.data?.default_row_height ?? undefined, defaultColWidth: r.data.data?.default_col_width ?? undefined, colStyles: r.data.data?.col_styles ?? {}, rowStyles: r.data.data?.row_styles ?? {}, images: r.data.data?.images ?? [], equations: r.data.data?.equations ?? [], charts: r.data.data?.charts ?? [] },
        col_widths:  r.data.data?.col_widths  ?? {},
        row_heights: r.data.data?.row_heights ?? {},
        frozen_rows: r.data.data?.frozen_rows ?? 0,
        frozen_cols: r.data.data?.frozen_cols ?? 0,
        names: r.data.names ?? {},
      } as SpreadsheetSheet & { names?: Record<string, string> })),

  updateSheet: (ssId: string, sheetId: string, data: {
    name?: string
    data?: SheetData
    col_widths?: Record<string, number>
    row_heights?: Record<string, number>
    frozen_rows?: number
    frozen_cols?: number
    merges?: string[]
    gridlines?: boolean
    images?: SheetImage[]
    equations?: SheetEquation[]
    charts?: SheetChart[]
    cf?: SheetData['cf']
    validations?: SheetData['validations']
    row_groups?: SheetData['rowGroups']
    col_groups?: SheetData['colGroups']
  }) =>
    api.patch<{ sheet: SpreadsheetSheet; data?: { cells?: Record<string, CellData> } }>(`/office/spreadsheets/${ssId}/sheets/${sheetId}`, data)
      .then(r => ({ ...r.data.sheet, data: { cells: r.data.data?.cells ?? {} } } as SpreadsheetSheet)),

  createSheet: (ssId: string, name?: string) =>
    api.post<{ sheet: SheetMeta }>(`/office/spreadsheets/${ssId}/sheets`, { name }).then(r => r.data.sheet),

  deleteSheet: (ssId: string, sheetId: string) =>
    api.delete(`/office/spreadsheets/${ssId}/sheets/${sheetId}`),

  listVersions: (ssId: string) =>
    api.get<{ versions: SpreadsheetVersion[] }>(`/office/spreadsheets/${ssId}/versions`).then(r => r.data.versions),

  createVersion: (ssId: string, label?: string) =>
    api.post<{ version: SpreadsheetVersion }>(`/office/spreadsheets/${ssId}/versions`, { label }).then(r => r.data.version),

  openByFile: (fileId: string) =>
    api.post<{ spreadsheet: Spreadsheet }>('/office/spreadsheets/open-by-file', { file_id: fileId }).then(r => r.data.spreadsheet),

  // ── Partage utilisateur-à-utilisateur (collaborateurs) ──────────────────────
  listCollaborators: (id: string) =>
    api.get<{ owner: Recipient | null; collaborators: CollaboratorEntry[] }>(`/office/spreadsheets/${id}/collaborators`).then(r => r.data),
  addCollaborator: (id: string, userId: string, permission: CollabPermission = 'edit') =>
    api.post(`/office/spreadsheets/${id}/collaborators`, { user_id: userId, permission }),
  updateCollaborator: (id: string, userId: string, permission: CollabPermission) =>
    api.patch(`/office/spreadsheets/${id}/collaborators/${userId}`, { permission }),
  removeCollaborator: (id: string, userId: string) =>
    api.delete(`/office/spreadsheets/${id}/collaborators/${userId}`),
  listShared: () =>
    api.get<{ spreadsheets: Spreadsheet[] }>('/office/spreadsheets', { params: { shared: true } }).then(r => r.data.spreadsheets),
}

// ── Présentations ─────────────────────────────────────────────────────────────

export interface Presentation {
  id: string
  owner_id: string
  title: string
  theme: {
    name: string
    primaryColor: string
    bgColor: string
    fontFamily: string
    accentColor: string
    textColor: string
  }
  aspect_ratio: string
  slide_width: number
  slide_height: number
  slide_count: number
  is_starred: boolean
  is_trashed: boolean
  trashed_at: string | null
  last_edited_by: string | null
  created_at: string
  updated_at: string
}

export interface SlideSummary {
  id: string
  presentation_id: string
  position: number
  is_hidden: boolean
  thumbnail_path: string | null
  thumbnail_dirty: boolean
  created_at: string
  updated_at: string
}

export interface SlideBackground {
  type: 'color' | 'gradient' | 'image'
  color?: string
  gradient?: { from: string; to: string; angle: number }  // legacy 2-stop
  grad?: Gradient                                          // full multi-stop (préféré si présent)
  imagePath?: string
}

export interface SlideTransition {
  type: string
  duration: number
}

export interface BaseElement {
  id: string
  type: string
  x: number
  y: number
  w: number
  h: number
  rotation: number
  zIndex: number
  locked: boolean
  hidden: boolean
  /** Miroir horizontal / vertical (menu « Faire pivoter »). */
  flipX?: boolean
  flipY?: boolean
  /** Texte alternatif (accessibilité). */
  alt?: string
  /** Animation d'entrée (jouée en mode diaporama). `delay` en ms. */
  anim?: { type: string; duration?: number; delay?: number }
  /** Animation de sortie (jouée avant de passer à la diapo suivante). */
  animExit?: { type: string; duration?: number }
  /** Hyperlien (URL ou #slide:<index>) ouvert/suivi au clic en diaporama. */
  link?: string
  /** Identifiant de groupe : les éléments partageant un `groupId` se sélectionnent
   * et se déplacent ensemble (grouper / dégrouper, façon PowerPoint). */
  groupId?: string
  /** Opacité globale 0..1 (défaut 1). */
  opacity?: number
  /** Ombre portée : `true` = ombre douce par défaut, ou réglages fins (px espace-diapo). */
  shadow?: boolean | { color?: string; blur?: number; dx?: number; dy?: number }
}

export interface TextElement extends BaseElement {
  type: 'text'
  content: object | null
  padding: number
  verticalAlign: 'top' | 'middle' | 'bottom'
  background: string | null
  borderRadius: number
  placeholder: string | null
  /** Surcharges de style (placeholders + barre de mise en forme du texte). */
  fontSize?: number
  align?: 'left' | 'center' | 'right'
  color?: string
  fontFamily?: string
  bold?: boolean
  italic?: boolean
  underline?: boolean
  /** Ajustement texte ↔ forme (menu d'édition de zone de texte). */
  autofit?: 'none' | 'shape' | 'shrink'
  /** Ombre du texte. */
  textShadow?: boolean | { color?: string; blur?: number; dx?: number; dy?: number }
  /** Contour du texte (couleur + épaisseur px espace-diapo). */
  textOutline?: { color: string; width: number }
  /** Transformation de casse à l'affichage. */
  textTransform?: 'upper' | 'lower' | 'capitalize'
  /** Espacement des caractères (px espace-diapo). */
  letterSpacing?: number
  /** Nombre de colonnes (1 ou 2). */
  columns?: number
  /** WordArt : remplissage du texte par un dégradé (de → vers). */
  wordArt?: { from: string; to: string }
}

export interface ShapeElement extends BaseElement {
  type: 'shape'
  shape: string
  fill: { type: string; color?: string; gradient?: { from: string; to: string; angle: number }; grad?: Gradient }
  stroke: { color: string; width: number; style: string }
  content: object | null
  /** Rayon d'angle (px espace-diapo) pour `roundRect`. Défaut ~12px canvas. */
  cornerRadius?: number
  /** Texte centré dans la forme : taille / couleur / police. */
  fontSize?: number
  color?: string
  fontFamily?: string
}

export interface ImageElement extends BaseElement {
  type: 'image'
  storagePath: string
  alt: string
  opacity: number
  /** Région visible de la source (fractions 0..1). Absent = image entière. */
  crop?: { x: number; y: number; w: number; h: number }
  /** Filtres CSS appliqués (canvas `ctx.filter`). */
  filters?: { grayscale?: number; sepia?: number; brightness?: number; contrast?: number; blur?: number; saturate?: number }
  /** Bordure (couleur + épaisseur px espace-diapo). */
  border?: { color: string; width: number }
  /** Rayon d'angle (px espace-diapo) — coins arrondis / rognage. */
  cornerRadius?: number
  /** Teinte appliquée (recolorisation, mélange « multiply »). */
  tint?: string
}

export type LineKind = 'straight' | 'arrow' | 'elbow' | 'curved' | 'arc' | 'polyline' | 'freehand'

export interface LineElement extends BaseElement {
  type: 'line'
  /** Variante de tracé. Absent = ancien format → traité comme 'straight'. */
  lineType?: LineKind
  x2: number
  y2: number
  stroke: { color: string; width: number; style: string }
  arrowEnd: string | null
  /** Tête de flèche au début (connecteur double sens). */
  arrowStart?: string | null
  /** Taille des têtes de flèche (px espace-diapo, défaut 12). */
  arrowSize?: number
  /** Sommets normalisés (0..1) pour polyligne / dessin à main levée. */
  points?: { x: number; y: number }[]
}

export interface ChartElement extends BaseElement {
  type: 'chart'
  chartType: 'column' | 'bar' | 'line' | 'area' | 'pie' | 'donut'
  /** Étiquettes d'axe / parts (catégories). */
  categories: string[]
  /** Séries de données (une par couleur). */
  series: { name: string; values: number[] }[]
  showLegend?: boolean
  title?: string
  /** Palette de couleurs (par série / par part). */
  palette?: string[]
}

export interface TableCell { text: string; bg?: string; color?: string; bold?: boolean; align?: 'left' | 'center' | 'right' }
export interface TableElement extends BaseElement {
  type: 'table'
  rows: number
  cols: number
  /** Cellules [ligne][colonne]. */
  cells: TableCell[][]
  /** Largeurs de colonne (fractions ; somme ≈ 1). Absent = égales. */
  colWidths?: number[]
  /** Hauteurs de ligne (fractions ; somme ≈ 1). Absent = égales. */
  rowHeights?: number[]
  headerRow?: boolean
  /** Première colonne mise en évidence (gras). */
  firstCol?: boolean
  banded?: boolean
  borderColor?: string
  /** Couleurs du style (en-tête / bandes). */
  headerBg?: string
  bandBg?: string
  fontSize?: number
}

export type SlideElement = TextElement | ShapeElement | ImageElement | LineElement | ChartElement | TableElement

export interface Slide extends SlideSummary {
  background: SlideBackground
  notes: string
  elements: SlideElement[]
  transition: SlideTransition
}

export interface ListPresentationsParams {
  search?: string
  starred?: boolean
  trashed?: boolean
  recent?: boolean
  limit?: number
  offset?: number
}

export const presentationsApi = {
  list: (params?: ListPresentationsParams) =>
    api.get<{ presentations: Presentation[]; total: number }>('/office/presentations', { params }).then(r => r.data),

  create: (data?: { title?: string }) =>
    api.post<{ presentation: Presentation }>('/office/presentations', data ?? {}).then(r => r.data.presentation),

  get: (id: string) =>
    api.get<{ presentation: Presentation; slides: SlideSummary[] }>(`/office/presentations/${id}`).then(r => r.data),

  update: (id: string, data: { title?: string; is_starred?: boolean; theme?: object }) =>
    api.patch<{ presentation: Presentation }>(`/office/presentations/${id}`, data).then(r => r.data.presentation),

  trash: (id: string) =>
    api.post(`/office/presentations/${id}/trash`),

  restore: (id: string) =>
    api.post(`/office/presentations/${id}/restore`),

  delete: (id: string) =>
    api.delete(`/office/presentations/${id}/delete`),

  duplicate: (id: string) =>
    api.post<{ presentation: Presentation }>(`/office/presentations/${id}/duplicate`).then(r => r.data.presentation),

  listSlides: (id: string) =>
    api.get<{ slides: SlideSummary[] }>(`/office/presentations/${id}/slides`).then(r => r.data.slides),

  createSlide: (id: string, position?: number) =>
    api.post<{ slide: SlideSummary }>(`/office/presentations/${id}/slides`, { position }).then(r => r.data.slide),

  getSlide: (id: string, sid: string) =>
    // Le contenu (elements / notes / background / transition) arrive dans un champ
    // FRÈRE `data` (miroir du fichier .kbsld), à fusionner dans la slide.
    api.get<{ slide: SlideSummary; data?: Partial<Slide> }>(`/office/presentations/${id}/slides/${sid}`)
      .then(r => ({ elements: [], notes: '', ...r.data.slide, ...(r.data.data ?? {}) }) as Slide),

  updateSlide: (id: string, sid: string, data: { elements?: SlideElement[]; notes?: string }) =>
    // La réponse renvoie le contenu dans le champ FRÈRE `data` ; le fusionner,
    // sinon le state perd ses éléments après chaque sauvegarde automatique.
    api.put<{ slide: SlideSummary; data?: Partial<Slide> }>(`/office/presentations/${id}/slides/${sid}`, data)
      .then(r => ({ elements: [], notes: '', ...r.data.slide, ...(r.data.data ?? {}) }) as Slide),

  updateSlideMeta: (id: string, sid: string, data: { background?: SlideBackground; transition?: SlideTransition; is_hidden?: boolean }) =>
    api.patch<{ slide: SlideSummary; data?: Partial<Slide> }>(`/office/presentations/${id}/slides/${sid}`, data)
      .then(r => ({ elements: [], notes: '', ...r.data.slide, ...(r.data.data ?? {}) }) as Slide),

  deleteSlide: (id: string, sid: string) =>
    api.delete(`/office/presentations/${id}/slides/${sid}`),

  duplicateSlide: (id: string, sid: string) =>
    api.post<{ slide: SlideSummary }>(`/office/presentations/${id}/slides/${sid}/duplicate`).then(r => r.data.slide),

  reorderSlides: (id: string, order: { id: string; position: number }[]) =>
    api.patch(`/office/presentations/${id}/slides/reorder`, { slides: order }),

  openByFile: (fileId: string) =>
    api.post<{ presentation: Presentation }>('/office/presentations/open-by-file', { file_id: fileId }).then(r => r.data.presentation),

  uploadThumbnail: (id: string, sid: string, blob: Blob) => {
    const fd = new FormData()
    fd.append('thumbnail', blob, 'thumb.png')
    return api.post(`/office/presentations/${id}/slides/${sid}/thumbnail`, fd, {
      headers: { 'Content-Type': 'multipart/form-data' },
    })
  },

  // ── Assets image (sortis du doc Yjs, stockés en fichiers cachés) ────────────
  // Upload d'une image → { file_id, ref:"kbfile:<id>" }. Le ref (compact) est ce
  // qu'on stocke dans l'élément de diapo, à la place du base64.
  uploadAsset: (id: string, blob: Blob, filename = 'image') => {
    const fd = new FormData()
    fd.append('file', blob, filename)
    return api.post<{ file_id: string; ref: string }>(`/office/presentations/${id}/assets`, fd, {
      headers: { 'Content-Type': 'multipart/form-data' },
    }).then(r => r.data)
  },
  // Récupère les octets d'un asset (autorisé par l'accès présentation) — pour le rendu canvas.
  fetchAssetBlob: (id: string, fileId: string) =>
    api.get(`/office/presentations/${id}/assets/${fileId}`, { responseType: 'blob' }).then(r => r.data as Blob),

  // ── Partage utilisateur-à-utilisateur (collaborateurs) ──────────────────────
  listCollaborators: (id: string) =>
    api.get<{ owner: Recipient | null; collaborators: CollaboratorEntry[] }>(`/office/presentations/${id}/collaborators`).then(r => r.data),
  addCollaborator: (id: string, userId: string, permission: CollabPermission = 'edit') =>
    api.post(`/office/presentations/${id}/collaborators`, { user_id: userId, permission }),
  updateCollaborator: (id: string, userId: string, permission: CollabPermission) =>
    api.patch(`/office/presentations/${id}/collaborators/${userId}`, { permission }),
  removeCollaborator: (id: string, userId: string) =>
    api.delete(`/office/presentations/${id}/collaborators/${userId}`),
  listShared: () =>
    api.get<{ presentations: Presentation[] }>('/office/presentations', { params: { shared: true } }).then(r => r.data.presentations),
}

// ── Projets ───────────────────────────────────────────────────────────────────

export interface Project {
  id:             string
  owner_id:       string
  title:          string
  description:    string
  color:          string
  status:         'active' | 'on_hold' | 'completed' | 'cancelled'
  start_date:     string | null
  end_date:       string | null
  is_starred:     boolean
  is_trashed:     boolean
  trashed_at:     string | null
  last_edited_by: string | null
  created_at:     string
  updated_at:     string
}

export interface ProjectTask {
  id:            string
  project_id:    string
  parent_id:     string | null
  position:      number
  wbs:           string
  name:          string
  description:   string
  status:        'not_started' | 'in_progress' | 'completed' | 'cancelled' | 'on_hold'
  priority:      'low' | 'medium' | 'high' | 'critical'
  task_type:     'task' | 'milestone' | 'summary'
  start_date:    string | null
  end_date:      string | null
  duration_days: number
  progress:      number
  early_start:   number | null
  early_finish:  number | null
  late_start:    number | null
  late_finish:   number | null
  total_float:   number | null
  is_critical:   boolean
  cpm_dirty:     boolean
  created_at:    string
  updated_at:    string
}

export interface TaskDependency {
  id:           string
  project_id:   string
  from_task_id: string
  to_task_id:   string
  dep_type:     'FS' | 'SS' | 'FF' | 'SF'
  lag_days:     number
}

export interface ProjectResource {
  id:         string
  project_id: string
  name:       string
  role:       string
  color:      string
  capacity:   number
  created_at: string
}

export interface TaskAssignment {
  id:          string
  task_id:     string
  resource_id: string
  units:       number
}

export interface ProjectData {
  project:      Project
  tasks:        ProjectTask[]
  dependencies: TaskDependency[]
  resources:    ProjectResource[]
  assignments:  TaskAssignment[]
}

export const projectsApi = {
  list: (params?: { search?: string; starred?: boolean; trashed?: boolean; recent?: boolean }) =>
    api.get<{ projects: Project[]; total: number }>('/office/projects', { params }).then(r => r.data),

  create: (data?: { title?: string; color?: string; start_date?: string; end_date?: string }) =>
    api.post<{ project: Project }>('/office/projects', data ?? {}).then(r => r.data.project),

  get: (id: string) =>
    api.get<ProjectData>(`/office/projects/${id}`).then(r => r.data),

  update: (id: string, data: Partial<Pick<Project, 'title' | 'description' | 'color' | 'status' | 'start_date' | 'end_date' | 'is_starred'>>) =>
    api.patch<{ project: Project }>(`/office/projects/${id}`, data).then(r => r.data.project),

  trash: (id: string) =>
    api.post(`/office/projects/${id}/trash`),

  restore: (id: string) =>
    api.post(`/office/projects/${id}/restore`),

  delete: (id: string) =>
    api.delete(`/office/projects/${id}/delete`),

  duplicate: (id: string) =>
    api.post<{ id: string }>(`/office/projects/${id}/duplicate`).then(r => r.data.id),

  createTask: (projectId: string, data?: { name?: string; parent_id?: string; position?: number; task_type?: string; start_date?: string; duration_days?: number }) =>
    api.post<{ task: ProjectTask }>(`/office/projects/${projectId}/tasks`, data ?? {}).then(r => r.data.task),

  updateTask: (projectId: string, taskId: string, data: Partial<Pick<ProjectTask, 'name' | 'description' | 'status' | 'priority' | 'task_type' | 'start_date' | 'end_date' | 'duration_days' | 'progress' | 'position' | 'wbs'>>) =>
    api.patch<{ task: ProjectTask }>(`/office/projects/${projectId}/tasks/${taskId}`, data).then(r => r.data.task),

  deleteTask: (projectId: string, taskId: string) =>
    api.delete(`/office/projects/${projectId}/tasks/${taskId}`),

  createDependency: (projectId: string, data: { from_task_id: string; to_task_id: string; dep_type?: string; lag_days?: number }) =>
    api.post<{ dependency: TaskDependency }>(`/office/projects/${projectId}/dependencies`, data).then(r => r.data.dependency),

  deleteDependency: (projectId: string, depId: string) =>
    api.delete(`/office/projects/${projectId}/dependencies/${depId}`),

  listResources: (projectId: string) =>
    api.get<{ resources: ProjectResource[] }>(`/office/projects/${projectId}/resources`).then(r => r.data.resources),

  createResource: (projectId: string, data: { name: string; role?: string; color?: string; capacity?: number }) =>
    api.post<{ resource: ProjectResource }>(`/office/projects/${projectId}/resources`, data).then(r => r.data.resource),

  updateResource: (projectId: string, resourceId: string, data: Partial<Pick<ProjectResource, 'name' | 'role' | 'color' | 'capacity'>>) =>
    api.patch<{ resource: ProjectResource }>(`/office/projects/${projectId}/resources/${resourceId}`, data).then(r => r.data.resource),

  deleteResource: (projectId: string, resourceId: string) =>
    api.delete(`/office/projects/${projectId}/resources/${resourceId}`),

  assignResource: (projectId: string, taskId: string, data: { resource_id: string; units?: number }) =>
    api.post<{ assignment: TaskAssignment }>(`/office/projects/${projectId}/tasks/${taskId}/assign`, data).then(r => r.data.assignment),

  unassignResource: (projectId: string, taskId: string, resourceId: string) =>
    api.delete(`/office/projects/${projectId}/tasks/${taskId}/assign/${resourceId}`),

  computeCpm: (projectId: string) =>
    api.post<{ ok: boolean; tasks: ProjectTask[] }>(`/office/projects/${projectId}/cpm`).then(r => r.data),

  openByFile: (fileId: string) =>
    api.post<{ project: Project }>('/office/projects/open-by-file', { file_id: fileId }).then(r => r.data.project),

  // ── Partage utilisateur-à-utilisateur (collaborateurs) ──────────────────────
  listCollaborators: (id: string) =>
    api.get<{ owner: Recipient | null; collaborators: CollaboratorEntry[] }>(`/office/projects/${id}/collaborators`).then(r => r.data),
  addCollaborator: (id: string, userId: string, permission: CollabPermission = 'edit') =>
    api.post(`/office/projects/${id}/collaborators`, { user_id: userId, permission }),
  updateCollaborator: (id: string, userId: string, permission: CollabPermission) =>
    api.patch(`/office/projects/${id}/collaborators/${userId}`, { permission }),
  removeCollaborator: (id: string, userId: string) =>
    api.delete(`/office/projects/${id}/collaborators/${userId}`),
}

// ── Diagrammes ────────────────────────────────────────────────────────────────

export interface Diagram {
  id:             string
  owner_id:       string
  title:          string
  diagram_type:   string
  settings:       object
  is_starred:     boolean
  is_trashed:     boolean
  trashed_at:     string | null
  last_edited_by: string | null
  created_at:     string
  updated_at:     string
}

export interface DiagramPageSummary {
  id:         string
  diagram_id: string
  name:       string
  position:   number
  bg_color:   string
  width:      number
  height:     number
  is_hidden:  boolean
  created_at: string
  updated_at: string
}

export interface DiagramPage extends DiagramPageSummary {
  data: object
}

export interface DiagramCustomShape {
  id:         string
  owner_id:   string
  name:       string
  category:   string
  shape_def:  object
  thumbnail:  string | null
  created_at: string
}

export const diagramsApi = {
  list: (params?: { search?: string; starred?: boolean; trashed?: boolean; recent?: boolean; limit?: number; offset?: number }) =>
    api.get<{ diagrams: Diagram[]; total: number }>('/office/diagrams', { params }).then(r => r.data),

  create: (data?: { title?: string; diagram_type?: string }) =>
    api.post<{ diagram: Diagram }>('/office/diagrams', data ?? {}).then(r => r.data.diagram),

  get: (id: string) =>
    api.get<{ diagram: Diagram; pages: DiagramPageSummary[] }>(`/office/diagrams/${id}`).then(r => r.data),

  update: (id: string, data: { title?: string; diagram_type?: string; settings?: object; is_starred?: boolean }) =>
    api.patch<{ diagram: Diagram }>(`/office/diagrams/${id}`, data).then(r => r.data.diagram),

  trash: (id: string) =>
    api.post(`/office/diagrams/${id}/trash`),

  restore: (id: string) =>
    api.post(`/office/diagrams/${id}/restore`),

  delete: (id: string) =>
    api.delete(`/office/diagrams/${id}/delete`),

  duplicate: (id: string) =>
    api.post<{ diagram: Diagram }>(`/office/diagrams/${id}/duplicate`).then(r => r.data.diagram),

  exportJson: (id: string) =>
    api.get<object>(`/office/diagrams/${id}/export/json`).then(r => r.data),

  listPages: (id: string) =>
    api.get<{ pages: DiagramPageSummary[] }>(`/office/diagrams/${id}/pages`).then(r => r.data.pages),

  createPage: (id: string, data?: { name?: string; bg_color?: string }) =>
    api.post<{ page: DiagramPageSummary }>(`/office/diagrams/${id}/pages`, data ?? {}).then(r => r.data.page),

  reorderPages: (id: string, order: { id: string; position: number }[]) =>
    api.patch(`/office/diagrams/${id}/pages/reorder`, { pages: order }),

  getPage: (id: string, pid: string) =>
    // `data` (shapes/connectors) est un champ FRÈRE de `page` dans la réponse — il faut
    // le fusionner, sinon le contenu .kbdia n'est jamais relu (diagramme vide au chargement).
    api.get<{ page: DiagramPage; data: object }>(`/office/diagrams/${id}/pages/${pid}`).then(r => ({ ...r.data.page, data: r.data.data })),

  updatePageData: (id: string, pid: string, data: object) =>
    api.put<{ page: DiagramPage }>(`/office/diagrams/${id}/pages/${pid}/data`, { data }).then(r => r.data.page),

  updatePageMeta: (id: string, pid: string, meta: { name?: string; bg_color?: string; is_hidden?: boolean; position?: number }) =>
    api.patch<{ page: DiagramPageSummary }>(`/office/diagrams/${id}/pages/${pid}`, meta).then(r => r.data.page),

  deletePage: (id: string, pid: string) =>
    api.delete(`/office/diagrams/${id}/pages/${pid}`),

  listCustomShapes: () =>
    api.get<{ shapes: DiagramCustomShape[] }>('/office/shapes/custom').then(r => r.data.shapes),

  createCustomShape: (data: { name: string; category?: string; shape_def: object; thumbnail?: string }) =>
    api.post<{ shape: DiagramCustomShape }>('/office/shapes/custom', data).then(r => r.data.shape),

  deleteCustomShape: (sid: string) =>
    api.delete(`/office/shapes/custom/${sid}`),

  openByFile: (fileId: string) =>
    api.post<{ diagram: Diagram }>('/office/diagrams/open-by-file', { file_id: fileId }).then(r => r.data.diagram),
}

export const officeApi = {
  list: (params?: ListDocumentsParams) =>
    api.get<{ documents: DocumentSummary[]; total: number }>('/office/documents', { params }).then(r => r.data),

  create: (data: { title?: string; icon?: string; parent_id?: string; template_id?: string }) =>
    api.post<{ document: Document; content_json?: object }>('/office/documents', data)
      .then(r => ({ ...r.data.document, content_json: r.data.content_json ?? r.data.document.content_json })),

  get: (id: string) =>
    api.get<{ document: Document; content_json?: object }>(`/office/documents/${id}`)
      .then(r => ({ ...r.data.document, content_json: r.data.content_json ?? r.data.document.content_json })),

  update: (id: string, data: { title?: string; icon?: string; cover_url?: string; content_json?: object; is_starred?: boolean; parent_id?: string }) =>
    api.patch<{ document: Document; content_json?: object }>(`/office/documents/${id}`, data)
      .then(r => ({ ...r.data.document, content_json: r.data.content_json ?? r.data.document.content_json })),

  trash: (id: string) =>
    api.post(`/office/documents/${id}/trash`),

  restore: (id: string) =>
    api.post(`/office/documents/${id}/restore`),

  delete: (id: string) =>
    api.delete(`/office/documents/${id}/delete`),

  duplicate: (id: string) =>
    api.post<{ document: Document; content_json?: object }>(`/office/documents/${id}/duplicate`)
      .then(r => ({ ...r.data.document, content_json: r.data.content_json ?? r.data.document.content_json })),

  listVersions: (id: string) =>
    api.get<{ versions: DocumentVersion[] }>(`/office/documents/${id}/versions`).then(r => r.data.versions),

  createVersion: (id: string, label?: string) =>
    api.post<{ version: DocumentVersion }>(`/office/documents/${id}/versions`, { label }).then(r => r.data.version),

  restoreVersion: (docId: string, verId: string) =>
    api.post<{ document: Document; content_json?: object }>(`/office/documents/${docId}/versions/${verId}/restore`)
      .then(r => ({ ...r.data.document, content_json: r.data.content_json ?? r.data.document.content_json })),

  listComments: (docId: string) =>
    api.get<{ comments: Comment[] }>(`/office/documents/${docId}/comments`).then(r => r.data.comments),

  createComment: (docId: string, content: string, parentId?: string) =>
    api.post<{ comment: Comment }>(`/office/documents/${docId}/comments`, { content, parent_id: parentId }).then(r => r.data.comment),

  deleteComment: (docId: string, commentId: string) =>
    api.delete(`/office/documents/${docId}/comments/${commentId}`),

  resolveComment: (docId: string, commentId: string) =>
    api.post(`/office/documents/${docId}/comments/${commentId}/resolve`),

  listTemplates: () =>
    api.get<{ templates: Template[] }>('/office/documents/templates').then(r => r.data.templates),

  createShare: (docId: string, permission?: string) =>
    api.post<{ share: Share }>(`/office/documents/${docId}/shares`, { permission }).then(r => r.data.share),

  listShares: (docId: string) =>
    api.get<{ shares: Share[] }>(`/office/documents/${docId}/shares`).then(r => r.data.shares),

  revokeShare: (docId: string, shareId: string) =>
    api.delete(`/office/documents/${docId}/shares/${shareId}`),

  // ── Partage utilisateur-à-utilisateur (collaborateurs) ──────────────────────
  searchRecipients: (q: string) =>
    api.get<{ recipients: Recipient[] }>('/office/recipients', { params: { q } }).then(r => r.data.recipients),

  listCollaborators: (docId: string) =>
    api.get<{ owner: Recipient | null; collaborators: CollaboratorEntry[] }>(`/office/documents/${docId}/collaborators`).then(r => r.data),

  addCollaborator: (docId: string, userId: string, permission: CollabPermission = 'edit') =>
    api.post(`/office/documents/${docId}/collaborators`, { user_id: userId, permission }),

  updateCollaborator: (docId: string, userId: string, permission: CollabPermission) =>
    api.patch(`/office/documents/${docId}/collaborators/${userId}`, { permission }),

  removeCollaborator: (docId: string, userId: string) =>
    api.delete(`/office/documents/${docId}/collaborators/${userId}`),

  listSharedWithMe: () =>
    api.get<{ documents: DocumentSummary[] }>('/office/documents', { params: { shared: true } }).then(r => r.data.documents),

  openByFile: (fileId: string) =>
    api.post<{ document: Document; content_json?: object }>('/office/documents/open-by-file', { file_id: fileId })
      .then(r => ({ ...r.data.document, content_json: r.data.content_json ?? r.data.document.content_json })),
}

export type CollabPermission = 'view' | 'comment' | 'edit'

export interface Recipient {
  id:           string
  display_name: string | null
  email:        string
  avatar_url:   string | null
}

export interface CollaboratorEntry {
  user_id:      string
  permission:   CollabPermission
  display_name: string | null
  email:        string
  avatar_url:   string | null
}
