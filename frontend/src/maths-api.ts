import { api } from '@kubuno/sdk'

const BASE = '/office/maths'

// ── Types ─────────────────────────────────────────────────────────────────────

export interface MathFormula {
  id:          string
  owner_id:    string
  name:        string
  description: string | null
  latex:       string
  file_id:     string | null
  is_trashed:  boolean
  created_at:  string
  updated_at:  string
}

// ── API ─────────────────────────────────────────────────────────────────────────

export const formulasApi = {
  list: (params?: { search?: string; trashed?: boolean; limit?: number; offset?: number }) =>
    api.get<{ formulas: MathFormula[]; total: number }>(`${BASE}/formulas`, { params }).then(r => r.data),

  create: (data?: { name?: string; description?: string; latex?: string }) =>
    api.post<{ formula: MathFormula }>(`${BASE}/formulas`, data ?? {}).then(r => r.data),

  get: (id: string) =>
    api.get<{ formula: MathFormula }>(`${BASE}/formulas/${id}`).then(r => r.data),

  openByFile: (fileId: string) =>
    api.post<{ formula: MathFormula }>(`${BASE}/formulas/open-by-file`, { file_id: fileId }).then(r => r.data),

  update: (id: string, data: Partial<Pick<MathFormula, 'name' | 'description' | 'latex'>>) =>
    api.patch<{ formula: MathFormula }>(`${BASE}/formulas/${id}`, data).then(r => r.data),

  delete: (id: string) =>
    api.delete(`${BASE}/formulas/${id}`),

  trash: (id: string) =>
    api.post(`${BASE}/formulas/${id}/trash`),

  restore: (id: string) =>
    api.post(`${BASE}/formulas/${id}/restore`),

  duplicate: (id: string) =>
    api.post<{ formula: MathFormula }>(`${BASE}/formulas/${id}/duplicate`).then(r => r.data),
}
