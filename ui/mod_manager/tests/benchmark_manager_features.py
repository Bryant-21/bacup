"""Use hyperfine; all sources are disposable fixtures or synthetic index objects."""
import argparse
from pathlib import Path
import subprocess
import tempfile
import threading

from creation_lib.mod_manager.conflicts import FileEntry, FileIndex, Provider, build_index
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import reject_links
from .fixtures import make_mo2


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("copy", "instance", "scan", "scan-existing", "scan-timing", "installer-existing", "installer-timing", "filter"), required=True)
    parser.add_argument("--rows", type=int, default=20_000)
    parser.add_argument("--fixture-root", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.mode == "filter":
        entries = tuple(FileEntry(f"textures/Fixture{i:05d}.txt", (
            Provider("base", "Base", 0, True, False, "synthetic/base", 1, 1),
            Provider("override", "Override", 1, True, False, "synthetic/override", 1, 1))) for i in range(args.rows))
        index = FileIndex("fixture", "Synthetic index", entries, (), ())
        for query, mode, provider in (("0199", "all", ""), ("", "conflicts", ""),
                                       ("", "losing", "base"), ("missing", "all", ""), ("0199", "winning", "override")):
            result = index.filtered(query, mode, provider)
            for _ in range(10_000):
                assert index.filtered(query, mode, provider) is result
        print(f"{args.rows:,} synthetic indexed paths; five filters / 50,000 cached calls; no source I/O")
        return
    if args.mode in {"scan-existing", "installer-existing"}:
        root = args.fixture_root
        if not root or not root.absolute().is_relative_to(Path(tempfile.gettempdir()).resolve()):
            parser.error("Existing benchmark fixture must be a generated temporary folder.")
        reject_links(root / ".synthetic-benchmark")
        if (root / ".synthetic-benchmark").read_text() != "generated synthetic benchmark only\n":
            parser.error("Missing generated fixture marker.")
        library = Library(root / "library")
        if args.mode == "installer-existing":
            from creation_lib.mod_manager.fomod import Fomod
            from creation_lib.mod_manager.installer_context import capture, validate
            fomod = Fomod(b'<config><moduleName>Generated dependency scan</moduleName><moduleDependencies><fileDependency file="textures/Fixture00000.txt" state="Active"/></moduleDependencies><requiredInstallFiles><file source="base.txt"/></requiredInstallFiles></config>')
            context, states = capture(library, library.profiles()[0].id, fomod)
            assert context.file_count == args.rows + 2 and states["textures/fixture00000.txt"] == "Active"
            validate(library, context)
            print(f"FOMOD capture (two passes) + validation: {context.file_count:,} generated paths; setup/cleanup excluded")
            return
        index = build_index(library, library.profiles()[0].id)
        assert len(index.files) == args.rows + 3
        print(f"Scanned {len(index.files):,} generated paths; fixture creation/cleanup excluded")
        return
    with tempfile.TemporaryDirectory(prefix="mod-manager-features-benchmark-") as directory:
        root = Path(directory)
        source = make_mo2(root)
        count = 32 if args.mode in {"copy", "instance"} else args.rows
        size = 128 * 1024 if args.mode in {"copy", "instance"} else 1
        for mod in ("Base", "Override"):
            for number in range(count):
                (source / "mods" / mod / "textures" / f"Fixture{number:05d}.txt").write_bytes(bytes([number % 256]) * size)
        if args.mode == "instance":
            from creation_lib.mod_manager.mo2_migration import migrate
            orphan, overwrite = source / "mods" / "Unassigned fixture", source / "overwrite"
            orphan.mkdir()
            overwrite.mkdir()
            (orphan / "fixture.txt").write_bytes(b"generated unassigned data")
            (overwrite / "generated.txt").write_bytes(b"generated overwrite data")
            manager = ManagerController()
            try:
                result = migrate(source, root / "library", manager._validate_destination, manager._prepare_library,
                                 threading.Event(), lambda *args: None)
                assert result.state == "complete" and len(result.referenced) == 5 and len(result.library.profiles()) == 3
                repeated = migrate(source, root / "library", manager._validate_destination, manager._prepare_library,
                                   threading.Event(), lambda *args: None)
                assert repeated.new_profiles == 0 and len(repeated.referenced) == 5
                assert not tuple((result.library.root / "mods").iterdir())
                print("Reference metadata discovery/import/repeat: 2 source profiles, 5 original folder references; no payload scan/hash/copy; generated fixture setup/cleanup included")
            finally:
                manager.cleanup()
            return
        library = Library(root / "library", create=True)
        library.import_mo2(inspect_instance(source))
        profile = library.profiles()[0].id
        if args.mode in {"scan-timing", "installer-timing"}:
            if not args.output:
                parser.error("--output is required for scan-timing")
            (root / ".synthetic-benchmark").write_text("generated synthetic benchmark only\n")
            command = subprocess.list2cmdline(["uv", "run", "--no-sync", "python", "-m",
                                               "ui.mod_manager.tests.benchmark_manager_features", "--mode", "installer-existing" if args.mode == "installer-timing" else "scan-existing",
                                               "--rows", str(args.rows), "--fixture-root", root.as_posix()])
            subprocess.run(["hyperfine", "--shell", "none", "--warmup", "1", "--runs", "3",
                            "--export-json", str(args.output), command], check=True)
        elif args.mode == "copy":
            selected = [row.id for row in library.rows(profile) if row.name in {"Base", "Override"}]
            review = library.review_migration(selected)
            result = library.migrate(review)
            assert len(result) == 2 and len(library.origins()) == 2
            print(f"Reviewed/copied {sum(len(item.manifest) for item in review.sources):,} files / {review.bytes:,} bytes, two sources; originals retained")
        else:
            index = build_index(library, profile)
            assert len(index.files) == args.rows + 3
            assert len(index.filtered("", "conflicts")) == args.rows + 2
            print(f"Scanned {len(index.files):,} distinct paths / {sum(len(item.providers) for item in index.files):,} providers from synthetic disk fixtures")


if __name__ == "__main__":
    main()
