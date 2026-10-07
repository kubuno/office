//! The formula-function catalogue: a port of the web editor's `formula-catalog.ts` and of the autocomplete
//! tables at the top of `SpreadsheetApp.tsx` (`FORMULA_FUNCTIONS`, `FN_CATEGORY`, `CAT_COLOR`, `fnColor`).
//!
//! * [`FnCat`], [`cat_label`], [`cat_color`], [`GROUPS`], [`guess_cat`] and the rich [`SYNTAXES`] give every
//!   engine function a category, a syntax string and a label;
//! * [`function_catalog`] is `FUNCTION_CATALOG`: the names of `known_functions()` that look like function
//!   names, with category and syntax, sorted like `String.prototype.localeCompare` (see
//!   [`locale_compare_names`]);
//! * [`FORMULA_FUNCTIONS`] are the 42 most common functions with their i18n description key and the French
//!   (and English) description texts of that key, from the web's `i18n.ts`.
//!
//! The catalogue is built from the functions *registered in this crate*, so it grows as the function
//! modules are ported; for the web's complete set it is identical entry by entry.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::formula::functions::known_functions;

/// `FnCat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnCat {
    Math,
    Stat,
    Logical,
    Lookup,
    Text,
    Date,
    Info,
    Engineering,
    Database,
    Array,
    Other,
}

impl FnCat {
    /// The web's string key (`'math'`, `'stat'`, …).
    pub fn key(self) -> &'static str {
        match self {
            FnCat::Math => "math",
            FnCat::Stat => "stat",
            FnCat::Logical => "logical",
            FnCat::Lookup => "lookup",
            FnCat::Text => "text",
            FnCat::Date => "date",
            FnCat::Info => "info",
            FnCat::Engineering => "engineering",
            FnCat::Database => "database",
            FnCat::Array => "array",
            FnCat::Other => "other",
        }
    }
}

/// The categories in the web's key order.
pub const ALL_CATS: [FnCat; 11] =
    [FnCat::Math, FnCat::Stat, FnCat::Logical, FnCat::Lookup, FnCat::Text, FnCat::Date, FnCat::Info, FnCat::Engineering, FnCat::Database, FnCat::Array, FnCat::Other];

/// `CAT_LABEL[cat]` (French).
pub fn cat_label(cat: FnCat) -> &'static str {
    match cat {
        FnCat::Math => "Maths",
        FnCat::Stat => "Statistiques",
        FnCat::Logical => "Logique",
        FnCat::Lookup => "Recherche",
        FnCat::Text => "Texte",
        FnCat::Date => "Date & heure",
        FnCat::Info => "Information",
        FnCat::Engineering => "Ingénierie",
        FnCat::Database => "Base de données",
        FnCat::Array => "Tableaux dynamiques",
        FnCat::Other => "Autres",
    }
}

/// `CAT_COLOR[cat]` of `formula-catalog.ts`.
pub fn cat_color(cat: FnCat) -> &'static str {
    match cat {
        FnCat::Math => "#1a73e8",
        FnCat::Stat => "#12b5cb",
        FnCat::Logical => "#9334e6",
        FnCat::Lookup => "#188038",
        FnCat::Text => "#e8710a",
        FnCat::Date => "#d93025",
        FnCat::Info => "#795548",
        FnCat::Engineering => "#5f6368",
        FnCat::Database => "#0b8043",
        FnCat::Array => "#a142f4",
        FnCat::Other => "#80868b",
    }
}

/// Explicit membership by category (the rest is guessed by [`guess_cat`]).
pub const GROUPS: [(FnCat, &[&str]); 9] = [
    (FnCat::Math, &["SUM","PRODUCT","ABS","SIGN","MOD","POWER","SQRT","SQRTPI","EXP","LN","LOG","LOG10","INT","TRUNC","ROUND","ROUNDUP","ROUNDDOWN","MROUND","CEILING","FLOOR","EVEN","ODD","GCD","LCM","FACT","FACTDOUBLE","COMBIN","COMBINA","PERMUT","MULTINOMIAL","QUOTIENT","PI","RAND","RANDBETWEEN","DEGREES","RADIANS","SIN","COS","TAN","ASIN","ACOS","ATAN","ATAN2","SINH","COSH","TANH","ASINH","ACOSH","ATANH","SEC","CSC","COT","SECH","CSCH","COTH","ACOT","ACOTH","SUMSQ","SUMPRODUCT","SUMIF","SUMIFS","SUMX2MY2","SUMX2PY2","SUMXMY2","SERIESSUM","ARABIC","ROMAN","BASE","DECIMAL","SUBTOTAL"]),
    (FnCat::Stat, &["AVERAGE","AVERAGEA","AVERAGEIF","AVERAGEIFS","COUNT","COUNTA","COUNTBLANK","COUNTIF","COUNTIFS","MAX","MAXA","MAXIFS","MIN","MINA","MINIFS","MEDIAN","MODE","LARGE","SMALL","RANK","PERCENTILE","PERCENTRANK","QUARTILE","STDEV","STDEVA","STDEVP","STDEVPA","VAR","VARA","VARP","VARPA","DEVSQ","AVEDEV","GEOMEAN","HARMEAN","TRIMMEAN","KURT","SKEW","CORREL","PEARSON","RSQ","SLOPE","INTERCEPT","FORECAST","STEYX","STANDARDIZE","CONFIDENCE","NORMDIST","NORMINV","NORMSDIST","NORMSINV","BINOMDIST","POISSON","EXPONDIST","FISHER","FISHERINV","GAMMALN","GAUSS","PHI","PERMUTATIONA"]),
    (FnCat::Logical, &["IF","IFS","IFERROR","IFNA","AND","OR","NOT","XOR","TRUE","FALSE","SWITCH"]),
    (FnCat::Lookup, &["VLOOKUP","HLOOKUP","INDEX","MATCH","LOOKUP","CHOOSE","OFFSET","ROW","COLUMN","ROWS","COLUMNS","TRANSPOSE","HYPERLINK","GETPIVOTDATA"]),
    (FnCat::Text, &["CONCAT","CONCATENATE","TEXTJOIN","LEFT","RIGHT","MID","LEN","LOWER","UPPER","PROPER","TRIM","SUBSTITUTE","REPLACE","TEXT","VALUE","FIND","SEARCH","REPT","CHAR","CODE","UNICHAR","UNICODE","EXACT","NUMBERVALUE","DOLLAR","FIXED"]),
    (FnCat::Date, &["DATE","TIME","TODAY","NOW","YEAR","MONTH","DAY","HOUR","MINUTE","SECOND","WEEKDAY","WEEKNUM","ISOWEEKNUM","DATEDIF","DATEVALUE","TIMEVALUE","EDATE","EOMONTH","NETWORKDAYS","WORKDAY","DAYS","DAYS360","YEARFRAC"]),
    (FnCat::Info, &["ISBLANK","ISERR","ISERROR","ISEVEN","ISODD","ISFORMULA","ISLOGICAL","ISNA","ISNONTEXT","ISNUMBER","ISREF","ISTEXT","ISOMITTED","NA","TYPE","SHEET","SHEETS","CELL","INFO"]),
    (FnCat::Database, &["DSUM","DAVERAGE","DCOUNT","DCOUNTA","DMAX","DMIN","DGET","DPRODUCT","DSTDEV","DSTDEVP","DVAR","DVARP"]),
    (FnCat::Array, &["FILTER","SORT","SORTBY","UNIQUE","SEQUENCE","RANDARRAY","MAP","REDUCE","SCAN","BYROW","BYCOL","MAKEARRAY","LAMBDA","LET","MMULT","MINVERSE","MDETERM","MUNIT","XLOOKUP","GROUPBY","PIVOTBY"]),
];

fn cat_by_name() -> &'static HashMap<&'static str, FnCat> {
    static M: OnceLock<HashMap<&'static str, FnCat>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = HashMap::new();
        for (cat, names) in GROUPS {
            for n in names {
                m.insert(*n, cat);
            }
        }
        m
    })
}

/// `guessCat`: the heuristic category of a function that is in no group.
pub fn guess_cat(name: &str) -> FnCat {
    if name.starts_with("IM")
        || name.starts_with("BESSEL")
        || ["BIN", "DEC", "HEX", "OCT", "COMPLEX", "CONVERT", "BIT", "DELTA", "GESTEP", "ERF"].iter().any(|p| name.contains(p))
    {
        return FnCat::Engineering;
    }
    if name.starts_with("IS") {
        return FnCat::Info;
    }
    if ["DIST", "INV", "STDEV", "VAR", "MEAN", "MEDIAN", "MODE", "RANK", "QUART", "PERCENT"].iter().any(|p| name.contains(p)) {
        return FnCat::Stat;
    }
    FnCat::Other
}

/// `SYN`: the rich syntaxes of the most used functions (the rest gets `NAME(…)`).
pub const SYNTAXES: [(&str, &str); 118] = [
    ("SUM", "SUM(nombre1, ...)"),
    ("PRODUCT", "PRODUCT(nombre1, ...)"),
    ("AVERAGE", "AVERAGE(nombre1, ...)"),
    ("COUNT", "COUNT(valeur1, ...)"),
    ("COUNTA", "COUNTA(valeur1, ...)"),
    ("COUNTBLANK", "COUNTBLANK(plage)"),
    ("MIN", "MIN(nombre1, ...)"),
    ("MAX", "MAX(nombre1, ...)"),
    ("MEDIAN", "MEDIAN(nombre1, ...)"),
    ("IF", "IF(condition, si_vrai, si_faux)"),
    ("IFS", "IFS(cond1, val1, ...)"),
    ("IFERROR", "IFERROR(valeur, si_erreur)"),
    ("IFNA", "IFNA(valeur, si_na)"),
    ("AND", "AND(val1, val2, ...)"),
    ("OR", "OR(val1, val2, ...)"),
    ("NOT", "NOT(valeur)"),
    ("XOR", "XOR(val1, val2, ...)"),
    ("SWITCH", "SWITCH(expr, cas1, val1, ..., [défaut])"),
    ("ROUND", "ROUND(nombre, décimales)"),
    ("ROUNDUP", "ROUNDUP(nombre, décimales)"),
    ("ROUNDDOWN", "ROUNDDOWN(nombre, décimales)"),
    ("FLOOR", "FLOOR(nombre, [multiple])"),
    ("CEILING", "CEILING(nombre, [multiple])"),
    ("MROUND", "MROUND(nombre, multiple)"),
    ("ABS", "ABS(nombre)"),
    ("MOD", "MOD(nombre, diviseur)"),
    ("POWER", "POWER(nombre, exposant)"),
    ("SQRT", "SQRT(nombre)"),
    ("INT", "INT(nombre)"),
    ("TRUNC", "TRUNC(nombre, [décimales])"),
    ("SIGN", "SIGN(nombre)"),
    ("EXP", "EXP(nombre)"),
    ("LN", "LN(nombre)"),
    ("LOG", "LOG(nombre, [base])"),
    ("LOG10", "LOG10(nombre)"),
    ("GCD", "GCD(nombre1, ...)"),
    ("LCM", "LCM(nombre1, ...)"),
    ("SUMIF", "SUMIF(plage, critère, [plage_somme])"),
    ("SUMIFS", "SUMIFS(plage_somme, plage1, crit1, ...)"),
    ("COUNTIF", "COUNTIF(plage, critère)"),
    ("COUNTIFS", "COUNTIFS(plage1, crit1, ...)"),
    ("AVERAGEIF", "AVERAGEIF(plage, critère, [plage_moy])"),
    ("AVERAGEIFS", "AVERAGEIFS(plage_moy, plage1, crit1, ...)"),
    ("MAXIFS", "MAXIFS(plage_max, plage1, crit1, ...)"),
    ("MINIFS", "MINIFS(plage_min, plage1, crit1, ...)"),
    ("SUMPRODUCT", "SUMPRODUCT(plage1, [plage2], ...)"),
    ("SUBTOTAL", "SUBTOTAL(fonction, plage1, ...)"),
    ("VLOOKUP", "VLOOKUP(valeur, plage, colonne, [exact])"),
    ("HLOOKUP", "HLOOKUP(valeur, plage, ligne, [exact])"),
    ("XLOOKUP", "XLOOKUP(valeur, plage_rech, plage_rés, [si_absent], [mode], [sens])"),
    ("GROUPBY", "GROUPBY(champs_lignes, valeurs, fonction, [en-têtes], [totaux], [tri], [filtre])"),
    ("PIVOTBY", "PIVOTBY(champs_lignes, champs_colonnes, valeurs, fonction, [en-têtes], [totaux_lig], [tri_lig], [totaux_col], [tri_col], [filtre])"),
    ("INDEX", "INDEX(plage, ligne, [colonne])"),
    ("MATCH", "MATCH(valeur, plage, [type])"),
    ("LOOKUP", "LOOKUP(valeur, vecteur, [résultat])"),
    ("CHOOSE", "CHOOSE(index, val1, val2, ...)"),
    ("OFFSET", "OFFSET(réf, lignes, colonnes, [hauteur], [largeur])"),
    ("ROW", "ROW([réf])"),
    ("COLUMN", "COLUMN([réf])"),
    ("ROWS", "ROWS(plage)"),
    ("COLUMNS", "COLUMNS(plage)"),
    ("TRANSPOSE", "TRANSPOSE(plage)"),
    ("LEN", "LEN(texte)"),
    ("LEFT", "LEFT(texte, [nb])"),
    ("RIGHT", "RIGHT(texte, [nb])"),
    ("MID", "MID(texte, début, nb)"),
    ("UPPER", "UPPER(texte)"),
    ("LOWER", "LOWER(texte)"),
    ("PROPER", "PROPER(texte)"),
    ("TRIM", "TRIM(texte)"),
    ("CONCAT", "CONCAT(texte1, ...)"),
    ("CONCATENATE", "CONCATENATE(texte1, ...)"),
    ("TEXTJOIN", "TEXTJOIN(sép, ignorer_vides, texte1, ...)"),
    ("SUBSTITUTE", "SUBSTITUTE(texte, ancien, nouveau, [occurrence])"),
    ("REPLACE", "REPLACE(texte, début, nb, nouveau)"),
    ("FIND", "FIND(cherché, texte, [début])"),
    ("SEARCH", "SEARCH(cherché, texte, [début])"),
    ("REPT", "REPT(texte, n)"),
    ("TEXT", "TEXT(valeur, format)"),
    ("VALUE", "VALUE(texte)"),
    ("EXACT", "EXACT(texte1, texte2)"),
    ("CHAR", "CHAR(code)"),
    ("CODE", "CODE(texte)"),
    ("DATE", "DATE(année, mois, jour)"),
    ("TIME", "TIME(heure, minute, seconde)"),
    ("TODAY", "TODAY()"),
    ("NOW", "NOW()"),
    ("YEAR", "YEAR(date)"),
    ("MONTH", "MONTH(date)"),
    ("DAY", "DAY(date)"),
    ("HOUR", "HOUR(heure)"),
    ("MINUTE", "MINUTE(heure)"),
    ("SECOND", "SECOND(heure)"),
    ("WEEKDAY", "WEEKDAY(date, [type])"),
    ("WEEKNUM", "WEEKNUM(date, [type])"),
    ("DATEDIF", "DATEDIF(début, fin, unité)"),
    ("EDATE", "EDATE(date, mois)"),
    ("EOMONTH", "EOMONTH(date, mois)"),
    ("NETWORKDAYS", "NETWORKDAYS(début, fin, [fériés])"),
    ("WORKDAY", "WORKDAY(début, jours, [fériés])"),
    ("DAYS", "DAYS(fin, début)"),
    ("YEARFRAC", "YEARFRAC(début, fin, [base])"),
    ("ISBLANK", "ISBLANK(valeur)"),
    ("ISNUMBER", "ISNUMBER(valeur)"),
    ("ISTEXT", "ISTEXT(valeur)"),
    ("ISERROR", "ISERROR(valeur)"),
    ("ISNA", "ISNA(valeur)"),
    ("NA", "NA()"),
    ("LAMBDA", "LAMBDA([param, ...], calcul)"),
    ("LET", "LET(nom1, val1, ..., calcul)"),
    ("MAP", "MAP(tableau1, ..., lambda)"),
    ("REDUCE", "REDUCE(init, tableau, lambda)"),
    ("SCAN", "SCAN(init, tableau, lambda)"),
    ("FILTER", "FILTER(plage, condition, [si_vide])"),
    ("SORT", "SORT(plage, [col], [ordre])"),
    ("UNIQUE", "UNIQUE(plage)"),
    ("SEQUENCE", "SEQUENCE(lignes, [colonnes], [début], [pas])"),
    ("RANDARRAY", "RANDARRAY([lignes], [colonnes])"),
];

fn syntax_by_name() -> &'static HashMap<&'static str, &'static str> {
    static M: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    M.get_or_init(|| SYNTAXES.iter().copied().collect())
}

/// One catalogue entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogFn {
    pub name: String,
    pub cat: FnCat,
    pub syntax: String,
}

/// The catalogue entry of one function name (category from [`GROUPS`] or [`guess_cat`], syntax from
/// [`SYNTAXES`] or `NAME(…)`).
pub fn catalog_entry(name: &str) -> CatalogFn {
    CatalogFn {
        name: name.to_string(),
        cat: cat_by_name().get(name).copied().unwrap_or_else(|| guess_cat(name)),
        syntax: syntax_by_name().get(name).map(|s| (*s).to_string()).unwrap_or_else(|| format!("{name}(…)")),
    }
}

/// `/^[A-Z][A-Z0-9_.]*$/`.
pub fn is_catalog_name(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c.is_ascii_uppercase()) && cs.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// The collation rank of a character of a catalogue name under ICU's root collation (what `localeCompare`
/// uses): `_` < `.` < digits < letters, letters in alphabetical order.
fn collation_rank(c: char) -> u32 {
    match c {
        '_' => 0,
        '.' => 1,
        '0'..='9' => 2 + (c as u32 - '0' as u32),
        'A'..='Z' => 20 + (c as u32 - 'A' as u32),
        _ => 100 + c as u32,
    }
}

/// `a.localeCompare(b)` for catalogue names (upper-case ASCII letters, digits, `_` and `.`): the primary
/// weights compared left to right, a prefix sorting first.
pub fn locale_compare_names(a: &str, b: &str) -> Ordering {
    a.chars().map(collation_rank).cmp(b.chars().map(collation_rank))
}

/// `FUNCTION_CATALOG`: built once from the registered functions, sorted like the web's.
pub fn function_catalog() -> &'static [CatalogFn] {
    static C: OnceLock<Vec<CatalogFn>> = OnceLock::new();
    C.get_or_init(|| {
        let mut v: Vec<CatalogFn> = known_functions().into_iter().filter(|n| is_catalog_name(n)).map(catalog_entry).collect();
        v.sort_by(|a, b| locale_compare_names(&a.name, &b.name));
        v
    })
}

/// `CATALOG_BY_NAME.get(name)`.
pub fn catalog_by_name(name: &str) -> Option<&'static CatalogFn> {
    function_catalog().iter().find(|f| f.name == name)
}

/// `ALL_FN_NAMES.has(name)`.
pub fn is_catalog_fn(name: &str) -> bool {
    catalog_by_name(name).is_some()
}

// ── SpreadsheetApp.tsx: the autocomplete tables ───────────────────────────────

/// An entry of `FORMULA_FUNCTIONS`: a common function with its description key and texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormulaFn {
    pub name: &'static str,
    /// The i18n key of the description (`sheet_fn_sum`).
    pub desc_key: &'static str,
    pub syntax: &'static str,
    /// The French text of `desc_key` in `i18n.ts`.
    pub desc_fr: &'static str,
    /// The English text of `desc_key` in `i18n.ts`.
    pub desc_en: &'static str,
}

/// `FORMULA_FUNCTIONS`.
pub const FORMULA_FUNCTIONS: [FormulaFn; 42] = [
    FormulaFn { name: "SUM", desc_key: "sheet_fn_sum", syntax: "SUM(nombre1, ...)", desc_fr: "Somme", desc_en: "Sum" },
    FormulaFn { name: "AVERAGE", desc_key: "sheet_fn_average", syntax: "AVERAGE(nombre1, ...)", desc_fr: "Moyenne", desc_en: "Average" },
    FormulaFn { name: "COUNT", desc_key: "sheet_fn_count", syntax: "COUNT(val1, ...)", desc_fr: "Nombre de valeurs numériques", desc_en: "Count of numeric values" },
    FormulaFn { name: "COUNTA", desc_key: "sheet_fn_counta", syntax: "COUNTA(val1, ...)", desc_fr: "Cellules non vides", desc_en: "Non-empty cells" },
    FormulaFn { name: "MIN", desc_key: "sheet_fn_min", syntax: "MIN(nombre1, ...)", desc_fr: "Valeur minimale", desc_en: "Minimum value" },
    FormulaFn { name: "MAX", desc_key: "sheet_fn_max", syntax: "MAX(nombre1, ...)", desc_fr: "Valeur maximale", desc_en: "Maximum value" },
    FormulaFn { name: "IF", desc_key: "sheet_fn_if", syntax: "IF(condition, si_vrai, si_faux)", desc_fr: "Condition", desc_en: "Condition" },
    FormulaFn { name: "IFERROR", desc_key: "sheet_fn_iferror", syntax: "IFERROR(valeur, si_erreur)", desc_fr: "Valeur si erreur", desc_en: "Value if error" },
    FormulaFn { name: "AND", desc_key: "sheet_fn_and", syntax: "AND(val1, val2, ...)", desc_fr: "ET logique", desc_en: "Logical AND" },
    FormulaFn { name: "OR", desc_key: "sheet_fn_or", syntax: "OR(val1, val2, ...)", desc_fr: "OU logique", desc_en: "Logical OR" },
    FormulaFn { name: "NOT", desc_key: "sheet_fn_not", syntax: "NOT(valeur)", desc_fr: "NON logique", desc_en: "Logical NOT" },
    FormulaFn { name: "ROUND", desc_key: "sheet_fn_round", syntax: "ROUND(nombre, décimales)", desc_fr: "Arrondi", desc_en: "Round" },
    FormulaFn { name: "FLOOR", desc_key: "sheet_fn_floor", syntax: "FLOOR(nombre, [multiple])", desc_fr: "Arrondi vers le bas", desc_en: "Round down" },
    FormulaFn { name: "CEILING", desc_key: "sheet_fn_ceiling", syntax: "CEILING(nombre, [multiple])", desc_fr: "Arrondi vers le haut", desc_en: "Round up" },
    FormulaFn { name: "ABS", desc_key: "sheet_fn_abs", syntax: "ABS(nombre)", desc_fr: "Valeur absolue", desc_en: "Absolute value" },
    FormulaFn { name: "MOD", desc_key: "sheet_fn_mod", syntax: "MOD(nombre, diviseur)", desc_fr: "Reste de division", desc_en: "Remainder of division" },
    FormulaFn { name: "POWER", desc_key: "sheet_fn_power", syntax: "POWER(nombre, exposant)", desc_fr: "Puissance", desc_en: "Power" },
    FormulaFn { name: "SQRT", desc_key: "sheet_fn_sqrt", syntax: "SQRT(nombre)", desc_fr: "Racine carrée", desc_en: "Square root" },
    FormulaFn { name: "SUMIF", desc_key: "sheet_fn_sumif", syntax: "SUMIF(plage, critère, [plage_somme])", desc_fr: "Somme conditionnelle", desc_en: "Conditional sum" },
    FormulaFn { name: "COUNTIF", desc_key: "sheet_fn_countif", syntax: "COUNTIF(plage, critère)", desc_fr: "Compte avec condition", desc_en: "Count with condition" },
    FormulaFn { name: "AVERAGEIF", desc_key: "sheet_fn_averageif", syntax: "AVERAGEIF(plage, critère, [plage_moy])", desc_fr: "Moyenne conditionnelle", desc_en: "Conditional average" },
    FormulaFn { name: "VLOOKUP", desc_key: "sheet_fn_vlookup", syntax: "VLOOKUP(valeur, plage, colonne, [exact])", desc_fr: "Recherche verticale", desc_en: "Vertical lookup" },
    FormulaFn { name: "HLOOKUP", desc_key: "sheet_fn_hlookup", syntax: "HLOOKUP(valeur, plage, ligne, [exact])", desc_fr: "Recherche horizontale", desc_en: "Horizontal lookup" },
    FormulaFn { name: "INDEX", desc_key: "sheet_fn_index", syntax: "INDEX(plage, ligne, [colonne])", desc_fr: "Valeur à position", desc_en: "Value at position" },
    FormulaFn { name: "MATCH", desc_key: "sheet_fn_match", syntax: "MATCH(valeur, plage, [type])", desc_fr: "Position dans liste", desc_en: "Position in list" },
    FormulaFn { name: "LEN", desc_key: "sheet_fn_len", syntax: "LEN(texte)", desc_fr: "Longueur du texte", desc_en: "Text length" },
    FormulaFn { name: "LEFT", desc_key: "sheet_fn_left", syntax: "LEFT(texte, [nb_car])", desc_fr: "Gauche du texte", desc_en: "Left of text" },
    FormulaFn { name: "RIGHT", desc_key: "sheet_fn_right", syntax: "RIGHT(texte, [nb_car])", desc_fr: "Droite du texte", desc_en: "Right of text" },
    FormulaFn { name: "MID", desc_key: "sheet_fn_mid", syntax: "MID(texte, début, nb_car)", desc_fr: "Milieu du texte", desc_en: "Middle of text" },
    FormulaFn { name: "TRIM", desc_key: "sheet_fn_trim", syntax: "TRIM(texte)", desc_fr: "Supprimer espaces", desc_en: "Remove spaces" },
    FormulaFn { name: "UPPER", desc_key: "sheet_fn_upper", syntax: "UPPER(texte)", desc_fr: "Majuscules", desc_en: "Uppercase" },
    FormulaFn { name: "LOWER", desc_key: "sheet_fn_lower", syntax: "LOWER(texte)", desc_fr: "Minuscules", desc_en: "Lowercase" },
    FormulaFn { name: "CONCAT", desc_key: "sheet_fn_concat", syntax: "CONCAT(texte1, texte2, ...)", desc_fr: "Concaténer", desc_en: "Concatenate" },
    FormulaFn { name: "TEXT", desc_key: "sheet_fn_text", syntax: "TEXT(valeur, format)", desc_fr: "Formater en texte", desc_en: "Format as text" },
    FormulaFn { name: "TODAY", desc_key: "sheet_fn_today", syntax: "TODAY()", desc_fr: "Date d'aujourd'hui", desc_en: "Today's date" },
    FormulaFn { name: "NOW", desc_key: "sheet_fn_now", syntax: "NOW()", desc_fr: "Date et heure actuelles", desc_en: "Current date and time" },
    FormulaFn { name: "YEAR", desc_key: "sheet_fn_year", syntax: "YEAR(date)", desc_fr: "Année", desc_en: "Year" },
    FormulaFn { name: "MONTH", desc_key: "sheet_fn_month", syntax: "MONTH(date)", desc_fr: "Mois", desc_en: "Month" },
    FormulaFn { name: "DAY", desc_key: "sheet_fn_day", syntax: "DAY(date)", desc_fr: "Jour", desc_en: "Day" },
    FormulaFn { name: "ISBLANK", desc_key: "sheet_fn_isblank", syntax: "ISBLANK(valeur)", desc_fr: "Vide ?", desc_en: "Is blank?" },
    FormulaFn { name: "ISNUMBER", desc_key: "sheet_fn_isnumber", syntax: "ISNUMBER(valeur)", desc_fr: "Nombre ?", desc_en: "Is number?" },
    FormulaFn { name: "ISTEXT", desc_key: "sheet_fn_istext", syntax: "ISTEXT(valeur)", desc_fr: "Texte ?", desc_en: "Is text?" },
];

/// `FN_NAMES.has(name)`: names valid for the formula bar's misspelling underline.
pub fn is_formula_fn_name(name: &str) -> bool {
    FORMULA_FUNCTIONS.iter().any(|f| f.name == name)
}

/// `FN_CATEGORY[name]`: the basic colour categories of the autocomplete.
pub fn fn_category(name: &str) -> Option<FnCat> {
    Some(match name {
        "SUM" | "ROUND" | "FLOOR" | "CEILING" | "ABS" | "MOD" | "POWER" | "SQRT" => FnCat::Math,
        "AVERAGE" | "COUNT" | "COUNTA" | "MIN" | "MAX" | "SUMIF" | "COUNTIF" | "AVERAGEIF" => FnCat::Stat,
        "IF" | "IFERROR" | "AND" | "OR" | "NOT" => FnCat::Logical,
        "VLOOKUP" | "HLOOKUP" | "INDEX" | "MATCH" => FnCat::Lookup,
        "LEN" | "LEFT" | "RIGHT" | "MID" | "TRIM" | "UPPER" | "LOWER" | "CONCAT" | "TEXT" => FnCat::Text,
        "TODAY" | "NOW" | "YEAR" | "MONTH" | "DAY" => FnCat::Date,
        "ISBLANK" | "ISNUMBER" | "ISTEXT" => FnCat::Info,
        _ => return None,
    })
}

/// `CAT_COLOR` of `SpreadsheetApp.tsx` (only the seven basic categories; `None` for the others).
pub fn basic_cat_color(cat: FnCat) -> Option<&'static str> {
    Some(match cat {
        FnCat::Math => "#1a73e8",
        FnCat::Stat => "#12b5cb",
        FnCat::Logical => "#9334e6",
        FnCat::Lookup => "#188038",
        FnCat::Text => "#e8710a",
        FnCat::Date => "#d93025",
        FnCat::Info => "#795548",
        _ => return None,
    })
}

/// `fnColor(name)`: the colour of a function in the autocomplete (`math` blue when it has no category).
pub fn fn_color(name: &str) -> &'static str {
    basic_cat_color(fn_category(name).unwrap_or(FnCat::Math)).unwrap_or("#1a73e8")
}

/// An autocomplete suggestion: a catalogue entry with the description key of the common functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub entry: CatalogFn,
    pub desc_key: Option<&'static str>,
}

/// `SUGGESTIONS`: the whole catalogue enriched with the description keys of [`FORMULA_FUNCTIONS`].
pub fn suggestions() -> Vec<Suggestion> {
    function_catalog()
        .iter()
        .map(|f| Suggestion { entry: f.clone(), desc_key: FORMULA_FUNCTIONS.iter().find(|x| x.name == f.name).map(|x| x.desc_key) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(catalog_entry("SUM").cat, FnCat::Math);
        assert_eq!(catalog_entry("SUM").syntax, "SUM(nombre1, ...)");
        assert_eq!(catalog_entry("IMABS").cat, FnCat::Engineering);
        assert_eq!(catalog_entry("ISFOO").cat, FnCat::Info);
        assert_eq!(catalog_entry("NORM.DIST").cat, FnCat::Stat);
        assert_eq!(catalog_entry("ZZZ").cat, FnCat::Other);
        assert_eq!(catalog_entry("ZZZ").syntax, "ZZZ(…)");
    }

    #[test]
    fn collation() {
        assert_eq!(locale_compare_names("LOG", "LOG10"), Ordering::Less);
        assert_eq!(locale_compare_names("LOG10", "LOGNORM"), Ordering::Less);
        assert_eq!(locale_compare_names("BETA.DIST", "BETADIST"), Ordering::Less);
        assert_eq!(locale_compare_names("A_B", "A.B"), Ordering::Less);
    }

    #[test]
    fn tables() {
        assert_eq!(FORMULA_FUNCTIONS.len(), 42);
        assert_eq!(fn_color("SUM"), "#1a73e8");
        assert_eq!(fn_color("VLOOKUP"), "#188038");
        assert_eq!(fn_color("NOPE"), "#1a73e8");
        assert!(FORMULA_FUNCTIONS.iter().all(|f| fn_category(f.name).is_some()));
        assert!(is_formula_fn_name("SUM") && !is_formula_fn_name("PRODUCT"));
        for w in function_catalog().windows(2) {
            assert_ne!(locale_compare_names(&w[0].name, &w[1].name), Ordering::Greater);
        }
    }
}
