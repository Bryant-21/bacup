from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCHED_CASES = {
    "W05_MQ_004P_Crane_DoorTriggerScript": ("OnTriggerEnter", "OnTimer"),
    "W05_MQ_101P_A_RepairSubTerminalScript": ("OnMenuItemRun",),
    "W05_MQA_206P_SSTalkTriggerBoxScript": ("OnTriggerEnter",),
}

INTENTIONAL_NO_PATCH = (
    "W05_MQ_004p_UpstairDoorAliasScript",
    "W05_MQ_101P_A_RepairTerminalScript",
)


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))

    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                configured = line.split("=", 1)[1].strip().strip('"')
                if configured:
                    candidates.append(Path(configured))
                break

    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _merged_source(script_name: str) -> str:
    skeleton = (GENERATED_ROOT / f"{script_name}.psc").read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "members"), PATCHED_CASES.items())
def test_w05_patched_members_merge_once(script_name: str, members: tuple[str, ...]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not patch.lstrip().lower().startswith("scriptname ")

    merged = _merged_source(script_name)
    for member in members:
        assert merged.lower().count(f"event {member.lower()}(") == 1


def test_crane_door_uses_its_configured_open_stage():
    merged = _merged_source("W05_MQ_004P_Crane_DoorTriggerScript")

    assert "StageToSetOnOpen > 0" in merged
    assert "IsStageDone(StageToSetOnOpen)" in merged
    assert "SetStage(StageToSetOnOpen)" in merged
    assert "IsStageDone(1000)" not in merged


@pytest.mark.parametrize("script_name", INTENTIONAL_NO_PATCH)
def test_w05_open_controller_scripts_remain_unpatched(script_name: str):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCHED_CASES)
def test_w05_merged_scripts_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
