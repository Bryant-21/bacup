from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "RadioGeneral_MasterScript"


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    skeleton = (SOURCE_ROOT / f"{SCRIPT_NAME}.psc").read_text(encoding="utf-8")
    return _merge_script_method_patches(skeleton, patch)


def test_radio_lifecycle_members_merge_once_and_idempotently():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_production_source()
    member_names = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]

    for member_name in (
        "oninit",
        "onquestinit",
        "onstageset",
        "ensurequeuerunning",
    ):
        assert member_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_radio_lifecycle_rearms_stage_10_without_overlapping_playback():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    quest_init = _member_body(patch, "onquestinit")
    stage_set = _member_body(patch, "onstageset")
    ensure_running = _member_body(patch, "ensurequeuerunning")

    assert quest_init.count("EnsureQueueRunning()") == 1
    assert "If auiStageID == 10" in stage_set
    assert stage_set.count("EnsureQueueRunning()") == 1
    assert "lastScenePlayed.IsPlaying()" in ensure_running
    assert ensure_running.index("lastScenePlayed.IsPlaying()") < ensure_running.index(
        "QueueNextScene()"
    )
    assert ensure_running.count("QueueNextScene()") == 1


def test_radio_queue_never_strands_the_station_without_a_pending_timer():
    # EnsureQueueRunning() cancels the failsafe timer before calling
    # QueueNextScene(), so any path out of QueueNextScene() that does not arm a
    # timer leaves the station permanently silent with nothing logged.
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    init = _member_body(patch, "oninit")
    quest_init = _member_body(patch, "onquestinit")
    queue_next = _member_body(patch, "queuenextscene")

    assert "StartTimer(0.1, iFailSafeTimerID)" in init
    assert quest_init.count("EnsureQueueRunning()") == 1
    assert "StartTimer(1.0, iFailSafeTimerID)" not in patch

    queue_next_code = "\n".join(
        line for line in queue_next.splitlines() if not line.strip().startswith(";")
    )

    # IsRunning() is False while the quest is still starting up, so gating the
    # queue on it stalls playback for a whole failsafe interval after every
    # load. The station that has always played has no such gate.
    assert "IsRunning()" not in queue_next_code

    # Every early return must leave a timer pending, since EnsureQueueRunning()
    # cancels the timer before calling in. The only allowed bare Return is the
    # re-entrancy guard, whose in-flight caller arms the timer itself.
    bare_returns = [
        line for line in queue_next_code.splitlines() if line.strip() == "Return"
    ]
    assert len(bare_returns) == 1
    assert "If lock_SceneQueue" in queue_next_code.split("Return")[0]

    # Scene.Start() is asynchronous: gating the bookkeeping on IsPlaying() drops
    # the started scene and lets the next tick stack a second track over it.
    assert "If nextScene.IsPlaying()" not in queue_next
    assert queue_next.index("nextScene.Start()") < queue_next.index(
        "lastScenePlayed = nextScene"
    )


def test_radio_lifecycle_full_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
