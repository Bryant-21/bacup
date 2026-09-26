"""The native FO76->FO4 terrain-texture entry point rejects legacy handle options."""
from __future__ import annotations

import json

import pytest


@pytest.mark.parametrize(
    "legacy_key", ["source_handle_id", "target_handle_id", "record_output_mode"]
)
def test_standalone_terrain_rejects_legacy_handle_options(legacy_key):
    from bacup_lib.native_runtime import load_native_module

    with pytest.raises(RuntimeError, match=legacy_key):
        load_native_module().conversion_terrain_with_textures(
            json.dumps({legacy_key: 1})
        )
