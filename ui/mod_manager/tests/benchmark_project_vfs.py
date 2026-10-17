"""Synthetic native timing; no real game/source/payload is read or deployed."""
import argparse
import json
from pathlib import Path
import tempfile
import uuid

from creation_lib.mod_manager.project_vfs import NativeFilespace


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-root", type=Path, default=Path("cache/mod-manager-native/Release"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--unique-sources", action="store_true", help="Generate a distinct 7-byte backing file for every mapping")
    args = parser.parse_args()
    native = args.native_root.absolute()
    with tempfile.TemporaryDirectory(prefix="modbox-native-benchmark-") as directory:
        root = Path(directory)
        owner, data, source = root / "owner", root / "game/Data", root / "source"
        for folder in (owner / "output", data, source):
            folder.mkdir(parents=True)
        file = source / "one-fixture.txt"
        file.write_bytes(b"winning")
        adapter = NativeFilespace(native, uuid.uuid4().hex)
        adapter.configure(owner, owner / "filespace.txt", (str(data.parent), str(source)))
        adapter.link_directory(owner / "output", data, create=True)
        for index in range(50000):
            backing = file
            if args.unique_sources:
                backing = source / f"source{index:05d}.txt"
                backing.write_bytes(b"winning")
            adapter.link_file(backing, data / f"mapped{index:05d}.txt")
        result_path = owner / "result.json"
        try:
            adapter.start([str(native / "modbox_vfs_fixture.exe"), "benchmark", str(data), str(result_path)], str(root))
            assert adapter.kernel.WaitForSingleObject(adapter.process.process, 30000) == 0
            result = json.loads(result_path.read_text())
            assert not adapter.exit_code() and result["valid"] and result["files"] == 50000
            assert file.read_bytes() == b"winning" and not list(data.iterdir()) and not list((owner / "output").iterdir())
            result["source_files"] = 50000 if args.unique_sources else 1
            result["limits"] = f"50k mappings reference {result['source_files']} generated 7-byte source files; warm native lookups/metadata and one full synchronous enumeration. Native timings exclude initialization/injection. Command timing includes fixture creation/config/injection/cleanup. Shared host; no cold real payload, API, game, solid archive or UI SLA."
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, indent=2))
        finally:
            adapter.close()


if __name__ == "__main__":
    main()
