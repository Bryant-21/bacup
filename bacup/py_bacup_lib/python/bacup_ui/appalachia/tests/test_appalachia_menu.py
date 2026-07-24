from types import SimpleNamespace
from unittest.mock import MagicMock

from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace
from bacup_ui.appalachia.window_title import appalachia_window_title
import bacup_ui.appalachia.appalachia_workspace as mod
from bacup_lib.upgrade_manifest import UpgradeManifest, UpgradeVersion


def test_workspace_name_includes_current_manifest_version():
    assert AppalachiaWorkspace.name == appalachia_window_title()


def test_changelog_entries_newest_first_with_current_flagged():
    ws = AppalachiaWorkspace()

    entries = ws._changelog_entries()

    assert [version_id for version_id, _is_current, _notes in entries] == [
        "alpha2.1",
        "alpha2",
        "alpha1",
    ]
    assert entries[0][1] is True
    assert entries[1][1] is False
    assert entries[2][1] is False
    assert all(notes for _version_id, _is_current, notes in entries)


def test_changelog_entries_empty_on_manifest_load_failure(monkeypatch):
    import bacup_lib.upgrade_manifest as upgrade_manifest

    def _raise(_path):
        raise RuntimeError("broken manifest")

    monkeypatch.setattr(upgrade_manifest, "load_upgrade_manifest", _raise)

    ws = AppalachiaWorkspace()

    assert ws._changelog_entries() == []


def test_changelog_omits_versions_without_notes_for_active_conversion(monkeypatch):
    import bacup_lib.upgrade_manifest as upgrade_manifest

    manifest = UpgradeManifest(
        current="alpha4",
        versions=(
            UpgradeVersion(
                "alpha3",
                families_by_conversion=(("fo76:fo4", ("Textures",)),),
                notes_by_conversion=(("fo76:fo4", ("Appalachia change",)),),
            ),
            UpgradeVersion(
                "alpha4",
                families_by_conversion=(("skyrimse:fo4", ("Meshes",)),),
                notes_by_conversion=(("skyrimse:fo4", ("Skyrim change",)),),
            ),
        ),
    )
    monkeypatch.setattr(
        upgrade_manifest, "load_upgrade_manifest", lambda _path: manifest
    )
    ws = AppalachiaWorkspace()
    ws._active_project_id = "north"

    assert ws._changelog_entries() == [
        ("alpha4", True, ("Skyrim change",))
    ]


def test_rerun_setup_clears_extracted_dir_and_restarts(monkeypatch):
    calls = []
    requested = []

    class FakeSettings:
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
    fake_runner_params = SimpleNamespace(app_shall_exit=False)
    monkeypatch.setattr(mod.hello_imgui, "get_runner_params", lambda: fake_runner_params)

    ws = AppalachiaWorkspace(toolkit_settings=FakeSettings())
    ws._rerun_setup()

    assert ("set_game_extracted_dir", "fo76", "") in calls
    assert calls[-1] == ("save",)
    assert requested == [(ws._toolkit_settings, "appalachia")]
    assert len(popen_calls) == 1
    assert fake_runner_params.app_shall_exit is True


def test_draw_menu_smoke(monkeypatch):
    monkeypatch.setattr(mod.imgui, "begin_menu", lambda _label: True)
    monkeypatch.setattr(mod.imgui, "end_menu", lambda: None)
    monkeypatch.setattr(mod.imgui, "menu_item", lambda *a, **kw: (False, False))

    ws = AppalachiaWorkspace()

    ws.draw_menu()

    assert ws._changelog_pending is False
    assert ws._setup_confirm_pending is False


def test_draw_popups_smoke(monkeypatch):
    fake_imgui = MagicMock()
    fake_imgui.begin_popup_modal.return_value = (True, True)
    fake_imgui.button.return_value = False
    monkeypatch.setattr(mod, "imgui", fake_imgui)

    ws = AppalachiaWorkspace()
    ws._changelog_pending = True
    ws._setup_confirm_pending = True

    ws._draw_changelog_popup()
    ws._draw_setup_confirm_popup()

    assert ws._changelog_pending is False
    assert ws._setup_confirm_pending is False
