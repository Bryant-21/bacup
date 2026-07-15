from types import SimpleNamespace

from creation_lib.nif.nif_file import NifFile
from creation_lib.renderer.scene_renderer import SceneNode

from ui.editor.animation import AnimationManager
from ui.editor.animation_authoring import (
    AuthoringTarget,
    ControllerChainSpec,
    LinkContext,
    ValueKind,
    add_controller_chain,
)
from ui.editor.animation_effects import build_effect_stacks
from ui.editor.panels.animation_editor import AnimationEditorPanel

def _app(nif, manager):
    return SimpleNamespace(
        nif=nif,
        animation_mgr=manager,
        undo_manager=SimpleNamespace(push=lambda *args, **kwargs: None),
        registry=SimpleNamespace(
            active_id="main",
            active_session=SimpleNamespace(anim_manager=manager),
        ),
    )


def test_authored_standalone_chain_imports_as_normal_effect_stack():
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    nif.add_block("BSTriShape", {"Name": "fx1:0", "Shader Property": prop.block_id})
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "V Offset"},
        start_time=0.0,
        stop_time=1.0,
    )
    add_controller_chain(nif, spec)
    manager = AnimationManager()
    manager.scan(nif)
    panel = AnimationEditorPanel(_app(nif, manager))

    panel._load_property_controllers()

    assert [stack.effect_type for stack in build_effect_stacks(panel._sequence)] == ["Texture Scroll"]
    channel = panel._sequence.channels[0]
    assert channel.controller_block_id >= 0
    assert channel.interp_block_id >= 0
    assert channel.data_block_id >= 0


def test_authored_sequence_chain_imports_as_normal_effect_stack():
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {"Name": "x_scroll", "Start Time": 0.0, "Stop Time": 0.0, "Cycle Type": 0, "Num Controlled Blocks": 0, "Controlled Blocks": []},
    )
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.SEQUENCE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "U Offset"},
        start_time=0.0,
        stop_time=1.0,
        sequence_block_id=seq.block_id,
        node_name="fx1:0",
    )
    add_controller_chain(nif, spec)
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))

    loaded = panel._load_sequence_from_nif(nif, seq.block_id)

    assert [stack.effect_type for stack in build_effect_stacks(loaded)] == ["Texture Scroll"]
    assert loaded.channels[0].controller_block_id >= 0
    assert loaded.channels[0].target_property == "U Offset"


def test_standalone_import_preserves_actual_controller_type_metadata():
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSLightingShaderProperty")
    nif.add_block("BSTriShape", {"Name": "LightPlane:0", "Shader Property": prop.block_id})
    spec = ControllerChainSpec(
        controller_type="BSLightingShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "LightPlane:0", "lighting_shader_property", "BSLightingShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "U Offset"},
        start_time=0.0,
        stop_time=1.0,
    )
    result = add_controller_chain(nif, spec)
    manager = AnimationManager()
    manager.scan(nif)
    panel = AnimationEditorPanel(_app(nif, manager))

    panel._load_property_controllers()

    channel = panel._sequence.channels[0]
    assert channel.controller_block_id == result.controller_id
    assert channel.controller_type == "BSLightingShaderPropertyFloatController"
    assert channel.property_type == "BSLightingShaderProperty"


def test_sequence_import_prefers_actual_controller_ref_type_over_stale_metadata():
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSLightingShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {"Name": "x_light", "Start Time": 0.0, "Stop Time": 0.0, "Cycle Type": 0, "Num Controlled Blocks": 0, "Controlled Blocks": []},
    )
    spec = ControllerChainSpec(
        controller_type="BSLightingShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "LightPlane:0", "lighting_shader_property", "BSLightingShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.SEQUENCE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "U Offset"},
        sequence_block_id=seq.block_id,
        node_name="LightPlane:0",
    )
    add_controller_chain(nif, spec)
    controlled = seq.get_field("Controlled Blocks")
    controlled[0]["Controller Type"] = "BSEffectShaderPropertyFloatController"
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))

    loaded = panel._load_sequence_from_nif(nif, seq.block_id)

    assert loaded.channels[0].controller_type == "BSLightingShaderPropertyFloatController"


def test_scan_discovers_standalone_transform_chain_as_synthetic_sequence():
    nif = NifFile()
    root = nif.add_block("BSFadeNode", {"Name": "Root"})
    spec = ControllerChainSpec(
        controller_type="NiTransformController",
        target=AuthoringTarget(root.block_id, "Root", "node"),
        value_kind=ValueKind.TRANSFORM,
        interpolator_type="NiTransformInterpolator",
        data_type="NiTransformData",
        link_context=LinkContext.STANDALONE,
        keys=[
            (0.0, {"translation": (0.0, 0.0, 0.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0}),
            (1.0, {"translation": (3.0, 4.0, 5.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0}),
        ],
        start_time=0.0,
        stop_time=1.0,
    )
    add_controller_chain(nif, spec)
    manager = AnimationManager()

    manager.scan(nif)

    assert manager.has_sequence("[Legacy Transform Controllers]")
    assert manager.get_sequences() == ["[Legacy Transform Controllers]"]
    assert manager.current_sequence is None


def test_synthetic_transform_sequence_playback_updates_target_node():
    nif = NifFile()
    root = nif.add_block("BSFadeNode", {"Name": "Root"})
    spec = ControllerChainSpec(
        controller_type="NiTransformController",
        target=AuthoringTarget(root.block_id, "Root", "node"),
        value_kind=ValueKind.TRANSFORM,
        interpolator_type="NiTransformInterpolator",
        data_type="NiTransformData",
        link_context=LinkContext.STANDALONE,
        keys=[
            (0.0, {"translation": (0.0, 0.0, 0.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0}),
            (1.0, {"translation": (3.0, 4.0, 5.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0}),
        ],
        start_time=0.0,
        stop_time=1.0,
    )
    add_controller_chain(nif, spec)
    manager = AnimationManager()
    manager.scan(nif)
    manager.loop = False

    scene_root = SceneNode(name="nif_root", block_id=-1)
    animated = SceneNode(name="Root", block_id=root.block_id)
    scene_root.children.append(animated)

    manager.play("[Legacy Transform Controllers]")
    manager.update(1.0, scene_root)

    assert manager.is_playing is False
    assert tuple(round(float(animated.transform[3][i]), 4) for i in range(3)) == (
        3.0,
        4.0,
        5.0,
    )
