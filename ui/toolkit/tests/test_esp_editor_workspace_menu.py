from __future__ import annotations

from ui.esp_editor.workspace import EspEditorWorkspace


def test_draw_menu_exposes_esp_editor_panels_to_view_helper(monkeypatch):
    ws = EspEditorWorkspace()
    calls: list[list[str]] = []

    class FakeViewHelper:
        def draw(self, labels):
            calls.append(labels)

    class FakeApp:
        def open_plugin(self):
            pass

        def open_folder(self):
            pass

        def import_load_order(self):
            pass

        def open_save_popup(self):
            pass

        def save_active(self, *, save_as=False):
            pass

        def close_active(self):
            pass

        def undo(self):
            pass

        def redo(self):
            pass

        def run_conflict_scan(self):
            pass

        def run_build_ref_info(self):
            pass

        def run_build_reachable(self):
            pass

        def open_new_patch_popup(self):
            pass

    ws._app = FakeApp()
    ws._view_helper = FakeViewHelper()
    monkeypatch.setattr("ui.esp_editor.workspace.imgui.begin_menu", lambda *args, **kwargs: False)

    ws.draw_menu()

    assert calls == [["Plugins##esp_editor", "Record##esp_editor", "Info##esp_editor"]]


def test_draw_menu_tools_check_active_plugin_for_errors_runs_validation(monkeypatch):
    ws = EspEditorWorkspace()
    calls: list[str] = []

    class FakeSession:
        active = object()
        _patch_handle = None

    class FakeSelection:
        record = None

    class FakeApp:
        session = FakeSession()
        selection = FakeSelection()

        def run_validation(self):
            calls.append("run_validation")

        def run_conflict_scan(self):
            pass

        def run_build_ref_info(self):
            pass

        def run_build_reachable(self):
            pass

    def fake_begin_menu(label):
        return label == "Tools"

    def fake_menu_item(label, shortcut="", selected=False, enabled=True):
        return (label == "Check Active Plugin for Errors", False)

    ws._app = FakeApp()
    ws._view_helper = None
    monkeypatch.setattr("ui.esp_editor.workspace.imgui.begin_menu", fake_begin_menu)
    monkeypatch.setattr("ui.esp_editor.workspace.imgui.menu_item", fake_menu_item)
    monkeypatch.setattr("ui.esp_editor.workspace.imgui.separator", lambda: None)
    monkeypatch.setattr("ui.esp_editor.workspace.imgui.end_menu", lambda: None)

    ws.draw_menu()

    assert calls == ["run_validation"]
