import contextlib
import threading
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import pytest

from bacup_lib.input_preflight import InputPreflightReport, MissingInput
from bacup_lib.regen_pipeline import RegenResult
from bacup_lib.lod_settings import PROFILE_HIGH_QUALITY
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


def _fake_runner(captured):
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

    return FakeRunner


def _regen_result(*, deployed):
    return RegenResult(
        exit_code=0,
        output_root=Path("X:/app/mods/SeventySix"),
        elapsed_seconds=1.0,
        deployed=deployed,
        failures=[],
        warnings=[],
    )


def _fake_lod_settings(self, profile, lod_mode):
    return {"global": {"worldspaces": ["APPALACHIA"]}, "profile": profile, "mode": lod_mode}


_EXPECTED_LOD_SETTINGS = {
    "global": {"worldspaces": ["APPALACHIA"]},
    "profile": PROFILE_HIGH_QUALITY,
    "mode": "hybrid-atlas",
    "objects": {"atlas_mip_flooding": False},
}


def test_start_conversion_invokes_run_full_regen_with_built_args():
    panel, ws = _panel()
    panel.install_location = "none"
    panel.full_logging = True
    captured = {}
    diagnostics_root = Path("X:/logs/conversion/SeventySix/run")

    def fake_run(paths, options, *, phases, runner, **kw):
        captured["paths"] = paths
        captured["options"] = options
        captured["lod_settings"] = kw.get("lod_settings")
        return _regen_result(deployed=False)

    @contextlib.contextmanager
    def fake_full_logging(root, runner):
        captured["logging_root"] = root
        yield runner

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", _fake_runner(captured)), patch(
        "bacup_lib.regen_pipeline.run_full_regen", fake_run
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel.load_lod_settings",
        _fake_lod_settings,
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
    assert captured["options"].direct_deploy_archives is True
    assert captured["options"].update_runtime_ini is True
    assert captured["options"].memory_report is True
    assert captured["lod_settings"] == _EXPECTED_LOD_SETTINGS
    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["paths"].diagnostics_root == diagnostics_root
    assert captured["logging_root"] == diagnostics_root
    assert Path(captured["complete"][0]) == Path("X:/app/mods/SeventySix")


@pytest.mark.parametrize("start_method, pipeline_fn, summary_key, summary_value", [
    ("start_deploy_existing", "deploy_existing", "deploy_existing", True),
    ("start_resume_from_phase", "run_resume_from_phase", "resume_from", "lodgen"),
])
def test_deploy_and_recovery_runs_deploy_companion(start_method, pipeline_fn, summary_key, summary_value):
    panel, _ = _panel()
    panel.recovery_phase = "lodgen"
    captured = {}

    def fake_pipeline(paths, *args, **kw):
        captured["paths"] = paths
        captured["options"] = kw.get("options") or args[0]
        captured["kw"] = kw
        return _regen_result(deployed=True)

    def fake_deploy_companion(self, paths, runner):
        captured["companion_paths"] = paths
        return ["B21_TalesFromAppalachia.esm"]

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", _fake_runner(captured)), patch(
        f"bacup_lib.regen_pipeline.{pipeline_fn}", fake_pipeline
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel.load_lod_settings",
        _fake_lod_settings,
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._deploy_companion_mod",
        fake_deploy_companion,
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")):
        getattr(panel, start_method)()

    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["options"].update_runtime_ini is True
    if pipeline_fn == "run_resume_from_phase":
        assert captured["kw"]["start_phase"] == "lodgen"
        assert captured["kw"]["lod_settings"] == _EXPECTED_LOD_SETTINGS
    assert captured["companion_paths"].output_root == Path("X:/app/mods/SeventySix")
    assert captured["complete"][1][summary_key] == summary_value
    assert captured["complete"][1]["companion_deployed"] == ["B21_TalesFromAppalachia.esm"]


def test_continue_on_error_defaults_on():
    panel, _ = _panel()

    assert panel.continue_on_error is True
    assert panel.build_options().continue_on_error is True


@pytest.mark.parametrize("continue_on_error", [True, False])
def test_deploy_existing_with_failures_follows_continue_on_error(continue_on_error):
    panel, _ = _panel()
    panel.continue_on_error = continue_on_error
    captured = {}
    cleanup_calls = []

    def fake_deploy_existing(paths, **_kw):
        return RegenResult(
            exit_code=2,
            output_root=Path("X:/app/mods/SeventySix"),
            elapsed_seconds=1.0,
            deployed=True,
            failures=["Pack BA2: disk full"],
        )

    def failing_companion(self, paths, runner):
        raise FileNotFoundError("Bundled mod directory not found: B21_FullScreenMap")

    def fake_cleanup(self, paths, deployed):
        cleanup_calls.append(deployed)
        return []

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", _fake_runner(captured)), patch(
        "bacup_lib.regen_pipeline.deploy_existing", fake_deploy_existing
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._deploy_companion_mod", failing_companion
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._cleanup_after_deploy", fake_cleanup
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")):
        if not continue_on_error:
            with pytest.raises(RuntimeError, match="Pack BA2: disk full"):
                panel.start_deploy_existing()
            return
        panel.start_deploy_existing()

    summary = captured["complete"][1]
    assert summary["deployed"] is True
    assert summary["failures"] == [
        "Pack BA2: disk full",
        "Deploy companion mod: Bundled mod directory not found: B21_FullScreenMap",
    ]
    # Failed runs keep their output so they can be resumed or redeployed.
    assert cleanup_calls == [False]


def test_start_undeploy_removes_output_plugin_and_bundled_mods():
    panel, _ = _panel()
    captured = {}

    def fake_undeploy(paths, *, plugin_names):
        captured["paths"] = paths
        captured["plugin_names"] = plugin_names
        return _regen_result(deployed=False)

    def fake_undeploy_bundled(self, paths, runner):
        return ["F4SE/Plugins/B21_TalesFromAppalachia.dll"]

    with patch("bacup_ui.conversion.panels.regen_panel.ConversionRunner", _fake_runner(captured)), patch(
        "bacup_lib.regen_pipeline.undeploy", fake_undeploy
    ), patch(
        "bacup_ui.conversion.panels.regen_panel.RegenPanel._undeploy_bundled_mods",
        fake_undeploy_bundled,
    ), patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app")):
        panel.start_undeploy()

    assert captured["plugin_names"] == ["SeventySix.esm"]
    assert captured["paths"].output_root == Path("X:/app/mods/SeventySix")
    summary = captured["complete"][1]
    assert summary["undeployed"] is True
    assert summary["deployed"] is False
    assert summary["bundled_removed"] == ["F4SE/Plugins/B21_TalesFromAppalachia.dll"]


def test_can_convert_requires_both_games_and_store_installs():
    panel, ws = _panel()
    assert panel.can_convert() is True
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

    panel, ws = _panel()
    ws._toolkit_settings.get_game_paths = lambda g: (
        {"root_dir": "C:/FO4", "extracted_dir": ""} if g == "fo4" else {"root_dir": "", "extracted_dir": ""}
    )
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
