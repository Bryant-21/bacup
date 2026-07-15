from types import SimpleNamespace

from bacup_lib.workflows.asset_phases import _ASSET_PHASE_GATES, _gated_asset_phases


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


def test_default_keeps_standard_asset_phases():
    names = _names(SimpleNamespace())
    assert "Convert Terrain BTOs" not in names
    assert "Convert NIFs" in names
    assert "Convert Textures" in names
    assert "Convert Materials" in names
    assert "Convert Havok" in names
    assert "Postprocess Havok Assets" in names


def test_btos_are_opt_in():
    names = _names(SimpleNamespace(convert_btos=True))
    assert "Convert Terrain BTOs" in names


def test_no_textures_drops_only_textures():
    names = _names(SimpleNamespace(convert_textures=False))
    assert "Convert Textures" not in names
    assert "Convert NIFs" in names
    assert "Convert Materials" in names
    assert "Convert Havok" in names


def test_no_nifs_and_havok():
    names = _names(SimpleNamespace(convert_nifs=False, convert_havok=False))
    assert "Convert NIFs" not in names
    assert "Convert Havok" not in names
    assert "Postprocess Havok Assets" not in names
    assert "Convert Textures" in names
    assert "Resolve Dependencies" in names  # non-asset phases untouched


def test_postprocess_havok_dropped_with_havok():
    names = _names(SimpleNamespace(convert_havok=False))
    assert "Postprocess Havok Assets" not in names
    assert "Convert Havok" not in names


def test_gate_map_covers_asset_phases():
    assert set(_ASSET_PHASE_GATES) == {
        "Convert Terrain BTOs",
        "Convert NIFs",
        "Convert Textures",
        "Convert Materials",
        "Convert Havok",
        "Postprocess Havok Assets",
    }
