import struct

from bacup_lib import player_ghoul as ghoul
from bacup_lib.player_ghoul_native import build_native_rules
from bacup_lib.tests.test_player_ghoul import Source, Resolver, ref


def condition(function, parameter=0, comparison=1, flags=0):
    data = bytearray(32)
    data[0] = flags
    struct.pack_into("<H", data, 8, function)
    struct.pack_into("<I", data, 12, parameter)
    if flags & 4:
        struct.pack_into("<I", data, 4, comparison)
    else:
        struct.pack_into("<f", data, 4, comparison)
    return {"CTDA": {"raw_hex": data.hex()}}


def test_source_glow_threshold_and_spell_duration_are_exported_without_guesses():
    source = Source()
    rads, _ = source.lookup("AVIF", "Rads")
    threshold = source.add("GLOB", "SourceGlowThreshold", [{"Value": 37}])
    data = bytearray(152)
    struct.pack_into("<I", data, 72, rads)
    struct.pack_into("<I", data, 84, 1)
    effect = source.add("MGEF", "SourceGainGlow", [{"Data": {"raw_hex": data.hex()}}])
    spell = source.add("SPEL", "SourceMeleeGlowSpell", [
        {"BaseEffect": ref(effect)}, {"EFIT": {"Magnitude": 19, "Duration": 3}},
        condition(14, rads, threshold, 0x64)])
    perk = source.add("PERK", "GHL_RadReaver01", [
        {"PRKE": {"Type": "EntryPoint"}}, {"EffectData": {"raw_hex": "330A0300"}},
        {"Type": "SpellItem"}, {"EPFD": {"raw_hex": struct.pack("<I", spell).hex()}},
        {"UnknownSpellValue": 3}, {"EndMarker": True}])
    card = {"source": "Source.esm:000F00", "editor_id": "GHL_RadReaverCard", "ranks": [{"source": f"Source.esm:{perk:06X}"}]}
    result = build_native_rules(source, Resolver(), [card], ghoul.condition_data, ghoul.number)
    profile = result["cards"][card["source"]]
    assert profile["kind"] == "rad_reaver"
    assert profile["ranks"][0][0]["spell"] == spell
    assert profile["ranks"][0][0]["recipient"] == 3
    exported = result["records"][str(spell)]["effects"][0]
    assert exported["magnitude"] == 19
    assert exported["duration"] == 3
    assert exported["conditions"][0]["comparison"] == 37
    assert exported["conditions"][0]["flags"] == 0x60
    assert result["records"][str(effect)]["associated"] == 0


def test_nested_conditions_and_explosion_reference_keep_source_identity():
    source = Source()
    perk = source.add("PERK", "GHL_BombScientist01", [])
    predicate = source.add("CNDF", "SourceGlowPredicate", [condition(448, perk)])
    explosion = source.add("EXPL", "SourceGlowExplosion", [])
    data = bytearray(152)
    struct.pack_into("<I", data, 68, 45)
    struct.pack_into("<I", data, 80, explosion)
    effect = source.add("MGEF", "SourceExplosionEffect", [{"Data": {"raw_hex": data.hex()}}])
    spell = source.add("SPEL", "SourceExplosionSpell", [
        {"BaseEffect": ref(effect)}, {"EFIT": {"Magnitude": 1}}, condition(875, predicate)])
    source.records[perk]["fields"] = [
        {"PRKE": {"Type": "Ability"}}, {"EffectData": {"raw_hex": struct.pack("<I", spell).hex()}}, {"EndMarker": True}]
    card = {"source": "Source.esm:000F01", "editor_id": "GHL_BombScientistCard", "ranks": [{"source": f"Source.esm:{perk:06X}"}]}
    result = build_native_rules(source, Resolver(), [card], ghoul.condition_data, ghoul.number)
    assert result["records"][str(predicate)]["conditions"][0]["parameter"] == perk
    assert result["records"][str(effect)]["associated"] == explosion
    assert result["records"][str(explosion)]["signature"] == "EXPL"
    assert result["records"][str(explosion)]["editor_id"] == "SourceGlowExplosion"
