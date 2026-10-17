"""Actual managed mutation APIs; generated source/game files remain byte-identical."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
from pathlib import Path
import tempfile
import uuid

from creation_lib.mod_manager.project_vfs import NativeFilespace


def run(native_root, fault_root=None):
    native_root = Path(native_root).absolute()
    with tempfile.TemporaryDirectory(prefix="modbox-mutations-") as directory:
        root = Path(directory)
        game, source, owner = [root / name for name in ("game", "source", "manager")]
        data, output, temp = game / "Data", owner / "output", owner / "temp"
        for folder in (data / "tree", data / "empty", source, output, temp):
            folder.mkdir(parents=True)
        originals = {
            "delete.txt": "delete original", "atomic.txt": "original atomic",
            "rename.txt": "original rename", "tree/winner.txt": "tree winner",
            "tree/nested/child.txt": "nested child", "sharing.txt": "sharing original",
            "cancel.txt": "cancel original", "onclose.txt": "onclose original",
            "multiprocess.txt": "multiprocess moved",
        }
        for name, content in originals.items():
            path = source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        (data / "tree/base.txt").write_text("tree base")
        protected = {path: path.read_bytes() for base in (game, source) for path in base.rglob("*") if path.is_file()}

        def adapter(bundle=native_root):
            instance = NativeFilespace(bundle, uuid.uuid4().hex)
            instance.configure(owner, owner / "sessions" / uuid.uuid4().hex / "filespace.txt", protected=(str(game), str(source)))
            instance.link_directory(output, data, create=True)
            instance.link_directory(temp, temp, create=True)
            for name in originals:
                instance.link_file(source / name, data / name)
            return instance

        instance = adapter()
        report = owner / "mutations.json"
        try:
            instance.start([str(native_root / "modbox_vfs_fixture.exe"), "mutations", str(data), str(temp), str(report)], str(root))
            assert instance.kernel.WaitForSingleObject(instance.process.process, 45000) == 0, "Mutation fixture timed out"
            result = json.loads(report.read_text())
            result["exit_code"] = instance.exit_code()
        finally:
            instance.close()
        # A fresh manifest and process must observe persistent deletions; neither
        # session IDs nor frozen source mappings may resurrect removed winners.
        restart = adapter()
        persisted = owner / "persisted.txt"
        try:
            restart.start([str(native_root / "modbox_vfs_fixture.exe"), "grandchild", str(data / "delete.txt"), str(persisted)], str(root))
            assert restart.kernel.WaitForSingleObject(restart.process.process, 15000) == 0
            result["checks"]["new_session_whiteout_persists"] = persisted.read_text() == ""
        finally:
            restart.close()
        result["source_game_unchanged"] = all(path.read_bytes() == content for path, content in protected.items())
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateEventW.argtypes = [ctypes.c_void_p, wintypes.BOOL, wintypes.BOOL, wintypes.LPCWSTR]
        kernel.CreateEventW.restype = wintypes.HANDLE
        kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        kernel.WaitForSingleObject.restype = wintypes.DWORD
        kernel.SetEvent.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
        kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        kernel.CreateFileW.restype = wintypes.HANDLE
        kernel.ReadFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]

        def shared_read(path):
            handle = kernel.CreateFileW(str(path), 0x80000000, 7, None, 3, 0, None)
            assert handle != ctypes.c_void_p(-1).value, ctypes.get_last_error()
            try:
                buffer, count = ctypes.create_string_buffer(256), wintypes.DWORD()
                assert kernel.ReadFile(handle, buffer, 256, ctypes.byref(count), None)
                return buffer.raw[:count.value].decode()
            finally:
                kernel.CloseHandle(handle)

        def events():
            prefix = "Local\\ModBoxFixture_" + uuid.uuid4().hex
            return prefix, [kernel.CreateEventW(None, True, False, prefix + suffix) for suffix in ("_entered", "_release")]

        prefix, signals = events()
        observer, writer = adapter(), adapter()
        try:
            observer.start([str(native_root / "modbox_vfs_fixture.exe"), "lease-wait", str(data / "atomic.txt"), str(data / "multiprocess-moved.txt"), prefix], str(root))
            assert kernel.WaitForSingleObject(signals[0], 15000) == 0, "Observer did not hold its managed file"
            writer.start([str(native_root / "modbox_vfs_fixture.exe"), "contended", str(data / "atomic.txt"), str(data / "multiprocess.txt"), str(data / "multiprocess-moved.txt")], str(root))
            assert kernel.WaitForSingleObject(writer.process.process, 15000) == 0
            result["checks"]["multiprocess_sharing_and_rename"] = writer.exit_code() == 0
            kernel.SetEvent(signals[1])
            assert kernel.WaitForSingleObject(observer.process.process, 15000) == 0
            result["checks"]["multiprocess_epoch_refresh"] = observer.exit_code() == 0
        finally:
            writer.close()
            observer.close()
            for signal in signals:
                kernel.CloseHandle(signal)

        if fault_root:
            fault_root = Path(fault_root).absolute()
            for mode in ("fail", "pause", "directory-fail", "directory-pause"):
                (output / "transaction-source.txt").write_text("transaction source")
                (output / "transaction-destination.txt").write_text("transaction destination")
                prefix, signals = events()
                directory_test = mode.startswith("directory")
                if directory_test:
                    directory_source = output / ("transaction-dir-" + mode)
                    directory_source.mkdir()
                    (directory_source / "child.txt").write_text("transaction child")
                    virtual_source = data / directory_source.name
                    virtual_destination = data / (directory_source.name + "-moved")
                    physical_destination = output / virtual_destination.name
                else:
                    virtual_source, virtual_destination = data / "transaction-source.txt", data / "transaction-destination.txt"
                    physical_destination = output / "transaction-destination.txt"
                transaction = adapter(fault_root)
                try:
                    transaction.start([str(fault_root / "modbox_vfs_fixture.exe"), "transaction", str(virtual_source), str(virtual_destination), prefix, mode], str(root))
                    if mode.endswith("fail"):
                        assert kernel.WaitForSingleObject(transaction.process.process, 15000) == 0
                        result["checks"]["directory_failed_commit_live_rollback" if directory_test else "failed_commit_live_rollback"] = transaction.exit_code() == 0
                    else:
                        assert kernel.WaitForSingleObject(signals[0], 15000) == 0, "Transaction did not reach physical rename"
                        if directory_test:
                            assert (physical_destination / "child.txt").read_text() == "transaction child" and not directory_source.exists()
                        else:
                            assert shared_read(physical_destination) == "transaction source" and not (output / "transaction-source.txt").exists()
                        assert kernel.TerminateProcess(transaction.process.process, 49)
                        assert kernel.WaitForSingleObject(transaction.process.process, 5000) == 0
                finally:
                    transaction.close()
                    for signal in signals:
                        kernel.CloseHandle(signal)
                if mode.endswith("pause"):
                    recovery = adapter()
                    try:
                        recovered = virtual_source / "child.txt" if directory_test else virtual_source
                        recovery.start([str(native_root / "modbox_vfs_fixture.exe"), "grandchild", str(recovered), str(owner / "recovered.txt")], str(root))
                        assert kernel.WaitForSingleObject(recovery.process.process, 15000) == 0
                        if directory_test:
                            result["checks"]["crashed_directory_journal_recovers"] = (owner / "recovered.txt").read_text() == "transaction child" and not physical_destination.exists()
                        else:
                            result["checks"]["crashed_rename_journal_recovers"] = (owner / "recovered.txt").read_text() == "transaction source" and physical_destination.read_text() == "transaction destination"
                    finally:
                        recovery.close()
            orphan_tree = output / "orphan-tree"
            orphan_tree.mkdir()
            (orphan_tree / "orphan.txt").write_text("owned orphan")
            prefix, signals = events()
            deletion = adapter(fault_root)
            try:
                deletion.start([str(fault_root / "modbox_vfs_fixture.exe"), "delete-pause", str(data / "orphan-tree/orphan.txt"), prefix], str(root))
                assert kernel.WaitForSingleObject(signals[0], 15000) == 0
                assert kernel.TerminateProcess(deletion.process.process, 59)
                assert kernel.WaitForSingleObject(deletion.process.process, 5000) == 0
            finally:
                deletion.close()
                for signal in signals:
                    kernel.CloseHandle(signal)
            assert (orphan_tree / "orphan.txt").read_text() == "owned orphan", "Fixture did not retain a committed-but-unflagged owned deletion"
            cleanup = adapter()
            try:
                cleanup.start([str(native_root / "modbox_vfs_fixture.exe"), "hidden-tree", str(data / "orphan-tree"), str(data / "orphan-tree-moved")], str(root))
                assert kernel.WaitForSingleObject(cleanup.process.process, 15000) == 0
                result["checks"]["crashed_delete_tombstone_moves_and_cleans"] = cleanup.exit_code() == 0 and not (output / "orphan-tree-moved").exists()
            finally:
                cleanup.close()
        result["source_game_unchanged"] = all(path.read_bytes() == content for path, content in protected.items())
        result["backend_sha256"] = hashlib.sha256((native_root / "modbox_vfs.dll").read_bytes()).hexdigest()
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-root", type=Path, default=Path("cache/mod-manager-native/Release"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fault-root", type=Path)
    args = parser.parse_args()
    result = run(args.native_root, args.fault_root)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if not result["exit_code"] and result["source_game_unchanged"] and all(result["checks"].values()) else 1)
