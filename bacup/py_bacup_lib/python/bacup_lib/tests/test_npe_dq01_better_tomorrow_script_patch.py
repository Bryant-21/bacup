from __future__ import annotations

import os
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:Quests:QF_NPE_DQ01_BetterTomorrow_006FD072"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
PATCH_PATH = (
    PATCH_ROOT
    / "Fragments"
    / "Quests"
    / "QF_NPE_DQ01_BetterTomorrow_006FD072.psc"
)

EXPECTED_MEMBERS = {
    "fragment_stage_0100_item_00",
    "fragment_stage_0175_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0310_item_00",
    "fragment_stage_0320_item_00",
    "fragment_stage_0330_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0410_item_00",
    "fragment_stage_0415_item_00",
    "fragment_stage_0420_item_00",
    "fragment_stage_0430_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0505_item_00",
    "fragment_stage_0510_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_9000_item_00",
}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def test_better_tomorrow_patch_merges_once_and_compiles():
    from bacup_lib.workflows.unified import (
        _iter_top_level_papyrus_members,
        _merge_script_method_patches,
        _script_patch_source,
        _script_relative_path,
    )
    from creation_lib.pex.native_runtime import compile_psc

    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in EXPECTED_MEMBERS:
        assert members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_better_tomorrow_patch_restores_offline_quest_contract():
    patch = PATCH_PATH.read_text(encoding="utf-8")

    assert patch.count("Function Fragment_Stage_") == len(EXPECTED_MEMBERS)
    assert "SetObjectiveDisplayed(10, True, True)" in patch
    assert "SetStage(20)" in patch
    assert "SetStage(300)" in patch
    assert "SetStage(400)" in patch
    assert "SetStage(500)" in patch
    assert "SetObjectiveCompleted(50)" in patch
    assert "SetObjectiveCompleted(51)" in patch
    assert "SetObjectiveCompleted(52)" in patch
    assert "playerRef.RemoveItem(NPE_DQ01_FreshMeat, meatCount, True)" in patch
    assert (
        "playerRef.RemoveItem(NPE_DQ01_CultistArtifact, artifactCount, True)"
        in patch
    )
    assert "SetStage(9000)" in patch
    assert "Stop()" in patch
    assert "RandomInt" not in patch
    assert "Reward" not in patch
    assert "Reset()" not in patch

    stage_415 = patch.split("Function Fragment_Stage_0415_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    assert stage_415.strip() == "Return"
    assert not (
        PATCH_ROOT / "QUESTS" / "npe_dq01_bettertomorrow" / "playerscript.psc"
    ).exists()
