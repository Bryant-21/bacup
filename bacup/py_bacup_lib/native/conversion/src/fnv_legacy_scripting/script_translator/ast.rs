//! FNV script AST types.
//!
//! The tree is deliberately small — only the constructs that actually appear
//! in FNV GECK scripts.  Comments and blank lines are stripped at the lex
//! stage; the AST carries no whitespace.

// ---------------------------------------------------------------------------
// ExprAst
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ExprAst {
    /// Integer constant.
    Int(i64),
    /// Float constant.
    Float(f64),
    /// String literal.
    Str(String),
    /// Variable or bare symbol reference.
    Ident(String),
    /// Function or symbol call with zero or more argument expressions.
    ///
    /// `receiver` is `Some(expr)` when the call was written `receiver.Name(…)`.
    Call {
        receiver: Option<Box<ExprAst>>,
        name: String,
        args: Vec<ExprAst>,
    },
    /// Binary infix operation.
    BinOp {
        op: BinOpKind,
        lhs: Box<ExprAst>,
        rhs: Box<ExprAst>,
    },
    /// Unary prefix `!`.
    Not(Box<ExprAst>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

// ---------------------------------------------------------------------------
// StmtAst (top-level statements)
// ---------------------------------------------------------------------------

/// A single executable statement inside a Begin/End block.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtAst {
    /// `set <target> to <value>`
    Set { target: String, value: ExprAst },
    /// `if … / elseif … / else / endif`
    If {
        cond: ExprAst,
        then_body: Vec<StmtAst>,
        elseif_branches: Vec<(ExprAst, Vec<StmtAst>)>,
        else_body: Option<Vec<StmtAst>>,
    },
    /// `return`
    Return,
    /// A bare expression used as a statement (function call at statement level).
    Expr(ExprAst),
}

// ---------------------------------------------------------------------------
// VarDecl
// ---------------------------------------------------------------------------

/// A script-level variable declaration (`short`, `long`, `int`, `float`, `ref`, `string_var`).
#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    /// Source type keyword (`"short"`, `"long"`, `"int"`, `"float"`, `"ref"`, `"string_var"`).
    pub source_type: String,
    /// Variable name.
    pub name: String,
}

// ---------------------------------------------------------------------------
// BeginBlock
// ---------------------------------------------------------------------------

/// A `begin <EventName> … end` block.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginBlock {
    /// Event name + optional args (e.g. `"OnActivate"`, `"GameMode"`).
    pub event: String,
    pub body: Vec<StmtAst>,
}

// ---------------------------------------------------------------------------
// ScriptAst (root)
// ---------------------------------------------------------------------------

/// Root of an FNV script.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptAst {
    /// Optional `ScriptName`/`scn` declaration.
    pub script_name: Option<String>,
    /// Script-level variable declarations.
    pub vars: Vec<VarDecl>,
    /// `Begin … End` event blocks.
    pub blocks: Vec<BeginBlock>,
}
