import json
from types import SimpleNamespace

import pytest
from click.testing import CliRunner

from cli.main import cli
from creation_lib.mod_manager.agent_launch import actions, create_instance, workflow
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import ManagerError
from .test_standalone import game_fixture


@pytest.fixture
def library(tmp_path):
    root = tmp_path / "library"
    create_instance(root, "fo4", str(game_fixture(tmp_path)), "Default")
    return Library(root)


def run(library, *steps, apply=False, profile="Default"):
    return workflow(library.root, profile, list(steps), apply=apply)["results"]


def test_contract_rejects_internal_actions_and_preflights_all_mutations(library):
    catalog = actions()
    assert len(catalog["actions"]) >= 90
    assert catalog["atomic"] is False
    for steps in [[{"action": "cleanup"}], [{"action": "create_profile", "args": ["Forbidden"]}],
                  [{"action": "create_profile", "args": ["Forbidden"]}, {"action": "bad"}]]:
        with pytest.raises(ManagerError):
            workflow(library.root, "Default", steps)
    assert len(library.profiles()) == 1


def test_profiles_organization_ini_and_recovery_share_controller_state(library):
    created = run(library, {"action": "create_empty_mod", "args": ["CLI mod"]}, apply=True)[0]
    mod = created["selected_id"]
    run(library, {"action": "customize", "args": [mod], "options": {"enabled": False, "notes": "Shared"}}, apply=True)
    row = library.rows(library.profiles()[0].id)[0]
    assert not row.enabled and row.notes == "Shared"
    clone = run(library, {"action": "create_profile", "args": ["Clone", True]}, apply=True)[0]["profile_id"]
    assert library.rows(clone)[0].notes == "Shared"
    run(library, {"action": "rename_profile", "args": ["Renamed"]}, apply=True, profile=clone)
    run(library, {"action": "delete_profile", "args": [clone]}, apply=True)
    run(library, {"action": "restore_profile", "args": [clone]}, apply=True)
    assert "Renamed" in {p.name for p in library.profiles()}
    before = run(library, {"action": "ini", "args": ["Fallout4Custom.ini"]})[0]["result"]
    assert isinstance(before, str)
    run(library, {"action": "save_profile_ini", "args": ["Fallout4Custom.ini", "[General]\nbAlwaysActive=1\n", before]}, apply=True)
    assert "bAlwaysActive=1" in run(library, {"action": "ini", "args": ["Fallout4Custom.ini"]})[0]["result"]
    result = run(library, {"action": "review_owned", "args": [mod]}, {"action": "uninstall"}, apply=True)
    operation = result[-1]["recovery_history"][-1]["id"]
    assert not library.rows(library.profiles()[0].id)
    run(library, {"action": "review_restore", "args": [operation]}, {"action": "restore"}, apply=True)
    assert library.rows(library.profiles()[0].id)[0].id == mod


def test_files_categories_and_read_surfaces(library):
    for action, field in [("profiles", "profiles"), ("mods", "rows"), ("executables", "executables"),
                          ("targets", "targets"), ("downloads", "download_jobs"), ("history", "recovery_history"),
                          ("inspect_files", "file_index"), ("inspect_archives", "archive_index"),
                          ("inspect_combined_data", "combined_index"), ("inspect_saves", "save_index"),
                          ("inspect_save_backups", "save_backups"), ("inspect_categories", "categories")]:
        assert field in run(library, {"action": action})[0]
    run(library, {"action": "edit_category", "args": ["save", "Weapons"]}, apply=True)
    assert run(library, {"action": "inspect_categories"})[0]["categories"]


def test_plugin_review_and_apply_keep_dependency_rules(library, monkeypatch):
    import creation_lib.mod_manager.plugin_state as plugins
    game = library.root.parent / "game"
    from pathlib import Path
    for name in ["Fallout4.esm", "Master.esm", "Patch.esp"]:
        (Path(game) / "Data" / name).write_bytes(b"header fixture")
    monkeypatch.setattr(plugins, "modkit_metadata", lambda path, game, cancel=None: (
        {"flags": {"esm": path.suffix == ".esm"}}, {"Fallout4.esm": (), "Master.esm": ("Fallout4.esm",), "Patch.esp": ("Master.esm",)}[path.name]))
    enabled = ["Fallout4.esm", "Master.esm", "Patch.esp"]
    result = run(library, {"action": "review_plugins", "options": {"enabled": enabled}}, {"action": "apply_plugins"}, apply=True)
    assert result[0]["result"]["plan"]["order"] == enabled
    with pytest.raises(ManagerError):
        run(library, {"action": "review_plugins", "options": {"order": list(reversed(enabled)), "enabled": enabled}}, {"action": "apply_plugins"}, apply=True)
    assert library.profiles()[0].files["loadorder.txt"].splitlines() == enabled


def test_cli_click_routes_commands_to_native_backend(library):
    runner = CliRunner()
    base = ["--format", "json", "manager"]
    for command in ["profiles", "mods", "targets", "executables", "downloads", "files", "saves"]:
        result = runner.invoke(cli, [*base, command, "--library", str(library.root)])
        assert result.exit_code == 0, result.output
        assert json.loads(result.output)["results"]
    result = runner.invoke(cli, [*base, "profiles", "create", "--library", str(library.root), "--name", "CLI", "--apply"])
    assert result.exit_code == 0, result.output
    assert len(library.profiles()) == 2


def test_update_failure_through_cli_persists_rate_backoff(library, monkeypatch):
    from creation_lib.mod_manager.controller import ManagerController
    from creation_lib.mod_manager.nexus import NexusError
    original = ManagerController.refresh
    def refresh(manager):
        original(manager)
        manager.rows = [SimpleNamespace(nexus_game="fallout4", nexus_mod_id=42)]
    def limited(*args):
        raise NexusError("Rate limited", 7200, http_status=429)
    client = SimpleNamespace(user={"user_id": 42}, mod_info=limited, forget=lambda: None)
    monkeypatch.setattr(ManagerController, "refresh", refresh)
    monkeypatch.setattr(ManagerController, "connect_nexus", lambda manager: setattr(manager, "nexus_client", client))
    with pytest.raises(ManagerError, match="Rate limited"):
        run(library, {"action": "updates", "options": {"check": True}})
    result = run(library, {"action": "updates"})[0]["result"]
    assert result["next_check_seconds"] > 7000


def test_download_receipt_recovery_needs_no_saved_account(library, monkeypatch):
    from pathlib import Path
    from creation_lib.mod_manager.download_queue import DownloadQueue
    from creation_lib.mod_manager.nexus import parse_nxm
    from .test_download_queue import CDN, LINK
    queue, cdn = DownloadQueue(library), CDN()
    identifier = queue.enqueue(parse_nxm(LINK))
    def interrupted(*args, **kwargs):
        raise SystemExit("interrupted before publishing")
    monkeypatch.setattr(queue, "_finish", interrupted)
    with pytest.raises(SystemExit):
        queue.run(identifier, cdn.client())
    result = run(library, {"action": "resume_download", "args": [identifier]}, apply=True)[0]
    assert Path(result["download_records"][0]["path"]).read_bytes() == cdn.payload
