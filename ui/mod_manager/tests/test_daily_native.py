"""Daily download/filter journeys through actual ImGui input, using disposable ZIPs."""
from pathlib import Path

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.mo2_migration import augment
from creation_lib.mod_manager.nexus_state import File, State
from .fixtures import make_mo2
from .test_download_install import archive, install_owned, owned_download
from .test_status_query import synthetic
from . import test_widget_ids as widgets
from . import test_launch_targets as target_fixtures
from .test_widget_ids import panels, finish_job, visible_rows

native, workspace = widgets.native, widgets.workspace
targets = target_fixtures.targets


def imported_library(workspace, tmp_path):
    source_root = tmp_path / "daily source"
    source_root.mkdir()
    source = make_mo2(source_root)
    path = archive(source / "downloads" / "imported.zip")
    Path(str(path) + ".meta").write_text("[General]\ngameName=Fallout4\nmodID=123\nfileID=2\nversion=2\n")
    preview = inspect_instance(source)
    augment(preview)
    m = workspace.manager
    m.library = Library(tmp_path / "daily library", create=True)
    m.profile_id = ""
    m.library.import_mo2(preview)
    m.refresh()
    workspace._right_pane, workspace._show_filters = "Downloads", False
    return path


def open_download(native, workspace, draw, *, owned=False):
    native.click(native.find("Downloads"), draw)
    native.click(native.find("Install download" if owned else "##imported_download"), draw)
    finish_job(workspace, native, draw)
    assert workspace._dialog == "Download destination"


def review_install(native, workspace, draw, *, replace=False):
    finish_job(workspace, native, draw)
    assert workspace._dialog == "Install" and workspace.manager.prepared
    if replace:
        native.click(native.find("Replace owned content in all affected profiles; keep backup"), draw)
    native.click(native.find("Review destination files"), draw)
    finish_job(workspace, native, draw)


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_imported_archive_cancel_install_repeat_and_explicit_reinstall_native(native, workspace, tmp_path, size):
    native.io.display_size = size
    path = imported_library(workspace, tmp_path)
    source_bytes = {str(p): p.read_bytes() for p in path.parent.parent.rglob("*") if p.is_file()}
    m, draw = workspace.manager, panels(workspace)
    native.warm(draw)
    open_download(native, workspace, draw)
    native.click(native.find("Review reinstall"), draw)  # No implicit choice.
    assert m.prepared is None and workspace._dialog == "Download destination"
    native.click(native.find("Cancel download install"), draw)
    assert m.download_review is None and not workspace._dialog
    open_download(native, workspace, draw)
    native.click(native.find("Install as a new mod"), draw)
    review_install(native, workspace, draw)
    native.click(native.find("Install into library"), draw)
    finish_job(workspace, native, draw)
    mod = m.selected_id
    assert m.rows_by_id[mod].name == "imported" and m.rows_by_id[mod].nexus_file_id == 2
    assert not workspace._dialog
    open_download(native, workspace, draw)
    native.click(native.find("Install as a new mod"), draw)
    review_install(native, workspace, draw)
    native.click(native.find("Install into library"), draw)
    if m._future is not None:
        assert m._future.exception(timeout=10)
    m.poll()
    native.warm(draw)
    assert "already has this name" in m.error
    assert len([r for r in m.rows if r.kind == "owned"]) == 1
    m.error = ""
    native.warm(draw)
    native.click(native.find("Back to Library"), draw)
    assert not workspace._dialog
    open_download(native, workspace, draw)
    native.click(native.find("##download_destination"), draw)
    native.click(native.find("Review reinstall"), draw)
    review_install(native, workspace, draw, replace=True)
    native.click(native.find("Reinstall with backup"), draw)
    finish_job(workspace, native, draw)
    assert m.selected_id == mod and len(m.library.history()) == 1
    assert not list((m.library.root / "downloads").iterdir())
    assert source_bytes == {str(p): p.read_bytes() for p in path.parent.parent.rglob("*") if p.is_file()}


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
@pytest.mark.parametrize("replacement", [False, True])
def test_native_download_install_rollback_and_retry(native, workspace, tmp_path, monkeypatch, size, outcome, replacement):
    native.io.display_size = size
    path = imported_library(workspace, tmp_path)
    m, draw = workspace.manager, panels(workspace)
    mod = install_owned(m.library, m.profile_id, tmp_path) if replacement else ""
    if replacement:
        path = owned_download(m.library)
        m.refresh()
    original_rows = m.library.rows(m.profile_id)
    original_content = {str(p): p.read_bytes() for p in (m.library.root / "mods").rglob("*") if p.is_file()}
    original_archive = path.read_bytes()
    native.warm(draw)
    open_download(native, workspace, draw, owned=replacement)
    if replacement:
        native.click(native.find("##download_destination"), draw)
    native.click(native.find("Review reinstall" if replacement else "Install as a new mod"), draw)
    review_install(native, workspace, draw, replace=replacement)
    def stop(stage):
        if stage == "placed":
            if outcome == "cancelled":
                m.cancel()
            else:
                raise OSError("generated native install failure")
    monkeypatch.setattr(m.library.owned, "_checkpoint", stop)
    native.click(native.find("Reinstall with backup" if replacement else "Install into library"), draw)
    assert m._future.exception(timeout=10)
    m.poll()
    native.warm(draw)
    assert m.job_state == outcome and m.prepared
    assert m.library.rows(m.profile_id) == original_rows
    assert {str(p): p.read_bytes() for p in (m.library.root / "mods").rglob("*") if p.is_file()} == original_content
    assert not m.library.history() and not list((m.library.root / ".staging").iterdir())
    assert path.read_bytes() == original_archive
    monkeypatch.setattr(m.library.owned, "_checkpoint", lambda stage: None)
    m.error = ""
    native.warm(draw)
    native.click(native.find("Reinstall with backup" if replacement else "Install into library"), draw)
    finish_job(workspace, native, draw)
    assert m.rows_by_id[m.selected_id].nexus_file_id == 2
    assert not replacement or m.selected_id == mod


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_native_completed_replacement_ambiguous_selection_preserves_all_profile_groups(native, workspace, tmp_path, size):
    native.io.display_size = size
    imported_library(workspace, tmp_path)
    m, draw = workspace.manager, panels(workspace)
    first = install_owned(m.library, m.profile_id, tmp_path, "Owned A", 1)
    second = install_owned(m.library, m.profile_id, tmp_path, "Owned B", 3)
    ref = next(row for row in m.rows if row.kind == "external")
    with m.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1 WHERE id=?", (ref.id,))
    clone = m.library.create_profile("Clone", m.profile_id)
    m.library.customize(clone, second, enabled=False, group="Keep/Sub")
    path = owned_download(m.library)
    m.refresh()
    m.nexus_states[("fallout4", 123)] = State("fallout4", 123, (File(2, "replacement.zip", "2", "Main"),), ((1, 2), (3, 2)))
    native.warm(draw)
    open_download(native, workspace, draw, owned=True)
    assert not workspace.download_installer.selected
    candidates = [item for item in native.items if item.label == "##download_destination"]
    assert len(candidates) == 3
    frozen = m.download_review
    blocked_index = next(i for i, c in enumerate(frozen.candidates) if c.id == ref.id)
    native.click(candidates[blocked_index], draw)
    native.click(native.find("Review reinstall"), draw)
    assert not m.prepared
    wanted_index = next(i for i, c in enumerate(frozen.candidates) if c.id == second)
    native.click([item for item in native.items if item.label == "##download_destination"][wanted_index], draw)
    native.click(native.find("Review reinstall"), draw)
    review_install(native, workspace, draw, replace=True)
    native.click(native.find("Reinstall with backup"), draw)
    finish_job(workspace, native, draw)
    assert m.selected_id == second and m.rows_by_id[first].nexus_file_id == 1
    preserved = next(r for r in m.library.rows(clone) if r.id == second)
    assert not preserved.enabled and preserved.group == "Keep/Sub"
    assert path.read_bytes() and Path(ref.source).is_dir()


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_native_combined_status_filters_sort_unknown_and_disconnect(native, workspace, size):
    native.io.display_size = size
    m, draw = workspace.manager, panels(workspace)
    m.rows, m.file_index, m.nexus_states = synthetic()
    # The real library scan must not replace this test's synthetic index mid-click.
    m._conflict_due = float("inf")
    m.rows_by_id = {row.id: row for row in m.rows}
    m.updates = {("fallout4", 124): "different"}
    workspace._expanded = {"group:Root"}
    native.warm(draw)
    native.click(native.find("Filter options"), draw)
    native.choose_next("##status_conflicts", draw, count=3)  # loses
    native.choose_next("##status_updates", draw)  # published
    native.choose_next("##status_endorsement", draw)  # endorsed
    native.choose_next("##status_tracking", draw)  # tracked
    assert set(visible_rows(native, m, workspace)) == {"a"}
    assert [n.key for n in workspace._visible_tree] == ["group:Root", "group:Root/Sub", "a"]
    assert workspace.collect_settings()["status_filters"] == {"conflicts": "loses", "updates": "published", "endorsement": "endorsed", "tracking": "tracked"}
    priority = [(row.id, row.priority) for row in m.rows]
    m.disconnect_nexus()
    native.warm(draw)
    assert not visible_rows(native, m, workspace)
    native.click(native.find("Clear filters"), draw)
    assert all(value == "all" for value in workspace._status_filters.values())
    assert [(row.id, row.priority) for row in m.rows] == priority
    native.choose_next("##display_sort", draw, count=11)  # updates
    assert workspace._sort_by == "updates"
    native.choose_next("##status_updates", draw, count=2)  # different
    assert set(visible_rows(native, m, workspace)) == {"b"}
    m.file_index = None
    native.choose_next("##status_conflicts", draw, count=9)  # unknown
    assert set(visible_rows(native, m, workspace)) == {"b"}
    assert [(row.id, row.priority) for row in m.rows] == priority


@pytest.mark.parametrize("size", [(1280, 800), (1600, 900)])
def test_native_target_source_override_blocks_reinstall_and_clears_published_filter(native, workspace, targets, tmp_path, size):
    from creation_lib.mod_manager.launch_targets import TargetLibrary
    from creation_lib.mod_manager import target_mod_sources as sources
    native.io.display_size = size
    library, profile, _, ids = targets
    m, draw = workspace.manager, panels(workspace)
    mod = install_owned(library, profile, tmp_path)
    archive_path = owned_download(library)
    variant = tmp_path / "native target variant"
    variant.mkdir()
    (variant / "file.txt").write_bytes(b"external variant stays intact")
    sources.apply(library, sources.review(library, ids[0], profile, mod, source=str(variant), enabled=False), accept_identity=True)
    m.library, m.profile_id, m.target_id = library, profile, ids[0]
    m._context_view = TargetLibrary(library, ids[0])
    m.refresh()
    m.nexus_states[("fallout4", 123)] = State("fallout4", 123, (File(2, "replacement.zip", "2", "Main"),), ((1, 2),), "Endorsed", True)
    workspace._show_filters, workspace._right_pane = True, "Downloads"
    native.warm(draw)
    native.click(native.find("Filter options"), draw)
    native.choose_next("##status_updates", draw)  # published replacement
    assert not visible_rows(native, m, workspace)
    open_download(native, workspace, draw, owned=True)
    assert not next(c for c in m.download_review.candidates if c.id == mod).eligible
    native.click(native.find("##download_destination"), draw)
    native.click(native.find("Review reinstall"), draw)
    assert m.prepared is None and not m.busy
    native.click(native.find("Cancel download install"), draw)
    m.target_id, m._context_view = ids[1], TargetLibrary(library, ids[1])
    m.refresh()
    native.warm(draw)
    assert set(visible_rows(native, m, workspace)) == {mod}
    open_download(native, workspace, draw, owned=True)
    assert next(c for c in m.download_review.candidates if c.id == mod).eligible
    native.click(native.find("##download_destination"), draw)
    native.click(native.find("Review reinstall"), draw)
    review_install(native, workspace, draw, replace=True)
    native.click(native.find("Reinstall with backup"), draw)
    finish_job(workspace, native, draw)
    assert library.rows(profile)[0].nexus_file_id == 2
    overridden = TargetLibrary(library, ids[0]).profile_snapshot(profile)[1][0]
    assert overridden.source == str(variant) and overridden.nexus_file_id == 0 and not overridden.enabled
    assert (variant / "file.txt").read_bytes() == b"external variant stays intact"
    assert archive_path.is_file()
