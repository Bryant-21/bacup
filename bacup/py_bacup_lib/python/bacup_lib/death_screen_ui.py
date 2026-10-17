"""Convert FO76's death respawn screen for the Tales death camera."""

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import replace_classes
from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags
from bacup_lib.status_hud_source import inject_art
from bacup_lib.translations import merge_ui_translations, translation_path

MENU = "deathrespawn.swf"
HINT_LIBRARY = "bsbuttonhintbar.swf"
HINT_BAR = "Shared.AS3.BSButtonHintBar"
HINT = "Shared.AS3.BSButtonHint"
DOCUMENT_CLASS = "DeathRespawnMenu"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/DeathScreen")
BRIDGE = Path(__file__).with_name("resources") / "death_screen" / "DeathRespawnMenu.as"
TEXT_KEYS = ("$RESPAWN",)

__all__ = ["MENU", "OUTPUT", "convert_death_screen_ui", "pick_translations", "translation_path"]


def pick_translations(interface: Path, keys: tuple[str, ...], description: str) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    rows = {}
    for line in data.decode(encoding).splitlines():
        key, separator, value = line.partition("\t")
        if separator and key in keys:
            rows[key] = value
    missing = [key for key in keys if key not in rows]
    if missing:
        raise ValueError(f"{description} translations are missing: {', '.join(missing)}")
    return [f"{key}\t{rows[key]}" for key in keys]


def bind_hint_bars(movie: bytes, library: bytes) -> bytes:
    """FO4's Scaleform ignores FO76's class-only placements, so ButtonHintBar_mc stayed null and the
    RESPAWN hint never drew. Copy the hint bar art in from its library and place it by character."""
    from creation_lib.swf import native_runtime

    exported = {name for _, name in native_runtime.list_symbols(library)}
    if HINT_BAR not in exported:
        return movie
    classes = set(native_runtime.abc_class_names(movie))
    movie = inject_art(library, movie, [(name, name) for name in (HINT_BAR, HINT) if name in exported & classes])
    character = {name: cid for cid, name in native_runtime.list_symbols(movie)}[HINT_BAR]

    def placement(body: bytes) -> bytes:
        if not body[1] & 8 or body[0] & 2:
            return body
        end = body.index(b"\0", 4)
        if body[4:end].decode() != HINT_BAR:
            return body
        return bytes([body[0] | 2, body[1] & ~8]) + body[2:4] + struct.pack("<H", character) + body[end + 1:]

    tags = []
    for code, payload in swf_tags(movie):
        if code == 70:
            payload = placement(payload)
        elif code == 39:
            offset, nested = 4, []
            while offset < len(payload):
                header, = struct.unpack_from("<H", payload, offset)
                offset += 2
                child_code, size = header >> 6, header & 63
                if size == 63:
                    size, = struct.unpack_from("<I", payload, offset)
                    offset += 4
                body = payload[offset:offset + size]
                offset += size
                if child_code == 70:
                    body = placement(body)
                nested.append(struct.pack("<HI", child_code << 6 | 63, len(body)) + body)
            payload = payload[:4] + b"".join(nested)
        tags.append((code, payload))
    return rebuild(movie, tags)


def convert_death_screen_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    translations = pick_translations(interface, TEXT_KEYS, "Death screen")
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    pending = [MENU]
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported death screen UI import: {name}")
        source = files.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing death screen UI import: {interface / name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    converted = replace_classes(closure[MENU], {DOCUMENT_CLASS: BRIDGE}, closure.values())
    if HINT_LIBRARY in closure:
        converted = bind_hint_bars(converted, closure[HINT_LIBRARY])
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema_version": 1,
        "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
        "menu": (OUTPUT / MENU).as_posix(),
        "code_object": "root1.DeathRespawnMenu",
        "files": [],
    }
    for name, original in sorted(closure.items()):
        artifact = remap_menu_fonts(converted if name == MENU else original)
        (output / name).write_bytes(artifact)
        manifest["files"].append({
            "path": (OUTPUT / name).as_posix(),
            "source_sha256": hashlib.sha256(original).hexdigest(),
            "sha256": hashlib.sha256(artifact).hexdigest(),
        })
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest
