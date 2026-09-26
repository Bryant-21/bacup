import pytest

from bacup_lib.family_map import (
    FAMILY_BA2_LABEL,
    resolve_upgrade_plan,
)


def test_meshes_materials_closure_and_labels():
    plan = resolve_upgrade_plan(frozenset({"Meshes", "Materials"}))
    assert not plan.full_build and not plan.regen_terrain
    assert plan.phases.convert_nifs and plan.phases.convert_npc_faces and plan.phases.convert_materials
    assert plan.phases.generate_anim_text_data
    assert not plan.phases.convert_textures and not plan.phases.convert_havok
    assert plan.phases.convert_terrain is True          # Correction B: graft mode, always on
    assert set(plan.swap_labels) == {"Meshes", "MeshesExtra", "Materials"}   # Correction A


@pytest.mark.parametrize(
    "families,labels",
    [
        ({"Scripts"}, {"Misc"}),
        ({"LOD"}, {"LOD", "LODTextures"}),
        ({"Terrain"}, {"Materials", "Textures"}),
    ],
)
def test_family_swap_labels(families, labels):
    plan = resolve_upgrade_plan(frozenset(families))
    assert set(plan.swap_labels) == labels
    assert plan.regen_terrain == ("Terrain" in families)
    if "Terrain" in families:
        assert plan.phases.convert_terrain


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


def test_anim_text_data_and_driver_synthesis_gated_by_meshes_and_havok():
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

    assert resolve_upgrade_plan(frozenset({"Meshes"})).phases.generate_anim_text_data
    assert resolve_upgrade_plan(frozenset({"Havok"})).phases.generate_anim_text_data
    assert not resolve_upgrade_plan(frozenset({"NIFs"})).phases.generate_anim_text_data
    assert not resolve_upgrade_plan(frozenset({"Materials"})).phases.generate_anim_text_data
