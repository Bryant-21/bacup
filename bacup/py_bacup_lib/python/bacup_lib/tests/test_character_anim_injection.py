"""Tests for character.hkx animation injection."""
from pathlib import Path

SNALLYGASTER_CHARACTER_FIXTURE = Path(
    "py_creation_lib/python/creation_lib/hkxpack/tests/fixtures/fo76_snallygastercharacter.hkx"
)


def test_inject_animation_names_preserves_character_objects(tmp_path):
    from creation_lib._native.havok_native import HKXArrayMember, load_hkx_bytes
    from bacup_lib.orchestrator import _inject_animation_names_into_character_hkx
    from creation_lib.havok_convert.converter import HavokConverter

    converted = tmp_path / "snallygastercharacter.hkx"
    HavokConverter().convert_file(str(SNALLYGASTER_CHARACTER_FIXTURE), str(converted), 53)
    anim_dir = tmp_path / "animations"
    anim_dir.mkdir()

    injected = _inject_animation_names_into_character_hkx(
        str(converted),
        str(anim_dir),
        {"Animations\\Attack1.hkt", "Animations\\Idle.hkt"},
    )

    assert injected == 2
    hkx, _registry = load_hkx_bytes(converted.read_bytes())
    assert len(hkx.objects) > 0
    asset_names = set()
    for obj in hkx.objects:
        if obj.class_name != "hkbCharacterStringData":
            continue
        for member in obj.members:
            if isinstance(member, HKXArrayMember) and member.name == "animationBundleNameData":
                bundle = member.contents[0]
                for bundle_member in bundle.members:
                    if isinstance(bundle_member, HKXArrayMember) and bundle_member.name == "assetNames":
                        asset_names.update(bundle_member.contents)
    assert asset_names == {"Animations\\Attack1.hkt", "Animations\\Idle.hkt"}
