"""Catalog editor, features group B: annotated schemas still validate real output, and the guided editors.

Covers WeaponModelSwaps (candidates from a plugin, warnings, the guided form), CampWallpapers, Emotes and Reputation
warnings, and that the twelve annotated schemas carry the help the editor shows.
"""
from __future__ import annotations

import json
import struct
from pathlib import Path

import jsonschema
import pytest
from referencing import Registry, Resource

from creation_lib.catalog.plugin_catalog import SCHEMAS_DIR
from creation_lib.esp.editor import EditorSession
from creation_lib.esp.native_runtime import (
    plugin_handle_add_record_raw, plugin_handle_call, plugin_handle_close, plugin_handle_new,
)
from ui.tools.catalog_editor.features import camp_emotes_reputation as extras
from ui.tools.catalog_editor.features import extension_for, weapon_model_swaps as swaps
from ui.tools.catalog_editor.forms import PluginFormSource
from ui.tools.catalog_editor.model import ADDED, CatalogSession

FEATURES_B = ("WeaponModelSwaps", "CampWallpapers", "Emotes", "Pets", "Reputation", "Infestations", "DailyOps",
              "Challenges", "ContextualLoot", "QuestCategories", "LegendaryPerks", "PlayerMutations")
FORM = "SeventySix.esm:000800"


def load(feature: str) -> dict:
    return json.loads((SCHEMAS_DIR / f"{feature}.schema.json").read_text(encoding="utf-8"))


def validator(feature: str) -> jsonschema.Draft202012Validator:
    contents = [json.loads(p.read_text(encoding="utf-8")) for p in SCHEMAS_DIR.glob("*.schema.json")]
    registry = Registry().with_resources([(item["$id"], Resource.from_contents(item)) for item in contents])
    return jsonschema.Draft202012Validator(load(feature), registry=registry)


# Documents shaped like the BACUP emitters' output (the emitters' own tests validate the real thing).
SAMPLES = {
    "WeaponModelSwaps": {"version": 1, "settings": {"source_plugin": "SeventySix.esm"}, "skins": [
        {"form": "SeventySix.esm:6C50EB", "editor_id": "mod_skin", "models": [["weapons\\a.nif", "atx\\b.nif"]]}]},
    "CampWallpapers": {"version": 1, "settings": {}, "wallpapers": [
        {"form": FORM, "strip": True},
        {"form": "SeventySix.esm:000801", "material_swap": "SeventySix.esm:000802", "model_swap": "Set", "art": None}],
        "model_swaps": {"Set": [["a\\wall.nif", "a\\wall_paper.nif"]]}},
    "Emotes": {"version": 1, "settings": {"event_name": "B21EmoteV1", "playback_route": {"event": "x"},
                                          "ui": {"bridge_version": 3}},
               "categories": [{"id": "SeventySix.esm:000900", "name": "Social", "icon": "icon"}],
               "entries": [{"form": FORM, "name": "Wave", "category_id": "SeventySix.esm:000900", "kind": "human",
                            "icon": "icon", "idle_id": "SeventySix.esm:000801",
                            "animation": "Actors\\Character\\Animations\\B21\\Emotes\\Wave.hkx", "duration": 3.0,
                            "dice_faces": 0, "sound_ids": []}]},
    "Pets": {"version": 1, "settings": {"bindings": {"sandbox": FORM, "stimpak": None}, "interactions": {
        "0": {"pet": FORM, "speak": None, "sit": FORM}}, "xp_thresholds": [10, 20], "xp_per_second": 1.0},
        "pets": [{"actor": FORM, "home": "SeventySix.esm:000801", "species": "dog"}],
        "apparel": [{"form": "SeventySix.esm:000802", "species": 0, "slots": ["33"]}]},
    "Reputation": {"version": 1, "settings": {"output_plugin": "SeventySix.esm", "sounds": {"increase": {"form": FORM}}},
                   "factions": [{"code": "Crater", "name": "Crater", "actor_value": FORM, "tiers": [
                       {"index": i, "label": f"T{i}", "threshold": i * 10, "global": None} for i in range(7)]}]},
    "Infestations": {"version": 1, "settings": {"plugin": "SeventySix.esm", "tuning": {
        "expire_seconds": 18000, "marker_radius": 35000, "boss_kill_fraction": 0.8, "remaining_fraction": 0.5},
        "forms": {"quest": FORM, "announce_toggle": None}, "reward": {"xp_global": FORM, "boss_rank": 4}},
        "sites": [], "factions": [{"key": "Scorched", "mob": FORM, "boss": FORM}],
        "regions": [{"key": "Forest", "location": FORM, "toggle": FORM}], "mutations": [{"form": FORM}]},
    "DailyOps": {"version": 1, "settings": {"speaker": "Dodge"}, "refs": [{"form": FORM, "members": [FORM]}],
                 "entrances": [{"form": FORM, "bounds": [10, 10, 10]}],
                 "voice": {FORM: {"variants": [{"info": FORM, "clips": [
                     {"path": "Sound/FX/B21/TalesFromAppalachia/DailyOps/a.xwm"}]}]}}},
    "Challenges": {"version": 1, "settings": {}, "categories": [{"key": "kills", "label": "Kills"}],
                   "challenges": [{"id": FORM, "name": "Kill", "category": "kills", "frequency": "daily", "kind": "kill",
                                   "count": 5, "parent": None, "prerequisites": [], "sub_challenges": [],
                                   "reward": {"perkCardPacks": 1}, "conditions": [[{"fn": "keyword", "op": "==", "value": 1}]]}]},
    "ContextualLoot": {"version": 1, "settings": {}, "lists": [], "ammo_groups": [], "rewards": [], "tokens": [],
                       "counts": []},
    "QuestCategories": {"version": 1, "settings": {}, "quests": [{"form": FORM, "category": "SIDE"}]},
    "LegendaryPerks": {"version": 1, "settings": {}, "bindings": {"B21_Legendary_FunkyDuds_1": FORM,
                                                                 "B21_Legendary_FunkyDuds_1_Value": {"remove": True}}},
    "PlayerMutations": {"version": 1, "settings": {}, "foods": [{"form": FORM, "type": 1}],
                        "consumables": [{"form": FORM, "action": 3}], "decontamination": [{"form": FORM}]},
}


@pytest.mark.parametrize("feature", FEATURES_B)
def test_annotated_schema_still_validates_emitter_shaped_output(feature):
    validator(feature).validate(SAMPLES[feature])


def _named_nodes(node, path=""):
    """Every named property under a node (through items, prefixItems, additionalProperties and branches)."""
    if not isinstance(node, dict):
        return
    for name, child in (node.get("properties") or {}).items():
        yield f"{path}.{name}", name, child
        yield from _named_nodes(child, f"{path}.{name}")
    for key in ("items", "additionalProperties"):
        yield from _named_nodes(node.get(key), path)
    for key in ("oneOf", "anyOf", "allOf"):
        for branch in node.get(key) or []:
            yield from _named_nodes(branch, path)


@pytest.mark.parametrize("feature", FEATURES_B)
def test_feature_and_every_section_and_field_are_explained(feature):
    schema = load(feature)
    assert schema.get("description")
    sections = {n: b for n, b in schema["properties"].items() if n not in ("version", "settings")}
    for name, body in sections.items():
        assert body.get("title") or feature in ("Infestations",) or body.get("description"), f"{feature}.{name}"
        assert body.get("description"), f"{feature}.{name} description"
        for path, field, node in _named_nodes(body):
            if field == "remove":
                continue
            assert node.get("title"), f"{feature}.{name}{path} needs a title"
            assert node.get("description") or node.get("type") == "object" or "enum" in node or "const" in node \
                or node.get("title") in ("Clip hash", "Source hash"), f"{feature}.{name}{path} needs a description"


@pytest.mark.parametrize("feature", FEATURES_B)
def test_help_text_is_present_for_multi_step_features(feature):
    assert load(feature).get("x-help"), f"{feature} needs an x-help panel"


@pytest.mark.parametrize("feature", FEATURES_B)
def test_asset_annotations_are_complete(feature):
    def walk(node):
        if isinstance(node, dict):
            if "x-asset" in node:
                yield node["x-asset"]
            for value in node.values():
                yield from walk(value)
        elif isinstance(node, list):
            for value in node:
                yield from walk(value)

    for asset in walk(load(feature)):
        assert asset["kind"] in ("nif", "texture", "material", "swf", "sound", "any")
        assert asset["root"] in ("Meshes", "Textures", "Materials", "Interface", "Sound", "")
        assert isinstance(asset["strip_root"], bool)


def test_tuple_columns_match_the_tuple_width():
    models = load("WeaponModelSwaps")["properties"]["skins"]["items"]["properties"]["models"]
    assert models["x-columns"] == ["Replace this mesh", "With this mesh"]
    assert len(models["items"]["prefixItems"]) == 2
    assert load("CampWallpapers")["properties"]["model_swaps"]["x-key-title"]
    assert load("DailyOps")["properties"]["voice"]["x-key-form-type"] == ["DIAL"]


def test_enum_labels_cover_every_enum_value():
    def walk(node):
        if isinstance(node, dict):
            if "x-enum-labels" in node:
                yield node
            for value in node.values():
                yield from walk(value)
        elif isinstance(node, list):
            for value in node:
                yield from walk(value)

    for feature in FEATURES_B:
        for node in walk(load(feature)):
            values = node.get("enum")
            if values is not None:
                assert {str(v) for v in values} == set(node["x-enum-labels"]), feature


# --- WeaponModelSwaps --------------------------------------------------------------------------------------


def test_mesh_paths_compare_the_way_tales_does():
    assert swaps.normalize_mesh("Meshes/Weapons/Bat.NIF") == "weapons\\bat.nif"
    assert swaps.normalize_mesh("weapons\\bat.nif") == "weapons\\bat.nif"


def test_pair_problems_follow_the_reader():
    pairs = [["a.nif", "b.nif"], ["Meshes\\A.nif", "c.nif"], ["x.nif", "X.nif"], ["", "y.nif"], ["z.nif", ""]]
    messages = swaps.pair_issues(pairs)
    assert len(messages) == 4
    assert "earlier pair" in messages[0] and "itself" in messages[1] and "empty" in messages[2]
    assert swaps.skin_issues({"form": FORM, "models": []}) == ["no mesh swaps; Tales ignores a skin without any"]
    missing = swaps.skin_issues({"models": [["a.nif", "gone.nif"]]}, lambda path: path != "gone.nif")
    assert missing and "not found" in missing[0]


def plugin_file(directory: Path, name: str, masters=(), records=()) -> Path:
    handle = plugin_handle_new(name, "fo4")
    try:
        if name.casefold().endswith(".esm"):
            plugin_handle_call(handle, "set_header_flags", 1)
        for master in masters:
            plugin_handle_call(handle, "add_master", master)
        for signature, form_id, subrecords in records:
            plugin_handle_add_record_raw(handle, signature, form_id, 0, 0, 131, 0, subrecords)
        path = directory / name
        plugin_handle_call(handle, "save", str(path))
        return path
    finally:
        plugin_handle_close(handle)


def omod(editor_id: str, attach: int, keywords=(), model: str = "", form_type: bytes = b"WEAP"):
    data = struct.pack("<IIBB4sBBII", 0, 0, 0, 0, form_type, 0, 0, attach, 0) + bytes(12)
    out = [("EDID", editor_id.encode() + b"\0", None), ("DATA", data, None)]
    if model:
        out.append(("MODL", model.encode() + b"\0", None))
    if keywords:
        out.append(("MNAM", struct.pack(f"<{len(keywords)}I", *keywords), None))
    return out


def write_fake_world(directory: Path) -> Path:
    plugin_file(directory, "Base.esm", records=[
        ("KYWD", 0x800, [("EDID", b"AP_Bat\0", None)]),
        ("KYWD", 0x801, [("EDID", b"WeaponTypeMelee\0", None)]),
        ("KYWD", 0x802, [("EDID", b"AP_Rifle\0", None)]),
        ("WEAP", 0x810, [("EDID", b"BaseballBat\0", None), ("MODL", b"Weapons\\Bat\\Bat.nif\0", None),
                         ("APPR", struct.pack("<I", 0x800), None), ("KWDA", struct.pack("<I", 0x801), None)]),
        ("WEAP", 0x811, [("EDID", b"HuntingRifle\0", None), ("MODL", b"Weapons\\Rifle\\Rifle.nif\0", None),
                         ("APPR", struct.pack("<I", 0x802), None), ("KWDA", struct.pack("<I", 0x801), None)]),
        ("OMOD", 0x820, omod("mod_Bat_Spikes", 0x800, [0x801], "Weapons\\Bat\\Spikes.nif")),
        ("OMOD", 0x821, omod("mod_Rifle_Scope", 0x802, [0x801], "Weapons\\Rifle\\Scope.nif")),
    ])
    return plugin_file(directory, "Mod.esp", masters=("Base.esm",), records=[
        ("OMOD", 0x01000900, omod("mod_Bat_Skin", 0x00000800, [0x00000801])),
        ("OMOD", 0x01000901, omod("mod_Armor_Paint", 0x00000800, form_type=b"ARMO")),
    ])


def fake_forms(directory: Path, path: Path) -> PluginFormSource:
    session = EditorSession(auto_scan_conflicts=False, master_search_paths=[directory], lazy_masters=True)
    return PluginFormSource(session, str(path), directory)


@pytest.fixture
def world(tmp_path):
    path = write_fake_world(tmp_path)
    return path, fake_forms(tmp_path, path)


def test_the_skin_knows_which_weapons_take_it_and_their_meshes(world):
    _, forms = world
    extension = extension_for("WeaponModelSwaps")
    info = extension.skin_info(forms, "Mod.esp:000900")
    assert (info.signature, info.editor_id) == ("OMOD", "mod_Bat_Skin")
    assert [weapon.editor_id for weapon in info.weapons] == ["BaseballBat"]
    (weapon,) = info.weapons
    assert swaps.candidate_meshes(weapon, info.mods_by_weapon[weapon.key]) == [
        ("Weapon BaseballBat", "Weapons\\Bat\\Bat.nif"), ("Mod mod_Bat_Spikes", "Weapons\\Bat\\Spikes.nif")]


def test_a_non_weapon_omod_applies_to_nothing(world):
    _, forms = world
    info = extension_for("WeaponModelSwaps").skin_info(forms, "Mod.esp:000901")
    assert info.signature == "OMOD" and info.weapons == []


def test_warnings_for_an_empty_skin_and_a_wrong_record_type(world):
    path, forms = world
    session = CatalogSession.open(path, data_dir=path.parent, forms=forms)
    doc = session.feature("WeaponModelSwaps")
    doc.set_entry("skins", {"form": "Base.esm:000800", "models": [["a.nif", "a.nif"]]})
    extension = extension_for("WeaponModelSwaps")
    messages = [p.message for p in extension.problems(doc, forms)]
    assert any("itself" in m for m in messages) and any("KYWD record" in m for m in messages)
    assert all(not p.blocking for p in extension.problems(doc, forms))


def test_add_skin_action_creates_a_labelled_row(world):
    path, forms = world
    session = CatalogSession.open(path, data_dir=path.parent, forms=forms)
    doc = session.feature("WeaponModelSwaps")
    picked = {}

    class Tool:
        def __init__(self):
            self.forms = forms
            self.selected = None

        def pick_record(self, signatures, apply):
            picked["signatures"] = signatures
            apply("Mod.esp:000900")

        def select_row(self, key):
            self.selected = key

        def notify(self, *args, **kwargs):
            pass

    tool = Tool()
    (action,) = extension_for("WeaponModelSwaps").section_actions(tool, doc, "skins")
    action.run()
    assert picked["signatures"] == ("OMOD",) and tool.selected == "Mod.esp:000900"
    (row,) = doc.rows("skins")
    assert row.status == ADDED and row.entry == {"form": "Mod.esp:000900", "editor_id": "mod_Bat_Skin", "models": []}


@pytest.fixture
def headless(world, tmp_path):
    from imgui_bundle import imgui

    from ui.tools.catalog_editor.picker import Picker
    from ui.tools.catalog_editor.tool import CatalogEditorTool

    path, forms = world
    context = imgui.create_context()
    tool = CatalogEditorTool()
    io = imgui.get_io()
    io.display_size = imgui.ImVec2(1600, 1000)
    io.delta_time = 1 / 60
    io.backend_flags |= imgui.BackendFlags_.renderer_has_textures
    tool._session = CatalogSession.open(path, data_dir=tmp_path, forms=forms)
    tool._picker = Picker(forms)

    def frames(count=2):
        for _ in range(count):
            imgui.new_frame()
            imgui.begin("test")
            tool.draw_content()
            imgui.end()
            imgui.end_frame()

    try:
        yield tool, frames
    finally:
        tool.cleanup()
        imgui.destroy_context(context)


def test_the_guided_skin_form_draws_and_edits(headless):
    from imgui_bundle import imgui

    tool, frames = headless
    tool._feature, tool._section = "WeaponModelSwaps", "skins"
    doc = tool._current_document()
    key = doc.set_entry("skins", {"form": "Mod.esp:000900", "editor_id": "mod_Bat_Skin", "models": [["a.nif", "b.nif"]]})
    tool.select_row(key)
    frames(3)
    extension = extension_for("WeaponModelSwaps")
    row = doc.rows("skins")[0]
    assert extension.custom_form(doc, "skins")
    imgui.new_frame()
    imgui.begin("direct")
    changed, entry = extension.draw_form(tool, doc, "skins", row, tool._render_context(doc, read_only=False), False)
    imgui.end()
    imgui.end_frame()
    assert entry["models"] == [["a.nif", "b.nif"]] and entry["form"] == "Mod.esp:000900"
    assert not changed


def test_other_feature_extensions_are_registered():
    for feature in ("CampWallpapers", "Emotes", "Reputation"):
        assert extension_for(feature) is not None


# --- CampWallpapers, Emotes, Reputation ----------------------------------------------------------------


def test_wallpaper_rows_that_tales_drops_are_reported():
    sets = {"Set"}
    good = {"form": FORM, "material_swap": FORM, "model_swap": "Set"}
    assert extras.wallpaper_issues(good, sets) == []
    assert extras.wallpaper_issues({"form": FORM, "strip": True}, sets) == []
    assert len(extras.wallpaper_issues({"form": FORM, "model_swap": "Other"}, sets)) == 2
    assert "carries no swaps" in extras.wallpaper_issues({"form": FORM, "strip": True, "model_swap": "Set"}, sets)[0]


def test_emote_rules_match_the_reader():
    categories = {"SeventySix.esm:000900"}
    human = SAMPLES["Emotes"]["entries"][0]
    assert extras.emote_issues(human, categories) == []
    assert extras.emote_issues({**human, "duration": 61}, categories)
    assert extras.emote_issues({**human, "animation": "x.nif"}, categories)
    assert extras.emote_issues({**human, "animation": ""}, categories)
    assert extras.emote_issues({**human, "category_id": "SeventySix.esm:000999"}, categories)
    assert extras.emote_issues({"kind": "icon", "category_id": "SeventySix.esm:000900", "name": "n", "icon": "i",
                                "duration": 4}, categories) == []
    assert "pet" in extras.emote_issues({"kind": "pet"}, categories)[0]


def test_reputation_needs_seven_rising_tiers():
    faction = SAMPLES["Reputation"]["factions"][0]
    assert extras.faction_issues(faction) == []
    assert any("7 tiers" in m for m in extras.faction_issues({**faction, "tiers": faction["tiers"][:6]}))
    flat = [{**tier, "threshold": 5} for tier in faction["tiers"]]
    assert any("rise strictly" in m for m in extras.faction_issues({**faction, "tiers": flat}))
    assert any("HUD art" in m for m in extras.faction_issues({**faction, "code": "Other"}))
