"""Source/target game picker widget."""
from __future__ import annotations

import os
from dataclasses import dataclass

from imgui_bundle import imgui

from creation_lib.core.game_profiles import GAME_PROFILES


@dataclass
class GamePickerResult:
    source_changed: bool
    target_changed: bool
    source_game: str
    target_game: str


def draw_game_picker(
    namespace: str,
    source_game: str,
    target_game: str,
    *,
    target_must_be_moddable: bool = True,
) -> GamePickerResult:
    """Draw From/To game dropdowns and return the selected pair."""
    available_source = []
    available_target = []
    for gid, profile in sorted(GAME_PROFILES.items()):
        db_path = os.path.join("data", f"{gid}_records.db")
        if os.path.isfile(db_path):
            available_source.append(gid)
            if not target_must_be_moddable or profile.is_moddable:
                available_target.append(gid)

    imgui.text("From:")
    imgui.same_line()
    imgui.push_item_width(180)
    src_labels = [GAME_PROFILES[g].display_name for g in available_source]
    src_idx = available_source.index(source_game) if source_game in available_source else -1
    source_changed, new_idx = imgui.combo(f"##src_game{namespace}", src_idx, src_labels)
    if source_changed and 0 <= new_idx < len(available_source):
        source_game = available_source[new_idx]
    imgui.pop_item_width()

    imgui.same_line()
    imgui.text("  To:")
    imgui.same_line()
    imgui.push_item_width(180)
    tgt_labels = [GAME_PROFILES[g].display_name for g in available_target]
    tgt_idx = available_target.index(target_game) if target_game in available_target else -1
    target_changed, new_idx = imgui.combo(f"##tgt_game{namespace}", tgt_idx, tgt_labels)
    if target_changed and 0 <= new_idx < len(available_target):
        target_game = available_target[new_idx]
    imgui.pop_item_width()

    return GamePickerResult(
        source_changed=source_changed,
        target_changed=target_changed,
        source_game=source_game,
        target_game=target_game,
    )
