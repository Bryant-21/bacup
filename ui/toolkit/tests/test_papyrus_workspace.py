from pathlib import Path
from unittest.mock import MagicMock, patch


def test_resolve_launch_target_routes_papyrus_extensions():
    from ui.toolkit.app import ToolkitApp

    psc_target = ToolkitApp._resolve_launch_target(r"C:\tmp\TestScript.psc")
    pex_target = ToolkitApp._resolve_launch_target(r"C:\tmp\TestScript.pex")

    assert psc_target == ("papyrus", str(Path(r"C:\tmp\TestScript.psc").resolve(strict=False)))
    assert pex_target == ("papyrus", str(Path(r"C:\tmp\TestScript.pex").resolve(strict=False)))


def test_papyrus_workspace_open_file_opens_psc_directly():
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace()
    ws._app = MagicMock()

    ws.open_file(r"C:\tmp\TestScript.psc")

    ws._app.open_file.assert_called_once_with(r"C:\tmp\TestScript.psc")
    assert ws._app.active_path == r"C:\tmp\TestScript.psc"


def test_papyrus_workspace_open_file_decompiles_pex(tmp_path):
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace()
    ws._app = MagicMock()
    buf = MagicMock()
    ws._app.open_files = {}

    pex_path = tmp_path / "TestScript.pex"
    pex_path.write_bytes(b"pex")

    decompiled_path = ws._decompiled_pex_path(str(pex_path))
    ws._app.open_files[str(decompiled_path)] = buf

    with patch("creation_lib.pex.decompile_pex", return_value="Scriptname TestScript\n"):
        ws.open_file(str(pex_path))

    ws._app.open_file.assert_called_once_with(str(decompiled_path))
    assert decompiled_path.read_text(encoding="utf-8") == "Scriptname TestScript\n"
    assert ws._app.active_path == str(decompiled_path)
    buf.editor.set_read_only_enabled.assert_called_once_with(True)


def test_papyrus_workspace_toolbar_no_longer_emits_local_help_button(monkeypatch):
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace()
    ws._app = type(
        "App",
        (),
        {
            "active_path": None,
            "open_files": {},
            "save_file": lambda self, path: None,
        },
    )()
    ws._editor_tabs = type(
        "Tabs",
        (),
        {"font_scale": 1.0, "extra_line_spacing": 0.0},
    )()

    labels: list[str] = []
    tooltips: list[str] = []
    toggles = {"count": 0}

    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.fa.ICON_FA_CIRCLE_QUESTION", "LOCAL_HELP_SENTINEL")

    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.push_font", lambda *args, **kwargs: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.pop_font", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        "ui.toolkit.workspaces.papyrus_workspace.imgui.button",
        lambda label: labels.append(label) or (label == "LOCAL_HELP_SENTINEL"),
    )
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.set_item_tooltip", lambda text: tooltips.append(text))
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.same_line", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.text", lambda *args, **kwargs: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.begin_disabled", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.end_disabled", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.begin_popup", lambda *args, **kwargs: False)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.end_popup", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.open_popup", lambda *args, **kwargs: None)
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.slider_float", lambda *args, **kwargs: (False, 1.0))
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.begin_menu", lambda *args, **kwargs: False)
    monkeypatch.setattr(
        ws,
        "_toggle_help_panel",
        lambda: toggles.__setitem__("count", toggles["count"] + 1),
    )

    ws.draw_toolbar()

    assert "LOCAL_HELP_SENTINEL" not in labels
    assert toggles["count"] == 0
    assert "User Guide (F1)" not in tooltips


def test_aligner_workspace_toolbar_no_longer_emits_local_help_button(monkeypatch):
    from ui.toolkit.workspaces.aligner_workspace import AlignerWorkspace

    ws = AlignerWorkspace()
    ws._app = type("App", (), {"skinned_meshes": []})()

    labels: list[str] = []
    toggles = {"count": 0}

    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.fa.ICON_FA_CIRCLE_QUESTION", "LOCAL_HELP_SENTINEL")
    monkeypatch.setattr(
        "ui.toolkit.workspaces.aligner_workspace.imgui.button",
        lambda label: labels.append(label) or (label == "LOCAL_HELP_SENTINEL"),
    )
    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.imgui.push_style_color", lambda *args, **kwargs: None)
    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.imgui.pop_style_color", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.imgui.set_item_tooltip", lambda *args, **kwargs: None)
    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.imgui.same_line", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.imgui.begin_disabled", lambda: None)
    monkeypatch.setattr("ui.toolkit.workspaces.aligner_workspace.imgui.end_disabled", lambda: None)
    monkeypatch.setattr(
        ws,
        "_toggle_help_panel",
        lambda: toggles.__setitem__("count", toggles["count"] + 1),
    )

    ws.draw_toolbar()

    assert "LOCAL_HELP_SENTINEL" not in labels
    assert toggles["count"] == 0
