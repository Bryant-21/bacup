from __future__ import annotations

from collections import Counter

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


QF_A = "Fragments:Quests:QF_W05_MQ_101P_A_003FBC0D"
QF_B = "Fragments:Quests:QF_W05_MQ_101P_B_003FBC10"


def _member(stage: int, item: int = 0) -> str:
    return f"fragment_stage_{stage:04d}_item_{item:02d}"


EXACT_MEMBERS = {
    QF_A: tuple(
        _member(stage, item)
        for stage, item in (
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 0),
            (10, 0),
            (50, 0),
            (100, 0),
            (100, 1),
            (200, 0),
            (300, 0),
            (310, 0),
            (311, 0),
            (320, 0),
            (330, 0),
            (331, 0),
            (350, 0),
            (375, 0),
            (400, 0),
            (500, 0),
            (600, 0),
            (650, 0),
            (680, 0),
            (700, 0),
            (710, 0),
            (730, 0),
            (800, 0),
            (810, 0),
            (820, 0),
            (830, 0),
            (900, 0),
            (910, 0),
            (930, 0),
            (950, 0),
            (960, 0),
            (970, 0),
            (1000, 0),
            (1050, 0),
            (1100, 0),
            (1110, 0),
            (1200, 0),
            (1210, 0),
            (1300, 0),
            (1400, 0),
            (1415, 0),
            (1420, 0),
            (1430, 0),
            (1440, 0),
            (1450, 0),
            (1500, 0),
            (1530, 0),
            (8000, 0),
            (9000, 0),
        )
    ),
    QF_B: tuple(
        _member(stage)
        for stage in (
            10,
            100,
            200,
            230,
            231,
            232,
            240,
            300,
            350,
            400,
            450,
            500,
            590,
            600,
            700,
            9000,
        )
    ),
}


HELPER_MEMBERS = {
    "W05_MQ_101P_A_QuestScript": (
        "onstageset",
        "preparemezzanine",
        "checkmezzaninehostiles",
        "actor.ondeath",
        "spawnmegparty",
    ),
    "W05_MQ_101P_A_AldridgeOnLoadScript": ("onload",),
    "W05_MQ_101P_A_HookUpHoloScript": ("onmenuitemrun",),
    "W05_MQ_101P_A_RepairTerminalScript": ("onactivate",),
    "W05_MQ_101P_A_EWSBossScript": ("ondeath",),
    "W05_MQ_101P_A_RepairSubTerminalScript": ("onmenuitemrun",),
    "W05_MQ_101P_B_AubrieAliasScript": (
        "onaliasinit",
        "prepareforcave",
        "sendhome",
    ),
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[tuple[str, str, int, int]]:
    return list(_iter_top_level_papyrus_members(source.splitlines()))


def _names(source: str) -> list[str]:
    return [name for _kind, name, _start, _end in _members(source)]


def _body(source: str, name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, member_name, start, end in _members(source)
        if member_name == name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def test_qf_patches_cover_every_exact_live_vmad_member_once():
    assert len(EXACT_MEMBERS[QF_A]) == 55
    assert len(EXACT_MEMBERS[QF_B]) == 16
    for script_name, expected in EXACT_MEMBERS.items():
        patch = _patch(script_name)
        names = _names(patch)
        assert names == list(expected)
        assert Counter(names) == Counter({name: 1 for name in expected})
        assert _iter_papyrus_states(patch.splitlines()) == []
        assert "; TODO" not in patch


def test_no_live_qf_member_has_an_empty_or_comment_only_body():
    for script_name, expected in EXACT_MEMBERS.items():
        patch = _patch(script_name)
        for member_name in expected:
            lines = _body(patch, member_name).splitlines()[1:-1]
            executable = [
                line
                for line in lines
                if line.strip() and not line.lstrip().startswith(";")
            ]
            assert executable, f"empty live member: {script_name}.{member_name}"


def test_route_critical_helpers_have_complete_member_surfaces():
    for script_name, expected in HELPER_MEMBERS.items():
        patch = _patch(script_name)
        assert _names(patch) == list(expected)
        assert _iter_papyrus_states(patch.splitlines()) == []


def test_repair_terminal_routes_only_a_player_without_the_password_to_stage_650():
    patch = _patch("W05_MQ_101P_A_RepairTerminalScript")
    body = _body(patch, "onactivate")
    assert "akActionRef != Game.GetPlayer()" in body
    assert "W05_MQ_101P_A == None" in body
    assert "W05_MQ_101P_A_RepairTerminalKey == None" in body
    assert "akActionRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) < 1" in body
    assert "!W05_MQ_101P_A.IsStageDone(iStageGetPasscode)" in body
    assert "W05_MQ_101P_A.SetStage(iStageGetPasscode)" in body


def test_aubrie_helper_uses_the_bound_enable_parent_as_her_package_destination():
    patch = _patch("W05_MQ_101P_B_AubrieAliasScript")
    alias_init = _body(patch, "onaliasinit")
    prepare = _body(patch, "prepareforcave")
    send_home = _body(patch, "sendhome")

    assert "AubrieEnableParent.GetReference()" in alias_init
    assert "aubrieRef.SetLinkedRef(enableParent)" in alias_init
    assert "enableParent.Enable()" in prepare
    assert "aubrieRef.Enable()" in prepare
    assert "aubrieRef.EvaluatePackage()" in prepare
    assert "aubrieRef.SetLinkedRef(enableParent)" in send_home
    assert "aubrieRef.EvaluatePackage()" in send_home


def test_strange_bedfellows_route_converges_without_skipping_dialogue_choices():
    patch = _patch(QF_A)
    assert "MTNS01_Intro_Quest_Keyword.SendStoryEvent" in _body(patch, _member(10))
    assert "SetStage(600)" in _body(patch, _member(500))
    assert "SetStage(350)" in _body(patch, _member(330))
    assert "SetStage(400)" in _body(patch, _member(375))
    assert "SetStage(900)" in _body(patch, _member(820))
    assert "W05_MQ_101P_A_1000_DavidScene.Start()" in _body(patch, _member(960))
    assert "SetStage(970)" in _body(patch, _member(960))
    assert "SetStage(1000)" in _body(patch, _member(970))
    assert "controller.CheckMezzanineHostiles()" in _body(patch, _member(1050))
    assert "SetStage(1110)" in _body(patch, _member(1100))
    assert "controller.SpawnMegParty()" in _body(patch, _member(1110))
    assert "SetStage(1200)" in _body(patch, _member(1110))
    assert "SetStage(1500)" in _body(patch, _member(1450))
    assert "W05_MQ_101P.SetStage(200)" in _body(patch, _member(9000))
    assert "SetStage(1300)" not in _body(patch, _member(1200))


def test_here_to_stay_route_converges_all_clue_and_aubrie_outcomes():
    patch = _patch(QF_B)
    for stage in (230, 231, 232, 240):
        assert "SetStage(300)" in _body(patch, _member(stage))
    assert "aubrieAlias.PrepareForCave()" in _body(patch, _member(300))
    assert "SetObjectiveDisplayed(50)" in _body(patch, _member(350))
    for stage in (400, 450, 500, 600):
        assert "SetStage(700)" in _body(patch, _member(stage))
    hostile = _body(patch, _member(590))
    assert "aubrieRef.SetValue(Aggression, 2.0)" in hostile
    assert "aubrieRef.StartCombat(Game.GetPlayer())" in hostile
    assert "SetObjectiveDisplayed(60)" in _body(patch, _member(700))
    assert "W05_MQ_101P.SetStage(300)" in _body(patch, _member(9000))
    assert "SetStage(400)" not in _body(patch, _member(350))


def test_single_player_helper_edges_are_guarded_and_use_fo4_collection_api():
    controller = _patch("W05_MQ_101P_A_QuestScript")
    assert "MezzanineHostiles.GetAt(index) as Actor" in controller
    assert 'RegisterForRemoteEvent(hostileRef, "OnDeath")' in controller
    assert "livingHostiles == 0" in controller
    assert "SetStage(iStageSpawnMeg)" in controller
    assert "GetActorAt" not in controller

    ews = _patch("W05_MQ_101P_A_EWSBossScript")
    assert "EWSBossSolomonsPond.GetAt(index) as Actor" in ews
    assert "GetActorAt" not in ews

    terminal = _patch("W05_MQ_101P_A_HookUpHoloScript")
    assert "auiMenuItemID == 1" in terminal
    assert "W05_MQ_101P_A.SetStage(iStagePlayBroadcast)" in terminal

    aldridge = _patch("W05_MQ_101P_A_AldridgeOnLoadScript")
    assert "W05_MQ_101P_A.IsStageDone(1300)" in aldridge
    assert "W05_MQ_101P_A.SetStage(1400)" in aldridge
