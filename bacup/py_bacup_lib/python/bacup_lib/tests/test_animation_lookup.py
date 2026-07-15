"""Tests for animation lookup via Race subgraph data."""
from __future__ import annotations

from bacup_lib.animation.lookup import _is_fallback_path, is_grip_keyword


def test_is_grip_keyword():
    assert is_grip_keyword("AnimsGripPistol")
    assert is_grip_keyword("AnimsGripRifleStraight")
    assert is_grip_keyword("AnimsGripHeavy")
    assert not is_grip_keyword("AnimsGaussPistol")
    assert not is_grip_keyword("Anims44")
    assert not is_grip_keyword("AnimsBlackPowderRifle")
    assert not is_grip_keyword("")


def test_fallback_path_filter():
    assert _is_fallback_path("Actors/Character/Animations/Weapon/GaussShotgun")
    assert _is_fallback_path("Actors/Character/_1stPerson/Animations/GaussShotgun")
    assert not _is_fallback_path("Actors/Character/Animations/Common")
    assert not _is_fallback_path("Actors/Character/Animations/Player")
    assert not _is_fallback_path("Actors/Character/Animations")
    assert not _is_fallback_path("Actors/Character/_1stPerson/Animations/Paired")
    # Creature-specific paths should pass — they ARE the content we want
    # when converting that creature or its weapons directly.
    # (The walker prevents these from leaking via Race → UnarmedWeapon chains.)
    assert _is_fallback_path("Actors/Snallygaster/Animations")
    assert _is_fallback_path("Actors/Deathclaw/Animations/Attack")
    assert _is_fallback_path("Actors/ZetanInvader/Animations")
    assert _is_fallback_path("Actors/DLC04Scorchbeast/Animations")
    # Non-actor paths should pass
    assert _is_fallback_path("Weapons/GaussShotgun/Animations")


def test_fallback_path_weapon_name_filtering():
    """With weapon_name, only dirs containing the weapon name pass."""
    # Matching weapon name — passes
    assert _is_fallback_path(
        "Actors/Character/Animations/Weapon/GaussPistol",
        weapon_name="GaussPistol",
    )
    assert _is_fallback_path(
        "Actors/Character/_1stPerson/Animations/GaussPistol",
        weapon_name="GaussPistol",
    )
    # Wrong weapon — rejected
    assert not _is_fallback_path(
        "Actors/PowerArmor/Animations/Weapons/GammaGun",
        weapon_name="GaussPistol",
    )
    # Shared grip dir — rejected
    assert not _is_fallback_path(
        "Actors/Character/Animations/Weapon/Pistol",
        weapon_name="GaussPistol",
    )
    assert not _is_fallback_path(
        "Actors/PowerArmor/Animations/Grips/Pistol",
        weapon_name="GaussPistol",
    )
    # Non-actor paths always pass (even with weapon_name)
    assert _is_fallback_path(
        "Weapons/SomeOtherWeapon/Animations",
        weapon_name="GaussPistol",
    )
    # Creature paths always pass
    assert _is_fallback_path(
        "Actors/Snallygaster/Animations",
        weapon_name="GaussPistol",
    )


def test_fallback_path_race_subdirs_with_weapon_name():
    """Player/Synth subdirs pass when nested under matching weapon folder."""
    # Player under weapon folder — passes with matching weapon_name
    assert _is_fallback_path(
        "Actors/Character/Animations/Weapon/GaussPistol/Player",
        weapon_name="GaussPistol",
    )
    assert _is_fallback_path(
        "Actors/Character/Animations/Weapon/GaussPistol/Synth",
        weapon_name="GaussPistol",
    )
    # Player at top level — still rejected
    assert not _is_fallback_path(
        "Actors/Character/Animations/Player",
        weapon_name="GaussPistol",
    )
    # Player under wrong weapon — rejected (parent doesn't match)
    assert not _is_fallback_path(
        "Actors/Character/Animations/Weapon/GammaGun/Player",
        weapon_name="GaussPistol",
    )


def test_fallback_path_emotes_subdirectory():
    """Emotes/Female subdirectory must be excluded."""
    assert not _is_fallback_path("Actors/Character/Animations/Common/Emotes/Female")
    assert not _is_fallback_path("Actors/Character/Animations/Common/Something")
    assert not _is_fallback_path("Actors/Character/Animations/Emotes")
    assert not _is_fallback_path("Actors/Character/Animations/Common")
