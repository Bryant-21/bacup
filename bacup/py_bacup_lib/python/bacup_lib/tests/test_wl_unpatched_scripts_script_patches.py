from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

# Every script below is confirmed non-defect / evidence-blocked and carries no
# fix-folder patch (`_script_patch_source(...) is None`). Merged from the
# former w05_omitted_sequences / wl005_sequences / wl005_wl006_wl036 /
# wl_remaining_declarations shards, which each verified a disjoint subset of
# scripts under the identical "confirm no patch exists, then compile" shape.


def _member_names(source: str) -> list[str]:
    # _iter_top_level_papyrus_members only ever yields function/event kinds,
    # so this is equivalent for every caller below regardless of whether the
    # original shard filtered on kind.
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _generated_source(script_name: str) -> str:
    source_path = GENERATED_SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), f"generated source unavailable: {source_path}"
    return source_path.read_text(encoding="utf-8")


def _deployed_source(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _decompiled(script_file: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / script_file
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


# -- former w05_omitted_sequences: WL005/WL006 client-only sequence scripts,
# read by explicit generated filename (case matches the file on disk).
SEQUENCE_SCRIPTS = {
    "WL005_FallingDustLoopScript": (
        "wl005_fallingdustloopscript.psc",
        ["Play", "OnActivate"],
        [
            'utility.Wait(utility.RandomFloat(randMin, randMax))',
            'Self.PlayAnimation("stage2")',
            'Self.PlayAnimation("Reset")',
            "sequenceIsActive = True",
        ],
    ),
    "WL005_FloorCollapseSequenceScript": (
        "WL005_FloorCollapseSequenceScript.psc",
        [
            "RotateHelpersPhase01",
            "ClientPlayCollapseEchoSound",
            "ClientPlayFinalCrackingConcreteSound",
            "ClientPlayCrackingConcreteSound",
            "RotateHelpers",
            "RotateHelpersPhase03",
            "RotateHelpersPhase02",
        ],
        [
            "Self.GetLinkedRefChain(floorCollapsePhase01_RotationKeyword, 100)",
            "Self.GetLinkedRefChain(floorCollapsePhase02_RotationKeyword, 100)",
            "Self.GetLinkedRefChain(floorCollapsePhase03_RotationKeyword, 100)",
            "rotationHelper.SetMotionType(Self.Motion_Keyframed, True)",
            'mangled_rotationhelper_0.playAnimation("play01")',
            "collapseEchoSound.Play(Self as ObjectReference)",
        ],
    ),
    "WL006_SentryBotRevealScript": (
        "WL006_SentryBotRevealScript.psc",
        ["ClientPlayKlaxonSound"],
        [
            "instance = klaxonSound.Play(Self as ObjectReference)",
            "utility.Wait(20.0)",
            "sound.StopInstance(instance)",
        ],
    ),
}


@pytest.mark.parametrize(
    ("script_name", "source_file", "members", "required_calls"),
    [
        (script_name, source_file, members, required_calls)
        for script_name, (source_file, members, required_calls) in SEQUENCE_SCRIPTS.items()
    ],
)
def test_w05_omitted_sequence_scripts_retain_original_client_members(
    script_name: str,
    source_file: str,
    members: list[str],
    required_calls: list[str],
):
    source = (GENERATED_SOURCE_ROOT / source_file).read_text(encoding="utf-8")

    assert _script_patch_source(script_name) is None
    assert [
        name.casefold()
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ] == [member.casefold() for member in members]
    for call in required_calls:
        assert call in source


@pytest.mark.parametrize(
    "source_file",
    [case[0] for case in SEQUENCE_SCRIPTS.values()],
)
def test_w05_omitted_sequence_sources_compile_for_fo4(source_file: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        (GENERATED_SOURCE_ROOT / source_file).read_text(encoding="utf-8"),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=source_file,
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# -- former wl005_sequences: WL005 furniture/door sequence scripts, cross-checked
# against both the generated source and the deployed PEX.
WL005_SEQUENCE_SCRIPTS = (
    "WL005_BombActivateFurnitureScript",
    "WL005_ExplodingDoorSequence01Script",
    "WL005_ExplodingDoorSequence02Script",
)


@pytest.mark.parametrize("script_name", WL005_SEQUENCE_SCRIPTS)
def test_client_owned_wl005_sequence_scripts_remain_unpatched(script_name: str):
    assert _script_patch_source(script_name) is None


def test_source_and_deployed_members_show_the_actual_client_contract():
    for script_name in (
        "WL005_BombActivateFurnitureScript",
        "WL005_ExplodingDoorSequence02Script",
    ):
        assert _member_names(_generated_source(script_name)) == []
        assert _member_names(_deployed_source(script_name)) == []

    expected = ["clientplayrockrumblesound"]
    generated = _generated_source("WL005_ExplodingDoorSequence01Script")
    deployed = _deployed_source("WL005_ExplodingDoorSequence01Script")
    assert _member_names(generated) == expected
    assert _member_names(deployed) == expected
    assert generated.count("rockrumbleSound.Play(Self as ObjectReference)") == 1
    assert deployed.count("rockrumbleSound.Play(Self as ObjectReference)") == 1


@pytest.mark.parametrize("script_name", WL005_SEQUENCE_SCRIPTS)
def test_unpatched_wl005_sequence_sources_compile_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _generated_source(script_name),
        imports=[str(base_source), str(tmp_path)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# -- former wl005_wl006_wl036: WL005/WL006/WL036 client sound + zero-member
# scripts, read directly from their deployed PEX filenames.
CLIENT_SOUND_SCRIPTS = {
    "WL005_MiniQuakeOnTriggerEnterScript": (
        "wl005_miniquakeontriggerenterscript.pex",
        "clientplayrockrumblesound",
        "rockrumbleSound.Play(Self as ObjectReference)",
    ),
    "WL005_PlaySoundOnTriggerEnter": (
        "wl005_playsoundontriggerenter.pex",
        "clientplaysound",
        "soundToPlay.Play(soundSource)",
    ),
    "WL006_PlayButtonSound": (
        "wl006_playbuttonsound.pex",
        "clientplaysound",
        "soundToPlay.Play(Self as ObjectReference)",
    ),
}

ZERO_MEMBER_SCRIPTS = {
    "WL006_SwapButtonScript": "wl006_swapbuttonscript.pex",
    "WL036KeypadScript": "wl036keypadscript.pex",
}


@pytest.mark.parametrize(
    ("script_name", "script_file", "member_name", "call"),
    [
        (script_name, script_file, member_name, call)
        for script_name, (script_file, member_name, call) in CLIENT_SOUND_SCRIPTS.items()
    ],
)
def test_wl_client_sound_scripts_are_retained_nondefect_members(
    script_name: str, script_file: str, member_name: str, call: str
):
    source = _decompiled(script_file)

    assert _script_patch_source(script_name) is None
    assert _member_names(source) == [member_name]
    assert call in source


@pytest.mark.parametrize(("script_name", "script_file"), ZERO_MEMBER_SCRIPTS.items())
def test_wl_zero_member_scripts_remain_unpatched(
    script_name: str, script_file: str
):
    source = _decompiled(script_file)

    assert _script_patch_source(script_name) is None
    assert _member_names(source) == []


@pytest.mark.parametrize(
    "script_file",
    [case[0] for case in CLIENT_SOUND_SCRIPTS.values()]
    + list(ZERO_MEMBER_SCRIPTS.values()),
)
def test_wl_nondefect_production_sources_compile_for_fo4(script_file: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _decompiled(script_file),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=script_file.removesuffix(".pex") + ".psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# -- former wl_remaining_declarations: pure declaration-only WL005/WL006
# scripts (zero functions/events in the generated source).
REMAINING_ZERO_MEMBER_SCRIPTS = (
    "WL005_LousRadioScript",
    "WL005_PreventFallDamageScript",
    "WL006_ManageActivationLight",
)


@pytest.mark.parametrize("script_name", REMAINING_ZERO_MEMBER_SCRIPTS)
def test_wl_remaining_declaration_only_scripts_have_no_marker_only_patch(
    script_name: str,
):
    assert _script_patch_source(script_name) is None
    assert _member_names(_generated_source(script_name)) == []


@pytest.mark.parametrize("script_name", REMAINING_ZERO_MEMBER_SCRIPTS)
def test_wl_remaining_declaration_only_scripts_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _generated_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
