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
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
CONTRACT = DOCS / "contracts" / "gameplay-local-event-dependencies-2026-09-01.md"

LIGHTNING_SCRIPT = "LightningStrike_DamageOnHitScript"
LIGHTNING_SKELETON = """Scriptname LightningStrike_DamageOnHitScript Extends ObjectReference

Float Property Damage Auto
"""
SOURCE_LIGHTNING_CARRIERS = ("7CE2CE", "76982F")
LIVE_LIGHTNING_RAW_CARRIERS = ("087CE2CE", "0876982F")
SOURCE_ESM = Path("N:/Steam Games/steamapps/common/Fallout76/Data/SeventySix.esm")
LIVE_ESM = REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm"


def test_gameplay_local_event_contract_names_the_lightning_evidence() -> None:
    source = CONTRACT.read_text(encoding="utf-8")
    normalized = " ".join(source.split())

    assert "`7CE2CE`" in source
    assert "`76982F`" in source
    assert "raw `087CE2CE`" in source
    assert "raw `0876982F`" in source
    assert "`76TestQDJP` (`649B43`)" in source
    assert "`StormHighknobFireTowerExt03` (`262DEA`)" in source
    assert "base `BarrelTankFlammable02` (`22DBAF`)" in source
    assert "destruction health 1" in normalized
    assert "one terminal stage" in normalized
    assert "`ExplosionBarrelTank` (`0A4C97`)" in normalized
    assert "two source and two live bindings" in normalized


def test_lightning_hit_patch_merges_once_and_compiles() -> None:
    patch = _script_patch_source(LIGHTNING_SCRIPT)
    assert patch is not None

    merged = _merge_script_method_patches(LIGHTNING_SKELETON, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.casefold().count("event onload()") == 1
    assert merged.casefold().count("event onunload()") == 1
    assert merged.casefold().count("event onhit(") == 1

    on_load_start = merged.index("Event OnLoad()")
    on_load_end = merged.index("EndEvent", on_load_start)
    on_load = merged[on_load_start:on_load_end]
    on_unload_start = merged.index("Event OnUnload()")
    on_unload_end = merged.index("EndEvent", on_unload_start)
    on_unload = merged[on_unload_start:on_unload_end]
    on_hit_start = merged.index("Event OnHit(")
    on_hit_end = merged.index("EndEvent", on_hit_start)
    on_hit = merged[on_hit_start:on_hit_end]

    assert (
        "Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, "
        "Form akSource, Projectile akProjectile, Bool abPowerAttack, "
        "Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, "
        "String apMaterial)"
    ) in on_hit
    assert on_load.count("RegisterForHitEvent(Self)") == 1
    assert on_hit.count("RegisterForHitEvent(Self)") == 1
    assert "UnregisterForAllHitEvents()" in on_unload
    assert "\n    DamageObject(Damage)\n" in on_hit
    assert "If Is3DLoaded() && !IsDestroyed()" in on_load
    assert "If !IsDestroyed()" in on_hit

    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=f"{LIGHTNING_SCRIPT}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def _read_carriers(
    plugin: Path, carrier_ids: tuple[str, str]
) -> dict[int, dict]:
    result = subprocess.run(
        [
            "modkit.exe",
            "esp",
            "get-records",
            "--authoring",
            str(plugin),
            *carrier_ids,
        ],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    payload = json.loads(result.stdout)
    assert payload["found"] == 2
    assert payload["missing"] == []
    return {
        int(record["form_id"], 16): record for record in payload["records"]
    }


def _field(record: dict, field_name: str):
    for entry in record["fields"]:
        if field_name in entry:
            return entry[field_name]
    return None


@pytest.mark.skipif(
    shutil.which("modkit.exe") is None
    or not SOURCE_ESM.is_file()
    or not LIVE_ESM.is_file(),
    reason="source and live SeventySix masters are required",
)
def test_source_and_live_lightning_carriers_keep_the_exact_vmad_contract() -> None:
    source_carriers = _read_carriers(SOURCE_ESM, SOURCE_LIGHTNING_CARRIERS)
    live_carriers = _read_carriers(LIVE_ESM, LIVE_LIGHTNING_RAW_CARRIERS)

    assert set(source_carriers) == {0x007CE2CE, 0x0076982F}
    assert set(live_carriers) == {0x007CE2CE, 0x0076982F}
    assert {int(value, 16) for value in LIVE_LIGHTNING_RAW_CARRIERS} == {
        0x087CE2CE,
        0x0876982F,
    }

    for carriers in (source_carriers, live_carriers):
        for record in carriers.values():
            assert _field(record, "Base") == {
                "reference": {"plugin": "SeventySix.esm", "object_id": "22DBAF"}
            }
            vmad = _field(record, "VirtualMachineAdapter")
            assert vmad["Scripts"] == [
                {
                    "ScriptName": "lightningstrike_damageonhitscript",
                    "Properties": [
                        {
                            "propertyName": "damage",
                            "Type": "Float",
                            "Flags": 1,
                            "Value": 100.0,
                        }
                    ],
                }
            ]
