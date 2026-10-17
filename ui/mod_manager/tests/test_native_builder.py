import hashlib
import json
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager import native_builder as builder
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_runtime import pe


@pytest.fixture
def builds(tmp_path, monkeypatch):
    source = tmp_path / "source"
    (source / "vendor/minhook").mkdir(parents=True)
    (source / "filespace.cpp").write_text("original fixture source")
    (source / "vendor/minhook/LICENSE.txt").write_text("fixture notice")
    (source / "LICENSE.txt").write_text("fixture project GPL-3.0 notice")
    monkeypatch.setattr(builder, "SOURCE", source)
    monkeypatch.setattr("app.paths.is_frozen", lambda: False)
    calls = []
    def command(argv, cancel, progress):
        calls.append(argv)
        if "--build" in argv:
            binary = Path(argv[argv.index("--build") + 1]) / "Release/modbox_vfs.dll"
            binary.parent.mkdir(parents=True, exist_ok=True)
            pe(binary)
    monkeypatch.setattr(builder, "_command", command)
    monkeypatch.setattr("shutil.which", lambda _: "fixture-cmake.exe")
    return tmp_path / "cache", calls, source


def test_build_cache_repeated_use_has_no_upstream_dependency_or_compile(builds):
    cache, calls, _ = builds
    root = builder.ensure(cache=cache)
    assert len(calls) == 2 and builder.ensure(cache=cache) == root and len(calls) == 2
    assert "-DMODBOX_VFS_TESTING=OFF" in calls[0]
    receipt = json.loads((root / "native-receipt.json").read_text())
    assert receipt["abi"] == 1 and receipt["modbox_vfs.dll"] == hashlib.sha256((root / "modbox_vfs.dll").read_bytes()).hexdigest()
    assert (root / "MinHook-LICENSE.txt").read_text() == "fixture notice"
    assert (root / "Project-LICENSE.txt").read_text() == "fixture project GPL-3.0 notice"


@pytest.mark.parametrize("fault", ["dll", "receipt", "notice", "project_notice", "source", "pointer"])
def test_repair_or_source_update_publishes_new_generation_and_retains_prior(builds, fault):
    cache, calls, source = builds
    old = builder.ensure(cache=cache)
    if fault == "source":
        (source / "filespace.cpp").write_text("changed original source")
    elif fault == "pointer":
        next(cache.glob("*.json")).write_text('{"generation":"../outside"}')
    else:
        (old / {"dll": "modbox_vfs.dll", "receipt": "native-receipt.json", "notice": "MinHook-LICENSE.txt", "project_notice": "Project-LICENSE.txt"}[fault]).write_bytes(b"corrupt")
    repaired = builder.ensure(cache=cache)
    assert repaired != old and old.is_dir() and len(calls) == 4
    assert builder.validate(repaired) == repaired


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_failed_cancelled_build_does_not_publish_or_replace_prior_generation(builds, monkeypatch, outcome):
    cache, _, source = builds
    old = builder.ensure(cache=cache)
    pointer = next(cache.glob("*.json"))
    original = pointer.read_bytes()
    (source / "filespace.cpp").write_text("next source")
    def stop(*args):
        raise Cancelled("fixture cancel") if outcome == "cancelled" else ManagerError("fixture failure")
    monkeypatch.setattr(builder, "_command", stop)
    with pytest.raises(Cancelled if outcome == "cancelled" else ManagerError):
        builder.ensure(cache=cache)
    assert pointer.read_bytes() == original and old.is_dir()
    assert len(list(cache.glob("*/native-receipt.json"))) == 1


def test_cancelled_cache_wait_never_compiles(builds):
    cache, calls, _ = builds
    cancelled = threading.Event()
    cancelled.set()
    with pytest.raises(Cancelled):
        builder.ensure(cancelled, cache=cache)
    assert calls == []


def test_missing_compiler_is_specific_development_error(builds, monkeypatch):
    cache, calls, _ = builds
    monkeypatch.setattr("shutil.which", lambda _: None)
    with pytest.raises(ManagerError, match="CMake and MSVC"):
        builder.ensure(cache=cache)
    assert calls == []


def test_frozen_helper_dispatch_is_early_exact_and_inert_for_normal_args(monkeypatch):
    from creation_lib.mod_manager.runtime_entry import FLAG, dispatch
    calls = []
    monkeypatch.setattr("creation_lib.mod_manager.runtime_helper.run", lambda value: calls.append(value) or 0)
    assert dispatch(["--normal"]) is False and not calls
    with pytest.raises(SystemExit) as complete:
        dispatch([FLAG, "fixture-request.json"])
    assert complete.value.code == 0 and calls == ["fixture-request.json"]
    with pytest.raises(SystemExit):
        dispatch([FLAG])


def test_private_package_contains_only_project_native_payload_source_and_notices(builds, monkeypatch, tmp_path):
    from creation_lib.mod_manager.runtime_delivery import freeze_datas
    cache, _, source = builds
    root = builder.ensure(cache=cache)
    monkeypatch.setattr(builder, "ensure", lambda **kwargs: root)
    (tmp_path / "resource").mkdir()
    (tmp_path / "resource/icon.ico").write_bytes(b"fixture icon")
    data = freeze_datas(tmp_path)
    assert {Path(path).name for path, folder in data if folder.endswith("project-vfs")} == {"modbox_vfs.dll", "MinHook-LICENSE.txt", "Project-LICENSE.txt", "native-receipt.json"}
    assert any(Path(path) == source / "filespace.cpp" for path, _ in data)
    assert not any("usvfs" in path.casefold() for path, _ in data)
    assert (str(tmp_path / "resource/icon.ico"), "resource") in data
    for kwargs in ({"public": True}, {"onefile": True}):
        with pytest.raises(ManagerError):
            freeze_datas(tmp_path, **kwargs)
