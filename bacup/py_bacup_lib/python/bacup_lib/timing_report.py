from __future__ import annotations

import json
import logging
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

_LOG = logging.getLogger(__name__)


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


@dataclass
class TimingReport:
    started_at: float = field(default_factory=time.perf_counter)
    events: list[dict[str, Any]] = field(default_factory=list)

    def record(self, name: str, elapsed_seconds: float, **fields: Any) -> None:
        event: dict[str, Any] = {
            "name": name,
            "elapsed_seconds": round(float(elapsed_seconds), 6),
        }
        event.update({key: _json_safe(value) for key, value in fields.items()})
        self.events.append(event)
        memory_report = getattr(self, "memory_report", None)
        record_timing_event = getattr(memory_report, "record_timing_event", None)
        if callable(record_timing_event):
            try:
                record_timing_event(name, event)
            except Exception:
                _LOG.warning("memory timing event failed: %s", name, exc_info=True)

    def to_dict(self, *, total_elapsed_seconds: float | None = None) -> dict[str, Any]:
        payload: dict[str, Any] = {"events": self.events}
        if total_elapsed_seconds is not None:
            payload["total_elapsed_seconds"] = round(float(total_elapsed_seconds), 6)
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
