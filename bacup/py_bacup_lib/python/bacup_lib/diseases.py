"""FO76 disease catalog for the Tales runtime (Diseases.json).

FO76 rolled diseases on the server; the client scripts that carry the tables are hollow.
Every number here comes from their VMAD properties in the user's SeventySix.esm:
- Player (NPC_ 000007) SURV_PlayerDiseaseScript: DiseaseData (disease spells, specific immunity),
  DiseaseChanceData (hazard/trap/bed/water rows), DiseasedHitData (creature hits),
  EnduraceDiseaseChanceMultData, SURV_DiseaseResistance, the SURV_DiseaseChanceMult_*_Formula AVs,
  SURV_Disease_ImmunityDuration_Global and the TimerDuration_* intervals.
- SURV_Master (QUST) SURV_DiseasedCreatureScript: which creature types can be "Diseased".
- SURV_DiseaseVector_{Food,Drink}_Effect MGEFs: the eat/drink vector (magnitude is the chance).
- SURV_DiseaseCure*/SURV_CureAddiction* MGEFs and SURV_AddictionScript: cures.
- PerkAntibioticScript: stimpak cure chance per rank.
- Each disease's effect MGEFs: SURV_DiseasePeriodicEffectScript (start/repeat/finish messages and
  periodic spells), SURV_DiseaseOnHitEffectScript and the FO76-only "spell when hit" perk entry
  point, which FO4 has no equivalent for.
- DefaultApplyDiseaseOnTriggerEnter placements (their primitive box) and the RadAway resistance
  penalty that the conversion loses when RadAway resolves to Fallout4.esm's record.
Struct members the record leaves unset use the script's declared defaults (DiseaseChance 5,
DiseaseChanceMin 1, MinCreatureLevelForDisease 10, TimerDuration_InWater/InHazard 10 s,
DiseasedCreatureTransmissionChanceLevelMult 0.1).
"""
from __future__ import annotations

import json
import re
import struct
from pathlib import Path

from bacup_lib.plugin_catalog import write_document
from bacup_lib.reputation_catalog import first, reference_id

FEATURE = "Diseases"

CHANCE_TYPES = {1: "hazard", 2: "trap", 3: "weather", 4: "bed", 5: "water"}
DEFAULT_ROW_CHANCE = 5
DEFAULT_ROW_MINIMUM = 1
DEFAULT_CREATURE_MIN_LEVEL = 10
DEFAULT_TRANSMISSION_LEVEL_MULT = 0.1
DEFAULT_INTERVALS = {"water": 10.0, "hazard": 10.0}
# FO76 perk entry point 193 with function SelectSpell casts its spell on the perk owner when hit
# (SURV_Disease_GlowingPustules_Perk). FO4 has no such entry point, so Tales casts it.
HIT_SPELL_ENTRY_POINT, SELECT_SPELL_FUNCTION = 0xC1, 0x0A
MGEF_PERK_OFFSET = 140
RESISTANCE_PENALTY_EFFECTS = ("SURV_ReduceDiseaseResistance_Potion_Effect",)
TRIGGER_SCRIPT = "DefaultApplyDiseaseOnTriggerEnter"
# The *_Formula AVs the script reads are engine formulas over these plain AVs, which is what
# FO76 perks, magazines and buffs modify (AbReduceDiseaseChance*, Magazine_ReduceDiseaseChance*).
CHANCE_MULTIPLIERS = {
    "generic": "SURV_DiseaseChanceMult_Generic", "food": "SURV_DiseaseChanceMult_Food",
    "drink": "SURV_DiseaseChanceMult_Drink", "water": "SURV_DiseaseChanceMult_InWater",
    "bed": "SURV_DiseaseChanceMult_Bed", "combat": "SURV_DiseaseChanceMult_Combat",
    "trap": "SURV_DiseaseChanceMult_Trap", "hazard": "SURV_DiseaseChanceMult_EnvironmentalHazard",
}
VECTORS = {"SURV_DiseaseVector_Food_Effect": "food", "SURV_DiseaseVector_Drink_Effect": "drink",
           "SURV_DiseaseVector_Drink_Effect_WaterWitchPerk": "drink"}
CURES = {"SURV_DiseaseCure_Potion_Effect": "SURV_DiseaseCureEffectScript",
         "SURV_DiseaseCureAll_Potion_Effect": "SURV_DiseaseCureAllEffectScript"}
ADDICTION_CURES = ("SURV_CureAddictionAll", "SURV_CureAddictionRandom")
# SPEL SURV_PlayerInWater picks dirty vs toxic from the water's contact spell
# (IsWaterContactSpell WaterRadiationHazard / WaterRadiationHighHazard); any other contact
# spell (nuclear waste, FEV) is strictly worse and takes the toxic tier.
DIRTY_WATER_CONTACT = "WaterRadiationHazard"
WATER_RISK = re.compile(r"SURV_DiseaseRiskForm_(Dirty|Toxic)Water_(Standing|Swimming|UnderWater)$")


def struct_rows(value):
    return [{member["memberName"]: member["Value"] for member in row} for row in value or []]


def object_id(value):
    return reference_id(value.get("FormID")) if isinstance(value, dict) else None


def script_properties(fields, script_name):
    for vmad in fields.get("VirtualMachineAdapter") or []:
        for script in vmad.get("Scripts") or []:
            if script.get("ScriptName", "").lower() == script_name.lower():
                return {row["propertyName"]: row["Value"] for row in script.get("Properties") or []}
    return None


class ChainResolver:
    """The first resolver that knows a form wins: the conversion maps FO76 records onto
    same-EditorID records in every target master (Fallout4.esm and the DLC), not just the base game."""

    def __init__(self, resolvers):
        self.resolvers = [resolver for resolver in resolvers if resolver is not None]

    def resolve(self, signature, editor_id):
        for resolver in self.resolvers:
            value = resolver.resolve(signature, editor_id)
            if value is not None:
                return value
        return None


def converted_owner(value):
    return value.split(":", 1)[0] if isinstance(value, str) else None


def build_catalog(source, resolver, number, base_resolver=None):
    def editor_id(raw):
        return source.fields(raw)[1]

    def binding(raw):
        if not raw:
            return None
        signature, eid, _ = source.fields(raw)
        value = resolver.resolve(signature, eid)
        if value is None and base_resolver is not None:
            value = base_resolver.resolve(signature, eid)
        return value

    def required(raw, what):
        value = binding(raw)
        if value is None:
            raise ValueError(f"{what} {editor_id(raw) if raw else '(unset)'} has no converted form")
        return value

    def global_value(raw):
        value = first(source.fields(raw)[2], "Value")
        return number(value.get("value") if isinstance(value, dict) else value)

    def lookup(signature, eid):
        rows = [raw for raw, _, found, _ in source.rows(signature, eid) if found == eid]
        if len(rows) != 1:
            raise ValueError(f"Expected one FO76 {signature} {eid}, found {len(rows)}")
        return rows[0]

    def form_list(raw):
        return [value for value in (reference_id(row) for row in source.fields(raw)[2].get("FormID") or []) if value]

    def message(raw):
        if not raw:
            return None
        description = first(source.fields(raw)[2], "Description")
        if isinstance(description, str):
            return description or None
        text = next((row["String"] for row in (description or {}).get("Values", []) if row["Language"] == "English"), "")
        return text or None

    def chance_global(raw):
        return global_value(raw) if raw else None

    player = source.record(7)
    fields = {key: [value] for field in player["fields"] for key, value in field.items()}
    props = script_properties(fields, "SURV_PlayerDiseaseScript")
    if props is None:
        raise ValueError("FO76 Player has no SURV_PlayerDiseaseScript")

    def raw_bytes(value):
        return bytes.fromhex(value["raw_hex"]) if isinstance(value, dict) and "raw_hex" in value else b""

    def hit_spell_from_perk(effect_fields):
        data = raw_bytes(first(effect_fields, "Data"))
        if len(data) < MGEF_PERK_OFFSET + 4:
            return None
        perk = struct.unpack_from("<I", data, MGEF_PERK_OFFSET)[0]
        try:
            signature, _, perk_fields = source.fields(perk)
        except (KeyError, ValueError, TypeError):
            return None
        if signature != "PERK":
            return None
        entry = next((raw_bytes(value) for value in perk_fields.get("EffectData") or [] if len(raw_bytes(value)) == 4), b"")
        spell = raw_bytes(first(perk_fields, "EPFD"))
        if len(entry) != 4 or entry[0] != HIT_SPELL_ENTRY_POINT or entry[1] != SELECT_SPELL_FUNCTION or len(spell) != 4:
            return None
        return binding(struct.unpack("<I", spell)[0])

    def symptoms(spell_raw):
        result = {"timers": [], "periodic": [], "finish_message": None, "on_hit": []}
        for effect in source.fields(spell_raw)[2].get("BaseEffect") or []:
            effect_raw = reference_id(effect)
            if not effect_raw:
                continue
            effect_fields = source.fields(effect_raw)[2]
            periodic = script_properties(effect_fields, "SURV_DiseasePeriodicEffectScript")
            if periodic:
                for timer in struct_rows(periodic.get("TimerData")):
                    interval = object_id(timer.get("IntervalGlobal"))
                    if interval:
                        result["timers"].append({"id": int(timer.get("ID", 0)), "interval": global_value(interval)})
                for row in struct_rows(periodic.get("PeriodicEffectData")):
                    result["periodic"].append({"on_start": bool(row.get("FireOnStart", False)),
                                               "timer": int(row.get("FireOnTimerID", -1)),
                                               "message": message(object_id(row.get("MessageToDisplay"))),
                                               "spell": binding(object_id(row.get("SpellToCast")))})
                result["finish_message"] = result["finish_message"] or message(object_id(periodic.get("MessageToDisplayOnFinish")))
            on_hit = script_properties(effect_fields, "SURV_DiseaseOnHitEffectScript")
            if on_hit and (spell := binding(object_id(on_hit.get("SpellToCastOnTargetWhenHit")))):
                result["on_hit"].append({"spell": spell, "dispel": bool(on_hit.get("DispelBeforeRecast", True))})
            if spell := hit_spell_from_perk(effect_fields):
                result["on_hit"].append({"spell": spell, "dispel": False})
        return result

    diseases = []
    for row in struct_rows(props["DiseaseData"]):
        spell = object_id(row.get("Disease"))
        immunity = object_id(row.get("ImmunityDuration"))
        diseases.append({"spell": required(spell, "Disease"), "editor_id": editor_id(spell),
                         "immunity_seconds": global_value(immunity) if immunity else 0.0, **symptoms(spell)})

    rows = []
    for row in struct_rows(props["DiseaseChanceData"]):
        kind = CHANCE_TYPES.get(row.get("ChanceType"))
        risk = object_id(row.get("DiseaseRiskForm"))
        disease = object_id(row.get("Disease"))
        if kind is None or not risk or not disease:
            continue
        risk_signature, risk_eid, _ = source.fields(risk)
        entry = {"type": kind, "risk": binding(risk), "risk_editor_id": risk_eid, "risk_signature": risk_signature,
                 "disease": required(disease, "Disease"), "chance": number(row.get("DiseaseChance", DEFAULT_ROW_CHANCE)),
                 "minimum": number(row.get("DiseaseChanceMin", DEFAULT_ROW_MINIMUM)),
                 "prevented_by_worn": binding(object_id(row.get("PreventedByWornKeyword"))),
                 "prevented_by_effect_keyword": binding(object_id(row.get("PreventedByRaceMagicEffectKeyword"))),
                 "required_location": binding(object_id(row.get("RequiredLocation"))),
                 "gained_message": message(object_id(row.get("DiseaseGainedMessage"))),
                 "chance_message": message(object_id(row.get("MessageToShowOnChance"))),
                 "prevented_message": message(object_id(row.get("PreventedByWornMessage"))),
                 "spell_on_chance": binding(object_id(row.get("SpellToCastOnChance")))}
        if kind == "water":
            match = WATER_RISK.match(risk_eid)
            if not match:
                raise ValueError(f"Unrecognised water risk form {risk_eid}")
            entry["tier"], entry["state"] = match.group(1).lower(), match.group(2).lower()
        elif entry["risk"] is None:
            continue
        rows.append(entry)

    # FO76 marks creatures with ActorType keywords on their race. Seven of them (Bloatfly, Bloodbug,
    # Stingwing, RadRoach, RadRat, MutantHound, RadAntSwarm) have no FO4 counterpart, and the
    # conversion maps those races onto FO4's own, which lack the keyword: Tales matches the race.
    races_by_keyword = {}
    for race_raw, _, _, _ in source.rows("RACE", "*"):
        race_keywords = first(source.fields(race_raw)[2], "Keywords") or []
        race = None
        for value in race_keywords:
            keyword_raw = reference_id(value)
            if keyword_raw is None:
                continue
            race = race or binding(race_raw)
            if race is not None and race not in races_by_keyword.setdefault(keyword_raw, []):
                races_by_keyword[keyword_raw].append(race)

    hits = []
    for row in struct_rows(props["DiseasedHitData"]):
        aggressor, disease = object_id(row.get("AggressorFilter")), object_id(row.get("Disease"))
        chance = chance_global(object_id(row.get("DiseaseChance")))
        keyword = binding(aggressor)
        races = races_by_keyword.get(aggressor, [])
        if (keyword is None and not races) or not disease or chance is None:
            continue
        hits.append({"aggressor": keyword, "aggressor_editor_id": editor_id(aggressor), "aggressor_races": races,
                     "disease": required(disease, "Disease"), "chance": chance,
                     "minimum": number(row.get("DiseaseChanceMin", DEFAULT_ROW_MINIMUM))})

    endurance = sorted((int(row["EnduranceValue"]), number(row["DiseaseChanceMult"]))
                       for row in struct_rows(props["EnduraceDiseaseChanceMultData"]))

    multipliers = {name: resolver.resolve("AVIF", eid) for name, eid in CHANCE_MULTIPLIERS.items()}

    vectors, min_player_level = [], None
    for eid, kind in VECTORS.items():
        raw = lookup("MGEF", eid)
        vector = script_properties(source.fields(raw)[2], "SURV_DiseaseVectorEffectScript") or {}
        level = object_id(vector.get("SURV_Disease_MinPlayerLevel"))
        if level:
            min_player_level = global_value(level)
        listed = object_id(vector.get("VectorDiseasesFormlist"))
        vectors.append({"effect": required(raw, "Vector effect"), "editor_id": eid, "kind": kind,
                        "diseases": [required(value, "Vector disease") for value in form_list(listed)] if listed else []})

    cures = []
    for eid, script in CURES.items():
        raw = lookup("MGEF", eid)
        cure = script_properties(source.fields(raw)[2], script) or {}
        severity = cure.get("MaxSeverityLevelToCure")
        cures.append({"effect": required(raw, "Cure effect"), "editor_id": eid,
                      "include_incurable": bool(cure.get("IncludeIncurable", False)),
                      "max_severity": int(severity) if isinstance(severity, int) and not isinstance(severity, bool) else None})

    addiction_cures = []
    for eid in ADDICTION_CURES:
        raw = lookup("MGEF", eid)
        cure = script_properties(source.fields(raw)[2], "SURV_CureAddictionEffectScript") or {}
        addiction_cures.append({"effect": required(raw, "Addiction cure"), "editor_id": eid,
                                "all": bool(cure.get("CureAll", False)), "random": bool(cure.get("CureRandom", False))})

    master = lookup("QUST", "SURV_Master")
    master_fields = source.fields(master)[2]
    addiction = script_properties(master_fields, "SURV_AddictionScript") or {}
    addictions = []
    for row in struct_rows(addiction.get("AddictionData")):
        spell, value = binding(object_id(row.get("AddictionAbility"))), binding(object_id(row.get("AddictionActorValue")))
        if spell:
            addictions.append({"spell": spell, "value": value})

    creature = script_properties(master_fields, "SURV_DiseasedCreatureScript") or {}
    carriers = {"keyword": binding(object_id(creature.get("ActorTypeDiseaseCarrier"))),
                "min_level": number(creature.get("MinCreatureLevelForDisease", DEFAULT_CREATURE_MIN_LEVEL)),
                "ignore_min_level": [value for value in (binding(raw) for raw in form_list(
                    object_id(creature.get("SURV_DiseaseIgnoreMinCreatureLevelKeywords")))) if value],
                "exclude": [value for value in [binding(object_id(creature.get("DailyOpsCreatureKeyword")))] if value],
                "transmission_max": number(props.get("DiseasedCreatureTransmissionChanceMax", 85)),
                "transmission_level_mult": number(props.get("DiseasedCreatureTransmissionChanceLevelMult",
                                                            DEFAULT_TRANSMISSION_LEVEL_MULT)),
                "types": []}
    for row in struct_rows(creature.get("DiseasedCreatureData")):
        actor_type = object_id(row.get("ActorTypeKeyword"))
        keyword, chance = binding(actor_type), chance_global(object_id(row.get("DiseasedCreatureChance")))
        races = races_by_keyword.get(actor_type, []) if actor_type else []
        if (keyword or races) and chance is not None:
            carriers["types"].append({"actor_type": keyword, "races": races, "chance": chance})

    antibiotic_effect = script_properties(source.fields(lookup("MGEF", "AbPerkAntibiotic_ScriptEffect"))[2], "PerkAntibioticScript") or {}
    card = lookup("PCRD", "AntibioticCard")
    antibiotic = {"card": f"SeventySix.esm:{card & 0xFFFFFF:06X}",
                  "chances": [chance_global(object_id(antibiotic_effect.get(name))) or 0.0
                              for name in ("PerkAntibioticChance1", "PerkAntibioticChance2")],
                  "stimpaks": [value for value in (binding(object_id(antibiotic_effect.get(name)))
                                                   for name in ("StimpakTypeWeak", "StimpakTypeNormal", "StimpakTypeSuper")) if value]}

    # SURV_DiseaseAllowed_*: below this level FO76 only lets players catch diseases this far from Vault 76.
    allowed_level, allowed_distance = (object_id(props.get(name)) for name in
                                       ("SURV_DiseaseAllowed_PlayerLevel", "SURV_DiseaseAllowed_DistanceFromVault76"))
    allowed = {"player_level": global_value(allowed_level) if allowed_level else 0.0,
               "distance": global_value(allowed_distance) if allowed_distance else 0.0,
               "vault76_marker": binding(object_id(props.get("Vault76MapMarker")))}

    location_severity = []
    for row in struct_rows(props.get("LocationSeverityData")):
        location = binding(object_id(row.get("LocationAcquired")))
        if location is not None:
            location_severity.append({"location": location, "severity": int(row.get("SeverityLevel", 0))})

    triggers = []
    for raw, _, eid, _ in source.rows("REFR", "*DiseaseTrigger*"):
        trigger_fields = source.fields(raw)[2]
        script = script_properties(trigger_fields, TRIGGER_SCRIPT)
        primitive = first(trigger_fields, "Primitive") or {}
        effect = binding(object_id((script or {}).get("DiseaseToApply")))
        ref = resolver.resolve("REFR", eid)
        if script is None or effect is None or ref is None or primitive.get("Type") != "Box":
            continue
        triggers.append({"ref": ref, "editor_id": eid, "effect": effect,
                         "bounds": [number(primitive[axis]) for axis in ("BoundsX", "BoundsY", "BoundsZ")]})

    # RadAway lowers disease resistance in FO76; the conversion resolves FO76 RadAway onto Fallout4.esm's
    # record, which has no such effect, so Tales applies it from here.
    resistance_items = []
    penalty_effects = {raw for eid in RESISTANCE_PENALTY_EFFECTS for raw, _, found, _ in source.rows("MGEF", eid) if found == eid}
    for raw, _, eid, _ in source.rows("ALCH", "*RadAway*"):
        item_fields = source.fields(raw)[2]
        if resolver.resolve("ALCH", eid) is not None or (item := binding(raw)) is None:
            continue
        for effect, efit in zip(item_fields.get("BaseEffect") or [], item_fields.get("EFIT") or []):
            if reference_id(effect) in penalty_effects and efit.get("Magnitude") and efit.get("Duration"):
                resistance_items.append({"item": item, "editor_id": eid, "resistance": -number(efit["Magnitude"]),
                                         "duration": number(efit["Duration"])})

    sounds = {key: binding(object_id(props.get(name))) for key, name in
              (("contract", "UIHealthDiseaseContract"), ("cure", "UIHealthDiseaseCure"))}

    contact = next((raw for raw, _, found, _ in source.rows("SPEL", DIRTY_WATER_CONTACT) if found == DIRTY_WATER_CONTACT), None)
    global_immunity = object_id(props.get("SURV_Disease_ImmunityDuration_Global"))
    if min_player_level is None:
        raise ValueError("SURV_Disease_MinPlayerLevel missing from the vector effects")
    return {
        "version": 1,
        "min_player_level": min_player_level,
        "global_immunity_seconds": global_value(global_immunity) if global_immunity else 0.0,
        "intervals": dict(DEFAULT_INTERVALS, **{key: number(props[name]) for key, name in
                                                (("water", "TimerDuration_InWater"), ("hazard", "TimerDuration_InHazard")) if name in props}),
        "resistance": resolver.resolve("AVIF", "SURV_DiseaseResistance"),
        "chance_multipliers": multipliers,
        "endurance_multipliers": [{"endurance": value, "mult": mult} for value, mult in endurance],
        "bed_ignore_keyword": binding(object_id(props.get("SURV_BedIgnoreDisease"))),
        "dirty_water_contact": binding(contact),
        "diseases": diseases,
        "chance_rows": rows,
        "hits": hits,
        "carriers": carriers,
        "vectors": vectors,
        "cures": cures,
        "addiction_cures": addiction_cures,
        "addictions": addictions,
        "antibiotic": antibiotic,
        "allowed": allowed,
        "location_severity": location_severity,
        "triggers": triggers,
        "resistance_items": resistance_items,
        "sounds": sounds,
    }


def open_master_resolvers(resolver, data_dir, skip=()):
    """Resolvers for the converted plugin's other masters found in `data_dir` (the DLC: Gulper
    and Angler keywords live in DLCCoast.esm). The caller closes the returned handles."""
    from bacup_lib.reputation_catalog import NativeResolver, plugin_masters

    resolvers, handles = [], []
    for name in resolver.masters:
        path = Path(data_dir) / name
        if name in skip or not path.is_file():
            continue
        handle = resolver.native.plugin_handle_load_index(str(path), game="fo4")
        handles.append(handle)
        resolvers.append(NativeResolver(resolver.native, name, handle, plugin_masters(resolver.native, handle)))
    return resolvers, handles


def emit_catalog(source, resolver, output_mod, number, base_resolver=None, data_dir=None):
    masters, handles = [], []
    if data_dir is not None and hasattr(resolver, "masters"):
        masters, handles = open_master_resolvers(resolver, data_dir, skip={getattr(base_resolver, "plugin", None)})
    try:
        catalog = build_catalog(source, resolver, number, ChainResolver([base_resolver, *masters]))
    finally:
        for handle in reversed(handles):
            resolver.native.plugin_handle_close(handle)
    document = to_document(catalog)
    json.dumps(document, allow_nan=False)
    write_document(FEATURE, resolver.plugin, document, output_mod)
    return catalog


SETTING_KEYS = ("min_player_level", "global_immunity_seconds", "intervals", "resistance", "chance_multipliers",
                "endurance_multipliers", "bed_ignore_keyword", "dirty_water_contact", "antibiotic", "allowed", "sounds")
SECTION_KEYS = ("diseases", "chance_rows", "hits", "vectors", "cures", "addiction_cures", "addictions", "location_severity",
                "triggers", "resistance_items")


def to_document(catalog):
    """Tales layout: globals in `settings`, list rows as keyed sections (the carrier type rows as `carriers`)."""
    carriers = catalog["carriers"]
    settings = {key: catalog[key] for key in SETTING_KEYS}
    settings["carriers"] = {key: value for key, value in carriers.items() if key != "types"}
    document = {"version": catalog["version"], "settings": settings}
    for key in SECTION_KEYS:
        document[key] = catalog[key]
    document["carriers"] = carriers["types"]
    return document


def main(argv=None):
    """Write Diseases.json for an existing conversion without a regen."""
    import argparse

    from bacup_lib.esp_native_runtime import load_esp_native
    from bacup_lib.player_ghoul import Source, number
    from bacup_lib.reputation_catalog import NativeResolver, plugin_masters

    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("source_plugin", type=Path, help="FO76 SeventySix.esm")
    parser.add_argument("converted_plugin", type=Path, help="converted SeventySix.esm")
    parser.add_argument("output_mod", type=Path, help="converted mod folder (receives F4SE/Plugins/...)")
    parser.add_argument("--fallout4", type=Path, help="Fallout4.esm, for forms the conversion leaves in the base game")
    args = parser.parse_args(argv)
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(args.source_plugin), game="fo76")
        handles.append(source)
        converted = native.plugin_handle_load_index(str(args.converted_plugin), game="fo4")
        handles.append(converted)
        base_resolver = None
        if args.fallout4 and args.fallout4.is_file():
            base = native.plugin_handle_load_index(str(args.fallout4), game="fo4")
            handles.append(base)
            base_resolver = NativeResolver(native, args.fallout4.name, base, plugin_masters(native, base))
        catalog = emit_catalog(Source(native, source),
                               NativeResolver(native, args.converted_plugin.name, converted, plugin_masters(native, converted)),
                               args.output_mod, number, base_resolver,
                               data_dir=args.fallout4.parent if args.fallout4 else None)
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    print(f"{args.output_mod}: {len(catalog['diseases'])} diseases, {len(catalog['chance_rows'])} chance rows, "
          f"{len(catalog['hits'])} creature hit rows, {len(catalog['carriers']['types'])} carrier types")


if __name__ == "__main__":
    main()
