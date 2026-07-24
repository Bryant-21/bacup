from __future__ import annotations

import os
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.workflows.unified import (
    CONVERTER_VERSIONS,
    _UnifiedRecordRuntime,
    _augment_fo76_to_fo4_script_skeleton,
    _fo76_to_fo4_script_type,
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "DefaultAliasInventoryManagement"
SOURCE_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "defaultaliasinventorymanagement.pex"
)
VARIANT_A_PEX = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / "defaultaliasinventorymanagementa.pex"
)
PARENT_SOURCE_DIR = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
CACHE_DECLARATION = "ObjectReference ShutdownReferenceCache"

# PATCH_CASES: every script this shard's contract covers. Only the base carries a
# patch file (bacup_lib/script_patches/DefaultAliasInventoryManagement.psc) -- the
# variants A-M and Wastelanders' W05_InventoryScriptJ/W05_Inventory_ScriptK are pure
# `Extends DefaultAliasInventoryManagement` shells with zero own members, so they
# inherit the base patch's behavior verbatim (contract A.1, A.7; approved condition
# (c): "no per-variant patch files -- declaration-only inheritance is the design").
PATCH_CASES = (SCRIPT_NAME,)


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


def _raw_source_skeleton() -> str:
    if not SOURCE_PEX.is_file():
        pytest.skip(f"FO76 source PEX unavailable: {SOURCE_PEX}")
    return decompile_pex(
        SOURCE_PEX,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )


def _production_skeleton() -> str:
    return _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, _raw_source_skeleton())


def _patch_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_production_source() -> str:
    return _merge_script_method_patches(_production_skeleton(), _patch_source())


def _emitted_production_source(tmp_path: Path) -> str:
    runtime = _UnifiedRecordRuntime(
        SimpleNamespace(
            source_game="fo76",
            target_game="fo4",
            output_root=tmp_path / "out",
        )
    )
    logs: list[tuple[str, str]] = []
    result = runtime._decompile_script_source_for_fo4(
        SCRIPT_NAME,
        SOURCE_PEX,
        SimpleNamespace(mod_path=tmp_path / "mod"),
        SimpleNamespace(emit_log=lambda level, message: logs.append((level, message))),
    )

    assert result is None
    assert any("merged fix-folder method patch" in message for _level, message in logs)
    return (
        tmp_path / "mod" / "Scripts" / "Source" / "User" / f"{SCRIPT_NAME}.psc"
    ).read_text(encoding="utf-8")


def _code_only(source: str) -> str:
    """Strip full-line ``;`` comments so identifier checks can't be tripped by
    prose in the patch's own explanatory comments."""
    return "\n".join(
        line for line in source.splitlines() if not line.strip().startswith(";")
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_exists_with_no_scriptname_line(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert CACHE_DECLARATION not in {line.strip() for line in patch.splitlines()}


def test_cache_declaration_is_exact_keyed_and_idempotent():
    raw = _raw_source_skeleton()
    assert CACHE_DECLARATION not in raw

    augmented = _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, raw)

    lines = augmented.splitlines()
    header_index = next(
        index
        for index, line in enumerate(lines)
        if line.lower().startswith("scriptname ")
    )
    assert lines[header_index + 1] == CACHE_DECLARATION
    assert lines.count(CACHE_DECLARATION) == 1
    assert _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, augmented) == augmented
    assert (
        _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME.swapcase(), raw)
        == augmented
    )
    assert (
        _augment_fo76_to_fo4_script_skeleton("DefaultAliasInventoryManagementA", raw)
        == raw
    )


def test_cache_declaration_accepts_one_same_typed_case_variant():
    raw = _raw_source_skeleton()
    existing = raw.replace(
        "\n", "\nobjectreference shutdownreferencecache\n", 1
    )

    assert _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, existing) == existing


def test_cache_declaration_rejects_conflicting_existing_type():
    raw = _raw_source_skeleton()
    conflicting = raw.replace("\n", "\nForm ShutdownReferenceCache\n", 1)

    with pytest.raises(ValueError, match="conflicting Papyrus declaration"):
        _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, conflicting)


@pytest.mark.parametrize(
    "collision",
    (
        "ObjectReference Property shutdownreferenceCACHE Auto",
        "ObjectReference Function ShutdownREFERENCECache()\nEndFunction",
        "Event SHUTDOWNreferencecache()\nEndEvent",
    ),
)
def test_cache_declaration_rejects_property_and_member_collisions(collision: str):
    raw = _raw_source_skeleton()
    conflicting = raw.replace("\n", f"\n{collision}\n", 1)

    with pytest.raises(ValueError, match="conflicting Papyrus (?:property|member)"):
        _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, conflicting)


def test_script_converter_cache_version_covers_skeleton_augmentation():
    assert CONVERTER_VERSIONS["scripts"] == "3"


def test_patch_declares_no_states_beyond_the_hollow_skeleton():
    # The skeleton declares '' and 'checking' (FO76's own two states, both empty in
    # source); the merger cannot safely introduce a new one.
    skeleton_states = {
        name for name, _start, _end in _iter_papyrus_states(_production_skeleton().splitlines())
    }
    patch_states = {
        name for name, _start, _end in _iter_papyrus_states(_patch_source().splitlines())
    }
    assert patch_states <= skeleton_states


def test_and_keyword_property_is_never_read():
    """Regression guard for the coordinator-approved A.9.1 resolution: the property
    is stored (compiles, preserves VMAD binding) but never branches on -- reading it
    anywhere would mean either dead code (if the branch can't be reached) or a silent
    reintroduction of the intersection semantics that no native FO4 API can express."""
    assert "RequiredItemsUseANDedKeywords" not in _code_only(_patch_source())


def test_text_replacement_properties_are_never_read():
    """A.4: no verified FO4 text-replacement API: the properties compile but the
    patch must not pretend to wire them."""
    patch = _code_only(_patch_source())
    assert "ItemCountTextVar" not in patch
    assert "ItemRequiredAmountTextVar" not in patch


def test_count_removed_but_counted_is_incremented_before_removal():
    """RemoveItemsOnAdded's docstring promises removed items are 'still counted' --
    the running tally must be incremented before the item physically leaves the
    container, since GetItemCount() can no longer see it afterward."""
    patch = _patch_source()
    added_body = patch.split("Event OnItemAdded", 1)[1].split("EndEvent", 1)[0]
    increment_idx = added_body.find("CountRemovedButCounted +=")
    remove_idx = added_body.find("RemoveItem(akBaseItem")
    assert increment_idx != -1 and remove_idx != -1
    assert increment_idx < remove_idx


def test_stop_managing_flag_gates_evaluation_and_added_removal():
    patch = _patch_source()
    evaluate_body = patch.split("Function EvaluateInventoryState", 1)[1]
    assert "If StopManagingInventoryFlag" in evaluate_body.split("EndFunction", 1)[0]
    added_body = patch.split("Event OnItemAdded", 1)[1].split("EndEvent", 1)[0]
    assert "!StopManagingInventoryFlag" in added_body


def test_alias_shutdown_uses_the_real_fo4_event_name():
    # Alias.OnAliasShutdown, not a Quest-level OnQuestShutdown (which is never
    # delivered to an alias) -- contract A.9.3.
    patch = _patch_source()
    assert "Event OnAliasShutdown()" in patch
    assert "OnQuestShutdown" not in patch


def test_add_inventory_event_filter_registers_all_items():
    patch = _patch_source()
    assert ".AddInventoryEventFilter(" not in patch
    assert [
        line.strip()
        for line in patch.splitlines()
        if "AddInventoryEventFilter(" in line
    ] == ["AddInventoryEventFilter(None)"]


def test_cache_is_seeded_before_filter_registration_and_evaluation():
    init_body = (
        _patch_source().split("Event OnAliasInit()", 1)[1].split("EndEvent", 1)[0]
    )

    assign_idx = init_body.find("ShutdownReferenceCache = GetReference()")
    filter_idx = init_body.find("AddInventoryEventFilter(None)")
    evaluate_idx = init_body.find("EvaluateInventoryState()")

    assert -1 not in (assign_idx, filter_idx, evaluate_idx)
    assert assign_idx < filter_idx < evaluate_idx


@pytest.mark.parametrize("event_name", ("OnItemAdded", "OnItemRemoved"))
def test_inventory_events_refresh_only_from_a_live_current_reference(event_name: str):
    event_body = (
        _patch_source().split(f"Event {event_name}", 1)[1].split("EndEvent", 1)[0]
    )

    get_ref_idx = event_body.find("ObjectReference currentRef = GetReference()")
    guard_idx = event_body.find("If currentRef != None")
    assign_idx = event_body.find("ShutdownReferenceCache = currentRef")
    end_guard_idx = event_body.find("EndIf", assign_idx)
    evaluate_idx = event_body.find("EvaluateInventoryState()")

    assert -1 not in (
        get_ref_idx,
        guard_idx,
        assign_idx,
        end_guard_idx,
        evaluate_idx,
    )
    assert get_ref_idx < guard_idx < assign_idx < end_guard_idx < evaluate_idx


def test_alias_shutdown_consumes_only_the_cache_then_always_clears_it():
    shutdown_body = (
        _patch_source().split("Event OnAliasShutdown()", 1)[1].split("EndEvent", 1)[0]
    )

    assert "GetReference" not in shutdown_body
    guard_idx = shutdown_body.find(
        "If RemoveItemsOnShutDown && !StopManagingInventoryFlag "
        "&& ShutdownReferenceCache != None"
    )
    remove_idx = shutdown_body.find("RemoveManagedItemsFrom(ShutdownReferenceCache)")
    clear_idx = shutdown_body.find("ShutdownReferenceCache = None")

    assert -1 not in (guard_idx, remove_idx, clear_idx)
    assert guard_idx < remove_idx < clear_idx
    assert shutdown_body.count("ShutdownReferenceCache = None") == 1


def test_merged_production_source_preserves_declarations_and_states():
    merged = _merged_production_source()
    assert "Scriptname DefaultAliasInventoryManagement Extends DefaultAlias" in merged
    assert merged.splitlines().count(CACHE_DECLARATION) == 1
    for prop in (
        "AdditionalStageData",
        "RequiredAmount",
        "RequiredItems",
        "RemoveItemsOnAdded",
        "RemoveItemsOnShutDown",
        "DependentObjectives",
        "NextObjectives",
        "StageToShowObjective",
        "RequiredItemsUseANDedKeywords",
        "RequireActivePlayerToComplete",
        "Objective",
        "ItemCountTextVar",
        "ItemRequiredAmountTextVar",
    ):
        assert f"Property {prop} Auto" in merged

    merged_lines = merged.splitlines()
    merged_states = {
        name for name, _start, _end in _iter_papyrus_states(merged_lines)
    }
    # The default state has no State/EndState wrapper in Papyrus source, so
    # _iter_papyrus_states only ever sees the one explicit named state.
    assert merged_states == {"checking"}

    top_level = _iter_top_level_papyrus_members(merged_lines)
    top_level_names = {(k, n) for k, n, _s, _e in top_level}
    for expected in (
        ("event", "onaliasinit"),
        ("event", "onitemadded"),
        ("event", "onitemremoved"),
        ("event", "onaliasshutdown"),
        ("function", "removerequireditems"),
        ("function", "transferrequireditemsfromplayertothiscontainer"),
        ("function", "setrequiredamount"),
        ("function", "setrequireditems"),
        ("function", "evaluateinventorystate"),
        ("function", "getmanagedcount"),
        ("function", "ismanageditem"),
        ("function", "hasallmanageditems"),
        ("function", "removemanageditemsfrom"),
    ):
        assert expected in top_level_names, f"missing merged top-level member: {expected}"


def test_production_merge_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not PARENT_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")

    source = _emitted_production_source(tmp_path)
    assert source == _merged_production_source()
    assert source.splitlines().count(CACHE_DECLARATION) == 1

    result = compile_psc(
        source,
        # FO4 base first (real DefaultAlias.psc / DefaultScriptFunctions.psc live
        # here), then the mod's generated Source/User tree, then the deployed
        # compiled Scripts dir for anything only ever shipped as PEX.
        imports=[str(base_source), str(PARENT_SOURCE_DIR), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_variant_a_compiles_against_the_patched_base(tmp_path):
    """Proves the declaration-only-inheritance design (approved condition (c)):
    DefaultAliasInventoryManagementA has zero own members and must compile cleanly
    against the PATCHED base, inheriting its behavior with no variant-specific
    patch file. Also stands in for Wastelanders' W05_InventoryScriptJ/
    W05_Inventory_ScriptK, which are the same shape (contract A.7)."""
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not PARENT_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")
    if not VARIANT_A_PEX.is_file():
        pytest.skip(f"deployed variant PEX unavailable: {VARIANT_A_PEX}")

    patched_base_dir = tmp_path / "patched_base"
    patched_base_dir.mkdir()
    (patched_base_dir / "DefaultAliasInventoryManagement.psc").write_text(
        _merged_production_source(), encoding="utf-8"
    )

    variant_skeleton = decompile_pex(VARIANT_A_PEX, fo4_api_compat=True)
    assert "Scriptname DefaultAliasInventoryManagementA Extends DefaultAliasInventoryManagement" in variant_skeleton
    # Confirms this variant really has nothing of its own to merge -- a patch file
    # would be dead weight (Lesson 19) if this ever stops being true.
    assert not any(
        line.strip().lower().startswith(("function ", "event "))
        for line in variant_skeleton.splitlines()
    )

    result = compile_psc(
        variant_skeleton,
        # patched_base_dir listed before PARENT_SOURCE_DIR so it shadows the stale
        # unpatched DefaultAliasInventoryManagement.psc already on disk there.
        imports=[str(base_source), str(patched_base_dir), str(PARENT_SOURCE_DIR), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="DefaultAliasInventoryManagementA.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
