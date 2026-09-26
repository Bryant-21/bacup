import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "scripts"))
import export_stamps
from export_stamps import build_catalog, read_stock, stock_children


def item():
    return {"form_id": "64B933", "eid": "Recipe_XPD_mod_PowerArmor_Union_Leg_Misc_Carry",
            "signature": "BOOK", "fields.Data": {"Value": 50, "Weight": 0.25}}


def _bullion_tagged_item():
    source = item()
    source["fields.GoldBullionValue"] = {"raw_hex": "4D505A00"}
    return source


@pytest.mark.parametrize(
    "source,price",
    [
        pytest.param(_bullion_tagged_item(), 50, id="stamp_value_not_bullion"),
        pytest.param(
            {"form_id": "72D4FC", "eid": "SCORE_BobbleheadBox", "signature": "ALCH",
             "fields.Data": None, "fields.EffectData": {"Value": 20}},
            20,
            id="bobblehead_box_effect_data",
        ),
        pytest.param(
            {"form_id": "3FC7DC", "eid": "TadpoleHat", "signature": "ARMO",
             "fields.Data": {"Value": 500}, "fields.ArmorData": {"Value": 3}},
            3,
            id="scout_uniform_armor_value",
        ),
    ],
)
def test_price_source_field(source, price):
    converted = item() if source["form_id"] == "64B933" else source
    prices = build_catalog([source], [converted], "Converted.esm")["prices"]
    assert prices == [{"plugin": "Converted.esm", "object_id": int(source["form_id"], 16), "price": price}]


def test_only_verified_converted_identities_are_priced():
    assert build_catalog([item()], [], "Converted.esm")["skipped_unmapped"] == 1
    changed = item()
    changed["eid"] = "AnotherItem"
    with pytest.raises(ValueError, match="identity mismatch"):
        build_catalog([item()], [changed], "Converted.esm")


@pytest.mark.parametrize("value", [0, -1, 2**31, 50.5, True, None])
def test_invalid_stamp_price_rejected(value):
    source = item()
    source["fields.Data"]["Value"] = value
    with pytest.raises(ValueError, match="Invalid currency"):
        build_catalog([source], [item()], "Converted.esm")


def test_stock_graph_accepts_single_and_multiple_entries():
    entry = {"variant": "reference", "value": 0x64B933}
    assert stock_children({"fields.LVLO": entry}) == [0x64B933]
    assert stock_children({"fields.LVLO": [entry, entry]}) == [0x64B933] * 2
    with pytest.raises(ValueError, match="Unsupported"):
        stock_children({"fields.LVLO": {"variant": "unknown", "value": 1}})


def test_multi_currency_stock_graph_keeps_roots_separate(monkeypatch):
    def stock(key, children):
        return {"form_id": f"{key:X}", "signature": "LVLI", "fields.LVLO": [
            {"variant": "reference", "value": child} for child in children]}
    first, second = dict(item(), form_id="3"), dict(item(), form_id="4")
    records = {1: stock(1, [3, 5]), 2: stock(2, [4]), 3: first, 4: second, 5: stock(5, [1, 3])}
    def query(_source, _game, _fields, *filters):
        return [records[int(value, 16)] for value in filters[1::2]]
    monkeypatch.setattr(export_stamps, "query", query)
    assert read_stock(Path("source.esm"), [1, 2]) == {1: [first], 2: [second]}


def test_stock_graph_rejects_missing_or_wrong_root(monkeypatch):
    monkeypatch.setattr(export_stamps, "query", lambda *args: [])
    with pytest.raises(ValueError, match="Incomplete"):
        read_stock(Path("source.esm"), [1])
    monkeypatch.setattr(export_stamps, "query", lambda *args: [dict(item(), form_id="1")])
    with pytest.raises(ValueError, match="Expected stock list"):
        read_stock(Path("source.esm"), [1])
