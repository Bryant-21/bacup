//! FNV script lexer — converts raw source text into a flat token stream.
//!
//! FNV scripts use a line-oriented syntax derived from Bethesda GECK scripting.
//! The lexer is hand-written and intentionally simple: one token kind per
//! production, no backtracking.

// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

/// Byte-offset range in the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

// ---------------------------------------------------------------------------
// Keywords
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordKind {
    Set,
    To,
    If,
    Else,
    ElseIf,
    EndIf,
    Begin,
    End,
    Return,
    ScriptName,
    Scn,
    Short,
    Long,
    Int,
    Float,
    Ref,
    StringVar,
    Equals,
}

fn keyword_from_str(s: &str) -> Option<KeywordKind> {
    match s.to_lowercase().as_str() {
        "set" => Some(KeywordKind::Set),
        "to" => Some(KeywordKind::To),
        "if" => Some(KeywordKind::If),
        "else" => Some(KeywordKind::Else),
        "elseif" => Some(KeywordKind::ElseIf),
        "endif" => Some(KeywordKind::EndIf),
        "begin" => Some(KeywordKind::Begin),
        "end" => Some(KeywordKind::End),
        "return" => Some(KeywordKind::Return),
        "scriptname" => Some(KeywordKind::ScriptName),
        "scn" => Some(KeywordKind::Scn),
        "short" => Some(KeywordKind::Short),
        "long" => Some(KeywordKind::Long),
        "int" => Some(KeywordKind::Int),
        "float" => Some(KeywordKind::Float),
        "ref" => Some(KeywordKind::Ref),
        "string_var" => Some(KeywordKind::StringVar),
        "equals" => Some(KeywordKind::Equals),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorKind {
    Plus,
    Minus,
    Star,
    Slash,
    Assign,     // =  (in non-comparison context)
    EqEq,       // ==
    NotEq,      // !=
    Lt,         // <
    Gt,         // >
    LtEq,       // <=
    GtEq,       // >=
    Dot,        // .  (member access)
    LParen,     // (
    RParen,     // )
    LBracket,   // [
    RBracket,   // ]
    Comma,      // ,
    Ampersand2, // &&
    Pipe2,      // ||
    Bang,       // !
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(KeywordKind),
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    Op(OperatorKind),
    Newline,
    Eof,
}

// ---------------------------------------------------------------------------
// LexError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub offset: usize,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lex error at byte {}: {}", self.offset, self.message)
    }
}
impl std::error::Error for LexError {}

// ---------------------------------------------------------------------------
// tokenize
// ---------------------------------------------------------------------------

/// Tokenise `input` into a flat list of `(Token, Span)` pairs.
///
/// Newlines are emitted as `Token::Newline`; whitespace within a line is
/// silently skipped.  Comments (`;` through end-of-line) are also discarded.
/// The final token is always `Token::Eof`.
pub fn tokenize(input: &str) -> Result<Vec<(Token, Span)>, LexError> {
    let src = input.as_bytes();
    let len = src.len();
    let mut pos = 0usize;
    let mut out: Vec<(Token, Span)> = Vec::new();

    macro_rules! err {
        ($msg:expr) => {
            return Err(LexError {
                message: $msg.to_string(),
                offset: pos,
            })
        };
    }

    while pos < len {
        let start = pos;
        let ch = src[pos];

        match ch {
            // Inline whitespace — skip silently.
            b' ' | b'\t' | b'\r' => {
                pos += 1;
            }

            // Newline.
            b'\n' => {
                // Collapse consecutive blank lines into a single Newline.
                let last_is_nl = out
                    .last()
                    .map(|(t, _)| matches!(t, Token::Newline))
                    .unwrap_or(false);
                if !last_is_nl {
                    out.push((Token::Newline, Span::new(start, pos + 1)));
                }
                pos += 1;
            }

            // Comment — skip to end of line.
            b';' => {
                while pos < len && src[pos] != b'\n' {
                    pos += 1;
                }
            }

            // String literal.
            b'"' => {
                pos += 1; // skip opening "
                let string_start = pos;
                while pos < len && src[pos] != b'"' && src[pos] != b'\n' {
                    pos += 1;
                }
                if pos >= len || src[pos] == b'\n' {
                    err!("unterminated string literal");
                }
                let s = &input[string_start..pos];
                pos += 1; // skip closing "
                out.push((Token::StringLit(s.to_string()), Span::new(start, pos)));
            }

            // Two-character operators first.
            b'=' if pos + 1 < len && src[pos + 1] == b'=' => {
                pos += 2;
                out.push((Token::Op(OperatorKind::EqEq), Span::new(start, pos)));
            }
            b'!' if pos + 1 < len && src[pos + 1] == b'=' => {
                pos += 2;
                out.push((Token::Op(OperatorKind::NotEq), Span::new(start, pos)));
            }
            b'<' if pos + 1 < len && src[pos + 1] == b'=' => {
                pos += 2;
                out.push((Token::Op(OperatorKind::LtEq), Span::new(start, pos)));
            }
            b'>' if pos + 1 < len && src[pos + 1] == b'=' => {
                pos += 2;
                out.push((Token::Op(OperatorKind::GtEq), Span::new(start, pos)));
            }
            b'&' if pos + 1 < len && src[pos + 1] == b'&' => {
                pos += 2;
                out.push((Token::Op(OperatorKind::Ampersand2), Span::new(start, pos)));
            }
            b'|' if pos + 1 < len && src[pos + 1] == b'|' => {
                pos += 2;
                out.push((Token::Op(OperatorKind::Pipe2), Span::new(start, pos)));
            }

            // Single-character operators.
            b'=' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Assign), Span::new(start, pos)));
            }
            b'<' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Lt), Span::new(start, pos)));
            }
            b'>' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Gt), Span::new(start, pos)));
            }
            b'!' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Bang), Span::new(start, pos)));
            }
            b'+' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Plus), Span::new(start, pos)));
            }
            b'-' if !is_digit_next(src, pos + 1) => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Minus), Span::new(start, pos)));
            }
            b'*' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Star), Span::new(start, pos)));
            }
            b'/' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Slash), Span::new(start, pos)));
            }
            b'.' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Dot), Span::new(start, pos)));
            }
            b'(' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::LParen), Span::new(start, pos)));
            }
            b')' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::RParen), Span::new(start, pos)));
            }
            b'[' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::LBracket), Span::new(start, pos)));
            }
            b']' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::RBracket), Span::new(start, pos)));
            }
            b',' => {
                pos += 1;
                out.push((Token::Op(OperatorKind::Comma), Span::new(start, pos)));
            }

            // Numbers (int or float), and negative numbers.
            b'0'..=b'9' | b'-' => {
                let negative = ch == b'-';
                if negative {
                    pos += 1;
                }
                let num_start = pos;
                while pos < len && src[pos].is_ascii_digit() {
                    pos += 1;
                }
                let is_float =
                    pos < len && src[pos] == b'.' && pos + 1 < len && src[pos + 1].is_ascii_digit();
                if is_float {
                    pos += 1; // skip dot
                    while pos < len && src[pos].is_ascii_digit() {
                        pos += 1;
                    }
                    let s = &input[if negative { start } else { num_start }..pos];
                    let v: f64 = s.parse().map_err(|_| LexError {
                        message: format!("invalid float {s:?}"),
                        offset: start,
                    })?;
                    out.push((Token::FloatLit(v), Span::new(start, pos)));
                } else {
                    let s = &input[if negative { start } else { num_start }..pos];
                    let v: i64 = s.parse().map_err(|_| LexError {
                        message: format!("invalid integer {s:?}"),
                        offset: start,
                    })?;
                    out.push((Token::IntLit(v), Span::new(start, pos)));
                }
            }

            // Identifiers and keywords.
            b'_' | b'a'..=b'z' | b'A'..=b'Z' => {
                while pos < len && is_ident_char(src[pos]) {
                    pos += 1;
                }
                let word = &input[start..pos];
                if let Some(kw) = keyword_from_str(word) {
                    out.push((Token::Keyword(kw), Span::new(start, pos)));
                } else {
                    out.push((Token::Ident(word.to_string()), Span::new(start, pos)));
                }
            }

            other => {
                err!(format!("unexpected character {:?}", other as char));
            }
        }
    }

    out.push((Token::Eof, Span::new(len, len)));
    Ok(out)
}

#[inline]
fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[inline]
fn is_digit_next(src: &[u8], pos: usize) -> bool {
    pos < src.len() && src[pos].is_ascii_digit()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens_only(input: &str) -> Vec<Token> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .filter(|t| !matches!(t, Token::Eof))
            .collect()
    }

    #[test]
    fn lex_set_to_int() {
        let toks = tokens_only("set Speed to 100");
        assert_eq!(
            toks,
            vec![
                Token::Keyword(KeywordKind::Set),
                Token::Ident("Speed".into()),
                Token::Keyword(KeywordKind::To),
                Token::IntLit(100),
            ]
        );
    }

    #[test]
    fn lex_identifiers_and_keywords() {
        let toks = tokens_only("begin GameMode");
        assert_eq!(
            toks,
            vec![
                Token::Keyword(KeywordKind::Begin),
                Token::Ident("GameMode".into()),
            ]
        );
    }

    #[test]
    fn lex_comment_skipped() {
        let toks = tokens_only("; this is a comment\nset x to 1");
        assert_eq!(
            toks,
            vec![
                Token::Newline,
                Token::Keyword(KeywordKind::Set),
                Token::Ident("x".into()),
                Token::Keyword(KeywordKind::To),
                Token::IntLit(1),
            ]
        );
    }

    #[test]
    fn lex_string_literal() {
        let toks = tokens_only(r#""hello world""#);
        assert_eq!(toks, vec![Token::StringLit("hello world".into())]);
    }

    #[test]
    fn lex_float() {
        let toks = tokens_only("3.14");
        assert_eq!(toks, vec![Token::FloatLit(3.14)]);
    }

    #[test]
    fn lex_negative_int() {
        let toks = tokens_only("-42");
        assert_eq!(toks, vec![Token::IntLit(-42)]);
    }

    #[test]
    fn lex_two_char_ops() {
        let toks = tokens_only("== != <= >= && ||");
        assert_eq!(
            toks,
            vec![
                Token::Op(OperatorKind::EqEq),
                Token::Op(OperatorKind::NotEq),
                Token::Op(OperatorKind::LtEq),
                Token::Op(OperatorKind::GtEq),
                Token::Op(OperatorKind::Ampersand2),
                Token::Op(OperatorKind::Pipe2),
            ]
        );
    }

    #[test]
    fn lex_newlines_collapsed() {
        let toks: Vec<Token> = tokenize("a\n\n\nb")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .filter(|t| !matches!(t, Token::Eof))
            .collect();
        // Two idents with a single Newline between them.
        assert_eq!(
            toks,
            vec![
                Token::Ident("a".into()),
                Token::Newline,
                Token::Ident("b".into()),
            ]
        );
    }

    #[test]
    fn lex_multiline_script() {
        let src = "begin GameMode\n  set x to 1\nend";
        let toks = tokens_only(src);
        assert_eq!(
            toks,
            vec![
                Token::Keyword(KeywordKind::Begin),
                Token::Ident("GameMode".into()),
                Token::Newline,
                Token::Keyword(KeywordKind::Set),
                Token::Ident("x".into()),
                Token::Keyword(KeywordKind::To),
                Token::IntLit(1),
                Token::Newline,
                Token::Keyword(KeywordKind::End),
            ]
        );
    }

    #[test]
    fn lex_dot_member_access() {
        let toks = tokens_only("myQuest.GetStage");
        assert_eq!(
            toks,
            vec![
                Token::Ident("myQuest".into()),
                Token::Op(OperatorKind::Dot),
                Token::Ident("GetStage".into()),
            ]
        );
    }

    #[test]
    fn lex_var_decl_types() {
        let toks = tokens_only("short myVar");
        assert_eq!(
            toks,
            vec![
                Token::Keyword(KeywordKind::Short),
                Token::Ident("myVar".into()),
            ]
        );
    }

    #[test]
    fn lex_unterminated_string_errors() {
        let result = tokenize(r#""no close"#);
        assert!(
            result.is_err(),
            "expected lex error for unterminated string"
        );
    }
}
