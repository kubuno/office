// Start content (Accueil) for the Diagrams sub-module — reused by BOTH the landing
// page (DiagramsApp) AND the open editor's « Fichier » backstage tab
// (DiagramEditorPage). Recents + Browse + New. Kept in a standalone file to avoid a
// circular import (DiagramsApp already imports the editor).
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Network, Plus, Trash2, Copy, ExternalLink } from 'lucide-react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Button, StartPage } from '@ui'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { ModuleFileBrowser } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { diagramsApi } from './api'

const DIAGRAM_MIME = 'image/svg+xml'

// `onOpen` lets the host decide how a diagram is opened (route change in the landing
// page, or in-place navigation from the editor backstage).
export function DiagramsStartContent({ onOpen }: { onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const [isOpeningFile, setIsOpeningFile] = useState(false)

  const { data: recentData } = useQuery({
    queryKey: ['diagrams', { recent: true }],
    queryFn:  () => diagramsApi.list({ recent: true, limit: 20 }),
    staleTime: 30_000,
  })

  const createMut = useMutation({
    mutationFn: () => diagramsApi.create({ title: t('diagrams_default_title') }),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['diagrams'] }); onOpen(d.id) },
  })
  const dupMut = useMutation({
    mutationFn: (id: string) => diagramsApi.duplicate(id),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['diagrams'] }); onOpen(d.id) },
  })
  const trashMut = useMutation({
    mutationFn: (id: string) => diagramsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['diagrams'] }),
  })

  const handleOpenFile = (file: FileItem): boolean => {
    const meta = file.metadata as Record<string, unknown> | undefined
    const diagId = meta?.office_diagram_id as string | undefined
    if (diagId) {
      onOpen(diagId)
      return true
    }
    if (file.mime_type !== DIAGRAM_MIME) return false
    if (isOpeningFile) return true
    setIsOpeningFile(true)
    diagramsApi.openByFile(file.id)
      .then(d => onOpen(d.id))
      .catch(() => { /* silently ignore */ })
      .finally(() => setIsOpeningFile(false))
    return true
  }

  const recentItems: StartPageRecentItem[] = (recentData?.diagrams ?? []).map(d => ({
    id:       d.id,
    name:     d.title || t('common_untitled'),
    subtitle: format(new Date(d.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
    icon:     <Network size={18} className="text-text-tertiary" strokeWidth={1.5} />,
    onClick:  () => onOpen(d.id),
    actions: [
      { id: 'open',  label: t('diagrams_open_in_editor'), icon: <ExternalLink size={15} />, onClick: () => onOpen(d.id) },
      { id: 'dup',   label: t('common_duplicate', { defaultValue: 'Dupliquer' }),            icon: <Copy size={15} />,         onClick: () => dupMut.mutate(d.id) },
      { id: 'trash', label: t('common_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => trashMut.mutate(d.id) },
    ],
  }))

  const tabs: StartPageTab[] = [{
    id: 'browse', label: t('diagrams_tab_browse'),
    content: (
      <ModuleFileBrowser
        folderPathPrefix="Office/Diagrams"
        title={t('diagrams_browse_title')}
        onOpenFile={handleOpenFile}
        fileTypeModuleId="office-diagrams"
        toolbarContent={
          <Button size="sm" icon={<Plus size={14} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('diagrams_new')}
          </Button>
        }
        fileContextActions={[
          {
            id:      'open-editor',
            label:   t('diagrams_open_in_editor'),
            icon:    ExternalLink,
            visible: (f) =>
              !!(f.metadata as Record<string, unknown>)?.office_diagram_id ||
              f.mime_type === DIAGRAM_MIME,
            onClick: handleOpenFile,
          },
        ]}
        emptyState={
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <Network size={48} className="text-text-tertiary mb-4 opacity-30" strokeWidth={1.5} />
            <p className="text-text-secondary font-medium mb-1">{t('diagrams_empty_title')}</p>
            <button onClick={() => createMut.mutate()} className="text-sm text-primary hover:underline mt-2">
              {t('diagrams_empty_create')}
            </button>
          </div>
        }
      />
    ),
  }]

  return (
    <StartPage
      recentTitle={t('diagrams_tab_recent')}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <Network size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
          <p className="text-text-tertiary text-xs">{t('diagrams_recent_empty_hint')}</p>
        </div>
      }
      tabs={tabs}
      defaultTab="browse"
    />
  )
}
