"""Phase: resolve dependencies."""
from __future__ import annotations

from typing import TYPE_CHECKING

from bacup_lib.pipeline._shim import build_orchestrator_shim
from bacup_lib.record import phases as _record_phases

if TYPE_CHECKING:
    from bacup_lib.models import (
        ConversionContext,
        PhaseProgress,
        RecordNode,
    )
    from bacup_lib.runner import ConversionRunner


def resolve_dependencies(
    records: list["RecordNode"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Phase 1: log dependency graph summary and run dependency augmenters."""
    shim = build_orchestrator_shim(records, ctx)
    _record_phases.phase_resolve(shim, runner, progress)
