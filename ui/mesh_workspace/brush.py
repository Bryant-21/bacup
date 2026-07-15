"""Shared brush state — dataclass for brush parameters and hit state."""
from __future__ import annotations

from dataclasses import dataclass
import numpy as np


@dataclass
class BrushState:
    """Shared brush state across mesh workspaces."""
    radius: float = 5.0
    strength: float = 0.5
    falloff: float = 0.5
    active: bool = False  # True while mouse is held painting
    hit_point: np.ndarray | None = None  # World-space surface hit
    hit_normal: np.ndarray | None = None  # Surface normal at hit
    hit_tri_idx: int = -1  # Triangle index at hit
