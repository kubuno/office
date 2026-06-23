import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import {
  Network, Plus, Search, Star, Trash2, MoreVertical,
  RotateCcw, Loader2, Copy,
} from 'lucide-react'
import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { diagramsApi, type Diagram } from './api'
import { Button } from '@ui'
import { ModuleHome } from './ribbon/ModuleBackstage'
import { DiagramsStartContent } from './DiagramsStartContent'
import { THEME_DIAGRAMS } from './ribbon/officeThemes'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  recent?:  boolean
  starred?: boolean
  trashed?: boolean
}

const TYPE_KEYS: Record<string, string> = {
  flowchart:    'diagrams_type_flowchart',
  network:      'diagrams_type_network',
  uml:          'diagrams_type_uml',
  architecture: 'diagrams_type_architecture',
  general:      'diagrams_type_general',
}

function DiagramCard({ d, trashed, onOpen, onStar, onDuplicate, onTrash, onRestore, onDelete }: {
  d: Diagram
  trashed: boolean
  onOpen:      () => void
  onStar:      () => void
  onDuplicate: () => void
  onTrash:     () => void
  onRestore:   () => void
  onDelete:    () => void
}) {
  const { t, i18n } = useTranslation('office')
  return (
    <div
      className="group relative bg-white border border-border rounded-xl overflow-hidden
                 hover:shadow-md hover:border-border-strong transition-all cursor-pointer"
      onDoubleClick={onOpen}
    >
      {/* Preview area */}
      <div
        className="h-32 flex items-center justify-center"
        style={{ background: '#f8f9ff' }}
        onClick={onOpen}
      >
        <Network size={36} className="text-primary/40" strokeWidth={1.5} />
      </div>

      {/* Footer */}
      <div className="px-3 py-2.5 border-t border-border">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <p className="text-sm font-medium text-text-primary truncate">{d.title}</p>
            <p className="text-xs text-text-tertiary mt-0.5">
              {TYPE_KEYS[d.diagram_type] ? t(TYPE_KEYS[d.diagram_type]) : d.diagram_type} ·{' '}
              {format(new Date(d.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) })}
            </p>
          </div>

          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button
                className="opacity-0 group-hover:opacity-100 w-7 h-7 flex items-center justify-center
                           rounded hover:bg-surface-2 text-text-secondary transition-all shrink-0"
                onClick={(e) => e.stopPropagation()}
              >
                <MoreVertical size={15} />
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="end"
                sideOffset={4}
                className="min-w-44 bg-white rounded-[5px] border border-border shadow-lg py-1 z-50"
                onClick={(e) => e.stopPropagation()}
              >
                {!trashed ? (
                  <>
                    <DropdownMenu.Item
                      onSelect={onStar}
                      className="flex items-center gap-2.5 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    >
                      <Star size={14} className={d.is_starred ? 'text-warning fill-warning' : 'text-text-secondary'} />
                      {d.is_starred ? t('diagrams_unstar') : t('diagrams_star')}
                    </DropdownMenu.Item>
                    <DropdownMenu.Item
                      onSelect={onDuplicate}
                      className="flex items-center gap-2.5 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    >
                      <Copy size={14} className="text-text-secondary" />
                      {t('common_duplicate')}
                    </DropdownMenu.Item>
                    <DropdownMenu.Separator className="my-1 h-px bg-border mx-2" />
                    <DropdownMenu.Item
                      onSelect={onTrash}
                      className="flex items-center gap-2.5 px-3 py-2 text-sm text-danger hover:bg-danger-light cursor-pointer outline-none"
                    >
                      <Trash2 size={14} />
                      {t('diagrams_move_to_trash')}
                    </DropdownMenu.Item>
                  </>
                ) : (
                  <>
                    <DropdownMenu.Item
                      onSelect={onRestore}
                      className="flex items-center gap-2.5 px-3 py-2 text-sm text-text-primary hover:bg-surface-1 cursor-pointer outline-none"
                    >
                      <RotateCcw size={14} className="text-text-secondary" />
                      {t('diagrams_restore')}
                    </DropdownMenu.Item>
                    <DropdownMenu.Item
                      onSelect={onDelete}
                      className="flex items-center gap-2.5 px-3 py-2 text-sm text-danger hover:bg-danger-light cursor-pointer outline-none"
                    >
                      <Trash2 size={14} />
                      {t('diagrams_delete_permanently')}
                    </DropdownMenu.Item>
                  </>
                )}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>

        {d.is_starred && !trashed && (
          <Star size={10} className="absolute top-2 right-2 text-warning fill-warning" />
        )}
      </div>
    </div>
  )
}

export default function DiagramsApp({ recent, starred, trashed }: Props) {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const qc       = useQueryClient()
  const [search, setSearch] = useState('')

  const { data, isLoading } = useQuery({
    queryKey: ['diagrams', { recent, starred, trashed, search }],
    queryFn:  () => diagramsApi.list({ recent, starred, trashed, search: search || undefined }),
  })

  const diagrams = data?.diagrams ?? []

  const createMut = useMutation({
    mutationFn: () => diagramsApi.create({ title: t('diagrams_default_title') }),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['diagrams'] })
      navigate(`/office/diagrams/${d.id}`)
    },
  })

  const starMut = useMutation({
    mutationFn: ({ id, starred }: { id: string; starred: boolean }) =>
      diagramsApi.update(id, { is_starred: starred }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['diagrams'] }),
  })

  const dupMut = useMutation({
    mutationFn: (id: string) => diagramsApi.duplicate(id),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['diagrams'] })
      navigate(`/office/diagrams/${d.id}`)
    },
  })

  const trashMut = useMutation({
    mutationFn: (id: string) => diagramsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['diagrams'] }),
  })

  const restoreMut = useMutation({
    mutationFn: (id: string) => diagramsApi.restore(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['diagrams'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => diagramsApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['diagrams'] }),
  })

  const title = trashed ? t('diagrams_page_trash') : starred ? t('diagrams_page_starred') : recent ? t('diagrams_page_recent') : t('diagrams_page_all')

  if (!recent && !starred && !trashed) {
    return (
      <ModuleHome
        theme={THEME_DIAGRAMS}
        title={t('diagrams_title', { defaultValue: 'Diagrams' })}
        titleIcon={<Network size={16} className="text-white/90 flex-shrink-0" />}
        fileLabel={t('doc_bs_file', { defaultValue: 'Fichier' })}
        homeLabel={t('doc_bs_home', { defaultValue: 'Accueil' })}
        onBack={() => navigate('/office')}
        startContent={
          <DiagramsStartContent onOpen={(did) => navigate(`/office/diagrams/${did}`)} />
        }
      />
    )
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-6 pt-5 pb-3 flex-shrink-0">
        <h1 className="text-xl font-medium text-text-primary">{title}</h1>
        {!trashed && (
          <Button icon={<Plus size={14} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('diagrams_new')}
          </Button>
        )}
      </div>

      {/* Search */}
      <div className="px-6 pb-4 flex-shrink-0">
        <div className="relative max-w-xs">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('common_search')}
            className="w-full pl-8 pr-3 py-1.5 text-sm bg-surface-2 border border-transparent
                       rounded-full outline-none focus:border-primary focus:bg-white transition-colors"
          />
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 pb-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-32 text-text-tertiary text-sm gap-2">
            <Loader2 size={16} className="animate-spin" />
            {t('common_loading')}
          </div>
        ) : diagrams.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-64 text-center">
            <Network size={48} className="text-text-tertiary mb-4" strokeWidth={1.5} />
            <p className="text-text-secondary font-medium">
              {trashed ? t('diagrams_trash_empty') : starred ? t('diagrams_starred_empty') : t('diagrams_empty_title')}
            </p>
            {!trashed && !starred && !recent && (
              <p className="text-text-tertiary text-sm mt-1">
                {t('diagrams_empty_subtitle')}
              </p>
            )}
          </div>
        ) : (
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
            {diagrams.map((d) => (
              <DiagramCard
                key={d.id}
                d={d}
                trashed={!!trashed}
                onOpen={() => navigate(`/office/diagrams/${d.id}`)}
                onStar={() => starMut.mutate({ id: d.id, starred: !d.is_starred })}
                onDuplicate={() => dupMut.mutate(d.id)}
                onTrash={() => trashMut.mutate(d.id)}
                onRestore={() => restoreMut.mutate(d.id)}
                onDelete={() => deleteMut.mutate(d.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
