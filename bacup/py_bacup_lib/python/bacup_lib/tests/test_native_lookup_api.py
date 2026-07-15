from __future__ import annotations

import pytest

from creation_lib.esp.plugin import Plugin


@pytest.fixture
def native_lookup_plugin() -> Plugin:
    plugin = Plugin.new("NativeLookup.esp", game="fo4")

    misc = plugin.new_record("MISC")
    misc.editor_id = "SharedEditor"
    plugin.add_record(misc)

    kywd = plugin.new_record("KYWD")
    kywd.editor_id = "SharedEditor"
    plugin.add_record(kywd)

    source = plugin.new_record("OMOD")
    source.editor_id = "RefSource"
    source.add_subrecord("YNAM", semantic_type="formid").set_form_ref(misc.form_id)
    source.add_subrecord("ZNAM", semantic_type="formid").set_form_ref(kywd.form_id)
    plugin.add_record(source)

    try:
        yield plugin
    finally:
        plugin.close()


def test_get_referenced_form_keys_by_subrecord_filters_by_signature(
    native_lookup_plugin: Plugin,
) -> None:
    source_form_key = f"{native_lookup_plugin.plugin_name}:000802"

    assert native_lookup_plugin.get_referenced_form_keys_by_subrecord(
        source_form_key,
        "YNAM",
    ) == [f"{native_lookup_plugin.plugin_name}:000800"]
    assert native_lookup_plugin.get_referenced_form_keys_by_subrecord(
        source_form_key,
        "ZNAM",
    ) == [f"{native_lookup_plugin.plugin_name}:000801"]
