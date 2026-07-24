from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Creatures:SentryBotShoulderClusterScript"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


def test_sentry_bot_shoulder_cluster_effect_start_patch_merges_once_and_compiles():
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch
    assert patch.count("Event OnEffectStart(Actor akTarget, Actor akCaster)") == 1
    assert "akCaster != None && akCaster.IsInInterior()" in patch
    assert "akCaster.UnequipItem(OutdoorLauncher)" in patch
    assert "akCaster.EquipItem(IndoorLauncher)" in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert members.count("oneffectstart") == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
