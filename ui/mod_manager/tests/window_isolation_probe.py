import ctypes as ct
from ctypes import wintypes as w
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid

from creation_lib.mod_manager.project_vfs import NativeFilespace


def run(native_root):
    native_root = Path(native_root).absolute()
    kernel = ct.WinDLL("kernel32", use_last_error=True)
    kernel.CreateEventW.argtypes = [ct.c_void_p, w.BOOL, w.BOOL, w.LPCWSTR]
    kernel.CreateEventW.restype = w.HANDLE
    kernel.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
    kernel.SetEvent.argtypes = [w.HANDLE]
    kernel.CloseHandle.argtypes = [w.HANDLE]
    processes, handles = [], []
    token = uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix="modbox-window-fixture-") as temporary:
        root = Path(temporary)
        game = root / "Fallout4.exe"
        shutil.copyfile(native_root / "modbox_window_fixture.exe", game)
        stop_name = "Local\\ModBoxStop" + token
        stop = kernel.CreateEventW(None, True, False, stop_name)
        handles.append(stop)
        try:
            for index, (executable, name) in enumerate(((game, "ModBoxGameWindow"), (native_root / "modbox_window_fixture.exe", "ModBoxOtherWindow"))):
                ready_name = "Local\\ModBoxReady" + token + str(index)
                ready = kernel.CreateEventW(None, True, False, ready_name)
                handles.append(ready)
                processes.append(subprocess.Popen([str(executable), "server", name, ready_name, stop_name], creationflags=subprocess.CREATE_NO_WINDOW))
                assert kernel.WaitForSingleObject(ready, 10000) == 0, "Hidden fixture window failed to initialize"
            owner = root / "manager"
            owner.mkdir()
            adapter = NativeFilespace(native_root, uuid.uuid4().hex)
            try:
                adapter.configure(owner, owner / "session" / "filespace.txt")
                adapter.start([str(game), "check"], str(root))
                assert adapter.kernel.WaitForSingleObject(adapter.process.process, 10000) == 0
                assert adapter.exit_code() == 0, f"Window isolation fixture failed: {adapter.exit_code()}"
            finally:
                adapter.close()
        finally:
            kernel.SetEvent(stop)
            for process in processes:
                process.wait(timeout=15)
            for handle in handles:
                kernel.CloseHandle(handle)
    return {"foreign_game_hidden": True, "own_window_visible": True, "unrelated_window_visible": True}


if __name__ == "__main__":
    import sys
    print(run(sys.argv[1]))
