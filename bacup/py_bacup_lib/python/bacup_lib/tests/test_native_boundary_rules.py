from __future__ import annotations

from pathlib import Path

from bacup_lib.native_runtime import load_native_module


def test_conversion_native_boundary_has_no_python_dicts() -> None:
    repo = Path(__file__).resolve().parents[5]
    src_dir = repo / "bacup" / "py_bacup_lib" / "native" / "conversion" / "src"
    forbidden = (
        "PyDict",
        "downcast::<PyDict>",
        "plugin_handle_read_record_authoring_value_native",
        "conversion_read_record",
        "conversion_add_record",
        "conversion_run_drain_records",
        "conversion_run_drain_translated_records",
        'name = "conversion_run_fnv_legacy_scripting"',
        "record_from_pyraw",
        "field_value_from_pyraw",
    )
    violations: list[str] = []
    for path in src_dir.rglob("*.rs"):
        text = path.read_text(encoding="utf-8")
        for token in forbidden:
            if token in text:
                violations.append(f"{path.relative_to(repo)} contains {token}")

    assert not violations, "\n".join(violations)


def test_conversion_native_boundary_does_not_export_legacy_record_apis() -> None:
    module = load_native_module()
    raw_module = module._raw
    forbidden = (
        "conversion_read_record",
        "conversion_add_record",
        "conversion_run_drain_records",
        "conversion_run_drain_translated_records",
        "conversion_run_fnv_legacy_scripting",
    )

    for name in forbidden:
        assert not hasattr(module, name), name
        assert not hasattr(raw_module, name), name


def test_conversion_python_boundary_has_no_legacy_record_mutation_helpers() -> None:
    repo = Path(__file__).resolve().parents[5]
    src_dir = repo / "bacup" / "py_bacup_lib" / "python" / "bacup_lib"
    forbidden = (
        "_apply_rust_vmad_intents",
        "_attach_vmad_to_record",
        "_ScriptBindingIntent",
        "_phase_convert_nifs_python_fallback",
        "_wire_weapon_anim_data",
        "def phase_havok",
        "def phase_synthesize_drivers(",
        "_translated_records",
        "translated_records",
        "phase_havok as _py_phase",
        "phase_synthesize_drivers as _py_phase",
        "prepare_fnv_legacy_record_payload",
        "merge_fnv_legacy_record_payloads",
        "normalize_fnv_legacy_translated_records",
        "iter_fnv_legacy_vmad_targets",
        "translated_record_payloads",
        "fallback to the Python phase",
        "Python conversion loop",
        "record_as_authoring_dict",
        "records_by_signature_v2",
        "record_by_form_key",
        "add_authoring_record",
        "plugin_handle_record_as_authoring_dict",
        "plugin_handle_records_by_signature_v2",
        "plugin_handle_add_authoring_record",
        "NativeRecordLookup",
        "authoring_dict(",
        "get_record_by_object_id",
        "get_records_by_signature",
        "record_by_editor_id_and_signature",
        "get_referenced_form_keys",
        "get_referencing_form_keys",
        "_rust_handle",
        "rust_target_handle_id",
        "_BorrowedNativeHandle",
        "_snapshot_land_cache_from_handle",
    )
    violations: list[str] = []
    for path in src_dir.rglob("*.py"):
        if "tests" in path.parts:
            continue
        text = path.read_text(encoding="utf-8")
        for token in forbidden:
            if token in text:
                violations.append(f"{path.relative_to(repo)} contains {token}")

    assert not violations, "\n".join(violations)


def test_task15_path_boundaries_do_not_feed_creation_handles_to_bacup() -> None:
    repo = Path(__file__).resolve().parents[5]
    package = repo / "bacup" / "py_bacup_lib" / "python" / "bacup_lib"
    fixed_sites = (
        package / "run" / "run_handle.py",
        package / "formkey" / "formkey_mapper.py",
        package / "target_preflight.py",
    )

    violations = [
        str(path.relative_to(repo))
        for path in fixed_sites
        if "native_handle_id" in path.read_text(encoding="utf-8")
    ]
    regen_source = (package / "regen_pipeline.py").read_text(encoding="utf-8")
    land_cache_helper = regen_source[
        regen_source.index("def _snapshot_land_cache_from_run") : regen_source.index(
            "def _write_land_cache_marker"
        )
    ]
    if "native_handle_id" in land_cache_helper or "plugin_handle_call" in land_cache_helper:
        violations.append("regen_pipeline.py land-cache helper")

    assert not violations, "creation handle leaked into BACUP boundary:\n" + "\n".join(
        violations
    )
