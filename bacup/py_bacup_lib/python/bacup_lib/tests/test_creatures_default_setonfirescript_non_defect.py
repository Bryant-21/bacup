from __future__ import annotations

from bacup_lib.workflows.unified import _script_patch_source


def test_set_on_fire_script_remains_an_unpatched_memberless_carrier():
    assert _script_patch_source("Creatures:_Default:SetOnFireScript") is None
