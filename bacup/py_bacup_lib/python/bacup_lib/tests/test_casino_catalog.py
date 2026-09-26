import json

import pytest

from bacup_lib.casino_catalog import build_casino_catalog


def write_curves(root, three=None, five=None):
    folder = root / "misc/curvetables/json/misc/expeditions"
    folder.mkdir(parents=True, exist_ok=True)
    for count, values in ((3, three or [0.2, 1.0]), (5, five or [0.4, 1.0])):
        (folder / f"xpd_ac_slotmachinechances_{count}tumbler.json").write_text(
            json.dumps({"curve": [{"x": index, "y": value} for index, value in enumerate(values)]}))


def test_catalog_retains_source_costs_curves_and_missing_server_fields(tmp_path):
    write_curves(tmp_path)
    catalog = build_casino_catalog(tmp_path)
    assert len(catalog["machines"]) == 18
    assert {entry["entry_caps"] for entry in catalog["machines"]} == {10, 25, 50, 100}
    assert catalog["slot_cumulative_result_thresholds"] == {"3": [0.2, 1.0], "5": [0.4, 1.0]}
    assert catalog["payout_provenance"] == "local_policy_not_fo76_parity"
    assert any("server" in item for item in catalog["unsupported_source_data"])


def test_catalog_rejects_non_cumulative_or_incomplete_curves(tmp_path):
    write_curves(tmp_path, three=[0.8, 0.7, 1.0])
    with pytest.raises(ValueError, match="cumulative"):
        build_casino_catalog(tmp_path)
    write_curves(tmp_path, three=[0.5, 0.9])
    with pytest.raises(ValueError, match="cumulative"):
        build_casino_catalog(tmp_path)
