"""Tests for FNV weapon -> FO4 family classification."""
from __future__ import annotations

from bacup_lib.animation.weapon_family_classifier import (
    classify_weapon,
    family_subgraph,
)


def test_known_weapon_returns_curated_family():
    family, bones, remap = classify_weapon(
        weap_eid="WeapNV10mmPistol", animation_type="Pistol"
    )
    assert family == "PipeGun"
    assert "Bip01 Magazine" in bones
    assert remap["Bip01 Magazine"] == "WeaponMagazine"
    assert family_subgraph(family) == "AnimSubgraph_PipeGun"


def test_unknown_pistol_falls_back_to_unclassified_pistol_family():
    family, _, _ = classify_weapon(
        weap_eid="WeapNV12_7mmPistol", animation_type="Pistol"
    )
    assert family == "PipeGun"


def test_unknown_rifle_falls_back_to_unclassified_rifle_family():
    family, _, _ = classify_weapon(
        weap_eid="WeapNVMedicineStick", animation_type="Rifle"
    )
    assert family == "HuntingRifle"


def test_unknown_with_no_anim_type_returns_unclassified():
    family, bones, remap = classify_weapon(
        weap_eid="WeapNVMystery", animation_type=""
    )
    assert family == "B21_FNVUnclassified"
    assert bones == []
    assert remap == {}
    assert family_subgraph(family) == "B21_FNVUnclassified"
