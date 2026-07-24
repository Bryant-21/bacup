import threading
from pathlib import Path
from types import SimpleNamespace

from bacup_lib.regen_pipeline import (
    _clean_forced_regen_output,
    RegenOptions,
    RegenPaths,
)
from bacup_lib.lod_settings import PROFILE_HIGH_QUALITY
from bacup_ui.conversion.panels.regen_panel import (
    _COMPANION_MOD_NAME,
    _DiskSpaceVolume,
    _LOOSE_WORKSPACE_PEAK_BYTES,
    _PACKED_MOD_PEAK_BYTES,
    _PHASE_ROWS,
    _RECOVERY_PHASE_LABELS,
    _RECOVERY_PHASE_VALUES,
    _mod_archive_sizes,
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
    p._disk_usage_cache_key = None
    p._disk_space_cache = None
    p._disk_usage_lock = threading.Lock()
    p._disk_usage_running = False
    p._disk_usage_thread = None
    p._waiting_for_space_check = False
    p._low_space_warning = None
    p.ba2_target = "auto"
    p._ba2_detect_cache = None
    p._steam_install_cache = {}
    p._preflight_report = None
    p._preflight_cache = None
    return p


def test_recovery_menu_covers_every_displayed_conversion_stage():
    assert len(_RECOVERY_PHASE_VALUES) == len(_RECOVERY_PHASE_LABELS)
    assert [label.split(" (", 1)[0] for label in _RECOVERY_PHASE_LABELS] == [
        label for _slug, label in _PHASE_ROWS
    ]


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


def test_generate_precombines_toggle_defaults_off_and_maps_to_options():
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    # _panel() (via __new__) never sets the attr — build_options must still
    # default it off through the getattr guard.
    assert panel.build_options().generate_precombines is False

    panel.generate_precombines = True
    assert panel.build_options().generate_precombines is True


def test_generate_precombines_toggle_loads_from_workspace_settings():
    default = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    assert default.generate_precombines is False

    enabled = RegenPanel(
        _ws("C:/FO4", "C:/FO76", "C:/x/fo76", {"generate_precombines": True})
    )
    assert enabled.generate_precombines is True
    assert enabled.build_options().generate_precombines is True


def test_min_eligible_refs_is_not_a_panel_or_options_control():
    # Advanced precombine tuning stays config-file only; it must never surface as
    # a panel attribute or a RegenOptions field.
    panel = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    assert not hasattr(panel, "min_eligible_refs")
    assert not hasattr(panel.build_options(), "min_eligible_refs")


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


def test_disk_usage_archive_check_does_not_block_first_draw(monkeypatch):
    started = threading.Event()
    release = threading.Event()

    def slow_archive_sizes(*_args):
        started.set()
        release.wait(2.0)
        return 5, 5

    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._mod_archive_sizes",
        slow_archive_sizes,
    )

    summary = panel.disk_usage_summary()

    assert summary == {
        "extracted": 0,
        "mod_output": 0,
        "mod_ba2": 0,
        "deployed_ba2": 0,
    }
    assert started.wait(1.0)
    assert panel.disk_usage_loading() is True
    assert panel._disk_space_projection() is None

    release.set()
    assert panel._disk_usage_thread is not None
    panel._disk_usage_thread.join(timeout=2.0)

    assert panel.disk_usage_loading() is False
    assert panel.disk_usage_summary() == {
        "extracted": 0,
        "mod_output": 5,
        "mod_ba2": 5,
        "deployed_ba2": 5,
    }


def test_disk_space_projection_groups_requirements_on_same_volume():
    usage = SimpleNamespace(total=500 * 1024**3, used=100 * 1024**3, free=400 * 1024**3)

    volumes = _project_disk_space(
        output_root=Path("C:/BACUP/mods/SeventySix"),
        archive_root=Path("C:/Fallout4/Data"),
        volume_key=lambda _path: "c:",
        disk_usage=lambda _path: usage,
    )

    assert len(volumes) == 1
    assert volumes[0].required_bytes == (
        _LOOSE_WORKSPACE_PEAK_BYTES + _PACKED_MOD_PEAK_BYTES
    )
    assert volumes[0].labels == ("loose workspace", "packed BA2s")


def test_disk_space_projection_splits_direct_deploy_across_volumes():
    usages = {
        "c:": SimpleNamespace(total=250 * 1024**3, used=100, free=150 * 1024**3),
        "n:": SimpleNamespace(total=100 * 1024**3, used=50, free=50 * 1024**3),
    }

    volumes = _project_disk_space(
        output_root=Path("C:/BACUP/mods/SeventySix"),
        archive_root=Path("N:/MO2/mods/SeventySix"),
        volume_key=lambda path: f"{str(path)[0].lower()}:",
        disk_usage=lambda path: usages[f"{str(path)[0].lower()}:"],
    )

    by_key = {volume.key: volume for volume in volumes}
    assert by_key["c:"].required_bytes == _LOOSE_WORKSPACE_PEAK_BYTES
    assert by_key["c:"].insufficient is False
    assert by_key["n:"].required_bytes == _PACKED_MOD_PEAK_BYTES
    assert by_key["n:"].insufficient is True


def test_disk_space_level_reflects_projected_capacity():
    gib = 1024**3

    def volume(*, free_gib: int) -> _DiskSpaceVolume:
        return _DiskSpaceVolume(
            key="c:",
            path=Path("C:/"),
            labels=("conversion",),
            required_bytes=10 * gib,
            total_bytes=100 * gib,
            free_bytes=free_gib * gib,
        )

    assert volume(free_gib=50).space_level == "green"
    assert volume(free_gib=30).space_level == "green"
    assert volume(free_gib=25).space_level == "yellow"
    assert volume(free_gib=15).space_level == "yellow"
    assert volume(free_gib=5).space_level == "red"
    assert volume(free_gib=-1).space_level == "yellow"

    large_drive = _DiskSpaceVolume(
        key="d:",
        path=Path("D:/"),
        labels=("conversion",),
        required_bytes=180 * gib,
        total_bytes=4_000 * gib,
        free_bytes=881 * gib,
    )
    assert large_drive.space_level == "green"


def test_mod_archive_sizes_filter_other_mods(tmp_path):
    output_root = tmp_path / "SeventySix"
    deploy_root = tmp_path / "Data"
    output_root.mkdir()
    deploy_root.mkdir()
    (output_root / "SeventySix - Meshes.ba2").write_bytes(b"a" * 10)
    (output_root / "OtherMod - Meshes.ba2").write_bytes(b"b" * 50)
    (deploy_root / "SeventySix - Textures.ba2").write_bytes(b"c" * 20)
    (deploy_root / "Fallout4 - Textures.ba2").write_bytes(b"d" * 100)

    local_bytes, deployed_bytes = _mod_archive_sizes(
        output_root,
        deploy_root,
        "SeventySix",
    )

    assert local_bytes == 10
    assert deployed_bytes == 20


def test_mod_archive_sizes_deduplicate_same_root(tmp_path):
    output_root = tmp_path / "SeventySix"
    output_root.mkdir()
    (output_root / "SeventySix - Meshes.ba2").write_bytes(b"a" * 10)

    local_bytes, deployed_bytes = _mod_archive_sizes(
        output_root,
        output_root,
        "SeventySix",
    )

    assert local_bytes == 10
    assert deployed_bytes == 0


def test_disk_usage_summary_counts_only_named_mod_once(tmp_path):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    archive_root = tmp_path / "shared"
    archive_root.mkdir()
    (archive_root / "CurrentMod - Meshes.ba2").write_bytes(b"a" * 10)
    (archive_root / "SeventySix - Textures.ba2").write_bytes(b"b" * 20)
    (archive_root / "Fallout4 - Textures.ba2").write_bytes(b"c" * 30)
    paths = panel.build_paths()
    paths.output_root = archive_root
    paths.deploy_data_dir = archive_root
    paths.mod_name = "CurrentMod"
    summary = panel._compute_disk_usage_summary(paths)

    assert summary["extracted"] == 0
    assert summary["mod_output"] == 10
    assert summary["mod_ba2"] == 10
    assert summary["deployed_ba2"] == 0
    assert summary["mod_ba2"] + summary["deployed_ba2"] == 10


def test_panel_projection_uses_direct_deploy_root_and_measured_larger_sizes(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    captured = {}
    paths = SimpleNamespace(
        output_root=Path("C:/BACUP/mods/SeventySix"),
        deploy_data_dir=Path("N:/MO2/mods/SeventySix"),
        target_data_dir=Path("D:/Fallout4/Data"),
    )
    options = SimpleNamespace(deploy=True, direct_deploy_archives=True)
    panel._disk_usage_cache = (
        0.0,
        {
            "extracted": 0,
            "mod_output": 200 * 1024**3,
            "mod_ba2": 20 * 1024**3,
            "deployed_ba2": 75 * 1024**3,
        },
    )
    panel._disk_usage_cache_key = panel._disk_space_target(paths, options)[0]
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._project_disk_space",
        lambda **kwargs: captured.update(kwargs) or (),
    )

    projection = panel._disk_space_projection(
        paths=paths,
        options=options,
    )
    assert projection is None
    assert panel._disk_usage_thread is not None
    panel._disk_usage_thread.join(timeout=2.0)

    assert captured["archive_root"] == Path("N:/MO2/mods/SeventySix")
    assert captured["loose_bytes"] == 180 * 1024**3
    assert captured["packed_bytes"] == 75 * 1024**3


def test_changed_deploy_root_recomputes_disk_usage_summary(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    old_paths = panel.build_paths()
    old_paths.deploy_data_dir = Path("N:/MO2/mods/SeventySix")
    new_paths = panel.build_paths()
    new_paths.deploy_data_dir = Path("M:/MO2/mods/SeventySix")
    old_key = panel._disk_space_target(old_paths)[0]
    new_key = panel._disk_space_target(new_paths)[0]
    old_summary = {
        "extracted": 1,
        "mod_output": 2,
        "mod_ba2": 3,
        "deployed_ba2": 4,
    }
    new_summary = {
        "extracted": 5,
        "mod_output": 6,
        "mod_ba2": 7,
        "deployed_ba2": 8,
    }
    scans = []
    panel._disk_usage_cache = (0.0, old_summary)
    panel._disk_usage_cache_key = old_key
    panel._disk_space_cache = (old_key, ())
    monkeypatch.setattr(panel, "build_paths", lambda: new_paths)
    monkeypatch.setattr(
        panel,
        "_compute_disk_usage_summary",
        lambda paths=None: scans.append(paths) or new_summary,
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._project_disk_space",
        lambda **_kwargs: (),
    )

    assert panel.disk_usage_summary() == {
        "extracted": 0,
        "mod_output": 0,
        "mod_ba2": 0,
        "deployed_ba2": 0,
    }
    assert panel._disk_usage_thread is not None
    panel._disk_usage_thread.join(timeout=2.0)

    assert scans == [new_paths]
    assert panel._disk_usage_cache_key == new_key
    assert panel.disk_usage_summary() == new_summary


def test_projection_reads_drive_capacity_only_in_background(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    paths = panel.build_paths()
    panel._disk_usage_cache = (
        0.0,
        {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0},
    )
    panel._disk_usage_cache_key = panel._disk_space_target(paths)[0]
    started = threading.Event()
    release = threading.Event()
    ui_thread = threading.get_ident()

    def slow_disk_usage(_path):
        assert threading.get_ident() != ui_thread
        started.set()
        release.wait(2.0)
        return SimpleNamespace(total=500 * 1024**3, used=50 * 1024**3, free=450 * 1024**3)

    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._disk_usage_for_target",
        slow_disk_usage,
    )

    assert panel._disk_space_projection() is None
    assert started.wait(1.0)
    assert panel._disk_space_projection() is None

    release.set()
    assert panel._disk_usage_thread is not None
    panel._disk_usage_thread.join(timeout=2.0)
    projection = panel._disk_space_projection()

    assert projection is not None
    assert len(projection) == 2


def test_request_conversion_never_blocks_on_drive_capacity(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    paths = panel.build_paths()
    space_key = panel._disk_space_target(paths)[0]
    summary = {
        "extracted": 0,
        "mod_output": 0,
        "mod_ba2": 0,
        "deployed_ba2": 0,
    }
    old_projection = _project_disk_space(
        output_root=paths.output_root,
        archive_root=paths.target_data_dir,
        disk_usage=lambda _path: SimpleNamespace(
            total=500 * 1024**3,
            used=50 * 1024**3,
            free=450 * 1024**3,
        ),
    )
    panel._disk_usage_cache = (0.0, summary)
    panel._disk_usage_cache_key = space_key
    panel._disk_space_cache = (space_key, old_projection)
    started = threading.Event()
    release = threading.Event()
    ui_thread = threading.get_ident()
    starts = []
    capacity_reads = []

    def slow_disk_usage(_path):
        assert threading.get_ident() != ui_thread
        capacity_reads.append(_path)
        started.set()
        release.wait(2.0)
        return SimpleNamespace(
            total=500 * 1024**3,
            used=50 * 1024**3,
            free=450 * 1024**3,
        )

    monkeypatch.setattr(
        panel,
        "_compute_disk_usage_summary",
        lambda _paths=None: (_ for _ in ()).throw(
            AssertionError("fresh capacity check must reuse the size summary")
        ),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._disk_usage_for_target",
        slow_disk_usage,
    )
    monkeypatch.setattr(panel, "start_conversion", lambda: starts.append(True))

    panel._request_conversion()

    assert panel._waiting_for_space_check is True
    assert starts == []
    assert started.wait(1.0)
    assert panel._disk_space_cache is None

    release.set()
    assert panel._disk_usage_thread is not None
    panel._disk_usage_thread.join(timeout=2.0)
    panel._resolve_pending_space_check()

    assert capacity_reads
    assert panel._disk_space_cache is not None
    assert panel._disk_space_cache[1] is not old_projection
    assert starts == [True]


def test_request_conversion_waits_for_background_space_measurement(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    starts = []
    monkeypatch.setattr(panel, "_start_disk_usage_worker", lambda **_kwargs: None)
    monkeypatch.setattr(panel, "_disk_space_projection", lambda: ())
    monkeypatch.setattr(panel, "start_conversion", lambda: starts.append(True))

    panel._request_conversion()

    assert panel._waiting_for_space_check is True
    assert starts == []

    panel._resolve_pending_space_check()

    assert panel._waiting_for_space_check is False
    assert starts == [True]


def test_request_conversion_warns_before_start_and_can_continue(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    low_volume = _project_disk_space(
        output_root=Path("C:/BACUP/mods/SeventySix"),
        archive_root=Path("C:/Fallout4/Data"),
        volume_key=lambda _path: "c:",
        disk_usage=lambda _path: SimpleNamespace(total=200 * 1024**3, used=190 * 1024**3, free=10 * 1024**3),
    )[0]
    starts = []
    monkeypatch.setattr(panel, "_start_disk_usage_worker", lambda **_kwargs: None)
    monkeypatch.setattr(panel, "_disk_space_projection", lambda: (low_volume,))
    monkeypatch.setattr(panel, "start_conversion", lambda: starts.append(True))

    panel._request_conversion()

    assert panel._waiting_for_space_check is True
    assert starts == []

    panel._resolve_pending_space_check()

    assert panel._low_space_warning == (low_volume,)
    assert starts == []

    panel._continue_conversion_with_low_space()

    assert panel._low_space_warning is None
    assert starts == [True]


def test_non_fo76_pair_skips_fo76_space_estimate(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.pair_id = "skyrimse:fo4"
    starts = []
    monkeypatch.setattr(
        panel,
        "_disk_space_projection",
        lambda: (_ for _ in ()).throw(AssertionError("FO76 estimate must not run")),
    )
    monkeypatch.setattr(panel, "start_conversion", lambda: starts.append(True))

    panel._request_conversion()

    assert starts == [True]


def test_request_conversion_starts_after_fresh_check_with_enough_space(monkeypatch):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    enough_volume = _project_disk_space(
        output_root=Path("C:/BACUP/mods/SeventySix"),
        archive_root=Path("N:/MO2/mods/SeventySix"),
        volume_key=lambda _path: "c:",
        disk_usage=lambda _path: SimpleNamespace(total=500 * 1024**3, used=50 * 1024**3, free=450 * 1024**3),
    )[0]
    starts = []
    monkeypatch.setattr(panel, "_start_disk_usage_worker", lambda **_kwargs: None)
    monkeypatch.setattr(panel, "_disk_space_projection", lambda: (enough_volume,))
    monkeypatch.setattr(panel, "start_conversion", lambda: starts.append(True))

    panel._request_conversion()

    assert panel._waiting_for_space_check is True
    assert starts == []

    panel._resolve_pending_space_check()

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
