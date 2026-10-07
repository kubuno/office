//! The formula syntax tree (`formula-engine.ts` `Node`).

/// One node of a parsed formula.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Num(f64),
    Str(String),
    Bool(bool),
    /// `A1`, optionally sheet-qualified (`'Sheet 2'!A1`); the reference is upper case, without `$`.
    Ref { r: String, sheet: Option<String> },
    /// `A1:C10`, optionally sheet-qualified.
    Range { a: String, b: String, sheet: Option<String> },
    Unary { op: char, e: Box<Node> },
    /// `%` after an expression.
    Postfix { op: char, e: Box<Node> },
    Bin { op: BinOp, l: Box<Node>, r: Box<Node> },
    /// A function call by (upper-case) name.
    Call { name: String, args: Vec<Node> },
    /// `{1,2;3,4}`.
    Array(Vec<Vec<Node>>),
    /// `A1#`: the whole array spilled from the anchor.
    SpillRef(String),
    /// A bare name: a LET/LAMBDA variable or a workbook defined name.
    Var(String),
    /// Applying a value to arguments: `LAMBDA(x, x+1)(5)`.
    Apply { f: Box<Node>, args: Vec<Node> },
}

/// The binary operators, in the web's spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::Concat => "&",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
        }
    }
    pub fn is_comparison(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge)
    }
}

impl Node {
    /// `{ k: 'var' }` — the parameter names of LAMBDA and LET must be bare names.
    pub fn var_name(&self) -> Option<&str> {
        match self {
            Node::Var(n) => Some(n),
            _ => None,
        }
    }
}
