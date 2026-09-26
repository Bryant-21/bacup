from types import SimpleNamespace

from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace
import bacup_ui.appalachia.appalachia_workspace as mod


def test_rerun_setup_clears_extracted_dir_and_restarts(monkeypatch):
    calls = []
    requested = []

    class FakeSettings:
        def get_workspace_settings(self, _workspace_id):
            return {}

        def get_game_paths(self, _game_id):
            return {}

        def set_game_extracted_dir(self, game_id, path):
            calls.append(("set_game_extracted_dir", game_id, path))

        def save(self):
            calls.append(("save",))

    popen_calls = []
    monkeypatch.setattr(
        mod.subprocess, "Popen", lambda *a, **kw: popen_calls.append((a, kw))
    )
    monkeypatch.setattr(
        "bacup_ui.setup.request_project_setup",
        lambda settings, project_id: requested.append((settings, project_id)),
    )
    monkeypatch.setattr(
        "bacup_ui.setup.clear_project_owned_extractions",
        lambda settings, project_id: calls.append(("clear_owned", project_id)),
    )
    fake_runner_params = SimpleNamespace(app_shall_exit=False)
    monkeypatch.setattr(mod.hello_imgui, "get_runner_params", lambda: fake_runner_params)

    ws = AppalachiaWorkspace(toolkit_settings=FakeSettings())
    ws._rerun_setup()

    assert ("clear_owned", "appalachia") in calls
    assert ("set_game_extracted_dir", "fo76", "") in calls
    assert calls[-1] == ("save",)
    assert requested == [(ws._toolkit_settings, "appalachia")]
    assert len(popen_calls) == 1
    assert fake_runner_params.app_shall_exit is True
