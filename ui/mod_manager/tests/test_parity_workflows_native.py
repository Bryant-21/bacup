"""Actual ImGui input for new source notices, local rules and owned file review."""
from pathlib import Path
import time

from imgui_bundle import imgui
import pytest

from . import test_widget_ids as widgets
from .test_plugin_file_actions import owned as owned

native, workspace = widgets.native, widgets.workspace


def observe(m, native, draw, count):
    m.metadata_observer.due = 0
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        native.frame(draw)
        index = m.metadata_observer.index
        if index and index.count == count and m.metadata_observer.future is None:
            return index
        time.sleep(.005)
    raise AssertionError((m.metadata_observer.error, m.metadata_observer.index))


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.25), ((1280, 800), 1.5)])
def test_notice_review_cancel_stale_invalidation_and_dismiss_actual_input(native, workspace, size, scale):
    native.io.display_size = size
    imgui.get_style().font_size_base = 16
    imgui.get_style().font_scale_main = scale
    m, draw = workspace.manager, widgets.panels(workspace)
    native.warm(draw)
    observe(m, native, draw, 0)
    row = next(r for r in m.rows if Path(r.source).name == "Base")
    path = Path(row.source) / "meta.ini"
    before = path.read_bytes()
    path.write_bytes(before.replace(b"version=1.2", b"version=2.4"))
    observe(m, native, draw, 1)
    native.click(native.find("Source metadata changed (1)"), draw)
    native.warm(draw)
    found = [i for i in native.items if i.label == row.name + " - mod changed"]
    assert found, (workspace._dialog, [i.label for i in native.items if "workflow" in i.window], [(n.name, n.change) for n in m.metadata_observer.index.notices])
    item = found[0]
    native.click(item, draw)
    native.click(native.find("Review changed source fields"), draw)
    widgets.finish_job(workspace, native, draw)
    assert workspace._dialog == "Refresh source metadata" and not workspace.source_metadata.selected
    native.click(native.find("Cancel metadata refresh"), draw)
    assert m.rows_by_id[row.id].version == "1.2"
    workspace.metadata_notifications.open()
    native.warm(draw)
    native.click(native.find(row.name + " - mod changed"), draw)
    native.click(native.find("Review changed source fields"), draw)
    widgets.finish_job(workspace, native, draw)
    assert m.source_metadata_review
    path.write_bytes(before.replace(b"version=1.2", b"version=3.7"))
    # Refresh detects a new observation even though the notice count stays one.
    m.metadata_observer.due = 0
    deadline = time.monotonic() + 5
    while m.source_metadata_review and time.monotonic() < deadline:
        native.frame(draw)
        time.sleep(.005)
    assert m.source_metadata_review is None
    assert m.rows_by_id[row.id].version == "1.2"
    native.click(native.find("Cancel metadata refresh"), draw)
    workspace.metadata_notifications.open()
    native.warm(draw)
    native.click(native.find(row.name + " - mod changed"), draw)
    native.click(native.find("Dismiss this metadata notice"), draw)
    observe(m, native, draw, 0)
    assert path.read_bytes() == before.replace(b"version=1.2", b"version=3.7")




@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.25), ((1280, 800), 1.5)])
def test_owned_ghost_review_cancel_apply_undo_actual_input(native, workspace, owned, size, scale):
    native.io.display_size = size
    imgui.get_style().font_size_base = 16
    imgui.get_style().font_scale_main = scale
    library, profile, _, catalog, plugin, _ = owned
    m = workspace.manager
    m.library, m.profile_id = library, profile
    m.refresh()
    m.plugin_catalog = catalog
    draw = widgets.panels(workspace)
    workspace.plugin_file_ui.open(plugin.name)
    native.warm(draw)
    native.choose_next("Owned file action", draw)
    native.click(native.find("Review owned plugin action"), draw)
    widgets.finish_job(workspace, native, draw)
    assert workspace.plugin_file_ui.plan.action == "ghost"
    native.click(native.find("Cancel owned plugin action"), draw)
    assert Path(plugin.path).exists()
    workspace.plugin_file_ui.open(plugin.name)
    native.warm(draw)
    native.choose_next("Owned file action", draw)
    native.click(native.find("Review owned plugin action"), draw)
    widgets.finish_job(workspace, native, draw)
    footer = native.find("Apply owned plugin action")
    assert footer.rect[3] <= size[1]
    native.click(footer, draw)
    widgets.finish_job(workspace, native, draw)
    assert not Path(plugin.path).exists() and Path(plugin.path + ".ghost").exists()
    workspace.plugin_file_ui.undo()
    widgets.finish_job(workspace, native, draw)
    assert Path(plugin.path).exists() and not Path(plugin.path + ".ghost").exists()


@pytest.mark.parametrize("outcome", ["stale", "failed"])
def test_owned_apply_error_preserves_current_file_and_cancel_remains_available(native, workspace, owned, monkeypatch, outcome):
    library, profile, _, catalog, plugin, _ = owned
    m = workspace.manager
    m.library, m.profile_id = library, profile
    m.refresh()
    m.plugin_catalog = catalog
    draw = widgets.panels(workspace)
    workspace.plugin_file_ui.open(plugin.name)
    native.warm(draw)
    native.choose_next("Owned file action", draw)
    native.click(native.find("Review owned plugin action"), draw)
    widgets.finish_job(workspace, native, draw)
    original = Path(plugin.path).read_bytes()
    if outcome == "stale":
        original = b"generated later owned edit"
        Path(plugin.path).write_bytes(original)
    else:
        def fail(phase):
            if phase == "placed":
                raise OSError("generated owned publish failure")
        monkeypatch.setattr("creation_lib.mod_manager.plugin_file_actions._checkpoint", fail)
    native.click(native.find("Apply owned plugin action"), draw)
    m._future.exception(timeout=10)
    m.poll()
    native.warm(draw)
    assert m.job_state == "failed" and m.error
    assert Path(plugin.path).read_bytes() == original and not Path(plugin.path + ".ghost").exists()
    native.click(native.find("Cancel owned plugin action"), draw)
    assert not workspace._dialog
