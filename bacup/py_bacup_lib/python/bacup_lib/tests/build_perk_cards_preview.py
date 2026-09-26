from pathlib import Path
import argparse
import json
import subprocess

from bacup_lib.perk_cards_ui import BRIDGE, RANK_BRIDGE, FILTER_BRIDGE


def build(output: Path) -> Path:
    output.mkdir(parents=True, exist_ok=True)
    fixtures = Path(__file__).with_name("fixtures") / "perk_cards_bridge"
    project = output / "bridge.swfproj"
    project.write_text(json.dumps({"canvas": [1000, 400], "fps": 30, "version": 14, "background": "#182029",
        "scripts": [str(BRIDGE), str(FILTER_BRIDGE), *[str(p) for p in sorted(fixtures.glob("*.as"))]],
        "exports": [{"character": 0, "class": "PerkCardsBridgePreview"}]}), encoding="utf-8")
    movie = output / "bridge.swf"
    subprocess.run(["modkit.exe", "swf", "pack", str(project), "-o", str(movie)], check=True)
    from creation_lib.swf.native_runtime import augment_as3_classes
    movie.write_bytes(augment_as3_classes(movie.read_bytes(),
        {"PerkCardRankConfirmation": RANK_BRIDGE.read_text(encoding="utf-8")}, []))
    return movie


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    print(build(parser.parse_args().output.resolve()))
