"""PapyrusSyntaxEditor — custom ImGui text editor with Papyrus syntax highlighting."""
from __future__ import annotations

import math
import re
import time
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from imgui_bundle import imgui as _imgui


# Everforest dark palette — RGBA float tuples for draw_list coloring
_C_LINE_NUM        = (0x7A/255, 0x84/255, 0x78/255, 1.0)  # grey0
_C_LINE_NUM_ACTIVE = (0xD3/255, 0xC6/255, 0xAA/255, 1.0)  # fg — brighter for active line
_C_CURRENT_LINE    = (0x29/255, 0x32/255, 0x36/255, 1.0)  # subtle tint, one step above editor bg
_C_MATCH_WORD      = (0x42/255, 0x50/255, 0x47/255, 0.55) # bg_green at 55% for occurrence highlights
_C_DIAG_ERROR      = (0xE6/255, 0x7E/255, 0x80/255, 1.0)  # red — Everforest red
_C_DIAG_WARNING    = (0xE6/255, 0x98/255, 0x75/255, 1.0)  # orange — Everforest orange
_C_CURSOR          = (0xD3/255, 0xC6/255, 0xAA/255, 1.0)  # fg
_C_SELECTION       = (0x42/255, 0x50/255, 0x47/255, 0.8)  # bg_green
_C_SEPARATOR       = (0x47/255, 0x52/255, 0x58/255, 1.0)  # bg3
_C_INDENT_GUIDE    = (0x47/255, 0x52/255, 0x58/255, 0.35) # bg3 at 35% — indent guides
_C_DEFAULT         = (0xD3/255, 0xC6/255, 0xAA/255, 1.0)  # fg
_C_BRACKET_MATCH   = (0x83/255, 0xC0/255, 0x92/255, 0.35) # aqua at 35% — bracket match highlight
_C_FIND_MATCH      = (0xDB/255, 0xBC/255, 0x7F/255, 0.35) # yellow at 35% — find match highlight
_C_FIND_CURRENT    = (0xDB/255, 0xBC/255, 0x7F/255, 0.65) # yellow at 65% — current find match
_C_WHITESPACE      = (0x47/255, 0x52/255, 0x58/255, 0.45) # bg3 at 45% — subtle space dots


_TOKEN_COLORS: dict | None = None
_TOKEN_COLORS_U32: dict | None = None   # TokenType -> u32, populated on first render
_FIXED_COLORS_U32: dict | None = None   # name -> u32, populated on first render


def _get_token_colors() -> dict:
    global _TOKEN_COLORS
    if _TOKEN_COLORS is None:
        from ui.papyrus.papyrus_tokenizer import TokenType as TT
        _TOKEN_COLORS = {  # Everforest dark
            TT.KEYWORD:       (0xE6/255, 0x7E/255, 0x80/255, 1.0),  # red
            TT.TYPE_NAME:     (0x83/255, 0xC0/255, 0x92/255, 1.0),  # aqua
            TT.BUILTIN_CONST: (0xE6/255, 0x98/255, 0x75/255, 1.0),  # orange
            TT.CLASS_NAME:    (0xDB/255, 0xBC/255, 0x7F/255, 1.0),  # yellow
            TT.FUNCTION_CALL: (0xA7/255, 0xC0/255, 0x80/255, 1.0),  # green
            TT.VARIABLE:      (0x7F/255, 0xBB/255, 0xB3/255, 1.0),  # blue
            TT.STRING:        (0xA7/255, 0xC0/255, 0x80/255, 1.0),  # green
            TT.COMMENT:       (0x85/255, 0x92/255, 0x89/255, 1.0),  # grey1
            TT.DOC_COMMENT:   (0x7F/255, 0xBB/255, 0xB3/255, 0.7),  # blue dimmed
            TT.NUMBER:        (0xD6/255, 0x99/255, 0xB6/255, 1.0),  # purple
            TT.OPERATOR:      (0x85/255, 0x92/255, 0x89/255, 1.0),  # grey1 — muted so identifiers/keywords pop
            TT.WHITESPACE:    _C_DEFAULT,
        }
    return _TOKEN_COLORS


def _get_token_colors_u32(imgui) -> dict:
    global _TOKEN_COLORS_U32
    if _TOKEN_COLORS_U32 is None:
        src = _get_token_colors()
        _TOKEN_COLORS_U32 = {
            ttype: imgui.color_convert_float4_to_u32(imgui.ImVec4(*rgba))
            for ttype, rgba in src.items()
        }
    return _TOKEN_COLORS_U32


def _get_fixed_colors_u32(imgui) -> dict:
    global _FIXED_COLORS_U32
    if _FIXED_COLORS_U32 is None:
        def _u32(c): return imgui.color_convert_float4_to_u32(imgui.ImVec4(*c))
        _FIXED_COLORS_U32 = {
            'sep':          _u32(_C_SEPARATOR),
            'line_num':     _u32(_C_LINE_NUM),
            'line_num_act': _u32(_C_LINE_NUM_ACTIVE),
            'sel':          _u32(_C_SELECTION),
            'cur_line':     _u32(_C_CURRENT_LINE),
            'match':        _u32(_C_MATCH_WORD),
            'diag_err':     _u32(_C_DIAG_ERROR),
            'diag_warn':    _u32(_C_DIAG_WARNING),
            'find_match':   _u32(_C_FIND_MATCH),
            'find_current': _u32(_C_FIND_CURRENT),
            'indent_guide': _u32(_C_INDENT_GUIDE),
            'cursor':       _u32(_C_CURSOR),
            'bracket':      _u32(_C_BRACKET_MATCH),
            'ghost':        _u32((0xD3/255, 0xC6/255, 0xAA/255, 0.35)),
            'default':      _u32(_C_DEFAULT),
            'whitespace':   _u32(_C_WHITESPACE),
        }
    return _FIXED_COLORS_U32


def _jumpable_ttypes():
    from ui.papyrus.papyrus_tokenizer import TokenType as TT
    return frozenset({TT.CLASS_NAME, TT.FUNCTION_CALL, TT.VARIABLE, TT.TYPE_NAME})

_JUMPABLE_TTYPES = _jumpable_ttypes()


class SetViewAtLineMode(Enum):
    always         = auto()  # always scroll to put line at top
    if_not_visible = auto()  # scroll only if outside visible range


@dataclass
class Cursor:
    line:        int
    col:         int
    anchor_line: int | None = None
    anchor_col:  int | None = None

    def has_selection(self) -> bool:
        return self.anchor_line is not None

    def clear_selection(self) -> None:
        self.anchor_line = None
        self.anchor_col = None

    def set_anchor(self) -> None:
        self.anchor_line = self.line
        self.anchor_col = self.col

    def selection_range(self) -> tuple[int, int, int, int] | None:
        """Return (start_line, start_col, end_line, end_col) or None."""
        if not self.has_selection():
            return None
        al, ac = self.anchor_line, self.anchor_col
        cl, cc = self.line, self.col
        if (al, ac) <= (cl, cc):
            return al, ac, cl, cc
        return cl, cc, al, ac


@dataclass
class UndoState:
    lines:   list[str]
    cursors: list[Cursor]


class _ImVec2Stub:
    """Test stub for imgui.ImVec2 — allows testing without an imgui context."""
    def __init__(self, x: float, y: float):
        self.x = x
        self.y = y


class PapyrusSyntaxEditor:
    """Custom ImGui Papyrus text editor widget, drop-in for te.TextEditor."""

    def __init__(self) -> None:
        self._lines: list[str] = ['']
        self._cursors: list[Cursor] = [Cursor(line=0, col=0)]
        self._read_only = False

        # Undo stack
        self._undo_stack: list[UndoState] = []
        self._undo_index: int = -1
        self._last_edit_time: float = 0.0
        self._pending_undo: bool = False

        # Scroll state
        self._scroll_line: int = 0
        self._scroll_x: float = 0.0

        # Custom focus tracking — immune to ImGui's internal focused-window state
        # changes caused by other panels (diagnostics table, suggestion list, etc.).
        self._editor_focused: bool = False

        # Token cache: line_index -> (line_text, tokens)
        self._token_cache: dict[int, tuple[str, list]] = {}
        # Pixel-width caches — measure once, reuse across frames.
        # _glyph_advance: char -> rendered advance (in current font), shared across lines.
        # _line_widths_cache: line_idx -> (line_text, prefix_widths) where
        # prefix_widths[col] is the pixel x offset of column `col` (length = len+1).
        # Both are invalidated when the font/scale changes (detected via _char_width).
        self._glyph_advance: dict[str, float] = {}
        self._line_widths_cache: dict[int, tuple[str, list[float]]] = {}
        self._cached_char_width: float = 0.0
        # True only inside an active ImGui frame. calc_text_size outside a frame is
        # an access violation (not a Python exception), so helpers gate measurement
        # on this flag and fall back to `_char_width` otherwise.
        self._in_frame: bool = False

        # Layout metrics — populated during render(), used by helpers
        self._char_width: float = 0.0
        self._line_height: float = 0.0
        self._gutter_width: float = 0.0
        self._text_origin_x: float = 0.0
        self._text_origin_y: float = 0.0
        self._visible_lines: int = 40

        # Mouse drag state
        self._dragging: bool = False

        # Inline suggestion (ghost text)
        self._inline_suggestion: str = ""

        # Diagnostic markers (set by caller each frame or on change)
        self._error_lines:   frozenset[int] = frozenset()
        self._warning_lines: frozenset[int] = frozenset()

        # Go-to-line popup
        self._goto_open: bool = False
        self._goto_line_buf: str = ""
        self._goto_focus: bool = False

        # Find & Replace state
        self._find_open: bool = False
        self._find_replace_open: bool = False  # Show replace bar too
        self._find_query: str = ""
        self._find_replace: str = ""
        self._find_case_sensitive: bool = False
        self._find_matches: list[tuple[int, int, int]] = []  # (line, start_col, end_col)
        self._find_index: int = -1  # Current highlighted match
        self._find_focus_input: bool = False  # Flag to focus input next frame

    # ---- Public API --------------------------------------------------------

    def set_text(self, text: str) -> None:
        self._lines = text.expandtabs(4).split('\n')
        self._clear_line_caches()
        self._clamp_cursors()

    def get_text(self) -> str:
        return '\n'.join(self._lines)

    def get_text_lines(self) -> list[str]:
        return list(self._lines)

    def set_text_lines(self, lines: list[str]) -> None:
        self._lines = [ln.expandtabs(4) for ln in lines]
        self._clear_line_caches()
        self._clamp_cursors()

    def get_cursor_position(self) -> tuple[int, int]:
        c = self._cursors[0]
        return c.line, c.col

    def set_cursor_position(self, line: int, col: int) -> None:
        self._cursors = [Cursor(line=self._clamp_line(line),
                                col=self._clamp_col(line, col))]
        self._scroll_to_cursor()

    def get_first_visible_line(self) -> int:
        return self._scroll_line

    def get_last_visible_line(self) -> int:
        return min(len(self._lines) - 1,
                   self._scroll_line + self._visible_lines)

    def get_line_count(self) -> int:
        return len(self._lines)

    def is_read_only_enabled(self) -> bool:
        return self._read_only

    def set_read_only_enabled(self, read_only: bool) -> None:
        self._read_only = read_only

    def get_undo_index(self) -> int:
        return self._undo_index

    def set_diagnostics(self, diagnostics: list) -> None:
        """Update error/warning line sets from a list of Diagnostic objects."""
        errors:   set[int] = set()
        warnings: set[int] = set()
        for d in diagnostics:
            if d.severity == "error":
                errors.add(d.line)
            else:
                warnings.add(d.line)
        self._error_lines   = frozenset(errors)
        self._warning_lines = frozenset(warnings)

    def set_inline_suggestion(self, text: str) -> None:
        """Show ghost text after the cursor (accepted with Tab, dismissed with Escape)."""
        self._inline_suggestion = text

    def clear_inline_suggestion(self) -> None:
        self._inline_suggestion = ""

    def get_cursor_screen_pos(self) -> tuple[float, float] | None:
        """Absolute screen position just below the primary cursor, for popup placement.
        Returns None before the first render or when the cursor is off-screen.
        """
        if self._line_height == 0.0:
            return None
        c = self._cursors[0]
        if not (self._scroll_line <= c.line < self._scroll_line + self._visible_lines):
            return None
        from imgui_bundle import imgui
        x = self._col_x(imgui, c.line, c.col)
        y = self._text_origin_y + (c.line - self._scroll_line + 1) * self._line_height
        return x, y

    def set_view_at_line(self, line: int,
                         mode: SetViewAtLineMode | None = None) -> None:
        if mode is None:
            mode = SetViewAtLineMode.if_not_visible
        if mode == SetViewAtLineMode.always:
            self._scroll_line = self._clamp_line(line)
        else:
            if line < self._scroll_line or line > self.get_last_visible_line():
                self._scroll_line = self._clamp_line(line)

    def get_word_at_screen_pos(self, pos: "_imgui.ImVec2 | _ImVec2Stub") -> str:
        line, col = self._screen_to_pos(pos.x, pos.y)
        return self._word_at_pos(line, col)

    def _get_cursor_word(self) -> str:
        """Return the identifier word under the primary cursor for occurrence highlighting.

        Returns '' when:
        - there is a non-empty selection
        - cursor is on whitespace/punctuation or a word < 2 chars
        - the token is a keyword, type, or builtin (only variables/functions/classes are highlighted)
        """
        from ui.papyrus.papyrus_tokenizer import TokenType as TT
        _HIGHLIGHT_TYPES = frozenset({TT.VARIABLE, TT.CLASS_NAME, TT.FUNCTION_CALL})

        c = self._cursors[0]
        # Ignore zero-width "selections" left by mouse drag tracking
        sel = c.selection_range()
        if sel and (sel[0] != sel[2] or sel[1] != sel[3]):
            return ''
        line_text = self._lines[c.line] if c.line < len(self._lines) else ''
        col = c.col
        ws = col
        while ws > 0 and (line_text[ws - 1].isalnum() or line_text[ws - 1] == '_'):
            ws -= 1
        we = col
        while we < len(line_text) and (line_text[we].isalnum() or line_text[we] == '_'):
            we += 1
        if we - ws < 2:
            return ''
        word = line_text[ws:we]
        # Only highlight if the token is a variable, function call, or class name
        for ts, te, ttype in self._tokenize_cached(c.line):
            if ts == ws and te == we:
                return word if ttype in _HIGHLIGHT_TYPES else ''
        return ''

    # Everforest bg_dim — darker than panel bg, clearly distinct
    _EDITOR_BG = (0x23/255, 0x2A/255, 0x2E/255, 1.0)
    _show_whitespace: bool = True

    def render(self, id: str, border: bool, size: "_imgui.ImVec2",
               read_only: bool = False, extra_line_spacing: float = 0.0) -> None:
        from imgui_bundle import imgui

        # --- begin child window ------------------------------------------
        imgui.push_style_color(imgui.Col_.child_bg,
                               imgui.ImVec4(*self._EDITOR_BG))
        child_flags = imgui.ChildFlags_.border if border else 0
        win_flags = (imgui.WindowFlags_.no_scroll_with_mouse
                     | imgui.WindowFlags_.no_scrollbar)
        imgui.begin_child(id, size, child_flags, win_flags)
        imgui.pop_style_color()
        try:
            self._render_body(imgui, size, read_only, extra_line_spacing)
        finally:
            imgui.end_child()

    def _get_line_prefix_widths(self, imgui, line_idx: int) -> list[float]:
        """Return prefix-pixel-widths for a line (length = len(text) + 1).
        pw[col] is the pixel x offset of column `col` from the line start.
        Per-glyph advances are cached across lines and frames; per-line prefix
        arrays are cached until the line text changes.
        """
        if line_idx < 0 or line_idx >= len(self._lines):
            return [0.0]
        text = self._lines[line_idx]
        cached = self._line_widths_cache.get(line_idx)
        if cached is not None and cached[0] == text:
            return cached[1]
        advances = self._glyph_advance
        cw = self._char_width
        can_measure = self._in_frame
        pw = [0.0]
        acc = 0.0
        for ch in text:
            w = advances.get(ch)
            if w is None:
                if can_measure:
                    w = imgui.calc_text_size(ch).x
                    if w <= 0:
                        w = cw
                    advances[ch] = w
                else:
                    w = cw  # fallback for tests / pre-frame calls
            acc += w
            pw.append(acc)
        if can_measure:
            self._line_widths_cache[line_idx] = (text, pw)
        return pw

    def _col_x(self, imgui, line_idx: int, col: int) -> float:
        """Convert a column index to pixel x using cached per-glyph advances.
        Pixel-accurate, O(1) per call after first measurement of the line.
        """
        pw = self._get_line_prefix_widths(imgui, line_idx)
        if col >= len(pw):
            col = len(pw) - 1
        elif col < 0:
            col = 0
        return self._text_origin_x + pw[col] - self._scroll_x

    def _render_body(self, imgui, size, read_only: bool,
                     extra_line_spacing: float = 0.0) -> None:
        # --- layout metrics ----------------------------------------------
        self._in_frame = True  # gate calc_text_size in helpers
        self._char_width  = imgui.calc_text_size("X").x
        # Font/scale change → blow away cached pixel widths so they get re-measured.
        if abs(self._cached_char_width - self._char_width) > 0.01:
            self._cached_char_width = self._char_width
            self._glyph_advance.clear()
            self._line_widths_cache.clear()
        self._line_height = imgui.get_text_line_height() + extra_line_spacing
        digit_count       = max(1, len(str(len(self._lines))))
        _diag_slot        = 8.0  # reserved on left of gutter for ! marker
        self._gutter_width = digit_count * self._char_width + 6.0 + _diag_slot

        # get_cursor_screen_pos() right after begin_child gives the absolute
        # top-left of the content area (equivalent to win_pos + content_min)
        content_origin = imgui.get_cursor_screen_pos()
        self._text_origin_x = content_origin.x + self._gutter_width + 2.0
        self._text_origin_y = content_origin.y
        self._visible_lines = max(1, int(size.y / self._line_height) + 1)

        draw_list = imgui.get_window_draw_list()
        font = imgui.get_font()
        font_size = imgui.get_font_size()

        # --- vertical scroll via mouse wheel -----------------------------
        io = imgui.get_io()
        if imgui.is_window_hovered():
            imgui.set_mouse_cursor(imgui.MouseCursor_.text_input)
            if io.key_shift:
                self._scroll_x = max(0.0, self._scroll_x - io.mouse_wheel * self._char_width * 3)
            else:
                max_scroll = max(0, len(self._lines) - self._visible_lines)
                self._scroll_line = max(0, min(
                    max_scroll,
                    self._scroll_line - int(io.mouse_wheel * 3),
                ))

        # --- draw gutter separator line ----------------------------------
        _fc = _get_fixed_colors_u32(imgui)
        sep_x = content_origin.x + self._gutter_width
        col_sep = _fc['sep']
        draw_list.add_line(
            imgui.ImVec2(sep_x, self._text_origin_y),
            imgui.ImVec2(sep_x, self._text_origin_y + size.y),
            col_sep, 1.0,
        )

        col_default      = _fc['default']
        col_line_num     = _fc['line_num']
        col_line_num_act = _fc['line_num_act']
        col_sel          = _fc['sel']
        col_cur_line     = _fc['cur_line']
        col_match        = _fc['match']
        col_diag_err     = _fc['diag_err']
        col_diag_warn    = _fc['diag_warn']
        col_find_match   = _fc['find_match']
        col_find_current = _fc['find_current']
        col_indent_guide = _fc['indent_guide']
        token_colors_u32 = _get_token_colors_u32(imgui)
        col_default_u32  = _fc['default']

        primary_line = self._cursors[0].line

        # Occurrence highlights only make sense when the cursor line is visible
        first_pre = self._scroll_line
        last_pre  = min(len(self._lines), first_pre + self._visible_lines + 1)
        match_word = self._get_cursor_word() if first_pre <= primary_line < last_pre else ''
        match_wlen = len(match_word)

        # --- draw visible lines ------------------------------------------
        first = self._scroll_line
        last  = min(len(self._lines), first + self._visible_lines + 1)

        for i in range(first, last):
            y = self._text_origin_y + (i - first) * self._line_height
            line_text = self._lines[i]

            # Current line background highlight
            if i == primary_line:
                draw_list.add_rect_filled(
                    imgui.ImVec2(content_origin.x, y),
                    imgui.ImVec2(content_origin.x + size.x, y + self._line_height),
                    col_cur_line,
                )

            # Indent guides — vertical lines at each 4-space indent level
            indent_level = (len(line_text) - len(line_text.lstrip(' '))) // 4 if line_text.strip() else 0
            if indent_level > 0:
                for lvl in range(1, indent_level + 1):
                    gx = self._text_origin_x + lvl * 4 * self._char_width - self._scroll_x - 2 * self._char_width
                    draw_list.add_line(
                        imgui.ImVec2(gx, y),
                        imgui.ImVec2(gx, y + self._line_height),
                        col_indent_guide, 1.0)

            # Occurrence highlights (word under cursor, whole-word matches)
            if match_word:
                pos = 0
                while True:
                    idx = line_text.find(match_word, pos)
                    if idx < 0:
                        break
                    before_ok = (idx == 0 or not (line_text[idx - 1].isalnum() or line_text[idx - 1] == '_'))
                    after_ok  = (idx + match_wlen >= len(line_text) or not (line_text[idx + match_wlen].isalnum() or line_text[idx + match_wlen] == '_'))
                    if before_ok and after_ok:
                        rx0 = self._col_x(imgui, i, idx)
                        rx1 = self._col_x(imgui, i, idx + match_wlen)
                        draw_list.add_rect_filled(
                            imgui.ImVec2(rx0, y),
                            imgui.ImVec2(rx1, y + self._line_height),
                            col_match,
                        )
                    pos = idx + 1

            # Find match highlights
            if self._find_open and self._find_matches:
                for j, (ml, mc, me) in enumerate(self._find_matches):
                    if ml != i:
                        continue
                    rx0 = self._col_x(imgui, i, mc)
                    rx1 = self._col_x(imgui, i, me)
                    is_current = (j == self._find_index)
                    draw_list.add_rect_filled(
                        imgui.ImVec2(rx0, y),
                        imgui.ImVec2(rx1, y + self._line_height),
                        col_find_current if is_current else col_find_match,
                    )

            # Selection highlight
            for c in self._cursors:
                sel = c.selection_range()
                if sel is None:
                    continue
                sl, sc, el, ec = sel
                if i < sl or i > el:
                    continue
                xs = sc if i == sl else 0
                rx0 = self._col_x(imgui, i, xs)
                if sl == el:
                    # Same-line selection: use exact text width
                    rx1 = self._col_x(imgui, i, ec)
                elif i == el:
                    # End line: from left edge to ec
                    rx1 = self._col_x(imgui, i, ec)
                else:
                    # Start line or intermediate line: extend to right editor edge
                    rx1 = content_origin.x + size.x
                draw_list.add_rect_filled(
                    imgui.ImVec2(rx0, y),
                    imgui.ImVec2(rx1, y + self._line_height),
                    col_sel,
                )

            # Diagnostic marker (! in left gutter slot) + squiggle underline
            is_err  = i in self._error_lines
            is_warn = not is_err and i in self._warning_lines
            if is_err or is_warn:
                col_marker = col_diag_err if is_err else col_diag_warn
                draw_list.add_text(imgui.ImVec2(content_origin.x + 2.0, y), col_marker, "!")
                if is_err and line_text.strip():
                    # Wavy underline (squiggle) across the line text
                    rstripped_len = len(line_text.rstrip())
                    txt_w = self._get_line_prefix_widths(imgui, i)[rstripped_len]
                    sy = y + font_size + 1.0
                    sx = self._text_origin_x - self._scroll_x
                    ex = sx + txt_w
                    amp, step = 1.5, 3.0
                    x = sx
                    toggle = True
                    while x < ex:
                        xn = min(x + step, ex)
                        y0 = sy - amp if toggle else sy + amp
                        y1 = sy + amp if toggle else sy - amp
                        draw_list.add_line(imgui.ImVec2(x, y0), imgui.ImVec2(xn, y1),
                                           col_diag_err, 1.0)
                        x = xn
                        toggle = not toggle

            # Line number in gutter (active line uses brighter color)
            ln_str = str(i + 1)
            ln_x = content_origin.x + self._gutter_width - len(ln_str) * self._char_width - 4.0
            draw_list.add_text(imgui.ImVec2(ln_x, y),
                               col_line_num_act if i == primary_line else col_line_num,
                               ln_str)

            # Cached prefix widths for this line — used for whitespace dots and tokens.
            prefix_widths = self._get_line_prefix_widths(imgui, i)
            text_x_base = self._text_origin_x - self._scroll_x

            # Whitespace dots — subtle space markers drawn before token text
            if self._show_whitespace and ' ' in line_text:
                col_ws = _fc['whitespace']
                dot_y = y + self._line_height * 0.5
                for col, ch in enumerate(line_text):
                    if ch == ' ':
                        cx = (prefix_widths[col] + prefix_widths[col + 1]) * 0.5
                        draw_list.add_circle_filled(
                            imgui.ImVec2(text_x_base + cx, dot_y), 0.8, col_ws)

            # Syntax-highlighted text — pixel-accurate using cached prefix widths.
            for start, end, ttype in self._tokenize_cached(i):
                tok_text = line_text[start:end]
                if tok_text.strip():
                    draw_list.add_text(imgui.ImVec2(text_x_base + prefix_widths[start], y),
                                       token_colors_u32.get(ttype, col_default_u32),
                                       tok_text)

        # --- cursor bar(s) -----------------------------------------------
        show = math.fmod(imgui.get_time(), 1.0) < 0.5
        if show:
            col_cur = _fc['cursor']
            for c in self._cursors:
                if first <= c.line < last:
                    cy = self._text_origin_y + (c.line - first) * self._line_height
                    cx = self._col_x(imgui, c.line, c.col)
                    draw_list.add_rect_filled(
                        imgui.ImVec2(cx, cy),
                        imgui.ImVec2(cx + 1.5, cy + self._line_height),
                        col_cur,
                    )

        # --- bracket matching highlight ------------------------------------
        col_bracket = _fc['bracket']
        c0 = self._cursors[0]
        # Check character at cursor and one before cursor
        for check_col in (c0.col, c0.col - 1):
            if check_col < 0:
                continue
            match = self._find_matching_bracket(c0.line, check_col)
            if match is not None:
                # Highlight both brackets
                for bl, bc in ((c0.line, check_col), match):
                    if first <= bl < last:
                        by = self._text_origin_y + (bl - first) * self._line_height
                        bx = self._col_x(imgui, bl, bc)
                        bw = self._char_width
                        draw_list.add_rect(
                            imgui.ImVec2(bx, by),
                            imgui.ImVec2(bx + bw, by + self._line_height),
                            col_bracket, 0.0, 0, 1.0,
                        )
                break  # Only highlight first match found

        # --- inline suggestion (ghost text) ---------------------------------
        if self._inline_suggestion:
            c0 = self._cursors[0]
            if first <= c0.line < last:
                gy = self._text_origin_y + (c0.line - first) * self._line_height
                gx = self._col_x(imgui, c0.line, c0.col)
                col_ghost = _fc['ghost']
                draw_list.add_text(imgui.ImVec2(gx, gy), col_ghost, self._inline_suggestion)

        # --- Ctrl+hover underline (go-to-definition affordance) ----------
        if io.key_ctrl and imgui.is_window_hovered():
            mp = io.mouse_pos
            hover_line = self._scroll_line + int(
                (mp.y - self._text_origin_y) / max(self._line_height, 1.0)
            )
            if first <= hover_line < last:
                hover_col = self._x_to_col(imgui, hover_line,
                                           mp.x - self._text_origin_x + self._scroll_x)
                line_text = self._lines[hover_line]
                # Expand to word boundaries
                ws = hover_col
                while ws > 0 and (line_text[ws - 1].isalnum() or line_text[ws - 1] == '_'):
                    ws -= 1
                we = hover_col
                while we < len(line_text) and (line_text[we].isalnum() or line_text[we] == '_'):
                    we += 1
                if ws < we:
                    # Check if this word's token type is jumpable
                    col_ul = None
                    for ts, te, ttype in self._tokenize_cached(hover_line):
                        if ts == ws and te == we and ttype in _JUMPABLE_TTYPES:
                            col_ul = token_colors_u32.get(ttype)
                            break
                    if col_ul is not None:
                        ux0 = self._col_x(imgui, hover_line, ws)
                        ux1 = self._col_x(imgui, hover_line, we)
                        uy = (self._text_origin_y
                              + (hover_line - first) * self._line_height
                              + font_size + 1.0)
                        draw_list.add_line(
                            imgui.ImVec2(ux0, uy), imgui.ImVec2(ux1, uy), col_ul, 1.0
                        )
                        imgui.set_mouse_cursor(imgui.MouseCursor_.hand)

        # --- Find/Replace overlay bar ---------------------------------------
        if self._find_open:
            self._render_find_bar(imgui, content_origin, size)

        # --- Go-to-line popup -----------------------------------------------
        if self._goto_open:
            self._render_goto_line(imgui, content_origin, size)

        # --- idle undo commit check --------------------------------------
        self._check_idle_commit()

        # --- input handling --------------------------------------------------
        # Track focus via mouse clicks rather than imgui.is_window_focused().
        # is_window_focused() drops to False when unrelated panels (diagnostics
        # table, suggestion list) update ImGui's internal NavWindow state.
        is_hovered = imgui.is_window_hovered(imgui.HoveredFlags_.child_windows)
        if imgui.is_mouse_clicked(0) or imgui.is_mouse_clicked(1):
            self._editor_focused = is_hovered
        if self._editor_focused and not self._read_only:
            self._handle_keyboard(io)
            self._handle_mouse(io)

    # ---- Edit primitives (all cursors) ------------------------------------

    def _delete_selection(self, cursor: Cursor) -> bool:
        """Delete the selection for a cursor. Returns True if something was deleted."""
        sel = cursor.selection_range()
        if sel is None:
            return False
        sl, sc, el, ec = sel
        if sl == el:
            self._lines[sl] = self._lines[sl][:sc] + self._lines[sl][ec:]
            self._invalidate_line_cache(sl)
        else:
            new_line = self._lines[sl][:sc] + self._lines[el][ec:]
            self._lines[sl:el + 1] = [new_line]
            # Invalidate all affected lines
            for ln in range(sl, el + 1):
                self._invalidate_line_cache(ln)
        cursor.line = sl
        cursor.col = sc
        cursor.clear_selection()
        return True

    def _insert_text_at_cursor(self, cursor: Cursor, text: str) -> None:
        """Insert text at cursor position (after deleting any selection)."""
        self._delete_selection(cursor)
        line_text = self._lines[cursor.line]
        col = cursor.col
        if '\n' not in text:
            self._lines[cursor.line] = line_text[:col] + text + line_text[col:]
            self._invalidate_line_cache(cursor.line)
            cursor.col += len(text)
        else:
            parts = text.split('\n')
            first_part = line_text[:col] + parts[0]
            last_part = parts[-1] + line_text[col:]
            middle = parts[1:-1]
            replacement = [first_part] + middle + [last_part]
            self._lines[cursor.line:cursor.line + 1] = replacement
            for ln in range(cursor.line, cursor.line + len(replacement)):
                self._invalidate_line_cache(ln)
            cursor.line += len(parts) - 1
            cursor.col = len(parts[-1])

    # Auto-indent patterns (reused from papyrus_formatter)
    _INDENT_AFTER = re.compile(
        r'^(function|event|state|if|while|else|elseif|group)\b', re.IGNORECASE)
    _DEDENT_BEFORE = re.compile(
        r'^(else|elseif|endif|endwhile|endfunction|endevent|endstate|endproperty|endgroup)\b',
        re.IGNORECASE)
    _PROPERTY_AUTO = re.compile(r'^property\b.*\bauto\b\s*$', re.IGNORECASE)

    def _get_affected_line_range(self) -> tuple[int, int]:
        """Return (first, last) line indices affected by primary cursor/selection."""
        c = self._cursors[0]
        sel = c.selection_range()
        if sel:
            return sel[0], sel[2]
        return c.line, c.line

    def _move_lines(self, direction: int) -> None:
        """Move affected line(s) up (-1) or down (+1)."""
        first, last = self._get_affected_line_range()
        if direction < 0 and first == 0:
            return
        if direction > 0 and last >= len(self._lines) - 1:
            return
        self._push_undo()
        if direction < 0:
            moved = self._lines.pop(first - 1)
            self._lines.insert(last, moved)
            for c in self._cursors:
                c.line -= 1
                if c.anchor_line is not None:
                    c.anchor_line -= 1
        else:
            moved = self._lines.pop(last + 1)
            self._lines.insert(first, moved)
            for c in self._cursors:
                c.line += 1
                if c.anchor_line is not None:
                    c.anchor_line += 1
        self._clear_line_caches()

    def _duplicate_lines(self) -> None:
        """Duplicate affected line(s) below."""
        first, last = self._get_affected_line_range()
        self._push_undo()
        dup = self._lines[first:last + 1]
        self._lines[last + 1:last + 1] = dup
        count = last - first + 1
        for c in self._cursors:
            c.line += count
            if c.anchor_line is not None:
                c.anchor_line += count
        self._clear_line_caches()

    def _toggle_comment(self) -> None:
        """Toggle '; ' comment prefix on affected lines."""
        c = self._cursors[0]
        sel = c.selection_range()
        if sel:
            start_line, _, end_line, _ = sel
        else:
            start_line = end_line = c.line

        # Check if all non-empty lines are already commented
        lines_range = range(start_line, end_line + 1)
        all_commented = all(
            self._lines[i].lstrip().startswith(';')
            for i in lines_range if self._lines[i].strip()
        )

        for i in lines_range:
            line = self._lines[i]
            if all_commented:
                # Uncomment: remove first '; ' or ';'
                stripped = line.lstrip(' ')
                leading = len(line) - len(stripped)
                if stripped.startswith('; '):
                    self._lines[i] = line[:leading] + stripped[2:]
                elif stripped.startswith(';'):
                    self._lines[i] = line[:leading] + stripped[1:]
            else:
                if line.strip():  # Don't comment blank lines
                    leading = len(line) - len(line.lstrip(' '))
                    self._lines[i] = line[:leading] + '; ' + line[leading:]
            self._invalidate_line_cache(i)

    def _insert_newline_auto_indent(self, cursor: Cursor) -> None:
        """Insert newline with smart indentation based on surrounding context."""
        self._delete_selection(cursor)
        prev_line = self._lines[cursor.line]
        stripped = prev_line.strip()

        # Copy indentation from current line
        leading = len(prev_line) - len(prev_line.lstrip(' '))
        indent = prev_line[:leading]

        # Increase indent after block openers
        if stripped and (self._INDENT_AFTER.match(stripped)
                         or self._PROPERTY_AUTO.match(stripped)):
            indent += '    '

        self._insert_text_at_cursor(cursor, '\n' + indent)

    _AUTO_PAIRS = {'(': ')', '[': ']', '{': '}', '"': '"'}

    def _backspace_at_cursor(self, cursor: Cursor) -> None:
        if self._delete_selection(cursor):
            return
        if cursor.col > 0:
            line = self._lines[cursor.line]
            # Auto-pair: delete both if cursor is between an empty pair
            before = line[cursor.col - 1]
            after = line[cursor.col] if cursor.col < len(line) else ''
            if before in self._AUTO_PAIRS and self._AUTO_PAIRS[before] == after:
                self._lines[cursor.line] = line[:cursor.col - 1] + line[cursor.col + 1:]
                self._invalidate_line_cache(cursor.line)
                cursor.col -= 1
                return
            self._lines[cursor.line] = line[:cursor.col - 1] + line[cursor.col:]
            self._invalidate_line_cache(cursor.line)
            cursor.col -= 1
        elif cursor.line > 0:
            prev = self._lines[cursor.line - 1]
            curr = self._lines[cursor.line]
            cursor.col = len(prev)
            cursor.line -= 1
            self._lines[cursor.line] = prev + curr
            self._lines.pop(cursor.line + 1)
            self._invalidate_line_cache(cursor.line)

    def _delete_at_cursor(self, cursor: Cursor) -> None:
        if self._delete_selection(cursor):
            return
        line = self._lines[cursor.line]
        if cursor.col < len(line):
            self._lines[cursor.line] = line[:cursor.col] + line[cursor.col + 1:]
            self._invalidate_line_cache(cursor.line)
        elif cursor.line < len(self._lines) - 1:
            next_line = self._lines[cursor.line + 1]
            self._lines[cursor.line] = line + next_line
            self._lines.pop(cursor.line + 1)
            self._invalidate_line_cache(cursor.line)

    def _delete_word_left(self, cursor: Cursor) -> None:
        if self._delete_selection(cursor):
            return
        line = self._lines[cursor.line]
        col = cursor.col
        while col > 0 and not (line[col - 1].isalnum() or line[col - 1] == '_'):
            col -= 1
        while col > 0 and (line[col - 1].isalnum() or line[col - 1] == '_'):
            col -= 1
        self._lines[cursor.line] = line[:col] + line[cursor.col:]
        self._invalidate_line_cache(cursor.line)
        cursor.col = col

    def _delete_word_right(self, cursor: Cursor) -> None:
        if self._delete_selection(cursor):
            return
        line = self._lines[cursor.line]
        col = cursor.col
        while col < len(line) and not (line[col].isalnum() or line[col] == '_'):
            col += 1
        while col < len(line) and (line[col].isalnum() or line[col] == '_'):
            col += 1
        self._lines[cursor.line] = line[:cursor.col] + line[col:]
        self._invalidate_line_cache(cursor.line)

    # ---- Clipboard ----------------------------------------------------------

    def _copy(self) -> None:
        from imgui_bundle import imgui
        parts = []
        for c in self._cursors:
            sel = c.selection_range()
            if sel is None:
                continue
            sl, sc, el, ec = sel
            if sl == el:
                parts.append(self._lines[sl][sc:ec])
            else:
                chunk = self._lines[sl][sc:]
                for ln in range(sl + 1, el):
                    chunk += '\n' + self._lines[ln]
                chunk += '\n' + self._lines[el][:ec]
                parts.append(chunk)
        if parts:
            imgui.set_clipboard_text('\n'.join(parts))

    def _cut(self) -> None:
        self._copy()
        self._push_undo()
        for c in self._cursors:
            self._delete_selection(c)
        self._dedup_cursors()

    def _paste(self) -> None:
        from imgui_bundle import imgui
        text = imgui.get_clipboard_text()
        if not text:
            return
        self._push_undo()
        for c in self._cursors:
            self._insert_text_at_cursor(c, text)
        self._dedup_cursors()
        self._scroll_to_cursor()

    # ---- Cursor movement ---------------------------------------------------

    def _move_cursor(self, cursor: Cursor, dline: int, dcol: int,
                     extend: bool) -> None:
        """Move by delta, optionally extending selection."""
        if not extend:
            cursor.clear_selection()
        elif not cursor.has_selection():
            cursor.set_anchor()

        cursor.line = self._clamp_line(cursor.line + dline)
        if dline != 0:
            cursor.col = self._clamp_col(cursor.line, cursor.col)
        else:
            cursor.col = self._clamp_col(cursor.line, cursor.col + dcol)

    def _move_cursor_word(self, cursor: Cursor, direction: int,
                          extend: bool) -> None:
        """Move to next word boundary (direction = +1 or -1)."""
        if not extend:
            cursor.clear_selection()
        elif not cursor.has_selection():
            cursor.set_anchor()

        line = self._lines[cursor.line]
        col = cursor.col
        if direction > 0:
            while col < len(line) and not (line[col].isalnum() or line[col] == '_'):
                col += 1
            while col < len(line) and (line[col].isalnum() or line[col] == '_'):
                col += 1
        else:
            while col > 0 and not (line[col - 1].isalnum() or line[col - 1] == '_'):
                col -= 1
            while col > 0 and (line[col - 1].isalnum() or line[col - 1] == '_'):
                col -= 1
        cursor.col = col

    def _handle_keyboard(self, io) -> None:
        from imgui_bundle import imgui

        # Undo / Redo
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.z):
            if io.key_shift:
                self.redo()
            else:
                self.undo()
            return
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.y):
            self.redo()
            return

        shift = io.key_shift
        ctrl  = io.key_ctrl

        # Navigation
        if imgui.is_key_pressed(imgui.Key.left_arrow):
            for c in self._cursors:
                if not ctrl:
                    self._move_cursor(c, 0, -1, shift)
                else:
                    self._move_cursor_word(c, -1, shift)
            self._scroll_to_cursor()
            return
        if imgui.is_key_pressed(imgui.Key.right_arrow):
            for c in self._cursors:
                if not ctrl:
                    self._move_cursor(c, 0, 1, shift)
                else:
                    self._move_cursor_word(c, 1, shift)
            self._scroll_to_cursor()
            return
        if imgui.is_key_pressed(imgui.Key.up_arrow):
            if ctrl and io.key_alt:
                # Add cursor above
                c0 = self._cursors[0]
                new_line = max(0, c0.line - 1)
                new_col = self._clamp_col(new_line, c0.col)
                self._cursors.append(Cursor(line=new_line, col=new_col))
                self._dedup_cursors()
                return
            elif io.key_alt and not ctrl:
                # Alt+Up: move line(s) up
                self._move_lines(-1)
                self._scroll_to_cursor()
                return
            else:
                for c in self._cursors:
                    self._move_cursor(c, -1, 0, shift)
                self._scroll_to_cursor()
            return
        if imgui.is_key_pressed(imgui.Key.down_arrow):
            if ctrl and io.key_alt:
                # Add cursor below
                c0 = self._cursors[0]
                new_line = min(len(self._lines) - 1, c0.line + 1)
                new_col = self._clamp_col(new_line, c0.col)
                self._cursors.append(Cursor(line=new_line, col=new_col))
                self._dedup_cursors()
                return
            elif io.key_alt and not ctrl:
                # Alt+Down: move line(s) down
                self._move_lines(1)
                self._scroll_to_cursor()
                return
            else:
                for c in self._cursors:
                    self._move_cursor(c, 1, 0, shift)
                self._scroll_to_cursor()
            return
        if imgui.is_key_pressed(imgui.Key.home):
            for c in self._cursors:
                if not shift:
                    c.clear_selection()
                elif not c.has_selection():
                    c.set_anchor()
                if ctrl:
                    c.line, c.col = 0, 0
                else:
                    c.col = 0
            self._scroll_to_cursor()
            return
        if imgui.is_key_pressed(imgui.Key.end):
            for c in self._cursors:
                if not shift:
                    c.clear_selection()
                elif not c.has_selection():
                    c.set_anchor()
                if ctrl:
                    c.line = len(self._lines) - 1
                c.col = len(self._lines[c.line])
            self._scroll_to_cursor()
            return

        # Editing
        if imgui.is_key_pressed(imgui.Key.enter) or imgui.is_key_pressed(imgui.Key.keypad_enter):
            self._inline_suggestion = ""
            self._push_undo()
            for c in self._cursors:
                self._insert_newline_auto_indent(c)
            self._dedup_cursors()
            self._scroll_to_cursor()
            return
        if imgui.is_key_pressed(imgui.Key.tab):
            if self._inline_suggestion:
                # Accept inline suggestion at primary cursor
                self._push_undo()
                self._insert_text_at_cursor(self._cursors[0], self._inline_suggestion)
                self._inline_suggestion = ""
                self._dedup_cursors()
                self._scroll_to_cursor()
            else:
                for c in self._cursors:
                    self._insert_text_at_cursor(c, '    ')
                self._mark_typing()
            return
        if imgui.is_key_pressed(imgui.Key.backspace):
            self._inline_suggestion = ""
            if ctrl:
                self._push_undo()
                for c in self._cursors:
                    self._delete_word_left(c)
            else:
                for c in self._cursors:
                    self._backspace_at_cursor(c)
                self._mark_typing()
            self._dedup_cursors()
            return
        if imgui.is_key_pressed(imgui.Key.delete):
            self._inline_suggestion = ""
            if ctrl:
                self._push_undo()
                for c in self._cursors:
                    self._delete_word_right(c)
            else:
                for c in self._cursors:
                    self._delete_at_cursor(c)
                self._mark_typing()
            self._dedup_cursors()
            return

        # Ctrl+Shift+D — duplicate line(s)
        if ctrl and shift and imgui.is_key_pressed(imgui.Key.d):
            self._duplicate_lines()
            return

        # Ctrl+/ — toggle line comment
        if ctrl and imgui.is_key_pressed(imgui.Key.slash):
            self._push_undo()
            self._toggle_comment()
            return

        # Ctrl+G — go to line
        if ctrl and imgui.is_key_pressed(imgui.Key.g):
            self._goto_open = True
            self._goto_line_buf = ""
            self._goto_focus = True
            return

        # Ctrl+F — open find bar
        if ctrl and not shift and imgui.is_key_pressed(imgui.Key.f):
            self._find_open = True
            self._find_replace_open = False
            self._find_focus_input = True
            # Pre-fill with selected text
            c0 = self._cursors[0]
            sel = c0.selection_range()
            if sel and sel[0] == sel[2]:  # single-line selection
                self._find_query = self._lines[sel[0]][sel[1]:sel[3]]
            return

        # Ctrl+H — open find+replace bar
        if ctrl and imgui.is_key_pressed(imgui.Key.h):
            self._find_open = True
            self._find_replace_open = True
            self._find_focus_input = True
            return

        # F3 / Shift+F3 — next/prev match
        if imgui.is_key_pressed(imgui.Key.f3):
            if self._find_matches:
                if shift:
                    self._find_index = (self._find_index - 1) % len(self._find_matches)
                else:
                    self._find_index = (self._find_index + 1) % len(self._find_matches)
                self._find_jump_to_match()
            return

        # Ctrl+A — select all
        if ctrl and imgui.is_key_pressed(imgui.Key.a):
            last = len(self._lines) - 1
            self._cursors = [Cursor(
                line=last, col=len(self._lines[last]),
                anchor_line=0, anchor_col=0,
            )]
            return

        # Clipboard
        if ctrl and imgui.is_key_pressed(imgui.Key.c):
            self._copy()
            return
        if ctrl and imgui.is_key_pressed(imgui.Key.x):
            self._cut()
            return
        if ctrl and imgui.is_key_pressed(imgui.Key.v):
            self._paste()
            return

        # Escape — dismiss find bar > inline suggestion > multi-cursor
        if imgui.is_key_pressed(imgui.Key.escape):
            if self._find_open:
                self._find_open = False
                self._find_replace_open = False
                self._find_matches.clear()
                return
            if self._inline_suggestion:
                self._inline_suggestion = ""
                return
            self._cursors = [self._cursors[0]]
            self._cursors[0].clear_selection()
            return

        # Printable character input
        if not ctrl and not io.key_alt:
            _typed = False
            for i in range(io.input_queue_characters.size()):
                char_code = io.input_queue_characters[i]
                if 32 <= char_code < 0x10000:
                    _typed = True
                    self._inline_suggestion = ""
                    ch = chr(char_code)
                    # Auto-pair: skip over closing bracket/quote if already there
                    if ch in ')]}"':
                        c0 = self._cursors[0]
                        ln = self._lines[c0.line]
                        if c0.col < len(ln) and ln[c0.col] == ch:
                            for c in self._cursors:
                                c.col = min(c.col + 1, len(self._lines[c.line]))
                            self._mark_typing()
                            continue
                    # Auto-pair: insert closer after opener
                    _PAIR_MAP = {'(': ')', '[': ']', '{': '}', '"': '"'}
                    if ch in _PAIR_MAP:
                        closer = _PAIR_MAP[ch]
                        for c in self._cursors:
                            self._insert_text_at_cursor(c, ch + closer)
                            c.col -= 1  # Place cursor between the pair
                    else:
                        for c in self._cursors:
                            self._insert_text_at_cursor(c, ch)
                    self._mark_typing()
            if _typed:
                self._dedup_cursors()
                self._scroll_to_cursor()

    def _handle_mouse(self, io) -> None:
        from imgui_bundle import imgui

        # Only process if hovering the text area (not gutter)
        if not imgui.is_window_hovered():
            if not self._dragging:
                return

        mp = imgui.get_mouse_pos()
        # Pass absolute screen coords — _screen_to_pos subtracts _text_origin_x/y
        # which are also in absolute screen space (set from win_pos in render()).
        line, col = self._screen_to_pos(mp.x, mp.y)

        left_pressed  = imgui.is_mouse_clicked(0)
        left_down     = imgui.is_mouse_down(0)
        left_released = imgui.is_mouse_released(0)

        if left_pressed:
            if io.key_alt:
                # Alt+click: add cursor
                self._cursors.append(Cursor(line=line, col=col))
                self._dedup_cursors()
            elif io.key_shift:
                # Shift+click: extend primary selection
                c = self._cursors[0]
                if not c.has_selection():
                    c.set_anchor()
                c.line = line
                c.col = col
            else:
                # Normal click: move primary cursor
                self._cursors = [Cursor(line=line, col=col)]

                # Double-click: select word
                if imgui.is_mouse_double_clicked(0):
                    c = self._cursors[0]
                    text = self._lines[line]
                    if col < len(text) and (text[col].isalnum() or text[col] == '_'):
                        start = col
                        while start > 0 and (text[start-1].isalnum() or text[start-1] == '_'):
                            start -= 1
                        end = col
                        while end < len(text) and (text[end].isalnum() or text[end] == '_'):
                            end += 1
                        c.anchor_line = line
                        c.anchor_col = start
                        c.col = end

            self._dragging = True

        elif left_down and self._dragging:
            # Drag: extend primary cursor selection
            c = self._cursors[0]
            if not c.has_selection():
                c.set_anchor()
            c.line = line
            c.col = col

        if left_released:
            self._dragging = False

    # ---- Internal helpers --------------------------------------------------

    def _clamp_line(self, line: int) -> int:
        return max(0, min(line, len(self._lines) - 1))

    def _clamp_col(self, line: int, col: int) -> int:
        ln = self._clamp_line(line)
        return max(0, min(col, len(self._lines[ln])))

    def _clamp_cursors(self) -> None:
        for c in self._cursors:
            c.line = self._clamp_line(c.line)
            c.col = self._clamp_col(c.line, c.col)
            if c.anchor_line is not None:
                c.anchor_line = self._clamp_line(c.anchor_line)
                c.anchor_col = self._clamp_col(c.anchor_line, c.anchor_col or 0)

    def _scroll_to_cursor(self) -> None:
        c = self._cursors[0]
        if c.line < self._scroll_line:
            self._scroll_line = c.line
        elif c.line >= self._scroll_line + self._visible_lines:
            self._scroll_line = max(0, c.line - self._visible_lines + 1)

    def _screen_to_pos(self, x: float, y: float) -> tuple[int, int]:
        if self._line_height <= 0:
            return 0, 0
        from imgui_bundle import imgui
        line = self._scroll_line + int((y - self._text_origin_y) / self._line_height)
        line = self._clamp_line(line)
        rel_x = x - self._text_origin_x + self._scroll_x
        col = self._x_to_col(imgui, line, rel_x)
        col = self._clamp_col(line, col)
        return line, col

    def _x_to_col(self, imgui, line_idx: int, rel_x: float) -> int:
        """Convert a pixel offset from text origin to the nearest character column.
        Uses cached prefix widths for pixel-accurate hit testing.
        """
        if rel_x <= 0:
            return 0
        pw = self._get_line_prefix_widths(imgui, line_idx)
        n = len(pw)
        for i in range(1, n):
            if pw[i] >= rel_x:
                return i - 1 if (rel_x - pw[i - 1]) < (pw[i] - rel_x) else i
        return n - 1

    def _word_at_pos(self, line: int, col: int) -> str:
        if line < 0 or line >= len(self._lines):
            return ''
        text = self._lines[line]
        if col < 0 or col >= len(text):
            return ''
        if not (text[col].isalnum() or text[col] == '_'):
            return ''
        start = col
        while start > 0 and (text[start - 1].isalnum() or text[start - 1] == '_'):
            start -= 1
        end = col + 1
        while end < len(text) and (text[end].isalnum() or text[end] == '_'):
            end += 1
        return text[start:end]

    def _dedup_cursors(self) -> None:
        self._cursors.sort(key=lambda c: (c.line, c.col))
        seen: set[tuple[int, int]] = set()
        unique = []
        for c in self._cursors:
            key = (c.line, c.col)
            if key not in seen:
                seen.add(key)
                unique.append(c)
        self._cursors = unique if unique else [Cursor(0, 0)]

    def _invalidate_line_cache(self, line: int) -> None:
        self._token_cache.pop(line, None)
        self._line_widths_cache.pop(line, None)

    def _clear_line_caches(self) -> None:
        """Drop all per-line caches (used on whole-buffer changes)."""
        self._token_cache.clear()
        self._line_widths_cache.clear()

    # ---- Find & Replace public API ----------------------------------------

    def open_find(self) -> None:
        """Open the find bar (equivalent to Ctrl+F)."""
        self._find_open = True
        self._find_replace_open = False
        self._find_focus_input = True

    def open_find_replace(self) -> None:
        """Open the find+replace bar (equivalent to Ctrl+H)."""
        self._find_open = True
        self._find_replace_open = True
        self._find_focus_input = True

    # ---- Find & Replace helpers ------------------------------------------

    def _find_update_matches(self) -> None:
        """Rebuild match list from current query."""
        self._find_matches.clear()
        self._find_index = -1
        if not self._find_query:
            return
        query = self._find_query
        if not self._find_case_sensitive:
            query = query.lower()
        qlen = len(query)
        for i, line in enumerate(self._lines):
            text = line if self._find_case_sensitive else line.lower()
            pos = 0
            while True:
                idx = text.find(query, pos)
                if idx < 0:
                    break
                self._find_matches.append((i, idx, idx + qlen))
                pos = idx + 1
        # Auto-select nearest match to cursor
        if self._find_matches:
            c0 = self._cursors[0]
            for j, (ml, mc, _) in enumerate(self._find_matches):
                if (ml, mc) >= (c0.line, c0.col):
                    self._find_index = j
                    return
            self._find_index = 0

    def _find_jump_to_match(self) -> None:
        """Move cursor to the current match and scroll it into view."""
        if 0 <= self._find_index < len(self._find_matches):
            ml, mc, me = self._find_matches[self._find_index]
            self._cursors = [Cursor(line=ml, col=me, anchor_line=ml, anchor_col=mc)]
            self.set_view_at_line(ml, SetViewAtLineMode.if_not_visible)

    def _find_replace_current(self) -> None:
        """Replace the current match and advance to next."""
        if not (0 <= self._find_index < len(self._find_matches)):
            return
        ml, mc, me = self._find_matches[self._find_index]
        self._push_undo()
        self._lines[ml] = self._lines[ml][:mc] + self._find_replace + self._lines[ml][me:]
        self._invalidate_line_cache(ml)
        self._find_update_matches()
        if self._find_matches:
            self._find_index = min(self._find_index, len(self._find_matches) - 1)
            self._find_jump_to_match()

    def _find_replace_all(self) -> None:
        """Replace all matches."""
        if not self._find_matches:
            return
        self._push_undo()
        # Process in reverse order to preserve positions
        for ml, mc, me in reversed(self._find_matches):
            self._lines[ml] = self._lines[ml][:mc] + self._find_replace + self._lines[ml][me:]
            self._invalidate_line_cache(ml)
        self._find_update_matches()

    def _render_goto_line(self, imgui, content_origin, size) -> None:
        """Render go-to-line popup at top-right."""
        bar_w = 200.0
        bar_x = content_origin.x + size.x - bar_w - 8.0
        bar_y = content_origin.y + 4.0

        draw_list = imgui.get_window_draw_list()
        bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.18, 0.22, 0.25, 0.95))
        border_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(*_C_SEPARATOR))
        draw_list.add_rect_filled(
            imgui.ImVec2(bar_x, bar_y),
            imgui.ImVec2(bar_x + bar_w, bar_y + 30.0),
            bg_col, 4.0)
        draw_list.add_rect(
            imgui.ImVec2(bar_x, bar_y),
            imgui.ImVec2(bar_x + bar_w, bar_y + 30.0),
            border_col, 4.0)

        imgui.set_cursor_screen_pos(imgui.ImVec2(bar_x + 6.0, bar_y + 4.0))
        imgui.push_item_width(bar_w - 16.0)
        if self._goto_focus:
            imgui.set_keyboard_focus_here()
            self._goto_focus = False
        confirmed, self._goto_line_buf = imgui.input_text_with_hint(
            "##goto_line", f"Go to line (1-{len(self._lines)})",
            self._goto_line_buf, imgui.InputTextFlags_.enter_returns_true)
        imgui.pop_item_width()

        if confirmed:
            try:
                target = int(self._goto_line_buf) - 1  # 1-based to 0-based
                target = max(0, min(target, len(self._lines) - 1))
                self.set_cursor_position(target, 0)
                self.set_view_at_line(target, SetViewAtLineMode.always)
            except ValueError:
                pass
            self._goto_open = False

        # Close on Escape (handled in _handle_keyboard, but also check here)
        if imgui.is_key_pressed(imgui.Key.escape):
            self._goto_open = False

    def _render_find_bar(self, imgui, content_origin, size) -> None:
        """Render the find/replace overlay bar at the top-right of the editor."""
        bar_w = min(400.0, size.x - 20.0)
        bar_x = content_origin.x + size.x - bar_w - 8.0
        bar_y = content_origin.y + 4.0
        row_h = 26.0
        bar_h = row_h + 6.0
        if self._find_replace_open:
            bar_h += row_h + 4.0

        draw_list = imgui.get_window_draw_list()
        bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.18, 0.22, 0.25, 0.95))
        border_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(*_C_SEPARATOR))
        draw_list.add_rect_filled(
            imgui.ImVec2(bar_x, bar_y),
            imgui.ImVec2(bar_x + bar_w, bar_y + bar_h),
            bg_col, 4.0)
        draw_list.add_rect(
            imgui.ImVec2(bar_x, bar_y),
            imgui.ImVec2(bar_x + bar_w, bar_y + bar_h),
            border_col, 4.0)

        imgui.set_cursor_screen_pos(imgui.ImVec2(bar_x + 6.0, bar_y + 4.0))
        input_w = bar_w - 130.0

        # Find input
        imgui.push_item_width(input_w)
        if self._find_focus_input:
            imgui.set_keyboard_focus_here()
            self._find_focus_input = False
        changed, new_query = imgui.input_text("##find_input", self._find_query,
                                               imgui.InputTextFlags_.enter_returns_true)
        imgui.pop_item_width()
        if new_query != self._find_query:
            self._find_query = new_query
            self._find_update_matches()
        if changed and self._find_matches:
            # Enter pressed — go to next match
            self._find_index = (self._find_index + 1) % len(self._find_matches)
            self._find_jump_to_match()

        imgui.same_line()
        match_count = len(self._find_matches)
        if self._find_query:
            idx_str = str(self._find_index + 1) if self._find_index >= 0 else "0"
            imgui.text(f"{idx_str}/{match_count}")
        else:
            imgui.text("")

        imgui.same_line()
        if imgui.small_button("X##find_close"):
            self._find_open = False
            self._find_replace_open = False
            self._find_matches.clear()

        # Replace row
        if self._find_replace_open:
            imgui.set_cursor_screen_pos(imgui.ImVec2(bar_x + 6.0, bar_y + row_h + 6.0))
            imgui.push_item_width(input_w)
            _, self._find_replace = imgui.input_text("##replace_input", self._find_replace)
            imgui.pop_item_width()

            imgui.same_line()
            if imgui.small_button("R##replace_one"):
                self._find_replace_current()
            if imgui.is_item_hovered():
                imgui.set_tooltip("Replace current")

            imgui.same_line()
            if imgui.small_button("A##replace_all"):
                self._find_replace_all()
            if imgui.is_item_hovered():
                imgui.set_tooltip("Replace all")

    _BRACKET_PAIRS = {'(': ')', '[': ']', '{': '}'}
    _BRACKET_CLOSE = {')': '(', ']': '[', '}': '{'}

    def _find_matching_bracket(self, line: int, col: int) -> tuple[int, int] | None:
        """Find the matching bracket for the character at (line, col).
        Returns (match_line, match_col) or None.
        """
        text = self._lines[line]
        if col >= len(text):
            return None
        ch = text[col]
        if ch in self._BRACKET_PAIRS:
            target, direction = self._BRACKET_PAIRS[ch], 1
        elif ch in self._BRACKET_CLOSE:
            target, direction = self._BRACKET_CLOSE[ch], -1
        else:
            return None

        depth = 0
        r, c_pos = line, col
        while 0 <= r < len(self._lines):
            ln = self._lines[r]
            start = c_pos if r == line else (0 if direction > 0 else len(ln) - 1)
            end = len(ln) if direction > 0 else -1
            for j in range(start, end, direction):
                c_ch = ln[j]
                if c_ch == ch:
                    depth += 1
                elif c_ch == target:
                    depth -= 1
                    if depth == 0:
                        return r, j
            r += direction
            c_pos = 0
        return None

    # ---- Undo stack --------------------------------------------------------

    def _push_undo(self) -> None:
        """Save current state to undo stack, truncating any redo history."""
        state = UndoState(
            lines=list(self._lines),
            cursors=[Cursor(c.line, c.col, c.anchor_line, c.anchor_col)
                     for c in self._cursors],
        )
        # Truncate forward history
        if self._undo_index < len(self._undo_stack) - 1:
            self._undo_stack = self._undo_stack[:self._undo_index + 1]
        self._undo_stack.append(state)
        if len(self._undo_stack) > 100:
            self._undo_stack.pop(0)
        self._undo_index = len(self._undo_stack) - 1
        self._pending_undo = False

    def _restore_state(self, state: UndoState) -> None:
        self._lines = list(state.lines)
        self._cursors = [Cursor(c.line, c.col, c.anchor_line, c.anchor_col)
                         for c in state.cursors]
        self._clear_line_caches()

    def can_undo(self) -> bool:
        return self._undo_index > 0 or self._pending_undo

    def can_redo(self) -> bool:
        return self._undo_index < len(self._undo_stack) - 1

    def undo(self) -> None:
        if self._pending_undo:
            self._push_undo()
        if self._undo_index > 0:
            self._undo_index -= 1
            self._restore_state(self._undo_stack[self._undo_index])

    def redo(self) -> None:
        if self._undo_index < len(self._undo_stack) - 1:
            self._undo_index += 1
            self._restore_state(self._undo_stack[self._undo_index])

    def _mark_typing(self) -> None:
        """Call after each character insert; manages undo coalescing."""
        self._last_edit_time = time.monotonic()
        self._pending_undo = True

    def _check_idle_commit(self) -> None:
        """Push undo state if >=1 second has passed since last keystroke."""
        if self._pending_undo and time.monotonic() - self._last_edit_time >= 1.0:
            self._push_undo()

    def _tokenize_cached(self, line_idx: int) -> list:
        text = self._lines[line_idx]
        cached = self._token_cache.get(line_idx)
        if cached and cached[0] == text:
            return cached[1]
        from ui.papyrus.papyrus_tokenizer import tokenize_line
        tokens = tokenize_line(text)
        self._token_cache[line_idx] = (text, tokens)
        return tokens
