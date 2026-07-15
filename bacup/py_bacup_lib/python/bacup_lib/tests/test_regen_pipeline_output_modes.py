from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import PhaseSelection, regen_pipeline
from bacup_lib.regen_pipeline import RegenOptions, RegenPaths


def _paths(tmp_path: Path) -> RegenPaths:
    source_data = tmp_path / "fo76" / "Data"
    source_data.mkdir(parents=True)
    (source_data / "SeventySix.esm").write_bytes(b"TES4")
    return RegenPaths(
        source_extracted_dir=tmp_path / "fo76_extracted",
        source_data_dir=source_data,
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=tmp_path / "mods" / "SeventySix",
        resource_dir=tmp_path / "resource",
    )


@pytest.mark.parametrize(
    (
        "deploy",
        "deep_invariants",
        "expected_enable_ba2",
        "expected_skip_pack",
        "expected_deploy_calls",
    ),
    [
        (False, False, False, None, 0),
        (True, False, True, None, 1),
        (True, True, True, False, 1),
    ],
)
def test_ba2_creation_tracks_deploy_and_overwrite_is_forced(
    monkeypatch,
    tmp_path,
    deploy,
    deep_invariants,
    expected_enable_ba2,
    expected_skip_pack,
    expected_deploy_calls,
):
    paths = _paths(tmp_path)
    captures: dict[str, object] = {}
    deploy_calls: list[tuple[object, ...]] = []

    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_snapshot_land_cache", lambda *_a, **_k: True)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_deploy_post_steps", lambda *a, **_k: deploy_calls.append(a))

    def fake_check_run_invariants(*_args, **kwargs):
        captures["skip_pack"] = kwargs["skip_pack"]
        return [], []

    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", fake_check_run_invariants)

    import bacup_lib.models as models
    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)

    def fake_run_unified(request, _runner, **kwargs):
        captures["enable_ba2"] = kwargs["enable_ba2"]
        captures["archive_output_dir"] = kwargs.get("archive_output_dir")
        captures["overwrite_existing"] = request.options.overwrite_existing
        paths.output_root.mkdir(parents=True, exist_ok=True)
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4")
        return SimpleNamespace(
            run_result=SimpleNamespace(
                decisions=[],
                translated_counts={},
                skipped_counts={},
                failed_nifs=[],
                failed_textures=[],
                failed_bgsms=[],
                btos_failed=0,
                btos_total=0,
            )
        )

    monkeypatch.setattr(unified, "run_unified", fake_run_unified)

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(
            deploy=deploy,
            lod_mode="none",
            deep_invariants=deep_invariants,
        ),
        phases=PhaseSelection(lod_mode="none"),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert captures["enable_ba2"] is expected_enable_ba2
    assert captures.get("skip_pack") is expected_skip_pack
    assert captures["overwrite_existing"] is True
    assert captures["archive_output_dir"] is None
    assert len(deploy_calls) == expected_deploy_calls


def test_direct_deploy_archives_packs_to_deploy_data_and_skips_archive_deploy(
    monkeypatch, tmp_path
):
    paths = _paths(tmp_path)
    paths.deploy_data_dir = tmp_path / "MO2" / "mods" / "SeventySix"
    captures: dict[str, object] = {}
    deploy_kwargs: list[dict] = []

    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_snapshot_land_cache", lambda *_a, **_k: True)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)
    def fake_check_run_invariants(*_args, **kwargs):
        captures["skip_pack"] = kwargs["skip_pack"]
        return [], []

    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", fake_check_run_invariants)
    monkeypatch.setattr(
        regen_pipeline,
        "_deploy_post_steps",
        lambda *a, **kwargs: deploy_kwargs.append(kwargs),
    )

    import bacup_lib.models as models
    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)

    def fake_run_unified(request, _runner, **kwargs):
        captures["archive_output_dir"] = kwargs.get("archive_output_dir")
        captures["archive_max_bytes"] = kwargs["archive_max_bytes"]
        captures["papyrus_compiler"] = request.options.papyrus_compiler
        captures["generate_anim_text_data"] = request.options.generate_anim_text_data
        captures["anim_text_data_native"] = request.options.anim_text_data_native
        paths.output_root.mkdir(parents=True, exist_ok=True)
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4")
        return SimpleNamespace(
            run_result=SimpleNamespace(
                decisions=[],
                translated_counts={},
                skipped_counts={},
                failed_nifs=[],
                failed_textures=[],
                failed_bgsms=[],
                btos_failed=0,
                btos_total=0,
            )
        )

    monkeypatch.setattr(unified, "run_unified", fake_run_unified)

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(
            deploy=True,
            lod_mode="none",
            archive_max_bytes=4 * 1024**3,
            direct_deploy_archives=True,
            generate_anim_text_data=True,
            anim_text_data_native=True,
            deep_invariants=True,
        ),
        phases=PhaseSelection(lod_mode="none"),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert captures["archive_output_dir"] == paths.deploy_data_dir
    assert captures["archive_max_bytes"] == 4 * 1024**3
    assert captures["papyrus_compiler"] == "native"
    assert captures["generate_anim_text_data"] is True
    assert captures["anim_text_data_native"] is True
    assert captures["skip_pack"] is True
    assert deploy_kwargs == [
        {"archives_already_deployed": True, "update_runtime_ini": True}
    ]


def test_write_land_cache_false_skips_cache_snapshots(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    captures: dict[str, object] = {}
    final_snapshots: list[object] = []
    run_snapshots: list[object] = []

    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(
        regen_pipeline,
        "_snapshot_land_cache",
        lambda *a, **_k: final_snapshots.append(a) or True,
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_snapshot_land_cache_from_run",
        lambda *a, **_k: run_snapshots.append(a) or True,
    )
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", lambda *_a, **_k: ([], []))

    import bacup_lib.models as models
    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)

    def fake_run_unified(request, _runner, **kwargs):
        captures["land_cache_hook_result"] = kwargs["land_cache_hook"](
            SimpleNamespace(_rust_conversion_run=object())
        )
        paths.output_root.mkdir(parents=True, exist_ok=True)
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4")
        return SimpleNamespace(
            run_result=SimpleNamespace(
                decisions=[],
                translated_counts={},
                skipped_counts={},
                failed_nifs=[],
                failed_textures=[],
                failed_bgsms=[],
                btos_failed=0,
                btos_total=0,
            )
        )

    monkeypatch.setattr(unified, "run_unified", fake_run_unified)

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(deploy=False, lod_mode="none", write_land_cache=False),
        phases=PhaseSelection(lod_mode="none"),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert captures["land_cache_hook_result"] is False
    assert final_snapshots == []
    assert run_snapshots == []


def test_land_cache_snapshot_saves_through_conversion_run(tmp_path):
    class FakeRun:
        def __init__(self) -> None:
            self.calls: list[tuple[str, bool, bool]] = []

        def save_target(
            self,
            output_path: str,
            *,
            emit_authoring_yaml: bool,
            run_nvnm_validator: bool,
        ) -> None:
            self.calls.append((output_path, emit_authoring_yaml, run_nvnm_validator))
            Path(output_path).write_bytes(b"TES4")

    run = FakeRun()
    extracted = tmp_path / "extracted"
    extracted.mkdir()

    assert regen_pipeline._snapshot_land_cache_from_run(
        tmp_path,
        ["Output.esm"],
        run,
        data_dir=None,
        extracted_dir=extracted,
    )

    assert (tmp_path / ".regen_land_cache.esm").read_bytes() == b"TES4"
    assert run.calls == [
        (
            str(tmp_path / ".regen_land_cache.esm.tmp"),
            False,
            False,
        )
    ]


def test_land_cache_snapshots_and_restores_terrain_assets(tmp_path):
    output_root = tmp_path / "mods" / "SeventySix"
    source_data = tmp_path / "fo76" / "Data"
    source_data.mkdir(parents=True)
    (source_data / "SeventySix.esm").write_bytes(b"source")
    (output_root / "SeventySix.esm").parent.mkdir(parents=True)
    (output_root / "SeventySix.esm").write_bytes(b"built")

    terrain_texture = (
        output_root
        / "data"
        / "Textures"
        / "terrain"
        / "appalachia"
        / "LForestDirt01_d.dds"
    )
    terrain_material = (
        output_root
        / "data"
        / "Materials"
        / "terrain"
        / "appalachia"
        / "LForestDirt01.bgsm"
    )
    unrelated_texture = output_root / "data" / "Textures" / "actors" / "body_d.dds"
    terrain_texture.parent.mkdir(parents=True)
    terrain_material.parent.mkdir(parents=True)
    unrelated_texture.parent.mkdir(parents=True)
    terrain_texture.write_bytes(b"dds")
    terrain_material.write_bytes(b"bgsm")
    unrelated_texture.write_bytes(b"actor")

    assert regen_pipeline._snapshot_land_cache(
        output_root,
        ["SeventySix.esm"],
        data_dir=source_data,
        extracted_dir=tmp_path / "fo76_extracted",
    )

    import shutil

    shutil.rmtree(output_root / "data")
    restored = regen_pipeline._restore_land_cache_assets(output_root)

    assert restored == 2
    assert terrain_texture.read_bytes() == b"dds"
    assert terrain_material.read_bytes() == b"bgsm"
    assert not unrelated_texture.exists()


def test_reuse_land_restores_cached_assets_before_unified_run(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    paths.output_root.mkdir(parents=True)
    (paths.output_root / ".regen_land_cache.esm").write_bytes(b"cache")
    cached_texture = (
        paths.output_root
        / ".regen_land_cache_assets"
        / "data"
        / "Textures"
        / "terrain"
        / "appalachia"
        / "LForestDirt01_d.dds"
    )
    cached_texture.parent.mkdir(parents=True)
    cached_texture.write_bytes(b"dds")

    captures: dict[str, object] = {}

    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", lambda *_a, **_k: ([], []))

    import bacup_lib.models as models
    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)

    def fake_run_unified(request, _runner, **_kwargs):
        restored_texture = (
            request.output_root
            / "SeventySix"
            / "data"
            / "Textures"
            / "terrain"
            / "appalachia"
            / "LForestDirt01_d.dds"
        )
        captures["restored_before_run"] = restored_texture.is_file()
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4")
        return SimpleNamespace(
            run_result=SimpleNamespace(
                decisions=[],
                translated_counts={},
                skipped_counts={},
                failed_nifs=[],
                failed_textures=[],
                failed_bgsms=[],
                btos_failed=0,
                btos_total=0,
            )
        )

    monkeypatch.setattr(unified, "run_unified", fake_run_unified)

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(deploy=False, lod_mode="none", re_use_land=True),
        phases=PhaseSelection(lod_mode="none"),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert captures["restored_before_run"] is True


def test_resume_from_textures_skips_prior_phases_and_overwrites(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    paths.output_root.mkdir(parents=True)
    (paths.output_root / "SeventySix.esm").write_bytes(b"TES4")
    captures: dict[str, object] = {}

    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", lambda *_a, **_k: ([], []))

    import bacup_lib.models as models
    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)

    def fake_run_unified(request, _runner, **kwargs):
        opts = request.options
        captures["enable_ba2"] = kwargs["enable_ba2"]
        captures["translate_records"] = opts.translate_records
        captures["convert_terrain"] = opts.convert_terrain
        captures["build_esp"] = opts.build_esp
        captures["convert_nifs"] = opts.convert_nifs
        captures["convert_textures"] = opts.convert_textures
        captures["convert_materials"] = opts.convert_materials
        captures["convert_havok"] = opts.convert_havok
        captures["synthesize_drivers"] = opts.synthesize_drivers
        captures["convert_animations"] = opts.convert_animations
        captures["copy_sounds"] = opts.copy_sounds
        captures["overwrite_existing"] = opts.overwrite_existing
        return SimpleNamespace(
            run_result=SimpleNamespace(
                decisions=[],
                translated_counts={},
                skipped_counts={},
                failed_nifs=[],
                failed_textures=[],
                failed_bgsms=[],
                btos_failed=0,
                btos_total=0,
            )
        )

    monkeypatch.setattr(unified, "run_unified", fake_run_unified)

    result = regen_pipeline.run_resume_from_phase(
        paths,
        RegenOptions(deploy=False, lod_mode="none"),
        start_phase="textures",
        phases=PhaseSelection(lod_mode="none"),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert captures == {
        "enable_ba2": False,
        "translate_records": False,
        "convert_terrain": False,
        "build_esp": False,
        "convert_nifs": False,
        "convert_textures": True,
        "convert_materials": True,
        "convert_havok": True,
        "synthesize_drivers": True,
        "convert_animations": True,
        "copy_sounds": True,
        "overwrite_existing": True,
    }


def test_resume_requires_existing_generated_plugin(tmp_path):
    paths = _paths(tmp_path)

    with pytest.raises(FileNotFoundError, match="requires existing generated plugin"):
        regen_pipeline.run_resume_from_phase(
            paths,
            RegenOptions(deploy=False, lod_mode="none"),
            start_phase="nifs",
            phases=PhaseSelection(lod_mode="none"),
            runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
        )


def test_resume_from_lodgen_runs_lod_pack_and_deploy(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    paths.deploy_data_dir = tmp_path / "MO2" / "mods" / "SeventySix"
    paths.output_root.mkdir(parents=True)
    (paths.output_root / "SeventySix.esm").write_bytes(b"TES4")
    captures: dict[str, object] = {"phase_complete": []}

    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 2)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)

    def fake_lod(**kwargs):
        captures["lod"] = kwargs

    def fake_pack(pack_paths, pack_options, *, resolved_workers):
        captures["pack"] = (pack_paths, pack_options, resolved_workers)
        return True

    def fake_deploy(*_args, **kwargs):
        captures["deploy_kwargs"] = kwargs

    def fake_invariants(*_args, **kwargs):
        captures["skip_pack"] = kwargs["skip_pack"]
        return [], []

    monkeypatch.setattr(regen_pipeline, "_run_generate_lod", fake_lod)
    monkeypatch.setattr(regen_pipeline, "_target_lod_asset_dirs", lambda *_a, **_k: [])
    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.discover_worldspaces",
        lambda *_a, **_k: (_ for _ in ()).throw(
            AssertionError("FO76 resume must keep configured APPALACHIA")
        ),
    )
    monkeypatch.setattr(regen_pipeline, "_pack_existing_output", fake_pack)
    monkeypatch.setattr(regen_pipeline, "_deploy_post_steps", fake_deploy)
    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", fake_invariants)

    runner = SimpleNamespace(
        emit_log=lambda *_a, **_k: None,
        emit_phase_start=lambda _p: None,
        emit_phase_complete=lambda p: captures["phase_complete"].append(p.phase_name),
    )
    result = regen_pipeline.run_resume_from_phase(
        paths,
        RegenOptions(
            deploy=True,
            lod_mode="hybrid-atlas",
            direct_deploy_archives=True,
            deep_invariants=True,
        ),
        start_phase="lodgen",
        phases=PhaseSelection(lod_mode="hybrid-atlas"),
        runner=runner,
        lod_settings={"global": {"worldspaces": ["APPALACHIA"]}},
    )

    assert result.exit_code == 0
    assert result.deployed is True
    assert captures["lod"]["mod_root"] == paths.output_root
    assert captures["lod"]["working_esm"] == paths.output_root / "SeventySix.esm"
    assert captures["lod"]["worldspaces"] == ["APPALACHIA"]
    assert captures["lod"]["source_data_dir"] == paths.source_extracted_dir
    assert captures["pack"][0] == paths
    assert captures["pack"][1].lod_mode == "hybrid-atlas"
    assert captures["pack"][2] == 2
    assert captures["skip_pack"] is True
    assert captures["deploy_kwargs"] == {
        "archives_already_deployed": True,
        "update_runtime_ini": True,
    }
    assert captures["phase_complete"] == ["Generate LOD", "Pack BA2", "Deploy Mod"]
