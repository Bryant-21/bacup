"""Papyrus syntax tokenizer — regex-based, line-by-line."""
from __future__ import annotations

import re
from enum import Enum, auto


class TokenType(Enum):
    KEYWORD       = auto()
    TYPE_NAME     = auto()
    BUILTIN_CONST = auto()
    CLASS_NAME    = auto()   # PascalCase not followed by '(': type/script references
    FUNCTION_CALL = auto()   # PascalCase followed by '(': function/method calls
    VARIABLE      = auto()   # camelCase/lowercase identifiers: local vars, parameters
    STRING        = auto()
    COMMENT       = auto()
    DOC_COMMENT   = auto()   # { ... } documentation comments
    NUMBER        = auto()
    OPERATOR      = auto()
    WHITESPACE    = auto()


_KEYWORDS: frozenset[str] = frozenset({
    'if', 'elseif', 'else', 'endif', 'while', 'endwhile', 'return',
    'scriptname', 'extends', 'function', 'endfunction', 'event', 'endevent',
    'state', 'endstate', 'property', 'endproperty', 'struct', 'endstruct',
    'group', 'endgroup', 'import', 'customevent', 'new', 'as', 'is',
    'self', 'parent', 'length',
    'auto', 'autoreadonly', 'native', 'global', 'hidden', 'conditional',
    'const', 'mandatory', 'default', 'betaonly', 'debugonly',
    'collapsed', 'collapsedonref', 'collapsedonbase',
    # Starfield: guard blocks and access modifiers
    'lockguard', 'endlockguard', 'trylockguard', 'elsetrylockguard', 'endtrylockguard',
    'requiresguard', 'protectsfunctionlogic',
    'selfonly', 'private', 'protected', 'internal',
})

_TYPES: frozenset[str] = frozenset({
    'int', 'float', 'bool', 'string', 'var',
    'objectreference', 'actor', 'quest', 'sound', 'weapon', 'armor', 'activator',
    'guard',  # Starfield guard type
})

_BUILTINS: frozenset[str] = frozenset({'none', 'true', 'false'})

# Order matters: first match wins per position.
# Note: escaped quotes inside strings are not supported (matches Lark grammar behaviour).
_PATTERN = re.compile(
    r'(?P<DOC_COMMENT>\{[^}]*\})'
    r'|(?P<COMMENT>;[^\n]*)'
    r'|(?P<STRING>"[^"]*")'
    r'|(?P<NUMBER>0[xX][0-9a-fA-F]+|[0-9]+\.[0-9]*|[0-9]+)'
    r'|(?P<WORD>[A-Za-z_][A-Za-z0-9_]*)'
    r'|(?P<OPERATOR>[()[\]\.,=+\-*/%<>!&|])'
    r'|(?P<WHITESPACE>[ \t]+)'
)


def tokenize_line(line: str) -> list[tuple[int, int, TokenType]]:
    """Return list of (start_col, end_col, token_type) for a single line."""
    tokens: list[tuple[int, int, TokenType]] = []
    for m in _PATTERN.finditer(line):
        start, end = m.start(), m.end()
        kind = m.lastgroup
        if kind == 'DOC_COMMENT':
            ttype = TokenType.DOC_COMMENT
        elif kind == 'COMMENT':
            ttype = TokenType.COMMENT
        elif kind == 'STRING':
            ttype = TokenType.STRING
        elif kind == 'NUMBER':
            ttype = TokenType.NUMBER
        elif kind == 'WORD':
            raw = m.group()
            w = raw.lower()
            if w in _BUILTINS:
                ttype = TokenType.BUILTIN_CONST
            elif w in _KEYWORDS:
                ttype = TokenType.KEYWORD
            elif w in _TYPES:
                ttype = TokenType.TYPE_NAME
            elif raw[0].isupper():
                # Peek past the token: if '(' follows it's a function call
                ttype = TokenType.FUNCTION_CALL if line[end:end+1] == '(' else TokenType.CLASS_NAME
            else:
                ttype = TokenType.VARIABLE
        elif kind == 'OPERATOR':
            ttype = TokenType.OPERATOR
        else:
            ttype = TokenType.WHITESPACE
        tokens.append((start, end, ttype))
    return tokens
