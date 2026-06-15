import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useConfirm } from '@kubuno/sdk'
import { ConfirmDialog } from '@ui'
import {
  Plus, Star, Trash2, MoreVertical, LayoutTemplate, Clock, Search,
  RefreshCw, Copy, ExternalLink,
} from 'lucide-react'
import { presentationsApi, Presentation } from './api'
import { Button, StartPage, MenuDropdown, type MenuItem, type MenuDropdownPos } from '@ui'
import type { StartPageRecentItem, StartPageTab } from '@ui'
import { ModuleFileBrowser } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { formatDistanceToNow } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'

const PRES_MIME = 'application/vnd.oasis.opendocument.presentation'

// ── Props ─────────────────────────────────────────────────────────────────────

interface PresentationAppProps {
  recent?: boolean
  starred?: boolean
  trashed?: boolean
}

// ── Slide thumbnail placeholder ───────────────────────────────────────────────

function PresentationCard({
  pres,
  onOpen,
  onStar,
  onDuplicate,
  onTrash,
  onRestore,
  onDelete,
  trashed,
}: {
  pres: Presentation
  onOpen: () => void
  onStar: () => void
  onDuplicate: () => void
  onTrash: () => void
  onRestore: () => void
  onDelete: () => void
  trashed: boolean
}) {
  const { t, i18n } = useTranslation('office')
  const [menuPos, setMenuPos] = useState<MenuDropdownPos | null>(null)

  const menuItems: MenuItem[] = trashed
    ? [
        { type: 'action', icon: <RefreshCw size={14} />, label: t('presentations_restore'), onClick: onRestore },
        { type: 'action', icon: <Trash2 size={14} />, label: t('presentations_delete_permanently'), danger: true, onClick: onDelete },
      ]
    : [
        { type: 'action', icon: <Star size={14} className={pres.is_starred ? 'text-warning fill-warning' : undefined} />, label: pres.is_starred ? t('presentations_unstar') : t('presentations_star'), onClick: onStar },
        { type: 'action', icon: <Copy size={14} />, label: t('common_duplicate'), onClick: onDuplicate },
        { type: 'separator' },
        { type: 'action', icon: <Trash2 size={14} />, label: t('presentations_move_to_trash'), danger: true, onClick: onTrash },
      ]

  return (
    <div className="group relative flex flex-col rounded-xl border border-border hover:border-border-strong hover:shadow-md transition-all bg-white overflow-hidden cursor-pointer">
      {/* Thumbnail area */}
      <div
        className="relative w-full bg-surface-2 flex items-center justify-center"
        style={{ aspectRatio: '16/9' }}
        onClick={onOpen}
      >
        <div
          className="w-full h-full flex flex-col items-center justify-center gap-2"
          style={{ background: pres.theme?.bgColor ?? '#ffffff' }}
        >
          <LayoutTemplate size={32} className="text-text-tertiary opacity-40" />
          <div className="text-center px-4">
            <p className="text-xs text-text-tertiary font-medium truncate max-w-[120px]">{pres.title}</p>
          </div>
        </div>
        <div className="absolute bottom-1 right-1 bg-black/40 text-white text-[10px] px-1.5 py-0.5 rounded-full">
          {t('presentations_slide_count', { count: pres.slide_count })}
        </div>
      </div>

      {/* Info footer */}
      <div className="flex items-start justify-between px-3 py-2 gap-2">
        <div className="flex-1 min-w-0" onClick={onOpen}>
          <p className="text-sm font-medium text-text-primary truncate">{pres.title}</p>
          <p className="text-xs text-text-tertiary mt-0.5 flex items-center gap-1">
            <Clock size={10} />
            {formatDistanceToNow(new Date(pres.updated_at), { addSuffix: true, locale: getDateLocale(i18n.language) })}
          </p>
        </div>

        <div className="relative flex-shrink-0">
          <button
            onClick={e => {
              e.stopPropagation()
              const r = e.currentTarget.getBoundingClientRect()
              setMenuPos(p => p ? null : { top: r.bottom + 4, left: r.right - 176 })
            }}
            className="w-7 h-7 rounded-full flex items-center justify-center text-text-tertiary
                       hover:bg-surface-2 opacity-0 group-hover:opacity-100 transition-opacity"
          >
            <MoreVertical size={15} />
          </button>

          {menuPos && (
            <MenuDropdown pos={menuPos} onClose={() => setMenuPos(null)} items={menuItems} minWidth={176} />
          )}
        </div>
      </div>
    </div>
  )
}

// ── Main component ────────────────────────────────────────────────────────────

export default function PresentationApp({ recent, starred, trashed }: PresentationAppProps) {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [search, setSearch] = useState('')
  const [isOpeningFile, setIsOpeningFile] = useState(false)
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  // Récents pour la colonne du StartPage (page d'accueil de l'outil).
  const { data: recentData } = useQuery({
    queryKey: ['presentations', { recent: true }],
    queryFn:  () => presentationsApi.list({ recent: true, limit: 20 }),
    staleTime: 30_000,
    enabled:  !recent && !starred && !trashed,
  })

  const params = {
    search: search || undefined,
    starred: starred || undefined,
    trashed: trashed || undefined,
    recent: recent || undefined,
    limit: 100,
  }

  const { data, isLoading } = useQuery({
    queryKey: ['presentations', params],
    queryFn: () => presentationsApi.list(params),
  })

  const createMut = useMutation({
    mutationFn: () => presentationsApi.create({ title: t('presentations_default_title') }),
    onSuccess: (pres) => {
      qc.invalidateQueries({ queryKey: ['presentations'] })
      navigate(`/office/presentations/${pres.id}`)
    },
  })

  const starMut = useMutation({
    mutationFn: ({ id, is_starred }: { id: string; is_starred: boolean }) =>
      presentationsApi.update(id, { is_starred }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentations'] }),
  })

  const duplicateMut = useMutation({
    mutationFn: (id: string) => presentationsApi.duplicate(id),
    onSuccess: (pres) => {
      qc.invalidateQueries({ queryKey: ['presentations'] })
      navigate(`/office/presentations/${pres.id}`)
    },
  })

  const trashMut = useMutation({
    mutationFn: (id: string) => presentationsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentations'] }),
  })

  const restoreMut = useMutation({
    mutationFn: (id: string) => presentationsApi.restore(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentations'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => presentationsApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['presentations'] }),
  })

  const presentations = data?.presentations ?? []

  const pageTitle = trashed ? t('presentations_page_trash') : starred ? t('presentations_page_starred') : recent ? t('presentations_page_recent') : t('presentations_page_all')

  const handleOpenFile = (file: FileItem): boolean => {
    const meta = file.metadata as Record<string, unknown> | undefined
    const presId = meta?.office_presentation_id as string | undefined
    if (presId) {
      navigate(`/office/presentations/${presId}`)
      return true
    }
    if (file.mime_type !== PRES_MIME) return false
    if (isOpeningFile) return true
    setIsOpeningFile(true)
    presentationsApi.openByFile(file.id)
      .then(pres => navigate(`/office/presentations/${pres.id}`))
      .catch(() => { /* silently ignore */ })
      .finally(() => setIsOpeningFile(false))
    return true
  }

  if (!recent && !starred && !trashed) {
    const recentItems: StartPageRecentItem[] = (recentData?.presentations ?? []).map(p => ({
      id:       p.id,
      name:     p.title || t('common_untitled'),
      subtitle: formatDistanceToNow(new Date(p.updated_at), { addSuffix: true, locale: getDateLocale(i18n.language) }),
      icon:     <LayoutTemplate size={18} className="text-text-tertiary" />,
      onClick:  () => navigate(`/office/presentations/${p.id}`),
      actions: [
        { id: 'open',  label: t('presentations_open_in_editor'), icon: <ExternalLink size={15} />, onClick: () => navigate(`/office/presentations/${p.id}`) },
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
          onOpenFile={handleOpenFile}
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
              onClick: handleOpenFile,
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
      <>
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
        {confirmState && (
          <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
        )}
      </>
    )
  }

  return (
    <div className="flex flex-col h-full bg-white">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-border flex-shrink-0">
        <h1 className="text-lg font-medium text-text-primary">{pageTitle}</h1>
        <div className="flex items-center gap-3">
          {/* Search */}
          <div className="relative">
            <Search size={15} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary" />
            <input
              type="text"
              placeholder={t('common_search')}
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="pl-8 pr-3 py-1.5 text-sm border border-border rounded-full bg-surface-1
                         focus:outline-none focus:border-primary focus:bg-white transition-colors w-48"
            />
          </div>
          {!trashed && (
            <Button size="sm" icon={<Plus size={16} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
              {t('presentations_new')}
            </Button>
          )}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        {isLoading ? (
          <div className="flex items-center justify-center h-48 text-text-tertiary text-sm">
            {t('common_loading')}
          </div>
        ) : presentations.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-64 text-center">
            <LayoutTemplate size={48} className="text-text-tertiary opacity-30 mb-4" />
            <p className="text-text-secondary font-medium mb-1">
              {trashed ? t('presentations_trash_empty') : starred ? t('presentations_starred_empty') : t('presentations_empty_title')}
            </p>
            {!trashed && !starred && (
              <p className="text-sm text-text-tertiary mb-4">
                {t('presentations_empty_subtitle')}
              </p>
            )}
            {!trashed && (
              <Button icon={<Plus size={16} />} onClick={() => createMut.mutate()}>
                {t('presentations_new')}
              </Button>
            )}
          </div>
        ) : (
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
            {presentations.map(pres => (
              <PresentationCard
                key={pres.id}
                pres={pres}
                trashed={!!trashed}
                onOpen={() => navigate(`/office/presentations/${pres.id}`)}
                onStar={() => starMut.mutate({ id: pres.id, is_starred: !pres.is_starred })}
                onDuplicate={() => duplicateMut.mutate(pres.id)}
                onTrash={() => trashMut.mutate(pres.id)}
                onRestore={() => restoreMut.mutate(pres.id)}
                onDelete={async () => {
                  const ok = await confirm({
                    title:        t('presentations_delete_confirm_title'),
                    message:      t('presentations_delete_confirm_message'),
                    confirmLabel: t('common_delete'),
                    variant:      'danger',
                  })
                  if (ok) deleteMut.mutate(pres.id)
                }}
              />
            ))}
          </div>
        )}
      </div>

      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}
