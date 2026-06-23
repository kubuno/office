import { useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { FileText, Plus, ExternalLink, Loader2, Upload, Trash2, FileDown, X } from 'lucide-react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Button, StartPage } from '@ui'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { getDateLocale } from '@kubuno/sdk'
import { format } from 'date-fns'
import { ModuleFileBrowser } from '@kubuno/drive'
import { filesApi } from '@kubuno/drive'
import type { FileItem, Folder } from '@kubuno/drive'
import { useOfficeStore } from './store'
import { useAuthStore } from '@kubuno/sdk'
import { api } from '@kubuno/sdk'
import { officeApi } from './api'

// ── Constants ──────────────────────────────────────────────────────────────────


const FOLDER_PATH = 'Office/Documents'
const TEMPLATES_PATH = ['Office', 'Templates']

// Formats importables en édition sans office_doc_id en métadonnée
export const EDITABLE_IMPORT_MIME_TYPES = [
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'application/vnd.oasis.opendocument.text',
  'application/vnd.oasis.opendocument.text-template',
]

const TEMPLATE_ACCEPT = '.docx,.odt,.dotx,.ott'

const RECENT_KEY  = 'kubuno:office:recent-docs'
const MAX_RECENT  = 20

// ── Recent-docs helpers (localStorage) ────────────────────────────────────────

function addToRecent(file: FileItem): void {
  try {
    const prev: FileItem[] = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]')
    const next = [file, ...prev.filter(f => f.id !== file.id)].slice(0, MAX_RECENT)
    localStorage.setItem(RECENT_KEY, JSON.stringify(next))
  } catch { /* ignore */ }
}

function getRecentDocs(): FileItem[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]')
  } catch {
    return []
  }
}

// ── Templates folder helpers ───────────────────────────────────────────────────

async function ensureFolder(parentId: string | null, name: string): Promise<string> {
  const { folders } = await filesApi.listFolders(parentId)
  const found = folders.find(f => f.name === name)
  if (found) return found.id
  const { folder } = await filesApi.createFolder(name, parentId)
  return folder.id
}

async function ensureTemplatesFolderId(): Promise<string> {
  let parentId: string | null = null
  for (const seg of TEMPLATES_PATH) {
    parentId = await ensureFolder(parentId, seg)
  }
  return parentId as string
}

async function resolveTemplatesFolder(): Promise<Folder | null> {
  let parentId: string | null = null
  let folder: Folder | null = null
  for (const seg of TEMPLATES_PATH) {
    const { folders } = await filesApi.listFolders(parentId)
    const match = folders.find(f => f.name === seg)
    if (!match) return null
    folder = match
    parentId = match.id
  }
  return folder
}



// ── Onglet Modèles ─────────────────────────────────────────────────────────────

export function ModelsTab() {
  const { t, i18n } = useTranslation('office')
  const navigate    = useNavigate()
  const qc          = useQueryClient()
  const importRef   = useRef<HTMLInputElement>(null)
  const [importing, setImporting] = useState(false)

  const { data: folder, isLoading: folderLoading } = useQuery({
    queryKey: ['office-templates-folder'],
    queryFn:  resolveTemplatesFolder,
    refetchOnWindowFocus: false,
    staleTime: 5 * 60 * 1000,
  })

  const { data: filesData, isLoading: filesLoading } = useQuery({
    queryKey: ['office-templates-files', folder?.id],
    queryFn:  () => filesApi.listFiles(folder!.id),
    enabled:  !!folder,
    refetchOnWindowFocus: false,
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => filesApi.deleteFile(id),
    onSuccess:  () => qc.invalidateQueries({ queryKey: ['office-templates-files'] }),
  })

  const handleImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    setImporting(true)
    try {
      const folderId = await ensureTemplatesFolderId()
      await filesApi.uploadFile(file, folderId)
      qc.invalidateQueries({ queryKey: ['office-templates-folder'] })
      qc.invalidateQueries({ queryKey: ['office-templates-files'] })
    } finally {
      setImporting(false)
    }
  }

  const handleUseTemplate = async (file: FileItem) => {
    try {
      const token = useAuthStore.getState().accessToken
      const res   = await fetch(filesApi.downloadUrl(file.id), {
        headers: { Authorization: `Bearer ${token}` },
      })
      if (!res.ok) return
      const blob     = await res.blob()
      const template = new File([blob], file.name, { type: file.mime_type })
      const fd       = new FormData()
      fd.append('file', template)
      const r = await api.post<{ document: { id: string } }>('/documents/import', fd)
      navigate(`/office/${r.data.document.id}`, { state: { from: '/office/documents' } })
    } catch { /* silently ignore */ }
  }

  const isLoading = folderLoading || filesLoading
  const templates = filesData?.files ?? []

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <input
        ref={importRef}
        type="file"
        accept={TEMPLATE_ACCEPT}
        hidden
        onChange={handleImport}
      />

      {/* Header */}
      <div className="flex items-center justify-between px-6 pt-6 pb-3 flex-shrink-0">
        <h1 className="text-xl font-medium text-text-primary">{t('documents_templates_title')}</h1>
        <Button
          size="sm"
          variant="secondary"
          icon={importing ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />}
          onClick={() => importRef.current?.click()}
          disabled={importing}
        >
          {t('documents_templates_import')}
        </Button>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-y-auto px-6 pb-6">
        {isLoading ? (
          <div className="flex items-center gap-2 text-text-secondary text-sm py-16 justify-center">
            <Loader2 size={18} className="animate-spin" />
            {t('common_loading')}
          </div>
        ) : templates.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center gap-3">
            <FileText size={48} className="text-text-tertiary opacity-30" />
            <p className="text-text-secondary font-medium">{t('documents_templates_empty_title')}</p>
            <p className="text-text-tertiary text-xs">
              {t('documents_templates_empty_hint')}
            </p>
            <button
              onClick={() => importRef.current?.click()}
              className="text-sm text-primary hover:underline mt-1"
            >
              {t('documents_templates_import')}
            </button>
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-4">
            {templates.map(tpl => (
              <div
                key={tpl.id}
                className="group relative flex flex-col items-start rounded-xl border border-border
                           hover:shadow-md transition-all overflow-hidden bg-white"
              >
                {/* Prévisualisation */}
                <div className="w-full bg-surface-1 flex items-center justify-center" style={{ height: 130 }}>
                  <div className="w-3/4 h-4/5 bg-white rounded shadow-sm flex flex-col p-2 gap-1">
                    <div className="h-2 bg-surface-2 rounded w-3/5" />
                    <div className="h-1.5 bg-surface-2 rounded w-full opacity-50 mt-1" />
                    <div className="h-1.5 bg-surface-2 rounded w-5/6 opacity-50" />
                    <div className="h-1.5 bg-surface-2 rounded w-full opacity-50" />
                    <div className="h-1.5 bg-surface-2 rounded w-4/6 opacity-50" />
                  </div>
                </div>

                {/* Infos */}
                <div className="px-3 py-2 w-full">
                  <p className="text-sm font-medium text-text-primary truncate" title={tpl.name}>
                    {tpl.name}
                  </p>
                  <p className="text-xs text-text-tertiary mt-0.5">
                    {format(new Date(tpl.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })}
                  </p>
                </div>

                {/* Actions */}
                <div className="flex items-center gap-1.5 px-3 pb-3 w-full">
                  <Button
                    size="sm"
                    icon={<FileDown size={12} />}
                    onClick={() => handleUseTemplate(tpl)}
                    className="flex-1"
                  >
                    {t('documents_templates_use')}
                  </Button>
                  <button
                    onClick={() => deleteMut.mutate(tpl.id)}
                    className="p-1.5 rounded-lg hover:bg-danger/10 text-text-tertiary hover:text-danger transition-colors"
                    title={t('documents_templates_delete_tooltip')}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

// ── Composant principal ────────────────────────────────────────────────────────

// Contenu « démarrage » (récents + Parcourir + Modèles) — RÉUTILISÉ par l'onglet
// Fichier (backstage) de l'éditeur et par la page d'accueil sans document.
export function DocumentsStartContent() {
  const { t, i18n } = useTranslation('office')
  const navigate          = useNavigate()
  const { createDoc }     = useOfficeStore()
  const [isOpeningFile, setIsOpeningFile] = useState(false)
  const [recents, setRecents] = useState<FileItem[]>(() => getRecentDocs())

  const removeFromRecent = (id: string) => {
    const next = getRecentDocs().filter(f => f.id !== id)
    localStorage.setItem(RECENT_KEY, JSON.stringify(next))
    setRecents(next)
  }

  const handleNew = async () => {
    const doc = await createDoc()
    navigate(`/office/${doc.id}`, { state: { from: '/office/documents' } })
  }

  const handleOpenFile = (file: FileItem): boolean => {
    const docId = (file.metadata as Record<string, unknown> | undefined)?.office_doc_id as string | undefined
    if (docId) {
      addToRecent(file)
      navigate(`/office/${docId}`, { state: { from: '/office/documents' } })
      return true
    }

    // Importer les formats éditables (.docx, .odt) via l'endpoint office
    if (!EDITABLE_IMPORT_MIME_TYPES.includes(file.mime_type)) return false
    if (isOpeningFile) return true

    setIsOpeningFile(true)
    officeApi.openByFile(file.id)
      .then(doc => {
        addToRecent(file)
        navigate(`/office/${doc.id}`, { state: { from: '/office/documents' } })
      })
      .catch(() => { /* erreur silencieuse — le download fallback n'est pas déclenché */ })
      .finally(() => setIsOpeningFile(false))

    return true  // empêche le téléchargement en fallback
  }

  // Colonne « Récents » : 20 derniers documents ouverts (MAX_RECENT).
  const recentItems: StartPageRecentItem[] = recents.map(file => ({
    id:       file.id,
    name:     file.name,
    subtitle: format(new Date(file.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
    icon:     <FileText size={18} className="text-blue-500" />,
    onClick:  () => handleOpenFile(file),
    actions: [
      { id: 'open',   label: t('documents_open_in_editor'), icon: <ExternalLink size={15} />, onClick: () => handleOpenFile(file) },
      { id: 'remove', label: t('startpage_remove_recent', { defaultValue: 'Retirer des récents' }), icon: <X size={15} />, onClick: () => removeFromRecent(file.id) },
    ],
  }))

  const tabs: StartPageTab[] = [
    {
      id:    'browse',
      label: t('documents_tab_browse'),
      content: (
        <ModuleFileBrowser
          folderPathPrefix={FOLDER_PATH}
          title={t('documents_browse_title')}
          onOpenFile={handleOpenFile}
          fileTypeModuleId="office-documents"
          toolbarContent={
            <Button size="sm" icon={<Plus size={15} />} onClick={handleNew}>
              {t('documents_new')}
            </Button>
          }
          fileContextActions={[
            {
              id:      'open-editor',
              label:   t('documents_open_in_editor'),
              icon:    ExternalLink,
              visible: (f) =>
                !!(f.metadata as Record<string, unknown>)?.office_doc_id ||
                EDITABLE_IMPORT_MIME_TYPES.includes(f.mime_type),
              onClick: handleOpenFile,
            },
          ]}
          emptyState={
            <div className="flex flex-col items-center justify-center py-24 text-center">
              <FileText size={48} className="text-text-tertiary mb-4 opacity-30" />
              <p className="text-text-secondary font-medium mb-1">{t('documents_empty_title')}</p>
              <div className="flex items-center gap-3 mt-3">
                <button onClick={handleNew} className="text-sm text-primary hover:underline">
                  {t('documents_empty_create')}
                </button>
              </div>
            </div>
          }
        />
      ),
    },
    { id: 'models', label: t('documents_tab_models'), content: <ModelsTab /> },
  ]

  return (
    <StartPage
      recentTitle={t('documents_tab_recent')}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <FileText size={32} className="text-text-tertiary opacity-30" />
          <p className="text-text-tertiary text-xs">{t('documents_recent_empty_hint')}</p>
        </div>
      }
      tabs={tabs}
      defaultTab="browse"
    />
  )
}

export default function DocumentsApp() {
  return <DocumentsStartContent />
}
