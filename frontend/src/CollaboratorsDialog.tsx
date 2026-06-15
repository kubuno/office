import { useState, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { Search, Trash2, Users, Check } from 'lucide-react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { FloatingWindow, Dropdown, Spinner } from '@ui'
import type { CollabPermission, Recipient, CollaboratorEntry } from './api'

const PERMS: CollabPermission[] = ['edit', 'comment', 'view']

function initials(name: string): string {
  const p = name.trim().split(/\s+/).filter(Boolean)
  if (p.length === 0) return '?'
  return (p.length === 1 ? p[0].slice(0, 2) : p[0][0] + p[p.length - 1][0]).toUpperCase()
}
function colorFor(id: string): string {
  const palette = ['#1a73e8', '#d93025', '#1e8e3e', '#f9ab00', '#9334e6', '#e8710a', '#12b5cb', '#d01884']
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0
  return palette[h % palette.length]
}
function Avatar({ id, name, url }: { id: string; name: string; url: string | null }) {
  return (
    <div className="w-8 h-8 rounded-full flex items-center justify-center text-xs font-semibold text-white overflow-hidden flex-shrink-0"
      style={{ backgroundColor: colorFor(id) }}>
      {url ? <img src={url} alt={name} className="w-full h-full object-cover" /> : initials(name)}
    </div>
  )
}

// API d'un type d'entité partageable (document, tableur, …). Permet de réutiliser
// ce même dialog pour tous les éditeurs : on injecte juste les fonctions.
export interface CollaboratorsApi {
  listCollaborators: (id: string) => Promise<{ owner: Recipient | null; collaborators: CollaboratorEntry[] }>
  addCollaborator:   (id: string, userId: string, permission: CollabPermission) => Promise<unknown>
  updateCollaborator: (id: string, userId: string, permission: CollabPermission) => Promise<unknown>
  removeCollaborator: (id: string, userId: string) => Promise<unknown>
  searchRecipients:  (q: string) => Promise<Recipient[]>
}

interface Props {
  entityId: string
  api:      CollaboratorsApi
  cacheKey: string          // préfixe de clé react-query (ex. 'doc-collab', 'sheet-collab')
  title?:   string
  onClose:  () => void
}

export default function CollaboratorsDialog({ entityId, api, cacheKey, title, onClose }: Props) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const [search, setSearch]       = useState('')
  const [debounced, setDebounced] = useState('')
  const [perm, setPerm]           = useState<CollabPermission>('edit')
  const [open, setOpen]           = useState(false)
  const boxRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const id = setTimeout(() => setDebounced(search.trim()), 250)
    return () => clearTimeout(id)
  }, [search])

  useEffect(() => {
    if (!open) return
    const h = (e: MouseEvent) => { if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false) }
    document.addEventListener('mousedown', h)
    return () => document.removeEventListener('mousedown', h)
  }, [open])

  const { data, isLoading } = useQuery({
    queryKey: [cacheKey, entityId],
    queryFn: () => api.listCollaborators(entityId),
  })
  const { data: results = [] } = useQuery({
    queryKey: [cacheKey, 'recipients', debounced],
    queryFn: () => api.searchRecipients(debounced),
    enabled: debounced.length > 0,
  })

  const invalidate = () => qc.invalidateQueries({ queryKey: [cacheKey, entityId] })
  const addMut = useMutation({
    mutationFn: (r: Recipient) => api.addCollaborator(entityId, r.id, perm),
    onSuccess: () => { setSearch(''); setDebounced(''); setOpen(false); invalidate() },
  })
  const updateMut = useMutation({
    mutationFn: ({ userId, permission }: { userId: string; permission: CollabPermission }) =>
      api.updateCollaborator(entityId, userId, permission),
    onSuccess: invalidate,
  })
  const removeMut = useMutation({
    mutationFn: (userId: string) => api.removeCollaborator(entityId, userId),
    onSuccess: invalidate,
  })

  const existingIds = new Set([
    data?.owner?.id,
    ...(data?.collaborators ?? []).map(c => c.user_id),
  ].filter(Boolean) as string[])
  const filtered = results.filter(r => !existingIds.has(r.id))

  return (
    <FloatingWindow
      title={<span className="flex items-center gap-2"><Users size={16} /> {title ?? t('share_title', 'Partager le document')}</span>}
      onClose={onClose}
      defaultWidth={480}
      backdrop
    >
      <div className="p-4 space-y-4">
        <div ref={boxRef} className="relative">
          <div className="flex gap-2">
            <div className="flex-1 flex items-center gap-2 px-3 h-10 rounded border border-border focus-within:border-primary">
              <Search size={16} className="text-text-tertiary" />
              <input
                value={search}
                onChange={e => { setSearch(e.target.value); setOpen(true) }}
                onFocus={() => setOpen(true)}
                placeholder={t('share_search_ph', 'Ajouter par nom ou email')}
                className="flex-1 bg-transparent outline-none text-sm"
              />
            </div>
            <Dropdown
              value={perm}
              onChange={v => setPerm(v as CollabPermission)}
              options={PERMS.map(p => ({ value: p, label: t(`share_perm_${p}`, p) }))}
              height={40}
            />
          </div>
          {open && debounced.length > 0 && (
            <div className="absolute z-10 left-0 right-0 mt-1 bg-white border border-border rounded shadow-lg max-h-60 overflow-y-auto">
              {filtered.length === 0
                ? <div className="px-3 py-2 text-sm text-text-tertiary">{t('share_no_results', 'Aucun utilisateur')}</div>
                : filtered.map(r => (
                  <button key={r.id} onClick={() => addMut.mutate(r)} disabled={addMut.isPending}
                    className="w-full flex items-center gap-2 px-3 py-2 hover:bg-surface-2 text-left">
                    <Avatar id={r.id} name={r.display_name || r.email} url={r.avatar_url} />
                    <div className="min-w-0">
                      <div className="text-sm truncate">{r.display_name || r.email}</div>
                      {r.display_name && <div className="text-xs text-text-tertiary truncate">{r.email}</div>}
                    </div>
                  </button>
                ))}
            </div>
          )}
        </div>

        <div>
          <div className="text-xs font-semibold text-text-secondary mb-2">{t('share_people', 'Personnes ayant accès')}</div>
          {isLoading ? (
            <div className="flex justify-center py-4"><Spinner size="sm" /></div>
          ) : (
            <div className="space-y-1">
              {data?.owner && (
                <div className="flex items-center gap-2 py-1.5">
                  <Avatar id={data.owner.id} name={data.owner.display_name || data.owner.email} url={data.owner.avatar_url} />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm truncate">{data.owner.display_name || data.owner.email}</div>
                    <div className="text-xs text-text-tertiary truncate">{data.owner.email}</div>
                  </div>
                  <span className="text-xs text-text-tertiary flex items-center gap-1"><Check size={12} /> {t('share_owner', 'Propriétaire')}</span>
                </div>
              )}
              {(data?.collaborators ?? []).map(c => (
                <div key={c.user_id} className="flex items-center gap-2 py-1.5">
                  <Avatar id={c.user_id} name={c.display_name || c.email} url={c.avatar_url} />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm truncate">{c.display_name || c.email}</div>
                    <div className="text-xs text-text-tertiary truncate">{c.email}</div>
                  </div>
                  <Dropdown
                    value={c.permission}
                    onChange={v => updateMut.mutate({ userId: c.user_id, permission: v as CollabPermission })}
                    options={PERMS.map(p => ({ value: p, label: t(`share_perm_${p}`, p) }))}
                    height={32}
                    fontSize={12}
                  />
                  <button onClick={() => removeMut.mutate(c.user_id)} disabled={removeMut.isPending}
                    title={t('share_remove', 'Retirer')}
                    className="p-1.5 rounded hover:bg-danger-light text-text-tertiary hover:text-danger">
                    <Trash2 size={15} />
                  </button>
                </div>
              ))}
              {(data?.collaborators ?? []).length === 0 && (
                <div className="text-sm text-text-tertiary py-1">{t('share_only_you', 'Vous seul·e avez accès')}</div>
              )}
            </div>
          )}
        </div>
      </div>
    </FloatingWindow>
  )
}
