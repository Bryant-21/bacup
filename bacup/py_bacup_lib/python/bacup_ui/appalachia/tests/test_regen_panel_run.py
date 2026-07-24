import threading
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from bacup_lib.input_preflight import InputPreflightReport, MissingInput
from bacup_lib.regen_pipeline import RegenResult
from bacup_lib.lod_settings import PROFILE_HIGH_QUALITY
from bacup_ui.conversion.panels.regen_panel import RegenPanel
from ui.toolkit.steam_install import SteamInstallResult


def _ok_steam_install(
    *,
    game_id: str = "fo4",
    app_id: int = 377160,
    root: str = "C:/FO4",
    name: str = "Fallout 4",
) -> SteamInstallResult:
    return SteamInstallResult(
        ok=True,
        game_id=game_id,
        app_id=app_id,
        root_dir=root,
        local_install_valid=True,
        steam_layout_valid=True,
        steam_api_present=True,
        appmanifest_present=True,
        appmanifest_matches=True,
        steam_library_dir="C:/SteamLibrary",
        appmanifest_path=f"C:/SteamLibrary/steamapps/appmanifest_{app_id}.acf",
        message=f"{name} Steam install verified.",
    )


def _panel():
    ws = SimpleNamespace(
        _toolkit_settings=SimpleNamespace(
            get_game_paths=lambda g: {
                "root_dir": "C:/FO4" if g == "fo4" else "C:/FO76",
                "extracted_dir": ("C:/x/fo4" if g == "fo4" else "C:/x/fo76"),
            },
            get_workspace_settings=lambda _w: {},
        ),
        _runner=None,
    )
    panel = RegenPanel(ws)
    panel._steam_install_cache = {
        "fo4": ("C:/FO4", _ok_steam_install()),
        "fo76": (
            "C:/FO76",
            _ok_steam_install(
                game_id="fo76",
                app_id=1151340,
                root="C:/FO76",
                name="Fallout 76",
            ),
        ),
    }
    return panel, ws


def test_start_conversion_invokes_run_full_regen_with_built_args():
    panel, ws = _panel()
    panel.install_location = "none"
    captured = {}

    class FakeRunner:
        def __init__(self, work):
            self._work = work

        def start(self):
            self._work(self)

        def emit_complete(self, mod_path, summary):
            captured["complete"] = (mod_path, summary)

        def emit_log(self, *a):
            pass

        def emit_phase_start(self, progress):
            captured.setdefault("pre_conversion", []).append(
                ("start", SimpleNamespace(**vars(progress)))
            )

        def emit_item_progress(self, progress):
            captured.setdefault("pre_conversion", []).append(
                ("item", SimpleNamespace(**vars(progress)))
            )

        def emit_phase_complete(self, progress):
            captured.setdefault("pre_conversion", []).append(
                ("complete", SimpleNamespace(**vars(progress)))
            )

        def is_cancelled(self):
            return False

    def fake_run(paths, options, *, phases, runner, **kw):
        captured["paths"] = paths
        captured["options"] = options
        captured["lod_settings"] = kw.get("lod_settings")
        return RegenResult(
            exit_code=0,
            output_root=Path("X:/app/mods/SeventySix"),
            elapsed_seconds=1.0,
            deployed=False,
            failures=[],
            warnings=[],
        )

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", FakeRunner), patch(
        "bacup_lib.regen_pipeline.run_full_regen", fake_run
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel.load_lod_settings",
        lambda self, profile, lod_mode: {
            "global": {"worldspaces": ["APPALACHIA"]},
            "profile": profile,
            "mode": lod_mode,
        },
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")), patch(
        "bacup_ui.conversion.panels.regen_panel.scan_conversion_inputs",
        lambda *_a, **_k: InputPreflightReport(),
    ):
        panel.start_conversion()

    assert captured["options"].deploy is False
    assert captured["options"].lod_mode == "hybrid-atlas"
    assert captured["options"].archive_max_bytes == 4 * 1024**3
    assert captured["options"].include_interior is True
    assert captured["options"].records_limit is None
    assert captured["options"].direct_deploy_archives is True
    assert captured["options"].update_runtime_ini is True
    assert captured["options"].write_land_cache is False
    assert captured["lod_settings"] == {
        "global": {"worldspaces": ["APPALACHIA"]},
        "profile": PROFILE_HIGH_QUALITY,
        "mode": "hybrid-atlas",
        "objects": {"atlas_mip_flooding": False},
    }
    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    assert Path(captured["complete"][0]) == Path("X:/app/mods/SeventySix")
    pre_conversion = captured["pre_conversion"]
    assert pre_conversion[0][0] == "start"
    assert pre_conversion[0][1].phase_name == "Prepare Conversion"
    assert pre_conversion[0][1].total_items == 4
    assert [event[1].completed_items for event in pre_conversion[1:-1]] == [1, 2, 3, 4]
    assert pre_conversion[-1][0] == "complete"
    assert pre_conversion[-1][1].status == "completed"


def test_start_conversion_deploys_companion_after_main_deploy():
    panel, _ = _panel()
    captured = {}

    class FakeRunner:
        def __init__(self, work):
            self._work = work

        def start(self):
            self._work(self)

        def emit_complete(self, mod_path, summary):
            captured["complete"] = (mod_path, summary)

        def emit_log(self, *a):
            pass

        def emit_phase_start(self, progress):
            pass

        def emit_item_progress(self, progress):
            pass

        def emit_phase_complete(self, progress):
            pass

        def is_cancelled(self):
            return False

    def fake_run(paths, options, *, phases, runner, **kw):
        captured["run_done"] = True
        return RegenResult(
            exit_code=0,
            output_root=Path("X:/app/mods/SeventySix"),
            elapsed_seconds=1.0,
            deployed=True,
            failures=[],
            warnings=[],
        )

    def fake_deploy_companion(self, paths, runner):
        assert captured["run_done"] is True
        captured["companion_paths"] = paths
        captured["companion_runner"] = runner
        return ["B21_TalesFromAppalachia.esp"]

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", FakeRunner), patch(
        "bacup_lib.regen_pipeline.run_full_regen", fake_run
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel.load_lod_settings",
        lambda self, profile, lod_mode: {},
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._deploy_companion_mod",
        fake_deploy_companion,
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")), patch(
        "bacup_ui.conversion.panels.regen_panel.scan_conversion_inputs",
        lambda *_a, **_k: InputPreflightReport(),
    ):
        panel.start_conversion()

    assert captured["companion_paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["complete"][1]["companion_deployed"] == ["B21_TalesFromAppalachia.esp"]


def test_start_deploy_existing_deploys_generated_and_companion():
    panel, _ = _panel()
    panel.deploy_data_dir = "D:/MO2/mods/SeventySix"
    captured = {}

    class FakeRunner:
        def __init__(self, work):
            self._work = work

        def start(self):
            self._work(self)

        def emit_complete(self, mod_path, summary):
            captured["complete"] = (mod_path, summary)

        def emit_log(self, *a):
            pass

        def is_cancelled(self):
            return False

    def fake_deploy_existing(paths, *, update_runtime_ini):
        captured["paths"] = paths
        captured["update_runtime_ini"] = update_runtime_ini
        return RegenResult(
            exit_code=0,
            output_root=Path("X:/app/mods/SeventySix"),
            elapsed_seconds=1.0,
            deployed=True,
            failures=[],
            warnings=[],
        )

    def fake_deploy_companion(self, paths, runner):
        captured["companion_paths"] = paths
        captured["companion_runner"] = runner
        return ["B21_TalesFromAppalachia.esp"]

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", FakeRunner), patch(
        "bacup_lib.regen_pipeline.deploy_existing",
        fake_deploy_existing,
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._deploy_companion_mod",
        fake_deploy_companion,
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")):
        panel.start_deploy_existing()

    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["paths"].deploy_data_dir is None
    assert captured["update_runtime_ini"] is True
    assert captured["companion_paths"].deploy_data_dir is None
    assert captured["complete"][1]["deploy_existing"] is True
    assert captured["complete"][1]["companion_deployed"] == ["B21_TalesFromAppalachia.esp"]


def test_start_resume_from_phase_deploys_generated_and_companion():
    panel, _ = _panel()
    panel.recovery_phase = "lodgen"
    captured = {}

    class FakeRunner:
        def __init__(self, work):
            self._work = work

        def start(self):
            self._work(self)

        def emit_complete(self, mod_path, summary):
            captured["complete"] = (mod_path, summary)

        def emit_log(self, *a):
            pass

        def is_cancelled(self):
            return False

    def fake_resume(paths, options, *, start_phase, phases, runner, **kw):
        captured["paths"] = paths
        captured["options"] = options
        captured["start_phase"] = start_phase
        captured["phases"] = phases
        captured["lod_settings"] = kw.get("lod_settings")
        return RegenResult(
            exit_code=0,
            output_root=Path("X:/app/mods/SeventySix"),
            elapsed_seconds=1.0,
            deployed=True,
            failures=[],
            warnings=[],
        )

    def fake_deploy_companion(self, paths, runner):
        captured["companion_paths"] = paths
        captured["companion_runner"] = runner
        return ["B21_TalesFromAppalachia.esp"]

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", FakeRunner), patch(
        "bacup_lib.regen_pipeline.run_resume_from_phase", fake_resume
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel.load_lod_settings",
        lambda self, profile, lod_mode: {
            "global": {"worldspaces": ["APPALACHIA"]},
            "profile": profile,
            "mode": lod_mode,
        },
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._deploy_companion_mod",
        fake_deploy_companion,
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")):
        panel.start_resume_from_phase()

    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["options"].direct_deploy_archives is True
    assert captured["start_phase"] == "lodgen"
    assert captured["phases"].lod_mode == "hybrid-atlas"
    assert captured["lod_settings"] == {
        "global": {"worldspaces": ["APPALACHIA"]},
        "profile": PROFILE_HIGH_QUALITY,
        "mode": "hybrid-atlas",
        "objects": {"atlas_mip_flooding": False},
    }
    assert captured["companion_paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["complete"][1]["resume_from"] == "lodgen"
    assert captured["complete"][1]["companion_deployed"] == ["B21_TalesFromAppalachia.esp"]


def test_handle_complete_sets_completion_for_non_deploy():
    panel, _ = _panel()
    panel.handle_event(
        {
            "type": "complete",
            "mod_path": "X:/app/mods/SeventySix",
            "summary": {"deployed": False},
        }
    )
    assert panel._completion is not None
    assert panel._completion["mod_path"] == "X:/app/mods/SeventySix"
    assert panel._completion["deployed"] is False


def test_runner_progress_uses_phase_and_item_progress():
    fraction, message = RegenPanel._runner_progress(
        [
            {
                "ui_key": "translate_records",
                "phase_name": "Translate Records",
                "status": "completed",
            },
            {
                "ui_key": "convert_terrain",
                "phase_name": "Convert Terrain",
                "status": "running",
                "total_items": 10,
                "completed_items": 5,
                "current_item": "Meshes/Terrain/Appalachia/tile.bto",
            },
        ],
        [
            ("translate_records", "Translate Records"),
            ("convert_terrain", "Convert Terrain"),
            ("convert_nifs", "Convert NIFs"),
            ("build_esp", "Build ESP"),
        ],
    )

    assert fraction == 0.375
    assert message == "Convert Terrain: tile.bto"


def test_runner_progress_infers_previous_phase_slots_for_active_phase():
    fraction, message = RegenPanel._runner_progress(
        [
            {
                "ui_key": "build_esp",
                "phase_name": "Build ESP",
                "status": "running",
                "total_items": 0,
                "completed_items": 0,
            },
        ],
        [
            ("translate_records", "Translate Records"),
            ("convert_terrain", "Convert Terrain"),
            ("convert_nifs", "Convert NIFs"),
            ("build_esp", "Build ESP"),
        ],
    )

    assert fraction == 0.75
    assert message == "Build ESP"


def test_runner_progress_uses_specific_post_phase_status():
    fraction, message = RegenPanel._runner_progress(
        [
            {
                "ui_key": "pack",
                "phase_name": "Pack BA2",
                "status": "completed",
            }
        ],
        [("pack", "Pack BA2"), ("deploy", "Deploy Mod")],
        "Sanitizing generated plugin files",
    )

    assert fraction == 0.5
    assert message == "Sanitizing generated plugin files"


def test_handle_event_updates_specific_runner_status():
    panel, _ = _panel()

    panel.handle_event({"type": "status", "message": "Writing conversion reports"})

    assert panel._runner_status == "Writing conversion reports"


def test_handle_event_maps_native_asset_stage_to_visible_phase():
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {"phase": 0, "phase_name": "convert_nifs_v2", "status": "running"},
        }
    )
    panel.handle_event(
        {
            "type": "item_progress",
            "data": {
                "phase": 0,
                "phase_name": "convert_nifs_v2",
                "status": "running",
                "total_items": 10,
                "completed_items": 4,
                "current_item": "Meshes/Weapons/test.nif",
            },
        }
    )

    assert panel._phases == [
        {
            "phase": 0,
            "phase_name": "Convert NIFs",
            "status": "running",
            "ui_key": "convert_nifs",
            "total_items": 10,
            "completed_items": 4,
            "current_item": "Meshes/Weapons/test.nif",
        }
    ]


def test_late_material_progress_does_not_reopen_completed_phase():
    panel, _ = _panel()
    panel.handle_event(
        {
            "type": "phase_complete",
            "data": {
                "phase": 0,
                "phase_name": "convert_materials_v2",
                "status": "completed",
                "completed_items": 29_471,
                "total_items": 29_471,
            },
        }
    )

    panel.handle_event(
        {
            "type": "item_progress",
            "data": {
                "phase": 0,
                "phase_name": "convert_materials_v2",
                "status": "running",
                "completed_items": 29_461,
                "total_items": 29_471,
            },
        }
    )

    assert panel._phases[0]["status"] == "completed"
    assert panel._phases[0]["completed_items"] == 29_471


def test_handle_event_strips_implementation_language_from_phase_label():
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "phase": 2,
                "phase_name": "Translate Records (Rust)",
                "status": "running",
            },
        }
    )

    assert panel._phases[0]["phase_name"] == "Translate Records"
    assert panel._phases[0]["ui_key"] == "translate_records"


def test_animtext_phase_is_visible_before_lod():
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "phase": 0,
                "phase_name": "Generate AnimTextData",
                "status": "running",
                "current_item": "AnimationFileData: starting 2130 subgraph(s)",
            },
        }
    )

    assert panel._phases[0]["ui_key"] == "generate_anim_text_data"
    rows = panel._phase_rows(panel._phases)
    assert rows.index(("generate_anim_text_data", "Generate AnimTextData")) < rows.index(
        ("lodgen", "Generate LOD")
    )


def test_modt_phase_is_visible_between_build_and_animtext():
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "phase": 0,
                "phase_name": "Regenerate MODT",
                "status": "running",
                "current_item": "Building mesh manifest",
            },
        }
    )

    assert panel._phases[0]["ui_key"] == "regenerate_modt"
    rows = panel._phase_rows(panel._phases)
    assert rows.index(("build_esp", "Build ESP")) < rows.index(
        ("regenerate_modt", "Regenerate MODT")
    ) < rows.index(("generate_anim_text_data", "Generate AnimTextData"))


def test_generate_precombines_phase_tracked_by_generic_row_appending():
    # The experimental phase is intentionally NOT a static _PHASE_ROWS entry (that
    # would drag it into the recovery menu). The panel's generic phase tracking
    # must surface it dynamically once its events arrive.
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "phase": 0,
                "phase_name": "Generate precombines",
                "status": "running",
                "current_item": "Baking precombines",
            },
        }
    )

    assert panel._phases[0]["ui_key"] == "generate_precombines"
    rows = panel._phase_rows(panel._phases)
    assert ("generate_precombines", "Generate precombines") in rows


def test_cell_offsets_phase_is_visible_between_modt_and_animtext():
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "phase": 0,
                "phase_name": "Rebuild Cell Offsets",
                "status": "running",
                "current_item": "Rebuilding WRLD cell offset tables",
            },
        }
    )

    assert panel._phases[0]["ui_key"] == "rebuild_cell_offsets"
    rows = panel._phase_rows(panel._phases)
    assert rows.index(("regenerate_modt", "Regenerate MODT")) < rows.index(
        ("rebuild_cell_offsets", "Rebuild Cell Offsets")
    ) < rows.index(("generate_anim_text_data", "Generate AnimTextData"))


def test_draw_status_column_shows_non_modal_runner_status(monkeypatch):
    panel, ws = _panel()
    ws._runner = SimpleNamespace(done=False, cancel=lambda: None)
    panel._phases = [
        {
            "ui_key": "translate_records",
            "phase_name": "Translate Records",
            "status": "completed",
        },
        {
            "ui_key": "convert_terrain",
            "phase_name": "Convert Terrain",
            "status": "running",
            "total_items": 10,
            "completed_items": 5,
            "current_item": "Meshes/Terrain/Appalachia/tile.bto",
        },
    ]
    captured = {"status": [], "phase_progress": 0, "overlay": 0}

    monkeypatch.setattr(
        panel,
        "disk_usage_summary",
        lambda: {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0},
    )
    monkeypatch.setattr(panel, "disk_usage_loading", lambda: False)
    monkeypatch.setattr(
        "bacup_ui.conversion.widgets.draw_phase_progress",
        lambda *a, **kw: captured.update(
            {"phase_progress": captured["phase_progress"] + 1}
        ),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.widgets.draw_runner_overlay",
        lambda *a, **kw: captured.update(
            {"overlay": captured["overlay"] + 1}
        ),
    )

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.text",
        lambda message: captured["status"].append(message),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.text_disabled",
        lambda message: captured["status"].append(message),
    )
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.separator", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.checkbox", lambda _label, value: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.input_text", lambda _label, value: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.collapsing_header", lambda _label: False)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.slider_int", lambda _label, value, _min, _max: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.same_line", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.button", lambda _label: False)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_disabled", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.end_disabled", lambda: None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.get_style",
        lambda: SimpleNamespace(item_spacing=SimpleNamespace(y=4.0)),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.get_time", lambda: 0.0
    )

    panel._draw_status_column()

    assert captured == {
        "status": [
            "Converting Tales From Appalachia — 13%",
            "|  Convert Terrain: tile.bto",
        ],
        "phase_progress": 1,
        "overlay": 0,
    }


def test_draw_settings_column_balances_disabled_stack_when_convert_starts(monkeypatch):
    panel, ws = _panel()
    disabled_depth = 0

    monkeypatch.setattr(
        panel,
        "disk_usage_summary",
        lambda: {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0},
    )
    monkeypatch.setattr(panel, "disk_usage_loading", lambda: False)
    monkeypatch.setattr(
        panel,
        "start_conversion",
        lambda: setattr(ws, "_runner", SimpleNamespace(done=False, cancel=lambda: None)),
    )
    monkeypatch.setattr("bacup_ui.conversion.widgets.draw_phase_progress", lambda *a, **kw: None)
    monkeypatch.setattr("bacup_ui.conversion.widgets.draw_runner_overlay", lambda *a, **kw: None)

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.ImVec4", lambda *a: a)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.text", lambda *a, **kw: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.text_disabled", lambda *a, **kw: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.separator", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.text_colored", lambda *a: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.checkbox", lambda _label, value: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.input_text", lambda _label, value: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.collapsing_header", lambda *_a, **_k: False)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.slider_int", lambda _label, value, _min, _max: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.same_line", lambda: None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.button",
        lambda label, *_a: label.startswith("Convert Tales From Appalachia"),
    )

    def begin_disabled():
        nonlocal disabled_depth
        disabled_depth += 1

    def end_disabled():
        nonlocal disabled_depth
        if disabled_depth <= 0:
            raise RuntimeError("end_disabled without begin_disabled")
        disabled_depth -= 1

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_disabled", begin_disabled)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.end_disabled", end_disabled)

    panel._draw_settings_column()

    assert disabled_depth == 0


def test_maintenance_controls_remain_available_while_cleanup_is_measuring(monkeypatch):
    from imgui_bundle import imgui

    panel, _ws = _panel()
    panel.fixed_pair_id = "fo76:fo4"
    panel._cleanup_status = "scanning"
    panel._is_admin = False
    panel.disk_usage_summary = lambda: {
        "extracted": 0,
        "mod_output": 0,
        "mod_ba2": 0,
        "deployed_ba2": 0,
    }
    panel.disk_usage_loading = lambda: True
    panel._disk_space_projection = lambda **_kwargs: None
    panel._detect_ba2_target = lambda: ("og", "1.10.163")
    panel.resolve_ba2_target = lambda: "og"
    panel._deployed_esm_exists = lambda: False
    panel._load_upgrade_manifest_cached = lambda: None
    panel.can_convert = lambda: False
    panel.can_deploy_existing = lambda: False
    panel.generated_plugin_path = lambda: Path("Z:/missing/SeventySix.esm")

    disabled_depth = 0
    button_depths = {}
    invoked = []

    def begin_disabled():
        nonlocal disabled_depth
        disabled_depth += 1

    def end_disabled():
        nonlocal disabled_depth
        disabled_depth -= 1

    def button(label, *_args):
        name = label.split("##", 1)[0]
        button_depths[name] = disabled_depth
        return disabled_depth == 0 and name in {
            "Restart as administrator",
            "Check / repair INI",
            "Free up space...",
        }

    monkeypatch.setattr(imgui, "begin_disabled", begin_disabled)
    monkeypatch.setattr(imgui, "end_disabled", end_disabled)
    monkeypatch.setattr(imgui, "button", button)
    monkeypatch.setattr(imgui, "combo", lambda _label, index, _items: (False, index))
    monkeypatch.setattr(imgui, "checkbox", lambda _label, value: (False, value))
    monkeypatch.setattr(
        imgui,
        "slider_int",
        lambda _label, value, _minimum, _maximum: (False, value),
    )
    monkeypatch.setattr(
        imgui,
        "collapsing_header",
        lambda label, *_args: label.startswith("Install Info"),
    )
    monkeypatch.setattr(panel, "_restart_elevated", lambda: invoked.append("restart"))
    monkeypatch.setattr(panel, "_run_install_audit", lambda: invoked.append("repair"))
    monkeypatch.setattr(panel, "_open_cleanup_dialog", lambda: invoked.append("cleanup"))

    context = imgui.create_context()
    try:
        io = imgui.get_io()
        io.display_size = (1100, 900)
        io.delta_time = 1 / 60
        io.backend_flags |= imgui.BackendFlags_.renderer_has_textures
        imgui.new_frame()
        imgui.begin("maintenance controls")
        panel._draw_settings_column()
        imgui.end()
        imgui.render()
    finally:
        imgui.destroy_context(context)

    assert disabled_depth == 0
    assert button_depths["Restart as administrator"] == 0
    assert button_depths["Check / repair INI"] == 0
    assert button_depths["Free up space..."] == 1
    assert invoked == ["restart", "repair"]


def test_cancel_button_replaces_deploy_next_to_convert_while_running(monkeypatch):
    panel, ws = _panel()
    cancelled = []
    ws._runner = SimpleNamespace(done=False, cancel=lambda: cancelled.append(True))
    ws._runner_owner = panel
    labels = []

    monkeypatch.setattr(
        panel,
        "disk_usage_summary",
        lambda: {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0},
    )
    monkeypatch.setattr(panel, "disk_usage_loading", lambda: False)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.text", lambda *a, **kw: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.text_disabled", lambda *a, **kw: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.separator", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.text_colored", lambda *a: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.checkbox", lambda _label, value: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.input_text", lambda _label, value: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.collapsing_header", lambda *_a, **_k: False)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.slider_int", lambda _label, value, _min, _max: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.same_line", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_disabled", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.end_disabled", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_table", lambda *_a, **_kw: True)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.table_next_row", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.table_next_column", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.end_table", lambda: None)

    def click_cancel(label, *_args):
        labels.append(label.split("##", 1)[0])
        return label.startswith("Cancel")

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.button", click_cancel)

    panel._draw_settings_column()

    assert labels[:2] == ["Convert Tales From Appalachia", "Cancel"]
    assert "Deploy existing mod" not in labels
    assert cancelled == [True]


def test_panel_defaults_workers_to_ram_aware_recommendation():
    panel, _ = _panel()
    assert panel.workers >= 1
    assert isinstance(panel._worker_rec.note, str) and panel._worker_rec.note


def test_can_convert_requires_both_games_configured():
    panel, ws = _panel()
    assert panel.can_convert() is True
    ws._toolkit_settings.get_game_paths = lambda g: (
        {"root_dir": "C:/FO4", "extracted_dir": ""} if g == "fo4" else {"root_dir": "", "extracted_dir": ""}
    )
    assert panel.can_convert() is False


def test_can_convert_requires_both_steam_installs():
    panel, _ = _panel()
    panel._steam_install_cache = {
        "fo4": ("C:/FO4", _ok_steam_install()),
        "fo76": (
            "C:/FO76",
            SteamInstallResult(
                ok=False,
                game_id="fo76",
                app_id=1151340,
                root_dir="C:/FO76",
                local_install_valid=True,
                steam_layout_valid=True,
                steam_api_present=False,
                appmanifest_present=True,
                appmanifest_matches=True,
                steam_library_dir="C:/SteamLibrary",
                appmanifest_path="C:/SteamLibrary/steamapps/appmanifest_1151340.acf",
                message="Fallout 76 install is missing steam_api64.dll.",
            ),
        ),
    }

    assert panel.can_convert() is False


def test_start_conversion_checks_required_inputs_off_ui_thread(monkeypatch):
    from bacup_ui.appalachia.tests.test_regen_panel_options import _panel, _ws

    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    report = InputPreflightReport(
        required_missing=[MissingInput("FO76 MaterialsDB", "X/MaterialsDB.cdb", "extract it")]
    )
    ui_thread_id = threading.get_ident()

    def scan_inputs(*_args, **_kwargs):
        assert threading.get_ident() != ui_thread_id
        return report

    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.scan_conversion_inputs", scan_inputs
    )
    monkeypatch.setattr(RegenPanel, "_require_steam_installs", lambda self: None)

    panel.start_conversion()
    runner = panel._workspace._runner
    runner._thread.join(timeout=2.0)
    for event in runner.drain():
        panel.handle_event(event)

    assert runner.done is True
    assert panel._preflight_report is report
    assert panel._completion is None


def test_start_conversion_proceeds_when_inputs_ok(monkeypatch):
    from bacup_ui.appalachia.tests.test_regen_panel_options import _panel, _ws

    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.scan_conversion_inputs",
        lambda *_a, **_k: InputPreflightReport(),
    )
    monkeypatch.setattr(RegenPanel, "_require_steam_installs", lambda self: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel.load_lod_settings",
        lambda self, profile, lod_mode: {},
    )
    monkeypatch.setattr(
        "bacup_lib.runner.ConversionRunner.start", lambda self: None
    )

    panel.start_conversion()

    assert panel._preflight_report is None
    assert panel._workspace._runner is not None


def test_draw_renders_two_pane_split_with_log_in_status_column(monkeypatch):
    from bacup_ui.conversion.panels import regen_panel as regen_panel_module
    from bacup_ui.conversion.panels.conversion_log import ConversionLogPanel

    panel, ws = _panel()
    panel._log_panel = ConversionLogPanel(ws)

    log_drawn = {"count": 0}
    original_log_draw_body = panel._log_panel.draw_body

    def spy_log_draw_body():
        log_drawn["count"] += 1
        original_log_draw_body()

    monkeypatch.setattr(panel._log_panel, "draw_body", spy_log_draw_body)
    monkeypatch.setattr(
        panel,
        "disk_usage_summary",
        lambda: {"extracted": 0, "mod_output": 0, "mod_ba2": 0, "deployed_ba2": 0},
    )
    monkeypatch.setattr(panel, "disk_usage_loading", lambda: False)

    imgui = "bacup_ui.conversion.panels.regen_panel.imgui"
    child_calls = []

    def begin_child(*args, **kwargs):
        child_calls.append((args, kwargs))
        return True

    monkeypatch.setattr(f"{imgui}.begin", lambda *_a, **_kw: True)
    monkeypatch.setattr(f"{imgui}.begin_table", lambda *_a, **_kw: True)
    monkeypatch.setattr(f"{imgui}.begin_child", begin_child)
    monkeypatch.setattr(f"{imgui}.button", lambda *_a, **_kw: False)
    monkeypatch.setattr(f"{imgui}.collapsing_header", lambda *_a, **_kw: False)
    monkeypatch.setattr(f"{imgui}.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr(f"{imgui}.checkbox", lambda _label, value: (False, value))
    monkeypatch.setattr(f"{imgui}.input_text", lambda _label, value: (False, value))
    monkeypatch.setattr(
        f"{imgui}.slider_int", lambda _label, value, _min, _max: (False, value)
    )
    monkeypatch.setattr(f"{imgui}.begin_tab_bar", lambda *_a, **_kw: True)
    monkeypatch.setattr(f"{imgui}.begin_tab_item", lambda *_a, **_kw: (True, True))
    monkeypatch.setattr(f"{imgui}.end_tab_item", lambda: None)
    monkeypatch.setattr(f"{imgui}.end_tab_bar", lambda: None)
    monkeypatch.setattr(f"{imgui}.tab_item_button", lambda *_a, **_kw: False)
    monkeypatch.setattr(
        f"{imgui}.get_style",
        lambda: SimpleNamespace(item_spacing=SimpleNamespace(y=4.0)),
    )
    monkeypatch.setattr(f"{imgui}.begin_disabled", lambda: None)
    monkeypatch.setattr(f"{imgui}.end_disabled", lambda: None)

    panel.draw()  # must not raise; renders header + split + relocated log

    assert log_drawn["count"] == 1
    status_call = next(
        call for call in child_calls if call[0][0] == "##appalachia_status_pane"
    )
    assert status_call[1]["window_flags"] == (
        regen_panel_module.imgui.WindowFlags_.no_scrollbar.value
        | regen_panel_module.imgui.WindowFlags_.no_scroll_with_mouse.value
    )
