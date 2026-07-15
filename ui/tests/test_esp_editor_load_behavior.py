from __future__ import annotations

from concurrent.futures import Future

from creation_lib.esp.editor import ConflictScan
from creation_lib.esp.editor import session as session_module
from creation_lib.esp.editor.session import EditorSession
from creation_lib.esp.model import Group
from ui.esp_editor import app as app_module
from ui.esp_editor.app import EspEditorApp


def test_esp_editor_defers_auto_conflict_scan_outside_plugin_load() -> None:
    app = EspEditorApp()

    try:
        assert app.session.auto_scan_conflicts is False
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_esp_editor_auto_scans_conflicts_after_load() -> None:
    app = EspEditorApp()
    scan = ConflictScan()
    calls = 0

    def fake_scan() -> ConflictScan:
        nonlocal calls
        calls += 1
        return scan

    try:
        app.session._plugins = [
            session_module.LoadedPlugin(
                handle=1,
                path="/fake/Test.esp",
                game="fo4",
                is_master=False,
                load_order_index=0,
                plugin_name="Test.esp",
            )
        ]
        app.session.run_conflict_scan = fake_scan  # type: ignore[method-assign]

        app._start_auto_conflict_scan()
        app.poll()

        assert calls == 1
        assert app._conflict_scan is scan
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_esp_editor_queues_auto_rescan_when_plugin_list_changes_during_scan() -> None:
    app = EspEditorApp()
    first_scan = ConflictScan()
    second_scan = ConflictScan()
    running_scan: Future = Future()
    calls = 0

    def fake_scan() -> ConflictScan:
        nonlocal calls
        calls += 1
        return second_scan

    try:
        app.session._plugins = [
            session_module.LoadedPlugin(
                handle=1,
                path="/fake/Test.esp",
                game="fo4",
                is_master=False,
                load_order_index=0,
                plugin_name="Test.esp",
            )
        ]
        app.session.run_conflict_scan = fake_scan  # type: ignore[method-assign]
        app._auto_conflict_future = running_scan

        app._start_auto_conflict_scan()
        running_scan.set_result(first_scan)
        app.poll()
        assert app._auto_conflict_future is not None
        app._auto_conflict_future.result(timeout=5)
        app.poll()

        assert calls == 1
        assert app._conflict_scan is second_scan
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_esp_editor_lazy_nav_draws_nested_groups_recursively(monkeypatch) -> None:
    app = EspEditorApp()
    nested_group = Group(b"CELL", 0)
    drawn_items = []

    monkeypatch.setattr(app_module, "_plugin_handle_group_signatures", lambda _handle: [])
    monkeypatch.setattr(app_module, "_plugin_handle_group_record_summaries", lambda _handle, _label: [])
    monkeypatch.setattr(app_module.imgui, "tree_node", lambda _label: True)
    monkeypatch.setattr(app_module.imgui, "tree_pop", lambda: None)
    app._record_label = lambda signature, game=None: signature
    app._draw_item = lambda handle, item: drawn_items.append((handle, item))

    try:
        app._cached_groups[1] = [("CELL", 1)]
        app._cached_group_records[(1, "CELL")] = [nested_group]

        app._draw_plugin_children(1)

        assert drawn_items == [(1, nested_group)]
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_editor_session_resolves_masters_from_explicit_search_paths(tmp_path) -> None:
    masters_dir = tmp_path / "masters"
    masters_dir.mkdir()
    master = masters_dir / "Fallout4.esm"
    master.write_bytes(b"TES4")
    session = EditorSession(
        default_game="fo4",
        master_search_paths=[masters_dir],
        auto_scan_conflicts=False,
    )

    assert session._resolve_master_path("Fallout4.esm", "fo4") == master
