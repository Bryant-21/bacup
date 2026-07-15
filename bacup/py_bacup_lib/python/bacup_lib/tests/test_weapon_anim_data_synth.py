"""Tests for WEAP AnimSubgraph wiring."""
from __future__ import annotations

from bacup_lib.animation.weapon_anim_data_synth import (
    wire_weap_animation,
)


def _weap() -> dict:
    return {
        "eid": "WeapNV10mmPistol",
        "fields": [
            {"FULL": {"TargetLanguage": "English", "Values": [{"Value": "10mm Pistol"}]}}
        ],
    }


def test_wire_attaches_subgraph_and_overlay():
    weap = wire_weap_animation(
        _weap(),
        family_subgraph="AnimSubgraph_PipeGun",
        overlay_relpath="Meshes/AnimsTextData/B21_NV10mm_FireAdditive.hkx",
        rest_pose_relpath=None,
    )
    fields = {next(iter(entry)): next(iter(entry.values())) for entry in weap["fields"]}
    assert fields["AnimSubgraph"] == "AnimSubgraph_PipeGun"
    assert (
        fields["AdditiveAnimationOverlay"]
        == "Meshes/AnimsTextData/B21_NV10mm_FireAdditive.hkx"
    )


def test_wire_omits_rest_pose_when_absent():
    weap = wire_weap_animation(
        _weap(),
        family_subgraph="AnimSubgraph_PipeGun",
        overlay_relpath=None,
        rest_pose_relpath=None,
    )
    keys = {next(iter(entry)) for entry in weap["fields"]}
    assert "AnimRestPose" not in keys
