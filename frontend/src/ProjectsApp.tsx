import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  FolderKanban, Plus, Search, Star, Trash2, MoreVertical,
  Clock, CheckCircle, PauseCircle, XCircle, Loader2, Copy, ExternalLink,
} from 'lucide-react'
import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { projectsApi, type Project } from './api'
import { Button, StartPage } from '@ui'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { ModuleFileBrowser } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'

const PROJECT_MIME = 'application/json'

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  recent?:  boolean
  starred?: boolean
  trashed?: boolean
}

const STATUS_LABEL_KEYS: Record<string, string> = {
  active:    'proj_pstatus_active',
  on_hold:   'proj_pstatus_on_hold',
  completed: 'proj_pstatus_completed',
  cancelled: 'proj_pstatus_cancelled',
}

const STATUS_ICONS: Record<string, React.ReactNode> = {
  active:    <Clock size={12} className="text-primary" />,
  on_hold:   <PauseCircle size={12} className="text-warning" />,
  completed: <CheckCircle size={12} className="text-success" />,
  cancelled: <XCircle size={12} className="text-danger" />,
}

function ProjectCard({ project, onTrash, onRestore, onDelete, onStar, onDuplicate }: {
  project: Project
  onTrash:     (id: string) => void
  onRestore:   (id: string) => void
  onDelete:    (id: string) => void
  onStar:      (id: string, v: boolean) => void
  onDuplicate: (id: string) => void
}) {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()

  return (
    <div
      className="group relative flex flex-col gap-3 p-4 bg-white border border-[#e8eaed]
                 rounded-xl hover:shadow-md transition-shadow cursor-pointer"
      onClick={() => !project.is_trashed && navigate(`/office/projects/${project.id}`)}
    >
      {/* Color strip */}
      <div
        className="absolute top-0 left-0 right-0 h-1 rounded-t-xl"
        style={{ background: project.color }}
      />

      {/* Header */}
      <div className="flex items-start gap-2 mt-1">
        <div
          className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
          style={{ background: project.color + '22' }}
        >
          <FolderKanban size={16} style={{ color: project.color }} />
        </div>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-text-primary truncate">{project.title}</p>
          {project.description && (
            <p className="text-xs text-text-tertiary truncate mt-0.5">{project.description}</p>
          )}
        </div>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <button
              className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-surface-2 text-text-tertiary"
              onClick={e => e.stopPropagation()}
            >
              <MoreVertical size={14} />
            </button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              className="z-50 bg-white border border-[#e8eaed] rounded-xl shadow-lg py-1 min-w-[160px]"
              align="end"
              onClick={e => e.stopPropagation()}
            >
              {project.is_trashed ? (
                <>
                  <DropdownMenu.Item
                    className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    onSelect={() => onRestore(project.id)}
                  >
                    <Clock size={14} /> {t('proj_restore')}
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    className="flex items-center gap-2 px-3 py-2 text-sm text-danger hover:bg-danger/5 cursor-pointer outline-none"
                    onSelect={() => onDelete(project.id)}
                  >
                    <Trash2 size={14} /> {t('proj_delete_permanently')}
                  </DropdownMenu.Item>
                </>
              ) : (
                <>
                  <DropdownMenu.Item
                    className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    onSelect={() => navigate(`/office/projects/${project.id}`)}
                  >
                    <FolderKanban size={14} /> {t('proj_open')}
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    onSelect={() => onStar(project.id, !project.is_starred)}
                  >
                    <Star size={14} className={project.is_starred ? 'text-warning fill-warning' : ''} />
                    {project.is_starred ? t('proj_unstar') : t('proj_star')}
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    className="flex items-center gap-2 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    onSelect={() => onDuplicate(project.id)}
                  >
                    <Copy size={14} /> {t('common_duplicate')}
                  </DropdownMenu.Item>
                  <DropdownMenu.Separator className="my-1 h-px bg-border" />
                  <DropdownMenu.Item
                    className="flex items-center gap-2 px-3 py-2 text-sm text-danger hover:bg-danger/5 cursor-pointer outline-none"
                    onSelect={() => onTrash(project.id)}
                  >
                    <Trash2 size={14} /> {t('proj_move_to_trash')}
                  </DropdownMenu.Item>
                </>
              )}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      </div>

      {/* Status badge */}
      <div className="flex items-center gap-1.5">
        {STATUS_ICONS[project.status]}
        <span className="text-xs text-text-secondary">{STATUS_LABEL_KEYS[project.status] ? t(STATUS_LABEL_KEYS[project.status]) : project.status}</span>
        {project.is_starred && <Star size={12} className="ml-auto text-warning fill-warning" />}
      </div>

      {/* Dates */}
      {(project.start_date || project.end_date) && (
        <div className="text-xs text-text-tertiary">
          {project.start_date && format(new Date(project.start_date), 'd MMM', { locale: getDateLocale(i18n.language) })}
          {project.start_date && project.end_date && ' → '}
          {project.end_date && format(new Date(project.end_date), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })}
        </div>
      )}

      {/* Footer */}
      <div className="text-xs text-text-tertiary">
        {t('proj_modified_on', { date: format(new Date(project.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) }) })}
      </div>
    </div>
  )
}

export default function ProjectsApp({ recent, starred, trashed }: Props) {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [search, setSearch] = useState('')
  const [isOpeningFile, setIsOpeningFile] = useState(false)

  const { data: recentData } = useQuery({
    queryKey: ['projects', { recent: true }],
    queryFn:  () => projectsApi.list({ recent: true }),
    staleTime: 30_000,
    enabled:  !recent && !starred && !trashed,
  })

  const { data, isLoading } = useQuery({
    queryKey: ['projects', { recent, starred, trashed, search }],
    queryFn: () => projectsApi.list({ recent, starred, trashed, search: search || undefined }),
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

  const restoreMut = useMutation({
    mutationFn: (id: string) => projectsApi.restore(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const starMut = useMutation({
    mutationFn: ({ id, val }: { id: string; val: boolean }) => projectsApi.update(id, { is_starred: val }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const duplicateMut = useMutation({
    mutationFn: (id: string) => projectsApi.duplicate(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const projects = data?.projects ?? []

  const pageTitle = trashed ? t('proj_page_trash') : starred ? t('proj_page_starred') : recent ? t('proj_tab_recent') : t('proj_page_projects')

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

  if (!recent && !starred && !trashed) {
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

  return (
    <div className="flex flex-col h-full bg-surface-1">
      {/* Header */}
      <div className="flex items-center gap-3 px-6 py-4 border-b border-border bg-white">
        <FolderKanban size={20} className="text-primary" />
        <h1 className="text-lg font-semibold text-text-primary">{pageTitle}</h1>
        <div className="flex-1" />
        <div className="relative">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary" />
          <input
            type="text"
            placeholder={t('common_search')}
            value={search}
            onChange={e => setSearch(e.target.value)}
            className="pl-8 pr-3 py-1.5 text-sm bg-surface-1 border border-border rounded-full
                       focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent w-48"
          />
        </div>
        {!trashed && (
          <Button size="sm" icon={<Plus size={14} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('proj_new_project')}
          </Button>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-40">
            <Loader2 size={20} className="animate-spin text-text-tertiary" />
          </div>
        ) : projects.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-40 text-center gap-3">
            <FolderKanban size={40} className="text-text-tertiary" />
            <p className="text-sm text-text-secondary">
              {trashed ? t('proj_trash_empty') : search ? t('proj_no_results') : t('proj_empty_yet')}
            </p>
            {!trashed && !search && (
              <button
                onClick={() => createMut.mutate()}
                className="text-sm text-primary hover:underline"
              >
                {t('proj_create_first')}
              </button>
            )}
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-4">
            {projects.map(p => (
              <ProjectCard
                key={p.id}
                project={p}
                onTrash={id => trashMut.mutate(id)}
                onRestore={id => restoreMut.mutate(id)}
                onDelete={id => deleteMut.mutate(id)}
                onStar={(id, val) => starMut.mutate({ id, val })}
                onDuplicate={id => duplicateMut.mutate(id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
