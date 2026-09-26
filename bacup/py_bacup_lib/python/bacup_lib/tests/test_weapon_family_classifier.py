"""Tests for FNV weapon -> FO4 family classification."""
from __future__ import annotations

import pytest

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


@pytest.mark.parametrize(
    "weap_eid,animation_type,expected",
    [
        ("WeapNV12_7mmPistol", "Pistol", "PipeGun"),
        ("WeapNVMedicineStick", "Rifle", "HuntingRifle"),
    ],
)
def test_unknown_weapon_falls_back_to_animation_type_family(weap_eid, animation_type, expected):
    family, _, _ = classify_weapon(weap_eid=weap_eid, animation_type=animation_type)
    assert family == expected


def test_unknown_with_no_anim_type_returns_unclassified():
    family, bones, remap = classify_weapon(
        weap_eid="WeapNVMystery", animation_type=""
    )
    assert family == "B21_FNVUnclassified"
    assert bones == []
    assert remap == {}
    assert family_subgraph(family) == "B21_FNVUnclassified"
