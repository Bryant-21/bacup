import threading
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_ui import storage_cleanup
from bacup_ui.conversion.panels import regen_panel as regen_panel_module
from bacup_ui.conversion.panels.regen_panel import RegenPanel
from bacup_ui.storage_cleanup import (
    CleanupResult,
    CleanupTarget,
    delete_cleanup_targets,
    discover_cleanup_targets,
    measure_cleanup_targets,
)


def test_discovers_only_safe_known_cleanup_targets(tmp_path):
    fo4_root = tmp_path / "Fallout 4"
    fo4_root.mkdir()
    fo4_extracted = tmp_path / "extracted" / "fo4"
    fo4_extracted.mkdir(parents=True)
    fo76_extracted = tmp_path / "extracted" / "fo76"
    geoexporter = fo76_extracted / "GeoExporter"
    vis = fo76_extracted / "VIS"
    geoexporter.mkdir(parents=True)
    vis.mkdir()
    temp_root = tmp_path / "Temp"
    expected_temp = temp_root / "hkxunpack_old"
    ignored_temp = temp_root / "unrelated-app"
    expected_temp.mkdir(parents=True)
    ignored_temp.mkdir()
    (temp_root / "bacup-current.log").write_text("keep", encoding="utf-8")
    legacy_local_data = tmp_path / "LocalAppData" / "modkit21" / "conversion"
    legacy_local_data.mkdir(parents=True)

    targets = discover_cleanup_targets(
        fo4_extracted_dir=fo4_extracted,
        fo76_extracted_dir=fo76_extracted,
        forbidden_roots=(fo4_root,),
        temp_root=temp_root,
        legacy_local_data_root=legacy_local_data,
    )

    by_key = {target.key: target for target in targets}
    assert set(by_key) == {
        "fo4_extracted",
        "fo76_geoexporter",
        "fo76_vis",
        "bacup_temp",
        "legacy_local_data",
    }
    assert by_key["fo76_geoexporter"].paths == (geoexporter,)
    assert by_key["fo76_vis"].paths == (vis,)
    assert by_key["bacup_temp"].paths == (expected_temp,)
    assert by_key["legacy_local_data"].paths == (legacy_local_data,)


def test_never_offers_game_root_as_extracted_cleanup_target(tmp_path):
    game_root = tmp_path / "Fallout 4"
    game_root.mkdir()

    targets = discover_cleanup_targets(
        fo4_extracted_dir=game_root,
        fo76_extracted_dir=None,
        forbidden_roots=(game_root,),
        temp_root=tmp_path / "Temp",
    )

    assert all(target.key != "fo4_extracted" for target in targets)


def test_measure_and_delete_touch_only_selected_target(tmp_path):
    selected_dir = tmp_path / "selected"
    kept_dir = tmp_path / "kept"
    selected_dir.mkdir()
    kept_dir.mkdir()
    (selected_dir / "large.bin").write_bytes(b"12345")
    (kept_dir / "keep.bin").write_bytes(b"keep")
    targets = measure_cleanup_targets(
        (
            CleanupTarget("selected", "Selected", "", (selected_dir,)),
            CleanupTarget("kept", "Kept", "", (kept_dir,)),
        )
    )

    result = delete_cleanup_targets((targets[0],))

    assert result.deleted_keys == ("selected",)
    assert result.freed_bytes == 5
    assert not selected_dir.exists()
    assert kept_dir.is_dir()


def test_delete_uses_bacup_native_removal(monkeypatch, tmp_path):
    selected_dir = tmp_path / "selected"
    selected_dir.mkdir()
    (selected_dir / "file.bin").write_bytes(b"data")
    calls = []

    def remove_tree(path):
        calls.append(path)
        for child in selected_dir.iterdir():
            child.unlink()
        selected_dir.rmdir()

    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: SimpleNamespace(conversion_remove_path=remove_tree),
    )
    target = CleanupTarget("selected", "Selected", "", (selected_dir,), 4)

    result = delete_cleanup_targets((target,))

    assert calls == [str(selected_dir.absolute())]
    assert result.deleted_keys == ("selected",)
    assert not selected_dir.exists()


def test_elevated_frozen_launch_preserves_arguments(monkeypatch, tmp_path):
    executable = tmp_path / "B.A.C.U.P.exe"
    monkeypatch.setattr(storage_cleanup.sys, "frozen", True, raising=False)
    monkeypatch.setattr(storage_cleanup.sys, "executable", str(executable))
    monkeypatch.setattr(
        storage_cleanup.sys,
        "argv",
        [str(executable), "--profile", "Fallout 4"],
    )

    command, parameters, working_dir = storage_cleanup.elevated_launch_command()

    assert command == str(executable)
    assert parameters == '--profile "Fallout 4"'
    assert working_dir == str(executable.parent)


def test_panel_clears_deleted_fo4_extracted_setting():
    cleared = []
    panel = RegenPanel.__new__(RegenPanel)
    panel._workspace = SimpleNamespace(
        _toolkit_settings=SimpleNamespace(
            set_game_extracted_dir=lambda game, path: cleared.append((game, path))
        )
    )
    panel._disk_usage_lock = threading.Lock()
    panel._disk_usage_running = True
    panel._disk_usage_cache = (0.0, {})
    panel._disk_usage_cache_key = ("old", "old")
    panel._disk_space_cache = (("old", "old"), ())
    panel._ensure_cleanup_state()
    panel._cleanup_targets = (
        CleanupTarget("fo4_extracted", "Fallout 4", "", (Path("C:/old/fo4"),)),
    )
    panel._cleanup_selected = {"fo4_extracted"}
    panel._cleanup_pending_result = CleanupResult(
        deleted_keys=("fo4_extracted",),
        removed_paths=(Path("C:/old/fo4"),),
        freed_bytes=10,
        failures=(),
    )

    panel._poll_cleanup_result()

    assert cleared == [("fo4", "")]
    assert panel._cleanup_targets == ()
    assert panel._cleanup_disk_refresh_pending is True


def test_panel_restarts_disk_check_after_cleanup_without_stale_arguments():
    panel = RegenPanel.__new__(RegenPanel)
    panel._disk_usage_lock = threading.Lock()
    panel._disk_usage_running = False
    panel._disk_usage_cache = (0.0, {})
    panel._disk_usage_cache_key = ("old", "old")
    panel._disk_space_cache = (("old", "old"), ())
    panel._ensure_cleanup_state()
    panel._cleanup_disk_refresh_pending = True
    calls = []
    panel._start_disk_usage_worker = lambda **kwargs: calls.append(kwargs)

    panel._poll_cleanup_result()

    assert calls == [{}]
    assert panel._cleanup_disk_refresh_pending is False


def test_restart_as_admin_reports_windows_rejection(monkeypatch):
    shell32 = SimpleNamespace(ShellExecuteW=lambda *_args: 5)
    monkeypatch.setattr(
        storage_cleanup.ctypes,
        "windll",
        SimpleNamespace(shell32=shell32),
        raising=False,
    )

    with pytest.raises(OSError, match=r"failed \(5\)"):
        storage_cleanup.restart_as_admin()


def _cleanup_dialog_imgui():
    calls = SimpleNamespace(text=[], colored=[], checkboxes=[], buttons=[], table_size=None)
    flag = SimpleNamespace(value=1)

    def begin_table(_name, _columns, _flags, size):
        calls.table_size = size
        return True

    return SimpleNamespace(
        ImVec2=lambda x, y: SimpleNamespace(x=x, y=y),
        Cond_=SimpleNamespace(appearing=1),
        WindowFlags_=SimpleNamespace(no_scrollbar=flag, no_scroll_with_mouse=flag),
        TableFlags_=SimpleNamespace(
            borders=flag,
            row_bg=flag,
            scroll_y=flag,
            sizing_stretch_prop=flag,
        ),
        TableColumnFlags_=SimpleNamespace(
            width_fixed=flag,
            width_stretch=flag,
            no_resize=flag,
        ),
        set_next_window_size=lambda *_args: None,
        begin=lambda *_args, **_kwargs: True,
        end=lambda: None,
        begin_disabled=lambda: None,
        end_disabled=lambda: None,
        text_wrapped=lambda text: calls.text.append(text),
        text=lambda text: calls.text.append(text),
        text_disabled=lambda text: calls.text.append(text),
        text_colored=lambda _color, text: calls.colored.append(text),
        separator=lambda: None,
        get_content_region_avail=lambda: SimpleNamespace(y=340.0),
        begin_table=begin_table,
        end_table=lambda: None,
        table_setup_column=lambda *_args: None,
        table_headers_row=lambda: None,
        table_next_row=lambda: None,
        table_set_column_index=lambda *_args: None,
        checkbox=lambda label, checked: (
            calls.checkboxes.append(label) or False,
            checked,
        ),
        button=lambda label: calls.buttons.append(label) or False,
        same_line=lambda: None,
    ), calls


def _cleanup_dialog_panel(status, targets):
    panel = RegenPanel.__new__(RegenPanel)
    panel._workspace = SimpleNamespace(_runner=None)
    panel._cleanup_lock = threading.Lock()
    panel._cleanup_dialog_open = True
    panel._cleanup_status = status
    panel._cleanup_targets = targets
    panel._cleanup_selected = {targets[0].key} if targets else set()
    panel._cleanup_pending_result = None
    panel._cleanup_disk_refresh_pending = False
    panel._cleanup_message = None
    panel._cleanup_error = None
    panel._admin_restart_error = None
    panel._is_admin = False
    panel._runner_running = lambda: False
    panel._start_cleanup_delete = lambda: None
    return panel


def test_cleanup_dialog_uses_compact_grid_with_fixed_action_footer(monkeypatch):
    target = CleanupTarget(
        "fo76_geoexporter",
        "Fallout 76 GeoExporter data",
        "This detail should not consume dialog space.",
        (Path("C:/a/very/long/path/that/should/not/be/rendered"),),
        30 * 1024**3,
    )
    panel = _cleanup_dialog_panel("idle", (target,))
    fake_imgui, calls = _cleanup_dialog_imgui()
    monkeypatch.setattr(regen_panel_module, "imgui", fake_imgui)

    panel._draw_cleanup_dialog()

    rendered = "\n".join((*calls.text, *calls.colored))
    assert target.label in rendered
    assert target.detail not in rendered
    assert str(target.paths[0]) not in rendered
    assert "30.0 GB" in calls.colored
    assert calls.checkboxes == ["##cleanup_select_fo76_geoexporter"]
    assert calls.table_size.y <= 240.0
    assert calls.buttons[:3] == [
        "Delete selected (30.0 GB)##appalachia_cleanup_delete",
        "Close##appalachia_cleanup_close",
        "Open Windows Temp##appalachia_cleanup_temp",
    ]


def test_cleanup_dialog_masks_controls_while_calculating(monkeypatch):
    panel = _cleanup_dialog_panel("scanning", ())
    fake_imgui, _calls = _cleanup_dialog_imgui()
    overlays = []
    monkeypatch.setattr(regen_panel_module, "imgui", fake_imgui)
    monkeypatch.setattr(
        regen_panel_module,
        "draw_runner_overlay",
        lambda *args: overlays.append(args),
    )

    panel._draw_cleanup_dialog()

    assert overlays == [
        (
            "Calculating cleanup space",
            "Measuring reclaimable data...",
            None,
        )
    ]
