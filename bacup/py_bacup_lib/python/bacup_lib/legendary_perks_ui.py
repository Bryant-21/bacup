from __future__ import annotations

import hashlib
import json
import os
import shutil
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path


MENU = "legendaryperksmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/LegendaryPerks")
BRIDGE = Path(__file__).with_name("resources") / "legendary_perks/MainTimeline.as"


def swf_tags(data: bytes) -> list[tuple[int, bytes]]:
    if data[:3] == b"CWS":
        body = zlib.decompress(data[8:])
    elif data[:3] == b"FWS":
        body = data[8:]
    else:
        raise ValueError("Expected an FWS or CWS menu")
    if len(body) + 8 != struct.unpack_from("<I", data, 4)[0]:
        raise ValueError("SWF length mismatch")
    offset = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    tags = []
    while offset < len(body):
        header, = struct.unpack_from("<H", body, offset)
        offset += 2
        size = header & 63
        if size == 63:
            size, = struct.unpack_from("<I", body, offset)
            offset += 4
        if offset + size > len(body):
            raise ValueError("Truncated SWF tag")
        tags.append((header >> 6, body[offset:offset + size]))
        offset += size
        if header >> 6 == 0:
            if offset != len(body):
                raise ValueError("Trailing data after SWF End tag")
            return tags
    raise ValueError("Missing SWF End tag")


def imports(data: bytes) -> list[str]:
    return [body.split(b"\0", 1)[0].decode("utf-8")
            for code, body in swf_tags(data) if code in (57, 71)]


def find_ffdec_jar() -> Path:
    launcher = shutil.which("ffdec") or shutil.which("ffdec-cli")
    candidates = [Path(launcher).parent / "ffdec.jar"] if launcher else []
    for variable in ("ProgramFiles(x86)", "ProgramFiles"):
        if location := os.environ.get(variable):
            candidates.append(Path(location) / "FFDec/ffdec.jar")
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise FileNotFoundError("Legendary perk UI conversion requires JPEXS FFDec (ffdec.jar)")


def convert_legendary_perk_ui(
    source_root: Path, output_data: Path, *, ffdec_jar: Path | None = None,
) -> dict:
    source_interface = Path(source_root) / "interface"
    source_files = {p.name.lower(): p for p in source_interface.iterdir() if p.is_file()}
    pending = [MENU]
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported legendary UI import: {name}")
        source = source_files.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing legendary UI dependency: {source_interface / name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    with tempfile.TemporaryDirectory(prefix="b21-legendary-ui-") as directory:
        staging = Path(directory)
        source_menu = staging / MENU
        source_menu.write_bytes(closure[MENU])
        patched_menu = staging / "patched.swf"
        result = subprocess.run(
            ["java", "-jar", str(ffdec_jar or find_ffdec_jar()), "-replace",
             str(source_menu), str(patched_menu), "LegendaryPerksMenu_fla.MainTimeline",
             str(BRIDGE)], capture_output=True, text=True, encoding="utf-8", timeout=120,
        )
        if result.returncode or not patched_menu.is_file():
            raise RuntimeError(f"Legendary menu bridge compilation failed: {result.stdout}\n{result.stderr}")
        patched = patched_menu.read_bytes()
        original_art = [tag for tag in swf_tags(closure[MENU]) if tag[0] != 82]
        patched_art = [tag for tag in swf_tags(patched) if tag[0] != 82]
        if original_art != patched_art:
            raise ValueError("Legendary UI conversion changed non-ActionScript tags")

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "menu": (OUTPUT / MENU).as_posix(), "files": []}
    for name, source in sorted(closure.items()):
        converted = patched if name == MENU else source
        destination = output / name
        destination.write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
                                  "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest
