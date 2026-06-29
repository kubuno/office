import { api } from '@kubuno/sdk'

const BASE = '/office/script'

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Script {
  id: string
  owner_id: string
  name: string
  description: string | null
  source_code: string
  compiled_code: string | null
  compile_error: string | null
  timeout_secs: number
  memory_limit_mb: number
  run_count: number
  last_run_at: string | null
  last_run_status: string | null
  is_starred: boolean
  is_trashed: boolean
  created_at: string
  updated_at: string
}

export interface ScriptTrigger {
  id: string
  script_id: string
  owner_id: string
  name: string
  trigger_type: 'cron' | 'event' | 'webhook'
  cron_expression: string | null
  event_name: string | null
  event_module: string | null
  event_filter: Record<string, unknown>
  webhook_token: string | null
  input_vars: Record<string, unknown>
  is_active: boolean
  last_fired_at: string | null
  fire_count: number
  created_at: string
}

export interface ConsoleEntry {
  level: 'log' | 'warn' | 'error'
  args: unknown[]
  time_ms: number
}

export interface ScriptRun {
  id: string
  script_id: string
  owner_id: string
  trigger_id: string | null
  run_source: string
  status: 'running' | 'success' | 'error' | 'timeout'
  duration_ms: number | null
  memory_used_kb: number | null
  console_output: ConsoleEntry[]
  return_value: unknown | null
  error_message: string | null
  error_stack: string | null
  trigger_data: unknown | null
  started_at: string
  finished_at: string | null
}

export interface ScriptMacro {
  id: string
  script_id: string
  owner_id: string
  document_type: string | null
  document_id: string | null
  button_label: string
  button_icon: string
  position: number
  created_at: string
}

// ── Scripts API ───────────────────────────────────────────────────────────────

export const scriptsApi = {
  list: (params?: { search?: string; trashed?: boolean; limit?: number; offset?: number }) =>
    api.get<{ scripts: Script[]; total: number }>(`${BASE}/scripts`, { params }).then(r => r.data),

  create: (data?: { name?: string; description?: string; source_code?: string; timeout_secs?: number; memory_limit_mb?: number }) =>
    api.post<{ script: Script }>(`${BASE}/scripts`, data).then(r => r.data),

  get: (id: string) =>
    api.get<{ script: Script }>(`${BASE}/scripts/${id}`).then(r => r.data),

  openByFile: (fileId: string) =>
    api.post<{ script: Script }>(`${BASE}/scripts/open-by-file`, { file_id: fileId }).then(r => r.data),

  update: (id: string, data: Partial<Script>) =>
    api.patch<{ script: Script }>(`${BASE}/scripts/${id}`, data).then(r => r.data),

  delete: (id: string) =>
    api.delete(`${BASE}/scripts/${id}`),

  trash: (id: string) =>
    api.post(`${BASE}/scripts/${id}/trash`),

  restore: (id: string) =>
    api.post(`${BASE}/scripts/${id}/restore`),

  duplicate: (id: string) =>
    api.post<{ script: Script }>(`${BASE}/scripts/${id}/duplicate`).then(r => r.data),

  compile: (id: string) =>
    api.post<{ compiled: boolean; compiled_code: string }>(`${BASE}/scripts/${id}/compile`).then(r => r.data),

  run: (id: string) =>
    api.post<{ run_id: string }>(`${BASE}/scripts/${id}/run`).then(r => r.data),
}

// ── Triggers API ──────────────────────────────────────────────────────────────

export const triggersApi = {
  list: (scriptId: string) =>
    api.get<{ triggers: ScriptTrigger[] }>(`${BASE}/scripts/${scriptId}/triggers`).then(r => r.data),

  create: (scriptId: string, data: {
    name?: string
    trigger_type: 'cron' | 'event' | 'webhook'
    cron_expression?: string
    event_name?: string
    event_module?: string
    event_filter?: Record<string, unknown>
    input_vars?: Record<string, unknown>
  }) =>
    api.post<{ trigger: ScriptTrigger }>(`${BASE}/scripts/${scriptId}/triggers`, data).then(r => r.data),

  update: (id: string, data: Partial<ScriptTrigger>) =>
    api.patch<{ trigger: ScriptTrigger }>(`${BASE}/triggers/${id}`, data).then(r => r.data),

  delete: (id: string) =>
    api.delete(`${BASE}/triggers/${id}`),

  toggle: (id: string) =>
    api.post<{ is_active: boolean }>(`${BASE}/triggers/${id}/toggle`).then(r => r.data),
}

// ── Runs API ──────────────────────────────────────────────────────────────────

export const runsApi = {
  listForScript: (scriptId: string, params?: { limit?: number; offset?: number }) =>
    api.get<{ runs: ScriptRun[]; total: number }>(`${BASE}/scripts/${scriptId}/runs`, { params }).then(r => r.data),

  get: (id: string) =>
    api.get<{ run: ScriptRun }>(`${BASE}/runs/${id}`).then(r => r.data),

  /** Returns the SSE stream URL — use with EventSource */
  streamUrl: (runId: string) => `/api/v1${BASE}/runs/${runId}/stream`,
}

// ── Macros API ────────────────────────────────────────────────────────────────

export const macrosApi = {
  list: () =>
    api.get<{ macros: ScriptMacro[] }>(`${BASE}/macros`).then(r => r.data),

  listForDocument: (docType: string, docId: string) =>
    api.get<{ macros: ScriptMacro[] }>(`${BASE}/macros/${docType}/${docId}`).then(r => r.data),

  create: (data: {
    script_id: string
    document_type?: string
    document_id?: string
    button_label?: string
    button_icon?: string
    position?: number
  }) =>
    api.post<{ macro: ScriptMacro }>(`${BASE}/macros`, data).then(r => r.data),

  delete: (id: string) =>
    api.delete(`${BASE}/macros/${id}`),

  run: (id: string) =>
    api.post<{ run_id: string }>(`${BASE}/macros/${id}/run`).then(r => r.data),
}

// ── Macros « container-bound » (stockées DANS la donnée du document) ───────────
// Remplace l'attache macrosApi (table externe) : les macros voyagent avec le fichier.

export type MacroKind = 'module' | 'form'
export type ControlType = 'label' | 'textbox' | 'button' | 'checkbox'

// Contrôle d'un formulaire (façon UserForm VBA).
export interface FormControl {
  id:    string
  type:  ControlType
  name:  string                       // identifiant utilisé dans le code (ex. "Button1")
  x:     number; y: number; w: number; h: number
  text?: string                       // légende (label/bouton/case) ou valeur initiale (textbox)
  value?: boolean                     // état initial d'une case à cocher
}

// Une entrée du projet de macros : un MODULE de code (kind 'module') OU un
// FORMULAIRE (kind 'form' : contrôles + code événementiel). `kind` absent = module.
export interface DocMacro {
  id:        string
  name:      string
  kind?:     MacroKind
  source:    string                   // code (module) ou code événementiel (form)
  controls?: FormControl[]            // formulaires uniquement
  formW?:    number
  formH?:    number
}

export const docMacrosApi = {
  list: (docType: string, docId: string) =>
    api.get<{ macros: DocMacro[] }>(`/office/doc-macros/${docType}/${docId}`).then(r => r.data.macros),

  save: (docType: string, docId: string, macros: DocMacro[]) =>
    api.put<{ macros: DocMacro[] }>(`/office/doc-macros/${docType}/${docId}`, { macros }).then(r => r.data.macros),
}

// ── API Types ─────────────────────────────────────────────────────────────────

export const getApiTypes = () =>
  fetch('/api/v1' + BASE + '/api-types').then(r => r.text())
