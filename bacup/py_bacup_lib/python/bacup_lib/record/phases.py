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

    # Augment the graph with content the main walker cannot reach via
    # record references alone:
    #   - ATX (Atomic Shop) skin BGSMs and their textures, plus a
    #     synthesized MaterialSwap record per skin variant.
    #   - Heuristic per-weapon sound samples discovered by EditorID
    #     token-matching against ``sound/fx/wpn/<dir>/`` directories.
    # Both passes are best-effort: silent on no-match.
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

