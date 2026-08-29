import { useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { addMonths, format, parseISO } from 'date-fns'
import { CalendarPlus } from 'lucide-react'
import { Button } from '@ui'
import { useTranslation } from 'react-i18next'
import { projectsApi, holidaysApi } from '../api'
import WorkingCalendarEditor, {
  type WorkingWeek, type CalendarException,
} from '../shared/WorkingCalendarEditor'
import { useOfficeInstance } from '../useOfficeInstance'

// Binds the reusable working-calendar editor to a project: it loads the calendar,
// saves what the editor reports, and replans afterwards — the editor itself stays
// free of any of that, so it can serve another entity later.
export default function ProjectCalendarSection({ projectId }: { projectId: string }) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()
  const inst = useOfficeInstance()

  const { data, isLoading } = useQuery({
    queryKey: ['project-calendars', projectId],
    queryFn: () => projectsApi.listCalendars(projectId),
  })
  // The import window starts at the project, not at today: a plan that begins
  // next year needs next year's holidays.
  const { data: projectData } = useQuery({
    queryKey: ['project', projectId],
    queryFn: () => projectsApi.get(projectId),
  })
  const project = projectData?.project

  // Changing when work happens changes every date, so the schedule is recomputed
  // right after saving — otherwise the plan would keep showing the old dates.
  const afterChange = async () => {
    try { await projectsApi.computeCpm(projectId) } catch { /* the plan may be unschedulable; the editor already said so */ }
    qc.invalidateQueries({ queryKey: ['project-calendars', projectId] })
    qc.invalidateQueries({ queryKey: ['project', projectId] })
  }

  const active = useMemo(
    () => data?.calendars.find(c => c.id === data.project_calendar_id) ?? data?.calendars[0] ?? null,
    [data],
  )

  const createMut = useMutation({
    mutationFn: async () => {
      const cal = await projectsApi.createCalendar(projectId, t('cal_default_name', { defaultValue: 'Calendrier du projet' }))
      // Seed public holidays too when the administrator's default asks for it — the
      // working week itself was already seeded server-side at creation.
      if (inst.projectExcludeHolidays) {
        try {
          const from = project?.start_date ?? format(new Date(), 'yyyy-MM-dd')
          const to = format(addMonths(new Date(from), HOLIDAY_HORIZON_MONTHS), 'yyyy-MM-dd')
          const holidays = await holidaysApi.list(from, to, i18n.language)
          for (const h of holidays) {
            await projectsApi.setCalendarException(projectId, cal.id, { day: h.date, is_working: false, note: h.name })
          }
        } catch { /* holidays are optional; the calendar is created regardless */ }
      }
      return cal
    },
    onSuccess: afterChange,
  })
  const weekMut = useMutation({
    mutationFn: (week: WorkingWeek) =>
      projectsApi.updateCalendar(projectId, active!.id, { ...week, set_as_project_default: true }),
    onSuccess: afterChange,
  })
  const nameMut = useMutation({
    mutationFn: (name: string) => projectsApi.updateCalendar(projectId, active!.id, { name }),
    onSuccess: afterChange,
  })
  const setExMut = useMutation({
    mutationFn: (ex: CalendarException) =>
      projectsApi.setCalendarException(projectId, active!.id, { day: ex.day, is_working: ex.is_working, note: ex.note }),
    onSuccess: afterChange,
  })
  const removeExMut = useMutation({
    mutationFn: (day: string) => projectsApi.removeCalendarException(projectId, active!.id, day),
    onSuccess: afterChange,
  })

  // Public holidays are written into the plan as ordinary exceptions rather than
  // resolved live at every computation: a plan is a commitment, and the days it
  // does not work belong to it — visible, editable, and stable even if the
  // referential is later corrected.
  const importMut = useMutation({
    mutationFn: async () => {
      const from = project?.start_date ?? format(new Date(), 'yyyy-MM-dd')
      const to = format(addMonths(new Date(from), HOLIDAY_HORIZON_MONTHS), 'yyyy-MM-dd')
      const holidays = await holidaysApi.list(from, to, i18n.language)
      for (const h of holidays) {
        await projectsApi.setCalendarException(projectId, active!.id, {
          day: h.date, is_working: false, note: h.name,
        })
      }
      return holidays.length
    },
    onSuccess: afterChange,
  })

  if (isLoading) {
    return <p className="text-sm text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</p>
  }

  // No calendar yet: say what the project falls back on, and offer to make one.
  if (!active) {
    return (
      <div className="bg-surface-0 border border-border rounded-xl p-5">
        <h2 className="text-sm font-semibold text-text-primary">{t('cal_section_title', { defaultValue: 'Calendrier ouvré' })}</h2>
        <p className="text-xs text-text-tertiary mt-0.5 mb-3">
          {inst.projectIncludeWeekends
            ? t('cal_none_hint_we', { defaultValue: 'Par défaut (choix de l’administrateur), ce projet compte tous les jours, week-ends inclus. Créez un calendrier pour ajuster la semaine travaillée ou déclarer des jours chômés.' })
            : t('cal_none_hint', { defaultValue: 'Par défaut (choix de l’administrateur), ce projet compte du lundi au vendredi — week-ends exclus. Créez un calendrier pour changer la semaine travaillée ou déclarer des jours chômés.' })}
          {inst.projectExcludeHolidays && ' ' + t('cal_none_hint_hol', { defaultValue: 'Les jours fériés seront aussi retirés à la création.' })}
        </p>
        <button
          onClick={() => createMut.mutate()}
          disabled={createMut.isPending}
          className="text-sm text-[var(--color-primary)] hover:underline disabled:opacity-50"
        >
          {t('cal_create', { defaultValue: 'Créer un calendrier' })}
        </button>
      </div>
    )
  }

  const week: WorkingWeek = {
    mon: active.mon, tue: active.tue, wed: active.wed, thu: active.thu,
    fri: active.fri, sat: active.sat, sun: active.sun,
  }
  const exceptions: CalendarException[] = (data?.exceptions ?? [])
    .filter(e => e.calendar_id === active.id)
    .map(e => ({ day: e.day, is_working: e.is_working, note: e.note }))

  const busy = weekMut.isPending || setExMut.isPending || removeExMut.isPending || nameMut.isPending

  return (
    <div className="bg-surface-0 border border-border rounded-xl p-5">
      <h2 className="text-sm font-semibold text-text-primary">{t('cal_section_title', { defaultValue: 'Calendrier ouvré' })}</h2>
      <p className="text-xs text-text-tertiary mt-0.5 mb-3">
        {t('cal_section_hint', { defaultValue: 'Les durées se comptent en jours travaillés : le planning saute les jours chômés.' })}
      </p>
      <div className="mb-3">
        <Button
          size="sm" variant="secondary" icon={<CalendarPlus size={14} />}
          loading={importMut.isPending}
          onClick={() => importMut.mutate()}
        >
          {t('cal_import_holidays', { defaultValue: 'Importer les jours fériés' })}
        </Button>
        <p className="text-xs text-text-tertiary mt-1">
          {t('cal_import_hint', { defaultValue: 'Ajoute les jours fériés du pays de l’instance comme jours chômés. Vous pouvez ensuite en rouvrir un.' })}
        </p>
      </div>
      <WorkingCalendarEditor
        week={week}
        exceptions={exceptions}
        disabled={busy}
        name={active.name}
        onNameChange={name => nameMut.mutate(name)}
        onWeekChange={w => weekMut.mutate(w)}
        onExceptionSet={ex => setExMut.mutate(ex)}
        onExceptionRemove={day => removeExMut.mutate(day)}
      />
    </div>
  )
}

// How far ahead holidays are imported. Two years covers the horizon of almost
// every plan; beyond that the button can simply be pressed again.
const HOLIDAY_HORIZON_MONTHS = 24
