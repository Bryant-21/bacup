from types import SimpleNamespace
from unittest.mock import MagicMock, patch


def test_loading_overlay_uses_cross_version_path_stroke_signature():
    from ui.editor import app as app_module

    app = app_module.NifEditorApp.__new__(app_module.NifEditorApp)
    app._viewport_label = "Viewport"
    app.renderer = None
    app.status_text = ""
    app._loading = True
    app._attaching = False
    app._loading_filename = "example.nif"

    draw_list = MagicMock()
    fake_imgui = MagicMock()
    fake_imgui.WindowFlags_.no_scrollbar.value = 1
    fake_imgui.WindowFlags_.no_scroll_with_mouse.value = 2
    fake_imgui.begin_popup.return_value = False
    fake_imgui.get_cursor_screen_pos.return_value = SimpleNamespace(x=10.0, y=20.0)
    fake_imgui.get_content_region_avail.return_value = SimpleNamespace(x=320.0, y=200.0)
    fake_imgui.get_window_height.return_value = 240.0
    fake_imgui.get_text_line_height_with_spacing.return_value = 16.0
    fake_imgui.get_foreground_draw_list.return_value = draw_list
    fake_imgui.get_time.return_value = 0.0
    fake_imgui.get_color_u32.return_value = 0xFFFFFFFF
    fake_imgui.ImVec2.side_effect = lambda x, y: SimpleNamespace(x=x, y=y)
    fake_imgui.calc_text_size.return_value = SimpleNamespace(x=80.0)

    with patch.object(app_module, "imgui", fake_imgui):
        app._draw_viewport()

    draw_list.path_stroke.assert_called_once_with(0xFFFFFFFF, thickness=3.0)
