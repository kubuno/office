// Diagram interchange: draw.io / mxGraph XML (compressed or not) and CSV import.
// Pure functions (regex-based XML parse, no DOM) so they are unit-testable.
import { inflateRaw } from './inflate'

export interface IoShape {
  id: string; type: string; x: number; y: number; w: number; h: number
  label: string; style: Record<string, unknown>; labelStyle: Record<string, unknown>; zIndex: number
}
export interface IoConnector {
  id: string; sourceId: string | null; targetId: string | null
  sourcePoint: { x: number; y: number } | null; targetPoint: { x: number; y: number } | null
  waypoints: { x: number; y: number }[]; label: string
  style: { strokeColor: string; strokeWidth: number; strokeStyle: string; arrowStart: string; arrowEnd: string; routing: 'straight' | 'orthogonal' | 'curved' }
}
export interface IoData { shapes: IoShape[]; connectors: IoConnector[] }

const enc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
const dec = (s: string) => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
const stripHtml = (s: string) => dec(s).replace(/<br\s*\/?>/gi, '\n').replace(/<[^>]+>/g, '').trim()

// ── Export: our model → mxGraph XML (uncompressed, openable in draw.io) ───────

// Map our shape type → an mxGraph style prefix.
const TYPE_TO_MX: Record<string, string> = {
  rect: '', flow_process: '', er_entity: '', uml_object: '', ui_input: '',
  rounded_rect: 'rounded=1;', flow_terminator: 'rounded=1;arcSize=40;', flow_start: 'rounded=1;arcSize=40;', bpmn_task: 'rounded=1;', ui_button: 'rounded=1;',
  ellipse: 'ellipse;', flow_connector: 'ellipse;', uml_usecase: 'ellipse;', bpmn_start: 'ellipse;', bpmn_end: 'ellipse;', bpmn_event: 'ellipse;', er_attribute: 'ellipse;',
  diamond: 'rhombus;', flow_decision: 'rhombus;', bpmn_gateway: 'rhombus;', er_relationship: 'rhombus;',
  triangle: 'triangle;',
  parallelogram: 'shape=parallelogram;', flow_data: 'shape=parallelogram;',
  hexagon: 'shape=hexagon;', flow_prep: 'shape=hexagon;',
  cylinder: 'shape=cylinder;', flow_db: 'shape=cylinder;', net_database: 'shape=cylinder;',
  cloud: 'ellipse;shape=cloud;', net_cloud: 'ellipse;shape=cloud;', net_internet: 'ellipse;shape=cloud;',
  flow_document: 'shape=document;',
  shp_cube: 'shape=cube;', uml_node: 'shape=cube;',
  shp_step: 'shape=step;',
  shp_card: 'shape=card;', flow_card: 'shape=card;',
  shp_note: 'shape=note;', uml_note: 'shape=note;',
  text: 'text;html=1;',
  container: 'swimlane;', swimlane_v: 'swimlane;', swimlane_h: 'swimlane;horizontal=0;',
  shp_pentagon: 'shape=pentagon;', shp_octagon: 'shape=octagon;',
}

function shapeStyle(s: IoShape): string {
  let st = TYPE_TO_MX[s.type] ?? ''
  const sy = s.style as Record<string, unknown>
  if (sy.fillColor && sy.fillColor !== 'none') st += `fillColor=${sy.fillColor};`
  else if (sy.fillColor === 'none') st += 'fillColor=none;'
  if (sy.strokeColor && sy.strokeColor !== 'none') st += `strokeColor=${sy.strokeColor};`
  if (sy.strokeStyle === 'dashed') st += 'dashed=1;'
  else if (sy.strokeStyle === 'dotted') st += 'dashed=1;dashPattern=1 4;'
  if (typeof sy.strokeWidth === 'number') st += `strokeWidth=${sy.strokeWidth};`
  if (typeof sy.opacity === 'number' && sy.opacity < 100) st += `opacity=${sy.opacity};`
  if (sy.shadow) st += 'shadow=1;'
  st += 'whiteSpace=wrap;html=1;'
  return st
}

function edgeStyle(c: IoConnector): string {
  let st = 'edgeStyle=' + (c.style.routing === 'orthogonal' ? 'orthogonalEdgeStyle' : 'none') + ';'
  if (c.style.routing === 'curved') st += 'curved=1;'
  if (c.style.strokeColor) st += `strokeColor=${c.style.strokeColor};`
  if (typeof c.style.strokeWidth === 'number') st += `strokeWidth=${c.style.strokeWidth};`
  if (c.style.strokeStyle === 'dashed') st += 'dashed=1;'
  else if (c.style.strokeStyle === 'dotted') st += 'dashed=1;dashPattern=1 4;'
  st += `startArrow=${c.style.arrowStart && c.style.arrowStart !== 'none' ? 'classic' : 'none'};`
  st += `endArrow=${c.style.arrowEnd && c.style.arrowEnd !== 'none' ? 'classic' : 'none'};`
  st += 'html=1;rounded=0;'
  return st
}

export function toDrawioXml(data: IoData, title = 'Kubuno'): string {
  const cells: string[] = []
  for (const s of data.shapes) {
    cells.push(
      `        <mxCell id="${enc(s.id)}" value="${enc(s.label || '')}" style="${enc(shapeStyle(s))}" vertex="1" parent="1">\n` +
      `          <mxGeometry x="${Math.round(s.x)}" y="${Math.round(s.y)}" width="${Math.round(s.w)}" height="${Math.round(s.h)}" as="geometry" />\n` +
      `        </mxCell>`,
    )
  }
  for (const c of data.connectors) {
    const pts = (c.waypoints ?? []).map((p) => `            <mxPoint x="${Math.round(p.x)}" y="${Math.round(p.y)}" />`).join('\n')
    const geom = pts
      ? `          <mxGeometry relative="1" as="geometry">\n            <Array as="points">\n${pts}\n            </Array>\n          </mxGeometry>`
      : `          <mxGeometry relative="1" as="geometry" />`
    cells.push(
      `        <mxCell id="${enc(c.id)}" value="${enc(c.label || '')}" style="${enc(edgeStyle(c))}" edge="1" parent="1"` +
      (c.sourceId ? ` source="${enc(c.sourceId)}"` : '') + (c.targetId ? ` target="${enc(c.targetId)}"` : '') + `>\n${geom}\n        </mxCell>`,
    )
  }
  return `<mxfile host="kubuno">\n  <diagram name="${enc(title)}">\n    <mxGraphModel dx="800" dy="600" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" math="0" shadow="0">\n      <root>\n        <mxCell id="0" />\n        <mxCell id="1" parent="0" />\n${cells.join('\n')}\n      </root>\n    </mxGraphModel>\n  </diagram>\n</mxfile>\n`
}

// ── Import: mxGraph XML → our model (uncompressed only) ───────────────────────

function parseStyle(style: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const tok of style.split(';')) {
    if (!tok) continue
    const eq = tok.indexOf('=')
    if (eq < 0) out[tok.trim()] = '1' // bare token (e.g. "ellipse", "rounded")
    else out[tok.slice(0, eq).trim()] = tok.slice(eq + 1).trim()
  }
  return out
}

function mxToType(st: Record<string, string>): string {
  const shape = st.shape
  if (st.text || shape === 'text') return 'text'
  if (st.ellipse || st.shape === 'cloud') return st.shape === 'cloud' ? 'cloud' : 'ellipse'
  if (st.rhombus) return 'diamond'
  if (st.triangle) return 'triangle'
  if (shape === 'cloud') return 'cloud'
  if (shape === 'parallelogram') return 'parallelogram'
  if (shape === 'hexagon') return 'hexagon'
  if (shape === 'cylinder') return 'cylinder'
  if (shape === 'cube') return 'shp_cube'
  if (shape === 'step') return 'shp_step'
  if (shape === 'card') return 'shp_card'
  if (shape === 'note') return 'shp_note'
  if (shape === 'document') return 'flow_document'
  if (shape === 'process') return 'flow_predefined'
  if (shape === 'pentagon') return 'shp_pentagon'
  if (shape === 'octagon') return 'shp_octagon'
  if (st.swimlane !== undefined || shape === 'swimlane') return st.horizontal === '0' ? 'swimlane_h' : 'swimlane_v'
  if (st.rounded === '1') return 'rounded_rect'
  return 'rect'
}

const attr = (tag: string, name: string): string | null => {
  const m = tag.match(new RegExp(`\\s${name}="([^"]*)"`))
  return m ? m[1] : null
}

function b64ToBytes(b64: string): Uint8Array {
  const clean = b64.replace(/\s/g, '')
  const bin = atob(clean)
  const bytes = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
  return bytes
}

// Decompress a draw.io <diagram> payload (base64 → deflateRaw → urlencoded XML).
function inflateDiagram(xml: string): string | null {
  const m = xml.match(/<diagram[^>]*>([\s\S]*?)<\/diagram>/)
  if (!m) return null
  const payload = m[1].trim()
  if (!payload || /</.test(payload)) return null // already XML, not a payload
  try {
    const inflated = new TextDecoder().decode(inflateRaw(b64ToBytes(payload)))
    let decoded = inflated
    try { const u = decodeURIComponent(inflated); if (/<mxCell\b/.test(u)) decoded = u } catch { /* keep raw */ }
    return /<mxCell\b/.test(decoded) ? decoded : null
  } catch { return null }
}

export function fromDrawioXml(xml: string): IoData | null {
  if (!/<mxCell\b/.test(xml)) {
    const inflated = inflateDiagram(xml)
    return inflated ? fromDrawioXml(inflated) : null
  }
  const shapes: IoShape[] = []
  const connectors: IoConnector[] = []
  const blocks = xml.split(/<mxCell\b/).slice(1)
  let z = 0
  for (let raw of blocks) {
    raw = '<mxCell ' + raw
    const close = raw.search(/<\/mxCell>|\/>/)
    const block = close >= 0 ? raw.slice(0, raw.indexOf('>', close >= 0 ? 0 : 0)) : raw
    const openTag = raw.slice(0, raw.indexOf('>') + 1)
    const id = attr(openTag, 'id'); if (!id || id === '0' || id === '1') continue
    const isEdge = attr(openTag, 'edge') === '1'
    const isVertex = attr(openTag, 'vertex') === '1'
    const styleStr = dec(attr(openTag, 'style') ?? '')
    const st = parseStyle(styleStr)
    const value = stripHtml(attr(openTag, 'value') ?? '')
    if (isEdge) {
      const pts: { x: number; y: number }[] = []
      const reP = /<mxPoint\s+x="([\-\d.]+)"\s+y="([\-\d.]+)"\s*\/>/g
      let mp: RegExpExecArray | null
      // only points inside the Array (waypoints); skip exit/entry source/target points
      const arrM = raw.match(/<Array[^>]*as="points"[^>]*>([\s\S]*?)<\/Array>/)
      if (arrM) while ((mp = reP.exec(arrM[1]))) pts.push({ x: +mp[1], y: +mp[2] })
      connectors.push({
        id, sourceId: attr(openTag, 'source'), targetId: attr(openTag, 'target'),
        sourcePoint: null, targetPoint: null, waypoints: pts, label: value,
        style: {
          strokeColor: st.strokeColor || '#6c8ebf', strokeWidth: st.strokeWidth ? +st.strokeWidth : 1.5,
          strokeStyle: st.dashed === '1' ? (st.dashPattern ? 'dotted' : 'dashed') : 'solid',
          arrowStart: st.startArrow && st.startArrow !== 'none' ? 'block' : 'none',
          arrowEnd: !st.endArrow ? 'block' : (st.endArrow !== 'none' ? 'block' : 'none'),
          routing: st.curved === '1' ? 'curved' : (st.edgeStyle && st.edgeStyle !== 'none' ? 'orthogonal' : 'straight'),
        },
      })
    } else if (isVertex) {
      const g = raw.match(/<mxGeometry\b([^>]*)/)
      const gt = g ? g[1] : ''
      const num = (n: string, d: number) => { const m = gt.match(new RegExp(`${n}="([\\-\\d.]+)"`)); return m ? +m[1] : d }
      const style: Record<string, unknown> = {}
      if (st.fillColor) style.fillColor = st.fillColor
      if (st.strokeColor) style.strokeColor = st.strokeColor
      if (st.dashed === '1') style.strokeStyle = st.dashPattern ? 'dotted' : 'dashed'
      if (st.strokeWidth) style.strokeWidth = +st.strokeWidth
      if (st.opacity) style.opacity = +st.opacity
      if (st.shadow === '1') style.shadow = true
      if (st.rounded === '1' && mxToType(st) === 'rounded_rect') style.rounded = 12
      shapes.push({ id, type: mxToType(st), x: num('x', 0), y: num('y', 0), w: num('width', 120), h: num('height', 60), label: value, style, labelStyle: {}, zIndex: z++ })
    }
  }
  return { shapes, connectors }
}

// ── Import: CSV → diagram (one node per row, grid layout) ──────────────────────

export function fromCsv(text: string): IoData {
  const lines = text.split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith('#'))
  const shapes: IoShape[] = []
  const cols = Math.max(1, Math.ceil(Math.sqrt(lines.length)))
  lines.forEach((line, i) => {
    const label = (line.split(',')[0] || line).trim().replace(/^"|"$/g, '')
    shapes.push({
      id: 'csv' + i + '_' + Math.random().toString(36).slice(2, 7),
      type: 'rounded_rect', x: 80 + (i % cols) * 180, y: 80 + Math.floor(i / cols) * 110,
      w: 150, h: 60, label, style: { fillColor: '#dae8fc', strokeColor: '#6c8ebf', rounded: 12 }, labelStyle: {}, zIndex: i,
    })
  })
  return { shapes, connectors: [] }
}
