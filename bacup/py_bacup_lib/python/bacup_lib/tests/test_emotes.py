import hashlib
import json
import struct
from collections import Counter
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import emotes
from bacup_lib.workflows import unified
from bacup_lib.translations import read_table, TRANSLATIONS
from creation_lib.havok.native_runtime import load_native_module
from creation_lib.swf import native_runtime as swf


ROOT = Path(__file__).resolve().parents[5]
SOURCE = ROOT / "extracted/fo76"
TARGET = ROOT / "extracted/fo4"
PLUGIN = Path("N:/Steam Games/steamapps/common/Fallout76/Data/SeventySix.esm")
CATEGORY_ORDER = ["005741", "005740", "004154", "006862", "00682A", "002BE6",
                  "0040A9", "002BCB", "002BE5", "006860", "0040AA", "0040A8"]


@pytest.fixture(scope="module")
def graphs():
    human, armor = TARGET / emotes.HUMAN_GRAPH, TARGET / emotes.POWER_ARMOR_GRAPH
    if not human.is_file() or not armor.is_file():
        pytest.skip("extracted Fallout 4 behavior graphs unavailable")
    havok = load_native_module()
    return havok.unpack_hkx_to_xml(str(human)), havok.unpack_hkx_to_xml(str(armor))


def test_event_routes_reach_dynamic_idle_for_human_and_power_armor(graphs):
    human, armor = graphs
    route = emotes.consuming_route(human, emotes.EVENT)
    assert route["state_name"] == "DynamicIdle_CullWeapons"
    with pytest.raises(ValueError, match="No graph event"):
        emotes.consuming_route(human, "invented_event")
    # The PA root does not consume the event itself; it swaps in the shared weapon subgraph.
    with pytest.raises(ValueError, match="no unconditional transition"):
        emotes.consuming_route(armor, emotes.EVENT)
    power_armor = emotes.power_armor_route(armor, route)
    assert power_armor["subgraph"] == emotes.HUMAN_GRAPH and power_armor["swap_generators"]
    with pytest.raises(ValueError, match="lacks"):
        emotes.power_armor_route(armor, dict(route, event="invented_event"))


@pytest.fixture(scope="module")
def converted(tmp_path_factory):
    if not PLUGIN.is_file() or not (SOURCE / "interface/radialmenu.swf").is_file():
        pytest.skip("FO76 source data unavailable")
    output = tmp_path_factory.mktemp("converted-emotes")
    manifest = emotes.convert_emotes(SOURCE, output, source_plugin=PLUGIN, target_root=TARGET)
    return output, manifest


def test_catalog_carries_every_emote_in_fo76_category_order(converted):
    output, manifest = converted
    assert manifest["schema_version"] == 2 and manifest["ui"]["bridge_version"] == 3
    assert [row["id"] for row in manifest["categories"]] == CATEGORY_ORDER
    assert all(row["icon"] and row["icon_source"] for row in manifest["categories"])
    entries = {row["editor_id"]: row for row in manifest["entries"]}
    assert {row["editor_id"] for row in manifest["excluded"]} == {"DEBUG_ATX_Emote_TestWave01", "TEMPLATE_Emote_Emote"}
    assert len(entries) + len(manifest["excluded"]) == 147
    kinds = Counter(row["kind"] for row in manifest["entries"])
    assert kinds["human"] > 100 and kinds["pet"] >= 14 and kinds["icon"] >= 2
    for name in ("Emote_ImSelling1", "Emote_PlayerFear", "ATX_Emote_E3_GoldPanning1"):
        assert entries[name]["category_id"] == emotes.MISC_CATEGORY and entries[name]["kind"] != "pet"
    assert entries["Emote_ImSelling1"]["kind"] == "icon" and entries["Emote_ImSelling1"]["duration"] == 4.0
    assert all(row["kind"] == "pet" for name, row in entries.items() if "Pets_" in name or "PETS_" in name)
    assert entries["ATX_Emote_LuckyDice"]["dice_faces"] == 6 and entries["Emote_Wave1"]["dice_faces"] == 0
    assert all(row["animation"] or row["power_armor_only"] for row in manifest["entries"] if row["kind"] == "human")
    assert entries["ATX_Emote_WhataDrag"]["duration"] == 69.0 and entries["ATX_Emote_MothmanWorship1"]["animation"]
    wave = entries["Emote_Wave1"]
    assert (wave["source_id"], wave["idle_id"], wave["category_id"]) == ("11016C", "3AC5F1", "005741")
    assert wave["animation"] == "Actors\\Character\\Animations\\B21\\Emotes\\Wave.hkx"
    assert wave["animation_female"] == "Actors\\Character\\Animations\\B21\\Emotes\\Female\\Wave.hkx"
    assert wave["animation_power_armor"] == "Actors\\PowerArmor\\Animations\\B21\\Emotes\\Wave.hkx"
    assert manifest["playback_route"]["power_armor"]["event"] == emotes.EVENT
    assert manifest["power_armor_dropped_tracks"] == ["AimSource"]
    ranks = [(CATEGORY_ORDER.index(row["category_id"]), row["source_id"]) for row in manifest["entries"] if row["kind"] != "pet"]
    assert ranks == sorted(ranks)
    translations = dict(line.split("\t", 1) for line in read_table(output / TRANSLATIONS))
    assert translations["$B21_TFA_EmoteTab"] == "EMOTES" and translations["$B21_TFA_EmoteScroll"] == "SCROLL"
    assert json.loads((output / emotes.CATALOG).read_text()) == manifest


def test_every_catalog_clip_is_written_and_binds_its_fo4_skeleton(converted):
    output, manifest = converted
    havok = load_native_module()
    files = {row["path"]: row for row in manifest["files"]}
    for row in manifest["files"]:
        assert hashlib.sha256((output / "data" / row["path"]).read_bytes()).hexdigest() == row["sha256"]
        assert row["source_sha256"]
    clips = {path for row in manifest["entries"] for path in
             (row["animation"], row["animation_female"], row["animation_power_armor"]) if path}
    assert all("Meshes/" + path.replace("\\", "/") in files for path in clips)
    for path, tracks in (("Actors\\Character\\Animations\\B21\\Emotes\\Salute.hkx", 95),
                         ("Actors\\PowerArmor\\Animations\\B21\\Emotes\\Salute.hkx", 110)):
        xml = havok.hkx_to_xml((output / "data/Meshes" / path.replace("\\", "/")).read_bytes())
        assert emotes.animation_metadata(xml, tracks)["tracks"] == tracks
    assert all(row["reason"] for row in manifest["skipped"])


def test_movie_ships_ring_disk_badge_bitmaps_icons_and_bridge(converted):
    output, manifest = converted
    movie = (output / "data" / emotes.OUTPUT / "menu.swf").read_bytes()
    symbols = {name for _, name in swf.list_symbols(movie)}
    assert swf.roundtrip_ok(movie)
    assert swf.unbacked_symbol_classes(movie) == []
    icons = {row["icon"] for row in manifest["categories"] + manifest["entries"]}
    assert {f"B21Emotes_{icon}" for icon in icons} <= symbols
    assert {"B21Emotes_RadialMenuRingInner", "B21Emotes_RadialMenuRingOuter", "B21Emotes_B21EmoteMenu",
            "B21Emotes." + manifest["ui"]["corner_tab"]} <= symbols
    assert "LuckyDice1" in manifest["ui"]["text_stripped_icons"]
    assert sum(code == 36 for code, _ in emotes.swf_tags(movie)) == 6, "sector backer bitmaps"
    root = swf.abc_class_outline(movie, "B21Emotes_B21EmoteMenu")
    assert {"SetData", "Navigate", "PointAt", "Present", "BGSCodeObj", "EmoteVersion", "SetPlatform",
            "SetInputMode", "ControllerMode"} <= {trait["name"] for trait in root["instance_traits"]}
    strings = {value for item in swf.abc_string_pools(movie) for value in item[-1]}
    assert {"EmoteAction", "$B21_TFA_EmoteTab", "B21Emotes_"} <= strings
    assert "B21/TalesFromAppalachia/Emotes/fonts_en.swf" in movie.decode("latin1")
    source = (SOURCE / "interface/radialmenu.swf").read_bytes()
    def sprite(data, name):
        cid = next(cid for cid, symbol in swf.list_symbols(data) if symbol == name)
        return next(p for c, p in emotes.swf_tags(data) if c == 39 and struct.unpack_from("<H", p)[0] == cid)
    shapes = {struct.unpack_from("<H", p)[0] for c, p in emotes.swf_tags(source) if c in emotes.SHAPE_CODES}
    assert sprite(movie, "B21Emotes_RadialMenu") == emotes.radial_layout(sprite(source, "RadialMenu"), shapes)


def test_layout_keeps_named_clips_and_the_single_disk_shape():
    def sprite(rows):
        tags = []
        for depth, (name, character) in enumerate(rows, 1):
            flags = 2 | (0x20 if name else 0)
            tags.append((26, bytes((flags,)) + struct.pack("<HH", depth, character) + (name.encode() + b"\0" if name else b"")))
        tags += [(1, b""), (0, b"")]
        return struct.pack("<HH", 1, 1) + b"".join(struct.pack("<HI", c << 6 | 63, len(p)) + p for c, p in tags)
    named = [(name, 10 + i) for i, name in enumerate(emotes.LAYOUT_INSTANCES)]
    layout = emotes.radial_layout(sprite([(None, 5), *named, ("UnrelatedInventory_mc", 40), (None, 41)]), {5})
    assert b"UnrelatedInventory_mc" not in layout and struct.pack("<HH", 1, 5) in layout
    assert struct.pack("<HH", 8, 41) not in layout
    with pytest.raises(ValueError, match="layout changed"):
        emotes.radial_layout(sprite(named[:-1]))
    with pytest.raises(ValueError, match="Duplicate"):
        emotes.radial_layout(sprite([*named, named[0]]))
    with pytest.raises(ValueError, match="ring disk"):
        emotes.radial_layout(sprite(named), {5})


def test_artwork_adapter_keeps_stop_frames_and_holds_dice_roll(monkeypatch):
    stop = ["0 GetLocal { index: 0 }", "1 PushScope", "2 FindPropStrict stop", "5 CallPropVoid stop (0)", "9 ReturnVoid"]
    hide = ["0 GetLocal { index: 0 }", "1 PushScope", "2 FindPropStrict dispatchEvent", "4 FindPropStrict flash.events.Event",
            '6 PushString "AnimCompleteForceHide"', "9 PushTrue", "10 PushTrue", "11 ConstructProp flash.events.Event (3)",
            "14 CallPropVoid dispatchEvent (1)", "17 ReturnVoid"]
    monkeypatch.setattr(swf, "abc_disassemble", lambda *_: [
        {"method": "constructor", "code": ["0 PushByte { value: 2 }", "2 GetLocal { index: 0 }", "3 GetProperty frame9",
                                           "5 PushShort { value: 204 }", "8 GetLocal { index: 0 }", "9 GetProperty frame205"]},
        {"method": "frame9", "code": stop}, {"method": "frame205", "code": hide}])
    assert "addFrameScript(2, stop, 204, stop)" in emotes.artwork_class(b"fixture", "Fixture")
    monkeypatch.setattr(swf, "abc_disassemble", lambda *_: [
        {"method": "constructor", "code": []}, {"method": "frame3", "code": ["0 GetLocal { index: 0 }", "1 Nop"]}])
    with pytest.raises(ValueError, match="nontrivial"):
        emotes.artwork_class(b"fixture", "Fixture")


def test_missing_source_asset_does_not_write_partial_catalog(tmp_path, graphs):
    with pytest.raises((FileNotFoundError, OSError)):
        emotes.convert_emotes(tmp_path / "missing", tmp_path / "out", source_plugin=PLUGIN, target_root=TARGET)
    assert not (tmp_path / "out").exists()


def test_workflow_registers_only_fo76_to_fo4(monkeypatch, tmp_path):
    (tmp_path / "source/interface").mkdir(parents=True)
    (tmp_path / "source/interface/radialmenu.swf").write_bytes(b"fixture")
    calls = []
    monkeypatch.setattr(emotes, "convert_emotes", lambda *args, **kwargs: calls.append((args, kwargs)) or {
        "entries": [{}, {}], "files": [1, 2]})
    ctx = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "mod",
                          source_plugin_path=tmp_path / "SeventySix.esm", target_extracted_dir=tmp_path / "fo4")
    request = SimpleNamespace(source_game="fo76", target_game="fo4", target_extracted_dir=None)
    assert unified._convert_fo76_emotes(request, ctx) == 3
    assert calls[0][0] == (ctx.source_data_dir, ctx.mod_path)
    assert calls[0][1]["target_root"] == ctx.target_extracted_dir
    request.source_game = "skyrimse"
    assert unified._convert_fo76_emotes(request, ctx) == 0
