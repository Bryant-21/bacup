"""Subprocess host for VST3 plugin editor windows.

Launched by FilterBuilderPanel._open_plugin_editor().  Runs on its own main
thread so the plugin's native GUI window can open without blocking the ImGui
render loop.

Usage: python -m ui.voice_changer._plugin_editor_host <tmp_json_path>

The JSON file contains {"plugin_path": ..., "backend": ..., "params": {...}}.
On exit the file is overwritten with the updated params.
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import json
import os
import sys
import threading
import time

# Win32 helpers for centering the editor window
_user32 = ctypes.windll.user32
_kernel32 = ctypes.windll.kernel32
_SWP_NOSIZE = 0x0001
_SWP_NOZORDER = 0x0004


def _find_process_window() -> int:
    """Return the HWND of the first visible top-level window in this process."""
    pid = _kernel32.GetCurrentProcessId()
    found: list[int] = []

    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def _cb(hwnd, _):
        if _user32.IsWindowVisible(hwnd):
            w_pid = wt.DWORD()
            _user32.GetWindowThreadProcessId(hwnd, ctypes.byref(w_pid))
            if w_pid.value == pid:
                found.append(hwnd)
                return False
        return True

    _user32.EnumWindows(_cb, 0)
    return found[0] if found else 0


def _center_editor_window():
    """Background thread: wait for the editor window to appear then center it."""
    for _ in range(100):           # poll up to 10 s
        time.sleep(0.1)
        hwnd = _find_process_window()
        if not hwnd:
            continue
        rect = wt.RECT()
        _user32.GetWindowRect(hwnd, ctypes.byref(rect))
        w = rect.right - rect.left
        h = rect.bottom - rect.top
        sw = _user32.GetSystemMetrics(0)
        sh = _user32.GetSystemMetrics(1)
        x = max(0, (sw - w) // 2)
        y = max(0, (sh - h) // 2)
        _user32.SetWindowPos(hwnd, None, x, y, 0, 0, _SWP_NOSIZE | _SWP_NOZORDER)
        return


def _write_params_atomic(tmp_path: str, data: dict):
    """Write params JSON atomically via write-to-tmp + os.replace."""
    tmp_write = tmp_path + ".tmp"
    try:
        with open(tmp_write, "w") as f:
            json.dump(data, f)
        os.replace(tmp_write, tmp_path)
    except OSError:
        pass


def _read_params(plugin, param_names: list[str]) -> dict:
    """Read current parameter values from a pedalboard plugin."""
    snapshot = {}
    for name in param_names:
        if hasattr(plugin, name):
            val = getattr(plugin, name)
            if isinstance(val, bool):
                snapshot[name] = val
            elif isinstance(val, str):
                snapshot[name] = val
            else:
                try:
                    snapshot[name] = float(val)
                except (TypeError, ValueError):
                    snapshot[name] = 1.0 if val else 0.0
    return snapshot


def _read_params_dawdreamer(processor, name_to_index: dict, param_names: list[str]) -> dict[str, float]:
    """Read current parameter values from a DawDreamer processor."""
    snapshot = {}
    for name in param_names:
        idx = name_to_index.get(name)
        if idx is not None:
            snapshot[name] = processor.get_parameter(idx)
    return snapshot


def _poll_loop(read_fn, tmp_path: str, data: dict, stop: threading.Event):
    """Background thread: snapshot params to JSON every 200ms while editor is open."""
    while not stop.wait(0.2):
        data["params"] = read_fn()
        _write_params_atomic(tmp_path, data)
    # One final write after editor closes
    data["params"] = read_fn()
    _write_params_atomic(tmp_path, data)


def main():
    tmp_path = sys.argv[1]
    with open(tmp_path, "r") as f:
        data = json.load(f)

    plugin_path: str = data["plugin_path"]
    backend: str = data["backend"]
    params: dict[str, float] = data["params"]

    threading.Thread(target=_center_editor_window, daemon=True).start()

    if backend == "dawdreamer":
        _run_dawdreamer(plugin_path, params, data, tmp_path)
    else:
        _run_pedalboard(plugin_path, params, data, tmp_path)


def _run_pedalboard(plugin_path: str, params: dict, data: dict, tmp_path: str):
    import pedalboard

    plugin = pedalboard.load_plugin(plugin_path)
    param_names = list(params.keys())

    for name, value in params.items():
        if not hasattr(plugin, name):
            continue
        try:
            p = plugin.parameters.get(name)
            if p and getattr(p, "type", None) == bool:
                setattr(plugin, name, bool(value))
            elif isinstance(value, str):
                setattr(plugin, name, value)
            else:
                setattr(plugin, name, float(value))
        except (ValueError, TypeError):
            pass

    # Start polling thread — writes param snapshots to JSON every 200ms
    poll_stop = threading.Event()
    poll_thread = threading.Thread(
        target=_poll_loop,
        args=(lambda: _read_params(plugin, param_names), tmp_path, data, poll_stop),
        daemon=True,
    )
    poll_thread.start()

    # show_editor blocks until the user closes the plugin's native window.
    plugin.show_editor()

    # Let plugin finalize, then stop polling (final write happens in _poll_loop)
    time.sleep(0.15)
    poll_stop.set()
    poll_thread.join(timeout=2)


def _run_dawdreamer(plugin_path: str, params: dict, data: dict, tmp_path: str):
    import dawdreamer

    engine = dawdreamer.RenderEngine(sample_rate=44100, block_size=512)
    name = os.path.splitext(os.path.basename(plugin_path))[0]
    processor = engine.make_plugin_processor(name, plugin_path)

    descs = processor.get_plugin_parameters_description()
    name_to_index = {d["name"]: d["index"] for d in descs}
    param_names = list(params.keys())

    for param_name, value in params.items():
        idx = name_to_index.get(param_name)
        if idx is not None:
            processor.set_parameter(idx, float(value))

    poll_stop = threading.Event()
    poll_thread = threading.Thread(
        target=_poll_loop,
        args=(
            lambda: _read_params_dawdreamer(processor, name_to_index, param_names),
            tmp_path, data, poll_stop,
        ),
        daemon=True,
    )
    poll_thread.start()

    # open_editor blocks until the user closes the window
    processor.open_editor()

    time.sleep(0.15)
    poll_stop.set()
    poll_thread.join(timeout=2)


if __name__ == "__main__":
    main()
