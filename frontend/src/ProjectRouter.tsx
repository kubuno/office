import { lazy, Suspense } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { FolderX } from 'lucide-react'
import { EmptyState } from '@ui'
import { projectsApi } from './api'
import CloudProjectPage from './CloudProjectPage'

// A project id can address either a management (Gantt) project or a cloud project;
// they use different editors. This router reads the project's `kind` (from the
// shared react-query cache the target pages reuse) and renders the right one.
const ProjectEditorPage = lazy(() => import('./ProjectEditorPage'))

export default function ProjectRouter() {
  const { id } = useParams<{ id: string }>()
  const { t } = useTranslation('office')
  const navigate = useNavigate()
  const { data, isLoading, isError } = useQuery({
    queryKey: ['project', id],
    queryFn: () => projectsApi.get(id!),
    enabled: !!id,
    // A missing/forbidden project answers 404 — retrying can't change that, and
    // without this the view would sit on "Chargement…" forever.
    retry: false,
  })

  if (isLoading) {
    return <div className="p-8 text-sm text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</div>
  }
  // Not found OR no access (the API doesn't distinguish, to avoid leaking existence).
  if (isError || !data) {
    return (
      <div className="p-8">
        <EmptyState
          variant="unavailable"
          icon={<FolderX size={26} />}
          title={t('proj_not_found_title', { defaultValue: 'Projet introuvable' })}
          description={t('proj_not_found_desc', { defaultValue: "Ce projet n'existe pas, ou vous n'y avez pas accès. Demandez à son propriétaire de vous le partager." })}
          action={{ label: t('proj_back_to_list', { defaultValue: 'Voir mes projets' }), onClick: () => navigate('/office/projects') }}
        />
      </div>
    )
  }
  if (data.project.kind === 'cloud') {
    return <CloudProjectPage id={id!} />
  }
  return (
    <Suspense fallback={<div className="p-8 text-sm text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</div>}>
      <ProjectEditorPage />
    </Suspense>
  )
}
