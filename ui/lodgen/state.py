from __future__ import annotations

import copy
import json
from dataclasses import dataclass, field

from creation_lib.lod.default_settings import fo4_default_settings


@dataclass
class LodgenState:
    game: str = "fo4"
    worldspace: str = ""
    worldspaces: list[str] = field(default_factory=list)
    output_dir: str = ""
    settings: dict = field(default_factory=fo4_default_settings)
    running: bool = False
    progress_frac: float = 0.0
    progress_msg: str = ""
    log_lines: list[str] = field(default_factory=list)
    error_message: str = ""
    last_result: object | None = None


def settings_to_json(state: LodgenState) -> str:
    return json.dumps(state.settings)


def collect_preset(state: LodgenState) -> dict:
    return {
        "game": state.game,
        "worldspace": state.worldspace,
        "output_dir": state.output_dir,
        "settings": copy.deepcopy(state.settings),
    }


def apply_preset(state: LodgenState, preset: dict) -> None:
    state.game = str(preset.get("game", state.game) or "fo4")
    state.worldspace = str(preset.get("worldspace", state.worldspace) or "")
    state.output_dir = str(preset.get("output_dir", state.output_dir) or "")
    incoming = preset.get("settings")
    if isinstance(incoming, dict):
        merged = fo4_default_settings()
        merged.update(copy.deepcopy(incoming))
        state.settings = merged
