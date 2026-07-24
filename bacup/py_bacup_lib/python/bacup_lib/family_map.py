from __future__ import annotations
from dataclasses import dataclass

from bacup_lib.models import PhaseSelection

# Experimental precombine gate (see models.PhaseSelection.generate_precombines).
# While the PhaseSelection default is False the phase is wired but DORMANT: it is
# neither a Meshes feeder nor force-enabled on upgrade, so `resolve_upgrade_plan`
# leaves it off in every family set unless a caller sets the flag explicitly.
# Flipping the default to True flips this constant, which in turn makes it a
# Meshes feeder (Bucket A) AND restamp-always in `_phases_off()` (Bucket B,
# mirroring `regenerate_modt`) — the correct end state with no other edits.
_PRECOMBINES_ENABLED = PhaseSelection.generate_precombines

FAMILY_FEEDERS: dict[str, tuple[str, ...]] = {
    "NIFs":       ("convert_nifs",),
    "Havok":      ("convert_havok", "synthesize_drivers", "generate_anim_text_data"),
    "Meshes": (
        ("convert_nifs", "convert_npc_faces", "generate_anim_text_data")
        + (("generate_precombines",) if _PRECOMBINES_ENABLED else ())
    ),
    "Materials":  ("convert_materials",),
    "Textures":   ("convert_textures", "convert_npc_faces"),
    "Terrain":    ("convert_terrain",),
    "LOD":        ("convert_lod",),
    "Animations": ("convert_havok", "synthesize_drivers", "convert_animations", "generate_anim_text_data"),
    "Scripts":    ("convert_scripts",),
    "Sounds":     ("copy_sounds",),
}

# BA2 label prefixes per family. NOT identity: FO4 archive aliasing splits
# Meshes into Meshes/MeshesExtra and renames Scripts' archive to Misc
# (mod_pack.rs family_label_aliases; verified against the deployed shard names).
FAMILY_BA2_LABEL: dict[str, tuple[str, ...]] = {
    "NIFs":       ("Meshes", "MeshesExtra"),
    "Havok":      ("Animations",),
    "Meshes":     ("Meshes", "MeshesExtra"),
    "Materials":  ("Materials",),
    "Textures":   ("Textures",),
    "Terrain":    ("Terrain", "TerrainTextures"),
    "LOD":        ("LOD", "LODTextures"),
    "Animations": ("Animations",),
    "Scripts":    ("Misc",),
    "Sounds":     ("Sounds",),
}

# Every phase flag on PhaseSelection that this map controls.
_ALL_CONTROLLED = tuple(sorted({p for ps in FAMILY_FEEDERS.values() for p in ps}))

_ALL_LABELS = tuple(sorted({lbl for labels in FAMILY_BA2_LABEL.values() for lbl in labels}))


@dataclass(frozen=True)
class UpgradePlan:
    phases: PhaseSelection
    regen_terrain: bool
    swap_labels: tuple[str, ...]
    full_build: bool
    force_regen: bool = False


def _phases_off() -> PhaseSelection:
    ps = PhaseSelection()
    for name in _ALL_CONTROLLED:
        setattr(ps, name, False)
    ps.build_esp = True           # ESM always rebuilt
    ps.translate_records = True
    ps.convert_placed_records = True
    ps.regenerate_modt = True     # Bucket B: ESM-record mutation (MODT), always re-populated on the rebuilt ESM
    if _PRECOMBINES_ENABLED:
        # Bucket B when the gate is lifted: the rebuilt ESM drops CELL/REFR
        # precombine stamps, so restamp always runs — exactly like regenerate_modt.
        # Runs before rebuild_cell_offsets so CELL sizes are final before OFST/CLSZ rebuild.
        ps.generate_precombines = True
    ps.rebuild_cell_offsets = True  # Bucket B: OFST/CLSZ encode file layout; always rebuilt on the rebuilt ESM (must stay last)
    return ps


def resolve_upgrade_plan(family_set: frozenset[str]) -> UpgradePlan:
    if "ALL" in family_set:
        return UpgradePlan(
            PhaseSelection.defaults(),
            regen_terrain=True,
            swap_labels=_ALL_LABELS,
            full_build=True,
        )
    ps = _phases_off()
    for fam in family_set:
        for phase in FAMILY_FEEDERS[fam]:
            setattr(ps, phase, True)
    # convert_terrain always runs in upgrade mode (even when Terrain isn't in
    # family_set): the LAND/NAVM graft from the deployed ESM lives inside this
    # phase, so turning it off would drop all terrain records from the
    # always-rebuilt ESM. regen_terrain (below) is what distinguishes a fresh
    # terrain compute from a graft-only pass — not this flag.
    ps.convert_terrain = True
    return UpgradePlan(
        phases=ps,
        regen_terrain="Terrain" in family_set,
        swap_labels=tuple(sorted({lbl for f in family_set for lbl in FAMILY_BA2_LABEL[f]})),
        full_build=False,
    )
