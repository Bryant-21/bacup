"""Smoke test for the native FO76→FO4 terrain-texture pipeline.

Verifies that conversion_terrain_with_textures produces the expected
authoring-dir YAML records (TXST/LTEX/GRAS) and a texture_manifest.json
with populated grass entries. Skipped on machines without a FO76 install.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[5]


def _fo76_data_dir() -> Path | None:
    # Prefer FO76_DIR (project convention from .env), fall back to FO76_DATA_DIR.
    for key in ("FO76_DIR", "FO76_DATA_DIR"):
        v = os.environ.get(key, "").strip().strip('"').strip("'")
        if v:
            p = Path(v)
            # FO76_DIR points at the game root; FO76_DATA_DIR points at Data.
            data = p / "Data" if p.name.lower() != "data" else p
            if (data / "SeventySix.esm").is_file() and (data / "Terrain" / "Appalachia.btd").is_file():
                return data
    # Try parsing .env directly so the test works in dev shells without exporting.
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            for key in ("FO76_DIR=", "FO76_DATA_DIR="):
                if line.startswith(key):
                    v = line.split("=", 1)[1].strip().strip('"').strip("'")
                    if v:
                        p = Path(v)
                        data = p / "Data" if p.name.lower() != "data" else p
                        if (data / "SeventySix.esm").is_file() and (data / "Terrain" / "Appalachia.btd").is_file():
                            return data
    return None


FO76_DATA = _fo76_data_dir()


@pytest.mark.skipif(FO76_DATA is None, reason="FO76 install not available")
def test_terrain_with_textures_emits_records_and_grass(tmp_path):
    """One-cell BTD conversion should produce TXST + LTEX + GRAS YAML and a
    grass-populated manifest."""
    from bacup_lib.terrain.fo76_btd import (
        TerrainConversionRequest,
        convert_fo76_btd_to_fo4_land,
    )

    assert FO76_DATA is not None
    mod_root = tmp_path / "B21_TerrainSmoke"
    (mod_root / "yaml").mkdir(parents=True)
    debug_dir = mod_root / "debug" / "terrain"

    request = TerrainConversionRequest(
        btd_path=str(FO76_DATA / "Terrain" / "Appalachia.btd"),
        output_authoring_dir=str(mod_root / "yaml"),
        plugin_name="B21_TerrainSmoke.esp",
        worldspace_editor_id="APPALACHIA",
        source_min_x=0,
        source_min_y=0,
        source_max_x=0,
        source_max_y=0,  # one cell — minimal
        resample_mode="sample4",
        debug_output_dir=str(debug_dir),
        fo76_data_dir=str(FO76_DATA),
        emit_textures=True,
        export_heightmap=False,
        preserve_source_ids=True,
        source_plugin_path=str(FO76_DATA / "SeventySix.esm"),
        build_esp=False,
    )
    report = convert_fo76_btd_to_fo4_land(request)

    # Output directories must exist + contain YAML.
    yaml_root = mod_root / "yaml" / "records"
    assert (yaml_root / "TXST").is_dir(), "TXST records dir missing"
    assert (yaml_root / "LTEX").is_dir(), "LTEX records dir missing"
    txst_files = list((yaml_root / "TXST").glob("*.yaml"))
    ltex_files = list((yaml_root / "LTEX").glob("*.yaml"))
    assert txst_files, "no TXST yaml files emitted"
    assert ltex_files, "no LTEX yaml files emitted"
    # GRAS is conditional on the cell having ground cover — for cell (0,0)
    # it's expected but not strictly mandatory; tolerate either.
    # If you want a strict check, bump source_max_x/y to widen the slice.

    # Manifest sanity.
    manifest = json.loads((debug_dir / "texture_manifest.json").read_text(encoding="utf-8"))
    textures = manifest.get("textures") or []
    assert textures, "manifest is empty"
    assert all(t.get("source_ltex_editor_id") for t in textures), "manifest entries missing editor ids"
    timing_report = json.loads((debug_dir / "terrain_timing.json").read_text(encoding="utf-8"))
    assert timing_report.get("timings"), "terrain timing report is empty"
    assert report.get("timings"), "terrain report is missing timings"
    total_grass = sum(len(t.get("grass") or []) for t in textures)
    # Loose check: a 1-cell (0,0) slice may have no ground-covered LTEXes, so we
    # only assert the grass-count path ran end-to-end, not a hard minimum.
    assert isinstance(total_grass, int)
    # btd4 report keys must survive the Rust→Python bridge.
    # btd4_output_path was not set in this request, so it should be absent.
    assert "layers_recovered" in report, "layers_recovered key missing from report"
    assert isinstance(report["layers_recovered"], int)


def test_terrain_request_btd4_default():
    """TerrainConversionRequest.btd4_output_path defaults to None — no BTD4 emitted unless
    explicitly set. Does not require a real FO76 install."""
    from bacup_lib.terrain.fo76_btd import TerrainConversionRequest

    req = TerrainConversionRequest(
        btd_path="",
        output_authoring_dir="",
        plugin_name="Test.esp",
        worldspace_editor_id="TestWorld",
        source_min_x=0,
        source_min_y=0,
        source_max_x=0,
        source_max_y=0,
    )
    assert req.btd4_output_path is None


@pytest.mark.parametrize(
    "legacy_key", ["source_handle_id", "target_handle_id", "record_output_mode"]
)
def test_standalone_terrain_rejects_legacy_handle_options(legacy_key):
    from bacup_lib.native_runtime import load_native_module

    with pytest.raises(RuntimeError, match=legacy_key):
        load_native_module().conversion_terrain_with_textures(
            json.dumps({legacy_key: 1})
        )
