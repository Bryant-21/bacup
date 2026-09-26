import threading
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.regen_pipeline import _clean_forced_regen_output
from bacup_lib import tales_config
from bacup_lib.lod_settings import PROFILE_HIGH_QUALITY
from bacup_ui.conversion.panels.regen_panel import (
    _COMPANION_MOD_NAME,
    _FULL_SCREEN_MAP_MOD_NAME,
    _UNLIMITED_ARCHIVE_MAX_BYTES,
    _project_disk_space,
    RegenPanel,
)


def _ws(fo4_root, fo76_root, fo76_ext, workspace_settings=None):
    ws_settings = dict(workspace_settings or {})
    paths = {
        "fo4": {"root_dir": fo4_root, "extracted_dir": fo4_root + "/Data"},
        "fo76": {"root_dir": fo76_root, "extracted_dir": fo76_ext},
    }
    return SimpleNamespace(
        _toolkit_settings=SimpleNamespace(
            get_game_paths=lambda g: dict(paths.get(g, {})),
            get_workspace_settings=lambda _w: dict(ws_settings),
            set_workspace_settings=lambda _w, values: ws_settings.update(values),
        ),
        _runner=None,
        _workspace_settings=ws_settings,
    )


def _panel(ws):
    p = RegenPanel.__new__(RegenPanel)
    p._workspace = ws
    p.install_location = "game"
    p.install_path = ""
    p.fo76_source = "retail"
    p.mo2_use_profile_ini = True
    p.deploy = True
    p.add_archives_to_ini = True
    p.deploy_data_dir = ""
    p._install_audit = None
    p._install_audit_error = None
    p.archive_max_gb = 8
    p.ba2_compression_level = None
    p.deploy_format = "expanded"
    p.workers = 0
    p.lod_mode = "hybrid-atlas"
    p.lod_profile = PROFILE_HIGH_QUALITY
    p.atlas_mip_flooding = False
    p.texture_landscape_mip_flooding = False
    p.full_logging = False
    p.re_use_land = False
    p.recovery_phase = "lodgen"
    p._phases = []
    p._summary = None
    p._completion = None
    p._disk_usage_cache = None
    p._disk_usage_cache_key = None
    p._disk_space_cache = None
    p._disk_usage_lock = threading.Lock()
    p._disk_usage_running = False
    p._disk_usage_thread = None
    p._waiting_for_space_check = False
    p._low_space_warning = None
    p._fo4_exe_version_cache = None
    p._store_install_cache = {}
    p._preflight_report = None
    p._preflight_cache = None
    return p


def test_build_paths_resolves_install_targets(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    paths = panel.build_paths()
    docs = Path.home() / "Documents" / "My Games" / "Fallout4"
    assert paths.target_data_dir == Path("C:/FO4/Data")
    assert paths.source_extracted_dir == Path("C:/x/fo76")
    assert paths.source_data_dir == Path("C:/FO76/Data")
    assert paths.target_ck_ini_path == Path("C:/FO4/CreationKitCustom.ini")
    assert paths.output_root == Path("X:/app/mods/SeventySix")
    assert paths.deploy_data_dir is None
    assert paths.runtime_ini_path == docs / "Fallout4Custom.ini"

    panel.deploy_data_dir = "C:/FO4/Data"
    assert panel.build_paths().deploy_data_dir is None

    panel = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76", {
        "install_location": "vortex",
        "install_path": "D:/Vortex/fallout4/mods/SeventySix",
    }))
    assert panel.build_paths().deploy_data_dir == Path("D:/Vortex/fallout4/mods/SeventySix")


def test_build_paths_relocates_workspace_outside_target_data(monkeypatch, tmp_path):
    fo4_root = tmp_path / "Fallout 4"
    fo4_data = fo4_root / "Data"
    fo76_root = tmp_path / "Fallout76"
    fo76_extracted = tmp_path / "extracted" / "fo76"
    fo4_data.mkdir(parents=True)
    (fo76_root / "Data").mkdir(parents=True)
    fo76_extracted.mkdir(parents=True)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_exe_dir",
        lambda: fo4_data,
    )
    panel = _panel(_ws(str(fo4_root), str(fo76_root), str(fo76_extracted)))

    paths = panel.build_paths()

    assert paths.output_root == (
        fo4_root / "BACUP Workspace" / "mods" / "SeventySix"
    )
    paths.output_root.mkdir(parents=True)
    runner = SimpleNamespace(emit_log=lambda *_args: None)
    _clean_forced_regen_output(paths, runner)
    assert not paths.output_root.exists()


@pytest.mark.parametrize("deploy_format, add_to_ini, expected", [
    ("expanded", True, {
        "ba2_mode": "expanded", "archive_max_bytes": 8 * 1024**3,
        "deploy_loose": False, "direct_deploy_archives": True,
    }),
    ("standard", True, {
        "ba2_mode": "packed", "archive_max_bytes": _UNLIMITED_ARCHIVE_MAX_BYTES,
        "deploy_loose": False, "direct_deploy_archives": True,
        "update_runtime_ini": True, "upgrade": True, "hydrate_upgrade_from_deployed": True,
    }),
    ("loose", False, {
        "deploy_loose": True, "direct_deploy_archives": False,
        "update_runtime_ini": True, "upgrade": True, "hydrate_upgrade_from_deployed": True,
    }),
])
def test_deploy_formats_map_to_regen_options(deploy_format, add_to_ini, expected):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.deploy_format = deploy_format
    panel.add_archives_to_ini = add_to_ini
    panel.upgrade = True
    panel._upgrade_requires_full_build = lambda _manifest: False
    opts = panel.build_options()
    assert {key: getattr(opts, key) for key in expected} == expected
    assert opts.workers is None
    assert opts.include_interior is True
    assert opts.records_limit is None


def test_interior_only_cdx_is_off_by_default_and_reaches_regen_options():
    assert RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76")).interior_cdx_only is False
    saved = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76", {"interior_cdx_only": True}))
    assert saved.interior_cdx_only is True

    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.upgrade = True
    panel._upgrade_requires_full_build = lambda _manifest: False
    assert panel.build_options().interior_cdx_only is False
    panel.interior_cdx_only = True
    assert panel.build_options().interior_cdx_only is True


@pytest.mark.parametrize("free_gib, expect_warning", [(10, True), (450, False)])
def test_request_conversion_gates_on_fresh_space_check(monkeypatch, free_gib, expect_warning):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    volume = _project_disk_space(
        output_root=Path("C:/BACUP/mods/SeventySix"),
        archive_root=Path("C:/Fallout4/Data"),
        volume_key=lambda _path: "c:",
        disk_usage=lambda _path: SimpleNamespace(
            total=500 * 1024**3, used=(500 - free_gib) * 1024**3, free=free_gib * 1024**3
        ),
    )[0]
    starts = []
    monkeypatch.setattr(panel, "_start_disk_usage_worker", lambda **_kwargs: None)
    monkeypatch.setattr(panel, "_disk_space_projection", lambda: (volume,))
    monkeypatch.setattr(panel, "start_conversion", lambda: starts.append(True))

    panel._request_conversion()

    assert panel._waiting_for_space_check is True
    assert starts == []

    panel._resolve_pending_space_check()

    if expect_warning:
        assert panel._low_space_warning == (volume,)
        assert starts == []
        panel._continue_conversion_with_low_space()
    assert panel._low_space_warning is None
    assert starts == [True]


def test_cleanup_removes_only_app_owned_default_paths(tmp_path, monkeypatch):
    exe_dir = tmp_path / "app"
    output_root = exe_dir / "mods" / "SeventySix"
    fo4_ext = exe_dir / "extracted" / "fo4"
    fo76_ext = exe_dir / "extracted" / "fo76"
    for path in (output_root, fo4_ext, fo76_ext):
        path.mkdir(parents=True)
        (path / "file.txt").write_text("data", encoding="utf-8")

    workspace_settings = {
        "cleanup_mod_output_after_deploy": True,
        "cleanup_app_owned_extracted": True,
        "app_owned_extracted_games": ["fo4", "fo76"],
        "app_owned_extracted_paths": {
            "fo4": str(fo4_ext),
            "fo76": str(fo76_ext),
        },
    }
    paths = {
        "fo4": {"root_dir": str(tmp_path / "FO4"), "extracted_dir": str(fo4_ext)},
        "fo76": {"root_dir": str(tmp_path / "FO76"), "extracted_dir": str(fo76_ext)},
    }
    class Settings:
        def get_game_paths(self, game_id):
            return dict(paths.get(game_id, {}))

        def set_game_extracted_dir(self, game_id, value):
            paths[game_id]["extracted_dir"] = value

        def get_workspace_settings(self, _workspace_id):
            return dict(workspace_settings)

        def set_workspace_settings(self, _workspace_id, values):
            workspace_settings.update(values)

        def save(self):
            pass

    ws = SimpleNamespace(_toolkit_settings=Settings(), _runner=None)
    panel = _panel(ws)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: exe_dir)
    regen_paths = panel.build_paths()

    removed = panel._cleanup_after_deploy(regen_paths, True)

    assert str(output_root) in removed
    assert str(fo76_ext) in removed
    assert not output_root.exists()
    assert not fo76_ext.exists()
    assert fo4_ext.exists()
    assert paths["fo76"]["extracted_dir"] == ""


def test_deploy_companion_mod_copies_runtime_payload(tmp_path, monkeypatch):
    exe_dir = tmp_path / "app"
    companion = exe_dir / "mods" / _COMPANION_MOD_NAME
    full_screen_map = exe_dir / "mods" / _FULL_SCREEN_MAP_MOD_NAME
    fo4_root = tmp_path / "Fallout4"
    fo4_data = fo4_root / "Data"

    (companion / "data" / "Scripts" / "B21").mkdir(parents=True)
    (companion / "data" / "Meshes" / "B21").mkdir(parents=True)
    companion_map = (
        companion
        / "PrismaUI_F4"
        / "views"
        / _FULL_SCREEN_MAP_MOD_NAME
        / "maps"
        / "appalachia"
    )
    companion_map.mkdir(parents=True)
    (companion / "F4SE" / "Plugins").mkdir(parents=True)
    (full_screen_map / "data" / "Scripts").mkdir(parents=True)
    full_screen_map_view = (
        full_screen_map / "PrismaUI_F4" / "views" / _FULL_SCREEN_MAP_MOD_NAME
    )
    (full_screen_map_view / "maps" / "appalachia").mkdir(parents=True)
    (full_screen_map / "F4SE" / "Plugins").mkdir(parents=True)
    (companion / f"{_COMPANION_MOD_NAME}.esm").write_bytes(b"esm")
    (companion / "F4SE" / "Plugins" / f"{_COMPANION_MOD_NAME}.dll").write_bytes(b"dll")
    (companion / "F4SE" / "Plugins" / f"{_COMPANION_MOD_NAME}.pdb").write_bytes(b"pdb")
    (companion / "F4SE" / "Plugins" / f"{_COMPANION_MOD_NAME}.ini").write_text(
        "[General]\niVersion=2\n[HUD]\nbCrosshair=1\nbDamageNumbers=0\n", encoding="utf-8"
    )
    (companion / "data" / "Scripts" / "B21" / "B21_AT_TeleportSign.pex").write_bytes(b"pex")
    (companion / "data" / "Meshes" / "B21" / "marker.nif").write_bytes(b"nif")
    (companion_map / "map.json").write_text(
        "{}",
        encoding="utf-8",
    )
    (full_screen_map / "data" / "Scripts" / "B21_FullScreenMap.pex").write_bytes(
        b"map-pex"
    )
    (full_screen_map / "F4SE" / "Plugins" / "B21_FullScreenMap.dll").write_bytes(
        b"map-dll"
    )
    (full_screen_map_view / "index.html").write_text(
        "<html></html>",
        encoding="utf-8",
    )
    (full_screen_map_view / "maps" / "appalachia" / "map.json").write_text(
        '{"source": "base"}',
        encoding="utf-8",
    )
    (fo4_data / "Scripts" / "B21").mkdir(parents=True)
    (fo4_data / "Meshes" / "B21").mkdir(parents=True)
    (fo4_data / "Scripts" / "B21" / "B21_AT_TeleportSign.pex").write_bytes(b"stale")
    (fo4_data / "Meshes" / "B21" / "marker.nif").write_bytes(b"stale")
    (fo4_data / "Strings").mkdir(parents=True)
    stale_string = fo4_data / "Strings" / f"{_COMPANION_MOD_NAME}_cn.DLSTRINGS"
    stale_string.write_bytes(b"stale")
    unrelated_string = fo4_data / "Strings" / "B21_OtherMod_cn.DLSTRINGS"
    unrelated_string.write_bytes(b"keep")
    installed_ini = fo4_data / "F4SE" / "Plugins" / f"{_COMPANION_MOD_NAME}.ini"
    installed_ini.parent.mkdir(parents=True)
    installed_ini.write_text("[General]\niVersion=2\n[HUD]\nbCrosshair=0\n", encoding="utf-8")

    ws = _ws(str(fo4_root), "C:/FO76", "C:/x/fo76",
             {"tales_config_edits": {"HUD/bDamageNumbers": "1"}})
    panel = _panel(ws)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: exe_dir)
    pack_calls = []

    def fake_pack_mod(mod_name, **kwargs):
        pack_calls.append((mod_name, kwargs))
        (companion / f"{_COMPANION_MOD_NAME} - Main.ba2").write_bytes(b"ba2")

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.pack_mod", fake_pack_mod)
    paths = panel.build_paths()
    logs: list[tuple[str, str]] = []
    runner = SimpleNamespace(emit_log=lambda level, message: logs.append((level, message)))

    deployed = panel._deploy_companion_mod(paths, runner)

    assert f"{_COMPANION_MOD_NAME}.esm" in deployed
    assert f"{_COMPANION_MOD_NAME} - Main.ba2" in deployed
    # Map 4.0 is native: neither Tales nor the bundled FullScreenMap deploys a PrismaUI view.
    assert not any(path.startswith("PrismaUI_F4/") for path in deployed)
    assert "F4SE/Plugins/B21_FullScreenMap.dll" in deployed
    assert "Scripts/B21_FullScreenMap.pex" in deployed
    assert (fo4_data / f"{_COMPANION_MOD_NAME}.esm").read_bytes() == b"esm"
    assert (fo4_data / f"{_COMPANION_MOD_NAME} - Main.ba2").read_bytes() == b"ba2"
    assert not stale_string.exists()
    assert unrelated_string.read_bytes() == b"keep"
    assert (fo4_data / "Scripts" / "B21" / "B21_AT_TeleportSign.pex").read_bytes() == b"stale"
    assert (fo4_data / "Meshes" / "B21" / "marker.nif").read_bytes() == b"stale"
    # The F4SE plugin is a hard runtime dependency -- it corrects Fallout 4's 16-bit
    # auto-calc health truncation, without which converted creatures above 32767 health
    # spawn negative. Its .pdb ships too, so user crash reports can be symbolicated.
    assert f"F4SE/Plugins/{_COMPANION_MOD_NAME}.dll" in deployed
    assert f"F4SE/Plugins/{_COMPANION_MOD_NAME}.pdb" in deployed
    assert (fo4_data / "F4SE" / "Plugins" / f"{_COMPANION_MOD_NAME}.dll").read_bytes() == b"dll"
    assert (fo4_data / "F4SE" / "Plugins" / f"{_COMPANION_MOD_NAME}.pdb").read_bytes() == b"pdb"
    assert (
        fo4_data / "F4SE" / "Plugins" / "B21_FullScreenMap.dll"
    ).read_bytes() == b"map-dll"
    assert (fo4_data / "Scripts" / "B21_FullScreenMap.pex").read_bytes() == b"map-pex"
    # The installed ini's values survive the shipped copy; pending Configure Features edits apply once.
    tales_ini = tales_config.parse_ini(installed_ini.read_text(encoding="utf-8"))
    assert tales_ini["hud"]["bcrosshair"] == "0"
    assert tales_ini["hud"]["bdamagenumbers"] == "1"
    assert ws._workspace_settings["tales_config_edits"] == {}
    deployed_map_view = (
        fo4_data / "PrismaUI_F4" / "views" / _FULL_SCREEN_MAP_MOD_NAME
    )
    assert not deployed_map_view.exists()
    assert pack_calls == [
        (
            _COMPANION_MOD_NAME,
            {
                "game": "fo4",
                "project_root": exe_dir,
                "archive_max_bytes": _UNLIMITED_ARCHIVE_MAX_BYTES,
                "expanded_archives": False,
                "archive_workers": panel.workers,
                "fo4_ba2_target": "og",
            },
        )
    ]
    assert logs and logs[-1][1].startswith("Companion mod")


@pytest.mark.parametrize("install_devtools", [False, True])
def test_deploy_companion_mod_installs_devtools_only_when_opted_in(
    tmp_path, monkeypatch, install_devtools
):
    exe_dir = tmp_path / "app"
    companion = exe_dir / "mods" / _COMPANION_MOD_NAME
    full_screen_map = exe_dir / "mods" / _FULL_SCREEN_MAP_MOD_NAME
    devtools_plugins = exe_dir / "mods" / "B21_DevTools" / "F4SE" / "Plugins"
    fo4_root = tmp_path / "Fallout4"
    installed_plugins = fo4_root / "Data" / "F4SE" / "Plugins"

    (companion / "data").mkdir(parents=True)
    (companion / "F4SE" / "Plugins").mkdir(parents=True)
    (companion / f"{_COMPANION_MOD_NAME}.esm").write_bytes(b"esm")
    (full_screen_map / "data").mkdir(parents=True)
    (full_screen_map / "F4SE").mkdir(parents=True)
    (devtools_plugins / "B21_DevTools" / "fonts").mkdir(parents=True)
    (devtools_plugins / "B21_DevTools.dll").write_bytes(b"devtools-dll")
    (devtools_plugins / "B21_DevTools.ini").write_text("[Hotkeys]\niWorkbench=121\n", encoding="utf-8")
    (devtools_plugins / "B21_DevTools" / "fonts" / "Roboto-Regular.ttf").write_bytes(b"font")
    installed_plugins.mkdir(parents=True)
    user_ini = installed_plugins / "B21_DevTools.ini"
    user_ini.write_text("[Hotkeys]\niWorkbench=0\n", encoding="utf-8")

    panel = _panel(_ws(str(fo4_root), "C:/FO76", "C:/x/fo76"))
    panel.install_devtools = install_devtools
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: exe_dir)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.pack_mod",
        lambda mod_name, **kwargs: (companion / f"{_COMPANION_MOD_NAME} - Main.ba2").write_bytes(b"ba2"),
    )

    deployed = panel._deploy_companion_mod(panel.build_paths())

    assert ("F4SE/Plugins/B21_DevTools.dll" in deployed) is install_devtools
    assert (installed_plugins / "B21_DevTools.dll").exists() is install_devtools
    assert (installed_plugins / "B21_DevTools" / "fonts" / "Roboto-Regular.ttf").exists() is install_devtools
    assert user_ini.read_text(encoding="utf-8") == "[Hotkeys]\niWorkbench=0\n"


def test_undeploy_bundled_mods_removes_every_bundled_file_and_nothing_else(
    tmp_path, monkeypatch
):
    exe_dir = tmp_path / "app"
    (exe_dir / "mods" / _FULL_SCREEN_MAP_MOD_NAME / "data" / "Scripts").mkdir(parents=True)
    (exe_dir / "mods" / _FULL_SCREEN_MAP_MOD_NAME / "data" / "Scripts" / "B21_FullScreenMap.pex").write_bytes(b"src")
    fo4_root = tmp_path / "Fallout4"
    data = fo4_root / "Data"
    bundled = [
        f"{_COMPANION_MOD_NAME}.esm",
        f"{_COMPANION_MOD_NAME} - Main.ba2",
        f"Strings/{_COMPANION_MOD_NAME}_en.STRINGS",
        f"F4SE/Plugins/{_COMPANION_MOD_NAME}.dll",
        f"F4SE/Plugins/{_COMPANION_MOD_NAME}.ini",
        f"F4SE/Plugins/{_COMPANION_MOD_NAME}_en.txt",
        f"F4SE/Plugins/{_COMPANION_MOD_NAME}/challenges.json",
        "F4SE/Plugins/B21_FullScreenMap.dll",
        "F4SE/Plugins/B21_FullScreenMap/maps/appalachia/map.dds",
        "F4SE/Plugins/B21_DevTools.dll",
        "F4SE/Plugins/B21_DevTools.ini",
        "F4SE/Plugins/B21_DevTools/fonts/Roboto-Regular.ttf",
        "Interface/B21/TalesFromAppalachia/QuickBoy.swf",
        "Scripts/B21_FullScreenMap.pex",
    ]
    unrelated = [
        "F4SE/Plugins/B21_OtherMod.dll",
        "Strings/B21_OtherMod_en.STRINGS",
        "Interface/B21/OtherMod/menu.swf",
        "Scripts/OtherScript.pex",
        "SeventySix.esm",
    ]
    for rel in bundled + unrelated:
        (data / rel).parent.mkdir(parents=True, exist_ok=True)
        (data / rel).write_bytes(b"x")

    panel = _panel(_ws(str(fo4_root), "C:/FO76", "C:/x/fo76"))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: exe_dir)

    removed = panel._undeploy_bundled_mods(panel.build_paths())

    assert sorted(removed) == sorted(bundled)
    assert not any((data / rel).exists() for rel in bundled)
    assert all((data / rel).exists() for rel in unrelated)
    assert not (data / "F4SE" / "Plugins" / _COMPANION_MOD_NAME).exists()
    assert not (data / "Interface" / "B21" / "TalesFromAppalachia").exists()
