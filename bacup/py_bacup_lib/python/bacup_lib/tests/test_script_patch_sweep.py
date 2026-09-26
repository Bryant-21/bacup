"""One sweep over every script patch instead of a test file per patch.

Each patch is merged into a synthetic skeleton built from its own state names,
so no deployed PEX or game install is needed.
"""
from __future__ import annotations

import re
from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _state_rename_directives,
)

SCRIPT_PATCHES_ROOT = Path(__file__).resolve().parents[1] / "script_patches"
_STATE_RE = re.compile(r"^\s*(?:auto\s+)?state\s+(\w+)", re.IGNORECASE | re.MULTILINE)
_SCRIPTNAME_RE = re.compile(r"^\s*scriptname\b", re.IGNORECASE | re.MULTILINE)


def _synthetic_skeleton(patch: str, renames: list[tuple[str, str]]) -> str:
    rename_sources = {old.lower() for old, _new in renames}
    states = {name.lower() for name in _STATE_RE.findall(patch)} - rename_sources
    lines = ["Scriptname Synthetic extends ObjectReference"]
    lines += [f"Auto State {name}\nEndState" for name in sorted(rename_sources)]
    lines += [f"State {name}\nEndState" for name in sorted(states)]
    return "\n".join(lines) + "\n"


def _patch_problem(path: Path) -> str | None:
    patch = path.read_text(encoding="utf-8")
    if _SCRIPTNAME_RE.search(patch):
        return "declares Scriptname; declarations come from the skeleton"
    members = [(kind, name) for kind, name, _start, _end in _iter_top_level_papyrus_members(patch.splitlines())]
    if len(members) != len(set(members)):
        return "duplicate top-level member"
    renames = _state_rename_directives(patch.splitlines())
    merged = _merge_script_method_patches(_synthetic_skeleton(patch, renames), patch)
    if not renames and _merge_script_method_patches(merged, patch) != merged:
        return "merge is not idempotent"
    return None


def test_every_script_patch_merges_cleanly():
    patches = sorted(SCRIPT_PATCHES_ROOT.rglob("*.psc"))
    assert len(patches) > 1000
    problems = {}
    for path in patches:
        try:
            problem = _patch_problem(path)
        except ValueError as exc:
            problem = str(exc)
        if problem:
            problems[path.relative_to(SCRIPT_PATCHES_ROOT).as_posix()] = problem
    assert not problems, problems
