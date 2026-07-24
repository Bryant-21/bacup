from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "FlipCardSignScript"
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / f"{SCRIPT_NAME}.psc"
)


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert SOURCE_PATH.is_file(), SOURCE_PATH
    return _merge_script_method_patches(
        SOURCE_PATH.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def test_character_event_lookup_uses_the_bound_struct_map():
    body = _member_body(_merged_source(), "geteventstring")
    lookup = 'CharEventMapData.FindStruct("Character", Character)'
    assert lookup in body
    assert body.index(lookup) < body.index("CharEventMapData[index].AnimationEvent")


def test_display_message_rechecks_both_array_bounds_each_iteration():
    body = _member_body(_merged_source(), "displaymessage")
    loop = "While I < messageTextToDisplay.Length && I < LinkedRefs.Length"
    assert loop in body
    assert "While temp33" not in body
    assert body.index(loop) < body.index("I += 1")
    assert body.rindex("DisplayMessageSpinLock = False") > body.index(loop)


def test_on_load_clears_a_persisted_spin_lock_before_displaying():
    body = _member_body(_merged_source(), "onload")
    reset = "DisplayMessageSpinLock = False"
    display = "DisplayMessage(None)"
    assert body.index(reset) < body.index(display)


def test_flip_card_sign_full_merged_source_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
