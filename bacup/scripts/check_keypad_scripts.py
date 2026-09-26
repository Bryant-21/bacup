from __future__ import annotations

import json
from pathlib import Path
import subprocess

from bacup_lib.workflows.unified import _augment_fo76_to_fo4_script_skeleton, _merge_script_method_patches, _script_patch_source

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "mods/SeventySix/Scripts/Source/User"
TALES = REPO / "mods/B21_TalesFromAppalachia"
SCOPE = REPO / "tmp/keypad-inspection/B21_KeypadVerification"
NAMES = (
    "DefaultKeypadScript", "DefaultAliasSetStageOnKeypadSuccess", "W05_MQR_Vault79KeypadAliasScript",
    "Nuke_MasterScript", "Nuke_CodePageRefScript", "Nuke_CodeSolutionMasterScript",
    "Nuke_CodesSolutionPrinterScript", "EN07_ExternalKeypadAliasScript", "EN07_NukeMasterScript",
    "EN07_CodeHuntQuestScript", "Nuke_LaunchCardPatrolTerminalScript",
    "EN07_TargetingComputerAliasScript",
    "Nuke_CodesScript", "Nuke_CodesOfficerScript",
)


def main():
    output = SCOPE / "Scripts/Source/User"
    output.mkdir(parents=True, exist_ok=True)
    for name in NAMES:
        origin = SOURCE / f"{name}.psc"
        owned = TALES / "Scripts/Source/User" / f"{name}.psc"
        if owned.is_file():
            origin = owned
        skeleton = _augment_fo76_to_fo4_script_skeleton(name, origin.read_text(encoding="utf-8-sig"))
        source = _merge_script_method_patches(skeleton, _script_patch_source(name))
        (output / f"{name}.psc").write_text(source, encoding="utf-8")
    (output / "B21").mkdir(exist_ok=True)
    (output / "B21/KeypadNative.psc").write_bytes((TALES / "Scripts/Source/User/B21/KeypadNative.psc").read_bytes())
    (output / "B21/B21_TFA_NukeRuntime.psc").write_bytes((TALES / "Scripts/Source/User/B21/B21_TFA_NukeRuntime.psc").read_bytes())
    (SCOPE / ".game").write_text("fo4")
    (SCOPE / ".papyrus-imports.json").write_text(json.dumps([str(SOURCE),
        str(TALES / "Scripts/Source/Imports"), str(TALES / "Scripts/Source/User")]))
    (SCOPE / ".papyrus-stock.json").write_text(json.dumps([f"{name}.psc" for name in NAMES] + ["B21/KeypadNative.psc", "B21/B21_TFA_NukeRuntime.psc"]))
    subprocess.run(["modkit.exe", "mod", "compile", str(SCOPE), "--verify-stock"], cwd=REPO, check=True)


if __name__ == "__main__":
    main()
