import fnmatch
import json
import struct

import pytest

from bacup_lib.diseases import ChainResolver, build_catalog, emit_catalog
from bacup_lib.tests.catalog_schema_validation import hex_key, validate_catalog
from bacup_lib.player_ghoul import number


def ref(object_id):
    return {"reference": {"plugin": "SeventySix.esm", "object_id": f"{object_id:06X}"}}


def obj(object_id):
    return {"Alias": -1, "FormID": ref(object_id)}


def member(name, value):
    return {"memberName": name, "Value": value}


def prop(name, value):
    return {"propertyName": name, "Value": value}


def vmad(script, *properties):
    return {"Scripts": [{"ScriptName": script, "Properties": list(properties)}]}


class FakeSource:
    def __init__(self):
        self.records = {}
        self.player = []

    def add(self, raw, signature, eid, **fields):
        self.records[raw] = (signature, eid, {key: value if key == "FormID" else [value] for key, value in fields.items()})

    def add_lists(self, raw, signature, eid, fields):
        self.records[raw] = (signature, eid, fields)

    def rows(self, signature, query="*"):
        return [(raw, signature, eid, None) for raw, (sig, eid, _) in self.records.items()
                if sig == signature and fnmatch.fnmatchcase(eid.lower(), query.lower())]

    def fields(self, raw):
        return self.records[raw]

    def record(self, object_id):
        assert object_id == 7
        return {"fields": self.player}


class FakeResolver:
    def __init__(self, missing=(), plugin="SeventySix.esm"):
        self.missing, self.plugin = set(missing), plugin

    def resolve(self, signature, eid):
        return None if eid in self.missing else f"{self.plugin}:{signature}:{eid}"


class OnlyResolver(FakeResolver):
    def __init__(self, known, plugin):
        super().__init__(plugin=plugin)
        self.known = set(known)

    def resolve(self, signature, eid):
        return f"{self.plugin}:{signature}:{eid}" if eid in self.known else None


@pytest.fixture
def source():
    source = FakeSource()
    names = {0x10: ("SPEL", "SURV_Disease_Parasites"), 0x11: ("SPEL", "SURV_Disease_Dysentery"),
             0x12: ("SPEL", "SURV_Disease_SwampItch"), 0x13: ("SPEL", "SURV_Disease_SludgeLung"),
             0x20: ("KYWD", "SURV_DiseaseRiskForm_DirtyWater_Swimming"), 0x21: ("KYWD", "BedTypeSleepingBag"),
             0x22: ("KYWD", "SURV_DiseasePrevention_InWater"), 0x23: ("LCTN", "RegionSwampForestLocation"),
             0x24: ("MGEF", "SURV_DiseaseHazardEffect_ToxicSmoke"), 0x25: ("KYWD", "ActorTypeBloodbug"),
             0x26: ("KYWD", "ActorTypeDiseaseCarrier"), 0x27: ("KYWD", "ActorTypeGulper"),
             0x28: ("KYWD", "SURV_BedIgnoreDisease"), 0x29: ("KYWD", "DailyOpsEncounterKeyword"),
             0x2A: ("SPEL", "AbAddictionBuffout"), 0x2B: ("AVIF", "AddictionBuffout"),
             0x2C: ("ALCH", "Stimpak"), 0x2D: ("PERK", "Antibiotic02"), 0x2E: ("SPEL", "WaterRadiationHazard")}
    for raw, (signature, eid) in names.items():
        source.add(raw, signature, eid)
    for raw, eid, value in [(0x30, "SURV_Disease_ImmunityDuration_Specific_Standard", 2700.0),
                            (0x31, "SURV_Disease_ImmunityDuration_Global", 1800.0),
                            (0x32, "SURV_DiseaseChance_Combat_High", 15.0), (0x33, "SURV_DiseasedCreatureChance_High", 30.0),
                            (0x34, "SURV_Disease_MinPlayerLevel", 4.0), (0x35, "PerkAntibioticChance1", 25.0),
                            (0x36, "PerkAntibioticChance2", 50.0)]:
        source.add(raw, "GLOB", eid, Value=value)
    source.add(0x40, "MESG", "SURV_DiseaseGained_Msg_InWater",
               Description={"TargetLanguage": "English", "Values": [{"Language": "English", "String": "Waterborne."}]})
    source.add(0x41, "FLST", "SURV_VectorDiseases_Food", FormID=[ref(0x10), ref(0x11)])
    source.add(0x42, "FLST", "SURV_DiseaseIgnoreMinCreatureLevelKeywords", FormID=[ref(0x25)])
    vector = lambda flag: vmad("SURV_DiseaseVectorEffectScript", prop("VectorDiseasesFormlist", obj(0x41)),
                               prop("SURV_Disease_MinPlayerLevel", obj(0x34)), prop(flag, True))
    source.add(0x50, "MGEF", "SURV_DiseaseVector_Food_Effect", VirtualMachineAdapter=vector("IsFoodDisease"))
    source.add(0x51, "MGEF", "SURV_DiseaseVector_Drink_Effect", VirtualMachineAdapter=vector("IsDrinkDisease"))
    source.add(0x52, "MGEF", "SURV_DiseaseVector_Drink_Effect_WaterWitchPerk", VirtualMachineAdapter=vector("IsDrinkDisease"))
    source.add(0x53, "MGEF", "SURV_DiseaseCure_Potion_Effect",
               VirtualMachineAdapter=vmad("SURV_DiseaseCureEffectScript", prop("MaxSeverityLevelToCure", 4)))
    source.add(0x54, "MGEF", "SURV_DiseaseCureAll_Potion_Effect", VirtualMachineAdapter=vmad("SURV_DiseaseCureAllEffectScript"))
    source.add(0x55, "MGEF", "SURV_CureAddictionAll", VirtualMachineAdapter=vmad("SURV_CureAddictionEffectScript", prop("CureAll", True)))
    source.add(0x56, "MGEF", "SURV_CureAddictionRandom",
               VirtualMachineAdapter=vmad("SURV_CureAddictionEffectScript", prop("CureRandom", True)))
    source.add(0x57, "MGEF", "AbPerkAntibiotic_ScriptEffect", VirtualMachineAdapter=vmad(
        "PerkAntibioticScript", prop("PerkAntibioticChance1", obj(0x35)), prop("PerkAntibioticChance2", obj(0x36)),
        prop("StimpakTypeNormal", obj(0x2C)), prop("Antibiotic02", obj(0x2D))))
    source.add(0x3D295E, "PCRD", "AntibioticCard")
    source.add(0x60, "QUST", "SURV_Master", VirtualMachineAdapter={"Scripts": [
        {"ScriptName": "SURV_AddictionScript", "Properties": [prop("AddictionData", [
            [member("AddictionAbility", obj(0x2A)), member("AddictionActorValue", obj(0x2B))]])]},
        {"ScriptName": "SURV_DiseasedCreatureScript", "Properties": [
            prop("ActorTypeDiseaseCarrier", obj(0x26)), prop("DailyOpsCreatureKeyword", obj(0x29)),
            prop("SURV_DiseaseIgnoreMinCreatureLevelKeywords", obj(0x42)),
            prop("DiseasedCreatureData", [[member("ActorTypeKeyword", obj(0x25)), member("DiseasedCreatureChance", obj(0x33))]])]}]})
    def text(raw, eid, value):
        source.add(raw, "MESG", eid, Description={"TargetLanguage": "English", "Values": [{"Language": "English", "String": value}]})

    # Parasites: a periodic effect (start/repeat/finish messages, a food-loss spell) and an on-hit spell.
    source.add_lists(0x10, "SPEL", "SURV_Disease_Parasites", {"BaseEffect": [ref(0x70)]})
    source.add_lists(0x70, "MGEF", "SURV_Disease_Effect_Parasites", {"VirtualMachineAdapter": [{"Scripts": [
        {"ScriptName": "SURV_DiseasePeriodicEffectScript", "Properties": [
            prop("TimerData", [[member("IntervalGlobal", obj(0x74)), member("ID", 0)]]),
            prop("PeriodicEffectData", [[member("FireOnStart", True), member("MessageToDisplay", obj(0x75))],
                                        [member("FireOnTimerID", 0), member("MessageToDisplay", obj(0x76)),
                                         member("SpellToCast", obj(0x77))]]),
            prop("MessageToDisplayOnFinish", obj(0x78))]},
        {"ScriptName": "SURV_DiseaseOnHitEffectScript", "Properties": [prop("SpellToCastOnTargetWhenHit", obj(0x79))]}]}]})
    source.add(0x74, "GLOB", "SURV_Disease_PeriodicEffect_Interval_Normal", Value=90.0)
    text(0x75, "SURV_Disease_Msg_Parasites_Initial", "You have contracted Parasites")
    text(0x76, "SURV_Disease_Msg_Parasites_Repeated", "Suffering food loss from Parasites")
    text(0x78, "SURV_Disease_Msg_Parasites_Cured", "You've recovered from Parasites!")
    source.add(0x77, "SPEL", "SURV_Disease_Parasites_PerioidicFoodLoss")
    source.add(0x79, "SPEL", "SURV_Disease_WeepingSores_OnHit")
    # Sludge Lung: its effect applies a perk whose FO76-only entry point casts a spell when hit.
    data = bytearray(160)
    struct.pack_into("<I", data, 140, 0x72)
    source.add_lists(0x13, "SPEL", "SURV_Disease_SludgeLung", {"BaseEffect": [ref(0x71)]})
    source.add(0x71, "MGEF", "SURV_Disease_Effect_SludgeLung", Data={"raw_hex": data.hex().upper()})
    source.add_lists(0x72, "PERK", "SURV_Disease_GlowingPustules_Perk",
                     {"EffectData": [{"raw_hex": "010101"}, {"raw_hex": "C10A0400"}], "EPFD": [{"raw_hex": "73000000"}]})
    source.add(0x73, "SPEL", "SURV_Disease_GlowingPustules_OnHit")
    text(0x7A, "SURV_DiseaseChance_Msg_ShowOnChance_ToxicAir", "Suffering damage and risking lung disease from Toxic Air.")
    source.add(0x7B, "SPEL", "SURV_Disease_SpellToCastOnChance_ToxicAir_DamageHealth")
    text(0x7C, "SURV_DiseasePrevented_Msg_Inhalation", "Your gear has protected you from lung disease.")
    source.add(0x80, "GLOB", "SURV_DiseaseAllowed_PlayerLevel", Value=15.0)
    source.add(0x81, "GLOB", "SURV_DiseaseAllowed_DistanceFromVault76", Value=250000.0)
    source.add(0x82, "REFR", "Vault76MapMarker")
    source.add(0x83, "SNDR", "UIHealthDiseaseContract")
    source.add(0x84, "SNDR", "UIHealthDiseaseCure")
    source.add_lists(0x90, "REFR", "MTR06_DiseaseTrigger", {
        "VirtualMachineAdapter": [vmad("DefaultApplyDiseaseOnTriggerEnter", prop("DiseaseToApply", obj(0x24)))],
        "Primitive": [{"BoundsX": 100.0, "BoundsY": 200.0, "BoundsZ": 50.0, "Type": "Box"}]})
    source.add_lists(0x91, "ALCH", "RadAway", {"BaseEffect": [ref(0x92), ref(0x93)],
                                               "EFIT": [{"Magnitude": 50.0, "Duration": 900}, {"Magnitude": 60.0}]})
    source.add_lists(0x94, "ALCH", "RadAwayDiluted", {"BaseEffect": [ref(0x92)], "EFIT": [{"Magnitude": 50.0, "Duration": 420}]})
    source.add(0x92, "MGEF", "SURV_ReduceDiseaseResistance_Potion_Effect")
    source.add(0x93, "MGEF", "RestoreRads")
    source.add_lists(0x95, "RACE", "BloodbugRace", {"Keywords": [[ref(0x25), ref(0x26)]]})
    source.player = [{"VirtualMachineAdapter": vmad(
        "SURV_PlayerDiseaseScript",
        prop("DiseaseData", [[member("Disease", obj(0x10)), member("ImmunityDuration", obj(0x30))],
                             [member("Disease", obj(0x11))], [member("Disease", obj(0x13))]]),
        prop("DiseaseChanceData", [
            [member("ChanceType", 5), member("DiseaseRiskForm", obj(0x20)), member("Disease", obj(0x10)),
             member("DiseaseChance", 5), member("PreventedByWornKeyword", obj(0x22)), member("DiseaseGainedMessage", obj(0x40))],
            [member("ChanceType", 4), member("DiseaseRiskForm", obj(0x21)), member("Disease", obj(0x12)),
             member("DiseaseChance", 30)],
            [member("ChanceType", 1), member("DiseaseRiskForm", obj(0x24)), member("Disease", obj(0x13)),
             member("MessageToShowOnChance", obj(0x7A)), member("SpellToCastOnChance", obj(0x7B)),
             member("PreventedByWornMessage", obj(0x7C))]]),
        prop("DiseasedHitData", [
            [member("AggressorFilter", obj(0x25)), member("DiseaseChance", obj(0x32)), member("Disease", obj(0x10))],
            [member("AggressorFilter", obj(0x27)), member("DiseaseChance", obj(0x32)), member("Disease", obj(0x10))]]),
        prop("EnduraceDiseaseChanceMultData", [[member("EnduranceValue", 2), member("DiseaseChanceMult", 1.9)],
                                               [member("EnduranceValue", 1), member("DiseaseChanceMult", 2.0)]]),
        prop("SURV_Disease_ImmunityDuration_Global", obj(0x31)),
        prop("SURV_BedIgnoreDisease", obj(0x28)),
        prop("DiseasedCreatureTransmissionChanceMax", 85),
        prop("SURV_DiseaseAllowed_PlayerLevel", obj(0x80)),
        prop("SURV_DiseaseAllowed_DistanceFromVault76", obj(0x81)),
        prop("Vault76MapMarker", obj(0x82)),
        prop("LocationSeverityData", [[member("LocationAcquired", obj(0x23)), member("SeverityLevel", 4)]]),
        prop("UIHealthDiseaseContract", obj(0x83)),
        prop("UIHealthDiseaseCure", obj(0x84)))}]
    return source


def test_player_tables(source):
    catalog = build_catalog(source, FakeResolver(missing={"ActorTypeGulper"}), number)
    assert catalog["version"] == 1
    assert catalog["min_player_level"] == 4
    assert catalog["global_immunity_seconds"] == 1800
    assert catalog["intervals"] == {"water": 10.0, "hazard": 10.0}
    assert [(row["spell"], row["editor_id"], row["immunity_seconds"]) for row in catalog["diseases"]] == [
        ("SeventySix.esm:SPEL:SURV_Disease_Parasites", "SURV_Disease_Parasites", 2700.0),
        ("SeventySix.esm:SPEL:SURV_Disease_Dysentery", "SURV_Disease_Dysentery", 0.0),
        ("SeventySix.esm:SPEL:SURV_Disease_SludgeLung", "SURV_Disease_SludgeLung", 0.0)]
    assert [row["mult"] for row in catalog["endurance_multipliers"]] == [2.0, 1.9]
    water, bed, hazard = catalog["chance_rows"]
    assert (water["type"], water["tier"], water["state"], water["chance"], water["minimum"]) == ("water", "dirty", "swimming", 5, 1)
    assert water["prevented_by_worn"] == "SeventySix.esm:KYWD:SURV_DiseasePrevention_InWater"
    assert water["gained_message"] == "Waterborne."
    assert (bed["type"], bed["risk_signature"], bed["chance"]) == ("bed", "KYWD", 30)
    assert (hazard["type"], hazard["chance"]) == ("hazard", 5)  # struct default
    assert catalog["bed_ignore_keyword"] == "SeventySix.esm:KYWD:SURV_BedIgnoreDisease"
    assert catalog["dirty_water_contact"] == "SeventySix.esm:SPEL:WaterRadiationHazard"


def test_hits_drop_unconverted_aggressors(source):
    catalog = build_catalog(source, FakeResolver(missing={"ActorTypeGulper"}), number)
    assert catalog["hits"] == [{"aggressor": "SeventySix.esm:KYWD:ActorTypeBloodbug", "aggressor_editor_id": "ActorTypeBloodbug",
                                "aggressor_races": ["SeventySix.esm:RACE:BloodbugRace"],
                                "disease": "SeventySix.esm:SPEL:SURV_Disease_Parasites", "chance": 15.0, "minimum": 1.0}]


def test_carriers_vectors_and_cures(source):
    catalog = build_catalog(source, FakeResolver(), number)
    carriers = catalog["carriers"]
    assert carriers["types"] == [{"actor_type": "SeventySix.esm:KYWD:ActorTypeBloodbug",
                                  "races": ["SeventySix.esm:RACE:BloodbugRace"], "chance": 30.0}]
    assert carriers["min_level"] == 10 and carriers["transmission_max"] == 85
    assert carriers["ignore_min_level"] == ["SeventySix.esm:KYWD:ActorTypeBloodbug"]
    assert carriers["exclude"] == ["SeventySix.esm:KYWD:DailyOpsEncounterKeyword"]
    assert [(row["kind"], len(row["diseases"])) for row in catalog["vectors"]] == [("food", 2), ("drink", 2), ("drink", 2)]
    assert [row["editor_id"] for row in catalog["cures"]] == ["SURV_DiseaseCure_Potion_Effect", "SURV_DiseaseCureAll_Potion_Effect"]
    assert [(row["all"], row["random"]) for row in catalog["addiction_cures"]] == [(True, False), (False, True)]
    assert catalog["addictions"] == [{"spell": "SeventySix.esm:SPEL:AbAddictionBuffout", "value": "SeventySix.esm:AVIF:AddictionBuffout"}]
    assert catalog["antibiotic"] == {"card": "SeventySix.esm:3D295E", "chances": [25.0, 50.0],
                                     "stimpaks": ["SeventySix.esm:ALCH:Stimpak"]}


def test_missing_disease_spell_is_an_error(source):
    with pytest.raises(ValueError, match="SURV_Disease_Parasites"):
        build_catalog(source, FakeResolver(missing={"SURV_Disease_Parasites"}), number)


def test_base_resolver_fills_base_game_forms(source):
    class Base(FakeResolver):
        def resolve(self, signature, eid):
            return f"Fallout4.esm:{eid}"

    catalog = build_catalog(source, FakeResolver(missing={"WaterRadiationHazard"}), number, Base())
    assert catalog["dirty_water_contact"] == "Fallout4.esm:WaterRadiationHazard"


def test_emit_writes_json(source, tmp_path):
    emit_catalog(source, FakeResolver(), tmp_path, number)
    written = json.loads((tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/Diseases/SeventySix.esm.json").read_text(encoding="utf-8"))
    assert written["version"] == 1 and len(written["diseases"]) == 3
    assert "types" not in written["settings"]["carriers"] and written["carriers"]


class HexResolver(FakeResolver):
    def resolve(self, signature, eid):
        return None if eid in self.missing else hex_key(self.plugin, eid)


def test_emitted_document_matches_its_schema(source, tmp_path):
    emit_catalog(source, HexResolver(), tmp_path, number)
    path = tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/Diseases/SeventySix.esm.json"
    validate_catalog("Diseases", json.loads(path.read_text(encoding="utf-8")))


def test_dlc_master_keywords_fill_hit_rows(source):
    # The conversion maps FO76's ActorTypeGulper onto DLCCoast.esm's keyword of the same EditorID.
    dlc = OnlyResolver({"ActorTypeGulper"}, "DLCCoast.esm")
    catalog = build_catalog(source, FakeResolver(missing={"ActorTypeGulper"}), number, ChainResolver([None, dlc]))
    assert [row["aggressor"] for row in catalog["hits"]] == ["SeventySix.esm:KYWD:ActorTypeBloodbug", "DLCCoast.esm:KYWD:ActorTypeGulper"]
    assert all(row["minimum"] == 1 for row in catalog["hits"])


def test_actor_types_fo4_lacks_match_by_race(source):
    # FO4 has no ActorTypeBloodbug: the conversion maps BloodbugRace onto Fallout4.esm's race, which
    # lacks the keyword, so the row must carry the race for Tales to match.
    catalog = build_catalog(source, FakeResolver(missing={"ActorTypeBloodbug", "BloodbugRace"}), number,
                            OnlyResolver({"BloodbugRace"}, "Fallout4.esm"))
    bloodbug = next(row for row in catalog["hits"] if row["aggressor_editor_id"] == "ActorTypeBloodbug")
    assert bloodbug["aggressor"] is None and bloodbug["aggressor_races"] == ["Fallout4.esm:RACE:BloodbugRace"]
    assert catalog["carriers"]["types"] == [{"actor_type": None, "races": ["Fallout4.esm:RACE:BloodbugRace"], "chance": 30.0}]


def test_chain_resolver_prefers_the_first_that_knows():
    chain = ChainResolver([None, OnlyResolver({"A"}, "Fallout4.esm"), OnlyResolver({"A", "B"}, "DLCCoast.esm")])
    assert chain.resolve("KYWD", "A") == "Fallout4.esm:KYWD:A"
    assert chain.resolve("KYWD", "B") == "DLCCoast.esm:KYWD:B"
    assert chain.resolve("KYWD", "C") is None


def test_disease_symptoms(source):
    parasites, dysentery, sludge = build_catalog(source, FakeResolver(), number)["diseases"]
    assert parasites["timers"] == [{"id": 0, "interval": 90.0}]
    assert parasites["periodic"] == [
        {"on_start": True, "timer": -1, "message": "You have contracted Parasites", "spell": None},
        {"on_start": False, "timer": 0, "message": "Suffering food loss from Parasites",
         "spell": "SeventySix.esm:SPEL:SURV_Disease_Parasites_PerioidicFoodLoss"}]
    assert parasites["finish_message"] == "You've recovered from Parasites!"
    assert parasites["on_hit"] == [{"spell": "SeventySix.esm:SPEL:SURV_Disease_WeepingSores_OnHit", "dispel": True}]
    assert (dysentery["timers"], dysentery["periodic"], dysentery["on_hit"]) == ([], [], [])
    # FO76 perk entry point 193 (SelectSpell): Tales casts the spell when the player is hit.
    assert sludge["on_hit"] == [{"spell": "SeventySix.esm:SPEL:SURV_Disease_GlowingPustules_OnHit", "dispel": False}]


def test_toxic_air_row_messages_and_spell(source):
    hazard = next(row for row in build_catalog(source, FakeResolver(), number)["chance_rows"] if row["type"] == "hazard")
    assert hazard["chance_message"] == "Suffering damage and risking lung disease from Toxic Air."
    assert hazard["prevented_message"] == "Your gear has protected you from lung disease."
    assert hazard["spell_on_chance"] == "SeventySix.esm:SPEL:SURV_Disease_SpellToCastOnChance_ToxicAir_DamageHealth"
    assert hazard["disease"] == "SeventySix.esm:SPEL:SURV_Disease_SludgeLung"


def test_grace_severity_triggers_and_sounds(source):
    catalog = build_catalog(source, FakeResolver(), number)
    assert catalog["allowed"] == {"player_level": 15.0, "distance": 250000.0, "vault76_marker": "SeventySix.esm:REFR:Vault76MapMarker"}
    assert catalog["location_severity"] == [{"location": "SeventySix.esm:LCTN:RegionSwampForestLocation", "severity": 4}]
    assert catalog["triggers"] == [{"ref": "SeventySix.esm:REFR:MTR06_DiseaseTrigger", "editor_id": "MTR06_DiseaseTrigger",
                                    "effect": "SeventySix.esm:MGEF:SURV_DiseaseHazardEffect_ToxicSmoke", "bounds": [100.0, 200.0, 50.0]}]
    assert catalog["sounds"] == {"contract": "SeventySix.esm:SNDR:UIHealthDiseaseContract", "cure": "SeventySix.esm:SNDR:UIHealthDiseaseCure"}
    assert catalog["carriers"]["transmission_level_mult"] == pytest.approx(0.1)
    assert [row["max_severity"] for row in catalog["cures"]] == [4, None]


def test_radaway_penalty_only_for_items_the_conversion_left_on_fallout4(source):
    base = OnlyResolver({"RadAway"}, "Fallout4.esm")
    catalog = build_catalog(source, FakeResolver(missing={"RadAway"}), number, ChainResolver([base]))
    assert catalog["resistance_items"] == [{"item": "Fallout4.esm:ALCH:RadAway", "editor_id": "RadAway",
                                            "resistance": -50.0, "duration": 900.0}]
