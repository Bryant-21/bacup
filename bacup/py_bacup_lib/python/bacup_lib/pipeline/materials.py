"""Phase: convert materials."""
from __future__ import annotations

from typing import TYPE_CHECKING

from bacup_lib.pipeline._shim import build_orchestrator_shim

if TYPE_CHECKING:
    from bacup_lib.models import AssetRef, ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


def convert_materials(
    assets: list["AssetRef"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Phase 5: convert material files referenced by translated records."""
    from bacup_lib.workflows.asset_phases import phase_convert_materials_native

    shim = build_orchestrator_shim(records=[], ctx=ctx)
    shim.graph.all_assets = list(assets)
    phase_convert_materials_native(shim, runner, progress)
