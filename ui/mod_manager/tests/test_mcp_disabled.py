from dataclasses import replace
import json

import pytest

from creation_lib.mod_manager.backend import call
from creation_lib.mod_manager import runtime
from creation_lib.mod_manager.runtime_helper import run
from .test_runtime import fixture as fixture, FakeVFS, request


def test_disabled_mcp_allocates_no_port_or_lock_directory(tmp_path):
    directory = tmp_path / "leases"
    session = call("McpSession", directory, disabled=True)
    assert session.disabled and session.port is None and session.endpoint is None
    assert session.environment == {"B21_MCP_DISABLED": "1"}
    assert not directory.exists()
    session.close()
    with pytest.raises(ValueError, match="disabled"):
        call("McpSession", directory, 8922, True)


def test_helper_passes_disable_to_game_and_reports_no_endpoint(fixture):
    library, profile, executable, game, bundle, targets = fixture
    plan = runtime.prepare(library, profile, executable, str(game), str(bundle), user_targets=targets)
    path = request(library, replace(plan, disable_mcp=True))

    class Disabled(FakeVFS):
        def start(self, argv, cwd, **kwargs):
            assert kwargs["environment"]["B21_MCP_DISABLED"] == "1"
            assert "B21_MCP_PORT" not in kwargs["environment"]
            return super().start(argv, cwd, **kwargs)

    assert run(path, native_factory=Disabled, wait=lambda _: None) == 0
    status = json.loads((path.parent / "status.json").read_text())
    assert status["mcp_disabled"] and status["mcp_port"] is None and status["mcp_endpoint"] is None


def test_disabled_setting_survives_workspace_reload(tmp_path):
    from ui.mod_manager.workspace import ModManagerWorkspace
    from ui.toolkit.settings import ToolkitSettings
    path = tmp_path / "settings.json"
    settings = ToolkitSettings(path=path, editor_settings_path=tmp_path / "absent.json")
    original, restored = ModManagerWorkspace(), ModManagerWorkspace()
    try:
        original.launcher.disable_mcp = True
        settings.set_workspace_settings("mod_manager", original.collect_settings())
        settings.save()
        loaded = ToolkitSettings(path=path, editor_settings_path=tmp_path / "absent.json")
        restored.apply_settings(loaded.get_workspace_settings("mod_manager"))
        assert restored.launcher.disable_mcp is True
        assert restored.collect_settings()["disable_mcp"] is True
    finally:
        original.manager.cleanup()
        restored.manager.cleanup()


def test_direct_launch_inherits_disabled_override(tmp_path):
    from creation_lib.mod_manager.launcher import definition, review, start
    binary = tmp_path / "fixture.exe"
    binary.write_bytes(b"inert fixture; never executed")
    plan = review(definition(id="fixture", title="Fixture", binary=str(binary), mode="direct"))
    calls = []
    start(plan, disable_mcp=True, spawn=lambda *args, **kwargs: calls.append(kwargs))
    assert calls[0]["env"]["B21_MCP_DISABLED"] == "1"


def test_disabled_and_explicit_port_rejected_before_library_access(tmp_path):
    from creation_lib.mod_manager.agent_launch import launch
    with pytest.raises(ValueError, match="disabled"):
        launch(tmp_path / "absent", "Default", disable_mcp=True, mcp_port=8922, dry_run=True)
    assert not (tmp_path / "absent").exists()
