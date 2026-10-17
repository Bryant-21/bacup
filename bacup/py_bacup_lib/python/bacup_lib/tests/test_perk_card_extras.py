import json

from bacup_lib.perk_card_extras import to_document
from bacup_lib.plugin_catalog import write_document
from bacup_lib.tests.catalog_schema_validation import validate_catalog


def catalog():
    return {"version": 1,
            "mystery_meat": {"meat": [{"form": "SeventySix.esm:110BA4", "maximum": 3, "source": 1117092},
                                      {"form": None, "maximum": 3, "source": 5}],
                             "chances": [0.1, 0.2, 0.3], "stimpak_keyword": "SeventySix.esm:0A1B2C",
                             "cooldown": 60, "radiation_maximum": 1000, "radiation_multiplier": [0.5, 1.5]},
            "survival": {"version": 1, "cards": {}, "records": {"157761": {"signature": "SPEL", "form": "SeventySix.esm:026841"}}},
            "survival_spells": [0x026841, 0x054DF3]}


def test_document_has_no_food_duplicate_and_validates(tmp_path):
    document = to_document(catalog())
    assert "foods" not in document and "foods" not in document["settings"]
    assert [row["form"] for row in document["meat"]] == ["SeventySix.esm:110BA4"]
    assert document["meat"][0]["source"] == "SeventySix.esm:110BA4"
    assert [row["form"] for row in document["survival_spells"]] == ["SeventySix.esm:026841", "SeventySix.esm:054DF3"]
    path = write_document("PerkCardExtras", "SeventySix.esm", document, tmp_path)
    assert path.as_posix().endswith("PerkCardExtras/SeventySix.esm.json")
    validate_catalog("PerkCardExtras", json.loads(path.read_text(encoding="utf-8")))
