from dataclasses import replace
import subprocess
import threading

import pytest

from creation_lib.mod_manager.launcher import Executable, definition, review, start, windows_arguments
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import make_mo2


@pytest.mark.parametrize("arguments", [[], [""], ["one", "two words"], ["C:\\test\\", "C:\\with spaces\\"],
                                       ['quote"test', "a\\\"b"], ["&", "$(never executed)", "`literal`", "##title"],
                                       ["", " ", "\\\\", '"', 'tail\\\\"']])
def test_windows_argument_roundtrip_has_no_shell(arguments):
    assert windows_arguments(subprocess.list2cmdline(arguments)) == arguments


def test_invalid_quotes_and_credential_arguments():
    with pytest.raises(ManagerError, match="unclosed quote"):
        windows_arguments('"unfinished')
    with pytest.raises(ManagerError, match="Credential"):
        definition(id="", title="Test", binary="", arguments="--api-key secret")


def test_executable_import_edit_remove_repeat_and_restart_are_local(tmp_path):
    source = make_mo2(tmp_path)
    ini = source / "ModOrganizer.ini"
    with ini.open("a", encoding="utf-8") as output:
        output.write('[customExecutables]\n1\\title=F4SE\n1\\binary=C:/Fixture/f4se_loader.exe\n1\\workingDirectory=C:/Fixture\n2\\title=Private\n2\\arguments=--password secret\n')
    unchanged = ini.read_bytes()
    preview = inspect_instance(source)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(preview)
    instance_id = library.profiles()[0].files["_modbox_instance_id"]
    entries = library.executables(instance_id)
    assert len(entries) == 1 and entries[0].title == "F4SE" and entries[0].mode == "profile"
    edited = library.save_executable(replace(entries[0], title="My F4SE", arguments='"two words"'))
    library.import_mo2(preview, reconcile=True)
    assert library.executables(instance_id) == [edited]
    library.remove_executable(edited.id)
    library.import_mo2(preview, reconcile=True)
    reopened = Library(library.root)
    assert reopened.executables(instance_id) == []
    assert ini.read_bytes() == unchanged


def test_direct_launch_is_explicit_revalidated_and_failure_has_no_fallback(tmp_path):
    binary = tmp_path / "harmless fixture.exe"
    binary.write_bytes(b"fixture image; never executed")
    exe = definition(id="fixture", title="Tool", binary=str(binary), arguments='"two words" &', mode="direct")
    plan = review(exe, "profile-id", "Profile")
    calls = []
    result = start(plan, spawn=lambda *args, **kwargs: calls.append((args, kwargs)) or "process")
    assert result == "process" and calls[0][0][0] == [str(binary), "two words", "&"]
    assert calls[0][1]["shell"] is False and calls[0][1]["cwd"] == str(tmp_path)
    binary.write_bytes(b"changed")
    with pytest.raises(ManagerError, match="changed after review"):
        start(plan, spawn=lambda *args, **kwargs: pytest.fail("stale executable ran"))
    binary.write_bytes(b"fixture")
    fresh = review(exe)
    def failed(*args, **kwargs):
        raise OSError("fixture access denied")
    with pytest.raises(ManagerError, match="fixture access denied"):
        start(fresh, spawn=failed)
    with pytest.raises(ManagerError, match="configured VFS"):
        review(replace(exe, mode="profile"), "profile-id", "Profile")
    with pytest.raises(ManagerError, match="verified process adapter"):
        start(replace(fresh, runtime="usvfs-fo4-x64"), spawn=lambda *args, **kwargs: pytest.fail("fallback ran"))


def test_paths_and_scripts_are_rejected_before_launch(tmp_path):
    for path in ("relative.exe", r"\\unapproved\share\tool.exe", str(tmp_path / "missing.exe")):
        with pytest.raises(ManagerError):
            review(Executable("fixture", "Fixture", path, mode="direct"))
    script = tmp_path / "script.cmd"
    script.write_text("echo fixture")
    with pytest.raises(ManagerError, match=".exe"):
        review(Executable("fixture", "Fixture", str(script), mode="direct"))


def test_controller_cancelled_launch_preparation_cannot_run(tmp_path, monkeypatch):
    from creation_lib.mod_manager.controller import ManagerController
    from creation_lib.mod_manager.paths import check_cancel
    controller = ManagerController()
    controller.library = Library(tmp_path / "library", create=True)
    controller.library.create_profile("Fixture")
    from .test_launch_resolution import installation
    from .test_pe_metadata import fixture_version
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)
    game = installation(tmp_path / "game")
    executable = controller.library.save_executable(Executable("", "Fixture", str(game / "f4se_loader.exe")))
    controller.refresh()
    entered, released = threading.Event(), threading.Event()
    def paused(*args, **kwargs):
        entered.set()
        released.wait(5)
        check_cancel(args[5])
    monkeypatch.setattr("creation_lib.mod_manager.runtime.prepare", paused)
    try:
        controller.prepare_launch(executable.id)
        assert entered.wait(5)
        controller.cancel()
        released.set()
        while controller.busy:
            controller.poll()
        assert controller.job_state == "cancelled" and controller.launch_review is None
    finally:
        released.set()
        controller.cleanup()
