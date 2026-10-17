"""No-copy additive updates, profile scope and transactional recovery."""
from contextlib import closing
from pathlib import Path

import pytest

from creation_lib.mod_manager import reference_overlay as overlays
from creation_lib.mod_manager.conflicts import build_index
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2
from .test_controller import finish
from .test_launch_targets import targets  # noqa: F401


@pytest.fixture
def fixture(tmp_path):
    lib = Library(tmp_path / "library", create=True)
    preview = inspect_instance(make_mo2(tmp_path))
    lib.import_mo2(preview)
    profile = next(p for p in lib.profiles() if p.name == "Default").id
    row = next(r for r in lib.rows(profile) if r.kind == "external" and r.enabled)
    root = Path(row.source)
    (root / "shared.txt").write_bytes(b"original shared")
    (root / "keep.txt").write_bytes(b"original unmatched")
    incoming = tmp_path / "incoming"
    incoming.mkdir()
    (incoming / "shared.txt").write_bytes(b"reviewed update")
    before = {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}
    yield lib, profile, row, incoming, preview
    assert {p: p.read_bytes() for p in before} == before


def apply(fixture, **kwargs):
    lib, profile, row, incoming, _ = fixture
    review = overlays.capture(lib, profile, row.id)
    with closing(Package(incoming)) as package:
        mod = overlays.install(lib, review, package, package.plain_plan(), "Owned update", **kwargs)
    return mod


def test_overlay_keeps_reference_and_unmatched_bytes_in_place_across_imports(fixture):
    lib, profile, row, _, preview = fixture
    mod = apply(fixture)
    owned = lib.root / "mods" / mod
    assert [p.name for p in owned.iterdir()] == ["shared.txt"]
    assert owned.joinpath("shared.txt").read_bytes() == b"reviewed update"
    rows = lib.rows(profile)
    assert row.name in next(r for r in rows if r.id == mod).notes
    assert next(r for r in rows if r.id == mod).priority == next(r for r in rows if r.id == row.id).priority + 1
    index = build_index(lib, profile)
    by_name = {entry.path: entry for entry in index.files}
    assert by_name["shared.txt"].winner.mod_id == mod
    assert by_name["keep.txt"].winner.mod_id == row.id
    for _ in range(2):
        lib.import_mo2(preview, reconcile=True)
        assert any(r.id == mod and r.enabled for r in lib.rows(profile))
    with lib._db() as db:
        assert len(overlays.records(db)) == 1


def test_overlay_follows_reference_enablement_and_stays_in_its_profile(fixture):
    lib, profile, row, _, _ = fixture
    mod = apply(fixture)
    other = lib.create_profile("Another", clone=profile)
    assert not next(r for r in lib.rows(other) if r.id == mod).enabled
    with lib._db() as db:
        db.execute("UPDATE entries SET enabled=0 WHERE profile_id=? AND mod_id=?", (profile, row.id))
    assert not next(r for r in lib.rows(profile) if r.id == mod).enabled
    with lib._db() as db:
        db.execute("UPDATE entries SET enabled=1 WHERE profile_id=? AND mod_id=?", (profile, row.id))
    assert next(r for r in lib.profile_snapshot(profile)[1] if r.id == mod).enabled
    with lib._db() as db:
        scoped = overlays.effective(lib.rows(profile), db, profile, "different-target")
    assert not next(r for r in scoped if r.id == mod).enabled


@pytest.mark.parametrize("checkpoint", ["prepared", "placed", "before-commit"])
@pytest.mark.parametrize("outcome", ["failed", "cancelled"])
def test_overlay_failure_and_cancel_roll_back_payload_and_association(fixture, monkeypatch, checkpoint, outcome):
    lib = fixture[0]
    def stop(name):
        if name == checkpoint:
            raise Cancelled() if outcome == "cancelled" else OSError("generated failure")
    monkeypatch.setattr(lib.owned, "_checkpoint", stop)
    with pytest.raises((Cancelled, OSError)):
        apply(fixture)
    reopened = Library(lib.root)
    with reopened._db() as db:
        assert not overlays.records(db)
    assert not list((lib.root / "mods").iterdir())
    assert not list((lib.root / ".staging").iterdir())


def test_overlay_uninstall_restore_preserves_recovery_association(fixture):
    lib, profile, row, _, _ = fixture
    mod = apply(fixture)
    operation = lib.uninstall(lib.review_owned(mod))
    entry, review = lib.review_restore(operation)
    lib.restore(operation, review, backup_token=entry["backup_token"])
    assert next(r for r in lib.rows(profile) if r.id == mod).enabled
    with lib._db() as db:
        assert overlays.records(db)[0]["base_id"] == row.id


@pytest.mark.parametrize("change", ["enable", "remove", "source"])
def test_overlay_stale_base_review_is_rejected_without_source_writes(fixture, change):
    lib, profile, row, incoming, _ = fixture
    review = overlays.capture(lib, profile, row.id)
    with lib._db() as db:
        if change == "enable":
            db.execute("UPDATE entries SET enabled=0 WHERE profile_id=? AND mod_id=?", (profile, row.id))
        elif change == "remove":
            db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, row.id))
        else:
            db.execute("UPDATE mods SET source=? WHERE id=?", (str(incoming), row.id))
    with closing(Package(incoming)) as package, pytest.raises(ManagerError):
        overlays.install(lib, review, package, package.plain_plan(), "Blocked")
    assert not list((lib.root / "mods").iterdir())


def test_controller_overlay_uses_the_same_review_and_cancel_guards(fixture):
    lib, profile, row, incoming, _ = fixture
    m = ManagerController()
    try:
        m.library, m.profile_id = lib, profile
        m.refresh()
        m.prepare_install(incoming, overlay_id=row.id)
        finish(m)
        assert m.prepared.overlay_review.source == row.source
        m.review_install()
        finish(m)
        m.install("Controller update")
        finish(m)
        assert next(r for r in m.rows if r.id == m.selected_id).kind == "owned"
    finally:
        m.cleanup()


def test_real_target_views_route_patch_only_to_selected_target_and_pin_plugin_snapshot(targets, tmp_path):  # noqa: F811 - imported pytest fixture
    from creation_lib.mod_manager.launch_targets import TargetLibrary
    from creation_lib.mod_manager import plugin_state, target_mod_sources
    from creation_lib.mod_manager.runtime import profile_signature
    lib, profile, _, ids = targets
    source = tmp_path / "referenced base"
    source.mkdir()
    (source / "base.txt").write_bytes(b"original")
    with lib._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind) VALUES ('reference','Referenced base',?,'external')", (str(source),))
        db.execute("INSERT INTO entries VALUES (?,'reference',0,1,'')", (profile,))
    incoming = tmp_path / "patch"
    incoming.mkdir()
    (incoming / "patch.txt").write_bytes(b"owned")
    a, b = (TargetLibrary(lib, identifier) for identifier in ids)
    review = overlays.capture(a, profile, "reference")
    with closing(Package(incoming)) as package:
        mod = overlays.install(a, review, package, package.plain_plan(), "Target patch")
    assert next(r for r in a.profile_snapshot(profile)[1] if r.id == mod).enabled
    assert not next(r for r in b.profile_snapshot(profile)[1] if r.id == mod).enabled
    assert not next(r for r in lib.rows(profile) if r.id == mod).enabled
    before = profile_signature(a, profile)
    assert plugin_state.inventory(a, profile).signature == plugin_state.signature(a, profile)
    plan = target_mod_sources.review(lib, ids[0], profile, "reference", enabled=False)
    target_mod_sources.apply(lib, plan)
    assert not next(r for r in a.profile_snapshot(profile)[1] if r.id == mod).enabled
    assert profile_signature(a, profile) != before
    assert plugin_state.inventory(a, profile).signature == plugin_state.signature(a, profile)
    assert (source / "base.txt").read_bytes() == b"original"


def test_target_source_substitution_invalidates_additive_review_and_disables_old_patch(targets, tmp_path):  # noqa: F811 - imported pytest fixture
    from creation_lib.mod_manager.launch_targets import TargetLibrary
    from creation_lib.mod_manager import target_mod_sources
    lib, profile, _, ids = targets
    source = tmp_path / "original"
    source.mkdir()
    (source / "payload.txt").write_bytes(b"original")
    replacement = tmp_path / "replacement"
    replacement.mkdir()
    (replacement / "payload.txt").write_bytes(b"different reference")
    with lib._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind) VALUES ('reference','Referenced base',?,'external')", (str(source),))
        db.execute("INSERT INTO entries VALUES (?,'reference',0,1,'')", (profile,))
    view = TargetLibrary(lib, ids[0])
    review = overlays.capture(view, profile, "reference")
    with closing(Package(replacement)) as package:
        mod = overlays.install(view, review, package, package.plain_plan(), "Owned overlay")
        plan = target_mod_sources.review(lib, ids[0], profile, "reference", source=str(replacement))
        target_mod_sources.apply(lib, plan, accept_identity=True)
        with pytest.raises(ManagerError, match="changed"):
            overlays.install(view, review, package, package.plain_plan(), "Stale overlay")
    assert not next(r for r in view.profile_snapshot(profile)[1] if r.id == mod).enabled
    assert (source / "payload.txt").read_bytes() == b"original"
    assert (replacement / "payload.txt").read_bytes() == b"different reference"
