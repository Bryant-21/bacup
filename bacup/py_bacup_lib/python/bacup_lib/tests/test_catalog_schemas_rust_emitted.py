"""Hand-written sample documents for the Rust-emitted catalogs, validated against their schemas.

The emitters' own tests (test_fishing_catalog, test_leveled_catalog, test_objective_areas and the Rust
inline tests) validate real output; these samples cover the folders whose emitter is not reachable from
Python and pin what a reader accepts: Plugin.ext:HEX keys, remove entries, no legacy spellings.
"""

import copy

import jsonschema
import pytest

from bacup_lib.tests.catalog_schema_validation import validate_catalog

SAMPLES = {
    "Condition": {
        "version": 1,
        "settings": {"crafting_cost_schema": 1, "source_plugin": "SeventySix.esm", "source_masters": ["Fallout4.esm"],
                     "output_plugin": "SeventySix.esm", "mapping_available": True, "missing_curves": []},
        "records": [{"form": "SeventySix.esm:000800", "source_form_id": "01000800", "signature": "OMOD",
                     "editor_id": "B21_Test", "fields": [{"signature": "DATA", "hex": "0a0b"}]}],
        "target_forms": {"SeventySix.esm:000800": "SeventySix.esm:000900", "SeventySix.esm:000801": None},
    },
    "GoldBullion": {
        "version": 1,
        "settings": {"skipped_unmapped": {"bullion": 0, "stamps": 1}},
        "bullion": [{"form": "SeventySix.esm:000900", "price": 1250}],
        "stamps": [{"form": "SeventySix.esm:000901", "price": 50}, {"form": "SeventySix.esm:000902", "remove": True}],
        "tadpole": [],
    },
    "WeaponModelSwaps": {
        "version": 1,
        "settings": {"source_plugin": "SeventySix.esm"},
        "skins": [{"form": "SeventySix.esm:6C50EB", "editor_id": "mod_skin", "models": [["weapons\a.nif", "atx\b.nif"]]}],
    },
    "NpcStats": {
        "version": 1,
        "settings": {"source_plugin": "SeventySix.esm", "marker": "SeventySix.esm:F00001"},
        "curves": {"SeventySix.esm:76E9F8": {"points": [[1, 31], [100, 718]]}},
        "npcs": {"SeventySix.esm:3ADDF4": {"source": "SeventySix.esm:3ADDF4", "editor_id": "enc", "auto_calc": True,
                                           "race": "SeventySix.esm:06B4EC",
                                           "properties": {"Fallout4.esm:0002D4": {"value": 0.0, "curve": "SeventySix.esm:76E9F8"}}}},
        "explosions": {"SeventySix.esm:222222": {"remove": True}},
    },
    "PerkCards": {
        "version": 1,
        "settings": {"source_plugin": "SeventySix.esm", "output_plugin": "SeventySix.esm", "mapping_available": True,
                     "starting_deck": ["SeventySix.esm:005272"]},
        "cards": [{"form": "SeventySix.esm:005272", "name": "Number Cruncher", "sound": None,
                   "ranks": [{"cost": 3, "source_perk": "SeventySix.esm:04A0C5", "status": "effect-review-required",
                              "perk": {"form": "SeventySix.esm:04A0C5", "signature": "PERK"}}]}],
        "effect_records": {"SeventySix.esm:00000F": {"signature": "CNCY", "references": ["SeventySix.esm:000010"]}},
        "target_forms": {"SeventySix.esm:00000F": {"form": "Fallout4.esm:00000F", "signature": "MISC"}, "SeventySix.esm:000010": None},
        "weight_items": [{"source_item": "SeventySix.esm:844A74", "keywords": ["SeventySix.esm:3896DF"],
                          "targets": [{"form": "SeventySix.esm:844A74", "provenance": "final-mapper"}]}],
    },
    "LeveledLists": {
        "version": 1,
        "settings": {"source_plugin": "SeventySix.esm"},
        "lists": [{"source": "SeventySix.esm:000830", "target": "SeventySix.esm:000830", "editor_id": "B21_Test", "signature": "LVLI",
                   "scope": "seventysix", "flags": 0, "chance_none": {"value": 0.0}, "conditions": [], "entries": [
                       {"form": "SeventySix.esm:000901", "level": {"value": 1.0}, "count": {"value": 1.0}}]},
                  {"target": "SeventySix.esm:000831", "remove": True}],
        "curves": {"SeventySix.esm:000880": {"points": [[1.0, 99.0]]}},
        "delegates": {"SeventySix.esm:000890": "SeventySix.esm:000840", "SeventySix.esm:000891": {"remove": True}},
    },
    "ObjectiveAreas": {
        "version": 1,
        "settings": {"output_plugin": "SeventySix.esm"},
        "areas": [{"quest": "SeventySix.esm:000900", "objective": 10, "target": 0, "alias": 0, "flags": 0, "area_flags": 0,
                   "radius": {"kind": "global", "form": "SeventySix.esm:000901"}, "keyword": None, "conditions": []}],
    },
    "Fishing": {
        "version": 1,
        "settings": {"source_plugin": "SeventySix.esm", "source_masters": [], "output_plugin": "SeventySix.esm",
                     "fish_count": 0, "mapping_available": True},
        "records": [],
        "target_forms": {"SeventySix.esm:000800": "SeventySix.esm:000900"},
    },
}


@pytest.mark.parametrize("feature", sorted(SAMPLES))
def test_sample_validates(feature):
    validate_catalog(feature, SAMPLES[feature])


@pytest.mark.parametrize("feature, mutate", [
    ("Condition", lambda d: d["target_forms"].update({"000800:SeventySix.esm": None})),
    ("GoldBullion", lambda d: d["bullion"].append({"form": "000900:SeventySix.esm", "price": 5})),
    ("WeaponModelSwaps", lambda d: d["skins"].append({"form": "6C50EB:SeventySix.esm"})),
    ("NpcStats", lambda d: d["settings"].update({"marker": "F00001:SeventySix.esm"})),
    ("PerkCards", lambda d: d["settings"]["starting_deck"].append("005272:SeventySix.esm")),
    ("LeveledLists", lambda d: d["curves"].update({"000880:SeventySix.esm": {"points": []}})),
    ("ObjectiveAreas", lambda d: d["areas"][0].update({"quest": {"plugin": "SeventySix.esm", "object_id": 2304}})),
    ("Fishing", lambda d: d["target_forms"].update({"SeventySix.esm:000801": {"plugin": "SeventySix.esm", "object_id": "000901"}})),
])
def test_legacy_spellings_are_rejected(feature, mutate):
    document = copy.deepcopy(SAMPLES[feature])
    mutate(document)
    with pytest.raises(jsonschema.ValidationError):
        validate_catalog(feature, document)
