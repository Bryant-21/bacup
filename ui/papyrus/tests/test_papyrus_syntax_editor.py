"""Tests for PapyrusSyntaxEditor data model (no ImGui context required)."""
import pytest


def make_editor(text=""):
    from ui.papyrus.papyrus_syntax_editor import PapyrusSyntaxEditor
    ed = PapyrusSyntaxEditor()
    if text:
        ed.set_text(text)
    return ed


# ---- Text model ------------------------------------------------------------

def test_set_get_text_roundtrip():
    ed = make_editor("ScriptName Foo\nInt x = 1\n")
    assert ed.get_text() == "ScriptName Foo\nInt x = 1\n"


def test_get_text_lines():
    ed = make_editor("line1\nline2\nline3")
    assert ed.get_text_lines() == ["line1", "line2", "line3"]


def test_set_text_lines():
    ed = make_editor()
    ed.set_text_lines(["a", "b", "c"])
    assert ed.get_text() == "a\nb\nc"


def test_empty_text_is_one_empty_line():
    ed = make_editor("")
    assert ed.get_text_lines() == [""]
    assert ed.get_line_count() == 1


def test_get_line_count():
    ed = make_editor("a\nb\nc")
    assert ed.get_line_count() == 3


# ---- Cursor model ----------------------------------------------------------

def test_initial_cursor_at_origin():
    ed = make_editor("hello")
    assert ed.get_cursor_position() == (0, 0)


def test_set_cursor_position():
    ed = make_editor("hello\nworld")
    ed.set_cursor_position(1, 3)
    assert ed.get_cursor_position() == (1, 3)


def test_set_cursor_position_clamped_line():
    ed = make_editor("hello")
    ed.set_cursor_position(99, 0)
    assert ed.get_cursor_position()[0] == 0  # only 1 line


def test_set_cursor_position_clamped_col():
    ed = make_editor("hi")
    ed.set_cursor_position(0, 99)
    assert ed.get_cursor_position() == (0, 2)  # len("hi") = 2


# ---- set_view_at_line ------------------------------------------------------

def test_set_view_always_scrolls():
    from ui.papyrus.papyrus_syntax_editor import SetViewAtLineMode
    ed = make_editor("\n".join(str(i) for i in range(100)))
    ed._visible_lines = 20
    ed.set_view_at_line(50, SetViewAtLineMode.always)
    assert ed.get_first_visible_line() == 50


def test_set_view_if_not_visible_scrolls_when_outside():
    from ui.papyrus.papyrus_syntax_editor import SetViewAtLineMode
    ed = make_editor("\n".join(str(i) for i in range(100)))
    ed._visible_lines = 20
    ed._scroll_line = 0
    ed.set_view_at_line(50, SetViewAtLineMode.if_not_visible)
    assert ed.get_first_visible_line() == 50


def test_set_view_if_not_visible_noop_when_visible():
    from ui.papyrus.papyrus_syntax_editor import SetViewAtLineMode
    ed = make_editor("\n".join(str(i) for i in range(100)))
    ed._visible_lines = 30
    ed._scroll_line = 10  # visible range: 10-39
    ed.set_view_at_line(15, SetViewAtLineMode.if_not_visible)
    assert ed.get_first_visible_line() == 10  # unchanged


# ---- get_word_at_screen_pos ------------------------------------------------

def test_word_at_pos_returns_word():
    ed = make_editor("ScriptName Foo")
    # Manually set layout metrics so _screen_to_pos works
    ed._char_width = 8.0
    ed._line_height = 16.0
    ed._text_origin_x = 0.0
    ed._text_origin_y = 0.0
    ed._scroll_line = 0
    ed._scroll_x = 0.0
    from ui.papyrus.papyrus_syntax_editor import _ImVec2Stub
    word = ed.get_word_at_screen_pos(_ImVec2Stub(11 * 8.0, 0.0))  # col 11 = 'F' in "Foo"
    assert word == "Foo"


def test_word_at_pos_empty_on_space():
    ed = make_editor("Foo Bar")
    ed._char_width = 8.0
    ed._line_height = 16.0
    ed._text_origin_x = 0.0
    ed._text_origin_y = 0.0
    ed._scroll_line = 0
    ed._scroll_x = 0.0
    from ui.papyrus.papyrus_syntax_editor import _ImVec2Stub
    # col 3 = space between Foo and Bar
    word = ed.get_word_at_screen_pos(_ImVec2Stub(3 * 8.0, 0.0))
    assert word == ""


# ---- read-only -------------------------------------------------------------

def test_read_only_toggle():
    ed = make_editor()
    assert not ed.is_read_only_enabled()
    ed.set_read_only_enabled(True)
    assert ed.is_read_only_enabled()


# ---- _dedup_cursors --------------------------------------------------------

def test_dedup_removes_duplicate_cursors():
    from ui.papyrus.papyrus_syntax_editor import PapyrusSyntaxEditor, Cursor
    ed = make_editor("hello")
    ed._cursors = [Cursor(0, 2), Cursor(0, 2), Cursor(0, 5)]
    ed._dedup_cursors()
    assert len(ed._cursors) == 2


def test_dedup_sorts_by_position():
    from ui.papyrus.papyrus_syntax_editor import PapyrusSyntaxEditor, Cursor
    ed = make_editor("hello\nworld")
    ed._cursors = [Cursor(1, 0), Cursor(0, 3)]
    ed._dedup_cursors()
    assert ed._cursors[0].line == 0
    assert ed._cursors[1].line == 1


# ---- Undo stack ------------------------------------------------------------

def test_push_undo_increments_index():
    ed = make_editor("hello")
    assert ed.get_undo_index() == -1
    ed._push_undo()
    assert ed.get_undo_index() == 0
    ed._push_undo()
    assert ed.get_undo_index() == 1


def test_undo_restores_text():
    ed = make_editor("hello")
    ed._push_undo()
    ed.set_text("world")
    ed._push_undo()
    ed.undo()
    assert ed.get_text() == "hello"


def test_redo_reapplies_text():
    ed = make_editor("hello")
    ed._push_undo()
    ed.set_text("world")
    ed._push_undo()
    ed.undo()
    ed.redo()
    assert ed.get_text() == "world"


def test_undo_noop_at_bottom():
    ed = make_editor("hello")
    ed.undo()  # nothing to undo — should not raise
    assert ed.get_text() == "hello"


def test_redo_noop_at_top():
    ed = make_editor("hello")
    ed._push_undo()
    ed.redo()  # nothing to redo — should not raise
    assert ed.get_text() == "hello"


def test_push_undo_truncates_redo_history():
    ed = make_editor("v1")
    ed._push_undo()
    ed.set_text("v2")
    ed._push_undo()
    ed.undo()  # back to v1
    # Now push a new state — redo branch (v2) should be discarded
    ed.set_text("v3")
    ed._push_undo()
    ed.redo()  # should be noop (no redo history)
    assert ed.get_text() == "v3"


def test_undo_max_100_states():
    ed = make_editor("")
    for i in range(110):
        ed.set_text(str(i))
        ed._push_undo()
    assert len(ed._undo_stack) <= 100


def test_typing_coalesces_until_idle():
    """Rapid typing accumulates as one pending undo state until _check_idle_commit fires."""
    import time
    ed = make_editor("hello")
    ed._push_undo()  # baseline

    # Simulate typing three characters without idle
    for ch in "abc":
        for c in ed._cursors:
            ed._insert_text_at_cursor(c, ch)
        ed._mark_typing()

    # Not yet committed
    assert ed._pending_undo is True
    assert ed.get_undo_index() == 0  # still at baseline

    # Simulate idle commit (manually set time to force it)
    ed._last_edit_time = time.monotonic() - 2.0
    ed._check_idle_commit()
    assert ed._pending_undo is False
    assert ed.get_undo_index() == 1  # new undo state pushed
