"""Generate repo-local M2 fixture copies from existing valid FNV NIFs."""
from __future__ import annotations

import shutil
from pathlib import Path

HERE = Path(__file__).resolve().parent


def write_fixtures() -> tuple[Path, Path]:
    base_source = HERE / "gaussrifle.nif"
    sibling_source = HERE.parent / "grass" / "GrassWasteland04.NIF"
    base_path = HERE / "m2_min_base.nif"
    sibling_path = HERE / "m2_min_with_attachment.nif"
    shutil.copyfile(base_source, base_path)
    shutil.copyfile(sibling_source, sibling_path)
    return base_path, sibling_path


if __name__ == "__main__":
    for path in write_fixtures():
        print(path)
