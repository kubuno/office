//! The tokenizer and the recursive-descent parser of `formula-engine.ts`, rule for rule.
//!
//! Precedence, loosest first: comparison (`= <> < > <= >=`), concatenation (`&`), `+ -`, `* /`, `^`
//! (left-associative, as on the web), unary `- +`, postfix `%` and application `(…)`, primaries. Arguments
//! and array columns are separated by `,`, array rows by `;` (`;` also separates arguments).

use super::ast::{BinOp, Node};
use super::value::ErrCode;
use crate::js;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Bool(bool),
    Ref(String),
    Sheet(String),
    Name(String),
    Op(&'static str),
    Lp,
    Rp,
    Comma,
    Semi,
    LBrace,
    RBrace,
    Hash,
    Colon,
}

fn is_word_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '$' || c == '_'
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '$' || c == '_' || c == '.'
}

/// `/^[A-Za-z]+[0-9]+$/`
fn looks_like_ref(s: &str) -> bool {
    let b = s.as_bytes();
    let letters = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    letters > 0 && letters < b.len() && b[letters..].iter().all(|c| c.is_ascii_digit())
}

fn tokenize(src: &str) -> Result<Vec<Tok>, ErrCode> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == ' ' || c == '\t' || c == '\n' {
            i += 1;
            continue;
        }
        if c == '"' {
            // The web's loop stops at the first `"`: a doubled quote closes the string (its `""` branch
            // is unreachable), so `"a""b"` is two strings.
            let mut s = String::new();
            i += 1;
            while i < n && chars[i] != '"' {
                s.push(chars[i]);
                i += 1;
            }
            if i >= n {
                return Err(ErrCode::VALUE);
            }
            i += 1;
            toks.push(Tok::Str(s));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let mut j = i + 1;
            while j < n && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            if j < n && (chars[j] == 'e' || chars[j] == 'E') {
                j += 1;
                if j < n && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                while j < n && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let text: String = chars[i..j].iter().collect();
            toks.push(Tok::Num(js::parse_float(&text)));
            i = j;
            continue;
        }
        if c == '\'' {
            let mut s = String::new();
            i += 1;
            while i < n && chars[i] != '\'' {
                s.push(chars[i]);
                i += 1;
            }
            if i >= n {
                return Err(ErrCode::VALUE);
            }
            i += 1;
            if chars.get(i) == Some(&'!') {
                i += 1;
                toks.push(Tok::Sheet(s));
                continue;
            }
            return Err(ErrCode::VALUE);
        }
        if is_word_start(c) {
            let mut j = i;
            while j < n && is_word(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            i = j;
            if chars.get(i) == Some(&'!') {
                i += 1;
                toks.push(Tok::Sheet(word.replace('$', "")));
                continue;
            }
            let bare = word.replace('$', "");
            if looks_like_ref(&bare) {
                toks.push(Tok::Ref(bare.to_ascii_uppercase()));
            } else if bare.eq_ignore_ascii_case("true") {
                toks.push(Tok::Bool(true));
            } else if bare.eq_ignore_ascii_case("false") {
                toks.push(Tok::Bool(false));
            } else {
                toks.push(Tok::Name(bare.to_uppercase()));
            }
            continue;
        }
        let next = chars.get(i + 1).copied();
        if c == '<' && next == Some('=') {
            toks.push(Tok::Op("<="));
            i += 2;
            continue;
        }
        if c == '>' && next == Some('=') {
            toks.push(Tok::Op(">="));
            i += 2;
            continue;
        }
        if c == '<' && next == Some('>') {
            toks.push(Tok::Op("<>"));
            i += 2;
            continue;
        }
        let op = match c {
            '+' => Some("+"),
            '-' => Some("-"),
            '*' => Some("*"),
            '/' => Some("/"),
            '^' => Some("^"),
            '&' => Some("&"),
            '=' => Some("="),
            '<' => Some("<"),
            '>' => Some(">"),
            '%' => Some("%"),
            _ => None,
        };
        if let Some(op) = op {
            toks.push(Tok::Op(op));
            i += 1;
            continue;
        }
        let t = match c {
            '(' => Tok::Lp,
            ')' => Tok::Rp,
            ',' => Tok::Comma,
            ';' => Tok::Semi,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '#' => Tok::Hash,
            ':' => Tok::Colon,
            _ => return Err(ErrCode::VALUE),
        };
        toks.push(t);
        i += 1;
    }
    Ok(toks)
}

struct Parser {
    toks: Vec<Tok>,
    i: usize,
}

type PResult = Result<Node, ErrCode>;

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn peek_op(&self, ops: &[&str]) -> Option<&'static str> {
        match self.peek() {
            Some(Tok::Op(o)) if ops.contains(o) => Some(o),
            _ => None,
        }
    }

    fn parse_compare(&mut self) -> PResult {
        let mut l = self.parse_concat()?;
        while let Some(op) = self.peek_op(&["=", "<>", "<", ">", "<=", ">="]) {
            self.next();
            let r = self.parse_concat()?;
            let op = match op {
                "=" => BinOp::Eq,
                "<>" => BinOp::Ne,
                "<" => BinOp::Lt,
                ">" => BinOp::Gt,
                "<=" => BinOp::Le,
                _ => BinOp::Ge,
            };
            l = Node::Bin { op, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }
    fn parse_concat(&mut self) -> PResult {
        let mut l = self.parse_add()?;
        while self.peek_op(&["&"]).is_some() {
            self.next();
            let r = self.parse_add()?;
            l = Node::Bin { op: BinOp::Concat, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }
    fn parse_add(&mut self) -> PResult {
        let mut l = self.parse_mul()?;
        while let Some(op) = self.peek_op(&["+", "-"]) {
            self.next();
            let r = self.parse_mul()?;
            l = Node::Bin { op: if op == "+" { BinOp::Add } else { BinOp::Sub }, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }
    fn parse_mul(&mut self) -> PResult {
        let mut l = self.parse_pow()?;
        while let Some(op) = self.peek_op(&["*", "/"]) {
            self.next();
            let r = self.parse_pow()?;
            l = Node::Bin { op: if op == "*" { BinOp::Mul } else { BinOp::Div }, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }
    fn parse_pow(&mut self) -> PResult {
        let mut l = self.parse_unary()?;
        while self.peek_op(&["^"]).is_some() {
            self.next();
            let r = self.parse_unary()?;
            l = Node::Bin { op: BinOp::Pow, l: Box::new(l), r: Box::new(r) };
        }
        Ok(l)
    }
    fn parse_unary(&mut self) -> PResult {
        if let Some(op) = self.peek_op(&["-", "+"]) {
            self.next();
            let e = self.parse_unary()?;
            return Ok(Node::Unary { op: if op == "-" { '-' } else { '+' }, e: Box::new(e) });
        }
        self.parse_postfix()
    }
    fn parse_postfix(&mut self) -> PResult {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Op("%")) => {
                    self.next();
                    e = Node::Postfix { op: '%', e: Box::new(e) };
                }
                Some(Tok::Lp) => {
                    self.next();
                    let args = self.parse_arg_list()?;
                    e = Node::Apply { f: Box::new(e), args };
                }
                _ => break,
            }
        }
        Ok(e)
    }
    fn parse_arg_list(&mut self) -> Result<Vec<Node>, ErrCode> {
        let mut args = Vec::new();
        if self.peek() != Some(&Tok::Rp) {
            args.push(self.parse_compare()?);
            while matches!(self.peek(), Some(Tok::Comma) | Some(Tok::Semi)) {
                self.next();
                args.push(self.parse_compare()?);
            }
        }
        if self.next() != Some(Tok::Rp) {
            return Err(ErrCode::VALUE);
        }
        Ok(args)
    }
    fn parse_primary(&mut self) -> PResult {
        let tk = self.next().ok_or(ErrCode::VALUE)?;
        match tk {
            Tok::Num(v) => Ok(Node::Num(v)),
            Tok::Str(s) => Ok(Node::Str(s)),
            Tok::Bool(b) => Ok(Node::Bool(b)),
            Tok::Ref(a) => {
                if self.peek() == Some(&Tok::Colon) {
                    self.next();
                    return match self.next() {
                        Some(Tok::Ref(b)) => Ok(Node::Range { a, b, sheet: None }),
                        _ => Err(ErrCode::REF),
                    };
                }
                if self.peek() == Some(&Tok::Hash) {
                    self.next();
                    return Ok(Node::SpillRef(a));
                }
                Ok(Node::Ref { r: a, sheet: None })
            }
            Tok::Sheet(sheet) => {
                let a = match self.next() {
                    Some(Tok::Ref(a)) => a,
                    _ => return Err(ErrCode::REF),
                };
                if self.peek() == Some(&Tok::Colon) {
                    self.next();
                    return match self.next() {
                        Some(Tok::Ref(b)) => Ok(Node::Range { a, b, sheet: Some(sheet) }),
                        _ => Err(ErrCode::REF),
                    };
                }
                Ok(Node::Ref { r: a, sheet: Some(sheet) })
            }
            Tok::Name(name) => {
                if self.peek() == Some(&Tok::Lp) {
                    self.next();
                    let args = self.parse_arg_list()?;
                    return Ok(Node::Call { name, args });
                }
                Ok(Node::Var(name))
            }
            Tok::Lp => {
                let e = self.parse_compare()?;
                if self.next() != Some(Tok::Rp) {
                    return Err(ErrCode::VALUE);
                }
                Ok(e)
            }
            Tok::LBrace => {
                let mut rows = Vec::new();
                let mut row = Vec::new();
                if self.peek() != Some(&Tok::RBrace) {
                    row.push(self.parse_compare()?);
                    loop {
                        match self.peek() {
                            Some(Tok::Comma) => {
                                self.next();
                                row.push(self.parse_compare()?);
                            }
                            Some(Tok::Semi) => {
                                self.next();
                                rows.push(std::mem::take(&mut row));
                                row.push(self.parse_compare()?);
                            }
                            _ => break,
                        }
                    }
                }
                rows.push(row);
                if self.next() != Some(Tok::RBrace) {
                    return Err(ErrCode::VALUE);
                }
                Ok(Node::Array(rows))
            }
            _ => Err(ErrCode::VALUE),
        }
    }
}

/// `parseFormula`: tokenises and parses a formula (with or without its leading `=`). An empty body
/// is the empty string.
pub fn parse_formula(formula: &str) -> Result<Node, ErrCode> {
    let body = formula.strip_prefix('=').unwrap_or(formula);
    let toks = tokenize(body)?;
    if toks.is_empty() {
        return Ok(Node::Str(String::new()));
    }
    let mut p = Parser { toks, i: 0 };
    let ast = p.parse_compare()?;
    if p.peek().is_some() {
        return Err(ErrCode::VALUE);
    }
    Ok(ast)
}

/// `validateNameFormula`: the error text of a defined name's definition, `None` when it parses.
pub fn validate_name_formula(formula: &str) -> Option<String> {
    parse_formula(formula).err().map(|e| e.as_str().to_string())
}

/// `isValidDefinedName`: a letter or `_` first, then letters, digits, `_` and `.`; not a cell
/// reference (1–3 letters + 1–7 digits) and not TRUE/FALSE.
pub fn is_valid_defined_name(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || !(b[0].is_ascii_alphabetic() || b[0] == b'_') {
        return false;
    }
    if !b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'.') {
        return false;
    }
    let letters = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = b.len() - letters;
    if (1..=3).contains(&letters) && (1..=7).contains(&digits) && b[letters..].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    !(name.eq_ignore_ascii_case("true") || name.eq_ignore_ascii_case("false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Node {
        parse_formula(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn precedence_matches_the_web() {
        // ^ binds tighter than unary minus on the right but unary is parsed below ^: -2^2 = (-2)^2.
        assert_eq!(p("=-2^2"), Node::Bin { op: BinOp::Pow, l: Box::new(Node::Unary { op: '-', e: Box::new(Node::Num(2.0)) }), r: Box::new(Node::Num(2.0)) });
        // 2^3^2 is left-associative on the web.
        assert!(matches!(p("=2^3^2"), Node::Bin { op: BinOp::Pow, l, .. } if matches!(*l, Node::Bin { op: BinOp::Pow, .. })));
        assert!(matches!(p("=1+2&3"), Node::Bin { op: BinOp::Concat, .. }));
        assert!(matches!(p("=1&2=12"), Node::Bin { op: BinOp::Eq, .. }));
    }

    #[test]
    fn references_and_names() {
        assert_eq!(p("=$a$1"), Node::Ref { r: "A1".into(), sheet: None });
        assert_eq!(p("='My Sheet'!B2:c3"), Node::Range { a: "B2".into(), b: "C3".into(), sheet: Some("My Sheet".into()) });
        assert_eq!(p("=Feuil1!A1"), Node::Ref { r: "A1".into(), sheet: Some("Feuil1".into()) });
        assert_eq!(p("=A1#"), Node::SpillRef("A1".into()));
        assert_eq!(p("=total"), Node::Var("TOTAL".into()));
        assert!(matches!(p("=sum(1;2)"), Node::Call { ref name, ref args } if name == "SUM" && args.len() == 2));
        assert!(matches!(p("=LAMBDA(x,x+1)(5)"), Node::Apply { .. }));
        assert_eq!(p("={1,2;3}"), Node::Array(vec![vec![Node::Num(1.0), Node::Num(2.0)], vec![Node::Num(3.0)]]));
    }

    #[test]
    fn quirks_are_kept() {
        // "1.2.3" reads as 1.2 then the tokenizer moves past the whole run.
        assert_eq!(p("=1.2.3"), Node::Num(1.2));
        // A doubled quote ends the string: two strings in a row are a syntax error.
        assert_eq!(parse_formula("=\"a\"\"b\""), Err(ErrCode::VALUE));
        assert_eq!(parse_formula("="), Ok(Node::Str(String::new())));
        assert_eq!(parse_formula("=A1:"), Err(ErrCode::REF));
        assert_eq!(parse_formula("=é"), Err(ErrCode::VALUE));
    }

    #[test]
    fn defined_names() {
        assert!(is_valid_defined_name("Taux_TVA"));
        assert!(!is_valid_defined_name("AB12"));
        assert!(is_valid_defined_name("ABCD12"));
        assert!(!is_valid_defined_name("true"));
        assert!(!is_valid_defined_name("1abc"));
    }
}
