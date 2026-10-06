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
  {
    id: 'accents', label: 'Accents',
    items: [
      { tex: '\\bar{\\square}', ins: '\\bar{‸}', title: 'Barre' },
      { tex: '\\hat{\\square}', ins: '\\hat{‸}', title: 'Chapeau' },
      { tex: '\\tilde{\\square}', ins: '\\tilde{‸}', title: 'Tilde' },
      { tex: '\\check{\\square}', ins: '\\check{‸}', title: 'Caron' },
      { tex: '\\breve{\\square}', ins: '\\breve{‸}', title: 'Brève' },
      { tex: '\\acute{\\square}', ins: '\\acute{‸}', title: 'Accent aigu' },
      { tex: '\\grave{\\square}', ins: '\\grave{‸}', title: 'Accent grave' },
      { tex: '\\dot{\\square}', ins: '\\dot{‸}', title: 'Point' },
      { tex: '\\ddot{\\square}', ins: '\\ddot{‸}', title: 'Tréma' },
      { tex: '\\dddot{\\square}', ins: '\\dddot{‸}', title: 'Triple point' },
      { tex: '\\mathring{\\square}', ins: '\\mathring{‸}', title: 'Rond' },
      { tex: '\\widehat{\\square}', ins: '\\widehat{‸}', title: 'Chapeau large' },
      { tex: '\\widetilde{\\square}', ins: '\\widetilde{‸}', title: 'Tilde large' },
      { tex: '\\overrightarrow{\\square}', ins: '\\overrightarrow{‸}', title: 'Flèche droite' },
      { tex: '\\overleftarrow{\\square}', ins: '\\overleftarrow{‸}', title: 'Flèche gauche' },
      { tex: '\\overleftrightarrow{\\square}', ins: '\\overleftrightarrow{‸}', title: 'Double flèche' },
      { tex: '\\underbrace{\\square}', ins: '\\underbrace{‸}_{}', title: 'Accolade dessous' },
      { tex: '\\overbrace{\\square}', ins: '\\overbrace{‸}^{}', title: 'Accolade dessus' },
    ],
  },
  {
    id: 'fonts', label: 'Polices & styles',
    items: [
      { tex: '\\mathbb{R}', ins: '\\mathbb{‸}', title: 'Ajouré (blackboard)' },
      { tex: '\\mathcal{L}', ins: '\\mathcal{‸}', title: 'Calligraphique' },
      { tex: '\\mathfrak{g}', ins: '\\mathfrak{‸}', title: 'Fraktur' },
      { tex: '\\mathrm{d}', ins: '\\mathrm{‸}', title: 'Romain' },
      { tex: '\\mathbf{v}', ins: '\\mathbf{‸}', title: 'Gras' },
      { tex: '\\mathit{x}', ins: '\\mathit{‸}', title: 'Italique' },
      { tex: '\\mathsf{A}', ins: '\\mathsf{‸}', title: 'Sans-serif' },
      { tex: '\\mathtt{x}', ins: '\\mathtt{‸}', title: 'Machine à écrire' },
      { tex: '\\boldsymbol{\\alpha}', ins: '\\boldsymbol{‸}', title: 'Symbole gras' },
      { tex: '\\text{abc}', ins: '\\text{‸}', title: 'Texte' },
      { tex: '\\textbf{abc}', ins: '\\textbf{‸}', title: 'Texte gras' },
      { tex: '\\textit{abc}', ins: '\\textit{‸}', title: 'Texte italique' },
      { tex: '\\boxed{\\square}', ins: '\\boxed{‸}', title: 'Encadré' },
      { tex: '\\cancel{\\square}', ins: '\\cancel{‸}', title: 'Barré' },
      { tex: '\\color{red}{\\square}', ins: '\\color{red}{‸}', title: 'Couleur' },
      { tex: '\\underset{\\square}{\\square}', ins: '\\underset{‸}{}', title: 'Sous-placé' },
      { tex: '\\overset{\\square}{\\square}', ins: '\\overset{‸}{}', title: 'Sur-placé' },
      { tex: '\\stackrel{\\square}{\\square}', ins: '\\stackrel{‸}{}', title: 'Empilé' },
    ],
  },
  {
    id: 'calculus', label: 'Analyse',
    items: [
      { tex: '\\frac{d\\square}{d\\square}', ins: '\\frac{d‸}{d}', title: 'Dérivée' },
      { tex: '\\frac{d^{2}\\square}{d\\square^{2}}', ins: '\\frac{d^{2}‸}{d^{2}}', title: 'Dérivée seconde' },
      { tex: '\\frac{\\partial\\square}{\\partial\\square}', ins: '\\frac{\\partial ‸}{\\partial }', title: 'Dérivée partielle' },
      { tex: '\\frac{\\partial^{2}\\square}{\\partial\\square^{2}}', ins: '\\frac{\\partial^{2}‸}{\\partial ^{2}}', title: 'Partielle seconde' },
      { tex: '\\int_{\\square}^{\\square}\\square\\,d\\square', ins: '\\int_{‸}^{} \\,d', title: 'Intégrale définie' },
      { tex: '\\iint_{\\square}\\square\\,dA', ins: '\\iint_{‸} \\,dA', title: 'Double intégrale' },
      { tex: '\\iiint_{\\square}\\square\\,dV', ins: '\\iiint_{‸} \\,dV', title: 'Triple intégrale' },
      { tex: '\\oint_{\\square}\\square\\,d\\square', ins: '\\oint_{‸} \\,d', title: 'Intégrale curviligne' },
      { tex: '\\lim_{\\square\\to\\square}\\square', ins: '\\lim_{‸\\to } ', title: 'Limite' },
      { tex: '\\lim_{\\square\\to\\infty}\\square', ins: '\\lim_{‸\\to\\infty} ', title: 'Limite ∞' },
      { tex: '\\sum_{\\square=\\square}^{\\square}\\square', ins: '\\sum_{‸=}^{} ', title: 'Série' },
      { tex: '\\nabla\\square', ins: '\\nabla ‸', title: 'Gradient' },
      { tex: '\\nabla\\cdot\\square', ins: '\\nabla\\cdot ‸', title: 'Divergence' },
      { tex: '\\nabla\\times\\square', ins: '\\nabla\\times ‸', title: 'Rotationnel' },
      { tex: '\\nabla^{2}\\square', ins: '\\nabla^{2} ‸', title: 'Laplacien' },
      { tex: '\\square\'', ins: '‸\'', title: 'Prime' },
      { tex: '\\square\'\'', ins: '‸\'\'', title: 'Double prime' },
    ],
  },
  {
    id: 'setsx', label: 'Ensembles & logique +',
    items: ['\\subsetneq', '\\supsetneq', '\\sqsubseteq', '\\sqsupseteq', '\\sqcup', '\\sqcap', '\\uplus', '\\biguplus', '\\bigsqcup', '\\complement', '\\bot', '\\top', '\\vdash', '\\dashv', '\\models', '\\nvdash', '\\angle', '\\measuredangle', '\\sphericalangle', '\\triangle', '\\square', '\\blacksquare', '\\Diamond', '\\lozenge'].map(x => s(x)),
  },
  {
    id: 'arrowsx', label: 'Flèches +',
    items: ['\\Longrightarrow', '\\Longleftarrow', '\\Longleftrightarrow', '\\longmapsto', '\\longrightarrow', '\\longleftarrow', '\\rightrightarrows', '\\leftleftarrows', '\\rightleftarrows', '\\twoheadrightarrow', '\\rightarrowtail', '\\hookleftarrow', '\\rightsquigarrow', '\\leadsto', '\\curvearrowright', '\\curvearrowleft', '\\circlearrowright', '\\circlearrowleft', '\\Uparrow', '\\Downarrow', '\\Updownarrow', '\\upharpoonright', '\\downharpoonright', '\\nrightarrow', '\\nleftarrow', '\\nLeftrightarrow'].map(x => s(x)),
  },
  {
    id: 'opsx', label: 'Opérateurs +',
    items: ['\\bullet', '\\diamond', '\\bigtriangleup', '\\bigtriangledown', '\\triangleleft', '\\triangleright', '\\wr', '\\amalg', '\\dagger', '\\ddagger', '\\boxplus', '\\boxtimes', '\\boxminus', '\\boxdot', '\\ominus', '\\oslash', '\\bigstar', '\\bigodot', '\\bigotimes', '\\bigoplus', '\\barwedge', '\\veebar', '\\intercal', '\\rtimes', '\\ltimes', '\\curlywedge', '\\curlyvee'].map(x => s(x)),
  },
  {
    id: 'relx', label: 'Relations +',
    items: ['\\prec', '\\succ', '\\preceq', '\\succeq', '\\precsim', '\\succsim', '\\asymp', '\\bowtie', '\\vDash', '\\Vdash', '\\nmid', '\\mid', '\\smile', '\\frown', '\\between', '\\pitchfork', '\\lesssim', '\\gtrsim', '\\lessgtr', '\\gtrless', '\\trianglelefteq', '\\trianglerighteq', '\\subseteqq', '\\supseteqq', '\\approxeq', '\\backsim'].map(x => s(x)),
  },
  {
    id: 'misc', label: 'Symboles divers',
    items: ['\\aleph', '\\beth', '\\gimel', '\\hbar', '\\ell', '\\wp', '\\Re', '\\Im', '\\Finv', '\\Game', '\\imath', '\\jmath', '\\flat', '\\natural', '\\sharp', '\\clubsuit', '\\diamondsuit', '\\heartsuit', '\\spadesuit', '\\cdots', '\\vdots', '\\ddots', '\\ldots', '\\dots', '\\backslash', '\\prime', '\\degree', '\\angle', '\\surd', '\\checkmark'].map(x => s(x)),
  },
  {
    id: 'formulas', label: 'Formules usuelles',
    items: [
      { tex: 'x=\\frac{-b\\pm\\sqrt{b^{2}-4ac}}{2a}', ins: 'x=\\frac{-b\\pm\\sqrt{b^{2}-4ac}}{2a}‸', title: 'Équation quadratique' },
      { tex: 'a^{2}+b^{2}=c^{2}', ins: 'a^{2}+b^{2}=c^{2}‸', title: 'Pythagore' },
      { tex: 'e^{i\\pi}+1=0', ins: 'e^{i\\pi}+1=0‸', title: "Identité d'Euler" },
      { tex: '(a+b)^{n}=\\sum_{k=0}^{n}\\binom{n}{k}a^{n-k}b^{k}', ins: '(a+b)^{n}=\\sum_{k=0}^{n}\\binom{n}{k}a^{n-k}b^{k}‸', title: 'Binôme de Newton' },
      { tex: "f'(x)=\\lim_{h\\to 0}\\frac{f(x+h)-f(x)}{h}", ins: "f'(x)=\\lim_{h\\to 0}\\frac{f(x+h)-f(x)}{h}‸", title: 'Définition de la dérivée' },
      { tex: '\\int u\\,dv=uv-\\int v\\,du', ins: '\\int u\\,dv=uv-\\int v\\,du‸', title: 'Intégration par parties' },
      { tex: '\\sum_{k=1}^{n}k=\\frac{n(n+1)}{2}', ins: '\\sum_{k=1}^{n}k=\\frac{n(n+1)}{2}‸', title: 'Somme 1..n' },
      { tex: '\\sum_{k=0}^{\\infty}ar^{k}=\\frac{a}{1-r}', ins: '\\sum_{k=0}^{\\infty}ar^{k}=\\frac{a}{1-r}‸', title: 'Série géométrique' },
      { tex: '\\bar{x}=\\frac{1}{n}\\sum_{i=1}^{n}x_{i}', ins: '\\bar{x}=\\frac{1}{n}\\sum_{i=1}^{n}x_{i}‸', title: 'Moyenne' },
      { tex: '\\sigma=\\sqrt{\\frac{1}{n}\\sum_{i=1}^{n}(x_{i}-\\bar{x})^{2}}', ins: '\\sigma=\\sqrt{\\frac{1}{n}\\sum_{i=1}^{n}(x_{i}-\\bar{x})^{2}}‸', title: 'Écart-type' },
      { tex: 'P(A\\mid B)=\\frac{P(B\\mid A)P(A)}{P(B)}', ins: 'P(A\\mid B)=\\frac{P(B\\mid A)P(A)}{P(B)}‸', title: 'Théorème de Bayes' },
      { tex: 'd=\\sqrt{(x_2-x_1)^{2}+(y_2-y_1)^{2}}', ins: 'd=\\sqrt{(x_2-x_1)^{2}+(y_2-y_1)^{2}}‸', title: 'Distance' },
      { tex: 'E=mc^{2}', ins: 'E=mc^{2}‸', title: 'Énergie de masse' },
      { tex: '\\cos^{2}\\theta+\\sin^{2}\\theta=1', ins: '\\cos^{2}\\theta+\\sin^{2}\\theta=1‸', title: 'Identité trigo' },
    ],
  },
  {
    id: 'environments', label: 'Environnements',
    items: [
      { tex: '\\begin{aligned}\\square&=\\square\\\\&=\\square\\end{aligned}', ins: '\\begin{aligned}\n‸ &= \\\\\n &= \n\\end{aligned}', title: 'Alignement' },
      { tex: '\\begin{gathered}\\square\\\\\\square\\end{gathered}', ins: '\\begin{gathered}\n‸ \\\\\n \n\\end{gathered}', title: 'Centré multi-lignes' },
      { tex: '\\begin{array}{cc}\\square&\\square\\\\\\square&\\square\\end{array}', ins: '\\begin{array}{cc}\n‸ & \\\\\n & \n\\end{array}', title: 'Tableau' },
      { tex: '\\begin{matrix}\\square&\\square&\\square\\\\\\square&\\square&\\square\\\\\\square&\\square&\\square\\end{matrix}', ins: '\\begin{matrix}\n‸ & & \\\\\n & & \\\\\n & & \n\\end{matrix}', title: 'Matrice 3×3' },
      { tex: '\\begin{Bmatrix}\\square&\\square\\\\\\square&\\square\\end{Bmatrix}', ins: '\\begin{Bmatrix}\n‸ & \\\\\n & \n\\end{Bmatrix}', title: 'Matrice { }' },
      { tex: '\\begin{Vmatrix}\\square&\\square\\\\\\square&\\square\\end{Vmatrix}', ins: '\\begin{Vmatrix}\n‸ & \\\\\n & \n\\end{Vmatrix}', title: 'Matrice ‖ ‖' },
      { tex: '\\begin{smallmatrix}\\square&\\square\\\\\\square&\\square\\end{smallmatrix}', ins: '\\begin{smallmatrix}\n‸ & \\\\\n & \n\\end{smallmatrix}', title: 'Petite matrice' },
    ],
  },
  {
    id: 'spacing', label: 'Espaces & ponctuation',
    items: ['\\,', '\\:', '\\;', '\\quad', '\\qquad', '\\!', '\\ ', '\\colon', '\\cdotp', '\\ldotp', '\\vert', '\\Vert', '\\|', '\\#', '\\%', '\\&', '\\_', '\\{', '\\}'].map(x => ({ tex: x === '\\,' || x.startsWith('\\ ') || x === '\\:' || x === '\\;' || x === '\\!' ? '\\square' + x + '\\square' : x, ins: x, title: x })),
  },
]
