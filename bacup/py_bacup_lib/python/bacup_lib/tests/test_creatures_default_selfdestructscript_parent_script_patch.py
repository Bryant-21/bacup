from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "Creatures"
    / "_Default"
    / "selfdestructscript.psc"
)
SOURCE_ROOT = SOURCE_PATH.parents[2]
SCRIPT_NAME = "Creatures:_Default:SelfDestructScript"


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
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(
        SOURCE_PATH.read_text(encoding="utf-8"), patch
    )


def test_self_destruct_parent_patch_restores_effect_and_timer_entrypoints():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname " not in patch

    merged = _merged_source()
    assert merged.count("Event OnEffectStart(") == 1
    assert merged.count("Event OnEffectFinish(") == 1
    assert merged.count("Event OnTimer(") == 1
    assert merged.count("Function ResetSelfDestruct(") == 1
    assert merged.count("Function StartSelfDestruct(") == 1
    assert merged.count("Function ExplodeSelfDestruct(") == 1


def test_self_destruct_parent_patch_has_complete_countdown_and_terminal_states():
    merged = _merged_source()

    self_destruct_start = merged.index("State selfdestruct\n")
    self_destruct_end = merged.index("EndState", self_destruct_start)
    self_destruct = merged[self_destruct_start:self_destruct_end]
    counting_down_start = merged.index("State selfdestructing")
    counting_down_end = merged.index("EndState", counting_down_start)
    counting_down = merged[counting_down_start:counting_down_end]
    exploded_start = merged.index("State selfdestructed")
    exploded_end = merged.index("EndState", exploded_start)
    exploded = merged[exploded_start:exploded_end]

    assert 'GoToState("selfdestructing")' in self_destruct
    assert "StartSelfDestruct()" in counting_down
    assert 'GoToState("selfdestructed")' in counting_down
    assert "ExplodeSelfDestruct()" in exploded


def test_self_destruct_parent_patch_uses_bound_forms_and_preserves_no_disintegrate():
    merged = _merged_source()

    assert "selfRef.HasKeyword(NoSelfDestruct)" in merged
    assert "selfRef.EquipItem(SelfDestructingWeapon)" in merged
    assert "selfRef.AddSpell(SelfDestructingCloakSpell, False)" in merged
    assert "selfRef.PlaceAtMe(SelfDestructExplosion)" in merged
    assert "selfRef.AttachAshPile(SelfDestructContainer)" in merged
    assert "selfRef.HasKeyword(NoDisintegrateOnSelfDestruct)" in merged
    assert "selfRef.DamageValue(Health, selfRef.GetValue(Health) + 100.0)" in merged


def test_self_destruct_parent_patch_merges_and_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Creatures/_Default/selfdestructscript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
