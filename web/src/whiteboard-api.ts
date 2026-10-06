import { api } from '@kubuno/sdk'
import type { CollabPermission, Recipient, CollaboratorEntry } from './api'

const BASE = '/office/whiteboard'

export interface Board {
  id: string
  owner_id: string
  title: string
  description: string | null
  thumbnail_path: string | null
  share_token: string | null
  is_public: boolean
  background: 'white' | 'grid' | 'dots' | 'lines'
  collaborators: unknown[]
  element_count: number
  frame_count: number
  is_trashed: boolean
  trashed_at: string | null
  last_edited_at: string | null
  last_edited_by: string | null
  is_starred: boolean
  created_at: string
  updated_at: string
}

export const boardsApi = {
  list: (params?: { search?: string; trashed?: boolean; starred?: boolean; limit?: number; offset?: number }) =>
    api.get<{ boards: Board[]; total: number }>(`${BASE}/boards`, { params }).then(r => r.data),
  create: (data?: { title?: string; background?: string; description?: string }) =>
    api.post<{ board: Board }>(`${BASE}/boards`, data).then(r => r.data),
  get: (id: string) =>
    api.get<{ board: Board }>(`${BASE}/boards/${id}`).then(r => r.data),
  openByFile: (fileId: string) =>
    api.post<{ board: Board }>(`${BASE}/boards/open-by-file`, { file_id: fileId }).then(r => r.data),
  update: (id: string, data: Partial<Board>) =>
    api.patch<{ board: Board }>(`${BASE}/boards/${id}`, data).then(r => r.data),
  delete: (id: string) => api.delete(`${BASE}/boards/${id}`),
  trash: (id: string) => api.post(`${BASE}/boards/${id}/trash`),
  restore: (id: string) => api.post(`${BASE}/boards/${id}/restore`),
  duplicate: (id: string) => api.post<{ board: Board }>(`${BASE}/boards/${id}/duplicate`).then(r => r.data),
  uploadThumbnail: (id: string, blob: Blob) => {
    return api.post(`${BASE}/boards/${id}/thumbnail`, blob, {
      headers: { 'Content-Type': 'image/png' },
    })
  },

  // ── Partage utilisateur-à-utilisateur (collaborateurs) ──────────────────────
  listCollaborators: (id: string) =>
    api.get<{ owner: Recipient | null; collaborators: CollaboratorEntry[] }>(`${BASE}/boards/${id}/collaborators`).then(r => r.data),
  addCollaborator: (id: string, userId: string, permission: CollabPermission = 'edit') =>
    api.post(`${BASE}/boards/${id}/collaborators`, { user_id: userId, permission }),
  updateCollaborator: (id: string, userId: string, permission: CollabPermission) =>
    api.patch(`${BASE}/boards/${id}/collaborators/${userId}`, { permission }),
  removeCollaborator: (id: string, userId: string) =>
    api.delete(`${BASE}/boards/${id}/collaborators/${userId}`),
}
