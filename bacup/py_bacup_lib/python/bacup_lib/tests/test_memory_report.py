import json
from pathlib import Path

from bacup_lib.memory_report import MemoryReport


class FakeClock:
    def __init__(self) -> None:
        self.value = 0.0

    def __call__(self) -> float:
        return self.value


def test_memory_report_writes_peak_stage_summary(tmp_path: Path) -> None:
    clock = FakeClock()
    rss_values = iter(
        [
            1 * 1024**3,
            3 * 1024**3,
            2 * 1024**3,
            2 * 1024**3,
        ]
    )

    def snapshot() -> dict:
        rss = next(rss_values)
        return {
            "available": True,
            "process_rss_bytes": rss,
            "process_vms_bytes": rss,
            "child_rss_bytes": 0,
            "child_vms_bytes": 0,
            "child_count": 0,
            "total_rss_bytes": rss,
            "total_vms_bytes": rss,
        }

    report = MemoryReport(snapshot_provider=snapshot, clock=clock)

    report.record_event("before")
    with report.scoped_stage("phase:Translate Records (Rust)", phase=2):
        clock.value = 1.0
        report.record_event("during")

    json_path = tmp_path / "conversion_memory.json"
    md_path = tmp_path / "conversion_memory.md"
    report.write_json(json_path, total_elapsed_seconds=2.0)
    report.write_markdown(md_path, total_elapsed_seconds=2.0)

    payload = json.loads(json_path.read_text(encoding="utf-8"))
    assert payload["summary"]["peak"]["total_rss_gb"] == 3.0
    assert payload["summary"]["peak"]["stage"] == "phase:Translate Records (Rust)"
    assert "phase:Translate Records (Rust)" in md_path.read_text(encoding="utf-8")
