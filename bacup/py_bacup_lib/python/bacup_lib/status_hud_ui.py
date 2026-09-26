from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import remap_menu_fonts, swf_tags
from bacup_lib.quest_area_ui import closure, symbol_ids, tag
from bacup_lib.ui_contract import resolve_numbered_name
from bacup_lib.translations import merge_ui_translations, packaged_tales_lines, read_table
from bacup_lib.status_hud_source import inject_art, replace_tags, resolve_class_placements, select_children, sprite_tags, states, transform
from bacup_lib.favorites_ui import read_icon_keywords, resolve_icon_keywords, render_icon_resolver
from bacup_lib.reward_hud_source import reward_symbols, reward_contract, bitmap_dependencies

OUTPUT = Path("Interface/B21/TalesFromAppalachia/StatusHUD")
MOVIE = "statushud.swf"
ROOT = "B21_StatusHUD"
SCRIPT = Path(__file__).with_name("resources") / "status_hud/B21_StatusHUD.as"


def build_movie(source: bytes, title_library: bytes, marker_library: bytes,
                radial_library: bytes, source_icon_keywords: dict[str, str],
                frobber_library: bytes, button_library: bytes) -> tuple[bytes, dict]:
    from creation_lib.swf.native_runtime import compile_as3_do_abc, unbacked_symbol_classes

    tags = swf_tags(source)
    symbols = symbol_ids(tags)
    condition = resolve_numbered_name(symbols, "HUDMenu_fla.quickContainerConditionMeter_", "HUD condition meter")
    equipped_condition = resolve_numbered_name(symbols, "HUDMenu_fla.colorConditionMeter_", "HUD overrepair condition meter")
    divider = resolve_numbered_name(symbols, "HUDMenu_fla.QuestTrackerDivider_", "HUD quest divider")
    xp = resolve_numbered_name(symbols, "HUDMenu_fla.XPMeter_", "HUD XP meter")
    level = resolve_numbered_name(symbols, "HUDMenu_fla.LevelUpAnimation_", "HUD level-up animation")
    core = resolve_numbered_name(symbols, "HUDMenu_fla.HUDFusionCoreMeter_", "HUD fusion core meter")
    hit = resolve_numbered_name(symbols, "HUDMenu_fla.HitIndicator_", "HUD hit indicator")
    wanted = {"LeftMeters": "B21_StatusLeft", "HUDRightMeters": "B21_StatusRight",
              "HUDPlayerHPMeter": "B21_StatusHealth", "ActionPointMeter": "B21_StatusAP",
              "HUDCompassWidget": "B21_StatusCompass",
              "HUDCrosshair": "B21_StatusCrosshair", hit: "B21_StatusHit",
              "QuickContainerWidget": "B21_StatusQuickLoot",
              "HUDActiveEffectClip": "B21_StatusEffect",
              "ExplosiveIndicator": "B21_StatusExplosive",
              "DamageNumberClip": "B21_StatusDamage",
              "StealthMeter": "B21_StatusStealth", "EnemyHealthMeter": "B21_StatusEnemy",
              "EncounterHealthMeterContainer": "B21_StatusEncounter",
              "CritMeter": "B21_StatusCritical", "CritMeterStar": "B21_StatusStar",
              core: "B21_StatusCore",
              "HUDQuestTrackerEntry": "B21_StatusQuestEntry",
              "HUDQuestTrackerObjective": "B21_StatusQuestObjective",
              xp: "B21_StatusXP", level: "B21_StatusLevelUp",
              divider: "B21_StatusQuestDivider", condition: "B21_StatusCondition",
              equipped_condition: "B21_StatusEquippedCondition"}
    rewards = reward_symbols(symbols)
    reward_ids, reward_data = reward_contract(source, tags, symbols, rewards)
    wanted.update(rewards)
    groups = {
        symbols["QuickContainerWidget"]: {"ListHeaderAndBracket_mc", "ListItems_mc", "Spinner_mc",
                                           "WeightIcon_mc", "WeightText_mc"},
        symbols["DamageNumberClip"]: {"Base_mc", "Crit_mc"},
        symbols["LeftMeters"]: {"HPMeter_mc", "RadsMeter_mc"},
        symbols["HUDRightMeters"]: {"ActionPointMeter_mc", "HUDActiveEffectsWidget_mc", "FeralMeter_mc",
                                    "HUDHungerMeter_mc", "HUDThirstMeter_mc", "AmmoCount_mc",
                                    "ExplosiveAmmoCount_mc", "OverheatMeter_mc"},
        symbols["EnemyHealthMeter"]: {"HealthBarFrame_mc", "Optional_mc", "MeterBarFriendly_mc",
                                     "MeterBar_mc", "MeterBarEnemy_mc", "DisplayText_mc", "LevelText_mc",
                                     "EncounterHolder_mc"},
        symbols["EncounterHealthMeter"]: {"DisplayText_mc", "MeterFrame_mc", "MeterBarNonhostile_mc",
                                         "MeterBar_mc", "EncounterHolder_mc"},
        symbols["HUDCompassWidget"]: {"CompassBGHolder_mc", "CompassBar_mc", "OtherMask_mc",
                                      "AreaQuest_WithinClip_mc", "AreaQuest_WithinClipPA_mc",
                                      "OtherMarkerHolder_mc", "QuestMask_mc", "QuestMarkerHolder_mc"},
        symbols[xp]: {"BG", "Optional_mc", "LevelUPBar", "CurrentLevelField", "LevelUpBracket",
                      "xptext", "NumberText", "PlusSign", "LeveUpTextClip"},
    }
    tags = [(code, select_children(payload, groups[struct.unpack_from("<H", payload)[0]],
                                  keep_unnamed=struct.unpack_from("<H", payload)[0] == symbols["QuickContainerWidget"]))
            if code == 39 and struct.unpack_from("<H", payload)[0] in groups else (code, payload)
            for code, payload in tags]
    for index, (code, payload) in enumerate(tags):
        if code != 37:
            continue
        at = 2 + (5 + 4 * (payload[2] >> 3) + 7) // 8
        if payload[at] & 1:
            tags[index] = (code, payload[:at] + bytes((payload[at] & ~1, payload[at + 1] | 128)) +
                           b"$MAIN_Font_Bold\0" + payload[at + 4:])
    selected = set()
    for name in wanted:
        if name not in symbols:
            raise ValueError(f"Missing FO76 status HUD symbol: {name}")
        selected.update(closure(tags, symbols[name], name))
    selected.update(bitmap_dependencies(tags, selected))
    art = [entry for entry in tags if entry in selected]
    hud_art_count = len(art)
    body = zlib.decompress(source[8:]) if source[:3] == b"CWS" else source[8:]
    header = body[:(5 + 4 * (body[0] >> 3) + 7) // 8 + 2] + struct.pack("<H", 1)
    def assemble(entries):
        body = header + b"".join(tag(*entry) for entry in entries)
        return b"FWS" + source[3:4] + struct.pack("<I", len(body) + 8) + body

    seed = assemble([*art, (76, struct.pack("<HH", 1, 0) + ROOT.encode() + b"\0"), (1, b""), (0, b"")])
    seed = inject_art(title_library, seed, [("QuestTrackerObjectiveTitle", "B21_StatusObjectiveTitle")])
    from creation_lib.swf.markers import marker_icon_table
    marker_table = marker_icon_table()
    marker_names = symbol_ids(swf_tags(marker_library))
    marker_names = {name: "B21_StatusMap" + name for name in marker_names
                    if name.endswith("Marker") or name in ("CompassMarkerWidget", "DirectionMarkerWidget")
                    or name in {row.source_symbol for row in marker_table}}
    seed = inject_art(marker_library, seed, sorted(marker_names.items()))
    radial_symbols = set(symbol_ids(swf_tags(radial_library)))
    weapon_keywords = {name: icon for name, icon in source_icon_keywords.items() if name.startswith("UI_WeaponType")}
    icon_keywords = resolve_icon_keywords(weapon_keywords, radial_symbols)
    if not icon_keywords or weapon_keywords.keys() != icon_keywords.keys():
        raise ValueError("Missing FO76 condition weapon icon keywords/artwork")
    weapon_names = {name: "B21_StatusWeapon" + name for name in sorted(set(icon_keywords.values()) | {"UnknownIcon"})}
    seed = inject_art(radial_library, seed, sorted(weapon_names.items()))
    frobber_tags = swf_tags(frobber_library)
    frobber_symbols = symbol_ids(frobber_tags)
    frobber_internal = frobber_symbols[resolve_numbered_name(frobber_symbols,
        "HUDFrobberWidget_fla.HUDFrobberWidgetInternal_", "HUD interaction prompt")]
    frobber_body = next(p for c, p in frobber_tags if c == 39 and struct.unpack_from("<H", p)[0] == frobber_internal)
    hold_position = transform(states(sprite_tags(frobber_body))[0]["ButtonHintBar_mc"])
    frobber = replace_tags(frobber_library, [(c, select_children(p, {"Header_mc"}))
        if c == 39 and struct.unpack_from("<H", p)[0] == frobber_internal else (c, p) for c, p in frobber_tags])
    seed = inject_art(frobber, seed, [("HUDFrobberWidget", "B21_StatusInteraction")])
    seed = inject_art(button_library, seed, [("Shared.AS3.BSButtonHint", "B21_StatusHoldButton")])
    marker_aliases = {row.symbol: marker_names[row.source_symbol] for row in marker_table}
    injected = symbol_ids(swf_tags(seed))
    title_id = injected["B21_StatusObjectiveTitle"]
    art = [(code, resolve_class_placements(payload, {"QuestTrackerObjectiveTitle": title_id}))
           if code == 39 else (code, payload) for code, payload in swf_tags(seed) if code not in (76, 1, 0)]
    # Nested source classes must not bind against identically named FO4 classes.
    classes = {symbols[name]: replacement for name, replacement in wanted.items()}
    classes.update({cid: name for name, cid in injected.items() if cid})
    for code, payload in art:
        if code == 39:
            cid = struct.unpack_from("<H", payload)[0]
            classes.setdefault(cid, f"B21_StatusArt{cid}")
    scripts = [render_icon_resolver(SCRIPT.read_text(encoding="utf-8"), icon_keywords),
               SCRIPT.with_name("B21_StatusReticle.as").read_text(encoding="utf-8"),
               SCRIPT.with_name("B21_StatusLoot.as").read_text(encoding="utf-8")]
    original_sprites = {struct.unpack_from("<H", p)[0]: p for c, p in swf_tags(source) if c == 39}
    root = states(swf_tags(source))[0]
    top = states(sprite_tags(original_sprites[root["TopRightGroup_mc"].character_id]))[0]["QuestTracker"]
    quest_position = transform(top)
    quest_position[4] += root["TopRightGroup_mc"].matrix.translate_x / 20
    quest_position[5] += root["TopRightGroup_mc"].matrix.translate_y / 20
    bottom = states(sprite_tags(original_sprites[root["BottomCenterGroup_mc"].character_id]))[0]["CompassWidget_mc"]
    compass_position = transform(bottom)
    compass_position[4] += root["BottomCenterGroup_mc"].matrix.translate_x / 20
    compass_position[5] += root["BottomCenterGroup_mc"].matrix.translate_y / 20
    layout = {"left": transform(root["LeftMeters_mc"]), "right": transform(root["RightMeters_mc"]),
              "compass": compass_position, "quest": quest_position,
              "level": transform(root["LevelUpAnimation_mc"]), "holdButton": hold_position,
              "holdFPS": struct.unpack_from("<H", body, (5 + 4 * (body[0] >> 3) + 7) // 8)[0] / 256}
    notifications = root["HUDNotificationsGroup_mc"]
    xp_position = transform(states(sprite_tags(original_sprites[notifications.character_id]))[0]["XPMeter_mc"])
    xp_position[4] += notifications.matrix.translate_x / 20
    xp_position[5] += notifications.matrix.translate_y / 20
    layout["xp"] = xp_position
    for key, group, child in (("enemy", "TopCenterGroup_mc", "EnemyHealthMeter_mc"),
                              ("stealth", "TopCenterGroup_mc", "StealthMeter_mc"),
                              ("critical", "BottomCenterGroup_mc", "CritMeter_mc"),
                              ("encounter", "BottomCenterGroup_mc", "EncounterHealthMeterContainer_mc"),
                              ("crosshair", "CenterGroup_mc", "HUDCrosshair_mc"),
                              ("hit", "CenterGroup_mc", "HitIndicator_mc"),
                              ("quickLoot", "CenterGroup_mc", "QuickContainerWidget_mc"),
                              ("core", "RightMeters_mc", "HUDFusionCoreMeter_mc")):
        parent = transform(root[group])
        local = transform(states(sprite_tags(original_sprites[root[group].character_id]))[0][child])
        a, b, c, d, x, y = parent
        e, f, g, h, u, v = local
        layout[key] = [a*e+c*f, b*e+d*f, a*g+c*h, b*g+d*h, a*u+c*v+x, b*u+d*v+y]
    quick_children = states(sprite_tags(original_sprites[symbols["QuickContainerWidget"]]))[0]
    layout["quickButtons"] = transform(quick_children["ButtonHintBar_mc"])
    damage_frames = {}
    damage_children = states(sprite_tags(original_sprites[symbols["DamageNumberClip"]]))[0]
    for key in ("Base_mc", "Crit_mc"):
        payload = original_sprites[damage_children[key].character_id]
        damage_frames[key] = struct.unpack_from("<H", payload, 2)[0]
    scripts.extend([SCRIPT.with_name("B21_StatusRewards.as").read_text(),
                    SCRIPT.with_name("B21_StatusRewardTimeline.as").read_text(),
                    "package { public class B21_StatusRewardSource { public static var data:Object = " +
                    json.dumps(reward_data) + "; } }"])
    scripts.append("package { public class B21_StatusSource { public static var damageFrames:Object = " +
                   json.dumps(damage_frames) + "; public static var damageFPS:Number = " +
                   str(struct.unpack_from("<H", header, len(header)-4)[0] / 256) +
                   "; public static var layout:Object = " +
                   json.dumps(layout) + "; public static var markers:Object = " + json.dumps(marker_names | marker_aliases) +
                   "; public static var weapons:Object = " + json.dumps(weapon_names) + "; } }")
    area_ids = {symbols[resolve_numbered_name(symbols, stem, "area objective")] for stem in
                ("HUDMenu_fla.AreaQuest_CompassWithinRect_", "HUDMenu_fla.AreaQuest_CompassWithinRectPA_")}
    animated_ids = {struct.unpack_from("<H", p)[0] for c, p in closure(tags, symbols[level], level) if c == 39}
    # FO76's ghoul meters stop on these frames; without them rollOn, emptyAnim and the Glow outline loop.
    ghoul_stops = {symbols[resolve_numbered_name(symbols, stem, "ghoul meter")]: frames for stem, frames in (
        ("HUDMenu_fla.FeralMeter_", (0, 6, 12)), ("HUDMenu_fla.FeralMeterInternal_", (159,)),
        ("HUDMenu_fla.GlowMeterGlowAnim_", (0, 48, 96)))}
    for cid, name in classes.items():
        if cid in reward_ids:
            scripts.append(f'package {{ public dynamic class {name} extends B21_StatusRewardTimeline {{ '
                           f'public function {name}() {{ super("{cid}"); }} }} }}')
            continue
        frames = "addFrameScript(0, frame1, 139, frame140);" if cid in area_ids else "" if cid in animated_ids else "stop();"
        methods = "private function frame1():void { stop(); } private function frame140():void { gotoAndPlay(50); }" if cid in area_ids else ""
        if cid in ghoul_stops:
            frames = "addFrameScript(" + ", ".join(f"{frame}, halt" for frame in ghoul_stops[cid]) + ");"
            methods = "private function halt():void { stop(); }"
        if cid == symbols[hit]:
            # FO76's own scripts stop on Hidden and on the last frame of Start, StartProtected and
            # StartReflected; the engine then hides it, so here the clip returns to Hidden itself.
            frames = "addFrameScript(0, halt, 12, hide, 27, hide, 40, hide);"
            methods = "private function halt():void { stop(); } private function hide():void { gotoAndStop(1); }"
        scripts.append(f"package {{ import flash.display.MovieClip; public dynamic class {name} extends MovieClip {{"
                       f" public function {name}() {{ {frames} }} {methods} }} }}")
    exports = [(0, ROOT), *sorted(classes.items())]
    bindings = struct.pack("<H", len(exports)) + b"".join(
        struct.pack("<H", cid) + name.encode() + b"\0" for cid, name in exports)
    body = header + tag(69, struct.pack("<I", 8)) + b"".join(tag(*entry) for entry in art) + \
        tag(82, compile_as3_do_abc(scripts)) + tag(76, bindings) + tag(1, b"") + tag(0, b"")
    movie = remap_menu_fonts(b"FWS" + source[3:4] + struct.pack("<I", len(body) + 8) + body)
    if unbacked_symbol_classes(movie):
        raise ValueError("Unbound status HUD artwork")
    return movie, {"source_symbols": wanted, "art_tags": len(art), "hud_art_tags": hud_art_count,
                   "classes": list(classes.values()),
                   "source_layout": layout, "marker_symbols": marker_names, "weapon_symbols": weapon_names,
                   "damage_frames": damage_frames,
                   "reward_contract": reward_data,
                   "icon_keywords": icon_keywords,
                   "import_closure": {"HUDObjectiveTitleIconLibrary.swf": ["QuestTrackerObjectiveTitle"],
                                      "MapMarkerLibrary.swf": sorted(marker_names), "RadialMenu.swf": sorted(weapon_names),
                                      "HUDFrobberWidget.swf": ["HUDFrobberWidget"],
                                      "BSButtonHintBar.swf": ["Shared.AS3.BSButtonHint"]},
                   "imports": [], "font_provider": "FO4 HUDMenu fonts_en.swf"}


def convert_status_hud_ui(source_root: Path, output_data: Path, *, source_plugin: Path) -> dict:
    source = Path(source_root) / "interface/hudmenu.swf"
    data = source.read_bytes()
    title_library = (source.parent / "hudobjectivetitleiconlibrary.swf").read_bytes()
    marker_library = (source.parent / "mapmarkerlibrary.swf").read_bytes()
    radial_library = (source.parent / "radialmenu.swf").read_bytes()
    movie, manifest = build_movie(data, title_library, marker_library, radial_library, read_icon_keywords(source_plugin),
        (source.parent / "hudfrobberwidget.swf").read_bytes(), (source.parent / "bsbuttonhintbar.swf").read_bytes())
    target = Path(output_data) / OUTPUT
    target.mkdir(parents=True, exist_ok=True)
    (target / MOVIE).write_bytes(movie)
    manifest.update(schema_version=7, source_sha256=hashlib.sha256(data).hexdigest(),
                    bridge_sha256=hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),
                    files=[{"path": (OUTPUT / MOVIE).as_posix(), "sha256": hashlib.sha256(movie).hexdigest()}])
    (target / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    reward_keys = {"$EVENTCOMPLETED", "$EVENTFAILED", "$ITEMREWARD", "$QUESTCOMPLETED", "$QUESTFAILED"}
    reward_lines = [line for line in read_table(source.parent / "translate_en.txt")
                    if line.split("\t", 1)[0] in reward_keys]
    merge_ui_translations(output_data, reward_lines +
        [line for line in packaged_tales_lines() if line.startswith("$B21_StatusHUD_")])
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    parser.add_argument("--source-plugin", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(convert_status_hud_ui(args.source_root, args.output_data, source_plugin=args.source_plugin), indent=2))
