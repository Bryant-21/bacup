"""Tests for RecordComparator."""
from __future__ import annotations

import pytest


def test_identical_records():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"Name": "Gun", "Value": 100, "Keywords": ["A", "B"]}
    b = {"Name": "Gun", "Value": 100, "Keywords": ["A", "B"]}
    result = RecordComparator.compare(a, b)
    assert result.matched == {"Name", "Value", "Keywords"}
    assert result.mismatched == {}
    assert result.extra == set()
    assert result.missing == set()


def test_mismatched_field():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"Name": "Gun", "Value": 100}
    b = {"Name": "Gun", "Value": 200}
    result = RecordComparator.compare(a, b)
    assert result.matched == {"Name"}
    assert "Value" in result.mismatched
    assert result.mismatched["Value"] == (100, 200)


def test_extra_and_missing_fields():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"Name": "Gun", "Extra": 42}
    b = {"Name": "Gun", "Missing": 99}
    result = RecordComparator.compare(a, b)
    assert result.matched == {"Name"}
    assert result.extra == {"Extra"}  # in actual but not expected
    assert result.missing == {"Missing"}  # in expected but not actual


def test_nested_dict_compare():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"Model": {"File": "gun.nif", "Data": "0x04"}}
    b = {"Model": {"File": "gun.nif", "Data": "0x04"}}
    result = RecordComparator.compare(a, b)
    assert result.matched == {"Model"}
    assert result.mismatched == {}


def test_nested_dict_mismatch():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"Model": {"File": "gun.nif", "Data": "0x04"}}
    b = {"Model": {"File": "gun.nif", "Data": "0x08"}}
    result = RecordComparator.compare(a, b)
    assert "Model" in result.mismatched


def test_ignore_fields():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"Name": "Gun", "FormKey": "000001:Mod.esp", "EditorID": "MyGun"}
    b = {"Name": "Gun", "FormKey": "000099:Other.esp", "EditorID": "OtherGun"}
    result = RecordComparator.compare(a, b, ignore={"FormKey", "EditorID"})
    assert result.matched == {"Name"}
    assert result.extra == set()
    assert result.missing == set()


def test_match_ratio():
    from bacup_lib.tests.record_comparator import RecordComparator

    a = {"A": 1, "B": 2, "C": 3, "D": 4}
    b = {"A": 1, "B": 2, "C": 99, "D": 99}
    result = RecordComparator.compare(a, b)
    assert result.match_ratio() == 0.5  # 2 of 4 matched
