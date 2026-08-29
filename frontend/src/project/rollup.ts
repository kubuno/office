import type { ProjectTask } from '../api'

/**
 * Effective completion of every task in the project.
 *
 * A leaf keeps its own stored `progress`. A parent (any task that has children,
 * whatever its stored type) does NOT: once a task becomes a summary its manually
 * entered percentage stops being meaningful, so it is replaced for display by a
 * roll-up of its DIRECT children — duration-weighted, so a long child counts for
 * more than a short one. The roll-up is recursive: a child that is itself a
 * parent already carries its own rolled-up value, so aggregating the direct
 * children covers the whole subtree.
 *
 * The stored value is left untouched in the database; this is a view-only
 * derivation. Callers read `map.get(id)` and fall back to `task.progress` only
 * for tasks the map does not cover (it always covers every task, so the fallback
 * is just defensive).
 */
export function effectiveProgress(tasks: ProjectTask[]): Map<string, number> {
  const childrenOf = new Map<string, ProjectTask[]>()
  for (const t of tasks) {
    const p = t.parent_id
    if (!p) continue
    const arr = childrenOf.get(p)
    if (arr) arr.push(t); else childrenOf.set(p, [t])
  }

  const out = new Map<string, number>()
  const inProgress = new Set<string>()   // breaks any accidental parent cycle

  const visit = (t: ProjectTask): number => {
    const cached = out.get(t.id)
    if (cached !== undefined) return cached
    if (inProgress.has(t.id)) return t.progress   // cycle guard
    const kids = childrenOf.get(t.id)
    if (!kids || kids.length === 0) {
      out.set(t.id, t.progress)
      return t.progress
    }
    inProgress.add(t.id)
    let weighted = 0, weight = 0, plain = 0
    for (const c of kids) {
      const cp = visit(c)
      const w = Math.max(0, c.duration_days || 0)
      weighted += cp * w; weight += w; plain += cp
    }
    inProgress.delete(t.id)
    // Duration-weighted; if every child is a zero-length milestone the weight is
    // 0, so fall back to a plain average — the number stays meaningful.
    const val = weight > 0 ? Math.round(weighted / weight) : Math.round(plain / kids.length)
    out.set(t.id, val)
    return val
  }

  for (const t of tasks) visit(t)
  return out
}

/** The ids of every task that has at least one child. */
export function parentIds(tasks: ProjectTask[]): Set<string> {
  const s = new Set<string>()
  for (const t of tasks) if (t.parent_id) s.add(t.parent_id)
  return s
}
