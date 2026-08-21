import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { ChevronRight, ChevronDown, Cloud, FolderKanban, Loader2, Search } from 'lucide-react'
import { Input } from '@ui'
import { projectsApi, type Project } from './api'

// Portfolio view: every project as an expandable tree (OpenProject-style project
// list). A project is the "folder" of its children (parent_id); roots are projects
// with no parent (or whose parent is filtered out). Clicking a row opens it.
const STATUS: Record<string, { key: string; def: string; cls: string }> = {
  active:    { key: 'proj_pstatus_active',    def: 'Actif',    cls: 'text-primary' },
  on_hold:   { key: 'proj_pstatus_on_hold',   def: 'En pause', cls: 'text-warning' },
  completed: { key: 'proj_pstatus_completed', def: 'Terminé',  cls: 'text-success' },
  cancelled: { key: 'proj_pstatus_cancelled', def: 'Annulé',   cls: 'text-danger'  },
}

export function ProjectTree() {
  const { t, i18n } = useTranslation('office')
  const navigate = useNavigate()
  const [search, setSearch] = useState('')
  const { data, isLoading } = useQuery({ queryKey: ['projects', 'all'], queryFn: () => projectsApi.list({}) })
  const all = useMemo(() => (data?.projects ?? []).filter(p => !p.is_trashed), [data])
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())

  const { childMap, roots } = useMemo(() => {
    const ids = new Set(all.map(p => p.id))
    const childMap = new Map<string, Project[]>()
    const roots: Project[] = []
    for (const p of all) {
      const parent = p.parent_id && ids.has(p.parent_id) ? p.parent_id : null
      if (parent) { const arr = childMap.get(parent) ?? []; arr.push(p); childMap.set(parent, arr) }
      else roots.push(p)
    }
    const byName = (a: Project, b: Project) => a.title.localeCompare(b.title)
    roots.sort(byName); childMap.forEach(v => v.sort(byName))
    return { childMap, roots }
  }, [all])

  // Flatten to visible rows (respecting collapse). A search shows a flat filtered list.
  const q = search.trim().toLowerCase()
  const rows: { p: Project; depth: number; hasKids: boolean }[] = []
  if (q) {
    for (const p of all) if (p.title.toLowerCase().includes(q) || (p.slug ?? '').includes(q)) rows.push({ p, depth: 0, hasKids: false })
  } else {
    const walk = (list: Project[], depth: number) => {
      for (const p of list) {
        const kids = childMap.get(p.id) ?? []
        rows.push({ p, depth, hasKids: kids.length > 0 })
        if (kids.length && !collapsed.has(p.id)) walk(kids, depth + 1)
      }
    }
    walk(roots, 0)
  }

  const toggle = (id: string) => setCollapsed(s => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n })

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-border shrink-0">
        <h2 className="text-sm font-semibold text-text-primary">{t('proj_portfolio', { defaultValue: 'Portefeuille' })}</h2>
        <span className="text-xs text-text-tertiary">{all.length}</span>
        <div className="flex-1" />
        <Input value={search} onChange={e => setSearch(e.target.value)} placeholder={t('common_search', { defaultValue: 'Rechercher' })}
          leftIcon={<Search size={14} />} className="!rounded-full bg-surface-1 w-44" />
      </div>

      {/* Column header */}
      <div className="flex items-center gap-2 px-4 py-1.5 border-b border-border text-[11px] font-medium text-text-tertiary uppercase tracking-wide shrink-0">
        <span className="flex-1">{t('proj_col_name', { defaultValue: 'Nom' })}</span>
        <span className="w-24">{t('office_bs_info_type', { defaultValue: 'Type' })}</span>
        <span className="w-24">{t('proj_col_status', { defaultValue: 'Statut' })}</span>
        <span className="w-28 text-right">{t('office_bs_info_modified', { defaultValue: 'Modifié le' })}</span>
      </div>

      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="flex justify-center py-10"><Loader2 size={18} className="animate-spin text-text-tertiary" /></div>
        ) : rows.length === 0 ? (
          <p className="text-sm text-text-tertiary text-center py-10">{search ? t('proj_no_results', { defaultValue: 'Aucun résultat.' }) : t('proj_empty_yet', { defaultValue: 'Aucun projet.' })}</p>
        ) : rows.map(({ p, depth, hasKids }) => {
          const st = STATUS[p.status]
          const TypeIcon = p.kind === 'cloud' ? Cloud : FolderKanban
          return (
            <div key={p.id} onClick={() => navigate(`/office/projects/${p.id}`)}
              className="flex items-center gap-2 px-4 py-1.5 border-b border-[#f1f3f4] hover:bg-surface-1 cursor-pointer text-sm">
              <div className="flex-1 flex items-center min-w-0" style={{ paddingLeft: depth * 18 }}>
                {hasKids ? (
                  <button onClick={e => { e.stopPropagation(); toggle(p.id) }} className="p-0.5 text-text-tertiary hover:text-text-primary shrink-0">
                    {collapsed.has(p.id) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
                  </button>
                ) : <span className="w-[19px] shrink-0" />}
                <TypeIcon size={14} className="text-text-tertiary shrink-0 mr-1.5" style={{ color: p.color }} />
                <span className="truncate text-text-primary">{p.title}</span>
                {p.slug && <code className="ml-2 text-[11px] font-mono text-text-tertiary truncate">{p.slug}</code>}
              </div>
              <span className="w-24 text-xs text-text-secondary">{p.kind === 'cloud' ? t('proj_type_cloud', { defaultValue: 'Projet cloud' }) : t('proj_type_mgmt', { defaultValue: 'Gestion' })}</span>
              <span className={`w-24 text-xs ${st?.cls ?? 'text-text-secondary'}`}>{st ? t(st.key, { defaultValue: st.def }) : p.status}</span>
              <span className="w-28 text-right text-xs text-text-tertiary">{format(new Date(p.updated_at), 'd MMM yy', { locale: getDateLocale(i18n.language) })}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
