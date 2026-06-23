// Teintes du ruban par sous-éditeur Office (façon MS Office : 1 couleur de bande
// d'onglets par app). La couleur sert AUSSI d'accent (texte de l'onglet actif +
// surbrillance des items du ruban). Le corps + le contenu du ruban restent blancs.
import { WORKSPACE_OFFICE, WORKSPACE_DARK, type WorkspaceTheme } from '@kubuno/sdk'

// Construit un thème « ruban coloré » à partir d'une couleur de bande d'onglets.
export function officeTheme(color: string): WorkspaceTheme {
  return { ...WORKSPACE_OFFICE, topbarBg: color, topbarText: '#ffffff', accent: color }
}

// Couleurs par app (alignées sur l'esprit MS Office).
export const OFFICE_TONE = {
  documents:    '#1557b0', // bleu        (Word)
  spreadsheet:  '#0f7b3f', // vert        (Excel)
  presentation: '#b7472a', // brique      (PowerPoint)
  projects:     '#1e7a6f', // sarcelle    (Project)
  diagrams:     '#3b53b5', // indigo      (Visio)
  data:         '#0e7490', // cyan foncé  (Power BI)
  maths:        '#6a3fa0', // violet
  whiteboard:   '#5b4bd0', // indigo-violet
} as const

// Éclaircit une couleur hex en la mélangeant vers le blanc (f = 0..1).
function lighten(hex: string, f: number): string {
  const h = hex.replace('#', '')
  const n = parseInt(h.length === 3 ? h.split('').map(c => c + c).join('') : h, 16)
  const r = (n >> 16) & 255, g = (n >> 8) & 255, b = n & 255
  const mix = (c: number) => Math.round(c + (255 - c) * f)
  return `#${[mix(r), mix(g), mix(b)].map(c => c.toString(16).padStart(2, '0')).join('')}`
}

// Couleur de l'onglet « Fichier » + rail du backstage pour un accent de module donné :
// une teinte PLUS CLAIRE de l'accent (ressort sur la bande d'onglets foncée, façon
// Office). Cas particulier documents (#1557b0) → #3f7dd0 (choisi par l'utilisateur).
export function fileAccentFor(accent: string): string {
  if (accent.toLowerCase() === OFFICE_TONE.documents) return '#3f7dd0'
  return lighten(accent, 0.3)
}

export const THEME_DOCUMENTS    = officeTheme(OFFICE_TONE.documents)
export const THEME_SPREADSHEET  = officeTheme(OFFICE_TONE.spreadsheet)
export const THEME_PRESENTATION = officeTheme(OFFICE_TONE.presentation)
export const THEME_PROJECTS     = officeTheme(OFFICE_TONE.projects)
export const THEME_DIAGRAMS     = officeTheme(OFFICE_TONE.diagrams)
export const THEME_DATA         = officeTheme(OFFICE_TONE.data)
export const THEME_MATHS        = officeTheme(OFFICE_TONE.maths)
export const THEME_WHITEBOARD   = officeTheme(OFFICE_TONE.whiteboard)

// Script (éditeur de code) : ruban SOMBRE harmonieux (façon VS Code) — topbar + bande
// d'onglets UNIFIÉES (#2b2b2b), onglet actif + contenu = fond éditeur (#1e1e1e), accent
// bleu. `topbarText` active le mode « sans couture » (pas de filet, hovers translucides).
export const THEME_SCRIPT: WorkspaceTheme = {
  ...WORKSPACE_DARK,
  topbarBg: '#2b2b2b', topbarText: '#cfcfcf',
  header: '#2b2b2b', bg: '#1e1e1e', accent: '#5a9bdc',
}
