import copy
import struct
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import player_ghoul as ghoul


def ref(value):
    return {"reference": {"plugin": "SeventySix.esm", "object_id": f"{value:06X}"}}


def perk(entry, value, function=1):
    return [{"PRKE": {"Type": "EntryPoint"}},
            {"EffectData": {"raw_hex": bytes([entry, function, 1, 0]).hex()}},
            {"EPFD": {"raw_hex": struct.pack("<f", value).hex()}}, {"EndMarker": True}]


class Source:
    def __init__(self, fraction=.35):
        self.records = {}
        self.add("PERK", "GHL_PlayerPerk", perk(202, fraction) + perk(203, fraction, 3))
        self.add("AVIF", "GHL_SURV_Feral", [{"MaximumValue": 7200}])
        for name, value in zip(ghoul.THRESHOLDS, [1800, 3600, 5400, 7200]):
            self.add("GLOB", name, [{"Value": value}])
        feral = self.add("MGEF", "GHL_SURV_Feral_Effect", [])
        strength = self.add("MGEF", "AbFortifyStrength", [])
        self.add("SPEL", "GHL_SURV_Feral_Ability", [
            {"BaseEffect": ref(feral)}, {"EFIT": {"Magnitude": 1}},
            {"BaseEffect": ref(feral)}, {"EFIT": {"Magnitude": .5}},
            {"BaseEffect": ref(strength)}, {"EFIT": {"Magnitude": 3}}])
        evade = self.add("MGEF", "AbPerkFortifyEvadeChance", [])
        rads = self.add("AVIF", "Rads", [])
        armor = self.add("KYWD", "ArmorTypePower", [])
        hyper_perks = [self.add("PERK", f"GHL_HyperReflexes0{i}", []) for i in range(1, 4)]
        hyper = []
        for index, chance in enumerate([10, 15, 20]):
            hyper.extend([{"BaseEffect": ref(evade)}, {"EFIT": {"Magnitude": chance}},
                          {"CTDA": {"Function": 14, "Type": 0x60, "ComparisonValue": {"value": 180},
                                    "Parameter1": {"variant": "actor_value", "value": {
                                        "ActorValueActorValue": {"variant": "actor_value", "value": rads}}}}},
                          {"CTDA": {"Function": 10017, "ComparisonValue": {"value": 1}}},
                          {"CTDA": {"Function": 682, "Parameter1": {"value": ref(armor)}}}])
            if index < 2:
                hyper.append({"CTDA": {"Function": 448, "Parameter1": {"value": ref(hyper_perks[index + 1])}}})
            if index > 0:
                hyper.append({"CTDA": {"Function": 448, "ComparisonValue": {"value": 1}, "Parameter1": {"value": ref(hyper_perks[index])}}})
        self.add("SPEL", "GHL_AbPerkHyperReflexes", hyper)
        chem = self.add("MGEF", "GHL_SURV_Chem_Effect", [])
        recovery = self.add("GLOB", "ChemRecovery", [{"Value": 240}])
        self.add("ALCH", "TestChem", [{"BaseEffect": ref(chem)}, {"EFIT": {}}, {"Magnitude": ref(recovery)}])
        diet = []
        for rank, value in enumerate([1.25, 1.5, 1.75], 1):
            diet.append(self.add("PERK", f"GHL_ChemDiet0{rank}", perk(204, value)))
        self.add("PCRD", "GHL_ChemDietCard", [
            {"Unknown": {"RaceRestriction": "Ghoul", "Special": "Endurance", "MinLevel": 50}},
            {"MaleName": "ChemDiet"}, *[{"CardRankCost": i} for i in [1, 2, 3]],
            *[{"MalePerk": ref(value)} for value in diet]])

    def add(self, signature, eid, fields):
        raw = len(self.records) + 1
        self.records[raw] = {"signature": signature, "eid": eid, "fields": fields}
        return raw

    def record(self, raw):
        return self.records[raw]

    def fields(self, raw):
        record = self.record(raw)
        fields = {}
        for field in record["fields"]:
            for key, value in field.items():
                fields.setdefault(key, []).append(value)
        return record["signature"], record["eid"], fields

    def lookup(self, signature, eid):
        raw = next(raw for raw, record in self.records.items() if record["signature"] == signature and record["eid"] == eid)
        return raw, self.fields(raw)[2]

    def rows(self, signature, query="*"):
        return [(raw, signature, record["eid"], "") for raw, record in self.records.items() if record["signature"] == signature]


class Resolver:
    def __init__(self, missing=(), plugin="Converted.esm"):
        self.missing, self.plugin = missing, plugin

    def resolve(self, signature, eid):
        return None if eid in self.missing else f"{self.plugin}:000800"


@pytest.mark.parametrize("fraction", [.15, .35])
def test_patch_version_is_read_from_player_perk(fraction):
    result = ghoul.build_catalog(Source(fraction), Resolver(), "Source.esm", "digest")
    assert result["rules"]["radiation_fraction"] == pytest.approx(fraction)
    assert result["rules"]["thresholds"] == [1800, 3600, 5400, 7200]
    assert result["modifiers"][0][0] == 3
    assert result["chems"][0]["recovery"] == 240
    assert [rank["multiplier"] for rank in result["chem_diet"]] == [1.25, 1.5, 1.75]
    assert len(result["cards"][0]["ranks"]) == 3


def test_missing_form_disables_controller_and_no_appearance_assets_are_required():
    result = ghoul.build_catalog(Source(), Resolver(["GHL_ActorTypePlayerGhoul"]), "Source.esm", "digest")
    assert not result["available"]
    assert result["appearance"] == []
    assert result["appearance_status"].endswith("asset-preflight")
    assert ghoul.OUTPUT.as_posix().startswith("F4SE/Plugins/")


def test_evasion_rules_use_source_magnitudes_and_reject_changed_predicates():
    source = Source()
    raw, _ = source.lookup("SPEL", "GHL_AbPerkHyperReflexes")
    source.records[raw]["fields"][1]["EFIT"]["Magnitude"] = 12
    result = ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")
    assert result["hyper_reflexes"] == {"chances": [.12, .15, .20], "glow_threshold": 180}
    source.records[raw]["fields"][4]["CTDA"]["ComparisonValue"] = {"value": 1}
    with pytest.raises(ValueError, match="eligibility changed"):
        ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")


def test_base_game_chem_alias_keeps_source_recovery():
    result = ghoul.build_catalog(Source(), Resolver(["TestChem"]), "Source.esm", "digest", Resolver(plugin="Fallout4.esm"))
    assert result["chems"] == [{"form": "Fallout4.esm:000800", "recovery": 240, "editor_id": "TestChem"}]


def test_radiation_entry_conditions_are_inventory_not_actor_immunity():
    source = Source()
    effect = source.add("MGEF", "DamageRadiationEating", [])
    keyword = source.add("KYWD", "GHL_RadiationBonusEffect", [])
    data = bytearray(32)
    struct.pack_into("<H", data, 8, 854)
    struct.pack_into("<I", data, 12, effect)
    raw, _ = source.lookup("PERK", "GHL_PlayerPerk")
    source.records[raw]["fields"].insert(2, {"CTDA": {"raw_hex": data.hex()}})
    source.records[raw]["fields"].insert(3, {"CTDA": {"Function": 699,
        "ComparisonValue": {"value": 0}, "Parameter1": {"value": ref(keyword)}}})
    result = ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")
    assert [(row["entry"], row["function"], row["parameter_editor_id"]) for row in result["source_radiation_conditions"]] == [
        (202, 854, "DamageRadiationEating"), (202, 699, "GHL_RadiationBonusEffect")]
    assert "radiation_blockers" not in result
    assert ghoul.build_catalog(source, Resolver(["DamageRadiationEating"]), "Source.esm", "digest")["available"]
    source.records[raw]["fields"][3]["CTDA"]["ComparisonValue"]["value"] = 1
    with pytest.raises(ValueError, match="radiation condition"):
        ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")


def test_sparse_record_zero_special_decodes_as_strength():
    source = Source()
    raw, _ = source.lookup("PCRD", "GHL_ChemDietCard")
    del source.records[raw]["fields"][0]["Unknown"]["Special"]
    assert ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")["cards"][0]["special"] == "Strength"


def test_baseline_charisma_and_exposure_resistance_apply_to_every_tier():
    source = Source()
    for effect, ability, magnitude in [("abReduceCharisma", "GHL_SURV_AbGhoulCharisma", 10),
                                       ("GHL_AbPerkFortifyRadiationExpose", "GHL_abPerkBaseRadResist", 50)]:
        raw = source.add("MGEF", effect, [])
        source.add("SPEL", ability, [{"BaseEffect": ref(raw)}, {"EFIT": {"Magnitude": magnitude}}])
    result = ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")
    assert all(tier[4] == -10 and tier[6] == -50 for tier in result["modifiers"])


def test_ambiguous_radiation_entries_do_not_silently_use_an_old_value():
    source = Source()
    raw, _ = source.lookup("PERK", "GHL_PlayerPerk")
    source.records[raw]["fields"] = perk(202, .15) + perk(203, .35, 3)
    with pytest.raises(ValueError, match="radiation conversion"):
        ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")


@pytest.mark.parametrize("value", [float("nan"), float("inf"), True])
def test_invalid_numeric_rules_rejected(value):
    with pytest.raises(ValueError):
        ghoul.number(value)


def test_raw_and_decoded_threshold_conditions_select_same_tier():
    source = Source()
    threshold, _ = source.lookup("GLOB", ghoul.THRESHOLDS[1])
    ability, _ = source.lookup("SPEL", "GHL_SURV_Feral_Ability")
    data = bytearray(32); data[0] = 0x64
    struct.pack_into("<I", data, 4, threshold)
    source.records[ability]["fields"].append({"CTDA": {"raw_hex": data.hex()}})
    raw = ghoul.build_catalog(source, Resolver(), "Source.esm", "digest")
    decoded = copy.deepcopy(source)
    decoded.records[ability]["fields"][-1] = {"CTDA": {"Type": 0x64, "ComparisonValue": {"value": ref(threshold)}}}
    assert ghoul.build_catalog(decoded, Resolver(), "Source.esm", "digest")["modifiers"] == raw["modifiers"]
    assert raw["modifiers"][2][0] == 3
    assert raw["modifiers"][0][0] == 0


def test_conversion_phase_uses_loose_xse_root_and_is_pair_gated(monkeypatch, tmp_path):
    from bacup_lib.workflows.unified import _convert_fo76_player_ghoul
    calls = []
    monkeypatch.setattr(ghoul, "emit_player_ghoul", lambda *args: calls.append(args) or {"cards": [], "chems": []})
    request = SimpleNamespace(source_game="fo76", target_game="fo4", target_data_dir=str(tmp_path / "target"))
    ctx = SimpleNamespace(mod_path=str(tmp_path / "SeventySix"), output_plugin_name="SeventySix.esm",
                          source_plugin_path=str(tmp_path / "source.esm"))
    assert _convert_fo76_player_ghoul(request, ctx) == 1
    assert calls[0] == (Path(ctx.source_plugin_path), Path(ctx.mod_path) / ctx.output_plugin_name,
                        Path(ctx.mod_path), Path(request.target_data_dir))
    request.source_game = "skyrimse"
    assert _convert_fo76_player_ghoul(request, ctx) == 0
    assert len(calls) == 1
