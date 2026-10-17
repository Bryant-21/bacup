"""Load the in-tree native extensions from shadow copies.

Windows locks a loaded .pyd, so a running toolkit would block maturin from
rebuilding `creation_lib._native` / `bacup_lib._native`.
"""

from __future__ import annotations

import importlib
import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path

_PACKAGES = ("creation_lib", "bacup_lib")


def _shadow_load(package_name: str) -> None:
    module_name = f"{package_name}._native"
    if module_name in sys.modules:
        return
    try:
        package = importlib.import_module(package_name)
    except ImportError:
        return
    source = Path(package.__file__).parent / "_native.pyd"
    if not source.is_file():
        return

    stat = source.stat()
    root = Path(tempfile.gettempdir()) / "modbox21-native" / package_name
    stamp = f"{stat.st_size}-{stat.st_mtime_ns}"
    target = root / stamp / "_native.pyd"
    if not target.is_file():
        target.parent.mkdir(parents=True, exist_ok=True)
        partial = target.with_suffix(".partial")
        shutil.copy2(source, partial)
        partial.replace(target)
    for old in root.iterdir():
        if old.name != stamp:
            # Copies still loaded by another toolkit instance stay locked; skip them.
            shutil.rmtree(old, ignore_errors=True)

    spec = importlib.util.spec_from_file_location(module_name, target)
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    setattr(package, "_native", module)


def load_native_from_shadow_copies() -> None:
    for package_name in _PACKAGES:
        try:
            _shadow_load(package_name)
        except OSError:
            pass  # fall back to the normal in-place import
