import math
from types import SimpleNamespace
from unittest.mock import MagicMock

from creation_lib.nif.actions import SnapshotAction
from creation_lib.nif.nif_file import NifFile

from ui.editor.particles.authoring import (
    add_modifier_to_particle_system,
    add_modifier_to_particle_system_session,
    apply_draft_to_session,
    apply_draft_to_nif,
    draft_from_particle_model,
    remove_modifier_from_particle_system,
    remove_modifier_from_particle_system_session,
    validate_draft,
)
from ui.editor.particles.builder_model import (
    EmissionShape,
    ModifierKind,
    ParticleEffectDraft,
    ParticleModifierDraft,
    ParticleSystemDraft,
)
from ui.editor.particles.catalog import build_preset
from ui.editor.particles.model import ParticleSupportLevel, ParticleSystemModel, build_particle_models


def _new_nif() -> NifFile:
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root", "Children": [], "Num Children": 0})
    return nif


def _modifier_blocks(nif: NifFile, system_block_id: int):
    system = nif.get_block(system_block_id)
    return [nif.get_block(block_id) for block_id in system.get_field("Modifiers")]


def _schema_field_names(type_name: str) -> set[str]:
    return {name for name, _value in NifFile().add_block(type_name).fields}


def _representative_model(**overrides) -> ParticleSystemModel:
    values = {
        "nif_id": "main",
        "system_block_id": 10,
        "name": "Imported Sparks",
        "data_block_id": 11,
        "shader_property_block_id": 12,
        "alpha_property_block_id": 13,
        "controller_block_id": 14,
        "emitter_block_id": 15,
        "emitter_type": "NiPSysSphereEmitter",
        "modifier_block_ids": (15, 16),
        "modifier_types": ("NiPSysSphereEmitter", "NiPSysAgeDeathModifier"),
        "source_texture": r"textures\effects\sparks.dds",
        "base_color": (0.25, 0.5, 0.75, 0.6),
        "emitter_initial_color": (0.9, 0.8, 0.7, 0.4),
        "emitter_speed": 4.5,
        "emitter_declination_variation": math.radians(10.0),
        "emitter_planar_angle_variation": math.radians(35.0),
        "emitter_lifetime": 2.25,
        "emitter_initial_radius": 0.35,
        "emitter_radius": 0.75,
        "emitter_object_block_id": 2,
        "atlas_offsets": (
            (0.0, 0.5, 0.0, 0.5),
            (0.5, 1.0, 0.0, 0.5),
            (0.0, 0.5, 0.5, 1.0),
            (0.5, 1.0, 0.5, 1.0),
        ),
        "support_level": ParticleSupportLevel.SUPPORTED,
    }
    values.update(overrides)
    return ParticleSystemModel(**values)


class _SessionApp:
    def __init__(self, nif: NifFile | None, read_only: bool = False):
        session = SimpleNamespace(nif=nif, dirty=False, read_only=read_only)
        self.registry = SimpleNamespace(active_session=session, active_id="main")
        self.undo_manager = MagicMock()
        self.rebuild_scene_from_nif = MagicMock()
        self.selection_mgr = MagicMock()
        self.status_text = ""
        self.session = session


def test_empty_effect_reports_issue_and_invalid_apply_does_not_mutate_nif():
    draft = ParticleEffectDraft(effect_name="Empty", systems=[])
    nif = _new_nif()
    root = nif.get_block(0)
    before_fields = list(root.fields)

    issues = validate_draft(draft)
    result = apply_draft_to_nif(nif, draft)

    assert [issue.message for issue in issues] == ["Effect has no particle systems."]
    assert [issue.message for issue in result.issues] == ["Effect has no particle systems."]
    assert result.system_block_ids == ()
    assert len(nif.blocks) == 1
    assert root.fields == before_fields


def test_draft_from_particle_model_preserves_guided_fields_and_source_metadata():
    model = _representative_model()

    draft = draft_from_particle_model(model)

    assert draft.effect_name == "Imported Sparks"
    assert draft.active_system_index == 0
    assert len(draft.systems) == 1
    system = draft.systems[0]
    assert system.display_name == "Imported Sparks"
    assert system.texture_path == r"textures\effects\sparks.dds"
    assert system.color_rgba == (0.25, 0.5, 0.75, 0.6)
    assert system.alpha == 0.6
    assert system.speed == 4.5
    assert system.lifetime == 2.25
    assert system.particle_size == 0.35
    assert math.isclose(system.spread_degrees, 35.0)
    assert system.emission_shape is EmissionShape.SPHERE
    assert system.atlas_rows == 2
    assert system.atlas_columns == 2
    assert system.subtexture_index == 0
    assert system.raw_overrides["source"] == {
        "nif_id": "main",
        "system_block_id": 10,
        "emitter_block_id": 15,
        "emitter_object_block_id": 2,
    }


def test_draft_from_single_subtexture_offset_preserves_selected_atlas_cell():
    model = _representative_model(atlas_offsets=((0.5, 1.0, 0.0, 0.5),))

    draft = draft_from_particle_model(model)

    system = draft.systems[0]
    assert system.atlas_rows == 2
    assert system.atlas_columns == 2
    assert system.subtexture_index == 1


def test_draft_from_particle_model_preserves_emitter_variations():
    model = _representative_model(
        emitter_speed_variation=1.5,
        emitter_lifetime_variation=0.25,
        emitter_radius_variation=0.125,
    )

    draft = draft_from_particle_model(model)

    emitter_overrides = draft.systems[0].raw_overrides["emitter"]
    assert emitter_overrides == {
        "speed_variation": 1.5,
        "lifetime_variation": 0.25,
        "radius_variation": 0.125,
    }


def test_draft_from_particle_model_imports_known_modifiers_and_preserves_unknown_types():
    model = _representative_model(
        emitter_type="NiPSysBombEmitter",
        modifier_types=(
            "NiPSysBombEmitter",
            "NiPSysAgeDeathModifier",
            "NiPSysPositionModifier",
            "NiPSysBoundUpdateModifier",
            "NiPSysGravityModifier",
            "NiPSysDragModifier",
            "BSWindModifier",
            "NiPSysRotationModifier",
            "BSPSysScaleModifier",
            "NiPSysGrowFadeModifier",
            "BSPSysSimpleColorModifier",
            "NiPSysColorModifier",
            "NiPSysColliderManager",
            "NiPSysSpawnModifier",
            "BSPSysLODModifier",
            "NiPSysBombModifier",
        ),
    )

    draft = draft_from_particle_model(model)

    system = draft.systems[0]
    assert system.emission_shape is EmissionShape.POINT
    assert system.raw_overrides["raw_emitter_type"] == "NiPSysBombEmitter"
    assert [modifier.kind for modifier in system.modifiers] == [
        ModifierKind.GRAVITY,
        ModifierKind.DRAG,
        ModifierKind.WIND,
        ModifierKind.ROTATION,
        ModifierKind.SIZE_OVER_LIFE,
        ModifierKind.SIZE_OVER_LIFE,
        ModifierKind.COLOR_OVER_LIFE,
        ModifierKind.COLOR_OVER_LIFE,
        ModifierKind.COLLISION,
        ModifierKind.SPAWN_RATE,
    ]
    assert system.raw_overrides["raw_modifier_types"] == ("NiPSysBombModifier",)


def test_blank_effect_and_modifier_names_report_issues_and_do_not_mutate_nif():
    system = ParticleSystemDraft(
        display_name="Smoke",
        modifiers=(ParticleModifierDraft(ModifierKind.GRAVITY, " "),),
    )
    draft = ParticleEffectDraft(effect_name=" ", systems=(system,))
    nif = _new_nif()

    result = apply_draft_to_nif(nif, draft)

    assert [(issue.system_index, issue.message) for issue in result.issues] == [
        (None, "Particle effect name is required."),
        (0, "Particle modifier display name is required."),
    ]
    assert result.system_block_ids == ()
    assert len(nif.blocks) == 1


def test_apply_draft_to_session_records_snapshot_and_updates_editor_hooks():
    nif = _new_nif()
    app = _SessionApp(nif)
    draft = ParticleEffectDraft(effect_name="Session Sparks", systems=[
        ParticleSystemDraft(display_name="Session Sparks", emission_shape=EmissionShape.SPHERE),
    ])

    result = apply_draft_to_session(app, draft)

    assert result.issues == ()
    assert len(result.system_block_ids) == 1
    assert len(nif.blocks) > 1
    app.undo_manager.push.assert_called_once()
    active_id, action = app.undo_manager.push.call_args.args
    assert active_id == "main"
    assert isinstance(action, SnapshotAction)
    assert len(action._before_blocks) == 1
    assert len(action._after_blocks) == len(nif.blocks)
    assert app.session.dirty is True
    app.rebuild_scene_from_nif.assert_called_once_with("main")
    app.selection_mgr.select_by_id.assert_called_once_with("main", result.system_block_ids[0])
    assert app.status_text == "Applied particle effect: Session Sparks"


def test_apply_draft_to_session_does_not_write_read_only_sessions():
    nif = _new_nif()
    app = _SessionApp(nif, read_only=True)
    draft = ParticleEffectDraft(effect_name="Read Only", systems=[
        ParticleSystemDraft(display_name="Read Only"),
    ])

    result = apply_draft_to_session(app, draft)

    assert [issue.message for issue in result.issues] == [
        "Cannot apply particle effect to a read-only session.",
    ]
    assert result.system_block_ids == ()
    assert len(nif.blocks) == 1
    app.undo_manager.push.assert_not_called()
    app.rebuild_scene_from_nif.assert_not_called()
    app.selection_mgr.select_by_id.assert_not_called()


def test_apply_draft_to_session_does_not_mutate_when_validation_fails():
    nif = _new_nif()
    app = _SessionApp(nif)
    draft = ParticleEffectDraft(effect_name="Invalid", systems=[])

    result = apply_draft_to_session(app, draft)

    assert [issue.message for issue in result.issues] == ["Effect has no particle systems."]
    assert result.system_block_ids == ()
    assert len(nif.blocks) == 1
    app.undo_manager.push.assert_not_called()
    app.rebuild_scene_from_nif.assert_not_called()
    app.selection_mgr.select_by_id.assert_not_called()
    assert app.status_text == "Effect has no particle systems."


def test_apply_draft_to_session_rolls_back_when_writer_raises():
    nif = _new_nif()
    app = _SessionApp(nif)
    draft = ParticleEffectDraft(effect_name="Bad Collision", systems=[
        ParticleSystemDraft(
            display_name="Bad Collision",
            modifiers=(
                ParticleModifierDraft(ModifierKind.COLLISION, "Bad Collision", settings={"bounce": "invalid"}),
            ),
        ),
    ])

    result = apply_draft_to_session(app, draft)

    assert result.system_block_ids == ()
    assert len(result.issues) == 1
    assert result.issues[0].message.startswith("Failed to apply particle effect:")
    assert len(nif.blocks) == 1
    assert app.session.dirty is False
    app.undo_manager.push.assert_not_called()
    app.rebuild_scene_from_nif.assert_not_called()
    app.selection_mgr.select_by_id.assert_not_called()


def test_add_spawn_modifier_wires_through_age_death_child_ref():
    nif = _new_nif()
    system = ParticleSystemDraft(display_name="Plasma", emission_shape=EmissionShape.SPHERE)
    [system_block_id] = apply_draft_to_nif(
        nif,
        ParticleEffectDraft(effect_name="Plasma", systems=[system]),
    ).system_block_ids
    [model] = build_particle_models(nif, nif_id="main")
    age_death = next(
        block
        for block in _modifier_blocks(nif, system_block_id)
        if block.type_name == "NiPSysAgeDeathModifier"
    )
    age_death.set_field("Spawn Modifier", -1)
    direct_modifiers_before = tuple(nif.get_block(system_block_id).get_field("Modifiers"))

    spawn_block_id = add_modifier_to_particle_system(nif, model, ModifierKind.SPAWN_RATE)

    assert age_death.get_field("Spawn Modifier") == spawn_block_id
    assert tuple(nif.get_block(system_block_id).get_field("Modifiers")) == direct_modifiers_before
    [updated] = build_particle_models(nif, nif_id="main")
    assert spawn_block_id in updated.modifier_block_ids
    assert updated.modifier_parent_block_ids[updated.modifier_block_ids.index(spawn_block_id)] == age_death.block_id


def test_add_modifier_to_particle_system_session_records_snapshot_and_refreshes_editor():
    nif = _new_nif()
    [system_block_id] = apply_draft_to_nif(
        nif,
        ParticleEffectDraft(systems=[ParticleSystemDraft(display_name="Smoke")]),
    ).system_block_ids
    app = _SessionApp(nif)
    model = build_particle_models(nif, nif_id="main")[0]

    result = add_modifier_to_particle_system_session(
        app,
        model,
        ModifierKind.GRAVITY,
    )

    assert result.issues == ()
    assert result.system_block_ids == (system_block_id,)
    assert any(block.type_name == "NiPSysGravityModifier" for block in _modifier_blocks(nif, system_block_id))
    app.undo_manager.push.assert_called_once()
    assert isinstance(app.undo_manager.push.call_args.args[1], SnapshotAction)
    assert app.session.dirty is True
    app.rebuild_scene_from_nif.assert_called_once_with("main")


def test_remove_direct_modifier_detaches_it_from_particle_system():
    nif = _new_nif()
    [system_block_id] = apply_draft_to_nif(
        nif,
        ParticleEffectDraft(systems=[ParticleSystemDraft(display_name="Smoke")]),
    ).system_block_ids
    model = build_particle_models(nif, nif_id="main")[0]
    gravity_block_id = add_modifier_to_particle_system(nif, model, ModifierKind.GRAVITY)
    model = build_particle_models(nif, nif_id="main")[0]

    removed = remove_modifier_from_particle_system(nif, model, gravity_block_id)

    assert removed == gravity_block_id
    system = nif.get_block(system_block_id)
    assert gravity_block_id not in system.get_field("Modifiers")
    assert system.get_field("Num Modifiers") == len(system.get_field("Modifiers"))
    [updated] = build_particle_models(nif, nif_id="main")
    assert gravity_block_id not in updated.modifier_block_ids


def test_remove_child_spawn_modifier_clears_parent_ref():
    nif = _new_nif()
    system = ParticleSystemDraft(display_name="Plasma", emission_shape=EmissionShape.SPHERE)
    apply_draft_to_nif(
        nif,
        ParticleEffectDraft(effect_name="Plasma", systems=[system]),
    )
    model = build_particle_models(nif, nif_id="main")[0]
    age_death = next(
        nif.get_block(block_id)
        for block_id in model.modifier_block_ids
        if nif.get_block(block_id).type_name == "NiPSysAgeDeathModifier"
    )
    spawn_block_id = add_modifier_to_particle_system(nif, model, ModifierKind.SPAWN_RATE)
    model = build_particle_models(nif, nif_id="main")[0]

    removed = remove_modifier_from_particle_system(nif, model, spawn_block_id)

    assert removed == spawn_block_id
    assert age_death.get_field("Spawn Modifier") == -1
    [updated] = build_particle_models(nif, nif_id="main")
    assert spawn_block_id not in updated.modifier_block_ids


def test_remove_modifier_from_particle_system_session_records_snapshot_and_selects_system():
    nif = _new_nif()
    [system_block_id] = apply_draft_to_nif(
        nif,
        ParticleEffectDraft(systems=[ParticleSystemDraft(display_name="Smoke")]),
    ).system_block_ids
    model = build_particle_models(nif, nif_id="main")[0]
    gravity_block_id = add_modifier_to_particle_system(nif, model, ModifierKind.GRAVITY)
    model = build_particle_models(nif, nif_id="main")[0]
    app = _SessionApp(nif)

    result = remove_modifier_from_particle_system_session(
        app,
        model,
        gravity_block_id,
    )

    assert result.issues == ()
    assert result.system_block_ids == (system_block_id,)
    app.undo_manager.push.assert_called_once()
    assert isinstance(app.undo_manager.push.call_args.args[1], SnapshotAction)
    assert app.session.dirty is True
    app.rebuild_scene_from_nif.assert_called_once_with("main")
    app.selection_mgr.select_by_id.assert_called_once_with("main", system_block_id)


def test_validation_reports_blank_names_and_invalid_numeric_fields():
    system = ParticleSystemDraft(
        display_name=" ",
        atlas_rows=0,
        atlas_columns=0,
        lifetime=-1.0,
        emission_rate=-2.0,
        speed=-3.0,
        particle_size=0.0,
    )

    issues = validate_draft(ParticleEffectDraft(systems=[system]))

    assert [(issue.system_index, issue.message) for issue in issues] == [
        (0, "Particle system display name is required."),
        (0, "Particle system atlas_rows must be at least 1."),
        (0, "Particle system atlas_columns must be at least 1."),
        (0, "Particle system lifetime cannot be negative."),
        (0, "Particle system emission_rate cannot be negative."),
        (0, "Particle system speed cannot be negative."),
        (0, "Particle system particle_size must be greater than 0."),
    ]


def test_smoke_puff_preset_writes_supported_particle_system_model():
    system = build_preset("smoke_puff").with_updates(
        atlas_rows=2,
        atlas_columns=2,
        subtexture_index=1,
    )
    draft = ParticleEffectDraft(effect_name="Smoke", systems=[system])
    nif = _new_nif()

    result = apply_draft_to_nif(nif, draft)

    assert result.issues == ()
    assert len(result.system_block_ids) == 1
    root = nif.get_block(0)
    assert result.system_block_ids[0] in root.get_field("Children")
    assert root.get_field("Num Children") == 1

    [model] = build_particle_models(nif, nif_id="main")
    system_block = nif.get_block(model.system_block_id)
    emitter_controller = nif.get_block(system_block.get_field("Controller"))
    update_controller = nif.get_block(emitter_controller.get_field("Next Controller"))
    birth_rate = nif.get_block(emitter_controller.get_field("Interpolator"))
    visibility = nif.get_block(emitter_controller.get_field("Visibility Interpolator"))
    visibility_data = nif.get_block(visibility.get_field("Data"))

    assert model.support_level is ParticleSupportLevel.SUPPORTED
    assert model.name == "Smoke Puff"
    assert model.source_texture == r"textures\effects\smoke.dds"
    assert model.base_color == (0.55, 0.55, 0.55, 0.75)
    assert model.atlas_offsets == ((0.5, 1.0, 0.0, 0.5),)
    assert model.emitter_type == "NiPSysSphereEmitter"
    assert model.emitter_speed == 0.35
    assert model.emitter_lifetime == 2.5
    assert model.emitter_radius == 1.4
    assert model.emitter_object_block_id == 0
    assert model.max_particles == 64
    assert emitter_controller.type_name == "NiPSysEmitterCtlr"
    assert emitter_controller.get_field("Target") == model.system_block_id
    assert update_controller.type_name == "NiPSysUpdateCtlr"
    assert update_controller.get_field("Target") == model.system_block_id
    assert birth_rate.type_name == "NiFloatInterpolator"
    assert birth_rate.get_field("Value") == system.emission_rate
    assert visibility.type_name == "NiBoolInterpolator"
    assert visibility_data.type_name == "NiBoolData"


def test_authored_particle_system_survives_save_reload(tmp_path):
    path = tmp_path / "authored_particles.nif"
    system = build_preset("smoke_puff").with_updates(
        atlas_rows=2,
        atlas_columns=2,
        subtexture_index=1,
    )
    nif = _new_nif()

    result = apply_draft_to_nif(nif, ParticleEffectDraft(effect_name="Smoke", systems=[system]))
    nif.save(str(path))
    reloaded = NifFile.load(str(path))
    [model] = build_particle_models(reloaded, nif_id="reloaded")

    assert result.system_block_ids
    assert model.name == "Smoke Puff"
    assert model.source_texture == r"textures\effects\smoke.dds"
    assert model.atlas_offsets == ((0.5, 1.0, 0.0, 0.5),)
    assert model.emitter_type == "NiPSysSphereEmitter"
    assert math.isclose(model.emitter_speed, 0.35, abs_tol=1e-6)
    assert math.isclose(model.emitter_lifetime, 2.5, abs_tol=1e-6)
    assert math.isclose(model.emitter_radius, 1.4, abs_tol=1e-6)
    assert model.modifier_types[:4] == (
        "NiPSysSphereEmitter",
        "NiPSysAgeDeathModifier",
        "NiPSysPositionModifier",
        "NiPSysBoundUpdateModifier",
    )


def test_emission_shapes_choose_expected_emitter_type_and_target_system():
    shape_cases = [
        (EmissionShape.BOX, "NiPSysBoxEmitter"),
        (EmissionShape.CYLINDER, "NiPSysCylinderEmitter"),
        (EmissionShape.SPHERE, "NiPSysSphereEmitter"),
    ]

    for shape, expected_type in shape_cases:
        nif = _new_nif()
        draft = ParticleEffectDraft(systems=[
            ParticleSystemDraft(display_name=shape.value, emission_shape=shape),
        ])

        result = apply_draft_to_nif(nif, draft)

        [model] = build_particle_models(nif)
        emitter = nif.get_block(model.emitter_block_id)
        assert result.system_block_ids == (model.system_block_id,)
        assert model.emitter_type == expected_type
        assert emitter.get_field("Target") == model.system_block_id


def test_explicit_modifiers_are_written_after_lifecycle_modifiers_with_targets():
    modifiers = (
        ParticleModifierDraft(ModifierKind.GRAVITY, "Gravity", settings={"strength": 2.0}),
        ParticleModifierDraft(ModifierKind.DRAG, "Drag", settings={"amount": 0.25}),
        ParticleModifierDraft(ModifierKind.WIND, "Wind", settings={"strength": 0.75}),
        ParticleModifierDraft(ModifierKind.ROTATION, "Spin", settings={"degrees_per_second": 180.0}),
    )
    draft = ParticleEffectDraft(systems=[
        ParticleSystemDraft(display_name="With Modifiers", modifiers=modifiers),
    ])
    nif = _new_nif()

    [system_block_id] = apply_draft_to_nif(nif, draft).system_block_ids

    modifier_blocks = _modifier_blocks(nif, system_block_id)
    modifier_types = [block.type_name for block in modifier_blocks]
    assert modifier_types[:4] == [
        "NiPSysBoxEmitter",
        "NiPSysAgeDeathModifier",
        "NiPSysPositionModifier",
        "NiPSysBoundUpdateModifier",
    ]
    assert "NiPSysGravityModifier" in modifier_types
    assert "NiPSysDragModifier" in modifier_types
    assert "BSWindModifier" in modifier_types
    assert "NiPSysRotationModifier" in modifier_types
    for block in modifier_blocks:
        assert block.get_field("Target") == system_block_id

    by_type = {block.type_name: block for block in modifier_blocks}
    assert by_type["NiPSysGravityModifier"].get_field("Strength") == 2.0
    assert by_type["NiPSysDragModifier"].get_field("Percentage") == 0.25
    assert by_type["BSWindModifier"].get_field("Strength") == 0.75
    assert math.isclose(by_type["NiPSysRotationModifier"].get_field("Rotation Speed"), math.pi)


def test_advanced_modifiers_use_schema_backed_blocks_and_companions():
    modifiers = (
        ParticleModifierDraft(ModifierKind.SIZE_OVER_LIFE, "Size", settings={"curve": (1.0, 0.5, 0.0)}),
        ParticleModifierDraft(
            ModifierKind.COLOR_OVER_LIFE,
            "Color",
            settings={"start_color": (1.0, 0.0, 0.0, 1.0), "end_color": (0.0, 0.0, 1.0, 0.25)},
        ),
        ParticleModifierDraft(ModifierKind.ALPHA_OVER_LIFE, "Fade", settings={"curve": (0.0, 1.0, 0.0)}),
        ParticleModifierDraft(ModifierKind.SPAWN_RATE, "Spawn", settings={"percentage_spawned": 0.75}),
        ParticleModifierDraft(ModifierKind.COLLISION, "Bounce", settings={"bounce": 0.5, "radius": 2.0}),
    )
    draft = ParticleEffectDraft(systems=[
        ParticleSystemDraft(display_name="Advanced", modifiers=modifiers),
    ])
    nif = _new_nif()

    [system_block_id] = apply_draft_to_nif(nif, draft).system_block_ids

    modifier_blocks = _modifier_blocks(nif, system_block_id)
    modifier_types = [block.type_name for block in modifier_blocks]
    assert "BSPSysScaleModifier" in modifier_types
    assert modifier_types.count("BSPSysSimpleColorModifier") == 2
    assert "NiPSysSpawnModifier" in modifier_types
    assert "NiPSysColliderManager" in modifier_types
    for block in modifier_blocks:
        assert set(name for name, _value in block.fields) <= _schema_field_names(block.type_name)

    collision_manager = next(block for block in modifier_blocks if block.type_name == "NiPSysColliderManager")
    collider = nif.get_block(collision_manager.get_field("Collider"))
    assert collider.type_name == "NiPSysSphericalCollider"
    assert collider.get_field("Parent") == collision_manager.block_id
    assert collider.get_field("Collider Object") == 0
    assert collider.get_field("Bounce") == 0.5
    assert collider.get_field("Radius") == 2.0


def test_raw_overrides_are_applied_after_defaults():
    system = ParticleSystemDraft(
        display_name="Override",
        emission_shape=EmissionShape.SPHERE,
        raw_overrides={"emitter": {"Radius": 3.0}},
    )
    nif = _new_nif()

    apply_draft_to_nif(nif, ParticleEffectDraft(systems=[system]))

    [model] = build_particle_models(nif)
    assert math.isclose(model.emitter_radius, 3.0)
