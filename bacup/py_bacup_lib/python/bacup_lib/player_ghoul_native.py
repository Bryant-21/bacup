from __future__ import annotations

import struct

from bacup_lib.reputation_catalog import first, reference_id


CARD_KINDS = {
    "GHL_ScienceMonsterCard": "science_monster",
    "GHL_WildWestHandsCard": "wild_west_hands",
    "GHL_RadReaverCard": "rad_reaver",
    "GHL_RadiationPowerCard": "radiation_power",
    "GHL_MadScientistCard": "mad_scientist",
    "GHL_RadioactiveStrengthCard": "radioactive_strength",
    "GHL_BombScientistCard": "bomb_scientist",
    "GHL_FaultySpotsCard": "faulty_spots",
    "GHL_FeralPresenceCard": "feral_presence",
    "GHL_RadSpecialistCard": "rad_specialist",
    "GHL_GlowingOneCard": "glowing_one",
    "GHL_EyeOfTheHunterCard": "eye_of_the_hunter",
    "GHL_GunTricksCard": "gun_tricks",
    "GHL_BreatheItInCard": "breathe_it_in",
    "GHL_ActionGhoulCard": "action_ghoul",
    "GHL_JaguarSpeedCard": "jaguar_speed",
    "GHL_BattleGenesCard": "battle_genes",
    "GHL_BoneShattererCard": "bone_shatterer",
    "GHL_BrickWallCard": "brick_wall",
    "GHL_GlowingCriticalsCard": "glowing_criticals",
    "GHL_GlowingGutCard": "glowing_gut",
    "GHL_ThickSkinCard": "thick_skin",
    "GHL_ArmsOfSteelCard": "arms_of_steel",
    "GHL_GlowingHunterCard": "glowing_hunter",
}


def build_native_rules(source, resolver, cards, condition_data, number, extra_spells=()):
    records, profiles = {}, {}

    def record(raw):
        if not raw:
            return None
        key = str(raw)
        if key in records:
            return records[key]
        signature, eid, fields = source.fields(raw)
        row = {"signature": signature, "editor_id": eid, "form": resolver.resolve(signature, eid)}
        records[key] = row
        if signature == "GLOB":
            row["value"] = number(first(fields, "Value"))
        elif signature == "CNDF":
            row["conditions"] = [condition(value) for value in fields.get("CTDA", [])]
        elif signature == "MGEF":
            data = bytes.fromhex((first(fields, "Data") or {}).get("raw_hex", ""))
            if len(data) >= 88:
                row["flags"] = struct.unpack_from("<I", data, 0)[0]
                row["archetype"] = struct.unpack_from("<I", data, 68)[0]
                av = struct.unpack_from("<I", data, 72)[0]
                associated = (struct.unpack_from("<I", data, 12)[0] if row["archetype"] == 35 else
                              struct.unpack_from("<I", data, 80)[0] if row["archetype"] in (20, 45) else 0)
                row["stat"] = record(av)["editor_id"] if av else ""
                row["associated"] = associated
                if associated:
                    record(associated)
        elif signature == "SPEL":
            effects, current = [], None
            for field in source.record(raw)["fields"]:
                if "BaseEffect" in field:
                    effect = reference_id(field["BaseEffect"])
                    current = {"base": effect, "magnitude": 0, "duration": 0, "area": 0, "conditions": []}
                    effects.append(current)
                    record(effect)
                elif current is not None:
                    if "EFIT" in field:
                        current.update({name.lower(): number(field["EFIT"].get(name, 0)) for name in ("Magnitude", "Duration", "Area")})
                    if "Magnitude" in field:
                        current["magnitude"] = record(reference_id(field["Magnitude"]))["value"]
                    if "CTDA" in field:
                        current["conditions"].append(condition(field["CTDA"]))
            row["effects"] = effects
        return row

    def condition(value):
        function, comparison, flags, parameter = condition_data(value)
        raw = bytes.fromhex(value.get("raw_hex", ""))
        run_on = struct.unpack_from("<I", raw, 20)[0] if len(raw) == 32 else int(value.get("RunOn") == "Target")
        if flags & 4:
            comparison_id = struct.unpack_from("<I", raw, 4)[0] if len(raw) == 32 else reference_id(value["ComparisonValue"]["value"])
            comparison = record(comparison_id)["value"]
            flags &= ~4
        if parameter:
            record(parameter)
        return {"function": function, "comparison": number(comparison), "flags": flags,
                "parameter": parameter, "run_on": run_on}

    def entries(raw):
        result, current, tab = [], None, 0
        for field in source.record(raw)["fields"]:
            if "PRKE" in field:
                current = {"type": field["PRKE"].get("Type", "Quest"), "conditions": {}}
                tab = 0
            elif current is not None:
                if "EffectData" in field:
                    data = bytes.fromhex(field["EffectData"].get("raw_hex", ""))
                    if current["type"] == "Ability" and len(data) == 4:
                        current["spell"] = struct.unpack("<I", data)[0]
                        record(current["spell"])
                    elif current["type"] == "EntryPoint" and len(data) == 4:
                        current["point"], current["operation"] = data[0], data[1]
                if "RunOnTabIndex" in field:
                    tab = field["RunOnTabIndex"]
                if "CTDA" in field:
                    current["conditions"].setdefault(str(tab), []).append(condition(field["CTDA"]))
                if "Type" in field:
                    current["data_type"] = field["Type"]
                if "EPFD" in field:
                    data = bytes.fromhex(field["EPFD"].get("raw_hex", ""))
                    if len(data) == 4 and current.get("data_type") == "SpellItem":
                        current["spell"] = struct.unpack("<I", data)[0]
                        record(current["spell"])
                    elif len(data) == 4 and current.get("data_type") == "LeveledList":
                        current["form_value"] = struct.unpack("<I", data)[0]
                        record(current["form_value"])
                    elif len(data) == 4:
                        current["value"] = number(struct.unpack("<f", data)[0])
                if "UnknownSpellValue" in field:
                    current["recipient"] = field["UnknownSpellValue"]
                if "EndMarker" in field:
                    result.append(current)
                    current = None
        return result

    for raw in extra_spells:
        record(raw)
    for card in cards:
        kind = CARD_KINDS.get(card["editor_id"])
        if kind is None:
            continue
        ranks = [int(rank["source"].split(":")[1], 16) for rank in card["ranks"]]
        for raw in ranks:
            record(raw)
        profiles[card["source"]] = {"kind": kind, "perks": ranks, "ranks": [entries(raw) for raw in ranks]}
    return {"version": 1, "cards": profiles, "records": records}
