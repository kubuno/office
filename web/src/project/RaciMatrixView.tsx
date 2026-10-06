import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import {
  Grid3x3, AlertTriangle, CheckCircle2, RefreshCw, UserPlus, Users, Layers, Plus, X, Info,
} from 'lucide-react'
import {
  Badge, Button, Callout, Checkbox, ConfirmDialog, EmptyState, MenuDropdown, ProgressBar, Tooltip,
  useIsMobile, useMenuDropdown, type MenuItem,
} from '@ui'
import { useConfirm } from '@kubuno/sdk'
import { projectsApi, type RaciRole, type RaciRow } from '../api'

// RACI matrix — who does what on each task, and above all what is MISSING.
//
// The matrix is read for its holes: a task nobody answers for (no A) is the
// classic defect it exists to reveal, and a task nobody does (no R) is work
// that will not happen. Both are put at the top, as findings, never as a
// footnote under the grid.
//
// Rollup rows (a task with children) are shown greyed out and refuse new
// assignments: their children already carry the roles, and duplicating them
// there only makes the matrix harder to read.

const ROLES: RaciRole[] = ['R', 'A', 'C', 'I']

/** One colour per role, so a column can be scanned without reading the letters. */
const ROLE_COLOR: Record<RaciRole, string> = {
  R: 'var(--color-primary)',
  A: 'var(--color-warning)',
  C: 'var(--color-success)',
  I: 'var(--color-text-tertiary)',
}

const ROLE_BADGE: Record<RaciRole, 'primary' | 'warning' | 'success' | 'neutral'> = {
  R: 'primary', A: 'warning', C: 'success', I: 'neutral',
}

interface RoleLabels {
  /** Short verb — "Réalise", "Approuve"… */
  short: Record<RaciRole, string>
  /** What the role actually commits its holder to. */
  hint: Record<RaciRole, string>
  /** Verb + commitment, for a tooltip that must stand alone. */
  long: Record<RaciRole, string>
}

function useRoleLabels(t: TFunction): RoleLabels {
  return useMemo(() => {
    const parts = {
      short: {
        R: t('raci_role_r', { defaultValue: 'Réalise' }),
        A: t('raci_role_a', { defaultValue: 'Approuve' }),
        C: t('raci_role_c', { defaultValue: 'Consulté' }),
        I: t('raci_role_i', { defaultValue: 'Informé' }),
      },
      hint: {
        R: t('raci_role_r_desc', { defaultValue: 'fait le travail' }),
        A: t('raci_role_a_desc', { defaultValue: 'en répond, une seule personne par tâche' }),
        C: t('raci_role_c_desc', { defaultValue: 'son avis est demandé avant' }),
        I: t('raci_role_i_desc', { defaultValue: 'on le lui dit après' }),
      },
    }
    const long = Object.fromEntries(
      ROLES.map(r => [r, `${r} · ${parts.short[r]} — ${parts.hint[r]}`]),
    ) as Record<RaciRole, string>
    return { ...parts, long }
  }, [t])
}

/** The letter, as a square glyph — used inside menus where a Badge is too wide. */
function RoleGlyph({ role }: { role: RaciRole }) {
  return (
    <span
      className="inline-flex items-center justify-center rounded-md font-semibold"
      style={{ width: 18, height: 18, fontSize: 11, color: 'white', background: ROLE_COLOR[role] }}
    >
      {role}
    </span>
  )
}

/** The letter as it appears in a cell — explained on hover, never left bare. */
function RoleChip({ role, labels }: { role: RaciRole; labels: RoleLabels }) {
  return (
    // Tooltip clones its child to attach handlers: give it a real element.
    <Tooltip label={labels.long[role]}>
      <span className="inline-flex">
        <Badge variant={ROLE_BADGE[role]} size="sm">{role}</Badge>
      </span>
    </Tooltip>
  )
}

/** The legend — without it, R/A/C/I is jargon on screen. */
function RaciLegend({ labels, t }: { labels: RoleLabels; t: TFunction }) {
  return (
    <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
      <span className="flex items-center gap-1.5 text-sm text-text-secondary">
        <Info size={14} className="shrink-0" />
        {t('raci_legend', { defaultValue: 'Les quatre rôles' })}
      </span>
      {ROLES.map(r => (
        <span key={r} className="flex items-center gap-2 text-sm text-text-secondary">
          <RoleGlyph role={r} />
          <span className="text-text-primary font-medium">{labels.short[r]}</span>
          <span>{labels.hint[r]}</span>
        </span>
      ))}
    </div>
  )
}

/** One of the two findings lists: the tasks that are missing something. */
function GapCard({ title, description, none, items, tone, icon, onOpenTask }: {
  title: string
  description: string
  none: string
  items: Array<{ task_id: string; wbs: string; name: string }>
  tone: 'danger' | 'warning'
  icon: React.ReactNode
  onOpenTask?: (taskId: string) => void
}) {
  const color = tone === 'danger' ? 'var(--color-danger)' : 'var(--color-warning)'
  return (
    <div className="bg-surface-0 border border-border rounded-xl p-5">
      <div className="flex items-center gap-2 mb-1">
        <span className="shrink-0" style={{ color }}>{icon}</span>
        <h2 className="text-sm font-semibold text-text-primary">{title}</h2>
        <Badge variant={items.length > 0 ? tone : 'success'}>{items.length}</Badge>
      </div>
      <p className="text-sm text-text-secondary mb-3">{description}</p>
      {items.length === 0 ? (
        <p className="text-sm text-text-tertiary italic">{none}</p>
      ) : (
        <ul className="space-y-1">
          {items.map(it => (
            <li key={it.task_id}>
              <button
                onClick={() => onOpenTask?.(it.task_id)}
                disabled={!onOpenTask}
                className="w-full text-left flex items-center gap-2 rounded-lg px-2 py-1.5 hover:bg-surface-1 disabled:cursor-default"
              >
                <span className="text-sm text-text-tertiary tabular-nums shrink-0">{it.wbs}</span>
                <span className="text-sm text-text-primary truncate flex-1 min-w-0">{it.name}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Row heading, shared by the table and the mobile cards. */
function TaskHeading({ row, isRollup, missingA, missingR, t }: {
  row: RaciRow
  isRollup: boolean
  missingA: boolean
  missingR: boolean
  t: TFunction
}) {
  return (
    <div className="flex items-center gap-2 min-w-0">
      <span className="text-sm text-text-tertiary tabular-nums shrink-0">{row.wbs}</span>
      {isRollup && <Layers size={13} className="text-text-tertiary shrink-0" />}
      <span className={`text-sm truncate ${isRollup ? 'text-text-secondary font-medium' : 'text-text-primary'}`}>
        {row.name}
      </span>
      {missingA && (
        <Tooltip label={t('raci_row_no_a', { defaultValue: 'Personne ne répond de cette tâche' })}>
          <span className="inline-flex shrink-0" style={{ color: 'var(--color-danger)' }}>
            <AlertTriangle size={14} />
          </span>
        </Tooltip>
      )}
      {missingR && (
        <Tooltip label={t('raci_row_no_r', { defaultValue: 'Personne ne réalise cette tâche' })}>
          <span className="inline-flex shrink-0" style={{ color: 'var(--color-warning)' }}>
            <UserPlus size={14} />
          </span>
        </Tooltip>
      )}
    </div>
  )
}

/** Which menu is open: a matrix cell, or the mobile "assign someone" button. */
type MenuTarget =
  | { kind: 'cell'; taskId: string; holderId: string }
  | { kind: 'add'; taskId: string }

export default function RaciMatrixView({ projectId, canEdit = false, onOpenStakeholders, onOpenTask }: {
  projectId: string
  canEdit?: boolean
  /** Jumps to the stakeholder register — the matrix has no columns without it. */
  onOpenStakeholders?: () => void
  /** Jumps to the task, from the findings lists. */
  onOpenTask?: (taskId: string) => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const labels = useRoleLabels(t)
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()
  const menu = useMenuDropdown()

  const [target, setTarget] = useState<MenuTarget | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [gapsOnly, setGapsOnly] = useState(false)

  const { data, isLoading, isFetching } = useQuery({
    queryKey: ['raci', projectId],
    queryFn: () => projectsApi.getRaci(projectId),
  })

  const inval = () => { qc.invalidateQueries({ queryKey: ['raci', projectId] }) }
  // The api client flattens server errors to the root — `response.data` is undefined.
  // This is how the server's "someone already answers for this task" reaches the screen.
  const fail = (err: unknown) =>
    setErrorMsg((err as { message?: string }).message
      ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const applyMut = useMutation({
    mutationFn: async (v: { taskId: string; holderId: string; role: RaciRole; transferFrom?: string }) => {
      // Only one person is accountable, enforced by a unique index: the previous
      // holder must lose the role before the new one can take it. The order is
      // forced — hence the confirmation asked beforehand.
      if (v.transferFrom) await projectsApi.clearRaciRole(projectId, v.taskId, v.transferFrom)
      await projectsApi.setRaciRole(projectId, v.taskId, v.holderId, v.role)
    },
    onSuccess: () => setErrorMsg(null),
    onError: fail,
    onSettled: inval,
  })

  const clearMut = useMutation({
    mutationFn: (v: { taskId: string; holderId: string }) =>
      projectsApi.clearRaciRole(projectId, v.taskId, v.holderId),
    onSuccess: () => setErrorMsg(null),
    onError: fail,
    onSettled: inval,
  })

  const stakeholders = useMemo(() => data?.stakeholders ?? [], [data])
  const nameOf = useMemo(() => {
    const map = new Map<string, string>()
    for (const s of stakeholders) map.set(s.id, s.name)
    return map
  }, [stakeholders])

  /** A rollup, or a summary line: its children carry the roles. */
  const isRollupRow = (row: RaciRow) => row.is_rollup || row.task_type === 'summary'

  const rows = useMemo(() => {
    const list = data?.tasks ?? []
    if (!gapsOnly) return list
    return list.filter(r => !isRollupRow(r) && (!r.accountable || r.responsible_count === 0))
  }, [data, gapsOnly])

  const roleAt = (row: RaciRow, holderId: string): RaciRole | null =>
    row.cells.find(c => c.stakeholder_id === holderId)?.role ?? null

  const closeMenu = () => { menu.close(); setTarget(null) }

  const openCellMenu = (e: React.MouseEvent, taskId: string, holderId: string) => {
    setTarget({ kind: 'cell', taskId, holderId })
    menu.open(e)
  }

  const openAddMenu = (e: React.MouseEvent, taskId: string) => {
    setTarget({ kind: 'add', taskId })
    menu.open(e)
  }

  /** Assign a role, asking to transfer when someone else already answers. */
  const assign = async (row: RaciRow, holderId: string, role: RaciRole) => {
    closeMenu()
    if (role === 'A' && row.accountable && row.accountable !== holderId) {
      const from = nameOf.get(row.accountable) ?? t('raci_someone', { defaultValue: 'quelqu’un' })
      const to = nameOf.get(holderId) ?? ''
      const ok = await confirm({
        title: t('raci_transfer_title', { defaultValue: 'Transférer le rôle « Approuve » ?' }),
        message: t('raci_transfer_msg', {
          defaultValue: '{{from}} répond déjà de « {{task}} ». Une tâche n’a qu’un seul responsable : le rôle sera retiré à {{from}} et attribué à {{to}}.',
          from, to, task: row.name,
        }),
        confirmLabel: t('raci_transfer_confirm', { defaultValue: 'Transférer' }),
        variant: 'warning',
      })
      if (!ok) return
      applyMut.mutate({ taskId: row.task_id, holderId, role, transferFrom: row.accountable })
      return
    }
    applyMut.mutate({ taskId: row.task_id, holderId, role })
  }

  const clear = (taskId: string, holderId: string) => {
    closeMenu()
    clearMut.mutate({ taskId, holderId })
  }

  /** Items of the menu opened on a cell — the four roles, plus the way out. */
  const cellItems = (row: RaciRow, holderId: string): MenuItem[] => {
    const current = roleAt(row, holderId)
    const rollup = isRollupRow(row)
    const holderName = nameOf.get(holderId) ?? ''
    const items: MenuItem[] = [
      { type: 'label', text: `${row.wbs} · ${holderName}` },
    ]
    if (rollup) {
      items.push({
        type: 'label',
        text: t('raci_rollup_hint', { defaultValue: 'Récapitulatif : les rôles sont portés par ses tâches filles.' }),
      })
    } else {
      for (const r of ROLES) {
        const heldByOther = r === 'A' && !!row.accountable && row.accountable !== holderId
        const label = heldByOther
          ? t('raci_menu_transfer', {
            defaultValue: '{{role}} (transférer depuis {{from}})',
            role: labels.short.A, from: nameOf.get(row.accountable ?? '') ?? '',
          })
          : labels.short[r]
        items.push({
          type: 'action',
          icon: <RoleGlyph role={r} />,
          label,
          shortcut: r,
          checked: current === r,
          onClick: () => { void assign(row, holderId, r) },
        })
      }
    }
    items.push({ type: 'separator' })
    items.push({
      type: 'action',
      icon: <X size={14} />,
      label: t('raci_menu_clear', { defaultValue: 'Retirer le rôle' }),
      danger: true,
      disabled: !current,
      onClick: () => clear(row.task_id, holderId),
    })
    return items
  }

  /** Items of the mobile "assign someone" menu: a person, then a role. */
  const addItems = (row: RaciRow): MenuItem[] => {
    const free = stakeholders.filter(s => !roleAt(row, s.id))
    if (free.length === 0) {
      return [{ type: 'label', text: t('raci_all_assigned', { defaultValue: 'Toutes les parties prenantes ont déjà un rôle.' }) }]
    }
    return free.map(s => ({
      type: 'submenu' as const,
      label: s.name,
      items: ROLES.map(r => ({
        type: 'action' as const,
        icon: <RoleGlyph role={r} />,
        label: labels.short[r],
        shortcut: r,
        onClick: () => { void assign(row, s.id, r) },
      })),
    }))
  }

  const menuItems = (): MenuItem[] => {
    if (!target || !data) return []
    const row = data.tasks.find(r => r.task_id === target.taskId)
    if (!row) return []
    return target.kind === 'cell' ? cellItems(row, target.holderId) : addItems(row)
  }

  if (isLoading || !data) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        </div>
      </div>
    )
  }

  const { gaps } = data
  // The backend leaves rollups and summaries out of the findings: a role on them
  // would duplicate what their children already say.
  const assignable = data.tasks.filter(r => !isRollupRow(r))
  const withAccountable = assignable.length - gaps.no_accountable.length
  const coverage = assignable.length > 0 ? Math.round((withAccountable / assignable.length) * 100) : 0
  const clean = gaps.no_accountable.length === 0 && gaps.no_responsible.length === 0
  const noAccountableIds = new Set(gaps.no_accountable.map(g => g.task_id))
  const noResponsibleIds = new Set(gaps.no_responsible.map(g => g.task_id))

  const th = 'text-left text-sm font-medium text-text-secondary px-3 py-2 whitespace-nowrap'
  const TASK_COL = isMobile ? 220 : 340
  const HOLDER_COL = 116

  const header = (
    <div className="flex items-center gap-3 flex-wrap">
      <Grid3x3 size={20} className="text-text-secondary shrink-0" />
      <h1 className="text-xl font-semibold text-text-primary">
        {t('raci_title', { defaultValue: 'Matrice RACI' })}
      </h1>
      <div className="flex-1" />
      {onOpenStakeholders && (
        <Button size="sm" variant="secondary" icon={<Users size={14} />} onClick={onOpenStakeholders}>
          {t('raci_open_register', { defaultValue: 'Parties prenantes' })}
        </Button>
      )}
      <Button size="sm" variant="secondary" icon={<RefreshCw size={14} />} loading={isFetching}
        onClick={inval}>
        {t('common_refresh', { defaultValue: 'Actualiser' })}
      </Button>
    </div>
  )

  // No column, no matrix: the register comes first.
  if (stakeholders.length === 0) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className={isMobile ? 'p-4 space-y-3' : 'p-6 space-y-4'}>
          {header}
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              icon={<Users size={26} />}
              variant="first-use"
              title={t('raci_empty_title', { defaultValue: 'Aucune partie prenante enregistrée' })}
              description={t('raci_empty_desc', { defaultValue: 'La matrice RACI attribue à chaque tâche qui la réalise (R), qui en répond (A), qui est consulté avant (C) et qui est informé après (I). Elle n’a pas de colonnes tant que personne n’est inscrit au registre des parties prenantes.' })}
              action={onOpenStakeholders ? {
                label: t('raci_empty_action', { defaultValue: 'Ouvrir le registre des parties prenantes' }),
                onClick: onOpenStakeholders,
                icon: <UserPlus size={15} />,
              } : undefined}
              t={t}
            />
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className={isMobile ? 'p-4 space-y-3' : 'p-6 space-y-4'}>
        {header}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)}
            title={t('raci_error_title', { defaultValue: 'Attribution refusée' })} t={t}>
            {errorMsg}
          </Callout>
        )}

        {/* ── Ce que la matrice révèle ── */}
        {clean ? (
          <Callout variant="success" icon={<CheckCircle2 size={16} />}
            title={t('raci_clean_title', { defaultValue: 'Aucun angle mort' })} t={t}>
            {t('raci_clean', { defaultValue: 'Chaque tâche a quelqu’un qui en répond et quelqu’un qui la réalise. Les responsabilités sont posées.' })}
          </Callout>
        ) : (
          <div className={isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-4'}>
            <GapCard
              tone="danger"
              icon={<AlertTriangle size={17} />}
              title={t('raci_gap_a_title', { defaultValue: 'Tâches dont personne ne répond' })}
              description={t('raci_gap_a_desc', { defaultValue: 'Aucun « A » n’est attribué : en cas de problème, personne n’a à rendre de comptes. C’est le défaut que cette matrice sert à révéler.' })}
              none={t('raci_gap_a_none', { defaultValue: 'Aucune — chaque tâche a son responsable.' })}
              items={gaps.no_accountable}
              onOpenTask={onOpenTask}
            />
            <GapCard
              tone="warning"
              icon={<UserPlus size={17} />}
              title={t('raci_gap_r_title', { defaultValue: 'Tâches que personne ne réalise' })}
              description={t('raci_gap_r_desc', { defaultValue: 'Aucun « R » n’est attribué : le travail est planifié, mais personne n’est désigné pour le faire.' })}
              none={t('raci_gap_r_none', { defaultValue: 'Aucune — chaque tâche a quelqu’un pour la faire.' })}
              items={gaps.no_responsible}
              onOpenTask={onOpenTask}
            />
          </div>
        )}

        {/* ── Couverture + légende ── */}
        <div className="bg-surface-0 border border-border rounded-xl p-5 space-y-4">
          <div>
            <div className="flex items-baseline gap-2 mb-1.5 flex-wrap">
              <span className="text-sm text-text-secondary">
                {t('raci_coverage', { defaultValue: 'Tâches dont quelqu’un répond' })}
              </span>
              <span className="text-2xl font-semibold text-text-primary">{coverage}%</span>
              <span className="text-sm text-text-tertiary">
                {t('raci_coverage_detail', {
                  defaultValue: '{{done}} tâche(s) sur {{total}}',
                  done: Math.max(withAccountable, 0), total: assignable.length,
                })}
              </span>
            </div>
            <ProgressBar
              value={Math.max(withAccountable, 0)}
              max={Math.max(assignable.length, 1)}
              variant={coverage === 100 ? 'success' : coverage >= 60 ? 'primary' : coverage >= 30 ? 'warning' : 'danger'}
              t={t}
            />
          </div>
          <RaciLegend labels={labels} t={t} />
        </div>

        {/* ── La matrice ── */}
        <div className="bg-surface-0 border border-border rounded-xl">
          <div className="flex items-center gap-3 px-5 pt-4 pb-3 flex-wrap">
            <h2 className="text-sm font-semibold text-text-primary">
              {t('raci_matrix', { defaultValue: 'Qui fait quoi, tâche par tâche' })}
            </h2>
            <span className="text-sm text-text-tertiary">
              {t('raci_col_count', {
                defaultValue: '{{count}} partie(s) prenante(s)',
                count: stakeholders.length,
              })}
            </span>
            <div className="flex-1" />
            <Checkbox
              checked={gapsOnly}
              onChange={setGapsOnly}
              label={t('raci_filter_gaps', { defaultValue: 'Seulement les tâches à compléter' })}
            />
          </div>

          {rows.length === 0 ? (
            <EmptyState
              icon={<CheckCircle2 size={26} />}
              variant={gapsOnly ? 'no-results' : 'first-use'}
              compact
              title={gapsOnly
                ? t('raci_filter_empty', { defaultValue: 'Aucune tâche à compléter' })
                : t('raci_no_tasks', { defaultValue: 'Aucune tâche dans ce projet' })}
              description={gapsOnly
                ? t('raci_filter_empty_desc', { defaultValue: 'Chaque tâche a déjà quelqu’un qui en répond et quelqu’un qui la réalise.' })
                : t('raci_no_tasks_desc', { defaultValue: 'La matrice se remplit à partir de l’échéancier : ajoutez des tâches pour leur attribuer des rôles.' })}
              action={gapsOnly ? {
                label: t('raci_show_all', { defaultValue: 'Afficher toute la matrice' }),
                onClick: () => setGapsOnly(false),
              } : undefined}
              t={t}
            />
          ) : isMobile ? (
            // A grid one column per person is unreadable on a phone: the task
            // comes first, then who does what on it.
            <div className="px-3 pb-3 space-y-2">
              {rows.map(row => {
                const rollup = isRollupRow(row)
                const assigned = ROLES
                  .map(r => ({ role: r, holders: row.cells.filter(c => c.role === r) }))
                  .filter(g => g.holders.length > 0)
                return (
                  <div key={row.task_id}
                    className={`border border-border rounded-xl p-3 ${rollup ? 'bg-surface-1' : 'bg-surface-0'}`}>
                    <TaskHeading
                      row={row} isRollup={rollup} t={t}
                      missingA={noAccountableIds.has(row.task_id)}
                      missingR={noResponsibleIds.has(row.task_id)}
                    />
                    {rollup ? (
                      <p className="text-sm text-text-tertiary italic mt-2">
                        {t('raci_rollup_hint', { defaultValue: 'Récapitulatif : les rôles sont portés par ses tâches filles.' })}
                      </p>
                    ) : assigned.length === 0 ? (
                      <p className="text-sm text-text-tertiary italic mt-2">
                        {t('raci_row_empty', { defaultValue: 'Aucun rôle attribué.' })}
                      </p>
                    ) : (
                      <ul className="mt-2 space-y-1.5">
                        {assigned.map(g => (
                          <li key={g.role} className="flex items-start gap-2">
                            <span className="shrink-0 mt-0.5"><RoleGlyph role={g.role} /></span>
                            <span className="text-sm text-text-secondary shrink-0 w-20">{labels.short[g.role]}</span>
                            <span className="flex flex-wrap gap-1.5 flex-1 min-w-0">
                              {g.holders.map(c => (
                                <button
                                  key={c.stakeholder_id}
                                  onClick={e => canEdit ? openCellMenu(e, row.task_id, c.stakeholder_id) : undefined}
                                  disabled={!canEdit}
                                  className="rounded-full bg-surface-2 px-2.5 py-1 text-sm text-text-primary disabled:cursor-default"
                                >
                                  {nameOf.get(c.stakeholder_id) ?? ''}
                                </button>
                              ))}
                            </span>
                          </li>
                        ))}
                      </ul>
                    )}
                    {canEdit && !rollup && (
                      <div className="mt-2">
                        <Button size="sm" variant="ghost" icon={<Plus size={14} />}
                          onClick={e => openAddMenu(e, row.task_id)}>
                          {t('raci_assign', { defaultValue: 'Attribuer un rôle' })}
                        </Button>
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          ) : (
            // One column per person: the grid scrolls INSIDE its box, never past
            // the page edge, and the task column stays pinned while it does.
            <div className="overflow-x-auto">
              <table className="border-collapse"
                style={{ minWidth: TASK_COL + stakeholders.length * HOLDER_COL }}>
                <thead>
                  <tr>
                    <th className={`${th} sticky left-0 z-20 bg-surface-0`} style={{ width: TASK_COL, minWidth: TASK_COL }}>
                      {t('raci_col_task', { defaultValue: 'Tâche' })}
                    </th>
                    {stakeholders.map(s => (
                      <th key={s.id} className="px-2 py-2 align-bottom bg-surface-0"
                        style={{ width: HOLDER_COL, minWidth: HOLDER_COL }}>
                        <Tooltip label={s.role_title ? `${s.name} — ${s.role_title}` : s.name}>
                          <div className="min-w-0">
                            <p className="text-sm font-medium text-text-primary truncate">{s.name}</p>
                            {s.role_title && (
                              <p className="text-sm text-text-tertiary truncate">{s.role_title}</p>
                            )}
                          </div>
                        </Tooltip>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.map(row => {
                    const rollup = isRollupRow(row)
                    const rowBg = rollup ? 'bg-surface-1' : 'bg-surface-0'
                    return (
                      <tr key={row.task_id} className="group border-t border-border">
                        <td
                          className={`px-3 py-2 sticky left-0 z-10 ${rowBg} group-hover:bg-surface-1`}
                          style={{ width: TASK_COL, minWidth: TASK_COL }}
                        >
                          <TaskHeading
                            row={row} isRollup={rollup} t={t}
                            missingA={noAccountableIds.has(row.task_id)}
                            missingR={noResponsibleIds.has(row.task_id)}
                          />
                        </td>
                        {stakeholders.map(s => {
                          const current = roleAt(row, s.id)
                          // A rollup takes no new role, but a stale one must stay removable.
                          const clickable = canEdit && (!rollup || !!current)
                          return (
                            <td key={s.id} className={`px-2 py-1.5 text-center ${rollup ? 'bg-surface-1' : ''} group-hover:bg-surface-1`}>
                              <button
                                onClick={e => clickable ? openCellMenu(e, row.task_id, s.id) : undefined}
                                disabled={!clickable}
                                aria-label={`${row.name} — ${s.name}`}
                                className="w-full h-8 inline-flex items-center justify-center rounded-md enabled:hover:bg-surface-2 disabled:cursor-default"
                              >
                                {current
                                  ? <RoleChip role={current} labels={labels} />
                                  : <span className="text-sm text-text-tertiary">·</span>}
                              </button>
                            </td>
                          )
                        })}
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>

      {target && menu.pos && (
        <MenuDropdown items={menuItems()} pos={menu.pos} onClose={closeMenu} minWidth={220} />
      )}
      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}
