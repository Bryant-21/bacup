"""Source-derived rules for Tales' playable-ghoul controller; no asset redistribution."""
from __future__ import annotations

import hashlib
import json
import math
import struct
from pathlib import Path

from bacup_lib.esp_native_runtime import load_esp_native
from bacup_lib.reputation_catalog import NativeReader, NativeResolver, first, reference_id, plugin_masters

OUTPUT = Path("F4SE/Plugins/B21_TalesFromAppalachia/player_ghoul.json")
THRESHOLDS = ("GHL_SURV_FeralThreshold_1_Fed", "GHL_SURV_FeralThreshold_2_Hungry",
              "GHL_SURV_FeralThreshold_3_Famished", "GHL_SURV_FeralThreshold_4_Starving")
FORMS = {"keyword": ("KYWD", "GHL_ActorTypePlayerGhoul"),
         "disguise": ("KYWD", "GHL_PlayerGhoulDisguiseKeyword"),
         "feral": ("AVIF", "GHL_SURV_Feral"), "tier": ("AVIF", "GHL_FeralTier"),
         "glow": ("AVIF", "B21_TFA_GhoulGlow"),
         "feral_perk": ("PERK", "GHL_SURV_FeralPerk"),
         "quest": ("QUST", "GHL00_Quest_TransformPlayer")}
ENGINE_CARDS = ("GHL_ArmsOfSteel", "GHL_ThickSkin", "GHL_GlowingGut", "GHL_GlowingHunter",
                "GHL_GlowingCriticals", "GHL_BrickWall", "GHL_ActionGhoul", "GHL_BattleGenes",
                "GHL_BreatheItIn", "GHL_GunTricks", "GHL_JaguarSpeed", "GHL_BoneShatterer")
MODIFIERS = {"AbFortifyStrength": (0, 1), "AbFortifyEndurance": (1, 1),
             "abReduceEndurance": (1, -1), "AbFortifyHealth": (2, 1),
             "abReduceHealth": (2, -1), "AbReduceActionPoints": (3, -1),
             "abReduceCharisma": (4, -1), "abFortifyDamageMeleeAll": (5, 1),
             "GHL_AbPerkFortifyRadiationExpose": (6, -1)}


def number(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError("Invalid numeric ghoul rule")
    return float(value)


def english(value):
    if isinstance(value, str):
        return value
    return next((row["String"] for row in (value or {}).get("Values", []) if row["Language"] == "English"), "")


def effects(record):
    result, current = [], None
    for field in record["fields"]:
        if "BaseEffect" in field:
            current = {"base": reference_id(field["BaseEffect"]), "conditions": []}
            result.append(current)
        elif current is not None:
            if "EFIT" in field:
                current["magnitude"] = number(field["EFIT"].get("Magnitude", 0))
            if "Magnitude" in field:
                current["global"] = reference_id(field["Magnitude"])
            if "CTDA" in field:
                current["conditions"].append(field["CTDA"])
    return result


def perk_floats(record):
    entries, current = [], None
    for field in record["fields"]:
        if "PRKE" in field:
            current = {"kind": field["PRKE"].get("Type"), "conditions": []}
        elif current is not None:
            if "EffectData" in field:
                data = bytes.fromhex(field["EffectData"].get("raw_hex", ""))
                if len(data) == 4:
                    current["entry"] = data[0]
                    current["function"] = data[1]
            if "EPFD" in field:
                data = bytes.fromhex(field["EPFD"].get("raw_hex", ""))
                if len(data) == 4:
                    current["value"] = number(struct.unpack("<f", data)[0])
            if "CTDA" in field:
                current["conditions"].append(field["CTDA"])
            if "EndMarker" in field:
                entries.append(current)
                current = None
    return entries


def condition_data(condition):
    data = bytes.fromhex(condition.get("raw_hex", ""))
    if len(data) == 32:
        return (struct.unpack_from("<H", data, 8)[0], struct.unpack_from("<f", data, 4)[0],
                data[0], struct.unpack_from("<I", data, 12)[0])
    parameter = condition.get("Parameter1")
    while isinstance(parameter, dict):
        if "value" in parameter:
            parameter = parameter["value"]
        elif "ActorValueActorValue" in parameter:
            parameter = parameter["ActorValueActorValue"]
        else:
            break
    return (condition.get("Function"), condition.get("ComparisonValue", {}).get("value", 0),
            condition.get("Type", 0), parameter if isinstance(parameter, int) else reference_id(parameter))


class Source(NativeReader):
    def record(self, object_id):
        return json.loads(self.native.plugin_handle_inspect_record(self.handle, object_id))["record"]

    def rows(self, signature, query="*"):
        return self.native.plugin_handle_search_records(self.handle, query, signatures=[signature], case_sensitive=False)


def build_catalog(source, resolver, source_name, digest, base_resolver=None):
    def get(signature, eid):
        return source.lookup(signature, eid)

    def global_value(eid):
        return number(first(get("GLOB", eid)[1], "Value"))

    player_perk, _ = get("PERK", "GHL_PlayerPerk")
    fractions = [entry["value"] for entry in perk_floats(source.record(player_perk))
                 if entry.get("entry") in (202, 203) and entry.get("function") in (1, 3) and "value" in entry]
    if not fractions or any(abs(value - fractions[0]) > .00001 for value in fractions) or not 0 < fractions[0] <= 1:
        raise ValueError("GHL_PlayerPerk radiation conversion is unsupported; refuse an assumed patch-era value")
    radiation_conditions = []
    for entry in perk_floats(source.record(player_perk)):
        if entry.get("entry") not in (202, 203):
            continue
        for condition in entry["conditions"]:
            function, comparison, operation, raw = condition_data(condition)
            if function not in (854, 699, 10019) or operation != 0 or comparison != 0 or (function != 10019 and not raw):
                raise ValueError(f"Unsupported radiation condition {function}")
            radiation_conditions.append({"entry": entry["entry"], "function": function,
                "comparison": comparison, "type": operation,
                "parameter_editor_id": source.fields(raw)[1] if raw else None})
    maximum = number(first(get("AVIF", "GHL_SURV_Feral")[1], "MaximumValue"))
    thresholds = [global_value(eid) for eid in THRESHOLDS]
    if not 0 < thresholds[0] < thresholds[1] < thresholds[2] < thresholds[3] == maximum:
        raise ValueError("Unsupported Feral thresholds")
    feral_effects = effects(source.record(get("SPEL", "GHL_SURV_Feral_Ability")[0]))
    feral_id = get("MGEF", "GHL_SURV_Feral_Effect")[0]
    rates = [row["magnitude"] for row in feral_effects if row["base"] == feral_id]
    if len(rates) != 2 or not 0 < rates[1] <= rates[0]:
        raise ValueError("Unsupported Feral decay/Moral Support records")

    tiers = [[0.0] * 8 for _ in range(5)]
    for row in feral_effects:
        _, eid, _ = source.fields(row["base"])
        if eid not in MODIFIERS:
            continue
        lower = 0.0
        for condition in row["conditions"]:
            data = bytes.fromhex(condition.get("raw_hex", ""))
            if len(data) >= 16 and data[0] & 4 and data[0] >> 5 == 3:
                ref = struct.unpack_from("<I", data, 4)[0]
                lower = number(first(source.fields(ref)[2], "Value"))
            elif condition.get("Type", 0) & 4 and condition.get("Type", 0) >> 5 == 3:
                ref = reference_id(condition["ComparisonValue"]["value"])
                lower = number(first(source.fields(ref)[2], "Value"))
        tier = sum(lower >= threshold for threshold in thresholds)
        slot, sign = MODIFIERS[eid]
        tiers[tier][slot] += sign * row["magnitude"]

    for ability in ("GHL_SURV_AbGhoulCharisma", "GHL_abPerkBaseRadResist"):
        for raw, _, eid, _ in source.rows("SPEL", ability):
            if eid != ability:
                continue
            for row in effects(source.record(raw)):
                _, effect_eid, _ = source.fields(row["base"])
                if effect_eid not in MODIFIERS:
                    raise ValueError(f"Unsupported baseline ghoul effect {effect_eid}")
                slot, sign = MODIFIERS[effect_eid]
                for tier in tiers:
                    tier[slot] += sign * row["magnitude"]

    chem_effect = get("MGEF", "GHL_SURV_Chem_Effect")[0]
    chems = []
    for raw, _, eid, _ in source.rows("ALCH"):
        relevant = [row for row in effects(source.record(raw)) if row["base"] == chem_effect]
        if not relevant:
            continue
        if len(relevant) != 1:
            raise ValueError(f"Ambiguous Feral chem effects on {eid}")
        row = relevant[0]
        recovery = number(first(source.fields(row["global"])[2], "Value")) if row.get("global") else row["magnitude"]
        binding = resolver.resolve("ALCH", eid)
        if not binding and base_resolver is not None:
            binding = base_resolver.resolve("ALCH", eid)
        if binding and recovery > 0:
            chems.append({"form": binding, "recovery": recovery, "editor_id": eid})

    diet = []
    for eid in ("GHL_ChemDiet01", "GHL_ChemDiet02", "GHL_ChemDiet03"):
        raw, _ = get("PERK", eid)
        values = [row["value"] for row in perk_floats(source.record(raw)) if "value" in row]
        if len(values) != 1 or not 1 <= values[0] <= 10:
            raise ValueError(f"Unsupported {eid} multiplier")
        diet.append({"form": resolver.resolve("PERK", eid), "multiplier": values[0]})

    hyper = effects(source.record(get("SPEL", "GHL_AbPerkHyperReflexes")[0]))
    if len(hyper) != 3 or any(source.fields(row["base"])[1] != "AbPerkFortifyEvadeChance" for row in hyper):
        raise ValueError("Hyper Reflexes effect changed")
    chances = [row["magnitude"] / 100 for row in hyper]
    glow_thresholds = []
    for index, row in enumerate(hyper):
        thresholds_for_rank = [comparison for function, comparison, operation, param in map(condition_data, row["conditions"])
                               if function == 14 and operation == 0x60 and source.fields(param)[1] == "Rads"]
        if len(thresholds_for_rank) != 1:
            raise ValueError("Hyper Reflexes Glow condition changed")
        glow_thresholds.append(number(thresholds_for_rank[0]))
        actual = {(function, operation, comparison, source.fields(param)[1] if param else None)
                  for function, comparison, operation, param in map(condition_data, row["conditions"])}
        expected = {(10017, 0, 1, None), (14, 0x60, glow_thresholds[-1], "Rads"),
                    (682, 0, 0, "ArmorTypePower")}
        if index < 2:
            expected.add((448, 0, 0, f"GHL_HyperReflexes0{index + 2}"))
        if index > 0:
            expected.add((448, 0, 1, f"GHL_HyperReflexes0{index + 1}"))
        if actual != expected:
            raise ValueError("Hyper Reflexes eligibility changed")
    if not all(0 < chance <= 1 for chance in chances) or not 0 < glow_thresholds[0] or len(set(glow_thresholds)) != 1:
        raise ValueError("Unsupported Hyper Reflexes rules")

    cards, appearance = [], []
    for raw, _, eid, _ in source.rows("PCRD", "GHL_*"):
        _, _, fields = source.fields(raw)
        restriction = first(fields, "Unknown") or {}
        if restriction.get("RaceRestriction") != "Ghoul":
            continue
        ranks = []
        for value in fields.get("MalePerk", []):
            perk_id = reference_id(value)
            _, perk_eid, perk_fields = source.fields(perk_id)
            ranks.append({"source": f"{perk_id:06X}:{source_name}", "editor_id": perk_eid,
                          "form": resolver.resolve("PERK", perk_eid),
                          "name": english(first(perk_fields, "Name")),
                          "description": english(first(perk_fields, "Description")),
                          "status": ("native-adapter" if perk_eid.startswith(("GHL_ChemDiet", "GHL_HyperReflexes")) or perk_eid == "GHL_MoralSupport01"
                                     else "engine-ready" if perk_eid.startswith(ENGINE_CARDS) else "blocked-effect-review")})
        cards.append({"source": f"{raw & 0xFFFFFF:06X}:{source_name}", "editor_id": eid,
                      "special": restriction.get("Special", "Strength"), "min_level": restriction.get("MinLevel", 0),
                      "art": first(fields, "MaleName"), "costs": fields.get("CardRankCost", []), "ranks": ranks})
    for signature in ("RACE", "ARMO", "ARMA", "HDPT", "TXST"):
        for raw, _, eid, _ in source.rows(signature, "GHL_*"):
            appearance.append({"source": f"{raw & 0xFFFFFF:06X}:{source_name}", "signature": signature,
                               "editor_id": eid, "converted": resolver.resolve(signature, eid)})
    bindings = {key: resolver.resolve(*value) for key, value in FORMS.items()}
    return {"schema_version": 1, "source": {"plugin": source_name, "sha256": digest},
            "forms": bindings, "available": all(bindings.values()),
            "source_radiation_conditions": radiation_conditions,
            "rules": {"radiation_fraction": fractions[0], "feral_maximum": maximum,
                      "feral_rate": rates[0], "team_feral_rate": rates[1], "thresholds": thresholds},
            "modifiers": tiers, "chems": chems, "chem_diet": diet,
            "hyper_reflexes": {"chances": chances, "glow_threshold": glow_thresholds[0]},
            "moral_support": resolver.resolve("PERK", "GHL_MoralSupport01"),
            "cards": cards, "appearance": appearance,
            "appearance_status": "human-race-head-and-skin-profile-with-asset-preflight",
            "glow_capacity_policy": "Tales: one current maximum-health bar; FO76 executable cap not established",
            "radiation_context_policy": "Tales applies the installed fraction to all post-resistance radiation; source effect-context conditions are inventoried, not reinterpreted as actor immunity.",
            "quest": {"required_level": 50, "completion_stage": 9000},
            "limitations": ["No player race swap; two converted head/skin profiles and source eyes; original appearance is the asset fallback.",
                            "Chem Diet, Moral Support and Hyper Reflexes use native adapters; twelve other families use reviewed converted engine effects.",
                            "Unreviewed FO76 combat/team/legendary effects remain unavailable rather than applying inert custom actor values.",
                            "Server team checks use a recruited FO4 companion.",
                            "Disguise condition is adapted to wearing its source keyword; server outfit/character services are not reproduced."]}


def emit_player_ghoul(source_plugin: Path, converted_plugin: Path, output_mod: Path, target_data_dir: Path | None = None):
    native = load_esp_native()
    handles = []
    try:
        source_handle = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        handles.append(source_handle)
        converted = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles.append(converted)
        base_resolver = None
        base_path = (target_data_dir or converted_plugin.parent) / "Fallout4.esm"
        if base_path.is_file():
            base = native.plugin_handle_load_index(str(base_path), game="fo4")
            handles.append(base)
            base_resolver = NativeResolver(native, base_path.name, base, plugin_masters(native, base))
        with source_plugin.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        catalog = build_catalog(Source(native, source_handle),
                                NativeResolver(native, converted_plugin.name, converted, plugin_masters(native, converted)),
                                source_plugin.name, digest, base_resolver)
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    destination = output_mod / OUTPUT
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(".tmp")
    temporary.write_text(json.dumps(catalog, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    temporary.replace(destination)
    return catalog


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--converted-plugin", type=Path, required=True)
    parser.add_argument("--output-mod", type=Path, required=True)
    parser.add_argument("--target-data-dir", type=Path)
    args = parser.parse_args()
    result = emit_player_ghoul(args.source, args.converted_plugin, args.output_mod, args.target_data_dir)
    print(f"Ghoul catalog: available={result['available']}, {len(result['chems'])} chems, {len(result['cards'])} cards")
