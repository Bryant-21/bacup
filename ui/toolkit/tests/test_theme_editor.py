from pathlib import Path
import subprocess
import sys

import pytest

def _check_resolved_colors():
    from creation_lib.ui.theme import get_theme, get_theme_colors
    from creation_lib.ui.theme.themes import STYLE_COLORS, STATUS_COLORS, normalize_color_overrides

    theme = get_theme("fallout76")
    defaults = get_theme_colors(theme)
    assert set(defaults) == set(STYLE_COLORS) | set(STATUS_COLORS)
    custom = get_theme_colors(theme, {"tab_selected": [0.1, 0.2, 0.3, 0.4]})
    assert custom["tab_selected"] == (0.1, 0.2, 0.3, 0.4)
    assert defaults["tab_selected"] != custom["tab_selected"]
    assert get_theme_colors(theme) == defaults
    for invalid in (None, "#123456", [0, 1], [0, 1, 2, 1], [0, float("nan"), 0, 1], [0, 0, "1", 1]):
        assert normalize_color_overrides({"tab_selected": invalid, "removed_color": [0, 0, 0, 1]}) == {}


def _check_editor_drafts():
    from creation_lib.ui.theme import get_theme, get_theme_colors
    from creation_lib.ui.theme.editor import ThemeEditor

    saved = {"fallout76": {"tab_selected": [0.1, 0.2, 0.3, 1]}, "falloutnv": {"text": [1, 1, 1, 1]}}
    editor = ThemeEditor()
    editor.open("fallout76", saved)
    editor.set_color("tab_selected", [0.4, 0.3, 0.2, 1])
    assert saved["fallout76"]["tab_selected"] == [0.1, 0.2, 0.3, 1]
    editor.search, editor.group = "selected, focused", "Tabs"
    assert [name for name, _rgba in editor.visible_colors()] == ["tab_selected"]
    editor.reset_color("tab_selected")
    assert get_theme_colors(get_theme("fallout76"), editor.current_overrides) == get_theme_colors(get_theme("fallout76"))
    editor.reset_theme()
    assert editor.overrides == {"falloutnv": {"text": [1, 1, 1, 1]}}


def _check_new_vegas_checkbox_contrast():
    from creation_lib.ui.theme import get_theme, get_theme_colors

    def luminance(rgba):
        linear = [v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in rgba[:3]]
        return sum(v * weight for v, weight in zip(linear, (0.2126, 0.7152, 0.0722)))

    colors = get_theme_colors(get_theme("falloutnv"))
    mark = luminance(colors["check_mark"])
    for name in ("checkbox_selected_bg", "frame_bg", "frame_bg_hovered", "frame_bg_active"):
        background = luminance(colors[name])
        contrast = (max(mark, background) + 0.05) / (min(mark, background) + 0.05)
        assert contrast >= 4.5, f"Checkmark contrast against {name} is only {contrast:.2f}:1"


def _check_persistence(tmp_path):
    from ui.toolkit.settings import ToolkitSettings

    first = ToolkitSettings(path=tmp_path / "first.json", editor_settings_path=tmp_path / "missing.json")
    first.theme_colors = {"fallout76": {"tab_selected": [0.1, 0.2, 0.3, 0.4]}, "falloutnv": {"status_warning": [0.8, 0.4, 0, 1]}}
    first.save()
    second = ToolkitSettings(path=tmp_path / "second.json", editor_settings_path=tmp_path / "missing.json")
    assert second.theme_colors == first.theme_colors


def test_theme_editor_with_real_imgui():
    result = subprocess.run([sys.executable, "-m", "ui.toolkit.tests.test_theme_editor"],
                            cwd=Path(__file__).parents[3], capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, result.stdout + result.stderr


def _check_native_editor():
    from tempfile import TemporaryDirectory
    from unittest.mock import patch

    from imgui_bundle import imgui, hello_imgui, immapp
    from creation_lib.ui.theme import get_theme
    from creation_lib.ui.theme.editor import ThemeEditor
    from creation_lib.ui.theme import editor as editor_module
    from creation_lib.ui.theme.appearance import configure_runner_appearance
    from creation_lib.ui.widgets.modern import semantic_color
    from ui.toolkit.app import ToolkitApp
    from ui.toolkit.settings import ToolkitSettings

    with TemporaryDirectory() as directory:
        settings = ToolkitSettings(path=Path(directory) / "test.json", editor_settings_path=Path(directory) / "missing.json")
        app = ToolkitApp.__new__(ToolkitApp)
        app._settings = settings
        app._current_theme = get_theme(settings.theme)
        app._theme_editor = ThemeEditor()
        app._show_theme_selector = True
        state = {"frame": 0}
        color_edit = imgui.color_edit4
        action_button = editor_module.action_button

        def capture_edit(*args, **kwargs):
            result = color_edit(*args, **kwargs)
            lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
            state["edit"] = (lo.x + 32, (lo.y + hi.y) / 2)
            return result

        def capture_button(label, **kwargs):
            result = action_button(label, **kwargs)
            lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
            state[label] = ((lo.x + hi.x) / 2, (lo.y + hi.y) / 2)
            return result

        def events():
            frame = state["frame"]
            io = imgui.get_io()
            io.add_focus_event(True)
            if frame in (3, 21):
                io.add_mouse_pos_event(*state["edit"])
                io.add_mouse_button_event(0, True)
            if frame in (4, 22):
                io.add_mouse_button_event(0, False)
            if frame in (5, 23):
                io.add_key_event(imgui.Key.mod_ctrl, True)
                io.add_key_event(imgui.Key.a, True)
            if frame in (6, 24):
                io.add_key_event(imgui.Key.a, False)
                io.add_key_event(imgui.Key.mod_ctrl, False)
                io.add_input_characters_utf8("#112233FF" if frame == 6 else "#445566FF")
            if frame in (7, 25):
                io.add_key_event(imgui.Key.enter, True)
            if frame in (8, 26):
                io.add_key_event(imgui.Key.enter, False)
            if frame in (12, 29):
                io.add_mouse_pos_event(*state["Save" if frame == 12 else "Cancel"])
                io.add_mouse_button_event(0, True)
            if frame in (13, 30):
                io.add_mouse_button_event(0, False)

        def draw():
            app._draw_theme_editor()
            frame = state["frame"]
            if frame in (0, 18):
                app._theme_editor.search = "selected, focused"
            if frame == 0:
                app._theme_editor.set_color("status_warning", [0.1, 0.9, 0.1, 1])
            if frame == 18:
                app._theme_editor.theme_id = "fallout76"
            if frame == 10:
                assert app._theme_editor.current_overrides["tab_selected"] == pytest.approx([17 / 255, 34 / 255, 51 / 255, 1])
                assert tuple(semantic_color("warning")) == pytest.approx([0.1, 0.9, 0.1, 1])
                assert not settings.theme_colors
            if frame == 16:
                assert not app._theme_editor.is_open
                assert settings.theme_colors[settings.theme]["tab_selected"] == pytest.approx([17 / 255, 34 / 255, 51 / 255, 1])
                app._show_theme_selector = True
            if frame == 28:
                assert app._theme_editor.current_overrides["tab_selected"] == pytest.approx([68 / 255, 85 / 255, 102 / 255, 1])
            if frame == 33:
                assert not app._theme_editor.is_open
                assert settings.theme == "falloutnv"
                assert "fallout76" not in settings.theme_colors
                color = tuple(imgui.get_style_color_vec4(imgui.Col_.tab_selected))
                assert color == pytest.approx([17 / 255, 34 / 255, 51 / 255, 1])
                restored = ToolkitSettings(path=Path(directory) / "test.json", editor_settings_path=Path(directory) / "missing.json")
                assert restored.theme_colors == settings.theme_colors
                params.app_shall_exit = True
            state["frame"] += 1

        params = hello_imgui.RunnerParams()
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1280, 760)
        params.dpi_aware_params.dpi_window_size_factor = 1
        params.callbacks.pre_new_frame = events
        params.callbacks.show_gui = draw
        configure_runner_appearance(params, get_theme(settings.theme))
        params.callbacks.post_new_frame = app._apply_tab_style
        with patch.object(editor_module.imgui, "color_edit4", capture_edit), patch.object(editor_module, "action_button", capture_button):
            immapp.run(params)


if __name__ == "__main__":
    from tempfile import TemporaryDirectory

    _check_resolved_colors()
    _check_new_vegas_checkbox_contrast()
    _check_editor_drafts()
    with TemporaryDirectory() as directory:
        _check_persistence(Path(directory))
    _check_native_editor()
