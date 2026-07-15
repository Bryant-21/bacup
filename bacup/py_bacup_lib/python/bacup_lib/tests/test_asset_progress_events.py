import threading

from bacup_lib.workflows.unified import MultiRunDrainer


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

