from types import SimpleNamespace

import glm

from creation_lib.renderer.scene_renderer import Material, SceneNode
from ui.editor.animation import AnimationManager, AnimSequence, ControlledChannel, FloatKey


def test_sequence_float_material_channel_updates_uv_offset():
    material = Material()
    node = SceneNode(name="Plane032:0", block_id=74)
    node.mesh = SimpleNamespace(material=material)
    channel = ControlledChannel(
        node_name="Plane032:0",
        float_keys=[FloatKey(0.0, 0.0), FloatKey(1.0, 9.0)],
        material_var="V Offset",
    )
    manager = AnimationManager()
    manager._node_cache = {"Plane032:0": node}
    manager._current_seq = AnimSequence(
        name="laser",
        start_time=0.0,
        stop_time=1.0,
        cycle_type=2,
        channels=[channel],
    )

    manager._apply_frame(1.0)

    assert material.uv_scale_offset.w == 9.0


def test_sequence_color_material_channel_updates_emissive_color():
    material = Material()
    material.emissive_color = glm.vec4(1.0, 1.0, 1.0, 1.0)
    node = SceneNode(name="RadsCountTens:0", block_id=74)
    node.mesh = SimpleNamespace(material=material)
    channel = ControlledChannel(node_name="RadsCountTens:0")
    channel.material_color_var = "Emissive Color"
    channel.color_keys = [
        SimpleNamespace(time=0.0, value=(1.0, 1.0, 1.0)),
        SimpleNamespace(time=1.0, value=(0.2, 0.3, 0.4)),
    ]
    manager = AnimationManager()
    manager._node_cache = {"RadsCountTens:0": node}
    manager._current_seq = AnimSequence(
        name="DisplayFlicker",
        start_time=0.0,
        stop_time=1.0,
        cycle_type=2,
        channels=[channel],
    )

    manager._apply_frame(1.0)

    assert tuple(round(float(v), 4) for v in material.emissive_color) == (
        0.2,
        0.3,
        0.4,
        1.0,
    )
