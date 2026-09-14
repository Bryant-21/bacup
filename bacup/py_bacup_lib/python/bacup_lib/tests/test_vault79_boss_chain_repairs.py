from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Vault79_SentryBotPodScript"


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))

    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break

    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    merged = _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def test_sentry_pod_patch_merges_one_activation_handler():
    merged = _merged_source()
    assert merged.lower().count("scriptname ") == 1
    assert merged.count("Event OnActivate(ObjectReference akActionRef)") == 1


def test_sentry_pod_completion_signal_prevents_a_respawn():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    gate = patch.index("If encounterComplete")
    blocked = patch.index("If IsActivationBlocked()")
    assert "sentryBot.IsDead()" in patch[:gate]
    assert "playerRef.GetValue(myActorValue) >= 1.0" in patch[:gate]
    assert "sentryBot.GetValue(myActorValue) >= 1.0" in patch[:gate]
    assert "podDoor.SetOpen(True)" in patch[gate:]
    assert patch.index("Return", gate) < blocked
    assert blocked < patch.index("BlockActivation(True, True)")
    assert blocked < patch.index("sentryActivator.Activate(Self)")


def test_sentry_pod_releases_the_bound_actor_after_opening_the_pod():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    start_sound = patch.index("SentryPodSoundStart.Play(soundMarker)")
    klaxon = patch.index("klaxonMarker.Activate(Self)")
    steam = patch.index("steamMarker.Enable()")
    door = patch.rindex("podDoor.SetOpen(True)")
    activator = patch.index("sentryActivator.Activate(Self)")
    fallback = patch.index("sentryBot.Enable()")
    assert start_sound < klaxon < steam < door < activator < fallback
    assert "BlockActivation(True, True)" in patch
    assert "BlockActivation(False)" not in patch


def test_reactor_door_controller_is_patched_after_reverse_topology_closure():
    assert _script_patch_source("Vault79ReactorDoorOpenScript") is not None


def test_sentry_pod_merged_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
