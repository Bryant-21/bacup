"""Phase: synthesize behavior drivers."""
from __future__ import annotations

from typing import TYPE_CHECKING

from bacup_lib.pipeline._shim import build_orchestrator_shim
from bacup_lib.workflows.asset_phases import phase_synthesize_drivers_native

if TYPE_CHECKING:
    from bacup_lib.models import ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


def synthesize_drivers(
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Phase 7: synthesize internal behavior driver chains."""
    phase_synthesize_drivers_native(
        build_orchestrator_shim([], ctx),
        runner,
        progress,
    )
