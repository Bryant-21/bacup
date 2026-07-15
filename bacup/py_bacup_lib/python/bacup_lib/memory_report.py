from __future__ import annotations

import contextlib
import json
import logging
import os
import threading
import time
from pathlib import Path
from typing import Any, Callable, Iterator

_BYTES_PER_GB = 1024**3
_LOG = logging.getLogger("conversion.memory")


def _json_safe(value: Any) -> Any:
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_safe(item) for item in value]
    return str(value)


def _gb(value: int | float | None) -> float | None:
    if value is None:
        return None
    return round(float(value) / _BYTES_PER_GB, 6)


def _build_psutil_snapshot_provider() -> Callable[[], dict[str, Any]] | None:
    try:
        import psutil
    except ImportError:
        return None

    process = psutil.Process(os.getpid())

    def snapshot() -> dict[str, Any]:
        info = process.memory_info()
        child_rss_bytes = 0
        child_vms_bytes = 0
        child_count = 0
        for child in process.children(recursive=True):
            try:
                child_info = child.memory_info()
            except (psutil.AccessDenied, psutil.NoSuchProcess):
                continue
            child_count += 1
            child_rss_bytes += int(child_info.rss)
            child_vms_bytes += int(child_info.vms)

        system = psutil.virtual_memory()
        process_rss_bytes = int(info.rss)
        process_vms_bytes = int(info.vms)
        total_rss_bytes = process_rss_bytes + child_rss_bytes
        total_vms_bytes = process_vms_bytes + child_vms_bytes
        return {
            "available": True,
            "process_rss_bytes": process_rss_bytes,
            "process_vms_bytes": process_vms_bytes,
            "child_rss_bytes": child_rss_bytes,
            "child_vms_bytes": child_vms_bytes,
            "child_count": child_count,
            "total_rss_bytes": total_rss_bytes,
            "total_vms_bytes": total_vms_bytes,
            "system_available_bytes": int(system.available),
            "system_used_percent": float(system.percent),
        }

    return snapshot


class MemoryReport:
    def __init__(
        self,
        *,
        sample_interval_seconds: float = 2.0,
        snapshot_provider: Callable[[], dict[str, Any]] | None = None,
        clock: Callable[[], float] = time.perf_counter,
        logger: logging.Logger | None = None,
    ) -> None:
        self.sample_interval_seconds = max(float(sample_interval_seconds), 0.1)
        self.started_at = clock()
        self._clock = clock
        self._logger = logger if logger is not None else _LOG
        self._snapshot_provider = (
            snapshot_provider
            if snapshot_provider is not None
            else _build_psutil_snapshot_provider()
        )
        self._samples: list[dict[str, Any]] = []
        self._stage_stack: list[dict[str, Any]] = []
        self._lock = threading.RLock()
        self._stop_event = threading.Event()
        self._thread: threading.Thread | None = None
        self._started = False

    def start(self) -> None:
        if self._started:
            return
        self._started = True
        self.record_event("memory_report_start")
        if self._snapshot_provider is None:
            self._logger.warning("memory sampling unavailable: psutil is not installed")
            return
        self._stop_event.clear()
        self._thread = threading.Thread(
            target=self._sample_periodically,
            name="conversion-memory-report",
            daemon=True,
        )
        self._thread.start()

    def stop(self) -> None:
        if not self._started:
            return
        self._stop_event.set()
        if self._thread is not None:
            self._thread.join(timeout=self.sample_interval_seconds + 1.0)
            self._thread = None
        self.record_event("memory_report_stop")
        self._started = False

    def record_sample(self) -> None:
        self._record("sample")

    def record_event(self, event: str, **fields: Any) -> None:
        self._record("event", event=event, **fields)

    def record_timing_event(self, name: str, event_fields: dict[str, Any]) -> None:
        fields = dict(event_fields)
        fields.pop("name", None)
        fields["timing_name"] = name
        self.record_event(f"timing:{name}", **fields)

    def mark(self, label: str) -> None:
        """Record an instant RSS snapshot with a named label.

        Appends a ``kind="mark"`` entry to the sample list so callers can
        correlate peak RSS at specific pipeline boundaries (e.g. after source
        open, after repair, before build_esp).
        """
        snap = self._snapshot()
        rss_bytes = snap.get("rss_bytes", snap.get("process_rss_bytes", 0))
        sample: dict[str, Any] = {
            "kind": "mark",
            "label": label,
            "elapsed_seconds": round(float(self._clock() - self.started_at), 6),
            "rss_bytes": rss_bytes,
        }
        sample.update(snap)
        _add_gb_fields(sample)
        with self._lock:
            self._samples.append(sample)
        self._logger.info(
            "memory mark=%s rss_bytes=%d",
            label,
            rss_bytes,
        )

    def peak_rss_gb(self) -> float:
        """Return the global peak total RSS in GB across all samples."""
        return self.summary().get("peak", {}).get("total_rss_gb", 0.0) or 0.0

    @contextlib.contextmanager
    def scoped_stage(self, stage: str, **fields: Any) -> Iterator[None]:
        self._push_stage(stage, fields)
        self.record_event("stage_start", stage_name=stage, **fields)
        try:
            yield
        finally:
            self.record_event("stage_end", stage_name=stage, **fields)
            self._pop_stage()

    def to_dict(self, *, total_elapsed_seconds: float | None = None) -> dict[str, Any]:
        samples = self.samples()
        mark_events = [s for s in samples if s.get("kind") == "mark"]
        payload: dict[str, Any] = {
            "sample_interval_seconds": self.sample_interval_seconds,
            "summary": self.summary(total_elapsed_seconds=total_elapsed_seconds),
            "samples": samples,
            "events": mark_events,
        }
        if total_elapsed_seconds is not None:
            payload["total_elapsed_seconds"] = round(float(total_elapsed_seconds), 6)
        if self._snapshot_provider is None:
            payload["available"] = False
            payload["unavailable_reason"] = "psutil is not installed"
        else:
            payload["available"] = True
        return payload

    def write_json(
        self,
        path: Path,
        *,
        total_elapsed_seconds: float | None = None,
    ) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(
                self.to_dict(total_elapsed_seconds=total_elapsed_seconds),
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )

    def write_markdown(
        self,
        path: Path,
        *,
        total_elapsed_seconds: float | None = None,
    ) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        summary = self.summary(total_elapsed_seconds=total_elapsed_seconds)
        lines = [
            "# Conversion Memory Report",
            "",
            "RSS values are process working-set memory, including child processes.",
            "",
            "## Summary",
            "",
            f"- sample interval: {self.sample_interval_seconds:.3f}s",
            f"- samples: {summary['sample_count']}",
        ]
        if total_elapsed_seconds is not None:
            lines.append(f"- elapsed: {float(total_elapsed_seconds):.3f}s")
        if not summary.get("available", True):
            lines.append("- status: memory sampling unavailable")
        peak = summary.get("peak")
        if isinstance(peak, dict):
            lines.extend(
                [
                    f"- peak total RSS: {float(peak['total_rss_gb']):.3f} GB",
                    f"- peak elapsed: {float(peak['elapsed_seconds']):.3f}s",
                    f"- peak stage: `{peak.get('stage', '')}`",
                    f"- peak event: `{peak.get('event', '')}`",
                ]
            )

        stage_peaks = summary.get("stage_peaks", [])
        if stage_peaks:
            lines.extend(
                [
                    "",
                    "## Stage Peaks",
                    "",
                    "| Stage | Peak RSS GB | Elapsed | Event |",
                    "| --- | ---: | ---: | --- |",
                ]
            )
            for row in stage_peaks:
                lines.append(
                    "| "
                    f"{_md_cell(row.get('stage', ''))} | "
                    f"{float(row.get('total_rss_gb', 0.0)):.3f} | "
                    f"{float(row.get('elapsed_seconds', 0.0)):.3f}s | "
                    f"{_md_cell(row.get('event', ''))} |"
                )

        event_samples = [
            sample for sample in self.samples() if sample.get("kind") == "event"
        ]
        if event_samples:
            lines.extend(
                [
                    "",
                    "## Event Samples",
                    "",
                    "| Elapsed | Event | Stage | Total RSS GB | Process RSS GB | Child RSS GB |",
                    "| ---: | --- | --- | ---: | ---: | ---: |",
                ]
            )
            for sample in event_samples:
                lines.append(
                    "| "
                    f"{float(sample.get('elapsed_seconds', 0.0)):.3f}s | "
                    f"{_md_cell(sample.get('event', ''))} | "
                    f"{_md_cell(sample.get('stage', ''))} | "
                    f"{float(sample.get('total_rss_gb', 0.0)):.3f} | "
                    f"{float(sample.get('process_rss_gb', 0.0)):.3f} | "
                    f"{float(sample.get('child_rss_gb', 0.0)):.3f} |"
                )

        path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def samples(self) -> list[dict[str, Any]]:
        with self._lock:
            return [dict(sample) for sample in self._samples]

    def summary(self, *, total_elapsed_seconds: float | None = None) -> dict[str, Any]:
        samples = self.samples()
        rss_samples = [
            sample
            for sample in samples
            if isinstance(sample.get("total_rss_bytes"), int)
        ]
        summary: dict[str, Any] = {
            "available": self._snapshot_provider is not None,
            "sample_count": len(samples),
            "event_count": sum(
                1 for sample in samples if sample.get("kind") == "event"
            ),
            "periodic_count": sum(
                1 for sample in samples if sample.get("kind") == "sample"
            ),
        }
        if total_elapsed_seconds is not None:
            summary["total_elapsed_seconds"] = round(float(total_elapsed_seconds), 6)
        if not rss_samples:
            return summary

        peak_sample = max(
            rss_samples, key=lambda sample: int(sample["total_rss_bytes"])
        )
        summary["peak"] = _peak_payload(peak_sample)
        summary["peak_rss_bytes"] = int(peak_sample["total_rss_bytes"])
        stage_peaks: dict[str, dict[str, Any]] = {}
        for sample in rss_samples:
            stage = str(sample.get("stage") or "(none)")
            existing = stage_peaks.get(stage)
            if existing is None or int(sample["total_rss_bytes"]) > int(
                existing["total_rss_bytes"]
            ):
                stage_peaks[stage] = _peak_payload(sample)
        summary["stage_peaks"] = sorted(
            stage_peaks.values(),
            key=lambda sample: int(sample["total_rss_bytes"]),
            reverse=True,
        )
        return summary

    def _sample_periodically(self) -> None:
        while not self._stop_event.wait(self.sample_interval_seconds):
            self.record_sample()

    def _record(self, kind: str, *, event: str | None = None, **fields: Any) -> None:
        stage = self._current_stage()
        snapshot = self._snapshot()
        sample: dict[str, Any] = {
            "kind": kind,
            "elapsed_seconds": round(float(self._clock() - self.started_at), 6),
        }
        if event is not None:
            sample["event"] = event
        if stage is not None:
            sample["stage"] = stage["name"]
            for key, value in stage.items():
                if key == "name":
                    continue
                sample[f"stage_{key}"] = value
        sample.update(_json_safe(fields))
        sample.update(snapshot)
        _add_gb_fields(sample)
        with self._lock:
            self._samples.append(sample)
        if kind == "event":
            self._log_event_sample(sample)

    def _snapshot(self) -> dict[str, Any]:
        if self._snapshot_provider is None:
            return {
                "available": False,
                "unavailable_reason": "psutil is not installed",
            }
        try:
            return self._snapshot_provider()
        except Exception as exc:
            self._logger.warning("memory sampling failed: %s", exc, exc_info=True)
            return {
                "available": False,
                "unavailable_reason": str(exc),
            }

    def _push_stage(self, stage: str, fields: dict[str, Any]) -> None:
        entry = {"name": stage}
        entry.update({str(key): _json_safe(value) for key, value in fields.items()})
        with self._lock:
            self._stage_stack.append(entry)

    def _pop_stage(self) -> None:
        with self._lock:
            if self._stage_stack:
                self._stage_stack.pop()

    def _current_stage(self) -> dict[str, Any] | None:
        with self._lock:
            if not self._stage_stack:
                return None
            return dict(self._stage_stack[-1])

    def _log_event_sample(self, sample: dict[str, Any]) -> None:
        total_rss_gb = sample.get("total_rss_gb")
        if total_rss_gb is None:
            return
        self._logger.info(
            "memory event=%s stage=%s total_rss=%.3fGB process_rss=%.3fGB "
            "child_rss=%.3fGB children=%s",
            sample.get("event", ""),
            sample.get("stage", ""),
            float(total_rss_gb),
            float(sample.get("process_rss_gb", 0.0)),
            float(sample.get("child_rss_gb", 0.0)),
            sample.get("child_count", 0),
        )


def _add_gb_fields(sample: dict[str, Any]) -> None:
    for key in (
        "process_rss_bytes",
        "process_vms_bytes",
        "child_rss_bytes",
        "child_vms_bytes",
        "total_rss_bytes",
        "total_vms_bytes",
        "system_available_bytes",
    ):
        gb = _gb(sample.get(key))
        if gb is not None:
            sample[key.removesuffix("_bytes") + "_gb"] = gb


def _peak_payload(sample: dict[str, Any]) -> dict[str, Any]:
    return {
        "stage": sample.get("stage", ""),
        "event": sample.get("event", ""),
        "elapsed_seconds": sample.get("elapsed_seconds", 0.0),
        "total_rss_bytes": sample.get("total_rss_bytes", 0),
        "total_rss_gb": sample.get("total_rss_gb", 0.0),
        "process_rss_gb": sample.get("process_rss_gb", 0.0),
        "child_rss_gb": sample.get("child_rss_gb", 0.0),
    }


def _md_cell(value: Any) -> str:
    return str(value).replace("|", "\\|")
