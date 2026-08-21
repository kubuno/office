import { useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import { getDateLocale, useConfirm } from '@kubuno/sdk'
import { Receipt, Plus, Trash2, AlertTriangle } from 'lucide-react'
import {
  Button, Input, Dropdown, DataTable, Callout, EmptyState, Tooltip,
  FloatingWindow, ConfirmDialog, useIsMobile,
  type DataTableColumn, type DropdownOption,
} from '@ui'
import { projectsApi, type CostEntry, type CostEntryEdit, type CostCategory } from '../api'

// Direct cost register — the actual money spent, entry by entry.
//
// This screen holds ONE half of the actual cost: the direct spend (licences,
// hardware, subcontracting, travel). The other half — labour — is derived from
// the hours logged on the tasks times the resource rate, and is NEVER typed in
// here. Both halves are added by the server when it computes AC, so an expense
// filed as `labour` on top of logged hours is counted twice. The category still
// exists (a fixed-price invoice for an external team is real labour nobody logs
// hours for), so it is offered — with a warning rather than a ban.

const CATEGORIES: CostCategory[] = ['subcontract', 'licence', 'hardware', 'travel', 'other', 'labour']

/** Today, as the `yyyy-MM-dd` an `<Input type="date">` expects. */
const today = () => new Date().toISOString().slice(0, 10)

/** Cell field committed on blur — one PATCH per edit, not one per keystroke. */
function CellInput({ value, placeholder, type, className, align, onCommit }: {
  value: string
  placeholder?: string
  type?: string
  className?: string
  align?: 'left' | 'right'
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <Input
      type={type}
      className={`${className ?? ''}${align === 'right' ? ' text-right tabular-nums' : ''}`}
      value={draft}
      placeholder={placeholder}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        if (draft === value) return
        onCommit(draft)
      }}
    />
  )
}

export default function CostEntriesView({ projectId, canEdit = true, currency }: {
  projectId: string
  /** False in the mobile reading mode, where expenses are shown, not edited. */
  canEdit?: boolean
  /** Currency of the project's cost configuration, e.g. `EUR`. */
  currency?: string
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const [draft, setDraft] = useState<{ amount: string; incurred_on: string; category: CostCategory; description: string; task_id: string }>(
    { amount: '', incurred_on: today(), category: 'other', description: '', task_id: '' })

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['cost-entries', projectId],
    queryFn: () => projectsApi.listCostEntries(projectId),
  })
  // The breakdown supplies the work packages an expense can be charged to; the
  // query key is shared with the WBS screen, so both read one cache entry.
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })

  // An expense moves the actual cost, so the earned-value screen goes stale too.
  const inval = () => {
    qc.invalidateQueries({ queryKey: ['cost-entries', projectId] })
    qc.invalidateQueries({ queryKey: ['costs', projectId] })
  }
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const done = () => { setErrorMsg(null); inval() }

  const createMut = useMutation({
    mutationFn: (payload: CostEntryEdit & { amount: number }) => projectsApi.createCostEntry(projectId, payload),
    onSuccess: () => {
      done()
      setCreateOpen(false)
      setDraft({ amount: '', incurred_on: today(), category: 'other', description: '', task_id: '' })
    },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: CostEntryEdit }) => projectsApi.updateCostEntry(projectId, id, payload),
    onSuccess: done,
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteCostEntry(projectId, id),
    onSuccess: done,
    onError: fail,
  })

  const rows = useMemo(() => data?.entries ?? [], [data])
  const total = data?.total ?? 0

  // Where the money went. This — not the list — is what the screen is opened for.
  const byCategory = useMemo(() => {
    const sums = new Map<CostCategory, number>()
    for (const e of rows) sums.set(e.category, (sums.get(e.category) ?? 0) + e.amount)
    return [...sums.entries()].sort((a, b) => b[1] - a[1])
  }, [rows])

  const hasLabour = rows.some(e => e.category === 'labour')

  // `Intl` throws on anything that is not an ISO 4217 code, so an exotic currency
  // falls back to a plain number followed by the code as written.
  const money = useMemo(() => {
    if (currency) {
      try {
        const nf = new Intl.NumberFormat(i18n.language, { style: 'currency', currency, maximumFractionDigits: 2 })
        return (n: number) => nf.format(n)
      } catch { /* not an ISO code — handled below */ }
      const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 })
      return (n: number) => `${nf.format(n)} ${currency}`
    }
    const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 })
    return (n: number) => nf.format(n)
  }, [i18n.language, currency])

  const categoryLabel = (c: CostCategory) => ({
    labour:      t('proj_cost_cat_labour', { defaultValue: 'Main-d’œuvre' }),
    subcontract: t('proj_cost_cat_subcontract', { defaultValue: 'Sous-traitance' }),
    licence:     t('proj_cost_cat_licence', { defaultValue: 'Licences' }),
    hardware:    t('proj_cost_cat_hardware', { defaultValue: 'Matériel' }),
    travel:      t('proj_cost_cat_travel', { defaultValue: 'Déplacements' }),
    other:       t('proj_cost_cat_other', { defaultValue: 'Autre' }),
  })[c]

  const labourWarning = t('proj_cost_labour_warning', {
    defaultValue: 'La main-d’œuvre est déjà calculée à partir des heures saisies × taux horaire. Ne saisissez ici que ce qu’aucune heure ne couvre — une prestation au forfait, par exemple.',
  })

  const categoryOptions: DropdownOption[] = CATEGORIES.map(c => ({ value: c, label: categoryLabel(c) }))

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_cost_no_task', { defaultValue: '— Projet (aucun lot) —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])

  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })
  const patch = (id: string, payload: CostEntryEdit) => updateMut.mutate({ id, payload })

  const askDelete = async (e: CostEntry) => {
    const ok = await confirm({
      title: t('proj_cost_delete_title', { defaultValue: 'Supprimer la dépense ?' }),
      message: t('proj_cost_delete_msg', {
        defaultValue: 'La dépense de {{amount}} du {{date}} sera supprimée définitivement, et retirée du coût réel du projet.',
        amount: money(e.amount), date: fmtDate(e.incurred_on),
      }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) deleteMut.mutate(e.id)
  }

  const columns: DataTableColumn<CostEntry>[] = [
    {
      id: 'date',
      header: t('proj_cost_date', { defaultValue: 'Date' }),
      headerText: t('proj_cost_date', { defaultValue: 'Date' }),
      width: 150,
      sortValue: e => e.incurred_on,
      cell: e => canEdit
        // `incurred_on` is never null server-side: an emptied field is ignored
        // rather than sent, so the row keeps the date it had.
        ? <Input type="date" value={e.incurred_on}
            onChange={ev => { if (ev.target.value) patch(e.id, { incurred_on: ev.target.value }) }} />
        : <span className="text-sm text-text-secondary">{fmtDate(e.incurred_on)}</span>,
    },
    {
      id: 'description',
      header: t('proj_cost_description', { defaultValue: 'Dépense' }),
      headerText: t('proj_cost_description', { defaultValue: 'Dépense' }),
      minWidth: 220,
      primary: true,
      required: true,
      sortValue: e => e.description,
      cell: e => canEdit
        ? <CellInput value={e.description}
            placeholder={t('proj_cost_description_ph', { defaultValue: 'Ce qui a été payé…' })}
            onCommit={v => patch(e.id, { description: v })} />
        : <span className="text-sm text-text-primary">{e.description || '—'}</span>,
    },
    {
      id: 'category',
      header: t('proj_cost_category', { defaultValue: 'Catégorie' }),
      headerText: t('proj_cost_category', { defaultValue: 'Catégorie' }),
      width: 200,
      sortValue: e => e.category,
      cell: e => (
        <div className="flex items-center gap-1.5">
          {canEdit
            ? <Dropdown className="flex-1 min-w-0" height={36} fontSize={14} focusable
                value={e.category} options={categoryOptions}
                onChange={v => patch(e.id, { category: v as CostCategory })} />
            : <span className="text-sm text-text-secondary">{categoryLabel(e.category)}</span>}
          {/* Discreet, not blocking: the double count is a risk, not an error. */}
          {e.category === 'labour' && (
            <Tooltip label={labourWarning}>
              <span className="text-warning shrink-0 flex items-center"><AlertTriangle size={14} /></span>
            </Tooltip>
          )}
        </div>
      ),
    },
    {
      id: 'task',
      header: t('proj_cost_task', { defaultValue: 'Lot de travail' }),
      headerText: t('proj_cost_task', { defaultValue: 'Lot de travail' }),
      width: 220,
      sortValue: e => e.task_name,
      cell: e => canEdit
        ? <Dropdown className="w-full" height={36} fontSize={14} focusable
            value={e.task_id ?? ''} options={taskOptions}
            onChange={v => patch(e.id, { task_id: v || null })} />
        : <span className="text-sm text-text-secondary">
            {e.task_name ?? t('proj_cost_project_level', { defaultValue: 'Projet' })}
          </span>,
    },
    {
      id: 'amount',
      header: t('proj_cost_amount', { defaultValue: 'Montant' }),
      headerText: t('proj_cost_amount', { defaultValue: 'Montant' }),
      width: 160,
      align: 'right',
      sortValue: e => e.amount,
      cell: e => canEdit
        ? <CellInput type="number" align="right" value={String(e.amount)}
            onCommit={v => {
              const n = parseFloat(v)
              // An amount is what the entry IS: refuse to turn it into a blank.
              if (!Number.isFinite(n)) { void refetch(); return }
              patch(e.id, { amount: n })
            }} />
        : <span className="text-sm text-text-primary tabular-nums">{money(e.amount)}</span>,
    },
  ]

  const draftAmount = parseFloat(draft.amount)
  const canCreate = Number.isFinite(draftAmount)
  const submitCreate = () => {
    if (!canCreate) return
    createMut.mutate({
      amount: draftAmount,
      incurred_on: draft.incurred_on || today(),
      category: draft.category,
      description: draft.description.trim(),
      task_id: draft.task_id || null,
    })
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <Receipt size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">{t('proj_cost_title', { defaultValue: 'Dépenses directes' })}</h1>
          <div className="flex-1" />
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_cost_new', { defaultValue: 'Nouvelle dépense' })}
            </Button>
          )}
        </div>

        {/* The total first, then where it went: the two figures anyone opening
            this screen came for. */}
        {rows.length > 0 && (
          <div className="bg-surface-0 border border-border rounded-xl p-5 space-y-4">
            <div>
              <p className="text-sm text-text-secondary">{t('proj_cost_total', { defaultValue: 'Total des dépenses directes' })}</p>
              <p className="text-2xl font-semibold text-text-primary tabular-nums">{money(total)}</p>
              <p className="text-xs text-text-tertiary mt-1">
                {t('proj_cost_total_hint', { defaultValue: 'Hors main-d’œuvre : celle-ci est valorisée à partir des heures saisies.' })}
              </p>
            </div>
            <div className="flex flex-col gap-2">
              {byCategory.map(([cat, sum]) => (
                <div key={cat} className="flex items-center gap-3">
                  <span className="text-sm text-text-secondary flex-1 min-w-0 truncate">{categoryLabel(cat)}</span>
                  {/* The bar is a comfort, the figures are the point: a narrow
                      panel drops it rather than squeezing the amounts. */}
                  {!isMobile && (
                    <span className="w-40 h-2 rounded-full bg-surface-2 overflow-hidden shrink-0">
                      <span className="block h-full rounded-full bg-primary"
                        style={{ width: total > 0 ? `${Math.max(2, (sum / total) * 100)}%` : '0%' }} />
                    </span>
                  )}
                  <span className="text-sm text-text-primary tabular-nums text-right shrink-0">{money(sum)}</span>
                  <span className="text-xs text-text-tertiary tabular-nums w-12 text-right shrink-0">
                    {total > 0 ? `${Math.round((sum / total) * 100)} %` : '—'}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {hasLabour && (
          <Callout variant="warning" t={t}
            title={t('proj_cost_labour_title', { defaultValue: 'Une dépense est classée en main-d’œuvre' })}>
            {labourWarning}
          </Callout>
        )}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        <DataTable<CostEntry>
          rows={rows}
          columns={columns}
          rowKey={e => e.id}
          loading={isLoading}
          error={isError ? t('proj_cost_load_error', { defaultValue: 'Les dépenses n’ont pas pu être chargées.' }) : undefined}
          onRetry={() => { void refetch() }}
          defaultSort={{ columnId: 'date', direction: 'desc' }}
          minTableWidth={1180}
          pageSize={0}
          resizableColumns={false}
          rowActions={canEdit ? [{
            id: 'delete',
            label: t('common_delete', { defaultValue: 'Supprimer' }),
            icon: <Trash2 size={14} />,
            danger: true,
            onClick: e => { void askDelete(e) },
          }] : undefined}
          emptyState={
            <EmptyState
              variant="first-use"
              icon={<Receipt size={26} />}
              title={t('proj_cost_empty_title', { defaultValue: 'Aucune dépense' })}
              description={t('proj_cost_empty', {
                defaultValue: 'On ne saisit ici que les dépenses DIRECTES : licences, matériel, sous-traitance, déplacements. Le coût de la main-d’œuvre n’a rien à y faire — il est calculé à partir des heures saisies sur les tâches × le taux horaire des ressources. Le ressaisir ici le compterait deux fois.',
              })}
              action={canEdit ? {
                label: t('proj_cost_new', { defaultValue: 'Nouvelle dépense' }),
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
          title={t('proj_cost_new', { defaultValue: 'Nouvelle dépense' })}
          icon={<Receipt size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={480} defaultHeight={460} padding={16} t={t}
          actions={{
            confirm: {
              label: t('common_create', { defaultValue: 'Créer' }),
              disabled: !canCreate,
              loading: createMut.isPending,
              autoFocus: true,
              onClick: submitCreate,
            },
          }}
        >
          <div className="space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {currency
                    ? t('proj_cost_amount_cur', { defaultValue: 'Montant ({{cur}})', cur: currency })
                    : t('proj_cost_amount', { defaultValue: 'Montant' })}
                </label>
                <Input
                  autoFocus type="number" className="text-right tabular-nums"
                  value={draft.amount} placeholder="0"
                  onChange={e => setDraft(d => ({ ...d, amount: e.target.value }))}
                  onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); submitCreate() } }}
                />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">{t('proj_cost_date', { defaultValue: 'Date' })}</label>
                <Input type="date" value={draft.incurred_on}
                  onChange={e => setDraft(d => ({ ...d, incurred_on: e.target.value }))} />
              </div>
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">{t('proj_cost_category', { defaultValue: 'Catégorie' })}</label>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={draft.category} options={categoryOptions}
                onChange={v => setDraft(d => ({ ...d, category: v as CostCategory }))} />
              {draft.category === 'labour' && (
                <p className="text-xs text-warning mt-1 flex items-start gap-1">
                  <AlertTriangle size={13} className="shrink-0 mt-0.5" />
                  <span>{labourWarning}</span>
                </p>
              )}
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">{t('proj_cost_description', { defaultValue: 'Dépense' })}</label>
              <Input value={draft.description}
                placeholder={t('proj_cost_description_ph', { defaultValue: 'Ce qui a été payé…' })}
                onChange={e => setDraft(d => ({ ...d, description: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreate) { e.preventDefault(); submitCreate() } }} />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">{t('proj_cost_task', { defaultValue: 'Lot de travail' })}</label>
              <Dropdown
                className="w-full" width="100%" height={36} fontSize={14} focusable
                value={draft.task_id} options={taskOptions}
                onChange={v => setDraft(d => ({ ...d, task_id: v }))} />
              <p className="text-xs text-text-tertiary mt-1">
                {t('proj_cost_task_hint', { defaultValue: 'Sans lot, la dépense reste au niveau du projet : elle compte dans le coût réel, mais n’est imputée à aucun lot.' })}
              </p>
            </div>
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
