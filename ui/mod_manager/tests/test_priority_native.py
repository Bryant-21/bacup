"""Rendered native input through editing, authoritative review and local badges."""
from pathlib import Path

from imgui_bundle import imgui
import pytest

from creation_lib.mod_manager import runtime

from ui.mod_manager import mod_list_ui
from creation_lib.mod_manager.launcher import Executable
from . import test_widget_ids as widgets
from .test_launch_resolution import installation
from .test_local_status import wait_scan
from .test_pe_metadata import fixture_version

native, workspace = widgets.native, widgets.workspace


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.0), ((1280, 800), 1.25), ((1600, 900), 1.5)])
def test_edit_executable_default_save_cancel_authoritative_review_and_visible_badges(native, workspace, tmp_path, monkeypatch, size, scale):
    native.io.display_size = size
    imgui.get_style().font_size_base = 16
    imgui.get_style().font_scale_main = scale
    workspace._show_filters = False
    manager = workspace.manager
    game = installation(tmp_path / "selected loader installation")
    stale = installation(tmp_path / "previous installation", (1, 11, 240, 0))
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)
    for row in manager.rows:
        if row.id.startswith("duplicate-"):
            manager.library.customize(manager.profile_id, row.id, enabled=False)
    item = manager.save_executable(Executable("preconfigured", "Imported F4SE", str(game / "f4se_loader.exe"), "-v", str(game)))
    workspace.launcher.selected_id = item.id
    workspace.launcher.game_root = str(stale)
    bundle = tmp_path / "inert bundle"
    bundle.mkdir()
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: bundle)
    users = (tmp_path / "user-plugins", tmp_path / "user-inis")
    for path in users:
        path.mkdir()
    monkeypatch.setattr(runtime, "fo4_user_targets", lambda: users)
    before = {str(path): path.read_bytes() for folder in (game, stale, Path(manager.import_preview.root)) for path in folder.rglob("*") if path.is_file()}
    draw = widgets.panels(workspace)
    observed = []
    original = mod_list_ui.draw_badge
    def record(draw_list, badge, *args):
        observed.append((badge.text, badge.description))
        original(draw_list, badge, *args)
    monkeypatch.setattr(mod_list_ui, "draw_badge", record)
    native.warm(draw)
    button = native.find("Launch options")
    assert button.rect[3] - button.rect[1] >= 20 * scale
    assert 0 <= button.rect[0] < button.rect[2] <= size[0]
    tabs = [item for item in native.items if item.label == "Plugins"]
    assert tabs and tabs[0].rect[3] - tabs[0].rect[1] >= 20 * scale
    native.click(button, draw)
    native.click(native.find("Edit executables..."), draw)
    native.text(native.find("##exe_title"), draw, "Draft must cancel")
    native.click(native.find("Cancel"), draw)
    assert manager.executables[0].title == "Imported F4SE"
    native.click(native.find("Launch options"), draw)
    native.click(native.find("Edit executables..."), draw)
    native.text(native.find("##exe_title"), draw, "My external F4SE")
    native.text(native.find("##exe_arguments"), draw, "-v -waitforclose")
    native.click(native.find("Set as default"), draw)
    native.click(native.find("Save and close"), draw)
    assert not workspace._dialog and workspace.launcher.selected().title == "My external F4SE"
    native.click(native.find("Run"), draw)
    widgets.finish_job(workspace, native, draw)
    plan = manager.launch_review
    assert plan and plan.game_root == str(game) and plan.target_id == manager.target_id
    assert plan.launch.argv == (str(game / "f4se_loader.exe"), "-v", "-waitforclose")
    native.click(native.find("Cancel launch"), draw)
    assert manager.launch_review is None and not manager.runtime_session
    assert workspace.launcher.selected().arguments == "-v -waitforclose"
    wait_scan(manager)
    observed.clear()
    native.warm(draw)
    assert any(text == "W" for text, _ in observed) and any(text == "L" for text, _ in observed)
    assert any("Observed loose-file providers" in description for _, description in observed)
    assert "status" in {field for _, field in workspace._field_rects}
    assert {str(path): path.read_bytes() for folder in (game, stale, Path(manager.import_preview.root)) for path in folder.rglob("*") if path.is_file()} == before
