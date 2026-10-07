// Extracts KaTeX's data tables (font metrics, symbol table, spacing, sigmas) into data/*.json for the Rust port.
// Run on the Linux host from office/common/math-core/tools:
//   node ../../../web/node_modules/jiti/lib/jiti-cli.mjs extract.ts
import { writeFileSync } from 'node:fs'
import metricMap from '../../../web/node_modules/katex/src/fontMetricsData'
import symbols from '../../../web/node_modules/katex/src/symbols'
import { spacings, tightSpacings } from '../../../web/node_modules/katex/src/spacingData'
import * as fm from '../../../web/node_modules/katex/src/fontMetrics'

const out = '../data/'
writeFileSync(out + 'metrics.json', JSON.stringify(metricMap))
writeFileSync(out + 'symbols.json', JSON.stringify(symbols))
writeFileSync(out + 'spacing.json', JSON.stringify({ spacings, tightSpacings }))
// Sigmas: the three size classes.
const sig = [fm.getGlobalMetrics(5), fm.getGlobalMetrics(3), fm.getGlobalMetrics(1)]
writeFileSync(out + 'sigmas.json', JSON.stringify(sig))
console.log('ok', Object.keys(metricMap).length, Object.keys(symbols.math).length, Object.keys(symbols.text).length)
import { path as svgPaths } from '../../../web/node_modules/katex/src/svgGeometry'
writeFileSync(out + 'svgpaths.json', JSON.stringify(svgPaths))
import macros from '../../../web/node_modules/katex/src/macros'
const strMacros: Record<string, string> = {}
const fnMacros: string[] = []
for (const [k, v] of Object.entries(macros as Record<string, unknown>)) {
  if (typeof v === 'string') strMacros[k] = v
  else if (v && typeof v === 'object' && 'tokens' in (v as object)) fnMacros.push(k + ' (tokens)')
  else fnMacros.push(k)
}
writeFileSync(out + 'macros.json', JSON.stringify(strMacros))
console.log('function macros:', fnMacros.join(' '))
