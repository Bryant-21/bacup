import threading
from pathlib import Path
from types import SimpleNamespace

from bacup_lib.regen_pipeline import RegenOptions, RegenPaths
from bacup_lib.lod_settings import PROFILE_HIGH_QUALITY
from bacup_ui.conversion.panels.regen_panel import (
    _COMPANION_MOD_NAME,
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
    p.mo2_use_profile_ini = True
    p.deploy = True
    p.add_archives_to_ini = True
    p.deploy_data_dir = ""
    p._install_audit = None
    p._install_audit_error = None
    p.archive_max_gb = 4
    p.workers = 0
    p.lod_mode = "hybrid-atlas"
    p.lod_profile = PROFILE_HIGH_QUALITY
    p.atlas_mip_flooding = False
    p.texture_landscape_mip_flooding = False
    p.re_use_land = False
    p.recovery_phase = "lodgen"
    p._phases = []
    p._summary = None
    p._completion = None
    p._disk_usage_cache = None
    p._disk_usage_lock = threading.Lock()
    p._disk_usage_running = False
    p._disk_usage_thread = None
    p.ba2_target = "auto"
    p._ba2_detect_cache = None
    p._steam_install_cache = {}
    p._preflight_report = None
    p._preflight_cache = None
    return p


def test_build_paths_from_settings(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    paths = panel.build_paths()
    assert isinstance(paths, RegenPaths)
    assert paths.target_data_dir == Path("C:/FO4/Data")
    assert paths.source_extracted_dir == Path("C:/x/fo76")
    assert paths.source_data_dir == Path("C:/FO76/Data")
    assert paths.target_ck_ini_path == Path("C:/FO4/CreationKitCustom.ini")
    assert paths.output_root == Path("X:/app/mods/SeventySix")
    assert paths.deploy_data_dir is None


def test_build_paths_game_mode_deploys_to_fo4_data_and_docs_ini(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    # Default install_location "game": no virtual deploy dir (None sentinel = FO4
    # Data), archives register in the Documents Fallout4Custom.ini.
    paths = panel.build_paths()
    docs = Path.home() / "Documents" / "My Games" / "Fallout4"
    assert paths.output_root == Path("X:/app/mods/SeventySix")
    assert paths.target_data_dir == Path("C:/FO4/Data")
    assert paths.deploy_data_dir is None
    assert paths.runtime_ini_path == docs / "Fallout4Custom.ini"


def test_build_paths_treats_default_deploy_folder_as_standard_fo4_deploy(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.deploy_data_dir = "C:/FO4/Data"
    paths = panel.build_paths()
    assert paths.deploy_data_dir is None


def test_build_paths_sets_resource_dir_from_get_resource_dir(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_resource_dir",
        lambda: Path("X:/app/_internal/resource"),
    )
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    paths = panel.build_paths()
    assert paths.resource_dir == Path("X:/app/_internal/resource")


def test_build_options_reflects_controls():
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = "none"
    panel.ba2_mode = "packed"
    panel.archive_max_gb = 6
    panel.add_archives_to_ini = False
    panel.workers = 6
    panel.include_interior = False
    panel.records_limit = 2000
    opts = panel.build_options()
    assert isinstance(opts, RegenOptions)
    assert opts.deploy is False
    assert opts.ba2_mode == "expanded"
    assert opts.archive_max_bytes == 6 * 1024**3
    assert opts.workers == 6
    assert opts.include_interior is True
    assert opts.records_limit is None
    assert opts.generate_anim_text_data is True
    assert opts.anim_text_data_native is True
    assert opts.direct_deploy_archives is True
    assert opts.update_runtime_ini is False
    assert opts.write_land_cache is False
    assert opts.texture_landscape_mip_flooding is False


def test_build_options_zero_means_unset():
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.workers = 0
    opts = panel.build_options()
    assert opts.workers is None
    assert opts.include_interior is True
    assert opts.records_limit is None


def test_panel_defaults_to_expanded_high_quality_atlas_generation():
    ws = _ws("C:/FO4", "C:/FO76", "C:/x/fo76")
    panel = RegenPanel(ws)
    assert panel.add_archives_to_ini is True
    assert panel.lod_mode == "hybrid-atlas"
    assert panel.lod_profile == PROFILE_HIGH_QUALITY
    assert panel.atlas_mip_flooding is False
    assert panel.texture_landscape_mip_flooding is False
    assert panel.recovery_phase == "lodgen"
    opts = panel.build_options()
    assert opts.generate_anim_text_data is True
    assert opts.anim_text_data_native is True
    assert opts.direct_deploy_archives is True
    assert opts.update_runtime_ini is True
    assert opts.write_land_cache is False
    assert opts.include_interior is True
    assert opts.records_limit is None
    assert opts.archive_max_bytes == 4 * 1024**3


def test_panel_loads_saved_install_location_and_archive_size(monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    ws = _ws(
        "C:/FO4",
        "C:/FO76",
        "C:/x/fo76",
        {
            "install_location": "vortex",
            "install_path": "D:/Vortex/fallout4/mods/SeventySix",
            "archive_max_gb": 8,
            "recovery_phase": "textures",
            "atlas_mip_flooding": True,
            "texture_landscape_mip_flooding": True,
        },
    )
    panel = RegenPanel(ws)
    assert panel.install_location == "vortex"
    assert panel.install_path == "D:/Vortex/fallout4/mods/SeventySix"
    assert panel.archive_max_gb == 8
    assert panel.recovery_phase == "textures"
    assert panel.atlas_mip_flooding is True
    assert panel.texture_landscape_mip_flooding is True
    assert panel.build_options().texture_landscape_mip_flooding is True
    # A saved install_path now drives the deploy target (was previously ignored).
    paths = panel.build_paths()
    assert paths.deploy_data_dir == Path("D:/Vortex/fallout4/mods/SeventySix")


def test_selected_lod_settings_applies_mip_flooding_override(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.atlas_mip_flooding = True
    monkeypatch.setattr(
        panel,
        "load_lod_settings",
        lambda _profile, _lod_mode: {"objects": {"source": "fo76_bto_atlas"}},
    )

    settings = panel._selected_lod_settings("hybrid-atlas")

    assert settings["objects"]["source"] == "fo76_bto_atlas"
    assert settings["objects"]["atlas_mip_flooding"] is True


def test_disk_usage_scan_does_not_block_first_draw(monkeypatch):
    started = threading.Event()
    release = threading.Event()

    def slow_dir_size(_path):
        started.set()
        release.wait(2.0)
        return 10

    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel._dir_size_bytes", slow_dir_size)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel._ba2_size_bytes", lambda _p: 5)

    summary = panel.disk_usage_summary()

    assert summary == {
        "extracted": 0,
        "mod_output": 0,
        "mod_ba2": 0,
        "deployed_ba2": 0,
    }
    assert started.wait(1.0)
    assert panel.disk_usage_loading() is True

    release.set()
    assert panel._disk_usage_thread is not None
    panel._disk_usage_thread.join(timeout=2.0)

    assert panel.disk_usage_loading() is False
    assert panel.disk_usage_summary() == {
        "extracted": 20,
        "mod_output": 10,
        "mod_ba2": 5,
        "deployed_ba2": 5,
    }


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
    fo4_root = tmp_path / "Fallout4"
    fo4_data = fo4_root / "Data"

    (companion / "data" / "Scripts" / "B21").mkdir(parents=True)
    (companion / "data" / "Meshes" / "B21").mkdir(parents=True)
    (companion / "PrismaUI_F4" / "views" / "B21_FullScreenMap").mkdir(parents=True)
    (companion / f"{_COMPANION_MOD_NAME}.esp").write_bytes(b"esp")
    (companion / "data" / "Scripts" / "B21" / "B21_AT_TeleportSign.pex").write_bytes(b"pex")
    (companion / "data" / "Meshes" / "B21" / "marker.nif").write_bytes(b"nif")
    (companion / "PrismaUI_F4" / "views" / "B21_FullScreenMap" / "index.html").write_text(
        "<html></html>",
        encoding="utf-8",
    )
    (fo4_data / "Scripts" / "B21").mkdir(parents=True)
    (fo4_data / "Meshes" / "B21").mkdir(parents=True)
    (fo4_data / "Scripts" / "B21" / "B21_AT_TeleportSign.pex").write_bytes(b"stale")
    (fo4_data / "Meshes" / "B21" / "marker.nif").write_bytes(b"stale")

    panel = _panel(_ws(str(fo4_root), "C:/FO76", "C:/x/fo76"))
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

    assert f"{_COMPANION_MOD_NAME}.esp" in deployed
    assert f"{_COMPANION_MOD_NAME} - Main.ba2" in deployed
    assert "PrismaUI_F4/views/B21_FullScreenMap/index.html" in deployed
    assert (fo4_data / f"{_COMPANION_MOD_NAME}.esp").read_bytes() == b"esp"
    assert (fo4_data / f"{_COMPANION_MOD_NAME} - Main.ba2").read_bytes() == b"ba2"
    assert (fo4_data / "Scripts" / "B21" / "B21_AT_TeleportSign.pex").read_bytes() == b"stale"
    assert (fo4_data / "Meshes" / "B21" / "marker.nif").read_bytes() == b"stale"
    assert not (fo4_data / "F4SE").exists()
    assert (fo4_data / "PrismaUI_F4" / "views" / "B21_FullScreenMap" / "index.html").is_file()
    assert pack_calls == [
        (
            _COMPANION_MOD_NAME,
            {
                "game": "fo4",
                "project_root": exe_dir,
                "archive_max_bytes": panel.archive_max_gb * 1024**3,
                "archive_workers": panel.workers,
            },
        )
    ]
    assert logs and logs[-1][1].startswith("Companion mod")


def test_resolve_ba2_target_auto_uses_detection(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.ba2_target = "auto"
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.detect_ba2_target",
        lambda _root, **_kw: ("og", (1, 10, 163, 0)),
    )
    assert panel.resolve_ba2_target() == "og"
    assert panel.build_options().fo4_ba2_target == "og"


def test_resolve_ba2_target_manual_override(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.ba2_target = "nextgen"
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.detect_ba2_target",
        lambda _root, **_kw: ("og", (1, 10, 163, 0)),
    )
    assert panel.resolve_ba2_target() == "nextgen"
    assert panel.build_options().fo4_ba2_target == "nextgen"


def test_ba2_target_persisted_default_is_auto():
    ws = _ws("C:/FO4", "C:/FO76", "C:/x/fo76", {"ba2_target": "og"})
    panel = RegenPanel(ws)
    assert panel.ba2_target == "og"
