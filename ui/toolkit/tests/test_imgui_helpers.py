"""Tests for imgui_helpers form layout helpers."""
from unittest.mock import MagicMock, call, patch
import sys


def _mock_imgui():
    """Return a fresh imgui mock with table API."""
    m = MagicMock()
    m.begin_table.return_value = True
    m.TableFlags_.sizing_fixed_fit = 1
    m.TableFlags_.no_borders_in_body = 2
    m.TableColumnFlags_.width_fixed = 1
    m.TableColumnFlags_.width_stretch = 2
    m.InputTextFlags_.read_only = 1
    # input_* return (changed, value)
    m.input_int.return_value = (False, 5)
    m.input_float.return_value = (False, 1.0)
    m.input_text.return_value = (False, "hello")
    m.combo.return_value = (False, 0)
    m.get_content_region_avail.return_value = MagicMock(x=400.0)
    m.calc_text_size.return_value = MagicMock(x=60.0)
    m.get_style.return_value = MagicMock(
        frame_padding=MagicMock(x=4.0),
        item_spacing=MagicMock(x=8.0),
    )
    return m


def test_begin_form_calls_begin_table():
    imgui_mock = _mock_imgui()
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    result = h.begin_form("##test")
    assert result is True
    imgui_mock.begin_table.assert_called_once()
    args = imgui_mock.begin_table.call_args
    assert args[0][0] == "##test"
    assert args[0][1] == 2  # 2 columns


def test_end_form_calls_end_table():
    imgui_mock = _mock_imgui()
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    h.end_form()
    imgui_mock.end_table.assert_called_once()


def test_draw_int_field_returns_clamped_value():
    imgui_mock = _mock_imgui()
    imgui_mock.input_int.return_value = (True, 999)
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    changed, val = h.draw_int_field("Scale", 999, min_val=1, max_val=8)
    assert changed is True
    assert val == 8  # clamped to max


def test_draw_int_field_no_clamp_when_bounds_not_set():
    imgui_mock = _mock_imgui()
    imgui_mock.input_int.return_value = (True, 50)
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    changed, val = h.draw_int_field("Top N", 50)
    assert val == 50


def test_draw_float_field_advances_table_columns():
    imgui_mock = _mock_imgui()
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    h.draw_float_field("Pitch", 0.5)
    imgui_mock.table_next_row.assert_called_once()
    # Column 0 then column 1
    calls = imgui_mock.table_set_column_index.call_args_list
    assert calls[0] == call(0)
    assert calls[1] == call(1)


def test_draw_text_field_uses_full_width():
    imgui_mock = _mock_imgui()
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    h.draw_text_field("Model", "esrgan")
    imgui_mock.set_next_item_width.assert_called_with(-1)


def test_draw_combo_field_uses_full_width():
    imgui_mock = _mock_imgui()
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    changed, idx = h.draw_combo_field("Method", ["a", "b"], 0)
    imgui_mock.set_next_item_width.assert_called_with(-1)
    assert idx == 0


def test_draw_path_row_calls_table_row():
    imgui_mock = _mock_imgui()
    imgui_mock.input_text.return_value = (False, "/some/path")
    sys.modules["imgui_bundle.imgui"] = imgui_mock
    sys.modules["imgui_bundle"] = MagicMock(imgui=imgui_mock)

    from importlib import reload
    import ui.tools.imgui_helpers as h
    reload(h)

    path, clicked = h.draw_path_row("Input", "/some/path")
    imgui_mock.table_next_row.assert_called_once()
    assert path == "/some/path"  # unchanged since input_text returns unchanged
