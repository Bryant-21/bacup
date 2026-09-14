from types import SimpleNamespace
from unittest.mock import MagicMock

import pytest


@pytest.fixture(autouse=True)
def imgui_layout_metrics(monkeypatch):
    from imgui_bundle import icons_fontawesome_6 as fa, imgui

    if not isinstance(imgui, MagicMock):
        return
    for name, glyph in (("ICON_FA_PLAY", "\uf04b"), ("ICON_FA_STOP", "\uf04d"), ("ICON_FA_FOLDER_OPEN", "\uf07c")):
        monkeypatch.setattr(fa, name, glyph)
    def vector(x, y):
        return SimpleNamespace(x=x, y=y)
    monkeypatch.setattr(imgui, "get_font_size", lambda: 16.0)
    monkeypatch.setattr(imgui, "get_frame_height", lambda: 28.0)
    monkeypatch.setattr(imgui, "get_text_line_height", lambda: 16.0)
    monkeypatch.setattr(imgui, "get_text_line_height_with_spacing", lambda: 25.0)
    monkeypatch.setattr(imgui, "get_content_region_avail", lambda: vector(640.0, 480.0))
    monkeypatch.setattr(imgui, "get_cursor_screen_pos", lambda: vector(0.0, 0.0))
    monkeypatch.setattr(imgui, "get_main_viewport", lambda: SimpleNamespace(
        work_size=vector(1280.0, 760.0), get_center=lambda: vector(640.0, 380.0),
    ))
    monkeypatch.setattr(imgui, "calc_text_size", lambda *_args, **_kwargs: vector(100.0, 16.0))
    monkeypatch.setattr(imgui, "get_style_color_vec4", lambda _role: SimpleNamespace(x=.1, y=.1, z=.1, w=1.0))
    monkeypatch.setattr(imgui, "get_style", lambda: SimpleNamespace(
        item_spacing=vector(10.0, 9.0), frame_padding=vector(10.0, 6.0), alpha=1.0,
    ))
    monkeypatch.setattr(imgui, "invisible_button", lambda *_args, **_kwargs: False)
