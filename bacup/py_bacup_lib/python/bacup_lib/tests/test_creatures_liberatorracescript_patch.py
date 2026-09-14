from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


def _merged() -> str:
    skeleton = (SOURCE_ROOT / "Creatures" / "liberatorracescript.psc").read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source("Creatures:LiberatorRaceScript")
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_liberator_paces_its_laser_off_the_weaponfire_anim_event():
    merged = _merged()
    # The FO76 skeleton keeps the counter, timer id and event name; the patch is
    # only useful if it drives all three.
    assert "RegisterForAnimationEvent(selfActorRef, animEventWeaponFire)" in merged
    assert "currentWeaponFireCount += 1" in merged
    assert "currentWeaponFireCount < WeaponFireMaxShots" in merged
    assert "StartTimer(WeaponFireRestTime as Float, weaponFireTimerID)" in merged


def test_liberator_hands_the_weapon_back_after_the_rest():
    merged = _merged()
    assert "UnequipItem(LiberatorRangedWeapon, True, True)" in merged
    assert "EquipItem(LiberatorRangedWeapon, False, True)" in merged
    # Stowing without a matching re-equip would disarm the liberator permanently.
    assert "Event OnTimer(Int aiTimerID)" in merged


def test_liberator_patch_compiles():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Creatures/LiberatorRaceScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
