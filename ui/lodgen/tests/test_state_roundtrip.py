from __future__ import annotations

import json

from ui.lodgen.state import LodgenState, apply_preset, collect_preset, settings_to_json
from creation_lib.lod.default_settings import fo4_default_settings


def test_default_state_settings_are_fo4_defaults():
    st = LodgenState()
    assert st.settings == fo4_default_settings()


def test_settings_to_json_roundtrips():
    st = LodgenState()
    assert json.loads(settings_to_json(st)) == fo4_default_settings()


def test_preset_collect_then_apply_is_identity():
    st = LodgenState()
    st.game = "fo4"
    st.worldspace = "DLC03FarHarbor"
    st.output_dir = "C:/out"
    st.settings["terrain"]["skirts"] = 128
    preset = collect_preset(st)
    assert preset["worldspace"] == "DLC03FarHarbor"
    assert preset["settings"]["terrain"]["skirts"] == 128

    st2 = LodgenState()
    apply_preset(st2, preset)
    assert st2.worldspace == "DLC03FarHarbor"
    assert st2.output_dir == "C:/out"
    assert st2.settings["terrain"]["skirts"] == 128
