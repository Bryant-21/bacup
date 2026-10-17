from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only

MENU = "petprogressiontrackmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Pets")
BRIDGE = Path(__file__).with_name("resources") / "pets" / "PetProgressionTrackMenu.as"


def convert_pet_progression_ui(source_root: Path, output_data: Path) -> dict:
    interface = source_root / "interface"
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()} if interface.is_dir() else {}
    if MENU not in files:
        return {"available": False, "reason": "Source pet progression board is absent"}
    try:
        from creation_lib.swf.native_runtime import replace_as3_classes

        closure: dict[str, bytes] = {}
        pending = [MENU]
        while pending:
            name = pending.pop().casefold()
            if name in closure:
                continue
            if "/" in name or "\\" in name or not name.endswith(".swf"):
                raise ValueError(f"Unsupported pet progression import: {name}")
            if name not in files:
                raise FileNotFoundError(f"Missing pet progression import: {name}")
            closure[name] = files[name].read_bytes()
            pending.extend(imports(closure[name]))
        source = closure[MENU]
        converted = verify_script_only(source, replace_as3_classes(
            source, {"PetProgressionTrackMenu": BRIDGE.read_text(encoding="utf-8")}, list(closure.values())))
        artifacts = {name: remap_menu_fonts(converted if name == MENU else data) for name, data in closure.items()}
        output = output_data / OUTPUT
        output.mkdir(parents=True, exist_ok=True)
        manifest = {"available": True, "version": 2, "menu": (OUTPUT / MENU).as_posix(), "files": []}
        for name, artifact in artifacts.items():
            (output / name).write_bytes(artifact)
            manifest["files"].append({"path": (OUTPUT / name).as_posix(), "sha256": hashlib.sha256(artifact).hexdigest()})
        (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        return manifest
    except Exception as error:
        # This optional presentation must never abort the CAMP/world pet catalog.
        return {"available": False, "reason": f"Pet progression conversion failed: {error}"}


def parse_track(fields: list[dict]) -> list[dict]:
    nodes: list[dict] = []
    node = None
    ended = False
    for field in fields:
        for tag, value in field.items():
            if not isinstance(value, dict) or "raw_hex" not in value:
                continue
            raw = bytes.fromhex(value["raw_hex"])
            if tag == "PGTI":
                if len(raw) != 8:
                    raise ValueError("Invalid pet progression node UID")
                node = {"uid": str(int.from_bytes(raw, "little"))}
                nodes.append(node)
                ended = False
            elif node is not None and tag == "NAME" and len(raw) == 8:
                node.setdefault("requires", []).append(str(int.from_bytes(raw, "little")))
            elif node is not None and not ended:
                if tag == "ITME":
                    ended = True
                elif tag in ("NAM0", "NAM4", "NAM5", "VPRR"):
                    if len(raw) != 4:
                        raise ValueError(f"Invalid pet progression {tag}")
                    node[{"NAM0": "level", "NAM4": "image_id", "NAM5": "icon_id", "VPRR": "reward_id"}[tag]] = int.from_bytes(raw, "little")
                elif tag in ("NAM1", "NAM2", "NAM3") and raw.startswith(b"<ID="):
                    node[{"NAM1": "name", "NAM2": "title", "NAM3": "description"}[tag]] = raw.split(b">", 1)[1].rstrip(b"\0").decode("utf-8")
    if not nodes or len({n["uid"] for n in nodes}) != len(nodes) or any(not n.get("level") or not n.get("name") for n in nodes):
        raise ValueError("Incomplete pet progression track")
    return nodes
