from pathlib import Path
import subprocess
import sys


def test_effect_card_actions_and_saved_expansion_with_real_imgui():
    result = subprocess.run([sys.executable, "-m", "ui.tests.test_expandable_effect_cards"],
                            cwd=Path(__file__).resolve().parents[2],
                            capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, result.stdout + result.stderr


def _check_effect_cards():
    from types import SimpleNamespace
    from unittest.mock import patch

    from imgui_bundle import hello_imgui, imgui, immapp
    from creation_lib.ui.theme import get_theme
    from creation_lib.ui.theme.appearance import configure_runner_appearance
    from ui.voice_changer.panels.filter_builder_panel import FilterBuilderPanel

    panel = FilterBuilderPanel(SimpleNamespace(vst3_plugins=[]))
    panel.restore_settings({"card_0": True, "card_1": False})
    nodes = [{"type": "HighpassFilter", "enabled": True, "params": {"cutoff_frequency_hz": 300.0}},
             {"type": "LowpassFilter", "enabled": True, "params": {"cutoff_frequency_hz": 4000.0}}]
    positions = {}
    state = {"frame": 0, "deleted": False, "swap": None}
    checkbox, button, header = imgui.checkbox, imgui.button, imgui.collapsing_header

    def remember(key, *, left=False):
        lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
        positions[key] = (lo.x + 50 if left else (lo.x + hi.x) / 2, (lo.y + hi.y) / 2)

    def tracked_checkbox(label, *args, **kwargs):
        result = checkbox(label, *args, **kwargs)
        remember(label)
        return result

    def tracked_button(label, *args, **kwargs):
        result = button(label, *args, **kwargs)
        remember(label)
        return result

    def tracked_header(label, *args, **kwargs):
        result = header(label, *args, **kwargs)
        remember(label, left=True)
        return result

    params = hello_imgui.RunnerParams()
    params.ini_disable = True
    params.app_window_params.hidden = True
    params.app_window_params.window_geometry.size = (800, 650)
    params.dpi_aware_params.dpi_window_size_factor = 1

    def events():
        io, frame = imgui.get_io(), state["frame"]
        io.add_focus_event(True)
        for label, start in (("##en_0", 3), ("HighpassFilter##card_0", 7), ("X##del_1", 11)):
            if frame == start:
                io.add_mouse_pos_event(*positions[label])
            if frame in (start + 1, start + 2):
                io.add_mouse_button_event(0, frame == start + 1)
        if frame == 15:
            io.add_mouse_pos_event(*positions["HighpassFilter##card_0"])
        if frame == 16:
            io.add_mouse_button_event(0, True)
        if frame == 17:
            x, y = positions["HighpassFilter##card_0"]
            io.add_mouse_pos_event(x + 20, y)
        if frame in (18, 19):
            io.add_mouse_pos_event(*positions["LowpassFilter##card_1"])
        if frame == 20:
            io.add_mouse_button_event(0, False)

    def draw():
        frame = state["frame"]
        for i, node in enumerate(nodes):
            imgui.push_id(i)
            swap, deleted = panel._draw_effect_card(i, node)
            if swap is not None:
                state["swap"] = swap
            state["deleted"] |= deleted
            imgui.pop_id()
        if frame == 2:
            assert panel.collect_settings()["filter_builder_expanded"] == {"card_0": True, "card_1": False}
        if frame == 6:
            assert not nodes[0]["enabled"], "Enable checkbox did not toggle"
            assert panel._expanded["card_0"], "Enable checkbox collapsed its card"
        if frame == 10:
            assert not panel._expanded["card_0"], "Header click did not update saved expansion"
        if frame == 14:
            assert state["deleted"], "Delete button did not receive its click"
            assert not panel._expanded["card_1"], "Delete button expanded its card"
        state["frame"] += 1
        if frame == 24:
            assert state["swap"] == (0, 1), "Header drag/drop did not return the reordered pair"
            params.app_shall_exit = True

    params.callbacks.pre_new_frame = events
    params.callbacks.show_gui = draw
    configure_runner_appearance(params, get_theme("falloutnv"))
    with patch.object(imgui, "checkbox", tracked_checkbox), patch.object(imgui, "button", tracked_button), \
         patch.object(imgui, "collapsing_header", tracked_header):
        immapp.run(params)


if __name__ == "__main__":
    _check_effect_cards()
