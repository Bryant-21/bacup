from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import _merge_script_method_patches
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)

SKELETONS = {
    "EN06_DisableOnUnload": """Scriptname EN06_DisableOnUnload Extends ObjectReference

keyword Property ActiveKeyword Auto mandatory
""",
    "LC204_ScanEnableScript": """Scriptname LC204_ScanEnableScript Extends ObjectReference

Bool Property DisableWhileActive = False Auto
Bool Property FadeInObjects = True Auto
Float Property ActiveTimeSeconds = 3.0 Auto

State waiting
EndState

State active
EndState
""",
    "E09C_TOLHandyScript": """Scriptname E09C_TOLHandyScript Extends Actor

ObjectReference Property ResetLocation Auto mandatory
""",
    "WL019_DisarmFlamethrowersOnDeath": """Scriptname WL019_DisarmFlamethrowersOnDeath Extends Actor

keyword Property flameThrowerKeyword Auto
""",
}


def _merged(script_name: str) -> str:
    patch = (PATCH_ROOT / f"{script_name}.psc").read_text(encoding="utf-8")
    merged = _merge_script_method_patches(SKELETONS[script_name], patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("script_name", sorted(SKELETONS))
def test_facility_dependency_patch_merges_idempotently_and_compiles(script_name: str):
    merged = _merged(script_name)
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_en06_unload_patch_preserves_the_unused_unbound_guard_declaration():
    merged = _merged("EN06_DisableOnUnload")
    assert merged.casefold().count("event onunload()") == 1
    assert "DisableNoWait()" in merged
    assert "keyword Property ActiveKeyword Auto mandatory" in merged


def test_lc204_scan_patch_uses_default_and_waiting_entrypoints_with_active_lock():
    merged = _merged("LC204_ScanEnableScript")
    folded = merged.casefold()
    assert folded.count("function runscan()") == 1
    assert folded.count("event onactivate(objectreference akactivator)") == 3
    assert "getlinkedrefchain()" in folded
    assert 'gotostate("active")' in folded
    assert 'gotostate("waiting")' in folded
    assert "utility.wait(activetimeseconds)" in folded
    assert "linkedrefs[i].enable(fadeinobjects)" in folded
    assert "linkedrefs[i].disable(fadeinobjects)" in folded

    waiting = folded.split("state waiting", 1)[1].split("endstate", 1)[0]
    active = folded.split("state active", 1)[1].split("endstate", 1)[0]
    assert "runscan()" in waiting
    assert "runscan()" not in active


def test_e09c_reset_patch_returns_mr_lovely_to_the_bound_marker():
    merged = _merged("E09C_TOLHandyScript")
    assert merged.casefold().count("event onreset()") == 1
    assert "MoveTo(ResetLocation)" in merged


def test_wl019_death_patch_disarms_the_complete_keyword_chain():
    merged = _merged("WL019_DisarmFlamethrowersOnDeath")
    folded = merged.casefold()
    assert folded.count("event ondeath(actor akkiller)") == 1
    assert "getlinkedrefchain(flamethrowerkeyword, 100)" in folded
    assert "linkedrefs[i] as trapbase" in folded
    assert 'linkedtrap.gotostate("disarm")' in folded
