from pathlib import Path
from unittest.mock import MagicMock, patch


def test_resolve_launch_target_routes_behavior_hkx_by_filename():
    from ui.toolkit.app import ToolkitApp

    target = ToolkitApp._resolve_launch_target(r"C:\tmp\Weap\Behavior.HKX")

    assert target == ("behavior", str(Path(r"C:\tmp\Weap\Behavior.HKX").resolve(strict=False)))


def test_resolve_launch_target_routes_other_hkx_to_viewer():
    from ui.toolkit.app import ToolkitApp

    target = ToolkitApp._resolve_launch_target(r"C:\tmp\Weap\idle.hkx")

    assert target == ("hkx_viewer", str(Path(r"C:\tmp\Weap\idle.hkx").resolve(strict=False)))


def test_behavior_workspace_open_file_imports_hkx():
    from ui.toolkit.workspaces.behavior_workspace import BehaviorWorkspace

    ws = BehaviorWorkspace()
    ws._app = MagicMock()
    ws._app.model.nodes = {90: {}}
    ws._app.model.connections = [[90, 91, "out", "in"]]
    ws._app._auto_layout_on_import = True

    ws.open_file(r"C:\tmp\Behavior.hkx")

    ws._app.model.import_hkx.assert_called_once_with(r"C:\tmp\Behavior.hkx")
    assert ws._app._status_msg == "Imported HKX: 1 nodes, 1 connections"
    assert ws._app.prop_editor.selected_node_id is None
    ws._app.canvas.request_layout.assert_called_once_with()


def test_hkx_viewer_tool_open_file_unpacks_to_xml(tmp_path):
    from ui.tools.animation.hkx_viewer import HKXViewerTool

    tool = HKXViewerTool()
    hkx_path = tmp_path / "idle.hkx"
    hkx_path.write_bytes(b"hkx")

    with patch("creation_lib._native.havok_native.hkx_to_xml", return_value="<hkpackfile />"):
        tool.open_file(str(hkx_path))

    assert tool._input_path == str(hkx_path)
    assert tool._xml_text == "<hkpackfile />"
    assert tool._result_msg == "Loaded HKX: idle.hkx"


def test_hkx_viewer_tool_save_file_writes_xml(tmp_path):
    from ui.tools.animation.hkx_viewer import HKXViewerTool

    tool = HKXViewerTool()
    tool._xml_text = "<hkpackfile />"
    tool._dirty = True

    out_path = tmp_path / "idle.xml"
    tool.save_file(str(out_path))

    assert out_path.read_text(encoding="utf-8") == "<hkpackfile />"
    assert tool._result_msg == "Saved XML: idle.xml"
    assert tool._dirty is False


def test_hkx_viewer_tool_save_file_packs_hkx(tmp_path):
    from ui.tools.animation.hkx_viewer import HKXViewerTool

    tool = HKXViewerTool()
    tool._xml_text = "<hkpackfile />"
    tool._dirty = True
    out_path = tmp_path / "idle.hkx"

    with patch("creation_lib._native.havok_native.xml_to_hkx", return_value=b"HKXBYTES") as mock_pack:
        tool.save_file(str(out_path))

    mock_pack.assert_called_once_with("<hkpackfile />")
    assert out_path.read_bytes() == b"HKXBYTES"
    assert tool._result_msg == "Saved HKX: idle.hkx"
    assert tool._dirty is False
