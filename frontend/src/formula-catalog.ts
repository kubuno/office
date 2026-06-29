// ── Catalogue des fonctions de formule ───────────────────────────────────────
// Construit à partir de TOUTES les fonctions du moteur (KNOWN_FUNCTIONS) → catégorie,
// syntaxe et libellé pour l'autocomplétion ET le navigateur de fonctions. Couvre
// les ~270 fonctions (avant : ~40 codées en dur).

import { KNOWN_FUNCTIONS } from './formula-engine'

export type FnCat =
  | 'math' | 'stat' | 'logical' | 'lookup' | 'text' | 'date'
  | 'info' | 'engineering' | 'database' | 'array' | 'other'

export const CAT_LABEL: Record<FnCat, string> = {
  math: 'Maths', stat: 'Statistiques', logical: 'Logique', lookup: 'Recherche',
  text: 'Texte', date: 'Date & heure', info: 'Information', engineering: 'Ingénierie',
  database: 'Base de données', array: 'Tableaux dynamiques', other: 'Autres',
}
export const CAT_COLOR: Record<FnCat, string> = {
  math: '#1a73e8', stat: '#12b5cb', logical: '#9334e6', lookup: '#188038',
  text: '#e8710a', date: '#d93025', info: '#795548', engineering: '#5f6368',
  database: '#0b8043', array: '#a142f4', other: '#80868b',
}

// Appartenance explicite par catégorie (le reste est déduit par heuristique).
const GROUPS: Partial<Record<FnCat, string[]>> = {
  math: ['SUM','PRODUCT','ABS','SIGN','MOD','POWER','SQRT','SQRTPI','EXP','LN','LOG','LOG10','INT','TRUNC','ROUND','ROUNDUP','ROUNDDOWN','MROUND','CEILING','FLOOR','EVEN','ODD','GCD','LCM','FACT','FACTDOUBLE','COMBIN','COMBINA','PERMUT','MULTINOMIAL','QUOTIENT','PI','RAND','RANDBETWEEN','DEGREES','RADIANS','SIN','COS','TAN','ASIN','ACOS','ATAN','ATAN2','SINH','COSH','TANH','ASINH','ACOSH','ATANH','SEC','CSC','COT','SECH','CSCH','COTH','ACOT','ACOTH','SUMSQ','SUMPRODUCT','SUMIF','SUMIFS','SUMX2MY2','SUMX2PY2','SUMXMY2','SERIESSUM','ARABIC','ROMAN','BASE','DECIMAL','SUBTOTAL'],
  stat: ['AVERAGE','AVERAGEA','AVERAGEIF','AVERAGEIFS','COUNT','COUNTA','COUNTBLANK','COUNTIF','COUNTIFS','MAX','MAXA','MAXIFS','MIN','MINA','MINIFS','MEDIAN','MODE','LARGE','SMALL','RANK','PERCENTILE','PERCENTRANK','QUARTILE','STDEV','STDEVA','STDEVP','STDEVPA','VAR','VARA','VARP','VARPA','DEVSQ','AVEDEV','GEOMEAN','HARMEAN','TRIMMEAN','KURT','SKEW','CORREL','PEARSON','RSQ','SLOPE','INTERCEPT','FORECAST','STEYX','STANDARDIZE','CONFIDENCE','NORMDIST','NORMINV','NORMSDIST','NORMSINV','BINOMDIST','POISSON','EXPONDIST','FISHER','FISHERINV','GAMMALN','GAUSS','PHI','PERMUTATIONA'],
  logical: ['IF','IFS','IFERROR','IFNA','AND','OR','NOT','XOR','TRUE','FALSE','SWITCH'],
  lookup: ['VLOOKUP','HLOOKUP','INDEX','MATCH','LOOKUP','CHOOSE','OFFSET','ROW','COLUMN','ROWS','COLUMNS','TRANSPOSE','HYPERLINK','GETPIVOTDATA'],
  text: ['CONCAT','CONCATENATE','TEXTJOIN','LEFT','RIGHT','MID','LEN','LOWER','UPPER','PROPER','TRIM','SUBSTITUTE','REPLACE','TEXT','VALUE','FIND','SEARCH','REPT','CHAR','CODE','UNICHAR','UNICODE','EXACT','NUMBERVALUE','DOLLAR','FIXED'],
  date: ['DATE','TIME','TODAY','NOW','YEAR','MONTH','DAY','HOUR','MINUTE','SECOND','WEEKDAY','WEEKNUM','ISOWEEKNUM','DATEDIF','DATEVALUE','TIMEVALUE','EDATE','EOMONTH','NETWORKDAYS','WORKDAY','DAYS','DAYS360','YEARFRAC'],
  info: ['ISBLANK','ISERR','ISERROR','ISEVEN','ISODD','ISFORMULA','ISLOGICAL','ISNA','ISNONTEXT','ISNUMBER','ISREF','ISTEXT','ISOMITTED','NA','TYPE','SHEET','SHEETS','CELL','INFO'],
  database: ['DSUM','DAVERAGE','DCOUNT','DCOUNTA','DMAX','DMIN','DGET','DPRODUCT','DSTDEV','DSTDEVP','DVAR','DVARP'],
  array: ['FILTER','SORT','SORTBY','UNIQUE','SEQUENCE','RANDARRAY','MAP','REDUCE','SCAN','BYROW','BYCOL','MAKEARRAY','LAMBDA','LET','MMULT','MINVERSE','MDETERM','MUNIT','XLOOKUP'],
}
const CAT: Record<string, FnCat> = {}
for (const [cat, names] of Object.entries(GROUPS)) for (const n of names!) CAT[n] = cat as FnCat

function guessCat(name: string): FnCat {
  if (name.startsWith('IM') || name.startsWith('BESSEL') || /BIN|DEC|HEX|OCT|COMPLEX|CONVERT|BIT|DELTA|GESTEP|ERF/.test(name)) return 'engineering'
  if (name.startsWith('IS')) return 'info'
  if (/DIST|INV|STDEV|VAR|MEAN|MEDIAN|MODE|RANK|QUART|PERCENT/.test(name)) return 'stat'
  return 'other'
}

// Syntaxes riches (les plus utilisées) ; le reste reçoit « NOM(…) ».
const SYN: Record<string, string> = {
  SUM: 'SUM(nombre1, ...)', PRODUCT: 'PRODUCT(nombre1, ...)', AVERAGE: 'AVERAGE(nombre1, ...)',
  COUNT: 'COUNT(valeur1, ...)', COUNTA: 'COUNTA(valeur1, ...)', COUNTBLANK: 'COUNTBLANK(plage)',
  MIN: 'MIN(nombre1, ...)', MAX: 'MAX(nombre1, ...)', MEDIAN: 'MEDIAN(nombre1, ...)',
  IF: 'IF(condition, si_vrai, si_faux)', IFS: 'IFS(cond1, val1, ...)', IFERROR: 'IFERROR(valeur, si_erreur)',
  IFNA: 'IFNA(valeur, si_na)', AND: 'AND(val1, val2, ...)', OR: 'OR(val1, val2, ...)', NOT: 'NOT(valeur)',
  XOR: 'XOR(val1, val2, ...)', SWITCH: 'SWITCH(expr, cas1, val1, ..., [défaut])',
  ROUND: 'ROUND(nombre, décimales)', ROUNDUP: 'ROUNDUP(nombre, décimales)', ROUNDDOWN: 'ROUNDDOWN(nombre, décimales)',
  FLOOR: 'FLOOR(nombre, [multiple])', CEILING: 'CEILING(nombre, [multiple])', MROUND: 'MROUND(nombre, multiple)',
  ABS: 'ABS(nombre)', MOD: 'MOD(nombre, diviseur)', POWER: 'POWER(nombre, exposant)', SQRT: 'SQRT(nombre)',
  INT: 'INT(nombre)', TRUNC: 'TRUNC(nombre, [décimales])', SIGN: 'SIGN(nombre)', EXP: 'EXP(nombre)',
  LN: 'LN(nombre)', LOG: 'LOG(nombre, [base])', LOG10: 'LOG10(nombre)', GCD: 'GCD(nombre1, ...)', LCM: 'LCM(nombre1, ...)',
  SUMIF: 'SUMIF(plage, critère, [plage_somme])', SUMIFS: 'SUMIFS(plage_somme, plage1, crit1, ...)',
  COUNTIF: 'COUNTIF(plage, critère)', COUNTIFS: 'COUNTIFS(plage1, crit1, ...)',
  AVERAGEIF: 'AVERAGEIF(plage, critère, [plage_moy])', AVERAGEIFS: 'AVERAGEIFS(plage_moy, plage1, crit1, ...)',
  MAXIFS: 'MAXIFS(plage_max, plage1, crit1, ...)', MINIFS: 'MINIFS(plage_min, plage1, crit1, ...)',
  SUMPRODUCT: 'SUMPRODUCT(plage1, [plage2], ...)', SUBTOTAL: 'SUBTOTAL(fonction, plage1, ...)',
  VLOOKUP: 'VLOOKUP(valeur, plage, colonne, [exact])', HLOOKUP: 'HLOOKUP(valeur, plage, ligne, [exact])',
  XLOOKUP: 'XLOOKUP(valeur, plage_rech, plage_rés, [si_absent], [mode], [sens])',
  INDEX: 'INDEX(plage, ligne, [colonne])', MATCH: 'MATCH(valeur, plage, [type])', LOOKUP: 'LOOKUP(valeur, vecteur, [résultat])',
  CHOOSE: 'CHOOSE(index, val1, val2, ...)', OFFSET: 'OFFSET(réf, lignes, colonnes, [hauteur], [largeur])',
  ROW: 'ROW([réf])', COLUMN: 'COLUMN([réf])', ROWS: 'ROWS(plage)', COLUMNS: 'COLUMNS(plage)', TRANSPOSE: 'TRANSPOSE(plage)',
  LEN: 'LEN(texte)', LEFT: 'LEFT(texte, [nb])', RIGHT: 'RIGHT(texte, [nb])', MID: 'MID(texte, début, nb)',
  UPPER: 'UPPER(texte)', LOWER: 'LOWER(texte)', PROPER: 'PROPER(texte)', TRIM: 'TRIM(texte)',
  CONCAT: 'CONCAT(texte1, ...)', CONCATENATE: 'CONCATENATE(texte1, ...)', TEXTJOIN: 'TEXTJOIN(sép, ignorer_vides, texte1, ...)',
  SUBSTITUTE: 'SUBSTITUTE(texte, ancien, nouveau, [occurrence])', REPLACE: 'REPLACE(texte, début, nb, nouveau)',
  FIND: 'FIND(cherché, texte, [début])', SEARCH: 'SEARCH(cherché, texte, [début])', REPT: 'REPT(texte, n)',
  TEXT: 'TEXT(valeur, format)', VALUE: 'VALUE(texte)', EXACT: 'EXACT(texte1, texte2)', CHAR: 'CHAR(code)', CODE: 'CODE(texte)',
  DATE: 'DATE(année, mois, jour)', TIME: 'TIME(heure, minute, seconde)', TODAY: 'TODAY()', NOW: 'NOW()',
  YEAR: 'YEAR(date)', MONTH: 'MONTH(date)', DAY: 'DAY(date)', HOUR: 'HOUR(heure)', MINUTE: 'MINUTE(heure)', SECOND: 'SECOND(heure)',
  WEEKDAY: 'WEEKDAY(date, [type])', WEEKNUM: 'WEEKNUM(date, [type])', DATEDIF: 'DATEDIF(début, fin, unité)',
  EDATE: 'EDATE(date, mois)', EOMONTH: 'EOMONTH(date, mois)', NETWORKDAYS: 'NETWORKDAYS(début, fin, [fériés])',
  WORKDAY: 'WORKDAY(début, jours, [fériés])', DAYS: 'DAYS(fin, début)', YEARFRAC: 'YEARFRAC(début, fin, [base])',
  ISBLANK: 'ISBLANK(valeur)', ISNUMBER: 'ISNUMBER(valeur)', ISTEXT: 'ISTEXT(valeur)', ISERROR: 'ISERROR(valeur)',
  ISNA: 'ISNA(valeur)', NA: 'NA()', LAMBDA: 'LAMBDA([param, ...], calcul)', LET: 'LET(nom1, val1, ..., calcul)',
  MAP: 'MAP(tableau1, ..., lambda)', REDUCE: 'REDUCE(init, tableau, lambda)', SCAN: 'SCAN(init, tableau, lambda)',
  FILTER: 'FILTER(plage, condition, [si_vide])', SORT: 'SORT(plage, [col], [ordre])', UNIQUE: 'UNIQUE(plage)',
  SEQUENCE: 'SEQUENCE(lignes, [colonnes], [début], [pas])', RANDARRAY: 'RANDARRAY([lignes], [colonnes])',
}

export interface CatalogFn { name: string; cat: FnCat; syntax: string }

export const FUNCTION_CATALOG: CatalogFn[] = KNOWN_FUNCTIONS
  .filter(n => /^[A-Z][A-Z0-9_.]*$/.test(n))
  .map(name => ({ name, cat: CAT[name] ?? guessCat(name), syntax: SYN[name] ?? `${name}(…)` }))
  .sort((a, b) => a.name.localeCompare(b.name))

export const CATALOG_BY_NAME = new Map(FUNCTION_CATALOG.map(f => [f.name, f]))
export const ALL_FN_NAMES = new Set(FUNCTION_CATALOG.map(f => f.name))
