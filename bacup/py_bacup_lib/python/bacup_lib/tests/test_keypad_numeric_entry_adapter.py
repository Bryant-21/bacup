"""The FO4 numeric-entry adapter for the three proven keypad contracts.

FO76 opened a native keypad menu and reported success through a native event;
FO4 has neither. Activation is not a result: only a complete, matching code
reaches the quest, only through the keypad's success event, and only after the
alias's prerequisite and actor filters agree.

Runtime behaviour needs a playthrough. These tests check that both patches merge
into the shipped skeletons, the merged scripts compile, and the guards the
negative cases depend on are in the source.
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows import unified

KEYPAD = "DefaultKeypadScript"
ALIAS = "DefaultAliasSetStageOnKeypadSuccess"

_REPO_ROOT = Path(__file__).resolve().parents[5]
_FO76_CLIENT_SCRIPTS = _REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
_FO4_HEADERS = _REPO_ROOT / "extracted" / "fo4" / "scripts" / "GeneratedHeaders"
_GENERATED_USER_SCRIPTS = (
    _REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
)


def _patch_source(script_name: str) -> str:
    path = unified._SCRIPT_PATCH_DIR / unified._script_relative_path(
        script_name, ".psc"
    )
    return path.read_text(encoding="utf-8")


def _source_pex(script_name: str) -> Path | None:
    relative = unified._script_relative_path(script_name, ".pex")
    candidate = _FO76_CLIENT_SCRIPTS / relative
    if candidate.is_file():
        return candidate
    matches = list(_FO76_CLIENT_SCRIPTS.rglob(relative.name))
    return matches[0] if matches else None


def _merged_source(script_name: str) -> str:
    from creation_lib.pex import decompile_pex

    pex = _source_pex(script_name)
    assert pex is not None, f"no FO76 client PEX for {script_name}"
    skeleton = decompile_pex(
        pex,
        type_adapter=unified._fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    skeleton = unified._augment_fo76_to_fo4_script_skeleton(script_name, skeleton)
    return unified._merge_script_method_patches(skeleton, _patch_source(script_name))


requires_corpora = pytest.mark.skipif(
    not _FO76_CLIENT_SCRIPTS.is_dir() or not _FO4_HEADERS.is_dir(),
    reason="FO76 client scripts / FO4 generated headers not extracted",
)


# --- The forbidden shortcut ---

def _code_lines(source: str) -> str:
    return "\n".join(
        line for line in source.splitlines() if not line.lstrip().startswith(";")
    )


def test_alias_never_advances_the_quest_from_activation():
    source = _code_lines(_patch_source(ALIAS))

    assert "OnActivate" not in source
    assert "DefaultKeypadScript.KeypadSuccess" in source


def test_keypad_sets_no_stage_and_owns_no_quest():
    source = _code_lines(_patch_source(KEYPAD))

    assert "SetStage" not in source
    assert "Quest" not in source


# --- A3's negative cases, as source-level guards ---

@pytest.mark.parametrize(
    "guard",
    [
        # 6 — wrong actor never even enters a digit.
        "akActionRef != Game.GetPlayer()",
        # 2/3 — a code shorter than the record's digit count is never evaluated.
        "If iEntryPosition < DigitCount()",
        # 4 — a complete but wrong code is rejected, and clears the buffer.
        "enteredCode != expectedCode",
        # 8 — success is one-shot.
        "If bKeypadSolved",
        # 5/9 — leaving the cell abandons a partial code instead of banking it.
        "Event OnUnload()",
    ],
)
def test_keypad_guard_is_present(guard):
    assert guard in _patch_source(KEYPAD)


@pytest.mark.parametrize(
    "guard",
    [
        # 7 — Crane's preReqStage 700 gate.
        "If preReqStage > 0 && !owningQuest.IsStageDone(preReqStage)",
        # 6 — Vault 79's ActivatedByAliases filter, plus PlayerActivateType.
        "If !ActivatorIsAllowed(Game.GetPlayer())",
        "If PlayerActivateType != 0 && akActivator != Game.GetPlayer()",
        # 8 — the stage is never set twice.
        "If !owningQuest.IsStageDone(StageToSet)",
    ],
)
def test_alias_guard_is_present(guard):
    assert guard in _patch_source(ALIAS)


def test_refused_success_leaves_the_keypad_retryable():
    """Case 7: a correct code before the prerequisite opens nothing.

    The keypad hands its completion to the alias, so a refusal leaves both the
    door locked and the latch clear for a later attempt.
    """
    keypad = _patch_source(KEYPAD)
    alias = _patch_source(ALIAS)

    assert "If !bCompletionDelegated" in keypad
    assert "bKeypadSolved = True" in keypad.split("Function CompleteKeypad()")[1]
    assert "keypad.SetCompletionDelegated(True)" in alias
    # The delegate only completes after every guard above it has passed.
    tail = alias.split("Function ApplyKeypadSuccess")[1]
    assert tail.index("akKeypad.CompleteKeypad()") > tail.index("ActivatorIsAllowed")


# --- Delivery ---

@requires_corpora
@pytest.mark.parametrize("script_name", [KEYPAD, ALIAS])
def test_patch_merges_into_the_shipped_skeleton(script_name):
    merged = _merged_source(script_name)

    for kind, name, _start, _end in unified._iter_top_level_papyrus_members(
        _patch_source(script_name).splitlines()
    ):
        assert (kind, name) in {
            (member_kind, member_name)
            for member_kind, member_name, _s, _e in (
                unified._iter_top_level_papyrus_members(merged.splitlines())
            )
        }


@requires_corpora
@pytest.mark.parametrize("script_name", [KEYPAD, ALIAS])
def test_merged_script_compiles_against_fo4(script_name, tmp_path):
    from creation_lib.pex.native_runtime import compile_psc

    psc = tmp_path / f"{Path(script_name).name}.psc"
    psc.write_text(_merged_source(script_name), encoding="utf-8")
    result = compile_psc(
        psc.read_text(encoding="utf-8"),
        imports=[str(tmp_path), str(_FO4_HEADERS)],
        game="fo4",
        flags=str(_FO4_HEADERS / "Institute_Papyrus_Flags.flg"),
        source_path=str(psc),
    )

    assert result.ok, result.diagnostics


# --- Vault 79's code is dynamic ----------------------------------------------
#
# REFR 0056BA45 binds DefaultKeypadScript with ZERO VMAD properties, and its
# base ACTI 0038A8E2 carries no VMAD at all, so presetCode is 0 and KeypadCode
# is None. The code lives in AVIF 005698F3 W05_MQ00_CodeAV (Type Variable,
# default -1.0), which the sibling script on the same reference does bind.


def test_vault79_reads_the_code_through_the_sibling_script() -> None:
    keypad = _patch_source(KEYPAD)
    assert "Return DynamicCode()" in keypad
    body = keypad.split("Int Function DynamicCode()")[1]
    # Sibling scripts are unrelated types; stock PapyrusCompiler.exe rejects a
    # direct cast, so the route goes through the shared ObjectReference base.
    assert "ObjectReference keypadRef = Self as ObjectReference" in body
    assert "keypadRef as W05_Vaut79EntranceKeypadScript" in body
    assert "playerRef.GetValue(vaultKeypad.W05_MQ00_CodeAV)" in body
    # No FormID is hard-coded into a script 26 keypads share.
    assert "GetFormFromFile" not in keypad


def test_the_dynamic_fallback_still_fails_closed() -> None:
    """Until a producer sets the AV it reads -1, and entry never starts."""
    keypad = _patch_source(KEYPAD)
    body = keypad.split("Int Function DynamicCode()")[1]
    assert "If vaultKeypad == None || vaultKeypad.W05_MQ00_CodeAV == None" in body
    assert "If dynamicCode < 0.0" in body
    assert "If ExpectedCode() < 0" in keypad.split("Event OnActivate(")[1]


def test_a_keypad_without_the_vault79_script_is_unaffected() -> None:
    """The cast is the scope: only references carrying that script resolve."""
    keypad = _patch_source(KEYPAD)
    body = keypad.split("Int Function DynamicCode()")[1]
    assert body.index("Return -1") < body.index("playerRef.GetValue")


def test_the_alias_no_longer_redeclares_its_parents_properties() -> None:
    """`DefaultAlias` owns StageToSet and PrereqStage.

    Re-declaring either is a hard `already defined on parent` error under stock
    PapyrusCompiler.exe, which failed the whole script — and a failed script
    loses its binding, which would have handed every keypad back its own
    completion and reopened the unlock-before-prerequisite defect.
    """
    key = unified._script_key(ALIAS)
    assert key not in unified._FO76_TO_FO4_SCRIPT_PROPERTY_ADDITIONS
    assert unified._FO76_TO_FO4_SCRIPT_PROPERTY_REMOVALS[key] == (
        "StageToSet",
        "preReqStage",
    )


def test_the_removal_converges_from_a_skeleton_that_already_declares_them() -> None:
    skeleton = (
        "Scriptname DefaultAliasSetStageOnKeypadSuccess Extends DefaultAlias default\n"
        "Int Property StageToSet Auto Mandatory\n"
        "Int Property preReqStage Auto\n"
        "\n"
        "Int Property PlayerActivateType = 3 Auto\n"
    )
    once = unified._augment_fo76_to_fo4_script_skeleton(ALIAS, skeleton)
    assert "Property StageToSet" not in once
    assert "Property preReqStage" not in once
    assert "Property PlayerActivateType" in once
    assert unified._augment_fo76_to_fo4_script_skeleton(ALIAS, once) == once


def test_a_multi_line_property_block_is_never_half_removed() -> None:
    skeleton = (
        "Scriptname DefaultAliasSetStageOnKeypadSuccess Extends DefaultAlias default\n"
        "Int Property StageToSet\n"
        "    Int Function Get()\n"
        "        Return 1\n"
        "    EndFunction\n"
        "EndProperty\n"
    )
    kept = unified._augment_fo76_to_fo4_script_skeleton(ALIAS, skeleton)
    assert "Int Property StageToSet" in kept


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


@requires_corpora
@pytest.mark.skipif(
    _stock_compiler() is None, reason="FO4 PapyrusCompiler.exe / Base scripts unavailable"
)
@pytest.mark.parametrize("script_name", [KEYPAD, ALIAS])
def test_merged_script_compiles_under_stock_papyruscompiler(script_name, tmp_path):
    """The native compiler accepts APIs and redeclarations FO4 does not.

    Both keypad scripts passed it while the alias half could not compile at all
    under the real toolchain, so this is the check that actually holds.
    """
    import subprocess

    compiler, base = _stock_compiler()
    user = tmp_path / "User"
    user.mkdir()
    for name in (KEYPAD, ALIAS, "W05_Vaut79EntranceKeypadScript"):
        relative = unified._script_relative_path(name, ".psc")
        (user / relative).parent.mkdir(parents=True, exist_ok=True)
        if name in (KEYPAD, ALIAS):
            (user / relative).write_text(_merged_source(name), encoding="utf-8")
        else:
            # The sibling that carries Vault 79's code AV property.
            (user / relative).write_text(
                (_GENERATED_USER_SCRIPTS / relative).read_text(encoding="utf-8"),
                encoding="utf-8",
            )
    out = tmp_path / "out"
    out.mkdir()
    relative = str(unified._script_relative_path(script_name, "")).replace("\\", "/")
    result = subprocess.run(
        [
            str(compiler),
            relative,
            f"-import={user};{base}",
            f"-output={out}",
            "-f=Institute_Papyrus_Flags.flg",
        ],
        cwd=str(user),
        capture_output=True,
        text=True,
    )
    assert (out / (relative + ".pex")).is_file(), result.stdout + result.stderr
