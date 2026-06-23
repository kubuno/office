// Start content (Accueil) shared by the landing page (ModuleHome) AND the open
// editor's « Fichier » backstage tab. Reproduces the recents column + browse tab
// + « New » button. Kept in its own file to avoid the circular import that would
// arise from importing it out of PresentationApp (which already imports the editor).
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Copy, Trash2, ExternalLink, LayoutTemplate } from 'lucide-react'
import { formatDistanceToNow } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Button, StartPage } from '@ui'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { ModuleFileBrowser } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { presentationsApi } from './api'

const PRES_MIME = 'application/vnd.oasis.opendocument.presentation'

// `onOpen(id)` opens a presentation by id (editor route). `onOpenFile` resolves a
// Drive file to a presentation and opens it (returns true if handled).
export function PresentationStartContent({
  onOpen,
  onOpenFile,
}: {
  onOpen: (id: string) => void
  onOpenFile: (file: FileItem) => boolean
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()

  const { data: recentData } = useQuery({
    queryKey: ['presentations', { recent: true }],
    queryFn:  () => presentationsApi.list({ recent: true, limit: 20 }),
    staleTime: 30_000,
  })

  const createMut = useMutation({
    mutationFn: () => presentationsApi.create({ title: t('presentations_default_title') }),
    onSuccess: (pres) => { qc.invalidateQueries({ queryKey: ['presentations'] }); onOpen(pres.id) },
  })
  const duplicateMut = useMutation({
    mutationFn: (id: string) => presentationsApi.duplicate(id),
    onSuccess: (pres) => { qc.invalidateQueries({ queryKey: ['presentations'] }); onOpen(pres.id) },
  })
  const trashMut = useMutation({
    mutationFn: (id: string) => presentationsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentations'] }),
  })

  const recentItems: StartPageRecentItem[] = (recentData?.presentations ?? []).map(p => ({
    id:       p.id,
    name:     p.title || t('common_untitled'),
    subtitle: formatDistanceToNow(new Date(p.updated_at), { addSuffix: true, locale: getDateLocale(i18n.language) }),
    icon:     <LayoutTemplate size={18} className="text-text-tertiary" />,
    onClick:  () => onOpen(p.id),
    actions: [
      { id: 'open',  label: t('presentations_open_in_editor'), icon: <ExternalLink size={15} />, onClick: () => onOpen(p.id) },
      { id: 'dup',   label: t('common_duplicate', { defaultValue: 'Dupliquer' }),                icon: <Copy size={15} />,         onClick: () => duplicateMut.mutate(p.id) },
      { id: 'trash', label: t('common_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => trashMut.mutate(p.id) },
    ],
  }))

  const tabs: StartPageTab[] = [{
    id: 'browse', label: t('presentations_tab_browse'),
    content: (
      <ModuleFileBrowser
        folderPathPrefix="Office/Presentations"
        title={t('presentations_browse_title')}
        onOpenFile={onOpenFile}
        fileTypeModuleId="office-presentations"
        toolbarContent={
          <Button size="sm" icon={<Plus size={15} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('presentations_new')}
          </Button>
        }
        fileContextActions={[
          {
            id:      'open-editor',
            label:   t('presentations_open_in_editor'),
            icon:    ExternalLink,
            visible: (f) =>
              !!(f.metadata as Record<string, unknown>)?.office_presentation_id ||
              f.mime_type === PRES_MIME,
            onClick: onOpenFile,
          },
        ]}
        emptyState={
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <LayoutTemplate size={48} className="text-text-tertiary mb-4 opacity-30" />
            <p className="text-text-secondary font-medium mb-1">{t('presentations_empty_title')}</p>
            <button onClick={() => createMut.mutate()} className="text-sm text-primary hover:underline mt-2">
              {t('presentations_empty_create')}
            </button>
          </div>
        }
      />
    ),
  }]

  return (
    <StartPage
      recentTitle={t('presentations_tab_recent')}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <LayoutTemplate size={32} className="text-text-tertiary opacity-30" />
          <p className="text-text-tertiary text-xs">{t('presentations_recent_empty_hint')}</p>
        </div>
      }
      tabs={tabs}
      defaultTab="browse"
    />
  )
}
