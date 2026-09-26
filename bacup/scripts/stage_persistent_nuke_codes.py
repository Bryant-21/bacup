from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil


REPO = Path(__file__).resolve().parents[2]
TALES = REPO / "mods/B21_TalesFromAppalachia"
COMPILED = REPO / "tmp/keypad-inspection/B21_KeypadVerification/data/Scripts"
RUNTIME = TALES / "build/persistent-nuke-codes-20260915/B21_PersistentNukeCodesRuntime"
COMBINED = REPO / "mods/SeventySix/build/persistent-nuke-codes-20260915/B21_PersistentNukeCodes"
OWNED = ("EN07_NukeMasterScript", "EN07_ExternalKeypadAliasScript", "EN07_TargetingComputerAliasScript",
         "Nuke_CodesOfficerScript", "B21/B21_TFA_NukeRuntime")
CONVERTED = ("Nuke_MasterScript", "Nuke_CodesScript", "Nuke_LaunchCardPatrolTerminalScript")


def main():
    for package, names in ((RUNTIME, OWNED), (COMBINED, OWNED + CONVERTED)):
        package.mkdir(parents=True, exist_ok=True)
        (package / ".game").write_text("fo4\n")
        for name in names:
            target = package / "data/Scripts" / f"{name}.pex"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(COMPILED / f"{name}.pex", target)
    shutil.copytree(RUNTIME / "F4SE", COMBINED / "F4SE", dirs_exist_ok=True)
    files = [{"path": path.relative_to(COMBINED).as_posix(), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
             for directory in (COMBINED / "F4SE", COMBINED / "data")
             for path in sorted(directory.rglob("*")) if path.is_file()]
    report = {"package": str(COMBINED), "files": files, "installed": False,
              "native_baseline_sha256": "87c53f8468aaba9bf2d50d692205f2a51d5f71085f31e418550e7c01e8fd7723"}
    (COMBINED.parent / "staged.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
