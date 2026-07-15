//! Recursive-descent parser — turns a flat token list into a `ScriptAst`.
//!
//! Grammar (informal):
//!
//! ```text
//! script     ::= (scriptname_decl)? (var_decl | begin_block)*
//! scriptname ::= ("ScriptName"|"scn") IDENT NL
//! var_decl   ::= type_kw IDENT NL
//! type_kw    ::= "short"|"long"|"int"|"float"|"ref"|"string_var"
//! begin_block::= "begin" IDENT NL stmt* "end" NL
//! stmt       ::= set_stmt | if_stmt | return_stmt | expr_stmt
//! set_stmt   ::= "set" IDENT "to" expr NL
//! if_stmt    ::= "if" expr NL stmt* (elseif_clause | else_clause)? "endif" NL
//! elseif_clause ::= "elseif" expr NL stmt*
//! else_clause   ::= "else" NL stmt*
//! expr_stmt  ::= expr NL
//! expr       ::= cmp_expr (("&&"|"||") cmp_expr)*
//! cmp_expr   ::= add_expr (("=="|"!="|"<"|">"|"<="|">=") add_expr)?
//! add_expr   ::= mul_expr ("+"|"-") mul_expr)*
//! mul_expr   ::= unary (("*"|"/") unary)*
//! unary      ::= "!" unary | postfix
//! postfix    ::= primary ("." IDENT ("(" args ")")?)* | primary ("(" args ")")?
//! primary    ::= INT | FLOAT | STR | IDENT | "(" expr ")"
//! ```

use super::ast::{BeginBlock, BinOpKind, ExprAst, ScriptAst, StmtAst, VarDecl};
use super::lexer::{KeywordKind, OperatorKind, Span, Token};

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.span {
            Some(s) => write!(f, "parse error at {}..{}: {}", s.start, s.end, self.message),
            None => write!(f, "parse error: {}", self.message),
        }
    }
}
impl std::error::Error for ParseError {}

macro_rules! parse_err {
    ($span:expr, $($t:tt)*) => {
        ParseError { message: format!($($t)*), span: $span }
    };
}

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

struct Parser<'t> {
    tokens: &'t [(Token, Span)],
    pos: usize,
}

impl<'t> Parser<'t> {
    fn new(tokens: &'t [(Token, Span)]) -> Self {
        Parser { tokens, pos: 0 }
    }

    // -----------------------------------------------------------------------
    // Peek / consume helpers
    // -----------------------------------------------------------------------

    fn current(&self) -> &Token {
        &self.tokens[self.pos].0
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].1
    }

    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current(), Token::Newline) {
            self.advance();
        }
    }

    fn expect_newline_or_eof(&mut self) -> Result<(), ParseError> {
        match self.current() {
            Token::Newline | Token::Eof => {
                self.skip_newlines();
                Ok(())
            }
            _ => Err(parse_err!(
                Some(self.current_span()),
                "expected newline, got {:?}",
                self.current()
            )),
        }
    }

    fn expect_keyword(&mut self, kw: &KeywordKind) -> Result<Span, ParseError> {
        let span = self.current_span();
        if let Token::Keyword(k) = self.current() {
            if k == kw {
                self.advance();
                return Ok(span);
            }
        }
        Err(parse_err!(
            Some(span),
            "expected keyword {:?}, got {:?}",
            kw,
            self.current()
        ))
    }

    fn expect_ident(&mut self) -> Result<(String, Span), ParseError> {
        let span = self.current_span();
        if let Token::Ident(name) = self.current().clone() {
            self.advance();
            return Ok((name, span));
        }
        Err(parse_err!(
            Some(span),
            "expected identifier, got {:?}",
            self.current()
        ))
    }

    // -----------------------------------------------------------------------
    // Top-level parse
    // -----------------------------------------------------------------------

    fn parse_script(&mut self) -> Result<ScriptAst, ParseError> {
        self.skip_newlines();

        // Optional scriptname.
        let script_name = self.try_parse_scriptname()?;

        let mut vars: Vec<VarDecl> = Vec::new();
        let mut blocks: Vec<BeginBlock> = Vec::new();

        loop {
            self.skip_newlines();
            match self.current() {
                Token::Eof => break,
                Token::Keyword(KeywordKind::Begin) => {
                    blocks.push(self.parse_begin_block()?);
                }
                Token::Keyword(
                    KeywordKind::Short
                    | KeywordKind::Long
                    | KeywordKind::Int
                    | KeywordKind::Float
                    | KeywordKind::Ref
                    | KeywordKind::StringVar,
                ) => {
                    vars.push(self.parse_var_decl()?);
                }
                _ => {
                    // Bare statements outside a begin block — wrap in GameMode.
                    let body = self.parse_stmts_until_end()?;
                    if !body.is_empty() {
                        blocks.push(BeginBlock {
                            event: "GameMode".into(),
                            body,
                        });
                    }
                    break;
                }
            }
        }

        Ok(ScriptAst {
            script_name,
            vars,
            blocks,
        })
    }

    fn try_parse_scriptname(&mut self) -> Result<Option<String>, ParseError> {
        match self.current() {
            Token::Keyword(KeywordKind::ScriptName | KeywordKind::Scn) => {
                self.advance();
                let (name, _) = self.expect_ident()?;
                self.expect_newline_or_eof()?;
                Ok(Some(name))
            }
            _ => Ok(None),
        }
    }

    fn parse_var_decl(&mut self) -> Result<VarDecl, ParseError> {
        let source_type = match self.current().clone() {
            Token::Keyword(KeywordKind::Short) => "short",
            Token::Keyword(KeywordKind::Long) => "long",
            Token::Keyword(KeywordKind::Int) => "int",
            Token::Keyword(KeywordKind::Float) => "float",
            Token::Keyword(KeywordKind::Ref) => "ref",
            Token::Keyword(KeywordKind::StringVar) => "string_var",
            _ => unreachable!("parse_var_decl called on non-type keyword"),
        }
        .to_string();
        self.advance();
        let (name, _) = self.expect_ident()?;
        self.expect_newline_or_eof()?;
        Ok(VarDecl { source_type, name })
    }

    fn parse_begin_block(&mut self) -> Result<BeginBlock, ParseError> {
        self.expect_keyword(&KeywordKind::Begin)?;
        // Event name is an identifier (e.g. GameMode, OnActivate).
        let (event, _) = self.expect_ident()?;
        self.expect_newline_or_eof()?;
        let body = self.parse_stmts_until_end()?;
        // Consume "end".
        self.expect_keyword(&KeywordKind::End)?;
        self.expect_newline_or_eof()?;
        Ok(BeginBlock { event, body })
    }

    // Parse statements until we see "end" or Eof.
    fn parse_stmts_until_end(&mut self) -> Result<Vec<StmtAst>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            match self.current() {
                Token::Eof => break,
                Token::Keyword(
                    KeywordKind::End | KeywordKind::EndIf | KeywordKind::Else | KeywordKind::ElseIf,
                ) => break,
                _ => stmts.push(self.parse_stmt()?),
            }
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<StmtAst, ParseError> {
        match self.current().clone() {
            Token::Keyword(KeywordKind::Set) => self.parse_set_stmt(),
            Token::Keyword(KeywordKind::If) => self.parse_if_stmt(),
            Token::Keyword(KeywordKind::Return) => {
                self.advance();
                self.expect_newline_or_eof()?;
                Ok(StmtAst::Return)
            }
            // Variable type keywords shouldn't appear inside a block (they'd be
            // at script scope), but accept them gracefully.
            Token::Keyword(
                KeywordKind::Short
                | KeywordKind::Long
                | KeywordKind::Int
                | KeywordKind::Float
                | KeywordKind::Ref
                | KeywordKind::StringVar,
            ) => {
                let decl = self.parse_var_decl()?;
                // Treat as an implicit `set name to 0` placeholder.
                Ok(StmtAst::Set {
                    target: decl.name,
                    value: ExprAst::Int(0),
                })
            }
            _ => {
                let expr = self.parse_expr()?;
                self.expect_newline_or_eof()?;
                Ok(StmtAst::Expr(expr))
            }
        }
    }

    fn parse_set_stmt(&mut self) -> Result<StmtAst, ParseError> {
        self.expect_keyword(&KeywordKind::Set)?;
        let (target, _) = self.expect_ident()?;
        self.expect_keyword(&KeywordKind::To)?;
        let value = self.parse_expr()?;
        self.expect_newline_or_eof()?;
        Ok(StmtAst::Set { target, value })
    }

    fn parse_if_stmt(&mut self) -> Result<StmtAst, ParseError> {
        self.expect_keyword(&KeywordKind::If)?;
        let cond = self.parse_expr()?;
        self.expect_newline_or_eof()?;
        let then_body = self.parse_stmts_until_end()?;

        let mut elseif_branches: Vec<(ExprAst, Vec<StmtAst>)> = Vec::new();
        let mut else_body: Option<Vec<StmtAst>> = None;

        loop {
            self.skip_newlines();
            match self.current().clone() {
                Token::Keyword(KeywordKind::ElseIf) => {
                    self.advance();
                    let elif_cond = self.parse_expr()?;
                    self.expect_newline_or_eof()?;
                    let elif_body = self.parse_stmts_until_end()?;
                    elseif_branches.push((elif_cond, elif_body));
                }
                Token::Keyword(KeywordKind::Else) => {
                    self.advance();
                    self.expect_newline_or_eof()?;
                    else_body = Some(self.parse_stmts_until_end()?);
                    break;
                }
                _ => break,
            }
        }

        self.skip_newlines();
        self.expect_keyword(&KeywordKind::EndIf)?;
        self.expect_newline_or_eof()?;

        Ok(StmtAst::If {
            cond,
            then_body,
            elseif_branches,
            else_body,
        })
    }

    // -----------------------------------------------------------------------
    // Expression parsing (Pratt-style precedence climbing)
    // -----------------------------------------------------------------------

    fn parse_expr(&mut self) -> Result<ExprAst, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<ExprAst, ParseError> {
        let mut lhs = self.parse_and_expr()?;
        while matches!(self.current(), Token::Op(OperatorKind::Pipe2)) {
            self.advance();
            let rhs = self.parse_and_expr()?;
            lhs = ExprAst::BinOp {
                op: BinOpKind::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_and_expr(&mut self) -> Result<ExprAst, ParseError> {
        let mut lhs = self.parse_cmp_expr()?;
        while matches!(self.current(), Token::Op(OperatorKind::Ampersand2)) {
            self.advance();
            let rhs = self.parse_cmp_expr()?;
            lhs = ExprAst::BinOp {
                op: BinOpKind::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_cmp_expr(&mut self) -> Result<ExprAst, ParseError> {
        let lhs = self.parse_add_expr()?;
        let op = match self.current() {
            Token::Op(OperatorKind::EqEq) => BinOpKind::Eq,
            Token::Op(OperatorKind::NotEq) => BinOpKind::NotEq,
            Token::Op(OperatorKind::Lt) => BinOpKind::Lt,
            Token::Op(OperatorKind::Gt) => BinOpKind::Gt,
            Token::Op(OperatorKind::LtEq) => BinOpKind::LtEq,
            Token::Op(OperatorKind::GtEq) => BinOpKind::GtEq,
            // Also accept bare `=` as equality in expression position (FNV quirk).
            Token::Op(OperatorKind::Assign) => BinOpKind::Eq,
            _ => return Ok(lhs),
        };
        self.advance();
        let rhs = self.parse_add_expr()?;
        Ok(ExprAst::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    fn parse_add_expr(&mut self) -> Result<ExprAst, ParseError> {
        let mut lhs = self.parse_mul_expr()?;
        loop {
            let op = match self.current() {
                Token::Op(OperatorKind::Plus) => BinOpKind::Add,
                Token::Op(OperatorKind::Minus) => BinOpKind::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_mul_expr()?;
            lhs = ExprAst::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_mul_expr(&mut self) -> Result<ExprAst, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.current() {
                Token::Op(OperatorKind::Star) => BinOpKind::Mul,
                Token::Op(OperatorKind::Slash) => BinOpKind::Div,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = ExprAst::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<ExprAst, ParseError> {
        if matches!(self.current(), Token::Op(OperatorKind::Bang)) {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(ExprAst::Not(Box::new(inner)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<ExprAst, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.current() {
                // `expr.member` or `expr.method(args)`
                Token::Op(OperatorKind::Dot) => {
                    self.advance();
                    let (member_name, _) = self.expect_ident()?;
                    if matches!(self.current(), Token::Op(OperatorKind::LParen)) {
                        self.advance();
                        let args = self.parse_arg_list()?;
                        expr = ExprAst::Call {
                            receiver: Some(Box::new(expr)),
                            name: member_name,
                            args,
                        };
                    } else {
                        // Bare field / property access — treat as zero-arg call.
                        expr = ExprAst::Call {
                            receiver: Some(Box::new(expr)),
                            name: member_name,
                            args: vec![],
                        };
                    }
                }
                // `expr(args)` — direct call.
                Token::Op(OperatorKind::LParen) => {
                    // Only valid if expr is an Ident at this point.
                    if let ExprAst::Ident(name) = expr {
                        self.advance();
                        let args = self.parse_arg_list()?;
                        expr = ExprAst::Call {
                            receiver: None,
                            name,
                            args,
                        };
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<ExprAst, ParseError> {
        match self.current().clone() {
            Token::IntLit(v) => {
                self.advance();
                Ok(ExprAst::Int(v))
            }
            Token::FloatLit(v) => {
                self.advance();
                Ok(ExprAst::Float(v))
            }
            Token::StringLit(s) => {
                self.advance();
                Ok(ExprAst::Str(s))
            }
            Token::Ident(name) => {
                self.advance();
                // Check if immediately followed by `(` (no-dot call).
                if matches!(self.current(), Token::Op(OperatorKind::LParen)) {
                    self.advance();
                    let args = self.parse_arg_list()?;
                    Ok(ExprAst::Call {
                        receiver: None,
                        name,
                        args,
                    })
                } else {
                    Ok(ExprAst::Ident(name))
                }
            }
            // Some keywords can appear as identifiers in expression position
            // (e.g. GetPlayer used as a value).
            Token::Keyword(_) => {
                // Extract text from ident-like keywords — map them to ident.
                let name = format!("{:?}", self.current());
                let span = self.current_span();
                // Try to get the keyword name directly.
                if let Token::Keyword(kw) = self.current().clone() {
                    let kw_name = keyword_name(&kw);
                    self.advance();
                    if matches!(self.current(), Token::Op(OperatorKind::LParen)) {
                        self.advance();
                        let args = self.parse_arg_list()?;
                        return Ok(ExprAst::Call {
                            receiver: None,
                            name: kw_name.to_string(),
                            args,
                        });
                    }
                    return Ok(ExprAst::Ident(kw_name.to_string()));
                }
                Err(parse_err!(
                    Some(span),
                    "unexpected token in expression: {:?}",
                    name
                ))
            }
            Token::Op(OperatorKind::LParen) => {
                self.advance();
                let inner = self.parse_expr()?;
                if !matches!(self.current(), Token::Op(OperatorKind::RParen)) {
                    return Err(parse_err!(Some(self.current_span()), "expected ')'"));
                }
                self.advance();
                Ok(inner)
            }
            other => Err(parse_err!(
                Some(self.current_span()),
                "unexpected token {:?} in expression",
                other
            )),
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<ExprAst>, ParseError> {
        let mut args = Vec::new();
        while !matches!(self.current(), Token::Op(OperatorKind::RParen) | Token::Eof) {
            args.push(self.parse_expr()?);
            if matches!(self.current(), Token::Op(OperatorKind::Comma)) {
                self.advance();
            }
        }
        if matches!(self.current(), Token::Op(OperatorKind::RParen)) {
            self.advance();
        }
        Ok(args)
    }
}

/// Return the canonical lower-case spelling of a keyword (used when a keyword
/// appears in identifier position, e.g. `player` or `playerref`).
fn keyword_name(kw: &KeywordKind) -> &'static str {
    match kw {
        KeywordKind::Set => "set",
        KeywordKind::To => "to",
        KeywordKind::If => "if",
        KeywordKind::Else => "else",
        KeywordKind::ElseIf => "elseif",
        KeywordKind::EndIf => "endif",
        KeywordKind::Begin => "begin",
        KeywordKind::End => "end",
        KeywordKind::Return => "return",
        KeywordKind::ScriptName => "scriptname",
        KeywordKind::Scn => "scn",
        KeywordKind::Short => "short",
        KeywordKind::Long => "long",
        KeywordKind::Int => "int",
        KeywordKind::Float => "float",
        KeywordKind::Ref => "ref",
        KeywordKind::StringVar => "string_var",
        KeywordKind::Equals => "equals",
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a flat token stream (produced by `tokenize`) into a `ScriptAst`.
pub fn parse(tokens: Vec<(Token, Span)>) -> Result<ScriptAst, ParseError> {
    let mut p = Parser::new(&tokens);
    p.parse_script()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fnv_legacy_scripting::script_translator::lexer::tokenize;

    fn parse_src(src: &str) -> ScriptAst {
        let tokens = tokenize(src).expect("lex ok");
        parse(tokens).expect("parse ok")
    }

    #[test]
    fn parse_empty_begin_block() {
        let ast = parse_src("begin GameMode\nend\n");
        assert_eq!(ast.blocks.len(), 1);
        assert_eq!(ast.blocks[0].event, "GameMode");
        assert!(ast.blocks[0].body.is_empty());
    }

    #[test]
    fn parse_set_statement() {
        let ast = parse_src("begin GameMode\nset Speed to 100\nend\n");
        assert_eq!(ast.blocks[0].body.len(), 1);
        assert!(matches!(
            &ast.blocks[0].body[0],
            StmtAst::Set { target, value: ExprAst::Int(100) }
            if target == "Speed"
        ));
    }

    #[test]
    fn parse_scriptname() {
        let ast = parse_src("ScriptName MyScript\nbegin GameMode\nend\n");
        assert_eq!(ast.script_name.as_deref(), Some("MyScript"));
    }

    #[test]
    fn parse_var_decl() {
        let ast = parse_src("short myVar\nbegin GameMode\nend\n");
        assert_eq!(ast.vars.len(), 1);
        assert_eq!(ast.vars[0].source_type, "short");
        assert_eq!(ast.vars[0].name, "myVar");
    }

    #[test]
    fn parse_if_stmt() {
        let src = "begin GameMode\nif x == 1\nset y to 2\nendif\nend\n";
        let ast = parse_src(src);
        assert!(matches!(&ast.blocks[0].body[0], StmtAst::If { .. }));
    }

    #[test]
    fn parse_return_stmt() {
        let ast = parse_src("begin GameMode\nreturn\nend\n");
        assert!(matches!(&ast.blocks[0].body[0], StmtAst::Return));
    }

    #[test]
    fn parse_function_call_as_expr_stmt() {
        let ast = parse_src("begin GameMode\nDisable()\nend\n");
        assert!(matches!(
            &ast.blocks[0].body[0],
            StmtAst::Expr(ExprAst::Call { .. })
        ));
    }

    #[test]
    fn parse_multiple_blocks() {
        let src = "begin GameMode\nend\nbegin OnActivate\nend\n";
        let ast = parse_src(src);
        assert_eq!(ast.blocks.len(), 2);
        assert_eq!(ast.blocks[0].event, "GameMode");
        assert_eq!(ast.blocks[1].event, "OnActivate");
    }
}
