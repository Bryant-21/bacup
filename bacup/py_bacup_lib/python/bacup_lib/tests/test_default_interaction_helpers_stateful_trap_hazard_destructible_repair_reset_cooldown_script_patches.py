from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_papyrus_members_in_state,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
PARENT_SOURCE_DIR = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
# Fallback source for scripts not (yet) present in the deployed conversion output —
# the raw FO76-shipped PEX. Valid as a skeleton source here because these are
# property-only/hollow scripts where the FO76 and expected FO4 interface (Scriptname/
# Extends/properties) are identical; see the shard contract's program-wide note.
RAW_FO76_SCRIPTS_DIR = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"

# script_name -> (deployed pex filename, expected new/repaired top-level member names)
#
# Only DefaultDestructibleMultiStateActivator is genuinely stateful (a named
# "Destroyed" state with a repair-detection branch); the other six are
# deterministic guarded one-shot OnDeath/OnInit/OnCellLoad/OnTriggerEnter
# handlers, so a full compile of the merged patch is sufficient coverage for
# them — see repair-papyrus-stubs SKILL.md's dedicated-test-file criteria.
PATCH_CASES = {
    "DefaultActorSetAVOnDeathInstOwner": (
        "defaultactorsetavondeathinstowner.pex",
        {("event", "ondeath"), ("event", "ondying"), ("function", "applyavondeath")},
    ),
    "defaultkillmylinkedrefondeathscript": (
        "defaultkillmylinkedrefondeathscript.pex",
        {("event", "ondeath")},
    ),
    "DefaultDestructibleMultiStateActivator": (
        "defaultdestructiblemultistateactivator.pex",
        {
            ("event", "ondestructionstagechanged"),
            ("function", "setlocalstatebyname"),
        },
    ),
    "DefaultRefDestroyOnLoad": (
        "defaultrefdestroyonload.pex",
        {("event", "oncellload")},
    ),
    "DefaultRefKillTriggerScript": (
        "defaultrefkilltriggerscript.pex",
        {("event", "ontriggerenter")},
    ),
    "DefaultRepairableActorScript": (
        "defaultrepairableactorscript.pex",
        {("event", "oninit")},
    ),
    "DefaultTriggerRespawnActorGroup": (
        "defaulttriggerrespawnactorgroup.pex",
        {("function", "addplayerasvip"), ("function", "removeplayerasvip")},
    ),
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


def _production_skeleton(pex_filename: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_DIR / pex_filename
    if not pex_path.is_file():
        pex_path = RAW_FO76_SCRIPTS_DIR / pex_filename
    if not pex_path.is_file():
        pytest.skip(f"production PEX unavailable in deployed or raw-FO76 dirs: {pex_filename}")
    return decompile_pex(pex_path, fo4_api_compat=True)


def _patch_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged_production_source(script_name: str, pex_filename: str) -> str:
    return _merge_script_method_patches(
        _production_skeleton(pex_filename), _patch_source(script_name)
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_exists_with_no_scriptname_line(script_name: str):
    patch = _patch_source(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_declares_no_states_beyond_the_hollow_skeleton(script_name: str):
    # The merger cannot safely introduce new states — every state named in the
    # patch must already exist (empty, per the worksheet skeleton) in the
    # production skeleton. Only DefaultDestructibleMultiStateActivator declares a
    # named state (the pre-existing empty "Destroyed" state).
    pex_filename = PATCH_CASES[script_name][0]
    skeleton_states = {
        name for name, _start, _end in _iter_papyrus_states(_production_skeleton(pex_filename).splitlines())
    }
    patch_states = {
        name for name, _start, _end in _iter_papyrus_states(_patch_source(script_name).splitlines())
    }
    assert patch_states <= skeleton_states


@pytest.mark.parametrize(("script_name", "case"), PATCH_CASES.items())
def test_patch_supplies_every_repaired_member(script_name: str, case: tuple[str, set[tuple[str, str]]]):
    _pex_filename, expected_members = case
    patch_lines = _patch_source(script_name).splitlines()

    found: set[tuple[str, str]] = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(patch_lines)
    }
    for state_name, start, end in _iter_papyrus_states(patch_lines):
        found |= {
            (kind, name)
            for kind, name, _start, _end in _iter_papyrus_members_in_state(patch_lines, start, end)
        }

    assert expected_members <= found


def test_destructible_multi_state_activator_repair_branch_is_state_scoped():
    """Regression guard: the repair-detection branch (aiCurrentStage == 0) must live
    inside the existing "Destroyed" state, not the default state — otherwise it can
    never fire once the object has actually transitioned to Destroyed, since named
    states only dispatch events declared in that same state name."""
    patch_lines = _patch_source("DefaultDestructibleMultiStateActivator").splitlines()
    states = {name: (start, end) for name, start, end in _iter_papyrus_states(patch_lines)}
    assert "destroyed" in states
    start, end = states["destroyed"]
    body = "\n".join(patch_lines[start : end + 1])
    assert "aiCurrentStage == 0" in body
    assert 'GoToState("")' in body

    top_level = "\n".join(patch_lines[: start])
    assert "aiCurrentStage > 0" in top_level
    assert 'GoToState("Destroyed")' in top_level


@pytest.mark.parametrize(("script_name", "case"), PATCH_CASES.items())
def test_production_merge_native_compiles_for_fo4(script_name: str, case: tuple[str, set[tuple[str, str]]]):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not PARENT_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")

    pex_filename, _expected_members = case
    result = compile_psc(
        _merged_production_source(script_name, pex_filename),
        # FO4 base first, then the mod's own generated custom-parent source
        # (covers DefaultDestructibleMultiStateActivator's parent
        # DefaultMultiStateActivator if/when it is decompiled to Source/User),
        # then the deployed compiled Scripts dir for PEX-only parents.
        imports=[str(base_source), str(PARENT_SOURCE_DIR), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
