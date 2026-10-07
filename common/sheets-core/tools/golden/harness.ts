// Golden-vector harness: runs the WEB engine (office/web/src/formula-engine.ts) on test cases and prints
// what it computes, so the Rust port (kubuno-office-sheets-core) can be checked against the real thing.
//
//   cd office/common/sheets-core/tools/golden
//   TZ=UTC node ../../../../web/node_modules/jiti/lib/jiti-cli.mjs harness.ts cases/<file>.json > ../../tests/golden/<file>.json
//
// A case file is a JSON array of { "name", "cells"?: {"A1": {"v"|"f"}}, "names"?: {"NAME": "=…"},
// "sheets"?: {"Sheet": {"A1": …}}, "formulas": ["=…", …] }. The clock is fixed at 2026-10-07 14:30:15.250
// (TZ=UTC, so local time is the zone-less model of the port) and Math.random is a fixed sequence.
import { readFileSync } from 'node:fs'
import { evaluate, computeSpills, type Value } from '../../../../web/src/formula-engine'

const FIXED = Date.UTC(2026, 9, 7, 14, 30, 15, 250)
const RealDate = Date
class FixedDate extends RealDate {
  constructor(...a: unknown[]) {
    if (a.length === 0) super(FIXED)
    else super(...(a as [number]))
  }
  static now() { return FIXED }
}
;(globalThis as unknown as { Date: DateConstructor }).Date = FixedDate as unknown as DateConstructor
let seed = 42
Math.random = () => { seed = (seed * 16807) % 2147483647; return (seed - 1) / 2147483646 }

type Enc = { n: number | string } | { s: string } | { b: boolean } | { e: string } | { m: Enc[][] } | { lambda: true } | { u: true }
function enc(v: unknown): Enc {
  if (Array.isArray(v)) return { m: (v as unknown[][]).map(r => (Array.isArray(r) ? r : [r]).map(enc)) }
  if (typeof v === 'number') return { n: Number.isFinite(v) ? v : String(v) }
  if (typeof v === 'string') return { s: v }
  if (typeof v === 'boolean') return { b: v }
  if (v && typeof v === 'object' && 'err' in v) return { e: (v as { err: string }).err }
  if (v && typeof v === 'object' && (v as { kind?: string }).kind === 'lambda') return { lambda: true }
  return { u: true }
}

interface Call { module: string; fn: string; args: unknown[] }
interface Case { name: string; cells?: Record<string, unknown>; names?: Record<string, string>; sheets?: Record<string, Record<string, unknown>>; spill?: boolean; formulas?: string[]; calls?: Call[] }
const cases: Case[] = JSON.parse(readFileSync(process.argv[2], 'utf8'))

// A case with `calls` runs exported functions of other web modules (fill-series, formula-refs, …):
// `{ "module": "fill-series", "fn": "fillSeries", "args": [...] }` → `{ call, json }` (the return value as
// JSON; NaN/Infinity become null like JSON.stringify).
const modules: Record<string, Record<string, (...a: unknown[]) => unknown>> = {}
for (const c of cases) for (const call of c.calls ?? []) {
  if (!modules[call.module]) modules[call.module] = await import(`../../../../web/src/${call.module}`)
}

const out = cases.map(c => {
  if (c.calls) {
    return {
      name: c.name,
      results: c.calls.map(call => {
        try {
          const f = modules[call.module][call.fn]
          return { call, json: JSON.parse(JSON.stringify(f(...call.args)) ?? 'null') }
        } catch (e) { return { call, threw: String(e) } }
      }),
    }
  }
  const data = { cells: (c.cells ?? {}) as never, names: c.names ?? {}, sheets: c.sheets as never }
  const spill = c.spill ? computeSpills(data as never) : undefined
  return {
    name: c.name,
    results: (c.formulas ?? []).map(f => {
      let r: Value | undefined
      try { r = evaluate(f, data as never, new Set(), spill) } catch (e) { return { formula: f, threw: String(e) } }
      return { formula: f, value: enc(r) }
    }),
  }
})
// One case per line: diffs of the expected files stay readable.
process.stdout.write('[\n' + out.map(c => JSON.stringify(c)).join(',\n') + '\n]\n')
