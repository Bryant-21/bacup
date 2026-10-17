"""Generated metadata-only model/save inspection benchmarks; no game assets."""
import argparse
from pathlib import Path
import tempfile

from creation_lib.mod_manager.archive_index import Archive
from creation_lib.mod_manager.combined_data import resolve
from creation_lib.mod_manager.conflicts import FileEntry, Provider
from creation_lib.mod_manager.data_tree import Tree
from creation_lib.mod_manager.save_inspection import compare_saved, inspect
from .test_save_inspection import fixture


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("combined", "save"), required=True)
    parser.add_argument("--rows", type=int, default=50_000)
    args = parser.parse_args()
    if args.mode == "save":
        with tempfile.TemporaryDirectory(prefix="mod-manager-save-metadata-bench-") as directory:
            source = fixture(Path(directory) / "generated.fos", regular=tuple(f"Full{index:03}.esp" for index in range(254)), light=tuple(f"Light{index:04}.esl" for index in range(4096)))
            saved = inspect(source)
            assert compare_saved(saved, inspect(source)).same_plugins
            print("Two bounded reads + comparison: 254 regular / 4096 light plugins; generated fixture creation/cleanup included; no body decoding")
        return
    owned = Provider("owned", "Owned", 1, True, False, "generated metadata path", 1, 1)
    archived = Provider("base", "Generated archive", -1, True, False, "generated archive path", 1, 1)
    archive = Archive("Base.ba2", archived, True, "listed", "Generated", 0)
    entries = []
    for index in range(args.rows):
        path = f"textures/generated-{index:06}.txt"
        loose = FileEntry(path, (owned,))
        entries.append(resolve(path, loose, (archive,), {"binvalidateolderfiles": "1", "sresourcedatadirsfinal": ""}))
    tree = Tree(tuple(entries))
    rows = tree.visible({"textures"}, "")
    assert len(rows) == args.rows + 1 and all(entry.winner.kind == "loose" for entry in entries)
    for _ in range(10_000):
        assert tree.visible({"textures"}, "") is rows
    print(f"{args.rows:,} generated combined entries, hierarchy build and 10k cached visible calls; no disk inventory/archive decoding")


if __name__ == "__main__":
    main()
