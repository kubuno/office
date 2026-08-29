import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfirm } from '@kubuno/sdk'
import {
  Input, Button, Dropdown, MenuDropdown, useMenuDropdown, Tooltip, ConfirmDialog,
  type MenuItem, type DropdownOption,
} from '@ui'
import {
  UserRound, Trash2, Plus, X, Search, Layers, UsersRound,
} from 'lucide-react'
import { projectsApi, type ProjectResource, type ResourceKind, type OrgMember } from '../../api'
import type { ResourceViewProps } from './types'
import { KIND_META, CATEGORY_GROUPS, behaviorOf, peakLoadByResource, taskCountByResource } from './types'
import { KIND_ICON } from './kindIcons'

// ── Constants ───────────────────────────────────────────────────────────────

/** One shared grid template so the header and every row keep their columns aligned:
 *  type · name · role · skills · capacity · rate · tasks · utilisation · delete. */
const GRID_COLS = '40px minmax(180px,1.6fr) minmax(120px,1fr) minmax(170px,1.4fr) 96px 128px 72px 160px 44px'
const MIN_WIDTH = 980

/** Utilisation band colours (fixed tints — a red overload must read the same
 *  everywhere, so these never come from the theme). */
const LOAD_OK = '#188038'   // < 80 %
const LOAD_WARN = '#e37400' // 80–100 %
const LOAD_OVER = '#d93025' // > 100 % (overallocation)

/** A stable avatar colour for a directory member added as a resource. */
function memberColor(userId: string): string {
  const palette = ['#1a73e8', '#188038', '#e37400', '#9334e6', '#d93025', '#00897b', '#c2185b', '#5f6368']
  let h = 0
  for (const ch of userId) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return palette[h % palette.length]
}

function loadColor(pct: number): string {
  if (pct > 100) return LOAD_OVER
  if (pct >= 80) return LOAD_WARN
  return LOAD_OK
}

// ── Small inline editors ──────────────────────────────────────────────────────

/** Borderless text field that commits on blur / Enter (one PATCH per edit).
 *  Reads like plain text until focused; mirrors the server echo when it moves. */
function GhostText({ value, onCommit, canEdit, placeholder, className = '' }: {
  value: string
  onCommit: (next: string) => void
  canEdit: boolean
  placeholder?: string
  className?: string
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])
  if (!canEdit) {
    return <span className={`truncate ${value ? 'text-text-primary' : 'text-text-tertiary'} ${className}`}>{value || placeholder || '—'}</span>
  }
  return (
    <input
      className={`w-full min-w-0 bg-transparent outline-none rounded px-1 -mx-1 py-0.5 text-text-primary
                  placeholder:text-text-tertiary focus:bg-surface-0 focus:ring-1 focus:ring-primary ${className}`}
      value={draft}
      placeholder={placeholder}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => { const v = draft.trim(); if (v !== value) onCommit(v) }}
      onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur(); if (e.key === 'Escape') { setDraft(value); e.currentTarget.blur() } }}
    />
  )
}

/** Borderless number field that commits on blur. `empty` maps an empty field to a
 *  distinct value (null for the rate — "unpriced" ≠ "free"). */
function GhostNumber({ value, onCommit, canEdit, suffix, min = 0, step = 1, align = 'right' }: {
  value: number | null
  onCommit: (next: number | null) => void
  canEdit: boolean
  suffix?: string
  min?: number
  step?: number
  align?: 'left' | 'right'
}) {
  const [draft, setDraft] = useState(value == null ? '' : String(value))
  useEffect(() => { setDraft(value == null ? '' : String(value)) }, [value])
  return (
    <span className={`inline-flex items-center gap-0.5 w-full ${align === 'right' ? 'justify-end' : ''}`}>
      <input
        type="number" min={min} step={step}
        readOnly={!canEdit}
        className={`w-full min-w-0 bg-transparent outline-none tabular-nums rounded px-1 -mx-1 py-0.5 text-text-primary
                    [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none
                    ${align === 'right' ? 'text-right' : ''} ${canEdit ? 'focus:bg-surface-0 focus:ring-1 focus:ring-primary' : ''}`}
        value={draft}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => {
          const n = parseFloat(draft)
          const next = Number.isFinite(n) ? n : null
          if (next !== value) onCommit(next)
        }}
        onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur() }}
      />
      {suffix && <span className="shrink-0 text-[11px] text-text-tertiary">{suffix}</span>}
    </span>
  )
}

/** Skill chips + a compact "add" field. Skills are replaced wholesale on commit. */
function SkillsCell({ skills, onChange, canEdit }: {
  skills: string[]
  onChange: (next: string[]) => void
  canEdit: boolean
}) {
  const { t } = useTranslation('office')
  const [draft, setDraft] = useState('')
  const add = () => {
    const v = draft.trim()
    if (!v || skills.includes(v)) { setDraft(''); return }
    onChange([...skills, v])
    setDraft('')
  }
  return (
    <div className="flex flex-wrap items-center gap-1">
      {skills.map(s => (
        <span key={s} className="inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded-full bg-surface-2 text-text-secondary text-[11px] max-w-full">
          <span className="truncate">{s}</span>
          {canEdit && (
            <button
              onClick={() => onChange(skills.filter(x => x !== s))}
              className="shrink-0 text-text-tertiary hover:text-danger rounded-full"
              title={t('proj_res_skill_remove', { defaultValue: 'Retirer' })}
            >
              <X size={11} />
            </button>
          )}
        </span>
      ))}
      {canEdit && (
        <span className="inline-flex items-center rounded-full border border-dashed border-border px-1.5 py-0.5">
          <Plus size={11} className="text-text-tertiary shrink-0" />
          <input
            className="w-16 bg-transparent outline-none text-[11px] text-text-primary placeholder:text-text-tertiary"
            value={draft}
            placeholder={t('proj_res_skill_add', { defaultValue: 'compétence' })}
            onChange={e => setDraft(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') add(); if (e.key === ',') { e.preventDefault(); add() } }}
            onBlur={add}
          />
        </span>
      )}
    </div>
  )
}

// ── Row ────────────────────────────────────────────────────────────────────────

function ResourceRow({ r, taskCount, loadPct, canEdit, onPatch, onDelete }: {
  r: ProjectResource
  taskCount: number
  loadPct: number | null
  canEdit: boolean
  onPatch: (data: Partial<ProjectResource>) => void
  onDelete: () => void
}) {
  const { t } = useTranslation('office')
  const [editingName, setEditingName] = useState(false)
  const [nameDraft, setNameDraft] = useState(r.name)
  useEffect(() => { setNameDraft(r.name) }, [r.name])

  const meta = KIND_META[r.kind]
  const Icon = KIND_ICON[r.kind]
  const isPerson = KIND_META[r.kind]?.category === 'human'
  const isMaterial = behaviorOf(r.kind) === 'material'
  const hasCapacity = behaviorOf(r.kind) === 'work' // capacity only for time-based (work) resources
  const rateSuffix = isMaterial ? `/${r.unit_label || t('proj_res_unit', { defaultValue: 'unité' })}` : '/h'

  return (
    <div
      className="grid items-center gap-x-2 px-3 h-12 border-b border-border text-xs hover:bg-surface-1 group"
      style={{ gridTemplateColumns: GRID_COLS }}
    >
      {/* 1 · Type */}
      <div className="flex justify-center">
        <Tooltip label={t(`proj_res_kind_${r.kind}`, { defaultValue: meta.label })}>
          <span
            className="w-6 h-6 rounded-md flex items-center justify-center"
            style={{ background: `color-mix(in srgb, ${meta.tint} 14%, transparent)`, color: meta.tint }}
          >
            <Icon size={14} />
          </span>
        </Tooltip>
      </div>

      {/* 2 · Name (avatar + editable + member/external badge) */}
      <div className="flex items-center gap-2 min-w-0">
        {r.avatar_url
          ? <img src={r.avatar_url} alt="" className="w-7 h-7 rounded-full object-cover shrink-0" />
          : <span className="w-7 h-7 rounded-full flex items-center justify-center text-white text-[11px] font-semibold shrink-0" style={{ background: r.color }}>{r.name[0]?.toUpperCase() || '?'}</span>}
        <div className="min-w-0 flex items-center gap-1.5">
          {editingName && canEdit ? (
            <input
              autoFocus
              className="min-w-0 bg-transparent border-b border-primary outline-none text-xs font-medium text-text-primary"
              value={nameDraft}
              onChange={e => setNameDraft(e.target.value)}
              onBlur={() => { setEditingName(false); const v = nameDraft.trim(); if (v && v !== r.name) onPatch({ name: v }); else setNameDraft(r.name) }}
              onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur(); if (e.key === 'Escape') { setNameDraft(r.name); setEditingName(false) } }}
            />
          ) : (
            <span
              className={`truncate font-medium text-text-primary ${canEdit ? 'cursor-text' : ''}`}
              onDoubleClick={() => canEdit && setEditingName(true)}
              title={r.name}
            >
              {r.name}
            </span>
          )}
          {isPerson && (
            <span className={`shrink-0 px-1.5 py-px rounded text-[10px] font-medium ${r.user_id ? 'bg-primary/10 text-primary' : 'bg-surface-2 text-text-secondary'}`}>
              {r.user_id ? t('proj_res_member', { defaultValue: 'Membre' }) : t('proj_res_external', { defaultValue: 'Externe' })}
            </span>
          )}
        </div>
      </div>

      {/* 3 · Role */}
      <div className="min-w-0">
        <GhostText value={r.role} canEdit={canEdit} placeholder={t('proj_res_role_ph', { defaultValue: 'Rôle…' })}
          onCommit={v => onPatch({ role: v })} />
      </div>

      {/* 4 · Skills */}
      <div className="min-w-0">
        <SkillsCell skills={r.skills} canEdit={canEdit} onChange={next => onPatch({ skills: next })} />
      </div>

      {/* 5 · Capacity (percentage) */}
      <div className="min-w-0">
        {hasCapacity
          ? <GhostNumber value={Math.round(r.capacity * 100)} canEdit={canEdit} suffix="%" min={0} step={5}
              onCommit={v => onPatch({ capacity: (v ?? 0) / 100 })} />
          : <span className="block text-right text-text-tertiary">—</span>}
      </div>

      {/* 6 · Rate */}
      <div className="min-w-0">
        <GhostNumber value={r.hourly_rate} canEdit={canEdit} suffix={rateSuffix} min={0} step={1}
          onCommit={v => onPatch({ hourly_rate: v })} />
      </div>

      {/* 7 · Tasks */}
      <div className="text-center tabular-nums text-text-secondary">{taskCount}</div>

      {/* 8 · Utilisation */}
      <div className="min-w-0">
        {loadPct == null ? (
          <span className="block text-text-tertiary">—</span>
        ) : (
          <div className="flex items-center gap-2">
            <div className="flex-1 h-2 rounded-full bg-surface-2 overflow-hidden">
              <div className="h-full rounded-full" style={{ width: `${Math.min(100, loadPct)}%`, background: loadColor(loadPct) }} />
            </div>
            <span className="shrink-0 tabular-nums text-[11px] font-medium" style={{ color: loadColor(loadPct) }}>{Math.round(loadPct)}%</span>
          </div>
        )}
      </div>

      {/* 9 · Delete */}
      <div className="flex justify-center">
        {canEdit && (
          <button
            onClick={onDelete}
            className="text-text-tertiary hover:text-danger p-1 rounded opacity-0 group-hover:opacity-100 focus:opacity-100"
            title={t('common_delete', { defaultValue: 'Supprimer' })}
          >
            <Trash2 size={14} />
          </button>
        )}
      </div>
    </div>
  )
}

// ── Registry ────────────────────────────────────────────────────────────────

type GroupMode = 'none' | 'type' | 'role' | 'skill'

export default function ResourceRegistry(props: ResourceViewProps) {
  const { projectId, resources, assignments, canEdit, onRefresh } = props
  const { t } = useTranslation('office')
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const addMenu = useMenuDropdown()

  const [members, setMembers] = useState<OrgMember[]>([])
  const [query, setQuery] = useState('')
  const [groupBy, setGroupBy] = useState<GroupMode>('none')

  // Load the directory members of the user's unit once (candidates for "person" rows).
  useEffect(() => {
    let alive = true
    projectsApi.orgMembers(projectId).then(m => { if (alive) setMembers(m) }).catch(() => {})
    return () => { alive = false }
  }, [projectId])

  const taskCounts = useMemo(() => taskCountByResource(assignments), [assignments])
  const peaks = useMemo(() => peakLoadByResource(props), [props])

  // Peak utilisation as a percentage of capacity — the shared measure the heatmap uses.
  const loadOf = (r: ProjectResource): number | null => {
    if (behaviorOf(r.kind) !== 'work') return null
    const cap = r.capacity > 0 ? r.capacity : 1
    const peak = peaks.get(r.id) ?? 0
    return (peak / cap) * 100
  }

  // ── Mutations (every one reloads the project bundle) ──
  const addResource = async (data: Parameters<typeof projectsApi.createResource>[1]) => {
    try { await projectsApi.createResource(projectId, data); onRefresh() } catch { /* surfaced by the bundle refetch */ }
  }
  const patchResource = async (id: string, data: Partial<ProjectResource>) => {
    try { await projectsApi.updateResource(projectId, id, data); onRefresh() } catch { /* ignore */ }
  }
  const removeResource = async (r: ProjectResource) => {
    const ok = await confirm({
      title:        t('proj_res_delete_title', { defaultValue: 'Supprimer la ressource' }),
      message:      t('proj_res_delete_msg', { defaultValue: `Retirer « ${r.name} » du projet ? Ses affectations aux tâches seront supprimées.` }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant:      'danger',
    })
    if (!ok) return
    try { await projectsApi.deleteResource(projectId, r.id); onRefresh() } catch { /* ignore */ }
  }

  // ── Add menu ──
  const memberItems: MenuItem[] = members.length > 0
    ? members.map(m => ({
        type: 'action' as const,
        label: m.display_name,
        shortcut: m.email,
        icon: m.avatar_url
          ? <img src={m.avatar_url} alt="" className="w-4 h-4 rounded-full object-cover" />
          : <UsersRound size={14} />,
        onClick: () => void addResource({ name: m.display_name, user_id: m.id, kind: 'person', color: memberColor(m.id) }),
      }))
    : [{ type: 'action' as const, label: t('proj_no_org_members', { defaultValue: 'Aucun autre membre dans votre unité' }), disabled: true, onClick: () => {} }]

  const kindItem = (kind: string): MenuItem => {
    const meta = KIND_META[kind]
    const Icon = KIND_ICON[kind] ?? UserRound
    const label = t(`proj_res_kind_${kind}`, { defaultValue: meta.label })
    return {
      type: 'action',
      label,
      icon: <Icon size={14} style={{ color: meta.tint }} />,
      onClick: () => void addResource({
        name: label,
        kind: kind as ResourceKind,
        unit_label: meta.behavior === 'material' ? t('proj_res_unit', { defaultValue: 'unité' }) : undefined,
      }),
    }
  }
  const addItems: MenuItem[] = [
    { type: 'submenu', label: t('proj_add_member_short', { defaultValue: 'Membre de mon unité' }), icon: <UsersRound size={14} />, items: memberItems },
    { type: 'separator' },
    ...CATEGORY_GROUPS.map(g => ({ type: 'submenu' as const, label: g.label, items: g.kinds.map(kindItem) })),
  ]

  // ── Filter + group ──
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return resources
    return resources.filter(r =>
      r.name.toLowerCase().includes(q) ||
      r.role.toLowerCase().includes(q) ||
      r.skills.some(s => s.toLowerCase().includes(q)))
  }, [resources, query])

  const groups = useMemo(() => {
    const sortByName = (a: ProjectResource, b: ProjectResource) => a.name.localeCompare(b.name)
    if (groupBy === 'none') return [{ key: '', label: '', rows: [...filtered].sort(sortByName) }]

    const map = new Map<string, { label: string; rows: ProjectResource[] }>()
    const push = (key: string, label: string, r: ProjectResource) => {
      let g = map.get(key)
      if (!g) { g = { label, rows: [] }; map.set(key, g) }
      g.rows.push(r)
    }
    for (const r of filtered) {
      if (groupBy === 'type') {
        push(r.kind, t(`proj_res_kind_${r.kind}`, { defaultValue: KIND_META[r.kind].label }), r)
      } else if (groupBy === 'role') {
        const role = r.role.trim()
        push(role || '∅role', role || t('proj_res_no_role', { defaultValue: 'Sans rôle' }), r)
      } else {
        // A resource with several skills shows up under each of them.
        if (r.skills.length === 0) push('∅skill', t('proj_res_no_skill', { defaultValue: 'Sans compétence' }), r)
        else for (const s of r.skills) push(s, s, r)
      }
    }
    return [...map.entries()]
      .map(([key, g]) => ({ key, label: g.label, rows: g.rows.sort(sortByName) }))
      .sort((a, b) => a.label.localeCompare(b.label))
  }, [filtered, groupBy, t])

  const groupOptions: DropdownOption[] = [
    { value: 'none', label: t('proj_res_group_none', { defaultValue: 'Aucun' }) },
    { value: 'type', label: t('proj_res_group_type', { defaultValue: 'Type' }) },
    { value: 'role', label: t('proj_res_group_role', { defaultValue: 'Rôle' }) },
    { value: 'skill', label: t('proj_res_group_skill', { defaultValue: 'Compétence' }) },
  ]

  const headerCell = 'text-[11px] font-semibold uppercase tracking-wide text-text-tertiary'

  return (
    <div className="h-full w-full flex flex-col bg-surface-0">
      {/* ── Toolbar ── */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0 flex-wrap">
        {canEdit && (
          <>
            <Button
              icon={<Plus size={15} />}
              onClick={e => addMenu.openAt(e.currentTarget.getBoundingClientRect().left, e.currentTarget.getBoundingClientRect().bottom + 4)}
            >
              {t('proj_res_add', { defaultValue: 'Ajouter' })}
            </Button>
            {addMenu.pos && <MenuDropdown items={addItems} pos={addMenu.pos} onClose={addMenu.close} minWidth={220} />}
          </>
        )}

        <div className="flex items-center gap-1.5">
          <Layers size={14} className="text-text-tertiary" />
          <span className="text-xs text-text-secondary">{t('proj_res_group_by', { defaultValue: 'Grouper par' })}</span>
          <Dropdown height={32} fontSize={13} value={groupBy} onChange={v => setGroupBy(v as GroupMode)} options={groupOptions} />
        </div>

        <div className="flex-1 min-w-[160px] max-w-xs">
          <Input
            type="text"
            leftIcon={<Search size={14} />}
            placeholder={t('proj_res_search_ph', { defaultValue: 'Rechercher (nom, rôle, compétence)…' })}
            value={query}
            onChange={e => setQuery(e.target.value)}
          />
        </div>

        <span className="ml-auto text-xs text-text-tertiary tabular-nums">
          {t('proj_res_count', { count: filtered.length, defaultValue: `${filtered.length} ressource(s)` })}
        </span>
      </div>

      {/* ── Table ── */}
      <div className="flex-1 overflow-auto">
        <div style={{ minWidth: MIN_WIDTH }}>
          {/* Column header */}
          <div className="grid items-center gap-x-2 px-3 h-9 border-b border-border bg-surface-1 sticky top-0 z-10" style={{ gridTemplateColumns: GRID_COLS }}>
            <div className={`${headerCell} text-center`}>{t('proj_res_col_type', { defaultValue: 'Type' })}</div>
            <div className={headerCell}>{t('proj_res_col_name', { defaultValue: 'Nom' })}</div>
            <div className={headerCell}>{t('proj_res_col_role', { defaultValue: 'Rôle' })}</div>
            <div className={headerCell}>{t('proj_res_col_skills', { defaultValue: 'Compétences' })}</div>
            <div className={`${headerCell} text-right`}>{t('proj_res_col_capacity', { defaultValue: 'Capacité' })}</div>
            <div className={`${headerCell} text-right`}>{t('proj_res_col_rate', { defaultValue: 'Taux' })}</div>
            <div className={`${headerCell} text-center`}>{t('proj_res_col_tasks', { defaultValue: 'Tâches' })}</div>
            <div className={headerCell}>{t('proj_res_col_load', { defaultValue: 'Utilisation' })}</div>
            <div />
          </div>

          {/* Rows */}
          {filtered.length === 0 ? (
            <div className="text-center py-16 px-4">
              <UsersRound size={30} className="mx-auto text-text-tertiary opacity-40 mb-2" />
              <p className="text-sm text-text-secondary">
                {resources.length === 0
                  ? t('proj_res_empty', { defaultValue: 'Aucune ressource pour l’instant' })
                  : t('proj_res_no_match', { defaultValue: 'Aucune ressource ne correspond à la recherche' })}
              </p>
              {resources.length === 0 && canEdit && (
                <p className="text-xs text-text-tertiary mt-1">{t('proj_res_empty_hint', { defaultValue: 'Ajoutez des membres, équipements ou coûts, puis affectez-les aux tâches.' })}</p>
              )}
            </div>
          ) : (
            groups.map(g => (
              <div key={g.key || 'all'}>
                {groupBy !== 'none' && (
                  <div className="flex items-center gap-2 px-3 h-8 bg-surface-1/70 border-b border-border sticky top-9 z-[9]">
                    <span className="text-xs font-semibold text-text-primary truncate">{g.label}</span>
                    <span className="text-[11px] text-text-tertiary tabular-nums">{g.rows.length}</span>
                  </div>
                )}
                {g.rows.map(r => (
                  <ResourceRow
                    key={groupBy === 'skill' ? `${g.key}:${r.id}` : r.id}
                    r={r}
                    taskCount={taskCounts.get(r.id) ?? 0}
                    loadPct={loadOf(r)}
                    canEdit={canEdit}
                    onPatch={data => void patchResource(r.id, data)}
                    onDelete={() => void removeResource(r)}
                  />
                ))}
              </div>
            ))
          )}
        </div>
      </div>

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
