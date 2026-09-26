"""The shared `DefaultQuestOnTuneRadioScript`, 7 live bindings.

The generated PSC has the properties and no members, so tuning the radio never
advances anything; on WK146 Cheating Death that leaves stage 800 -> 860 dead.

Neither game raises an event on a station change. The FO76 script's only
variables, `timerSeconds = 2.0` and `timerID = 1`, describe a poll, and Fallout 4
exposes the same reading through `Game.IsPlayerRadioOn()` /
`Game.GetPlayerRadioFrequency()`. The transmitters survive conversion (90 placed
refs carry an `XRDO` frequency, including `0040D4E3` at 92.5), so this is a
recovery, not a substitute.
"""

from __future__ import annotations

import pytest

from bacup_lib.workflows import unified

SCRIPT = "DefaultQuestOnTuneRadioScript"
# QUST 0040D28D W05_MQR_201P, read from the live plugin.
CHEATING_DEATH = {
    "RadioFrequency": 92.5,
    "preReqStage": 800,
    "StageToSet": 860,
    "TurnOffStage": 860,
}


def _patch() -> str:
    return (
        unified._SCRIPT_PATCH_DIR / unified._script_relative_path(SCRIPT, ".psc")
    ).read_text(encoding="utf-8")


def _body(source: str, header: str) -> str:
    return source.split(header, 1)[1].split("EndFunction", 1)[0]


def test_the_poll_is_the_fo76_cadence_not_an_invented_one() -> None:
    patch = _patch()
    # Both are script variables the decompiled skeleton already declares.
    assert "StartTimer(timerSeconds, timerID)" in patch
    assert "CancelTimer(timerID)" in patch
    assert "If aiTimerID != timerID" in patch
    # FO4 replaced Skyrim's update API; using it would compile natively and
    # never fire.
    for absent in ("RegisterForSingleUpdate", "OnUpdate", "UnregisterForUpdate"):
        assert absent not in patch


@pytest.mark.parametrize(
    "guard",
    [
        # tuning before the prerequisite does nothing
        "If PrereqStage >= 0 && !IsStageDone(PrereqStage)",
        # tuning again afterwards does nothing
        "If TurnOffStage >= 0 && IsStageDone(TurnOffStage)",
        # the stage is set at most once
        "If IsStageDone(StageToSet)",
        # an unconfigured carrier never polls at all
        "If StageToSet < 0 || !IsRunning()",
    ],
)
def test_guard_is_present(guard: str) -> None:
    assert guard in _body(_patch(), "Bool Function RadioWatchIsLive()")


def test_cheating_death_stops_polling_even_though_turnoff_equals_stagetoset() -> None:
    """WK146 binds TurnOffStage == StageToSet == 860.

    `IsStageDone(TurnOffStage)` alone would still be false at the instant 860 is
    set, so the `IsStageDone(StageToSet)` clause is what ends the loop.
    """
    assert CHEATING_DEATH["TurnOffStage"] == CHEATING_DEATH["StageToSet"]
    guards = _body(_patch(), "Bool Function RadioWatchIsLive()")
    assert guards.index("IsStageDone(StageToSet)") < guards.index("TurnOffStage")
