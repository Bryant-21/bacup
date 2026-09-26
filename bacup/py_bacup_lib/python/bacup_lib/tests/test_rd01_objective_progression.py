from pathlib import Path
import re

from bacup_lib.workflows.unified import _merge_script_method_patches


PATCH = Path(__file__).parents[1] / "script_patches/Fragments/Quests/QF_RD01_GleamingDepths_0078DA2A.psc"


def test_encounter_start_completes_exploration_and_reload_does_not_redisplay_it():
    source = _merge_script_method_patches(
        "Scriptname Fragments:Quests:QF_RD01_GleamingDepths_0078DA2A Extends Quest\n",
        PATCH.read_text(encoding="utf-8"),
    )

    def body(name):
        match = re.search(rf"Function {name}\([^\n]*\)\s*(.*?)EndFunction", source, re.S)
        assert match, name
        return match[1]

    start = body("StartEncounterCheckpoint")
    assert start.index("SetObjectiveCompleted(aiDefeatObjective - 5)") < start.index(
        "SetObjectiveDisplayed(aiDefeatObjective)"
    )
    for stage, objective in ((175, 15), (250, 25), (350, 35), (550, 55), (650, 65)):
        assert f"StartEncounterCheckpoint({objective}," in body(f"Fragment_Stage_{stage:04d}_Item_00")

    redisplay = body("RedisplayEncounterObjectives")
    started, unstarted = redisplay.split("Else", 1)
    assert "If IsStageDone(aiStartStage)" in started
    assert "SetObjectiveCompleted(aiExploreObjective)" in started
    assert "SetObjectiveDisplayed(aiExploreObjective + 5)" in started
    assert "SetObjectiveDisplayed(aiExploreObjective)" not in started
    assert "SetObjectiveDisplayed(aiExploreObjective)" in unstarted
