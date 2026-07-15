import json
from pathlib import Path

from bacup_lib.timing_report import TimingReport


def test_timing_report_writes_json(tmp_path: Path) -> None:
    report = TimingReport()

    report.record("apply_fixups", 1.23456, changed=7, path=tmp_path / "out")
    report.write_json(tmp_path / "conversion_timing.json", total_elapsed_seconds=2.0)

    text = (tmp_path / "conversion_timing.json").read_text(encoding="utf-8")
    payload = json.loads(text)
    assert '"name": "apply_fixups"' in text
    assert '"changed": 7' in text
    assert '"total_elapsed_seconds": 2.0' in text
    assert payload["events"][0]["path"] == str(tmp_path / "out")


def test_timing_report_keeps_event_order() -> None:
    report = TimingReport()

    report.record("setup", 0.1)
    report.record("pack", 0.2)

    assert [event["name"] for event in report.to_dict()["events"]] == ["setup", "pack"]


def test_timing_report_mirrors_events_to_memory_report() -> None:
    class FakeMemoryReport:
        def __init__(self) -> None:
            self.events = []

        def record_timing_event(self, name: str, event: dict) -> None:
            self.events.append((name, event))

    report = TimingReport()
    memory_report = FakeMemoryReport()
    report.memory_report = memory_report

    report.record("phase", 0.5, phase_name="Build ESP")

    assert memory_report.events == [
        (
            "phase",
            {
                "name": "phase",
                "elapsed_seconds": 0.5,
                "phase_name": "Build ESP",
            },
        )
    ]
