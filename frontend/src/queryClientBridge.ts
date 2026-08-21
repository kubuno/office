/**
 * Bridges the host's React Query client to non-React code (the "New" menu item
 * builders are plain functions, not components, so `useQueryClient()` is out of
 * reach there). `OfficeSidebarBody` — mounted whenever the Office sidebar, and
 * therefore the "New" button, is available — captures the client on render.
 */
import type { QueryClient } from '@tanstack/react-query'

let client: QueryClient | null = null

export function bindQueryClient(qc: QueryClient): void {
  client = qc
}

/** Invalidate a query key on the host's client; no-op until it was captured
 *  (the affected list queries then simply refetch when they go stale). */
export function invalidateQueries(queryKey: unknown[]): void {
  void client?.invalidateQueries({ queryKey })
}
