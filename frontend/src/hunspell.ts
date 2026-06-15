// Correcteur orthographique COMPLET via Hunspell (nspell) + dictionnaires LIBRES :
//   FR = Dicollecte/Grammalecte (MPL-2.0) · EN = SCOWL (MIT/BSD) — compatibles AGPLv3.
// Les `.aff/.dic` sont servis comme ASSETS fetchés à la demande (≈2 Mo au total) — JAMAIS
// inlinés dans le bundle JS. Chargement paresseux à la 1ʳᵉ activation du correcteur.
// Un mot est CORRECT s'il est accepté par FR OU EN (document potentiellement bilingue),
// avec repli sur la forme minuscule (débuts de phrase / majuscules).
import nspell from 'nspell'
import frAff from './dict/fr.aff?url'
import frDic from './dict/fr.dic?url'
import enDic from './dict/en.dic?url'
// en.aff est minuscule (~3 Ko) : Vite l'inline sinon en data: URL, que `fetch` ne peut
// PAS charger sous la CSP `connect-src` → on l'embarque directement en texte (`?raw`).
import enAff from './dict/en.aff?raw'
import { loadSystemDictPairs } from './systemAssets'

interface Speller { correct(w: string): boolean; suggest(w: string): string[]; add(w: string): void }

// Tableau de spellers actifs : FR + EN bundlés, plus tout dictionnaire déposé par un
// admin dans System/Dictionaries. Un mot est CORRECT s'il est accepté par AU MOINS UN.
let spellers: Speller[] = []
let state: 'idle' | 'loading' | 'ready' | 'error' = 'idle'
const cache = new Map<string, boolean>()
let readyCb: (() => void) | null = null

export function onSpellerReady(cb: () => void): void { readyCb = cb }
export function spellerState(): typeof state { return state }

async function txt(url: string): Promise<string> {
  const r = await fetch(url)
  if (!r.ok) throw new Error('dict ' + r.status)
  return r.text()
}

export async function loadSpeller(): Promise<void> {
  if (state === 'loading' || state === 'ready') return
  state = 'loading'
  try {
    const [fa, fd, ed] = await Promise.all([txt(frAff), txt(frDic), txt(enDic)])
    // Construction nspell (synchrone, ~1 s sur le gros .dic FR) — laissée hors du chemin
    // de frappe (appelée une seule fois, après le fetch async). `enAff` est déjà du texte.
    spellers = [
      nspell(fa, fd) as unknown as Speller,
      nspell(enAff, ed) as unknown as Speller,
    ]
    cache.clear()
    state = 'ready'
    readyCb?.()
    // Dictionnaires système (admin) — best-effort, hors chemin critique : on ne bloque
    // pas la disponibilité du correcteur FR/EN si System/Dictionaries est vide/illisible.
    loadSystemDictPairs().then(pairs => {
      if (!pairs.length) return
      for (const p of pairs) {
        try { spellers.push(nspell(p.aff, p.dic) as unknown as Speller) }
        catch (e) { console.warn(`[spell] dictionnaire système « ${p.name} » illisible`, e) }
      }
      cache.clear()
      readyCb?.()   // re-signaler → re-vérification des squiggles avec les nouvelles langues
    }).catch(() => {})
  } catch (e) {
    console.warn('[spell] échec du chargement des dictionnaires', e)
    state = 'error'
  }
}

export function isWordCorrect(word: string): boolean {
  if (state !== 'ready' || !spellers.length) return true   // pas prêt → ne rien signaler
  const c = cache.get(word); if (c !== undefined) return c
  const lower = word.toLowerCase()
  const ok = spellers.some(s => s.correct(word)) ||
             (lower !== word && spellers.some(s => s.correct(lower)))
  cache.set(word, ok)
  return ok
}

export function suggestWord(word: string): string[] {
  if (state !== 'ready' || !spellers.length) return []
  const out: string[] = []
  for (const s of spellers) for (const w of s.suggest(word)) if (!out.includes(w)) out.push(w)
  return out.slice(0, 6)
}
