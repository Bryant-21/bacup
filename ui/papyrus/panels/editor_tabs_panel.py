"""EditorTabsPanel — tabbed TextEditor instances for open .psc files."""
from __future__ import annotations

import logging
import os
import time
from typing import Optional

from imgui_bundle import imgui

_log = logging.getLogger("toolkit.papyrus.tabs")

_DEBOUNCE_S = 0.3   # 300ms parse debounce


def _estimate_cursor(old_text: str, new_text: str) -> tuple[int, int]:
    """Estimate cursor line/col after a text change by finding the first differing line."""
    old_lines = old_text.split("\n")
    new_lines = new_text.split("\n")
    for i, (a, b) in enumerate(zip(old_lines, new_lines)):
        if a != b:
            col = next((j for j, (ca, cb) in enumerate(zip(a, b)) if ca != cb), min(len(a), len(b)))
            return i, col
    return max(0, min(len(old_lines), len(new_lines)) - 1), 0


def _cursor_from_mouse(editor, rel_x: float, rel_y: float, word: str) -> tuple[int, int]:
    """Compute line/col from mouse position relative to the editor widget."""
    from imgui_bundle import imgui
    line_height = imgui.get_text_line_height_with_spacing()
    first = editor.get_first_visible_line()
    line = first + max(0, int(rel_y / line_height)) if line_height > 0 else 0
    lines = editor.get_text_lines()
    if 0 <= line < len(lines):
        col = lines[line].find(word)
        if col < 0:
            col = 0
    else:
        line, col = 0, 0
    return line, col


class EditorTabsPanel:
    """Main editor area with one TextEditor tab per open file."""

    def __init__(self, app, mono_font=None):
        """
        Args:
            app: PapyrusEditorApp instance.
            mono_font: ImFont for the code editor (pushed during render).
        """
        self._app = app
        self._mono_font = mono_font
        self.font_scale: float = 1.0
        self.extra_line_spacing: float = 0.0
        self.window_name = "Editor##papyrus"
        self._visible = True
        self._last_keystroke: dict[str, float] = {}   # path -> time of last keystroke
        self._last_drawn_active: str | None = None  # track programmatic tab switches
        # Close queue & save-confirm state
        self._close_queue: list[str] = []
        self._confirm_path: str | None = None
        self._open_confirm_popup: bool = False
        # Completion suggestion list
        self._suggestion_items: list = []   # all items from last LSP result

    def draw(self):
        if not self._visible:
            return
        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return
        if expanded and self._app.open_files:
            self._app.update()   # poll LSP results
            self._check_debounced_parse()
            self._check_completions()
            self._process_close_queue()
            self._draw_tabs()
        self._draw_save_confirm_modal()
        imgui.end()

    def _check_debounced_parse(self):
        """Submit PARSE_RESOLVE for paths where 300ms has elapsed since last keystroke."""
        now = time.monotonic()
        for path, ts in list(self._last_keystroke.items()):
            if now - ts >= _DEBOUNCE_S:
                del self._last_keystroke[path]
                buf = self._app.open_files.get(path)
                if buf:
                    from ui.papyrus.papyrus_lsp_service import LspRequest, PARSE_RESOLVE
                    self._app.lsp.submit(LspRequest(PARSE_RESOLVE, path=path,
                                                     text=buf.text))

    def _check_completions(self):
        """Check for new completion results; update suggestion list and ghost text."""
        result = self._app.lsp.poll_completions()
        if result is None:
            return
        if result.path != self._app.active_path:
            self._suggestion_items = []
            return
        buf = self._app.open_files.get(result.path)
        if buf is None:
            return
        if not result.items:
            self._suggestion_items = []
            return

        # Always update the suggestion list
        self._suggestion_items = result.items

        # Ghost text: only when cursor is inside a word with a non-empty suffix
        cursor = buf.editor.get_cursor_position()
        lines = buf.text.split('\n')
        if cursor[0] >= len(lines):
            return
        line = lines[cursor[0]]
        col = cursor[1]
        word_start = col
        while word_start > 0 and (line[word_start - 1].isalnum() or line[word_start - 1] == '_'):
            word_start -= 1
        partial = line[word_start:col]
        if len(partial) >= 3:
            top = result.items[0]
            if top.label.lower().startswith(partial.lower()):
                suffix = top.label[len(partial):]
                if suffix:
                    buf.editor.set_inline_suggestion(suffix)

    def _draw_tabs(self):
        tab_bar_flags = imgui.TabBarFlags_.reorderable | imgui.TabBarFlags_.auto_select_new_tabs
        need_select = self._app.active_path != self._last_drawn_active
        if imgui.begin_tab_bar("##papyrus_tabs", tab_bar_flags):
            paths = list(self._app.open_files.keys())
            for path, buf in list(self._app.open_files.items()):
                dirty_marker = " \u25cf" if buf.dirty else ""
                tab_label = os.path.basename(path) + dirty_marker + f"##{path}"
                open_flag = imgui.TabItemFlags_.set_selected if (need_select and path == self._app.active_path) else 0
                selected, tab_still_open = imgui.begin_tab_item(tab_label, True, open_flag)

                # Context menu must be called immediately after begin_tab_item (uses last item ID)
                if imgui.begin_popup_context_item(f"##tabctx_{path}"):
                    idx = paths.index(path) if path in paths else -1
                    if imgui.menu_item("Close", "", False)[0]:
                        self._queue_close([path])
                    if imgui.menu_item("Close All to Left", "", False, idx > 0)[0]:
                        self._queue_close(paths[:idx])
                    if imgui.menu_item("Close All to Right", "", False, idx < len(paths) - 1)[0]:
                        self._queue_close(paths[idx + 1:])
                    imgui.separator()
                    if imgui.menu_item("Close All", "", False)[0]:
                        self._queue_close(list(paths))
                    imgui.end_popup()

                if selected:
                    if self._app.active_path != path:
                        self._app.active_path = path
                    self._draw_buffer(path, buf)
                    imgui.end_tab_item()

                if not tab_still_open:
                    self._queue_close([path])
            imgui.end_tab_bar()
        self._last_drawn_active = self._app.active_path

    def _queue_close(self, paths: list[str]):
        """Add paths to the close queue (deduplicating preserving order)."""
        seen = set(self._close_queue)
        for p in paths:
            if p not in seen and p in self._app.open_files:
                self._close_queue.append(p)
                seen.add(p)

    def _process_close_queue(self):
        """Drive the close queue: if no confirm is pending, pop and process next item."""
        if self._confirm_path is not None:
            return  # waiting for user response in modal
        while self._close_queue:
            path = self._close_queue.pop(0)
            if path not in self._app.open_files:
                continue
            buf = self._app.open_files[path]
            if buf.dirty and not buf.editor.is_read_only_enabled():
                self._confirm_path = path
                self._open_confirm_popup = True
                return  # pause — modal will continue
            self._app.close_file(path)

    def _draw_save_confirm_modal(self):
        """Modal dialog: 'Save changes to <file>?' — Save / Don't Save / Cancel."""
        if self._open_confirm_popup:
            imgui.open_popup("##save_confirm")
            self._open_confirm_popup = False

        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        imgui.set_next_window_size(imgui.ImVec2(460, 0), imgui.Cond_.appearing)

        opened, _ = imgui.begin_popup_modal("##save_confirm", None,
                                            imgui.WindowFlags_.no_title_bar
                                            | imgui.WindowFlags_.always_auto_resize)
        if opened:
            fname = os.path.basename(self._confirm_path) if self._confirm_path else ""
            imgui.text(f"Save changes to {fname}?")
            imgui.spacing()
            avail = imgui.get_content_region_avail().x
            spacing = imgui.get_style().item_spacing.x
            btn_w = (avail - 2 * spacing) / 3
            if imgui.button("Save", imgui.ImVec2(btn_w, 0)):
                if self._confirm_path:
                    self._app.save_file(self._confirm_path)
                    self._app.close_file(self._confirm_path)
                self._confirm_path = None
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Don't Save", imgui.ImVec2(btn_w, 0)):
                if self._confirm_path:
                    self._app.close_file(self._confirm_path)
                self._confirm_path = None
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(btn_w, 0)):
                self._close_queue.clear()  # abort remaining queue
                self._confirm_path = None
                imgui.close_current_popup()
            imgui.end_popup()

    def _draw_buffer(self, path: str, buf):
        """Draw a single editor buffer (called inside a tab item)."""
        # Read-only banner (base game files)
        if buf.editor.is_read_only_enabled():
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.85, 0.85, 0.5, 1.0))
            imgui.text_unformatted("\u26d4  Base game file — read only")
            imgui.pop_style_color()
            imgui.separator()

        # External change banner
        if buf.external_changed:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.8, 0.2, 1.0))
            imgui.text_unformatted("File changed on disk \u2014")
            imgui.same_line()
            if imgui.small_button("Reload"):
                self._reload_buffer(path, buf)
            imgui.same_line()
            if imgui.small_button("Keep"):
                buf.external_changed = False
            imgui.pop_style_color()
            imgui.separator()

        # Diagnostic error summary (since TextEditor has no inline markers)
        errors = [d for d in buf.diagnostics if d.severity == "error"]
        warnings = [d for d in buf.diagnostics if d.severity == "warning"]
        if errors or warnings:
            msg = f"  {len(errors)} error(s)  {len(warnings)} warning(s)"
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.4, 0.4, 1.0))
            imgui.text_unformatted(msg)
            imgui.pop_style_color()

        # Toolbar: find / find+replace buttons
        if imgui.small_button("Find"):
            buf.editor.open_find()
        if imgui.is_item_hovered():
            imgui.set_tooltip("Find  (Ctrl+F)")
        imgui.same_line()
        if imgui.small_button("Replace"):
            buf.editor.open_find_replace()
        if imgui.is_item_hovered():
            imgui.set_tooltip("Find & Replace  (Ctrl+H)")
        imgui.separator()

        # Detect Ctrl+click before rendering editor
        io = imgui.get_io()
        ctrl_click = io.key_ctrl and imgui.is_mouse_clicked(0)

        # Push current diagnostics into the editor widget for gutter/squiggle rendering
        buf.editor.set_diagnostics(buf.diagnostics)

        # Render editor — push mono font at scaled size if available
        if self._mono_font:
            scaled_size = self._mono_font.legacy_size * self.font_scale
            imgui.push_font(self._mono_font, scaled_size)
        avail = imgui.get_content_region_avail()
        buf.editor.render("##editor_" + path, False, avail, False,
                          extra_line_spacing=self.extra_line_spacing)
        if self._mono_font:
            imgui.pop_font()

        # Sync text back to buffer (detect edits)
        current_text = buf.editor.get_text()
        if current_text != buf._last_editor_text:
            cursor_line, cursor_col = _estimate_cursor(buf._last_editor_text, current_text)
            buf.text = current_text
            buf._last_editor_text = current_text
            buf.dirty = True
            buf.editor.clear_inline_suggestion()
            # Record keystroke time for debounce
            self._last_keystroke[path] = time.monotonic()
            # Submit completion only for editable files with 3+ char partial word
            if not buf.editor.is_read_only_enabled():
                cur_lines = current_text.split('\n')
                if cursor_line < len(cur_lines):
                    cur_line = cur_lines[cursor_line]
                    ws = cursor_col
                    while ws > 0 and (cur_line[ws - 1].isalnum() or cur_line[ws - 1] == '_'):
                        ws -= 1
                    if cursor_col - ws >= 3:
                        from ui.papyrus.papyrus_lsp_service import LspRequest, COMPLETE
                        self._app.lsp.submit(LspRequest(COMPLETE, path=path, text=current_text,
                                                         line=cursor_line, col=cursor_col))
                    else:
                        self._suggestion_items = []

        # Ctrl+Shift+F — format document
        if (not buf.editor.is_read_only_enabled()
                and io.key_ctrl and io.key_shift
                and imgui.is_key_pressed(imgui.Key.f)):
            from ui.papyrus.papyrus_formatter import format_papyrus
            current = buf.editor.get_text()
            formatted = format_papyrus(current)
            if formatted != current:
                buf.editor.set_text(formatted)
                buf.text = formatted
                buf.dirty = True
                buf._last_editor_text = formatted

        # Clear suggestion list on Escape
        if self._suggestion_items and imgui.is_key_pressed(imgui.Key.escape):
            self._suggestion_items = []

        # Completion suggestion list (non-interactive, filters as you type)
        if self._suggestion_items and not buf.editor.is_read_only_enabled():
            self._draw_suggestion_list(buf)

        # Handle Ctrl+click on editor
        if ctrl_click and imgui.is_item_hovered():
            mp = imgui.get_mouse_pos()
            item_pos = imgui.get_item_rect_min()
            rel_x = mp.x - item_pos.x
            rel_y = mp.y - item_pos.y
            word = buf.editor.get_word_at_screen_pos(imgui.ImVec2(mp.x, mp.y))
            if word:
                click_line, click_col = _cursor_from_mouse(buf.editor, rel_x, rel_y, word)
                from ui.papyrus.papyrus_lsp_service import LspRequest, DEFINITION
                self._app.lsp.submit(LspRequest(DEFINITION, path=path,
                                                 text=buf.text,
                                                 line=click_line, col=click_col))


    def _draw_suggestion_list(self, buf):
        """Non-interactive floating completion list below the cursor."""
        pos = buf.editor.get_cursor_screen_pos()
        if pos is None:
            return

        # Compute live partial at cursor position
        cursor = buf.editor.get_cursor_position()
        lines_text = buf.text.split('\n')
        if cursor[0] >= len(lines_text):
            return
        line_text = lines_text[cursor[0]]
        col = cursor[1]
        ws = col
        while ws > 0 and (line_text[ws - 1].isalnum() or line_text[ws - 1] == '_'):
            ws -= 1
        partial = line_text[ws:col]

        if len(partial) < 3:
            return

        filtered = [it for it in self._suggestion_items
                    if it.label.lower().startswith(partial.lower())][:20]
        if not filtered:
            return

        line_h = imgui.get_text_line_height_with_spacing()
        pad_y = imgui.get_style().window_padding.y
        box_w = 360.0
        box_h = min(len(filtered) * line_h + pad_y * 2, 220.0)

        # Clamp to viewport, flip above cursor if too close to bottom
        vp = imgui.get_main_viewport()
        sx = min(pos[0], vp.pos.x + vp.size.x - box_w - 4.0)
        sy = pos[1]
        if sy + box_h > vp.pos.y + vp.size.y - 4.0:
            sy = pos[1] - buf.editor._line_height - box_h

        imgui.set_next_window_pos(imgui.ImVec2(sx, sy), imgui.Cond_.always)
        imgui.set_next_window_size(imgui.ImVec2(box_w, box_h), imgui.Cond_.always)
        imgui.set_next_window_bg_alpha(0.93)
        flags = (imgui.WindowFlags_.no_title_bar
                 | imgui.WindowFlags_.no_resize
                 | imgui.WindowFlags_.no_move
                 | imgui.WindowFlags_.no_nav
                 | imgui.WindowFlags_.no_focus_on_appearing)
        expanded, _ = imgui.begin("##suggestion_list", None, flags)
        if expanded:
            p_len = len(partial)
            for item in filtered:
                # Typed portion in yellow, rest in fg, detail in grey
                if p_len > 0:
                    imgui.push_style_color(imgui.Col_.text,
                                           imgui.ImVec4(0xDB/255, 0xBC/255, 0x7F/255, 1.0))
                    imgui.text_unformatted(item.label[:p_len])
                    imgui.pop_style_color()
                    imgui.same_line(0.0, 0.0)
                imgui.push_style_color(imgui.Col_.text,
                                       imgui.ImVec4(0xD3/255, 0xC6/255, 0xAA/255, 1.0))
                imgui.text_unformatted(item.label[p_len:])
                imgui.pop_style_color()
                if getattr(item, 'detail', None):
                    imgui.same_line()
                    imgui.push_style_color(imgui.Col_.text,
                                           imgui.ImVec4(0x85/255, 0x92/255, 0x89/255, 1.0))
                    imgui.text_unformatted(f"  {item.detail}")
                    imgui.pop_style_color()
        imgui.end()

    def _reload_buffer(self, path: str, buf):
        """Reload file from disk into editor."""
        try:
            from pathlib import Path as PPath
            text = PPath(path).read_text(encoding="utf-8", errors="replace")
            buf.text = text
            buf._last_editor_text = text
            buf.editor.set_text(text)
            buf.dirty = False
            buf.external_changed = False
            _log.info("Reloaded: %s", path)
        except OSError as e:
            _log.error("Reload failed for %s: %s", path, e)

