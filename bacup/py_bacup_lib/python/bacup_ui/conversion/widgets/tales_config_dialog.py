"""Configure Features: the in-game TALES CONFIG menu, set before install.

Rows come from ``bacup_lib.tales_config`` (synced from Tales' TalesConfigRules.cpp). Changes are
kept as pending ``{"Section/key": value}`` edits and written into the deployed
B21_TalesFromAppalachia.ini by the next install.
"""
from __future__ import annotations

import sys
from collections.abc import Callable
from pathlib import Path

from imgui_bundle import icons_fontawesome_6 as fa, imgui

from bacup_lib import tales_config as tc
from creation_lib.ui.widgets.modern import (
    InteractionState, action_button, navigation_item, prepare_dialog, scaled, section, semantic_color, toggle,
)

POPUP = "Configure Features##tales_config"
_CONTROL_WIDTH = 280

_VK_BY_KEY: dict[int, int] = {
    **{getattr(imgui.Key, chr(c)): c - 32 for c in range(ord("a"), ord("z") + 1)},
    **{getattr(imgui.Key, f"_{d}"): 0x30 + d for d in range(10)},
    **{getattr(imgui.Key, f"keypad{d}"): 0x60 + d for d in range(10)},
    **{getattr(imgui.Key, f"f{n}"): 0x6F + n for n in range(1, 13)},
    imgui.Key.backspace: 0x08, imgui.Key.tab: 0x09, imgui.Key.enter: 0x0D, imgui.Key.pause: 0x13,
    imgui.Key.caps_lock: 0x14, imgui.Key.escape: 0x1B, imgui.Key.space: 0x20, imgui.Key.page_up: 0x21,
    imgui.Key.page_down: 0x22, imgui.Key.end: 0x23, imgui.Key.home: 0x24, imgui.Key.left_arrow: 0x25,
    imgui.Key.up_arrow: 0x26, imgui.Key.right_arrow: 0x27, imgui.Key.down_arrow: 0x28,
    imgui.Key.print_screen: 0x2C, imgui.Key.insert: 0x2D, imgui.Key.delete: 0x2E,
    imgui.Key.keypad_multiply: 0x6A, imgui.Key.keypad_add: 0x6B, imgui.Key.keypad_subtract: 0x6D,
    imgui.Key.keypad_decimal: 0x6E, imgui.Key.keypad_divide: 0x6F, imgui.Key.num_lock: 0x90,
    imgui.Key.scroll_lock: 0x91, imgui.Key.left_shift: 0xA0, imgui.Key.right_shift: 0xA1,
    imgui.Key.left_ctrl: 0xA2, imgui.Key.right_ctrl: 0xA3, imgui.Key.left_alt: 0xA4, imgui.Key.right_alt: 0xA5,
    imgui.Key.semicolon: 0xBA, imgui.Key.equal: 0xBB, imgui.Key.comma: 0xBC, imgui.Key.minus: 0xBD,
    imgui.Key.period: 0xBE, imgui.Key.slash: 0xBF, imgui.Key.grave_accent: 0xC0, imgui.Key.left_bracket: 0xDB,
    imgui.Key.backslash: 0xDC, imgui.Key.right_bracket: 0xDD, imgui.Key.apostrophe: 0xDE,
}


def key_name(vk: int) -> str:
    """The keyboard-layout name Tales shows for ``vk`` (HotkeySettings.cpp Name)."""
    if not vk:
        return "Unbound"
    if sys.platform == "win32":
        import ctypes

        user32 = ctypes.windll.user32
        scan = user32.MapVirtualKeyExW(vk, 4, user32.GetKeyboardLayout(0))  # MAPVK_VK_TO_VSC_EX
        parameter = (scan & 0xFF) << 16 | (1 << 24 if scan & 0xFF00 else 0)
        buffer = ctypes.create_string_buffer(128)
        if user32.GetKeyNameTextA(parameter, buffer, len(buffer)):
            return buffer.value.decode("mbcs", "replace")
    return str(vk)


class TalesConfigDialog:
    def __init__(self, save_edits: Callable[[dict[str, str]], None]) -> None:
        self._save_edits = save_edits
        self._pending_open = False
        self._schema: tc.Schema | None = None
        self._text = ""
        self._ini: dict[str, dict[str, str]] = {}
        self._edits: dict[str, str] = {}
        self._drafts: dict[str, str] = {}
        self._page = 0
        self._capturing: str | None = None
        self._source = ""
        self._error = ""
        self._nav_state = InteractionState()

    def open(self, data_dir: Path | None, edits: dict[str, str]) -> None:
        deployed = None
        try:
            self._schema = tc.load_schema()
            deployed = tc.read_deployed(data_dir)
            self._text = tc.apply_edits(tc.carry_forward(deployed), edits)
            self._error = ""
        except (OSError, ValueError, KeyError) as exc:
            self._schema, self._error = None, f"Tales settings could not be loaded: {exc}"
        self._ini = tc.parse_ini(self._text)
        self._edits = dict(edits)
        self._drafts.clear()
        self._capturing = None
        self._source = (
            f"Starting from the installed {tc.INI_RELATIVE_PATH.name}."
            if deployed is not None else "No installed Tales settings found; starting from the defaults."
        ) if self._schema else ""
        self._pending_open = True

    def draw(self) -> None:
        if self._pending_open:
            imgui.open_popup(POPUP)
            self._pending_open = False
        prepare_dialog(1040, 720)
        opened, _ = imgui.begin_popup_modal(POPUP, None, imgui.WindowFlags_.no_saved_settings)
        if not opened:
            return
        footer = 2 * imgui.get_text_line_height_with_spacing() + scaled(20)
        if self._schema is None:
            imgui.text_colored(semantic_color("error"), self._error)
        else:
            self._draw_body(imgui.get_content_region_avail().y - footer)
        self._draw_footer()
        imgui.end_popup()

    def _draw_body(self, height: float) -> None:
        pages = self._schema.pages
        imgui.begin_child("##tc_pages", imgui.ImVec2(scaled(210), height))
        for index, page in enumerate(pages):
            if navigation_item(f"tc_page{index}", page.title, icon=page.icon,
                               selected=index == self._page, state=self._nav_state):
                self._page, self._capturing = index, None
        imgui.end_child()
        imgui.same_line(0, scaled(14))
        imgui.begin_child("##tc_content", imgui.ImVec2(0, height))
        for group_index, group in enumerate(pages[self._page].groups):
            settings = [s for s in group.settings if s.kind != "action"]
            if not settings:
                continue
            with section(f"##tc_group{self._page}_{group_index}", group.title):
                if imgui.begin_table("##rows", 3, imgui.TableFlags_.sizing_stretch_prop):
                    imgui.table_setup_column("label", imgui.TableColumnFlags_.width_stretch)
                    imgui.table_setup_column("control", imgui.TableColumnFlags_.width_fixed, scaled(_CONTROL_WIDTH))
                    imgui.table_setup_column("reset", imgui.TableColumnFlags_.width_fixed, imgui.get_frame_height())
                    for setting in settings:
                        self._draw_row(setting)
                    imgui.end_table()
            imgui.spacing()
        imgui.end_child()

    def _draw_row(self, s: tc.Setting) -> None:
        imgui.push_id(s.id)
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.align_text_to_frame_padding()
        imgui.text(s.label)
        if s.help:
            imgui.push_text_wrap_pos(imgui.get_cursor_pos_x() + imgui.get_content_region_avail().x - scaled(16))
            imgui.text_disabled(s.help)
            imgui.pop_text_wrap_pos()
        imgui.dummy(imgui.ImVec2(0, scaled(4)))

        imgui.table_next_column()
        imgui.set_next_item_width(-1)
        self._draw_control(s)
        imgui.table_next_column()
        if not tc.is_default(self._ini, s):
            if _icon_button("reset", fa.ICON_FA_ROTATE_LEFT, "Reset to default"):
                self._drafts.pop(s.id, None)
                self._write(s, tc.format_bool(tc.read(self._ini, s), s.default == "1")
                            if s.kind == "toggle" else s.default)
        imgui.pop_id()

    def _draw_control(self, s: tc.Setting) -> None:
        if s.kind == "toggle":
            on = tc.read_toggle(self._ini, s)
            # The switch is shorter than a frame; centre it on the label's frame-padded line.
            imgui.set_cursor_pos_y(imgui.get_cursor_pos_y() + (imgui.get_frame_height() - scaled(20)) / 2)
            changed, on = toggle("On###toggle" if on else "Off###toggle", on)
            if changed:
                self._write(s, tc.format_bool(tc.read(self._ini, s), on))
        elif s.kind == "choice":
            current = tc.read(self._ini, s)
            labels = [c.label for c in s.choices]
            index = next((i for i, c in enumerate(s.choices) if c.value == current), -1)
            if index < 0:
                labels.append(current)
                index = len(labels) - 1
            changed, index = imgui.combo("##choice", index, labels)
            if changed and index < len(s.choices):
                self._write(s, s.choices[index].value)
        elif s.kind in ("int", "float"):
            value = float(self._drafts.get(s.id, tc.read_number(self._ini, s)))
            decimals = 0 if s.kind == "int" else max(1, len(tc.format_number(s.step, s.step)) - 2)
            changed, value = imgui.slider_float("##value", value, s.min, s.max, f"%.{decimals}f")
            if changed:
                self._drafts[s.id] = str(value)
            if imgui.is_item_deactivated_after_edit():
                stepped = min(max(round(value / s.step) * s.step, s.min), s.max)
                self._drafts.pop(s.id, None)
                self._write(s, tc.format_number(stepped, s.step))
        elif s.kind == "text":
            draft = self._drafts.setdefault(s.id, tc.read(self._ini, s))
            changed, draft = imgui.input_text("##text", draft)
            if changed:
                self._drafts[s.id] = draft
            if imgui.is_item_deactivated_after_edit():
                self._write(s, draft)
        elif s.kind == "key":
            self._draw_key(s)

    def _draw_key(self, s: tc.Setting) -> None:
        # Keys are read before the button so Enter/Space bind instead of re-pressing it.
        if self._capturing == s.id:
            for key, vk in _VK_BY_KEY.items():
                if imgui.is_key_pressed(key, False):
                    self._capturing = None
                    if vk != 0x1B:
                        self._write(s, tc.format_key(0 if vk in (0x08, 0x2E) else vk))
                    break
        capturing = self._capturing == s.id
        label = "Press a key...  (Esc cancels)" if capturing else key_name(tc.key_code(tc.read(self._ini, s)))
        if imgui.button(f"{label}##key", imgui.ImVec2(imgui.calc_item_width(), 0)):
            self._capturing = None if capturing else s.id
        if imgui.is_item_hovered():
            imgui.set_tooltip("Click, then press a key. Backspace or Delete unbinds.")

    def _draw_footer(self) -> None:
        imgui.separator()
        imgui.dummy(imgui.ImVec2(0, scaled(4)))
        if self._edits:
            note = f"{len(self._edits)} change{'s' if len(self._edits) != 1 else ''} will be written on the next install."
        else:
            note = self._source or "Changes are written to the Tales ini on the next install."
        button_width = scaled(120)
        buttons_x = (imgui.get_cursor_pos_x() + imgui.get_content_region_avail().x
                     - 2 * button_width - imgui.get_style().item_spacing.x)
        imgui.begin_group()
        imgui.push_text_wrap_pos(buttons_x - scaled(16))
        imgui.text_colored(semantic_color("accent" if self._edits else "muted"), note)
        imgui.pop_text_wrap_pos()
        imgui.end_group()
        imgui.same_line(buttons_x)
        if action_button("Discard##tc_discard", width=button_width, enabled=bool(self._edits)):
            self._edits.clear()
            self._save_edits({})
            imgui.close_current_popup()
        imgui.same_line()
        if action_button("Done##tc_done", primary=True, width=button_width):
            self._capturing = None
            imgui.close_current_popup()

    def _write(self, s: tc.Setting, value: str) -> None:
        self._text = tc.set_value(self._text, s.section, s.key, value)
        self._ini = tc.parse_ini(self._text)
        self._edits[s.id] = value
        self._save_edits(dict(self._edits))


def _icon_button(identifier: str, glyph: str, tooltip: str) -> bool:
    has_glyph = imgui.get_font_baked().find_glyph_no_fallback(ord(glyph))
    side = imgui.get_frame_height()
    clicked = imgui.button(f"{glyph if has_glyph else 'R'}##{identifier}", imgui.ImVec2(side, side))
    if imgui.is_item_hovered():
        imgui.set_tooltip(tooltip)
    return clicked
