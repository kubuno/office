import { useEffect, useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { format, parseISO } from 'date-fns'
import { getDateLocale, useConfirm } from '@kubuno/sdk'
import {
  Megaphone, Plus, Trash2, Send, Users, History, ChevronRight, ChevronDown,
  AlertTriangle, CalendarClock, ClipboardList,
} from 'lucide-react'
import {
  Button, Input, Textarea, Dropdown, DataTable, Badge, Callout, EmptyState, Checkbox,
  ProgressBar, Tooltip, FloatingWindow, ConfirmDialog, useIsMobile,
  type DataTableColumn, type DropdownOption,
} from '@ui'
import {
  projectsApi,
  type CommunicationLine, type CommunicationEdit, type CommChannel, type CommFrequency,
  type UncoveredStakeholder, type Stakeholder,
} from '../api'

// Communication plan — who is told what, by which means, how often; and the log
// of what was actually sent.
//
// The screen leads with its AUDIT, not with its table: `uncovered` lists the
// stakeholders no communication reaches. A regulator with a power of 5 that
// nobody informs is a defect of the plan, exactly like a requirement nothing
// realises — so it is stated in words, ranked by weight, before anything else.
//
// Two other things this screen refuses to hide:
//  • `purpose` gets its own column. A report nobody can name a purpose for is a
//    habit, not a need, and the header counts how many are in that state.
//  • `overdue` is the only figure here that demands an action, so it is the only
//    one rendered in the danger colour.

/** Frequencies for which the server cannot compute a next date — it returns
 *  `next_due: null` after a send, and that is an answer, not a gap. */
const NO_AUTO_NEXT: CommFrequency[] = ['milestone', 'on_demand', 'once']

const CHANNELS: CommChannel[] = ['email', 'meeting', 'report', 'dashboard', 'chat', 'workshop', 'other']
const FREQUENCIES: CommFrequency[] = ['daily', 'weekly', 'biweekly', 'monthly', 'quarterly', 'milestone', 'on_demand', 'once']

/** Parses a plain `YYYY-MM-DD` day as LOCAL midnight. `new Date('2026-12-12')`
 *  is parsed as UTC and slips to the previous day west of Greenwich. */
function parseDay(day: string): Date {
  const [y, m, d] = day.slice(0, 10).split('-').map(Number)
  return new Date(y, (m ?? 1) - 1, d ?? 1)
}

const todayIso = () => format(new Date(), 'yyyy-MM-dd')

/** Weight = power × interest. Uncovering a 25 is not uncovering a 2. */
function weightVariant(weight: number): 'danger' | 'warning' | 'neutral' {
  if (weight >= 15) return 'danger'
  if (weight >= 6) return 'warning'
  return 'neutral'
}

/** Cell field committed on blur — one PATCH per edit, not one per keystroke.
 *  Declared at module level: a component defined inside the render would be a
 *  new type on every pass, remounting the row and losing what is being typed. */
function CellInput({ value, placeholder, required, type, className, onCommit }: {
  value: string
  placeholder?: string
  /** Refuse an empty value and snap back — the server answers 422. */
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
      onChange={e => setDraft(e.target.value)}
      onBlur={() => {
        if (draft === value) return
        if (required && !draft.trim()) { setDraft(value); return }
        onCommit(draft)
      }}
    />
  )
}

/** The audit: stakeholders the plan reaches nobody about, worst first. */
function UncoveredPanel({ uncovered, covered, total, onOpenStakeholders }: {
  uncovered: UncoveredStakeholder[]
  covered: number
  total: number
  onOpenStakeholders?: () => void
}) {
  const { t } = useTranslation('office')

  if (total === 0) {
    return (
      <Callout variant="info" t={t}
        title={t('proj_comm_no_stakeholders', { defaultValue: 'Aucune partie prenante enregistrée' })}
        action={onOpenStakeholders ? {
          label: t('proj_comm_open_register', { defaultValue: 'Ouvrir le registre' }),
          onClick: onOpenStakeholders,
        } : undefined}>
        {t('proj_comm_no_stakeholders_desc', {
          defaultValue: 'Tant que personne n’est inscrit au registre, un plan de communication ne s’adresse à personne : rien ne permet de dire s’il laisse quelqu’un de côté.',
        })}
      </Callout>
    )
  }

  return (
    <div className="bg-surface-0 border border-border rounded-xl p-5">
      <div className="flex items-center gap-2 mb-1 flex-wrap">
        {uncovered.length > 0
          ? <AlertTriangle size={17} className="text-warning shrink-0" />
          : <Users size={17} className="text-success shrink-0" />}
        <h2 className="text-sm font-semibold text-text-primary">
          {t('proj_comm_uncovered_title', { defaultValue: 'Qui ne reçoit rien' })}
        </h2>
        <Badge variant={uncovered.length > 0 ? 'warning' : 'success'}>{uncovered.length}</Badge>
        <div className="flex-1" />
        {onOpenStakeholders && (
          <Button size="sm" variant="text" icon={<Users size={14} />} onClick={onOpenStakeholders}>
            {t('proj_comm_open_register', { defaultValue: 'Ouvrir le registre' })}
          </Button>
        )}
      </div>

      <div className="mb-3">
        <ProgressBar
          value={covered} max={Math.max(total, 1)}
          variant={uncovered.length === 0 ? 'success' : covered * 2 >= total ? 'warning' : 'danger'}
          label={t('proj_comm_coverage', {
            defaultValue: '{{covered}} partie(s) prenante(s) sur {{total}} sont destinataires d’au moins une communication',
            covered, total,
          })}
          t={t}
        />
      </div>

      {uncovered.length === 0 ? (
        <p className="text-sm text-text-secondary">
          {t('proj_comm_uncovered_none', {
            defaultValue: 'Personne n’est laissé de côté : chaque partie prenante du registre figure dans au moins une communication du plan.',
          })}
        </p>
      ) : (
        <>
          <p className="text-sm text-text-secondary mb-3">
            {t('proj_comm_uncovered_desc', {
              defaultValue: 'Ces personnes sont au registre du projet et n’apparaissent dans aucune communication. Les plus lourdes d’abord — le poids est le produit du pouvoir par l’intérêt.',
            })}
          </p>
          <ul className="space-y-2">
            {uncovered.map(s => (
              <li key={s.id} className="flex items-start gap-2.5">
                <Tooltip label={t('proj_comm_weight_tip', {
                  defaultValue: 'Poids = pouvoir {{power}} × intérêt {{interest}}',
                  power: s.power, interest: s.interest,
                })}>
                  <span className="inline-flex shrink-0 mt-0.5">
                    <Badge variant={weightVariant(s.weight)}>{s.weight}</Badge>
                  </span>
                </Tooltip>
                <p className="text-sm text-text-primary min-w-0">
                  {t('proj_comm_uncovered_line', {
                    defaultValue: '{{name}} a un pouvoir de {{power}} et un intérêt de {{interest}}, et ne figure dans aucune communication.',
                    name: s.name, power: s.power, interest: s.interest,
                  })}
                </p>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}

/** Picks the WHOLE audience of one communication. The server replaces the set,
 *  so this window always sends the complete list of identifiers. */
function AudienceWindow({ line, stakeholders, saving, onCancel, onSave }: {
  line: CommunicationLine
  stakeholders: Stakeholder[]
  saving: boolean
  onCancel: () => void
  onSave: (ids: string[]) => void
}) {
  const { t } = useTranslation('office')
  const [picked, setPicked] = useState<string[]>(() => line.audience.map(a => a.id))

  // Heaviest first: choosing an audience is choosing who matters here.
  const ordered = useMemo(
    () => [...stakeholders].sort((a, b) => (b.power * b.interest) - (a.power * a.interest) || a.name.localeCompare(b.name)),
    [stakeholders],
  )

  const toggle = (id: string, on: boolean) =>
    setPicked(prev => (on ? [...new Set([...prev, id])] : prev.filter(x => x !== id)))

  return (
    <FloatingWindow
      title={t('proj_comm_audience_title', { defaultValue: 'Destinataires de « {{name}} »', name: line.communication.name })}
      icon={<Users size={16} />}
      onClose={onCancel}
      defaultWidth={480} defaultHeight={520} padding={16} t={t}
      actions={{
        confirm: {
          label: t('common_save', { defaultValue: 'Enregistrer' }),
          loading: saving,
          autoFocus: true,
          onClick: () => onSave(picked),
        },
      }}
    >
      {stakeholders.length === 0 ? (
        <EmptyState
          variant="first-use"
          compact
          icon={<Users size={26} />}
          title={t('proj_comm_audience_empty', { defaultValue: 'Aucune partie prenante' })}
          description={t('proj_comm_audience_empty_desc', {
            defaultValue: 'Les destinataires se choisissent parmi les parties prenantes du projet : le registre doit être rempli d’abord.',
          })}
          t={t}
        />
      ) : (
        <div className="space-y-2">
          <p className="text-sm text-text-secondary">
            {t('proj_comm_audience_desc', {
              defaultValue: 'La liste cochée remplace entièrement les destinataires actuels.',
            })}
          </p>
          <div className="space-y-1.5">
            {ordered.map(s => (
              <div key={s.id} className="border border-border rounded-lg px-3 py-2 bg-surface-0">
                <Checkbox
                  checked={picked.includes(s.id)}
                  onChange={on => toggle(s.id, on)}
                  label={s.role_title ? `${s.name} — ${s.role_title}` : s.name}
                  description={t('proj_comm_audience_weight', {
                    defaultValue: 'Pouvoir {{power}} · Intérêt {{interest}}',
                    power: s.power, interest: s.interest,
                  })}
                />
              </div>
            ))}
          </div>
        </div>
      )}
    </FloatingWindow>
  )
}

export default function CommunicationsView({ projectId, canEdit = true, onOpenStakeholders }: {
  projectId: string
  /** False in the mobile reading mode, where the plan is read, not edited. */
  canEdit?: boolean
  /** Jumps to the stakeholder register — where an uncovered person is fixed. */
  onOpenStakeholders?: () => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const isMobile = useIsMobile()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [sentNotice, setSentNotice] = useState<string | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const [logOpen, setLogOpen] = useState(false)
  const [logFormOpen, setLogFormOpen] = useState(false)
  const [audienceFor, setAudienceFor] = useState<string | null>(null)
  const [draft, setDraft] = useState<{ name: string; purpose: string; channel: CommChannel; frequency: CommFrequency; format: string; next_due: string }>(
    { name: '', purpose: '', channel: 'email', frequency: 'weekly', format: '', next_due: '' })
  const [logDraft, setLogDraft] = useState<{ communication_id: string; sent_on: string; summary: string }>(
    { communication_id: '', sent_on: todayIso(), summary: '' })

  const { data: plan, isLoading, isError, refetch } = useQuery({
    queryKey: ['communications', projectId],
    queryFn: () => projectsApi.getCommunications(projectId),
  })
  // The register supplies the only people a communication may be addressed to;
  // the key is shared with the stakeholder screen, so both read one cache entry.
  const { data: register } = useQuery({
    queryKey: ['stakeholders', projectId],
    queryFn: () => projectsApi.getStakeholders(projectId),
  })
  const { data: logEntries } = useQuery({
    queryKey: ['communication-log', projectId],
    queryFn: () => projectsApi.listCommunicationLog(projectId),
  })

  // The api client flattens server errors to `{ message, code }` at the root:
  // there is no `response.data` to read here.
  const fail = (err: unknown) => setErrorMsg((err as { message?: string }).message
    ?? t('common_error', { defaultValue: 'Une erreur est survenue.' }))
  const invalPlan = () => qc.invalidateQueries({ queryKey: ['communications', projectId] })
  const invalLog = () => qc.invalidateQueries({ queryKey: ['communication-log', projectId] })
  const done = () => { setErrorMsg(null); invalPlan() }

  const createMut = useMutation({
    mutationFn: (data: CommunicationEdit & { name: string }) => projectsApi.createCommunication(projectId, data),
    onSuccess: created => {
      done()
      setCreateOpen(false)
      setDraft({ name: '', purpose: '', channel: 'email', frequency: 'weekly', format: '', next_due: '' })
      // A communication with no audience reaches nobody: offer the picker right
      // away rather than adding a line to the plan that changes nothing.
      setAudienceFor(created.id)
    },
    onError: fail,
  })
  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: CommunicationEdit }) => projectsApi.updateCommunication(projectId, id, data),
    onSuccess: () => { done(); setAudienceFor(null) },
    onError: fail,
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => projectsApi.deleteCommunication(projectId, id),
    onSuccess: done,
    onError: fail,
  })
  const logMut = useMutation({
    mutationFn: ({ data }: { data: { communication_id?: string; sent_on?: string; summary?: string }; label: string; frequency: CommFrequency | null }) =>
      projectsApi.logCommunication(projectId, data),
    onSuccess: (res, vars) => {
      setErrorMsg(null)
      invalPlan(); invalLog()
      setLogFormOpen(false)
      setLogDraft({ communication_id: '', sent_on: todayIso(), summary: '' })
      // The server moved the next date forward itself — say where it landed
      // instead of leaving the user to re-read the row.
      if (res.next_due) {
        setSentNotice(t('proj_comm_sent_next', {
          defaultValue: '{{label}} : envoi consigné le {{sent}}. Prochain envoi le {{next}}.',
          label: vars.label, sent: fmtDate(res.sent_on), next: fmtDate(res.next_due),
        }))
      } else if (vars.frequency && NO_AUTO_NEXT.includes(vars.frequency)) {
        setSentNotice(t('proj_comm_sent_no_next', {
          defaultValue: '{{label}} : envoi consigné le {{sent}}. Sa fréquence est « {{freq}} » : il n’y a pas de prochaine date à calculer.',
          label: vars.label, sent: fmtDate(res.sent_on), freq: freqLabel(vars.frequency),
        }))
      } else {
        setSentNotice(t('proj_comm_sent_plain', {
          defaultValue: 'Envoi consigné le {{sent}}.', sent: fmtDate(res.sent_on),
        }))
      }
    },
    onError: fail,
  })

  const rows = useMemo(() => plan?.communications ?? [], [plan])
  const summary = plan?.summary
  const stakeholders = register?.stakeholders ?? []
  const withoutPurpose = rows.filter(l => !l.communication.purpose.trim()).length

  const fmtDate = (day: string) => format(parseDay(day), 'd MMMM yyyy', { locale: getDateLocale(i18n.language) })
  const fmtShort = (day: string) => format(parseDay(day), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })

  const channelLabel = (c: CommChannel) => ({
    email:     t('proj_comm_chan_email',     { defaultValue: 'Courriel' }),
    meeting:   t('proj_comm_chan_meeting',   { defaultValue: 'Réunion' }),
    report:    t('proj_comm_chan_report',    { defaultValue: 'Rapport' }),
    dashboard: t('proj_comm_chan_dashboard', { defaultValue: 'Tableau de bord' }),
    chat:      t('proj_comm_chan_chat',      { defaultValue: 'Messagerie' }),
    workshop:  t('proj_comm_chan_workshop',  { defaultValue: 'Atelier' }),
    other:     t('proj_comm_chan_other',     { defaultValue: 'Autre' }),
  })[c]

  const freqLabel = (f: CommFrequency) => ({
    daily:     t('proj_comm_freq_daily',     { defaultValue: 'Quotidienne' }),
    weekly:    t('proj_comm_freq_weekly',    { defaultValue: 'Hebdomadaire' }),
    biweekly:  t('proj_comm_freq_biweekly',  { defaultValue: 'Toutes les deux semaines' }),
    monthly:   t('proj_comm_freq_monthly',   { defaultValue: 'Mensuelle' }),
    quarterly: t('proj_comm_freq_quarterly', { defaultValue: 'Trimestrielle' }),
    milestone: t('proj_comm_freq_milestone', { defaultValue: 'À chaque jalon' }),
    on_demand: t('proj_comm_freq_on_demand', { defaultValue: 'À la demande' }),
    once:      t('proj_comm_freq_once',      { defaultValue: 'Une seule fois' }),
  })[f]

  const channelOptions: DropdownOption[] = useMemo(
    () => CHANNELS.map(c => ({ value: c, label: channelLabel(c) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [i18n.language])
  const freqOptions: DropdownOption[] = useMemo(
    () => FREQUENCIES.map(f => ({ value: f, label: freqLabel(f) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [i18n.language])
  const commOptions: DropdownOption[] = useMemo(() => [
    { value: '', label: t('proj_comm_log_offplan', { defaultValue: '— Hors plan —' }) },
    ...rows.map(l => ({ value: l.communication.id, label: l.communication.name })),
  ], [rows, t])

  const patch = (id: string, data: CommunicationEdit) => updateMut.mutate({ id, data })

  const askDelete = async (line: CommunicationLine) => {
    const ok = await confirm({
      title: t('proj_comm_delete_title', { defaultValue: 'Supprimer la communication ?' }),
      message: t('proj_comm_delete_msg', {
        defaultValue: '« {{name}} » sera retirée du plan. Les envois déjà consignés restent au journal, sans plus rien pointer.',
        name: line.communication.name,
      }),
      confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
      variant: 'danger',
    })
    if (ok) deleteMut.mutate(line.communication.id)
  }

  const markSent = (line: CommunicationLine) => logMut.mutate({
    data: { communication_id: line.communication.id, sent_on: todayIso() },
    label: line.communication.name,
    frequency: line.communication.frequency,
  })

  const columns: DataTableColumn<CommunicationLine>[] = [
    {
      id: 'name',
      header: t('proj_comm_name', { defaultValue: 'Communication' }),
      headerText: t('proj_comm_name', { defaultValue: 'Communication' }),
      width: 210,
      primary: true,
      required: true,
      sortValue: l => l.communication.name,
      cell: l => (
        <div className="flex flex-col gap-1 py-1 min-w-0">
          {canEdit
            ? <CellInput value={l.communication.name} required
                onCommit={v => patch(l.communication.id, { name: v.trim() })} />
            : <span className="text-sm text-text-primary">{l.communication.name}</span>}
          {/* Suspended, not deleted: it stays readable and stays countable. */}
          {!l.communication.is_active && (
            <span className="inline-flex">
              <Badge variant="neutral">{t('proj_comm_inactive', { defaultValue: 'Suspendue' })}</Badge>
            </span>
          )}
        </div>
      ),
    },
    {
      id: 'purpose',
      header: t('proj_comm_purpose', { defaultValue: 'Objet' }),
      headerText: t('proj_comm_purpose', { defaultValue: 'Objet' }),
      width: 290,
      sortValue: l => l.communication.purpose,
      cell: l => (
        <div className="flex flex-col gap-1 py-1 min-w-0">
          {canEdit
            ? <CellInput value={l.communication.purpose}
                placeholder={t('proj_comm_purpose_ph', { defaultValue: 'À quoi sert-elle ?' })}
                onCommit={v => patch(l.communication.id, { purpose: v })} />
            : <span className="text-sm text-text-secondary">{l.communication.purpose || '—'}</span>}
          {/* The screen's second defect, after an uncovered stakeholder. */}
          {!l.communication.purpose.trim() && (
            <p className="text-[11px] text-warning">
              {t('proj_comm_purpose_missing', { defaultValue: 'Sans objet, c’est une habitude, pas un besoin.' })}
            </p>
          )}
        </div>
      ),
    },
    {
      id: 'channel',
      header: t('proj_comm_channel', { defaultValue: 'Canal' }),
      headerText: t('proj_comm_channel', { defaultValue: 'Canal' }),
      width: 150,
      sortValue: l => l.communication.channel,
      cell: l => canEdit
        ? <Dropdown className="w-full" width="100%" height={36} fontSize={14} focusable
            value={l.communication.channel} options={channelOptions}
            onChange={v => patch(l.communication.id, { channel: v as CommChannel })} />
        : <span className="text-sm text-text-secondary">{channelLabel(l.communication.channel)}</span>,
    },
    {
      id: 'format',
      header: t('proj_comm_format', { defaultValue: 'Format' }),
      headerText: t('proj_comm_format', { defaultValue: 'Format' }),
      width: 150,
      sortValue: l => l.communication.format,
      cell: l => canEdit
        ? <CellInput value={l.communication.format}
            placeholder={t('proj_comm_format_ph', { defaultValue: 'Une page, visio 30 min…' })}
            onCommit={v => patch(l.communication.id, { format: v })} />
        : <span className="text-sm text-text-secondary">{l.communication.format || '—'}</span>,
    },
    {
      id: 'frequency',
      header: t('proj_comm_frequency', { defaultValue: 'Fréquence' }),
      headerText: t('proj_comm_frequency', { defaultValue: 'Fréquence' }),
      width: 170,
      sortValue: l => l.communication.frequency,
      cell: l => canEdit
        ? <Dropdown className="w-full" width="100%" height={36} fontSize={14} focusable
            value={l.communication.frequency} options={freqOptions}
            onChange={v => patch(l.communication.id, { frequency: v as CommFrequency })} />
        : <span className="text-sm text-text-secondary">{freqLabel(l.communication.frequency)}</span>,
    },
    {
      id: 'next',
      header: t('proj_comm_next', { defaultValue: 'Prochaine échéance' }),
      headerText: t('proj_comm_next', { defaultValue: 'Prochaine échéance' }),
      width: 190,
      sortValue: l => l.communication.next_due,
      cell: l => (
        <div className="flex flex-col gap-1 py-1">
          {canEdit
            ? <Input type="date" value={l.communication.next_due ?? ''}
                onChange={e => patch(l.communication.id, { next_due: e.target.value || null })} />
            : <span className={`text-sm ${l.overdue ? 'text-danger font-medium' : 'text-text-secondary'}`}>
                {l.communication.next_due ? fmtShort(l.communication.next_due) : '—'}
              </span>}
          {/* The only figure on this screen that asks for something to be done. */}
          {l.overdue && (
            <span className="inline-flex">
              <Badge variant="danger" dot>{t('proj_comm_overdue', { defaultValue: 'En retard' })}</Badge>
            </span>
          )}
          {!l.communication.next_due && NO_AUTO_NEXT.includes(l.communication.frequency) && (
            <p className="text-[11px] text-text-tertiary">
              {t('proj_comm_no_next', { defaultValue: 'Pas de prochaine date à calculer.' })}
            </p>
          )}
        </div>
      ),
    },
    {
      id: 'audience',
      header: t('proj_comm_audience', { defaultValue: 'Destinataires' }),
      headerText: t('proj_comm_audience', { defaultValue: 'Destinataires' }),
      width: 220,
      sortValue: l => l.audience.length,
      cell: l => {
        const names = l.audience.map(a => a.name)
        const label = names.length === 0
          ? t('proj_comm_audience_nobody', { defaultValue: 'Personne' })
          : names.length <= 2 ? names.join(' · ')
          : t('proj_comm_audience_more', { defaultValue: '{{first}} + {{n}} autre(s)', first: names[0], n: names.length - 1 })
        const tip = names.length === 0
          ? t('proj_comm_audience_nobody_tip', { defaultValue: 'Cette communication ne va à personne : elle ne couvre aucune partie prenante.' })
          : names.join(', ')
        if (!canEdit) {
          return (
            <Tooltip label={tip}>
              <span className={`text-sm truncate block ${names.length === 0 ? 'text-warning' : 'text-text-secondary'}`}>{label}</span>
            </Tooltip>
          )
        }
        return (
          <Tooltip label={tip}>
            <Button size="sm" variant="ghost" className="w-full justify-start" icon={<Users size={14} />}
              onClick={() => setAudienceFor(l.communication.id)}>
              <span className={`truncate ${names.length === 0 ? 'text-warning' : ''}`}>{label}</span>
            </Button>
          </Tooltip>
        )
      },
    },
    {
      id: 'sent',
      header: t('proj_comm_sent_col', { defaultValue: 'Envoi' }),
      headerText: t('proj_comm_sent_col', { defaultValue: 'Envoi' }),
      width: 130,
      align: 'right',
      cell: l => canEdit ? (
        <Tooltip label={t('proj_comm_sent_tip', {
          defaultValue: 'Consigne un envoi aujourd’hui ; la prochaine échéance est avancée selon la fréquence.',
        })}>
          <Button size="sm" variant="secondary" icon={<Send size={14} />}
            disabled={logMut.isPending}
            onClick={() => markSent(l)}>
            {t('proj_comm_mark_sent', { defaultValue: 'Envoyé' })}
          </Button>
        </Tooltip>
      ) : <span className="text-sm text-text-tertiary">—</span>,
    },
  ]

  const canCreate = draft.name.trim().length > 0
  const submitCreate = () => {
    if (!canCreate) return
    createMut.mutate({
      name: draft.name.trim(),
      purpose: draft.purpose.trim(),
      channel: draft.channel,
      frequency: draft.frequency,
      format: draft.format.trim(),
      next_due: draft.next_due || null,
    })
  }

  const editingAudience = audienceFor ? rows.find(l => l.communication.id === audienceFor) ?? null : null
  const entries = logEntries ?? []

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className={`${isMobile ? 'p-4' : 'p-6'} space-y-4`}>

        <div className="flex items-center gap-3 flex-wrap">
          <Megaphone size={20} className="text-text-secondary shrink-0" />
          <h1 className="text-xl font-semibold text-text-primary">
            {t('proj_comm_title', { defaultValue: 'Plan de communication' })}
          </h1>
          {summary && summary.overdue > 0 && (
            <Badge variant="danger" dot>
              {t('proj_comm_overdue_count', { defaultValue: '{{n}} en retard', n: summary.overdue })}
            </Badge>
          )}
          {summary && (
            <span className="text-sm text-text-tertiary">
              {t('proj_comm_active_count', {
                defaultValue: '{{active}} active(s) sur {{total}}',
                active: summary.active, total: summary.total,
              })}
            </span>
          )}
          <div className="flex-1" />
          {canEdit && (
            <Button size="sm" variant="secondary" icon={<ClipboardList size={14} />}
              onClick={() => { setLogDraft({ communication_id: '', sent_on: todayIso(), summary: '' }); setLogFormOpen(true) }}>
              {t('proj_comm_log_add', { defaultValue: 'Consigner un envoi' })}
            </Button>
          )}
          {canEdit && (
            <Button size="sm" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
              {t('proj_comm_new', { defaultValue: 'Nouvelle communication' })}
            </Button>
          )}
        </div>

        {/* The audit comes FIRST: it is what one opens this screen to find out. */}
        <UncoveredPanel
          uncovered={plan?.uncovered ?? []}
          covered={summary?.covered ?? 0}
          total={summary?.stakeholders ?? 0}
          onOpenStakeholders={onOpenStakeholders}
        />

        {withoutPurpose > 0 && (
          <Callout variant="warning" t={t}>
            {t('proj_comm_purpose_missing_count', {
              defaultValue: '{{n}} communication(s) n’ont pas d’objet écrit. Une communication dont personne ne sait dire à quoi elle sert est une habitude, pas un besoin.',
              n: withoutPurpose,
            })}
          </Callout>
        )}

        {sentNotice && (
          <Callout variant="success" dismissible onDismiss={() => setSentNotice(null)} t={t}>{sentNotice}</Callout>
        )}
        {errorMsg && (
          <Callout variant="danger" dismissible onDismiss={() => setErrorMsg(null)} t={t}>{errorMsg}</Callout>
        )}

        <DataTable<CommunicationLine>
          rows={rows}
          columns={columns}
          rowKey={l => l.communication.id}
          loading={isLoading}
          error={isError ? t('proj_comm_load_error', { defaultValue: 'Le plan de communication n’a pas pu être chargé.' }) : undefined}
          onRetry={() => { void refetch() }}
          minTableWidth={1560}
          pageSize={0}
          resizableColumns={false}
          rowActions={canEdit ? [
            {
              id: 'audience',
              label: t('proj_comm_audience_edit', { defaultValue: 'Destinataires…' }),
              icon: <Users size={14} />,
              onClick: l => setAudienceFor(l.communication.id),
            },
            {
              id: 'toggle',
              label: t('proj_comm_suspend', { defaultValue: 'Suspendre' }),
              icon: <CalendarClock size={14} />,
              hidden: l => !l.communication.is_active,
              onClick: l => patch(l.communication.id, { is_active: false }),
            },
            {
              id: 'resume',
              label: t('proj_comm_resume', { defaultValue: 'Réactiver' }),
              icon: <CalendarClock size={14} />,
              hidden: l => l.communication.is_active,
              onClick: l => patch(l.communication.id, { is_active: true }),
            },
            {
              id: 'delete',
              label: t('common_delete', { defaultValue: 'Supprimer' }),
              icon: <Trash2 size={14} />,
              danger: true,
              onClick: l => { void askDelete(l) },
            },
          ] : undefined}
          emptyState={
            <EmptyState
              variant="first-use"
              icon={<Megaphone size={26} />}
              title={t('proj_comm_empty_title', { defaultValue: 'Aucune communication planifiée' })}
              description={t('proj_comm_empty', {
                defaultValue: 'Une communication dit qui est informé de quoi, par quel moyen et à quelle fréquence. Sans plan, l’information ne va qu’à ceux qui pensent à la demander.',
              })}
              action={canEdit ? {
                label: t('proj_comm_new', { defaultValue: 'Nouvelle communication' }),
                icon: <Plus size={14} />,
                onClick: () => setCreateOpen(true),
              } : undefined}
              t={t}
            />
          }
          t={t}
        />

        {/* ── Journal des envois ──
            Le plan dit ce qui était prévu ; le journal dit ce qui est parti. */}
        <div className="bg-surface-0 border border-border rounded-xl">
          <Button
            variant="ghost"
            className="w-full justify-start px-5 py-3"
            icon={logOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            onClick={() => setLogOpen(o => !o)}
          >
            <span className="flex items-center gap-2 flex-wrap">
              <History size={16} className="text-text-secondary" />
              <span className="text-sm font-semibold text-text-primary">
                {t('proj_comm_log_title', { defaultValue: 'Journal des envois' })}
              </span>
              <Badge variant="neutral">{entries.length}</Badge>
            </span>
          </Button>

          {logOpen && (
            <div className="px-5 pb-5 space-y-3">
              <p className="text-sm text-text-secondary">
                {t('proj_comm_log_desc', {
                  defaultValue: 'Ce qui est réellement parti, à comparer à ce qui était prévu.',
                })}
              </p>
              {entries.length === 0 ? (
                <EmptyState
                  variant="first-use"
                  compact
                  icon={<History size={26} />}
                  title={t('proj_comm_log_empty', { defaultValue: 'Aucun envoi consigné' })}
                  description={t('proj_comm_log_empty_desc', {
                    defaultValue: 'Le bouton « Envoyé » d’une ligne consigne l’envoi du jour et avance la prochaine échéance.',
                  })}
                  t={t}
                />
              ) : (
                <ul className="divide-y divide-border">
                  {entries.map(e => (
                    <li key={e.id} className="py-2.5 flex items-start gap-3">
                      <span className="text-sm text-text-tertiary shrink-0 w-32">{fmtShort(e.sent_on)}</span>
                      <div className="min-w-0 flex-1">
                        <p className="text-sm text-text-primary">
                          {e.communication_name ?? t('proj_comm_log_offplan_name', { defaultValue: 'Envoi hors plan' })}
                        </p>
                        {e.summary && (
                          <p className="text-sm text-text-secondary whitespace-pre-wrap break-words">{e.summary}</p>
                        )}
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      </div>

      {createOpen && (
        <FloatingWindow
          title={t('proj_comm_new', { defaultValue: 'Nouvelle communication' })}
          icon={<Megaphone size={16} />}
          onClose={() => setCreateOpen(false)}
          defaultWidth={520} defaultHeight={520} padding={16} t={t}
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
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_comm_name', { defaultValue: 'Communication' })}
              </label>
              <Input
                autoFocus
                value={draft.name}
                placeholder={t('proj_comm_name_ph', { defaultValue: 'Point d’avancement hebdomadaire…' })}
                onChange={e => setDraft(d => ({ ...d, name: e.target.value }))}
                onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submitCreate() } }}
              />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_comm_purpose', { defaultValue: 'Objet' })}
              </label>
              <Textarea
                rows={2}
                value={draft.purpose}
                placeholder={t('proj_comm_purpose_win_ph', { defaultValue: 'À quoi sert-elle ? Quelle décision ou quelle inquiétude nourrit-elle ?' })}
                onChange={e => setDraft(d => ({ ...d, purpose: e.target.value }))}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_comm_channel', { defaultValue: 'Canal' })}
                </label>
                <Dropdown className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={draft.channel} options={channelOptions}
                  onChange={v => setDraft(d => ({ ...d, channel: v as CommChannel }))} />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_comm_frequency', { defaultValue: 'Fréquence' })}
                </label>
                <Dropdown className="w-full" width="100%" height={36} fontSize={14} focusable
                  value={draft.frequency} options={freqOptions}
                  onChange={v => setDraft(d => ({ ...d, frequency: v as CommFrequency }))} />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_comm_format', { defaultValue: 'Format' })}
                </label>
                <Input value={draft.format}
                  placeholder={t('proj_comm_format_ph', { defaultValue: 'Une page, visio 30 min…' })}
                  onChange={e => setDraft(d => ({ ...d, format: e.target.value }))} />
              </div>
              <div>
                <label className="text-sm text-text-secondary mb-1 block">
                  {t('proj_comm_next', { defaultValue: 'Prochaine échéance' })}
                </label>
                <Input type="date" value={draft.next_due}
                  onChange={e => setDraft(d => ({ ...d, next_due: e.target.value }))} />
              </div>
            </div>
            <p className="text-xs text-text-tertiary">
              {t('proj_comm_new_hint', {
                defaultValue: 'Les destinataires se choisissent juste après, parmi les parties prenantes du projet.',
              })}
            </p>
          </div>
        </FloatingWindow>
      )}

      {editingAudience && (
        <AudienceWindow
          // Remounted when the target changes: the picked set is seeded once.
          key={editingAudience.communication.id}
          line={editingAudience}
          stakeholders={stakeholders}
          saving={updateMut.isPending}
          onCancel={() => setAudienceFor(null)}
          // The audience is replaced as a whole: the full list of identifiers
          // goes over the wire, never a delta.
          onSave={ids => patch(editingAudience.communication.id, { audience: ids })}
        />
      )}

      {logFormOpen && (
        <FloatingWindow
          title={t('proj_comm_log_add', { defaultValue: 'Consigner un envoi' })}
          icon={<Send size={16} />}
          onClose={() => setLogFormOpen(false)}
          defaultWidth={480} defaultHeight={420} padding={16} t={t}
          actions={{
            confirm: {
              label: t('proj_comm_log_save', { defaultValue: 'Consigner' }),
              loading: logMut.isPending,
              autoFocus: true,
              onClick: () => {
                const target = rows.find(l => l.communication.id === logDraft.communication_id)
                logMut.mutate({
                  data: {
                    communication_id: logDraft.communication_id || undefined,
                    sent_on: logDraft.sent_on || undefined,
                    summary: logDraft.summary.trim() || undefined,
                  },
                  label: target?.communication.name
                    ?? t('proj_comm_log_offplan_name', { defaultValue: 'Envoi hors plan' }),
                  frequency: target?.communication.frequency ?? null,
                })
              },
            },
          }}
        >
          <div className="space-y-3">
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_comm_name', { defaultValue: 'Communication' })}
              </label>
              <Dropdown className="w-full" width="100%" height={36} fontSize={14} focusable
                value={logDraft.communication_id} options={commOptions}
                onChange={v => setLogDraft(d => ({ ...d, communication_id: v }))} />
              <p className="text-xs text-text-tertiary mt-1">
                {t('proj_comm_log_pick_hint', {
                  defaultValue: 'Rattachée au plan, la prochaine échéance est avancée selon sa fréquence.',
                })}
              </p>
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_comm_log_date', { defaultValue: 'Date d’envoi' })}
              </label>
              <Input type="date" value={logDraft.sent_on}
                onChange={e => setLogDraft(d => ({ ...d, sent_on: e.target.value }))} />
            </div>
            <div>
              <label className="text-sm text-text-secondary mb-1 block">
                {t('proj_comm_log_summary', { defaultValue: 'Résumé' })}
              </label>
              <Textarea
                rows={4}
                value={logDraft.summary}
                placeholder={t('proj_comm_log_summary_ph', { defaultValue: 'Ce qui a été dit, et à qui.' })}
                onChange={e => setLogDraft(d => ({ ...d, summary: e.target.value }))}
              />
            </div>
          </div>
        </FloatingWindow>
      )}

      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </div>
  )
}
