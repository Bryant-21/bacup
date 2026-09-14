"""Tests for native asset contribution metadata."""
from __future__ import annotations

from bacup_lib.record.extractors import (
    ASSET_CONTRIBUTIONS,
    FNV_KNOWN_DANGLING_CREATURE_KF_PATHS,
    is_known_dangling_asset_reference,
    signatures_for_asset_kind,
    source_prefixes_for_asset_kind,
)


def test_asset_contribution_kinds_are_known() -> None:
    assert {entry.kind for entry in ASSET_CONTRIBUTIONS} == {
        "behavior",
        "creature_dir_scan",
        "kf_animation",
        "material",
        "nif",
        "sound",
        "texture",
    }


def test_global_asset_kinds_do_not_filter_by_signature() -> None:
    assert signatures_for_asset_kind("nif") == frozenset()
    assert signatures_for_asset_kind("texture") == frozenset()


def test_signature_limited_asset_kinds_return_native_filters() -> None:
    assert signatures_for_asset_kind("creature_dir_scan") == frozenset({"CREA"})
    assert signatures_for_asset_kind("kf_animation") == frozenset({"IDLE"})
    assert signatures_for_asset_kind("material") == frozenset({"MSWP"})
    assert signatures_for_asset_kind("behavior") == frozenset({"IDLE", "RACE"})
    assert signatures_for_asset_kind("sound") == frozenset(
        {"SNDR", "SOUN", "MUSC", "MUST"}
    )


def test_legacy_kf_resolution_uses_meshes_source_prefix() -> None:
    assert source_prefixes_for_asset_kind("kf_animation") == ("Meshes",)
    assert source_prefixes_for_asset_kind("texture") == ()


def test_five_audited_fnv_dangling_kfs_are_typed_missing_references() -> None:
    assert len(FNV_KNOWN_DANGLING_CREATURE_KF_PATHS) == 5
    for path in FNV_KNOWN_DANGLING_CREATURE_KF_PATHS:
        assert is_known_dangling_asset_reference("fnv", "kf_animation", path)
        assert is_known_dangling_asset_reference(
            "FalloutNV",
            "KF_ANIMATION",
            f"Meshes\\{path}",
        )

    assert not is_known_dangling_asset_reference(
        "fnv",
        "kf_animation",
        "creatures/nvgecko/mtidle.kf",
    )
    assert not is_known_dangling_asset_reference(
        "fo3",
        "kf_animation",
        next(iter(FNV_KNOWN_DANGLING_CREATURE_KF_PATHS)),
    )
