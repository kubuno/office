// Premières notions de correction orthographique + grammaticale, SANS moteur lourd
// (pas de hunspell/dictionnaire complet bundlé). Approche fiable (peu de faux positifs) :
//   1) ORTHOGRAPHE : un dictionnaire de FAUTES FRÉQUENTES FR+EN → suggestions.
//   2) GRAMMAIRE   : règles simples (mot répété, double espace, « a » vs « à »…).
// Le rendu (soulignés ondulés rouges/bleus sur le canvas) + les suggestions au clic
// droit sont câblés dans DocumentEditorPage. Dictionnaire perso (mots ignorés) persisté
// en localStorage. Extensible : enrichir COMMON_MISTAKES / les règles.

import { isWordCorrect } from './hunspell'

export interface SpellIssue {
  from: number          // position ProseMirror (début du mot)
  to: number            // position ProseMirror (fin)
  type: 'spelling' | 'grammar'
  word: string
  suggestions: string[]
  message?: string
}

// ── Dictionnaire de fautes fréquentes (clé = forme FAUSSE en minuscules) ─────────
export const COMMON_MISTAKES: Record<string, string[]> = {
  // Français
  'developement': ['développement'], 'developpement': ['développement'], 'developper': ['développer'],
  'fonctionalité': ['fonctionnalité'], 'fonctionalités': ['fonctionnalités'],
  'apparament': ['apparemment'], 'language': ['langage'], 'languages': ['langages'],
  'parmis': ['parmi'], 'malgrés': ['malgré'], 'acceuil': ['accueil'], 'accueille': ['accueil'],
  'adress': ['adresse'], 'addresse': ['adresse'], 'apel': ['appel'], 'apeler': ['appeler'],
  'ortographe': ['orthographe'], 'orthographie': ['orthographe'], 'exercise': ['exercice'],
  'connection': ['connexion'], 'connaissace': ['connaissance'], 'environement': ['environnement'],
  'envirronement': ['environnement'], 'evenement': ['événement'], 'evenements': ['événements'],
  'differente': ['différente'], 'differents': ['différents'], 'definitivement': ['définitivement'],
  'occurence': ['occurrence'], 'occurences': ['occurrences'],
  'professionel': ['professionnel'], 'professionelle': ['professionnelle'], 'personel': ['personnel'],
  'personelle': ['personnelle'], 'traditionel': ['traditionnel'], 'rationel': ['rationnel'],
  'comformément': ['conformément'], 'quelquechose': ['quelque chose'],
  'malheureusment': ['malheureusement'], 'finalemnt': ['finalement'], 'égalemnt': ['également'],
  'biensûr': ['bien sûr'], 'biensur': ['bien sûr'], 'plutot': ['plutôt'], 'bientot': ['bientôt'],
  'aussitot': ['aussitôt'], 'tot': ['tôt'], 'controle': ['contrôle'], 'controler': ['contrôler'],
  'entrainement': ['entraînement'], 'chaine': ['chaîne'], 'maitre': ['maître'], 'parait': ['paraît'],
  'connait': ['connaît'], 'apparait': ['apparaît'], 'gout': ['goût'], 'diner': ['dîner'],
  'numero': ['numéro'], 'modele': ['modèle'], 'systeme': ['système'],
  'probleme': ['problème'], 'problemes': ['problèmes'], 'theme': ['thème'], 'eleve': ['élève'],
  'caractere': ['caractère'], 'parametre': ['paramètre'], 'parametres': ['paramètres'],
  'derniere': ['dernière'], 'premiere': ['première'], 'maniere': ['manière'], 'lumiere': ['lumière'],
  // Anglais
  'recieve': ['receive'], 'recieved': ['received'], 'beleive': ['believe'], 'occured': ['occurred'],
  'occuring': ['occurring'], 'seperate': ['separate'], 'definately': ['definitely'], 'wich': ['which'],
  'teh': ['the'], 'occassion': ['occasion'], 'accross': ['across'],
  'untill': ['until'], 'wierd': ['weird'], 'thier': ['their'], 'becuase': ['because'],
  'enviroment': ['environment'], 'goverment': ['government'], 'neccessary': ['necessary'],
  'accomodate': ['accommodate'], 'tommorow': ['tomorrow'], 'calender': ['calendar'],
  'noticable': ['noticeable'], 'publically': ['publicly'], 'arguement': ['argument'],
}

// ── Dictionnaire personnel (mots ignorés) ────────────────────────────────────────
// Deux niveaux, façon Word :
//   • PERSISTANT  : « Ajouter au dictionnaire » → localStorage (tous documents, à jamais).
//   • SESSION     : « Ignorer (partout) » → en mémoire seulement (jusqu'au rechargement),
//     ne pollue pas le dictionnaire permanent.
const STORE_KEY = 'kubuno_spell_ignore'
function loadIgnored(): Set<string> {
  try { return new Set(JSON.parse(localStorage.getItem(STORE_KEY) || '[]')) } catch { return new Set() }
}
const ignored = loadIgnored()
const sessionIgnored = new Set<string>()

// Ajout au dictionnaire personnel (persistant).
export function ignoreWord(word: string): void {
  ignored.add(word.toLowerCase())
  try { localStorage.setItem(STORE_KEY, JSON.stringify([...ignored])) } catch { /* quota */ }
}
// Ignorer pour la session courante uniquement (non persistant).
export function ignoreWordSession(word: string): void { sessionIgnored.add(word.toLowerCase()) }
// Retire un mot du dictionnaire personnel (et de l'ignore de session).
export function unignoreWord(word: string): void {
  const w = word.toLowerCase()
  ignored.delete(w); sessionIgnored.delete(w)
  try { localStorage.setItem(STORE_KEY, JSON.stringify([...ignored])) } catch { /* quota */ }
}
export function isIgnored(word: string): boolean {
  const w = word.toLowerCase()
  return ignored.has(w) || sessionIgnored.has(w)
}
// Mots du dictionnaire personnel (pour un panneau de gestion).
export function personalDictionary(): string[] { return [...ignored].filter(w => !w.startsWith('§rep§')).sort() }

// ── Analyse d'un fragment de texte (offset = position PM du 1er caractère) ───────
const WORD_RE = /[\p{L}\p{M}][\p{L}\p{M}'’-]*/gu
export function findIssues(text: string, offset: number): SpellIssue[] {
  const out: SpellIssue[] = []
  // 1) Orthographe — fautes fréquentes
  let m: RegExpExecArray | null
  WORD_RE.lastIndex = 0
  const words: Array<{ w: string; i: number }> = []
  while ((m = WORD_RE.exec(text))) {
    const w = m[0]
    words.push({ w, i: m.index })
    // Ignorer : trop court, contient un chiffre, acronyme tout en majuscules, dico perso.
    if (w.length < 2 || /\d/.test(w) || isIgnored(w)) continue
    if (w.length >= 2 && w === w.toUpperCase()) continue
    // Détection par Hunspell (FR ou EN). Si correct → rien. Sinon → faute orthographe.
    // (Tant que les dictionnaires ne sont pas chargés, `isWordCorrect` renvoie true.)
    if (isWordCorrect(w)) continue
    const key = w.toLowerCase().replace(/^['’-]+|['’-]+$/g, '')
    // Suggestion précise depuis la liste de fautes fréquentes ; sinon calculée à la
    // demande (clic droit) via `suggestWord` (coûteux → pas ici).
    out.push({ from: offset + m.index, to: offset + m.index + w.length, type: 'spelling', word: w, suggestions: COMMON_MISTAKES[key] ?? [] })
  }
  // 2) Grammaire — mot répété consécutif (même mot, ≥3 lettres)
  for (let k = 1; k < words.length; k++) {
    const a = words[k - 1], b = words[k]
    if (a.w.length >= 3 && a.w.toLowerCase() === b.w.toLowerCase()) {
      // s'assurer qu'il n'y a que des espaces entre les deux
      const between = text.slice(a.i + a.w.length, b.i)
      if (/^\s+$/.test(between) && !isIgnored('§rep§' + b.w)) {
        out.push({ from: offset + b.i, to: offset + b.i + b.w.length, type: 'grammar', word: b.w, suggestions: [], message: 'Mot répété' })
      }
    }
  }
  // 3) Grammaire — double espace
  const dbl = /  +/g
  let d: RegExpExecArray | null
  while ((d = dbl.exec(text))) {
    out.push({ from: offset + d.index, to: offset + d.index + d[0].length, type: 'grammar', word: d[0], suggestions: [' '], message: 'Espace en trop' })
  }
  return out
}
