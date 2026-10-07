//! KaTeX's `Parser.ts`, its function handlers (`functions/*.ts`) and environments (`environments/array.ts`):
//! LaTeX to parse nodes, with KaTeX's rules (macros expanded as tokens are fetched, `^`/`_`/primes,
//! `\limits`, infix `\over`, `\left…\right`, text mode, environments).

use crate::data::{self, Group, Measurement, Mode};
use crate::lexer::{Gullet, MacroDef, Token};
use crate::ParseError;

/// The four math styles a `styling` node selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleStr {
    Display,
    Text,
    Script,
    ScriptScript,
}

/// A column of an array (`AlignSpec`).
#[derive(Clone, Debug, PartialEq)]
pub enum AlignSpec {
    Separator(char),
    Align { align: char, pregap: Option<f64>, postgap: Option<f64> },
}

/// An array (`ParseNode<"array">`).
#[derive(Clone, Debug, PartialEq)]
pub struct ArrayNode {
    pub mode: Mode,
    pub add_jot: bool,
    pub arraystretch: f64,
    pub body: Vec<Vec<P>>,
    pub cols: Vec<AlignSpec>,
    pub row_gaps: Vec<Option<Measurement>>,
    pub hskip_before_and_after: bool,
    pub hlines_before_row: Vec<Vec<bool>>,
    /// `"align"`, `"alignat"`, `"gather"`, `"small"`, `"CD"`.
    pub col_separation_type: Option<&'static str>,
}

/// A parse node (`AnyParseNode`), restricted to what the builders render.
#[derive(Clone, Debug, PartialEq)]
pub enum P {
    OrdGroup { mode: Mode, body: Vec<P> },
    MathOrd { mode: Mode, text: String },
    TextOrd { mode: Mode, text: String },
    Atom { mode: Mode, family: Group, text: String },
    Spacing { mode: Mode, text: String },
    AccentToken { mode: Mode, text: String },
    OpToken { mode: Mode, text: String },
    SupSub { mode: Mode, base: Option<Box<P>>, sup: Option<Box<P>>, sub: Option<Box<P>> },
    GenFrac { mode: Mode, numer: Box<P>, denom: Box<P>, has_bar: bool, left: Option<String>, right: Option<String>, continued: bool, bar_size: Option<Measurement> },
    Infix { replace_with: String },
    Styling { mode: Mode, style: StyleStr, body: Vec<P>, reset_font: bool },
    Sizing { mode: Mode, size: u8, body: Vec<P> },
    Sqrt { mode: Mode, body: Box<P>, index: Option<Box<P>> },
    Op { mode: Mode, limits: bool, always_handle_supsub: bool, suppress_base_shift: bool, symbol: bool, name: Option<String>, body: Option<Vec<P>> },
    OperatorName { mode: Mode, body: Vec<P>, always_handle_supsub: bool, limits: bool },
    LeftRight { mode: Mode, body: Vec<P>, left: String, right: String, right_color: Option<String> },
    LeftRightRight { delim: String, color: Option<String> },
    Middle { mode: Mode, delim: String },
    DelimSizing { mode: Mode, size: u8, mclass: &'static str, delim: String },
    Accent { mode: Mode, label: String, is_stretchy: bool, is_shifty: bool, base: Box<P> },
    Text { mode: Mode, body: Vec<P>, font: Option<String> },
    Font { mode: Mode, font: String, body: Box<P> },
    Color { mode: Mode, color: String, body: Vec<P> },
    ColorToken { color: String },
    Size { value: Measurement, is_blank: bool },
    Raw { string: String },
    Kern { mode: Mode, dim: Measurement },
    Overline { mode: Mode, body: Box<P> },
    Underline { mode: Mode, body: Box<P> },
    MClass { mode: Mode, mclass: String, body: Vec<P>, is_character_box: bool },
    Lap { mode: Mode, alignment: String, body: Box<P> },
    Phantom { mode: Mode, body: Vec<P> },
    Smash { mode: Mode, body: Box<P>, height: bool, depth: bool },
    HorizBrace { mode: Mode, label: String, is_over: bool, base: Box<P> },
    VPhantom { mode: Mode, body: Box<P> },
    Rule { mode: Mode, shift: Option<Measurement>, width: Measurement, height: Measurement },
    Enclose { mode: Mode, label: String, body: Box<P>, background: Option<String>, border: Option<String> },
    Array(Box<ArrayNode>),
    /// `html@mathml`: only the HTML half is kept.
    HtmlMathml { mode: Mode, body: Vec<P> },
    Environment { name: String },
    Cr,
    /// Produces nothing (`\relax`, a consumed `\limits`).
    Internal,
}

impl P {
    pub fn mode(&self) -> Mode {
        match self {
            P::OrdGroup { mode, .. }
            | P::MathOrd { mode, .. }
            | P::TextOrd { mode, .. }
            | P::Atom { mode, .. }
            | P::Spacing { mode, .. }
            | P::AccentToken { mode, .. }
            | P::OpToken { mode, .. }
            | P::SupSub { mode, .. }
            | P::GenFrac { mode, .. }
            | P::Styling { mode, .. }
            | P::Sizing { mode, .. }
            | P::Sqrt { mode, .. }
            | P::Op { mode, .. }
            | P::OperatorName { mode, .. }
            | P::LeftRight { mode, .. }
            | P::Middle { mode, .. }
            | P::DelimSizing { mode, .. }
            | P::Accent { mode, .. }
            | P::Text { mode, .. }
            | P::Font { mode, .. }
            | P::Color { mode, .. }
            | P::Kern { mode, .. }
            | P::Overline { mode, .. }
            | P::Underline { mode, .. }
            | P::MClass { mode, .. }
            | P::Lap { mode, .. }
            | P::Phantom { mode, .. }
            | P::Smash { mode, .. }
            | P::HorizBrace { mode, .. }
            | P::VPhantom { mode, .. }
            | P::Rule { mode, .. }
            | P::Enclose { mode, .. }
            | P::HtmlMathml { mode, .. } => *mode,
            P::Array(a) => a.mode,
            _ => Mode::Math,
        }
    }

    /// The symbol text of a symbol node (`checkSymbolNodeType`).
    pub fn symbol_text(&self) -> Option<&str> {
        match self {
            P::MathOrd { text, .. } | P::TextOrd { text, .. } | P::Atom { text, .. } | P::Spacing { text, .. } | P::AccentToken { text, .. } | P::OpToken { text, .. } => Some(text),
            _ => None,
        }
    }
}

/// `ordargument`.
pub fn ordargument(p: P) -> Vec<P> {
    match p {
        P::OrdGroup { body, .. } => body,
        other => vec![other],
    }
}

/// `normalizeArgument`.
pub fn normalize_argument(p: P) -> P {
    match p {
        P::OrdGroup { mut body, mode } => {
            if body.len() == 1 {
                body.remove(0)
            } else {
                P::OrdGroup { body, mode }
            }
        }
        other => other,
    }
}

/// `getBaseElem`.
pub fn base_elem(p: &P) -> Option<&P> {
    match p {
        P::OrdGroup { body, .. } => {
            if body.len() == 1 {
                base_elem(&body[0])
            } else {
                Some(p)
            }
        }
        P::Color { body, .. } => {
            if body.len() == 1 {
                base_elem(&body[0])
            } else {
                Some(p)
            }
        }
        P::Font { body, .. } => base_elem(body),
        _ => Some(p),
    }
}

/// `isCharacterBox`.
pub fn is_character_box(p: &P) -> bool {
    matches!(base_elem(p), Some(P::MathOrd { .. } | P::TextOrd { .. } | P::Atom { .. }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArgType {
    Color,
    Size,
    Text,
    #[allow(dead_code)]
    Math,
    Primitive,
    Original,
    Raw,
    HBox,
}

#[derive(Clone, Copy, Debug)]
struct FuncSpec {
    num_args: usize,
    num_optional: usize,
    arg_types: &'static [ArgType],
    allowed_in_text: bool,
    allowed_in_math: bool,
    allowed_in_argument: bool,
    infix: bool,
    primitive: bool,
}

const fn spec(num_args: usize) -> FuncSpec {
    FuncSpec { num_args, num_optional: 0, arg_types: &[], allowed_in_text: false, allowed_in_math: true, allowed_in_argument: false, infix: false, primitive: false }
}

const OPS_SYMBOL_LIMITS: &[&str] = &["\\coprod", "\\bigvee", "\\bigwedge", "\\biguplus", "\\bigcap", "\\bigcup", "\\intop", "\\prod", "\\sum", "\\bigotimes", "\\bigoplus", "\\bigodot", "\\bigsqcup", "\\smallint", "\u{220F}", "\u{2210}", "\u{2211}", "\u{22c0}", "\u{22c1}", "\u{22c2}", "\u{22c3}", "\u{2a00}", "\u{2a01}", "\u{2a02}", "\u{2a04}", "\u{2a06}"];
const OPS_NAMED: &[&str] = &["\\arcsin", "\\arccos", "\\arctan", "\\arctg", "\\arcctg", "\\arg", "\\ch", "\\cos", "\\cosec", "\\cosh", "\\cot", "\\cotg", "\\coth", "\\csc", "\\ctg", "\\cth", "\\deg", "\\dim", "\\exp", "\\hom", "\\ker", "\\lg", "\\ln", "\\log", "\\sec", "\\sin", "\\sinh", "\\sh", "\\tan", "\\tanh", "\\tg", "\\th"];
const OPS_NAMED_LIMITS: &[&str] = &["\\det", "\\gcd", "\\inf", "\\lim", "\\max", "\\min", "\\Pr", "\\sup"];
const OPS_INTEGRALS: &[&str] = &["\\int", "\\iint", "\\iiint", "\\oint", "\\oiint", "\\oiiint", "\u{222b}", "\u{222c}", "\u{222d}", "\u{222e}", "\u{222f}", "\u{2230}"];
const MATH_ACCENTS: &[&str] = &["\\acute", "\\grave", "\\ddot", "\\tilde", "\\bar", "\\breve", "\\check", "\\hat", "\\vec", "\\dot", "\\mathring", "\\widecheck", "\\widehat", "\\widetilde", "\\overrightarrow", "\\overleftarrow", "\\Overrightarrow", "\\overleftrightarrow", "\\overgroup", "\\overlinesegment", "\\overleftharpoon", "\\overrightharpoon"];
const TEXT_ACCENTS: &[&str] = &["\\'", "\\`", "\\^", "\\~", "\\=", "\\u", "\\.", "\\\"", "\\c", "\\r", "\\H", "\\v", "\\textcircled"];
const SIZE_FUNCS: &[&str] = &["\\tiny", "\\sixptsize", "\\scriptsize", "\\footnotesize", "\\small", "\\normalsize", "\\large", "\\Large", "\\LARGE", "\\huge", "\\Huge"];
const DELIM_SIZES: &[(&str, &str, u8)] = &[
    ("\\bigl", "mopen", 1),
    ("\\Bigl", "mopen", 2),
    ("\\biggl", "mopen", 3),
    ("\\Biggl", "mopen", 4),
    ("\\bigr", "mclose", 1),
    ("\\Bigr", "mclose", 2),
    ("\\biggr", "mclose", 3),
    ("\\Biggr", "mclose", 4),
    ("\\bigm", "mrel", 1),
    ("\\Bigm", "mrel", 2),
    ("\\biggm", "mrel", 3),
    ("\\Biggm", "mrel", 4),
    ("\\big", "mord", 1),
    ("\\Big", "mord", 2),
    ("\\bigg", "mord", 3),
    ("\\Bigg", "mord", 4),
];
const DELIMITERS: &[&str] = &[
    "(", "\\lparen", ")", "\\rparen", "[", "\\lbrack", "]", "\\rbrack", "\\{", "\\lbrace", "\\}", "\\rbrace", "\\lfloor", "\\rfloor", "\u{230a}", "\u{230b}", "\\lceil", "\\rceil", "\u{2308}", "\u{2309}", "<", ">", "\\langle", "\u{27e8}", "\\rangle", "\u{27e9}", "\\lt", "\\gt", "\\lvert", "\\rvert", "\\lVert", "\\rVert", "\\lgroup", "\\rgroup", "\u{27ee}", "\u{27ef}", "\\lmoustache", "\\rmoustache", "\u{23b0}", "\u{23b1}", "/", "\\backslash", "|", "\\vert", "\\|", "\\Vert", "\\uparrow", "\\Uparrow", "\\downarrow", "\\Downarrow", "\\updownarrow", "\\Updownarrow", ".",
];

fn func_spec(name: &str) -> Option<FuncSpec> {
    use ArgType::*;
    let s = match name {
        "\\cfrac" | "\\dfrac" | "\\frac" | "\\tfrac" | "\\dbinom" | "\\binom" | "\\tbinom" | "\\\\atopfrac" | "\\\\bracefrac" | "\\\\brackfrac" => FuncSpec { allowed_in_argument: true, ..spec(2) },
        "\\over" | "\\choose" | "\\atop" | "\\brace" | "\\brack" => FuncSpec { infix: true, ..spec(0) },
        "\\sqrt" => FuncSpec { num_optional: 1, ..spec(1) },
        "\\mathop" => FuncSpec { primitive: true, ..spec(1) },
        "\\operatorname@" | "\\operatornamewithlimits" => spec(1),
        "\\left" | "\\right" | "\\middle" => FuncSpec { primitive: true, ..spec(1) },
        "\\text" | "\\textrm" | "\\textsf" | "\\texttt" | "\\textnormal" | "\\textbf" | "\\textmd" | "\\textit" | "\\textup" | "\\emph" => FuncSpec { arg_types: &[Text], allowed_in_argument: true, allowed_in_text: true, ..spec(1) },
        "\\mathrm" | "\\mathit" | "\\mathbf" | "\\mathnormal" | "\\mathsfit" | "\\mathbb" | "\\mathcal" | "\\mathfrak" | "\\mathscr" | "\\mathsf" | "\\mathtt" | "\\Bbb" | "\\bold" | "\\frak" => FuncSpec { allowed_in_argument: true, ..spec(1) },
        "\\boldsymbol" | "\\bm" => spec(1),
        "\\rm" | "\\sf" | "\\tt" | "\\bf" | "\\it" | "\\cal" => FuncSpec { allowed_in_text: true, ..spec(0) },
        "\\textcolor" => FuncSpec { arg_types: &[Color, Original], allowed_in_text: true, ..spec(2) },
        "\\color" => FuncSpec { arg_types: &[Color], allowed_in_text: true, ..spec(1) },
        "\\displaystyle" | "\\textstyle" | "\\scriptstyle" | "\\scriptscriptstyle" => FuncSpec { allowed_in_text: true, primitive: true, ..spec(0) },
        "\\kern" | "\\mkern" | "\\hskip" | "\\mskip" => FuncSpec { arg_types: &[Size], primitive: true, allowed_in_text: true, ..spec(1) },
        "\\overline" => spec(1),
        "\\underline" => FuncSpec { allowed_in_text: true, ..spec(1) },
        "\\mathord" | "\\mathbin" | "\\mathrel" | "\\mathopen" | "\\mathclose" | "\\mathpunct" | "\\mathinner" => FuncSpec { primitive: true, ..spec(1) },
        "\\@binrel" | "\\stackrel" | "\\overset" | "\\underset" => spec(2),
        "\\mathllap" | "\\mathrlap" | "\\mathclap" => FuncSpec { allowed_in_text: true, ..spec(1) },
        "\\html@mathml" => FuncSpec { allowed_in_text: true, ..spec(2) },
        "\\relax" => FuncSpec { allowed_in_text: true, ..spec(0) },
        "\\@char" => FuncSpec { arg_types: &[Raw], allowed_in_text: true, ..spec(1) },
        "\\begin" | "\\end" => FuncSpec { arg_types: &[Text], ..spec(1) },
        "\\\\" | "\\newline" => FuncSpec { allowed_in_text: true, ..spec(0) },
        "\\phantom" => FuncSpec { allowed_in_text: true, ..spec(1) },
        "\\vphantom" => FuncSpec { allowed_in_text: true, ..spec(1) },
        "\\smash" => FuncSpec { num_optional: 1, allowed_in_text: true, ..spec(1) },
        "\\overbrace" | "\\underbrace" | "\\overbracket" | "\\underbracket" => spec(1),
        "\\rule" => FuncSpec { num_optional: 1, arg_types: &[Size, Size, Size], allowed_in_text: true, ..spec(2) },
        "\\boxed" | "\\fbox" => FuncSpec { arg_types: &[HBox], allowed_in_text: true, ..spec(1) },
        "\\colorbox" => FuncSpec { arg_types: &[Color, HBox], allowed_in_text: true, ..spec(2) },
        "\\cancel" | "\\bcancel" | "\\xcancel" | "\\sout" => spec(1),
        _ if OPS_SYMBOL_LIMITS.contains(&name) || OPS_NAMED.contains(&name) || OPS_NAMED_LIMITS.contains(&name) => spec(0),
        _ if OPS_INTEGRALS.contains(&name) => FuncSpec { allowed_in_argument: true, ..spec(0) },
        _ if MATH_ACCENTS.contains(&name) => spec(1),
        _ if TEXT_ACCENTS.contains(&name) => FuncSpec { arg_types: &[Primitive], allowed_in_text: true, ..spec(1) },
        _ if SIZE_FUNCS.contains(&name) => FuncSpec { allowed_in_text: true, ..spec(0) },
        _ if DELIM_SIZES.iter().any(|d| d.0 == name) => FuncSpec { arg_types: &[Primitive], ..spec(1) },
        _ => return None,
    };
    Some(s)
}

/// Whether `name` is one of the functions this port implements.
pub fn is_function(name: &str) -> bool {
    func_spec(name).is_some()
}

/// Whether the function is a TeX primitive (not expandable).
pub fn is_primitive(name: &str) -> bool {
    func_spec(name).is_some_and(|s| s.primitive)
}

/// The parser (`Parser`).
pub struct Parser {
    pub mode: Mode,
    pub gullet: Gullet,
    next_token: Option<Token>,
    leftright_depth: usize,
}

const END_OF_EXPRESSION: &[&str] = &["}", "\\endgroup", "\\end", "\\right", "&"];

impl Parser {
    pub fn new(input: &str) -> Parser {
        Parser { mode: Mode::Math, gullet: Gullet::new(input, Mode::Math), next_token: None, leftright_depth: 0 }
    }

    fn fetch(&mut self) -> Result<Token, ParseError> {
        if self.next_token.is_none() {
            self.next_token = Some(self.gullet.expand_next_token()?);
        }
        Ok(self.next_token.clone().unwrap_or_else(|| Token::new("EOF")))
    }

    fn consume(&mut self) {
        self.next_token = None;
    }

    fn expect(&mut self, text: &str, consume: bool) -> Result<(), ParseError> {
        let t = self.fetch()?;
        if t.text != text {
            return Err(ParseError(format!("Expected '{text}', got '{}'", t.text)));
        }
        if consume {
            self.consume();
        }
        Ok(())
    }

    fn switch_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.gullet.mode = mode;
    }

    /// `parse()`.
    pub fn parse(&mut self) -> Result<Vec<P>, ParseError> {
        self.gullet.begin_group();
        let body = self.parse_expression(false, None)?;
        self.expect("EOF", true)?;
        self.gullet.end_group();
        Ok(body)
    }

    fn consume_spaces(&mut self) -> Result<(), ParseError> {
        while self.fetch()?.text == " " {
            self.consume();
        }
        Ok(())
    }

    pub fn parse_expression(&mut self, break_on_infix: bool, break_on: Option<&str>) -> Result<Vec<P>, ParseError> {
        let mut body = Vec::new();
        loop {
            if self.mode == Mode::Math {
                self.consume_spaces()?;
            }
            let lex = self.fetch()?;
            if END_OF_EXPRESSION.contains(&lex.text.as_str()) {
                break;
            }
            if break_on.is_some_and(|b| b == lex.text) {
                break;
            }
            if break_on_infix && func_spec(&lex.text).is_some_and(|s| s.infix) {
                break;
            }
            let Some(atom) = self.parse_atom(break_on)? else { break };
            if matches!(atom, P::Internal) {
                continue;
            }
            body.push(atom);
        }
        if self.mode == Mode::Text {
            form_ligatures(&mut body);
        }
        self.handle_infix_nodes(body)
    }

    fn handle_infix_nodes(&mut self, body: Vec<P>) -> Result<Vec<P>, ParseError> {
        let mut over = None;
        for (i, n) in body.iter().enumerate() {
            if let P::Infix { replace_with } = n {
                if over.is_some() {
                    return Err(ParseError("only one infix operator per group".into()));
                }
                over = Some((i, replace_with.clone()));
            }
        }
        let Some((idx, func)) = over else { return Ok(body) };
        let mode = self.mode;
        let mut body = body;
        let denom_body = body.split_off(idx + 1);
        body.pop();
        let numer_body = body;
        let wrap = |mut b: Vec<P>| -> P {
            if b.len() == 1 && matches!(b[0], P::OrdGroup { .. }) {
                b.remove(0)
            } else {
                P::OrdGroup { mode, body: b }
            }
        };
        let node = self.call_function(&func, vec![wrap(numer_body), wrap(denom_body)], vec![], None)?;
        Ok(vec![node])
    }

    fn handle_sup_subscript(&mut self) -> Result<P, ParseError> {
        let symbol = self.fetch()?.text;
        self.consume();
        self.consume_spaces()?;
        loop {
            match self.parse_group(false, None)? {
                Some(P::Internal) => continue,
                Some(g) => return Ok(g),
                None => return Err(ParseError(format!("Expected group after '{symbol}'"))),
            }
        }
    }

    fn format_unsupported_cmd(&self, text: &str) -> P {
        let body = text.chars().map(|c| P::TextOrd { mode: Mode::Text, text: c.to_string() }).collect();
        P::Color { mode: self.mode, color: crate::ERROR_COLOR.into(), body: vec![P::Text { mode: self.mode, body, font: None }] }
    }

    fn parse_atom(&mut self, break_on: Option<&str>) -> Result<Option<P>, ParseError> {
        let Some(mut base) = self.parse_group(true, break_on)? else {
            if self.mode == Mode::Text {
                return Ok(None);
            }
            // A sup/sub with no base.
            return self.parse_scripts(None);
        };
        if matches!(base, P::Internal) || self.mode == Mode::Text {
            return Ok(Some(base));
        }
        // \limits / \nolimits are handled inside parse_scripts on the base.
        let _ = &mut base;
        self.parse_scripts(Some(base))
    }

    fn parse_scripts(&mut self, mut base: Option<P>) -> Result<Option<P>, ParseError> {
        let mut sup: Option<P> = None;
        let mut sub: Option<P> = None;
        loop {
            self.consume_spaces()?;
            let lex = self.fetch()?;
            match lex.text.as_str() {
                "\\limits" | "\\nolimits" => {
                    let limits = lex.text == "\\limits";
                    match &mut base {
                        Some(P::Op { limits: l, always_handle_supsub, .. }) => {
                            *l = limits;
                            *always_handle_supsub = true;
                        }
                        Some(P::OperatorName { limits: l, always_handle_supsub, .. }) => {
                            if *always_handle_supsub {
                                *l = limits;
                            }
                        }
                        _ => return Err(ParseError("Limit controls must follow a math operator".into())),
                    }
                    self.consume();
                }
                "^" => {
                    if sup.is_some() {
                        return Err(ParseError("Double superscript".into()));
                    }
                    sup = Some(self.handle_sup_subscript()?);
                }
                "_" => {
                    if sub.is_some() {
                        return Err(ParseError("Double subscript".into()));
                    }
                    sub = Some(self.handle_sup_subscript()?);
                }
                "'" => {
                    if sup.is_some() {
                        return Err(ParseError("Double superscript".into()));
                    }
                    let prime = P::TextOrd { mode: self.mode, text: "\\prime".into() };
                    let mut primes = vec![prime.clone()];
                    self.consume();
                    while self.fetch()?.text == "'" {
                        primes.push(prime.clone());
                        self.consume();
                    }
                    if self.fetch()?.text == "^" {
                        primes.push(self.handle_sup_subscript()?);
                    }
                    sup = Some(P::OrdGroup { mode: self.mode, body: primes });
                }
                _ => break,
            }
        }
        if base.is_none() && sup.is_none() && sub.is_none() {
            return Ok(None);
        }
        if sup.is_some() || sub.is_some() {
            Ok(Some(P::SupSub { mode: self.mode, base: base.map(Box::new), sup: sup.map(Box::new), sub: sub.map(Box::new) }))
        } else {
            Ok(base)
        }
    }

    fn parse_function(&mut self, break_on: Option<&str>, as_atom: bool) -> Result<Option<P>, ParseError> {
        let token = self.fetch()?;
        let func = token.text.clone();
        let Some(spec) = func_spec(&func) else { return Ok(None) };
        self.consume();
        if !as_atom && !spec.allowed_in_argument {
            return Err(ParseError(format!("Got function '{func}' with no arguments as argument")));
        } else if self.mode == Mode::Text && !spec.allowed_in_text {
            return Err(ParseError(format!("Can't use function '{func}' in text mode")));
        } else if self.mode == Mode::Math && !spec.allowed_in_math {
            return Err(ParseError(format!("Can't use function '{func}' in math mode")));
        }
        let (args, opt) = self.parse_arguments(&func, &spec)?;
        Ok(Some(self.call_function(&func, args, opt, break_on)?))
    }

    fn parse_arguments(&mut self, func: &str, spec: &FuncSpec) -> Result<(Vec<P>, Vec<Option<P>>), ParseError> {
        let total = spec.num_args + spec.num_optional;
        let mut args = Vec::new();
        let mut opt = Vec::new();
        for i in 0..total {
            let mut arg_type = spec.arg_types.get(i).copied();
            let is_optional = i < spec.num_optional;
            if (spec.primitive && arg_type.is_none()) || (func == "\\sqrt" && i == 1 && opt.first().is_some_and(|o: &Option<P>| o.is_none())) {
                arg_type = Some(ArgType::Primitive);
            }
            let arg = self.parse_group_of_type(arg_type, is_optional)?;
            if is_optional {
                opt.push(arg);
            } else if let Some(a) = arg {
                args.push(a);
            } else {
                return Err(ParseError("Null argument".into()));
            }
        }
        Ok((args, opt))
    }

    fn parse_group_of_type(&mut self, t: Option<ArgType>, optional: bool) -> Result<Option<P>, ParseError> {
        match t {
            Some(ArgType::Color) => self.parse_color_group(optional),
            Some(ArgType::Size) => self.parse_size_group(optional),
            Some(ArgType::Math) => self.parse_argument_group(optional, Some(Mode::Math)),
            Some(ArgType::Text) => self.parse_argument_group(optional, Some(Mode::Text)),
            Some(ArgType::HBox) => {
                let g = self.parse_argument_group(optional, Some(Mode::Text))?;
                Ok(g.map(|g| P::Styling { mode: g.mode(), style: StyleStr::Text, body: vec![g], reset_font: true }))
            }
            Some(ArgType::Raw) => Ok(self.parse_string_group(optional)?.map(|s| P::Raw { string: s })),
            Some(ArgType::Primitive) => {
                if optional {
                    return Err(ParseError("A primitive argument cannot be optional".into()));
                }
                match self.parse_group(false, None)? {
                    Some(g) => Ok(Some(g)),
                    None => Err(ParseError("Expected group".into())),
                }
            }
            Some(ArgType::Original) | None => self.parse_argument_group(optional, None),
        }
    }

    fn parse_string_group(&mut self, optional: bool) -> Result<Option<String>, ParseError> {
        if self.gullet.scan_argument(optional)?.is_none() {
            return Ok(None);
        }
        let mut s = String::new();
        loop {
            let t = self.fetch()?;
            if t.text == "EOF" {
                break;
            }
            s.push_str(&t.text);
            self.consume();
        }
        self.consume();
        Ok(Some(s))
    }

    fn parse_color_group(&mut self, optional: bool) -> Result<Option<P>, ParseError> {
        let Some(s) = self.parse_string_group(optional)? else { return Ok(None) };
        let ok = {
            let hex = |x: &str| x.chars().all(|c| c.is_ascii_hexdigit());
            if let Some(h) = s.strip_prefix('#') {
                matches!(h.len(), 3 | 4 | 6 | 8) && hex(h)
            } else {
                (s.len() == 6 && hex(&s)) || (!s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()))
            }
        };
        if !ok {
            return Err(ParseError(format!("Invalid color: '{s}'")));
        }
        let color = if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) { format!("#{s}") } else { s };
        Ok(Some(P::ColorToken { color }))
    }

    fn parse_size_group(&mut self, optional: bool) -> Result<Option<P>, ParseError> {
        self.gullet.consume_spaces()?;
        let text = if !optional && self.gullet.future()?.text != "{" {
            // parseRegexGroup(/^[-+]? *(?:$|\d+|\d+\.\d*|\.\d*) *[a-z]{0,2} *$/)
            let mut s = String::new();
            loop {
                let t = self.fetch()?;
                if t.text == "EOF" {
                    break;
                }
                let cand = format!("{s}{}", t.text);
                if !size_prefix_ok(&cand) {
                    break;
                }
                s = cand;
                self.consume();
            }
            if s.is_empty() {
                return Err(ParseError("Invalid size".into()));
            }
            Some(s)
        } else {
            self.parse_string_group(optional)?
        };
        let Some(mut text) = text else { return Ok(None) };
        let mut is_blank = false;
        if !optional && text.is_empty() {
            text = "0pt".into();
            is_blank = true;
        }
        let m = parse_size(&text).ok_or_else(|| ParseError(format!("Invalid size: '{text}'")))?;
        Ok(Some(P::Size { value: m, is_blank }))
    }

    fn parse_argument_group(&mut self, optional: bool, mode: Option<Mode>) -> Result<Option<P>, ParseError> {
        if self.gullet.scan_argument(optional)?.is_none() {
            return Ok(None);
        }
        let outer = self.mode;
        if let Some(m) = mode {
            self.switch_mode(m);
        }
        self.gullet.begin_group();
        let expression = self.parse_expression(false, Some("EOF"))?;
        self.expect("EOF", true)?;
        self.gullet.end_group();
        let result = P::OrdGroup { mode: self.mode, body: expression };
        if mode.is_some() {
            self.switch_mode(outer);
        }
        Ok(Some(result))
    }

    /// `parseGroup`: a `{…}` group, a function or a symbol.
    fn parse_group(&mut self, as_atom: bool, break_on: Option<&str>) -> Result<Option<P>, ParseError> {
        let first = self.fetch()?;
        let text = first.text.clone();
        if text == "{" || text == "\\begingroup" {
            self.consume();
            let end = if text == "{" { "}" } else { "\\endgroup" };
            self.gullet.begin_group();
            let expression = self.parse_expression(false, Some(end))?;
            self.expect(end, true)?;
            self.gullet.end_group();
            return Ok(Some(P::OrdGroup { mode: self.mode, body: expression }));
        }
        let mut result = self.parse_function(break_on, as_atom)?;
        if result.is_none() {
            result = self.parse_symbol()?;
        }
        if result.is_none() && text.starts_with('\\') && !matches!(text.as_str(), "^" | "_" | "\\limits" | "\\nolimits") {
            // throwOnError: false → the command in the error colour.
            result = Some(self.format_unsupported_cmd(&text));
            self.consume();
        }
        Ok(result)
    }

    fn parse_symbol(&mut self) -> Result<Option<P>, ParseError> {
        let nucleus = self.fetch()?;
        let mut text = nucleus.text.clone();
        // Unicode accented letters (unicodeSymbols): decomposed into base + combining marks.
        if let Some(c) = text.chars().next() {
            if c as u32 >= 0x80 && data::symbol(self.mode, &c.to_string()).is_none() {
                if let Some(dec) = decompose(c) {
                    text = format!("{dec}{}", &text[c.len_utf8()..]);
                }
            }
        }
        // Trailing combining marks become accents.
        let mut marks: Vec<char> = Vec::new();
        while let Some(last) = text.chars().last() {
            if is_combining(last) && text.chars().count() > 1 {
                marks.insert(0, last);
                text.pop();
            } else {
                break;
            }
        }
        if !marks.is_empty() {
            if text == "i" {
                text = "\u{0131}".into();
            } else if text == "j" {
                text = "\u{0237}".into();
            }
        }
        let mut symbol = if let Some(info) = data::symbol(self.mode, &text) {
            let mode = self.mode;
            if info.group.is_atom() {
                P::Atom { mode, family: info.group, text }
            } else {
                match info.group {
                    Group::MathOrd => P::MathOrd { mode, text },
                    Group::TextOrd => P::TextOrd { mode, text },
                    Group::Spacing => P::Spacing { mode, text },
                    Group::AccentToken => P::AccentToken { mode, text },
                    Group::OpToken => P::OpToken { mode, text },
                    _ => P::TextOrd { mode, text },
                }
            }
        } else if text.chars().next().is_some_and(|c| c as u32 >= 0x80) {
            P::TextOrd { mode: Mode::Text, text }
        } else {
            return Ok(None);
        };
        self.consume();
        for m in marks {
            let Some(cmd) = combining_accent(m, self.mode) else {
                return Err(ParseError(format!("Unknown accent ' {m}'")));
            };
            symbol = P::Accent { mode: self.mode, label: cmd.to_string(), is_stretchy: false, is_shifty: true, base: Box::new(symbol) };
        }
        Ok(Some(symbol))
    }

    fn check_delimiter(&self, p: &P, func: &str) -> Result<String, ParseError> {
        match p.symbol_text() {
            Some(t) if DELIMITERS.contains(&t) => Ok(t.to_string()),
            Some(t) => Err(ParseError(format!("Invalid delimiter '{t}' after '{func}'"))),
            None => Err(ParseError("Invalid delimiter type".into())),
        }
    }

    fn call_function(&mut self, name: &str, mut args: Vec<P>, mut opt: Vec<Option<P>>, break_on: Option<&str>) -> Result<P, ParseError> {
        let mode = self.mode;
        let take = |args: &mut Vec<P>| -> P { if args.is_empty() { P::OrdGroup { mode, body: vec![] } } else { args.remove(0) } };
        match name {
            "\\cfrac" | "\\dfrac" | "\\frac" | "\\tfrac" | "\\dbinom" | "\\binom" | "\\tbinom" | "\\\\atopfrac" | "\\\\bracefrac" | "\\\\brackfrac" => {
                let numer = take(&mut args);
                let denom = take(&mut args);
                let (has_bar, left, right) = match name {
                    "\\cfrac" | "\\dfrac" | "\\frac" | "\\tfrac" => (true, None, None),
                    "\\\\atopfrac" => (false, None, None),
                    "\\dbinom" | "\\binom" | "\\tbinom" => (false, Some("(".to_string()), Some(")".to_string())),
                    "\\\\bracefrac" => (false, Some("\\{".to_string()), Some("\\}".to_string())),
                    _ => (false, Some("[".to_string()), Some("]".to_string())),
                };
                let continued = name == "\\cfrac";
                let frac = P::GenFrac { mode, numer: Box::new(numer), denom: Box::new(denom), has_bar, left, right, continued, bar_size: None };
                let style = if continued || name.starts_with("\\d") {
                    Some(StyleStr::Display)
                } else if name.starts_with("\\t") {
                    Some(StyleStr::Text)
                } else {
                    None
                };
                Ok(match style {
                    Some(s) => P::Styling { mode, style: s, body: vec![frac], reset_font: false },
                    None => frac,
                })
            }
            "\\over" | "\\choose" | "\\atop" | "\\brace" | "\\brack" => {
                let replace_with = match name {
                    "\\over" => "\\frac",
                    "\\choose" => "\\binom",
                    "\\atop" => "\\\\atopfrac",
                    "\\brace" => "\\\\bracefrac",
                    _ => "\\\\brackfrac",
                };
                Ok(P::Infix { replace_with: replace_with.into() })
            }
            "\\sqrt" => {
                let body = take(&mut args);
                let index = opt.pop().flatten();
                Ok(P::Sqrt { mode, body: Box::new(body), index: index.map(Box::new) })
            }
            "\\mathop" => Ok(P::Op { mode, limits: false, always_handle_supsub: false, suppress_base_shift: false, symbol: false, name: None, body: Some(ordargument(take(&mut args))) }),
            "\\operatorname@" | "\\operatornamewithlimits" => {
                let body = ordargument(take(&mut args));
                let with = name == "\\operatornamewithlimits";
                Ok(P::OperatorName { mode, body, always_handle_supsub: with, limits: with })
            }
            _ if OPS_SYMBOL_LIMITS.contains(&name) => {
                let f = match name {
                    "\u{220F}" => "\\prod",
                    "\u{2210}" => "\\coprod",
                    "\u{2211}" => "\\sum",
                    "\u{22c0}" => "\\bigwedge",
                    "\u{22c1}" => "\\bigvee",
                    "\u{22c2}" => "\\bigcap",
                    "\u{22c3}" => "\\bigcup",
                    "\u{2a00}" => "\\bigodot",
                    "\u{2a01}" => "\\bigoplus",
                    "\u{2a02}" => "\\bigotimes",
                    "\u{2a04}" => "\\biguplus",
                    "\u{2a06}" => "\\bigsqcup",
                    n => n,
                };
                Ok(P::Op { mode, limits: true, always_handle_supsub: false, suppress_base_shift: false, symbol: true, name: Some(f.to_string()), body: None })
            }
            _ if OPS_NAMED.contains(&name) => Ok(P::Op { mode, limits: false, always_handle_supsub: false, suppress_base_shift: false, symbol: false, name: Some(name.to_string()), body: None }),
            _ if OPS_NAMED_LIMITS.contains(&name) => Ok(P::Op { mode, limits: true, always_handle_supsub: false, suppress_base_shift: false, symbol: false, name: Some(name.to_string()), body: None }),
            _ if OPS_INTEGRALS.contains(&name) => {
                let f = match name {
                    "\u{222b}" => "\\int",
                    "\u{222c}" => "\\iint",
                    "\u{222d}" => "\\iiint",
                    "\u{222e}" => "\\oint",
                    "\u{222f}" => "\\oiint",
                    "\u{2230}" => "\\oiiint",
                    n => n,
                };
                Ok(P::Op { mode, limits: false, always_handle_supsub: false, suppress_base_shift: false, symbol: true, name: Some(f.to_string()), body: None })
            }
            _ if DELIM_SIZES.iter().any(|d| d.0 == name) => {
                let (_, mclass, size) = DELIM_SIZES.iter().copied().find(|d| d.0 == name).unwrap_or(("", "mord", 1));
                let delim = self.check_delimiter(&take(&mut args), name)?;
                Ok(P::DelimSizing { mode, size, mclass, delim })
            }
            "\\right" => {
                let delim = self.check_delimiter(&take(&mut args), name)?;
                let color = match self.gullet.get_macro("\\current@color") {
                    Some(MacroDef::Text(s)) => Some(s),
                    _ => None,
                };
                Ok(P::LeftRightRight { delim, color })
            }
            "\\left" => {
                let left = self.check_delimiter(&take(&mut args), name)?;
                self.leftright_depth += 1;
                let body = self.parse_expression(false, None)?;
                self.leftright_depth -= 1;
                self.expect("\\right", false)?;
                let right = self.parse_function(None, true)?;
                let Some(P::LeftRightRight { delim, color }) = right else {
                    return Err(ParseError("Expected \\right".into()));
                };
                Ok(P::LeftRight { mode, body, left, right: delim, right_color: color })
            }
            "\\middle" => {
                let delim = self.check_delimiter(&take(&mut args), name)?;
                if self.leftright_depth == 0 {
                    return Err(ParseError("\\middle without preceding \\left".into()));
                }
                Ok(P::Middle { mode, delim })
            }
            _ if MATH_ACCENTS.contains(&name) => {
                let base = normalize_argument(take(&mut args));
                let non_stretchy = ["\\acute", "\\grave", "\\ddot", "\\tilde", "\\bar", "\\breve", "\\check", "\\hat", "\\vec", "\\dot", "\\mathring"];
                let is_stretchy = !non_stretchy.contains(&name);
                let is_shifty = !is_stretchy || matches!(name, "\\widehat" | "\\widetilde" | "\\widecheck");
                Ok(P::Accent { mode, label: name.to_string(), is_stretchy, is_shifty, base: Box::new(base) })
            }
            _ if TEXT_ACCENTS.contains(&name) => {
                let base = take(&mut args);
                Ok(P::Accent { mode: Mode::Text, label: name.to_string(), is_stretchy: false, is_shifty: true, base: Box::new(base) })
            }
            "\\text" | "\\textrm" | "\\textsf" | "\\texttt" | "\\textnormal" | "\\textbf" | "\\textmd" | "\\textit" | "\\textup" | "\\emph" => Ok(P::Text { mode, body: ordargument(take(&mut args)), font: Some(name.to_string()) }),
            "\\mathrm" | "\\mathit" | "\\mathbf" | "\\mathnormal" | "\\mathsfit" | "\\mathbb" | "\\mathcal" | "\\mathfrak" | "\\mathscr" | "\\mathsf" | "\\mathtt" | "\\Bbb" | "\\bold" | "\\frak" => {
                let body = normalize_argument(take(&mut args));
                let f = match name {
                    "\\Bbb" => "\\mathbb",
                    "\\bold" => "\\mathbf",
                    "\\frak" => "\\mathfrak",
                    n => n,
                };
                Ok(P::Font { mode, font: f[1..].to_string(), body: Box::new(body) })
            }
            "\\boldsymbol" | "\\bm" => {
                let body = take(&mut args);
                let mclass = binrel_class(&body);
                let icb = is_character_box(&body);
                Ok(P::MClass { mode, mclass, body: vec![P::Font { mode, font: "boldsymbol".into(), body: Box::new(body) }], is_character_box: icb })
            }
            "\\rm" | "\\sf" | "\\tt" | "\\bf" | "\\it" | "\\cal" => {
                let body = self.parse_expression(true, break_on)?;
                let style = format!("math{}", &name[1..]);
                Ok(P::Font { mode, font: style, body: Box::new(P::OrdGroup { mode, body }) })
            }
            "\\textcolor" => {
                let Some(P::ColorToken { color }) = args.first().cloned() else { return Err(ParseError("Expected color".into())) };
                let body = ordargument(args.get(1).cloned().unwrap_or(P::OrdGroup { mode, body: vec![] }));
                Ok(P::Color { mode, color, body })
            }
            "\\color" => {
                let Some(P::ColorToken { color }) = args.first().cloned() else { return Err(ParseError("Expected color".into())) };
                self.gullet.set_macro("\\current@color", Some(MacroDef::Text(color.clone())), false);
                let body = self.parse_expression(true, break_on)?;
                Ok(P::Color { mode, color, body })
            }
            "\\displaystyle" | "\\textstyle" | "\\scriptstyle" | "\\scriptscriptstyle" => {
                let body = self.parse_expression(true, break_on)?;
                let style = match name {
                    "\\displaystyle" => StyleStr::Display,
                    "\\textstyle" => StyleStr::Text,
                    "\\scriptstyle" => StyleStr::Script,
                    _ => StyleStr::ScriptScript,
                };
                Ok(P::Styling { mode, style, body, reset_font: false })
            }
            _ if SIZE_FUNCS.contains(&name) => {
                let body = self.parse_expression(false, break_on)?;
                let size = SIZE_FUNCS.iter().position(|s| *s == name).unwrap_or(5) as u8 + 1;
                Ok(P::Sizing { mode, size, body })
            }
            "\\kern" | "\\mkern" | "\\hskip" | "\\mskip" => {
                let Some(P::Size { value, .. }) = args.first().cloned() else { return Err(ParseError("Expected size".into())) };
                Ok(P::Kern { mode, dim: value })
            }
            "\\overline" => Ok(P::Overline { mode, body: Box::new(take(&mut args)) }),
            "\\underline" => Ok(P::Underline { mode, body: Box::new(take(&mut args)) }),
            "\\mathord" | "\\mathbin" | "\\mathrel" | "\\mathopen" | "\\mathclose" | "\\mathpunct" | "\\mathinner" => {
                let body = take(&mut args);
                let icb = is_character_box(&body);
                Ok(P::MClass { mode, mclass: format!("m{}", &name[5..]), body: ordargument(body), is_character_box: icb })
            }
            "\\@binrel" => {
                let a0 = take(&mut args);
                let a1 = take(&mut args);
                let icb = is_character_box(&a1);
                Ok(P::MClass { mode, mclass: binrel_class(&a0), body: ordargument(a1), is_character_box: icb })
            }
            "\\stackrel" | "\\overset" | "\\underset" => {
                let shifted = take(&mut args);
                let base_arg = take(&mut args);
                let mclass = if name != "\\stackrel" { binrel_class(&base_arg) } else { "mrel".to_string() };
                let base_mode = base_arg.mode();
                let base_op = P::Op { mode: base_mode, limits: true, always_handle_supsub: true, suppress_base_shift: name != "\\stackrel", symbol: false, name: None, body: Some(ordargument(base_arg)) };
                let (sup, sub) = if name == "\\underset" { (None, Some(Box::new(shifted))) } else { (Some(Box::new(shifted)), None) };
                let supsub = P::SupSub { mode, base: Some(Box::new(base_op)), sup, sub };
                let icb = is_character_box(&supsub);
                Ok(P::MClass { mode, mclass, body: vec![supsub], is_character_box: icb })
            }
            "\\mathllap" | "\\mathrlap" | "\\mathclap" => Ok(P::Lap { mode, alignment: name[5..].to_string(), body: Box::new(take(&mut args)) }),
            "\\html@mathml" => {
                let html = take(&mut args);
                Ok(P::HtmlMathml { mode, body: ordargument(html) })
            }
            "\\relax" => Ok(P::Internal),
            "\\@char" => {
                let Some(P::Raw { string }) = args.first().cloned() else { return Err(ParseError("Expected raw".into())) };
                let code: u32 = string.trim().parse().map_err(|_| ParseError(format!("\\@char has non-numeric argument {string}")))?;
                let text = char::from_u32(code).map(|c| c.to_string()).unwrap_or_default();
                Ok(P::TextOrd { mode, text })
            }
            "\\phantom" => Ok(P::Phantom { mode, body: ordargument(take(&mut args)) }),
            "\\smash" => {
                let (mut height, mut depth) = (true, true);
                if let Some(Some(P::OrdGroup { body, .. })) = opt.first() {
                    height = false;
                    depth = false;
                    for n in body {
                        match n.symbol_text() {
                            Some("t") => height = true,
                            Some("b") => depth = true,
                            _ => {
                                height = false;
                                depth = false;
                                break;
                            }
                        }
                    }
                }
                Ok(P::Smash { mode, body: Box::new(take(&mut args)), height, depth })
            }
            "\\overbrace" | "\\underbrace" | "\\overbracket" | "\\underbracket" => Ok(P::HorizBrace { mode, label: name.to_string(), is_over: name.contains("\\over"), base: Box::new(take(&mut args)) }),
            "\\vphantom" => Ok(P::VPhantom { mode, body: Box::new(take(&mut args)) }),
            "\\rule" => {
                let shift = opt.pop().flatten().and_then(|s| if let P::Size { value, .. } = s { Some(value) } else { None });
                let width = if let P::Size { value, .. } = take(&mut args) { value } else { return Err(ParseError("Expected size".into())) };
                let height = if let P::Size { value, .. } = take(&mut args) { value } else { return Err(ParseError("Expected size".into())) };
                Ok(P::Rule { mode, shift, width, height })
            }
            "\\boxed" | "\\fbox" => Ok(P::Enclose { mode, label: "\\fbox".into(), body: Box::new(take(&mut args)), background: None, border: None }),
            "\\colorbox" => {
                let Some(P::ColorToken { color }) = args.first().cloned() else { return Err(ParseError("Expected color".into())) };
                let body = args.get(1).cloned().unwrap_or(P::OrdGroup { mode, body: vec![] });
                Ok(P::Enclose { mode, label: "\\colorbox".into(), body: Box::new(body), background: Some(color), border: None })
            }
            "\\cancel" | "\\bcancel" | "\\xcancel" | "\\sout" => Ok(P::Enclose { mode, label: name.to_string(), body: Box::new(take(&mut args)), background: None, border: None }),
            "\\\\" | "\\newline" => Ok(P::Cr),
            "\\begin" | "\\end" => {
                let name_group = take(&mut args);
                let P::OrdGroup { body, .. } = &name_group else { return Err(ParseError("Invalid environment name".into())) };
                let env: String = body.iter().filter_map(|p| p.symbol_text()).collect();
                if name == "\\end" {
                    return Ok(P::Environment { name: env });
                }
                let result = self.parse_environment(&env)?;
                self.expect("\\end", false)?;
                let end = self.parse_function(None, true)?;
                match end {
                    Some(P::Environment { name: e }) if e == env => Ok(result),
                    Some(P::Environment { name: e }) => Err(ParseError(format!("Mismatch: \\begin{{{env}}} matched by \\end{{{e}}}"))),
                    _ => Err(ParseError("Expected \\end".into())),
                }
            }
            _ => Err(ParseError(format!("No function handler for {name}"))),
        }
    }

    // ── Environments (environments/array.ts) ────────────────────────────────

    fn get_hlines(&mut self) -> Result<Vec<bool>, ParseError> {
        let mut info = Vec::new();
        self.consume_spaces()?;
        let mut nxt = self.fetch()?.text;
        if nxt == "\\relax" {
            self.consume();
            self.consume_spaces()?;
            nxt = self.fetch()?.text;
        }
        while nxt == "\\hline" || nxt == "\\hdashline" {
            self.consume();
            info.push(nxt == "\\hdashline");
            self.consume_spaces()?;
            nxt = self.fetch()?.text;
        }
        Ok(info)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_array(&mut self, hskip: bool, add_jot: bool, cols: Vec<AlignSpec>, arraystretch: Option<f64>, col_sep: Option<&'static str>, empty_single_row: bool, max_num_cols: Option<usize>, style: Option<StyleStr>) -> Result<ArrayNode, ParseError> {
        self.gullet.begin_group();
        self.gullet.set_macro("\\cr", Some(MacroDef::Text("\\\\\\relax".into())), false);
        let arraystretch = match arraystretch {
            Some(a) => a,
            None => match self.gullet.expand_macro_as_text("\\arraystretch")? {
                Some(s) => s.trim().parse::<f64>().ok().filter(|v| *v > 0.0).ok_or_else(|| ParseError(format!("Invalid \\arraystretch: {s}")))?,
                None => 1.0,
            },
        };
        self.gullet.begin_group();
        let mut row: Vec<P> = Vec::new();
        let mut body: Vec<Vec<P>> = Vec::new();
        let mut row_gaps = Vec::new();
        let mut hlines = Vec::new();
        hlines.push(self.get_hlines()?);
        loop {
            let cell_body = self.parse_expression(false, Some("\\\\"))?;
            self.gullet.end_group();
            self.gullet.begin_group();
            let mut cell = P::OrdGroup { mode: self.mode, body: cell_body };
            if let Some(s) = style {
                cell = P::Styling { mode: self.mode, style: s, body: vec![cell], reset_font: true };
            }
            row.push(cell);
            let next = self.fetch()?.text;
            if next == "&" {
                if let Some(m) = max_num_cols {
                    if row.len() == m && col_sep.is_some() {
                        return Err(ParseError("Too many tab characters: &".into()));
                    }
                }
                self.consume();
            } else if next == "\\end" {
                let empty_last = row.len() == 1 && matches!(&row[0], P::Styling { body, .. } if body.len() == 1 && matches!(&body[0], P::OrdGroup { body: b, .. } if b.is_empty()));
                body.push(std::mem::take(&mut row));
                if empty_last && (body.len() > 1 || !empty_single_row) {
                    body.pop();
                }
                if hlines.len() < body.len() + 1 {
                    hlines.push(Vec::new());
                }
                break;
            } else if next == "\\\\" {
                self.consume();
                let mut size = None;
                if self.gullet.future()?.text != " " {
                    if let Some(P::Size { value, .. }) = self.parse_size_group(true)? {
                        size = Some(value);
                    }
                }
                row_gaps.push(size);
                hlines.push(self.get_hlines()?);
                body.push(std::mem::take(&mut row));
            } else {
                return Err(ParseError("Expected & or \\\\ or \\cr or \\end".into()));
            }
        }
        self.gullet.end_group();
        self.gullet.end_group();
        Ok(ArrayNode { mode: self.mode, add_jot, arraystretch, body, cols, row_gaps, hskip_before_and_after: hskip, hlines_before_row: hlines, col_separation_type: col_sep })
    }

    fn parse_environment(&mut self, env: &str) -> Result<P, ParseError> {
        let mode = self.mode;
        let d_cell = |e: &str| if e.starts_with('d') { StyleStr::Display } else { StyleStr::Text };
        match env {
            "array" | "darray" => {
                let spec = self.parse_argument_group(false, None)?.ok_or_else(|| ParseError("Expected column alignment".into()))?;
                let nodes = match spec {
                    P::OrdGroup { body, .. } => body,
                    other => vec![other],
                };
                let mut cols = Vec::new();
                for n in &nodes {
                    let t = n.symbol_text().unwrap_or("");
                    match t {
                        "l" | "c" | "r" => cols.push(AlignSpec::Align { align: t.chars().next().unwrap_or('c'), pregap: None, postgap: None }),
                        "|" | "\u{2223}" => cols.push(AlignSpec::Separator('|')),
                        ":" => cols.push(AlignSpec::Separator(':')),
                        _ => return Err(ParseError(format!("Unknown column alignment: {t}"))),
                    }
                }
                let n = cols.len();
                let a = self.parse_array(true, false, cols, None, None, false, Some(n), Some(d_cell(env)))?;
                Ok(P::Array(Box::new(a)))
            }
            "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" | "matrix*" | "pmatrix*" | "bmatrix*" | "Bmatrix*" | "vmatrix*" | "Vmatrix*" => {
                let base = env.trim_end_matches('*');
                let delims = match base {
                    "pmatrix" => Some(("(", ")")),
                    "bmatrix" => Some(("[", "]")),
                    "Bmatrix" => Some(("\\{", "\\}")),
                    "vmatrix" => Some(("|", "|")),
                    "Vmatrix" => Some(("\\Vert", "\\Vert")),
                    _ => None,
                };
                let mut col_align = 'c';
                if env.ends_with('*') {
                    self.gullet.consume_spaces()?;
                    if self.fetch()?.text == "[" {
                        self.consume();
                        self.consume_spaces()?;
                        let t = self.fetch()?.text;
                        if !matches!(t.as_str(), "l" | "c" | "r") {
                            return Err(ParseError("Expected l or c or r".into()));
                        }
                        col_align = t.chars().next().unwrap_or('c');
                        self.consume();
                        self.consume_spaces()?;
                        self.expect("]", true)?;
                    }
                }
                let mut a = self.parse_array(false, false, vec![AlignSpec::Align { align: col_align, pregap: None, postgap: None }], None, None, false, None, Some(d_cell(env)))?;
                let num_cols = a.body.iter().map(Vec::len).max().unwrap_or(0);
                a.cols = vec![AlignSpec::Align { align: col_align, pregap: None, postgap: None }; num_cols];
                let arr = P::Array(Box::new(a));
                Ok(match delims {
                    Some((l, r)) => P::LeftRight { mode, body: vec![arr], left: l.into(), right: r.into(), right_color: None },
                    None => arr,
                })
            }
            "smallmatrix" => {
                let mut a = self.parse_array(false, false, vec![], Some(0.5), None, false, None, Some(StyleStr::Script))?;
                a.col_separation_type = Some("small");
                Ok(P::Array(Box::new(a)))
            }
            "cases" | "dcases" | "rcases" | "drcases" => {
                let cols = vec![AlignSpec::Align { align: 'l', pregap: Some(0.0), postgap: Some(1.0) }, AlignSpec::Align { align: 'l', pregap: Some(0.0), postgap: Some(0.0) }];
                let a = self.parse_array(false, false, cols, Some(1.2), None, false, None, Some(d_cell(env)))?;
                let r = env.contains('r');
                Ok(P::LeftRight { mode, body: vec![P::Array(Box::new(a))], left: if r { ".".into() } else { "\\{".into() }, right: if r { "\\}".into() } else { ".".into() }, right_color: None })
            }
            "aligned" | "align" | "align*" | "split" | "alignedat" | "alignat" | "alignat*" => {
                let num_maths = if env.contains("at") {
                    let g = self.parse_argument_group(false, None)?;
                    let s: String = match g {
                        Some(P::OrdGroup { body, .. }) => body.iter().filter_map(|p| p.symbol_text()).collect(),
                        _ => String::new(),
                    };
                    s.parse::<usize>().unwrap_or(0)
                } else {
                    0
                };
                let sep = if env.contains("at") { "alignat" } else { "align" };
                let max = if env == "split" { Some(2) } else { None };
                let mut a = self.parse_array(false, true, vec![], None, Some(sep), true, max, Some(StyleStr::Display))?;
                let mut num_cols = num_maths * 2;
                let is_aligned = num_cols == 0;
                for row in &mut a.body {
                    let mut i = 1;
                    while i < row.len() {
                        if let P::Styling { body, .. } = &mut row[i] {
                            if let Some(P::OrdGroup { body: b, mode }) = body.first_mut() {
                                b.insert(0, P::OrdGroup { mode: *mode, body: vec![] });
                            }
                        }
                        i += 2;
                    }
                    if is_aligned && num_cols < row.len() {
                        num_cols = row.len();
                    }
                }
                a.cols = (0..num_cols)
                    .map(|i| {
                        let (align, pregap) = if i % 2 == 1 { ('l', 0.0) } else if i > 0 && is_aligned { ('r', 1.0) } else { ('r', 0.0) };
                        AlignSpec::Align { align, pregap: Some(pregap), postgap: Some(0.0) }
                    })
                    .collect();
                a.col_separation_type = Some(if is_aligned { "align" } else { "alignat" });
                Ok(P::Array(Box::new(a)))
            }
            "gathered" | "gather" | "gather*" => {
                let mut a = self.parse_array(false, true, vec![AlignSpec::Align { align: 'c', pregap: None, postgap: None }], None, Some("gather"), true, None, Some(StyleStr::Display))?;
                a.cols = vec![AlignSpec::Align { align: 'c', pregap: None, postgap: None }];
                Ok(P::Array(Box::new(a)))
            }
            "equation" | "equation*" => {
                let mut a = self.parse_array(false, false, vec![], None, None, true, Some(1), Some(StyleStr::Display))?;
                a.cols = vec![];
                Ok(P::Array(Box::new(a)))
            }
            _ => Err(ParseError(format!("No such environment: {env}"))),
        }
    }
}

/// `binrelClass`.
fn binrel_class(arg: &P) -> String {
    let atom = match arg {
        P::OrdGroup { body, .. } if !body.is_empty() => &body[0],
        other => other,
    };
    match atom {
        P::Atom { family: Group::Bin, .. } => "mbin".into(),
        P::Atom { family: Group::Rel, .. } => "mrel".into(),
        _ => "mord".into(),
    }
}

fn size_prefix_ok(s: &str) -> bool {
    // /^[-+]? *(?:$|\d+|\d+\.\d*|\.\d*) *[a-z]{0,2} *$/
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    if i < b.len() && (b[i] == '-' || b[i] == '+') {
        i += 1;
    }
    while i < b.len() && b[i] == ' ' {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == '.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    let _ = ds;
    while i < b.len() && b[i] == ' ' {
        i += 1;
    }
    let ls = i;
    while i < b.len() && b[i].is_ascii_lowercase() && i - ls < 2 {
        i += 1;
    }
    while i < b.len() && b[i] == ' ' {
        i += 1;
    }
    i == b.len()
}

/// `([-+]?) *(\d+(?:\.\d*)?|\.\d+) *([a-z]{2})` and a valid unit.
pub fn parse_size(text: &str) -> Option<Measurement> {
    let t: String = text.chars().filter(|c| *c != ' ').collect();
    let (sign, rest) = match t.chars().next() {
        Some('-') => (-1.0, &t[1..]),
        Some('+') => (1.0, &t[1..]),
        _ => (1.0, t.as_str()),
    };
    let num_end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
    let num: f64 = rest[..num_end].parse().ok()?;
    let unit = &rest[num_end..];
    if unit.len() < 2 {
        return None;
    }
    let unit = &unit[..2];
    let valid = matches!(unit, "pt" | "mm" | "cm" | "in" | "bp" | "pc" | "dd" | "cc" | "nd" | "nc" | "sp" | "px" | "ex" | "em" | "mu");
    if !valid {
        return None;
    }
    Some(Measurement { number: sign * num, unit: unit.to_string() })
}

fn is_combining(c: char) -> bool {
    ('\u{0300}'..='\u{036f}').contains(&c)
}

/// `unicodeSymbols`: a precomposed Latin letter decomposed (NFD) into base + combining marks.
fn decompose(c: char) -> Option<String> {
    let (base, mark) = match c {
        'à' => ('a', '\u{300}'),
        'á' => ('a', '\u{301}'),
        'â' => ('a', '\u{302}'),
        'ã' => ('a', '\u{303}'),
        'ä' => ('a', '\u{308}'),
        'è' => ('e', '\u{300}'),
        'é' => ('e', '\u{301}'),
        'ê' => ('e', '\u{302}'),
        'ë' => ('e', '\u{308}'),
        'ì' => ('i', '\u{300}'),
        'í' => ('i', '\u{301}'),
        'î' => ('i', '\u{302}'),
        'ï' => ('i', '\u{308}'),
        'ò' => ('o', '\u{300}'),
        'ó' => ('o', '\u{301}'),
        'ô' => ('o', '\u{302}'),
        'õ' => ('o', '\u{303}'),
        'ö' => ('o', '\u{308}'),
        'ù' => ('u', '\u{300}'),
        'ú' => ('u', '\u{301}'),
        'û' => ('u', '\u{302}'),
        'ü' => ('u', '\u{308}'),
        'ñ' => ('n', '\u{303}'),
        'ç' => ('c', '\u{327}'),
        'ÿ' => ('y', '\u{308}'),
        'À' => ('A', '\u{300}'),
        'Á' => ('A', '\u{301}'),
        'Â' => ('A', '\u{302}'),
        'Ã' => ('A', '\u{303}'),
        'Ä' => ('A', '\u{308}'),
        'È' => ('E', '\u{300}'),
        'É' => ('E', '\u{301}'),
        'Ê' => ('E', '\u{302}'),
        'Ë' => ('E', '\u{308}'),
        'Î' => ('I', '\u{302}'),
        'Ï' => ('I', '\u{308}'),
        'Ô' => ('O', '\u{302}'),
        'Ö' => ('O', '\u{308}'),
        'Ù' => ('U', '\u{300}'),
        'Û' => ('U', '\u{302}'),
        'Ü' => ('U', '\u{308}'),
        'Ç' => ('C', '\u{327}'),
        _ => return None,
    };
    Some(format!("{base}{mark}"))
}

/// `unicodeAccents`: the accent command of a combining mark in `mode`.
fn combining_accent(m: char, mode: Mode) -> Option<&'static str> {
    let (text, math) = match m {
        '\u{0301}' => ("\\'", Some("\\acute")),
        '\u{0300}' => ("\\`", Some("\\grave")),
        '\u{0308}' => ("\\\"", Some("\\ddot")),
        '\u{0303}' => ("\\~", Some("\\tilde")),
        '\u{0304}' => ("\\=", Some("\\bar")),
        '\u{0306}' => ("\\u", Some("\\breve")),
        '\u{030c}' => ("\\v", Some("\\check")),
        '\u{0302}' => ("\\^", Some("\\hat")),
        '\u{0307}' => ("\\.", Some("\\dot")),
        '\u{030a}' => ("\\r", Some("\\mathring")),
        '\u{030b}' => ("\\H", None),
        '\u{0327}' => ("\\c", None),
        _ => return None,
    };
    Some(match mode {
        Mode::Math => math.unwrap_or(text),
        Mode::Text => text,
    })
}

/// `formLigatures` (text mode): `--`, `---`, ``` `` ``` and `''`.
fn form_ligatures(group: &mut Vec<P>) {
    let mut i = 0;
    while i + 1 < group.len() {
        let (a, b) = (&group[i], &group[i + 1]);
        if let (P::TextOrd { text: v, .. }, P::TextOrd { text: n, .. }) = (a, b) {
            if v == "-" && n == "-" {
                let third = matches!(group.get(i + 2), Some(P::TextOrd { text, .. }) if text == "-");
                if third {
                    group.splice(i..i + 3, [P::TextOrd { mode: Mode::Text, text: "---".into() }]);
                } else {
                    group.splice(i..i + 2, [P::TextOrd { mode: Mode::Text, text: "--".into() }]);
                }
            } else if (v == "'" || v == "`") && n == v {
                let t = format!("{v}{v}");
                group.splice(i..i + 2, [P::TextOrd { mode: Mode::Text, text: t }]);
            }
        }
        i += 1;
    }
}

/// Parses `latex` as KaTeX does with `displayMode: true, throwOnError: false`.
pub fn parse(latex: &str) -> Result<Vec<P>, ParseError> {
    Parser::new(latex).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_presets() {
        for src in [
            "A = \\pi r^2",
            "(x+a)^n = \\sum_{k=0}^{n} \\binom{n}{k} x^k a^{n-k}",
            "(1+x)^n = 1 + \\frac{nx}{1!} + \\frac{n(n-1)x^2}{2!} + \\cdots",
            "f(x) = a_0 + \\sum_{n=1}^{\\infty} \\left( a_n \\cos \\frac{n\\pi x}{L} + b_n \\sin \\frac{n\\pi x}{L} \\right)",
            "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
            "e^x = 1 + \\frac{x}{1!} + \\frac{x^2}{2!} + \\frac{x^3}{3!} + \\cdots, \\quad -\\infty < x < \\infty",
            "\\sin\\alpha \\pm \\sin\\beta = 2 \\sin\\tfrac{1}{2}(\\alpha \\pm \\beta) \\cos\\tfrac{1}{2}(\\alpha \\mp \\beta)",
            "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}",
            "a \\neq b, \\text{si } x > 0",
            "f(x) = \\begin{cases} x & x \\ge 0 \\\\ -x & x < 0 \\end{cases}",
            "\\hat{x} \\vec{v} \\bar{y} \\color{red}{z} \\mathbb{R}",
        ] {
            let r = parse(src);
            assert!(r.is_ok(), "{src}: {r:?}");
        }
    }

    #[test]
    fn unknown_commands_become_red_text() {
        let r = parse("\\foo x").unwrap_or_default();
        assert!(matches!(&r[0], P::Color { color, .. } if color == crate::ERROR_COLOR));
    }

    #[test]
    fn errors_are_reported() {
        assert!(parse("\\frac{a}{").is_err());
        assert!(parse("x^^2").is_err());
    }
}
