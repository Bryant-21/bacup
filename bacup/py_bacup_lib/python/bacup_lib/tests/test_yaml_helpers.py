"""Tests for the public YAML helper module."""

from bacup_lib import yaml_helpers


def test_field_at_simple_lookup():
    record = {"fields": [{"EDID": "TestEDID"}]}
    assert yaml_helpers.field_at(record, "EDID") == "TestEDID"


def test_field_all_returns_all_matching_entries():
    record = {"fields": [{"KWDA": "A"}, {"EDID": "TestEDID"}, {"KWDA": "B"}]}
    assert yaml_helpers.field_all(record, "KWDA") == ["A", "B"]


def test_from_ref_roundtrip():
    ref = yaml_helpers.to_ref("000123:Fallout4.esm")
    assert yaml_helpers.from_ref(ref) == "000123:Fallout4.esm"
