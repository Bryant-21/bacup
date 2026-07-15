"""Tests for papyrus_tokenizer.tokenize_line()."""
import pytest


def _types(tokens):
    """Extract just token types from a tokenize_line result."""
    from ui.papyrus.papyrus_tokenizer import TokenType
    return [t for _, _, t in tokens]


def _texts(line, tokens):
    """Extract token text strings from a tokenize_line result."""
    return [line[s:e] for s, e, _ in tokens]


def test_keyword_function():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("Function MyFunc()")
    by_text = {line[s:e]: t for line, (s, e, t) in
               [("Function MyFunc()", tok) for tok in tokenize_line("Function MyFunc()")]}
    # Re-do cleanly
    line = "Function MyFunc()"
    tokens = tokenize_line(line)
    texts = _texts(line, tokens)
    types = _types(tokens)
    assert "Function" in texts
    idx = texts.index("Function")
    assert types[idx] == TokenType.KEYWORD


def test_keyword_case_insensitive():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    for variant in ("function", "FUNCTION", "Function", "fUnCtIoN"):
        line = variant
        tokens = tokenize_line(line)
        assert len(tokens) == 1
        assert tokens[0][2] == TokenType.KEYWORD, f"Failed for {variant!r}"


def test_all_keywords_recognised():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType, _KEYWORDS
    for kw in _KEYWORDS:
        tokens = tokenize_line(kw)
        assert tokens, f"No tokens for keyword {kw!r}"
        assert tokens[0][2] == TokenType.KEYWORD, f"{kw!r} not KEYWORD: {tokens[0][2]}"


def test_keyword_length():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("length")
    assert tokens[0][2] == TokenType.KEYWORD


def test_type_name():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    for typename in ("Int", "float", "Bool", "String", "Actor", "ObjectReference"):
        tokens = tokenize_line(typename)
        assert tokens[0][2] == TokenType.TYPE_NAME, f"{typename!r} not TYPE_NAME"


def test_builtin_constants():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    for builtin in ("None", "True", "False", "none", "true", "false"):
        tokens = tokenize_line(builtin)
        assert tokens[0][2] == TokenType.BUILTIN_CONST, f"{builtin!r} not BUILTIN_CONST"


def test_string():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line('"hello world"')
    assert tokens[0][2] == TokenType.STRING
    assert tokens[0][0] == 0
    assert tokens[0][1] == 13


def test_comment():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    line = "x = 1 ; this is a comment"
    tokens = tokenize_line(line)
    comment_tok = [t for t in tokens if t[2] == TokenType.COMMENT]
    assert comment_tok
    assert line[comment_tok[0][0]:comment_tok[0][1]] == "; this is a comment"


def test_number_int():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("42")
    assert tokens[0][2] == TokenType.NUMBER


def test_number_float():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("3.14")
    assert tokens[0][2] == TokenType.NUMBER


def test_number_hex():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("0xFF00AA")
    assert tokens[0][2] == TokenType.NUMBER


def test_variable():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("myVariable")
    assert tokens[0][2] == TokenType.VARIABLE


def test_class_name():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("Container")
    assert tokens[0][2] == TokenType.CLASS_NAME
    tokens2 = tokenize_line("GlobalVariable")
    assert tokens2[0][2] == TokenType.CLASS_NAME


def test_operator():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("=")
    assert tokens[0][2] == TokenType.OPERATOR


def test_empty_line():
    from ui.papyrus.papyrus_tokenizer import tokenize_line
    assert tokenize_line("") == []


def test_mixed_line():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    line = 'Int x = 5 ; comment'
    tokens = tokenize_line(line)
    texts = _texts(line, tokens)
    types = _types(tokens)
    assert types[texts.index("Int")] == TokenType.TYPE_NAME
    assert types[texts.index("x")] == TokenType.VARIABLE
    assert types[texts.index("=")] == TokenType.OPERATOR
    assert types[texts.index("5")] == TokenType.NUMBER
    comment_types = [t for t in types if t == TokenType.COMMENT]
    assert comment_types


def test_keyword_not_substring():
    """'iffy' should be VARIABLE, not keyword 'if' + VARIABLE 'fy'."""
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("iffy")
    assert len(tokens) == 1
    assert tokens[0][2] == TokenType.VARIABLE


def test_span_coverage():
    """All characters in a non-whitespace line should be covered by token spans."""
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    line = 'Function Foo(Int akValue)'
    tokens = tokenize_line(line)
    covered = set()
    for s, e, t in tokens:
        if t != TokenType.WHITESPACE:
            covered.update(range(s, e))
    # All non-space characters should be covered
    for i, ch in enumerate(line):
        if ch not in (' ', '\t'):
            assert i in covered, f"char {i!r}={ch!r} not covered"


# --- Starfield keyword tests ---

def test_starfield_guard_keywords():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    for kw in ("LockGuard", "EndLockGuard", "TryLockGuard",
               "ElseTryLockGuard", "EndTryLockGuard"):
        tokens = tokenize_line(kw)
        assert tokens[0][2] == TokenType.KEYWORD, f"{kw!r} not KEYWORD"


def test_starfield_access_modifiers():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    for kw in ("Private", "Protected", "Internal", "SelfOnly",
               "RequiresGuard", "ProtectsFunctionLogic"):
        tokens = tokenize_line(kw)
        assert tokens[0][2] == TokenType.KEYWORD, f"{kw!r} not KEYWORD"


def test_guard_type():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("Guard")
    assert tokens[0][2] == TokenType.TYPE_NAME


def test_doc_comment():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    line = 'Int Property MyProp Auto {This is documentation}'
    tokens = tokenize_line(line)
    types = _types(tokens)
    texts = _texts(line, tokens)
    assert "{This is documentation}" in texts
    idx = texts.index("{This is documentation}")
    assert types[idx] == TokenType.DOC_COMMENT


def test_doc_comment_standalone():
    from ui.papyrus.papyrus_tokenizer import tokenize_line, TokenType
    tokens = tokenize_line("{A doc comment}")
    assert len(tokens) == 1
    assert tokens[0][2] == TokenType.DOC_COMMENT
