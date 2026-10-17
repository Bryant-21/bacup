"""Installer context freshness through Review and the actual install commit boundary."""
from pathlib import Path

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import make_mo2
from .test_controller import finish


@pytest.fixture
def prepared(tmp_path):
    manager = ManagerController()
    source = make_mo2(tmp_path)
    manager.open_library(tmp_path / "library", create=True)
    finish(manager)
    manager.inspect_mo2(source)
    finish(manager)
    manager.import_profiles({manager.import_preview.profiles[0].id})
    finish(manager)
    package = tmp_path / "dependency package"
    (package / "fomod").mkdir(parents=True)
    (package / "base.txt").write_bytes(b"new payload must not be placed when stale")
    (package / "fomod/ModuleConfig.xml").write_text('<config><moduleName>Dependency fixture</moduleName><moduleDependencies><fileDependency file="Fixture.esp" state="Active"/></moduleDependencies><requiredInstallFiles><file source="base.txt"/></requiredInstallFiles></config>')
    manager.prepare_install(package)
    finish(manager)
    yield manager, package, source
    manager.cleanup()


@pytest.mark.parametrize("fault", ["disabled", "added-asset", "changed-asset", "profile", "plugins"])
def test_prepared_dependency_context_cannot_survive_profile_or_source_changes(prepared, fault):
    manager, _, source = prepared
    profile = manager.profile_id
    if fault == "disabled":
        for row in manager.rows:
            if row.kind != "separator":
                manager.library.customize(profile, row.id, enabled=False)
    elif fault == "added-asset":
        (source / "mods/Override/new.txt").write_bytes(b"generated new winner")
    elif fault == "changed-asset":
        (source / "mods/Override/textures/shared.txt").write_bytes(b"changed captured winner")
    elif fault == "profile":
        manager.profile_id = manager.library.create_profile("Other", clone=manager.profile_id)
    else:
        import json
        with manager.library._db() as db:
            files = json.loads(db.execute("SELECT files FROM profiles WHERE id=?", (profile,)).fetchone()[0])
            files["plugins.txt"] = "Fixture.esp\n"
            db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
    before = manager.library.rows(profile)
    manager.review_install()
    assert isinstance(manager._future.exception(timeout=10), ManagerError)
    manager.poll()
    assert manager.job_state == "failed" and manager.install_plan is None
    assert manager.library.rows(profile) == before
    assert not list((manager.library.root / "mods").iterdir())


@pytest.mark.parametrize("mode", ["install", "replace", "merge"])
def test_context_changes_during_staging_abort_before_placement_and_keep_existing_mod(prepared, monkeypatch, mode):
    manager, package, source = prepared
    old = None
    if mode != "install":
        # Initial valid install, then a new captured context for replacement.
        manager.review_install()
        finish(manager)
        manager.install("Context fixture")
        finish(manager)
        old = manager.rows_by_id[manager.selected_id]
        manager.prepare_install(package, old.id)
        finish(manager)
        manager.install_mode = "merge" if mode == "merge" else "replace"
    manager.review_install()
    finish(manager)
    before = manager.library.rows(manager.profile_id)
    payload = (Path(old.source) / "base.txt").read_bytes() if old else None
    def change(stage):
        if stage == "prepared":
            (source / "mods/Override/late-condition.txt").write_bytes(b"condition changed after copying")
    monkeypatch.setattr(manager.library.owned, "_checkpoint", change)
    manager.install("Context fixture")
    assert isinstance(manager._future.exception(timeout=10), ManagerError)
    manager.poll()
    assert manager.job_state == "failed" and manager.library.rows(manager.profile_id) == before
    assert not list((manager.library.root / ".staging").iterdir())
    if old:
        assert (Path(old.source) / "base.txt").read_bytes() == payload and manager.library.history() == []
    else:
        assert not list((manager.library.root / "mods").iterdir())
