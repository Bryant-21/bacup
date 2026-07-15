from types import SimpleNamespace
from unittest.mock import MagicMock


def test_toolkit_routes_file_drop_to_active_workspace():
    from ui.toolkit.app import ToolkitApp

    app = ToolkitApp.__new__(ToolkitApp)
    workspace = SimpleNamespace(
        handle_file_drop=MagicMock(return_value=True),
    )
    app._active_ws = workspace

    handled = ToolkitApp._handle_dropped_files(
        app,
        [r"C:\tmp\weapon.nif"],
        x=12.0,
        y=34.0,
    )

    assert handled is True
    workspace.handle_file_drop.assert_called_once_with(
        [r"C:\tmp\weapon.nif"],
        x=12.0,
        y=34.0,
    )


def test_nif_workspace_delegates_file_drop_to_editor_app():
    from ui.toolkit.workspaces.nif_workspace import NifWorkspace

    workspace = NifWorkspace.__new__(NifWorkspace)
    workspace._app = SimpleNamespace(
        handle_file_drop=MagicMock(return_value=True),
    )

    handled = NifWorkspace.handle_file_drop(
        workspace,
        [r"C:\tmp\weapon.nif"],
        x=12.0,
        y=34.0,
    )

    assert handled is True
    workspace._app.handle_file_drop.assert_called_once_with(
        [r"C:\tmp\weapon.nif"],
        x=12.0,
        y=34.0,
    )


def test_papyrus_workspace_drop_opens_psc():
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace.__new__(PapyrusWorkspace)
    ws._app = MagicMock()
    ws.open_file = MagicMock()

    handled = PapyrusWorkspace.handle_file_drop(
        ws, [r"C:\tmp\notes.txt", r"C:\tmp\Foo.psc"], x=0.0, y=0.0
    )

    assert handled is True
    ws.open_file.assert_called_once_with(r"C:\tmp\Foo.psc")


def test_papyrus_workspace_drop_opens_pex():
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace.__new__(PapyrusWorkspace)
    ws._app = MagicMock()
    ws.open_file = MagicMock()

    handled = PapyrusWorkspace.handle_file_drop(ws, [r"C:\tmp\Bar.pex"])

    assert handled is True
    ws.open_file.assert_called_once_with(r"C:\tmp\Bar.pex")


def test_papyrus_workspace_drop_ignores_unsupported():
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace.__new__(PapyrusWorkspace)
    ws._app = MagicMock()
    ws.open_file = MagicMock()

    handled = PapyrusWorkspace.handle_file_drop(ws, [r"C:\tmp\notes.txt"])

    assert handled is False
    ws.open_file.assert_not_called()


def test_esp_editor_workspace_drop_opens_plugin():
    from ui.esp_editor.workspace import EspEditorWorkspace

    ws = EspEditorWorkspace.__new__(EspEditorWorkspace)
    ws._app = MagicMock()

    handled = EspEditorWorkspace.handle_file_drop(
        ws, [r"C:\tmp\notes.txt", r"C:\tmp\Mod.esp"]
    )

    assert handled is True
    ws._app.open_plugin.assert_called_once_with(r"C:\tmp\Mod.esp")


def test_esp_editor_workspace_drop_accepts_esm_and_esl():
    from ui.esp_editor.workspace import EspEditorWorkspace

    for path in (r"C:\tmp\Master.esm", r"C:\tmp\Light.esl"):
        ws = EspEditorWorkspace.__new__(EspEditorWorkspace)
        ws._app = MagicMock()

        handled = EspEditorWorkspace.handle_file_drop(ws, [path])

        assert handled is True
        ws._app.open_plugin.assert_called_once_with(path)


def test_esp_editor_workspace_drop_ignores_unsupported():
    from ui.esp_editor.workspace import EspEditorWorkspace

    ws = EspEditorWorkspace.__new__(EspEditorWorkspace)
    ws._app = MagicMock()

    handled = EspEditorWorkspace.handle_file_drop(ws, [r"C:\tmp\Foo.nif"])

    assert handled is False
    ws._app.open_plugin.assert_not_called()


def test_swf_editor_workspace_drop_opens_swf():
    from ui.toolkit.workspaces.swf_editor_workspace import SwfEditorWorkspace

    ws = SwfEditorWorkspace.__new__(SwfEditorWorkspace)
    ws._app = MagicMock()

    handled = SwfEditorWorkspace.handle_file_drop(
        ws, [r"C:\tmp\notes.txt", r"C:\tmp\Pipboy.swf"]
    )

    assert handled is True
    ws._app.open_swf.assert_called_once_with(r"C:\tmp\Pipboy.swf")


def test_swf_editor_workspace_drop_ignores_unsupported():
    from ui.toolkit.workspaces.swf_editor_workspace import SwfEditorWorkspace

    ws = SwfEditorWorkspace.__new__(SwfEditorWorkspace)
    ws._app = MagicMock()

    handled = SwfEditorWorkspace.handle_file_drop(ws, [r"C:\tmp\notes.txt"])

    assert handled is False
    ws._app.open_swf.assert_not_called()


def test_behavior_workspace_drop_opens_hkx():
    from ui.toolkit.workspaces.behavior_workspace import BehaviorWorkspace

    ws = BehaviorWorkspace.__new__(BehaviorWorkspace)
    ws._app = MagicMock()
    ws.open_file = MagicMock()

    handled = BehaviorWorkspace.handle_file_drop(
        ws, [r"C:\tmp\notes.txt", r"C:\tmp\Behavior.hkx"]
    )

    assert handled is True
    ws.open_file.assert_called_once_with(r"C:\tmp\Behavior.hkx")


def test_behavior_workspace_drop_opens_xml():
    from ui.toolkit.workspaces.behavior_workspace import BehaviorWorkspace

    ws = BehaviorWorkspace.__new__(BehaviorWorkspace)
    ws._app = MagicMock()

    handled = BehaviorWorkspace.handle_file_drop(ws, [r"C:\tmp\Behavior.xml"])

    assert handled is True
    ws._app._import_xml_path.assert_called_once_with(r"C:\tmp\Behavior.xml")


def test_behavior_workspace_drop_ignores_unsupported():
    from ui.toolkit.workspaces.behavior_workspace import BehaviorWorkspace

    ws = BehaviorWorkspace.__new__(BehaviorWorkspace)
    ws._app = MagicMock()
    ws.open_file = MagicMock()

    handled = BehaviorWorkspace.handle_file_drop(ws, [r"C:\tmp\Foo.nif"])

    assert handled is False
    ws.open_file.assert_not_called()
    ws._app._import_xml_path.assert_not_called()
