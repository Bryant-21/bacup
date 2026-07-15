from pathlib import Path

from bacup_lib.native_runtime import load_native_module
from creation_lib.esp.native_runtime import (
    plugin_handle_add_record_raw,
    plugin_handle_call,
    plugin_handle_close,
    plugin_handle_new,
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
