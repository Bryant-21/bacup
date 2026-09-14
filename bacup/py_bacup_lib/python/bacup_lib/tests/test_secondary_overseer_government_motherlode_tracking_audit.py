from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "secondary-overseer-government-motherlode-tracking-audit-2026-09-02.md"
)

STORY_START_TRIPLES = (
    ("004E49D9", "004E4A13", "004E49E5"),
    ("0026AA34", "00126717", "0012F8A2"),
)

MQ_ALIASES = (
    "QuestObject01",
    "QuestObject01A",
    "QuestObject02",
    "QuestObject03",
    "QuestObject04",
    "QuestObject05",
    "QuestObject06",
    "QuestObject07",
    "QuestObject08",
    "QuestObject09",
    "QuestObject10",
    "QuestObject11",
    "QuestObject12",
    "QuestObjectX1",
    "QuestObjectX2",
    "QuestObjectX3",
    "QuestObjectX4",
    "QuestObjectX5",
)

PERSONAL_ALIASES = (
    "QuestObject01_VTecAg",
    "QuestObject02_Family",
    "QuestObject03_HighSchool",
    "QuestObject04_University",
    "QuestObject05_House",
    "QuestObject06_Mine",
)


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def test_contract_pins_all_five_quests_and_exact_fail_closed_start_triples():
    contract = CONTRACT.read_text(encoding="utf-8")
    for form_id, editor_id in (
        ("004E49D9", "MQ_Overseer"),
        ("0026AA34", "OverseerPersonal"),
        ("00006F3C", "GQ_DropGovtIntro"),
        ("0006A379", "MTR05_Mother"),
        ("00131033", "SFL02_Track"),
    ):
        assert form_id in contract
        assert editor_id in contract

    for quest, node, selector in STORY_START_TRIPLES:
        assert quest in contract
        assert node in contract
        assert selector in contract
    assert "fail closed for every other" in contract
    assert "quest_start_unqualified:no_active_keyword_role" in contract
    assert "completion-count-equals-zero condition" in contract


def test_mq_overseer_registers_native_holotape_play_for_all_eighteen_logs():
    patch = _patch("MQ_OverseerPlayerScript")
    handler = _member_body(patch, "objectreference.onholotapeplay")
    reconcile = _member_body(patch, "reconcileholotapeplayregistrations")
    clear = _member_body(patch, "clearholotapeplayregistrations")

    assert 'PlayedStageFor(akSender.GetBaseObject())' in handler
    for alias in MQ_ALIASES:
        assert reconcile.count(f"RegisterHolotapePlay({alias})") == 1
        assert clear.count(f"UnregisterHolotapePlay({alias})") == 1
    assert len(MQ_ALIASES) == 18
    assert "OnItemEquipped" in patch
    assert "OnAliasShutdown" in patch


def test_mq_overseer_contract_reconciles_eighteen_logs_with_seventeen_targets():
    contract = CONTRACT.read_text(encoding="utf-8")
    assert "all 18 bound Overseer logs" in contract
    assert "other 17 logs are map\n  targets" in contract
    assert "Vault 76 log is the starting handoff" in contract


def test_personal_matters_registers_native_play_for_all_six_journals():
    patch = _patch("OverseerPersonal_PlayerScript")
    handler = _member_body(patch, "objectreference.onholotapeplay")
    reconcile = _member_body(patch, "reconcileholotapeplayregistrations")
    clear = _member_body(patch, "clearholotapeplayregistrations")

    for alias in PERSONAL_ALIASES:
        assert reconcile.count(f"RegisterHolotapePlay({alias})") == 1
        assert clear.count(f"UnregisterHolotapePlay({alias})") == 1
    for stage in (15, 25, 35, 45, 55, 65):
        assert f"SetStageOnce({stage})" in handler
    assert "OnItemEquipped" in patch
    assert "OnAliasShutdown" in patch


def test_government_drop_advances_only_through_the_bound_request_holotape():
    quest_patch = _patch("GQ_DropGovtIntroScript")
    fragment = _patch("Fragments:Quests:QF_GQ_DropGovtIntro_00006F3C")

    init = _member_body(quest_patch, "onquestinit")
    removed = _member_body(quest_patch, "objectreference.onitemremoved")
    timer = _member_body(quest_patch, "ontimer")
    assert "SetStage(Stage_FirstObjective)" in init
    assert "akBaseItem == GQ_DropGovt01Holotape" in removed
    assert "IsStageDone(200)" in removed
    assert "SetStage(Stage_HolotapeUsed)" in timer
    for stage in (100, 200, 300):
        assert f"Fragment_Stage_{stage:04}_Item_00" in fragment


def test_government_drop_holotape_sender_is_exact_and_idempotent():
    sender = _patch("GQ_DropGovt01HolotapeScript")
    on_init = _member_body(sender, "oninit")
    moved = _member_body(sender, "oncontainerchanged")
    request = _member_body(sender, "requestgovernmentdropstart")

    assert "RequestGovernmentDropStart(GetContainer())" in on_init
    assert "RequestGovernmentDropStart(akNewContainer)" in moved
    assert "akContainer != playerRef" in request
    assert 'Game.GetFormFromFile(0x006F3C, "SeventySix.esm") as Quest' in request
    guard = "governmentDrop.IsRunning() || governmentDrop.IsCompleted()"
    assert guard in request
    assert (
        "GQ_DropGovtIntroKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in request
    )
    assert ".Start()" not in request
    assert "SetStage(" not in request


def test_motherlode_completion_converges_to_stop_without_invented_handoff():
    fragment = _patch("Fragments:Quests:QF_MTR05_Mother_0006A379")
    stage_500 = _member_body(fragment, "fragment_stage_0500_item_00")
    stage_510 = _member_body(fragment, "fragment_stage_0510_item_00")

    assert "SetObjectiveCompleted(310, True)" in stage_500
    assert "SetStage(510)" in stage_500
    assert "Stop()" in stage_510
    assert "W05_MQS_201P" not in fragment


def test_tracking_unknowns_child_routes_and_lucy_completion_are_pinned():
    root = _patch("SFL02_Track_QuestScript")
    main = _patch("Fragments:Quests:QF_SFL02_Track_00131033")
    vertibot = _patch("Fragments:Quests:QF_SFL02_Track_VertibotQuest_0032BB59")

    playback = _member_body(root, "objectreference.onholotapeplay")
    reconciliation = _member_body(root, "reconcileruntimeregistrations")
    shutdown = _member_body(root, "onquestshutdown")
    for alias, stage in (
        ("HolotapeNari", 1010),
        ("HolotapeRandy", 1020),
        ("HolotapeLucy", 2000),
    ):
        assert f"SetStageForAliasForm(playedHolotape, {alias}, {stage})" in playback
        assert reconciliation.count(f"UnregisterHolotapePlay({alias})") == 1
        assert reconciliation.count(f"RegisterHolotapePlay({alias})") == 1
        assert shutdown.count(f"UnregisterHolotapePlay({alias})") == 1
    assert "SFL02_Track_Vertibot_QuestStartKeyword.SendStoryEvent()" in main
    assert "SFL02_Track_Radio_QuestStartKeyword.SendStoryEvent()" in main
    assert "SFL02_Track.SetStage(500)" in vertibot
    assert "SFL02_Track.SetStage(550)" in vertibot
    assert "SFL02_Track.SetStage(800)" in vertibot
    assert "Fragment_Stage_2000_Item_00" in main
    assert "CompleteObjective(1300)" in _member_body(
        main, "fragment_stage_2000_item_00"
    )


def test_sfl_terminal_advances_bosz01_only_when_already_running():
    terminal = _patch(
        "Fragments:Terminals:TERM_SFL02_Track_VertibotTer_00184A03"
    )
    body = _member_body(terminal, "fragment_terminal_05")

    assert "LCP_BoSZ01.SetValue(1.0)" in body
    assert "playerRef.SetValue(pBoSz01_PlayerKACacheDepot, 1.0)" in body
    guard = "pBoSZ01 != None && pBoSZ01.IsRunning() && !pBoSZ01.IsStageDone(150)"
    assert guard in body
    assert body.index(guard) < body.index("pBoSZ01.SetStage(150)")
    assert "pBoSZ01.Start()" not in body


def test_contract_keeps_recipe_rewards_outside_supported_fo4_reward_set():
    contract = CONTRACT.read_text(encoding="utf-8")
    assert "recipe list\n  `0043470C`" in contract
    assert "recipe BOOK `0043700D`" in contract
    assert contract.count("intentionally unsupported") >= 2
    assert "reward translator before regeneration" in contract
    assert "excluded by shared translation before regeneration" in contract


def test_direct_source_and_live_carrier_findings_are_pinned():
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "shipped client PEX and converted script contain\n  only properties" in contract
    assert "FragmentCount=0" in contract
    for form_id in (
        "0013250C",
        "001324FD",
        "0013250A",
        "0046FABE",
        "003E5CB0",
        "0046FABD",
        "003E994E",
    ):
        assert form_id in contract


def test_live_reward_audit_pins_currency_and_recipe_translation_failures():
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "currency `00000F`" in contract
    assert "legendary-token currency `003F7410`" in contract
    assert "incorrectly binds `0059B7D3` as\n  `RewardCaps`" in contract
    assert "current converted VMAD still binds\n  all three item lists" in contract
    assert "current converted VMAD still binds all four completion items" in contract
