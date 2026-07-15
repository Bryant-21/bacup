"""Tests for --include-interior / --carry-interior-previs CLI+Python plumbing.

Verifies that:
1. `PluginPortOptions` exposes the two new bool fields.
2. The unified workflow calls run_phase("convert_interior_cells", ...) when
   include_interior=True (the default), and skips it when opted out.
3. carry_interior_previs is forwarded as params["carry_previs"].

These tests do NOT hit the native dispatcher — run_phase is mocked on the
stub rust_run object attached to the conversion context.
"""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.models import (
    ConversionSummary,
    PluginPortOptions,
    PluginPortRequest,
)
from bacup_lib.workflows.unified import TrackSignals, UnifiedDriver


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class StubRunner:
    def __init__(self) -> None:
        self.logs: list[tuple[str, str]] = []

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))

    def is_cancelled(self) -> bool:
        return False

    def emit_complete(self, output_root, summary) -> None:
        pass


class RecordingRustRun:
    """Minimal stub for ctx._rust_conversion_run that records run_phase calls."""

    def __init__(self) -> None:
        self.phase_calls: list[dict] = []
        self.id = 0

    def run_phase(self, name: str, *, mod_path: str = "", params: dict | None = None) -> dict:
        self.phase_calls.append({"phase": name, "mod_path": mod_path, "params": params or {}})
        return {"records_added": 0, "records_dropped": 0, "warnings": 0}

    def drain_warnings(self) -> list:
        return []

    def release_remap_state(self) -> None:
        pass

    def release_master_handles(self) -> int:
        return 0

    def release_source_handle(self) -> bool:
        return False

    def drain_events(self, limit: int) -> list:
        return []


def _make_request(tmp_path: Path, *, include_interior: bool = False,
                  carry_interior_previs: bool = False) -> PluginPortRequest:
    src = tmp_path / "SeventySix.esm"
    src.write_bytes(b"not a real plugin")
    return PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[src],
        output_root=tmp_path / "out",
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(
            translate_records=True,
            convert_terrain=True,
            build_esp=True,
            convert_scripts=True,
            convert_nifs=False,
            convert_btos=False,
            convert_textures=False,
            convert_materials=False,
            convert_havok=False,
            synthesize_drivers=False,
            convert_animations=False,
            copy_sounds=False,
            validate_output=False,
            include_interior=include_interior,
            carry_interior_previs=carry_interior_previs,
        ),
    )


def _stub_record_runtime(driver: UnifiedDriver, rust_run: RecordingRustRun,
                         monkeypatch, recorded: list) -> None:
    """Patch the record runtime so it runs minimally but exercises the phase
    sequencing code, with a stub rust_run that records run_phase calls."""
    runtime = driver.record_runtime
    # Stub driver-level terrain helpers (they live on UnifiedDriver, not the runtime).
    monkeypatch.setattr(
        driver, "_harvest_terrain_products", lambda mod_path, runner: None
    )
    monkeypatch.setattr(driver, "_mark_terrain_done", lambda: None)

    def record_phase(phase_no, label, body, runner, timing_ctx=None, raise_on_error=False):
        # Execute the body so that run_phase calls are actually made.
        recorded.append(("phase", label))
        progress = SimpleNamespace(total_items=0, completed_items=0)
        try:
            body(progress)
        except Exception:
            pass  # ignore errors from other phases; we only care about run_phase recording

    def make_ctx(source_plugin, plugin_name, mod_path, runner=None):
        return SimpleNamespace(
            mod_path=mod_path,
            output_plugin_name=plugin_name,
            source_game="fo76",
            target_game="fo4",
            is_whole_plugin=True,
            target_record_preflight_missing_masters=[],
            target_record_preflight_warnings=[],
            target_asset_index=None,
            summary=ConversionSummary(mod_path=str(mod_path)),
            addon_index_map={},
            _rust_conversion_run=rust_run,
            _source_closed=False,
        )

    monkeypatch.setattr(runtime, "_run_phase", record_phase)
    monkeypatch.setattr(runtime, "_topo_sort", lambda plugins, runner: list(plugins))
    monkeypatch.setattr(runtime, "_build_context", make_ctx)
    monkeypatch.setattr(
        runtime, "_clean_stale_authoring_for_direct_esp", lambda mod_path: None
    )
    monkeypatch.setattr(
        runtime, "_collect_assets_native", lambda sp, ctx, runner: []
    )
    monkeypatch.setattr(runtime, "_apply_registry_mappings", lambda ctx: None)
    monkeypatch.setattr(
        runtime, "_run_optional_fnv_legacy_phase", lambda ctx, sp, runner: False
    )
    monkeypatch.setattr(runtime, "_run_convert_creatures_phase", lambda ctx, runner: None)
    monkeypatch.setattr(runtime, "_run_convert_equipment_phase", lambda ctx, runner: None)
    monkeypatch.setattr(runtime, "_close_source_handle", lambda ctx: None)
    monkeypatch.setattr(runtime, "_close_target_master_handles", lambda ctx: None)
    monkeypatch.setattr(runtime, "_emit_authoring_yaml_for_build", lambda ctx, runner: False)
    monkeypatch.setattr(
        runtime, "_patch_projected_worldspace_subrecords", lambda ctx, runner, sp: None
    )
    monkeypatch.setattr(runtime, "_drain_and_drop_rust_run", lambda ctx: None)
    monkeypatch.setattr(runtime, "_update_registry", lambda ctx: None)
    monkeypatch.setattr(runtime, "_merge_summary", lambda summary: None)
    monkeypatch.setattr(runtime, "_merge_run_result", lambda ctx: None)
    # Stub out pipeline.convert_terrain so it doesn't try to load files.
    monkeypatch.setattr(
        "bacup_lib.workflows.unified.pipeline.convert_terrain",
        lambda ctx, runner, p: None,
    )


# ---------------------------------------------------------------------------
# Unit tests: PluginPortOptions fields
# ---------------------------------------------------------------------------


def test_plugin_port_options_include_interior_defaults_true():
    opts = PluginPortOptions()
    assert opts.include_interior is True


def test_plugin_port_options_carry_interior_previs_defaults_false():
    opts = PluginPortOptions()
    assert opts.carry_interior_previs is False


def test_plugin_port_options_accepts_include_interior():
    opts = PluginPortOptions(include_interior=True)
    assert opts.include_interior is True


def test_plugin_port_options_accepts_carry_interior_previs():
    opts = PluginPortOptions(include_interior=True, carry_interior_previs=True)
    assert opts.carry_interior_previs is True


# ---------------------------------------------------------------------------
# Integration tests: unified workflow phase sequencing
# ---------------------------------------------------------------------------


def test_unified_runs_interior_phase_when_flag_set(tmp_path, monkeypatch):
    """run_phase("convert_interior_cells") is called when include_interior=True."""
    rust_run = RecordingRustRun()
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(
        _make_request(tmp_path, include_interior=True),
        sink_id=None,
        signals=signals,
    )
    _stub_record_runtime(driver, rust_run, monkeypatch, recorded)

    driver.run_record_track(StubRunner())

    phase_names = [c["phase"] for c in rust_run.phase_calls]
    assert "convert_interior_cells" in phase_names


def test_unified_skips_interior_phase_when_opted_out(tmp_path, monkeypatch):
    """run_phase("convert_interior_cells") is NOT called when include_interior=False (opt-out)."""
    rust_run = RecordingRustRun()
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(
        _make_request(tmp_path, include_interior=False),
        sink_id=None,
        signals=signals,
    )
    _stub_record_runtime(driver, rust_run, monkeypatch, recorded)

    driver.run_record_track(StubRunner())

    phase_names = [c["phase"] for c in rust_run.phase_calls]
    assert "convert_interior_cells" not in phase_names


def test_unified_forwards_carry_previs_param(tmp_path, monkeypatch):
    """carry_interior_previs=True is forwarded as params["carry_previs"]=True."""
    rust_run = RecordingRustRun()
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(
        _make_request(tmp_path, include_interior=True, carry_interior_previs=True),
        sink_id=None,
        signals=signals,
    )
    _stub_record_runtime(driver, rust_run, monkeypatch, recorded)

    driver.run_record_track(StubRunner())

    interior_calls = [c for c in rust_run.phase_calls if c["phase"] == "convert_interior_cells"]
    assert interior_calls, "expected at least one convert_interior_cells call"
    assert interior_calls[0]["params"].get("carry_previs") is True


def test_unified_interior_phase_carry_previs_default_false(tmp_path, monkeypatch):
    """carry_interior_previs defaults to False → params["carry_previs"]=False."""
    rust_run = RecordingRustRun()
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(
        _make_request(tmp_path, include_interior=True, carry_interior_previs=False),
        sink_id=None,
        signals=signals,
    )
    _stub_record_runtime(driver, rust_run, monkeypatch, recorded)

    driver.run_record_track(StubRunner())

    interior_calls = [c for c in rust_run.phase_calls if c["phase"] == "convert_interior_cells"]
    assert interior_calls, "expected at least one convert_interior_cells call"
    assert interior_calls[0]["params"].get("carry_previs") is False


def test_unified_interior_phase_ordering(tmp_path, monkeypatch):
    """convert_interior_cells fires after emit_projected_navmeshes and
    before rebuild_projected_navi in the phase label sequence."""
    rust_run = RecordingRustRun()
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(
        _make_request(tmp_path, include_interior=True),
        sink_id=None,
        signals=signals,
    )
    _stub_record_runtime(driver, rust_run, monkeypatch, recorded)

    driver.run_record_track(StubRunner())

    labels = [label for (_, label) in recorded if _ == "phase"]
    assert "Emit Projected NavMeshes" in labels
    assert "Convert Interior Cells" in labels
    assert "Rebuild Projected NAVI" in labels

    nav_idx = labels.index("Emit Projected NavMeshes")
    interior_idx = labels.index("Convert Interior Cells")
    navi_idx = labels.index("Rebuild Projected NAVI")
    assert nav_idx < interior_idx < navi_idx, (
        f"Expected NavMeshes({nav_idx}) < Interior({interior_idx}) < NAVI({navi_idx})"
    )


def test_unified_sync_cell_locations_before_encounter_zones(tmp_path, monkeypatch):
    """Cell-location sync must run BEFORE encounter-zone synthesis.

    The ECZN interior pull-model links interior cells to their Location via the
    cell's XLCN, but that XLCN is written by the cell-location sync (derived from
    LCTN ref-arrays). If synthesis runs first, every interior cell still has no
    XLCN, so interior-only locations get no ECZN and the cell's XEZN is left
    dangling (regression: WhitespringMall01 pointed XEZN at a terrain CELL)."""
    rust_run = RecordingRustRun()
    recorded: list = []
    signals = TrackSignals()
    driver = UnifiedDriver(
        _make_request(tmp_path, include_interior=True),
        sink_id=None,
        signals=signals,
    )
    _stub_record_runtime(driver, rust_run, monkeypatch, recorded)

    driver.run_record_track(StubRunner())

    labels = [label for (_, label) in recorded if _ == "phase"]
    assert "Sync Projected Cell Locations" in labels
    assert "Synthesize Encounter Zones" in labels

    sync_idx = labels.index("Sync Projected Cell Locations")
    eczn_idx = labels.index("Synthesize Encounter Zones")
    assert sync_idx < eczn_idx, (
        f"Expected Sync Cell Locations({sync_idx}) < "
        f"Synthesize Encounter Zones({eczn_idx})"
    )
