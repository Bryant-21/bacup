"""Read-only runtime discovery and pre-load refusals use disposable PE fixtures."""
import ctypes
from pathlib import Path

import pytest

from creation_lib.mod_manager import runtime_setup
from creation_lib.mod_manager.native_runtime import NativeVFS
from creation_lib.mod_manager.paths import ManagerError
from .test_runtime import pe


def make_bundle(root):
    root.mkdir(parents=True)
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c),
                          ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(root / name, machine)
    return root


def test_empty_relative_and_drive_relative_paths_are_actionable(tmp_path):
    with pytest.raises(ManagerError, match="Runtime setup"):
        runtime_setup.resolve("", tmp_path)
    root = make_bundle(tmp_path / "resource/usvfs")
    assert runtime_setup.resolve("resource/usvfs", tmp_path) == root
    with pytest.raises(ManagerError, match="complete local"):
        runtime_setup.resolve("C:bundle", tmp_path)
    with pytest.raises(ManagerError):
        runtime_setup.resolve(r"\\server\bundle", tmp_path)


@pytest.mark.parametrize("fault", ["unknown", "version", "architecture", "missing", "mixed-proxy"])
def test_unsupported_candidates_fail_without_loading_any_candidate(tmp_path, monkeypatch, fault):
    root = make_bundle(tmp_path / "bundle")
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", lambda path: None if fault == "unknown" else
                        (0, 5, 7, 1) if fault == "version" or fault == "mixed-proxy" and path.name == "usvfs_proxy_x86.exe" else (0, 5, 7, 2))
    if fault == "architecture":
        pe(root / "usvfs_x86.dll", 0x8664)
    elif fault == "missing":
        (root / "usvfs_proxy_x86.exe").unlink()
    calls = []
    monkeypatch.setattr(ctypes, "WinDLL", lambda *args, **kwargs: calls.append(args) or pytest.fail("Candidate DLL loaded before validation"))
    with pytest.raises(ManagerError):
        runtime_setup.supported_bundle(str(root))
    with pytest.raises(ManagerError):
        NativeVFS(str(root), "inert-session")
    assert not calls


def test_bounded_discovery_checks_only_known_paths_and_preserves_bytes(tmp_path, monkeypatch):
    root = make_bundle(tmp_path / "MO2")
    app = tmp_path / "app"
    app.mkdir()
    other = make_bundle(tmp_path / "unrelated")
    before = {str(path): path.read_bytes() for directory in (root, other) for path in directory.iterdir()}
    checked = []
    def version(path):
        checked.append(path)
        return (0, 5, 7, 2)
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", version)
    candidates, issues = runtime_setup.discover({"paths": {"instance": str(root), "base_directory": str(root)}}, app)
    assert candidates == [str(root)] and len(checked) == 4
    assert all(path.parent == root for path in checked)
    assert all("No such" not in issue for issue in issues)
    assert {str(path): Path(path).read_bytes() for path in before} == before


def test_static_unknown_metadata_is_not_treated_as_supported(tmp_path):
    root = make_bundle(tmp_path / "bundle")
    with pytest.raises(ManagerError, match="unknown .*missing version"):
        runtime_setup.supported_bundle(str(root))
