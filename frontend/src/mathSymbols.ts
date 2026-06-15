// Palette de symboles/modèles LaTeX du sous-module Maths (façon LibreOffice Math).
// `tex` = aperçu rendu dans le bouton de la palette (avec des cases \square).
// `ins` = code LaTeX inséré au curseur ; le marqueur ‸ indique où placer le curseur
//         après insertion (retiré du texte inséré).

export interface MathTemplate {
  tex:    string   // aperçu KaTeX
  ins:    string   // code inséré (peut contenir le marqueur ‸)
  title?: string   // infobulle
}

export interface MathCategory {
  id:    string
  label: string
  items: MathTemplate[]
}

export const CARET = '‸'

const s = (latex: string, title?: string): MathTemplate => ({ tex: latex, ins: latex, title })

export const MATH_CATEGORIES: MathCategory[] = [
  {
    id: 'formats', label: 'Mise en forme',
    items: [
      { tex: '\\frac{\\square}{\\square}',     ins: '\\frac{‸}{}',       title: 'Fraction' },
      { tex: '\\sqrt{\\square}',               ins: '\\sqrt{‸}',         title: 'Racine carrée' },
      { tex: '\\sqrt[\\square]{\\square}',     ins: '\\sqrt[‸]{}',       title: 'Racine n-ième' },
      { tex: '\\square^{\\square}',            ins: '‸^{}',              title: 'Exposant' },
      { tex: '\\square_{\\square}',            ins: '‸_{}',              title: 'Indice' },
      { tex: '\\square_{\\square}^{\\square}', ins: '‸_{}^{}',           title: 'Indice & exposant' },
      { tex: '\\overline{\\square}',           ins: '\\overline{‸}',     title: 'Surligne' },
      { tex: '\\underline{\\square}',          ins: '\\underline{‸}',    title: 'Souligne' },
      { tex: '\\vec{\\square}',                ins: '\\vec{‸}',          title: 'Vecteur' },
      { tex: '\\hat{\\square}',                ins: '\\hat{‸}',          title: 'Chapeau' },
      { tex: '\\dot{\\square}',                ins: '\\dot{‸}',          title: 'Point' },
      { tex: '\\tilde{\\square}',              ins: '\\tilde{‸}',        title: 'Tilde' },
    ],
  },
  {
    id: 'ops', label: 'Opérateurs',
    items: ['+', '-', '\\times', '\\div', '\\pm', '\\mp', '\\cdot', '\\ast', '\\circ', '\\star', '\\otimes', '\\oplus', '\\odot', '\\setminus'].map(x => s(x)),
  },
  {
    id: 'relations', label: 'Relations',
    items: ['=', '\\neq', '<', '>', '\\leq', '\\geq', '\\ll', '\\gg', '\\approx', '\\equiv', '\\cong', '\\sim', '\\simeq', '\\propto', '\\parallel', '\\perp', '\\doteq'].map(x => s(x)),
  },
  {
    id: 'big', label: 'Grands opérateurs',
    items: [
      { tex: '\\sum_{\\square}^{\\square}',  ins: '\\sum_{‸}^{}',  title: 'Somme' },
      { tex: '\\prod_{\\square}^{\\square}', ins: '\\prod_{‸}^{}', title: 'Produit' },
      { tex: '\\int_{\\square}^{\\square}',  ins: '\\int_{‸}^{}',  title: 'Intégrale' },
      { tex: '\\oint_{\\square}',            ins: '\\oint_{‸}',    title: 'Intégrale de contour' },
      { tex: '\\iint',                       ins: '\\iint ',       title: 'Double intégrale' },
      { tex: '\\lim_{\\square}',             ins: '\\lim_{‸}',     title: 'Limite' },
      { tex: '\\bigcup_{\\square}',          ins: '\\bigcup_{‸}',  title: 'Union' },
      { tex: '\\bigcap_{\\square}',          ins: '\\bigcap_{‸}',  title: 'Intersection' },
    ],
  },
  {
    id: 'brackets', label: 'Crochets',
    items: [
      { tex: '(\\square)',                       ins: '(‸)',                       title: 'Parenthèses' },
      { tex: '[\\square]',                       ins: '[‸]',                       title: 'Crochets' },
      { tex: '\\{\\square\\}',                   ins: '\\{‸\\}',                   title: 'Accolades' },
      { tex: '\\langle\\square\\rangle',         ins: '\\langle ‸\\rangle',        title: 'Chevrons' },
      { tex: '\\left|\\square\\right|',          ins: '\\left|‸\\right|',          title: 'Valeur absolue' },
      { tex: '\\left\\|\\square\\right\\|',      ins: '\\left\\|‸\\right\\|',      title: 'Norme' },
      { tex: '\\lceil\\square\\rceil',           ins: '\\lceil ‸\\rceil',          title: 'Plafond' },
      { tex: '\\lfloor\\square\\rfloor',         ins: '\\lfloor ‸\\rfloor',        title: 'Plancher' },
      { tex: '\\left(\\frac{\\square}{\\square}\\right)', ins: '\\left(\\frac{‸}{}\\right)', title: 'Parenthèses extensibles' },
    ],
  },
  {
    id: 'greek', label: 'Lettres grecques',
    items: ['\\alpha', '\\beta', '\\gamma', '\\delta', '\\epsilon', '\\varepsilon', '\\zeta', '\\eta', '\\theta', '\\vartheta', '\\iota', '\\kappa', '\\lambda', '\\mu', '\\nu', '\\xi', '\\pi', '\\rho', '\\sigma', '\\tau', '\\upsilon', '\\phi', '\\varphi', '\\chi', '\\psi', '\\omega', '\\Gamma', '\\Delta', '\\Theta', '\\Lambda', '\\Xi', '\\Pi', '\\Sigma', '\\Phi', '\\Psi', '\\Omega'].map(x => s(x)),
  },
  {
    id: 'functions', label: 'Fonctions',
    items: ['\\sin', '\\cos', '\\tan', '\\cot', '\\sec', '\\csc', '\\arcsin', '\\arccos', '\\arctan', '\\sinh', '\\cosh', '\\tanh', '\\log', '\\ln', '\\exp', '\\max', '\\min', '\\deg', '\\gcd'].map(x => ({ tex: x, ins: x + ' ' })),
  },
  {
    id: 'arrows', label: 'Flèches',
    items: ['\\to', '\\rightarrow', '\\leftarrow', '\\leftrightarrow', '\\Rightarrow', '\\Leftarrow', '\\Leftrightarrow', '\\mapsto', '\\uparrow', '\\downarrow', '\\nearrow', '\\searrow', '\\hookrightarrow', '\\rightleftharpoons'].map(x => s(x)),
  },
  {
    id: 'logic', label: 'Logique & ensembles',
    items: ['\\in', '\\notin', '\\ni', '\\subset', '\\subseteq', '\\supset', '\\supseteq', '\\cup', '\\cap', '\\emptyset', '\\forall', '\\exists', '\\nexists', '\\neg', '\\wedge', '\\vee', '\\implies', '\\iff', '\\therefore', '\\because', '\\mathbb{R}', '\\mathbb{N}', '\\mathbb{Z}', '\\mathbb{Q}', '\\mathbb{C}', '\\infty', '\\partial', '\\nabla'].map(x => s(x)),
  },
  {
    id: 'structures', label: 'Structures',
    items: [
      { tex: '\\begin{matrix}\\square&\\square\\\\\\square&\\square\\end{matrix}', ins: '\\begin{matrix}\n‸ & \\\\\n & \n\\end{matrix}', title: 'Matrice' },
      { tex: '\\begin{pmatrix}\\square&\\square\\\\\\square&\\square\\end{pmatrix}', ins: '\\begin{pmatrix}\n‸ & \\\\\n & \n\\end{pmatrix}', title: 'Matrice ( )' },
      { tex: '\\begin{bmatrix}\\square&\\square\\\\\\square&\\square\\end{bmatrix}', ins: '\\begin{bmatrix}\n‸ & \\\\\n & \n\\end{bmatrix}', title: 'Matrice [ ]' },
      { tex: '\\begin{vmatrix}\\square&\\square\\\\\\square&\\square\\end{vmatrix}', ins: '\\begin{vmatrix}\n‸ & \\\\\n & \n\\end{vmatrix}', title: 'Déterminant' },
      { tex: '\\begin{cases}\\square\\\\\\square\\end{cases}', ins: '\\begin{cases}\n‸ & \\text{si } \\\\\n & \\text{sinon}\n\\end{cases}', title: 'Système (accolade)' },
      { tex: '\\binom{\\square}{\\square}', ins: '\\binom{‸}{}', title: 'Coefficient binomial' },
    ],
  },
]
