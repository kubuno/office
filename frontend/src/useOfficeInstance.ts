import { useQuery } from '@tanstack/react-query'
import { api } from '@kubuno/sdk'

// Editor-side instance defaults, as the administrator left them in the console.
//
// Only the settings that act on the RUNNING editor live here — today, the
// autosave cadences. They are declared `public` in office's module.toml, which
// is what puts them in `/api/v1/config` under `office.<key>` — the only config
// route a non-admin user's editor may read. A missing key falls back to the
// compiled default, never to a permissive value.
//
// Page margins, paper size and track-changes used to be read here too, against
// keys the manifest never declared: they always resolved to the constant below
// and the console never moved them. They are now stamped by the backend into a
// new document's layout envelope (`stamp_document_layout`), which is where a
// per-document default belongs — so they are gone from this hook rather than
// left as three reads of nothing.

export interface OfficeInstance {
  autosaveIntervalS:     number
  spreadsheetAutosaveS:  number
  mathsAutosaveS:        number
  scriptAutosaveS:       number
}

const DEFAULTS: OfficeInstance = {
  autosaveIntervalS:    30,
  spreadsheetAutosaveS: 30,
  mathsAutosaveS:       30,
  scriptAutosaveS:      30,
}

export function useOfficeInstance(): OfficeInstance {
  const { data } = useQuery({
    queryKey: ['office-instance-config'],
    queryFn: async () => {
      const res = await api.get<{ config: Record<string, unknown> }>('/config')
      return res.data.config ?? {}
    },
    // Instance config changes rarely; a few minutes stale is harmless and keeps
    // every editor from re-fetching on mount.
    staleTime: 5 * 60_000,
  })
  const cfg = data ?? {}
  const num = (k: string, d: number) => {
    const v = cfg[`office.${k}`]
    return typeof v === 'number' && Number.isFinite(v) ? v : d
  }
  return {
    autosaveIntervalS:    num('autosave_interval_s', DEFAULTS.autosaveIntervalS),
    spreadsheetAutosaveS: num('spreadsheet_autosave_s', DEFAULTS.spreadsheetAutosaveS),
    mathsAutosaveS:       num('maths_autosave_s', DEFAULTS.mathsAutosaveS),
    scriptAutosaveS:      num('script_autosave_s', DEFAULTS.scriptAutosaveS),
  }
}
