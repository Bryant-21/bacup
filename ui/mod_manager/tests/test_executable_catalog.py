from dataclasses import replace
import threading

import pytest

from creation_lib.mod_manager import executable_catalog as catalog
from creation_lib.mod_manager.launcher import Executable
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


def test_edit_order_default_remove_repeat_import_reopen_source_unchanged(tmp_path):
    source = make_mo2(tmp_path)
    with (source / "ModOrganizer.ini").open("a") as stream:
        stream.write("\n[customExecutables]\n1\\title=F4SE\n1\\binary=C:/Fixture/f4se_loader.exe\n2\\title=Game\n2\\binary=C:/Fixture/Fallout4.exe\n")
    original = (source / "ModOrganizer.ini").read_bytes()
    library = Library(tmp_path / "library", create=True)
    preview = inspect_instance(source)
    library.import_mo2(preview)
    instance = library.profiles()[0].files["_modbox_instance_id"]
    first, second = library.executables(instance)
    added = Executable("added", "Tool", "C:/Fixture/Tool.exe", mode="direct", instance_id=instance)
    edited = replace(first, title="My loader", arguments="-v", working_directory="C:/Fixture")
    catalog.save(library, instance, catalog.snapshot(library, instance), [added, edited], edited.id)
    library.import_mo2(preview, reconcile=True)
    reopened = Library(library.root)
    assert catalog.ordered(reopened, reopened.executables(instance), instance) == [added, edited]
    assert catalog.preferences(reopened, instance)["default"] == edited.id
    assert second not in reopened.executables(instance)
    assert (source / "ModOrganizer.ini").read_bytes() == original


@pytest.mark.parametrize("fault", ["cancel", "commit", "stale", "hidden-default"])
def test_executable_draft_atomic_failure_and_cancel(tmp_path, monkeypatch, fault):
    library = Library(tmp_path / "library", create=True)
    old = library.save_executable(Executable("original", "Original", "C:/Fixture/Tool.exe"))
    expected = catalog.snapshot(library)
    cancel = threading.Event()
    changed = replace(old, title="Changed")
    if fault == "cancel":
        monkeypatch.setattr(catalog, "_checkpoint", cancel.set)
    elif fault == "commit":
        monkeypatch.setattr(catalog, "_checkpoint", lambda: (_ for _ in ()).throw(OSError("generated commit failure")))
    elif fault == "stale":
        library.save_executable(replace(old, arguments="new external metadata"))
    else:
        changed = replace(changed, hidden=True)
    before = library.executables()
    with pytest.raises((Cancelled, ManagerError, OSError)):
        catalog.save(library, "", expected, [changed], changed.id, cancel)
    assert library.executables() == before
    assert catalog.preferences(library) == {"order": [], "default": ""}


def test_removing_last_executable_can_be_saved(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.save_executable(Executable("last", "Last", "C:/Fixture/Tool.exe"))
    catalog.save(library, "", catalog.snapshot(library), [])
    assert library.executables() == []
