"""Whole-app quit transactions use disposable plugins, never configured game data."""
from concurrent.futures import Future
from types import SimpleNamespace
from unittest.mock import MagicMock

import pytest

from ui.esp_editor import app as editor_module
from ui.esp_editor.app import EspEditorApp
from ui.esp_editor.workspace import EspEditorWorkspace
from ui.tests.test_esp_editor_workflow import modal_buttons, plugin_file
from ui.toolkit import app as host_module
from ui.toolkit.app import ToolkitApp


@pytest.fixture
def quit_app(monkeypatch):
    editor = EspEditorApp()
    workspace = EspEditorWorkspace.__new__(EspEditorWorkspace)
    workspace._app = editor
    workspace.active = False
    host = ToolkitApp.__new__(ToolkitApp)
    host._workspaces = [SimpleNamespace(id="other"), workspace]
    host._quit_pending = False
    host._quit_action = None
    params = SimpleNamespace(app_shall_exit=False)
    monkeypatch.setattr(host_module.hello_imgui, "get_runner_params", lambda: params)
    modal_buttons(monkeypatch, "")
    for name in ("begin_disabled", "end_disabled"):
        monkeypatch.setattr(editor_module.imgui, name, lambda *args: None)
    yield host, editor, params
    editor.cleanup()


def choose(monkeypatch, label):
    monkeypatch.setattr(editor_module.imgui, "button", lambda text, *args: text == label)


def test_inactive_editor_cancel_and_repeated_quit_keeps_all_edits(quit_app, tmp_path, monkeypatch):
    host, editor, params = quit_app
    plugins = [editor.session.load(plugin_file(tmp_path, name), game="fo4")
               for name in ("First.esp", "Second.esp")]
    editor._dirty_handles.update(p.handle for p in plugins)
    popup = MagicMock()
    monkeypatch.setattr(editor_module.imgui, "open_popup", popup)
    for _ in range(5):
        host._request_quit()
        host._confirm_quit()
        assert not params.app_shall_exit
    assert popup.call_count == 1
    choose(monkeypatch, "Cancel")
    host._confirm_quit()
    assert not host._quit_pending and not params.app_shall_exit
    assert editor._dirty_handles == {p.handle for p in plugins}
    assert editor._busy_future is None
    host._request_quit()
    choose(monkeypatch, "Discard all and quit")
    host._confirm_quit()
    assert params.app_shall_exit
    assert editor.session.plugins == plugins  # destruction is still deferred to cleanup


def test_save_all_and_quit_waits_for_all_writes(quit_app, tmp_path, monkeypatch):
    host, editor, params = quit_app
    plugins = [editor.session.load(plugin_file(tmp_path, name), game="fo4")
               for name in ("First.esp", "Second.esp")]
    editor._dirty_handles.update(p.handle for p in plugins)
    host._request_quit()
    choose(monkeypatch, "Save all and quit")
    host._confirm_quit()
    assert editor._busy and not params.app_shall_exit
    choose(monkeypatch, "")
    editor._busy_future.result(timeout=10)
    host._confirm_quit()
    assert params.app_shall_exit and not editor._dirty_handles
    assert all((tmp_path / (p.plugin_name + ".bak")).is_file() for p in plugins)


def test_partial_save_failure_blocks_quit_and_allows_retry(quit_app, tmp_path, monkeypatch):
    host, editor, params = quit_app
    plugins = [editor.session.load(plugin_file(tmp_path, name), game="fo4")
               for name in ("First.esp", "Second.esp")]
    editor._dirty_handles.update(p.handle for p in plugins)
    native_call = editor_module.plugin_handle_call
    def failing_call(handle, method, *args):
        if handle == plugins[1].handle and method == "save":
            raise OSError("fixture write denied")
        return native_call(handle, method, *args)
    monkeypatch.setattr(editor_module, "plugin_handle_call", failing_call)
    choose(monkeypatch, "Save all and quit")
    host._request_quit()
    host._confirm_quit()
    editor._busy_future.result(timeout=10)
    choose(monkeypatch, "")
    host._confirm_quit()
    assert not params.app_shall_exit and host._quit_pending
    assert editor._dirty_handles == {plugins[1].handle}
    assert "fixture write denied" in editor._latest_error
    host._request_quit()  # repeated X/quit after failure cannot bypass it
    host._confirm_quit()
    assert not params.app_shall_exit
    monkeypatch.setattr(editor_module, "plugin_handle_call", native_call)
    choose(monkeypatch, "Save all and quit")
    host._confirm_quit()
    editor._busy_future.result(timeout=10)
    choose(monkeypatch, "")
    host._confirm_quit()
    assert params.app_shall_exit and not editor._dirty_handles


def test_backup_failure_then_cancel_never_writes(quit_app, tmp_path, monkeypatch):
    host, editor, params = quit_app
    plugin = editor.session.load(plugin_file(tmp_path, "Keep.esp"), game="fo4")
    editor._dirty_handles.add(plugin.handle)
    def fail(*args):
        raise OSError("fixture backup denied")
    monkeypatch.setattr(editor_module, "_make_backup", fail)
    save = MagicMock()
    monkeypatch.setattr(editor_module, "plugin_handle_call", save)
    host._request_quit()
    choose(monkeypatch, "Save all and quit")
    host._confirm_quit()
    editor._busy_future.result(timeout=10)
    choose(monkeypatch, "Cancel")
    host._confirm_quit()
    assert not params.app_shall_exit and not host._quit_pending
    assert plugin.handle in editor._dirty_handles
    save.assert_not_called()


def test_quit_during_mutation_waits_and_cancel_does_not_drop_completion(quit_app, tmp_path, monkeypatch):
    host, editor, params = quit_app
    plugin = editor.session.load(plugin_file(tmp_path, "Pending.esp"), game="fo4")
    future = Future()
    future.set_running_or_notify_cancel()
    editor._busy, editor._busy_future = True, future
    editor._busy_done_cb = lambda result, error: editor._dirty_handles.add(plugin.handle)
    host._request_quit()
    choose(monkeypatch, "Discard all and quit")
    host._confirm_quit()
    assert not params.app_shall_exit
    choose(monkeypatch, "Cancel")
    host._confirm_quit()
    future.set_result(None)
    editor.poll()
    assert plugin.handle in editor._dirty_handles
    choose(monkeypatch, "")
    host._request_quit()
    host._confirm_quit()
    assert host._quit_pending and not params.app_shall_exit


def test_direct_runner_exit_request_and_relaunch_are_guarded(quit_app, tmp_path, monkeypatch):
    host, editor, params = quit_app
    plugin = editor.session.load(plugin_file(tmp_path, "Keep.esp"), game="fo4")
    editor._dirty_handles.add(plugin.handle)
    params.app_shall_exit = True
    host._confirm_quit()
    assert not params.app_shall_exit
    choose(monkeypatch, "Cancel")
    host._confirm_quit()
    action = MagicMock()
    host._request_quit(action=action)
    host._confirm_quit()
    action.assert_not_called()
    host._request_quit(action=action)
    choose(monkeypatch, "Discard all and quit")
    host._confirm_quit()
    assert params.app_shall_exit
    action.assert_called_once()


def test_native_close_callback_clears_glfw_flag_and_queues_confirmation(quit_app, monkeypatch):
    import imgui_bundle

    host, editor, params = quit_app
    glfw = MagicMock()
    host._install_file_drop_callback = MagicMock()
    monkeypatch.setattr(host_module.ctypes, "CDLL", lambda *args: glfw)
    monkeypatch.setattr(imgui_bundle, "__file__", "C:/fixture/imgui_bundle/__init__.py", raising=False)
    monkeypatch.setattr(host_module.hello_imgui, "get_glfw_window_address", lambda: 123)
    host._install_platform_callbacks()
    host._glfw_close_callback(123)
    host._glfw_close_callback(123)
    glfw.glfwSetWindowShouldClose.assert_called_with(123, 0)
    assert host._quit_pending and not params.app_shall_exit
    host._confirm_quit()
    assert params.app_shall_exit  # clean editor has no prompt
