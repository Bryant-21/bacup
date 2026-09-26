from types import SimpleNamespace

import pytest

from bacup_lib import legendary_perk_bindings as bindings


def test_catalog_uses_local_ids_and_ignores_non_runtime_helpers():
    result = bindings.binding_catalog("Converted.esm", [
        (0x01B04579, "PERK", "B21_Legendary_AmmoFactory_1", None),
        (0x01B04578, "GLOB", "B21_Legendary_AmmoFactory_1_Value", None),
        (0x01B04580, "SPEL", "B21_Legendary_LegendaryAgility_1_Ability", None),
        (0x01000800, "PERK", "LGN_LegendaryAgility_Perk01", None),
    ])
    assert result == {"schema_version": 1, "plugin": "Converted.esm", "forms": {
        "B21_Legendary_AmmoFactory_1": 0xB04579,
        "B21_Legendary_AmmoFactory_1_Value": 0xB04578,
    }}


@pytest.mark.parametrize("records", [
    [(0, "PERK", "B21_Legendary_AmmoFactory_1", None)],
    [(0x800, "GLOB", "B21_Legendary_AmmoFactory_1", None)],
    [(0x800, "PERK", "B21_Legendary_AmmoFactory_1_Value", None)],
    [(0x800, "PERK", "B21_Legendary_AmmoFactory_1", None)] * 2,
])
def test_invalid_bindings_are_rejected(records):
    with pytest.raises(ValueError):
        bindings.binding_catalog("Converted.esm", records)


def test_emitter_owns_and_closes_its_native_handle(tmp_path, monkeypatch):
    closed = []
    calls = []
    def search(handle, pattern, **kwargs):
        calls.append((handle, pattern, kwargs))
        return [(0x01B04579, "PERK", "B21_Legendary_AmmoFactory_1", None)]
    native = SimpleNamespace(plugin_handle_load_index=lambda *a, **kw: 123,
        plugin_handle_search_records=search, plugin_handle_close=closed.append)
    monkeypatch.setattr(bindings, "load_esp_native", lambda: native)
    output = tmp_path / "converted"
    result = bindings.emit_legendary_perk_bindings(tmp_path / "Converted.esm", output)
    assert result["forms"]["B21_Legendary_AmmoFactory_1"] == 0xB04579
    assert (output / bindings.OUTPUT).is_file()
    assert calls == [(123, "B21_Legendary_*", {"signatures": ["PERK", "GLOB"], "case_sensitive": True})]
    assert closed == [123]
    native.plugin_handle_search_records = lambda *a, **kw: [(0, "PERK", "B21_Legendary_AmmoFactory_1", None)]
    with pytest.raises(ValueError):
        bindings.emit_legendary_perk_bindings(tmp_path / "Converted.esm", output)
    assert closed == [123, 123]
