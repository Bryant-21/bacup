"""Synthetic metadata benchmarks; run with hyperfine, not a real game library."""
from __future__ import annotations

import argparse
from pathlib import Path
import tempfile

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import ImportedMod, ImportedProfile, ImportPreview


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rows", type=int, default=20000)
    parser.add_argument("--mode", choices=("import", "search"), default="import")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="mod-manager-benchmark-") as temporary:
        root = Path(temporary)
        library = Library(root / "library", create=True)
        mods = [ImportedMod(str(i), f"Fixture Mod {i:05d}", str(root / "source" / f"mod{i}"), "external",
                            i % 3 != 0, f"Group {i % 20}", "1.0", "Visuals") for i in range(args.rows)]
        profile = ImportedProfile("benchmark-profile", "Benchmark", str(root / "source" / "profiles" / "main"), mods, {})
        preview = ImportPreview(str(root / "source"), str(root / "source" / "mods"), str(root / "source" / "profiles"), "fo4", [profile])
        library.import_mo2(preview)
        manager = ManagerController()
        manager.library = library
        try:
            manager.refresh()
            if args.mode == "search":
                for query in ("0199", "Group 4", "Visuals", "not present", "0199"):
                    manager.filtered(query)
                    for _ in range(10000):
                        manager.filtered(query)
            assert len(manager.rows) == args.rows
            assert library.import_mo2(preview) == 0
        finally:
            manager.cleanup()
    print(f"{args.mode}: {args.rows:,} synthetic metadata rows; no payload I/O")


if __name__ == "__main__":
    main()
