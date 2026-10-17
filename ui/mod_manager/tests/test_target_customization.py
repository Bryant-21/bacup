"""Target-only metadata/source variants use inert executable and disposable file fixtures."""
import threading

import pytest

from creation_lib.mod_manager import target_mod_sources as sources, target_options as options, target_removal as removal
from creation_lib.mod_manager.launch_targets import TargetLibrary, records
from creation_lib.mod_manager.runtime import profile_signature
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_launch_targets import targets  # noqa: F401


@pytest.fixture
def context(targets, tmp_path):  # noqa: F811 - imported pytest fixture
    library, profile, games, ids = targets
    source = tmp_path / "shared mod"
    source.mkdir()
    (source / "file.txt").write_bytes(b"shared")
    with library._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind,version) VALUES ('mod','Shared mod',?,'external','1')", (str(source),))
        db.execute("INSERT INTO entries VALUES (?,'mod',0,1,'Logical group')", (profile,))
    return library, profile, games, ids, source


@pytest.mark.parametrize("argument", ["", "-v", "-waitforclose -priority above_normal -affinity 0x3", "-priority high"])
def test_argument_review_apply_invalidates_runtime_and_preserves_bootstrap_definition(context, argument):
    library, profile, _, ids, _ = context
    view = TargetLibrary(library, ids[0])
    before, signature = records(library), profile_signature(view, profile)
    plan = options.review(library, ids[0], argument)
    options.apply(library, plan)
    assert records(library) == before
    assert options.executable(library, view.target()).arguments == argument
    assert options.value(library, ids[1])["arguments"] == ""
    assert (profile_signature(view, profile) != signature) == bool(argument)


@pytest.mark.parametrize("argument", ["-altexe x.exe", "-altdll x.dll", "-launchsteam", "-priority realtime", "-affinity 0", "-affinity -1", "-v -v", "anything", "-waitfordebugger", "-notimeout", '-priority "unterminated'])
def test_argument_override_modes_and_invalid_masks_fail_closed(argument):
    with pytest.raises(ManagerError):
        options.validate(argument)


def test_source_version_enable_overrides_are_target_specific_keep_logical_ids_and_reset(context, tmp_path):
    library, profile, games, ids, shared = context
    variant = tmp_path / "older version"
    variant.mkdir()
    (variant / "file.txt").write_bytes(b"older")
    before = library.rows(profile)
    view = TargetLibrary(library, ids[0])
    signature = profile_signature(view, profile)
    plan = sources.review(library, ids[0], profile, "mod", source=str(variant), version="0.8", enabled=False)
    with pytest.raises(ManagerError, match="same mod"):
        sources.apply(library, plan)
    sources.apply(library, plan, accept_identity=True)
    row = view.profile_snapshot(profile)[1][0]
    assert (row.id, row.priority, row.group, row.source, row.version, row.enabled) == ("mod", 0, "Logical group", str(variant), "0.8", False)
    assert library.rows(profile) == before and TargetLibrary(library, ids[1]).profile_snapshot(profile)[1] == before
    assert profile_signature(view, profile) != signature
    sources.set_enabled(library, ids[0], profile, ["mod"], True)
    assert view.profile_snapshot(profile)[1][0].enabled
    assert view.profile_snapshot(profile)[1][0].source == str(variant)
    sources.apply(library, sources.review(library, ids[0], profile, "mod", reset=True))
    assert view.profile_snapshot(profile)[1] == before
    assert (shared / "file.txt").read_bytes() == b"shared" and (variant / "file.txt").read_bytes() == b"older"
    with pytest.raises(ManagerError):
        sources.review(library, ids[0], profile, "mod", source=str(games[0]))


@pytest.mark.parametrize("operation", ["options", "source", "remove"])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_new_target_metadata_operations_roll_back_at_commit_boundary(context, tmp_path, monkeypatch, operation, outcome):
    library, profile, _, ids, _ = context
    module = {"options": options, "source": sources, "remove": removal}[operation]
    if operation == "options":
        plan = options.review(library, ids[0], "-v")
    elif operation == "source":
        plan = sources.review(library, ids[0], profile, "mod", enabled=False)
    else:
        plan = removal.review(library, ids[0])
    before = records(library), library.rows(profile), profile_signature(TargetLibrary(library, ids[0]), profile)
    cancel = threading.Event()
    def fault():
        if outcome == "cancelled":
            cancel.set()
        else:
            raise OSError("Fixture SQL fault")
    monkeypatch.setattr(module, "_checkpoint", fault)
    with pytest.raises((Cancelled, OSError)):
        module.apply(library, plan, cancel=cancel)
    assert (records(library), library.rows(profile), profile_signature(TargetLibrary(library, ids[0]), profile)) == before


def test_removal_restore_preserves_all_owned_and_external_files_and_overlays(context):
    library, profile, games, ids, shared = context
    view = TargetLibrary(library, ids[0])
    owned = view.root / "runtime/profiles" / profile / "saves/inert.fos"
    owned.parent.mkdir(parents=True)
    owned.write_bytes(b"owned save")
    options.apply(library, options.review(library, ids[0], "-v"))
    sources.set_enabled(library, ids[0], profile, ["mod"], False)
    before = profile_signature(view, profile)
    plan = removal.review(library, ids[0])
    removal.apply(library, plan)
    assert len(records(library)) == 1 and removal.removed(library)[0].id == ids[0]
    assert owned.read_bytes() == b"owned save" and (shared / "file.txt").read_bytes() == b"shared"
    assert all((game / "Fallout4.exe").is_file() for game in games)
    removal.apply(library, removal.review(library, ids[0], restore=True))
    assert len(records(library)) == 2 and not removal.removed(library)
    assert profile_signature(TargetLibrary(library, ids[0]), profile) == before


def test_stale_shared_source_or_target_metadata_refuses_apply(context):
    library, profile, _, ids, _ = context
    plan = sources.review(library, ids[0], profile, "mod", enabled=False)
    with library._db() as db:
        db.execute("UPDATE mods SET version='Changed' WHERE id='mod'")
    with pytest.raises(ManagerError, match="changed after"):
        sources.apply(library, plan)
    plan = options.review(library, ids[0], "-v")
    options.apply(library, options.review(library, ids[0], "-waitforclose"))
    with pytest.raises(ManagerError, match="changed after"):
        options.apply(library, plan)


def test_source_inventory_change_and_owned_folder_refused(context, tmp_path):
    library, profile, _, ids, _ = context
    variant = tmp_path / "reviewed version"
    variant.mkdir()
    file = variant / "file.txt"
    file.write_bytes(b"first")
    plan = sources.review(library, ids[0], profile, "mod", source=str(variant))
    file.write_bytes(b"changed version")
    with pytest.raises(ManagerError, match="changed after"):
        sources.apply(library, plan, accept_identity=True)
    with pytest.raises(ManagerError):
        sources.review(library, ids[0], profile, "mod", source=str(library.root))
    assert not sources.records(library, ids[0])


@pytest.mark.parametrize("restore", [False, True])
def test_removal_restore_refuse_game_root_lock(context, restore):
    from creation_lib.mod_manager.f4se_root_setup import LOCK
    library, _, games, ids, _ = context
    if restore:
        removal.apply(library, removal.review(library, ids[0]))
    lock = games[0] / LOCK
    lock.write_bytes(b"foreign ownership")
    with pytest.raises(ManagerError, match="pending game-root"):
        removal.review(library, ids[0], restore=restore)
    assert lock.read_bytes() == b"foreign ownership"


def test_removal_refuses_pending_history_and_history_changed_after_review(context, monkeypatch):
    library, _, _, ids, _ = context
    monkeypatch.setattr("creation_lib.mod_manager.f4se_root_setup.history", lambda *_: [{"state": "pending"}])
    with pytest.raises(ManagerError, match="Restore/recover"):
        removal.review(library, ids[0])
    monkeypatch.setattr("creation_lib.mod_manager.f4se_root_setup.history", lambda *_: [])
    plan = removal.review(library, ids[0])
    root = library.root / ".launch-targets" / ids[0] / "f4se-root-history/retained"
    root.mkdir(parents=True)
    (root / "receipt.json").write_text("{}")
    with pytest.raises(ManagerError, match="history changed after"):
        removal.apply(library, plan)
    assert len(records(library)) == 2


def test_restore_refuses_changed_retained_receipt(context, monkeypatch):
    library, _, _, ids, _ = context
    monkeypatch.setattr("creation_lib.mod_manager.f4se_root_setup.history", lambda *_: [])
    root = library.root / ".launch-targets" / ids[0] / "f4se-root-history/retained"
    root.mkdir(parents=True)
    receipt = root / "receipt.json"
    receipt.write_text("{}")
    removal.apply(library, removal.review(library, ids[0]))
    receipt.write_text('{"changed":true}')
    with pytest.raises(ManagerError, match="changed after target removal"):
        removal.review(library, ids[0], restore=True)
    assert len(records(library)) == 1 and receipt.is_file()


@pytest.mark.parametrize("root_route", [False, True])
def test_optional_arguments_preserve_bootstrap_receipt_mappings_and_restore(tmp_path, monkeypatch, root_route):
    from creation_lib.mod_manager import f4se_root_setup as roots, f4se_setup
    from creation_lib.mod_manager.runtime import prepare
    from .test_f4se_root_setup import prepared, files
    from .test_runtime_setup import make_bundle
    library, profile, game, identifier = prepared(tmp_path, monkeypatch)
    before = files(game)
    if root_route:
        roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    target = records(library)[0]
    options.apply(library, options.review(library, identifier, "-v -priority above_normal"))
    view = TargetLibrary(library, identifier)
    # Target definition/root receipt stays intact; options live in their own metadata table.
    assert records(library)[0] == target
    f4se_setup.validate_ready(target, owner_root=view.root)
    plan = prepare(view, profile, options.executable(library, target), game, make_bundle(tmp_path / "bundle"), user_targets=(tmp_path / "plugins", tmp_path / "ini"))
    assert "-v" in plan.launch.argv and "above_normal" in plan.launch.argv
    assert any(name == "Scripts/F4SE.pex" for name, *_ in plan.files)
    assert plan.experimental != root_route
    assert ("-altexe" in plan.launch.argv) != root_route and ("-altdll" in plan.launch.argv) != root_route
    if root_route:
        roots.restore(library, roots.review_restore(library, identifier), accept_root_writes=True)
        assert options.value(library, identifier)["arguments"] == "-v -priority above_normal"
    assert files(game) == before
