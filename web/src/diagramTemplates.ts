// Predefined diagram templates inserted into the current page (draw.io-style gallery).
import type { IoData, IoShape, IoConnector } from './diagramIo'

type Pal = [string, string] // [fill, stroke]
const BLUE: Pal = ['#dae8fc', '#6c8ebf']
const GREEN: Pal = ['#d5e8d4', '#82b366']
const ORANGE: Pal = ['#ffe6cc', '#d79b00']
const YELLOW: Pal = ['#fff2cc', '#d6b656']
const PURPLE: Pal = ['#e1d5e7', '#9673a6']
const RED: Pal = ['#f8cecc', '#b85450']
const GREY: Pal = ['#f5f5f5', '#666666']

let _n = 0
const sid = () => 't' + (_n++)

function S(type: string, x: number, y: number, w: number, h: number, label: string, pal: Pal, extra: Record<string, unknown> = {}): IoShape {
  return { id: sid(), type, x, y, w, h, label, style: { fillColor: pal[0], strokeColor: pal[1], ...extra }, labelStyle: {}, zIndex: 0 }
}
function E(src: string, tgt: string, label = '', routing: IoConnector['style']['routing'] = 'orthogonal'): IoConnector {
  return { id: sid(), sourceId: src, targetId: tgt, sourcePoint: null, targetPoint: null, waypoints: [], label, style: { strokeColor: '#6c8ebf', strokeWidth: 1.5, strokeStyle: 'solid', arrowStart: 'none', arrowEnd: 'block', routing } }
}

export interface DiagramTemplate { id: string; name: string; build: () => IoData }

export const TEMPLATES: DiagramTemplate[] = [
  {
    id: 'blank', name: 'Vierge', build: () => ({ shapes: [], connectors: [] }),
  },
  {
    id: 'flowchart', name: 'Organigramme',
    build: () => {
      const a = S('flow_start', 200, 40, 140, 50, 'Début', GREEN, { rounded: 30 })
      const b = S('flow_process', 200, 140, 140, 60, 'Traitement', BLUE)
      const c = S('flow_decision', 190, 250, 160, 90, 'Condition ?', YELLOW)
      const d = S('flow_process', 60, 390, 140, 60, 'Oui', BLUE)
      const e = S('flow_terminator', 360, 390, 140, 50, 'Non', RED, { rounded: 30 })
      return { shapes: [a, b, c, d, e], connectors: [E(a.id, b.id), E(b.id, c.id), E(c.id, d.id, 'oui'), E(c.id, e.id, 'non')] }
    },
  },
  {
    id: 'org', name: 'Organigramme hiérarchique',
    build: () => {
      const ceo = S('rounded_rect', 240, 40, 140, 56, 'Direction', BLUE, { rounded: 8 })
      const m1 = S('rounded_rect', 90, 180, 140, 56, 'Manager A', GREEN, { rounded: 8 })
      const m2 = S('rounded_rect', 390, 180, 140, 56, 'Manager B', GREEN, { rounded: 8 })
      const s1 = S('rounded_rect', 20, 320, 120, 50, 'Équipe 1', GREY, { rounded: 8 })
      const s2 = S('rounded_rect', 170, 320, 120, 50, 'Équipe 2', GREY, { rounded: 8 })
      const s3 = S('rounded_rect', 330, 320, 120, 50, 'Équipe 3', GREY, { rounded: 8 })
      const s4 = S('rounded_rect', 480, 320, 120, 50, 'Équipe 4', GREY, { rounded: 8 })
      return { shapes: [ceo, m1, m2, s1, s2, s3, s4], connectors: [E(ceo.id, m1.id), E(ceo.id, m2.id), E(m1.id, s1.id), E(m1.id, s2.id), E(m2.id, s3.id), E(m2.id, s4.id)] }
    },
  },
  {
    id: 'mindmap', name: 'Carte mentale',
    build: () => {
      const c = S('ellipse', 240, 200, 160, 80, 'Idée centrale', PURPLE)
      const b1 = S('ellipse', 30, 60, 130, 60, 'Branche 1', BLUE)
      const b2 = S('ellipse', 480, 60, 130, 60, 'Branche 2', GREEN)
      const b3 = S('ellipse', 30, 360, 130, 60, 'Branche 3', ORANGE)
      const b4 = S('ellipse', 480, 360, 130, 60, 'Branche 4', YELLOW)
      const e = (s: string, t: string) => { const ed = E(s, t, '', 'curved'); ed.style.arrowEnd = 'none'; return ed }
      return { shapes: [c, b1, b2, b3, b4], connectors: [e(c.id, b1.id), e(c.id, b2.id), e(c.id, b3.id), e(c.id, b4.id)] }
    },
  },
  {
    id: 'network', name: 'Réseau',
    build: () => {
      const i = S('net_internet', 220, 30, 120, 80, 'Internet', BLUE)
      const f = S('net_firewall', 240, 160, 80, 80, 'Pare-feu', RED)
      const s = S('net_server', 130, 290, 100, 70, 'Serveur', BLUE)
      const db = S('net_database', 350, 280, 80, 100, 'BD', BLUE)
      return { shapes: [i, f, s, db], connectors: [E(i.id, f.id), E(f.id, s.id), E(s.id, db.id)] }
    },
  },
  {
    id: 'aws', name: 'AWS',
    build: () => {
      const u = S('net_user', 40, 200, 60, 80, 'Utilisateur', BLUE)
      const cf = S('aws_cloudfront', 180, 200, 80, 80, 'CloudFront', PURPLE, { rounded: 8 })
      const ec2 = S('aws_ec2', 330, 200, 80, 80, 'EC2', ORANGE)
      const rds = S('aws_rds', 480, 200, 80, 80, 'RDS', BLUE)
      return { shapes: [u, cf, ec2, rds], connectors: [E(u.id, cf.id), E(cf.id, ec2.id), E(ec2.id, rds.id)] }
    },
  },
  {
    id: 'uml', name: 'Classe UML',
    build: () => {
      const a = S('uml_class', 60, 80, 160, 110, 'Compte', BLUE)
      const b = S('uml_class', 360, 80, 160, 110, 'Client', GREEN)
      const e = E(b.id, a.id, '1..*'); e.style.arrowEnd = 'open'
      return { shapes: [a, b], connectors: [e] }
    },
  },
  {
    id: 'bpmn', name: 'BPMN',
    build: () => {
      const s = S('bpmn_start', 40, 200, 50, 50, '', GREEN)
      const t1 = S('bpmn_task', 140, 190, 120, 70, 'Recevoir', BLUE, { rounded: 10 })
      const g = S('bpmn_gateway', 320, 200, 70, 70, 'OK ?', YELLOW)
      const t2 = S('bpmn_task', 440, 190, 120, 70, 'Traiter', BLUE, { rounded: 10 })
      const end = S('bpmn_end', 620, 200, 50, 50, '', RED)
      return { shapes: [s, t1, g, t2, end], connectors: [E(s.id, t1.id), E(t1.id, g.id), E(g.id, t2.id, 'oui'), E(t2.id, end.id)] }
    },
  },
]
