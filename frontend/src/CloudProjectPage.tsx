import { useState, useEffect, useMemo, useRef } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { getDateLocale, useModulesStore, WaffleAppRegistry } from '@kubuno/sdk'
import {
  Cloud, LayoutDashboard, Users, Tag, Boxes, Gauge, UserPlus, Trash2, Plus,
  Search, Star, Info, Check, ArrowRight, Loader2, FolderTree,
} from 'lucide-react'
import { Button, Input, Dropdown, Spinner } from '@ui'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_PROJECTS } from './ribbon/officeThemes'
import { useFileTab, backstageLabels, BackstageInfo } from './ribbon/ModuleBackstage'
import { SaveButton } from './ribbon/SaveButton'
import { ProjectsStartContent } from './ProjectsStartContent'
import type { RibbonTab } from './ribbon/types'
import {
  projectsApi, officeApi,
  type Project, type CollabPermission, type CollaboratorEntry, type Recipient,
} from './api'

type View = 'overview' | 'access' | 'labels' | 'resources' | 'usage'

// A cloud project uses the SAME shell + ribbon as the Gantt editor; only its pages
// differ (a container of resources, not a schedule). Pages are switched from the
// ribbon's "Affichage" tab.
export default function CloudProjectPage({ id }: { id: string }) {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [view, setView] = useState<View>('overview')
  const [titleDraft, setTitleDraft] = useState('')

  const { data, isLoading } = useQuery({
    queryKey: ['project', id],
    queryFn: () => projectsApi.get(id),
    enabled: !!id,
  })
  const project = data?.project

  useEffect(() => { if (project) setTitleDraft(project.title) }, [project?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const updateMut = useMutation({
    mutationFn: (patch: Partial<Pick<Project, 'title' | 'is_starred' | 'labels'>>) => projectsApi.update(id, patch),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['project', id] }),
  })
  const trashMut = useMutation({
    mutationFn: () => projectsApi.trash(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['projects'] }); navigate('/office/projects') },
  })

  const { data: access } = useQuery({
    queryKey: ['proj-collab', id],
    queryFn: () => projectsApi.listCollaborators(id),
    enabled: !!id,
  })

  const commitTitle = () => { if (project && titleDraft && titleDraft !== project.title) updateMut.mutate({ title: titleDraft }) }

  const labelCount = project ? Object.keys(project.labels ?? {}).length : 0
  const memberCount = (access?.collaborators.length ?? 0) + (access?.owner ? 1 : 0)

  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_PROJECTS,
    labels: backstageLabels(t),
    startContent: <ProjectsStartContent />,
    defaultTab: 'home',
    openKey: id,
    doc: {
      info: (
        <BackstageInfo
          title={titleDraft}
          onTitleChange={setTitleDraft}
          onTitleCommit={commitTitle}
          extension=""
          subtitle={t('proj_type_cloud', { defaultValue: 'Projet cloud' })}
          general={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('proj_type_cloud', { defaultValue: 'Projet cloud' })],
            [t('proj_field_id', { defaultValue: 'Identifiant' }), project?.slug ?? '—'],
            ...(project?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(project.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
          stats={[
            [t('proj_cloud_labels', { defaultValue: 'Libellés' }), labelCount],
            [t('proj_cloud_members', { defaultValue: 'Membres' }), memberCount],
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: () => navigate('/office/projects'),
    },
  })

  const PAGES: { id: View; Icon: typeof LayoutDashboard; label: string }[] = useMemo(() => [
    { id: 'overview',  Icon: LayoutDashboard, label: t('proj_cloud_overview', { defaultValue: 'Tableau de bord' }) },
    { id: 'access',    Icon: Users,           label: t('proj_cloud_access', { defaultValue: 'Accès' }) },
    { id: 'labels',    Icon: Tag,             label: t('proj_cloud_labels', { defaultValue: 'Libellés' }) },
    { id: 'resources', Icon: Boxes,           label: t('proj_cloud_resources', { defaultValue: 'Ressources' }) },
    { id: 'usage',     Icon: Gauge,           label: t('proj_cloud_usage', { defaultValue: 'Consommation' }) },
  ], [t])

  if (isLoading || !project) {
    return <div className="flex items-center justify-center h-full"><Loader2 size={24} className="animate-spin text-text-tertiary" /></div>
  }

  const ribbon: RibbonTab[] = [
    { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
      { id: 'project', label: t('proj_page_projects', { defaultValue: 'Projet' }), items: [
        { id: 'access', kind: 'button', size: 'large', icon: <UserPlus size={18} />, label: t('proj_share', { defaultValue: 'Gérer l’accès' }), onClick: () => setView('access') },
        { id: 'star', kind: 'toggle', icon: <Star size={18} />, label: t('proj_star', { defaultValue: 'Favori' }), active: project.is_starred, onClick: () => updateMut.mutate({ is_starred: !project.is_starred }) },
      ] },
    ] },
    { id: 'view', label: t('office_tab_view', { defaultValue: 'Affichage' }), groups: [
      { id: 'pages', label: t('proj_cloud_pages', { defaultValue: 'Pages' }), items: PAGES.map(p => ({
        id: p.id, kind: 'toggle', size: 'large', paletteTile: true,
        icon: <p.Icon size={18} />, label: p.label, active: view === p.id, onClick: () => setView(p.id),
      })) },
    ] },
  ]

  return (
    <OfficeShell
      ribbon={[fileTab, ...ribbon]}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_PROJECTS}
      chromeless
      topbarHeight={64}
      onBack={() => navigate('/office/projects')}
      titleIcon={<Cloud size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onTitleChange={setTitleDraft}
      onTitleCommit={commitTitle}
      titlePlaceholder={t('common_untitled')}
      titleActions={
        <SaveButton
          onSave={commitTitle}
          saving={updateMut.isPending}
          label={t('doc_save', { defaultValue: 'Enregistrer' })}
        />
      }
      onDelete={() => trashMut.mutate()}
      deleteTitle={t('proj_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('proj_delete_confirm_title', { defaultValue: 'Supprimer ce projet ?' }),
        message: t('proj_delete_confirm_msg', { defaultValue: 'Le projet sera déplacé dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }), variant: 'danger',
      }}
    >
      <div className="flex-1 overflow-y-auto bg-surface-1" data-module="office">
        {view === 'overview' && <OverviewPage project={project} locale={i18n.language} access={access} labelCount={labelCount} memberCount={memberCount} onGo={setView} t={t} />}
        {view === 'access' && <AccessPage id={id} t={t} />}
        {view === 'labels' && <LabelsPage project={project} saving={updateMut.isPending} onSave={labels => updateMut.mutate({ labels })} t={t} />}
        {view === 'resources' && <ResourcesPage id={id} t={t} />}
        {view === 'usage' && <PlaceholderPage Icon={Gauge} title={t('proj_cloud_usage', { defaultValue: 'Consommation' })} body={t('proj_cloud_usage_soon', { defaultValue: 'La consommation est aujourd’hui suivie par module et par compte, pas encore par projet. Le suivi par projet arrivera lorsque les modules déclareront leur usage à cette granularité.' })} />}
      </div>
    </OfficeShell>
  )
}

type TFn = ReturnType<typeof useTranslation>['t']

// ── Shared bits ───────────────────────────────────────────────────────────────

function PageShell({ title, subtitle, children, actions }: { title: string; subtitle?: string; children: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <div className="max-w-4xl mx-auto p-6">
      <div className="flex items-start justify-between gap-4 mb-5">
        <div>
          <h1 className="text-xl font-semibold text-text-primary">{title}</h1>
          {subtitle && <p className="text-sm text-text-secondary mt-0.5">{subtitle}</p>}
        </div>
        {actions}
      </div>
      {children}
    </div>
  )
}

function Card({ title, Icon, children, onManage, manageLabel }: { title: string; Icon: typeof Info; children: React.ReactNode; onManage?: () => void; manageLabel?: string }) {
  return (
    <div className="bg-surface-0 border border-border rounded-xl p-5">
      <div className="flex items-center gap-2 mb-3">
        <Icon size={16} className="text-text-secondary" />
        <h2 className="text-sm font-medium text-text-primary">{title}</h2>
        {onManage && (
          <button onClick={onManage} className="ml-auto text-xs text-[var(--color-primary)] hover:underline font-medium flex items-center gap-0.5">
            {manageLabel} <ArrowRight size={12} />
          </button>
        )}
      </div>
      {children}
    </div>
  )
}

const AVATAR_PALETTE = ['#1f6fc7', '#d93025', '#1e8e3e', '#e8710a', '#9334e6', '#0b8043', '#c5221f', '#3949ab']
function avatarColor(id: string): string {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0
  return AVATAR_PALETTE[h % AVATAR_PALETTE.length]
}
function initials(name: string): string {
  const p = name.trim().split(/\s+/).filter(Boolean)
  if (!p.length) return '?'
  return (p.length === 1 ? p[0].slice(0, 2) : p[0][0] + p[p.length - 1][0]).toUpperCase()
}
function Avatar({ id, name, url }: { id: string; name: string; url: string | null }) {
  return (
    <div className="w-8 h-8 rounded-full flex items-center justify-center text-xs font-semibold text-white overflow-hidden flex-shrink-0" style={{ backgroundColor: avatarColor(id) }}>
      {url ? <img src={url} alt={name} className="w-full h-full object-cover" /> : initials(name)}
    </div>
  )
}

// ── Overview ──────────────────────────────────────────────────────────────────

function OverviewPage({ project, locale, access, labelCount, memberCount, onGo, t }: {
  project: Project; locale: string; access: { owner: Recipient | null; collaborators: CollaboratorEntry[] } | undefined
  labelCount: number; memberCount: number; onGo: (v: View) => void; t: TFn
}) {
  const infoRows: [string, string][] = [
    [t('proj_field_name', { defaultValue: 'Nom' }), project.title],
    [t('proj_field_id', { defaultValue: 'Identifiant' }), project.slug ?? '—'],
    [t('proj_field_number', { defaultValue: 'Numéro' }), project.id],
    [t('proj_field_created', { defaultValue: 'Créé le' }), new Date(project.created_at).toLocaleDateString(locale)],
  ]
  const labels = Object.entries(project.labels ?? {})
  const { data: mods } = useQuery({ queryKey: ['proj-modules', project.id], queryFn: () => projectsApi.listModules(project.id) })
  return (
    <PageShell title={t('proj_cloud_overview', { defaultValue: 'Tableau de bord' })} subtitle={project.slug ?? undefined}>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card title={t('proj_cloud_info', { defaultValue: 'Infos du projet' })} Icon={Info}>
          <dl className="grid grid-cols-[minmax(0,7rem)_1fr] gap-x-3 gap-y-2 text-sm">
            {infoRows.map(([k, v]) => (
              <div key={k} className="contents">
                <dt className="text-text-tertiary">{k}</dt>
                <dd className="text-text-primary font-mono text-[13px] break-all">{v}</dd>
              </div>
            ))}
          </dl>
        </Card>

        <Card title={t('proj_cloud_access', { defaultValue: 'Accès' })} Icon={Users} onManage={() => onGo('access')} manageLabel={t('proj_share', { defaultValue: 'Gérer' })}>
          <div className="flex items-center gap-2">
            {access?.owner && <Avatar id={access.owner.id} name={access.owner.display_name || access.owner.email} url={access.owner.avatar_url} />}
            {(access?.collaborators ?? []).slice(0, 5).map(c => (
              <Avatar key={c.user_id} id={c.user_id} name={c.display_name || c.email} url={c.avatar_url} />
            ))}
          </div>
          <p className="text-xs text-text-tertiary mt-2">{t('proj_cloud_members_count', { defaultValue: '{{n}} personne(s) ont accès', n: memberCount })}</p>
        </Card>

        <Card title={t('proj_cloud_labels', { defaultValue: 'Libellés' })} Icon={Tag} onManage={() => onGo('labels')} manageLabel={t('common_edit', { defaultValue: 'Modifier' })}>
          {labels.length === 0 ? (
            <p className="text-xs text-text-tertiary">{t('proj_cloud_labels_empty', { defaultValue: 'Aucun libellé.' })}</p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {labels.map(([k, v]) => (
                <span key={k} className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-surface-2 text-xs text-text-secondary">
                  <span className="font-medium text-text-primary">{k}</span>{v && <span>: {v}</span>}
                </span>
              ))}
            </div>
          )}
          <p className="text-xs text-text-tertiary mt-2">{t('proj_cloud_labels_count', { defaultValue: '{{n}} libellé(s)', n: labelCount })}</p>
        </Card>

        <HierarchyCard project={project} t={t} />

        <Card title={t('proj_cloud_resources', { defaultValue: 'Ressources' })} Icon={Boxes} onManage={() => onGo('resources')} manageLabel={t('common_open', { defaultValue: 'Ouvrir' })}>
          {(mods ?? []).length === 0 ? (
            <p className="text-xs text-text-tertiary">{t('proj_cloud_resources_empty_short', { defaultValue: 'Aucun module rattaché.' })}</p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {(mods ?? []).map(m => {
                const { label, Icon } = moduleMeta(m.module_id)
                return (
                  <span key={m.module_id} className="inline-flex items-center gap-1.5 px-2 py-1 rounded-lg bg-surface-2 text-xs text-text-secondary">
                    <Icon size={14} /> {label}
                  </span>
                )
              })}
            </div>
          )}
        </Card>
      </div>
    </PageShell>
  )
}

// ── Hierarchy (parent / subprojects) ──────────────────────────────────────────

function HierarchyCard({ project, t }: { project: Project; t: TFn }) {
  const navigate = useNavigate()
  const qc = useQueryClient()
  const { data } = useQuery({ queryKey: ['projects', 'all'], queryFn: () => projectsApi.list({}) })
  const all = data?.projects ?? []
  const parent = all.find(p => p.id === project.parent_id) ?? null
  const children = all.filter(p => p.parent_id === project.id && !p.is_trashed)
  const candidates = all.filter(p => p.id !== project.id && !p.is_trashed)
  const moveMut = useMutation({
    mutationFn: (pid: string | null) => projectsApi.update(project.id, { parent_id: pid }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['projects'] }); qc.invalidateQueries({ queryKey: ['project', project.id] }) },
  })
  return (
    <Card title={t('proj_cloud_hierarchy', { defaultValue: 'Hiérarchie' })} Icon={FolderTree}>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-xs text-text-tertiary w-14 shrink-0">{t('proj_hier_parent', { defaultValue: 'Parent' })}</span>
        <Dropdown
          height={36} fontSize={14} value={project.parent_id ?? ''}
          onChange={v => moveMut.mutate(v || null)}
          options={[{ value: '', label: t('proj_hier_root', { defaultValue: 'Aucun (racine)' }) }, ...candidates.map(p => ({ value: p.id, label: p.title }))]}
        />
      </div>
      <p className="text-xs text-text-tertiary mb-1">{t('proj_hier_subprojects', { defaultValue: 'Sous-projets' })} ({children.length})</p>
      {children.length === 0 ? (
        <p className="text-xs text-text-tertiary">{t('proj_hier_none', { defaultValue: 'Aucun sous-projet.' })}</p>
      ) : (
        <div className="flex flex-col gap-0.5">
          {children.map(c => (
            <button key={c.id} onClick={() => navigate(`/office/projects/${c.id}`)} className="flex items-center gap-1.5 text-left text-sm text-[var(--color-primary)] hover:underline">
              <FolderTree size={13} className="opacity-60" /> {c.title}
            </button>
          ))}
        </div>
      )}
      {parent && (
        <p className="text-xs text-text-tertiary mt-2">
          {t('proj_hier_under', { defaultValue: 'Sous' })}{' '}
          <button onClick={() => navigate(`/office/projects/${parent.id}`)} className="text-[var(--color-primary)] hover:underline">{parent.title}</button>
        </p>
      )}
    </Card>
  )
}

// ── Access (members + roles) ──────────────────────────────────────────────────

const ROLE_KEYS: Record<CollabPermission, string> = { edit: 'proj_role_edit', comment: 'proj_role_comment', view: 'proj_role_view' }
const ROLE_DEFAULTS: Record<CollabPermission, string> = { edit: 'Éditeur', comment: 'Commentateur', view: 'Lecteur' }

function AccessPage({ id, t }: { id: string; t: TFn }) {
  const qc = useQueryClient()
  const { data, isLoading } = useQuery({ queryKey: ['proj-collab', id], queryFn: () => projectsApi.listCollaborators(id) })
  const inval = () => qc.invalidateQueries({ queryKey: ['proj-collab', id] })

  const addMut = useMutation({ mutationFn: ({ userId, perm }: { userId: string; perm: CollabPermission }) => projectsApi.addCollaborator(id, userId, perm), onSuccess: inval })
  const updateMut = useMutation({ mutationFn: ({ userId, perm }: { userId: string; perm: CollabPermission }) => projectsApi.updateCollaborator(id, userId, perm), onSuccess: inval })
  const removeMut = useMutation({ mutationFn: (userId: string) => projectsApi.removeCollaborator(id, userId), onSuccess: inval })

  const roleOptions = (['edit', 'comment', 'view'] as CollabPermission[]).map(p => ({ value: p, label: t(ROLE_KEYS[p], { defaultValue: ROLE_DEFAULTS[p] }) }))

  return (
    <PageShell title={t('proj_cloud_access', { defaultValue: 'Accès' })} subtitle={t('proj_cloud_access_sub', { defaultValue: 'Qui peut voir et modifier ce projet.' })}>
      <AddPrincipal id={id} roleOptions={roleOptions} onAdd={(userId, perm) => addMut.mutate({ userId, perm })} adding={addMut.isPending} t={t} />

      <div className="mt-5 bg-surface-0 border border-border rounded-xl divide-y divide-border">
        {isLoading ? (
          <div className="p-6 flex justify-center"><Spinner /></div>
        ) : (
          <>
            {data?.owner && (
              <div className="flex items-center gap-3 p-3">
                <Avatar id={data.owner.id} name={data.owner.display_name || data.owner.email} url={data.owner.avatar_url} />
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-text-primary truncate">{data.owner.display_name || data.owner.email}</p>
                  <p className="text-xs text-text-tertiary truncate">{data.owner.email}</p>
                </div>
                <span className="text-xs text-text-secondary px-2.5 py-1 rounded-full bg-surface-2">{t('proj_role_owner', { defaultValue: 'Propriétaire' })}</span>
              </div>
            )}
            {(data?.collaborators ?? []).map(c => (
              <div key={c.user_id} className="flex items-center gap-3 p-3 group">
                <Avatar id={c.user_id} name={c.display_name || c.email} url={c.avatar_url} />
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-text-primary truncate">{c.display_name || c.email}</p>
                  <p className="text-xs text-text-tertiary truncate">{c.email}</p>
                </div>
                <Dropdown height={30} fontSize={13} value={c.permission} onChange={v => updateMut.mutate({ userId: c.user_id, perm: v as CollabPermission })} options={roleOptions} />
                <button onClick={() => removeMut.mutate(c.user_id)} className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger" title={t('proj_access_remove', { defaultValue: 'Retirer l’accès' })}>
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
            {(data?.collaborators.length ?? 0) === 0 && (
              <p className="p-4 text-xs text-text-tertiary text-center">{t('proj_access_none', { defaultValue: 'Aucun autre membre pour l’instant.' })}</p>
            )}
          </>
        )}
      </div>
    </PageShell>
  )
}

function AddPrincipal({ id, roleOptions, onAdd, adding, t }: {
  id: string; roleOptions: { value: string; label: string }[]; onAdd: (userId: string, perm: CollabPermission) => void; adding: boolean; t: TFn
}) {
  const [q, setQ] = useState('')
  const [debounced, setDebounced] = useState('')
  const [perm, setPerm] = useState<CollabPermission>('edit')
  const [open, setOpen] = useState(false)
  const boxRef = useRef<HTMLDivElement>(null)

  useEffect(() => { const h = setTimeout(() => setDebounced(q.trim()), 250); return () => clearTimeout(h) }, [q])
  useEffect(() => {
    const onDoc = (e: MouseEvent) => { if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false) }
    document.addEventListener('mousedown', onDoc); return () => document.removeEventListener('mousedown', onDoc)
  }, [])

  const { data: results, isFetching } = useQuery({
    queryKey: ['proj-recipients', id, debounced],
    queryFn: () => officeApi.searchRecipients(debounced),
    enabled: debounced.length >= 1,
  })

  return (
    <div className="flex items-center gap-2">
      <div ref={boxRef} className="relative flex-1">
        <Input
          value={q}
          onChange={e => { setQ(e.target.value); setOpen(true) }}
          onFocus={() => setOpen(true)}
          leftIcon={<Search size={14} />}
          placeholder={t('proj_access_add_placeholder', { defaultValue: 'Ajouter une personne (nom ou e-mail)…' })}
        />
        {open && debounced.length >= 1 && (
          <div className="absolute z-20 mt-1 w-full bg-surface-0 border border-border rounded-lg shadow-lg py-1 max-h-64 overflow-y-auto">
            {isFetching && <div className="px-3 py-2 text-xs text-text-tertiary flex items-center gap-2"><Spinner /> {t('common_loading', { defaultValue: 'Chargement…' })}</div>}
            {!isFetching && (results ?? []).length === 0 && <div className="px-3 py-2 text-xs text-text-tertiary">{t('proj_access_no_match', { defaultValue: 'Aucun résultat.' })}</div>}
            {(results ?? []).map(r => (
              <button key={r.id} onClick={() => { onAdd(r.id, perm); setQ(''); setOpen(false) }} disabled={adding}
                className="w-full flex items-center gap-2 px-3 py-2 hover:bg-surface-1 text-left disabled:opacity-50">
                <Avatar id={r.id} name={r.display_name || r.email} url={r.avatar_url} />
                <div className="min-w-0">
                  <p className="text-sm text-text-primary truncate">{r.display_name || r.email}</p>
                  <p className="text-xs text-text-tertiary truncate">{r.email}</p>
                </div>
                <Plus size={14} className="ml-auto text-text-tertiary" />
              </button>
            ))}
          </div>
        )}
      </div>
      <Dropdown height={36} fontSize={14} value={perm} onChange={v => setPerm(v as CollabPermission)} options={roleOptions} />
    </div>
  )
}

// ── Labels ────────────────────────────────────────────────────────────────────

function LabelsPage({ project, saving, onSave, t }: { project: Project; saving: boolean; onSave: (l: Record<string, string>) => void; t: TFn }) {
  const [rows, setRows] = useState<[string, string][]>(Object.entries(project.labels ?? {}))
  const [dirty, setDirty] = useState(false)
  const setRow = (i: number, k: string, v: string) => { setRows(r => r.map((row, j) => (j === i ? [k, v] : row))); setDirty(true) }
  const addRow = () => { setRows(r => [...r, ['', '']]); setDirty(true) }
  const delRow = (i: number) => { setRows(r => r.filter((_, j) => j !== i)); setDirty(true) }
  const save = () => {
    const obj: Record<string, string> = {}
    for (const [k, v] of rows) { const key = k.trim(); if (key) obj[key] = v }
    onSave(obj); setDirty(false)
  }
  return (
    <PageShell
      title={t('proj_cloud_labels', { defaultValue: 'Libellés' })}
      subtitle={t('proj_cloud_labels_sub', { defaultValue: 'Des paires clé/valeur pour organiser et filtrer vos projets.' })}
      actions={dirty ? <Button size="sm" icon={<Check size={14} />} onClick={save} loading={saving}>{t('common_save', { defaultValue: 'Enregistrer' })}</Button> : undefined}
    >
      <div className="bg-surface-0 border border-border rounded-xl p-5 flex flex-col gap-2 max-w-xl">
        {rows.length === 0 && <p className="text-xs text-text-tertiary">{t('proj_cloud_labels_empty', { defaultValue: 'Aucun libellé.' })}</p>}
        {rows.map(([k, v], i) => (
          <div key={i} className="group flex items-center gap-2">
            <Input value={k} onChange={e => setRow(i, e.target.value, v)} placeholder={t('proj_label_key', { defaultValue: 'clé' })} />
            <Input value={v} onChange={e => setRow(i, k, e.target.value)} placeholder={t('proj_label_value', { defaultValue: 'valeur' })} />
            <button onClick={() => delRow(i)} className="p-1 rounded hover:bg-surface-2 text-text-tertiary opacity-0 group-hover:opacity-100"><Trash2 size={14} /></button>
          </div>
        ))}
        <div className="pt-1">
          <Button size="sm" variant="ghost" icon={<Plus size={14} />} onClick={addRow}>{t('proj_label_add', { defaultValue: 'Ajouter un libellé' })}</Button>
        </div>
      </div>
    </PageShell>
  )
}

// ── Resources (attached Kubuno modules) ───────────────────────────────────────

// A module's display identity for the picker. WaffleAppRegistry (shared via the
// SDK) is where every module registers its label + icon component; fall back to
// the bare id + a generic icon when a module hasn't registered (e.g. headless).
function moduleMeta(moduleId: string): { label: string; Icon: React.ComponentType<{ size?: number; className?: string }> } {
  const entry = WaffleAppRegistry.get(moduleId)
  const root = entry?.apps.find(a => a.id === moduleId) ?? entry?.apps[0]
  return { label: entry?.label ?? moduleId, Icon: root?.Icon ?? Boxes }
}

function ResourcesPage({ id, t }: { id: string; t: TFn }) {
  const qc = useQueryClient()
  const activeModules = useModulesStore(s => s.activeModules)
  const [picking, setPicking] = useState(false)

  const { data: attached, isLoading } = useQuery({ queryKey: ['proj-modules', id], queryFn: () => projectsApi.listModules(id) })
  const inval = () => qc.invalidateQueries({ queryKey: ['proj-modules', id] })
  const attachMut = useMutation({ mutationFn: (mid: string) => projectsApi.attachModule(id, mid), onSuccess: inval })
  const detachMut = useMutation({ mutationFn: (mid: string) => projectsApi.detachModule(id, mid), onSuccess: inval })

  const attachedIds = new Set((attached ?? []).map(m => m.module_id))
  const available = activeModules.filter(m => !attachedIds.has(m.module_id))

  return (
    <PageShell
      title={t('proj_cloud_resources', { defaultValue: 'Ressources' })}
      subtitle={t('proj_cloud_resources_sub', { defaultValue: 'Les modules Kubuno rattachés à ce projet.' })}
      actions={<Button size="sm" icon={<Plus size={14} />} onClick={() => setPicking(v => !v)}>{t('proj_res_add', { defaultValue: 'Ajouter une ressource' })}</Button>}
    >
      {picking && (
        <div className="mb-4 bg-surface-0 border border-border rounded-xl p-4">
          <p className="text-xs text-text-secondary mb-3">{t('proj_res_pick', { defaultValue: 'Choisissez un module à rattacher.' })}</p>
          {available.length === 0 ? (
            <p className="text-xs text-text-tertiary">{t('proj_res_none_left', { defaultValue: 'Tous les modules installés sont déjà rattachés.' })}</p>
          ) : (
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
              {available.map(m => {
                const { label, Icon } = moduleMeta(m.module_id)
                return (
                  <button key={m.module_id} disabled={attachMut.isPending}
                    onClick={() => { attachMut.mutate(m.module_id); }}
                    className="flex items-center gap-2 p-2.5 rounded-lg border border-border bg-surface-1 hover:bg-surface-2 hover:border-border-strong text-left disabled:opacity-50">
                    <span aria-hidden className="w-8 h-8 rounded-lg bg-surface-0 border border-border flex items-center justify-center shrink-0"><Icon size={18} /></span>
                    <span className="text-sm text-text-primary truncate">{label}</span>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      )}

      <div className="bg-surface-0 border border-border rounded-xl divide-y divide-border">
        {isLoading ? (
          <div className="p-6 flex justify-center"><Spinner /></div>
        ) : (attached ?? []).length === 0 ? (
          <div className="p-10 flex flex-col items-center text-center gap-3">
            <div className="w-12 h-12 rounded-xl bg-surface-2 flex items-center justify-center"><Boxes size={22} className="text-text-tertiary" /></div>
            <p className="text-sm text-text-secondary max-w-md">{t('proj_cloud_resources_empty', { defaultValue: 'Aucune ressource attachée. Rattachez des modules Kubuno pour les regrouper sous ce projet.' })}</p>
          </div>
        ) : (
          (attached ?? []).map(m => {
            const { label, Icon } = moduleMeta(m.module_id)
            return (
              <div key={m.module_id} className="flex items-center gap-3 p-3 group">
                <span aria-hidden className="w-9 h-9 rounded-lg bg-surface-1 border border-border flex items-center justify-center shrink-0"><Icon size={20} /></span>
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-text-primary truncate">{label}</p>
                  <p className="text-xs text-text-tertiary font-mono truncate">{m.module_id}</p>
                </div>
                <button onClick={() => detachMut.mutate(m.module_id)} className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger" title={t('proj_res_detach', { defaultValue: 'Détacher' })}>
                  <Trash2 size={15} />
                </button>
              </div>
            )
          })
        )}
      </div>
    </PageShell>
  )
}

// ── Placeholder pages (usage) ─────────────────────────────────────────────────

function PlaceholderPage({ Icon, title, body }: { Icon: typeof Info; title: string; body: string }) {
  return (
    <PageShell title={title}>
      <div className="bg-surface-0 border border-border rounded-xl p-10 flex flex-col items-center text-center gap-3">
        <div className="w-12 h-12 rounded-xl bg-surface-2 flex items-center justify-center"><Icon size={22} className="text-text-tertiary" /></div>
        <p className="text-sm text-text-secondary max-w-md">{body}</p>
      </div>
    </PageShell>
  )
}
