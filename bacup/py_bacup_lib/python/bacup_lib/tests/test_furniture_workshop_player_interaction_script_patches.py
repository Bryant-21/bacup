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
# w2-furniture-workshop-player-interaction-ungrouped-furniture-workshop-player-interaction,
# mapped to the top-level member(s) its patch must supply. MTRZ05MiningSiteScript's
# OnActivate is state-scoped (inside "open") and is verified separately below via
# _state_block, matching test_two_state_sync's / the default-interaction-helpers
# shard's pattern (_iter_top_level_papyrus_members only sees top-level members).
#
# AddSpellOnFurnitureEnter and OverseersCampToolboxScript are deterministic
# guarded one-shot/paired-event handlers (no named states) — a full compile of
# the merged patch is sufficient coverage for them; see repair-papyrus-stubs
# SKILL.md's dedicated-test-file criteria. MTRZ05MiningSiteScript's open/used
# state machine keeps its detailed regression test below.
PATCH_CASES = {
    "AddSpellOnFurnitureEnter": {"onactivate", "onexitfurniture"},
    "MTRZ05MiningSiteScript": {"onload"},
    "OverseersCampToolboxScript": {"oninit", "onload", "onitemremoved"},
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


def test_mtrz05_mining_site_gates_used_sites_out_of_the_open_state():
    merged = _merged_source("MTRZ05MiningSiteScript")
    # The merger appends unmatched top-level members after existing state blocks,
    # so OnLoad (new) lands after the skeleton's pre-existing "open" state.
    onload = merged[merged.index("Event OnLoad(") :]
    assert "parent.OnLoad()" in onload
    assert "MTRZ05MiningSiteUsedKeyword" in onload
    assert 'GoToState("open")' in onload

    open_state = _state_block(merged, "open")
    assert "Event OnActivate(" in open_state
    assert "MTRZ05MiningSiteInUseKeyword" in open_state
    assert "SetOpen(True)" in open_state
    assert "MTRZ05MiningSiteUsedKeyword" in open_state


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
