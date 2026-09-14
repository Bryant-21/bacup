"""Record dependency resolution for graph-scoped conversion."""
from __future__ import annotations

from bacup_lib.runner import ConversionRunner
from bacup_lib.models import PhaseProgress


def phase_resolve(orchestrator, runner: ConversionRunner, progress: PhaseProgress) -> None:
    """Phase 1: Dependencies are already resolved in the graph. Log summary.
    """
    progress.total_items = 1
    progress.completed_items = 1
    progress.current_item = "Dependency graph"

    # Add content the record walker can't reach by reference: ATX skin BGSMs,
    # their textures and one synthesized MaterialSwap per variant, plus weapon
    # sounds matched by EditorID tokens under ``sound/fx/wpn/<dir>/``. Best-effort.
    orchestrator._augment_graph_with_atx_and_sounds(runner)

    n_records = len(orchestrator.graph.all_records)
    n_assets = len(orchestrator.graph.all_assets)
    n_errors = len(orchestrator.graph.errors)

    msg = f"Resolved {n_records} records, {n_assets} assets"
    if n_errors:
        msg += f" ({n_errors} errors)"
    runner.emit_log("INFO", msg)
    orchestrator._log_lines.append(f"[INFO] {msg}")

    for err in orchestrator.graph.errors:
        runner.emit_log("WARN", err)
        orchestrator._log_lines.append(f"[WARN] {err}")

    runner.emit_item_progress(progress)

