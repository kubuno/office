// Catalogue de formes PARTAGÉ par les sous-modules d'office (documents et tableur).
//
// Extrait de DocumentEditorPage : le tableur n'avait que 10 formes maison face aux
// 144 de l'éditeur de documents. Plutôt que d'entretenir deux jeux divergents, les
// deux consomment ce module — un utilisateur retrouve les mêmes formes, les mêmes
// libellés et le même rendu qu'il dessine dans un texte ou dans une feuille.

export type ShapeKind =
  // Traits
  | 'line' | 'lineArrow' | 'lineDouble' | 'elbowConnector' | 'elbowArrow' | 'elbowDoubleArrow'
  | 'curveConnector' | 'curveArrow' | 'curveDoubleArrow' | 'curve'
  // Rectangles
  | 'rect' | 'roundRect' | 'snipRect' | 'snip2SameRect' | 'snip2DiagRect' | 'snipRoundRect'
  | 'roundRect1' | 'round2SameRect' | 'round2DiagRect' | 'plaque' | 'frame'
  // Formes de base
  | 'ellipse' | 'triangle' | 'rtTriangle' | 'parallelogram' | 'trapezoid' | 'diamond'
  | 'pentagon' | 'hexagon' | 'heptagon' | 'octagon' | 'decagon' | 'dodecagon'
  | 'pie' | 'chord' | 'teardrop' | 'halfFrame' | 'corner' | 'diagStripe'
  | 'cross' | 'bevel' | 'cylinder' | 'cube' | 'blockArc' | 'foldedCorner'
  | 'heart' | 'lightning' | 'sun' | 'moon' | 'cloud' | 'smiley' | 'arc' | 'donut' | 'noSymbol'
  | 'leftBrace' | 'rightBrace' | 'leftBracket' | 'rightBracket' | 'doubleBrace' | 'doubleBracket'
  // Flèches pleines
  | 'arrow' | 'arrowLeft' | 'arrowUp' | 'arrowDown' | 'arrowLeftRight' | 'arrowUpDown'
  | 'arrowQuad' | 'leftRightUpArrow' | 'chevron' | 'pentagonArrow' | 'bentArrow' | 'bentUpArrow'
  | 'uTurnArrow' | 'curvedRightArrow' | 'curvedLeftArrow' | 'curvedUpArrow' | 'curvedDownArrow'
  | 'stripedRightArrow' | 'notchedArrow' | 'circularArrow'
  | 'rightArrowCallout' | 'leftArrowCallout' | 'upArrowCallout' | 'downArrowCallout'
  // Formes d'équation
  | 'mathPlus' | 'mathMinus' | 'mathMultiply' | 'mathDivide' | 'mathEqual' | 'mathNotEqual'
  // Organigrammes
  | 'flowProcess' | 'flowAltProcess' | 'flowDecision' | 'flowData' | 'flowPredefined'
  | 'flowInternal' | 'flowDocument' | 'flowMultidoc' | 'flowTerminator' | 'flowPreparation'
  | 'flowManualInput' | 'flowManualOp' | 'flowConnector' | 'flowCard' | 'flowPunchedTape' | 'flowOr'
  | 'flowSumming' | 'flowCollate' | 'flowSort' | 'flowExtract' | 'flowMerge' | 'flowStored'
  | 'flowSequential' | 'flowMagneticDisk' | 'flowDirectAccess' | 'flowDisplay'
  | 'flowDelay' | 'flowOffPage'
  // Étoiles et bannières
  | 'star4' | 'star' | 'star6' | 'star7' | 'star8' | 'star10' | 'star12' | 'star16' | 'star24' | 'star32'
  | 'explosion1' | 'explosion2' | 'ribbon' | 'ribbonDown' | 'ribbonCurved'
  | 'scrollH' | 'scrollV' | 'wave' | 'doubleWave'
  // Bulles et légendes
  | 'calloutRect' | 'calloutRoundRect' | 'calloutOval' | 'calloutCloud'
  | 'lineCallout' | 'calloutLine2' | 'calloutLineAccent'

// Taille d'insertion par défaut selon la forme (lignes plates, flèches hautes, …).
export function shapeDefaultSize(kind: ShapeKind): { w: number; h: number } {
  if (kind === 'line' || kind === 'lineArrow' || kind === 'lineDouble' || kind === 'elbowConnector' || kind === 'elbowArrow' || kind === 'elbowDoubleArrow' || kind === 'curveConnector' || kind === 'curveArrow' || kind === 'curveDoubleArrow' || kind === 'curve') return { w: 280, h: 90 }
  if (kind === 'arrowUp' || kind === 'arrowDown' || kind === 'arrowUpDown' || kind === 'upArrowCallout' || kind === 'downArrowCallout' || kind === 'bentUpArrow') return { w: 160, h: 240 }
  if (kind === 'leftBrace' || kind === 'rightBrace' || kind === 'leftBracket' || kind === 'rightBracket' || kind === 'doubleBrace' || kind === 'doubleBracket') return { w: 70, h: 220 }
  if (kind === 'scrollV') return { w: 170, h: 230 }
  if (kind.startsWith('star') || kind.startsWith('explosion') || kind === 'cross' || kind === 'noSymbol' || kind === 'sun' || kind === 'smiley' || kind === 'arrowQuad' || kind === 'leftRightUpArrow' || kind === 'circularArrow' || kind === 'flowConnector' || kind === 'donut' || kind === 'flowOr' || kind === 'flowSumming') return { w: 200, h: 200 }
  return { w: 240, h: 180 }
}

// Catalogue des formes par catégorie (façon ruban Word). Libellés FR par défaut.
export interface ShapeDef { kind: ShapeKind | 'textBox'; label: string }
export interface ShapeCat { id: string; title: string; shapes: ShapeDef[] }
export const SHAPE_CATALOG: ShapeCat[] = [
  { id: 'lines', title: 'Traits', shapes: [
    { kind: 'line', label: 'Trait' }, { kind: 'lineArrow', label: 'Flèche' }, { kind: 'lineDouble', label: 'Double flèche' },
    { kind: 'elbowConnector', label: 'Connecteur coudé' }, { kind: 'elbowArrow', label: 'Connecteur coudé fléché' }, { kind: 'elbowDoubleArrow', label: 'Connecteur coudé double flèche' },
    { kind: 'curveConnector', label: 'Connecteur courbe' }, { kind: 'curveArrow', label: 'Connecteur courbe fléché' }, { kind: 'curveDoubleArrow', label: 'Connecteur courbe double flèche' }, { kind: 'curve', label: 'Courbe' },
  ] },
  { id: 'rectangles', title: 'Rectangles', shapes: [
    { kind: 'rect', label: 'Rectangle' }, { kind: 'roundRect', label: 'Rectangle arrondi' }, { kind: 'snipRect', label: 'Coin coupé' },
    { kind: 'snip2SameRect', label: 'Deux coins coupés (même côté)' }, { kind: 'snip2DiagRect', label: 'Deux coins coupés (diagonale)' }, { kind: 'snipRoundRect', label: 'Coin coupé et arrondi' },
    { kind: 'roundRect1', label: 'Un coin arrondi' }, { kind: 'round2SameRect', label: 'Deux coins arrondis (même côté)' }, { kind: 'round2DiagRect', label: 'Deux coins arrondis (diagonale)' }, { kind: 'plaque', label: 'Plaque' }, { kind: 'frame', label: 'Cadre' },
  ] },
  { id: 'basic', title: 'Formes de base', shapes: [
    { kind: 'textBox', label: 'Zone de texte' },
    { kind: 'ellipse', label: 'Ellipse' }, { kind: 'triangle', label: 'Triangle isocèle' }, { kind: 'rtTriangle', label: 'Triangle rectangle' },
    { kind: 'parallelogram', label: 'Parallélogramme' }, { kind: 'trapezoid', label: 'Trapèze' }, { kind: 'diamond', label: 'Losange' },
    { kind: 'pentagon', label: 'Pentagone' }, { kind: 'hexagon', label: 'Hexagone' }, { kind: 'heptagon', label: 'Heptagone' }, { kind: 'octagon', label: 'Octogone' },
    { kind: 'decagon', label: 'Décagone' }, { kind: 'dodecagon', label: 'Dodécagone' }, { kind: 'pie', label: 'Camembert' }, { kind: 'chord', label: 'Corde' }, { kind: 'teardrop', label: 'Goutte' },
    { kind: 'halfFrame', label: 'Demi-cadre' }, { kind: 'corner', label: 'Coin' }, { kind: 'diagStripe', label: 'Bande diagonale' },
    { kind: 'cross', label: 'Croix' }, { kind: 'bevel', label: 'Biseau' }, { kind: 'cylinder', label: 'Cylindre' }, { kind: 'cube', label: 'Cube' }, { kind: 'blockArc', label: 'Arc plein' }, { kind: 'foldedCorner', label: 'Coin replié' },
    { kind: 'heart', label: 'Cœur' }, { kind: 'lightning', label: 'Éclair' }, { kind: 'sun', label: 'Soleil' }, { kind: 'moon', label: 'Lune' }, { kind: 'cloud', label: 'Nuage' },
    { kind: 'smiley', label: 'Visage souriant' }, { kind: 'arc', label: 'Arc' }, { kind: 'donut', label: 'Anneau' }, { kind: 'noSymbol', label: 'Symbole interdit' },
    { kind: 'leftBracket', label: 'Crochet gauche' }, { kind: 'rightBracket', label: 'Crochet droit' }, { kind: 'doubleBracket', label: 'Crochets' },
    { kind: 'leftBrace', label: 'Accolade gauche' }, { kind: 'rightBrace', label: 'Accolade droite' }, { kind: 'doubleBrace', label: 'Accolades' },
  ] },
  { id: 'arrows', title: 'Flèches pleines', shapes: [
    { kind: 'arrow', label: 'Flèche droite' }, { kind: 'arrowLeft', label: 'Flèche gauche' }, { kind: 'arrowUp', label: 'Flèche haut' }, { kind: 'arrowDown', label: 'Flèche bas' },
    { kind: 'arrowLeftRight', label: 'Flèche gauche-droite' }, { kind: 'arrowUpDown', label: 'Flèche haut-bas' }, { kind: 'arrowQuad', label: 'Flèche quadruple' }, { kind: 'leftRightUpArrow', label: 'Flèche gauche-droite-haut' },
    { kind: 'bentArrow', label: 'Flèche coudée' }, { kind: 'bentUpArrow', label: 'Flèche coudée vers le haut' }, { kind: 'uTurnArrow', label: 'Flèche demi-tour' },
    { kind: 'curvedRightArrow', label: 'Flèche courbe droite' }, { kind: 'curvedLeftArrow', label: 'Flèche courbe gauche' }, { kind: 'curvedUpArrow', label: 'Flèche courbe haut' }, { kind: 'curvedDownArrow', label: 'Flèche courbe bas' },
    { kind: 'stripedRightArrow', label: 'Flèche rayée' }, { kind: 'notchedArrow', label: 'Flèche en V' }, { kind: 'pentagonArrow', label: 'Flèche pentagone' }, { kind: 'chevron', label: 'Chevron' }, { kind: 'circularArrow', label: 'Flèche circulaire' },
    { kind: 'rightArrowCallout', label: 'Légende flèche droite' }, { kind: 'leftArrowCallout', label: 'Légende flèche gauche' }, { kind: 'upArrowCallout', label: 'Légende flèche haut' }, { kind: 'downArrowCallout', label: 'Légende flèche bas' },
  ] },
  { id: 'equation', title: "Formes d'équation", shapes: [
    { kind: 'mathPlus', label: 'Plus' }, { kind: 'mathMinus', label: 'Moins' }, { kind: 'mathMultiply', label: 'Multiplier' },
    { kind: 'mathDivide', label: 'Diviser' }, { kind: 'mathEqual', label: 'Égal' }, { kind: 'mathNotEqual', label: 'Différent' },
  ] },
  { id: 'flowchart', title: 'Organigrammes', shapes: [
    { kind: 'flowProcess', label: 'Processus' }, { kind: 'flowAltProcess', label: 'Autre processus' }, { kind: 'flowDecision', label: 'Décision' },
    { kind: 'flowData', label: 'Données' }, { kind: 'flowPredefined', label: 'Processus prédéfini' }, { kind: 'flowInternal', label: 'Stockage interne' },
    { kind: 'flowDocument', label: 'Document' }, { kind: 'flowMultidoc', label: 'Plusieurs documents' }, { kind: 'flowTerminator', label: 'Terminaison' },
    { kind: 'flowPreparation', label: 'Préparation' }, { kind: 'flowManualInput', label: 'Saisie manuelle' }, { kind: 'flowManualOp', label: 'Opération manuelle' },
    { kind: 'flowConnector', label: 'Connecteur' }, { kind: 'flowOffPage', label: 'Renvoi de page' }, { kind: 'flowCard', label: 'Carte' }, { kind: 'flowPunchedTape', label: 'Bande perforée' },
    { kind: 'flowOr', label: 'Ou' }, { kind: 'flowSumming', label: 'Jonction de sommation' }, { kind: 'flowCollate', label: 'Assemblage' }, { kind: 'flowSort', label: 'Tri' },
    { kind: 'flowExtract', label: 'Extraction' }, { kind: 'flowMerge', label: 'Fusion' }, { kind: 'flowStored', label: 'Données stockées' }, { kind: 'flowDelay', label: 'Délai' },
    { kind: 'flowSequential', label: 'Accès séquentiel' }, { kind: 'flowMagneticDisk', label: 'Disque magnétique' }, { kind: 'flowDirectAccess', label: 'Accès direct' }, { kind: 'flowDisplay', label: 'Affichage' },
  ] },
  { id: 'stars', title: 'Étoiles et bannières', shapes: [
    { kind: 'explosion1', label: 'Explosion 1' }, { kind: 'explosion2', label: 'Explosion 2' },
    { kind: 'star4', label: 'Étoile à 4 branches' }, { kind: 'star', label: 'Étoile à 5 branches' }, { kind: 'star6', label: 'Étoile à 6 branches' }, { kind: 'star7', label: 'Étoile à 7 branches' },
    { kind: 'star8', label: 'Étoile à 8 branches' }, { kind: 'star10', label: 'Étoile à 10 branches' }, { kind: 'star12', label: 'Étoile à 12 branches' }, { kind: 'star16', label: 'Étoile à 16 branches' }, { kind: 'star24', label: 'Étoile à 24 branches' }, { kind: 'star32', label: 'Étoile à 32 branches' },
    { kind: 'ribbon', label: 'Bannière' }, { kind: 'ribbonDown', label: 'Bannière vers le bas' }, { kind: 'ribbonCurved', label: 'Bannière courbe' },
    { kind: 'scrollH', label: 'Parchemin horizontal' }, { kind: 'scrollV', label: 'Parchemin vertical' }, { kind: 'wave', label: 'Vague' }, { kind: 'doubleWave', label: 'Double vague' },
  ] },
  { id: 'callouts', title: 'Bulles et légendes', shapes: [
    { kind: 'calloutRect', label: 'Bulle rectangulaire' }, { kind: 'calloutRoundRect', label: 'Bulle arrondie' }, { kind: 'calloutOval', label: 'Bulle ovale' }, { kind: 'calloutCloud', label: 'Bulle nuage' },
    { kind: 'lineCallout', label: 'Légende avec trait' }, { kind: 'calloutLine2', label: 'Légende trait à 2 segments' }, { kind: 'calloutLineAccent', label: 'Légende trait avec barre' },
  ] },
]

