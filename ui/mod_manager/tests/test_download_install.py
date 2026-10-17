"""Read-only MO2 downloads and explicit owned update destinations, using tiny ZIPs."""
import hashlib
import json
from pathlib import Path
import threading
import uuid
import zipfile

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.download_install import capture, destination, validate
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.mo2_migration import augment
from creation_lib.mod_manager.nexus_state import File, State
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2
from .test_controller import finish
from .test_launch_targets import targets  # noqa: F401


def archive(path, content=b"download payload"):
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as writer:
        writer.writestr("file.txt", content)
    return path


def owned_download(library, *, file_id=2):
    path = archive(library.root / "downloads" / uuid.uuid4().hex / "replacement.zip", b"replacement")
    with library._db() as db:
        db.execute("INSERT INTO downloads VALUES ('fallout4',123,?,?, '2',?)", (file_id, str(path), hashlib.sha256(path.read_bytes()).hexdigest()))
    return path


def install_owned(library, profile, tmp_path, name="Owned", file_id=1):
    path = archive(tmp_path / (name + ".zip"), b"original " + name.encode())
    package = Package(path)
    try:
        return library.install(package, package.plain_plan(), name, profile,
                               nexus={"game": "fallout4", "mod_id": 123, "file_id": file_id, "version": "1"})
    finally:
        package.close()


@pytest.fixture
def imported(tmp_path):
    source = make_mo2(tmp_path)
    path = archive(source / "downloads" / "imported.zip")
    Path(str(path) + ".meta").write_text("[General]\ngameName=Fallout4\nmodID=123\nfileID=2\nversion=2\n")
    preview = inspect_instance(source)
    augment(preview)
    manager = ManagerController()
    manager.library = Library(tmp_path / "manager", create=True)
    manager.library.import_mo2(preview)
    manager.refresh()
    yield manager, path
    manager.cleanup()


def test_imported_download_direct_review_install_preserves_source_and_repeats_safely(imported):
    manager, path = imported
    before = {str(p): p.read_bytes() for p in path.parent.parent.rglob("*") if p.is_file()}
    manager.review_download(path, imported=True)
    finish(manager)
    review = manager.download_review
    assert review.imported and review.metadata() == {"game": "fallout4", "mod_id": 123, "file_id": 2, "version": "2"}
    assert not list((manager.library.root / "mods").iterdir())
    manager.prepare_install(path, download_review=review)
    finish(manager)
    manager.review_install()
    finish(manager)
    manager.install("Imported archive")
    finish(manager)
    installed = manager.selected_id
    assert manager.rows_by_id[installed].nexus_file_id == 2
    assert (manager.library.root / "mods" / installed / "file.txt").read_bytes() == b"download payload"
    assert not list((manager.library.root / "downloads").iterdir())
    assert before == {str(p): p.read_bytes() for p in path.parent.parent.rglob("*") if p.is_file()}
    manager.review_download(path, imported=True)
    finish(manager)
    manager.prepare_install(path, download_review=manager.download_review)
    finish(manager)
    manager.review_install()
    finish(manager)
    manager.install("Imported archive")
    assert isinstance(manager._future.exception(timeout=10), ManagerError)
    manager.poll()
    assert "already has this name" in manager.error and len([row for row in manager.rows if row.kind == "owned"]) == 1
    assert not list((manager.library.root / ".staging").iterdir())


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
@pytest.mark.parametrize("replacement", [False, True])
def test_download_install_rollback_retains_archive_and_original_owned_content(imported, tmp_path, monkeypatch, outcome, replacement):
    manager, imported_path = imported
    path = imported_path
    mod_id = ""
    if replacement:
        mod_id = install_owned(manager.library, manager.profile_id, tmp_path)
        path = owned_download(manager.library)
        manager.refresh()
    before = manager.library.rows(manager.profile_id)
    originals = {str(p): p.read_bytes() for p in (manager.library.root / "mods").rglob("*") if p.is_file()}
    manager.review_download(path, imported=not replacement)
    finish(manager)
    manager.prepare_install(path, mod_id, download_review=manager.download_review)
    finish(manager)
    manager.review_install()
    finish(manager)
    source_bytes = path.read_bytes()
    def stop(stage):
        if stage == "placed":
            if outcome == "cancelled":
                manager.cancel()
            else:
                raise OSError("generated archive install failure")
    monkeypatch.setattr(manager.library.owned, "_checkpoint", stop)
    manager.install("Owned" if replacement else "Never installed")
    assert isinstance(manager._future.exception(timeout=10), Cancelled if outcome == "cancelled" else OSError)
    manager.poll()
    assert manager.job_state == outcome and manager.library.rows(manager.profile_id) == before
    assert {str(p): p.read_bytes() for p in (manager.library.root / "mods").rglob("*") if p.is_file()} == originals
    assert path.read_bytes() == source_bytes and manager.prepared is not None
    assert not manager.library.history() and not list((manager.library.root / ".staging").iterdir())


def test_replacement_candidates_ambiguous_owned_and_references_require_explicit_destination(imported, tmp_path):
    manager, _ = imported
    first = install_owned(manager.library, manager.profile_id, tmp_path, "Owned A", 1)
    second = install_owned(manager.library, manager.profile_id, tmp_path, "Owned B", 3)
    referenced = next(row for row in manager.rows if row.kind == "external")
    with manager.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1 WHERE id=?", (referenced.id,))
    path = owned_download(manager.library)
    manager.refresh()
    state = State("fallout4", 123, (File(2, "replacement.zip", "2", "Main"),), ((1, 2), (3, 2)))
    review = capture(manager.context_library, manager.profile_id, path, states={("fallout4", 123): state})
    eligible = [candidate for candidate in review.candidates if candidate.eligible]
    assert {item.id for item in eligible} == {first, second} and all(item.published for item in eligible)
    assert destination(review, "") == "" and destination(review, second) == second
    for forbidden in (referenced.id, "not-reviewed"):
        with pytest.raises(ManagerError, match="manager-owned"):
            destination(review, forbidden)
    clone = manager.library.create_profile("Clone", manager.profile_id)
    manager.library.customize(clone, second, enabled=False, group="Keep/Sub")
    # A changed profile set must receive another archive/destination review.
    review = capture(manager.context_library, manager.profile_id, path)
    manager.prepare_install(path, second, download_review=review)
    finish(manager)
    assert len(manager.prepared.replacement_review.profiles) == 2
    manager.review_install()
    finish(manager)
    manager.install("Owned B")
    finish(manager)
    assert manager.selected_id == second and manager.rows_by_id[first].nexus_file_id == 1
    assert manager.rows_by_id[second].nexus_file_id == 2
    cloned = next(row for row in manager.library.rows(clone) if row.id == second)
    assert not cloned.enabled and cloned.group == "Keep/Sub"
    assert Path(referenced.source).is_dir() and manager.library.history()[0]["mod_id"] == second


@pytest.mark.parametrize("fault", ["archive", "metadata", "destination", "profile", "outside", "missing"])
def test_frozen_download_review_refuses_stale_and_redirected_sources(imported, tmp_path, fault):
    manager, path = imported
    review = capture(manager.library, manager.profile_id, path, imported=True)
    if fault == "archive":
        path.write_bytes(path.read_bytes() + b"changed")
    elif fault == "metadata":
        with manager.library._db() as db:
            row = db.execute("SELECT id,configuration FROM instances").fetchone()
            config = json.loads(row[1])
            config["download_inventory"][0]["metadata"]["version"] = "later"
            db.execute("UPDATE instances SET configuration=? WHERE id=?", (json.dumps(config), row[0]))
    elif fault == "destination":
        manager.library.customize(manager.profile_id, manager.rows[0].id, notes="later")
    elif fault == "profile":
        manager.profile_id = manager.library.create_profile("Other")
    elif fault == "outside":
        redirected = archive(tmp_path / "unrelated.zip")
        with manager.library._db() as db:
            row = db.execute("SELECT id,configuration FROM instances").fetchone()
            config = json.loads(row[1])
            config["download_inventory"][0]["source"] = str(redirected)
            db.execute("UPDATE instances SET configuration=? WHERE id=?", (json.dumps(config), row[0]))
    else:
        path.unlink()
    with pytest.raises(ManagerError):
        validate(manager.context_library, manager.profile_id, review)
    assert not list((manager.library.root / "mods").iterdir())


def test_completed_archive_checksum_is_verified_before_destination_review(imported):
    manager, _ = imported
    path = owned_download(manager.library)
    path.write_bytes(path.read_bytes() + b"tampered")
    with pytest.raises(ManagerError, match="checksum"):
        capture(manager.library, manager.profile_id, path)


def test_target_source_override_cannot_reinstall_referenced_variant(targets, tmp_path):  # noqa: F811
    from creation_lib.mod_manager.launch_targets import TargetLibrary
    from creation_lib.mod_manager import target_mod_sources as sources
    library, profile, _, ids = targets
    mod = install_owned(library, profile, tmp_path)
    archive_path = owned_download(library)
    view = TargetLibrary(library, ids[0])
    original = capture(view, profile, archive_path)
    assert next(item for item in original.candidates if item.id == mod).eligible
    variant = tmp_path / "variant"
    variant.mkdir()
    (variant / "file.txt").write_bytes(b"referenced variant")
    sources.apply(library, sources.review(library, ids[0], profile, mod, source=str(variant), version="variant", enabled=False), accept_identity=True)
    with pytest.raises(ManagerError, match="metadata changed"):
        validate(view, profile, original)
    review = capture(view, profile, archive_path)
    candidate = next(item for item in review.candidates if item.id == mod)
    assert not candidate.eligible and candidate.file_id == 0
    with pytest.raises(ManagerError):
        destination(review, mod)
    assert capture(TargetLibrary(library, ids[1]), profile, archive_path).candidates[0].eligible
    assert (variant / "file.txt").read_bytes() == b"referenced variant"


def test_cancelled_destination_review_does_not_prepare_package(imported):
    manager, path = imported
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        capture(manager.library, manager.profile_id, path, imported=True, cancel=cancel)
    assert manager.prepared is None and not list((manager.library.root / "mods").iterdir())
