//! Port of `office/web/src/mathTree.ts` - the WYSIWYG math expression tree (MathQuill/Mathcha
//! style). A document is a row of nodes; structures hold sub-rows. This module ports the model, the
//! tree → LaTeX serialiser ([`ser`]) and the tolerant LaTeX → tree parser ([`parse`]), which round-
//! trip the subset the editor emits and degrade unknown commands to literal atoms (never panics).
//!
//! The cursor-based in-place edit operations of `mathTree.ts` (`insertNode`, `backspace`, `makeFrac`
//! …) rely on JavaScript reference identity (the cursor holds a live reference to a sub-row); they
//! belong to the desktop editor layer with a path-addressed cursor and are not ported here.

/// A row of nodes (`mathTree.ts` `MRow`).
pub type MRow = Vec<MNode>;

/// A math node (`mathTree.ts` `MNode`).
#[derive(Clone, Debug, PartialEq)]
pub enum MNode {
    /// `{ k:'atom', t }` - a literal token: `x`, `1`, `+`, `\alpha`, `\pi`, …
    Atom(String),
    /// `{ k:'frac', n, d }`
    Frac { n: MRow, d: MRow },
    /// `{ k:'sqrt', b }`
    Sqrt { b: MRow },
    /// `{ k:'root', i, b }`
    Root { i: MRow, b: MRow },
    /// `{ k:'script', base, sup, sub }`
    Script { base: Box<MNode>, sup: Option<MRow>, sub: Option<MRow> },
    /// `{ k:'delim', o, c, b }`
    Delim { o: String, c: String, b: MRow },
    /// `{ k:'big', op, lo, hi }`
    Big { op: String, lo: Option<MRow>, hi: Option<MRow> },
    /// `{ k:'acc', cmd, b }`
    Acc { cmd: String, b: MRow },
    /// `{ k:'mat', env, rows }`
    Mat { env: String, rows: Vec<Vec<MRow>> },
    /// `{ k:'raw', t }`
    Raw(String),
}

const ACCENTS: &[&str] = &[
    "\\hat", "\\vec", "\\bar", "\\tilde", "\\overline", "\\underline", "\\widehat", "\\widetilde",
    "\\dot", "\\ddot", "\\dddot", "\\check", "\\breve", "\\acute", "\\grave", "\\mathring",
    "\\overrightarrow", "\\overleftarrow",
];
const BIGOPS: &[&str] = &[
    "\\sum", "\\prod", "\\int", "\\oint", "\\iint", "\\iiint", "\\bigcup", "\\bigcap", "\\bigsqcup",
    "\\bigvee", "\\bigwedge", "\\bigodot", "\\bigotimes", "\\bigoplus", "\\lim", "\\limsup",
    "\\liminf", "\\coprod",
];
const MAT_ENVS: &[&str] = &[
    "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "smallmatrix", "cases",
    "aligned", "gathered", "array",
];

/// `true` for a control word like `\alpha` (backslash + one or more letters), which needs a
/// trailing space when serialised so `\pi x` does not become `\pix`.
fn is_control_word(t: &str) -> bool {
    t.len() > 1 && t.starts_with('\\') && t[1..].chars().all(|c| c.is_ascii_alphabetic())
}

/// Serialise a row to LaTeX (`mathTree.ts` `ser`).
pub fn ser(row: &MRow) -> String {
    row.iter().map(ser_node).collect::<Vec<_>>().join("")
}

fn ser_node(n: &MNode) -> String {
    match n {
        MNode::Atom(t) => {
            if is_control_word(t) {
                format!("{t} ")
            } else {
                t.clone()
            }
        }
        MNode::Raw(t) => t.clone(),
        MNode::Frac { n, d } => format!("\\frac{{{}}}{{{}}}", ser(n), ser(d)),
        MNode::Sqrt { b } => format!("\\sqrt{{{}}}", ser(b)),
        MNode::Root { i, b } => format!("\\sqrt[{}]{{{}}}", ser(i), ser(b)),
        MNode::Script { base, sup, sub } => {
            let mut s = ser_node(base);
            if let Some(sub) = sub {
                s += &format!("_{{{}}}", ser(sub));
            }
            if let Some(sup) = sup {
                s += &format!("^{{{}}}", ser(sup));
            }
            s
        }
        MNode::Delim { o, c, b } => format!("\\left{} {} \\right{}", o, ser(b), c),
        MNode::Big { op, lo, hi } => {
            let mut s = format!("{op} ");
            if let Some(lo) = lo {
                s += &format!("_{{{}}}", ser(lo));
            }
            if let Some(hi) = hi {
                s += &format!("^{{{}}}", ser(hi));
            }
            s
        }
        MNode::Acc { cmd, b } => format!("{cmd}{{{}}}", ser(b)),
        MNode::Mat { env, rows } => {
            let body = rows
                .iter()
                .map(|r| r.iter().map(ser).collect::<Vec<_>>().join(" & "))
                .collect::<Vec<_>>()
                .join(" \\\\ ");
            format!("\\begin{{{env}}}{body}\\end{{{env}}}")
        }
    }
}

/// Tokenise LaTeX into control words / single characters, dropping whitespace (`mathTree.ts`
/// `tokenize`).
fn tokenize(src: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            // `\\([a-zA-Z]+|.)` - a run of letters, else the single next character.
            if i + 1 < chars.len() {
                if chars[i + 1].is_ascii_alphabetic() {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                    toks.push(format!("\\{}", chars[i + 1..j].iter().collect::<String>()));
                    i = j;
                } else {
                    toks.push(format!("\\{}", chars[i + 1]));
                    i += 2;
                }
                continue;
            }
            // a trailing lone backslash: no match, fall through to skip? JS leaves it (regex fails),
            // then the generic branch pushes it as a char.
        }
        if c == ' ' || c == '\n' || c == '\t' {
            i += 1;
            continue;
        }
        toks.push(c.to_string());
        i += 1;
    }
    toks
}

struct Parser {
    toks: Vec<String>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.p).map(|s| s.as_str())
    }
    fn eat(&mut self) -> Option<String> {
        let t = self.toks.get(self.p).cloned();
        if t.is_some() {
            self.p += 1;
        }
        t
    }

    fn parse_group(&mut self) -> MRow {
        if self.peek() == Some("{") {
            self.eat();
            let r = self.parse_row(&["}"]);
            if self.peek() == Some("}") {
                self.eat();
            }
            return r;
        }
        if self.peek().is_none() {
            return Vec::new();
        }
        vec![self.parse_atom_or_struct()]
    }

    fn parse_optional(&mut self) -> Option<MRow> {
        if self.peek() != Some("[") {
            return None;
        }
        self.eat();
        let r = self.parse_row(&["]"]);
        if self.peek() == Some("]") {
            self.eat();
        }
        Some(r)
    }

    fn attach_scripts(&mut self, base: MNode) -> MNode {
        let mut sup: Option<MRow> = None;
        let mut sub: Option<MRow> = None;
        while matches!(self.peek(), Some("^") | Some("_")) {
            let op = self.eat().unwrap();
            let g = self.parse_group();
            if op == "^" {
                sup = Some(g);
            } else {
                sub = Some(g);
            }
        }
        if sup.is_some() || sub.is_some() {
            MNode::Script { base: Box::new(base), sup, sub }
        } else {
            base
        }
    }

    fn parse_atom_or_struct(&mut self) -> MNode {
        let tk = self.eat().unwrap_or_default();
        match tk.as_str() {
            "\\frac" | "\\dfrac" | "\\tfrac" => {
                let n = self.parse_group();
                let d = self.parse_group();
                MNode::Frac { n, d }
            }
            "\\sqrt" => {
                let idx = self.parse_optional();
                let b = self.parse_group();
                match idx {
                    Some(i) => MNode::Root { i, b },
                    None => MNode::Sqrt { b },
                }
            }
            "\\left" => {
                let o = self.eat().unwrap_or_else(|| "(".into());
                let b = self.parse_row(&["\\right"]);
                if self.peek() == Some("\\right") {
                    self.eat();
                }
                let c = self.eat().unwrap_or_else(|| ")".into());
                MNode::Delim {
                    o: if o == "." { String::new() } else { o },
                    c: if c == "." { String::new() } else { c },
                    b,
                }
            }
            "\\begin" => {
                let mut env = String::new();
                if self.peek() == Some("{") {
                    self.eat();
                    while self.peek().is_some() && self.peek() != Some("}") {
                        env += &self.eat().unwrap();
                    }
                    if self.peek() == Some("}") {
                        self.eat();
                    }
                }
                self.parse_matrix(&env)
            }
            t if ACCENTS.contains(&t) => {
                let b = self.parse_group();
                MNode::Acc { cmd: t.to_string(), b }
            }
            t if BIGOPS.contains(&t) => {
                let mut lo: Option<MRow> = None;
                let mut hi: Option<MRow> = None;
                while matches!(self.peek(), Some("^") | Some("_")) {
                    let op = self.eat().unwrap();
                    let g = self.parse_group();
                    if op == "^" {
                        hi = Some(g);
                    } else {
                        lo = Some(g);
                    }
                }
                MNode::Big { op: t.to_string(), lo, hi }
            }
            "{" => {
                let mut r = self.parse_row(&["}"]);
                if self.peek() == Some("}") {
                    self.eat();
                }
                if r.len() == 1 {
                    r.remove(0)
                } else {
                    MNode::Delim { o: String::new(), c: String::new(), b: r }
                }
            }
            _ => MNode::Atom(tk),
        }
    }

    fn parse_matrix(&mut self, env: &str) -> MNode {
        let mut rows: Vec<Vec<MRow>> = vec![Vec::new()];
        let mut cur: MRow = Vec::new();
        macro_rules! push {
            () => {{
                let last = rows.len() - 1;
                rows[last].push(std::mem::take(&mut cur));
            }};
        }
        while self.peek().is_some() && self.peek() != Some("\\end") {
            match self.peek() {
                Some("&") => {
                    self.eat();
                    push!();
                }
                Some("\\\\") => {
                    self.eat();
                    push!();
                    rows.push(Vec::new());
                }
                _ => cur.push(self.parse_scripted()),
            }
        }
        push!();
        if self.peek() == Some("\\end") {
            self.eat();
            if self.peek() == Some("{") {
                self.eat();
                while self.peek().is_some() && self.peek() != Some("}") {
                    self.eat();
                }
                if self.peek() == Some("}") {
                    self.eat();
                }
            }
        }
        MNode::Mat {
            env: if MAT_ENVS.contains(&env) { env.to_string() } else { "matrix".to_string() },
            rows,
        }
    }

    fn parse_scripted(&mut self) -> MNode {
        let base = self.parse_atom_or_struct();
        self.attach_scripts(base)
    }

    fn parse_row(&mut self, stop: &[&str]) -> MRow {
        let mut row: MRow = Vec::new();
        while let Some(tk) = self.peek() {
            if stop.contains(&tk) {
                break;
            }
            if tk == "&" || tk == "\\\\" {
                break;
            }
            row.push(self.parse_scripted());
        }
        row
    }
}

/// Parse LaTeX into a tree (`mathTree.ts` `parse`). Tolerant: unknown commands become literal atoms.
pub fn parse(src: &str) -> MRow {
    let toks = tokenize(src.trim());
    let mut parser = Parser { toks, p: 0 };
    parser.parse_row(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for src in [
            "x+1",
            "\\frac{a}{b}",
            "\\sqrt{x}",
            "\\sqrt[3]{x}",
            "x^{2}",
            "\\sum _{i}^{n}",
            "\\left( x \\right)",
            "\\hat{x}",
        ] {
            let tree = parse(src);
            // ser(parse(src)) parses again to the same tree (idempotent round-trip).
            assert_eq!(parse(&ser(&tree)), tree, "round-trip of {src:?}");
        }
    }

    #[test]
    fn parses_structures() {
        assert_eq!(parse("a"), vec![MNode::Atom("a".into())]);
        assert!(matches!(parse("\\frac{1}{2}")[0], MNode::Frac { .. }));
        assert!(matches!(parse("\\sqrt[3]{x}")[0], MNode::Root { .. }));
        let m = parse("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}");
        match &m[0] {
            MNode::Mat { env, rows } => {
                assert_eq!(env, "pmatrix");
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
            }
            _ => panic!("expected matrix"),
        }
    }

    #[test]
    fn control_word_spacing() {
        assert_eq!(ser(&vec![MNode::Atom("\\pi".into()), MNode::Atom("x".into())]), "\\pi x");
    }
}
