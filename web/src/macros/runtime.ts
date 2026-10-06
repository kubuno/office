// Client-side macro runtime. Document-bound macros (the `office_script.macros` rows)
// run HERE, in the browser, against the LIVE open document — like Apps Script's
// container-bound scripts: edits are immediate. (Standalone automation scripts with
// cron/webhook/event triggers keep running server-side in QuickJS; this runtime is
// only for the per-document macros surfaced in each editor's Macros menu.)
//
// The macro source is executed with two injected globals:
//   • `Kubuno` — the per-document API object provided by the host editor
//   • `console` — captured (log/warn/error) so we can show the output
// The body is wrapped in an async IIFE, so macros may `await` and `return` a value.

export interface MacroLog { level: 'log' | 'warn' | 'error'; text: string }
export interface MacroResult {
  ok: boolean
  logs: MacroLog[]
  error?: string
  returnValue?: unknown
  durationMs: number
}

function stringify(args: unknown[]): string {
  return args.map(a => {
    if (typeof a === 'string') return a
    try { return JSON.stringify(a) } catch { return String(a) }
  }).join(' ')
}

export async function runMacro(source: string, kubuno: unknown, startedAt: number): Promise<MacroResult> {
  const logs: MacroLog[] = []
  const sandboxConsole = {
    log:   (...a: unknown[]) => logs.push({ level: 'log',   text: stringify(a) }),
    warn:  (...a: unknown[]) => logs.push({ level: 'warn',  text: stringify(a) }),
    error: (...a: unknown[]) => logs.push({ level: 'error', text: stringify(a) }),
    info:  (...a: unknown[]) => logs.push({ level: 'log',   text: stringify(a) }),
  }
  try {
    // eslint-disable-next-line no-new-func
    const fn = new Function('Kubuno', 'console', `"use strict"; return (async () => {\n${source}\n})()`)
    const returnValue = await fn(kubuno, sandboxConsole)
    return { ok: true, logs, returnValue, durationMs: Math.max(0, Date.now() - startedAt) }
  } catch (e) {
    const error = e instanceof Error ? (e.stack || e.message) : String(e)
    return { ok: false, logs, error, durationMs: Math.max(0, Date.now() - startedAt) }
  }
}
