//! Papyrus source emitter — walks the post-semantic AST and writes Papyrus.
//!
//! The emitter respects the sentinel prefix injected by the semantic pass
//! (`\x00papyrus\x00`) and emits raw pre-rendered fragments directly without
//! additional quoting.

use super::ast::{BeginBlock, BinOpKind, ExprAst, ScriptAst, StmtAst, VarDecl};
use super::semantic::{RENDERED_PREFIX, expr_to_string};

// ---------------------------------------------------------------------------
// Type map
// ---------------------------------------------------------------------------

fn fnv_type_to_papyrus(source_type: &str) -> &'static str {
    match source_type {
        "short" | "long" | "int" => "Int",
        "float" => "Float",
        "ref" => "ObjectReference",
        "string_var" => "String",
        _ => "Int",
    }
}

// ---------------------------------------------------------------------------
// Event-name translation
// ---------------------------------------------------------------------------

/// Translate an FNV block event name into its Papyrus `Event` declaration.
fn papyrus_event_decl(event: &str) -> String {
    let name = event.split_whitespace().next().unwrap_or(event);
    match name.to_lowercase().as_str() {
        "onactivate" => "OnActivate(ObjectReference akActionRef)".to_string(),
        "gamemode" => "OnInit()".to_string(),
        _ => format!("{name}()"),
    }
}

// ---------------------------------------------------------------------------
// emit_papyrus
// ---------------------------------------------------------------------------

/// Emit a complete Papyrus `.psc` file from a post-semantic `ScriptAst`.
///
/// `script_class_name` and `papyrus_extends` override the in-script `ScriptName`
/// when provided (the conversion pipeline always passes explicit names).
pub fn emit_papyrus(ast: &ScriptAst, script_class_name: &str, papyrus_extends: &str) -> String {
    let mut out = String::new();

    // ScriptName header.
    out.push_str(&format!(
        "ScriptName {script_class_name} extends {papyrus_extends}\n\n"
    ));

    // Variable declarations.
    for var in &ast.vars {
        emit_var(&mut out, var);
    }
    if !ast.vars.is_empty() {
        out.push('\n');
    }

    // Begin/Event blocks.
    for block in &ast.blocks {
        emit_block(&mut out, block);
        out.push('\n');
    }

    // Trim trailing whitespace from the last block's trailing newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out.push('\n');

    out
}

fn emit_var(out: &mut String, var: &VarDecl) {
    let ptype = fnv_type_to_papyrus(&var.source_type);
    out.push_str(&format!("{ptype} {}\n", var.name));
}

fn emit_block(out: &mut String, block: &BeginBlock) {
    let decl = papyrus_event_decl(&block.event);
    out.push_str(&format!("Event {decl}\n"));
    for stmt in &block.body {
        emit_stmt(out, stmt, 1);
    }
    out.push_str("EndEvent\n");
}

fn emit_stmt(out: &mut String, stmt: &StmtAst, indent: usize) {
    let pad = "    ".repeat(indent);
    match stmt {
        StmtAst::Set { target, value } => {
            out.push_str(&format!("{pad}{target} = {}\n", emit_expr(value)));
        }
        StmtAst::If {
            cond,
            then_body,
            elseif_branches,
            else_body,
        } => {
            out.push_str(&format!("{pad}If {}\n", emit_expr(cond)));
            for s in then_body {
                emit_stmt(out, s, indent + 1);
            }
            for (elif_cond, elif_body) in elseif_branches {
                out.push_str(&format!("{pad}ElseIf {}\n", emit_expr(elif_cond)));
                for s in elif_body {
                    emit_stmt(out, s, indent + 1);
                }
            }
            if let Some(else_stmts) = else_body {
                out.push_str(&format!("{pad}Else\n"));
                for s in else_stmts {
                    emit_stmt(out, s, indent + 1);
                }
            }
            out.push_str(&format!("{pad}EndIf\n"));
        }
        StmtAst::Return => {
            out.push_str(&format!("{pad}Return\n"));
        }
        StmtAst::Expr(e) => {
            out.push_str(&format!("{pad}{}\n", emit_expr(e)));
        }
    }
}

/// Emit an expression as a Papyrus string fragment.
fn emit_expr(expr: &ExprAst) -> String {
    match expr {
        ExprAst::Str(s) if s.starts_with(RENDERED_PREFIX) => s[RENDERED_PREFIX.len()..].to_string(),
        _ => expr_to_string(expr),
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
    use crate::fnv_legacy_scripting::script_translator::semantic::apply_semantic;

    fn compile(src: &str) -> String {
        let tokens = tokenize(src).unwrap();
        let ast = parse(tokens).unwrap();
        let ctx = FnvScriptContext::load().unwrap();
        let semantic = apply_semantic(ast, &ctx).unwrap();
        emit_papyrus(&semantic, "TestScript", "ObjectReference")
    }

    #[test]
    fn emit_header() {
        let out = compile("begin GameMode\nend\n");
        assert!(out.starts_with("ScriptName TestScript extends ObjectReference"));
    }

    #[test]
    fn emit_set_statement() {
        let out = compile("begin GameMode\nset x to 42\nend\n");
        assert!(out.contains("x = 42"), "output:\n{out}");
    }

    #[test]
    fn emit_get_player_call() {
        let out = compile("begin GameMode\nGetPlayer()\nend\n");
        assert!(out.contains("Game.GetPlayer()"), "output:\n{out}");
    }

    #[test]
    fn emit_event_block_gamemode_becomes_oninit() {
        let out = compile("begin GameMode\nend\n");
        assert!(out.contains("Event OnInit()"), "output:\n{out}");
    }

    #[test]
    fn emit_onactivate_event() {
        let out = compile("begin OnActivate\nend\n");
        assert!(
            out.contains("Event OnActivate(ObjectReference akActionRef)"),
            "output:\n{out}"
        );
    }

    #[test]
    fn emit_var_decl_types() {
        let out = compile("short myInt\nfloat myFloat\nref myRef\nbegin GameMode\nend\n");
        assert!(out.contains("Int myInt"), "output:\n{out}");
        assert!(out.contains("Float myFloat"), "output:\n{out}");
        assert!(out.contains("ObjectReference myRef"), "output:\n{out}");
    }

    #[test]
    fn emit_if_else() {
        let src = "begin GameMode\nif x == 1\nreturn\nelse\nset y to 0\nendif\nend\n";
        let out = compile(src);
        assert!(out.contains("If x == 1"), "output:\n{out}");
        assert!(out.contains("Else"), "output:\n{out}");
        assert!(out.contains("EndIf"), "output:\n{out}");
    }

    #[test]
    fn emit_return() {
        let out = compile("begin GameMode\nreturn\nend\n");
        assert!(out.contains("Return"), "output:\n{out}");
    }
}
