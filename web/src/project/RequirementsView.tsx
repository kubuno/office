import { useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useConfirm } from '@kubuno/sdk'
import {
  ClipboardList, Plus, Trash2, ChevronRight, ChevronDown, Link2, Unlink,
  AlertTriangle, Package, ListTree, Search,
} from 'lucide-react'
import {
  Button, Input, Textarea, Dropdown, Badge, Callout, Tooltip, EmptyState,
  FloatingWindow, ConfirmDialog, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type Requirement, type RequirementEdit, type RequirementLink,
  type RequirementType, type RequirementPriority, type RequirementStatus,
  type VerificationMethod, type Deliverable, type ProjectTask,
} from '../api'

// Requirements register — what the project has committed to deliver, ranked with
// MoSCoW and traced to whatever actually realises it.
//
// Two rules drive the layout:
//  • MoSCoW must be readable at a glance: it is the arbitration vocabulary, so
//    "must" and "won't" never share a colour.
//  • A requirement WITHOUT a single link is a promise nothing keeps — it is
//    flagged in the register itself, not only in the traceability matrix.
//
// Editing happens in place (the modal is for creation only); the long fields
// live in a per-row detail panel and are committed on blur.

// ── Vocabularies (the server rejects anything outside these lists) ────────────

export const REQ_TYPES: RequirementType[] = [
  'business', 'stakeholder', 'functional', 'non_functional', 'transition', 'quality', 'project',
]
export const REQ_PRIORITIES: RequirementPriority[] = ['must', 'should', 'could', 'wont']
export const REQ_STATUSES: RequirementStatus[] = [
  'proposed', 'approved', 'implemented', 'verified', 'deferred', 'rejected',
]
export const VERIF_METHODS: VerificationMethod[] = ['test', 'inspection', 'demonstration', 'analysis']

type BadgeVariant = 'default' | 'primary' | 'success' | 'warning' | 'danger' | 'neutral'

/** Colour per MoSCoW rank — four clearly different colours, never four greys. */
export const MOSCOW_BADGE: Record<RequirementPriority, BadgeVariant> = {
  must: 'danger', should: 'warning', could: 'primary', wont: 'default',
}
/** Same ranking applied to a form control (the dropdown trigger's own colours). */
const MOSCOW_FG: Record<RequirementPriority, string> = {
  must: 'var(--color-danger)',
  should: 'var(--color-warning)',
  could: 'var(--color-primary)',
  wont: 'var(--color-text-tertiary)',
}

export const STATUS_BADGE: Record<RequirementStatus, BadgeVariant> = {
  proposed: 'default', approved: 'primary', implemented: 'neutral',
  verified: 'success', deferred: 'warning', rejected: 'danger',
}

/** Human labels for every scope vocabulary — shared with the traceability view. */
export function useScopeLabels() {
  const { t } = useTranslation('office')
  return useMemo(() => ({
    type: {
      business: t('req_type_business', { defaultValue: 'Métier' }),
      stakeholder: t('req_type_stakeholder', { defaultValue: 'Partie prenante' }),
      functional: t('req_type_functional', { defaultValue: 'Fonctionnelle' }),
      non_functional: t('req_type_non_functional', { defaultValue: 'Non fonctionnelle' }),
      transition: t('req_type_transition', { defaultValue: 'Transition' }),
      quality: t('req_type_quality', { defaultValue: 'Qualité' }),
      project: t('req_type_project', { defaultValue: 'Projet' }),
    } as Record<RequirementType, string>,
    priority: {
      must: t('req_prio_must', { defaultValue: 'Doit (must)' }),
      should: t('req_prio_should', { defaultValue: 'Devrait (should)' }),
      could: t('req_prio_could', { defaultValue: 'Pourrait (could)' }),
      wont: t('req_prio_wont', { defaultValue: 'Ne sera pas (won’t)' }),
    } as Record<RequirementPriority, string>,
    status: {
      proposed: t('req_status_proposed', { defaultValue: 'Proposée' }),
      approved: t('req_status_approved', { defaultValue: 'Approuvée' }),
      implemented: t('req_status_implemented', { defaultValue: 'Implémentée' }),
      verified: t('req_status_verified', { defaultValue: 'Vérifiée' }),
      deferred: t('req_status_deferred', { defaultValue: 'Reportée' }),
      rejected: t('req_status_rejected', { defaultValue: 'Rejetée' }),
    } as Record<RequirementStatus, string>,
    verification: {
      test: t('req_verif_test', { defaultValue: 'Test' }),
      inspection: t('req_verif_inspection', { defaultValue: 'Inspection' }),
      demonstration: t('req_verif_demonstration', { defaultValue: 'Démonstration' }),
      analysis: t('req_verif_analysis', { defaultValue: 'Analyse' }),
    } as Record<VerificationMethod, string>,
  }), [t])
}

/** The MoSCoW rank as a pill — the same colours the register and the matrix use. */
export function PriorityBadge({ priority, label }: { priority: RequirementPriority; label: string }) {
  return (
    <Badge variant={MOSCOW_BADGE[priority]} dot className={priority === 'wont' ? 'line-through' : undefined}>
      {label}
    </Badge>
  )
}

// ── Small in-place editors ───────────────────────────────────────────────────

/** Single-line field committed to the server on blur (never on each keystroke). */
function InlineText({ value, placeholder, disabled, className, mono, onCommit }: {
  value: string
  placeholder?: string
  disabled?: boolean
  className?: string
  mono?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  if (disabled) {
    return (
      <span className={`text-sm text-text-primary ${mono ? 'font-mono' : ''} ${className ?? ''}`}>
        {value || '—'}
      </span>
    )
  }
  return (
    <Input
      className={className}
      value={draft}
      placeholder={placeholder}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => { if (draft !== value) onCommit(draft) }}
    />
  )
}

/** Long field of the detail panel, committed on blur. */
function LongField({ label, hint, value, rows = 3, disabled, onCommit }: {
  label: string
  hint?: string
  value: string
  rows?: number
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="text-sm text-text-secondary mb-1 block">{label}</label>
      <Textarea
        rows={rows}
        className="h-auto min-h-0 resize-y"
        value={draft}
        disabled={disabled}
        hint={hint}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => { if (draft !== value) onCommit(draft) }}
      />
    </div>
  )
}

// ── Traceability links of one requirement ────────────────────────────────────

/** Encoded dropdown value: a link target is either a deliverable or a task. */
function targetValue(kind: 'd' | 't', id: string) { return `${kind}:${id}` }

function LinksPanel({ projectId, req, canEdit, deliverables, tasks, onError }: {
  projectId: string
  req: Requirement
  canEdit: boolean
  deliverables: Deliverable[]
  tasks: ProjectTask[]
  onError: (err: unknown) => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const [notice, setNotice] = useState<string | null>(null)

  const inval = () => {
    qc.invalidateQueries({ queryKey: ['requirements', projectId] })
    qc.invalidateQueries({ queryKey: ['traceability', projectId] })
  }

  const linkMut = useMutation({
    mutationFn: (data: { deliverable_id?: string; task_id?: string }) =>
      projectsApi.linkRequirement(projectId, req.id, data),
    onSuccess: (r) => {
      // Already linked is not an error — say so plainly and move on.
      setNotice(r.existing
        ? t('req_link_existing', { defaultValue: 'Ce lien existait déjà.' })
        : null)
      inval()
    },
    onError,
  })
  const unlinkMut = useMutation({
    mutationFn: (linkId: string) => projectsApi.unlinkRequirement(projectId, req.id, linkId),
    onSuccess: () => { setNotice(null); inval() },
    onError,
  })

  const linkedDeliverables = new Set(req.links.map(l => l.deliverable_id).filter(Boolean) as string[])
  const linkedTasks = new Set(req.links.map(l => l.task_id).filter(Boolean) as string[])

  const options: DropdownOption[] = [
    ...deliverables
      .filter(d => !linkedDeliverables.has(d.id))
      .map(d => ({
        value: targetValue('d', d.id),
        label: d.code ? `${d.code} · ${d.name}` : d.name,
        icon: <Package size={14} className="text-text-secondary" />,
      })),
    ...tasks
      .filter(tk => !linkedTasks.has(tk.id))
      .map(tk => ({
        value: targetValue('t', tk.id),
        label: tk.wbs ? `${tk.wbs} ${tk.name}` : tk.name,
        icon: <ListTree size={14} className="text-text-secondary" />,
      })),
  ]

  const addTarget = (v: string) => {
    const [kind, id] = [v.slice(0, 1), v.slice(2)]
    if (!id) return
    linkMut.mutate(kind === 'd' ? { deliverable_id: id } : { task_id: id })
  }

  const describe = (l: RequirementLink) => l.deliverable_id
    ? { icon: <Package size={13} />, name: l.deliverable_name ?? t('req_link_deliverable', { defaultValue: 'Livrable' }) }
    : { icon: <ListTree size={13} />, name: l.task_name ?? t('req_link_task', { defaultValue: 'Lot de travail' }) }

  return (
    <div>
      <p className="text-sm text-text-secondary mb-1.5">
        {t('req_links_title', { defaultValue: 'Traçabilité — ce qui réalise cette exigence' })}
      </p>

      {req.links.length === 0 ? (
        <Callout variant="warning" icon={<AlertTriangle size={16} />} className="mb-2">
          {t('req_untraced_hint', { defaultValue: 'Rien ne réalise cette exigence : c’est un engagement pris que personne ne tient. Rattachez-la à un livrable ou à un lot de travail.' })}
        </Callout>
      ) : (
        <ul className="flex flex-wrap gap-1.5 mb-2">
          {req.links.map(l => {
            const d = describe(l)
            return (
              <li key={l.id} className="flex items-center gap-1.5 rounded-full bg-surface-2 pl-2.5 pr-1 py-1 text-sm text-text-primary">
                <span className="text-text-secondary shrink-0">{d.icon}</span>
                <span className="truncate max-w-[16rem]">{d.name}</span>
                {canEdit && (
                  <button
                    onClick={() => unlinkMut.mutate(l.id)}
                    title={t('req_link_remove', { defaultValue: 'Retirer le lien' })}
                    className="p-1 rounded-full text-text-tertiary hover:text-danger hover:bg-surface-3"
                  >
                    <Unlink size={13} />
                  </button>
                )}
              </li>
            )
          })}
        </ul>
      )}

      {canEdit && (
        <div className="flex items-center gap-2 flex-wrap">
          <Link2 size={15} className="text-text-tertiary shrink-0" />
          <Dropdown
            height={36}
            fontSize={14}
            focusable
            width={320}
            value=""
            placeholder={options.length === 0
              ? t('req_link_no_target', { defaultValue: 'Aucune cible disponible' })
              : t('req_link_add', { defaultValue: 'Rattacher à un livrable ou un lot…' })}
            options={options}
            disabled={options.length === 0 || linkMut.isPending}
            onChange={addTarget}
          />
          {notice && <span className="text-sm text-text-tertiary">{notice}</span>}
        </div>
      )}
    </div>
  )
}

// ── One row of the register ──────────────────────────────────────────────────

function RequirementRow({ projectId, req, canEdit, isMobile, expanded, onToggle, deliverables, tasks, onUpdate, onDelete, colCount, onError }: {
  projectId: string
  req: Requirement
  canEdit: boolean
  isMobile: boolean
  expanded: boolean
  onToggle: () => void
  deliverables: Deliverable[]
  tasks: ProjectTask[]
  onUpdate: (patch: RequirementEdit) => void
  onDelete: () => void
  colCount: number
  onError: (err: unknown) => void
}) {
  const { t } = useTranslation('office')
  const labels = useScopeLabels()
  const untraced = req.links.length === 0

  const opts = (values: string[], map: Record<string, string>): DropdownOption[] =>
    values.map(v => ({ value: v, label: map[v] }))

  const cell = 'px-2 py-1.5 align-middle'

  return (
    <>
      <tr className="border-t border-border hover:bg-surface-1">
        {/* Code — also the disclosure control for the detail panel. */}
        <td className={cell}>
          <div className="flex items-center gap-1">
            <button
              onClick={onToggle}
              title={t('req_toggle_detail', { defaultValue: 'Détail de l’exigence' })}
              className="p-1 rounded text-text-tertiary hover:bg-surface-2 shrink-0"
            >
              {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
            </button>
            <InlineText
              value={req.code}
              mono
              className="w-[6.5rem] font-mono"
              disabled={!canEdit}
              placeholder={t('req_code_ph', { defaultValue: 'Sans code' })}
              onCommit={v => onUpdate({ code: v })}
            />
          </div>
        </td>

        {/* Title */}
        <td className={cell}>
          <InlineText
            value={req.title}
            className="w-full min-w-[14rem]"
            disabled={!canEdit}
            onCommit={v => { if (v.trim()) onUpdate({ title: v.trim() }) }}
          />
        </td>

        {!isMobile && (
          <td className={cell}>
            {canEdit ? (
              <Dropdown height={36} fontSize={14} focusable width="100%" value={req.req_type}
                options={opts(REQ_TYPES, labels.type)}
                onChange={v => onUpdate({ req_type: v as RequirementType })} />
            ) : <span className="text-sm text-text-primary">{labels.type[req.req_type]}</span>}
          </td>
        )}

        {/* MoSCoW — coloured so "must" and "won't" never look alike. */}
        <td className={cell}>
          {canEdit ? (
            <Dropdown
              height={36} fontSize={14} focusable width="100%"
              value={req.priority}
              options={opts(REQ_PRIORITIES, labels.priority)}
              buttonStyle={{
                color: MOSCOW_FG[req.priority],
                borderColor: MOSCOW_FG[req.priority],
                fontWeight: 600,
                textDecoration: req.priority === 'wont' ? 'line-through' : undefined,
              }}
              onChange={v => onUpdate({ priority: v as RequirementPriority })}
            />
          ) : <PriorityBadge priority={req.priority} label={labels.priority[req.priority]} />}
        </td>

        {!isMobile && (
          <td className={cell}>
            <InlineText
              value={req.source}
              className="w-full min-w-[8rem]"
              disabled={!canEdit}
              placeholder={t('req_source_ph', { defaultValue: 'Qui l’exige' })}
              onCommit={v => onUpdate({ source: v })}
            />
          </td>
        )}

        <td className={cell}>
          {canEdit ? (
            <Dropdown height={36} fontSize={14} focusable width="100%" value={req.status}
              options={opts(REQ_STATUSES, labels.status)}
              onChange={v => onUpdate({ status: v as RequirementStatus })} />
          ) : <Badge variant={STATUS_BADGE[req.status]}>{labels.status[req.status]}</Badge>}
        </td>

        {!isMobile && (
          <td className={cell}>
            {canEdit ? (
              <Dropdown height={36} fontSize={14} focusable width="100%" value={req.verification_method}
                options={opts(VERIF_METHODS, labels.verification)}
                onChange={v => onUpdate({ verification_method: v as VerificationMethod })} />
            ) : <span className="text-sm text-text-primary">{labels.verification[req.verification_method]}</span>}
          </td>
        )}

        {/* Traceability at a glance: an untraced requirement must stand out. */}
        <td className={cell}>
          <button onClick={onToggle} className="inline-flex">
            {untraced ? (
              <Tooltip label={t('req_untraced_tip', { defaultValue: 'Aucun livrable ni lot de travail ne réalise cette exigence.' })}>
                {/* Wrapped in a native element: `Tooltip` injects mouse handlers by
                    cloning its child, and `Badge` does not forward unknown props. */}
                <span className="inline-flex">
                  <Badge variant="warning" dot>
                    {t('req_untraced', { defaultValue: 'Non tracée' })}
                  </Badge>
                </span>
              </Tooltip>
            ) : (
              <Badge variant="success">
                {t('req_traced_count', { defaultValue: '{{n}} lien(s)', n: req.links.length })}
              </Badge>
            )}
          </button>
        </td>

        <td className={`${cell} text-right`}>
          {canEdit && (
            <button
              onClick={onDelete}
              title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={15} />
            </button>
          )}
        </td>
      </tr>

      {expanded && (
        <tr className="border-t border-border bg-surface-1">
          <td colSpan={colCount} className="p-4">
            <div className="space-y-3">
              {/* On a narrow screen these attributes leave the table; they live here. */}
              {isMobile && (
                <div className="grid grid-cols-1 gap-3">
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('req_col_type', { defaultValue: 'Type' })}</label>
                    <Dropdown height={36} fontSize={14} focusable width="100%" value={req.req_type} disabled={!canEdit}
                      options={opts(REQ_TYPES, labels.type)}
                      onChange={v => onUpdate({ req_type: v as RequirementType })} />
                  </div>
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('req_col_source', { defaultValue: 'Source' })}</label>
                    <InlineText value={req.source} className="w-full" disabled={!canEdit} onCommit={v => onUpdate({ source: v })} />
                  </div>
                  <div>
                    <label className="text-sm text-text-secondary mb-1 block">{t('req_col_verif', { defaultValue: 'Vérification' })}</label>
                    <Dropdown height={36} fontSize={14} focusable width="100%" value={req.verification_method} disabled={!canEdit}
                      options={opts(VERIF_METHODS, labels.verification)}
                      onChange={v => onUpdate({ verification_method: v as VerificationMethod })} />
                  </div>
                </div>
              )}

              <LongField
                label={t('req_description', { defaultValue: 'Énoncé' })}
                hint={t('req_description_hint', { defaultValue: 'Ce que le système ou le projet doit faire, formulé de façon vérifiable.' })}
                value={req.description} rows={3} disabled={!canEdit}
                onCommit={v => onUpdate({ description: v })}
              />
              <LongField
                label={t('req_rationale', { defaultValue: 'Justification' })}
                value={req.rationale} rows={2} disabled={!canEdit}
                onCommit={v => onUpdate({ rationale: v })}
              />
              <LongField
                label={t('req_verification_notes', { defaultValue: 'Notes de vérification' })}
                value={req.verification_notes} rows={2} disabled={!canEdit}
                onCommit={v => onUpdate({ verification_notes: v })}
              />

              <LinksPanel projectId={projectId} req={req} canEdit={canEdit}
                deliverables={deliverables} tasks={tasks} onError={onError} />
            </div>
          </td>
        </tr>
      )}
    </>
  )
}

// ── Creation dialog (a modal is for creation only — edits happen in place) ────

function CreateDialog({ onClose, onCreate, pending }: {
  onClose: () => void
  onCreate: (data: RequirementEdit & { title: string }) => void
  pending: boolean
}) {
  const { t } = useTranslation('office')
  const labels = useScopeLabels()
  const [title, setTitle] = useState('')
  const [code, setCode] = useState('')
  const [source, setSource] = useState('')
  const [reqType, setReqType] = useState<RequirementType>('functional')
  const [priority, setPriority] = useState<RequirementPriority>('should')

  const submit = () => {
    const trimmed = title.trim()
    if (!trimmed) return
    onCreate({
      title: trimmed,
      // An empty code is left out so the server can number it itself.
      ...(code.trim() ? { code: code.trim() } : {}),
      ...(source.trim() ? { source: source.trim() } : {}),
      req_type: reqType,
      priority,
      status: 'proposed',
      verification_method: 'test',
    })
  }

  return (
    <FloatingWindow
      title={t('req_new_title', { defaultValue: 'Nouvelle exigence' })}
      icon={<ClipboardList size={16} />}
      onClose={onClose}
      defaultWidth={520} defaultHeight={400} padding={16} t={t}
      actions={{
        confirm: {
          label: t('common_create', { defaultValue: 'Créer' }),
          onClick: submit,
          disabled: !title.trim(),
          loading: pending,
          autoFocus: false,
        },
        cancel: { label: t('common_cancel', { defaultValue: 'Annuler' }) },
      }}
    >
      <div className="space-y-3">
        <div>
          <label className="text-sm text-text-secondary mb-1 block">
            {t('req_col_title', { defaultValue: 'Intitulé' })}
          </label>
          <Input
            autoFocus
            value={title}
            placeholder={t('req_title_ph', { defaultValue: 'Ce que le projet s’engage à fournir…' })}
            onChange={e => setTitle(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submit() } }}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('req_col_code', { defaultValue: 'Code' })}</label>
            <Input value={code} placeholder={t('req_code_auto', { defaultValue: 'Automatique' })} onChange={e => setCode(e.target.value)} />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('req_col_source', { defaultValue: 'Source' })}</label>
            <Input value={source} placeholder={t('req_source_ph', { defaultValue: 'Qui l’exige' })} onChange={e => setSource(e.target.value)} />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('req_col_type', { defaultValue: 'Type' })}</label>
            <Dropdown height={36} fontSize={14} focusable width="100%" value={reqType}
              options={REQ_TYPES.map(v => ({ value: v, label: labels.type[v] }))}
              onChange={v => setReqType(v as RequirementType)} />
          </div>
          <div>
            <label className="text-sm text-text-secondary mb-1 block">{t('req_col_priority', { defaultValue: 'Priorité (MoSCoW)' })}</label>
            <Dropdown height={36} fontSize={14} focusable width="100%" value={priority}
              buttonStyle={{ color: MOSCOW_FG[priority], borderColor: MOSCOW_FG[priority], fontWeight: 600 }}
              options={REQ_PRIORITIES.map(v => ({ value: v, label: labels.priority[v] }))}
              onChange={v => setPriority(v as RequirementPriority)} />
          </div>
        </div>
        <p className="text-sm text-text-tertiary">
          {t('req_new_hint', { defaultValue: 'L’énoncé détaillé, la justification et les liens de traçabilité se saisissent ensuite dans le registre.' })}
        </p>
      </div>
    </FloatingWindow>
  )
}

// ── The register ─────────────────────────────────────────────────────────────

export default function RequirementsView({ projectId, canEdit = true }: {
  projectId: string
  /** False in the mobile reading mode, where the project is shown, not edited. */
  canEdit?: boolean
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const labels = useScopeLabels()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()

  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [creating, setCreating] = useState(false)
  const [search, setSearch] = useState('')
  const [prioFilter, setPrioFilter] = useState<'all' | RequirementPriority>('all')
  const [untracedOnly, setUntracedOnly] = useState(false)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const { data: requirements, isLoading } = useQuery({
    queryKey: ['requirements', projectId],
    queryFn: () => projectsApi.listRequirements(projectId),
  })
  const { data: deliverables } = useQuery({
    queryKey: ['deliverables', projectId],
    queryFn: () => projectsApi.listDeliverables(projectId),
  })
  // Work packages come from the project itself (it carries its tasks).
  const { data: project } = useQuery({
    queryKey: ['project', projectId],
    queryFn: () => projectsApi.get(projectId),
  })

  const inval = () => {
    qc.invalidateQueries({ queryKey: ['requirements', projectId] })
    qc.invalidateQueries({ queryKey: ['traceability', projectId] })
  }
  // The api client flattens server errors to the root — `response.data` is undefined.
  const fail = (err: unknown) =>
    setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const createMut = useMutation({
    mutationFn: (data: RequirementEdit & { title: string }) => projectsApi.createRequirement(projectId, data),
    onSuccess: () => { setErrorMsg(null); setCreating(false); inval() },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ rid, patch }: { rid: string; patch: RequirementEdit }) =>
      projectsApi.updateRequirement(projectId, rid, patch),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (rid: string) => projectsApi.deleteRequirement(projectId, rid),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })

  const all = useMemo(() => requirements ?? [], [requirements])
  const tasks = useMemo(
    () => (project?.tasks ?? []).slice().sort((a, b) => a.wbs.localeCompare(b.wbs, undefined, { numeric: true })),
    [project],
  )

  const untracedCount = all.filter(r => r.links.length === 0).length
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return all.filter(r => {
      if (prioFilter !== 'all' && r.priority !== prioFilter) return false
      if (untracedOnly && r.links.length > 0) return false
      if (!q) return true
      return r.title.toLowerCase().includes(q)
        || r.code.toLowerCase().includes(q)
        || r.source.toLowerCase().includes(q)
        || r.description.toLowerCase().includes(q)
    })
  }, [all, search, prioFilter, untracedOnly])

  const toggle = (id: string) => setExpanded(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  const askDelete = async (req: Requirement) => {
    const ok = await confirm({
      title: t('req_delete_title', { defaultValue: 'Supprimer cette exigence ?' }),
      message: t('req_delete_msg', {
        defaultValue: '« {{title}} » et ses liens de traçabilité seront supprimés.',
        title: req.title,
      }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) deleteMut.mutate(req.id)
  }

  // Columns actually rendered — the detail panel takes over the rest on mobile.
  const colCount = isMobile ? 5 : 9

  const filtersActive = search.trim() !== '' || prioFilter !== 'all' || untracedOnly
  const clearFilters = () => { setSearch(''); setPrioFilter('all'); setUntracedOnly(false) }

  const th = 'text-left text-sm font-medium text-text-secondary px-2 py-2 whitespace-nowrap'

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <ClipboardList size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('req_title', { defaultValue: 'Exigences' })}
          </h1>
          {all.length > 0 && (
            <span className="text-sm text-text-tertiary">
              {t('req_count_summary', {
                defaultValue: '{{total}} exigence(s) · {{untraced}} non tracée(s)',
                total: all.length, untraced: untracedCount,
              })}
            </span>
          )}
          <div className="flex-1" />
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreating(true)}>
              {t('req_new', { defaultValue: 'Nouvelle exigence' })}
            </Button>
          )}
        </div>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)}>{errorMsg}</Callout>
        )}

        {untracedCount > 0 && (
          <Callout variant="warning" icon={<AlertTriangle size={16} />}
            title={t('req_untraced_banner_title', {
              defaultValue: '{{n}} exigence(s) que rien ne réalise', n: untracedCount,
            })}
            action={untracedOnly
              ? { label: t('req_show_all', { defaultValue: 'Tout afficher' }), onClick: () => setUntracedOnly(false) }
              : { label: t('req_show_untraced', { defaultValue: 'Les afficher' }), onClick: () => setUntracedOnly(true) }}>
            {t('req_untraced_banner', { defaultValue: 'Aucun livrable ni lot de travail ne leur est rattaché : ce sont des engagements sans réalisation prévue.' })}
          </Callout>
        )}

        {all.length > 0 && (
          <div className="flex items-center gap-2 flex-wrap">
            <Input
              className="w-64"
              value={search}
              leftIcon={<Search size={15} />}
              placeholder={t('req_search_ph', { defaultValue: 'Rechercher une exigence…' })}
              onChange={e => setSearch(e.target.value)}
            />
            <Dropdown
              height={36} fontSize={14} focusable width={190}
              value={prioFilter}
              options={[
                { value: 'all', label: t('req_prio_all', { defaultValue: 'Toutes priorités' }) },
                ...REQ_PRIORITIES.map(v => ({ value: v, label: labels.priority[v] })),
              ]}
              onChange={v => setPrioFilter(v as 'all' | RequirementPriority)}
            />
            {filtersActive && (
              <Button size="sm" variant="text" onClick={clearFilters}>
                {t('common_clear_filters', { defaultValue: 'Effacer les filtres' })}
              </Button>
            )}
          </div>
        )}

        {isLoading ? (
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        ) : all.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              icon={<ClipboardList size={26} />}
              variant="first-use"
              title={t('req_empty_title', { defaultValue: 'Aucune exigence' })}
              description={t('req_empty_desc', { defaultValue: 'Le registre des exigences recense ce que le projet s’engage à fournir, le classe par priorité MoSCoW et le relie aux livrables et lots de travail qui le réalisent.' })}
              action={canEdit ? {
                label: t('req_new', { defaultValue: 'Nouvelle exigence' }),
                onClick: () => setCreating(true),
                icon: <Plus size={15} />,
              } : undefined}
              t={t}
            />
          </div>
        ) : filtered.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              icon={<Search size={26} />}
              variant="no-results"
              title={t('req_no_results', { defaultValue: 'Aucune exigence ne correspond' })}
              description={t('req_no_results_desc', { defaultValue: 'Les filtres actifs excluent toutes les exigences du registre.' })}
              action={{ label: t('common_clear_filters', { defaultValue: 'Effacer les filtres' }), onClick: clearFilters }}
              t={t}
            />
          </div>
        ) : (
          // A wide register scrolls INSIDE its own box, never past the page edge.
          <div className="bg-surface-0 border border-border rounded-xl overflow-x-auto">
            <table className="w-full border-collapse" style={{ minWidth: isMobile ? 480 : 1180 }}>
              <thead>
                <tr>
                  <th className={th} style={{ width: 170 }}>{t('req_col_code', { defaultValue: 'Code' })}</th>
                  <th className={th}>{t('req_col_title', { defaultValue: 'Intitulé' })}</th>
                  {!isMobile && <th className={th} style={{ width: 170 }}>{t('req_col_type', { defaultValue: 'Type' })}</th>}
                  <th className={th} style={{ width: 165 }}>{t('req_col_priority', { defaultValue: 'Priorité (MoSCoW)' })}</th>
                  {!isMobile && <th className={th} style={{ width: 160 }}>{t('req_col_source', { defaultValue: 'Source' })}</th>}
                  <th className={th} style={{ width: 160 }}>{t('req_col_status', { defaultValue: 'Statut' })}</th>
                  {!isMobile && <th className={th} style={{ width: 160 }}>{t('req_col_verif', { defaultValue: 'Vérification' })}</th>}
                  <th className={th} style={{ width: 120 }}>{t('req_col_trace', { defaultValue: 'Traçabilité' })}</th>
                  <th className={th} style={{ width: 44 }} />
                </tr>
              </thead>
              <tbody>
                {filtered.map(req => (
                  <RequirementRow
                    key={req.id}
                    projectId={projectId}
                    req={req}
                    canEdit={canEdit}
                    isMobile={isMobile}
                    expanded={expanded.has(req.id)}
                    onToggle={() => toggle(req.id)}
                    deliverables={deliverables ?? []}
                    tasks={tasks}
                    colCount={colCount}
                    onError={fail}
                    onUpdate={patch => updateMut.mutate({ rid: req.id, patch })}
                    onDelete={() => { void askDelete(req) }}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {creating && (
        <CreateDialog
          onClose={() => setCreating(false)}
          onCreate={data => createMut.mutate(data)}
          pending={createMut.isPending}
        />
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
