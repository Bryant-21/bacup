from bacup_lib.family_map import (
    resolve_upgrade_plan,
    FAMILY_FEEDERS,
    FAMILY_BA2_LABEL,
    _PRECOMBINES_ENABLED,
)
from bacup_lib.models import PhaseSelection


def test_meshes_materials_closure_and_labels():
    plan = resolve_upgrade_plan(frozenset({"Meshes", "Materials"}))
    assert not plan.full_build and not plan.regen_terrain
    assert plan.phases.convert_nifs and plan.phases.convert_npc_faces and plan.phases.convert_materials
    assert plan.phases.generate_anim_text_data
    assert not plan.phases.convert_textures and not plan.phases.convert_havok
    assert plan.phases.convert_terrain is True          # Correction B: graft mode, always on
    assert set(plan.swap_labels) == {"Meshes", "MeshesExtra", "Materials"}   # Correction A


def test_scripts_maps_to_misc_label():
    plan = resolve_upgrade_plan(frozenset({"Scripts"}))
    assert set(plan.swap_labels) == {"Misc"}


def test_terrain_sets_regen_terrain():
    plan = resolve_upgrade_plan(frozenset({"Terrain"}))
    assert plan.regen_terrain and plan.phases.convert_terrain
    assert set(plan.swap_labels) == {"Terrain", "TerrainTextures"}


def test_lod_swaps_general_and_texture_archives():
    plan = resolve_upgrade_plan(frozenset({"LOD"}))
    assert set(plan.swap_labels) == {"LOD", "LODTextures"}


def test_all_is_full_build():
    plan = resolve_upgrade_plan(frozenset({"ALL"}))
    assert plan.full_build and plan.regen_terrain
    assert plan.phases.convert_nifs and plan.phases.convert_textures and plan.phases.convert_terrain
    assert "MeshesExtra" in plan.swap_labels and "Misc" in plan.swap_labels


def test_regenerate_modt_always_on_upgrade():
    # Bucket B: MODT compute mutates ESM records only (no assets) -> never family-gated.
    assert resolve_upgrade_plan(frozenset({"Textures"})).phases.regenerate_modt is True
    assert resolve_upgrade_plan(frozenset()).phases.regenerate_modt is True
    assert resolve_upgrade_plan(frozenset({"ALL"})).phases.regenerate_modt is True


def test_anim_text_data_is_gated_by_meshes_and_havok_families():
    assert resolve_upgrade_plan(frozenset({"Meshes"})).phases.generate_anim_text_data
    assert resolve_upgrade_plan(frozenset({"Havok"})).phases.generate_anim_text_data
    assert not resolve_upgrade_plan(
        frozenset({"NIFs"})
    ).phases.generate_anim_text_data
    assert not resolve_upgrade_plan(
        frozenset({"Materials"})
    ).phases.generate_anim_text_data


def test_generate_precombines_gate_matches_phaseselection_default():
    # The wiring is driven entirely by the PhaseSelection default; while that is
    # False the phase must be dormant in the family map.
    assert _PRECOMBINES_ENABLED is PhaseSelection.generate_precombines
    if not _PRECOMBINES_ENABLED:
        assert "generate_precombines" not in FAMILY_FEEDERS["Meshes"]


def test_generate_precombines_off_in_every_upgrade_mode_while_gated():
    # Experimental gate: stays off unless explicitly enabled — including the
    # Meshes family, empty scope, and the full-build ALL scope.
    if _PRECOMBINES_ENABLED:
        return
    for family_set in (
        frozenset({"Meshes"}),
        frozenset({"Meshes", "Materials"}),
        frozenset({"Textures"}),
        frozenset(),
        frozenset({"ALL"}),
    ):
        plan = resolve_upgrade_plan(family_set)
        assert plan.phases.generate_precombines is False, family_set


def test_generate_precombines_restamp_always_when_gate_lifted():
    # Documents the end-state: flipping the PhaseSelection default makes it a
    # Meshes feeder (bake) AND force-enabled on every upgrade (restamp).
    if not _PRECOMBINES_ENABLED:
        return
    assert "generate_precombines" in FAMILY_FEEDERS["Meshes"]
    assert resolve_upgrade_plan(frozenset({"Meshes"})).phases.generate_precombines
    assert resolve_upgrade_plan(frozenset({"Textures"})).phases.generate_precombines
    assert resolve_upgrade_plan(frozenset()).phases.generate_precombines


def test_nifs_havok_scope_runs_havok_dependent_driver_synthesis():
    plan = resolve_upgrade_plan(frozenset({"NIFs", "Havok"}))

    assert plan.phases.convert_nifs is True
    assert plan.phases.convert_havok is True
    assert plan.phases.convert_npc_faces is False
    assert plan.phases.synthesize_drivers is True
    assert plan.phases.convert_animations is False
    assert plan.phases.generate_anim_text_data is True
    assert plan.phases.convert_materials is False
    assert plan.phases.convert_textures is False
    assert plan.phases.convert_terrain is True
    assert set(plan.swap_labels) == {"Meshes", "MeshesExtra", "Animations"}
