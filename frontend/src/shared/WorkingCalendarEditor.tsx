/*
 * WorkingCalendarEditor — a self-contained editor for a "working calendar":
 * which weekdays are worked, plus the individual days that break the rule
 * (public holidays, closures, caught-up Saturdays…).
 *
 * DELIBERATELY STANDALONE — this component is a candidate for promotion into the
 * shared `@kubuno/ui` library, so it must stay portable:
 *   - it is driven ONLY by its props (no store, no context, no router);
 *   - it knows nothing about projects, documents or any office-specific concept;
 *   - it performs NO network call and owns no server state — the parent decides
 *     what to persist and when;
 *   - its only dependencies are `@ui` primitives, `@kubuno/sdk` (date locale),
 *     `date-fns`, `lucide-react` and i18n, all of which exist in every module.
 * Local state is limited to the "add an exception" draft row, which is pure UI.
 */
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { format, parseISO, isValid } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Badge, Button, Checkbox, Dropdown, Input } from '@ui'
import { Trash2, Plus } from 'lucide-react'

export interface WorkingWeek { mon: boolean; tue: boolean; wed: boolean; thu: boolean; fri: boolean; sat: boolean; sun: boolean }
export interface CalendarException { day: string; is_working: boolean; note: string }   // day = 'YYYY-MM-DD'

export interface WorkingCalendarEditorProps {
  week: WorkingWeek
  exceptions: CalendarException[]
  onWeekChange: (week: WorkingWeek) => void
  onExceptionSet: (ex: CalendarException) => void      // add OR update (keyed by day)
  onExceptionRemove: (day: string) => void
  disabled?: boolean
  name?: string
  onNameChange?: (name: string) => void
}

type DayKey = keyof WorkingWeek

/** Monday-first order — the ISO week, and the one French users expect. */
const DAY_KEYS: DayKey[] = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun']

/** Fields shared by the add-row: same height / same font size as `<Input>` (h-9, 14px). */
const FIELD_HEIGHT = 36
const FIELD_FONT_SIZE = 14

export default function WorkingCalendarEditor({
  week,
  exceptions,
  onWeekChange,
  onExceptionSet,
  onExceptionRemove,
  disabled = false,
  name,
  onNameChange,
}: WorkingCalendarEditorProps) {
  const { t, i18n } = useTranslation('office')
  const locale = getDateLocale(i18n.language)

  // Draft of the exception being composed. Cleared once it is handed to the parent.
  const [draftDay, setDraftDay] = useState('')
  const [draftWorking, setDraftWorking] = useState<'off' | 'on'>('off')
  const [draftNote, setDraftNote] = useState('')

  const dayLabel: Record<DayKey, string> = {
    mon: t('cal_day_mon', { defaultValue: 'Lun' }),
    tue: t('cal_day_tue', { defaultValue: 'Mar' }),
    wed: t('cal_day_wed', { defaultValue: 'Mer' }),
    thu: t('cal_day_thu', { defaultValue: 'Jeu' }),
    fri: t('cal_day_fri', { defaultValue: 'Ven' }),
    sat: t('cal_day_sat', { defaultValue: 'Sam' }),
    sun: t('cal_day_sun', { defaultValue: 'Dim' }),
  }

  // A calendar with no working day at all is meaningless — and the server rejects
  // it (422). The last day still ticked is therefore locked instead of unticked.
  const workedCount = DAY_KEYS.filter(k => week[k]).length
  const isLastWorkedDay = (k: DayKey) => week[k] && workedCount === 1

  const sorted = useMemo(
    () => [...exceptions].sort((a, b) => a.day.localeCompare(b.day)),
    [exceptions],
  )

  /** 'YYYY-MM-DD' → 'lundi 3 mars 2026'. Falls back to the raw value if unparsable. */
  const formatDay = (day: string) => {
    const parsed = parseISO(day)
    return isValid(parsed) ? format(parsed, 'EEEE d MMMM yyyy', { locale }) : day
  }

  const addException = () => {
    if (!draftDay) return
    onExceptionSet({ day: draftDay, is_working: draftWorking === 'on', note: draftNote.trim() })
    setDraftDay('')
    setDraftNote('')
    setDraftWorking('off')
  }

  const kindOptions = [
    { value: 'off', label: t('cal_exc_off', { defaultValue: 'Chômé' }) },
    { value: 'on', label: t('cal_exc_on', { defaultValue: 'Travaillé' }) },
  ]

  return (
    <div className="flex flex-col gap-5">
      {/* Optional calendar name — only when the parent wires it up. */}
      {onNameChange && (
        <div className="flex flex-col gap-1.5">
          <span className="text-sm font-medium text-text-primary">
            {t('cal_name_label', { defaultValue: 'Nom du calendrier' })}
          </span>
          <Input
            value={name ?? ''}
            onChange={e => onNameChange(e.target.value)}
            disabled={disabled}
            placeholder={t('cal_name_placeholder', { defaultValue: 'Calendrier standard' })}
            className="max-w-sm"
          />
        </div>
      )}

      {/* ---- Working week ---- */}
      <div className="flex flex-col gap-2">
        <span className="text-sm font-medium text-text-primary">
          {t('cal_week_label', { defaultValue: 'Jours travaillés' })}
        </span>
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
          {DAY_KEYS.map(k => {
            const locked = isLastWorkedDay(k)
            return (
              <span
                key={k}
                // `title` sits on the wrapper so it still shows while the checkbox is disabled.
                title={locked ? t('cal_week_last_day', { defaultValue: 'Au moins un jour travaillé est requis.' }) : undefined}
              >
                <Checkbox
                  checked={week[k]}
                  onChange={(checked: boolean) => onWeekChange({ ...week, [k]: checked })}
                  label={dayLabel[k]}
                  disabled={disabled || locked}
                />
              </span>
            )
          })}
        </div>
      </div>

      {/* ---- Exceptions ---- */}
      <div className="flex flex-col gap-2">
        <span className="text-sm font-medium text-text-primary">
          {t('cal_exceptions_label', { defaultValue: 'Exceptions' })}
        </span>

        {sorted.length === 0 ? (
          <p className="text-sm text-text-tertiary">
            {t('cal_exceptions_empty', { defaultValue: 'Aucune exception : la semaine type s’applique toute l’année.' })}
          </p>
        ) : (
          <ul className="flex flex-col rounded-lg border border-border overflow-hidden">
            {sorted.map(ex => (
              <li
                key={ex.day}
                className="flex items-center gap-3 px-3 py-2 border-b border-border last:border-b-0 bg-white"
              >
                <span className="text-sm text-text-primary first-letter:uppercase min-w-0 truncate">
                  {formatDay(ex.day)}
                </span>
                <Badge variant={ex.is_working ? 'success' : 'neutral'} size="sm">
                  {ex.is_working
                    ? t('cal_exc_on', { defaultValue: 'Travaillé' })
                    : t('cal_exc_off', { defaultValue: 'Chômé' })}
                </Badge>
                {ex.note && (
                  <span className="text-sm text-text-tertiary min-w-0 truncate">{ex.note}</span>
                )}
                <button
                  type="button"
                  onClick={() => onExceptionRemove(ex.day)}
                  disabled={disabled}
                  aria-label={t('common_delete', { defaultValue: 'Supprimer' })}
                  title={t('common_delete', { defaultValue: 'Supprimer' })}
                  className="ml-auto shrink-0 p-1.5 rounded text-text-tertiary hover:bg-surface-2 hover:text-danger disabled:opacity-40 disabled:pointer-events-none"
                >
                  <Trash2 size={15} />
                </button>
              </li>
            ))}
          </ul>
        )}

        {/* Add row — every field shares the same height and font size. */}
        <div className="grid grid-cols-[minmax(0,11rem)_minmax(0,9rem)_minmax(0,1fr)_auto] items-center gap-2">
          <Input
            type="date"
            value={draftDay}
            onChange={e => setDraftDay(e.target.value)}
            disabled={disabled}
            aria-label={t('cal_exc_date', { defaultValue: 'Date' })}
          />
          <Dropdown
            value={draftWorking}
            onChange={v => setDraftWorking(v === 'on' ? 'on' : 'off')}
            options={kindOptions}
            disabled={disabled}
            width="100%"
            height={FIELD_HEIGHT}
            fontSize={FIELD_FONT_SIZE}
            focusable
          />
          <Input
            value={draftNote}
            onChange={e => setDraftNote(e.target.value)}
            disabled={disabled}
            placeholder={t('cal_exc_note_placeholder', { defaultValue: 'Note (facultative)' })}
            aria-label={t('cal_exc_note', { defaultValue: 'Note' })}
          />
          <Button
            variant="secondary"
            icon={<Plus size={15} />}
            onClick={addException}
            disabled={disabled || !draftDay}
            title={!draftDay ? t('cal_exc_need_date', { defaultValue: 'Choisissez d’abord une date.' }) : undefined}
          >
            {t('common_add', { defaultValue: 'Ajouter' })}
          </Button>
        </div>
      </div>
    </div>
  )
}
