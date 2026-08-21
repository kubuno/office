import { api } from '@kubuno/sdk'
import { isUnlocked, encryptCells, decryptCells, type SheetEncEnvelope } from './wbEncryption'
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
  /** Format the document was opened from; null for one created here. */
  source_format: 'docx' | 'odt' | 'doc' | null
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
  /** Format the workbook was opened from ("xlsx" | "ods"); null = native. */
  source_format?: string | null
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

// A persistent pivot table: bound to a source range and recomputed on change.
export interface PivotDef {
  id: string
  src: string                 // source range "A1:D20" (first row = headers)
  dest: string                // output anchor cell "H1"
  rowFields: number[]         // ABSOLUTE sheet column indices
  colField: number | null
  values: { field: number; agg: 'sum' | 'count' | 'countNum' | 'avg' | 'min' | 'max' }[]
  filters: { field: number; selected: string[] }[]
  lastRows?: number           // last written extent (to clear stale cells)
  lastCols?: number
}

// OOXML <sheetProtection> attributes (Excel-compatible password hash).
export interface SheetProtection {
  algorithmName: string
  hashValue: string
  saltValue: string
  spinCount: number
}

export interface CellData {
  v?: string | number | boolean | null  // raw value
  f?: string | null                     // formula (e.g. "=A1+B1")
  s?: CellStyle
  c?: string | null                     // cell note/comment (Excel-style)
  link?: string | null                  // hyperlink target (imported; "#Sheet!A1" = internal)
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
  // Conditional formatting blocks (imported or user-created). Base rule shape:
  // { type, op, formulas, dxf, stop }; typed extensions per rule type (kept in
  // sync with `CondRule` in formula-engine.ts): `cs`/`csv` (colorScale),
  // `bar` (dataBar), `icons` (iconSet), `text` (contains/begins/ends),
  // `period` (timePeriod), `rank`/`percent`/`bottom` (top10),
  // `above`/`equal`/`stdDev` (aboveAverage), `priority` (OOXML order).
  cf?: {
    ranges: string[]
    rules: {
      type: string; op: string; formulas: string[]
      dxf: { bg?: string; color?: string; bold?: boolean; italic?: boolean }
      stop: boolean
      priority?: number
      cs?: { lo: string; mid?: string; hi: string }
      csv?: { t: string; v?: number | string }[]
      bar?: { color: string; showValue?: boolean; min?: { t: string; v?: number | string }; max?: { t: string; v?: number | string } }
      icons?: { set: string; showValue?: boolean; reverse?: boolean; cfvo?: { t: string; v?: number | string }[] }
      text?: string
      period?: string
      rank?: number; percent?: boolean; bottom?: boolean
      above?: boolean; equal?: boolean; stdDev?: number
    }[]
  }[]
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
  // Drawing shapes (rectangles, arrows, callouts…) floating over the grid, as DOM
  // overlays like charts. Absent on every workbook authored before shapes existed.
  shapes?: SheetShape[]
  // Persistent pivot tables (recomputed when their source range changes).
  pivots?: PivotDef[]
  // Sheet password protection (OOXML sheetProtection hash — see sheetProtect.ts).
  protection?: SheetProtection | null
  // Imported auto-filter range ("A1:C5"): the editor shows the header funnels
  // when present (the filter state itself is not modelled).
  autoFilter?: string
  // Agile-encrypted cell payload (present when the workbook is encrypted; cells then empty).
  enc?: SheetEncEnvelope | null
}

export interface SheetEquation {
  id: string
  bx: number; by: number   // base (zoom=1) px from the grid data origin
  latex: string
  // Box and rotation, like every other floating object. Absent = the equation is
  // laid out at its NATURAL size (what KaTeX renders); once resized, the rendered
  // formula is scaled to fill the box, so the geometry contract is the same as a
  // shape's and the shared selection chrome / group commands apply unchanged.
  bw?: number; bh?: number
  rot?: number
}

// Chart types. Legacy values: 'bar' = vertical columns, 'hbar' = horizontal.
// v2 additions: 'donut', 'radar', 'combo' (column + line).
export type ChartType = 'bar' | 'hbar' | 'line' | 'area' | 'pie' | 'scatter' | 'donut' | 'radar' | 'combo'

// One chart series. Imports use flat refs (vals/cats); UI-authored v2 series
// use A1 ranges (valsRange/xRange) which stay live when cells change.
export interface SheetChartSeries {
  name?: string          // literal name (xlsx strCache or user text)
  nameRef?: string       // cell ref holding the name, e.g. "B1" (v2)
  vals?: string[]        // flat value refs (xlsx import shape)
  cats?: string[]        // flat per-series category refs (xlsx import shape)
  valsRange?: string     // rectangular value range "B2:B10" (v2)
  xRange?: string        // scatter X-values range (v2)
  color?: string         // "#RRGGBB"
  kind?: 'column' | 'line'          // combo per-series override (v2)
  axis?: 'primary' | 'secondary'    // reserved: secondary value axis (v2)
}

export interface SheetChartAxis {
  title?: string
  min?: number           // manual bound; absent = auto scale
  max?: number
  format?: string        // number format for tick labels
}

export interface SheetChart {
  id?: string
  type: ChartType
  title?: string
  // UI-created charts: an absolute box + a rectangular range.
  bx?: number; by?: number; bw?: number; bh?: number
  rot?: number             // rotation in degrees, like pictures (xlsx a:xfrm rot)
  // Chart-area formatting (xlsx chartSpace spPr / txPr, graphicFrame descr).
  fill?: string            // "#RRGGBB" or "none"
  border?: string          // outline colour "#RRGGBB" or "none"
  borderWidth?: number     // outline width in px
  font?: string            // font family for every chart text
  altText?: string         // accessibility description
  range?: string           // "A1:B6" (legacy: 1st col = labels, 2nd = values)
  // Imported charts: a cell anchor (EMU, like images) + explicit cell refs.
  fromCol?: number; fromColOff?: number; fromRow?: number; fromRowOff?: number
  toCol?: number; toColOff?: number; toRow?: number; toRowOff?: number
  extCx?: number; extCy?: number
  vals?: string[]; cats?: string[]   // bare cell refs e.g. ["R18","R19","R20"]
  // Presentation options (imported from the chart part).
  legend?: boolean
  dataLabels?: 'value' | 'percent'
  colors?: string[]                  // explicit per-slice colours "#RRGGBB"
  // Multi-series data (was persisted-but-untyped for xlsx imports; typed since v2).
  series?: SheetChartSeries[]
  grouping?: 'stacked' | 'percentStacked'
  // ── v2 additions (all optional; absent on legacy workbooks) ──
  // Simple-range interpretation (wizard step 2); absent = auto-detect.
  seriesIn?: 'cols' | 'rows'
  firstRowHeader?: boolean
  firstColHeader?: boolean
  catsRange?: string                 // explicit categories range "A2:A10"
  // Elements (wizard step 4).
  legendPos?: 'right' | 'left' | 'top' | 'bottom'   // default 'right'
  axisX?: SheetChartAxis
  axisY?: SheetChartAxis
  grid?: { x?: boolean; y?: boolean }               // default: y major only
  // Line/scatter styling.
  lines?: boolean; symbols?: boolean; smooth?: boolean
  // Pie/donut.
  holeSize?: number                  // 0..1 inner-radius fraction
  explode?: number                   // slice offset fraction of the radius
  // Radar.
  filled?: boolean
  // Combo: number of trailing series drawn as lines (default 1).
  numLines?: number
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
  // Picture formatting / properties (xlsx pic spPr ln, cNvPr descr + hlinkClick).
  border?: string          // outline colour "#RRGGBB" or "none"
  borderWidth?: number     // outline width in px
  shadow?: boolean         // drop shadow behind the picture
  altText?: string         // accessibility description
  link?: string            // hyperlink target (URL, or "#Sheet!A1" internally)
}

// Preset geometries a sheet shape can take. Deliberately a small, round set — the
// same ten Excel offers on its first shapes row — each mapping 1:1 to an OOXML
// `a:prstGeom` preset so the xlsx round-trip stays lossless:
//   rect · roundRect · ellipse · triangle · diamond · rightArrow · line · star5 ·
//   wedgeRectCallout · mathPlus
// Les 144 formes du catalogue partagé (voir shapes/catalog.ts) : le tableur et
// l'éditeur de documents dessinent le même jeu.
import type { ShapeKind } from './shapes/catalog'
/**
 * Les 144 formes du catalogue partagé, plus deux alias HISTORIQUES du tableur
 * (`callout`, `plus`) : des feuilles enregistrées les portent déjà, on ne peut pas
 * les renommer sans réécrire les données existantes.
 */
export type SheetShapeKind = ShapeKind | 'callout' | 'plus'

/** Text formatting of a shape's caption (shape-wide: OOXML runs are not modelled). */
export interface SheetShapeTextStyle {
  bold?: boolean
  italic?: boolean
  color?: string           // "#RRGGBB"
  size?: number            // pt
  align?: 'left' | 'center' | 'right'
}

/**
 * A drawing shape floating over the grid. Geometry follows the same contract as
 * charts and pictures — an explicit box in base (zoom = 1) pixels measured from the
 * grid data origin, plus a rotation in degrees — so the shared direct-manipulation
 * helpers (`sheet-chart/interaction.ts`) apply unchanged.
 */
export interface SheetShape {
  id: string
  kind: SheetShapeKind
  bx: number; by: number; bw: number; bh: number
  rot?: number             // degrees
  fill?: string            // "#RRGGBB" or "none"
  border?: string          // outline colour "#RRGGBB" or "none"
  borderWidth?: number     // outline width in px
  text?: string            // caption drawn inside the shape
  textStyle?: SheetShapeTextStyle
  altText?: string         // accessibility description
  link?: string            // hyperlink target
  // Mirroring, like LibreOffice's Flip menu (xlsx a:xfrm flipH / flipV).
  flipH?: boolean
  flipV?: boolean
  /** How the shape follows the cells underneath it (Calc's anchoring). */
  anchor?: 'cell' | 'cellResize' | 'page'
  /** Object name — shown in the Name Box and set by « Renommer l'objet… ». */
  name?: string
  /**
   * Adjustment fractions (OOXML `avLst`): the yellow knobs that reshape a geometry
   * without resizing it — arrow head, corner radius, star waist, callout tail.
   */
  adj?: number[]
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

  /** Write the workbook back into the file it was imported from, in that format. */
  saveToSource: (id: string) =>
    api.post<{ saved: boolean; format: string }>(`/office/spreadsheets/${id}/save-source`).then(r => r.data),

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
    api.get<{ sheet: SpreadsheetSheet; names?: Record<string, string>; data?: { cells?: Record<string, CellData>; col_widths?: Record<string, number>; row_heights?: Record<string, number>; frozen_rows?: number; frozen_cols?: number; merges?: string[]; cf?: SheetData['cf']; validations?: SheetData['validations']; row_groups?: SheetData['rowGroups']; col_groups?: SheetData['colGroups']; gridlines?: boolean; default_row_height?: number | null; default_col_width?: number | null; col_styles?: SheetData['colStyles']; row_styles?: SheetData['rowStyles']; images?: SheetData['images']; equations?: SheetData['equations']; charts?: SheetData['charts']; shapes?: SheetData['shapes']; pivots?: SheetData['pivots']; protection?: SheetData['protection']; auto_filter?: string; enc?: SheetEncEnvelope } }>(`/office/spreadsheets/${ssId}/sheets/${sheetId}`)
      .then(async r => {
        const enc = r.data.data?.enc ?? null
        // Decrypt cells if the workbook is unlocked this session; otherwise leave them empty
        // and surface `enc` so the editor can show the unlock gate.
        let cells: Record<string, CellData> = r.data.data?.cells ?? {}
        if (enc) {
          if (isUnlocked(ssId)) { try { cells = await decryptCells(ssId, enc) } catch { cells = {} } }
          else cells = {}
        }
        return {
          ...r.data.sheet,
          data: { cells, merges: r.data.data?.merges ?? [], cf: r.data.data?.cf ?? [], validations: r.data.data?.validations ?? [], rowGroups: r.data.data?.row_groups ?? [], colGroups: r.data.data?.col_groups ?? [], gridlines: r.data.data?.gridlines !== false, defaultRowHeight: r.data.data?.default_row_height ?? undefined, defaultColWidth: r.data.data?.default_col_width ?? undefined, colStyles: r.data.data?.col_styles ?? {}, rowStyles: r.data.data?.row_styles ?? {}, images: r.data.data?.images ?? [], equations: r.data.data?.equations ?? [], charts: r.data.data?.charts ?? [], shapes: r.data.data?.shapes ?? [], pivots: r.data.data?.pivots ?? [], protection: r.data.data?.protection ?? null, autoFilter: r.data.data?.auto_filter ?? undefined, enc },
          col_widths:  r.data.data?.col_widths  ?? {},
          row_heights: r.data.data?.row_heights ?? {},
          frozen_rows: r.data.data?.frozen_rows ?? 0,
          frozen_cols: r.data.data?.frozen_cols ?? 0,
          names: r.data.names ?? {},
        } as SpreadsheetSheet & { names?: Record<string, string> }
      }),

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
    shapes?: SheetShape[]
    cf?: SheetData['cf']
    validations?: SheetData['validations']
    row_groups?: SheetData['rowGroups']
    col_groups?: SheetData['colGroups']
    enc?: SheetEncEnvelope | null
    clear_enc?: boolean
  }, opts?: { plaintext?: boolean }) => (async () => {
    // Transparent encryption: when the workbook is unlocked, replace the clear cells with an
    // Agile envelope before they ever reach the server. `opts.plaintext` forces clear storage
    // (used when removing encryption) and drops the stored envelope via the clear_enc flag
    // (JSON null would deserialize to None server-side and leave the envelope in place).
    let payload = data
    if (opts?.plaintext) {
      payload = { ...data, clear_enc: true }
    } else if (data.data?.cells && isUnlocked(ssId)) {
      const env = await encryptCells(ssId, data.data.cells)
      payload = { ...data, data: { ...data.data, cells: {} }, enc: env }
    }
    return api.patch<{ sheet: SpreadsheetSheet; data?: { cells?: Record<string, CellData> } }>(`/office/spreadsheets/${ssId}/sheets/${sheetId}`, payload)
      .then(r => ({ ...r.data.sheet, data: { cells: r.data.data?.cells ?? {} } } as SpreadsheetSheet))
  })(),

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
  /**
   * Valeurs d'ajustement (poignées jaunes, OOXML `avLst`) — partagées avec le
   * tableur et les documents via le moteur `shapes/`. Absent = géométrie par défaut.
   */
  adj?: number[]
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
  kind:           'management' | 'cloud'
  slug:           string | null
  labels:         Record<string, string>
  parent_id:      string | null
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
  /** Slack before this task pushes a successor (distinct from total float). */
  free_float:    number | null
  /** Scheduling constraint applied by the CPM pass (ASAP/ALAP need no date). */
  constraint_type: 'ASAP' | 'ALAP' | 'SNET' | 'SNLT' | 'FNET' | 'FNLT' | 'MSO' | 'MFO'
  constraint_date: string | null
  /** Target date that never moves the schedule; only flags lateness. */
  deadline_date:   string | null
  deadline_missed: boolean
  is_critical:   boolean
  cpm_dirty:     boolean
  estimated_hours: number | null
  spent_hours:     number | null
  version_id:    string | null
  created_at:    string
  updated_at:    string
  /** Budget at completion of this work package; null means never costed. */
  budget_cost:  number | null
}

export interface ProjectVersion {
  id:          string
  project_id:  string
  name:        string
  description: string
  start_date:  string | null
  due_date:    string | null
  status:      'open' | 'locked' | 'closed'
  position:    number
  created_at:  string
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
  /** What an hour of this resource costs; null falls back to the project rate. */
  hourly_rate: number | null
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

export interface ProjectCharter {
  id:                      string
  project_id:              string
  purpose:                 string
  business_case:           string
  objectives:              string
  success_criteria:        string
  high_level_requirements: string
  assumptions:             string
  constraints:             string
  risks_summary:           string
  budget_summary:          string
  sponsor:                 string
  pm_name:                 string
  pm_authority:            string
  /** An approved charter is read-only: reopening it files a dated revision. */
  status:                  'draft' | 'approved'
  approved_by:             string | null
  approved_at:             string | null
  created_at:              string
  updated_at:              string
}

/** The charter fields the editor writes; everything else is server-owned. */
export type CharterEdit = Partial<Pick<ProjectCharter,
  | 'purpose' | 'business_case' | 'objectives' | 'success_criteria'
  | 'high_level_requirements' | 'assumptions' | 'constraints'
  | 'risks_summary' | 'budget_summary' | 'sponsor' | 'pm_name' | 'pm_authority'>>

export interface CharterMilestone {
  id:          string
  charter_id:  string
  name:        string
  target_date: string | null
  position:    number
  /** Set once the milestone has been pushed into the schedule as a task. */
  task_id:     string | null
}

export interface CharterRevision {
  id:              string
  /** The charter as it stood when it was approved. Partial: an older snapshot
   *  predates any field added since, so nothing here is guaranteed present. */
  snapshot:        Partial<ProjectCharter>
  reason:          string
  revised_by:      string | null
  revised_by_name: string | null
  revised_at:      string
}

export interface ProjectCharterData {
  charter:          ProjectCharter
  milestones:       CharterMilestone[]
  revision_count:   number
  /** Display name (or e-mail) of the approver — never show the raw id. */
  approved_by_name: string | null
}

export const projectsApi = {
  list: (params?: { search?: string; starred?: boolean; trashed?: boolean; recent?: boolean; shared?: boolean }) =>
    api.get<{ projects: Project[]; total: number }>('/office/projects', { params }).then(r => r.data),

  create: (data?: { title?: string; color?: string; start_date?: string; end_date?: string; kind?: 'management' | 'cloud'; slug?: string; labels?: Record<string, string> }) =>
    api.post<{ project: Project }>('/office/projects', data ?? {}).then(r => r.data.project),

  get: (id: string) =>
    api.get<ProjectData>(`/office/projects/${id}`).then(r => r.data),

  update: (id: string, data: Partial<Pick<Project, 'title' | 'description' | 'color' | 'status' | 'start_date' | 'end_date' | 'is_starred' | 'labels' | 'parent_id'>>) =>
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

  updateTask: (projectId: string, taskId: string, data: Partial<Pick<ProjectTask, 'name' | 'description' | 'status' | 'priority' | 'task_type' | 'start_date' | 'end_date' | 'duration_days' | 'progress' | 'position' | 'wbs' | 'estimated_hours' | 'spent_hours' | 'budget_cost' | 'version_id' | 'constraint_type' | 'constraint_date' | 'deadline_date'>>) =>
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

  updateResource: (projectId: string, resourceId: string, data: Partial<Pick<ProjectResource, 'name' | 'role' | 'color' | 'capacity' | 'hourly_rate'>>) =>
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

  // Ressources d'un projet cloud : modules Kubuno rattachés.
  listModules: (id: string) =>
    api.get<{ modules: { module_id: string; added_at: string }[] }>(`/office/projects/${id}/modules`).then(r => r.data.modules),
  attachModule: (id: string, moduleId: string) =>
    api.post(`/office/projects/${id}/modules`, { module_id: moduleId }),
  detachModule: (id: string, moduleId: string) =>
    api.delete(`/office/projects/${id}/modules/${moduleId}`),

  // Plans de référence (baseline) : photo du planning prévu, pour comparer prévu vs réel.
  listBaselines: (id: string) =>
    api.get<{ baselines: Baseline[] }>(`/office/projects/${id}/baselines`).then(r => r.data.baselines),
  captureBaseline: (id: string, name?: string) =>
    api.post<{ id: string; name: string; captured_at: string; task_count: number }>(`/office/projects/${id}/baselines`, name ? { name } : {}).then(r => r.data),
  deleteBaseline: (id: string, baselineId: string) =>
    api.delete(`/office/projects/${id}/baselines/${baselineId}`),
  updateBaseline: (id: string, baselineId: string, data: { name?: string; is_primary?: boolean }) =>
    api.patch<{ id: string; name: string; is_primary: boolean }>(`/office/projects/${id}/baselines/${baselineId}`, data).then(r => r.data),

  // Versions (roadmap / jalons de livraison).
  listVersions: (id: string) =>
    api.get<{ versions: ProjectVersion[] }>(`/office/projects/${id}/versions`).then(r => r.data.versions),
  createVersion: (id: string, data?: { name?: string; description?: string; start_date?: string; due_date?: string }) =>
    api.post<{ version: ProjectVersion }>(`/office/projects/${id}/versions`, data ?? {}).then(r => r.data.version),
  updateVersion: (id: string, versionId: string, data: Partial<Pick<ProjectVersion, 'name' | 'description' | 'start_date' | 'due_date' | 'status'>>) =>
    api.patch<{ version: ProjectVersion }>(`/office/projects/${id}/versions/${versionId}`, data).then(r => r.data.version),
  deleteVersion: (id: string, versionId: string) =>
    api.delete(`/office/projects/${id}/versions/${versionId}`),
  listCalendars: (id: string) =>
    api.get<ProjectCalendars>(`/office/projects/${id}/calendars`).then(r => r.data),
  createCalendar: (id: string, name?: string) =>
    api.post<{ calendar: ProjectCalendar }>(`/office/projects/${id}/calendars`, { name }).then(r => r.data.calendar),
  updateCalendar: (id: string, calendarId: string, data: Partial<Pick<ProjectCalendar, 'name' | 'mon' | 'tue' | 'wed' | 'thu' | 'fri' | 'sat' | 'sun'>> & { set_as_project_default?: boolean }) =>
    api.patch<{ calendar: ProjectCalendar }>(`/office/projects/${id}/calendars/${calendarId}`, data).then(r => r.data.calendar),
  deleteCalendar: (id: string, calendarId: string) =>
    api.delete(`/office/projects/${id}/calendars/${calendarId}`),
  setCalendarException: (id: string, calendarId: string, ex: { day: string; is_working: boolean; note?: string }) =>
    api.put(`/office/projects/${id}/calendars/${calendarId}/exceptions`, ex),
  removeCalendarException: (id: string, calendarId: string, day: string) =>
    api.delete(`/office/projects/${id}/calendars/${calendarId}/exceptions/${day}`),
  listTimeEntries: (id: string, taskId?: string) =>
    api.get<{ entries: TimeEntry[] }>(`/office/projects/${id}/time-entries`, { params: taskId ? { task_id: taskId } : undefined }).then(r => r.data.entries),
  createTimeEntry: (id: string, data: { task_id: string; spent_on?: string; hours: number; activity?: string; comment?: string }) =>
    api.post<{ entry: TimeEntry }>(`/office/projects/${id}/time-entries`, data).then(r => r.data.entry),
  updateTimeEntry: (id: string, entryId: string, data: Partial<Pick<TimeEntry, 'spent_on' | 'hours' | 'activity' | 'comment'>>) =>
    api.patch<{ entry: TimeEntry }>(`/office/projects/${id}/time-entries/${entryId}`, data).then(r => r.data.entry),
  deleteTimeEntry: (id: string, entryId: string) =>
    api.delete(`/office/projects/${id}/time-entries/${entryId}`),

  // Charte de projet : ce à quoi le projet s'engage, qui l'autorise, et ce que
  // « réussi » voudra dire. Approuvée, elle passe en lecture seule côté serveur.
  getCharter: (id: string) =>
    api.get<ProjectCharterData>(`/office/projects/${id}/charter`).then(r => r.data),
  updateCharter: (id: string, data: CharterEdit) =>
    api.put<{ charter: ProjectCharter }>(`/office/projects/${id}/charter`, data).then(r => r.data.charter),
  approveCharter: (id: string) =>
    api.post<{ charter: ProjectCharter }>(`/office/projects/${id}/charter/approve`).then(r => r.data.charter),
  reviseCharter: (id: string, reason?: string) =>
    api.post<{ charter: ProjectCharter }>(`/office/projects/${id}/charter/revise`, { reason: reason ?? '' }).then(r => r.data.charter),
  listCharterRevisions: (id: string) =>
    api.get<{ revisions: CharterRevision[] }>(`/office/projects/${id}/charter/revisions`).then(r => r.data.revisions),
  createCharterMilestone: (id: string, data: { name: string; target_date?: string | null; position?: number }) =>
    api.post<{ milestone: CharterMilestone }>(`/office/projects/${id}/charter/milestones`, data).then(r => r.data.milestone),
  updateCharterMilestone: (id: string, milestoneId: string, data: Partial<Pick<CharterMilestone, 'name' | 'target_date' | 'position'>>) =>
    api.patch<{ milestone: CharterMilestone }>(`/office/projects/${id}/charter/milestones/${milestoneId}`, data).then(r => r.data.milestone),
  deleteCharterMilestone: (id: string, milestoneId: string) =>
    api.delete(`/office/projects/${id}/charter/milestones/${milestoneId}`),
  /** Turn the charter's milestones into real milestones in the schedule (idempotent). */
  generateCharterMilestones: (id: string) =>
    api.post<{ created: number; updated: number }>(`/office/projects/${id}/charter/milestones/generate`).then(r => r.data),

  // ── Scope ──────────────────────────────────────────────────────────────────
  // The breakdown, numbered from the tree itself; the promises it produces; and
  // the requirements each promise answers to.
  getWbs: (id: string) =>
    api.get<{ elements: WbsElement[] }>(`/office/projects/${id}/wbs`).then(r => r.data.elements),
  renumberWbs: (id: string) =>
    api.post(`/office/projects/${id}/wbs/renumber`),
  /** Null when nobody has described this work package yet — that absence is the point. */
  getDictionaryEntry: (id: string, taskId: string) =>
    api.get<{ entry: WbsDictionaryEntry | null }>(`/office/projects/${id}/tasks/${taskId}/dictionary`).then(r => r.data.entry),
  updateDictionaryEntry: (id: string, taskId: string, data: WbsDictionaryEdit) =>
    api.put<{ entry: WbsDictionaryEntry }>(`/office/projects/${id}/tasks/${taskId}/dictionary`, data).then(r => r.data.entry),

  listDeliverables: (id: string) =>
    api.get<{ deliverables: Deliverable[] }>(`/office/projects/${id}/deliverables`).then(r => r.data.deliverables),
  createDeliverable: (id: string, data: DeliverableEdit & { name: string }) =>
    api.post<{ deliverable: Deliverable }>(`/office/projects/${id}/deliverables`, data).then(r => r.data.deliverable),
  updateDeliverable: (id: string, deliverableId: string, data: DeliverableEdit) =>
    api.patch<{ deliverable: Deliverable }>(`/office/projects/${id}/deliverables/${deliverableId}`, data).then(r => r.data.deliverable),
  deleteDeliverable: (id: string, deliverableId: string) =>
    api.delete(`/office/projects/${id}/deliverables/${deliverableId}`),
  /** Owner only: a deliverable nobody accepted is not done. */
  acceptDeliverable: (id: string, deliverableId: string) =>
    api.post<{ deliverable: Deliverable }>(`/office/projects/${id}/deliverables/${deliverableId}/accept`).then(r => r.data.deliverable),
  rejectDeliverable: (id: string, deliverableId: string, reason: string) =>
    api.post<{ deliverable: Deliverable }>(`/office/projects/${id}/deliverables/${deliverableId}/reject`, { reason }).then(r => r.data.deliverable),

  listRequirements: (id: string) =>
    api.get<{ requirements: Requirement[] }>(`/office/projects/${id}/requirements`).then(r => r.data.requirements),
  createRequirement: (id: string, data: RequirementEdit & { title: string }) =>
    api.post<{ requirement: Requirement }>(`/office/projects/${id}/requirements`, data).then(r => r.data.requirement),
  updateRequirement: (id: string, requirementId: string, data: RequirementEdit) =>
    api.patch<{ requirement: Requirement }>(`/office/projects/${id}/requirements/${requirementId}`, data).then(r => r.data.requirement),
  deleteRequirement: (id: string, requirementId: string) =>
    api.delete(`/office/projects/${id}/requirements/${requirementId}`),
  linkRequirement: (id: string, requirementId: string, data: { deliverable_id?: string; task_id?: string }) =>
    api.post<{ link: RequirementLink; existing: boolean }>(`/office/projects/${id}/requirements/${requirementId}/links`, data).then(r => r.data),
  unlinkRequirement: (id: string, requirementId: string, linkId: string) =>
    api.delete(`/office/projects/${id}/requirements/${requirementId}/links/${linkId}`),
  getTraceability: (id: string) =>
    api.get<TraceabilityMatrix>(`/office/projects/${id}/traceability`).then(r => r.data),

  // ── Risks and issues ───────────────────────────────────────────────────────
  // What might still happen, and what already has.
  getRisks: (id: string) =>
    api.get<RiskRegister>(`/office/projects/${id}/risks`).then(r => r.data),
  createRisk: (id: string, data: RiskEdit & { title: string }) =>
    api.post<{ risk: Risk }>(`/office/projects/${id}/risks`, data).then(r => r.data.risk),
  updateRisk: (id: string, riskId: string, data: RiskEdit) =>
    api.patch<{ risk: Risk }>(`/office/projects/${id}/risks/${riskId}`, data).then(r => r.data.risk),
  deleteRisk: (id: string, riskId: string) =>
    api.delete(`/office/projects/${id}/risks/${riskId}`),
  /** The risk came true: opens an issue for it and marks the risk as occurred. */
  materializeRisk: (id: string, riskId: string) =>
    api.post<{ issue: Issue; existing: boolean }>(`/office/projects/${id}/risks/${riskId}/materialize`).then(r => r.data),

  getIssues: (id: string) =>
    api.get<IssueLog>(`/office/projects/${id}/issues`).then(r => r.data),
  createIssue: (id: string, data: IssueEdit & { title: string }) =>
    api.post<{ issue: Issue }>(`/office/projects/${id}/issues`, data).then(r => r.data.issue),
  updateIssue: (id: string, issueId: string, data: IssueEdit) =>
    api.patch<{ issue: Issue }>(`/office/projects/${id}/issues/${issueId}`, data).then(r => r.data.issue),
  deleteIssue: (id: string, issueId: string) =>
    api.delete(`/office/projects/${id}/issues/${issueId}`),

  // ── Cost and earned value ──────────────────────────────────────────────────
  getCosts: (id: string) =>
    api.get<CostOverview>(`/office/projects/${id}/costs`).then(r => r.data),
  updateCostConfig: (id: string, data: Partial<Pick<CostConfig, 'currency' | 'eac_method' | 'default_hourly_rate' | 'status_date' | 'manual_etc'>>) =>
    api.put<{ config: CostConfig }>(`/office/projects/${id}/costs/config`, data).then(r => r.data.config),
  listCostEntries: (id: string) =>
    api.get<{ entries: CostEntry[]; total: number }>(`/office/projects/${id}/costs/entries`).then(r => r.data),
  createCostEntry: (id: string, data: CostEntryEdit & { amount: number }) =>
    api.post<{ entry: CostEntry }>(`/office/projects/${id}/costs/entries`, data).then(r => r.data.entry),
  updateCostEntry: (id: string, entryId: string, data: CostEntryEdit) =>
    api.patch<{ entry: CostEntry }>(`/office/projects/${id}/costs/entries/${entryId}`, data).then(r => r.data.entry),
  deleteCostEntry: (id: string, entryId: string) =>
    api.delete(`/office/projects/${id}/costs/entries/${entryId}`),

  // ── Stakeholders and RACI ──────────────────────────────────────────────────
  getStakeholders: (id: string) =>
    api.get<StakeholderRegister>(`/office/projects/${id}/stakeholders`).then(r => r.data),
  createStakeholder: (id: string, data: StakeholderEdit & { name: string }) =>
    api.post<{ stakeholder: Stakeholder }>(`/office/projects/${id}/stakeholders`, data).then(r => r.data.stakeholder),
  updateStakeholder: (id: string, holderId: string, data: StakeholderEdit) =>
    api.patch<{ stakeholder: Stakeholder }>(`/office/projects/${id}/stakeholders/${holderId}`, data).then(r => r.data.stakeholder),
  deleteStakeholder: (id: string, holderId: string) =>
    api.delete(`/office/projects/${id}/stakeholders/${holderId}`),

  getRaci: (id: string) =>
    api.get<RaciMatrix>(`/office/projects/${id}/raci`).then(r => r.data),
  /** Refused with a readable message when someone else is already accountable. */
  setRaciRole: (id: string, taskId: string, holderId: string, role: RaciRole) =>
    api.put<{ task_id: string; stakeholder_id: string; role: RaciRole }>(`/office/projects/${id}/tasks/${taskId}/raci/${holderId}`, { role }).then(r => r.data),
  clearRaciRole: (id: string, taskId: string, holderId: string) =>
    api.delete(`/office/projects/${id}/tasks/${taskId}/raci/${holderId}`),

  // ── Quality ────────────────────────────────────────────────────────────────
  getQuality: (id: string) =>
    api.get<QualityOverview>(`/office/projects/${id}/quality`).then(r => r.data),
  createQualityMetric: (id: string, data: QualityMetricEdit & { name: string }) =>
    api.post<{ metric: QualityMetric }>(`/office/projects/${id}/quality/metrics`, data).then(r => r.data.metric),
  updateQualityMetric: (id: string, metricId: string, data: QualityMetricEdit) =>
    api.patch<{ metric: QualityMetric }>(`/office/projects/${id}/quality/metrics/${metricId}`, data).then(r => r.data.metric),
  deleteQualityMetric: (id: string, metricId: string) =>
    api.delete(`/office/projects/${id}/quality/metrics/${metricId}`),
  addQualityMeasurement: (id: string, metricId: string, data: { value: number; measured_on?: string; notes?: string }) =>
    api.post<{ id: string; value: number; conforms: boolean | null }>(`/office/projects/${id}/quality/metrics/${metricId}/measurements`, data).then(r => r.data),
  deleteQualityMeasurement: (id: string, metricId: string, readingId: string) =>
    api.delete(`/office/projects/${id}/quality/metrics/${metricId}/measurements/${readingId}`),

  listQualityChecks: (id: string) =>
    api.get<{ checks: QualityCheck[] }>(`/office/projects/${id}/quality/checks`).then(r => r.data.checks),
  createQualityCheck: (id: string, data: QualityCheckEdit & { label: string }) =>
    api.post<{ check: QualityCheck }>(`/office/projects/${id}/quality/checks`, data).then(r => r.data.check),
  updateQualityCheck: (id: string, checkId: string, data: QualityCheckEdit) =>
    api.patch<{ check: QualityCheck }>(`/office/projects/${id}/quality/checks/${checkId}`, data).then(r => r.data.check),
  deleteQualityCheck: (id: string, checkId: string) =>
    api.delete(`/office/projects/${id}/quality/checks/${checkId}`),

  // ── Communications and decisions ───────────────────────────────────────────
  getCommunications: (id: string) =>
    api.get<CommunicationPlan>(`/office/projects/${id}/communications`).then(r => r.data),
  createCommunication: (id: string, data: CommunicationEdit & { name: string }) =>
    api.post<{ communication: Communication }>(`/office/projects/${id}/communications`, data).then(r => r.data.communication),
  updateCommunication: (id: string, commId: string, data: CommunicationEdit) =>
    api.patch<{ communication: Communication }>(`/office/projects/${id}/communications/${commId}`, data).then(r => r.data.communication),
  deleteCommunication: (id: string, commId: string) =>
    api.delete(`/office/projects/${id}/communications/${commId}`),
  /** Records a send and moves the next date forward by the frequency. */
  logCommunication: (id: string, data: { communication_id?: string; sent_on?: string; summary?: string }) =>
    api.post<{ id: string; sent_on: string; next_due: string | null }>(`/office/projects/${id}/communications/log`, data).then(r => r.data),
  listCommunicationLog: (id: string) =>
    api.get<{ entries: CommunicationLogEntry[] }>(`/office/projects/${id}/communications/log`).then(r => r.data.entries),

  getDecisions: (id: string) =>
    api.get<DecisionLog>(`/office/projects/${id}/decisions`).then(r => r.data),
  createDecision: (id: string, data: DecisionEdit & { title: string }) =>
    api.post<{ decision: Decision }>(`/office/projects/${id}/decisions`, data).then(r => r.data.decision),
  updateDecision: (id: string, decisionId: string, data: DecisionEdit) =>
    api.patch<{ decision: Decision }>(`/office/projects/${id}/decisions/${decisionId}`, data).then(r => r.data.decision),
  deleteDecision: (id: string, decisionId: string) =>
    api.delete(`/office/projects/${id}/decisions/${decisionId}`),

  // ── Change control ─────────────────────────────────────────────────────────
  getChanges: (id: string) =>
    api.get<ChangeLog>(`/office/projects/${id}/changes`).then(r => r.data),
  createChange: (id: string, data: ChangeRequestEdit & { title: string }) =>
    api.post<{ change: ChangeRequest }>(`/office/projects/${id}/changes`, data).then(r => r.data.change),
  updateChange: (id: string, changeId: string, data: ChangeRequestEdit) =>
    api.patch<{ change: ChangeRequest }>(`/office/projects/${id}/changes/${changeId}`, data).then(r => r.data.change),
  deleteChange: (id: string, changeId: string) =>
    api.delete(`/office/projects/${id}/changes/${changeId}`),
  /** Records what the change would cost. At least one dimension must say something. */
  assessChange: (id: string, changeId: string, data: ChangeAssessment) =>
    api.post<{ change: ChangeRequest }>(`/office/projects/${id}/changes/${changeId}/assess`, data).then(r => r.data.change),
  /** Owner only. Approving an unassessed change is refused. */
  decideChange: (id: string, changeId: string, data: { status: ChangeStatus; decision_note?: string; baseline_id?: string | null }) =>
    api.post<{ change: ChangeRequest }>(`/office/projects/${id}/changes/${changeId}/decide`, data).then(r => r.data.change),

  // ── Closure and lessons learned ────────────────────────────────────────────
  getClosure: (id: string) =>
    api.get<ClosureOverview>(`/office/projects/${id}/closure`).then(r => r.data),
  updateClosure: (id: string, data: ClosureEdit) =>
    api.put<{ closure: Closure }>(`/office/projects/${id}/closure`, data).then(r => r.data.closure),
  /** Owner only. Refused without a reason while blocking checks fail, and refused
   *  outright until the charter's question is answered. */
  closeProject: (id: string, data?: { override_reason?: string; closed_on?: string }) =>
    api.post<{ closure: Closure; closed_with_open_points: number }>(`/office/projects/${id}/closure/close`, data ?? {}).then(r => r.data),
  reopenProject: (id: string) =>
    api.post<{ closure: Closure }>(`/office/projects/${id}/closure/reopen`).then(r => r.data.closure),

  listLessons: (id: string) =>
    api.get<LessonLog>(`/office/projects/${id}/lessons`).then(r => r.data),
  createLesson: (id: string, data: LessonEdit & { title: string }) =>
    api.post<{ lesson: Lesson }>(`/office/projects/${id}/lessons`, data).then(r => r.data.lesson),
  updateLesson: (id: string, lessonId: string, data: LessonEdit) =>
    api.patch<{ lesson: Lesson }>(`/office/projects/${id}/lessons/${lessonId}`, data).then(r => r.data.lesson),
  deleteLesson: (id: string, lessonId: string) =>
    api.delete(`/office/projects/${id}/lessons/${lessonId}`),

  // ── Procurement ────────────────────────────────────────────────────────────
  getProcurement: (id: string) =>
    api.get<ProcurementRegister>(`/office/projects/${id}/procurement`).then(r => r.data),
  createProcurement: (id: string, data: ProcurementEdit & { title: string }) =>
    api.post<{ contract: Procurement }>(`/office/projects/${id}/procurement`, data).then(r => r.data.contract),
  /** Awarding without a committed value is refused. */
  updateProcurement: (id: string, contractId: string, data: ProcurementEdit) =>
    api.patch<{ contract: Procurement }>(`/office/projects/${id}/procurement/${contractId}`, data).then(r => r.data.contract),
  deleteProcurement: (id: string, contractId: string) =>
    api.delete(`/office/projects/${id}/procurement/${contractId}`),
  addPayment: (id: string, contractId: string, data: PaymentEdit & { amount: number }) =>
    api.post<{ payment: ProcurementPayment }>(`/office/projects/${id}/procurement/${contractId}/payments`, data).then(r => r.data.payment),
  updatePayment: (id: string, contractId: string, paymentId: string, data: PaymentEdit) =>
    api.patch<{ payment: ProcurementPayment }>(`/office/projects/${id}/procurement/${contractId}/payments/${paymentId}`, data).then(r => r.data.payment),
  deletePayment: (id: string, contractId: string, paymentId: string) =>
    api.delete(`/office/projects/${id}/procurement/${contractId}/payments/${paymentId}`),

  // ── Portfolio ──────────────────────────────────────────────────────────────
  /** Every project the user can see, judged by the same measures. Reads the
   *  registers the projects already keep; nothing is stored. */
  getPortfolio: () =>
    api.get<Portfolio>('/office/portfolio').then(r => r.data),

  // ── Subsidiary management plans ────────────────────────────────────────────
  getPlans: (id: string) =>
    api.get<ManagementPlans>(`/office/projects/${id}/plans`).then(r => r.data),
  /** A threshold offered on a plan that does not read it is refused, not stored. */
  savePlan: (id: string, area: PlanArea, data: ManagementPlanEdit) =>
    api.put<{ plan: ManagementPlan }>(`/office/projects/${id}/plans/${area}`, data).then(r => r.data.plan),
  deletePlan: (id: string, area: PlanArea) =>
    api.delete(`/office/projects/${id}/plans/${area}`),

  // ── Document production ─────────────────────────────────────────────────────
  getProducibleDocs: (id: string) =>
    api.get<{ documents: ProducibleDoc[] }>(`/office/projects/${id}/documents/available`).then(r => r.data.documents),
  /** Builds the document from the registers and drops it in Drive as a real,
   *  editable Kubuno document. Returns the id to open it with. */
  produceDocument: (id: string, kind: ProducibleDocKind) =>
    api.post<{ file_id: string; document_id: string; title: string }>(`/office/projects/${id}/documents`, { kind }).then(r => r.data),

  // Tailoring: which artifacts (views) the project uses and how it is run.
  getSettings: (id: string) =>
    api.get<ProjectSettings>(`/office/projects/${id}/settings`).then(r => r.data),
  updateSettings: (id: string, data: { methodology?: ProjectMethodology; artifacts?: Partial<Record<ProjectArtifactKey, { enabled: boolean; config?: Record<string, unknown> }>> }) =>
    api.put<ProjectSettings>(`/office/projects/${id}/settings`, data).then(r => r.data),
}

export type ProjectMethodology = 'predictive' | 'agile' | 'hybrid'
export type ProjectArtifactKey = 'schedule' | 'board' | 'calendar' | 'workload' | 'network' | 'roadmap' | 'baselines' | 'timelog' | 'charter' | 'wbs' | 'deliverables' | 'requirements' | 'risks' | 'issues' | 'costs' | 'stakeholders' | 'quality' | 'communications' | 'decisions' | 'changes' | 'closure' | 'procurement' | 'plans'

// ── Scope: breakdown, deliverables, requirements ─────────────────────────────

/** One entry of the WBS dictionary: what a work package covers — and does not. */
export interface WbsDictionaryEntry {
  id:                   string
  task_id:              string
  code_of_account:      string
  statement_of_work:    string
  acceptance_criteria:  string
  assumptions:          string
  /** What is explicitly out of scope: the line drawn against scope creep. */
  exclusions:           string
  quality_requirements: string
  risks:                string
  responsible:          string
  created_at:           string
  updated_at:           string
}

export type WbsDictionaryEdit = Partial<Omit<WbsDictionaryEntry, 'id' | 'task_id' | 'created_at' | 'updated_at'>>

/** A node of the breakdown, carrying its outline number (1, 1.2, 1.2.3…). */
export interface WbsElement {
  id:                string
  parent_id:         string | null
  position:          number
  /** Outline number, derived from the tree — never entered by hand. */
  wbs:               string
  name:              string
  task_type:         'task' | 'milestone' | 'summary'
  progress:          number
  deliverable_count: number
  has_dictionary:    boolean
  dictionary:        WbsDictionaryEntry | null
}

export type DeliverableStatus = 'planned' | 'in_progress' | 'delivered' | 'accepted' | 'rejected'

/** What the project hands over, followed through to acceptance. */
export interface Deliverable {
  id:                  string
  project_id:          string
  /** The work package that produces it; kept null when the task is removed. */
  task_id:             string | null
  task_name:           string | null
  /** Who accepted it, named rather than an identifier. */
  accepted_by_name:    string | null
  code:                string
  name:                string
  description:         string
  acceptance_criteria: string
  due_date:            string | null
  status:              DeliverableStatus
  accepted_by:         string | null
  accepted_at:         string | null
  rejection_reason:    string
  position:            number
  created_at:          string
  updated_at:          string
}

export type DeliverableEdit = Partial<Pick<Deliverable,
  'code' | 'name' | 'description' | 'acceptance_criteria' | 'due_date' | 'status' | 'task_id' | 'position'>>

export type RequirementType = 'business' | 'stakeholder' | 'functional' | 'non_functional' | 'transition' | 'quality' | 'project'
/** MoSCoW: a vocabulary that forces a real ranking instead of everything being "high". */
export type RequirementPriority = 'must' | 'should' | 'could' | 'wont'
export type RequirementStatus = 'proposed' | 'approved' | 'implemented' | 'verified' | 'deferred' | 'rejected'
export type VerificationMethod = 'test' | 'inspection' | 'demonstration' | 'analysis'

/** One traced line: a requirement reaches a deliverable, a work package, or both. */
export interface RequirementLink {
  id:               string
  requirement_id:   string
  deliverable_id:   string | null
  deliverable_name: string | null
  task_id:          string | null
  task_name:        string | null
  created_at:       string
}

export interface Requirement {
  id:                  string
  project_id:          string
  code:                string
  title:               string
  description:         string
  req_type:            RequirementType
  priority:            RequirementPriority
  /** Where it comes from: the stakeholder, the business need, the regulation. */
  source:              string
  rationale:           string
  status:              RequirementStatus
  verification_method: VerificationMethod
  verification_notes:  string
  verified_at:         string | null
  position:            number
  created_at:          string
  updated_at:          string
  links:               RequirementLink[]
}

export type RequirementEdit = Partial<Pick<Requirement,
  'code' | 'title' | 'description' | 'req_type' | 'priority' | 'source' | 'rationale'
  | 'status' | 'verification_method' | 'verification_notes' | 'verified_at' | 'position'>>

/** The traceability matrix. Its point is `orphans`: what nothing accounts for. */
export interface TraceabilityMatrix {
  requirements: Requirement[]
  orphans: {
    /** Requirements nothing realises. */
    requirements: Array<Pick<Requirement, 'id' | 'code' | 'title' | 'priority' | 'status'>>
    /** Deliverables no requirement justifies. */
    deliverables: Array<Pick<Deliverable, 'id' | 'code' | 'name' | 'status' | 'task_id'>>
  }
  summary: {
    total: number
    traced: number
    untraced: number
    by_priority: Record<RequirementPriority, number>
    verified: number
  }
}

// ── Risks and issues ─────────────────────────────────────────────────────────

export type RiskKind = 'threat' | 'opportunity'
export type RiskCategory = 'technical' | 'external' | 'organizational' | 'management' | 'commercial'
export type RiskStatus = 'identified' | 'analysing' | 'responding' | 'occurred' | 'closed'
/** Threats are avoided/mitigated/transferred, opportunities exploited/enhanced/shared. */
export type RiskStrategy = 'avoid' | 'mitigate' | 'transfer' | 'exploit' | 'enhance' | 'share' | 'accept' | 'escalate'

export interface Risk {
  id:                string
  project_id:        string
  code:              string
  title:             string
  description:       string
  category:          RiskCategory
  kind:              RiskKind
  /** 1 to 5. */
  probability:       number
  impact:            number
  /** probability × impact, computed by the database. */
  score:             number
  probability_pct:   number | null
  monetary_impact:   number | null
  /** Expected monetary value: negative for a threat, positive for an opportunity. */
  emv:               number | null
  status:            RiskStatus
  owner_id:          string | null
  owner_name:        string | null
  /** The early warning: what tells you it is about to happen. */
  trigger_signs:     string
  response_strategy: RiskStrategy
  response_plan:     string
  /** What is still there once the response has been carried out. */
  residual_notes:    string
  /** Set when this risk was created by responding to another one. */
  parent_risk_id:    string | null
  parent_code:       string | null
  parent_title:      string | null
  task_id:           string | null
  task_name:         string | null
  identified_at:     string | null
  closed_at:         string | null
  position:          number
  created_at:        string
  updated_at:        string
}

export type RiskEdit = Partial<Omit<Risk,
  'id' | 'project_id' | 'score' | 'emv' | 'owner_name' | 'parent_code' | 'parent_title'
  | 'task_name' | 'closed_at' | 'created_at' | 'updated_at'>>

export interface RiskRegister {
  risks: Risk[]
  /** From the risk management plan: the score above which a risk is escalated
   *  rather than merely owned. Null when the project set none — nothing is then
   *  presumed. Read only from an ACTIVE plan. */
  risk_appetite_score: number | null
  /** The open risks at or above that score. */
  to_escalate: Array<{ id: string; code: string; title: string; score: number }>
  /**
   * 5×5 counts, matrix[probability-1][impact-1]. Risks that are closed OR that
   * have already occurred are left out: the matrix only plots what may still
   * happen, and something that happened is an issue, not a probability.
   */
  matrix: number[][]
  summary: {
    total: number; open: number
    /** Risks that came true — excluded from `open`, from the matrix and from the EMV. */
    occurred: number
    threats: number; opportunities: number
    /** Expected monetary value, summed over the risks still ahead only. */
    total_emv: number; priced: number
  }
}

export type IssueStatus = 'open' | 'in_progress' | 'resolved' | 'closed'

export interface Issue {
  id:          string
  project_id:  string
  code:        string
  title:       string
  description: string
  /** 1 to 5. */
  severity:    number
  status:      IssueStatus
  owner_id:    string | null
  owner_name:  string | null
  due_date:    string | null
  resolution:  string
  resolved_at: string | null
  /** The risk this issue is the realisation of, when it had been foreseen. */
  risk_id:     string | null
  risk_code:   string | null
  risk_title:  string | null
  task_id:     string | null
  task_name:   string | null
  position:    number
  created_at:  string
  updated_at:  string
}

export type IssueEdit = Partial<Omit<Issue,
  'id' | 'project_id' | 'owner_name' | 'risk_code' | 'risk_title' | 'task_name'
  | 'resolved_at' | 'created_at' | 'updated_at'>>

export interface IssueLog {
  issues: Issue[]
  summary: {
    total: number; open: number
    /** Unresolved and past its date — the number that actually demands something. */
    overdue: number
    by_severity: Record<string, number>
  }
}

// ── Cost and earned value ────────────────────────────────────────────────────

export type CostCategory = 'labour' | 'subcontract' | 'licence' | 'hardware' | 'travel' | 'other'
/** How the final cost is forecast. The three give very different answers. */
export type EacMethod = 'cpi' | 'budget' | 'cpi_spi' | 'manual'

export interface CostConfig {
  project_id:          string
  currency:            string
  /** Applied to logged hours when the resource has no rate of its own. */
  default_hourly_rate: number | null
  /** The date the measurement is taken at; null means today. */
  status_date:         string | null
  eac_method:          EacMethod
  /** Bottom-up re-estimate of the remaining work, used by the 'manual' method. */
  manual_etc:          number | null
}

export interface CostEntry {
  id:          string
  project_id:  string
  task_id:     string | null
  task_name:   string | null
  incurred_on: string
  amount:      number
  category:    CostCategory
  /** Cost of quality, when the expense is one. Most are neither. */
  coq_category: CoqCategory | null
  description: string
  created_at:  string
}

export type CostEntryEdit = Partial<Pick<CostEntry, 'amount' | 'category' | 'description' | 'incurred_on' | 'task_id' | 'coq_category'>>

/** One work package, measured. Indices are null when their divisor is zero. */
export interface CostTaskLine {
  task_id:  string
  wbs:      string
  name:     string
  bac:      number | null
  progress: number
  pv:       number
  ev:       number
  ac:       number
  cv:       number
  sv:       number
  cpi:      number | null
  spi:      number | null
}

/** A point of the S-curve.
 *
 *  `ac` is a real history — it follows the dates money was actually spent — and is
 *  null beyond the status date, where nothing is known yet.
 *
 *  `ev` is null EVERYWHERE except at `status_offset`. Progress is a state, not a
 *  history: the server knows what is earned today, not what was earned last month.
 *  The single point is deliberate — do not "fix" it by drawing a line. */
export interface CostCurvePoint {
  offset: number
  pv:     number
  ev:     number | null
  ac:     number | null
}

export interface CostOverview {
  config:      CostConfig
  status_date: string
  /** The day `curve[].offset` counts from — the project's start. */
  origin:      string
  /** The status date expressed as an offset, so the marker lands on a sample. */
  status_offset: number
  totals: {
    /** Budget at completion. */
    bac:  number
    /** Planned value: what should have been done by the status date. */
    pv:   number
    /** Earned value: what has been done, valued at its budget. */
    ev:   number
    /** Actual cost. */
    ac:   number
    /** EV − AC. Negative means overspent. */
    cv:   number
    /** EV − PV. Negative means behind. */
    sv:   number
    cpi:  number | null
    spi:  number | null
    /** Estimate at completion, by the chosen method. */
    eac:  number | null
    etc:  number | null
    /** BAC − EAC. Negative means the project lands over budget. */
    vac:  number | null
    /** The performance the remaining work must achieve to still land on budget.
     *  Above 1 means it has to go better than it ever has. */
    tcpi: number | null
  }
  /** From the cost and schedule management plans: the variance beyond which a
   *  deviation is reported. Null where the project set none, in which case the
   *  view keeps its own judgement. Read only from ACTIVE plans. */
  thresholds: {
    cost_variance_pct:     number | null
    schedule_variance_pct: number | null
    cost_breached:         boolean | null
    schedule_breached:     boolean | null
  }
  /** How much of the plan can be measured at all — an index over a third of the
   *  work is not a project-level statement. */
  coverage: {
    costed_tasks: number
    leaf_tasks:   number
    logged_hours: number
    labour_cost:  number
    direct_cost:  number
    has_rate:     boolean
  }
  tasks: CostTaskLine[]
  curve: CostCurvePoint[]
}

// ── Stakeholders and RACI ────────────────────────────────────────────────────

export type StakeholderCategory = 'internal' | 'external' | 'sponsor' | 'customer' | 'supplier' | 'regulator' | 'team'
/** PMBOK's five levels, in order — the order is what makes a gap a direction. */
export type EngagementLevel = 'unaware' | 'resistant' | 'neutral' | 'supportive' | 'leading'
/** Responsible does the work · Accountable answers for it · Consulted is asked
 *  beforehand · Informed is told afterwards. */
export type RaciRole = 'R' | 'A' | 'C' | 'I'
/** The four quadrants of the power/interest grid, each prescribing a different
 *  amount of attention. */
export type StakeholderQuadrant = 'manage_closely' | 'keep_satisfied' | 'keep_informed' | 'monitor'

export interface Stakeholder {
  id:                  string
  project_id:          string
  name:                string
  organisation:        string
  role_title:          string
  contact_email:       string
  category:            StakeholderCategory
  /** 1 to 5. */
  power:               number
  interest:            number
  engagement_current:  EngagementLevel
  engagement_desired:  EngagementLevel
  expectations:        string
  influence_notes:     string
  communication_notes: string
  user_id:             string | null
  position:            number
  created_at:          string
  updated_at:          string
}

export type StakeholderEdit = Partial<Omit<Stakeholder, 'id' | 'project_id' | 'created_at' | 'updated_at'>>

/** A stakeholder who is not where the project needs them to be. */
export interface EngagementGap {
  id:        string
  name:      string
  power:     number
  interest:  number
  current:   EngagementLevel
  desired:   EngagementLevel
  /** Positive: they must be brought along. Negative: less involvement is wanted. */
  distance:  number
  quadrant:  StakeholderQuadrant
}

export interface StakeholderRegister {
  stakeholders: Stakeholder[]
  /** 5×5 counts, grid[power-1][interest-1]. */
  grid: number[][]
  summary: {
    total: number
    by_quadrant: Record<StakeholderQuadrant, number>
    aligned: number
  }
  gaps: EngagementGap[]
}

export interface RaciCell { stakeholder_id: string; role: RaciRole }

export interface RaciRow {
  task_id:            string
  wbs:                string
  name:               string
  task_type:          string
  /** True when the task has children: a rollup, which its children already cover. */
  is_rollup:          boolean
  cells:              RaciCell[]
  accountable:        string | null
  responsible_count:  number
}

export interface RaciMatrix {
  stakeholders: Array<{ id: string; name: string; role_title: string }>
  tasks: RaciRow[]
  /** What the matrix is missing — the reason for reading it. */
  gaps: {
    no_accountable: Array<{ task_id: string; wbs: string; name: string }>
    no_responsible: Array<{ task_id: string; wbs: string; name: string }>
  }
}

// ── Quality ──────────────────────────────────────────────────────────────────

export type QualityDirection = 'higher' | 'lower' | 'target'
export type QualityFrequency = 'continuous' | 'daily' | 'weekly' | 'sprint' | 'monthly' | 'milestone' | 'once'
export type QualityResult = 'pending' | 'pass' | 'fail' | 'waived'
/** Prevention and appraisal are what a project spends to avoid failure; the two
 *  failure lines are what it pays when that did not work. */
export type CoqCategory = 'prevention' | 'appraisal' | 'internal_failure' | 'external_failure'

export interface QualityMetric {
  id:             string
  project_id:     string
  code:           string
  name:           string
  description:    string
  /** How the number is obtained. A metric nobody can reproduce is a slogan. */
  method:         string
  unit:           string
  target:         number | null
  /** The band that still counts as conforming. Either bound may stand alone. */
  tolerance_min:  number | null
  tolerance_max:  number | null
  direction:      QualityDirection
  frequency:      QualityFrequency
  owner_id:       string | null
  deliverable_id: string | null
  task_id:        string | null
  is_active:      boolean
  position:       number
  created_at:     string
  updated_at:     string
}

export type QualityMetricEdit = Partial<Omit<QualityMetric, 'id' | 'project_id' | 'created_at' | 'updated_at'>>

export interface QualityReading {
  id:          string
  measured_on: string
  value:       number
  notes:       string
}

export interface QualityMetricLine {
  metric:   QualityMetric
  latest:   { measured_on: string; value: number } | null
  previous: number | null
  /** Null when the metric states nothing to compare against — a number being
   *  collected is not a standard being held to. */
  conforms: boolean | null
  /** Conforming, but close to a bound and heading that way. */
  at_risk:  boolean
  /** Share of the tolerance band left to the nearest edge; null without a band. */
  margin:   number | null
  series:   QualityReading[]
}

export interface QualityCheck {
  id:               string
  project_id:       string
  deliverable_id:   string | null
  deliverable_name: string | null
  task_id:          string | null
  task_name:        string | null
  label:            string
  result:           QualityResult
  /** What the check found. Required before declaring a failure. */
  evidence:         string
  checked_on:       string | null
  checked_by:       string | null
  issue_id:         string | null
  position:         number
  created_at:       string
  updated_at:       string
}

export type QualityCheckEdit = Partial<Pick<QualityCheck,
  'label' | 'result' | 'evidence' | 'deliverable_id' | 'task_id' | 'checked_on' | 'position'>>

export interface QualityOverview {
  metrics: QualityMetricLine[]
  summary: {
    total: number; active: number
    conforming: number; breaching: number
    /** Never measured — needs a reading. */
    unmeasured: number
    /** Measured, but with nothing to compare against — needs a bound. */
    unrated: number
    /** Conforming, but drifting towards a bound. */
    drifting: number
  }
  checks: Record<QualityResult, number>
  cost_of_quality: {
    by_category: Partial<Record<CoqCategory, number>>
    /** Prevention + appraisal. */
    conformance: number
    /** Internal + external failure. */
    failure: number
    /** Expenses nobody classified — reported rather than folded in. */
    unclassified: number
  }
}

// ── Communications and decisions ─────────────────────────────────────────────

export type CommChannel = 'email' | 'meeting' | 'report' | 'dashboard' | 'chat' | 'workshop' | 'other'
export type CommFrequency = 'daily' | 'weekly' | 'biweekly' | 'monthly' | 'quarterly' | 'milestone' | 'on_demand' | 'once'
export type DecisionStatus = 'proposed' | 'decided' | 'superseded' | 'rejected'

export interface Communication {
  id:         string
  project_id: string
  name:       string
  /** Why it exists. A report nobody can name a purpose for is a habit. */
  purpose:    string
  channel:    CommChannel
  format:     string
  frequency:  CommFrequency
  owner_id:   string | null
  next_due:   string | null
  is_active:  boolean
  position:   number
  created_at: string
  updated_at: string
}

export type CommunicationEdit = Partial<Pick<Communication,
  'name' | 'purpose' | 'channel' | 'format' | 'frequency' | 'owner_id' | 'next_due' | 'is_active' | 'position'>>
  /** The whole audience, replaced as a set. Absent leaves it untouched. */
  & { audience?: string[] }

export interface CommunicationLine {
  communication: Communication
  audience: Array<{ id: string; name: string }>
  /** Active, and its next date has passed. */
  overdue: boolean
}

/** A stakeholder the plan reaches nobody about. */
export interface UncoveredStakeholder {
  id: string; name: string; power: number; interest: number
  /** power × interest — an uncovered regulator is not an uncovered bystander. */
  weight: number
  engagement_desired: EngagementLevel
}

export interface CommunicationPlan {
  communications: CommunicationLine[]
  summary: {
    total: number; active: number; overdue: number
    stakeholders: number; covered: number; uncovered: number
  }
  uncovered: UncoveredStakeholder[]
}

export interface CommunicationLogEntry {
  id:                 string
  communication_id:   string | null
  communication_name: string | null
  sent_on:            string
  summary:            string
}

export interface Decision {
  id:               string
  project_id:       string
  code:             string
  title:            string
  /** The question that had to be settled. */
  context:          string
  decision:         string
  /** Why. Without it a decision cannot be revisited honestly. */
  rationale:        string
  /** What was ruled out — the half everyone forgets. */
  alternatives:     string
  consequences:     string
  status:           DecisionStatus
  decided_on:       string | null
  decided_by:       string | null
  stakeholder_id:   string | null
  stakeholder_name: string | null
  task_id:          string | null
  task_name:        string | null
  risk_id:          string | null
  risk_code:        string | null
  /** The decision this one replaces; that one is marked superseded. */
  supersedes_id:    string | null
  supersedes_title: string | null
  position:         number
  created_at:       string
  updated_at:       string
}

export type DecisionEdit = Partial<Pick<Decision,
  'code' | 'title' | 'context' | 'decision' | 'rationale' | 'alternatives' | 'consequences'
  | 'status' | 'decided_on' | 'stakeholder_id' | 'task_id' | 'risk_id' | 'supersedes_id' | 'position'>>

export interface DecisionLog {
  decisions: Decision[]
  summary: {
    total: number; pending: number; decided: number; superseded: number; unexplained: number
  }
  /** Decided, with no reasoning recorded — the log's own defect. */
  unexplained: Array<{ id: string; code: string; title: string }>
}

// ── Change control ───────────────────────────────────────────────────────────

export type ChangeCategory = 'scope' | 'schedule' | 'cost' | 'quality' | 'resource' | 'requirement' | 'other'
/** Corrective and preventive actions and defect repairs are changes too. */
export type ChangeKind = 'change' | 'corrective' | 'preventive' | 'defect_repair'
export type ChangeUrgency = 'low' | 'normal' | 'high' | 'critical'
export type ChangeStatus = 'submitted' | 'assessing' | 'approved' | 'partially_approved'
  | 'rejected' | 'deferred' | 'implemented' | 'withdrawn'

export interface ChangeRequest {
  id:               string
  project_id:       string
  code:             string
  title:            string
  description:      string
  /** Why it is being asked for. A change nobody can justify is a preference. */
  justification:    string
  category:         ChangeCategory
  kind:             ChangeKind
  urgency:          ChangeUrgency
  requested_by:     string | null
  stakeholder_id:   string | null
  stakeholder_name: string | null
  requested_on:     string
  /** Null until somebody assessed it — NOT zero. Approving an unassessed change
   *  is refused by the server. */
  impact_days:      number | null
  impact_cost:      number | null
  impact_scope:     string
  impact_risk:      string
  impact_quality:   string
  assessed_by:      string | null
  assessed_on:      string | null
  status:           ChangeStatus
  decision_note:    string
  decided_by:       string | null
  decided_on:       string | null
  /** The baseline captured after approval — the plan the change moved to. */
  baseline_id:      string | null
  baseline_name:    string | null
  task_id:          string | null
  task_name:        string | null
  risk_id:          string | null
  risk_code:        string | null
  decision_id:      string | null
  decision_title:   string | null
  position:         number
  created_at:       string
  updated_at:       string
}

export type ChangeRequestEdit = Partial<Pick<ChangeRequest,
  'code' | 'title' | 'description' | 'justification' | 'category' | 'kind' | 'urgency'
  | 'stakeholder_id' | 'task_id' | 'risk_id' | 'decision_id' | 'requested_on' | 'position'>>

export interface ChangeAssessment {
  impact_days?:    number | null
  impact_cost?:    number | null
  impact_scope?:   string
  impact_risk?:    string
  impact_quality?: string
}

export interface ChangeLog {
  changes: ChangeRequest[]
  summary: {
    total: number
    awaiting_decision: number
    awaiting_assessment: number
    approved: number; rejected: number; deferred: number
    /** What everything said yes to has already done to the plan. */
    approved_impact: { days: number; cost: number; costed: number }
    /** Approved with nothing recording what the plan became. */
    approved_without_baseline: number
    /** From the change management plan: what the project manager may decide
     *  alone. Either limit exceeded sends the request to the board. */
    authority: { amount: number | null; days: number | null }
  }
  awaiting: Array<{
    id: string; code: string; title: string
    urgency: ChangeUrgency; requested_on: string; assessed: boolean
    /** True when either delegation limit is exceeded. Null while the change is
     *  unassessed — you cannot know before somebody has costed it — and null
     *  when no delegation was written down. */
    board_required: boolean | null
  }>
}

// ── Closure and lessons learned ──────────────────────────────────────────────

export type ClosureStatus = 'open' | 'closing' | 'closed'
export type LessonCategory = 'process' | 'technical' | 'people' | 'supplier' | 'estimation' | 'communication' | 'risk' | 'other'
/** A register of failures alone teaches people to hide them. */
export type LessonOutcome = 'positive' | 'negative' | 'mixed'
export type LessonStatus = 'draft' | 'validated' | 'shared'

export interface Closure {
  project_id:      string
  status:          ClosureStatus
  /** Against the charter: was the purpose actually served? */
  objectives_met:  string
  acceptance_note: string
  handover_note:   string
  /** Contracts, licences, accesses — what outlives the project. */
  loose_ends:      string
  final_note:      string
  /** Why it was closed with checks still failing. */
  override_reason: string
  closed_on:       string | null
  closed_by:       string | null
  /** Named rather than an identifier. */
  closed_by_name:  string | null
  updated_at:      string
}

export type ClosureEdit = Partial<Pick<Closure,
  'objectives_met' | 'acceptance_note' | 'handover_note' | 'loose_ends' | 'final_note'>>
  & { status?: 'open' | 'closing' }

/** One thing to settle before the project can be called over. Blocking checks are
 *  things left undone; advisory ones are things left unwritten. */
export interface ClosureCheck {
  key:      string
  blocking: boolean
  count:    number
  ok:       boolean
}

export interface ClosureOverview {
  closure: Closure
  checks:  ClosureCheck[]
  summary: { blocking: number; advisory: number; ready: boolean; closed: boolean }
}

export interface Lesson {
  id:             string
  project_id:     string
  code:           string
  title:          string
  category:       LessonCategory
  outcome:        LessonOutcome
  situation:      string
  what_happened:  string
  /** The only part that travels to the next project. Without it, an anecdote. */
  recommendation: string
  task_id:        string | null
  task_name:      string | null
  risk_id:        string | null
  risk_code:      string | null
  issue_id:       string | null
  issue_code:     string | null
  change_id:      string | null
  change_code:    string | null
  status:         LessonStatus
  recorded_by:    string | null
  recorded_on:    string
  position:       number
  created_at:     string
  updated_at:     string
}

export type LessonEdit = Partial<Pick<Lesson,
  'code' | 'title' | 'category' | 'outcome' | 'situation' | 'what_happened'
  | 'recommendation' | 'status' | 'task_id' | 'risk_id' | 'issue_id' | 'change_id' | 'position'>>

export interface LessonLog {
  lessons: Lesson[]
  summary: {
    total: number; positive: number; negative: number; mixed: number
    validated: number; without_recommendation: number
  }
  without_recommendation: Array<{ id: string; code: string; title: string }>
}

// ── Procurement ──────────────────────────────────────────────────────────────

/** The field that decides who pays when the estimate turns out to be wrong. */
export type ContractType = 'fixed_price' | 'fixed_incentive' | 'cost_plus_fee'
  | 'cost_plus_incentive' | 'time_material' | 'other'
export type ProcurementStatus = 'planned' | 'tendering' | 'awarded' | 'active' | 'closed' | 'cancelled'
export type PaymentStatus = 'planned' | 'invoiced' | 'paid' | 'disputed' | 'cancelled'
/** Who absorbs an overrun under this contract. */
export type RiskSide = 'supplier' | 'buyer' | 'shared' | 'unknown'

export interface Procurement {
  id:                string
  project_id:        string
  code:              string
  title:             string
  statement_of_work: string
  /** Why it is bought rather than built. */
  make_or_buy_note:  string
  contract_type:     ContractType
  supplier_name:     string
  supplier_contact:  string
  stakeholder_id:    string | null
  stakeholder_name:  string | null
  value:             number | null
  /** The ceiling on a time-and-material contract; null means unbounded exposure. */
  not_to_exceed:     number | null
  status:            ProcurementStatus
  awarded_on:        string | null
  starts_on:         string | null
  ends_on:           string | null
  deliverable_id:    string | null
  deliverable_name:  string | null
  task_id:           string | null
  task_name:         string | null
  risk_id:           string | null
  risk_code:         string | null
  terms:             string
  performance_note:  string
  closed_on:         string | null
  closure_note:      string
  position:          number
  created_at:        string
  updated_at:        string
}

export type ProcurementEdit = Partial<Omit<Procurement,
  'id' | 'project_id' | 'stakeholder_name' | 'deliverable_name' | 'task_name' | 'risk_code'
  | 'created_at' | 'updated_at'>>

export interface ProcurementPayment {
  id:             string
  procurement_id: string
  label:          string
  due_on:         string | null
  amount:         number
  status:         PaymentStatus
  invoice_ref:    string
  paid_on:        string | null
  cost_entry_id:  string | null
  position:       number
}

export type PaymentEdit = Partial<Pick<ProcurementPayment,
  'label' | 'due_on' | 'amount' | 'status' | 'invoice_ref' | 'paid_on' | 'position'>>

export interface ProcurementLine {
  contract:         Procurement
  payments:         ProcurementPayment[]
  paid:             number
  invoiced:         number
  /** Committed minus paid; null when the contract carries no value. */
  remaining:        number | null
  overdue_payments: number
  risk_side:        RiskSide
}

export interface ProcurementRegister {
  contracts: ProcurementLine[]
  summary: {
    total: number
    open: number
    /** Split by who absorbs an overrun — summing them into one figure would
     *  describe an exposure the project does not carry. */
    committed: {
      supplier_risk: number; buyer_risk: number; shared_risk: number
      /** Contracts typed "other": they declare no risk allocation at all. */
      unknown_risk: number
      total: number
    }
    paid: number
    overdue_payments: number
    /** Live contracts with no value recorded. */
    unpriced: number
  }
  /** Time and material with no ceiling — not an amount, the absence of one. */
  uncapped: Array<{ id: string; code: string; title: string; supplier: string }>
}

// ── Portfolio ────────────────────────────────────────────────────────────────

/** What asks for attention, named rather than scored: one health figure hides
 *  which of them is wrong. */
export type PortfolioFlag = 'late_work' | 'overdue_issues' | 'high_risks'
  | 'changes_waiting' | 'over_budget' | 'past_end_date'

export interface PortfolioProject {
  id:             string
  title:          string
  status:         string
  start_date:     string | null
  end_date:       string | null
  parent_id:      string | null
  closure_status: ClosureStatus
  /** Share of leaf tasks completed. */
  progress:       number
  tasks:          { total: number; done: number; late: number; critical: number }
  risks:          { high: number; occurred: number }
  issues:         { open: number; overdue: number }
  changes:        { awaiting: number }
  deliverables:   { total: number; accepted: number }
  money: {
    budget: number
    spent_direct: number
    /** Committed on contracts where the project, not the supplier, absorbs an overrun. */
    exposure: number
  }
  flags: PortfolioFlag[]
}

export interface Portfolio {
  projects: PortfolioProject[]
  summary: {
    total: number
    needs_attention: number
    closed: number
    budget: number
    spent_direct: number
    exposure: number
  }
}

// ── Subsidiary management plans ──────────────────────────────────────────────

/** The twelve areas a project can plan, in the reading order of the integrated
 *  plan document — not alphabetical. */
export type PlanArea = 'scope' | 'requirements' | 'schedule' | 'cost' | 'quality'
  | 'resource' | 'communications' | 'risk' | 'procurement' | 'stakeholder'
  | 'change' | 'configuration'
export type PlanReviewFrequency = 'weekly' | 'biweekly' | 'monthly' | 'quarterly' | 'milestone' | 'on_demand'

export interface ManagementPlan {
  id:         string
  project_id: string
  area:       PlanArea
  is_active:  boolean
  /** How the area is run. */
  approach:   string
  roles:      string
  procedures: string
  tools:      string
  /** Cost and schedule only: the variance beyond which a deviation is reported. */
  variance_threshold_pct:  number | null
  /** Risk only: the score above which a risk must be escalated, 1–25. */
  risk_appetite_score:     number | null
  /** Change only: what the project manager may decide alone. */
  change_authority_amount: number | null
  change_authority_days:   number | null
  review_frequency:        PlanReviewFrequency
  updated_at: string
}

export type ManagementPlanEdit = Partial<Pick<ManagementPlan,
  'is_active' | 'approach' | 'roles' | 'procedures' | 'tools' | 'review_frequency'
  | 'variance_threshold_pct' | 'risk_appetite_score'
  | 'change_authority_amount' | 'change_authority_days'>>

/** Every area is returned whether or not it has been planned: an area nobody has
 *  written about is a plan not made, not a plan missing from the interface. */
export interface PlanSlot {
  area:    PlanArea
  planned: boolean
  plan:    ManagementPlan | null
}

export interface ManagementPlans {
  plans: PlanSlot[]
  summary: { areas: number; active: number; without_approach: number }
  /** Active but never written: they claim the area is governed while nothing
   *  says how. */
  without_approach: PlanArea[]
}

// ── Document production ──────────────────────────────────────────────────────

export type ProducibleDocKind = 'charter' | 'status_report' | 'closure_report'
  | 'risk_register' | 'lessons_register' | 'traceability_matrix' | 'wbs_dictionary'
  | 'management_plan'

/** A document the project can produce, and whether the data behind it exists —
 *  a document offered but empty wastes a click. */
export interface ProducibleDoc {
  kind:     ProducibleDocKind
  has_data: boolean
  /** Why it has no data yet, when it does not. */
  hint:     string
}

export interface ProjectSettings {
  methodology:  ProjectMethodology
  default_view: string
  artifacts:    Record<ProjectArtifactKey, { enabled: boolean; config: Record<string, unknown> }>
}

// ── Public holidays (read from the core's referential) ────────────────────────
// The core resolves which calendars apply to the reader (organisational unit,
// falling back to the timezone), so a project does not have to ask for a country.

export interface Holiday {
  date: string
  name: string
  key: string
  category: string
  calendar_code: string
  calendar_name: string
}

export const holidaysApi = {
  applicable: (locale?: string) =>
    api.get<{ calendars: { code: string; name: string }[]; codes: string[]; source: string }>(
      '/holidays/applicable', { params: { locale } }).then(r => r.data),
  list: (from: string, to: string, locale?: string) =>
    api.get<{ holidays: Holiday[] }>('/holidays', { params: { from, to, locale } })
      .then(r => r.data.holidays),
}

// ── Working calendars (projects) ──────────────────────────────────────────────

export interface ProjectCalendar {
  id: string
  project_id: string
  name: string
  mon: boolean; tue: boolean; wed: boolean; thu: boolean; fri: boolean; sat: boolean; sun: boolean
  created_at: string
}
export interface ProjectCalendarException {
  id: string
  calendar_id: string
  day: string
  is_working: boolean
  note: string
}
export interface ProjectCalendars {
  calendars: ProjectCalendar[]
  exceptions: ProjectCalendarException[]
  project_calendar_id: string | null
}

export type TimeActivity = 'development' | 'design' | 'coordination' | 'testing' | 'documentation' | 'other'

export interface TimeEntry {
  id:         string
  project_id: string
  task_id:    string
  user_id:    string
  spent_on:   string
  hours:      number
  activity:   TimeActivity
  comment:    string
  created_at: string
}

export interface BaselineTaskSnapshot {
  task_id:       string
  name:          string
  early_start:   number | null
  early_finish:  number | null
  duration_days: number
}
export interface Baseline {
  id:            string
  name:          string
  project_start: string | null
  tasks:         BaselineTaskSnapshot[]
  captured_at:   string
  is_primary:    boolean
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
