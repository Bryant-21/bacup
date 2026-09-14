import contextlib
import threading
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import pytest

from bacup_lib.input_preflight import InputPreflightReport, MissingInput
from bacup_lib.regen_pipeline import RegenResult
from bacup_lib.lod_settings import PROFILE_HIGH_QUALITY
from bacup_ui.conversion.music import MusicPlayerState
from bacup_ui.conversion.panels.regen_panel import (
    _UNLIMITED_ARCHIVE_MAX_BYTES,
    RegenPanel,
)
from creation_lib.core.gog_install import GogInstallResult
from creation_lib.core.steam_install import SteamInstallResult
from creation_lib.core.store_install import StoreInstallResult


def _steam_detail(
    *,
    ok: bool,
    game_id: str,
    app_id: int,
    root: str,
    message: str,
    steam_api_present: bool = True,
) -> SteamInstallResult:
    return SteamInstallResult(
        ok=ok,
        game_id=game_id,
        app_id=app_id,
        root_dir=root,
        local_install_valid=True,
        steam_layout_valid=True,
        steam_api_present=steam_api_present,
        appmanifest_present=True,
        appmanifest_matches=True,
        steam_library_dir="C:/SteamLibrary",
        appmanifest_path=f"C:/SteamLibrary/steamapps/appmanifest_{app_id}.acf",
        message=message,
    )


def _gog_detail(*, game_id: str, root: str, message: str) -> GogInstallResult:
    return GogInstallResult(
        ok=False,
        game_id=game_id,
        root_dir=root,
        local_install_valid=True,
        info_present=False,
        info_parsed=False,
        play_task_present=False,
        product_id="",
        info_path="",
        message=message,
    )


def _ok_store_install(
    *,
    game_id: str = "fo4",
    app_id: int = 377160,
    root: str = "C:/FO4",
    name: str = "Fallout 4",
) -> StoreInstallResult:
    message = f"{name} Steam install verified."
    return StoreInstallResult(
        ok=True,
        game_id=game_id,
        store="steam",
        root_dir=root,
        local_install_valid=True,
        message=message,
        steam=_steam_detail(
            ok=True, game_id=game_id, app_id=app_id, root=root, message=message
        ),
        gog=_gog_detail(
            game_id=game_id, root=root, message="GOG verification not attempted."
        ),
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
    panel._store_install_cache = {
        "fo4": ("C:/FO4", _ok_store_install()),
        "fo76": (
            "C:/FO76",
            _ok_store_install(
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
    panel.full_logging = True
    captured = {}
    music_calls = []
    diagnostics_root = Path("X:/logs/conversion/SeventySix/run")

    class FakeMusicPlayer:
        current_track = None

        def start(self, tracks):
            music_calls.append(("start", tuple(tracks)))

        def stop(self):
            music_calls.append(("stop",))

    panel.music_muted = False
    panel._music_player = FakeMusicPlayer()
    panel._conversion_music_tracks = lambda: (Path("mus_maintheme.wav"),)

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

    @contextlib.contextmanager
    def fake_full_logging(root, runner):
        captured["logging_root"] = root
        yield runner

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
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.create_run_diagnostics_dir",
        lambda *_a: diagnostics_root,
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.full_logging_scope",
        fake_full_logging,
    ):
        panel.start_conversion()

    assert captured["options"].deploy is False
    assert captured["options"].lod_mode == "hybrid-atlas"
    assert captured["options"].archive_max_bytes == _UNLIMITED_ARCHIVE_MAX_BYTES
    assert captured["options"].include_interior is True
    assert captured["options"].records_limit is None
    assert captured["options"].direct_deploy_archives is True
    assert captured["options"].update_runtime_ini is True
    assert captured["options"].write_land_cache is False
    assert captured["options"].memory_report is True
    assert captured["lod_settings"] == {
        "global": {"worldspaces": ["APPALACHIA"]},
        "profile": PROFILE_HIGH_QUALITY,
        "mode": "hybrid-atlas",
        "objects": {"atlas_mip_flooding": False},
    }
    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["paths"].diagnostics_root == diagnostics_root
    assert captured["logging_root"] == diagnostics_root
    assert Path(captured["complete"][0]) == Path("X:/app/mods/SeventySix")
    assert music_calls == [
        ("start", (Path("mus_maintheme.wav"),)),
        ("stop",),
    ]
    pre_conversion = captured["pre_conversion"]
    assert pre_conversion[0][0] == "start"
    assert pre_conversion[0][1].phase_name == "Prepare Conversion"
    assert pre_conversion[0][1].total_items == 4
    assert [event[1].completed_items for event in pre_conversion[1:-1]] == [1, 2, 3, 4]
    assert pre_conversion[-1][0] == "complete"
    assert pre_conversion[-1][1].status == "completed"


def test_start_conversion_deploys_companion_after_main_deploy(monkeypatch):
    panel, _ = _panel()
    captured = {}
    clock = [100.0]
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.time.perf_counter", lambda: clock[0])

    def cleanup(_paths, _deployed):
        clock[0] += 7
        return []

    monkeypatch.setattr(panel, "_cleanup_after_deploy", cleanup)

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
        clock[0] += 10
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
        clock[0] += 5
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
    assert captured["complete"][1]["elapsed_seconds"] == 22.0


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

    def fake_deploy_existing(paths, *, options):
        captured["paths"] = paths
        captured["options"] = options
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
    assert captured["options"].update_runtime_ini is True
    assert captured["companion_paths"].deploy_data_dir is None
    assert captured["complete"][1]["deploy_existing"] is True
    assert captured["complete"][1]["elapsed_seconds"] >= 0
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
    assert captured["complete"][1]["elapsed_seconds"] >= 0
    assert captured["complete"][1]["companion_deployed"] == ["B21_TalesFromAppalachia.esp"]


def test_handle_complete_hides_ini_snippet_for_standard_ba2s():
    panel, _ = _panel()
    panel.deploy_format = "standard"
    panel._read_ini_snippet = lambda _path: (_ for _ in ()).throw(
        AssertionError("standard BA2 completion must not read INI guidance")
    )
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
    assert panel._completion["ini_snippet"] is None


@pytest.mark.parametrize("seconds,summary,title", [
    (0, {}, "Total conversion time: 0s"),
    (42.2, {}, "Total conversion time: 42s"),
    (59.6, {}, "Total conversion time: 1m 0s"),
    (5025, {}, "Total conversion time: 1h 23m 45s"),
    (65, {"deploy_existing": True}, "Total deployment time: 1m 5s"),
    (3665, {"resume_from": "modt"}, "Total recovery time: 1h 1m 5s"),
    (None, {}, None),
])
def test_completion_popup_displays_formatted_run_duration(monkeypatch, seconds, summary, title):
    from bacup_ui.conversion.panels import regen_panel as module

    panel, _ = _panel()
    titles = []
    monkeypatch.setattr(module, "heading", titles.append)
    monkeypatch.setattr(module.imgui, "begin", lambda *_args: True)
    monkeypatch.setattr(module.imgui, "button", lambda *_args: False)
    panel.handle_event({"type": "complete", "mod_path": "X:/app/mods/SeventySix", "summary": {
        "deployed": True, "elapsed_seconds": seconds, **summary,
    }})
    panel._draw_completion_popup()
    assert titles == ([title] if title is not None else [])


def test_runner_progress_uses_phase_and_item_progress():
    panel, _ = _panel()
    for phase in [
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
    ]:
        panel.handle_event({"type": "phase_start", "data": phase})
    fraction, message = panel._runner_progress()

    assert fraction == (203 + 141 * 0.5) / sum(panel._progress_estimate.weights.values())
    assert message == "Convert Terrain: tile.bto"


def test_runner_progress_does_not_assume_other_phases_finished():
    panel, _ = _panel()
    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "ui_key": "build_esp",
                "phase_name": "Build ESP",
                "status": "running",
                "total_items": 0,
                "completed_items": 0,
            },
        }
    )
    fraction, message = panel._runner_progress()

    assert fraction == 0.0
    assert message == "Build ESP"


def test_runner_progress_uses_specific_post_phase_status():
    panel, _ = _panel()
    panel.handle_event(
        {
            "type": "phase_complete",
            "data": {
                "ui_key": "pack",
                "phase_name": "Pack BA2",
                "status": "completed",
            },
        }
    )
    panel.handle_event({"type": "status", "message": "Writing conversion coverage report"})
    fraction, message = panel._runner_progress()

    assert fraction == 300 / sum(panel._progress_estimate.weights.values())
    assert message == "Writing conversion coverage report"


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


def test_animtext_phase_does_not_show_unstarted_lod():
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
    assert rows == [("generate_anim_text_data", "Generate AnimTextData")]


@pytest.mark.parametrize("phase_name,key,label", [
    ("Regenerate MODT", "regenerate_modt", "Regenerate MODT"),
    ("Finalize Plugin Records", "finalize_plugin_records", "Regenerate MODT / Finalize Plugin"),
    ("Building FO4 target-asset catalog", "building_fo4_target_asset_catalog", "Building FO4 target-asset catalog"),
])
def test_post_and_preparation_phases_do_not_show_unstarted_phases(phase_name, key, label):
    panel, _ = _panel()

    panel.handle_event(
        {
            "type": "phase_start",
            "data": {
                "phase": 0,
                "phase_name": phase_name,
                "status": "running",
                "current_item": "Building mesh manifest",
            },
        }
    )

    assert panel._phases[0]["ui_key"] == key
    rows = panel._phase_rows(panel._phases)
    assert rows == [(key, label)]


def test_generate_precombines_phase_tracked_by_generic_row_appending():
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


def test_cell_offsets_phase_does_not_show_unstarted_phases():
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
    assert rows == [("rebuild_cell_offsets", "Rebuild Cell Offsets")]


@pytest.mark.parametrize("show_logs", [True, False])
def test_draw_status_column_keeps_overall_progress_outside_phase_area(monkeypatch, show_logs):
    panel, ws = _panel()
    ws.show_logs = show_logs
    children = []
    log_draws = []
    panel._log_panel = SimpleNamespace(draw_body=lambda: log_draws.append(True))
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.begin_child",
        lambda name, size, *_args: children.append((name, size)),
    )
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.is_item_active", lambda: False)
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
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.checkbox", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.input_text", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.expandable_section", lambda *_a, **_k: contextlib.nullcontext(False))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.slider_int", lambda _label, value, _min, _max: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.same_line", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.button", lambda _label: False)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_disabled", lambda: None)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.end_disabled", lambda: None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.get_style",
        lambda: SimpleNamespace(item_spacing=SimpleNamespace(x=10.0, y=4.0), frame_padding=SimpleNamespace(x=10.0, y=6.0), alpha=1.0),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.get_time", lambda: 0.0
    )

    panel._draw_status_column()

    assert [name for name, _size in children] == (
        ["progress##appalachia", "log##appalachia"] if show_logs else ["progress##appalachia"]
    )
    assert log_draws == ([True] if show_logs else [])
    assert panel._status_log_frac == 0.44
    assert captured == {
        "status": [],
        "phase_progress": 1,
        "overlay": 0,
    }


def test_sidebar_music_controls_expose_full_transport(monkeypatch):
    panel, ws = _panel()
    ws._runner = SimpleNamespace(done=False, cancel=lambda: None)
    panel.music_muted = False
    actions = []
    rendered_text = []

    class Player:
        def snapshot(self):
            return MusicPlayerState(
                active=True,
                paused=False,
                current_track=Path("mus_maintheme.wav"),
                track_index=0,
                track_count=5,
                position_seconds=30.0,
                duration_seconds=120.0,
                volume=0.12,
            )

        def previous(self):
            actions.append("previous")

        def pause(self):
            actions.append("pause")

        def resume(self):
            actions.append("resume")

        def next(self):
            actions.append("next")

        def seek(self, progress):
            actions.append(("seek", progress))

    panel._music_player = Player()
    panel._set_music_volume = lambda volume: actions.append(("volume", volume))
    monkeypatch.setattr(
        "bacup_ui.conversion.widgets.draw_phase_progress",
        lambda *_args, **_kwargs: None,
    )
    imgui = "bacup_ui.conversion.panels.regen_panel.imgui"
    monkeypatch.setattr(f"{imgui}.begin_child", lambda *_args, **_kwargs: True)
    monkeypatch.setattr(f"{imgui}.end_child", lambda: None)
    monkeypatch.setattr(f"{imgui}.invisible_button", lambda *_args: None)
    monkeypatch.setattr(f"{imgui}.is_item_active", lambda: False)
    monkeypatch.setattr(f"{imgui}.is_item_hovered", lambda: False)
    monkeypatch.setattr(f"{imgui}.same_line", lambda: None)
    monkeypatch.setattr(f"{imgui}.separator", lambda: None)
    monkeypatch.setattr(f"{imgui}.set_next_item_width", lambda _width: None)
    monkeypatch.setattr(
        f"{imgui}.get_content_region_avail",
        lambda: SimpleNamespace(x=300.0, y=500.0),
    )
    monkeypatch.setattr(
        f"{imgui}.get_style",
        lambda: SimpleNamespace(item_spacing=SimpleNamespace(x=10.0, y=4.0), frame_padding=SimpleNamespace(x=10.0, y=6.0), alpha=1.0),
    )
    monkeypatch.setattr(f"{imgui}.get_time", lambda: 0.0)
    monkeypatch.setattr(
        f"{imgui}.checkbox",
        lambda _label, value, *_args: (False, value),
    )
    monkeypatch.setattr(
        f"{imgui}.button",
        lambda label: any(
            name in label
            for name in ("_music_previous", "_music_pause", "_music_next")
        ),
    )

    def slider(label, value, _minimum, _maximum, **_kwargs):
        if "music_volume" in label:
            return True, 40.0
        if "music_timeline" in label:
            return True, 0.5
        return False, value

    monkeypatch.setattr(f"{imgui}.slider_float", slider)
    monkeypatch.setattr(
        f"{imgui}.text",
        lambda message: rendered_text.append(str(message)),
    )
    monkeypatch.setattr(
        f"{imgui}.text_disabled",
        lambda message: rendered_text.append(str(message)),
    )

    panel._draw_music_controls()

    assert actions == [
        "previous",
        "pause",
        "next",
        ("volume", 0.4),
        ("seek", 0.5),
    ]
    assert "1/5  ·  mus_maintheme" in rendered_text
    assert "00:30 / 02:00" in rendered_text


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
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.checkbox", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.input_text", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.expandable_section", lambda *_a, **_k: contextlib.nullcontext(False))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.slider_int", lambda _label, value, _min, _max: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.same_line", lambda: None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.imgui.button",
        lambda label, *_a: label.startswith("Convert##"),
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
    panel._draw_actions()

    assert disabled_depth == 0


def test_standard_ba2_hides_ini_controls_while_cleanup_is_measuring(monkeypatch):
    from imgui_bundle import imgui

    panel, _ws = _panel()
    panel.fixed_pair_id = "fo76:fo4"
    panel.deploy_format = "standard"
    panel.install_location = "mo2"
    panel.install_path = "C:/ModOrganizer/mods/SeventySix"
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
    panel.resolve_ba2_target = lambda: "og"
    panel._deployed_esm_exists = lambda: False
    panel._load_upgrade_manifest_cached = lambda: None
    panel.can_convert = lambda: False
    panel.can_deploy_existing = lambda: False
    panel.generated_plugin_path = lambda: Path("Z:/missing/SeventySix.esm")

    disabled_depth = 0
    button_depths = {}
    invoked = []
    rendered_text = []

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
    monkeypatch.setattr(imgui, "begin_table", lambda *_args, **_kwargs: True)
    monkeypatch.setattr(imgui, "end_table", lambda: None)
    monkeypatch.setattr(imgui, "table_next_row", lambda: None)
    monkeypatch.setattr(imgui, "table_next_column", lambda: None)
    original_text = imgui.text

    def capture_text(message):
        rendered_text.append(str(message))
        original_text(message)

    monkeypatch.setattr(imgui, "text", capture_text)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.begin_form",
        lambda *_args, **_kwargs: True,
    )
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.end_form", lambda: None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel._form_row_label",
        lambda label: rendered_text.append(label),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.draw_combo_field",
        lambda label, _items, index: (
            rendered_text.append(label) or False,
            index,
        ),
    )
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.draw_path_row",
        lambda _label, value: (value, False),
    )
    monkeypatch.setattr(imgui, "combo", lambda _label, index, _items: (False, index))
    monkeypatch.setattr(imgui, "checkbox", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr(
        imgui,
        "input_text",
        lambda _label, value, *_args: (False, value),
    )
    monkeypatch.setattr(
        imgui,
        "slider_int",
        lambda _label, value, _minimum, _maximum: (False, value),
    )
    headers = []

    def expandable_section(label, *args, **kwargs):
        headers.append((label.split("##", 1)[0], args))
        return contextlib.nullcontext(True)

    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.expandable_section", expandable_section)
    monkeypatch.setattr(panel, "_restart_elevated", lambda: invoked.append("restart"))
    monkeypatch.setattr(panel, "_run_install_audit", lambda: invoked.append("repair"))
    monkeypatch.setattr(panel, "_draw_install_audit", lambda: invoked.append("draw audit"))
    monkeypatch.setattr(panel, "_open_cleanup_dialog", lambda: invoked.append("cleanup"))

    context = imgui.create_context()
    try:
        io = imgui.get_io()
        io.display_size = (1100, 2600)
        io.delta_time = 1 / 60
        io.backend_flags |= imgui.BackendFlags_.renderer_has_textures
        imgui.new_frame()
        imgui.set_next_window_size(imgui.ImVec2(1100, 2600))
        imgui.begin("maintenance controls")
        panel._draw_settings_column()
        imgui.end()
        imgui.render()
    finally:
        imgui.destroy_context(context)

    assert disabled_depth == 0
    assert button_depths.get("Restart as administrator") == 0, button_depths
    assert "Check / repair INI" not in button_depths
    assert button_depths["Free up space..."] == 1
    assert invoked == ["restart"]
    assert "Deploy To:" in rendered_text
    assert "Install location" not in rendered_text
    assert "Detected FO4" not in rendered_text
    assert "INI" not in rendered_text
    assert "MO2 profile INI" not in rendered_text
    assert "Update MO2 Custom.ini" not in rendered_text
    assert "Update Fallout4Custom.ini" not in rendered_text
    assert [label for label, _args in headers] == ["Game install information", "Advanced"]


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
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.checkbox", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.input_text", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.expandable_section", lambda *_a, **_k: contextlib.nullcontext(False))
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

    panel._draw_actions()

    assert labels[:2] == ["Convert", "Cancel"]
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


def test_can_convert_requires_both_store_installs():
    panel, _ = _panel()
    panel._store_install_cache = {
        "fo4": ("C:/FO4", _ok_store_install()),
        "fo76": (
            "C:/FO76",
            StoreInstallResult(
                ok=False,
                game_id="fo76",
                store="",
                root_dir="C:/FO76",
                local_install_valid=True,
                message=(
                    "Fallout 76 was not recognized as a Steam or GOG install."
                ),
                steam=_steam_detail(
                    ok=False,
                    game_id="fo76",
                    app_id=1151340,
                    root="C:/FO76",
                    message="Fallout 76 install is missing steam_api64.dll.",
                    steam_api_present=False,
                ),
                gog=_gog_detail(
                    game_id="fo76",
                    root="C:/FO76",
                    message=(
                        "No GOG goggame-*.info manifest was found in the "
                        "Fallout 76 folder."
                    ),
                ),
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
    monkeypatch.setattr(RegenPanel, "_require_store_installs", lambda self: None)

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
    monkeypatch.setattr(RegenPanel, "_require_store_installs", lambda self: None)
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
    ws.show_logs = True
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
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.expandable_section", lambda *_a, **_kw: contextlib.nullcontext(False))
    monkeypatch.setattr(f"{imgui}.combo", lambda _label, idx, _items: (False, idx))
    monkeypatch.setattr(f"{imgui}.checkbox", lambda _label, value, *_args: (False, value))
    monkeypatch.setattr(f"{imgui}.input_text", lambda _label, value, *_args: (False, value))
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
        lambda: SimpleNamespace(item_spacing=SimpleNamespace(x=10.0, y=4.0), frame_padding=SimpleNamespace(x=10.0, y=6.0), alpha=1.0),
    )
    monkeypatch.setattr(f"{imgui}.begin_disabled", lambda: None)
    monkeypatch.setattr(f"{imgui}.end_disabled", lambda: None)
    monkeypatch.setattr(f"{imgui}.get_scroll_y", lambda: 0.0)
    monkeypatch.setattr(f"{imgui}.get_scroll_max_y", lambda: 0.0)

    panel.draw()  # must not raise; renders header + split + relocated log

    assert log_drawn["count"] == 1
    status_call = next(
        call for call in child_calls if call[0][0] == "##appalachia_status_pane"
    )
    assert status_call[1]["window_flags"] == (
        regen_panel_module.imgui.WindowFlags_.no_scrollbar.value
        | regen_panel_module.imgui.WindowFlags_.no_scroll_with_mouse.value
    )


def test_phase_rows_follow_start_order_despite_reused_phase_numbers():
    panel, _ = _panel()
    names = [
        "Translate Records", "Copy Sounds", "Convert Terrain",
        "Emit Projected NavMeshes", "Convert Interior Cells", "Convert Scripts",
        "Build ESP", "convert_nifs_v2", "Regenerate MODT", "Pack BA2", "Deploy Mod",
    ]
    for name in names:
        panel.handle_event({"type": "phase_start", "data": {
            "phase": 0, "phase_name": name, "status": "running",
        }})
    panel.handle_event({"type": "phase_complete", "data": {
        "phase": 2, "phase_name": "Translate Records", "status": "completed",
    }})
    labels = [label for _key, label in panel._phase_rows(panel._phases)]
    assert labels == [name.replace("convert_nifs_v2", "Convert NIFs") for name in names]


def test_unused_and_skipped_bars_stay_hidden_but_fnv_animations_appear():
    panel, _ = _panel()
    assert panel._phase_rows(panel._phases) == []
    panel.handle_event({"type": "phase_start", "data": {
        "phase_name": "Convert Havok", "status": "running",
    }})
    panel.handle_event({"type": "phase_complete", "data": {
        "phase_name": "Convert Animations", "status": "skipped",
    }})
    assert panel._phase_rows(panel._phases) == [("convert_havok", "Convert Havok")]
    panel.pair_id = "fnvfo3:fo4"
    panel._phases = []
    panel._reset_progress()
    panel.handle_event({"type": "phase_start", "data": {
        "phase_name": "convert_animations", "status": "running",
    }})
    assert panel._phase_rows(panel._phases) == [("convert_animations", "Convert Animations")]


def test_overall_reaches_100_only_after_successful_final_result():
    panel, _ = _panel()
    panel.handle_event({"type": "phase_complete", "data": {
        "phase_name": "Pack BA2", "status": "completed",
    }})
    panel.handle_event({"type": "complete", "summary": {"esp_built": True}})
    assert panel._runner_progress()[0] < 1.0
    panel.handle_event({"type": "complete", "summary": {"exit_code": 2}})
    assert panel._runner_progress()[0] < 1.0
    panel.handle_event({"type": "complete", "summary": {"exit_code": 0}})
    assert panel._runner_progress() == (1.0, "Conversion complete")
    panel._phases = []
    panel._reset_progress()
    assert panel._runner_progress()[0] == 0.0


def test_overall_bar_precedes_scrolling_status_content(monkeypatch):
    panel, _ws = _panel()
    calls = []
    for method in ("_draw_settings_column", "_draw_actions"):
        monkeypatch.setattr(panel, method, lambda: None)
    monkeypatch.setattr(panel, "_draw_overall_progress", lambda: calls.append("overall"))
    monkeypatch.setattr(panel, "_draw_status_column", lambda: calls.append("status"))
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_table", lambda *_a, **_kw: True)
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.imgui.begin_child", lambda *_a, **_kw: True)
    panel._draw_split()
    assert calls == ["overall", "status"]
