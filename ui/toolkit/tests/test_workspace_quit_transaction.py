"""Cross-editor quit approvals cover only the reviewed disposable state."""
from concurrent.futures import Future
from types import SimpleNamespace

import pytest

from ui.esp_editor.app import EspEditorApp
from ui.esp_editor.workspace import EspEditorWorkspace
from ui.swf_editor.document import SwfEditingDocument
from ui.swf_editor.swf_editor_app import SwfEditorApp
from ui.tests.test_esp_editor_workflow import modal_buttons, plugin_file
from ui.toolkit import app as host_module
from ui.toolkit.app import ToolkitApp
from ui.toolkit.workspaces.swf_editor_workspace import SwfEditorWorkspace


@pytest.fixture(params=["esp_editor", "swf"])
def combined_quit(request, monkeypatch, tmp_path):
    esp, swf = EspEditorApp(), SwfEditorApp()
    plugin = esp.session.load(plugin_file(tmp_path, "B21_CombinedQuit.esp"), game="fo4")
    esp._after_record_edit(plugin.handle, 0x800)
    swf._replace_document(SwfEditingDocument.menu())
    swf.document.path = tmp_path / "B21_CombinedQuit.swfproj"
    swf.document.sources["DocumentMenu.as"] += "\n// unsaved fixture"
    esp_ws, swf_ws = EspEditorWorkspace.__new__(EspEditorWorkspace), SwfEditorWorkspace.__new__(SwfEditorWorkspace)
    esp_ws._app, swf_ws._app = esp, swf
    by_id = {esp_ws.id: esp_ws, swf_ws.id: swf_ws}
    first = by_id[request.param]
    second = by_id["swf" if request.param == "esp_editor" else "esp_editor"]
    host = ToolkitApp.__new__(ToolkitApp)
    host._workspaces = [first, second]
    host._quit_pending = False
    host._quit_action = None
    params = SimpleNamespace(app_shall_exit=False)
    monkeypatch.setattr(host_module.hello_imgui, "get_runner_params", lambda: params)
    modal_buttons(monkeypatch, "")
    imgui = host_module.imgui
    monkeypatch.setattr(imgui, "begin_popup_modal", lambda *a, **kw: (True, None))
    for name in ("begin_disabled", "end_disabled", "new_line"):
        monkeypatch.setattr(imgui, name, lambda *a: None)
    monkeypatch.setattr(imgui, "is_popup_open", lambda *a: False)
    frame = [0]
    monkeypatch.setattr(imgui, "get_frame_count", lambda: frame[0])
    def tick(label=""):
        frame[0] += 1
        chosen = [False]
        def button(text, *a):
            if text == label and not chosen[0]:
                chosen[0] = True
                return True
            return False
        monkeypatch.setattr(imgui, "button", button)
        host._confirm_quit()
    def approve_first():
        host._request_quit()
        tick()
        tick("Discard all and quit" if first.id == "esp_editor" else "Discard edits")
        tick()
        assert id(first) in host._quit_approvals
        assert host._quit_waiting is second and not params.app_shall_exit
    yield SimpleNamespace(host=host, esp=esp, swf=swf, plugin=plugin, params=params,
                          first=first, second=second, tick=tick, approve_first=approve_first)
    esp.cleanup()
    swf.cleanup()


def test_discard_approval_survives_wait_repeated_close_and_cancel_preserves_edits(combined_quit):
    state = combined_quit
    original = state.swf.document.state_key()
    state.approve_first()
    approved = state.host._quit_approvals.copy()
    for _ in range(4):
        state.host._request_quit()
        state.tick()
        assert state.host._quit_approvals == approved
    assert state.esp._dirty_handles and state.swf.document.dirty
    state.tick("Cancel")
    state.tick()  # SWF consumes its cancel result on the following host pass.
    assert not state.host._quit_pending and not state.params.app_shall_exit
    assert not state.host._quit_approvals
    assert state.swf.document.state_key() == original and state.esp._dirty_handles
    assert not state.esp._exit_popup_active and not state.swf._pending_for_exit
    state.host._request_quit()
    state.tick()
    assert state.host._quit_waiting is state.first and not state.host._quit_approvals


def test_new_edits_invalidate_approval_and_reset_the_later_modal(combined_quit):
    state = combined_quit
    state.approve_first()
    if state.first.id == "esp_editor":
        state.esp._after_record_edit(state.plugin.handle, 0x800)
    else:
        state.swf.document.sources["DocumentMenu.as"] += "\n// changed after approval"
    state.tick()
    assert id(state.first) not in state.host._quit_approvals
    assert state.host._quit_waiting is state.first and not state.params.app_shall_exit
    if state.second.id == "swf":
        assert state.swf._pending_action is None and not state.swf._pending_for_exit
    else:
        assert not state.esp._exit_popup_active
    state.tick("Cancel")
    state.tick()
    assert not state.host._quit_pending and state.esp._dirty_handles and state.swf.document.dirty


def test_second_editor_save_failure_cannot_bypass_first_approval(combined_quit, monkeypatch):
    state = combined_quit
    state.approve_first()
    def failure(*a):
        raise OSError("combined fixture save denied")
    if state.second.id == "esp_editor":
        from ui.esp_editor import app as module
        monkeypatch.setattr(module, "_make_backup", failure)
        state.tick("Save all and quit")
        state.esp._busy_future.result(timeout=5)
    else:
        monkeypatch.setattr(state.swf.document, "save", failure)
        state.tick("Save project")
    state.tick()
    assert state.host._quit_pending and not state.params.app_shall_exit
    assert id(state.first) in state.host._quit_approvals
    assert state.esp._dirty_handles and state.swf.document.dirty
    state.host._request_quit()
    state.tick()
    assert state.host._quit_waiting is state.second
    state.tick("Cancel")
    state.tick()
    assert not state.host._quit_pending and not state.host._quit_approvals


def test_pending_job_invalidates_previously_approved_editor(combined_quit):
    state = combined_quit
    state.approve_first()
    future = Future()
    future.set_running_or_notify_cancel()
    if state.first.id == "esp_editor":
        state.esp._busy, state.esp._busy_future = True, future
        state.esp._busy_done_cb = lambda result, error: state.esp._after_record_edit(state.plugin.handle, 0x800)
    else:
        state.swf.jobs.future = future
        state.swf.jobs.kind = "build"
        state.swf.jobs.key = state.swf.document.state_key()
    state.tick()
    assert not state.params.app_shall_exit and id(state.first) not in state.host._quit_approvals
    if state.first.id == "swf":
        from ui.tests.test_swf_editor_workflow import compiled
        future.set_result(compiled())
    else:
        future.set_result(None)
    state.tick()
    assert not state.params.app_shall_exit and state.host._quit_waiting is state.first


def test_final_validation_catches_edits_during_later_confirmation(combined_quit, monkeypatch):
    state = combined_quit
    state.approve_first()
    def approve_and_change_other_editor():
        if state.first.id == "esp_editor":
            state.esp._after_record_edit(state.plugin.handle, 0x800)
        else:
            state.swf.document.sources["DocumentMenu.as"] += "\n// changed in final confirmation"
        return True
    monkeypatch.setattr(state.second, "confirm_exit", approve_and_change_other_editor)
    state.tick()
    assert not state.params.app_shall_exit and state.host._quit_pending
    assert id(state.first) not in state.host._quit_approvals


def test_new_swf_edit_between_discard_click_and_ready_signal_requires_review(combined_quit):
    state = combined_quit
    state.host._workspaces = [state.host._workspaces[i] for i in sorted(range(2), key=lambda i: state.host._workspaces[i].id != "swf")]
    state.host._request_quit()
    state.tick("Discard edits")
    assert state.swf._exit_ready
    state.swf.document.sources["DocumentMenu.as"] += "\n// edit before ready signal"
    state.tick()
    assert not state.params.app_shall_exit
    assert id(next(w for w in state.host._workspaces if w.id == "swf")) not in state.host._quit_approvals
    assert state.host._quit_waiting.id == "swf"


def test_exit_state_error_cancels_transaction_without_touching_edits(combined_quit, monkeypatch):
    state = combined_quit
    state.approve_first()
    def failure():
        raise RuntimeError("fixture state unavailable")
    monkeypatch.setattr(state.second, "exit_confirmation_state", failure)
    state.tick()
    assert not state.params.app_shall_exit and not state.host._quit_pending
    assert not state.host._quit_approvals and state.esp._dirty_handles and state.swf.document.dirty


def test_exit_reset_error_keeps_app_open_and_clears_approvals(combined_quit, monkeypatch):
    state = combined_quit
    def failure():
        raise RuntimeError("fixture modal reset unavailable")
    monkeypatch.setattr(state.second, "reset_exit_confirmation", failure)
    state.host._request_quit()
    state.params.app_shall_exit = True  # A direct runner request must also fail closed.
    state.tick()
    assert not state.params.app_shall_exit and not state.host._quit_pending
    assert not state.host._quit_approvals and state.esp._dirty_handles and state.swf.document.dirty
