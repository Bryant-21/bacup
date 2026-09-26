from __future__ import annotations

import hashlib
import json
import re
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path


MENU = "perksmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/PerkCards")
BRIDGE = Path(__file__).with_name("resources") / "perk_cards/Bridge.as"
RANK_BRIDGE = BRIDGE.with_name("RankBridge.as")
FILTER_BRIDGE = BRIDGE.with_name("FilteredCarousel.as")
PACK_PROMPT = BRIDGE.with_name("PackPrompt.as")


def pack_prompt_movie(source: bytes) -> bytes:
    from creation_lib.swf.native_runtime import augment_as3_classes, rename_as3_classes, patch_as3_method
    if any(name.lower() != "fonts_en.swf" for name in imports(source)):
        raise ValueError("Unexpected perk-pack prompt dependency")
    movie = augment_as3_classes(source, {"MessageBoxMenu": PACK_PROMPT.read_text(encoding="utf-8")})
    movie = patch_as3_method(movie, "MessageBoxMenu", "$constructor",
        [["getlex", "Shared.AS3.Styles.MessageBoxButtonListStyle"]],
        [["getlocal", 0], ["callproperty", "B21ListStyle", 0]])
    verify_script_only(source, movie)
    return rename_as3_classes(movie, "B21PerkPackPrompt", keep=("scaleform.gfx",))


def menu_translation_keys(movies: list[bytes]) -> set[str]:
    from creation_lib.swf.native_runtime import abc_string_pools
    keys = set()
    for movie in movies:
        for pool in abc_string_pools(movie):
            keys.update(value.casefold() for value in pool[-1] if value.startswith("$"))
        for code, payload in swf_tags(movie):
            if code == 37:
                keys.update(value.decode("ascii").casefold() for value in
                    re.findall(rb"\$[A-Za-z][A-Za-z0-9_]*(?: [A-Za-z][A-Za-z0-9_]*)*", payload))
    return keys


def embed_lettering_fonts(movie: bytes, fonts: bytes) -> bytes:
    aliases = {b"$KiddieCocktails": b"Kiddie Cocktails", b"$BRODY": b"Brody",
               b"$Futura_bold": b"Futura-CondensedBold"}
    tags = swf_tags(movie)
    definitions = {payload[5:5 + payload[4]].rstrip(b"\0"): payload
                   for code, payload in swf_tags(fonts) if code == 75}
    used_ids = {int.from_bytes(payload[:2], "little") for _, payload in tags if len(payload) >= 2}
    embedded = {}
    converted = []
    for code, payload in tags:
        if code == 37:
            offset = 2 + (5 + 4 * (payload[2] >> 3) + 7) // 8
            flags, = struct.unpack_from("<H", payload, offset)
            start = offset + 2 + (2 if flags & 1 else 0)
            end = payload.index(b"\0", start) if flags & 0x8000 else start
            alias = payload[start:end]
            if alias in aliases:
                if alias not in embedded:
                    font = definitions[aliases[alias]]
                    font_id = next(value for value in range(65535, 0, -1) if value not in used_ids)
                    used_ids.add(font_id)
                    embedded[alias] = font_id
                    converted.append((75, struct.pack("<H", font_id) + font[2:]))
                # FO4 has no mapping for these aliases. Bind the source glyphs by ID.
                payload = (payload[:offset] + struct.pack("<HH", (flags & ~0x8000) | 0x101,
                            embedded[alias]) + payload[end + 1:])
        converted.append((code, payload))
    if not embedded:
        return movie
    body = zlib.decompress(movie[8:]) if movie[:3] == b"CWS" else movie[8:]
    header_size = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    body = body[:header_size] + b"".join(struct.pack("<HI", code << 6 | 63, len(payload)) + payload
                                        for code, payload in converted)
    header = movie[:4] + struct.pack("<I", len(body) + 8)
    return header + (zlib.compress(body) if movie[:3] == b"CWS" else body)


def convert_perk_card_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    raw_text = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if raw_text.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    shared = {"$SELECT", "$EXIT", "$INSPECT", "$CONTINUE", "$CANCEL", "$ACCEPT", "$YES", "$NO", "$RANKS",
              "$RANK_UP", "$EQUIP", "$SHARE", "$SCRAP", "$ALLPERKS", "$CHANGEVIEW", "$TAG FOR SEARCH",
              "$CLEAR", "$CONFIRM", "$BOOST", "$BACK", "$UNEQUIP", "$Choose", "$NEW", "$GHOUL",
              "$TAGGED", "$OWNED", "$UNOWNED", "$My Perks", "$STRENGTH", "$PERCEPTION", "$ENDURANCE",
              "$CHARISMA", "$INTELLIGENCE", "$AGILITY", "$LUCK"}
    shared = {key.casefold() for key in shared}
    translations = [line for line in raw_text.decode(encoding).splitlines()
                    if line.casefold().startswith(("$perks_", "$perkcard", "$perkjoke", "$special", "$legendaryperk", "$filter", "$type_", "$pickaperk"))
                    or line.split("\t", 1)[0].casefold() in shared]
    if not any(line.startswith("$Perks_Rank\t") for line in translations):
        raise ValueError("Source regular perk translations are missing")
    sources = {path.name.lower(): path for path in interface.iterdir() if path.is_file()}
    pending = [MENU, "perkcardpacks.swf", "messageboxmenu.swf"]
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported regular perk UI import: {name}")
        source = sources.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing regular perk UI dependency: {name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    from creation_lib.swf.native_runtime import augment_as3_classes, patch_as3_method
    movie = augment_as3_classes(closure[MENU], {"PerksMenu": BRIDGE.read_text(encoding="utf-8"),
        "PerkCardRankConfirmation": RANK_BRIDGE.read_text(encoding="utf-8"),
        "FilteredCarousel": FILTER_BRIDGE.read_text(encoding="utf-8")}, list(closure.values()))
    movie = patch_as3_method(movie, "PerksMenu", "$constructor",
        [["getlocal", 0], ["callpropvoid", "addEvents", 0]],
        [["getlocal", 0], ["callpropvoid", "B21Initialize", 0], ["keep", 0], ["keep", 1]])
    movie = patch_as3_method(movie, "PerksMenu", "onRespecPicked",
        [["callpropvoid", "dispatchEvent", 1]],
        [["keep", 0], ["getlocal", 0], ["callpropvoid", "B21FreeRespec", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "SetButtons", [["getproperty", "_AllowEditsToCardsAndDecks"]],
        [["callproperty", "B21SetButtons", 0]])
    for method in ("HandleShowingInitialElements", "SetStageFocus", "onPerkPickAnimFinished", "onCancelPress", "onLevelUpButtonPressed"):
        movie = patch_as3_method(movie, "PerksMenu", method, [["getproperty", "shouldShowPickSpecial"]],
            [["callproperty", "B21PickPerkOnly", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "$constructor", [["getproperty", "onBoostPress"]],
        [["callproperty", "B21SPECIALHandler", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "$constructor", [["getproperty", "onLevelUpButtonPressed"]],
        [["callproperty", "B21PerksHandler", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "ProcessUserEvent", [["callpropvoid", "onLevelUpButtonPressed", 0]],
        [["callpropvoid", "B21OpenPerks", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "ProcessUserEvent", [["callpropvoid", "onBoostPress", 0]],
        [["callpropvoid", "B21OpenSPECIAL", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "onSpecialPicked", [["getproperty", "_CharacterInfoData"]],
        [["callproperty", "B21AllocationValues", 0]])
    movie = patch_as3_method(movie, "PerksMenu", "onSpecialPicked", [["getproperty", "_NumLevelUpPoints"]],
        [["callproperty", "B21PicksAfterSpecial", 0]])
    movie = patch_as3_method(movie, "FilteredCarousel", "GetFilterText", [["getlocal", 1]],
        [["getlocal", 0], ["keep", 0], ["callproperty", "B21FilterText", 1]])
    for method in ("onPerkPickAnimFinished", "CanCardBeRankedUp", "onRankUpPress", "ShowRankUpForAquiringCard"):
        movie = patch_as3_method(movie, "PerksMenu", method,
            [["callproperty", "GetCardsAvailableForRankUp", 1]], [["callproperty", "B21RankCandidates", 1]])
    for method in ("onRankUpPress", "ShowRankUpForAquiringCard"):
        movie = patch_as3_method(movie, "PerksMenu", method,
            [["callpropvoid", "SetData", 2]], [["callpropvoid", "B21SetData", 2]])
    verify_script_only(closure[MENU], movie)

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "rank_bridge_sha256": hashlib.sha256(RANK_BRIDGE.read_bytes()).hexdigest(),
                "filter_bridge_sha256": hashlib.sha256(FILTER_BRIDGE.read_bytes()).hexdigest(),
                "menu": (OUTPUT / MENU).as_posix(), "files": []}
    for name, source in sorted(closure.items()):
        converted = pack_prompt_movie(source) if name == "messageboxmenu.swf" else movie if name == MENU else source
        converted = remap_menu_fonts(converted)
        if name == "messageboxmenu.swf":
            name = "packprompt.swf"
        if "fonts_en.swf" in closure:
            converted = embed_lettering_fonts(converted, closure["fonts_en.swf"])
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
            "source_sha256": hashlib.sha256(source).hexdigest(), "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    keys = menu_translation_keys(list(closure.values()))
    translations.extend(line for line in raw_text.decode(encoding).splitlines()
        if line.split("\t", 1)[0].casefold() in keys)
    merge_ui_translations(output_data, translations)
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_perk_card_ui(args.source_root, args.output_data), indent=2))
