from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil

from bacup_lib.inspect_ui import OUTPUT, convert_inspect_ui
from bacup_lib.legendary_perks_ui import TRANSLATIONS
from bacup_lib.translations import merge_packaged, read_table

REPO = Path(__file__).resolve().parents[2]
PACKAGE = REPO / "tmp/inspect-card-runtime/B21_InspectCardTest"
GENERATED_TABLE = REPO / "mods/SeventySix/data" / TRANSLATIONS
TREE_DLL = REPO / "mods/B21_TalesFromAppalachia/F4SE/Plugins/B21_TalesFromAppalachia.dll"


def main() -> None:
    if PACKAGE.exists():
        shutil.rmtree(PACKAGE)
    data = PACKAGE / "data"
    table = data / TRANSLATIONS
    table.parent.mkdir(parents=True)
    # Start from the full generated table: the game loads one table per DLL, so a partial copy would drop keys.
    shutil.copy2(GENERATED_TABLE, table)
    before = len(read_table(table))
    conversion = convert_inspect_ui(REPO / "extracted/fo76", data)
    merge_packaged(table)
    after = len(read_table(table))
    if after < before:
        raise SystemExit(f"translation table shrank from {before} to {after} lines")
    plugins = PACKAGE / "F4SE/Plugins"
    plugins.mkdir(parents=True)
    shutil.copy2(TREE_DLL, plugins / TREE_DLL.name)
    files = [{"path": path.relative_to(PACKAGE).as_posix(), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
             for path in sorted(PACKAGE.rglob("*")) if path.is_file()]
    (PACKAGE.parent / "files.json").write_text(json.dumps(files, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"package": str(PACKAGE), "card": (data / OUTPUT).as_posix(), "translations": [before, after],
                      "conversion": conversion}, indent=2))


if __name__ == "__main__":
    main()
