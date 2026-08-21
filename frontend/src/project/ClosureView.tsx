import { useEffect, useMemo, useRef, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import type { TFunction } from 'i18next'
import { getDateLocale, useAuthStore, useConfirm } from '@kubuno/sdk'
import {
  AlertTriangle, ArrowUpRight, BookOpen, CalendarCheck, CheckCircle2, ClipboardCheck,
  FileText, Handshake, KeyRound, Lightbulb, Link2, ListChecks, Lock, Plus, Scale,
  Target, ThumbsDown, ThumbsUp, Trash2, Undo2,
} from 'lucide-react'
import {
  Badge, Button, Callout, ConfirmDialog, Dropdown, EmptyState, FloatingWindow,
  Input, ProgressBar, Textarea, Tooltip, useIsMobile, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type ClosureCheck, type ClosureEdit, type Lesson, type LessonCategory,
  type LessonEdit, type LessonOutcome, type LessonStatus,
} from '../api'

// Project closure — confronting the project with what it promised before
// declaring it over, and writing down what it taught.
//
// Almost nothing on this screen is typed here: the CHECKLIST is computed from
// the registers every other artifact built (tasks, deliverables, issues, change
// requests, quality checks, risks, requirements, charter, baselines). That is
// why it leads the page — a closure form nobody confronts with the registers is
// a signature on an empty page.
//
// The checklist is split in two, because the two halves mean different things:
//   • BLOCKING  — things left UNDONE. They stop nothing absolutely (the project
//     can still be closed), but closing over them requires saying why, and the
//     reason is kept forever beside the closure.
//   • ADVISORY  — things left UNWRITTEN. The project can end without them; it
//     will simply not have taught anything.
//
// Every unmet line points at the artifact that would settle it, so the screen is
// a place to leave rather than a place to stare at.
//
// The lessons register follows the same idea: a lesson without a RECOMMENDATION
// is an anecdote — it says what happened and leaves the next project to work it
// out. Those are listed first, and the server refuses to validate one.

/** Which artifact tab each failing check sends the reader to. `no_lessons` has
 *  none: the register it complains about is on this very page. */
const CHECK_TAB: Record<string, string | undefined> = {
  tasks_open:              'gantt',
  deliverables_unaccepted: 'deliverables',
  issues_open:             'issues',
  changes_undecided:       'changes',
  quality_checks_pending:  'quality',
  risks_open:              'risks',
  requirements_unverified: 'requirements',
  charter_unapproved:      'charter',
  no_baseline:             'gantt',
  no_lessons:              undefined,
}

/** Checks whose `count` is a yes/no rather than a quantity: printing "1" beside
 *  "Aucun plan de référence" would read as one missing baseline among many. */
const BOOLEAN_CHECKS = new Set(['charter_unapproved', 'no_baseline', 'no_lessons'])

const CATEGORIES: LessonCategory[] = [
  'process', 'technical', 'people', 'supplier', 'estimation', 'communication', 'risk', 'other',
]
const OUTCOMES: LessonOutcome[] = ['positive', 'negative', 'mixed']
const LESSON_STATUSES: LessonStatus[] = ['draft', 'validated', 'shared']

const OUTCOME_VARIANT: Record<LessonOutcome, 'success' | 'danger' | 'warning'> = {
  positive: 'success',
  negative: 'danger',
  mixed:    'warning',
}

const STATUS_VARIANT: Record<LessonStatus, 'neutral' | 'success' | 'primary'> = {
  draft:     'neutral',
  validated: 'success',
  shared:    'primary',
}

/** The wording of each check, in the reader's language. Falls back to the raw key
 *  so a check added server-side still shows up rather than silently vanishing. */
function checkLabel(t: TFunction, key: string): string {
  switch (key) {
    case 'tasks_open':              return t('proj_closure_check_tasks', { defaultValue: 'Tâches non terminées' })
    case 'deliverables_unaccepted': return t('proj_closure_check_deliverables', { defaultValue: 'Livrables non acceptés' })
    case 'issues_open':             return t('proj_closure_check_issues', { defaultValue: 'Incidents ouverts' })
    case 'changes_undecided':       return t('proj_closure_check_changes', { defaultValue: 'Demandes de changement non tranchées' })
    case 'quality_checks_pending':  return t('proj_closure_check_quality', { defaultValue: 'Contrôles qualité en attente ou en échec' })
    case 'risks_open':              return t('proj_closure_check_risks', { defaultValue: 'Risques encore ouverts' })
    case 'requirements_unverified': return t('proj_closure_check_requirements', { defaultValue: 'Exigences non vérifiées' })
    case 'charter_unapproved':      return t('proj_closure_check_charter', { defaultValue: 'Charte non approuvée' })
    case 'no_baseline':             return t('proj_closure_check_baseline', { defaultValue: 'Aucun plan de référence' })
    case 'no_lessons':              return t('proj_closure_check_lessons', { defaultValue: 'Aucun enseignement consigné' })
    default:                        return key
  }
}

function categoryLabel(t: TFunction, c: LessonCategory): string {
  return {
    process:       t('proj_lesson_cat_process', { defaultValue: 'Processus' }),
    technical:     t('proj_lesson_cat_technical', { defaultValue: 'Technique' }),
    people:        t('proj_lesson_cat_people', { defaultValue: 'Équipe' }),
    supplier:      t('proj_lesson_cat_supplier', { defaultValue: 'Fournisseur' }),
    estimation:    t('proj_lesson_cat_estimation', { defaultValue: 'Estimation' }),
    communication: t('proj_lesson_cat_communication', { defaultValue: 'Communication' }),
    risk:          t('proj_lesson_cat_risk', { defaultValue: 'Risque' }),
    other:         t('proj_lesson_cat_other', { defaultValue: 'Autre' }),
  }[c]
}

function outcomeLabel(t: TFunction, o: LessonOutcome): string {
  return {
    positive: t('proj_lesson_outcome_positive', { defaultValue: 'Ce qui a marché' }),
    negative: t('proj_lesson_outcome_negative', { defaultValue: 'Ce qui a échoué' }),
    mixed:    t('proj_lesson_outcome_mixed', { defaultValue: 'Nuancé' }),
  }[o]
}

function statusLabel(t: TFunction, s: LessonStatus): string {
  return {
    draft:     t('proj_lesson_status_draft', { defaultValue: 'Brouillon' }),
    validated: t('proj_lesson_status_validated', { defaultValue: 'Validé' }),
    shared:    t('proj_lesson_status_shared', { defaultValue: 'Partagé' }),
  }[s]
}

/** One line of the checklist: a state, what it names, how many, and the way out. */
function CheckRow({ check, label, onOpenArtifact, t }: {
  check: ClosureCheck
  label: string
  onOpenArtifact?: (tab: string) => void
  t: TFunction
}) {
  const tab = CHECK_TAB[check.key]
  const countable = !BOOLEAN_CHECKS.has(check.key)
  const tone = check.ok
    ? 'var(--color-success)'
    : check.blocking ? 'var(--color-danger)' : 'var(--color-warning)'
  return (
    <li className="flex items-center gap-2 py-1.5 border-b border-border last:border-b-0">
      {check.ok
        ? <CheckCircle2 size={16} className="shrink-0" style={{ color: tone }} />
        : <AlertTriangle size={16} className="shrink-0" style={{ color: tone }} />}
      <span className={check.ok ? 'text-sm text-text-tertiary' : 'text-sm text-text-primary'}>
        {label}
      </span>
      {!check.ok && countable && (
        <span className="inline-flex">
          <Badge variant={check.blocking ? 'danger' : 'warning'} size="sm">{check.count}</Badge>
        </span>
      )}
      <div className="flex-1" />
      {check.ok ? (
        <span className="text-xs text-text-tertiary shrink-0">
          {t('proj_closure_check_ok', { defaultValue: 'réglé' })}
        </span>
      ) : tab && onOpenArtifact ? (
        <Button size="sm" variant="text" icon={<ArrowUpRight size={14} />}
          onClick={() => onOpenArtifact(tab)}>
          {t('proj_closure_check_open', { defaultValue: 'Ouvrir' })}
        </Button>
      ) : null}
    </li>
  )
}

/** One half of the checklist. The two halves are titled by what they MEAN, not
 *  by their severity flag: undone versus unwritten. */
function CheckPanel({ title, intro, icon, checks, onOpenArtifact, t }: {
  title: string
  intro: string
  icon: React.ReactNode
  checks: ClosureCheck[]
  onOpenArtifact?: (tab: string) => void
  t: TFunction
}) {
  const settled = checks.filter(c => c.ok).length
  // Unmet first: the satisfied lines are kept visible (they say what was
  // verified) but they are not what the reader came for.
  const ordered = [...checks].sort((a, b) => Number(a.ok) - Number(b.ok))
  return (
    <div className="flex-1 min-w-0 rounded-lg border border-border bg-surface-1 p-4">
      <div className="flex items-center gap-2 mb-1">
        {icon}
        <h3 className="text-sm font-semibold text-text-primary">{title}</h3>
        <div className="flex-1" />
        <span className="text-xs text-text-tertiary shrink-0">
          {t('proj_closure_panel_count', { defaultValue: '{{done}}/{{total}}', done: settled, total: checks.length })}
        </span>
      </div>
      <p className="text-xs text-text-secondary mb-2 leading-relaxed">{intro}</p>
      <ul>
        {ordered.map(c => (
          <CheckRow key={c.key} check={c} label={checkLabel(t, c.key)}
            onOpenArtifact={onOpenArtifact} t={t} />
        ))}
      </ul>
    </div>
  )
}

/** A titled block of the closure dossier. */
function Section({ title, icon, children }: {
  title: string
  icon?: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <div className="bg-surface-0 border border-border rounded-xl p-5">
      <h2 className="flex items-center gap-2 text-sm font-semibold text-text-primary mb-3">
        {icon}
        <span>{title}</span>
      </h2>
      <div className="space-y-3">{children}</div>
    </div>
  )
}

/** A long field of the dossier: prose when read-only, a textarea committed on
 *  blur when editable (one PUT per edit, not per keystroke). */
function DossierField({ label, hint, value, placeholder, rows = 4, readOnly, missing, missingLabel, onCommit }: {
  label: string
  hint?: string
  value: string
  placeholder?: string
  rows?: number
  readOnly?: boolean
  /** The absence is itself the news — say it in words rather than leave a dash. */
  missing?: boolean
  missingLabel?: string
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  // The server echo is the source of truth: follow it whenever it moves.
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="text-sm text-text-secondary mb-1 block">{label}</label>
      {readOnly ? (
        value.trim()
          ? <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">{value}</p>
          : <p className="text-sm italic text-text-tertiary">—</p>
      ) : (
        <Textarea
          rows={rows}
          className="h-auto min-h-0 resize-y"
          value={draft}
          placeholder={placeholder}
          onChange={e => setDraft(e.target.value)}
          onBlur={() => { if (draft !== value) onCommit(draft) }}
        />
      )}
      {missing && missingLabel && (
        <p className="text-xs mt-1" style={{ color: 'var(--color-danger)' }}>{missingLabel}</p>
      )}
      {hint && !missing && <p className="text-xs text-text-tertiary mt-1">{hint}</p>}
    </div>
  )
}

/** One number of the lessons strip. */
function Stat({ label, value, tone, icon }: {
  label: string
  value: number
  tone?: 'danger' | 'success' | 'muted'
  icon?: React.ReactNode
}) {
  const color = tone === 'danger' ? 'var(--color-danger)'
    : tone === 'success' ? 'var(--color-success)'
      : tone === 'muted' ? 'var(--color-text-secondary)'
        : 'var(--color-text-primary)'
  return (
    <div className="flex-1 min-w-[8rem] rounded-lg bg-surface-1 border border-border px-3 py-2">
      <div className="flex items-center gap-1.5 text-sm text-text-secondary mb-0.5">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <p className="text-xl font-semibold" style={{ color }}>{value}</p>
    </div>
  )
}

/** Everything a lesson card needs from the screen. Passed as one object so the
 *  card lives at module level: defined inside the parent it would be a new
 *  component type on every render, remounting the list — and dropping the edit
 *  in progress — each time a query settles. */
interface LessonCtx {
  t: TFunction
  canEdit: boolean
  isMobile: boolean
  patch: (id: string, body: LessonEdit) => void
  askDelete: (l: Lesson) => void
  fmtDate: (iso: string) => string
  nameOf: (userId: string | null) => string | null
  categoryOptions: DropdownOption[]
  outcomeOptions: DropdownOption[]
  statusOptions: DropdownOption[]
  taskOptions: DropdownOption[]
  riskOptions: DropdownOption[]
  issueOptions: DropdownOption[]
  changeOptions: DropdownOption[]
  registerCard: (id: string, el: HTMLDivElement | null) => void
  /** Briefly ringed after a jump, so the eye lands on the right card. */
  flashId: string | null
  onOpenArtifact?: (tab: string) => void
  /** Anticipates the server's refusal instead of letting it come back as a 400. */
  refuse: (message: string) => void
}

/** One free-text block of a lesson, committed on blur. */
function LessonField({ label, icon, value, placeholder, rows = 2, disabled, onCommit }: {
  label: string
  icon?: React.ReactNode
  value: string
  placeholder?: string
  rows?: number
  disabled?: boolean
  onCommit: (next: string) => void
}) {
  const [draft, setDraft] = useState(value)
  useEffect(() => { setDraft(value) }, [value])
  return (
    <div>
      <label className="flex items-center gap-1.5 text-sm font-medium text-text-secondary mb-1">
        {icon}
        <span>{label}</span>
      </label>
      {disabled ? (
        value.trim()
          ? <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">{value}</p>
          : <p className="text-sm italic text-text-tertiary">—</p>
      ) : (
        <Textarea
          rows={rows}
          className="h-auto min-h-0 resize-y"
          value={draft}
          placeholder={placeholder}
          onChange={e => setDraft(e.target.value)}
          onBlur={() => { if (draft !== value) onCommit(draft) }}
        />
      )}
    </div>
  )
}

/** One attachment of a lesson: the register entry it came from. Editable as a
 *  dropdown, and always openable — the point of the link is to be followed. */
function Attachment({ label, options, value, display, tab, ctx, onChange }: {
  label: string
  options: DropdownOption[]
  value: string | null
  /** What the link reads as when there is nothing to pick from. */
  display: string | null
  tab: string
  ctx: LessonCtx
  onChange: (next: string | null) => void
}) {
  const { t, canEdit, onOpenArtifact } = ctx
  return (
    <div className="min-w-0">
      <label className="text-xs text-text-tertiary mb-1 block">{label}</label>
      <div className="flex items-center gap-1 min-w-0">
        {canEdit ? (
          <Dropdown
            className="flex-1 min-w-0" height={36} fontSize={14} focusable
            value={value ?? ''} options={options}
            onChange={v => onChange(v || null)}
          />
        ) : (
          <span className="flex-1 min-w-0 text-sm text-text-primary truncate">
            {display ?? '—'}
          </span>
        )}
        {value && onOpenArtifact && (
          <Tooltip label={t('proj_lesson_open_link', { defaultValue: 'Ouvrir dans le registre' })}>
            <button
              type="button"
              onClick={() => onOpenArtifact(tab)}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-text-primary"
            >
              <ArrowUpRight size={15} />
            </button>
          </Tooltip>
        )}
      </div>
    </div>
  )
}

/** One lesson. The recommendation is set apart on purpose: it is the only part
 *  that travels to the next project. */
function LessonCard({ lesson, ctx }: { lesson: Lesson; ctx: LessonCtx }) {
  const { t, canEdit, isMobile } = ctx
  const [titleDraft, setTitleDraft] = useState(lesson.title)
  useEffect(() => { setTitleDraft(lesson.title) }, [lesson.title])

  const hasReco = lesson.recommendation.trim().length > 0
  const recorder = ctx.nameOf(lesson.recorded_by)

  return (
    <div
      ref={el => ctx.registerCard(lesson.id, el)}
      className="bg-surface-0 border border-border rounded-xl p-5 space-y-3"
      style={{ boxShadow: ctx.flashId === lesson.id ? '0 0 0 2px var(--color-primary)' : undefined }}
    >
      <div className="flex items-center gap-2 flex-wrap">
        <span className="font-mono text-xs text-text-tertiary shrink-0">{lesson.code}</span>
        {canEdit ? (
          <Input
            className="flex-1 min-w-[12rem]"
            value={titleDraft}
            onChange={e => setTitleDraft(e.target.value)}
            onBlur={() => {
              const next = titleDraft.trim()
              if (next && next !== lesson.title) ctx.patch(lesson.id, { title: next })
              else if (!next) setTitleDraft(lesson.title)
            }}
          />
        ) : (
          <h3 className="flex-1 min-w-0 text-sm font-semibold text-text-primary">{lesson.title}</h3>
        )}
        <span className="inline-flex">
          <Badge variant={OUTCOME_VARIANT[lesson.outcome]} size="sm">
            {outcomeLabel(t, lesson.outcome)}
          </Badge>
        </span>
        <span className="inline-flex">
          <Badge variant={STATUS_VARIANT[lesson.status]} size="sm">
            {statusLabel(t, lesson.status)}
          </Badge>
        </span>
        {canEdit && (
          <Tooltip label={t('common_delete', { defaultValue: 'Supprimer' })}>
            <button
              type="button"
              onClick={() => ctx.askDelete(lesson)}
              className="shrink-0 p-1.5 rounded hover:bg-surface-2 text-text-tertiary hover:text-danger"
            >
              <Trash2 size={15} />
            </button>
          </Tooltip>
        )}
      </div>

      <div className={isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-4'}>
        <LessonField
          label={t('proj_lesson_situation', { defaultValue: 'Situation' })}
          value={lesson.situation}
          placeholder={t('proj_lesson_situation_ph', { defaultValue: 'Le contexte dans lequel cela s’est produit…' })}
          disabled={!canEdit}
          onCommit={v => ctx.patch(lesson.id, { situation: v })}
        />
        <LessonField
          label={t('proj_lesson_what_happened', { defaultValue: 'Ce qui s’est passé' })}
          value={lesson.what_happened}
          placeholder={t('proj_lesson_what_happened_ph', { defaultValue: 'Les faits, sans la morale…' })}
          disabled={!canEdit}
          onCommit={v => ctx.patch(lesson.id, { what_happened: v })}
        />
      </div>

      {/* The recommendation carries its own frame: it is the half that is read
          by someone who never worked on this project. */}
      <div
        className="rounded-lg border p-3"
        style={{
          borderColor: hasReco ? 'var(--color-primary)' : 'var(--color-warning)',
          background: 'var(--color-surface-1)',
        }}
      >
        <LessonField
          label={t('proj_lesson_recommendation', { defaultValue: 'Recommandation' })}
          icon={<Lightbulb size={14} style={{ color: hasReco ? 'var(--color-primary)' : 'var(--color-warning)' }} />}
          rows={3}
          value={lesson.recommendation}
          placeholder={t('proj_lesson_recommendation_ph', { defaultValue: 'Ce que le prochain projet devrait faire — ou éviter…' })}
          disabled={!canEdit}
          onCommit={v => ctx.patch(lesson.id, { recommendation: v })}
        />
        {!hasReco && (
          <p className="text-xs mt-1" style={{ color: 'var(--color-warning)' }}>
            {t('proj_lesson_no_reco_inline', {
              defaultValue: 'Sans recommandation, cet enseignement reste une anecdote : il dit ce qui s’est passé et laisse le projet suivant se débrouiller.',
            })}
          </p>
        )}
      </div>

      {canEdit && (
        <div className={isMobile ? 'grid grid-cols-1 gap-3' : 'grid grid-cols-3 gap-3'}>
          <div className="min-w-0">
            <label className="text-xs text-text-tertiary mb-1 block">
              {t('proj_lesson_category', { defaultValue: 'Catégorie' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={lesson.category} options={ctx.categoryOptions}
              onChange={v => ctx.patch(lesson.id, { category: v as LessonCategory })}
            />
          </div>
          <div className="min-w-0">
            <label className="text-xs text-text-tertiary mb-1 block">
              {t('proj_lesson_outcome', { defaultValue: 'Nature' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={lesson.outcome} options={ctx.outcomeOptions}
              onChange={v => ctx.patch(lesson.id, { outcome: v as LessonOutcome })}
            />
          </div>
          <div className="min-w-0">
            <label className="text-xs text-text-tertiary mb-1 block">
              {t('proj_lesson_status', { defaultValue: 'Statut' })}
            </label>
            <Dropdown
              className="w-full" width="100%" height={36} fontSize={14} focusable
              value={lesson.status} options={ctx.statusOptions}
              onChange={v => {
                // The server refuses this exact move; saying so here costs one
                // sentence and keeps the field where the fix belongs.
                if (v !== 'draft' && !hasReco) {
                  ctx.refuse(t('proj_lesson_validate_refused', {
                    defaultValue: 'Indiquez la recommandation avant de valider : c’est la seule partie qui serve au projet suivant.',
                  }))
                  return
                }
                ctx.patch(lesson.id, { status: v as LessonStatus })
              }}
            />
          </div>
        </div>
      )}

      <div className={isMobile ? 'grid grid-cols-1 gap-3' : 'grid grid-cols-4 gap-3'}>
        <Attachment
          label={t('proj_lesson_link_task', { defaultValue: 'Lot de travail' })}
          options={ctx.taskOptions} value={lesson.task_id} display={lesson.task_name}
          tab="gantt" ctx={ctx}
          onChange={v => ctx.patch(lesson.id, { task_id: v })}
        />
        <Attachment
          label={t('proj_lesson_link_risk', { defaultValue: 'Risque' })}
          options={ctx.riskOptions} value={lesson.risk_id} display={lesson.risk_code}
          tab="risks" ctx={ctx}
          onChange={v => ctx.patch(lesson.id, { risk_id: v })}
        />
        <Attachment
          label={t('proj_lesson_link_issue', { defaultValue: 'Incident' })}
          options={ctx.issueOptions} value={lesson.issue_id} display={lesson.issue_code}
          tab="issues" ctx={ctx}
          onChange={v => ctx.patch(lesson.id, { issue_id: v })}
        />
        <Attachment
          label={t('proj_lesson_link_change', { defaultValue: 'Changement' })}
          options={ctx.changeOptions} value={lesson.change_id} display={lesson.change_code}
          tab="changes" ctx={ctx}
          onChange={v => ctx.patch(lesson.id, { change_id: v })}
        />
      </div>

      <p className="text-xs text-text-tertiary">
        {recorder
          ? t('proj_lesson_recorded_by', {
              defaultValue: 'Consigné le {{date}} par {{name}}',
              date: ctx.fmtDate(lesson.recorded_on), name: recorder,
            })
          : t('proj_lesson_recorded_on', {
              defaultValue: 'Consigné le {{date}}', date: ctx.fmtDate(lesson.recorded_on),
            })}
      </p>
    </div>
  )
}

export default function ClosureView({ projectId, canEdit = true, isOwner = false, onOpenArtifact }: {
  projectId: string
  /** False in the mobile reading mode, where the project is shown, not edited. */
  canEdit?: boolean
  /** Closing and reopening a project are the owner's calls (the server agrees). */
  isOwner?: boolean
  /** Jumps to the artifact that would settle an unmet check, or a lesson's link. */
  onOpenArtifact?: (tab: string) => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  // Responsive layout is driven in JS: a module's `sm:` variant loses to the
  // host's base utility (cascade layer `kubuno-module` sits below `utilities`).
  const isMobile = useIsMobile()
  const me = useAuthStore(s => s.user)

  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [closeOpen, setCloseOpen] = useState(false)
  const [overrideReason, setOverrideReason] = useState('')
  const [closedOn, setClosedOn] = useState(() => format(new Date(), 'yyyy-MM-dd'))
  const [createOpen, setCreateOpen] = useState(false)
  const [outcomeFilter, setOutcomeFilter] = useState('')
  const [flashId, setFlashId] = useState<string | null>(null)
  const [draft, setDraft] = useState<{
    title: string; situation: string; what_happened: string
    recommendation: string; category: LessonCategory; outcome: LessonOutcome
  }>({ title: '', situation: '', what_happened: '', recommendation: '', category: 'process', outcome: 'positive' })

  const cardRefs = useRef(new Map<string, HTMLDivElement>())
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => { if (flashTimer.current) clearTimeout(flashTimer.current) }, [])

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['closure', projectId],
    queryFn: () => projectsApi.getClosure(projectId),
  })
  const { data: lessonLog } = useQuery({
    queryKey: ['lessons', projectId],
    queryFn: () => projectsApi.listLessons(projectId),
  })
  // The registers a lesson can be attached to. Shared cache keys, so opening this
  // screen after the risk register costs nothing.
  const { data: wbs } = useQuery({
    queryKey: ['wbs', projectId],
    queryFn: () => projectsApi.getWbs(projectId),
  })
  const { data: risks } = useQuery({
    queryKey: ['risks', projectId],
    queryFn: () => projectsApi.getRisks(projectId),
  })
  const { data: issues } = useQuery({
    queryKey: ['issues', projectId],
    queryFn: () => projectsApi.getIssues(projectId),
  })
  const { data: changes } = useQuery({
    queryKey: ['changes', projectId],
    queryFn: () => projectsApi.getChanges(projectId),
  })
  // The closure only records WHO as an identifier; the names live with the
  // project's people. Same cache key as the sharing panel.
  const { data: people } = useQuery({
    queryKey: ['proj-collab', projectId],
    queryFn: () => projectsApi.listCollaborators(projectId),
  })

  const invalClosure = () => qc.invalidateQueries({ queryKey: ['closure', projectId] })
  const invalLessons = () => {
    qc.invalidateQueries({ queryKey: ['lessons', projectId] })
    // `no_lessons` is one of the checks: the checklist has just changed.
    invalClosure()
  }
  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here — and the refusals of this screen
  // are precisely what the server has to say.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))

  const updateMut = useMutation({
    mutationFn: (patch: ClosureEdit) => projectsApi.updateClosure(projectId, patch),
    onSuccess: () => { setErrorMsg(null); invalClosure() },
    onError: fail,
  })
  const closeMut = useMutation({
    mutationFn: (body: { override_reason?: string; closed_on?: string }) =>
      projectsApi.closeProject(projectId, body),
    onSuccess: () => {
      setErrorMsg(null)
      setCloseOpen(false)
      setOverrideReason('')
      invalClosure()
      // The project's own status followed the closure server-side.
      qc.invalidateQueries({ queryKey: ['project', projectId] })
    },
    onError: fail,
  })
  const reopenMut = useMutation({
    mutationFn: () => projectsApi.reopenProject(projectId),
    onSuccess: () => {
      setErrorMsg(null)
      invalClosure()
      qc.invalidateQueries({ queryKey: ['project', projectId] })
    },
    onError: fail,
  })
  const createLessonMut = useMutation({
    mutationFn: (payload: LessonEdit & { title: string }) => projectsApi.createLesson(projectId, payload),
    onSuccess: lesson => {
      setErrorMsg(null)
      setCreateOpen(false)
      setDraft({ title: '', situation: '', what_happened: '', recommendation: '', category: 'process', outcome: 'positive' })
      invalLessons()
      focusLesson(lesson.id)
    },
    onError: fail,
  })
  const updateLessonMut = useMutation({
    mutationFn: ({ id, body }: { id: string; body: LessonEdit }) =>
      projectsApi.updateLesson(projectId, id, body),
    onSuccess: () => { setErrorMsg(null); invalLessons() },
    onError: fail,
  })
  const deleteLessonMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteLesson(projectId, id),
    onSuccess: () => { setErrorMsg(null); invalLessons() },
    onError: fail,
  })

  const closure = data?.closure
  const checks = useMemo(() => data?.checks ?? [], [data])
  const summary = data?.summary
  const closed = summary?.closed ?? false
  const blockingChecks = useMemo(() => checks.filter(c => c.blocking), [checks])
  const advisoryChecks = useMemo(() => checks.filter(c => !c.blocking), [checks])
  const remainingBlockers = useMemo(() => blockingChecks.filter(c => !c.ok), [blockingChecks])
  const remainingAdvisory = useMemo(() => advisoryChecks.filter(c => !c.ok), [advisoryChecks])
  const settled = checks.filter(c => c.ok).length
  // The dossier is frozen once closed: the server refuses every write until the
  // project is reopened, so the fields say so rather than bouncing.
  const readOnly = closed || !canEdit
  const objectivesAnswered = (closure?.objectives_met ?? '').trim().length > 0

  const lessons = useMemo(() => lessonLog?.lessons ?? [], [lessonLog])
  const lessonSummary = lessonLog?.summary
  const withoutReco = lessonLog?.without_recommendation ?? []

  const nameOf = useMemo(() => {
    const map = new Map<string, string>()
    const owner = people?.owner
    if (owner) map.set(owner.id, owner.display_name?.trim() || owner.email)
    for (const c of people?.collaborators ?? []) {
      map.set(c.user_id, c.display_name?.trim() || c.email)
    }
    return (userId: string | null): string | null => {
      if (!userId) return null
      const known = map.get(userId)
      if (known) return known
      return userId === me?.id ? t('proj_closure_you', { defaultValue: 'vous' }) : null
    }
  }, [people, me, t])

  const fmtDate = (iso: string) => format(parseISO(iso), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })

  const categoryOptions: DropdownOption[] = useMemo(
    () => CATEGORIES.map(c => ({ value: c, label: categoryLabel(t, c) })), [t])
  const outcomeOptions: DropdownOption[] = useMemo(
    () => OUTCOMES.map(o => ({ value: o, label: outcomeLabel(t, o) })), [t])
  const lessonStatusOptions: DropdownOption[] = useMemo(
    () => LESSON_STATUSES.map(s => ({ value: s, label: statusLabel(t, s) })), [t])
  const outcomeFilterOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_lesson_filter_all', { defaultValue: 'Toutes natures' }) },
    ...outcomeOptions,
  ], [outcomeOptions, t])

  const taskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_lesson_no_task', { defaultValue: '— Aucun lot —' }) },
    ...(wbs ?? []).map(el => ({ value: el.id, label: `${el.wbs} ${el.name}`.trim() })),
  ], [wbs, t])
  const riskOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_lesson_no_risk', { defaultValue: '— Aucun risque —' }) },
    ...(risks?.risks ?? []).map(r => ({ value: r.id, label: `${r.code} ${r.title}`.trim() })),
  ], [risks, t])
  const issueOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_lesson_no_issue', { defaultValue: '— Aucun incident —' }) },
    ...(issues?.issues ?? []).map(i => ({ value: i.id, label: `${i.code} ${i.title}`.trim() })),
  ], [issues, t])
  const changeOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_lesson_no_change', { defaultValue: '— Aucun changement —' }) },
    ...(changes?.changes ?? []).map(c => ({ value: c.id, label: `${c.code} ${c.title}`.trim() })),
  ], [changes, t])

  const registerCard = (id: string, el: HTMLDivElement | null) => {
    if (el) cardRefs.current.set(id, el); else cardRefs.current.delete(id)
  }

  /** Jumps to a lesson and rings it briefly — a silent scroll leaves the reader
   *  hunting for what just moved. */
  const focusLesson = (id: string) => {
    // Clearing the filter first: the target may well be hidden by it, and
    // jumping to nothing is worse than not jumping.
    setOutcomeFilter('')
    setFlashId(id)
    if (flashTimer.current) clearTimeout(flashTimer.current)
    flashTimer.current = setTimeout(() => setFlashId(null), 2200)
    requestAnimationFrame(() => {
      cardRefs.current.get(id)?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    })
  }

  const askDelete = (l: Lesson) => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_lesson_delete_title', { defaultValue: 'Supprimer l’enseignement ?' }),
        message: t('proj_lesson_delete_msg', {
          defaultValue: '« {{title}} » disparaîtra du registre, avec sa recommandation. C’est ce que le projet suivant aurait lu.',
          title: l.title,
        }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      })
      if (ok) deleteLessonMut.mutate(l.id)
    })()
  }

  const askReopen = () => {
    void (async () => {
      const ok = await confirm({
        title: t('proj_closure_reopen_title', { defaultValue: 'Rouvrir le projet ?' }),
        message: t('proj_closure_reopen_msg', {
          defaultValue: 'Le projet redeviendra actif et son dossier de clôture modifiable. La date de clôture sera effacée ; le motif de dérogation, lui, reste consigné.',
        }),
        confirmLabel: t('proj_closure_reopen', { defaultValue: 'Rouvrir' }),
      })
      if (ok) reopenMut.mutate()
    })()
  }

  const openCloseWindow = () => {
    setOverrideReason(closure?.override_reason ?? '')
    setClosedOn(format(new Date(), 'yyyy-MM-dd'))
    setCloseOpen(true)
  }

  const canSubmitClose = objectivesAnswered
    && (remainingBlockers.length === 0 || overrideReason.trim().length > 0)

  const submitClose = () => {
    if (!canSubmitClose) return
    closeMut.mutate({
      override_reason: overrideReason.trim() || undefined,
      closed_on: closedOn || undefined,
    })
  }

  const canCreateLesson = draft.title.trim().length > 0

  const submitCreateLesson = () => {
    const title = draft.title.trim()
    if (!title) return
    void (async () => {
      if (!draft.recommendation.trim()) {
        const ok = await confirm({
          title: t('proj_lesson_warn_title', { defaultValue: 'Enregistrer sans recommandation ?' }),
          message: t('proj_lesson_warn_msg', {
            defaultValue: 'L’enseignement sera consigné, mais sans ce qu’il faudrait en faire : une anecdote de plus, que le projet suivant ne pourra pas utiliser. Il ne pourra pas non plus être validé tant que la recommandation manquera.',
          }),
          confirmLabel: t('proj_lesson_warn_confirm', { defaultValue: 'Enregistrer quand même' }),
          cancelLabel: t('proj_lesson_warn_cancel', { defaultValue: 'Compléter d’abord' }),
          variant: 'warning',
        })
        if (!ok) return
      }
      createLessonMut.mutate({
        title,
        situation: draft.situation.trim(),
        what_happened: draft.what_happened.trim(),
        recommendation: draft.recommendation.trim(),
        category: draft.category,
        outcome: draft.outcome,
      })
    })()
  }

  const ctx: LessonCtx = {
    t, canEdit, isMobile, fmtDate, nameOf,
    patch: (id, body) => updateLessonMut.mutate({ id, body }),
    askDelete,
    categoryOptions, outcomeOptions, statusOptions: lessonStatusOptions,
    taskOptions, riskOptions, issueOptions, changeOptions,
    registerCard, flashId, onOpenArtifact,
    refuse: setErrorMsg,
  }

  const visibleLessons = outcomeFilter
    ? lessons.filter(l => l.outcome === outcomeFilter)
    : lessons

  if (isError) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="error"
              icon={<ClipboardCheck size={26} />}
              title={t('proj_closure_load_error', { defaultValue: 'La clôture n’a pas pu être chargée.' })}
              action={{ label: t('common_retry', { defaultValue: 'Réessayer' }), onClick: () => { void refetch() } }}
              t={t}
            />
          </div>
        </div>
      </div>
    )
  }

  if (isLoading || !data || !closure) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="p-6">
          <div className="bg-surface-0 border border-border rounded-xl p-8 text-center text-sm text-text-secondary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        </div>
      </div>
    )
  }

  const closerName = nameOf(closure.closed_by)

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="p-6 space-y-4">

        <div className="flex items-center gap-3 flex-wrap">
          <ClipboardCheck size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_closure_title', { defaultValue: 'Clôture du projet' })}
          </h1>
          <span className="inline-flex">
            <Badge variant={closed ? 'success' : summary?.ready ? 'primary' : 'warning'}>
              {closed
                ? t('proj_closure_status_closed', { defaultValue: 'Clos' })
                : summary?.ready
                  ? t('proj_closure_status_ready', { defaultValue: 'Prêt à clore' })
                  : t('proj_closure_status_open', { defaultValue: 'En cours' })}
            </Badge>
          </span>
        </div>

        {/* The state banner — the one thing that changes what this page lets you do. */}
        {closed ? (
          <Callout
            variant="success"
            icon={<Lock size={16} />}
            title={closure.closed_on
              ? t('proj_closure_closed_on', { defaultValue: 'Projet clos le {{date}}', date: fmtDate(closure.closed_on) })
              : t('proj_closure_closed', { defaultValue: 'Projet clos' })}
          >
            <div className="space-y-2">
              <div className="flex items-center gap-3 flex-wrap">
                <span className="text-sm">
                  {closerName
                    ? t('proj_closure_closed_by', { defaultValue: 'Clos par {{name}}. Le dossier est en lecture seule.', name: closerName })
                    : t('proj_closure_closed_ro', { defaultValue: 'Le dossier de clôture est en lecture seule.' })}
                </span>
                <div className="flex-1" />
                {isOwner && canEdit && (
                  <Button size="sm" variant="secondary" icon={<Undo2 size={14} />}
                    loading={reopenMut.isPending} onClick={askReopen}>
                    {t('proj_closure_reopen_action', { defaultValue: 'Rouvrir le projet' })}
                  </Button>
                )}
              </div>
              {closure.override_reason.trim() && (
                // The trace of what was knowingly left open. It outlives the
                // project precisely because nobody would remember it otherwise.
                <div className="rounded-lg border p-3" style={{ borderColor: 'var(--color-warning)' }}>
                  <p className="flex items-center gap-1.5 text-xs font-medium mb-1" style={{ color: 'var(--color-warning)' }}>
                    <AlertTriangle size={13} />
                    {t('proj_closure_override_title', { defaultValue: 'Clos malgré des points ouverts — motif' })}
                  </p>
                  <p className="text-sm text-text-primary whitespace-pre-wrap">{closure.override_reason}</p>
                </div>
              )}
            </div>
          </Callout>
        ) : (
          <Callout
            variant={summary?.ready ? 'success' : 'warning'}
            icon={summary?.ready ? <CheckCircle2 size={16} /> : <AlertTriangle size={16} />}
            title={summary?.ready
              ? t('proj_closure_ready_title', { defaultValue: 'Rien ne bloque la clôture' })
              : t('proj_closure_blocked_title', {
                  defaultValue: '{{n}} point(s) bloquant(s) restent à régler',
                  n: remainingBlockers.length,
                })}
          >
            <div className="flex items-center gap-3 flex-wrap">
              <span className="text-sm">
                {summary?.ready
                  ? t('proj_closure_ready_msg', { defaultValue: 'Les registres ne signalent plus rien d’inachevé. Il reste à répondre à la charte : les objectifs ont-ils été atteints ?' })
                  : t('proj_closure_blocked_msg', { defaultValue: 'Clôturer malgré tout reste possible, mais il faudra dire pourquoi : ce qui reste ouvert disparaîtrait sinon avec le projet.' })}
              </span>
              <div className="flex-1" />
              {canEdit && (isOwner ? (
                <Button size="sm" icon={<Lock size={14} />} onClick={openCloseWindow}>
                  {t('proj_closure_close_action', { defaultValue: 'Clore le projet' })}
                </Button>
              ) : (
                <span className="text-xs text-text-tertiary">
                  {t('proj_closure_owner_only', { defaultValue: 'Seul le propriétaire du projet peut le clore.' })}
                </span>
              ))}
            </div>
          </Callout>
        )}

        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        {/* ── La liste de contrôle ── */}
        <div className="bg-surface-0 border border-border rounded-xl p-5">
          <div className="flex items-center gap-3 flex-wrap mb-1">
            <ListChecks size={18} className="text-text-secondary shrink-0" />
            <h2 className="text-sm font-semibold text-text-primary">
              {t('proj_closure_checklist', { defaultValue: 'Liste de contrôle' })}
            </h2>
          </div>
          <p className="text-xs text-text-secondary mb-3 leading-relaxed">
            {t('proj_closure_checklist_intro', {
              defaultValue: 'Rien ici n’est saisi à la main : ces contrôles interrogent les registres construits par les autres artefacts et confrontent le projet à ce qu’il a promis avant de le déclarer fini.',
            })}
          </p>

          <div className="mb-4">
            <ProgressBar
              value={settled} max={Math.max(checks.length, 1)} variant="primary" showValue
              label={t('proj_closure_settled', { defaultValue: 'Points réglés' })}
              formatValue={(v, m) => `${v}/${m}`}
              t={t}
            />
          </div>

          <div className={isMobile ? 'space-y-3' : 'flex gap-4 items-start'}>
            <CheckPanel
              title={t('proj_closure_blocking_title', { defaultValue: 'Ce qui n’est pas fait' })}
              intro={t('proj_closure_blocking_intro', {
                defaultValue: 'Du travail engagé qui n’est pas arrivé à son terme. Clore par-dessus est permis, mais jamais en silence : le motif reste attaché à la clôture.',
              })}
              icon={<AlertTriangle size={15} style={{ color: 'var(--color-danger)' }} />}
              checks={blockingChecks} onOpenArtifact={onOpenArtifact} t={t}
            />
            <CheckPanel
              title={t('proj_closure_advisory_title', { defaultValue: 'Ce qui n’est pas écrit' })}
              intro={t('proj_closure_advisory_intro', {
                defaultValue: 'Le projet peut finir sans — mais il n’aura rien appris, et le suivant repartira de zéro.',
              })}
              icon={<BookOpen size={15} style={{ color: 'var(--color-warning)' }} />}
              checks={advisoryChecks} onOpenArtifact={onOpenArtifact} t={t}
            />
          </div>
        </div>

        {/* ── Le dossier de clôture ── */}
        {/* Multi-column flow rather than a grid: a grid makes every row as tall as
            its tallest card, so a short section leaves a hole beside a long one.
            No width cap either — the page uses the window it is given. */}
        <div className={isMobile ? 'space-y-4' : 'columns-2 gap-4 [&>*]:break-inside-avoid [&>*]:mb-4'}>
          <Section
            title={t('proj_closure_objectives_section', { defaultValue: 'Les objectifs ont-ils été atteints ?' })}
            icon={<Target size={15} className="text-text-secondary" />}
          >
            <DossierField
              label={t('proj_closure_objectives', { defaultValue: 'Réponse à la charte' })}
              value={closure.objectives_met}
              rows={5}
              readOnly={readOnly}
              placeholder={t('proj_closure_objectives_ph', { defaultValue: 'Objectif par objectif : atteint, partiellement, abandonné — et pourquoi…' })}
              hint={t('proj_closure_objectives_hint', { defaultValue: 'La charte disait à quoi servait ce projet. C’est la seule question à laquelle elle attend une réponse.' })}
              missing={!objectivesAnswered && !closed}
              missingLabel={t('proj_closure_objectives_missing', {
                defaultValue: 'Sans réponse, le projet ne peut pas être clos — le serveur la réclamera.',
              })}
              onCommit={v => updateMut.mutate({ objectives_met: v })}
            />
          </Section>

          <Section
            title={t('proj_closure_acceptance_section', { defaultValue: 'Recette' })}
            icon={<CheckCircle2 size={15} className="text-text-secondary" />}
          >
            <DossierField
              label={t('proj_closure_acceptance', { defaultValue: 'Qui a prononcé la recette du tout' })}
              value={closure.acceptance_note}
              readOnly={readOnly}
              placeholder={t('proj_closure_acceptance_ph', { defaultValue: 'La personne ou l’instance, la date, ce sur quoi elle s’est prononcée…' })}
              hint={t('proj_closure_acceptance_hint', { defaultValue: 'Les livrables sont acceptés un par un ; l’ensemble, lui, est reçu une fois.' })}
              onCommit={v => updateMut.mutate({ acceptance_note: v })}
            />
          </Section>

          <Section
            title={t('proj_closure_handover_section', { defaultValue: 'Transfert' })}
            icon={<Handshake size={15} className="text-text-secondary" />}
          >
            <DossierField
              label={t('proj_closure_handover', { defaultValue: 'À qui c’est transféré, qui l’exploite' })}
              value={closure.handover_note}
              readOnly={readOnly}
              placeholder={t('proj_closure_handover_ph', { defaultValue: 'L’équipe qui reprend, la documentation remise, la période de garantie…' })}
              hint={t('proj_closure_handover_hint', { defaultValue: 'Le projet s’arrête ; ce qu’il a produit continue, et quelqu’un doit s’en occuper.' })}
              onCommit={v => updateMut.mutate({ handover_note: v })}
            />
          </Section>

          <Section
            title={t('proj_closure_loose_section', { defaultValue: 'Ce qui survit au projet' })}
            icon={<KeyRound size={15} className="text-text-secondary" />}
          >
            <DossierField
              label={t('proj_closure_loose', { defaultValue: 'Contrats, licences, accès' })}
              value={closure.loose_ends}
              readOnly={readOnly}
              placeholder={t('proj_closure_loose_ph', { defaultValue: 'Contrats à résilier, licences à transférer, comptes et accès à révoquer, matériel à rendre…' })}
              hint={t('proj_closure_loose_hint', { defaultValue: 'Les engagements ne s’arrêtent pas avec le projet : ils continuent de coûter tant que personne ne les ferme.' })}
              onCommit={v => updateMut.mutate({ loose_ends: v })}
            />
          </Section>

          <Section
            title={t('proj_closure_final_section', { defaultValue: 'Note finale' })}
            icon={<FileText size={15} className="text-text-secondary" />}
          >
            <DossierField
              label={t('proj_closure_final', { defaultValue: 'Ce qu’il reste à dire' })}
              value={closure.final_note}
              readOnly={readOnly}
              placeholder={t('proj_closure_final_ph', { defaultValue: 'Remerciements, réserves, contexte que les registres ne portent pas…' })}
              onCommit={v => updateMut.mutate({ final_note: v })}
            />
          </Section>
        </div>

        {/* ── Les enseignements ── */}
        <div className="flex items-center gap-3 flex-wrap pt-2">
          <Lightbulb size={20} className="text-text-secondary shrink-0" />
          <h2 className="text-xl font-semibold text-text-primary">
            {t('proj_lessons_title', { defaultValue: 'Enseignements tirés' })}
          </h2>
          <div className="flex-1" />
          {lessons.length > 0 && (
            <Dropdown
              height={36} fontSize={14} focusable width={180}
              value={outcomeFilter} options={outcomeFilterOptions}
              onChange={setOutcomeFilter}
            />
          )}
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_lesson_new', { defaultValue: 'Nouvel enseignement' })}
            </Button>
          )}
        </div>

        {/* The register's own defect, before the register itself. */}
        {withoutReco.length > 0 && (
          <Callout
            variant="warning"
            title={t('proj_lessons_gaps_title', {
              defaultValue: '{{n}} enseignement(s) sans recommandation',
              n: withoutReco.length,
            })}
          >
            <div className="space-y-2">
              <p className="text-sm">
                {t('proj_lessons_gaps_msg', {
                  defaultValue: 'Ceux-là racontent ce qui s’est passé et s’arrêtent là. La recommandation est la seule partie qui voyage : sans elle, le projet suivant refera le même chemin. Elle est aussi exigée pour valider l’enseignement.',
                })}
              </p>
              <div className="flex flex-wrap gap-2">
                {withoutReco.map(l => (
                  <button
                    key={l.id}
                    type="button"
                    onClick={() => focusLesson(l.id)}
                    className="inline-flex items-center gap-1.5 text-sm px-2 py-1 rounded border border-border bg-surface-0 hover:bg-surface-2 text-text-primary"
                  >
                    <span className="font-mono text-xs text-text-tertiary">{l.code}</span>
                    <span className="truncate max-w-[22rem]">{l.title}</span>
                    <ArrowUpRight size={13} className="text-text-tertiary shrink-0" />
                  </button>
                ))}
              </div>
            </div>
          </Callout>
        )}

        {lessonSummary && lessonSummary.total > 0 && (
          <div className="bg-surface-0 border border-border rounded-xl p-5 space-y-4">
            <div className={isMobile ? 'grid grid-cols-2 gap-2' : 'flex gap-2'}>
              <Stat label={t('proj_lessons_stat_total', { defaultValue: 'Enseignements' })}
                value={lessonSummary.total} icon={<Lightbulb size={14} />} />
              <Stat label={t('proj_lessons_stat_validated', { defaultValue: 'Validés' })}
                value={lessonSummary.validated} tone="success" icon={<CheckCircle2 size={14} />} />
              <Stat label={t('proj_lessons_stat_gaps', { defaultValue: 'Sans recommandation' })}
                value={lessonSummary.without_recommendation}
                tone={lessonSummary.without_recommendation > 0 ? 'danger' : 'muted'}
                icon={<AlertTriangle size={14} />} />
            </div>

            {/* The distribution, spelled out: a register that holds only failures
                teaches people to hide them, and one that holds only successes
                teaches nothing at all. */}
            <div>
              <p className="text-sm font-semibold text-text-primary mb-2">
                {t('proj_lessons_distribution', { defaultValue: 'Répartition' })}
              </p>
              <div className="space-y-2">
                <ProgressBar
                  value={lessonSummary.positive} max={Math.max(lessonSummary.total, 1)} variant="success" showValue
                  label={<span className="inline-flex items-center gap-1.5"><ThumbsUp size={13} />{outcomeLabel(t, 'positive')}</span>}
                  formatValue={v => String(v)} size="sm" t={t}
                />
                <ProgressBar
                  value={lessonSummary.negative} max={Math.max(lessonSummary.total, 1)} variant="danger" showValue
                  label={<span className="inline-flex items-center gap-1.5"><ThumbsDown size={13} />{outcomeLabel(t, 'negative')}</span>}
                  formatValue={v => String(v)} size="sm" t={t}
                />
                <ProgressBar
                  value={lessonSummary.mixed} max={Math.max(lessonSummary.total, 1)} variant="warning" showValue
                  label={<span className="inline-flex items-center gap-1.5"><Scale size={13} />{outcomeLabel(t, 'mixed')}</span>}
                  formatValue={v => String(v)} size="sm" t={t}
                />
              </div>
              {lessonSummary.positive === 0 && lessonSummary.total > 0 && (
                <p className="text-xs mt-2" style={{ color: 'var(--color-warning)' }}>
                  {t('proj_lessons_only_failures', {
                    defaultValue: 'Aucune réussite consignée. Un registre qui ne contient que des échecs apprend surtout aux gens à les cacher.',
                  })}
                </p>
              )}
            </div>
          </div>
        )}

        {lessons.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="first-use"
              icon={<Lightbulb size={26} />}
              title={t('proj_lessons_empty_title', { defaultValue: 'Aucun enseignement consigné' })}
              description={t('proj_lessons_empty', {
                defaultValue: 'C’est ici que se décide si le prochain projet repart de zéro. Consignez ce qui a marché autant que ce qui a échoué — et surtout la recommandation : c’est la seule partie qu’une autre équipe lira.',
              })}
              action={canEdit ? {
                label: t('proj_lesson_new', { defaultValue: 'Nouvel enseignement' }),
                icon: <Plus size={14} />,
                onClick: () => setCreateOpen(true),
              } : undefined}
              t={t}
            />
          </div>
        ) : visibleLessons.length === 0 ? (
          <div className="bg-surface-0 border border-border rounded-xl">
            <EmptyState
              variant="no-results"
              icon={<Lightbulb size={26} />}
              title={t('proj_lessons_filter_empty', { defaultValue: 'Aucun enseignement de cette nature' })}
              action={{
                label: t('proj_lessons_filter_clear', { defaultValue: 'Afficher toutes les natures' }),
                onClick: () => setOutcomeFilter(''),
              }}
              t={t}
            />
          </div>
        ) : (
          // The server's order is kept: re-sorting here would bury exactly what
          // it chose to put on top.
          <div className="space-y-4">
            {visibleLessons.map(l => <LessonCard key={l.id} lesson={l} ctx={ctx} />)}
          </div>
        )}
      </div>

      {/* ── Clore : le motif est demandé dans le même geste ── */}
      {closeOpen && (
        <FloatingWindow
          title={t('proj_closure_close_title', { defaultValue: 'Clore le projet' })}
          icon={<Lock size={16} />}
          onClose={() => setCloseOpen(false)}
          defaultWidth={620} defaultHeight={560} padding={16} t={t}
          actions={{
            confirm: {
              label: t('proj_closure_close_confirm', { defaultValue: 'Clore le projet' }),
              disabled: !canSubmitClose,
              loading: closeMut.isPending,
              onClick: submitClose,
            },
          }}
        >
          <div className="space-y-3">
            {!objectivesAnswered && (
              <Callout
                variant="danger"
                title={t('proj_closure_need_objectives_title', { defaultValue: 'La charte attend une réponse' })}
              >
                <span className="text-sm">
                  {t('proj_closure_need_objectives', {
                    defaultValue: 'Dites si les objectifs ont été atteints avant de clore : c’est la seule question à laquelle la charte attend une réponse. Le champ est dans le dossier, juste derrière cette fenêtre.',
                  })}
                </span>
              </Callout>
            )}

            {remainingBlockers.length > 0 ? (
              <>
                {/* What is about to be signed off. Shown at the moment the reason
                    is asked for: that is what the person is committing to. */}
                <div className="rounded-lg border p-3" style={{ borderColor: 'var(--color-danger)' }}>
                  <p className="flex items-center gap-1.5 text-sm font-semibold mb-2" style={{ color: 'var(--color-danger)' }}>
                    <AlertTriangle size={15} />
                    {t('proj_closure_will_remain', { defaultValue: 'Ce qui restera ouvert' })}
                  </p>
                  <ul className="space-y-1">
                    {remainingBlockers.map(c => (
                      <li key={c.key} className="flex items-center gap-2 text-sm text-text-primary">
                        <span className="flex-1 min-w-0">{checkLabel(t, c.key)}</span>
                        {!BOOLEAN_CHECKS.has(c.key) && (
                          <span className="inline-flex">
                            <Badge variant="danger" size="sm">{c.count}</Badge>
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                  <p className="text-xs text-text-secondary mt-2">
                    {t('proj_closure_will_remain_hint', {
                      defaultValue: 'Ces points ne seront pas réglés par la clôture : ils cesseront simplement d’être suivis.',
                    })}
                  </p>
                </div>

                <div>
                  <label className="text-sm text-text-secondary mb-1 block">
                    {t('proj_closure_override_label', { defaultValue: 'Pourquoi clore malgré tout' })}
                  </label>
                  <Textarea
                    autoFocus
                    rows={4}
                    className="h-auto min-h-0 resize-y"
                    value={overrideReason}
                    placeholder={t('proj_closure_override_ph', { defaultValue: 'Ce qui justifie de laisser ces points ouverts, et ce qu’il en advient…' })}
                    onChange={e => setOverrideReason(e.target.value)}
                  />
                  <p className="text-xs mt-1" style={{
                    color: overrideReason.trim() ? 'var(--color-text-tertiary)' : 'var(--color-warning)',
                  }}>
                    {overrideReason.trim()
                      ? t('proj_closure_override_hint', { defaultValue: 'Ce motif restera affiché avec la clôture : c’est la trace de ce qui a été assumé.' })
                      : t('proj_closure_override_required', { defaultValue: 'Obligatoire tant qu’il reste des points bloquants — sinon ce qui reste ouvert disparaît avec le projet.' })}
                  </p>
                </div>
              </>
            ) : (
              <Callout variant="success" icon={<CheckCircle2 size={16} />}
                title={t('proj_closure_nothing_blocking', { defaultValue: 'Aucun point bloquant' })}>
                <span className="text-sm">
                  {t('proj_closure_nothing_blocking_msg', {
                    defaultValue: 'Les registres ne signalent plus rien d’inachevé : la clôture n’enterre rien.',
                  })}
                </span>
              </Callout>
            )}

            {remainingAdvisory.length > 0 && (
              <p className="text-xs text-text-secondary leading-relaxed">
                {t('proj_closure_advisory_remaining', {
                  defaultValue: 'Resteront aussi non écrits : {{list}}. Cela n’empêche pas de clore, mais le projet n’en laissera pas trace.',
                  list: remainingAdvisory.map(c => checkLabel(t, c.key).toLocaleLowerCase(i18n.language)).join(', '),
                })}
              </p>
            )}

            <div className={isMobile ? '' : 'max-w-[14rem]'}>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_closure_closed_on_label', { defaultValue: 'Date de clôture' })}
              </label>
              <Input
                type="date"
                value={closedOn}
                leftIcon={<CalendarCheck size={15} />}
                onChange={e => setClosedOn(e.target.value)}
              />
            </div>
          </div>
        </FloatingWindow>
      )}

      {/* ── Nouvel enseignement ── */}
      {createOpen && (
        <FloatingWindow
          title={t('proj_lesson_new', { defaultValue: 'Nouvel enseignement' })}
          icon={<Lightbulb size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={620} defaultHeight={600} padding={16} t={t}
          actions={{
            confirm: {
              label: t('common_create', { defaultValue: 'Créer' }),
              disabled: !canCreateLesson,
              loading: createLessonMut.isPending,
              onClick: submitCreateLesson,
            },
          }}
        >
          <div className="space-y-3">
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_lesson_col_title', { defaultValue: 'Intitulé' })}
              </label>
              <Input
                autoFocus
                value={draft.title}
                placeholder={t('proj_lesson_title_ph', { defaultValue: 'Ce que le projet a appris, en une phrase…' })}
                onChange={e => setDraft(d => ({ ...d, title: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter' && canCreateLesson) { e.preventDefault(); submitCreateLesson() } }}
              />
            </div>

            <div className={isMobile ? 'space-y-3' : 'grid grid-cols-2 gap-3'}>
              <div className="min-w-0">
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_lesson_outcome', { defaultValue: 'Nature' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={draft.outcome} options={outcomeOptions}
                  onChange={v => setDraft(d => ({ ...d, outcome: v as LessonOutcome }))}
                />
                <p className="text-xs text-text-tertiary mt-1">
                  {t('proj_lesson_outcome_hint', { defaultValue: 'Les réussites comptent autant que les échecs — et se consignent aussi mal.' })}
                </p>
              </div>
              <div className="min-w-0">
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_lesson_category', { defaultValue: 'Catégorie' })}
                </label>
                <Dropdown
                  className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={draft.category} options={categoryOptions}
                  onChange={v => setDraft(d => ({ ...d, category: v as LessonCategory }))}
                />
              </div>
            </div>

            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_lesson_situation', { defaultValue: 'Situation' })}
              </label>
              <Textarea
                rows={2}
                className="h-auto min-h-0 resize-y"
                value={draft.situation}
                placeholder={t('proj_lesson_situation_ph', { defaultValue: 'Le contexte dans lequel cela s’est produit…' })}
                onChange={e => setDraft(d => ({ ...d, situation: e.target.value }))}
              />
            </div>

            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_lesson_what_happened', { defaultValue: 'Ce qui s’est passé' })}
              </label>
              <Textarea
                rows={2}
                className="h-auto min-h-0 resize-y"
                value={draft.what_happened}
                placeholder={t('proj_lesson_what_happened_ph', { defaultValue: 'Les faits, sans la morale…' })}
                onChange={e => setDraft(d => ({ ...d, what_happened: e.target.value }))}
              />
            </div>

            <div>
              <label className="flex items-center gap-1.5 text-sm text-text-secondary mb-1">
                <Lightbulb size={14} />
                {t('proj_lesson_recommendation', { defaultValue: 'Recommandation' })}
              </label>
              <Textarea
                rows={3}
                className="h-auto min-h-0 resize-y"
                value={draft.recommendation}
                placeholder={t('proj_lesson_recommendation_ph', { defaultValue: 'Ce que le prochain projet devrait faire — ou éviter…' })}
                onChange={e => setDraft(d => ({ ...d, recommendation: e.target.value }))}
              />
              <p className="text-xs mt-1" style={{
                color: draft.recommendation.trim() ? 'var(--color-text-tertiary)' : 'var(--color-warning)',
              }}>
                {draft.recommendation.trim()
                  ? t('proj_lesson_reco_hint', { defaultValue: 'Les rattachements (lot, risque, incident, changement) se renseignent ensuite, sur la fiche.' })
                  : t('proj_lesson_reco_warn', { defaultValue: 'Sans elle, l’enseignement restera une anecdote — et ne pourra pas être validé.' })}
              </p>
            </div>

            <p className="flex items-center gap-1.5 text-xs text-text-tertiary">
              <Link2 size={13} />
              {t('proj_lesson_links_later', { defaultValue: 'Un enseignement se rattache ensuite à un lot de travail, un risque, un incident ou un changement.' })}
            </p>
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
