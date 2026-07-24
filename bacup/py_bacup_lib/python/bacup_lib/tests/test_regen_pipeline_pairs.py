from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import PhaseSelection, regen_pipeline
from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.regen_pipeline import RegenOptions, RegenPaths
from bacup_lib.source_pairs import FNV_MVP_EXCLUDE_SIGNATURES, get_pair
import bacup_lib.workflows.unified as unified


def _paths(tmp_path: Path, *, mod_name: str = "MojaveCapital") -> RegenPaths:
    return RegenPaths(
        source_extracted_dir=tmp_path / "fnv_extracted",
        source_data_dir=tmp_path / "fnv" / "Data",
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=tmp_path / "mods" / mod_name,
        mod_name=mod_name,
        diagnostics_root=tmp_path / "run-logs",
        merge_primary_plugin_paths=(tmp_path / "FalloutNV.esm",),
        merge_grafted_plugin_paths=(tmp_path / "Fallout3.esm",),
        additional_source_asset_roots=(tmp_path / "fo3_extracted",),
    )


def test_run_full_regen_merges_pair_sources_before_unified(monkeypatch, tmp_path):
    paths = _paths(tmp_path, mod_name="CustomMojave")
    source_strings_dir = paths.source_extracted_dir / "Strings"
    source_strings_dir.mkdir(parents=True)
    paths.additional_source_asset_roots[0].mkdir(parents=True)
    merge_calls: list[dict] = []
    captured_requests: list[PluginPortRequest] = []
    invariant_plugin_names: list[list[str]] = []
    preflight_sources: list[Path] = []
    preflight_excludes: list[frozenset[str]] = []
    unified_kwargs: list[dict] = []

    class _Native:
        def conversion_merge_sources(self, options):
            merge_calls.append(options)
            Path(options["output_path"]).write_bytes(b"TES4")
            Path(options["report_path"]).write_bytes(b'{"native":"exact"}\n')
            return {
                "copied": 1,
                "deduped": 0,
                "pack_origins": [
                    {
                        "merged_form_key": "000900@FNV_FO3_Merged.esm",
                        "source_game": "fo3",
                        "source_plugin": "Fallout3.esm",
                        "source_form_key": "00000900@Fallout3.esm",
                    }
                ],
                "pack_accounting": {
                    "raw_source": {"fnv": 4_888, "fo3": 4_567},
                    "final_survivors": {"fnv": 4_885, "fo3": 3_264},
                },
            }

    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: _Native(),
    )
    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_existing_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(
        regen_pipeline,
        "_check_run_invariants",
        lambda _root, _records_only, plugin_names, **_kwargs: (
            invariant_plugin_names.append(plugin_names) or [],
            [],
        ),
    )
    monkeypatch.setattr(
        "bacup_lib.target_assets.ensure_target_asset_catalog",
        lambda *_a, **_k: None,
    )
    monkeypatch.setattr(
        "bacup_lib.models.write_coverage_report",
        lambda *_a, **_k: None,
    )

    def fake_preflight(request, _runner):
        preflight_sources.append(Path(request.source_plugins[0]))
        preflight_excludes.append(request.options.exclude_signatures)
        assert not paths.output_root.exists()

    monkeypatch.setattr(unified, "_preflight_legacy_packs", fake_preflight)

    def fake_run_unified(request, _runner, **kwargs):
        captured_requests.append(request)
        unified_kwargs.append(kwargs)
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
            deploy=False,
            lod_mode="none",
            write_land_cache=False,
            deep_invariants=True,
        ),
        pair=get_pair("fnvfo3:fo4"),
        phases=PhaseSelection(lod_mode="none"),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
        build_option_overrides={
            "exclude_signatures": FNV_MVP_EXCLUDE_SIGNATURES
        },
    )

    assert result.exit_code == 0
    assert len(merge_calls) == 1
    merge_call = merge_calls[0]
    assert merge_call["primary_paths"] == [str(tmp_path / "FalloutNV.esm")]
    assert merge_call["grafted_paths"] == [str(tmp_path / "Fallout3.esm")]
    assert merge_call["source_strings_dir"] == str(source_strings_dir)
    assert Path(merge_call["output_path"]).name == "FNV_FO3_Merged.esm"
    assert Path(merge_call["output_path"]).parent.name == "merge"
    assert not Path(merge_call["output_path"]).is_relative_to(paths.diagnostics_root)
    assert Path(merge_call["report_path"]).parent == Path(
        merge_call["output_path"]
    ).parent
    assert merge_call["game"] == "fnv"
    assert preflight_sources == [Path(merge_call["output_path"])]
    assert preflight_excludes == [FNV_MVP_EXCLUDE_SIGNATURES]
    assert unified_kwargs[0]["record_preflight_complete"] is True
    request = captured_requests[0]
    merged_source = request.source_plugins[0]
    resolved_mod_root = unified._resolved_mod_root(request)
    final_build_destination = resolved_mod_root / merged_source.name
    merge_report = paths.diagnostics_root / "merge" / "merge_report.json"

    assert merged_source == paths.diagnostics_root / "merge" / "FNV_FO3_Merged.esm"
    assert resolved_mod_root == paths.output_root
    assert final_build_destination == paths.output_root / "FNV_FO3_Merged.esm"
    assert merged_source != final_build_destination
    assert merge_report.parent == merged_source.parent
    assert merge_report.is_file()
    assert merge_report.read_bytes() == b'{"native":"exact"}\n'
    assert request.legacy_pack_provenance_required is True
    assert request.legacy_pack_raw_source_counts is not None
    assert request.legacy_pack_raw_source_counts.fnv == 4_888
    assert request.legacy_pack_raw_source_counts.fo3 == 4_567
    assert request.legacy_pack_expected_counts is not None
    assert request.legacy_pack_expected_counts.fnv == 4_885
    assert request.legacy_pack_expected_counts.fo3 == 3_264
    assert request.legacy_pack_origins[0].source_game == "fo3"
    assert request.source_game == "fnv"
    assert request.target_game == "fo4"
    assert request.output_mod_name == "CustomMojave"
    assert request.options.exclude_signatures == FNV_MVP_EXCLUDE_SIGNATURES
    assert request.additional_source_asset_roots == (tmp_path / "fo3_extracted",)
    assert invariant_plugin_names == [["FNV_FO3_Merged.esm"]]


def test_fatal_pack_preflight_precedes_forced_cleanup_and_all_output_mutation(
    monkeypatch,
    tmp_path,
):
    from bacup_lib.family_map import UpgradePlan

    paths = _paths(tmp_path)
    paths.additional_source_asset_roots[0].mkdir(parents=True)
    paths.source_extracted_dir.mkdir(parents=True)
    paths.target_asset_cache_dir = tmp_path / "asset-cache"
    paths.output_root.mkdir(parents=True)
    paths.target_asset_cache_dir.mkdir(parents=True)
    paths.diagnostics_root.mkdir(parents=True)
    output_sentinel = paths.output_root / "keep.bin"
    cache_sentinel = paths.target_asset_cache_dir / "keep.bin"
    diagnostics_sentinel = paths.diagnostics_root / "keep.bin"
    output_sentinel.write_bytes(b"output-before")
    cache_sentinel.write_bytes(b"cache-before")
    diagnostics_sentinel.write_bytes(b"diagnostics-before")

    class _Native:
        def conversion_merge_sources(self, merge_options):
            Path(merge_options["output_path"]).write_bytes(b"TES4")
            return {
                "pack_origins": [],
                "pack_accounting": {
                    "raw_source": {"fnv": 4_888, "fo3": 4_567},
                    "final_survivors": {"fnv": 4_885, "fo3": 3_264},
                },
            }

    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: _Native(),
    )
    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(
        regen_pipeline,
        "_resolve_upgrade_plan",
        lambda *_a, **_k: UpgradePlan(
            phases=PhaseSelection(lod_mode="none"),
            regen_terrain=True,
            swap_labels=(),
            full_build=True,
            force_regen=True,
        ),
    )

    forbidden_calls: list[str] = []

    def forbidden(name):
        def fail(*_args, **_kwargs):
            forbidden_calls.append(name)
            raise AssertionError(f"{name} ran before fatal PACK preflight")

        return fail

    monkeypatch.setattr(
        unified,
        "_preflight_legacy_packs",
        lambda *_a, **_k: (_ for _ in ()).throw(
            RuntimeError("legacy PACK preflight blocked conversion")
        ),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_clean_forced_regen_output",
        forbidden("forced-clean"),
    )
    monkeypatch.setattr(
        "bacup_lib.target_assets.ensure_target_asset_catalog",
        forbidden("asset-catalog"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_check_land_cache",
        forbidden("land-cache-check"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_restore_land_cache_assets",
        forbidden("land-cache-restore"),
    )
    monkeypatch.setattr(unified, "run_unified", forbidden("unified-assets"))

    with pytest.raises(RuntimeError, match="legacy PACK preflight blocked"):
        regen_pipeline.run_full_regen(
            paths,
            RegenOptions(
                deploy=False,
                upgrade=True,
                re_use_land=True,
                lod_mode="none",
                memory_report=True,
            ),
            pair=get_pair("fnvfo3:fo4"),
            phases=PhaseSelection(lod_mode="none"),
            runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
        )

    assert forbidden_calls == []
    assert output_sentinel.read_bytes() == b"output-before"
    assert cache_sentinel.read_bytes() == b"cache-before"
    assert diagnostics_sentinel.read_bytes() == b"diagnostics-before"
    assert sorted(path.name for path in paths.output_root.iterdir()) == ["keep.bin"]
    assert sorted(path.name for path in paths.target_asset_cache_dir.iterdir()) == [
        "keep.bin"
    ]
    assert sorted(path.name for path in paths.diagnostics_root.iterdir()) == [
        "keep.bin"
    ]
    assert not list(paths.output_root.parent.glob("bacup-pack-preflight-*"))


def test_run_full_regen_record_failure_skips_validation_and_cache_snapshot(
    monkeypatch,
    tmp_path,
):
    paths = _paths(tmp_path)
    paths.additional_source_asset_roots[0].mkdir(parents=True)
    calls: list[str] = []

    class _Native:
        def conversion_merge_sources(self, merge_options):
            Path(merge_options["output_path"]).write_bytes(b"TES4")
            Path(merge_options["report_path"]).write_bytes(b"{}\n")
            return {
                "pack_origins": [],
                "pack_accounting": {
                    "raw_source": {"fnv": 4_888, "fo3": 4_567},
                    "final_survivors": {"fnv": 4_885, "fo3": 3_264},
                },
            }

    def forbidden(name):
        def fail(*_args, **_kwargs):
            calls.append(name)
            raise AssertionError(f"{name} ran after fatal record failure")

        return fail

    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: _Native(),
    )
    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(
        "bacup_lib.target_assets.ensure_target_asset_catalog",
        lambda *_a, **_k: None,
    )
    monkeypatch.setattr(unified, "_preflight_legacy_packs", lambda *_a, **_k: None)

    def fail_unified(_request, _runner, **kwargs):
        calls.append("unified")
        assert kwargs["serialize_tracks"] is True
        assert callable(kwargs["land_cache_hook"])
        raise RuntimeError("Translate Records failed")

    monkeypatch.setattr(unified, "run_unified", fail_unified)
    monkeypatch.setattr(
        regen_pipeline,
        "_write_conversion_reports",
        lambda *_a, **_k: calls.append("failure_reports"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_sanitize_existing_outputs",
        forbidden("sanitize"),
    )
    monkeypatch.setattr(
        "bacup_lib.models.write_coverage_report",
        forbidden("coverage"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_check_run_invariants",
        forbidden("deep_invariants"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_snapshot_land_cache",
        forbidden("land_cache_snapshot"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_snapshot_land_cache_from_run",
        forbidden("early_land_cache_snapshot"),
    )
    monkeypatch.setattr(
        regen_pipeline,
        "_deploy_post_steps",
        forbidden("deploy"),
    )

    with pytest.raises(RuntimeError, match="Translate Records failed"):
        regen_pipeline.run_full_regen(
            paths,
            RegenOptions(
                deploy=False,
                lod_mode="none",
                write_land_cache=True,
                validate_output=True,
                deep_invariants=True,
            ),
            pair=get_pair("fnvfo3:fo4"),
            phases=PhaseSelection(lod_mode="none"),
            runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
            build_option_overrides={"exclude_signatures": frozenset({"PACK"})},
        )

    assert calls == ["unified", "failure_reports"]


def test_run_full_regen_rejects_missing_additional_asset_root(tmp_path):
    paths = _paths(tmp_path)

    try:
        regen_pipeline.run_full_regen(
            paths,
            RegenOptions(deploy=False, lod_mode="none"),
            pair=get_pair("fnvfo3:fo4"),
            phases=PhaseSelection(lod_mode="none"),
            runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
        )
    except FileNotFoundError as exc:
        assert str(paths.additional_source_asset_roots[0]) in str(exc)
    else:
        raise AssertionError("missing grafted asset root was accepted")


def test_unified_record_track_uses_explicit_output_mod_root(tmp_path):
    merged = tmp_path / "FNV_FO3_Merged.esm"
    merged.write_bytes(b"TES4")
    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[merged],
        output_root=tmp_path / "mods",
        output_mod_name="CustomMojave",
        options=PluginPortOptions(),
    )
    driver = object.__new__(unified.UnifiedDriver)
    driver._req = request

    class StopAfterScaffold(RuntimeError):
        pass

    driver._record_runtime = SimpleNamespace(
        _plugin_name=lambda source_plugin: source_plugin.name,
        _build_context=lambda *_args: (_ for _ in ()).throw(StopAfterScaffold()),
    )
    runner = SimpleNamespace(emit_log=lambda *_args: None)

    try:
        driver._convert_record_track(merged, runner)
    except StopAfterScaffold:
        pass
    else:
        raise AssertionError("record-track scaffold did not stop at the test seam")

    assert (tmp_path / "mods" / "CustomMojave" / ".source_plugin").read_text(
        encoding="utf-8"
    ) == "FNV_FO3_Merged.esm"
    assert not (tmp_path / "mods" / "FNV_FO3_Merged").exists()


def test_run_unified_uses_explicit_output_mod_name_for_merged_source(
    monkeypatch, tmp_path
):
    captured_mod_roots: list[Path] = []

    monkeypatch.setattr(unified, "_preflight_legacy_packs", lambda *_args: None)

    class _Native:
        def sinks_create(self, _config):
            return 1

        def sinks_abort(self, _sink_id):
            pass

        def sinks_drop(self, _sink_id):
            pass

    monkeypatch.setattr(unified, "load_native_module", lambda: _Native())
    monkeypatch.setattr(
        unified.AssetWaveToggles,
        "from_options",
        staticmethod(lambda _options: unified.AssetWaveToggles()),
    )

    class _FakeDriver:
        def __init__(self, *_args, **_kwargs):
            self.record_runtime = SimpleNamespace(
                _aggregate_summary=object(), run_result=object()
            )
            self.signals = SimpleNamespace(
                record_done=SimpleNamespace(set=lambda: None)
            )
            self.defer_asset_a2_until_record_done = False
            self.ctx = None
            self.assets = []
            self.terrain_texture_jobs = []

        def run_record_track(self, _runner):
            pass

        def emit_complete(self, _runner):
            pass

    monkeypatch.setattr(unified, "UnifiedDriver", _FakeDriver)

    class _Mirror:
        def __init__(self, *_args, **_kwargs):
            pass

        def start(self):
            pass

        def finish(self, _status):
            pass

    monkeypatch.setattr(unified, "RunStateMirror", _Mirror)

    class _AssetRuns:
        def drop_all(self):
            pass

    def fake_run_asset_track(*_args, **kwargs):
        captured_mod_roots.append(Path(kwargs["mod_root"]))
        return _AssetRuns()

    monkeypatch.setattr(unified, "run_asset_track", fake_run_asset_track)
    monkeypatch.setattr(unified, "collect_cache_entries", lambda *_a, **_k: [])
    monkeypatch.setattr(unified, "write_cache_manifest", lambda *_a, **_k: None)

    merged = tmp_path / "FNV_FO3_Merged.esm"
    merged.write_bytes(b"TES4")
    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[merged],
        output_root=tmp_path / "mods",
        output_mod_name="MojaveCapital",
        options=PluginPortOptions(),
    )

    runner = SimpleNamespace(
        emit_log=lambda *_a, **_k: None,
        emit_phase_start=lambda *_a, **_k: None,
        emit_phase_complete=lambda *_a, **_k: None,
    )
    unified.run_unified(
        request,
        runner,
        enable_ba2=False,
        serialize_tracks=True,
    )

    expected = tmp_path / "mods" / "MojaveCapital"
    assert captured_mod_roots == [expected]
    assert expected.is_dir()
    assert not (tmp_path / "mods" / "FNV_FO3_Merged").exists()
