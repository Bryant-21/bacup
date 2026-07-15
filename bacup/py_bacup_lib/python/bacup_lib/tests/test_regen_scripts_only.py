from __future__ import annotations

import types

from bacup_lib import regen_pipeline, target_assets
from bacup_lib.workflows import unified


def test_scripts_only_preserves_existing_plugin_and_disables_non_script_phases(
    monkeypatch, tmp_path
):
    source_root = tmp_path / "source"
    source_root.mkdir()
    target_data = tmp_path / "target" / "Data"
    target_data.mkdir(parents=True)
    output_root = tmp_path / "mods" / "SeventySix"
    output_root.mkdir(parents=True)
    plugin_path = output_root / "SeventySix.esm"
    plugin_bytes = b"existing plugin must remain untouched"
    plugin_path.write_bytes(plugin_bytes)
    diagnostics_root = tmp_path / "diagnostics"

    observed = {}
    target_store = types.SimpleNamespace(
        list_assets=lambda **_kwargs: [],
    )
    monkeypatch.setattr(
        target_assets,
        "ensure_target_asset_catalog",
        lambda *_args, **_kwargs: tmp_path / "target-assets.sqlite3",
    )
    monkeypatch.setattr(
        target_assets,
        "build_target_asset_store",
        lambda **_kwargs: target_store,
    )

    def run_script_phase(runtime, ctx, runner):
        observed["request"] = runtime._req
        observed["context"] = ctx
        runner.emit_log("INFO", "test script phase")

    monkeypatch.setattr(
        unified._UnifiedRecordRuntime,
        "_run_convert_scripts_phase",
        run_script_phase,
    )
    paths = regen_pipeline.RegenPaths(
        source_extracted_dir=source_root,
        source_data_dir=tmp_path / "source-data",
        target_data_dir=target_data,
        target_ck_ini_path=tmp_path / "CreationKit.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=output_root,
        diagnostics_root=diagnostics_root,
    )
    runner = types.SimpleNamespace(emit_log=lambda *_args: None)

    result = regen_pipeline.run_scripts_only(
        paths,
        regen_pipeline.RegenOptions(deploy=False, workers=3),
        runner=runner,
    )

    request = observed["request"]
    context = observed["context"]
    assert result.exit_code == 0
    assert result.deployed is False
    assert plugin_path.read_bytes() == plugin_bytes
    assert request.source_plugins == []
    assert request.options.convert_scripts is True
    assert request.options.build_esp is False
    assert request.options.translate_records is False
    assert request.options.convert_terrain is False
    assert request.options.convert_nifs is False
    assert request.options.convert_textures is False
    assert request.options.convert_materials is False
    assert request.options.convert_havok is False
    assert request.options.convert_animations is False
    assert request.options.copy_sounds is False
    assert not hasattr(context, "rust_target_handle_id")
    assert context.scripts_only is True
    assert context.target_asset_store is target_store
