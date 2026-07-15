from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

from bacup_lib import PhaseSelection, regen_pipeline
from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.regen_pipeline import RegenOptions, RegenPaths
from bacup_lib.source_pairs import get_pair
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
    paths.additional_source_asset_roots[0].mkdir(parents=True)
    merge_calls: list[dict] = []
    captured_requests: list[PluginPortRequest] = []
    invariant_plugin_names: list[list[str]] = []

    class _Native:
        def conversion_merge_sources(self, options):
            merge_calls.append(options)
            Path(options["output_path"]).write_bytes(b"TES4")
            return {"copied": 1, "deduped": 0}

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

    def fake_run_unified(request, _runner, **_kwargs):
        captured_requests.append(request)
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
    )

    assert result.exit_code == 0
    assert merge_calls == [
        {
            "primary_paths": [str(tmp_path / "FalloutNV.esm")],
            "grafted_paths": [str(tmp_path / "Fallout3.esm")],
            "output_path": str(
                paths.diagnostics_root / "merge" / "FNV_FO3_Merged.esm"
            ),
            "report_path": str(paths.diagnostics_root / "merge" / "merge_report.json"),
            "game": "fnv",
        }
    ]
    request = captured_requests[0]
    merged_source = request.source_plugins[0]
    resolved_mod_root = unified._resolved_mod_root(request)
    final_build_destination = resolved_mod_root / merged_source.name
    merge_report = Path(merge_calls[0]["report_path"])

    assert merged_source == paths.diagnostics_root / "merge" / "FNV_FO3_Merged.esm"
    assert resolved_mod_root == paths.output_root
    assert final_build_destination == paths.output_root / "FNV_FO3_Merged.esm"
    assert merged_source != final_build_destination
    assert merge_report.parent == merged_source.parent
    assert request.source_game == "fnv"
    assert request.target_game == "fo4"
    assert request.output_mod_name == "CustomMojave"
    assert request.additional_source_asset_roots == (tmp_path / "fo3_extracted",)
    assert invariant_plugin_names == [["FNV_FO3_Merged.esm"]]


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

    unified.run_unified(
        request,
        SimpleNamespace(emit_log=lambda *_a, **_k: None),
        enable_ba2=False,
        serialize_tracks=True,
    )

    expected = tmp_path / "mods" / "MojaveCapital"
    assert captured_mod_roots == [expected]
    assert expected.is_dir()
    assert not (tmp_path / "mods" / "FNV_FO3_Merged").exists()
