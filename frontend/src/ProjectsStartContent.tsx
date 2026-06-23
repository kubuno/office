// Shared "Accueil" start content for the Projects sub-module — reused by both the
// landing page (ProjectsApp, no project open) and the editor backstage (« Fichier »
// tab of ProjectEditorPage). Recents + browse + New. Kept in a standalone file to
// avoid a circular import (ProjectsApp already imports the editor).
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { FolderKanban, Plus, Trash2, Copy, ExternalLink } from 'lucide-react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Button, StartPage } from '@ui'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { ModuleFileBrowser } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { projectsApi } from './api'

const PROJECT_MIME = 'application/json'

// Start content (recents + browse + New). Navigation/creation are handled here so
// the same UI works from the landing page and from inside the editor backstage.
export function ProjectsStartContent() {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [isOpeningFile, setIsOpeningFile] = useState(false)

  const { data: recentData } = useQuery({
    queryKey: ['projects', { recent: true }],
    queryFn:  () => projectsApi.list({ recent: true }),
    staleTime: 30_000,
  })

  const createMut = useMutation({
    mutationFn: () => projectsApi.create({ title: t('proj_new_project') }),
    onSuccess: (p) => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      navigate(`/office/projects/${p.id}`)
    },
  })

  const trashMut = useMutation({
    mutationFn: (id: string) => projectsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const duplicateMut = useMutation({
    mutationFn: (id: string) => projectsApi.duplicate(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const handleOpenFile = (file: FileItem): boolean => {
    const meta = file.metadata as Record<string, unknown> | undefined
    const projId = meta?.office_project_id as string | undefined
    if (projId) {
      navigate(`/office/projects/${projId}`)
      return true
    }
    if (file.mime_type !== PROJECT_MIME) return false
    if (isOpeningFile) return true
    setIsOpeningFile(true)
    projectsApi.openByFile(file.id)
      .then(p => navigate(`/office/projects/${p.id}`))
      .catch(() => { /* silently ignore */ })
      .finally(() => setIsOpeningFile(false))
    return true
  }

  const recentItems: StartPageRecentItem[] = (recentData?.projects ?? []).map(p => ({
    id:       p.id,
    name:     p.title || t('common_untitled'),
    subtitle: format(new Date(p.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
    icon:     <FolderKanban size={18} style={{ color: p.color }} />,
    onClick:  () => navigate(`/office/projects/${p.id}`),
    actions: [
      { id: 'open',  label: t('proj_open_in_manager'), icon: <ExternalLink size={15} />, onClick: () => navigate(`/office/projects/${p.id}`) },
      { id: 'dup',   label: t('common_duplicate', { defaultValue: 'Dupliquer' }),         icon: <Copy size={15} />,         onClick: () => duplicateMut.mutate(p.id) },
      { id: 'trash', label: t('common_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => trashMut.mutate(p.id) },
    ],
  }))

  const tabs: StartPageTab[] = [{
    id: 'browse', label: t('proj_tab_browse'),
    content: (
      <ModuleFileBrowser
        folderPathPrefix="Office/Projects"
        title={t('proj_page_projects')}
        onOpenFile={handleOpenFile}
        fileTypeModuleId="office-projects"
        toolbarContent={
          <Button size="sm" icon={<Plus size={14} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('proj_new_project')}
          </Button>
        }
        fileContextActions={[
          {
            id:      'open-editor',
            label:   t('proj_open_in_manager'),
            icon:    ExternalLink,
            visible: (f) =>
              !!(f.metadata as Record<string, unknown>)?.office_project_id ||
              f.mime_type === PROJECT_MIME,
            onClick: handleOpenFile,
          },
        ]}
        emptyState={
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <FolderKanban size={48} className="text-text-tertiary mb-4 opacity-30" />
            <p className="text-text-secondary font-medium mb-1">{t('proj_empty')}</p>
            <button onClick={() => createMut.mutate()} className="text-sm text-primary hover:underline mt-2">
              {t('proj_create_first')}
            </button>
          </div>
        }
      />
    ),
  }]

  return (
    <StartPage
      recentTitle={t('proj_tab_recent')}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <FolderKanban size={32} className="text-text-tertiary opacity-30" />
          <p className="text-text-tertiary text-xs">{t('proj_no_recent')}</p>
        </div>
      }
      tabs={tabs}
      defaultTab="browse"
    />
  )
}
