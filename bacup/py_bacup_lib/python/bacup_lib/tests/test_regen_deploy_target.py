from pathlib import Path

import pytest

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenPaths


def _paths(tmp_path: Path, *, deploy_data_dir: Path | None = None) -> RegenPaths:
    return RegenPaths(
        source_extracted_dir=tmp_path / "fo76_extracted",
        source_data_dir=tmp_path / "fo76" / "Data",
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        deploy_data_dir=deploy_data_dir,
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=tmp_path / "mods" / "SeventySix",
        resource_dir=tmp_path / "resource",
    )


class _Timing:
    def __init__(self) -> None:
        self.records: list[tuple[str, dict]] = []

    def record(self, name: str, _elapsed: float, **kwargs) -> None:
        self.records.append((name, kwargs))


def test_deploy_post_steps_uses_mo2_target_without_ini_edits(monkeypatch, tmp_path):
    mo2_dir = tmp_path / "ModOrganizer" / "mods" / "SeventySix"
    calls: dict[str, object] = {}

    def fake_deploy_output_mods(
        output_root_name,
        *,
        plugin_names,
        project_root,
        game_data_dir,
        resource_dir,
        deploy_archives=True,
    ):
        calls["deploy"] = {
            "output_root_name": output_root_name,
            "plugin_names": plugin_names,
            "project_root": project_root,
            "game_data_dir": game_data_dir,
            "resource_dir": resource_dir,
            "deploy_archives": deploy_archives,
        }

    monkeypatch.setattr(regen_pipeline, "_deploy_output_mods", fake_deploy_output_mods)
    monkeypatch.setattr(regen_pipeline, "_deployed_archive_names", lambda *a, **k: ["SeventySix - Main.ba2"])
    def fail_if_called(message: str):
        def _fail(*_args, **_kwargs):
            raise AssertionError(message)

        return _fail

    monkeypatch.setattr(
        regen_pipeline,
        "_write_runtime_archive_ini_state",
        fail_if_called("INI state should not be written"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_fo4_ini_archive_names_for_plugins",
        fail_if_called("INI entries should not be read"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_remove_fo4_archive_ini_entries",
        fail_if_called("INI entries should not be removed"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_cleanup_fo4_archive_ini_overrides",
        fail_if_called("CK INI should not be cleaned"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_register_runtime_archive_ini_entries",
        fail_if_called("runtime INI should not be registered"),
    )

    timing = _Timing()
    regen_pipeline._deploy_post_steps(_paths(tmp_path, deploy_data_dir=mo2_dir), ["SeventySix.esm"], timing)

    assert calls["deploy"]["game_data_dir"] == mo2_dir
    assert calls["deploy"]["deploy_archives"] is True
    assert timing.records[0][0] == "deploy"
    assert timing.records[0][1]["deploy_data_dir"] == str(mo2_dir)
    assert timing.records[0][1]["registered_runtime_ini_entries"] == 0


def test_deploy_post_steps_can_skip_runtime_ini_updates(monkeypatch, tmp_path):
    calls: dict[str, object] = {}

    def fake_deploy_output_mods(
        output_root_name,
        *,
        plugin_names,
        project_root,
        game_data_dir,
        resource_dir,
        deploy_archives=True,
    ):
        calls["deploy"] = {
            "output_root_name": output_root_name,
            "plugin_names": plugin_names,
            "project_root": project_root,
            "game_data_dir": game_data_dir,
            "resource_dir": resource_dir,
            "deploy_archives": deploy_archives,
        }

    def fail_if_called(message: str):
        def _fail(*_args, **_kwargs):
            raise AssertionError(message)

        return _fail

    monkeypatch.setattr(regen_pipeline, "_deploy_output_mods", fake_deploy_output_mods)
    monkeypatch.setattr(regen_pipeline, "_deployed_archive_names", lambda *a, **k: ["SeventySix - Main.ba2"])
    monkeypatch.setattr(
        regen_pipeline,
        "_write_runtime_archive_ini_state",
        fail_if_called("INI state should not be written"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_fo4_ini_archive_names_for_plugins",
        fail_if_called("INI entries should not be read"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_remove_fo4_archive_ini_entries",
        fail_if_called("INI entries should not be removed"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_cleanup_fo4_archive_ini_overrides",
        fail_if_called("CK INI should not be cleaned"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_register_runtime_archive_ini_entries",
        fail_if_called("runtime INI should not be registered"),
    )

    timing = _Timing()
    regen_pipeline._deploy_post_steps(
        _paths(tmp_path),
        ["SeventySix.esm"],
        timing,
        update_runtime_ini=False,
    )

    assert calls["deploy"]["game_data_dir"] == tmp_path / "Fallout4" / "Data"
    assert timing.records[0][0] == "deploy"
    assert timing.records[0][1]["registered_runtime_ini_entries"] == 0
    assert timing.records[0][1]["ini_updates_skipped"] is True


def test_deploy_existing_requires_generated_plugin(tmp_path):
    result = regen_pipeline.deploy_existing(_paths(tmp_path))

    assert result.exit_code == 2
    assert result.deployed is False
    assert "missing generated plugin" in result.failures[0]


def test_deploy_existing_copies_archives_when_output_has_ba2(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    paths.output_root.mkdir(parents=True)
    (paths.output_root / "SeventySix.esm").write_bytes(b"esm")
    (paths.output_root / "SeventySix - Main.ba2").write_bytes(b"ba2")
    calls: dict[str, object] = {}

    def fake_deploy_post_steps(
        paths_arg,
        plugin_names,
        timing_report,
        *,
        archives_already_deployed=False,
        update_runtime_ini=True,
    ):
        calls["paths"] = paths_arg
        calls["plugin_names"] = plugin_names
        calls["archives_already_deployed"] = archives_already_deployed
        calls["update_runtime_ini"] = update_runtime_ini

    monkeypatch.setattr(regen_pipeline, "_deploy_post_steps", fake_deploy_post_steps)

    result = regen_pipeline.deploy_existing(paths, update_runtime_ini=False)

    assert result.exit_code == 0
    assert result.deployed is True
    assert calls["paths"] is paths
    assert calls["plugin_names"] == ["SeventySix.esm"]
    assert calls["archives_already_deployed"] is False
    assert calls["update_runtime_ini"] is False


def test_deploy_existing_skips_archive_copy_when_output_has_no_ba2(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    paths.output_root.mkdir(parents=True)
    (paths.output_root / "SeventySix.esm").write_bytes(b"esm")
    calls: dict[str, object] = {}

    def fake_deploy_post_steps(
        paths_arg,
        plugin_names,
        timing_report,
        *,
        archives_already_deployed=False,
        update_runtime_ini=True,
    ):
        calls["archives_already_deployed"] = archives_already_deployed

    monkeypatch.setattr(regen_pipeline, "_deploy_post_steps", fake_deploy_post_steps)

    result = regen_pipeline.deploy_existing(paths)

    assert result.exit_code == 0
    assert result.deployed is True
    assert calls["archives_already_deployed"] is True


def test_deploy_output_mods_reuses_built_outputs_without_validation(monkeypatch, tmp_path):
    project_root = tmp_path
    (project_root / "mods" / "SeventySix").mkdir(parents=True)
    game_data_dir = tmp_path / "Fallout4" / "Data"
    resource_dir = tmp_path / "resource"
    calls: list[tuple[str, dict]] = []

    def fake_deploy_mod(mod_name, **kwargs):
        calls.append((mod_name, kwargs))

    monkeypatch.setattr("creation_lib.build.deployer.deploy_mod", fake_deploy_mod)

    regen_pipeline._deploy_output_mods(
        "SeventySix",
        plugin_names=["SeventySix.esm"],
        project_root=project_root,
        game_data_dir=game_data_dir,
        resource_dir=resource_dir,
    )

    assert calls == [
        (
            "SeventySix",
            {
                "game": "fo4",
                "game_data_dir": game_data_dir,
                "skip_build": True,
                "skip_pack": True,
                "skip_papyrus_compile": True,
                "esp_only": False,
                "skip_validation": True,
                "project_root": project_root,
                "resource_dir": resource_dir,
                "deploy_archives": True,
            },
        )
    ]


def test_deploy_output_mods_supports_distinct_mod_and_plugin_names(
    monkeypatch, tmp_path
):
    project_root = tmp_path
    mod_dir = project_root / "mods" / "CustomMojave"
    mod_dir.mkdir(parents=True)
    plugin_path = mod_dir / "FNV_FO3_Merged.esm"
    plugin_path.write_bytes(b"merged")
    game_data_dir = tmp_path / "Fallout4" / "Data"

    def fake_deploy_mod(mod_name, **kwargs):
        exposed = project_root / "mods" / mod_name / f"{mod_name}.esm"
        assert exposed.read_bytes() == b"merged"
        game_data_dir.mkdir(parents=True, exist_ok=True)
        (game_data_dir / exposed.name).write_bytes(exposed.read_bytes())

    monkeypatch.setattr("creation_lib.build.deployer.deploy_mod", fake_deploy_mod)

    regen_pipeline._deploy_output_mods(
        "CustomMojave",
        plugin_names=["FNV_FO3_Merged.esm"],
        project_root=project_root,
        game_data_dir=game_data_dir,
        resource_dir=tmp_path / "resource",
    )

    assert (game_data_dir / "FNV_FO3_Merged.esm").read_bytes() == b"merged"
    assert not (game_data_dir / "CustomMojave.esm").exists()
    assert not (mod_dir / "CustomMojave.esm").exists()


def test_distinct_plugin_deploy_cleans_temporary_alias_on_failure(
    monkeypatch, tmp_path
):
    mod_dir = tmp_path / "mods" / "MojaveCapital"
    mod_dir.mkdir(parents=True)
    (mod_dir / "FNV_FO3_Merged.esm").write_bytes(b"merged")

    monkeypatch.setattr(
        "creation_lib.build.deployer.deploy_mod",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(RuntimeError("deploy failed")),
    )

    with pytest.raises(RuntimeError, match="deploy failed"):
        regen_pipeline._deploy_output_mods(
            "MojaveCapital",
            plugin_names=["FNV_FO3_Merged.esm"],
            project_root=tmp_path,
            game_data_dir=tmp_path / "Fallout4" / "Data",
            resource_dir=tmp_path / "resource",
        )

    assert not (mod_dir / "MojaveCapital.esm").exists()


def test_deploy_post_steps_uses_explicit_mod_name_for_mod_and_archives(
    monkeypatch, tmp_path
):
    paths = _paths(tmp_path, deploy_data_dir=tmp_path / "MO2" / "CustomMojave")
    paths.output_root = tmp_path / "mods" / "CustomMojave"
    paths.mod_name = "CustomMojave"
    captured = {}

    monkeypatch.setattr(
        regen_pipeline,
        "_deploy_output_mods",
        lambda output_root_name, **_kwargs: captured.update(
            output_root_name=output_root_name
        ),
    )

    def deployed_archives(_data_dir, plugin_names):
        captured["archive_plugin_names"] = plugin_names
        return []

    monkeypatch.setattr(regen_pipeline, "_deployed_archive_names", deployed_archives)

    regen_pipeline._deploy_post_steps(
        paths,
        ["FNV_FO3_Merged.esm"],
        _Timing(),
        update_runtime_ini=False,
    )

    assert captured == {
        "output_root_name": "CustomMojave",
        "archive_plugin_names": ["CustomMojave.esm"],
    }
