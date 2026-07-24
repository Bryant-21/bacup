from types import SimpleNamespace

from imgui_bundle import hello_imgui

from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace
from bacup_ui.variant import BACUP_VARIANT
from ui.toolkit.app import (
    ToolkitApp,
    _configure_app_window_params,
    _resolve_window_icon_path,
)


def test_bacup_variant_metadata():
    v = BACUP_VARIANT
    assert v.exe_name == "BACUP"
    assert v.window_title == (
        "B.A.C.U.P. Bethesda Asset Converter Universal Platform"
    )
    assert v.workspace_ids == ("appalachia",)
    assert v.default_workspace == "appalachia"
    assert v.is_standalone is True
    assert v.auto_hide_single_window_tabs is True


def test_bacup_hides_redundant_single_window_dock_tab():
    app = object.__new__(ToolkitApp)
    app._app_variant = BACUP_VARIANT
    app._ai_chat = None
    app._workspaces = []

    params = app._get_docking_params()

    assert (
        params.main_dock_space_node_flags
        & hello_imgui.ImGuiDockNodeFlags_.auto_hide_tab_bar
    )


def test_bacup_content_window_cannot_restore_hidden():
    workspace = AppalachiaWorkspace()

    window = workspace.get_dockable_windows()[0]

    assert window.is_visible is True
    assert window.can_be_closed is False
    assert window.remember_is_visible is False


def test_bacup_window_icon_uses_product_asset(tmp_path):
    resource_dir = tmp_path / "resource"
    variant_icon = resource_dir / "icons" / "modbox21-converter.ico"
    variant_icon.parent.mkdir(parents=True)
    variant_icon.write_bytes(b"variant")

    assert _resolve_window_icon_path(resource_dir, BACUP_VARIANT) == variant_icon


def test_bacup_window_starts_centered_with_owned_ini(monkeypatch, tmp_path):
    monkeypatch.setattr("ui.toolkit.app.get_ini_dir", lambda: tmp_path)
    params = hello_imgui.RunnerParams()
    settings = SimpleNamespace(window_width=400, window_height=300)

    _configure_app_window_params(params, settings, BACUP_VARIANT)

    assert params.ini_filename == str(tmp_path / "bacup.ini")
    assert params.app_window_params.window_geometry.size == (1280, 760)
    assert (
        params.app_window_params.window_geometry.window_size_state
        == hello_imgui.WindowSizeState.standard
    )
    assert (
        params.app_window_params.window_geometry.position_mode
        == hello_imgui.WindowPositionMode.monitor_center
    )


def test_bacup_seeds_invalid_initial_imgui_display_size(monkeypatch):
    display = SimpleNamespace(display_size=SimpleNamespace(x=-1.0, y=-1.0))
    app = object.__new__(ToolkitApp)
    app._app_variant = BACUP_VARIANT
    app._initial_display_size = (1280, 760)
    app._current_theme = "dark"
    app._mono_font = None
    app._ws_map = {}

    monkeypatch.setattr("ui.toolkit.app.imgui.get_io", lambda: display)
    monkeypatch.setattr("ui.toolkit.app.set_window_icon", lambda _variant: None)
    monkeypatch.setattr("ui.toolkit.app.set_native_dark_title_bar", lambda: None)
    monkeypatch.setattr("ui.toolkit.app.apply_theme", lambda _theme: None)
    monkeypatch.setattr("ui.toolkit.app._signal_ready_file", lambda: None)

    app._post_init()

    assert display.display_size == (1280.0, 760.0)


def test_bacup_launcher_constructs_its_workspace_directly(monkeypatch):
    import bacup_ui.__main__ as launcher

    events = []

    class FakeSettings:
        def __init__(self, variant_id):
            self.variant_id = variant_id
            self.active_workspace = None

    class FakeWorkspace:
        def __init__(self, toolkit_settings):
            events.append(("workspace", toolkit_settings.variant_id))

    class FakeApp:
        def __init__(self, workspaces, settings, *, launch_path, app_variant):
            events.append(
                (
                    "app",
                    len(workspaces),
                    settings.active_workspace,
                    launch_path,
                    app_variant.exe_name,
                )
            )

        def run(self):
            events.append(("run",))

    monkeypatch.setattr(launcher, "ToolkitSettings", FakeSettings)
    monkeypatch.setattr(launcher, "AppalachiaWorkspace", FakeWorkspace)
    monkeypatch.setattr(launcher, "ToolkitApp", FakeApp)
    monkeypatch.setattr(launcher, "_set_taskbar_identity", lambda: None)
    monkeypatch.setattr(
        launcher, "_run_bacup_project_setup", lambda _settings: (False, True)
    )

    launcher.run_bacup("input.ba2")

    assert events == [
        ("workspace", "appalachia"),
        ("app", 1, "appalachia", "input.ba2", "BACUP"),
        ("run",),
    ]
