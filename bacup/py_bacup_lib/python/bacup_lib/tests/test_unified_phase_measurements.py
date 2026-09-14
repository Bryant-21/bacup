from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.timing_report import TimingReport
from bacup_lib.workflows import unified


def _runtime(tmp_path: Path) -> unified._UnifiedRecordRuntime:
    request = PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[],
        output_root=tmp_path / "out",
        source_data_dir=tmp_path / "source",
        target_extracted_dir=tmp_path / "target-extracted",
        target_data_dir=tmp_path / "target-data",
        options=PluginPortOptions(convert_scripts=True),
    )
    return unified._UnifiedRecordRuntime(request)


def _ctx(tmp_path: Path) -> SimpleNamespace:
    return SimpleNamespace(
        _rust_conversion_run=SimpleNamespace(id=7),
        mod_path=tmp_path / "mod",
        diagnostics_root=tmp_path / "diagnostics",
        summary=SimpleNamespace(scripts_flagged=0),
        timing_report=TimingReport(),
        target_asset_store=None,
    )


def _runner() -> SimpleNamespace:
    logs: list[tuple[str, str]] = []
    return SimpleNamespace(
        logs=logs,
        emit_log=lambda level, message: logs.append((level, message)),
        emit_complete=lambda *_args: None,
    )


def _event_names(ctx: SimpleNamespace) -> list[str]:
    return [event["name"] for event in ctx.timing_report.events]


def test_convert_scripts_emits_ordered_child_measurements(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _ctx(tmp_path)
    runner = _runner()
    ref = unified._ScriptReference(
        script_name="MissingScript",
        variable_name=None,
        form_key="000001:Source.esm",
        form_id=1,
        record_sig="REFR",
        editor_id="TestRef",
        kind="vmad",
    )
    monkeypatch.setattr(runtime, "_collect_script_references", lambda *_: ([ref], 1))
    monkeypatch.setattr(unified, "_build_target_pex_index", lambda **_: {})
    monkeypatch.setattr(unified, "_build_pex_index", lambda _: {})
    monkeypatch.setattr(unified, "_script_addition_sources", lambda *_: {})
    monkeypatch.setattr(unified, "_script_patch_inventory", lambda: {})
    monkeypatch.setattr(
        unified, "_extend_script_names_with_ancestor_closure", lambda *_, **__: None
    )
    monkeypatch.setattr(
        runtime, "_decompile_source_scripts_for_fo4", lambda *_, **__: []
    )
    monkeypatch.setattr(
        runtime, "_compile_decompiled_scripts_for_fo4", lambda *_, **__: []
    )
    monkeypatch.setattr(
        runtime,
        "_reconcile_script_references",
        lambda *_, **__: (0, 0, 0, 0, [], [], [], [], 0, 0, 0),
    )
    monkeypatch.setattr(runtime, "_write_script_port_report", lambda *_, **__: None)

    runtime._run_convert_scripts_phase(ctx, runner)

    expected = [
        "convert_scripts_collect_references",
        "convert_scripts_target_index",
        "convert_scripts_authoritative_target_index",
        "convert_scripts_source_index",
        "convert_scripts_patch_addition_discovery",
        "convert_scripts_resolution",
        "convert_scripts_decompile_port",
        "convert_scripts_compile",
        "convert_scripts_reconciliation",
        "convert_scripts_report",
        "convert_scripts",
    ]
    assert _event_names(ctx) == expected
    for event in ctx.timing_report.events[:-1]:
        assert event["scope"] == "detail"
        assert event["parent"] == "Convert Scripts"
        assert event["offset_seconds"] >= 0
        assert event["elapsed_seconds"] >= 0


def test_convert_scripts_early_return_preserves_log_and_collect_measurement(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    runtime = _runtime(tmp_path)
    runtime._req.source_game = "skyrimse"
    ctx = _ctx(tmp_path)
    runner = _runner()
    monkeypatch.setattr(runtime, "_collect_script_references", lambda *_: ([], 0))

    runtime._run_convert_scripts_phase(ctx, runner)

    assert _event_names(ctx) == ["convert_scripts_collect_references"]
    assert runner.logs[-1] == ("INFO", "[Scripts] no Papyrus script references found")


def test_quest_inventory_emits_ordered_measurements_and_unchanged_report(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _ctx(tmp_path)
    runner = _runner()
    payload = {
        "coverage_complete": True,
        "summary": {
            "native": 1,
            "adapted": 2,
            "explicitly_unsupported": 3,
            "unclassified": 0,
        },
    }
    native = SimpleNamespace(
        conversion_run_quest_runtime_inventory_json=lambda *args: json.dumps(payload)
    )
    monkeypatch.setattr(unified, "load_native_module", lambda: native)
    monkeypatch.setattr(unified, "_script_addition_sources", lambda *_: {})

    runtime._run_quest_runtime_inventory_phase(ctx, runner)

    assert _event_names(ctx) == [
        "quest_runtime_inventory_input_preparation",
        "quest_runtime_inventory_native",
        "quest_runtime_inventory_report_write",
        "quest_runtime_inventory_json_decode",
        "quest_runtime_inventory_summary",
    ]
    for event in ctx.timing_report.events:
        assert event["scope"] == "detail"
        assert event["parent"] == "Inventory Quest Runtime Routes"
        assert event["offset_seconds"] >= 0
    report_path = ctx.diagnostics_root / "quest_runtime_inventory.json"
    assert json.loads(report_path.read_text(encoding="utf-8")) == payload
    assert runner.logs[-1][0] == "INFO"


def test_quest_inventory_native_error_preserves_warning_and_return(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _ctx(tmp_path)
    runner = _runner()

    def fail(*_args):
        raise RuntimeError("inventory failed")

    monkeypatch.setattr(
        unified,
        "load_native_module",
        lambda: SimpleNamespace(conversion_run_quest_runtime_inventory_json=fail),
    )
    monkeypatch.setattr(unified, "_script_addition_sources", lambda *_: {})

    runtime._run_quest_runtime_inventory_phase(ctx, runner)

    assert _event_names(ctx) == [
        "quest_runtime_inventory_input_preparation",
        "quest_runtime_inventory_native",
    ]
    assert ctx.timing_report.events[-1]["status"] == "failed"
    assert ctx.timing_report.events[-1]["error_type"] == "RuntimeError"
    assert runner.logs[-1] == (
        "WARN",
        "[Quest Runtime] native inventory could not complete: inventory failed",
    )
    assert not (ctx.diagnostics_root / "quest_runtime_inventory.json").exists()


def test_quest_inventory_decode_error_records_failed_stage_after_report_write(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _ctx(tmp_path)
    runner = _runner()
    native = SimpleNamespace(
        conversion_run_quest_runtime_inventory_json=lambda *_args: "not-json"
    )
    monkeypatch.setattr(unified, "load_native_module", lambda: native)
    monkeypatch.setattr(unified, "_script_addition_sources", lambda *_: {})

    runtime._run_quest_runtime_inventory_phase(ctx, runner)

    assert _event_names(ctx) == [
        "quest_runtime_inventory_input_preparation",
        "quest_runtime_inventory_native",
        "quest_runtime_inventory_report_write",
        "quest_runtime_inventory_json_decode",
    ]
    assert ctx.timing_report.events[-1]["status"] == "failed"
    assert (ctx.diagnostics_root / "quest_runtime_inventory.json").read_text(
        encoding="utf-8"
    ) == "not-json\n"
    assert runner.logs[-1][0] == "WARN"


def test_run_unified_emits_setup_completion_and_cleanup_measurements(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    runtime = _runtime(tmp_path)
    request = runtime._req
    request.source_plugins = [tmp_path / "source.esm"]
    request.timing_report = TimingReport()
    runner = _runner()
    calls: list[str] = []
    native = SimpleNamespace(
        sinks_create=lambda _payload: 41,
        sinks_drop=lambda sink_id: calls.append(f"sink_drop:{sink_id}"),
    )

    class Mirror:
        def __init__(self, *_args):
            pass

        def start(self):
            calls.append("mirror_start")

        def finish(self, status):
            calls.append(f"mirror_finish:{status}")

    class AssetRuns:
        nifs = None

        def drop_all(self):
            calls.append("asset_drop")

    monkeypatch.setattr(unified, "load_native_module", lambda: native)
    monkeypatch.setattr(unified, "_preflight_legacy_packs", lambda *_: None)
    monkeypatch.setattr(unified, "RunStateMirror", Mirror)
    monkeypatch.setattr(unified.UnifiedDriver, "run_record_track", lambda *_: None)
    monkeypatch.setattr(
        unified, "run_asset_track", lambda *_args, **_kwargs: AssetRuns()
    )
    monkeypatch.setattr(unified, "_run_post_phase", lambda *_args, **_kwargs: None)

    unified.run_unified(request, runner, enable_ba2=False, serialize_tracks=True)

    names = [event["name"] for event in request.timing_report.events]
    assert names == [
        "unified_initial_preflight",
        "unified_sink_creation",
        "unified_completion_emit_and_mirror_finish",
        "unified_final_asset_runs_drop",
        "unified_final_sink_drop",
    ]
    assert calls == ["mirror_start", "mirror_finish:done", "asset_drop", "sink_drop:41"]
    for event in request.timing_report.events:
        assert event["scope"] == "detail"
        assert event["parent"] == "run_unified"
        assert event["offset_seconds"] >= 0
