// Interop vectors of the collaboration module, produced by the web's own libraries (yjs, y-protocols).
//
//   node collab.mjs <office/web dir> <collab_from_rust.json> > ../../tests/golden/collab_web.json
//
// It writes the spreadsheet schema the way SpreadsheetApp.tsx does (cells:<sheet> maps of plain JSON, the
// UPPERCASE `names` map, an awareness state with user/sel/cursor), and reads back what the Rust side wrote.
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import path from 'node:path'

const [webDir, rustFile] = process.argv.slice(2)
const require = createRequire(path.join(webDir, 'package.json'))
const Y = require('yjs')
const { Awareness, encodeAwarenessUpdate, applyAwarenessUpdate } = require('y-protocols/awareness')

const b64 = (u8) => Buffer.from(u8).toString('base64')
const unb64 = (s) => new Uint8Array(Buffer.from(s, 'base64'))

// ── the web writes ──────────────────────────────────────────────────────────
const doc = new Y.Doc()
doc.clientID = 4242
const cells = doc.getMap('cells:s1')
const names = doc.getMap('names')
const webCells = {
  A1: { v: 12000, s: { numFmt: 'currency', bold: true } },
  B1: { f: '=A1*TAUX' },
  C1: { v: 0.1 },
  D1: { v: 'Zoé ✓' },
  E1: { v: -3.5e-7, c: 'note' },
  F1: { v: 2 ** 40 },
  G1: { v: true },
  H1: { v: null },
}
doc.transact(() => {
  for (const [k, c] of Object.entries(webCells)) cells.set(k, JSON.parse(JSON.stringify(c)))
  names.set('TAUX', '=0.2')
}, 'local')
// A second, incremental update: one edit, one deletion (sent as separate frames by the web).
const firstUpdate = Y.encodeStateAsUpdate(doc)
const sv = Y.encodeStateVector(doc)
doc.transact(() => { cells.set('C1', { v: 0.25 }); cells.delete('H1') }, 'local')
const incremental = Y.encodeStateAsUpdate(doc, sv)
const finalCells = {}
cells.forEach((v, k) => { finalCells[k] = v })

const aw = new Awareness(doc)
aw.setLocalStateField('user', { id: 'u-web', name: 'Web Zoé', color: '#d93025', avatar: null })
aw.setLocalStateField('sel', { col: 'C', row: 4, sheet: 's1' })
aw.setLocalStateField('cursor', { x: 120.5, y: 48 })
const webAwareness = encodeAwarenessUpdate(aw, [doc.clientID])
const webAwState = aw.getLocalState()
const webAwClock = aw.meta.get(doc.clientID).clock
aw.destroy()

// ── the web reads what Rust wrote ───────────────────────────────────────────
let rustRead = null
if (rustFile) {
  const r = JSON.parse(readFileSync(rustFile, 'utf8'))
  const d = new Y.Doc()
  Y.applyUpdate(d, unb64(r.update), 'remote')
  const rc = {}
  d.getMap('cells:s1').forEach((v, k) => { rc[k] = v })
  const rn = {}
  d.getMap('names').forEach((v, k) => { rn[k] = v })
  const a = new Awareness(new Y.Doc())
  applyAwarenessUpdate(a, unb64(r.awareness), 'remote')
  const states = {}
  a.getStates().forEach((st, cid) => { if (cid !== a.clientID) states[cid] = st })
  a.destroy()
  rustRead = { cells: rc, names: rn, awareness: states }
}

console.log(JSON.stringify({
  yjs: require('yjs/package.json').version,
  y_protocols: require('y-protocols/package.json').version,
  update: b64(firstUpdate),
  first_cells: webCells,
  incremental: b64(incremental),
  final_cells: finalCells,
  names: { TAUX: '=0.2' },
  awareness: b64(webAwareness),
  awareness_client: doc.clientID,
  awareness_clock: webAwClock,
  awareness_state: webAwState,
  rust_read: rustRead,
}, null, 1))
process.exit(0)
