from __future__ import annotations

import os
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.esp import Plugin
from creation_lib.pex.native_runtime import compile_psc
from tools.stub_evidence.live_vmad_probe import locate


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
PLUGIN = Path(
    os.environ.get(
        "B21_TEST_SEVENTYSIX_ESM",
        REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm",
    )
)
LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "root-nonfragment-gap-closure-2026-08-11.md"
)
SCRIPT_NAME = "OverseerTerminalScript"
LIVE_CARRIERS = {
    "0863EE3A": ("TERM", "NWOT_Boss_Terminal", "0D4D34"),
    "08370B8A": ("TERM", "76CharGenOverseerTerminal", "0D4D34"),
    "08631D68": ("REFR", "Zachariadis_FSMainTerminalCompREF", None),
}


def _merged() -> str:
    skeleton = (SOURCE_ROOT / f"{SCRIPT_NAME}.psc").read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def test_tranche6_overseer_patch_has_exact_terminal_callback():
    patch = (PATCH_ROOT / f"{SCRIPT_NAME}.psc").read_text(encoding="utf-8")
    members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert members == [("event", "onmenuitemrun")]
    assert "SURV_Tutorial.IsRunning()" in patch
    assert "!SURV_Tutorial.IsStageDone(20)" in patch
    assert "SURV_Tutorial.SetStage(20)" in patch
    assert "auiMenuItemID ==" not in patch


def test_tranche6_overseer_patch_merges_idempotently_and_compiles():
    merged = _merged()
    assert merged.casefold().count("event onmenuitemrun(") == 1
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_tranche6_overseer_manifest_matches_exact_live_carriers_and_payloads():
    hits = locate(PLUGIN, [SCRIPT_NAME])[SCRIPT_NAME]
    assert {
        (hit["form_id"], hit["record_signature"], hit["editor_id"])
        for hit in hits
    } == {
        (form_id, signature, editor_id)
        for form_id, (signature, editor_id, _quest_id) in LIVE_CARRIERS.items()
    }

    target = Plugin.load(PLUGIN, game="fo4", lazy_index=True)
    try:
        for form_id, (_signature, _editor_id, quest_id) in LIVE_CARRIERS.items():
            record = target.read_authoring_record(int(form_id[-6:], 16))
            assert record is not None
            vmad = next(
                field["VirtualMachineAdapter"]
                for field in record["fields"]
                if "VirtualMachineAdapter" in field
            )
            scripts = [
                script
                for script in vmad["Scripts"]
                if script["ScriptName"] == SCRIPT_NAME
            ]
            assert len(scripts) == 1
            expected_properties = []
            if quest_id is not None:
                expected_properties = [
                    {
                        "propertyName": "SURV_Tutorial",
                        "Type": "Object",
                        "Flags": 1,
                        "Value": {
                            "Alias": -1,
                            "FormID": {
                                "reference": {
                                    "plugin": "SeventySix.esm",
                                    "object_id": quest_id,
                                }
                            },
                        },
                    }
                ]
            assert scripts[0]["Properties"] == expected_properties
    finally:
        target.close()


def test_tranche6_overseer_contract_records_exact_owner_stage():
    ledger = LEDGER.read_text(encoding="utf-8")
    assert "`OverseerTerminalScript` TERM owners" in ledger
    assert "`SURV_Tutorial` (`0D4D34`)" in ledger
    assert "`Activated Overseer Terminal`" in ledger
