from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenPaths


def _paths(tmp_path: Path) -> RegenPaths:
    return RegenPaths(
        source_extracted_dir=tmp_path / "fo76_extracted",
        source_data_dir=tmp_path / "fo76" / "Data",
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=tmp_path / "BACUP" / "mods" / "SeventySix",
        resource_dir=tmp_path / "resource",
    )


def test_hydrate_upgrade_workspace_reuses_complete_loose_assets(
    monkeypatch, tmp_path
):
    paths = _paths(tmp_path)
    local_data = paths.output_root / "data"
    for root in regen_pipeline._UPGRADE_LOOSE_DATA_ROOTS:
        (local_data / root.lower()).mkdir(parents=True)
    existing = local_data / "meshes" / "existing.nif"
    existing.write_bytes(b"loose")
    (paths.output_root / "SeventySix - Main.ba2").write_bytes(b"stale archive")
    paths.target_data_dir.mkdir(parents=True)
    deployed = paths.target_data_dir / "SeventySix - Main.ba2"
    deployed.write_bytes(b"deployed archive")
    other_mod = paths.target_data_dir / "Other - Main.ba2"
    other_mod.write_bytes(b"other")
    logs: list[str] = []
    runner = SimpleNamespace(
        emit_log=lambda _level, message: logs.append(message),
        emit_status=lambda *_a, **_k: None,
        is_cancelled=lambda: False,
    )

    count = regen_pipeline._hydrate_upgrade_workspace_from_deployed(
        paths,
        ["SeventySix.esm"],
        runner=runner,
        workers=3,
    )

    assert count == 0
    assert existing.read_bytes() == b"loose"
    assert not list(paths.output_root.glob("*.ba2"))
    assert not deployed.exists(), "the local files replace the deployed archives"
    assert other_mod.exists()
    assert any("reusing existing loose assets" in message for message in logs)


def test_hydrate_upgrade_workspace_replaces_local_data_with_deployed_archives(
    monkeypatch, tmp_path
):
    paths = _paths(tmp_path)
    paths.target_data_dir.mkdir(parents=True)
    archives = [
        paths.target_data_dir / "SeventySix - Main.ba2",
        paths.target_data_dir / "SeventySix - Textures.ba2",
    ]
    for archive in archives:
        archive.write_bytes(b"archive")

    local_data = paths.output_root / "data"
    for root in regen_pipeline._UPGRADE_LOOSE_DATA_ROOTS[:-1]:
        (local_data / root).mkdir(parents=True)
    (local_data / "stale.txt").write_text("stale", encoding="utf-8")
    (paths.output_root / "SeventySix - Meshes.ba2").write_bytes(b"stale archive")

    extracted: list[tuple[Path, Path]] = []

    def fake_extract_archive(archive, output_dir, **_kwargs):
        archive_path = Path(archive)
        output_path = Path(output_dir)
        extracted.append((archive_path, output_path))
        family = archive_path.stem.rsplit(" - ", 1)[-1]
        member = output_path / "seed" / f"{family}.txt"
        member.parent.mkdir(parents=True, exist_ok=True)
        member.write_text(family, encoding="utf-8")
        return 1

    from creation_lib.ba2 import native_runtime

    monkeypatch.setattr(native_runtime, "extract_archive", fake_extract_archive)
    runner = SimpleNamespace(
        emit_log=lambda *_a, **_k: None,
        emit_status=lambda *_a, **_k: None,
        is_cancelled=lambda: False,
    )

    count = regen_pipeline._hydrate_upgrade_workspace_from_deployed(
        paths,
        ["SeventySix.esm"],
        runner=runner,
        workers=3,
    )

    assert count == 2
    assert [archive for archive, _output in extracted] == archives
    assert len({output for _archive, output in extracted}) == 1
    assert not (local_data / "stale.txt").exists()
    assert (local_data / "seed" / "Main.txt").read_text(encoding="utf-8") == "Main"
    assert (
        local_data / "seed" / "Textures.txt"
    ).read_text(encoding="utf-8") == "Textures"
    assert not list(paths.output_root.glob("*.ba2"))


def test_hydrate_upgrade_workspace_requires_a_deployed_ba2(tmp_path):
    paths = _paths(tmp_path)
    paths.target_data_dir.mkdir(parents=True)
    runner = SimpleNamespace(
        emit_log=lambda *_a, **_k: None,
        emit_status=lambda *_a, **_k: None,
        is_cancelled=lambda: False,
    )

    with pytest.raises(FileNotFoundError, match="requires deployed BA2 archives"):
        regen_pipeline._hydrate_upgrade_workspace_from_deployed(
            paths,
            ["SeventySix.esm"],
            runner=runner,
            workers=1,
        )


def test_failed_upgrade_extraction_preserves_existing_workspace(
    monkeypatch, tmp_path
):
    paths = _paths(tmp_path)
    paths.target_data_dir.mkdir(parents=True)
    (paths.target_data_dir / "SeventySix - Main.ba2").write_bytes(b"archive")
    local_data = paths.output_root / "data"
    local_data.mkdir(parents=True)
    existing = local_data / "existing.txt"
    existing.write_text("keep", encoding="utf-8")

    from creation_lib.ba2 import native_runtime

    def fail_extract(*_args, **_kwargs):
        raise RuntimeError("broken BA2")

    monkeypatch.setattr(
        native_runtime,
        "extract_archive",
        fail_extract,
    )
    runner = SimpleNamespace(
        emit_log=lambda *_a, **_k: None,
        emit_status=lambda *_a, **_k: None,
        is_cancelled=lambda: False,
    )

    with pytest.raises(RuntimeError, match="broken BA2"):
        regen_pipeline._hydrate_upgrade_workspace_from_deployed(
            paths,
            ["SeventySix.esm"],
            runner=runner,
            workers=1,
        )

    assert existing.read_text(encoding="utf-8") == "keep"


def _runner(logs=None):
    return SimpleNamespace(
        emit_log=lambda _level, message: (logs if logs is not None else []).append(message),
        emit_status=lambda *_a, **_k: None,
        is_cancelled=lambda: False,
    )


def test_hydrate_upgrade_workspace_copies_a_loose_deployment_back(tmp_path):
    import json

    paths = _paths(tmp_path)
    data = paths.target_data_dir
    for rel in ("meshes/a.nif", "textures/b.dds", "SeventySix.esm"):
        (data / rel).parent.mkdir(parents=True, exist_ok=True)
        (data / rel).write_bytes(rel.encode())
    (data / ".bacup-loose-SeventySix.json").write_text(json.dumps({"files": [
        {"rel": "meshes/a.nif", "src_root": "Meshes", "src_rel": "a.nif"},
        {"rel": "textures/b.dds", "src_root": "data", "src_rel": "textures/b.dds"},
        {"rel": "SeventySix.esm", "src_root": "", "src_rel": "SeventySix.esm"},
        {"rel": "../escape.txt", "src_root": "data", "src_rel": "../escape.txt"},
    ]}), encoding="utf-8")

    count = regen_pipeline._hydrate_upgrade_workspace_from_deployed(paths, ["SeventySix.esm"], runner=_runner(), workers=4)

    assert count == 2
    assert (paths.output_root / "Meshes/a.nif").read_bytes() == b"meshes/a.nif"
    assert (paths.output_root / "data/textures/b.dds").read_bytes() == b"textures/b.dds"
    assert not (paths.output_root / "SeventySix.esm").exists(), "the plugin is rebuilt, never copied back"


def test_hydration_restores_btd4_and_precombine_sidecars_without_clobbering_local(tmp_path):
    paths = _paths(tmp_path)
    local_data = paths.output_root / "data"
    for root in regen_pipeline._UPGRADE_LOOSE_DATA_ROOTS:
        (local_data / root).mkdir(parents=True)
    data = paths.target_data_dir
    (data / "Terrain").mkdir(parents=True)
    (data / "Terrain/Appalachia.btd4").write_bytes(b"dense")
    for name in ("SeventySix - Geometry.csg", "SeventySix.cdx"):
        (data / name).write_bytes(b"deployed")
    (paths.output_root / "SeventySix.cdx").write_bytes(b"local")

    regen_pipeline._hydrate_upgrade_workspace_from_deployed(paths, ["SeventySix.esm"], runner=_runner(), workers=1)

    assert (paths.output_root / "Terrain/Appalachia.btd4").read_bytes() == b"dense"
    assert (paths.output_root / "SeventySix - Geometry.csg").read_bytes() == b"deployed"
    assert (paths.output_root / "SeventySix.cdx").read_bytes() == b"local"


def test_previous_output_esm_prefers_the_deployed_copy_then_the_workspace(tmp_path):
    paths = _paths(tmp_path)
    local = paths.output_root / "SeventySix.esm"
    local.parent.mkdir(parents=True)
    local.write_bytes(b"local")
    assert regen_pipeline.previous_output_esm(paths, "SeventySix.esm") == local
    deployed = paths.target_data_dir / "SeventySix.esm"
    deployed.parent.mkdir(parents=True)
    deployed.write_bytes(b"deployed")
    assert regen_pipeline.previous_output_esm(paths, "SeventySix.esm") == deployed

    staged = regen_pipeline._stage_previous_output_esm(paths, "SeventySix.esm", _runner())
    assert staged.read_bytes() == b"deployed" and staged.parent == paths.output_root
