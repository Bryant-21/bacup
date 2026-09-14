from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "quests"
    / "storm"
    / "mq08"
    / "hallucgaseffectscript.pex"
)
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Quests:Storm:MQ08:HallucGasEffectScript"


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _production_skeleton() -> str:
    assert SOURCE_PEX.is_file(), SOURCE_PEX
    return decompile_pex(SOURCE_PEX, fo4_api_compat=True)


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_production_source() -> str:
    return _merge_script_method_patches(_production_skeleton(), _patch())


def test_halluc_gas_source_pex_retains_the_incompatible_fo76_finish_shape():
    finish = _member_body(_production_skeleton(), "oneffectfinish")
    assert finish.splitlines()[0] == (
        "Event OnEffectFinish(actor akTarget, actor akCaster, Float afMagnitude, "
        "Float afDuration, Float afElapsed)"
    )


def test_halluc_gas_patch_is_member_only_with_exact_fo4_finish_signature():
    patch = _patch()
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "endstate"))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    members = list(_iter_top_level_papyrus_members(patch.splitlines()))
    assert [(kind, name) for kind, name, _start, _end in members] == [
        ("event", "oneffectfinish")
    ]
    finish = _member_body(patch, "oneffectfinish")
    assert finish.splitlines()[0] == (
        "Event OnEffectFinish(Actor akTarget, Actor akCaster)"
    )
    for unavailable_parameter in ("afMagnitude", "afDuration", "afElapsed"):
        assert unavailable_parameter not in finish


def test_halluc_gas_production_merge_is_unique_and_idempotent():
    patch = _patch()
    merged = _merged_production_source()

    assert merged.lower().count("event oneffectfinish(") == 1
    assert _member_body(merged, "oneffectfinish") == _member_body(
        patch, "oneffectfinish"
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_halluc_gas_finish_performs_each_reachable_cleanup_once():
    finish = _member_body(_merged_production_source(), "oneffectfinish")

    sound_cancel = "CancelTimer(iSoundTimerID)"
    shadow_cancel = "CancelTimer(iSpawnShadowTimerID)"
    unregister = "UnregisterForAllRemoteEvents()"
    image_remove = "ImageSpaceToApply.Remove()"
    for cleanup in (sound_cancel, shadow_cancel, unregister, image_remove):
        assert finish.count(cleanup) == 1

    assert finish.index(sound_cancel) < finish.index(shadow_cancel)
    assert finish.index(shadow_cancel) < finish.index(unregister)
    assert finish.index(unregister) < finish.index(image_remove)
    assert "OnEffectFinishShared" not in finish
    assert "StartTimer(" not in finish
    assert ".Apply(" not in finish


def test_halluc_gas_full_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert SOURCE_ROOT.is_dir(), SOURCE_ROOT

    result = compile_psc(
        _merged_production_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
