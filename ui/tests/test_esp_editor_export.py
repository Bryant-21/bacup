from __future__ import annotations

import csv
import json
from types import SimpleNamespace

from creation_lib.esp.editor import ConflictReport, ConflictScan, ConflictStatus, Issue, IssueCategory, Severity, ValidationReport
from creation_lib.esp.editor.conflicts import OverrideEntry
from ui.esp_editor import app as app_module
from ui.esp_editor.app import EspEditorApp


def _loaded_plugin(handle: int, name: str, load_order_index: int = 0):
    return SimpleNamespace(
        handle=handle,
        plugin_name=name,
        path=f"/fake/{name}",
        game="fo4",
        load_order_index=load_order_index,
    )


def _wait_for_background(app: EspEditorApp) -> None:
    assert app._busy_future is not None
    app._busy_future.result(timeout=5)
    app.poll()


def test_export_plugin_text_routes_to_native_plugin_export(monkeypatch, tmp_path) -> None:
    app = EspEditorApp()
    out_path = tmp_path / "plugin.yaml"
    calls: list[tuple[object, ...]] = []

    def fake_pick_save_file(*, title: str, filetypes, default_ext: str, initialfile: str | None = None):
        assert title == "Export Plugin as YAML"
        assert default_ext == ".yaml"
        assert initialfile == "Test.esp.yaml"
        return str(out_path)

    def fake_plugin_handle_call(handle, method, *args):
        calls.append((handle, method, *args))
        assert (handle, method, args) == (7, "export_plugin_text", ("lossless", "yaml"))
        return "plugin-export"

    monkeypatch.setattr(app_module, "pick_save_file", fake_pick_save_file)
    monkeypatch.setattr(app_module, "plugin_handle_call", fake_plugin_handle_call)

    try:
        plugin = _loaded_plugin(7, "Test.esp")
        app._export_plugin_text(plugin, "yaml")
        _wait_for_background(app)

        assert out_path.read_text(encoding="utf-8") == "plugin-export"
        assert calls == [(7, "export_plugin_text", "lossless", "yaml")]
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_export_record_text_routes_to_native_record_export(monkeypatch, tmp_path) -> None:
    app = EspEditorApp()
    out_path = tmp_path / "record.json"
    calls: list[tuple[object, ...]] = []
    item = SimpleNamespace(form_id=0x01001234, signature="MISC", editor_id="B21_Record")

    def fake_pick_save_file(*, title: str, filetypes, default_ext: str, initialfile: str | None = None):
        assert title == "Export Record as JSON"
        assert default_ext == ".json"
        assert initialfile == "B21_Record.json"
        return str(out_path)

    def fake_plugin_handle_call(handle, method, *args):
        calls.append((handle, method, *args))
        assert (handle, method, args) == (7, "export_record_text", (0x01001234, "json"))
        return "record-export"

    monkeypatch.setattr(app_module, "pick_save_file", fake_pick_save_file)
    monkeypatch.setattr(app_module, "plugin_handle_call", fake_plugin_handle_call)

    try:
        app._export_record_text(7, item, "json")
        _wait_for_background(app)

        assert out_path.read_text(encoding="utf-8") == "record-export"
        assert calls == [(7, "export_record_text", 0x01001234, "json")]
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_export_validation_report_writes_visible_rows(monkeypatch, tmp_path) -> None:
    app = EspEditorApp()
    app.session._plugins = [_loaded_plugin(7, "Test.esp")]
    app.session.set_active(7)
    app._validation_target_handle = 7
    app._validation_report = ValidationReport(
        issues=[
            Issue(
                severity=Severity.ERROR,
                category=IssueCategory.MISSING_MASTER,
                plugin_handle=7,
                plugin_name="Test.esp",
                message="Missing master",
                form_id=0x01001234,
            ),
            Issue(
                severity=Severity.WARNING,
                category=IssueCategory.ITM,
                plugin_handle=7,
                plugin_name="Test.esp",
                message="Identical to master",
                form_id=None,
            ),
        ]
    )

    def fake_pick_save_file(*, title: str, filetypes, default_ext: str, initialfile: str | None = None):
        assert title == "Export Validation Report as JSON"
        assert default_ext == ".json"
        assert initialfile == "Test.esp.validation.json"
        return str(tmp_path / "validation.json")

    monkeypatch.setattr(app_module, "pick_save_file", fake_pick_save_file)

    try:
        app._export_validation_report("json")
        _wait_for_background(app)

        payload = json.loads((tmp_path / "validation.json").read_text(encoding="utf-8"))
        assert payload["kind"] == "validation_report"
        assert payload["plugin"] == "Test.esp"
        assert [row["message"] for row in payload["rows"]] == ["Missing master", "Identical to master"]
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_export_conflict_report_respects_current_filters(monkeypatch, tmp_path) -> None:
    app = EspEditorApp()
    app.session._plugins = [
        _loaded_plugin(1, "Base.esp", 0),
        _loaded_plugin(2, "Patch.esp", 1),
    ]
    app.session.set_active(2)
    app._conflict_filter_signature = "WEAP"
    app._conflict_only_mergeable = True
    app._conflict_scan = ConflictScan(
        by_form_id={
            0x02001234: ConflictReport(
                form_id=0x02001234,
                signature="WEAP",
                editor_id="B21_Weapon",
                chain=[
                    OverrideEntry(
                        plugin_handle=1,
                        plugin_name="Base.esp",
                        load_order_index=0,
                        form_id=0x01001234,
                        payload_hash=100,
                    ),
                    OverrideEntry(
                        plugin_handle=2,
                        plugin_name="Patch.esp",
                        load_order_index=1,
                        form_id=0x02001234,
                        payload_hash=101,
                    ),
                ],
                status=ConflictStatus.CONFLICT,
                mergeable=False,
            ),
            0x02005678: ConflictReport(
                form_id=0x02005678,
                signature="WEAP",
                editor_id="B21_Axe",
                chain=[
                    OverrideEntry(
                        plugin_handle=1,
                        plugin_name="Base.esp",
                        load_order_index=0,
                        form_id=0x01005678,
                        payload_hash=200,
                    ),
                    OverrideEntry(
                        plugin_handle=2,
                        plugin_name="Patch.esp",
                        load_order_index=1,
                        form_id=0x02005678,
                        payload_hash=201,
                    ),
                ],
                status=ConflictStatus.CONFLICT,
                mergeable=True,
            ),
        },
        by_signature={"WEAP": [0x02001234, 0x02005678]},
        by_handle_form_id={},
    )
    app._conflict_scan.by_handle_form_id = {
        (1, 0x01001234): app._conflict_scan.by_form_id[0x02001234],
        (2, 0x02001234): app._conflict_scan.by_form_id[0x02001234],
        (1, 0x01005678): app._conflict_scan.by_form_id[0x02005678],
        (2, 0x02005678): app._conflict_scan.by_form_id[0x02005678],
    }

    def fake_pick_save_file(*, title: str, filetypes, default_ext: str, initialfile: str | None = None):
        assert title == "Export Conflict Report as CSV"
        assert default_ext == ".csv"
        assert initialfile == "Patch.esp.conflicts.csv"
        return str(tmp_path / "conflicts.csv")

    monkeypatch.setattr(app_module, "pick_save_file", fake_pick_save_file)

    try:
        app._export_conflict_report("csv")
        _wait_for_background(app)

        with (tmp_path / "conflicts.csv").open(encoding="utf-8", newline="") as f:
            rows = list(csv.DictReader(f))
        assert len(rows) == 1
        assert rows[0]["editor_id"] == "B21_Axe"
        assert rows[0]["mergeable"] == "true"
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)
