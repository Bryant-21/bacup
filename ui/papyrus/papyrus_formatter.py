"""papyrus_formatter — simple indentation formatter for Papyrus scripts."""
from __future__ import annotations

import re

# Keywords that decrease indent *before* the line is written
_DEDENT_BEFORE = re.compile(
    r'^(else|elseif|endif|endwhile|endfunction|endevent|endstate|endproperty|endgroup)\b',
    re.IGNORECASE,
)

# Keywords that increase indent *after* the line is written
_INDENT_AFTER = re.compile(
    r'^(function|event|state|if|while|else|elseif|group)\b',
    re.IGNORECASE,
)

# Property that ends with 'auto' is a one-liner — indent block comes after
_PROPERTY_AUTO = re.compile(r'^property\b.*\bauto\b\s*$', re.IGNORECASE)

# Top-level header lines — always at indent 0
_TOP_LEVEL = re.compile(r'^(scriptname|import)\b', re.IGNORECASE)

_INDENT = '    '


def format_papyrus(text: str) -> str:
    """Re-indent a Papyrus source file.

    Rules:
    - 4-space indentation
    - Trailing whitespace stripped from every line
    - Blank lines preserved without added indent
    - scriptname/import lines always at column 0
    - Block openers (Function, Event, State, If, While, Else, ElseIf, Group,
      Property…Auto) → indent following lines
    - Block closers (EndFunction, EndEvent, EndState, EndIf, EndWhile,
      EndProperty, EndGroup, Else, ElseIf) → dedent before writing
    - File ends with a single newline
    """
    lines = text.split('\n')
    out: list[str] = []
    indent = 0

    for raw in lines:
        stripped = raw.strip()

        if not stripped:
            out.append('')
            continue

        lower = stripped.lower()

        # Top-level header — always column 0
        if _TOP_LEVEL.match(stripped):
            out.append(stripped)
            continue

        # Dedent before writing this line
        if _DEDENT_BEFORE.match(stripped):
            indent = max(0, indent - 1)

        out.append(_INDENT * indent + stripped)

        # Indent after writing this line
        if _INDENT_AFTER.match(stripped) or _PROPERTY_AUTO.match(stripped):
            indent += 1

    result = '\n'.join(out)
    if not result.endswith('\n'):
        result += '\n'
    return result
