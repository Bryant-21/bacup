"""Mechanizes SHARD_PROTOCOL.md lesson #11: FO4 never dispatches
OnItemAdded/OnItemRemoved to a script that hasn't called
AddInventoryEventFilter on the script or member reference that actually
receives the event. This sweeps every patch under script_patches/ so bare
handlers and filters installed on the wrong receiver fail CI instead.
"""
from __future__ import annotations

import re
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_PATCHES_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)

_INVENTORY_EVENT_RE = re.compile(
    r"\bEvent\s+(?:(?P<qualifier>[A-Za-z_]\w*(?::[A-Za-z_]\w*)*)\s*\.\s*)?"
    r"OnItem(?:Added|Removed)\s*\((?P<parameters>[^)]*)\)",
    re.IGNORECASE | re.DOTALL,
)
_FILTER_CALL_RE = re.compile(r"\bAddInventoryEventFilter\s*\(", re.IGNORECASE)
_END_EVENT_RE = re.compile(r"\bEndEvent\b", re.IGNORECASE)
_PARAMETER_RE = re.compile(
    r"\s*(?P<type>[A-Za-z_]\w*(?::[A-Za-z_]\w*)*)\s+[A-Za-z_]\w*\s*",
    re.IGNORECASE,
)
_REF_COLLECTION_INVENTORY_PARAMETERS = (
    "objectreference",
    "form",
    "int",
    "objectreference",
    "objectreference",
)


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


def _strip_inventory_comments_and_strings(text: str) -> str:
    out: list[str] = []
    index = 0
    in_block_comment = False
    in_string = False
    escaped = False
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""
        if in_block_comment:
            out.append("\n" if char == "\n" else " ")
            if char == "/" and next_char == ";":
                out.append(" ")
                in_block_comment = False
                index += 2
            else:
                index += 1
            continue
        if in_string:
            out.append("\n" if char == "\n" else " ")
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == '"':
            out.append(" ")
            in_string = True
        elif char == ";" and next_char == "/":
            out.extend((" ", " "))
            in_block_comment = True
            index += 2
            continue
        elif char == ";":
            while index < len(text) and text[index] not in "\r\n":
                out.append(" ")
                index += 1
            continue
        else:
            out.append(char)
        index += 1
    return "".join(out)


def _parameter_types(parameters: str) -> tuple[str, ...] | None:
    parts = parameters.split(",")
    matches = [_PARAMETER_RE.fullmatch(part) for part in parts]
    if any(match is None for match in matches):
        return None
    return tuple(match.group("type").casefold() for match in matches if match)


def _filter_is_receiver_owned(source: str, match: re.Match[str]) -> bool:
    prefix = source[: match.start()].rstrip()
    if not prefix.endswith("."):
        return True
    receiver = re.search(r"([A-Za-z_]\w*)\s*$", prefix[:-1])
    return receiver is not None and receiver.group(1).casefold() == "self"


def _handler_body_is_empty(source: str, handler: re.Match[str]) -> bool:
    # An empty handler only silences the skeleton's body, so it loses nothing
    # when FO4 never dispatches it.
    end = _END_EVENT_RE.search(source, handler.end())
    return end is not None and not source[handler.end() : end.start()].strip()


def has_unregistered_inventory_handler(psc_text: str) -> bool:
    """True when an inventory handler has no filter on its actual receiver."""
    stripped = _strip_inventory_comments_and_strings(psc_text)
    handlers = [
        handler
        for handler in _INVENTORY_EVENT_RE.finditer(stripped)
        if not _handler_body_is_empty(stripped, handler)
    ]
    if not handlers:
        return False

    filters = list(_FILTER_CALL_RE.finditer(stripped))
    has_receiver_owned_filter = any(
        _filter_is_receiver_owned(stripped, match) for match in filters
    )
    has_member_owned_filter = any(
        not _filter_is_receiver_owned(stripped, match) for match in filters
    )

    for handler in handlers:
        is_ref_collection_member_handler = (
            handler.group("qualifier") is None
            and _parameter_types(handler.group("parameters"))
            == _REF_COLLECTION_INVENTORY_PARAMETERS
        )
        if is_ref_collection_member_handler:
            if not has_member_owned_filter:
                return True
        elif not has_receiver_owned_filter:
            return True
    return False


def _all_patch_files() -> list[Path]:
    return sorted(SCRIPT_PATCHES_ROOT.rglob("*.psc"))


def _candidate_files() -> list[Path]:
    """.psc files whose comment-stripped text declares an inventory handler —
    the only files the convention applies to."""
    candidates = []
    for path in _all_patch_files():
        text = path.read_text(encoding="utf-8")
        if _INVENTORY_EVENT_RE.search(_strip_inventory_comments_and_strings(text)):
            candidates.append(path)
    return candidates


# --- unit tests for the checker itself --------------------------------------


@pytest.mark.parametrize(
    "fake_patch",
    [
        pytest.param(
            """
Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="flags_bare_handler",
        ),
        pytest.param(
            """
Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
""",
            id="flags_bare_on_item_added_handler",
        ),
        pytest.param(
            """
Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
""",
            id="flags_bare_qualified_remote_handler",
        ),
        pytest.param(
            """
Event OnQuestInit()
    ObjectReference player = Game.GetPlayer()
    player.AddInventoryEventFilter(TargetItem)
    RegisterForRemoteEvent(player, "OnItemRemoved")
EndEvent

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="flags_filter_registered_on_remote_sender",
        ),
        pytest.param(
            """
Event OnQuestInit()
    PlayerRefs[0].AddInventoryEventFilter(TargetItem)
    RegisterForRemoteEvent(PlayerRefs[0], "OnItemRemoved")
EndEvent

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="flags_complex_filter_registered_on_remote_sender",
        ),
        pytest.param(
            """
Function ArmMembers()
    ObjectReference memberRef = GetAt(0)
    memberRef.AddInventoryEventFilter(None)
EndFunction

Event OnItemRemoved(ObjectReference akSenderRef, Form akBaseItem, int aiItemCount, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="requires_exact_ref_collection_five_argument_shape",
        ),
        pytest.param(
            """
Event OnQuestInit()
    Debug.Trace("AddInventoryEventFilter(None)")
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
""",
            id="does_not_count_filter_name_in_string_literal",
        ),
        pytest.param(
            """
Event OnQuestInit()
    Debug.Trace(";/")
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
""",
            id="does_not_treat_comment_marker_in_string_as_a_real_comment",
        ),
        pytest.param(
            """
Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="flags_non_empty_handler_beside_empty_override",
        ),
    ],
)
def test_checker_flags_unregistered_handler(fake_patch: str):
    assert has_unregistered_inventory_handler(fake_patch)


@pytest.mark.parametrize(
    "fake_patch",
    [
        pytest.param(
            """
Event OnInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="allows_registered_handler",
        ),
        pytest.param(
            """
Event OnQuestInit()
    AddInventoryEventFilter(TargetItem)
    RegisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
""",
            id="allows_receiver_owned_filter_for_qualified_remote_handler",
        ),
        pytest.param(
            """
Event OnQuestInit()
    Self.AddInventoryEventFilter(TargetItem)
    RegisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Foo()
EndEvent
""",
            id="allows_explicit_self_filter_for_qualified_remote_handler",
        ),
        pytest.param(
            """
Function ArmMembers()
    ObjectReference memberRef = GetAt(0)
    memberRef.AddInventoryEventFilter(None)
EndFunction

Event OnItemRemoved(ObjectReference akSenderRef, Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
""",
            id="allows_ref_collection_member_filtering",
        ),
        pytest.param(
            """
Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    Foo()
EndEvent
""",
            id="does_not_treat_on_container_changed_as_inventory_handler",
        ),
        pytest.param(
            """
Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    ; counted by the quest instead
EndEvent
""",
            id="allows_empty_override_handler_without_filter",
        ),
        pytest.param(
            """
; Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
;     Foo()
; EndEvent
""",
            id="ignores_commented_out_handler",
        ),
        pytest.param(
            """
;/
Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Foo()
EndEvent
/;
""",
            id="ignores_block_commented_handler",
        ),
        pytest.param(
            """
Event OnActivate(ObjectReference akActionRef)
    Foo()
EndEvent
""",
            id="ignores_files_without_inventory_handlers",
        ),
    ],
)
def test_checker_allows_registered_or_absent_handler(fake_patch: str):
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


# --- sibling-script `Self as <ScriptType>` casts ------------------------------
# Papyrus has no sibling-script cast: two scripts attached to the same quest (or
# the same reference) are unrelated types. Stock PapyrusCompiler.exe rejects
# `Self as OtherScript` with "types are incompatible" and drops the WHOLE file,
# so one bad cast silently costs a quest all of its fragments. The supported form
# hops through the shared ancestor: `(Self as Quest) as OtherScript`.

_SELF_CAST_RE = re.compile(
    r"\bSelf\s+as\s+([A-Za-z_][A-Za-z0-9_]*(?::[A-Za-z_][A-Za-z0-9_]*)*)",
    re.IGNORECASE,
)

# Engine types Self may legally be cast to. Anything else is a script type.
_ENGINE_CAST_TARGETS = frozenset(
    name.lower()
    for name in (
        "ScriptObject", "Form", "ObjectReference", "Actor", "Quest",
        "ReferenceAlias", "RefCollectionAlias", "LocationAlias", "Alias",
        "ActiveMagicEffect", "TrapBase", "Scene",
        "Bool", "Int", "Float", "String", "Var",
    )
)

# Script ancestors of patches that upcast Self. Patch fragments carry no
# `Extends` line, so the skeleton's parent is recorded here; an upcast to it is
# a legal cast, not a sibling one.
_PATCH_SCRIPT_ANCESTORS = {
    "DeconArchScript.psc": ("Default2StateActivator",),
    "DefaultFixable2StateActivator.psc": ("Default2StateActivator",),
}


def sibling_script_casts(
    psc_text: str, ancestors: tuple[str, ...] = ()
) -> list[tuple[int, str]]:
    """(line number, target type) for every `Self as <ScriptType>` cast whose
    target is neither an engine base type nor one of the script's `ancestors`
    (comments excluded)."""
    allowed = _ENGINE_CAST_TARGETS | {name.lower() for name in ancestors}
    hits: list[tuple[int, str]] = []
    for lineno, line in enumerate(_strip_papyrus_comments(psc_text).splitlines(), 1):
        for match in _SELF_CAST_RE.finditer(line):
            target = match.group(1)
            if target.lower() not in allowed:
                hits.append((lineno, target))
    return hits


@pytest.mark.parametrize(
    "source,ancestors,expected",
    [
        pytest.param(
            "Function F()\n    DefaultQuestEncounterWaveScript w = Self as DefaultQuestEncounterWaveScript\nEndFunction\n",
            (),
            [(2, "DefaultQuestEncounterWaveScript")],
            id="direct_cast",
        ),
        pytest.param(
            "    Quests:MTR04:Chow c = Self as Quests:MTR04:Chow\n",
            (),
            [(1, "Quests:MTR04:Chow")],
            id="namespaced_cast",
        ),
        pytest.param(
            "    If (Self as Default2StateActivator).isOpen\n",
            (),
            [(1, "Default2StateActivator")],
            id="undeclared_ancestor",
        ),
        pytest.param(
            "    If (Self as Default2StateActivator).isOpen\n",
            ("Default2StateActivator",),
            [],
            id="declared_ancestor",
        ),
        pytest.param(
            "    DefaultQuestEncounterWaveScript w = (Self as Quest) as DefaultQuestEncounterWaveScript\n",
            (),
            [],
            id="ancestor_hop",
        ),
        pytest.param(
            "    Quest q = Self as Quest\n    Int n = (Self as RefCollectionAlias).GetCount()\n"
            "    Scene s = Self as Scene\n",
            (),
            [],
            id="engine_casts",
        ),
        pytest.param(
            "; TWZ11_Script c = Self as TWZ11_Script\n;/\nSelf as SFZ03_Queen_QuestScript\n/;\n",
            (),
            [],
            id="comments",
        ),
    ],
)
def test_sibling_cast_checker(source: str, ancestors: tuple[str, ...], expected):
    assert sibling_script_casts(source, ancestors) == expected


@pytest.mark.parametrize(
    "psc_path",
    _all_patch_files(),
    ids=lambda p: str(p.relative_to(SCRIPT_PATCHES_ROOT)),
)
def test_no_sibling_script_self_cast(psc_path: Path):
    ancestors = _PATCH_SCRIPT_ANCESTORS.get(
        psc_path.relative_to(SCRIPT_PATCHES_ROOT).as_posix(), ()
    )
    hits = sibling_script_casts(psc_path.read_text(encoding="utf-8"), ancestors)
    assert not hits, (
        f"{psc_path.relative_to(SCRIPT_PATCHES_ROOT)} casts Self directly to a "
        "sibling script type at "
        + ", ".join(f"line {n} (`Self as {t}`)" for n, t in hits)
        + " — stock PapyrusCompiler.exe rejects this and drops the entire file. "
        "Hop through the shared ancestor instead, e.g. `(Self as Quest) as "
        "<ScriptType>` for a quest fragment or `(Self as ReferenceAlias) as "
        "<ScriptType>` for an alias script."
    )
