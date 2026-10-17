import json
from types import SimpleNamespace

import pytest

from bacup_lib import realtime_vats_catalog as catalog
from bacup_lib.tests.catalog_schema_validation import validate_catalog


class Source:
    def __init__(self, relative="SPECIAL\\LuckCriticalChargeRate.json", signature="CURV"):
        self.records = {
            1: ("DFOB", catalog.LUCK_CURVE, {"Object": [{"reference": {"plugin": "SeventySix.esm", "object_id": "000002"}}]}),
            2: (signature, "CT_LuckVATSCriticalCharge", {"JASF": [relative]}),
        }

    def fields(self, raw):
        return self.records[raw]

    def lookup(self, signature, eid):
        raw = next(raw for raw, (sig, name, _) in self.records.items() if sig == signature and name == eid)
        return raw, self.records[raw][2]


def write_curve(root, points, name="special/luckcriticalchargerate.json"):
    path = root / "misc/curvetables/json" / name
    path.parent.mkdir(parents=True)
    path.write_text(json.dumps({"curve": points}), encoding="utf-8")


def test_emits_source_luck_curve_beside_tales_plugin(tmp_path):
    write_curve(tmp_path / "fo76", [{"x": 1, "y": 3}, {"x": 10, "y": 17}, {"x": 100, "y": 45}])
    result = catalog.emit_catalog(Source(), tmp_path / "mod", tmp_path / "fo76")
    assert result["luck_critical_charge_curve"] == [{"x": 1.0, "y": 3.0}, {"x": 10.0, "y": 17.0}, {"x": 100.0, "y": 45.0}]
    assert result["source"] == {"default_object": catalog.LUCK_CURVE, "curve": "CT_LuckVATSCriticalCharge",
                                "file": "SPECIAL\\LuckCriticalChargeRate.json"}
    output = tmp_path / "mod/F4SE/Plugins/B21_TalesFromAppalachia/RealtimeVATS/SeventySix.esm.json"
    written = json.loads(output.read_text(encoding="utf-8"))
    assert written == catalog.to_document(result)
    validate_catalog("RealtimeVATS", written)


@pytest.mark.parametrize("points", [[], [{"x": 2, "y": 1}, {"x": 2, "y": 3}], [{"x": 1, "y": -1}],
                                    [{"x": 1, "y": float("nan")}]])
def test_rejects_unusable_curves(tmp_path, points):
    write_curve(tmp_path, points)
    with pytest.raises(ValueError):
        catalog.build_catalog(Source(), tmp_path)


def test_rejects_non_curve_and_escaping_paths(tmp_path):
    write_curve(tmp_path, [{"x": 1, "y": 3}])
    with pytest.raises(ValueError):
        catalog.build_catalog(Source(signature="GLOB"), tmp_path)
    with pytest.raises(ValueError):
        catalog.build_catalog(Source(relative="..\\..\\..\\outside.json"), tmp_path)


def test_pipeline_pair_gate(monkeypatch, tmp_path):
    from bacup_lib.workflows import unified
    context = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "converted",
                              source_plugin_path=tmp_path / "SeventySix.esm")
    calls = []
    monkeypatch.setattr(catalog, "emit_from_plugin", lambda plugin, mod, root:
                        calls.append((plugin, mod, root)) or {"luck_critical_charge_curve": [1, 2]})
    for source_game, target in (("skyrimse", "fo4"), ("fo76", "skyrimse")):
        assert unified._emit_fo76_realtime_vats_catalog(SimpleNamespace(source_game=source_game, target_game=target), context) == 0
    assert calls == []
    assert unified._emit_fo76_realtime_vats_catalog(SimpleNamespace(source_game="fo76", target_game="fo4"), context) == 1
    assert calls == [(context.source_plugin_path, context.mod_path, context.source_data_dir)]
