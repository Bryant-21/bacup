"""Tests for FormKeyMapper.rewrite_formkeys on canonical-shape records.

Canonical-shape records embed FormKey references as
``{reference: {plugin, object_id}}`` dicts (Task 2/3 reindex output).
These tests cover the walker's recognition and rewriting of that shape
in addition to the legacy ``"OBJID:Plugin.esm"`` string form.
"""
from __future__ import annotations

import os
from pathlib import Path

import pytest
import yaml

from bacup_lib.formkey.formkey_mapper import FormKeyMapper


def test_canonical_ref_is_rewritten():
    """A bare {reference: {...}} dict gets its plugin/object_id swapped."""
    mapping = {
        "248AB9:Fallout4.esm": {"new_formkey": "000800:B21_Test.esp"},
    }
    data = {"reference": {"plugin": "Fallout4.esm", "object_id": "248AB9"}}
    result = FormKeyMapper.rewrite_formkeys(data, mapping)
    assert result == {
        "reference": {"plugin": "B21_Test.esp", "object_id": "000800"},
    }


def test_canonical_ref_unmapped_left_alone():
    """Unmapped canonical refs pass through untouched."""
    mapping = {
        "248AB9:Fallout4.esm": {"new_formkey": "000800:B21_Test.esp"},
    }
    data = {"reference": {"plugin": "Fallout4.esm", "object_id": "ABCDEF"}}
    result = FormKeyMapper.rewrite_formkeys(data, mapping)
    assert result == data


def test_string_fk_still_rewritten():
    """Existing string-FK behavior unchanged."""
    mapping = {
        "591667:SeventySix.esm": {"new_formkey": "013F42:Fallout4.esm"},
    }
    result = FormKeyMapper.rewrite_formkeys("591667:SeventySix.esm", mapping)
    assert result == "013F42:Fallout4.esm"


def test_mixed_string_and_canonical():
    """A record with both shapes — both get rewritten."""
    mapping = {
        "591667:SeventySix.esm": {"new_formkey": "013F42:Fallout4.esm"},
        "55C153:SeventySix.esm": {"new_formkey": "000800:B21_Test.esp"},
        "248AB9:SeventySix.esm": {"new_formkey": "248AB9:Fallout4.esm"},
    }
    record = {
        "FormKey": "55C153:SeventySix.esm",
        "EquipmentType": "591667:SeventySix.esm",
        "PreviewTransform": {
            "reference": {"plugin": "SeventySix.esm", "object_id": "248AB9"},
        },
    }
    result = FormKeyMapper.rewrite_formkeys(record, mapping)
    assert result["FormKey"] == "000800:B21_Test.esp"
    assert result["EquipmentType"] == "013F42:Fallout4.esm"
    assert result["PreviewTransform"] == {
        "reference": {"plugin": "Fallout4.esm", "object_id": "248AB9"},
    }


def test_nested_canonical_in_list_in_dict():
    """Canonical ref nested inside a list inside another dict gets reached."""
    mapping = {
        "0F4AE8:Fallout4.esm": {"new_formkey": "ABC123:B21_Test.esp"},
    }
    record = {
        "fields": [
            {"Keywords": [
                {"reference": {"plugin": "Fallout4.esm", "object_id": "0F4AE8"}},
                {"reference": {"plugin": "Fallout4.esm", "object_id": "DEADBE"}},
            ]},
        ],
    }
    result = FormKeyMapper.rewrite_formkeys(record, mapping)
    kw_list = result["fields"][0]["Keywords"]
    assert kw_list[0] == {
        "reference": {"plugin": "B21_Test.esp", "object_id": "ABC123"},
    }
    # Unmapped sibling untouched.
    assert kw_list[1] == {
        "reference": {"plugin": "Fallout4.esm", "object_id": "DEADBE"},
    }


def test_canonical_ref_with_sibling_keys_preserved():
    """A dict that has 'reference' alongside other keys keeps the siblings.

    This protects against future shapes like
    ``{reference: {...}, condition: ...}`` — currently the canonical
    extractor doesn't emit those, but the walker should not lose data.
    """
    mapping = {
        "248AB9:Fallout4.esm": {"new_formkey": "000800:B21_Test.esp"},
    }
    data = {
        "reference": {"plugin": "Fallout4.esm", "object_id": "248AB9"},
        "weight": 1.0,
    }
    result = FormKeyMapper.rewrite_formkeys(data, mapping)
    assert result["reference"] == {
        "plugin": "B21_Test.esp", "object_id": "000800",
    }
    assert result["weight"] == 1.0


def test_top_level_record_with_fields_list():
    """End-to-end: canonical-shape record with fields list — all refs rewritten."""
    mapping = {
        "248AB9:Fallout4.esm": {"new_formkey": "000800:B21_Test.esp"},
        "04334D:Fallout4.esm": {"new_formkey": "000801:B21_Test.esp"},
    }
    record = {
        "form_id": "004822",
        "eid": "10mm",
        "fields": [
            {"PreviewTransform": {
                "reference": {"plugin": "Fallout4.esm", "object_id": "248AB9"},
            }},
            {"EquipmentType": {
                "reference": {"plugin": "Fallout4.esm", "object_id": "04334D"},
            }},
            {"MODL": "Weapons\\10mmPistol\\10mmRecieverDummy.nif"},
        ],
    }
    result = FormKeyMapper.rewrite_formkeys(record, mapping)
    fields = result["fields"]
    assert fields[0]["PreviewTransform"]["reference"] == {
        "plugin": "B21_Test.esp", "object_id": "000800",
    }
    assert fields[1]["EquipmentType"]["reference"] == {
        "plugin": "B21_Test.esp", "object_id": "000801",
    }
    assert fields[2]["MODL"] == "Weapons\\10mmPistol\\10mmRecieverDummy.nif"


# --- Real data round-trip ---------------------------------------------------

_FO4_ESM_YAML = Path(
    os.environ.get("FO4_ESM_YAML_DIR")
    or Path(__file__).resolve().parents[3] / "data" / "fo4_esm_yaml"
)
_RECORDS_DIR = _FO4_ESM_YAML / "Fallout4" / "records"


@pytest.mark.skipif(
    not _RECORDS_DIR.exists(),
    reason="canonical FO4 records DB not present",
)
def test_real_canonical_records_get_rewritten():
    """Load 3 paired canonical YAMLs, build a synthetic mappings dict that
    rewrites a few of their references, and assert the rewrites took effect.
    """
    paths = [
        _RECORDS_DIR / "WEAP" / "10mm - 004822_Fallout4.esm.yaml",
        _RECORDS_DIR / "AMMO" / "Ammo10mm - 01F276_Fallout4.esm.yaml",
    ]
    paths = [p for p in paths if p.exists()]
    if not paths:
        pytest.skip("expected canonical records not present")

    for path in paths:
        with open(path, encoding="utf-8") as f:
            record = yaml.safe_load(f)

        # Find at least one canonical ref to target.
        targets: list[tuple[str, str]] = []  # (plugin, object_id)
        def collect(node):
            if isinstance(node, dict):
                inner = node.get("reference")
                if (
                    isinstance(inner, dict)
                    and "plugin" in inner
                    and "object_id" in inner
                ):
                    targets.append((str(inner["plugin"]), str(inner["object_id"])))
                else:
                    for v in node.values():
                        collect(v)
            elif isinstance(node, list):
                for item in node:
                    collect(item)
        collect(record)

        assert targets, f"{path.name} expected to contain canonical refs"

        # Build a mapping for the first 2 refs found.
        mapping: dict[str, dict] = {}
        expected: list[tuple[str, str, str]] = []  # (src_fk, new_plugin, new_objid)
        for i, (plugin, obj_id) in enumerate(targets[:2]):
            src_fk = f"{obj_id}:{plugin}"
            new_objid = f"00080{i}"
            new_fk = f"{new_objid}:B21_RealTest.esp"
            mapping[src_fk] = {"new_formkey": new_fk}
            expected.append((src_fk, "B21_RealTest.esp", new_objid))

        rewritten = FormKeyMapper.rewrite_formkeys(record, mapping)

        # Walk the result and confirm each mapped src_fk no longer appears
        # under its source plugin and the new (plugin, object_id) is present.
        found_new: set[tuple[str, str]] = set()
        found_old: set[tuple[str, str]] = set()
        def collect_refs(node):
            if isinstance(node, dict):
                inner = node.get("reference")
                if (
                    isinstance(inner, dict)
                    and "plugin" in inner
                    and "object_id" in inner
                ):
                    found_new.add((str(inner["plugin"]), str(inner["object_id"])))
                else:
                    for v in node.values():
                        collect_refs(v)
            elif isinstance(node, list):
                for item in node:
                    collect_refs(item)
            elif isinstance(node, str):
                # Should not have produced bare-string forms.
                pass
        collect_refs(rewritten)

        for src_fk, new_plugin, new_objid in expected:
            obj_id, plugin = src_fk.split(":", 1)
            assert (new_plugin, new_objid) in found_new, (
                f"{path.name}: expected rewritten ref "
                f"({new_plugin}, {new_objid}) not found"
            )
            # The original (plugin, object_id) was rewritten — it should NOT
            # remain in the result *unless* another ref happened to share it
            # (e.g. duplicate). Assert at least one fewer occurrence.
            orig_count_before = sum(
                1 for p, o in targets if p == plugin and o == obj_id
            )
            orig_count_after = sum(
                1 for p, o in found_new if p == plugin and o == obj_id
            )
            assert orig_count_after < orig_count_before, (
                f"{path.name}: {src_fk} not rewritten "
                f"(before={orig_count_before}, after={orig_count_after})"
            )
