// JS mirror of the backend's WorkingCalendar (office/src/services/working_calendar.rs),
// in the same day-offset space, so the Gantt can predict EXACTLY where the CPM will
// put a bar before the server answers — drops settle on their final day, no hop.
//
// Everything is expressed in day offsets relative to a reference date (the project
// start), matching the scheduler; converting a real date is the caller's job.

const MAX_SCAN = 3660   // ~10 years — same bound as the Rust service

export class WorkingCal {
  /** Monday first, Sunday last. */
  private week: boolean[]
  /** Day offset → worked or not, overriding the weekly pattern. */
  private exceptions: Map<number, boolean>
  /** Weekday (0=Mon … 6=Sun) of the reference date, to resolve offsets locally. */
  private refDow: number

  constructor(week: boolean[], exceptions: Map<number, boolean>, reference: Date) {
    this.week = week
    this.exceptions = exceptions
    // JS getDay(): 0=Sun … 6=Sat → shift to 0=Mon … 6=Sun.
    this.refDow = (reference.getDay() + 6) % 7
  }

  /** Every day worked — the identity calendar (offsets pass through untouched). */
  static allDays(reference: Date): WorkingCal {
    return new WorkingCal([true, true, true, true, true, true, true], new Map(), reference)
  }

  isWorking(offset: number): boolean {
    const forced = this.exceptions.get(offset)
    if (forced !== undefined) return forced
    return this.week[(((this.refDow + offset) % 7) + 7) % 7]
  }

  /** The first worked day at or after `offset`. */
  nextWorking(offset: number): number {
    for (let o = offset; o < offset + MAX_SCAN; o++) if (this.isWorking(o)) return o
    return offset
  }

  /** Exclusive finish of a task starting at `start` lasting `workingDays` worked days. */
  advance(start: number, workingDays: number): number {
    if (workingDays <= 0) return start   // a milestone sits on its day
    let remaining = workingDays
    const from = this.nextWorking(start)
    for (let o = from; o < from + MAX_SCAN; o++) {
      if (this.isWorking(o)) { remaining--; if (remaining === 0) return o + 1 }
    }
    return start + workingDays
  }

  /** Working days in the half-open range `[start, finish)`. */
  workingDaysBetween(start: number, finish: number): number {
    let n = 0
    for (let o = start; o < finish; o++) if (this.isWorking(o)) n++
    return n
  }
}

/** Build the calendar the SCHEDULER uses for this project, from the same inputs:
 *  the project's active calendar row + its exceptions, or — when the project has
 *  none — the instance default (Mon–Fri, weekends per the administrator's choice).
 *  Mirrors compute_cpm's fallback in office/src/handlers/projects.rs. */
export function calendarFromApi(
  data: {
    calendars: { id: string; mon: boolean; tue: boolean; wed: boolean; thu: boolean; fri: boolean; sat: boolean; sun: boolean }[]
    exceptions: { calendar_id: string; day: string; is_working: boolean }[]
    project_calendar_id: string | null
  } | undefined,
  projectStart: Date,
  includeWeekendsDefault: boolean,
): WorkingCal {
  const active = data?.calendars.find(c => c.id === data.project_calendar_id) ?? data?.calendars[0]
  if (!active) {
    const we = includeWeekendsDefault
    return new WorkingCal([true, true, true, true, true, we, we], new Map(), projectStart)
  }
  const week = [active.mon, active.tue, active.wed, active.thu, active.fri, active.sat, active.sun]
  const ex = new Map<number, boolean>()
  const t0 = Date.UTC(projectStart.getFullYear(), projectStart.getMonth(), projectStart.getDate())
  for (const e of data!.exceptions) {
    if (e.calendar_id !== active.id) continue
    const [y, m, d] = e.day.split('-').map(Number)
    ex.set(Math.round((Date.UTC(y, m - 1, d) - t0) / 86400000), e.is_working)
  }
  return new WorkingCal(week, ex, projectStart)
}
