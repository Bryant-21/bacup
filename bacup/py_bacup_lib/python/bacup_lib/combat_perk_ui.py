from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import symbol_ids

OUTPUT = Path("Interface/B21/TalesFromAppalachia/CombatPerks")
REQUIRED_EXPORTS = ("HUDActiveEffectsWidget", "HUDActiveEffectClip")


def convert_combat_perk_ui(source_root: Path, output_data: Path) -> dict:
    source_path = Path(source_root) / "interface/hudmenu.swf"
    if not source_path.is_file():
        raise FileNotFoundError(f"Missing combat perk HUD source: {source_path}")
    source = source_path.read_bytes()
    missing = sorted(set(REQUIRED_EXPORTS) - symbol_ids(swf_tags(source)).keys())
    if missing:
        raise ValueError(f"FO76 HUD omits required compiled combat perk clips: {', '.join(missing)}")
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    (output / "combatperkshud.swf").unlink(missing_ok=True)
    manifest = {"schema_version": 2, "source_sha256": hashlib.sha256(source).hexdigest(),
                "source_exports": list(REQUIRED_EXPORTS), "presentation_owner": "StatusHUD",
                "movie": "Interface/B21/TalesFromAppalachia/StatusHUD/statushud.swf"}
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Validate the FO76 effect clips used by the shared Tales HUD")
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_combat_perk_ui(args.source_root, args.output_data), indent=2))
