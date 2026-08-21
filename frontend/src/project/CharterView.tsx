import { useEffect, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import { getDateLocale, prompt, useConfirm } from '@kubuno/sdk'
import { ScrollText, Milestone, Plus, Trash2, History, Wand2, Lock, CheckCircle2 } from 'lucide-react'
import { Button, Input, Textarea, Badge, Callout, Accordion, FloatingWindow, ConfirmDialog, useIsMobile, type AccordionItemDef } from '@ui'
import { projectsApi, type CharterEdit, type CharterMilestone } from '../api'

// Project charter: what the project is for, who authorises it, and what success
// will mean. Read as a document rather than a form — successive titled sections,
// each field saved on blur, no global "Save" button.
//
// An APPROVED charter is read-only: every field is disabled here, not merely
// refused by the server, and reopening it (owner only) files a dated revision.

/** Long free-text field of the charter, committed to the server on blur. */
function LongField({ label, hint, value, rows = 3, disabled, onCommit }: {
  label: string
  hint?: string
  value: string
  rows?: number
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves (another
  // editor, an undone revision) — while typing it does not, so nothing is lost.
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

/** Short single-line field of the charter, committed to the server on blur. */
function ShortField({ label, value, placeholder, disabled, onCommit }: {
  label: string
  value: string
  placeholder?: string
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
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

/** One milestone row: its name is committed on blur, its date as soon as picked. */
function MilestoneRow({ milestone, disabled, onRename, onSetDate, onDelete }: {
  milestone: CharterMilestone
  disabled?: boolean
  onRename: (name: string) => void
  onSetDate: (day: string) => void
  onDelete: () => void
}) {
  const { t } = useTranslation('office')
  const [draft, setDraft] = useState(milestone.name)
  useEffect(() => { setDraft(milestone.name) }, [milestone.name])
  return (
    <li className="flex items-center gap-2">
      <Milestone size={15} className="text-text-secondary shrink-0" />
      <Input
        className="flex-1 min-w-0"
        value={draft}
        disabled={disabled}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => { if (draft.trim() && draft !== milestone.name) onRename(draft.trim()) }}
      />
      <Input
        type="date"
        className="max-w-[9.5rem]"
        value={milestone.target_date ?? ''}
        disabled={disabled}
        title={t('proj_charter_ms_date', { defaultValue: 'Date cible' })}
        onChange={e => onSetDate(e.target.value)}
      />
      {milestone.task_id && (
        <span className="text-[11px] text-text-tertiary shrink-0 whitespace-nowrap" title={t('proj_charter_ms_linked_hint', { defaultValue: 'Ce jalon existe déjà comme jalon du planning.' })}>
          {t('proj_charter_ms_linked', { defaultValue: 'dans le planning' })}
        </span>
      )}
      <button
        onClick={onDelete}
        disabled={disabled}
        title={t('common_delete', { defaultValue: 'Supprimer' })}
        className="p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger disabled:opacity-40 disabled:pointer-events-none"
      >
        <Trash2 size={15} />
      </button>
    </li>
  )
}

/** A titled block of the charter document. */
function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-surface-0 border border-border rounded-xl p-5">
      <h2 className="text-sm font-semibold text-text-primary mb-3">{title}</h2>
      <div className="space-y-3">{children}</div>
    </div>
  )
}

export default function CharterView({ projectId, isOwner, canEdit = true }: {
  projectId: string
  /** Approving and reopening a charter are the owner's calls (the server agrees). */
  isOwner: boolean
  /** False in the mobile reading mode, where the project is shown, not edited. */
  canEdit?: boolean
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()
  const [historyOpen, setHistoryOpen] = useState(false)
  const [genResult, setGenResult] = useState<string | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const { data, isLoading } = useQuery({
    queryKey: ['charter', projectId],
    queryFn: () => projectsApi.getCharter(projectId),
  })

  const inval = () => qc.invalidateQueries({ queryKey: ['charter', projectId] })
  // The server flattens errors to `{ message, code }` at the root — there is no
  // `response.data` to read here (core api client interceptor).
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const updateMut = useMutation({
    mutationFn: (patch: CharterEdit) => projectsApi.updateCharter(projectId, patch),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const approveMut = useMutation({
    mutationFn: () => projectsApi.approveCharter(projectId),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const reviseMut = useMutation({
    mutationFn: (reason: string) => projectsApi.reviseCharter(projectId, reason),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const addMsMut = useMutation({
    mutationFn: (name: string) => projectsApi.createCharterMilestone(projectId, { name }),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const updateMsMut = useMutation({
    mutationFn: ({ mid, data: d }: { mid: string; data: Partial<Pick<CharterMilestone, 'name' | 'target_date'>> }) =>
      projectsApi.updateCharterMilestone(projectId, mid, d),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const deleteMsMut = useMutation({
    mutationFn: (mid: string) => projectsApi.deleteCharterMilestone(projectId, mid),
    onSuccess: () => { setErrorMsg(null); inval() },
    onError: fail,
  })
  const generateMut = useMutation({
    mutationFn: () => projectsApi.generateCharterMilestones(projectId),
    onSuccess: (r) => {
      setErrorMsg(null)
      setGenResult(t('proj_charter_generated', {
        defaultValue: '{{created}} jalon(s) créé(s), {{updated}} mis à jour.',
        created: r.created, updated: r.updated,
      }))
      inval()
      // The schedule just gained (or lost) milestones: refresh what the editor holds.
      qc.invalidateQueries({ queryKey: ['project', projectId] })
    },
    onError: fail,
  })

  // Revisions are only fetched when the history window is actually opened.
  const { data: revisions } = useQuery({
    queryKey: ['charter-revisions', projectId],
    queryFn: () => projectsApi.listCharterRevisions(projectId),
    enabled: historyOpen,
  })

  const [newMs, setNewMs] = useState('')

  if (isLoading || !data) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="max-w-3xl mx-auto p-6">
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        </div>
      </div>
    )
  }

  const { charter, milestones, revision_count: revisionCount, approved_by_name: approvedByName } = data
  // Two different things: an APPROVED charter is frozen for everyone (and says
  // so), while a read-only *context* — the mobile reading mode — merely disables
  // the fields without pretending the charter has been signed off.
  const approved = charter.status === 'approved'
  const readOnly = approved || !canEdit
  const set = (patch: CharterEdit) => updateMut.mutate(patch)
  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })
  const fmtDateTime = (iso: string) => format(parseISO(iso), 'd MMM yyyy, HH:mm', { locale: getDateLocale(i18n.language) })

  const askApprove = async () => {
    const ok = await confirm({
      title: t('proj_charter_approve_title', { defaultValue: 'Approuver la charte ?' }),
      message: t('proj_charter_approve_msg', { defaultValue: 'La charte deviendra non modifiable. Pour la modifier ensuite, il faudra la rouvrir : une révision datée sera conservée.' }),
      confirmLabel: t('proj_charter_approve', { defaultValue: 'Approuver' }),
    })
    if (ok) approveMut.mutate()
  }

  const askRevise = async () => {
    const reason = await prompt({
      title: t('proj_charter_revise_title', { defaultValue: 'Rouvrir la charte' }),
      message: t('proj_charter_revise_msg', { defaultValue: 'La version approuvée sera archivée en révision datée. Pourquoi la rouvrez-vous ?' }),
      placeholder: t('proj_charter_revise_placeholder', { defaultValue: 'Motif de la révision…' }),
      confirmLabel: t('proj_charter_reopen', { defaultValue: 'Rouvrir' }),
      multiline: true,
      allowEmpty: true,
    })
    if (reason === null) return // cancelled
    reviseMut.mutate(reason)
  }

  const addMilestone = () => {
    const name = newMs.trim()
    if (!name) return
    setNewMs('')
    addMsMut.mutate(name)
  }

  const revisionItems: AccordionItemDef[] = (revisions ?? []).map(rev => ({
    id: rev.id,
    title: (
      <span className="flex items-center gap-2 min-w-0">
        <span className="text-sm text-text-primary shrink-0">{fmtDateTime(rev.revised_at)}</span>
        {rev.revised_by_name && <span className="text-xs text-text-tertiary shrink-0">· {rev.revised_by_name}</span>}
        <span className="text-xs text-text-tertiary truncate">
          {rev.reason.trim() || t('proj_charter_no_reason', { defaultValue: 'Sans motif' })}
        </span>
      </span>
    ),
    content: (
      <div className="space-y-3 text-sm">
        <div>
          <p className="text-xs text-text-tertiary mb-0.5">{t('proj_charter_purpose', { defaultValue: 'Objet du projet' })}</p>
          <p className="text-text-primary whitespace-pre-wrap">
            {rev.snapshot.purpose?.trim() || t('proj_charter_empty_field', { defaultValue: '—' })}
          </p>
        </div>
        <div>
          <p className="text-xs text-text-tertiary mb-0.5">{t('proj_charter_success', { defaultValue: 'Critères de succès' })}</p>
          <p className="text-text-primary whitespace-pre-wrap">
            {rev.snapshot.success_criteria?.trim() || t('proj_charter_empty_field', { defaultValue: '—' })}
          </p>
        </div>
      </div>
    ),
  }))

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3">
          <ScrollText size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">{t('proj_charter_title', { defaultValue: 'Charte du projet' })}</h1>
          <Badge variant={approved ? 'success' : 'neutral'}>
            {approved
              ? t('proj_charter_status_approved', { defaultValue: 'Approuvée' })
              : t('proj_charter_status_draft', { defaultValue: 'Brouillon' })}
          </Badge>
        </div>

        {/* State banner — the one thing that changes what this page lets you do. */}
        <Callout variant={approved ? 'success' : 'info'} icon={approved ? <Lock size={16} /> : <ScrollText size={16} />}
          title={approved
            ? t('proj_charter_locked_title', { defaultValue: 'Charte approuvée — lecture seule' })
            : t('proj_charter_draft_title', { defaultValue: 'Brouillon' })}>
          <div className="flex items-center gap-3 flex-wrap">
            <span className="text-sm">
              {approved
                ? (approvedByName
                    ? t('proj_charter_approved_by', { defaultValue: 'Approuvée par {{name}} le {{date}}.', name: approvedByName, date: charter.approved_at ? fmtDate(charter.approved_at) : '—' })
                    : t('proj_charter_approved_on', { defaultValue: 'Approuvée le {{date}}.', date: charter.approved_at ? fmtDate(charter.approved_at) : '—' }))
                : t('proj_charter_draft_hint', { defaultValue: 'Les modifications sont enregistrées au fil de la saisie. Une fois approuvée, la charte ne sera plus modifiable.' })}
            </span>
            <div className="flex-1" />
            {revisionCount > 0 && (
              <Button size="sm" variant="secondary" icon={<History size={14} />} onClick={() => setHistoryOpen(true)}>
                {t('proj_charter_history', { defaultValue: 'Historique ({{n}})', n: revisionCount })}
              </Button>
            )}
            {isOwner && canEdit && (approved ? (
              <Button size="sm" variant="secondary" loading={reviseMut.isPending} onClick={() => { void askRevise() }}>
                {t('proj_charter_reopen_action', { defaultValue: 'Rouvrir pour modifier' })}
              </Button>
            ) : (
              <Button size="sm" icon={<CheckCircle2 size={14} />} loading={approveMut.isPending} onClick={() => { void askApprove() }}>
                {t('proj_charter_approve_action', { defaultValue: 'Approuver la charte' })}
              </Button>
            ))}
          </div>
        </Callout>

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)}>{errorMsg}</Callout>
        )}

        {/* Two columns on a wide screen: a charter is prose, and a 1500-pixel line
            is unreadable — but capping the page and leaving the rest of the window
            empty is not the answer either. The width is used, the measure is kept. */}
        {/* Multi-column flow rather than a grid: a grid makes every row as tall as
            its tallest card, so a short section leaves a hole beside a long one.
            The columns pack instead, and `break-inside` keeps a card whole. */}
        <div className={isMobile ? 'space-y-4' : 'columns-2 gap-4 [&>*]:break-inside-avoid [&>*]:mb-4'}>
          {/* ── Identité ── */}
          <Section title={t('proj_charter_identity', { defaultValue: 'Identité' })}>
            <div className={isMobile ? 'grid grid-cols-1 gap-3' : 'grid grid-cols-2 gap-3'}>
              <ShortField label={t('proj_charter_sponsor', { defaultValue: 'Sponsor' })} value={charter.sponsor} disabled={readOnly}
                placeholder={t('proj_charter_sponsor_ph', { defaultValue: 'Qui finance et arbitre' })}
                onCommit={v => set({ sponsor: v })} />
              <ShortField label={t('proj_charter_pm', { defaultValue: 'Chef de projet' })} value={charter.pm_name} disabled={readOnly}
                placeholder={t('proj_charter_pm_ph', { defaultValue: 'Qui conduit le projet' })}
                onCommit={v => set({ pm_name: v })} />
            </div>
          </Section>

          {/* ── Objet et justification ── */}
          <Section title={t('proj_charter_purpose_section', { defaultValue: 'Objet et justification' })}>
            <LongField label={t('proj_charter_purpose', { defaultValue: 'Objet du projet' })} value={charter.purpose} rows={3} disabled={readOnly}
              hint={t('proj_charter_purpose_hint', { defaultValue: 'En une phrase : pourquoi ce projet existe.' })}
              onCommit={v => set({ purpose: v })} />
            <LongField label={t('proj_charter_business_case', { defaultValue: 'Justification' })} value={charter.business_case} rows={3} disabled={readOnly}
              onCommit={v => set({ business_case: v })} />
          </Section>

          {/* ── Objectifs et critères de succès ── */}
          <Section title={t('proj_charter_objectives_section', { defaultValue: 'Objectifs et critères de succès' })}>
            <LongField label={t('proj_charter_objectives', { defaultValue: 'Objectifs' })} value={charter.objectives} rows={3} disabled={readOnly}
              onCommit={v => set({ objectives: v })} />
            <LongField label={t('proj_charter_success', { defaultValue: 'Critères de succès' })} value={charter.success_criteria} rows={3} disabled={readOnly}
              hint={t('proj_charter_success_hint', { defaultValue: 'Ce qui permettra de dire, à la fin, que c’est réussi.' })}
              onCommit={v => set({ success_criteria: v })} />
          </Section>

          {/* ── Exigences de haut niveau ── */}
          <Section title={t('proj_charter_requirements_section', { defaultValue: 'Exigences de haut niveau' })}>
            <LongField label={t('proj_charter_requirements', { defaultValue: 'Exigences' })} value={charter.high_level_requirements} rows={4} disabled={readOnly}
              onCommit={v => set({ high_level_requirements: v })} />
          </Section>

          {/* ── Hypothèses et contraintes ── */}
          <Section title={t('proj_charter_assumptions_section', { defaultValue: 'Hypothèses et contraintes' })}>
            <LongField label={t('proj_charter_assumptions', { defaultValue: 'Hypothèses' })} value={charter.assumptions} rows={3} disabled={readOnly}
              onCommit={v => set({ assumptions: v })} />
            <LongField label={t('proj_charter_constraints', { defaultValue: 'Contraintes' })} value={charter.constraints} rows={3} disabled={readOnly}
              onCommit={v => set({ constraints: v })} />
          </Section>

          {/* ── Risques majeurs / Budget ── */}
          <Section title={t('proj_charter_risks_section', { defaultValue: 'Risques majeurs et budget' })}>
            <LongField label={t('proj_charter_risks', { defaultValue: 'Risques majeurs' })} value={charter.risks_summary} rows={3} disabled={readOnly}
              onCommit={v => set({ risks_summary: v })} />
            <LongField label={t('proj_charter_budget', { defaultValue: 'Budget' })} value={charter.budget_summary} rows={2} disabled={readOnly}
              onCommit={v => set({ budget_summary: v })} />
          </Section>

          {/* ── Niveau d'autorité du chef de projet ── */}
          <Section title={t('proj_charter_authority_section', { defaultValue: 'Niveau d’autorité du chef de projet' })}>
            <LongField label={t('proj_charter_authority', { defaultValue: 'Autorité déléguée' })} value={charter.pm_authority} rows={3} disabled={readOnly}
              hint={t('proj_charter_authority_hint', { defaultValue: 'Ce que le chef de projet peut décider seul : budget, ressources, arbitrages.' })}
              onCommit={v => set({ pm_authority: v })} />
          </Section>

        </div>

        {/* ── Jalons ── */}
        <div className="bg-surface-0 border border-border rounded-xl p-5">
          <div className="flex items-center gap-3 mb-3">
            <h2 className="text-sm font-semibold text-text-primary">{t('proj_charter_milestones', { defaultValue: 'Jalons' })}</h2>
            <div className="flex-1" />
            <Button size="sm" variant="secondary" icon={<Wand2 size={14} />} loading={generateMut.isPending}
              disabled={milestones.length === 0}
              title={t('proj_charter_generate_hint', { defaultValue: 'Crée dans le planning un jalon par jalon de la charte. Relancer met à jour ceux déjà créés.' })}
              onClick={() => generateMut.mutate()}>
              {t('proj_charter_generate', { defaultValue: 'Générer dans le planning' })}
            </Button>
          </div>

          {milestones.length === 0 ? (
            <p className="text-sm text-text-tertiary italic mb-3">
              {t('proj_charter_no_milestones', { defaultValue: 'Aucun jalon. Les jalons de la charte sont les grandes étapes engagées ; ils peuvent ensuite être reportés dans le planning.' })}
            </p>
          ) : (
            <ul className="space-y-2 mb-3">
              {milestones.map(ms => (
                <MilestoneRow key={ms.id} milestone={ms} disabled={readOnly}
                  onRename={name => updateMsMut.mutate({ mid: ms.id, data: { name } })}
                  // The server COALESCEs `target_date`, so a null never clears it:
                  // only a real date is sent, and clearing the field snaps back.
                  onSetDate={day => { if (day) updateMsMut.mutate({ mid: ms.id, data: { target_date: day } }) }}
                  onDelete={() => deleteMsMut.mutate(ms.id)} />
              ))}
            </ul>
          )}

          {genResult && <p className="text-xs text-text-secondary mb-3">{genResult}</p>}

          {!readOnly && (
            <div className="flex items-center gap-2">
              <Input
                className="flex-1 min-w-0"
                value={newMs}
                placeholder={t('proj_charter_ms_new', { defaultValue: 'Nom du jalon…' })}
                onChange={e => setNewMs(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); addMilestone() } }}
              />
              <Button size="sm" icon={<Plus size={14} />} loading={addMsMut.isPending} disabled={!newMs.trim()} onClick={addMilestone}>
                {t('proj_charter_ms_add', { defaultValue: 'Ajouter' })}
              </Button>
            </div>
          )}
        </div>
      </div>

      {historyOpen && (
        <FloatingWindow
          title={t('proj_charter_history_title', { defaultValue: 'Révisions de la charte' })}
          icon={<History size={16} />}
          onClose={() => setHistoryOpen(false)}
          defaultWidth={560} defaultHeight={440} padding={16} t={t}
          actions={{ cancel: { label: t('common_close', { defaultValue: 'Fermer' }) } }}
        >
          {revisionItems.length === 0 ? (
            <p className="text-sm text-text-tertiary">{t('proj_charter_no_revisions', { defaultValue: 'Aucune révision.' })}</p>
          ) : (
            <Accordion items={revisionItems} size="sm" />
          )}
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
