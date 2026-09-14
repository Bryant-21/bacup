from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import _merge_script_method_patches
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
PATCH_PATH = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "BoSSetStageTriggerScript.psc"
)

SKELETON = """Scriptname BoSSetStageTriggerScript Extends ObjectReference

quest Property pBoSQuest Auto mandatory
Int Property bOnLeave Auto
Int Property pPreReqStage Auto
Int Property pStageToSet Auto mandatory
"""


def _merged() -> str:
    patch = PATCH_PATH.read_text(encoding="utf-8")
    merged = _merge_script_method_patches(SKELETON, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def test_vault79_stage460_trigger_patch_preserves_the_bound_event_contract() -> None:
    merged = _merged()
    folded = merged.casefold()

    assert folded.count("event ontriggerenter(objectreference akactionref)") == 1
    assert folded.count("event ontriggerleave(objectreference akactionref)") == 1
    assert folded.count("function trysetstage(objectreference akactionref)") == 1
    assert "if bonleave == 0" in folded
    assert "if bonleave != 0" in folded
    assert "if akactionref != game.getplayer() || pbosquest == none" in folded
    assert "if pprereqstage <= 0 || pbosquest.isstagedone(pprereqstage)" in folded
    assert "pbosquest.setstage(pstagetoset)" in folded


def test_vault79_stage460_trigger_patch_compiles_for_fo4() -> None:
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        _merged(),
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path="BoSSetStageTriggerScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
