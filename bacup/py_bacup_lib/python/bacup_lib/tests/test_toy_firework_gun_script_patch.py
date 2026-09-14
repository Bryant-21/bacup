from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex, parse_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ESM = Path("N:/Steam Games/steamapps/common/Fallout76/Data/SeventySix.esm")
LIVE_ESM = REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm"
SOURCE_PEX = (
    REPO_ROOT / "extracted" / "fo76" / "scripts" / "client" / "toyfireworkgunscript.pex"
)
LIVE_PEX = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / "toyfireworkgunscript.pex"
)
SCRIPT_NAME = "ToyFireworkGunScript"
SKELETON = """Scriptname ToyFireworkGunScript Extends ObjectReference

actor myWielder

form Property myAmmo Auto
sound Property BreakSFX Auto
message Property BreakMSG Auto
"""

WEAPON_AMMO = {
    0x64A56F: 0x64A567,
    0x64A570: 0x64A568,
    0x64A571: 0x64A569,
    0x64A572: 0x64A56A,
    0x64A573: 0x64A56B,
    0x64A574: 0x64A56C,
    0x64A575: 0x64A56D,
}
WEAPON_EDITOR_IDS = {
    0x64A56F: "ToyFireworkLauncher_Gold",
    0x64A570: "ToyFireworkLauncher_Silver",
    0x64A571: "ToyFireworkLauncher_Blue",
    0x64A572: "ToyFireworkLauncher_Green",
    0x64A573: "ToyFireworkLauncher_Pink",
    0x64A574: "ToyFireworkLauncher_Red",
    0x64A575: "ToyFireworkLauncher_Yellow",
}
AMMO_PROJECTILE = {
    0x64A567: 0x11202B,
    0x64A568: 0x111FCB,
    0x64A569: 0x111FC4,
    0x64A56A: 0x111C00,
    0x64A56B: 0x111FC5,
    0x64A56C: 0x111D5C,
    0x64A56D: 0x111FC6,
}
PROJECTILE_EXPLOSION_TIMER = {
    0x11202B: (0x11202A, 2.0),
    0x111FCB: (0x112028, 2.0),
    0x111FC4: (0x111FD9, 3.5),
    0x111C00: (0x111BF0, 3.5),
    0x111FC5: (0x111FD8, 3.5),
    0x111D5C: (0x111D59, 3.5),
    0x111FC6: (0x111FD7, 3.5),
}
DURABILITY_CURVES = {
    0x1FABB9: "Durability_Weapons_Melee_Min",
    0x50A606: "ConditionDamageScaleFactor_Weapons_Ballistic_SixShot",
    0x1FABBA: "SecondaryScaleFactor_Weapons_Ranged_Bash",
    0x345A4C: "Durability_Weapons_Melee_Max",
}


def _field(record: dict, name: str):
    for entry in record["fields"]:
        if name in entry:
            return entry[name]
    return None


def _form_id(value: dict) -> int:
    return int(value["reference"]["object_id"], 16)


def _read_records(
    plugin: Path, game: str, form_ids: list[int]
) -> tuple[dict, list[str]]:
    result = subprocess.run(
        [
            "modkit.exe",
            "--game",
            game,
            "esp",
            "get-records",
            str(plugin),
            *(f"{form_id:06X}" for form_id in form_ids),
            "--authoring",
        ],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    payload = json.loads(result.stdout)
    return (
        {int(record["form_id"], 16): record for record in payload["records"]},
        payload["missing"],
    )


def test_toy_firework_patch_merges_idempotently_and_compiles() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    merged = _merge_script_method_patches(SKELETON, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.casefold().count("event onequipped(") == 1
    assert merged.casefold().count("event onunequipped(") == 1
    assert merged.casefold().count("event onanimationevent(") == 1
    equip = merged[
        merged.index("Event OnEquipped") : merged.index("Event OnUnequipped")
    ]
    assert equip.index('UnregisterForAnimationEvent(myWielder, "weaponFire")') < (
        equip.index('RegisterForAnimationEvent(myWielder, "weaponFire")')
    )
    assert equip.index('RegisterForAnimationEvent(myWielder, "weaponFire")') < (
        equip.rindex("myWielder = None")
    )
    unequip = merged[
        merged.index("Event OnUnequipped") : merged.index("Event OnAnimationEvent")
    ]
    assert unequip.index('UnregisterForAnimationEvent(myWielder, "weaponFire")') < (
        unequip.index("myWielder = None")
    )
    assert 'UnregisterForAnimationEvent(myWielder, "weaponFire")' in merged
    assert 'RegisterForAnimationEvent(myWielder, "weaponFire")' in merged
    assert 'UnregisterForAnimationEvent(wielder, "weaponFire")' in merged
    assert "If !RegisterForAnimationEvent" in merged
    assert "myWielder = None" in merged
    assert "Ammo expectedAmmo = myAmmo as Ammo" in merged
    assert "expectedAmmo == None" in merged
    assert "myWielder.GetItemCount(expectedAmmo) > 1" in merged
    assert "equippedToy.GetAmmo() != expectedAmmo" in merged
    assert "GetBaseObject()" not in merged
    assert "If BreakSFX != None" in merged
    assert "BreakSFX.Play(wielder)" in merged
    assert "If BreakMSG != None" in merged
    assert "BreakMSG.Show()" in merged
    assert "wielder.UnequipItem(equippedToy, False, True)" in merged
    assert "wielder.RemoveItem(equippedToy, 1, True)" in merged
    assert merged.index(
        "myWielder = None", merged.index("Actor wielder")
    ) < merged.index("BreakSFX.Play(wielder)")
    assert merged.index("BreakSFX.Play(wielder)") < merged.index("BreakMSG.Show()")
    assert merged.index("BreakMSG.Show()") < merged.index(
        "wielder.UnequipItem(equippedToy, False, True)"
    )
    assert merged.index("wielder.UnequipItem(equippedToy, False, True)") < merged.index(
        "wielder.RemoveItem(equippedToy, 1, True)"
    )
    assert "OnTimer" not in merged
    assert "StartTimer" not in merged
    assert "Utility.Wait" not in merged

    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.skipif(
    not SOURCE_PEX.is_file() or not LIVE_PEX.is_file(),
    reason="source client and converted PEX files are required",
)
def test_source_client_and_live_pex_are_declaration_only() -> None:
    for pex_path in (SOURCE_PEX, LIVE_PEX):
        source = decompile_pex(pex_path)
        parsed = parse_pex(pex_path)

        assert "Event " not in source
        assert "Function " not in source
        obj = parsed.objects[0]
        assert obj.name.casefold() == SCRIPT_NAME.casefold()
        assert all(not state.functions for state in obj.states)
        assert {prop.name.casefold() for prop in obj.properties} == {
            "myammo",
            "breaksfx",
            "breakmsg",
        }
        assert any(
            variable.name.casefold() == "mywielder" for variable in obj.variables
        )


@pytest.mark.skipif(
    shutil.which("modkit.exe") is None
    or not SOURCE_ESM.is_file()
    or not LIVE_ESM.is_file(),
    reason="source and live SeventySix masters are required",
)
def test_source_and_live_keep_effect_chain_but_only_source_has_durability() -> None:
    assert set(WEAPON_AMMO) == set(WEAPON_EDITOR_IDS)
    assert len(set(WEAPON_AMMO.values())) == 7
    shared_ids = [
        *WEAPON_AMMO,
        *AMMO_PROJECTILE,
        *PROJECTILE_EXPLOSION_TIMER,
        0x64A56E,
        0x4ED233,
    ]
    all_ids = [*shared_ids, *DURABILITY_CURVES]
    source, source_missing = _read_records(SOURCE_ESM, "fo76", all_ids)
    live, live_missing = _read_records(LIVE_ESM, "fo4", all_ids)

    assert source_missing == []
    assert set(live_missing) == {f"{form_id:06X}" for form_id in DURABILITY_CURVES}
    assert set(shared_ids) <= source.keys()
    assert set(shared_ids) <= live.keys()

    for form_id, ammo_id in WEAPON_AMMO.items():
        for records in (source, live):
            assert records[form_id]["eid"] == WEAPON_EDITOR_IDS[form_id]
            vmad = _field(records[form_id], "VirtualMachineAdapter")
            scripts = vmad["Scripts"]
            assert len(scripts) == 1
            assert scripts[0]["ScriptName"].casefold() == SCRIPT_NAME.casefold()
            properties = {
                prop["propertyName"].casefold(): prop
                for prop in scripts[0]["Properties"]
            }
            assert _form_id(properties["myammo"]["Value"]["FormID"]) == ammo_id
            assert _form_id(properties["breakmsg"]["Value"]["FormID"]) == 0x64A56E
            assert _form_id(properties["breaksfx"]["Value"]["FormID"]) == 0x4ED233
            if form_id in {0x64A56F, 0x64A570}:
                assert _form_id(properties["myform"]["Value"]["FormID"]) == 0x64A56F
            else:
                assert "myform" not in properties

        source_fields = {next(iter(entry)) for entry in source[form_id]["fields"]}
        live_fields = {next(iter(entry)) for entry in live[form_id]["fields"]}
        assert {
            "MinDurabilityCurve",
            "ConditionLossScale",
            "BashConditionLossScale",
            "MaxDurabilityCurve",
        } <= source_fields
        assert (
            not {
                "MinDurabilityCurve",
                "ConditionLossScale",
                "BashConditionLossScale",
                "MaxDurabilityCurve",
            }
            & live_fields
        )
        assert _form_id(_field(live[form_id], "Data")["Ammo"]) == ammo_id
        assert {"CantDrop", "NotPlayable"} <= set(
            _field(live[form_id], "Data")["Flags"]
        )

    for ammo_id, projectile_id in AMMO_PROJECTILE.items():
        for records in (source, live):
            dnam = _field(records[ammo_id], "DNAM")
            assert _form_id(dnam["Projectile"]) == projectile_id
            assert "HasCountBased3D" in dnam["Flags"]

    for projectile_id, (explosion_id, timer) in PROJECTILE_EXPLOSION_TIMER.items():
        for records in (source, live):
            data = _field(records[projectile_id], "Data")
            assert _form_id(data["Explosion"]) == explosion_id
            assert data["ExplosionAltTriggerTimer"] == timer
            assert {"Explosion", "AltTrigger", "MuzzleFlash"} <= set(data["Flags"])

    for form_id, editor_id in DURABILITY_CURVES.items():
        assert source[form_id]["eid"] == editor_id
        assert form_id not in live

    for records in (source, live):
        description = _field(records[0x64A56E], "Description")
        assert any(
            value["Language"] == "English"
            and value["String"] == "Toy Firework Gun Broke!"
            for value in description["Values"]
        )
