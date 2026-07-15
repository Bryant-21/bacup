"""Boundary contract for the removed authoring-dict batch API."""

from __future__ import annotations

import json

from creation_lib.esp import native_runtime


def test_authoring_dict_batch_api_is_not_exported() -> None:
    module = native_runtime.load_native_module()
    assert not hasattr(module, "plugin_handle_record_as_authoring_dict")
    assert not hasattr(module, "plugin_handle_records_as_authoring_dicts_batch")


def test_record_text_export_replaces_per_record_authoring_dict_view() -> None:
    handle = native_runtime.plugin_handle_import_text(
        json.dumps(
            {
                "plugin": "TextExport.esp",
                "game": "fo4",
                "header": {"version": 1.0, "next_object_id": "000801"},
                "items": [
                    {
                        "type": "group",
                        "label_text": "KYWD",
                        "group_type": 0,
                        "children": [
                            {
                                "signature": "KYWD",
                                "form_id": "000800",
                                "subrecords": [
                                    {
                                        "signature": "EDID",
                                        "data_hex": "4232315F546578744578706F727400",
                                    }
                                ],
                            }
                        ],
                    }
                ],
            }
        ),
        "json",
        "fo4",
    )

    exported = native_runtime.plugin_handle_call(handle, "export_record_text", 0x000800, "json")
    payload = json.loads(exported)
    assert payload["signature"] == "KYWD"
    assert payload["eid"] == "B21_TextExport"
