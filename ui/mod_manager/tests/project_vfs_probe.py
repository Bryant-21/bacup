"""Actual original native interception, exclusively against generated local fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
import time
import uuid

from creation_lib.mod_manager.project_vfs import NativeFilespace
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.process_identity import process_state


def run(native_root):
    native_root = Path(native_root).absolute()
    with tempfile.TemporaryDirectory(prefix="modbox-project-vfs-") as directory:
        root = Path(directory)
        game, low, high, owner = [root / name for name in ("game", "low", "high", "manager")]
        data, output = game / "Data", owner / "output"
        for folder in (data, low, high, output):
            folder.mkdir(parents=True)
        (data / "base.txt").write_bytes(b"game")
        (game / "root.txt").write_bytes(b"root")
        (data / "shared.txt").write_bytes(b"physical")
        (low / "shared.txt").write_bytes(b"losing")
        (high / "shared.txt").write_bytes(b"winning")
        (high / "mod.txt").write_bytes(b"only")
        protected = {path: path.read_bytes() for base in (game, low, high) for path in base.rglob("*") if path.is_file()}
        adapter = NativeFilespace(native_root, uuid.uuid4().hex)
        adapter.configure(owner, owner / "session" / "filespace.txt", protected=(str(game), str(low), str(high)))
        adapter.link_directory(output, data, create=True)
        adapter.link_directory(owner / "game-root", game, create=True)
        adapter.link_file(high / "shared.txt", data / "shared.txt")
        adapter.link_file(high / "mod.txt", data / "virtual" / "mod.txt")
        (output / "snapshot.ini").write_text("original owned snapshot")
        adapter.link_file(output / "snapshot.ini", game / "Settings.ini")
        report = owner / "result.json"
        try:
            adapter.start([str(native_root / "modbox_vfs_fixture.exe"), "probe", str(data), str(high), str(report)], str(root))
            assert adapter.kernel.WaitForSingleObject(adapter.process.process, 30000) == 0, "Native fixture did not finish"
            result = json.loads(report.read_text())
            result["exit_code"] = adapter.exit_code()
            result["source_and_game_unchanged"] = all(path.read_bytes() == value for path, value in protected.items())
            assert (output / "snapshot.ini").read_text() == "owned snapshot write" and not (game / "Settings.ini").exists()
            assert (owner / "game-root/ScreenShot1.png").read_bytes() == b"png" and not (game / "ScreenShot1.png").exists()
            result["owned_output"] = {str(path.relative_to(output)): path.read_text() for path in output.rglob("*") if path.is_file()}
            result["backend_sha256"] = hashlib.sha256((native_root / "modbox_vfs.dll").read_bytes()).hexdigest()
            class InvalidManifest(NativeFilespace):
                def _write_config(self):
                    super()._write_config()
                    self.config.write_bytes("INVALID\n".encode("utf-16-le"))
            broken = InvalidManifest(native_root, uuid.uuid4().hex)
            broken.configure(owner, owner / "invalid-session/filespace.txt")
            unexpected = owner / "unexpected-unhooked-report.json"
            identities = []
            try:
                try:
                    broken.start([str(native_root / "modbox_vfs_fixture.exe"), "probe", str(data), str(high), str(unexpected)], str(root),
                                 before_resume=lambda pid, identity: identities.append(identity))
                except ManagerError as error:
                    assert "injection/initialization failed" in str(error)
                else:
                    raise AssertionError("Malformed native manifest unexpectedly launched")
            finally:
                broken.close()
            deadline = time.monotonic() + 5
            while identities and process_state(identities[0]) == "alive" and time.monotonic() < deadline:
                time.sleep(.03)
            assert identities and process_state(identities[0]) in {"exited", "reused"} and not unexpected.exists()
            result["failure_checks"] = {"invalid_manifest_no_fallback": True, "failed_injection_owned_process_reaped": True}
            return result
        finally:
            adapter.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-root", type=Path, default=Path("cache/mod-manager-native/Release"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.native_root)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if not result["exit_code"] and result["source_and_game_unchanged"] and all(result["checks"].values()) else 1)
