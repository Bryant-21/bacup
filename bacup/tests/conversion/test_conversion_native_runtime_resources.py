from __future__ import annotations

import os
from pathlib import Path
from types import SimpleNamespace

import pytest


@pytest.fixture(autouse=True)
def restore_native_runtime_state():
    from bacup_lib import native_runtime

    old_native = native_runtime._NATIVE
    old_resource_dir = os.environ.get("CREATION_LIB_RESOURCE_DIR")
    yield
    native_runtime._NATIVE = old_native
    if old_resource_dir is None:
        os.environ.pop("CREATION_LIB_RESOURCE_DIR", None)
    else:
        os.environ["CREATION_LIB_RESOURCE_DIR"] = old_resource_dir


def test_conversion_native_loader_sets_creation_lib_resource_dir(monkeypatch, tmp_path):
    from bacup_lib import native_runtime

    native_runtime._NATIVE = None
    resource_dir = tmp_path / "creation_lib" / "resources"

    def fake_import_module(name: str):
        if name == "bacup_lib._native":
            return SimpleNamespace(conversion_native=SimpleNamespace())
        raise ImportError(name)

    monkeypatch.delenv("CREATION_LIB_RESOURCE_DIR", raising=False)
    monkeypatch.setattr(native_runtime, "import_module", fake_import_module)
    monkeypatch.setattr("creation_lib.paths.get_resource_dir", lambda: resource_dir)

    native_runtime.load_native_module()

    assert Path(os.environ["CREATION_LIB_RESOURCE_DIR"]) == resource_dir


def test_conversion_native_loader_preserves_explicit_resource_dir(monkeypatch, tmp_path):
    from bacup_lib import native_runtime

    native_runtime._NATIVE = None
    explicit_dir = tmp_path / "explicit"
    packaged_dir = tmp_path / "packaged"

    def fake_import_module(name: str):
        if name == "bacup_lib._native":
            return SimpleNamespace(conversion_native=SimpleNamespace())
        raise ImportError(name)

    monkeypatch.setenv("CREATION_LIB_RESOURCE_DIR", str(explicit_dir))
    monkeypatch.setattr(native_runtime, "import_module", fake_import_module)
    monkeypatch.setattr("creation_lib.paths.get_resource_dir", lambda: packaged_dir)

    native_runtime.load_native_module()

    assert Path(os.environ["CREATION_LIB_RESOURCE_DIR"]) == explicit_dir
