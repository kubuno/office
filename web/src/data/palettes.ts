// Chart color palettes and report themes for the Data (BI) sub-module.
// Themes mirror Power BI's built-in report themes; palettes drive every visual.

export interface ChartPalette { id: string; name: string; colors: string[] }

export const PALETTES: ChartPalette[] = [
  { id: 'kubuno',    name: 'Kubuno',      colors: ['#1a73e8', '#ea4335', '#fbbc04', '#34a853', '#ff6d00', '#a142f4', '#0097a7', '#e91e63'] },
  { id: 'classic',   name: 'Classique',   colors: ['#118dff', '#12239e', '#e66c37', '#6b007b', '#e044a7', '#744ec2', '#d9b300', '#d64550'] },
  { id: 'vibrant',   name: 'Vibrant',     colors: ['#f94144', '#f3722c', '#f8961e', '#f9c74f', '#90be6d', '#43aa8b', '#577590', '#277da1'] },
  { id: 'cool',      name: 'Froid',       colors: ['#0d3b66', '#1d70a2', '#3895d3', '#58cced', '#7ee8fa', '#80ffdb', '#64dfdf', '#48bfe3'] },
  { id: 'warm',      name: 'Chaud',       colors: ['#7f1d1d', '#b91c1c', '#dc2626', '#ef4444', '#f97316', '#fb923c', '#fbbf24', '#fde047'] },
  { id: 'pastel',    name: 'Pastel',      colors: ['#a7c7e7', '#c1e1c1', '#fdfd96', '#ffb7b2', '#e0bbe4', '#b5ead7', '#ffdac1', '#c7ceea'] },
  { id: 'mono_blue', name: 'Bleu mono',   colors: ['#08306b', '#08519c', '#2171b5', '#4292c6', '#6baed6', '#9ecae1', '#c6dbef', '#deebf7'] },
  { id: 'earth',     name: 'Terre',       colors: ['#582f0e', '#7f4f24', '#936639', '#a68a64', '#b6ad90', '#a4ac86', '#656d4a', '#414833'] },
  { id: 'colorblind',name: 'Daltonien',   colors: ['#1170aa', '#fc7d0b', '#a3acb9', '#57606c', '#5fa2ce', '#c85200', '#7b848f', '#a3cce9'] },
  { id: 'grayscale', name: 'Niveaux gris',colors: ['#1f2937', '#374151', '#4b5563', '#6b7280', '#9ca3af', '#d1d5db', '#e5e7eb', '#f3f4f6'] },
]

export function paletteById(id?: string): string[] {
  return PALETTES.find(p => p.id === id)?.colors ?? PALETTES[0].colors
}

export interface ReportTheme {
  id: string
  name: string
  primaryColor: string
  fontFamily: string
  background: string       // report canvas background
  pageBackground: string   // individual page background
  cardBackground: string   // visual container background
  foreground: string       // primary text
  paletteId: string
  chartPalette: string[]
}

export const REPORT_THEMES: ReportTheme[] = [
  { id: 'default',  name: 'Par défaut', primaryColor: '#1a73e8', fontFamily: 'Inter, system-ui, sans-serif', background: '#f8f9fa', pageBackground: '#ffffff', cardBackground: '#ffffff', foreground: '#202124', paletteId: 'kubuno',    chartPalette: paletteById('kubuno') },
  { id: 'executive',name: 'Direction',  primaryColor: '#12239e', fontFamily: 'Georgia, serif',               background: '#eef1f6', pageBackground: '#ffffff', cardBackground: '#ffffff', foreground: '#1a1a2e', paletteId: 'classic',   chartPalette: paletteById('classic') },
  { id: 'dark',     name: 'Sombre',     primaryColor: '#58cced', fontFamily: 'Inter, system-ui, sans-serif', background: '#1a1d24', pageBackground: '#23272f', cardBackground: '#2b303b', foreground: '#e8eaed', paletteId: 'cool',      chartPalette: paletteById('cool') },
  { id: 'sunset',   name: 'Coucher',    primaryColor: '#e66c37', fontFamily: 'Inter, system-ui, sans-serif', background: '#fff7f0', pageBackground: '#ffffff', cardBackground: '#fffaf5', foreground: '#3d2c20', paletteId: 'warm',      chartPalette: paletteById('warm') },
  { id: 'mint',     name: 'Menthe',     primaryColor: '#34a853', fontFamily: 'Inter, system-ui, sans-serif', background: '#f0faf4', pageBackground: '#ffffff', cardBackground: '#ffffff', foreground: '#163a26', paletteId: 'vibrant',   chartPalette: paletteById('vibrant') },
  { id: 'highcontrast', name: 'Contraste élevé', primaryColor: '#000000', fontFamily: 'Inter, system-ui, sans-serif', background: '#ffffff', pageBackground: '#ffffff', cardBackground: '#ffffff', foreground: '#000000', paletteId: 'colorblind', chartPalette: paletteById('colorblind') },
  { id: 'accessible', name: 'Accessible', primaryColor: '#1170aa', fontFamily: 'Inter, system-ui, sans-serif', background: '#f5f5f5', pageBackground: '#ffffff', cardBackground: '#ffffff', foreground: '#1a1a1a', paletteId: 'colorblind', chartPalette: paletteById('colorblind') },
]

export function themeById(id?: string): ReportTheme {
  return REPORT_THEMES.find(t => t.id === id) ?? REPORT_THEMES[0]
}

/** Diverging scale used by conditional formatting and heatmaps. */
export function colorScale(t: number, lo = '#fce8e6', mid = '#fff3cd', hi = '#e6f4ea'): string {
  const clamp = Math.max(0, Math.min(1, t))
  const lerp = (a: number, b: number, x: number) => Math.round(a + (b - a) * x)
  const hex = (c: string) => [parseInt(c.slice(1, 3), 16), parseInt(c.slice(3, 5), 16), parseInt(c.slice(5, 7), 16)]
  const [a, b] = clamp < 0.5
    ? [hex(lo), hex(mid)] as const
    : [hex(mid), hex(hi)] as const
  const x = clamp < 0.5 ? clamp * 2 : (clamp - 0.5) * 2
  return `rgb(${lerp(a[0], b[0], x)},${lerp(a[1], b[1], x)},${lerp(a[2], b[2], x)})`
}
