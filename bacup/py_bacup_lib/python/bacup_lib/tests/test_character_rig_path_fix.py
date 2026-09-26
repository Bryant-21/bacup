"""Regression test for FO76→FO4 character.hkx rigName rewrite.

FO76 ships zSingleBoneSkeleton one level above UniqueBehaviors, so its
weapon FX character files reference it via "..\\zSingleBoneSkeleton\\...".
FO4 ships the same skeleton under GenericBehaviors (two levels up), so
the path must be rewritten to "..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\...".
"""
from __future__ import annotations

from pathlib import Path

from creation_lib._native.havok_native import HKXStringMember, load_hkx_bytes, write_hkx

from bacup_lib.orchestrator import _fix_character_rig_path_fo4

CHARACTER = (
    Path(__file__).resolve().parents[3]
    / "native/conversion/src/test_fixtures/havok_postprocess/floatercharacter.hkx"
)


def _rig_name_member(hkx) -> HKXStringMember:
    return next(
        member
        for obj in hkx.objects
        if obj.class_name == "hkbCharacterStringData"
        for member in obj.members
        if isinstance(member, HKXStringMember) and member.name == "rigName"
    )


def test_fo76_sibling_rig_path_is_rewritten_once_to_fo4_genericbehaviors(tmp_path):
    hkx, registry = load_hkx_bytes(CHARACTER.read_bytes())
    _rig_name_member(hkx).value = "..\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt"
    character = tmp_path / "character00.hkx"
    character.write_bytes(bytes(write_hkx(hkx, registry)))

    expected = "..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt"
    assert _fix_character_rig_path_fo4(str(character)) == expected
    assert _rig_name_member(load_hkx_bytes(character.read_bytes())[0]).value == expected
    assert _fix_character_rig_path_fo4(str(character)) is None
