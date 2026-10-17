import json
import struct

from bacup_lib.camp_scrap import (
    CAN_BE_DELETED_NEW, CAN_BE_DELETED_OLD, FEATURE, ScrapTargets, SourceObject, TargetBase, build_document,
    can_be_deleted, tokens,
)
from bacup_lib.plugin_catalog import write_document
from bacup_lib.tests.catalog_schema_validation import validate_catalog


def prps(*rows):
    return b"".join(struct.pack("<IfI", actor_value, value, 0) for actor_value, value in rows)


def base(object_id, signature, editor_id, model, plugin="fallout4.esm"):
    return TargetBase((plugin, object_id), signature, editor_id, model)


CARS = ("fallout4.esm", 0x106D9F)
RUSTY = ("fallout4.esm", 0x0E2C5E)
JUNK = ("fallout4.esm", 0x0E2C60)
DLC_FENCE = ("dlccoast.esm", 0x0423A9)


def targets():
    bases = [
        base(0x1A304B, "MSTT", "CarSportsCar01", "Vehicles\\Automotive\\SportsCar01_Postwar.nif"),
        base(0x0FE4D1, "STAT", "Sedan01_Static", "Vehicles\\Automotive\\Sedan01.nif"),
        base(0x0FE4D2, "STAT", "TrashCanMetal", "SetDressing\\TrashCan01.nif"),
        base(0x0423B0, "STAT", "DLC03_DeerFence01", "DLC03\\Fences\\DeerFence01.nif", "dlccoast.esm"),
        base(0x200000, "STAT", "SomethingElse", "Other\\Thing.nif"),
    ]
    recipe_of = {
        ("fallout4.esm", 0x1A304B): CARS,
        ("fallout4.esm", 0x0FE4D1): RUSTY,
        ("fallout4.esm", 0x0FE4D2): JUNK,
        ("dlccoast.esm", 0x0423B0): DLC_FENCE,
    }
    return ScrapTargets(
        recipe_of, {item.key: item for item in bases},
        {"workshop_co_scrapcars": CARS, "workshop_co_scrapmetalverysmall": JUNK},
        {"fallout4.esm": "Fallout4.esm", "dlccoast.esm": "DLCCoast.esm"})


def objects(document):
    return document["objects"]


def test_new_property_wins_over_old():
    assert can_be_deleted(prps((CAN_BE_DELETED_OLD, 1.0)))
    assert can_be_deleted(prps((CAN_BE_DELETED_NEW, 1.0)))
    assert not can_be_deleted(prps((CAN_BE_DELETED_OLD, 1.0), (CAN_BE_DELETED_NEW, 0.0)))
    assert not can_be_deleted(prps((0x0002C2, 5.0)))
    assert not can_be_deleted(b"")


def test_same_id_needs_matching_type_and_name_or_model():
    car = SourceObject(0x1A304B, "CarSportsCar01", "Vehicles\\Automotive\\SportsCar01_Postwar.nif")
    reused_id = SourceObject(0x0FE4D2, "PacificationEmitter", "SetDressing\\Emitter.nif")
    document, _ = build_document("SeventySix.esm", [], [(car, "MSTT"), (reused_id, "STAT")], targets())
    assert objects(document)["SeventySix.esm:1A304B"] == {"recipe": "Fallout4.esm:106D9F", "match": "id"}
    assert objects(document)["SeventySix.esm:0FE4D2"]["match"] == "fallback"


def test_editor_id_ignores_fo76_suffix_and_model_matches_across_plugins():
    renamed = SourceObject(0x500001, "Sedan01_Staticfo76", "Vehicles\\Other.nif")
    same_model = SourceObject(0x500002, "Anything", "dlc03/fences/deerfence01.nif")
    document, tiers = build_document("SeventySix.esm", [], [(renamed, "STAT"), (same_model, "STAT")], targets())
    assert objects(document)["SeventySix.esm:500001"] == {"recipe": "Fallout4.esm:0E2C5E", "match": "editor_id"}
    assert objects(document)["SeventySix.esm:500002"] == {"recipe": "DLCCoast.esm:0423A9", "match": "model"}
    assert tiers == {"editor_id": 1, "model": 1}


def test_class_rule_then_fallback():
    limousine = SourceObject(0x500003, "Government_Limousine02Weathered", "Vehicles\\Limo.nif")
    juggling = SourceObject(0x500004, "JugglingBall", "Props\\JugglingBall.nif")
    document, _ = build_document("SeventySix.esm", [], [(limousine, "STAT"), (juggling, "STAT")], targets())
    assert objects(document)["SeventySix.esm:500003"] == {"recipe": "Fallout4.esm:106D9F", "match": "class"}
    assert objects(document)["SeventySix.esm:500004"] == {"recipe": "Fallout4.esm:0E2C60", "match": "fallback"}


def test_tokens_drop_kit_and_condition_words():
    assert tokens("Burn_Vehicle_Cheap_Sedan01_Dirty", "dlc04/vehicles/automotive/dlc04_sedandirt01.nif") == {
        "vehicle", "sedan", "sedandirt"}


def test_document_validates_against_schema(tmp_path):
    car = SourceObject(0x1A304B, "CarSportsCar01", "Vehicles\\Automotive\\SportsCar01_Postwar.nif")
    document, _ = build_document("SeventySix.esm", [], [(car, "MSTT")], targets())
    validate_catalog(FEATURE, document)
    path = write_document(FEATURE, "SeventySix.esm", document, tmp_path)
    assert json.loads(path.read_text(encoding="utf-8"))["objects"]["SeventySix.esm:1A304B"]["recipe"] == "Fallout4.esm:106D9F"
