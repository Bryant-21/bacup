from bacup_lib.family_map import resolve_upgrade_plan, FAMILY_FEEDERS, FAMILY_BA2_LABEL


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


def test_anim_text_data_is_gated_by_meshes_family():
    assert resolve_upgrade_plan(frozenset({"Meshes"})).phases.generate_anim_text_data
    assert not resolve_upgrade_plan(
        frozenset({"Materials"})
    ).phases.generate_anim_text_data
