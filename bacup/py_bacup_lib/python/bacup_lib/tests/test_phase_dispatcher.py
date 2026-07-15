"""Phase dispatcher smoke test."""
from __future__ import annotations

import pytest

from bacup_lib.native_runtime import load_native_module


def _make_run(m, tmp_path):
    return m.conversion_run_create_from_paths(
        "fo76", "fo4", None, "Output.esp", None, [], None,
        {"output_plugin_name": "Output.esp", "mod_path": str(tmp_path)},
    )


def test_list_phases_contains_standard_phases_and_stubs() -> None:
    m = load_native_module()
    names = m.conversion_run_list_phases()
    assert "translate" in names
    assert "fixups" not in names
    assert "fixups_v2" in names
    assert "fnv_legacy" in names
    assert "apply_registry_mappings" in names
    for old_asset_phase in ["convert_textures", "convert_nifs", "convert_materials"]:
        assert old_asset_phase not in names
    for phase in ["convert_textures_v2", "convert_nifs_v2", "convert_materials_v2"]:
        assert phase in names
    # stubs and standard phases (must already be registered)
    for stub in ["walk", "convert_havok",
                 "convert_face", "convert_equipment", "build_esp"]:
        assert stub in names


def test_unknown_phase_raises(tmp_path) -> None:
    """Calling an unregistered phase name should fail with a clear message."""
    m = load_native_module()
    run_id = _make_run(m, tmp_path)
    try:
        with pytest.raises(ValueError, match="unknown phase"):
            m.conversion_run_phase(run_id, "__not_a_real_phase__", {
                "mod_path": str(tmp_path),
                "source_extracted_dir": str(tmp_path),
                "params": {},
            })
    finally:
        m.conversion_run_drop(run_id)


def test_drain_events_non_blocking(tmp_path) -> None:
    """drain_events returns [] immediately when no events are queued."""
    m = load_native_module()
    run_id = _make_run(m, tmp_path)
    try:
        events = m.conversion_run_drain_events(run_id, 256)
        assert events == []
    finally:
        m.conversion_run_drop(run_id)


def test_cancel_sets_flag(tmp_path) -> None:
    """conversion_run_cancel should not raise and is idempotent."""
    m = load_native_module()
    run_id = _make_run(m, tmp_path)
    try:
        m.conversion_run_cancel(run_id)
        m.conversion_run_cancel(run_id)  # idempotent — second call must not raise
    finally:
        m.conversion_run_drop(run_id)
