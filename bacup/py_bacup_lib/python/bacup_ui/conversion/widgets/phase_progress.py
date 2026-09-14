"""Compact pipeline phase progress rows."""
from __future__ import annotations

import os

from creation_lib.ui.widgets.modern import progress_row


def phase_bar_state(phase: dict) -> tuple[str, float]:
    """Decide how a phase's progress bar should render.

    Batch phases run as a single native call and never set item counts, so a
    completed phase must still show a full bar and a running one an
    indeterminate sweep rather than an empty cell.
    """
    status = str(phase.get("status") or "pending")
    if status == "completed":
        return ("complete", 1.0)
    if status == "running":
        try:
            total = int(phase.get("total_items", 0) or 0)
            completed = int(phase.get("completed_items", 0) or 0)
        except (TypeError, ValueError):
            total, completed = 0, 0
        if total > 0:
            return ("determinate", max(0.0, min(completed / total, 1.0)))
        return ("indeterminate", 0.0)
    return ("none", 0.0)


def draw_phase_progress(
    namespace: str,
    phase_names: list[str] | list[tuple[str, str]],
    phases: list[dict],
) -> None:
    rows = [row if isinstance(row, tuple) else (str(row), str(row)) for row in phase_names]
    seen = {key for key, _label in rows}
    for phase in phases:
        key = str(phase.get("ui_key") or phase.get("phase") or "")
        if key and key not in seen and phase.get("status", "pending") not in {"pending", "skipped"}:
            rows.append((key, str(phase.get("phase_name") or key)))
            seen.add(key)
    data_by_key = {str(p.get("ui_key") or p.get("phase") or ""): p for p in phases}
    for key, name in rows:
        phase = data_by_key.get(key, {})
        status = str(phase.get("status") or "pending")
        mode, fraction = phase_bar_state(phase)
        total = phase.get("total_items") or 0
        completed = phase.get("completed_items") or 0
        current = str(phase.get("current_item") or "") if status == "running" else ""
        detail = os.path.basename(current.replace("\\", "/"))
        if status == "error":
            detail = str(phase.get("error") or "Phase failed")
        count = f"{total:,} / {total:,}" if status == "completed" and total else (
            f"{completed:,} / {total:,}" if total else status.capitalize()
        )
        progress_row(f"{namespace}:{key}", str(phase.get("phase_name") or name), status,
                     fraction if mode in {"complete", "determinate"} else None,
                     detail=detail, count=count, detail_tooltip=current)
