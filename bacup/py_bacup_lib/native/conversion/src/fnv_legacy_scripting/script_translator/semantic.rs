//! Semantic transform — rewrites FNV AST nodes to FO4/Papyrus equivalents.
//!
//! Strategy:
//! - Walk the AST recursively.
//! - For every `ExprAst::Call { name, args }`, look up `name` (case-insensitive)
//!   in `ctx.function_map`.  If found, emit a `SemanticCall` annotation that
//!   the emitter uses to build the Papyrus template.
//! - For every arg whose kind is `"actor_value"`, replace the `Ident` string
//!   using `ctx.actor_value_map`.
//! - Functions not in the map are left as-is unless `strict` is requested.

use super::ast::{BeginBlock, BinOpKind, ExprAst, ScriptAst, StmtAst};
use crate::fnv_legacy_scripting::function_map::{FnvScriptContext, FunctionEntry};

// ---------------------------------------------------------------------------
// SemanticError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticErrorKind {
    UnmappedFunction,
    UnmappedActorValue,
    DroppedFunction,
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            SemanticErrorKind::UnmappedFunction => {
                write!(f, "unmapped function '{}'", self.name)
            }
            SemanticErrorKind::UnmappedActorValue => {
                write!(f, "unmapped actor_value '{}'", self.name)
            }
            SemanticErrorKind::DroppedFunction => {
                write!(
                    f,
                    "script translation drop: function '{}' has no FO4 equivalent",
                    self.name
                )
            }
        }
    }
}
impl std::error::Error for SemanticError {}

// ---------------------------------------------------------------------------
// Annotated expression — post-semantic
// ---------------------------------------------------------------------------

/// A call that has been resolved through the function map.
///
/// The `template` field holds the raw Papyrus template string from the YAML
/// (e.g. `"{self}.GetValue({arg0})"`).  The emitter substitutes `{self}`,
/// `{arg0}`, … at emit time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCall {
    /// Original FNV function name (for diagnostics).
    pub fnv_name: String,
    /// Optional receiver expression (left of `.`).
    pub receiver: Option<Box<ExprAst>>,
    /// Argument expressions (post-AV-remap).
    pub args: Vec<ExprAst>,
    /// Papyrus template string.
    pub template: String,
}

// The semantic pass has no dedicated AST variant for a resolved call; it
// encodes the rendered Papyrus template as an `ExprAst::Str` carrying the
// `RENDERED_PREFIX` sentinel, which the emitter strips.
//
// TODO: introduce a dedicated `SemanticExprAst` variant carrying `ResolvedCall`
// without string encoding.

/// Marker prefix written into a `StringLit` to signal a pre-rendered Papyrus
/// fragment produced by the semantic pass.
pub(super) const RENDERED_PREFIX: &str = "\x00papyrus\x00";

// ---------------------------------------------------------------------------
// apply_semantic
// ---------------------------------------------------------------------------

/// Walk `ast`, substituting FNV names with FO4/Papyrus equivalents.
///
/// Errors are returned eagerly on the first unmapped name.  In non-strict mode
/// (`ctx` doesn't expose a strict flag at this level), unknown functions are
/// left untouched as bare `Ident` references.
pub fn apply_semantic(ast: ScriptAst, ctx: &FnvScriptContext) -> Result<ScriptAst, SemanticError> {
    let blocks = ast
        .blocks
        .into_iter()
        .map(|b| transform_block(b, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ScriptAst {
        script_name: ast.script_name,
        vars: ast.vars,
        blocks,
    })
}

fn transform_block(block: BeginBlock, ctx: &FnvScriptContext) -> Result<BeginBlock, SemanticError> {
    let body = transform_stmts(block.body, ctx)?;
    Ok(BeginBlock {
        event: block.event,
        body,
    })
}

fn transform_stmts(
    stmts: Vec<StmtAst>,
    ctx: &FnvScriptContext,
) -> Result<Vec<StmtAst>, SemanticError> {
    stmts.into_iter().map(|s| transform_stmt(s, ctx)).collect()
}

fn transform_stmt(stmt: StmtAst, ctx: &FnvScriptContext) -> Result<StmtAst, SemanticError> {
    match stmt {
        StmtAst::Set { target, value } => Ok(StmtAst::Set {
            target,
            value: transform_expr(value, None, ctx)?,
        }),
        StmtAst::If {
            cond,
            then_body,
            elseif_branches,
            else_body,
        } => {
            let cond = transform_expr(cond, None, ctx)?;
            let then_body = transform_stmts(then_body, ctx)?;
            let elseif_branches = elseif_branches
                .into_iter()
                .map(|(c, b)| Ok((transform_expr(c, None, ctx)?, transform_stmts(b, ctx)?)))
                .collect::<Result<Vec<_>, SemanticError>>()?;
            let else_body = else_body.map(|b| transform_stmts(b, ctx)).transpose()?;
            Ok(StmtAst::If {
                cond,
                then_body,
                elseif_branches,
                else_body,
            })
        }
        StmtAst::Return => Ok(StmtAst::Return),
        StmtAst::Expr(e) => Ok(StmtAst::Expr(transform_expr(e, None, ctx)?)),
    }
}

/// Transform an expression node.
///
/// `parent_entry` is `Some(entry)` when we are transforming an argument whose
/// kind is known from the parent call's `arg_kinds` list.
fn transform_expr(
    expr: ExprAst,
    parent_kind: Option<&str>,
    ctx: &FnvScriptContext,
) -> Result<ExprAst, SemanticError> {
    // AV remap — apply when the parent declared this arg as "actor_value".
    if parent_kind == Some("actor_value") {
        if let ExprAst::Ident(ref name) = expr {
            let mapped = ctx
                .actor_value_map
                .get(&name.to_lowercase())
                .cloned()
                .ok_or_else(|| SemanticError {
                    kind: SemanticErrorKind::UnmappedActorValue,
                    name: name.clone(),
                })?;
            return Ok(ExprAst::Ident(mapped));
        }
    }

    match expr {
        ExprAst::Int(_) | ExprAst::Float(_) | ExprAst::Str(_) => Ok(expr),

        ExprAst::Ident(_) => Ok(expr),

        ExprAst::Not(inner) => Ok(ExprAst::Not(Box::new(transform_expr(*inner, None, ctx)?))),

        ExprAst::BinOp { op, lhs, rhs } => Ok(ExprAst::BinOp {
            op,
            lhs: Box::new(transform_expr(*lhs, None, ctx)?),
            rhs: Box::new(transform_expr(*rhs, None, ctx)?),
        }),

        ExprAst::Call {
            receiver,
            name,
            args,
        } => transform_call(receiver, name, args, ctx),
    }
}

fn transform_call(
    receiver: Option<Box<ExprAst>>,
    name: String,
    args: Vec<ExprAst>,
    ctx: &FnvScriptContext,
) -> Result<ExprAst, SemanticError> {
    let key = name.to_lowercase();
    let entry: Option<&FunctionEntry> = ctx.function_map.get(&key);

    let Some(entry) = entry else {
        // Not in the map — pass through unchanged.
        return Ok(ExprAst::Call {
            receiver: receiver
                .map(|r| transform_expr(*r, None, ctx).map(Box::new))
                .transpose()?,
            name,
            args: args
                .into_iter()
                .map(|a| transform_expr(a, None, ctx))
                .collect::<Result<_, _>>()?,
        });
    };

    // Check for `drop_with_warning`.
    if entry.rewrite.as_deref() == Some("drop_with_warning") {
        return Err(SemanticError {
            kind: SemanticErrorKind::DroppedFunction,
            name,
        });
    }

    let template = entry
        .papyrus
        .as_deref()
        .or(entry.expansion.as_deref())
        .unwrap_or(&name)
        .to_string();

    // Transform args, applying per-arg kind remaps.
    let arg_kinds = &entry.arg_kinds;
    let transformed_args: Vec<ExprAst> = args
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            let kind = arg_kinds.get(i).map(String::as_str);
            transform_expr(a, kind, ctx)
        })
        .collect::<Result<_, _>>()?;

    // Transform receiver.
    let transformed_receiver = receiver
        .map(|r| transform_expr(*r, None, ctx).map(Box::new))
        .transpose()?;

    // Render the template eagerly and embed in a `Str` sentinel.
    let rendered = render_template(
        &template,
        transformed_receiver.as_deref(),
        &transformed_args,
    );
    Ok(ExprAst::Str(format!("{RENDERED_PREFIX}{rendered}")))
}

/// Fill `{self}`, `{arg0}`, `{arg1}`, … placeholders in a template.
fn render_template(template: &str, receiver: Option<&ExprAst>, args: &[ExprAst]) -> String {
    let self_str = receiver
        .map(expr_to_string)
        .unwrap_or_else(|| "Self".to_string());
    let mut out = template.replace("{self}", &self_str);
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{arg{i}}}"), &expr_to_string(arg));
    }
    out
}

/// Convert an expression to its Papyrus source representation.
pub(super) fn expr_to_string(expr: &ExprAst) -> String {
    match expr {
        ExprAst::Int(v) => v.to_string(),
        ExprAst::Float(v) => format!("{v}"),
        ExprAst::Str(s) => {
            if let Some(inner) = s.strip_prefix(RENDERED_PREFIX) {
                inner.to_string()
            } else {
                format!("\"{s}\"")
            }
        }
        ExprAst::Ident(name) => name.clone(),
        ExprAst::Not(inner) => format!("!{}", expr_to_string(inner)),
        ExprAst::BinOp { op, lhs, rhs } => {
            let op_str = match op {
                BinOpKind::Add => "+",
                BinOpKind::Sub => "-",
                BinOpKind::Mul => "*",
                BinOpKind::Div => "/",
                BinOpKind::Eq => "==",
                BinOpKind::NotEq => "!=",
                BinOpKind::Lt => "<",
                BinOpKind::Gt => ">",
                BinOpKind::LtEq => "<=",
                BinOpKind::GtEq => ">=",
                BinOpKind::And => "&&",
                BinOpKind::Or => "||",
            };
            format!("{} {} {}", expr_to_string(lhs), op_str, expr_to_string(rhs))
        }
        ExprAst::Call {
            receiver,
            name,
            args,
        } => {
            let arg_str = args
                .iter()
                .map(expr_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            match receiver {
                Some(r) => format!("{}.{}({})", expr_to_string(r), name, arg_str),
                None => format!("{name}({arg_str})"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fnv_legacy_scripting::function_map::FnvScriptContext;
    use crate::fnv_legacy_scripting::script_translator::lexer::tokenize;
    use crate::fnv_legacy_scripting::script_translator::parser::parse;

    fn translate(src: &str) -> ScriptAst {
        let ctx = FnvScriptContext::load().expect("ctx ok");
        let tokens = tokenize(src).expect("lex ok");
        let ast = parse(tokens).expect("parse ok");
        apply_semantic(ast, &ctx).expect("semantic ok")
    }

    #[test]
    fn semantic_pass_through_unknown_function() {
        // UnknownFn is not in the map; should remain as-is.
        let ast = translate("begin GameMode\nUnknownFn()\nend\n");
        let stmt = &ast.blocks[0].body[0];
        assert!(matches!(stmt, StmtAst::Expr(ExprAst::Call { name, .. }) if name == "UnknownFn"));
    }

    #[test]
    fn semantic_actor_value_remap() {
        // GetActorValue Strength → GetValue(Strength) where arg0 kind=actor_value.
        let ast = translate("begin GameMode\nGetActorValue(Strength)\nend\n");
        let stmt = &ast.blocks[0].body[0];
        if let StmtAst::Expr(ExprAst::Str(s)) = stmt {
            assert!(
                s.contains("Strength"),
                "rendered template should contain Strength"
            );
        } else {
            panic!("expected Expr(Str(..)) sentinel, got: {stmt:?}");
        }
    }

    #[test]
    fn semantic_get_player_renders() {
        let ast = translate("begin GameMode\nGetPlayer()\nend\n");
        let stmt = &ast.blocks[0].body[0];
        if let StmtAst::Expr(ExprAst::Str(s)) = stmt {
            assert!(s.contains("Game.GetPlayer()"));
        } else {
            panic!("expected Str sentinel, got: {stmt:?}");
        }
    }

    #[test]
    fn semantic_drop_with_warning_errors() {
        let ctx = FnvScriptContext::load().expect("ctx ok");
        let tokens = tokenize("begin GameMode\nRewardKarma(10)\nend\n").expect("lex ok");
        let ast = parse(tokens).expect("parse ok");
        let err = apply_semantic(ast, &ctx).expect_err("expected semantic error");
        assert_eq!(err.kind, SemanticErrorKind::DroppedFunction);
        assert_eq!(err.name.to_lowercase(), "rewardkarma");
    }
}
