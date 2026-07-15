"""Tests for FormKeyMapper — FormKey remapping and allocation."""
from __future__ import annotations

import json
import sqlite3
from pathlib import Path

import pytest


class _FakeTargetHandle:
    def __init__(self, rust_handle: int = 99) -> None:
        self.plugin_name = "Fallout4.esm"
        self.file_path = Path(str(rust_handle))


def _patch_eid_rows(monkeypatch, rows_by_handle: dict[int, list[dict]]) -> dict[int, int]:
    calls: dict[int, int] = {}

    def record_index_rows(handle) -> list[tuple[str, str, str, int, int]]:
        handle_id = int(handle.file_path.name)
        calls[handle_id] = calls.get(handle_id, 0) + 1
        return [
            (
                str(row["form_key"]),
                str(row["editor_id"]),
                str(row["signature"]),
                0,
                0,
            )
            for row in rows_by_handle.get(handle_id, [])
        ]

    monkeypatch.setattr(_FakeTargetHandle, "record_index_rows", record_index_rows, raising=False)
    return calls


class _FailingTargetLoader:
    def search_by_editor_id_and_type(self, editor_id: str, record_type: str) -> list[dict]:
        raise AssertionError("DB target_loader fallback should not run when handle lookup matches")


@pytest.fixture
def target_db(tmp_path):
    """Create a target game records DB with some vanilla records."""
    db_path = tmp_path / "fo4_records.db"
    conn = sqlite3.connect(str(db_path))
    conn.execute("""
        CREATE TABLE records (
            form_key TEXT PRIMARY KEY,
            editor_id TEXT,
            editor_id_tokens TEXT,
            record_type TEXT,
            name TEXT,
            name_tokens TEXT,
            source TEXT,
            keywords TEXT,
            yaml_path TEXT,
            content TEXT,
            node_index INTEGER
        )
    """)
    conn.execute("CREATE INDEX idx_records_type ON records(record_type)")
    conn.execute("CREATE INDEX idx_records_editor_id ON records(editor_id)")

    # Vanilla FO4 records
    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("013F42:Fallout4.esm", "RightHand", "right hand", "EquipTypes",
         "Right Hand", "", "Fallout4.esm", "", "", "", 0),
    )
    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("023465:Fallout4.esm", "WeaponTypeRifle", "weapon type rifle", "Keywords",
         "Weapon Type - Rifle", "", "Fallout4.esm", "", "", "", 0),
    )
    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("0001F4:Fallout4.esm", "GaussRifle", "gauss rifle", "Weapons",
         "Gauss Rifle", "", "Fallout4.esm", "", "", "", 0),
    )
    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("1870F4:Fallout4.esm", "RandomEncounters", "random encounters", "LAYR",
         "", "", "Fallout4.esm", "", "", "", 0),
    )
    conn.commit()
    conn.close()
    return db_path


@pytest.fixture
def mod_path(tmp_path):
    """Create a temp mod output directory."""
    mod_dir = tmp_path / "B21_TestMod"
    mod_dir.mkdir()
    return mod_dir


def test_vanilla_remap(target_db, mod_path):
    """Records with matching EditorID+type in target DB get remapped to vanilla."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    result = mapper.map_formkey(
        source_formkey="591667:SeventySix.esm",
        editor_id="RightHand",
        record_type="EquipTypes",
    )

    assert result["new_formkey"] == "013F42:Fallout4.esm"
    assert result["strategy"] == "vanilla_remap"


def test_vanilla_remap_prefers_target_master_handles(mod_path, monkeypatch):
    """Target master handles are authoritative for vanilla remap when provided."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    target_handle = _FakeTargetHandle()
    calls = _patch_eid_rows(
        monkeypatch,
        {
            99: [
                {
                    "form_key": "Fallout4.esm:099999",
                    "editor_id": "RightHand",
                    "signature": "EQUP",
                }
            ]
        },
    )
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=_FailingTargetLoader(),
        target_master_handles=[target_handle],
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    result = mapper.map_formkey(
        source_formkey="591667:SeventySix.esm",
        editor_id="RightHand",
        record_type="EquipTypes",
    )

    assert result["new_formkey"] == "099999:Fallout4.esm"
    assert result["strategy"] == "vanilla_remap"
    assert calls == {99: 1}


def test_preserves_source_object_id_by_default(target_db, mod_path):
    """Records with no vanilla match keep their source object ID by default."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    result = mapper.map_formkey(
        source_formkey="55C153:SeventySix.esm",
        editor_id="Cattleprod",
        record_type="Weapons",
    )

    assert result["new_formkey"] == "55C153:B21_TestMod.esp"
    assert result["strategy"] == "source_id_preserved"


def test_preserves_source_object_id_with_output_plugin_extension(target_db, mod_path):
    """Preserved local FormKeys use the configured output plugin extension."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
        output_plugin_extension=".esm",
    )

    result = mapper.map_formkey(
        source_formkey="55C153:SeventySix.esm",
        editor_id="Cattleprod",
        record_type="Weapons",
    )

    assert result["new_formkey"] == "55C153:B21_TestMod.esm"
    assert result["strategy"] == "source_id_preserved"


def test_opt_out_new_allocation(target_db, mod_path):
    """preserve_source_ids=False keeps the legacy sequential allocation path."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
        preserve_source_ids=False,
    )

    result = mapper.map_formkey(
        source_formkey="55C153:SeventySix.esm",
        editor_id="Cattleprod",
        record_type="Weapons",
    )

    assert result["new_formkey"] == "000800:B21_TestMod.esp"
    assert result["strategy"] == "new_allocation"


def test_preserved_source_id_collision_allocates_fallback(target_db, mod_path):
    """If two source plugins share an object ID, only the collision allocates."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    first = mapper.map_formkey("123456:OneSource.esm", "FirstThing", "MiscItems")
    second = mapper.map_formkey("123456:OtherSource.esm", "SecondThing", "MiscItems")

    assert first["new_formkey"] == "123456:B21_TestMod.esp"
    assert first["strategy"] == "source_id_preserved"
    assert second["new_formkey"] == "000800:B21_TestMod.esp"
    assert second["strategy"] == "new_allocation"


def test_preserved_source_ids_do_not_consume_allocation_range(target_db, mod_path):
    """Multiple preserved source IDs keep their own object IDs."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    r1 = mapper.map_formkey("55C153:SeventySix.esm", "Cattleprod", "Weapons")
    r2 = mapper.map_formkey("55C154:SeventySix.esm", "CattleprodMod1", "ObjectModifications")

    assert r1["new_formkey"] == "55C153:B21_TestMod.esp"
    assert r2["new_formkey"] == "55C154:B21_TestMod.esp"


def test_incremental_from_existing_map(target_db, mod_path):
    """Existing mappings are honored; new source records preserve IDs by default."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    # Pre-populate a formkey_map.json
    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000805",
        "mappings": {
            "55C153:SeventySix.esm": {
                "new_formkey": "000800:B21_TestMod.esp",
                "editor_id": "Cattleprod",
                "record_type": "Weapons",
                "strategy": "new_allocation",
                "source_game": "fo76",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    # Already-mapped record returns existing mapping
    r1 = mapper.map_formkey("55C153:SeventySix.esm", "Cattleprod", "Weapons")
    assert r1["new_formkey"] == "000800:B21_TestMod.esp"

    # New records preserve their source object ID by default; next_id is only
    # used by legacy opt-out allocation or collisions.
    r2 = mapper.map_formkey("55C155:SeventySix.esm", "CattleprodMod2", "ObjectModifications")
    assert r2["new_formkey"] == "55C155:B21_TestMod.esp"


def test_existing_local_mapping_updates_output_plugin_extension(target_db, mod_path):
    """Cached local mappings follow the current conversion plugin extension."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000800",
        "mappings": {
            "55C153:SeventySix.esm": {
                "new_formkey": "55C153:B21_TestMod.esp",
                "editor_id": "Cattleprod",
                "record_type": "Weapons",
                "strategy": "source_id_preserved",
                "source_game": "fo76",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
        output_plugin_extension=".esm",
    )

    result = mapper.map_formkey("55C153:SeventySix.esm", "Cattleprod", "Weapons")

    assert result["new_formkey"] == "55C153:B21_TestMod.esm"
    assert result["strategy"] == "source_id_preserved"


def test_cached_source_id_preserved_does_not_recheck_vanilla(mod_path):
    """Cached source-id mappings are stable and do not hit target lookup on reconvert."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000800",
        "mappings": {
            "55C153:SeventySix.esm": {
                "new_formkey": "55C153:B21_TestMod.esp",
                "editor_id": "Cattleprod",
                "record_type": "Weapons",
                "strategy": "source_id_preserved",
                "source_game": "fnv",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    class FailingTargetLoader:
        def search_by_editor_id_and_type(self, editor_id, record_type):
            raise AssertionError("cached source_id_preserved should not query target DB")

    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=FailingTargetLoader(),
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    def fail_find_vanilla(editor_id, record_type):
        raise AssertionError("cached source_id_preserved should not recheck vanilla")

    mapper._find_vanilla_match = fail_find_vanilla

    result = mapper.map_formkey("55C153:SeventySix.esm", "Cattleprod", "Weapons")

    assert result["new_formkey"] == "55C153:B21_TestMod.esp"
    assert result["strategy"] == "source_id_preserved"


def test_cached_source_id_preserved_system_record_upgrades_to_vanilla(target_db, mod_path):
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000800",
        "mappings": {
            "27E044:SeventySix.esm": {
                "new_formkey": "27E044:B21_TestMod.esp",
                "editor_id": "RandomEncounters",
                "record_type": "LAYR",
                "strategy": "source_id_preserved",
                "source_game": "fo76",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    result = mapper.map_formkey(
        source_formkey="27E044:SeventySix.esm",
        editor_id="RandomEncounters",
        record_type="LAYR",
    )

    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "1870F4:Fallout4.esm"
    assert mapper._mappings["27E044:SeventySix.esm"]["new_formkey"] == "1870F4:Fallout4.esm"


def test_base_game_disabled_business_records(target_db, mod_path):
    """With use_base_game_assets=False, content records (Weapons, Armors, NPCs)
    preserve source IDs even when an EditorID match exists in the target DB.
    Standalone-mod conversions clone the convertable record into the mod.
    """
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    result = mapper.map_formkey(
        source_formkey="56DCA4:SeventySix.esm",
        editor_id="GaussRifle",
        record_type="Weapons",
    )

    assert result["strategy"] == "source_id_preserved"
    assert result["new_formkey"] == "56DCA4:B21_TestMod.esp"


def test_base_game_disabled_layers_still_remap_by_editor_id(target_db, mod_path):
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    result = mapper.map_formkey(
        source_formkey="27E044:SeventySix.esm",
        editor_id="RandomEncounters",
        record_type="LAYR",
    )

    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "1870F4:Fallout4.esm"


def test_base_game_disabled_system_records_still_remap(target_db, mod_path):
    """Game-system records (Keywords, EquipTypes, etc.) auto-remap to vanilla
    even with use_base_game_assets=False — these are never legitimately
    cloned, and cloning them would surface as broken sub-field references
    in xEdit (e.g. IPDS PNAM Material -> NULL).
    """
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    result = mapper.map_formkey(
        source_formkey="591667:SeventySix.esm",
        editor_id="RightHand",
        record_type="EquipTypes",
    )

    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "013F42:Fallout4.esm"


def test_base_game_disabled_system_signature_records_still_remap(target_db, mod_path):
    """Canonical signatures get the same system-record remap policy."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    result = mapper.map_formkey(
        source_formkey="591667:SeventySix.esm",
        editor_id="RightHand",
        record_type="EQUP",
    )

    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "013F42:Fallout4.esm"


def test_rewrite_formkeys_in_yaml():
    """Recursive FormKey rewriting in YAML dicts."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    mapping = {
        "591667:SeventySix.esm": {"new_formkey": "013F42:Fallout4.esm"},
        "55C153:SeventySix.esm": {"new_formkey": "000800:B21_Test.esp"},
    }

    record = {
        "FormKey": "55C153:SeventySix.esm",
        "EditorID": "Cattleprod",
        "EquipmentType": "591667:SeventySix.esm",
        "Keywords": [
            "591667:SeventySix.esm",
            "AAAAAA:SomeOther.esm",
        ],
        "Nested": {
            "Ref": "55C153:SeventySix.esm",
        },
        "NotAFormKey": "hello world",
        "Number": 42,
    }

    result = FormKeyMapper.rewrite_formkeys(record, mapping)

    assert result["FormKey"] == "000800:B21_Test.esp"
    assert result["EquipmentType"] == "013F42:Fallout4.esm"
    assert result["Keywords"][0] == "013F42:Fallout4.esm"
    assert result["Keywords"][1] == "AAAAAA:SomeOther.esm"  # unmapped, unchanged
    assert result["Nested"]["Ref"] == "000800:B21_Test.esp"
    assert result["NotAFormKey"] == "hello world"  # not a FormKey pattern
    assert result["Number"] == 42


def test_save_and_load(target_db, mod_path):
    """formkey_map.json round-trips correctly."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    mapper.map_formkey("55C153:SeventySix.esm", "Cattleprod", "Weapons")
    mapper.map_formkey("591667:SeventySix.esm", "RightHand", "EquipTypes")
    mapper.save()

    map_path = mod_path / "formkey_map.json"
    assert map_path.exists()

    data = json.loads(map_path.read_text())
    assert data["mod_name"] == "B21_TestMod"
    assert data["next_id"] == "000800"
    assert data["use_base_game_assets"] is True
    assert data["preserve_source_ids"] is True
    assert len(data["mappings"]) == 2
    assert data["mappings"]["591667:SeventySix.esm"]["strategy"] == "vanilla_remap"
    assert data["mappings"]["55C153:SeventySix.esm"]["strategy"] == "source_id_preserved"


def test_stale_new_allocation_upgrades_to_vanilla_remap(target_db, mod_path):
    """Stale cached new_allocation entries self-heal to vanilla_remap on reconvert.

    Regression for: early runs with use_base_game_assets=False (or with a
    target DB that lacked the record) cached a new_allocation mapping. On
    a later reconvert with use_base_game_assets=True and a vanilla match
    available, the cache was returning the bad new_allocation instead of
    upgrading. Symptom: duplicate Ammo2mmEC record and other records
    shadowing base game entries.
    """
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    # Pre-populate a stale cache entry: EquipTypes/RightHand mapped to
    # a fresh new_allocation FormKey, even though the target DB has
    # the vanilla 013F42:Fallout4.esm copy.
    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000801",
        "mappings": {
            "591667:SeventySix.esm": {
                "new_formkey": "000800:B21_TestMod.esp",
                "editor_id": "RightHand",
                "record_type": "EquipTypes",
                "strategy": "new_allocation",
                "source_game": "fo76",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    target_loader = RecordLoader(str(target_db))
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    result = mapper.map_formkey(
        source_formkey="591667:SeventySix.esm",
        editor_id="RightHand",
        record_type="EquipTypes",
    )

    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "013F42:Fallout4.esm"
    # And the internal cache is updated, not just the returned value
    assert mapper._mappings["591667:SeventySix.esm"]["strategy"] == "vanilla_remap"
    assert mapper._mappings["591667:SeventySix.esm"]["new_formkey"] == "013F42:Fallout4.esm"


def test_stale_new_allocation_upgrade_uses_target_master_handles(mod_path, monkeypatch):
    """Stale local cache entries upgrade via target handles before DB fallback."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000801",
        "mappings": {
            "591667:SeventySix.esm": {
                "new_formkey": "000800:B21_TestMod.esp",
                "editor_id": "RightHand",
                "record_type": "EquipTypes",
                "strategy": "new_allocation",
                "source_game": "fo76",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    target_handle = _FakeTargetHandle()
    _patch_eid_rows(
        monkeypatch,
        {
            99: [
                {
                    "form_key": "Fallout4.esm:099999",
                    "editor_id": "RightHand",
                    "signature": "EQUP",
                }
            ]
        },
    )
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=_FailingTargetLoader(),
        target_master_handles=[target_handle],
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    result = mapper.map_formkey("591667:SeventySix.esm", "RightHand", "EquipTypes")

    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "099999:Fallout4.esm"
    assert mapper._mappings["591667:SeventySix.esm"]["new_formkey"] == "099999:Fallout4.esm"


def test_vanilla_remap_never_downgrades(target_db, mod_path):
    """Never downgrade vanilla_remap back to new_allocation — would break save games."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000800",
        "mappings": {
            "591667:SeventySix.esm": {
                "new_formkey": "013F42:Fallout4.esm",
                "editor_id": "RightHand",
                "record_type": "EquipTypes",
                "strategy": "vanilla_remap",
                "source_game": "fo76",
            }
        },
    }
    with open(mod_path / "formkey_map.json", "w") as f:
        json.dump(existing, f)

    target_loader = RecordLoader(str(target_db))
    # Even with use_base_game_assets=False, cached vanilla_remap stays
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=target_loader,
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    result = mapper.map_formkey("591667:SeventySix.esm", "RightHand", "EquipTypes")
    assert result["strategy"] == "vanilla_remap"
    assert result["new_formkey"] == "013F42:Fallout4.esm"


def test_find_vanilla_uses_target_master_handles(mod_path, monkeypatch):
    """find_vanilla resolves through target handles even without a DB loader."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    target_handle = _FakeTargetHandle()
    _patch_eid_rows(
        monkeypatch,
        {
            99: [
                {
                    "form_key": "Fallout4.esm:099999",
                    "editor_id": "RightHand",
                    "signature": "EQUP",
                }
            ]
        },
    )
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=None,
        target_master_handles=[target_handle],
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    assert mapper.find_vanilla("RightHand", "EquipTypes") == "099999:Fallout4.esm"
