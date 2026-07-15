from ui.editor.particles import (
    ParticleSupportLevel as ExportedParticleSupportLevel,
    build_particle_models as exported_build_particle_models,
)
from ui.editor.particles.model import (
    ParticleSupportLevel,
    build_particle_models,
    owner_system_for_block,
)


class FakeSchema:
    def is_subtype_of(self, type_name, base_name):
        if base_name == "NiParticleSystem":
            return type_name == "NiParticleSystem"
        if base_name == "NiPSysModifier":
            return type_name.startswith("NiPSys") or type_name.startswith("BSPSys") or type_name == "BSWindModifier"
        if base_name == "NiPSysEmitter":
            return type_name in {
                "NiPSysMeshEmitter",
                "NiPSysBoxEmitter",
                "NiPSysSphereEmitter",
                "NiPSysCylinderEmitter",
                "NiPSysUnsupportedEmitter",
            }
        return type_name == base_name


class FakeBlock:
    def __init__(self, block_id, type_name, **fields):
        self.block_id = block_id
        self.type_name = type_name
        self._fields = fields

    def get_field(self, name):
        return self._fields.get(name)


class FakeNif:
    def __init__(self, blocks):
        self.blocks = blocks
        self.schema = FakeSchema()

    def get_block(self, block_id):
        if block_id < 0 or block_id >= len(self.blocks):
            return None
        return self.blocks[block_id]


class FakeNifWithoutSchema:
    def __init__(self, blocks):
        self.blocks = blocks

    def get_block(self, block_id):
        if block_id < 0 or block_id >= len(self.blocks):
            return None
        return self.blocks[block_id]


def test_build_particle_model_extracts_core_links():
    nif = FakeNif([
        FakeBlock(0, "NiNode", Name="Root"),
        FakeBlock(1, "NiPSysData", **{"BS Max Vertices": 64, "Has Texture Indices": 1, "Num Subtexture Offsets": 2, "Subtexture Offsets": [{"x": 0, "y": 0.5, "z": 0, "w": 1}, {"x": 0.5, "y": 0.5, "z": 0, "w": 1}]}),
        FakeBlock(2, "BSEffectShaderProperty", **{"Shader Flags 1": ["GreyscaleToPalette_Color", "GreyscaleToPalette_Alpha"], "Source Texture": r"textures\effects\smoke.dds", "Greyscale Texture": r"textures\effects\gradients\smokegrad.dds", "Base Color": {"r": 0.25, "g": 0.5, "b": 0.75, "a": 1.25}}),
        FakeBlock(3, "NiAlphaProperty"),
        FakeBlock(4, "NiPSysBoxEmitter", Name="Emitter", Target=5, Active=1, Speed=10.0, **{"Speed Variation": 1.5, "Declination": 0.5, "Declination Variation": 0.1, "Planar Angle": 1.5, "Planar Angle Variation": 0.2, "Life Span": 2.0, "Life Span Variation": 0.25, "Initial Radius": 1.0, "Radius Variation": 0.2, "Initial Color": {"r": 0.1, "g": 0.2, "b": 0.3, "a": 0.4}, "Emitter Object": 0, "Radius": 3.0}),
        FakeBlock(5, "NiParticleSystem", Name="Smoke", Data=1, **{"Shader Property": 2, "Alpha Property": 3, "World Space": 1, "Num Modifiers": 1, "Modifiers": [4], "Controller": -1}),
    ])

    models = build_particle_models(nif, nif_id="main")

    assert len(models) == 1
    model = models[0]
    assert model.nif_id == "main"
    assert model.system_block_id == 5
    assert model.name == "Smoke"
    assert model.data_block_id == 1
    assert model.shader_property_block_id == 2
    assert model.alpha_property_block_id == 3
    assert model.controller_block_id is None
    assert model.emitter_block_id == 4
    assert model.emitter_type == "NiPSysBoxEmitter"
    assert model.modifier_block_ids == (4,)
    assert model.modifier_types == ("NiPSysBoxEmitter",)
    assert model.world_space is True
    assert model.max_particles == 64
    assert model.support_level is ParticleSupportLevel.SUPPORTED
    assert model.atlas_offsets == ((0.0, 0.5, 0.0, 1.0), (0.5, 1.0, 0.0, 1.0))
    assert model.source_texture == r"textures\effects\smoke.dds"
    assert model.greyscale_texture == r"textures\effects\gradients\smokegrad.dds"
    assert model.greyscale_color is True
    assert model.greyscale_alpha is True
    assert model.base_color == (0.25, 0.5, 0.75, 1.25)
    assert model.emitter_initial_color == (0.1, 0.2, 0.3, 0.4)
    assert model.emitter_speed == 10.0
    assert model.emitter_speed_variation == 1.5
    assert model.emitter_declination == 0.5
    assert model.emitter_declination_variation == 0.1
    assert model.emitter_planar_angle == 1.5
    assert model.emitter_planar_angle_variation == 0.2
    assert model.emitter_lifetime == 2.0
    assert model.emitter_lifetime_variation == 0.25
    assert model.emitter_initial_radius == 1.0
    assert model.emitter_radius_variation == 0.2
    assert model.emitter_radius == 3.0
    assert model.emitter_object_block_id == 0


def test_atlas_offsets_convert_nif_rectangles_to_uv_min_max():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData", **{"Has Texture Indices": 1, "Num Subtexture Offsets": 1, "Subtexture Offsets": [{"x": 0.25, "y": 0.25, "z": 0.5, "w": 0.25}]}),
        FakeBlock(1, "NiPSysSphereEmitter"),
        FakeBlock(2, "NiParticleSystem", Data=0, Modifiers=[1]),
    ])

    [model] = build_particle_models(nif)

    assert model.atlas_offsets == ((0.25, 0.5, 0.5, 0.75),)


def test_build_particle_model_reports_missing_required_links():
    nif = FakeNif([
        FakeBlock(0, "NiParticleSystem", Name="Broken", Data=-1, **{"Shader Property": -1, "Alpha Property": -1, "Modifiers": []}),
    ])

    [model] = build_particle_models(nif, nif_id="main")

    assert model.support_level is ParticleSupportLevel.UNSUPPORTED
    assert "missing NiPSysData" in model.warning_text
    assert "missing emitter" in model.warning_text


def test_owner_system_for_modifier_block():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData"),
        FakeBlock(1, "NiPSysBoxEmitter", Name="Emitter"),
        FakeBlock(2, "NiPSysGravityModifier", Name="Gravity"),
        FakeBlock(3, "NiParticleSystem", Name="Smoke", Data=0, Modifiers=[1, 2]),
    ])
    [model] = build_particle_models(nif, nif_id="main")

    assert owner_system_for_block([model], 1) is model
    assert owner_system_for_block([model], 2) is model
    assert owner_system_for_block([model], 999) is None


def test_nested_modifier_refs_are_owned_and_keep_hierarchy_metadata():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData"),
        FakeBlock(1, "NiPSysSphereEmitter", Name="Emitter"),
        FakeBlock(2, "NiPSysAgeDeathModifier", Name="Age Death", Target=4, **{"Spawn Modifier": 3}),
        FakeBlock(3, "NiPSysSpawnModifier", Name="Spawn", Target=4),
        FakeBlock(4, "NiParticleSystem", Name="Plasma", Data=0, Modifiers=[1, 2]),
    ])

    [model] = build_particle_models(nif, nif_id="main")

    assert model.modifier_block_ids == (1, 2, 3)
    assert model.modifier_types == (
        "NiPSysSphereEmitter",
        "NiPSysAgeDeathModifier",
        "NiPSysSpawnModifier",
    )
    assert model.modifier_parent_block_ids == (None, None, 2)
    assert model.modifier_depths == (0, 0, 1)
    assert owner_system_for_block([model], 3) is model


def test_package_exports_model_api():
    assert ExportedParticleSupportLevel is ParticleSupportLevel
    assert exported_build_particle_models is build_particle_models


def test_particle_support_level_defines_plan_values():
    assert ParticleSupportLevel.SUPPORTED.value == "supported"
    assert ParticleSupportLevel.APPROXIMATE.value == "approximate"
    assert ParticleSupportLevel.DIAGNOSTIC_ONLY.value == "diagnostic-only"
    assert ParticleSupportLevel.UNSUPPORTED.value == "unsupported"


def test_build_particle_models_defaults_to_main_nif_id():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData"),
        FakeBlock(1, "NiPSysSphereEmitter"),
        FakeBlock(2, "NiParticleSystem", Data=0, Modifiers=[1]),
    ])

    [model] = build_particle_models(nif)

    assert model.nif_id == "main"


def test_block_ids_include_owned_refs_and_exclude_negative_refs():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData"),
        FakeBlock(1, "BSEffectShaderProperty"),
        FakeBlock(2, "NiAlphaProperty"),
        FakeBlock(3, "NiTimeController"),
        FakeBlock(4, "NiNode", Name="Gravity Helper"),
        FakeBlock(5, "NiNode", Name="Drag Helper"),
        FakeBlock(6, "NiNode", Name="Field Helper"),
        FakeBlock(7, "NiNode", Name="Emitter Helper"),
        FakeBlock(8, "NiTriShape", Name="Emitter Mesh"),
        FakeBlock(9, "NiPSysBoxEmitter", **{"Emitter": 7, "Emitter Meshes": [8, -1]}),
        FakeBlock(10, "NiPSysGravityModifier", **{"Gravity Object": 4}),
        FakeBlock(11, "NiPSysDragModifier", **{"Drag Object": 5}),
        FakeBlock(12, "NiPSysFieldModifier", **{"Field Object": 6}),
        FakeBlock(13, "NiParticleSystem", Data=0, **{"Shader Property": 1, "Alpha Property": 2, "Controller": 3, "Modifiers": [9, 10, 11, 12, -1]}),
    ])

    [model] = build_particle_models(nif)

    assert model.helper_node_block_ids == (7, 4, 5, 6)
    assert model.emitter_mesh_block_ids == (8,)
    assert model.block_ids == {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13}
    for block_id in model.block_ids:
        assert owner_system_for_block([model], block_id) is model
    assert owner_system_for_block([model], -1) is None


def test_unsupported_emitter_is_diagnostic_only_with_warning():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData"),
        FakeBlock(1, "NiPSysUnsupportedEmitter"),
        FakeBlock(2, "NiParticleSystem", Data=0, Modifiers=[1]),
    ])

    [model] = build_particle_models(nif)

    assert model.support_level is ParticleSupportLevel.DIAGNOSTIC_ONLY
    assert model.warnings[0].block_id == 1
    assert model.warnings[0].severity == "warning"
    assert "unsupported emitter NiPSysUnsupportedEmitter" in model.warning_text


def test_dict_refs_are_treated_as_scalar_block_refs():
    nif = FakeNif([
        FakeBlock(0, "NiPSysData", **{"Has Texture Indices": 1, "Num Subtexture Offsets": 1, "Subtexture Offsets": [(0, 0.25, 0.5, 0.25)]}),
        FakeBlock(1, "NiNode", Name="Emitter Helper"),
        FakeBlock(2, "NiNode", Name="Gravity Helper"),
        FakeBlock(3, "NiTriShape", Name="Emitter Mesh"),
        FakeBlock(4, "NiTriShape", Name="Second Emitter Mesh"),
        FakeBlock(5, "NiPSysBoxEmitter", **{"Emitter": {"Ref": 1}, "Emitter Meshes": [{"Ref": 3}, {"Block ID": 4}, {"value": -1}]}),
        FakeBlock(6, "NiPSysGravityModifier", **{"Gravity Object": {"Ref": 2}}),
        FakeBlock(7, "NiParticleSystem", Data={"Ref": 0}, Modifiers=[{"Ref": 5}, {"Block ID": 6}, {"Value": -1}]),
    ])

    [model] = build_particle_models(nif)

    assert model.data_block_id == 0
    assert model.modifier_block_ids == (5, 6)
    assert model.helper_node_block_ids == (1, 2)
    assert model.emitter_mesh_block_ids == (3, 4)
    assert model.atlas_offsets == ((0.0, 0.25, 0.5, 0.75),)


def test_schema_less_fallback_recognizes_particle_emitters():
    nif = FakeNifWithoutSchema([
        FakeBlock(0, "NiPSysData"),
        FakeBlock(1, "NiPSysBoxEmitter"),
        FakeBlock(2, "NiParticleSystem", Data=0, Modifiers=[1]),
    ])

    [model] = build_particle_models(nif)

    assert model.emitter_block_id == 1
    assert model.support_level is ParticleSupportLevel.SUPPORTED
