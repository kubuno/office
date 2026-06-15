import { create } from 'zustand'
import { officeApi, Document, DocumentSummary } from './api'

interface OfficeState {
  documents: DocumentSummary[]
  activeDoc: Document | null
  view: 'all' | 'recent' | 'starred' | 'trashed' | 'templates'
  searchQuery: string
  isLoading: boolean
  isSaving: boolean
  error: string | null

  fetchDocuments: () => Promise<void>
  setView: (v: OfficeState['view']) => void
  setSearchQuery: (q: string) => void
  openDoc: (id: string) => Promise<void>
  closeDoc: () => void
  createDoc: (templateId?: string, title?: string) => Promise<Document>
  saveDoc: (id: string, data: Parameters<typeof officeApi.update>[1]) => Promise<void>
  trashDoc: (id: string) => Promise<void>
  restoreDoc: (id: string) => Promise<void>
  deleteDoc: (id: string) => Promise<void>
  starDoc: (id: string, starred: boolean) => Promise<void>
  duplicateDoc: (id: string) => Promise<Document>
}

export const useOfficeStore = create<OfficeState>((set, get) => ({
  documents: [],
  activeDoc: null,
  view: 'all',
  searchQuery: '',
  isLoading: false,
  isSaving: false,
  error: null,

  fetchDocuments: async () => {
    set({ isLoading: true, error: null })
    try {
      const { view, searchQuery } = get()
      const params: Parameters<typeof officeApi.list>[0] = {}
      if (view === 'recent')    params.recent   = true
      if (view === 'starred')   params.starred  = true
      if (view === 'trashed')   params.trashed  = true
      if (searchQuery)          params.search   = searchQuery
      const result = await officeApi.list(params)
      set({ documents: result.documents })
    } catch (e: unknown) {
      set({ error: (e as Error).message })
    } finally {
      set({ isLoading: false })
    }
  },

  setView: (view) => {
    set({ view })
    get().fetchDocuments()
  },

  setSearchQuery: (searchQuery) => {
    set({ searchQuery })
    get().fetchDocuments()
  },

  openDoc: async (id) => {
    if (get().activeDoc?.id !== id) set({ activeDoc: null })
    try {
      const doc = await officeApi.get(id)
      set({ activeDoc: doc })
    } catch (e: unknown) {
      set({ error: (e as Error).message })
    }
  },

  closeDoc: () => set({ activeDoc: null }),

  createDoc: async (templateId, title = 'Nouveau document.odt') => {
    const doc = await officeApi.create({ title, template_id: templateId })
    set(s => ({ documents: [{ ...doc }, ...s.documents], activeDoc: doc }))
    return doc
  },

  saveDoc: async (id, data) => {
    set({ isSaving: true })
    try {
      const updated = await officeApi.update(id, data)
      set(s => ({
        activeDoc: s.activeDoc?.id === id ? updated : s.activeDoc,
        documents: s.documents.map(d => d.id === id
          ? { ...d, title: updated.title, word_count: updated.word_count, updated_at: updated.updated_at }
          : d
        ),
      }))
    } finally {
      set({ isSaving: false })
    }
  },

  trashDoc: async (id) => {
    await officeApi.trash(id)
    set(s => ({
      documents: s.documents.filter(d => d.id !== id),
      activeDoc: s.activeDoc?.id === id ? null : s.activeDoc,
    }))
  },

  restoreDoc: async (id) => {
    await officeApi.restore(id)
    set(s => ({ documents: s.documents.filter(d => d.id !== id) }))
  },

  deleteDoc: async (id) => {
    await officeApi.delete(id)
    set(s => ({ documents: s.documents.filter(d => d.id !== id) }))
  },

  starDoc: async (id, starred) => {
    await officeApi.update(id, { is_starred: starred })
    const { view } = get()
    if (view === 'starred' && !starred) {
      // Remove from list — doc no longer belongs in the starred view
      set(s => ({
        documents: s.documents.filter(d => d.id !== id),
        activeDoc: s.activeDoc?.id === id ? { ...s.activeDoc, is_starred: false } : s.activeDoc,
      }))
    } else {
      set(s => ({
        documents: s.documents.map(d => d.id === id ? { ...d, is_starred: starred } : d),
        activeDoc: s.activeDoc?.id === id ? { ...s.activeDoc, is_starred: starred } : s.activeDoc,
      }))
    }
  },

  duplicateDoc: async (id) => {
    const doc = await officeApi.duplicate(id)
    // Re-fetch to respect current view filters (e.g. starred view shouldn't show the non-starred copy)
    await get().fetchDocuments()
    return doc
  },
}))
