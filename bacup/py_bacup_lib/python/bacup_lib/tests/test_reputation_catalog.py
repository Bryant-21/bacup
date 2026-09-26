import json

import pytest

from bacup_lib import reputation_catalog as cat


def ref(object_id: int):
    return {"reference": {"plugin": "SeventySix.esm", "object_id": f"{object_id:06X}"}}


class Reader:
    def __init__(self):
        self.records = {}
        self.by_name = {}

    def add(self, object_id, signature, editor_id, **fields):
        grouped = {key: value if isinstance(value, list) else [value] for key, value in fields.items()}
        self.records[object_id] = (signature, editor_id, grouped)
        self.by_name[(signature, editor_id)] = object_id

    def lookup(self, signature, editor_id):
        object_id = self.by_name[(signature, editor_id)]
        return object_id, self.records[object_id][2]

    def fields(self, object_id):
        return self.records[object_id]


class Resolver:
    def __init__(self, missing=None):
        self.missing = missing

    def resolve(self, signature, editor_id):
        if (signature, editor_id) == self.missing:
            return None
        return f"SeventySix.esm:{sum(editor_id.encode()) & 0xFFFFFF:06X}"


def source_reader():
    reader = Reader()
    faction_ids = {"Crater": 0x3FE94A, "Foundation": 0x3FC008}
    faction_names = {"Crater": "W05_CraterRaiderFaction", "Foundation": "W05_SettlerFaction"}
    for faction_index, spec in enumerate(cat.FACTIONS):
        actor_id = 0x55C5BF + faction_index
        reader.add(actor_id, "AVIF", spec["actor_value"], MinimumValue=-3000.0,
                   MaximumValue=13000.0, DefaultValue=-2000.0 if faction_index == 0 else -800.0)
        reader.add(0x586F1F + faction_index, "DFOB", spec["actor_value_default"], Object=ref(actor_id))
        reader.add(faction_ids[spec["code"]], "FACT", faction_names[spec["code"]])
        reader.add(0x59F65B + faction_index, "DFOB", spec["faction_default"],
                   Object=ref(faction_ids[spec["code"]]))
        tiers = []
        for tier, threshold in enumerate((-2000, -1000, 0, 1000, 3000, 6000, 12000)):
            object_id = 0x55C5B1 + faction_index * 7 + tier
            tiers.append(ref(object_id))
            reader.add(object_id, "GLOB", f"Rep_Tier_{spec['code']}_{tier}", Value=float(threshold))
        reader.add(0x586F21 + faction_index, "FLST", spec["tier_list"], FormID=tiers)
    for offset, editor_id in enumerate(cat.SOUNDS.values()):
        reader.add(0x5A0304 + offset, "SNDR", editor_id)
    return reader


def test_catalog_is_source_derived_and_complete():
    catalog = cat.build_catalog(source_reader(), Resolver(), "SeventySix.esm", "SeventySix.esm", "abc")
    assert catalog["schema_version"] == 1
    assert catalog["source_movie_contract"] == {
        "movie": "hudreputationmeter.swf",
        "root_class": "HUDReputationUpdatesWidget",
        "factions": ["Crater", "Foundation"],
    }
    assert [row["code"] for row in catalog["factions"]] == ["Crater", "Foundation"]
    assert catalog["missing_converted"] == []
    for row in catalog["factions"]:
        assert row["available"] is True
        assert [tier["threshold"] for tier in row["tiers"]] == [
            -2000.0, -1000.0, 0.0, 1000.0, 3000.0, 6000.0, 12000.0
        ]
        assert row["actor_value"].startswith("SeventySix.esm:")
        assert row["faction"].startswith("SeventySix.esm:")
        assert all(tier["global"].startswith("SeventySix.esm:") for tier in row["tiers"])
    assert set(catalog["sounds"]) == {"increase", "decrease", "level_up"}
    json.dumps(catalog, allow_nan=False)


def test_catalog_rejects_invented_or_unsorted_thresholds():
    reader = source_reader()
    _, _, fields = reader.records[0x55C5B5]
    fields["Value"] = [-1001.0]
    with pytest.raises(ValueError, match="Non-increasing tier"):
        cat.build_catalog(reader, Resolver(), "SeventySix.esm", "SeventySix.esm", "abc")


def test_catalog_marks_missing_converted_faction_without_guessing_a_form():
    catalog = cat.build_catalog(
        source_reader(), Resolver(("AVIF", "Reputation_AV_Crater")),
        "SeventySix.esm", "SeventySix.esm", "abc",
    )
    crater = catalog["factions"][0]
    assert crater["available"] is False
    assert crater["actor_value"] is None
    assert catalog["factions"][1]["available"] is True
    assert catalog["missing_converted"] == ["AVIF:Reputation_AV_Crater"]


def test_emit_checks_inputs_before_loading_native(tmp_path, monkeypatch):
    monkeypatch.setattr(cat, "load_esp_native", lambda: pytest.fail("native should not load"))
    with pytest.raises(FileNotFoundError):
        cat.emit_reputation_catalog(tmp_path / "source.esm", tmp_path / "converted.esm", tmp_path)


def test_native_reader_inspects_raw_id_but_returns_local_id():
    class Native:
        def __init__(self):
            self.inspected = []

        def plugin_handle_search_records(self, handle, query, **kwargs):
            return [(0x08365BCC, "CONT", "Lootbag_Dropped_PaperBag", None)]

        def plugin_handle_inspect_record(self, handle, raw_id):
            self.inspected.append(raw_id)
            if raw_id != 0x08365BCC:
                return None
            return json.dumps({"signature": "CONT", "record": {
                "eid": "Lootbag_Dropped_PaperBag", "fields": [{"MODL": "bag.nif"}],
            }})

    native = Native()
    reader = cat.NativeReader(native, 1)
    object_id, fields = reader.lookup("CONT", "Lootbag_Dropped_PaperBag")
    assert object_id == 0x365BCC
    assert fields["MODL"] == ["bag.nif"]
    assert native.inspected == [0x08365BCC]


def test_native_reader_reports_missing_raw_record():
    class Native:
        def plugin_handle_inspect_record(self, handle, raw_id):
            return None

    with pytest.raises(ValueError, match="Unable to inspect record 08365BCC"):
        cat.NativeReader(Native(), 1).fields(0x08365BCC)
