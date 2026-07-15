"""Phase: convert animations."""
from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from bacup_lib.models import AssetRef, ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


def convert_animations(
    assets: list["AssetRef"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Animations are handled as part of the Havok conversion phase."""
    runner.emit_log(
        "INFO",
        "convert_animations: animations are folded into convert_havok; skipping standalone phase",
    )
    progress.total_items = 0
    progress.completed_items = 0
