from __future__ import annotations

from types import SimpleNamespace

from creation_lib.esp.editor import ConflictReport, ConflictStatus
from creation_lib.esp.editor import session as session_module
from creation_lib.esp.editor.conflicts import OverrideEntry
from ui.esp_editor import app as app_module
from ui.esp_editor.app import EspEditorApp


def _loaded_plugin(handle: int, name: str, load_order_index: int) -> session_module.LoadedPlugin:
    return session_module.LoadedPlugin(
        handle=handle,
        path=f"/fake/{name}",
        game="fo4",
        is_master=False,
        load_order_index=load_order_index,
        plugin_name=name,
    )


def _entry(handle: int, name: str, load_order_index: int, form_id: int) -> OverrideEntry:
    return OverrideEntry(
        plugin_handle=handle,
        plugin_name=name,
        load_order_index=load_order_index,
        form_id=form_id,
        payload_hash=load_order_index + 100,
    )


def test_referenced_by_label_includes_editor_id(monkeypatch) -> None:
    app = EspEditorApp()

    def fake_plugin_handle_call(handle, method, form_id):
        assert (handle, method, form_id) == (7, "get_record_by_form_id", 0x01001234)
        return SimpleNamespace(editor_id="B21_SourceRecord")

    monkeypatch.setattr(app_module, "plugin_handle_call", fake_plugin_handle_call)

    try:
        app.session._plugins = [_loaded_plugin(7, "Patch.esp", 0)]

        assert app._referenced_by_label(7, 0x01001234) == (
            "Patch.esp  0x01001234  B21_SourceRecord"
        )
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_conflict_row_color_uses_active_plugin_role() -> None:
    app = EspEditorApp()
    report = ConflictReport(
        form_id=0x02001234,
        signature="WEAP",
        editor_id="B21_TestWeapon",
        chain=[
            _entry(1, "Base.esp", 0, 0x01001234),
            _entry(2, "Patch.esp", 1, 0x02001234),
        ],
        status=ConflictStatus.CONFLICT,
        mergeable=False,
    )

    try:
        app.session._plugins = [
            _loaded_plugin(1, "Base.esp", 0),
            _loaded_plugin(2, "Patch.esp", 1),
        ]

        app.session.set_active(2)
        assert app._conflict_row_color(report) == app_module._ROLE_COLORS["winner"]

        app.session.set_active(1)
        assert app._conflict_row_color(report) == app_module._ROLE_COLORS["loser"]
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_conflict_row_color_uses_yellow_for_identical_override() -> None:
    app = EspEditorApp()
    report = ConflictReport(
        form_id=0x02001234,
        signature="WEAP",
        editor_id="B21_TestWeapon",
        chain=[
            _entry(1, "Base.esp", 0, 0x01001234),
            _entry(2, "Patch.esp", 1, 0x02001234),
        ],
        status=ConflictStatus.OVERRIDE,
        mergeable=False,
    )

    try:
        app.session._plugins = [_loaded_plugin(2, "Patch.esp", 1)]
        app.session.set_active(2)

        assert app._conflict_row_color(report) == app_module._OVERRIDE_ROW_COLOR
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)
