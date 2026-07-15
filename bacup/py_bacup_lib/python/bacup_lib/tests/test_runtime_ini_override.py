from pathlib import Path

from bacup_lib import regen_pipeline
from bacup_lib.tests.test_regen_deploy_target import _paths


class _Timing:
    def __init__(self) -> None:
        self.records: list[tuple[str, dict]] = []

    def record(self, name: str, _elapsed: float, **kwargs) -> None:
        self.records.append((name, kwargs))


def test_terrain_and_lod_texture_archives_use_texture_ini_key():
    assert (
        regen_pipeline._fo4_archive_ini_key_for_archive("SeventySix - TerrainTextures2.ba2")
        == regen_pipeline._FO4_ARCHIVE_TEXTURE_KEY
    )
    assert (
        regen_pipeline._fo4_archive_ini_key_for_archive("SeventySix - LODTextures.ba2")
        == regen_pipeline._FO4_ARCHIVE_TEXTURE_KEY
    )
    assert (
        regen_pipeline._fo4_archive_ini_key_for_archive("SeventySix - LOD.ba2")
        == regen_pipeline._FO4_ARCHIVE_MAIN_KEY
    )


def _fake_deploy_output_mods(
    output_root_name,
    *,
    plugin_names,
    project_root,
    game_data_dir,
    resource_dir,
    deploy_archives=True,
):
    return None


def test_runtime_ini_override_writes_ini_for_virtual_deploy_target(monkeypatch, tmp_path):
    mo2_dir = tmp_path / "ModOrganizer" / "mods" / "SeventySix"
    override_ini = tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini"

    monkeypatch.setattr(regen_pipeline, "_deploy_output_mods", _fake_deploy_output_mods)
    monkeypatch.setattr(
        regen_pipeline, "_deployed_archive_names", lambda *a, **k: ["SeventySix - Main.ba2"]
    )

    paths = _paths(tmp_path, deploy_data_dir=mo2_dir)
    paths.runtime_ini_path = override_ini
    assert not paths.target_ck_ini_path.exists()

    timing = _Timing()
    regen_pipeline._deploy_post_steps(
        paths, ["SeventySix.esm"], timing, update_runtime_ini=True
    )

    assert override_ini.is_file()
    content = override_ini.read_text(encoding="utf-8")
    assert "[Archive]" in content
    assert "SeventySix - Main.ba2" in content
    assert timing.records[0][0] == "deploy"
    assert timing.records[0][1]["registered_runtime_ini_entries"] == 1
    assert timing.records[0][1]["ini_updates_skipped"] is False

    # legacy default (target_custom_ini_path) is untouched -- entries went to the override only
    assert not paths.target_custom_ini_path.exists()
    # a virtual deploy must never write the real-install CreationKitCustom.ini
    assert not paths.target_ck_ini_path.exists()


def test_runtime_ini_override_replaces_previous_archive_names(
    monkeypatch, tmp_path: Path
):
    mo2_dir = tmp_path / "ModOrganizer" / "mods" / "SeventySix"
    override_ini = tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini"
    override_ini.parent.mkdir(parents=True)
    override_ini.write_text(
        "[Archive]\n"
        "sResourceArchiveList=OtherMod - Main.ba2, SeventySix - Sounds.ba2\n"
        "sResourceIndexFileList=SeventySix - Textures.ba2\n",
        encoding="utf-8",
    )
    current_names = [
        "SeventySix - Sounds1.ba2",
        "SeventySix - Sounds2.ba2",
        "SeventySix - Textures1.ba2",
    ]

    monkeypatch.setattr(regen_pipeline, "_deploy_output_mods", _fake_deploy_output_mods)
    monkeypatch.setattr(
        regen_pipeline, "_deployed_archive_names", lambda *a, **k: current_names
    )

    paths = _paths(tmp_path, deploy_data_dir=mo2_dir)
    paths.runtime_ini_path = override_ini
    timing = _Timing()

    regen_pipeline._deploy_post_steps(
        paths, ["SeventySix.esm"], timing, update_runtime_ini=True
    )

    content = override_ini.read_text(encoding="utf-8")
    assert "SeventySix - Sounds.ba2" not in content
    assert "SeventySix - Textures.ba2" not in content
    assert all(name in content for name in current_names)
    assert "OtherMod - Main.ba2" in content
    assert timing.records[0][1]["removed_runtime_archive_ini_entries"] == 2
    assert timing.records[0][1]["registered_runtime_ini_entries"] == 3


def test_no_runtime_ini_override_still_skips_ini_for_virtual_deploy_target(monkeypatch, tmp_path):
    mo2_dir = tmp_path / "ModOrganizer" / "mods" / "SeventySix"

    def fail_if_called(message: str):
        def _fail(*_args, **_kwargs):
            raise AssertionError(message)

        return _fail

    monkeypatch.setattr(regen_pipeline, "_deploy_output_mods", _fake_deploy_output_mods)
    monkeypatch.setattr(
        regen_pipeline, "_deployed_archive_names", lambda *a, **k: ["SeventySix - Main.ba2"]
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_write_runtime_archive_ini_state",
        fail_if_called("INI state should not be written"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_register_runtime_archive_ini_entries",
        fail_if_called("runtime INI should not be registered"),
    )

    paths = _paths(tmp_path, deploy_data_dir=mo2_dir)
    assert paths.runtime_ini_path is None

    timing = _Timing()
    regen_pipeline._deploy_post_steps(
        paths, ["SeventySix.esm"], timing, update_runtime_ini=True
    )

    assert not paths.target_custom_ini_path.exists()
    assert timing.records[0][0] == "deploy"
    assert timing.records[0][1]["registered_runtime_ini_entries"] == 0
    assert timing.records[0][1]["ini_updates_skipped"] is True
