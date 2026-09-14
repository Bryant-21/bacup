from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PHOTO_QUEST = "W05_Daily_PhotoQuestScript"
PHOTO_FRAGMENT = "Fragments:Quests:QF_W05_Daily_Photo_00548761"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "w05-photo-callback-repair-2026-09-01.md"
)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize("script_name", [PHOTO_QUEST, PHOTO_FRAGMENT])
def test_w05_photo_patches_are_member_only_and_merge_idempotently(
    script_name: str,
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert _member_names(patch)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    for member_name in _member_names(patch):
        assert _member_names(merged).count(member_name) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_photo_target_registration_uses_the_exact_fo4_hit_event_contract() -> None:
    patch = _script_patch_source(PHOTO_QUEST)
    assert patch is not None

    camera = _member_body(patch, "photocamera")
    register = _member_body(patch, "registerphototarget")
    refresh = _member_body(patch, "refreshphototargetregistrations")
    on_hit = _member_body(patch, "onhit")

    assert 'Game.GetFormFromFile(0x0046F481, "SeventySix.esm") as Weapon' in camera
    assert "targetData == None || targetData.TriggerAlias == None" in register
    assert "targetData.StageToSet < 5" in register
    assert "!IsStageDone(targetData.StageToSet - 5)" in register
    assert "IsStageDone(targetData.StageToSet)" in register
    assert "targetData.TriggerAlias.GetReference()" in register
    assert "targetRef != None" in register
    assert "myPlayer != None" in register
    assert "camera != None" in register
    assert "RegisterForHitEvent(targetRef, myPlayer, camera)" in register
    assert "PhotoTargets == None" in refresh

    assert on_hit.startswith(
        "Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, "
        "Form akSource, Projectile akProjectile, Bool abPowerAttack, "
        "Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, "
        "String apMaterial)"
    )
    assert "akAggressor == PlayerRef" in on_hit
    assert "akSource == PhotoCamera()" in on_hit
    assert "targetData != None" in on_hit
    assert "targetData.TriggerAlias != None" in on_hit
    assert "targetData.TriggerAlias.GetReference() == akTarget" in on_hit
    assert "targetData.StageToSet >= 5" in on_hit
    assert "IsStageDone(targetData.StageToSet - 5)" in on_hit
    assert "!IsStageDone(targetData.StageToSet)" in on_hit
    assert on_hit.index("SetStage(targetData.StageToSet)") < on_hit.index(
        "RefreshPhotoTargetRegistrations()"
    )
    assert on_hit.count("RefreshPhotoTargetRegistrations()") == 2
    assert on_hit.index("RefreshPhotoTargetRegistrations()") < on_hit.index(
        "Return"
    )


def test_photo_hit_registrations_follow_stage_and_player_load_lifecycle() -> None:
    patch = _script_patch_source(PHOTO_QUEST)
    assert patch is not None

    refresh = _member_body(patch, "refreshphototargetregistrations")
    quest_init = _member_body(patch, "onquestinit")
    stage_set = _member_body(patch, "onstageset")
    load_game = _member_body(patch, "actor.onplayerloadgame")
    shutdown = _member_body(patch, "onquestshutdown")

    assert refresh.index("UnregisterForAllHitEvents()") < refresh.index(
        "RegisterPhotoTarget(PhotoTargets[targetIndex])"
    )
    assert 'RegisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")' in quest_init
    assert "RefreshPhotoTargetRegistrations()" in quest_init
    assert "RefreshPhotoTargetRegistrations()" in stage_set
    assert "RegisterPhotoTarget(" not in stage_set
    assert "akSender == PlayerRef" in load_game
    assert "RefreshPhotoTargetRegistrations()" in load_game
    assert "UnregisterForAllHitEvents()" in shutdown
    assert 'UnregisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")' in shutdown
    assert shutdown.index(
        'UnregisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")'
    ) < shutdown.index("PlayerRef = None")


@pytest.mark.parametrize(
    ("stage", "objective", "group_stage"),
    [
        (215, 200, 400),
        (225, 220, 400),
        (235, 230, 400),
        (255, 250, 500),
        (265, 260, 500),
        (275, 270, 500),
        (315, 300, 400),
        (325, 320, 400),
        (335, 330, 400),
        (355, 350, 500),
        (365, 360, 500),
        (375, 370, 500),
    ],
)
def test_photo_completion_stages_preserve_objective_then_group_order(
    stage: int, objective: int, group_stage: int
) -> None:
    patch = _script_patch_source(PHOTO_FRAGMENT)
    assert patch is not None

    fragment = _member_body(
        patch, f"fragment_stage_0{stage:03d}_item_00"
    )
    helper = _member_body(patch, "completephotoobjective")

    assert f"CompletePhotoObjective({objective}, {group_stage})" in fragment
    assert helper.index("SetObjectiveCompleted(objectiveID)") < helper.index(
        "SetStage(groupStage)"
    )


def test_photo_group_completion_routes_to_the_exact_return_objective() -> None:
    patch = _script_patch_source(PHOTO_FRAGMENT)
    assert patch is not None

    group_check = _member_body(patch, "checkphotogroupscomplete")
    stage_600 = _member_body(patch, "fragment_stage_0600_item_00")

    assert "IsStageDone(400) && IsStageDone(500)" in group_check
    assert "SetStage(600)" in group_check
    assert stage_600.index("IsStageDone(200)") < stage_600.index(
        "SetObjectiveDisplayed(500)"
    )
    assert stage_600.index("IsStageDone(300)") < stage_600.index(
        "SetObjectiveDisplayed(400)"
    )


def test_photo_fragment_does_not_invent_turn_in_or_reward_tail() -> None:
    patch = _script_patch_source(PHOTO_FRAGMENT)
    assert patch is not None

    member_names = set(_member_names(patch))
    for stage in (
        700,
        800,
        900,
        950,
        975,
        1000,
        1100,
        1200,
        1300,
        1400,
        1500,
        2000,
    ):
        assert f"fragment_stage_{stage:04d}_item_00" not in member_names


def test_photo_contract_discloses_the_source_live_carrier_boundary() -> None:
    source = CONTRACT.read_text(encoding="utf-8")

    for form_id in ("548761", "69F2C7", "46F481", "3F1211", "3F1228", "54EB26"):
        assert f"`{form_id}`" in source
    assert "CameraWeaponDetectable" in source
    assert "IsALookatTrigger" in source
    assert "single-player substitute" in source
    assert "not FO76 photo-service parity" in source


@pytest.mark.parametrize("script_name", [PHOTO_QUEST, PHOTO_FRAGMENT])
def test_w05_photo_production_merge_native_compiles(script_name: str) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
