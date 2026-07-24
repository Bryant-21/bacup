from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import MagicMock

from creation_lib.ui.widgets.user_guide import UserGuide


class GuideWorkspace:
    name = "Guide Workspace"
    id = "guide"

    def __init__(self):
        self.toggles = 0

    def get_user_guide(self):
        return UserGuide("Guide Workspace User Guide", "Guide body.")

    def toggle_user_guide(self):
        self.toggles += 1

    def has_toolbar(self):
        return False

    def draw_menu(self):
        pass

    def get_dockable_windows(self):
        return []


class NoGuideWorkspace:
    name = "No Guide Workspace"
    id = "no_guide"

    def draw_menu(self):
        pass

    def has_toolbar(self):
        return False


class GuideOnlyWorkspace:
    name = "Guide Only Workspace"
    id = "guide_only"

    def __init__(self):
        self.toggles = 0

    def get_user_guide(self):
        return UserGuide("Guide Only Workspace User Guide", "Guide body.")

    def toggle_user_guide(self):
        self.toggles += 1

    def has_toolbar(self):
        return False

    def draw_menu(self):
        pass


class FakeVariant:
    is_standalone = False


class GuiWorkspace(GuideWorkspace):
    def __init__(self):
        super().__init__()
        self.draw_calls = 0
        self.guide_draws = 0

    def draw(self):
        self.draw_calls += 1

    def draw_user_guide_window(self):
        self.guide_draws += 1


def test_toolkit_show_menus_draws_common_help_menu(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    ws = GuideWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._app_variant = FakeVariant()
    app._active_ws = ws
    app._ws_map = {}
    app._show_about = False
    app._show_theme_selector = False

    labels: list[str] = []

    def fake_begin_menu(label):
        labels.append(label)
        return label == "Help"

    monkeypatch.setattr("ui.toolkit.app.imgui.begin_menu", fake_begin_menu)
    monkeypatch.setattr("ui.toolkit.app.imgui.menu_item", lambda *args, **kwargs: (args[0] == "User Guide", False))
    monkeypatch.setattr("ui.toolkit.app.imgui.end_menu", lambda: None)
    monkeypatch.setattr("ui.toolkit.app.imgui.separator", lambda: None)

    app._show_menus()

    assert "Help" in labels
    assert ws.toggles == 1


def test_toolkit_show_menus_draws_common_help_menu_in_standalone(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    ws = GuideWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._app_variant = type("Variant", (), {"is_standalone": True})()
    app._active_ws = ws
    app._ws_map = {}
    app._show_about = False
    app._show_theme_selector = False

    labels: list[str] = []

    def fake_begin_menu(label):
        labels.append(label)
        return label == "Help"

    monkeypatch.setattr("ui.toolkit.app.imgui.begin_menu", fake_begin_menu)
    monkeypatch.setattr("ui.toolkit.app.imgui.menu_item", lambda *args, **kwargs: (args[0] == "User Guide", False))
    monkeypatch.setattr("ui.toolkit.app.imgui.end_menu", lambda: None)
    monkeypatch.setattr("ui.toolkit.app.imgui.separator", lambda: None)

    app._show_menus()

    assert "Help" in labels
    assert ws.toggles == 1


def test_toolkit_show_menus_skips_common_help_menu_without_guide(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    ws = NoGuideWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._app_variant = FakeVariant()
    app._active_ws = ws
    app._ws_map = {}
    app._show_about = False
    app._show_theme_selector = False

    labels: list[str] = []

    def fake_begin_menu(label):
        labels.append(label)
        return label == "Workspace"

    monkeypatch.setattr("ui.toolkit.app.imgui.begin_menu", fake_begin_menu)
    monkeypatch.setattr("ui.toolkit.app.imgui.menu_item", lambda *args, **kwargs: (False, False))
    monkeypatch.setattr("ui.toolkit.app.imgui.end_menu", lambda: None)
    monkeypatch.setattr("ui.toolkit.app.imgui.separator", lambda: None)

    app._show_menus()

    assert "Help" not in labels


def test_toolbar_help_button_keeps_strip_hidden_for_guide_only_workspace(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    ws = GuideOnlyWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._active_ws = ws
    app._toolbar_icon_font = None

    class Top:
        def __init__(self):
            self.options = type("Options", (), {"size_em": 0.0})()

    top = Top()
    edge_top = object()
    runner_params = type(
        "RunnerParams",
        (),
        {"callbacks": type("Callbacks", (), {"edges_toolbars": {}})()},
    )()
    runner_params.callbacks.edges_toolbars = {edge_top: top}

    monkeypatch.setattr("ui.toolkit.app.hello_imgui.get_runner_params", lambda: runner_params)
    monkeypatch.setattr("ui.toolkit.app.hello_imgui.EdgeToolbarType", type("EdgeToolbarType", (), {"top": edge_top}))
    calls = []
    monkeypatch.setattr("ui.toolkit.app.draw_toolbar_help_button", lambda *args, **kwargs: calls.append(args) or False)
    monkeypatch.setattr("ui.toolkit.app.has_user_guide", lambda provider: True)

    app._draw_top_toolbar()

    assert top.options.size_em == 0.01
    assert calls == []


def test_toolbar_help_helper_toggles_provider(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import draw_toolbar_help_button

    ws = GuideWorkspace()
    monkeypatch.setattr(user_guide.imgui, "get_content_region_avail", lambda: type("Vec", (), {"x": 200.0})())
    monkeypatch.setattr(user_guide.imgui, "calc_text_size", lambda text: type("Vec", (), {"x": 18.0})())
    monkeypatch.setattr(user_guide.imgui, "get_style", lambda: type("Style", (), {"frame_padding": type("Pad", (), {"x": 4.0})()})())
    monkeypatch.setattr(user_guide.imgui, "get_cursor_pos_x", lambda: 0.0)
    monkeypatch.setattr(user_guide.imgui, "set_cursor_pos_x", lambda x: None)
    monkeypatch.setattr(user_guide.imgui, "get_frame_height", lambda: 28.0)
    monkeypatch.setattr(user_guide.imgui, "button", lambda label, size=None: True)
    monkeypatch.setattr(user_guide.imgui, "set_item_tooltip", lambda text: None)

    draw_toolbar_help_button(ws)

    assert ws.toggles == 1


def test_toolbar_help_helper_uses_square_icon_button_on_same_row(monkeypatch):
    from creation_lib.ui.widgets import user_guide
    from creation_lib.ui.widgets.user_guide import draw_toolbar_help_button

    ws = GuideWorkspace()
    calls = {"same_line": 0, "button": []}

    monkeypatch.setattr(user_guide.imgui, "same_line", lambda: calls.__setitem__("same_line", calls["same_line"] + 1))
    monkeypatch.setattr(user_guide.imgui, "get_content_region_avail", lambda: type("Vec", (), {"x": 200.0})())
    monkeypatch.setattr(user_guide.imgui, "calc_text_size", lambda text: type("Vec", (), {"x": 18.0})())
    monkeypatch.setattr(user_guide.imgui, "get_style", lambda: type("Style", (), {"frame_padding": type("Pad", (), {"x": 4.0})()})())
    monkeypatch.setattr(user_guide.imgui, "get_cursor_pos_x", lambda: 10.0)
    monkeypatch.setattr(user_guide.imgui, "set_cursor_pos_x", lambda x: None)
    monkeypatch.setattr(user_guide.imgui, "get_frame_height", lambda: 28.0)
    monkeypatch.setattr(user_guide.imgui, "button", lambda label, size=None: calls["button"].append((label, size)) or False)
    monkeypatch.setattr(user_guide.imgui, "set_item_tooltip", lambda text: None)
    monkeypatch.setattr(user_guide.fa, "ICON_FA_CIRCLE_QUESTION", "Q")

    draw_toolbar_help_button(ws, same_line=True)

    assert calls["same_line"] == 1
    assert calls["button"] == [("Q##user_guide", user_guide.imgui.ImVec2(28.0, 0))]


def test_toolkit_gui_draws_workspace_user_guide_window(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    ws = GuiWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._active_ws = ws
    app._first_frame = False
    app._current_theme = SimpleNamespace(id="theme")
    app._settings_window = SimpleNamespace(
        _active_workspace=None,
        draw=lambda: None,
        saved_and_closed=False,
        rerun_setup=False,
    )
    app._settings = SimpleNamespace(theme="theme")
    app._log_panel = SimpleNamespace(_active_workspace_id=None, install=lambda: None)
    app._show_about = False
    app._show_theme_selector = False
    app._apply_tab_style = lambda: None

    monkeypatch.setattr("ui.toolkit.app._log", SimpleNamespace(error=lambda *args, **kwargs: None))
    # Replace the whole imgui reference rather than patching individual calls —
    # under the real imgui_bundle (no ambient test stub, no GL context) the
    # untouched calls (open_popup, get_main_viewport, ...) segfault the process.
    mock_imgui = MagicMock()
    mock_imgui.begin_popup_modal.return_value = (False, False)
    monkeypatch.setattr("ui.toolkit.app.imgui", mock_imgui)

    app._gui()

    assert ws.draw_calls == 1
    assert ws.guide_draws == 1


def test_toolkit_docking_params_adds_generic_help_window_for_guided_workspace(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    class DockableWindow:
        def __init__(self, label_=None, dock_space_name_=None):
            self.label = label_
            self.dock_space_name = dock_space_name_
            self.call_begin_end = True
            self.gui_function = None

    ws = GuideWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._ai_chat = None
    app._workspaces = [ws]
    app._log_panel = SimpleNamespace(draw=lambda: None)

    # Real hello_imgui.DockingParams() type-checks its dockable_windows setter
    # against the real DockableWindow class, which the fake below isn't — so
    # the whole hello_imgui reference must be replaced, not just DockableWindow.
    monkeypatch.setattr("ui.toolkit.app.hello_imgui", MagicMock())
    monkeypatch.setattr("ui.toolkit.app.hello_imgui.DockableWindow", DockableWindow)
    monkeypatch.setattr("ui.toolkit.app.has_user_guide", lambda provider: True)

    params = app._get_docking_params()
    labels = [window.label for window in params.dockable_windows]

    assert "Help##guide" in labels


def test_toolkit_docking_params_does_not_duplicate_existing_help_window(monkeypatch):
    from ui.toolkit.app import ToolkitApp
    from creation_lib.ui.shell import make_window

    class DockableWindow:
        def __init__(self, label_=None, dock_space_name_=None):
            self.label = label_
            self.dock_space_name = dock_space_name_
            self.call_begin_end = True
            self.gui_function = None

    class ExistingHelpWorkspace(GuideWorkspace):
        id = "existing"

        def get_dockable_windows(self):
            return [make_window("Help##existing", "RightDock")]

    ws = ExistingHelpWorkspace()
    app = ToolkitApp.__new__(ToolkitApp)
    app._ai_chat = None
    app._workspaces = [ws]
    app._log_panel = SimpleNamespace(draw=lambda: None)

    # Real hello_imgui.DockingParams() type-checks its dockable_windows setter
    # against the real DockableWindow class, which the fake below isn't — so
    # the whole hello_imgui reference must be replaced, not just DockableWindow.
    monkeypatch.setattr("ui.toolkit.app.hello_imgui", MagicMock())
    monkeypatch.setattr("ui.toolkit.app.hello_imgui.DockableWindow", DockableWindow)
    monkeypatch.setattr("creation_lib.ui.shell.base_workspace.hello_imgui.DockableWindow", DockableWindow)
    monkeypatch.setattr("ui.toolkit.app.has_user_guide", lambda provider: True)

    params = app._get_docking_params()
    labels = [window.label for window in params.dockable_windows]

    assert labels.count("Help##existing") == 1


def test_papyrus_workspace_provides_existing_rich_guide():
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace()
    guide = ws.get_user_guide()

    assert guide is not None
    assert guide.title == "Papyrus Editor User Guide"
    assert "Papyrus Editor" in guide.body


def test_all_registered_toolkit_workspaces_provide_user_guides(monkeypatch, tmp_path):
    from ui.toolkit.workspaces import create_workspaces

    temp_index = 0

    def make_temp_dir(prefix: str) -> str:
        nonlocal temp_index
        temp_index += 1
        temp_dir = tmp_path / f"{prefix}{temp_index}"
        temp_dir.mkdir()
        return str(temp_dir)

    monkeypatch.setattr("tempfile.mkdtemp", make_temp_dir)
    missing = [ws.id for ws in create_workspaces() if ws.get_user_guide() is None]

    assert missing == []


def test_nif_workspace_provides_existing_rich_guide():
    from ui.toolkit.workspaces.nif_workspace import NifWorkspace

    ws = NifWorkspace()
    guide = ws.get_user_guide()

    assert guide is not None
    assert guide.title == "NIF Editor User Guide"
    assert "NIF Editor" in guide.body


def test_gun_fire_workspace_provides_current_rich_guide():
    from ui.toolkit.workspaces.audio_tools import GunFireWorkspace

    ws = GunFireWorkspace()
    guide = ws.get_user_guide()

    assert guide is not None
    assert guide.title == "Gun Fire Generator User Guide"
    assert "Shot Variant Pool" in guide.body
    assert "Early Reflections" in guide.body
    assert "Tonal Color" in guide.body
    assert "Bass Reinforcement" in guide.body
    assert "loop markers" in guide.body


def test_papyrus_workspace_draw_menu_no_longer_emits_help_menu(monkeypatch):
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace

    ws = PapyrusWorkspace()
    ws._view_helper = None
    ws._app = type(
        "App",
        (),
        {
            "new_file": lambda self: None,
            "save_current_file": lambda self: None,
            "open_file_dialog": lambda self: None,
            "active_path": None,
        },
    )()
    labels: list[str] = []

    monkeypatch.setattr(
        "ui.toolkit.workspaces.papyrus_workspace.imgui.begin_menu",
        lambda label: labels.append(label) or False,
    )
    monkeypatch.setattr("ui.toolkit.workspaces.papyrus_workspace.imgui.end_menu", lambda: None)

    ws.draw_menu()

    assert "Help" not in labels


def test_behavior_workspace_leaves_help_menu_to_toolkit_host(monkeypatch):
    from ui.toolkit.workspaces.behavior_workspace import BehaviorWorkspace

    ws = BehaviorWorkspace()
    calls = []
    ws._view_helper = None
    ws._app = type(
        "App",
        (),
        {"draw_menu_items": lambda self, include_help=True: calls.append(include_help)},
    )()

    ws.draw_menu()

    assert calls == [False]
