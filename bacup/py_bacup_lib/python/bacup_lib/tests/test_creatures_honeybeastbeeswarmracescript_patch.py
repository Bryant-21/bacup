from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


def _merged() -> str:
    skeleton = (SOURCE_ROOT / "Creatures" / "HoneyBeastBeeSwarmRaceScript.psc").read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source("Creatures:HoneyBeastBeeSwarmRaceScript")
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_swarm_starts_its_fx_without_waiting_on_the_auto_state():
    merged = _merged()
    # An auto state's OnBeginState never runs at initialization, so the opening
    # stage event has to be sent from OnEffectStart or the swarm renders nothing.
    start = merged.split("Event OnEffectStart", 1)[1].split("EndEvent", 1)[0]
    assert "StartSwarmStage(animEventHealthFull)" in start
    assert "selfActorRef = akCaster" in start


def test_swarm_reaches_its_remaining_stages():
    merged = _merged()
    # Without these the healthmid/healthlow/disperse/death states stay unreachable.
    assert 'GoToState("healthlow")' in merged
    assert 'GoToState("healthmid")' in merged
    assert 'GoToState("disperse")' in merged
    assert 'GoToState("death")' in merged
    assert "GetValuePercentage(Health)" in merged


def test_swarm_rearms_its_single_shot_hit_registration():
    merged = _merged()
    # RegisterForHitEvent delivers one OnHit; without re-arming, staging stops
    # after the first hit.
    hit = merged.split("Event OnHit", 1)[1].split("EndEvent", 1)[0]
    assert "ArmHitWatch()" in hit
    assert "RegisterForHitEvent(selfActorRef)" in merged


def test_swarm_patch_preserves_the_skeleton():
    merged = _merged()
    assert merged.count("Event OnEffectStart") == 1
    assert "Scriptname Creatures:HoneyBeastBeeSwarmRaceScript" in merged
    for state in ("healthlow", "disperse", "death", "healthmid", "HealthFull"):
        assert f"State {state}" in merged


def test_swarm_patch_compiles():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Creatures/HoneyBeastBeeSwarmRaceScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
