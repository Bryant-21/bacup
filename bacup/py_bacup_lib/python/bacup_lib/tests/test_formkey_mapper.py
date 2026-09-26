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


def _mapper(target_db, mod_path, **kwargs):
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper
    from creation_lib.db.record_loader import RecordLoader

    kwargs.setdefault("use_base_game_assets", True)
    return FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=RecordLoader(str(target_db)),
        mod_path=str(mod_path),
        **kwargs,
    )


def _write_existing_map(mod_path, source_fk, new_fk, editor_id, record_type, strategy):
    existing = {
        "mod_name": "B21_TestMod",
        "target_game": "fo4",
        "next_id": "000801",
        "mappings": {
            source_fk: {
                "new_formkey": new_fk,
                "editor_id": editor_id,
                "record_type": record_type,
                "strategy": strategy,
                "source_game": "fo76",
            }
        },
    }
    (mod_path / "formkey_map.json").write_text(json.dumps(existing), encoding="utf-8")


@pytest.mark.parametrize(
    ("kwargs", "source_fk", "editor_id", "record_type", "expected_fk", "strategy"),
    [
        ({}, "591667:SeventySix.esm", "RightHand", "EquipTypes", "013F42:Fallout4.esm", "vanilla_remap"),
        ({}, "55C153:SeventySix.esm", "Cattleprod", "Weapons", "55C153:B21_TestMod.esp", "source_id_preserved"),
        (
            {"output_plugin_extension": ".esm"},
            "55C153:SeventySix.esm", "Cattleprod", "Weapons", "55C153:B21_TestMod.esm", "source_id_preserved",
        ),
        (
            {"preserve_source_ids": False},
            "55C153:SeventySix.esm", "Cattleprod", "Weapons", "000800:B21_TestMod.esp", "new_allocation",
        ),
        # Standalone conversions clone content records even when an EditorID matches.
        (
            {"use_base_game_assets": False},
            "56DCA4:SeventySix.esm", "GaussRifle", "Weapons", "56DCA4:B21_TestMod.esp", "source_id_preserved",
        ),
        (
            {"use_base_game_assets": False},
            "27E044:SeventySix.esm", "RandomEncounters", "LAYR", "1870F4:Fallout4.esm", "vanilla_remap",
        ),
        # System records are never cloned: that surfaces as NULL sub-field refs in xEdit.
        (
            {"use_base_game_assets": False},
            "591667:SeventySix.esm", "RightHand", "EquipTypes", "013F42:Fallout4.esm", "vanilla_remap",
        ),
        (
            {"use_base_game_assets": False},
            "591667:SeventySix.esm", "RightHand", "EQUP", "013F42:Fallout4.esm", "vanilla_remap",
        ),
    ],
)
def test_fresh_mapping_policy(
    target_db, mod_path, kwargs, source_fk, editor_id, record_type, expected_fk, strategy
):
    result = _mapper(target_db, mod_path, **kwargs).map_formkey(
        source_formkey=source_fk,
        editor_id=editor_id,
        record_type=record_type,
    )

    assert result["new_formkey"] == expected_fk
    assert result["strategy"] == strategy


def test_preserved_source_id_collision_allocates_fallback(target_db, mod_path):
    """If two source plugins share an object ID, only the collision allocates."""
    mapper = _mapper(target_db, mod_path)

    first = mapper.map_formkey("123456:OneSource.esm", "FirstThing", "MiscItems")
    second = mapper.map_formkey("123456:OtherSource.esm", "SecondThing", "MiscItems")
    third = mapper.map_formkey("123457:OneSource.esm", "ThirdThing", "MiscItems")

    assert first["new_formkey"] == "123456:B21_TestMod.esp"
    assert first["strategy"] == "source_id_preserved"
    assert second["new_formkey"] == "000800:B21_TestMod.esp"
    assert second["strategy"] == "new_allocation"
    assert third["new_formkey"] == "123457:B21_TestMod.esp"


@pytest.mark.parametrize(
    ("cached", "kwargs", "expected_fk", "strategy"),
    [
        (
            ("55C153:SeventySix.esm", "000800:B21_TestMod.esp", "Cattleprod", "Weapons", "new_allocation"),
            {},
            "000800:B21_TestMod.esp",
            "new_allocation",
        ),
        (
            ("55C153:SeventySix.esm", "55C153:B21_TestMod.esp", "Cattleprod", "Weapons", "source_id_preserved"),
            {"output_plugin_extension": ".esm"},
            "55C153:B21_TestMod.esm",
            "source_id_preserved",
        ),
        (
            ("27E044:SeventySix.esm", "27E044:B21_TestMod.esp", "RandomEncounters", "LAYR", "source_id_preserved"),
            {"use_base_game_assets": False},
            "1870F4:Fallout4.esm",
            "vanilla_remap",
        ),
        # Stale new_allocation entries self-heal; otherwise they shadow base game records.
        (
            ("591667:SeventySix.esm", "000800:B21_TestMod.esp", "RightHand", "EquipTypes", "new_allocation"),
            {},
            "013F42:Fallout4.esm",
            "vanilla_remap",
        ),
        # Never downgrade vanilla_remap: that would break save games.
        (
            ("591667:SeventySix.esm", "013F42:Fallout4.esm", "RightHand", "EquipTypes", "vanilla_remap"),
            {"use_base_game_assets": False},
            "013F42:Fallout4.esm",
            "vanilla_remap",
        ),
    ],
)
def test_cached_mapping_policy(target_db, mod_path, cached, kwargs, expected_fk, strategy):
    source_fk, _new_fk, editor_id, record_type, _strategy = cached
    _write_existing_map(mod_path, *cached)
    mapper = _mapper(target_db, mod_path, **kwargs)

    result = mapper.map_formkey(source_fk, editor_id, record_type)

    assert result["new_formkey"] == expected_fk
    assert result["strategy"] == strategy
    assert mapper._mappings[source_fk]["new_formkey"] == expected_fk


def test_cached_source_id_preserved_does_not_recheck_vanilla(mod_path):
    """Cached source-id mappings are stable and do not hit target lookup on reconvert."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    _write_existing_map(
        mod_path,
        "55C153:SeventySix.esm",
        "55C153:B21_TestMod.esp",
        "Cattleprod",
        "Weapons",
        "source_id_preserved",
    )

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


@pytest.mark.parametrize("stale_cache", [False, True])
def test_vanilla_remap_prefers_target_master_handles(mod_path, monkeypatch, stale_cache):
    """Target master handles are authoritative for vanilla remap when provided."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    if stale_cache:
        _write_existing_map(
            mod_path,
            "591667:SeventySix.esm",
            "000800:B21_TestMod.esp",
            "RightHand",
            "EquipTypes",
            "new_allocation",
        )
    calls = _patch_eid_rows(
        monkeypatch,
        {99: [{"form_key": "Fallout4.esm:099999", "editor_id": "RightHand", "signature": "EQUP"}]},
    )
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=_FailingTargetLoader(),
        target_master_handles=[_FakeTargetHandle()],
        mod_path=str(mod_path),
        use_base_game_assets=True,
    )

    result = mapper.map_formkey("591667:SeventySix.esm", "RightHand", "EquipTypes")

    assert result["new_formkey"] == "099999:Fallout4.esm"
    assert result["strategy"] == "vanilla_remap"
    assert mapper._mappings["591667:SeventySix.esm"]["new_formkey"] == "099999:Fallout4.esm"
    assert calls == {99: 1}


def test_find_vanilla_uses_target_master_handles(mod_path, monkeypatch):
    """find_vanilla resolves through target handles even without a DB loader."""
    from bacup_lib.formkey.formkey_mapper import FormKeyMapper

    _patch_eid_rows(
        monkeypatch,
        {99: [{"form_key": "Fallout4.esm:099999", "editor_id": "RightHand", "signature": "EQUP"}]},
    )
    mapper = FormKeyMapper(
        mod_name="B21_TestMod",
        target_game="fo4",
        target_loader=None,
        target_master_handles=[_FakeTargetHandle()],
        mod_path=str(mod_path),
        use_base_game_assets=False,
    )

    assert mapper.find_vanilla("RightHand", "EquipTypes") == "099999:Fallout4.esm"


def test_save_and_load(target_db, mod_path):
    """formkey_map.json round-trips correctly."""
    mapper = _mapper(target_db, mod_path)

    mapper.map_formkey("55C153:SeventySix.esm", "Cattleprod", "Weapons")
    mapper.map_formkey("591667:SeventySix.esm", "RightHand", "EquipTypes")
    mapper.save()

    data = json.loads((mod_path / "formkey_map.json").read_text())
    assert data["mod_name"] == "B21_TestMod"
    assert data["next_id"] == "000800"
    assert data["use_base_game_assets"] is True
    assert data["preserve_source_ids"] is True
    assert len(data["mappings"]) == 2
    assert data["mappings"]["591667:SeventySix.esm"]["strategy"] == "vanilla_remap"
    assert data["mappings"]["55C153:SeventySix.esm"]["strategy"] == "source_id_preserved"
