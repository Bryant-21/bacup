from __future__ import annotations

import time


# Rounded phase seconds from SeventySix/20260910-131205-pid28856. These are
# relative work estimates, not an ETA; concurrent tracks can overlap.
PHASE_SECONDS = {
    "prepare_conversion": 85,
    "translate_records": 203,
    "convert_terrain": 141,
    "emit_projected_navmeshes": 9,
    "convert_interior_cells": 22,
    "rebuild_projected_navi": 11,
    "copy_projected_placed_children": 24,
    "synthesize_worldspace_persistent_cell": 8,
    "sync_projected_cell_locations": 1,
    "synthesize_encounter_zones": 4,
    "synthesize_interior_sky_regions": 2,
    "repair_placed_child_refs": 131,
    "synthesize_vendor_dialogue": 11,
    "scaffold_mod": 1,
    "convert_scripts": 43,
    "inventory_quest_runtime_routes": 50,
    "build_esp": 9,
    "check_runtime_hazards": 1,
    "copy_sounds": 8,
    "convert_nifs": 532,
    "convert_textures": 543,
    "convert_materials": 13,
    "convert_havok": 26,
    "postprocess_havok_assets": 196,
    "synthesize_drivers": 19,
    "copy_materialized_facegen": 8,
    "regenerate_modt": 38,
    "rebuild_cell_offsets": 23,
    "copy_vaultboy_swfs": 1,
    "generate_anim_text_data": 129,
    "lodgen": 240,
    # Not measured in that run; conservative starting weights.
    "convert_npc_faces": 120,
    "synthesize_object_lod": 30,
    "convert_terrain_assets": 120,
    "convert_skeleton": 10,
    "convert_animations": 60,
    "generate_precombines": 300,
    "pack": 300,
    "deploy": 120,
}

_PROJECTED_PHASES = (
    "emit_projected_navmeshes",
    "convert_interior_cells",
    "rebuild_projected_navi",
    "copy_projected_placed_children",
    "synthesize_worldspace_persistent_cell",
    "sync_projected_cell_locations",
    "synthesize_encounter_zones",
    "synthesize_interior_sky_regions",
    "repair_placed_child_refs",
    "synthesize_vendor_dialogue",
)
_ASSET_PHASES = (
    "convert_nifs",
    "convert_textures",
    "convert_materials",
    "convert_havok",
    "postprocess_havok_assets",
    "synthesize_drivers",
    "convert_skeleton",
    "convert_animations",
    "copy_materialized_facegen",
    "copy_sounds",
)
_POST_PHASES = (
    "regenerate_modt",
    "generate_precombines",
    "rebuild_cell_offsets",
    "copy_vaultboy_swfs",
    "generate_anim_text_data",
    "lodgen",
    "pack",
    "deploy",
)


def estimated_phase_weights(
    pair_id: str,
    *,
    lod_mode: str = "hybrid-atlas",
    packed: bool = True,
    deploy: bool = True,
    precombines: bool = False,
    start_phase: str = "prepare",
) -> dict[str, float]:
    keys = [
        "prepare_conversion",
        "translate_records",
        "convert_terrain",
        "scaffold_mod",
        "convert_scripts",
        "build_esp",
        "check_runtime_hazards",
        *_ASSET_PHASES,
        *_POST_PHASES,
    ]
    if pair_id == "fo76:fo4":
        keys += [*_PROJECTED_PHASES, "inventory_quest_runtime_routes"]
        keys.remove("convert_skeleton")
        keys.remove("convert_animations")
        if lod_mode == "convert":
            keys.append("convert_terrain_assets")
    else:
        for key in (
            "convert_havok",
            "postprocess_havok_assets",
            "synthesize_drivers",
            "copy_materialized_facegen",
        ):
            keys.remove(key)
        if pair_id != "fnvfo3:fo4":
            keys.remove("convert_skeleton")
            keys.remove("convert_animations")
        if pair_id in {"fnvfo3:fo4", "skyrimse:fo4"}:
            keys.remove("convert_materials")
            keys.append("convert_npc_faces")
    if lod_mode == "generate" or (
        pair_id in {"fnvfo3:fo4", "skyrimse:fo4"}
        and lod_mode in {"hybrid", "hybrid-atlas"}
    ):
        keys.append("synthesize_object_lod")
    if lod_mode not in {"generate", "hybrid", "hybrid-atlas"}:
        keys.remove("lodgen")
    if not packed:
        keys.remove("pack")
    if not deploy:
        keys.remove("deploy")
    if not precombines:
        keys.remove("generate_precombines")

    asset_starts = {
        "nifs": "convert_nifs",
        "textures": "convert_textures",
        "materials": "convert_materials",
        "havok": "convert_havok",
        "drivers": "convert_havok",
        "animations": "convert_animations",
        "sounds": "copy_sounds",
    }
    post_starts = {
        "modt": "regenerate_modt",
        "offsets": "rebuild_cell_offsets",
        "animtext": "generate_anim_text_data",
        "lodgen": "lodgen",
        "pack": "pack",
    }
    if start_phase in asset_starts:
        start = _ASSET_PHASES.index(asset_starts[start_phase])
        allowed = {*_ASSET_PHASES[start:], *_POST_PHASES}
        keys = [key for key in keys if key in allowed]
    elif start_phase in post_starts:
        start = _POST_PHASES.index(post_starts[start_phase])
        keys = [key for key in keys if key in _POST_PHASES[start:]]
    elif start_phase == "deploy":
        keys = [key for key in ("pack", "deploy") if key in keys]
    return {key: PHASE_SECONDS[key] for key in keys}


def phase_fraction(phase: dict) -> float:
    if phase.get("status") == "completed":
        return 1.0
    if phase.get("status") == "skipped":
        return 0.0
    try:
        total = int(phase.get("total_items", 0) or 0)
        completed = int(phase.get("completed_items", 0) or 0)
    except (TypeError, ValueError):
        return 0.0
    return max(0.0, min(completed / total, 1.0)) if total > 0 else 0.0


class ProgressEstimate:
    def __init__(self, weights: dict[str, float]):
        self.weights = dict(weights)
        self.phases: dict[str, dict] = {}
        self.started: dict[str, float] = {}
        self.fractions: dict[str, float] = {}
        self.high_water = 0.0

    def update(self, phase: dict, *, now: float | None = None) -> None:
        key = phase["ui_key"]
        self.phases[key] = dict(phase)
        self.weights.setdefault(key, PHASE_SECONDS.get(key, 10))
        self.started.setdefault(key, time.monotonic() if now is None else now)

    def fraction(self, *, now: float | None = None, advance: bool = True) -> float:
        now = time.monotonic() if now is None else now
        for key, phase in self.phases.items():
            if phase.get("status") == "skipped":
                continue
            fraction = phase_fraction(phase)
            if phase.get("status") == "running":
                if advance and not phase.get("total_items"):
                    fraction = max(0.0, (now - self.started[key]) / self.weights[key])
                fraction = min(fraction, 0.95)
            self.fractions[key] = max(self.fractions.get(key, 0.0), fraction)
        weights = {
            key: weight
            for key, weight in self.weights.items()
            if self.phases.get(key, {}).get("status") != "skipped"
        }
        total = sum(weights.values())
        fraction = (
            sum(
                weight * self.fractions.get(key, 0.0) for key, weight in weights.items()
            )
            / total
            if total
            else 0.0
        )
        # Reserve completion for the UI's final result, after deployment/cleanup.
        self.high_water = max(self.high_water, min(fraction, 0.99))
        return self.high_water
