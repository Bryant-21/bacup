import pytest

from bacup_lib.pet_rewards import node_delivery


def ref(value):
    return {"reference": {"object_id": f"{value:06X}"}}


class Reader:
    def __init__(self, records):
        self.records = records

    def fields(self, object_id):
        return self.records[object_id]

    def lookup(self, signature, eid):
        for object_id, (sig, name, fields) in self.records.items():
            if (sig, name) == (signature, eid):
                return object_id, fields
        raise ValueError("missing source record")


class Resolver:
    def resolve(self, signature, eid):
        return "Test.esm:000123" if eid == "TestItem" else None


def reward(fields, extra):
    return node_delivery({"image_id": 1, "reward_id": 2}, Reader({
        1: ("KYWD", "TestImage", {}), 2: ("GMRW", "TestReward", fields), **extra}), Resolver())


def test_perk_coins_use_the_native_balance_instead_of_inventory():
    result = reward({"QuestRewardCurrencyObject": [ref(3)], "CurrencyAmountGlobal": [ref(4)]}, {
        3: ("CNCY", "PerkScrapCurrency", {}), 4: ("GLOB", "TestQuantity", {"Value": [17]})})
    assert result == {"kind": "perk_coins", "count": 17}


def test_inventory_rewards_require_a_converted_form():
    fields = {"RewardedItem": [{"Item": ref(3), "Count": 7}]}
    assert reward(fields, {3: ("MISC", "TestItem", {})}) == {"kind": "item", "form": "Test.esm:000123", "count": 7}
    assert reward(fields, {3: ("MISC", "Missing", {})})["kind"] == "unavailable"


@pytest.mark.parametrize("field", ["Entitlement", "UnknownGlobal"])
def test_unsupported_online_rewards_are_not_consumable(field):
    assert reward({field: [ref(3)]}, {})["kind"] == "unavailable"


@pytest.mark.parametrize("count", [-1, 0, 0.5, 2147483648])
def test_invalid_item_counts_do_not_enable_claiming(count):
    assert reward({"RewardedItem": [{"Item": ref(3), "Count": count}]}, {3: ("MISC", "TestItem", {})})["kind"] == "unavailable"


def test_passive_rank_comes_from_source_image_and_magnitude_not_level_or_translated_name():
    records = {1: ("KYWD", "WorldPets_Img_All_PetCarrier02", {}),
               2: ("GLOB", "WorldPets_Buff_All_PetCarrier02", {"Value": [27]})}
    result = node_delivery({"image_id": 1, "level": 99, "name": "localized"}, Reader(records), Resolver())
    assert result == {"kind": "passive", "family": "carry", "rank": 2, "magnitude": 27}


def test_fetch_joins_quantity_globals_not_broken_cross_species_condition_uids():
    records = {1: ("KYWD", "WorldPets_Img_Dog_StimpakFetcher02", {}),
               2: ("GLOB", "WorldPets_ConsumableBuff_Dog02", {"Value": [7]}),
               3: ("LVLI", "WorldPets_LL_ConsumableBuff_Dog", {
                   "LVLO": [{"value": ref(4)}], "QuantityGlobal": [ref(2)]}),
               4: ("ALCH", "TestItem", {}),
               5: ("GLOB", "WorldPets_ConsumableGiftInterval", {"Value": [31]})}
    result = node_delivery({"image_id": 1}, Reader(records), Resolver())
    assert result == {"kind": "passive", "family": "fetch", "rank": 2, "interval": 31,
                      "reward": {"kind": "item", "form": "Test.esm:000123", "count": 7}}


def test_compound_rewards_fail_closed_without_partial_delivery():
    assert reward({"RewardedItem": [{"Item": ref(3), "Count": 1}] * 2}, {3: ("MISC", "TestItem", {})})["kind"] == "unavailable"
