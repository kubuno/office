import { api } from '@kubuno/sdk'

const BASE = '/office/data'

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Datasource {
  id: string
  owner_id: string
  name: string
  description: string | null
  source_type: string
  config: Record<string, unknown>
  connection_status: 'ok' | 'error' | 'untested'
  last_tested_at: string | null
  connection_error: string | null
  created_at: string
  updated_at: string
}

export interface Dataset {
  id: string
  owner_id: string
  datasource_id: string | null
  name: string
  description: string | null
  raw_sql: string | null
  query_steps: QueryStep[]
  schema_cache: ColumnSchema[]
  row_count: number | null
  last_refresh_at: string | null
  refresh_error: string | null
  refresh_schedule: string | null
  status: 'empty' | 'refreshing' | 'ready' | 'error'
  created_at: string
  updated_at: string
}

export interface ColumnSchema {
  name: string
  type: string
}

export type QueryStep =
  | { type: 'source'; sql: string }
  | { type: 'filter'; column: string; operator: string; value: unknown }
  | { type: 'sort'; column: string; direction: 'ASC' | 'DESC' }
  | { type: 'group'; by: string[]; aggregations: Aggregation[] }
  | { type: 'add_column'; name: string; expression: string }
  | { type: 'rename'; from: string; to: string }
  | { type: 'remove_columns'; columns: string[] }
  | { type: 'limit'; count: number }

export interface Aggregation {
  column: string
  function: 'SUM' | 'COUNT' | 'AVG' | 'MIN' | 'MAX'
  alias?: string
}

export interface Measure {
  id: string
  owner_id: string
  dataset_id: string
  name: string
  description: string | null
  expression: string
  result_type: string
  format_string: string | null
  display_folder: string | null
  is_valid: boolean
  compile_error: string | null
  created_at: string
  updated_at: string
}

export interface Relation {
  id: string
  owner_id: string
  from_dataset_id: string
  from_column: string
  to_dataset_id: string
  to_column: string
  cardinality: string
  cross_filter: string
  is_active: boolean
  created_at: string
}

export interface Report {
  id: string
  owner_id: string
  title: string
  description: string | null
  theme: ReportTheme
  page_count: number
  dataset_ids: string[]
  share_token: string | null
  is_public: boolean
  is_trashed: boolean
  is_starred: boolean
  thumbnail_url: string | null
  created_at: string
  updated_at: string
}

export interface ReportTheme {
  primaryColor: string
  fontFamily: string
  background: string
  chartPalette: string[]
}

export interface ReportPage {
  id: string
  report_id: string
  title: string
  position: number
  width: number
  height: number
  background: string | null
  created_at: string
}

// Widget type is a free-form string at the backend (JSONB-driven visuals). The
// known names below give editor hints while `(string & {})` keeps any new visual
// type valid without a schema/type change.
export type WidgetType =
  | 'kpi_card' | 'line_chart' | 'bar_chart' | 'pie_chart' | 'data_table'
  | 'scorecard' | 'text' | 'filter_date' | 'filter_dropdown' | 'scatter_chart'
  | 'area_chart' | 'donut_chart' | 'gauge'
  | (string & {})

export interface Widget {
  id: string
  page_id: string
  report_id: string
  widget_type: WidgetType
  x: number
  y: number
  width: number
  height: number
  config: WidgetConfig
  z_index: number
  created_at: string
  updated_at: string
}

export interface WidgetConfig {
  title?: string
  dataset_id?: string
  dimensions?: string[]
  metrics?: { column: string; function: string; alias?: string }[]
  filters?: { column: string; operator: string; value: unknown }[]
  sort?: { column: string; direction: 'ASC' | 'DESC' }[]
  limit?: number
  format?: string
  color?: string
  compare_metric?: string
  compare_label?: string
  show_sparkline?: boolean
  [key: string]: unknown
}

export interface ExecuteResult {
  columns: string[]
  rows: Record<string, unknown>[]
  total: number
}

export interface SemanticModel {
  datasets: Dataset[]
  relations: Relation[]
  measures: Measure[]
}

// ── Datasources ───────────────────────────────────────────────────────────────

export const datasourcesApi = {
  list: () => api.get<{ datasources: Datasource[] }>(`${BASE}/datasources`).then(r => r.data),
  create: (data: Partial<Datasource>) => api.post<{ datasource: Datasource }>(`${BASE}/datasources`, data).then(r => r.data),
  get: (id: string) => api.get<{ datasource: Datasource }>(`${BASE}/datasources/${id}`).then(r => r.data),
  update: (id: string, data: Partial<Datasource>) => api.patch<{ datasource: Datasource }>(`${BASE}/datasources/${id}`, data).then(r => r.data),
  delete: (id: string) => api.delete(`${BASE}/datasources/${id}`),
  test: (id: string) => api.post<{ ok: boolean; error: string | null }>(`${BASE}/datasources/${id}/test`).then(r => r.data),
}

// ── Datasets ──────────────────────────────────────────────────────────────────

export const datasetsApi = {
  list: () => api.get<{ datasets: Dataset[] }>(`${BASE}/datasets`).then(r => r.data),
  create: (data: Partial<Dataset>) => api.post<{ dataset: Dataset }>(`${BASE}/datasets`, data).then(r => r.data),
  get: (id: string) => api.get<{ dataset: Dataset }>(`${BASE}/datasets/${id}`).then(r => r.data),
  update: (id: string, data: Partial<Dataset>) => api.patch<{ dataset: Dataset }>(`${BASE}/datasets/${id}`, data).then(r => r.data),
  delete: (id: string) => api.delete(`${BASE}/datasets/${id}`),
  refresh: (id: string) => api.post<{ ok: boolean; row_count: number; schema: ColumnSchema[] }>(`${BASE}/datasets/${id}/refresh`).then(r => r.data),
  preview: (id: string, params?: { limit?: number }) => api.get<{ columns: ColumnSchema[] | string[]; rows: Record<string, unknown>[]; from_cache: boolean }>(`${BASE}/datasets/${id}/preview`, { params }).then(r => r.data),
  validateSql: (id: string, sql: string) => api.post<{ valid: boolean; error?: string }>(`${BASE}/datasets/${id}/validate-sql`, { sql }).then(r => r.data),
}

// ── Measures ──────────────────────────────────────────────────────────────────

export const measuresApi = {
  list: () => api.get<{ measures: Measure[] }>(`${BASE}/measures`).then(r => r.data),
  create: (data: Partial<Measure>) => api.post<{ measure: Measure }>(`${BASE}/measures`, data).then(r => r.data),
  update: (id: string, data: Partial<Measure>) => api.patch<{ measure: Measure }>(`${BASE}/measures/${id}`, data).then(r => r.data),
  delete: (id: string) => api.delete(`${BASE}/measures/${id}`),
  validate: (data: { expression: string; dataset_id: string }) => api.post<{ valid: boolean; error?: string; preview?: unknown }>(`${BASE}/measures/validate`, data).then(r => r.data),
}

// ── Model ─────────────────────────────────────────────────────────────────────

export const modelApi = {
  get: () => api.get<SemanticModel>(`${BASE}/model`).then(r => r.data),
  createRelation: (data: Partial<Relation>) => api.post<{ relation: Relation }>(`${BASE}/model/relations`, data).then(r => r.data),
  updateRelation: (id: string, data: Partial<Relation>) => api.patch<{ relation: Relation }>(`${BASE}/model/relations/${id}`, data).then(r => r.data),
  deleteRelation: (id: string) => api.delete(`${BASE}/model/relations/${id}`),
}

// ── Execute ───────────────────────────────────────────────────────────────────

export const executeApi = {
  query: (data: {
    dataset_id: string
    dimensions?: string[]
    metrics?: { column: string; function: string; alias?: string }[]
    filters?: { column: string; operator: string; value: unknown }[]
    sort?: { column: string; direction: string }[]
    limit?: number
  }) => api.post<ExecuteResult>(`${BASE}/execute`, data).then(r => r.data),
  measure: (data: { dataset_id: string; expression?: string; measure_id?: string }) =>
    api.post<{ value: unknown }>(`${BASE}/execute/measure`, data).then(r => r.data),
}

// ── Reports ───────────────────────────────────────────────────────────────────

export const reportsApi = {
  list: (params?: { search?: string; starred?: boolean; trashed?: boolean; limit?: number; offset?: number }) =>
    api.get<{ reports: Report[]; total: number }>(`${BASE}/reports`, { params }).then(r => r.data),
  create: (data?: { title?: string; description?: string }) =>
    api.post<{ report: Report }>(`${BASE}/reports`, data).then(r => r.data),
  get: (id: string) =>
    api.get<{ report: Report; pages: ReportPage[]; widgets: Widget[] }>(`${BASE}/reports/${id}`).then(r => r.data),
  update: (id: string, data: Partial<Report>) =>
    api.patch<{ report: Report }>(`${BASE}/reports/${id}`, data).then(r => r.data),
  delete: (id: string) => api.delete(`${BASE}/reports/${id}`),
  trash: (id: string) => api.post(`${BASE}/reports/${id}/trash`),
  restore: (id: string) => api.post(`${BASE}/reports/${id}/restore`),
  duplicate: (id: string) => api.post<{ report: Report }>(`${BASE}/reports/${id}/duplicate`).then(r => r.data),
  openByFile: (fileId: string) =>
    api.post<{ report: Report; pages: ReportPage[]; widgets: Widget[] }>(`${BASE}/reports/open-by-file`, { file_id: fileId }).then(r => r.data),
}

export const pagesApi = {
  create: (reportId: string, data?: Partial<ReportPage>) =>
    api.post<{ page: ReportPage }>(`${BASE}/reports/${reportId}/pages`, data).then(r => r.data),
  update: (reportId: string, pageId: string, data: Partial<ReportPage>) =>
    api.patch<{ page: ReportPage }>(`${BASE}/reports/${reportId}/pages/${pageId}`, data).then(r => r.data),
  delete: (reportId: string, pageId: string) =>
    api.delete(`${BASE}/reports/${reportId}/pages/${pageId}`),
}

export const widgetsApi = {
  create: (pageId: string, data: Partial<Widget>) =>
    api.post<{ widget: Widget }>(`${BASE}/pages/${pageId}/widgets`, data).then(r => r.data),
  update: (widgetId: string, data: Partial<Widget>) =>
    api.patch<{ widget: Widget }>(`${BASE}/widgets/${widgetId}`, data).then(r => r.data),
  delete: (widgetId: string) => api.delete(`${BASE}/widgets/${widgetId}`),
  batchUpdate: (widgets: { id: string; x?: number; y?: number; width?: number; height?: number }[]) =>
    api.patch(`${BASE}/widgets/batch`, { widgets }),
}
