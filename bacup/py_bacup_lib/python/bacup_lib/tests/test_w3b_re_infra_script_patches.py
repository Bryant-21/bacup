from __future__ import annotations

import csv
import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
PACKAGE_CLOSURE_CONTRACT = "contracts/w05-re-package-client-body-closure.md"

PATCH_CASES = {
    "W05_RE_ObjJP01_DistanceCheckStageSet": (
        "StartTimer(5.0, 1)",
        "CancelTimer(1)",
        "SetStage(RangeCheckStage)",
    ),
    "W05_RE_PositionAndRotationCorrection": (
        "kMoveTarget.SetPosition(",
        "kRotator.SetAngle(",
        "SetStage(StageToSetAfterInit)",
    ),
    "W05_RE_ObjectBB02_ObjectMoveScript": (
        "kNote.MoveTo(kTable)",
        "kBox.MoveTo(kTable)",
    ),
    "W05_RE_TravelBB01_SoundtrackBotScript": (
        'RegisterForRemoteEvent(kWanderer, "OnCombatStateChanged")',
        'RegisterForRemoteEvent(kWanderer, "OnDeath")',
        "W05_RE_TravelBB01_DeadScene.Start()",
    ),
    # Cannibal package fragments (narrow partial repair): only the 3 rows whose
    # own bound Cannibal02Alias property resolves to the owning quest's
    # ClutterMarkerEnable alias (562281, alias 24) get this patch.
    # PF_W05_RE_SceneAF04_TravelTo_00584A26 binds the same-named property to
    # another quest's alias 24 (5849E2's "CraterDoor"), so it gets none; its client
    # PEX is memberless (non-defect).
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573891": (
        "Cannibal02Alias as ReferenceAlias",
        "kClutterRef.Enable()",
    ),
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573892": (
        "Cannibal02Alias as ReferenceAlias",
        "kClutterRef.Enable()",
    ),
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573895": (
        "Cannibal02Alias as ReferenceAlias",
        "kClutterRef.Enable()",
    ),
}

EXPECTED_MEMBERS = {
    "W05_RE_ObjJP01_DistanceCheckStageSet": {"oninit", "ontimer"},
    "W05_RE_PositionAndRotationCorrection": {"oninit"},
    "W05_RE_ObjectBB02_ObjectMoveScript": {"oninit"},
    "W05_RE_TravelBB01_SoundtrackBotScript": {
        "oninit",
        "actor.ondeath",
        "actor.oncombatstatechanged",
    },
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573891": {"fragment_end"},
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573892": {"fragment_end"},
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573895": {"fragment_end"},
}

# Minimum count of "!= None" / "== None" guard tokens each patch must retain —
# one per external reference (alias/property/native-return) that can be unbound.
MIN_NONE_GUARD_TOKENS = {
    "W05_RE_ObjJP01_DistanceCheckStageSet": 3,
    "W05_RE_PositionAndRotationCorrection": 10,
    "W05_RE_ObjectBB02_ObjectMoveScript": 3,
    "W05_RE_TravelBB01_SoundtrackBotScript": 5,
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573891": 2,
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573892": 2,
    "Fragments:Packages:PF_W05_RE_CampAF03_TravelToL_00573895": 2,
}

BODYLESS_PACKAGE_CASES = (
    "Fragments:Packages:PF_W05_RE_SceneAF04_TravelTo_00584A26",
    "Fragments:Packages:PF_W05_RE_Scene_TravelersJM0_00569407",
    "Fragments:Packages:PF_W05_RE_Scene_TravelersJM0_0056EC46",
    "Fragments:Packages:PF_W05_RE_Scene_TravelersJM0_00571D1C",
    "Fragments:Packages:PF_W05_RE_Scene_TravelersJM0_00587BDF",
    "Fragments:Packages:PF_W05_RE_Scene_TravelersJM0_00587BE0",
    "Fragments:Packages:PF_W05_RE_TravelAF02_TravelT_0059DD34",
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
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_calls"), PATCH_CASES.items())
def test_w3b_re_infra_patch_restores_confirmed_behavior(
    script_name: str, expected_calls: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert _member_names(patch) == EXPECTED_MEMBERS[script_name]
    for call in expected_calls:
        assert call in patch


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_w3b_re_infra_actions_are_none_guarded(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    guard_count = patch.count("!= None") + patch.count("== None")
    assert guard_count >= MIN_NONE_GUARD_TOKENS[script_name]


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_w3b_re_infra_patch_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_w3b_re_infra_patch_count_matches_confirmed_batch():
    assert len(PATCH_CASES) == 7


def test_w3b_re_bodyless_packages_are_closed_without_invented_patches():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"].lower(): row for row in csv.DictReader(status_file)}

    assert len(BODYLESS_PACKAGE_CASES) == 7
    for script_name in BODYLESS_PACKAGE_CASES:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "non-defect"
        assert row["evidence"] == PACKAGE_CLOSURE_CONTRACT
        assert _script_patch_source(script_name) is None
