// Assets PARTAGÉS du répertoire System (core/drive) : polices et dictionnaires
// Hunspell déposés par un admin dans System/Fonts et System/Dictionaries deviennent
// disponibles pour TOUS les utilisateurs, sans ajout par-utilisateur. Tout est
// best-effort et non bloquant : un répertoire vide ou une erreur réseau laisse
// l'éditeur fonctionner sur ses polices intégrées et ses dictionnaires bundlés.
import { useQuery } from '@tanstack/react-query'
import { systemApi, type FileItem } from '@kubuno/drive'
import { parseFontMeta } from '@ui'

// IDs fixes créés par la migration drive 000018 (owner système).
const FONTS_FOLDER_ID = '00000000-0000-0000-0000-0000000005a2'
const DICTS_FOLDER_ID = '00000000-0000-0000-0000-0000000005a3'

const FONT_EXTS = ['ttf', 'otf', 'woff', 'woff2']

function ext(name: string): string { return (name.split('.').pop() ?? '').toLowerCase() }
function baseName(name: string): string { return name.replace(/\.[^.]+$/, '') }

// ── Polices ─────────────────────────────────────────────────────────────────
// Cache par fichier déjà traité (id → vraie famille) : évite de re-télécharger à
// chaque refetch et empêche de réenregistrer le même FontFace.
const _processedFiles = new Map<string, string>()

/**
 * Liste System/Fonts et enregistre chaque fichier via `FontFace` (octets récupérés
 * sous authentification, pas d'URL → ni souci d'auth ni de CSP `font-src`). La VRAIE
 * famille + le style (gras/italique/poids) sont lus dans la table `name` du fichier
 * (`parseFontMeta`, @ui) : ainsi calibri/calibrib/calibril/calibrii… s'enregistrent
 * TOUS sous « Calibri » (avec le bon poids/style) au lieu d'apparaître comme des
 * familles distinctes « CALIBRIB », « CALIBRIL »… Repli sur le nom de fichier si la
 * table `name` est illisible (WOFF/WOFF2 compressés).
 */
export async function loadSystemFonts(): Promise<string[]> {
  let files: FileItem[]
  try { files = (await systemApi.listFiles(FONTS_FOLDER_ID)).files }
  catch { return [] }

  const families: string[] = []
  const addFamily = (fam: string) => {
    if (fam && !families.some(x => x.toLowerCase() === fam.toLowerCase())) families.push(fam)
  }
  for (const f of files) {
    if (!FONT_EXTS.includes(ext(f.name))) continue
    const cached = _processedFiles.get(f.id)
    if (cached) { addFamily(cached); continue }
    try {
      const buf  = await (await systemApi.downloadBlob(f.id)).arrayBuffer()
      const meta = parseFontMeta(buf)
      const family = meta?.family ?? baseName(f.name)
      const weight = meta ? String(meta.weight) : '400'
      const style  = meta?.style ?? 'normal'
      // Enregistre la VARIANTE (poids/style) sous la famille → un seul « Calibri ».
      const face = await new FontFace(family, buf, { weight, style }).load()
      document.fonts.add(face)
      _processedFiles.set(f.id, family)
      addFamily(family)
      // Prévient le canvas (cf. canvas-engine) qu'une police est prête → purge cache + re-rendu.
      window.dispatchEvent(new Event('kubuno-font-loaded'))
    } catch { /* fichier illisible → ignoré */ }
  }
  return families
}

/**
 * Hook React : familles de polices System/Fonts (enregistrées + prêtes à proposer).
 * Fusionne sans doublon avec une liste de base (polices intégrées de l'éditeur).
 */
export function useSystemFonts(base: readonly string[] = []): string[] {
  const { data = [] } = useQuery({ queryKey: ['system-fonts'], queryFn: loadSystemFonts, staleTime: 60_000 })
  const seen = new Set(base)
  return [...base, ...data.filter(f => !seen.has(f) && (seen.add(f), true))]
}

// ── Dictionnaires Hunspell ────────────────────────────────────────────────────
export interface SystemDict { name: string; aff: string; dic: string }

/**
 * Liste System/Dictionaries, apparie les fichiers `<langue>.aff` + `<langue>.dic`
 * par nom de base, et renvoie le texte de chaque paire complète. Le correcteur
 * (hunspell.ts) en construit un speller nspell supplémentaire par langue.
 */
export async function loadSystemDictPairs(): Promise<SystemDict[]> {
  let files: FileItem[]
  try { files = (await systemApi.listFiles(DICTS_FOLDER_ID)).files }
  catch { return [] }

  const byBase = new Map<string, { aff?: FileItem; dic?: FileItem }>()
  for (const f of files) {
    const e = ext(f.name)
    if (e !== 'aff' && e !== 'dic') continue
    const b = baseName(f.name)
    const entry = byBase.get(b) ?? {}
    entry[e] = f
    byBase.set(b, entry)
  }

  const out: SystemDict[] = []
  for (const [name, { aff, dic }] of byBase) {
    if (!aff || !dic) continue
    try {
      const [a, d] = await Promise.all([
        systemApi.downloadBlob(aff.id).then(b => b.text()),
        systemApi.downloadBlob(dic.id).then(b => b.text()),
      ])
      out.push({ name, aff: a, dic: d })
    } catch { /* paire illisible → ignorée */ }
  }
  return out
}
