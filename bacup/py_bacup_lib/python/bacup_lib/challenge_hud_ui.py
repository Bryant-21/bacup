from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline
from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags
from bacup_lib.translations import merge_ui_translations, read_table

OUTPUT = Path("Interface/B21/TalesFromAppalachia/ChallengeHUD")
RESOURCES = Path(__file__).with_name("resources") / "challenge_hud"
MOVIES = {
    "challengetracker.swf": "ChallengeTracker",
    "challengeupdateflyout.swf": "ChallengeUpdateFlyout",
    "challengeflyout.swf": "ChallengeCompleteFlyout",
}
# FO76's HUD builds these flyouts from library symbols, so their movie roots have no class and Tales
# could never find the bridge. Each gets a root class that creates the flyout and forwards its bridge.
ROOTS = {
    "challengeupdateflyout.swf": ("B21_ChallengeUpdate", "B21SetUpdate"),
    "challengeflyout.swf": ("B21_ChallengeComplete", "B21SetCompletion"),
}
# Reward icons the completion flyout shows for Tales' caps and XP (FO76 loads them from its icon library).
ICONS = {"IconCR_Caps": "B21_ChallengeIconCaps", "IconCR_Experience": "B21_ChallengeIconExperience"}
# The completion banner's title is static text holding this key.
TITLE = "$ChallengeComplete_FlyoutTitle"


def root_class(name: str, flyout: str, bridge: str) -> str:
    return ("package { import flash.display.DisplayObject; import flash.display.MovieClip;"
            " import flash.utils.getDefinitionByName;"
            f" public class {name} extends MovieClip {{ private var flyout:Object;"
            f" public function {name}() {{ var type:Class = getDefinitionByName(\"{flyout}\") as Class;"
            " flyout = new type(); addChild(DisplayObject(flyout)); }"
            f" public function {bridge}(data:Object):uint {{ return uint(flyout.{bridge}(data)); }} }} }}")


def symbol_classes(data: bytes) -> list[tuple[int, bytes]]:
    entries = []
    for code, payload in swf_tags(data):
        if code != 76:
            continue
        count, = struct.unpack_from("<H", payload)
        offset = 2
        for _ in range(count):
            character, = struct.unpack_from("<H", payload, offset)
            end = payload.index(b"\0", offset + 2)
            entries.append((character, payload[offset + 2:end]))
            offset = end + 1
    return entries


def bind_root(data: bytes, class_name: str, script: bytes) -> bytes:
    tags = swf_tags(data)
    index = next(i for i, (code, _) in enumerate(tags) if code == 76)
    entries = symbol_classes(data) + [(0, class_name.encode("utf-8"))]
    tags.insert(index, (82, script))
    tags[index + 1] = (76, struct.pack("<H", len(entries)) + b"".join(
        struct.pack("<H", character) + name + b"\0" for character, name in entries))
    return rebuild(data, tags)


def convert_challenge_hud_ui(source_root: Path, output_data: Path) -> dict:
    from creation_lib.swf import native_runtime

    interface = Path(source_root) / "interface"
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    closure: dict[str, bytes] = {}
    pending = list(MOVIES)
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported challenge HUD dependency: {name}")
        source = files.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing challenge HUD source: {interface / name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    converted: dict[str, bytes] = {}
    for name, class_name in MOVIES.items():
        bridge = (RESOURCES / f"{class_name}.as").read_text(encoding="utf-8")
        source = closure[name]
        movie = native_runtime.replace_as3_classes(empty_hud_timeline(source), {class_name: bridge}, closure.values())
        expected = symbol_classes(source)
        if name in ROOTS:
            root, method = ROOTS[name]
            movie = bind_root(movie, root, native_runtime.compile_as3_do_abc([root_class(root, class_name, method)]))
            expected.append((0, root.encode("utf-8")))
        if symbol_classes(movie) != expected:
            raise ValueError(f"Challenge HUD class bindings changed unexpectedly: {name}")
        kept = lambda data: [tag for tag in swf_tags(data) if tag[0] not in (72, 76, 82)]
        if kept(movie) != kept(empty_hud_timeline(source)):
            raise ValueError(f"Challenge HUD conversion changed non-ActionScript tags: {name}")
        if name == "challengeflyout.swf":
            movie = native_runtime.inject_symbols_renamed(closure["challengerewardiconlibrary.swf"], movie, list(ICONS.items()))
            tags = swf_tags(movie)
            tags.insert(next(i for i, (code, _) in enumerate(tags) if code == 76),
                        (82, native_runtime.build_movieclip_class_doabc(list(ICONS.values()))))
            movie = rebuild(movie, tags)
            if native_runtime.unbacked_symbol_classes(movie) or not {n.encode() for n in ICONS.values()} <= {
                    n for _, n in symbol_classes(movie)}:
                raise ValueError("Challenge reward icons were not bound")
        converted[name] = movie

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "source_movies": sorted(MOVIES), "files": [], "bridges": {}}
    for name, class_name in MOVIES.items():
        manifest["bridges"][class_name] = hashlib.sha256((RESOURCES / f"{class_name}.as").read_bytes()).hexdigest()
    for name, source in sorted(closure.items()):
        movie = remap_menu_fonts(converted[name] if name in converted else source)
        target = output / name
        target.write_bytes(movie)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(), "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(movie).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    title = [line for line in read_table(interface / "translate_en.txt") if line.split("\t", 1)[0] == TITLE]
    if not title:
        raise ValueError(f"Source challenge translations are missing {TITLE}")
    merge_ui_translations(output_data, title)
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Convert FO76 challenge HUD movies for Tales")
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_challenge_hud_ui(args.source_root, args.output_data), indent=2))
