"""Tests for native asset contribution metadata."""
from __future__ import annotations

from bacup_lib.record.extractors import (
    ASSET_CONTRIBUTIONS,
    signatures_for_asset_kind,
)


def test_asset_contribution_kinds_are_known() -> None:
    assert {entry.kind for entry in ASSET_CONTRIBUTIONS} == {
        "behavior",
        "creature_dir_scan",
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
    assert signatures_for_asset_kind("material") == frozenset({"MSWP"})
    assert signatures_for_asset_kind("behavior") == frozenset({"IDLE", "RACE"})
    assert signatures_for_asset_kind("sound") == frozenset(
        {"SNDR", "SOUN", "MUSC", "MUST"}
    )
