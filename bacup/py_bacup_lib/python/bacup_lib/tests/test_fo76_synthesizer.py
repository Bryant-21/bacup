"""Tests for FO76 source record synthesizer."""
from __future__ import annotations

import pytest


def test_synthesize_reverses_remap_formkey():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    fo4_record = {"ImpactDataSet": "18ABDF:Fallout4.esm", "FULL": "Gun"}
    fo76_record = synth.synthesize(fo4_record, "WEAP")
    assert fo76_record["ImpactDataSet"] == "18ABDF:SeventySix.esm"


def test_synthesize_reverses_remap_formkey_list():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    fo4_record = {"Keywords": ["092A86:Fallout4.esm", "000003:GaussShotgun.esp"]}
    fo76_record = synth.synthesize(fo4_record, "WEAP")
    # Only Fallout4.esm refs get swapped; custom ESP refs stay
    assert fo76_record["Keywords"] == ["092A86:SeventySix.esm", "000003:GaussShotgun.esp"]


def test_synthesize_adds_back_stripped_subfields():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    fo4_record = {"MODL": {"File": "gun.nif", "Data": "0x04"}}
    fo76_record = synth.synthesize(fo4_record, "WEAP")
    # Should have XFLG, ENLT, ENLS, AUUV, MODD added back
    assert "XFLG" in fo76_record["MODL"]
    assert "ENLT" in fo76_record["MODL"]


def test_synthesize_adds_dropped_fields():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    fo4_record = {"Name": "Gun"}
    fo76_record = synth.synthesize(fo4_record, "WEAP")
    # Dropped fields should be present with dummy values. Names are the
    # FO76 native-emitted authoring keys (after sig→friendly normalization).
    assert "EligibleLevels" in fo76_record
    assert "BreakSound" in fo76_record
    assert "AimAssistModel" in fo76_record


def test_synthesize_reverses_scale():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    fo4_record = {
        "Data": {
            "MinRange": 12.226,
            "MaxRange": 8.498,
            "MinPowerPerShot": 1.0,
        }
    }
    fo76_record = synth.synthesize(fo4_record, "WEAP")
    assert fo76_record["Data"]["MinRange"] == pytest.approx(100.0)
    assert fo76_record["Data"]["MaxRange"] == pytest.approx(100.0)
    assert fo76_record["Data"]["MinPowerPerShot"] == pytest.approx(10.0)


def test_synthesize_reverses_flatten_curvetable():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    # Use a synthetic map with flatten_curvetable — the real fo76_to_fo4 map
    # doesn't use it (those fields are passthrough).
    synth._maps = {
        "TestType": {
            "fields": {},
            "transforms": {
                "CurveField": {"type": "flatten_curvetable", "default": 0}
            },
            "drop": [],
            "defaults": {},
        }
    }

    fo4_record = {"CurveField": 256}
    fo76_record = synth.synthesize(fo4_record, "TestType")
    # Should be replaced with a synthetic CurveTable FK
    assert isinstance(fo76_record["CurveField"], str)
    assert "SeventySix.esm" in fo76_record["CurveField"]


def test_synthesize_adds_extra_language():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    fo4_record = {
        "FULL": {
            "TargetLanguage": "English",
            "Values": [
                {"Language": "English", "String": "Gun"},
            ],
        }
    }
    fo76_record = synth.synthesize(fo4_record, "WEAP")
    langs = [v["Language"] for v in fo76_record["FULL"]["Values"]]
    assert "ChineseSimplified" in langs


def test_synthesize_does_not_mutate_input():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    synth.load_map("fo76", "fo4")

    original = {"MODL": {"File": "gun.nif", "Data": "0x04"}, "FULL": "Gun"}
    original_model_keys = set(original["MODL"].keys())

    synth.synthesize(original, "WEAP")

    # Original must NOT be mutated
    assert set(original["MODL"].keys()) == original_model_keys, (
        f"Synthesizer mutated input! Model keys: {set(original['MODL'].keys())}"
    )


def test_synthesize_removes_defaults():
    from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer

    synth = FO76Synthesizer()
    # ExtraData is a default — if a field matches its default, strip it
    default_extra = {
        "AnimationFireSeconds": 1.0e-05,
        "RumbleLeftMotorStrength": 0.5,
        "RumbleRightMotorStrength": 0.5,
        "RumbleDuration": 0.2,
        "AnimationReloadSeconds": 2.0,
        "SightedTransitionSeconds": 0.2,
        "NumProjectiles": 1,
    }
    synth._maps = {
        "TestType": {
            "fields": {},
            "transforms": {},
            "drop": [],
            "defaults": {"ExtraData": default_extra},
        }
    }
    fo4_record = {
        "Name": "Gun",
        "ExtraData": default_extra,
    }
    fo76_record = synth.synthesize(fo4_record, "TestType")
    assert "ExtraData" not in fo76_record
