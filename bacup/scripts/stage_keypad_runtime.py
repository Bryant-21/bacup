from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil

from bacup_lib.keypad_ui import convert_keypad_ui

REPO = Path(__file__).resolve().parents[2]
TALES = REPO / "mods/B21_TalesFromAppalachia"
GENERATED = REPO / "mods/SeventySix"
COMPILED = REPO / "tmp/keypad-inspection/B21_KeypadVerification/data/Scripts"
RUNTIME = TALES / "build/keypad-runtime-20260915/B21_KeypadRuntime"
CONVERTED = GENERATED / "build/keypad-runtime-20260915/B21_ConvertedKeypad"
SCRIPT_NAMES = (
    "DefaultKeypadScript", "DefaultAliasSetStageOnKeypadSuccess", "W05_MQR_Vault79KeypadAliasScript",
    "Nuke_MasterScript", "Nuke_CodePageRefScript", "Nuke_CodeSolutionMasterScript",
    "Nuke_CodesSolutionPrinterScript", "EN07_ExternalKeypadAliasScript",
    "EN07_CodeHuntQuestScript", "Nuke_LaunchCardPatrolTerminalScript",
    "EN07_TargetingComputerAliasScript",
)


def copy_script(name: str, destination: Path):
    source = COMPILED / f"{name}.pex"
    target = destination / "data/Scripts" / source.relative_to(COMPILED)
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def main():
    for package in (RUNTIME, CONVERTED):
        package.mkdir(parents=True, exist_ok=True)
        (package / ".game").write_text("fo4\n")
    for name in SCRIPT_NAMES:
        copy_script(name, CONVERTED)
    for name in ("B21/KeypadNative", "EN07_NukeMasterScript"):
        copy_script(name, RUNTIME)
        copy_script(name, CONVERTED)
        copy_script(name, TALES)
    shutil.copytree(RUNTIME / "F4SE", CONVERTED / "F4SE", dirs_exist_ok=True)
    conversion = convert_keypad_ui(REPO / "extracted/fo76", GENERATED / "data")
    for item in conversion["files"]:
        source = GENERATED / "data" / item["path"]
        target = CONVERTED / "data" / item["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for package in (RUNTIME, CONVERTED):
        files = []
        for directory in (package / "data", package / "F4SE"):
            if directory.exists():
                for source in sorted(directory.rglob("*")):
                    if source.is_file():
                        files.append({"path": source.relative_to(package).as_posix(),
                                      "sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
        (package.parent / f"{package.name}-files.json").write_text(json.dumps(files, indent=2) + "\n")
    print(json.dumps({"runtime": str(RUNTIME), "converted": str(CONVERTED), "conversion": conversion}, indent=2))


if __name__ == "__main__":
    main()
