from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

PATCH_CASES = {
    "Creatures:WendigoColossusRaceScript": {
        "pex": Path("Creatures/WendigoColossusRaceScript.pex"),
        "members": {
            ("event", "oneffectstart"),
            ("event", "onanimationevent"),
            ("event", "ontimer"),
            ("event", "oneffectfinish"),
            ("function", "launchvomit"),
            ("function", "applynextcombatstyle"),
        },
    },
    "Creatures:GraftonRaceScript": {
        "pex": Path("Creatures/GraftonRaceScript.pex"),
        "members": {
            ("event", "oneffectstart"),
            ("event", "onanimationevent"),
            ("event", "ontimer"),
            ("event", "oneffectfinish"),
            ("function", "launchoilbomb"),
            ("function", "startsalvocooldown"),
        },
    },
    "Creatures:StormBossRaceScript": {
        "pex": Path("Creatures/StormBossRaceScript.pex"),
        "members": {
            ("event", "oneffectstart"),
            ("event", "onanimationevent"),
            ("event", "oncripple"),
            ("event", "oneffectfinish"),
            ("function", "placelocalstrikemarker"),
            ("function", "setmeleeenabled"),
        },
    },
    "Quests:BS02_MQ05_Catalyst:SMBehemothBossRaceScript": {
        "pex": Path("QUESTS/BS02_MQ05_Catalyst/SMBehemothBossRaceScript.pex"),
        "members": {
            ("event", "oneffectstart"),
            ("event", "onanimationevent"),
            ("event", "oneffectfinish"),
            ("function", "launchbosssalvo"),
        },
    },
    "CreatureUltraciteAbominationScript": {
        "pex": Path("CreatureUltraciteAbominationScript.pex"),
        "members": {
            ("event", "onaliasinit"),
            ("event", "onhit"),
            ("event", "ontimer"),
            ("event", "ondeath"),
            ("event", "onaliasshutdown"),
            ("function", "beginmutation"),
            ("function", "completemutation"),
            ("function", "evaluatemutation"),
            ("function", "spawnbossat"),
        },
    },
}


def _fo4_base_source() -> Path | None:
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    candidates: list[Path] = []
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


def _merged_production_source(script_name: str) -> str:
    case = PATCH_CASES[script_name]
    pex_path = DEPLOYED_SCRIPT_ROOT / case["pex"]
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_p0_creature_patch_is_a_method_fragment(script_name: str):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    actual_members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert PATCH_CASES[script_name]["members"] <= actual_members


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_p0_creature_patch_merges_into_production_skeleton(script_name: str):
    merged = _merged_production_source(script_name)

    assert merged.lower().count("scriptname ") == 1
    for _kind, member_name in PATCH_CASES[script_name]["members"]:
        assert member_name in merged.lower()


def test_p0_patches_omit_unavailable_server_orchestration():
    colossus = _script_patch_source("Creatures:WendigoColossusRaceScript")
    storm_boss = _script_patch_source("Creatures:StormBossRaceScript")
    ultracite = _script_patch_source("CreatureUltraciteAbominationScript")

    assert colossus is not None
    assert "SummonAlliesQuest" not in colossus
    assert "SummonedWendigoAllies" not in colossus
    assert storm_boss is not None
    assert "bossEventInstance" not in storm_boss
    assert "SendCustomEvent" not in storm_boss
    assert ultracite is not None
    assert "EWSScriptRef" not in ultracite
    assert "Alias_Enemies_MoleMiners" not in ultracite
    assert "Alias_Enemies_Ultramites" not in ultracite


def test_projectile_cleanup_never_tracks_actor_fallbacks_as_temporary_markers():
    colossus = _script_patch_source("Creatures:WendigoColossusRaceScript")
    grafton = _script_patch_source("Creatures:GraftonRaceScript")
    behemoth = _script_patch_source(
        "Quests:BS02_MQ05_Catalyst:SMBehemothBossRaceScript"
    )

    assert colossus is not None
    colossus_lines = {line.strip() for line in colossus.splitlines()}
    assert "radVomitSourceMarker = akColossus" not in colossus_lines
    assert "poisonVomitSourceMarker = akColossus" not in colossus_lines
    assert "radVomitTargetMarker = targetActor" not in colossus_lines
    assert "poisonVomitTargetMarker = targetActor" not in colossus_lines

    assert grafton is not None
    grafton_lines = {line.strip() for line in grafton.splitlines()}
    assert "salvoSourceMarker = selfRef" not in grafton_lines
    assert "salvoTargetMarker = targetActor" not in grafton_lines

    assert behemoth is not None
    behemoth_lines = {line.strip() for line in behemoth.splitlines()}
    assert "salvoSourceMarker = selfRef" not in behemoth_lines
    assert "salvoTargetMarker = targetActor" not in behemoth_lines


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_p0_production_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    dependency_root = tmp_path / "dependencies"
    region_boss_dir = dependency_root / "Quests" / "Storm" / "RegionBoss"
    region_boss_dir.mkdir(parents=True)
    (region_boss_dir / "RegionBossQuestScript.psc").write_text(
        "Scriptname Quests:Storm:RegionBoss:RegionBossQuestScript extends Quest\n",
        encoding="utf-8",
    )
    (dependency_root / "defaultquestencounterwavescript.psc").write_text(
        "Scriptname defaultquestencounterwavescript extends Quest\n",
        encoding="utf-8",
    )

    source = _merged_production_source(script_name)
    result = compile_psc(
        source,
        imports=[str(dependency_root), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
