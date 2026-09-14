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
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)
QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQA_206P_0054EDB9"


def _patch() -> str:
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    return patch


def _production_skeleton() -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(QUEST_FRAGMENT, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_source() -> str:
    return _merge_script_method_patches(_production_skeleton(), _patch())


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind == "function" and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def test_stage_9000_dispatches_only_the_raider_lou_story_selector():
    body = _member_body(_patch(), "fragment_stage_9000_item_00")
    lookup = 'Game.GetFormFromFile(0x005588EF, "SeventySix.esm") as Quest'
    send = (
        "W05_MQR_205P_A_QuestStart_Keyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )

    assert body.count(lookup) == 1
    assert body.count(send) == 1
    assert body.index("SetObjectiveCompleted(800)") < body.index(lookup)
    assert body.index(lookup) < body.index("W05_MQR_205P.IsStageDone(9000)")
    assert body.index("W05_MQR_205P.IsStageDone(9000)") < body.index(send)

    before_send, after_send = body.split(send)
    assert "!louQuest.IsRunning()" in before_send
    assert "!louQuest.IsCompleted()" in before_send
    assert "!louQuest.IsRunning()" in after_send
    assert "!louQuest.IsCompleted()" in after_send
    assert "louQuest.Start(" not in body
    assert "louQuest.SetStage(" not in body


def test_mqa206_story_sender_full_merge_is_idempotent():
    skeleton = _production_skeleton()
    patch = _patch()
    merged = _merge_script_method_patches(skeleton, patch)

    assert merged.count("SendStoryEventAndWait(None, playerRef, playerRef)") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_mqa206_story_sender_full_merge_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="QF_W05_MQA_206P_0054EDB9.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None

    compiled_pex = tmp_path / "QF_W05_MQA_206P_0054EDB9.pex"
    compiled_pex.write_bytes(result.pex_bytes)
    compiled_source = decompile_pex(compiled_pex, fo4_api_compat=True)
    assert "SendStoryEventAndWait" in compiled_source
