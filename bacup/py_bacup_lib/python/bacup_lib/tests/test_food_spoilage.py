import json

import pytest

from bacup_lib.food_spoilage import build_catalog, emit_catalog
from bacup_lib.tests.catalog_schema_validation import hex_key, validate_catalog
from bacup_lib.player_ghoul import number


def ref(object_id):
    return {"reference": {"plugin": "SeventySix.esm", "object_id": f"{object_id:06X}"}}


class FakeSource:
    def __init__(self):
        self.records = {}

    def add(self, raw, signature, eid, **fields):
        self.records[raw] = (signature, eid, {key: [value] for key, value in fields.items()})

    def rows(self, signature, query="*"):
        return [(raw, signature, eid, None) for raw, (sig, eid, _) in self.records.items()
                if sig == signature and (query == "*" or eid == query)]

    def fields(self, raw):
        return self.records[raw]


class FakeResolver:
    plugin = "SeventySix.esm"

    def __init__(self, missing=()):
        self.missing = set(missing)

    def resolve(self, signature, eid):
        return None if eid in self.missing else f"SeventySix.esm:{signature}:{eid}"


def food(source, raw, eid, health, spoiled, *keywords):
    source.add(raw, "ALCH", eid, EffectData={"Health": ref(health), "Spoiled": ref(spoiled)},
               Keywords=[ref(keyword) for keyword in keywords])


@pytest.fixture
def source_root(tmp_path):
    folder = tmp_path / "misc/curvetables/json/crafting"
    folder.mkdir(parents=True)
    for name, ys in {"raw": (75, 249), "cooked": (240, 797), "veg": (75, 380), "cookedveg": (288, 1458)}.items():
        (folder / f"{name}.json").write_text(json.dumps({"curve": [{"x": 1, "y": ys[0]}, {"x": 10, "y": ys[1]}]}))
    return tmp_path


@pytest.fixture
def source():
    source = FakeSource()
    for raw, eid in {0x10: "IngredientTypeMeat", 0x11: "MealTypeRaw", 0x12: "MealTypeCooked", 0x13: "MealTypeTasty",
                     0x14: "IngredientTypeVegetable", 0x15: "Workbench_Crafting_Refrigerator",
                     0x16: "Workbench_Crafting_Freezer", 0x17: "ObjectTypeCanSpoil"}.items():
        source.add(raw, "KYWD", eid)
    for raw, eid, path in [(0x20, "CT_Raw", "Crafting\\raw.json"), (0x21, "CT_Cooked", "Crafting\\cooked.json"),
                           (0x22, "CT_Veg", "Crafting\\veg.json"), (0x23, "CT_CookedVeg", "Crafting\\cookedveg.json")]:
        source.add(raw, "CURV", eid, JASF=path)
    source.add(0x30, "ALCH", "SpoiledFood_Meat")
    source.add(0x31, "ALCH", "SpoiledFood_Vegetable")
    food(source, 0x40, "RadstagMeat", 0x20, 0x30, 0x10, 0x11, 0x17)
    food(source, 0x41, "BrahminMeat", 0x20, 0x30, 0x10, 0x11, 0x17)
    food(source, 0x42, "GrilledRadstag", 0x21, 0x30, 0x10, 0x12, 0x17)
    food(source, 0x43, "RadstagStew", 0x23, 0x30, 0x10, 0x12, 0x13, 0x17)
    food(source, 0x44, "Carrot", 0x22, 0x31, 0x14, 0x11, 0x17)
    food(source, 0x45, "CarrotSoup", 0x23, 0x31, 0x14, 0x12, 0x17)
    source.add(0x46, "ALCH", "NukaCola", EffectData={})
    for raw, eid, value in [(0x50, "WorkshopStashActivatorGroup_Refrigerator", 150.0),
                            (0x51, "WorkshopStashActivatorGroup_Cryo_Freezer", 50.0),
                            (0x52, "WorkshopStashActivatorGroup_Fermenter", 50.0),
                            (0x53, "Refrigerator_SpoilRate", 0.5), (0x54, "Freezer_SpoilRate", 0.0),
                            (0x55, "Fermenter_SpoilRate", 2.0)]:
        source.add(raw, "GLOB", eid, Value=value)
    source.add(0x60, "DFOB", "WorkshopStashActivatorGroupRefrigeratorSpoilRate_DO", Object=ref(0x53))
    source.add(0x61, "DFOB", "WorkshopStashActivatorGroupFreezerSpoilRate_DO", Object=ref(0x54))
    source.add(0x62, "DFOB", "WorkshopStashActivatorGroupFermenterSpoilRate_DO", Object=ref(0x55))
    source.add(0x70, "ACTI", "ATX_Refrigerator", Keywords=[ref(0x15)],
               VendableItemDatas=[{"VendableItemDatasUnknown": 0x50, "VendableItemDatasMaxAmount": 150}])
    source.add(0x71, "ACTI", "Score_S17_Refrigerator_CampingCooler",
               VendableItemDatas=[{"VendableItemDatasUnknown": 0x01000050}])
    source.add(0x72, "ACTI", "SCORE_S14_Military_Cryo_Freezer", Keywords=[ref(0x16)],
               VendableItemDatas=[{"VendableItemDatasUnknown": 0x51}])
    source.add(0x73, "ACTI", "Brewing_Fermenter", VendableItemDatas=[{"VendableItemDatasUnknown": 0x52}])
    source.add(0x74, "ACTI", "ATX_VendingMachine", VendableItemDatas=[{"VendableItemDatasUnknown": 0x99}])
    source.add(0x80, "GMST", "fFoodHealthDegradationRatePerMinute", Value={"variant": "float", "value": 1.0})
    return source


def test_foods_carry_their_fo76_curve_and_spoiled_item(source, source_root):
    catalog = build_catalog(source, FakeResolver(), source_root, number)
    foods = {row["editor_id"]: row for row in catalog["foods"]}
    assert set(foods) == {"RadstagMeat", "BrahminMeat", "GrilledRadstag", "RadstagStew", "Carrot", "CarrotSoup"}
    assert foods["RadstagMeat"]["spoiled"] == "SeventySix.esm:ALCH:SpoiledFood_Meat"
    assert foods["RadstagMeat"]["curve"] == [{"x": 1, "y": 75}, {"x": 10, "y": 249}]
    assert foods["RadstagMeat"]["curve_id"] == "CT_Raw"


def test_defaults_are_the_most_common_fo76_profile_per_category(source, source_root):
    defaults = build_catalog(source, FakeResolver(), source_root, number)["defaults"]
    assert defaults["raw_meat"]["curve_id"] == "CT_Raw" and defaults["raw_meat"]["fo76_items"] == 2
    assert defaults["cooked_meat"]["curve_id"] == "CT_Cooked"  # the tasty stew is excluded
    assert defaults["raw_vegetable"]["spoiled"] == "SeventySix.esm:ALCH:SpoiledFood_Vegetable"
    assert defaults["cooked_vegetable"]["curve_id"] == "CT_CookedVeg"


def test_storage_groups_come_from_the_stash_activator_group_and_its_rate(source, source_root):
    storage = build_catalog(source, FakeResolver(), source_root, number)["storage"]
    assert storage["refrigerator"]["rate"] == 0.5
    assert storage["refrigerator"]["keyword"] == "SeventySix.esm:KYWD:Workbench_Crafting_Refrigerator"
    # A fridge without the keyword still joins through its group, which may carry a load-order byte.
    assert storage["refrigerator"]["bases"] == ["SeventySix.esm:ACTI:ATX_Refrigerator",
                                                "SeventySix.esm:ACTI:Score_S17_Refrigerator_CampingCooler"]
    assert storage["freezer"]["rate"] == 0.0
    assert storage["fermenter"] == {"rate": 2.0, "group": "WorkshopStashActivatorGroup_Fermenter",
                                    "bases": ["SeventySix.esm:ACTI:Brewing_Fermenter"], "keyword": None}


def test_unconverted_forms_stay_null_instead_of_guessed(source, source_root):
    catalog = build_catalog(source, FakeResolver({"Carrot", "ATX_Refrigerator"}), source_root, number)
    assert next(row for row in catalog["foods"] if row["editor_id"] == "Carrot")["form"] is None
    assert "SeventySix.esm:ACTI:ATX_Refrigerator" not in catalog["storage"]["refrigerator"]["bases"]


def test_curve_outside_the_source_root_is_rejected(source, source_root):
    source.add(0x20, "CURV", "CT_Raw", JASF="..\\..\\..\\escape.json")
    with pytest.raises(ValueError, match="escapes"):
        build_catalog(source, FakeResolver(), source_root, number)


def test_emit_writes_the_tales_catalog(source, source_root, tmp_path):
    emit_catalog(source, FakeResolver(), tmp_path / "mod", source_root, number)
    path = tmp_path / "mod" / "F4SE/Plugins/B21_TalesFromAppalachia/FoodSpoilage/SeventySix.esm.json"
    written = json.loads(path.read_text(encoding="utf-8"))
    assert written["version"] == 1 and len(written["foods"]) == 6
    assert written["settings"]["points_per_minute"] == 1.0 and "storage" in written["settings"]


def test_decay_rate_comes_from_the_fo76_gmst(source, source_root):
    assert build_catalog(source, FakeResolver(), source_root, number)["points_per_minute"] == 1.0
    source.add(0x80, "GMST", "fFoodHealthDegradationRatePerMinute", Value={"variant": "float", "value": 2.5})
    assert build_catalog(source, FakeResolver(), source_root, number)["points_per_minute"] == 2.5
    del source.records[0x80]
    with pytest.raises(ValueError, match="fFoodHealthDegradationRatePerMinute"):
        build_catalog(source, FakeResolver(), source_root, number)


class HexResolver(FakeResolver):
    def resolve(self, signature, eid):
        return None if eid in self.missing else hex_key("SeventySix.esm", eid)


def test_emitted_document_matches_its_schema(source, source_root, tmp_path):
    emit_catalog(source, HexResolver(), tmp_path / "mod", source_root, number)
    path = tmp_path / "mod" / "F4SE/Plugins/B21_TalesFromAppalachia/FoodSpoilage/SeventySix.esm.json"
    validate_catalog("FoodSpoilage", json.loads(path.read_text(encoding="utf-8")))
