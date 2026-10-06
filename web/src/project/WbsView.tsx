import { useEffect, useMemo, useState, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { ListTree, Milestone, FolderTree, FileText, Ban, ExternalLink, RotateCcw, Package, AlertTriangle } from 'lucide-react'
import { Button, Input, Textarea, Badge, Callout, EmptyState, useIsMobile } from '@ui'
import { projectsApi, type WbsElement, type WbsDictionaryEdit } from '../api'

// Work breakdown structure: the outline numbering that gives every work package
// an address, plus the dictionary that says what each package covers — and what
// it does not.
//
// The list arrives flat and already ordered by code; the tree is drawn from the
// code itself (one indent per dot), so nothing here can disagree with the
// numbering the server derived from the plan.
//
// ⚠️ The dictionary shown on the right comes from the LIST payload, never from
// `getDictionaryEntry`: that endpoint CREATES an empty entry on first read, so
// merely selecting a row would flip `has_dictionary` to true and destroy the one
// signal this screen exists for — which work packages nobody has defined.

/** Depth of an outline code: "1" → 0, "1.2" → 1, "1.2.3" → 2. */
const depthOf = (wbs: string) => (wbs ? wbs.split('.').length - 1 : 0)

/** Short single-line field of the dictionary, committed to the server on blur. */
function ShortField({ label, value, placeholder, disabled, onCommit }: {
  label: string
  value: string
  placeholder?: string
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves, while
  // typing it does not, so nothing being typed is ever pulled from under.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="text-sm text-text-secondary mb-1 block">{label}</label>
      <Input
        value={draft}
        placeholder={placeholder}
        disabled={disabled}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => { if (draft !== value) onCommit(draft) }}
      />
    </div>
  )
}

/** Long free-text field of the dictionary, committed to the server on blur. */
function LongField({ label, hint, value, rows = 3, disabled, onCommit }: {
  label?: string
  hint?: string
  value: string
  rows?: number
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])

  // A dictionary entry is written once and read many times, often by someone
  // arguing about scope. Clipping the last line of "what this package excludes"
  // behind a scrollbar defeats the point, so the field grows to its content.
  // The @ui Textarea does not forward a ref, so the element is reached through
  // its wrapper rather than held directly.
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = box.current?.querySelector('textarea')
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 420)}px`
  }, [draft])

  return (
    <div ref={box}>
      {label && <label className="text-sm text-text-secondary mb-1 block">{label}</label>}
      <Textarea
        rows={rows}
        className="h-auto min-h-0 resize-y overflow-hidden"
        value={draft}
        disabled={disabled}
        hint={hint}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => { if (draft !== value) onCommit(draft) }}
      />
    </div>
  )
}

/** One node of the breakdown, indented by the depth of its own outline code. */
function WbsRow({ element, depth, selected, undefinedPackage, onSelect }: {
  element: WbsElement
  depth: number
  selected: boolean
  /** A terminal work package with no dictionary entry: nobody has defined it. */
  undefinedPackage: boolean
  onSelect: () => void
}) {
  const { t } = useTranslation('office')
  const isSummary = element.task_type === 'summary'
  const Icon = element.task_type === 'milestone' ? Milestone : isSummary ? FolderTree : FileText

  return (
    <button
      onClick={onSelect}
      className={`w-full flex items-center gap-2 py-1.5 pr-3 text-left rounded-md ${selected ? 'bg-surface-2' : 'hover:bg-surface-2'}`}
      style={{ paddingLeft: 8 + depth * 18 }}
    >
      {/* Monospaced so the codes line up in a column whatever their length. */}
      <span className="font-mono text-xs text-text-tertiary shrink-0 tabular-nums">{element.wbs || '—'}</span>
      <Icon size={14} className={`shrink-0 ${isSummary ? 'text-text-secondary' : 'text-text-tertiary'}`} />
      <span className={`truncate ${isSummary ? 'text-sm font-semibold text-text-primary' : 'text-sm text-text-primary'}`}>
        {element.name}
      </span>

      {undefinedPackage && (
        <span
          className="shrink-0"
          title={t('proj_wbs_undefined_hint', { defaultValue: 'Ce lot de travail n’a pas d’entrée de dictionnaire : personne n’a écrit ce qu’il couvre.' })}
        >
          <Badge variant="warning" size="sm">
            <span className="inline-flex items-center gap-1"><AlertTriangle size={11} />{t('proj_wbs_undefined', { defaultValue: 'à définir' })}</span>
          </Badge>
        </span>
      )}

      <div className="flex-1" />

      {element.deliverable_count > 0 && (
        <span
          className="shrink-0 inline-flex items-center gap-1 text-[11px] text-text-tertiary"
          title={t('proj_wbs_deliverables_hint', { defaultValue: 'Livrables produits par ce lot de travail' })}
        >
          <Package size={11} />
          {element.deliverable_count}
        </span>
      )}

      <div className="shrink-0 flex items-center gap-1.5 w-[5.5rem] justify-end">
        <div className="w-10 h-1.5 rounded-full bg-surface-2 overflow-hidden">
          <div
            className="h-full rounded-full"
            style={{ width: `${Math.max(0, Math.min(100, element.progress))}%`, background: element.progress >= 100 ? '#1e8e3e' : 'var(--color-primary)' }}
          />
        </div>
        <span className="text-[11px] text-text-tertiary tabular-nums w-8 text-right">{element.progress}%</span>
      </div>
    </button>
  )
}

export default function WbsView({ projectId, onOpenTask, canEdit = true }: {
  projectId: string
  /** Opens the selected work package in the schedule, when the host offers it. */
  onOpenTask?: (taskId: string) => void
  /** False in the mobile reading mode, where the breakdown is shown, not edited. */
  canEdit?: boolean
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const { data: elements, isLoading } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['wbs', projectId] })
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const updateMut = useMutation({
    mutationFn: ({ taskId, data }: { taskId: string; data: WbsDictionaryEdit }) =>
      projectsApi.updateDictionaryEntry(projectId, taskId, data),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const renumberMut = useMutation({
    mutationFn: () => projectsApi.renumberWbs(projectId),
    onSuccess: () => {
      setErrorMsg(null)
      inval()
      // The codes belong to the tasks: whoever is showing the plan must reread it.
      qc.invalidateQueries({ queryKey: ['project', projectId] })
    },
    onError: fail,
  })

  const list = useMemo(() => elements ?? [], [elements])

  // A node is terminal when nothing declares it as a parent. Summaries and
  // milestones are excluded from the "undefined" count: a summary is an
  // aggregate and a milestone marks a date — neither is a work package.
  const parents = useMemo(() => {
    const set = new Set<string>()
    for (const el of list) if (el.parent_id) set.add(el.parent_id)
    return set
  }, [list])
  const isUndefinedPackage = (el: WbsElement) =>
    !el.has_dictionary && !parents.has(el.id) && el.task_type !== 'summary' && el.task_type !== 'milestone'
  const undefinedCount = useMemo(() => list.filter(isUndefinedPackage).length, [list, parents])
  const packageCount = useMemo(
    () => list.filter(el => !parents.has(el.id) && el.task_type !== 'summary' && el.task_type !== 'milestone').length,
    [list, parents],
  )

  const selected = list.find(el => el.id === selectedId) ?? null
  const dict = selected?.dictionary ?? null
  const set = (patch: WbsDictionaryEdit) => { if (selected) updateMut.mutate({ taskId: selected.id, data: patch }) }
  // Reading a field the server has never written: an absent entry reads empty
  // and is created by the first commit, which is exactly when it starts to mean
  // something.
  const field = (key: keyof WbsDictionaryEdit) => (dict?.[key] as string | undefined) ?? ''

  const panel = selected ? (
    <div className="h-full overflow-y-auto">
      <div className="px-4 py-3 border-b border-border flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="font-mono text-xs text-text-tertiary">{selected.wbs}</p>
          <p className="text-sm font-semibold text-text-primary break-words">{selected.name}</p>
        </div>
        {onOpenTask && (
          <Button
            size="sm" variant="ghost" icon={<ExternalLink size={14} />}
            title={t('proj_wbs_open_task', { defaultValue: 'Ouvrir dans le planning' })}
            onClick={() => onOpenTask(selected.id)}
          >
            {t('proj_wbs_open_task_short', { defaultValue: 'Planning' })}
          </Button>
        )}
      </div>

      <div className="p-4 space-y-4">
        {!selected.has_dictionary && (
          <Callout variant="info" icon={<FileText size={16} />}>
            {t('proj_wbs_dict_empty', { defaultValue: 'Aucune entrée de dictionnaire pour ce lot. Elle sera créée dès la première valeur saisie.' })}
          </Callout>
        )}

        <div className={isMobile ? 'grid grid-cols-1 gap-3' : 'grid grid-cols-2 gap-3'}>
          <ShortField
            label={t('proj_wbs_code_of_account', { defaultValue: 'Code comptable' })}
            placeholder={t('proj_wbs_code_of_account_ph', { defaultValue: 'Imputation budgétaire' })}
            value={field('code_of_account')} disabled={!canEdit}
            onCommit={v => set({ code_of_account: v })} />
          <ShortField
            label={t('proj_wbs_responsible', { defaultValue: 'Responsable' })}
            placeholder={t('proj_wbs_responsible_ph', { defaultValue: 'Qui répond de ce lot' })}
            value={field('responsible')} disabled={!canEdit}
            onCommit={v => set({ responsible: v })} />
        </div>

        <LongField
          label={t('proj_wbs_sow', { defaultValue: 'Énoncé des travaux' })}
          hint={t('proj_wbs_sow_hint', { defaultValue: 'Ce que ce lot produit, concrètement.' })}
          rows={4} value={field('statement_of_work')} disabled={!canEdit}
          onCommit={v => set({ statement_of_work: v })} />

        {/* Out of scope — the field that lets a request be turned down. Given its
            own block rather than a line among eight, because it is the only one
            that is read AFTER the project started, under pressure. */}
        <Callout
          variant="warning" icon={<Ban size={16} />}
          title={t('proj_wbs_exclusions', { defaultValue: 'Hors périmètre' })}
        >
          <LongField
            hint={t('proj_wbs_exclusions_hint', { defaultValue: 'Ce que ce lot ne couvre PAS : la ligne opposable à une demande hors sujet.' })}
            rows={3} value={field('exclusions')} disabled={!canEdit}
            onCommit={v => set({ exclusions: v })} />
        </Callout>

        <LongField
          label={t('proj_wbs_acceptance', { defaultValue: 'Critères d’acceptation' })}
          hint={t('proj_wbs_acceptance_hint', { defaultValue: 'À quoi on reconnaîtra que le lot est réellement livré.' })}
          rows={3} value={field('acceptance_criteria')} disabled={!canEdit}
          onCommit={v => set({ acceptance_criteria: v })} />

        <LongField
          label={t('proj_wbs_assumptions', { defaultValue: 'Hypothèses' })}
          rows={3} value={field('assumptions')} disabled={!canEdit}
          onCommit={v => set({ assumptions: v })} />

        <LongField
          label={t('proj_wbs_quality', { defaultValue: 'Exigences de qualité' })}
          rows={3} value={field('quality_requirements')} disabled={!canEdit}
          onCommit={v => set({ quality_requirements: v })} />

        <LongField
          label={t('proj_wbs_risks', { defaultValue: 'Risques' })}
          rows={3} value={field('risks')} disabled={!canEdit}
          onCommit={v => set({ risks: v })} />
      </div>
    </div>
  ) : (
    <div className="h-full flex items-center justify-center p-6">
      <p className="text-sm text-text-tertiary text-center max-w-[15rem]">
        {t('proj_wbs_pick', { defaultValue: 'Sélectionnez un élément de l’arborescence pour lire et compléter son dictionnaire.' })}
      </p>
    </div>
  )

  const tree = (
    <div className="p-4">
      {list.length === 0 ? (
        <EmptyState
          variant="first-use"
          icon={<ListTree size={26} />}
          title={t('proj_wbs_empty_title', { defaultValue: 'Aucun élément' })}
          description={t('proj_wbs_empty', { defaultValue: 'L’arborescence est numérotée à partir du planning : créez des tâches et des tâches récapitulatives, leur code (1, 1.2, 1.2.3…) en découle.' })}
          t={t}
        />
      ) : (
        <div className="bg-surface-0 border border-border rounded-xl p-2">
          {list.map(el => (
            <WbsRow
              key={el.id}
              element={el}
              depth={depthOf(el.wbs)}
              selected={el.id === selectedId}
              undefinedPackage={isUndefinedPackage(el)}
              onSelect={() => setSelectedId(prev => (prev === el.id ? null : el.id))}
            />
          ))}
        </div>
      )}
    </div>
  )

  if (isLoading) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1 p-6">
        <p className="text-sm text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</p>
      </div>
    )
  }

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-surface-1">
      <div className="px-4 pt-4 flex items-center gap-3 flex-wrap">
        <ListTree size={20} className="text-text-secondary shrink-0" />
        <h1 className="text-xl font-semibold text-text-primary">{t('proj_wbs_title', { defaultValue: 'Organigramme des tâches (WBS)' })}</h1>
        {packageCount > 0 && (
          <Badge variant={undefinedCount > 0 ? 'warning' : 'success'}>
            {undefinedCount > 0
              ? t('proj_wbs_undefined_count', { defaultValue: '{{n}} lot(s) sans dictionnaire', n: undefinedCount })
              : t('proj_wbs_all_defined', { defaultValue: 'Tous les lots sont définis' })}
          </Badge>
        )}
        <div className="flex-1" />
        {canEdit && (
          <Button
            size="sm" variant="secondary" icon={<RotateCcw size={14} />}
            loading={renumberMut.isPending}
            title={t('proj_wbs_renumber_hint', { defaultValue: 'Recalcule les codes à partir de l’arborescence du planning.' })}
            onClick={() => renumberMut.mutate()}
          >
            {t('proj_wbs_renumber', { defaultValue: 'Renuméroter' })}
          </Button>
        )}
      </div>

      {errorMsg && (
        <div className="px-4 pt-3">
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        </div>
      )}

      {/* On a phone the dictionary sits BELOW the tree; on a desktop it is the
          right-hand panel, so the tree keeps its own scroll. */}
      {isMobile ? (
        <div className="flex-1 min-h-0 overflow-y-auto">
          {tree}
          {selected && <div className="mx-4 mb-4 bg-surface-0 border border-border rounded-xl">{panel}</div>}
        </div>
      ) : (
        <div className="flex-1 min-h-0 flex">
          <div className="flex-1 min-w-0 overflow-y-auto">{tree}</div>
          <div className="w-[26rem] shrink-0 border-l border-border bg-surface-0 overflow-hidden">{panel}</div>
        </div>
      )}
    </div>
  )
}
