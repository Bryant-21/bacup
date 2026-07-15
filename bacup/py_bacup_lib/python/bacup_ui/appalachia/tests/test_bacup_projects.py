from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_ui.appalachia.appalachia_workspace import (
    APP_EXPANSION,
    APP_NAME,
    AppalachiaWorkspace,
    _ENABLED_PROJECTS,
    _PROJECTS,
)
from bacup_ui.conversion.panels.regen_panel import RegenPanel


class _Settings:
    def __init__(self):
        self.workspaces = {"appalachia": {}}
        self.paths = {
            game: {"root_dir": f"C:/{game}", "extracted_dir": f"C:/{game}/Data"}
            for game in ("fo4", "fo76", "fnv", "fo3", "skyrimse")
        }

    def get_workspace_settings(self, workspace_id):
        return dict(self.workspaces.get(workspace_id, {}))

    def set_workspace_settings(self, workspace_id, values):
        self.workspaces.setdefault(workspace_id, {}).update(values)

    def get_game_paths(self, game_id):
        return dict(self.paths.get(game_id, {}))


def test_bacup_keeps_all_projects_but_enables_only_appalachia():
    assert APP_NAME == "B.A.C.U.P."
    assert APP_EXPANSION == "Bethesda Asset Converter Universal Platform"
    assert _PROJECTS == (
        ("appalachia", "Tales From Appalachia", "fo76:fo4"),
        ("wasteland", "Legends of the Wasteland", "fnvfo3:fo4"),
        ("north", "Fables of the North", "skyrimse:fo4"),
    )
    assert _ENABLED_PROJECTS == (
        ("appalachia", "Tales From Appalachia", "fo76:fo4"),
    )

    workspace = AppalachiaWorkspace(_Settings())
    workspace.initialize()

    assert len(workspace._regen_panels) == 1
    assert {
        project_id: panel.fixed_pair_id
        for project_id, panel in workspace._regen_panels.items()
    } == {
        "appalachia": "fo76:fo4",
    }


def test_project_settings_and_status_are_isolated(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.os.cpu_count", lambda: 16)
    settings = _Settings()
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    north = RegenPanel(
        workspace,
        fixed_pair_id="skyrimse:fo4",
        project_id="north",
    )

    wasteland._set_workspace_settings(
        {
            "install_location": "mo2",
            "install_path": "N:/mods/fnv",
            "workers": 3,
            "lod_profile": "performance",
            "recovery_phase": "textures",
        }
    )
    north._set_workspace_settings(
        {
            "install_location": "vortex",
            "install_path": "N:/mods/skyrim",
            "workers": 7,
            "lod_profile": "high-quality",
            "recovery_phase": "havok",
        }
    )
    wasteland.handle_event(
        {"type": "phase_start", "data": {"phase": 1, "phase_name": "FNV phase"}}
    )

    reloaded_wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    reloaded_north = RegenPanel(
        workspace,
        fixed_pair_id="skyrimse:fo4",
        project_id="north",
    )
    assert (reloaded_wasteland.install_location, reloaded_wasteland.install_path) == (
        "mo2",
        "N:/mods/fnv",
    )
    assert reloaded_wasteland.workers == 3
    assert reloaded_wasteland.lod_profile == "performance"
    assert reloaded_wasteland.recovery_phase == "textures"
    assert (reloaded_north.install_location, reloaded_north.install_path) == (
        "vortex",
        "N:/mods/skyrim",
    )
    assert reloaded_north.workers == 7
    assert reloaded_north.lod_profile == "high-quality"
    assert reloaded_north.recovery_phase == "havok"
    assert wasteland._phases[0]["phase_name"] == "FNV phase"
    assert north._phases == []


def test_shared_runner_routes_events_only_to_owner(monkeypatch):
    workspace = AppalachiaWorkspace(_Settings())
    owner_events = []
    other_events = []
    log_events = []
    owner = SimpleNamespace(
        handle_event=owner_events.append,
        _log_panel=SimpleNamespace(handle_event=log_events.append),
    )
    other = SimpleNamespace(handle_event=other_events.append)
    event = {"type": "log", "level": "INFO", "message": "owned"}
    workspace._regen_panel = other
    workspace._runner_owner = owner
    workspace._runner = SimpleNamespace(drain=lambda: [event], done=False)
    workspace._initialized = True
    workspace.active = True
    monkeypatch.setattr(workspace, "_draw_changelog_popup", lambda: None)
    monkeypatch.setattr(workspace, "_draw_setup_confirm_popup", lambda: None)

    workspace.draw()

    assert owner_events == [event]
    assert log_events == [event]
    assert other_events == []


def test_shared_runner_rejects_concurrent_projects():
    workspace = AppalachiaWorkspace(_Settings())
    first = SimpleNamespace(done=False, start=lambda: None)
    workspace.start_conversion_runner(object(), first)

    with pytest.raises(RuntimeError, match="already running"):
        workspace.start_conversion_runner(
            object(),
            SimpleNamespace(done=False, start=lambda: None),
        )


def test_fixed_projects_resolve_pair_specific_output_plugins(monkeypatch):
    settings = _Settings()
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_exe_dir",
        lambda: Path("X:/BACUP"),
    )
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    north = RegenPanel(
        workspace,
        fixed_pair_id="skyrimse:fo4",
        project_id="north",
    )

    wasteland_paths = wasteland.build_paths()
    north_paths = north.build_paths()
    assert wasteland.generated_plugin_path().name == "FNV_FO3_Merged.esm"
    assert north.generated_plugin_path().name == "Skyrim_Merged.esm"
    assert wasteland_paths.additional_source_asset_roots == (Path("C:/fo3/Data"),)
    assert north_paths.additional_source_asset_roots == ()
    assert wasteland._required_game_ids() == ("fo4", "fnv", "fo3")
    assert north._required_game_ids() == ("fo4", "skyrimse")


def test_can_convert_needs_source_extractions_but_not_fo4_extraction():
    settings = _Settings()
    settings.paths["fo4"]["extracted_dir"] = ""
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    appalachia = RegenPanel(
        workspace,
        fixed_pair_id="fo76:fo4",
        project_id="appalachia",
    )
    appalachia._steam_installs_ok = lambda: True

    assert appalachia.can_convert() is True

    settings.paths["fo76"]["extracted_dir"] = ""
    assert appalachia.can_convert() is False


def test_wasteland_requires_both_source_asset_extractions():
    settings = _Settings()
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    wasteland._steam_installs_ok = lambda: True

    assert wasteland.can_convert() is True

    settings.paths["fo3"]["extracted_dir"] = ""
    assert wasteland.can_convert() is False


def test_wasteland_cleanup_routes_both_source_games_through_public_api(
    monkeypatch,
):
    settings = _Settings()
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_project_setup_ownership",
        lambda _settings, _project_id: SimpleNamespace(
            cleanup_mod_output=False,
            cleanup_extracted=True,
        ),
    )
    owned_checks = []
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.project_owns_extracted_path",
        lambda _settings, project_id, game_id, path, **_kwargs: (
            owned_checks.append((project_id, game_id, path)) or True
        ),
    )
    cleared = []
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.clear_project_owned_extractions",
        lambda _settings, project_id, **_kwargs: (
            cleared.append(project_id) or ("fnv", "fo3")
        ),
    )

    removed = wasteland._cleanup_after_deploy(wasteland.build_paths(), True)

    assert {(project_id, game_id) for project_id, game_id, _path in owned_checks} == {
        ("wasteland", "fnv"),
        ("wasteland", "fo3"),
    }
    assert cleared == ["wasteland"]
    assert removed == ["C:/fnv/Data", "C:/fo3/Data"]


def test_wasteland_preflight_checks_grafted_asset_root(tmp_path):
    settings = _Settings()
    fnv_extracted = tmp_path / "fnv"
    fnv_extracted.mkdir()
    settings.paths["fnv"]["extracted_dir"] = str(fnv_extracted)
    settings.paths["fo3"]["extracted_dir"] = str(tmp_path / "missing-fo3")
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )

    report = wasteland._input_preflight_report()

    assert [item.label for item in report.required_missing] == [
        "FO3 extracted directory"
    ]
    assert report.required_missing[0].checked_path == str(tmp_path / "missing-fo3")


def test_disk_usage_deduplicates_primary_and_grafted_asset_roots(monkeypatch):
    settings = _Settings()
    settings.paths["fo4"]["extracted_dir"] = ""
    settings.paths["fo3"]["extracted_dir"] = settings.paths["fnv"]["extracted_dir"]
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    scanned = []
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._dir_size_bytes",
        lambda path: scanned.append(path) or 10,
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._ba2_size_bytes",
        lambda _path: 0,
    )

    summary = wasteland._compute_disk_usage_summary()

    assert summary["extracted"] == 10
    assert scanned.count(Path("C:/fnv/Data")) == 1
