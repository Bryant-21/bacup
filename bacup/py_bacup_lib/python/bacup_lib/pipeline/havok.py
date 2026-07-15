"""Phase: convert Havok assets."""
from __future__ import annotations

from typing import TYPE_CHECKING

from bacup_lib.pipeline._shim import build_orchestrator_shim
from bacup_lib.workflows.asset_phases import phase_convert_havok_native

if TYPE_CHECKING:
    from bacup_lib.models import AssetRef, ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


def convert_havok(
    assets: list["AssetRef"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Phase 6: convert Havok behavior and animation assets."""
    shim = build_orchestrator_shim([], ctx)
    shim.graph.all_assets = list(assets)
    phase_convert_havok_native(shim, runner, progress)
