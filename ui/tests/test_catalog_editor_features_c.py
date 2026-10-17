"""Catalog editor, agent C's features: LeveledLists form, schema annotations, feature extensions."""
from __future__ import annotations

from test_catalog_editor_leveled import example_document, headless  # noqa: F401  (headless is a fixture)

from ui.tools.catalog_editor.features import leveled_form as form


def test_flag_helpers_round_trip():
    flags = form.set_flag(0, 0x04, True)
    flags = form.set_flag(flags, 0x200, True)
    assert flags == 0x204
    assert form.set_flag(flags, 0x04, False) == 0x200


def test_extra_keeps_rank_over_global_and_drops_it_without_owner():
    assert form.normalized_extra({"owner": "A.esp:1", "rank": 2, "global": "A.esp:2", "health": 0.5}) == {
        "health": 0.5, "owner": "A.esp:1", "rank": 2}
    assert form.normalized_extra({"owner": "A.esp:1", "global": "A.esp:2"}) == {
        "health": 1.0, "owner": "A.esp:1", "global": "A.esp:2"}
    assert form.normalized_extra({"rank": 3, "global": "A.esp:2", "health": 0.25}) == {"health": 0.25}
    assert form.normalized_extra({}) is None
    assert form.owner_kind({"owner": "A.esp:1", "rank": 0}) == "faction"
    assert form.owner_kind({"owner": "A.esp:1"}) == "other"
    assert form.owner_kind(None) == "none"


def test_plain_summary_reads_as_a_paragraph():
    row = example_document()["lists"][0]
    text = form.plain_summary(row)
    assert text.startswith("Everywhere, this loot list") and "2 items" in text and "equally likely" in text
    row = {**row, "flags": 0x04 | 0x02, "chance_none": {"value": 25}}
    text = form.plain_summary(row)
    assert "every one of its 2 items" in text and "25% chance" in text and "separately" in text


def _leveled_view(tool, entry):
    doc = tool._current_document()
    key = doc.set_entry("lists", entry)
    tool.select_row(key)
    (row,) = doc.rows("lists")
    return doc, row, tool._workbench.view(doc, "lists", row)


def test_every_leveled_inspector_tab_and_the_drops_table_draw_without_changing_the_list(headless):  # noqa: F811
    from ui.tools.catalog_editor.features import extension_for

    tool, frames = headless
    entry = example_document()["lists"][0]
    entry["entries"][0]["extra"] = {"owner": "Fallout4.esm:000123", "rank": 2, "health": 0.5}
    entry["entries"][1]["extra"] = {"owner": "Mod.esp:000810", "global": "Mod.esp:000811", "health": 1.0}
    doc, row, view = _leveled_view(tool, entry)
    extension = extension_for("LeveledLists")
    results = []
    for item in (None, 0, 1):
        view.item = item
        tabs = extension.inspector_tabs(view)
        assert [tab.title.split(" (")[0] for tab in tabs] == ["List", "Item & numbers", "Checks", "Owner & condition", "In game"]
        frames(2, lambda _frame: results.extend(tab.draw(view) for tab in tabs))
    frames(2, lambda _frame: results.append(extension.result_view(doc, "lists").draw(view)))
    assert all(not changed for changed, _ in results)
    frames(3)  # the whole workbench, with the list selected


def test_item_sentence_reads_like_the_rule():
    item = example_document()["lists"][0]["entries"][1]
    names = {"MyMod.esp:000820": "Rifle", "MyMod.esp:000810": "Scrounger"}
    text = form.item_sentence(item, lambda key: names.get(key, key))
    assert text.startswith("Give Rifle x1 from level 1, only when ")
    assert form.item_sentence({**item, "conditions": [], "chance_none": {"value": 25}}, names.get) == (
        "Give Rifle x1 from level 1, with a 25% chance of nothing.")


# --- schemas ---------------------------------------------------------------------------------

import json

import pytest

from creation_lib.catalog.plugin_catalog import SCHEMAS_DIR

FEATURES_C = ("PerkCards", "PerkCardExtras", "PlayerGhoul", "CombatPerks", "Diseases", "FoodSpoilage", "Condition",
              "Fishing", "NpcStats", "GoldBullion", "EquipmentBonuses", "RealtimeVATS", "ObjectiveAreas", "LeveledLists")


def _schema(name):
    return json.loads((SCHEMAS_DIR / f"{name}.schema.json").read_text(encoding="utf-8"))


def _nodes(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from _nodes(value)
    elif isinstance(node, list):
        for value in node:
            yield from _nodes(value)


@pytest.mark.parametrize("feature", FEATURES_C)
def test_feature_explains_itself(feature):
    schema = _schema(feature)
    assert schema.get("description") and schema.get("x-help"), "feature description and x-help"
    assert len(schema["description"]) < 220
    for name, section in schema["properties"].items():
        if name == "version":
            continue
        assert section.get("title") and section.get("description"), f"{feature}.{name} needs a title and description"


@pytest.mark.parametrize("feature", FEATURES_C)
def test_labels_match_their_fields(feature):
    for node in _nodes(_schema(feature)):
        if "x-flags" in node:
            assert all(key.isdigit() and int(key) & (int(key) - 1) == 0 for key in node["x-flags"]), node["x-flags"]
        if "x-enum-labels" in node and "enum" in node:
            assert set(node["x-enum-labels"]) <= set(node["enum"]), node["x-enum-labels"]


def test_leveled_flag_labels_cover_every_bit_the_form_offers():
    flags = _schema("LeveledLists")["properties"]["lists"]["items"]["properties"]["flags"]["x-flags"]
    bits = {bit for bit, _, _ in form.FLAG_HELP} | {bit for bit, _, _ in form.IGNORED_FLAG_HELP}
    assert {int(key) for key in flags} == bits


def test_annotated_schemas_still_validate_emitter_samples():
    samples = pytest.importorskip("bacup_lib.tests.test_catalog_schemas_rust_emitted").SAMPLES
    validate = pytest.importorskip("bacup_lib.tests.catalog_schema_validation").validate_catalog
    covered = [name for name in samples if name in FEATURES_C]
    assert {"Condition", "GoldBullion", "NpcStats", "PerkCards"} <= set(covered)
    for name in covered:
        validate(name, samples[name])


# --- feature extensions ------------------------------------------------------------------------

from ui.tools.catalog_editor.features import diseases, extension_for, food_spoilage, gold_bullion, npc_stats, perk_cards


def messages(tool, name):
    doc = tool._session.feature(name)
    return [problem.message for problem in extension_for(name).problems(doc, None)], doc


def test_diseases_warn_about_dead_ends(headless):  # noqa: F811
    tool, _ = headless
    doc = tool._session.feature("Diseases")
    doc.set_entry("diseases", {"spell": "Mod.esp:000A01"})
    found, _ = messages(tool, "Diseases")
    assert any("Nothing gives this disease" in m for m in found) and any("no cure effects" in m for m in found)
    doc.set_entry("chance_rows", {"type": "water", "risk": "Mod.esp:000A02", "disease": "mod.esp:0A01", "chance": 5})
    doc.set_entry("cures", {"effect": "Mod.esp:000A03"})
    found, _ = messages(tool, "Diseases")
    assert found == []
    (row,) = doc.rows("diseases")
    assert diseases.sources_of(doc, row.entry["spell"]) == ["water: 5% base chance (Mod.esp:000A02)"]


def test_food_spoilage_shows_time_to_spoil(headless):  # noqa: F811
    tool, _ = headless
    doc = tool._session.feature("FoodSpoilage")
    points = [{"x": 1, "y": 60}, {"x": 50, "y": 600}]
    assert food_spoilage.minutes_to_spoil(points, 1, 1.0) == 60
    assert food_spoilage.minutes_to_spoil(points, 100, 2.0) == 300
    assert food_spoilage.minutes_to_spoil(points, 1, 1.0, 0.5) == 120
    assert food_spoilage.minutes_to_spoil(points, 1, 1.0, 0) is None
    assert food_spoilage.duration(90) == "1.5 h" and food_spoilage.duration(None) == "never"
    doc.set_entry("foods", {"form": "Mod.esp:000B01", "curve": [{"x": 5, "y": 1}, {"x": 5, "y": 2}]})
    found, _ = messages(tool, "FoodSpoilage")
    assert found == ["The curve's levels (x) must increase from point to point."]


def test_gold_bullion_warns_about_prices_nobody_can_pay(headless):  # noqa: F811
    tool, _ = headless
    doc = tool._session.feature("GoldBullion")
    doc.set_entry("bullion", {"form": "Mod.esp:000C01", "price": 20000})
    doc.set_entry("stamps", {"form": "Mod.esp:000C02", "price": 20000})
    doc.set_entry("tadpole", {"form": "Mod.esp:000C03", "price": 500000})
    found, _ = messages(tool, "GoldBullion")
    assert len(found) == 2 and "hold" in found[0] and "unusually high" in found[1]
    assert gold_bullion.price_warning("stamps", 20000) is None
    assert gold_bullion.discounted(1250) == 938


def test_perk_cards_describe_and_check_ranks(headless):  # noqa: F811
    tool, _ = headless
    entry = {"form": "Mod.esp:000D01", "special": 6, "min_level": 5, "race_restriction": 2,
             "ranks": [{"cost": 1, "perk": {"form": "Mod.esp:000D02"}}, {"cost": 0}]}
    assert perk_cards.card_summary(entry) == "Luck card for ghouls only, from level 5. Rank costs: 1/0."
    assert perk_cards.card_problems(entry) == ["Rank 2 grants no perk, so it does nothing.", "Rank 2 costs no SPECIAL points."]
    tool._session.feature("PerkCards").set_entry("cards", entry)
    found, _ = messages(tool, "PerkCards")
    assert len(found) == 2


def test_npc_stats_curve_preview_follows_tales_rules(headless):  # noqa: F811
    points = [[1, 10], [11, 20], [5, 99]]  # the last x is held to the previous x
    assert npc_stats.curve_at(points, 0) == 10
    assert npc_stats.curve_at(points, 6) == 15
    assert npc_stats.curve_at(points, 50) == 99
    tool, _ = headless
    doc = tool._session.feature("NpcStats")
    doc.set_entry("npcs", {"source": "Mod.esp:000E01", "health_curve": "Mod.esp:000E02"}, map_key="Mod.esp:000E01")
    found, _ = messages(tool, "NpcStats")
    assert found == ["Curve Mod.esp:000E02 is not in any Curves row, so the flat value is used."]
    doc.set_entry("curves", {"points": points}, map_key="Mod.esp:000E02")
    assert messages(tool, "NpcStats")[0] == []


SMOKE = {
    "Diseases": ("diseases", {"spell": "Mod.esp:000A01"}, None),
    "FoodSpoilage": ("foods", {"form": "Mod.esp:000B01", "curve": [{"x": 1, "y": 60}, {"x": 50, "y": 600}]}, None),
    "GoldBullion": ("bullion", {"form": "Mod.esp:000C01", "price": 20000}, None),
    "PerkCards": ("cards", {"form": "Mod.esp:000D01", "special": 1, "ranks": [{"cost": 1, "name": "A", "description": "B"}]}, None),
    "NpcStats": ("curves", {"points": [[1, 2], [50, 9]]}, "Mod.esp:000E02"),
    "PlayerGhoul": ("chems", {"form": "Mod.esp:000F01", "recovery": 5}, None),
    "ObjectiveAreas": ("areas", {"quest": "Mod.esp:001001", "objective": 1, "target": 0, "target_count": 1, "alias": 0,
                                 "flags": 0, "area_flags": 2, "keyword": None, "radius": {"kind": "literal", "value": 100},
                                 "conditions": []}, None),
    "CombatPerks": ("cards", {"form": "Mod.esp:001101", "rank": 1}, "bullet_storm"),
    "EquipmentBonuses": ("curves", {"points": [{"x": 0, "y": 0}, {"x": 1, "y": 1}]}, "lastBastion"),
}


@pytest.mark.parametrize("feature", sorted(SMOKE))
def test_feature_pages_draw(headless, feature):  # noqa: F811
    tool, frames = headless
    section, entry, map_key = SMOKE[feature]
    doc = tool._session.feature(feature)
    key = doc.set_entry(section, entry, map_key=map_key)
    tool._feature, tool._section = feature, section
    tool.select_row(key)
    frames(3)
    tool._section = "settings"
    frames(2)
