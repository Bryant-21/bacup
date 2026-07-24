from __future__ import annotations

import os
import re
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

# Every script patched by shard
# w2-default-interaction-helpers-activator-enable-disable-toggle-linked-ref,
# mapped to the top-level member(s) its patch must supply. State-scoped members
# (DefaultExplosionOnActivate's startstate/donestate OnActivate overrides) are
# verified separately below via _state_block, matching test_two_state_sync's
# pattern (_iter_top_level_papyrus_members only sees top-level members).
#
# DefaultKeypadTargetScript, DefaultOnReadAddToMap, and
# defaultunlockandopenlinkonactivate are deterministic guarded one-shot
# handlers (no named states/timers) — a full compile of the merged patch is
# sufficient coverage for them; see repair-papyrus-stubs SKILL.md's
# dedicated-test-file criteria. DefaultExplosionOnActivate (named-state
# reentry lock) and DefaultLockRefOnUnload (armed countdown timer) are
# genuinely stateful and keep their detailed regression tests below.
PATCH_CASES = {
    "DefaultExplosionOnActivate": {"onactivate"},
    "DefaultKeypadTargetScript": {"oninit"},
    "DefaultLockRefOnUnload": {"onunload", "ontimer"},
    "DefaultOnReadAddToMap": {"onread"},
    "defaultunlockandopenlinkonactivate": {"onactivate"},
}


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


def _state_block(source: str, state_name: str) -> str:
    """Return the text between `State <state_name>` and its matching `EndState`."""
    pattern = re.compile(
        rf"^[ \t]*(?:Auto\s+)?State\s+{re.escape(state_name)}\b[^\n]*\n"
        rf"(?P<body>.*?)"
        rf"^[ \t]*EndState\b",
        re.IGNORECASE | re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(source)
    assert match is not None, f"state {state_name!r} not found in merged source"
    return match.group("body")


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_patch_supplies_confirmed_members(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert expected_members <= _member_names(patch)


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_source_has_single_scriptname_line(script_name: str):
    merged = _merged_source(script_name)
    assert merged.lower().count("scriptname ") == 1


def test_explosion_on_activate_sequences_wait_then_placeatme_then_donestate():
    merged = _merged_source("DefaultExplosionOnActivate")
    assert merged.count("Event OnActivate(") == 3  # top-level + 2 state overrides

    goto_start = merged.find('GoToState("startstate")')
    wait_call = merged.find("Utility.Wait(")
    place_call = merged.find("PlaceAtMe(myExplosion)")
    goto_done = merged.find('GoToState("donestate")')
    assert -1 not in (goto_start, wait_call, place_call, goto_done)
    assert goto_start < wait_call < place_call < goto_done


def test_explosion_on_activate_named_states_block_reentry():
    merged = _merged_source("DefaultExplosionOnActivate")
    startstate = _state_block(merged, "startstate")
    donestate = _state_block(merged, "donestate")
    assert "Event OnActivate(" in startstate
    assert "Event OnActivate(" in donestate
    # Busy-lock / terminal-lock overrides are no-ops: no explosion/state-change
    # calls leak into either named-state override.
    assert "PlaceAtMe" not in startstate
    assert "PlaceAtMe" not in donestate


def test_lock_ref_on_unload_arms_timer_unless_prevent_relock_keyword_present():
    merged = _merged_source("DefaultLockRefOnUnload")
    onunload = merged[
        merged.index("Event OnUnload(") : merged.index("Event OnTimer(")
    ]
    assert "HasKeyword(PreventRelockKeyword)" in onunload
    assert "StartTimer(iCountdownTimerLength, iCountdownTimerID)" in onunload

    ontimer = merged[merged.index("Event OnTimer(") :]
    assert "aiTimerID == iCountdownTimerID" in ontimer
    assert "Lock()" in ontimer


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
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
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
