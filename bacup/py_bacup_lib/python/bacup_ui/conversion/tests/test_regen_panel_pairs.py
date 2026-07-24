from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.regen_pipeline import RegenResult
from bacup_lib.source_pairs import get_pair
from bacup_ui.conversion.panels.regen_panel import RegenPanel


def _workspace(game_paths):
    return SimpleNamespace(
        _toolkit_settings=SimpleNamespace(
            get_game_paths=lambda game_id: dict(game_paths.get(game_id, {})),
            get_workspace_settings=lambda _workspace_id: {},
            set_workspace_settings=lambda _workspace_id, _values: None,
        ),
        _runner=None,
    )


def _game_paths(tmp_path: Path):
    return {
        "fo4": {
            "root_dir": str(tmp_path / "Fallout4"),
            "extracted_dir": str(tmp_path / "extracted" / "fo4"),
        },
        "fo76": {
            "root_dir": str(tmp_path / "Fallout76"),
            "extracted_dir": str(tmp_path / "extracted" / "fo76"),
        },
        "fnv": {
            "root_dir": str(tmp_path / "FalloutNV"),
            "extracted_dir": str(tmp_path / "extracted" / "fnv"),
        },
        "fo3": {
            "root_dir": str(tmp_path / "Fallout3"),
            "extracted_dir": str(tmp_path / "extracted" / "fo3"),
        },
        "skyrimse": {
            "root_dir": str(tmp_path / "SkyrimSE"),
            "extracted_dir": str(tmp_path / "extracted" / "skyrimse"),
        },
    }


def _touch_plugins(root: Path, names):
    data_dir = root / "Data"
    data_dir.mkdir(parents=True, exist_ok=True)
    for name in names:
        (data_dir / name).write_bytes(b"TES4")


def test_build_paths_uses_fnv_pair_and_populates_merge_inputs(monkeypatch, tmp_path):
    pair = get_pair("fnvfo3:fo4")
    paths_by_game = _game_paths(tmp_path)
    _touch_plugins(tmp_path / "FalloutNV", pair.source_plugins)
    _touch_plugins(tmp_path / "FalloutNV", ("CaravanPack.esm",))
    _touch_plugins(tmp_path / "Fallout3", pair.merge.grafted_plugins)
    panel = RegenPanel(_workspace(paths_by_game))
    panel.pair_id = pair.pair_id
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: tmp_path / "app"
    )

    paths = panel.build_paths()

    assert paths.source_data_dir == tmp_path / "FalloutNV" / "Data"
    assert paths.source_extracted_dir == tmp_path / "extracted" / "fnv"
    assert paths.target_data_dir == tmp_path / "Fallout4" / "Data"
    assert paths.output_root == tmp_path / "app" / "mods" / "FNV_FO3_Merged"
    assert paths.target_asset_catalog_path == (
        tmp_path / "app" / "cache" / "conversion" / "fo4_target_assets.sqlite3"
    )
    assert paths.target_asset_cache_dir == (
        tmp_path / "app" / "cache" / "conversion" / "target_assets"
    )
    assert paths.mod_name == "FNV_FO3_Merged"
    assert paths.merge_primary_plugin_paths == tuple(
        tmp_path / "FalloutNV" / "Data" / name
        for name in (*pair.source_plugins, "CaravanPack.esm")
    )
    assert paths.merge_grafted_plugin_paths == tuple(
        tmp_path / "Fallout3" / "Data" / name for name in pair.merge.grafted_plugins
    )
    assert paths.additional_source_asset_roots == (tmp_path / "extracted" / "fo3",)


def test_build_paths_populates_skyrim_flatten_only_inputs(monkeypatch, tmp_path):
    pair = get_pair("skyrimse:fo4")
    _touch_plugins(tmp_path / "SkyrimSE", pair.source_plugins)
    panel = RegenPanel(_workspace(_game_paths(tmp_path)))
    panel.pair_id = pair.pair_id
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: tmp_path / "app"
    )

    paths = panel.build_paths()

    assert paths.output_root == tmp_path / "app" / "mods" / "Skyrim_Merged"
    assert paths.merge_primary_plugin_paths == tuple(
        tmp_path / "SkyrimSE" / "Data" / name for name in pair.source_plugins
    )
    assert paths.merge_grafted_plugin_paths == ()
    assert paths.additional_source_asset_roots == ()


def test_steam_gate_uses_selected_pair_games(monkeypatch, tmp_path):
    panel = RegenPanel(_workspace(_game_paths(tmp_path)))
    panel.pair_id = "fnvfo3:fo4"
    checked = []

    def result(game_id):
        checked.append(game_id)
        return SimpleNamespace(ok=True)

    monkeypatch.setattr(panel, "_steam_install_result", result)

    assert panel._steam_installs_ok() is True
    assert checked == ["fo4", "fnv", "fo3"]


def test_non_default_start_passes_pair_and_supports_pair_upgrade(
    monkeypatch, tmp_path
):
    pair = get_pair("fnvfo3:fo4")
    paths_by_game = _game_paths(tmp_path)
    _touch_plugins(tmp_path / "FalloutNV", pair.source_plugins)
    _touch_plugins(tmp_path / "Fallout3", pair.merge.grafted_plugins)
    (tmp_path / "extracted" / "fnv").mkdir(parents=True)
    (tmp_path / "extracted" / "fo3").mkdir(parents=True)
    panel = RegenPanel(_workspace(paths_by_game))
    panel.pair_id = pair.pair_id
    panel.install_location = "none"
    panel.lod_mode = "none"
    panel.ba2_target = "og"
    panel.upgrade = True
    captured = {}

    class FakeRunner:
        def __init__(self, work):
            self._work = work

        def start(self):
            self._work(self)

        def emit_complete(self, mod_path, summary):
            captured["complete"] = (mod_path, summary)

        def emit_log(self, *_args):
            pass

        def emit_phase_start(self, _progress):
            pass

        def emit_item_progress(self, _progress):
            pass

        def emit_phase_complete(self, _progress):
            pass

        def is_cancelled(self):
            return False

    def fake_run(paths, options, *, pair, phases, runner, **_kwargs):
        captured["paths"] = paths
        captured["options"] = options
        captured["pair"] = pair
        return RegenResult(
            exit_code=0,
            output_root=paths.output_root,
            elapsed_seconds=0.1,
            deployed=True,
        )

    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: tmp_path / "app"
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.scan_conversion_inputs",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(
            AssertionError("FO76 preflight must not run")
        ),
    )
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.ConversionRunner", FakeRunner)
    monkeypatch.setattr(
        "bacup_lib.regen_pipeline.run_full_regen", fake_run
    )
    monkeypatch.setattr(
        panel,
        "_require_steam_installs",
        lambda: None,
    )
    monkeypatch.setattr(
        panel,
        "_deploy_companion_mod",
        lambda *_args: (_ for _ in ()).throw(
            AssertionError("Appalachia companion must not deploy")
        ),
    )
    monkeypatch.setattr(panel, "_cleanup_after_deploy", lambda *_args: [])
    monkeypatch.setattr(
        panel,
        "_load_upgrade_manifest_cached",
        lambda: SimpleNamespace(current="alpha2"),
    )

    panel.start_conversion()

    assert captured["pair"] is pair
    assert captured["options"].upgrade is True
    assert captured["paths"].mod_name == "FNV_FO3_Merged"
    assert captured["paths"].additional_source_asset_roots == (
        tmp_path / "extracted" / "fo3",
    )
    assert captured["complete"][1]["companion_deployed"] == []


@pytest.mark.parametrize(
    ("pair_id", "expected_plugin"),
    [
        ("fnvfo3:fo4", "FNV_FO3_Merged.esm"),
        ("skyrimse:fo4", "Skyrim_Merged.esm"),
        ("fo76:fo4", "SeventySix.esm"),
    ],
)
def test_install_audit_uses_pair_output_plugin(
    monkeypatch, tmp_path, pair_id, expected_plugin
):
    pair = get_pair(pair_id)
    panel = RegenPanel(_workspace(_game_paths(tmp_path)))
    panel.pair_id = pair_id
    panel.install_location = "none"
    captured = {}
    paths = SimpleNamespace(
        output_root=tmp_path / "mods" / pair.output_mod_name,
        mod_name=pair.output_mod_name,
        target_data_dir=tmp_path / "Fallout4" / "Data",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
    )

    monkeypatch.setattr(panel, "build_paths", lambda: paths)
    monkeypatch.setattr(
        panel,
        "_resolve_install_target",
        lambda *_args: SimpleNamespace(
            deploy_data_dir=None,
            runtime_ini_path=tmp_path / "Fallout4Custom.ini",
        ),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.audit_archive_ini",
        lambda **kwargs: captured.update(kwargs) or SimpleNamespace(),
    )

    panel._run_install_audit()

    assert captured["plugin_name"] == expected_plugin
