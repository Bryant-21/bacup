from __future__ import annotations

import json

from bacup_lib.tests import helpers
from creation_lib.esp.native_runtime import (
    plugin_handle_call,
    plugin_handle_group_record_summaries,
    plugin_handle_import_text,
    plugin_handle_record_summary,
)


def _fixture_handle() -> int:
    payload = {
        "plugin": "Test.esp",
        "game": "fo4",
        "header": {"version": 1.0, "next_object_id": "000830"},
        "items": [
            {
                "type": "group",
                "label_text": "KYWD",
                "group_type": 0,
                "children": [
                    {
                        "signature": "KYWD",
                        "form_id": "000810",
                        "subrecords": [
                            {"signature": "EDID", "data_hex": "546573744B5700"}
                        ],
                    }
                ],
            },
            {
                "type": "group",
                "label_text": "WEAP",
                "group_type": 0,
                "children": [
                    {
                        "signature": "WEAP",
                        "form_id": "000820",
                        "subrecords": [
                            {"signature": "EDID", "data_hex": "546573745765617000"},
                            {"signature": "FULL", "data_hex": "5465737400"},
                        ],
                    }
                ],
            },
        ],
    }
    return int(plugin_handle_import_text(json.dumps(payload), "json", "fo4"))


def test_test_helpers_do_not_build_plugins_from_authoring_dicts() -> None:
    assert not hasattr(helpers, "make_test_plugin_with_records")


def test_text_import_replaces_native_plugin_record_dict_helper() -> None:
    handle = _fixture_handle()

    keyword = plugin_handle_record_summary(handle, 0x000810)
    weapon = plugin_handle_record_summary(handle, 0x000820)
    assert keyword is not None
    assert keyword.signature == "KYWD"
    assert keyword.editor_id == "TestKW"
    assert weapon is not None
    assert weapon.signature == "WEAP"
    assert weapon.editor_id == "TestWeap"

    kywd_records = plugin_handle_group_record_summaries(handle, "KYWD")
    assert [record.editor_id for record in kywd_records] == ["TestKW"]

    record_text = plugin_handle_call(handle, "export_record_text", 0x000820, "json")
    payload = json.loads(record_text)
    assert payload["eid"] == "TestWeap"
