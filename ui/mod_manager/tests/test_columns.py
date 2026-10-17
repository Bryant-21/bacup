import math

import pytest

from creation_lib.mod_manager import columns
from creation_lib.mod_manager.paths import ManagerError


def test_default_geometry_keeps_name_and_status_and_hides_optional_annotations():
    result = columns.layout(400, ["status", "version", "category", "nexus"])
    assert [item.key for item in result] == ["enabled", "priority", "name", "status"]
    assert result[2].width >= 160 and sum(item.width for item in result) == 400
    assert all(b.offset == a.offset + a.width for a, b in zip(result, result[1:]))


def test_explicit_geometry_scrolls_and_persists_order_instead_of_losing_requested_fields():
    config = columns.moved(columns.resized(None, "name", 500), "status", "name")
    result = columns.layout(400, ["status", "notes", "version"], config)
    assert [item.key for item in result] == ["enabled", "priority", "status", "name", "version", "notes"]
    assert next(item.width for item in result if item.key == "name") == 500
    assert sum(item.width for item in result) > 400
    assert columns.normalize(config) == config
    assert columns.moved(config, "status", "status") == config


def test_old_or_malformed_preferences_keep_every_required_column():
    value = columns.normalize({"order": ["notes", "name", "notes", "unknown"], "widths": {"name": math.nan, "enabled": True, "notes": 90000}})
    assert value["order"][:2] == ["notes", "name"] and set(value["order"]) == set(columns.ORDER)
    assert value["widths"] == {"notes": columns.MAXIMUMS["notes"]}
    assert columns.normalize("old preference") == columns.normalize()


@pytest.mark.parametrize("key,width", [("unknown", 100), ("name", math.inf), ("name", "100"), ("name", True)])
def test_invalid_gesture_cannot_corrupt_settings(key, width):
    with pytest.raises(ManagerError):
        columns.resized(None, key, width)


def test_width_limits_and_unknown_drop_target():
    assert columns.resized(None, "name", -1)["widths"]["name"] == 120
    with pytest.raises(ManagerError):
        columns.moved(None, "name", "unknown")
