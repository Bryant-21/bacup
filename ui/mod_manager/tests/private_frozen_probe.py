"""Private onedir GUI/helper acceptance from a retained disposable copy."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import uuid

from PIL import ImageGrab

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.process_identity import from_handle, terminate_owned
from .fixtures import make_mo2
from .project_runtime_probe import run as helper_probe


def run(package, evidence, existing_copy=None, refresh_copy=False):
    package, evidence = Path(package).absolute(), Path(evidence).absolute()
    evidence.mkdir(parents=True, exist_ok=True)
    if existing_copy:
        copied = Path(existing_copy).resolve()
        root = copied.parent
        assert root.name.startswith("modbox-private-acceptance-") and root.is_relative_to(Path(tempfile.gettempdir()).resolve()), "Acceptance can reuse only its generated Temp copy"
        if refresh_copy:
            shutil.copytree(package, copied, dirs_exist_ok=True)
    else:
        root = Path(tempfile.mkdtemp(prefix="modbox-private-acceptance-"))
        copied = root / "app"
        shutil.copytree(package, copied)
    executable = copied / "ModBox21-ModManager.exe"
    from creation_lib.mod_manager.native_builder import validate
    payload = validate(copied / "_internal/creation_lib/mod_manager/_runtime/project-vfs")
    native = ctypes.WinDLL(str(payload / "modbox_vfs.dll"))
    assert not hasattr(native, "MBVFS_TestControl"), "Private delivery must not contain fixture fault controls"
    fixture_root = root / ("fixtures-" + uuid.uuid4().hex)
    fixture_root.mkdir()
    source = make_mo2(fixture_root)
    library = Library(root / ("library-" + uuid.uuid4().hex), create=True)
    library.import_mo2(inspect_instance(source))
    protected = {path: path.read_bytes() for path in source.rglob("*") if path.is_file()}
    settings = copied / "settings"
    (settings / "variants").mkdir(parents=True, exist_ok=True)
    (settings / "shared_settings.json").write_text(json.dumps({"setup_complete": False, "paths": {}, "indexes": {"fo4_data": False, "fnv_data": False, "fo76_data": False}, "theme": "modern"}))
    (settings / "variants/mod_manager.json").write_text(json.dumps({"active_workspace": "mod_manager", "window_width": 1280, "window_height": 900, "workspaces": {"mod_manager": {"library_path": str(library.root), "auto_updates": False}}}))
    environment = dict(os.environ)
    environment.pop("MOD_ROOT", None)
    ready = root / ("gui-ready-" + uuid.uuid4().hex)
    environment["MODBOX21_READY_FILE"] = str(ready)
    user = ctypes.WinDLL("user32", use_last_error=True)
    user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user.IsWindowVisible.argtypes = [wintypes.HWND]
    user.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    user.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
    user.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.UINT]
    user.SetForegroundWindow.argtypes = [wintypes.HWND]
    user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    child = subprocess.Popen([str(executable)], cwd=copied, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    identity = from_handle(child.pid, child._handle)
    result = {"retained_private_copy": str(copied), "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest()}
    try:
        deadline = time.monotonic() + 60
        while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
            time.sleep(.1)
        assert ready.exists() and child.poll() is None, "Frozen GUI did not signal readiness; inspect copied logs"
        # Readiness precedes asynchronous library restoration. Give this tiny
        # generated library time to finish; visual inspection verifies its rows.
        time.sleep(3)
        found = []
        @callback_type
        def inspect_window(window, _):
            pid = wintypes.DWORD()
            user.GetWindowThreadProcessId(window, ctypes.byref(pid))
            if pid.value == child.pid and user.IsWindowVisible(window):
                found.append(window)
            return True
        user.EnumWindows(inspect_window, 0)
        assert found, "No visible window belonged to the retained GUI process"
        def area(window):
            rect = wintypes.RECT()
            user.GetWindowRect(window, ctypes.byref(rect))
            return max(0, rect.right - rect.left) * max(0, rect.bottom - rect.top)
        window = max(found, key=area)
        user.ShowWindow(window, 9)  # Restore only the newly owned GUI window.
        user.SetWindowPos(window, None, 40, 40, 1280, 900, 0x40)
        user.SetForegroundWindow(window)
        time.sleep(1)
        rect = wintypes.RECT()
        assert user.GetWindowRect(window, ctypes.byref(rect))
        captured = ImageGrab.grab(window=int(window))
        capture_method = "window"
        if not any(upper - lower > 8 for lower, upper in captured.convert("RGB").getextrema()):
            # Some OpenGL/compositor combinations return a blank PrintWindow image.
            # Crop only this owned foreground window from the screen in that case.
            try:
                captured = ImageGrab.grab(bbox=(rect.left, rect.top, rect.right, rect.bottom), all_screens=True)
                capture_method = "owned-window-screen-crop"
            except OSError as error:
                # An unavailable interactive desktop is a capture limitation,
                # not evidence of successful rendered-pixel acceptance.
                result["capture_error"] = str(error)
        capture_nonblank = any(upper - lower > 8 for lower, upper in captured.convert("RGB").getextrema())
        captured.save(evidence / "private-frozen-manager.png")
        result.update(capture_method=capture_method, capture_nonblank=capture_nonblank)
        user.PostMessageW(window, 0x10, 0, 0)
        child.wait(timeout=20)
        assert child.returncode == 0
        result.update(gui_ready=True, gui_normal_close=True, source_unchanged=all(path.read_bytes() == content for path, content in protected.items()))
    finally:
        if child.poll() is None:
            terminate_owned(child, identity)
            child.wait(timeout=10)
        (evidence / "private-frozen-gui.json").write_text(json.dumps(result, indent=2))
    result["helper"] = helper_probe(executable)
    (evidence / "private-frozen-helper.json").write_text(json.dumps(result["helper"], indent=2))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--existing-copy", type=Path)
    parser.add_argument("--refresh-copy", action="store_true")
    args = parser.parse_args()
    print(json.dumps(run(args.package, args.evidence, args.existing_copy, args.refresh_copy), indent=2))
