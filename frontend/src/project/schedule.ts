import { addDays } from 'date-fns'
import type { ProjectTask } from '../api'

// Scheduled dates consistent with the Gantt bar (derived from the CPM offset).
export function schedStart(task: ProjectTask, projectStart: Date): Date {
  return addDays(projectStart, task.early_start ?? 0)
}
export function schedEnd(task: ProjectTask, projectStart: Date): Date {
  const ef = (task.early_finish ?? ((task.early_start ?? 0) + task.duration_days))
  return addDays(projectStart, Math.max(0, ef - 1))
}
