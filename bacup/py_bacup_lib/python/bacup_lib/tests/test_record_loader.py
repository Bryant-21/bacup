"""Tests for record_loader — uses a temporary SQLite DB."""
from __future__ import annotations

import sqlite3
import tempfile
from pathlib import Path

import pytest


@pytest.fixture
def test_db(tmp_path):
    """Create a minimal records DB for testing."""
    db_path = tmp_path / "test_records.db"
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

    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("004822:Fallout4.esm", "10mm", "10mm", "Weapons", "10mm Pistol",
         "10mm pistol", "Fallout4.esm", "017E69:Fallout4.esm",
         "", "FormKey: 004822:Fallout4.esm\nEditorID: 10mm", 0),
    )
    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("01F276:Fallout4.esm", "Ammo10mm", "ammo 10mm", "Ammunitions",
         "10mm Round", "10mm round", "Fallout4.esm", "",
         "", "FormKey: 01F276:Fallout4.esm\nEditorID: Ammo10mm", 0),
    )
    conn.execute(
        "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("001234:Fallout4.esm", "LeatherArmor", "leather armor", "Armors",
         "Leather Armor", "leather armor", "Fallout4.esm", "",
         "", "FormKey: 001234:Fallout4.esm", 0),
    )
    conn.commit()
    conn.close()
    return db_path


def test_load_by_form_key(test_db):
    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    record = loader.load_by_form_key("004822:Fallout4.esm")

    assert record is not None
    assert record["editor_id"] == "10mm"
    assert record["record_type"] == "Weapons"


def test_load_by_form_key_not_found(test_db):
    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    record = loader.load_by_form_key("FFFFFF:Fallout4.esm")
    assert record is None


def test_search_by_editor_id(test_db):
    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    results = loader.search_by_editor_id("10mm")
    assert len(results) == 1
    assert results[0]["form_key"] == "004822:Fallout4.esm"


def test_list_record_types(test_db):
    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    types = loader.list_record_types()
    assert "Weapons" in types
    assert "Ammunitions" in types
    assert "Armors" in types


def test_list_records_by_type(test_db):
    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    records = loader.list_by_type("Weapons")
    assert len(records) == 1
    assert records[0]["editor_id"] == "10mm"


def test_search_records(test_db):
    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    # Search by name substring
    results = loader.search("leather")
    assert len(results) >= 1
    assert any(r["editor_id"] == "LeatherArmor" for r in results)


def test_search_by_editor_id_and_type(test_db):
    from creation_lib.db.record_loader import RecordLoader
    loader = RecordLoader(str(test_db))

    # Exact match on EditorID + type
    results = loader.search_by_editor_id_and_type("10mm", "Weapons")
    assert len(results) == 1
    assert results[0]["form_key"] == "004822:Fallout4.esm"

    # Same EditorID, wrong type -> no match
    results = loader.search_by_editor_id_and_type("10mm", "Armors")
    assert len(results) == 0

    # Case-insensitive
    results = loader.search_by_editor_id_and_type("10MM", "Weapons")
    assert len(results) == 1


def test_search_by_editor_id_and_type_accepts_signatures(test_db):
    from creation_lib.db.record_loader import RecordLoader
    loader = RecordLoader(str(test_db))

    results = loader.search_by_editor_id_and_type("10mm", "WEAP")

    assert len(results) == 1
    assert results[0]["record_type"] == "Weapons"


def test_search_by_editor_id_and_type_reuses_loaded_type_index(test_db):
    from unittest.mock import patch

    from creation_lib.db.record_loader import RecordLoader

    loader = RecordLoader(str(test_db))
    with patch.object(loader, "_connect", wraps=loader._connect) as connect_spy:
        weapons = loader.search_by_editor_id_and_type("10mm", "Weapons")
        armors = loader.search_by_editor_id_and_type("LeatherArmor", "Armors")
        weapons_again = loader.search_by_editor_id_and_type("10MM", "Weapons")

    assert len(weapons) == 1
    assert len(armors) == 1
    assert len(weapons_again) == 1
    # One DB load for Weapons, one for Armors; second Weapons lookup is cached.
    assert connect_spy.call_count == 2
