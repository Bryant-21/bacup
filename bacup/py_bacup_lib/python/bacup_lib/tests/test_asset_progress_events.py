import threading
from types import SimpleNamespace

import pytest

from bacup_lib.models import ConversionSummary
from bacup_lib.native_runtime import _ConversionNativeProxy
from bacup_lib.timing_report import TimingReport
from bacup_lib.workflows.unified import (
    MultiRunDrainer, _merge_wave_report_into_summary, _run_post_phase,
    _has_havok_postprocess,
)


class _Runner:
    def __init__(self) -> None:
        self.events: list[tuple[str, object]] = []
        self.logs: list[tuple[str, str]] = []

    def emit_phase_start(self, progress) -> None:
        self.events.append(("start", progress))

    def emit_item_progress(self, progress) -> None:
        self.events.append(("item", progress))

    def emit_phase_complete(self, progress) -> None:
        self.events.append(("complete", progress))

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))


@pytest.mark.parametrize("event_order", ["missing", "before", "after"])
def test_durable_phase_reports_merge_once_with_planning_failures(event_order):
    drainer = MultiRunDrainer([], _Runner())
    summary = ConversionSummary(nifs_failed=670)
    native_report = {"assets_written": 52610, "items_failed": 4, "warnings": 5000}
    event = {"kind": "completed", "phase": "convert_nifs_v2", "report": native_report}
    report = {
        "stages": [("convert_nifs_v2", 52610, 4, 5000, 678334)],
        "phase_reports": {"convert_nifs_v2": native_report},
    }
    if event_order == "before":
        drainer._dispatch(event)
    drainer.reconcile_pipeline_report(report)
    for phase, values in drainer.drain_completed():
        _merge_wave_report_into_summary(summary, phase, values)
    if event_order == "after":
        drainer._dispatch(event)
    drainer.reconcile_pipeline_report(report)
    assert drainer.drain_completed() == []
    assert summary.nifs_converted == 52610
    assert summary.nifs_failed == 674


def test_repeated_phase_in_later_wave_counts_new_outputs():
    drainer = MultiRunDrainer([], _Runner())
    summary = ConversionSummary()
    for written in [100, 3]:
        drainer.begin_wave(["convert_nifs_v2"])
        drainer.reconcile_pipeline_report({
            "stages": [("convert_nifs_v2", written, 0, 0, 100)],
            "phase_reports": {"convert_nifs_v2": {"assets_written": written}},
        })
        for phase, report in drainer.drain_completed():
            _merge_wave_report_into_summary(summary, phase, report)
    assert summary.nifs_converted == 103


def test_pipeline_proxy_decodes_full_phase_report():
    raw_phase = (0, 0, 9, 22744, 80, 13297, 1, 0, 0)
    raw = SimpleNamespace(conversion_pipeline_run=lambda _plan: (
        [("convert_materials_v2", 22744, 1, 80, 13297, raw_phase)], 13300, []
    ))
    report = _ConversionNativeProxy(raw).conversion_pipeline_run("{}")
    assert report["stages"] == [("convert_materials_v2", 22744, 1, 80, 13297)]
    assert report["phase_reports"]["convert_materials_v2"]["assets_written"] == 22744
    assert report["phase_reports"]["convert_materials_v2"]["records_dropped"] == 9


def test_installed_pipeline_returns_full_report_for_written_asset(tmp_path):
    import json

    from bacup_lib.native_runtime import load_native_module
    from bacup_lib.run import ConversionRun

    source = tmp_path / "source.wav"
    source.write_bytes(b"RIFFfixture")
    with ConversionRun.create_new(
        "fo76", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
    ) as run:
        report = load_native_module().conversion_pipeline_run(json.dumps({
            "events_run_id": run.id,
            "stages": [{
                "phase": "copy_sounds", "run_id": run.id,
                "mod_path": str(tmp_path), "source_extracted_dir": str(tmp_path),
                "params": {"sound_paths": [{
                    "source_path": "FX/test.wav", "resolved_path": str(source),
                }]},
            }],
        }))
    assert (tmp_path / "data/Sound/FX/test.wav").read_bytes() == b"RIFFfixture"
    assert report["stages"][0][1:4] == (1, 0, 0)
    assert report["phase_reports"]["copy_sounds"]["assets_written"] == 1
    assert report["phase_reports"]["copy_sounds"]["items_failed"] == 0


@pytest.mark.parametrize("source,target,havok,corpus,expected", [
    ("fo76", "fo4", True, None, True),
    ("fo76", "fo4", False, None, False),
    ("fo76", "fo4", True, {}, False),
    ("skyrimse", "fo4", True, None, False),
    ("fnv", "fo4", True, None, False),
    ("starfield", "fo4", True, None, False),
    ("fo4", "starfield", True, None, False),
])
def test_only_defer_repairs_when_later_havok_phase_exists(source, target, havok, corpus, expected):
    ctx = SimpleNamespace(source_game=source, target_game=target,
                          mvp_creature_corpus_policy=corpus)
    assert _has_havok_postprocess(ctx, SimpleNamespace(convert_havok=havok)) is expected


def test_multi_run_drainer_emits_structured_asset_stage_progress():
    runner = _Runner()
    drainer = MultiRunDrainer([], runner)

    drainer._dispatch({"kind": "stage_started", "stage": "convert_nifs_v2"})
    drainer._dispatch(
        {
            "kind": "progress",
            "phase": "convert_nifs_v2",
            "current": 4,
            "total": 10,
            "item": "Meshes/Weapons/test.nif",
        }
    )
    drainer._dispatch(
        {
            "kind": "stage_completed",
            "stage": "convert_nifs_v2",
            "items_done": 10,
            "elapsed_ms": 1250,
        }
    )

    start = runner.events[0][1]
    item = runner.events[1][1]
    complete = runner.events[2][1]

    assert start.phase_name == "convert_nifs_v2"
    assert start.status == "running"
    assert item.completed_items == 4
    assert item.total_items == 10
    assert item.current_item == "Meshes/Weapons/test.nif"
    assert complete.status == "completed"
    assert complete.completed_items == 10
    assert complete.elapsed_seconds == 1.25


def test_multi_run_drainer_reconciles_dropped_stage_completion_from_report():
    runner = _Runner()
    drainer = MultiRunDrainer([], runner)

    drainer._dispatch(
        {"kind": "stage_started", "stage": "convert_nifs_v2"}
    )
    drainer._dispatch(
        {"kind": "stage_started", "stage": "convert_materials_v2"}
    )
    report = {
        "stages": [
            ("convert_nifs_v2", 18_240, 7, 7, 90_000),
            ("convert_materials_v2", 29_471, 0, 0, 12_500),
        ],
        "elapsed_ms": 12_500,
        "counters": {},
    }
    drainer.reconcile_pipeline_report(report)
    drainer.reconcile_pipeline_report(report)

    assert [event_type for event_type, _progress in runner.events] == [
        "start",
        "start",
        "complete",
        "complete",
    ]
    nifs_complete = runner.events[-2][1]
    materials_complete = runner.events[-1][1]
    assert nifs_complete.phase_name == "convert_nifs_v2"
    assert nifs_complete.completed_items == 18_247
    assert nifs_complete.total_items == 18_247
    assert nifs_complete.elapsed_seconds == 90.0
    assert materials_complete.phase_name == "convert_materials_v2"
    assert materials_complete.completed_items == 29_471
    assert materials_complete.total_items == 29_471
    assert materials_complete.elapsed_seconds == 12.5


def test_multi_run_drainer_serializes_final_reconciliation_after_progress_dispatch():
    runner = _Runner()
    drainer = MultiRunDrainer([1], runner)
    progress_dispatch_started = threading.Event()
    release_progress_dispatch = threading.Event()
    second_drain_started = threading.Event()
    native_lock = threading.Lock()
    drain_calls = 0

    class _Native:
        @staticmethod
        def conversion_run_drain_events(_run_id, _limit):
            nonlocal drain_calls
            with native_lock:
                drain_calls += 1
                if drain_calls == 1:
                    return [
                        {
                            "kind": "progress",
                            "phase": "convert_materials_v2",
                            "current": 29_461,
                            "total": 29_471,
                        }
                    ]
                second_drain_started.set()
                return []

    dispatch = drainer._dispatch

    def delayed_dispatch(event):
        progress_dispatch_started.set()
        assert release_progress_dispatch.wait(timeout=2.0)
        dispatch(event)

    drainer._dispatch = delayed_dispatch
    background_drain = threading.Thread(target=drainer._drain_once, args=(_Native(),))
    background_drain.start()
    assert progress_dispatch_started.wait(timeout=2.0)

    def finalize_wave():
        drainer._drain_once(_Native())
        drainer.reconcile_pipeline_report(
            {
                "stages": [
                    ("convert_materials_v2", 29_471, 0, 0, 82_500),
                ]
            }
        )

    final_drain = threading.Thread(target=finalize_wave)
    final_drain.start()
    second_drain_started.wait(timeout=0.1)
    release_progress_dispatch.set()
    background_drain.join(timeout=2.0)
    final_drain.join(timeout=2.0)

    assert not background_drain.is_alive()
    assert not final_drain.is_alive()
    assert [progress.status for _kind, progress in runner.events] == [
        "running",
        "completed",
    ]


def test_multi_run_drainer_ignores_progress_after_stage_completion():
    runner = _Runner()
    drainer = MultiRunDrainer([], runner)
    drainer.reconcile_pipeline_report(
        {"stages": [("convert_materials_v2", 29_471, 0, 0, 82_500)]}
    )

    drainer._dispatch(
        {
            "kind": "progress",
            "phase": "convert_materials_v2",
            "current": 29_461,
            "total": 29_471,
        }
    )

    assert [progress.status for _kind, progress in runner.events] == ["completed"]
    assert drainer.stage_counters["convert_materials_v2"] == 29_471


@pytest.mark.parametrize("completion_source", ["event", "report"])
def test_phase_report_does_not_hide_authoritative_stage_timing(completion_source):
    runner = _Runner()
    timing = TimingReport()
    drainer = MultiRunDrainer([], runner, timing_report=timing)
    drainer._dispatch({"kind": "stage_started", "stage": "convert_nifs_v2"})
    drainer._dispatch({
        "kind": "completed", "phase": "convert_nifs_v2",
        "report": {"assets_written": 8, "warnings": 5, "items_failed": 2, "elapsed_ms": 0},
    })
    assert len(runner.events) == 1
    if completion_source == "event":
        drainer._dispatch({
            "kind": "stage_completed", "stage": "convert_nifs_v2",
            "items_done": 8, "items_failed": 2, "elapsed_ms": 2500,
        })
    drainer.reconcile_pipeline_report({"stages": [("convert_nifs_v2", 8, 2, 5, 2500)]})
    assert len(runner.events) == 2
    assert runner.events[-1][1].completed_items == 10
    assert runner.events[-1][1].elapsed_seconds == 2.5
    assert len(timing.events) == 1
    assert timing.events[0]["elapsed_seconds"] == 2.5
    assert timing.events[0]["items_failed"] == 2
    assert len(drainer.drain_completed()) == 1


@pytest.mark.parametrize("fails", [False, True])
def test_post_phase_records_elapsed_time_on_success_and_failure(fails):
    timing = TimingReport()
    runner = _Runner()

    def body(progress):
        if fails:
            raise RuntimeError("fixture failure")

    if fails:
        with pytest.raises(RuntimeError, match="fixture failure"):
            _run_post_phase("Generate LOD", body, runner, timing_report=timing)
    else:
        _run_post_phase("Generate LOD", body, runner, timing_report=timing)
    assert len(timing.events) == 1
    event = timing.events[0]
    assert event["name"] == "Generate LOD"
    assert event["track"] == "post"
    assert event["status"] == ("error" if fails else "completed")
    assert event["elapsed_seconds"] >= 0


def test_havok_postprocessing_does_not_count_converted_assets_twice():
    summary = ConversionSummary()
    _merge_wave_report_into_summary(summary, "convert_havok", {
        "assets_written": 8, "warnings": 5, "items_failed": 2, "records_dropped": 3,
    })
    _merge_wave_report_into_summary(summary, "postprocess_havok_assets", {
        "assets_written": 20, "warnings": 4, "items_failed": 0,
    })
    assert summary.havok_total == 13
    assert summary.havok_converted == 8
    assert summary.havok_failed == 2


def test_failed_asset_stage_is_in_timing_report():
    timing = TimingReport()
    drainer = MultiRunDrainer([], _Runner(), timing_report=timing)
    drainer._dispatch({"kind": "stage_started", "stage": "convert_nifs_v2"})
    drainer._dispatch({"kind": "stage_failed", "stage": "convert_nifs_v2", "message": "failure"})
    assert len(timing.events) == 1
    assert timing.events[0]["status"] == "error"
    assert timing.events[0]["error"] == "failure"

