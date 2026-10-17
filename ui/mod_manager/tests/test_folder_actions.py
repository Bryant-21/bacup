from pathlib import Path
import subprocess

import pytest

from creation_lib.mod_manager.folder_actions import reveal_library, reveal_mod
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import make_mo2, make_package
from .test_mo2 import junction


@pytest.fixture
def folders(tmp_path, monkeypatch):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0].id
    calls = []
    popen = subprocess.Popen
    def launch(argv, **kwargs):
        if argv[0] == "explorer.exe":
            return calls.append((argv, kwargs))
        return popen(argv, **kwargs)
    monkeypatch.setattr(subprocess, "Popen", launch)
    return library, profile, calls


def test_external_owned_and_library_folders_are_directory_arguments_without_shell(folders, tmp_path):
    from creation_lib.mod_manager.package import Package
    library, profile, calls = folders
    mod = next(row for row in library.rows(profile) if row.kind == "external")
    reveal_mod(library, profile, mod.id)
    package = Package(make_package(tmp_path))
    try:
        owned = library.install(package, {"base.txt": "base.txt"}, "Owned", profile)
    finally:
        package.close()
    reveal_mod(library, profile, owned)
    reveal_library(library)
    assert calls == [(["explorer.exe", mod.source], {"shell": False}),
                     (["explorer.exe", str(library.root / "mods" / owned)], {"shell": False}),
                     (["explorer.exe", str(library.root)], {"shell": False})]


def test_changed_or_missing_profile_membership_and_separators_never_open(folders):
    library, profile, calls = folders
    mod = next(row for row in library.rows(profile) if row.kind == "external")
    for profile_id, mod_id in (("missing profile", mod.id), (profile, "missing mod"),
                               (profile, next(row.id for row in library.rows(profile) if row.kind == "separator"))):
        with pytest.raises(ManagerError, match="Select an available"):
            reveal_mod(library, profile_id, mod_id)
    with library._db() as db:
        db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, mod.id))
    with pytest.raises(ManagerError, match="Select an available"):
        reveal_mod(library, profile, mod.id)
    assert not calls


@pytest.mark.parametrize("source", ["relative path", "", "\\\\server\\share\\mods", "\\\\?\\C:\\mods", "C:\\"])
def test_nonlocal_root_device_or_relative_paths_fail_before_explorer(folders, source):
    library, profile, calls = folders
    mod = next(row for row in library.rows(profile) if row.kind == "external")
    with library._db() as db:
        db.execute("UPDATE mods SET source=? WHERE id=?", (source, mod.id))
    with pytest.raises(ManagerError, match="absolute path on a local drive"):
        reveal_mod(library, profile, mod.id)
    assert not calls


def test_links_missing_files_and_owned_metadata_redirect_are_rejected(folders, tmp_path):
    library, profile, calls = folders
    mod = next(row for row in library.rows(profile) if row.kind == "external")
    link = tmp_path / "linked source"
    target = Path(mod.source)
    junction(link, target)
    for source in (link, tmp_path / "missing", target / "base.txt"):
        with library._db() as db:
            db.execute("UPDATE mods SET source=? WHERE id=?", (str(source), mod.id))
        with pytest.raises(ManagerError):
            reveal_mod(library, profile, mod.id)
    with library._db() as db:
        db.execute("UPDATE mods SET source=?,kind='owned' WHERE id=?", (str(target), mod.id))
    with pytest.raises(ManagerError, match="no longer matches"):
        reveal_mod(library, profile, mod.id)
    assert not calls and target.is_dir()


@pytest.mark.parametrize("name", ["Folder with spaces & punctuation", "$(literal) `literal`"])
def test_folder_punctuation_stays_literal_in_one_argument(folders, tmp_path, name):
    library, profile, calls = folders
    mod = next(row for row in library.rows(profile) if row.kind == "external")
    folder = tmp_path / name
    folder.mkdir()
    with library._db() as db:
        db.execute("UPDATE mods SET source=? WHERE id=?", (str(folder), mod.id))
    reveal_mod(library, profile, mod.id)
    assert calls == [(["explorer.exe", str(folder)], {"shell": False})]
