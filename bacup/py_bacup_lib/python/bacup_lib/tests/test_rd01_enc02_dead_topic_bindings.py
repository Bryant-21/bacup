from pathlib import Path

from bacup_lib.workflows.unified import _merge_script_method_patches


PATCH = (
    Path(__file__).parents[1]
    / "script_patches"
    / "Raids"
    / "RD01"
    / "Enc02"
    / "QuestScript.psc"
)


def test_rd01_enc02_patch_drops_only_dead_topic_declarations():
    skeleton = """Scriptname Raids:RD01:Enc02:QuestScript Extends Quest

Int Property iMaxDifficulty Auto Mandatory
Topic Property kDifficultyMaxedTopic Auto Mandatory
Topic Property kDifficultyIncreasedTopic Auto Mandatory
Topic Property kEncounterStartTopic Auto Mandatory
"""

    merged = _merge_script_method_patches(skeleton, PATCH.read_text(encoding="utf-8"))

    assert "Property iMaxDifficulty" in merged
    assert "kDifficultyMaxedTopic" not in merged
    assert "kDifficultyIncreasedTopic" not in merged
    assert "kEncounterStartTopic" not in merged
