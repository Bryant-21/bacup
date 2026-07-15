from __future__ import annotations

import json
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

from bacup_lib.run import ConversionRun
from creation_lib.esp import Plugin


REPO_ROOT = Path(__file__).resolve().parents[2]


def _write_empty_plugin(path: Path, game: str) -> None:
    plugin = Plugin.new(path.name, game=game)
    try:
        plugin.header.description = "disk-seed"
        plugin.save(path)
    finally:
        plugin.close()


def test_creation_and_bacup_plugin_registries_are_isolated_in_fresh_process(
    tmp_path: Path,
) -> None:
    plugin_path = tmp_path / "Isolation.esm"
    _write_empty_plugin(plugin_path, "fo4")
    script = textwrap.dedent(
        """
        import json
        import sys
        from pathlib import Path

        import bacup_lib._native as bacup_native
        from bacup_lib.run import ConversionRun
        from creation_lib.esp import Plugin
        from creation_lib.esp import native_runtime as creation_runtime

        plugin_path = Path(sys.argv[1])
        output_dir = Path(sys.argv[2])
        creation_handle = None
        bacup_handle = None
        live_creation_plugin = None
        run = None
        result = {}

        def bacup_description(handle_id):
            return bacup_native.esp_authoring_core.plugin_handle_get_meta(handle_id)[4][4]

        try:
            creation_handle = creation_runtime.plugin_handle_load(
                str(plugin_path), game="fo4"
            )
            bacup_handle = bacup_native.esp_authoring_core.plugin_handle_load(
                str(plugin_path), "fo4"
            )
            assert creation_handle == bacup_handle == 1

            creation_runtime.plugin_handle_call(
                creation_handle, "set_header_description", "creation-only"
            )
            assert creation_runtime.plugin_handle_get(
                creation_handle, "header_description"
            ) == "creation-only"
            assert bacup_description(bacup_handle) == "disk-seed"

            bacup_native.esp_authoring_core.plugin_handle_set_header_field(
                bacup_handle, "description", "bacup-only"
            )
            assert bacup_description(bacup_handle) == "bacup-only"
            assert creation_runtime.plugin_handle_get(
                creation_handle, "header_description"
            ) == "creation-only"

            assert creation_runtime.plugin_handle_close(creation_handle)
            creation_handle = None
            assert bacup_native.esp_authoring_core.plugin_handle_close(bacup_handle)
            bacup_handle = None

            live_creation_plugin = Plugin.load(plugin_path, game="fo4")
            assert live_creation_plugin.record_count == 0
            run = ConversionRun.create_new(
                "fo4",
                "fo4",
                str(plugin_path),
                "RunTarget.esm",
                config={"mod_path": str(output_dir)},
            )
            run.close()
            run = None

            assert live_creation_plugin.record_count == 0
            live_creation_plugin.header.description = "creation-still-live"
            assert live_creation_plugin.header.description == "creation-still-live"
            result = {
                "creation_handle": 1,
                "bacup_handle": 1,
                "record_count_after_run_drop": live_creation_plugin.record_count,
            }
        finally:
            if run is not None:
                run.close()
            if live_creation_plugin is not None:
                live_creation_plugin.close()
            if creation_handle is not None:
                creation_runtime.plugin_handle_close(creation_handle)
            if bacup_handle is not None:
                bacup_native.esp_authoring_core.plugin_handle_close(bacup_handle)

        print(json.dumps(result, sort_keys=True))
        """
    )
    completed = subprocess.run(
        [sys.executable, "-c", script, str(plugin_path), str(tmp_path)],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        timeout=60,
    )

    assert completed.returncode == 0, completed.stderr
    assert json.loads(completed.stdout) == {
        "bacup_handle": 1,
        "creation_handle": 1,
        "record_count_after_run_drop": 0,
    }


def test_conversion_module_does_not_register_foreign_plugin_handle_apis() -> None:
    import bacup_lib._native as bacup_native

    conversion = bacup_native.conversion_native
    assert not [name for name in dir(bacup_native) if name.startswith("plugin_handle_")]
    assert not [name for name in dir(conversion) if name.startswith("plugin_handle_")]
    forbidden = {
        "conversion_collect_eid_rows",
        "conversion_collect_eid_rows_from_path",
        "conversion_record_refs_by_signature",
        "conversion_record_refs_by_form_keys",
        "conversion_normalize_placed_records",
        "conversion_plugin_set_snam",
        "conversion_run_source_handle",
        "conversion_run_target_handle",
    }
    assert forbidden.isdisjoint(dir(conversion))


@pytest.mark.parametrize(
    ("source_game", "phase", "params", "legacy_key"),
    [
        ("fo4", "walk", {"source_handle": 1}, "source_handle"),
        ("fo4", "walk", {"master_handles": [1]}, "master_handles"),
        ("fo4", "graft_terrain", {"prior_handle_id": 1}, "prior_handle_id"),
        ("fo4", "regenerate_modt", {"output_handle_id": 1}, "output_handle_id"),
        (
            "fo4",
            "regenerate_modt",
            {"deployed_esm_handle_id": 1},
            "deployed_esm_handle_id",
        ),
        ("fo76", "convert_terrain", {"source_handle_id": 1}, "source_handle_id"),
        ("fo76", "convert_terrain", {"target_handle_id": 1}, "target_handle_id"),
        (
            "fo76",
            "convert_terrain",
            {"record_output_mode": "target_handle"},
            "record_output_mode",
        ),
    ],
)
def test_run_phases_reject_legacy_hidden_handle_options(
    tmp_path: Path,
    source_game: str,
    phase: str,
    params: dict[str, object],
    legacy_key: str,
) -> None:
    source_path = tmp_path / "Source.esm"
    _write_empty_plugin(source_path, source_game)

    with ConversionRun.create_new(
        source_game,
        "fo4",
        str(source_path),
        "Target.esm",
        config={"mod_path": str(tmp_path)},
    ) as run:
        with pytest.raises((ValueError, RuntimeError), match=legacy_key):
            run.run_phase(phase, mod_path=str(tmp_path), params=params)


@pytest.mark.parametrize(
    "legacy_key", ("source_handle_id", "target_handle_id", "record_output_mode")
)
def test_standalone_terrain_rejects_legacy_hidden_handle_options(
    legacy_key: str,
) -> None:
    import bacup_lib._native as bacup_native

    with pytest.raises(RuntimeError, match=legacy_key):
        bacup_native.conversion_native.conversion_terrain_with_textures(
            json.dumps({legacy_key: 1})
        )
