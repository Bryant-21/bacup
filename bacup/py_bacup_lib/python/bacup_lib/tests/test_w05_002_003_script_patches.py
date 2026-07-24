from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCES_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


PATCH_CASES = {
    "W05_002P_IntroSceneTriggerScript": (
        "Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)",
        "Loudspeaker.GetReference().Say(W05_002P_RoperWarning)",
        "Game.GetPlayer().SetValue(W05_MQ_002P_Radical_HeardRoperWarning, 1.0)",
    ),
    "W05_002P_RadicalHostilityTrigger": (
        "Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)",
        "W05_MQ_002P_Radical_PlayerKilledRoper != None &&",
        "Game.GetPlayer().AddToFaction(W05_RadicalEnemyFaction)",
    ),
    "W05_003P_EnterAnyTriggerRefColl": (
        "Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)",
        "owningQuest.IsStageDone(ShutDownStage)",
        "owningQuest.SetStage(StageToSet)",
    ),
    "W05_003P_HiddenDoorTriggerScript": (
        "Event OnTriggerEnter(ObjectReference akActionRef)",
        "enteringActor.GetValue(W05_MQ_003P_Muscle_PlayerCanAccessDuncan) > 0.0",
        "hiddenDoor.SetOpen(True)",
    ),
}

ZERO_MEMBER_CASES = (
    "W05_002P_DeathclawIsleTriggerScript",
    "W05_003P_MusicOverrideTriggerScript",
)


def _members(source: str) -> list[tuple[str, str]]:
    return [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"}
    ]


def _merged_production_source(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / f"{script_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(
        decompile_pex(pex_path, fo4_api_compat=True), patch
    )


@pytest.mark.parametrize(("script_name", "snippets"), PATCH_CASES.items())
def test_w05_002_003_patch_has_one_evidence_backed_trigger_handler(
    script_name: str, snippets: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert _members(patch) == [("event", "ontriggerenter")]
    for snippet in snippets:
        assert snippet in patch


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_w05_002_003_merged_patch_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", ZERO_MEMBER_CASES)
def test_w05_002_003_zero_member_scripts_have_no_hollow_patch(script_name: str):
    generated = (GENERATED_SOURCES_ROOT / f"{script_name}.psc").read_text(
        encoding="utf-8"
    )

    assert _script_patch_source(script_name) is None
    assert _members(generated) == []
