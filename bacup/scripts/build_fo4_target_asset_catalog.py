from __future__ import annotations

import argparse
import shutil
import sqlite3
from pathlib import Path


_CATALOG_VIEWS = """
CREATE VIEW IF NOT EXISTS catalog_assets AS
    SELECT
        CASE WHEN d.path_key = '' THEN a.name_key
             ELSE d.path_key || '/' || a.name_key END AS path_key,
        CASE WHEN d.path_key = '' THEN a.name_key
             ELSE d.path_key || '/' || a.name_key END AS canonical_path,
        a.kind AS asset_type,
        ar.name AS archive_owner,
        ao.priority AS load_priority
    FROM assets a
    JOIN directories d ON d.id = a.directory_id
    JOIN asset_owners ao ON ao.asset_id = a.id
    JOIN archives ar ON ar.id = ao.archive_id;
CREATE VIEW IF NOT EXISTS catalog_dependencies AS
    SELECT
        CASE WHEN sd.path_key = '' THEN sa.name_key
             ELSE sd.path_key || '/' || sa.name_key END AS source_key,
        CASE WHEN td.path_key = '' THEN ta.name_key
             ELSE td.path_key || '/' || ta.name_key END AS target_key,
        dep.ref_kind
    FROM asset_dependencies dep
    JOIN assets sa ON sa.id = dep.source_asset_id
    JOIN directories sd ON sd.id = sa.directory_id
    JOIN assets ta ON ta.id = dep.target_asset_id
    JOIN directories td ON td.id = ta.directory_id;
"""


def _finalize_completed_temp(output: Path) -> None:
    temporary = output.with_suffix(".sqlite3.tmp")
    if not temporary.is_file():
        raise FileNotFoundError(f"completed catalog temp file is missing: {temporary}")
    with sqlite3.connect(temporary) as db:
        integrity = db.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise RuntimeError(f"catalog temp integrity check failed: {integrity}")
        db.executescript(_CATALOG_VIEWS)
    try:
        temporary.replace(output)
    except PermissionError:
        publish_copy = output.with_suffix(".sqlite3.publish.tmp")
        shutil.copy2(temporary, publish_copy)
        publish_copy.replace(output)
        try:
            temporary.unlink(missing_ok=True)
        except PermissionError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build the packaged FO4 target-asset catalog directly from official BA2s."
    )
    parser.add_argument("fo4_data_dir", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--game-build", default="")
    parser.add_argument("--workers", type=int)
    parser.add_argument(
        "--finalize-temp",
        action="store_true",
        help="Validate and publish a completed .sqlite3.tmp after an interrupted rename.",
    )
    args = parser.parse_args()

    if args.finalize_temp:
        _finalize_completed_temp(args.output)
        return 0

    from bacup_lib.native_runtime import load_native_module

    native = load_native_module()
    builder = getattr(native, "conversion_build_target_asset_catalog", None)
    if builder is None:
        raise RuntimeError(
            "conversion native runtime lacks catalog builder; run scripts/ensure_native.py"
        )
    builder(
        str(args.fo4_data_dir),
        str(args.output),
        str(args.game_build),
        args.workers,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
