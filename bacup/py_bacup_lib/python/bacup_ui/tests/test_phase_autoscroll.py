from pathlib import Path
import subprocess
import sys

import pytest


@pytest.mark.parametrize("theme,scale", [("falloutnv", 1), ("fallout76", 1.5), ("starfield", 2)])
def test_phase_autoscroll_with_real_imgui(theme, scale, tmp_path):
    result = subprocess.run(
        [sys.executable, "-m", "bacup_ui.tests.test_phase_autoscroll", theme, str(scale), str(tmp_path / "phases.png")],
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def _check_native(theme, scale, output):
    from types import SimpleNamespace
    from unittest.mock import patch

    from imgui_bundle import hello_imgui, imgui, immapp
    from PIL import Image

    from bacup_ui.conversion.panels.regen_panel import RegenPanel
    from creation_lib.ui.theme import get_theme
    from creation_lib.ui.theme.appearance import configure_runner_appearance

    workspace = SimpleNamespace(
        _toolkit_settings=SimpleNamespace(
            get_game_paths=lambda _game: {},
            get_workspace_settings=lambda _name: {},
        ),
        _runner=None,
    )
    panel = RegenPanel(workspace)
    panel._log_panel = SimpleNamespace(draw_body=lambda: pytest.fail("Logs should start hidden"))

    def event(kind, name, status, **data):
        panel.handle_event({"type": kind, "data": {"phase_name": name, "status": status, **data}})

    for index in range(25):
        event("phase_complete", f"Earlier phase {index + 1}", "completed")
    event("phase_start", "Building FO4 target-asset catalog", "running")

    params = hello_imgui.RunnerParams()
    params.ini_disable = True
    params.app_window_params.hidden = True
    params.app_window_params.window_geometry.size = (int(350 * scale), int(650 * scale))
    params.app_window_params.window_geometry.window_size_measure_mode = hello_imgui.WindowSizeMeasureMode.screen_coords
    params.dpi_aware_params.dpi_window_size_factor = scale
    frame = 0
    positions = {}
    begin_child = imgui.begin_child

    def inspect_child(name, *args, **kwargs):
        visible = begin_child(name, *args, **kwargs)
        if name == "progress##appalachia":
            positions[frame] = (imgui.get_scroll_y(), imgui.get_scroll_max_y())
            if frame == 4:
                imgui.set_scroll_y(0)
        return visible

    def draw():
        nonlocal frame
        if frame == 6:
            event("item_progress", "Building FO4 target-asset catalog", "running", current_item="Still indexing")
        elif frame == 8:
            event("phase_complete", "Building FO4 target-asset catalog", "completed")
        elif frame == 11:
            event("phase_start", "Finalize Plugin Records", "running", current_item="Regenerating MODT and finalizing plugin records")
        elif frame == 14:
            event("item_progress", "Finalize Plugin Records", "running", total_items=3, completed_items=2, current_item="Saving updated plugin")
        viewport = imgui.get_main_viewport()
        imgui.set_next_window_pos(viewport.work_pos)
        imgui.set_next_window_size(viewport.work_size)
        imgui.begin("Phase scrolling", flags=imgui.WindowFlags_.no_decoration)
        panel._draw_status_column()
        imgui.end()
        if frame == 16:
            params.app_shall_exit = True
        frame += 1

    params.callbacks.show_gui = draw
    configure_runner_appearance(params, get_theme(theme))
    with patch.object(imgui, "begin_child", inspect_child):
        immapp.run(params)
    for frame in (3, 10, 13):
        position, maximum = positions[frame]
        assert maximum > 0
        assert position == pytest.approx(maximum, abs=1), (frame, positions)
    for frame in (5, 7):
        assert positions[frame][0] == pytest.approx(0, abs=1), (frame, positions)
    assert not panel._phase_scroll_frames
    event("phase_complete", "Finalize Plugin Records", "completed")
    assert panel._phase_scroll_frames
    panel._reset_progress()
    assert not panel._phase_scroll_frames
    output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(output)


if __name__ == "__main__":
    _check_native(sys.argv[1], float(sys.argv[2]), Path(sys.argv[3]))
