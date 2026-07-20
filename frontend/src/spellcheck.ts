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

// ── Règles de grammaire (options, façon Word « Règle de style : Grammaire ») ─────
// `key` = identifiant stable (persisté) ; `label` = libellé FR affiché ; `impl` = un
// détecteur existe réellement (les autres sont listés pour la parité mais inertes tant
// qu'aucun détecteur n'est ajouté). L'ordre suit la liste de Word.
export interface GrammarRuleDef { key: string; label: string; impl?: boolean }
export const GRAMMAR_RULES: GrammarRuleDef[] = [
  { key: 'nounAgreement',  label: 'Accord des noms, adjectifs et déterminants' },
  { key: 'verbAgreement',  label: 'Accord du sujet et du verbe' },
  { key: 'contractedArt',  label: 'Articles définis contractés ("du", "des", "au", "aux")' },
  { key: 'comparative',    label: 'Comparatif/superlatif' },
  { key: 'elision',        label: 'Élision' },
  { key: 'auxiliaries',    label: 'Emploi des auxiliaires' },
  { key: 'pronouns',       label: 'Emploi des pronoms' },
  { key: 'punctSpace',     label: 'Espacement avant ou après une ponctuation', impl: true },
  { key: 'wordSpace',      label: 'Espacement entre deux mots', impl: true },
  { key: 'hyphenVerb',     label: 'Emploi du trait d\'union avec le verbe' },
  { key: 'attribForm',     label: 'Forme de l\'adjectif attribut et du participe passé' },
  { key: 'invariable',     label: 'Forme invariable de certains mots et groupes de mots' },
  { key: 'phonetic',       label: 'Harmonie phonétique (ex. "ce" vs "cet")' },
  { key: 'homophones',     label: 'Homophones (ex. "a" vs "à", "mangé" vs "manger")' },
  { key: 'sentenceCap',    label: 'Majuscule en début de phrase', impl: true },
  { key: 'negation',       label: 'Négation' },
  { key: 'repeatedWord',   label: 'Mot répété', impl: true },
  { key: 'punctSeq',       label: 'Séquence de ponctuations' },
  { key: 'verbTense',      label: 'Temps et modes des verbes' },
]
// Une règle est active sauf si explicitement désactivée dans `rules`.
const ruleOn = (rules: Record<string, boolean> | undefined, key: string): boolean => !rules || rules[key] !== false

// Clé d'ignore STABLE par TYPE de faute grammaticale (préfixe + jeton). Employée par les
// règles (skip si ignorée) ET par le panneau « Vérification » (Ignorer / Ne pas rechercher).
export function grammarIgnoreKey(message: string | undefined, word: string): string {
  switch (message) {
    case 'Mot répété':                    return '§rep§' + word
    case 'Majuscule en début de phrase':  return '§maj§' + word
    case 'Espace avant la ponctuation':   return '§sp§' + word.trim()
    case 'Espace en trop':                return '§dsp§'
    default:                              return word
  }
}
// Explication lisible d'une règle de grammaire (panneau « Vérification », façon Word).
export const GRAMMAR_EXPLAIN: Record<string, string> = {
  'Mot répété':                    'Supprimez le mot répété.',
  'Majuscule en début de phrase':  'Commencez la phrase par une majuscule.',
  'Espace avant la ponctuation':   'Supprimez l\'espace avant le signe de ponctuation.',
  'Espace en trop':                'Supprimez l\'espace en trop.',
}

// ── Analyse d'un fragment de texte (offset = position PM du 1er caractère) ───────
// `opts` active/désactive indépendamment ORTHOGRAPHE et GRAMMAIRE (façon Word) — voir
// les interrupteurs « Correcteur orthographique » / « Grammaire » de la barre de statut.
const WORD_RE = /[\p{L}\p{M}][\p{L}\p{M}'’-]*/gu
export function findIssues(text: string, offset: number, opts: { spelling?: boolean; grammar?: boolean; lang?: string | null; blockStart?: boolean; rules?: Record<string, boolean> } = {}): SpellIssue[] {
  const spelling = opts.spelling !== false
  const grammar  = opts.grammar  !== false
  const lang     = opts.lang ?? null   // langue de la plage (marque spellLang) ou null = active
  const blockStart = opts.blockStart === true   // ce fragment commence son bloc (paragraphe/titre)
  const rules    = opts.rules          // catégories de grammaire activées (undefined = toutes)
  const out: SpellIssue[] = []
  // On balaye TOUJOURS les mots (la grammaire « mot répété » en a besoin) ; les fautes
  // d'orthographe ne sont émises que si `spelling` est actif.
  let m: RegExpExecArray | null
  WORD_RE.lastIndex = 0
  const words: Array<{ w: string; i: number }> = []
  while ((m = WORD_RE.exec(text))) {
    const w = m[0]
    words.push({ w, i: m.index })
    if (!spelling) continue
    // Ignorer : trop court, contient un chiffre, acronyme tout en majuscules, dico perso.
    if (w.length < 2 || /\d/.test(w) || isIgnored(w)) continue
    if (w.length >= 2 && w === w.toUpperCase()) continue
    // Détection par Hunspell (FR ou EN). Si correct → rien. Sinon → faute orthographe.
    // (Tant que les dictionnaires ne sont pas chargés, `isWordCorrect` renvoie true.)
    if (isWordCorrect(w, lang)) continue
    const key = w.toLowerCase().replace(/^['’-]+|['’-]+$/g, '')
    // Suggestion précise depuis la liste de fautes fréquentes ; sinon calculée à la
    // demande (clic droit) via `suggestWord` (coûteux → pas ici).
    out.push({ from: offset + m.index, to: offset + m.index + w.length, type: 'spelling', word: w, suggestions: COMMON_MISTAKES[key] ?? [] })
  }
  if (grammar) {
    // Grammaire 1) mot répété consécutif (même mot, ≥3 lettres)
    if (ruleOn(rules, 'repeatedWord')) for (let k = 1; k < words.length; k++) {
      const a = words[k - 1], b = words[k]
      if (a.w.length >= 3 && a.w.toLowerCase() === b.w.toLowerCase()) {
        // s'assurer qu'il n'y a que des espaces entre les deux
        const between = text.slice(a.i + a.w.length, b.i)
        if (/^\s+$/.test(between) && !isIgnored('§rep§' + b.w)) {
          out.push({ from: offset + b.i, to: offset + b.i + b.w.length, type: 'grammar', word: b.w, suggestions: [], message: 'Mot répété' })
        }
      }
    }
    // Grammaire 2) double espace
    const dbl = /  +/g
    let d: RegExpExecArray | null
    if (ruleOn(rules, 'wordSpace')) while ((d = dbl.exec(text))) {
      if (isIgnored(grammarIgnoreKey('Espace en trop', d[0]))) continue
      out.push({ from: offset + d.index, to: offset + d.index + d[0].length, type: 'grammar', word: d[0], suggestions: [' '], message: 'Espace en trop' })
    }
    // Grammaire 3) espace AVANT une virgule ou un point (typographie FR : « , » et « . »
    // ne prennent jamais d'espace avant — contrairement à « ; : ! ? » qui en prennent un).
    const sp = / +([,.])/g
    let s: RegExpExecArray | null
    if (ruleOn(rules, 'punctSpace')) while ((s = sp.exec(text))) {
      if (isIgnored(grammarIgnoreKey('Espace avant la ponctuation', s[0]))) continue
      out.push({ from: offset + s.index, to: offset + s.index + s[0].length, type: 'grammar', word: s[0], suggestions: [s[1]], message: 'Espace avant la ponctuation' })
    }
    // Grammaire 4) majuscule en début de phrase (comme Word). Débuts de phrase : début du
    // BLOC (si `blockStart`) + après « . ! ? … » (éventuel guillemet/parenthèse fermante)
    // suivi d'espaces. Le 1er mot doit commencer par une lettre → sinon on suggère la capitale.
    const starts: number[] = []
    if (ruleOn(rules, 'sentenceCap')) {
    if (blockStart) { const m0 = /\S/u.exec(text); if (m0) starts.push(m0.index) }
    // NB : PAS de `+` sur la classe de ponctuation. `[.!?…]+` (gourmand) rebrousse chemin
    // caractère par caractère quand `\s+` échoue → backtracking catastrophique O(n²) sur un
    // long run de « . » / « … » (ex. points de suite), qui GELAIT le thread sur un gros
    // document (200k points ≈ 55 s). Un seul signal terminal juste avant l'espace suffit :
    // « … » ou « ?! » donnent la MÊME position de début de phrase (vérifié), en temps linéaire.
    const sentRe = /[.!?…]["»'’)\]]?\s+/gu
    let se: RegExpExecArray | null
    while ((se = sentRe.exec(text))) { const after = se.index + se[0].length; if (after < text.length) starts.push(after) }
    for (const si of starts) {
      const ch = text[si]
      if (!ch || !/\p{Ll}/u.test(ch)) continue           // doit débuter par une minuscule
      WORD_RE.lastIndex = si
      const wm = WORD_RE.exec(text)
      if (!wm || wm.index !== si) continue
      const word = wm[0]
      if (word.length < 2 || isIgnored('§maj§' + word)) continue
      const cap = word.charAt(0).toLocaleUpperCase('fr') + word.slice(1)
      if (cap === word) continue
      out.push({ from: offset + si, to: offset + si + word.length, type: 'grammar', word, suggestions: [cap], message: 'Majuscule en début de phrase' })
    }
    }
  }
  return out
}
