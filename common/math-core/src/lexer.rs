//! KaTeX's `Lexer.ts` and `MacroExpander.ts` (the "gullet"): the input split into TeX tokens, and the macros
//! expanded as tokens are fetched. String macros come from KaTeX's own table (`data/macros.json`); the few
//! macros KaTeX defines as functions (`\dots`, `\char`, `\TextOrMath`…) are written here.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::data::{self, Group, Mode};
use crate::ParseError;

/// A token: its text and the expansion flags of `\noexpand`.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub text: String,
    pub noexpand: bool,
    pub treat_as_relax: bool,
}

impl Token {
    pub fn new(text: impl Into<String>) -> Token {
        Token { text: text.into(), noexpand: false, treat_as_relax: false }
    }
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\r' | '\n' | '\t')
}

fn is_combining(c: char) -> bool {
    ('\u{0300}'..='\u{036f}').contains(&c)
}

/// The single-codepoint token class of KaTeX's lexer (printable BMP minus the backslash and the bidi controls) or a non-BMP character.
fn is_single(c: char) -> bool {
    let u = c as u32;
    (0x21..=0x5b).contains(&u) || (0x5d..=0x2027).contains(&u) || (0x202a..=0xd7ff).contains(&u) || (0xf900..=0xffff).contains(&u) || u > 0xffff
}

/// The lexer over one input (`Lexer.lex`). `%` comments and `~` active are the only catcodes.
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Lexer {
        Lexer { chars: input.chars().collect(), pos: 0 }
    }

    pub fn lex(&mut self) -> Result<Token, ParseError> {
        let c = &self.chars;
        let start = self.pos;
        if start >= c.len() {
            return Ok(Token::new("EOF"));
        }
        let ch = c[start];
        if is_space(ch) {
            let mut i = start;
            while i < c.len() && is_space(c[i]) {
                i += 1;
            }
            self.pos = i;
            return Ok(Token::new(" "));
        }
        if ch == '%' {
            // Comment: up to and including the newline.
            let mut i = start;
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            self.pos = (i + 1).min(c.len());
            return self.lex();
        }
        if ch == '\\' {
            let next = c.get(start + 1).copied();
            match next {
                // Control space: `\` + (newline | spaces + optional newline) + spaces.
                Some(n) if n == '\n' || n == ' ' || n == '\r' || n == '\t' => {
                    let mut i = start + 1;
                    if n == '\n' {
                        i += 1;
                    } else {
                        while i < c.len() && matches!(c[i], ' ' | '\r' | '\t') {
                            i += 1;
                        }
                        if i < c.len() && c[i] == '\n' {
                            i += 1;
                        }
                    }
                    while i < c.len() && matches!(c[i], ' ' | '\r' | '\t') {
                        i += 1;
                    }
                    self.pos = i;
                    return Ok(Token::new("\\ "));
                }
                Some(n) if n.is_ascii_alphabetic() || n == '@' => {
                    let mut i = start + 1;
                    while i < c.len() && (c[i].is_ascii_alphabetic() || c[i] == '@') {
                        i += 1;
                    }
                    let text: String = c[start..i].iter().collect();
                    while i < c.len() && is_space(c[i]) {
                        i += 1;
                    }
                    self.pos = i;
                    return Ok(Token::new(text));
                }
                Some(n) => {
                    self.pos = start + 2;
                    return Ok(Token::new(format!("\\{n}")));
                }
                None => return Err(ParseError("Unexpected character: '\\'".into())),
            }
        }
        if is_single(ch) {
            let mut i = start + 1;
            while i < c.len() && is_combining(c[i]) {
                i += 1;
            }
            self.pos = i;
            return Ok(Token::new(c[start..i].iter().collect::<String>()));
        }
        Err(ParseError(format!("Unexpected character: '{ch}'")))
    }
}

/// A macro definition.
#[derive(Clone, Debug)]
pub enum MacroDef {
    /// A body to lex (`#1`… placeholders).
    Text(String),
    /// Already lexed tokens (reversed) and the argument count.
    Tokens(Vec<Token>, usize),
    /// One of the macros KaTeX writes as a function.
    Builtin(Builtin),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    NoExpand,
    ExpandAfter,
    FirstOfTwo,
    SecondOfTwo,
    IfNextChar,
    TextOrMath,
    Char,
    Dots,
    Dotso,
    Dotsc,
    Cdots,
}

fn builtin_macros() -> &'static HashMap<String, MacroDef> {
    static M: OnceLock<HashMap<String, MacroDef>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = HashMap::new();
        let v: Value = serde_json::from_str(include_str!("../data/macros.json")).unwrap_or(Value::Null);
        if let Some(o) = v.as_object() {
            for (k, body) in o {
                if let Some(s) = body.as_str() {
                    m.insert(k.clone(), MacroDef::Text(s.to_string()));
                }
            }
        }
        for (name, b) in [
            ("\\noexpand", Builtin::NoExpand),
            ("\\expandafter", Builtin::ExpandAfter),
            ("\\@firstoftwo", Builtin::FirstOfTwo),
            ("\\@secondoftwo", Builtin::SecondOfTwo),
            ("\\@ifnextchar", Builtin::IfNextChar),
            ("\\TextOrMath", Builtin::TextOrMath),
            ("\\char", Builtin::Char),
            ("\\dots", Builtin::Dots),
            ("\\dotso", Builtin::Dotso),
            ("\\dotsc", Builtin::Dotsc),
            ("\\cdots", Builtin::Cdots),
        ] {
            m.insert(name.to_string(), MacroDef::Builtin(b));
        }
        m
    })
}

const SPACE_AFTER_DOTS: &[&str] = &[")", "]", "\\rbrack", "\\}", "\\rbrace", "\\rangle", "\\rceil", "\\rfloor", "\\rgroup", "\\rmoustache", "\\right", "\\bigr", "\\biggr", "\\Bigr", "\\Biggr", "$", ";", ".", ","];

fn dots_by_token(t: &str) -> Option<&'static str> {
    Some(match t {
        "," => "\\dotsc",
        "\\not" => "\\dotsb",
        "+" | "=" | "<" | ">" | "-" | "*" | ":" => "\\dotsb",
        "\\DOTSB" | "\\coprod" | "\\bigvee" | "\\bigwedge" | "\\biguplus" | "\\bigcap" | "\\bigcup" | "\\prod" | "\\sum" | "\\bigotimes" | "\\bigoplus" | "\\bigodot" | "\\bigsqcup" | "\\And" | "\\longrightarrow" | "\\Longrightarrow" | "\\longleftarrow" | "\\Longleftarrow" | "\\longleftrightarrow" | "\\Longleftrightarrow" | "\\mapsto" | "\\longmapsto" | "\\hookrightarrow" | "\\doteq" | "\\mathbin" | "\\mathrel" | "\\relbar" | "\\Relbar" | "\\xrightarrow" | "\\xleftarrow" => "\\dotsb",
        "\\DOTSI" | "\\int" | "\\oint" | "\\iint" | "\\iiint" | "\\iiiint" | "\\idotsint" => "\\dotsi",
        "\\DOTSX" => "\\dotsx",
        _ => return None,
    })
}

/// The macro expander (`MacroExpander`).
pub struct Gullet {
    lexer: Lexer,
    /// Scoped user definitions (`\def`-like sets made while parsing, e.g. `\current@color`).
    scopes: Vec<HashMap<String, Option<MacroDef>>>,
    globals: HashMap<String, Option<MacroDef>>,
    /// Tokens in reverse order.
    pub stack: Vec<Token>,
    pub mode: Mode,
    expansion_count: usize,
}

const MAX_EXPAND: usize = 1000;

impl Gullet {
    pub fn new(input: &str, mode: Mode) -> Gullet {
        Gullet { lexer: Lexer::new(input), scopes: Vec::new(), globals: HashMap::new(), stack: Vec::new(), mode, expansion_count: 0 }
    }

    pub fn begin_group(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn end_group(&mut self) {
        self.scopes.pop();
    }

    /// `macros.set(name, value, global)`.
    pub fn set_macro(&mut self, name: &str, value: Option<MacroDef>, global: bool) {
        if global {
            for s in &mut self.scopes {
                s.remove(name);
            }
            self.globals.insert(name.to_string(), value);
        } else if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), value);
        } else {
            self.globals.insert(name.to_string(), value);
        }
    }

    pub fn get_macro(&self, name: &str) -> Option<MacroDef> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return v.clone();
            }
        }
        if let Some(v) = self.globals.get(name) {
            return v.clone();
        }
        builtin_macros().get(name).cloned()
    }

    pub fn has_macro(&self, name: &str) -> bool {
        self.get_macro(name).is_some()
    }

    pub fn future(&mut self) -> Result<Token, ParseError> {
        if self.stack.is_empty() {
            let t = self.lexer.lex()?;
            self.stack.push(t);
        }
        Ok(self.stack.last().cloned().unwrap_or_else(|| Token::new("EOF")))
    }

    pub fn pop_token(&mut self) -> Result<Token, ParseError> {
        self.future()?;
        Ok(self.stack.pop().unwrap_or_else(|| Token::new("EOF")))
    }

    pub fn push_token(&mut self, t: Token) {
        self.stack.push(t);
    }

    pub fn push_tokens(&mut self, ts: Vec<Token>) {
        self.stack.extend(ts);
    }

    pub fn consume_spaces(&mut self) -> Result<(), ParseError> {
        loop {
            let t = self.future()?;
            if t.text == " " {
                self.stack.pop();
            } else {
                return Ok(());
            }
        }
    }

    /// `scanArgument`: the next argument pushed back followed by an `EOF` token. `None` when an optional
    /// argument is absent.
    pub fn scan_argument(&mut self, optional: bool) -> Result<Option<()>, ParseError> {
        let tokens = if optional {
            self.consume_spaces()?;
            if self.future()?.text != "[" {
                return Ok(None);
            }
            self.pop_token()?;
            self.consume_arg(Some(&["]"]))?
        } else {
            self.consume_arg(None)?
        };
        self.push_token(Token::new("EOF"));
        self.push_tokens(tokens);
        Ok(Some(()))
    }

    /// `consumeArg`: the tokens of one argument (reversed).
    pub fn consume_arg(&mut self, delims: Option<&[&str]>) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        let delimited = delims.is_some_and(|d| !d.is_empty());
        if !delimited {
            self.consume_spaces()?;
        }
        let start = self.future()?;
        let mut depth: i32 = 0;
        let mut matched = 0usize;
        loop {
            let tok = self.pop_token()?;
            tokens.push(tok.clone());
            if tok.text == "{" {
                depth += 1;
            } else if tok.text == "}" {
                depth -= 1;
                if depth == -1 {
                    return Err(ParseError("Extra }".into()));
                }
            } else if tok.text == "EOF" {
                return Err(ParseError("Unexpected end of input in a macro argument".into()));
            }
            if let (Some(d), true) = (delims, delimited) {
                if (depth == 0 || (depth == 1 && d[matched] == "{")) && tok.text == d[matched] {
                    matched += 1;
                    if matched == d.len() {
                        let n = tokens.len();
                        tokens.truncate(n - matched);
                        break;
                    }
                } else {
                    matched = 0;
                }
            }
            if depth == 0 && !delimited {
                break;
            }
        }
        if start.text == "{" && tokens.last().is_some_and(|t| t.text == "}") {
            tokens.pop();
            tokens.remove(0);
        }
        tokens.reverse();
        Ok(tokens)
    }

    pub fn consume_args(&mut self, n: usize) -> Result<Vec<Vec<Token>>, ParseError> {
        let mut out = Vec::new();
        for _ in 0..n {
            out.push(self.consume_arg(None)?);
        }
        Ok(out)
    }

    fn count_expansion(&mut self, amount: usize) -> Result<(), ParseError> {
        self.expansion_count += amount;
        if self.expansion_count > MAX_EXPAND {
            return Err(ParseError("Too many expansions: infinite loop or need to increase maxExpand setting".into()));
        }
        Ok(())
    }

    fn lex_body(body: &str) -> Result<Vec<Token>, ParseError> {
        let mut l = Lexer::new(body);
        let mut out = Vec::new();
        loop {
            let t = l.lex()?;
            if t.text == "EOF" {
                break;
            }
            out.push(t);
        }
        out.reverse();
        Ok(out)
    }

    /// `_getExpansion`: the tokens (reversed) and argument count of `name`, `None` when it is no macro.
    fn get_expansion(&mut self, name: &str) -> Result<Option<(Vec<Token>, usize)>, ParseError> {
        let Some(def) = self.get_macro(name) else { return Ok(None) };
        if name.chars().count() == 1 && name == "%" {
            return Ok(None);
        }
        match def {
            MacroDef::Text(s) => {
                let stripped = s.replace("##", "");
                let mut n = 0;
                while stripped.contains(&format!("#{}", n + 1)) {
                    n += 1;
                }
                Ok(Some((Self::lex_body(&s)?, n)))
            }
            MacroDef::Tokens(t, n) => Ok(Some((t, n))),
            MacroDef::Builtin(b) => self.run_builtin(b).map(Some),
        }
    }

    fn run_builtin(&mut self, b: Builtin) -> Result<(Vec<Token>, usize), ParseError> {
        match b {
            Builtin::NoExpand => {
                let mut t = self.pop_token()?;
                if self.is_expandable(&t.text) {
                    t.noexpand = true;
                    t.treat_as_relax = true;
                }
                Ok((vec![t], 0))
            }
            Builtin::ExpandAfter => {
                let t = self.pop_token()?;
                self.expand_once(true)?;
                Ok((vec![t], 0))
            }
            Builtin::FirstOfTwo => {
                let mut a = self.consume_args(2)?;
                Ok((a.swap_remove(0), 0))
            }
            Builtin::SecondOfTwo => {
                let mut a = self.consume_args(2)?;
                Ok((a.swap_remove(1), 0))
            }
            Builtin::IfNextChar => {
                let mut a = self.consume_args(3)?;
                self.consume_spaces()?;
                let next = self.future()?;
                if a[0].len() == 1 && a[0][0].text == next.text {
                    Ok((a.swap_remove(1), 0))
                } else {
                    Ok((a.swap_remove(2), 0))
                }
            }
            Builtin::TextOrMath => {
                let mut a = self.consume_args(2)?;
                if self.mode == Mode::Text {
                    Ok((a.swap_remove(0), 0))
                } else {
                    Ok((a.swap_remove(1), 0))
                }
            }
            Builtin::Char => {
                let mut token = self.pop_token()?;
                let digit = |s: &str| -> Option<u32> {
                    let c = s.chars().next()?;
                    if s.chars().count() != 1 {
                        return None;
                    }
                    c.to_digit(16)
                };
                let mut number: u32;
                let base: u32;
                if token.text == "'" {
                    base = 8;
                    token = self.pop_token()?;
                } else if token.text == "\"" {
                    base = 16;
                    token = self.pop_token()?;
                } else if token.text == "`" {
                    token = self.pop_token()?;
                    let units: Vec<u16> = token.text.encode_utf16().collect();
                    let n = if token.text.starts_with('\\') { units.get(1).copied().unwrap_or(0) } else { units.first().copied().unwrap_or(0) };
                    if token.text == "EOF" {
                        return Err(ParseError("\\char` missing argument".into()));
                    }
                    return Ok((Self::lex_body(&format!("\\@char{{{n}}}"))?, 0));
                } else {
                    base = 10;
                }
                number = match digit(&token.text) {
                    Some(d) if d < base => d,
                    _ => return Err(ParseError(format!("Invalid base-{base} digit {}", token.text))),
                };
                loop {
                    let f = self.future()?;
                    match digit(&f.text) {
                        Some(d) if d < base => {
                            number = number * base + d;
                            self.pop_token()?;
                        }
                        _ => break,
                    }
                }
                Ok((Self::lex_body(&format!("\\@char{{{number}}}"))?, 0))
            }
            Builtin::Dots => {
                self.expand_once(false)?;
                let next = self.future()?.text;
                let mut thedots = "\\dotso";
                if let Some(d) = dots_by_token(&next) {
                    thedots = d;
                } else if next.starts_with("\\not") {
                    thedots = "\\dotsb";
                } else if let Some(s) = data::symbol(Mode::Math, &next) {
                    if matches!(s.group, Group::Bin | Group::Rel) {
                        thedots = "\\dotsb";
                    }
                }
                Ok((Self::lex_body(thedots)?, 0))
            }
            Builtin::Dotso | Builtin::Dotsc | Builtin::Cdots => {
                let next = self.future()?.text;
                let spaced = SPACE_AFTER_DOTS.contains(&next.as_str()) && !(b == Builtin::Dotsc && next == ",");
                let base = if b == Builtin::Cdots { "\\@cdots" } else { "\\ldots" };
                let body = if spaced { format!("{base}\\,") } else { base.to_string() };
                Ok((Self::lex_body(&body)?, 0))
            }
        }
    }

    /// `expandOnce`: expands the top token when it is a macro; `false` when it was not.
    pub fn expand_once(&mut self, expandable_only: bool) -> Result<bool, ParseError> {
        let top = self.pop_token()?;
        let expansion = if top.noexpand { None } else { self.get_expansion(&top.text)? };
        let Some((mut tokens, num_args)) = expansion else {
            if expandable_only && top.text.starts_with('\\') && !self.is_defined(&top.text) {
                return Err(ParseError(format!("Undefined control sequence: {}", top.text)));
            }
            self.push_token(top);
            return Ok(false);
        };
        self.count_expansion(1)?;
        let args = self.consume_args(num_args)?;
        if num_args > 0 {
            let mut i = tokens.len();
            while i > 0 {
                i -= 1;
                if tokens[i].text == "#" {
                    if i == 0 {
                        return Err(ParseError("Incomplete placeholder at end of macro body".into()));
                    }
                    i -= 1;
                    let t = tokens[i].text.clone();
                    if t == "#" {
                        tokens.remove(i + 1);
                    } else if let Some(d) = t.chars().next().and_then(|c| c.to_digit(10)).filter(|&d| (1..=9).contains(&d) && t.len() == 1) {
                        let arg = args.get(d as usize - 1).cloned().unwrap_or_default();
                        tokens.splice(i..i + 2, arg);
                    } else {
                        return Err(ParseError("Not a valid argument number".into()));
                    }
                }
            }
        }
        self.push_tokens(tokens);
        Ok(true)
    }

    /// `expandNextToken`: the next fully expanded token.
    pub fn expand_next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if !self.expand_once(false)? {
                let mut t = self.stack.pop().unwrap_or_else(|| Token::new("EOF"));
                if t.treat_as_relax {
                    t.text = "\\relax".into();
                }
                return Ok(t);
            }
        }
    }

    /// `expandMacroAsText`.
    pub fn expand_macro_as_text(&mut self, name: &str) -> Result<Option<String>, ParseError> {
        if !self.has_macro(name) {
            return Ok(None);
        }
        let base = self.stack.len();
        self.push_token(Token::new(name));
        let mut out = Vec::new();
        while self.stack.len() > base {
            if !self.expand_once(true)? {
                if let Some(t) = self.stack.pop() {
                    out.push(t.text);
                }
            }
        }
        Ok(Some(out.concat()))
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.has_macro(name) || crate::parse::is_function(name) || data::symbol(Mode::Math, name).is_some() || data::symbol(Mode::Text, name).is_some() || matches!(name, "^" | "_" | "\\limits" | "\\nolimits")
    }

    pub fn is_expandable(&self, name: &str) -> bool {
        match self.get_macro(name) {
            Some(_) => true,
            None => crate::parse::is_function(name) && !crate::parse::is_primitive(name),
        }
    }
}
