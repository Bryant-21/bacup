from __future__ import annotations

from creation_lib.esp.editor import Issue, IssueCategory, Severity, ValidationReport
from creation_lib.esp.editor import session as session_module
from ui.esp_editor.app import EspEditorApp


def _loaded_plugin(handle: int, name: str = "Test.esp") -> session_module.LoadedPlugin:
    return session_module.LoadedPlugin(
        handle=handle,
        path=f"/fake/{name}",
        game="fo4",
        is_master=False,
        load_order_index=0,
        plugin_name=name,
    )


def test_show_error_records_visible_message() -> None:
    app = EspEditorApp()

    try:
        app._show_error("load failed")

        assert app._latest_error == "load failed"
        assert app._messages[-1] == "load failed"
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)


def test_run_validation_stores_report_for_active_plugin(monkeypatch) -> None:
    app = EspEditorApp()
    report = ValidationReport(
        issues=[
            Issue(
                severity=Severity.ERROR,
                category=IssueCategory.MISSING_MASTER,
                plugin_handle=7,
                plugin_name="Patch.esp",
                message="Master 'Fallout4.esm' is not loaded",
            )
        ]
    )
    calls: list[int | None] = []

    def fake_validate(session, *, handle=None):
        calls.append(handle)
        return report

    monkeypatch.setattr("ui.esp_editor.app.validate", fake_validate)

    try:
        app.session._plugins = [_loaded_plugin(7, "Patch.esp")]
        app.session.set_active(7)

        app.run_validation()
        assert app._busy_future is not None
        app._busy_future.result(timeout=5)
        app.poll()

        assert calls == [7]
        assert app._validation_report is report
        assert app._validation_target_handle == 7
        assert "1 issue(s)" in app._validation_summary
        assert app._messages[-1] == "Check for errors: 1 issue(s) in Patch.esp"
    finally:
        app._executor.shutdown(wait=False, cancel_futures=True)
