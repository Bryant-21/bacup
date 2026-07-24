from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
OLD_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySixOld"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "topicinfos"
)

PATCH_CASES = (
    "TIF_W05_DialogueSettlers_Fou_0059EDE8",
    "TIF_W05_DialogueSettlers_Fou_0059EDE9",
    "TIF_W05_DialogueSettlers_Fou_0059EDEA",
    "TIF_W05_DialogueSettlers_Fou_0059EDEB",
    "TIF_W05_DialogueSettlers_Fou_0059EDEC",
    "TIF_W05_DialogueSettlers_Fou_0059EDED",
    "TIF_W05_DialogueSettlers_Fou_005A3311",
    "TIF_W05_DialogueSettlers_Fou_005A3312",
    "TIF_W05_DialogueSettlers_Fou_005A3313",
    "TIF_W05_DialogueSettlers_Fou_005A3314",
    "TIF_W05_DialogueSettlers_Fou_005A3316",
    "TIF_W05_DialogueSettlers_Fou_005A3317",
    "TIF_W05_DialogueSettlers_Fou_005A3318",
    "TIF_W05_DialogueSettlers_Fou_005A331A",
    "TIF_W05_DialogueSettlers_Fou_005A331B",
    "TIF_W05_DialogueSettlers_Fou_005A331C",
    "TIF_W05_DialogueSettlers_Fou_005A331D",
    "TIF_W05_DialogueSettlers_Fou_005A331E",
    "TIF_W05_DialogueSettlers_Fou_005A331F",
    "TIF_W05_DialogueSettlers_Fou_005A3320",
    "TIF_W05_DialogueSettlers_Fou_005A3322",
    "TIF_W05_DialogueSettlers_Fou_005A3323",
    "TIF_W05_DialogueSettlers_Fou_005A3324",
    "TIF_W05_DialogueSettlers_Fou_005A3325",
    "TIF_W05_DialogueSettlers_Fou_005A3326",
    "TIF_W05_DialogueSettlers_Fou_005A3327",
    "TIF_W05_DialogueSettlers_Fou_005A3328",
)

DIRECT_PATCH_CASES = (
    "TIF_BS_RE_TravelDWD01_005C54AD",
    "TIF_BS_RE_TravelDWD02_005C721A",
    "TIF_BS_RE_TravelDWD03_005C757F",
    "TIF_E09D_MostWanted_0066DD77",
    "TIF_MOON_MiddleMountainPitst_006BE893",
    "TIF_RE_SceneDWD03_0052754B",
    "TIF_RE_SceneDWD05_003E1887",
    "TIF_RE_SceneSM03_003B7DD4_1",
    "TIF_W05_Community_RaiderFish_0057CF59",
    "TIF_W05_Community_RaiderFish_0057CF5A",
    "TIF_W05_Community_RaiderFish_0057CF5B",
    "TIF_W05_Community_RaiderFish_0057CF5C",
)

ALL_PATCH_CASES = (*PATCH_CASES, *DIRECT_PATCH_CASES)


def _script_name(base_name: str) -> str:
    return f"Fragments:TopicInfos:{base_name}"


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


@pytest.mark.parametrize("base_name", ALL_PATCH_CASES)
def test_topicinfo_patch_loads_merges_and_compiles_for_fo4(base_name: str):
    source_path = OLD_TOPICINFO_ROOT / f"{base_name}.psc"
    source = source_path.read_text(encoding="utf-8-sig")
    patch = _script_patch_source(_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merge_script_method_patches(source, patch)
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_topicinfo_patch_count_matches_confirmed_old_esm_batch():
    assert len(ALL_PATCH_CASES) == 39
