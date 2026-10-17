"""Generated distinct metadata files; use Hyperfine for whole-command timing."""
import argparse
import json
from pathlib import Path
import tempfile

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.metadata_observer import scan


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--files", type=int, default=5000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 129 <= args.files <= 100_000:
        parser.error("Use 129–100,000 generated files")
    with tempfile.TemporaryDirectory(prefix="modbox-source-metadata-probe-") as temporary:
        root = Path(temporary)
        library = Library(root / "library", create=True)
        sources = root / "source"
        sources.mkdir()
        rows = []
        for number in range(args.files):
            folder = sources / str(number)
            folder.mkdir()
            (folder / "meta.ini").write_bytes(b"[General]\nversion=1\n")
            rows.append((f"{number:032x}", f"Generated {number}", str(folder)))
        with library._db() as db:
            db.executemany("INSERT INTO mods(id,name,source,kind) VALUES (?,?,?,'external')", rows)
        selected = rows[-1][0]
        first = scan(library, selected=selected)
        assert first.count == 0 and first.visited == 128
        changed = Path(rows[-1][2]) / "meta.ini"
        changed.write_bytes(b"[General]\nversion=2\n")
        second = scan(library, first.targets, first.cursor, first.visited, selected=selected)
        assert second.count == 1 and second.notices[0].owner_id == selected and second.visited == 256
        with library._db() as db:
            cached = db.execute("SELECT count(*) FROM source_metadata_observations").fetchone()[0]
        assert cached == 257  # Two disjoint 128-file batches plus the selected file.
        report = {"distinct_files": args.files, "rotating_batch_files": 128, "selected_extra_files": 1,
                  "files_cached_after_two_batches": cached, "selected_change_notified": True,
                  "unvisited_files_not_claimed_fresh": args.files - cached,
                  "limitations": "Whole-command timing includes generating/removing every directory/file, library initialization, two bounded scan/cache commits and writing this report. Tiny synthetic metadata; no watcher cycle, real payload, MO2/game path or storage SLA measured."}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
