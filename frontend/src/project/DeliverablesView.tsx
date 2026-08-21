import { useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import { getDateLocale, prompt, useConfirm } from '@kubuno/sdk'
import { PackageCheck, Plus, Trash2, CheckCircle2, XCircle, Undo2 } from 'lucide-react'
import {
  Button, Input, Dropdown, DataTable, Badge, Callout, EmptyState, ProgressBar,
  FloatingWindow, ConfirmDialog,
  type DataTableColumn, type DropdownOption,
} from '@ui'
import { projectsApi, type Deliverable, type DeliverableEdit, type DeliverableStatus } from '../api'

// Deliverables — what the project actually hands over, followed through to
// acceptance.
//
// The point of this screen is the STATUS column: a deliverable nobody accepted
// is not done, whatever the schedule says. Acceptance and rejection are the
// owner's acts (the server answers 403 to anyone else) and go through their own
// endpoints, which record who accepted and why something was sent back — so they
// are never offered as just another value in the status dropdown.

/** Statuses an editor may set directly; the last two are reached only by the
 *  owner's accept/reject acts, which is why they are absent here. */
const EDITABLE_STATUSES: DeliverableStatus[] = ['planned', 'in_progress', 'delivered']

const STATUS_VARIANT: Record<DeliverableStatus, 'neutral' | 'success' | 'danger'> = {
  planned:     'neutral',
  in_progress: 'neutral',
  delivered:   'neutral',
  accepted:    'success',
  rejected:    'danger',
}

const isTerminal = (s: DeliverableStatus) => s === 'accepted' || s === 'rejected'

/** Cell field committed on blur — one PATCH per edit, not one per keystroke. */
function CellInput({ value, placeholder, disabled, required, type, className, onCommit }: {
  value: string
  placeholder?: string
  disabled?: boolean
  /** Refuse to commit an empty value and snap back (the server answers 422). */
  required?: boolean
  type?: string
  className?: string
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <Input
      type={type}
      className={className}
      value={draft}
      placeholder={placeholder}
      disabled={disabled}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        if (draft === value) return
        if (required && !draft.trim()) { setDraft(value); return }
        onCommit(draft)
      }}
    />
  )
}

export default function DeliverablesView({ projectId, isOwner = false, canEdit = true }: {
  projectId: string
  /** Accepting and rejecting are the owner's calls — the server enforces it. */
  isOwner?: boolean
  /** False in the mobile reading mode, where deliverables are shown, not edited. */
  canEdit?: boolean
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const [draft, setDraft] = useState<{ name: string; code: string; due_date: string; task_id: string }>(
    { name: '', code: '', due_date: '', task_id: '' })

  const { data: deliverables, isLoading, isError, refetch } = useQuery({
    queryKey: ['deliverables', projectId],
    queryFn: () => projectsApi.listDeliverables(projectId),
  })
  // The breakdown supplies the work packages a deliverable can be attached to;
  // the query key is shared with the WBS screen, so both read one cache entry.
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['deliverables', projectId] })
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const done = () => { setErrorMsg(null); inval() }

  const createMut = useMutation({
    mutationFn: (data: DeliverableEdit & { name: string }) => projectsApi.createDeliverable(projectId, data),
    onSuccess: () => { done(); setCreateOpen(false); setDraft({ name: '', code: '', due_date: '', task_id: '' }) },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: DeliverableEdit }) => projectsApi.updateDeliverable(projectId, id, data),
    onSuccess: done,
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteDeliverable(projectId, id),
    onSuccess: done,
    onError: fail,
  })
  const acceptMut = useMutation({
    mutationFn: (id: string) => projectsApi.acceptDeliverable(projectId, id),
    onSuccess: done,
    onError: fail,
  })
  const rejectMut = useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) => projectsApi.rejectDeliverable(projectId, id, reason),
    onSuccess: done,
    onError: fail,
  })

  const rows = useMemo(() => deliverables ?? [], [deliverables])
  const total = rows.length
  const accepted = rows.filter(d => d.status === 'accepted').length
  const rejected = rows.filter(d => d.status === 'rejected').length

  const statusLabel = (s: DeliverableStatus) => ({
    planned:     t('proj_deliv_status_planned', { defaultValue: 'Prévu' }),
    in_progress: t('proj_deliv_status_in_progress', { defaultValue: 'En cours' }),
    delivered:   t('proj_deliv_status_delivered', { defaultValue: 'Livré' }),
    accepted:    t('proj_deliv_status_accepted', { defaultValue: 'Accepté' }),
    rejected:    t('proj_deliv_status_rejected', { defaultValue: 'Refusé' }),
  })[s]

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_deliv_no_task', { defaultValue: '— Aucun lot —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])

  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })
  const patch = (id: string, data: DeliverableEdit) => updateMut.mutate({ id, data })

  const askAccept = async (d: Deliverable) => {
    const ok = await confirm({
      title: t('proj_deliv_accept_title', { defaultValue: 'Accepter le livrable ?' }),
      message: t('proj_deliv_accept_msg', {
        defaultValue: '« {{name}} » sera marqué accepté, à votre nom et à la date du jour. C’est la seule chose qui vaut réception.',
        name: d.name,
      }),
      confirmLabel: t('proj_deliv_accept', { defaultValue: 'Accepter' }),
    })
    if (ok) acceptMut.mutate(d.id)
  }

  const askReject = async (d: Deliverable) => {
    // The reason is mandatory server-side (422 when empty): `allowEmpty` stays
    // off so the dialog refuses to validate rather than the request failing.
    const reason = await prompt({
      title: t('proj_deliv_reject_title', { defaultValue: 'Refuser le livrable' }),
      message: t('proj_deliv_reject_msg', {
        defaultValue: 'Pourquoi « {{name}} » est-il renvoyé ? Un refus sans motif n’apprend rien à l’équipe.',
        name: d.name,
      }),
      placeholder: t('proj_deliv_reject_ph', { defaultValue: 'Motif du refus…' }),
      confirmLabel: t('proj_deliv_reject', { defaultValue: 'Refuser' }),
      multiline: true,
    })
    if (reason === null || !reason.trim()) return
    rejectMut.mutate({ id: d.id, reason: reason.trim() })
  }

  const askDelete = async (d: Deliverable) => {
    const ok = await confirm({
      title: t('proj_deliv_delete_title', { defaultValue: 'Supprimer le livrable ?' }),
      message: t('proj_deliv_delete_msg', { defaultValue: '« {{name}} » sera supprimé définitivement.', name: d.name }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) deleteMut.mutate(d.id)
  }

  const columns: DataTableColumn<Deliverable>[] = [
    {
      id: 'code',
      header: t('proj_deliv_code', { defaultValue: 'Code' }),
      headerText: t('proj_deliv_code', { defaultValue: 'Code' }),
      width: 120,
      sortValue: d => d.code,
      cell: d => canEdit
        ? <CellInput value={d.code} placeholder="—" className="w-full font-mono"
            onCommit={v => patch(d.id, { code: v })} />
        : <span className="font-mono text-xs text-text-tertiary">{d.code || '—'}</span>,
    },
    {
      id: 'name',
      header: t('proj_deliv_name', { defaultValue: 'Livrable' }),
      headerText: t('proj_deliv_name', { defaultValue: 'Livrable' }),
      minWidth: 200,
      primary: true,
      required: true,
      sortValue: d => d.name,
      cell: d => canEdit
        ? <CellInput value={d.name} required onCommit={v => patch(d.id, { name: v.trim() })} />
        : <span className="text-sm text-text-primary">{d.name}</span>,
    },
    {
      id: 'task',
      header: t('proj_deliv_task', { defaultValue: 'Lot de travail' }),
      headerText: t('proj_deliv_task', { defaultValue: 'Lot de travail' }),
      width: 220,
      sortValue: d => d.task_name,
      cell: d => canEdit
        ? <Dropdown className="w-full" height={36} fontSize={14} focusable
            value={d.task_id ?? ''} options={taskOptions}
            onChange={v => patch(d.id, { task_id: v || null })} />
        : <span className="text-sm text-text-secondary">{d.task_name ?? '—'}</span>,
    },
    {
      id: 'due',
      header: t('proj_deliv_due', { defaultValue: 'Échéance' }),
      headerText: t('proj_deliv_due', { defaultValue: 'Échéance' }),
      width: 160,
      sortValue: d => d.due_date,
      cell: d => canEdit
        ? <Input type="date" value={d.due_date ?? ''}
            onChange={e => patch(d.id, { due_date: e.target.value || null })} />
        : <span className="text-sm text-text-secondary">{d.due_date ? fmtDate(d.due_date) : '—'}</span>,
    },
    {
      id: 'status',
      header: t('proj_deliv_status', { defaultValue: 'Statut' }),
      headerText: t('proj_deliv_status', { defaultValue: 'Statut' }),
      width: 230,
      sortValue: d => d.status,
      cell: d => (
        <div className="flex flex-col gap-1 py-1">
          {isTerminal(d.status) || !canEdit ? (
            <Badge variant={STATUS_VARIANT[d.status]} dot>{statusLabel(d.status)}</Badge>
          ) : (
            <Dropdown className="w-full" height={36} fontSize={14} focusable
              value={d.status}
              options={EDITABLE_STATUSES.map(s => ({ value: s, label: statusLabel(s) }))}
              onChange={v => patch(d.id, { status: v as DeliverableStatus })} />
          )}
          {/* What the verdict actually was: a rejection is only useful with its
              reason, an acceptance only with its date. */}
          {d.status === 'rejected' && d.rejection_reason && (
            <p className="text-[11px] text-danger whitespace-pre-wrap break-words">{d.rejection_reason}</p>
          )}
          {d.status === 'accepted' && d.accepted_at && (
            <p className="text-[11px] text-text-tertiary">
              {/* Who signed off matters as much as when: acceptance is an act, not
                  a timestamp. The name falls back to the date alone for rows
                  accepted before the server started resolving it. */}
              {d.accepted_by_name
                ? t('proj_deliv_accepted_by', { defaultValue: 'Accepté par {{name}} le {{date}}', name: d.accepted_by_name, date: fmtDate(d.accepted_at) })
                : t('proj_deliv_accepted_on', { defaultValue: 'Accepté le {{date}}', date: fmtDate(d.accepted_at) })}
            </p>
          )}
        </div>
      ),
    },
    {
      id: 'verdict',
      header: t('proj_deliv_verdict', { defaultValue: 'Réception' }),
      headerText: t('proj_deliv_verdict', { defaultValue: 'Réception' }),
      width: 190,
      align: 'right',
      cell: d => (
        <div className="flex items-center gap-1 justify-end">
          {/* Owner only: the server answers 403 to anyone else, so offering the
              buttons at all would only produce failures. */}
          {isOwner && d.status !== 'accepted' && (
            <Button size="sm" variant="secondary" icon={<CheckCircle2 size={14} />}
              onClick={() => { void askAccept(d) }}>
              {t('proj_deliv_accept', { defaultValue: 'Accepter' })}
            </Button>
          )}
          {isOwner && d.status !== 'rejected' && (
            <Button size="sm" variant="text" icon={<XCircle size={14} />}
              title={t('proj_deliv_reject', { defaultValue: 'Refuser' })}
              onClick={() => { void askReject(d) }}>
              {t('proj_deliv_reject', { defaultValue: 'Refuser' })}
            </Button>
          )}
          {/* A verdict must not be a dead end: sending a rejected deliverable back
              into the flow clears the reason (and an acceptance, its signature). */}
          {canEdit && isTerminal(d.status) && (
            <Button size="sm" variant="ghost" icon={<Undo2 size={14} />}
              title={t('proj_deliv_reopen_hint', { defaultValue: 'Remettre le livrable en circulation (efface le motif de refus ou la réception).' })}
              onClick={() => patch(d.id, { status: 'delivered' })}>
              {t('proj_deliv_reopen', { defaultValue: 'Rouvrir' })}
            </Button>
          )}
        </div>
      ),
    },
  ]

  const canCreate = draft.name.trim().length > 0

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <PackageCheck size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">{t('proj_deliv_title', { defaultValue: 'Livrables' })}</h1>
          <div className="flex-1" />
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_deliv_new', { defaultValue: 'Nouveau livrable' })}
            </Button>
          )}
        </div>

        {/* The only progress figure that cannot be talked up: how many promises
            have actually been accepted. */}
        {total > 0 && (
          <div className="bg-surface-0 border border-border rounded-xl p-5">
            <ProgressBar
              value={accepted} max={total} variant="success" showValue t={t}
              label={t('proj_deliv_summary', {
                defaultValue: '{{accepted}} livrable(s) accepté(s) sur {{total}}',
                accepted, total,
              })}
              formatValue={(v, m) => `${v}/${m}`}
            />
            {rejected > 0 && (
              <p className="text-xs text-danger mt-2">
                {t('proj_deliv_rejected_count', { defaultValue: '{{n}} livrable(s) refusé(s), en attente de reprise.', n: rejected })}
              </p>
            )}
          </div>
        )}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        <DataTable<Deliverable>
          rows={rows}
          columns={columns}
          rowKey={d => d.id}
          loading={isLoading}
          error={isError ? t('proj_deliv_load_error', { defaultValue: 'Les livrables n’ont pas pu être chargés.' }) : undefined}
          onRetry={() => { void refetch() }}
          minTableWidth={1240}
          pageSize={0}
          resizableColumns={false}
          rowActions={canEdit ? [{
            id: 'delete',
            label: t('common_delete', { defaultValue: 'Supprimer' }),
            icon: <Trash2 size={14} />,
            danger: true,
            onClick: d => { void askDelete(d) },
          }] : undefined}
          emptyState={
            <EmptyState
              variant="first-use"
              icon={<PackageCheck size={26} />}
              title={t('proj_deliv_empty_title', { defaultValue: 'Aucun livrable' })}
              description={t('proj_deliv_empty', { defaultValue: 'Un livrable est ce que le projet remet : un document, un lot logiciel, une formation. Il n’est terminé que lorsque quelqu’un l’accepte.' })}
              action={canEdit ? {
                label: t('proj_deliv_new', { defaultValue: 'Nouveau livrable' }),
                icon: <Plus size={14} />,
                onClick: () => setCreateOpen(true),
              } : undefined}
              t={t}
            />
          }
          t={t}
        />
      </div>

      {createOpen && (
        <FloatingWindow
          title={t('proj_deliv_new', { defaultValue: 'Nouveau livrable' })}
          icon={<PackageCheck size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={480} defaultHeight={400} padding={16} t={t}
          actions={{
            confirm: {
              label: t('common_create', { defaultValue: 'Créer' }),
              disabled: !canCreate,
              loading: createMut.isPending,
              autoFocus: true,
              onClick: () => createMut.mutate({
                name: draft.name.trim(),
                code: draft.code.trim(),
                due_date: draft.due_date || null,
                task_id: draft.task_id || null,
              }),
            },
          }}
        >
          <div className="space-y-3">
            <div>
              <label className="text-sm text-text-secondary mb-1 block">{t('proj_deliv_name', { defaultValue: 'Livrable' })}</label>
              <Input
                autoFocus
                value={draft.name}
                placeholder={t('proj_deliv_name_ph', { defaultValue: 'Ce que le projet remettra…' })}
                onChange={e => setDraft(d => ({ ...d, name: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); createMut.mutate({ name: draft.name.trim(), code: draft.code.trim(), due_date: draft.due_date || null, task_id: draft.task_id || null }) } }}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">{t('proj_deliv_code', { defaultValue: 'Code' })}</label>
                <Input value={draft.code} placeholder="L-01" onChange={e => setDraft(d => ({ ...d, code: e.target.value }))} />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">{t('proj_deliv_due', { defaultValue: 'Échéance' })}</label>
                <Input type="date" value={draft.due_date} onChange={e => setDraft(d => ({ ...d, due_date: e.target.value }))} />
              </div>
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">{t('proj_deliv_task', { defaultValue: 'Lot de travail' })}</label>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={draft.task_id} options={taskOptions}
                onChange={v => setDraft(d => ({ ...d, task_id: v }))} />
            </div>
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
