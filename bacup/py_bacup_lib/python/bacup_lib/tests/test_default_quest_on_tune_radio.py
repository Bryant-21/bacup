"""The shared `DefaultQuestOnTuneRadioScript`, 7 live bindings.

The generated PSC has the properties and no members, so tuning the radio never
advances anything; on WK146 Cheating Death that leaves stage 800 -> 860 dead.

Neither game raises an event on a station change. The FO76 script's only
variables, `timerSeconds = 2.0` and `timerID = 1`, describe a poll, and Fallout 4
exposes the same reading through `Game.IsPlayerRadioOn()` /
`Game.GetPlayerRadioFrequency()`. The transmitters survive conversion (90 placed
refs carry an `XRDO` frequency, including `0040D4E3` at 92.5), so this is a
recovery, not a substitute.

Compiled with stock `PapyrusCompiler.exe` and symbols checked through the Papyrus
LSP, because the native compiler accepts unknown identifiers and member calls.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

from bacup_lib.workflows import unified
from creation_lib.papyrus_lsp import ScriptDB

SCRIPT = "DefaultQuestOnTuneRadioScript"
GENERATED_USER_SCRIPTS = (
    Path(__file__).resolve().parents[5]
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
)
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


def _merged() -> str:
    skeleton = (
        GENERATED_USER_SCRIPTS / unified._script_relative_path(SCRIPT, ".psc")
    ).read_text(encoding="utf-8")
    skeleton = unified._augment_fo76_to_fo4_script_skeleton(SCRIPT, skeleton)
    return unified._merge_script_method_patches(skeleton, _patch())


def _body(source: str, header: str) -> str:
    return source.split(header, 1)[1].split("EndFunction", 1)[0]


def test_the_station_check_uses_the_native_fo4_radio_reading() -> None:
    body = _body(_patch(), "Bool Function PlayerIsTunedToStation()")
    assert "Game.IsPlayerRadioOn()" in body
    assert "Game.GetPlayerRadioFrequency()" in body
    assert "RadioFrequency" in body
    # Authored to one decimal place; equality on a float round-trip would miss.
    assert "Math.Abs" in body and "< 0.05" in body


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


def test_the_watch_is_re_evaluated_on_every_stage_change() -> None:
    patch = _patch()
    assert "Event OnQuestInit()" in patch
    assert "Event OnStageSet(Int auiStageID, Int auiItemID)" in patch
    assert patch.count("RefreshRadioWatch()") >= 3
    # Save/load: the quest re-runs OnStageSet-driven refresh rather than
    # trusting a timer that a reload dropped.
    assert "Event OnQuestShutdown()" in patch


def test_the_patch_merges_once_into_the_shipped_skeleton() -> None:
    merged = _merged()
    patch_members = {
        (kind, name)
        for kind, name, _s, _e in unified._iter_top_level_papyrus_members(
            _patch().splitlines()
        )
    }
    merged_members = [
        (kind, name)
        for kind, name, _s, _e in unified._iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert patch_members <= set(merged_members)
    for member in patch_members:
        assert merged_members.count(member) == 1
    assert merged.count("Scriptname ") == 1
    assert unified._merge_script_method_patches(merged, _patch()) == merged


def test_symbols_resolve_through_the_papyrus_lsp(tmp_path) -> None:
    root = tmp_path / "src"
    root.mkdir()
    (root / f"{SCRIPT}.psc").write_text(_merged(), encoding="utf-8")
    db = ScriptDB(str(tmp_path / "lsp.db"), source_dirs=[str(root)])
    try:
        assert db.get_extends(SCRIPT).lower() == "quest"
        for name in ("StageToSet", "TurnOffStage", "PrereqStage", "RadioFrequency"):
            assert db.has_property(SCRIPT, name), name
        for name in (
            "RefreshRadioWatch",
            "RadioWatchIsLive",
            "PlayerIsTunedToStation",
        ):
            assert db.has_function(SCRIPT, name), name
        assert db.has_event(SCRIPT, "OnTimer")
        assert db.has_event(SCRIPT, "OnStageSet")
        # Negative control.
        assert not db.has_property(SCRIPT, "RadioFrequencyZ")
    finally:
        db.close()


def _stock_compiler() -> tuple[Path, Path] | None:
    raw = os.environ.get("FO4_DIR", "").strip().strip('"')
    if not raw:
        env_file = Path(__file__).resolve().parents[5] / ".env"
        if env_file.is_file():
            for line in env_file.read_text(encoding="utf-8").splitlines():
                key, _, value = line.partition("=")
                if key.strip() == "FO4_DIR":
                    raw = value.strip().strip('"')
                    break
    if not raw:
        return None
    root = Path(raw)
    compiler = root / "Papyrus Compiler" / "PapyrusCompiler.exe"
    base = root / "Data" / "Scripts" / "Source" / "Base"
    if not compiler.is_file() or not base.is_dir():
        return None
    return compiler, base


@pytest.mark.skipif(
    _stock_compiler() is None,
    reason="FO4 PapyrusCompiler.exe / Base scripts unavailable",
)
def test_merged_script_compiles_under_stock_papyruscompiler(tmp_path) -> None:
    compiler, base = _stock_compiler()
    user = tmp_path / "User"
    user.mkdir()
    (user / f"{SCRIPT}.psc").write_text(_merged(), encoding="utf-8")
    out = tmp_path / "out"
    out.mkdir()
    result = subprocess.run(
        [
            str(compiler),
            SCRIPT,
            f"-import={user};{base}",
            f"-output={out}",
            "-f=Institute_Papyrus_Flags.flg",
        ],
        cwd=str(user),
        capture_output=True,
        text=True,
    )
    assert (out / f"{SCRIPT}.pex").is_file(), result.stdout + result.stderr
