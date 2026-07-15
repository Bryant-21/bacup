"""Reusable imgui widgets shared across Fallout 4 MCP desktop tools.

Provides:
- draw_output_log() — scrollable monospace log panel
- draw_status_bar() — separator + text at bottom
- syntax_highlight_yaml() — YAML tokenizer returning colored spans
- syntax_highlight_papyrus() — Papyrus tokenizer returning colored spans
- draw_highlighted_text() — render colored spans via imgui.text_colored
"""

import re
from imgui_bundle import imgui


# ---------------------------------------------------------------------------
# Colors (ImVec4 RGBA)
# ---------------------------------------------------------------------------
COL_YAML_KEY = imgui.ImVec4(0.475, 0.753, 1.0, 1.0)        # #79c0ff
COL_YAML_FORMKEY = imgui.ImVec4(0.910, 0.784, 0.471, 1.0)  # #e8c878
COL_YAML_STRING = imgui.ImVec4(0.659, 0.847, 0.659, 1.0)   # #a8d8a8
COL_YAML_BOOL = imgui.ImVec4(1.0, 0.482, 0.447, 1.0)       # #ff7b72
COL_YAML_NUMBER = imgui.ImVec4(0.941, 0.659, 0.408, 1.0)   # #f0a868
COL_YAML_LIST = imgui.ImVec4(0.784, 0.659, 0.910, 1.0)     # #c8a8e8
COL_YAML_COMMENT = imgui.ImVec4(0.545, 0.580, 0.624, 1.0)  # #8b949e
COL_DEFAULT = imgui.ImVec4(0.85, 0.85, 0.85, 1.0)

COL_PAP_KEYWORD = imgui.ImVec4(1.0, 0.482, 0.447, 1.0)     # #ff7b72
COL_PAP_TYPE = imgui.ImVec4(0.475, 0.753, 1.0, 1.0)        # #79c0ff
COL_PAP_LITERAL = imgui.ImVec4(0.941, 0.659, 0.408, 1.0)   # #f0a868
COL_PAP_STRING = imgui.ImVec4(0.659, 0.847, 0.659, 1.0)    # #a8d8a8
COL_PAP_COMMENT = imgui.ImVec4(0.545, 0.580, 0.624, 1.0)   # #8b949e
COL_PAP_FUNC = imgui.ImVec4(0.824, 0.659, 1.0, 1.0)        # #d2a8ff

# Source badge colors
SOURCE_COLORS = {
    "records": imgui.ImVec4(0.314, 0.549, 0.863, 1.0),    # blue
    "papyrus": imgui.ImVec4(0.314, 0.745, 0.471, 1.0),    # green
    "ck": imgui.ImVec4(0.863, 0.627, 0.235, 1.0),          # orange
    "wiki": imgui.ImVec4(0.314, 0.745, 0.471, 1.0),        # green (unified wiki)
    "scripts": imgui.ImVec4(0.784, 0.471, 0.784, 1.0),     # purple
    "havok": imgui.ImVec4(0.835, 0.424, 0.235, 1.0),       # rust
    "nifs": imgui.ImVec4(0.471, 0.784, 0.784, 1.0),        # teal
    "ext_records": imgui.ImVec4(0.471, 0.549, 0.863, 1.0), # light blue
    "ext_scripts": imgui.ImVec4(0.620, 0.471, 0.784, 1.0), # light purple
    "function": imgui.ImVec4(0.314, 0.745, 0.471, 1.0),    # green
    "browse": imgui.ImVec4(0.7, 0.7, 0.7, 1.0),            # gray
}


# ---------------------------------------------------------------------------
# YAML syntax highlighting
# ---------------------------------------------------------------------------
_YAML_RULES = [
    # FormKeys (before strings so they get gold)
    (re.compile(r"\b[0-9A-Fa-f]{6}:[A-Za-z0-9_.]+"), COL_YAML_FORMKEY),
    # Keys
    (re.compile(r"^\s*[\w ./<>|-]+\s*:"), COL_YAML_KEY),
    # Strings
    (re.compile(r'"[^"]*"'), COL_YAML_STRING),
    (re.compile(r"'[^']*'"), COL_YAML_STRING),
    # Booleans / null
    (re.compile(r"\b(true|false|null|True|False|Null|NULL)\b"), COL_YAML_BOOL),
    # Numbers
    (re.compile(r"\b-?\d+(\.\d+)?\b"), COL_YAML_NUMBER),
    # List dash
    (re.compile(r"^\s*- "), COL_YAML_LIST),
    # Comments
    (re.compile(r"#.*$"), COL_YAML_COMMENT),
]


def syntax_highlight_yaml(text: str) -> list[list[tuple]]:
    """Tokenize YAML text into colored spans per line.

    Returns a list of lines, each line is a list of (ImVec4_color, str) tuples.
    """
    result = []
    for line in text.split("\n"):
        spans = _highlight_line(line, _YAML_RULES)
        result.append(spans)
    return result


# ---------------------------------------------------------------------------
# Papyrus syntax highlighting
# ---------------------------------------------------------------------------
_PAP_KEYWORDS = [
    "scriptname", "extends", "import",
    "function", "endfunction", "event", "endevent",
    "state", "endstate", "property", "endproperty",
    "if", "elseif", "else", "endif",
    "while", "endwhile", "return",
    "new", "as", "is", "self", "parent", "length",
    "auto", "autoreadonly", "native", "global",
    "conditional", "hidden", "mandatory", "const",
    "betaonly", "debugonly",
]
_PAP_TYPES = ["int", "float", "bool", "string", "var"]
_PAP_LITERALS = ["true", "false", "none"]

_PAP_RULES = []
for _kw in _PAP_KEYWORDS:
    _PAP_RULES.append((re.compile(rf"\b{_kw}\b", re.IGNORECASE), COL_PAP_KEYWORD))
for _t in _PAP_TYPES:
    _PAP_RULES.append((re.compile(rf"\b{_t}\b", re.IGNORECASE), COL_PAP_TYPE))
for _lit in _PAP_LITERALS:
    _PAP_RULES.append((re.compile(rf"\b{_lit}\b", re.IGNORECASE), COL_PAP_LITERAL))
_PAP_RULES.append((re.compile(r'"[^"]*"'), COL_PAP_STRING))
_PAP_RULES.append((re.compile(r"\b-?\d+(\.\d+)?\b"), COL_PAP_LITERAL))
_PAP_RULES.append((re.compile(r";.*$"), COL_PAP_COMMENT))


def syntax_highlight_papyrus(text: str) -> list[list[tuple]]:
    """Tokenize Papyrus text into colored spans per line.

    Returns a list of lines, each line is a list of (ImVec4_color, str) tuples.
    """
    result = []
    for line in text.split("\n"):
        spans = _highlight_line(line, _PAP_RULES)
        result.append(spans)
    return result


# ---------------------------------------------------------------------------
# Shared highlighting engine
# ---------------------------------------------------------------------------
def _highlight_line(line: str, rules: list) -> list[tuple]:
    """Apply regex rules to produce colored spans for a single line.

    Each rule is (compiled_regex, ImVec4_color). Rules are applied in order;
    later rules override earlier ones on overlap.
    """
    if not line:
        return [(COL_DEFAULT, "")]

    # Build color map per character
    colors = [COL_DEFAULT] * len(line)
    for pattern, color in rules:
        for m in pattern.finditer(line):
            for i in range(m.start(), m.end()):
                colors[i] = color

    # Merge consecutive chars with same color into spans
    spans = []
    current_color = colors[0]
    start = 0
    for i in range(1, len(line)):
        if colors[i] is not current_color:
            spans.append((current_color, line[start:i]))
            current_color = colors[i]
            start = i
    spans.append((current_color, line[start:]))
    return spans


# ---------------------------------------------------------------------------
# Drawing helpers
# ---------------------------------------------------------------------------
def draw_highlighted_text(spans: list[tuple]):
    """Render a single line of colored spans using imgui.text_colored + same_line."""
    first = True
    for color, text in spans:
        if not text:
            continue
        if not first:
            imgui.same_line(spacing=0)
        first = False
        imgui.text_colored(color, text)
    if first:
        # Empty line — still need to advance cursor
        imgui.text("")


def draw_output_log(log_lines: list[str], max_lines: int = 5000, label: str = "##log"):
    """Draw a scrollable monospace output log.

    Args:
        log_lines: List of strings to display.
        max_lines: Trim to this many lines (from the end).
        label: imgui child region label.
    """
    n = len(log_lines)
    if n > max_lines:
        del log_lines[: n - max_lines]
        n = max_lines

    imgui.begin_child(label, imgui.ImVec2(0, 0), child_flags=imgui.ChildFlags_.borders.value)
    clipper = imgui.ListClipper()
    clipper.begin(n)
    while clipper.step():
        for i in range(clipper.display_start, clipper.display_end):
            imgui.text(log_lines[i])
    clipper.end()

    # Auto-scroll to bottom when near bottom
    if imgui.get_scroll_y() >= imgui.get_scroll_max_y() - imgui.get_text_line_height() * 2:
        imgui.set_scroll_here_y(1.0)
    imgui.end_child()


def draw_status_bar(left_text: str, right_items: list[tuple] | None = None):
    """Draw a status bar at the bottom of the window.

    Args:
        left_text: Text shown on the left.
        right_items: Optional list of (text, ImVec4_color) for right-aligned items.
    """
    imgui.separator()
    imgui.text(left_text)

    if right_items:
        # Right-align items
        for text, color in reversed(right_items):
            text_width = imgui.calc_text_size(text).x + 16
            imgui.same_line(imgui.get_window_width() - text_width)
            imgui.text_colored(color, text)
