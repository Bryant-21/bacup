import math
from types import SimpleNamespace

import pytest

from ui.editor.animation import AnimationManager
from ui.editor.panels.animation_editor import AnimationEditorPanel


class _Block:
    def __init__(self, block_id=0, **fields):
        self.block_id = block_id
        self._fields = fields

    def get_field(self, name):
        return self._fields.get(name)


def test_playback_parses_xyz_rotation_keys():
    manager = AnimationManager()
    data = _Block(
        **{
            "XYZ Rotations": [
                {"Keys": [{"Time": 0.0, "Value": 0.0}]},
                {"Keys": [{"Time": 0.0, "Value": 0.0}]},
                {
                    "Keys": [
                        {"Time": 0.0, "Value": 0.0},
                        {"Time": 1.0, "Value": math.pi / 2.0},
                    ]
                },
            ],
        }
    )

    keys = {}
    manager._extract_rotation_keys(data, keys)

    assert sorted(keys) == [0.0, 1.0]
    assert keys[1.0].rot == pytest.approx(
        (math.sqrt(0.5), 0.0, 0.0, math.sqrt(0.5))
    )


def test_animation_editor_lists_xyz_rotation_channels():
    panel = AnimationEditorPanel(SimpleNamespace())
    data = _Block(
        1,
        **{
            "XYZ Rotations": [
                {"Interpolation": 1, "Keys": [{"Time": 0.0, "Value": 0.0}]},
                {"Interpolation": 1, "Keys": [{"Time": 0.0, "Value": 0.0}]},
                {
                    "Interpolation": 2,
                    "Keys": [
                        {"Time": 0.0, "Value": -math.pi / 2.0},
                        {"Time": 1.0, "Value": 0.0},
                    ],
                },
            ],
        },
    )
    interp = _Block(7, Data=1)
    nif = SimpleNamespace(get_block=lambda block_id: data if block_id == 1 else None)

    channels = panel._parse_transform_channels(nif, interp, "Door")

    assert [channel.component for channel in channels] == ["rot_x", "rot_y", "rot_z"]
    rot_z = channels[2]
    assert rot_z.label == "Door : Rot Z"
    assert [key.value for key in rot_z.keys] == pytest.approx([-math.pi / 2.0, 0.0])
