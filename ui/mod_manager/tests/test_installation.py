"""Manual destinations and recoverable merge behavior, entirely generated payloads."""
from contextlib import closing
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.installation import include_sources, merge, merge_preview, remap, source_tree
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_transactions import setup_owned


def test_manual_mapping_renames_relocates_excludes_and_preserves_sources():
    base = {"textures/a.dds": "source/a.dds", "b.esp": "source/b.esp", "readme.txt": "source/readme.txt"}
    assert remap(base, {"textures/a.dds": "textures/custom/a.dds", "readme.txt": None}) == {
        "textures/custom/a.dds": "source/a.dds", "b.esp": "source/b.esp"}
    assert base["readme.txt"] == "source/readme.txt"


def test_additional_source_tree_and_explicit_destinations_preserve_package(tmp_path):
    root = tmp_path / "package"
    (root / "optional").mkdir(parents=True)
    (root / "optional" / "extra.txt").write_text("generated extra")
    (root / "required.txt").write_text("generated required")
    (root / "hidden.mohidden").write_text("hidden")
    (root / "meta.ini").write_text("metadata")
    with closing(Package(root)) as package:
        tree = source_tree(package)
        assert [row.path for row in tree.visible(set())] == ["optional", "required.txt"]
        assert [row.path for row in tree.visible(set(), "extra")] == ["optional", "optional/extra.txt"]
        plan = include_sources(package, {"required.txt": "required.txt"}, {"custom/extra.txt": "OPTIONAL/EXTRA.TXT"})
        assert plan["custom/extra.txt"] == "optional/extra.txt"
        for extra in ({"required.TXT": "optional/extra.txt"}, {"../escape": "optional/extra.txt"}, {"extra": "missing"}, {"extra": "meta.ini"}, {"extra": "hidden.mohidden"}):
            with pytest.raises(ManagerError):
                include_sources(package, {"required.txt": "required.txt"}, extra)
        assert include_sources(package, remap({"required.txt": "required.txt"}, {"required.txt": None}, allow_empty=True), {"extra.txt": "optional/extra.txt"}) == {"extra.txt": "optional/extra.txt"}
    assert (root / "optional" / "extra.txt").read_text() == "generated extra"


@pytest.mark.parametrize("edits", [
    {"a.txt": "../a.txt"}, {"a.txt": "A.txt", "b.txt": "a.TXT"}, {"a.txt": "folder", "b.txt": "folder/file"},
    {"a.txt": None, "b.txt": None}, {"changed.txt": "elsewhere.txt"}])
def test_invalid_or_stale_manual_mapping_fails(edits):
    with pytest.raises(ManagerError):
        remap({"a.txt": "a.txt", "b.txt": "b.txt"}, edits)


def test_merge_preserves_unmatched_hidden_customization_nexus_and_full_backup(tmp_path):
    library, profile, mod, replacement = setup_owned(tmp_path)
    clone = library.create_profile("Clone", profile)
    library.customize(clone, mod, enabled=False, group="Nested/Fixture", notes="keep note")
    owned = library.root / "mods" / mod
    (owned / "keep.ini").write_text("user-owned unmatched content")
    (owned / "hidden.txt.mohidden").write_text("hidden user-owned content")
    (owned / "empty" / "nested").mkdir(parents=True)
    review = library.review_owned(mod)
    before = library.rows(clone)[0]
    source_before = {p.name: p.read_bytes() for p in replacement.iterdir()}
    with closing(Package(replacement)) as package:
        pairs = merge_preview(library, review, package, package.plain_plan())
        assert {target for target, _ in pairs} == {"keep.ini", "hidden.txt.mohidden", "file.txt", "extra.txt"}
        assert merge(library, review, package, package.plain_plan()) == mod
    assert (owned / "file.txt").read_text() == "replacement"
    assert (owned / "keep.ini").read_text() == "user-owned unmatched content"
    assert (owned / "hidden.txt.mohidden").read_text() == "hidden user-owned content"
    assert (owned / "empty" / "nested").is_dir()
    after = library.rows(clone)[0]
    assert (after.id, after.group, after.enabled, after.priority, after.notes) == (before.id, before.group, before.enabled, before.priority, before.notes)
    backup = library.history()[0]
    assert Path(backup["path"]).joinpath("file.txt").read_text() == "original"
    assert {p.name: p.read_bytes() for p in replacement.iterdir()} == source_before
    library.restore(backup["id"], library.review_owned(mod))
    assert (owned / "file.txt").read_text() == "original" and not (owned / "extra.txt").exists()


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "stale"])
def test_merge_cancel_failure_and_stale_review_preserve_owned_content(tmp_path, monkeypatch, outcome):
    library, _, mod, replacement = setup_owned(tmp_path)
    owned = library.root / "mods" / mod
    review = library.review_owned(mod)
    cancel = threading.Event()
    if outcome == "stale":
        (owned / "late.ini").write_text("user edit after review")
    def checkpoint(name):
        if name == "placed":
            if outcome == "cancelled":
                cancel.set()
            elif outcome == "failed":
                raise OSError("generated failure after payload placement")
    monkeypatch.setattr(library.owned, "_checkpoint", checkpoint)
    with closing(Package(replacement)) as package:
        with pytest.raises(Cancelled if outcome == "cancelled" else OSError if outcome == "failed" else ManagerError):
            merge(library, review, package, package.plain_plan(), cancel)
    assert (owned / "file.txt").read_text() == "original" and not (owned / "extra.txt").exists()
    assert bool((owned / "late.ini").exists()) == (outcome == "stale")
    assert not library.history()
