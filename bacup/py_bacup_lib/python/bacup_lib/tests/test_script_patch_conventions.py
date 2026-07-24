"""Mechanizes SHARD_PROTOCOL.md lesson #11: FO4 never dispatches
OnItemAdded/OnItemRemoved to a script that hasn't called
AddInventoryEventFilter. Three independent shards shipped patches with bare
handlers this wave, caught only by reviewer proofreading — this sweeps every
patch under script_patches/ so the next one fails CI instead.
"""
from __future__ import annotations

import re
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_PATCHES_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)

_INVENTORY_EVENT_RE = re.compile(r"\bEvent\s+OnItem(?:Added|Removed)\b", re.IGNORECASE)
_FILTER_CALL_RE = re.compile(r"\bAddInventoryEventFilter\s*\(", re.IGNORECASE)


def _strip_papyrus_comments(text: str) -> str:
    """Strip `;` line comments and `;/ ... /;` block comments (which may span
    multiple lines), preserving line boundaries so line-based regexes still work."""
    out: list[str] = []
    in_block = False
    for line in text.splitlines():
        parts: list[str] = []
        i = 0
        n = len(line)
        while i < n:
            if in_block:
                end = line.find("/;", i)
                if end == -1:
                    i = n
                else:
                    in_block = False
                    i = end + 2
                continue
            semi = line.find(";", i)
            if semi == -1:
                parts.append(line[i:])
                break
            parts.append(line[i:semi])
            if semi + 1 < n and line[semi + 1] == "/":
                in_block = True
                i = semi + 2
            else:
                break
        out.append("".join(parts))
    return "\n".join(out)


def has_unregistered_inventory_handler(psc_text: str) -> bool:
    """True if `psc_text` declares an OnItemAdded/OnItemRemoved member but
    never calls AddInventoryEventFilter anywhere in the same file (comments
    excluded)."""
    stripped = _strip_papyrus_comments(psc_text)
    if not _INVENTORY_EVENT_RE.search(stripped):
        return False
    return not _FILTER_CALL_RE.search(stripped)


def _all_patch_files() -> list[Path]:
    return sorted(SCRIPT_PATCHES_ROOT.rglob("*.psc"))


def _candidate_files() -> list[Path]:
    """.psc files whose comment-stripped text declares an inventory handler —
    the only files the convention applies to."""
    candidates = []
    for path in _all_patch_files():
        text = path.read_text(encoding="utf-8")
        if _INVENTORY_EVENT_RE.search(_strip_papyrus_comments(text)):
            candidates.append(path)
    return candidates


# --- unit tests for the checker itself --------------------------------------


def test_checker_flags_bare_handler():
    fake_patch = """
Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
"""
    assert has_unregistered_inventory_handler(fake_patch)


def test_checker_flags_bare_on_item_added_handler():
    fake_patch = """
Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
"""
    assert has_unregistered_inventory_handler(fake_patch)


def test_checker_allows_registered_handler():
    fake_patch = """
Event OnInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
"""
    assert not has_unregistered_inventory_handler(fake_patch)


def test_checker_ignores_commented_out_handler():
    fake_patch = """
; Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
;     Foo()
; EndEvent
"""
    assert not has_unregistered_inventory_handler(fake_patch)


def test_checker_ignores_block_commented_handler():
    fake_patch = """
;/
Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
/;
"""
    assert not has_unregistered_inventory_handler(fake_patch)


def test_checker_ignores_files_without_inventory_handlers():
    fake_patch = """
Event OnActivate(ObjectReference akActionRef)
    Foo()
EndEvent
"""
    assert not has_unregistered_inventory_handler(fake_patch)


# --- program-wide sweep ------------------------------------------------------


def test_script_patches_directory_is_discoverable():
    assert SCRIPT_PATCHES_ROOT.is_dir()
    assert _all_patch_files(), "expected at least one .psc file under script_patches/"


@pytest.mark.parametrize(
    "psc_path",
    _candidate_files(),
    ids=lambda p: str(p.relative_to(SCRIPT_PATCHES_ROOT)),
)
def test_inventory_handler_registers_filter(psc_path: Path):
    text = psc_path.read_text(encoding="utf-8")
    assert not has_unregistered_inventory_handler(text), (
        f"{psc_path.relative_to(SCRIPT_PATCHES_ROOT)} declares an "
        "OnItemAdded/OnItemRemoved handler but never calls "
        "AddInventoryEventFilter(...) anywhere in the file — "
        "see SHARD_PROTOCOL.md lesson #11."
    )
