from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path

from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path
from creation_lib.swf.parser import parse_swf

SOURCE = "hudmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Experience")
SOUNDS = (
    Path("sound/fx/ui/ui_leveluptext.xwm"),
    Path("sound/fx/ui/ui_levelup_perkicon_01.xwm"),
    Path("sound/fx/ui/ui_levelup_perkicon_02.xwm"),
    Path("sound/fx/ui/ui_levelup_perkicon_03.xwm"),
    Path("sound/fx/ui/ui_leveluptext_vaultboy.xwm"),
)
TRANSLATION_KEYS = {"$LEVEL UP", "$LEVELUP", "$YOU'VE_REACHED_LEVEL"}
SYMBOLS = {"LevelUpClip", "XPMeterBar"}
SOURCE_LAYOUT = {
    "meter": {"x": 801.2, "y": 803.35},
    "level_up": {"x": 541.3, "y": 660.0},
    "level_up_on_frame": 20,
    "level_up_frames": 202,
    "meter_xp_frame": 1,
    "meter_level_up_frame": 8,
    "meter_frames": 14,
    "fps": 30.0,
}


@dataclass(frozen=True)
class XPSourceContract:
    meter_class: str
    level_up_class: str

    @property
    def symbols(self) -> list[str]:
        return sorted({*SYMBOLS, self.meter_class, self.level_up_class})


def source_translations(interface: Path) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    found: dict[str, str] = {}
    for line in data.decode(encoding).splitlines():
        key = line.split("\t", 1)[0]
        if key in TRANSLATION_KEYS:
            found[key] = line
    missing = sorted(TRANSLATION_KEYS - found.keys())
    if missing:
        raise ValueError(f"XP translations are missing: {', '.join(missing)}")
    return [found[key] for key in sorted(found)]


def _source_class(document, character_id: int, instance_name: str) -> str:
    names = sorted(
        name for character, name in document.symbols if character == character_id
    )
    if len(names) != 1:
        detail = "none" if not names else ", ".join(names)
        raise ValueError(
            f"XP source instance {instance_name} does not have one exported class: {detail}"
        )
    return names[0]


def verify_source_contract(movie: bytes) -> XPSourceContract:
    document = parse_swf(movie)
    exported_names = {name for _, name in document.symbols}
    missing = sorted(SYMBOLS - exported_names)
    if missing:
        raise ValueError(f"XP source symbols are missing: {', '.join(missing)}")
    root = document.main_timeline.frames[0].placements
    group = next(
        (value for value in root.values() if value.name == "HUDNotificationsGroup_mc"),
        None,
    )
    level = next(
        (value for value in root.values() if value.name == "LevelUpAnimation_mc"), None
    )
    missing_instances = [
        name
        for name, value in (
            ("HUDNotificationsGroup_mc", group),
            ("LevelUpAnimation_mc", level),
        )
        if value is None
    ]
    if missing_instances:
        raise ValueError(
            f"XP source instances are missing: {', '.join(missing_instances)}"
        )
    group_timeline = document.sprites[group.character_id].timeline
    meter = next(
        (
            value
            for value in group_timeline.frames[0].placements.values()
            if value.name == "XPMeter_mc"
        ),
        None,
    )
    if meter is None:
        raise ValueError("XP source instance is missing: XPMeter_mc")
    meter_timeline = document.sprites[meter.character_id].timeline
    level_timeline = document.sprites[level.character_id].timeline
    positions = {
        "meter": (
            (group.matrix.translate_x + meter.matrix.translate_x) / 20.0,
            (group.matrix.translate_y + meter.matrix.translate_y) / 20.0,
        ),
        "level_up": (level.matrix.translate_x / 20.0, level.matrix.translate_y / 20.0),
    }
    expected = {
        name: (value["x"], value["y"])
        for name, value in SOURCE_LAYOUT.items()
        if isinstance(value, dict)
    }
    if (
        positions != expected
        or document.header.fps != SOURCE_LAYOUT["fps"]
        or len(meter_timeline.frames) != SOURCE_LAYOUT["meter_frames"]
        or meter_timeline.frames[SOURCE_LAYOUT["meter_xp_frame"] - 1].label != "xp"
        or meter_timeline.frames[SOURCE_LAYOUT["meter_level_up_frame"] - 1].label
        != "levelup"
        or len(level_timeline.frames) != SOURCE_LAYOUT["level_up_frames"]
        or level_timeline.frames[SOURCE_LAYOUT["level_up_on_frame"] - 1].label != "On"
    ):
        raise ValueError(
            "FO76 XP source layout/timeline differs from the verified HUD contract"
        )
    return XPSourceContract(
        meter_class=_source_class(document, meter.character_id, meter.name),
        level_up_class=_source_class(document, level.character_id, level.name),
    )


def convert_xp_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    source = (interface / SOURCE).read_bytes()
    translations = source_translations(interface)
    contract = verify_source_contract(source)
    sounds = {path: (Path(source_root) / path).read_bytes() for path in SOUNDS}
    manifest = {
        "schema_version": 2,
        "source_movie": SOURCE,
        "source_symbols": contract.symbols,
        "source_layout": SOURCE_LAYOUT,
        "source_sound_descriptors": {
            "text": "057B44:SeventySix.esm",
            "perk_icon": "06C581:SeventySix.esm",
            "vault_boy": "23AE9D:SeventySix.esm",
        },
        "menu": "Interface/B21/TalesFromAppalachia/StatusHUD/statushud.swf",
        "presentation_owner": "StatusHUD",
        "files": [],
        "audio": [],
    }
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    previous = output / "conversion.json"
    if previous.is_file():
        for entry in json.loads(previous.read_text(encoding="utf-8")).get("files", []):
            candidate = (Path(output_data) / entry["path"]).resolve()
            if candidate.is_relative_to(output.resolve()) and candidate.suffix.lower() == ".swf":
                candidate.unlink(missing_ok=True)
    for path, data in sounds.items():
        target = Path(output_data) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        manifest["audio"].append({"path": path.as_posix(), "sha256": hashlib.sha256(data).hexdigest()})
    (output / "conversion.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    merge_ui_translations(output_data, translations)
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Convert the FO76 XP and level-up HUD for Tales.")
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-data", type=Path, required=True)
    args = parser.parse_args()
    result = convert_xp_ui(args.source_root, args.output_data)
    print(f"Wrote {len(result['files'])} files to {args.output_data / OUTPUT}")
