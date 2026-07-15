"""Regression test for FO76→FO4 character.hkx rigName rewrite.

FO76 ships zSingleBoneSkeleton one level above UniqueBehaviors, so its
weapon FX character files reference it via "..\\zSingleBoneSkeleton\\...".
FO4 ships the same skeleton under GenericBehaviors (two levels up), so
the path must be rewritten to "..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\...".
"""
from __future__ import annotations

import os

import pytest


FO76_SRC = (
    "extracted/fo76/meshes/uniquebehaviors/meltdownfx/characters/character00.hkx"
)


@pytest.mark.skipif(
    not os.path.isfile(FO76_SRC),
    reason="FO76 MeltdownFX extract not available",
)
def test_fix_character_rig_path_rewrites_fo76_sibling_to_fo4_genericbehaviors(
    tmp_path,
):
    """A freshly-version-converted FO76 character should have its rigName rewritten."""
    from pathlib import Path

    from bacup_lib.orchestrator import _fix_character_rig_path_fo4
    from creation_lib.havok_convert.converter import HavokConverter
    from creation_lib.havok_convert.versions import FO4
    from creation_lib.hkxpack import load_hkx_bytes

    # Convert FO76 TAG0 character → FO4 packfile format so we can read it
    # with our hkxpack reader.
    out = tmp_path / "character00.hkx"
    converter = HavokConverter()
    converter.convert_file(FO76_SRC, str(out), FO4.id)

    # Sanity: pre-fix, rigName should still be the FO76 sibling reference.
    hkx, _ = load_hkx_bytes(Path(out).read_bytes())
    pre_rig = _find_rig_name(hkx)
    assert pre_rig is not None
    assert pre_rig.lower().replace("/", "\\").startswith(
        "..\\zsingleboneskeleton\\"
    ), f"unexpected pre-fix rigName: {pre_rig!r}"

    result = _fix_character_rig_path_fo4(str(out))
    assert result == (
        "..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt"
    )

    hkx, _ = load_hkx_bytes(Path(out).read_bytes())
    post_rig = _find_rig_name(hkx)
    assert post_rig == result


def test_fix_character_rig_path_noop_when_already_fo4_shaped(tmp_path):
    """If the rigName is already FO4-shaped, the function returns None."""
    from bacup_lib.orchestrator import _fix_character_rig_path_fo4

    # Start from the FO76 file (if available) and pre-rewrite manually.
    if not os.path.isfile(FO76_SRC):
        pytest.skip("FO76 MeltdownFX extract not available")

    from creation_lib.havok_convert.converter import HavokConverter
    from creation_lib.havok_convert.versions import FO4

    out = tmp_path / "character00.hkx"
    converter = HavokConverter()
    converter.convert_file(FO76_SRC, str(out), FO4.id)

    # First pass rewrites.
    first = _fix_character_rig_path_fo4(str(out))
    assert first is not None
    # Second pass is a no-op.
    second = _fix_character_rig_path_fo4(str(out))
    assert second is None


def _find_rig_name(hkx) -> str | None:
    from creation_lib.hkxpack.model import HKXStringMember

    for obj in hkx.objects:
        if obj.class_name != "hkbCharacterStringData":
            continue
        for m in obj.members:
            if isinstance(m, HKXStringMember) and m.name == "rigName":
                return m.value
    return None
