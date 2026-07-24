from pathlib import Path

from bacup_lib.native_runtime import load_native_module
from creation_lib.esp.native_runtime import (
    plugin_handle_add_record_raw,
    plugin_handle_call,
    plugin_handle_close,
    plugin_handle_load,
    plugin_handle_new,
    plugin_handle_record_subrecords,
)


def _write_plugin(path: Path, game: str, records: list[tuple[str, int, str]]) -> None:
    handle = plugin_handle_new(path.name, game)
    try:
        for signature, form_id, editor_id in records:
            plugin_handle_add_record_raw(
                handle,
                signature,
                form_id,
                0,
                0,
                None,
                None,
                [("EDID", editor_id.encode("cp1252") + b"\0", None)],
            )
        plugin_handle_call(handle, "save", str(path))
    finally:
        plugin_handle_close(handle)


def _write_localized_plugin(
    path: Path,
    records: list[tuple[int, str, int, str]],
    *,
    masters: tuple[str, ...] = (),
) -> None:
    handle = plugin_handle_new(path.name, "skyrimse")
    try:
        for master in masters:
            plugin_handle_call(handle, "add_master", master)
        plugin_handle_call(handle, "set_is_localized", True)
        strings = {string_id: text for _, _, string_id, text in records}
        plugin_handle_call(
            handle,
            "set_localized_strings_by_language",
            {"en": strings},
            "en",
            {string_id: "strings" for string_id in strings},
        )
        for form_id, editor_id, string_id, _ in records:
            plugin_handle_add_record_raw(
                handle,
                "FACT",
                form_id,
                0,
                44,
                None,
                None,
                [
                    ("EDID", editor_id.encode("cp1252") + b"\0", None),
                    ("FULL", string_id.to_bytes(4, "little"), "localized_string"),
                ],
            )
        plugin_handle_call(handle, "save", str(path))
    finally:
        plugin_handle_close(handle)


def _write_nonlocalized_named_plugin(path: Path, name: str) -> None:
    handle = plugin_handle_new(path.name, "fnv")
    try:
        plugin_handle_add_record_raw(
            handle,
            "MISC",
            0x3407B,
            0,
            15,
            None,
            None,
            [
                ("EDID", b"DrinkingGlass01\0", None),
                ("FULL", name.encode("cp1252") + b"\0", None),
            ],
        )
        plugin_handle_call(handle, "save", str(path))
    finally:
        plugin_handle_close(handle)


def _resolved_full_name(handle: int, form_id: int) -> str | None:
    subrecords = plugin_handle_record_subrecords(handle, form_id) or []
    full = next(data for signature, data, _ in subrecords if signature == "FULL")
    return plugin_handle_call(
        handle,
        "resolve_string",
        int.from_bytes(full, "little"),
        "en",
    )


def test_merge_sources_native_roundtrip(tmp_path: Path) -> None:
    primary = tmp_path / "Primary.esm"
    grafted = tmp_path / "Grafted.esm"
    _write_plugin(
        primary,
        "fnv",
        [("GLOB", 0x1200, "TimeScale"), ("WEAP", 0x1300, "NVPistol")],
    )
    _write_plugin(
        grafted,
        "fo3",
        [("GLOB", 0x9900, "TimeScale"), ("FACT", 0x9A00, "FO3Faction")],
    )
    output = tmp_path / "FNV_FO3_Merged.esm"
    report_path = tmp_path / "merge_report.json"

    report = load_native_module().conversion_merge_sources(
        {
            "primary_paths": [str(primary)],
            "grafted_paths": [str(grafted)],
            "output_path": str(output),
            "report_path": str(report_path),
            "game": "fnv",
        }
    )

    assert report["deduped"] == 1
    assert report["copied"] == 1
    assert output.exists()
    assert report_path.exists()


def test_merge_sources_preserves_four_byte_inline_lstring(tmp_path: Path) -> None:
    primary = tmp_path / "FalloutNV.esm"
    _write_nonlocalized_named_plugin(primary, "Cup")
    output = tmp_path / "FNV_FO3_Merged.esm"

    load_native_module().conversion_merge_sources(
        {
            "primary_paths": [str(primary)],
            "grafted_paths": [],
            "output_path": str(output),
            "report_path": str(tmp_path / "merge_report.json"),
            "game": "fnv",
        }
    )

    handle = plugin_handle_load(str(output), game="fnv")
    try:
        subrecords = plugin_handle_record_subrecords(handle, 0x3407B) or []
        full = next(data for signature, data, _ in subrecords if signature == "FULL")
        assert full == b"Cup\0"
    finally:
        plugin_handle_close(handle)


def test_merge_sources_remaps_localized_ids_across_primary_lineage(
    tmp_path: Path,
) -> None:
    base = tmp_path / "Base.esm"
    patch = tmp_path / "Patch.esm"
    _write_localized_plugin(
        base,
        [
            (0x1000, "OverriddenFaction", 0x10, "Base Name"),
            (0x1100, "BaseFaction", 0x11, "Base Survivor"),
        ],
    )
    _write_localized_plugin(
        patch,
        [
            (0x1000, "OverriddenFaction", 0x10, "Override Name"),
            (0x01002000, "PatchFaction", 0x20, "Patch Name"),
        ],
        masters=(base.name,),
    )
    output = tmp_path / "Merged.esm"

    load_native_module().conversion_merge_sources(
        {
            "primary_paths": [str(base), str(patch)],
            "grafted_paths": [],
            "output_path": str(output),
            "report_path": str(tmp_path / "merge_report.json"),
            "game": "skyrimse",
            "source_strings_dir": str(tmp_path / "Strings"),
        }
    )

    handle = plugin_handle_load(
        str(output),
        game="skyrimse",
        strings_dir=str(tmp_path / "Strings"),
        language="en",
    )
    try:
        assert _resolved_full_name(handle, 0x1000) == "Override Name"
        assert _resolved_full_name(handle, 0x1100) == "Base Survivor"
        assert _resolved_full_name(handle, 0x2000) == "Patch Name"
    finally:
        plugin_handle_close(handle)
