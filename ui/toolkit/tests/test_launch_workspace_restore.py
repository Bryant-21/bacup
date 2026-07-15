from types import SimpleNamespace


class FakeWorkspace:
    def __init__(self, workspace_id: str):
        self.id = workspace_id
        self.name = workspace_id
        self.active = False
        self.opened_files: list[str] = []

    def on_activate(self) -> None:
        self.active = True

    def on_deactivate(self) -> None:
        self.active = False

    def open_file(self, path: str) -> None:
        self.opened_files.append(path)


def test_launch_file_open_does_not_replace_remembered_workspace(tmp_path):
    from ui.toolkit.app import ToolkitApp

    launch_file = tmp_path / "weapon.nif"
    settings = SimpleNamespace(active_workspace="behavior")
    nif_workspace = FakeWorkspace("nif")

    app = ToolkitApp.__new__(ToolkitApp)
    app._active_ws = None
    app._launch_open_done = False
    app._launch_path = str(launch_file)
    app._settings = settings
    app._ws_map = {"nif": nif_workspace}

    app._handle_launch_open()

    assert app._active_ws is nif_workspace
    assert nif_workspace.opened_files == [str(launch_file.resolve(strict=False))]
    assert settings.active_workspace == "behavior"


def test_user_workspace_switch_updates_remembered_workspace():
    from ui.toolkit.app import ToolkitApp

    settings = SimpleNamespace(active_workspace="behavior")
    behavior_workspace = FakeWorkspace("behavior")
    nif_workspace = FakeWorkspace("nif")

    app = ToolkitApp.__new__(ToolkitApp)
    app._active_ws = behavior_workspace
    app._settings = settings
    app._small_font = None
    app._ws_map = {
        "behavior": behavior_workspace,
        "nif": nif_workspace,
    }

    app._switch_workspace("nif")

    assert behavior_workspace.active is False
    assert app._active_ws is nif_workspace
    assert settings.active_workspace == "nif"


def test_workspace_switch_keeps_current_workspace_active_when_init_fails():
    from ui.toolkit.app import ToolkitApp

    class FailingWorkspace(FakeWorkspace):
        def __init__(self, workspace_id: str):
            super().__init__(workspace_id)
            self._initialized = False

        def initialize(self) -> None:
            raise RuntimeError("boom")

    settings = SimpleNamespace(active_workspace="behavior")
    behavior_workspace = FakeWorkspace("behavior")
    behavior_workspace.active = True
    failing_workspace = FailingWorkspace("nif")

    app = ToolkitApp.__new__(ToolkitApp)
    app._active_ws = behavior_workspace
    app._settings = settings
    app._small_font = None
    app._ws_map = {
        "behavior": behavior_workspace,
        "nif": failing_workspace,
    }

    app._switch_workspace("nif")

    assert behavior_workspace.active is True
    assert failing_workspace.active is False
    assert app._active_ws is behavior_workspace
    assert settings.active_workspace == "behavior"
