from __future__ import annotations

import sys
import time
import types

_hello = types.SimpleNamespace(
    DockableWindow=lambda **k: types.SimpleNamespace(**k),
    get_runner_params=lambda: types.SimpleNamespace(
        docking_params=types.SimpleNamespace(dockable_windows=[]),
        fps_idling=types.SimpleNamespace(fps_idle=60.0),
    ),
)
_fa = types.SimpleNamespace()


class _ImVec2:
    def __init__(self, x=0.0, y=0.0):
        self.x = x
        self.y = y


_imgui_ns = types.SimpleNamespace(ImVec2=_ImVec2)
_im_guizmo_inner = types.SimpleNamespace(
    OPERATION=types.SimpleNamespace(translate=0, rotate=1, scale=2),
    MODE=types.SimpleNamespace(local=0, world=1),
    Matrix16=list,
    begin_frame=lambda: None,
    set_draw_list=lambda *a: None,
    set_rect=lambda *a: None,
    manipulate=lambda *a, **k: False,
    view_manipulate=lambda *a, **k: None,
)
_imguizmo = types.SimpleNamespace(im_guizmo=_im_guizmo_inner)
sys.modules.setdefault(
    "imgui_bundle",
    types.SimpleNamespace(
        imgui=_imgui_ns,
        hello_imgui=_hello,
        portable_file_dialogs=types.SimpleNamespace(),
        icons_fontawesome_6=_fa,
        ImVec2=_ImVec2,
        imguizmo=_imguizmo,
    ),
)
sys.modules.setdefault("imgui_bundle.hello_imgui", _hello)
sys.modules.setdefault("imgui_bundle.icons_fontawesome_6", _fa)
sys.modules.setdefault("imgui_bundle.imguizmo", _imguizmo)

from ui.lodgen.app import LodgenApp  # noqa: E402


class _Settings:
    def get_game_paths(self, game):
        return {"root_dir": "C:/Games/FO4", "extracted_dir": "C:/extract", "additional_paths": ["E:/more"]}


def test_data_dirs_from_toolkit_settings():
    app = LodgenApp(toolkit_settings=_Settings())
    app.state.game = "fo4"
    dirs = app.data_dirs()
    assert "C:/extract" in dirs
    assert any(d.endswith("Data") for d in dirs)
    assert "E:/more" in dirs


def test_start_generate_runs_thread_and_polls(monkeypatch):
    app = LodgenApp(toolkit_settings=_Settings())
    app.state.worldspace = "DLC03FarHarbor"
    app.state.output_dir = "C:/out"

    def _fake_generate(world, settings, *, data_dirs, output_dir, progress):
        progress("terrain", 0.5)
        class _R:
            btr = 5; bto = 2; btt = 0; dds = 10; lod_written = True; warnings = ()
        return _R()

    monkeypatch.setattr("ui.lodgen.app.generate_lod", _fake_generate)
    app.start_generate()
    assert app.state.running is True
    # drain
    for _ in range(200):
        app.poll()
        if not app.state.running:
            break
        time.sleep(0.01)
    assert app.state.running is False
    assert app.state.last_result.btr == 5
    assert app.state.progress_frac == 0.5
    assert any("terrain" in line for line in app.state.log_lines)


def test_start_generate_requires_worldspace():
    app = LodgenApp(toolkit_settings=_Settings())
    app.state.worldspace = ""
    app.start_generate()
    assert app.state.running is False
    assert app.state.error_message
