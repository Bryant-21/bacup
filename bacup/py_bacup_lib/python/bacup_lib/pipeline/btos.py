"""Phase: convert terrain BTO files."""
from __future__ import annotations

from typing import TYPE_CHECKING

from bacup_lib.pipeline._shim import build_orchestrator_shim
from bacup_lib.workflows.asset_phases import (
    phase_convert_btos_native,
    phase_convert_btos_native_v2,
)

if TYPE_CHECKING:
    from bacup_lib.models import ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


def convert_btos(
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Convert terrain .bto files discovered from source roots."""
    shim = build_orchestrator_shim(records=[], ctx=ctx)
    phase_convert_btos_native(shim, runner, progress)


def convert_btos_v2(
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Single-call memoized terrain BTO conversion."""
    shim = build_orchestrator_shim(records=[], ctx=ctx)
    phase_convert_btos_native_v2(shim, runner, progress)
