// Golden vectors for the chart renderer: renders every case of a cases file through the web's own
// `ChartView` (office/web/src/sheet-chart) with react-dom/server, then flattens the SVG into a list of
// drawing elements `{ tag, attrs, text }` in document order (`<g>`, `<defs>`, `<clipPath>` and their ids
// dropped; the clip rectangle is kept as a `clip` element). The Rust port (`sheets-core::chart`) emits
// the same list; `tests/golden_charts.rs` compares them.
//
// Run on the Linux host from office/web (jiti + the JSX transform, React in global scope for the
// classic runtime the web modules do not import):
//   cd ~/projects/kubuno/office/web && TZ=UTC LANG=en_US.UTF-8 NODE_PATH=$PWD/node_modules JITI_JSX=1 \
//     node node_modules/jiti/lib/jiti-cli.mjs <dir>/charts-harness.tsx <cases.json> > <out.json>
import * as React from 'react'
;(globalThis as unknown as { React: typeof React }).React = React
import { readFileSync } from 'node:fs'

type Cells = Record<string, string | number | boolean | null>
interface Case { name: string; chart: Record<string, unknown>; width: number; height: number; cells: Cells }

async function main() {
  const { renderToStaticMarkup } = await import('react-dom/server')
  const { ChartView } = await import('../../../../web/src/sheet-chart/ChartView')
  const cases: Case[] = JSON.parse(readFileSync(process.argv[2], 'utf8'))
  const out: unknown[] = []
  for (const c of cases) {
    const getCell = (ref: string) => c.cells[ref] ?? ''
    const svg = renderToStaticMarkup(React.createElement(ChartView, {
      chart: c.chart as never, width: c.width, height: c.height, getCell,
      t: (_k: string, o?: { defaultValue?: string }) => o?.defaultValue ?? '',
    }))
    out.push({ name: c.name, els: flatten(svg) })
  }
  process.stdout.write(JSON.stringify(out, null, 1) + '\n')
}

const decode = (s: string) => s.replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

/** A minimal SVG tokenizer: React's static markup is well-formed and attribute values are quoted. */
function flatten(svg: string) {
  const els: { tag: string; attrs: Record<string, string>; text?: string }[] = []
  const re = /<(\/?)([a-zA-Z]+)((?:\s+[a-zA-Z0-9:-]+="[^"]*")*)\s*(\/?)>|([^<]+)/g
  let m: RegExpExecArray | null
  let inClip = false
  let current: { tag: string; attrs: Record<string, string>; text?: string } | null = null
  while ((m = re.exec(svg))) {
    if (m[5] !== undefined) {
      if (current && current.tag === 'text') current.text = (current.text ?? '') + decode(m[5])
      continue
    }
    const closing = m[1] === '/', tag = m[2]
    if (closing) {
      if (tag === 'clipPath') inClip = false
      current = null
      continue
    }
    if (tag === 'clipPath') { inClip = true; continue }
    if (tag === 'g' || tag === 'defs' || tag === 'svg') continue
    const attrs: Record<string, string> = {}
    const ar = /([a-zA-Z0-9:-]+)="([^"]*)"/g
    let a: RegExpExecArray | null
    while ((a = ar.exec(m[3]))) {
      if (a[1] === 'id' || a[1] === 'clip-path' || a[1] === 'aria-label') continue
      attrs[a[1]] = decode(a[2])
    }
    const el = { tag: inClip ? 'clip' : tag, attrs }
    els.push(el)
    current = m[4] === '/' ? null : el
  }
  return els
}

main().catch(e => { console.error(e); process.exit(1) })
