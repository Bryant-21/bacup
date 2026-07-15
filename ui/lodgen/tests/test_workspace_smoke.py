from __future__ import annotations

import sys
import types

_imgui = types.SimpleNamespace(
    begin=lambda *a, **k: (True, True),
    end=lambda: None,
)
_hello = types.SimpleNamespace(
    DockableWindow=lambda **k: types.SimpleNamespace(**k),
    get_runner_params=lambda: types.SimpleNamespace(
        docking_params=types.SimpleNamespace(dockable_windows=[]),
        fps_idling=types.SimpleNamespace(fps_idle=60.0),
    ),
)
_fa = types.SimpleNamespace()
_bundle = types.SimpleNamespace(
    imgui=_imgui,
    hello_imgui=_hello,
    portable_file_dialogs=types.SimpleNamespace(),
    icons_fontawesome_6=_fa,
)
sys.modules.setdefault("imgui_bundle", _bundle)
sys.modules.setdefault("imgui_bundle.hello_imgui", _hello)
sys.modules.setdefault("imgui_bundle.icons_fontawesome_6", _fa)


def test_workspace_registered_in_toolkit():
    from ui.toolkit.workspaces import _WORKSPACE_SPECS
    ids = [spec[0] for spec in _WORKSPACE_SPECS]
    assert "lodgen" in ids
    spec = next(s for s in _WORKSPACE_SPECS if s[0] == "lodgen")
    assert spec == ("lodgen", "ui.lodgen.workspace", "LodgenWorkspace")


def test_workspace_instantiates_and_roundtrips_settings():
    from ui.lodgen.workspace import LodgenWorkspace

    class _Settings:
        def get_game_paths(self, game):
            return {"root_dir": "C:/FO4", "extracted_dir": "C:/x", "additional_paths": []}

    ws = LodgenWorkspace(toolkit_settings=_Settings())
    assert ws.id == "lodgen"
    defaults = ws.get_settings_defaults()
    assert "game" in defaults
    ws.apply_settings({"game": "fo4", "output_dir": "C:/out", "worldspace": "DLC03FarHarbor"})
    collected = ws.collect_settings()
    assert collected["output_dir"] == "C:/out"
    assert collected["worldspace"] == "DLC03FarHarbor"
