from types import SimpleNamespace

import pytest

from bacup_lib.workflows.asset_phases import _gated_asset_phases


def _phases():
    names = [
        "Resolve Dependencies", "Translate Records", "Convert Terrain BTOs",
        "Convert NIFs", "Convert Textures", "Extract ATX", "Convert Materials",
        "Convert Havok", "Convert Animations", "Convert Skeleton",
        "Synthesize Behavior Drivers", "Postprocess Havok Assets", "Scaffold Mod", "Build ESP",
    ]
    return [(i + 1, n, None) for i, n in enumerate(names)]


def _names(orch):
    return [name for (_num, name, _fn) in _gated_asset_phases(orch, _phases())]


@pytest.mark.parametrize(
    "options,present,absent",
    [
        (
            {},
            {"Convert NIFs", "Convert Textures", "Convert Materials", "Convert Havok", "Postprocess Havok Assets"},
            {"Convert Terrain BTOs"},
        ),
        ({"convert_btos": True}, {"Convert Terrain BTOs"}, set()),
        (
            {"convert_textures": False},
            {"Convert NIFs", "Convert Materials", "Convert Havok"},
            {"Convert Textures"},
        ),
        (
            {"convert_nifs": False, "convert_havok": False},
            {"Convert Textures", "Resolve Dependencies"},
            {"Convert NIFs", "Convert Havok", "Postprocess Havok Assets"},
        ),
    ],
)
def test_asset_phase_gating(options, present, absent):
    names = set(_names(SimpleNamespace(**options)))
    assert present <= names
    assert not (absent & names)
