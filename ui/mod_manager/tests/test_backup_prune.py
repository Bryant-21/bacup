"""Disposable bundle deletion, rollback, crash recovery and byte-preservation journeys."""
from pathlib import Path
import json
import subprocess
import threading

import pytest

from creation_lib.mod_manager import backup_prune as prune
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.transactions import CommittedTransaction, digest
from .test_save_transfer import owned as owned_fixture


def fixture(tmp_path, kind):
    library, profile, _, path = owned_fixture.__wrapped__(tmp_path)
    if kind == "save":
        from creation_lib.mod_manager.save_transfer import apply, review
        from creation_lib.mod_manager.saves import inventory
        source = path.parent
        first = Path(apply(library, review(library, inventory(library, profile), str(path))))
        second = Path(apply(library, review(library, inventory(library, profile), str(path))))
    else:
        from creation_lib.mod_manager.overwrite import clear, review, root
        source = root(library, profile)
        (source / "generated" / "empty").mkdir(parents=True)
        (source / "generated" / "output.txt").write_bytes(b"first generated output")
        identifier = clear(library, review(library, profile))
        first = library.root / ".overwrite-history" / profile / identifier
        (source / "second.txt").write_bytes(b"second generated output")
        identifier = clear(library, review(library, profile))
        second = first.parent / identifier
        (source / "keep.txt").write_bytes(b"current runtime output")
    return library, profile, source, first, second


def files(path):
    return {str(item.relative_to(path)): item.read_bytes() for item in path.rglob("*") if item.is_file()}


@pytest.mark.parametrize("kind", ["save", "overwrite"])
def test_remove_one_bundle_preserves_current_output_other_backups_and_repeats(tmp_path, kind):
    library, profile, source, first, second = fixture(tmp_path, kind)
    original, retained = files(source), files(second)
    plan = prune.review(library, kind, profile, first.name)
    assert plan.files == len(files(first)) and plan.bytes == sum(map(len, files(first).values()))
    assert prune.remove(library, plan) == first.name
    assert not first.exists() and files(source) == original and files(second) == retained
    assert not list((library.root / ".backup-prune").iterdir())
    reopened = Library(library.root)
    with reopened._db() as db:
        assert not db.execute("SELECT * FROM backup_prunes").fetchall()
    with pytest.raises(ManagerError, match="no longer exists"):
        prune.remove(reopened, plan)


@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("checkpoint", ["receipt", "moved", "metadata"])
@pytest.mark.parametrize("outcome", ["cancel", "failure"])
def test_precommit_cancel_and_failure_restore_the_whole_bundle(tmp_path, monkeypatch, kind, checkpoint, outcome):
    library, profile, source, first, second = fixture(tmp_path, kind)
    originals = files(first), files(source), files(second)
    plan, cancel = prune.review(library, kind, profile, first.name), threading.Event()
    def stop(stage):
        if stage == checkpoint:
            if outcome == "cancel":
                cancel.set()
            else:
                raise OSError("disposable bundle deletion failure")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancel" else OSError):
        prune.remove(library, plan, cancel)
    assert (files(first), files(source), files(second)) == originals
    assert not list((library.root / ".backup-prune").iterdir())


@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("checkpoint", ["committed", "file-removed", "receipt-removed"])
def test_committed_cleanup_interruption_finishes_on_reopen(tmp_path, monkeypatch, kind, checkpoint):
    library, profile, source, first, second = fixture(tmp_path, kind)
    originals = files(source), files(second)
    plan = prune.review(library, kind, profile, first.name)
    def stop(stage):
        if stage == checkpoint:
            raise OSError("disposable committed cleanup interruption")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(CommittedTransaction, match="deletion committed"):
        prune.remove(library, plan)
    monkeypatch.setattr(prune, "_checkpoint", lambda stage: None)
    reopened = Library(library.root)
    assert not first.exists() and not list((library.root / ".backup-prune").iterdir())
    assert (files(source), files(second)) == originals
    with reopened._db() as db:
        assert not db.execute("SELECT * FROM backup_prunes").fetchall()


@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("change", ["contents", "added", "directory", "receipt"])
def test_stale_review_preserves_bundle_and_current_payload(tmp_path, kind, change):
    library, profile, source, first, second = fixture(tmp_path, kind)
    plan, original = prune.review(library, kind, profile, first.name), files(source)
    if change == "contents":
        payload = next(item for item in first.rglob("*") if item.is_file() and item.name != "receipt.json")
        payload.write_bytes(b"changed retained bytes")
    elif change == "added":
        (first / "unexpected.txt").write_bytes(b"retain new file")
    elif change == "directory":
        (first / "unexpected-empty").mkdir()
    else:
        path = first / "receipt.json"
        path.write_text(json.dumps(json.loads(path.read_text()), indent=2))
    retained = files(first)
    with pytest.raises(ManagerError):
        prune.remove(library, plan)
    assert files(first) == retained and files(source) == original and second.is_dir()


@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("change", ["contents", "added", "directory", "receipt", "original", "marker"])
def test_committed_recovery_preserves_unexpected_content(tmp_path, monkeypatch, kind, change):
    library, profile, source, first, second = fixture(tmp_path, kind)
    def stop(stage):
        if stage == "committed":
            raise OSError("disposable pending cleanup")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(CommittedTransaction):
        prune.remove(library, prune.review(library, kind, profile, first.name))
    journal = next((library.root / ".backup-prune").glob("*.json"))
    record = json.loads(journal.read_text())
    bundle = library.root / ".backup-prune" / record["id"] / "bundle"
    if change == "contents":
        next(item for item in bundle.rglob("*") if item.is_file()).write_bytes(b"changed retained bytes")
    elif change == "added":
        (bundle / "unexpected.txt").write_bytes(b"keep new file")
    elif change == "directory":
        (bundle / "unexpected-empty").mkdir()
    elif change == "receipt":
        record["manifest"]["../outside.txt"] = record["manifest"].pop("receipt.json")
        record["token"] = digest((record["library"], record["kind"], record["profile"], record["backup"], record["manifest"], record["directories"], record["identity"]))
        journal.write_text(json.dumps(record))
    elif change == "original":
        first.mkdir()
        (first / "keep.txt").write_bytes(b"unrelated replacement")
    else:
        with library._db() as db:
            db.execute("UPDATE backup_prunes SET token=? WHERE id=?", ("0" * 64, record["id"]))
    retained = files(bundle), files(source), files(second)
    monkeypatch.setattr(prune, "_checkpoint", lambda stage: None)
    with pytest.raises(ManagerError):
        prune.recover(library)
    assert bundle.is_dir() and (files(bundle), files(source), files(second)) == retained
    assert journal.is_file()


@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("checkpoint", ["receipt", "moved", "committed"])
def test_real_process_exit_recovers_according_to_commit_marker(tmp_path, kind, checkpoint):
    library, profile, source, first, second = fixture(tmp_path, kind)
    originals = files(first), files(source), files(second)
    code = """import os,sys
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager import backup_prune as prune
library=Library(sys.argv[1])
plan=prune.review(library,sys.argv[2],sys.argv[3],sys.argv[4])
prune._checkpoint=lambda stage: os._exit(83) if stage==sys.argv[5] else None
prune.remove(library,plan)
"""
    result = subprocess.run(["uv", "run", "--no-sync", "python", "-c", code, str(library.root), kind, profile, first.name, checkpoint], capture_output=True, timeout=30)
    assert result.returncode == 83, result.stderr.decode(errors="replace")
    reopened = Library(library.root)
    assert (files(source), files(second)) == originals[1:]
    assert not first.exists() if checkpoint == "committed" else files(first) == originals[0]
    assert reopened.recovery_report and not list((library.root / ".backup-prune").iterdir())


def test_restored_overwrite_receipt_removal_keeps_restored_output(tmp_path):
    from creation_lib.mod_manager.overwrite import restore, review_restore
    library, profile, source, first, _ = fixture(tmp_path, "overwrite")
    (source / "keep.txt").unlink()
    restore(library, first.name, review_restore(library, profile, first.name))
    original = files(source)
    plan = prune.review(library, "overwrite", profile, first.name)
    assert plan.files == 1
    prune.remove(library, plan)
    assert files(source) == original and not first.exists()


@pytest.mark.parametrize("kind", ["save", "overwrite"])
def test_runtime_and_pre_cancel_do_not_move_any_bundle(tmp_path, monkeypatch, kind):
    library, profile, source, first, _ = fixture(tmp_path, kind)
    plan, original = prune.review(library, kind, profile, first.name), files(first)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        prune.remove(library, plan, cancel)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda root: [{}])
    with pytest.raises(ManagerError, match="runtime"):
        prune.remove(library, plan)
    assert files(first) == original and source.is_dir()


@pytest.mark.parametrize("kind", ["save", "overwrite"])
@pytest.mark.parametrize("moment", ["receipt", "moved"])
def test_mid_staging_mutation_rolls_back_and_preserves_new_content(tmp_path, monkeypatch, kind, moment):
    library, profile, source, first, _ = fixture(tmp_path, kind)
    plan, original = prune.review(library, kind, profile, first.name), files(source)
    def mutate(stage):
        if stage == moment:
            location = first if moment == "receipt" else next((library.root / ".backup-prune").glob("*/bundle"))
            (location / "unexpected.txt").write_bytes(b"new content after review")
    monkeypatch.setattr(prune, "_checkpoint", mutate)
    with pytest.raises(ManagerError, match="changed while staging"):
        prune.remove(library, plan)
    assert first.joinpath("unexpected.txt").read_bytes() == b"new content after review"
    assert files(source) == original
    with library._db() as db:
        assert not db.execute("SELECT * FROM backup_prunes").fetchall()


@pytest.mark.parametrize("kind", ["save", "overwrite"])
def test_nested_junction_is_never_followed_or_removed(tmp_path, kind):
    from .test_mo2 import junction
    library, profile, source, first, _ = fixture(tmp_path, kind)
    outside = tmp_path / "unrelated fixture"
    outside.mkdir()
    (outside / "keep.txt").write_bytes(b"never follow this")
    link = first / "linked"
    junction(link, outside)
    try:
        with pytest.raises(ManagerError):
            prune.review(library, kind, profile, first.name)
        assert (outside / "keep.txt").read_bytes() == b"never follow this" and source.is_dir()
    finally:
        link.rmdir()
