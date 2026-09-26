import importlib.util
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[4] / "scripts/export_gold_bullion.py"
spec = importlib.util.spec_from_file_location("export_gold_bullion", SCRIPT)
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)


def inputs():
    return (
        [{"form_id": "58935B", "eid": "W05_Recipe_Armor_SecretService_Torso_GoldVendor", "signature": "BOOK", "fields.GoldBullionValue": {"raw_hex": "4D505A00"}}],
        [{"form_id": "5A504D", "fields.Value": 1250.0}],
        [{"form_id": "58935B", "eid": "W05_Recipe_Armor_SecretService_Torso_GoldVendor", "signature": "BOOK"}],
    )


def test_source_bullion_price_and_output_ownership():
    result = exporter.build_catalog(*inputs(), "Converted.esm")
    assert result["prices"] == [{"plugin": "Converted.esm", "object_id": 0x58935B, "price": 1250}]
    assert result["skipped_unmapped"] == 0


def test_missing_output_is_reported():
    items, globals_, _ = inputs()
    result = exporter.build_catalog(items, globals_, [], "Converted.esm")
    assert result["prices"] == []
    assert result["skipped_unmapped"] == 1


@pytest.mark.parametrize("price", [0, -1, float("nan"), float("inf"), 1.5, 2**31])
def test_invalid_prices_fail_before_writing(price):
    items, globals_, targets = inputs()
    globals_[0]["fields.Value"] = price
    with pytest.raises(ValueError, match="Invalid bullion price"):
        exporter.build_catalog(items, globals_, targets, "Converted.esm")


def test_reused_id_cannot_price_an_unrelated_item():
    items, globals_, targets = inputs()
    targets[0]["eid"] = "DifferentItem"
    with pytest.raises(ValueError, match="identity mismatch"):
        exporter.build_catalog(items, globals_, targets, "Converted.esm")


def test_truncated_price_reference_is_rejected():
    items, globals_, targets = inputs()
    items[0]["fields.GoldBullionValue"]["raw_hex"] = "4D505A"
    with pytest.raises(ValueError, match="Invalid bullion reference"):
        exporter.build_catalog(items, globals_, targets, "Converted.esm")
