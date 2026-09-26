from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import verify_script_only

MENU = Path("Interface/ExamineMenu.swf")
BRIDGE = Path(__file__).with_name("resources") / "inspect/WorkbenchMenu.as"


def build_workbench_ui(stock: bytes, output_data: Path) -> dict:
    from creation_lib.swf import native_runtime as native

    bridge = BRIDGE.read_text(encoding="utf-8")
    movie = native.augment_as3_classes(stock, {"ExamineMenu": bridge})
    verify_script_only(stock, movie)
    output = Path(output_data) / MENU
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(movie)
    return {"menu": MENU.as_posix(), "sha256": hashlib.sha256(movie).hexdigest(),
            "source_sha256": hashlib.sha256(stock).hexdigest(), "bridge_sha256": hashlib.sha256(bridge.encode()).hexdigest()}


def convert_workbench_ui(output_data: Path, *, target_data_dir: Path) -> dict:
    from creation_lib.ba2.native_runtime import extract_one

    archive = Path(target_data_dir) / "Fallout4 - Interface.ba2"
    stock = extract_one(str(archive), "interface/examinemenu.swf")
    if not stock:
        raise FileNotFoundError(f"Missing stock workbench menu in {archive}")
    return build_workbench_ui(stock, output_data)


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("target_movie", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(build_workbench_ui(args.target_movie.read_bytes(), args.output_data), indent=2))
