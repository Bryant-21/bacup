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
    / "Vault79MineCollapseScript.psc"
)

SKELETON = """Scriptname Vault79MineCollapseScript Extends ObjectReference

ObjectReference Property myCollapseMarker Auto mandatory
"""


def _merged() -> str:
    patch = PATCH_PATH.read_text(encoding="utf-8")
    merged = _merge_script_method_patches(SKELETON, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def test_mine_collapse_patch_is_player_only_and_one_shot() -> None:
    merged = _merged()
    folded = merged.casefold()

    assert folded.count("event ontriggerenter(objectreference akactionref)") == 1
    guard = folded.index(
        "if akactionref != game.getplayer() || mycollapsemarker == none"
    )
    early_return = folded.index("return", guard)
    disable = folded.index("disable()")
    enable = folded.index("mycollapsemarker.enable()")
    activate = folded.index("mycollapsemarker.activate(akactionref)")
    assert guard < early_return < disable < enable < activate


def test_mine_collapse_patch_compiles_for_fo4() -> None:
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        _merged(),
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path="Vault79MineCollapseScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
