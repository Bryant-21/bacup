from functools import lru_cache
import json
from pathlib import Path


ICONS = Path(__file__).with_name("resources") / "shared/inventory_icons"


def catalog() -> list[dict]:
    return json.loads((ICONS / "catalog.json").read_text())["icons"]


def glyph_class(name: str) -> str:
    return "B21Glyph" + {"powerarmor": "PowerArmor"}.get(name, name[:1].upper() + name[1:])


@lru_cache(maxsize=1)
def icon_library() -> bytes:
    from creation_lib.swf.project import build_document
    from creation_lib.swf.writer import write_swf

    icons = catalog()
    project = {
        "canvas": [32, 32], "version": 14, "fps": 30, "background": "#000000",
        "shapes": [{"id": 1 + index, "svg": f"{icon['name']}.svg"} for index, icon in enumerate(icons)],
        "sprites": [{"id": 1001 + index, "frames": [{"place": [{"depth": 1, "character": 1 + index}]}]}
                    for index in range(len(icons))],
        "exports": [{"character": 1001 + index, "class": glyph_class(icon["name"])} for index, icon in enumerate(icons)],
    }
    return write_swf(build_document(project, ICONS))


def inject_inventory_icons(movie: bytes) -> bytes:
    from creation_lib.swf.native_runtime import inject_symbols
    from bacup_lib.inspect_ui import rebuild
    from bacup_lib.legendary_perks_ui import swf_tags
    from bacup_lib.workshop_ui import uncompressed

    names = [glyph_class(icon["name"]) for icon in catalog()]
    library = icon_library()
    injected = inject_symbols(library, movie, names)
    tags = list(swf_tags(uncompressed(injected)))
    at = next(index for index, (code, _) in enumerate(tags) if code in (76, 82))
    tags[at:at] = [(code, payload) for code, payload in swf_tags(uncompressed(library)) if code == 82]
    return rebuild(uncompressed(injected), tags)
