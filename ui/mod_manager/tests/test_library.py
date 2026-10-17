import hashlib
import os
from pathlib import Path
import sqlite3
import stat
import threading
import zipfile

import pytest

from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.package import Limits, Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError, relative_path
from .fixtures import FOMOD_XML, make_mo2, make_package


def digest(root):
    return {path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in root.rglob("*") if path.is_file()}


def test_mo2_ini_profile_import_priority_metadata_and_source_unchanged(tmp_path):
    source = make_mo2(tmp_path)
    before = digest(source)
    preview = inspect_instance(source / "ModOrganizer.ini")
    assert preview.game == "Fallout 4"
    assert len(preview.profiles) == 2
    assert [mod.name for mod in preview.profiles[0].mods] == ["Visual_separator", "Base", "Override", "Disabled"]
    assert preview.profiles[0].mods[1].category == "Visuals"
    assert preview.profiles[0].mods[1].group == "Visual"
    assert not preview.profiles[0].mods[3].enabled
    profile_only = inspect_instance(source / "profiles" / "Default")
    assert [profile.name for profile in profile_only.profiles] == ["Default"]
    library = Library(tmp_path / "library", create=True)
    assert library.import_mo2(preview, {preview.profiles[0].id}) == 1
    profile = library.profiles()[0]
    assert "plugins.txt" in profile.files and "Fallout4.ini" in profile.files
    rows = library.rows(profile.id)
    library.customize(profile.id, rows[1].id, group="My group", tags="favorite", notes="mine", enabled=False)
    library.reorder(profile.id, rows[2].id, 0)
    assert library.import_mo2(preview) == 1
    assert library.import_mo2(preview) == 0
    refreshed = {row.name: row for row in library.rows(profile.id)}
    assert refreshed["Base"].group == "My group" and not refreshed["Base"].enabled
    assert refreshed["Base"].notes == "mine" and refreshed["Base"].tags == "favorite"
    assert refreshed["Override"].priority == 0
    assert digest(source) == before


def test_cloned_profile_changes_are_independent(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    original = library.profiles()[0]
    clone = library.create_profile("Experiment", original.id)
    row = library.rows(clone)[1]
    library.customize(clone, row.id, enabled=False, group="Experiment")
    original_row = next(item for item in library.rows(original.id) if item.id == row.id)
    assert original_row.enabled and original_row.group == "Visual"


def test_profile_export_conflict_winners_hidden_files_and_priority(tmp_path):
    source = make_mo2(tmp_path)
    before = digest(source)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0]
    plan = library.export_plan(profile.id)
    assert plan.conflicts["textures/shared.txt"] == ["Base", "Override"]
    output = library.export(plan)
    assert output.is_relative_to(library.root / "exports")
    assert (output / "textures/shared.txt").read_bytes() == b"override"
    assert not (output / "meta.ini").exists() and not (output / "hidden.mohidden").exists()
    assert not (output / "plugins.txt").exists()
    row = next(row for row in library.rows(profile.id) if row.name == "Base")
    library.reorder(profile.id, row.id, 3)
    assert Path(library.export_plan(profile.id).files["textures/shared.txt"][1]).read_bytes() == b"base"
    assert digest(source) == before


def test_installs_commit_once_and_never_replace(tmp_path):
    path = make_package(tmp_path)
    before = path.read_bytes()
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    package = Package(path)
    plan = Fomod(FOMOD_XML).plan(package)
    mod_id = library.install(package, plan, "Fixture", profile)
    destination = library.root / "mods" / mod_id
    assert (destination / "textures/shared.txt").read_bytes() == b"standard"
    assert library.rows(profile)[0].enabled
    with pytest.raises(ManagerError, match="already has this name"):
        library.install(package, plan, "FIXTURE", profile)
    assert len(list((library.root / "mods").iterdir())) == 1
    assert list((library.root / ".staging").iterdir()) == []
    assert path.read_bytes() == before


def test_cancelled_install_rolls_back_files_and_metadata(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    package = Package(make_package(tmp_path))
    cancel = threading.Event()
    with pytest.raises(Cancelled):
        library.install(package, Fomod(FOMOD_XML).plan(package), "Cancelled", profile, cancel,
                        lambda fraction, name: cancel.set())
    assert library.rows(profile) == []
    assert list((library.root / "mods").iterdir()) == []
    assert list((library.root / ".staging").iterdir()) == []


def test_metadata_failure_after_file_move_rolls_back(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    with library._db() as db:
        db.execute("CREATE TRIGGER reject_mod BEFORE INSERT ON mods BEGIN SELECT RAISE(ABORT, 'fixture metadata failure'); END")
    package = Package(make_package(tmp_path))
    with pytest.raises(sqlite3.IntegrityError, match="fixture metadata failure"):
        library.install(package, Fomod(FOMOD_XML).plan(package), "Failure", profile)
    assert library.rows(profile) == []
    assert list((library.root / "mods").iterdir()) == []
    assert list((library.root / ".staging").iterdir()) == []


def test_failed_source_copy_and_cancelled_export_roll_back(tmp_path, monkeypatch):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    plan = library.export_plan(library.profiles()[0].id)
    cancel = threading.Event()
    with pytest.raises(Cancelled):
        library.export(plan, cancel, lambda fraction, name: cancel.set())
    assert list((library.root / "exports").iterdir()) == []
    assert list((library.root / ".staging").iterdir()) == []
    package = Package(make_package(tmp_path))
    monkeypatch.setattr(package, "open", lambda name: (_ for _ in ()).throw(OSError("fixture read failure")))
    with pytest.raises(OSError, match="read failure"):
        library.install(package, {"safe": "base.txt"}, "Failure", library.profiles()[0].id)
    assert list((library.root / "mods").iterdir()) == []
    assert list((library.root / ".staging").iterdir()) == []


def test_cancelled_import_is_atomic(tmp_path):
    source = make_mo2(tmp_path)
    preview = inspect_instance(source)
    library = Library(tmp_path / "library", create=True)
    class CancelAfterChecks:
        count = 0
        def is_set(self):
            self.count += 1
            return self.count > 3
    with pytest.raises(Cancelled):
        library.import_mo2(preview, cancel=CancelAfterChecks())
    assert library.profiles() == []


def test_refuse_nonempty_library_and_overlapping_mo2(tmp_path):
    source = make_mo2(tmp_path)
    with pytest.raises(ManagerError, match="empty folder"):
        Library(source, create=True)
    preview = inspect_instance(source)
    nested = Library(source / "new-library", create=True)
    with pytest.raises(ManagerError, match="separate"):
        nested.import_mo2(preview)


@pytest.mark.parametrize("path", ["../escape", "/absolute", "C:/absolute", "file:stream", "CON.txt", "aux", "dir/../escape", "trailing.", "name ", "a//b", "\\\\server\\share", "a\x00b"])
def test_windows_path_guards(path):
    with pytest.raises(ManagerError):
        relative_path(path)


@pytest.mark.parametrize("names", [("A.txt", "a.TXT"), ("a", "a/b"), ("../outside", "safe"), ("C:/outside", "safe")])
def test_unsafe_zip_entries_rejected_before_extraction(tmp_path, names):
    path = tmp_path / "bad.zip"
    with zipfile.ZipFile(path, "w") as archive:
        for name in names:
            archive.writestr(name, b"fixture")
    with pytest.raises(ManagerError):
        Package(path)
    assert not (tmp_path / "outside").exists()


def test_archive_links_and_size_limits(tmp_path):
    path = tmp_path / "link.zip"
    with zipfile.ZipFile(path, "w") as archive:
        item = zipfile.ZipInfo("link")
        item.create_system = 3
        item.external_attr = (stat.S_IFLNK | 0o777) << 16
        archive.writestr(item, "../outside")
    with pytest.raises(ManagerError, match="link"):
        Package(path)
    with pytest.raises(ManagerError, match="limit"):
        Package(make_package(tmp_path), limits=Limits(files=2))


def test_plain_package_root_and_unsupported_archive(tmp_path):
    folder = tmp_path / "plain"
    (folder / "Data" / "textures").mkdir(parents=True)
    (folder / "Data" / "textures" / "safe.txt").write_text("safe")
    package = Package(folder)
    assert package.plain_plan("data") == {"textures/safe.txt": "Data/textures/safe.txt"}
    with pytest.raises(ManagerError, match="No installable"):
        package.plain_plan("missing")
    path = tmp_path / "unsupported.7z"
    path.write_bytes(b"fixture")
    with pytest.raises(ManagerError, match="Expected a 7z or RAR"):
        Package(path)


def test_source_changed_after_preview_rolls_back(tmp_path):
    folder = tmp_path / "package"
    folder.mkdir()
    file = folder / "a.txt"
    file.write_bytes(b"before")
    package = Package(folder)
    info = file.stat()
    file.write_bytes(b"after!")
    os.utime(file, ns=(info.st_atime_ns, info.st_mtime_ns + 1_000_000_000))
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    with pytest.raises(ManagerError, match="changed"):
        library.install(package, {"a.txt": "a.txt"}, "Changed", profile)
    assert not library.rows(profile)
    assert not any((library.root / ".staging").iterdir())


def test_export_source_changed_after_preview_rolls_back(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    plan = library.export_plan(library.profiles()[0].id)
    file = source / "mods" / "Override" / "textures" / "shared.txt"
    file.write_bytes(b"changed payload")
    with pytest.raises(ManagerError, match="changed"):
        library.export(plan)
    assert not any((library.root / "exports").iterdir())
    assert not any((library.root / ".staging").iterdir())


def test_bad_archive_crc_rolls_back_install(tmp_path):
    archive_path = make_package(tmp_path)
    with zipfile.ZipFile(archive_path) as archive:
        entry = archive.getinfo("base.txt")
        offset = entry.header_offset + 30 + len(entry.filename.encode()) + len(entry.extra)
    content = bytearray(archive_path.read_bytes())
    content[offset] ^= 1
    archive_path.write_bytes(content)
    package = Package(archive_path)
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    with pytest.raises(zipfile.BadZipFile, match="CRC"):
        library.install(package, {"base.txt": "base.txt"}, "Corrupt", profile)
    assert not library.rows(profile)
    assert not any((library.root / "mods").iterdir())
    assert not any((library.root / ".staging").iterdir())


def test_windows_reparse_point_rejected(tmp_path):
    from creation_lib.mod_manager.paths import reject_links
    from .test_mo2 import junction
    target = tmp_path / "target"
    target.mkdir()
    link = tmp_path / "fixture-junction"
    junction(link, target)
    with pytest.raises(ManagerError, match="reparse"):
        reject_links(link)


def test_foreign_and_missing_enabled_mods_block_partial_export(tmp_path):
    source = make_mo2(tmp_path)
    profile = source / "profiles" / "Default" / "modlist.txt"
    profile.write_text("*GameContent\n+MissingMod\n+Base\n")
    library = Library(tmp_path / "library", create=True)
    preview = inspect_instance(source / "profiles" / "Default")
    assert any("missing mod folder" in warning for warning in preview.warnings)
    library.import_mo2(preview)
    with pytest.raises(ManagerError, match="no importable"):
        library.export_plan(library.profiles()[0].id)
